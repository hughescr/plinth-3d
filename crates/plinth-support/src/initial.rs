//! Initial contact stubs are constructed once, then either remain terminal
//! seeds or are quarantined.  This leaf deliberately does not route or grow.

use std::cmp::Ordering;

use geom_kernel::{Interval, PenetrationBatchLimits};
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_mesh::Mesh;
use thiserror::Error;

use crate::{
    anchor::{AnchorGeometryAuthority, AnchorSearchError},
    contact::ContactPolicy,
    designed_embed::{
        attest_members_batch, ContactAreaDeficit, ContactAttestationRefusal,
        ContactAttestationRequest, DesignedMaskError, EndpointMaskValidationLimits,
        PositiveFiniteMm,
    },
    dyadic::CandidatePayloadConfig,
    forest::{
        modeled_initial_contact_bytes, ContactEnd, DigestWriter, Forest, ForestError, Member,
        MemberKey, MemberKind, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius,
        TerminalLedgerState,
    },
    perception::{
        FinalizedContactLoads, GenericContactPlan, GenericPerceptionError, GenericPerceptionLimits,
        SpecializedOwnerReservation,
    },
    specialized::{SpecializedContactPlan, SpecializedError, SpecializedLimits},
    BuildFrame, ContactSelectionConfig, SelectedContacts, SupportPerceptionConfig,
};

/// Caller-owned refusal bounds for the contact-sized staging work.  They are
/// intentionally result-inert and are checked before the forest is changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactConstructionLimits {
    max_contacts: usize,
    max_load_rows: usize,
    max_work_items: usize,
    max_scratch_bytes: usize,
}

impl ContactConstructionLimits {
    #[must_use]
    pub const fn new(
        max_contacts: usize,
        max_load_rows: usize,
        max_work_items: usize,
        max_scratch_bytes: usize,
    ) -> Option<Self> {
        if max_contacts == 0 || max_load_rows == 0 || max_work_items == 0 || max_scratch_bytes == 0
        {
            return None;
        }
        Some(Self {
            max_contacts,
            max_load_rows,
            max_work_items,
            max_scratch_bytes,
        })
    }
    #[must_use]
    pub(crate) const fn production() -> Self {
        Self {
            max_contacts: 1_000_000,
            max_load_rows: 10_000_000,
            max_work_items: 4_000_000,
            max_scratch_bytes: 512 * 1024 * 1024,
        }
    }
}

/// The frozen, ordered terminal namespaces that a later route stage must use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InitialRoutingOrder {
    specialized_terminals: Vec<u32>,
    regular_terminals: Vec<u32>,
}

impl InitialRoutingOrder {
    #[must_use]
    pub(crate) fn specialized_terminals(&self) -> &[u32] {
        &self.specialized_terminals
    }

    #[must_use]
    pub(crate) fn regular_terminals(&self) -> &[u32] {
        &self.regular_terminals
    }
}

/// Evidence for a retained or condemned contact terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InitialTerminalEvidence {
    pub(crate) terminal: u32,
    pub(crate) key: MemberKey,
    pub(crate) state: TerminalLedgerState,
}

/// Exact post-mask overlap which condemned an initial stub.  It intentionally
/// carries no clearance certificate: this stage does not mint certificates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CondemnedInitialStub {
    pub(crate) terminal: u32,
    pub(crate) key: MemberKey,
    pub(crate) remaining_inside_axis_mm: f64,
}

/// PR-8 evidence for one stub whose model contact could NOT be affirmatively
/// attested.
///
/// This is a candidate refusal routed to the same quarantine the overlap cutoff
/// uses, exactly as OD-4 requires — never a hard pipeline failure. The stub, its
/// terminal ledger record and its carried load all survive quarantine, so a
/// refusal is visible and accountable rather than a support that silently
/// vanished.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct UnattestedInitialStub {
    pub(crate) terminal: u32,
    pub(crate) key: MemberKey,
    pub(crate) refusal: ContactAttestationRefusal,
}

/// PR-4 evidence for one stub that IS attached but over less area than its tip
/// radius claims. Recorded, never quarantined — see
/// [`crate::designed_embed::ContactAreaDeficit`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeficientInitialContact {
    pub(crate) terminal: u32,
    pub(crate) key: MemberKey,
    pub(crate) deficit: ContactAreaDeficit,
}

/// Opaque binding for the completed all-stub construction/quarantine leaf.
#[derive(Clone, Debug)]
pub(crate) struct InitialContactBuild {
    forest: Forest,
    plan_digest: u64,
    load_digest: u64,
    policy_digest: u64,
    native_digest: u64,
    initial_forest_digest: u64,
    post_quarantine_forest_digest: u64,
    routing_order: InitialRoutingOrder,
    terminals: Vec<InitialTerminalEvidence>,
    condemned: Vec<CondemnedInitialStub>,
    unattested: Vec<UnattestedInitialStub>,
    area_deficient: Vec<DeficientInitialContact>,
    contact_attestation_digest: u64,
    attested_contacts: usize,
    interval_batch_count: usize,
    queried_stubs: usize,
    scratch_bytes: usize,
    all_terminals_registered_before_quarantine: bool,
}

impl InitialContactBuild {
    #[must_use]
    pub(crate) const fn forest(&self) -> &Forest {
        &self.forest
    }
    pub(crate) const fn forest_mut(&mut self) -> &mut Forest {
        &mut self.forest
    }
    #[must_use]
    pub(crate) const fn plan_digest(&self) -> u64 {
        self.plan_digest
    }
    #[must_use]
    pub(crate) const fn load_digest(&self) -> u64 {
        self.load_digest
    }
    #[must_use]
    pub(crate) const fn policy_digest(&self) -> u64 {
        self.policy_digest
    }
    #[must_use]
    pub(crate) const fn native_digest(&self) -> u64 {
        self.native_digest
    }
    #[must_use]
    pub(crate) const fn initial_forest_digest(&self) -> u64 {
        self.initial_forest_digest
    }
    #[must_use]
    pub(crate) const fn post_quarantine_forest_digest(&self) -> u64 {
        self.post_quarantine_forest_digest
    }
    #[must_use]
    pub(crate) const fn routing_order(&self) -> &InitialRoutingOrder {
        &self.routing_order
    }
    #[must_use]
    pub(crate) fn terminals(&self) -> &[InitialTerminalEvidence] {
        &self.terminals
    }
    #[must_use]
    pub(crate) fn condemned(&self) -> &[CondemnedInitialStub] {
        &self.condemned
    }
    /// PR-8 — every stub whose model contact was refused, with its reason.
    #[must_use]
    pub(crate) fn unattested(&self) -> &[UnattestedInitialStub] {
        &self.unattested
    }
    /// PR-4 — every stub that is attached but over an interface too small for
    /// its routed load. Live, not quarantined; see [`ContactAreaDeficit`].
    #[must_use]
    pub(crate) fn area_deficient(&self) -> &[DeficientInitialContact] {
        &self.area_deficient
    }
    /// PR-8 — the attestation transcript digest.
    ///
    /// It covers EVERY contact's verdict, attested and refused alike, so a
    /// contact that silently stopped being attested moves this number even
    /// though the forest digest cannot see it (an attested contact and an
    /// unattested-but-still-live one are the same geometry).
    #[must_use]
    pub(crate) const fn contact_attestation_digest(&self) -> u64 {
        self.contact_attestation_digest
    }
    #[must_use]
    pub(crate) const fn attested_contacts(&self) -> usize {
        self.attested_contacts
    }
    #[must_use]
    pub(crate) const fn interval_batch_count(&self) -> usize {
        self.interval_batch_count
    }
    #[must_use]
    pub(crate) const fn queried_stubs(&self) -> usize {
        self.queried_stubs
    }
    #[must_use]
    pub(crate) const fn scratch_bytes(&self) -> usize {
        self.scratch_bytes
    }
    #[must_use]
    pub(crate) const fn all_terminals_registered_before_quarantine(&self) -> bool {
        self.all_terminals_registered_before_quarantine
    }

    #[cfg(test)]
    /// Test-only corruption seam for downstream transcript rejection tests.
    /// Production construction has no mutation path for this binding.
    pub(crate) fn test_corrupt_initial_forest_digest(&mut self) {
        self.initial_forest_digest = self.initial_forest_digest.wrapping_add(1);
    }

