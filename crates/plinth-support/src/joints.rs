//! Frozen pre-P8 joint-ball placement and exact enclosure certificates.
//!
//! The implementation is deliberately conservative: a ball is removed only
//! when every spherical patch is certified covered by a frustum whose solid
//! contains the ball centre.  Any numerical, depth, work, or memory limit
//! leaves the ball in the result.

use geom_kernel::vec3::{add, dot, norm, normalize, scale, sub};
use plinth_core::{Finite, PositiveFinite};
use thiserror::Error;

use crate::forest::{
    ContactEnd, Forest, Member, MemberKey, MemberKind, Owner, TerminalLedgerState,
};

const MODEL_FOOT_FACTOR: f64 = 1.4;
const COINCIDE_TOL_MM: f64 = 0.15;
const ON_AXIS_TOL_MM: f64 = 0.2;
const PLATE_BAND_MM: f64 = 0.2;
const COLINEAR_RADIUS_FRAC: f64 = 0.20;
const COLINEAR_COS_MAX: f64 = -0.984_807_753_012_208;
const ENCLOSURE_PROOF_TOL_MM: f64 = 0.018;
const DEGENERATE_LENGTH_SQUARED: f64 = 1e-18;

/// Bounded current-parity policy.  The defaults are the frozen Python limits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointConfig {
    plate_z_mm: Finite,
    joint_ball_factor: PositiveFinite,
    max_depth: u8,
    max_patches: usize,
    max_work_items: usize,
    max_scratch_bytes: usize,
    max_certificate_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JointStagingLimits {
    max_members: usize,
    max_nodes: usize,
    max_neighbor_checks: usize,
    max_neighbor_pairs: usize,
    max_clusters: usize,
    max_axis_members: usize,
    max_bytes: usize,
}

impl JointStagingLimits {
    pub(crate) const fn new(
        max_members: usize,
        max_nodes: usize,
        max_neighbor_checks: usize,
        max_neighbor_pairs: usize,
        max_clusters: usize,
        max_axis_members: usize,
        max_bytes: usize,
    ) -> Self {
        Self {
            max_members,
            max_nodes,
            max_neighbor_checks,
            max_neighbor_pairs,
            max_clusters,
            max_axis_members,
            max_bytes,
        }
    }

    pub(crate) fn from_joint_config(config: JointConfig) -> Self {
        Self::new(
            config.max_work_items,
            config.max_work_items,
            config.max_work_items,
            config.max_work_items,
            config.max_work_items,
            config.max_work_items,
            config.max_scratch_bytes,
        )
    }
}

/// Derive all runner-facing staging limits from one already-validated joint
/// configuration.  The runner must not duplicate the private current-parity
/// resource values merely to construct the one shared live view.
pub(crate) fn runner_planning_limits(config: JointConfig) -> (LiveViewLimits, JointStagingLimits) {
    (
        LiveViewLimits::new(
            config.max_work_items,
            config.max_work_items,
            config.max_scratch_bytes,
        ),
        JointStagingLimits::from_joint_config(config),
    )
}

impl JointConfig {
    pub fn current(plate_z_mm: Finite, joint_ball_factor: PositiveFinite) -> Self {
        Self {
            plate_z_mm,
            joint_ball_factor,
            max_depth: 22,
            max_patches: 2_000_000,
            max_work_items: 50_000_000,
            max_scratch_bytes: 512 * 1024 * 1024,
            max_certificate_bytes: 256 * 1024 * 1024,
        }
    }

    #[cfg(test)]
    fn with_limits(mut self, max_depth: u8, max_patches: usize) -> Self {
        self.max_depth = max_depth;
        self.max_patches = max_patches;
        self
    }

