//! Sealed support preparation through initial physical-contact construction.
//!
//! This is deliberately not a public `run` entry point. It consumes the move-only hollowing
//! hand-off, executes the accepted planning leaves in their frozen order, and seals their
//! authority together for the later controller. Each native leaf remains batch-first: perception
//! gathers its rays and slices, specialized planning batches projections and gates, and initial
//! construction makes one axis-interval batch.

use std::collections::BTreeSet;

use geom_kernel::{NativeSurface, NativeSurfaceConfig, PenetrationBatchLimits, SurfaceBuildError};
use plinth_core::LayerPlan;
use plinth_hollow::{HollowHistory, HollowedModel};
use plinth_mesh::{mesh_fingerprint, FaceTopology, Mesh};
use thiserror::Error;

use crate::{
    anchor::{AnchorGeometryAuthority, AnchorSearchConfig, AnchorSearchError, SurfaceAnchorPool},
    brace::{brace_fixed_point_current_parity, BraceError, BraceFixedPointPlan},
    cco::{
        CcoController, CcoError, FinalizedAnchorTerminalRoute, FinalizedCcoTranscript,
        TerminalCcoRequest,
    },
    dyadic::CandidatePayloadConfig,
    forest::{Forest, TerminalLedgerState},
    initial::{
        build_initial_contacts, ContactConstructionLimits, InitialContactBuild, InitialContactError,
    },
    perception::{
        prepare_generic_contacts, FinalizedContactLoads, GenericContactPlan,
        GenericPerceptionError, GenericPerceptionLimits, SpecializedOwnerReservation,
    },
    rewire::{optimize as optimize_rewire, RewireError, RewireReport, RewireRequest},
    select_contacts_baseline,
    specialized::{
        prepare_specialized_contacts, SpecializedContactPlan, SpecializedError, SpecializedLimits,
    },
    transient::{transient_report, TransientError, TransientReport},
    validate_perception_input, BuildFrame, ContactSelectionConfig, ContactSelectionError,
    SelectedContacts, SupportPerceptionConfig, SupportPerceptionError, SupportRequest,
};

/// Complete typed caller authority for the sealed preparation stage.
///
/// Construction is crate-private and every field remains immutable. Result-changing values live
/// in their existing registered config types; this type introduces no support-physics defaults.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SupportPreparationAuthority<'a> {
    layers: &'a LayerPlan,
    perception_config: SupportPerceptionConfig<'a>,
    candidate_config: CandidatePayloadConfig,
    reservations: &'a [SpecializedOwnerReservation],
    perception_limits: GenericPerceptionLimits,
    selection_config: ContactSelectionConfig<'a>,
    specialized_limits: SpecializedLimits,
    construction_limits: ContactConstructionLimits,
    brace_penetration_limits: PenetrationBatchLimits,
}

impl<'a> SupportPreparationAuthority<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        layers: &'a LayerPlan,
        perception_config: SupportPerceptionConfig<'a>,
        candidate_config: CandidatePayloadConfig,
        reservations: &'a [SpecializedOwnerReservation],
        perception_limits: GenericPerceptionLimits,
        selection_config: ContactSelectionConfig<'a>,
        specialized_limits: SpecializedLimits,
        construction_limits: ContactConstructionLimits,
        brace_penetration_limits: PenetrationBatchLimits,
    ) -> Self {
        Self {
            layers,
            perception_config,
            candidate_config,
            reservations,
            perception_limits,
            selection_config,
            specialized_limits,
            construction_limits,
            brace_penetration_limits,
        }
    }
}

/// Stable evidence retained at the sealed preparation boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedSupportDigests {
    mesh: String,
    generic: u64,
    regular: u64,
    plan: u64,
    loads: u64,
    geometry: u64,
    initial_forest: u64,
    prepared: u64,
}

impl PreparedSupportDigests {
    pub(crate) fn mesh(&self) -> &str {
        &self.mesh
    }
    pub(crate) const fn generic(&self) -> u64 {
        self.generic
    }
    pub(crate) const fn regular(&self) -> u64 {
        self.regular
    }
    pub(crate) const fn plan(&self) -> u64 {
        self.plan
    }
    pub(crate) const fn loads(&self) -> u64 {
        self.loads
    }
    pub(crate) const fn geometry(&self) -> u64 {
        self.geometry
    }
    pub(crate) const fn initial_forest(&self) -> u64 {
        self.initial_forest
    }
    pub(crate) const fn prepared(&self) -> u64 {
        self.prepared
    }
}

/// Opaque, move-only hand-off to the later support controller.
///
/// The surface and topology are retained rather than reconstructed from raw arrays. The anchor
/// authority itself borrows those values, so safe Rust cannot store it self-referentially; the
/// sealed geometry digest proves that the retained pair is the one used for initial construction.
/// Immutable authority prepared once for a support run.
///
/// This intentionally owns the one retained mesh/topology/native-surface trio and all generic
/// and regular planning authority.  Later route state borrows nothing from a replacement geometry
/// or a rerun preparation pass.
struct SupportRunContext<'a> {
    mesh: Mesh,
    hollow_history: HollowHistory,
    request: SupportRequest,
    frame: BuildFrame,
    authority: SupportPreparationAuthority<'a>,
    topology: FaceTopology,
    surface: NativeSurface,
    generic: GenericContactPlan,
    regular: SelectedContacts,
    brace_penetration_limits: PenetrationBatchLimits,
    digests: PreparedSupportDigests,
}

impl<'a> SupportRunContext<'a> {
    pub(crate) const fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        &self.hollow_history
    }
    pub(crate) const fn request(&self) -> &SupportRequest {
        &self.request
    }
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.frame
    }
    pub(crate) const fn layers(&self) -> &LayerPlan {
        self.authority.layers
    }
    pub(crate) const fn generic(&self) -> &GenericContactPlan {
        &self.generic
    }
    pub(crate) const fn regular(&self) -> &SelectedContacts {
        &self.regular
    }
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        &self.digests
    }

    pub(crate) const fn brace_penetration_limits(&self) -> PenetrationBatchLimits {
        self.brace_penetration_limits
    }

    /// Rebind the retained immutable geometry without cloning or replacing either component.
    pub(crate) fn geometry(&self) -> Result<AnchorGeometryAuthority<'_>, AnchorSearchError> {
        let geometry = AnchorGeometryAuthority::new(&self.mesh, &self.topology, &self.surface)?;
        if geometry.digest() != self.digests.geometry {
            return Err(AnchorSearchError::TopologyMeshMismatch);
        }
        Ok(geometry)
    }
}

/// Opaque, move-only hand-off to the later support controller.
///
/// Its context is immutable and its two prepared plan artifacts are transferred exactly once into
/// the routed state. This preserves the established preparation API without retaining a second
/// geometry/index authority.
pub(crate) struct PreparedSupportRun<'a> {
    context: SupportRunContext<'a>,
    plan: SpecializedContactPlan,
    initial: InitialContactBuild,
}

