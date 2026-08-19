//! The pre-write physics gate: the only door between a sealed support run and
//! a `.chitubox` file.
//!
//! Until this module existed nothing consulted a physics verdict before
//! writing an artifact. `plinth_emit::write` took no verdict,
//! `plinth_verify::verify_coverage` had no non-test caller,
//! `EMIT_REQUIRE_PASSED_GATES_DEFAULT` had no reader, and
//! [`PipelineExitCode::Gates`] was reserved and unproduced. A model that could
//! not be supported still reached disk, and the only gate that ran ran *after*
//! the write, on the bytes.
//!
//! **There is no override.** `03-physics-and-constants.md` retires
//! `PERF_GATE_EMIT_ON_VERIFY` — "the false compatibility switch does not
//! survive: Rust always refuses blocking defects and canonical
//! `EMIT_REQUIRE_PASSED_GATES_DEFAULT=true` is the sole emission gate" — so
//! this module takes no force flag, no allow-list and no waiver, and
//! `CoverageReport::accepted()` it consults is itself non-overridable.
//!
//! **The refusal happens before the file exists.** The gate runs against the
//! sealed verification snapshot, which borrows the authority; only once every
//! gate has spoken is the authority consumed by `into_supported_model`, and
//! only then is `plinth_emit::write` called. A refused model therefore leaves
//! no path on disk at all — not a truncated one, not an empty one.
//!
//! That claim is about the *path*, not merely about this call, so
//! [`judge_then_emit`] also removes anything already sitting at the target on
//! every refusal. Two ways an artifact could otherwise survive a refusal, both
//! of which were reachable: `plinth_emit::write` used to save the project
//! before auditing it, so a disk-audit rejection left the file it had just
//! refused (it now stages and renames, and cleans up); and a *stale* artifact
//! from an earlier run was never touched, so a model that emitted yesterday
//! and is gate-refused today would still have had yesterday's file at the
//! target. The acceptance test is "the output path does not exist", and it is
//! the path this stage owns.
//!
//! [`PipelineExitCode::Gates`]: crate::PipelineExitCode::Gates

use plinth_core::{m0_constants, ArtifactId, ModelId};
use plinth_emit::{write as emit_write, EmitReport, EmitRequest};
use plinth_support::{
    CoverageMeasurementError, CoverageMeasurementRequest, FinalSupportAuthority,
    SupportVerificationError, SupportVerificationSnapshot,
};
use plinth_verify::{verify_coverage, CoverageReport, CoverageVerificationError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::PipelineError;

/// The twelve gates a candidate must pass before emission.
///
/// The list is `00-migration-guide.md` §"ported current verifier safety gates"
/// verbatim and in its stated order. Exactly one of them —
/// [`Self::UncoveredIslands`] — has a ported implementation today; the other
/// eleven are named here rather than left implicit, so that "which gates does
/// emission actually consult?" has one answer in the type system instead of
/// eleven silent omissions. [`judge`] matches this enum exhaustively and takes
/// no `_` arm, so a thirteenth gate is a compile error, not a gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreWriteGate {
    UncoveredIslands,
    Clearance,
    Load,
    UnsupportedMinima,
    IslandBirths,
    RegularContactInvariants,
    UnroutedSpecializedContacts,
    Connectivity,
    Printability,
    Murray,
    BallBreach,
    PhysicalContactInvariant,
}

impl PreWriteGate {
    /// Every gate, in the guide's order. Emission consults all of them.
    pub const ALL: [Self; 12] = [
        Self::UncoveredIslands,
        Self::Clearance,
        Self::Load,
        Self::UnsupportedMinima,
        Self::IslandBirths,
        Self::RegularContactInvariants,
        Self::UnroutedSpecializedContacts,
        Self::Connectivity,
        Self::Printability,
        Self::Murray,
        Self::BallBreach,
        Self::PhysicalContactInvariant,
    ];
}

/// What one gate said.
#[derive(Clone, Debug, PartialEq)]
enum GateVerdict {
    /// The gate ran against this snapshot and found nothing blocking.
    Passed(Box<CoverageReport>),
    /// The gate is named and wired into the refusal path, but its verifier has
    /// not been ported yet. It cannot pass or fail a model; it is reported so
    /// that a caller can see the gate set is incomplete.
    NotYetPorted,
}

/// The gate set's verdict on one sealed run.
///
/// `pending` is not decoration: it is the honest statement that emission is
/// authorized on one gate out of twelve today, carried out to the caller
/// rather than described in a comment.
#[derive(Clone, Debug, PartialEq)]
pub struct PreWriteGateReport {
    coverage: CoverageReport,
    pending: Vec<PreWriteGate>,
}

