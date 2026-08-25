//! Initial contact stubs are constructed once, then either remain terminal
//! seeds or are quarantined.  This leaf deliberately does not route or grow.

use std::cmp::Ordering;

use geom_kernel::{Interval, PenetrationBatchLimits, PenetrationBatchMeter};
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_mesh::Mesh;
use thiserror::Error;

use crate::{
    anchor::{AnchorGeometryAuthority, AnchorSearchError},
    contact::ContactPolicy,
    designed_embed::{
        attest_members_batch, grow_tips_to_attest, ContactAreaDeficit, ContactAttestationRefusal,
        ContactAttestationRequest, DesignedMaskError, EndpointMaskValidationLimits,
        PositiveFiniteMm, TipGrowthAttempt, TipGrowthRequest,
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

/// Why re-seating was not among the remedies tried.
///
/// One variant today, and it is a fact about this stage rather than a gap: an
/// initial contact's legal seat set has exactly ONE member, the point perception
/// chose. The tournament machinery that can offer a cheap legal alternative
/// belongs to the two TOUCHING families, which generate several candidate seats
/// and pick between them. Moving an initial contact means asking perception for
/// a different contact, which is another stage's decision.
///
/// It is an enum rather than a comment so that a stage which later DOES gain an
/// alternative seat set has somewhere to say so, and so the disclosure can never
/// silently imply a remedy was skipped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReseatAvailability {
    /// The seat set has one member; there is no alternative seat to try.
    NoAlternativeSeatAtThisStage,
}

/// Every remedy this stage had for one contact that failed PR-8, and what each
/// one measured. This is the "remedy attempts" half of the disclosure: a
/// refusal must say what was tried, not merely that it failed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InitialContactRemedy {
    /// Rung zero — the tip RADIUS in mm the load law and the registered clamp
    /// produced, and the radius PR-8 refused. Double it for the DIAMETER the
    /// slicer UI shows.
    pub(crate) law_radius_mm: f64,
    /// Every growth rung tried above the law radius, ascending, with its
    /// verdict. Empty means the law radius was already the registered maximum,
    /// so there was no larger legal tip to try.
    pub(crate) growth: Vec<TipGrowthAttempt>,
    /// The ceiling the ladder stopped at, in mm RADIUS — the SMALLER of the
    /// registered `tip_radius_max_grown_scar_mm` and `tip_radius_max_legacy_mm`.
    /// Recorded so the disclosure states the limit and not just the failure.
    pub(crate) growth_ceiling_radius_mm: f64,
    /// WHICH registered constant that ceiling is, by its registry id, so the
    /// disclosure can name the stopper rather than quote a bare number.
    pub(crate) growth_ceiling_constant: &'static str,
    /// **Whether the growth ladder was WALKED for this contact at all.**
    ///
    /// Only a `NotAttached` refusal enters the ladder. A `BuriedSurface`
    /// refusal is one no tip radius can move, so it is refused before any rung
    /// exists, and the ceiling recorded above is only the one growth WOULD have
    /// used. Without this flag the two cases are indistinguishable downstream —
    /// both carry an empty `growth` list — and the disclosure would name a
    /// registered maximum as the stopper of a rescue that was never attempted.
    ///
    /// An empty `growth` with this set TRUE is a different and honest thing:
    /// the ladder ran and had no legal rung to offer, because the load law had
    /// already sized this tip at or above the ceiling. There the ceiling IS the
    /// stopper.
    ///
    /// Deliberately NOT digested. `attestation_digest` already separates the
    /// two cases at a coarser grain — it tags a remedy `None` (no ladder) apart
    /// from `Some` (ladder walked) — and it runs before the fallback remedy
    /// below is substituted, so digesting this would add a byte that cannot
    /// vary at the point the digest is taken.
    pub(crate) ladder_entered: bool,
    pub(crate) reseat: ReseatAvailability,
}

/// PR-8 evidence for one stub whose model contact could NOT be affirmatively
/// attested, **after every remedy this stage has was tried and failed**.
///
/// This is a candidate refusal routed to the same quarantine the overlap cutoff
/// uses, exactly as OD-4 requires — never a hard pipeline failure. The stub, its
/// terminal ledger record and its carried load all survive quarantine, so a
/// refusal is visible and accountable rather than a support that silently
/// vanished.
///
/// **The load is named because it is being abandoned.** A quarantined stub stops
/// carrying, nothing is told to grow in its place, and the coverage measurement
/// works at a 6 mm radius — far too coarse to see a single missing claw tip. So
/// the area this contact was routed is recorded here, in mm^2, and disclosed.
/// The failure this closes is a run that dropped 2.98 mm^2 of Deinonychus toe
/// load in silence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UnattestedInitialStub {
    pub(crate) terminal: u32,
    pub(crate) key: MemberKey,
    pub(crate) refusal: ContactAttestationRefusal,
    /// The seat, in mm — the contact point itself, not the stub's outer end.
    pub(crate) seat_mm: [f64; 3],
    /// The routed load this refusal abandons, mm^2.
    pub(crate) abandoned_mm2: f64,
    pub(crate) remedy: InitialContactRemedy,
}

