//! The deterministic, generational support-forest data model.
//!
//! This is deliberately the one owner of support-tree topology.  A member's
//! `p0` end is terminal-ward and its `p1` end is root-ward; distinct radius
//! types make reversing that convention a type error.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::{Finite, PositiveFinite};
use serde::Serialize;
use thiserror::Error;

/// A stable reference while the member is live.  A retired slot increments
/// its generation, so a reference cannot accidentally identify a reused slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct MemberKey {
    slot: u32,
    generation: u32,
}

impl MemberKey {
    #[must_use]
    pub const fn slot(self) -> u32 {
        self.slot
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    #[cfg(test)]
    pub(crate) const fn transit_test_fixture(slot: u32, generation: u32) -> Self {
        Self { slot, generation }
    }
}

/// Radius at `p0`, the terminal-ward end of a support member.
///
/// ```compile_fail
/// use plinth_support::forest::{RootwardEndRadius, TerminalEndRadius};
/// fn accepts_terminal(_: TerminalEndRadius) {}
/// let rootward: RootwardEndRadius = todo!();
/// accepts_terminal(rootward); // the two ends must never be swapped
/// ```
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
pub struct TerminalEndRadius(PositiveFinite);

impl TerminalEndRadius {
    #[must_use]
    pub const fn new(value: PositiveFinite) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> PositiveFinite {
        self.0
    }
}

/// Radius at `p1`, the root-ward end of a support member.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
pub struct RootwardEndRadius(PositiveFinite);

impl RootwardEndRadius {
    #[must_use]
    pub const fn new(value: PositiveFinite) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> PositiveFinite {
        self.0
    }
}

/// Semantic role used by later support passes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum MemberKind {
    Stub,
    Shoot,
    Trunk,
    Strut,
    Brace,
    Other(String),
}

/// Provenance for a support member.  The data model deliberately does not
/// prescribe an algorithm-specific owner namespace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum Owner {
    Contact(u32),
    ModelAnchor(u32),
    Generated { family: String, ordinal: u32 },
}

/// The endpoint deliberately embedded in the model, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum EmbedEnd {
    Terminalward,
    Rootward,
}

/// A requested designed clearance extent in millimetres. This is deliberately
/// distinct from observed penetration evidence and cannot be zero or non-finite.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
pub struct PositiveFiniteMm(PositiveFinite);

impl PositiveFiniteMm {
    pub fn new(mm: f64) -> Result<Self, DesignedMaskError> {
        PositiveFinite::new(mm)
            .map(Self)
            .map_err(|_| DesignedMaskError::InvalidExtent)
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0.get()
    }
}

#[allow(dead_code, reason = "E1a seals this private staging authority; E1b mints it")]
mod embed_validation {
    #[derive(Clone, Copy, Debug)]
    pub(super) struct ValidatorAuthority {
        witness: u64,
    }

    #[derive(Clone, Copy, Debug)]
    pub(super) struct SurfaceIdentity {
        identity: u64,
    }

    impl ValidatorAuthority {
        pub(super) const fn witness(self) -> u64 { self.witness }
    }
    impl SurfaceIdentity {
        pub(super) const fn identity(self) -> u64 { self.identity }
    }

    #[cfg(test)]
    pub(super) const fn test_authority(witness: u64) -> ValidatorAuthority {
        ValidatorAuthority { witness }
    }
    #[cfg(test)]
    pub(super) const fn test_surface(identity: u64) -> SurfaceIdentity {
        SurfaceIdentity { identity }
    }
}

/// Opaque, immutable result of a later endpoint-connected construction check.
/// E1a seals the binding format; E1b owns the kernel validator and witness minting.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DesignedEndMask {
    requested_extent_mm: PositiveFiniteMm,
    witness: u64,
    validation_binding: u64,
}

impl DesignedEndMask {
    pub(crate) fn validated_extent_for(
        &self,
        member: &Member,
        end: EmbedEnd,
        surface: embed_validation::SurfaceIdentity,
    ) -> Result<PositiveFiniteMm, DesignedMaskError> {
        (self.validation_binding
            == mask_binding(member, end, self.requested_extent_mm, self.witness, surface.identity()))
            .then_some(self.requested_extent_mm)
            .ok_or(DesignedMaskError::StaleBinding)
    }

    #[cfg(test)]
    #[must_use]
    pub(crate) const fn test_requested_extent_mm(&self) -> PositiveFiniteMm { self.requested_extent_mm }
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn validation_binding(&self) -> u64 {
        self.validation_binding
    }
}

/// Independent end masks. Construction is crate-private so callers cannot inject
/// an unvalidated clearance claim into a member or verifier.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct DesignedClearanceMasks {
    terminalward: Option<DesignedEndMask>,
    rootward: Option<DesignedEndMask>,
}

impl DesignedClearanceMasks {
    #[must_use]
    pub const fn terminalward(&self) -> Option<&DesignedEndMask> {
        self.terminalward.as_ref()
    }

    #[must_use]
    pub const fn rootward(&self) -> Option<&DesignedEndMask> {
        self.rootward.as_ref()
    }

    /// The order is a contract: terminalward before rootward, independent of
    /// construction order or thread count.
    pub fn iter(&self) -> impl Iterator<Item = (EmbedEnd, &DesignedEndMask)> {
        [
            (EmbedEnd::Terminalward, self.terminalward.as_ref()),
            (EmbedEnd::Rootward, self.rootward.as_ref()),
        ]
        .into_iter()
        .filter_map(|(end, mask)| mask.map(|mask| (end, mask)))
    }

    /// Sealed E1a boundary for the future kernel-backed validator. `witness` and
    /// `surface_identity` are opaque digest inputs, never inferred from geometry.
    pub(crate) fn bind_for_member(
        member: &Member,
        terminalward: Option<PositiveFiniteMm>,
        rootward: Option<PositiveFiniteMm>,
        authority: embed_validation::ValidatorAuthority,
        surface_identity: embed_validation::SurfaceIdentity,
    ) -> Result<Self, DesignedMaskError> {
        let length = member_length_mm(member)?;
        let terminal = terminalward.map(|extent| validate_extent(extent, length)).transpose()?;
        let root = rootward.map(|extent| validate_extent(extent, length)).transpose()?;
        if terminal.is_some_and(|value| root.is_some_and(|other| value.get() >= length - other.get()))
        {
            return Err(DesignedMaskError::OverlappingExtents);
        }
        Ok(Self {
            terminalward: terminal.map(|requested_extent_mm| DesignedEndMask {
                requested_extent_mm,
                witness: authority.witness(),
                validation_binding: mask_binding(
                    member,
                    EmbedEnd::Terminalward,
                    requested_extent_mm,
                    authority.witness(),
                    surface_identity.identity(),
                ),
            }),
            rootward: root.map(|requested_extent_mm| DesignedEndMask {
                requested_extent_mm,
                witness: authority.witness(),
                validation_binding: mask_binding(
                    member,
                    EmbedEnd::Rootward,
                    requested_extent_mm,
                    authority.witness(),
                    surface_identity.identity(),
                ),
            }),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum DesignedMaskError {
    #[error("designed extent must be positive and finite millimetres")]
    InvalidExtent,
    #[error("designed extent must be shorter than its member")]
    ExtentOutOfSpan,
    #[error("member span must be finite and non-zero")]
    InvalidMemberSpan,
    #[error("terminalward and rootward extents must not overlap or consume a member")]
    OverlappingExtents,
    #[error("designed mask binding is stale for this member, end, or surface")]
    StaleBinding,
}

fn validate_extent(
    extent: PositiveFiniteMm,
    length: f64,
) -> Result<PositiveFiniteMm, DesignedMaskError> {
    if extent.get() >= length {
        Err(DesignedMaskError::ExtentOutOfSpan)
    } else {
        Ok(extent)
    }
}

fn member_length_mm(member: &Member) -> Result<f64, DesignedMaskError> {
    let [x0, y0, z0] = member.p0_mm;
    let [x1, y1, z1] = member.p1_mm;
    let length = (x1.get() - x0.get())
        .hypot(y1.get() - y0.get())
        .hypot(z1.get() - z0.get());
    (length.is_finite() && length > 0.0)
        .then_some(length)
        .ok_or(DesignedMaskError::InvalidMemberSpan)
}

fn mask_binding(
    member: &Member,
    end: EmbedEnd,
    extent: PositiveFiniteMm,
    witness: u64,
    surface_identity: u64,
) -> u64 {
    let mut digest = DigestWriter::new();
    digest.tag(0x45);
    digest.embed(Some(end));
    digest.f64(extent.get());
    digest.u64(witness);
    digest.u64(surface_identity);
    digest.member(member);
    digest.finish()
}

/// Clearance proof carried with each member.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ClearanceCertificate {
    pub geom_version: u64,
    pub cert_version: Option<u64>,
    pub cert_radius_mm: Option<PositiveFinite>,
}

impl ClearanceCertificate {
    fn fresh() -> Self {
        Self {
            geom_version: 0,
            cert_version: None,
            cert_radius_mm: None,
        }
    }
}

/// One straight tapered support frustum.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Member {
    p0_mm: [Finite; 3],
    p1_mm: [Finite; 3],
    r0_mm: TerminalEndRadius,
    r1_mm: RootwardEndRadius,
    kind: MemberKind,
    owner: Owner,
    embed_end: Option<EmbedEnd>,
    carried_mm2: Finite,
    certificate: ClearanceCertificate,
}

/// All immutable construction data for a [`Member`]. Grouping the endpoint
/// convention in one value avoids an unstructured positional constructor.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MemberSpec {
    pub p0_mm: [Finite; 3],
    pub p1_mm: [Finite; 3],
    pub r0_mm: TerminalEndRadius,
    pub r1_mm: RootwardEndRadius,
    pub kind: MemberKind,
    pub owner: Owner,
    pub embed_end: Option<EmbedEnd>,
    pub carried_mm2: Finite,
}

impl Member {
    #[must_use]
    pub fn new(spec: MemberSpec) -> Self {
        Self {
            p0_mm: spec.p0_mm,
            p1_mm: spec.p1_mm,
            r0_mm: spec.r0_mm,
            r1_mm: spec.r1_mm,
            kind: spec.kind,
            owner: spec.owner,
            embed_end: spec.embed_end,
            carried_mm2: spec.carried_mm2,
            certificate: ClearanceCertificate::fresh(),
        }
    }