impl PreWriteGateReport {
    /// The one gate with a ported implementation, and its full evidence.
    #[must_use]
    pub const fn coverage(&self) -> &CoverageReport {
        &self.coverage
    }

    /// The gates that are named and refused-through but not yet ported,
    /// ascending in [`PreWriteGate::ALL`] order.
    #[must_use]
    pub fn pending(&self) -> &[PreWriteGate] {
        &self.pending
    }
}

/// Why emission was refused. Every variant maps to
/// [`PipelineExitCode::Gates`](crate::PipelineExitCode::Gates).
#[derive(Clone, Debug, PartialEq, Error)]
pub enum GateRefusal {
    /// The registered emission authorization is not enabled.
    ///
    /// This is the read of `EMIT_REQUIRE_PASSED_GATES_DEFAULT`, and it can only
    /// ever *refuse*: the retired `PERF_GATE_EMIT_ON_VERIFY` was the switch
    /// that let a run emit without a verdict, and it does not survive. A
    /// registry that ever published `false` here would be describing an
    /// emission mode this pipeline does not implement, so it refuses instead of
    /// quietly skipping the gates.
    #[error(
        "the registered emission authorization is not enabled; \
         PERF_GATE_EMIT_ON_VERIFY is retired and unverified emission does not exist"
    )]
    EmissionAuthorizationNotEnabled,
    /// The ratified constants registry could not be read at all.
    #[error("the ratified M0 constants authority is unavailable to the pre-write gates")]
    ConstantsUnavailable,
    /// The production support resource policy could not be assembled, so the
    /// gate has no ceilings to measure under.
    #[error("the production support resource policy is unavailable to the pre-write gates: {0}")]
    ResourcePolicyUnavailable(String),
    /// The measurement request could not be built from the registry.
    #[error("the pre-write coverage measurement request is invalid")]
    InvalidCoverageRequest(#[source] CoverageMeasurementError),
    /// The sealed snapshot refused its own integrity check.
    #[error("the sealed support snapshot refused integrity validation before the gates ran")]
    SnapshotIntegrity(#[source] SupportVerificationError),
    /// A gate could not reach a verdict. Fail-closed: an unmeasurable gate
    /// refuses emission exactly as a failed one does.
    #[error("gate {gate:?} could not be measured")]
    Unmeasurable {
        gate: PreWriteGate,
        #[source]
        source: CoverageVerificationError,
    },
    /// A gate reached a verdict and it was blocking.
    #[error("gate {gate:?} refused emission with {} blocking defect(s)", report.defects().len())]
    Refused {
        gate: PreWriteGate,
        report: Box<CoverageReport>,
    },
}

impl GateRefusal {
    /// The gate that refused, when one gate owns the refusal.
    #[must_use]
    pub const fn gate(&self) -> Option<PreWriteGate> {
        match self {
            Self::Unmeasurable { gate, .. } | Self::Refused { gate, .. } => Some(*gate),
            Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_) => None,
        }
    }

    /// The measurement error behind an UNMEASURABLE **coverage** gate, when
    /// that is what this refusal is.
    ///
    /// FU-57 keeps evidence on every remaining coverage refusal, and an
    /// unmeasurable coverage gate is one: fail-closed, model refused, the
    /// coverage gate named, no artifact. There is no [`CoverageReport`] to
    /// persist — that is what unmeasurable means — but there is a reason, and
    /// the reason is the evidence. Scoped to gates that actually measure
    /// coverage, so a future non-coverage gate's measurement failure does not
    /// silently start writing a coverage sidecar.
    #[must_use]
    pub fn unmeasurable_coverage_reason(&self) -> Option<String> {
        match self {
            Self::Unmeasurable {
                gate: PreWriteGate::UncoveredIslands,
                source,
            } => Some(source.to_string()),
            Self::Unmeasurable { .. }
            | Self::Refused { .. }
            | Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_) => None,
        }
    }

    /// The full evidence behind a blocking verdict, when there is one.
    #[must_use]
    pub fn report(&self) -> Option<&CoverageReport> {
        match self {
            Self::Refused { report, .. } => Some(report),
            Self::EmissionAuthorizationNotEnabled
            | Self::ConstantsUnavailable
            | Self::ResourcePolicyUnavailable(_)
            | Self::InvalidCoverageRequest(_)
            | Self::SnapshotIntegrity(_)
            | Self::Unmeasurable { .. } => None,
        }
    }
}

