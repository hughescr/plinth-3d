//! Native H0 shell creation, before drain placement.
//!
//! The later stage entry point owns drain and vent placement:
//!
//! `plinth_hollow::run(OrientedModel, &HollowRequest) -> Result<HollowStageOutput, HollowStageFailure>`.
//!
//! The failure type is not decoration: it carries the well-composedness
//! repair's measurement out beside the error, because the refusal record a
//! caller builds from it is the record that decides how the model is treated.
//!
//! H0 deliberately does not claim that entry point: a shell without drilled
//! drains must not become a `HollowedAndDrilled` production model. Instead
//! [`create_shell`] returns one closed shell mesh and the cavity evidence H1
//! needs to place drains. It keeps exactly one mesh; the source solid never
//! travels with the candidate.
#![forbid(unsafe_code)]

mod cavity_field;
pub mod drains;
pub mod drilling;
mod well_composed;

pub use cavity_field::{CavityField, CavityFieldRetainedAllocations};

use fast_surface_nets::ndshape::RuntimeShape;
use fast_surface_nets::{surface_nets, SurfaceNetsBuffer};
use plinth_core::{m0_constants, Finite, M0ConstantsError, PositiveFinite, PrinterProfile};
use plinth_mesh::{
    build_face_topology, certify_closed_components, triage_settings, validate_face_topology, Mesh,
    StrictMeshAuditError, TopologyError, TopologyValidationError, TriageAdmissionError,
    TriageError, TriageTopologyAdmission, TriageTopologyLimits, TriagedMesh,
    TOPOLOGY_CUT_CLOSED_SOLID_MASK, WeldPolicy,
};
use plinth_orient::OrientationHistory;
use plinth_solid::{
    BooleanInputEstimate, BooleanLimits, ManifoldBackend, Mesh as SolidMesh, SolidError,
};
use scirs2_core::ndarray::Array3;
use scirs2_ndimage::{distance_transform_edt, morphology3d::label3d};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hollowing policy: the printer profile, plus the one hollowing value the
/// operator selects per model.
///
/// Legacy exposed the shell wall as a command-line option with two values in
/// use — `HOLLOW_WALL_DEFAULT_MM` (2.5 mm) for ordinary pieces and
/// `HOLLOW_WALL_HANDLING_HEAVY_MM` (3.0 mm) for pieces that get handled. Both
/// are registered `Placeholder`s carrying no measurement, so neither is an
/// acceptance claim and nothing here classifies a model: the caller states the
/// wall, and [`CavityEvidence::wall_thickness_mm`] records what was stated —
/// in-process, on the returned model; no serialized artifact carries it yet
/// (FU-37). [`HollowRequest::with_registered_wall_default`] is the ordinary
/// construction.
///
/// The stated wall is not unbounded below: [`checked_wall_mm`] refuses anything
/// finer than the frozen voxel pitch, because the lattice cannot produce it.
#[derive(Clone, Debug, PartialEq)]
pub struct HollowRequest {
    pub printer: PrinterProfile,
    pub wall_mm: PositiveFinite,
}

impl HollowRequest {
    /// The request for a piece taking the registered default wall.
    pub fn with_registered_wall_default(printer: PrinterProfile) -> Result<Self, HollowError> {
        Ok(Self {
            printer,
            wall_mm: registered_wall(WallSelection::Default).map_err(HollowError::Constants)?,
        })
    }

    /// The request for a handling-heavy piece, at the registered heavier wall.
    pub fn with_registered_handling_heavy_wall(
        printer: PrinterProfile,
    ) -> Result<Self, HollowError> {
        Ok(Self {
            printer,
            wall_mm: registered_wall(WallSelection::HandlingHeavy)
                .map_err(HollowError::Constants)?,
        })
    }
}

/// Which registered wall value a caller is asking for. There is deliberately no
/// classifier that picks one: the two arms exist so a caller can name its
/// choice, and any other wall is supplied directly as a `PositiveFinite`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallSelection {
    Default,
    HandlingHeavy,
}

/// Reads one of the two registered wall values. This is the one place either
/// value is read, so the CLI's default and the stage's default cannot drift.
pub fn registered_wall(
    selection: WallSelection,
) -> Result<PositiveFinite, &'static M0ConstantsError> {
    let placeholders = m0_constants()?.placeholders();
    Ok(match selection {
        WallSelection::Default => *placeholders.hollow_wall_default_mm().diagnostic(),
        WallSelection::HandlingHeavy => *placeholders.hollow_wall_handling_heavy_mm().diagnostic(),
    })
}

/// The finest wall H0 can actually produce: the frozen `HOLLOW_PITCH_MM` voxel
/// pitch the cavity is selected on.
///
/// Cavity eligibility is `edt > wall` over a lattice quantised at that pitch,
/// so the smallest non-zero interior distance any voxel can carry is one pitch.
/// Every wall strictly below the pitch therefore selects the *identical*
/// cavity — 1e-9 mm and 0.4 mm produce byte-for-byte the same shell as 0.0 —
/// while [`CavityEvidence::wall_thickness_mm`] would record the distinct number
/// that was asked for. That is evidence asserting a wall that was never
/// produced, so the value is refused instead ([`checked_wall_mm`]).
pub fn minimum_representable_wall_mm() -> Result<PositiveFinite, &'static M0ConstantsError> {
    Ok(*m0_constants()?
        .placeholders()
        .hollow_pitch_mm()
        .diagnostic())
}

/// The one wall admissibility rule, shared by the stage and the CLI so they
/// cannot come to two different answers.
///
/// A wall finer than [`minimum_representable_wall_mm`] is refused by name
/// rather than quietly collapsed onto the lattice. A wall exactly at the pitch
/// is the finest one this stage can honestly claim, and is accepted.
pub fn checked_wall_mm(wall_mm: PositiveFinite) -> Result<PositiveFinite, HollowError> {
    let voxel_pitch = minimum_representable_wall_mm().map_err(HollowError::Constants)?;
    if wall_mm.get() < voxel_pitch.get() {
        return Err(HollowError::WallBelowVoxelPitch {
            wall_mm: wall_mm.get(),
            voxel_pitch_mm: voxel_pitch.get(),
        });
    }
    Ok(wall_mm)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidDecisionReason {
    NoCavity,
    BelowThreshold,
    /// The wall left a cavity and the well-composedness repair removed all of
    /// it. **Not `NoCavity`:** that reason is a statement about the model's
    /// shape, and a reader who cannot tell the two apart will attribute a
    /// repair's cost to the geometry. The size of the repair travels with the
    /// decision in [`HollowReport::cavity_repair`].
    NoCavityAfterRepair,
    /// The repaired cavity fell under the savings gate. Same distinction as
    /// above: the gate was judged *after* voxels were handed back to material,
    /// and the record has to say so.
    BelowThresholdAfterRepair,
}

/// What the well-composedness repair did, carried into the record so that a
/// repair-caused outcome is never reported as a property of the geometry.
///
/// This is the same measurement `H0ResourceDiagnostics` carries for a shell
/// that was built; it exists separately because the refusal paths build no
/// shell and would otherwise discard the evidence exactly where it decides how
/// the model was treated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CavityRepairEvidence {
    /// Critical 2×2×2 cells in the selected mask before any removal.
    pub critical_cells: usize,
    /// Cavity voxels turned back into material, stranded ones included.
    pub repaired_voxels: usize,
    /// 2×2×2 cells the repair examined.
    pub cell_visits: usize,
    /// Passes over the whole grid: the opening cell scan plus one per component
    /// re-selection. Counted separately because the cell-visit bound does not
    /// cover them. A re-selection pass is **two** whole-grid traversals — the
    /// `label3d` and the sweep applying its verdict — so `p` here is `2p − 1`
    /// traversals of the grid.
    pub full_grid_passes: usize,
}

/// Exact frozen A5 refusal codes authorized to consume the oriented-solid reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InternalSupportRefusalReason {
    MissingCavityAuthority,
    InvalidCavityAuthority,
    CavityFingerprintMismatch,
    DescriptorProvenanceMismatch,
    NoInnerCeilingDemand,
    NoContainedInternalRoute,
    UnroutableOrTrappedInternalLobe,
    A5GateFailure,
    InternalVerifierException,
    InvalidInternalVerifierReport,
    NoInternalMembers,
    A5InternalSupportFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HollowDisposition {
    HollowedAndDrilled,
    Solid {
        reason: SolidDecisionReason,
    },
    SolidFallback {
        reason: InternalSupportRefusalReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrainRecord {
    pub cavity_id: u32,
    pub entrance_mm: [Finite; 3],
    pub exit_mm: [Finite; 3],
    pub radius_mm: Finite,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CavityRecord {
    pub cavity_id: u32,
    pub volume_ml: Finite,
    pub drain_ids: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HollowReport {
    disposition: HollowDisposition,
    cavities: Vec<CavityRecord>,
    drains: Vec<DrainRecord>,
    drilling_evidence: Option<drilling::DrillingEvidence>,
    drain_plan: Option<drains::DrainPlan>,
    /// What the well-composedness repair did to the cavity this report is
    /// about, on every path that ran one — the hollowed path and the two
    /// repair-caused solid decisions alike. `None` means no repair result was
    /// recorded, which is the case for a report built by hand and for a model
    /// that never reached the cavity selection.
    ///
    /// `default` rather than required, because reports frozen before the repair
    /// existed have to keep deserializing.
    #[serde(default)]
    cavity_repair: Option<CavityRepairEvidence>,
}

impl HollowReport {
    #[must_use]
    pub fn solid_decision(reason: SolidDecisionReason) -> Self {
        Self {
            disposition: HollowDisposition::Solid { reason },
            cavities: Vec::new(),
            drains: Vec::new(),
            drilling_evidence: None,
            drain_plan: None,
            cavity_repair: None,
        }
    }

    /// The repair measurement behind this report, when one ran.
    #[must_use]
    pub const fn cavity_repair(&self) -> Option<CavityRepairEvidence> {
        self.cavity_repair
    }

    #[must_use]
    pub const fn disposition(&self) -> HollowDisposition {
        self.disposition
    }

    #[must_use]
    pub fn cavities(&self) -> &[CavityRecord] {
        &self.cavities
    }

    #[must_use]
    pub fn drains(&self) -> &[DrainRecord] {
        &self.drains
    }

    /// Complete original H1b proof, including planned and audit drains, vent,
    /// refill, opening, volume, bounds, drainability, and Boolean resources.
    #[must_use]
    pub const fn drilling_evidence(&self) -> Option<&drilling::DrillingEvidence> {
        self.drilling_evidence.as_ref()
    }

    #[must_use]
    pub const fn drain_plan(&self) -> Option<&drains::DrainPlan> {
        self.drain_plan.as_ref()
    }
}

/// Accumulated report state after hollowing; it contains no historical mesh.
#[derive(Clone, Debug, PartialEq)]
pub struct HollowHistory {
    orientation: OrientationHistory,
    hollow: HollowReport,
}

impl HollowHistory {
    /// Builds passive accumulated report state; this does not construct a hollowed model.
    #[must_use]
    pub fn from_reports(orientation: OrientationHistory, hollow: HollowReport) -> Self {
        Self {
            orientation,
            hollow,
        }
    }

    #[must_use]
    pub const fn orientation(&self) -> &OrientationHistory {
        &self.orientation
    }

    #[must_use]
    pub const fn hollow(&self) -> &HollowReport {
        &self.hollow
    }

    #[must_use]
    pub fn into_reports(self) -> (OrientationHistory, HollowReport) {
        (self.orientation, self.hollow)
    }
}

/// Move-only hollowed/drilled model or explicitly retained oriented solid.
#[derive(Debug)]
pub struct HollowedModel {
    mesh: Mesh,
    history: HollowHistory,
    cavity_authority: Option<CavityAuthority>,
}

#[derive(Debug)]
struct CavityAuthority {
    cavity: CavityEvidence,
    field: CavityField,
}

impl HollowedModel {
    pub(crate) fn from_verified_hollow(
        mesh: Mesh,
        history: HollowHistory,
        cavity: CavityEvidence,
        field: CavityField,
    ) -> Self {
        Self {
            mesh,
            history,
            cavity_authority: Some(CavityAuthority { cavity, field }),
        }
    }

    /// Explicit non-hollow support handoff. The caller must name why the
    /// oriented solid was retained; no shell candidate can use this path.
    fn solid(
        mesh: Mesh,
        orientation: OrientationHistory,
        disposition: HollowDisposition,
        cavity_repair: Option<CavityRepairEvidence>,
    ) -> Result<Self, HollowError> {
        validate_oriented_solid(&mesh)?;
        let report = HollowReport {
            disposition,
            cavities: Vec::new(),
            drains: Vec::new(),
            drilling_evidence: None,
            drain_plan: None,
            cavity_repair,
        };
        Ok(Self {
            mesh,
            history: HollowHistory::from_reports(orientation, report),
            cavity_authority: None,
        })
    }

    pub fn solid_decision(
        mesh: Mesh,
        orientation: OrientationHistory,
        reason: SolidDecisionReason,
    ) -> Result<Self, HollowError> {
        Self::solid(mesh, orientation, HollowDisposition::Solid { reason }, None)
    }

    /// A solid decision recorded together with whatever the well-composedness
    /// repair measured on the shell candidate that was abandoned.
    ///
    /// The evidence is the point: without it a caller reading the record can
    /// see *that* the model went solid but not whether a repair decided it, nor
    /// how much cavity the repair handed back. `None` is a claim in its own
    /// right — nothing reached cavity selection — and is why the parameter is
    /// an `Option` rather than two constructors.
    pub fn solid_decision_recording_repair(
        mesh: Mesh,
        orientation: OrientationHistory,
        reason: SolidDecisionReason,
        cavity_repair: Option<CavityRepairEvidence>,
    ) -> Result<Self, HollowError> {
        Self::solid(
            mesh,
            orientation,
            HollowDisposition::Solid { reason },
            cavity_repair,
        )
    }
    #[must_use]
    pub fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn history(&self) -> &HollowHistory {
        &self.history
    }

    #[must_use]
    pub const fn cavity_evidence(&self) -> Option<&CavityEvidence> {
        match &self.cavity_authority {
            Some(authority) => Some(&authority.cavity),
            None => None,
        }
    }

    #[must_use]
    pub const fn cavity_field(&self) -> Option<&CavityField> {
        match &self.cavity_authority {
            Some(authority) => Some(&authority.field),
            None => None,
        }
    }

    /// Moves the sole downstream mesh and report history into support planning.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, HollowHistory) {
        (self.mesh, self.history)
    }
}

/// A proved hollow paired with the exact oriented solid reserved for A5.
/// Neither geometry can be extracted independently: resolving A5 consumes the
/// pair exactly once and yields the sole support input.
#[derive(Debug)]
pub struct HollowedWithSolidReserve {
    hollowed: HollowedModel,
    reserve_mesh: Mesh,
    reserve_history: OrientationHistory,
}

impl HollowedWithSolidReserve {
    #[must_use]
    pub const fn hollowed(&self) -> &HollowedModel {
        &self.hollowed
    }

    #[must_use]
    pub const fn reserve_mesh(&self) -> &Mesh {
        &self.reserve_mesh
    }

    #[must_use]
    pub const fn reserve_history(&self) -> &OrientationHistory {
        &self.reserve_history
    }

    /// A5 approved the hollow and its internal members.
    #[must_use]
    pub fn approve_hollow(self) -> HollowedModel {
        self.hollowed
    }

    /// A5 refused the hollow with one exact frozen reason code.
    pub fn into_solid_fallback(
        self,
        reason: InternalSupportRefusalReason,
    ) -> Result<HollowedModel, HollowError> {
        // The shell the repair ran on is discarded here, and with it the only
        // other copy of the measurement. The reserve is a different mesh and
        // makes no claim about its own wall — but the record this substitution
        // produces is the one a reader uses to decide how this *model* was
        // treated, and "a repair ran and then the shell was refused" is not the
        // same finding as "no repair was ever attempted". So the shell's
        // measurement is carried onto the substitution rather than dropped with
        // the shell.
        let cavity_repair = self.hollowed.history().hollow().cavity_repair();
        HollowedModel::solid(
            self.reserve_mesh,
            self.reserve_history,
            HollowDisposition::SolidFallback { reason },
            cavity_repair,
        )
    }
}

/// A hollow-stage failure paired with whatever the well-composedness repair had
/// already measured when it happened.
///
/// **Why the error is not enough on its own.** Two of the repair's outcomes name
/// themselves ([`HollowError::CavityRepairRemovedCavity`],
/// [`HollowError::IneligibleSavingsAfterCavityRepair`]); every *other* failure
/// downstream of the repair — a refused shell topology, a drain plan, a drill,
/// an invalid production mesh — says nothing about it, and the record those
/// failures produce is exactly the record that decides how the model was
/// treated. A repair that removed cavity voxels being invisible there is the
/// failure mode this whole measurement exists to prevent, so the evidence
/// travels beside the error instead of dying with the candidate that was never
/// built.
///
/// `cavity_repair` is `None` when the failure happened *before* cavity
/// selection reached the repair — there is nothing to report, as opposed to
/// something that was dropped.
#[derive(Debug)]
pub struct HollowStageFailure {
    error: HollowError,
    cavity_repair: Option<CavityRepairEvidence>,
}

impl HollowStageFailure {
    /// A failure that happened before the repair could have run.
    #[must_use]
    pub const fn before_cavity_repair(error: HollowError) -> Self {
        Self {
            error,
            cavity_repair: None,
        }
    }

    /// A failure downstream of a repair, carrying what that repair did.
    #[must_use]
    pub const fn after_cavity_repair(
        error: HollowError,
        cavity_repair: CavityRepairEvidence,
    ) -> Self {
        Self {
            error,
            cavity_repair: Some(cavity_repair),
        }
    }

    #[must_use]
    pub const fn error(&self) -> &HollowError {
        &self.error
    }

    #[must_use]
    pub const fn cavity_repair(&self) -> Option<CavityRepairEvidence> {
        self.cavity_repair
    }

    #[must_use]
    pub fn into_error(self) -> HollowError {
        self.error
    }
}

impl From<HollowError> for HollowStageFailure {
    fn from(error: HollowError) -> Self {
        Self::before_cavity_repair(error)
    }
}

/// Verbatim the inner error's own text, so a refusal string recorded by a
/// caller does not change because the evidence gained a carrier.
impl std::fmt::Display for HollowStageFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for HollowStageFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Debug)]
pub enum HollowStageOutput {
    Solid(Box<HollowedModel>),
    AwaitingInternalSupport(Box<HollowedWithSolidReserve>),
}

impl HollowStageOutput {
    #[must_use]
    pub fn report(&self) -> &HollowReport {
        match self {
            Self::Solid(model) => model.history().hollow(),
            Self::AwaitingInternalSupport(candidate) => candidate.hollowed().history().hollow(),
        }
    }
}

/// Executes the complete production hollow transition.
///
/// The oriented solid is retained unchanged while a shell candidate moves
/// through drain planning and drilling. Only a proved drilled mesh may take
/// the hollow disposition. An ineligible or unprovable candidate is discarded
/// and the original oriented solid moves forward with an explicit fallback
/// reason, so an undrilled shell can never cross the support boundary.
pub fn run(
    model: plinth_orient::OrientedModel,
    request: &HollowRequest,
) -> Result<HollowStageOutput, HollowStageFailure> {
    let constants = m0_constants().map_err(HollowError::Constants)?;
    let registered = constants.saturn4_ultra();
    if &request.printer != registered {
        return Err(HollowError::PrinterProfileMismatch {
            expected: registered.id().as_str().to_owned(),
            actual: request.printer.id().as_str().to_owned(),
        }
        .into());
    }
    let settings = HollowSettings::from_request(request)?;
    let (solid, orientation) = model.into_parts();
    let candidate = match create_shell_recording_repair(
        solid.clone(),
        settings,
        H0AdmissionLimits::production(),
    ) {
        Ok(candidate) => candidate,
        Err(failure) => {
            // Read the measurement off the failure *before* the error is
            // consumed, so every arm below — the typed decisions and the bare
            // propagation alike — hands on the same evidence. This is the seam
            // the first version of this change lost it at: five errors became
            // typed outputs and everything else propagated naked, and the
            // record a naked propagation produces is the one that decides how
            // the model is treated.
            let cavity_repair = failure.cavity_repair();
            let decided = |reason| {
                HollowedModel::solid_decision_recording_repair(
                    solid,
                    orientation,
                    reason,
                    cavity_repair,
                )
                .map(|model| HollowStageOutput::Solid(Box::new(model)))
                .map_err(HollowStageFailure::from)
            };
            return match failure.into_error() {
                HollowError::IneligibleNoCavity => decided(SolidDecisionReason::NoCavity),
                HollowError::IneligibleBelowBulk | HollowError::IneligibleSavings => {
                    decided(SolidDecisionReason::BelowThreshold)
                }
                // The two repair-caused decisions. They travel as their own
                // reasons because "the repair removed the cavity" and "this
                // shape has no cavity" are different findings and the record
                // has to be able to tell them apart; the numbers themselves
                // come from the same `cavity_repair` every other arm uses.
                HollowError::CavityRepairRemovedCavity { .. } => {
                    decided(SolidDecisionReason::NoCavityAfterRepair)
                }
                HollowError::IneligibleSavingsAfterCavityRepair { .. } => {
                    decided(SolidDecisionReason::BelowThresholdAfterRepair)
                }
                error => Err(HollowStageFailure {
                    error,
                    cavity_repair,
                }),
            };
        }
    };
    // Drain planning and drilling consume the candidate, and with it the only
    // other copy of the repair's measurement. Take it first.
    let cavity_repair = candidate.cavity().resources().cavity_repair();
    let planned = drains::plan_drains(candidate).map_err(|failure| {
        HollowStageFailure::after_cavity_repair(
            HollowError::DrainPlanning(failure.into_reason()),
            cavity_repair,
        )
    })?;
    let drilled = drilling::drill_planned_shell(planned).map_err(|failure| {
        HollowStageFailure::after_cavity_repair(
            HollowError::Drilling(failure.into_reason()),
            cavity_repair,
        )
    })?;
    let reserve_history = orientation;
    let hollowed = drilled.into_hollowed_model(reserve_history.clone());
    Ok(HollowStageOutput::AwaitingInternalSupport(Box::new(
        HollowedWithSolidReserve {
            hollowed,
            reserve_mesh: solid,
            reserve_history,
        },
    )))
}

/// H0's explicit extraction settings. Every value originates in the one typed
/// M0 publication; they remain diagnostic `Placeholder`s, never an acceptance
/// claim. The pitch is the frozen voxel-oracle resolution used for the EDT,
/// component labels, and cavity eligibility; Manifold only subtracts that
/// selected voxel-surface cavity from the source solid.
///
/// The wall is the one field a request may state: [`HollowSettings::from_request`]
/// takes it from [`HollowRequest::wall_mm`], and everything else still comes
/// from the registry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HollowSettings {
    wall_thickness_mm: Finite,
    voxel_pitch_mm: Finite,
    bulk_min_mm: Finite,
    save_min_mm3: Finite,
    save_min_fraction: Finite,
    volume_balance_rel_tol: Finite,
}