    #[cfg(test)]
    /// Test-only seam for downstream stage preflight rejection.
    pub(crate) fn test_corrupt_post_quarantine_forest_digest(&mut self) {
        self.post_quarantine_forest_digest = self.post_quarantine_forest_digest.wrapping_add(1);
    }

    #[cfg(test)]
    /// PR-8 test-only seam for downstream stage preflight rejection.
    ///
    /// The point of this one is that it moves NO geometry: it is exactly the
    /// shape of a regression in which the attestation quietly stops being
    /// performed while every forest digest stays put, which is the failure the
    /// separate transcript exists to catch.
    pub(crate) fn test_corrupt_contact_attestation_digest(&mut self) {
        self.contact_attestation_digest = self.contact_attestation_digest.wrapping_add(1);
    }

    #[cfg(test)]
    /// Test-only seam for downstream stage preflight rejection.
    pub(crate) fn test_clear_terminal_registration_evidence(&mut self) {
        self.all_terminals_registered_before_quarantine = false;
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub(crate) enum InitialContactError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Loads(#[from] GenericPerceptionError),
    #[error(transparent)]
    Forest(#[from] ForestError),
    #[error(transparent)]
    Geometry(#[from] AnchorSearchError),
    #[error("M0 constants are unavailable: {0}")]
    Constants(String),
    #[error("contact {contact_id} cannot be represented by the terminal namespace")]
    ContactIdOutOfRange { contact_id: u64 },
    #[error("physical contact id {contact_id} occurs more than once")]
    DuplicatePhysicalContact { contact_id: u64 },
    #[error("physical contact {contact_id} has an invalid position, normal, or carried area")]
    InvalidContact { contact_id: u64 },
    #[error("initial-contact {resource} requires {required}, limit is {limit}")]
    Limit {
        resource: &'static str,
        required: usize,
        limit: usize,
    },
    #[error("initial-contact resource arithmetic overflow for {0}")]
    ResourceOverflow(&'static str),
    #[error("initial-contact staging allocation failed for {0}")]
    Allocation(&'static str),
    #[error("initial contact interval batch failed: {0}")]
    NativeQuery(String),
    #[error("initial stub arithmetic crossed the finite boundary for contact {contact_id}")]
    NonFiniteStub { contact_id: u64 },
    /// PR-8. NOT a refused contact — a refused CONFIGURATION. The attestation
    /// asks the kernel to prove the stub is fused to the model over
    /// `default_contact_depth_mm` less one process resolution of margin (OD-4).
    /// If the run's own layer height is at least the contact depth there is no
    /// positive extent left to attest, so no contact in the run could be
    /// attested and the correct answer is to refuse the run rather than to
    /// quietly attest nothing. OD-4's "never a hard pipeline failure" governs a
    /// refused genuine-contact STUB; it does not license silently disabling the
    /// owner's governing requirement for every stub at once.
    #[error(
        "contact attestation needs a positive extent: contact depth {depth_mm} mm leaves nothing \
         above the {margin_mm} mm process-resolution margin"
    )]
    UnattestableContactDepth { depth_mm: f64, margin_mm: f64 },
    #[error("contact attestation batch failed: {0}")]
    Attestation(DesignedMaskError),
}

struct StubSeed {
    terminal: u32,
    p0: [Finite; 3],
    p1: [Finite; 3],
    r0: PositiveFinite,
    r1: PositiveFinite,
    carried_mm2: Finite,
    specialized: bool,
    /// OD-9 — the attachment point displaced OUTWARD along the contact normal
    /// by one process resolution. Computed here, at the only site that knows
    /// the outward sense of `normal`, rather than re-derived downstream where
    /// a sign slip would silently invert the screen.
    exterior_probe: [f64; 3],
    /// PR-4 non-regression (FU-CONTACT-1): the sizing policy was denied a
    /// larger tip by the registered `tip_radius_max_legacy_mm` clamp.
    tip_clamped_at_max: bool,
}

struct AxisBatchInput {
    p0: Vec<[f64; 3]>,
    p1: Vec<[f64; 3]>,
    r0: Vec<f64>,
    r1: Vec<f64>,
}

/// Validate every stale-able authority, add and register all physical-contact
/// stubs, make exactly one axis interval batch, then quarantine illegal ones.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_initial_contacts(
    plan: &SpecializedContactPlan,
    loads: &FinalizedContactLoads,
    mesh: &Mesh,
    perception_config: &SupportPerceptionConfig<'_>,
    frame: BuildFrame,
    layers: &plinth_core::LayerPlan,
    candidate_config: &CandidatePayloadConfig,
    reservations: &[SpecializedOwnerReservation],
    perception_limits: GenericPerceptionLimits,
    generic: &GenericContactPlan,
    regular: &SelectedContacts,
    selection_config: &ContactSelectionConfig<'_>,
    specialized_limits: SpecializedLimits,
    geometry: &AnchorGeometryAuthority<'_>,
    limits: ContactConstructionLimits,
) -> Result<InitialContactBuild, InitialContactError> {
    // This is intentionally first: no contact-sized allocation or forest
    // mutation is legal until the plan's complete authority matrix is fresh.
    plan.validate_binding(
        mesh,
        perception_config,
        frame,
        layers,
        candidate_config,
        reservations,
        perception_limits,
        generic,
        regular,
        selection_config,
        specialized_limits,
    )?;
    geometry.validate_mesh(mesh)?;
    let contacts = plan.contacts();
    let policy = m0_constants()
        .map_err(|error| InitialContactError::Constants(error.to_string()))?
        .placeholders();
    let contact_count = contacts.len();
    let interval_cap = usize::try_from(policy.initial_axis_majority_cap().diagnostic().get())
        .map_err(|_| InitialContactError::ResourceOverflow("majority cap"))?;
    let penetration_limits = geometry
        .surface()
        .penetration_intervals_batch_worst_case_limits(contact_count, 8, interval_cap, false)
        .map_err(|error| InitialContactError::NativeQuery(error.to_string()))?;
    let scratch_bytes = preflight(
        contact_count,
        plan.load_partition().rows().len(),
        penetration_limits,
        limits,
    )?;
    let mut contact_ids = Vec::new();
    contact_ids
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("contact ids"))?;
    contact_ids.extend(contacts.iter().map(|contact| contact.contact_id()));
    let mut sorted_ids = Vec::new();
    sorted_ids
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("contact id uniqueness"))?;
    sorted_ids.extend(contact_ids.iter().copied());
    sorted_ids.sort_unstable();
    if let Some(&contact_id) = sorted_ids
        .windows(2)
        .find_map(|pair| (pair[0] == pair[1]).then_some(&pair[0]))
    {
        return Err(InitialContactError::DuplicatePhysicalContact { contact_id });
    }
    loads.validate_binding(plan.load_partition(), &contact_ids)?;

    // PR-8/OD-9 sizing, fixed once for the whole run.
    //
    // Both numbers are the run's own process resolution — `max(LayerPlan
    // heights)`, a datum the run already carries — and neither is invented.
    //
    // **As the attested extent.** The attestation asks whether an interface
    // EXISTS and is renderable, and the resolvability floor is exactly one
    // layer: a fused region thinner than one layer cannot be rendered by the
    // slicer, so it cannot become a bond (PR-1(a)'s own argument, applied to
    // the depth the kernel is asked to confirm rather than to the depth the
    // stub is placed at).
    //
    // It is deliberately NOT the placement depth less a margin. That variant was
    // implemented, measured and DISCARDED because it refused reachable corpus
    // contacts that print today, and the refusals were geometry, not defects: a
    // disc of radius `r` pressed into a convex surface of radius `R` has its RIM
    // sitting `r^2/(2R)` shallower than its centre, so on a convex seat the outer
    // rays fall short of a demand set at the placement depth while the contact is
    // perfectly sound. (The refusal COUNT that used to stand here is struck under
    // OQ-11: the discarded variant was never instrumented, so the committed
    // census cannot reproduce the number. The geometric argument, which is what
    // this call rests on, is unaffected.) Demanding placement depth of every rim
    // ray conflates
    // attestation with the mask-validity proof, which is a different question
    // and legitimately does demand totality (see `validate_members_batch`).
    // What the shallow rim actually costs is AREA, and that is measured and
    // charged to PR-4 below rather than paid for by deleting the support.
    //
    // **As the OD-9 probe offset.** This reproduces the engine's own
    // established exterior-ness idiom: `NativeSurface::exterior_face_mask`
    // classifies a face by displacing sample points along its normal by 0.05 mm
    // — one layer at the corpus pitch — and asking `inside`. The screen this
    // stage runs is the per-attachment-point form of exactly that test, so the
    // offset is inherited rather than chosen. It is deliberately the SMALLEST
    // rung and not a ladder: a larger offset would walk further from the
    // surface and could leave a buried patch's covering solid entirely,
    // reporting "exterior" for the very case OD-9 exists to catch.
    let attestation_margin_mm = ContactPolicy::process_resolution_of(layers);
    let contact_depth_mm = policy.default_contact_depth_mm().diagnostic().get();
    let attested_extent = PositiveFiniteMm::new(attestation_margin_mm).map_err(|_| {
        InitialContactError::UnattestableContactDepth {
            depth_mm: contact_depth_mm,
            margin_mm: attestation_margin_mm,
        }
    })?;

    let tip_radius_max_mm = policy.tip_radius_max_legacy_mm().diagnostic().get();

    let native_digest = geometry.digest();
    let mut seeds = Vec::new();
    seeds
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("stub seeds"))?;
    for (slot, contact) in contacts.iter().enumerate() {
        let contact_id = contact.contact_id();
        let terminal = u32::try_from(contact_id)
            .map_err(|_| InitialContactError::ContactIdOutOfRange { contact_id })?;
        let position = contact.position_mm();
        let normal = contact.normal();
        let carried = loads.carried_area_mm2()[slot];
        if !position
            .iter()
            .chain(normal.iter())
            .chain(std::iter::once(&carried))
            .all(|value| value.is_finite())
            || carried < 0.0
        {
            return Err(InitialContactError::InvalidContact { contact_id });
        }
        let normal_length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
        if !normal_length.is_finite() || normal_length <= 0.0 {
            return Err(InitialContactError::InvalidContact { contact_id });
        }
        let unit = normal.map(|value| value / normal_length);
        let depth = contact_depth_mm;
        let span = policy.stub_length_mm().diagnostic().get();
        let p0 = std::array::from_fn(|axis| position[axis] - unit[axis] * depth);
        let p1 = std::array::from_fn(|axis| position[axis] + unit[axis] * span);
        // `p1` is `position + unit * span`, so `unit` is the OUTWARD sense by
        // construction and the probe steps away from the solid.
        let exterior_probe: [f64; 3] =
            std::array::from_fn(|axis| position[axis] + unit[axis] * attestation_margin_mm);
        if !exterior_probe.iter().all(|value| value.is_finite()) {
            return Err(InitialContactError::NonFiniteStub { contact_id });
        }
        let r0 = loads.tip_radii_mm()[slot];
        let r1 = PositiveFinite::new(r0.get().max(policy.min_shoot_r_mm().diagnostic().get()))
            .map_err(|_| InitialContactError::NonFiniteStub { contact_id })?;
        seeds.push(StubSeed {
            terminal,
            p0: finite(p0, contact_id)?,
            p1: finite(p1, contact_id)?,
            r0,
            r1,
            carried_mm2: Finite::new(carried)
                .map_err(|_| InitialContactError::InvalidContact { contact_id })?,
            specialized: !contact.demands().is_empty(),
            exterior_probe,
            // The sizing policy clamps at `tip_radius_max_legacy_mm`; a tip
            // sitting exactly there is one the policy wanted bigger.
            tip_clamped_at_max: r0.get() >= tip_radius_max_mm,
        });
    }