impl<'a> PreparedSupportRun<'a> {
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        self.context.hollow_history()
    }
    pub(crate) const fn request(&self) -> &SupportRequest {
        self.context.request()
    }
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }
    pub(crate) const fn layers(&self) -> &LayerPlan {
        self.context.layers()
    }
    pub(crate) const fn generic(&self) -> &GenericContactPlan {
        self.context.generic()
    }
    pub(crate) const fn regular(&self) -> &SelectedContacts {
        self.context.regular()
    }
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.plan
    }
    pub(crate) fn loads(&self) -> Result<&FinalizedContactLoads, SpecializedError> {
        self.plan.finalized_loads()
    }
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.initial
    }
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }
    pub(crate) fn geometry(&self) -> Result<AnchorGeometryAuthority<'_>, AnchorSearchError> {
        self.context.geometry()
    }

    /// Consume the prepared authority in the one sealed initial-terminal CCO
    /// transaction.  The frozen initial routing order is the only cadence;
    /// all route-shaped authority is rechecked before the first native query
    /// or forest transition.
    pub(crate) fn route_initial_cco(self) -> Result<RoutedSupportRun<'a>, SupportRoutingError> {
        let PreparedSupportRun {
            context,
            plan,
            mut initial,
        } = self;
        // Bind these three retained fields directly so the independent
        // initial forest remains available for the serial transaction below.
        let geometry =
            AnchorGeometryAuthority::new(&context.mesh, &context.topology, &context.surface)?;
        if geometry.digest() != context.digests.geometry {
            return Err(SupportRoutingError::GeometryDigestMismatch);
        }
        let loads = plan.finalized_loads()?;
        context.validate_initial_cco_preflight(&plan, &initial, &geometry, loads)?;

        // Constructing the exterior pool is a bounded, fail-closed resource
        // staging step. It uses the same required S0 sidecar that produced
        // the contact plan; no runner-local anchor policy exists.
        let delicacy = context
            .authority
            .perception_config
            .delicacy_by_face
            .ok_or(SupportRoutingError::MissingAnchorDelicacy)?;
        let anchor_config = AnchorSearchConfig::current()?;
        CcoController::validate_initial_route_config()?;
        let pool = SurfaceAnchorPool::from_geometry(&geometry, delicacy)?;

        let mut controller = CcoController::default();
        let mut cadence = initial.routing_order().specialized_terminals().to_vec();
        cadence.extend_from_slice(initial.routing_order().regular_terminals());
        for terminal in cadence {
            let ledger = initial
                .forest()
                .terminal_ledger(terminal)
                .ok_or(SupportRoutingError::MissingTerminalLedger { terminal })?;
            if ledger.state == TerminalLedgerState::Quarantined {
                controller.advance_skipped_terminal(initial.forest(), ledger)?;
                continue;
            }

            let request = TerminalCcoRequest::bind(initial.forest(), terminal, context.frame)?;
            let contact = plan
                .contacts()
                .iter()
                .find(|contact| contact.contact_id() == u64::from(terminal))
                .ok_or(SupportRoutingError::MissingPhysicalContact { terminal })?;
            controller.grow_terminal_with_finalized_anchor(
                initial.forest_mut(),
                &request,
                FinalizedAnchorTerminalRoute {
                    plan: &plan,
                    contact,
                    contact_id: u64::from(terminal),
                    loads,
                    pool: &pool,
                    config: anchor_config,
                    geometry: &geometry,
                },
            )?;
        }

        let transcript = controller.finalize_transcript(&initial, &plan, loads, &geometry)?;
        transcript.validate_integrity()?;
        Ok(RoutedSupportRun {
            context,
            state: RoutedPlanState {
                plan,
                initial,
                controller,
                transcript,
            },
        })
    }
}

impl SupportRunContext<'_> {
    fn validate_initial_cco_preflight(
        &self,
        plan: &SpecializedContactPlan,
        initial: &InitialContactBuild,
        geometry: &AnchorGeometryAuthority<'_>,
        loads: &FinalizedContactLoads,
    ) -> Result<(), SupportRoutingError> {
        // This repeats the planner's complete immutable binding before CCO
        // can make any query or forest mutation.  In particular it covers
        // candidate/refusal limits, specialized resource limits, and the
        // plan-owned finalized load artifact rather than accepting digests as
        // a substitute for the real authority.
        plan.validate_binding(
            &self.mesh,
            &self.authority.perception_config,
            self.frame,
            self.authority.layers,
            &self.authority.candidate_config,
            self.authority.reservations,
            self.authority.perception_limits,
            &self.generic,
            &self.regular,
            &self.authority.selection_config,
            self.authority.specialized_limits,
        )?;
        if plan.digest() != self.digests.plan
            || plan.finalized_load_plan_digest() != plan.digest()
            || loads.digest() != self.digests.loads
        {
            return Err(SupportRoutingError::PlanOrLoadDigestMismatch);
        }
        geometry.validate_mesh(&self.mesh)?;
        if geometry.digest() != self.digests.geometry {
            return Err(SupportRoutingError::GeometryDigestMismatch);
        }
        if initial.plan_digest() != plan.digest()
            || initial.load_digest() != loads.digest()
            || initial.native_digest() != geometry.digest()
            || initial.initial_forest_digest() != Forest::new().digest()
            || initial.post_quarantine_forest_digest() != self.digests.initial_forest
            || initial.forest().digest() != self.digests.initial_forest
            || !initial.all_terminals_registered_before_quarantine()
        {
            return Err(SupportRoutingError::InitialAuthorityMismatch);
        }

        let routing = initial.routing_order();
        let mut expected = BTreeSet::new();
        for terminal in routing
            .specialized_terminals()
            .iter()
            .chain(routing.regular_terminals())
            .copied()
        {
            if !expected.insert(terminal)
                || !initial
                    .terminals()
                    .iter()
                    .any(|evidence| evidence.terminal == terminal)
                || plan
                    .contacts()
                    .iter()
                    .filter(|contact| contact.contact_id() == u64::from(terminal))
                    .count()
                    != 1
                || initial.forest().terminal_ledger(terminal).is_none()
            {
                return Err(SupportRoutingError::TerminalNamespaceMismatch { terminal });
            }
        }
        if expected.len() != initial.terminals().len() || expected.len() != plan.contacts().len() {
            return Err(SupportRoutingError::TerminalNamespaceCardinalityMismatch);
        }
        Ok(())
    }
}

/// Route-only state, formed by consuming prepared plan state once.
///
/// Keeping every mutable/terminal artifact together makes the one-way transition explicit:
/// generic preparation and retained native geometry stay in `SupportRunContext`, while routing
/// owns the specialized plan, initial forest, controller, and finalized transcript.
struct RoutedPlanState {
    plan: SpecializedContactPlan,
    initial: InitialContactBuild,
    controller: CcoController,
    transcript: FinalizedCcoTranscript,
}

/// Opaque route-stage authority for the normalizer. It retains the exact prepared context and
/// sealed CCO state; no caller can replay an initial terminal after this value exists.
pub(crate) struct RoutedSupportRun<'a> {
    context: SupportRunContext<'a>,
    state: RoutedPlanState,
}

impl<'a> RoutedSupportRun<'a> {
    #[must_use]
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }

    #[must_use]
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        self.context.hollow_history()
    }

    #[must_use]
    pub(crate) const fn request(&self) -> &SupportRequest {
        self.context.request()
    }

    #[must_use]
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }

    #[must_use]
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.state.plan
    }

    #[must_use]
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.state.initial
    }

    #[must_use]
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }

    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        self.state.initial.forest()
    }

    #[must_use]
    pub(crate) const fn controller(&self) -> &CcoController {
        &self.state.controller
    }

    #[must_use]
    pub(crate) const fn transcript(&self) -> &FinalizedCcoTranscript {
        &self.state.transcript
    }

    /// Consume the sealed CCO route state in the one deterministic rewire
    /// transaction.  This rebinds the retained native surface and gathers the
    /// exterior anchor rows once; it never reconstructs mesh, topology, or a
    /// native index.
    pub(crate) fn rewire(self) -> Result<RewiredSupportRun<'a>, SupportRewireError> {
        let RoutedSupportRun { context, mut state } = self;
        let geometry = context.geometry()?;
        let loads = state.plan.finalized_loads()?;
        context.validate_rewire_preflight(&state, &geometry, loads)?;

        let delicacy = context
            .authority
            .perception_config
            .delicacy_by_face
            .ok_or(SupportRewireError::MissingAnchorDelicacy)?;
        let anchor_config = AnchorSearchConfig::current()?;
        // The pool is the sole gather pass for rewire. `optimize` receives
        // its immutable contiguous rows and performs its own batched native
        // candidate/foot checks; there is no per-member runner query.
        let pool = SurfaceAnchorPool::from_geometry(&geometry, delicacy)?;
        pool.validate_against(&geometry)?;
        let report = optimize_rewire(
            state.initial.forest_mut(),
            RewireRequest {
                frame: context.frame,
                anchor_samples: pool.rows(),
                delicacy_by_face: Some(delicacy),
                anchor_config,
                geometry: &geometry,
            },
        )?;
        state
            .initial
            .forest()
            .validate()
            .map_err(RewireError::from)?;
        let post_rewire_forest_digest = state.initial.forest().digest();

        Ok(RewiredSupportRun {
            context,
            state: RewiredPlanState {
                routed: state,
                rewire_report_digest: rewire_report_digest(&report),
                report,
                post_rewire_forest_digest,
            },
        })
    }
}

/// Route state after deterministic topology rewire. The prior plan, initial
/// construction, controller, and finalized CCO transcript remain sealed in
/// place; this state adds only the rewire evidence and exact final digest.
struct RewiredPlanState {
    routed: RoutedPlanState,
    report: RewireReport,
    rewire_report_digest: u64,
    post_rewire_forest_digest: u64,
}

/// Opaque hand-off after the rewire stage. Later stages can inspect evidence,
/// but cannot replay CCO or rewire the same routed state twice.
pub(crate) struct RewiredSupportRun<'a> {
    context: SupportRunContext<'a>,
    state: RewiredPlanState,
}