    fn fingerprint(self) -> ConfigFingerprint {
        ConfigFingerprint {
            plate_z: self.plate_z_mm.get().to_bits(),
            factor: self.joint_ball_factor.get().to_bits(),
            max_depth: self.max_depth,
            max_patches: self.max_patches,
            max_work_items: self.max_work_items,
            max_scratch_bytes: self.max_scratch_bytes,
            max_certificate_bytes: self.max_certificate_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConfigFingerprint {
    plate_z: u64,
    factor: u64,
    max_depth: u8,
    max_patches: usize,
    max_work_items: usize,
    max_scratch_bytes: usize,
    max_certificate_bytes: usize,
}

/// Caller-owned preflight bounds for the one filtered live-member view shared by joint
/// planning, primitive lowering, and raft planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LiveViewLimits {
    max_members: usize,
    max_unrouted_terminals: usize,
    max_bytes: usize,
}

impl LiveViewLimits {
    pub(crate) const fn new(
        max_members: usize,
        max_unrouted_terminals: usize,
        max_bytes: usize,
    ) -> Self {
        Self {
            max_members,
            max_unrouted_terminals,
            max_bytes,
        }
    }
}

/// Opaque filtered view matching Python `live_member_lists`: creation order is retained while
/// sealed retired/quarantined terminal provenance proves every unrouted contact is non-live.
#[derive(Clone, Debug)]
pub(crate) struct FilteredLiveView {
    forest_digest: u64,
    keys: Vec<MemberKey>,
    digest: u64,
}

impl FilteredLiveView {
    pub(crate) fn current(
        forest: &Forest,
        unrouted_terminals: &[u32],
        limits: LiveViewLimits,
    ) -> Result<Self, JointError> {
        if limits.max_members == 0 || limits.max_bytes == 0 {
            return Err(JointError::InvalidLiveViewLimits);
        }
        let live_count = forest.live_members_in_creation_order_iter().count();
        if live_count > limits.max_members
            || unrouted_terminals.len() > limits.max_unrouted_terminals
        {
            return Err(JointError::LiveViewMemberLimit);
        }
        let bytes = live_count
            .checked_mul(std::mem::size_of::<MemberKey>())
            .and_then(|bytes| {
                unrouted_terminals
                    .len()
                    .checked_mul(std::mem::size_of::<u32>() + std::mem::size_of::<MemberKey>())
                    .and_then(|unrouted_bytes| bytes.checked_add(unrouted_bytes))
            })
            .ok_or(JointError::LiveViewBytesLimit)?;
        if bytes > limits.max_bytes {
            return Err(JointError::LiveViewBytesLimit);
        }

        let mut terminal_ids = unrouted_terminals.to_vec();
        terminal_ids.sort_unstable();
        if terminal_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(JointError::DuplicateUnroutedTerminal);
        }
        for entry in forest.terminal_ledger_iter() {
            let evidence = forest
                .terminal_ledger_evidence(entry.contact)
                .ok_or(JointError::InvalidUnroutedTerminal(entry.contact))?;
            if evidence.original_key != entry.original_key
                || evidence.kind != MemberKind::Stub
                || evidence.owner != Owner::Contact(entry.contact)
                || evidence.terminal_contacts.as_slice() != [entry.contact]
            {
                return Err(JointError::InvalidUnroutedTerminal(entry.contact));
            }
            match entry.state {
                TerminalLedgerState::Live => {
                    let member = forest
                        .member(entry.original_key)
                        .ok_or(JointError::InvalidUnroutedTerminal(entry.contact))?;
                    if !crate::forest::terminal_member_matches_evidence(
                        member,
                        entry.original_key,
                        entry.contact,
                        evidence,
                    ) {
                        return Err(JointError::InvalidUnroutedTerminal(entry.contact));
                    }
                }
                TerminalLedgerState::Consumed => {
                    let member = forest
                        .member(entry.original_key)
                        .ok_or(JointError::InvalidUnroutedTerminal(entry.contact))?;
                    if member.kind() != &MemberKind::Stub
                        || member.owner() != &Owner::Contact(entry.contact)
                    {
                        return Err(JointError::InvalidUnroutedTerminal(entry.contact));
                    }
                }
                TerminalLedgerState::Quarantined | TerminalLedgerState::Retired => {}
            }
            if !matches!(
                entry.state,
                TerminalLedgerState::Live | TerminalLedgerState::Consumed
            ) && terminal_ids.binary_search(&entry.contact).is_err()
            {
                return Err(JointError::MissingUnroutedTerminalProvenance(entry.contact));
            }
        }
        let mut excluded = Vec::with_capacity(terminal_ids.len());
        for terminal in terminal_ids {
            let entry = forest
                .terminal_ledger(terminal)
                .ok_or(JointError::InvalidUnroutedTerminal(terminal))?;
            match entry.state {
                TerminalLedgerState::Live => {
                    let member = forest
                        .member(entry.original_key)
                        .ok_or(JointError::InvalidUnroutedTerminal(terminal))?;
                    let evidence = forest
                        .terminal_ledger_evidence(terminal)
                        .ok_or(JointError::InvalidUnroutedTerminal(terminal))?;
                    if !crate::forest::terminal_member_matches_evidence(
                        member,
                        entry.original_key,
                        terminal,
                        evidence,
                    ) {
                        return Err(JointError::InvalidUnroutedTerminal(terminal));
                    }
                    // A caller cannot turn a still-live routed contact into an unrouted one by
                    // assertion alone. Failed terminals must first become lifecycle evidence.
                    return Err(JointError::InvalidUnroutedTerminal(terminal));
                }
                TerminalLedgerState::Consumed => {
                    return Err(JointError::InvalidUnroutedTerminal(terminal));
                }
                TerminalLedgerState::Quarantined => {
                    let record = forest
                        .quarantine_record(entry.original_key)
                        .ok_or(JointError::InvalidUnroutedTerminal(terminal))?;
                    let evidence = forest
                        .terminal_ledger_evidence(terminal)
                        .ok_or(JointError::InvalidUnroutedTerminal(terminal))?;
                    if record != evidence {
                        return Err(JointError::InvalidUnroutedTerminal(terminal));
                    }
                }
                TerminalLedgerState::Retired => {
                    let evidence = forest
                        .terminal_ledger_evidence(terminal)
                        .ok_or(JointError::InvalidUnroutedTerminal(terminal))?;
                    if evidence.kind != MemberKind::Stub
                        || evidence.owner != Owner::Contact(terminal)
                    {
                        return Err(JointError::InvalidUnroutedTerminal(terminal));
                    }
                }
            }
        }
        excluded.sort_unstable();
        let keys = forest
            .live_members_in_creation_order_iter()
            .filter(|key| excluded.binary_search(key).is_err())
            .collect::<Vec<_>>();
        let forest_digest = forest.digest();
        let digest = live_view_digest(forest_digest, &keys);
        Ok(Self {
            forest_digest,
            keys,
            digest,
        })
    }

    pub(crate) fn validate(&self, forest: &Forest) -> Result<(), JointError> {
        if self.forest_digest != forest.digest()
            || self.digest != live_view_digest(self.forest_digest, &self.keys)
            || self.keys.iter().any(|key| forest.member(*key).is_none())
        {
            return Err(JointError::StaleAuthority);
        }
        Ok(())
    }

    pub(crate) fn keys(&self) -> &[MemberKey] {
        &self.keys
    }

    pub(crate) const fn digest(&self) -> u64 {
        self.digest
    }

    #[cfg(test)]
    pub(crate) fn test_view_with_keys(forest: &Forest, keys: Vec<MemberKey>) -> Self {
        let forest_digest = forest.digest();
        let digest = live_view_digest(forest_digest, &keys);
        Self {
            forest_digest,
            keys,
            digest,
        }
    }
}

fn live_view_digest(forest_digest: u64, keys: &[MemberKey]) -> u64 {
    live_view_digest_iter(forest_digest, keys.len(), keys.iter().copied())
}

fn live_view_digest_iter(
    forest_digest: u64,
    count: usize,
    keys: impl Iterator<Item = MemberKey>,
) -> u64 {
    let mut state = 0xcbf2_9ce4_8422_2325_u64;
    for byte in forest_digest
        .to_le_bytes()
        .into_iter()
        .chain((count as u64).to_le_bytes())
        .chain(keys.flat_map(|key| {
            key.slot()
                .to_le_bytes()
                .into_iter()
                .chain(key.generation().to_le_bytes())
        }))
    {
        state ^= u64::from(byte);
        state = state.wrapping_mul(0x0000_0100_0000_01b3);
    }
    state
}

/// A retained ball.  There is no public constructor: output geometry remains
/// inseparable from the proof-bound plan which authorized it.
#[derive(Clone, Debug, PartialEq)]
pub struct JointBall {
    center_mm: [Finite; 3],
    radius_mm: PositiveFinite,
    owner_key: MemberKey,
    owner: Owner,
}

impl JointBall {
    pub const fn center_mm(&self) -> [Finite; 3] {
        self.center_mm
    }
    pub const fn radius_mm(&self) -> PositiveFinite {
        self.radius_mm
    }
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub const fn owner_key(&self) -> MemberKey {
        self.owner_key
    }
}

/// Opaque result bound to the exact live forest and all result-shaping policy.
#[derive(Clone, Debug)]
pub struct JointPlan {
    forest_digest: u64,
    live_view_digest: u64,
    config: ConfigFingerprint,
    balls: Vec<JointBall>,
    certified_dropped: usize,
    certification_resource_refused: usize,
}

impl JointPlan {
    pub fn validate<'a>(
        &'a self,
        forest: &Forest,
        config: JointConfig,
    ) -> Result<&'a [JointBall], JointError> {
        let live_count = forest.live_members_in_creation_order_iter().count();
        let all_live_digest = live_view_digest_iter(
            forest.digest(),
            live_count,
            forest.live_members_in_creation_order_iter(),
        );
        if self.forest_digest != forest.digest()
            || self.live_view_digest != all_live_digest
            || self.config != config.fingerprint()
        {
            return Err(JointError::StaleAuthority);
        }
        Ok(&self.balls)
    }

    pub(crate) fn validate_filtered<'a>(
        &'a self,
        forest: &Forest,
        view: &FilteredLiveView,
        config: JointConfig,
    ) -> Result<&'a [JointBall], JointError> {
        view.validate(forest)?;
        if self.forest_digest != forest.digest()
            || self.live_view_digest != view.digest()
            || self.config != config.fingerprint()
        {
            return Err(JointError::StaleAuthority);
        }
        Ok(&self.balls)
    }

    pub const fn certified_dropped(&self) -> usize {
        self.certified_dropped
    }

    /// Candidates conservatively retained because aggregate staging could not admit even one
    /// certificate at the fixed per-certificate proof budget.
    pub const fn certification_resource_refused(&self) -> usize {
        self.certification_resource_refused
    }

    /// Stable semantic digest used by the thread-count determinism gate.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut digest = 0xcbf2_9ce4_8422_2325_u64;
        let mut byte = |value: u8| {
            digest ^= u64::from(value);
            digest = digest.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for value in self.forest_digest.to_le_bytes() {
            byte(value);
        }
        for value in self.live_view_digest.to_le_bytes() {
            byte(value);
        }
        for value in self.config.plate_z.to_le_bytes() {
            byte(value);
        }
        for value in self.config.factor.to_le_bytes() {
            byte(value);
        }
        byte(self.config.max_depth);
        for value in u64::try_from(self.config.max_patches)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.config.max_work_items)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.config.max_scratch_bytes)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.config.max_certificate_bytes)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.balls.len())
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for ball in &self.balls {
            for coordinate in ball.center_mm {
                for value in coordinate.get().to_bits().to_le_bytes() {
                    byte(value);
                }
            }
            for value in ball.radius_mm.get().to_bits().to_le_bytes() {
                byte(value);
            }
            for value in ball.owner_key.slot().to_le_bytes() {
                byte(value);
            }
            for value in ball.owner_key.generation().to_le_bytes() {
                byte(value);
            }
            match &ball.owner {
                Owner::Contact(contact) => {
                    byte(0);
                    for value in contact.to_le_bytes() {
                        byte(value);
                    }
                }
                Owner::ModelAnchor(anchor) => {
                    byte(1);
                    for value in anchor.to_le_bytes() {
                        byte(value);
                    }
                }
                Owner::Generated { family, ordinal } => {
                    byte(2);
                    for value in u64::try_from(family.len())
                        .expect("platform usize fits u64")
                        .to_le_bytes()
                    {
                        byte(value);
                    }
                    for value in family.bytes() {
                        byte(value);
                    }
                    for value in ordinal.to_le_bytes() {
                        byte(value);
                    }
                }
            }
        }
        for value in u64::try_from(self.certified_dropped)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        for value in u64::try_from(self.certification_resource_refused)
            .expect("platform usize fits u64")
            .to_le_bytes()
        {
            byte(value);
        }
        digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum JointError {
    #[error("joint policy has a zero bound or exceeds the frozen certificate depth")]
    InvalidLimits,
    #[error("filtered live-view limits must be nonzero")]
    InvalidLiveViewLimits,
    #[error("filtered live view exceeds its member/unrouted-terminal limit")]
    LiveViewMemberLimit,
    #[error("filtered live view exceeds its byte limit")]
    LiveViewBytesLimit,
    #[error("unrouted terminal list contains a duplicate")]
    DuplicateUnroutedTerminal,
    #[error("terminal {0} is not valid unrouted terminal-ledger provenance")]
    InvalidUnroutedTerminal(u32),
    #[error("retired or quarantined terminal {0} is missing from unrouted provenance")]
    MissingUnroutedTerminalProvenance(u32),
    #[error("joint planning would exceed its typed work limit")]
    WorkLimit,
    #[error("joint planning would exceed its typed scratch-memory limit")]
    ScratchLimit,
    #[error("joint staging limits must all be nonzero")]
    InvalidStagingLimits,
    #[error("joint staging exceeds its caller-owned member/node/cluster/axis limit")]
    StagingCountLimit,
    #[error("joint spatial clustering exceeds its caller-owned work limit")]
    StagingWorkLimit,
    #[error("joint spatial clustering exceeds its caller-owned candidate-pair limit")]
    StagingPairLimit,
    #[error("joint result contains non-finite or non-positive geometry")]
    InvalidGeometry,
    #[error("joint plan authority is stale or belongs to another input")]
    StaleAuthority,
    #[error(transparent)]
    NativeBatch(#[from] geom_kernel::PointSegmentBatchError),
    #[error(transparent)]
    OrderedMap(#[from] geom_kernel::OrderedMapError),
}

#[derive(Clone)]
struct LiveMember {
    key: MemberKey,
    p0: [f64; 3],
    p1: [f64; 3],
    r0: f64,
    r1: f64,
    kind: MemberKind,
    owner: Owner,
    model_contact_end: Option<ContactEnd>,
}

impl LiveMember {
    fn from_forest(key: MemberKey, member: &Member) -> Self {
        Self {
            key,
            p0: member.p0_mm().map(Finite::get),
            p1: member.p1_mm().map(Finite::get),
            r0: member.terminal_radius_mm().get().get(),
            r1: member.rootward_radius_mm().get().get(),
            kind: member.kind().clone(),
            owner: member.owner().clone(),
            model_contact_end: member.model_contact_end(),
        }
    }
}

#[derive(Clone)]
struct Node {
    point: [f64; 3],
    radius: f64,
    embedded: bool,
    outward: [f64; 3],
    member: usize,
}

#[derive(Clone)]
struct CandidateBall {
    center: [f64; 3],
    radius: f64,
    owner_key: MemberKey,
    owner: Owner,
    touching: Vec<usize>,
}

/// Build all current pre-P8 joint balls from one immutable live-forest view.
/// Retired and quarantined members are unreachable through `live_members`.
pub fn plan_joint_balls(forest: &Forest, config: JointConfig) -> Result<JointPlan, JointError> {
    validate_limits(config)?;
    let view = FilteredLiveView::current(
        forest,
        &[],
        LiveViewLimits::new(
            config.max_work_items,
            config.max_work_items,
            config.max_scratch_bytes,
        ),
    )?;
    plan_joint_balls_filtered(
        forest,
        &view,
        config,
        JointStagingLimits::from_joint_config(config),
    )
}

/// The production entry: joint clustering and enclosure certification consume the exact same
/// preflighted filtered live view later lowered and raft-planned.
pub(crate) fn plan_joint_balls_filtered(
    forest: &Forest,
    view: &FilteredLiveView,
    config: JointConfig,
    staging: JointStagingLimits,
) -> Result<JointPlan, JointError> {
    validate_limits(config)?;
    validate_staging_limits(staging)?;
    view.validate(forest)?;
    let member_count = view.keys().len();
    if member_count > staging.max_members {
        return Err(JointError::StagingCountLimit);
    }
    let mut node_count = 0_usize;
    let mut axis_count = 0_usize;
    let mut owner_clone_bytes = 0_usize;
    let mut kind_clone_bytes = 0_usize;
    let mut max_owner_clone_bytes = 0_usize;
    for key in view.keys() {
        let member = forest.member(*key).ok_or(JointError::StaleAuthority)?;
        if member.kind() != &MemberKind::Stub {
            axis_count = axis_count
                .checked_add(1)
                .ok_or(JointError::StagingCountLimit)?;
        }
        if member.kind() != &MemberKind::Stub
            && (member.p0_mm()[2].get() - config.plate_z_mm.get()).abs() > PLATE_BAND_MM
        {
            node_count = node_count
                .checked_add(1)
                .ok_or(JointError::StagingCountLimit)?;
        }
        if (member.p1_mm()[2].get() - config.plate_z_mm.get()).abs() > PLATE_BAND_MM {
            node_count = node_count
                .checked_add(1)
                .ok_or(JointError::StagingCountLimit)?;
        }
        if let Owner::Generated { family, .. } = member.owner() {
            owner_clone_bytes = owner_clone_bytes
                .checked_add(family.len())
                .ok_or(JointError::ScratchLimit)?;
            max_owner_clone_bytes = max_owner_clone_bytes.max(family.len());
        }
        if let MemberKind::Other(label) = member.kind() {
            kind_clone_bytes = kind_clone_bytes
                .checked_add(label.len())
                .ok_or(JointError::ScratchLimit)?;
        }
    }
    if node_count > staging.max_nodes || axis_count > staging.max_axis_members {
        return Err(JointError::StagingCountLimit);
    }
    let node_bytes = node_count
        .checked_mul(std::mem::size_of::<Node>())
        .ok_or(JointError::ScratchLimit)?;
    let base_bytes = member_count
        .checked_mul(std::mem::size_of::<LiveMember>())
        .and_then(|bytes| bytes.checked_add(owner_clone_bytes))
        .and_then(|bytes| bytes.checked_add(kind_clone_bytes))
        .and_then(|bytes| bytes.checked_add(node_bytes))
        .ok_or(JointError::ScratchLimit)?;
    if base_bytes > staging.max_bytes {
        return Err(JointError::ScratchLimit);
    }
    let mut members = Vec::with_capacity(member_count);
    for key in view.keys().iter().copied() {
        let member = forest.member(key).ok_or(JointError::StaleAuthority)?;
        members.push(LiveMember::from_forest(key, member));
    }
    let forest_digest = forest.digest();
    let config_fingerprint = config.fingerprint();
    if members.is_empty() {
        return Ok(JointPlan {
            forest_digest,
            live_view_digest: view.digest(),
            config: config_fingerprint,
            balls: Vec::new(),
            certified_dropped: 0,
            certification_resource_refused: 0,
        });
    }

    let nodes = gather_nodes(&members, config.plate_z_mm.get(), node_count);
    if nodes.is_empty() {
        return Ok(JointPlan {
            forest_digest,
            live_view_digest: view.digest(),
            config: config_fingerprint,
            balls: Vec::new(),
            certified_dropped: 0,
            certification_resource_refused: 0,
        });
    }
    let clusters = cluster_nodes(&nodes, staging, base_bytes)?;
    if clusters.len() > staging.max_clusters {
        return Err(JointError::StagingCountLimit);
    }
    let retained_staging_bytes = preflight_post_cluster_staging(
        base_bytes,
        node_count,
        axis_count,
        clusters.len(),
        max_owner_clone_bytes,
        staging.max_bytes,
    )?;
    let mut axis_members = Vec::with_capacity(axis_count);
    axis_members.extend(
        members
            .iter()
            .enumerate()
            .filter_map(|(index, member)| (member.kind != MemberKind::Stub).then_some(index)),
    );
    let pairs = clusters
        .len()
        .checked_mul(axis_members.len())
        .ok_or(JointError::WorkLimit)?;
    if pairs > config.max_work_items {
        return Err(JointError::WorkLimit);
    }
    let scratch = pairs.checked_mul(16).ok_or(JointError::ScratchLimit)?;
    if scratch > config.max_scratch_bytes {
        return Err(JointError::ScratchLimit);
    }

    let centers: Vec<_> = clusters
        .iter()
        .map(|cluster| cluster_center(cluster, &nodes))
        .collect();
    let segment_starts: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].p0)
        .collect();
    let segment_ends: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].p1)
        .collect();
    let start_radii: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].r0)
        .collect();
    let end_radii: Vec<_> = axis_members
        .iter()
        .map(|index| members[*index].r1)
        .collect();
    let axis_batch = geom_kernel::point_segment_taper_batch(
        &centers,
        &segment_starts,
        &segment_ends,
        geom_kernel::FrozenJointTaperBatch::new(&start_radii, &end_radii),
        geom_kernel::PointSegmentBatchLimits::new(pairs, scratch),
    )?;
    debug_assert_eq!(
        axis_batch.effective_threads(),
        geom_kernel::current_threads()
    );
    let mut candidates = Vec::with_capacity(clusters.len());
    for (cluster_index, cluster) in clusters.iter().enumerate() {
        if is_colinear_seam(cluster, &nodes) {
            continue;
        }
        let center = centers[cluster_index];
        let fattest_node = cluster
            .iter()
            .copied()
            .reduce(|best, candidate| {
                if nodes[candidate].radius > nodes[best].radius {
                    candidate
                } else {
                    best
                }
            })
            .expect("cluster nonempty");
        let mut radius = nodes[fattest_node].radius;
        let mut owner = None;
        let mut touching = Vec::with_capacity(cluster.len() + axis_members.len());
        touching.extend(cluster.iter().map(|i| nodes[*i].member));
        let distance_row = axis_batch
            .distance_row(cluster_index)
            .expect("native batch retains one row per gathered cluster");
        let radius_row = axis_batch
            .local_radius_row(cluster_index)
            .expect("native batch retains one radius row per gathered cluster");
        for (axis_col, member_index) in axis_members.iter().copied().enumerate() {
            if distance_row[axis_col] <= ON_AXIS_TOL_MM {
                touching.push(member_index);
                let local = radius_row[axis_col];
                if local >= radius {
                    radius = local;
                    owner = Some((
                        members[member_index].key,
                        members[member_index].owner.clone(),
                    ));
                }
            }
        }
        let embedded = cluster.iter().any(|i| nodes[*i].embedded);
        let mut ball_radius = config.joint_ball_factor.get() * radius;
        if embedded && cluster.len() == 1 {
            ball_radius = MODEL_FOOT_FACTOR * nodes[fattest_node].radius;
        }
        if center[2] - ball_radius < config.plate_z_mm.get() {
            continue;
        }
        touching.sort_unstable();
        touching.dedup();
        candidates.push(CandidateBall {
            center,
            radius: ball_radius,
            owner_key: owner
                .as_ref()
                .map_or(members[nodes[fattest_node].member].key, |(key, _)| *key),
            owner: owner
                .map(|(_, owner)| owner)
                .unwrap_or_else(|| members[nodes[fattest_node].member].owner.clone()),
            touching,
        });
    }

    let certificate_scratch = CertificateScratch {
        max_bytes: config.max_certificate_bytes,
    };
    let mut decisions = Vec::with_capacity(candidates.len());
    let mut certification_resource_refused = 0;
    if !candidates.is_empty() {
        let certificate_window = certificate_parallel_window(
            retained_staging_bytes,
            staging.max_bytes,
            config.max_certificate_bytes,
        )?;
        if certificate_window == 0 {
            certification_resource_refused = candidates.len();
            decisions.resize(candidates.len(), false);
        } else {
            for window in candidates.chunks(certificate_window) {
                let mapped = geom_kernel::ordered_index_map(
                    window,
                    geom_kernel::OrderedMapLimits::new(
                        config.max_work_items,
                        config.max_scratch_bytes,
                    ),
                    |_, ball| ball_enclosed(ball, &members, config, certificate_scratch),
                )?;
                decisions.extend(mapped.into_values());
            }
        }
    }
    let mut balls = Vec::with_capacity(candidates.len());
    let mut certified_dropped = 0;
    for (candidate, enclosed) in candidates.into_iter().zip(decisions) {
        if enclosed {
            certified_dropped += 1;
            continue;
        }
        balls.push(JointBall {
            center_mm: candidate
                .center
                .map(|value| Finite::new(value).map_err(|_| JointError::InvalidGeometry))
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?
                .try_into()
                .map_err(|_| JointError::InvalidGeometry)?,
            radius_mm: PositiveFinite::new(candidate.radius)
                .map_err(|_| JointError::InvalidGeometry)?,
            owner_key: candidate.owner_key,
            owner: candidate.owner,
        });
    }
    Ok(JointPlan {
        forest_digest,
        live_view_digest: view.digest(),
        config: config_fingerprint,
        balls,
        certified_dropped,
        certification_resource_refused,
    })
}