impl HollowSettings {
    pub fn from_registered_constants() -> Result<Self, HollowError> {
        Self::with_wall_mm(registered_wall(WallSelection::Default).map_err(HollowError::Constants)?)
    }

    /// The production seam: the operator's stated wall, the registry for the rest.
    pub fn from_request(request: &HollowRequest) -> Result<Self, HollowError> {
        Self::with_wall_mm(request.wall_mm)
    }

    fn with_wall_mm(wall_mm: PositiveFinite) -> Result<Self, HollowError> {
        // A wall the frozen lattice cannot represent is refused here, before it
        // can reach the extraction and be recorded as evidence of a shell that
        // was never cut. See `checked_wall_mm`.
        let wall_mm = checked_wall_mm(wall_mm)?;
        let constants = m0_constants().map_err(HollowError::Constants)?;
        let placeholders = constants.placeholders();
        finite_settings(
            wall_mm.get(),
            placeholders.hollow_pitch_mm().diagnostic().get(),
            placeholders.hollow_bulk_min_mm().diagnostic().get(),
            placeholders.hollow_save_min_ml().diagnostic().get() * 1000.0,
            placeholders.hollow_save_min_frac().diagnostic().get(),
            placeholders
                .hollow_volume_balance_rel_tol()
                .diagnostic()
                .get(),
        )
    }

    #[must_use]
    pub const fn wall_thickness_mm(self) -> Finite {
        self.wall_thickness_mm
    }

    #[must_use]
    pub const fn voxel_pitch_mm(self) -> Finite {
        self.voxel_pitch_mm
    }

    #[must_use]
    pub const fn bulk_min_mm(self) -> Finite {
        self.bulk_min_mm
    }

    #[must_use]
    pub const fn save_min_mm3(self) -> Finite {
        self.save_min_mm3
    }

    #[must_use]
    pub const fn save_min_fraction(self) -> Finite {
        self.save_min_fraction
    }

    #[must_use]
    pub const fn volume_balance_rel_tol(self) -> Finite {
        self.volume_balance_rel_tol
    }
}

fn finite_settings(
    wall_thickness_mm: f64,
    voxel_pitch_mm: f64,
    bulk_min_mm: f64,
    save_min_mm3: f64,
    save_min_fraction: f64,
    volume_balance_rel_tol: f64,
) -> Result<HollowSettings, HollowError> {
    let finite = |value| Finite::new(value).map_err(|_| HollowError::InvalidArithmetic);
    Ok(HollowSettings {
        wall_thickness_mm: finite(wall_thickness_mm)?,
        voxel_pitch_mm: finite(voxel_pitch_mm)?,
        bulk_min_mm: finite(bulk_min_mm)?,
        save_min_mm3: finite(save_min_mm3)?,
        save_min_fraction: finite(save_min_fraction)?,
        volume_balance_rel_tol: finite(volume_balance_rel_tol)?,
    })
}

const DEFAULT_DRAIN_PLANNING_BYTES: usize = 1024 * 1024 * 1024;
const DEFAULT_QUERY_WORKING_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_RASTER_WORK_ITEMS: usize = 16 * 1024 * 1024;
/// The six raw-input ceilings, taken from the one place they are defined:
/// `plinth_mesh::TriageTopologyLimits::production()`, which the repair pre-pass
/// admits raw input with as well. Keeping a private second copy here would let
/// the two stages drift into disagreeing about how large an input is too large
/// to open.
const PRODUCTION_TRIAGE_LIMITS: TriageTopologyLimits = TriageTopologyLimits::production();
const DEFAULT_MAX_VOXEL_CELLS: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_PRIMITIVE_UNITS: u64 = 256 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_MARSHALLED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES: u64 = 384 * 1024 * 1024;
const QUERY_BYTES_PER_POINT: usize = std::mem::size_of::<[f64; 3]>()
    + std::mem::size_of::<bool>()
    + std::mem::size_of::<f64>()
    + std::mem::size_of::<i64>();

/// Caller-selected admission limits for named H0 work and public Boolean ABI
/// payloads. These are not a whole-process or third-party private-heap cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct H0AdmissionLimits {
    max_raw_vertices: usize,
    max_raw_faces: usize,
    max_retained_source_buffer_bytes: usize,
    max_edge_occurrences: usize,
    max_vertex_incidences: usize,
    max_containment_probe_slots: usize,
    max_voxel_cells: usize,
    query_working_bytes: usize,
    max_raster_work_items: usize,
    max_boolean_input_primitive_units: u64,
    max_boolean_marshaled_input_bytes: u64,
    max_boolean_output_vertices: u64,
    max_boolean_output_triangles: u64,
    drain_planning_bytes: usize,
}

impl H0AdmissionLimits {
    pub fn new(
        triage: TriageTopologyLimits,
        max_voxel_cells: usize,
        query_working_bytes: usize,
        max_raster_work_items: usize,
        boolean: BooleanLimits,
        drain_planning_bytes: usize,
    ) -> Result<Self, HollowError> {
        if triage.raw_vertices() == usize::MAX
            || triage.raw_faces() == usize::MAX
            || triage.retained_source_buffer_bytes() == usize::MAX
            || triage.edge_occurrences() == usize::MAX
            || triage.vertex_incidences() == usize::MAX
            || triage.containment_probe_slots() == usize::MAX
            || max_voxel_cells == 0
            || max_voxel_cells == usize::MAX
            || query_working_bytes < QUERY_BYTES_PER_POINT
            || query_working_bytes == usize::MAX
            || max_raster_work_items == 0
            || max_raster_work_items == usize::MAX
            || boolean.max_input_primitive_units == u64::MAX
            || boolean.max_logical_marshaled_input_bytes == u64::MAX
            || boolean.max_output_vertices == u64::MAX
            || boolean.max_output_triangles == u64::MAX
            || drain_planning_bytes == 0
            || drain_planning_bytes == usize::MAX
        {
            return Err(HollowError::InvalidAdmissionLimits);
        }
        Ok(Self {
            max_raw_vertices: triage.raw_vertices(),
            max_raw_faces: triage.raw_faces(),
            max_retained_source_buffer_bytes: triage.retained_source_buffer_bytes(),
            max_edge_occurrences: triage.edge_occurrences(),
            max_vertex_incidences: triage.vertex_incidences(),
            max_containment_probe_slots: triage.containment_probe_slots(),
            max_voxel_cells,
            query_working_bytes,
            max_raster_work_items,
            max_boolean_input_primitive_units: boolean.max_input_primitive_units,
            max_boolean_marshaled_input_bytes: boolean.max_logical_marshaled_input_bytes,
            max_boolean_output_vertices: boolean.max_output_vertices,
            max_boolean_output_triangles: boolean.max_output_triangles,
            drain_planning_bytes,
        })
    }

    #[must_use]
    pub const fn production() -> Self {
        Self {
            max_raw_vertices: PRODUCTION_TRIAGE_LIMITS.raw_vertices(),
            max_raw_faces: PRODUCTION_TRIAGE_LIMITS.raw_faces(),
            max_retained_source_buffer_bytes: PRODUCTION_TRIAGE_LIMITS
                .retained_source_buffer_bytes(),
            max_edge_occurrences: PRODUCTION_TRIAGE_LIMITS.edge_occurrences(),
            max_vertex_incidences: PRODUCTION_TRIAGE_LIMITS.vertex_incidences(),
            max_containment_probe_slots: PRODUCTION_TRIAGE_LIMITS.containment_probe_slots(),
            max_voxel_cells: DEFAULT_MAX_VOXEL_CELLS,
            query_working_bytes: DEFAULT_QUERY_WORKING_BYTES,
            max_raster_work_items: DEFAULT_MAX_RASTER_WORK_ITEMS,
            max_boolean_input_primitive_units: DEFAULT_MAX_BOOLEAN_PRIMITIVE_UNITS,
            max_boolean_marshaled_input_bytes: DEFAULT_MAX_BOOLEAN_MARSHALLED_BYTES,
            max_boolean_output_vertices: DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES,
            max_boolean_output_triangles: DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES,
            drain_planning_bytes: DEFAULT_DRAIN_PLANNING_BYTES,
        }
    }

    #[must_use]
    pub const fn max_voxel_cells(self) -> usize {
        self.max_voxel_cells
    }

    #[must_use]
    pub const fn query_working_bytes(self) -> usize {
        self.query_working_bytes
    }

    #[must_use]
    pub const fn max_raster_work_items(self) -> usize {
        self.max_raster_work_items
    }

    #[must_use]
    pub const fn drain_planning_bytes(self) -> usize {
        self.drain_planning_bytes
    }

    fn triage_topology_limits(self, retained_source_buffer_bytes: usize) -> TriageTopologyLimits {
        TriageTopologyLimits::new(
            self.max_raw_vertices,
            self.max_raw_faces,
            retained_source_buffer_bytes.min(self.max_retained_source_buffer_bytes),
            self.max_edge_occurrences,
            self.max_vertex_incidences,
            self.max_containment_probe_slots,
        )
    }
}

/// Scoped H0 diagnostics for admitted counts and owned logical buffers.
/// This intentionally makes no claim about allocator slack, Manifold private
/// heap, dependency scratch, or whole-process peak memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct H0ResourceDiagnostics {
    voxel_count: usize,
    query_batch_points: usize,
    grid_coordinate_bytes: usize,
    retained_raw_source_buffer_bytes: usize,
    raster_work_items: usize,
    retained_cavity_membership_bytes: usize,
    retained_source_occupancy_bytes: usize,
    marching_vertex_upper_bound: usize,
    marching_triangle_upper_bound: usize,
    boolean_input_primitive_units: u64,
    boolean_marshaled_input_bytes: u64,
    boolean_output_vertex_limit: u64,
    boolean_output_triangle_limit: u64,
    cavity_repair: CavityRepairEvidence,
}

impl H0ResourceDiagnostics {
    #[must_use]
    pub const fn voxel_count(self) -> usize {
        self.voxel_count
    }
    #[must_use]
    pub const fn query_batch_points(self) -> usize {
        self.query_batch_points
    }
    #[must_use]
    pub const fn grid_coordinate_bytes(self) -> usize {
        self.grid_coordinate_bytes
    }
    #[must_use]
    pub const fn occupancy_bytes(self) -> usize {
        self.voxel_count * std::mem::size_of::<bool>()
    }
    #[must_use]
    pub const fn retained_raw_source_buffer_bytes(self) -> usize {
        self.retained_raw_source_buffer_bytes
    }
    #[must_use]
    pub const fn raster_work_items(self) -> usize {
        self.raster_work_items
    }
    #[must_use]
    pub const fn retained_cavity_membership_bytes(self) -> usize {
        self.retained_cavity_membership_bytes
    }
    #[must_use]
    pub const fn retained_source_occupancy_bytes(self) -> usize {
        self.retained_source_occupancy_bytes
    }
    #[must_use]
    pub const fn marching_vertex_upper_bound(self) -> usize {
        self.marching_vertex_upper_bound
    }
    #[must_use]
    pub const fn marching_triangle_upper_bound(self) -> usize {
        self.marching_triangle_upper_bound
    }
    #[must_use]
    pub const fn boolean_input_primitive_units(self) -> u64 {
        self.boolean_input_primitive_units
    }
    #[must_use]
    pub const fn boolean_marshaled_input_bytes(self) -> u64 {
        self.boolean_marshaled_input_bytes
    }
    #[must_use]
    pub const fn boolean_output_vertex_limit(self) -> u64 {
        self.boolean_output_vertex_limit
    }
    #[must_use]
    pub const fn boolean_output_triangle_limit(self) -> u64 {
        self.boolean_output_triangle_limit
    }
    /// What the well-composedness repair did to this shell's cavity. The same
    /// measurement travels on the repair's two refusal paths, so a repair is
    /// visible in the record whether or not it produced a shell.
    #[must_use]
    pub const fn cavity_repair(self) -> CavityRepairEvidence {
        self.cavity_repair
    }
    /// Critical 2×2×2 cells in the selected cavity mask before repair — the
    /// non-well-composed configurations that would have forced a non-manifold
    /// cavity surface out of a one-vertex-per-cell extractor. Zero on a mask
    /// that needed nothing.
    #[must_use]
    pub const fn cavity_critical_cells(self) -> usize {
        self.cavity_repair.critical_cells
    }
    /// Cavity voxels turned back into material to reach well-composedness,
    /// including any left stranded off the selected component. The repair only
    /// ever removes cavity, so this is an upper bound on the saving it cost.
    #[must_use]
    pub const fn cavity_repaired_voxels(self) -> usize {
        self.cavity_repair.repaired_voxels
    }
    /// 2×2×2 cells the repair examined. Bounded by one full scan of the grid
    /// the caller already admitted, plus the eight cells around each voxel it
    /// changed.
    #[must_use]
    pub const fn cavity_repair_cell_visits(self) -> usize {
        self.cavity_repair.cell_visits
    }
    /// Passes the repair made over the *whole* grid: the opening cell scan plus
    /// one `label3d` re-selection per round that removed anything. The
    /// cell-visit count above does not cover these, which is why they are
    /// counted separately rather than folded into it. Each re-selection pass is
    /// **two** whole-grid traversals (the `label3d` and the sweep that applies
    /// it), so `p` passes are `2p − 1` traversals; and the pass count has no
    /// admitted bound of its own, which is FU-54.
    #[must_use]
    pub const fn cavity_repair_full_grid_passes(self) -> usize {
        self.cavity_repair.full_grid_passes
    }
}