    crate::census_record!(if crate::census::enabled() {
        crate::census::record_contacts(contacts.iter().enumerate().map(|(slot, contact)| {
            let normal = contact.normal();
            let length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
            (
                contact.position_mm(),
                normal.map(|value| value / length),
                loads.tip_radii_mm()[slot].get(),
            )
        }));
    });

    let routing_order = routing_order(&seeds)?;
    let (overlaps, interval_batch_count) = classify_initial_overlaps(
        &seeds,
        geometry,
        policy.initial_axis_majority_step_mm().diagnostic().get(),
        interval_cap,
        policy.default_contact_depth_mm().diagnostic().get(),
        penetration_limits,
    )?;
    let mut terminals = Vec::new();
    terminals
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("terminal evidence"))?;

    let mut condemned = Vec::new();
    condemned
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("quarantine evidence"))?;
    // All recoverable validation, allocation, native work, and classification
    // have succeeded. The fresh local forest is now materialized. Allocations
    // for all contiguous contact-sized payloads, including the
    // forest arena vectors, are now attempted only after the logical payload
    // cap passed. Private BTree node layout and allocator rounding are outside
    // that portable cap and retain the process allocator's abort semantics.
    let mut keys = Vec::new();
    keys.try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("terminal keys"))?;
    let initial_forest_digest = Forest::new().digest();
    let mut forest = Forest::try_new_for_initial_contacts(contact_count)
        .map_err(|_| InitialContactError::Allocation("forest arena vectors"))?;
    for seed in &seeds {
        keys.push(forest.add_member(Member::new(MemberSpec {
            p0_mm: seed.p0,
            p1_mm: seed.p1,
            r0_mm: TerminalEndRadius::new(seed.r0),
            r1_mm: RootwardEndRadius::new(seed.r1),
            kind: MemberKind::Stub,
            owner: Owner::Contact(seed.terminal),
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: seed.carried_mm2,
        })));
    }
    for (seed, key) in seeds.iter().zip(&keys) {
        forest.register_terminal(seed.terminal, *key)?;
    }
    debug_assert!(seeds
        .iter()
        .all(|seed| forest.terminal(seed.terminal).is_some()));

    // PR-8 — AFFIRMATIVE CONTACT ATTESTATION.
    //
    // This is the owner's governing requirement: "be sure that the new code
    // properly attaches the tips to the mini for all cases". Every stub the
    // engine is about to hand to the routing stages has its model contact
    // affirmatively verified here, before anything is built on top of it, at a
    // cost of two batched native calls for the whole run.
    // MAJOR-7 — PR-4's area law finally has real load. The policy is assembled
    // from the same M0 placeholders every other contact-physics site reads, and
    // `tip_load` is the engine's own `FrozenTipLoadPolicy`, so the H2 allowance
    // is the registered `X_ALLOWED` chain rather than a second hand-typed
    // threshold.
    let contact_policy = ContactPolicy {
        floor_depth_mm: contact_depth_mm,
        process_resolution_mm: attestation_margin_mm,
        standoff_mm: policy.standoff_mm().diagnostic().get(),
        min_free_span_mm: policy.min_illegal_len_mm().diagnostic().get(),
        ball_factor_stub: policy.ball_factor_stub().diagnostic().get(),
        tip_load: crate::perception::FrozenTipLoadPolicy::from_placeholders(policy),
    };
    let attestation = attest_initial_contacts(
        &seeds,
        &forest,
        &keys,
        geometry,
        attested_extent,
        &contact_policy,
    )?;
    let mut unattested = Vec::new();
    unattested
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("attestation evidence"))?;
    let mut area_deficient = Vec::new();
    area_deficient
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("area deficit evidence"))?;

    let cutoff = policy.min_illegal_len_mm().diagnostic().get();
    for ((((seed, key), overlap), refusal), deficit) in seeds
        .iter()
        .zip(&keys)
        .zip(&overlaps)
        .zip(&attestation.refusals)
        .zip(&attestation.deficits)
    {
        // Both quarantine reasons are recorded even when both fire, so the
        // evidence never claims one cause for a stub that had two.
        if let Some(refusal) = refusal {
            unattested.push(UnattestedInitialStub {
                terminal: seed.terminal,
                key: *key,
                refusal: *refusal,
            });
        }
        if let Some(deficit) = deficit {
            area_deficient.push(DeficientInitialContact {
                terminal: seed.terminal,
                key: *key,
                deficit: *deficit,
            });
        }
        let condemned_by_overlap =
            initial_overlap_exceeds_cutoff(*overlap, cutoff, distance(seed.p0, seed.p1));
        if condemned_by_overlap {
            condemned.push(CondemnedInitialStub {
                terminal: seed.terminal,
                key: *key,
                remaining_inside_axis_mm: *overlap,
            });
        }
        if condemned_by_overlap || refusal.is_some() {
            forest.quarantine(*key)?;
        }
    }

    for seed in &seeds {
        let entry = forest
            .terminal_ledger(seed.terminal)
            .expect("registered terminal has ledger evidence");
        terminals.push(InitialTerminalEvidence {
            terminal: entry.contact,
            key: entry.original_key,
            state: entry.state,
        });
    }
    let post_quarantine_forest_digest = forest.digest();
    Ok(InitialContactBuild {
        forest,
        plan_digest: plan.digest(),
        load_digest: loads.digest(),
        policy_digest: policy_digest(policy),
        native_digest,
        initial_forest_digest,
        post_quarantine_forest_digest,
        routing_order,
        terminals,
        condemned,
        contact_attestation_digest: attestation.digest,
        attested_contacts: contact_count - unattested.len(),
        unattested,
        area_deficient,
        interval_batch_count,
        queried_stubs: contact_count,
        scratch_bytes,
        all_terminals_registered_before_quarantine: true,
    })
}