/// The one coverage measurement the pre-write gate makes.
///
/// Both geometric thresholds are the *planner's own*, read from the same
/// registered constants `plinth-support` builds its `ContactSelectionConfig`
/// from: the radius is `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` and the
/// patch-area floor is `VOXEL_PITCH_MM²`, exactly as
/// `specialized::consolidate` derives them for its own coverage question. The
/// gate therefore measures the promise the planner made, and introduces no
/// second, independently-chosen coverage physics. The ceilings come from the
/// same production support policy, through
/// [`SupportPreparationLimits::coverage_measurement_limits`].
///
/// **A thin delegation to `plinth_support::registered_coverage_request`
/// (FU-56).** That is now the one implementation: the coverage-closure
/// controller and this gate must measure under the identical request, or
/// closing coverage in the controller would not guarantee the gate agrees.
/// This wrapper exists only to keep this crate's own [`GateRefusal`] mapping
/// at the door, rather than leaking `plinth-support`'s setup error type into
/// the gate's public error surface.
///
/// # Errors
///
/// Returns [`GateRefusal::ConstantsUnavailable`] /
/// [`GateRefusal::ResourcePolicyUnavailable`] if the registry cannot be read
/// and [`GateRefusal::InvalidCoverageRequest`] if the registered values do not
/// form a valid request.
pub fn registered_coverage_request() -> Result<CoverageMeasurementRequest, GateRefusal> {
    plinth_support::registered_coverage_request().map_err(|error| match error {
        plinth_support::CoverageClosureSetupError::RegisteredConstantsUnavailable(_) => {
            GateRefusal::ConstantsUnavailable
        }
        plinth_support::CoverageClosureSetupError::ResourcePolicyUnavailable(detail) => {
            GateRefusal::ResourcePolicyUnavailable(detail)
        }
        plinth_support::CoverageClosureSetupError::InvalidCoverageRequest(source) => {
            GateRefusal::InvalidCoverageRequest(source)
        }
    })
}

/// Judge one gate against the sealed snapshot.
///
/// Matched exhaustively and deliberately without a `_` arm: adding a gate to
/// [`PreWriteGate`] without deciding what it does here is a compile error.
fn judge(
    gate: PreWriteGate,
    artifact: &ArtifactId,
    snapshot: &SupportVerificationSnapshot<'_>,
    request: CoverageMeasurementRequest,
) -> Result<GateVerdict, GateRefusal> {
    match gate {
        PreWriteGate::UncoveredIslands => {
            let report = verify_coverage(artifact.clone(), snapshot, request)
                .map_err(|source| GateRefusal::Unmeasurable { gate, source })?;
            if report.accepted() {
                Ok(GateVerdict::Passed(Box::new(report)))
            } else {
                Err(GateRefusal::Refused {
                    gate,
                    report: Box::new(report),
                })
            }
        }
        // TODO(gate-port): each of these is a live gate in the frozen current
        // verifier and is listed as binding by `00-migration-guide.md`. None
        // has a ported Rust implementation yet, so none can reach a verdict.
        // They are named here, in the refusal path, so that the gap is a
        // reported value rather than an omission — see
        // `PreWriteGateReport::pending`.
        PreWriteGate::Clearance
        | PreWriteGate::Load
        | PreWriteGate::UnsupportedMinima
        | PreWriteGate::IslandBirths
        | PreWriteGate::RegularContactInvariants
        | PreWriteGate::UnroutedSpecializedContacts
        | PreWriteGate::Connectivity
        | PreWriteGate::Printability
        | PreWriteGate::Murray
        | PreWriteGate::BallBreach
        | PreWriteGate::PhysicalContactInvariant => Ok(GateVerdict::NotYetPorted),
    }
}