impl<'a> RewiredSupportRun<'a> {
    #[must_use]
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }

    #[must_use]
    pub(crate) const fn hollow_history(&self) -> &HollowHistory {
        self.context.hollow_history()
    }

    #[must_use]
    pub(crate) const fn request(&self) -> &SupportRequest {
        self.context.request()
    }

    #[must_use]
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }

    #[must_use]
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.state.routed.plan
    }

    #[must_use]
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.state.routed.initial
    }

    #[must_use]
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }

    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        self.state.routed.initial.forest()
    }

    #[must_use]
    pub(crate) const fn controller(&self) -> &CcoController {
        &self.state.routed.controller
    }

    #[must_use]
    pub(crate) const fn transcript(&self) -> &FinalizedCcoTranscript {
        &self.state.routed.transcript
    }

    #[must_use]
    pub(crate) const fn rewire_report(&self) -> &RewireReport {
        &self.state.report
    }

    #[must_use]
    pub(crate) const fn post_rewire_forest_digest(&self) -> u64 {
        self.state.post_rewire_forest_digest
    }

    #[must_use]
    pub(crate) const fn rewire_report_digest(&self) -> u64 {
        self.state.rewire_report_digest
    }

    /// Consume the sealed rewire state in the frozen bracing fixed point.
    /// The retained surface, plate frame, and caller-owned batch resource are
    /// used directly; no geometry/index or bracing policy is reconstructed.
    pub(crate) fn brace(self) -> Result<BracedSupportRun<'a>, SupportBraceError> {
        let RewiredSupportRun { context, state } = self;
        let geometry = context.geometry()?;
        let loads = state.routed.plan.finalized_loads()?;
        context.validate_brace_preflight(&state, &geometry, loads)?;

        let plan = brace_fixed_point_current_parity(
            state.routed.initial.forest(),
            geometry.surface(),
            context.frame.plate_z_mm(),
            context.brace_penetration_limits(),
        )?;
        plan.forest.validate().map_err(BraceError::from)?;
        let report = transient_report(&plan.forest, context.frame.plate_z_mm())?;
        let post_brace_forest_digest = plan.forest.digest();
        let post_brace_report_digest = transient_report_digest(&report);

        Ok(BracedSupportRun {
            context,
            state: BracedPlanState {
                rewired: state,
                plan,
                report,
                post_brace_forest_digest,
                post_brace_report_digest,
            },
        })
    }
}

/// Route and rewire evidence plus the complete frozen brace fixed-point plan.
/// This opaque hand-off deliberately preserves both earlier stages while
/// making only the brace plan's final forest authoritative for later stages.
struct BracedPlanState {
    rewired: RewiredPlanState,
    plan: BraceFixedPointPlan,
    report: TransientReport,
    post_brace_forest_digest: u64,
    post_brace_report_digest: u64,
}

pub(crate) struct BracedSupportRun<'a> {
    context: SupportRunContext<'a>,
    state: BracedPlanState,
}

impl BracedSupportRun<'_> {
    #[must_use]
    pub(crate) const fn mesh(&self) -> &Mesh {
        self.context.mesh()
    }

    #[must_use]
    pub(crate) const fn frame(&self) -> BuildFrame {
        self.context.frame()
    }

    #[must_use]
    pub(crate) const fn brace_penetration_limits(&self) -> PenetrationBatchLimits {
        self.context.brace_penetration_limits()
    }

    #[must_use]
    pub(crate) const fn plan(&self) -> &SpecializedContactPlan {
        &self.state.rewired.routed.plan
    }

    #[must_use]
    pub(crate) const fn initial(&self) -> &InitialContactBuild {
        &self.state.rewired.routed.initial
    }

    #[must_use]
    pub(crate) const fn digests(&self) -> &PreparedSupportDigests {
        self.context.digests()
    }

    #[must_use]
    pub(crate) const fn controller(&self) -> &CcoController {
        &self.state.rewired.routed.controller
    }

    #[must_use]
    pub(crate) const fn transcript(&self) -> &FinalizedCcoTranscript {
        &self.state.rewired.routed.transcript
    }
    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        &self.state.plan.forest
    }

    #[must_use]
    pub(crate) const fn brace_plan(&self) -> &BraceFixedPointPlan {
        &self.state.plan
    }

    #[must_use]
    pub(crate) const fn transient_report(&self) -> &TransientReport {
        &self.state.report
    }

    #[must_use]
    pub(crate) const fn post_brace_forest_digest(&self) -> u64 {
        self.state.post_brace_forest_digest
    }

    #[must_use]
    pub(crate) const fn post_brace_report_digest(&self) -> u64 {
        self.state.post_brace_report_digest
    }

    #[must_use]
    pub(crate) const fn rewire_report(&self) -> &RewireReport {
        &self.state.rewired.report
    }

    #[must_use]
    pub(crate) const fn rewire_report_digest(&self) -> u64 {
        self.state.rewired.rewire_report_digest
    }

    #[must_use]
    pub(crate) const fn post_rewire_forest_digest(&self) -> u64 {
        self.state.rewired.post_rewire_forest_digest
    }

    /// Rebind and validate all retained provenance without another native
    /// mutation.  This is also the post-stage proof that the plan's stored
    /// report describes its exact final forest.
    pub(crate) fn validate_integrity(&self) -> Result<(), SupportBraceError> {
        let geometry = self.context.geometry()?;
        let loads = self.state.rewired.routed.plan.finalized_loads()?;
        self.context
            .validate_brace_preflight(&self.state.rewired, &geometry, loads)?;
        self.state
            .plan
            .forest
            .validate()
            .map_err(BraceError::from)?;
        if self.state.post_brace_forest_digest != self.state.plan.forest.digest() {
            return Err(SupportBraceError::FinalForestDigestMismatch);
        }
        let fresh = transient_report(&self.state.plan.forest, self.context.frame.plate_z_mm())?;
        if fresh != self.state.report
            || self.state.post_brace_report_digest != transient_report_digest(&self.state.report)
        {
            return Err(SupportBraceError::TransientReportDigestMismatch);
        }
        Ok(())
    }
}

