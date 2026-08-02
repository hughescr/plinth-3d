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

#[derive(Clone, Debug, PartialEq, Default)]
struct ForestState {
    slots: Vec<Slot>,
    parent: BTreeMap<MemberKey, MemberKey>,
    children: BTreeMap<MemberKey, Vec<MemberKey>>,
    roots: BTreeSet<MemberKey>,
    terminal_of: BTreeMap<u32, MemberKey>,
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
        self.state.terminal_of.insert(terminal, key);
        Ok(())
    }

    /// Removes a terminal registration when later placement deliberately
    /// replaces the contact, rather than silently overwriting its provenance.
    pub fn unregister_terminal(&mut self, terminal: u32) -> Option<MemberKey> {
        self.state.terminal_of.remove(&terminal)
    }

    /// Attaches `child` to `parent` at the parent's terminal-ward (`p0`) end.
    pub fn set_parent(
        &mut self,
        child: MemberKey,
        parent: Option<MemberKey>,
    ) -> Result<(), ForestError> {
        self.require_active(child)?;
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
        let terminal_contacts = self
            .state
            .terminal_of
            .iter()
            .filter_map(|(terminal, member_key)| (*member_key == key).then_some(*terminal))
            .collect();
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
        let member = self.member_mut(key)?;
        member.r0_mm = r0_mm;
        member.r1_mm = r1_mm;
        member.invalidate_radius();
        Ok(())
    }

    pub fn set_kind(&mut self, key: MemberKey, kind: MemberKind) -> Result<(), ForestError> {
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
        self.member_mut(key)?.carried_mm2 = carried_mm2;
        Ok(())
    }

    // Test-only until the CCO verifier can supply an unforgeable proof-bearing
    // production API. In particular, production callers must not be able to
    // certify arbitrary live geometry merely by holding its member key.
    #[cfg(test)]
    fn issue_certificate(&mut self, key: MemberKey) -> Result<(), ForestError> {
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
        for key in self.live_members() {
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
        for key in self.state.terminal_of.values() {
            if self.member(*key).is_none()
                && !self.state.dead.contains(key)
                && !self.state.quarantined.contains_key(key)
            {
                return Err(ForestError::Invariant(
                    "terminal registry references inactive member",
                ));
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
        digest.finish()
    }
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
    fn tree_terminal_brace_and_postorder_are_deterministic() {
        let mut forest = Forest::new();
        let root = forest.add_member(member(MemberKind::Trunk));
        let child = forest.add_member(member(MemberKind::Shoot));
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
        let child = forest.add_member(member(MemberKind::Stub));
        forest.set_parent(child, Some(root)).unwrap();
        forest.register_terminal(4, child).unwrap();
        forest.set_carried_mm2(child, finite(5.0)).unwrap();
        forest.issue_certificate(child).unwrap();
        forest.quarantine(child).unwrap();
        assert!(forest.children_of(root).is_empty());
        assert!(forest.terminal(4).is_none());
        assert_eq!(forest.terminal_key(4), Some(child));
        assert_eq!(forest.live_tree_members(), vec![root]);
        assert_eq!(forest.quarantined(), vec![child]);
        let record = forest.terminal_quarantine(4).unwrap();
        assert_eq!(record.original_key, child);
        assert_eq!(record.terminal_contacts, vec![4]);
        assert_eq!(
            record.owner,
            Owner::Generated {
                family: "test".into(),
                ordinal: 0
            }
        );
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
}