/// Run every pre-write gate against one sealed snapshot.
///
/// **Deliberately private.** It takes a caller-supplied
/// [`CoverageMeasurementRequest`], and a caller-supplied radius is an override
/// in all but name — pass a large enough one and every layout "covers". Public,
/// it would have been the override [`judge_then_emit`] refuses to be, and
/// [`PreWriteGateReport`] is constructible only here, so leaving it public
/// would have made the gate set's verdict forgeable from outside the crate.
/// The one public entry point is `judge_then_emit`, which derives the request
/// from the registry.
///
/// # Errors
///
/// Returns the first [`GateRefusal`] in [`PreWriteGate::ALL`] order. The
/// authorization read and the snapshot integrity check precede every gate, so
/// an unauthorized or incoherent run refuses before any measurement runs.
fn judge_pre_write_gates(
    artifact: &ArtifactId,
    snapshot: &SupportVerificationSnapshot<'_>,
    request: CoverageMeasurementRequest,
) -> Result<PreWriteGateReport, GateRefusal> {
    // The sole read of EMIT_REQUIRE_PASSED_GATES_DEFAULT. It is not an
    // override: `true` runs the gates, and anything else refuses outright.
    let authorized = *m0_constants()
        .map_err(|_| GateRefusal::ConstantsUnavailable)?
        .placeholders()
        .emit_require_passed_gates_default()
        .diagnostic();
    if !authorized {
        return Err(GateRefusal::EmissionAuthorizationNotEnabled);
    }

    snapshot
        .validate_integrity()
        .map_err(GateRefusal::SnapshotIntegrity)?;

    let mut coverage = None;
    let mut pending = Vec::new();
    for gate in PreWriteGate::ALL {
        match judge(gate, artifact, snapshot, request)? {
            GateVerdict::Passed(report) => coverage = Some(*report),
            GateVerdict::NotYetPorted => pending.push(gate),
        }
    }
    let coverage = coverage.ok_or(GateRefusal::Unmeasurable {
        gate: PreWriteGate::UncoveredIslands,
        source: CoverageVerificationError::SnapshotBinding,
    })?;
    Ok(PreWriteGateReport { coverage, pending })
}

/// Everything one gated emission produced, measured rather than assumed.
#[derive(Clone, Debug)]
pub struct GatedEmission {
    pub gates: PreWriteGateReport,
    /// Task #49. Every island this run's accepted contact set leaves with no
    /// owning contact, read off the authority before it is consumed. Empty on
    /// the ordinary model; non-empty means the artifact below covers an island
    /// nothing is charged to carry, forgiven by the registered censor on the
    /// measurement this same run took.
    pub ownerless_islands: Vec<plinth_support::OwnerlessIslandDisclosure>,
    pub routed_contacts: usize,
    pub final_live_members: usize,
    pub snapshot_digest: u64,
    pub primitive_count: usize,
    pub lift_mm: f64,
    pub plate_z_mm: f64,
    pub emit: EmitReport,
}

/// The stage runner: judge, then lower, then write — in that order, once.
///
/// This is the sequence `prepare()` will call per model, and it is the reason
/// `PipelineExitCode::Gates` can now be produced. The gate reads the snapshot
/// while the authority is still alive; only a passing verdict reaches
/// `into_supported_model`, which consumes it; only then does
/// `plinth_emit::write` create the target path. A gate refusal therefore
/// returns with **no file written at `request.target_path`**.
///
/// **It takes no gate parameter of any kind**, deliberately: not a force flag,
/// not a waiver, and not the coverage measurement request either. A
/// caller-supplied radius would be an override in all but name — pass a large
/// enough one and every layout "covers" — so the request is derived here from
/// the registry, through [`registered_coverage_request`]. The one knob is what
/// the constants say, and the constants are the ratified authority. The
/// request-taking [`judge_pre_write_gates`] is private for the same reason,
/// which makes this the only public door to the gate set.
///
/// **On every error path `request.target_path` is left nonexistent**, including
/// a path that was already occupied when this was called. See the module
/// documentation: the invariant is about the path, not about this call, and a
/// stale artifact at a refused model's target is exactly the misleading
/// hand-off the gate exists to prevent.
///
/// # Errors
///
/// - [`PipelineError::Gates`] (exit code 3) when a gate refuses or cannot be
///   measured, before any write.
/// - [`PipelineError::Lowering`] (exit code 5) when lowering the passed
///   authority fails.
/// - [`PipelineError::Emit`] (exit code 4) when the write or its disk audit
///   fails.
pub fn judge_then_emit(
    model: &ModelId,
    authority: FinalSupportAuthority<'_>,
    request: &EmitRequest,
) -> Result<GatedEmission, PipelineError> {
    match judged_emission(model, authority, request) {
        Ok(emission) => Ok(emission),
        Err(error) => {
            // The invariant is "a refused model leaves no path on disk", and
            // this stage owns the path. `plinth_emit::write` already cleans up
            // after itself; what only this scope can clean up is an artifact
            // an *earlier* run left at the same target, which a gate refusal
            // never reaches `write` to remove.
            let _ = std::fs::remove_file(&request.target_path);
            Err(error)
        }
    }
}