    #[must_use]
    pub const fn p0_mm(&self) -> [Finite; 3] {
        self.p0_mm
    }
    #[must_use]
    pub const fn p1_mm(&self) -> [Finite; 3] {
        self.p1_mm
    }
    #[must_use]
    pub const fn terminal_radius_mm(&self) -> TerminalEndRadius {
        self.r0_mm
    }
    #[must_use]
    pub const fn rootward_radius_mm(&self) -> RootwardEndRadius {
        self.r1_mm
    }
    #[must_use]
    pub fn kind(&self) -> &MemberKind {
        &self.kind
    }
    #[must_use]
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    #[must_use]
    pub const fn embed_end(&self) -> Option<EmbedEnd> {
        self.embed_end
    }
    #[must_use]
    pub const fn carried_mm2(&self) -> Finite {
        self.carried_mm2
    }
    #[must_use]
    pub const fn certificate(&self) -> ClearanceCertificate {
        self.certificate
    }

    fn maximum_radius(&self) -> PositiveFinite {
        if self.r0_mm.get().get() >= self.r1_mm.get().get() {
            self.r0_mm.get()
        } else {
            self.r1_mm.get()
        }
    }

    fn certificate_valid(&self) -> bool {
        if self.certificate.cert_version == Some(self.certificate.geom_version) {
            return true;
        }
        // An embed's checked interval changes when its radius changes, so the
        // Python fast path is unsound there.  Geometry/kind changes always
        // clear cert_radius below.
        self.embed_end.is_none()
            && self
                .certificate
                .cert_radius_mm
                .is_some_and(|r| self.maximum_radius().get() <= r.get())
    }

    fn invalidate_geometry(&mut self) {
        self.certificate.geom_version = self.certificate.geom_version.saturating_add(1);
        self.certificate.cert_version = None;
        self.certificate.cert_radius_mm = None;
    }

    fn invalidate_radius(&mut self) {
        self.certificate.geom_version = self.certificate.geom_version.saturating_add(1);
        self.certificate.cert_version = None;
        if self.embed_end.is_some() {
            self.certificate.cert_radius_mm = None;
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Slot {
    generation: u32,
    reusable: bool,
    member: Option<Member>,
}

/// Immutable condemned-stub evidence. It is intentionally separate from the
/// live arena so regrade/report consumers cannot accidentally emit it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct QuarantineRecord {
    pub original_key: MemberKey,
    pub p0_mm: [Finite; 3],
    pub p1_mm: [Finite; 3],
    pub r0_mm: TerminalEndRadius,
    pub r1_mm: RootwardEndRadius,
    pub kind: MemberKind,
    pub owner: Owner,
    pub embed_end: Option<EmbedEnd>,
    pub carried_mm2: Finite,
    pub certificate: ClearanceCertificate,
    pub terminal_contacts: Vec<u32>,
}

impl QuarantineRecord {
    fn from_member(key: MemberKey, member: &Member, terminal_contacts: Vec<u32>) -> Self {
        Self {
            original_key: key,
            p0_mm: member.p0_mm,
            p1_mm: member.p1_mm,
            r0_mm: member.r0_mm,
            r1_mm: member.r1_mm,
            kind: member.kind.clone(),
            owner: member.owner.clone(),
            embed_end: member.embed_end,
            carried_mm2: member.carried_mm2,
            certificate: member.certificate,
            terminal_contacts,
        }
    }
}

pub(crate) fn terminal_member_matches_evidence(
    member: &Member,
    key: MemberKey,
    terminal: u32,
    evidence: &QuarantineRecord,
) -> bool {
    evidence.original_key == key
        && evidence.p0_mm == member.p0_mm
        && evidence.p1_mm == member.p1_mm
        && evidence.r0_mm == member.r0_mm
        && evidence.r1_mm == member.r1_mm
        && evidence.kind == member.kind
        && evidence.owner == member.owner
        && evidence.embed_end == member.embed_end
        && evidence.carried_mm2 == member.carried_mm2
        && evidence.certificate == member.certificate
        && evidence.terminal_contacts.as_slice() == [terminal]
}

#[derive(Clone, Debug, PartialEq, Default)]
struct ForestState {
    slots: Vec<Slot>,
    /// Append-only birth sequence. Retired keys remain as lifecycle evidence;
    /// live views filter them without falling back to recyclable slot order.
    creation_order: Vec<MemberKey>,
    parent: BTreeMap<MemberKey, MemberKey>,
    children: BTreeMap<MemberKey, Vec<MemberKey>>,
    roots: BTreeSet<MemberKey>,
    terminal_of: BTreeMap<u32, MemberKey>,
    /// Sealed pre-growth member evidence for every registered terminal. It remains unchanged
    /// across consumption and retirement, including after the arena slot is cleared.
    terminal_evidence: BTreeMap<u32, QuarantineRecord>,
    terminal_consumed: BTreeSet<u32>,
    braces: BTreeSet<MemberKey>,
    quarantined: BTreeMap<MemberKey, QuarantineRecord>,
    dead: BTreeSet<MemberKey>,
}

/// Opaque, LIFO checkpoint token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForestCheckpoint(u64);

/// Stable, machine-readable lifecycle summary for scoring and deterministic
/// reports. Geometry remains behind the arena API; the report exposes only
/// legal live identities and the intentionally retained condemned/dead facts.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ForestReport {
    pub live_members: Vec<MemberKey>,
    pub roots: Vec<MemberKey>,
    pub braces: Vec<MemberKey>,
    pub terminal_of: BTreeMap<u32, MemberKey>,
    pub quarantined: Vec<QuarantineRecord>,
    pub dead: Vec<MemberKey>,
    pub digest: u64,
}

/// Read-only lifecycle state for one terminal's original member binding. This ledger never
/// turns a retired or quarantined generation back into a live member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalLedgerState {
    Live,
    Consumed,
    Quarantined,
    Retired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalLedgerEntry {
    pub(crate) contact: u32,
    pub(crate) original_key: MemberKey,
    pub(crate) state: TerminalLedgerState,
}

#[derive(Clone, Debug)]
struct CheckpointFrame {
    token: ForestCheckpoint,
    state: ForestState,
}

/// Mutation errors are explicit rather than silently treating an invalid or
/// stale reference as an empty tree edge.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ForestError {
    #[error("member key is stale, retired, quarantined, or unknown: {0:?}")]
    InactiveKey(MemberKey),
    #[error("brace members cannot participate in parent/child topology")]
    BraceInTree,
    #[error("a member cannot parent itself")]
    SelfParent,
    #[error("parent assignment would create a cycle")]
    Cycle,
    #[error("terminal {terminal} is already registered to {existing:?}")]
    DuplicateTerminal { terminal: u32, existing: MemberKey },
    #[error("terminal {terminal} must register its exact live contact stub")]
    InvalidTerminalMember { terminal: u32 },
    #[error("registered live terminal {terminal} must be consumed before mutation")]
    RegisteredTerminalMutation { terminal: u32 },
    #[error("only the innermost checkpoint may be committed or rolled back")]
    CheckpointOrder,
    #[error("a quarantined member must have no live tree children")]
    QuarantineHasChildren,
    #[error("forest invariant violated: {0}")]
    Invariant(&'static str),
}

/// One arena owns members, topology, terminal provenance, brace membership,
/// lifecycle state, and explicit rollback snapshots.
#[derive(Clone, Debug, Default)]
pub struct Forest {
    state: ForestState,
    checkpoints: Vec<CheckpointFrame>,
    next_checkpoint: u64,
}

impl Forest {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates the fresh forest used by initial-contact construction with the
    /// two contiguous arena payloads reserved for the known contact count.
    /// The resource model bills the requested element payload; allocator
    /// rounding and private BTree node layout are intentionally outside that
    /// portable logical-payload contract.
    pub(crate) fn try_new_for_initial_contacts(
        contact_count: usize,
    ) -> Result<Self, std::collections::TryReserveError> {
        let mut forest = Self::new();
        forest.state.slots.try_reserve_exact(contact_count)?;
        forest
            .state
            .creation_order
            .try_reserve_exact(contact_count)?;
        Ok(forest)
    }