impl SupportRunContext<'_> {
    fn validate_brace_preflight(
        &self,
        state: &RewiredPlanState,
        geometry: &AnchorGeometryAuthority<'_>,
        loads: &FinalizedContactLoads,
    ) -> Result<(), SupportBraceError> {
        state.routed.plan.validate_binding(
            &self.mesh,
            &self.authority.perception_config,
            self.frame,
            self.authority.layers,
            &self.authority.candidate_config,
            self.authority.reservations,
            self.authority.perception_limits,
            &self.generic,
            &self.regular,
            &self.authority.selection_config,
            self.authority.specialized_limits,
        )?;
        if self.generic.digest() != self.digests.generic
            || self.regular.digest() != self.digests.regular
            || state.routed.plan.digest() != self.digests.plan
            || state.routed.plan.finalized_load_plan_digest() != state.routed.plan.digest()
            || loads.digest() != self.digests.loads
            || prepared_digest(&self.digests, self.frame, self.authority) != self.digests.prepared
            || self.brace_penetration_limits != self.authority.brace_penetration_limits
        {
            return Err(SupportBraceError::PlanOrLoadDigestMismatch);
        }
        geometry.validate_mesh(&self.mesh)?;
        if geometry.digest() != self.digests.geometry
            || state.routed.initial.plan_digest() != state.routed.plan.digest()
            || state.routed.initial.load_digest() != loads.digest()
            || state.routed.initial.native_digest() != geometry.digest()
        {
            return Err(SupportBraceError::GeometryDigestMismatch);
        }
        if state.routed.initial.initial_forest_digest() != Forest::new().digest()
            || state.routed.initial.post_quarantine_forest_digest() != self.digests.initial_forest
            || !state
                .routed
                .initial
                .all_terminals_registered_before_quarantine()
        {
            return Err(SupportBraceError::InitialAuthorityMismatch);
        }
        state.routed.transcript.validate_integrity()?;
        if state.routed.transcript.plan_digest() != state.routed.plan.digest()
            || state.routed.transcript.finalized_load_digest() != loads.digest()
            || state.routed.transcript.geometry_digest() != geometry.digest()
            || state.routed.transcript.initial_forest_digest()
                != state.routed.initial.initial_forest_digest()
            || state
                .routed
                .transcript
                .initial_post_quarantine_forest_digest()
                != state.routed.initial.post_quarantine_forest_digest()
            || state.routed.transcript.final_controller_digest() != state.routed.controller.digest()
        {
            return Err(SupportBraceError::TranscriptDigestMismatch);
        }
        state
            .routed
            .initial
            .forest()
            .validate()
            .map_err(RewireError::from)?;
        if state.post_rewire_forest_digest != state.routed.initial.forest().digest()
            || rewire_report_digest(&state.report) != state.rewire_report_digest
        {
            return Err(SupportBraceError::RewireDigestMismatch);
        }
        Ok(())
    }

    fn validate_rewire_preflight(
        &self,
        state: &RoutedPlanState,
        geometry: &AnchorGeometryAuthority<'_>,
        loads: &FinalizedContactLoads,
    ) -> Result<(), SupportRewireError> {
        state.plan.validate_binding(
            &self.mesh,
            &self.authority.perception_config,
            self.frame,
            self.authority.layers,
            &self.authority.candidate_config,
            self.authority.reservations,
            self.authority.perception_limits,
            &self.generic,
            &self.regular,
            &self.authority.selection_config,
            self.authority.specialized_limits,
        )?;
        if self.generic.digest() != self.digests.generic
            || self.regular.digest() != self.digests.regular
            || state.plan.digest() != self.digests.plan
            || state.plan.finalized_load_plan_digest() != state.plan.digest()
            || loads.digest() != self.digests.loads
            || prepared_digest(&self.digests, self.frame, self.authority) != self.digests.prepared
        {
            return Err(SupportRewireError::PlanOrLoadDigestMismatch);
        }
        geometry.validate_mesh(&self.mesh)?;
        if geometry.digest() != self.digests.geometry
            || state.initial.plan_digest() != state.plan.digest()
            || state.initial.load_digest() != loads.digest()
            || state.initial.native_digest() != geometry.digest()
        {
            return Err(SupportRewireError::GeometryDigestMismatch);
        }
        if state.initial.initial_forest_digest() != Forest::new().digest()
            || state.initial.post_quarantine_forest_digest() != self.digests.initial_forest
            || !state.initial.all_terminals_registered_before_quarantine()
        {
            return Err(SupportRewireError::InitialAuthorityMismatch);
        }
        state.transcript.validate_integrity()?;
        if state.transcript.plan_digest() != state.plan.digest()
            || state.transcript.finalized_load_digest() != loads.digest()
            || state.transcript.geometry_digest() != geometry.digest()
            || state.transcript.initial_forest_digest() != state.initial.initial_forest_digest()
            || state.transcript.initial_post_quarantine_forest_digest()
                != state.initial.post_quarantine_forest_digest()
            || state.transcript.final_forest_digest() != state.initial.forest().digest()
            || state.transcript.final_controller_digest() != state.controller.digest()
        {
            return Err(SupportRewireError::TranscriptDigestMismatch);
        }
        state
            .initial
            .forest()
            .validate()
            .map_err(RewireError::from)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub(crate) enum SupportPreparationError {
    #[error(transparent)]
    PerceptionInput(#[from] SupportPerceptionError),
    #[error(transparent)]
    Perception(#[from] GenericPerceptionError),
    #[error(transparent)]
    Selection(#[from] ContactSelectionError),
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Surface(#[from] SurfaceBuildError),
    #[error(transparent)]
    Geometry(#[from] AnchorSearchError),
    #[error(transparent)]
    Initial(#[from] InitialContactError),
    #[error("support preparation perception and selection delicacy authorities disagree")]
    DelicacyAuthorityMismatch,
    #[error("support preparation selection resource authority is invalid")]
    InvalidSelectionResources,
    #[error("support preparation brace penetration resource authority is invalid")]
    InvalidBracePenetrationResources,
    #[error("support preparation layer schedule does not cover the model z range")]
    LayerScheduleCoverage,
    #[error("support preparation reservation names face {face_id} outside the model")]
    ReservationFaceOutOfRange { face_id: u32 },
    #[error("support preparation duplicates owner {owner_id} on face {face_id}")]
    DuplicateReservation { owner_id: u32, face_id: u32 },
    #[error("support preparation mesh fingerprint failed: {0}")]
    MeshFingerprint(String),
}

#[derive(Debug, Error)]
pub(crate) enum SupportRoutingError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error("sealed CCO routing requires the prepared face-order delicacy authority")]
    MissingAnchorDelicacy,
    #[error("sealed CCO routing found stale plan or finalized-load digest authority")]
    PlanOrLoadDigestMismatch,
    #[error("sealed CCO routing found stale mesh, topology, or native-surface digest authority")]
    GeometryDigestMismatch,
    #[error("sealed CCO routing found stale initial-forest authority")]
    InitialAuthorityMismatch,
    #[error("sealed CCO routing terminal {terminal} is absent or mismatched across the physical namespace")]
    TerminalNamespaceMismatch { terminal: u32 },
    #[error("sealed CCO routing physical/terminal namespaces have unequal cardinality")]
    TerminalNamespaceCardinalityMismatch,
    #[error("sealed CCO routing terminal {terminal} lacks a ledger entry")]
    MissingTerminalLedger { terminal: u32 },
    #[error("sealed CCO routing terminal {terminal} lacks its exact physical contact")]
    MissingPhysicalContact { terminal: u32 },
}

#[derive(Debug, Error)]
pub(crate) enum SupportRewireError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error(transparent)]
    Rewire(#[from] RewireError),
    #[error("sealed rewire requires the prepared face-order delicacy authority")]
    MissingAnchorDelicacy,
    #[error(
        "sealed rewire found stale generic, regular, plan, or finalized-load digest authority"
    )]
    PlanOrLoadDigestMismatch,
    #[error("sealed rewire found stale mesh, topology, or native-surface digest authority")]
    GeometryDigestMismatch,
    #[error("sealed rewire found stale initial-contact construction authority")]
    InitialAuthorityMismatch,
    #[error("sealed rewire found stale finalized CCO transcript authority")]
    TranscriptDigestMismatch,
}

#[derive(Debug, Error)]
pub(crate) enum SupportBraceError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Cco(#[from] CcoError),
    #[error(transparent)]
    Rewire(#[from] RewireError),
    #[error(transparent)]
    Brace(#[from] BraceError),
    #[error(transparent)]
    Transient(#[from] TransientError),
    #[error("sealed brace found stale generic, regular, plan, or finalized-load digest authority")]
    PlanOrLoadDigestMismatch,
    #[error("sealed brace found stale mesh, topology, or native-surface digest authority")]
    GeometryDigestMismatch,
    #[error("sealed brace found stale initial-contact construction authority")]
    InitialAuthorityMismatch,
    #[error("sealed brace found stale finalized CCO transcript authority")]
    TranscriptDigestMismatch,
    #[error("sealed brace found stale rewire forest or report digest authority")]
    RewireDigestMismatch,
    #[error("sealed brace plan's final forest digest is stale")]
    FinalForestDigestMismatch,
    #[error("sealed brace plan's transient report or report digest is stale")]
    TransientReportDigestMismatch,
}

/// Execute the sealed preparation pipeline through the one batched initial-contact build.
pub(crate) fn prepare_support_run<'a>(
    hollowed: HollowedModel,
    request: SupportRequest,
    authority: SupportPreparationAuthority<'a>,
) -> Result<PreparedSupportRun<'a>, SupportPreparationError> {
    let (mesh, hollow_history) = hollowed.into_parts();

    // Validate every independently stale-able caller input before the first native query. The
    // returned topology is the exact S0 topology authority retained for initial construction.
    let (topology, perception_delicacy) =
        validate_perception_input(&mesh, &authority.perception_config)?;
    validate_authority_pre_query(&mesh, perception_delicacy, authority)?;

    let frame = BuildFrame::new(authority.perception_config.plate_z_mm, request.lift_mm);
    let generic = prepare_generic_contacts(
        &mesh,
        &authority.perception_config,
        frame,
        authority.layers,
        &authority.candidate_config,
        authority.reservations,
        authority.perception_limits,
    )?;
    let regular = if generic.islands().is_empty() {
        SelectedContacts {
            contacts: Vec::new(),
            coverage_samples: Vec::new(),
        }
    } else {
        select_contacts_baseline(
            &mesh,
            generic.candidates(),
            generic.islands(),
            &authority.selection_config,
        )?
    };
    let plan = prepare_specialized_contacts(
        &mesh,
        &authority.perception_config,
        frame,
        authority.layers,
        &authority.candidate_config,
        authority.reservations,
        authority.perception_limits,
        &generic,
        &regular,
        &authority.selection_config,
        authority.specialized_limits,
    )?;

    let surface = NativeSurface::from_indexed(
        mesh.verts(),
        mesh.tris(),
        NativeSurfaceConfig {
            grid_pitch_mm: authority.perception_config.surface_grid_pitch_mm.get(),
            max_grid_bytes: authority.perception_config.max_grid_bytes,
        },
    )?;
    let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface)?;
    let geometry_digest = geometry.digest();
    let initial = build_initial_contacts(
        &plan,
        plan.finalized_loads()?,
        &mesh,
        &authority.perception_config,
        frame,
        authority.layers,
        &authority.candidate_config,
        authority.reservations,
        authority.perception_limits,
        &generic,
        &regular,
        &authority.selection_config,
        authority.specialized_limits,
        &geometry,
        authority.construction_limits,
    )?;

    let mesh_digest = mesh_fingerprint(&mesh)
        .map_err(|error| SupportPreparationError::MeshFingerprint(error.to_string()))?;
    let loads_digest = plan.finalized_loads()?.digest();
    let mut digests = PreparedSupportDigests {
        mesh: mesh_digest,
        generic: generic.digest(),
        regular: regular.digest(),
        plan: plan.digest(),
        loads: loads_digest,
        geometry: geometry_digest,
        initial_forest: initial.post_quarantine_forest_digest(),
        prepared: 0,
    };
    digests.prepared = prepared_digest(&digests, frame, authority);

    Ok(PreparedSupportRun {
        context: SupportRunContext {
            mesh,
            hollow_history,
            request,
            frame,
            authority,
            topology,
            surface,
            generic,
            regular,
            brace_penetration_limits: authority.brace_penetration_limits,
            digests,
        },
        plan,
        initial,
    })
}

fn validate_authority_pre_query(
    mesh: &Mesh,
    perception_delicacy: &[f64],
    authority: SupportPreparationAuthority<'_>,
) -> Result<(), SupportPreparationError> {
    let penetration = authority.brace_penetration_limits;
    if penetration.max_rows == 0
        || penetration.max_total_work == 0
        || penetration.max_total_hits == 0
        || penetration.max_total_samples == 0
        || penetration.max_total_intervals == 0
        || penetration.max_scratch_bytes == 0
    {
        return Err(SupportPreparationError::InvalidBracePenetrationResources);
    }
    if authority.selection_config.delicacy_by_face != Some(perception_delicacy) {
        return Err(SupportPreparationError::DelicacyAuthorityMismatch);
    }
    if authority
        .selection_config
        .coverage_samples_max_virtual_cells
        == 0
        || authority
            .selection_config
            .coverage_samples_max_compact_payload_bytes
            == 0
        || authority
            .selection_config
            .coverage_samples_max_working_cells
            == 0
        || authority.selection_config.coverage_samples_max_peak_bytes == 0
        || authority.selection_config.coverage_score_matrix_max_entries == 0
        || authority.selection_config.surface_max_grid_bytes == 0
    {
        return Err(SupportPreparationError::InvalidSelectionResources);
    }
    let (z_min, z_max) = mesh.bounds_z();
    let layers = authority.layers;
    if layers.count() == 0
        || layers.z0_mm() > z_min
        || layers.z_of(layers.count() - 1).unwrap_or(f64::NEG_INFINITY)
            + layers.heights_mm()[layers.count() - 1]
            < z_max
    {
        return Err(SupportPreparationError::LayerScheduleCoverage);
    }
    let mut seen = BTreeSet::new();
    for reservation in authority.reservations {
        if reservation.face_id as usize >= mesh.tris().len() {
            return Err(SupportPreparationError::ReservationFaceOutOfRange {
                face_id: reservation.face_id,
            });
        }
        if !seen.insert((reservation.face_id, reservation.owner_id)) {
            return Err(SupportPreparationError::DuplicateReservation {
                owner_id: reservation.owner_id,
                face_id: reservation.face_id,
            });
        }
    }
    Ok(())
}

fn prepared_digest(
    digests: &PreparedSupportDigests,
    frame: BuildFrame,
    authority: SupportPreparationAuthority<'_>,
) -> u64 {
    let mut hash = Fnv::new();
    hash.bytes(digests.mesh.as_bytes());
    hash.u64(digests.generic);
    hash.u64(digests.regular);
    hash.u64(digests.plan);
    hash.u64(digests.loads);
    hash.u64(digests.geometry);
    hash.u64(digests.initial_forest);
    hash.f64(frame.plate_z_mm().get());
    hash.f64(frame.lift_mm().get());
    hash.f64(authority.layers.z0_mm());
    for value in authority.layers.heights_mm() {
        hash.f64(*value);
    }
    let (depth, virtual_cells, candidates, compact_bytes, working_cells, edge_bits) =
        authority.candidate_config.authority_fields();
    hash.u32(depth);
    hash.usize(virtual_cells);
    hash.usize(candidates);
    hash.usize(compact_bytes);
    hash.usize(working_cells);
    hash.u64(edge_bits);
    let penetration = authority.brace_penetration_limits;
    hash.usize(penetration.max_rows);
    hash.usize(penetration.max_total_work);
    hash.usize(penetration.max_total_hits);
    hash.usize(penetration.max_total_samples);
    hash.usize(penetration.max_total_intervals);
    hash.usize(penetration.max_scratch_bytes);
    for reservation in authority.reservations {
        hash.u32(reservation.owner_id);
        hash.u32(reservation.face_id);
    }
    hash.finish()
}

struct Fnv(u64);

impl Fnv {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn byte(&mut self, value: u8) {
        self.0 = (self.0 ^ u64::from(value)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    fn bytes(&mut self, values: &[u8]) {
        for value in values {
            self.byte(*value);
        }
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn usize(&mut self, value: usize) {
        self.u64(value as u64);
    }
    fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }
    const fn finish(self) -> u64 {
        self.0
    }
}

fn rewire_report_digest(report: &RewireReport) -> u64 {
    let mut hash = Fnv::new();
    hash.f64(report.initial_volume_ml);
    hash.f64(report.final_volume_ml);
    hash.usize(report.curve.len());
    for (accepted, volume_ml) in &report.curve {
        hash.u64(*accepted);
        hash.f64(*volume_ml);
    }
    hash.u64(report.accepted);
    hash.u64(report.rejected);
    hash.u64(report.sweeps);
    hash.u64(report.move_evaluations);
    hash.u64(report.cco_candidate_batches);
    hash.u64(match report.termination {
        crate::rewire::RewireTermination::FullSweepNoAcceptance => 0,
        crate::rewire::RewireTermination::RollingSavingsConverged => 1,
        crate::rewire::RewireTermination::DeterministicWorkBudget => 2,
    });
    hash.finish()
}

fn transient_report_digest(report: &TransientReport) -> u64 {
    let mut hash = Fnv::new();
    hash.usize(report.members.len());
    for row in &report.members {
        hash.u32(row.member.slot());
        hash.u32(row.member.generation());
        hash.f64(row.worst_span_mm.get());
        hash.usize(row.restraint_zs_mm.len());
        for z in &row.restraint_zs_mm {
            hash.f64(z.get());
        }
        hash.usize(row.free_gaps_mm.len());
        for gap in &row.free_gaps_mm {
            hash.f64(gap.lo_mm.get());
            hash.f64(gap.hi_mm.get());
        }
        hash.usize(row.n_terminals);
        hash.f64(row.worst_span_interval_mm.lo_mm.get());
        hash.f64(row.worst_span_interval_mm.hi_mm.get());
        hash.usize(row.worst_span_path.len());
        for key in &row.worst_span_path {
            hash.u32(key.slot());
            hash.u32(key.generation());
        }
    }
    hash.finish()
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, process::Command};

    use plinth_core::{Finite, PositiveFinite};
    use plinth_decimate::{run as run_decimation, DecimationBudget, ProbeConfig};
    use plinth_hollow::{HollowedModel, SolidDecisionReason};
    use plinth_mesh::{make_box, make_overhang_table};
    use plinth_orient::{BaselineSelection, OrientationHistory, OrientationReport, RigidTransform};

    use super::*;

    const GRID_BYTES: usize = 64 * 1024 * 1024;
    const SWEEP_CHILD: &str = "PLINTH_RUNNER_PREPARE_SWEEP_CHILD";
    const SWEEP_LINE: &str = "RUNNER_PREPARE_SWEEP=";

    fn required_runner_corpus_path() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/baseline/make_overhang_table.prepped.stl")
    }

    fn load_required_runner_corpus() -> Mesh {
        let path = required_runner_corpus_path();
        assert!(
            path.is_file(),
            "RequiredRunnerCorpusMissing: {}",
            path.display()
        );
        plinth_mesh::load_binary_stl(&path, plinth_mesh::WeldPolicy::None).unwrap_or_else(|error| {
            panic!(
                "RequiredRunnerCorpusUnreadable: {}: {error}",
                path.display()
            )
        })
    }

    fn procedural_overhang_for_error_case() -> Mesh {
        make_overhang_table().expect("procedural overhang error-case fixture")
    }

    fn hollowed_fixture(mesh: Mesh) -> HollowedModel {
        let budget =
            DecimationBudget::ratified_default(ProbeConfig::new(1.0, usize::MAX, 10_000.0))
                .expect("registered decimation budget");
        let decimated = run_decimation(&mesh, budget)
            .expect("required fixture decimation")
            .into_decimated_model()
            .expect("required fixture scale");
        let (mesh, decimation) = decimated.into_parts();
        let orientation = OrientationHistory::from_reports(
            decimation,
            OrientationReport {
                pinned_report: PathBuf::from("runner-required-fixture.pipeline.json"),
                selection: BaselineSelection::Identity,
                transform: RigidTransform::identity(),
            },
        );
        HollowedModel::solid_decision(mesh, orientation, SolidDecisionReason::BelowThreshold)
            .expect("required fixture enters hollow hand-off")
    }

    fn layers() -> LayerPlan {
        LayerPlan::new(0.0, vec![0.05; 1_000]).expect("covering fixture layers")
    }

    fn candidate_config() -> CandidatePayloadConfig {
        CandidatePayloadConfig::current(8, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536)
            .expect("registered candidate config")
    }

    // This is caller-owned test runtime capacity for the named corpus.  It
    // is deliberately finite and not a production/default policy.
    fn brace_penetration_limits() -> PenetrationBatchLimits {
        PenetrationBatchLimits {
            max_rows: 100_000,
            max_total_work: 1_000_000_000,
            max_total_hits: 1_000_000_000,
            max_total_samples: 10_000_000_000,
            max_total_intervals: 1_000_000_000,
            max_scratch_bytes: 100_000_000_000,
        }
    }

    fn authority<'a>(
        delicacy: &'a [f64],
        layers: &'a LayerPlan,
    ) -> SupportPreparationAuthority<'a> {
        let perception = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            GRID_BYTES,
            Some(delicacy),
        )
        .unwrap();
        let selection =
            ContactSelectionConfig::current_defaults(Some(delicacy), GRID_BYTES).unwrap();
        SupportPreparationAuthority::new(
            layers,
            perception,
            candidate_config(),
            &[],
            GenericPerceptionLimits::production(),
            selection,
            SpecializedLimits::production(),
            ContactConstructionLimits::production(),
            brace_penetration_limits(),
        )
    }

    fn with_prepared_table<T>(use_prepared: impl FnOnce(PreparedSupportRun<'_>) -> T) -> T {
        let hollowed = hollowed_fixture(load_required_runner_corpus());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, use_prepared)
    }

    fn prepare_fixture<T>(
        hollowed: HollowedModel,
        delicacy: &[f64],
        layers: &LayerPlan,
        use_prepared: impl FnOnce(PreparedSupportRun<'_>) -> T,
    ) -> T {
        let prepared = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(delicacy, layers),
        )
        .expect("required fixture preparation");
        use_prepared(prepared)
    }

    fn rejected_rewire_preflight(routed: &RoutedSupportRun<'_>) -> SupportRewireError {
        let forest_digest = routed.forest().digest();
        let geometry = routed
            .context
            .geometry()
            .expect("retained geometry rebinds");
        let loads = routed
            .state
            .plan
            .finalized_loads()
            .expect("retained plan owns finalized loads");
        let error = routed
            .context
            .validate_rewire_preflight(&routed.state, &geometry, loads)
            .expect_err("corrupt route evidence must fail before rewire mutation");
        assert_eq!(routed.forest().digest(), forest_digest);
        error
    }

    fn rejected_brace_preflight(rewired: &RewiredSupportRun<'_>) -> SupportBraceError {
        let forest_digest = rewired.forest().digest();
        let geometry = rewired
            .context
            .geometry()
            .expect("retained geometry rebinds");
        let loads = rewired
            .state
            .routed
            .plan
            .finalized_loads()
            .expect("retained plan owns finalized loads");
        let error = rewired
            .context
            .validate_brace_preflight(&rewired.state, &geometry, loads)
            .expect_err("corrupt rewire evidence must fail before brace mutation");
        assert_eq!(rewired.forest().digest(), forest_digest);
        error
    }

    fn translated_mesh(mesh: Mesh, offset: [f64; 3]) -> Mesh {
        Mesh::new(
            mesh.verts()
                .iter()
                .map(|point| std::array::from_fn(|axis| point[axis] + offset[axis]))
                .collect(),
            mesh.tris().to_vec(),
        )
        .expect("translated test mesh remains valid")
    }

    fn merge_meshes(parts: &[Mesh]) -> Mesh {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for part in parts {
            let offset = u32::try_from(vertices.len()).expect("test mesh vertex namespace");
            vertices.extend_from_slice(part.verts());
            triangles.extend(
                part.tris()
                    .iter()
                    .map(|triangle| triangle.map(|vertex| vertex + offset)),
            );
        }
        Mesh::new(vertices, triangles).expect("merged test mesh remains valid")
    }

    fn prepared_digest_for_mesh(mesh: Mesh) -> u64 {
        let hollowed = hollowed_fixture(mesh);
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(&delicacy, &layers),
        )
        .expect("mesh preparation")
        .digests()
        .prepared()
    }

    #[test]
    fn required_named_corpus_fixture_is_loaded_from_disk() {
        let mesh = load_required_runner_corpus();
        assert!(!mesh.verts().is_empty());
        assert!(!mesh.tris().is_empty());
    }

    #[test]
    fn changed_corpus_geometry_changes_prepared_digest() {
        let corpus = load_required_runner_corpus();
        let original_digest = prepared_digest_for_mesh(corpus.clone());
        let translated_vertices = corpus
            .verts()
            .iter()
            .map(|vertex| [vertex[0] + 0.125, vertex[1], vertex[2]])
            .collect();
        let changed = Mesh::new(translated_vertices, corpus.tris().to_vec())
            .expect("translated corpus remains a valid mesh");
        let changed_digest = prepared_digest_for_mesh(changed);
        assert_ne!(original_digest, changed_digest);
    }

    #[test]
    fn prepares_required_fixture_and_retains_plan_owned_authority() {
        with_prepared_table(|prepared| {
            assert!(!prepared.plan().contacts().is_empty());
            assert_eq!(
                prepared.loads().unwrap().digest(),
                prepared.digests().loads()
            );
            assert_eq!(prepared.plan().digest(), prepared.digests().plan());
            assert_eq!(prepared.initial().plan_digest(), prepared.plan().digest());
            assert_eq!(
                prepared.initial().load_digest(),
                prepared.loads().unwrap().digest()
            );
            assert_eq!(
                prepared.geometry().unwrap().digest(),
                prepared.digests().geometry()
            );
            assert_eq!(
                prepared.initial().post_quarantine_forest_digest(),
                prepared.digests().initial_forest()
            );
            assert_eq!(prepared.frame().lift_mm(), prepared.request().lift_mm);
            assert_eq!(
                prepared.mesh().tris().len(),
                prepared
                    .hollow_history()
                    .orientation()
                    .decimation()
                    .report()
                    .output_faces()
            );
        });
    }

    #[test]
    fn routes_required_corpus_in_frozen_specialized_then_regular_cadence() {
        with_prepared_table(|prepared| {
            let expected = prepared
                .initial()
                .routing_order()
                .specialized_terminals()
                .iter()
                .chain(prepared.initial().routing_order().regular_terminals())
                .copied()
                .collect::<Vec<_>>();
            let plan_digest = prepared.digests().plan();
            let loads_digest = prepared.digests().loads();
            let geometry_digest = prepared.digests().geometry();
            let routed = prepared
                .route_initial_cco()
                .expect("required corpus routes through sealed CCO transaction");
            let transcript = routed.transcript();
            transcript
                .validate_integrity()
                .expect("runner retains an internally sealed transcript");
            assert_eq!(transcript.plan_digest(), plan_digest);
            assert_eq!(transcript.finalized_load_digest(), loads_digest);
            assert_eq!(transcript.geometry_digest(), geometry_digest);
            assert_eq!(transcript.rows().len(), expected.len());
            assert_eq!(
                transcript
                    .rows()
                    .iter()
                    .map(|row| row.terminal_id())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(transcript.final_forest_digest(), routed.forest().digest());
            assert_eq!(
                transcript.final_controller_digest(),
                routed.controller().digest()
            );
        });
    }

    #[test]
    fn route_consumes_prepared_state_once_and_retains_split_authority_digests() {
        with_prepared_table(|prepared| {
            let expected_prepared = prepared.digests().prepared();
            let expected_plan = prepared.plan().digest();
            let expected_loads = prepared.loads().unwrap().digest();
            let expected_initial_forest = prepared.initial().forest().digest();

            // `route_initial_cco` takes `self`; retaining this one resulting handle is the only
            // available route transition and proves the plan/initial pair moved intact.
            let routed = prepared
                .route_initial_cco()
                .expect("prepared state is consumed by exactly one CCO route");

            assert_eq!(routed.digests().prepared(), expected_prepared);
            assert_eq!(routed.plan().digest(), expected_plan);
            assert_eq!(routed.initial().forest().digest(), routed.forest().digest());
            assert_eq!(
                routed.transcript().plan_digest(),
                routed.plan().digest(),
                "specialized plan moved into routed plan state"
            );
            assert_eq!(routed.transcript().finalized_load_digest(), expected_loads);
            assert_eq!(
                routed.transcript().final_forest_digest(),
                routed.forest().digest()
            );
            assert_eq!(
                routed.transcript().final_controller_digest(),
                routed.controller().digest()
            );
            assert_ne!(expected_initial_forest, routed.forest().digest());
        });
    }

    #[test]
    fn rewires_required_corpus_with_sealed_route_evidence_and_named_termination() {
        with_prepared_table(|prepared| {
            let routed = prepared
                .route_initial_cco()
                .expect("required corpus routes before rewire");
            let plan_digest = routed.plan().digest();
            let loads_digest = routed.transcript().finalized_load_digest();
            let geometry_digest = routed.transcript().geometry_digest();
            let transcript_digest = routed.transcript().digest();
            let rewired = routed
                .rewire()
                .expect("required corpus rewires through retained authority");

            rewired
                .forest()
                .validate()
                .expect("rewire leaves a valid final forest");
            assert_eq!(rewired.plan().digest(), plan_digest);
            assert_eq!(rewired.transcript().digest(), transcript_digest);
            assert_eq!(rewired.transcript().plan_digest(), plan_digest);
            assert_eq!(rewired.transcript().finalized_load_digest(), loads_digest);
            assert_eq!(rewired.transcript().geometry_digest(), geometry_digest);
            assert_eq!(
                rewired.post_rewire_forest_digest(),
                rewired.forest().digest(),
                "rewired state retains the exact post-rewire forest digest"
            );
            assert!(matches!(
                rewired.rewire_report().termination,
                crate::rewire::RewireTermination::FullSweepNoAcceptance
                    | crate::rewire::RewireTermination::RollingSavingsConverged
                    | crate::rewire::RewireTermination::DeterministicWorkBudget
            ));
        });
    }

    #[test]
    fn braces_required_corpus_with_retained_rewire_evidence_and_named_termination() {
        with_prepared_table(|prepared| {
            let expected_limits = prepared.context.brace_penetration_limits();
            let rewired = prepared
                .route_initial_cco()
                .expect("required corpus routes before rewire")
                .rewire()
                .expect("required corpus rewires before brace");
            let rewire_forest_digest = rewired.post_rewire_forest_digest();
            let expected_rewire_report_digest = rewired.rewire_report_digest();
            let transcript_digest = rewired.transcript().digest();
            let braced = rewired
                .brace()
                .expect("required corpus braces through retained authority");

            braced
                .validate_integrity()
                .expect("braced final forest and transient report remain sealed");
            assert_eq!(braced.transcript().digest(), transcript_digest);
            assert_eq!(braced.brace_penetration_limits(), expected_limits);
            assert_eq!(braced.post_rewire_forest_digest(), rewire_forest_digest);
            assert_eq!(
                rewire_report_digest(braced.rewire_report()),
                expected_rewire_report_digest
            );
            assert_eq!(braced.post_brace_forest_digest(), braced.forest().digest());
            assert_eq!(
                braced.post_brace_report_digest(),
                transient_report_digest(braced.transient_report())
            );
            assert!(matches!(
                braced.brace_plan().termination,
                crate::brace::BraceFixedPointTermination::Converged
                    | crate::brace::BraceFixedPointTermination::IterationLimit
                    | crate::brace::BraceFixedPointTermination::Unplaceable
            ));
        });
    }

    #[test]
    fn stale_rewire_report_digest_fails_closed_before_brace_mutation() {
        with_prepared_table(|prepared| {
            let mut rewired = prepared
                .route_initial_cco()
                .expect("required corpus routes")
                .rewire()
                .expect("required corpus rewires");
            rewired.state.rewire_report_digest = rewired.state.rewire_report_digest.wrapping_add(1);
            assert!(matches!(
                rejected_brace_preflight(&rewired),
                SupportBraceError::RewireDigestMismatch
            ));
            assert!(matches!(
                rewired.brace(),
                Err(SupportBraceError::RewireDigestMismatch)
            ));
        });
    }

    #[test]
    fn stale_brace_transient_report_digest_fails_closed_on_revalidation() {
        with_prepared_table(|prepared| {
            let mut braced = prepared
                .route_initial_cco()
                .expect("required corpus routes")
                .rewire()
                .expect("required corpus rewires")
                .brace()
                .expect("required corpus braces");
            braced.state.post_brace_report_digest =
                braced.state.post_brace_report_digest.wrapping_add(1);
            assert!(matches!(
                braced.validate_integrity(),
                Err(SupportBraceError::TransientReportDigestMismatch)
            ));
        });
    }

    #[test]
    fn brace_penetration_cap_minus_one_fails_closed_at_runner_boundary() {
        let hollowed = hollowed_fixture(load_required_runner_corpus());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let mut limited_authority = authority(&delicacy, &layers);
        // The required corpus reaches the brace interval batch with more
        // than this one admitted row.  All other resources remain identical
        // to the successful finite fixture resource.
        limited_authority.brace_penetration_limits.max_rows = 1;
        let prepared = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            limited_authority,
        )
        .expect("one-row brace cap remains a valid caller authority");
        let result = prepared
            .route_initial_cco()
            .expect("required corpus routes")
            .rewire()
            .expect("required corpus rewires")
            .brace();
        assert!(matches!(
            result,
            Err(SupportBraceError::Brace(BraceError::Penetration(
                geom_kernel::PenetrationBatchError::ResourceLimitExceeded {
                    resource: "rows",
                    required: _,
                    limit: 1,
                }
            )))
        ));
    }

    #[test]
    fn stale_routed_digest_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco()
                .expect("required corpus routes before stale authority check");
            routed.context.digests.plan = routed.context.digests.plan.wrapping_add(1);
            assert!(matches!(
                routed.rewire(),
                Err(SupportRewireError::PlanOrLoadDigestMismatch)
            ));
        });
    }

    #[test]
    fn stale_initial_forest_digest_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco()
                .expect("required corpus routes");
            routed.state.initial.test_corrupt_initial_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::InitialAuthorityMismatch
            ));
            assert!(matches!(
                routed.rewire(),
                Err(SupportRewireError::InitialAuthorityMismatch)
            ));
        });
    }

    #[test]
    fn stale_post_quarantine_digest_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco()
                .expect("required corpus routes");
            routed
                .state
                .initial
                .test_corrupt_post_quarantine_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::InitialAuthorityMismatch
            ));
            assert!(matches!(
                routed.rewire(),
                Err(SupportRewireError::InitialAuthorityMismatch)
            ));
        });
    }

    #[test]
    fn missing_initial_registration_evidence_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco()
                .expect("required corpus routes");
            routed
                .state
                .initial
                .test_clear_terminal_registration_evidence();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::InitialAuthorityMismatch
            ));
            assert!(matches!(
                routed.rewire(),
                Err(SupportRewireError::InitialAuthorityMismatch)
            ));
        });
    }

    #[test]
    fn transcript_initial_forest_binding_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco()
                .expect("required corpus routes");
            routed.state.transcript.test_rebind_initial_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::TranscriptDigestMismatch
            ));
            assert!(matches!(
                routed.rewire(),
                Err(SupportRewireError::TranscriptDigestMismatch)
            ));
        });
    }

    #[test]
    fn transcript_post_quarantine_binding_fails_closed_before_rewire_mutation() {
        with_prepared_table(|prepared| {
            let mut routed = prepared
                .route_initial_cco()
                .expect("required corpus routes");
            routed
                .state
                .transcript
                .test_rebind_initial_post_quarantine_forest_digest();
            assert!(matches!(
                rejected_rewire_preflight(&routed),
                SupportRewireError::TranscriptDigestMismatch
            ));
            assert!(matches!(
                routed.rewire(),
                Err(SupportRewireError::TranscriptDigestMismatch)
            ));
        });
    }

    #[test]
    fn stale_initial_authority_is_refused_before_cco_routing() {
        with_prepared_table(|mut prepared| {
            prepared.initial.test_corrupt_initial_forest_digest();
            assert!(matches!(
                prepared.route_initial_cco(),
                Err(SupportRoutingError::InitialAuthorityMismatch)
            ));
        });
    }

    #[test]
    fn routes_real_mixed_quarantine_and_retained_terminals() {
        let buried = make_overhang_table().expect("buried table fixture");
        let blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.56]).expect("burial blocker fixture");
        let live = translated_mesh(
            make_overhang_table().expect("live table fixture"),
            [100.0, 0.0, 0.0],
        );
        let hollowed = hollowed_fixture(merge_meshes(&[buried, blocker, live]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let routed = prepared
                .route_initial_cco()
                .expect("mixed fixture seals through CCO");
            assert!(routed
                .transcript()
                .rows()
                .iter()
                .any(|row| row.kind() == crate::cco::FinalizedDispositionKind::Routed));
            assert!(routed.transcript().rows().iter().any(|row| {
                row.kind() == crate::cco::FinalizedDispositionKind::SkippedQuarantined
            }));
        });
    }

    #[test]
    fn routes_real_no_winner_terminal_with_sealed_evidence() {
        let table = load_required_runner_corpus();
        let boundary_blocker =
            make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.55]).expect("no-winner blocker fixture");
        let hollowed = hollowed_fixture(merge_meshes(&[table, boundary_blocker]));
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        prepare_fixture(hollowed, &delicacy, &layers, |prepared| {
            let routed = prepared
                .route_initial_cco()
                .expect("no-winner fixture seals through CCO");
            let row = routed
                .transcript()
                .rows()
                .iter()
                .find(|row| row.kind() == crate::cco::FinalizedDispositionKind::UnroutedNoWinner)
                .expect("fixture retains a real no-winner terminal");
            assert!(row.route().is_some());
            let reason = row
                .no_winner_reason()
                .expect("no-winner transcript row retains a reason");
            assert!(!reason.anchor_seat_refusals().is_empty());
        });
    }

    #[test]
    fn initial_routing_is_demand_bearing_before_regular_with_exact_ids() {
        with_prepared_table(|prepared| {
            let routing = prepared.initial().routing_order();
            let mut demanded = prepared
                .plan()
                .contacts()
                .iter()
                .filter(|contact| !contact.demands().is_empty())
                .map(|contact| u32::try_from(contact.contact_id()).unwrap())
                .collect::<Vec<_>>();
            let mut regular = prepared
                .plan()
                .contacts()
                .iter()
                .filter(|contact| contact.demands().is_empty())
                .map(|contact| u32::try_from(contact.contact_id()).unwrap())
                .collect::<Vec<_>>();
            demanded.sort_unstable();
            regular.sort_unstable();
            assert!(routing
                .specialized_terminals()
                .iter()
                .all(|terminal| { demanded.binary_search(terminal).is_ok() }));
            assert!(routing
                .regular_terminals()
                .iter()
                .all(|terminal| { regular.binary_search(terminal).is_ok() }));
            let mut routed = routing
                .specialized_terminals()
                .iter()
                .chain(routing.regular_terminals())
                .copied()
                .collect::<Vec<_>>();
            routed.sort_unstable();
            let mut physical_ids = prepared
                .plan()
                .contacts()
                .iter()
                .map(|contact| u32::try_from(contact.contact_id()).unwrap())
                .collect::<Vec<_>>();
            physical_ids.sort_unstable();
            let mut terminal_ids = prepared
                .initial()
                .terminals()
                .iter()
                .map(|row| row.terminal)
                .collect::<Vec<_>>();
            terminal_ids.sort_unstable();
            assert_eq!(routed, physical_ids);
            assert_eq!(terminal_ids, physical_ids);
        });
    }

    #[test]
    fn canonical_empty_regular_selection_reaches_specialized_planning() {
        let hollowed = hollowed_fixture(make_box([10.0; 3], [0.0, 0.0, 0.0]).unwrap());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let prepared = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(&delicacy, &layers),
        )
        .expect("empty regular authority is canonical");
        assert!(prepared.generic().islands().is_empty());
        assert!(prepared.regular().contacts().is_empty());
        assert!(prepared.regular().coverage_samples().is_empty());
        assert_eq!(
            prepared.initial().queried_stubs(),
            prepared.plan().contacts().len()
        );
    }

    #[test]
    fn mismatched_authority_is_rejected_before_native_preparation() {
        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let perception_delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let mut selection_delicacy = perception_delicacy.clone();
        selection_delicacy[0] = 0.25;
        let layers = layers();
        let perception = SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            GRID_BYTES,
            Some(&perception_delicacy),
        )
        .unwrap();
        let selection =
            ContactSelectionConfig::current_defaults(Some(&selection_delicacy), GRID_BYTES)
                .unwrap();
        let result = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            SupportPreparationAuthority::new(
                &layers,
                perception,
                candidate_config(),
                &[],
                GenericPerceptionLimits::production(),
                selection,
                SpecializedLimits::production(),
                ContactConstructionLimits::production(),
                brace_penetration_limits(),
            ),
        );
        assert!(matches!(
            result,
            Err(SupportPreparationError::DelicacyAuthorityMismatch)
        ));
    }

    #[test]
    fn uncovered_layers_and_stale_reservations_fail_typed() {
        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let short_layers = LayerPlan::new(0.0, vec![0.05; 10]).unwrap();
        let result = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority(&delicacy, &short_layers),
        );
        assert!(matches!(
            result,
            Err(SupportPreparationError::LayerScheduleCoverage)
        ));

        let hollowed = hollowed_fixture(procedural_overhang_for_error_case());
        let delicacy = vec![0.0; hollowed.mesh().tris().len()];
        let layers = layers();
        let mut authority = authority(&delicacy, &layers);
        let stale = [SpecializedOwnerReservation {
            owner_id: 7,
            face_id: u32::MAX,
        }];
        authority.reservations = &stale;
        let result = prepare_support_run(
            hollowed,
            SupportRequest {
                lift_mm: PositiveFinite::new(5.0).unwrap(),
            },
            authority,
        );
        assert!(matches!(
            result,
            Err(SupportPreparationError::ReservationFaceOutOfRange { .. })
        ));
    }

    #[test]
    fn runner_thread_sweep_child() {
        if std::env::var_os(SWEEP_CHILD).is_none() {
            return;
        }
        with_prepared_table(|prepared| {
            let requested = std::env::var("PLINTH_THREADS")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let thread_evidence = prepared.plan().generic_slice_thread_evidence();
            assert_eq!(thread_evidence.0, requested);
            assert!((1..=requested).contains(&thread_evidence.1));
            let routed = prepared
                .route_initial_cco()
                .expect("thread sweep routes the required corpus");
            let rewired = routed
                .rewire()
                .expect("thread sweep rewires the required corpus");
            let braced = rewired
                .brace()
                .expect("thread sweep braces the required corpus");
            braced
                .validate_integrity()
                .expect("thread sweep retains final brace evidence");
            println!(
                "{SWEEP_LINE}{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                braced.digests().prepared(),
                braced.digests().plan(),
                braced.digests().loads(),
                braced.digests().initial_forest(),
                braced.initial().queried_stubs(),
                braced.initial().condemned().len(),
                braced.transcript().final_forest_digest(),
                braced.transcript().final_controller_digest(),
                braced.transcript().digest(),
                rewire_report_digest(braced.rewire_report()),
                braced.post_rewire_forest_digest(),
                braced.post_brace_forest_digest(),
                braced.post_brace_report_digest(),
                braced.brace_plan().iterations,
                braced.brace_plan().additions.len(),
                match braced.brace_plan().termination {
                    crate::brace::BraceFixedPointTermination::Converged => 0,
                    crate::brace::BraceFixedPointTermination::IterationLimit => 1,
                    crate::brace::BraceFixedPointTermination::Unplaceable => 2,
                },
            );
        });
    }

    #[test]
    fn actual_one_four_sixteen_thread_brace_state_and_report_are_digest_identical() {
        if std::env::var_os(SWEEP_CHILD).is_some() {
            return;
        }
        let executable = std::env::current_exe().expect("current plinth-support test binary");
        let signatures = [1, 4, 16]
            .into_iter()
            .map(|threads| {
                let output = Command::new(&executable)
                    .arg("--exact")
                    .arg("runner::tests::runner_thread_sweep_child")
                    .arg("--nocapture")
                    .env(SWEEP_CHILD, "1")
                    .env("PLINTH_THREADS", threads.to_string())
                    .output()
                    .expect("runner thread-sweep child starts");
                assert!(
                    output.status.success(),
                    "runner {threads}-thread child failed:\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .expect("test output is UTF-8")
                    .split_once(SWEEP_LINE)
                    .and_then(|(_, signature)| signature.lines().next())
                    .map(str::to_owned)
                    .expect("runner child emits digest evidence")
            })
            .collect::<Vec<_>>();
        assert_eq!(signatures[0], signatures[1]);
        assert_eq!(signatures[0], signatures[2]);
    }
}