fn judged_emission(
    model: &ModelId,
    authority: FinalSupportAuthority<'_>,
    request: &EmitRequest,
) -> Result<GatedEmission, PipelineError> {
    let artifact = ArtifactId::new(model.as_str());
    let coverage = registered_coverage_request().map_err(|source| PipelineError::Gates {
        model: model.clone(),
        source,
    })?;
    // The snapshot borrows the authority, so this scope must close before the
    // authority can be consumed below. That ordering is the whole point: the
    // verdict is reached while the run is still un-lowered and un-written.
    // Task #49: read while the authority is still alive — `into_supported_model`
    // below consumes it, and the supported scene carries no §6 ledger.
    let ownerless_islands = authority.ownerless_islands().to_vec();
    let (gates, routed_contacts, final_live_members, snapshot_digest) = {
        let snapshot =
            authority
                .verification_snapshot()
                .map_err(|source| PipelineError::Gates {
                    model: model.clone(),
                    source: GateRefusal::SnapshotIntegrity(source),
                })?;
        let gates = judge_pre_write_gates(&artifact, &snapshot, coverage).map_err(|source| {
            PipelineError::Gates {
                model: model.clone(),
                source,
            }
        })?;
        (
            gates,
            snapshot.routed_contacts().count(),
            snapshot.final_live_members().count(),
            snapshot.digest(),
        )
    };

    let supported = authority
        .into_supported_model()
        .map_err(|source| PipelineError::Lowering {
            model: model.clone(),
            source,
        })?;

    let emit = emit_write(&supported, request).map_err(|source| PipelineError::Emit {
        model: model.clone(),
        source,
    })?;

    Ok(GatedEmission {
        gates,
        ownerless_islands,
        routed_contacts,
        final_live_members,
        snapshot_digest,
        primitive_count: supported.primitives().len(),
        lift_mm: supported.lift_mm().get(),
        plate_z_mm: supported.plate_z_mm().get(),
        emit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plinth_support::SupportPreparationLimits;

    #[test]
    fn every_named_gate_appears_exactly_once_in_the_consulted_set() {
        let mut sorted = PreWriteGate::ALL.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            PreWriteGate::ALL.len(),
            "a gate is listed twice, so one of them is never consulted"
        );
        assert_eq!(PreWriteGate::ALL[0], PreWriteGate::UncoveredIslands);
    }

    #[test]
    fn the_registered_emission_authorization_is_enabled_and_readable() {
        // The constant now has a reader, and this is the assertion that it
        // says what `03-physics-and-constants.md` says it must.
        assert!(
            *m0_constants()
                .expect("registered constants")
                .placeholders()
                .emit_require_passed_gates_default()
                .diagnostic(),
            "EMIT_REQUIRE_PASSED_GATES_DEFAULT is the sole emission gate and must be true"
        );
    }

    /// **FU-57 guard: evidence is never a gate input.** The sidecar/card/
    /// render writer lives in `prepare.rs`, at the two call sites that
    /// already hold a refusal; this module must not import that module, or
    /// "evidence, never an input" stops being true by construction and
    /// starts being true only by convention. `include_str!` embeds this
    /// file's own source at compile time, so the guard cannot go stale by
    /// reading the wrong copy.
    ///
    /// The forbidden module name is assembled at runtime, deliberately never
    /// spelled out as one literal anywhere in this file -- spelling it out
    /// would make this very doc comment trip its own guard the moment
    /// `include_str!` embeds it.
    #[test]
    fn gates_module_source_never_imports_the_evidence_module() {
        let forbidden = ["coverage", "_", "evidence"].concat();
        let source = include_str!("gates.rs");
        assert!(
            !source.contains(&forbidden),
            "gates.rs must never import the FU-57 evidence writer -- judge_pre_write_gates \
             stays private and unchanged, and the verdict must be decided before it is \
             explained, never the other way round"
        );
    }

    #[test]
    fn the_registered_coverage_request_is_the_planners_own_radius_and_floor() {
        let limits = SupportPreparationLimits::production().expect("production limits");
        let request = registered_coverage_request().expect("registered request");
        let constants = m0_constants().expect("registered constants");
        let policy = constants.placeholders();
        assert_eq!(
            request.radius_mm().get(),
            policy.coverage_radius_factor().diagnostic().get()
                * policy.target_spacing_mm().diagnostic().get()
        );
        assert_eq!(
            request.patch_area_floor_mm2().get(),
            policy.voxel_pitch_mm().diagnostic().get().powi(2)
        );
        // And the ceilings are the support policy's, not this module's.
        assert_eq!(
            request.limits(),
            limits
                .coverage_measurement_limits()
                .expect("registered ceilings")
        );
    }
}