/// The verdicts of one PR-8 attestation stage, index-aligned with the seeds.
struct InitialAttestation {
    refusals: Vec<Option<ContactAttestationRefusal>>,
    deficits: Vec<Option<ContactAreaDeficit>>,
    digest: u64,
}

/// PR-8 — attest every initial contact stub in ONE pass.
///
/// Cost is two batched native calls for the whole run, independent of the
/// contact count: one `inside` batch carrying the OD-9 exterior-ness probes and
/// one `penetration_intervals_batch_bounded` carrying the endpoint-connectivity
/// rows. A per-stub call here would be the batch-first defect, and it is the
/// reason the verifier was written whole-forest in the first place.
///
/// What is proven, per contact:
///
/// * **OD-9** — the attachment point is on EXTERIOR surface. The probe one
///   process resolution outward along the contact normal must test outside the
///   model. Buried surface — a patch lying inside the printed solid because the
///   input mesh self-interpenetrates — tests inside and is refused. No other
///   check subsumes this one: a foot on a buried patch is genuinely on the
///   surface, and an embedded tip there satisfies the connectivity proof below
///   TRIVIALLY, because its whole neighbourhood is interior.
/// * **PR-8** — the stub's own tapered cross-section is connected to its
///   endpoint through the solid, and over HOW MUCH of that cross-section. The
///   eight rim rays plus the centreline are sampled independently, so an
///   interface that exists but is smaller than the tip disc is measured rather
///   than either ignored or mistaken for absence.
/// * **PR-4 (MAJOR-7)** — the H2 area-ratio allowance, applied to the ATTESTED
///   interface and the REAL routed load. Until now `contact::admit`'s only
///   production caller passed `carried_mm2 = 0.0`, so the law gated nothing.
///
/// Both refusals route to the caller's quarantine (OD-4); an area deficit is
/// recorded rather than refused, for the reason given on
/// [`ContactAreaDeficit`]. Only a BATCH-level integrity failure — a starved
/// resource admission or a malformed request set — is an error, and none of
/// those is a statement about any contact.
fn attest_initial_contacts(
    seeds: &[StubSeed],
    forest: &Forest,
    keys: &[MemberKey],
    geometry: &AnchorGeometryAuthority<'_>,
    extent: PositiveFiniteMm,
    policy: &ContactPolicy,
) -> Result<InitialAttestation, InitialContactError> {
    let mut refusals = Vec::new();
    refusals
        .try_reserve_exact(seeds.len())
        .map_err(|_| InitialContactError::Allocation("attestation verdicts"))?;
    refusals.resize(seeds.len(), None);
    let mut deficits = Vec::new();
    deficits
        .try_reserve_exact(seeds.len())
        .map_err(|_| InitialContactError::Allocation("attestation area deficits"))?;
    deficits.resize(seeds.len(), None);
    if seeds.is_empty() {
        let digest = attestation_digest(seeds, &refusals, &deficits, extent);
        return Ok(InitialAttestation {
            refusals,
            deficits,
            digest,
        });
    }

    let members = keys
        .iter()
        .map(|key| forest.member(*key).expect("stub was just added"))
        .collect::<Vec<_>>();
    let requests = seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| ContactAttestationRequest {
            member: index,
            // Every stub declares its model contact at the terminalward end,
            // which is the end the mint site seats into the model.
            end: ContactEnd::Terminalward,
            extent,
            exterior_probe: seed.exterior_probe,
            end_radius_mm: seed.r0.get(),
            carried_mm2: seed.carried_mm2.get(),
            tip_clamped_at_max: seed.tip_clamped_at_max,
        })
        .collect::<Vec<_>>();

    // The sampling resolution is the attestation's own sensitivity knob, and it
    // is deliberately tied to the extent being proven rather than left at a
    // default: the majority step must be able to RESOLVE the OD-4 margin
    // (OD-4's own words), so it is one tenth of the claimed extent.
    let majority_step =
        PositiveFiniteMm::new(extent.get() / 10.0).map_err(InitialContactError::Attestation)?;
    let limits = EndpointMaskValidationLimits::for_batch(
        geometry.surface(),
        requests.len(),
        majority_step,
        ENDPOINT_ATTESTATION_SAMPLES_PER_SUBSEGMENT,
    )
    .map_err(InitialContactError::Attestation)?;

    let outcomes = attest_members_batch(geometry.surface(), &members, &requests, limits, policy)
        .map_err(InitialContactError::Attestation)?;
    if outcomes.len() != seeds.len() {
        return Err(InitialContactError::Attestation(
            DesignedMaskError::CorruptKernelResult,
        ));
    }
    for outcome in &outcomes {
        // `member` is the request index, which is the seed index.
        if outcome.member >= seeds.len() {
            return Err(InitialContactError::Attestation(
                DesignedMaskError::CorruptKernelResult,
            ));
        }
        crate::census_record!(if crate::census::enabled() {
            crate::census::record_contact_attestation(
                seeds[outcome.member].terminal,
                outcome.engagement.centreline,
                outcome.engagement.rim_engaged,
                outcome.engagement.rim_total,
                outcome.attested_radius_mm,
                outcome.verdict.is_err(),
                outcome.area_deficit.is_some(),
            );
        });
        refusals[outcome.member] = outcome.verdict.as_ref().err().copied();
        deficits[outcome.member] = outcome.area_deficit;
    }
    let digest = attestation_digest(seeds, &refusals, &deficits, extent);
    Ok(InitialAttestation {
        refusals,
        deficits,
        digest,
    })
}

/// Per-subsegment sampling for the attestation batch.
///
/// This matches [`crate::designed_embed::ENDPOINT_VALIDATION_OFFSET_RAYS`]: the
/// witness needs the eight rim rays plus the centreline resolved at the same
/// density it samples them, and using one number for both keeps the sampling
/// isotropic around the stub.
const ENDPOINT_ATTESTATION_SAMPLES_PER_SUBSEGMENT: usize = 8;

/// The attestation transcript digest.
///
/// It covers every contact's verdict, ATTESTED ROWS INCLUDED, and the extent
/// that was claimed. That totality is the point: a forest digest cannot see an
/// attestation regression, because an attested contact and an unattested one
/// are the same geometry until the refusal quarantines it, and a refusal that
/// stopped firing leaves no geometric trace at all.
fn attestation_digest(
    seeds: &[StubSeed],
    refusals: &[Option<ContactAttestationRefusal>],
    deficits: &[Option<ContactAreaDeficit>],
    extent: PositiveFiniteMm,
) -> u64 {
    let mut digest = DigestWriter::new();
    digest.tag(0x8b);
    digest.f64(extent.get());
    digest.u64(seeds.len() as u64);
    for ((seed, refusal), deficit) in seeds.iter().zip(refusals).zip(deficits) {
        digest.u64(u64::from(seed.terminal));
        digest.tag(match refusal {
            None => 0,
            Some(ContactAttestationRefusal::BuriedSurface { .. }) => 1,
            Some(ContactAttestationRefusal::NotAttached { .. }) => 2,
        });
        // The deficit's measured radius is digested, not just its presence, so
        // an interface that quietly shrinks is visible even while it stays on
        // the same side of the allowance.
        match deficit {
            None => digest.tag(0),
            Some(deficit) => {
                digest.tag(1);
                digest.f64(deficit.attested_radius_mm);
                digest.f64(deficit.carried_mm2);
            }
        }
    }
    digest.finish()
}