/// PR-8 evidence for one stub whose load-sized tip did not attest but whose
/// GROWN tip did.
///
/// The emitted radius is the smallest radius MEASURED — over the ascending
/// ladder and the descending refinement scan together — that satisfies both of
/// the tip's duties, the load law and attachment. So it is the honest minimum
/// for this contact rather than a concession to either one. It is recorded
/// because a grown tip is a real geometry change the run made on its own
/// initiative, and because a regression that stops growing must be visible in
/// something other than the absence of a support.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GrownInitialContact {
    pub(crate) terminal: u32,
    pub(crate) key: MemberKey,
    /// Where the grown tip stands, mm — the contact point itself, not the
    /// stub's outer end. Carried for the same reason the refusal record carries
    /// it: a scar the run resized on its own initiative is one an operator has
    /// to be able to FIND on the model, and a contact id alone does not locate
    /// anything.
    pub(crate) seat_mm: [f64; 3],
    /// mm RADIUS the load law asked for.
    pub(crate) law_radius_mm: f64,
    /// mm RADIUS actually emitted. Always `>= law_radius_mm`, so the load duty
    /// is satisfied at least as well as before growth.
    pub(crate) emitted_radius_mm: f64,
    /// Every radius tried, in the order it was measured: the ascending ladder,
    /// then the descending refinement scan. The CURE is the smallest attesting
    /// entry, which is not in general the last one.
    pub(crate) attempts: Vec<TipGrowthAttempt>,
    /// The load this growth KEPT, mm^2 — the area that would otherwise have
    /// been abandoned at quarantine.
    pub(crate) carried_mm2: f64,
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
    grown: Vec<GrownInitialContact>,
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
    /// PR-8 — every stub whose model contact was refused after every remedy
    /// this stage has was tried, with its reason, the load it abandons and the
    /// remedies that were attempted.
    #[must_use]
    pub(crate) fn unattested(&self) -> &[UnattestedInitialStub] {
        &self.unattested
    }
    /// PR-8 — every stub whose load-sized tip did not attest and whose GROWN
    /// tip did, with the ladder that got there.
    ///
    /// Each row is a support that would have been quarantined, and a routed
    /// load that would have been abandoned, before grow-to-attest existed.
    #[must_use]
    pub(crate) fn grown(&self) -> &[GrownInitialContact] {
        &self.grown
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
    /// FU-8g H2. The interval batch executed more than the preflight charged
    /// against the production caps. The census pass counts the demand and the
    /// fill pass executes it over the same rays with the same predicates, so
    /// this is impossible by construction; it exists so a future edit that
    /// lets the counting and the executing walks disagree refuses loudly here
    /// instead of quietly overspending an admitted charge.
    #[error("initial-contact executed {executed} {resource}; the preflight charged {charged}")]
    ChargeDrift {
        resource: &'static str,
        charged: usize,
        executed: usize,
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
    /// #36. The stub family's frozen seat zones could not be computed. This is
    /// a BATCH-level failure — a starved band admission or a mesh/topology
    /// disagreement — and never a statement about any one contact, so it takes
    /// the same channel every other batch integrity failure here takes rather
    /// than quarantining a stub.
    #[error("initial stub seat-zone freeze failed: {0}")]
    SeatZone(crate::seat_zone::SeatZoneError),
}

struct StubSeed {
    terminal: u32,
    /// The contact point itself, in mm — the seat. `p0` is one contact depth
    /// INSIDE it along the normal, so the two are different points and the
    /// disclosure must report this one.
    seat_mm: [f64; 3],
    p0: [Finite; 3],
    p1: [Finite; 3],
    /// What the load law and the registered clamp asked for, before any
    /// grow-to-attest. Kept beside `r0` so a disclosure can state both, and so
    /// growth can never be mistaken for the law having wanted a bigger tip.
    law_r0: PositiveFinite,
    /// The tip radius the run will EMIT. Equal to `law_r0` unless
    /// grow-to-attest raised it.
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
    // FU-8g H1 — count before charge before allocate. The logical contact and
    // load-row caps are still checked HERE, before any contact-sized
    // allocation; the work-items and scratch-bytes charges move below the
    // stub-seed loop, because they are now charged from the census pass's
    // counted totals and the census needs the stub geometry to count.
    preflight_contact_caps(contact_count, plan.load_partition().rows().len(), limits)?;
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
    // Craig's measured field ceiling on a GROWN scar, 0.400 mm RADIUS
    // (0.800 mm DIAMETER). It is read here and handed to growth alone. It is
    // deliberately NOT mixed into `tip_radius_max_mm` above: that value is the
    // LOAD LAW's clamp maximum and drives `tip_clamped_at_max`, and a load-sized
    // tip above the scar bound is legal, unaffected and emitted as it always
    // was. See `designed_embed::growth_ceiling`.
    let tip_radius_max_grown_scar_mm = *m0_constants()
        .map_err(|error| InitialContactError::Constants(error.to_string()))?
        .evidence()
        .tip_radius_max_grown_scar_mm()
        .value();
    let min_shoot_r_mm = policy.min_shoot_r_mm().diagnostic().get();
    // `STANDOFF_MM` — the PLACEMENT standoff, and the second term of the frozen
    // seat-zone band radius `r_seat + standoff_placement` (design §2). It is
    // the mask formula's own standoff, NOT a demanded gap; the two-tier ruling
    // of 2026-08-22 separated those and this is the former.
    let standoff_mm = policy.standoff_mm().diagnostic().get();

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
        let r1 = PositiveFinite::new(r0.get().max(min_shoot_r_mm))
            .map_err(|_| InitialContactError::NonFiniteStub { contact_id })?;
        seeds.push(StubSeed {
            terminal,
            seat_mm: position,
            p0: finite(p0, contact_id)?,
            p1: finite(p1, contact_id)?,
            law_r0: r0,
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

    // PR-8 — AFFIRMATIVE CONTACT ATTESTATION, and its remedy.
    //
    // This is the owner's governing requirement: "be sure that the new code
    // properly attaches the tips to the mini for all cases". Every stub the
    // engine is about to hand to the routing stages has its model contact
    // affirmatively verified here, before anything is built on top of it, at a
    // cost of two batched native calls for the whole run plus at most one
    // bounded ladder of rounds for the contacts that need regrowing.
    //
    // MAJOR-7 — PR-4's area law finally has real load. The policy is assembled
    // from the same M0 placeholders every other contact-physics site reads, and
    // `tip_load` is the engine's own `FrozenTipLoadPolicy`, so the H2 allowance
    // is the registered `X_ALLOWED` chain rather than a second hand-typed
    // threshold.
    //
    // **Why this runs HERE, before the census, the overlap classification and
    // the forest.** Grow-to-attest can change a tip radius, and a tip radius is
    // an input to all three: the census sizes the native charge from the stub
    // capsules, the overlap classification sweeps those same capsules, and the
    // forest stores the geometry the run will emit. Attesting after them would
    // leave every one of those computed for a tip the run is no longer going to
    // build — a stale charge, a stale overlap verdict and a forest that
    // disagrees with its own attestation. Deciding the radius first is what
    // keeps all four honest, and it costs nothing: the attestation admits its
    // own native work through `EndpointMaskValidationLimits::for_batch` and
    // never spent the construction charge.
    let contact_policy = ContactPolicy {
        floor_depth_mm: contact_depth_mm,
        process_resolution_mm: attestation_margin_mm,
        standoff_mm: policy.standoff_mm().diagnostic().get(),
        // This policy ATTESTS seats; it never grades a clearance distance, so
        // no `segment_clearance*` call reads this field on this path. It
        // carries the placement value because the contacts it attests are
        // screened next by the placement-family search.
        demanded_gap_mm: policy.standoff_mm().diagnostic().get(),
        min_free_span_mm: policy.min_illegal_len_mm().diagnostic().get(),
        ball_factor_stub: policy.ball_factor_stub().diagnostic().get(),
        tip_load: crate::perception::FrozenTipLoadPolicy::from_placeholders(policy),
    };
    let attestation = attest_initial_contacts(
        &mut seeds,
        geometry,
        attested_extent,
        &contact_policy,
        tip_radius_max_mm,
        tip_radius_max_grown_scar_mm,
        min_shoot_r_mm,
    )?;

    // FU-8g H1 — the census pass. The stub batch input is built ONCE and used
    // twice: by the counting pass here and by the interval batch inside
    // `classify_initial_overlaps`. The counted totals (work as traversal
    // tests, hits as crossings, scratch and intervals as per-row sums — never
    // `rows x faces`) are what the preflight below charges against the
    // UNCHANGED production caps, and they are also the per-row allocation
    // budgets the interval batch executes under.
    //
    // It reads `seeds` AFTER the attestation above, so a regrown tip is charged
    // and swept at the radius it will actually be built at.
    let majority_step_mm = policy.initial_axis_majority_step_mm().diagnostic().get();
    let axis_batch = axis_batch_input(&seeds)?;
    let penetration_limits = if seeds.is_empty() {
        // No stubs, no native query: the zero demand is stated as zero.
        PenetrationBatchLimits {
            max_rows: 0,
            max_total_work: 0,
            max_total_hits: 0,
            max_total_samples: 0,
            max_total_intervals: 0,
            max_scratch_bytes: 0,
        }
    } else {
        let census = geometry
            .surface()
            .penetration_census_batch(
                &axis_batch.p0,
                &axis_batch.p1,
                &axis_batch.r0,
                &axis_batch.r1,
                8,
                false,
            )
            .map_err(|error| InitialContactError::NativeQuery(error.to_string()))?;
        geometry
            .surface()
            .penetration_intervals_batch_census_limits(
                &census,
                8,
                interval_cap,
                majority_step_mm,
                false,
            )
            .map_err(|error| InitialContactError::NativeQuery(error.to_string()))?
    };
    let scratch_bytes = preflight_work(contact_count, penetration_limits, limits)?;

    crate::census_record!(if crate::census::enabled() {
        // The EMITTED radius, out of the seed, not the load law's request out
        // of `loads`. Grow-to-attest can raise one, and a census that reported
        // the request would be describing a tip the run is not building.
        crate::census::record_contacts(contacts.iter().enumerate().map(|(slot, contact)| {
            let normal = contact.normal();
            let length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
            (
                contact.position_mm(),
                normal.map(|value| value / length),
                seeds[slot].r0.get(),
            )
        }));
    });

    let routing_order = routing_order(&seeds)?;
    let (overlaps, interval_batch_count, meter) = classify_initial_overlaps(
        &seeds,
        &axis_batch,
        geometry,
        majority_step_mm,
        interval_cap,
        policy.default_contact_depth_mm().diagnostic().get(),
        penetration_limits,
    )?;
    // FU-8g H2 — the receipt, checked against the charge, at the site that
    // spent it. `penetration_limits` is exactly what `preflight_work` charged
    // against `ContactConstructionLimits::production()` a few lines above.
    charge_covers_execution(meter, penetration_limits)?;
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
        keys.push(forest.add_member(stub_member(seed)));
    }
    // **The honest contact exemption, creation site 1 of 4 (#36, design §8
    // step 4 / §9.4).** A stub is a designed `contact_depth_mm` embed: it is
    // legitimately inside the material it is seated in, so its own seat zone
    // must be exempt or the full-axis grading of §8 step 5 refuses every stub
    // for its own design.
    //
    // The stub family was NOT in §8's original inventory and had to be added:
    // `brace::exact_clearance_rows` excludes `MemberKind::Stub` by kind, but
    // `cco::clearance_rows` does not — it walks `live_tree_members` and keeps
    // every CHANGED member — so an unfrozen stub self-refuses there and CCO
    // growth loses its candidates wholesale.
    //
    // Frozen HERE rather than at seed time because grow-to-attest can still
    // raise `r0` above, and the frozen band radius must be the radius the
    // contact is MINTED with. Cost is one batched projection plus one batched
    // band call for the whole run, riding the same stage boundary as the
    // attestation batches; a per-stub call would be the batch-first defect.
    freeze_stub_seat_zones(&mut forest, &seeds, &keys, geometry, standoff_mm)?;
    for (seed, key) in seeds.iter().zip(&keys) {
        forest.register_terminal(seed.terminal, *key)?;
    }
    debug_assert!(seeds
        .iter()
        .all(|seed| forest.terminal(seed.terminal).is_some()));

    let mut unattested = Vec::new();
    unattested
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("attestation evidence"))?;
    let mut grown = Vec::new();
    grown
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("tip growth evidence"))?;
    let mut area_deficient = Vec::new();
    area_deficient
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("area deficit evidence"))?;

    let cutoff = policy.min_illegal_len_mm().diagnostic().get();
    let (fallback_growth_ceiling_mm, fallback_growth_ceiling_source) =
        crate::designed_embed::growth_ceiling_radius_mm(
            tip_radius_max_mm,
            tip_radius_max_grown_scar_mm,
        );
    let fallback_growth_ceiling_constant = fallback_growth_ceiling_source.constant_id();
    for (((((seed, key), overlap), refusal), deficit), remedy) in seeds
        .iter()
        .zip(&keys)
        .zip(&overlaps)
        .zip(&attestation.refusals)
        .zip(&attestation.deficits)
        .zip(&attestation.remedies)
    {
        // Both quarantine reasons are recorded even when both fire, so the
        // evidence never claims one cause for a stub that had two.
        if let Some(refusal) = refusal {
            unattested.push(UnattestedInitialStub {
                terminal: seed.terminal,
                key: *key,
                refusal: *refusal,
                seat_mm: seed.seat_mm,
                abandoned_mm2: seed.carried_mm2.get(),
                remedy: remedy.clone().unwrap_or_else(|| InitialContactRemedy {
                    law_radius_mm: seed.law_r0.get(),
                    growth: Vec::new(),
                    // This stub never entered the ladder — it was refused for a
                    // reason no tip radius can move (a buried seat). The ceiling
                    // stated is nevertheless the one growth WOULD have used, so
                    // the two disclosure paths quote the same limit — but
                    // `ladder_entered: false` says plainly that it did not stop
                    // this one, so the line cannot read as a policy refusal.
                    growth_ceiling_radius_mm: fallback_growth_ceiling_mm,
                    growth_ceiling_constant: fallback_growth_ceiling_constant,
                    ladder_entered: false,
                    reseat: ReseatAvailability::NoAlternativeSeatAtThisStage,
                }),
            });
        }
        // A tip the run REGREW to make it attest. Recorded whether or not it
        // ends up quarantined for overlap, because the growth happened and a
        // regression that stops it must move something visible.
        if seed.r0.get() > seed.law_r0.get() {
            grown.push(GrownInitialContact {
                terminal: seed.terminal,
                key: *key,
                seat_mm: seed.seat_mm,
                law_radius_mm: seed.law_r0.get(),
                emitted_radius_mm: seed.r0.get(),
                attempts: remedy
                    .as_ref()
                    .map(|remedy| remedy.growth.clone())
                    .unwrap_or_default(),
                carried_mm2: seed.carried_mm2.get(),
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
        grown,
        area_deficient,
        interval_batch_count,
        queried_stubs: contact_count,
        scratch_bytes,
        all_terminals_registered_before_quarantine: true,
    })
}

/// **#36 — freeze the stub family's seat zones, one batch for the whole run.**
///
/// `Z` is the connected component, containing the seat's own face, of the faces
/// within `r_seat + standoff_placement` of the stub's axis (design §2). For a
/// stub the contact end is `Terminalward`, so `r_seat` is `seed.r0` — the tip
/// radius the run will EMIT, after grow-to-attest, which is why this runs at
/// the mint and not at seed time.
///
/// The seat FACE comes from one batched `project` over the seats. That is the
/// same rule `brace::exact_clearance_rows` and `cco::clearance_survival` use to
/// resolve a contact's surface normal, so the face this forgives is the face
/// those sites would have measured the seat against — the identity the flood
/// fill rests on. `validate_perception_input` builds the retained topology with
/// `WeldPolicy::ExactBits` and `NativeSurface::from_indexed` indexes faces in
/// the same triangle order, so `project`'s face ID and `topology.neighbors`
/// address the SAME faces (design §10.6, verified independently by the
/// challenger and re-recorded there).
///
/// A projection that lands off the mesh (`face_id < 0`, which `project` returns
/// for an empty or degenerate surface) is left UNFROZEN rather than guessed at:
/// no zone means "exempt nothing", which is the strict direction.
fn freeze_stub_seat_zones(
    forest: &mut Forest,
    seeds: &[StubSeed],
    keys: &[MemberKey],
    geometry: &AnchorGeometryAuthority<'_>,
    standoff_mm: f64,
) -> Result<(), InitialContactError> {
    if seeds.is_empty() {
        return Ok(());
    }
    let seats = seeds.iter().map(|seed| seed.seat_mm).collect::<Vec<_>>();
    // ONE projection for the whole run.
    let (_, _, face_ids, _) = geometry.surface().project(&seats);
    let mut rows = Vec::new();
    let mut requests = Vec::new();
    for ((seed, key), face_id) in seeds.iter().zip(keys).zip(&face_ids) {
        let Ok(seat_face_id) = u32::try_from(*face_id) else {
            continue;
        };
        rows.push(*key);
        requests.push(crate::seat_zone::SeatZoneRequest {
            p0_mm: seed.p0.map(Finite::get),
            p1_mm: seed.p1.map(Finite::get),
            seat_face_id,
            // The contact end's radius at the moment of the mint, plus the
            // placement standoff. The transient sizer skips stubs and a
            // registered terminal cannot be mutated, but CCO consumes that
            // registration before later rewire moves. A consumed stub can then
            // move through `try_slide` or `try_retire`; the downstream re-size
            // compares this stored segment-band shape with the live member and
            // re-cuts it when either the radius or axis changed.
            band_radius_mm: seed.r0.get() + standoff_mm,
        });
    }
    // ONE band call plus a parallel flood fill per row.
    let records = crate::seat_zone::freeze_production_seat_zones(
        geometry.surface(),
        geometry.topology(),
        &requests,
    )
    .map_err(InitialContactError::SeatZone)?;
    for (key, record) in rows.into_iter().zip(records) {
        forest.set_seat_zone(key, record);
    }
    Ok(())
}

/// The exact member one stub seed becomes.
///
/// One function, called by BOTH the attestation and the forest build, so the
/// geometry that is measured and the geometry that is placed cannot drift
/// apart. They used to be two literals in two places; grow-to-attest makes the
/// radius mutable between them, and two literals would have been a way for the
/// run to attest one tip and emit another.
fn stub_member(seed: &StubSeed) -> Member {
    Member::new(MemberSpec {
        p0_mm: seed.p0,
        p1_mm: seed.p1,
        r0_mm: TerminalEndRadius::new(seed.r0),
        r1_mm: RootwardEndRadius::new(seed.r1),
        kind: MemberKind::Stub,
        owner: Owner::Contact(seed.terminal),
        model_contact_end: Some(ContactEnd::Terminalward),
        carried_mm2: seed.carried_mm2,
    })
}

/// The verdicts of one PR-8 attestation stage, index-aligned with the seeds.
struct InitialAttestation {
    refusals: Vec<Option<ContactAttestationRefusal>>,
    deficits: Vec<Option<ContactAreaDeficit>>,
    /// What grow-to-attest tried for each seed that needed it. `None` for a
    /// seat that attested at its load-sized tip and never entered the ladder.
    remedies: Vec<Option<InitialContactRemedy>>,
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
    seeds: &mut [StubSeed],
    geometry: &AnchorGeometryAuthority<'_>,
    extent: PositiveFiniteMm,
    policy: &ContactPolicy,
    // `tip_radius_max_legacy_mm` — the LOAD LAW's clamp maximum, mm RADIUS.
    tip_radius_max_mm: f64,
    // `tip_radius_max_grown_scar_mm` — Craig's field ceiling on a GROWN scar,
    // mm RADIUS. Handed to growth alone; it bounds nothing the load law sizes.
    tip_radius_max_grown_scar_mm: f64,
    min_shoot_r_mm: f64,
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
    let mut remedies = Vec::new();
    remedies
        .try_reserve_exact(seeds.len())
        .map_err(|_| InitialContactError::Allocation("attestation remedies"))?;
    remedies.resize(seeds.len(), None);
    if seeds.is_empty() {
        let digest = attestation_digest(seeds, &refusals, &deficits, &remedies, extent);
        return Ok(InitialAttestation {
            refusals,
            deficits,
            remedies,
            digest,
        });
    }

    // The exact members the forest build will place, built through the one
    // shared `stub_member`. They are not competing candidates: every one of
    // them is committed geometry, which is why an ungradable row below is
    // still a TOTAL refusal.
    let owned_members = seeds.iter().map(stub_member).collect::<Vec<_>>();
    let members = owned_members.iter().collect::<Vec<_>>();
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
    // #28 — this family attests COMMITTED members. Every entry in `members`
    // came out of the one `stub_member` the forest build also uses, so an
    // ungradable row here is not a
    // candidate proposal that can leave the tournament: it is evidence about a
    // load-carrying member that could not be graded AT ALL — an orphan or
    // out-of-order kernel interval, a non-finite target, a boundary outside the
    // span the producer is entitled to. Nothing can be said about such a row.
    //
    // `witness_batch` grades PER ROW so a CANDIDATE family
    // (`attest_seats_batch`) can discard one unmeasurable proposal and still
    // measure the other thirty-nine, and that is the whole point of #28. Left
    // unhandled here, the same per-row verdict would reach this caller's
    // quarantine (`refusal.is_some()` in `build_initial_contacts`) and delete a
    // placed stub on a measurement nobody made, while the run carried on
    // quietly — a silent deletion where this path used to stop loudly. So the
    // inner result is collected back into a TOTAL refusal, for the same reason
    // `validate_members_batch` is total and with the same failure this path
    // produced before grading went per-row.
    if let Some(cause) = outcomes.iter().find_map(|outcome| match &outcome.verdict {
        Err(ContactAttestationRefusal::UnusableGeometry { cause, .. }) => Some(*cause),
        _ => None,
    }) {
        return Err(InitialContactError::Attestation(cause));
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

    // GROW-TO-ATTEST. A tip refused `NotAttached` was sized for its LOAD alone,
    // and the ring PR-8 probes with is that same tip radius, so the thinner the
    // load law allows the tip to be, the less of the model the probe can reach.
    // Before this existed, such a seat was quarantined and the area it carried
    // was abandoned with nothing told to take it over.
    //
    // Only `NotAttached` enters the ladder, and the other two verdicts are
    // excluded for reasons, not by omission:
    //
    // * `BuriedSurface` — the OD-9 probe is a point on the surface normal, so
    //   no tip radius moves it. Growing a buried seat would re-ask the same
    //   question and get the same answer. Burial is a mesh fault.
    // * `UnusableGeometry` — already collected into a TOTAL build refusal
    //   above, before this point is reached.
    let growth_requests = seeds
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            matches!(
                refusals[*index],
                Some(ContactAttestationRefusal::NotAttached { .. })
            )
        })
        .map(|(index, seed)| TipGrowthRequest {
            row: index,
            p0_mm: seed.p0.map(Finite::get),
            p1_mm: seed.p1.map(Finite::get),
            end: ContactEnd::Terminalward,
            exterior_probe: seed.exterior_probe,
            carried_mm2: seed.carried_mm2.get(),
            law_radius_mm: seed.law_r0.get(),
            opposite_end_minimum_radius_mm: min_shoot_r_mm,
            max_radius_mm: tip_radius_max_mm,
            scar_bound_radius_mm: tip_radius_max_grown_scar_mm,
            tip_clamped_at_max: seed.tip_clamped_at_max,
        })
        .collect::<Vec<_>>();
    if !growth_requests.is_empty() {
        let outcomes = grow_tips_to_attest(
            geometry.surface(),
            &growth_requests,
            extent,
            majority_step,
            ENDPOINT_ATTESTATION_SAMPLES_PER_SUBSEGMENT,
            policy,
        )
        .map_err(InitialContactError::Attestation)?;
        for outcome in outcomes {
            let index = outcome.row;
            if index >= seeds.len() {
                return Err(InitialContactError::Attestation(
                    DesignedMaskError::CorruptKernelResult,
                ));
            }
            remedies[index] = Some(InitialContactRemedy {
                law_radius_mm: outcome.law_radius_mm,
                growth: outcome.attempts.clone(),
                growth_ceiling_radius_mm: outcome.growth_ceiling_radius_mm,
                growth_ceiling_constant: outcome.growth_ceiling_source.constant_id(),
                // The ladder RAN for this row: only `NotAttached` rows are in
                // `growth_requests`, and every one of them got a ceiling and a
                // (possibly empty) rung list from `grow_tips_to_attest`. An
                // empty list here means the ceiling had no legal rung to offer,
                // which is the ceiling genuinely stopping the rescue.
                ladder_entered: true,
                reseat: ReseatAvailability::NoAlternativeSeatAtThisStage,
            });
            let Some(cure) = outcome.cure else {
                // The ladder ran out at its ceiling — the registered scar bound
                // or the registered legacy maximum, whichever is smaller. The
                // refusal stands, and `build_initial_contacts` discloses it with
                // the load it abandons, every rung recorded above, and the
                // constant that stopped it NAMED.
                continue;
            };
            // The tip the run will BUILD is the tip that attested. `r0` is the
            // emitted radius; `law_r0` is untouched, so the disclosure can
            // always say what the load law asked for.
            seeds[index].r0 = PositiveFinite::new(cure.radius_mm)
                .map_err(|_| InitialContactError::Attestation(DesignedMaskError::InvalidExtent))?;
            seeds[index].r1 = PositiveFinite::new(cure.opposite_end_radius_mm)
                .map_err(|_| InitialContactError::Attestation(DesignedMaskError::InvalidExtent))?;
            crate::census_record!(if crate::census::enabled() {
                crate::census::record_contact_attestation(
                    seeds[index].terminal,
                    cure.attestation.engagement.centreline,
                    cure.attestation.engagement.rim_engaged,
                    cure.attestation.engagement.rim_total,
                    cure.attestation.attested_radius_mm,
                    false,
                    cure.attestation.area_deficit.is_some(),
                );
            });
            refusals[index] = None;
            deficits[index] = cure.attestation.area_deficit;
        }
    }

    let digest = attestation_digest(seeds, &refusals, &deficits, &remedies, extent);
    Ok(InitialAttestation {
        refusals,
        deficits,
        remedies,
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
    remedies: &[Option<InitialContactRemedy>],
    extent: PositiveFiniteMm,
) -> u64 {
    let mut digest = DigestWriter::new();
    digest.tag(0x8b);
    digest.f64(extent.get());
    digest.u64(seeds.len() as u64);
    for (((seed, refusal), deficit), remedy) in
        seeds.iter().zip(refusals).zip(deficits).zip(remedies)
    {
        digest.u64(u64::from(seed.terminal));
        // GROW-TO-ATTEST, digested at every rung. Both radii are covered
        // because they answer different questions: `law_r0` is what the load
        // law asked for and `r0` is what the run will emit, and a regression
        // that stops growing, grows to a different rung, or grows the wrong
        // contact moves one of them. The ladder itself is digested so a
        // change in the STEP is visible even where the winning radius happens
        // to land in the same place.
        digest.f64(seed.law_r0.get());
        digest.f64(seed.r0.get());
        match remedy {
            None => digest.tag(0),
            Some(remedy) => {
                digest.tag(1);
                digest.f64(remedy.growth_ceiling_radius_mm);
                // The ceiling's NAME as well as its number: a change that swaps
                // which registered constant bounds growth without moving the
                // value it lands on is still a change to what the run promises,
                // and the transcript must see it.
                digest.u64(remedy.growth_ceiling_constant.len() as u64);
                for byte in remedy.growth_ceiling_constant.as_bytes() {
                    digest.tag(*byte);
                }
                digest.u64(remedy.growth.len() as u64);
                for attempt in &remedy.growth {
                    digest.f64(attempt.radius_mm);
                    digest.tag(u8::from(attempt.attested));
                }
                digest.tag(match remedy.reseat {
                    ReseatAvailability::NoAlternativeSeatAtThisStage => 0,
                });
            }
        }
        digest.tag(match refusal {
            None => 0,
            Some(ContactAttestationRefusal::BuriedSurface { .. }) => 1,
            Some(ContactAttestationRefusal::NotAttached { .. }) => 2,
            // #28 — this family attests PLACED members, so `attest_initial_contacts`
            // collects an ungradable row back into a batch-level
            // `InitialContactError::Attestation` BEFORE any transcript is
            // written. No verdict that reaches this digest can therefore carry
            // this variant. The arm exists so that if that collection is ever
            // removed, the transcript says so instead of the compiler being
            // told to look away. The collection itself is pinned by test
            // `ungradable_evidence_on_a_placed_stub_refuses_the_build`.
            Some(ContactAttestationRefusal::UnusableGeometry { .. }) => 3,
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
    axis_batch: &AxisBatchInput,
    geometry: &AnchorGeometryAuthority<'_>,
    majority_step_mm: f64,
    interval_cap: usize,
    contact_depth_mm: f64,
    penetration_limits: PenetrationBatchLimits,
) -> Result<(Vec<f64>, usize, PenetrationBatchMeter), InitialContactError> {
    let mut overlaps = Vec::new();
    overlaps
        .try_reserve_exact(seeds.len())
        .map_err(|_| InitialContactError::Allocation("quarantine classification"))?;
    if seeds.is_empty() {
        return Ok((overlaps, 0, PenetrationBatchMeter::default()));
    }
    // FU-8g H2 — the metered entry point. The receipt comes back with the
    // intervals so the charge this run spent against the production caps can
    // be checked against what the fill pass actually executed, rather than
    // assumed to cover it.
    let (intervals, meter) = geometry
        .surface()
        .penetration_intervals_batch_bounded_metered(
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
    Ok((overlaps, 1, meter))
}

/// The charged-vs-executed meter, beside the charge (FU-8g H2, the G3/G6c
/// idiom). `charged` is the very number `preflight_work` spent against the
/// unchanged production caps — counted by the census pass at the preflight
/// site — and `meter` is what the fill pass then executed. `work` and `hits`
/// are the same quantities counted by the two walks, so on an undrifted pair
/// they are EQUAL, not merely under; `samples` and `intervals` are bounded
/// and land at or under.
///
/// The kernel checks its receipt against its OWN counting pass. This check is
/// the independent one: it holds the receipt against the number the caller
/// charged, which is what a cap is written in and what a resource plan
/// promised. A run that outruns its charge refuses here rather than silently
/// spending more than it was admitted for.
fn charge_covers_execution(
    meter: PenetrationBatchMeter,
    charged: PenetrationBatchLimits,
) -> Result<(), InitialContactError> {
    for (resource, charged, executed) in [
        ("rows", charged.max_rows, meter.rows),
        ("work", charged.max_total_work, meter.work),
        ("hits", charged.max_total_hits, meter.hits),
        ("samples", charged.max_total_samples, meter.samples),
        ("intervals", charged.max_total_intervals, meter.intervals),
    ] {
        if executed > charged {
            return Err(InitialContactError::ChargeDrift {
                resource,
                charged,
                executed,
            });
        }
    }
    Ok(())
}

/// The logical contact and load-row caps, checked before any contact-sized
/// allocation exists. Split from [`preflight_work`] by FU-8g H1: these two
/// caps need no counting, so they still refuse first.
fn preflight_contact_caps(
    contact_count: usize,
    load_rows: usize,
    limits: ContactConstructionLimits,
) -> Result<(), InitialContactError> {
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
    Ok(())
}

/// The work-items and scratch-bytes charges, on the census pass's counted
/// totals: `penetration` carries the batch's honest demand (sums over rows —
/// work as executed traversal tests, scratch and intervals as counted per-row
/// budgets), so the charge here is the demand the interval batch will
/// actually execute and allocate, checked against the UNCHANGED caps.
fn preflight_work(
    contact_count: usize,
    penetration: PenetrationBatchLimits,
    limits: ContactConstructionLimits,
) -> Result<usize, InitialContactError> {
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
                SpecializedLimits::production(
                    TEST_KNN_LIMITS,
                    crate::specialized::test_batch_work(),
                ),
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
                SpecializedLimits::production(
                    TEST_KNN_LIMITS,
                    crate::specialized::test_batch_work(),
                ),
                &geometry,
                limits,
            )
        }
    }

    fn seed(terminal: u32, z: f64, carried_mm2: f64, specialized: bool) -> StubSeed {
        StubSeed {
            terminal,
            seat_mm: [0.0, 0.0, z],
            p0: [0.0, 0.0, z].map(|value| Finite::new(value).unwrap()),
            p1: [0.0, 0.0, z].map(|value| Finite::new(value).unwrap()),
            law_r0: PositiveFinite::new(0.2).unwrap(),
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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

    /// **#36 CREATION SITE 1 — every initial stub leaves the mint with a frozen
    /// seat zone** (§8 step 6, §9.4).
    ///
    /// The stub family was NOT in §8's original inventory and had to be added.
    /// `brace::exact_clearance_rows` excludes `MemberKind::Stub` by kind, so the
    /// final pass never grades one — but `cco::clearance_rows` does not: it
    /// walks `live_tree_members` and keeps every CHANGED member. A stub is a
    /// designed `default_contact_depth_mm` embed, so an unfrozen one is graded
    /// over its own design and refuses itself, and CCO growth loses its
    /// candidates wholesale.
    ///
    /// This runs the real builder over the real overhang table, so it is the
    /// production path and not a re-implementation of it.
    ///
    /// **The named sabotage: `a_mint_that_skipped_the_stub_freeze`.** Delete the
    /// `freeze_stub_seat_zones` call and every assertion below fails at once.
    /// The band radius is pinned separately, against the EMITTED tip radius
    /// rather than the load law's request, because grow-to-attest can raise one
    /// and a band frozen from the law's number would describe a tip the run is
    /// not building.
    #[test]
    fn every_initial_stub_leaves_the_mint_with_a_frozen_seat_zone() {
        let fixture = Fixture::new(make_overhang_table().unwrap());
        let build = fixture
            .build(ContactConstructionLimits::production())
            .unwrap();
        assert!(
            !fixture.plan.contacts().is_empty(),
            "NON-VACUITY: the fixture must produce contacts"
        );

        let forest = build.forest();
        let face_count = fixture.topology.n_faces();
        let mut frozen = 0_usize;
        for key in forest.live_members() {
            let member = forest.member(key).expect("a live member resolves");
            if member.model_contact_end().is_none() {
                continue;
            }
            let zone = forest.seat_zone(key).unwrap_or_else(|| {
                panic!(
                    "stub {key:?} declares a model contact and must carry a frozen \
                     seat zone; without one the honest full-axis grading refuses \
                     it for its own designed embed (§9.4)"
                )
            });
            assert!(
                !zone.is_empty(),
                "a seated stub must forgive its own seat face; {key:?} forgives \
                 nothing"
            );
            assert!(
                zone.len() < face_count,
                "and it must forgive its SEAT, not the mesh: {key:?} claims {} of \
                 {face_count} faces",
                zone.len()
            );
            assert!(
                zone.windows(2).all(|pair| pair[0] < pair[1]),
                "the kernel's exclusion set must be sorted and deduplicated"
            );
            frozen += 1;
        }
        // Every stub that SURVIVED the mint, not merely most of them. A
        // quarantined stub leaves `live_members`, so the arithmetic states
        // exactly which population is covered rather than leaving a shortfall
        // to be read as noise.
        assert!(frozen > 0, "NON-VACUITY: the fixture must freeze something");
        assert_eq!(
            frozen,
            fixture.plan.contacts().len() - build.unattested().len(),
            "every LIVE stub must be frozen; {} of {} were quarantined by PR-8 \
             and correctly leave the forest",
            build.unattested().len(),
            fixture.plan.contacts().len()
        );
    }

    /// **#36 — the stub band radius is the EMITTED tip, not the load law's
    /// request** (§8 step 6, companion to the freeze test above).
    ///
    /// `r_seat` is "the member's radius at the contact end at the moment the
    /// contact was minted". Grow-to-attest can raise `r0` above `law_r0` before
    /// the forest is built, which is exactly why the freeze runs at the mint and
    /// not at seed time. This pins the difference where a fixture can see it:
    /// the zone the production path froze must equal the one a request built
    /// from the EMITTED radius produces, for every stub.
    ///
    /// **The named sabotage: `a_band_frozen_from_the_law_radius`.** Swap
    /// `seed.r0` for `seed.law_r0` in `freeze_stub_seat_zones` and this test
    /// separates them on any contact that grew.
    ///
    /// **MEASURED 2026-08-22, and stated rather than hidden: that sabotage is
    /// SILENT on this fixture.** The overhang table grows no tip, so
    /// `law_r0 == r0` for every stub and the two bands are the same number. The
    /// test asserts the growth population directly, so the day a fixture does
    /// grow a tip the sabotage becomes loud automatically — and until then a
    /// reader is told which arm is unexercised instead of inferring coverage
    /// this test does not have.
    #[test]
    fn the_stub_seat_zone_band_is_the_emitted_tip_radius_plus_the_placement_standoff() {
        let fixture = Fixture::new(make_overhang_table().unwrap());
        let build = fixture
            .build(ContactConstructionLimits::production())
            .unwrap();
        let forest = build.forest();
        let standoff_mm = m0_constants()
            .unwrap()
            .placeholders()
            .standoff_mm()
            .diagnostic()
            .get();

        let mut requests = Vec::new();
        let mut keys = Vec::new();
        for key in forest.live_members() {
            let member = forest.member(key).expect("a live member resolves");
            if member.model_contact_end() != Some(ContactEnd::Terminalward) {
                continue;
            }
            let p0 = member.p0_mm().map(Finite::get);
            let p1 = member.p1_mm().map(Finite::get);
            // The seat is the surface point, one contact depth outboard of `p0`
            // along the axis — the same point the mint projected.
            let axis: [f64; 3] = std::array::from_fn(|i| p1[i] - p0[i]);
            let length = (0..3).map(|i| axis[i] * axis[i]).sum::<f64>().sqrt();
            let depth = m0_constants()
                .unwrap()
                .placeholders()
                .default_contact_depth_mm()
                .diagnostic()
                .get();
            let seat: [f64; 3] = std::array::from_fn(|i| p0[i] + axis[i] / length * depth);
            let (_, _, face_ids, _) = fixture.surface.project(&[seat]);
            requests.push(crate::seat_zone::SeatZoneRequest {
                p0_mm: p0,
                p1_mm: p1,
                seat_face_id: u32::try_from(face_ids[0]).expect("the seat projects onto a face"),
                band_radius_mm: member.terminal_radius_mm().get().get() + standoff_mm,
            });
            keys.push(key);
        }
        assert!(!keys.is_empty(), "NON-VACUITY: the fixture must have stubs");
        // The discriminating population, stated. `a_band_frozen_from_the_law_radius`
        // can only separate `law_r0` from `r0` on a stub that GREW; this
        // fixture grows none, so the sabotage is silent here BY MEASUREMENT.
        println!(
            "STUB-BAND-DISCRIMINATION: {} of {} stubs grew a tip; the \
             law-radius sabotage is silent unless this is non-zero",
            build.grown().len(),
            keys.len()
        );

        let expected = crate::seat_zone::freeze_production_seat_zones(
            &fixture.surface,
            &fixture.topology,
            &requests,
        )
        .expect("the reference freeze succeeds");
        for ((key, zone), request) in keys.iter().zip(expected).zip(&requests) {
            assert_eq!(
                forest.seat_zone(*key).map(|frozen| frozen.to_vec()),
                Some(zone.zone().to_vec()),
                "stub {key:?}'s frozen zone must be the one its EMITTED tip \
                 radius produces (band {} mm). A band frozen from the load law's \
                 request would describe a tip the run is not building.",
                request.band_radius_mm
            );
        }
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

    /// **#28 — evidence that cannot be graded stops the build; it does not
    /// quietly quarantine a PLACED stub.**
    ///
    /// `witness_batch` grades per row so a CANDIDATE tournament can discard one
    /// unmeasurable proposal and keep the rest. This family is the other kind
    /// of caller: its members are already in the forest, and the way it
    /// disposes of a refusal is to DELETE the member. So an ungradable row must
    /// reach the caller as a batch-level error. Left per-row, a corrupt kernel
    /// interval would delete a load-carrying stub on a measurement nobody made
    /// and let the run continue — the silent deletion the loud pre-#28 failure
    /// at least made visible.
    ///
    /// The evidence is injected because no fixture can make a real kernel emit
    /// an out-of-span boundary on demand; the values that actually occur are a
    /// few ULP out, i.e. inside what #28 admits.
    #[test]
    fn ungradable_evidence_on_a_placed_stub_refuses_the_build() {
        /// Far outside anything `MOLLER_EPS` could excuse — a whole millimetre
        /// of negative arc length, not the 1e-16 the kernel legitimately emits.
        const OUT_OF_SPAN_S_A: f64 = -1.0;

        let fixture = Fixture::new(make_overhang_table().unwrap());
        // Baseline: this fixture grades every row, so the refusal below is the
        // injected evidence and not the fixture.
        let clean = fixture
            .build(ContactConstructionLimits::production())
            .unwrap();
        assert!(!fixture.plan.contacts().is_empty());
        assert!(
            clean.unattested().iter().all(|stub| !matches!(
                stub.refusal,
                ContactAttestationRefusal::UnusableGeometry { .. }
            )),
            "no row is ungradable before anything is injected"
        );

        let sabotaged = crate::designed_embed::with_sabotaged_interval(0, OUT_OF_SPAN_S_A, || {
            fixture.build(ContactConstructionLimits::production())
        });
        let Err(InitialContactError::Attestation(cause)) = sabotaged else {
            panic!(
                "an ungradable PLACED stub must refuse the whole build, not be \
                 quarantined with the run carrying on: {sabotaged:?}"
            );
        };
        let DesignedMaskError::EndpointEvidenceOutsideAdmittedSpan { s_a, .. } = cause else {
            panic!("the refusal must name the span check, not {cause:?}");
        };
        assert_eq!(s_a, OUT_OF_SPAN_S_A);
        // There is no `InitialContactBuild` at all, so no forest carrying a
        // stub quarantined on evidence that was never graded ever exists.
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
                SpecializedLimits::production(
                    TEST_KNN_LIMITS,
                    crate::specialized::test_batch_work()
                ),
                &alien,
                ContactConstructionLimits::production(),
            ),
            Err(InitialContactError::Geometry(
                AnchorSearchError::TopologyMeshMismatch
            ))
        ));
        let count = table.plan.contacts().len();
        // FU-8g H1 re-pin: the preflight charges the census pass's counted
        // demand, so the exact/minus-one probe must derive `required` from the
        // same counted numbers. The stub batch below is the seed loop's own
        // recipe (p0 = position - unit x depth, p1 = position + unit x span,
        // r1 = max(r0, min_shoot)), reconstructed from the same policy reads.
        let policy = m0_constants().unwrap().placeholders();
        let depth = policy.default_contact_depth_mm().diagnostic().get();
        let span = policy.stub_length_mm().diagnostic().get();
        let min_shoot = policy.min_shoot_r_mm().diagnostic().get();
        let majority_step = policy.initial_axis_majority_step_mm().diagnostic().get();
        let interval_cap =
            usize::try_from(policy.initial_axis_majority_cap().diagnostic().get()).unwrap();
        let mut p0s = Vec::new();
        let mut p1s = Vec::new();
        let mut r0s = Vec::new();
        let mut r1s = Vec::new();
        for (slot, contact) in table.plan.contacts().iter().enumerate() {
            let normal = contact.normal();
            let length = normal.iter().map(|value| value * value).sum::<f64>().sqrt();
            let unit = normal.map(|value| value / length);
            let position = contact.position_mm();
            p0s.push(std::array::from_fn(|axis| {
                position[axis] - unit[axis] * depth
            }));
            p1s.push(std::array::from_fn(|axis| {
                position[axis] + unit[axis] * span
            }));
            let r0 = table.loads.tip_radii_mm()[slot].get();
            r0s.push(r0);
            r1s.push(r0.max(min_shoot));
        }
        let census = table
            .surface
            .penetration_census_batch(&p0s, &p1s, &r0s, &r1s, 8, false)
            .unwrap();
        let penetration = table
            .surface
            .penetration_intervals_batch_census_limits(
                &census,
                8,
                interval_cap,
                majority_step,
                false,
            )
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

    /// A stack of `count` thin closed boxes sharing one footprint. A ray down
    /// the column is tested against every triangle in the stack, so the
    /// counted traversal demand grows with the stack and NOT with any
    /// worst-case `rows x nfaces` fiction — which is exactly what makes it a
    /// witness that the honest charge can still exceed the unchanged cap.
    fn lamella_stack(count: usize) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        let mut vertices = Vec::with_capacity(count * 8);
        let mut faces = Vec::with_capacity(count * 12);
        for slab in 0..count {
            let z0 = slab as f64 * 0.02;
            let z1 = z0 + 0.01;
            let base = u32::try_from(vertices.len()).unwrap();
            for z in [z0, z1] {
                for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
                    vertices.push([x, y, z]);
                }
            }
            for corners in [
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
            ] {
                faces.push(corners.map(|corner| base + corner));
            }
        }
        (vertices, faces)
    }

    /// FU-8g H2 — the refusal path survives the repricing. The whole point of
    /// charging the counted demand instead of `rows x nfaces` is that the
    /// charge falls; the obligation that comes with it is that a mesh whose
    /// HONEST demand is genuinely too large is still refused, at caps nobody
    /// touched. `ContactConstructionLimits::production()` is used verbatim
    /// here — 4,000,000 work items, byte-identical to `develop` — and the
    /// stack below overruns it on counted traversal tests alone.
    ///
    /// Measured at the tip that landed it (100 rows over an 8,000-slab,
    /// 96,000-face stack): counted work 4,223,900 against the 4,000,000 cap,
    /// counted hits 1,600,000, counted scratch 356,484,600 B against the
    /// 536,870,912 B cap — so it is the WORK charge that refuses, on counted
    /// traversal, with the byte charge still inside its own cap. The
    /// worst-case shape for the same batch would have said 9,600,000: the
    /// honest number is 2.3x smaller and still over.
    #[test]
    fn honest_over_demand_still_refuses_at_the_unchanged_production_caps() {
        let production = ContactConstructionLimits::production();
        // Pinned in the assertion, not read from a variable, so a future cap
        // move has to come here and change this literal.
        assert_eq!(production.max_work_items, 4_000_000);
        assert_eq!(production.max_scratch_bytes, 512 * 1024 * 1024);

        let (vertices, faces) = lamella_stack(8_000);
        let surface = NativeSurface::from_indexed(
            &vertices,
            &faces,
            NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        // 42 stub-shaped rows, each running the whole column: the seed loop's
        // own batch shape (`n_offset_rays = 8`, `rays_all = false`).
        let rows = 100;
        let top = 8_000.0 * 0.02 + 1.0;
        let p0s: Vec<[f64; 3]> = (0..rows)
            .map(|row| [0.05 + 0.008 * row as f64, 0.5, -1.0])
            .collect();
        let p1s: Vec<[f64; 3]> = p0s.iter().map(|p0| [p0[0], p0[1], top]).collect();
        let r0s = vec![0.05; rows];
        let r1s = vec![0.05; rows];
        let policy = m0_constants().unwrap().placeholders();
        let interval_cap =
            usize::try_from(policy.initial_axis_majority_cap().diagnostic().get()).unwrap();
        let majority_step = policy.initial_axis_majority_step_mm().diagnostic().get();
        let census = surface
            .penetration_census_batch(&p0s, &p1s, &r0s, &r1s, 8, false)
            .unwrap();
        let penetration = surface
            .penetration_intervals_batch_census_limits(
                &census,
                8,
                interval_cap,
                majority_step,
                false,
            )
            .unwrap();
        // The demand that refuses is the COUNTED one, not a worst-case shape:
        // it is under `rows x nfaces` and still over the cap.
        assert!(penetration.max_total_work > production.max_work_items);
        assert!(penetration.max_total_work < rows * faces.len());
        let required = rows * 4 + penetration.max_total_work;
        assert!(matches!(
            preflight_work(rows, penetration, production),
            Err(InitialContactError::Limit {
                resource: "work items",
                required: charged,
                limit: 4_000_000,
            }) if charged == required
        ));
    }

    /// FU-8g H2 — the receipt is checked, resource by resource. Deleting any
    /// row from `charge_covers_execution`'s table fails this test.
    #[test]
    fn the_meter_refuses_execution_that_outran_its_charge() {
        let charged = PenetrationBatchLimits {
            max_rows: 4,
            max_total_work: 100,
            max_total_hits: 10,
            max_total_samples: 50,
            max_total_intervals: 5,
            max_scratch_bytes: 4_096,
        };
        let exact = PenetrationBatchMeter {
            rows: 4,
            work: 100,
            hits: 10,
            samples: 50,
            intervals: 5,
        };
        // Executing exactly the charge is admitted: the charge is a boundary,
        // not a margin.
        assert_eq!(charge_covers_execution(exact, charged), Ok(()));
        for (resource, over) in [
            ("rows", PenetrationBatchMeter { rows: 5, ..exact }),
            ("work", PenetrationBatchMeter { work: 101, ..exact }),
            ("hits", PenetrationBatchMeter { hits: 11, ..exact }),
            (
                "samples",
                PenetrationBatchMeter {
                    samples: 51,
                    ..exact
                },
            ),
            (
                "intervals",
                PenetrationBatchMeter {
                    intervals: 6,
                    ..exact
                },
            ),
        ] {
            assert!(
                matches!(
                    charge_covers_execution(over, charged),
                    Err(InitialContactError::ChargeDrift {
                        resource: fired,
                        ..
                    }) if fired == resource
                ),
                "{resource} drift was not refused"
            );
        }
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
                    seat_mm: record.p0_mm.map(Finite::get),
                    p0: record.p0_mm,
                    p1: record.p1_mm,
                    law_r0: record.r0_mm.get(),
                    r0: record.r0_mm.get(),
                    r1: record.r1_mm.get(),
                    carried_mm2: record.carried_mm2,
                    specialized: false,
                    exterior_probe: record.p0_mm.map(Finite::get),
                    tip_clamped_at_max: false,
                }
            })
            .collect::<Vec<_>>();
        let (boundary_overlaps, batch_count, _meter) = classify_initial_overlaps(
            &boundary_seeds,
            &axis_batch_input(&boundary_seeds).unwrap(),
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

    /// **A refusal growth could not cure is DISCLOSED, not silently
    /// quarantined — and it is still quarantined.**
    ///
    /// `make_raking_blade` is the fixture for this: five of its forty contacts
    /// seat on a razor edge whose solid no legal tip can reach, so growth walks
    /// their whole ladder to the registered maximum and refuses. That is the
    /// correct answer — curing them would be inventing attachment — and the
    /// point of this test is what the engine must SAY while giving it.
    ///
    /// Before this change the answer was silence: the stub was quarantined, the
    /// area it carried stopped being carried, no neighbour was told, nothing was
    /// grown in its place, and coverage's 6 mm radius cannot see a hole a claw
    /// tip leaves. One contact on this fixture is charged 10.15 mm² and it went
    /// unmentioned.
    ///
    /// The end-to-end CURE — a contact that used to be quarantined and now
    /// attests at a grown tip — is pinned on the toed slab in
    /// `plinth-support/tests/support_runner_integration.rs`, where routed
    /// contacts move 64 -> 65 and live members 170 -> 184. It cannot be pinned
    /// here because this file's `Fixture` runs a zero delicacy field and lands a
    /// different contact set; the mechanism itself is pinned on a constructed
    /// edge seat in `designed_embed.rs`.
    #[test]
    fn an_incurable_refusal_is_quarantined_with_its_seat_load_and_every_remedy_disclosed() {
        let constants = m0_constants().unwrap();
        let policy = constants.placeholders();
        // The ceiling a rescue may walk to: the SMALLER of the load law's own
        // clamp maximum and Craig's registered maximum on a grown scar. Derived
        // from the registry rather than typed, so a ruling that moves either
        // limit moves this assertion with it.
        let (max_radius_mm, ceiling_source) = crate::designed_embed::growth_ceiling_radius_mm(
            policy.tip_radius_max_legacy_mm().diagnostic().get(),
            *constants.evidence().tip_radius_max_grown_scar_mm().value(),
        );
        let build = Fixture::new(make_raking_blade().unwrap())
            .build(ContactConstructionLimits::production())
            .unwrap();

        let unreachable = build
            .unattested()
            .iter()
            .filter(|stub| matches!(stub.refusal, ContactAttestationRefusal::NotAttached { .. }))
            .collect::<Vec<_>>();
        assert!(
            !unreachable.is_empty(),
            "make_raking_blade is the fixture that carries attachment refusals \
             no legal tip can cure; if none survive here this test is asserting \
             about an empty list"
        );
        // Nothing was cured, so nothing may be recorded as grown. The two lists
        // are complements on this fixture, and a row in both would mean the
        // engine claimed to have both saved and abandoned the same contact.
        assert!(build.grown().is_empty());

        let mut abandoned_mm2 = 0.0;
        let mut walked = 0_usize;
        let mut already_at_or_above_ceiling = 0_usize;
        for stub in &unreachable {
            assert!(stub.remedy.law_radius_mm > 0.0);
            assert_eq!(stub.remedy.growth_ceiling_radius_mm, max_radius_mm);
            if stub.remedy.law_radius_mm < max_radius_mm {
                // THE LADDER WAS WALKED, to the ceiling, before the refusal was
                // allowed to stand. A refusal with an unwalked ladder is the
                // silent quarantine this change exists to remove.
                walked += 1;
                assert!(
                    !stub.remedy.growth.is_empty(),
                    "terminal {} was refused without any larger tip being tried",
                    stub.terminal
                );
                assert_eq!(
                    stub.remedy.growth.last().unwrap().radius_mm,
                    max_radius_mm,
                    "the ladder must be walked to {max_radius_mm} mm radius \
                     ({} mm diameter) before a refusal stands",
                    max_radius_mm * 2.0
                );
                assert!(stub
                    .remedy
                    .growth
                    .iter()
                    .all(|rung| rung.radius_mm <= max_radius_mm));
            } else {
                // THE OTHER HONEST OUTCOME, and it is a class rather than an
                // accident: the LOAD LAW already sized this tip at or above the
                // ceiling a rescue may grow to. There is no larger tip growth is
                // allowed to propose, so the ladder is EMPTY and says so — a
                // rung above the ceiling would be a scar the run may not build.
                // The tip itself is untouched: `law_radius_mm` is what the load
                // law asked for and nothing shrank it.
                already_at_or_above_ceiling += 1;
                assert!(
                    stub.remedy.growth.is_empty(),
                    "terminal {} was already sized at {} mm radius, at or above \
                     the {max_radius_mm} mm radius ceiling, so no larger tip is \
                     legal to propose: {:?}",
                    stub.terminal,
                    stub.remedy.law_radius_mm,
                    stub.remedy.growth
                );
            }
            assert!(
                stub.remedy.growth.iter().all(|rung| !rung.attested),
                "terminal {} is recorded refused but a rung attested",
                stub.terminal
            );
            // The ceiling is NAMED, so the ABANDONED LOAD line can say which
            // rule stopped the rescue rather than quoting a bare number.
            assert_eq!(
                stub.remedy.growth_ceiling_constant,
                ceiling_source.constant_id()
            );
            assert_eq!(
                stub.remedy.growth_ceiling_constant, "TIP_RADIUS_MAX_GROWN_SCAR_MM",
                "these rescues stop at Craig's registered maximum on a grown \
                 scar, 0.400 mm radius (0.800 mm diameter) — not at the load \
                 law's own clamp"
            );
            // Re-seating is recorded as UNAVAILABLE rather than left unsaid, so
            // the disclosure can never imply a remedy was skipped.
            assert_eq!(
                stub.remedy.reseat,
                ReseatAvailability::NoAlternativeSeatAtThisStage
            );
            // THE SEAT AND THE LOAD. Both are what makes this a disclosure
            // rather than a count.
            assert!(stub.seat_mm.iter().all(|value| value.is_finite()));
            assert!(stub.abandoned_mm2 >= 0.0);
            abandoned_mm2 += stub.abandoned_mm2;
            // ITEM 2 — quarantine is PRESERVED for a seat that is genuinely
            // condemned. Grow-to-attest removes the swallowing, not the
            // quarantine.
            assert_eq!(
                build
                    .forest()
                    .terminal_ledger(stub.terminal)
                    .expect("registered terminal")
                    .state,
                TerminalLedgerState::Quarantined,
                "terminal {} carrying {} mm2 is refused and must still be \
                 quarantined",
                stub.terminal,
                stub.abandoned_mm2
            );
        }
        // Non-vacuity on the SHAPE of the evidence: this fixture must still
        // contain a genuinely walked ladder, or the paragraph above about
        // walking one is asserting over an empty set.
        assert!(
            walked > 0,
            "make_raking_blade must still contain at least one refusal whose \
             ladder was actually walked; got {walked} walked and \
             {already_at_or_above_ceiling} already at or above the ceiling"
        );
        // Non-vacuity on the number that matters: this fixture really does
        // abandon load, so the disclosure is reporting something.
        assert!(
            abandoned_mm2 > 10.0,
            "make_raking_blade abandons about 10.15 mm2 across its refused \
             contacts; got {abandoned_mm2} mm2, which means either the fixture \
             or the load partition moved and the disclosure is describing \
             something else"
        );
    }

    /// **The growth ladder's shape, asserted against the registered clamps
    /// rather than against a hand-typed list.**
    ///
    /// The step is one AREA DOUBLING — `pi (sqrt2 r)^2 = 2 pi r^2` — because
    /// area is the tip's registered duty, so it is the step that changes the
    /// answer the load law gives. The rung COUNT is therefore not chosen: it is
    /// `ceil(log(max/law)/log(sqrt2))`, fixed by the row's own law radius and
    /// the registered `TIP_RADIUS_MAX_LEGACY_MM`. This test states that
    /// relationship as arithmetic so a hand-edited ladder cannot pass.
    #[test]
    fn the_growth_ladder_doubles_the_interface_area_and_stops_at_the_registered_maximum() {
        let policy = m0_constants().unwrap().placeholders();
        let min_radius_mm = policy.tip_radius_default_mm().diagnostic().get();
        let max_radius_mm = policy.tip_radius_max_legacy_mm().diagnostic().get();

        // The deepest ladder the registered clamp span admits: from the
        // smallest legal tip to the largest.
        let rungs = crate::designed_embed::tip_growth_ladder_for_test(min_radius_mm, max_radius_mm);
        let expected_count = (max_radius_mm / min_radius_mm).ln() / 2.0_f64.sqrt().ln();
        assert_eq!(
            rungs.len(),
            expected_count.ceil() as usize,
            "the rung count is ceil(log(max/law)/log(sqrt2)) = ceil({expected_count}), \
             derived from TIP_RADIUS_DEFAULT_MM = {min_radius_mm} mm radius and \
             TIP_RADIUS_MAX_LEGACY_MM = {max_radius_mm} mm radius, never hand-typed"
        );
        assert_eq!(
            *rungs.last().unwrap(),
            max_radius_mm,
            "the ladder must END on the registered maximum, so the sweep always \
             stops at a legal radius"
        );
        // Strictly ascending, and every rung but the last exactly doubles the
        // nominal interface area of the one before it.
        let mut previous = min_radius_mm;
        for (index, radius) in rungs.iter().enumerate() {
            assert!(
                *radius > previous,
                "rung {index} at {radius} mm radius does not exceed {previous} mm"
            );
            if index + 1 < rungs.len() {
                let area_ratio = (radius * radius) / (previous * previous);
                assert!(
                    (area_ratio - 2.0).abs() < 1e-9,
                    "rung {index} must DOUBLE the nominal disc area; got {area_ratio}x"
                );
            }
            previous = *radius;
        }
        // A tip the load law already sized at the registered maximum has no
        // larger legal tip to try, and the ladder says so with no rungs rather
        // than by proposing an illegal one.
        assert!(
            crate::designed_embed::tip_growth_ladder_for_test(max_radius_mm, max_radius_mm)
                .is_empty()
        );
    }
}