fn validate_limits(config: JointConfig) -> Result<(), JointError> {
    if config.max_depth > 22
        || config.max_patches == 0
        || config.max_work_items == 0
        || config.max_scratch_bytes == 0
        || config.max_certificate_bytes == 0
        || config.max_certificate_bytes > config.max_scratch_bytes
    {
        return Err(JointError::InvalidLimits);
    }
    Ok(())
}

fn validate_staging_limits(staging: JointStagingLimits) -> Result<(), JointError> {
    if staging.max_members == 0
        || staging.max_nodes == 0
        || staging.max_neighbor_checks == 0
        || staging.max_neighbor_pairs == 0
        || staging.max_clusters == 0
        || staging.max_axis_members == 0
        || staging.max_bytes == 0
    {
        return Err(JointError::InvalidStagingLimits);
    }
    Ok(())
}

fn gather_nodes(members: &[LiveMember], plate_z: f64, node_count: usize) -> Vec<Node> {
    let mut nodes = Vec::with_capacity(node_count);
    for (index, member) in members.iter().enumerate() {
        let d = sub(member.p1, member.p0);
        let length = norm(d);
        let unit = if length > 1e-9 {
            scale(d, 1.0 / length)
        } else {
            [0.0; 3]
        };
        if member.kind != MemberKind::Stub && (member.p0[2] - plate_z).abs() > PLATE_BAND_MM {
            nodes.push(Node {
                point: member.p0,
                radius: member.r0,
                embedded: member.model_contact_end == Some(ContactEnd::Terminalward),
                outward: unit,
                member: index,
            });
        }
        if (member.p1[2] - plate_z).abs() > PLATE_BAND_MM {
            nodes.push(Node {
                point: member.p1,
                radius: member.r1,
                embedded: member.model_contact_end == Some(ContactEnd::Rootward),
                outward: scale(unit, -1.0),
                member: index,
            });
        }
    }
    nodes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CellRow {
    cell: [i64; 3],
    node: usize,
}

fn cluster_nodes(
    nodes: &[Node],
    staging: JointStagingLimits,
    base_bytes: usize,
) -> Result<Vec<Vec<usize>>, JointError> {
    let row_bytes = nodes
        .len()
        .checked_mul(std::mem::size_of::<CellRow>())
        .ok_or(JointError::ScratchLimit)?;
    ensure_staging_bytes(base_bytes, row_bytes, staging.max_bytes)?;
    let mut rows = Vec::with_capacity(nodes.len());
    for (node, value) in nodes.iter().enumerate() {
        rows.push(CellRow {
            cell: point_cell(value.point)?,
            node,
        });
    }
    rows.sort_unstable();

    let mut checks = 0_usize;
    let mut pair_count = 0_usize;
    visit_neighbor_pairs(nodes, &rows, |_, _, coincident| {
        checks = checks.checked_add(1).ok_or(JointError::StagingWorkLimit)?;
        if checks > staging.max_neighbor_checks {
            return Err(JointError::StagingWorkLimit);
        }
        if coincident {
            pair_count = pair_count
                .checked_add(1)
                .ok_or(JointError::StagingPairLimit)?;
            if pair_count > staging.max_neighbor_pairs {
                return Err(JointError::StagingPairLimit);
            }
        }
        Ok(())
    })?;
    let pair_bytes = pair_count
        .checked_mul(std::mem::size_of::<(usize, usize)>())
        .ok_or(JointError::ScratchLimit)?;
    let parent_bytes = nodes
        .len()
        .checked_mul(std::mem::size_of::<usize>())
        .ok_or(JointError::ScratchLimit)?;
    ensure_staging_bytes(
        base_bytes,
        row_bytes
            .checked_add(pair_bytes)
            .and_then(|value| value.checked_add(parent_bytes))
            .ok_or(JointError::ScratchLimit)?,
        staging.max_bytes,
    )?;
    let mut pairs = Vec::with_capacity(pair_count);
    visit_neighbor_pairs(nodes, &rows, |a, b, coincident| {
        if coincident {
            pairs.push((a, b));
        }
        Ok(())
    })?;
    pairs.sort_unstable();
    drop(rows);

    let mut parent: Vec<_> = (0..nodes.len()).collect();
    for (a, b) in pairs {
        let ra = find_root(&mut parent, a);
        let rb = find_root(&mut parent, b);
        if ra != rb {
            parent[rb] = ra;
        }
    }
    let mut grouped: Vec<_> = (0..nodes.len())
        .map(|node| (find_root(&mut parent, node), node))
        .collect();
    grouped.sort_unstable();
    let cluster_count = grouped
        .iter()
        .enumerate()
        .filter(|(index, row)| *index == 0 || grouped[*index - 1].0 != row.0)
        .count();
    if cluster_count > staging.max_clusters {
        return Err(JointError::StagingCountLimit);
    }
    let grouping_bytes = grouped
        .len()
        .checked_mul(std::mem::size_of::<(usize, usize)>())
        .and_then(|value| value.checked_add(parent_bytes))
        .and_then(|value| {
            value.checked_add(cluster_count.checked_mul(std::mem::size_of::<Vec<usize>>())?)
        })
        .and_then(|value| value.checked_add(nodes.len().checked_mul(std::mem::size_of::<usize>())?))
        .ok_or(JointError::ScratchLimit)?;
    ensure_staging_bytes(base_bytes, grouping_bytes, staging.max_bytes)?;
    let mut clusters: Vec<Vec<usize>> = Vec::with_capacity(cluster_count);
    let mut start = 0_usize;
    while start < grouped.len() {
        let root = grouped[start].0;
        let end = grouped[start..]
            .partition_point(|row| row.0 == root)
            .checked_add(start)
            .ok_or(JointError::StagingCountLimit)?;
        let mut cluster = Vec::with_capacity(end - start);
        cluster.extend(grouped[start..end].iter().map(|row| row.1));
        clusters.push(cluster);
        start = end;
    }
    // Python emits components at the first source node which encounters them. A union-find
    // representative is an implementation detail and is not record-order authority.
    clusters.sort_unstable_by_key(|cluster| cluster[0]);
    Ok(clusters)
}

fn ensure_staging_bytes(base: usize, additional: usize, limit: usize) -> Result<(), JointError> {
    if base
        .checked_add(additional)
        .ok_or(JointError::ScratchLimit)?
        > limit
    {
        return Err(JointError::ScratchLimit);
    }
    Ok(())
}

fn preflight_post_cluster_staging(
    base_bytes: usize,
    node_count: usize,
    axis_count: usize,
    cluster_count: usize,
    max_owner_clone_bytes: usize,
    limit: usize,
) -> Result<usize, JointError> {
    let pairs = cluster_count
        .checked_mul(axis_count)
        .ok_or(JointError::ScratchLimit)?;
    let touching_slots = node_count
        .checked_add(pairs)
        .ok_or(JointError::ScratchLimit)?;
    let terms = [
        cluster_count.checked_mul(std::mem::size_of::<Vec<usize>>()),
        node_count.checked_mul(std::mem::size_of::<usize>()),
        axis_count.checked_mul(std::mem::size_of::<usize>()),
        cluster_count.checked_mul(std::mem::size_of::<[f64; 3]>()),
        axis_count.checked_mul(2 * std::mem::size_of::<[f64; 3]>()),
        axis_count.checked_mul(2 * std::mem::size_of::<f64>()),
        pairs.checked_mul(16),
        cluster_count.checked_mul(std::mem::size_of::<CandidateBall>()),
        touching_slots.checked_mul(std::mem::size_of::<usize>()),
        cluster_count.checked_mul(max_owner_clone_bytes),
        cluster_count.checked_mul(std::mem::size_of::<bool>()),
        cluster_count.checked_mul(std::mem::size_of::<JointBall>()),
    ];
    let additional = terms
        .into_iter()
        .try_fold(0_usize, |total, term| total.checked_add(term?));
    let additional = additional.ok_or(JointError::ScratchLimit)?;
    ensure_staging_bytes(base_bytes, additional, limit)?;
    base_bytes
        .checked_add(additional)
        .ok_or(JointError::ScratchLimit)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "float-to-integer casts on lattice or step arithmetic: Rust's `as` saturates at the integer \
          bounds and maps NaN to 0, and every site in this function is range-checked before the cast \
          or clamped into range after it (read site by site, 2026-08-26). See \
          docs/evidence/remediation-2026-08-26/census/cast-audit.md"
)]
fn point_cell(point: [f64; 3]) -> Result<[i64; 3], JointError> {
    let mut cell = [0_i64; 3];
    for axis in 0..3 {
        let value = (point[axis] / COINCIDE_TOL_MM).floor();
        if !value.is_finite() || value < i64::MIN as f64 || value > i64::MAX as f64 {
            return Err(JointError::InvalidGeometry);
        }
        cell[axis] = value as i64;
    }
    Ok(cell)
}

