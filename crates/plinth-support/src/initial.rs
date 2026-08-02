//! Initial contact stubs are constructed once, then either remain terminal
//! seeds or are quarantined.  This leaf deliberately does not route or grow.

use std::cmp::Ordering;

use geom_kernel::NativeSurface;
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_mesh::Mesh;
use thiserror::Error;

use crate::{
    dyadic::CandidatePayloadConfig,
    forest::{
        EmbedEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec, Owner,
        RootwardEndRadius, TerminalEndRadius, TerminalLedgerState,
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
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InitialContactBuild {
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
}

impl InitialContactBuild {
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
}

#[derive(Clone, Debug, Error, PartialEq)]
pub(crate) enum InitialContactError {
    #[error(transparent)]
    Specialized(#[from] SpecializedError),
    #[error(transparent)]
    Loads(#[from] GenericPerceptionError),
    #[error(transparent)]
    Forest(#[from] ForestError),
    #[error("M0 constants are unavailable: {0}")]
    Constants(String),
    #[error("contact {contact_id} cannot be represented by the terminal namespace")]
    ContactIdOutOfRange { contact_id: u64 },
    #[error("physical contact id {contact_id} occurs more than once")]
    DuplicatePhysicalContact { contact_id: u64 },
    #[error("physical contact {contact_id} is already registered in the forest")]
    ExistingTerminal { contact_id: u32 },
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
    surface: &NativeSurface,
    limits: ContactConstructionLimits,
    forest: &mut Forest,
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
    let contacts = plan.contacts();
    let policy = m0_constants()
        .map_err(|error| InitialContactError::Constants(error.to_string()))?
        .placeholders();
    let contact_count = contacts.len();
    preflight(contact_count, plan.load_partition().rows().len(), limits)?;
    let scratch_bytes = required_scratch_bytes(contact_count)?;
    if scratch_bytes > limits.max_scratch_bytes {
        return Err(InitialContactError::Limit {
            resource: "scratch bytes",
            required: scratch_bytes,
            limit: limits.max_scratch_bytes,
        });
    }
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

    let initial_forest_digest = forest.digest();
    let native_digest = native_digest(surface);
    let mut seeds = Vec::new();
    seeds
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("stub seeds"))?;
    for (slot, contact) in contacts.iter().enumerate() {
        let contact_id = contact.contact_id();
        let terminal = u32::try_from(contact_id)
            .map_err(|_| InitialContactError::ContactIdOutOfRange { contact_id })?;
        if forest.terminal_key(terminal).is_some() {
            return Err(InitialContactError::ExistingTerminal {
                contact_id: terminal,
            });
        }
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
    let axis_batch = axis_batch_input(&seeds)?;
    let mut terminals = Vec::new();
    terminals
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("terminal evidence"))?;

    let mut keys = Vec::new();
    keys.try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("terminal keys"))?;
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

    let mut condemned = Vec::new();
    condemned
        .try_reserve_exact(contact_count)
        .map_err(|_| InitialContactError::Allocation("quarantine evidence"))?;
    let mut interval_batch_count = 0;
    if !seeds.is_empty() {
        interval_batch_count = 1;
        let intervals = surface
            .penetration_intervals_batch(
                &axis_batch.p0,
                &axis_batch.p1,
                &axis_batch.r0,
                &axis_batch.r1,
                8,
                policy.initial_axis_majority_step_mm().diagnostic().get(),
                usize::try_from(policy.initial_axis_majority_cap().diagnostic().get())
                    .map_err(|_| InitialContactError::ResourceOverflow("majority cap"))?,
                0.0,
                false,
            )
            .map_err(|error| InitialContactError::NativeQuery(error.to_string()))?;
        let depth = policy.default_contact_depth_mm().diagnostic().get();
        let cutoff = policy.min_illegal_len_mm().diagnostic().get();
        for ((seed, key), rows) in seeds.iter().zip(&keys).zip(intervals) {
            let length = distance(seed.p0, seed.p1);
            let overlap = remaining_illegal_axis_overlap(&rows, length, depth);
            if overlap > cutoff {
                forest.quarantine(*key)?;
                condemned.push(CondemnedInitialStub {
                    terminal: seed.terminal,
                    key: *key,
                    remaining_inside_axis_mm: overlap,
                });
            }
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
    Ok(InitialContactBuild {
        plan_digest: plan.digest(),
        load_digest: loads.digest(),
        policy_digest: policy_digest(policy),
        native_digest,
        initial_forest_digest,
        post_quarantine_forest_digest: forest.digest(),
        routing_order,
        terminals,
        condemned,
        interval_batch_count,
        queried_stubs: contact_count,
        scratch_bytes,
    })
}

fn preflight(
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
    let work = contact_count
        .checked_mul(4)
        .ok_or(InitialContactError::ResourceOverflow("work items"))?;
    if work > limits.max_work_items {
        return Err(InitialContactError::Limit {
            resource: "work items",
            required: work,
            limit: limits.max_work_items,
        });
    }
    Ok(())
}

fn required_scratch_bytes(contacts: usize) -> Result<usize, InitialContactError> {
    contacts
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
        .ok_or(InitialContactError::ResourceOverflow("scratch bytes"))
}

fn finite(values: [f64; 3], contact_id: u64) -> Result<[Finite; 3], InitialContactError> {
    values
        .map(Finite::new)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .and_then(|values| {
            values
                .try_into()
                .map_err(|_| plinth_core::FiniteError::NonFinite)
        })
        .map_err(|_| InitialContactError::NonFiniteStub { contact_id })
}

fn distance(a: [Finite; 3], b: [Finite; 3]) -> f64 {
    a.into_iter()
        .zip(b)
        .map(|(a, b)| (a.get() - b.get()).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn remaining_illegal_axis_overlap(
    intervals: &[geom_kernel::Interval],
    length: f64,
    terminalward_embed_depth: f64,
) -> f64 {
    intervals
        .iter()
        .map(|row| row.s_b.min(length) - row.s_a.max(terminalward_embed_depth))
        .fold(0.0_f64, f64::max)
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

fn native_digest(surface: &NativeSurface) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut write = |bytes: &[u8]| {
        for &byte in bytes {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for bound in surface.bounds() {
        for value in bound {
            write(&value.to_bits().to_le_bytes());
        }
    }
    for face in surface.face_ids() {
        write(&face.to_le_bytes());
    }
    for normal in surface.geometry().normals() {
        for value in normal {
            write(&value.to_bits().to_le_bytes());
        }
    }
    for area in surface.geometry().areas() {
        write(&area.to_bits().to_le_bytes());
    }
    hash
}

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn terminalward_embed_masks_only_the_exact_contact_depth_and_keeps_boundary_clean() {
        let row = geom_kernel::Interval {
            s_a: 0.3,
            s_b: 0.35,
            entered: true,
            exited: true,
            max_depth: 0.1,
            ray: -1,
        };
        assert!(remaining_illegal_axis_overlap(std::slice::from_ref(&row), 2.8, 0.3) <= 0.05);
        let buried = geom_kernel::Interval {
            s_a: 0.0,
            s_b: 0.3,
            ..row
        };
        assert_eq!(remaining_illegal_axis_overlap(&[buried], 2.8, 0.3), 0.0);
    }

    #[test]
    fn scratch_preflight_rejects_overflow() {
        assert_eq!(
            required_scratch_bytes(usize::MAX),
            Err(InitialContactError::ResourceOverflow("scratch bytes"))
        );
    }
}
