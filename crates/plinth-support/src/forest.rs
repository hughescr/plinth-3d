//! The deterministic, generational support-forest data model.
//!
//! This is deliberately the one owner of support-tree topology.  A member's
//! `p0` end is terminal-ward and its `p1` end is root-ward; distinct radius
//! types make reversing that convention a type error.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use geom_kernel::SurfaceTriangleSoupDigest;
use plinth_core::{Finite, PositiveFinite};
use serde::Serialize;
use thiserror::Error;

use crate::designed_embed::PositiveFiniteMm;
use crate::seat_zone::SeatZoneRecord;

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

/// The endpoint at which this member meets the model surface, if any.
///
/// This was named `EmbedEnd`, which conflated three separate jobs:
///
/// 1. *this end meets the model* — read by SCAR/transit selection
///    (`transit_measurement`), the joints `embedded` ball flag, the transient
///    brace-endpoint exclusion, `lower::is_embedded_taper`, and the
///    clearance-certificate fast path in [`Member::certificate_valid`];
/// 2. *this end carries a designed clearance extent* — the `designed_embed`
///    mask domain;
/// 3. *this end gets a clearance cushion* — the retired `_embed_inset_arc_exact`
///    inset.
///
/// Owner ruling OD-1 removes the *embed* claim (job 2) from the touching-foot
/// families (brace model rungs, CCO model-anchor struts, and the CCO upper
/// inheritance). It does **not** remove their *contact*. Deleting the field for
/// those families would silently drop them out of every job-1 reader — most
/// seriously `lower::is_embedded_taper`, where a CCO model-anchor strut is
/// tapered (`r0 = cco_terminal_radius`, `r1 = MIN_SHOOT_R_MM`) and therefore
/// does emit `SupportSegmentKind::ContactTaper` with a `contact_depth`
/// extension today. That is printed geometry, which the approved package
/// forbids changing.
///
/// So the field is renamed to what it always meant. The embed-vs-touch
/// distinction becomes a recomputed family property (OD-3), never stored.
///
/// The [`DigestWriter`] tag bytes (0 = `None`, 1 = `Terminalward`,
/// 2 = `Rootward`) are deliberately unchanged, so `Forest::digest`,
/// [`mask_binding`], `QuarantineRecord` digests, the runner spec/snapshot
/// hashes and every downstream golden are bit-identical across this rename.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum ContactEnd {
    Terminalward,
    Rootward,
}