fn classify_initial_overlaps(
    seeds: &[StubSeed],
    geometry: &AnchorGeometryAuthority<'_>,
    majority_step_mm: f64,
    interval_cap: usize,
    contact_depth_mm: f64,
    penetration_limits: PenetrationBatchLimits,
) -> Result<(Vec<f64>, usize), InitialContactError> {
    let mut overlaps = Vec::new();
    overlaps
        .try_reserve_exact(seeds.len())
        .map_err(|_| InitialContactError::Allocation("quarantine classification"))?;
    if seeds.is_empty() {
        return Ok((overlaps, 0));
    }
    let axis_batch = axis_batch_input(seeds)?;
    let intervals = geometry
        .surface()
        .penetration_intervals_batch_bounded(
            &axis_batch.p0,
            &axis_batch.p1,
            &axis_batch.r0,
            &axis_batch.r1,
            8,
            majority_step_mm,
            interval_cap,
            0.0,
            false,
            penetration_limits,
        )
        .map_err(|error| InitialContactError::NativeQuery(error.to_string()))?;
    for (seed, rows) in seeds.iter().zip(intervals) {
        let length = distance(seed.p0, seed.p1);
        overlaps.push(remaining_illegal_axis_overlap(
            &rows,
            length,
            contact_depth_mm,
        ));
    }
    Ok((overlaps, 1))
}

fn preflight(
    contact_count: usize,
    load_rows: usize,
    penetration: PenetrationBatchLimits,
    limits: ContactConstructionLimits,
) -> Result<usize, InitialContactError> {
    if contact_count > limits.max_contacts {
        return Err(InitialContactError::Limit {
            resource: "contacts",
            required: contact_count,
            limit: limits.max_contacts,
        });
    }
    if load_rows > limits.max_load_rows {
        return Err(InitialContactError::Limit {
            resource: "load rows",
            required: load_rows,
            limit: limits.max_load_rows,
        });
    }
    let work = contact_count
        .checked_mul(4)
        .and_then(|staging| staging.checked_add(penetration.max_total_work))
        .ok_or(InitialContactError::ResourceOverflow("work items"))?;
    if work > limits.max_work_items {
        return Err(InitialContactError::Limit {
            resource: "work items",
            required: work,
            limit: limits.max_work_items,
        });
    }
    let scratch_bytes = required_scratch_bytes(contact_count, penetration)?;
    if scratch_bytes > limits.max_scratch_bytes {
        return Err(InitialContactError::Limit {
            resource: "scratch bytes",
            required: scratch_bytes,
            limit: limits.max_scratch_bytes,
        });
    }
    Ok(scratch_bytes)
}

fn interval_output_bytes(
    contacts: usize,
    interval_count: usize,
) -> Result<usize, InitialContactError> {
    interval_count
        .checked_mul(std::mem::size_of::<Interval>())
        .and_then(|bytes| {
            bytes.checked_add(contacts.checked_mul(std::mem::size_of::<Vec<Interval>>())?)
        })
        .ok_or(InitialContactError::ResourceOverflow(
            "interval output bytes",
        ))
}

fn required_scratch_bytes(
    contacts: usize,
    penetration: PenetrationBatchLimits,
) -> Result<usize, InitialContactError> {
    let staging = contacts
        .checked_mul(std::mem::size_of::<StubSeed>())
        .and_then(|value| {
            value.checked_add(contacts.checked_mul(std::mem::size_of::<MemberKey>())?)
        })
        .and_then(|value| {
            value.checked_add(contacts.checked_mul(std::mem::size_of::<InitialTerminalEvidence>())?)
        })
        .and_then(|value| {
            value.checked_add(contacts.checked_mul(std::mem::size_of::<CondemnedInitialStub>())?)
        })
        .and_then(|value| value.checked_add(contacts.checked_mul(2 * std::mem::size_of::<u64>())?))
        .and_then(|value| {
            value.checked_add(contacts.checked_mul(2 * std::mem::size_of::<[f64; 3]>())?)
        })
        .and_then(|value| value.checked_add(contacts.checked_mul(2 * std::mem::size_of::<f64>())?))
        .and_then(|value| value.checked_add(contacts.checked_mul(std::mem::size_of::<u32>())?))
        .and_then(|value| {
            value.checked_add(contacts.checked_mul(2 * std::mem::size_of::<&StubSeed>())?)
        })
        .and_then(|value| value.checked_add(contacts.checked_mul(std::mem::size_of::<f64>())?))
        .ok_or(InitialContactError::ResourceOverflow("scratch bytes"))?;
    staging
        .checked_add(interval_output_bytes(
            contacts,
            penetration.max_total_intervals,
        )?)
        .and_then(|value| value.checked_add(penetration.max_scratch_bytes))
        .and_then(|value| value.checked_add(modeled_initial_contact_bytes(contacts)?))
        .ok_or(InitialContactError::ResourceOverflow("scratch bytes"))
}

fn finite(values: [f64; 3], contact_id: u64) -> Result<[Finite; 3], InitialContactError> {
    let [x, y, z] = values;
    Ok([
        Finite::new(x).map_err(|_| InitialContactError::NonFiniteStub { contact_id })?,
        Finite::new(y).map_err(|_| InitialContactError::NonFiniteStub { contact_id })?,
        Finite::new(z).map_err(|_| InitialContactError::NonFiniteStub { contact_id })?,
    ])
}

fn distance(a: [Finite; 3], b: [Finite; 3]) -> f64 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (a.get() - b.get()).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn remaining_illegal_axis_overlap(
    intervals: &[Interval],
    length: f64,
    terminalward_embed_depth: f64,
) -> f64 {
    intervals
        .iter()
        .map(|row| row.s_b.min(length) - row.s_a.max(terminalward_embed_depth))
        .fold(0.0_f64, f64::max)
}

fn initial_overlap_exceeds_cutoff(overlap: f64, cutoff: f64, axis_length: f64) -> bool {
    // `overlap` subtracts two independently rounded interval endpoints. A
    // nominal cutoff can therefore land slightly above its policy f64 when
    // the endpoints are at the full stub scale. Two machine epsilons at that
    // scale are the narrow forward-error budget for those endpoint roundings;
    // this is not a general-purpose fuzzy comparison.
    let endpoint_rounding_budget = 2.0 * f64::EPSILON * axis_length.max(cutoff);
    overlap > cutoff + endpoint_rounding_budget
}

fn axis_batch_input(seeds: &[StubSeed]) -> Result<AxisBatchInput, InitialContactError> {
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    for (label, values) in [("axis p0", &mut p0), ("axis p1", &mut p1)] {
        values
            .try_reserve_exact(seeds.len())
            .map_err(|_| InitialContactError::Allocation(label))?;
    }
    for (label, values) in [("axis r0", &mut r0), ("axis r1", &mut r1)] {
        values
            .try_reserve_exact(seeds.len())
            .map_err(|_| InitialContactError::Allocation(label))?;
    }
    for seed in seeds {
        p0.push(seed.p0.map(Finite::get));
        p1.push(seed.p1.map(Finite::get));
        r0.push(seed.r0.get());
        r1.push(seed.r1.get());
    }
    Ok(AxisBatchInput { p0, p1, r0, r1 })
}

fn routing_order(seeds: &[StubSeed]) -> Result<InitialRoutingOrder, InitialContactError> {
    let mut specialized = Vec::new();
    let mut regular = Vec::new();
    specialized
        .try_reserve_exact(seeds.len())
        .map_err(|_| InitialContactError::Allocation("specialized routing staging"))?;
    regular
        .try_reserve_exact(seeds.len())
        .map_err(|_| InitialContactError::Allocation("regular routing staging"))?;
    for seed in seeds {
        if seed.specialized {
            specialized.push(seed);
        } else {
            regular.push(seed);
        }
    }
    specialized.sort_by(|left, right| {
        cmp_number(left.p1[2].get(), right.p1[2].get())
            .then_with(|| cmp_number(right.carried_mm2.get(), left.carried_mm2.get()))
            .then_with(|| left.terminal.cmp(&right.terminal))
    });
    regular.sort_by(|left, right| {
        cmp_number(right.p1[2].get(), left.p1[2].get())
            .then_with(|| cmp_number(right.carried_mm2.get(), left.carried_mm2.get()))
            .then_with(|| left.terminal.cmp(&right.terminal))
    });
    let mut specialized_terminals = Vec::new();
    let mut regular_terminals = Vec::new();
    specialized_terminals
        .try_reserve_exact(specialized.len())
        .map_err(|_| InitialContactError::Allocation("specialized routing order"))?;
    regular_terminals
        .try_reserve_exact(regular.len())
        .map_err(|_| InitialContactError::Allocation("regular routing order"))?;
    specialized_terminals.extend(specialized.into_iter().map(|seed| seed.terminal));
    regular_terminals.extend(regular.into_iter().map(|seed| seed.terminal));
    Ok(InitialRoutingOrder {
        specialized_terminals,
        regular_terminals,
    })
}