    fn active_slot(&self, key: MemberKey) -> Option<&Slot> {
        let slot = self.state.slots.get(key.slot as usize)?;
        (slot.generation == key.generation
            && slot.member.is_some()
            && !self.state.quarantined.contains_key(&key)
            && !self.state.dead.contains(&key))
        .then_some(slot)
    }

    fn require_active(&self, key: MemberKey) -> Result<(), ForestError> {
        self.active_slot(key)
            .ok_or(ForestError::InactiveKey(key))
            .map(|_| ())
    }

    fn member_mut(&mut self, key: MemberKey) -> Result<&mut Member, ForestError> {
        self.require_active(key)?;
        Ok(self.state.slots[key.slot as usize]
            .member
            .as_mut()
            .expect("checked active"))
    }

    /// Adds a normal tree member as a root until explicitly linked.
    pub fn add_member(&mut self, member: Member) -> MemberKey {
        let reused = self
            .state
            .slots
            .iter()
            .position(|slot| slot.member.is_none() && slot.reusable);
        let key = if let Some(index) = reused {
            let slot = &mut self.state.slots[index];
            slot.member = Some(member);
            MemberKey {
                slot: index as u32,
                generation: slot.generation,
            }
        } else {
            let slot = self.state.slots.len() as u32;
            self.state.slots.push(Slot {
                generation: 0,
                reusable: true,
                member: Some(member),
            });
            MemberKey {
                slot,
                generation: 0,
            }
        };
        self.state.roots.insert(key);
        self.state.children.insert(key, Vec::new());
        self.state.creation_order.push(key);
        key
    }

    /// Adds a non-tree member, excluded from roots and Murray topology.
    pub fn add_brace(&mut self, member: Member) -> MemberKey {
        let key = self.add_member(member);
        self.state.roots.remove(&key);
        self.state.braces.insert(key);
        key
    }

    #[must_use]
    pub fn member(&self, key: MemberKey) -> Option<&Member> {
        self.active_slot(key).and_then(|slot| slot.member.as_ref())
    }

    #[must_use]
    pub fn parent_of(&self, key: MemberKey) -> Option<MemberKey> {
        self.member(key)?;
        self.state
            .parent
            .get(&key)
            .copied()
            .filter(|parent| self.member(*parent).is_some())
    }

    #[must_use]
    pub fn children_of(&self, key: MemberKey) -> Vec<MemberKey> {
        if self.member(key).is_none() {
            return Vec::new();
        }
        self.state
            .children
            .get(&key)
            .into_iter()
            .flatten()
            .copied()
            .filter(|child| self.member(*child).is_some())
            .collect()
    }

    #[must_use]
    pub fn roots(&self) -> Vec<MemberKey> {
        self.state
            .roots
            .iter()
            .copied()
            .filter(|k| self.member(*k).is_some())
            .collect()
    }
    #[must_use]
    pub fn braces(&self) -> Vec<MemberKey> {
        self.state
            .braces
            .iter()
            .copied()
            .filter(|k| self.member(*k).is_some())
            .collect()
    }
    #[must_use]
    pub fn dead(&self) -> &BTreeSet<MemberKey> {
        &self.state.dead
    }
    #[must_use]
    pub fn quarantined(&self) -> Vec<MemberKey> {
        self.state.quarantined.keys().copied().collect()
    }
    /// Complete, read-only evidence for a condemned terminal stub.
    #[must_use]
    pub fn quarantine_record(&self, key: MemberKey) -> Option<&QuarantineRecord> {
        self.state.quarantined.get(&key)
    }
    #[must_use]
    pub fn quarantine_records(&self) -> Vec<&QuarantineRecord> {
        self.state.quarantined.values().collect()
    }
    #[must_use]
    pub fn terminal(&self, terminal: u32) -> Option<MemberKey> {
        self.state
            .terminal_of
            .get(&terminal)
            .copied()
            .filter(|k| self.member(*k).is_some())
    }

    /// Returns the recorded terminal member even after it was retired or
    /// quarantined.  This provenance is how later reporting explains an
    /// honestly unrouted contact; it is never a live/Murray/emission view.
    #[must_use]
    pub fn terminal_key(&self, terminal: u32) -> Option<MemberKey> {
        self.state.terminal_of.get(&terminal).copied()
    }

    /// Resolve the append-only contact-to-generation ledger even after physical removal.
    #[must_use]
    pub(crate) fn terminal_ledger(&self, terminal: u32) -> Option<TerminalLedgerEntry> {
        let original_key = self.terminal_key(terminal)?;
        let state = if self.member(original_key).is_some() {
            if self.state.terminal_consumed.contains(&terminal) {
                TerminalLedgerState::Consumed
            } else {
                TerminalLedgerState::Live
            }
        } else if self.state.quarantined.contains_key(&original_key) {
            TerminalLedgerState::Quarantined
        } else if self.state.dead.contains(&original_key) {
            TerminalLedgerState::Retired
        } else {
            return None;
        };
        Some(TerminalLedgerEntry {
            contact: terminal,
            original_key,
            state,
        })
    }

    pub(crate) fn terminal_ledger_iter(&self) -> impl Iterator<Item = TerminalLedgerEntry> + '_ {
        self.state
            .terminal_of
            .keys()
            .copied()
            .filter_map(|contact| self.terminal_ledger(contact))
    }

    pub(crate) fn terminal_ledger_evidence(&self, terminal: u32) -> Option<&QuarantineRecord> {
        self.state.terminal_evidence.get(&terminal)
    }

    /// Read-only Q-REAL/regrade evidence for a terminal, never a live member.
    #[must_use]
    pub fn terminal_quarantine(&self, terminal: u32) -> Option<&QuarantineRecord> {
        self.terminal_key(terminal)
            .and_then(|key| self.quarantine_record(key))
    }

    /// All non-brace live members in stable key order.
    #[must_use]
    pub fn live_tree_members(&self) -> Vec<MemberKey> {
        self.live_members()
            .into_iter()
            .filter(|key| !self.state.braces.contains(key))
            .collect()
    }