/// Evidence that describes the sole inner cavity retained by H0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CavityEvidence {
    wall_thickness_mm: Finite,
    frozen_voxel_pitch_mm: Finite,
    cavity_volume_mm3: Finite,
    source_volume_mm3: Finite,
    resources: H0ResourceDiagnostics,
    drain_planning_cap_bytes: usize,
}

impl CavityEvidence {
    pub(crate) fn with_cavity_volume_mm3(mut self, cavity_volume_mm3: Finite) -> Self {
        self.cavity_volume_mm3 = cavity_volume_mm3;
        self
    }
    #[must_use]
    pub const fn wall_thickness_mm(self) -> Finite {
        self.wall_thickness_mm
    }

    #[must_use]
    pub const fn frozen_voxel_pitch_mm(self) -> Finite {
        self.frozen_voxel_pitch_mm
    }

    #[must_use]
    pub const fn cavity_volume_mm3(self) -> Finite {
        self.cavity_volume_mm3
    }

    #[must_use]
    pub const fn source_volume_mm3(self) -> Finite {
        self.source_volume_mm3
    }

    #[must_use]
    pub const fn resources(self) -> H0ResourceDiagnostics {
        self.resources
    }

    #[must_use]
    pub const fn drain_planning_cap_bytes(self) -> usize {
        self.drain_planning_cap_bytes
    }
}

/// The H0 hand-off to H1. It owns only the closed shell mesh and its one
/// retained cavity; it cannot be mistaken for drilled, printable output.
#[derive(Debug)]
pub struct ShellCandidate {
    mesh: Mesh,
    cavity: CavityEvidence,
    cavity_field: CavityField,
}

impl ShellCandidate {
    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub const fn cavity(&self) -> &CavityEvidence {
        &self.cavity
    }

    /// Opaque H0-selected voxel membership retained for H1 planning. It is
    /// not reconstructed from the shell and admits no raw mutable buffer.
    #[must_use]
    pub const fn cavity_field(&self) -> &CavityField {
        &self.cavity_field
    }

    #[must_use]
    pub fn into_parts(self) -> (Mesh, CavityEvidence, CavityField) {
        (self.mesh, self.cavity, self.cavity_field)
    }
}

/// Convert a triaged, strict oriented solid into a single shell mesh under the
/// caller's extraction settings. H0 does not add drains and therefore does not
/// expose `run`.
///
/// Settings are an argument rather than a registry read inside this function:
/// [`run`] builds them from the operator's [`HollowRequest`], and callers with
/// no request of their own build them with
/// [`HollowSettings::from_registered_constants`]. There is no separate
/// test-only settings entry point — this is it.
pub fn create_shell(input: Mesh, settings: HollowSettings) -> Result<ShellCandidate, HollowError> {
    create_shell_with_admission_limits(input, settings, H0AdmissionLimits::production())
}

/// The registry-default settings, for the tests in this crate that exercise
/// H0 itself rather than an operator's wall selection.
#[cfg(test)]
pub(crate) fn registered_settings() -> HollowSettings {
    HollowSettings::from_registered_constants().expect("M0 settings")
}

/// H0 with explicit caller-owned admission limits for each named work domain.
///
/// The repair's measurement is dropped on the failure paths here; callers that
/// have to *record* a refusal use [`create_shell_recording_repair`] instead.
pub fn create_shell_with_admission_limits(
    input: Mesh,
    settings: HollowSettings,
    limits: H0AdmissionLimits,
) -> Result<ShellCandidate, HollowError> {
    create_shell_recording_repair(input, settings, limits).map_err(HollowStageFailure::into_error)
}

/// H0, with every failure downstream of the well-composedness repair carrying
/// what that repair did.
///
/// Same computation as [`create_shell_with_admission_limits`], different
/// failure type. It exists because a refusal record built from a bare
/// [`HollowError`] cannot say whether a repair removed cavity voxels first, and
/// that record is what decides how the model is treated.
pub fn create_shell_recording_repair(
    input: Mesh,
    settings: HollowSettings,
    limits: H0AdmissionLimits,
) -> Result<ShellCandidate, HollowStageFailure> {
    let retained_raw_source_buffer_bytes = input
        .retained_buffer_bytes()
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    let admission = TriageTopologyAdmission::from_raw_mesh(
        &input,
        limits.triage_topology_limits(retained_raw_source_buffer_bytes),
    )
    .map_err(HollowError::from)?;
    let retained_raw_source_buffer_bytes = admission.retained_source_buffer_bytes();
    let triaged = admission
        .triage(WeldPolicy::ExactBits)
        .map_err(HollowError::from)?;
    let source = triaged.into_mesh();
    let source_volume_mm3 = validate_oriented_solid(&source)?;
    let spec = grid_spec(&source, settings.voxel_pitch_mm.get())?;
    if spec.count > limits.max_voxel_cells {
        return Err(HollowError::VoxelCellLimitExceeded {
            cells: spec.count,
            max_cells: limits.max_voxel_cells,
        }
        .into());
    }
    let raster_work_items = raster_work_item_count(&source, spec, limits.max_raster_work_items)?;
    let grid = voxelize_filled(&source, spec, raster_work_items)?;
    let VoxelGrid {
        origin: grid_origin,
        pitch: grid_pitch,
        material,
    } = grid;
    let edt = frozen_distance_transform(&material, grid_pitch)?;
    let bulk_mm = 2.0 * edt.iter().copied().fold(0.0_f64, f64::max);
    if !bulk_mm.is_finite() {
        return Err(HollowError::InvalidArithmetic.into());
    }
    if bulk_mm < settings.bulk_min_mm.get() {
        return Err(HollowError::IneligibleBelowBulk.into());
    }
    let cavity_mask = edt.mapv(|distance_mm| distance_mm > settings.wall_thickness_mm.get());
    drop(edt);
    let (labels, label_count) =
        label3d(cavity_mask.view(), 6).map_err(|_| HollowError::VoxelStack)?;
    drop(cavity_mask);
    if label_count == 0 {
        return Err(HollowError::IneligibleNoCavity.into());
    }
    let largest = largest_label(&labels, label_count)?;
    let selected = labels.mapv(|label| label == largest);
    // The extractor emits one vertex per lattice cell, which is only enough
    // where the mask is well-composed. Repair it here, before anything measures
    // or marches it, so the cavity volume the savings gate weighs is the cavity
    // that is actually cut. The repair only ever removes cavity voxels, so the
    // wall can only get thicker (`well_composed`).
    let (main, well_composed_repair) = match well_composed::well_composed_cavity_component(selected)
    {
        Ok(repaired) => repaired,
        // The fixpoint consumed the component. The error names the measurement
        // itself, but lift it onto the failure as well so that a caller reading
        // `cavity_repair` never has to special-case one variant to see it.
        Err(HollowError::CavityRepairRemovedCavity {
            critical_cells,
            repaired_voxels,
            cell_visits,
            full_grid_passes,
        }) => {
            let evidence = CavityRepairEvidence {
                critical_cells,
                repaired_voxels,
                cell_visits,
                full_grid_passes,
            };
            return Err(HollowStageFailure::after_cavity_repair(
                HollowError::CavityRepairRemovedCavity {
                    critical_cells,
                    repaired_voxels,
                    cell_visits,
                    full_grid_passes,
                },
                evidence,
            ));
        }
        Err(error) => return Err(error.into()),
    };
    let repair_evidence = CavityRepairEvidence {
        critical_cells: well_composed_repair.critical_cells(),
        repaired_voxels: well_composed_repair.removed_voxels()
            + well_composed_repair.reselection_dropped_voxels(),
        cell_visits: well_composed_repair.cell_visits(),
        full_grid_passes: well_composed_repair.full_grid_passes(),
    };
    // **The evidence seam.** Everything below runs on a mask the repair may
    // already have changed, so every failure from here on is a failure *after*
    // a repair and carries its measurement. Written as one fallible block whose
    // single exit attaches the evidence, rather than as a `map_err` per call
    // site, because the defect this fixes was one uncovered call site.
    let sealed = (move || -> Result<ShellCandidate, HollowError> {
        let cavity_voxels = main.iter().filter(|value| **value).count();
        let cavity_volume_mm3 = (cavity_voxels as f64) * grid_pitch.powi(3);
        if !cavity_volume_mm3.is_finite() {
            return Err(HollowError::InvalidArithmetic);
        }
        let required_fraction_mm3 = source_volume_mm3 * settings.save_min_fraction.get();
        if !required_fraction_mm3.is_finite() {
            return Err(HollowError::InvalidArithmetic);
        }
        if cavity_volume_mm3 < settings.save_min_mm3.get()
            && cavity_volume_mm3 < required_fraction_mm3
        {
            // A cavity the repair shrank and a cavity that was always this
            // small are the same number here and different findings. Say which
            // one this was in the error's own name, too, so a `create_shell`
            // caller holding only a `HollowError` can still tell them apart.
            if repair_evidence.repaired_voxels > 0 {
                return Err(HollowError::IneligibleSavingsAfterCavityRepair {
                    critical_cells: repair_evidence.critical_cells,
                    repaired_voxels: repair_evidence.repaired_voxels,
                    cell_visits: repair_evidence.cell_visits,
                    full_grid_passes: repair_evidence.full_grid_passes,
                });
            }
            return Err(HollowError::IneligibleSavings);
        }

        let cavity_field = CavityField::from_selected_mask_and_occupancy(
            &main,
            &material,
            grid_origin,
            grid_pitch,
        )?;
        drop(material);
        let inner = marching_cavity(&main, grid_origin, grid_pitch)?;
        drop(main);
        let (marching_vertex_upper_bound, marching_triangle_upper_bound) =
            marching_surface_bounds(spec)?;
        let boolean_limits = boolean_limits_for_h0(
            &source,
            marching_vertex_upper_bound,
            marching_triangle_upper_bound,
            limits,
        )?;
        let operands = [to_solid(&source)?, to_solid(&inner)?];
        let boolean_input =
            BooleanInputEstimate::for_meshes(&operands).map_err(HollowError::Solid)?;
        let shell = ManifoldBackend
            .difference_limited(&operands, boolean_limits)
            .map_err(HollowError::Solid)?;
        let mesh = orient_boolean_cavity(from_solid(shell)?)?;
        validate_shell_topology(
            &mesh,
            source_volume_mm3,
            cavity_volume_mm3,
            settings.volume_balance_rel_tol.get(),
        )?;
        validate_printable_bounds(&mesh)?;
        let retained_field_allocations = cavity_field.retained_allocations();
        let query_batch_points = spec
            .count
            .min(limits.query_working_bytes / QUERY_BYTES_PER_POINT)
            .max(1);
        let resources = H0ResourceDiagnostics {
            voxel_count: spec.count,
            query_batch_points,
            grid_coordinate_bytes: checked_scale(
                query_batch_points,
                std::mem::size_of::<[f64; 3]>(),
            )?,
            retained_raw_source_buffer_bytes,
            raster_work_items,
            retained_cavity_membership_bytes: retained_field_allocations.membership_bytes(),
            retained_source_occupancy_bytes: retained_field_allocations.source_occupancy_bytes(),
            marching_vertex_upper_bound,
            marching_triangle_upper_bound,
            boolean_input_primitive_units: boolean_input.operand_primitive_units,
            boolean_marshaled_input_bytes: boolean_input.logical_marshaled_input_bytes,
            boolean_output_vertex_limit: boolean_limits.max_output_vertices,
            boolean_output_triangle_limit: boolean_limits.max_output_triangles,
            cavity_repair: repair_evidence,
        };
        let cavity = CavityEvidence {
            wall_thickness_mm: settings.wall_thickness_mm,
            frozen_voxel_pitch_mm: settings.voxel_pitch_mm,
            cavity_volume_mm3: finite(cavity_volume_mm3)?,
            source_volume_mm3: finite(source_volume_mm3)?,
            resources,
            drain_planning_cap_bytes: limits.drain_planning_bytes,
        };
        Ok(ShellCandidate {
            mesh,
            cavity,
            cavity_field,
        })
    })();
    sealed.map_err(|error| HollowStageFailure::after_cavity_repair(error, repair_evidence))
}

const PRINTABLE_MAX_EXTENT_MM: f64 = 300.0;

/// **The closed-solid gate**, exactly as every hollow-stage caller runs it.
///
/// Made public by the D-22 revision of 2026-08-10 and **not otherwise
/// touched** — same three checks, same order, same error values. The repair
/// pre-pass lands strictly upstream of this function and earns no exemption
/// from it, so the diagnostic that measures whether repair rescued a model
/// (`plinth-pipeline`'s `repair_probe` example) has to consult the production
/// gate rather than a paraphrase of it. A second copy of these three lines
/// living in a diagnostic is exactly how a diagnostic starts reporting that
/// models pass a gate they do not pass.
///
/// Per the owner ruling of 2026-08-12 (FU-47, §4 of
/// `docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`):
/// a point-touch (`TOPOLOGY_CUT_POINT_ONLY`) is not a closed-solid defect, so
/// this gate refuses on [`TOPOLOGY_CUT_CLOSED_SOLID_MASK`] rather than on any
/// nonzero cut flag. Closure and orientation are not relaxed by that mask,
/// and no threshold moves.
///
/// # Errors
///
/// [`HollowError::StrictAudit`] for a repeated index or a zero-area face,
/// [`HollowError::Topology`] / [`HollowError::TopologyValidation`] when the
/// topology cannot be built or does not validate, and
/// [`HollowError::NotClosedOrOriented`] when any face carries a cut flag in
/// [`TOPOLOGY_CUT_CLOSED_SOLID_MASK`].
pub fn validate_closed_mesh(mesh: &Mesh) -> Result<(), HollowError> {
    mesh.audit_strict().map_err(HollowError::StrictAudit)?;
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    validate_face_topology(&topology).map_err(HollowError::TopologyValidation)?;
    if topology
        .cut_flags()
        .iter()
        .any(|flags| *flags & TOPOLOGY_CUT_CLOSED_SOLID_MASK != 0)
    {
        return Err(HollowError::NotClosedOrOriented);
    }
    Ok(())
}

fn validate_oriented_solid(mesh: &Mesh) -> Result<f64, HollowError> {
    validate_closed_mesh(mesh)?;
    validate_printable_bounds(mesh)?;
    source_volume_mm3(mesh)
}

fn triage_exact_with_measured_limits(mesh: &Mesh) -> Result<TriagedMesh, HollowError> {
    let settings = triage_settings()?;
    let faces = mesh.tris().len();
    let edge_occurrences = checked_scale(faces, 3)?;
    let containment_probe_slots = checked_scale(faces, settings.containment_samples())?;
    let retained_source_buffer_bytes = mesh
        .retained_buffer_bytes()
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    let admission = TriageTopologyAdmission::from_raw_mesh(
        mesh,
        TriageTopologyLimits::new(
            mesh.verts().len(),
            faces,
            retained_source_buffer_bytes,
            edge_occurrences,
            edge_occurrences,
            containment_probe_slots,
        ),
    )?;
    admission
        .triage(WeldPolicy::ExactBits)
        .map_err(HollowError::Triage)
}

fn validate_shell_topology(
    mesh: &Mesh,
    source_volume_mm3: f64,
    cavity_volume_mm3: f64,
    relative_tolerance: f64,
) -> Result<(), HollowError> {
    match triage_exact_with_measured_limits(mesh) {
        Err(HollowError::Triage(TriageError::PreHollowed(refusal)))
            if refusal.cavity_components().len() == 1 => {}
        _ => return Err(HollowError::InvalidShellTopology),
    }
    validate_closed_mesh(mesh)?;
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    let certificate = certify_closed_components(mesh, &topology).map_err(HollowError::Topology)?;
    let negative = certificate
        .components()
        .iter()
        .filter(|component| component.signed_volume_mm3() < 0.0)
        .count();
    if negative != 1
        || certificate.components().len() < 2
        || certificate
            .components()
            .iter()
            .any(|component| component.signed_volume_mm3() == 0.0)
    {
        return Err(HollowError::InvalidShellTopology);
    }
    let actual_material_mm3 = certificate.signed_volume_mm3();
    let expected_material_mm3 = source_volume_mm3 - cavity_volume_mm3;
    let tolerance_mm3 = source_volume_mm3 * relative_tolerance;
    if !actual_material_mm3.is_finite()
        || !expected_material_mm3.is_finite()
        || !tolerance_mm3.is_finite()
        || (actual_material_mm3 - expected_material_mm3).abs() > tolerance_mm3
    {
        return Err(HollowError::BooleanVolumeMismatch {
            actual_material_mm3,
            expected_material_mm3,
            tolerance_mm3,
        });
    }
    Ok(())
}