fn visit_neighbor_pairs(
    nodes: &[Node],
    rows: &[CellRow],
    mut visit: impl FnMut(usize, usize, bool) -> Result<(), JointError>,
) -> Result<(), JointError> {
    for (a, node) in nodes.iter().enumerate() {
        let cell = point_cell(node.point)?;
        for dx in -1_i64..=1 {
            for dy in -1_i64..=1 {
                for dz in -1_i64..=1 {
                    let Some(neighbor) = cell[0]
                        .checked_add(dx)
                        .zip(cell[1].checked_add(dy))
                        .zip(cell[2].checked_add(dz))
                        .map(|((x, y), z)| [x, y, z])
                    else {
                        continue;
                    };
                    let start = rows.partition_point(|row| row.cell < neighbor);
                    let end = rows.partition_point(|row| row.cell <= neighbor);
                    for row in &rows[start..end] {
                        if row.node <= a {
                            continue;
                        }
                        visit(
                            a,
                            row.node,
                            norm(sub(node.point, nodes[row.node].point)) <= COINCIDE_TOL_MM,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn find_root(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

fn cluster_center(cluster: &[usize], nodes: &[Node]) -> [f64; 3] {
    let index = cluster
        .iter()
        .copied()
        .reduce(|best, candidate| {
            if nodes[candidate].radius > nodes[best].radius {
                candidate
            } else {
                best
            }
        })
        .expect("cluster nonempty");
    nodes[index].point
}

fn is_colinear_seam(cluster: &[usize], nodes: &[Node]) -> bool {
    if cluster.len() != 2 {
        return false;
    }
    let a = &nodes[cluster[0]];
    let b = &nodes[cluster[1]];
    let cosine = dot(a.outward, b.outward);
    let high = a.radius.max(b.radius);
    let close = high <= 1e-12 || high - a.radius.min(b.radius) <= COLINEAR_RADIUS_FRAC * high;
    cosine <= COLINEAR_COS_MAX && close
}

#[derive(Clone, Copy)]
struct CertificateScratch {
    max_bytes: usize,
}

fn certificate_parallel_window(
    retained_bytes: usize,
    staging_limit: usize,
    per_certificate_limit: usize,
) -> Result<usize, JointError> {
    let available = staging_limit
        .checked_sub(retained_bytes)
        .ok_or(JointError::ScratchLimit)?;
    let window = available / per_certificate_limit;
    Ok(window)
}

fn ball_enclosed(
    ball: &CandidateBall,
    members: &[LiveMember],
    config: JointConfig,
    scratch: CertificateScratch,
) -> bool {
    if ball.radius <= 0.0 || ball.touching.is_empty() {
        return false;
    }
    let credited_bytes = match ball
        .touching
        .len()
        .checked_mul(std::mem::size_of::<usize>())
    {
        Some(bytes) if bytes <= scratch.max_bytes => bytes,
        _ => return false,
    };
    let mut credited = Vec::with_capacity(ball.touching.len());
    credited.extend(ball.touching.iter().copied().filter(|index| {
        signed_distance_to_frustum(ball.center, &members[*index]) <= ENCLOSURE_PROOF_TOL_MM
    }));
    if credited.is_empty() {
        return false;
    }
    certify_sphere(ball, &credited, credited_bytes, members, config, scratch)
}

#[derive(Clone, Copy)]
struct Patch {
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    depth: u8,
}

fn certify_sphere(
    ball: &CandidateBall,
    credited: &[usize],
    credited_bytes: usize,
    members: &[LiveMember],
    config: JointConfig,
    scratch: CertificateScratch,
) -> bool {
    let initial_bytes = match 20_usize.checked_mul(std::mem::size_of::<Patch>()) {
        Some(bytes) => bytes,
        None => return false,
    };
    if !certificate_peak_fits(credited_bytes, initial_bytes, scratch) {
        return false;
    }
    // Canon C4: `icosahedron` normalises its twelve seed vertices through
    // `vec3::normalize`, which declines rather than returning `[NaN; 3]`.
    // A decline here is a numerical refusal, and this function's contract is
    // that any numerical, depth, work, or memory limit leaves the ball in the
    // result (module header) - so it is `false`, exactly like every other
    // bail-out above.
    let Some(mut patches) = icosahedron() else {
        return false;
    };
    let mut total = patches.len();
    if total > config.max_patches {
        return false;
    }
    let mut work = 0_usize;
    while !patches.is_empty() {
        work = match patches
            .len()
            .checked_mul(credited.len())
            .and_then(|next| work.checked_add(next))
        {
            Some(value) if value <= config.max_work_items => value,
            _ => return false,
        };
        let frontier_count = patches.len();
        let frontier_bytes = match frontier_count
            .checked_mul(2 * std::mem::size_of::<Patch>() + 6 * std::mem::size_of::<f64>())
        {
            Some(bytes) => bytes,
            None => return false,
        };
        if !certificate_peak_fits(credited_bytes, frontier_bytes, scratch) {
            return false;
        }
        let mut theta = Vec::with_capacity(patches.len());
        let mut points = Vec::with_capacity(patches.len());
        for patch in &patches {
            // Canon C4 (`docs/specs/geometry-canon.md` §1 OP-6): the retired
            // private `normalize` returned a measured `[NaN, NaN, NaN]` for a
            // zero-length vector, and a NaN direction compares `false` against
            // every `<` and `>` downstream — a value meaning "measured
            // nothing" that every guard reads as "not violated". The kernel
            // declines instead, and a decline is a numerical refusal, which
            // this function answers with `false` (ball retained).
            let Some(direction) = normalize(add(add(patch.a, patch.b), patch.c)) else {
                return false;
            };
            theta.push(
                angle(direction, patch.a)
                    .max(angle(direction, patch.b))
                    .max(angle(direction, patch.c)),
            );
            points.push(add(ball.center, scale(direction, ball.radius)));
        }
        let mut best = vec![f64::NEG_INFINITY; patches.len()];
        let mut signed = vec![0.0; patches.len()];
        // Gather one complete patch frontier, make one batched distance call
        // per credited member, then scatter into the union maximum.
        for member in credited {
            signed_distances_to_frustum(&points, &members[*member], &mut signed);
            for (destination, distance) in best.iter_mut().zip(&signed) {
                *destination = destination.max(ENCLOSURE_PROOF_TOL_MM - *distance);
            }
        }
        let mut unresolved = Vec::with_capacity(frontier_count);
        for ((patch, best), theta) in patches.into_iter().zip(best).zip(theta) {
            if best < 0.0 {
                return false;
            }
            if best >= ball.radius * theta {
                continue;
            }
            if patch.depth >= config.max_depth {
                return false;
            }
            unresolved.push(patch);
        }
        if unresolved.is_empty() {
            return true;
        }
        total = match total.checked_add(3 * unresolved.len()) {
            Some(value) => value,
            None => return false,
        };
        if total > config.max_patches {
            return false;
        }
        drop(points);
        drop(signed);
        let child_count = match unresolved.len().checked_mul(4) {
            Some(value) => value,
            None => return false,
        };
        let child_phase_bytes = match frontier_count
            .checked_add(child_count)
            .and_then(|count| count.checked_mul(std::mem::size_of::<Patch>()))
        {
            Some(value) => value,
            None => return false,
        };
        if !certificate_peak_fits(credited_bytes, child_phase_bytes, scratch) {
            return false;
        }
        let scratch_bytes = match unresolved
            .len()
            .checked_mul(4)
            .and_then(|count| count.checked_mul(std::mem::size_of::<Patch>()))
        {
            Some(value) => value,
            None => return false,
        };
        if scratch_bytes > config.max_scratch_bytes {
            return false;
        }
        let mut children = Vec::with_capacity(child_count);
        for patch in unresolved {
            // Canon C4: as above — an edge midpoint with no direction is a
            // refusal (`false`, ball retained), never a `[NaN; 3]` vertex fed
            // into the next subdivision level.
            let (Some(ab), Some(bc), Some(ca)) = (
                normalize(add(patch.a, patch.b)),
                normalize(add(patch.b, patch.c)),
                normalize(add(patch.c, patch.a)),
            ) else {
                return false;
            };
            let depth = patch.depth + 1;
            children.extend([
                Patch {
                    a: patch.a,
                    b: ab,
                    c: ca,
                    depth,
                },
                Patch {
                    a: patch.b,
                    b: bc,
                    c: ab,
                    depth,
                },
                Patch {
                    a: patch.c,
                    b: ca,
                    c: bc,
                    depth,
                },
                Patch {
                    a: ab,
                    b: bc,
                    c: ca,
                    depth,
                },
            ]);
        }
        patches = children;
    }
    true
}

fn certificate_peak_fits(retained: usize, transient: usize, scratch: CertificateScratch) -> bool {
    retained
        .checked_add(transient)
        .is_some_and(|peak| peak <= scratch.max_bytes)
}

fn signed_distances_to_frustum(points: &[[f64; 3]], member: &LiveMember, output: &mut [f64]) {
    debug_assert_eq!(points.len(), output.len());
    for (point, destination) in points.iter().copied().zip(output) {
        *destination = signed_distance_to_frustum(point, member);
    }
}

fn signed_distance_to_frustum(point: [f64; 3], member: &LiveMember) -> f64 {
    let d = sub(member.p1, member.p0);
    // GEOM-EXEMPT(the THRESHOLD, not the arithmetic) — GEOMETRY-CANON EXEMPT:
    // canon §1 OP-3 rules only on the 2D slice-lattice degeneracy test; this is
    // a 3D axis-degeneracy guard on frustum member endpoints, which the canon
    // does not cover. The formula is now the kernel's `dot(d, d)` (identical
    // text, byte-neutral, canon §1 OP-9); `DEGENERATE_LENGTH_SQUARED` is left
    // exactly as it was because choosing a value for it would be improvising
    // policy the canon deliberately did not set.
    let l2 = dot(d, d);
    if l2 < DEGENERATE_LENGTH_SQUARED {
        return norm(sub(point, member.p0)) - member.r0;
    }
    let length = l2.sqrt();
    let axis = scale(d, 1.0 / length);
    let delta = sub(point, member.p0);
    let axial = dot(delta, axis);
    let radial_vector = sub(sub(point, member.p0), scale(axis, axial));
    let radial = norm(radial_vector);
    let cap0 = segment_distance_2d(axial, radial, 0.0, 0.0, 0.0, member.r0);
    let slant = segment_distance_2d(axial, radial, 0.0, member.r0, length, member.r1);
    let cap1 = segment_distance_2d(axial, radial, length, member.r1, length, 0.0);
    let distance = cap0.min(slant).min(cap1);
    let local_radius = member.r0 + (member.r1 - member.r0) * axial.clamp(0.0, length) / length;
    if axial >= 0.0 && axial <= length && radial <= local_radius {
        -distance
    } else {
        distance
    }
}

/// Distance from `(qx, qy)` to the segment `(ax, ay)`-`(bx, by)`.
///
/// GEOM-EXEMPT(different data: this is an `(axial, radial)` frustum PROFILE,
/// not a slice loop) — GEOMETRY-CANON EXEMPT: canon §4 item 5 and §1 OP-3.
/// `geom_kernel::poly2::point_segment_distance` carries
/// `DEGENERATE_EDGE_L2_MM2`, derived from the 1e-8 mm SLICE lattice; that
/// derivation is meaningless for a profile whose degeneracies come from
/// `r0`/`r1` caps collapsing to exactly zero, which is what
/// `DEGENERATE_LENGTH_SQUARED` guards. The canon forbids growing `poly2` a
/// tolerance parameter to absorb this, so the formula stays here.
fn segment_distance_2d(qx: f64, qy: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let l2 = dx * dx + dy * dy;
    if l2 < DEGENERATE_LENGTH_SQUARED {
        return (qx - ax).hypot(qy - ay);
    }
    let t = (((qx - ax) * dx + (qy - ay) * dy) / l2).clamp(0.0, 1.0);
    (qx - (ax + t * dx)).hypot(qy - (ay + t * dy))
}

/// The twenty seed patches of a unit icosahedron, or `None` if any seed vertex
/// has no direction.
///
/// Canon C4: the `None` arm is unreachable for the twelve compile-time seeds
/// below — every one has norm `sqrt(1 + phi^2)` = 1.902..., verified by the
/// `icosahedron_seeds_all_normalize` test — but it is a real `Option` rather
/// than an `expect`, because the alternative the canon forbids is exactly the
/// one this function used to take: divide by the norm unguarded and hand back
/// `[NaN, NaN, NaN]`.
fn icosahedron() -> Option<Vec<Patch>> {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let mut vertices = [[0.0; 3]; 12];
    let mut next = 0;
    for a in [-1.0, 1.0] {
        for b in [-1.0, 1.0] {
            for vertex in [[0.0, a, b * phi], [a, b * phi, 0.0], [b * phi, 0.0, a]] {
                vertices[next] = vertex;
                next += 1;
            }
        }
    }
    for vertex in &mut vertices {
        *vertex = normalize(*vertex)?;
    }
    let mut nearest = f64::INFINITY;
    for i in 0..vertices.len() {
        for j in (i + 1)..vertices.len() {
            nearest = nearest.min(norm(sub(vertices[i], vertices[j])));
        }
    }
    let adjacent = |i: usize, j: usize| norm(sub(vertices[i], vertices[j])) <= nearest * 1.01;
    let mut faces = Vec::with_capacity(20);
    for i in 0..vertices.len() {
        for j in (i + 1)..vertices.len() {
            if !adjacent(i, j) {
                continue;
            }
            for k in (j + 1)..vertices.len() {
                if adjacent(i, k) && adjacent(j, k) {
                    faces.push(Patch {
                        a: vertices[i],
                        b: vertices[j],
                        c: vertices[k],
                        depth: 0,
                    });
                }
            }
        }
    }
    debug_assert_eq!(faces.len(), 20);
    Some(faces)
}

/// Angle in radians between two UNIT vectors, `acos(clamp(a.b, -1, 1))`.
///
/// Not a kernel operation — it is a joints-local policy that a dot product a
/// hair outside `[-1, 1]` from rounding is clamped rather than turned into a
/// `NaN` by `acos`. The dot itself is the kernel's (canon §1 OP-7).
fn angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    dot(a, b).clamp(-1.0, 1.0).acos()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::forest::{MemberKey, MemberSpec, RootwardEndRadius, TerminalEndRadius};

    /// Canon item C4 (`docs/specs/geometry-canon.md` §1 OP-6): the retired
    /// private `normalize` divided by the norm unguarded and handed back
    /// `[NaN, NaN, NaN]` for the zero vector — a value meaning "measured
    /// nothing" that every downstream `<`/`>` reads as "not violated".
    /// `geom_kernel::vec3::normalize` declines instead, so `icosahedron`
    /// returns `Option`.
    ///
    /// The `None` arm is unreachable for these twelve compile-time seeds, and
    /// this test is what says so: every seed normalises, all twenty patches
    /// are built, and every patch vertex is a unit vector.
    #[test]
    fn icosahedron_seeds_all_normalize() {
        let patches = icosahedron().expect("every icosahedron seed has a direction");
        assert_eq!(patches.len(), 20);
        for patch in &patches {
            for vertex in [patch.a, patch.b, patch.c] {
                let length = norm(vertex);
                assert!(
                    (length - 1.0).abs() < 1e-15,
                    "patch vertex {vertex:?} has length {length}, expected unit"
                );
                assert!(
                    vertex.iter().all(|value| value.is_finite()),
                    "patch vertex {vertex:?} is not finite -- this is exactly the \
                     [NaN, NaN, NaN] the C4 refusal removes"
                );
            }
            assert_eq!(patch.depth, 0);
        }
    }

    /// Canon C4, the other half: the refusal the new contract adds is real,
    /// not decorative. A direction cannot be derived from a zero vector, and
    /// the answer is `None` rather than a sentinel every guard passes.
    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "exact pins are the claim: `normalize` answers `None` or an exact unit basis \
                  vector, and a tolerance here could not tell a sentinel from a refusal. See \
                  docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn a_directionless_vector_refuses_instead_of_returning_nan() {
        assert_eq!(normalize([0.0, 0.0, 0.0]), None);
        assert_eq!(normalize([-0.0, -0.0, -0.0]), None);
        assert_eq!(normalize([f64::NAN, 0.0, 0.0]), None);
        assert_eq!(normalize([f64::INFINITY, 0.0, 0.0]), None);
        // ...and a real direction still normalises.
        let unit = normalize([0.0, 0.0, 3.0]).expect("a 3 mm +z vector has a direction");
        assert_eq!(unit, [0.0, 0.0, 1.0]);
    }

    fn finite3(value: [f64; 3]) -> [Finite; 3] {
        value.map(|v| Finite::new(v).unwrap())
    }
    fn radius(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "test fixture helper mirrors the frozen member schema field-for-field"
    )]
    fn add_member(
        forest: &mut Forest,
        p0: [f64; 3],
        p1: [f64; 3],
        r0: f64,
        r1: f64,
        kind: MemberKind,
        owner: Owner,
        model_contact_end: Option<ContactEnd>,
    ) -> MemberKey {
        forest.add_member(Member::new(MemberSpec {
            p0_mm: finite3(p0),
            p1_mm: finite3(p1),
            r0_mm: TerminalEndRadius::new(radius(r0)),
            r1_mm: RootwardEndRadius::new(radius(r1)),
            kind,
            owner,
            model_contact_end,
            carried_mm2: Finite::new(0.0).unwrap(),
        }))
    }
    fn config(plate: f64) -> JointConfig {
        JointConfig::current(Finite::new(plate).unwrap(), radius(1.0))
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn filtered_live_view_preflights_exact_caps_and_excludes_unrouted_before_joint_clustering() {
        let mut forest = Forest::new();
        let routed = add_member(
            &mut forest,
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 3.0],
            0.2,
            0.4,
            MemberKind::Stub,
            Owner::Contact(0),
            Some(ContactEnd::Terminalward),
        );
        let unrouted = add_member(
            &mut forest,
            [9.0, 0.0, 5.0],
            [9.0, 0.0, 3.0],
            0.2,
            0.4,
            MemberKind::Stub,
            Owner::Contact(1),
            Some(ContactEnd::Terminalward),
        );
        forest.register_terminal(0, routed).unwrap();
        forest.register_terminal(1, unrouted).unwrap();
        forest.retire(unrouted).unwrap();
        let exact_bytes = std::mem::size_of::<MemberKey>()
            + std::mem::size_of::<u32>()
            + std::mem::size_of::<MemberKey>();
        let view = FilteredLiveView::current(&forest, &[1], LiveViewLimits::new(2, 1, exact_bytes))
            .unwrap();
        assert_eq!(view.keys(), &[routed]);
        assert!(
            FilteredLiveView::current(&forest, &[1], LiveViewLimits::new(1, 1, exact_bytes),)
                .is_ok()
        );
        assert_eq!(
            FilteredLiveView::current(&forest, &[1], LiveViewLimits::new(2, 0, exact_bytes),)
                .unwrap_err(),
            JointError::LiveViewMemberLimit
        );
        assert_eq!(
            FilteredLiveView::current(&forest, &[1], LiveViewLimits::new(2, 1, exact_bytes - 1),)
                .unwrap_err(),
            JointError::LiveViewBytesLimit
        );

        let joint_config = config(-10.0);
        let filtered = plan_joint_balls_filtered(
            &forest,
            &view,
            joint_config,
            JointStagingLimits::from_joint_config(joint_config),
        )
        .unwrap();
        assert!(filtered
            .balls
            .iter()
            .all(|ball| ball.center_mm[0].get() != 9.0));
        assert!(filtered
            .validate_filtered(&forest, &view, config(-10.0))
            .is_ok());
    }

    #[test]
    fn filtered_view_requires_retired_and_quarantined_terminal_ledger_provenance_without_resurrection(
    ) {
        let mut forest = Forest::new();
        let mut keys = Vec::new();
        for contact in 0..3 {
            let key = add_member(
                &mut forest,
                [contact as f64, 0.0, 5.0],
                [contact as f64, 0.0, 3.0],
                0.2,
                0.4,
                MemberKind::Stub,
                Owner::Contact(contact),
                Some(ContactEnd::Terminalward),
            );
            forest.register_terminal(contact, key).unwrap();
            keys.push(key);
        }
        forest.retire(keys[1]).unwrap();
        forest.quarantine(keys[2]).unwrap();
        assert_eq!(
            forest.terminal_ledger(1).unwrap().state,
            TerminalLedgerState::Retired
        );
        assert_eq!(
            forest.terminal_ledger(2).unwrap().state,
            TerminalLedgerState::Quarantined
        );
        assert_eq!(
            FilteredLiveView::current(&forest, &[1], LiveViewLimits::new(3, 3, 10_000),)
                .unwrap_err(),
            JointError::MissingUnroutedTerminalProvenance(2)
        );
        let view =
            FilteredLiveView::current(&forest, &[1, 2], LiveViewLimits::new(3, 3, 10_000)).unwrap();
        assert_eq!(view.keys(), &[keys[0]]);
        assert!(forest.member(keys[1]).is_none());
        assert!(forest.member(keys[2]).is_none());
        assert_eq!(forest.terminal(1), None);
        assert_eq!(forest.terminal(2), None);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn mid_column_enclosure_drops_but_elbow_direct_witness_keeps() {
        let mut forest = Forest::new();
        add_member(
            &mut forest,
            [4.0, 0.0, -5.0],
            [4.0, 0.0, 30.0],
            2.0,
            2.0,
            MemberKind::Trunk,
            Owner::Generated {
                family: "trunk".into(),
                ordinal: 0,
            },
            None,
        );
        add_member(
            &mut forest,
            [10.0, 0.0, 10.0],
            [4.0, 0.0, 10.0],
            0.3,
            0.3,
            MemberKind::Brace,
            Owner::Generated {
                family: "brace".into(),
                ordinal: 0,
            },
            None,
        );
        add_member(
            &mut forest,
            [20.0, 0.0, 0.0],
            [20.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Generated {
                family: "trunk".into(),
                ordinal: 1,
            },
            None,
        );
        add_member(
            &mut forest,
            [20.0, 0.0, 10.0],
            [30.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Generated {
                family: "trunk".into(),
                ordinal: 2,
            },
            None,
        );
        let plan = plan_joint_balls(&forest, config(-100.0)).unwrap();
        let balls = plan.validate(&forest, config(-100.0)).unwrap();
        assert!(plan.certified_dropped() >= 1);
        assert!(balls
            .iter()
            .any(|ball| ball.center_mm().map(Finite::get) == [20.0, 0.0, 10.0]));
        assert!(!balls
            .iter()
            .any(|ball| ball.center_mm().map(Finite::get) == [4.0, 0.0, 10.0]));
    }

    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn credited_member_restriction_keeps_jittered_real_hub() {
        let mut forest = Forest::new();
        let rows = [
            (
                [
                    -0.0012763566500100013,
                    -0.02018765375559623,
                    -0.0255239036087158,
                ],
                [0.8935354235585666, -9.178482782952365, -0.33559696897299834],
                0.37905493221448316,
            ),
            (
                [
                    -0.03286932321566698,
                    0.016295075739409165,
                    -0.046784730941915216,
                ],
                [-3.7516483645842533, 5.112004744985008, -2.272011145171266],
                0.3855562378313358,
            ),
            (
                [
                    -0.07496186943470244,
                    0.00031463602499591867,
                    0.012583727280577122,
                ],
                [1.8226767721828085, 3.1898157854197215, -3.832257784358506],
                0.3126512418157271,
            ),
            (
                [
                    -0.0006109511611611406,
                    -0.02637707564935988,
                    -0.04956143384038976,
                ],
                [1.6189135678109392, -4.498665682157377, 9.512731015295648],
                0.38313887477261743,
            ),
        ];
        for (ordinal, (p0, p1, r)) in rows.into_iter().enumerate() {
            add_member(
                &mut forest,
                p0,
                p1,
                r,
                r,
                MemberKind::Trunk,
                Owner::Generated {
                    family: "trunk".into(),
                    ordinal: ordinal as u32,
                },
                None,
            );
        }
        let plan = plan_joint_balls(&forest, config(-1000.0)).unwrap();
        let ball = plan
            .validate(&forest, config(-1000.0))
            .unwrap()
            .iter()
            .find(|b| norm(b.center_mm().map(Finite::get)) < 0.2)
            .unwrap();
        assert_eq!(ball.radius_mm().get().to_bits(), 0x3fd8_acf4_1210_47d9);
    }

    #[test]
    fn frozen_noncoplanar_taper_gap_is_kept() {
        let mut forest = Forest::new();
        add_member(
            &mut forest,
            [0.9364904936757851, 0.2714744007119751, 0.752930226115027],
            [0.0, 0.0, 0.0],
            0.04431473021227952,
            0.16790109202464978,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [
                -1.0818439371612572,
                -0.3694209493362457,
                -0.45908450006900064,
            ],
            0.1676098823931195,
            0.6619919334695765,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        let plan = plan_joint_balls(&forest, config(-1000.0)).unwrap();
        let ball = plan
            .validate(&forest, config(-1000.0))
            .unwrap()
            .iter()
            .find(|ball| norm(ball.center_mm().map(Finite::get)) < 1e-6)
            .expect("the measured 0.023 mm gap must retain its joint ball");
        assert_eq!(ball.radius_mm().get().to_bits(), 0x3fc5_7dc8_719a_b201);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn real_plate_crossing_specimen_is_dropped() {
        let mut forest = Forest::new();
        let plate_z = -66.97575;
        add_member(
            &mut forest,
            [13.40, -7.33, -40.0],
            [13.40, -7.33, -66.17],
            0.8268,
            0.8268,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        let cfg = config(plate_z);
        let plan = plan_joint_balls(&forest, cfg).unwrap();
        assert!(!plan
            .validate(&forest, cfg)
            .unwrap()
            .iter()
            .any(|ball| { ball.center_mm().map(Finite::get) == [13.40, -7.33, -66.17] }));
    }

    #[test]
    fn taper_distance_and_batch_arithmetic_are_component_ordered() {
        let mut forest = Forest::new();
        let key = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [3.0, 4.0, 12.0],
            0.25,
            1.25,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        let member = LiveMember::from_forest(key, forest.member(key).unwrap());
        let centers = [[1.5, 2.0, 6.0], [10.0, -2.0, 1.0]];
        let batch = geom_kernel::point_segment_taper_batch(
            &centers,
            &[member.p0],
            &[member.p1],
            geom_kernel::FrozenJointTaperBatch::new(&[member.r0], &[member.r1]),
            geom_kernel::PointSegmentBatchLimits::new(2, 32),
        )
        .unwrap();
        let distance = [
            batch.distance_row(0).unwrap()[0],
            batch.distance_row(1).unwrap()[0],
        ];
        let radius = [
            batch.local_radius_row(0).unwrap()[0],
            batch.local_radius_row(1).unwrap()[0],
        ];
        assert_eq!(distance[0].to_bits(), 0x0000_0000_0000_0000);
        assert_eq!(radius[0].to_bits(), 0x3fe8_0000_0000_0000);
        assert_eq!(
            distance[1].to_bits(),
            dist_and_radius_scalar(centers[1], &member).0.to_bits()
        );
        assert_eq!(
            radius[1].to_bits(),
            dist_and_radius_scalar(centers[1], &member).1.to_bits()
        );
        let endpoint = signed_distance_to_frustum([0.0, 0.0, 0.0], &member);
        assert!(endpoint.is_sign_negative());
    }

    #[test]
    #[expect(
        clippy::cast_sign_loss,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn coincidence_boundary_and_equal_radius_center_ties_are_frozen() {
        let node = |point, ordinal| Node {
            point,
            radius: 0.5,
            embedded: false,
            outward: [0.0, 0.0, 1.0],
            member: ordinal as usize,
        };
        let staging = JointStagingLimits::new(10, 10, 10, 10, 10, 10, 10_000);
        let inclusive = vec![node([-0.0, 0.0, 1.0], 0), node([0.15, 0.0, 1.0], 1)];
        let clusters = cluster_nodes(&inclusive, staging, 0).unwrap();
        assert_eq!(clusters, vec![vec![0, 1]]);
        let center = cluster_center(&clusters[0], &inclusive);
        assert!(center[0].is_sign_negative());

        let outside = vec![
            node([0.0, 0.0, 1.0], 0),
            node([f64::from_bits(0.15_f64.to_bits() + 1), 0.0, 1.0], 1),
        ];
        assert_eq!(
            cluster_nodes(&outside, staging, 0).unwrap(),
            vec![vec![0], vec![1]]
        );
    }

    #[test]
    fn spatial_staging_caps_are_exact_and_separated_stubs_do_not_form_quadratic_work() {
        let node = |x| Node {
            point: [x, 0.0, 1.0],
            radius: 0.5,
            embedded: false,
            outward: [0.0, 0.0, 1.0],
            member: 0,
        };
        let coincident = vec![node(0.0), node(0.0), node(0.0)];
        assert_eq!(
            cluster_nodes(
                &coincident,
                JointStagingLimits::new(3, 3, 2, 3, 3, 3, 100_000),
                0,
            )
            .unwrap_err(),
            JointError::StagingWorkLimit
        );
        assert_eq!(
            cluster_nodes(
                &coincident,
                JointStagingLimits::new(3, 3, 3, 2, 3, 3, 100_000),
                0,
            )
            .unwrap_err(),
            JointError::StagingPairLimit
        );

        let two = vec![node(0.0), node(0.0)];
        let clustering_peak = 2 * std::mem::size_of::<CellRow>()
            + std::mem::size_of::<(usize, usize)>()
            + 2 * std::mem::size_of::<usize>();
        let grouping_peak = 2 * std::mem::size_of::<(usize, usize)>()
            + 2 * std::mem::size_of::<usize>()
            + std::mem::size_of::<Vec<usize>>()
            + 2 * std::mem::size_of::<usize>();
        let exact_bytes = clustering_peak.max(grouping_peak);
        assert_eq!(
            cluster_nodes(
                &two,
                JointStagingLimits::new(2, 2, 1, 1, 1, 2, exact_bytes),
                0,
            )
            .unwrap(),
            vec![vec![0, 1]]
        );
        assert_eq!(
            cluster_nodes(
                &two,
                JointStagingLimits::new(2, 2, 1, 1, 1, 2, exact_bytes - 1),
                0,
            )
            .unwrap_err(),
            JointError::ScratchLimit
        );

        let separated: Vec<_> = (0..2_000).map(|index| node(index as f64)).collect();
        let clusters = cluster_nodes(
            &separated,
            JointStagingLimits::new(2_000, 2_000, 1, 1, 2_000, 2_000, 1_000_000),
            0,
        )
        .unwrap();
        assert_eq!(clusters.len(), 2_000);
    }

    #[test]
    fn spatial_components_follow_first_source_encounter_not_cell_or_representative_order() {
        let node = |x| Node {
            point: [x, 0.0, 1.0],
            radius: 0.5,
            embedded: false,
            outward: [0.0, 0.0, 1.0],
            member: 0,
        };
        let nodes = vec![node(10.0), node(-10.0), node(10.05), node(-9.95)];
        assert_eq!(
            cluster_nodes(
                &nodes,
                JointStagingLimits::new(4, 4, 2, 2, 2, 4, 100_000),
                0,
            )
            .unwrap(),
            vec![vec![0, 2], vec![1, 3]]
        );
    }

    #[test]
    fn staging_bills_cloned_other_kind_string_before_member_allocation() {
        let mut forest = Forest::new();
        add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0.2,
            0.2,
            MemberKind::Other("abc".into()),
            Owner::Contact(99),
            None,
        );
        let view = FilteredLiveView::current(
            &forest,
            &[],
            LiveViewLimits::new(1, 1, std::mem::size_of::<MemberKey>()),
        )
        .unwrap();
        let config = config(0.0);
        let exact = std::mem::size_of::<LiveMember>() + 3;
        assert!(plan_joint_balls_filtered(
            &forest,
            &view,
            config,
            JointStagingLimits::new(1, 1, 1, 1, 1, 1, exact),
        )
        .is_ok());
        assert_eq!(
            plan_joint_balls_filtered(
                &forest,
                &view,
                config,
                JointStagingLimits::new(1, 1, 1, 1, 1, 1, exact - 1),
            )
            .unwrap_err(),
            JointError::ScratchLimit
        );
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn retire_reuse_keeps_python_birth_order_for_center_owner_and_ball_order() {
        let mut forest = Forest::new();
        let first = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Contact(10),
            None,
        );
        let filler = add_member(
            &mut forest,
            [100.0, 0.0, 0.0],
            [100.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Shoot,
            Owner::Contact(99),
            None,
        );
        let older_at_joint = add_member(
            &mut forest,
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Shoot,
            Owner::Contact(11),
            None,
        );
        forest.retire(filler).unwrap();
        let later_reused = add_member(
            &mut forest,
            [0.0, 0.0, 10.0],
            [0.0, 10.0, 10.0],
            0.6,
            0.6,
            MemberKind::Strut,
            Owner::Contact(12),
            None,
        );
        assert_eq!(later_reused.slot(), filler.slot());
        assert_eq!(
            forest.live_members_in_creation_order(),
            vec![first, older_at_joint, later_reused]
        );
        assert_eq!(
            forest.live_members(),
            vec![first, later_reused, older_at_joint]
        );

        let cfg = config(-100.0);
        let plan = plan_joint_balls(&forest, cfg).unwrap();
        let joint = plan
            .validate(&forest, cfg)
            .unwrap()
            .iter()
            .find(|ball| ball.center_mm().map(Finite::get) == [0.0, 0.0, 10.0])
            .unwrap();
        assert_eq!(joint.owner_key(), later_reused);
        assert_eq!(joint.owner(), &Owner::Contact(12));
    }

    #[test]
    fn plan_digest_covers_config_ball_owner_key_generation_and_every_result_field() {
        let mut forest = Forest::new();
        let first = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        let old = add_member(
            &mut forest,
            [2.0, 0.0, 0.0],
            [2.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        forest.retire(old).unwrap();
        let reused = add_member(
            &mut forest,
            [3.0, 0.0, 0.0],
            [3.0, 0.0, 1.0],
            0.2,
            0.2,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        assert_eq!(old.slot(), reused.slot());
        assert_ne!(old.generation(), reused.generation());
        let ball = JointBall {
            center_mm: finite3([1.0, 2.0, 3.0]),
            radius_mm: radius(0.5),
            owner_key: old,
            owner: Owner::Contact(7),
        };
        let base = JointPlan {
            forest_digest: 5,
            live_view_digest: 6,
            config: config(-100.0).fingerprint(),
            balls: vec![ball],
            certified_dropped: 2,
            certification_resource_refused: 3,
        };
        let base_digest = base.digest();
        let mut variants = Vec::new();
        let mut changed = base.clone();
        changed.forest_digest += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.live_view_digest += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.plate_z ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.factor ^= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_depth -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_patches -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_work_items -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_scratch_bytes -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.config.max_certificate_bytes -= 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].center_mm[0] = Finite::new(1.5).unwrap();
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].radius_mm = radius(0.6);
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].owner_key = first;
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].owner_key = reused;
        variants.push(changed);
        let mut changed = base.clone();
        changed.balls[0].owner = Owner::Contact(8);
        variants.push(changed);
        let mut changed = base.clone();
        changed.certified_dropped += 1;
        variants.push(changed);
        let mut changed = base.clone();
        changed.certification_resource_refused += 1;
        variants.push(changed);
        assert!(variants
            .iter()
            .all(|variant| variant.digest() != base_digest));
    }

    fn dist_and_radius_scalar(center: [f64; 3], member: &LiveMember) -> (f64, f64) {
        let d = sub(member.p1, member.p0);
        let l2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let rel = sub(center, member.p0);
        if l2 < DEGENERATE_LENGTH_SQUARED {
            return (norm(rel), member.r0);
        }
        let t = ((rel[0] * d[0] + rel[1] * d[1] + rel[2] * d[2]) / l2).clamp(0.0, 1.0);
        let e = [
            center[0] - (member.p0[0] + t * d[0]),
            center[1] - (member.p0[1] + t * d[1]),
            center[2] - (member.p0[2] + t * d[2]),
        ];
        (
            (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt(),
            member.r0 + (member.r1 - member.r0) * t,
        )
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn cap_and_depth_boundary_keep_when_uncertified() {
        let mut forest = Forest::new();
        add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        add_member(
            &mut forest,
            [0.0, 0.0, 10.0],
            [10.0, 0.0, 10.0],
            0.6,
            0.6,
            MemberKind::Trunk,
            Owner::Contact(1),
            None,
        );
        let bounded = config(-100.0).with_limits(0, 20);
        let plan = plan_joint_balls(&forest, bounded).unwrap();
        assert!(plan
            .validate(&forest, bounded)
            .unwrap()
            .iter()
            .any(|b| b.center_mm().map(Finite::get) == [0.0, 0.0, 10.0]));

        let live_key = forest.live_members_in_creation_order()[0];
        let live = LiveMember::from_forest(live_key, forest.member(live_key).unwrap());
        let easy = CandidateBall {
            center: [0.0, 0.0, 5.0],
            radius: 0.01,
            owner_key: live_key,
            owner: Owner::Contact(0),
            touching: vec![0],
        };
        assert!(!certify_sphere(
            &easy,
            &[0],
            std::mem::size_of::<usize>(),
            std::slice::from_ref(&live),
            config(-100.0).with_limits(22, 19),
            CertificateScratch {
                max_bytes: usize::MAX,
            },
        ));
        let exact_certificate_peak = std::mem::size_of::<usize>()
            + 20 * (2 * std::mem::size_of::<Patch>() + 6 * std::mem::size_of::<f64>());
        let retained = 137;
        let fixed_certificate = CertificateScratch {
            max_bytes: exact_certificate_peak,
        };
        assert_eq!(
            certificate_parallel_window(
                retained,
                retained + 2 * exact_certificate_peak,
                exact_certificate_peak,
            )
            .unwrap(),
            2
        );
        assert_eq!(
            certificate_parallel_window(
                retained,
                retained + 2 * exact_certificate_peak - 1,
                exact_certificate_peak,
            )
            .unwrap(),
            1
        );
        assert_eq!(
            certificate_parallel_window(
                retained,
                retained + exact_certificate_peak - 1,
                exact_certificate_peak,
            )
            .unwrap(),
            0
        );
        assert_eq!(
            certificate_parallel_window(retained + 1, retained, exact_certificate_peak)
                .unwrap_err(),
            JointError::ScratchLimit
        );
        assert!(certify_sphere(
            &easy,
            &[0],
            std::mem::size_of::<usize>(),
            std::slice::from_ref(&live),
            config(-100.0).with_limits(22, 20),
            fixed_certificate,
        ));
        assert!(!certify_sphere(
            &easy,
            &[0],
            std::mem::size_of::<usize>(),
            std::slice::from_ref(&live),
            config(-100.0).with_limits(22, 20),
            CertificateScratch {
                max_bytes: exact_certificate_peak - 1,
            },
        ));
        assert!(matches!(
            plan_joint_balls(&forest, config(-100.0).with_limits(23, 20)),
            Err(JointError::InvalidLimits)
        ));
        assert!(matches!(
            plan_joint_balls(&forest, config(-100.0).with_limits(1, 0)),
            Err(JointError::InvalidLimits)
        ));
    }

    #[test]
    fn authority_binds_geometry_config_and_lifecycle() {
        let mut forest = Forest::new();
        let key = add_member(
            &mut forest,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            0.5,
            0.5,
            MemberKind::Trunk,
            Owner::Contact(0),
            None,
        );
        let cfg = config(-100.0);
        let plan = plan_joint_balls(&forest, cfg).unwrap();
        assert_eq!(
            plan.validate(&forest, config(-99.0)),
            Err(JointError::StaleAuthority)
        );
        forest.retire(key).unwrap();
        assert_eq!(plan.validate(&forest, cfg), Err(JointError::StaleAuthority));
    }

    #[test]
    fn full_plan_digest_is_identical_at_one_four_and_sixteen_threads() {
        const MARKER: &str = "PLINTH_JOINT_THREAD_CHILD";
        if std::env::var_os(MARKER).is_some() {
            let expected_threads = std::env::var("PLINTH_THREADS")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert_eq!(geom_kernel::current_threads(), expected_threads);
            let mut forest = Forest::new();
            for ordinal in 0..96_u32 {
                let x = f64::from(ordinal) * 25.0;
                add_member(
                    &mut forest,
                    [x, 0.0, 0.0],
                    [x, 0.0, 10.0],
                    0.6,
                    0.6,
                    MemberKind::Trunk,
                    Owner::Generated {
                        family: "joint-thread-fixture".into(),
                        ordinal: ordinal * 2,
                    },
                    None,
                );
                add_member(
                    &mut forest,
                    [x, 0.0, 10.0],
                    [x + 10.0, 0.0, 10.0],
                    0.6,
                    0.6,
                    MemberKind::Shoot,
                    Owner::Generated {
                        family: "joint-thread-fixture".into(),
                        ordinal: ordinal * 2 + 1,
                    },
                    None,
                );
            }
            let mut cfg = config(-100.0);
            // Deliberately between the old 512 MiB / 16 and / 4 thread-derived ranges. Proof
            // authority is now this fixed per-certificate policy, never the worker count.
            cfg.max_certificate_bytes = 48 * 1024 * 1024;
            let plan = plan_joint_balls(&forest, cfg).unwrap();
            assert!(plan.validate(&forest, cfg).unwrap().len() >= 96);
            println!("JOINT_PLAN={:016x}", plan.digest());
            return;
        }
        let test_name =
            "joints::tests::full_plan_digest_is_identical_at_one_four_and_sixteen_threads";
        let outputs = ["1", "4", "16"].map(|threads| {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test_name, "--nocapture"])
                .env(MARKER, "1")
                .env("PLINTH_THREADS", threads)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find(|line| line.starts_with("JOINT_PLAN="))
                .unwrap()
                .to_owned()
        });
        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn unavailable_aggregate_certificate_reservation_keeps_every_ball_at_all_thread_counts() {
        const MARKER: &str = "PLINTH_JOINT_RESOURCE_REFUSAL_CHILD";
        if std::env::var_os(MARKER).is_some() {
            let expected_threads = std::env::var("PLINTH_THREADS")
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert_eq!(geom_kernel::current_threads(), expected_threads);
            let mut forest = Forest::new();
            for ordinal in 0..24_u32 {
                let x = f64::from(ordinal) * 25.0;
                add_member(
                    &mut forest,
                    [x, 0.0, 0.0],
                    [x, 0.0, 10.0],
                    0.6,
                    0.6,
                    MemberKind::Trunk,
                    Owner::Generated {
                        family: "joint-resource-fixture".into(),
                        ordinal: ordinal * 2,
                    },
                    None,
                );
                add_member(
                    &mut forest,
                    [x, 0.0, 10.0],
                    [x + 10.0, 0.0, 10.0],
                    0.6,
                    0.6,
                    MemberKind::Shoot,
                    Owner::Generated {
                        family: "joint-resource-fixture".into(),
                        ordinal: ordinal * 2 + 1,
                    },
                    None,
                );
            }
            let mut cfg = config(-100.0);
            cfg.max_certificate_bytes = 48 * 1024 * 1024;
            let view =
                FilteredLiveView::current(&forest, &[], LiveViewLimits::new(48, 1, 1024 * 1024))
                    .unwrap();
            let plan = plan_joint_balls_filtered(
                &forest,
                &view,
                cfg,
                JointStagingLimits::new(48, 96, 10_000, 10_000, 96, 48, 1024 * 1024),
            )
            .unwrap();
            let kept = plan.validate_filtered(&forest, &view, cfg).unwrap().len();
            assert!(kept > 0);
            assert_eq!(plan.certified_dropped(), 0);
            assert_eq!(plan.certification_resource_refused(), kept);
            println!(
                "JOINT_RESOURCE_REFUSAL={:016x}:{kept}:{}",
                plan.digest(),
                plan.certification_resource_refused()
            );
            return;
        }
        let test_name = "joints::tests::unavailable_aggregate_certificate_reservation_keeps_every_ball_at_all_thread_counts";
        let outputs = ["1", "4", "16"].map(|threads| {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test_name, "--nocapture"])
                .env(MARKER, "1")
                .env("PLINTH_THREADS", threads)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find(|line| line.starts_with("JOINT_RESOURCE_REFUSAL="))
                .unwrap()
                .to_owned()
        });
        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