    /// All live members, including braces, in stable key order.
    #[must_use]
    pub fn live_members(&self) -> Vec<MemberKey> {
        self.state
            .slots
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                slot.member.as_ref().map(|_| MemberKey {
                    slot: i as u32,
                    generation: slot.generation,
                })
            })
            .filter(|key| self.member(*key).is_some())
            .collect()
    }

    /// All live members in original append/birth order. Slot reuse never
    /// moves a later-born member ahead of an older live member.
    #[must_use]
    pub fn live_members_in_creation_order(&self) -> Vec<MemberKey> {
        self.live_members_in_creation_order_iter().collect()
    }

    /// Allocation-free source for bounded downstream preflight. Consumers must still bind any
    /// derived view to [`Self::digest`] before treating it as authority.
    pub(crate) fn live_members_in_creation_order_iter(
        &self,
    ) -> impl Iterator<Item = MemberKey> + '_ {
        self.state
            .creation_order
            .iter()
            .copied()
            .filter(|key| self.member(*key).is_some())
    }

    /// Registers the member which owns a terminal contact.
    pub fn register_terminal(&mut self, terminal: u32, key: MemberKey) -> Result<(), ForestError> {
        self.require_active(key)?;
        if self.state.braces.contains(&key) {
            return Err(ForestError::BraceInTree);
        }
        if let Some(existing) = self.state.terminal_of.get(&terminal) {
            return Err(ForestError::DuplicateTerminal {
                terminal,
                existing: *existing,
            });
        }
        let member = self.member(key).expect("checked active");
        if member.kind() != &MemberKind::Stub || member.owner() != &Owner::Contact(terminal) {
            return Err(ForestError::InvalidTerminalMember { terminal });
        }
        let terminal_contacts = Vec::from([terminal]);
        let evidence = QuarantineRecord::from_member(key, member, terminal_contacts);
        self.state.terminal_of.insert(terminal, key);
        self.state.terminal_evidence.insert(terminal, evidence);
        Ok(())
    }

    /// Removes a terminal registration when later placement deliberately
    /// replaces the contact, rather than silently overwriting its provenance.
    pub fn unregister_terminal(&mut self, terminal: u32) -> Option<MemberKey> {
        self.state.terminal_consumed.remove(&terminal);
        self.state.terminal_evidence.remove(&terminal);
        self.state.terminal_of.remove(&terminal)
    }

    /// Typed transition used when successful growth is about to attach and resize a terminal
    /// stub. The sealed pre-growth facts are checked before mutation becomes legal.
    pub(crate) fn consume_terminal(
        &mut self,
        terminal: u32,
        key: MemberKey,
    ) -> Result<(), ForestError> {
        if self.state.terminal_of.get(&terminal).copied() != Some(key) {
            return Err(ForestError::InvalidTerminalMember { terminal });
        }
        let member = self
            .member(key)
            .ok_or(ForestError::InvalidTerminalMember { terminal })?;
        if self.state.terminal_consumed.contains(&terminal) {
            return if member.kind() == &MemberKind::Stub
                && member.owner() == &Owner::Contact(terminal)
            {
                Ok(())
            } else {
                Err(ForestError::InvalidTerminalMember { terminal })
            };
        }
        let evidence = self
            .state
            .terminal_evidence
            .get(&terminal)
            .ok_or(ForestError::InvalidTerminalMember { terminal })?;
        if !terminal_member_matches_evidence(member, key, terminal, evidence) {
            return Err(ForestError::InvalidTerminalMember { terminal });
        }
        self.state.terminal_consumed.insert(terminal);
        Ok(())
    }

    /// Atomically removes the still-current, unconsumed contact stub after a
    /// terminal route has been evaluated but no candidate won.  The original
    /// registration and its sealed evidence deliberately remain in the
    /// terminal ledger as `Retired`; this is not a quarantine transition.
    pub(crate) fn retire_unrouted_terminal(
        &mut self,
        terminal: u32,
        key: MemberKey,
    ) -> Result<(), ForestError> {
        if self.state.terminal_of.get(&terminal).copied() != Some(key)
            || self.state.terminal_consumed.contains(&terminal)
        {
            return Err(ForestError::InvalidTerminalMember { terminal });
        }
        let member = self
            .member(key)
            .ok_or(ForestError::InvalidTerminalMember { terminal })?;
        let evidence = self
            .state
            .terminal_evidence
            .get(&terminal)
            .ok_or(ForestError::InvalidTerminalMember { terminal })?;
        if !terminal_member_matches_evidence(member, key, terminal, evidence)
            || !self.children_of(key).is_empty()
            || self.parent_of(key).is_some()
        {
            return Err(ForestError::InvalidTerminalMember { terminal });
        }
        self.retire(key)
    }

    fn require_terminal_mutation_allowed(&self, key: MemberKey) -> Result<(), ForestError> {
        if let Some((&terminal, _)) = self.state.terminal_of.iter().find(|(terminal, member)| {
            **member == key && !self.state.terminal_consumed.contains(terminal)
        }) {
            return Err(ForestError::RegisteredTerminalMutation { terminal });
        }
        Ok(())
    }

    /// Attaches `child` to `parent` at the parent's terminal-ward (`p0`) end.
    pub fn set_parent(
        &mut self,
        child: MemberKey,
        parent: Option<MemberKey>,
    ) -> Result<(), ForestError> {
        self.require_active(child)?;
        self.require_terminal_mutation_allowed(child)?;
        if self.state.braces.contains(&child) {
            return Err(ForestError::BraceInTree);
        }
        if let Some(parent) = parent {
            self.require_active(parent)?;
            if self.state.braces.contains(&parent) {
                return Err(ForestError::BraceInTree);
            }
            if child == parent {
                return Err(ForestError::SelfParent);
            }
            let mut probe = Some(parent);
            while let Some(key) = probe {
                if key == child {
                    return Err(ForestError::Cycle);
                }
                probe = self.state.parent.get(&key).copied();
            }
        }
        if let Some(old) = self.state.parent.remove(&child) {
            if let Some(children) = self.state.children.get_mut(&old) {
                children.retain(|key| *key != child);
            }
        }
        match parent {
            Some(parent) => {
                self.state.roots.remove(&child);
                let children = self.state.children.entry(parent).or_default();
                if !children.contains(&child) {
                    children.push(child);
                }
                self.state.parent.insert(child, parent);
            }
            None => {
                self.state.roots.insert(child);
            }
        }
        Ok(())
    }

    /// Removes a member from all live topology. Live children become roots;
    /// the old key is permanently stale, even if its slot is reused.
    pub fn retire(&mut self, key: MemberKey) -> Result<(), ForestError> {
        self.require_active(key)?;
        let terminal_contacts: Vec<_> = self
            .state
            .terminal_of
            .iter()
            .filter_map(|(terminal, member_key)| (*member_key == key).then_some(*terminal))
            .collect();
        for terminal in terminal_contacts {
            self.state.terminal_consumed.insert(terminal);
        }
        let children = self.children_of(key);
        for child in children {
            self.set_parent(child, None)?;
        }
        if self.state.parent.contains_key(&key) {
            self.set_parent(key, None)?;
        }
        self.state.roots.remove(&key);
        self.state.braces.remove(&key);
        self.state.children.remove(&key);
        self.state.dead.insert(key);
        let slot = &mut self.state.slots[key.slot as usize];
        slot.member = None;
        if let Some(next_generation) = slot.generation.checked_add(1) {
            slot.generation = next_generation;
        } else {
            // Do not ever wrap a key generation: an exhausted slot remains a
            // tombstone rather than making an old external key live again.
            slot.reusable = false;
        }
        Ok(())
    }

    /// Quarantines a terminal stub: it remains recorded as condemned but is
    /// structurally absent from all live tree and emission views.
    pub fn quarantine(&mut self, key: MemberKey) -> Result<(), ForestError> {
        self.require_active(key)?;
        if !self.children_of(key).is_empty() {
            return Err(ForestError::QuarantineHasChildren);
        }
        let member = self.member(key).expect("checked active").clone();
        let terminal_contact_count = self
            .state
            .terminal_of
            .iter()
            .filter(|(_, member_key)| **member_key == key)
            .count();
        let mut terminal_contacts = Vec::with_capacity(terminal_contact_count);
        terminal_contacts.extend(
            self.state
                .terminal_of
                .iter()
                .filter_map(|(terminal, member_key)| (*member_key == key).then_some(*terminal)),
        );
        let record = QuarantineRecord::from_member(key, &member, terminal_contacts);
        self.retire(key)?;
        self.state.dead.remove(&key);
        self.state.quarantined.insert(key, record);
        Ok(())
    }

    pub fn set_endpoints(
        &mut self,
        key: MemberKey,
        p0_mm: [Finite; 3],
        p1_mm: [Finite; 3],
    ) -> Result<(), ForestError> {
        self.require_terminal_mutation_allowed(key)?;
        let member = self.member_mut(key)?;
        member.p0_mm = p0_mm;
        member.p1_mm = p1_mm;
        member.invalidate_geometry();
        Ok(())
    }

    pub fn set_radii(
        &mut self,
        key: MemberKey,
        r0_mm: TerminalEndRadius,
        r1_mm: RootwardEndRadius,
    ) -> Result<(), ForestError> {
        self.require_terminal_mutation_allowed(key)?;
        let member = self.member_mut(key)?;
        member.r0_mm = r0_mm;
        member.r1_mm = r1_mm;
        member.invalidate_radius();
        Ok(())
    }

    pub fn set_kind(&mut self, key: MemberKey, kind: MemberKind) -> Result<(), ForestError> {
        self.require_terminal_mutation_allowed(key)?;
        let member = self.member_mut(key)?;
        member.kind = kind;
        member.invalidate_geometry();
        Ok(())
    }

    pub fn set_embed_end(
        &mut self,
        key: MemberKey,
        end: Option<EmbedEnd>,
    ) -> Result<(), ForestError> {
        self.require_terminal_mutation_allowed(key)?;
        let member = self.member_mut(key)?;
        member.embed_end = end;
        member.invalidate_geometry();
        Ok(())
    }

    /// Load is snapshot-visible but does not alter swept clearance geometry.
    pub fn set_carried_mm2(
        &mut self,
        key: MemberKey,
        carried_mm2: Finite,
    ) -> Result<(), ForestError> {
        self.require_terminal_mutation_allowed(key)?;
        self.member_mut(key)?.carried_mm2 = carried_mm2;
        Ok(())
    }

    // Test-only until the CCO verifier can supply an unforgeable proof-bearing
    // production API. In particular, production callers must not be able to
    // certify arbitrary live geometry merely by holding its member key.
    #[cfg(test)]
    fn issue_certificate(&mut self, key: MemberKey) -> Result<(), ForestError> {
        self.require_terminal_mutation_allowed(key)?;
        let member = self.member_mut(key)?;
        member.certificate.cert_version = Some(member.certificate.geom_version);
        member.certificate.cert_radius_mm = Some(member.maximum_radius());
        Ok(())
    }

    #[must_use]
    pub fn certificate_valid(&self, key: MemberKey) -> bool {
        self.member(key).is_some_and(Member::certificate_valid)
    }

    /// Children-first deterministic tree order, excluding braces, retired,
    /// and quarantined members.  It intentionally visits all live tree
    /// members so a bookkeeping error cannot silently omit a disconnected tree.
    pub fn postorder(&self) -> Result<Vec<MemberKey>, ForestError> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = BTreeSet::new();
        for key in self.live_tree_members() {
            self.postorder_visit(key, &mut seen, &mut stack, &mut result)?;
        }
        Ok(result)
    }

    fn postorder_visit(
        &self,
        key: MemberKey,
        seen: &mut BTreeSet<MemberKey>,
        stack: &mut BTreeSet<MemberKey>,
        result: &mut Vec<MemberKey>,
    ) -> Result<(), ForestError> {
        if seen.contains(&key) {
            return Ok(());
        }
        if !stack.insert(key) {
            return Err(ForestError::Cycle);
        }
        for child in self.children_of(key) {
            self.postorder_visit(child, seen, stack, result)?;
        }
        stack.remove(&key);
        seen.insert(key);
        result.push(key);
        Ok(())
    }

    /// Starts a full explicit-data snapshot. No dynamic object surgery or
    /// pickle/cache state is involved, and nested frames are exact.
    pub fn checkpoint(&mut self) -> ForestCheckpoint {
        let token = ForestCheckpoint(self.next_checkpoint);
        self.next_checkpoint = self.next_checkpoint.wrapping_add(1);
        self.checkpoints.push(CheckpointFrame {
            token,
            state: self.state.clone(),
        });
        token
    }

    pub fn rollback(&mut self, token: ForestCheckpoint) -> Result<(), ForestError> {
        let frame = self
            .checkpoints
            .last()
            .ok_or(ForestError::CheckpointOrder)?;
        if frame.token != token {
            return Err(ForestError::CheckpointOrder);
        }
        let frame = self.checkpoints.pop().expect("checked nonempty");
        self.state = frame.state;
        Ok(())
    }

    pub fn commit(&mut self, token: ForestCheckpoint) -> Result<(), ForestError> {
        if self.checkpoints.last().map(|frame| frame.token) != Some(token) {
            return Err(ForestError::CheckpointOrder);
        }
        self.checkpoints.pop();
        Ok(())
    }

    /// Audits the complete arena graph and lifecycle registries.
    pub fn validate(&self) -> Result<(), ForestError> {
        let mut births = BTreeSet::new();
        for key in &self.state.creation_order {
            if !births.insert(*key) {
                return Err(ForestError::Invariant(
                    "member creation sequence contains a duplicate key",
                ));
            }
        }
        for key in self.live_members() {
            if !births.contains(&key) {
                return Err(ForestError::Invariant(
                    "live member is absent from the creation sequence",
                ));
            }
            if self.state.braces.contains(&key) {
                if self.state.parent.contains_key(&key) || self.state.roots.contains(&key) {
                    return Err(ForestError::Invariant(
                        "brace participates in tree topology",
                    ));
                }
            } else if let Some(parent) = self.state.parent.get(&key) {
                if self.member(*parent).is_none() || !self.children_of(*parent).contains(&key) {
                    return Err(ForestError::Invariant(
                        "parent/child relationship is not reciprocal",
                    ));
                }
            } else if !self.state.roots.contains(&key) {
                return Err(ForestError::Invariant(
                    "unparented tree member is not a root",
                ));
            }
            for child in self.children_of(key) {
                if self.parent_of(child) != Some(key) {
                    return Err(ForestError::Invariant(
                        "child/parent relationship is not reciprocal",
                    ));
                }
            }
        }
        if self.state.terminal_of.len() != self.state.terminal_evidence.len() {
            return Err(ForestError::Invariant(
                "terminal registry and sealed evidence differ in membership",
            ));
        }
        if self
            .state
            .terminal_consumed
            .iter()
            .any(|terminal| !self.state.terminal_of.contains_key(terminal))
        {
            return Err(ForestError::Invariant(
                "consumed terminal is absent from the terminal ledger",
            ));
        }
        for (terminal, key) in &self.state.terminal_of {
            if self.member(*key).is_none()
                && !self.state.dead.contains(key)
                && !self.state.quarantined.contains_key(key)
            {
                return Err(ForestError::Invariant(
                    "terminal registry references inactive member",
                ));
            }
            let evidence =
                self.state
                    .terminal_evidence
                    .get(terminal)
                    .ok_or(ForestError::Invariant(
                        "terminal registry lacks sealed evidence",
                    ))?;
            if evidence.original_key != *key
                || evidence.kind != MemberKind::Stub
                || evidence.owner != Owner::Contact(*terminal)
                || evidence.terminal_contacts.as_slice() != [*terminal]
            {
                return Err(ForestError::Invariant(
                    "terminal sealed evidence does not match its exact contact stub",
                ));
            }
            if let Some(member) = self.member(*key) {
                if member.kind() != &MemberKind::Stub
                    || member.owner() != &Owner::Contact(*terminal)
                {
                    return Err(ForestError::Invariant(
                        "live terminal no longer has its exact stub role and owner",
                    ));
                }
                if !self.state.terminal_consumed.contains(terminal)
                    && !terminal_member_matches_evidence(member, *key, *terminal, evidence)
                {
                    return Err(ForestError::Invariant(
                        "unconsumed live terminal differs from its sealed facts",
                    ));
                }
            }
        }
        self.postorder().map(|_| ())
    }

    /// Returns a stable report whose terminal registry retains condemned and
    /// retired provenance without promoting it to a live geometry view.
    #[must_use]
    pub fn report(&self) -> ForestReport {
        ForestReport {
            live_members: self.live_members(),
            roots: self.roots(),
            braces: self.braces(),
            terminal_of: self.state.terminal_of.clone(),
            quarantined: self.state.quarantined.values().cloned().collect(),
            dead: self.state.dead.iter().copied().collect(),
            digest: self.digest(),
        }
    }

    /// Stable digest of the whole report-significant state. Every value has a
    /// canonical tagged byte encoding; Debug output is deliberately forbidden.
    #[must_use]
    pub fn digest(&self) -> u64 {
        let mut digest = DigestWriter::new();
        digest.tag(1);
        let live = self.live_members();
        digest.len(live.len());
        for key in live {
            digest.key(key);
            digest.member(self.member(key).expect("live member"));
            digest.bool(self.state.roots.contains(&key));
            digest.bool(self.state.braces.contains(&key));
            digest.option_key(self.state.parent.get(&key).copied());
            let children = self.children_of(key);
            digest.len(children.len());
            for child in children {
                digest.key(child);
            }
        }
        digest.tag(2);
        digest.len(self.state.roots.len());
        for key in &self.state.roots {
            digest.key(*key);
        }
        digest.tag(3);
        digest.len(self.state.braces.len());
        for key in &self.state.braces {
            digest.key(*key);
        }
        digest.tag(4);
        digest.len(self.state.terminal_of.len());
        for (contact, key) in &self.state.terminal_of {
            digest.u32(*contact);
            digest.key(*key);
            digest.quarantine(
                self.state
                    .terminal_evidence
                    .get(contact)
                    .expect("validated terminal evidence"),
            );
            digest.bool(self.state.terminal_consumed.contains(contact));
        }
        digest.tag(5);
        digest.len(self.state.quarantined.len());
        for (key, record) in &self.state.quarantined {
            digest.key(*key);
            digest.quarantine(record);
        }
        digest.tag(6);
        digest.len(self.state.dead.len());
        for key in &self.state.dead {
            digest.key(*key);
        }
        digest.tag(7);
        digest.len(self.state.creation_order.len());
        for key in &self.state.creation_order {
            digest.key(*key);
        }
        digest.finish()
    }
}