/// The immutable identity a validated designed clearance mask is bound to.
///
/// This deliberately stays beside [`DigestWriter`] and the private `Member`
/// fields rather than moving with the mask domain, so the binding bytes are
/// literally unchanged by that move: any member mutation bumps the clearance
/// certificate's `geom_version`, which is part of `DigestWriter::member`, so a
/// stale mask can never re-validate.
pub(crate) fn mask_binding(
    member: &Member,
    end: ContactEnd,
    extent: PositiveFiniteMm,
    witness: u64,
    surface_identity: SurfaceTriangleSoupDigest,
) -> u64 {
    let mut digest = DigestWriter::new();
    digest.tag(0x45);
    digest.contact_end(Some(end));
    digest.f64(extent.get());
    digest.u64(witness);
    digest.bytes(surface_identity.as_bytes());
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
    model_contact_end: Option<ContactEnd>,
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
    pub model_contact_end: Option<ContactEnd>,
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
            model_contact_end: spec.model_contact_end,
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
    pub const fn model_contact_end(&self) -> Option<ContactEnd> {
        self.model_contact_end
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
        self.model_contact_end.is_none()
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
        if self.model_contact_end.is_some() {
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
    pub model_contact_end: Option<ContactEnd>,
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
            model_contact_end: member.model_contact_end,
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
        && evidence.model_contact_end == member.model_contact_end
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
    /// The FROZEN seat zone `Z` of every contact-declaring member: the global
    /// face IDs its own seat forgives, sorted ascending and deduplicated
    /// (`crate::seat_zone`).
    ///
    /// A SIDE TABLE, deliberately, and deliberately outside [`Forest::digest`]
    /// and off `Member` (honest-exemption design, section 6):
    ///
    /// * `Member` is `Serialize` and feeds the run's disclosed geometry. A
    ///   frozen face list is an implementation detail of a VERDICT, not
    ///   geometry, and on `Member` it would land in front of every artifact
    ///   consumer.
    /// * `digest()` hashes explicit fields, so a side table cannot move a digest
    ///   by accident. That is what makes "a model with no affected arms stays
    ///   bit-identical" provable BY INSPECTION rather than by running a corpus.
    /// * `MemberKey` carries a generation beside its slot, so a recycled slot
    ///   yields a different key: a stale entry is unreachable, never wrong.
    ///
    /// The table is PASSIVE: `set_radii` does not touch it, so nothing here
    /// tracks a radius implicitly — pinned by
    /// `a_frozen_seat_zone_survives_the_resize_that_changes_its_member_radius`.
    /// Re-sizing an exemption to a grown arm's final footprint is therefore an
    /// EXPLICIT pass someone has to call
    /// (`seat_zone::refreeze_resized_seat_zones`, Craig's ruling 2026-08-23),
    /// which is why the entry carries the band radius it was cut at: that is how
    /// the pass knows which members the sizer moved.
    seat_zones: BTreeMap<MemberKey, SeatZoneRecord>,
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
    #[error(
        "retiring {member:?} would strand its live tree children {children:?} as \
         ungrounded roots; re-anchor them first"
    )]
    RetireWouldStrandChildren {
        member: MemberKey,
        children: Vec<MemberKey>,
    },
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

    /// Freeze this member's seat zone `Z` — the global faces its OWN seat
    /// forgives, computed from the radius the contact end carries at the moment
    /// of the freeze.
    ///
    /// **This side table is PASSIVE.** It tracks nothing. `set_radii` does not
    /// touch it, and neither does any grading site, which is what the
    /// monotonicity theorem rests on: within a grading pass the tested face set
    /// cannot move when the radius does.
    ///
    /// Two things call this. A creation site, minting a contact; and
    /// `seat_zone::refreeze_resized_seat_zones`, the one explicit pass that runs
    /// AFTER a sizing pass and BEFORE the next grading pass, re-sizing a seated
    /// arm's exemption to its final footprint (Craig's ruling, 2026-08-23). A
    /// second call for the same key REPLACES the entry, which is what both of
    /// those need.
    pub(crate) fn set_seat_zone(&mut self, key: MemberKey, record: SeatZoneRecord) {
        self.state.seat_zones.insert(key, record);
    }

    /// Remove and return this member's frozen zone — the MOVE half of a
    /// contact declaration changing hands.
    ///
    /// `cco::apply_bifurcation` is the one caller. Splitting a host that
    /// declares a `Rootward` contact gives the contact to the UPPER half (it
    /// keeps `p1`, the seat) and clears it from the lower one. The zone belongs
    /// to the CONTACT, so it has to travel with the declaration rather than be
    /// recomputed for the new member — recomputing it from the upper half's own
    /// radius is exactly the un-freezing the monotonicity theorem forbids
    /// (design §3).
    ///
    /// Taking rather than copying also keeps the table honest: a member with no
    /// contact must forgive nothing, and a stale entry left behind on the lower
    /// half would exempt faces for a member that no longer has a seat.
    pub(crate) fn take_seat_zone(&mut self, key: MemberKey) -> Option<SeatZoneRecord> {
        self.state.seat_zones.remove(&key)
    }

    /// This member's frozen seat zone, or `None` if it never declared a
    /// contact.
    ///
    /// `None` means "exempt nothing": the row is then graded against the WHOLE
    /// surface, which is the strict direction and the correct one for a member
    /// with no seat of its own.
    #[must_use]
    #[allow(
        dead_code,
        reason = "the creation sites that freeze into the table land one commit ahead of the \
                  grading sites that read it; docs/engineering-journal/designs/\
                  honest-exemption-2026-08-22.md section 8 step 5 names them"
    )]
    pub(crate) fn seat_zone(&self, key: MemberKey) -> Option<&Arc<[u32]>> {
        self.state.seat_zones.get(&key).map(SeatZoneRecord::zone)
    }

    /// The frozen zone TOGETHER with the two facts a re-size needs: the seat's
    /// own face ID and the band radius the zone was frozen at.
    ///
    /// `seat_zone::refreeze_resized_seat_zones` is the reader. Grading sites use
    /// [`Self::seat_zone`] and never see this, because a grading site has no
    /// business knowing what radius the band was cut at.
    #[must_use]
    pub(crate) fn seat_zone_record(&self, key: MemberKey) -> Option<&SeatZoneRecord> {
        self.state.seat_zones.get(&key)
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

    /// Removes a member from all live topology. The old key is permanently
    /// stale, even if its slot is reused.
    ///
    /// Retirement **refuses** a member that still has live tree children.  It
    /// used to promote them to roots, and that was the one reachable path to
    /// shipping a floating support member: a promoted child keeps the rootward
    /// endpoint it had while it hung off its parent, so its `p1` sits wherever
    /// the retired member's terminal-ward end was — in the air, not on the
    /// plate.  Nothing downstream catches it.  The arena holds no plate
    /// reference, so [`Forest::validate`] cannot tell that root from a
    /// legitimately grounded one, and `lower` simply omits the root boss for a
    /// root outside the plate band rather than objecting.
    ///
    /// The arena therefore cannot re-anchor the child itself: choosing between
    /// extending the child's rootward end down to this member's own rootward
    /// end, splicing it onto this member's parent, and retiring the subtree is
    /// a geometric decision that belongs to the caller, who owns the plate
    /// frame.  The refusal names the member and the exact children that would
    /// have been stranded, mirroring [`Forest::quarantine`]'s
    /// [`ForestError::QuarantineHasChildren`].
    ///
    /// The check runs before any state changes, so a refused retirement leaves
    /// the terminal ledger and the topology exactly as it found them.
    pub fn retire(&mut self, key: MemberKey) -> Result<(), ForestError> {
        self.require_active(key)?;
        let children = self.children_of(key);
        if !children.is_empty() {
            return Err(ForestError::RetireWouldStrandChildren {
                member: key,
                children,
            });
        }
        let terminal_contacts: Vec<_> = self
            .state
            .terminal_of
            .iter()
            .filter_map(|(terminal, member_key)| (*member_key == key).then_some(*terminal))
            .collect();
        for terminal in terminal_contacts {
            self.state.terminal_consumed.insert(terminal);
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

    /// Quarantines each named member **together with everything that hangs off
    /// it**, deepest child first, and returns every member actually condemned
    /// in the order it was condemned.
    ///
    /// **Why this exists, and why it is not a weakening of
    /// [`Forest::quarantine`]'s childless rule.** `quarantine` refuses a member
    /// with live tree children for the same reason [`Forest::retire`] does: the
    /// arena cannot re-anchor a child, because choosing between extending it,
    /// splicing it onto its grandparent and condemning the subtree is a
    /// geometric decision belonging to the caller who owns the plate frame.
    /// This entry point is a caller making the third choice — *retire the
    /// subtree* — explicitly, by name, at the arena's own leaf-first order. The
    /// childless rule is untouched: every individual quarantine this performs
    /// sees a member with no live children, because the descendants went first.
    ///
    /// **The second choice is not available to a model-seated arm, which is
    /// what forces the third.** `cco::apply_model_anchor` mints the arm as a
    /// ROOT (nothing ever sets its parent — its rootward end is seated on the
    /// model, not on the plate) and re-parents the contact stub onto it. So the
    /// grandparent a splice would need does not exist, and `set_parent(stub,
    /// None)` is exactly the promotion-to-root that `retire`'s doc names as
    /// "the one reachable path to shipping a floating support member". Cascading
    /// is the only option the invariants leave.
    ///
    /// **The load argument, which is the physical one.** A condemned carrier
    /// takes its dependants' only route to ground with it. Every member of the
    /// subtree below a condemned member has no closed load path afterwards, so
    /// shipping it would ship a support that holds its tip up with nothing
    /// underneath. Condemning it is not collateral damage; it is the same
    /// verdict, propagated to the members the verdict applies to.
    ///
    /// **All-or-nothing.** Every member in the union is validated active before
    /// any state changes, so a refused sweep leaves the topology and the
    /// terminal ledger exactly as it found them — the same property `retire`
    /// keeps. Keys that name a member already inside another named member's
    /// subtree are swept once, not twice, so an overlapping batch is legal.
    ///
    /// **Disclosure rides the records that already exist.** A cascaded member
    /// gets an ordinary [`QuarantineRecord`], so it appears in
    /// [`ForestReport::quarantined`] and — when it is a contact stub —
    /// in [`Forest::terminal_quarantine`], which is what SPEC §6's removal and
    /// the noncoverage `quarantined` component already read. No new reporting
    /// channel is introduced and no digest gains a field: the forest digest
    /// moves because the members left the live view, which is the honest signal.
    pub fn quarantine_subtrees(
        &mut self,
        keys: &[MemberKey],
    ) -> Result<Vec<MemberKey>, ForestError> {
        let order = self.plan_quarantine_subtrees(keys)?;
        for key in &order {
            self.quarantine(*key)?;
        }
        Ok(order)
    }

    /// The **planner half** of [`Forest::quarantine_subtrees`]: exactly what
    /// that sweep would condemn, in exactly the order it would condemn it, with
    /// nothing mutated.
    ///
    /// **Why this is a split rather than a second walk.** A caller that needs
    /// to know the condemnation set BEFORE deciding whether the sweep may run
    /// at all — the seal-semantics classifier in `brace` (design
    /// `seal-semantics-2026-08-22.md` §4 Stage A(b)) — must read the same
    /// answer the mutation would produce. Exposing
    /// [`Forest::collect_subtree_leaf_first`] alone would reproduce the walk
    /// but neither `require_active` pass nor the union-dedup contract, and the
    /// query and the mutation would then drift. Here they cannot: the mutation
    /// is this function plus a `quarantine` per planned key, so the query is a
    /// literal prefix of the mutation rather than a copy of it.
    ///
    /// **It keeps the all-or-nothing refusal.** A key naming an inactive member
    /// is a [`ForestError`] here, at the same point and for the same reason it
    /// is one in the sweep: asking to condemn something already condemned is an
    /// engine defect, not a physical verdict, and the classifier must not
    /// swallow it.
    pub fn plan_quarantine_subtrees(
        &self,
        keys: &[MemberKey],
    ) -> Result<Vec<MemberKey>, ForestError> {
        let mut order = Vec::new();
        let mut seen = BTreeSet::new();
        for key in keys {
            if seen.contains(key) {
                continue;
            }
            self.require_active(*key)?;
            self.collect_subtree_leaf_first(*key, &mut seen, &mut order)?;
        }
        // Validate the whole union before touching any state: a sweep that
        // refuses halfway would leave a forest neither caller could reason
        // about.
        for key in &order {
            self.require_active(*key)?;
        }
        Ok(order)
    }

    /// The terminal ledger inverted: which terminal each registered contact
    /// member belongs to, built once for a whole batch.
    ///
    /// `terminal_of` is the only map that binds a member to a terminal, so the
    /// inverse is taken from it rather than hand-rolled from key comparisons —
    /// member generations move (a Giant Bat arm is `slot 377, generation 2`),
    /// and a hand-rolled comparison against a stale key is a defect waiting.
    /// The inverse is injective by construction: [`Forest::register_terminal`]
    /// refuses a member whose owner is not `Owner::Contact(terminal)`, so two
    /// terminals can never name one member.
    pub(crate) fn terminals_by_member(&self) -> BTreeMap<MemberKey, u32> {
        self.state
            .terminal_of
            .iter()
            .map(|(terminal, key)| (*key, *terminal))
            .collect()
    }

    /// Post-order walk of `key`'s live subtree, pushing each member after its
    /// own children so every entry is childless by the time it is quarantined.
    ///
    /// The forest is a tree — [`Forest::set_parent`] refuses a cycle — so the
    /// walk terminates; `seen` is the union bookkeeping across sibling roots in
    /// one batch, not a cycle guard.
    fn collect_subtree_leaf_first(
        &self,
        key: MemberKey,
        seen: &mut BTreeSet<MemberKey>,
        order: &mut Vec<MemberKey>,
    ) -> Result<(), ForestError> {
        if !seen.insert(key) {
            return Ok(());
        }
        for child in self.children_of(key) {
            self.collect_subtree_leaf_first(child, seen, order)?;
        }
        order.push(key);
        Ok(())
    }

    /// Quarantines a terminal stub: it remains recorded as condemned but is
    /// structurally absent from all live tree and emission views.
    ///
    /// **Refuses a member with live tree children.** Use
    /// [`Forest::quarantine_subtrees`] when the caller's verdict is meant to
    /// carry to the dependants too; that entry point states the choice rather
    /// than making it silently here.
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

    pub fn set_model_contact_end(
        &mut self,
        key: MemberKey,
        end: Option<ContactEnd>,
    ) -> Result<(), ForestError> {
        self.require_terminal_mutation_allowed(key)?;
        let member = self.member_mut(key)?;
        member.model_contact_end = end;
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

pub(crate) struct DigestWriter(u64);

impl DigestWriter {
    pub(crate) fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn byte(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }
    pub(crate) fn tag(&mut self, value: u8) {
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
    pub(crate) fn u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }
    fn bytes(&mut self, value: &[u8]) {
        self.len(value.len());
        for byte in value {
            self.byte(*byte);
        }
    }
    pub(crate) fn f64(&mut self, value: f64) {
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
    pub(crate) fn contact_end(&mut self, embed: Option<ContactEnd>) {
        match embed {
            None => self.tag(0),
            Some(ContactEnd::Terminalward) => self.tag(1),
            Some(ContactEnd::Rootward) => self.tag(2),
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
        self.contact_end(member.model_contact_end);
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
        self.contact_end(record.model_contact_end);
        self.f64(record.carried_mm2.get());
        self.certificate(record.certificate);
        self.len(record.terminal_contacts.len());
        for contact in &record.terminal_contacts {
            self.u32(*contact);
        }
    }
    pub(crate) fn finish(self) -> u64 {
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
            model_contact_end: None,
            carried_mm2: finite(1.0),
        })
    }
    /// A side-table entry with a chosen face list and band radius. The axis and
    /// the seat face are irrelevant to what these tests measure — they are about
    /// the TABLE's behaviour, not about geometry — so they are stated once here
    /// rather than repeated per test.
    fn test_seat_zone_record(faces: Vec<u32>, band_radius_mm: f64) -> SeatZoneRecord {
        SeatZoneRecord::new(
            Arc::from(faces),
            &crate::seat_zone::SeatZoneRequest {
                p0_mm: [0.0; 3],
                p1_mm: [1.0, 0.0, 0.0],
                seat_face_id: 0,
                band_radius_mm,
            },
        )
    }

    /// **The exact-prefix proof for the seal-semantics classifier.**
    ///
    /// `plan_quarantine_subtrees` exists so the brace stage can read what a
    /// sweep WOULD condemn before deciding whether the sweep may run. That is
    /// only sound while the plan is the same answer the mutation produces, in
    /// the same order — including the union dedup across overlapping keys.
    /// Re-implement the walk one layer up and the two drift; this test stands
    /// on the shortcut.
    #[test]
    fn the_planner_returns_precisely_the_order_the_sweep_removes() {
        let build = || {
            let mut forest = Forest::new();
            let root = forest.add_member(member(MemberKind::Strut));
            let mid = forest.add_member(member(MemberKind::Shoot));
            let leaf_a = forest.add_member(member(MemberKind::Stub));
            let leaf_b = forest.add_member(member(MemberKind::Stub));
            forest.set_parent(mid, Some(root)).unwrap();
            forest.set_parent(leaf_a, Some(mid)).unwrap();
            forest.set_parent(leaf_b, Some(root)).unwrap();
            (forest, root, mid)
        };

        // Overlapping keys on purpose: `mid` is already inside `root`'s
        // subtree, so a batch that names both must sweep each member once.
        let (planning, root, mid) = build();
        let planned = planning.plan_quarantine_subtrees(&[root, mid]).unwrap();
        let planning_digest = planning.digest();

        let (mut sweeping, root, mid) = build();
        let removed = sweeping.quarantine_subtrees(&[root, mid]).unwrap();

        assert_eq!(
            planned, removed,
            "the planner must return the sweep's own order, member for member"
        );
        assert_eq!(
            planned.len(),
            4,
            "the union dedup must condemn each member exactly once"
        );
        assert_eq!(
            planning.digest(),
            planning_digest,
            "planning must not mutate the forest"
        );
        for key in &removed {
            assert!(
                sweeping.quarantine_record(*key).is_some(),
                "every planned member must actually carry a quarantine record"
            );
        }
    }

    /// A directly named inactive key stays a `ForestError` in the PLANNER, at
    /// the same point the sweep raises it. Design §4 Stage A(a): asking to
    /// condemn something already condemned is an engine defect, not a physical
    /// verdict, and the classifier must not be able to swallow it.
    #[test]
    fn planning_a_directly_named_inactive_key_still_refuses() {
        let mut forest = Forest::new();
        let key = forest.add_member(member(MemberKind::Strut));
        forest.quarantine(key).unwrap();
        assert!(matches!(
            forest.plan_quarantine_subtrees(&[key]),
            Err(ForestError::InactiveKey(_))
        ));
    }

    /// The side table tracks NOTHING, and that is the property the whole rule
    /// rests on.
    ///
    /// A resize is `set_radii` on the SAME `MemberKey` (`brace::apply_resize`),
    /// and it leaves the table untouched. If the storage layer followed the
    /// radius implicitly, `Z` would move underneath a grading pass — a fatter
    /// arm would claim a wider band, a wider band would swallow more faces, and
    /// the fatter arm would be graded against strictly LESS surface than a thin
    /// one on the same axis: the retired skip-length's inversion, rebuilt one
    /// level up.
    ///
    /// Re-sizing an exemption to a grown arm's final footprint (Craig's ruling,
    /// 2026-08-23) is therefore something a caller has to ASK for, at a defined
    /// point between a sizing pass and the next grading pass —
    /// `seat_zone::refreeze_resized_seat_zones`. This test is what makes that
    /// explicitness true rather than merely intended.
    #[test]
    fn a_frozen_seat_zone_survives_the_resize_that_changes_its_member_radius() {
        let mut forest = Forest::new();
        let key = forest.add_member(member(MemberKind::Strut));
        forest.set_seat_zone(key, test_seat_zone_record(vec![3_u32, 11, 42], 0.95));

        forest
            .set_radii(
                key,
                TerminalEndRadius::new(radius(1.7)),
                RootwardEndRadius::new(radius(1.7)),
            )
            .unwrap();

        assert_eq!(
            forest.seat_zone(key).map(|zone| zone.to_vec()),
            Some(vec![3, 11, 42]),
            "a resize must not move the frozen zone"
        );
        assert_eq!(
            forest
                .seat_zone_record(key)
                .map(SeatZoneRecord::band_radius_mm),
            Some(0.95),
            "and it must not move the band radius either — the band the zone was \
             cut at is how the explicit re-size pass later recognises that this \
             member's radius moved"
        );
    }

    /// The side table is invisible to the digest, which is what makes "a model
    /// with no affected arms stays bit-identical" provable by inspection.
    #[test]
    fn freezing_a_seat_zone_does_not_move_the_forest_digest() {
        let mut forest = Forest::new();
        let key = forest.add_member(member(MemberKind::Strut));
        let before = forest.digest();
        forest.set_seat_zone(key, test_seat_zone_record(vec![7_u32, 9], 0.95));
        assert_eq!(
            forest.digest(),
            before,
            "the frozen face list is an implementation detail of a VERDICT, not \
             disclosed geometry: it must not reach the digest"
        );
    }

    /// A member that declared no contact has no zone, and `None` is read as
    /// "exempt nothing" — the strict direction.
    #[test]
    fn a_member_with_no_contact_has_no_zone_and_therefore_exempts_nothing() {
        let mut forest = Forest::new();
        let key = forest.add_member(member(MemberKind::Strut));
        assert!(forest.seat_zone(key).is_none());
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
            model_contact_end: Some(ContactEnd::Terminalward),
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
    fn retirement_refuses_to_strand_live_children_as_ungrounded_roots() {
        let mut forest = Forest::new();
        let root = forest.add_member(member(MemberKind::Trunk));
        let mut terminal = member(MemberKind::Stub);
        terminal.owner = Owner::Contact(7);
        let child = forest.add_member(terminal);
        forest.set_parent(child, Some(root)).unwrap();
        forest.register_terminal(7, child).unwrap();

        // The child hangs off `root`; promoting it to a root would leave its
        // rootward end wherever `root`'s terminal-ward end was — in the air.
        assert_eq!(
            forest.retire(root),
            Err(ForestError::RetireWouldStrandChildren {
                member: root,
                children: vec![child],
            })
        );

        // The refusal is total: no promotion, no terminal consumed, no slot
        // retired, and the forest still audits clean.
        assert_eq!(forest.parent_of(child), Some(root));
        assert_eq!(forest.roots(), vec![root]);
        assert_eq!(forest.children_of(root), vec![child]);
        assert_eq!(forest.terminal(7), Some(child));
        assert_eq!(forest.live_tree_members(), vec![root, child]);
        forest.validate().unwrap();

        // The caller states the re-anchor. Once the subtree is gone the same
        // retirement is legal, and a childless member never needed the guard.
        forest.quarantine(child).unwrap();
        forest.retire(root).unwrap();
        assert!(forest.live_tree_members().is_empty());
        forest.validate().unwrap();
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
            model_contact_end: None,
            carried_mm2: finite(1.0),
        }
    }
}
