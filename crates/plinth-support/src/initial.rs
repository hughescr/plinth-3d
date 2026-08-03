//! Initial contact stubs are constructed once, then either remain terminal
//! seeds or are quarantined.  This leaf deliberately does not route or grow.

use std::cmp::Ordering;

use geom_kernel::{Interval, PenetrationBatchLimits};
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_mesh::Mesh;
use thiserror::Error;

use crate::{
    anchor::{AnchorGeometryAuthority, AnchorSearchError},
    dyadic::CandidatePayloadConfig,
    forest::{
        modeled_initial_contact_bytes, EmbedEnd, Forest, ForestError, Member, MemberKey,
        MemberKind, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius, TerminalLedgerState,
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
pub(crate) struct ContactConstructionLimits {
    pub(crate) max_contacts: usize,
    pub(crate) max_load_rows: usize,
    pub(crate) max_work_items: usize,
    pub(crate) max_scratch_bytes: usize,
}

impl ContactConstructionLimits {
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
}

struct StubSeed {
    terminal: u32,
    p0: [Finite; 3],
    p1: [Finite; 3],
    r0: PositiveFinite,
    r1: PositiveFinite,
    carried_mm2: Finite,
    specialized: bool,
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
        let depth = policy.default_contact_depth_mm().diagnostic().get();
        let span = policy.stub_length_mm().diagnostic().get();
        let p0 = std::array::from_fn(|axis| position[axis] - unit[axis] * depth);
        let p1 = std::array::from_fn(|axis| position[axis] + unit[axis] * span);
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
        });
    }

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
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: seed.carried_mm2,
        })));
    }
    for (seed, key) in seeds.iter().zip(&keys) {
        forest.register_terminal(seed.terminal, *key)?;
    }
    debug_assert!(seeds
        .iter()
        .all(|seed| forest.terminal(seed.terminal).is_some()));
    let cutoff = policy.min_illegal_len_mm().diagnostic().get();
    for ((seed, key), overlap) in seeds.iter().zip(&keys).zip(&overlaps) {
        if initial_overlap_exceeds_cutoff(*overlap, cutoff, distance(seed.p0, seed.p1)) {
            forest.quarantine(*key)?;
            condemned.push(CondemnedInitialStub {
                terminal: seed.terminal,
                key: *key,
                remaining_inside_axis_mm: *overlap,
            });
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
        interval_batch_count,
        queried_stubs: contact_count,
        scratch_bytes,
        all_terminals_registered_before_quarantine: true,
    })
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

    use geom_kernel::{NativeSurface, NativeSurfaceConfig};
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
                GenericPerceptionLimits::production(),
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
                GenericPerceptionLimits::production(),
                &generic,
                &regular,
                &selection,
                SpecializedLimits::production(),
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
                GenericPerceptionLimits::production(),
                &self.generic,
                &self.regular,
                &selection,
                SpecializedLimits::production(),
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
            GenericPerceptionLimits::production(),
            &fixture.generic,
            &fixture.regular,
            &selection,
            SpecializedLimits::production(),
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
            GenericPerceptionLimits::production(),
            &fixture.generic,
            &fixture.regular,
            &selection,
            SpecializedLimits::production(),
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
            assert_eq!(record.embed_end, Some(EmbedEnd::Terminalward));
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
                GenericPerceptionLimits::production(),
                &table.generic,
                &table.regular,
                &selection,
                SpecializedLimits::production(),
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
            assert_eq!(
                build
                    .terminals()
                    .iter()
                    .filter(|terminal| terminal.state == TerminalLedgerState::Quarantined)
                    .count(),
                build.condemned().len()
            );
        }
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