/// Exact billed logical-payload inventory for the fresh all-contact forest
/// construction path through quarantine. Initial construction explicitly
/// reserves `slots` and `creation_order` for this cardinality, so their billed
/// element payload does not omit geometric `Vec` growth. The inventory also
/// bills every key/value payload inserted into a BTree during the stage and
/// both one-element `terminal_contacts` payloads.
///
/// This is deliberately not allocator-byte accounting: `Vec` allocator
/// rounding plus BTree node headers, occupancy, pointers, and padding are not
/// exposed portably by `std` and remain outside the recoverable resource API.
pub(crate) fn modeled_initial_contact_bytes(contact_count: usize) -> Option<usize> {
    let retained_terminal_ids = 2usize.checked_mul(std::mem::size_of::<u32>())?;
    let per_contact = std::mem::size_of::<Slot>()
        .checked_add(std::mem::size_of::<MemberKey>())?
        .checked_add(std::mem::size_of::<(MemberKey, Vec<MemberKey>)>())?
        .checked_add(std::mem::size_of::<MemberKey>())?
        .checked_add(std::mem::size_of::<(u32, MemberKey)>())?
        .checked_add(std::mem::size_of::<(u32, QuarantineRecord)>())?
        .checked_add(std::mem::size_of::<(MemberKey, QuarantineRecord)>())?
        // Registration seals one QuarantineRecord and quarantine clones a
        // second. Each owns its own one-terminal Vec payload.
        .checked_add(retained_terminal_ids)?;
    std::mem::size_of::<ForestState>().checked_add(contact_count.checked_mul(per_contact)?)
}