pub(crate) fn orient_boolean_cavity(mesh: Mesh) -> Result<Mesh, HollowError> {
    let cavity_component = match triage_exact_with_measured_limits(&mesh) {
        Err(HollowError::Triage(TriageError::PreHollowed(refusal)))
            if refusal.cavity_components().len() == 1 =>
        {
            u32::try_from(refusal.cavity_components()[0].component_id())
                .map_err(|_| HollowError::InvalidShellTopology)?
        }
        _ => return Err(HollowError::InvalidShellTopology),
    };
    let topology =
        build_face_topology(&mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    validate_face_topology(&topology).map_err(HollowError::TopologyValidation)?;
    let triangles = mesh
        .tris()
        .iter()
        .zip(topology.component_ids())
        .map(|(triangle, component)| {
            if *component == cavity_component {
                [triangle[0], triangle[2], triangle[1]]
            } else {
                *triangle
            }
        })
        .collect();
    Mesh::new(mesh.verts().to_vec(), triangles).map_err(HollowError::OutputMesh)
}

fn source_volume_mm3(mesh: &Mesh) -> Result<f64, HollowError> {
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    let certificate = certify_closed_components(mesh, &topology).map_err(HollowError::Topology)?;
    if certificate
        .components()
        .iter()
        .all(|component| component.signed_volume_mm3() > 0.0)
    {
        let volume = certificate.signed_volume_mm3();
        if volume.is_finite() && volume > 0.0 {
            return Ok(volume);
        }
    }
    Err(HollowError::NotClosedOrOriented)
}

struct VoxelGrid {
    origin: [f64; 3],
    pitch: f64,
    material: Array3<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GridSpec {
    origin: [f64; 3],
    dims: [usize; 3],
    pitch: f64,
    count: usize,
}

fn grid_spec(mesh: &Mesh, pitch: f64) -> Result<GridSpec, HollowError> {
    if !pitch.is_finite() || pitch <= 0.0 {
        return Err(HollowError::InvalidArithmetic);
    }
    let (low, high) = bounds(mesh)?;
    grid_spec_from_bounds(low, high, pitch)
}

fn grid_spec_from_bounds(
    low: [f64; 3],
    high: [f64; 3],
    pitch: f64,
) -> Result<GridSpec, HollowError> {
    if !pitch.is_finite() || pitch <= 0.0 {
        return Err(HollowError::InvalidArithmetic);
    }
    let mut origin = [0.0; 3];
    let mut dims = [0_usize; 3];
    for axis in 0..3 {
        let low_index = low[axis] / pitch;
        let high_index = high[axis] / pitch;
        if !low_index.is_finite() || !high_index.is_finite() {
            return Err(HollowError::VoxelGridTooLarge);
        }
        let origin_index = round_ties_even(low_index);
        let end_index = round_ties_even(high_index);
        let span_index = end_index - origin_index;
        origin[axis] = origin_index * pitch;
        if !origin[axis].is_finite()
            || !end_index.is_finite()
            || !span_index.is_finite()
            || span_index < 0.0
        {
            return Err(HollowError::InvalidArithmetic);
        }
        if span_index > (u32::MAX - 1) as f64 {
            return Err(HollowError::VoxelGridTooLarge);
        }
        let axis_samples = span_index as usize;
        dims[axis] = axis_samples
            .checked_add(1)
            .ok_or(HollowError::VoxelGridTooLarge)?;
        if dims[axis] < 3 {
            return Err(HollowError::InvalidArithmetic);
        }
    }
    let count = dims[0]
        .checked_mul(dims[1])
        .and_then(|value| value.checked_mul(dims[2]))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    Ok(GridSpec {
        origin,
        dims,
        pitch,
        count,
    })
}

fn checked_scale(count: usize, bytes: usize) -> Result<usize, HollowError> {
    count
        .checked_mul(bytes)
        .ok_or(HollowError::ResourceAccountingOverflow)
}

fn allocator_allocation_bound(
    count: usize,
    element_size: usize,
    element_align: usize,
) -> Result<usize, HollowError> {
    let requested = checked_scale(count, element_size)?;
    allocator_allocation_bound_bytes(requested, element_align)
}

fn allocator_allocation_bound_bytes(
    requested: usize,
    element_align: usize,
) -> Result<usize, HollowError> {
    let allocation_align = element_align.max(2 * std::mem::size_of::<usize>());
    let rounded = requested
        .checked_add(allocation_align - 1)
        .map(|value| value / allocation_align * allocation_align)
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    // These exact-size Vec allocations do not intentionally reserve spare
    // capacity. Doubling the aligned request plus two words conservatively
    // covers allocator size classes, alignment padding, and block metadata.
    rounded
        .checked_mul(2)
        .and_then(|value| value.checked_add(2 * std::mem::size_of::<usize>()))
        .ok_or(HollowError::ResourceAccountingOverflow)
}

fn marching_surface_bounds(spec: GridSpec) -> Result<(usize, usize), HollowError> {
    // Surface Nets can emit at most one vertex per padded lattice cell and
    // six triangles per emitted vertex. These are logical output-count bounds,
    // not estimates of the library's private scratch or allocator behavior.
    let vertices = spec.dims.iter().try_fold(1_usize, |total, dimension| {
        dimension
            .checked_add(2)
            .and_then(|padded| total.checked_mul(padded))
            .ok_or(HollowError::ResourceAccountingOverflow)
    })?;
    let triangles = checked_scale(vertices, 6)?;
    Ok((vertices, triangles))
}

fn boolean_limits_for_h0(
    source: &Mesh,
    marching_vertex_upper_bound: usize,
    marching_triangle_upper_bound: usize,
    admission: H0AdmissionLimits,
) -> Result<BooleanLimits, HollowError> {
    let combined_vertices = source
        .verts()
        .len()
        .checked_add(marching_vertex_upper_bound)
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    let combined_triangles = source
        .tris()
        .len()
        .checked_add(marching_triangle_upper_bound)
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    let vertices =
        u64::try_from(combined_vertices).map_err(|_| HollowError::ResourceAccountingOverflow)?;
    let triangles =
        u64::try_from(combined_triangles).map_err(|_| HollowError::ResourceAccountingOverflow)?;
    let primitive_units = vertices
        .checked_mul(3)
        .and_then(|value| value.checked_add(triangles.checked_mul(3)?))
        .and_then(|value| value.checked_add(6))
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    let marshalled_bytes = primitive_units
        .checked_mul(8)
        .ok_or(HollowError::ResourceAccountingOverflow)?;
    Ok(BooleanLimits {
        max_input_primitive_units: primitive_units.min(admission.max_boolean_input_primitive_units),
        max_logical_marshaled_input_bytes: marshalled_bytes
            .min(admission.max_boolean_marshaled_input_bytes),
        max_output_vertices: vertices.min(admission.max_boolean_output_vertices),
        max_output_triangles: triangles.min(admission.max_boolean_output_triangles),
    })
}

const MAX_RASTER_SUBDIVISION_ROUNDS: u8 = 10;
const RASTER_STACK_CAPACITY: usize = 1 + 3 * MAX_RASTER_SUBDIVISION_ROUNDS as usize;

#[derive(Clone, Copy)]
struct RasterNode {
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    depth: u8,
}

fn raster_work_item_count(
    mesh: &Mesh,
    spec: GridSpec,
    max_work_items: usize,
) -> Result<usize, HollowError> {
    let mut count = 0_usize;
    walk_raster_leaves(mesh, spec, |_, _, _| {
        count = count
            .checked_add(1)
            .ok_or(HollowError::ResourceAccountingOverflow)?;
        if count > max_work_items {
            return Err(HollowError::RasterWorkLimitExceeded {
                work_items: count,
                max_work_items,
            });
        }
        Ok(())
    })?;
    Ok(count)
}

fn voxelize_filled(
    mesh: &Mesh,
    spec: GridSpec,
    expected_work_items: usize,
) -> Result<VoxelGrid, HollowError> {
    // Frozen source: `trimesh.voxelized(pitch).fill()`. Its default
    // `subdivide` voxelizer recursively quarters each triangle until every
    // edge is at most pitch/2, rounds each resulting vertex to a grid point,
    // then `binary_fill_holes` fills the 6-connected exterior complement.
    // Point-in-mesh and closest-surface tests are a different voxelizer: their
    // boundary bands are not the source field and can put an H1 candidate
    // outside the Boolean cavity.
    let mut occupancy = vec![false; spec.count];
    let mut observed_work_items = 0_usize;
    walk_raster_leaves(mesh, spec, |a, b, c| {
        observed_work_items = observed_work_items
            .checked_add(1)
            .ok_or(HollowError::ResourceAccountingOverflow)?;
        mark_voxel_vertex(a, spec, &mut occupancy)?;
        mark_voxel_vertex(b, spec, &mut occupancy)?;
        mark_voxel_vertex(c, spec, &mut occupancy)
    })?;
    if observed_work_items != expected_work_items {
        return Err(HollowError::RasterWorkMismatch {
            expected_work_items,
            observed_work_items,
        });
    }
    let high_water = fill_voxel_holes(&mut occupancy, spec.dims)?;
    debug_assert!(high_water <= spec.count);
    let material = Array3::from_shape_vec((spec.dims[2], spec.dims[1], spec.dims[0]), occupancy)
        .map_err(|_| HollowError::VoxelGridTooLarge)?;
    Ok(VoxelGrid {
        origin: spec.origin,
        pitch: spec.pitch,
        material,
    })
}

fn walk_raster_leaves(
    mesh: &Mesh,
    spec: GridSpec,
    mut visit: impl FnMut([f64; 3], [f64; 3], [f64; 3]) -> Result<(), HollowError>,
) -> Result<(), HollowError> {
    for triangle in mesh.tris() {
        let [a, b, c] = *triangle;
        let point = |index| {
            mesh.verts()
                .get(usize::try_from(index).map_err(|_| HollowError::VoxelGridTooLarge)?)
                .copied()
                .ok_or(HollowError::VoxelGridTooLarge)
        };
        let mut stack = [None; RASTER_STACK_CAPACITY];
        stack[0] = Some(RasterNode {
            a: point(a)?,
            b: point(b)?,
            c: point(c)?,
            depth: 0,
        });
        let mut stack_len = 1_usize;
        while stack_len > 0 {
            stack_len -= 1;
            let node = stack[stack_len]
                .take()
                .ok_or(HollowError::ResourceAccountingOverflow)?;
            if raster_leaf(node, spec.pitch) {
                visit(node.a, node.b, node.c)?;
                continue;
            }
            if node.depth >= MAX_RASTER_SUBDIVISION_ROUNDS {
                return Err(HollowError::RasterSubdivisionLimitExceeded);
            }
            let ab = midpoint(node.a, node.b);
            let bc = midpoint(node.b, node.c);
            let ca = midpoint(node.c, node.a);
            let next = node.depth + 1;
            let children = [
                RasterNode {
                    a: ab,
                    b: bc,
                    c: ca,
                    depth: next,
                },
                RasterNode {
                    a: ca,
                    b: bc,
                    c: node.c,
                    depth: next,
                },
                RasterNode {
                    a: ab,
                    b: node.b,
                    c: bc,
                    depth: next,
                },
                RasterNode {
                    a: node.a,
                    b: ab,
                    c: ca,
                    depth: next,
                },
            ];
            let next_len = stack_len
                .checked_add(children.len())
                .ok_or(HollowError::ResourceAccountingOverflow)?;
            if next_len > RASTER_STACK_CAPACITY {
                return Err(HollowError::ResourceAccountingOverflow);
            }
            for child in children {
                stack[stack_len] = Some(child);
                stack_len += 1;
            }
        }
    }
    Ok(())
}

fn raster_leaf(node: RasterNode, pitch: f64) -> bool {
    let max_edge = pitch / 2.0;
    !edge_too_long(node.a, node.b, max_edge)
        && !edge_too_long(node.b, node.c, max_edge)
        && !edge_too_long(node.c, node.a, max_edge)
}

fn edge_too_long(left: [f64; 3], right: [f64; 3], max_edge: f64) -> bool {
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - right).powi(2))
        .sum::<f64>()
        .sqrt()
        > max_edge
}

fn midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        (left[0] + right[0]) / 2.0,
        (left[1] + right[1]) / 2.0,
        (left[2] + right[2]) / 2.0,
    ]
}

fn mark_voxel_vertex(
    point: [f64; 3],
    spec: GridSpec,
    occupancy: &mut [bool],
) -> Result<(), HollowError> {
    let mut index = [0_usize; 3];
    for axis in 0..3 {
        let grid = round_ties_even(point[axis] / spec.pitch);
        let origin = round_ties_even(spec.origin[axis] / spec.pitch);
        let relative = grid - origin;
        if !relative.is_finite()
            || relative < 0.0
            || relative >= spec.dims[axis] as f64
            || relative.fract() != 0.0
        {
            return Err(HollowError::VoxelGridTooLarge);
        }
        index[axis] = relative as usize;
    }
    let plane = spec.dims[0]
        .checked_mul(spec.dims[1])
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let flat = index[0]
        .checked_add(
            index[1]
                .checked_mul(spec.dims[0])
                .ok_or(HollowError::VoxelGridTooLarge)?,
        )
        .and_then(|value| value.checked_add(index[2].checked_mul(plane)?))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    *occupancy
        .get_mut(flat)
        .ok_or(HollowError::VoxelGridTooLarge)? = true;
    Ok(())
}

fn round_ties_even(value: f64) -> f64 {
    let lower = value.floor();
    let fractional = value - lower;
    if fractional < 0.5 {
        lower
    } else if fractional > 0.5 {
        lower + 1.0
    } else if lower.rem_euclid(2.0) == 0.0 {
        lower
    } else {
        lower + 1.0
    }
}

