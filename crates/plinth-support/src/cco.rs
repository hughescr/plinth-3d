//! Current pre-P8 CCO terminal growth.
//!
//! Candidate descriptions are generated serially from one forest snapshot,
//! evaluated independently, flattened into one native clearance batch, and
//! scattered back to their original indices.  Only the deterministic winner
//! is committed to the live forest.
use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::NativeSurface;
use plinth_core::{m0_constants, Finite, PositiveFinite};
use rayon::prelude::*;
use thiserror::Error;

use crate::forest::{
    EmbedEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec, Owner,
    RootwardEndRadius, TerminalEndRadius, TerminalLedgerEntry, TerminalLedgerState,
};
use crate::legacy_compat::{LegacyCompatibility, LegacyCompatibilityError};
use crate::transient::{derive_brace_attachments, TransientError};
use crate::{
    anchor::{
        discover_finalized_anchor_seats, discover_rewire_anchor_seats,
        validate_finalized_anchor_feet, validate_rewire_anchor_feet, AnchorFootOutcome,
        AnchorFootRefusal, AnchorFootRefusalReason, AnchorFootRequest, AnchorGeometryAuthority,
        AnchorSearchConfig, AnchorSearchError, AnchorSeatRefusal, AnchorTerminalBinding,
        BoundPhysicalAnchorContact, GeometryFingerprint, QualifiedAnchorSeat, RewireAnchorBinding,
        SurfaceAnchorPool, SurfaceAnchorSample, ValidatedModelAnchor,
    },
    initial::{InitialContactBuild, InitialTerminalEvidence},
    perception::FinalizedContactLoads,
    specialized::{SpecializedContactPlan, SpecializedPhysicalContact},
    BuildFrame,
};

const MILLIMETRES_PER_MILLILITRE: f64 = 1_000.0;
const DEGENERATE_MM: f64 = 1e-6;
const REWIRE_DEGENERATE_MM: f64 = 1e-4;
const KDTREE_REBUILD_EVERY: usize = 25;

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct XiaAngles {
    pub(crate) child_one_turn_deg: f64,
    pub(crate) child_two_turn_deg: f64,
    pub(crate) between_children_deg: f64,
}

/// Frozen Xia alpha=1/2 closed form from `cco.py::xia_angles`.
#[cfg(test)]
pub(crate) fn xia_angles(m1: f64, m2: f64) -> XiaAngles {
    let total = m1 + m2;
    if total <= 0.0 {
        return XiaAngles {
            child_one_turn_deg: 45.0,
            child_two_turn_deg: 45.0,
            between_children_deg: 90.0,
        };
    }
    let turn = |load: f64| {
        (load / total)
            .clamp(0.0, 1.0)
            .sqrt()
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    };
    XiaAngles {
        child_one_turn_deg: turn(m1),
        child_two_turn_deg: turn(m2),
        between_children_deg: 90.0,
    }
}

/// Frozen bounded Weiszfeld polish: exactly `iterations` updates at most.
fn weighted_fermat_point(
    targets: &[([f64; 3], f64)],
    seed: [f64; 3],
    iterations: usize,
) -> [f64; 3] {
    let mut junction = seed;
    for _ in 0..iterations {
        let mut numerator = [0.0; 3];
        let mut denominator = 0.0;
        let mut degenerate = false;
        for &(target, weight) in targets {
            let distance = norm(sub(junction, target));
            if distance < DEGENERATE_MM {
                degenerate = true;
                break;
            }
            for axis in 0..3 {
                numerator[axis] += weight * target[axis] / distance;
            }
            denominator += weight / distance;
        }
        if degenerate || denominator <= 1e-12 {
            break;
        }
        junction = numerator.map(|value| value / denominator);
    }
    junction
}

#[derive(Clone, Debug)]
pub(crate) struct TerminalCcoRequest {
    terminal_id: u32,
    physical_contact_id: u64,
    stub: MemberKey,
    stub_snapshot: Member,
    attach_point_mm: [Finite; 3],
    attach_radius_mm: PositiveFinite,
    connector_owner: Owner,
    frame: BuildFrame,
}

impl TerminalCcoRequest {
    pub(crate) fn bind(
        forest: &Forest,
        terminal_id: u32,
        frame: BuildFrame,
    ) -> Result<Self, CcoError> {
        let stub = forest
            .terminal(terminal_id)
            .ok_or(CcoError::InvalidTerminalStub {
                terminal_id,
                reason: "terminal map has no live member",
            })?;
        let stub_snapshot = forest
            .member(stub)
            .ok_or(CcoError::InvalidTerminalStub {
                terminal_id,
                reason: "terminal key is stale",
            })?
            .clone();
        validate_terminal_stub(forest, terminal_id, stub, &stub_snapshot)?;
        let ledger = forest
            .terminal_ledger(terminal_id)
            .filter(|entry| entry.original_key == stub && entry.state == TerminalLedgerState::Live)
            .ok_or(CcoError::InvalidTerminalStub {
                terminal_id,
                reason: "terminal ledger does not bind the live stub",
            })?;
        let Owner::Contact(owner_contact_id) = stub_snapshot.owner() else {
            return Err(CcoError::InvalidTerminalStub {
                terminal_id,
                reason: "stub owner is not a physical contact",
            });
        };
        if ledger.contact != *owner_contact_id {
            return Err(CcoError::InvalidTerminalStub {
                terminal_id,
                reason: "terminal ledger and stub owner disagree",
            });
        }
        Ok(Self {
            terminal_id,
            physical_contact_id: u64::from(*owner_contact_id),
            stub,
            attach_point_mm: stub_snapshot.p1_mm(),
            // p0 is the terminalward embed. CCO joins the registered stub at
            // its rootward p1 end, so preserve that end's radius verbatim.
            attach_radius_mm: stub_snapshot.rootward_radius_mm().get(),
            connector_owner: stub_snapshot.owner().clone(),
            stub_snapshot,
            frame,
        })
    }

    fn validate(&self, forest: &Forest) -> Result<(), CcoError> {
        let ledger_matches = forest
            .terminal_ledger(self.terminal_id)
            .is_some_and(|entry| {
                entry.contact == self.terminal_id
                    && entry.original_key == self.stub
                    && entry.state == TerminalLedgerState::Live
            });
        let owner_matches = matches!(
            self.stub_snapshot.owner(),
            Owner::Contact(owner_contact_id)
                if u64::from(*owner_contact_id) == self.physical_contact_id
        );
        if !ledger_matches
            || !owner_matches
            || forest.terminal(self.terminal_id) != Some(self.stub)
            || forest.member(self.stub) != Some(&self.stub_snapshot)
        {
            return Err(CcoError::StaleTerminal(self.terminal_id));
        }
        validate_terminal_stub(forest, self.terminal_id, self.stub, &self.stub_snapshot)
    }

    fn stub_end_mm(&self) -> [Finite; 3] {
        self.attach_point_mm
    }

    fn carried_mm2(&self) -> Finite {
        self.stub_snapshot.carried_mm2()
    }

    fn tip_radius_mm(&self) -> PositiveFinite {
        self.attach_radius_mm
    }

    /// The immutable physical-contact tip radius is distinct from the
    /// rootward attachment radius used by CCO growth.  Finalized load
    /// authority certifies the former; conflating them rejects valid initial
    /// stubs whenever the shoot-radius floor widens the attachment end.
    fn finalized_tip_radius_mm(&self) -> PositiveFinite {
        self.stub_snapshot.terminal_radius_mm().get()
    }

    fn plate_z_mm(&self) -> Finite {
        self.frame.plate_z_mm()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RewireSpliceFacts {
    DetachedSubtree {
        root: MemberKey,
        old_parent: MemberKey,
        carried_mm2: Finite,
    },
    RootReanchor {
        retiring_root: MemberKey,
        own_children: Vec<MemberKey>,
        fixed_point_mm: [Finite; 3],
    },
}

/// Unforgeable immutable candidate checkpoint for one current rewire root.
/// Geometry, radii, carried load, subtree membership, brace exclusions, and
/// splice facts are all derived from one exact generational Forest snapshot.
#[derive(Clone, Debug)]
pub(crate) struct RewireCcoBinding {
    snapshot: Forest,
    snapshot_digest: u64,
    live_digest: u64,
    request: TerminalCcoRequest,
    root_snapshot: Member,
    subtree: BTreeSet<MemberKey>,
    excluded_hosts: BTreeSet<MemberKey>,
    splice: RewireSpliceFacts,
}

#[derive(Clone, Debug, PartialEq)]
struct RewireBindingIdentity {
    root: MemberKey,
    root_snapshot: Member,
    live_digest: u64,
    snapshot_digest: u64,
    attach_point_mm: [Finite; 3],
    attach_radius_mm: PositiveFinite,
    frame: BuildFrame,
    subtree: BTreeSet<MemberKey>,
    excluded_hosts: BTreeSet<MemberKey>,
    splice: RewireSpliceFacts,
}

pub(crate) struct RewireAnchorRoute<'a> {
    pub(crate) samples: &'a [SurfaceAnchorSample],
    pub(crate) delicacy_by_face: Option<&'a [f64]>,
    pub(crate) config: AnchorSearchConfig,
    pub(crate) geometry: &'a AnchorGeometryAuthority<'a>,
}

impl RewireCcoBinding {
    pub(crate) fn bind_detached_subtree(
        forest: &Forest,
        root: MemberKey,
        frame: BuildFrame,
    ) -> Result<Self, CcoError> {
        let root_member = validate_rewire_root(forest, root)?;
        let old_parent = forest.parent_of(root).ok_or(CcoError::InvalidRewireRoot {
            root,
            reason: "detached-subtree root has no parent",
        })?;
        if forest.children_of(old_parent).len() < 2 {
            return Err(CcoError::InvalidRewireRoot {
                root,
                reason: "detachment would leave a pass-through parent",
            });
        }
        Self::bind(
            forest,
            root,
            root_member.p1_mm(),
            root_member.rootward_radius_mm().get(),
            frame,
            Some(old_parent),
        )
    }

    pub(crate) fn bind_root_reanchor(
        forest: &Forest,
        root: MemberKey,
        frame: BuildFrame,
    ) -> Result<Self, CcoError> {
        let root_member = validate_rewire_root(forest, root)?;
        if forest.parent_of(root).is_some() {
            return Err(CcoError::InvalidRewireRoot {
                root,
                reason: "root-reanchor binding is not parentless",
            });
        }
        Self::bind(
            forest,
            root,
            root_member.p0_mm(),
            root_member.terminal_radius_mm().get(),
            frame,
            None,
        )
    }

    fn bind(
        forest: &Forest,
        root: MemberKey,
        attach_point_mm: [Finite; 3],
        attach_radius_mm: PositiveFinite,
        frame: BuildFrame,
        old_parent: Option<MemberKey>,
    ) -> Result<Self, CcoError> {
        let root_snapshot = forest
            .member(root)
            .ok_or(CcoError::InvalidRewireRoot {
                root,
                reason: "rewire root is stale",
            })?
            .clone();
        let subtree = subtree_members(forest, root);
        let braces = forest.braces().into_iter().collect::<BTreeSet<_>>();
        let braced_hosts = derive_brace_attachments(forest)?
            .into_iter()
            .filter_map(|(host, attachments)| (!attachments.is_empty()).then_some(host))
            .collect::<BTreeSet<_>>();
        if old_parent.is_none() && braced_hosts.contains(&root) {
            return Err(CcoError::InvalidRewireRoot {
                root,
                reason: "root-reanchor would retire a braced host",
            });
        }
        let mut excluded_hosts = subtree.clone();
        excluded_hosts.extend(forest.dead().iter().copied());
        excluded_hosts.extend(braces);
        excluded_hosts.extend(braced_hosts);
        if let Some(parent) = old_parent {
            excluded_hosts.insert(parent);
        }
        let own_children = forest.children_of(root);
        let splice = old_parent.map_or_else(
            || RewireSpliceFacts::RootReanchor {
                retiring_root: root,
                own_children,
                fixed_point_mm: attach_point_mm,
            },
            |old_parent| RewireSpliceFacts::DetachedSubtree {
                root,
                old_parent,
                carried_mm2: root_snapshot.carried_mm2(),
            },
        );
        let request = TerminalCcoRequest {
            terminal_id: root.slot(),
            physical_contact_id: u64::from(root.slot()),
            stub: root,
            stub_snapshot: root_snapshot.clone(),
            attach_point_mm,
            attach_radius_mm,
            connector_owner: Owner::Contact(root.slot()),
            frame,
        };
        let mut snapshot = forest.clone();
        if let Some(parent) = old_parent {
            snapshot.set_parent(root, None)?;
            slim_rewire_ancestors(
                &mut snapshot,
                parent,
                root_snapshot.carried_mm2(),
                &Config::current()?,
            )?;
        }
        let snapshot_digest = snapshot.digest();
        Ok(Self {
            snapshot,
            snapshot_digest,
            live_digest: forest.digest(),
            request,
            root_snapshot,
            subtree,
            excluded_hosts,
            splice,
        })
    }

    pub(crate) fn validate_live(&self, forest: &Forest) -> Result<(), CcoError> {
        let root = self.request.stub;
        if forest.member(root) != Some(&self.root_snapshot) {
            return Err(CcoError::StaleRewireBinding(root));
        }
        match &self.splice {
            RewireSpliceFacts::DetachedSubtree { old_parent, .. } => {
                if forest.parent_of(root) != Some(*old_parent) {
                    return Err(CcoError::ReanchorBindingMismatch(root));
                }
            }
            RewireSpliceFacts::RootReanchor { own_children, .. } => {
                if forest.parent_of(root).is_some() || forest.children_of(root) != *own_children {
                    return Err(CcoError::ReanchorBindingMismatch(root));
                }
            }
        }
        if forest.digest() != self.live_digest || subtree_members(forest, root) != self.subtree {
            return Err(CcoError::StaleRewireBinding(root));
        }
        Ok(())
    }

    pub(crate) fn splice(&self) -> &RewireSpliceFacts {
        &self.splice
    }

    pub(crate) fn excluded_hosts(&self) -> &BTreeSet<MemberKey> {
        &self.excluded_hosts
    }

    fn anchor_binding(&self) -> RewireAnchorBinding {
        RewireAnchorBinding::new(
            self.request.stub,
            self.live_digest,
            self.snapshot_digest,
            self.request.attach_point_mm,
        )
    }

    fn identity(&self) -> RewireBindingIdentity {
        RewireBindingIdentity {
            root: self.request.stub,
            root_snapshot: self.root_snapshot.clone(),
            live_digest: self.live_digest,
            snapshot_digest: self.snapshot_digest,
            attach_point_mm: self.request.attach_point_mm,
            attach_radius_mm: self.request.attach_radius_mm,
            frame: self.request.frame,
            subtree: self.subtree.clone(),
            excluded_hosts: self.excluded_hosts.clone(),
            splice: self.splice.clone(),
        }
    }
}

/// Exact `rewire.py::_slim_ancestors` preparation of the immutable
/// detached-subtree checkpoint. Candidate evaluation starts from this one
/// prepared base so every descriptor sees identical old-path slimming.
fn slim_rewire_ancestors(
    forest: &mut Forest,
    mut key: MemberKey,
    removed_carried_mm2: Finite,
    config: &Config,
) -> Result<(), CcoError> {
    loop {
        let member = forest
            .member(key)
            .ok_or(ForestError::InactiveKey(key))?
            .clone();
        let carried =
            Finite::new((member.carried_mm2().get() - removed_carried_mm2.get()).max(0.0))
                .map_err(|_| CcoError::InvalidGeometry)?;
        forest.set_carried_mm2(key, carried)?;
        let murray = forest
            .children_of(key)
            .into_iter()
            .filter_map(|child| forest.member(child))
            .map(|child| child.rootward_radius_mm().get().get().powi(4))
            .sum::<f64>()
            .powf(0.25);
        let kind_floor = if matches!(member.kind(), MemberKind::Trunk) {
            config.min_trunk_radius_mm
        } else {
            config.min_shoot_radius_mm
        };
        let own_floor = o10_uniform_radius_for_span(
            member.p0_mm().map(Finite::get),
            member.p1_mm().map(Finite::get),
            member.kind().clone(),
            kind_floor,
            carried,
        )?;
        let radius = PositiveFinite::new(murray.max(own_floor.get()))
            .map_err(|_| CcoError::InvalidGeometry)?;
        forest.set_radii(
            key,
            TerminalEndRadius::new(radius),
            RootwardEndRadius::new(radius),
        )?;
        let Some(parent) = forest.parent_of(key) else {
            break;
        };
        key = parent;
    }
    Ok(())
}

fn validate_rewire_root(forest: &Forest, root: MemberKey) -> Result<&Member, CcoError> {
    let member = forest.member(root).ok_or(CcoError::InvalidRewireRoot {
        root,
        reason: "rewire root is not live",
    })?;
    if !forest.live_tree_members().contains(&root) {
        return Err(CcoError::InvalidRewireRoot {
            root,
            reason: "rewire root is a brace",
        });
    }
    if matches!(member.kind(), MemberKind::Stub) {
        return Err(CcoError::InvalidRewireRoot {
            root,
            reason: "rewire root is a terminal stub",
        });
    }
    if member.carried_mm2().get() < 0.0 {
        return Err(CcoError::InvalidRewireRoot {
            root,
            reason: "rewire root carried area is negative",
        });
    }
    Ok(member)
}

fn subtree_members(forest: &Forest, root: MemberKey) -> BTreeSet<MemberKey> {
    let mut subtree = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        if !subtree.insert(key) {
            continue;
        }
        stack.extend(forest.children_of(key));
    }
    subtree
}