fn cmp_number(left: f64, right: f64) -> Ordering {
    if left == right {
        Ordering::Equal
    } else {
        left.total_cmp(&right)
    }
}

fn policy_digest(policy: &plinth_core::M0PlaceholderConstants) -> u64 {
    let values = [
        policy.default_contact_depth_mm().diagnostic().get(),
        policy.stub_length_mm().diagnostic().get(),
        policy.min_shoot_r_mm().diagnostic().get(),
        policy.min_illegal_len_mm().diagnostic().get(),
        policy.initial_axis_majority_step_mm().diagnostic().get(),
        policy.initial_axis_majority_cap().diagnostic().get() as f64,
    ];
    fnv(values
        .iter()
        .flat_map(|value| value.to_bits().to_le_bytes()))
}

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use geom_kernel::{KnnBatchLimits, NativeSurface, NativeSurfaceConfig};
    use plinth_core::LayerPlan;
    use plinth_mesh::{
        build_face_topology, make_overhang_table, make_raking_blade, make_thin_fin, make_toed_slab,
        FaceTopology, WeldPolicy,
    };

    use super::*;
    use crate::{
        perception::{prepare_generic_contacts, FinalizedContactLoadLimits},
        select_contacts,
        specialized::{
            authority_consistent_empty_plan_for_initial_test, prepare_specialized_contacts,
        },
    };

    const SWEEP_CHILD: &str = "PLINTH_INITIAL_SWEEP_CHILD";
    const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: 1,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: 100_000_000,
    };
    const SWEEP_LINE: &str = "INITIAL_SWEEP=";

    struct Fixture {
        mesh: Mesh,
        delicacy: Vec<f64>,
        layers: LayerPlan,
        candidate: CandidatePayloadConfig,
        generic: GenericContactPlan,
        regular: SelectedContacts,
        plan: SpecializedContactPlan,
        loads: FinalizedContactLoads,
        surface: NativeSurface,
        topology: FaceTopology,
    }

    fn perception_config(delicacy: &[f64]) -> SupportPerceptionConfig<'_> {
        SupportPerceptionConfig::current(
            Finite::new(0.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            64 * 1024 * 1024,
            Some(delicacy),
        )
        .unwrap()
    }

    fn frame() -> BuildFrame {
        BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap())
    }

    fn combine_meshes(parts: &[Mesh]) -> Mesh {
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        for part in parts {
            let offset = u32::try_from(vertices.len()).unwrap();
            vertices.extend_from_slice(part.verts());
            faces.extend(
                part.tris()
                    .iter()
                    .map(|face| face.map(|index| index + offset)),
            );
        }
        Mesh::new(vertices, faces).unwrap()
    }

    fn stacked_obstruction(obstacle_top_z: f64, overhang_bottom_z: f64) -> Mesh {
        combine_meshes(&[
            plinth_mesh::make_box([30.0, 30.0, 1.0], [0.0, 0.0, obstacle_top_z - 1.0]).unwrap(),
            plinth_mesh::make_box([30.0, 30.0, 4.0], [0.0, 0.0, overhang_bottom_z]).unwrap(),
        ])
    }

    impl Fixture {
        fn new(mesh: Mesh) -> Self {
            Self::new_with_layer_count(mesh, 1_000)
        }

        fn new_with_layer_count(mesh: Mesh, layer_count: usize) -> Self {
            Self::new_with_layers(
                mesh,
                LayerPlan::new(0.0, vec![50.0 / layer_count as f64; layer_count]).unwrap(),
            )
        }

        fn new_with_layers(mesh: Mesh, layers: LayerPlan) -> Self {
            let delicacy = vec![0.0; mesh.tris().len()];
            let candidate =
                CandidatePayloadConfig::current(8, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536)
                    .unwrap();
            let config = perception_config(&delicacy);
            let generic = prepare_generic_contacts(
                &mesh,
                &config,
                frame(),
                &layers,
                &candidate,
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            )
            .unwrap();
            let selection =
                ContactSelectionConfig::current_defaults(Some(&delicacy), config.max_grid_bytes)
                    .unwrap();
            let regular = if generic.islands().is_empty() {
                SelectedContacts {
                    contacts: Vec::new(),
                    coverage_samples: Vec::new(),
                }
            } else {
                select_contacts(&mesh, generic.candidates(), generic.islands(), &selection).unwrap()
            };
            let plan = prepare_specialized_contacts(
                &mesh,
                &config,
                frame(),
                &layers,
                &candidate,
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
                &generic,
                &regular,
                &selection,
                SpecializedLimits::production(TEST_KNN_LIMITS),
            )
            .unwrap();
            let ids = plan
                .contacts()
                .iter()
                .map(|contact| contact.contact_id())
                .collect::<Vec<_>>();
            let loads = FinalizedContactLoads::from_partition(
                plan.load_partition(),
                &ids,
                FinalizedContactLoadLimits::new(
                    ids.len(),
                    plan.load_partition().rows().len(),
                    usize::MAX,
                ),
            )
            .unwrap();
            let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
            let surface = NativeSurface::from_indexed(
                mesh.verts(),
                mesh.tris(),
                NativeSurfaceConfig {
                    grid_pitch_mm: 1.0,
                    max_grid_bytes: 64 * 1024 * 1024,
                },
            )
            .unwrap();
            Self {
                mesh,
                delicacy,
                layers,
                candidate,
                generic,
                regular,
                plan,
                loads,
                surface,
                topology,
            }
        }

        fn build(
            &self,
            limits: ContactConstructionLimits,
        ) -> Result<InitialContactBuild, InitialContactError> {
            let config = perception_config(&self.delicacy);
            let selection = ContactSelectionConfig::current_defaults(
                Some(&self.delicacy),
                config.max_grid_bytes,
            )
            .unwrap();
            let geometry =
                AnchorGeometryAuthority::new(&self.mesh, &self.topology, &self.surface).unwrap();
            build_initial_contacts(
                &self.plan,
                &self.loads,
                &self.mesh,
                &config,
                frame(),
                &self.layers,
                &self.candidate,
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
                &self.generic,
                &self.regular,
                &selection,
                SpecializedLimits::production(TEST_KNN_LIMITS),
                &geometry,
                limits,
            )
        }
    }

    fn seed(terminal: u32, z: f64, carried_mm2: f64, specialized: bool) -> StubSeed {
        StubSeed {
            terminal,
            p0: [0.0, 0.0, z].map(|value| Finite::new(value).unwrap()),
            p1: [0.0, 0.0, z].map(|value| Finite::new(value).unwrap()),
            r0: PositiveFinite::new(0.2).unwrap(),
            r1: PositiveFinite::new(0.35).unwrap(),
            carried_mm2: Finite::new(carried_mm2).unwrap(),
            specialized,
            exterior_probe: [0.0, 0.0, z],
            tip_clamped_at_max: false,
        }
    }

    #[test]
    fn routing_order_canonicalizes_signed_zero_before_terminal_tie_break() {
        let order = routing_order(&[
            seed(9, -0.0, 2.0, true),
            seed(4, 0.0, 2.0, true),
            seed(8, -0.0, 1.0, false),
            seed(3, 0.0, 1.0, false),
        ])
        .unwrap();
        assert_eq!(order.specialized_terminals(), &[4, 9]);
        assert_eq!(order.regular_terminals(), &[3, 8]);
    }

    #[test]
    fn terminalward_embed_classifies_clean_transit_and_buried_boundaries() {
        let exact_cutoff = geom_kernel::Interval {
            s_a: 0.0,
            s_b: 0.05,
            entered: true,
            exited: true,
            max_depth: 0.1,
            ray: -1,
        };
        let exact_overlap = remaining_illegal_axis_overlap(&[exact_cutoff], 2.8, 0.0);
        assert_eq!(exact_overlap, 0.05);
        assert!(!initial_overlap_exceeds_cutoff(exact_overlap, 0.05, 2.8,));
        assert!(initial_overlap_exceeds_cutoff(0.06, 0.05, 2.8));
        let clean = geom_kernel::Interval {
            s_a: 0.3,
            s_b: 0.35,
            entered: true,
            exited: true,
            max_depth: 0.1,
            ray: -1,
        };
        let clean_overlap = remaining_illegal_axis_overlap(std::slice::from_ref(&clean), 2.8, 0.3);
        assert!(clean_overlap <= 0.05);
        assert!(!initial_overlap_exceeds_cutoff(clean_overlap, 0.05, 2.8));
        let transit = geom_kernel::Interval {
            s_a: 0.2,
            s_b: 0.6,
            ..clean
        };
        assert!(remaining_illegal_axis_overlap(&[transit], 2.8, 0.3) > 0.05);
        let buried = geom_kernel::Interval {
            s_a: 0.0,
            s_b: 0.3,
            ..clean
        };
        assert_eq!(remaining_illegal_axis_overlap(&[buried], 2.8, 0.3), 0.0);
    }

    #[test]
    fn scratch_preflight_rejects_overflow() {
        assert_eq!(
            required_scratch_bytes(
                usize::MAX,
                PenetrationBatchLimits {
                    max_rows: usize::MAX,
                    max_total_work: usize::MAX,
                    max_total_hits: usize::MAX,
                    max_total_samples: usize::MAX,
                    max_total_intervals: usize::MAX,
                    max_scratch_bytes: usize::MAX,
                },
            ),
            Err(InitialContactError::ResourceOverflow("scratch bytes"))
        );
    }

    #[test]
    fn zero_contact_builder_returns_a_fresh_empty_forest_without_a_native_query() {
        let fixture = Fixture::new(make_overhang_table().unwrap());
        assert!(!fixture.plan.contacts().is_empty());
        let config = perception_config(&fixture.delicacy);
        let selection = ContactSelectionConfig::current_defaults(
            Some(&fixture.delicacy),
            config.max_grid_bytes,
        )
        .unwrap();
        let plan = authority_consistent_empty_plan_for_initial_test(
            &fixture.mesh,
            &config,
            frame(),
            &fixture.layers,
            &fixture.candidate,
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            &fixture.generic,
            &fixture.regular,
            &selection,
            SpecializedLimits::production(TEST_KNN_LIMITS),
        )
        .unwrap();
        let loads = FinalizedContactLoads::from_partition(
            plan.load_partition(),
            &[],
            FinalizedContactLoadLimits::new(0, 0, usize::MAX),
        )
        .unwrap();
        let geometry =
            AnchorGeometryAuthority::new(&fixture.mesh, &fixture.topology, &fixture.surface)
                .unwrap();
        let build = build_initial_contacts(
            &plan,
            &loads,
            &fixture.mesh,
            &config,
            frame(),
            &fixture.layers,
            &fixture.candidate,
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            &fixture.generic,
            &fixture.regular,
            &selection,
            SpecializedLimits::production(TEST_KNN_LIMITS),
            &geometry,
            ContactConstructionLimits::production(),
        )
        .unwrap();
        assert!(plan.contacts().is_empty());
        assert_eq!(
            plan.contacts()
                .iter()
                .map(|contact| contact.demands().len())
                .sum::<usize>(),
            0
        );
        assert!(plan.load_partition().rows().is_empty());
        assert_eq!(build.queried_stubs(), 0);
        assert_eq!(build.interval_batch_count(), 0);
        assert!(build.terminals().is_empty());
        assert!(build.condemned().is_empty());
        assert_eq!(
            build.initial_forest_digest(),
            build.post_quarantine_forest_digest()
        );
    }

    #[test]
    fn real_table_build_materializes_exact_registered_stub_geometry_and_ledger() {
        let fixture = Fixture::new(make_overhang_table().unwrap());
        let build = fixture
            .build(ContactConstructionLimits::production())
            .unwrap();
        assert!(!fixture.plan.contacts().is_empty());
        assert_eq!(build.queried_stubs(), fixture.plan.contacts().len());
        assert_eq!(build.interval_batch_count(), 1);
        assert!(build.all_terminals_registered_before_quarantine());
        assert_eq!(build.terminals().len(), fixture.plan.contacts().len());
        let policy = m0_constants().unwrap().placeholders();
        for (slot, (contact, evidence)) in fixture
            .plan
            .contacts()
            .iter()
            .zip(build.terminals())
            .enumerate()
        {
            let record = build
                .forest()
                .terminal_ledger_evidence(evidence.terminal)
                .unwrap();
            let normal = contact.normal();
            let norm = normal.into_iter().map(|v| v * v).sum::<f64>().sqrt();
            let unit = normal.map(|v| v / norm);
            let expected_p0 = std::array::from_fn(|axis| {
                contact.position_mm()[axis]
                    - unit[axis] * policy.default_contact_depth_mm().diagnostic().get()
            });
            let expected_p1 = std::array::from_fn(|axis| {
                contact.position_mm()[axis]
                    + unit[axis] * policy.stub_length_mm().diagnostic().get()
            });
            assert_eq!(record.p0_mm.map(Finite::get), expected_p0);
            assert_eq!(record.p1_mm.map(Finite::get), expected_p1);
            assert_eq!(record.r0_mm.get(), fixture.loads.tip_radii_mm()[slot]);
            assert_eq!(
                record.r1_mm.get().get(),
                fixture.loads.tip_radii_mm()[slot]
                    .get()
                    .max(policy.min_shoot_r_mm().diagnostic().get())
            );
            assert_eq!(record.kind, MemberKind::Stub);
            assert_eq!(record.owner, Owner::Contact(evidence.terminal));
            assert_eq!(record.model_contact_end, Some(ContactEnd::Terminalward));
            assert_eq!(
                record.carried_mm2.get(),
                fixture.loads.carried_area_mm2()[slot]
            );
            assert!(record.certificate.cert_version.is_none());
            assert!(matches!(
                evidence.state,
                TerminalLedgerState::Live | TerminalLedgerState::Quarantined
            ));
        }
    }

    #[test]
    fn geometry_mismatch_and_cap_minus_one_refuse_without_a_result_forest() {
        let table = Fixture::new(make_overhang_table().unwrap());
        let toe = Fixture::new(make_toed_slab().unwrap());
        let config = perception_config(&table.delicacy);
        let selection =
            ContactSelectionConfig::current_defaults(Some(&table.delicacy), config.max_grid_bytes)
                .unwrap();
        let alien = AnchorGeometryAuthority::new(&toe.mesh, &toe.topology, &toe.surface).unwrap();
        assert!(matches!(
            build_initial_contacts(
                &table.plan,
                &table.loads,
                &table.mesh,
                &config,
                frame(),
                &table.layers,
                &table.candidate,
                &[],
                GenericPerceptionLimits::production(TEST_KNN_LIMITS),
                &table.generic,
                &table.regular,
                &selection,
                SpecializedLimits::production(TEST_KNN_LIMITS),
                &alien,
                ContactConstructionLimits::production(),
            ),
            Err(InitialContactError::Geometry(
                AnchorSearchError::TopologyMeshMismatch
            ))
        ));
        let count = table.plan.contacts().len();
        let penetration = table
            .surface
            .penetration_intervals_batch_worst_case_limits(count, 8, 80, false)
            .unwrap();
        let required = required_scratch_bytes(count, penetration).unwrap();
        let required_work = count * 4 + penetration.max_total_work;
        let exact = ContactConstructionLimits {
            max_contacts: count,
            max_load_rows: table.plan.load_partition().rows().len(),
            max_work_items: required_work,
            max_scratch_bytes: required,
        };
        assert!(table.build(exact).is_ok());
        assert!(matches!(
            table.build(ContactConstructionLimits {
                max_scratch_bytes: required - 1,
                ..exact
            }),
            Err(InitialContactError::Limit {
                resource: "scratch bytes",
                ..
            })
        ));
    }

    #[test]
    fn table_and_toed_slab_real_plans_return_complete_fresh_forests() {
        let table = Fixture::new(make_overhang_table().unwrap())
            .build(ContactConstructionLimits::production())
            .unwrap();
        let toe = Fixture::new(make_toed_slab().unwrap())
            .build(ContactConstructionLimits::production())
            .unwrap();
        for build in [&table, &toe] {
            assert_eq!(build.terminals().len(), build.queried_stubs());
            assert_eq!(
                build.interval_batch_count(),
                usize::from(build.queried_stubs() > 0)
            );
            assert!(build.all_terminals_registered_before_quarantine());
            assert!(build
                .terminals()
                .iter()
                .all(|terminal| { build.forest().terminal_ledger(terminal.terminal).is_some() }));
            // Quarantine has TWO causes since PR-8: the post-mask overlap
            // cutoff and a refused contact attestation, and a stub can carry
            // both. The invariant is that the quarantined set is exactly the
            // UNION of the two evidence lists — no quarantine without a
            // recorded reason, and no recorded reason without a quarantine.
            let quarantined = build
                .terminals()
                .iter()
                .filter(|terminal| terminal.state == TerminalLedgerState::Quarantined)
                .map(|terminal| terminal.terminal)
                .collect::<std::collections::BTreeSet<_>>();
            let explained = build
                .condemned()
                .iter()
                .map(|stub| stub.terminal)
                .chain(build.unattested().iter().map(|stub| stub.terminal))
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(quarantined, explained);
            // An area deficit is evidence, NOT a quarantine: the contact stays
            // live. Pinning that here is what stops a future edit from turning
            // the deficit into a silent support deletion.
            for deficient in build.area_deficient() {
                assert!(
                    !quarantined.contains(&deficient.terminal),
                    "terminal {} carries an area deficit and must remain live",
                    deficient.terminal
                );
            }
        }
        // Non-vacuity: `make_toed_slab` really does exercise the refusal path,
        // so the union assertion above is not comparing two empty sets.
        assert!(
            !toe.unattested().is_empty(),
            "the toed slab is the fixture that carries a real attestation refusal"
        );
        assert_eq!(
            table.attested_contacts() + table.unattested().len(),
            table.queried_stubs()
        );
    }

    #[test]
    fn real_stacked_obstruction_accepts_cutoff_and_condemns_above_it() {
        let cutoff = m0_constants()
            .unwrap()
            .placeholders()
            .min_illegal_len_mm()
            .diagnostic()
            .get();
        // The overhang underside is z=20 and its downward 2.5 mm stub ends at
        // z=17.5. Construct the obstacle at that real free-end coordinate plus
        // the exact policy cutoff; classification must absorb only the narrow
        // endpoint-subtraction rounding budget, not move the geometry inward.
        let free_end_z = 17.5;
        let boundary_top = free_end_z + cutoff;
        let boundary_fixture = Fixture::new(stacked_obstruction(boundary_top, 20.0));
        assert!(boundary_fixture
            .mesh
            .verts()
            .iter()
            .any(|vertex| vertex[2] == boundary_top));
        let boundary = boundary_fixture
            .build(ContactConstructionLimits::production())
            .unwrap();
        assert!(!boundary.terminals().is_empty());
        assert!(boundary.condemned().is_empty());
        assert!(boundary
            .terminals()
            .iter()
            .all(|terminal| terminal.state == TerminalLedgerState::Live));

        let geometry = AnchorGeometryAuthority::new(
            &boundary_fixture.mesh,
            &boundary_fixture.topology,
            &boundary_fixture.surface,
        )
        .unwrap();
        let boundary_seeds = boundary
            .terminals()
            .iter()
            .map(|terminal| {
                let record = boundary
                    .forest()
                    .terminal_ledger_evidence(terminal.terminal)
                    .unwrap();
                StubSeed {
                    terminal: terminal.terminal,
                    p0: record.p0_mm,
                    p1: record.p1_mm,
                    r0: record.r0_mm.get(),
                    r1: record.r1_mm.get(),
                    carried_mm2: record.carried_mm2,
                    specialized: false,
                    exterior_probe: record.p0_mm.map(Finite::get),
                    tip_clamped_at_max: false,
                }
            })
            .collect::<Vec<_>>();
        let (boundary_overlaps, batch_count) = classify_initial_overlaps(
            &boundary_seeds,
            &geometry,
            m0_constants()
                .unwrap()
                .placeholders()
                .initial_axis_majority_step_mm()
                .diagnostic()
                .get(),
            80,
            m0_constants()
                .unwrap()
                .placeholders()
                .default_contact_depth_mm()
                .diagnostic()
                .get(),
            geometry
                .surface()
                .penetration_intervals_batch_worst_case_limits(boundary_seeds.len(), 8, 80, false)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(batch_count, 1);
        assert!(boundary_overlaps.iter().any(|overlap| *overlap > 0.0));
        assert!(boundary_overlaps
            .iter()
            .any(|overlap| (*overlap - cutoff).abs() <= 2.0 * f64::EPSILON * 2.8));
        assert!(boundary_overlaps
            .iter()
            .all(|overlap| !initial_overlap_exceeds_cutoff(*overlap, cutoff, 2.8)));

        let above = Fixture::new(stacked_obstruction(free_end_z + 0.06, 20.0))
            .build(ContactConstructionLimits::production())
            .unwrap();
        assert!(!above.condemned().is_empty());
        assert!(above
            .condemned()
            .iter()
            .all(|row| row.remaining_inside_axis_mm > cutoff));
    }

    #[test]
    fn buried_free_ends_quarantine_and_retain_both_ledger_records_and_loads() {
        let fixture = Fixture::new(make_overhang_table().unwrap());
        let build = fixture
            .build(ContactConstructionLimits::production())
            .unwrap();
        assert!(!build.condemned().is_empty());
        for condemned in build.condemned() {
            let terminal = condemned.terminal;
            let slot = fixture
                .plan
                .contacts()
                .iter()
                .position(|contact| contact.contact_id() == u64::from(terminal))
                .unwrap();
            let sealed = build.forest().terminal_ledger_evidence(terminal).unwrap();
            let quarantined = build.forest().terminal_quarantine(terminal).unwrap();
            assert!(
                fixture
                    .surface
                    .inside(&[quarantined.p1_mm.map(Finite::get)])[0]
            );
            assert_eq!(build.forest().terminal(terminal), None);
            assert_eq!(sealed.terminal_contacts, [terminal]);
            assert_eq!(quarantined.terminal_contacts, [terminal]);
            assert_eq!(sealed.original_key, condemned.key);
            assert_eq!(quarantined.original_key, condemned.key);
            assert_eq!(
                sealed.carried_mm2.get(),
                fixture.loads.carried_area_mm2()[slot]
            );
            assert_eq!(
                quarantined.carried_mm2.get(),
                fixture.loads.carried_area_mm2()[slot]
            );
            assert_eq!(
                build.forest().terminal_ledger(terminal).unwrap().state,
                TerminalLedgerState::Quarantined
            );
        }
    }

    #[test]
    fn initial_thread_sweep_child() {
        if std::env::var_os(SWEEP_CHILD).is_none() {
            return;
        }
        let fixture = Fixture::new(make_overhang_table().unwrap());
        let evidence = [
            fixture.plan.generic_slice_thread_evidence(),
            Fixture::new(make_raking_blade().unwrap())
                .plan
                .generic_slice_thread_evidence(),
            Fixture::new(make_thin_fin().unwrap())
                .plan
                .generic_slice_thread_evidence(),
            Fixture::new_with_layer_count(make_toed_slab().unwrap(), 20_000)
                .plan
                .generic_slice_thread_evidence(),
        ]
        .into_iter()
        .max()
        .unwrap();
        let build = fixture
            .build(ContactConstructionLimits::production())
            .unwrap();
        let requested = std::env::var("PLINTH_THREADS")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert_eq!(evidence.0, requested);
        assert!((1..=requested).contains(&evidence.1));
        println!(
            "{SWEEP_LINE}{}:{}:{}:{}:{}",
            build.post_quarantine_forest_digest(),
            build.queried_stubs(),
            build.condemned().len(),
            evidence.0,
            evidence.1
        );
    }

    #[test]
    fn real_initial_digest_is_identical_at_one_four_and_sixteen_threads() {
        if std::env::var_os(SWEEP_CHILD).is_some() {
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let signatures = [1, 4, 16]
            .into_iter()
            .map(|threads| {
                let output = Command::new(&executable)
                    .arg("--exact")
                    .arg("initial::tests::initial_thread_sweep_child")
                    .arg("--nocapture")
                    .env(SWEEP_CHILD, "1")
                    .env("PLINTH_THREADS", threads.to_string())
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                String::from_utf8(output.stdout)
                    .unwrap()
                    .split_once(SWEEP_LINE)
                    .unwrap()
                    .1
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let parsed = signatures
            .iter()
            .map(|signature| {
                let (prefix, participating) = signature.rsplit_once(':').unwrap();
                let (fingerprint, effective) = prefix.rsplit_once(':').unwrap();
                (
                    fingerprint,
                    effective.parse::<usize>().unwrap(),
                    participating.parse::<usize>().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert!(parsed.windows(2).all(|pair| pair[0].0 == pair[1].0));
        for ((_, effective, participating), requested) in parsed.iter().zip([1, 4, 16]) {
            assert_eq!(*effective, requested);
            assert!((1..=requested).contains(participating));
        }
    }
}