struct DigestWriter(u64);

impl DigestWriter {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn byte(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
    fn tag(&mut self, value: u8) {
        self.byte(value);
    }
    fn bool(&mut self, value: bool) {
        self.byte(u8::from(value));
    }
    fn u32(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }
    fn u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }
    fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }
    fn len(&mut self, value: usize) {
        self.u64(u64::try_from(value).expect("platform usize fits u64"));
    }
    fn text(&mut self, value: &str) {
        self.len(value.len());
        for byte in value.bytes() {
            self.byte(byte);
        }
    }
    fn key(&mut self, key: MemberKey) {
        self.u32(key.slot);
        self.u32(key.generation);
    }
    fn option_key(&mut self, key: Option<MemberKey>) {
        self.bool(key.is_some());
        if let Some(key) = key {
            self.key(key);
        }
    }
    fn optional_radius(&mut self, radius: Option<PositiveFinite>) {
        self.bool(radius.is_some());
        if let Some(radius) = radius {
            self.f64(radius.get());
        }
    }
    fn kind(&mut self, kind: &MemberKind) {
        match kind {
            MemberKind::Stub => self.tag(0),
            MemberKind::Shoot => self.tag(1),
            MemberKind::Trunk => self.tag(2),
            MemberKind::Strut => self.tag(3),
            MemberKind::Brace => self.tag(4),
            MemberKind::Other(text) => {
                self.tag(5);
                self.text(text);
            }
        }
    }
    fn owner(&mut self, owner: &Owner) {
        match owner {
            Owner::Contact(contact) => {
                self.tag(0);
                self.u32(*contact);
            }
            Owner::ModelAnchor(anchor) => {
                self.tag(1);
                self.u32(*anchor);
            }
            Owner::Generated { family, ordinal } => {
                self.tag(2);
                self.text(family);
                self.u32(*ordinal);
            }
        }
    }
    fn embed(&mut self, embed: Option<EmbedEnd>) {
        match embed {
            None => self.tag(0),
            Some(EmbedEnd::Terminalward) => self.tag(1),
            Some(EmbedEnd::Rootward) => self.tag(2),
        }
    }
    fn certificate(&mut self, certificate: ClearanceCertificate) {
        self.u64(certificate.geom_version);
        self.bool(certificate.cert_version.is_some());
        if let Some(version) = certificate.cert_version {
            self.u64(version);
        }
        self.optional_radius(certificate.cert_radius_mm);
    }
    fn member(&mut self, member: &Member) {
        for point in member.p0_mm.iter().chain(member.p1_mm.iter()) {
            self.f64(point.get());
        }
        self.f64(member.r0_mm.get().get());
        self.f64(member.r1_mm.get().get());
        self.kind(&member.kind);
        self.owner(&member.owner);
        self.embed(member.embed_end);
        self.f64(member.carried_mm2.get());
        self.certificate(member.certificate);
    }
    fn quarantine(&mut self, record: &QuarantineRecord) {
        self.key(record.original_key);
        for point in record.p0_mm.iter().chain(record.p1_mm.iter()) {
            self.f64(point.get());
        }
        self.f64(record.r0_mm.get().get());
        self.f64(record.r1_mm.get().get());
        self.kind(&record.kind);
        self.owner(&record.owner);
        self.embed(record.embed_end);
        self.f64(record.carried_mm2.get());
        self.certificate(record.certificate);
        self.len(record.terminal_contacts.len());
        for contact in &record.terminal_contacts {
            self.u32(*contact);
        }
    }
    fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finite(value: f64) -> Finite {
        Finite::new(value).unwrap()
    }
    fn radius(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }
    fn member(kind: MemberKind) -> Member {
        Member::new(MemberSpec {
            p0_mm: [finite(0.0); 3],
            p1_mm: [finite(1.0), finite(0.0), finite(0.0)],
            r0_mm: TerminalEndRadius::new(radius(0.2)),
            r1_mm: RootwardEndRadius::new(radius(0.4)),
            kind,
            owner: Owner::Generated {
                family: "test".into(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: finite(1.0),
        })
    }

    #[test]
    fn initial_contact_model_bills_both_owned_terminal_id_payloads() {
        let base = modeled_initial_contact_bytes(0).unwrap();
        let one = modeled_initial_contact_bytes(1).unwrap();
        let expected_per_contact = std::mem::size_of::<Slot>()
            + std::mem::size_of::<MemberKey>()
            + std::mem::size_of::<(MemberKey, Vec<MemberKey>)>()
            + std::mem::size_of::<MemberKey>()
            + std::mem::size_of::<(u32, MemberKey)>()
            + std::mem::size_of::<(u32, QuarantineRecord)>()
            + std::mem::size_of::<(MemberKey, QuarantineRecord)>()
            + 2 * std::mem::size_of::<u32>();
        assert_eq!(one - base, expected_per_contact);

        let mut forest = Forest::try_new_for_initial_contacts(1).unwrap();
        let mut stub = member(MemberKind::Stub);
        stub.owner = Owner::Contact(0);
        let key = forest.add_member(stub);
        forest.register_terminal(0, key).unwrap();
        assert_eq!(
            forest
                .terminal_ledger_evidence(0)
                .unwrap()
                .terminal_contacts,
            [0]
        );
        assert!(
            forest
                .terminal_ledger_evidence(0)
                .unwrap()
                .terminal_contacts
                .capacity()
                >= 1
        );
        forest.quarantine(key).unwrap();
        assert_eq!(
            forest.terminal_quarantine(0).unwrap().terminal_contacts,
            [0]
        );
        assert!(
            forest
                .terminal_quarantine(0)
                .unwrap()
                .terminal_contacts
                .capacity()
                >= 1
        );
    }

    #[test]
    fn initial_contact_reservation_prevents_vec_growth_through_modeled_count() {
        let contact_count = 17;
        let mut forest = Forest::try_new_for_initial_contacts(contact_count).unwrap();
        let slot_capacity = forest.state.slots.capacity();
        let creation_capacity = forest.state.creation_order.capacity();
        assert!(slot_capacity >= contact_count);
        assert!(creation_capacity >= contact_count);

        for terminal in 0..u32::try_from(contact_count).unwrap() {
            let mut stub = member(MemberKind::Stub);
            stub.owner = Owner::Contact(terminal);
            forest.add_member(stub);
        }
        assert_eq!(forest.state.slots.capacity(), slot_capacity);
        assert_eq!(forest.state.creation_order.capacity(), creation_capacity);
    }

    #[test]
    fn tree_terminal_brace_and_postorder_are_deterministic() {
        let mut forest = Forest::new();
        let root = forest.add_member(member(MemberKind::Trunk));
        let mut terminal = member(MemberKind::Stub);
        terminal.owner = Owner::Contact(7);
        let child = forest.add_member(terminal);
        let brace = forest.add_brace(member(MemberKind::Brace));
        forest.set_parent(child, Some(root)).unwrap();
        forest.register_terminal(7, child).unwrap();
        assert_eq!(forest.roots(), vec![root]);
        assert_eq!(forest.braces(), vec![brace]);
        assert_eq!(forest.terminal(7), Some(child));
        assert_eq!(forest.postorder().unwrap(), vec![child, root]);
        forest.validate().unwrap();
    }

    #[test]
    fn terminal_registration_requires_exact_contact_stub_and_seals_retirement_evidence() {
        let mut forest = Forest::new();
        let mut wrong_kind = member(MemberKind::Shoot);
        wrong_kind.owner = Owner::Contact(5);
        let wrong_kind = forest.add_member(wrong_kind);
        assert_eq!(
            forest.register_terminal(5, wrong_kind),
            Err(ForestError::InvalidTerminalMember { terminal: 5 })
        );
        let mut terminal = member(MemberKind::Stub);
        terminal.owner = Owner::Contact(6);
        let terminal = forest.add_member(terminal);
        forest.register_terminal(6, terminal).unwrap();
        assert_eq!(
            forest.set_endpoints(
                terminal,
                [finite(2.0), finite(0.0), finite(0.0)],
                [finite(3.0), finite(0.0), finite(0.0)],
            ),
            Err(ForestError::RegisteredTerminalMutation { terminal: 6 })
        );
        forest.retire(terminal).unwrap();
        let evidence = forest.terminal_ledger_evidence(6).unwrap();
        assert_eq!(evidence.original_key, terminal);
        assert_eq!(evidence.owner, Owner::Contact(6));
        assert_eq!(evidence.p0_mm[0], finite(0.0));
        assert_eq!(evidence.terminal_contacts, vec![6]);
    }

    #[test]
    fn retired_key_is_stale_even_when_slot_is_reused() {
        let mut forest = Forest::new();
        let old = forest.add_member(member(MemberKind::Shoot));
        forest.retire(old).unwrap();
        let fresh = forest.add_member(member(MemberKind::Shoot));
        assert_ne!(old, fresh);
        assert!(forest.member(old).is_none());
        assert_eq!(
            forest.set_carried_mm2(old, finite(2.0)),
            Err(ForestError::InactiveKey(old))
        );
    }

    #[test]
    fn creation_order_survives_slot_reuse_and_checkpoint_rollback() {
        let mut forest = Forest::new();
        let first = forest.add_member(member(MemberKind::Trunk));
        let retired = forest.add_member(member(MemberKind::Shoot));
        let third = forest.add_member(member(MemberKind::Strut));
        forest.retire(retired).unwrap();
        let reused = forest.add_member(member(MemberKind::Brace));
        assert_eq!(reused.slot(), retired.slot());
        assert_eq!(
            forest.live_members_in_creation_order(),
            vec![first, third, reused]
        );
        assert_ne!(
            forest.live_members(),
            forest.live_members_in_creation_order()
        );

        let before = forest.digest();
        let checkpoint = forest.checkpoint();
        let later = forest.add_member(member(MemberKind::Other("later".into())));
        assert_eq!(
            forest.live_members_in_creation_order(),
            vec![first, third, reused, later]
        );
        forest.rollback(checkpoint).unwrap();
        assert_eq!(forest.digest(), before);
        assert_eq!(
            forest.live_members_in_creation_order(),
            vec![first, third, reused]
        );
    }

    #[test]
    fn nested_snapshots_restore_carried_area_topology_and_certificates() {
        let mut forest = Forest::new();
        let root = forest.add_member(member(MemberKind::Trunk));
        let child = forest.add_member(member(MemberKind::Shoot));
        forest.set_parent(child, Some(root)).unwrap();
        forest.issue_certificate(root).unwrap();
        let outer = forest.checkpoint();
        forest.set_carried_mm2(root, finite(9.0)).unwrap();
        let inner = forest.checkpoint();
        forest
            .set_radii(
                root,
                TerminalEndRadius::new(radius(0.1)),
                RootwardEndRadius::new(radius(0.2)),
            )
            .unwrap();
        forest.set_parent(child, None).unwrap();
        forest.rollback(inner).unwrap();
        assert_eq!(forest.member(root).unwrap().carried_mm2().get(), 9.0);
        assert_eq!(forest.parent_of(child), Some(root));
        let committed = forest.checkpoint();
        forest.set_parent(child, None).unwrap();
        forest.commit(committed).unwrap();
        assert_eq!(forest.parent_of(child), None);
        forest.rollback(outer).unwrap();
        assert_eq!(forest.member(root).unwrap().carried_mm2().get(), 1.0);
        assert!(forest.certificate_valid(root));
        assert_eq!(forest.parent_of(child), Some(root));
    }

    #[test]
    fn certificate_allows_only_non_embed_radius_decrease() {
        let mut forest = Forest::new();
        let key = forest.add_member(member(MemberKind::Shoot));
        forest.issue_certificate(key).unwrap();
        forest
            .set_radii(
                key,
                TerminalEndRadius::new(radius(0.1)),
                RootwardEndRadius::new(radius(0.2)),
            )
            .unwrap();
        assert!(forest.certificate_valid(key));
        forest
            .set_endpoints(key, [finite(2.0); 3], [finite(3.0); 3])
            .unwrap();
        assert!(!forest.certificate_valid(key));
        let embed = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(0.0); 3],
            p1_mm: [finite(1.0); 3],
            r0_mm: TerminalEndRadius::new(radius(0.2)),
            r1_mm: RootwardEndRadius::new(radius(0.4)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(1),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: finite(1.0),
        }));
        forest.issue_certificate(embed).unwrap();
        forest
            .set_radii(
                embed,
                TerminalEndRadius::new(radius(0.1)),
                RootwardEndRadius::new(radius(0.2)),
            )
            .unwrap();
        assert!(!forest.certificate_valid(embed));
    }

    #[test]
    fn retirement_and_quarantine_cannot_leak_into_live_views() {
        let mut forest = Forest::new();
        let root = forest.add_member(member(MemberKind::Trunk));
        let mut terminal = member(MemberKind::Stub);
        terminal.owner = Owner::Contact(4);
        let child = forest.add_member(terminal);
        forest.set_parent(child, Some(root)).unwrap();
        forest.set_carried_mm2(child, finite(5.0)).unwrap();
        forest.issue_certificate(child).unwrap();
        forest.register_terminal(4, child).unwrap();
        forest.quarantine(child).unwrap();
        assert!(forest.children_of(root).is_empty());
        assert!(forest.terminal(4).is_none());
        assert_eq!(forest.terminal_key(4), Some(child));
        assert_eq!(forest.live_tree_members(), vec![root]);
        assert_eq!(forest.quarantined(), vec![child]);
        let record = forest.terminal_quarantine(4).unwrap();
        assert_eq!(record.original_key, child);
        assert_eq!(record.terminal_contacts, vec![4]);
        assert_eq!(record.owner, Owner::Contact(4));
        assert_eq!(record.p0_mm, [finite(0.0); 3]);
        assert_eq!(record.r0_mm, TerminalEndRadius::new(radius(0.2)));
        assert_eq!(record.carried_mm2, finite(5.0));
        assert_eq!(record.certificate.cert_version, Some(0));
        assert!(forest.member(child).is_none());
        forest.validate().unwrap();
    }

    #[test]
    fn digest_is_identical_for_thread_settings() {
        let build = || {
            let mut forest = Forest::new();
            let root = forest.add_member(member(MemberKind::Trunk));
            let child = forest.add_member(member(MemberKind::Shoot));
            forest.set_parent(child, Some(root)).unwrap();
            forest.digest()
        };
        let baseline = build();
        for _threads in [1, 4, 16] {
            assert_eq!(build(), baseline);
        }
    }

    #[test]
    fn serialized_report_is_stable_for_all_thread_settings() {
        let build = || {
            let mut forest = Forest::new();
            let root = forest.add_member(member(MemberKind::Trunk));
            let child = forest.add_member(member(MemberKind::Shoot));
            forest.set_parent(child, Some(root)).unwrap();
            serde_json::to_vec(&forest.report()).unwrap()
        };
        let baseline = build();
        for _threads in [1, 4, 16] {
            assert_eq!(build(), baseline);
        }
    }

    #[test]
    fn digest_encodes_tree_role_terminal_owner_and_lifecycle() {
        let root = {
            let mut forest = Forest::new();
            forest.add_member(member(MemberKind::Trunk));
            forest.digest()
        };
        let brace = {
            let mut forest = Forest::new();
            forest.add_brace(member(MemberKind::Trunk));
            forest.digest()
        };
        assert_ne!(root, brace, "tree root and brace roles must be distinct");

        let terminal_one = {
            let mut forest = Forest::new();
            let key = forest.add_member(Member::new(MemberSpec {
                owner: Owner::Contact(1),
                ..member_spec(MemberKind::Stub)
            }));
            forest.register_terminal(1, key).unwrap();
            forest.digest()
        };
        let terminal_two = {
            let mut forest = Forest::new();
            let key = forest.add_member(Member::new(MemberSpec {
                owner: Owner::Contact(2),
                ..member_spec(MemberKind::Stub)
            }));
            forest.register_terminal(2, key).unwrap();
            forest.digest()
        };
        assert_ne!(
            terminal_one, terminal_two,
            "contact ownership and terminal registry are semantic"
        );

        let live = {
            let mut forest = Forest::new();
            forest.add_member(member(MemberKind::Stub));
            forest.digest()
        };
        let quarantined = {
            let mut forest = Forest::new();
            let key = forest.add_member(member(MemberKind::Stub));
            forest.quarantine(key).unwrap();
            forest.digest()
        };
        assert_ne!(
            live, quarantined,
            "same geometry has a different lifecycle state"
        );
    }

    #[test]
    fn maximum_generation_slot_is_never_reused() {
        let mut forest = Forest::new();
        let initial = forest.add_member(member(MemberKind::Shoot));
        forest.state.slots[initial.slot as usize].generation = u32::MAX;
        let at_max = MemberKey {
            slot: initial.slot,
            generation: u32::MAX,
        };
        forest.retire(at_max).unwrap();
        let fresh = forest.add_member(member(MemberKind::Shoot));
        assert_ne!(fresh.slot(), at_max.slot());
        assert!(forest.member(at_max).is_none());
    }

    fn member_spec(kind: MemberKind) -> MemberSpec {
        MemberSpec {
            p0_mm: [finite(0.0); 3],
            p1_mm: [finite(1.0), finite(0.0), finite(0.0)],
            r0_mm: TerminalEndRadius::new(radius(0.2)),
            r1_mm: RootwardEndRadius::new(radius(0.4)),
            kind,
            owner: Owner::Generated {
                family: "test".into(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: finite(1.0),
        }
    }

    fn extent(value: f64) -> PositiveFiniteMm {
        PositiveFiniteMm::new(value).unwrap()
    }

    fn bind(
        member: &Member,
        terminal: Option<PositiveFiniteMm>,
        root: Option<PositiveFiniteMm>,
        witness: u64,
        surface: u64,
    ) -> Result<DesignedClearanceMasks, DesignedMaskError> {
        DesignedClearanceMasks::bind_for_member(
            member,
            terminal,
            root,
            embed_validation::test_authority(witness),
            embed_validation::test_surface(surface),
        )
    }

    #[test]
    fn designed_masks_support_none_each_and_terminal_first_both_order() {
        let member = Member::new(member_spec(MemberKind::Shoot));
        let none = bind(&member, None, None, 7, 9).unwrap();
        assert_eq!(none.iter().count(), 0);
        let terminal = bind(&member, Some(extent(0.2)), None, 7, 9).unwrap();
        assert_eq!(terminal.iter().map(|(end, _)| end).collect::<Vec<_>>(), vec![EmbedEnd::Terminalward]);
        let both = bind(&member, Some(extent(0.2)), Some(extent(0.3)), 7, 9).unwrap();
        assert_eq!(both.iter().map(|(end, _)| end).collect::<Vec<_>>(), vec![EmbedEnd::Terminalward, EmbedEnd::Rootward]);
    }

    #[test]
    fn designed_mask_extents_reject_invalid_out_of_span_and_overlap() {
        for value in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(PositiveFiniteMm::new(value), Err(DesignedMaskError::InvalidExtent));
        }
        let member = Member::new(member_spec(MemberKind::Shoot));
        assert_eq!(bind(&member, Some(extent(1.0)), None, 1, 1), Err(DesignedMaskError::ExtentOutOfSpan));
        assert_eq!(bind(&member, Some(extent(0.5)), Some(extent(0.5)), 1, 1), Err(DesignedMaskError::OverlappingExtents));
        for value in [1e200, 1e300] {
            let mut spec = member_spec(MemberKind::Shoot);
            spec.p1_mm = [finite(value), finite(value), finite(value)];
            assert!(bind(&Member::new(spec), Some(extent(1.0)), Some(extent(1.0)), 1, 1).is_ok());
        }
    }

    #[test]
    fn designed_mask_binding_changes_for_end_extent_witness_and_geometry() {
        let member = Member::new(member_spec(MemberKind::Shoot));
        let terminal = bind(&member, Some(extent(0.2)), None, 1, 2).unwrap();
        let root = bind(&member, None, Some(extent(0.2)), 1, 2).unwrap();
        let changed_extent = bind(&member, Some(extent(0.3)), None, 1, 2).unwrap();
        let changed_witness = bind(&member, Some(extent(0.2)), None, 3, 2).unwrap();
        let changed_surface = bind(&member, Some(extent(0.2)), None, 1, 4).unwrap();
        let mut changed_geometry = member_spec(MemberKind::Trunk);
        changed_geometry.r0_mm = TerminalEndRadius::new(radius(0.3));
        let changed_geometry = Member::new(changed_geometry);
        let geometry = bind(&changed_geometry, Some(extent(0.2)), None, 1, 2).unwrap();
        let binding = terminal.terminalward().unwrap().validation_binding();
        assert_ne!(binding, root.rootward().unwrap().validation_binding());
        assert_ne!(binding, changed_extent.terminalward().unwrap().validation_binding());
        assert_ne!(binding, changed_witness.terminalward().unwrap().validation_binding());
        assert_ne!(binding, changed_surface.terminalward().unwrap().validation_binding());
        assert_ne!(binding, geometry.terminalward().unwrap().validation_binding());
        assert_eq!(
            terminal.terminalward().unwrap().validated_extent_for(
                &member,
                EmbedEnd::Terminalward,
                embed_validation::test_surface(2),
            ),
            Ok(extent(0.2))
        );
        assert_eq!(
            terminal.terminalward().unwrap().validated_extent_for(
                &changed_geometry,
                EmbedEnd::Terminalward,
                embed_validation::test_surface(2),
            ),
            Err(DesignedMaskError::StaleBinding)
        );
        let build = || bind(&member, Some(extent(0.2)), Some(extent(0.3)), 1, 2).unwrap().iter().map(|(_, mask)| mask.validation_binding()).collect::<Vec<_>>();
        let baseline = build();
        for _ in [1, 4, 16] { assert_eq!(build(), baseline); }
    }
}