fn fill_voxel_holes(occupancy: &mut [bool], dims: [usize; 3]) -> Result<usize, HollowError> {
    let [nx, ny, nz] = dims;
    let plane = nx.checked_mul(ny).ok_or(HollowError::VoxelGridTooLarge)?;
    let mut exterior = vec![false; occupancy.len()];
    let mut queue = vec![0_usize; occupancy.len()];
    let mut head = 0_usize;
    let mut tail = 0_usize;
    let mut high_water = 0_usize;
    let mark_exterior = |x: usize,
                         y: usize,
                         z: usize,
                         exterior: &mut [bool],
                         queue: &mut [usize],
                         head: usize,
                         tail: &mut usize,
                         high_water: &mut usize|
     -> Result<(), HollowError> {
        let flat = x + y * nx + z * plane;
        if !occupancy[flat] && !exterior[flat] {
            exterior[flat] = true;
            *queue
                .get_mut(*tail)
                .ok_or(HollowError::ResourceAccountingOverflow)? = flat;
            *tail = tail
                .checked_add(1)
                .ok_or(HollowError::ResourceAccountingOverflow)?;
            *high_water = (*high_water).max(*tail - head);
        }
        Ok(())
    };
    for z in 0..nz {
        for y in 0..ny {
            mark_exterior(
                0,
                y,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
            mark_exterior(
                nx - 1,
                y,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
    }
    for z in 0..nz {
        for x in 0..nx {
            mark_exterior(
                x,
                0,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
            mark_exterior(
                x,
                ny - 1,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
    }
    for y in 0..ny {
        for x in 0..nx {
            mark_exterior(
                x,
                y,
                0,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
            mark_exterior(
                x,
                y,
                nz - 1,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
    }
    while head < tail {
        let flat = queue[head];
        head += 1;
        let z = flat / plane;
        let remaining = flat % plane;
        let y = remaining / nx;
        let x = remaining % nx;
        if x > 0 {
            mark_exterior(
                x - 1,
                y,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
        if x + 1 < nx {
            mark_exterior(
                x + 1,
                y,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
        if y > 0 {
            mark_exterior(
                x,
                y - 1,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
        if y + 1 < ny {
            mark_exterior(
                x,
                y + 1,
                z,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
        if z > 0 {
            mark_exterior(
                x,
                y,
                z - 1,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
        if z + 1 < nz {
            mark_exterior(
                x,
                y,
                z + 1,
                &mut exterior,
                &mut queue,
                head,
                &mut tail,
                &mut high_water,
            )?;
        }
    }
    for (filled, exterior) in occupancy.iter_mut().zip(exterior) {
        if !exterior {
            *filled = true;
        }
    }
    debug_assert!(tail <= occupancy.len());
    Ok(high_water)
}

pub(crate) fn largest_label(labels: &Array3<i32>, label_count: usize) -> Result<i32, HollowError> {
    let mut counts = vec![
        0_usize;
        label_count
            .checked_add(1)
            .ok_or(HollowError::VoxelGridTooLarge)?
    ];
    for label in labels {
        let label = usize::try_from(*label).map_err(|_| HollowError::VoxelStack)?;
        if label > label_count {
            return Err(HollowError::VoxelStack);
        }
        counts[label] = counts[label]
            .checked_add(1)
            .ok_or(HollowError::VoxelGridTooLarge)?;
    }
    let (label, count) = counts
        .iter()
        .enumerate()
        .skip(1)
        .max_by_key(|(_, count)| **count)
        .ok_or(HollowError::VoxelStack)?;
    if *count == 0 {
        Err(HollowError::VoxelStack)
    } else {
        i32::try_from(label).map_err(|_| HollowError::VoxelGridTooLarge)
    }
}

/// Interior distance on the occupancy grid, measured against a real exterior.
///
/// **Why the grid is padded before the transform.** `grid_spec` spans exactly
/// the source's bounding box, so a material voxel lying on the array boundary
/// has no background neighbour *inside the array* — and a Euclidean distance
/// transform can only measure to background samples the array contains. Every
/// flat face that is tangent to its own bounding box therefore reads as
/// interior material many millimetres deep: all six faces of a box, the end
/// caps of a cylinder, and the flat base every mini stands on. `edt > wall`
/// then selects a "cavity" that reaches the surface or passes through it, the
/// difference is no longer one negative void inside positive material, and H0
/// refuses its own shell with [`HollowError::InvalidShellTopology`].
///
/// One background layer on each side restores the boundary condition the field
/// is supposed to have — outside the model is outside — and the transform is
/// cropped back to the caller's dimensions, so nothing downstream sees the
/// margin. The padded array is a transient derived from the already-admitted
/// grid; it moves no cap, and the `max_voxel_cells` admission still binds on
/// the unpadded `GridSpec::count` exactly as before.
///
/// The margin also makes an all-foreground padded array impossible, which is
/// why no all-foreground special case survives here: the branch that used to
/// reproduce SciPy's implicit background sample at `(-1, 0, 0)` was only ever
/// reachable for a source that filled its bounding box completely — a box —
/// and for that source it returned a field rising to 17.6 mm on a 10 mm cube.
fn frozen_distance_transform(
    material: &Array3<bool>,
    pitch: f64,
) -> Result<Array3<f64>, HollowError> {
    let [nz, ny, nx]: [usize; 3] = material
        .shape()
        .try_into()
        .map_err(|_| HollowError::VoxelStack)?;
    let padded = [
        nz.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
        ny.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
        nx.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
    ];
    padded[0]
        .checked_mul(padded[1])
        .and_then(|value| value.checked_mul(padded[2]))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let bordered = Array3::from_shape_fn((padded[0], padded[1], padded[2]), |(z, y, x)| {
        z >= 1
            && z <= nz
            && y >= 1
            && y <= ny
            && x >= 1
            && x <= nx
            && material[[z - 1, y - 1, x - 1]]
    });
    let (distances, _) =
        distance_transform_edt(&bordered.into_dyn(), Some(&[pitch; 3]), true, false)
            .map_err(|_| HollowError::VoxelStack)?;
    let distances: Array3<f64> = distances
        .ok_or(HollowError::VoxelStack)?
        .into_dimensionality()
        .map_err(|_| HollowError::VoxelStack)?;
    crop_margin_in_place(distances, [nz, ny, nx], padded)
}

/// Crops the one-voxel margin off a padded field **without allocating a second
/// full-size array**.
///
/// The obvious `Array3::from_shape_fn` over the caller's dimensions holds the
/// padded `(n+2)³` f64 array alive — it is the source being read — while it
/// allocates the `n³` f64 result: at the admitted 16,777,216-cell grid that is
/// ~141 MB and ~134 MB **simultaneously**, which is a transient peak of ~2× the
/// unpadded field rather than the +4 % the padding itself costs.
///
/// So the interior is compacted into the front of the padded array's own
/// buffer and the buffer is truncated. Every source index is greater than or
/// equal to its destination index, and both walk forward, so one forward sweep
/// of whole contiguous rows can never overwrite a value it has not yet read:
/// row `(z, y)`'s destination ends exactly where row `(z, y+1)`'s destination
/// begins, which is at or before that row's source.
fn crop_margin_in_place(
    padded_field: Array3<f64>,
    dims: [usize; 3],
    padded: [usize; 3],
) -> Result<Array3<f64>, HollowError> {
    let [nz, ny, nx] = dims;
    // The compaction addresses the buffer directly, so it is only valid on the
    // C-order layout `distance_transform_edt` returns. Refuse rather than
    // silently transpose the field.
    if !padded_field.is_standard_layout() {
        return Err(HollowError::VoxelStack);
    }
    let (mut buffer, offset) = padded_field.into_raw_vec_and_offset();
    if offset.unwrap_or(0) != 0 {
        return Err(HollowError::VoxelStack);
    }
    for z in 0..nz {
        for y in 0..ny {
            // One contiguous row per call: the copy is a `memmove`, not a
            // per-element loop, and there are `nz × ny` of them rather than
            // `nz × ny × nx` scalar reads.
            let source = ((z + 1) * padded[1] + (y + 1)) * padded[2] + 1;
            let destination = (z * ny + y) * nx;
            buffer.copy_within(source..source + nx, destination);
        }
    }
    buffer.truncate(nz * ny * nx);
    Array3::from_shape_vec((nz, ny, nx), buffer).map_err(|_| HollowError::VoxelStack)
}

pub(crate) fn marching_cavity(
    mask: &Array3<bool>,
    origin: [f64; 3],
    pitch: f64,
) -> Result<Mesh, HollowError> {
    let [nz, ny, nx]: [usize; 3] = mask
        .shape()
        .try_into()
        .map_err(|_| HollowError::VoxelStack)?;
    let padded = [
        nx.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
        ny.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
        nz.checked_add(2).ok_or(HollowError::VoxelGridTooLarge)?,
    ];
    let dimensions = [
        u32::try_from(padded[0]).map_err(|_| HollowError::VoxelGridTooLarge)?,
        u32::try_from(padded[1]).map_err(|_| HollowError::VoxelGridTooLarge)?,
        u32::try_from(padded[2]).map_err(|_| HollowError::VoxelGridTooLarge)?,
    ];
    let shape = RuntimeShape::<u32, 3>::new(dimensions);
    let max = dimensions.map(|value| value - 1);
    let padded_count = padded[0]
        .checked_mul(padded[1])
        .and_then(|value| value.checked_mul(padded[2]))
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let mut sdf = vec![1.0_f32; padded_count];
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                if mask[[z, y, x]] {
                    let index = (z + 1) * padded[1] * padded[0] + (y + 1) * padded[0] + x + 1;
                    sdf[index] = -1.0;
                }
            }
        }
    }
    let mut surface = SurfaceNetsBuffer::default();
    surface_nets(&sdf, &shape, [0; 3], max, &mut surface);
    let vertices = surface
        .positions
        .into_iter()
        .map(|[x, y, z]| {
            [
                origin[0] + (f64::from(x) - 1.0) * pitch,
                origin[1] + (f64::from(y) - 1.0) * pitch,
                origin[2] + (f64::from(z) - 1.0) * pitch,
            ]
        })
        .collect();
    if surface.indices.len() % 3 != 0 {
        return Err(HollowError::VoxelStack);
    }
    let triangles = surface
        .indices
        .chunks_exact(3)
        .map(|indices| {
            let a = indices[0];
            let b = indices[1];
            let c = indices[2];
            // `fast_surface_nets` orients this negative-inside level set toward
            // the filled cavity. Manifold operands must use outward winding.
            Ok::<[u32; 3], HollowError>([a, c, b])
        })
        .collect::<Result<Vec<_>, _>>()?;
    Mesh::new(vertices, triangles).map_err(HollowError::OutputMesh)
}

pub(crate) fn bounds(mesh: &Mesh) -> Result<([f64; 3], [f64; 3]), HollowError> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for point in mesh.verts() {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    if low.iter().chain(high.iter()).all(|value| value.is_finite()) {
        Ok((low, high))
    } else {
        Err(HollowError::InvalidArithmetic)
    }
}

fn validate_printable_bounds(mesh: &Mesh) -> Result<(), HollowError> {
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for point in mesh.verts() {
        for axis in 0..3 {
            low[axis] = low[axis].min(point[axis]);
            high[axis] = high[axis].max(point[axis]);
        }
    }
    for axis in 0..3 {
        let extent = high[axis] - low[axis];
        if !extent.is_finite() {
            return Err(HollowError::InvalidArithmetic);
        }
        if extent > PRINTABLE_MAX_EXTENT_MM {
            return Err(HollowError::OutsidePrintableEnvelope { axis });
        }
    }
    Ok(())
}

pub(crate) fn to_solid(mesh: &Mesh) -> Result<SolidMesh, HollowError> {
    let triangles = mesh
        .tris()
        .iter()
        .map(|triangle| triangle.map(u64::from))
        .collect();
    SolidMesh::new(mesh.verts().to_vec(), triangles).map_err(HollowError::SolidMesh)
}

pub(crate) fn from_solid(mesh: SolidMesh) -> Result<Mesh, HollowError> {
    let triangles = mesh
        .triangles()
        .iter()
        .enumerate()
        .map(|(triangle, indices)| {
            Ok::<[u32; 3], HollowError>([
                u32::try_from(indices[0])
                    .map_err(|_| HollowError::OutputIndexOverflow { triangle })?,
                u32::try_from(indices[1])
                    .map_err(|_| HollowError::OutputIndexOverflow { triangle })?,
                u32::try_from(indices[2])
                    .map_err(|_| HollowError::OutputIndexOverflow { triangle })?,
            ])
        })
        .collect::<Result<Vec<_>, _>>()?;
    Mesh::new(mesh.vertices().to_vec(), triangles).map_err(HollowError::OutputMesh)
}

fn finite(value: f64) -> Result<Finite, HollowError> {
    Finite::new(value).map_err(|_| HollowError::InvalidArithmetic)
}

#[derive(Debug, Error)]
pub enum HollowError {
    #[error("M0 constants publication failed: {0}")]
    Constants(&'static M0ConstantsError),
    #[error(
        "hollow request printer profile {actual} does not match registered profile {expected}"
    )]
    PrinterProfileMismatch { expected: String, actual: String },
    #[error(
        "requested wall {wall_mm} mm is finer than the frozen {voxel_pitch_mm} mm voxel pitch, which cannot produce it"
    )]
    WallBelowVoxelPitch { wall_mm: f64, voxel_pitch_mm: f64 },
    #[error("drain planning refused: {0}")]
    DrainPlanning(drains::DrainPlanningRefusal),
    #[error("drilling proof refused: {0}")]
    Drilling(drilling::DrillingRefusal),
    #[error(transparent)]
    Triage(#[from] TriageError),
    #[error(transparent)]
    TriageAdmission(#[from] TriageAdmissionError),
    #[error("strict mesh audit failed: {0}")]
    StrictAudit(StrictMeshAuditError),
    #[error("topology construction failed: {0}")]
    Topology(TopologyError),
    #[error("topology validation failed: {0}")]
    TopologyValidation(TopologyValidationError),
    #[error("input is not one closed, consistently oriented solid")]
    NotClosedOrOriented,
    #[error("geometry exceeds H0's printable 300mm extent on axis {axis}")]
    OutsidePrintableEnvelope { axis: usize },
    #[error("non-finite or unrepresentable H0 arithmetic")]
    InvalidArithmetic,
    #[error("the printable voxel grid exceeds addressable storage")]
    VoxelGridTooLarge,
    #[error("H0 admission limits are invalid")]
    InvalidAdmissionLimits,
    #[error("checked H0 resource accounting overflowed")]
    ResourceAccountingOverflow,
    #[error("voxel grid has {cells} cells, exceeding the H0 limit {max_cells}")]
    VoxelCellLimitExceeded { cells: usize, max_cells: usize },
    #[error(
        "frozen voxel subdivision exceeded the caller work cap {max_work_items} at item {work_items}"
    )]
    RasterWorkLimitExceeded {
        work_items: usize,
        max_work_items: usize,
    },
    #[error("frozen voxel subdivision exceeded trimesh's exact ten-round limit")]
    RasterSubdivisionLimitExceeded,
    #[error("the second frozen raster pass disagreed with its preflight work count")]
    RasterWorkMismatch {
        expected_work_items: usize,
        observed_work_items: usize,
    },
    #[error("geom-kernel could not construct the batched voxelization surface")]
    Kernel,
    #[error("the established EDT/label/marching-surface adapter rejected the voxel field")]
    VoxelStack,
    #[error("the model has no interior beyond the 10mm bulk threshold")]
    IneligibleBelowBulk,
    #[error("the wall offset leaves no cavity")]
    IneligibleNoCavity,
    #[error("the largest cavity does not meet the registered resin-saving threshold")]
    IneligibleSavings,
    /// The well-composedness repair consumed the whole selected component.
    ///
    /// Distinct from [`Self::IneligibleNoCavity`] on purpose: that one says the
    /// *geometry* has no room for a cavity at the stated wall, this one says
    /// the cavity existed and the **repair** removed it. Collapsing the two
    /// would put a repair outcome into the record as a property of the model,
    /// which is the one thing the diagnostics carried here exist to prevent.
    #[error(
        "the well-composedness repair removed the whole cavity: {critical_cells} critical cells, \
         {repaired_voxels} voxels returned to material over {cell_visits} cell visits and \
         {full_grid_passes} whole-grid passes"
    )]
    CavityRepairRemovedCavity {
        critical_cells: usize,
        repaired_voxels: usize,
        cell_visits: usize,
        full_grid_passes: usize,
    },
    /// The cavity survived the repair but fell under the savings gate, and the
    /// repair had taken voxels out of it. Whether it would have passed without
    /// the repair is not claimed here — what is claimed is that a repair ran,
    /// how big it was, and that the gate was judged after it.
    #[error(
        "the largest cavity does not meet the registered resin-saving threshold after the \
         well-composedness repair returned {repaired_voxels} voxels to material \
         ({critical_cells} critical cells, {cell_visits} cell visits, \
         {full_grid_passes} whole-grid passes)"
    )]
    IneligibleSavingsAfterCavityRepair {
        critical_cells: usize,
        repaired_voxels: usize,
        cell_visits: usize,
        full_grid_passes: usize,
    },
    #[error("plinth-solid mesh conversion failed: {0}")]
    SolidMesh(plinth_solid::MeshValidationError),
    #[error("plinth-solid operation failed: {0}")]
    Solid(SolidError),
    #[error("plinth-solid output index does not fit plinth-mesh at triangle {triangle}")]
    OutputIndexOverflow { triangle: usize },
    #[error("plinth-solid output cannot become a plinth-mesh: {0}")]
    OutputMesh(plinth_mesh::MeshError),
    #[error(
        "native shell did not contain one negative cavity enclosed by positive outer material"
    )]
    InvalidShellTopology,
    #[error("Boolean material volume {actual_material_mm3} differs from expected {expected_material_mm3} beyond {tolerance_mm3} mm3")]
    BooleanVolumeMismatch {
        actual_material_mm3: f64,
        expected_material_mm3: f64,
        tolerance_mm3: f64,
    },
    #[error("cavity extraction failed")]
    CavityExtractionFailed,
    #[error("drain geometry could not be constructed")]
    DrainGeometryFailed,
    #[error("the hollowed or retained-solid production mesh is invalid")]
    InvalidProductionMesh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HollowFailureCategory {
    PrinterProfile,
    Shell,
    DrainPlanning,
    Drilling,
}

impl HollowError {
    #[must_use]
    pub const fn category(&self) -> HollowFailureCategory {
        match self {
            Self::PrinterProfileMismatch { .. } => HollowFailureCategory::PrinterProfile,
            Self::DrainPlanning(_) => HollowFailureCategory::DrainPlanning,
            Self::Drilling(_) => HollowFailureCategory::Drilling,
            _ => HollowFailureCategory::Shell,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_decimate::{run as run_decimation, DecimationAudit, DecimationBudget, ProbeConfig};
    use plinth_mesh::{load_binary_stl, make_box, mesh_fingerprint};
    use plinth_orient::{
        BaselineSelection, OrientationProvenance, OrientationReport, RigidTransform,
    };
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn fixture_decimation_audit() -> DecimationAudit {
        let mesh = plinth_mesh::make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0))
                .expect("registered fixture budget");
        let outcome = run_decimation(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        audit
    }

    fn frozen_mesh(file: &str) -> Mesh {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(file);
        load_binary_stl(path, WeldPolicy::None).expect("committed frozen STL")
    }

    fn orientation_history() -> OrientationHistory {
        OrientationHistory::from_reports(
            fixture_decimation_audit(),
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        )
    }

    #[derive(Debug)]
    struct Characterization {
        spec: GridSpec,
        bulk_mm: f64,
        label_count: usize,
        cavity_voxels: usize,
    }

    fn characterize(input: Mesh) -> (usize, Characterization) {
        let triaged = triage_exact_with_measured_limits(&input).expect("frozen triage");
        let components = triaged.summary().component_count();
        let source = triaged.into_mesh();
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let spec = grid_spec(&source, settings.voxel_pitch_mm().get()).expect("grid spec");
        let work = raster_work_item_count(
            &source,
            spec,
            H0AdmissionLimits::production().max_raster_work_items(),
        )
        .expect("bounded source work");
        let grid = voxelize_filled(&source, spec, work).expect("bounded voxelization");
        let edt = frozen_distance_transform(&grid.material, grid.pitch).expect("EDT");
        let bulk_mm = 2.0 * edt.iter().copied().fold(0.0_f64, f64::max);
        let cavity = edt.mapv(|distance| distance > settings.wall_thickness_mm().get());
        let (labels, label_count) = label3d(cavity.view(), 6).expect("6-connected labels");
        let largest =
            (label_count > 0).then(|| largest_label(&labels, label_count).expect("largest label"));
        let cavity_voxels = largest.map_or(0, |label| {
            labels
                .iter()
                .filter(|candidate| **candidate == label)
                .count()
        });
        (
            components,
            Characterization {
                spec,
                bulk_mm,
                label_count,
                cavity_voxels,
            },
        )
    }

    #[test]
    fn frozen_oracle_characterizes_transforms_boundaries_eligibility_and_extra_body() {
        // Record of record: corpus/baseline/Sphere.pipeline.json.
        let (sphere_components, sphere) =
            characterize(frozen_mesh("Sphere.prepped.oriented-solid.stl"));
        assert_eq!(sphere_components, 1);
        assert_eq!(sphere.spec.dims, [41, 41, 41]);
        assert_eq!(sphere.spec.origin, [-10.0, -10.0, 0.0]);
        assert!((sphere.bulk_mm - 20.6).abs() <= 0.1);
        assert_eq!(sphere.label_count, 1);
        // Moved 16,563 -> 16,491 on 2026-08-10 by the cavity-field exterior
        // margin. The 72 voxels that left are the six polar tangent points
        // where the sphere touches its own bounding box: with no background
        // sample beyond the array edge they read as deep interior and were
        // selected as cavity, so the shell's wall at each pole was a fraction
        // of a millimetre rather than the stated 2.5 mm. See the 2026-08-10
        // journal section.
        assert_eq!(sphere.cavity_voxels, 16_491);

        // Record of record: make_box is the bounding-box-filling case. Every
        // one of its six faces is tangent to its own grid, so its occupancy is
        // 9,261/9,261 and the field has no interior background at all -- which
        // is the whole reason the margin exists.
        let (box_components, box_case) =
            characterize(frozen_mesh("make_box.prepped.oriented-solid.stl"));
        assert_eq!(box_components, 1);
        assert_eq!(box_case.spec.dims, [21, 21, 21]);
        assert_eq!(box_case.spec.origin, [-5.0, -5.0, 0.0]);
        // Moved 35.2 mm -> 11.0 mm on 2026-08-10. A 10 mm cube does not have
        // 35 mm of bulk; the old figure came from the SciPy all-foreground
        // compatibility branch measuring from an implicit sample at
        // (-1, 0, 0). 11.0 is 2 x the true centre distance of 5.5 mm, which
        // carries the same one-pitch centre-to-centre offset the rest of the
        // field carries (the Sphere's 20.6 on a 20 mm sphere, above).
        assert!((box_case.bulk_mm - 11.0).abs() <= 0.1, "{box_case:?}");
        assert_eq!(box_case.label_count, 1);
        // Moved 1.1 -> 0.166 mm^3-thousands. The old cavity was 1,148.5 mm^3
        // against a 1,000 mm^3 source -- a "cavity" larger than the solid it
        // was cut from, which is the arithmetic signature of the defect.
        assert!((box_case.cavity_voxels as f64 * 0.125 / 1000.0 - 0.166).abs() <= 0.01);
        assert!(
            box_case.cavity_voxels as f64 * 0.125 < 1_000.0,
            "a cavity can never be larger than the solid it is cut from"
        );

        // Record of record: make_overhang_table.pipeline.json preserves four
        // authored extra bodies. Its 6.1mm source raster bulk is well below
        // the registered 10mm eligibility threshold.
        let (table_components, table) =
            characterize(frozen_mesh("make_overhang_table.prepped.stl"));
        assert_eq!(table_components, 5);
        assert!((table.bulk_mm - 6.1).abs() <= 0.1, "{table:?}");
        assert!(table.bulk_mm < 10.0);
    }

    /// The cavity field's boundary condition, pinned on an array with no
    /// interior background at all.
    ///
    /// A solid that fills its own bounding box produces an all-foreground
    /// occupancy grid. A Euclidean distance transform can only measure to
    /// background samples the array contains, so without a margin this case
    /// has no answer, and the compatibility branch that used to serve it
    /// measured from an implicit sample at (-1, 0, 0) — 17.6 mm of "interior"
    /// on a 10 mm cube. With the margin the answer is the one the geometry
    /// actually has: one voxel step out of the corner, three out of the
    /// centre of a 5-cube.
    #[test]
    fn the_cavity_field_measures_to_the_exterior_rather_than_the_array_edge() {
        let pitch = 0.5_f64;
        let filled = Array3::from_elem((5, 5, 5), true);
        let field = frozen_distance_transform(&filled, pitch).expect("padded field");
        assert_eq!(field.shape(), &[5, 5, 5], "the margin must not escape");
        assert_eq!(field[[0, 0, 0]], pitch);
        assert_eq!(field[[4, 4, 4]], pitch);
        assert_eq!(field[[2, 2, 2]], 3.0 * pitch);
        let max = field.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(max, 3.0 * pitch);

        // The margin is a boundary condition, not a bias: a voxel with real
        // background beside it inside the array still measures to that
        // background and is unaffected.
        let mut punctured = Array3::from_elem((5, 5, 5), true);
        punctured[[2, 2, 2]] = false;
        let punctured = frozen_distance_transform(&punctured, pitch).expect("padded field");
        assert_eq!(punctured[[2, 2, 1]], pitch);
        assert_eq!(punctured[[0, 0, 0]], pitch);
    }

    /// The bounding-box-filling solid, end to end.
    ///
    /// Before the margin every cube refused with `InvalidShellTopology` — the
    /// selected cavity was *larger than the source solid* (1,148.5 mm³ against
    /// 1,000 mm³ on a 10 mm cube; 28,363.5 against 27,000 on this one), so the
    /// marching surface enclosed the solid instead of sitting inside it and
    /// the difference was not one negative void in positive material.
    ///
    /// A 30 mm cube rather than the corpus's 10 mm `make_box`, because the
    /// smaller one is now correctly *ineligible*: its true 166 mm³ cavity does
    /// not clear the registered resin-saving threshold, which is a different
    /// verdict and would not exercise the Boolean at all.
    #[test]
    fn a_solid_that_fills_its_own_bounding_box_hollows_instead_of_refusing_its_shell() {
        let source = make_box([30.0, 30.0, 30.0], [0.0, 0.0, 0.0]).expect("box fixture");
        let source_bounds = bounds(&source);
        let candidate = create_shell(source, registered_settings())
            .expect("a box is hollow-eligible once its field has an exterior");
        assert_eq!(
            bounds(candidate.mesh()),
            source_bounds,
            "outer surface must not drift"
        );
        let topology =
            build_face_topology(candidate.mesh(), WeldPolicy::ExactBits).expect("topology");
        let certificate =
            certify_closed_components(candidate.mesh(), &topology).expect("certificate");
        assert_eq!(certificate.components().len(), 2);
        assert_eq!(
            certificate
                .components()
                .iter()
                .filter(|component| component.signed_volume_mm3() < 0.0)
                .count(),
            1,
            "the inner surface must be a negative, contained cavity"
        );
        assert_eq!(candidate.cavity().source_volume_mm3().get(), 27_000.0);
        // A 30 mm cube at a 2.5 mm wall leaves a 25 mm cube of cavity, plus
        // the one-pitch centre-to-centre margin the discrete field carries.
        assert!(
            (16_000.0..=17_000.0).contains(&candidate.cavity().cavity_volume_mm3().get()),
            "{:?}",
            candidate.cavity().cavity_volume_mm3()
        );
        assert!(
            candidate.cavity().cavity_volume_mm3().get()
                < candidate.cavity().source_volume_mm3().get()
        );
    }

    /// Flat faces tangent to the bounding box, on committed corpus geometry.
    ///
    /// The `Cylinder`'s end caps lie exactly on the z = 0 and z = 30 grid
    /// planes. Before the margin they read as interior, the selected cavity
    /// reached from z = −0.25 to z = 30.25 — through both caps — and the shell
    /// was refused. This is the class every mini standing on a flat base is
    /// in, which is why it is gated on a corpus model and not only on the
    /// synthetic cube above.
    #[test]
    fn flat_faces_tangent_to_the_bounding_box_no_longer_leak_the_cavity_through_the_surface() {
        let source = frozen_mesh("Cylinder.prepped.oriented-solid.stl");
        let source_bounds = bounds(&source);
        let candidate = create_shell(source, registered_settings())
            .expect("the frozen cylinder is hollow-eligible");
        assert_eq!(bounds(candidate.mesh()), source_bounds);
        let topology =
            build_face_topology(candidate.mesh(), WeldPolicy::ExactBits).expect("topology");
        let certificate =
            certify_closed_components(candidate.mesh(), &topology).expect("certificate");
        assert_eq!(certificate.components().len(), 2);
        assert_eq!(
            certificate
                .components()
                .iter()
                .filter(|component| component.signed_volume_mm3() < 0.0)
                .count(),
            1
        );
        assert!(
            candidate.cavity().cavity_volume_mm3().get()
                < candidate.cavity().source_volume_mm3().get()
        );
    }

    #[test]
    fn frozen_voxel_rounding_and_exterior_fill_rules_are_independently_pinned() {
        // NumPy's `round` is nearest with half-to-even, including negatives.
        assert_eq!(round_ties_even(-2.5), -2.0);
        assert_eq!(round_ties_even(-1.5), -2.0);
        assert_eq!(round_ties_even(-0.5), 0.0);
        assert_eq!(round_ties_even(0.5), 0.0);
        assert_eq!(round_ties_even(1.5), 2.0);
        assert_eq!(round_ties_even(2.5), 2.0);

        // The complement flood uses SciPy's default face connectivity. The
        // central empty cell is only diagonally connected to the boundary,
        // so it is filled; a 26-connected flood would leave it empty.
        let mut material = vec![true; 27];
        material[0] = false;
        material[13] = false;
        let high_water =
            fill_voxel_holes(&mut material, [3, 3, 3]).expect("bounded 6-neighbor fill");
        assert!(!material[0]);
        assert!(material[13]);
        assert!(high_water <= material.len());
    }

    #[test]
    fn grid_spec_uses_numpy_ties_to_even_at_both_transform_ends() {
        let spec = grid_spec_from_bounds([0.5, -1.5, -0.5], [1.5, -0.5, 1.5], 1.0)
            .expect("all rounded endpoints span three samples");
        assert_eq!(spec.origin, [0.0, -2.0, 0.0]);
        assert_eq!(spec.dims, [3, 3, 3]);
    }

    #[test]
    fn frozen_subdivision_preflight_has_exact_cap_and_stops_before_raster_allocation() {
        let spec = GridSpec {
            origin: [0.0, 0.0, 0.0],
            dims: [5, 5, 5],
            pitch: 0.5,
            count: 125,
        };
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("finite triangle");
        let exact = raster_work_item_count(&mesh, spec, usize::MAX).expect("bounded work");
        assert!(exact > 1);
        assert_eq!(
            raster_work_item_count(&mesh, spec, exact).expect("exact cap"),
            exact
        );
        assert!(matches!(
            raster_work_item_count(&mesh, spec, exact - 1),
            Err(HollowError::RasterWorkLimitExceeded { .. })
        ));

        let malicious = Mesh::new(
            vec![[0.0, 0.0, 0.0], [300.0, 0.0, 0.0], [0.0, 300.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("finite triangle");
        assert!(matches!(
            raster_work_item_count(&malicious, spec, usize::MAX),
            Err(HollowError::RasterSubdivisionLimitExceeded)
        ));
    }

    #[test]
    fn authorized_internal_fallback_moves_one_mesh_and_is_visible_in_history() {
        let mesh = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let allocation = mesh.verts().as_ptr();
        let model = HollowedModel::solid(
            mesh,
            orientation_history(),
            HollowDisposition::SolidFallback {
                reason: InternalSupportRefusalReason::InternalVerifierException,
            },
            None,
        )
        .expect("closed oriented solid is an executable public fallback handoff");
        let (mesh, history) = model.into_parts();
        assert_eq!(mesh.verts().as_ptr(), allocation);
        assert_eq!(
            history.hollow().disposition(),
            HollowDisposition::SolidFallback {
                reason: InternalSupportRefusalReason::InternalVerifierException
            }
        );
    }

    /// A repair-caused solid decision is distinguishable in the record, and it
    /// carries the repair's own numbers.
    ///
    /// Before this existed, a repair that consumed the whole cavity returned
    /// `IneligibleNoCavity` and reached the record as `NoCavity` — a claim about
    /// the *model's shape*, made on behalf of a repair. The reason and the
    /// evidence are asserted together because either alone is insufficient: the
    /// reason without the numbers cannot say how much cavity was taken, and the
    /// numbers without the reason do not say the repair is why the model is
    /// solid.
    #[test]
    fn a_repair_caused_solid_decision_carries_its_reason_and_its_measurement() {
        let evidence = CavityRepairEvidence {
            critical_cells: 12,
            repaired_voxels: 34,
            cell_visits: 5_678,
            full_grid_passes: 2,
        };
        let model = HollowedModel::solid_decision_recording_repair(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            orientation_history(),
            SolidDecisionReason::NoCavityAfterRepair,
            Some(evidence),
        )
        .expect("a closed oriented solid is a valid reserve");
        assert_eq!(
            model.history().hollow().disposition(),
            HollowDisposition::Solid {
                reason: SolidDecisionReason::NoCavityAfterRepair
            }
        );
        assert_eq!(model.history().hollow().cavity_repair(), Some(evidence));
        // The plain decision path makes no repair claim at all, which is what
        // `None` means here — not "a repair of size zero".
        let plain = HollowedModel::solid_decision(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            orientation_history(),
            SolidDecisionReason::NoCavity,
        )
        .expect("a closed oriented solid is a valid reserve");
        assert_eq!(plain.history().hollow().cavity_repair(), None);
    }

    /// The failure type that carries the measurement past an untyped refusal.
    ///
    /// Both halves matter. The evidence has to be readable without consuming
    /// the error, because a caller records the refusal string *and* the
    /// measurement; and the text has to be the inner error's own, because a
    /// refusal string already recorded in artifacts and journals must not
    /// change because the evidence gained a carrier.
    #[test]
    fn a_hollow_stage_failure_states_its_error_verbatim_and_its_repair_beside_it() {
        let bare = HollowStageFailure::before_cavity_repair(HollowError::IneligibleNoCavity);
        assert_eq!(
            bare.to_string(),
            HollowError::IneligibleNoCavity.to_string()
        );
        assert_eq!(bare.cavity_repair(), None);

        let evidence = CavityRepairEvidence {
            critical_cells: 4,
            repaired_voxels: 4,
            cell_visits: 411_674,
            full_grid_passes: 2,
        };
        let carried =
            HollowStageFailure::after_cavity_repair(HollowError::InvalidShellTopology, evidence);
        assert_eq!(
            carried.to_string(),
            HollowError::InvalidShellTopology.to_string()
        );
        assert_eq!(carried.cavity_repair(), Some(evidence));
        assert!(matches!(
            carried.into_error(),
            HollowError::InvalidShellTopology
        ));
    }

    /// A support refusal substitutes the oriented solid for a shell that **was
    /// built**, so a repair may well have run on it.
    ///
    /// The measurement has to survive the substitution: the substituted record
    /// is the one a reader uses to decide how the model was treated, and the
    /// shell whose own report held the number is discarded here. The reserve
    /// makes no claim about its own wall — that is not what is being carried.
    #[test]
    fn a_support_refusal_substitution_carries_the_discarded_shells_repair_measurement() {
        let evidence = CavityRepairEvidence {
            critical_cells: 4,
            repaired_voxels: 4,
            cell_visits: 411_674,
            full_grid_passes: 2,
        };
        // Stands in for the drilled shell's own report; `into_solid_fallback`
        // reads the number off it rather than restating one of its own.
        let hollowed = HollowedModel::solid_decision_recording_repair(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            orientation_history(),
            SolidDecisionReason::NoCavityAfterRepair,
            Some(evidence),
        )
        .expect("a closed oriented solid is a valid stand-in");
        let pair = HollowedWithSolidReserve {
            hollowed,
            reserve_mesh: frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            reserve_history: orientation_history(),
        };
        let substituted = pair
            .into_solid_fallback(InternalSupportRefusalReason::A5GateFailure)
            .expect("the reserve is a valid oriented solid");
        assert_eq!(
            substituted.history().hollow().disposition(),
            HollowDisposition::SolidFallback {
                reason: InternalSupportRefusalReason::A5GateFailure
            }
        );
        assert_eq!(
            substituted.history().hollow().cavity_repair(),
            Some(evidence),
            "the substitution dropped the repair the discarded shell recorded"
        );
    }

    #[test]
    fn solid_fallback_rejects_inverted_and_oversized_closed_meshes() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let inverted = Mesh::new(
            source.verts().to_vec(),
            source
                .tris()
                .iter()
                .map(|triangle| [triangle[0], triangle[2], triangle[1]])
                .collect(),
        )
        .unwrap();
        assert!(matches!(
            HollowedModel::solid(
                inverted,
                orientation_history(),
                HollowDisposition::SolidFallback {
                    reason: InternalSupportRefusalReason::A5GateFailure,
                },
                None,
            ),
            Err(HollowError::NotClosedOrOriented)
        ));

        let oversized = Mesh::new(
            source
                .verts()
                .iter()
                .map(|point| point.map(|coordinate| coordinate * 20.0))
                .collect(),
            source.tris().to_vec(),
        )
        .unwrap();
        assert!(matches!(
            HollowedModel::solid(
                oversized,
                orientation_history(),
                HollowDisposition::SolidFallback {
                    reason: InternalSupportRefusalReason::A5GateFailure,
                },
                None,
            ),
            Err(HollowError::OutsidePrintableEnvelope { .. })
        ));
    }

    fn bounds(mesh: &Mesh) -> ([f64; 3], [f64; 3]) {
        let mut low = [f64::INFINITY; 3];
        let mut high = [f64::NEG_INFINITY; 3];
        for point in mesh.verts() {
            for axis in 0..3 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        }
        (low, high)
    }

    fn join(left: &Mesh, right: &Mesh) -> Mesh {
        let offset = u32::try_from(left.verts().len()).expect("fixture vertex count fits u32");
        let mut vertices = left.verts().to_vec();
        vertices.extend_from_slice(right.verts());
        let mut triangles = left.tris().to_vec();
        triangles.extend(right.tris().iter().map(|triangle| {
            triangle.map(|index| {
                index
                    .checked_add(offset)
                    .expect("fixture index cannot overflow")
            })
        }));
        Mesh::new(vertices, triangles).expect("joined fixture is structurally valid")
    }

    #[test]
    fn registered_shell_is_closed_with_one_retained_2_5mm_cavity() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let source_bounds = bounds(&source);
        let candidate =
            create_shell(source, registered_settings()).expect("frozen sphere is hollow-eligible");
        let output = candidate.mesh();
        assert_eq!(
            bounds(output),
            source_bounds,
            "outer surface must not drift"
        );
        let topology = build_face_topology(output, WeldPolicy::ExactBits).expect("topology");
        let certificate = certify_closed_components(output, &topology).expect("certificate");
        assert_eq!(
            certificate
                .components()
                .iter()
                .filter(|component| component.signed_volume_mm3() < 0.0)
                .count(),
            1,
            "the inner surface must be a negative, contained cavity"
        );
        assert_eq!(candidate.cavity().wall_thickness_mm().get(), 2.5);
        assert_eq!(candidate.cavity().frozen_voxel_pitch_mm().get(), 0.5);
        assert!(
            (1_900.0..=2_300.0).contains(&candidate.cavity().cavity_volume_mm3().get()),
            "the frozen 0.5mm EDT shell has the expected 2.5mm-wall cavity range"
        );
        assert!(
            candidate.cavity().cavity_volume_mm3().get()
                < candidate.cavity().source_volume_mm3().get()
        );
    }

    #[test]
    fn open_prehollowed_and_ineligible_meshes_refuse_without_repair() {
        let solid = make_box([50.0, 50.0, 50.0], [0.0, 0.0, 0.0]).expect("box fixture");
        let open =
            Mesh::new(solid.verts().to_vec(), solid.tris()[1..].to_vec()).expect("open fixture");
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let retained_bytes = open.retained_buffer_bytes().expect("retained source bytes");
        let mut exact_retained = H0AdmissionLimits::production();
        exact_retained.max_retained_source_buffer_bytes = retained_bytes;
        assert!(matches!(
            create_shell_with_admission_limits(open.clone(), settings, exact_retained),
            Err(HollowError::NotClosedOrOriented)
        ));
        let mut below_retained = exact_retained;
        below_retained.max_retained_source_buffer_bytes = retained_bytes - 1;
        assert!(matches!(
            create_shell_with_admission_limits(open.clone(), settings, below_retained),
            Err(HollowError::TriageAdmission(TriageAdmissionError::LimitExceeded {
                field: "retained source buffer bytes",
                actual,
                limit,
            })) if actual == retained_bytes && limit == retained_bytes - 1
        ));
        let mut admission_refusal = H0AdmissionLimits::production();
        admission_refusal.max_raw_faces = 0;
        assert!(matches!(
            create_shell_with_admission_limits(open.clone(), settings, admission_refusal),
            Err(HollowError::TriageAdmission(
                TriageAdmissionError::LimitExceeded {
                    field: "raw faces",
                    ..
                }
            ))
        ));
        assert!(matches!(
            create_shell(open, settings),
            Err(HollowError::NotClosedOrOriented)
        ));

        let outer = make_box([50.0, 50.0, 50.0], [0.0, 0.0, 0.0]).expect("outer fixture");
        let inner = make_box([20.0, 20.0, 20.0], [0.0, 0.0, 15.0]).expect("inner fixture");
        let prehollowed = join(&outer, &inner);
        assert!(matches!(
            create_shell_with_admission_limits(prehollowed.clone(), settings, admission_refusal),
            Err(HollowError::TriageAdmission(
                TriageAdmissionError::LimitExceeded {
                    field: "raw faces",
                    ..
                }
            ))
        ));
        assert!(matches!(
            create_shell(prehollowed, settings),
            Err(HollowError::Triage(TriageError::PreHollowed(_)))
        ));

        let thin = frozen_mesh("make_overhang_table.prepped.stl");
        assert!(matches!(
            create_shell(thin, settings),
            Err(HollowError::IneligibleBelowBulk)
        ));
    }

    fn registered_printer() -> PrinterProfile {
        m0_constants()
            .expect("registered constants")
            .saturn4_ultra()
            .clone()
    }

    #[test]
    fn the_wall_is_the_one_registry_value_a_request_states_and_both_options_are_registered() {
        let default = HollowRequest::with_registered_wall_default(registered_printer())
            .expect("registered wall default");
        let heavy = HollowRequest::with_registered_handling_heavy_wall(registered_printer())
            .expect("registered handling-heavy wall");
        assert_eq!(default.wall_mm.get(), 2.5);
        assert_eq!(heavy.wall_mm.get(), 3.0);

        // `from_request` replaces the wall and nothing else: every other field
        // still comes from the one M0 publication.
        let registered = HollowSettings::from_registered_constants().expect("M0 settings");
        assert_eq!(
            HollowSettings::from_request(&default).expect("default request settings"),
            registered
        );
        let heavy_settings = HollowSettings::from_request(&heavy).expect("heavy request settings");
        assert_eq!(heavy_settings.wall_thickness_mm().get(), 3.0);
        assert_eq!(
            HollowSettings {
                wall_thickness_mm: registered.wall_thickness_mm(),
                ..heavy_settings
            },
            registered,
            "only the wall may differ between a request and the registry defaults"
        );

        // An arbitrary operator wall is carried verbatim; nothing rounds it to
        // one of the two registered values, and nothing classifies the model.
        let stated = HollowRequest {
            printer: registered_printer(),
            wall_mm: PositiveFinite::new(1.75).expect("positive finite wall"),
        };
        assert_eq!(
            HollowSettings::from_request(&stated)
                .expect("stated request settings")
                .wall_thickness_mm()
                .get(),
            1.75
        );
    }

    #[test]
    fn a_wall_finer_than_the_frozen_voxel_pitch_is_refused_rather_than_silently_unrepresentable() {
        let pitch = minimum_representable_wall_mm().expect("the frozen pitch must publish");
        assert_eq!(pitch.get(), 0.5, "the frozen voxel pitch moved");

        // Cavity selection is `edt > wall` on a lattice quantised at the pitch,
        // so every one of these picks the identical cavity while asking the
        // evidence to record a different number. The settings seam refuses them
        // by name instead of producing a shell that does not match its record.
        for unrepresentable in [1e-9, 0.001, 0.25, 0.4, 0.499_999_999] {
            let request = HollowRequest {
                printer: registered_printer(),
                wall_mm: PositiveFinite::new(unrepresentable).expect("positive finite wall"),
            };
            let refusal = HollowSettings::from_request(&request);
            assert!(
                matches!(
                    refusal,
                    Err(HollowError::WallBelowVoxelPitch {
                        wall_mm,
                        voxel_pitch_mm,
                    }) if wall_mm == unrepresentable && voxel_pitch_mm == pitch.get()
                ),
                "wall {unrepresentable} mm must be refused by name, got {refusal:?}"
            );
            assert!(
                matches!(
                    checked_wall_mm(PositiveFinite::new(unrepresentable).unwrap()),
                    Err(HollowError::WallBelowVoxelPitch { .. })
                ),
                "the shared rule must refuse {unrepresentable} mm for every caller"
            );
        }

        // The pitch itself is the finest wall this stage can honestly claim, so
        // it is accepted — the bound is a floor, not a rounding.
        for representable in [pitch.get(), 0.500_000_001, 0.75, 2.5] {
            let request = HollowRequest {
                printer: registered_printer(),
                wall_mm: PositiveFinite::new(representable).expect("positive finite wall"),
            };
            assert_eq!(
                HollowSettings::from_request(&request)
                    .expect("a wall at or above the pitch is representable")
                    .wall_thickness_mm()
                    .get(),
                representable
            );
            assert_eq!(
                checked_wall_mm(PositiveFinite::new(representable).unwrap())
                    .expect("a wall at or above the pitch is representable")
                    .get(),
                representable
            );
        }

        // Both registered walls sit above the floor; neither option can be
        // refused by the rule that guards the operator's own values.
        for selection in [WallSelection::Default, WallSelection::HandlingHeavy] {
            let registered = registered_wall(selection).expect("registered wall");
            assert!(registered.get() >= pitch.get());
            assert!(checked_wall_mm(registered).is_ok());
        }
    }

    #[test]
    fn the_requested_wall_reaches_the_extraction_and_is_recorded_as_cavity_evidence() {
        let shell = |request: &HollowRequest| {
            create_shell(
                frozen_mesh("Sphere.prepped.oriented-solid.stl"),
                HollowSettings::from_request(request).expect("request settings"),
            )
            .expect("the frozen Sphere is hollow-eligible at either registered wall")
        };
        let default = shell(
            &HollowRequest::with_registered_wall_default(registered_printer())
                .expect("registered wall default"),
        );
        let heavy = shell(
            &HollowRequest::with_registered_handling_heavy_wall(registered_printer())
                .expect("registered handling-heavy wall"),
        );

        assert_eq!(default.cavity().wall_thickness_mm().get(), 2.5);
        assert_eq!(heavy.cavity().wall_thickness_mm().get(), 3.0);
        assert!(
            heavy.cavity().cavity_volume_mm3().get() < default.cavity().cavity_volume_mm3().get(),
            "a heavier wall must remove less material, not merely be recorded"
        );
        assert_eq!(
            heavy.cavity().source_volume_mm3().get(),
            default.cavity().source_volume_mm3().get(),
            "the outer solid is untouched by the wall selection"
        );
    }

    #[test]
    fn preserves_authored_extra_bodies_while_hollowing_only_the_largest_cavity() {
        let main = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let detail = make_box([2.0, 2.0, 2.0], [14.0, 0.0, 1.0]).expect("detail fixture");
        let candidate = create_shell(join(&main, &detail), registered_settings())
            .expect("extra authored body is retained");
        let topology =
            build_face_topology(candidate.mesh(), WeldPolicy::ExactBits).expect("output topology");
        let certificate =
            certify_closed_components(candidate.mesh(), &topology).expect("output certificate");
        assert_eq!(
            certificate.components().len(),
            3,
            "outer body, authored detail, and one cavity remain"
        );
    }

    #[test]
    fn voxel_admission_has_exact_boundary_and_refuses_300mm_before_voxelization() {
        assert!(matches!(
            H0AdmissionLimits::new(
                TriageTopologyLimits::new(1, 1, 1, 3, 3, 32),
                1,
                QUERY_BYTES_PER_POINT,
                1,
                BooleanLimits::UNLIMITED,
                1,
            ),
            Err(HollowError::InvalidAdmissionLimits)
        ));
        let huge = make_box([300.0, 300.0, 300.0], [0.0, 0.0, 0.0]).expect("300mm box");
        let error = create_shell(huge, registered_settings())
            .expect_err("300mm at 0.5mm must preflight-refuse");
        assert!(matches!(
            error,
            HollowError::VoxelCellLimitExceeded { cells, max_cells }
                if cells == 601_usize.pow(3) && max_cells == DEFAULT_MAX_VOXEL_CELLS
        ));

        let input = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let source = triage_exact_with_measured_limits(&input)
            .expect("sphere triage")
            .into_mesh();
        let spec = grid_spec(&source, settings.voxel_pitch_mm().get()).expect("grid spec");
        let mut exact = H0AdmissionLimits::production();
        exact.max_voxel_cells = spec.count;
        exact.query_working_bytes = QUERY_BYTES_PER_POINT * 127;
        let admitted = create_shell_with_admission_limits(input.clone(), settings, exact)
            .expect("exact voxel-cell cap");
        assert_eq!(admitted.cavity().resources().query_batch_points(), 127);

        let mut below = exact;
        below.max_voxel_cells = spec.count - 1;
        assert!(matches!(
            create_shell_with_admission_limits(input, settings, below),
            Err(HollowError::VoxelCellLimitExceeded { cells, max_cells })
                if cells == spec.count && max_cells == spec.count - 1
        ));
        assert!(matches!(
            grid_spec(&source, f64::MIN_POSITIVE),
            Err(HollowError::VoxelGridTooLarge)
        ));
    }

    #[test]
    fn limited_boolean_has_exact_input_output_boundaries_and_preserves_shell_bits() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let baseline = create_shell_with_admission_limits(
            source.clone(),
            settings,
            H0AdmissionLimits::production(),
        )
        .expect("baseline limited shell");
        let diagnostics = baseline.cavity().resources();
        let output_vertices = u64::try_from(baseline.mesh().verts().len()).expect("vertices fit");
        let output_triangles = u64::try_from(baseline.mesh().tris().len()).expect("triangles fit");

        let mut exact = H0AdmissionLimits::production();
        exact.max_boolean_input_primitive_units = diagnostics.boolean_input_primitive_units();
        exact.max_boolean_marshaled_input_bytes = diagnostics.boolean_marshaled_input_bytes();
        exact.max_boolean_output_vertices = output_vertices;
        exact.max_boolean_output_triangles = output_triangles;
        let admitted = create_shell_with_admission_limits(source.clone(), settings, exact)
            .expect("exact public Boolean limits");
        assert_eq!(
            mesh_fingerprint(admitted.mesh()).expect("admitted digest"),
            mesh_fingerprint(baseline.mesh()).expect("baseline digest")
        );
        assert_eq!(admitted.cavity_field(), baseline.cavity_field());
        let topology =
            build_face_topology(admitted.mesh(), WeldPolicy::ExactBits).expect("admitted topology");
        let certificate = certify_closed_components(admitted.mesh(), &topology)
            .expect("admitted winding certificate");
        assert_eq!(
            certificate
                .components()
                .iter()
                .filter(|component| component.signed_volume_mm3() < 0.0)
                .count(),
            1
        );

        let mut below_input = exact;
        below_input.max_boolean_input_primitive_units =
            diagnostics.boolean_input_primitive_units() - 1;
        assert!(matches!(
            create_shell_with_admission_limits(source.clone(), settings, below_input),
            Err(HollowError::Solid(SolidError::InputLimitExceeded {
                actual,
                maximum,
                ..
            })) if actual == diagnostics.boolean_input_primitive_units()
                && maximum == diagnostics.boolean_input_primitive_units() - 1
        ));

        let mut below_marshaled = exact;
        below_marshaled.max_boolean_marshaled_input_bytes =
            diagnostics.boolean_marshaled_input_bytes() - 1;
        assert!(matches!(
            create_shell_with_admission_limits(source.clone(), settings, below_marshaled),
            Err(HollowError::Solid(SolidError::InputLimitExceeded {
                actual,
                maximum,
                ..
            })) if actual == diagnostics.boolean_marshaled_input_bytes()
                && maximum == diagnostics.boolean_marshaled_input_bytes() - 1
        ));

        let mut below_output = exact;
        below_output.max_boolean_output_vertices = output_vertices - 1;
        assert!(matches!(
            create_shell_with_admission_limits(source.clone(), settings, below_output),
            Err(HollowError::Solid(SolidError::OutputLimitExceeded {
                actual,
                maximum,
                ..
            })) if actual == output_vertices && maximum == output_vertices - 1
        ));

        let mut below_triangles = exact;
        below_triangles.max_boolean_output_triangles = output_triangles - 1;
        assert!(matches!(
            create_shell_with_admission_limits(source, settings, below_triangles),
            Err(HollowError::Solid(SolidError::OutputLimitExceeded {
                actual,
                maximum,
                ..
            })) if actual == output_triangles && maximum == output_triangles - 1
        ));
    }

    #[test]
    fn boolean_volume_invariant_accepts_shell_and_rejects_wrong_cavity_evidence() {
        let candidate = create_shell(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            registered_settings(),
        )
        .expect("frozen sphere shell");
        let evidence = *candidate.cavity();
        validate_shell_topology(
            candidate.mesh(),
            evidence.source_volume_mm3().get(),
            evidence.cavity_volume_mm3().get(),
            0.02,
        )
        .expect("measured Boolean balance");
        assert!(matches!(
            validate_shell_topology(
                candidate.mesh(),
                evidence.source_volume_mm3().get(),
                0.0,
                0.02,
            ),
            Err(HollowError::BooleanVolumeMismatch { .. })
        ));
    }

    fn thread_guard() -> &'static Mutex<()> {
        static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        GUARD.get_or_init(|| Mutex::new(()))
    }

    struct RestoreThreads(Option<std::ffi::OsString>);

    impl Drop for RestoreThreads {
        fn drop(&mut self) {
            if let Some(value) = self.0.take() {
                std::env::set_var("PLINTH_THREADS", value);
            } else {
                std::env::remove_var("PLINTH_THREADS");
            }
        }
    }

    #[test]
    fn shell_digest_is_bit_identical_across_one_four_and_sixteen_kernel_workers() {
        let _guard = thread_guard().lock().expect("thread environment lock");
        let _restore = RestoreThreads(std::env::var_os("PLINTH_THREADS"));
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let mut baseline: Option<String> = None;
        let mut chunked_limits = H0AdmissionLimits::production();
        chunked_limits.query_working_bytes = QUERY_BYTES_PER_POINT * 1024;
        for workers in [1, 4, 16] {
            std::env::set_var("PLINTH_THREADS", workers.to_string());
            assert_eq!(geom_kernel::current_threads(), workers);
            let candidate =
                create_shell_with_admission_limits(source.clone(), settings, chunked_limits)
                    .expect("thread-count shell");
            let digest = mesh_fingerprint(candidate.mesh()).expect("shell fingerprint");
            if let Some(ref expected) = baseline {
                assert_eq!(
                    digest.as_str(),
                    expected.as_str(),
                    "worker count {workers} changed the shell"
                );
            } else {
                baseline = Some(digest);
            }
        }
        std::env::set_var("PLINTH_THREADS", "1");
        for query_points in [127, 4096] {
            let mut query_limits = H0AdmissionLimits::production();
            query_limits.query_working_bytes = QUERY_BYTES_PER_POINT * query_points;
            let candidate =
                create_shell_with_admission_limits(source.clone(), settings, query_limits)
                    .expect("chunk-size shell");
            let digest = mesh_fingerprint(candidate.mesh()).expect("chunk fingerprint");
            assert_eq!(digest, *baseline.as_ref().expect("worker baseline"));
            assert_eq!(
                candidate.cavity().resources().query_batch_points(),
                query_points
            );
        }
    }
}

/// The well-composedness repair, end to end through `create_shell`: once on a
/// small synthetic solid that a default `cargo test` runs, and once on the
/// committed corpus fixture that made FU-49 necessary.
///
/// **Why this exists as its own module.** Every other test of the repair works
/// on a synthetic *mask* a dozen voxels across, and the unit tests stop at the
/// mask or at `marching_cavity`: without these two, nothing anywhere ran
/// `create_shell` on a solid that formerly refused with `InvalidShellTopology`,
/// so a regression in the marching or Boolean half of the causal chain — the
/// half the repair exists to satisfy — would have passed unnoticed. FU-53's
/// companion finding is the proof that this failure class is real and lives
/// beside the repaired one: a synthetic finned block refuses with **zero**
/// critical cells and a perfectly manifold cavity.
///
/// **Both halves of the corpus/synthetic pair are load-bearing.** The corpus
/// row is release-gated because the fixture is 429,120 faces, so on its own it
/// leaves the default `cargo test` with *no* end-to-end coverage of the fix at
/// all — the two debug-runnable `create_shell` tests elsewhere in this file run
/// masks with zero critical cells, in which the repair is inert. The synthetic
/// dumbbell below closes that hole at a cost of well under a second.
///
/// It also makes the journal's numbers reproducible. The 2026-08-11 record
/// stated a repair measurement no committed input could produce; the row is now
/// this test's assertions, so the record and the engine cannot drift apart
/// again without a failure that names the number that moved.
#[cfg(test)]
mod corpus_cavity_repair {
    use super::*;
    use plinth_mesh::load_binary_stl;

    /// A UV sphere, `stacks` latitude bands by `slices` longitude bands.
    fn sphere(radius: f64, center: [f64; 3], stacks: usize, slices: usize) -> Mesh {
        let mut verts = vec![[center[0], center[1], center[2] + radius]];
        for stack in 1..stacks {
            let phi = std::f64::consts::PI * (stack as f64) / (stacks as f64);
            for slice in 0..slices {
                let theta = 2.0 * std::f64::consts::PI * (slice as f64) / (slices as f64);
                verts.push([
                    center[0] + radius * phi.sin() * theta.cos(),
                    center[1] + radius * phi.sin() * theta.sin(),
                    center[2] + radius * phi.cos(),
                ]);
            }
        }
        verts.push([center[0], center[1], center[2] - radius]);
        let south = (verts.len() - 1) as u32;
        let ring = |stack: usize, slice: usize| -> u32 {
            (1 + (stack - 1) * slices + (slice % slices)) as u32
        };
        let mut tris = Vec::new();
        for slice in 0..slices {
            tris.push([0, ring(1, slice), ring(1, slice + 1)]);
        }
        for stack in 1..stacks - 1 {
            for slice in 0..slices {
                tris.push([
                    ring(stack, slice),
                    ring(stack + 1, slice),
                    ring(stack + 1, slice + 1),
                ]);
                tris.push([
                    ring(stack, slice),
                    ring(stack + 1, slice + 1),
                    ring(stack, slice + 1),
                ]);
            }
        }
        for slice in 0..slices {
            tris.push([south, ring(stacks - 1, slice + 1), ring(stacks - 1, slice)]);
        }
        Mesh::new(verts, tris).expect("closed sphere fixture")
    }

    /// A capped cylinder along `+x` from `0` to `length`, yawed about `z`.
    fn rod(radius: f64, length: f64, yaw: f64, sides: usize) -> Mesh {
        let mut verts = Vec::new();
        for side in 0..sides {
            let theta = 2.0 * std::f64::consts::PI * (side as f64) / (sides as f64);
            verts.push([0.0, radius * theta.cos(), radius * theta.sin()]);
            verts.push([length, radius * theta.cos(), radius * theta.sin()]);
        }
        let low_center = verts.len() as u32;
        verts.push([0.0, 0.0, 0.0]);
        let high_center = verts.len() as u32;
        verts.push([length, 0.0, 0.0]);
        let mut tris = Vec::new();
        for side in 0..sides {
            let next = (side + 1) % sides;
            let (a, b) = ((2 * side) as u32, (2 * side + 1) as u32);
            let (c, d) = ((2 * next) as u32, (2 * next + 1) as u32);
            tris.push([a, c, d]);
            tris.push([a, d, b]);
            tris.push([low_center, c, a]);
            tris.push([high_center, b, d]);
        }
        let (sin, cos) = yaw.sin_cos();
        let verts = verts
            .into_iter()
            .map(|vertex| {
                [
                    vertex[0] * cos - vertex[1] * sin,
                    vertex[0] * sin + vertex[1] * cos,
                    vertex[2],
                ]
            })
            .collect::<Vec<_>>();
        Mesh::new(verts, tris).expect("closed rod fixture")
    }

    /// **The synthetic solid whose cavity mask is not well-composed.**
    ///
    /// Two 10 mm-radius lobes joined by a 3.375 mm-radius waist running at 45°
    /// in the xy plane. The waist is the whole point: at a 2.5 mm wall its
    /// cavity is a *sub-voxel* thread, so the selected mask crosses the waist as
    /// a chain of lattice-diagonal voxels — which is precisely Latecki's C1/C2
    /// configuration, the one a one-vertex-per-cell extractor cannot represent.
    ///
    /// Every number is exact in binary (the waist is 27/8) so the lattice
    /// decides the same way on any target. The diagonal is what makes it work:
    /// an axis-aligned waist produces a thread that is thin but well-composed.
    fn pinched_dumbbell() -> Mesh {
        let yaw = std::f64::consts::FRAC_PI_4;
        let span = 40.0;
        let far = [span * yaw.cos(), span * yaw.sin(), 0.0];
        let parts = [
            sphere(10.0, [0.0, 0.0, 0.0], 32, 48),
            sphere(10.0, far, 32, 48),
            rod(3.375, span, yaw, 48),
        ];
        let operands = parts
            .iter()
            .map(to_solid)
            .collect::<Result<Vec<_>, _>>()
            .expect("fixture operands");
        let welded = ManifoldBackend
            .union_limited(&operands, BooleanLimits::UNLIMITED)
            .expect("fixture union");
        let mesh = from_solid(welded).expect("fixture mesh");
        // Into the positive octant, as every oriented solid reaching H0 is.
        let min = mesh.verts().iter().fold([f64::MAX; 3], |low, vertex| {
            [
                low[0].min(vertex[0]),
                low[1].min(vertex[1]),
                low[2].min(vertex[2]),
            ]
        });
        Mesh::new(
            mesh.verts()
                .iter()
                .map(|vertex| [vertex[0] - min[0], vertex[1] - min[1], vertex[2] - min[2]])
                .collect(),
            mesh.tris().to_vec(),
        )
        .expect("fixture in the positive octant")
    }

    /// The selected cavity component **before** any repair, plus the grid it
    /// lives on: `create_shell`'s own selection, stopped one line early.
    fn unrepaired_cavity_mask(
        source: &Mesh,
        settings: HollowSettings,
    ) -> (Array3<bool>, VoxelGrid) {
        let spec = grid_spec(source, settings.voxel_pitch_mm().get()).expect("grid spec");
        let work = raster_work_item_count(
            source,
            spec,
            H0AdmissionLimits::production().max_raster_work_items(),
        )
        .expect("bounded source work");
        let grid = voxelize_filled(source, spec, work).expect("bounded voxelization");
        let edt = frozen_distance_transform(&grid.material, grid.pitch).expect("EDT");
        let cavity = edt.mapv(|distance| distance > settings.wall_thickness_mm().get());
        let (labels, label_count) = label3d(cavity.view(), 6).expect("6-connected labels");
        assert!(label_count > 0, "the fixture must select a cavity");
        let largest = largest_label(&labels, label_count).expect("largest label");
        (labels.mapv(|label| label == largest), grid)
    }

    /// `(boundary edges, non-manifold faces, point-only faces)`.
    fn cut_census(mesh: &Mesh) -> (usize, usize, usize) {
        let topology =
            build_face_topology(mesh, WeldPolicy::ExactBits).expect("shell topology is buildable");
        let count = |flag: u8| {
            topology
                .cut_flags()
                .iter()
                .filter(|flags| **flags & flag != 0)
                .count()
        };
        (
            topology.counts().boundary_edges,
            count(plinth_mesh::TOPOLOGY_CUT_NONMANIFOLD),
            count(plinth_mesh::TOPOLOGY_CUT_POINT_ONLY),
        )
    }

    /// **The default `cargo test`'s end-to-end gate on the repair.**
    ///
    /// It asserts the whole chain in the order the chain runs: the mask is not
    /// well-composed; the extractor *provably cannot* represent that mask (the
    /// unrepaired cavity surface is non-manifold, which is the refusal FU-49
    /// filed); the repair removes exactly the voxels the record states; and the
    /// shell that comes out of the marching, the Boolean and
    /// `validate_shell_topology` carries no cut face of any kind.
    ///
    /// The middle step is what makes this a regression test rather than a
    /// tautology: without it, an extractor change that made the repair
    /// unnecessary would leave this test passing while measuring nothing.
    #[test]
    fn a_synthetic_pinched_dumbbell_needs_the_repair_and_hollows_because_of_it() {
        let source = pinched_dumbbell();
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");

        // 1. The mask this fixture exists to produce, and the extractor's
        //    failure on it. The cavity thread across the 45° waist is a chain
        //    of lattice-diagonal voxels, so the one-vertex-per-cell surface has
        //    edges used by four faces instead of two.
        let (unrepaired, grid) = unrepaired_cavity_mask(&source, settings);
        let raw = marching_cavity(&unrepaired, grid.origin, grid.pitch)
            .expect("the unrepaired mask still extracts a closed surface");
        assert_eq!(
            cut_census(&raw),
            (0, 8, 26),
            "the fixture no longer exhibits the defect the repair exists to fix: \
             watertight but non-manifold is the whole premise"
        );

        // 2. The repair, through the production entry point. Integer
        //    combinatorics over a deterministic voxel field, so these are
        //    equalities.
        let candidate = create_shell(source, settings).expect("the repaired dumbbell hollows");
        let resources = candidate.cavity().resources();
        assert_eq!(resources.cavity_critical_cells(), 4);
        assert_eq!(resources.cavity_repaired_voxels(), 4);
        // The opening cell scan plus exactly one whole-grid re-selection: the
        // removal stranded part of the component, which is the second
        // `O(cells)` domain the cell-visit bound does not cover.
        assert_eq!(resources.cavity_repair_full_grid_passes(), 2);
        // The 98 x 98 x 41 grid's own `(n+1)³` opening scan plus eight cells per
        // changed voxel, arrived at rather than remembered.
        assert_eq!(resources.voxel_count(), 393_764);
        let cells = 99 * 99 * 42;
        assert_eq!(
            resources.cavity_repair_cell_visits(),
            cells + 8 * resources.cavity_repaired_voxels()
        );
        assert_eq!(resources.cavity_repair_cell_visits(), 411_674);

        // 3. The end of the chain. Not one face of the shell or its cavity is
        //    cut, and the difference is one negative void inside positive
        //    material — the two things the unrepaired mask could not deliver.
        assert_eq!(
            cut_census(candidate.mesh()),
            (0, 0, 0),
            "the repaired shell is cut; the repair no longer closes FU-49"
        );
        let topology =
            build_face_topology(candidate.mesh(), WeldPolicy::ExactBits).expect("shell topology");
        let certificate =
            certify_closed_components(candidate.mesh(), &topology).expect("shell certificate");
        assert_eq!(
            certificate
                .components()
                .iter()
                .filter(|component| component.signed_volume_mm3() < 0.0)
                .count(),
            1,
            "the shell must hold exactly one negative, contained cavity"
        );
    }

    /// Release-gated: the fixture is 429,120 faces and the run is seconds
    /// optimized, minutes without optimization. `cargo test --release` runs it;
    /// a debug `cargo test` reports it ignored rather than silently skipping it.
    #[test]
    #[cfg_attr(
        debug_assertions,
        ignore = "429,120-face corpus fixture; run under cargo test --release"
    )]
    fn the_committed_treant_hollows_and_its_repair_is_the_one_the_record_states() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/Weeping Treant v1.2.prepped.oriented-solid.stl");
        assert!(
            path.is_file(),
            "RequiredCorpusFixtureMissing: {} is committed evidence and must be present",
            path.display()
        );
        let source = load_binary_stl(&path, WeldPolicy::None).expect("committed frozen STL");
        assert_eq!(source.tris().len(), 429_120, "the fixture itself moved");

        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        let candidate = create_shell(source, settings)
            .expect("the committed treant is the model FU-49 was filed on and it must now hollow");

        // 1. A repair actually ran on this input. Without this the rest of the
        //    test would still pass on a mask that never needed anything, and it
        //    would be pinning the wrong thing.
        let resources = candidate.cavity().resources();
        assert!(resources.cavity_critical_cells() > 0);
        assert!(resources.cavity_repaired_voxels() > 0);

        // 2. The exact measurement, which is what the journal row states. These
        //    are integer combinatorics over a deterministic voxel field, so
        //    they are equalities, not tolerances.
        assert_eq!(resources.cavity_critical_cells(), 70);
        assert_eq!(resources.cavity_repaired_voxels(), 115);
        assert_eq!(resources.cavity_repair_cell_visits(), 5_405_880);
        // The opening cell scan and exactly one whole-grid re-selection. This
        // is the second `O(cells)` domain the cell-visit bound does not cover,
        // and it is here so that a change which turned the fixpoint into many
        // whole-grid passes would be caught by the count rather than by a
        // stopwatch.
        assert_eq!(resources.cavity_repair_full_grid_passes(), 2);

        // 3. The work bound the module doc claims, checked against the grid the
        //    caller admitted rather than against a remembered number.
        assert_eq!(resources.voxel_count(), 5_313_000);
        let cells = 166 * 176 * 185;
        assert!(
            resources.cavity_repair_cell_visits() <= cells + 8 * resources.cavity_repaired_voxels(),
            "cell visits {} exceeded cells + 8 x changed",
            resources.cavity_repair_cell_visits()
        );

        // 4. The cavity that survived, and the shell built from it. 8,043.875
        //    mm3 is 64,351 voxels at the frozen 0.5 mm pitch — the 64,466 the
        //    wall selected, less the 115 the repair handed back.
        let cavity_mm3 = candidate.cavity().cavity_volume_mm3().get();
        assert_eq!(cavity_mm3, 8_043.875);
        let pitch = candidate.cavity().frozen_voxel_pitch_mm().get();
        assert_eq!((cavity_mm3 / pitch.powi(3)) as usize, 64_351);

        // 5. **The end of the causal chain FU-49 filed.** The shell carries the
        //    outer surface and the cavity together, and not one face of either
        //    is cut. Before the repair the inner surface alone carried 132
        //    non-manifold and 483 point-only faces, the Boolean inherited them,
        //    and `create_shell` refused its own shell.
        assert_eq!(
            cut_census(candidate.mesh()),
            (0, 0, 0),
            "the shell or its cavity is cut again; the repair no longer closes FU-49"
        );

        // 6. The diagnostics record is populated by a real run, which nothing
        //    else asserts: every field a shell of this size must have made
        //    nonzero is nonzero.
        assert!(resources.query_batch_points() > 0);
        assert!(resources.raster_work_items() > 0);
        assert!(resources.retained_raw_source_buffer_bytes() > 0);
        assert!(resources.retained_cavity_membership_bytes() > 0);
        assert!(resources.retained_source_occupancy_bytes() > 0);
        assert!(resources.marching_vertex_upper_bound() > 0);
        assert!(resources.marching_triangle_upper_bound() > 0);
        assert!(resources.boolean_input_primitive_units() > 0);
        assert!(resources.boolean_marshaled_input_bytes() > 0);
        assert!(resources.boolean_output_vertex_limit() > 0);
        assert!(resources.boolean_output_triangle_limit() > 0);
    }
}