fn validate_terminal_stub(
    forest: &Forest,
    terminal_id: u32,
    stub: MemberKey,
    member: &Member,
) -> Result<(), CcoError> {
    let invalid = |reason| CcoError::InvalidTerminalStub {
        terminal_id,
        reason,
    };
    if !matches!(member.kind(), MemberKind::Stub) {
        return Err(invalid("terminal member is not a stub"));
    }
    if member.owner() != &Owner::Contact(terminal_id) {
        return Err(invalid("stub owner does not match terminal"));
    }
    if member.embed_end() != Some(EmbedEnd::Terminalward) {
        return Err(invalid("stub is not terminalward-embedded"));
    }
    if forest.parent_of(stub).is_some() {
        return Err(invalid("stub is already parented"));
    }
    if norm(sub(
        member.p1_mm().map(Finite::get),
        member.p0_mm().map(Finite::get),
    )) <= DEGENERATE_MM
    {
        return Err(invalid("stub endpoints are degenerate"));
    }
    if member.carried_mm2().get() < 0.0 {
        return Err(invalid("stub carried area is negative"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CandidateFamily {
    PlateDescent,
    ModelAnchor,
    Bifurcation,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CcoCandidateEvidence {
    pub(crate) family: CandidateFamily,
    pub(crate) generation_index: usize,
    pub(crate) price_ml: f64,
    pub(crate) clearance_rows: usize,
}

#[derive(Debug)]
pub(crate) struct TerminalCcoOutcome {
    pub(crate) winner: Option<CcoCandidateEvidence>,
    /// Assigned only by the controller after the live terminal has either
    /// committed its exact replay or been retired as an unrouted terminal.
    pub(crate) disposition: Option<TerminalDisposition>,
    pub(crate) candidates_generated: usize,
    pub(crate) candidates_feasible: usize,
    pub(crate) anchor_ray_batches: usize,
    pub(crate) anchor_foot_batches: usize,
    pub(crate) clearance_batches: usize,
    pub(crate) validated_model_anchor: Option<Box<ValidatedModelAnchor>>,
    pub(crate) anchor_seat_refusals: Vec<AnchorSeatRefusal>,
    pub(crate) anchor_refusals: Vec<AnchorFootRefusal>,
}

/// Exact finalized-authority identity retained for a terminal disposition.
/// `physical_contact_id` is intentionally distinct from the u32 terminal namespace so
/// later routing normalization can partition demand without reconstructing a
/// contact from an equal-load stub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TerminalRouteEvidence {
    pub(crate) physical_contact_id: u64,
    pub(crate) terminal_id: u32,
    pub(crate) original_key: MemberKey,
    pub(crate) plan_digest: u64,
    pub(crate) loads_digest: u64,
    pub(crate) geometry_digest: u64,
    pub(crate) forest_digest_before: u64,
    pub(crate) controller_digest_before: u64,
}

/// A named, sealed explanation for an evaluated route with no committed
/// winner.  It records the point at which candidates disappeared without
/// mistaking a refusal for a successful route.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NoWinnerEvidence {
    NoCandidatesGenerated {
        anchor_seat_refusals: Vec<AnchorSeatRefusal>,
    },
    NoFeasibleCandidate {
        candidates_generated: usize,
        anchor_seat_refusals: Vec<AnchorSeatRefusal>,
        anchor_refusals: Vec<AnchorFootRefusal>,
    },
    AllFeasibleCandidatesRefusedByClearance {
        candidates_generated: usize,
        candidates_feasible: usize,
        anchor_seat_refusals: Vec<AnchorSeatRefusal>,
        anchor_refusals: Vec<AnchorFootRefusal>,
    },
}

impl NoWinnerEvidence {
    /// Read-only seat-refusal evidence retained when no route survives. This
    /// distinguishes a genuine empty model-anchor batch from later foot or
    /// clearance rejection without exposing a mutable candidate capability.
    #[must_use]
    pub(crate) fn anchor_seat_refusals(&self) -> &[AnchorSeatRefusal] {
        match self {
            Self::NoCandidatesGenerated {
                anchor_seat_refusals,
            }
            | Self::NoFeasibleCandidate {
                anchor_seat_refusals,
                ..
            }
            | Self::AllFeasibleCandidatesRefusedByClearance {
                anchor_seat_refusals,
                ..
            } => anchor_seat_refusals,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TerminalDisposition {
    Routed {
        route: TerminalRouteEvidence,
        winner: CcoCandidateEvidence,
    },
    SkippedQuarantined {
        terminal_id: u32,
        original_key: MemberKey,
        forest_digest: u64,
        controller_digest_before: u64,
    },
    UnroutedNoWinner {
        route: TerminalRouteEvidence,
        reason: NoWinnerEvidence,
    },
}

/// The only disposition vocabulary admitted into a finalized initial-route
/// transcript.  This deliberately omits mutable candidate and forest details:
/// normalization needs to distinguish retained demand, condemned contacts,
/// and honestly omitted demand, but it must not be able to manufacture either
/// claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FinalizedDispositionKind {
    Routed,
    SkippedQuarantined,
    UnroutedNoWinner,
}

/// Preserved quarantine cadence evidence.  Unlike an unrouted terminal, a
/// quarantined terminal never had a route request; these two frozen digests
/// are therefore the only truthful record of the exact forest/controller
/// state at its cadence advancement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FinalizedSkippedEvidence {
    forest_digest: u64,
    controller_digest_before: u64,
}

impl FinalizedSkippedEvidence {
    #[must_use]
    pub(crate) const fn forest_digest(&self) -> u64 {
        self.forest_digest
    }

    #[must_use]
    pub(crate) const fn controller_digest_before(&self) -> u64 {
        self.controller_digest_before
    }
}

/// One immutable, ordered initial-terminal disposition.  There is no
/// constructor outside this module: rows are only copied from a controller
/// cadence after their terminal lifecycle has been checked against the live
/// forest and the original initial-contact build.
#[derive(Clone, Debug)]
pub(crate) struct FinalizedCcoTranscriptRow {
    cadence_index: usize,
    physical_contact_id: u64,
    terminal_id: u32,
    original_key: MemberKey,
    kind: FinalizedDispositionKind,
    route: Option<TerminalRouteEvidence>,
    skipped: Option<FinalizedSkippedEvidence>,
    no_winner_reason: Option<NoWinnerEvidence>,
}

impl FinalizedCcoTranscriptRow {
    #[must_use]
    pub(crate) const fn physical_contact_id(&self) -> u64 {
        self.physical_contact_id
    }

    #[must_use]
    pub(crate) const fn cadence_index(&self) -> usize {
        self.cadence_index
    }

    #[must_use]
    pub(crate) const fn terminal_id(&self) -> u32 {
        self.terminal_id
    }

    #[must_use]
    pub(crate) const fn original_key(&self) -> MemberKey {
        self.original_key
    }

    #[must_use]
    pub(crate) const fn kind(&self) -> FinalizedDispositionKind {
        self.kind
    }

    /// Exact route binding for retained and no-winner contacts.  A skipped
    /// quarantined contact intentionally has no route: its initial quarantine
    /// record is its complete evidence.
    #[must_use]
    pub(crate) fn route(&self) -> Option<&TerminalRouteEvidence> {
        self.route.as_ref()
    }

    #[must_use]
    pub(crate) const fn skipped_evidence(&self) -> Option<FinalizedSkippedEvidence> {
        self.skipped
    }

    #[must_use]
    pub(crate) fn no_winner_reason(&self) -> Option<&NoWinnerEvidence> {
        self.no_winner_reason.as_ref()
    }
}

/// Opaque authority joining the initial terminal namespace to the terminal
/// growth cadence that actually consumed it.  The final support normalizer
/// receives this value rather than independently reconstructing disposition
/// evidence from a mutable forest or controller map.
#[derive(Clone, Debug)]
pub(crate) struct FinalizedCcoTranscript {
    plan_digest: u64,
    finalized_load_digest: u64,
    load_partition_digest: u64,
    geometry_digest: u64,
    initial_forest_digest: u64,
    initial_post_quarantine_forest_digest: u64,
    final_forest_digest: u64,
    final_controller_digest: u64,
    ordered_disposition_digest: u64,
    transcript_digest: u64,
    rows: Vec<FinalizedCcoTranscriptRow>,
}

impl FinalizedCcoTranscript {
    #[must_use]
    pub(crate) const fn plan_digest(&self) -> u64 {
        self.plan_digest
    }

    #[must_use]
    pub(crate) const fn finalized_load_digest(&self) -> u64 {
        self.finalized_load_digest
    }

    #[must_use]
    pub(crate) const fn load_partition_digest(&self) -> u64 {
        self.load_partition_digest
    }

    #[must_use]
    pub(crate) const fn geometry_digest(&self) -> u64 {
        self.geometry_digest
    }

    #[must_use]
    pub(crate) const fn initial_forest_digest(&self) -> u64 {
        self.initial_forest_digest
    }

    #[must_use]
    pub(crate) const fn initial_post_quarantine_forest_digest(&self) -> u64 {
        self.initial_post_quarantine_forest_digest
    }

    #[must_use]
    pub(crate) const fn final_forest_digest(&self) -> u64 {
        self.final_forest_digest
    }

    #[must_use]
    pub(crate) const fn final_controller_digest(&self) -> u64 {
        self.final_controller_digest
    }

    #[must_use]
    pub(crate) const fn ordered_disposition_digest(&self) -> u64 {
        self.ordered_disposition_digest
    }

    /// Stable digest over every immutable header and row.  Consumers that
    /// retain this authority across a normalization boundary can cheaply
    /// reject an internally corrupted copy before treating it as evidence.
    #[must_use]
    pub(crate) const fn digest(&self) -> u64 {
        self.transcript_digest
    }

    pub(crate) fn validate_integrity(&self) -> Result<(), CcoError> {
        if self.ordered_disposition_digest != transcript_rows_digest(&self.rows)
            || self.transcript_digest != finalized_transcript_digest(self)
        {
            return Err(CcoError::TranscriptDigestMismatch);
        }
        Ok(())
    }

    #[cfg(test)]
    /// Test-only seam that preserves the transcript's self-digest while
    /// breaking its binding to retained initial-build evidence.
    pub(crate) fn test_rebind_initial_forest_digest(&mut self) {
        self.initial_forest_digest = self.initial_forest_digest.wrapping_add(1);
        self.transcript_digest = finalized_transcript_digest(self);
    }

    #[cfg(test)]
    /// Test-only seam that preserves the transcript's self-digest while
    /// breaking its binding to retained initial-build evidence.
    pub(crate) fn test_rebind_initial_post_quarantine_forest_digest(&mut self) {
        self.initial_post_quarantine_forest_digest =
            self.initial_post_quarantine_forest_digest.wrapping_add(1);
        self.transcript_digest = finalized_transcript_digest(self);
    }

    #[must_use]
    pub(crate) fn rows(&self) -> &[FinalizedCcoTranscriptRow] {
        &self.rows
    }
}

#[derive(Debug, Error)]
pub(crate) enum CcoError {
    #[error("registered current-support constants unavailable: {0}")]
    Constants(String),
    #[error("terminal {0} is not bound to its requested live stub")]
    StaleTerminal(u32),
    #[error("terminal {terminal_id} has invalid CCO stub state: {reason}")]
    InvalidTerminalStub {
        terminal_id: u32,
        reason: &'static str,
    },
    #[error("anchor route is not the exact binding used for this CCO request")]
    AnchorBindingMismatch,
    #[error("finalized anchor contact does not belong exactly to its supplied plan")]
    FinalizedContactMismatch,
    #[error("finalized contact loads do not bind exactly to the supplied plan and contact ids")]
    FinalizedLoadBindingMismatch,
    #[error("terminal stub does not carry the exact finalized contact-load leaf")]
    FinalizedStubLoadMismatch,
    #[error("finalized anchor route uses a stale anchor-search configuration")]
    StaleFinalizedAnchorConfig,
    #[error("finalized anchor authority changed before CCO replay")]
    StaleFinalizedAnchorAuthority,
    #[error("rewire root {root:?} is invalid: {reason}")]
    InvalidRewireRoot {
        root: MemberKey,
        reason: &'static str,
    },
    #[error("rewire binding for root {0:?} no longer matches live generational state")]
    StaleRewireBinding(MemberKey),
    #[error("rewire splice binding for root {0:?} no longer matches its reanchor facts")]
    ReanchorBindingMismatch(MemberKey),
    #[error("rewire candidate batch does not belong to this exact binding")]
    RewireBatchBindingMismatch,
    #[error("rewire candidate generation index {0} is not a survivor in this batch")]
    RewireCandidateNotFound(usize),
    #[error("rewire candidate evidence does not reproduce exactly")]
    RewireCandidateEvidenceMismatch,
    #[error("rewire replay geometry authority differs from the evaluated authority")]
    RewireGeometryAuthorityMismatch,
    #[error("rewire replay no longer satisfies its evaluated clearance proof")]
    RewireClearanceReplayFailed,
    #[error("terminal {0} is not current quarantined skipped evidence")]
    InvalidSkippedTerminal(u32),
    #[error("terminal {0} already advanced the CCO terminal cadence")]
    TerminalCadenceReplay(u32),
    #[error("CCO terminal controller is sealed by a finalized transcript")]
    TranscriptSealed,
    #[error("initial-contact build does not bind this exact specialized plan")]
    TranscriptPlanMismatch,
    #[error("initial-contact build does not bind this exact finalized load authority")]
    TranscriptLoadMismatch,
    #[error("initial-contact build does not bind this exact geometry authority")]
    TranscriptGeometryMismatch,
    #[error("initial-contact build has an invalid fresh-forest authority")]
    TranscriptInitialForestMismatch,
    #[error("initial-contact terminal namespace or route cadence is incomplete or reordered")]
    TranscriptCadenceMismatch,
    #[error("terminal {0} has a disposition inconsistent with its initial or final forest state")]
    TranscriptDispositionMismatch(u32),
    #[error("finalized CCO transcript headers or rows do not reproduce its sealed digest")]
    TranscriptDigestMismatch,
    #[error("CCO input or derived geometry is not finite and positive where required")]
    InvalidGeometry,
    #[error(transparent)]
    Forest(#[from] ForestError),
    #[error(transparent)]
    Sizing(#[from] LegacyCompatibilityError),
    #[error("native clearance batch failed: {0}")]
    Native(String),
    #[error("serial winner re-application did not reproduce its evaluated forest")]
    NonDeterministicWinner,
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Transient(#[from] TransientError),
}

#[derive(Clone, Debug)]
enum Descriptor {
    Plate {
        xy: [f64; 2],
        top_z: f64,
    },
    ModelAnchor {
        seat_index: usize,
        point_mm: [f64; 3],
    },
    Bifurcate {
        host: MemberKey,
    },
}

impl Descriptor {
    fn family(&self) -> CandidateFamily {
        match self {
            Self::Plate { .. } => CandidateFamily::PlateDescent,
            Self::ModelAnchor { .. } => CandidateFamily::ModelAnchor,
            Self::Bifurcate { .. } => CandidateFamily::Bifurcation,
        }
    }
}

#[derive(Clone, Debug)]
struct EvaluatedCandidate {
    forest: Forest,
    family: CandidateFamily,
    generation_index: usize,
    price_ml: f64,
    rows: Vec<ClearanceRow>,
    anchor: Option<(usize, PositiveFinite)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RewireCandidateRefusalReason {
    InfeasibleGeometry,
    DegenerateNewMember,
    AnchorFoot(AnchorFootRefusalReason),
    Clearance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RewireCandidateRefusal {
    pub(crate) generation_index: usize,
    pub(crate) family: CandidateFamily,
    pub(crate) reason: RewireCandidateRefusalReason,
}

#[derive(Debug)]
pub(crate) struct RewireEvaluatedCandidate {
    descriptor: Descriptor,
    evidence: CcoCandidateEvidence,
    evaluated_digest: u64,
    validated_model_anchor: Option<Box<ValidatedModelAnchor>>,
}

#[derive(Debug)]
pub(crate) struct RewireCandidateBatch {
    candidates: Vec<RewireEvaluatedCandidate>,
    refusals: Vec<RewireCandidateRefusal>,
    descriptors_generated: usize,
    clearance_batches: usize,
    anchor_ray_batches: usize,
    anchor_foot_batches: usize,
    anchor_seat_refusals: Vec<AnchorSeatRefusal>,
    splice: RewireSpliceFacts,
    excluded_hosts: BTreeSet<MemberKey>,
    snapshot_digest: u64,
    binding_identity: RewireBindingIdentity,
    geometry_fingerprint: GeometryFingerprint,
}

/// Move-only proof that one batch survivor was replayed exactly against its
/// still-current live binding and original surface authority.
#[derive(Debug)]
pub(crate) struct RewireReplay {
    forest: Forest,
    splice: RewireSpliceFacts,
    evidence: CcoCandidateEvidence,
    validated_model_anchor: Option<Box<ValidatedModelAnchor>>,
    binding_identity: RewireBindingIdentity,
}

impl RewireReplay {
    pub(crate) fn into_parts(
        self,
    ) -> (
        Forest,
        RewireSpliceFacts,
        CcoCandidateEvidence,
        Option<Box<ValidatedModelAnchor>>,
    ) {
        (
            self.forest,
            self.splice,
            self.evidence,
            self.validated_model_anchor,
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct ClearanceRow {
    p0: [f64; 3],
    p1: [f64; 3],
    r0: f64,
    r1: f64,
    inset0: f64,
    inset1: f64,
}

#[derive(Clone, Debug)]
struct Config {
    contact_depth_mm: f64,
    standoff_mm: f64,
    min_shoot_radius_mm: PositiveFinite,
    min_trunk_radius_mm: PositiveFinite,
    branch_angle_deg: f64,
    max_segment_angle_deg: f64,
    cone_tolerance_deg: f64,
    fermat_iterations: usize,
    minimum_split_mm: f64,
    split_start: f64,
    split_end: f64,
    split_step: f64,
    split_steps: usize,
    bifurcation_hosts: usize,
    trunk_directions: usize,
    trunk_radii_mm: Vec<f64>,
    brace_neighbor_max_xy_mm: f64,
    brace_radius_mm: f64,
}

impl Config {
    fn current() -> Result<Self, CcoError> {
        let constants = m0_constants().map_err(|error| CcoError::Constants(error.to_string()))?;
        let p = constants.placeholders();
        let slide = p.cco_split_slide_policy();
        if p.cco_xia_alpha().diagnostic().get() != 0.5 {
            return Err(CcoError::Constants(
                "live CCO port requires registered Xia alpha=0.5".to_owned(),
            ));
        }
        Ok(Self {
            contact_depth_mm: p.default_contact_depth_mm().diagnostic().get(),
            standoff_mm: p.standoff_mm().diagnostic().get(),
            min_shoot_radius_mm: *p.min_shoot_r_mm().diagnostic(),
            min_trunk_radius_mm: *p.min_trunk_r_mm().diagnostic(),
            branch_angle_deg: p.branch_angle_deg().diagnostic().get(),
            max_segment_angle_deg: p.max_segment_angle_deg().diagnostic().get(),
            cone_tolerance_deg: p.cco_cone_tolerance_deg().diagnostic().get(),
            fermat_iterations: p.cco_fermat_max_iterations().diagnostic().get() as usize,
            minimum_split_mm: p.cco_min_split_length_mm().diagnostic().get(),
            split_start: slide.diagnostic().start(),
            split_end: slide.diagnostic().end(),
            split_step: slide.diagnostic().step(),
            split_steps: slide.diagnostic().cap().get() as usize,
            bifurcation_hosts: p.cco_bifurcation_hosts().diagnostic().get() as usize,
            trunk_directions: p.trunk_search_directions().diagnostic().get() as usize,
            trunk_radii_mm: p
                .trunk_search_radii_mm()
                .diagnostic()
                .iter()
                .map(|value| value.get())
                .collect(),
            brace_neighbor_max_xy_mm: p.brace_neighbor_max_xy_mm().diagnostic().get(),
            brace_radius_mm: p.brace_r_mm().diagnostic().get(),
        })
    }
}

/// The only production terminal-anchor route.  It starts from the finalized
/// physical-contact authority and the canonical exterior pool; callers never
/// hand CCO a selected-contact projection or an arbitrary seat collection.
pub(crate) struct FinalizedAnchorTerminalRoute<'a, 'surface> {
    pub(crate) plan: &'a SpecializedContactPlan,
    pub(crate) contact: &'a SpecializedPhysicalContact,
    pub(crate) contact_id: u64,
    pub(crate) loads: &'a FinalizedContactLoads,
    pub(crate) pool: &'a SurfaceAnchorPool<'surface>,
    pub(crate) config: AnchorSearchConfig,
    pub(crate) geometry: &'a AnchorGeometryAuthority<'surface>,
}

/// Immutable authority carried from discovery through the serial CCO replay.
/// The copied digests make a cross-plan/load/pool/surface rebound fail before
/// committing the selected candidate, without issuing another native query.
struct FinalizedAnchorValidationContext<'a, 'surface> {
    route: FinalizedAnchorTerminalRoute<'a, 'surface>,
    bound_contact: BoundPhysicalAnchorContact,
    terminal: AnchorTerminalBinding,
    plan_digest: u64,
    loads_digest: u64,
    pool_digest: u64,
    geometry_fingerprint: GeometryFingerprint,
}

#[derive(Default)]
/// The only integration seam retained for the pending support runner.
pub(crate) struct CcoController {
    hosts: HostCache,
    inserted: usize,
    terminals: BTreeMap<u32, TerminalCadenceRecord>,
    /// Chronological terminal cadence.  The map above is deliberately keyed
    /// for lookup, so it cannot prove that the frozen specialized/regular
    /// route order was actually observed.
    terminal_cadence: Vec<u32>,
    sealed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct TerminalCadenceRecord {
    disposition: TerminalDisposition,
}

#[derive(Default)]
struct HostCache {
    indexed_members: BTreeSet<MemberKey>,
    samples: Vec<([f64; 3], MemberKey)>,
}

impl CcoController {
    /// Refuse stale or unavailable registered CCO policy before a runner
    /// starts staging terminal work.  The actual value remains private to
    /// this controller; this is only the preflight boundary needed to ensure
    /// that policy failure cannot follow a native anchor query.
    pub(crate) fn validate_initial_route_config() -> Result<(), CcoError> {
        let _ = Config::current()?;
        Ok(())
    }

    /// Seal the complete initial-terminal route transaction.  The returned
    /// transcript is the sole authority a later normalizer may use for
    /// retained, quarantined, and no-winner contact disposition.
    ///
    /// This is intentionally a mutating seal rather than a snapshot: after a
    /// successful return no further initial terminal can be advanced, so a
    /// transcript cannot be replayed against a controller with hidden pending
    /// work.
    pub(crate) fn finalize_transcript(
        &mut self,
        build: &InitialContactBuild,
        plan: &SpecializedContactPlan,
        loads: &FinalizedContactLoads,
        geometry: &AnchorGeometryAuthority<'_>,
    ) -> Result<FinalizedCcoTranscript, CcoError> {
        if self.sealed {
            return Err(CcoError::TranscriptSealed);
        }

        let plan_digest = plan.digest();
        if build.plan_digest() != plan_digest || plan.finalized_load_plan_digest() != plan_digest {
            return Err(CcoError::TranscriptPlanMismatch);
        }
        let plan_loads = plan
            .finalized_loads()
            .map_err(|_| CcoError::TranscriptLoadMismatch)?;
        if build.load_digest() != loads.digest() || plan_loads != loads {
            return Err(CcoError::TranscriptLoadMismatch);
        }
        let geometry_digest = geometry.digest();
        if build.native_digest() != geometry_digest {
            return Err(CcoError::TranscriptGeometryMismatch);
        }
        if build.initial_forest_digest() != Forest::new().digest()
            || !build.all_terminals_registered_before_quarantine()
        {
            return Err(CcoError::TranscriptInitialForestMismatch);
        }

        let expected_cadence = expected_initial_cadence(build)?;
        if self.inserted != expected_cadence.len()
            || self.terminals.len() != expected_cadence.len()
            || self.terminal_cadence != expected_cadence
        {
            return Err(CcoError::TranscriptCadenceMismatch);
        }
        build.forest().validate()?;

        let mut rows = Vec::with_capacity(expected_cadence.len());
        for (cadence_index, &terminal) in expected_cadence.iter().enumerate() {
            let evidence = build
                .terminals()
                .iter()
                .find(|evidence| evidence.terminal == terminal)
                .ok_or(CcoError::TranscriptCadenceMismatch)?;
            let record = self
                .terminals
                .get(&terminal)
                .ok_or(CcoError::TranscriptCadenceMismatch)?;
            rows.push(finalize_transcript_row(
                build.forest(),
                evidence,
                &record.disposition,
                cadence_index,
                plan_digest,
                loads.digest(),
                geometry_digest,
            )?);
        }
        let ordered_disposition_digest = transcript_rows_digest(&rows);
        let final_forest_digest = build.forest().digest();

        // Only after every final-state and authority check succeeds does the
        // controller become unable to route another terminal.
        self.sealed = true;
        let final_controller_digest = self.digest();
        let mut transcript = FinalizedCcoTranscript {
            plan_digest,
            finalized_load_digest: loads.digest(),
            load_partition_digest: finalized_load_partition_digest(plan),
            geometry_digest,
            initial_forest_digest: build.initial_forest_digest(),
            initial_post_quarantine_forest_digest: build.post_quarantine_forest_digest(),
            final_forest_digest,
            final_controller_digest,
            ordered_disposition_digest,
            transcript_digest: 0,
            rows,
        };
        transcript.transcript_digest = finalized_transcript_digest(&transcript);
        Ok(transcript)
    }

    #[cfg(test)]
    fn grow_terminal_for_test(
        &mut self,
        forest: &mut Forest,
        request: &TerminalCcoRequest,
        surface: &NativeSurface,
    ) -> Result<TerminalCcoOutcome, CcoError> {
        self.require_unadvanced(request.terminal_id)?;
        let controller_digest_before = self.digest();
        let forest_digest_before = forest.digest();
        let config = Config::current()?;
        let hosts = self.hosts.nearest(
            request.stub_end_mm().map(Finite::get),
            config.bifurcation_hosts,
            forest,
        );
        let mut outcome = grow_terminal_inner(
            forest,
            request,
            surface,
            Vec::new(),
            Vec::new(),
            None,
            0,
            Some(hosts),
        )?;
        let route = TerminalRouteEvidence {
            physical_contact_id: request.physical_contact_id,
            terminal_id: request.terminal_id,
            original_key: request.stub,
            plan_digest: 0,
            loads_digest: 0,
            geometry_digest: 0,
            forest_digest_before,
            controller_digest_before,
        };
        let disposition = match outcome.winner.clone() {
            Some(winner) => TerminalDisposition::Routed { route, winner },
            None => {
                forest.retire_unrouted_terminal(request.terminal_id, request.stub)?;
                TerminalDisposition::UnroutedNoWinner {
                    route,
                    reason: no_winner_evidence(&outcome),
                }
            }
        };
        self.finish_terminal(
            forest,
            request.terminal_id,
            TerminalCadenceRecord {
                disposition: disposition.clone(),
            },
        );
        outcome.disposition = Some(disposition);
        Ok(outcome)
    }

    /// The sole crate-visible terminal-growth entry point.
    ///
    /// Production callers must supply the finalized anchor route validated by
    /// this method; raw terminal-growth helpers remain private or test-only.
    pub(crate) fn grow_terminal_with_finalized_anchor(
        &mut self,
        forest: &mut Forest,
        request: &TerminalCcoRequest,
        route: FinalizedAnchorTerminalRoute<'_, '_>,
    ) -> Result<TerminalCcoOutcome, CcoError> {
        self.require_unadvanced(request.terminal_id)?;
        let controller_digest_before = self.digest();
        let forest_digest_before = forest.digest();
        let context = FinalizedAnchorValidationContext::bind(forest, request, route)?;
        let discovery = discover_finalized_anchor_seats(
            context.bound_contact,
            context.terminal,
            context.route.pool,
            context.route.config,
            context.route.geometry,
        )?;
        let anchor_ray_batches = discovery.query_counts().ray_batches;
        let anchor_seat_refusals = discovery.refusals().to_vec();
        let config = Config::current()?;
        let hosts = self.hosts.nearest(
            request.stub_end_mm().map(Finite::get),
            config.bifurcation_hosts,
            forest,
        );
        let mut outcome = grow_terminal_inner(
            forest,
            request,
            context.route.geometry.surface(),
            discovery.into_seats(),
            anchor_seat_refusals,
            Some(&context),
            anchor_ray_batches,
            Some(hosts),
        )?;
        let disposition = match outcome.winner.clone() {
            Some(winner) => TerminalDisposition::Routed {
                // `grow_terminal_inner` validated this authority immediately
                // before committing the replayed winner.
                route: context.route_evidence(
                    request,
                    forest_digest_before,
                    controller_digest_before,
                ),
                winner,
            },
            None => {
                // A no-winner path still mutates by retiring the stub, so it
                // needs the same stale-authority gate before that transition.
                context.validate_replay(forest, request)?;
                forest.retire_unrouted_terminal(request.terminal_id, request.stub)?;
                TerminalDisposition::UnroutedNoWinner {
                    route: context.route_evidence(
                        request,
                        forest_digest_before,
                        controller_digest_before,
                    ),
                    reason: no_winner_evidence(&outcome),
                }
            }
        };
        self.finish_terminal(
            forest,
            request.terminal_id,
            TerminalCadenceRecord {
                disposition: disposition.clone(),
            },
        );
        outcome.disposition = Some(disposition);
        Ok(outcome)
    }

    /// Advance the same Python terminal cadence after a terminal has been
    /// explicitly quarantined. The ledger entry is the skipped-contact
    /// evidence; this method never attempts CCO growth or mutates the forest.
    pub(crate) fn advance_skipped_terminal(
        &mut self,
        forest: &Forest,
        skipped: TerminalLedgerEntry,
    ) -> Result<(), CcoError> {
        self.require_unadvanced(skipped.contact)?;
        if skipped.state != TerminalLedgerState::Quarantined
            || forest.terminal_ledger(skipped.contact) != Some(skipped)
        {
            return Err(CcoError::InvalidSkippedTerminal(skipped.contact));
        }
        self.finish_terminal(
            forest,
            skipped.contact,
            TerminalCadenceRecord {
                disposition: TerminalDisposition::SkippedQuarantined {
                    terminal_id: skipped.contact,
                    original_key: skipped.original_key,
                    forest_digest: forest.digest(),
                    controller_digest_before: self.digest(),
                },
            },
        );
        Ok(())
    }

    /// Stable sealed outcome for a terminal.  In particular, a normalizer can
    /// select `UnroutedNoWinner` by its exact finalized physical contact id
    /// without treating a retired member as a live stub.
    pub(crate) fn terminal_disposition(&self, terminal: u32) -> Option<&TerminalDisposition> {
        self.terminals
            .get(&terminal)
            .map(|record| &record.disposition)
    }

    /// Evaluate the frozen rewire candidate batch against its immutable
    /// forest checkpoint. This never mutates a live forest, selects a winner,
    /// or commits a move; rewire owns the later serial splice transaction.
    pub(crate) fn evaluate_rewire_candidates(
        &self,
        binding: &RewireCcoBinding,
        anchor_route: RewireAnchorRoute<'_>,
    ) -> Result<RewireCandidateBatch, CcoError> {
        let config = Config::current()?;
        let binding_identity = binding.identity();
        let geometry_fingerprint = anchor_route.geometry.fingerprint();
        let anchor_binding = binding.anchor_binding();
        let discovery = discover_rewire_anchor_seats(
            anchor_binding,
            anchor_route.samples,
            anchor_route.delicacy_by_face,
            anchor_route.config,
            anchor_route.geometry,
        )?;
        let anchor_ray_batches = discovery.query_counts().ray_batches;
        let anchor_seat_refusals = discovery.refusals().to_vec();
        let mut seats = discovery.into_seats();
        let hosts = self.hosts.nearest_excluding(
            binding.request.stub_end_mm().map(Finite::get),
            config.bifurcation_hosts,
            &binding.snapshot,
            &binding.excluded_hosts,
        );
        let descriptors = generate_descriptors(
            &binding.snapshot,
            &binding.request,
            &config,
            &seats,
            Some(hosts),
            false,
        );
        let descriptors_generated = descriptors.len();
        let evaluated =
            evaluate_descriptors(&binding.snapshot, &binding.request, &descriptors, &config)?;
        let mut refusals = Vec::new();
        let mut feasible = Vec::new();
        for (index, result) in evaluated.into_iter().enumerate() {
            if let Some(mut candidate) = result {
                if has_rewire_degenerate_new_member(&binding.snapshot, &candidate.forest) {
                    refusals.push(RewireCandidateRefusal {
                        generation_index: index,
                        family: descriptors[index].family(),
                        reason: RewireCandidateRefusalReason::DegenerateNewMember,
                    });
                    continue;
                }
                // Python rewire compares total live forest volume, not the
                // insertion-only plate/anchor heuristic. The prepared base is
                // common to the whole batch, so its exact volume delta is the
                // stable equivalent ranking value.
                candidate.price_ml =
                    changed_forest_volume_delta(&binding.snapshot, &candidate.forest)?;
                feasible.push(candidate);
            } else {
                refusals.push(RewireCandidateRefusal {
                    generation_index: index,
                    family: descriptors[index].family(),
                    reason: RewireCandidateRefusalReason::InfeasibleGeometry,
                });
            }
        }
        let mut validated_anchors = BTreeMap::new();
        let mut anchor_foot_batches = 0;
        if !seats.is_empty() {
            let mut seats: Vec<Option<QualifiedAnchorSeat>> = seats.drain(..).map(Some).collect();
            let mut generation_indices = Vec::new();
            let mut foot_requests = Vec::new();
            for candidate in &feasible {
                if let Some((seat_index, final_radius)) = candidate.anchor {
                    let seat = seats
                        .get_mut(seat_index)
                        .and_then(Option::take)
                        .ok_or(CcoError::InvalidGeometry)?;
                    generation_indices.push(candidate.generation_index);
                    foot_requests.push(AnchorFootRequest::new(seat, final_radius));
                }
            }
            let validation = validate_rewire_anchor_feet(
                foot_requests,
                anchor_binding,
                anchor_route.config,
                anchor_route.geometry,
            )?;
            anchor_foot_batches = validation.query_counts().sphere_breach_batches;
            let mut valid = BTreeSet::new();
            for (generation_index, outcome) in generation_indices
                .into_iter()
                .zip(validation.into_outcomes())
            {
                match outcome {
                    AnchorFootOutcome::Validated(anchor) => {
                        valid.insert(generation_index);
                        validated_anchors.insert(generation_index, anchor);
                    }
                    AnchorFootOutcome::Refused(refusal) => {
                        refusals.push(RewireCandidateRefusal {
                            generation_index,
                            family: descriptors[generation_index].family(),
                            reason: RewireCandidateRefusalReason::AnchorFoot(refusal.reason()),
                        });
                    }
                }
            }
            feasible.retain(|candidate| {
                candidate.anchor.is_none() || valid.contains(&candidate.generation_index)
            });
        }
        if feasible.is_empty() {
            sort_rewire_refusals(&mut refusals);
            return Ok(RewireCandidateBatch {
                candidates: Vec::new(),
                refusals,
                descriptors_generated,
                clearance_batches: 0,
                anchor_ray_batches,
                anchor_foot_batches,
                anchor_seat_refusals,
                splice: binding.splice.clone(),
                excluded_hosts: binding.excluded_hosts.clone(),
                snapshot_digest: binding.snapshot_digest,
                binding_identity,
                geometry_fingerprint,
            });
        }
        let survives = clearance_survival(anchor_route.geometry.surface(), &config, &feasible)?;
        let mut candidates = Vec::new();
        for (candidate, survives) in feasible.into_iter().zip(survives) {
            if survives {
                let generation_index = candidate.generation_index;
                candidates.push(RewireEvaluatedCandidate {
                    descriptor: descriptors[generation_index].clone(),
                    evidence: CcoCandidateEvidence {
                        family: candidate.family,
                        generation_index,
                        price_ml: candidate.price_ml,
                        clearance_rows: candidate.rows.len(),
                    },
                    evaluated_digest: candidate.forest.digest(),
                    validated_model_anchor: validated_anchors.remove(&generation_index),
                });
            } else {
                refusals.push(RewireCandidateRefusal {
                    generation_index: candidate.generation_index,
                    family: candidate.family,
                    reason: RewireCandidateRefusalReason::Clearance,
                });
            }
        }
        sort_rewire_refusals(&mut refusals);
        Ok(RewireCandidateBatch {
            candidates,
            refusals,
            descriptors_generated,
            clearance_batches: 1,
            anchor_ray_batches,
            anchor_foot_batches,
            anchor_seat_refusals,
            splice: binding.splice.clone(),
            excluded_hosts: binding.excluded_hosts.clone(),
            snapshot_digest: binding.snapshot_digest,
            binding_identity,
            geometry_fingerprint,
        })
    }

    fn require_unadvanced(&self, terminal: u32) -> Result<(), CcoError> {
        if self.sealed {
            Err(CcoError::TranscriptSealed)
        } else if self.terminals.contains_key(&terminal) {
            Err(CcoError::TerminalCadenceReplay(terminal))
        } else {
            Ok(())
        }
    }

    fn finish_terminal(&mut self, forest: &Forest, terminal: u32, record: TerminalCadenceRecord) {
        let previous = self.terminals.insert(terminal, record);
        assert!(previous.is_none(), "terminal cadence was preflighted");
        self.terminal_cadence.push(terminal);
        self.inserted += 1;
        assert_eq!(self.inserted, self.terminals.len());
        assert_eq!(self.inserted, self.terminal_cadence.len());
        if self.inserted.is_multiple_of(KDTREE_REBUILD_EVERY) {
            self.hosts.rebuild(forest);
        }
    }

    /// Stable evidence for terminal dispositions and the exact host-cache
    /// scheduling state they produced.
    pub(crate) fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut word = |value: u64| {
            for byte in value.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        word(self.inserted as u64);
        word(self.terminals.len() as u64);
        word(self.terminal_cadence.len() as u64);
        for terminal in &self.terminal_cadence {
            word(u64::from(*terminal));
        }
        word(u64::from(self.sealed));
        for (&terminal, record) in &self.terminals {
            word(u64::from(terminal));
            match &record.disposition {
                TerminalDisposition::Routed { route, winner } => {
                    word(0);
                    digest_route_evidence(&mut word, route);
                    word(candidate_family_digest(winner.family));
                    word(winner.generation_index as u64);
                    word(winner.price_ml.to_bits());
                    word(winner.clearance_rows as u64);
                }
                TerminalDisposition::SkippedQuarantined {
                    terminal_id,
                    original_key,
                    forest_digest,
                    controller_digest_before,
                } => {
                    word(1);
                    word(u64::from(*terminal_id));
                    word(u64::from(original_key.slot()));
                    word(u64::from(original_key.generation()));
                    word(*forest_digest);
                    word(*controller_digest_before);
                }
                TerminalDisposition::UnroutedNoWinner { route, reason } => {
                    word(2);
                    digest_route_evidence(&mut word, route);
                    match reason {
                        NoWinnerEvidence::NoCandidatesGenerated {
                            anchor_seat_refusals,
                        } => {
                            word(0);
                            digest_anchor_seat_refusals(&mut word, anchor_seat_refusals);
                        }
                        NoWinnerEvidence::NoFeasibleCandidate {
                            candidates_generated,
                            anchor_seat_refusals,
                            anchor_refusals,
                        } => {
                            word(1);
                            word(*candidates_generated as u64);
                            digest_anchor_seat_refusals(&mut word, anchor_seat_refusals);
                            digest_anchor_refusals(&mut word, anchor_refusals);
                        }
                        NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
                            candidates_generated,
                            candidates_feasible,
                            anchor_seat_refusals,
                            anchor_refusals,
                        } => {
                            word(2);
                            word(*candidates_generated as u64);
                            word(*candidates_feasible as u64);
                            digest_anchor_seat_refusals(&mut word, anchor_seat_refusals);
                            digest_anchor_refusals(&mut word, anchor_refusals);
                        }
                    }
                }
            }
        }
        word(self.hosts.indexed_members.len() as u64);
        for key in &self.hosts.indexed_members {
            word(u64::from(key.slot()));
            word(u64::from(key.generation()));
        }
        word(self.hosts.samples.len() as u64);
        for (point, key) in &self.hosts.samples {
            for coordinate in point {
                word(coordinate.to_bits());
            }
            word(u64::from(key.slot()));
            word(u64::from(key.generation()));
        }
        hash
    }
}

fn digest_route_evidence(word: &mut impl FnMut(u64), route: &TerminalRouteEvidence) {
    word(route.physical_contact_id);
    word(u64::from(route.terminal_id));
    word(u64::from(route.original_key.slot()));
    word(u64::from(route.original_key.generation()));
    word(route.plan_digest);
    word(route.loads_digest);
    word(route.geometry_digest);
    word(route.forest_digest_before);
    word(route.controller_digest_before);
}

fn expected_initial_cadence(build: &InitialContactBuild) -> Result<Vec<u32>, CcoError> {
    let routing_order = build.routing_order();
    let mut cadence = Vec::with_capacity(build.terminals().len());
    cadence.extend_from_slice(routing_order.specialized_terminals());
    cadence.extend_from_slice(routing_order.regular_terminals());
    if cadence.len() != build.terminals().len() {
        return Err(CcoError::TranscriptCadenceMismatch);
    }
    let cadence_set = cadence.iter().copied().collect::<BTreeSet<_>>();
    let evidence_set = build
        .terminals()
        .iter()
        .map(|evidence| evidence.terminal)
        .collect::<BTreeSet<_>>();
    if cadence_set.len() != cadence.len() || cadence_set != evidence_set {
        return Err(CcoError::TranscriptCadenceMismatch);
    }
    Ok(cadence)
}

fn finalize_transcript_row(
    forest: &Forest,
    initial: &InitialTerminalEvidence,
    disposition: &TerminalDisposition,
    cadence_index: usize,
    plan_digest: u64,
    loads_digest: u64,
    geometry_digest: u64,
) -> Result<FinalizedCcoTranscriptRow, CcoError> {
    let terminal = initial.terminal;
    let physical_contact_id = u64::from(terminal);
    let ledger = forest
        .terminal_ledger(terminal)
        .ok_or(CcoError::TranscriptDispositionMismatch(terminal))?;
    if ledger.original_key != initial.key || ledger.contact != terminal {
        return Err(CcoError::TranscriptDispositionMismatch(terminal));
    }

    let checked_route = |route: &TerminalRouteEvidence| {
        (route.physical_contact_id == physical_contact_id
            && route.terminal_id == terminal
            && route.original_key == initial.key
            && route.plan_digest == plan_digest
            && route.loads_digest == loads_digest
            && route.geometry_digest == geometry_digest)
            .then_some(())
            .ok_or(CcoError::TranscriptDispositionMismatch(terminal))
    };

    match disposition {
        TerminalDisposition::Routed { route, .. } => {
            checked_route(route)?;
            if initial.state != TerminalLedgerState::Live
                || ledger.state != TerminalLedgerState::Consumed
                || forest.terminal(terminal) != Some(initial.key)
                || forest.member(initial.key).is_none()
                // Every winning candidate consumes the immutable stub and
                // attaches it during its serial replay.  A merely consumed,
                // still-root stub is not a committed route.
                || forest.parent_of(initial.key).is_none()
            {
                return Err(CcoError::TranscriptDispositionMismatch(terminal));
            }
            Ok(FinalizedCcoTranscriptRow {
                cadence_index,
                physical_contact_id,
                terminal_id: terminal,
                original_key: initial.key,
                kind: FinalizedDispositionKind::Routed,
                route: Some(route.clone()),
                skipped: None,
                no_winner_reason: None,
            })
        }
        TerminalDisposition::SkippedQuarantined {
            terminal_id,
            original_key,
            forest_digest,
            controller_digest_before,
            ..
        } => {
            if *terminal_id != terminal
                || *original_key != initial.key
                || initial.state != TerminalLedgerState::Quarantined
                || ledger.state != TerminalLedgerState::Quarantined
                || forest.terminal(terminal).is_some()
                || forest.quarantine_record(initial.key).is_none()
            {
                return Err(CcoError::TranscriptDispositionMismatch(terminal));
            }
            Ok(FinalizedCcoTranscriptRow {
                cadence_index,
                physical_contact_id,
                terminal_id: terminal,
                original_key: initial.key,
                kind: FinalizedDispositionKind::SkippedQuarantined,
                route: None,
                skipped: Some(FinalizedSkippedEvidence {
                    forest_digest: *forest_digest,
                    controller_digest_before: *controller_digest_before,
                }),
                no_winner_reason: None,
            })
        }
        TerminalDisposition::UnroutedNoWinner { route, reason } => {
            checked_route(route)?;
            if initial.state != TerminalLedgerState::Live
                || ledger.state != TerminalLedgerState::Retired
                || forest.terminal(terminal).is_some()
                || forest.member(initial.key).is_some()
                || forest.quarantine_record(initial.key).is_some()
            {
                return Err(CcoError::TranscriptDispositionMismatch(terminal));
            }
            Ok(FinalizedCcoTranscriptRow {
                cadence_index,
                physical_contact_id,
                terminal_id: terminal,
                original_key: initial.key,
                kind: FinalizedDispositionKind::UnroutedNoWinner,
                route: Some(route.clone()),
                skipped: None,
                no_winner_reason: Some(reason.clone()),
            })
        }
    }
}

fn transcript_rows_digest(rows: &[FinalizedCcoTranscriptRow]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut word = |value: u64| {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    word(rows.len() as u64);
    for row in rows {
        word(row.cadence_index as u64);
        word(row.physical_contact_id);
        word(u64::from(row.terminal_id));
        word(u64::from(row.original_key.slot()));
        word(u64::from(row.original_key.generation()));
        word(match row.kind {
            FinalizedDispositionKind::Routed => 0,
            FinalizedDispositionKind::SkippedQuarantined => 1,
            FinalizedDispositionKind::UnroutedNoWinner => 2,
        });
        match &row.route {
            Some(route) => {
                word(1);
                digest_route_evidence(&mut word, route);
            }
            None => word(0),
        }
        match row.skipped {
            Some(skipped) => {
                word(1);
                word(skipped.forest_digest);
                word(skipped.controller_digest_before);
            }
            None => word(0),
        }
        match &row.no_winner_reason {
            None => word(0),
            Some(NoWinnerEvidence::NoCandidatesGenerated {
                anchor_seat_refusals,
            }) => {
                word(1);
                digest_anchor_seat_refusals(&mut word, anchor_seat_refusals);
            }
            Some(NoWinnerEvidence::NoFeasibleCandidate {
                candidates_generated,
                anchor_seat_refusals,
                anchor_refusals,
            }) => {
                word(2);
                word(*candidates_generated as u64);
                digest_anchor_seat_refusals(&mut word, anchor_seat_refusals);
                digest_anchor_refusals(&mut word, anchor_refusals);
            }
            Some(NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
                candidates_generated,
                candidates_feasible,
                anchor_seat_refusals,
                anchor_refusals,
            }) => {
                word(3);
                word(*candidates_generated as u64);
                word(*candidates_feasible as u64);
                digest_anchor_seat_refusals(&mut word, anchor_seat_refusals);
                digest_anchor_refusals(&mut word, anchor_refusals);
            }
        }
    }
    hash
}

fn finalized_transcript_digest(transcript: &FinalizedCcoTranscript) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut word = |value: u64| {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    word(transcript.plan_digest);
    word(transcript.finalized_load_digest);
    word(transcript.load_partition_digest);
    word(transcript.geometry_digest);
    word(transcript.initial_forest_digest);
    word(transcript.initial_post_quarantine_forest_digest);
    word(transcript.final_forest_digest);
    word(transcript.final_controller_digest);
    word(transcript.ordered_disposition_digest);
    word(transcript_rows_digest(&transcript.rows));
    hash
}

fn finalized_load_partition_digest(plan: &SpecializedContactPlan) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut word = |value: u64| {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    let rows = plan.load_partition().rows();
    word(rows.len() as u64);
    for row in rows {
        word(u64::from(row.face_id));
        word(row.contact_id);
        word(match row.owner {
            crate::perception::LoadOwner::Specialized(owner) => u64::from(owner) << 1,
            crate::perception::LoadOwner::GenericIsland(owner) => (u64::from(owner) << 1) | 1,
        });
        word(row.carried_area_mm2.to_bits());
    }
    hash
}

fn candidate_family_digest(family: CandidateFamily) -> u64 {
    match family {
        CandidateFamily::PlateDescent => 0,
        CandidateFamily::ModelAnchor => 1,
        CandidateFamily::Bifurcation => 2,
    }
}

fn digest_anchor_seat_refusals(word: &mut impl FnMut(u64), refusals: &[AnchorSeatRefusal]) {
    word(refusals.len() as u64);
    for refusal in refusals {
        word(refusal.sample_index as u64);
        word(u64::from(refusal.source_sample_face_id));
        word(match refusal.reason {
            crate::anchor::AnchorSeatRefusalReason::SampleNonFinite => 0,
            crate::anchor::AnchorSeatRefusalReason::SampleFaceOutOfRange => 1,
            crate::anchor::AnchorSeatRefusalReason::SampleDelicacyMismatch => 2,
            crate::anchor::AnchorSeatRefusalReason::AboveContact => 3,
            crate::anchor::AnchorSeatRefusalReason::TooShort => 4,
            crate::anchor::AnchorSeatRefusalReason::TooLong => 5,
            crate::anchor::AnchorSeatRefusalReason::TooDelicate => 6,
            crate::anchor::AnchorSeatRefusalReason::TooLean => 7,
            crate::anchor::AnchorSeatRefusalReason::NoFirstHit => 8,
            crate::anchor::AnchorSeatRefusalReason::ZeroLengthSnappedAnchor => 9,
            crate::anchor::AnchorSeatRefusalReason::NearerNonExteriorObstruction => 10,
        });
        word(refusal.first_hit_t.map_or(0, |value| value.get().to_bits()));
        word(u64::from(refusal.first_hit_t.is_some()));
    }
}

fn digest_anchor_refusals(word: &mut impl FnMut(u64), refusals: &[AnchorFootRefusal]) {
    word(refusals.len() as u64);
    for refusal in refusals {
        let provenance = refusal.provenance();
        word(match refusal.reason() {
            AnchorFootRefusalReason::ReboundContact => 0,
            AnchorFootRefusalReason::ReboundTerminal => 1,
            AnchorFootRefusalReason::ReboundRewireBinding => 2,
            AnchorFootRefusalReason::StaleConfig => 3,
            AnchorFootRefusalReason::StaleGeometry => 4,
            AnchorFootRefusalReason::StalePool => 5,
            AnchorFootRefusalReason::FootBallBreached => 6,
        });
        word(provenance.physical_contact_id().unwrap_or(u64::MAX));
        word(provenance.sample_index() as u64);
        word(u64::from(provenance.source_sample_face_id()));
    }
}

fn sort_rewire_refusals(refusals: &mut [RewireCandidateRefusal]) {
    refusals.sort_by_key(|refusal| refusal.generation_index);
}

fn validate_anchor_route(
    request: &TerminalCcoRequest,
    terminal: AnchorTerminalBinding,
) -> Result<(), CcoError> {
    if terminal.terminal_id() != request.terminal_id
        || terminal.stub() != request.stub
        || terminal.stub_end_mm() != request.stub_end_mm()
    {
        return Err(CcoError::AnchorBindingMismatch);
    }
    Ok(())
}

impl<'a, 'surface> FinalizedAnchorValidationContext<'a, 'surface> {
    fn bind(
        forest: &Forest,
        request: &TerminalCcoRequest,
        route: FinalizedAnchorTerminalRoute<'a, 'surface>,
    ) -> Result<Self, CcoError> {
        request.validate(forest)?;
        let terminal =
            AnchorTerminalBinding::new(request.terminal_id, request.stub, request.stub_end_mm());
        let bound_contact = validate_finalized_anchor_route(request, &route, terminal)?;
        Ok(Self {
            plan_digest: route.plan.digest(),
            loads_digest: route.loads.digest(),
            pool_digest: route.pool.digest(),
            geometry_fingerprint: route.geometry.fingerprint(),
            route,
            bound_contact,
            terminal,
        })
    }

    /// Recheck every non-native authority immediately before the serial
    /// winner replay.  This is deliberately query-free: the one ray batch,
    /// one final-foot batch, and one clearance batch have already happened.
    fn validate_replay(
        &self,
        forest: &Forest,
        request: &TerminalCcoRequest,
    ) -> Result<(), CcoError> {
        request.validate(forest)?;
        if self.route.plan.digest() != self.plan_digest
            || self.route.loads.digest() != self.loads_digest
            || self.route.pool.digest() != self.pool_digest
            || self.route.geometry.fingerprint() != self.geometry_fingerprint
        {
            return Err(CcoError::StaleFinalizedAnchorAuthority);
        }
        let rebound = validate_finalized_anchor_route(request, &self.route, self.terminal)?;
        if rebound != self.bound_contact {
            return Err(CcoError::StaleFinalizedAnchorAuthority);
        }
        Ok(())
    }

    fn route_evidence(
        &self,
        request: &TerminalCcoRequest,
        forest_digest_before: u64,
        controller_digest_before: u64,
    ) -> TerminalRouteEvidence {
        TerminalRouteEvidence {
            physical_contact_id: request.physical_contact_id,
            terminal_id: request.terminal_id,
            original_key: request.stub,
            plan_digest: self.plan_digest,
            loads_digest: self.loads_digest,
            geometry_digest: self.route.geometry.digest(),
            forest_digest_before,
            controller_digest_before,
        }
    }
}

fn no_winner_evidence(outcome: &TerminalCcoOutcome) -> NoWinnerEvidence {
    debug_assert!(outcome.winner.is_none());
    if outcome.candidates_generated == 0 {
        NoWinnerEvidence::NoCandidatesGenerated {
            anchor_seat_refusals: outcome.anchor_seat_refusals.clone(),
        }
    } else if outcome.candidates_feasible == 0 {
        NoWinnerEvidence::NoFeasibleCandidate {
            candidates_generated: outcome.candidates_generated,
            anchor_seat_refusals: outcome.anchor_seat_refusals.clone(),
            anchor_refusals: outcome.anchor_refusals.clone(),
        }
    } else {
        NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
            candidates_generated: outcome.candidates_generated,
            candidates_feasible: outcome.candidates_feasible,
            anchor_seat_refusals: outcome.anchor_seat_refusals.clone(),
            anchor_refusals: outcome.anchor_refusals.clone(),
        }
    }
}

fn validate_finalized_anchor_route(
    request: &TerminalCcoRequest,
    route: &FinalizedAnchorTerminalRoute<'_, '_>,
    terminal: AnchorTerminalBinding,
) -> Result<BoundPhysicalAnchorContact, CcoError> {
    validate_anchor_route(request, terminal)?;
    if route.contact_id != request.physical_contact_id {
        return Err(CcoError::FinalizedContactMismatch);
    }
    if route.config != AnchorSearchConfig::current()? {
        return Err(CcoError::StaleFinalizedAnchorConfig);
    }
    route.pool.validate_against(route.geometry)?;
    if route.contact.contact_id() != route.contact_id
        || route
            .plan
            .contacts()
            .iter()
            .find(|contact| contact.contact_id() == route.contact_id)
            != Some(route.contact)
    {
        return Err(CcoError::FinalizedContactMismatch);
    }
    let contact_ids = route
        .plan
        .contacts()
        .iter()
        .map(SpecializedPhysicalContact::contact_id)
        .collect::<Vec<_>>();
    if route
        .loads
        .validate_binding(route.plan.load_partition(), &contact_ids)
        .is_err()
    {
        return Err(CcoError::FinalizedLoadBindingMismatch);
    }
    let leaf = route
        .loads
        .leaf_for(route.contact_id)
        .ok_or(CcoError::FinalizedLoadBindingMismatch)?;
    if leaf.contact_id() != route.contact_id
        || Finite::new(route.contact.carried_area_mm2())
            .map_err(|_| CcoError::FinalizedLoadBindingMismatch)?
            != Finite::new(leaf.carried_area_mm2())
                .map_err(|_| CcoError::FinalizedLoadBindingMismatch)?
        || Finite::new(leaf.carried_area_mm2())
            .map_err(|_| CcoError::FinalizedLoadBindingMismatch)?
            != request.carried_mm2()
        || leaf.tip_radius_mm() != request.finalized_tip_radius_mm()
    {
        return Err(CcoError::FinalizedStubLoadMismatch);
    }
    Ok(BoundPhysicalAnchorContact::new(
        route.plan,
        route.contact_id,
    )?)
}

impl HostCache {
    fn rebuild(&mut self, forest: &Forest) {
        self.samples.clear();
        self.indexed_members.clear();
        for key in forest.live_tree_members() {
            let Some(member) = forest.member(key) else {
                continue;
            };
            if matches!(member.kind(), MemberKind::Stub) {
                continue;
            }
            self.indexed_members.insert(key);
            let p0 = member.p0_mm().map(Finite::get);
            let p1 = member.p1_mm().map(Finite::get);
            self.samples.push((p0, key));
            self.samples.push((p1, key));
            self.samples.push((scale(add(p0, p1), 0.5), key));
        }
    }

    fn nearest(&self, point: [f64; 3], count: usize, forest: &Forest) -> Vec<MemberKey> {
        self.nearest_excluding(point, count, forest, &BTreeSet::new())
    }

    fn nearest_excluding(
        &self,
        point: [f64; 3],
        count: usize,
        forest: &Forest,
        excluded: &BTreeSet<MemberKey>,
    ) -> Vec<MemberKey> {
        let mut scored = std::collections::BTreeMap::<MemberKey, f64>::new();
        for &(sample, key) in &self.samples {
            if !excluded.contains(&key) && forest.member(key).is_some() {
                let distance = norm(sub(sample, point));
                scored
                    .entry(key)
                    .and_modify(|old| *old = old.min(distance))
                    .or_insert(distance);
            }
        }
        for key in forest.live_tree_members() {
            if excluded.contains(&key) || self.indexed_members.contains(&key) {
                continue;
            }
            let Some(member) = forest.member(key) else {
                continue;
            };
            if matches!(member.kind(), MemberKind::Stub) {
                continue;
            }
            let midpoint = scale(
                add(
                    member.p0_mm().map(Finite::get),
                    member.p1_mm().map(Finite::get),
                ),
                0.5,
            );
            scored
                .entry(key)
                .or_insert_with(|| norm(sub(midpoint, point)));
        }
        let mut ranked: Vec<(f64, MemberKey)> = scored
            .into_iter()
            .map(|(key, distance)| (distance, key))
            .collect();
        ranked.sort_by(|left, right| left.0.total_cmp(&right.0).then(left.1.cmp(&right.1)));
        ranked.into_iter().take(count).map(|(_, key)| key).collect()
    }
}

#[allow(clippy::too_many_arguments)]
fn grow_terminal_inner(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    surface: &NativeSurface,
    mut seats: Vec<QualifiedAnchorSeat>,
    anchor_seat_refusals: Vec<AnchorSeatRefusal>,
    anchor_context: Option<&FinalizedAnchorValidationContext<'_, '_>>,
    anchor_ray_batches: usize,
    cached_hosts: Option<Vec<MemberKey>>,
) -> Result<TerminalCcoOutcome, CcoError> {
    request.validate(forest)?;
    let config = Config::current()?;
    let descriptors = generate_descriptors(forest, request, &config, &seats, cached_hosts, true);
    let candidates_generated = descriptors.len();
    let snapshot = forest.clone();
    let mut evaluated = evaluate_descriptors(&snapshot, request, &descriptors, &config)?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut validated_anchors = BTreeMap::<usize, Box<ValidatedModelAnchor>>::new();
    let mut anchor_refusals = Vec::new();
    let mut anchor_foot_batches = 0;
    if !seats.is_empty() {
        let context = anchor_context.ok_or(CcoError::InvalidGeometry)?;
        let mut seats: Vec<Option<QualifiedAnchorSeat>> = seats.drain(..).map(Some).collect();
        let mut generation_indices = Vec::new();
        let mut foot_requests = Vec::new();
        for candidate in &evaluated {
            if let Some((seat_index, final_radius)) = candidate.anchor {
                let seat = seats
                    .get_mut(seat_index)
                    .and_then(Option::take)
                    .ok_or(CcoError::InvalidGeometry)?;
                generation_indices.push(candidate.generation_index);
                foot_requests.push(AnchorFootRequest::new(seat, final_radius));
            }
        }
        let validation = validate_finalized_anchor_feet(
            foot_requests,
            context.bound_contact,
            context.terminal,
            context.route.pool,
            context.route.config,
            context.route.geometry,
        )?;
        anchor_foot_batches = validation.query_counts().sphere_breach_batches;
        let mut valid = BTreeSet::new();
        for (index, outcome) in generation_indices
            .into_iter()
            .zip(validation.into_outcomes())
        {
            match outcome {
                AnchorFootOutcome::Validated(anchor) => {
                    valid.insert(index);
                    validated_anchors.insert(index, anchor);
                }
                AnchorFootOutcome::Refused(refusal) => anchor_refusals.push(refusal),
            }
        }
        evaluated.retain(|candidate| {
            candidate.anchor.is_none() || valid.contains(&candidate.generation_index)
        });
    }
    let candidates_feasible = evaluated.len();
    if evaluated.is_empty() {
        return Ok(TerminalCcoOutcome {
            winner: None,
            disposition: None,
            candidates_generated,
            candidates_feasible,
            anchor_ray_batches,
            anchor_foot_batches,
            clearance_batches: 0,
            validated_model_anchor: None,
            anchor_seat_refusals,
            anchor_refusals,
        });
    }

    let survival = clearance_survival(surface, &config, &evaluated)?;
    let mut survivors: Vec<&EvaluatedCandidate> = evaluated
        .iter()
        .zip(survival)
        .filter_map(|(candidate, survives)| survives.then_some(candidate))
        .collect();
    survivors.sort_by(|left, right| compare_candidates(left, right));
    let Some(winner) = survivors.first() else {
        return Ok(TerminalCcoOutcome {
            winner: None,
            disposition: None,
            candidates_generated,
            candidates_feasible,
            anchor_ray_batches,
            anchor_foot_batches,
            clearance_batches: 1,
            validated_model_anchor: None,
            anchor_seat_refusals,
            anchor_refusals,
        });
    };
    let winner_index = winner.generation_index;
    if let Some(context) = anchor_context {
        context.validate_replay(forest, request)?;
    }
    let checkpoint = forest.checkpoint();
    match apply_and_size(forest, request, &descriptors[winner_index], &config) {
        Ok(true) if forest.digest() == winner.forest.digest() => forest.commit(checkpoint)?,
        Ok(_) => {
            forest.rollback(checkpoint)?;
            return Err(CcoError::NonDeterministicWinner);
        }
        Err(error) => {
            forest.rollback(checkpoint)?;
            return Err(error);
        }
    }
    Ok(TerminalCcoOutcome {
        winner: Some(CcoCandidateEvidence {
            family: winner.family,
            generation_index: winner.generation_index,
            price_ml: winner.price_ml,
            clearance_rows: winner.rows.len(),
        }),
        disposition: None,
        candidates_generated,
        candidates_feasible,
        anchor_ray_batches,
        anchor_foot_batches,
        clearance_batches: 1,
        validated_model_anchor: validated_anchors.remove(&winner_index),
        anchor_seat_refusals,
        anchor_refusals,
    })
}

fn evaluate_descriptors(
    snapshot: &Forest,
    request: &TerminalCcoRequest,
    descriptors: &[Descriptor],
    config: &Config,
) -> Result<Vec<Option<EvaluatedCandidate>>, CcoError> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(geom_kernel::current_threads())
        .build()
        .map_err(|error| CcoError::Constants(error.to_string()))?;
    let evaluated: Vec<Result<Option<EvaluatedCandidate>, CcoError>> = pool.install(|| {
        descriptors
            .par_iter()
            .enumerate()
            .map(|(index, descriptor)| evaluate(snapshot, request, descriptor, index, config))
            .collect()
    });
    evaluated.into_iter().collect()
}

fn clearance_survival(
    surface: &NativeSurface,
    config: &Config,
    candidates: &[EvaluatedCandidate],
) -> Result<Vec<bool>, CcoError> {
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    let mut inset0 = Vec::new();
    let mut inset1 = Vec::new();
    let mut ranges = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let start = p0.len();
        for row in &candidate.rows {
            p0.push(row.p0);
            p1.push(row.p1);
            r0.push(row.r0);
            r1.push(row.r1);
            inset0.push(row.inset0);
            inset1.push(row.inset1);
        }
        ranges.push(start..p0.len());
    }
    let (clearances, _) = surface
        .segment_clearance(&p0, &p1, &r0, &r1, config.standoff_mm, &inset0, &inset1)
        .map_err(|error| CcoError::Native(error.to_string()))?;
    Ok(ranges
        .into_iter()
        .map(|range| clearances[range].iter().all(|clearance| *clearance >= 0.0))
        .collect())
}

fn compare_candidates(left: &EvaluatedCandidate, right: &EvaluatedCandidate) -> std::cmp::Ordering {
    left.price_ml
        .total_cmp(&right.price_ml)
        .then(left.generation_index.cmp(&right.generation_index))
}

impl RewireCandidateBatch {
    pub(crate) fn ranked_candidates(&self) -> Vec<&RewireEvaluatedCandidate> {
        let mut ranked = self.candidates.iter().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            left.evidence
                .price_ml
                .total_cmp(&right.evidence.price_ml)
                .then(
                    left.evidence
                        .generation_index
                        .cmp(&right.evidence.generation_index),
                )
        });
        ranked
    }

    pub(crate) fn snapshot_digest(&self) -> u64 {
        self.snapshot_digest
    }

    pub(crate) fn refusals(&self) -> &[RewireCandidateRefusal] {
        &self.refusals
    }

    /// Replay one caller-selected surviving generation index. This consumes
    /// the originating batch, so a candidate capability cannot be replayed
    /// twice or detached from the evidence ledger that proved it.
    pub(crate) fn replay_candidate(
        mut self,
        binding: &RewireCcoBinding,
        live: &Forest,
        generation_index: usize,
        geometry: &AnchorGeometryAuthority<'_>,
    ) -> Result<RewireReplay, CcoError> {
        binding.validate_live(live)?;
        let identity = binding.identity();
        if self.binding_identity != identity
            || self.snapshot_digest != binding.snapshot_digest
            || self.splice != binding.splice
            || self.excluded_hosts != binding.excluded_hosts
        {
            return Err(CcoError::RewireBatchBindingMismatch);
        }
        if self.geometry_fingerprint != geometry.fingerprint() {
            return Err(CcoError::RewireGeometryAuthorityMismatch);
        }
        let matches = self
            .candidates
            .iter()
            .enumerate()
            .filter_map(|(position, candidate)| {
                (candidate.evidence.generation_index == generation_index).then_some(position)
            })
            .collect::<Vec<_>>();
        let [position] = matches.as_slice() else {
            return Err(CcoError::RewireCandidateNotFound(generation_index));
        };
        let candidate = self.candidates.swap_remove(*position);
        if candidate.evidence.generation_index != generation_index
            || candidate.descriptor.family() != candidate.evidence.family
        {
            return Err(CcoError::RewireCandidateEvidenceMismatch);
        }

        let config = Config::current()?;
        let Some(mut replayed) = evaluate(
            &binding.snapshot,
            &binding.request,
            &candidate.descriptor,
            generation_index,
            &config,
        )?
        else {
            return Err(CcoError::RewireCandidateEvidenceMismatch);
        };
        if has_rewire_degenerate_new_member(&binding.snapshot, &replayed.forest) {
            return Err(CcoError::RewireCandidateEvidenceMismatch);
        }
        replayed.price_ml = changed_forest_volume_delta(&binding.snapshot, &replayed.forest)?;
        let replayed_digest = replayed.forest.digest();
        if replayed_digest != candidate.evaluated_digest
            || replayed.family != candidate.evidence.family
            || replayed.generation_index != candidate.evidence.generation_index
            || replayed.price_ml.to_bits() != candidate.evidence.price_ml.to_bits()
            || replayed.rows.len() != candidate.evidence.clearance_rows
        {
            return Err(CcoError::RewireCandidateEvidenceMismatch);
        }

        match (&candidate.descriptor, replayed.anchor) {
            (
                Descriptor::ModelAnchor {
                    seat_index,
                    point_mm,
                },
                Some((replayed_seat, final_radius)),
            ) => {
                let Some(anchor) = candidate.validated_model_anchor.as_ref() else {
                    return Err(CcoError::RewireCandidateEvidenceMismatch);
                };
                let proved_point = anchor.seat().point_mm().map(Finite::get);
                if replayed_seat != *seat_index
                    || proved_point.map(f64::to_bits) != point_mm.map(f64::to_bits)
                    || anchor.foot().final_member_radius_mm() != final_radius
                {
                    return Err(CcoError::RewireCandidateEvidenceMismatch);
                }
            }
            (Descriptor::ModelAnchor { .. }, None) | (_, Some(_)) => {
                return Err(CcoError::RewireCandidateEvidenceMismatch);
            }
            (_, None) if candidate.validated_model_anchor.is_some() => {
                return Err(CcoError::RewireCandidateEvidenceMismatch);
            }
            (_, None) => {}
        }
        if clearance_survival(geometry.surface(), &config, std::slice::from_ref(&replayed))?
            != [true]
        {
            return Err(CcoError::RewireClearanceReplayFailed);
        }
        Ok(RewireReplay {
            forest: replayed.forest,
            splice: binding.splice.clone(),
            evidence: candidate.evidence,
            validated_model_anchor: candidate.validated_model_anchor,
            binding_identity: identity,
        })
    }
}

impl RewireEvaluatedCandidate {
    pub(crate) const fn evidence(&self) -> &CcoCandidateEvidence {
        &self.evidence
    }
}

fn generate_descriptors(
    forest: &Forest,
    request: &TerminalCcoRequest,
    config: &Config,
    anchor_seats: &[QualifiedAnchorSeat],
    cached_hosts: Option<Vec<MemberKey>>,
    include_plate_spiral: bool,
) -> Vec<Descriptor> {
    let se = request.stub_end_mm().map(Finite::get);
    let mut out = vec![Descriptor::Plate {
        xy: [se[0], se[1]],
        top_z: se[2],
    }];
    if include_plate_spiral {
        for &radius in &config.trunk_radii_mm {
            if radius == 0.0 {
                continue;
            }
            for direction in 0..config.trunk_directions {
                let angle =
                    std::f64::consts::TAU * direction as f64 / config.trunk_directions as f64;
                let xy = [se[0] + radius * angle.cos(), se[1] + radius * angle.sin()];
                let top_z = se[2] - radius / config.branch_angle_deg.to_radians().tan().max(1e-9);
                if top_z > request.plate_z_mm().get() + 1.0 {
                    out.push(Descriptor::Plate { xy, top_z });
                }
            }
        }
    }
    out.extend(
        anchor_seats
            .iter()
            .enumerate()
            .map(|(seat_index, seat)| Descriptor::ModelAnchor {
                seat_index,
                point_mm: seat.point_mm().map(Finite::get),
            }),
    );
    let hosts = cached_hosts
        .unwrap_or_else(|| HostCache::default().nearest(se, config.bifurcation_hosts, forest));
    out.extend(hosts.into_iter().map(|host| Descriptor::Bifurcate { host }));
    out
}

fn evaluate(
    snapshot: &Forest,
    request: &TerminalCcoRequest,
    descriptor: &Descriptor,
    generation_index: usize,
    config: &Config,
) -> Result<Option<EvaluatedCandidate>, CcoError> {
    let mut candidate = snapshot.clone();
    if !apply_and_size(&mut candidate, request, descriptor, config)? {
        return Ok(None);
    }
    candidate.validate()?;
    let changed_delta_ml = changed_forest_volume_delta(snapshot, &candidate)?;
    let price_ml = candidate_price(descriptor, &candidate, request, config, changed_delta_ml)?;
    if !price_ml.is_finite() {
        return Err(CcoError::InvalidGeometry);
    }
    let rows = clearance_rows(&candidate, snapshot, config);
    let anchor = if let Descriptor::ModelAnchor { seat_index, .. } = descriptor {
        let parent = candidate
            .parent_of(request.stub)
            .and_then(|key| candidate.member(key))
            .ok_or(CcoError::InvalidGeometry)?;
        Some((*seat_index, parent.rootward_radius_mm().get()))
    } else {
        None
    };
    Ok(Some(EvaluatedCandidate {
        forest: candidate,
        family: descriptor.family(),
        generation_index,
        price_ml,
        rows,
        anchor,
    }))
}

fn has_rewire_degenerate_new_member(before: &Forest, after: &Forest) -> bool {
    after.live_tree_members().into_iter().any(|key| {
        before.member(key).is_none()
            && after.member(key).is_some_and(|member| {
                norm(sub(
                    member.p1_mm().map(Finite::get),
                    member.p0_mm().map(Finite::get),
                )) < REWIRE_DEGENERATE_MM
            })
    })
}

fn apply_and_size(
    candidate: &mut Forest,
    request: &TerminalCcoRequest,
    descriptor: &Descriptor,
    config: &Config,
) -> Result<bool, CcoError> {
    if candidate.terminal_key(request.terminal_id) == Some(request.stub) {
        candidate.consume_terminal(request.terminal_id, request.stub)?;
    }
    match descriptor {
        Descriptor::Plate { xy, top_z } => {
            if !apply_plate(candidate, request, *xy, *top_z, config)? {
                return Ok(false);
            }
            size_plate_candidate(candidate, request)?;
        }
        Descriptor::Bifurcate { host } => {
            if !apply_bifurcation(candidate, request, *host, config)? {
                return Ok(false);
            }
            size_bifurcation_candidate(candidate, request, *host)?;
        }
        Descriptor::ModelAnchor { point_mm, .. } => {
            if !apply_model_anchor(candidate, request, *point_mm, config)? {
                return Ok(false);
            }
            let strut = candidate
                .parent_of(request.stub)
                .ok_or(CcoError::InvalidGeometry)?;
            size_tapered_without_tree_fanin(candidate, strut)?;
        }
    }
    Ok(true)
}

fn apply_model_anchor(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    point_mm: [f64; 3],
    config: &Config,
) -> Result<bool, CcoError> {
    let stub_end = request.stub_end_mm().map(Finite::get);
    if norm(sub(point_mm, stub_end)) <= DEGENERATE_MM
        || !within_cone(stub_end, point_mm, config.min_shoot_radius_mm.get(), config)
    {
        return Ok(false);
    }
    let strut = forest.add_member(Member::new(MemberSpec {
        p0_mm: request.stub_end_mm(),
        p1_mm: finite3(point_mm)?,
        r0_mm: TerminalEndRadius::new(cco_terminal_radius(request, config)?),
        r1_mm: RootwardEndRadius::new(config.min_shoot_radius_mm),
        kind: MemberKind::Strut,
        owner: request.connector_owner.clone(),
        embed_end: Some(EmbedEnd::Rootward),
        carried_mm2: request.carried_mm2(),
    }));
    forest.set_parent(request.stub, Some(strut))?;
    Ok(true)
}

fn apply_plate(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    xy: [f64; 2],
    top_z: f64,
    config: &Config,
) -> Result<bool, CcoError> {
    let se = request.stub_end_mm().map(Finite::get);
    let top = [xy[0], xy[1], top_z];
    let bottom = [xy[0], xy[1], request.plate_z_mm().get()];
    if top_z - bottom[2] <= DEGENERATE_MM {
        return Ok(false);
    }
    let mut parent = request.stub;
    if norm(sub(top, se)) > DEGENERATE_MM {
        let radius = config.min_trunk_radius_mm;
        if !within_cone(se, top, radius.get(), config) {
            return Ok(false);
        }
        let diagonal = forest.add_member(member(
            se,
            top,
            cco_terminal_radius(request, config)?,
            radius,
            MemberKind::Trunk,
            request,
        )?);
        forest.set_parent(request.stub, Some(diagonal))?;
        parent = diagonal;
    }
    let column = forest.add_member(member(
        top,
        bottom,
        config.min_trunk_radius_mm,
        config.min_trunk_radius_mm,
        MemberKind::Trunk,
        request,
    )?);
    forest.set_parent(parent, Some(column))?;
    Ok(true)
}

fn apply_bifurcation(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    host_key: MemberKey,
    config: &Config,
) -> Result<bool, CcoError> {
    let Some(host) = forest.member(host_key).cloned() else {
        return Ok(false);
    };
    let p0 = host.p0_mm().map(Finite::get);
    let p1 = host.p1_mm().map(Finite::get);
    let se = request.stub_end_mm().map(Finite::get);
    let direction = sub(p1, p0);
    let length_squared = dot(direction, direction);
    if length_squared < 1e-12 {
        return Ok(false);
    }
    let t0 =
        (dot(sub(se, p0), direction) / length_squared).clamp(config.split_start, config.split_end);
    let seed = add(p0, scale(direction, t0));
    let m1 = host.carried_mm2().get().max(1e-9);
    let m2 = request.carried_mm2().get().max(1e-9);
    let mut junction = weighted_fermat_point(
        &[(p0, m1.sqrt()), (p1, (m1 + m2).sqrt()), (se, m2.sqrt())],
        seed,
        config.fermat_iterations,
    );
    let terminal_radius = cco_terminal_radius(request, config)?;
    if !junction_feasible(junction, se, &host, terminal_radius.get(), config) {
        let mut t = t0;
        let mut found = false;
        for _ in 0..config.split_steps {
            junction = add(p0, scale(direction, t));
            if junction_feasible(junction, se, &host, terminal_radius.get(), config) {
                found = true;
                break;
            }
            t += config.split_step;
            if t > config.split_end {
                break;
            }
        }
        if !found {
            return Ok(false);
        }
    }
    let old_parent = forest.parent_of(host_key);
    let interpolated = radius_along(&host, junction)?;
    let upper = forest.add_member(Member::new(MemberSpec {
        p0_mm: finite3(junction)?,
        p1_mm: host.p1_mm(),
        r0_mm: TerminalEndRadius::new(interpolated),
        r1_mm: host.rootward_radius_mm(),
        kind: host.kind().clone(),
        owner: host.owner().clone(),
        embed_end: host.embed_end(),
        carried_mm2: host.carried_mm2(),
    }));
    forest.set_parent(upper, old_parent)?;
    forest.set_endpoints(host_key, host.p0_mm(), finite3(junction)?)?;
    forest.set_radii(
        host_key,
        host.terminal_radius_mm(),
        RootwardEndRadius::new(interpolated),
    )?;
    if host.embed_end() == Some(EmbedEnd::Rootward) {
        forest.set_embed_end(host_key, None)?;
    }
    forest.set_parent(host_key, Some(upper))?;
    let child = forest.add_member(member(
        se,
        junction,
        terminal_radius,
        config.min_shoot_radius_mm,
        MemberKind::Shoot,
        request,
    )?);
    forest.set_parent(child, Some(upper))?;
    forest.set_parent(request.stub, Some(child))?;
    propagate_carried(forest, upper, request.carried_mm2())?;
    Ok(true)
}

fn size_plate_candidate(forest: &mut Forest, request: &TerminalCcoRequest) -> Result<(), CcoError> {
    let first = forest
        .parent_of(request.stub)
        .ok_or(CcoError::InvalidGeometry)?;
    let first_reaches_plate = forest
        .member(first)
        .is_some_and(|member| member.p1_mm()[2] == request.plate_z_mm());
    if first_reaches_plate {
        let radius = required_rootward_without_tree_fanin(forest, first)?;
        forest.set_radii(
            first,
            TerminalEndRadius::new(radius),
            RootwardEndRadius::new(radius),
        )?;
        return Ok(());
    }

    let diagonal = first;
    let column = forest
        .parent_of(diagonal)
        .ok_or(CcoError::InvalidGeometry)?;
    let column_radius = required_rootward_without_tree_fanin(forest, column)?;
    let diagonal_radius = required_rootward_without_tree_fanin(forest, diagonal)?;
    let final_radius = if diagonal_radius.get() >= column_radius.get() {
        diagonal_radius
    } else {
        column_radius
    };
    let diagonal_terminal = forest
        .member(diagonal)
        .ok_or(ForestError::InactiveKey(diagonal))?
        .terminal_radius_mm();
    forest.set_radii(
        diagonal,
        diagonal_terminal,
        RootwardEndRadius::new(final_radius),
    )?;
    forest.set_radii(
        column,
        TerminalEndRadius::new(final_radius),
        RootwardEndRadius::new(final_radius),
    )?;
    Ok(())
}

fn size_bifurcation_candidate(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    host: MemberKey,
) -> Result<(), CcoError> {
    let upper = forest.parent_of(host).ok_or(CcoError::InvalidGeometry)?;
    let child = forest
        .parent_of(request.stub)
        .ok_or(CcoError::InvalidGeometry)?;

    let junction_radius = required_rootward_without_tree_fanin(forest, host)?;
    let host_terminal = forest
        .member(host)
        .ok_or(ForestError::InactiveKey(host))?
        .terminal_radius_mm();
    forest.set_radii(host, host_terminal, RootwardEndRadius::new(junction_radius))?;
    let upper_rootward = forest
        .member(upper)
        .ok_or(ForestError::InactiveKey(upper))?
        .rootward_radius_mm();
    forest.set_radii(
        upper,
        TerminalEndRadius::new(junction_radius),
        upper_rootward,
    )?;
    size_tapered_without_tree_fanin(forest, child)?;
    let child_radius = forest
        .member(child)
        .ok_or(ForestError::InactiveKey(child))?
        .rootward_radius_mm();
    uniformly_size_ancestor_path(forest, upper, child_radius)
}

fn required_rootward_without_tree_fanin(
    forest: &Forest,
    key: MemberKey,
) -> Result<PositiveFinite, CcoError> {
    let member = forest.member(key).ok_or(ForestError::InactiveKey(key))?;
    o10_uniform_radius_for_span(
        member.p0_mm().map(Finite::get),
        member.p1_mm().map(Finite::get),
        member.kind().clone(),
        member.terminal_radius_mm().get(),
        member.carried_mm2(),
    )
}

fn size_tapered_without_tree_fanin(forest: &mut Forest, key: MemberKey) -> Result<(), CcoError> {
    let radius = required_rootward_without_tree_fanin(forest, key)?;
    let terminal = forest
        .member(key)
        .ok_or(ForestError::InactiveKey(key))?
        .terminal_radius_mm();
    forest.set_radii(key, terminal, RootwardEndRadius::new(radius))?;
    Ok(())
}

fn uniformly_size_ancestor_path(
    forest: &mut Forest,
    mut key: MemberKey,
    mut incoming_radius: RootwardEndRadius,
) -> Result<(), CcoError> {
    loop {
        let entry = forest
            .member(key)
            .ok_or(ForestError::InactiveKey(key))?
            .terminal_radius_mm()
            .get()
            .get();
        let incoming = incoming_radius.get().get();
        let murray = (entry.powi(4) + incoming.powi(4)).powf(0.25);
        let no_fanin = required_rootward_without_tree_fanin(forest, key)?;
        let radius = PositiveFinite::new(murray.max(no_fanin.get()))
            .map_err(|_| CcoError::InvalidGeometry)?;
        forest.set_radii(
            key,
            TerminalEndRadius::new(radius),
            RootwardEndRadius::new(radius),
        )?;
        incoming_radius = RootwardEndRadius::new(radius);
        let Some(parent) = forest.parent_of(key) else {
            break;
        };
        key = parent;
    }
    Ok(())
}

fn propagate_carried(
    forest: &mut Forest,
    mut key: MemberKey,
    added: Finite,
) -> Result<(), CcoError> {
    loop {
        let member = forest.member(key).ok_or(ForestError::InactiveKey(key))?;
        let carried = Finite::new(member.carried_mm2().get() + added.get())
            .map_err(|_| CcoError::InvalidGeometry)?;
        forest.set_carried_mm2(key, carried)?;
        let Some(parent) = forest.parent_of(key) else {
            break;
        };
        key = parent;
    }
    Ok(())
}

fn member(
    p0: [f64; 3],
    p1: [f64; 3],
    r0: PositiveFinite,
    r1: PositiveFinite,
    kind: MemberKind,
    request: &TerminalCcoRequest,
) -> Result<Member, CcoError> {
    Ok(Member::new(MemberSpec {
        p0_mm: finite3(p0)?,
        p1_mm: finite3(p1)?,
        r0_mm: TerminalEndRadius::new(r0),
        r1_mm: RootwardEndRadius::new(r1),
        kind,
        owner: request.connector_owner.clone(),
        embed_end: None,
        carried_mm2: request.carried_mm2(),
    }))
}

fn cco_terminal_radius(
    request: &TerminalCcoRequest,
    config: &Config,
) -> Result<PositiveFinite, CcoError> {
    PositiveFinite::new(
        request
            .tip_radius_mm()
            .get()
            .max(config.min_shoot_radius_mm.get()),
    )
    .map_err(|_| CcoError::InvalidGeometry)
}

fn clearance_rows(forest: &Forest, snapshot: &Forest, config: &Config) -> Vec<ClearanceRow> {
    forest
        .live_tree_members()
        .into_iter()
        .filter_map(|key| {
            let member = forest.member(key)?;
            (snapshot.member(key) != Some(member)).then_some(member)
        })
        .map(|member| ClearanceRow {
            p0: member.p0_mm().map(Finite::get),
            p1: member.p1_mm().map(Finite::get),
            r0: member.terminal_radius_mm().get().get(),
            r1: member.rootward_radius_mm().get().get(),
            inset0: if member.embed_end() == Some(EmbedEnd::Terminalward) {
                config.contact_depth_mm
            } else {
                0.0
            },
            inset1: if member.embed_end() == Some(EmbedEnd::Rootward) {
                config.standoff_mm
            } else {
                0.0
            },
        })
        .collect()
}

fn changed_forest_volume_delta(before: &Forest, after: &Forest) -> Result<f64, CcoError> {
    let keys = before
        .live_tree_members()
        .into_iter()
        .chain(after.live_tree_members())
        .collect::<BTreeSet<_>>();
    keys.into_iter().try_fold(0.0, |delta, key| {
        let old = before.member(key);
        let new = after.member(key);
        if old == new {
            return Ok(delta);
        }
        let removed = old.map(member_volume).transpose()?.unwrap_or(0.0);
        let added = new.map(member_volume).transpose()?.unwrap_or(0.0);
        Ok(delta + added - removed)
    })
}

fn candidate_price(
    descriptor: &Descriptor,
    candidate: &Forest,
    request: &TerminalCcoRequest,
    config: &Config,
    ordinary_delta_ml: f64,
) -> Result<f64, CcoError> {
    match descriptor {
        Descriptor::ModelAnchor { .. } => {
            let strut = candidate
                .parent_of(request.stub)
                .and_then(|key| candidate.member(key))
                .ok_or(CcoError::InvalidGeometry)?;
            return member_volume(strut);
        }
        Descriptor::Bifurcate { .. } => return Ok(ordinary_delta_ml),
        Descriptor::Plate { .. } => {}
    }
    let first = candidate
        .parent_of(request.stub)
        .ok_or(CcoError::InvalidGeometry)?;
    let (diagonal, column) = if candidate
        .member(first)
        .is_some_and(|member| member.p1_mm()[2].get() == request.plate_z_mm().get())
    {
        (None, first)
    } else {
        (
            Some(first),
            candidate
                .parent_of(first)
                .ok_or(CcoError::InvalidGeometry)?,
        )
    };
    let diagonal_delta = diagonal.map_or(Ok(0.0), |key| {
        let member = candidate.member(key).ok_or(ForestError::InactiveKey(key))?;
        member_volume(member)
    })?;
    let column_member = candidate
        .member(column)
        .ok_or(ForestError::InactiveKey(column))?;
    let column_length = norm(sub(
        column_member.p1_mm().map(Finite::get),
        column_member.p0_mm().map(Finite::get),
    ));
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    let brace_span = compatibility.brace_free_span_mm().diagnostic().get();
    let priced_span = column_length.min(brace_span);
    let priced_radius = o10_uniform_radius_for_span(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, priced_span],
        MemberKind::Trunk,
        config.min_trunk_radius_mm,
        request.carried_mm2(),
    )?;
    let column_price = std::f64::consts::PI * priced_radius.get().powi(2) * column_length
        / MILLIMETRES_PER_MILLILITRE;
    let rung_count = (column_length / brace_span).floor() as usize;
    let brace_price = if rung_count == 0 {
        0.0
    } else {
        let distance = config.brace_neighbor_max_xy_mm / 2.0;
        let rod_length = distance * 2.0_f64.sqrt();
        let raw = o10_uniform_radius_for_span(
            [0.0, 0.0, 0.0],
            [distance, 0.0, distance],
            MemberKind::Shoot,
            config.min_shoot_radius_mm,
            Finite::new(0.0).map_err(|_| CcoError::InvalidGeometry)?,
        )?;
        let rod_radius = config.brace_radius_mm.max(raw.get() * 0.5_f64.powf(0.25));
        rung_count as f64 * 2.0 * std::f64::consts::PI * rod_radius.powi(2) * rod_length
            / MILLIMETRES_PER_MILLILITRE
    };
    Ok(diagonal_delta + column_price + brace_price)
}

fn o10_uniform_radius_for_span(
    p0: [f64; 3],
    p1: [f64; 3],
    kind: MemberKind,
    floor: PositiveFinite,
    carried_mm2: Finite,
) -> Result<PositiveFinite, CcoError> {
    let mut forest = Forest::new();
    let key = forest.add_member(Member::new(MemberSpec {
        p0_mm: finite3(p0)?,
        p1_mm: finite3(p1)?,
        r0_mm: TerminalEndRadius::new(floor),
        r1_mm: RootwardEndRadius::new(floor),
        kind,
        owner: Owner::Generated {
            family: "cco-price".to_owned(),
            ordinal: 0,
        },
        embed_end: None,
        carried_mm2,
    }));
    let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
    Ok(compatibility
        .size_live_current_support_member(&forest, key)?
        .rootward_radius()
        .get())
}

fn member_volume(member: &Member) -> Result<f64, CcoError> {
    let delta = sub(
        member.p1_mm().map(Finite::get),
        member.p0_mm().map(Finite::get),
    );
    // NumPy's frozen `linalg.norm` vector executes sqrt(sum(delta**2));
    // `f64::hypot` has different rounding on mixed-axis spans.
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    let r0 = member.terminal_radius_mm().get().get();
    let r1 = member.rootward_radius_mm().get().get();
    let volume = std::f64::consts::PI / 3.0 * length * (r0 * r0 + r0 * r1 + r1 * r1)
        / MILLIMETRES_PER_MILLILITRE;
    volume
        .is_finite()
        .then_some(volume)
        .ok_or(CcoError::InvalidGeometry)
}

fn junction_feasible(
    j: [f64; 3],
    se: [f64; 3],
    host: &Member,
    radius: f64,
    config: &Config,
) -> bool {
    let p0 = host.p0_mm().map(Finite::get);
    let p1 = host.p1_mm().map(Finite::get);
    let direction = sub(p1, p0);
    let length_squared = dot(direction, direction);
    let t = if length_squared < 1e-12 {
        0.0
    } else {
        (dot(sub(j, p0), direction) / length_squared).clamp(0.0, 1.0)
    };
    let host_r0 = host.terminal_radius_mm().get().get();
    let host_r1 = host.rootward_radius_mm().get().get();
    let junction_radius = host_r0 + t * (host_r1 - host_r0);
    j[2] < se[2] - 1e-9
        && norm(sub(j, p0)) >= config.minimum_split_mm
        && norm(sub(j, p1)) >= config.minimum_split_mm
        && within_cone(se, j, radius.max(config.min_shoot_radius_mm.get()), config)
        && within_cone(p0, j, host_r0.max(junction_radius), config)
        && within_cone(j, p1, junction_radius.max(host_r1), config)
}

fn within_cone(p0: [f64; 3], p1: [f64; 3], radius: f64, config: &Config) -> bool {
    let delta = sub(p1, p0);
    let length = norm(delta);
    let lean = if length < 1e-9 {
        0.0
    } else {
        delta[0].hypot(delta[1]).atan2(delta[2].abs()).to_degrees()
    };
    let maximum = if radius >= config.min_shoot_radius_mm.get() {
        config.branch_angle_deg
    } else {
        config.max_segment_angle_deg
    };
    lean <= maximum + config.cone_tolerance_deg
}

fn radius_along(host: &Member, point: [f64; 3]) -> Result<PositiveFinite, CcoError> {
    let p0 = host.p0_mm().map(Finite::get);
    let p1 = host.p1_mm().map(Finite::get);
    let direction = sub(p1, p0);
    let length_squared = dot(direction, direction);
    let t = if length_squared < 1e-12 {
        0.0
    } else {
        (dot(sub(point, p0), direction) / length_squared).clamp(0.0, 1.0)
    };
    let r0 = host.terminal_radius_mm().get().get();
    let r1 = host.rootward_radius_mm().get().get();
    PositiveFinite::new(r0 + t * (r1 - r0)).map_err(|_| CcoError::InvalidGeometry)
}

fn finite3(values: [f64; 3]) -> Result<[Finite; 3], CcoError> {
    values
        .map(Finite::new)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CcoError::InvalidGeometry)?
        .try_into()
        .map_err(|_| CcoError::InvalidGeometry)
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use geom_kernel::{KnnBatchLimits, NativeSurface, NativeSurfaceConfig};
    use plinth_core::LayerPlan;
    use plinth_mesh::{
        build_face_topology, make_box, make_overhang_table, make_toed_slab, FaceTopology, Mesh,
        WeldPolicy,
    };

    use crate::{
        dyadic::CandidatePayloadConfig,
        initial::{build_initial_contacts, ContactConstructionLimits},
        perception::{prepare_generic_contacts, GenericPerceptionLimits},
        select_contacts,
        specialized::{prepare_specialized_contacts, SpecializedLimits},
        ContactSelectionConfig, SupportPerceptionConfig,
    };

    const TEST_KNN_LIMITS: KnnBatchLimits = KnnBatchLimits {
        max_source_points: 20_000_000,
        max_query_points: 20_000_000,
        max_neighbors: 1,
        max_logical_peak_bytes: 2 * 1024 * 1024 * 1024,
        max_work: 100_000_000,
    };

    fn transcript_frame() -> BuildFrame {
        BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap())
    }

    fn transcript_perception_config(
        delicacy: &[f64],
        plate_z_mm: Finite,
    ) -> SupportPerceptionConfig<'_> {
        SupportPerceptionConfig::current(
            plate_z_mm,
            PositiveFinite::new(1.0).unwrap(),
            PositiveFinite::new(1.0).unwrap(),
            64 * 1024 * 1024,
            Some(delicacy),
        )
        .unwrap()
    }

    /// Build a real initial-contact authority, including the frozen plan and
    /// native geometry it is required to retain.  Tests route it only through
    /// `grow_terminal_with_finalized_anchor` / `advance_skipped_terminal`.
    fn real_initial_transcript_fixture_for(
        mesh: Mesh,
    ) -> (
        InitialContactBuild,
        SpecializedContactPlan,
        FinalizedContactLoads,
        NativeSurface,
        FaceTopology,
        Mesh,
        Vec<f64>,
    ) {
        real_initial_transcript_fixture_for_frame(mesh, transcript_frame())
    }

    fn real_initial_transcript_fixture_for_frame(
        mesh: Mesh,
        frame: BuildFrame,
    ) -> (
        InitialContactBuild,
        SpecializedContactPlan,
        FinalizedContactLoads,
        NativeSurface,
        FaceTopology,
        Mesh,
        Vec<f64>,
    ) {
        real_initial_transcript_fixture_for_frame_with_delicacy(mesh, frame, 0.0)
    }

    fn real_initial_transcript_fixture_for_frame_with_delicacy(
        mesh: Mesh,
        frame: BuildFrame,
        delicacy_value: f64,
    ) -> (
        InitialContactBuild,
        SpecializedContactPlan,
        FinalizedContactLoads,
        NativeSurface,
        FaceTopology,
        Mesh,
        Vec<f64>,
    ) {
        let delicacy = vec![delicacy_value; mesh.tris().len()];
        let layers = LayerPlan::new(frame.plate_z_mm().get(), vec![0.05; 1_000]).unwrap();
        let candidate =
            CandidatePayloadConfig::current(8, 1_000_000, 3_000_000, 256 * 1024 * 1024, 65_536)
                .unwrap();
        let config = transcript_perception_config(&delicacy, frame.plate_z_mm());
        let generic = prepare_generic_contacts(
            &mesh,
            &config,
            frame,
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
            crate::SelectedContacts {
                contacts: Vec::new(),
                coverage_samples: Vec::new(),
            }
        } else {
            select_contacts(&mesh, generic.candidates(), generic.islands(), &selection).unwrap()
        };
        let plan = prepare_specialized_contacts(
            &mesh,
            &config,
            frame,
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
        let loads = plan.finalized_loads().unwrap().clone();
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
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let build = build_initial_contacts(
            &plan,
            &loads,
            &mesh,
            &config,
            frame,
            &layers,
            &candidate,
            &[],
            GenericPerceptionLimits::production(TEST_KNN_LIMITS),
            &generic,
            &regular,
            &selection,
            SpecializedLimits::production(TEST_KNN_LIMITS),
            &geometry,
            ContactConstructionLimits::production(),
        )
        .unwrap();
        (build, plan, loads, surface, topology, mesh, delicacy)
    }

    fn real_initial_transcript_fixture() -> (
        InitialContactBuild,
        SpecializedContactPlan,
        FinalizedContactLoads,
        NativeSurface,
        FaceTopology,
        Mesh,
        Vec<f64>,
    ) {
        real_initial_transcript_fixture_for(make_overhang_table().unwrap())
    }

    fn translated_mesh(mesh: Mesh, offset: [f64; 3]) -> Mesh {
        Mesh::new(
            mesh.verts()
                .iter()
                .map(|point| std::array::from_fn(|axis| point[axis] + offset[axis]))
                .collect(),
            mesh.tris().to_vec(),
        )
        .unwrap()
    }

    fn merge_meshes(parts: &[Mesh]) -> Mesh {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for part in parts {
            let offset = u32::try_from(vertices.len()).unwrap();
            vertices.extend_from_slice(part.verts());
            triangles.extend(
                part.tris()
                    .iter()
                    .map(|triangle| triangle.map(|vertex| vertex + offset)),
            );
        }
        Mesh::new(vertices, triangles).unwrap()
    }

    /// One table's underside is genuinely buried by the blocker while an
    /// identical translated table remains live.  This is a real mixed initial
    /// build, not a manually assembled lifecycle fixture.
    fn real_mixed_initial_transcript_fixture() -> (
        InitialContactBuild,
        SpecializedContactPlan,
        FinalizedContactLoads,
        NativeSurface,
        FaceTopology,
        Mesh,
        Vec<f64>,
    ) {
        let buried = make_overhang_table().unwrap();
        let blocker = make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.56]).unwrap();
        let live = translated_mesh(make_overhang_table().unwrap(), [100.0, 0.0, 0.0]);
        real_initial_transcript_fixture_for(merge_meshes(&[buried, blocker, live]))
    }

    fn no_winner_frame() -> BuildFrame {
        transcript_frame()
    }

    /// The legal-boundary blocker keeps the actual initial stub live. Its
    /// perception sidecar truthfully marks every exterior face above the
    /// registered model-anchor delicacy ceiling, so a real finalized route
    /// completes as `UnroutedNoWinner` rather than minting an anchor.
    fn real_no_winner_initial_transcript_fixture() -> (
        InitialContactBuild,
        SpecializedContactPlan,
        FinalizedContactLoads,
        NativeSurface,
        FaceTopology,
        Mesh,
        Vec<f64>,
    ) {
        let table = make_overhang_table().unwrap();
        let boundary_blocker = make_box([30.0, 30.0, 1.0], [0.0, 0.0, 16.55]).unwrap();
        real_initial_transcript_fixture_for_frame_with_delicacy(
            merge_meshes(&[table, boundary_blocker]),
            no_winner_frame(),
            0.6,
        )
    }

    type CompletedInitialRoutes = (
        CcoController,
        Vec<u32>,
        Option<TerminalCcoRequest>,
        Option<TerminalLedgerEntry>,
        Vec<(u32, TerminalCcoOutcome)>,
    );

    fn route_initial_build(
        build: &mut InitialContactBuild,
        plan: &SpecializedContactPlan,
        loads: &FinalizedContactLoads,
        geometry: &AnchorGeometryAuthority<'_>,
        delicacy: &[f64],
        frame: BuildFrame,
    ) -> CompletedInitialRoutes {
        let pool = SurfaceAnchorPool::from_geometry(geometry, delicacy).unwrap();
        let mut controller = CcoController::default();
        let mut expected = build.routing_order().specialized_terminals().to_vec();
        expected.extend_from_slice(build.routing_order().regular_terminals());
        let mut routed_request = None;
        let mut skipped_ledger = None;
        let mut outcomes = Vec::new();
        for terminal in &expected {
            let ledger = build.forest().terminal_ledger(*terminal).unwrap();
            if ledger.state == TerminalLedgerState::Quarantined {
                skipped_ledger.get_or_insert(ledger);
                controller
                    .advance_skipped_terminal(build.forest(), ledger)
                    .unwrap();
                continue;
            }
            let request = TerminalCcoRequest::bind(build.forest(), *terminal, frame).unwrap();
            routed_request.get_or_insert_with(|| request.clone());
            let contact = plan
                .contacts()
                .iter()
                .find(|contact| contact.contact_id() == u64::from(*terminal))
                .unwrap();
            let outcome = controller
                .grow_terminal_with_finalized_anchor(
                    build.forest_mut(),
                    &request,
                    FinalizedAnchorTerminalRoute {
                        plan,
                        contact,
                        contact_id: u64::from(*terminal),
                        loads,
                        pool: &pool,
                        config: AnchorSearchConfig::current().unwrap(),
                        geometry,
                    },
                )
                .unwrap();
            outcomes.push((*terminal, outcome));
        }
        (
            controller,
            expected,
            routed_request,
            skipped_ledger,
            outcomes,
        )
    }

    fn no_winner_reason_signature(reason: &NoWinnerEvidence) -> String {
        let refusals = |refusals: &[AnchorFootRefusal]| {
            let mut signature = String::new();
            for refusal in refusals {
                let provenance = refusal.provenance();
                signature.push_str(&format!(
                    ";{}:{}:{}:{}",
                    match refusal.reason() {
                        AnchorFootRefusalReason::ReboundContact => 0,
                        AnchorFootRefusalReason::ReboundTerminal => 1,
                        AnchorFootRefusalReason::ReboundRewireBinding => 2,
                        AnchorFootRefusalReason::StaleConfig => 3,
                        AnchorFootRefusalReason::StaleGeometry => 4,
                        AnchorFootRefusalReason::StalePool => 5,
                        AnchorFootRefusalReason::FootBallBreached => 6,
                    },
                    provenance.physical_contact_id().unwrap_or(u64::MAX),
                    provenance.sample_index(),
                    provenance.source_sample_face_id(),
                ));
            }
            signature
        };
        match reason {
            NoWinnerEvidence::NoCandidatesGenerated { .. } => "0:0:0".to_owned(),
            NoWinnerEvidence::NoFeasibleCandidate {
                candidates_generated,
                anchor_refusals,
                ..
            } => format!("1:{candidates_generated}:0:{}", refusals(anchor_refusals)),
            NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
                candidates_generated,
                candidates_feasible,
                anchor_refusals,
                ..
            } => format!(
                "2:{candidates_generated}:{candidates_feasible}:{}",
                refusals(anchor_refusals)
            ),
        }
    }

    #[test]
    fn finalized_transcript_requires_real_initial_authority_and_seals_replay() {
        let (mut build, plan, loads, surface, topology, mesh, delicacy) =
            real_mixed_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (mut controller, expected, routed_request, skipped_ledger, _) = route_initial_build(
            &mut build,
            &plan,
            &loads,
            &geometry,
            &delicacy,
            transcript_frame(),
        );
        assert!(routed_request.is_some());
        assert!(skipped_ledger.is_some());

        let transcript = controller
            .finalize_transcript(&build, &plan, &loads, &geometry)
            .unwrap();
        assert_eq!(transcript.rows().len(), expected.len());
        assert_eq!(
            transcript
                .rows()
                .iter()
                .map(|row| row.terminal_id())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(transcript.plan_digest(), plan.digest());
        assert_eq!(transcript.finalized_load_digest(), loads.digest());
        assert_eq!(transcript.geometry_digest(), geometry.digest());
        assert_eq!(transcript.final_forest_digest(), build.forest().digest());
        assert_eq!(transcript.final_controller_digest(), controller.digest());
        assert_ne!(transcript.ordered_disposition_digest(), 0);
        assert!(transcript.validate_integrity().is_ok());
        assert!(transcript
            .rows()
            .iter()
            .any(|row| row.kind() == FinalizedDispositionKind::Routed));
        assert!(transcript.rows().iter().any(|row| {
            row.kind() == FinalizedDispositionKind::SkippedQuarantined
                && row.skipped_evidence().is_some()
        }));
        println!(
            "CCO_FINALIZED_TRANSCRIPT={:016x}:{:016x}:{:016x}",
            transcript.final_forest_digest(),
            transcript.final_controller_digest(),
            transcript.ordered_disposition_digest(),
        );
        assert!(matches!(
            controller.finalize_transcript(&build, &plan, &loads, &geometry),
            Err(CcoError::TranscriptSealed)
        ));
        let replay = routed_request.unwrap();
        let replay_contact = plan
            .contacts()
            .iter()
            .find(|contact| contact.contact_id() == replay.physical_contact_id)
            .unwrap();
        let replay_pool = SurfaceAnchorPool::from_geometry(&geometry, &delicacy).unwrap();
        assert!(matches!(
            controller.grow_terminal_with_finalized_anchor(
                build.forest_mut(),
                &replay,
                FinalizedAnchorTerminalRoute {
                    plan: &plan,
                    contact: replay_contact,
                    contact_id: replay.physical_contact_id,
                    loads: &loads,
                    pool: &replay_pool,
                    config: AnchorSearchConfig::current().unwrap(),
                    geometry: &geometry,
                },
            ),
            Err(CcoError::TranscriptSealed)
        ));
        assert!(matches!(
            controller.advance_skipped_terminal(build.forest(), skipped_ledger.unwrap()),
            Err(CcoError::TranscriptSealed)
        ));
    }

    #[test]
    fn finalized_transcript_retains_real_unrouted_no_winner_evidence() {
        let (mut build, plan, loads, surface, topology, mesh, delicacy) =
            real_no_winner_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (mut controller, expected, _, _, outcomes) = route_initial_build(
            &mut build,
            &plan,
            &loads,
            &geometry,
            &delicacy,
            no_winner_frame(),
        );
        let transcript = controller
            .finalize_transcript(&build, &plan, &loads, &geometry)
            .unwrap();
        assert_eq!(transcript.rows().len(), expected.len());
        let row = transcript
            .rows()
            .iter()
            .find(|row| row.kind() == FinalizedDispositionKind::UnroutedNoWinner)
            .expect("real routed initial build produced no-winner evidence");
        let route = row.route().expect("no-winner keeps exact route evidence");
        let reason = row
            .no_winner_reason()
            .expect("no-winner keeps its exact terminal reason");
        let outcome = outcomes
            .iter()
            .find(|(terminal, _)| *terminal == row.terminal_id())
            .map(|(_, outcome)| outcome)
            .unwrap();
        assert_eq!(route.physical_contact_id, row.physical_contact_id());
        assert_eq!(route.terminal_id, row.terminal_id());
        assert_eq!(route.original_key, row.original_key());
        assert!(matches!(
            reason,
            NoWinnerEvidence::NoFeasibleCandidate {
                candidates_generated: 1,
                anchor_refusals,
                ..
            } if anchor_refusals.is_empty()
        ));
        assert!(outcome.winner.is_none());
        assert!(matches!(
            outcome.disposition,
            Some(TerminalDisposition::UnroutedNoWinner {
                reason: NoWinnerEvidence::NoFeasibleCandidate {
                    candidates_generated: 1,
                    ref anchor_refusals,
                    ..
                },
                ..
            }) if anchor_refusals.is_empty()
        ));
        assert_eq!(outcome.candidates_generated, 1);
        assert_eq!(outcome.candidates_feasible, 0);
        assert_eq!(outcome.anchor_ray_batches, 0);
        assert_eq!(outcome.anchor_foot_batches, 0);
        assert_eq!(outcome.clearance_batches, 0);
        assert!(outcome.validated_model_anchor.is_none());
        assert!(outcome.anchor_refusals.is_empty());
        assert_eq!(
            build
                .forest()
                .terminal_ledger(row.terminal_id())
                .unwrap()
                .state,
            TerminalLedgerState::Retired
        );
        assert!(build.forest().terminal(row.terminal_id()).is_none());
        assert!(build.forest().member(row.original_key()).is_none());
        assert!(build
            .forest()
            .quarantine_record(row.original_key())
            .is_none());
        assert!(transcript.validate_integrity().is_ok());
    }

    #[test]
    fn all_numerical_zero_seat_refusals_survive_no_winner_evidence() {
        let t = Finite::new(-geom_kernel::MOLLER_PARAMETER_SLACK).unwrap();
        let zero = AnchorSeatRefusal {
            sample_index: 3,
            source_sample_face_id: 7,
            reason: crate::anchor::AnchorSeatRefusalReason::ZeroLengthSnappedAnchor,
            first_hit_t: Some(t),
        };
        let outcome = TerminalCcoOutcome {
            winner: None,
            disposition: None,
            candidates_generated: 0,
            candidates_feasible: 0,
            anchor_ray_batches: 1,
            anchor_foot_batches: 0,
            clearance_batches: 0,
            validated_model_anchor: None,
            anchor_seat_refusals: vec![zero],
            anchor_refusals: Vec::new(),
        };
        let NoWinnerEvidence::NoCandidatesGenerated {
            anchor_seat_refusals,
        } = no_winner_evidence(&outcome)
        else {
            panic!("all numerical-zero candidates must remain a no-winner result");
        };
        assert_eq!(anchor_seat_refusals, vec![zero]);
    }

    #[test]
    fn finalized_transcript_rejects_each_real_authority_mismatch_before_cadence() {
        let (build, plan, loads, surface, topology, mesh, _) = real_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (_, alien_plan, alien_loads, _, _, _, _) =
            real_initial_transcript_fixture_for(make_toed_slab().unwrap());

        assert!(matches!(
            CcoController::default().finalize_transcript(&build, &alien_plan, &loads, &geometry),
            Err(CcoError::TranscriptPlanMismatch)
        ));
        assert!(matches!(
            CcoController::default().finalize_transcript(&build, &plan, &alien_loads, &geometry),
            Err(CcoError::TranscriptLoadMismatch)
        ));

        let rebound_surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            NativeSurfaceConfig {
                grid_pitch_mm: 0.4,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        let rebound_geometry =
            AnchorGeometryAuthority::new(&mesh, &topology, &rebound_surface).unwrap();
        assert!(matches!(
            CcoController::default().finalize_transcript(&build, &plan, &loads, &rebound_geometry,),
            Err(CcoError::TranscriptGeometryMismatch)
        ));

        let (
            mut corrupt_build,
            corrupt_plan,
            corrupt_loads,
            corrupt_surface,
            corrupt_topology,
            corrupt_mesh,
            _,
        ) = real_initial_transcript_fixture();
        let corrupt_geometry =
            AnchorGeometryAuthority::new(&corrupt_mesh, &corrupt_topology, &corrupt_surface)
                .unwrap();
        corrupt_build.test_corrupt_initial_forest_digest();
        assert!(matches!(
            CcoController::default().finalize_transcript(
                &corrupt_build,
                &corrupt_plan,
                &corrupt_loads,
                &corrupt_geometry,
            ),
            Err(CcoError::TranscriptInitialForestMismatch)
        ));
    }

    #[test]
    fn finalized_transcript_rejects_pending_duplicate_extra_and_inconsistent_disposition() {
        let (build, plan, loads, surface, topology, mesh, _) = real_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        assert!(matches!(
            CcoController::default().finalize_transcript(&build, &plan, &loads, &geometry),
            Err(CcoError::TranscriptCadenceMismatch)
        ));

        let (mut build, plan, loads, surface, topology, mesh, delicacy) =
            real_mixed_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (mut controller, expected, _, _, _) = route_initial_build(
            &mut build,
            &plan,
            &loads,
            &geometry,
            &delicacy,
            transcript_frame(),
        );

        let missing = controller.terminal_cadence.pop().unwrap();
        assert!(matches!(
            controller.finalize_transcript(&build, &plan, &loads, &geometry),
            Err(CcoError::TranscriptCadenceMismatch)
        ));
        controller.terminal_cadence.push(missing);

        let duplicate = expected[0];
        controller.terminal_cadence.push(duplicate);
        assert!(matches!(
            controller.finalize_transcript(&build, &plan, &loads, &geometry),
            Err(CcoError::TranscriptCadenceMismatch)
        ));
        controller.terminal_cadence.pop();

        controller.terminal_cadence.push(u32::MAX);
        assert!(matches!(
            controller.finalize_transcript(&build, &plan, &loads, &geometry),
            Err(CcoError::TranscriptCadenceMismatch)
        ));
        controller.terminal_cadence.pop();

        let routed = expected
            .iter()
            .copied()
            .find(|terminal| {
                matches!(
                    controller.terminal_disposition(*terminal),
                    Some(TerminalDisposition::Routed { .. })
                )
            })
            .unwrap();
        let original_key = build.forest().terminal_key(routed).unwrap();
        controller.terminals.get_mut(&routed).unwrap().disposition =
            TerminalDisposition::SkippedQuarantined {
                terminal_id: routed,
                original_key,
                forest_digest: build.forest().digest(),
                controller_digest_before: controller.digest(),
            };
        assert!(matches!(
            controller.finalize_transcript(&build, &plan, &loads, &geometry),
            Err(CcoError::TranscriptDispositionMismatch(terminal)) if terminal == routed
        ));
    }

    #[test]
    fn finalized_transcript_detects_internal_row_and_header_tampering() {
        let (mut build, plan, loads, surface, topology, mesh, delicacy) =
            real_mixed_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (mut controller, _, _, _, _) = route_initial_build(
            &mut build,
            &plan,
            &loads,
            &geometry,
            &delicacy,
            transcript_frame(),
        );
        let transcript = controller
            .finalize_transcript(&build, &plan, &loads, &geometry)
            .unwrap();
        let mut tampered_row = transcript.clone();
        tampered_row.rows[0].cadence_index = tampered_row.rows[0].cadence_index.wrapping_add(1);
        assert!(matches!(
            tampered_row.validate_integrity(),
            Err(CcoError::TranscriptDigestMismatch)
        ));
        let mut tampered_header = transcript;
        tampered_header.final_forest_digest = tampered_header.final_forest_digest.wrapping_add(1);
        assert!(matches!(
            tampered_header.validate_integrity(),
            Err(CcoError::TranscriptDigestMismatch)
        ));
    }

    #[test]
    fn finalized_transcript_thread_sweep_is_exactly_identical() {
        const CHILD: &str = "PLINTH_FINALIZED_CCO_TRANSCRIPT_SWEEP_CHILD";
        const LINE: &str = "CCO_FINALIZED_TRANSCRIPT_SWEEP=";
        if std::env::var_os(CHILD).is_some() {
            let (mut build, plan, loads, surface, topology, mesh, delicacy) =
                real_mixed_initial_transcript_fixture();
            let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
            let (mut controller, _, _, _, _) = route_initial_build(
                &mut build,
                &plan,
                &loads,
                &geometry,
                &delicacy,
                transcript_frame(),
            );
            let transcript = controller
                .finalize_transcript(&build, &plan, &loads, &geometry)
                .unwrap();
            let mut rows = String::new();
            for row in transcript.rows() {
                rows.push_str(&format!(
                    ";{}:{}:{}:{}:{}",
                    row.cadence_index(),
                    row.physical_contact_id(),
                    row.terminal_id(),
                    row.original_key().slot(),
                    match row.kind() {
                        FinalizedDispositionKind::Routed => 0,
                        FinalizedDispositionKind::SkippedQuarantined => 1,
                        FinalizedDispositionKind::UnroutedNoWinner => 2,
                    },
                ));
                match row.route() {
                    Some(route) => rows.push_str(&format!(
                        ":r{}:{}:{}:{}:{}:{}:{}:{}:{}",
                        route.physical_contact_id,
                        route.terminal_id,
                        route.original_key.slot(),
                        route.original_key.generation(),
                        route.plan_digest,
                        route.loads_digest,
                        route.geometry_digest,
                        route.forest_digest_before,
                        route.controller_digest_before,
                    )),
                    None => rows.push_str(":-"),
                }
                match row.skipped_evidence() {
                    Some(skipped) => rows.push_str(&format!(
                        ":s{}:{}",
                        skipped.forest_digest(),
                        skipped.controller_digest_before(),
                    )),
                    None => rows.push_str(":-"),
                }
            }
            let (mut no_build, no_plan, no_loads, no_surface, no_topology, no_mesh, no_delicacy) =
                real_no_winner_initial_transcript_fixture();
            let no_geometry =
                AnchorGeometryAuthority::new(&no_mesh, &no_topology, &no_surface).unwrap();
            let (mut no_controller, _, _, _, no_outcomes) = route_initial_build(
                &mut no_build,
                &no_plan,
                &no_loads,
                &no_geometry,
                &no_delicacy,
                no_winner_frame(),
            );
            let no_transcript = no_controller
                .finalize_transcript(&no_build, &no_plan, &no_loads, &no_geometry)
                .unwrap();
            let no_row = no_transcript
                .rows()
                .iter()
                .find(|row| row.kind() == FinalizedDispositionKind::UnroutedNoWinner)
                .unwrap();
            let no_route = no_row.route().unwrap();
            let no_reason = no_row.no_winner_reason().unwrap();
            let no_outcome = no_outcomes
                .iter()
                .find(|(terminal, _)| *terminal == no_row.terminal_id())
                .map(|(_, outcome)| outcome)
                .unwrap();
            println!(
                "{LINE}{:016x}:{:016x}:{rows}|no={:016x}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
                transcript.digest(),
                transcript.ordered_disposition_digest(),
                no_transcript.digest(),
                no_row.cadence_index(),
                no_row.physical_contact_id(),
                no_route.forest_digest_before,
                no_route.controller_digest_before,
                no_winner_reason_signature(no_reason),
                no_outcome.candidates_generated,
                no_outcome.candidates_feasible,
                no_outcome.anchor_ray_batches,
                no_outcome.anchor_foot_batches,
                no_outcome.clearance_batches,
                no_outcome.anchor_refusals.len(),
            );
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let mut results = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("cco::tests::finalized_transcript_thread_sweep_is_exactly_identical")
                .arg("--nocapture")
                .env(CHILD, "1")
                .env("PLINTH_THREADS", threads.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            results.push(
                String::from_utf8(output.stdout)
                    .unwrap()
                    .lines()
                    .find_map(|line| line.strip_prefix(LINE))
                    .expect("child emitted finalized transcript evidence")
                    .to_owned(),
            );
        }
        assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn frozen_xia_vectors_sum_to_right_angle() {
        let equal = xia_angles(1.0, 1.0);
        assert!((equal.child_one_turn_deg - 45.0).abs() < 1e-12);
        assert!((equal.child_two_turn_deg - 45.0).abs() < 1e-12);
        assert_eq!(equal.between_children_deg, 90.0);
        let split = xia_angles(1.0, 3.0);
        assert!((split.child_one_turn_deg - 60.0).abs() < 1e-12);
        assert!((split.child_two_turn_deg - 30.0).abs() < 1e-12);
        assert!((split.child_one_turn_deg + split.child_two_turn_deg - 90.0).abs() < 1e-12);
        assert_eq!(xia_angles(0.0, 0.0).child_one_turn_deg, 45.0);
    }

    #[test]
    fn brace_price_reads_the_single_o10_placeholder_authority() {
        let compatibility = LegacyCompatibility::o10_live_current_support_and_raft();
        let authority = compatibility.brace_free_span_mm();
        assert_eq!(authority.diagnostic().get(), 18.0);
        assert!(authority.note.contains("O-10 LegacyCompatibility only"));
    }

    #[test]
    fn bounded_weiszfeld_matches_frozen_numeric_vector() {
        let result = weighted_fermat_point(
            &[
                ([0.0, 0.0, 0.0], 1.0),
                ([10.0, 0.0, 0.0], 2.0),
                ([0.0, 10.0, 0.0], 3.0),
            ],
            [2.0, 2.0, 0.0],
            12,
        );
        assert!((result[0] - 1.002_486_160_390_787_5).abs() < 1e-15);
        assert!((result[1] - 8.259_149_868_747_48).abs() < 1e-15);
        assert_eq!(result[2], 0.0);
    }

    #[test]
    fn radius_interpolation_uses_distinct_entry_and_exit_ends() {
        let mut key_source = Forest::new();
        let temporary = Member::new(MemberSpec {
            p0_mm: [Finite::new(0.0).unwrap(); 3],
            p1_mm: [Finite::new(1.0).unwrap(); 3],
            r0_mm: TerminalEndRadius::new(PositiveFinite::new(0.2).unwrap()),
            r1_mm: RootwardEndRadius::new(PositiveFinite::new(0.2).unwrap()),
            kind: MemberKind::Stub,
            owner: Owner::Contact(0),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: Finite::new(1.0).unwrap(),
        });
        let key = key_source.add_member(temporary);
        key_source.register_terminal(0, key).unwrap();
        let request = TerminalCcoRequest::bind(
            &key_source,
            0,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let host = member(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 10.0],
            PositiveFinite::new(0.3).unwrap(),
            PositiveFinite::new(1.2).unwrap(),
            MemberKind::Trunk,
            &request,
        )
        .unwrap();
        let midpoint = radius_along(&host, [0.0, 0.0, 5.0]).unwrap();
        assert!((midpoint.get() - 0.75).abs() < 1e-12);
    }

    #[test]
    fn terminal_request_binds_the_rootward_attachment_radius() {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let mut forest = Forest::new();
        let stub = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(0.0), finite(0.0), finite(10.0)],
            p1_mm: [finite(0.0), finite(0.0), finite(12.0)],
            r0_mm: TerminalEndRadius::new(positive(0.11)),
            r1_mm: RootwardEndRadius::new(positive(0.73)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(9),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: finite(1.0),
        }));
        forest.register_terminal(9, stub).unwrap();

        let request =
            TerminalCcoRequest::bind(&forest, 9, BuildFrame::new(finite(0.0), positive(5.0)))
                .unwrap();

        assert_eq!(request.physical_contact_id, 9);
        assert_eq!(request.stub_end_mm(), forest.member(stub).unwrap().p1_mm());
        assert_eq!(request.tip_radius_mm(), positive(0.73));
        assert_ne!(request.tip_radius_mm(), positive(0.11));
    }

    #[test]
    fn murray_propagation_uses_entry_radius_and_ignores_dead_children() {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let make = |r0: f64, r1: f64| {
            Member::new(MemberSpec {
                p0_mm: [finite(0.0), finite(0.0), finite(0.0)],
                p1_mm: [finite(0.0), finite(0.0), finite(1.0)],
                r0_mm: TerminalEndRadius::new(positive(r0)),
                r1_mm: RootwardEndRadius::new(positive(r1)),
                kind: MemberKind::Shoot,
                owner: Owner::Generated {
                    family: "murray-vector".to_owned(),
                    ordinal: 0,
                },
                embed_end: None,
                carried_mm2: finite(0.0),
            })
        };
        let mut forest = Forest::new();
        let parent = forest.add_member(make(0.3, 1.2));
        let child = forest.add_member(make(0.5, 0.5));
        forest.set_parent(child, Some(parent)).unwrap();
        uniformly_size_ancestor_path(&mut forest, parent, RootwardEndRadius::new(positive(0.5)))
            .unwrap();
        let propagated = forest
            .member(parent)
            .unwrap()
            .terminal_radius_mm()
            .get()
            .get();
        assert!((propagated - (0.3_f64.powi(4) + 0.5_f64.powi(4)).powf(0.25)).abs() < 1e-12);

        let mut without_dead = Forest::new();
        let parent = without_dead.add_member(make(0.3, 1.2));
        let dead = without_dead.add_member(make(0.9, 0.9));
        without_dead.set_parent(dead, Some(parent)).unwrap();
        without_dead.retire(dead).unwrap();
        uniformly_size_ancestor_path(
            &mut without_dead,
            parent,
            RootwardEndRadius::new(positive(0.3)),
        )
        .unwrap();
        assert!(
            without_dead
                .member(parent)
                .unwrap()
                .terminal_radius_mm()
                .get()
                .get()
                < 0.9
        );
    }

    #[test]
    fn host_cache_treats_reused_low_slot_as_unindexed_tail() {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let make_host = |x: f64| {
            Member::new(MemberSpec {
                p0_mm: [finite(x), finite(0.0), finite(0.0)],
                p1_mm: [finite(x), finite(0.0), finite(10.0)],
                r0_mm: TerminalEndRadius::new(positive(0.4)),
                r1_mm: RootwardEndRadius::new(positive(0.8)),
                kind: MemberKind::Trunk,
                owner: Owner::Generated {
                    family: "cache-generation-vector".to_owned(),
                    ordinal: 0,
                },
                embed_end: None,
                carried_mm2: finite(1.0),
            })
        };
        let mut forest = Forest::new();
        let old = forest.add_member(make_host(50.0));
        let mut cache = HostCache::default();
        cache.rebuild(&forest);

        forest.retire(old).unwrap();
        let replacement = forest.add_member(make_host(0.0));
        assert_eq!(replacement.slot(), old.slot());
        assert_ne!(replacement, old);

        assert_eq!(
            cache.nearest([0.0, 0.0, 5.0], 1, &forest),
            vec![replacement]
        );
    }

    fn quarantined_cadence_fixture(
        count: u32,
        first_terminal: u32,
    ) -> (Forest, Vec<TerminalLedgerEntry>, MemberKey) {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let mut forest = Forest::new();
        let host = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(4.0), finite(0.0), finite(0.0)],
            p1_mm: [finite(4.0), finite(0.0), finite(10.0)],
            r0_mm: TerminalEndRadius::new(positive(0.4)),
            r1_mm: RootwardEndRadius::new(positive(0.8)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "cadence-host".to_owned(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: finite(1.0),
        }));
        let skipped = (first_terminal..first_terminal + count)
            .map(|terminal| {
                let stub = forest.add_member(Member::new(MemberSpec {
                    p0_mm: [finite(0.0), finite(0.0), finite(10.0)],
                    p1_mm: [finite(0.0), finite(0.0), finite(12.0)],
                    r0_mm: TerminalEndRadius::new(positive(0.2)),
                    r1_mm: RootwardEndRadius::new(positive(0.35)),
                    kind: MemberKind::Stub,
                    owner: Owner::Contact(terminal),
                    embed_end: Some(EmbedEnd::Terminalward),
                    carried_mm2: finite(1.0),
                }));
                forest.register_terminal(terminal, stub).unwrap();
                forest.quarantine(stub).unwrap();
                forest.terminal_ledger(terminal).unwrap()
            })
            .collect::<Vec<_>>();
        (forest, skipped, host)
    }

    #[test]
    fn quarantined_terminal_advances_the_25_terminal_host_cache_cadence() {
        let (forest, skipped, host) = quarantined_cadence_fixture(26, 0);
        let before = forest.digest();
        let mut processed = CcoController::default();
        let mut quarantined = CcoController::default();

        for (index, skipped) in skipped.into_iter().enumerate() {
            processed.finish_terminal(
                &forest,
                skipped.contact,
                TerminalCadenceRecord {
                    disposition: TerminalDisposition::Routed {
                        route: TerminalRouteEvidence {
                            physical_contact_id: u64::from(skipped.contact),
                            terminal_id: skipped.contact,
                            original_key: skipped.original_key,
                            plan_digest: 0,
                            loads_digest: 0,
                            geometry_digest: 0,
                            forest_digest_before: before,
                            controller_digest_before: 0,
                        },
                        winner: CcoCandidateEvidence {
                            family: CandidateFamily::PlateDescent,
                            generation_index: 0,
                            price_ml: 0.0,
                            clearance_rows: 0,
                        },
                    },
                },
            );
            quarantined
                .advance_skipped_terminal(&forest, skipped)
                .unwrap();

            if matches!(index + 1, 24..=26) {
                assert_eq!(quarantined.inserted, processed.inserted);
                assert_eq!(
                    quarantined.hosts.indexed_members,
                    processed.hosts.indexed_members
                );
                assert_eq!(quarantined.hosts.samples, processed.hosts.samples);
                assert_eq!(
                    quarantined.hosts.nearest([4.0, 0.0, 5.0], 1, &forest),
                    processed.hosts.nearest([4.0, 0.0, 5.0], 1, &forest),
                );
                assert_eq!(
                    quarantined.hosts.nearest([4.0, 0.0, 5.0], 1, &forest),
                    vec![host],
                );
                assert_eq!(forest.digest(), before);
                assert_ne!(quarantined.digest(), processed.digest());
                match index + 1 {
                    24 => assert!(quarantined.hosts.samples.is_empty()),
                    25 | 26 => assert_eq!(quarantined.hosts.samples.len(), 3),
                    _ => unreachable!("boundary filter above is exact"),
                }
            }
        }
        assert!(!processed.hosts.samples.is_empty());
        assert_eq!(processed.hosts.samples.len(), 3);
    }

    #[test]
    fn skipped_terminal_evidence_is_one_shot_and_replay_is_inert() {
        let (forest, skipped, _) = quarantined_cadence_fixture(1, 40);
        let mut controller = CcoController::default();
        controller
            .advance_skipped_terminal(&forest, skipped[0])
            .unwrap();
        let digest = controller.digest();
        let forest_digest = forest.digest();

        assert!(matches!(
            controller.advance_skipped_terminal(&forest, skipped[0]),
            Err(CcoError::TerminalCadenceReplay(40))
        ));
        assert_eq!(controller.inserted, 1);
        assert_eq!(controller.digest(), digest);
        assert_eq!(forest.digest(), forest_digest);
    }

    #[test]
    fn skipped_terminal_rejects_live_retired_and_stale_ledger_evidence() {
        let (live_forest, request, _) = growth_fixture();
        let live = live_forest.terminal_ledger(request.terminal_id).unwrap();
        let mut controller = CcoController::default();
        let initial = controller.digest();
        assert!(matches!(
            controller.advance_skipped_terminal(&live_forest, live),
            Err(CcoError::InvalidSkippedTerminal(7))
        ));

        let mut retired_forest = live_forest.clone();
        retired_forest.retire(request.stub).unwrap();
        let retired = retired_forest.terminal_ledger(request.terminal_id).unwrap();
        assert_eq!(retired.state, TerminalLedgerState::Retired);
        assert!(matches!(
            controller.advance_skipped_terminal(&retired_forest, retired),
            Err(CcoError::InvalidSkippedTerminal(7))
        ));

        let (quarantined_forest, skipped, host) = quarantined_cadence_fixture(1, 7);
        let stale = TerminalLedgerEntry {
            original_key: host,
            ..skipped[0]
        };
        assert!(matches!(
            controller.advance_skipped_terminal(&quarantined_forest, stale),
            Err(CcoError::InvalidSkippedTerminal(7))
        ));
        assert_eq!(controller.inserted, 0);
        assert_eq!(controller.digest(), initial);
    }

    #[test]
    fn terminal_cadence_cannot_cross_or_repeat_grown_and_skipped_dispositions() {
        let (mut grown_forest, request, surface) = growth_fixture();
        let mut grown = CcoController::default();
        grown
            .grow_terminal_for_test(&mut grown_forest, &request, &surface)
            .unwrap();
        let grown_digest = grown.digest();
        let grown_forest_digest = grown_forest.digest();
        assert!(matches!(
            grown.grow_terminal_for_test(&mut grown_forest, &request, &surface),
            Err(CcoError::TerminalCadenceReplay(7))
        ));
        assert_eq!(grown.digest(), grown_digest);
        assert_eq!(grown_forest.digest(), grown_forest_digest);

        let (quarantined_forest, skipped, _) = quarantined_cadence_fixture(1, 7);
        assert!(matches!(
            grown.advance_skipped_terminal(&quarantined_forest, skipped[0]),
            Err(CcoError::TerminalCadenceReplay(7))
        ));
        assert_eq!(grown.digest(), grown_digest);

        let (mut live_forest, request, surface) = growth_fixture();
        let live_digest = live_forest.digest();
        let mut skipped_first = CcoController::default();
        skipped_first
            .advance_skipped_terminal(&quarantined_forest, skipped[0])
            .unwrap();
        let skipped_digest = skipped_first.digest();
        assert!(matches!(
            skipped_first.grow_terminal_for_test(&mut live_forest, &request, &surface),
            Err(CcoError::TerminalCadenceReplay(7))
        ));
        assert_eq!(skipped_first.digest(), skipped_digest);
        assert_eq!(live_forest.digest(), live_digest);
    }

    #[test]
    fn skipped_terminal_thread_sweep_is_digest_identical() {
        const CHILD: &str = "PLINTH_CCO_SKIPPED_SWEEP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let (forest, skipped, _) = quarantined_cadence_fixture(26, 0);
            let mut controller = CcoController::default();
            for skipped in skipped {
                controller
                    .advance_skipped_terminal(&forest, skipped)
                    .unwrap();
            }
            println!(
                "CCO_SKIPPED_SWEEP={:016x}:{:016x}:{:?}",
                forest.digest(),
                controller.digest(),
                controller.hosts.nearest([4.0, 0.0, 5.0], 1, &forest),
            );
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let mut digests = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("cco::tests::skipped_terminal_thread_sweep_is_digest_identical")
                .arg("--nocapture")
                .env(CHILD, "1")
                .env("PLINTH_THREADS", threads.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            digests.push(
                stdout
                    .lines()
                    .find_map(|line| line.strip_prefix("CCO_SKIPPED_SWEEP="))
                    .expect("child emitted skipped cadence digest")
                    .to_owned(),
            );
        }
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn bound_request_rejects_endpoint_load_radius_and_generation_changes() {
        let (forest, request, _) = growth_fixture();

        let mut changed_endpoint = forest.clone();
        changed_endpoint
            .consume_terminal(request.terminal_id, request.stub)
            .unwrap();
        changed_endpoint
            .set_endpoints(
                request.stub,
                request.stub_snapshot.p0_mm(),
                [
                    Finite::new(0.25).unwrap(),
                    request.stub_end_mm()[1],
                    request.stub_end_mm()[2],
                ],
            )
            .unwrap();
        assert!(matches!(
            request.validate(&changed_endpoint),
            Err(CcoError::StaleTerminal(7))
        ));

        let mut changed_load = forest.clone();
        changed_load
            .consume_terminal(request.terminal_id, request.stub)
            .unwrap();
        changed_load
            .set_carried_mm2(request.stub, Finite::new(3.0).unwrap())
            .unwrap();
        assert!(matches!(
            request.validate(&changed_load),
            Err(CcoError::StaleTerminal(7))
        ));

        let mut changed_radius = forest.clone();
        changed_radius
            .consume_terminal(request.terminal_id, request.stub)
            .unwrap();
        changed_radius
            .set_radii(
                request.stub,
                TerminalEndRadius::new(PositiveFinite::new(0.21).unwrap()),
                request.stub_snapshot.rootward_radius_mm(),
            )
            .unwrap();
        assert!(matches!(
            request.validate(&changed_radius),
            Err(CcoError::StaleTerminal(7))
        ));

        let mut reused = forest.clone();
        reused.retire(request.stub).unwrap();
        let replacement = reused.add_member(request.stub_snapshot.clone());
        assert_eq!(replacement.slot(), request.stub.slot());
        assert_ne!(replacement, request.stub);
        assert!(matches!(
            request.validate(&reused),
            Err(CcoError::StaleTerminal(7))
        ));
    }

    #[test]
    fn anchor_route_requires_the_exact_terminal_binding() {
        let (_, request, _) = growth_fixture();
        let different_endpoint = AnchorTerminalBinding::new(
            request.terminal_id,
            request.stub,
            [
                Finite::new(1.0).unwrap(),
                request.stub_end_mm()[1],
                request.stub_end_mm()[2],
            ],
        );
        assert!(matches!(
            validate_anchor_route(&request, different_endpoint),
            Err(CcoError::AnchorBindingMismatch)
        ));

        let exact =
            AnchorTerminalBinding::new(request.terminal_id, request.stub, request.stub_end_mm());
        assert!(validate_anchor_route(&request, exact).is_ok());
    }

    #[test]
    fn request_binding_refuses_non_stub_mismatched_owner_and_parented_state() {
        let frame = BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap());
        let (forest, request, _) = growth_fixture();

        let mut wrong_kind = forest.clone();
        assert_eq!(
            wrong_kind.set_kind(request.stub, MemberKind::Shoot),
            Err(ForestError::RegisteredTerminalMutation { terminal: 7 })
        );
        wrong_kind
            .consume_terminal(request.terminal_id, request.stub)
            .unwrap();
        wrong_kind
            .set_kind(request.stub, MemberKind::Shoot)
            .unwrap();
        assert!(matches!(
            TerminalCcoRequest::bind(&wrong_kind, 7, frame),
            Err(CcoError::InvalidTerminalStub { .. })
        ));

        let mut parented = forest.clone();
        let parent = parented.add_member(Member::new(MemberSpec {
            p0_mm: request.stub_end_mm(),
            p1_mm: [
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
                Finite::new(0.0).unwrap(),
            ],
            r0_mm: TerminalEndRadius::new(PositiveFinite::new(0.35).unwrap()),
            r1_mm: RootwardEndRadius::new(PositiveFinite::new(0.8).unwrap()),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "binding-vector".to_owned(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: Finite::new(2.0).unwrap(),
        }));
        assert_eq!(
            parented.set_parent(request.stub, Some(parent)),
            Err(ForestError::RegisteredTerminalMutation { terminal: 7 })
        );
        parented
            .consume_terminal(request.terminal_id, request.stub)
            .unwrap();
        parented.set_parent(request.stub, Some(parent)).unwrap();
        assert!(matches!(
            TerminalCcoRequest::bind(&parented, 7, frame),
            Err(CcoError::InvalidTerminalStub { .. })
        ));

        let mut wrong_owner = Forest::new();
        let stub = wrong_owner.add_member(Member::new(MemberSpec {
            p0_mm: request.stub_snapshot.p0_mm(),
            p1_mm: request.stub_snapshot.p1_mm(),
            r0_mm: request.stub_snapshot.terminal_radius_mm(),
            r1_mm: request.stub_snapshot.rootward_radius_mm(),
            kind: MemberKind::Stub,
            owner: Owner::Contact(8),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: request.stub_snapshot.carried_mm2(),
        }));
        assert!(matches!(
            wrong_owner.register_terminal(7, stub),
            Err(ForestError::InvalidTerminalMember { terminal: 7 })
        ));
    }

    fn growth_fixture() -> (Forest, TerminalCcoRequest, NativeSurface) {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let mut forest = Forest::new();
        let stub = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(0.0), finite(0.0), finite(10.0)],
            p1_mm: [finite(0.0), finite(0.0), finite(12.5)],
            r0_mm: TerminalEndRadius::new(positive(0.2)),
            r1_mm: RootwardEndRadius::new(positive(0.35)),
            kind: MemberKind::Stub,
            owner: Owner::Contact(7),
            embed_end: Some(EmbedEnd::Terminalward),
            carried_mm2: finite(2.0),
        }));
        forest.register_terminal(7, stub).unwrap();
        let request =
            TerminalCcoRequest::bind(&forest, 7, BuildFrame::new(finite(0.0), positive(5.0)))
                .unwrap();
        let vertices = vec![
            [100.0, 100.0, 100.0],
            [101.0, 100.0, 100.0],
            [100.0, 101.0, 100.0],
            [100.0, 100.0, 101.0],
        ];
        let triangles = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let surface = NativeSurface::from_indexed(
            &vertices,
            &triangles,
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: 0.4,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        (forest, request, surface)
    }

    fn no_winner_fixture() -> (Forest, TerminalCcoRequest, NativeSurface) {
        let (forest, request, surface) = growth_fixture();
        let request = TerminalCcoRequest::bind(
            &forest,
            request.terminal_id,
            // The base plate descriptor is below this plate and every spiral
            // descriptor is lower still, so all generated descriptors refuse
            // before a clearance query or forest mutation.
            BuildFrame::new(
                Finite::new(20.0).unwrap(),
                PositiveFinite::new(5.0).unwrap(),
            ),
        )
        .unwrap();
        (forest, request, surface)
    }

    #[test]
    fn no_winner_retires_the_exact_stub_and_seals_unrouted_evidence() {
        let (mut forest, request, surface) = no_winner_fixture();
        let before = forest.digest();
        let mut controller = CcoController::default();
        let outcome = controller
            .grow_terminal_for_test(&mut forest, &request, &surface)
            .unwrap();

        assert!(outcome.winner.is_none());
        assert_eq!(outcome.candidates_generated, 1);
        assert_eq!(outcome.candidates_feasible, 0);
        assert_eq!(outcome.clearance_batches, 0);
        assert_ne!(forest.digest(), before);
        assert_eq!(forest.terminal(request.terminal_id), None);
        assert_eq!(forest.terminal_key(request.terminal_id), Some(request.stub));
        assert_eq!(
            forest.terminal_ledger(request.terminal_id).unwrap().state,
            TerminalLedgerState::Retired
        );
        assert!(forest.quarantine_record(request.stub).is_none());
        assert_eq!(
            forest
                .terminal_ledger_evidence(request.terminal_id)
                .unwrap()
                .original_key,
            request.stub
        );
        assert!(matches!(
            outcome.disposition,
            Some(TerminalDisposition::UnroutedNoWinner {
                reason: NoWinnerEvidence::NoFeasibleCandidate { .. },
                ..
            })
        ));
        assert!(matches!(
            controller.terminal_disposition(request.terminal_id),
            Some(TerminalDisposition::UnroutedNoWinner { .. })
        ));

        let view = crate::joints::FilteredLiveView::current(
            &forest,
            &[request.terminal_id],
            crate::joints::LiveViewLimits::new(8, 1, 4096),
        )
        .unwrap();
        assert!(!view.keys().contains(&request.stub));
        assert!(matches!(
            controller.grow_terminal_for_test(&mut forest, &request, &surface),
            Err(CcoError::TerminalCadenceReplay(7))
        ));
    }

    #[derive(Clone, Copy)]
    struct RewireFixtureKeys {
        root: MemberKey,
        old_parent: MemberKey,
        child: MemberKey,
        eligible: MemberKey,
        braced_host: MemberKey,
        brace: MemberKey,
        dead: MemberKey,
    }

    fn rewire_member(p0: [f64; 3], p1: [f64; 3], kind: MemberKind, ordinal: u32) -> Member {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        Member::new(MemberSpec {
            p0_mm: p0.map(finite),
            p1_mm: p1.map(finite),
            r0_mm: TerminalEndRadius::new(positive(0.45)),
            r1_mm: RootwardEndRadius::new(positive(0.65)),
            kind,
            owner: Owner::Generated {
                family: "rewire-vector".to_owned(),
                ordinal,
            },
            embed_end: None,
            carried_mm2: finite(2.0),
        })
    }

    struct RewireAnchorFixture {
        mesh: Mesh,
        topology: FaceTopology,
        surface: NativeSurface,
        samples: Vec<SurfaceAnchorSample>,
        sidecar: Vec<f64>,
    }

    impl RewireAnchorFixture {
        fn with_route<T>(&self, use_route: impl FnOnce(RewireAnchorRoute<'_>) -> T) -> T {
            let geometry =
                AnchorGeometryAuthority::new(&self.mesh, &self.topology, &self.surface).unwrap();
            use_route(RewireAnchorRoute {
                samples: &self.samples,
                delicacy_by_face: Some(&self.sidecar),
                config: AnchorSearchConfig::current().unwrap(),
                geometry: &geometry,
            })
        }
    }

    fn rewire_anchor_fixture() -> RewireAnchorFixture {
        let vertices = vec![
            [-20.0, -20.0, 0.0],
            [20.0, -20.0, 0.0],
            [20.0, 20.0, 0.0],
            [-20.0, 20.0, 0.0],
            [-20.0, -20.0, 5.0],
            [20.0, -20.0, 5.0],
            [20.0, 20.0, 5.0],
            [-20.0, 20.0, 5.0],
        ];
        let triangles = vec![
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
        ];
        let mesh = Mesh::new(vertices, triangles).unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        RewireAnchorFixture {
            sidecar: vec![0.0; surface.nfaces()],
            samples: vec![SurfaceAnchorSample {
                source_face_id: 0,
                point_mm: [0.0, 0.0, 0.0],
                delicacy: 0.0,
            }],
            mesh,
            topology,
            surface,
        }
    }

    fn empty_far_anchor_fixture() -> RewireAnchorFixture {
        let vertices = vec![
            [100.0, 100.0, 100.0],
            [101.0, 100.0, 100.0],
            [100.0, 101.0, 100.0],
            [100.0, 100.0, 101.0],
        ];
        let triangles = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let mesh = Mesh::new(vertices, triangles).unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: 0.4,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        RewireAnchorFixture {
            sidecar: vec![0.0; surface.nfaces()],
            samples: Vec::new(),
            mesh,
            topology,
            surface,
        }
    }

    fn alias_triangle_fixture(vertices: [[f64; 3]; 3]) -> RewireAnchorFixture {
        let mesh = Mesh::new(vertices.to_vec(), vec![[0, 1, 2]]).unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: 0.4,
                max_grid_bytes: 8 * 1024 * 1024,
            },
        )
        .unwrap();
        RewireAnchorFixture {
            sidecar: vec![0.0; surface.nfaces()],
            samples: Vec::new(),
            mesh,
            topology,
            surface,
        }
    }

    fn near_anchor_fixture(top_z: f64) -> RewireAnchorFixture {
        let bottom_z = top_z - 10.0;
        let vertices = vec![
            [-20.0, -20.0, bottom_z],
            [20.0, -20.0, bottom_z],
            [20.0, 20.0, bottom_z],
            [-20.0, 20.0, bottom_z],
            [-20.0, -20.0, top_z],
            [20.0, -20.0, top_z],
            [20.0, 20.0, top_z],
            [-20.0, 20.0, top_z],
        ];
        let triangles = vec![
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
        ];
        let mesh = Mesh::new(vertices, triangles).unwrap();
        let topology = build_face_topology(&mesh, WeldPolicy::ExactBits).unwrap();
        let surface = NativeSurface::from_indexed(
            mesh.verts(),
            mesh.tris(),
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: 1.0,
                max_grid_bytes: 64 * 1024 * 1024,
            },
        )
        .unwrap();
        RewireAnchorFixture {
            sidecar: vec![0.0; surface.nfaces()],
            samples: vec![SurfaceAnchorSample {
                source_face_id: 0,
                point_mm: [0.0, 0.0, bottom_z],
                delicacy: 0.0,
            }],
            mesh,
            topology,
            surface,
        }
    }

    fn rewire_fixture() -> (Forest, RewireFixtureKeys) {
        let mut forest = Forest::new();
        let old_parent = forest.add_member(rewire_member(
            [0.0, 0.0, 10.0],
            [0.0, 0.0, 0.0],
            MemberKind::Trunk,
            0,
        ));
        let root = forest.add_member(rewire_member(
            [0.0, 0.0, 15.0],
            [0.0, 0.0, 10.0],
            MemberKind::Shoot,
            1,
        ));
        forest.set_parent(root, Some(old_parent)).unwrap();
        let sibling = forest.add_member(rewire_member(
            [2.0, 0.0, 15.0],
            [0.0, 0.0, 10.0],
            MemberKind::Shoot,
            2,
        ));
        forest.set_parent(sibling, Some(old_parent)).unwrap();
        let child = forest.add_member(rewire_member(
            [0.0, 0.0, 18.0],
            [0.0, 0.0, 15.0],
            MemberKind::Shoot,
            3,
        ));
        forest.set_parent(child, Some(root)).unwrap();
        let eligible = forest.add_member(rewire_member(
            [4.0, 0.0, 20.0],
            [4.0, 0.0, 0.0],
            MemberKind::Trunk,
            4,
        ));
        let braced_host = forest.add_member(rewire_member(
            [8.0, 0.0, 20.0],
            [8.0, 0.0, 0.0],
            MemberKind::Trunk,
            5,
        ));
        let brace = forest.add_brace(rewire_member(
            [8.1, 0.0, 5.0],
            [12.0, 0.0, 5.0],
            MemberKind::Brace,
            6,
        ));
        let dead = forest.add_member(rewire_member(
            [40.0, 0.0, 10.0],
            [40.0, 0.0, 0.0],
            MemberKind::Trunk,
            7,
        ));
        forest.retire(dead).unwrap();
        forest.validate().unwrap();
        (
            forest,
            RewireFixtureKeys {
                root,
                old_parent,
                child,
                eligible,
                braced_host,
                brace,
                dead,
            },
        )
    }

    #[test]
    fn rewire_binding_rejects_stale_and_reused_root_generations() {
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();

        let mut changed = forest.clone();
        let root = changed.member(keys.root).unwrap().clone();
        changed
            .set_endpoints(
                keys.root,
                [Finite::new(0.25).unwrap(), root.p0_mm()[1], root.p0_mm()[2]],
                root.p1_mm(),
            )
            .unwrap();
        assert!(matches!(
            binding.validate_live(&changed),
            Err(CcoError::StaleRewireBinding(root)) if root == keys.root
        ));

        let mut reused = forest.clone();
        let replacement_member = reused.member(keys.root).unwrap().clone();
        reused.retire(keys.root).unwrap();
        let replacement = reused.add_member(replacement_member);
        assert_eq!(replacement.slot(), keys.root.slot());
        assert_ne!(replacement, keys.root);
        assert!(matches!(
            binding.validate_live(&reused),
            Err(CcoError::StaleRewireBinding(root)) if root == keys.root
        ));
    }

    #[test]
    fn rewire_binding_excludes_subtree_dead_braces_braced_hosts_and_old_parent() {
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        for excluded in [
            keys.root,
            keys.child,
            keys.old_parent,
            keys.braced_host,
            keys.brace,
            keys.dead,
        ] {
            assert!(binding.excluded_hosts().contains(&excluded), "{excluded:?}");
        }
        assert!(!binding.excluded_hosts().contains(&keys.eligible));

        let anchors = rewire_anchor_fixture();
        let batch = anchors.with_route(|route| {
            CcoController::default()
                .evaluate_rewire_candidates(&binding, route)
                .unwrap()
        });
        for candidate in &batch.candidates {
            if let Descriptor::Bifurcate { host } = candidate.descriptor {
                assert!(!batch.excluded_hosts.contains(&host));
            }
        }
    }

    #[test]
    fn root_reanchor_binding_reports_splice_mismatch_before_general_staleness() {
        let (mut forest, keys) = rewire_fixture();
        forest.set_parent(keys.root, None).unwrap();
        let binding = RewireCcoBinding::bind_root_reanchor(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        assert!(matches!(
            binding.splice(),
            RewireSpliceFacts::RootReanchor {
                retiring_root,
                own_children,
                fixed_point_mm,
            } if *retiring_root == keys.root
                && own_children == &vec![keys.child]
                && *fixed_point_mm == forest.member(keys.root).unwrap().p0_mm()
        ));
        let anchors = rewire_anchor_fixture();
        let batch = anchors.with_route(|route| {
            CcoController::default()
                .evaluate_rewire_candidates(&binding, route)
                .unwrap()
        });
        assert!(batch
            .candidates
            .iter()
            .map(|candidate| candidate.evidence.family)
            .chain(batch.refusals.iter().map(|refusal| refusal.family))
            .any(|family| family == CandidateFamily::ModelAnchor));

        forest.set_parent(keys.root, Some(keys.eligible)).unwrap();
        assert!(matches!(
            binding.validate_live(&forest),
            Err(CcoError::ReanchorBindingMismatch(root)) if root == keys.root
        ));
    }

    #[test]
    fn rewire_batch_is_indexed_stably_uses_one_clearance_batch_and_never_commits() {
        let (forest, keys) = rewire_fixture();
        let before = forest.digest();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let anchors = rewire_anchor_fixture();
        let batch = anchors.with_route(|route| {
            CcoController::default()
                .evaluate_rewire_candidates(&binding, route)
                .unwrap()
        });
        assert_eq!(batch.clearance_batches, 1);
        assert_eq!(batch.anchor_ray_batches, 1);
        assert_eq!(batch.anchor_foot_batches, 1);
        assert!(batch.refusals.iter().any(|refusal| {
            refusal.family == CandidateFamily::ModelAnchor
                && refusal.reason == RewireCandidateRefusalReason::Clearance
        }));
        assert_eq!(forest.digest(), before);
        assert_eq!(binding.live_digest, before);
        assert_eq!(batch.snapshot_digest(), binding.snapshot.digest());
        assert_ne!(batch.snapshot_digest(), before);
        assert_eq!(binding.snapshot.parent_of(keys.root), None);
        assert_eq!(
            binding
                .snapshot
                .member(keys.old_parent)
                .unwrap()
                .carried_mm2()
                .get(),
            0.0
        );
        assert!(batch
            .candidates
            .windows(2)
            .all(|pair| { pair[0].evidence.generation_index < pair[1].evidence.generation_index }));
        assert!(batch
            .candidates
            .iter()
            .all(|candidate| candidate.evaluated_digest != 0));

        let descriptor = Descriptor::Plate {
            xy: [0.0, 0.0],
            top_z: 10.0,
        };
        let tied = |generation_index| RewireEvaluatedCandidate {
            descriptor: descriptor.clone(),
            evidence: CcoCandidateEvidence {
                family: CandidateFamily::PlateDescent,
                generation_index,
                price_ml: 1.0,
                clearance_rows: 1,
            },
            evaluated_digest: before,
            validated_model_anchor: None,
        };
        let anchors = empty_far_anchor_fixture();
        let geometry =
            AnchorGeometryAuthority::new(&anchors.mesh, &anchors.topology, &anchors.surface)
                .unwrap();
        let tie_batch = RewireCandidateBatch {
            candidates: vec![tied(9), tied(3)],
            refusals: Vec::new(),
            descriptors_generated: 2,
            clearance_batches: 1,
            anchor_ray_batches: 1,
            anchor_foot_batches: 0,
            anchor_seat_refusals: Vec::new(),
            splice: binding.splice().clone(),
            excluded_hosts: binding.excluded_hosts().clone(),
            snapshot_digest: before,
            binding_identity: binding.identity(),
            geometry_fingerprint: geometry.fingerprint(),
        };
        assert_eq!(
            tie_batch.ranked_candidates()[0].evidence.generation_index,
            3
        );
    }

    #[test]
    fn rewire_rejects_new_members_between_shared_and_rewire_degenerate_thresholds() {
        let before = Forest::new();
        let mut after = before.clone();
        after.add_member(rewire_member(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 5.0e-5],
            MemberKind::Shoot,
            90,
        ));
        // This is above shared CCO's 1e-6 guard but below rewire.py's 1e-4 guard.
        assert!(has_rewire_degenerate_new_member(&before, &after));

        let mut printable = before.clone();
        printable.add_member(rewire_member(
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 2.0e-4],
            MemberKind::Shoot,
            91,
        ));
        assert!(!has_rewire_degenerate_new_member(&before, &printable));
    }

    #[test]
    fn all_refused_mixed_family_ledger_is_in_generation_order() {
        let mut refusals = vec![
            RewireCandidateRefusal {
                generation_index: 0,
                family: CandidateFamily::PlateDescent,
                reason: RewireCandidateRefusalReason::InfeasibleGeometry,
            },
            RewireCandidateRefusal {
                generation_index: 3,
                family: CandidateFamily::Bifurcation,
                reason: RewireCandidateRefusalReason::DegenerateNewMember,
            },
            RewireCandidateRefusal {
                generation_index: 1,
                family: CandidateFamily::ModelAnchor,
                reason: RewireCandidateRefusalReason::AnchorFoot(
                    AnchorFootRefusalReason::FootBallBreached,
                ),
            },
            RewireCandidateRefusal {
                generation_index: 2,
                family: CandidateFamily::Bifurcation,
                reason: RewireCandidateRefusalReason::InfeasibleGeometry,
            },
        ];
        sort_rewire_refusals(&mut refusals);
        assert_eq!(
            refusals
                .iter()
                .map(|refusal| (refusal.generation_index, refusal.family))
                .collect::<Vec<_>>(),
            vec![
                (0, CandidateFamily::PlateDescent),
                (1, CandidateFamily::ModelAnchor),
                (2, CandidateFamily::Bifurcation),
                (3, CandidateFamily::Bifurcation),
            ]
        );
    }

    fn with_rewire_batch<T>(
        binding: &RewireCcoBinding,
        anchors: &RewireAnchorFixture,
        use_batch: impl FnOnce(RewireCandidateBatch, &AnchorGeometryAuthority<'_>) -> T,
    ) -> T {
        anchors.with_route(|route| {
            let geometry = route.geometry;
            let batch = CcoController::default()
                .evaluate_rewire_candidates(binding, route)
                .unwrap();
            use_batch(batch, geometry)
        })
    }

    fn replay_family(
        live: &Forest,
        binding: &RewireCcoBinding,
        anchors: &RewireAnchorFixture,
        family: CandidateFamily,
    ) -> RewireReplay {
        with_rewire_batch(binding, anchors, |batch, geometry| {
            let candidate = batch
                .ranked_candidates()
                .into_iter()
                .find(|candidate| candidate.evidence().family == family)
                .unwrap_or_else(|| panic!("{family:?} survivor exists"));
            let generation_index = candidate.evidence().generation_index;
            let evaluated_digest = candidate.evaluated_digest;
            let replay = batch
                .replay_candidate(binding, live, generation_index, geometry)
                .unwrap();
            assert_eq!(replay.forest.digest(), evaluated_digest);
            replay
        })
    }

    #[test]
    fn detached_and_root_reanchor_replay_plate_and_bifurcation_bit_exactly() {
        let anchors = empty_far_anchor_fixture();
        let (forest, keys) = rewire_fixture();
        let detached = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let live_digest = forest.digest();
        for family in [CandidateFamily::PlateDescent, CandidateFamily::Bifurcation] {
            let replay = replay_family(&forest, &detached, &anchors, family);
            assert!(matches!(
                replay.splice,
                RewireSpliceFacts::DetachedSubtree { root, old_parent, .. }
                    if root == keys.root && old_parent == keys.old_parent
            ));
            assert_eq!(forest.digest(), live_digest);
        }

        let mut root_forest = forest.clone();
        root_forest.set_parent(keys.root, None).unwrap();
        let root_binding = RewireCcoBinding::bind_root_reanchor(
            &root_forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let replay = replay_family(
            &root_forest,
            &root_binding,
            &anchors,
            CandidateFamily::PlateDescent,
        );
        assert!(matches!(
            replay.splice,
            RewireSpliceFacts::RootReanchor { retiring_root, .. }
                if retiring_root == keys.root
        ));
    }

    #[test]
    fn detached_and_root_reanchor_model_anchor_replay_is_bit_exact() {
        let (forest, keys) = rewire_fixture();
        let detached = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let detached_anchors = near_anchor_fixture(9.75);
        let replay = replay_family(
            &forest,
            &detached,
            &detached_anchors,
            CandidateFamily::ModelAnchor,
        );
        assert!(matches!(
            replay.splice,
            RewireSpliceFacts::DetachedSubtree { root, old_parent, .. }
                if root == keys.root && old_parent == keys.old_parent
        ));
        assert!(replay.validated_model_anchor.is_some());

        let mut root_forest = forest.clone();
        root_forest.set_parent(keys.root, None).unwrap();
        let root_binding = RewireCcoBinding::bind_root_reanchor(
            &root_forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let root_anchors = near_anchor_fixture(14.75);
        let replay = replay_family(
            &root_forest,
            &root_binding,
            &root_anchors,
            CandidateFamily::ModelAnchor,
        );
        assert!(matches!(
            replay.splice,
            RewireSpliceFacts::RootReanchor { retiring_root, .. }
                if retiring_root == keys.root
        ));
        assert!(replay.validated_model_anchor.is_some());
    }

    #[test]
    fn replay_refuses_stale_cross_batch_geometry_alias_and_index_tampering() {
        let anchors = empty_far_anchor_fixture();
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let select = |batch: &RewireCandidateBatch| {
            batch
                .ranked_candidates()
                .first()
                .expect("survivor")
                .evidence()
                .generation_index
        };
        let before = forest.digest();

        let mut stale = forest.clone();
        stale
            .set_carried_mm2(keys.root, Finite::new(2.25).unwrap())
            .unwrap();
        with_rewire_batch(&binding, &anchors, |batch, geometry| {
            let generation_index = select(&batch);
            assert!(matches!(
                batch.replay_candidate(&binding, &stale, generation_index, geometry),
                Err(CcoError::StaleRewireBinding(root)) if root == keys.root
            ));
        });

        let cross_binding = RewireCcoBinding::bind_root_reanchor(
            &forest,
            keys.eligible,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        with_rewire_batch(&binding, &anchors, |batch, geometry| {
            let generation_index = select(&batch);
            assert!(matches!(
                batch.replay_candidate(&cross_binding, &forest, generation_index, geometry),
                Err(CcoError::RewireBatchBindingMismatch)
            ));
        });

        let alias_a = alias_triangle_fixture([[0.0, 0.0, 0.0], [1.0, 2.0, 0.0], [2.0, 1.0, 0.0]]);
        let alias_b = alias_triangle_fixture([[0.0, 1.0, 0.0], [1.0, 2.0, 0.0], [2.0, 0.0, 0.0]]);
        assert_eq!(
            alias_a.surface.geometry().centroids(),
            alias_b.surface.geometry().centroids()
        );
        assert_eq!(
            alias_a.surface.geometry().normals(),
            alias_b.surface.geometry().normals()
        );
        assert_eq!(
            alias_a.surface.geometry().face_bounds(),
            alias_b.surface.geometry().face_bounds()
        );
        assert_eq!(
            alias_a.surface.geometry().areas(),
            alias_b.surface.geometry().areas()
        );
        with_rewire_batch(&binding, &alias_a, |batch, geometry_a| {
            let generation_index = select(&batch);
            alias_b.with_route(|route_b| {
                assert_ne!(geometry_a.fingerprint(), route_b.geometry.fingerprint());
                assert!(matches!(
                    batch.replay_candidate(&binding, &forest, generation_index, route_b.geometry,),
                    Err(CcoError::RewireGeometryAuthorityMismatch)
                ));
            });
        });

        with_rewire_batch(&binding, &anchors, |mut batch, geometry| {
            let generation_index = select(&batch);
            batch
                .candidates
                .iter_mut()
                .find(|candidate| candidate.evidence.generation_index == generation_index)
                .unwrap()
                .evaluated_digest ^= 1;
            assert!(matches!(
                batch.replay_candidate(&binding, &forest, generation_index, geometry),
                Err(CcoError::RewireCandidateEvidenceMismatch)
            ));
        });

        with_rewire_batch(&binding, &anchors, |mut batch, geometry| {
            let generation_index = select(&batch);
            batch
                .candidates
                .iter_mut()
                .find(|candidate| candidate.evidence.generation_index == generation_index)
                .unwrap()
                .evidence
                .generation_index += 100;
            assert!(matches!(
                batch.replay_candidate(&binding, &forest, generation_index, geometry),
                Err(CcoError::RewireCandidateNotFound(index)) if index == generation_index
            ));
        });

        with_rewire_batch(&binding, &anchors, |mut batch, geometry| {
            let generation_index = select(&batch);
            batch
                .candidates
                .iter_mut()
                .find(|candidate| candidate.evidence.generation_index == generation_index)
                .unwrap()
                .evidence
                .price_ml = f64::from_bits(1);
            assert!(matches!(
                batch.replay_candidate(&binding, &forest, generation_index, geometry),
                Err(CcoError::RewireCandidateEvidenceMismatch)
            ));
        });
        assert_eq!(forest.digest(), before);
    }

    #[test]
    fn refused_model_anchor_has_no_replay_capability() {
        let anchors = rewire_anchor_fixture();
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        with_rewire_batch(&binding, &anchors, |batch, geometry| {
            let refusal = batch
                .refusals()
                .iter()
                .find(|refusal| refusal.family == CandidateFamily::ModelAnchor)
                .expect("model-anchor family evaluated");
            let generation_index = refusal.generation_index;
            assert!(matches!(
                batch.replay_candidate(&binding, &forest, generation_index, geometry),
                Err(CcoError::RewireCandidateNotFound(index)) if index == generation_index
            ));
        });
    }

    #[test]
    fn replay_payload_supports_outer_transaction_rollback_without_touching_live() {
        let anchors = empty_far_anchor_fixture();
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        let replay = replay_family(&forest, &binding, &anchors, CandidateFamily::PlateDescent);
        let (mut prepared, _, evidence, _) = replay.into_parts();
        let replayed_digest = prepared.digest();
        let checkpoint = prepared.checkpoint();
        prepared.add_member(rewire_member(
            [50.0, 0.0, 1.0],
            [50.0, 0.0, 0.0],
            MemberKind::Trunk,
            200,
        ));
        prepared.rollback(checkpoint).unwrap();
        assert_eq!(prepared.digest(), replayed_digest);
        assert_eq!(evidence.family, CandidateFamily::PlateDescent);
        assert_eq!(forest.digest(), binding.live_digest);
    }

    #[test]
    fn rewire_replay_thread_sweep_is_identical() {
        const CHILD: &str = "PLINTH_REWIRE_REPLAY_SWEEP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let anchors = empty_far_anchor_fixture();
            let (forest, keys) = rewire_fixture();
            let binding = RewireCcoBinding::bind_detached_subtree(
                &forest,
                keys.root,
                BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
            )
            .unwrap();
            let replay = replay_family(&forest, &binding, &anchors, CandidateFamily::Bifurcation);
            println!(
                "REWIRE_REPLAY_SWEEP={:016x}:{}:{:016x}",
                replay.forest.digest(),
                replay.evidence.generation_index,
                replay.evidence.price_ml.to_bits()
            );
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let mut results = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("cco::tests::rewire_replay_thread_sweep_is_identical")
                .arg("--nocapture")
                .env(CHILD, "1")
                .env("PLINTH_THREADS", threads.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            results.push(
                stdout
                    .lines()
                    .find_map(|line| line.strip_prefix("REWIRE_REPLAY_SWEEP="))
                    .expect("child emitted replay result")
                    .to_owned(),
            );
        }
        assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn rewire_batch_thread_sweep_is_identical() {
        const CHILD: &str = "PLINTH_REWIRE_CCO_SWEEP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let (forest, keys) = rewire_fixture();
            let binding = RewireCcoBinding::bind_detached_subtree(
                &forest,
                keys.root,
                BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
            )
            .unwrap();
            let anchors = rewire_anchor_fixture();
            let batch = anchors.with_route(|route| {
                CcoController::default()
                    .evaluate_rewire_candidates(&binding, route)
                    .unwrap()
            });
            let rows = batch
                .candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "{:?}:{}:{:016x}:{}:{:016x}",
                        candidate.evidence.family,
                        candidate.evidence.generation_index,
                        candidate.evidence.price_ml.to_bits(),
                        candidate.evidence.clearance_rows,
                        candidate.evaluated_digest,
                    )
                })
                .collect::<Vec<_>>()
                .join("|");
            println!("REWIRE_CCO_SWEEP={rows}");
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let mut results = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("cco::tests::rewire_batch_thread_sweep_is_identical")
                .arg("--nocapture")
                .env(CHILD, "1")
                .env("PLINTH_THREADS", threads.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            results.push(
                stdout
                    .lines()
                    .find_map(|line| line.strip_prefix("REWIRE_CCO_SWEEP="))
                    .expect("child emitted rewire result")
                    .to_owned(),
            );
        }
        assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn candidate_source_ledger_vectors() {
        let (mut forest, request, _) = growth_fixture();
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let host = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(1.0), finite(0.0), finite(20.0)],
            p1_mm: [finite(1.0), finite(0.0), finite(0.0)],
            r0_mm: TerminalEndRadius::new(positive(0.45)),
            r1_mm: RootwardEndRadius::new(positive(0.8)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "source-ledger-host".to_owned(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: finite(4.0),
        }));
        let unrelated = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(50.0), finite(0.0), finite(30.0)],
            p1_mm: [finite(50.0), finite(0.0), finite(0.0)],
            r0_mm: TerminalEndRadius::new(positive(0.05)),
            r1_mm: RootwardEndRadius::new(positive(0.05)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "unrelated-undersized-parity-guard".to_owned(),
                ordinal: 0,
            },
            embed_end: None,
            carried_mm2: finite(0.0),
        }));
        let unrelated_before = forest.member(unrelated).unwrap().clone();
        let member_bits = |member: &Member| {
            let mut bits = member
                .p0_mm()
                .into_iter()
                .chain(member.p1_mm())
                .map(|value| value.get().to_bits())
                .collect::<Vec<_>>();
            bits.push(member.terminal_radius_mm().get().get().to_bits());
            bits.push(member.rootward_radius_mm().get().get().to_bits());
            bits.push(member.carried_mm2().get().to_bits());
            bits
        };
        let unrelated_before_bits = member_bits(&unrelated_before);
        let config = Config::current().unwrap();
        let descriptors = [
            Descriptor::Plate {
                xy: [0.0, 0.0],
                top_z: 12.5,
            },
            Descriptor::ModelAnchor {
                seat_index: 0,
                point_mm: [3.0, 0.0, 5.0],
            },
            Descriptor::Bifurcate { host },
        ];
        let candidates = descriptors
            .iter()
            .enumerate()
            .map(|(index, descriptor)| {
                evaluate(&forest, &request, descriptor, index, &config)
                    .unwrap()
                    .unwrap_or_else(|| {
                        panic!("frozen {:?} candidate is feasible", descriptor.family())
                    })
            })
            .collect::<Vec<_>>();
        // Frozen from the live `cco.py` family equations and generation ledger:
        // plate candidates precede model anchors, which precede bifurcations;
        // price wins and generation index is the stable equal-price tiebreak.
        let python_prices = [
            0.025_132_741_228_718_35,
            0.003_796_009_366_922_39,
            0.010_013_605_074_968_252,
        ];
        for (candidate, python_price) in candidates.iter().zip(python_prices) {
            assert!((candidate.price_ml - python_price).abs() <= 5e-18);
            assert_eq!(candidate.forest.member(unrelated), Some(&unrelated_before));
            assert_eq!(
                candidate
                    .forest
                    .member(unrelated)
                    .map(&member_bits)
                    .unwrap(),
                unrelated_before_bits
            );
            let unrelated_p0 = unrelated_before.p0_mm().map(Finite::get);
            let unrelated_p1 = unrelated_before.p1_mm().map(Finite::get);
            assert!(!candidate
                .rows
                .iter()
                .any(|row| row.p0 == unrelated_p0 && row.p1 == unrelated_p1));
        }
        let mut by_price = candidates.iter().collect::<Vec<_>>();
        by_price.sort_by(|left, right| compare_candidates(left, right));
        assert_eq!(
            by_price.iter().map(|c| c.family).collect::<Vec<_>>(),
            vec![
                CandidateFamily::ModelAnchor,
                CandidateFamily::Bifurcation,
                CandidateFamily::PlateDescent,
            ]
        );

        let later = evaluate(&forest, &request, &descriptors[0], 9, &config)
            .unwrap()
            .unwrap();
        let earlier = evaluate(&forest, &request, &descriptors[0], 3, &config)
            .unwrap()
            .unwrap();
        assert_eq!(later.price_ml, earlier.price_ml);
        assert_eq!(
            compare_candidates(&earlier, &later),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn terminal_growth_commits_only_the_batched_winner() {
        let (mut forest, request, surface) = growth_fixture();
        let before = forest.digest();
        let outcome = CcoController::default()
            .grow_terminal_for_test(&mut forest, &request, &surface)
            .unwrap();
        let winner = outcome.winner.expect("plate descent survives");
        assert_eq!(winner.family, CandidateFamily::PlateDescent);
        assert_eq!(outcome.clearance_batches, 1);
        assert!(outcome.candidates_generated > 1);
        assert!(outcome.candidates_feasible > 0);
        assert_ne!(forest.digest(), before);
        forest.validate().unwrap();
    }

    #[test]
    fn winner_commit_remains_visible_to_an_outer_forest_rollback() {
        let (mut forest, request, surface) = growth_fixture();
        let before = forest.digest();
        let outer = forest.checkpoint();
        CcoController::default()
            .grow_terminal_for_test(&mut forest, &request, &surface)
            .unwrap();
        assert_ne!(forest.digest(), before);
        forest.rollback(outer).unwrap();
        assert_eq!(forest.digest(), before);
    }

    #[test]
    fn thread_sweep_is_digest_identical() {
        const CHILD: &str = "PLINTH_CCO_SWEEP_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let (mut forest, request, surface) = growth_fixture();
            let mut controller = CcoController::default();
            let outcome = controller
                .grow_terminal_for_test(&mut forest, &request, &surface)
                .unwrap();
            println!(
                "CCO_SWEEP_DIGEST={:016x}:{:016x}:{}:{}",
                forest.digest(),
                controller.digest(),
                outcome.winner.unwrap().generation_index,
                outcome.candidates_generated
            );
            return;
        }
        let executable = std::env::current_exe().unwrap();
        let mut digests = Vec::new();
        for threads in [1, 4, 16] {
            let output = std::process::Command::new(&executable)
                .arg("--exact")
                .arg("cco::tests::thread_sweep_is_digest_identical")
                .arg("--nocapture")
                .env(CHILD, "1")
                .env("PLINTH_THREADS", threads.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            let digest = stdout
                .lines()
                .find_map(|line| line.strip_prefix("CCO_SWEEP_DIGEST="))
                .expect("child emitted digest")
                .to_owned();
            digests.push(digest);
        }
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
