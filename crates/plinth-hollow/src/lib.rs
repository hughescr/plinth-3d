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
use geom_kernel::{NativeSurface, NativeSurfaceConfig};
use plinth_core::{m0_constants, Finite, M0ConstantsError, PositiveFinite, PrinterProfile};
use plinth_mesh::{
    build_face_topology, certify_closed_components, triage_settings, validate_face_topology, Mesh,
    StrictMeshAuditError, TopologyError, TopologyValidationError, TriageAdmissionError,
    TriageError, TriageTopologyAdmission, TriageTopologyLimits, TriagedMesh, WeldPolicy,
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
/// in-process, on the returned model. FU-37 carries that same requested wall
/// onto the serialized [`HollowReport::wall`], so a caller reading a written
/// artifact can see what wall built it without holding the in-process model.
/// [`HollowRequest::with_registered_wall_default`] is the ordinary
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
    /// FU-55. A shell was built and its cavity survived every other gate, but
    /// its realized minimum wall — measured directly against the source
    /// surface — came in strictly below the wall the caller stated. The
    /// shell is discarded and the oriented solid ships instead: a hollow
    /// refusal, never a model rejection (2026-08-12 owner ruling). Distinct
    /// from [`Self::NoCavityAfterRepair`] / [`Self::BelowThresholdAfterRepair`]:
    /// those are findings about the cavity's *volume*, this one is a finding
    /// about the shell's own *geometry* — the wall it actually cut, not the
    /// wall it was asked to leave. [`HollowReport::realized_wall_mm`] carries
    /// the number.
    ShellUnderWalled,
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
    /// FU-37: the wall the caller *requested*, carried onto every report a
    /// production path builds. This is what makes two artifacts prepared at
    /// 2.5 mm and 3.0 mm distinguishable in metadata — before this field,
    /// [`CavityEvidence::wall_thickness_mm`] recorded the same number
    /// in-process only and died with the process. `Option` rather than
    /// required: [`Self::solid_decision`] is a public hand-built constructor
    /// (used by `plinth-emit` and `plinth-support` test fixtures) with no
    /// wall to state, and `#[serde(default)]` for the same reason
    /// `cavity_repair` is — a report frozen before this field existed has to
    /// keep deserializing. Populated on every production path: the hollowed
    /// path ([`drilling::into_hollowed_model`]), every typed
    /// [`SolidDecisionReason`] arm `run` converts a failure into (via
    /// [`HollowedModel::solid_decision_recording_repair`]), and the internal
    /// support refusal substitution ([`HollowedWithSolidReserve::
    /// into_solid_fallback`], carried from the discarded shell's own report).
    ///
    /// Distinct from [`Self::realized_wall_mm`] (FU-53/FU-55, *realized*):
    /// this field states what the caller asked for, that one states what the
    /// shell actually measured. The 2026-08-12 owner ruling names this field
    /// the natural future home for the realized figure recorded beside it,
    /// without scheduling that as work of its own.
    #[serde(default)]
    wall: Option<Finite>,
    /// FU-55: the realized minimum wall this report's shell measured, when
    /// one was measured. Populated on the hollowed path (by
    /// [`drilling::into_hollowed_model`], from [`CavityEvidence::
    /// realized_wall_mm`]) and on the [`SolidDecisionReason::
    /// ShellUnderWalled`] fallback (by `run`, from the refusal's own
    /// measurement). `None` on every other report: a hand-built
    /// [`Self::solid_decision`], and every solid decision that never reached
    /// cavity selection or whose shell was discarded for an unrelated
    /// reason. Distinct from `wall` (FU-37, requested) — see that field's
    /// own doc.
    ///
    /// `default` for the same reason `cavity_repair` is: a report frozen
    /// before this field existed has to keep deserializing.
    #[serde(default)]
    realized_wall_mm: Option<Finite>,
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
            wall: None,
            realized_wall_mm: None,
        }
    }

    /// The repair measurement behind this report, when one ran.
    #[must_use]
    pub const fn cavity_repair(&self) -> Option<CavityRepairEvidence> {
        self.cavity_repair
    }

    /// FU-37: the wall the caller requested, when this report was built by a
    /// production path. See the field's own doc for exactly which reports
    /// carry it and how it differs from [`Self::realized_wall_mm`].
    #[must_use]
    pub const fn wall(&self) -> Option<Finite> {
        self.wall
    }

    /// FU-55: the realized minimum wall this report's shell measured, when
    /// one was measured. See the field's own doc for exactly which reports
    /// carry it.
    #[must_use]
    pub const fn realized_wall_mm(&self) -> Option<Finite> {
        self.realized_wall_mm
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
        wall: Option<Finite>,
    ) -> Result<Self, HollowError> {
        validate_oriented_solid(&mesh)?;
        let report = HollowReport {
            disposition,
            cavities: Vec::new(),
            drains: Vec::new(),
            drilling_evidence: None,
            drain_plan: None,
            cavity_repair,
            wall,
            // FU-55: `run`'s `ShellUnderWalled` arm attaches its measurement
            // after this constructor returns, via `with_realized_wall_mm` —
            // every other solid decision (including this function's other
            // two callers) has no shell to have measured.
            realized_wall_mm: None,
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
        Self::solid(
            mesh,
            orientation,
            HollowDisposition::Solid { reason },
            None,
            None,
        )
    }

    /// A solid decision recorded together with whatever the well-composedness
    /// repair measured on the shell candidate that was abandoned, and the
    /// wall that was requested of it.
    ///
    /// The evidence is the point: without it a caller reading the record can
    /// see *that* the model went solid but not whether a repair decided it, nor
    /// how much cavity the repair handed back. `None` is a claim in its own
    /// right — nothing reached cavity selection — and is why the parameter is
    /// an `Option` rather than two constructors. `wall` (FU-37) is `Some` on
    /// every production caller reached from `run` — see [`HollowReport::wall`]
    /// for the full list — and `None` only where this constructor is used as a
    /// hand-built test stand-in with no request behind it.
    pub fn solid_decision_recording_repair(
        mesh: Mesh,
        orientation: OrientationHistory,
        reason: SolidDecisionReason,
        cavity_repair: Option<CavityRepairEvidence>,
        wall: Option<Finite>,
    ) -> Result<Self, HollowError> {
        Self::solid(
            mesh,
            orientation,
            HollowDisposition::Solid { reason },
            cavity_repair,
            wall,
        )
    }

    /// FU-55: attaches the discarded shell's realized-wall measurement to a
    /// just-built solid decision. Crate-private — the only caller is `run`'s
    /// `ShellUnderWalled` arm, immediately after
    /// [`Self::solid_decision_recording_repair`] returns; every other solid
    /// decision has no shell measurement to attach.
    #[must_use]
    fn with_realized_wall_mm(mut self, realized_wall_mm: Option<Finite>) -> Self {
        self.history.hollow.realized_wall_mm = realized_wall_mm;
        self
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
        // makes no claim about its own realized wall — but the record this
        // substitution produces is the one a reader uses to decide how this
        // *model* was treated, and "a repair ran and then the shell was
        // refused" is not the same finding as "no repair was ever attempted".
        // So the shell's repair measurement is carried onto the substitution
        // rather than dropped with the shell — and so is the *requested* wall
        // (FU-37): the reserve was built to the same request the discarded
        // shell was, so the wall that was asked for is exactly as true of the
        // substitution as it was of the shell.
        let cavity_repair = self.hollowed.history().hollow().cavity_repair();
        let wall = self.hollowed.history().hollow().wall();
        HollowedModel::solid(
            self.reserve_mesh,
            self.reserve_history,
            HollowDisposition::SolidFallback { reason },
            cavity_repair,
            wall,
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
            return convert_shell_failure_to_solid(
                solid,
                orientation,
                failure,
                settings.wall_thickness_mm(),
            )
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

/// `run`'s own failure-to-solid-decision seam, factored out so it is
/// directly testable against a hand-built [`HollowStageFailure`] without
/// needing geometry that actually reproduces the failure — most of these
/// are cavity-selection defects that a correct engine (FU-53's own proof, for
/// [`HollowError::RealizedWallBelowSpec`]) makes structurally rare or
/// impossible to reach honestly through the full pipeline.
///
/// Read the measurement off the failure *before* the error is consumed, so
/// every arm below — the typed decisions and the bare propagation alike —
/// hands on the same evidence. This is the seam the first version of this
/// change lost it at: five errors became typed outputs and everything else
/// propagated naked, and the record a naked propagation produces is the one
/// that decides how the model is treated.
///
/// `requested_wall_mm` (FU-37) is `settings.wall_thickness_mm()` from the
/// same request that built the shell candidate this failure came from — it
/// is in scope at every one of `run`'s typed decisions, so every one of them
/// records what wall built it rather than leaving the report blank.
fn convert_shell_failure_to_solid(
    solid: Mesh,
    orientation: OrientationHistory,
    failure: HollowStageFailure,
    requested_wall_mm: Finite,
) -> Result<HollowStageOutput, HollowStageFailure> {
    let cavity_repair = failure.cavity_repair();
    let decided = |reason: SolidDecisionReason, realized_wall_mm: Option<Finite>| {
        HollowedModel::solid_decision_recording_repair(
            solid,
            orientation,
            reason,
            cavity_repair,
            Some(requested_wall_mm),
        )
        .map(|model| {
            HollowStageOutput::Solid(Box::new(model.with_realized_wall_mm(realized_wall_mm)))
        })
        .map_err(HollowStageFailure::from)
    };
    match failure.into_error() {
        HollowError::IneligibleNoCavity => decided(SolidDecisionReason::NoCavity, None),
        HollowError::IneligibleBelowBulk | HollowError::IneligibleSavings => {
            decided(SolidDecisionReason::BelowThreshold, None)
        }
        // The two repair-caused decisions. They travel as their own
        // reasons because "the repair removed the cavity" and "this
        // shape has no cavity" are different findings and the record
        // has to be able to tell them apart; the numbers themselves
        // come from the same `cavity_repair` every other arm uses.
        HollowError::CavityRepairRemovedCavity { .. } => {
            decided(SolidDecisionReason::NoCavityAfterRepair, None)
        }
        HollowError::IneligibleSavingsAfterCavityRepair { .. } => {
            decided(SolidDecisionReason::BelowThresholdAfterRepair, None)
        }
        // FU-55, the ruling this whole function exists to carry: a shell
        // that measured under spec is discarded, never a model failure, and
        // the measurement that condemned it travels onto the solid decision
        // that replaces it.
        HollowError::RealizedWallBelowSpec {
            realized_wall_mm, ..
        } => match finite(realized_wall_mm) {
            Ok(value) => decided(SolidDecisionReason::ShellUnderWalled, Some(value)),
            Err(error) => Err(HollowStageFailure {
                error,
                cavity_repair,
            }),
        },
        error => Err(HollowStageFailure {
            error,
            cavity_repair,
        }),
    }
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
    /// FU-53: `HOLLOW_CAVITY_MARCH_MARGIN_MM`, registered as one voxel body
    /// diagonal (`HOLLOW_PITCH_MM * sqrt(3)`). Added to the stated wall before
    /// cavity selection so the realised wall is provably >= the requested one;
    /// see [`create_shell_recording_repair`] and the module doc lemma above
    /// [`frozen_distance_transform`].
    cavity_march_margin_mm: Finite,
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
            placeholders
                .hollow_cavity_march_margin_mm()
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

    #[must_use]
    pub const fn cavity_march_margin_mm(self) -> Finite {
        self.cavity_march_margin_mm
    }
}

#[allow(clippy::too_many_arguments)]
fn finite_settings(
    wall_thickness_mm: f64,
    voxel_pitch_mm: f64,
    bulk_min_mm: f64,
    save_min_mm3: f64,
    save_min_fraction: f64,
    volume_balance_rel_tol: f64,
    cavity_march_margin_mm: f64,
) -> Result<HollowSettings, HollowError> {
    let finite = |value| Finite::new(value).map_err(|_| HollowError::InvalidArithmetic);
    Ok(HollowSettings {
        wall_thickness_mm: finite(wall_thickness_mm)?,
        voxel_pitch_mm: finite(voxel_pitch_mm)?,
        bulk_min_mm: finite(bulk_min_mm)?,
        save_min_mm3: finite(save_min_mm3)?,
        save_min_fraction: finite(save_min_fraction)?,
        volume_balance_rel_tol: finite(volume_balance_rel_tol)?,
        cavity_march_margin_mm: finite(cavity_march_margin_mm)?,
    })
}

const DEFAULT_DRAIN_PLANNING_BYTES: usize = 1024 * 1024 * 1024;
const DEFAULT_QUERY_WORKING_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_MAX_RASTER_WORK_ITEMS: usize = 16 * 1024 * 1024;
/// FU-53: the caller-owned ceiling on the BVH + parity-index memory estimate
/// [`geom_kernel::NativeSurface::estimate_index_memory_from_indexed`] reports
/// for the exact cavity-distance surface built over the triaged source mesh.
/// This is a new, independent H0 admission domain (drain planning and
/// drilling build their own `NativeSurface`s downstream of H0 and are not
/// affected). 512 MiB comfortably covers the largest committed corpus source
/// (the 429,120-face Weeping Treant) with headroom to spare, matching the
/// order of magnitude of the other named-work admission defaults above.
const DEFAULT_CAVITY_DISTANCE_INDEX_BYTES: usize = 512 * 1024 * 1024;
/// FU-54: the caller-owned ceiling on
/// [`well_composed::WellComposedRepair::full_grid_passes`] — the
/// well-composedness repair's whole-grid re-selection loop, previously
/// unbounded (`p <= 2 + dropped`, `dropped` up to `O(cells)`). A round that
/// would exceed this bounces off
/// [`HollowError::CavityRepairPassLimitExceeded`] instead of taking the next
/// re-selection.
///
/// **Provenance: this number is the owner's, not derived from anything.**
/// Decided 2026-08-12
/// (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
/// §7). Measured real models use 1-2 passes: 2 on the committed Weeping
/// Treant v1.2 fixture, the raw Warhorse, the raw Giant Toad, the raw
/// Awakened Tree and the synthetic pinched dumbbell; 1 on every
/// zero-critical-cell mask. 8 is 4x headroom over the worst measured case —
/// a ceiling that is never meant to bite real geometry, not a budget to grow
/// into. If a real model ever reaches it, the correct response is to
/// investigate the mask, never to raise this number.
const DEFAULT_MAX_CAVITY_REPAIR_PASSES: usize = 8;
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
    /// FU-53: the peak-bytes ceiling for the exact cavity-distance
    /// `NativeSurface` built over the triaged source mesh. A new, independent
    /// H0 admission domain — see [`DEFAULT_CAVITY_DISTANCE_INDEX_BYTES`].
    max_cavity_distance_index_bytes: usize,
    /// FU-54: the ceiling on the well-composedness repair's whole-grid
    /// re-selection passes. A new, independent H0 admission domain — see
    /// [`DEFAULT_MAX_CAVITY_REPAIR_PASSES`].
    max_cavity_repair_passes: usize,
}

impl H0AdmissionLimits {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        triage: TriageTopologyLimits,
        max_voxel_cells: usize,
        query_working_bytes: usize,
        max_raster_work_items: usize,
        boolean: BooleanLimits,
        drain_planning_bytes: usize,
        max_cavity_distance_index_bytes: usize,
        max_cavity_repair_passes: usize,
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
            || max_cavity_distance_index_bytes == 0
            || max_cavity_distance_index_bytes == usize::MAX
            || max_cavity_repair_passes == 0
            || max_cavity_repair_passes == usize::MAX
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
            max_cavity_distance_index_bytes,
            max_cavity_repair_passes,
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
            max_cavity_distance_index_bytes: DEFAULT_CAVITY_DISTANCE_INDEX_BYTES,
            max_cavity_repair_passes: DEFAULT_MAX_CAVITY_REPAIR_PASSES,
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

    #[must_use]
    pub const fn max_cavity_distance_index_bytes(self) -> usize {
        self.max_cavity_distance_index_bytes
    }

    /// FU-54: the ceiling on the well-composedness repair's whole-grid
    /// re-selection passes. Production default 8 — see
    /// [`DEFAULT_MAX_CAVITY_REPAIR_PASSES`] for the provenance.
    #[must_use]
    pub const fn max_cavity_repair_passes(self) -> usize {
        self.max_cavity_repair_passes
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
    /// FU-53: the measured peak-bytes estimate for the exact cavity-distance
    /// `NativeSurface`, admitted against
    /// [`H0AdmissionLimits::max_cavity_distance_index_bytes`]. Zero when no
    /// candidate voxel needed an exact query (the EDT prefilter alone already
    /// settled every voxel's cavity membership).
    cavity_distance_index_bytes: usize,
    /// FU-53: the number of voxel centres the EDT prefilter handed to the
    /// exact BVH nearest-triangle query — the count the module doc's lemma
    /// bounds well below the grid's total voxel count on every measured
    /// model. A blow-up toward `voxel_count` is the visible signature of a
    /// pathological candidate set the design's risk section names.
    cavity_distance_exact_query_points: usize,
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
    #[must_use]
    pub const fn cavity_distance_index_bytes(self) -> usize {
        self.cavity_distance_index_bytes
    }
    #[must_use]
    pub const fn cavity_distance_exact_query_points(self) -> usize {
        self.cavity_distance_exact_query_points
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
    /// it), so `p` passes are `2p − 1` traversals. FU-54 bounds `p` at
    /// [`H0AdmissionLimits::max_cavity_repair_passes`] (production default 8):
    /// a round that would exceed it is refused
    /// ([`HollowError::CavityRepairPassLimitExceeded`]) rather than taken, so
    /// a value read off a shell that was actually built never exceeds the
    /// limit it ran under.
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
    /// FU-55: the realized minimum wall, measured directly against the
    /// source surface before this shell was allowed to reach the Boolean
    /// difference. Present only because this `CavityEvidence` exists at
    /// all — a shell whose realized wall came in under spec is refused
    /// before a `CavityEvidence` is ever built (see
    /// [`HollowError::RealizedWallBelowSpec`]); the in-process authority
    /// behind [`HollowReport::realized_wall_mm`] on the hollowed path.
    realized_wall_mm: Finite,
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

    /// FU-55: the realized minimum wall this shell measured, `>=`
    /// [`Self::wall_thickness_mm`] by construction — [`create_shell`] never
    /// returns a `CavityEvidence` for which that would be false.
    #[must_use]
    pub const fn realized_wall_mm(self) -> Finite {
        self.realized_wall_mm
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
    // FU-53. `edt` is a SOUND PREFILTER, never the selection authority: the
    // module doc's lemma above `frozen_distance_transform` proves
    // `edt(v) >= d_true(v)` for every material voxel, so `{d_true > t}` is a
    // subset of `{edt > t}` for every threshold `t`. The exact distance only
    // has to be evaluated where the prefilter cannot already rule a voxel
    // out — the candidate set below — and it is sufficient to cover both
    // decisions this field feeds (cavity selection against `wall + margin`,
    // and the bulk gate against `bulk_min / 2`) with the smaller of the two
    // thresholds, which is the more inclusive (superset) filter of the two.
    let wall_plus_margin_mm = settings.wall_thickness_mm.get() + settings.cavity_march_margin_mm.get();
    let candidate_threshold_mm = wall_plus_margin_mm.min(settings.bulk_min_mm.get() / 2.0);
    if !candidate_threshold_mm.is_finite() {
        return Err(HollowError::InvalidArithmetic.into());
    }
    let (edt, cavity_distance_index_bytes, cavity_distance_exact_query_points, distance_surface) =
        exact_cavity_distance(
            &source,
            edt,
            grid_origin,
            grid_pitch,
            candidate_threshold_mm,
            limits,
        )?;
    // Honest bulk figure: `edt` is now exact everywhere a voxel could
    // plausibly be the true interior maximum (every voxel with
    // `edt > candidate_threshold_mm`, which by construction covers every
    // voxel with `edt > bulk_min / 2`), so its whole-grid maximum is the true
    // maximum wherever it matters to this gate, and a safe upper bound
    // (bounded by `candidate_threshold_mm`, which cannot flip the gate)
    // everywhere else.
    let bulk_mm = 2.0 * edt.iter().copied().fold(0.0_f64, f64::max);
    if !bulk_mm.is_finite() {
        return Err(HollowError::InvalidArithmetic.into());
    }
    if bulk_mm < settings.bulk_min_mm.get() {
        return Err(HollowError::IneligibleBelowBulk.into());
    }
    // Selection: `material[v] && d_true(v) > wall + margin`. The lemma above
    // ensures this can never select a voxel whose TRUE distance to the
    // source surface is `<= wall + margin`, so the realised wall is provably
    // `>=` the requested wall — see the margin's registry provenance
    // (`HOLLOW_CAVITY_MARCH_MARGIN_MM`) for the surface-nets half-cell-offset
    // argument that sizes it.
    let cavity_mask = edt.mapv(|distance_mm| distance_mm > wall_plus_margin_mm);
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
    let (main, well_composed_repair) = match well_composed::well_composed_cavity_component(
        selected,
        limits.max_cavity_repair_passes(),
    ) {
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
        // FU-54: the re-selection loop hit its admitted pass ceiling. Same
        // lift-onto-the-failure treatment as the variant above, using `passes`
        // (the pass count at refusal) as the evidence's `full_grid_passes` --
        // the same vocabulary every other repair-caused refusal uses, so a
        // caller reading `cavity_repair` cannot tell this apart from a repair
        // that converged, only from the error itself.
        Err(HollowError::CavityRepairPassLimitExceeded {
            passes,
            max_passes,
            critical_cells,
            repaired_voxels,
            cell_visits,
        }) => {
            let evidence = CavityRepairEvidence {
                critical_cells,
                repaired_voxels,
                cell_visits,
                full_grid_passes: passes,
            };
            return Err(HollowStageFailure::after_cavity_repair(
                HollowError::CavityRepairPassLimitExceeded {
                    passes,
                    max_passes,
                    critical_cells,
                    repaired_voxels,
                    cell_visits,
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
        // FU-55. Never ship an under-walled shell: measure the realized
        // minimum wall against the SOURCE surface here, before this shell
        // pays for a Boolean difference at all. `distance_surface` is the
        // exact `NativeSurface` already built above for cavity selection —
        // reused, not rebuilt, guaranteed `Some` because a selected cavity
        // (checked already, above) can only exist if at least one candidate
        // voxel entered `exact_cavity_distance`'s candidate set, which is
        // exactly the condition that builds it.
        //
        // The query bound is `2 x (wall + margin)`: the extracted surface
        // sits on the boundary of the mask selected at `wall + margin`, so
        // its true distance is expected within about one voxel of that
        // threshold; doubling it is headroom for what the well-composedness
        // repair may have moved, not a tolerance on the check itself — the
        // check below compares the raw measurement, never the clamped one.
        let distance_surface = distance_surface.ok_or(HollowError::Kernel)?;
        let realized_wall_query_bound_mm = 2.0 * wall_plus_margin_mm;
        let realized_wall_query_batch_points =
            (limits.query_working_bytes() / QUERY_BYTES_PER_POINT).max(1);
        let raw_realized_wall_mm = measure_realized_wall_mm(
            &distance_surface,
            &inner,
            realized_wall_query_bound_mm,
            realized_wall_query_batch_points,
        );
        let realized_wall_mm = checked_realized_wall_mm(
            raw_realized_wall_mm,
            settings.wall_thickness_mm.get(),
            realized_wall_query_bound_mm,
        )?;
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
            cavity_distance_index_bytes,
            cavity_distance_exact_query_points,
        };
        let cavity = CavityEvidence {
            wall_thickness_mm: settings.wall_thickness_mm,
            frozen_voxel_pitch_mm: settings.voxel_pitch_mm,
            cavity_volume_mm3: finite(cavity_volume_mm3)?,
            source_volume_mm3: finite(source_volume_mm3)?,
            resources,
            drain_planning_cap_bytes: limits.drain_planning_bytes,
            realized_wall_mm,
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
/// # Errors
///
/// [`HollowError::StrictAudit`] for a repeated index or a zero-area face,
/// [`HollowError::Topology`] / [`HollowError::TopologyValidation`] when the
/// topology cannot be built or does not validate, and
/// [`HollowError::NotClosedOrOriented`] when any face carries any cut flag.
pub fn validate_closed_mesh(mesh: &Mesh) -> Result<(), HollowError> {
    mesh.audit_strict().map_err(HollowError::StrictAudit)?;
    let topology =
        build_face_topology(mesh, WeldPolicy::ExactBits).map_err(HollowError::Topology)?;
    validate_face_topology(&topology).map_err(HollowError::TopologyValidation)?;
    if topology.cut_flags().iter().any(|flags| *flags != 0) {
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

fn raster_work_item_count(
    mesh: &Mesh,
    spec: GridSpec,
    max_work_items: usize,
) -> Result<usize, HollowError> {
    let mut count = 0_usize;
    // The verdict is never asked for here: the count is candidate enumeration
    // alone (see `walk_raster_candidates`), so this pass does not pay the
    // 13-axis separating-axis test that the mark pass pays.
    walk_raster_candidates(mesh, spec, |_candidate| {
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
    // Area-proportional conservative rasterizer (FU-50). See
    // docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md
    // §3 ("the frozen rasterizer is replaced, and the replacement is proven
    // first"). This is no longer a reproduction of trimesh's `subdivide`
    // voxelizer: that freeze was lifted by the ruling above, which the FU-50
    // journal row and the delta table recorded before this path shipped. A
    // voxel is marked when its axis-aligned cube (centred on the voxel, half
    // extent one pitch/2) exactly overlaps a source triangle, decided by the
    // 13-axis separating-axis test (`triangle_cube_overlaps`); candidate
    // cells are enumerated by clipping each triangle to the cell slabs of its
    // own dilated index-space bounding box, one axis at a time, so that the
    // cells offered to the exact test track the triangle rather than its
    // bounding box (`walk_triangle_candidates`). `binary_fill_holes`'s
    // 6-connected exterior flood (`fill_voxel_holes`) is unchanged.
    let mut occupancy = vec![false; spec.count];
    let mut observed_work_items = 0_usize;
    walk_raster_candidates(mesh, spec, |candidate| {
        observed_work_items = observed_work_items
            .checked_add(1)
            .ok_or(HollowError::ResourceAccountingOverflow)?;
        if candidate.overlaps() {
            mark_voxel_index(candidate.cell(), spec, &mut occupancy)?;
        }
        Ok(())
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

/// One candidate cell offered to a `walk_raster_candidates` visitor, carrying
/// its overlap verdict **unevaluated**.
///
/// The count pass (`raster_work_item_count`) never calls [`Self::overlaps`],
/// because the work-item count is candidate enumeration alone and candidate
/// enumeration does not consult the verdict. Handing the verdict over as a
/// value would therefore make every voxelization pay the 13-axis
/// separating-axis test twice per candidate — once in a count pass that
/// discards it, once in the mark pass that uses it — which is exactly the
/// waste this type exists to remove. The two-pass `RasterWorkMismatch`
/// contract is untouched: the candidate set still depends only on
/// `(mesh, spec)`.
#[derive(Clone, Copy)]
struct RasterCandidate<'tri> {
    tri: &'tri [[f64; 3]; 3],
    cell: [usize; 3],
}

impl RasterCandidate<'_> {
    const fn cell(self) -> [usize; 3] {
        self.cell
    }

    /// The exact 13-axis separating-axis verdict for this candidate, costed
    /// only when a caller asks for it.
    fn overlaps(self) -> bool {
        let center = [
            self.cell[0] as f64,
            self.cell[1] as f64,
            self.cell[2] as f64,
        ];
        triangle_cube_overlaps(*self.tri, center, [0.5, 0.5, 0.5])
    }
}

/// WORK ITEM = one candidate voxel cell tested against one triangle (not one
/// cell marked). Both callers (`raster_work_item_count`, `voxelize_filled`)
/// call `visit` once per candidate exactly, so the two passes agree on the
/// count by construction — candidate generation depends only on `mesh` and
/// `spec`, never on `visit` and never on the overlap verdict, which is why
/// the verdict travels unevaluated in [`RasterCandidate`].
fn walk_raster_candidates(
    mesh: &Mesh,
    spec: GridSpec,
    mut visit: impl FnMut(RasterCandidate<'_>) -> Result<(), HollowError>,
) -> Result<(), HollowError> {
    for triangle in mesh.tris() {
        let [ia, ib, ic] = *triangle;
        let point = |index: u32| -> Result<[f64; 3], HollowError> {
            let world = mesh
                .verts()
                .get(usize::try_from(index).map_err(|_| HollowError::VoxelGridTooLarge)?)
                .copied()
                .ok_or(HollowError::VoxelGridTooLarge)?;
            to_index_space(world, spec)
        };
        let tri = [point(ia)?, point(ib)?, point(ic)?];
        walk_triangle_candidates(tri, spec, &mut visit)?;
    }
    Ok(())
}

fn to_index_space(point: [f64; 3], spec: GridSpec) -> Result<[f64; 3], HollowError> {
    let mut out = [0.0; 3];
    for axis in 0..3 {
        let value = (point[axis] - spec.origin[axis]) / spec.pitch;
        if !value.is_finite() {
            return Err(HollowError::VoxelGridTooLarge);
        }
        out[axis] = value;
    }
    Ok(out)
}

/// Dilated per-axis index-space bounding box of `tri`, clamped to `spec`'s
/// voxel index range: `[lo, hi]` inclusive on each axis, or `lo > hi` on an
/// axis whose dilated extent misses the grid entirely.
fn dilated_index_aabb(
    tri: [[f64; 3]; 3],
    spec: GridSpec,
) -> Result<[[isize; 2]; 3], HollowError> {
    let mut axis_range = [[0_isize; 2]; 3];
    for axis in 0..3 {
        let lo = tri[0][axis].min(tri[1][axis]).min(tri[2][axis]) - 0.5;
        let hi = tri[0][axis].max(tri[1][axis]).max(tri[2][axis]) + 0.5;
        if !lo.is_finite() || !hi.is_finite() {
            return Err(HollowError::VoxelGridTooLarge);
        }
        let dim_max = isize::try_from(spec.dims[axis].saturating_sub(1))
            .map_err(|_| HollowError::VoxelGridTooLarge)?;
        let lo_index = (lo.floor() as isize).max(0);
        let hi_index = (hi.ceil() as isize).min(dim_max);
        axis_range[axis] = [lo_index, hi_index];
    }
    Ok(axis_range)
}

/// Relative index-space slack every clip slab and every clipped extent is
/// widened by before it becomes an inclusive cell-index range.
///
/// **This is not a tolerance on any decision.** The overlap decision is the
/// exact separating-axis test, unchanged; this only widens *candidate*
/// generation, so it can add a candidate the exact test then rejects for
/// free, and can never drop one. It is owed because the clipped extents come
/// out of floating-point segment/plane intersections while the exact test
/// reads the original triangle: a triangle edge that grazes a cell boundary
/// exactly (`x + y == 210` touching the corner of the cell above it) rounds
/// to a hair inside, and without this the walk drops that cell — measured, on
/// a flat fan-base triangle, before the slack existed.
///
/// Relative, not absolute: index-space coordinates run to a few hundred on an
/// admitted grid (300 mm printable extent over a 0.5 mm pitch), where an f64
/// ulp is ~1e-13. One part in 1e9 is four orders above the rounding those
/// intersections can produce and nine orders below one voxel.
const CANDIDATE_EXTENT_SLACK: f64 = 1e-9;

/// [`CANDIDATE_EXTENT_SLACK`] at the magnitude a value actually sits at,
/// floored at one voxel so a coordinate near zero still gets slack.
fn extent_slack(magnitude: f64) -> f64 {
    CANDIDATE_EXTENT_SLACK * magnitude.abs().max(1.0)
}

/// Vertices the per-column clip buffer holds.
///
/// A triangle (3) clipped by one half-space keeps at most its inside
/// vertices plus one point per boundary crossing; a 3-cycle has an even
/// number of crossings, so at most 2, giving at most 4. A second half-space
/// on that 4-cycle gives at most 4 + 4 = 8 by the same parity argument, which
/// is the bound for the two half-spaces of one slab. Eight is therefore
/// sufficient, not a guess — and [`ColumnPolygon::clip_half`] still returns
/// `None` rather than dropping a vertex if it were ever exceeded, which the
/// caller answers by keeping the *wider* unclipped polygon.
const MAX_COLUMN_VERTICES: usize = 8;

/// A triangle clipped to one index-space slab: the closed point cycle
/// bounding the convex set `triangle ∩ slab`, in order.
#[derive(Clone, Copy)]
struct ColumnPolygon {
    points: [[f64; 3]; MAX_COLUMN_VERTICES],
    len: usize,
}

impl ColumnPolygon {
    fn triangle(tri: &[[f64; 3]; 3]) -> Self {
        let mut points = [[0.0_f64; 3]; MAX_COLUMN_VERTICES];
        points[..3].copy_from_slice(tri);
        Self { points, len: 3 }
    }

    fn push(&mut self, point: [f64; 3]) -> bool {
        if self.len == MAX_COLUMN_VERTICES {
            return false;
        }
        self.points[self.len] = point;
        self.len += 1;
        true
    }

    /// Sutherland–Hodgman against one axis-aligned half-space. `None` means
    /// the buffer or the arithmetic could not represent the result; the
    /// caller keeps the unclipped polygon, which is conservative.
    fn clip_half(&self, axis: usize, bound: f64, keep_above: bool) -> Option<Self> {
        let mut out = Self {
            points: [[0.0_f64; 3]; MAX_COLUMN_VERTICES],
            len: 0,
        };
        if self.len == 0 {
            return Some(out);
        }
        let inside = |point: &[f64; 3]| {
            if keep_above {
                point[axis] >= bound
            } else {
                point[axis] <= bound
            }
        };
        for index in 0..self.len {
            let current = self.points[index];
            let next = self.points[(index + 1) % self.len];
            let current_inside = inside(&current);
            if current_inside && !out.push(current) {
                return None;
            }
            if current_inside == inside(&next) {
                continue;
            }
            // The endpoints sit on opposite sides of `bound`, so their `axis`
            // coordinates differ and this denominator is non-zero.
            let ratio = ((bound - current[axis]) / (next[axis] - current[axis])).clamp(0.0, 1.0);
            let mut crossing = [0.0_f64; 3];
            for coordinate in 0..3 {
                crossing[coordinate] =
                    current[coordinate] + ratio * (next[coordinate] - current[coordinate]);
            }
            if !crossing.iter().all(|value| value.is_finite()) {
                return None;
            }
            if !out.push(crossing) {
                return None;
            }
        }
        Some(out)
    }

    /// `self ∩ {lo <= x[axis] <= hi}`, or `self` itself when the clip could
    /// not be represented (see [`Self::clip_half`]).
    fn clipped_to_slab(&self, axis: usize, lo: f64, hi: f64) -> Self {
        self.clip_half(axis, lo, true)
            .and_then(|above| above.clip_half(axis, hi, false))
            .unwrap_or(*self)
    }

    /// Inclusive `[min, max]` of `out_axis` over the part of this polygon
    /// whose `slab_axis` coordinate lies in `[lo, hi]`, or `None` when that
    /// part is empty.
    ///
    /// The polygon bounds a convex set, so an extremum of a linear coordinate
    /// over its intersection with a slab is attained on its boundary cycle —
    /// which is what this walks, one segment at a time, without materializing
    /// a second clipped polygon.
    fn slab_extent(
        &self,
        slab_axis: usize,
        lo: f64,
        hi: f64,
        out_axis: usize,
    ) -> Option<(f64, f64)> {
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for index in 0..self.len {
            let start = self.points[index];
            let end = self.points[(index + 1) % self.len];
            let (mut first, mut last) = (0.0_f64, 1.0_f64);
            let delta = end[slab_axis] - start[slab_axis];
            if delta == 0.0 {
                if start[slab_axis] < lo || start[slab_axis] > hi {
                    continue;
                }
            } else {
                let to_lo = (lo - start[slab_axis]) / delta;
                let to_hi = (hi - start[slab_axis]) / delta;
                let (near, far) = if delta > 0.0 {
                    (to_lo, to_hi)
                } else {
                    (to_hi, to_lo)
                };
                first = first.max(near);
                last = last.min(far);
                if first > last {
                    continue;
                }
            }
            let span = end[out_axis] - start[out_axis];
            for ratio in [first, last] {
                let value = start[out_axis] + ratio * span;
                if !value.is_finite() {
                    return None;
                }
                low = low.min(value);
                high = high.max(value);
            }
        }
        (low <= high).then_some((low, high))
    }

    /// Inclusive `[min, max]` of `axis` over the whole polygon, or `None`
    /// when it is empty.
    fn extent(&self, axis: usize) -> Option<(f64, f64)> {
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for point in &self.points[..self.len] {
            if !point[axis].is_finite() {
                return None;
            }
            low = low.min(point[axis]);
            high = high.max(point[axis]);
        }
        (low <= high).then_some((low, high))
    }
}

/// The inclusive cell-index range on `axis` whose cubes can reach
/// `[low, high]`, clamped to the triangle's already-dilated grid range.
///
/// Cell `n` covers `[n - 0.5, n + 0.5]`, so it can meet `[low, high]` only
/// when `n >= low - 0.5` and `n <= high + 0.5`.
fn candidate_index_span(low: f64, high: f64, clamp: [isize; 2]) -> Option<[isize; 2]> {
    let slack = extent_slack(low.abs().max(high.abs()));
    let first = (low - 0.5 - slack).ceil();
    let last = (high + 0.5 + slack).floor();
    if !first.is_finite() || !last.is_finite() {
        return None;
    }
    let first = (first as isize).max(clamp[0]);
    let last = (last as isize).min(clamp[1]);
    (first <= last).then_some([first, last])
}

/// The three axes ordered by descending clamped index span, ties by axis
/// index so the walk is deterministic.
///
/// The innermost axis is the one the triangle is *thinnest* along, which is
/// what keeps the inner span short for a triangle that lies edge-on to it.
fn candidate_axis_order(axis_range: [[isize; 2]; 3]) -> [usize; 3] {
    let mut order = [0_usize, 1, 2];
    let span = |axis: usize| axis_range[axis][1] - axis_range[axis][0];
    order.sort_by(|left, right| span(*right).cmp(&span(*left)).then(left.cmp(right)));
    order
}

/// Candidate cell enumeration for one triangle, in index space.
///
/// **Area-proportional, in every orientation (FU-50).** The walk clips the
/// triangle to one cell slab at a time instead of sweeping its projected
/// bounding box: for each cell column `i` on the outermost axis it takes
/// `triangle ∩ slab(i)`, reads the *triangle's own* extent there to bound the
/// middle axis, and for each cell `(i, j)` reads the triangle's extent inside
/// that column to bound the innermost axis. A long thin sliver therefore
/// costs work along its length in every direction, not the area of the
/// bounding box it happens to span — the diagonal case a projected-AABB sweep
/// pays `Θ(L²)` for while covering `Θ(L)` cells. Degenerate (zero-area,
/// collinear or coincident) triangles need no special case and get none:
/// clipping a degenerate triangle yields a degenerate polygon whose extents
/// are still exact, so the old dilated-AABB fallback — `Θ(L³)` for a
/// grid-spanning collinear triangle — is gone rather than merely narrowed.
///
/// **Completeness.** If cell `(i, j, k)` overlaps the triangle then some
/// triangle point `x` lies in its cube, so `x` is in `triangle ∩ slab(i)`,
/// making that clip non-empty; `x`'s middle coordinate lies within half a
/// cell of `j`, so `j` is inside the span read off that clip; and `x` also
/// lies in the `(i, j)` column, so `k` is inside the span read off it. Every
/// overlapping cell is therefore enumerated, and each is decided by the exact
/// test ([`RasterCandidate::overlaps`]) — over-generating candidates costs
/// work but can never mark a false positive, and the marked-voxel set is
/// exactly the set of cells whose cube truly overlaps a triangle, unchanged
/// by how tightly candidates are generated.
fn walk_triangle_candidates(
    tri: [[f64; 3]; 3],
    spec: GridSpec,
    visit: &mut impl FnMut(RasterCandidate<'_>) -> Result<(), HollowError>,
) -> Result<(), HollowError> {
    let axis_range = dilated_index_aabb(tri, spec)?;
    if axis_range.iter().any(|range| range[0] > range[1]) {
        return Ok(());
    }
    let [outer, middle, inner] = candidate_axis_order(axis_range);
    let whole = ColumnPolygon::triangle(&tri);

    for i in axis_range[outer][0]..=axis_range[outer][1] {
        // Both slabs are widened by `extent_slack` for the reason that
        // constant documents: a clipped slab narrower than the cell it stands
        // for can drop a triangle that grazes the cell's face exactly.
        let outer_slack = extent_slack(i as f64);
        let column = whole.clipped_to_slab(
            outer,
            i as f64 - 0.5 - outer_slack,
            i as f64 + 0.5 + outer_slack,
        );
        let Some((middle_low, middle_high)) = column.extent(middle) else {
            continue;
        };
        let Some(middle_span) = candidate_index_span(middle_low, middle_high, axis_range[middle])
        else {
            continue;
        };
        for j in middle_span[0]..=middle_span[1] {
            let middle_slack = extent_slack(j as f64);
            let Some((inner_low, inner_high)) = column.slab_extent(
                middle,
                j as f64 - 0.5 - middle_slack,
                j as f64 + 0.5 + middle_slack,
                inner,
            ) else {
                continue;
            };
            let Some(inner_span) = candidate_index_span(inner_low, inner_high, axis_range[inner])
            else {
                continue;
            };
            for k in inner_span[0]..=inner_span[1] {
                let mut cell = [0_usize; 3];
                cell[outer] = i as usize;
                cell[middle] = j as usize;
                cell[inner] = k as usize;
                visit(RasterCandidate { tri: &tri, cell })?;
            }
        }
    }
    Ok(())
}

fn sub3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn cross3(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

/// Exact triangle/axis-aligned-box overlap: the 13-axis separating-axis test
/// (Akenine-Moller 2001; Ericson, *Real-Time Collision Detection* §5.2.9).
/// `half` is the box's per-axis half-extent about `center`. Touching exactly
/// (a shared point, edge, or face) counts as overlap: every comparison is a
/// non-strict separation test, so nothing that only grazes the cube is
/// missed. Degenerate triangle edges or a degenerate triangle normal produce
/// a zero test axis, which trivially fails to separate rather than needing a
/// special case.
fn triangle_cube_overlaps(tri: [[f64; 3]; 3], center: [f64; 3], half: [f64; 3]) -> bool {
    let v = [
        sub3(tri[0], center),
        sub3(tri[1], center),
        sub3(tri[2], center),
    ];
    let e = [sub3(v[1], v[0]), sub3(v[2], v[1]), sub3(v[0], v[2])];

    for axis in 0..3 {
        let lo = v[0][axis].min(v[1][axis]).min(v[2][axis]);
        let hi = v[0][axis].max(v[1][axis]).max(v[2][axis]);
        if lo > half[axis] || hi < -half[axis] {
            return false;
        }
    }

    const BOX_AXES: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for box_axis in BOX_AXES {
        for edge in e {
            let axis = cross3(box_axis, edge);
            let p0 = dot3(axis, v[0]);
            let p1 = dot3(axis, v[1]);
            let p2 = dot3(axis, v[2]);
            let r = half[0] * axis[0].abs() + half[1] * axis[1].abs() + half[2] * axis[2].abs();
            let lo = p0.min(p1).min(p2);
            let hi = p0.max(p1).max(p2);
            if lo > r || hi < -r {
                return false;
            }
        }
    }

    let normal = cross3(e[0], e[1]);
    let d = dot3(normal, v[0]);
    let r =
        half[0] * normal[0].abs() + half[1] * normal[1].abs() + half[2] * normal[2].abs();
    if d > r || d < -r {
        return false;
    }

    true
}

fn mark_voxel_index(
    cell: [usize; 3],
    spec: GridSpec,
    occupancy: &mut [bool],
) -> Result<(), HollowError> {
    let plane = spec.dims[0]
        .checked_mul(spec.dims[1])
        .ok_or(HollowError::VoxelGridTooLarge)?;
    let flat = cell[0]
        .checked_add(
            cell[1]
                .checked_mul(spec.dims[0])
                .ok_or(HollowError::VoxelGridTooLarge)?,
        )
        .and_then(|value| value.checked_add(cell[2].checked_mul(plane)?))
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
///
/// # FU-53: the sound-prefilter lemma
///
/// This function's field is deliberately **not** the cavity-selection
/// authority — [`create_shell_recording_repair`] uses it only to shrink the
/// set of voxels [`exact_cavity_distance`] has to query exactly. The lemma
/// that makes that sound:
///
/// **`edt(v) >= d_true(v)` for every material voxel `v`**, where `d_true(v)`
/// is the exact Euclidean distance from `v`'s centre to the nearest point on
/// the source mesh's surface. Proof: the EDT measures to the nearest
/// *background voxel centre* in the conservatively rasterized occupancy, and
/// any straight segment from an interior sample to a background voxel centre
/// must cross the true surface at least once (the occupancy is conservative:
/// every voxel the surface passes through is marked material, so a
/// background voxel's centre lies strictly outside the true solid). A
/// straight segment is the shortest path between its endpoints, so the
/// segment's length — `edt(v)` — is at least the length of the shortest path
/// from `v` to the surface, which is `d_true(v)`.
///
/// Two consequences this crate relies on: (1) `{d_true > t}` is a subset of
/// `{edt > t}` for every threshold `t`, so filtering candidates on `edt`
/// before an exact query never discards a voxel the exact selection could
/// have chosen; (2) the largest `edt` value over any candidate set is a
/// sound single upper bound to pass as a batched nearest-query's
/// `upper_bound` argument, because no candidate's true distance can exceed
/// its own `edt` value. `frozen_oracle_edt_never_understates_the_exact_distance`
/// below asserts this mechanically against the real production rasterizer on
/// four fixtures (`Sphere`, `Cylinder`, `make_box`, and a synthetic thin fin).
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

/// FU-53: refines the frozen EDT prefilter into exact nearest-surface
/// distance wherever the prefilter cannot already settle a voxel's cavity
/// membership, by one batched BVH nearest-triangle query against `source`.
///
/// **Soundness, not merely speed.** Every value in the returned field
/// remains `>= d_true(v)` for every material voxel: untouched voxels keep
/// `frozen_distance_transform`'s own sound bound (the lemma on that
/// function's doc), and touched voxels hold the exact value, which trivially
/// satisfies `d_true(v) >= d_true(v)`. Selecting cavity on this field can
/// therefore never choose a voxel whose true distance to the source surface
/// is at or below the caller's threshold — the defect this function exists
/// to close.
///
/// **Candidate gathering, not a per-voxel call.** Every voxel whose EDT
/// exceeds `candidate_threshold_mm` is gathered into one flat point buffer
/// and queried in caller-sized batches (`query_working_bytes /
/// QUERY_BYTES_PER_POINT` points per call) through
/// [`geom_kernel::NativeSurface::query`], which is itself an index-parallel
/// `par_iter().map().collect()` (`rust/geom_kernel/src/queries.rs`) —
/// batch-first end to end, no per-voxel call anywhere. A single scalar
/// `upper_bound`, the largest EDT value among the candidates, bounds the
/// whole batch: by the lemma no candidate's true distance can exceed its own
/// EDT value, so the query can never truncate a real answer to the
/// out-of-bound sentinel.
///
/// Returns the refined field, the measured `NativeSurface` peak-bytes
/// estimate (0 when no candidate existed and no surface was built), the
/// exact-query point count (both for [`H0ResourceDiagnostics`]), and the
/// `NativeSurface` itself (`None` under the same no-candidate condition) —
/// FU-55's realized-wall check reuses this exact surface rather than
/// building a second BVH over the same `source`.
fn exact_cavity_distance(
    source: &Mesh,
    mut edt: Array3<f64>,
    origin: [f64; 3],
    pitch: f64,
    candidate_threshold_mm: f64,
    limits: H0AdmissionLimits,
) -> Result<(Array3<f64>, usize, usize, Option<NativeSurface>), HollowError> {
    let candidates: Vec<([usize; 3], [f64; 3])> = edt
        .indexed_iter()
        .filter(|(_, distance)| **distance > candidate_threshold_mm)
        .map(|((iz, iy, ix), _)| {
            (
                [iz, iy, ix],
                [
                    origin[0] + (ix as f64) * pitch,
                    origin[1] + (iy as f64) * pitch,
                    origin[2] + (iz as f64) * pitch,
                ],
            )
        })
        .collect();
    if candidates.is_empty() {
        // Every voxel is bounded by `candidate_threshold_mm`: none can be the
        // true grid maximum past that threshold and none can satisfy a
        // selection test against a strictly larger threshold either, so the
        // EDT prefilter alone already settles both decisions this field
        // feeds. No surface is built.
        return Ok((edt, 0, 0, None));
    }
    let estimate =
        NativeSurface::estimate_index_memory_from_indexed(source.verts(), source.tris(), pitch)
            .map_err(|_| HollowError::Kernel)?;
    if estimate.peak_bytes() > limits.max_cavity_distance_index_bytes() {
        return Err(HollowError::CavityDistanceIndexLimitExceeded {
            bytes: estimate.peak_bytes(),
            max_bytes: limits.max_cavity_distance_index_bytes(),
        });
    }
    let surface = NativeSurface::from_indexed(
        source.verts(),
        source.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: pitch,
            max_grid_bytes: estimate.max_axis_grid_bytes(),
        },
    )
    .map_err(|_| HollowError::Kernel)?;
    let upper_bound = candidates
        .iter()
        .fold(0.0_f64, |max, (voxel, _)| max.max(edt[*voxel]));
    let batch_size = (limits.query_working_bytes() / QUERY_BYTES_PER_POINT).max(1);
    for chunk in candidates.chunks(batch_size) {
        let points: Vec<[f64; 3]> = chunk.iter().map(|(_, point)| *point).collect();
        let (distances, _faces) = surface.query(&points, upper_bound);
        for ((voxel, _), distance) in chunk.iter().zip(distances) {
            // `.min` is a defensive clamp, not load-bearing: by the lemma the
            // exact distance can never exceed the prefilter's own value for
            // a material voxel, so this can only hold or shrink it.
            edt[*voxel] = edt[*voxel].min(distance);
        }
    }
    Ok((edt, estimate.peak_bytes(), candidates.len(), Some(surface)))
}

/// FU-55. The realized minimum wall of a cavity shell: the minimum, over
/// every vertex of the marching-cavity surface `inner`, of the exact
/// distance to `surface` (the SOURCE mesh's own `NativeSurface`, reused
/// rather than rebuilt). One batched, index-parallel `NativeSurface::query`
/// call per chunk of at most `batch_points` vertices; the reduction across
/// chunks and across each chunk's results is a SERIAL min-fold — the repo
/// forbids a float reduction across parallel work items.
///
/// **Vertex-sampled, not a continuous minimum.** A thin spot strictly
/// between two vertices of the extracted surface could in principle be
/// missed. This is the same methodology the engineering journal records for
/// this measurement, and the surface is dense at the frozen `HOLLOW_PITCH_MM`
/// voxel pitch; it is not a proof, and is documented as such rather than
/// claimed as one.
///
/// A vertex whose true distance exceeds `upper_bound_mm` comes back as
/// [`f64::INFINITY`] ([`NativeSurface::query`]'s own over-bound sentinel)
/// and can therefore never become the minimum; see the call site for how
/// `upper_bound_mm` is sized.
fn measure_realized_wall_mm(
    surface: &NativeSurface,
    inner: &Mesh,
    upper_bound_mm: f64,
    batch_points: usize,
) -> f64 {
    let batch_size = batch_points.max(1);
    let mut minimum = f64::INFINITY;
    for chunk in inner.verts().chunks(batch_size) {
        let (distances, _faces) = surface.query(chunk, upper_bound_mm);
        for distance in distances {
            if distance < minimum {
                minimum = distance;
            }
        }
    }
    minimum
}

/// FU-55. The one refusal rule: a realized wall strictly below the wall the
/// caller stated is never shipped. Strict `<`, **no epsilon** — an epsilon
/// here would itself be a new tolerance, and FU-53's own margin
/// (`HOLLOW_CAVITY_MARCH_MARGIN_MM`) already makes floating-point noise at
/// the boundary irrelevant. `raw_realized_wall_mm == requested_wall_mm`
/// passes.
///
/// On success, the value recorded as evidence is clamped to
/// `clamp_upper_bound_mm` — the same bound the measurement was queried
/// against — so that a minimum the query could only bound (an over-bound
/// vertex reads as [`f64::INFINITY`], which [`Finite`] cannot represent) is
/// still recorded as a truthful lower bound ("at least this much") rather
/// than failing to construct evidence for a shell that comfortably passed.
fn checked_realized_wall_mm(
    raw_realized_wall_mm: f64,
    requested_wall_mm: f64,
    clamp_upper_bound_mm: f64,
) -> Result<Finite, HollowError> {
    if raw_realized_wall_mm < requested_wall_mm {
        return Err(HollowError::RealizedWallBelowSpec {
            realized_wall_mm: raw_realized_wall_mm,
            requested_wall_mm,
        });
    }
    finite(raw_realized_wall_mm.min(clamp_upper_bound_mm))
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
    /// FU-53: the exact cavity-distance `NativeSurface`'s conservative memory
    /// estimate exceeded the caller's
    /// [`H0AdmissionLimits::max_cavity_distance_index_bytes`]. Refused before
    /// any BVH or parity-grid allocation, exactly like the sibling admission
    /// checks above; there is no fallback to the EDT-only selection this
    /// admission domain exists to move H0 off of.
    #[error(
        "the exact cavity-distance index estimate needs {bytes} bytes, exceeding the H0 limit {max_bytes}"
    )]
    CavityDistanceIndexLimitExceeded { bytes: usize, max_bytes: usize },
    #[error(
        "voxel rasterization exceeded the caller work cap {max_work_items} at item {work_items}"
    )]
    RasterWorkLimitExceeded {
        work_items: usize,
        max_work_items: usize,
    },
    #[error("the second raster pass disagreed with its preflight work count")]
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
    /// FU-54. The well-composedness repair's whole-grid re-selection loop
    /// hit its admitted ceiling ([`H0AdmissionLimits::max_cavity_repair_passes`],
    /// production default 8) before it converged. The check runs at the top
    /// of each re-selection round, strictly before that round's `label3d`
    /// call, so this replaces the traversal that would have exceeded the
    /// bound rather than following it. `passes` is the pass count at the
    /// moment of refusal (equal to `max_passes`); the other fields are the
    /// same repair evidence [`Self::CavityRepairRemovedCavity`] and
    /// [`Self::IneligibleSavingsAfterCavityRepair`] carry, because a partial
    /// repair still ran and the record must say what it did. This is an
    /// *added* bound (FU-8b: adding a bound is allowed, loosening one is
    /// not) — it does not relax the well-composedness repair, and it never
    /// substitutes a different route to an artifact: `plinth-pipeline`
    /// classifies it as a resource ceiling like every other admission
    /// refusal.
    #[error(
        "the well-composedness repair's re-selection loop reached {passes} whole-grid passes, \
         exceeding the H0 limit {max_passes}: {critical_cells} critical cells, \
         {repaired_voxels} voxels returned to material over {cell_visits} cell visits"
    )]
    CavityRepairPassLimitExceeded {
        passes: usize,
        max_passes: usize,
        critical_cells: usize,
        repaired_voxels: usize,
        cell_visits: usize,
    },
    /// FU-55. A shell was built, its cavity survived every other gate, and
    /// its realized minimum wall — measured directly against the source
    /// surface, on every vertex of the extracted cavity mesh — came in
    /// strictly below the wall the caller stated. Never propagated as a
    /// model failure: [`run`] converts this into
    /// [`SolidDecisionReason::ShellUnderWalled`] and the oriented solid
    /// ships instead, per the 2026-08-12 owner ruling on FU-55. The shell
    /// itself is discarded here, before it ever reaches the Boolean
    /// difference.
    #[error(
        "the realized minimum wall {realized_wall_mm} mm is below the requested {requested_wall_mm} mm"
    )]
    RealizedWallBelowSpec {
        realized_wall_mm: f64,
        requested_wall_mm: f64,
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
        //
        // Moved 16,491 -> 16,697 on 2026-08-12 by FU-50's area-proportional
        // conservative rasterizer replacing the frozen midpoint-quartering
        // one (decisions/2026-08-12-owner-rulings-fu-batch.md §3). The new
        // marked-voxel set is a strict superset of the old one (proved in
        // `frozen_walker_marks_a_subset_of_the_area_proportional_rasterizer`
        // below): every old leaf vertex lies inside the cube of the voxel it
        // rounds to, so the new exact SAT test marks that voxel too, and may
        // mark others the old sampled-vertex rounding missed. More marked
        // surface material reads the EDT slightly deeper, so occupancy above
        // the wall threshold grows and the cavity gains voxels — the
        // direction the ruling records as expected, not a wall improvement.
        assert_eq!(sphere.cavity_voxels, 16_697);

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
        // Moved 16,000-17,000 -> 12,800-13,100 mm3 by FU-53 (measured
        // 12,977.875 mm3): selection now runs on the exact nearest-surface
        // distance plus HOLLOW_CAVITY_MARCH_MARGIN_MM (2.5 + 0.866 mm) rather
        // than the EDT's conservative overestimate, so a 30 mm cube leaves an
        // inner cube of edge ~23.268 mm analytically (~12,597 mm3); the
        // measured figure is the voxel-lattice quantization of that edge.
        assert!(
            (12_800.0..=13_100.0).contains(&candidate.cavity().cavity_volume_mm3().get()),
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
    fn raster_preflight_has_exact_cap_and_stops_before_raster_allocation() {
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

        // FU-50: a triangle far larger than the grid no longer explodes the
        // old recursive-quartering depth limit (`4^ceil(log2(L_max/(pitch/2)))`,
        // independent of area). Candidate generation is bounded by the
        // admitted grid itself — for one triangle, at most one candidate per
        // grid cell — never by the triangle's edge length.
        let huge = Mesh::new(
            vec![[0.0, 0.0, 0.0], [300.0, 0.0, 0.0], [0.0, 300.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .expect("finite triangle");
        let huge_work = raster_work_item_count(&huge, spec, usize::MAX)
            .expect("area-proportional work stays bounded by the grid, not the edge length");
        assert!(
            huge_work <= spec.count,
            "one triangle cannot generate more candidates than the grid has cells: {huge_work}"
        );
    }

    /// FU-50's exact overlap primitive, against hand (and search-verified;
    /// see the case below) cases. Every comparison in `triangle_cube_overlaps`
    /// is non-strict, so a triangle that only grazes the cube must still mark
    /// it -- the face/corner/edge cases below are the boundary-inclusive
    /// claim, not just the interior one.
    #[test]
    fn triangle_cube_overlap_hand_computed_cases() {
        let half = [0.5, 0.5, 0.5];
        let center = [0.0, 0.0, 0.0];

        // Strictly inside.
        assert!(triangle_cube_overlaps(
            [[-0.1, -0.1, 0.0], [0.1, -0.1, 0.0], [0.0, 0.1, 0.0]],
            center,
            half,
        ));
        // Grazing exactly one face (z = 0.5).
        assert!(triangle_cube_overlaps(
            [[-0.1, -0.1, 0.5], [0.1, -0.1, 0.5], [0.0, 0.1, 0.5]],
            center,
            half,
        ));
        // Grazing exactly one corner (0.5, 0.5, 0.5).
        assert!(triangle_cube_overlaps(
            [[0.5, 0.5, 0.5], [2.0, 0.5, 0.5], [0.5, 2.0, 0.5]],
            center,
            half,
        ));
        // Grazing exactly one edge (x = y = 0.5).
        assert!(triangle_cube_overlaps(
            [[0.5, 0.5, -2.0], [0.5, 0.5, 2.0], [2.0, 2.0, 0.0]],
            center,
            half,
        ));
        // A triangle at constant x = 0.6: its own (undilated) AABB does not
        // even reach the cube, caught by the plain box-axis test.
        assert!(!triangle_cube_overlaps(
            [[0.6, -2.0, -2.0], [0.6, 2.0, -2.0], [0.6, 0.0, 2.0]],
            center,
            half,
        ));
        // A triangle whose per-axis AABB overlaps the cube on x, y and z
        // independently, but which does not actually touch it: only one of
        // the nine edge-cross-box-axis tests catches this (box x-axis cross
        // edge 1, verified by exhaustive random search over triangles whose
        // per-axis AABB overlaps the cube, then rounded to clean values and
        // re-verified). This is the case a plain per-axis AABB overlap check
        // would wrongly mark.
        let sliver: [[f64; 3]; 3] = [[-1.5, 2.0, 0.5], [1.0, 1.5, 0.0], [0.0, 0.5, 2.0]];
        for axis in 0..3 {
            let lo = sliver[0][axis].min(sliver[1][axis]).min(sliver[2][axis]);
            let hi = sliver[0][axis].max(sliver[1][axis]).max(sliver[2][axis]);
            assert!(
                lo <= half[axis] && hi >= -half[axis],
                "fixture must overlap the cube's AABB on every axis to be a meaningful case"
            );
        }
        assert!(!triangle_cube_overlaps(sliver, center, half));

        // A general (non-axis-aligned, non-unit) box half-extent still works.
        assert!(triangle_cube_overlaps(
            [[-0.2, -0.2, -0.2], [0.2, -0.2, -0.2], [0.0, 0.2, 0.0]],
            [0.0, 0.0, 0.0],
            [1.0, 2.0, 0.5],
        ));
    }

    fn mesh_surface_area_mm2(mesh: &Mesh) -> f64 {
        mesh.tris()
            .iter()
            .map(|triangle| {
                let [ia, ib, ic] = *triangle;
                let a = mesh.verts()[ia as usize];
                let b = mesh.verts()[ib as usize];
                let c = mesh.verts()[ic as usize];
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let cross = [
                    ab[1] * ac[2] - ab[2] * ac[1],
                    ab[2] * ac[0] - ab[0] * ac[2],
                    ab[0] * ac[1] - ab[1] * ac[0],
                ];
                0.5 * (cross[0].powi(2) + cross[1].powi(2) + cross[2].powi(2)).sqrt()
            })
            .sum()
    }

    /// The ruling's area-proportionality claim, pinned on the two smallest
    /// committed fixtures and checked against the corpus's own surface area
    /// rather than a remembered number.
    ///
    /// **Re-pinned when the slab-clipping walk replaced the projected-AABB
    /// sweep** (the FU-50 follow-up fix). Both numbers move DOWN, and only
    /// down: candidate generation got tighter, the marked-voxel set did not
    /// move at all (it is decided by the exact test, not by how many
    /// candidates reach it — `frozen_walker_marks_a_subset_of_the_area_
    /// proportional_rasterizer` and `the_candidate_walk_enumerates_every_cell_
    /// an_exhaustive_aabb_scan_marks` hold it in place), so no shell, cavity
    /// or artifact changes with these counts.
    ///
    /// - `Sphere.prepped.oriented-solid.stl`: 161,491 → 30,802 work items,
    ///   work/area-cells 32.2 → 6.14.
    /// - `make_box.prepped.oriented-solid.stl`: 5,292 → 3,012 work items,
    ///   work/area-cells 2.21 → 1.26.
    ///
    /// The per-fixture ratio bound is **tightened** with them, from a shared
    /// 50.0 to 8.0 and 2.0: a bound that no longer bites is not a bound.
    #[test]
    fn raster_work_profile_is_area_proportional_and_pinned() {
        let settings = registered_settings();
        let pitch = settings.voxel_pitch_mm().get();
        let cap = H0AdmissionLimits::production().max_raster_work_items();

        for (name, expected_work_items, max_ratio_to_area) in [
            ("Sphere.prepped.oriented-solid.stl", 30_802_usize, 8.0_f64),
            ("make_box.prepped.oriented-solid.stl", 3_012_usize, 2.0_f64),
        ] {
            let mesh = frozen_mesh(name);
            let spec = grid_spec(&mesh, pitch).expect("grid spec");
            let work =
                raster_work_item_count(&mesh, spec, cap).expect("bounded corpus fixture work");
            assert_eq!(work, expected_work_items, "{name}: raster_work_items moved");
            let area_cells = mesh_surface_area_mm2(&mesh) / pitch.powi(2);
            let ratio = work as f64 / area_cells;
            assert!(
                ratio < max_ratio_to_area,
                "{name}: work/area-cells ratio {ratio} is not the small constant the ruling claims"
            );
        }
    }

    /// The two-pass count/mark contract `RasterWorkMismatch` exists to guard:
    /// candidate generation depends only on `(mesh, spec)`, so the count pass
    /// and the mark pass must always agree, on every fixture.
    #[test]
    fn raster_count_and_mark_passes_agree_on_every_fixture() {
        let settings = registered_settings();
        let pitch = settings.voxel_pitch_mm().get();
        let cap = H0AdmissionLimits::production().max_raster_work_items();
        for name in [
            "Cylinder.prepped.oriented-solid.stl",
            "make_box.prepped.oriented-solid.stl",
            "make_overhang_table.prepped.stl",
            "make_raking_blade.prepped.stl",
            "make_thin_fin.prepped.stl",
            "make_toed_slab.prepped.stl",
            "Sphere.prepped.oriented-solid.stl",
        ] {
            let mesh = frozen_mesh(name);
            let spec = grid_spec(&mesh, pitch).expect("grid spec");
            let work = raster_work_item_count(&mesh, spec, cap).expect("bounded fixture work");
            voxelize_filled(&mesh, spec, work)
                .unwrap_or_else(|error| panic!("{name}: two-pass mismatch: {error}"));
        }
    }

    // -----------------------------------------------------------------
    // FU-50 reference oracle: the frozen midpoint-quartering rasterizer,
    // retained here `#[cfg(test)]`-only (decisions/2026-08-12-owner-rulings-
    // fu-batch.md §3) so the superset property stays mechanically checkable
    // against the *real* production `walk_raster_candidates`, now that the
    // frozen walker itself is gone from production. A second, independent
    // copy of both walkers lives in `examples/raster_delta.rs` for the
    // corpus-wide delta measurement recorded in the journal; this one exists
    // only to drive the test below.
    // -----------------------------------------------------------------

    const FROZEN_MAX_RASTER_SUBDIVISION_ROUNDS: u8 = 10;

    #[derive(Clone, Copy)]
    struct FrozenRasterNode {
        a: [f64; 3],
        b: [f64; 3],
        c: [f64; 3],
        depth: u8,
    }

    fn frozen_midpoint(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
        [
            (left[0] + right[0]) / 2.0,
            (left[1] + right[1]) / 2.0,
            (left[2] + right[2]) / 2.0,
        ]
    }

    fn frozen_edge_too_long(left: [f64; 3], right: [f64; 3], max_edge: f64) -> bool {
        left.iter()
            .zip(right)
            .map(|(left, right)| (left - right).powi(2))
            .sum::<f64>()
            .sqrt()
            > max_edge
    }

    fn frozen_raster_leaf(node: FrozenRasterNode, pitch: f64) -> bool {
        let max_edge = pitch / 2.0;
        !frozen_edge_too_long(node.a, node.b, max_edge)
            && !frozen_edge_too_long(node.b, node.c, max_edge)
            && !frozen_edge_too_long(node.c, node.a, max_edge)
    }

    fn frozen_mark_voxel_vertex(point: [f64; 3], spec: GridSpec, occupancy: &mut [bool]) {
        let mut index = [0_usize; 3];
        for axis in 0..3 {
            let grid = round_ties_even(point[axis] / spec.pitch);
            let origin = round_ties_even(spec.origin[axis] / spec.pitch);
            index[axis] = (grid - origin) as usize;
        }
        mark_voxel_index(index, spec, occupancy).expect("fixture-sized grid");
    }

    /// The frozen rasterizer's marked-voxel set, for one small fixture. No
    /// cap and a bare `assert!` on the ten-round limit: this only ever runs
    /// on the small corpus fixtures below, never on production input.
    fn frozen_marked_set(mesh: &Mesh, spec: GridSpec) -> Vec<bool> {
        let mut occupancy = vec![false; spec.count];
        for triangle in mesh.tris() {
            let [ia, ib, ic] = *triangle;
            let point = |index: u32| mesh.verts()[index as usize];
            let mut stack = vec![FrozenRasterNode {
                a: point(ia),
                b: point(ib),
                c: point(ic),
                depth: 0,
            }];
            while let Some(node) = stack.pop() {
                if frozen_raster_leaf(node, spec.pitch) {
                    frozen_mark_voxel_vertex(node.a, spec, &mut occupancy);
                    frozen_mark_voxel_vertex(node.b, spec, &mut occupancy);
                    frozen_mark_voxel_vertex(node.c, spec, &mut occupancy);
                    continue;
                }
                assert!(
                    node.depth < FROZEN_MAX_RASTER_SUBDIVISION_ROUNDS,
                    "frozen reference oracle: fixture exceeded the ten-round subdivision limit"
                );
                let ab = frozen_midpoint(node.a, node.b);
                let bc = frozen_midpoint(node.b, node.c);
                let ca = frozen_midpoint(node.c, node.a);
                let next = node.depth + 1;
                stack.push(FrozenRasterNode {
                    a: ab,
                    b: bc,
                    c: ca,
                    depth: next,
                });
                stack.push(FrozenRasterNode {
                    a: ca,
                    b: bc,
                    c: node.c,
                    depth: next,
                });
                stack.push(FrozenRasterNode {
                    a: ab,
                    b: node.b,
                    c: bc,
                    depth: next,
                });
                stack.push(FrozenRasterNode {
                    a: node.a,
                    b: ab,
                    c: ca,
                    depth: next,
                });
            }
        }
        occupancy
    }

    fn production_marked_set(mesh: &Mesh, spec: GridSpec) -> Vec<bool> {
        let mut occupancy = vec![false; spec.count];
        walk_raster_candidates(mesh, spec, |candidate| {
            if candidate.overlaps() {
                mark_voxel_index(candidate.cell(), spec, &mut occupancy)?;
            }
            Ok(())
        })
        .expect("bounded fixture");
        occupancy
    }

    /// **The ruling's precondition.** Every voxel the frozen rasterizer would
    /// have marked is also marked by the area-proportional one -- proved in
    /// the module doc as "every old leaf vertex lies inside the cube of the
    /// voxel it rounds to", and checked here mechanically on real geometry
    /// rather than only argued. Also checked, corpus-wide with exact counts,
    /// by `examples/raster_delta.rs` (see the FU-50 journal section it wrote).
    #[test]
    fn frozen_walker_marks_a_subset_of_the_area_proportional_rasterizer() {
        let settings = registered_settings();
        let pitch = settings.voxel_pitch_mm().get();
        for name in [
            "make_box.prepped.oriented-solid.stl",
            "Cylinder.prepped.oriented-solid.stl",
            "Sphere.prepped.oriented-solid.stl",
            "make_thin_fin.prepped.stl",
            "make_raking_blade.prepped.stl",
        ] {
            let mesh = frozen_mesh(name);
            let spec = grid_spec(&mesh, pitch).expect("grid spec");
            let old = frozen_marked_set(&mesh, spec);
            let new = production_marked_set(&mesh, spec);
            for (index, (old_marked, new_marked)) in old.iter().zip(&new).enumerate() {
                assert!(
                    !*old_marked || *new_marked,
                    "{name}: voxel {index} was marked by the frozen rasterizer but not by \
                     the area-proportional one -- the superset property is violated"
                );
            }
        }
    }

    /// The maximal candidate set: every cell of every triangle's dilated
    /// bounding box, exactly tested. Completeness by construction, and far
    /// too expensive to ship — which is the point of comparing against it.
    fn exhaustive_marked_set(mesh: &Mesh, spec: GridSpec) -> Vec<bool> {
        let mut occupancy = vec![false; spec.count];
        for triangle in mesh.tris() {
            let tri = triangle.map(|index| {
                to_index_space(mesh.verts()[index as usize], spec).expect("finite vertex")
            });
            let range = dilated_index_aabb(tri, spec).expect("bounded fixture");
            if range.iter().any(|axis| axis[0] > axis[1]) {
                continue;
            }
            for x in range[0][0]..=range[0][1] {
                for y in range[1][0]..=range[1][1] {
                    for z in range[2][0]..=range[2][1] {
                        let center = [x as f64, y as f64, z as f64];
                        if triangle_cube_overlaps(tri, center, [0.5, 0.5, 0.5]) {
                            mark_voxel_index(
                                [x as usize, y as usize, z as usize],
                                spec,
                                &mut occupancy,
                            )
                            .expect("clamped index");
                        }
                    }
                }
            }
        }
        occupancy
    }

    /// Adversarial triangles the corpus does not contain: a long diagonal
    /// sliver, its axis-aligned twin, an exactly-collinear zero-area triangle
    /// spanning the grid, a single-point triangle, and a fan-base-sized flat
    /// triangle. Index space is world space here (unit pitch, origin at zero)
    /// so the coordinates read as cell indices.
    fn adversarial_raster_fixtures() -> Vec<(&'static str, Mesh, GridSpec)> {
        let flat_spec = GridSpec {
            origin: [0.0, 0.0, 0.0],
            dims: [220, 220, 3],
            pitch: 1.0,
            count: 220 * 220 * 3,
        };
        let cube_spec = GridSpec {
            origin: [0.0, 0.0, 0.0],
            dims: [204, 204, 204],
            pitch: 1.0,
            count: 204 * 204 * 204,
        };
        let triangle = |points: [[f64; 3]; 3]| {
            Mesh::new(points.to_vec(), vec![[0, 1, 2]]).expect("finite triangle")
        };
        vec![
            (
                "diagonal sliver",
                triangle([
                    [10.0, 10.0, 1.0],
                    [210.0, 210.0, 1.0],
                    [10.014_142_135_623_73, 9.985_857_864_376_27, 1.0],
                ]),
                flat_spec,
            ),
            (
                "axis-aligned sliver",
                triangle([[10.0, 10.0, 1.0], [210.0, 10.0, 1.0], [10.0, 10.02, 1.0]]),
                flat_spec,
            ),
            (
                "collinear grid diagonal",
                triangle([
                    [1.0, 1.0, 1.0],
                    [202.0, 202.0, 202.0],
                    [101.5, 101.5, 101.5],
                ]),
                cube_spec,
            ),
            (
                "single point",
                triangle([[7.25, 8.75, 1.5]; 3]),
                flat_spec,
            ),
            (
                "flat fan base",
                triangle([[5.0, 5.0, 1.0], [205.0, 5.0, 1.0], [5.0, 205.0, 1.0]]),
                flat_spec,
            ),
        ]
    }

    /// **Completeness, mechanically.** The slab-clipping walk's candidate set
    /// is a superset of every cell that truly overlaps a triangle, so the
    /// marked-voxel set it produces is *identical* to the one the maximal
    /// (whole-bounding-box) candidate set produces — on committed fixtures and
    /// on the adversarial aspect ratios alike. This is the property that lets
    /// candidate generation be tightened without moving a single voxel of any
    /// shell, and it is what makes the re-pinned work-item counts a pure cost
    /// reduction.
    #[test]
    fn the_candidate_walk_enumerates_every_cell_an_exhaustive_aabb_scan_marks() {
        // Reported as the first differing cell rather than as two 4-million
        // entry vectors: a completeness failure is one voxel, and the failure
        // message has to be able to name it.
        fn compare(name: &str, mesh: &Mesh, spec: GridSpec) {
            let walked = production_marked_set(mesh, spec);
            let exhaustive = exhaustive_marked_set(mesh, spec);
            let missed = walked
                .iter()
                .zip(&exhaustive)
                .enumerate()
                .filter(|(_, (walked, exhaustive))| **exhaustive && !**walked)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert!(
                missed.is_empty(),
                "{name}: the candidate walk missed {} of {} cells the exhaustive scan \
                 marks; first at linear index {} of dims {:?}",
                missed.len(),
                exhaustive.iter().filter(|marked| **marked).count(),
                missed[0],
                spec.dims
            );
            assert_eq!(
                walked, exhaustive,
                "{name}: the candidate walk marked a cell the exhaustive scan does not — \
                 impossible unless the exact test disagreed with itself"
            );
        }

        let settings = registered_settings();
        let pitch = settings.voxel_pitch_mm().get();
        for name in [
            "make_box.prepped.oriented-solid.stl",
            "make_thin_fin.prepped.stl",
            "make_toed_slab.prepped.stl",
            "make_raking_blade.prepped.stl",
            "make_overhang_table.prepped.stl",
            "Cylinder.prepped.oriented-solid.stl",
            "Sphere.prepped.oriented-solid.stl",
        ] {
            let mesh = frozen_mesh(name);
            let spec = grid_spec(&mesh, pitch).expect("grid spec");
            compare(name, &mesh, spec);
        }
        for (name, mesh, spec) in adversarial_raster_fixtures() {
            compare(name, &mesh, spec);
        }
    }

    /// **The shape-conditioned quadratic tail is gone, not narrowed.** A
    /// projected-AABB sweep costs `Θ(bounding-box area)` per triangle, so a
    /// long diagonal sliver with vanishing area still paid `Θ(L²)` — 20,736
    /// work items for 358 overlapping cells on this exact fixture, 34× its
    /// own axis-aligned twin. That is the geometry-conditioned cap-failure
    /// growth law FU-50 was ruled to remove, and this pins that it is: work
    /// tracks the cells the triangle actually covers, in EVERY orientation,
    /// and a zero-area collinear triangle spanning the grid diagonal no
    /// longer walks its `Θ(L³)` bounding box.
    #[test]
    fn candidate_work_tracks_covered_cells_at_every_aspect_ratio() {
        let cap = H0AdmissionLimits::production().max_raster_work_items();
        let mut work_by_name = std::collections::BTreeMap::new();
        for (name, mesh, spec) in adversarial_raster_fixtures() {
            let work = raster_work_item_count(&mesh, spec, cap).expect("bounded adversary");
            let covered = production_marked_set(&mesh, spec)
                .iter()
                .filter(|marked| **marked)
                .count();
            assert!(
                covered > 0,
                "{name}: fixture must actually cover cells to be a meaningful bound"
            );
            assert!(
                work <= covered * 8,
                "{name}: {work} work items for {covered} covered cells is not \
                 area-proportional"
            );
            work_by_name.insert(name, work);
        }
        // The diagonal and the axis-aligned sliver are the same length and
        // the same width; only the rotation differs, so only a walk that is
        // blind to orientation can charge them differently.
        let diagonal = work_by_name["diagonal sliver"];
        let axis_aligned = work_by_name["axis-aligned sliver"];
        assert!(
            diagonal <= axis_aligned * 3,
            "a rotated sliver costs {diagonal} where its axis-aligned twin costs \
             {axis_aligned}: candidate generation is still orientation-conditioned"
        );
        // A zero-area triangle spanning the 204³ grid diagonal used to walk
        // its whole dilated bounding box (8,242,408 work items, half the
        // production cap, from ONE triangle).
        let collinear = work_by_name["collinear grid diagonal"];
        assert!(
            collinear <= 204 * 32,
            "a collinear grid-diagonal triangle cost {collinear} work items: the \
             degenerate fallback's cubic tail is back"
        );
    }

    /// **FU-50's acceptance test.** `Round 50.stl` and `Round 100.stl` used to
    /// refuse with `RasterWorkLimitExceeded` under the frozen rasterizer
    /// (1.150x and 1.340x the cap; see the FU-50 journal row). Neither is
    /// committed -- both live read-only under `minis-to-process/`, beside the
    /// workspace -- so this follows the same loud, `#[ignore]`-by-default
    /// pattern as `plinth-decimate`'s `local_minis_repair` harness: silent in
    /// `cargo test --workspace`, and a loud, named failure if run explicitly
    /// without the fixtures where the harness expects them.
    #[test]
    #[ignore = "reads local, uncommitted minis-to-process STLs"]
    fn round_50_and_round_100_rasterize_under_the_unchanged_production_cap() {
        let directory = match std::env::var_os("PLINTH_MINIS_DIR") {
            Some(path) => PathBuf::from(path),
            None => std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .expect("plinth-hollow must remain in the workspace crates directory")
                .parent()
                .expect("the workspace must have a parent directory")
                .join("minis-to-process"),
        };
        let settings = registered_settings();
        let pitch = settings.voxel_pitch_mm().get();
        let cap = H0AdmissionLimits::production().max_raster_work_items();
        for name in ["Round 50.stl", "Round 100.stl"] {
            let path = directory.join(name);
            assert!(
                path.is_file(),
                "LocalMinisMissing: {} not found. Point PLINTH_MINIS_DIR at the read-only \
                 minis directory (FU-50's acceptance test needs {name} to rasterize under \
                 the cap).",
                path.display()
            );
            let mesh = load_binary_stl(&path, WeldPolicy::None)
                .unwrap_or_else(|error| panic!("{name}: could not load: {error}"));
            let spec = grid_spec(&mesh, pitch).expect("grid spec");
            let work = raster_work_item_count(&mesh, spec, cap);
            let work = work.unwrap_or_else(|error| {
                panic!(
                    "FU-50 regression: {name} must rasterize under the unchanged production \
                     cap {cap}, got {error}"
                )
            });
            println!(
                "{name}: {work} raster work items ({:.4}x cap {cap})",
                work as f64 / cap as f64
            );
        }
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
        let requested_wall = Finite::new(2.5).expect("finite wall");
        let model = HollowedModel::solid_decision_recording_repair(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            orientation_history(),
            SolidDecisionReason::NoCavityAfterRepair,
            Some(evidence),
            Some(requested_wall),
        )
        .expect("a closed oriented solid is a valid reserve");
        assert_eq!(
            model.history().hollow().disposition(),
            HollowDisposition::Solid {
                reason: SolidDecisionReason::NoCavityAfterRepair
            }
        );
        assert_eq!(model.history().hollow().cavity_repair(), Some(evidence));
        // FU-37: the requested wall travels with the repair-caused decision.
        assert_eq!(model.history().hollow().wall(), Some(requested_wall));
        // The plain decision path makes no repair claim and no wall claim at
        // all, which is what `None` means here — not "a repair of size zero"
        // or "a wall of zero".
        let plain = HollowedModel::solid_decision(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            orientation_history(),
            SolidDecisionReason::NoCavity,
        )
        .expect("a closed oriented solid is a valid reserve");
        assert_eq!(plain.history().hollow().cavity_repair(), None);
        assert_eq!(plain.history().hollow().wall(), None);
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
    /// makes no claim about its own *realized* wall — that is not what is
    /// being carried — but the *requested* wall (FU-37) is exactly as true of
    /// the reserve as it was of the discarded shell, since both were built to
    /// the same request, and it must survive the substitution too.
    #[test]
    fn a_support_refusal_substitution_carries_the_discarded_shells_repair_measurement() {
        let evidence = CavityRepairEvidence {
            critical_cells: 4,
            repaired_voxels: 4,
            cell_visits: 411_674,
            full_grid_passes: 2,
        };
        let requested_wall = Finite::new(2.5).expect("finite wall");
        // Stands in for the drilled shell's own report; `into_solid_fallback`
        // reads the number off it rather than restating one of its own.
        let hollowed = HollowedModel::solid_decision_recording_repair(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            orientation_history(),
            SolidDecisionReason::NoCavityAfterRepair,
            Some(evidence),
            Some(requested_wall),
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
        assert_eq!(
            substituted.history().hollow().wall(),
            Some(requested_wall),
            "the substitution dropped the requested wall the discarded shell recorded"
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
        // Moved 1,900-2,300 -> 1,150-1,300 mm3 by FU-53 (measured 1,221.375
        // mm3): the cavity is now selected on the EXACT nearest-surface
        // distance plus the HOLLOW_CAVITY_MARCH_MARGIN_MM marching margin,
        // not on the EDT's own conservative-rasterization overestimate, so
        // the realised wall is honestly >= 2.5 mm rather than the 1.85 mm
        // (74%) the pre-fix engine actually shipped — see
        // docs/engineering-journal/CURRENT-STATUS.md's FU-53 landing entry.
        assert!(
            (1_150.0..=1_300.0).contains(&candidate.cavity().cavity_volume_mm3().get()),
            "the corrected exact-distance shell has the expected honestly-walled cavity range"
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
        // FU-53 moved this off the frozen Sphere: at the corrected exact-distance
        // selection plus the new HOLLOW_CAVITY_MARCH_MARGIN_MM margin, the
        // Sphere's cavity at the registered handling-heavy wall (3.0 mm + 0.866
        // mm margin, cavity ~1,091 mm3 of a 4,175 mm3 source) falls under the
        // 25% savings floor, which the frozen fixture's default wall (2.5 mm,
        // cavity ~1,221 mm3, 29.3%) clears only narrowly itself — both are
        // legitimate outcomes of the honestly-measured cavity (the whole point
        // of this fix), not a defect, but they make the Sphere too tight a
        // fixture for a test about wall-selection *plumbing*. A 40 mm box has
        // ample headroom at both registered walls and isolates the property
        // this test actually asserts.
        let shell = |request: &HollowRequest| {
            create_shell(
                make_box([40.0, 40.0, 40.0], [0.0, 0.0, 0.0]).expect("box fixture"),
                HollowSettings::from_request(request).expect("request settings"),
            )
            .expect("a 40mm box is hollow-eligible at either registered wall")
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

    /// FU-53: the registered handling-heavy wall (3.0 mm) now correctly
    /// removes the frozen Sphere from eligibility once the honest margin is
    /// applied (3.866 mm effective threshold on a 10.25 mm-radius sphere
    /// leaves a cavity of ~1,091 mm3 against a ~1,044 mm3 25%-of-source
    /// floor) — the tightened savings gate the design's risk section
    /// predicted, converted by [`run`] into a typed Solid decision rather
    /// than a shipped shell. This is the "measured and recorded" the design
    /// asked for on the committed Sphere: the default wall clears with
    /// ~1,221 mm3 of ~4,175 mm3 (29.3%), and the heavier wall does not.
    #[test]
    fn the_committed_sphere_clears_savings_at_the_default_wall_but_not_the_handling_heavy_wall() {
        let default = create_shell(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            HollowSettings::from_registered_constants().expect("M0 settings"),
        )
        .expect("the frozen Sphere hollows at the default 2.5mm wall");
        assert!(default.cavity().cavity_volume_mm3().get() > 0.0);

        let heavy_wall = registered_wall(WallSelection::HandlingHeavy).expect("registered wall");
        let heavy_settings = HollowSettings::from_request(&HollowRequest {
            printer: registered_printer(),
            wall_mm: heavy_wall,
        })
        .expect("heavy wall settings");
        let error = create_shell(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            heavy_settings,
        )
        .expect_err("the honest cavity at the heavier wall falls under the savings floor");
        assert!(matches!(error, HollowError::IneligibleSavings));
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
                1,
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

    /// FU-54: `max_cavity_repair_passes` is validated exactly like its
    /// admission-limit siblings -- zero and `usize::MAX` both refuse
    /// construction, and every value strictly between is accepted.
    #[test]
    fn cavity_repair_pass_admission_rejects_zero_and_usize_max() {
        let valid_but_for_the_field_under_test = |max_cavity_repair_passes: usize| {
            H0AdmissionLimits::new(
                TriageTopologyLimits::production(),
                DEFAULT_MAX_VOXEL_CELLS,
                DEFAULT_QUERY_WORKING_BYTES,
                DEFAULT_MAX_RASTER_WORK_ITEMS,
                BooleanLimits {
                    max_input_primitive_units: DEFAULT_MAX_BOOLEAN_PRIMITIVE_UNITS,
                    max_logical_marshaled_input_bytes: DEFAULT_MAX_BOOLEAN_MARSHALLED_BYTES,
                    max_output_vertices: DEFAULT_MAX_BOOLEAN_OUTPUT_VERTICES,
                    max_output_triangles: DEFAULT_MAX_BOOLEAN_OUTPUT_TRIANGLES,
                },
                DEFAULT_DRAIN_PLANNING_BYTES,
                DEFAULT_CAVITY_DISTANCE_INDEX_BYTES,
                max_cavity_repair_passes,
            )
        };
        assert!(matches!(
            valid_but_for_the_field_under_test(0),
            Err(HollowError::InvalidAdmissionLimits)
        ));
        assert!(matches!(
            valid_but_for_the_field_under_test(usize::MAX),
            Err(HollowError::InvalidAdmissionLimits)
        ));
        let admitted = valid_but_for_the_field_under_test(1).expect("1 is a valid ceiling");
        assert_eq!(admitted.max_cavity_repair_passes(), 1);
        let production_default =
            valid_but_for_the_field_under_test(DEFAULT_MAX_CAVITY_REPAIR_PASSES)
                .expect("the production default is a valid ceiling");
        assert_eq!(
            production_default.max_cavity_repair_passes(),
            DEFAULT_MAX_CAVITY_REPAIR_PASSES
        );
        assert_eq!(
            H0AdmissionLimits::production().max_cavity_repair_passes(),
            DEFAULT_MAX_CAVITY_REPAIR_PASSES,
            "production() must carry the same default this constructor validates"
        );
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

/// FU-53: the exact-distance cavity selection, its soundness lemma, and the
/// wall it now honestly ships.
#[cfg(test)]
mod fu53_exact_cavity_distance {
    use super::*;
    use plinth_mesh::{load_binary_stl, make_box};

    fn frozen_mesh(file: &str) -> Mesh {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(file);
        load_binary_stl(path, WeldPolicy::None).expect("committed frozen STL")
    }

    /// A box with one face tilted into a ramp: axis-aligned everywhere except
    /// the top, which slopes linearly from `low_z` (at `x = 0`) to `high_z`
    /// (at `x = size`) — "one face at an angle", the FU-53 diagnosis's
    /// synthetic breach-class shape, built directly rather than recovered
    /// from an undocumented ad hoc script. Still a valid closed hexahedron:
    /// the top is planar (`z` depends only on `x`), the two faces at
    /// constant `x` stay flat rectangles, and the two faces at constant `y`
    /// stay flat (if sloped) trapezoids.
    fn ramped_box(size: f64, low_z: f64, high_z: f64, tilt_axis_is_x: bool) -> Mesh {
        let z_at = |primary: f64| low_z + (high_z - low_z) * (primary / size);
        let mut verts = Vec::with_capacity(8);
        let mut push = |x: f64, y: f64, z: f64| {
            verts.push([x, y, z]);
        };
        for &(x, y) in &[(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)] {
            push(x, y, 0.0);
        }
        for &(x, y) in &[(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)] {
            let top_z = z_at(if tilt_axis_is_x { x } else { y });
            push(x, y, top_z);
        }
        // Bottom 0..4 (z=0, outward normal -z), top 4..8 (ramped, outward
        // normal has a positive z component), four flat sides between them.
        let tris = vec![
            // Bottom, wound to face -z.
            [0, 2, 1],
            [0, 3, 2],
            // Ramped top, wound to face +z (ish).
            [4, 5, 6],
            [4, 6, 7],
            // Side x=0..size, y=0 (front).
            [0, 1, 5],
            [0, 5, 4],
            // Side x=size, y=0..size (right).
            [1, 2, 6],
            [1, 6, 5],
            // Side x=0..size, y=size (back).
            [2, 3, 7],
            [2, 7, 6],
            // Side x=0, y=0..size (left).
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh::new(verts, tris).expect("ramped box fixture")
    }

    /// **The lemma the whole fix rests on: `edt(v) >= d_true(v)` for every
    /// material voxel.** Proved mechanically against the real production
    /// rasterizer and the real geom-kernel BVH query, not asserted in prose,
    /// on four fixtures spanning smooth (Sphere, Cylinder), axis-aligned
    /// (box) and oblique (ramped box) geometry. If this ever fails the
    /// exact-distance prefilter in [`exact_cavity_distance`] is UNSOUND —
    /// candidates could be dropped that the true selection needed — which is
    /// a correctness defect, not merely a missed speedup.
    #[test]
    fn frozen_oracle_edt_never_understates_the_exact_distance() {
        for (label, mesh) in [
            (
                "sphere",
                frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            ),
            (
                "cylinder",
                frozen_mesh("Cylinder.prepped.oriented-solid.stl"),
            ),
            ("box", make_box([10.0, 10.0, 10.0], [0.0, 0.0, 0.0]).expect("box")),
            (
                "ramped_box",
                ramped_box(20.0, 4.0, 16.0, true),
            ),
        ] {
            let source = triage_exact_with_measured_limits(&mesh)
                .unwrap_or_else(|error| panic!("{label} triage: {error:?}"))
                .into_mesh();
            let pitch = registered_settings().voxel_pitch_mm().get();
            let spec = grid_spec(&source, pitch).unwrap_or_else(|error| {
                panic!("{label} grid spec: {error:?}")
            });
            let work = raster_work_item_count(
                &source,
                spec,
                H0AdmissionLimits::production().max_raster_work_items(),
            )
            .unwrap_or_else(|error| panic!("{label} raster work: {error:?}"));
            let grid = voxelize_filled(&source, spec, work)
                .unwrap_or_else(|error| panic!("{label} voxelize: {error:?}"));
            let edt = frozen_distance_transform(&grid.material, grid.pitch)
                .unwrap_or_else(|error| panic!("{label} EDT: {error:?}"));

            let candidates: Vec<([usize; 3], [f64; 3])> = edt
                .indexed_iter()
                .filter(|(_, distance)| **distance > 0.0)
                .map(|((iz, iy, ix), _)| {
                    (
                        [iz, iy, ix],
                        [
                            grid.origin[0] + (ix as f64) * grid.pitch,
                            grid.origin[1] + (iy as f64) * grid.pitch,
                            grid.origin[2] + (iz as f64) * grid.pitch,
                        ],
                    )
                })
                .collect();
            assert!(
                !candidates.is_empty(),
                "{label} must have at least one interior voxel"
            );

            let estimate = NativeSurface::estimate_index_memory_from_indexed(
                source.verts(),
                source.tris(),
                grid.pitch,
            )
            .unwrap_or_else(|error| panic!("{label} surface estimate: {error:?}"));
            let surface = NativeSurface::from_indexed(
                source.verts(),
                source.tris(),
                NativeSurfaceConfig {
                    grid_pitch_mm: grid.pitch,
                    max_grid_bytes: estimate.max_axis_grid_bytes(),
                },
            )
            .unwrap_or_else(|error| panic!("{label} surface build: {error:?}"));
            let points: Vec<[f64; 3]> =
                candidates.iter().map(|(_, point)| *point).collect();
            let (distances, _faces) = surface.query(&points, f64::INFINITY);

            for ((voxel, _), &d_true) in candidates.iter().zip(distances.iter()) {
                let edt_value = edt[*voxel];
                assert!(
                    d_true <= edt_value + 1e-9,
                    "{label} voxel {voxel:?}: EDT {edt_value} understated the exact distance {d_true}"
                );
            }
        }
    }

    /// **The margin's derivation is enforced, not merely written down.**
    ///
    /// `HOLLOW_CAVITY_MARCH_MARGIN_MM` is declared in the registry as the
    /// literal `0.5 * 1.7320508075688772`, where the `0.5` duplicates
    /// `HOLLOW_PITCH_MM`'s value rather than reading it, and its own prose
    /// says it is DERIVED and MUST be re-derived if the pitch ever moves.
    /// Nothing mechanical held those two together: the constants tests pin
    /// each as an independent literal, and `HollowSettings::with_wall_mm`
    /// reads each from the registry independently. Bumping the pitch to
    /// 0.6 mm would leave the margin at 0.866 mm while a surface-nets vertex
    /// could sit 0.6·√3 ≈ 1.039 mm from a selected cavity centre — the
    /// realised wall undershooting the requested one by ~0.17 mm, silently
    /// reopening the FU-53 shortfall with every constants test green.
    ///
    /// This is the cross-constant assertion that closes it: the margin is
    /// exactly one voxel body diagonal of the pitch the same registry
    /// publishes, checked against the settings the engine actually runs on.
    /// Exact equality, no tolerance — the registered literal is the f64
    /// nearest to `pitch * sqrt(3)` and a re-derivation that is not is a
    /// re-derivation that was not done.
    #[test]
    fn the_cavity_march_margin_is_one_body_diagonal_of_the_registered_pitch() {
        let settings = registered_settings();
        let pitch = settings.voxel_pitch_mm().get();
        let margin = settings.cavity_march_margin_mm().get();
        assert_eq!(
            margin,
            pitch * 3.0_f64.sqrt(),
            "HOLLOW_CAVITY_MARCH_MARGIN_MM ({margin}) is no longer HOLLOW_PITCH_MM \
             ({pitch}) x sqrt(3): the FU-53 soundness margin's own prose says it is \
             DERIVED from the pitch and must be re-derived whenever the pitch moves"
        );
        // The lemma the margin buys, restated as the inequality FU-53's
        // selection math relies on: no surface-nets vertex of a boundary
        // cavity cell can be further from that cell's centre than the margin.
        let furthest_vertex_from_a_cell_centre = pitch * 3.0_f64.sqrt();
        assert!(
            margin >= furthest_vertex_from_a_cell_centre,
            "the margin no longer covers a full cell body diagonal"
        );
    }

    /// **The analytic Sphere: the cavity radius and the realised minimum wall.**
    ///
    /// The committed Sphere is close enough to an ideal sphere (its origin
    /// and bounds are frozen in `frozen_oracle_characterizes_...` above:
    /// `origin = [-10, -10, 0]`, `dims = [41, 41, 41]` at 0.5 mm pitch, so
    /// radius ~= 10.25 mm) that the analytic cavity radius `R - wall` is a
    /// real prediction, not a hand-tuned number. The realised minimum wall —
    /// brute-force minimum distance from every cavity-shell vertex to the
    /// source surface — is what the FU-53 diagnosis measured directly and is
    /// what this test re-measures against the fixed engine: the journal
    /// recorded 1.85 mm (74 % of the registered 2.5 mm) before this fix.
    #[test]
    fn the_analytic_sphere_reaches_its_requested_wall() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let settings = registered_settings();
        let candidate = create_shell(source.clone(), settings)
            .expect("the frozen Sphere is hollow-eligible at the default wall");

        // The cavity radius: `V = 4/3 pi r^3`, so `r = (3V / 4pi)^(1/3)`.
        let cavity_mm3 = candidate.cavity().cavity_volume_mm3().get();
        let cavity_radius = (3.0 * cavity_mm3 / (4.0 * std::f64::consts::PI)).cbrt();
        // Source bounds pin the sphere's own radius independently of the
        // cavity: origin [-10,-10,0], dims [41,41,41] at 0.5 mm pitch, so the
        // bounding box spans 20.5 mm on every axis and the analytic radius is
        // 10.25 mm.
        let source_radius = 10.25_f64;
        let wall = settings.wall_thickness_mm().get();
        // Selection runs against `wall + margin` (this fix's whole point),
        // not against the bare requested wall.
        let predicted_cavity_radius =
            source_radius - (wall + settings.cavity_march_margin_mm().get());
        assert!(
            (cavity_radius - predicted_cavity_radius).abs() <= settings.voxel_pitch_mm().get(),
            "cavity radius {cavity_radius} is not within one pitch of the analytic R - (wall + margin) = {predicted_cavity_radius}"
        );

        // The realised minimum wall: brute-force nearest-source-surface
        // distance from every vertex of the shell's inner (negative) cavity
        // surface, exactly the FU-53 diagnosis's own measurement.
        let native_estimate = NativeSurface::estimate_index_memory_from_indexed(
            source.verts(),
            source.tris(),
            0.5,
        )
        .expect("surface estimate");
        let native = NativeSurface::from_indexed(
            source.verts(),
            source.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: native_estimate.max_axis_grid_bytes(),
            },
        )
        .expect("surface build");
        let topology = build_face_topology(candidate.mesh(), WeldPolicy::ExactBits)
            .expect("shell topology");
        let certificate = certify_closed_components(candidate.mesh(), &topology)
            .expect("shell certificate");
        let cavity_component_id = certificate
            .components()
            .iter()
            .find(|component| component.signed_volume_mm3() < 0.0)
            .expect("shell must have a negative cavity component")
            .component_id();
        let mut cavity_vertex_ids: Vec<u32> = topology
            .component_ids()
            .iter()
            .zip(candidate.mesh().tris())
            .filter(|(&component_id, _)| component_id == cavity_component_id)
            .flat_map(|(_, triangle)| *triangle)
            .collect();
        cavity_vertex_ids.sort_unstable();
        cavity_vertex_ids.dedup();
        let cavity_points: Vec<[f64; 3]> = cavity_vertex_ids
            .iter()
            .map(|&index| candidate.mesh().verts()[index as usize])
            .collect();
        let (distances, _faces) = native.query(&cavity_points, 10.0);
        let realised_wall_mm = distances
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        assert!(
            realised_wall_mm.is_finite(),
            "every cavity vertex must be within the 10.0mm query bound of the source surface"
        );
        assert!(
            realised_wall_mm >= wall,
            "realised wall {realised_wall_mm} mm fell below the requested {wall} mm -- FU-53 must guarantee realised >= requested"
        );
    }

    /// **The breach class disappears.** A synthetic box with one face tilted
    /// into a ramp (`ramped_box`, this module's own reconstruction of the
    /// FU-53 diagnosis's undocumented ad hoc fixture) refused with
    /// `InvalidShellTopology` in 13 of 15 configurations pre-fix: the cavity
    /// was cut so close to the oblique face that it broke through and opened
    /// the void to the outside, non-monotonically in fin thickness (the
    /// signature of lattice aliasing). A cavity that is never within `wall`
    /// of the true surface cannot break out, so this sweep asserts the class
    /// is gone: every configuration either hollows or refuses for a *named,
    /// different* reason (a legitimate savings/bulk shortfall on a thin
    /// synthetic ramp is expected and is not the defect this test guards).
    #[test]
    fn the_ramped_box_breach_class_is_gone() {
        let settings = HollowSettings {
            save_min_mm3: Finite::new(1.0).expect("finite"),
            save_min_fraction: Finite::new(0.0).expect("finite"),
            ..registered_settings()
        };
        let mut any_hollowed = false;
        for rise_mm in [2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0] {
            for tilt_axis_is_x in [true, false] {
                let mesh = ramped_box(20.0, 4.0, 4.0 + rise_mm, tilt_axis_is_x);
                match create_shell(mesh, settings) {
                    Ok(candidate) => {
                        any_hollowed = true;
                        assert_eq!(
                            cut_census(candidate.mesh()),
                            (0, 0, 0),
                            "rise={rise_mm} axis_is_x={tilt_axis_is_x}: a hollowed shell must be uncut"
                        );
                    }
                    Err(HollowError::InvalidShellTopology) => panic!(
                        "rise={rise_mm} axis_is_x={tilt_axis_is_x}: the breach class FU-53 exists to close is still live"
                    ),
                    Err(_other) => {
                        // A different, named refusal (no cavity, below bulk,
                        // below savings) is not the defect this test guards.
                    }
                }
            }
        }
        assert!(
            any_hollowed,
            "the sweep must hollow at least one configuration to be a meaningful proof"
        );
    }

    /// `(boundary edges, non-manifold faces, point-only faces)`, mirroring
    /// `corpus_cavity_repair::cut_census`.
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

    /// FU-53's ADMISSION addition: a caller-set `max_cavity_distance_index_bytes`
    /// too small to build the exact-distance surface refuses typed, with no
    /// fallback to the EDT-only selection this admission domain exists to
    /// move H0 off of.
    #[test]
    fn a_too_small_cavity_distance_index_limit_is_a_typed_refusal_not_a_fallback() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let mut limits = H0AdmissionLimits::production();
        limits.max_cavity_distance_index_bytes = 1;
        let error = create_shell_with_admission_limits(source, registered_settings(), limits)
            .expect_err("a 1-byte cavity-distance index limit must refuse");
        assert!(matches!(
            error,
            HollowError::CavityDistanceIndexLimitExceeded { bytes, max_bytes: 1 }
                if bytes > 1
        ));
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
    /// Two 10 mm-radius lobes joined by a thin waist running at 45° in the xy
    /// plane. The waist is the whole point: at the production selection
    /// threshold its cavity is a *sub-voxel* thread, so the selected mask
    /// crosses the waist as a chain of lattice-diagonal voxels — which is
    /// precisely Latecki's C1/C2 configuration, the one a one-vertex-per-cell
    /// extractor cannot represent.
    ///
    /// The rod radius is exact in binary at every value this module uses
    /// (3.375 mm = 27/8, 3.875 mm = 31/8) so the lattice decides the same way
    /// on any target. The diagonal is what makes it work: an axis-aligned
    /// waist produces a thread that is thin but well-composed.
    ///
    /// FU-53 note: the rod radius is a parameter (rather than the historical
    /// fixed 3.375 mm) because the lattice-diagonal defect this fixture
    /// exists to reproduce is non-monotonic in it — see the retune note on
    /// `a_synthetic_pinched_dumbbell_needs_the_repair_and_hollows_because_of_it`.
    fn pinched_dumbbell_with_rod_radius(rod_radius: f64) -> Mesh {
        let yaw = std::f64::consts::FRAC_PI_4;
        let span = 40.0;
        let far = [span * yaw.cos(), span * yaw.sin(), 0.0];
        let parts = [
            sphere(10.0, [0.0, 0.0, 0.0], 32, 48),
            sphere(10.0, far, 32, 48),
            rod(rod_radius, span, yaw, 48),
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
        let source = pinched_dumbbell_with_rod_radius(3.875);
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");

        // 1. The mask this fixture exists to produce, and the extractor's
        //    failure on it. The cavity thread across the 45° waist is a chain
        //    of lattice-diagonal voxels, so the one-vertex-per-cell surface has
        //    edges used by four faces instead of two.
        let (unrepaired, grid) = unrepaired_cavity_mask(&source, settings);
        let raw = marching_cavity(&unrepaired, grid.origin, grid.pitch)
            .expect("the unrepaired mask still extracts a closed surface");
        // Moved (0, 8, 26) -> (0, 16, 50) on 2026-08-12 by FU-50's area-
        // proportional conservative rasterizer replacing the frozen midpoint-
        // quartering one (decisions/2026-08-12-owner-rulings-fu-batch.md §3):
        // the marked-voxel set only grows, so the lattice-diagonal thread
        // across the waist this fixture is built to exercise now spans more
        // non-manifold and point-only faces.
        //
        // Unaffected by FU-53's rod-radius retune below (3.375 -> 3.875 mm):
        // `unrepaired_cavity_mask` stops one line early, at the pre-fix
        // `edt > wall` selection with no exact distance and no march margin,
        // so it characterizes the same lattice-diagonal thread topology
        // either way. Still watertight (0 boundary edges) but non-manifold,
        // which is the whole premise this fixture proves.
        assert_eq!(
            cut_census(&raw),
            (0, 16, 50),
            "the fixture no longer exhibits the defect the repair exists to fix: \
             watertight but non-manifold is the whole premise"
        );

        // 2. The repair, through the production entry point. Integer
        //    combinatorics over a deterministic voxel field, so these are
        //    equalities.
        //
        // FU-53 note: this fixture's two 10 mm lobes are deliberately small
        // so the repair closes in under a second, and its cavity (a few
        // thousand mm3) no longer clears the registered savings floor once
        // the honest exact-distance-plus-margin wall shrinks it — a real and
        // separately covered consequence of the fix (see
        // `the_committed_sphere_clears_savings_at_the_default_wall_but_not_the_handling_heavy_wall`),
        // not something this test exists to measure. `wall_thickness_mm`,
        // `voxel_pitch_mm` and `cavity_march_margin_mm` stay exactly the
        // registered values step 1 and the assertions below depend on; only
        // the unrelated savings floor is relaxed, so this test keeps
        // isolating repair mechanics from the savings gate.
        let step3_settings = HollowSettings {
            save_min_mm3: Finite::new(1.0).expect("finite"),
            save_min_fraction: Finite::new(0.0).expect("finite"),
            ..settings
        };
        let candidate =
            create_shell(source, step3_settings).expect("the repaired dumbbell hollows");
        let resources = candidate.cavity().resources();
        // Moved 4 -> 8 on 2026-08-12 by FU-50: the extra non-manifold/
        // point-only faces above (8 -> 16, 26 -> 50) come from a wider
        // lattice-diagonal thread, which the repair now finds more critical
        // cells on.
        // Moved 8 -> 132 critical cells (repaired_voxels 8 -> 68, full-grid
        // passes unchanged at 2) by FU-53's fixture retune: the rod radius
        // moved 3.375 -> 3.875 mm to keep exhibiting the non-manifold pinch
        // once the production selection threshold moved from the bare 2.5 mm
        // wall to 2.5 mm + HOLLOW_CAVITY_MARCH_MARGIN_MM (0.866 mm) evaluated
        // on the EXACT nearest-surface distance rather than the EDT. A sweep
        // over rod radius (3.0-6.0 mm in 0.125 mm steps, ad hoc, not
        // committed as a test) found the defect is non-monotonic in radius,
        // the same lattice-aliasing signature FU-53's finned-block
        // breach class shows: 3.875, 4.375, 4.875 mm (132 critical cells),
        // 5.375 mm (136) and 5.875 mm (140) reproduce it; every radius
        // between and outside them does not. 3.875 mm is the closest
        // reproduction to the original 3.375 mm.
        assert_eq!(resources.cavity_critical_cells(), 132);
        assert_eq!(resources.cavity_repaired_voxels(), 68);
        // The opening cell scan plus exactly one whole-grid re-selection: the
        // removal stranded part of the component, which is the second
        // `O(cells)` domain the cell-visit bound does not cover.
        assert_eq!(resources.cavity_repair_full_grid_passes(), 2);
        // `voxel_count` is `GridSpec::count` -- the admitted grid's cell
        // total, computed from the mesh's own bounds. Unchanged by the rod
        // radius retune: the rod is thin relative to the two 10 mm lobes
        // that dominate the bounding box either way.
        assert_eq!(resources.voxel_count(), 393_764);
        let cells = 99 * 99 * 42;
        assert_eq!(
            resources.cavity_repair_cell_visits(),
            cells + 8 * resources.cavity_repaired_voxels()
        );
        // Moved 411,706 -> 412,186 tracking `cavity_repaired_voxels` above
        // (8 -> 68, so +8*60 = 480 more visits).
        assert_eq!(resources.cavity_repair_cell_visits(), 412_186);

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

    /// FU-54, on real geometry rather than only the synthetic mask in
    /// `well_composed.rs`'s own tests. The same pinched-dumbbell fixture the
    /// test above proves needs exactly `full_grid_passes() == 2` at the
    /// production limit of 8. Admitted at a `max_cavity_repair_passes` of 1 —
    /// enough for the opening scan and nothing else — the same fixture
    /// refuses instead of repairing, through the real production entry
    /// point, `create_shell` and its H0 pipeline included, not a call
    /// straight into `well_composed_cavity_component`.
    #[test]
    fn the_pass_limit_refuses_a_real_shell_candidate_through_the_production_entry_point() {
        let source = pinched_dumbbell_with_rod_radius(3.875);
        let settings = HollowSettings::from_registered_constants().expect("M0 settings");
        // Same isolation as the test above: only the pass limit under test
        // moves, so a refusal here is provably about the pass bound and not
        // about the unrelated savings floor this fixture's small cavity
        // would otherwise also trip.
        let step3_settings = HollowSettings {
            save_min_mm3: Finite::new(1.0).expect("finite"),
            save_min_fraction: Finite::new(0.0).expect("finite"),
            ..settings
        };
        let mut starved = H0AdmissionLimits::production();
        starved.max_cavity_repair_passes = 1;
        let error = create_shell_with_admission_limits(source.clone(), step3_settings, starved)
            .expect_err("one admitted pass cannot repair this fixture's waist pinch");
        match error {
            HollowError::CavityRepairPassLimitExceeded {
                passes,
                max_passes,
                critical_cells,
                repaired_voxels,
                cell_visits,
            } => {
                assert_eq!(passes, 1, "refused at the opening scan's own pass count");
                assert_eq!(max_passes, 1);
                assert_eq!(critical_cells, 132, "same defect the unstarved run measures");
                assert!(repaired_voxels > 0, "the opening scan already removed voxels");
                assert!(cell_visits > 0);
            }
            other => panic!("expected CavityRepairPassLimitExceeded, got {other}"),
        }

        // The recording entry point lifts the same evidence onto the
        // failure, exactly as `CavityRepairRemovedCavity` and
        // `IneligibleSavingsAfterCavityRepair` do -- a caller reading
        // `cavity_repair` must see the partial repair regardless of which of
        // the three variants stopped it.
        let failure = create_shell_recording_repair(source, step3_settings, starved)
            .expect_err("the same starved limit refuses through the recording entry point");
        let evidence = failure
            .cavity_repair()
            .expect("a repair ran before the pass limit stopped it");
        assert_eq!(evidence.critical_cells, 132);
        assert_eq!(evidence.full_grid_passes, 1);
        assert!(matches!(
            failure.error(),
            HollowError::CavityRepairPassLimitExceeded { .. }
        ));
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
        // FU-53 note: at the REGISTERED savings floor the honestly-walled
        // Treant (exact distance plus HOLLOW_CAVITY_MARCH_MARGIN_MM) now
        // fails `IneligibleSavingsAfterCavityRepair` -- the mandated,
        // measured-and-recorded consequence FU-55's ruling anticipates (an
        // under-walled shell never ships; this one now correctly does not
        // clear savings and the pipeline substitutes the solid reserve). See
        // the FU-53 journal landing entry for that production-settings
        // measurement. This test's own subject is the well-composedness
        // repair (FU-49), proved end to end through the real production
        // entry point; only the unrelated savings floor is relaxed here, the
        // same isolation the synthetic pinched-dumbbell test above uses.
        let repair_test_settings = HollowSettings {
            save_min_mm3: Finite::new(1.0).expect("finite"),
            save_min_fraction: Finite::new(0.0).expect("finite"),
            ..settings
        };
        let candidate = create_shell(source, repair_test_settings)
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
        //
        // Moved 70 -> 52 critical cells, 115 -> 67 repaired voxels, and
        // 5,405,880 -> 5,405,496 cell visits on 2026-08-12 by FU-50's area-
        // proportional conservative rasterizer (decisions/2026-08-12-owner-
        // rulings-fu-batch.md §3). Unlike the pinched-dumbbell fixture above,
        // this fell rather than grew: the well-composedness repair's critical-
        // cell count is a property of the *local pinch pattern* the exact
        // marking produces, not a monotone function of the (monotonically
        // growing) marked-voxel set, so either direction is consistent with
        // the superset proof.
        //
        // Moved 52 -> 116 critical cells, 67 -> 170 repaired voxels, and
        // 5,405,496 -> 5,406,320 cell visits on 2026-08-12 by FU-53: cavity
        // selection now runs on the exact nearest-surface distance plus
        // HOLLOW_CAVITY_MARCH_MARGIN_MM instead of the EDT's conservative
        // overestimate, which is a different selection boundary with its own
        // local pinch pattern -- again not a monotone consequence of the
        // margin shrinking the cavity, for the same reason recorded above.
        assert_eq!(resources.cavity_critical_cells(), 116);
        assert_eq!(resources.cavity_repaired_voxels(), 170);
        assert_eq!(resources.cavity_repair_cell_visits(), 5_406_320);
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

        // 4. The cavity that survived, and the shell built from it.
        //
        // Moved 8,196.5 -> 3,118.75 mm3 on 2026-08-12 by FU-53: the honestly
        // larger wall (exact distance plus HOLLOW_CAVITY_MARCH_MARGIN_MM)
        // removes far more of the treant's branched, oblique geometry than
        // the EDT's own conservative overestimate did -- the same effect the
        // FU-53 diagnosis measured directly against the source surface as a
        // 0.70 mm (28%) realised wall on this exact fixture, now corrected.
        // This is measured with `save_min_mm3`/`save_min_fraction` relaxed
        // (see the note above `create_shell`, this test's whole reason for
        // doing that): at the REGISTERED savings floor this honest cavity no
        // longer clears it, and the model falls back to Solid in production
        // (see the FU-53 journal landing entry).
        let cavity_mm3 = candidate.cavity().cavity_volume_mm3().get();
        assert_eq!(cavity_mm3, 3_118.75);
        let pitch = candidate.cavity().frozen_voxel_pitch_mm().get();
        assert_eq!((cavity_mm3 / pitch.powi(3)) as usize, 24_950);

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

/// FU-55: never ship an under-walled shell. The realized-wall measurement,
/// its no-epsilon check, `run`'s conversion of a refusal into
/// [`SolidDecisionReason::ShellUnderWalled`], and the evidence continuity
/// the ruling asks for.
#[cfg(test)]
mod fu55_realized_wall_check {
    use super::*;
    use plinth_mesh::{load_binary_stl, make_box, mesh_fingerprint};
    use plinth_orient::{BaselineSelection, OrientationProvenance, OrientationReport, RigidTransform};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn frozen_mesh(file: &str) -> Mesh {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(file);
        load_binary_stl(path, WeldPolicy::None).expect("committed frozen STL")
    }

    fn orientation_history() -> OrientationHistory {
        let mesh = make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget = plinth_decimate::DecimationBudget::ratified_default(
            plinth_decimate::ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0),
        )
        .expect("registered fixture budget");
        let outcome = plinth_decimate::run(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        OrientationHistory::from_reports(
            audit,
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        )
    }

    /// **No epsilon.** `raw_realized_wall_mm == requested_wall_mm` passes;
    /// a hair under refuses with the refusal's own numbers, unrounded.
    #[test]
    fn the_check_has_no_epsilon() {
        checked_realized_wall_mm(2.5, 2.5, 10.0).expect("exactly at the requested wall passes");
        let error = checked_realized_wall_mm(2.5 - 1e-9, 2.5, 10.0)
            .expect_err("a hair under the requested wall must refuse");
        match error {
            HollowError::RealizedWallBelowSpec {
                realized_wall_mm,
                requested_wall_mm,
            } => {
                assert_eq!(requested_wall_mm, 2.5);
                assert_eq!(realized_wall_mm, 2.5 - 1e-9);
            }
            other => panic!("expected RealizedWallBelowSpec, got {other:?}"),
        }
    }

    /// A measurement the query could only bound (`f64::INFINITY`, which
    /// [`Finite`] cannot represent) is recorded as a truthful "at least this
    /// much" — clamped to the bound it was queried against — rather than
    /// failing to construct evidence for a shell that comfortably passed.
    #[test]
    fn an_over_bound_measurement_is_recorded_as_a_clamped_lower_bound() {
        let recorded = checked_realized_wall_mm(f64::INFINITY, 2.5, 7.0)
            .expect("an over-bound measurement still passes the check");
        assert_eq!(recorded.get(), 7.0);
    }

    /// `measure_realized_wall_mm`'s own contract: a serial min-fold over
    /// batched, index-parallel queries, invariant to how the vertices are
    /// chunked, and an over-bound vertex reads as [`f64::INFINITY`] rather
    /// than a false minimum below the true one.
    #[test]
    fn measure_realized_wall_mm_is_batch_invariant_and_over_bound_reads_as_infinity() {
        // A 20mm cube centred on the origin (x, y in [-10, 10], z in
        // [0, 20]).
        let source = make_box([20.0, 20.0, 20.0], [0.0, 0.0, 0.0]).expect("source box");
        let estimate =
            NativeSurface::estimate_index_memory_from_indexed(source.verts(), source.tris(), 0.5)
                .expect("estimate");
        let surface = NativeSurface::from_indexed(
            source.verts(),
            source.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: estimate.max_axis_grid_bytes(),
            },
        )
        .expect("surface");
        // A 2mm cube centred at (0, 0, 10), well inside the source cube: by
        // construction (hand-computed, not measured) every one of its 8
        // corners sits exactly 9.0 mm from the nearest source face -- 9.0 mm
        // in x, in y, and in z alike, at every corner.
        let inner = make_box([2.0, 2.0, 2.0], [0.0, 0.0, 9.0]).expect("inner box");
        assert_eq!(inner.verts().len(), 8);

        for batch_points in [1, 3, 8, 64] {
            let minimum = measure_realized_wall_mm(&surface, &inner, 20.0, batch_points);
            assert_eq!(
                minimum, 9.0,
                "batch size {batch_points} changed the measured minimum"
            );
        }

        // Below every vertex's true distance: every query comes back
        // over-bound, which `NativeSurface::query` reports as
        // `f64::INFINITY` -- never a false minimum below the true one.
        let over_bound = measure_realized_wall_mm(&surface, &inner, 5.0, 8);
        assert_eq!(over_bound, f64::INFINITY);
    }

    /// **Test 1 of the FU-55 ruling: the positive case.** The committed
    /// Sphere hollows at the registered wall, and the realized minimum wall
    /// this shell measured is `>=` the requested 2.5 mm -- the guarantee the
    /// ruling exists to enforce, read off the field the implementation
    /// actually populates rather than a parallel recomputation of it.
    #[test]
    fn the_committed_sphere_realizes_at_least_the_requested_wall() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let settings = registered_settings();
        let candidate =
            create_shell(source, settings).expect("the frozen Sphere is hollow-eligible");
        let realized = candidate.cavity().realized_wall_mm().get();
        assert!(
            realized >= settings.wall_thickness_mm().get(),
            "realized wall {realized} mm fell below the requested {} mm",
            settings.wall_thickness_mm().get()
        );
    }

    /// **Test 2 of the FU-55 ruling: the check fires on real geometry,
    /// before the Boolean.** FU-53's own margin is what keeps a real
    /// production shell's realized wall above the requested one; force it to
    /// zero here -- a purely locally-built `HollowSettings`, never a change
    /// to the registered `HOLLOW_CAVITY_MARCH_MARGIN_MM` -- to reproduce the
    /// exact defect FU-53 fixed (the surface-nets extractor emitting the
    /// cavity boundary up to one voxel diagonal outside the selected voxel,
    /// on smooth curved primitives specifically -- FU-53's own diagnosis) and
    /// confirm FU-55's independent measurement catches it. `create_shell`'s
    /// own doc names a locally-built `HollowSettings` the sanctioned way to
    /// exercise non-registry settings in tests.
    ///
    /// The Boolean stage's own admission limits are starved to zero
    /// alongside it: if the realized-wall check ran *after* the Boolean
    /// difference rather than before it, this would fail with a Boolean
    /// resource refusal instead of `RealizedWallBelowSpec`, so a passing
    /// assertion on the refusal's own variant is also a proof of ordering.
    #[test]
    fn a_zero_margin_shell_on_the_sphere_is_refused_before_the_starved_boolean_stage() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let registered = registered_settings();
        let settings = finite_settings(
            registered.wall_thickness_mm().get(),
            registered.voxel_pitch_mm().get(),
            registered.bulk_min_mm().get(),
            registered.save_min_mm3().get(),
            registered.save_min_fraction().get(),
            registered.volume_balance_rel_tol().get(),
            0.0,
        )
        .expect("locally built zero-margin settings");

        let mut limits = H0AdmissionLimits::production();
        limits.max_boolean_input_primitive_units = 0;
        limits.max_boolean_marshaled_input_bytes = 0;
        limits.max_boolean_output_vertices = 0;
        limits.max_boolean_output_triangles = 0;

        let error = create_shell_with_admission_limits(source, settings, limits)
            .expect_err("a zero-margin shell on the sphere's smooth curvature must measure under wall");
        match error {
            HollowError::RealizedWallBelowSpec {
                realized_wall_mm,
                requested_wall_mm,
            } => {
                assert_eq!(requested_wall_mm, registered.wall_thickness_mm().get());
                assert!(
                    realized_wall_mm < requested_wall_mm,
                    "the refusal's own numbers must be inconsistent with shipping: \
                     {realized_wall_mm} mm >= {requested_wall_mm} mm"
                );
            }
            other => panic!(
                "expected FU-55's realized-wall refusal ahead of the starved Boolean stage, got {other:?}"
            ),
        }
    }

    /// **Test 3 of the FU-55 ruling: the fallback wiring, direct.** `run`'s
    /// own failure-to-solid conversion, exercised against a hand-built
    /// [`HollowStageFailure`] rather than geometry that reproduces it (the
    /// previous test already proves the geometry side reliably reaches this
    /// exact error). A refusal must never propagate as a model failure: the
    /// output is `HollowStageOutput::Solid` with
    /// [`SolidDecisionReason::ShellUnderWalled`], the mesh is the oriented
    /// solid handed in, unchanged, and the report carries the refusal's own
    /// measurement.
    #[test]
    fn run_converts_the_refusal_into_a_shell_under_walled_solid_decision() {
        let mesh = make_box([10.0; 3], [0.0; 3]).expect("closed fixture");
        let expected_digest = mesh_fingerprint(&mesh).expect("fixture fingerprint");
        let history = orientation_history();
        let failure = HollowStageFailure::before_cavity_repair(HollowError::RealizedWallBelowSpec {
            realized_wall_mm: 1.9,
            requested_wall_mm: 2.5,
        });
        let requested_wall_mm = Finite::new(2.5).expect("finite");

        let output = convert_shell_failure_to_solid(mesh, history, failure, requested_wall_mm)
            .expect("a realized-wall refusal must never propagate as a model failure");
        let HollowStageOutput::Solid(model) = output else {
            panic!("expected the oriented-solid fallback, got AwaitingInternalSupport");
        };
        assert_eq!(
            model.history().hollow().disposition(),
            HollowDisposition::Solid {
                reason: SolidDecisionReason::ShellUnderWalled
            }
        );
        assert_eq!(
            mesh_fingerprint(model.mesh()).expect("solid fingerprint"),
            expected_digest,
            "the oriented solid must reach the record unchanged"
        );
        assert_eq!(
            model.history().hollow().realized_wall_mm(),
            Some(Finite::new(1.9).expect("finite"))
        );
        // FU-37: the requested wall reaches the report even on a discarded
        // shell -- the whole point of the field is that it does not depend
        // on the shell surviving.
        assert_eq!(model.history().hollow().wall(), Some(requested_wall_mm));
    }

    /// **Test 5 of the FU-55 ruling, half of it: evidence continuity.** A
    /// hand-built [`HollowReport::solid_decision`] carries no shell
    /// measurement -- there was no shell -- and `realized_wall_mm` says so.
    #[test]
    fn realized_wall_mm_is_none_on_a_hand_built_solid_decision() {
        assert_eq!(
            HollowReport::solid_decision(SolidDecisionReason::NoCavity).realized_wall_mm(),
            None
        );
    }

    /// **Test 5 of the FU-55 ruling, the other half: serde compatibility.**
    /// A report frozen before this field existed has to keep deserializing,
    /// exactly the pattern `cavity_repair` already established --
    /// `#[serde(default)]` reads a missing key as `None` rather than
    /// refusing the whole record.
    #[test]
    fn realized_wall_mm_round_trips_and_defaults_on_an_old_report() {
        let with_wall = HollowReport::solid_decision(SolidDecisionReason::NoCavity);
        let json = serde_json::to_string(&with_wall).expect("serialize");
        let round_tripped: HollowReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round_tripped.realized_wall_mm(), with_wall.realized_wall_mm());

        let old_report_json = r#"{
            "disposition": {"kind": "solid", "reason": "no_cavity"},
            "cavities": [],
            "drains": [],
            "drilling_evidence": null,
            "drain_plan": null
        }"#;
        let old_report: HollowReport =
            serde_json::from_str(old_report_json).expect("a report predating this field must still parse");
        assert_eq!(old_report.realized_wall_mm(), None);
        assert_eq!(old_report.cavity_repair(), None);
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

    /// **Test 6 of the FU-55 ruling: thread sweep.** The measured minimum is
    /// a SERIAL reduction over an index-parallel query batch (the repo
    /// forbids a float reduction across parallel work items); this asserts
    /// that discipline actually holds by measuring the same committed
    /// Sphere shell's `realized_wall_mm` at 1, 4 and 16 `PLINTH_THREADS` and
    /// requiring bit-identical results.
    #[test]
    fn realized_wall_mm_is_bit_identical_across_one_four_and_sixteen_kernel_workers() {
        let _guard = thread_guard().lock().expect("thread environment lock");
        let _restore = RestoreThreads(std::env::var_os("PLINTH_THREADS"));
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let settings = registered_settings();
        let mut baseline: Option<f64> = None;
        for workers in [1, 4, 16] {
            std::env::set_var("PLINTH_THREADS", workers.to_string());
            assert_eq!(geom_kernel::current_threads(), workers);
            let candidate = create_shell(source.clone(), settings)
                .expect("the frozen Sphere is hollow-eligible at every worker count");
            let realized = candidate.cavity().realized_wall_mm().get();
            if let Some(expected) = baseline {
                assert_eq!(
                    realized.to_bits(),
                    expected.to_bits(),
                    "worker count {workers} changed the measured realized wall"
                );
            } else {
                baseline = Some(realized);
            }
        }
    }
}

/// FU-37: the requested wall is persisted on the top-level [`HollowReport`],
/// on every production path.
#[cfg(test)]
mod fu37_wall_field {
    use super::*;
    use plinth_mesh::{load_binary_stl, make_box};
    use plinth_orient::{
        BaselineSelection, OrientationProvenance, OrientationReport, RigidTransform,
    };
    use std::path::PathBuf;

    fn frozen_mesh(file: &str) -> Mesh {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline")
            .join(file);
        load_binary_stl(path, WeldPolicy::None).expect("committed frozen STL")
    }

    fn orientation_history() -> OrientationHistory {
        let mesh = make_box([10.0; 3], [0.0; 3]).expect("closed audit fixture");
        let budget = plinth_decimate::DecimationBudget::ratified_default(
            plinth_decimate::ProbeConfig::new(1.0, 64 * 1024 * 1024, 10_000.0),
        )
        .expect("registered fixture budget");
        let outcome = plinth_decimate::run(&mesh, budget).expect("fixture decimation succeeds");
        let (_, audit) = outcome
            .into_decimated_model()
            .expect("fixture scale is in contract")
            .into_parts();
        OrientationHistory::from_reports(
            audit,
            OrientationReport {
                provenance: OrientationProvenance::Pinned {
                    pinned_report: PathBuf::from("baseline/model.pipeline.json"),
                },
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        )
    }

    fn registered_printer() -> PrinterProfile {
        m0_constants()
            .expect("registered constants")
            .saturn4_ultra()
            .clone()
    }

    /// `run`'s own production wiring, replicated exactly (a shell candidate
    /// on the happy path is drained and drilled; a refusal is converted to a
    /// solid decision) without needing a `plinth_orient::OrientedModel`,
    /// which this crate has no way to construct directly. Either branch ends
    /// in a `HollowReport`, which is all this module's tests examine.
    fn build_report(source: Mesh, settings: HollowSettings) -> HollowReport {
        match create_shell_recording_repair(
            source.clone(),
            settings,
            H0AdmissionLimits::production(),
        ) {
            Ok(candidate) => {
                let planned = drains::plan_drains(candidate).expect("drain plan succeeds");
                let drilled = drilling::drill_planned_shell(planned).expect("drilling succeeds");
                drilled
                    .into_hollowed_model(orientation_history())
                    .history()
                    .hollow()
                    .clone()
            }
            Err(failure) => {
                let output = convert_shell_failure_to_solid(
                    source,
                    orientation_history(),
                    failure,
                    settings.wall_thickness_mm(),
                )
                .expect("a hollow-stage refusal must never propagate as a model failure");
                output.report().clone()
            }
        }
    }

    /// **Test 1 of the FU-37 ruling: the row's own claim, inverted.** Two
    /// artifacts prepared at 2.5 mm and 3.0 mm were "byte-indistinguishable
    /// in metadata" before this field existed. They are not anymore: this
    /// builds the committed Sphere's report at both registered walls through
    /// exactly `run`'s own production wiring and asserts the two reports —
    /// and their serialized JSON — differ.
    #[test]
    fn two_artifacts_prepared_at_different_walls_are_distinguishable_in_serialized_metadata() {
        let source = frozen_mesh("Sphere.prepped.oriented-solid.stl");
        let default_request = HollowRequest::with_registered_wall_default(registered_printer())
            .expect("registered default wall request");
        let heavy_request =
            HollowRequest::with_registered_handling_heavy_wall(registered_printer())
                .expect("registered heavy wall request");
        let default_settings =
            HollowSettings::from_request(&default_request).expect("default settings");
        let heavy_settings = HollowSettings::from_request(&heavy_request).expect("heavy settings");

        let default_report = build_report(source.clone(), default_settings);
        let heavy_report = build_report(source, heavy_settings);

        assert_eq!(
            default_report.wall(),
            Some(Finite::new(2.5).expect("finite"))
        );
        assert_eq!(heavy_report.wall(), Some(Finite::new(3.0).expect("finite")));
        assert_ne!(
            default_report.wall(),
            heavy_report.wall(),
            "two artifacts prepared at different walls must differ in the requested wall they record"
        );

        let default_json = serde_json::to_string(&default_report).expect("serialize default");
        let heavy_json = serde_json::to_string(&heavy_report).expect("serialize heavy");
        assert_ne!(
            default_json, heavy_json,
            "two artifacts prepared at 2.5mm and 3.0mm must be byte-distinguishable in metadata"
        );
        assert!(
            default_json.contains("\"wall\":2.5"),
            "default report JSON missing the expected wall: {default_json}"
        );
        assert!(
            heavy_json.contains("\"wall\":3.0"),
            "heavy report JSON missing the expected wall: {heavy_json}"
        );
    }

    /// **Test 2 of the FU-37 ruling: every production path populates it.**
    /// The hollowed-and-drilled path and every typed
    /// [`SolidDecisionReason`] `run` converts a failure into all carry
    /// `Some(wall)`; only the hand-built [`HollowReport::solid_decision`]
    /// carries `None`.
    #[test]
    fn every_typed_solid_decision_reason_run_produces_carries_the_requested_wall() {
        let requested_wall_mm = Finite::new(2.5).expect("finite");
        let mesh = || make_box([10.0; 3], [0.0; 3]).expect("closed fixture");

        let cases = [
            (
                HollowError::IneligibleNoCavity,
                SolidDecisionReason::NoCavity,
            ),
            (
                HollowError::IneligibleBelowBulk,
                SolidDecisionReason::BelowThreshold,
            ),
            (
                HollowError::IneligibleSavings,
                SolidDecisionReason::BelowThreshold,
            ),
            (
                HollowError::CavityRepairRemovedCavity {
                    critical_cells: 1,
                    repaired_voxels: 1,
                    cell_visits: 1,
                    full_grid_passes: 1,
                },
                SolidDecisionReason::NoCavityAfterRepair,
            ),
            (
                HollowError::IneligibleSavingsAfterCavityRepair {
                    critical_cells: 1,
                    repaired_voxels: 1,
                    cell_visits: 1,
                    full_grid_passes: 1,
                },
                SolidDecisionReason::BelowThresholdAfterRepair,
            ),
            (
                HollowError::RealizedWallBelowSpec {
                    realized_wall_mm: 1.9,
                    requested_wall_mm: 2.5,
                },
                SolidDecisionReason::ShellUnderWalled,
            ),
        ];

        for (error, expected_reason) in cases {
            let failure = HollowStageFailure::before_cavity_repair(error);
            let output = convert_shell_failure_to_solid(
                mesh(),
                orientation_history(),
                failure,
                requested_wall_mm,
            )
            .expect("every typed conversion must never propagate as a model failure");
            let HollowStageOutput::Solid(model) = output else {
                panic!("expected a solid decision, got AwaitingInternalSupport");
            };
            assert_eq!(
                model.history().hollow().disposition(),
                HollowDisposition::Solid {
                    reason: expected_reason
                }
            );
            assert_eq!(
                model.history().hollow().wall(),
                Some(requested_wall_mm),
                "{expected_reason:?} did not carry the requested wall"
            );
        }

        // The hand-built constructor is the one path that legitimately has
        // no request behind it, and must keep saying so.
        assert_eq!(
            HollowReport::solid_decision(SolidDecisionReason::NoCavity).wall(),
            None
        );
    }

    /// **Test 3 of the FU-37 ruling: backward compatibility.** A `HollowReport`
    /// serialized before this field existed has no `wall` key at all; it must
    /// still deserialize, reading as `None` rather than tripping
    /// `deny_unknown_fields` or refusing to parse.
    #[test]
    fn a_report_predating_the_wall_field_still_deserializes() {
        let old_report_json = r#"{
            "disposition": {"kind": "solid", "reason": "no_cavity"},
            "cavities": [],
            "drains": [],
            "drilling_evidence": null,
            "drain_plan": null
        }"#;
        let old_report: HollowReport = serde_json::from_str(old_report_json)
            .expect("a report predating this field must still parse");
        assert_eq!(old_report.wall(), None);
        // The fields FU-53/FU-54/FU-55 added the same way must still default
        // too -- this fixture predates all of them.
        assert_eq!(old_report.cavity_repair(), None);
        assert_eq!(old_report.realized_wall_mm(), None);
    }

    /// **Test 4 of the FU-37 ruling: round trip.** Serializing and
    /// deserializing a report that does carry a wall reproduces the exact
    /// value, `Finite`'s bit pattern included.
    #[test]
    fn wall_round_trips_through_serialization() {
        let requested_wall_mm = Finite::new(2.5).expect("finite");
        let report = HollowedModel::solid_decision_recording_repair(
            frozen_mesh("Sphere.prepped.oriented-solid.stl"),
            orientation_history(),
            SolidDecisionReason::BelowThreshold,
            None,
            Some(requested_wall_mm),
        )
        .expect("a closed oriented solid is a valid reserve")
        .history()
        .hollow()
        .clone();

        let json = serde_json::to_string(&report).expect("serialize");
        let round_tripped: HollowReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(round_tripped.wall(), report.wall());
        assert_eq!(
            round_tripped.wall().expect("wall present").get().to_bits(),
            requested_wall_mm.get().to_bits(),
            "Finite must preserve its exact bits across the round trip"
        );
    }
}
