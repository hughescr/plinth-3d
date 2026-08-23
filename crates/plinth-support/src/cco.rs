//! Current pre-P8 CCO terminal growth.
//!
//! Candidate descriptions are generated serially from one forest snapshot,
//! evaluated independently, flattened into one native clearance batch, and
//! scattered back to their original indices.  Only the deterministic winner
//! is committed to the live forest.
use std::collections::{BTreeMap, BTreeSet};

use geom_kernel::NativeSurface;
use plinth_core::{m0_constants, Finite, PositiveFinite};
use plinth_mesh::FaceTopology;
use rayon::prelude::*;
use thiserror::Error;

use crate::forest::{
    ContactEnd, Forest, ForestError, Member, MemberKey, MemberKind, MemberSpec, Owner,
    RootwardEndRadius, TerminalEndRadius, TerminalLedgerEntry, TerminalLedgerState,
};
use crate::legacy_compat::{LegacyCompatibility, LegacyCompatibilityError};
use crate::transient::{derive_brace_attachments, TransientError};
use crate::{
    anchor::{
        discover_finalized_anchor_seats, discover_rewire_anchor_seats,
        validate_finalized_anchor_feet, validate_rewire_anchor_feet, AnchorCandidateProvenance,
        AnchorFootOutcome, AnchorFootRefusal, AnchorFootRefusalReason, AnchorFootRequest,
        AnchorGeometryAuthority, AnchorSearchConfig, AnchorSearchError, AnchorSeatRefusal,
        AnchorTerminalBinding, BoundPhysicalAnchorContact, GeometryFingerprint,
        QualifiedAnchorSeat, RewireAnchorBinding, SurfaceAnchorPool, SurfaceAnchorSample,
        ValidatedModelAnchor,
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
    /// F3 — the run's own process resolution, for the seat attestation's
    /// claimed extent and its OD-9 probe offset. It arrives on the route rather
    /// than from the registry because a layer height is a property of THIS RUN,
    /// exactly as `brace_pass_current_parity` already takes it.
    pub(crate) process_resolution_mm: f64,
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
        let existing_carried_mm2 = member.carried_mm2().get();
        let removed_mm2 = removed_carried_mm2.get();
        let carried =
            Finite::new((existing_carried_mm2 - removed_mm2).max(0.0)).map_err(|error| {
                CcoError::InvalidGeometry(format!(
                    "slim_rewire_ancestors: reducing carried area at member {key:?} \
                     (existing {existing_carried_mm2} mm2 minus removed {removed_mm2} mm2) \
                     produced a non-finite value: {error}"
                ))
            })?;
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
        let own_minimum_mm = own_floor.get();
        let radius = PositiveFinite::new(murray.max(own_minimum_mm)).map_err(|error| {
            CcoError::InvalidGeometry(format!(
                "slim_rewire_ancestors: Murray's-law radius blend at member {key:?} is not \
                 positive-finite (murray-law radius {murray} mm, minimum radius \
                 {own_minimum_mm} mm): {error}"
            ))
        })?;
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
    if member.model_contact_end() != Some(ContactEnd::Terminalward) {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
    /// **PHYSICS, and it keeps exactly one meaning.** How many arms passed
    /// feasibility. Craig's authorization item 5 (2026-08-23) is that this
    /// counter is NOT overloaded with ruled-out arms — the refutation §F.1
    /// reads this exact field to make its "arms existed, clearance refused
    /// them" argument, and an overload would need a footnote nobody will have.
    pub(crate) candidates_feasible: usize,
    /// **The second counter.** How many arms were ruled out before the
    /// tournament weighed them.
    pub(crate) candidates_ruled_out: usize,
    /// **Set only when a confirmation round has driven this terminal past its
    /// last arm.** The round's measured rows, carried here so
    /// `no_winner_evidence` can seal the round that earned the retirement
    /// rather than a count of refusals.
    pub(crate) confirmation_round: Option<Vec<crate::legalization::RoundArm>>,
    pub(crate) anchor_ray_batches: usize,
    pub(crate) anchor_foot_batches: usize,
    pub(crate) clearance_batches: usize,
    pub(crate) validated_model_anchor: Option<Box<ValidatedModelAnchor>>,
    pub(crate) anchor_seat_refusals: Vec<AnchorSeatRefusal>,
    pub(crate) anchor_refusals: Vec<AnchorFootRefusal>,
    /// PR-8/OD-9 (F3) — this terminal's model-anchor seat-attestation
    /// transcript, attested rows and refused rows alike.
    ///
    /// Deliberately NOT folded into the sealed CCO transcript: the transcript
    /// already records what the attestation DID (a refused anchor appears as an
    /// `anchor_refusals` row), while this records that it RAN and WHAT IT
    /// MEASURED. Those two regress independently, which is the same argument
    /// that keeps the stub family's transcript out of the forest digest.
    pub(crate) contact_attestation_digest: u64,
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
    /// **The retirement a confirmation round earned — design §1.3 verifier
    /// addition 1, and the only kind of retirement Stage B can produce.**
    ///
    /// Every remaining arm of this terminal was driven, one per pass, in the
    /// tournament's own order, under a steering set held frozen for the round's
    /// duration; every one of them was measured illegal by the exact
    /// post-brace check; and the last of them was measured in the pass before
    /// this one. `round_arms` carries, per arm, the MEASURED clearance margin
    /// in millimetres and the radii the resize actually assigned.
    ///
    /// **What this lets a reader check is not "here are k negative numbers".**
    /// Arithmetic over a stale universe adds up perfectly, and so does a pile
    /// of true measurements harvested from k different worlds. The ranks are
    /// contiguous from zero and they are the tournament's own order, so what
    /// the seal shows is a ROUND: nothing this terminal could have chosen was
    /// legal, in one world, measured.
    ///
    /// Margins from earlier STEERING passes are deliberately not here. §1.3
    /// requires them to be labelled as steering history rather than as part of
    /// the round, and the honest place for that is the run log, which prints
    /// every one of them as it happens.
    ConfirmationRoundExhausted {
        candidates_generated: usize,
        candidates_feasible: usize,
        round_arms: Vec<crate::legalization::RoundArm>,
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
            }
            | Self::ConfirmationRoundExhausted {
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
    /// **The second sealed counter — Craig's authorization item 5, 2026-08-23.**
    ///
    /// A ruled-out arm has to be counted somewhere. The cheap answer was to
    /// fold it into `candidates_feasible`, which keeps this fingerprint still
    /// for models that never rule anything out but leaves one number with two
    /// meanings: *"how many arms passed feasibility"* would silently become
    /// *"how many passed feasibility and were not ruled out in an earlier
    /// pass"*, and the refutation §F.1 reads that exact field to make its
    /// "arms existed, clearance refused them" argument.
    ///
    /// Craig ruled for the honest counter. `candidates_feasible` keeps its
    /// meaning word for word, and ruled-out arms get their own number.
    ///
    /// **The price, paid once, on purpose.** This field is folded into
    /// [`transcript_rows_digest`] UNCONDITIONALLY, so the sealed CCO
    /// transcript fingerprint moves for EVERY model, once, at the commit that
    /// introduces it — not only for models that rule something out. It is
    /// ledgered in `docs/engineering-journal/EMISSION-CANARY-LEDGER.md`.
    ///
    /// It is zero on every path today. Nothing rules an arm out yet; the
    /// steering channel that fills it is Stage B's. The counter lands with
    /// Stage A so the one-time re-freeze is paid at the small, revert-proof
    /// stage rather than inside the stage that changes behaviour.
    candidates_ruled_out: usize,
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

    /// How many of this terminal's arms were ruled out before the tournament
    /// weighed them — the second counter, kept separate from
    /// `candidates_feasible` so feasibility keeps one meaning.
    #[must_use]
    #[allow(
        dead_code,
        reason = "the counter is sealed and digested at Stage A; the steering channel that gives \
                  it a non-zero value, and the readers that report it, land with Stage B \
                  (docs/engineering-journal/designs/seal-semantics-2026-08-22.md §4 Stage B)"
    )]
    pub(crate) const fn candidates_ruled_out(&self) -> usize {
        self.candidates_ruled_out
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
    /// **Stage B.** Every arm this terminal could still take at its current
    /// context carries a steering no-good.
    ///
    /// This is NOT a retirement and it must never become one. §1.3: a no-good
    /// steers and never retires, because each one was measured in a world that
    /// has since been replaced. Returning `winner: None` here would hand the
    /// cadence a terminal with no winner, which
    /// `grow_terminal_with_finalized_anchor` retires `UnroutedNoWinner` on the
    /// spot — a real contact dropped, silently, under a clean seal, on a pile
    /// of arithmetic. So the pass stops instead, and the controller loop opens
    /// a confirmation round for this terminal: every remaining arm re-measured
    /// under the frozen set, one per pass, in the tournament's own order.
    #[error(
        "terminal {terminal_id} has a steering no-good for every arm it can still take; the \
         pass stops so a confirmation round can re-measure them"
    )]
    SteeringSpent { terminal_id: u32 },
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
    /// FU-truthful-errors: this variant used to be a bare unit value, so
    /// every one of its 33 construction sites (15 of them `.map_err(|_| ..)`,
    /// throwing the real underlying error away) collapsed to the same
    /// message no matter which check failed or what number failed it. The
    /// study that measured the seven large-mini failures found this was the
    /// error hiding behind every one: never actually "not finite and
    /// positive", always something specific (`ExtentOutOfSpan`,
    /// `CorruptKernelResult`, an out-of-range index, a non-finite computed
    /// price or volume). The `String` names the place, the check, and the
    /// measured value or source error, the same way `BraceError::NativeQuery`
    /// already does.
    #[error("CCO input or derived geometry is not finite and positive where required: {0}")]
    InvalidGeometry(String),
    #[error(transparent)]
    Forest(#[from] ForestError),
    #[error(transparent)]
    Sizing(#[from] LegacyCompatibilityError),
    /// A batched native geometry call this stage issued did not return a
    /// usable result.
    ///
    /// The message used to say "clearance batch" because clearance was the only
    /// caller. It is not: seat attestation routes here too (`brace.rs` already
    /// routes the identical error to `BraceError::NativeQuery`), and naming
    /// every native batch after one of them made the seat-attestation site the
    /// outlier that reached for `InvalidGeometry` instead. The `String` names
    /// which batch and which check, so nothing is lost by widening the variant.
    #[error("native geometry batch failed: {0}")]
    Native(String),
    #[error("serial winner re-application did not reproduce its evaluated forest")]
    NonDeterministicWinner,
    #[error(transparent)]
    Anchor(#[from] AnchorSearchError),
    #[error(transparent)]
    Transient(#[from] TransientError),
    /// #36. A frozen seat zone could not be computed. A BATCH-level failure —
    /// a starved band admission or a mesh/topology disagreement — and never a
    /// statement about one candidate, so it fails the terminal rather than
    /// silently dropping an anchor from the tournament.
    #[error("CCO seat-zone freeze failed: {0}")]
    SeatZone(String),
}

#[derive(Clone, Debug)]
enum Descriptor {
    Plate {
        xy: [f64; 2],
        top_z: f64,
    },
    ModelAnchor {
        seat_index: usize,
        /// The RAW qualified seat point, on the surface. This stays raw because
        /// it is the anchor's IDENTITY: `RewireCandidateBatch::replay` compares
        /// it bit-for-bit against `ValidatedModelAnchor::seat().point_mm()`.
        point_mm: [f64; 3],
        /// **PR-7.** The point the strut's rootward end is actually built at —
        /// `point_mm` pushed inward along the seat's snapped face normal by the
        /// PR-1 seat depth. Derived once per seat in [`generate_descriptors`]
        /// and frozen here so a rewire replay re-evaluates the same geometry
        /// without re-deriving it from the run's process resolution.
        seated_point_mm: [f64; 3],
        /// **#36 creation site 3 of 4 — the FROZEN seat zone `Z`.**
        ///
        /// The whole face set this anchor's own seat forgives, computed once
        /// per seat in [`generate_descriptors`] and carried here for the same
        /// reason `seated_point_mm` is: `apply_model_anchor` runs TWICE for a
        /// winner — once inside the candidate tournament and once replayed
        /// against the live forest — and the two must install the identical
        /// zone or the candidacy screen and the committed geometry disagree.
        /// Carrying the finished zone rather than the face ID is what makes
        /// that identity structural: the replay does an `Arc` clone, never a
        /// native call and never a recompute.
        seat_zone: std::sync::Arc<[u32]>,
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
    /// PR-8/OD-9 (F3) — this batch's model-anchor seat-attestation transcript.
    contact_attestation_digest: u64,
    /// The run's own process resolution, frozen with the batch so
    /// `replay_candidate` screens under the SAME PR-1/PR-2/PR-3 policy the
    /// batch was evaluated under. Registry constants are the same on both
    /// sides; a layer height is not a registry constant.
    process_resolution_mm: f64,
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

// No longer `Copy`: #36's frozen seat zone is an `Arc`, and sharing one is the
// point — the tournament, the mint and the replay must all name the SAME face
// set rather than three copies that could drift.
#[derive(Clone, Debug)]
struct ClearanceRow {
    p0: [f64; 3],
    p1: [f64; 3],
    r0: f64,
    r1: f64,
    /// The end that declares a model contact, if any, plus the seating state
    /// its design family gives it.
    ///
    /// **#51/#36.** This row used to carry a FLAT inset pair computed here —
    /// `contact_depth_mm` terminalward, `standoff_mm` rootward — which made the
    /// model-anchor clearance test unsatisfiable by exactly the arm's own
    /// radius. The inset is now the PR-2 mask, and PR-2 needs the surface
    /// normal at the contact point, which needs the kernel. So the row records
    /// WHAT it is and the mask is computed in [`clearance_survival`], where one
    /// batched `NativeSurface::project` covers the whole candidate batch. No
    /// per-item kernel call is introduced; the projection count per tournament
    /// goes from zero to one.
    contact: Option<ClearanceContact>,
    /// **#36 step 5 — the frozen seat zone `Z` for this row's contact.**
    ///
    /// Read out of the CANDIDATE forest by [`clearance_rows`], which runs after
    /// `apply_and_size` has minted or inherited the zone, so the tournament
    /// grades against the same face set the committed replay will install.
    ///
    /// `None` for a row with no contact — it forgives nothing — and also for a
    /// contact-declaring row whose zone is somehow missing, which grades it
    /// against its own designed embed and refuses it. That is the STRICT
    /// direction: a missed creation site surfaces as a refusal, never a silent
    /// pass.
    seat_zone: Option<std::sync::Arc<[u32]>>,
}

/// The contact facts a [`ClearanceRow`] needs to carry from candidate
/// construction (inside the `rayon` map, no surface available) to the batched
/// mask computation.
#[derive(Clone, Copy, Debug)]
struct ClearanceContact {
    end: ContactEnd,
    /// Recomputed from the design family per OD-3, never stored on `Member`.
    seating: crate::contact::ContactSeating,
    /// The member's radius AT the contact end — PR-1's flank term.
    end_radius_mm: f64,
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

/// The four immutable routing authorities, hashed **once** for a whole CCO
/// routing stage, with the subjects travelling inside the proof.
///
/// # FU-32 phase J1 — why this type exists
///
/// `plan`, `loads`, `pool` and `geometry` are behind shared references for the
/// entire routing loop (`SupportRoutingRun::route_initial_cco`) and none of
/// their types contains any interior mutability — a repo-wide grep for
/// `Cell`/`RefCell`/`Mutex`/`RwLock`/`Atomic`/`thread_local` across
/// `specialized.rs`, `perception.rs`, `anchor.rs` and this file finds exactly
/// one hit, a `#[cfg(test)]` thread-local at `specialized.rs:5334`
/// (re-verified 2026-08-09).  **The borrow checker already proves what the
/// per-terminal re-hashes were re-checking.**
///
/// What that cost, measured on the Weeping Treant v1.2 (FU-32 design §M2):
/// `SpecializedContactPlan::digest` 2,487,000 words × **4 calls per grown
/// terminal**, `surface_anchor_pool_digest` 2,250,235 words × **~3.9**,
/// `finalized_load_rows_fingerprint` 70,737 rows × **2** — ≈153 MB of hashing
/// per terminal, ≈66 GB over a full cadence.  On `Cylinder`, a 244-triangle
/// corpus model, it was ≈95 % of a 5,736 ms routing stage.
///
/// # What did NOT change, and must not
///
/// This is the `VerifiedMeshFingerprint` (FU-8f phase G2) precedent applied
/// verbatim, with both of its properties:
///
/// 1. **The subjects travel inside the proof.**  A holder of this token can
///    reach the plan, loads, pool and geometry *only* through it, so a digest
///    computed from one object can never be presented alongside another — by
///    construction rather than by discipline.  That is why
///    [`FinalizedAnchorTerminalRoute`] carries the token instead of four
///    independent references.
/// 2. **The staleness comparison stays at every site.**  This type hoists the
///    *hash*, not the *check*.  `bind`, `validate_replay`,
///    `validate_finalized_anchor_route`,
///    [`crate::anchor::SurfaceAnchorPool::validate_against_hoisted`],
///    [`crate::anchor::discover_finalized_anchor_seats`],
///    [`crate::anchor::validate_finalized_anchor_feet`] and
///    `FinalizedContactLoads::validate_binding_hoisted` all still compare, and
///    still refuse.  "Verify once, thread the proof — never stop verifying."
///
/// Each hoisted value is compared at its site against the subject's *own*
/// O(1) sealed field — `plan.finalized_load_plan_digest()`, `loads.digest()`,
/// `pool.digest()`, `geometry.fingerprint_ref()`, `loads.rows_fingerprint` —
/// which is an independently computed witness, not a copy of the hoist.  A
/// corrupted or foreign token therefore still fails a real comparison; that
/// claim is pinned arm by arm by the `fu32_*` tests at the bottom of this
/// module.
///
/// **Value-preserving.**  The digests recorded in every `TerminalRouteEvidence`
/// are the same `u64`s computed by the same functions from the same
/// structures — just once.
pub(crate) struct VerifiedRoutingAuthority<'a, 'surface> {
    plan: &'a SpecializedContactPlan,
    loads: &'a FinalizedContactLoads,
    pool: &'a SurfaceAnchorPool<'surface>,
    geometry: &'a AnchorGeometryAuthority<'surface>,
    plan_digest: u64,
    loads_digest: u64,
    pool_digest: u64,
    load_rows_fingerprint: u64,
    geometry_fingerprint: GeometryFingerprint,
    /// The plan's canonical contact-id vector, built once instead of once per
    /// `validate_finalized_anchor_route` call.
    contact_ids: Vec<u64>,
    /// Kills the two O(|contacts|) linear `find`s per terminal — one in the
    /// runner's cadence loop, one in `validate_finalized_anchor_route`.
    contact_by_id: BTreeMap<u64, usize>,
}

impl<'a, 'surface> VerifiedRoutingAuthority<'a, 'surface> {
    /// Hash the four immutable authorities once and bind the digests to them.
    ///
    /// This performs **exactly** the validation the per-terminal path used to
    /// perform — `pool.validate_against(geometry)` with its full content walk,
    /// `loads.validate_binding(plan.load_partition(), contact_ids)` with its
    /// full row walk, and the plan-seal agreement `plan.digest() ==
    /// plan.finalized_load_plan_digest()` that `validate_initial_cco_preflight`
    /// already requires — once, before the first terminal.
    pub(crate) fn compute(
        plan: &'a SpecializedContactPlan,
        loads: &'a FinalizedContactLoads,
        pool: &'a SurfaceAnchorPool<'surface>,
        geometry: &'a AnchorGeometryAuthority<'surface>,
    ) -> Result<Self, CcoError> {
        pool.validate_against(geometry)?;
        let contact_ids = plan
            .contacts()
            .iter()
            .map(SpecializedPhysicalContact::contact_id)
            .collect::<Vec<_>>();
        let mut contact_by_id = BTreeMap::new();
        for (index, &contact_id) in contact_ids.iter().enumerate() {
            if contact_by_id.insert(contact_id, index).is_some() {
                return Err(CcoError::FinalizedContactMismatch);
            }
        }
        let load_rows_fingerprint =
            FinalizedContactLoads::hoist_rows_fingerprint(plan.load_partition());
        if loads
            .validate_binding_hoisted(plan.load_partition(), &contact_ids, load_rows_fingerprint)
            .is_err()
        {
            return Err(CcoError::FinalizedLoadBindingMismatch);
        }
        let plan_digest = plan.digest();
        // The one place the plan's full walk still runs: the seal this token
        // will be compared against at every site must be proved equal to the
        // real hash exactly once, here, or the hoist would attest nothing.
        if plan_digest != plan.finalized_load_plan_digest() {
            return Err(CcoError::StaleFinalizedAnchorAuthority);
        }
        Ok(Self {
            plan,
            loads,
            pool,
            geometry,
            plan_digest,
            loads_digest: loads.digest(),
            pool_digest: pool.digest(),
            load_rows_fingerprint,
            geometry_fingerprint: geometry.fingerprint(),
            contact_ids,
            contact_by_id,
        })
    }

    pub(crate) const fn plan(&self) -> &'a SpecializedContactPlan {
        self.plan
    }

    pub(crate) const fn loads(&self) -> &'a FinalizedContactLoads {
        self.loads
    }

    pub(crate) const fn pool(&self) -> &'a SurfaceAnchorPool<'surface> {
        self.pool
    }

    pub(crate) const fn geometry(&self) -> &'a AnchorGeometryAuthority<'surface> {
        self.geometry
    }

    pub(crate) const fn plan_digest(&self) -> u64 {
        self.plan_digest
    }

    pub(crate) const fn loads_digest(&self) -> u64 {
        self.loads_digest
    }

    pub(crate) const fn pool_digest(&self) -> u64 {
        self.pool_digest
    }

    pub(crate) const fn load_rows_fingerprint(&self) -> u64 {
        self.load_rows_fingerprint
    }

    pub(crate) const fn geometry_fingerprint(&self) -> &GeometryFingerprint {
        &self.geometry_fingerprint
    }

    pub(crate) fn contact_ids(&self) -> &[u64] {
        &self.contact_ids
    }

    /// The plan contact for a stable physical-contact id, by index rather than
    /// by an O(|contacts|) scan.
    pub(crate) fn contact(&self, contact_id: u64) -> Option<&'a SpecializedPhysicalContact> {
        self.contact_by_id
            .get(&contact_id)
            .and_then(|&index| self.plan.contacts().get(index))
    }

    /// FU-32 J1 mutation hook: corrupt one hoisted digest without touching the
    /// subject it claims to attest.  Every site's comparison must catch it.
    #[cfg(test)]
    pub(crate) fn corrupt_for_test(&mut self, field: CorruptedAuthorityField) {
        match field {
            CorruptedAuthorityField::PlanDigest => self.plan_digest ^= 1,
            CorruptedAuthorityField::LoadsDigest => self.loads_digest ^= 1,
            CorruptedAuthorityField::PoolDigest => self.pool_digest ^= 1,
            CorruptedAuthorityField::LoadRowsFingerprint => self.load_rows_fingerprint ^= 1,
            CorruptedAuthorityField::GeometryFingerprint => {
                self.geometry_fingerprint.corrupt_mesh_sha256_for_test();
            }
        }
    }
}

/// Which hoisted value a `corrupt_for_test` call moves.  FU-32 J1.
#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) enum CorruptedAuthorityField {
    PlanDigest,
    LoadsDigest,
    PoolDigest,
    LoadRowsFingerprint,
    GeometryFingerprint,
}

/// The only production terminal-anchor route.  It starts from the finalized
/// physical-contact authority and the canonical exterior pool; callers never
/// hand CCO a selected-contact projection or an arbitrary seat collection.
///
/// FU-32 J1: the four immutable authorities now travel together inside
/// [`VerifiedRoutingAuthority`] rather than as four independent references, so
/// the hoisted digests cannot be paired with a different plan, pool, load
/// artifact or geometry than the one they were computed from.
pub(crate) struct FinalizedAnchorTerminalRoute<'a, 'surface> {
    pub(crate) authority: &'a VerifiedRoutingAuthority<'a, 'surface>,
    pub(crate) contact: &'a SpecializedPhysicalContact,
    pub(crate) contact_id: u64,
    pub(crate) config: AnchorSearchConfig,
    /// F3 — the run's own process resolution, for the seat attestation's
    /// claimed extent and its OD-9 probe offset. See [`RewireAnchorRoute`].
    pub(crate) process_resolution_mm: f64,
}

impl<'a, 'surface> FinalizedAnchorTerminalRoute<'a, 'surface> {
    pub(crate) const fn plan(&self) -> &'a SpecializedContactPlan {
        self.authority.plan()
    }

    pub(crate) const fn loads(&self) -> &'a FinalizedContactLoads {
        self.authority.loads()
    }

    pub(crate) const fn pool(&self) -> &'a SurfaceAnchorPool<'surface> {
        self.authority.pool()
    }

    pub(crate) const fn geometry(&self) -> &'a AnchorGeometryAuthority<'surface> {
        self.authority.geometry()
    }
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

/// FU-32 J5 — what the routing cadence ACTUALLY spent, counted by the walks
/// that spent it (design D3, the G6c charge-beside-meter idiom).
///
/// The charge lives at the preflight (`runner::RouteCcoWorkCharge`, checked
/// against `SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS` and
/// `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS` before the first terminal); this
/// is the meter beside it, and `runner::route_initial_cco` holds the receipt
/// against the charge once the cadence ends. A meter is never a substitute for
/// a charge: an over-spend refuses at `SupportRoutingError::ChargeDrift`
/// rather than being reported after the fact.
///
/// Both fields are `saturating_add`, so an impossible overflow surfaces as a
/// drift refusal instead of a panic or a wrap to a small number.
///
/// **Result-inert and deliberately NOT hashed.** `CcoController::digest`
/// enumerates the fields it folds, and this is not one of them: it is
/// execution accounting, it never alters an admitted plan, and folding it
/// would put a resource meter inside a transcript identity. The same argument
/// keeps every other cap out of every semantic digest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RouteCcoWorkMeter {
    pub(crate) seat_evaluations: usize,
    pub(crate) seat_refusal_rows: usize,
}

impl RouteCcoWorkMeter {
    /// Fold one terminal's counted discovery into the run total.
    fn record_terminal(&mut self, seat_evaluations: usize, seat_refusal_rows: usize) {
        self.seat_evaluations = self.seat_evaluations.saturating_add(seat_evaluations);
        self.seat_refusal_rows = self.seat_refusal_rows.saturating_add(seat_refusal_rows);
    }
}

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
    /// PR-8/OD-9 (F3) — the model-anchor seat-attestation CHAIN over every
    /// terminal this controller grew, in cadence order.
    ///
    /// Chained rather than accumulated into a set, so a terminal that stopped
    /// attesting moves the head even when a later terminal attests identical
    /// rows. Read once by the runner and folded into the run's contact
    /// attestation chain alongside the stub-stage head.
    contact_attestation_digest: u64,
    /// FU-32 J2 (§D2.1) — commutative accumulator of per-record hashes:
    /// `wrapping_add` of [`terminal_record_hash`] over every finished record,
    /// maintained in O(1) per record at [`Self::finish_terminal`], the sole
    /// record-insertion site.  [`Self::digest`] folds this value instead of
    /// re-walking every record's refusal lists per call (the Θ(N·R) term that
    /// was ~99.7 % of the treant's routing stage).
    ///
    /// See [`Self::digest`] for why this is an order-free accumulator and not
    /// a chain, and [`Self::rebuild_accumulators`] for the from-scratch path
    /// the F6 stage-boundary tripwires use.
    records_acc: u64,
    /// FU-32 J2 (§D2.1) — FNV-1a over the host kd-tree cache
    /// ([`host_cache_digest`]), refreshed only in [`Self::finish_terminal`]'s
    /// `KDTREE_REBUILD_EVERY` branch — the only place hosts change
    /// ([`HostCache::rebuild`]'s only caller).
    hosts_acc: u64,
    /// FU-32 J5 — the routing cadence's execution meter. See
    /// [`RouteCcoWorkMeter`] for why it is not hashed.
    route_work: RouteCcoWorkMeter,
}

impl Default for CcoController {
    fn default() -> Self {
        let hosts = HostCache::default();
        // The cached hosts accumulator must start equal to the from-scratch
        // hash of the empty cache, not to zero, so `rebuild_accumulators`
        // agrees with the cached pair from the very first `digest()` call.
        let hosts_acc = host_cache_digest(&hosts);
        Self {
            hosts,
            inserted: 0,
            terminals: BTreeMap::new(),
            terminal_cadence: Vec::new(),
            sealed: false,
            contact_attestation_digest: 0,
            records_acc: 0,
            hosts_acc,
            route_work: RouteCcoWorkMeter::default(),
        }
    }
}

impl CcoController {
    /// The head of this controller's model-anchor seat-attestation chain.
    pub(crate) const fn contact_attestation_digest(&self) -> u64 {
        self.contact_attestation_digest
    }

    /// FU-32 J5 — what this controller's routing cadence actually spent, for
    /// the receipt `runner::route_initial_cco` holds against its charge.
    pub(crate) const fn route_work_meter(&self) -> RouteCcoWorkMeter {
        self.route_work
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TerminalCadenceRecord {
    disposition: TerminalDisposition,
    /// **The SECOND counter (Craig's authorization item 5, 2026-08-23).** How
    /// many of this terminal's arms were RULED OUT before the tournament could
    /// weigh them.
    ///
    /// It is zero on every path today: nothing rules an arm out yet. The
    /// steering channel that will fill it is Stage B's, and the counter lands
    /// here first, deliberately, so `candidates_feasible` never has to carry
    /// two meanings — see [`FinalizedCcoTranscriptRow::candidates_ruled_out`].
    candidates_ruled_out: usize,
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
                record,
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
        // FU-32 J2 (§D2.2) -- the seal is one of the three genuine-recompute
        // sites: both accumulators are rebuilt from scratch here, never read
        // from the cached fields, so the sealed value is a pure function of
        // the controller's actual state at seal time.
        let (records_acc, hosts_acc) = self.rebuild_accumulators();
        let final_controller_digest = self.digest_with_accumulators(records_acc, hosts_acc);
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
        topology: &FaceTopology,
        process_resolution_mm: f64,
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
            topology,
            Vec::new(),
            Vec::new(),
            None,
            0,
            Some(hosts),
            process_resolution_mm,
            None,
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
                candidates_ruled_out: outcome.candidates_ruled_out,
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
        // **Stage B.** Borrowed, never owned, and never stored: see
        // [`PassSteering`] for why the borrow IS the finiteness obligation.
        // `None` is every pass of every model that has never abandoned.
        steering: Option<&crate::legalization::LegalizationSteering>,
    ) -> Result<TerminalCcoOutcome, CcoError> {
        self.require_unadvanced(request.terminal_id)?;
        // One projection per terminal per pass — not one per candidate.
        let terminal_view = steering.map(|steering| steering.for_terminal(request.terminal_id));
        let terminal_steering = terminal_view.as_ref();
        let controller_digest_before = self.digest();
        let forest_digest_before = forest.digest();
        let context = FinalizedAnchorValidationContext::bind(forest, request, route)?;
        let discovery = discover_finalized_anchor_seats(
            context.bound_contact,
            context.terminal,
            context.route.pool(),
            context.route.config,
            context.route.geometry(),
            context.route.authority.pool_digest(),
        )?;
        let anchor_ray_batches = discovery.query_counts().ray_batches;
        let anchor_seat_refusals = discovery.refusals().to_vec();
        // FU-32 J5 (design D3) — meter the two quantities the routing preflight
        // charged, at the one site that spends them, from the walk's own counts
        // rather than from `pool.rows().len()` re-derived here. A quarantined
        // terminal reaches `advance_skipped_terminal` instead and honestly
        // contributes nothing.
        self.route_work.record_terminal(
            discovery.query_counts().seat_evaluations,
            anchor_seat_refusals.len(),
        );
        let config = Config::current()?;
        let hosts = self.hosts.nearest(
            request.stub_end_mm().map(Finite::get),
            config.bifurcation_hosts,
            forest,
        );
        let mut outcome = grow_terminal_inner(
            forest,
            request,
            context.route.geometry().surface(),
            context.route.geometry().topology(),
            discovery.into_seats(),
            anchor_seat_refusals,
            Some(&context),
            anchor_ray_batches,
            Some(hosts),
            context.route.process_resolution_mm,
            // **Stage B — the read side of the key.** Not "the same five
            // digests" assembled by hand: the SAME CALL that seals this
            // terminal's route evidence twenty lines below, run early. The key
            // a no-good is matched against and the key a no-good is minted
            // from are then the same expression, so they cannot drift apart.
            terminal_steering.map(|terminal| PassSteering {
                prefix: crate::legalization::PrefixKey::of(&context.route_evidence(
                    request,
                    forest_digest_before,
                    controller_digest_before,
                )),
                terminal,
            }),
        )?;
        // F3 -- chain this terminal's seat-attestation transcript before any
        // disposition is recorded, so a terminal that hard-fails later still
        // cannot silently drop its attestation evidence.
        self.extend_contact_attestation(request.terminal_id, outcome.contact_attestation_digest);
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
                candidates_ruled_out: outcome.candidates_ruled_out,
            },
        );
        outcome.disposition = Some(disposition);
        Ok(outcome)
    }

    /// **Stage B — where a no-good is minted from, and why it is here.**
    ///
    /// The one place a no-good's two halves both exist as the ROUTING CADENCE'S
    /// OWN RECORD: the prefix digests this terminal's tournament read, and the
    /// four fields it sealed about the arm it chose.
    ///
    /// It is a read of sealed authority, not a new one. This value is already
    /// folded into `CcoController::digest`, which the transcript carries as
    /// `final_controller_digest` and folds into its own digest — so exposing it
    /// read-only adds no field, moves no fingerprint and lets nobody
    /// manufacture a claim. The alternative considered and rejected was putting
    /// the winner on `FinalizedCcoTranscriptRow`, which the row deliberately
    /// omits ("no mutable candidate and forest details") and which would have
    /// moved every model's sealed transcript digest a SECOND time, unledgered
    /// and unauthorized.
    ///
    /// `None` for a terminal that is quarantined, retired, or not in this
    /// cadence at all: only a terminal the record sealed as `Routed` has an arm
    /// its own tournament chose, and only such a terminal may earn a no-good.
    pub(crate) fn sealed_routed_arm(
        &self,
        terminal_id: u32,
    ) -> Option<(&TerminalRouteEvidence, &CcoCandidateEvidence)> {
        match &self.terminals.get(&terminal_id)?.disposition {
            TerminalDisposition::Routed { route, winner } => Some((route, winner)),
            TerminalDisposition::SkippedQuarantined { .. }
            | TerminalDisposition::UnroutedNoWinner { .. } => None,
        }
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
                // A skipped terminal never had a route request, so it never
                // had an arm to rule out. Zero is the truthful value here, not
                // a placeholder.
                candidates_ruled_out: 0,
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
            anchor_route.process_resolution_mm,
            anchor_route.geometry.surface(),
            // #36 step 3 — the adjacency the seat-zone fill walks, from the one
            // authority that already bound it to this surface and mesh.
            anchor_route.geometry.topology(),
        )?;
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
        // F3 -- this batch's model-anchor seat-attestation transcript. The
        // empty-batch digest rather than zero, so "no anchor was proposed" and
        // "the attestation stopped running" are different values.
        let mut contact_attestation_digest = crate::designed_embed::seat_attestation_digest(
            crate::designed_embed::SEAT_TAG_CCO_MODEL_ANCHOR,
            crate::designed_embed::PositiveFiniteMm::new(1.0).map_err(|error| {
                CcoError::InvalidGeometry(format!(
                    "evaluate_rewire_candidates: the empty-batch attestation digest seed \
                     constant 1.0 mm failed PositiveFiniteMm::new, which should be \
                     unreachable: {error}"
                ))
            })?,
            &[],
            &[],
        );
        if !seats.is_empty() {
            let mut seats: Vec<Option<QualifiedAnchorSeat>> = seats.drain(..).map(Some).collect();
            let mut generation_indices = Vec::new();
            let mut foot_requests = Vec::new();
            for candidate in &feasible {
                if let Some((seat_index, final_radius)) = candidate.anchor {
                    let seats_len = seats.len();
                    let seat = seats
                        .get_mut(seat_index)
                        .and_then(Option::take)
                        .ok_or_else(|| {
                            CcoError::InvalidGeometry(format!(
                                "evaluate_rewire_candidates: candidate generation \
                                 {} points at seat index {seat_index} of {seats_len}, but \
                                 that slot is out of range or was already taken by an \
                                 earlier candidate in this batch",
                                candidate.generation_index
                            ))
                        })?;
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
            // F3 -- PR-8/OD-9 at the seat, over exactly the anchors that
            // survived the foot ball, in ONE batch for this rewire candidate
            // set. Same physics, same channel and same non-failure semantics as
            // the terminal-growth path; the only difference is that a rewire
            // refusal is reported as a `RewireCandidateRefusal` because that is
            // this stage's own quarantine vocabulary.
            let attested_indices = validated_anchors.keys().copied().collect::<Vec<_>>();
            let attested = validated_anchors
                .values()
                .map(Box::as_ref)
                .collect::<Vec<_>>();
            let attested_seats = seated_points_of(&descriptors, &attested_indices)?;
            let attestation = attest_model_anchor_seats(
                anchor_route.geometry.surface(),
                binding.request.stub_end_mm().map(Finite::get),
                binding.request.carried_mm2().get(),
                cco_terminal_radius(&binding.request, &config)?.get(),
                anchor_route.process_resolution_mm,
                &attested,
                &attested_seats,
            )?;
            contact_attestation_digest = attestation.digest;
            for (generation_index, refusal) in attested_indices.iter().zip(&attestation.refusals) {
                // PR-7 — all three verdicts refuse at candidacy on this path
                // too, through the same single mapping the terminal-growth path
                // uses. The rewire commit-time tripwire in
                // `RewireCandidateBatch::winning_generation_index` came out with
                // it; see the argument on the terminal-growth loop.
                let kind = match refusal {
                    Some(crate::designed_embed::ContactAttestationRefusal::BuriedSurface {
                        ..
                    }) => SeatAttestationRefusalKind::Buried,
                    Some(crate::designed_embed::ContactAttestationRefusal::UnusableGeometry {
                        ..
                    }) => SeatAttestationRefusalKind::Unusable,
                    Some(crate::designed_embed::ContactAttestationRefusal::NotAttached {
                        ..
                    }) => SeatAttestationRefusalKind::NotAttached,
                    _ => continue,
                };
                let reason = kind.reason();
                validated_anchors.remove(generation_index).ok_or_else(|| {
                    CcoError::InvalidGeometry(format!(
                        "evaluate_rewire_candidates: seat refusal {reason:?} for candidate \
                         generation {generation_index} names a seat attestation that was never \
                         validated (not present in validated_anchors)"
                    ))
                })?;
                valid.remove(generation_index);
                refusals.push(RewireCandidateRefusal {
                    generation_index: *generation_index,
                    family: descriptors[*generation_index].family(),
                    reason: RewireCandidateRefusalReason::AnchorFoot(reason),
                });
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
                contact_attestation_digest,
                process_resolution_mm: anchor_route.process_resolution_mm,
            });
        }
        let contact_policy = seat_contact_policy(anchor_route.process_resolution_mm)?;
        let survives =
            clearance_survival(anchor_route.geometry.surface(), &contact_policy, &feasible)?;
        let mut candidates = Vec::new();
        for (candidate, verdict) in feasible.into_iter().zip(survives) {
            let generation_index = candidate.generation_index;
            match verdict {
                ClearanceVerdict::Survives => {
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
                }
                // PR-3 (design review F1) — a member that cannot emerge from its
                // own contact mask is refused by NAME, not folded into an
                // anonymous clearance failure.
                ClearanceVerdict::GrazingIncidence | ClearanceVerdict::DegenerateIncidence => {
                    validated_anchors.remove(&generation_index);
                    refusals.push(RewireCandidateRefusal {
                        generation_index,
                        family: candidate.family,
                        reason: RewireCandidateRefusalReason::AnchorFoot(
                            if verdict == ClearanceVerdict::DegenerateIncidence {
                                AnchorFootRefusalReason::SeatDegenerateIncidence
                            } else {
                                AnchorFootRefusalReason::SeatGrazingIncidence
                            },
                        ),
                    });
                }
                ClearanceVerdict::Distance => {
                    refusals.push(RewireCandidateRefusal {
                        generation_index,
                        family: candidate.family,
                        reason: RewireCandidateRefusalReason::Clearance,
                    });
                }
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
            contact_attestation_digest,
            process_resolution_mm: anchor_route.process_resolution_mm,
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

    /// F3 — fold one terminal's model-anchor seat-attestation transcript into
    /// the controller's chain, keyed by the terminal so cadence order matters.
    fn extend_contact_attestation(&mut self, terminal: u32, stage: u64) {
        let mut digest = crate::forest::DigestWriter::new();
        digest.tag(0x8e);
        digest.u64(self.contact_attestation_digest);
        digest.u64(u64::from(terminal));
        digest.u64(stage);
        self.contact_attestation_digest = digest.finish();
    }

    fn finish_terminal(&mut self, forest: &Forest, terminal: u32, record: TerminalCadenceRecord) {
        // FU-32 J2 (§D2.1) -- fold this record's own hash into the
        // commutative accumulator EXACTLY ONCE, at the sole record-insertion
        // site, before the map takes ownership.  The record's refusal lists
        // are hashed here, once, and never re-walked by `digest()`.
        let cadence_index = self.terminal_cadence.len();
        self.records_acc =
            self.records_acc
                .wrapping_add(terminal_record_hash(cadence_index, terminal, &record));
        let previous = self.terminals.insert(terminal, record);
        assert!(previous.is_none(), "terminal cadence was preflighted");
        self.terminal_cadence.push(terminal);
        self.inserted += 1;
        assert_eq!(self.inserted, self.terminals.len());
        assert_eq!(self.inserted, self.terminal_cadence.len());
        if self.inserted.is_multiple_of(KDTREE_REBUILD_EVERY) {
            self.hosts.rebuild(forest);
            // J2 -- hosts change only on this branch (`HostCache::rebuild`
            // has no other caller), so the hosts accumulator refreshes only
            // on this branch.  Skipping this refresh is mutation (b) of the
            // J2 triad and is pinned by
            // `fu32_mutation_b_stale_hosts_accumulator_at_rebuild_boundary_fails_equivalence`.
            self.hosts_acc = host_cache_digest(&self.hosts);
        }
    }

    /// Stable evidence for terminal dispositions and the exact host-cache
    /// scheduling state they produced.
    ///
    /// FU-32 J2 (§D2.1) — **state-pure incremental**.  The two expensive
    /// segments of the pre-J2 whole-state walk (every record's refusal lists,
    /// the whole host kd-tree) enter this stream through accumulators that
    /// are maintained in O(delta) where state changes and are rebuildable in
    /// O(state) by [`Self::rebuild_accumulators`]:
    ///
    /// - `records_acc` — a **commutative** (`wrapping_add`) combination of
    ///   [`terminal_record_hash`] values, folded once per record at
    ///   [`Self::finish_terminal`].  Commutativity is what makes the
    ///   `BTreeMap` iteration order irrelevant — F5 proved a hash CHAIN is
    ///   impossible twice over (FNV-1a has no composition law, and the
    ///   `(k,k,k)` header plus mid-stream `BTreeMap` splices invalidate any
    ///   retained fold at every step), while an order-free accumulator
    ///   composes.  Order is still bound twice: by the `cadence_index` inside
    ///   every record hash, and by the `terminal_cadence` vector, which stays
    ///   hashed in full below.
    /// - `hosts_acc` — [`host_cache_digest`] over the host kd-tree cache,
    ///   refreshed only in the `KDTREE_REBUILD_EVERY` branch, the only place
    ///   hosts change.
    ///
    /// The house precedent for O(1) maintenance at the arrival site under a
    /// justifying doc comment is [`Self::extend_contact_attestation`] just
    /// above: the F3 attestation head already folds per-terminal evidence in
    /// O(1) instead of re-walking history.  The difference here — an
    /// accumulator rather than a chain — is forced by F6: `digest()` must
    /// stay a **pure function of controller state** so the stage-boundary
    /// tripwires (`runner.rs` brace/rewire preflights) can genuinely
    /// recompute it, and by F5's proof that no chain can splice into this
    /// stream.
    ///
    /// **This restructure moves the digest's VALUE** — and with it, per F4,
    /// all seven frozen OD-8 snapshot digests.  That movement is licensed
    /// only under §D2.4's stop-and-report protocol: J2 lands with the digests
    /// moved and NOTHING re-pinned; the owner rules on the from→to table in
    /// J4.  The pre-J2 walk is retained verbatim as the `#[cfg(test)]` oracle
    /// ([`Self::digest_full_walk_oracle`], the G3/G6 retention idiom).
    pub(crate) fn digest(&self) -> u64 {
        self.digest_with_accumulators(self.records_acc, self.hosts_acc)
    }

    /// The §D2.1 word stream:
    /// `FNV(inserted, |terminals|, |cadence|, cadence…, sealed, records_acc, hosts_acc)`
    /// — Θ(k) cheap words per call (≈1.2 MB over a whole treant run, F5),
    /// against the Θ(N·R) ≈ 777 GB the pre-J2 walk cost.
    fn digest_with_accumulators(&self, records_acc: u64, hosts_acc: u64) -> u64 {
        let mut fnv = FnvWords::new();
        fnv.word(self.inserted as u64);
        fnv.word(self.terminals.len() as u64);
        fnv.word(self.terminal_cadence.len() as u64);
        for terminal in &self.terminal_cadence {
            fnv.word(u64::from(*terminal));
        }
        fnv.word(u64::from(self.sealed));
        fnv.word(records_acc);
        fnv.word(hosts_acc);
        fnv.finish()
    }

    /// FU-32 J2 (§D2.2) — the genuine recompute-from-state path: both
    /// accumulators rebuilt from scratch, never read from the cached fields.
    ///
    /// O(state) by design, and called from exactly the three sites that need
    /// a real recompute — [`Self::finalize_transcript`]'s seal and the two
    /// stage-boundary tripwires ([`Self::sealed_digest_matches_rebuilt`],
    /// called by the brace and rewire preflights in `runner.rs`) — versus the
    /// N per-cadence-entry calls that ride the cached pair.  This is the G3
    /// severance between *charged* and *executed*: incremental where the
    /// digest is consumed as evidence, exhaustive where it is consumed as a
    /// check.
    ///
    /// A cadence entry with no matching record folds nothing; the resulting
    /// accumulator then disagrees with any honestly sealed value (and the
    /// `(inserted, |terminals|, |cadence|)` header words diverge besides), so
    /// a tampered map still fails the comparison rather than panicking a
    /// tripwire.
    pub(crate) fn rebuild_accumulators(&self) -> (u64, u64) {
        let mut records_acc = 0_u64;
        for (cadence_index, &terminal) in self.terminal_cadence.iter().enumerate() {
            if let Some(record) = self.terminals.get(&terminal) {
                records_acc =
                    records_acc.wrapping_add(terminal_record_hash(cadence_index, terminal, record));
            }
        }
        (records_acc, host_cache_digest(&self.hosts))
    }

    /// F6/§D2.2 — the stage-boundary tripwire arm: recompute-and-compare
    /// against the sealed controller digest.
    ///
    /// This never trusts the cached accumulators.  Both are genuinely rebuilt
    /// from state by [`Self::rebuild_accumulators`], and the rebuilt pair is
    /// *additionally* compared against the cached pair, so the tripwire
    /// catches BOTH failure modes:
    ///
    /// 1. a controller whose **state** was mutated after seal — the rebuilt
    ///    digest moves off the sealed value (the pre-J2 tripwire's teeth,
    ///    kept whole);
    /// 2. a controller whose **cached accumulators** drifted from its state —
    ///    the incremental maintenance itself corrupted.  A pure rebuild
    ///    cannot see this case (state still matches the seal), and the cached
    ///    `digest()` alone cannot see case 1; comparing both arms is what
    ///    keeps a cache-corruption bug from silently signing every
    ///    per-terminal `controller_digest_before` with a wrong value while
    ///    the sealed comparison passes.
    ///
    /// Pinned by
    /// `fu32_a_corrupted_accumulator_is_caught_at_the_stage_boundary_tripwire`.
    pub(crate) fn sealed_digest_matches_rebuilt(&self, sealed: u64) -> bool {
        let (records_acc, hosts_acc) = self.rebuild_accumulators();
        self.digest_with_accumulators(records_acc, hosts_acc) == sealed
            && (records_acc, hosts_acc) == (self.records_acc, self.hosts_acc)
    }

    /// The pre-J2 whole-state walk, retained VERBATIM as the test oracle —
    /// the FU-8c G3/G6 retention idiom (`first_safe_proposal`,
    /// `reference_contacts_x_rows_scan`).  Deliberately self-contained (its
    /// own inline FNV fold, its own record match) so a defect in the shared
    /// helpers cannot fool it.
    ///
    /// Post-J2 its VALUE necessarily differs from [`Self::digest`] — F5
    /// proved no value-preserving restructure of this stream exists — so
    /// tests compare it for the *partition* it induces on states (two states
    /// the old walk distinguished must stay distinguished by the new digest,
    /// and trajectories the old walk called identical must stay identical),
    /// not for equality with the new stream.
    #[cfg(test)]
    fn digest_full_walk_oracle(&self) -> u64 {
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
                        NoWinnerEvidence::ConfirmationRoundExhausted {
                            candidates_generated,
                            candidates_feasible,
                            round_arms,
                            anchor_seat_refusals,
                            anchor_refusals,
                        } => {
                            word(3);
                            word(*candidates_generated as u64);
                            word(*candidates_feasible as u64);
                            digest_round_arms(&mut word, round_arms);
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

/// FNV-1a (`h ← (h ⊕ b) · p`) over little-endian `u64` words — the exact
/// fold `CcoController::digest` has always used (F5), factored so the
/// per-record hashes, the hosts accumulator and the digest stream itself are
/// provably one algorithm.
struct FnvWords(u64);

impl FnvWords {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    const fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    fn word(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    const fn finish(&self) -> u64 {
        self.0
    }
}

/// FU-32 J2 (§D2.1) — the per-record hash folded into `records_acc`: a fresh
/// FNV-1a over `cadence_index ‖ terminal_id ‖ record words`, computed ONCE,
/// at `finish_terminal`, over that record's own refusal lists only.
///
/// `cadence_index` is bound INSIDE each record hash so the commutative fold
/// cannot equate two trajectories that inserted the same records at swapped
/// positions — mutation (c) of the J2 triad, pinned by
/// `fu32_mutation_c_dropping_cadence_index_makes_swapped_records_collide`.
/// The record words after the header are emitted by the same
/// `digest_terminal_record` walk on both the incremental and the rebuild
/// paths.
fn terminal_record_hash(
    cadence_index: usize,
    terminal: u32,
    record: &TerminalCadenceRecord,
) -> u64 {
    let mut fnv = FnvWords::new();
    fnv.word(cadence_index as u64);
    fnv.word(u64::from(terminal));
    let mut word = |value: u64| fnv.word(value);
    digest_terminal_record(&mut word, record);
    fnv.finish()
}

/// One terminal record's disposition words — word-identical to the segment
/// the pre-J2 whole-state walk emitted for the same record (retained verbatim
/// as `digest_full_walk_oracle`).
fn digest_terminal_record(word: &mut impl FnMut(u64), record: &TerminalCadenceRecord) {
    match &record.disposition {
        TerminalDisposition::Routed { route, winner } => {
            word(0);
            digest_route_evidence(word, route);
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
            digest_route_evidence(word, route);
            match reason {
                NoWinnerEvidence::NoCandidatesGenerated {
                    anchor_seat_refusals,
                } => {
                    word(0);
                    digest_anchor_seat_refusals(word, anchor_seat_refusals);
                }
                NoWinnerEvidence::NoFeasibleCandidate {
                    candidates_generated,
                    anchor_seat_refusals,
                    anchor_refusals,
                } => {
                    word(1);
                    word(*candidates_generated as u64);
                    digest_anchor_seat_refusals(word, anchor_seat_refusals);
                    digest_anchor_refusals(word, anchor_refusals);
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
                    digest_anchor_seat_refusals(word, anchor_seat_refusals);
                    digest_anchor_refusals(word, anchor_refusals);
                }
                NoWinnerEvidence::ConfirmationRoundExhausted {
                    candidates_generated,
                    candidates_feasible,
                    round_arms,
                    anchor_seat_refusals,
                    anchor_refusals,
                } => {
                    word(3);
                    word(*candidates_generated as u64);
                    word(*candidates_feasible as u64);
                    digest_round_arms(word, round_arms);
                    digest_anchor_seat_refusals(word, anchor_seat_refusals);
                    digest_anchor_refusals(word, anchor_refusals);
                }
            }
        }
    }
}

/// FU-32 J2 (§D2.1) — `hosts_acc`: a fresh FNV-1a over the host kd-tree
/// cache, word-identical to the final segment of the pre-J2 walk.  Hosts
/// change only in `finish_terminal`'s `KDTREE_REBUILD_EVERY` branch
/// (`HostCache::rebuild` has no other caller), so that branch is the only
/// place the cached value refreshes.
fn host_cache_digest(hosts: &HostCache) -> u64 {
    let mut fnv = FnvWords::new();
    fnv.word(hosts.indexed_members.len() as u64);
    for key in &hosts.indexed_members {
        fnv.word(u64::from(key.slot()));
        fnv.word(u64::from(key.generation()));
    }
    fnv.word(hosts.samples.len() as u64);
    for (point, key) in &hosts.samples {
        for coordinate in point {
            fnv.word(coordinate.to_bits());
        }
        fnv.word(u64::from(key.slot()));
        fnv.word(u64::from(key.generation()));
    }
    fnv.finish()
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
    // The whole cadence record, rather than its disposition plus a loose
    // counter. The row IS the record, sealed: handing over the two halves
    // separately would let a caller pair one terminal's disposition with
    // another's count, and it pushed this signature past the argument lint for
    // no gain in clarity.
    record: &TerminalCadenceRecord,
    cadence_index: usize,
    plan_digest: u64,
    loads_digest: u64,
    geometry_digest: u64,
) -> Result<FinalizedCcoTranscriptRow, CcoError> {
    let TerminalCadenceRecord {
        disposition,
        candidates_ruled_out,
    } = record;
    let candidates_ruled_out = *candidates_ruled_out;
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
                candidates_ruled_out,
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
                candidates_ruled_out,
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
                candidates_ruled_out,
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
        // **The second counter, folded UNCONDITIONALLY** — Craig's
        // authorization item 5, 2026-08-23. Not "only when non-zero": a
        // conditional fold would keep this fingerprint still for models that
        // rule nothing out, which is precisely the cheap-today, expensive-later
        // shape the ruling declined. Every model's sealed transcript digest
        // moves once, here, on purpose, and the move is ledgered in
        // `docs/engineering-journal/EMISSION-CANARY-LEDGER.md`.
        word(row.candidates_ruled_out as u64);
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
            // **A NEW discriminant, so no existing row moves.** Every model
            // that retires nothing through a round keeps its transcript
            // fingerprint byte for byte: the three variants above fold exactly
            // as they did, and this one is reachable only by a terminal a round
            // actually retired. Craig's authorization item 5 bought ONE
            // corpus-wide re-freeze, at Stage A, and this deliberately does not
            // spend a second.
            Some(NoWinnerEvidence::ConfirmationRoundExhausted {
                candidates_generated,
                candidates_feasible,
                round_arms,
                anchor_seat_refusals,
                anchor_refusals,
            }) => {
                word(4);
                word(*candidates_generated as u64);
                word(*candidates_feasible as u64);
                digest_round_arms(&mut word, round_arms);
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
            // F3. New discriminants are APPENDED, never renumbered: the
            // existing six are load-bearing in the sealed transcript digest,
            // and reordering them would move every model's digest without any
            // geometry changing.
            AnchorFootRefusalReason::SeatBuriedSurface => 7,
            AnchorFootRefusalReason::SeatNotAttached => 8,
            AnchorFootRefusalReason::SeatGeometryUnusable => 9,
            // PR-3 (#51). Appended, for the same reason the F3 discriminants
            // were: renumbering would move every model's sealed transcript
            // digest without any geometry changing.
            AnchorFootRefusalReason::SeatGrazingIncidence => 10,
            AnchorFootRefusalReason::SeatDegenerateIncidence => 11,
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
        // FU-32 J1.  These four comparisons are new *code* but not a new
        // *check*: before the hoist, the four values recorded below were each
        // recomputed here from the subject itself, so recording them was the
        // check.  With the hash hoisted out of the loop, the recording has to
        // state explicitly that the hoisted proof still matches the subject's
        // own sealed witness — otherwise a corrupted token would be copied
        // straight into the transcript on the winner arm, which never reaches
        // `validate_replay`.  Every witness on the right-hand side is O(1):
        // three stored fields and one borrowed fingerprint.
        if route.authority.plan_digest() != route.plan().finalized_load_plan_digest()
            || route.authority.loads_digest() != route.loads().digest()
            || route.authority.pool_digest() != route.pool().digest()
            || route.authority.geometry_fingerprint() != route.geometry().fingerprint_ref()
        {
            return Err(CcoError::StaleFinalizedAnchorAuthority);
        }
        Ok(Self {
            plan_digest: route.authority.plan_digest(),
            loads_digest: route.authority.loads_digest(),
            pool_digest: route.authority.pool_digest(),
            geometry_fingerprint: route.authority.geometry_fingerprint().clone(),
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
        // FU-32 J1.  The comparison stays exactly here and refuses exactly as
        // it did; only the whole-structure re-hashes on the left-hand side
        // were hoisted.  Each right-hand value was copied from the token at
        // `bind`; each left-hand value is the subject's own O(1) sealed
        // witness — `finalized_load_plan_digest` is the plan's own record of
        // `digest()`, proved equal to it once by
        // `VerifiedRoutingAuthority::compute` and again by
        // `validate_initial_cco_preflight`.  A token corrupted after `compute`
        // therefore still fails here.
        if self.route.plan().finalized_load_plan_digest() != self.plan_digest
            || self.route.loads().digest() != self.loads_digest
            || self.route.pool().digest() != self.pool_digest
            || *self.route.geometry().fingerprint_ref() != self.geometry_fingerprint
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
            geometry_digest: self.route.geometry().digest(),
            forest_digest_before,
            controller_digest_before,
        }
    }
}

/// Fold a confirmation round's measured rows. Every field, by exact bits: the
/// round is the evidence a retirement rests on, so a row that changed and a
/// digest that did not would be exactly the hole this stage exists to close.
fn digest_round_arms(word: &mut impl FnMut(u64), arms: &[crate::legalization::RoundArm]) {
    word(arms.len() as u64);
    for arm in arms {
        word(arm.rank as u64);
        arm.arm.digest(word);
        word(arm.gap_mm.to_bits());
        word(arm.distance_clearance_mm.to_bits());
        word(u64::from(arm.axis_transit_illegal));
        word(arm.terminal_radius_mm.to_bits());
        word(arm.rootward_radius_mm.to_bits());
    }
}

fn no_winner_evidence(outcome: &TerminalCcoOutcome) -> NoWinnerEvidence {
    debug_assert!(outcome.winner.is_none());
    // Checked FIRST, because a round's verdict is about arms that DID exist and
    // DID pass feasibility, and folding it into one of the three counting
    // reasons would lose exactly the fact that makes the retirement earned.
    if let Some(round_arms) = outcome.confirmation_round.clone() {
        return NoWinnerEvidence::ConfirmationRoundExhausted {
            candidates_generated: outcome.candidates_generated,
            candidates_feasible: outcome.candidates_feasible,
            round_arms,
            anchor_seat_refusals: outcome.anchor_seat_refusals.clone(),
            anchor_refusals: outcome.anchor_refusals.clone(),
        };
    }
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

/// The placed stub's tip radius against the one the finalized load authority
/// sized, in the ONE direction that is a defect.
///
/// This used to be an equality, and the equality was standing in for the real
/// requirement: *the stub CCO is about to build on must not be under-sized for
/// the load the authority certified*. Equality enforced that and one more thing
/// besides — that nothing ever raised a tip — which grow-to-attest legitimately
/// does. A tip refused PR-8 at the radius the load law asked for is regrown, at
/// `initial.rs`, until it attests, and the run then BUILDS the grown tip. So the
/// placed radius can honestly exceed the finalized one.
///
/// What is checked instead is the same safety property with the accidental half
/// removed:
///
/// * **Never smaller.** A placed tip below the finalized radius would be
///   carrying more load than it was sized for. That is the failure this
///   function exists to catch and it is unchanged.
/// * **Never above the registered maximum.** `TIP_RADIUS_MAX_LEGACY_MM` is the
///   owner's cap on any emitted tip, so a radius above it is not a legal
///   revision however it arrived.
///
/// Growth can therefore only ever move a tip in the conservative direction —
/// `FrozenTipLoadPolicy::violates_safety_factor` is monotone decreasing in
/// radius, so a larger tip satisfies the area law at least as well as the
/// finalized one did. UNITS: both arguments are RADII in mm.
fn validate_placed_tip_against_finalized_load(
    placed_radius_mm: PositiveFinite,
    finalized_radius_mm: PositiveFinite,
) -> Result<(), CcoError> {
    if placed_radius_mm.get() < finalized_radius_mm.get() {
        return Err(CcoError::FinalizedStubLoadMismatch);
    }
    let max_radius_mm = m0_constants()
        .map_err(|error| CcoError::Constants(error.to_string()))?
        .placeholders()
        .tip_radius_max_legacy_mm()
        .diagnostic()
        .get();
    if placed_radius_mm.get() > max_radius_mm {
        return Err(CcoError::FinalizedStubLoadMismatch);
    }
    Ok(())
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
    // FU-32 J1: same check, same refusal; the whole-pool re-hash behind its
    // content arm was hoisted to `VerifiedRoutingAuthority::compute`.
    route
        .pool()
        .validate_against_hoisted(route.geometry(), route.authority.pool_digest())?;
    // FU-32 J1: the linear `find` over every plan contact became a map lookup
    // in the token's contact index.  The comparison is unchanged — the plan's
    // contact for this id must be equal to the one the route carries.
    if route.contact.contact_id() != route.contact_id
        || route.authority.contact(route.contact_id) != Some(route.contact)
    {
        return Err(CcoError::FinalizedContactMismatch);
    }
    // FU-32 J1: built once by the token instead of once per call.
    let contact_ids = route.authority.contact_ids();
    // FU-32 J1: same four-arm check, same refusal; the whole-partition row
    // walk behind its third arm was hoisted.
    if route
        .loads()
        .validate_binding_hoisted(
            route.plan().load_partition(),
            contact_ids,
            route.authority.load_rows_fingerprint(),
        )
        .is_err()
    {
        return Err(CcoError::FinalizedLoadBindingMismatch);
    }
    let leaf = route
        .loads()
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
    {
        return Err(CcoError::FinalizedStubLoadMismatch);
    }
    validate_placed_tip_against_finalized_load(
        request.finalized_tip_radius_mm(),
        leaf.tip_radius_mm(),
    )?;
    // FU-32 J1: the plan digest inside this fingerprint is the token's, hashed
    // once for the stage and proved equal to the plan's own seal at
    // `VerifiedRoutingAuthority::compute`; the contact is the one this
    // function just proved equal to the plan's contact for this id.  Same
    // fingerprint, same bytes — without a second full plan walk and a second
    // linear scan.
    Ok(BoundPhysicalAnchorContact::from_hoisted(
        route.authority.plan_digest(),
        route.contact,
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

/// **Stage B.** One pass's steering instruction for ONE terminal, plus the
/// live prefix key the no-goods are matched against.
///
/// It is `Copy` and borrowed, and it is deliberately assembled by the CALLER
/// from values the caller already holds: the five digests here are the very
/// same ones `route_evidence` seals for this terminal a few lines later, so the
/// read side of the key and the write side cannot drift apart.
///
/// **The finiteness obligation lives on this type's shape.** It is a borrowed
/// parameter and nothing more. It is not a field of `CcoController`, so
/// `CcoController::digest` cannot see it, so `controller_digest_before` cannot
/// move when the set grows — which is what keeps the context universe finite
/// and the termination argument non-circular (design §4 Stage B, and
/// `a_growing_no_good_set_moves_no_context_digest`).
#[derive(Clone, Copy, Debug)]
struct PassSteering<'a> {
    prefix: crate::legalization::PrefixKey,
    terminal: &'a crate::legalization::TerminalSteering<'a>,
}

/// What the steering did to this terminal's slate.
enum SteeringOutcome {
    /// Ordinary steering: this many arms were passed over, and at least one
    /// remains for the tournament to weigh.
    Suppressed { ruled_out: usize },
    /// A confirmation round: take the arm at this rank of the tournament's own
    /// order, suppressing nothing. A rank past the end of the slate is how a
    /// round ends in exhaustion, and it is handled by the caller's `nth`
    /// returning `None`.
    Forced { rank: usize },
}

/// Apply one pass's steering to a sorted survivor slate.
///
/// Batched by construction: one walk over the slate to build the identities,
/// one probe over the whole slate at once, one retain. No native call, and no
/// per-candidate call into anything.
///
/// **Every comparison is streamed, both halves.** Without it a no-good that
/// misses on every pass is indistinguishable from a model that genuinely
/// cannot be legalized: the second pass would learn nothing new, the abandon
/// rule would fire, and the run would refuse by name for what looks like the
/// right reason. The log line is what separates a live mechanism from a no-op.
fn apply_steering(
    steering: &PassSteering<'_>,
    survivors: &mut Vec<&EvaluatedCandidate>,
) -> Result<SteeringOutcome, CcoError> {
    let slate = survivors
        .iter()
        .map(|candidate| {
            crate::legalization::ArmIdentity::live(
                candidate.family,
                candidate.generation_index,
                candidate.price_ml,
                candidate.rows.len(),
            )
        })
        .collect::<Vec<_>>();
    let (verdict, probes) = steering.terminal.verdict(steering.prefix, &slate);
    for probe in &probes {
        crate::coverage_repair::emit_run_line(&format!(
            "legalize: steer t{} {}",
            steering.terminal.terminal_id(),
            probe.render(),
        ));
    }

    match verdict {
        crate::legalization::SteeringVerdict::Forced { rank } => {
            crate::coverage_repair::emit_run_line(&format!(
                "legalize: round t{} takes arm rank {rank} of {} in tournament order",
                steering.terminal.terminal_id(),
                survivors.len(),
            ));
            Ok(SteeringOutcome::Forced { rank })
        }
        crate::legalization::SteeringVerdict::Spent { ruled_out } => {
            crate::coverage_repair::emit_run_line(&format!(
                "legalize: steering spent t{} — all {ruled_out} arms carry a no-good; opening a \
                 confirmation round instead of retiring",
                steering.terminal.terminal_id(),
            ));
            Err(CcoError::SteeringSpent {
                terminal_id: steering.terminal.terminal_id(),
            })
        }
        crate::legalization::SteeringVerdict::Suppressed { ruled_out, keep } => {
            let mut index = 0_usize;
            survivors.retain(|_| {
                let keep = keep[index];
                index += 1;
                keep
            });
            Ok(SteeringOutcome::Suppressed { ruled_out })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn grow_terminal_inner(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    surface: &NativeSurface,
    topology: &FaceTopology,
    mut seats: Vec<QualifiedAnchorSeat>,
    anchor_seat_refusals: Vec<AnchorSeatRefusal>,
    anchor_context: Option<&FinalizedAnchorValidationContext<'_, '_>>,
    anchor_ray_batches: usize,
    cached_hosts: Option<Vec<MemberKey>>,
    process_resolution_mm: f64,
    steering: Option<PassSteering<'_>>,
) -> Result<TerminalCcoOutcome, CcoError> {
    request.validate(forest)?;
    let config = Config::current()?;
    // PR-1/PR-2/PR-3 all read this policy. It is built once per terminal, from
    // the registry plus THIS RUN's process resolution, and handed to both the
    // seat derivation and the clearance screen so the two cannot disagree.
    let contact_policy = seat_contact_policy(process_resolution_mm)?;
    let descriptors = generate_descriptors(
        forest,
        request,
        &config,
        &seats,
        cached_hosts,
        true,
        process_resolution_mm,
        surface,
        topology,
    )?;
    let candidates_generated = descriptors.len();
    let snapshot = forest.clone();
    let mut evaluated = evaluate_descriptors(&snapshot, request, &descriptors, &config)?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let mut validated_anchors = BTreeMap::<usize, Box<ValidatedModelAnchor>>::new();
    let mut anchor_refusals = Vec::new();
    let mut anchor_foot_batches = 0;
    // F3 -- this terminal's model-anchor seat-attestation transcript. A terminal
    // with no seats still produces the empty-batch digest rather than zero, so
    // "no anchor was proposed" and "the attestation stopped running" are
    // different values.
    let mut contact_attestation_digest = crate::designed_embed::seat_attestation_digest(
        crate::designed_embed::SEAT_TAG_CCO_MODEL_ANCHOR,
        crate::designed_embed::PositiveFiniteMm::new(1.0).map_err(|error| {
            CcoError::InvalidGeometry(format!(
                "grow_terminal_inner: the empty-batch attestation digest seed constant \
                 1.0 mm failed PositiveFiniteMm::new, which should be unreachable: {error}"
            ))
        })?,
        &[],
        &[],
    );
    if !seats.is_empty() {
        let seat_count = seats.len();
        let context = anchor_context.ok_or_else(|| {
            CcoError::InvalidGeometry(format!(
                "grow_terminal_inner: {seat_count} model-anchor seat(s) require foot \
                 validation but no finalized anchor validation context was supplied"
            ))
        })?;
        let mut seats: Vec<Option<QualifiedAnchorSeat>> = seats.drain(..).map(Some).collect();
        let mut generation_indices = Vec::new();
        let mut foot_requests = Vec::new();
        for candidate in &evaluated {
            if let Some((seat_index, final_radius)) = candidate.anchor {
                let seats_len = seats.len();
                let seat = seats
                    .get_mut(seat_index)
                    .and_then(Option::take)
                    .ok_or_else(|| {
                        CcoError::InvalidGeometry(format!(
                            "grow_terminal_inner: candidate generation {} points at seat \
                             index {seat_index} of {seats_len}, but that slot is out of \
                             range or was already taken by an earlier candidate",
                            candidate.generation_index
                        ))
                    })?;
                generation_indices.push(candidate.generation_index);
                foot_requests.push(AnchorFootRequest::new(seat, final_radius));
            }
        }
        let validation = validate_finalized_anchor_feet(
            foot_requests,
            context.bound_contact,
            context.terminal,
            context.route.pool(),
            context.route.config,
            context.route.geometry(),
            context.route.authority.pool_digest(),
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
        // F3 -- PR-8/OD-9 at the seat, over exactly the anchors that survived
        // the foot ball, in ONE batch for this terminal.
        let attested_indices = validated_anchors.keys().copied().collect::<Vec<_>>();
        let attested = validated_anchors
            .values()
            .map(Box::as_ref)
            .collect::<Vec<_>>();
        let attested_seats = seated_points_of(&descriptors, &attested_indices)?;
        let attestation = attest_model_anchor_seats(
            surface,
            request.stub_end_mm().map(Finite::get),
            request.carried_mm2().get(),
            cco_terminal_radius(request, &config)?.get(),
            context.route.process_resolution_mm,
            &attested,
            &attested_seats,
        )?;
        contact_attestation_digest = attestation.digest;
        for (index, refusal) in attested_indices.iter().zip(&attestation.refusals) {
            // **PR-7 landed, so all three verdicts now refuse at CANDIDACY.**
            //
            // `NotAttached` used to be recorded here and enforced at commit by
            // `elect_committable_winner`, and the reason was explicit: the CCO
            // family was deliberately UNSEATED, so refusing an unseated kiss
            // while it was still competing would have landed PR-7's
            // family-killing consequence without landing PR-7 — the accidental
            // physics change OD-8 forbids. `apply_model_anchor` now seats the
            // rootward end (PR-1/PR-7), so `NotAttached` stops being a
            // statement about a MISSING RULE and becomes an ordinary defect:
            // this candidate's seat does not reach the solid. It therefore
            // refuses at candidacy exactly as the brace model rung's does and
            // exactly as OD-9 burial already did, and the commit-time tripwire
            // and its rewire sibling came out in the same commit, as their own
            // doc comments predicted they would.
            let refusal_reason = match refusal {
                Some(crate::designed_embed::ContactAttestationRefusal::BuriedSurface {
                    ..
                }) => SeatAttestationRefusalKind::Buried,
                Some(crate::designed_embed::ContactAttestationRefusal::UnusableGeometry {
                    ..
                }) => SeatAttestationRefusalKind::Unusable,
                Some(crate::designed_embed::ContactAttestationRefusal::NotAttached { .. }) => {
                    SeatAttestationRefusalKind::NotAttached
                }
                _ => continue,
            };
            // Same channel a breached foot ball uses: the candidate leaves the
            // tournament, the terminal routes some other way, nothing fails.
            let anchor = validated_anchors.remove(index).ok_or_else(|| {
                CcoError::InvalidGeometry(format!(
                    "grow_terminal_inner: seat refusal ({refusal_reason:?}) for candidate \
                     generation {index} names a seat attestation that was never validated \
                     (not present in validated_anchors)"
                ))
            })?;
            valid.remove(index);
            anchor_refusals.push(refusal_reason.refusal(anchor.seat().provenance()));
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
            candidates_ruled_out: 0,
            confirmation_round: None,
            anchor_ray_batches,
            anchor_foot_batches,
            clearance_batches: 0,
            validated_model_anchor: None,
            anchor_seat_refusals,
            anchor_refusals,
            contact_attestation_digest,
        });
    }

    let survival = clearance_survival(surface, &contact_policy, &evaluated)?;
    // PR-3 (design review F1). A model anchor refused for grazing or degenerate
    // incidence leaves through the SAME channel a breached foot ball uses, so
    // it is visible in the terminal's refusal evidence instead of being folded
    // into an anonymous clearance failure. Non-anchor families have no such
    // channel and drop exactly as a distance refusal does; nothing hard-fails
    // either way (OD-4/PR-9).
    for (candidate, verdict) in evaluated.iter().zip(&survival) {
        let degenerate = match verdict {
            ClearanceVerdict::GrazingIncidence => false,
            ClearanceVerdict::DegenerateIncidence => true,
            ClearanceVerdict::Survives | ClearanceVerdict::Distance => continue,
        };
        if let Some(anchor) = validated_anchors.remove(&candidate.generation_index) {
            anchor_refusals.push(AnchorFootRefusal::seat_emergence(
                anchor.seat().provenance(),
                degenerate,
            ));
        }
    }
    let mut survivors: Vec<&EvaluatedCandidate> = evaluated
        .iter()
        .zip(survival)
        .filter_map(|(candidate, verdict)| verdict.survives().then_some(candidate))
        .collect();
    survivors.sort_by(|left, right| compare_candidates(left, right));

    // ---------------------------------------------- Stage B: the two channels
    //
    // `survivors` is now in the tournament's own order, which is what makes a
    // confirmation round replayable: "arm rank k" means the same arm on every
    // pass with the same prefix.
    //
    // A pass with nothing to steer reads nothing below — `is_idle` is the
    // whole cost this mechanism imposes on a model that never abandons.
    let arms_ruled_out = steering
        .filter(|steering| !steering.terminal.is_idle())
        .map(|steering| apply_steering(&steering, &mut survivors))
        .transpose()?;
    let (candidates_ruled_out, forced_rank) = match arms_ruled_out {
        Some(SteeringOutcome::Suppressed { ruled_out }) => (ruled_out, None),
        Some(SteeringOutcome::Forced { rank }) => (0, Some(rank)),
        None => (0, None),
    };
    // **The retirement a round earned.** A forced rank past the last arm means
    // the round has driven every one of them and every one was measured
    // illegal. This is the ONE path on which Stage B retires a terminal, and it
    // seals the round that earned it — the measured millimetres and the radii
    // the resize assigned, arm by arm, in the tournament's own order.
    let confirmation_round = forced_rank
        .filter(|rank| *rank >= survivors.len())
        .map(|_| {
            steering.map_or_else(Vec::new, |steering| {
                steering.terminal.round_measured().to_vec()
            })
        });

    // A round takes the arm at its rank; an ordinary pass takes the tournament
    // winner, which is rank 0 of the same order.
    let Some(winner) = survivors.into_iter().nth(forced_rank.unwrap_or(0)) else {
        return Ok(TerminalCcoOutcome {
            winner: None,
            disposition: None,
            candidates_generated,
            candidates_feasible,
            candidates_ruled_out,
            confirmation_round,
            anchor_ray_batches,
            anchor_foot_batches,
            clearance_batches: 1,
            validated_model_anchor: None,
            anchor_seat_refusals,
            anchor_refusals,
            contact_attestation_digest,
        });
    };
    let winner_index = winner.generation_index;
    if let Some(context) = anchor_context {
        context.validate_replay(forest, request)?;
    }
    let checkpoint = forest.checkpoint();
    crate::census_record!(crate::census::record_append(
        match &descriptors[winner_index] {
            Descriptor::Plate { .. } => "cco_winner_plate",
            Descriptor::Bifurcate { .. } => "cco_winner_bifurcate",
            Descriptor::ModelAnchor { .. } => "cco_winner_model_anchor",
        }
    ));
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
        candidates_ruled_out,
        // A pass that seated a winner retired nobody, so it seals no round.
        confirmation_round: None,
        anchor_ray_batches,
        anchor_foot_batches,
        clearance_batches: 1,
        validated_model_anchor: validated_anchors.remove(&winner_index),
        anchor_seat_refusals,
        anchor_refusals,
        contact_attestation_digest,
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

/// One candidate's clearance verdict.
///
/// This used to be a `bool`. It is an enum because the mask convergence
/// (#51/#36) gives the CCO family two REFUSALS that are not distance failures
/// and must not be reported as one: a member that cannot emerge from its own
/// contact mask (PR-3) and a contact whose incidence is numerically zero
/// (PR-2). Reporting either as `Clearance` would hide the physics behind a
/// number that the mask has already made meaningless for that row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClearanceVerdict {
    Survives,
    /// The kernel's distance test refused at least one row.
    Distance,
    /// PR-3 — at least one contact-declaring row cannot emerge from its own
    /// mask(s).
    GrazingIncidence,
    /// PR-2 — at least one contact-declaring row has no finite mask.
    DegenerateIncidence,
}

impl ClearanceVerdict {
    const fn survives(self) -> bool {
        matches!(self, Self::Survives)
    }
}

/// The batched clearance screen, with the PR-2 mask and the PR-3 emergence rule
/// applied at the ONE place the surface is available to this stage.
///
/// **#51 / #36 — what changed and why it had to change here.**
/// `clearance_rows` used to compute a FLAT inset pair per row
/// (`contact_depth_mm` terminalward, `standoff_mm` rootward). For a model
/// anchor that made the test unsatisfiable as a matter of arithmetic rather
/// than geometry: the last tested station sits exactly `standoff` from a point
/// ON the surface, so `clr = d_min - (r_max + standoff) <= -r_max`, always,
/// which on the corpus is `-0.350 mm` exactly and was attained 53 times.
/// The honest inset is `contact::placed_mask_inset_mm`, the PR-2 mask
/// `brace.rs` has used since Phase B-2 — and it needs the surface normal at the
/// contact point, which needs the kernel. `clearance_rows` runs inside the
/// `rayon` candidate map with no surface, so the row records WHAT it is and the
/// mask is derived here.
///
/// **Batching.** ONE `NativeSurface::project` for every contact-declaring row
/// in the whole candidate batch, alongside the one `segment_clearance` that was
/// already here. Not one per candidate and not one per member: the projection
/// count per tournament goes from zero to one, which is the same shape
/// `brace.rs` uses at its own exact pass.
///
/// **PR-3 is not optional here (design review F1).** PR-2 enlarges the exempt
/// region; when the mask exceeds the member's own length `seg_clear_one_excluding`
/// returns `INFINITY` and the member is admitted having never been tested. The
/// emergence rule is what keeps the enlarged exemption from becoming an
/// erasure, so it lands in the same commit as the mask and at the same site.
/// It introduces no constant: `min_illegal_len_mm` is already carried on
/// `ContactPolicy`.
fn clearance_survival(
    surface: &NativeSurface,
    policy: &crate::contact::ContactPolicy,
    candidates: &[EvaluatedCandidate],
) -> Result<Vec<ClearanceVerdict>, CcoError> {
    let mut p0 = Vec::new();
    let mut p1 = Vec::new();
    let mut r0 = Vec::new();
    let mut r1 = Vec::new();
    let mut contacts: Vec<(usize, ClearanceContact)> = Vec::new();
    // **#36 step 5 — one frozen exclusion set per row, in row order.** A row
    // with no contact, or a contact-declaring row with no frozen zone, exempts
    // NOTHING and is graded against the whole surface.
    let mut excluded: Vec<&[u32]> = Vec::new();
    const EMPTY: &[u32] = &[];
    let mut ranges = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let start = p0.len();
        for row in &candidate.rows {
            if let Some(contact) = row.contact {
                contacts.push((p0.len(), contact));
            }
            excluded.push(row.seat_zone.as_ref().map_or(EMPTY, |zone| zone.as_ref()));
            p0.push(row.p0);
            p1.push(row.p1);
            r0.push(row.r0);
            r1.push(row.r1);
        }
        ranges.push(start..p0.len());
    }
    // **#36 step 5 — the PR-2 mask survives here for PR-3 ONLY.**
    //
    // These two vectors no longer reach the kernel's distance path. They are
    // kept because `emergence_ok` needs the claimed mask, and PR-3 is not
    // optional under the honest rule but MORE necessary (design §5, reader 1):
    // at grazing incidence the whole nearby surface falls inside the band and
    // is connected, so `Z` swallows everything, nothing is left to grade, and
    // the kernel returns `INFINITY` — a pass by erasure. PR-3 is the gate that
    // refuses that member, and the mask is its yardstick.
    let mut mask_inset0 = vec![0.0; p0.len()];
    let mut mask_inset1 = vec![0.0; p0.len()];
    // PR-3 bookkeeping, row-aligned: the total mask a row's contact ends claim,
    // and whether any of them had no finite mask at all.
    let mut claimed_mask_mm = vec![0.0_f64; p0.len()];
    let mut degenerate = vec![false; p0.len()];
    let mut screened = vec![false; p0.len()];
    if !contacts.is_empty() {
        let points = contacts
            .iter()
            .map(|(row, contact)| match contact.end {
                ContactEnd::Terminalward => p0[*row],
                ContactEnd::Rootward => p1[*row],
            })
            .collect::<Vec<_>>();
        // THE one batched projection for this tournament.
        let (_, _, _, normals) = surface.project(&points);
        for ((row, contact), normal) in contacts.iter().zip(normals) {
            let axis = sub(p1[*row], p0[*row]);
            let length = norm(axis);
            screened[*row] = true;
            if length <= 0.0 {
                degenerate[*row] = true;
                continue;
            }
            let abs_cos = dot(scale(axis, 1.0 / length), normal).abs();
            let r_max = r0[*row].max(r1[*row]);
            match crate::contact::placed_mask_inset_mm(
                contact.seating,
                abs_cos,
                contact.end_radius_mm,
                r_max,
                policy,
            ) {
                Some(mask) => {
                    claimed_mask_mm[*row] += mask;
                    match contact.end {
                        ContactEnd::Terminalward => mask_inset0[*row] = mask,
                        ContactEnd::Rootward => mask_inset1[*row] = mask,
                    }
                }
                // A degenerate incidence masks NOTHING here, so the row is
                // fully distance-checked rather than silently exempted — and
                // the candidate is refused below regardless. Leaving the inset
                // at zero rather than at some large value is what makes that
                // safe if the refusal is ever dispositioned differently.
                None => degenerate[*row] = true,
            }
        }
    }
    let zero = vec![0.0; p0.len()];
    crate::census_record!(crate::census::record_segment_batch(
        "cco_candidate_clearance",
        &p0,
        &p1,
        // The insets the batch ACTUALLY receives, which are now zero.
        &zero,
        &zero,
        &[]
    ));
    // PLACEMENT. `policy.demanded_gap_mm` is this stage's registered demand,
    // read through the SAME shared predicate the final pass uses; only the
    // number differs (Craig's two-tier ruling, 2026-08-22).
    // #36 step 5 — the FULL axis against surface-minus-Z, at this stage's
    // registered demand, through the SAME shared kernel predicate the final
    // pass uses; only the number differs.
    let (clearances, _) = surface
        .segment_clearance_excluding(
            &p0,
            &p1,
            &r0,
            &r1,
            policy.demanded_gap_mm,
            &zero,
            &zero,
            &excluded,
        )
        .map_err(|error| CcoError::Native(error.to_string()))?;
    Ok(ranges
        .into_iter()
        .map(|range| {
            // Physics refusals outrank the distance verdict, because for a row
            // whose mask consumed the segment the kernel's number is `INFINITY`
            // and says nothing.
            if range.clone().any(|row| degenerate[row]) {
                return ClearanceVerdict::DegenerateIncidence;
            }
            if range.clone().any(|row| {
                screened[row] && !emergence_ok(claimed_mask_mm[row], &p0[row], &p1[row], policy)
            }) {
                return ClearanceVerdict::GrazingIncidence;
            }
            if clearances[range].iter().all(|clearance| *clearance >= 0.0) {
                ClearanceVerdict::Survives
            } else {
                ClearanceVerdict::Distance
            }
        })
        .collect())
}

/// PR-3 for one placed row — delegating the inequality to `contact::emerges` so
/// there is exactly one statement of it in the crate.
fn emergence_ok(
    claimed_mask_mm: f64,
    p0: &[f64; 3],
    p1: &[f64; 3],
    policy: &crate::contact::ContactPolicy,
) -> bool {
    crate::contact::emerges(claimed_mask_mm, norm(sub(*p1, *p0)), policy)
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

    /// The candidate this batch's tournament elects.
    ///
    /// **The commit-time TRIPWIRE that used to live here is gone, and its
    /// removal is part of PR-7.** It held a WINNING model anchor to the PR-8
    /// seat witness because `NotAttached` was deliberately not a candidacy
    /// refusal while the family was unseated — refusing it at candidacy then
    /// would have landed PR-7's family-killing consequence without landing
    /// PR-7. `cco::apply_model_anchor` now seats the rootward end, so the
    /// witness refuses at candidacy like the other two contact families
    /// (`evaluate_rewire_candidates`), exactly as this comment's predecessor
    /// predicted, and there is nothing left for a commit-time gate to catch.
    /// The two synthetic tests that reached it became candidacy-refusal tests
    /// rather than being deleted.
    pub(crate) fn winning_generation_index(&mut self) -> Option<usize> {
        self.ranked_candidates()
            .first()
            .map(|candidate| candidate.evidence.generation_index)
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
                    ..
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
        // The replay must screen under the SAME contact policy the batch was
        // evaluated with, so the run's process resolution travels on the batch
        // rather than being re-derived from a registry that does not know it.
        let contact_policy = seat_contact_policy(self.process_resolution_mm)?;
        if clearance_survival(
            geometry.surface(),
            &contact_policy,
            std::slice::from_ref(&replayed),
        )? != [ClearanceVerdict::Survives]
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

impl RewireCandidateBatch {
    /// F3 — this batch's model-anchor seat-attestation transcript.
    pub(crate) const fn seat_attestation_digest(&self) -> u64 {
        self.contact_attestation_digest
    }
}

/// **PR-7 lives here, once per seat.**
///
/// `process_resolution_mm` is the run's own max layer height, which is both the
/// PR-1(a) slicer-resolution seat minimum and the PR-2 mask MARGIN. It arrives
/// as an argument rather than from the registry because a layer height is a
/// property of THIS RUN, exactly as the seat attestation's claimed extent
/// already is.
#[allow(
    clippy::too_many_arguments,
    reason = "every argument is read; the surface and topology arrived together with #36's \
              seat-zone freeze and are the same validated pair the caller already holds"
)]
fn generate_descriptors(
    forest: &Forest,
    request: &TerminalCcoRequest,
    config: &Config,
    anchor_seats: &[QualifiedAnchorSeat],
    cached_hosts: Option<Vec<MemberKey>>,
    include_plate_spiral: bool,
    process_resolution_mm: f64,
    surface: &NativeSurface,
    topology: &FaceTopology,
) -> Result<Vec<Descriptor>, CcoError> {
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
    if !anchor_seats.is_empty() {
        let policy = seat_contact_policy(process_resolution_mm)?;
        let radius = config.min_shoot_radius_mm.get();
        // #36 — one seat-zone request per anchor seat, gathered in this loop
        // and frozen in ONE batched call below. Seat discovery already resolved
        // and SNAPPED every seat's face, so no fresh projection is needed here
        // and none is made; a per-seat band call would be the batch-first
        // defect this stage's whole pipeline is written to avoid.
        let mut zone_requests = Vec::with_capacity(anchor_seats.len());
        for (seat_index, seat) in anchor_seats.iter().enumerate() {
            let point_mm = seat.point_mm().map(Finite::get);
            let normal = seat.normal().map(Finite::get);
            // PR-1 evaluated at the UNSEATED incidence. The monotonicity lemma
            // (`contact::seat_endpoint`'s own proof) is what licenses the single
            // pass: seating raises `|cos|` and lengthens the member, so a seat
            // computed at the unseated incidence remains a valid seat at the
            // seated one. Over-seating is safe; under-seating cannot happen.
            //
            // `ball_tipped` is FALSE. PR-1(c) exists for a `lower.rs` stub end
            // emitted as a ball of radius `ball_factor * r_tip`; this end is a
            // frustum end sized by `size_tapered_without_tree_fanin`. `brace.rs`
            // passes `false` at its own seat for the same reason.
            let axis = sub(point_mm, se);
            let length = norm(axis);
            let abs_cos = if length > 0.0 {
                dot(scale(axis, 1.0 / length), normal).abs()
            } else {
                0.0
            };
            let seat_depth = crate::contact::seat_depth_mm(abs_cos, radius, &policy, false);
            let (seated_point_mm, _, _) =
                crate::contact::seat_endpoint(point_mm, normal, se, seat_depth);
            zone_requests.push(crate::seat_zone::SeatZoneRequest {
                p0_mm: se,
                p1_mm: seated_point_mm,
                seat_face_id: seat.snapped_face_id(),
                // FROZEN at the anchor's creation radius. `min_shoot_radius_mm`
                // is what `apply_model_anchor` mints the rootward (contact) end
                // with; `size_tapered_without_tree_fanin` may raise it
                // afterwards, and the band must NOT follow — a band that grew
                // with the radius would swallow more surface for a fatter arm
                // and rebuild the inversion this change exists to retire
                // (design §3).
                band_radius_mm: radius + policy.standoff_mm,
            });
            out.push(Descriptor::ModelAnchor {
                seat_index,
                point_mm,
                seated_point_mm,
                // Placeholder; every one is replaced from the batch below
                // before this function returns.
                seat_zone: std::sync::Arc::from(Vec::new()),
            });
        }
        // THE one batched band call plus parallel flood fill for this terminal.
        let frozen =
            crate::seat_zone::freeze_production_seat_zones(surface, topology, &zone_requests)
                .map_err(|error| CcoError::SeatZone(error.to_string()))?;
        // `out` ends with exactly one `ModelAnchor` per request, pushed in the
        // same order, so the tail of `out` re-pairs with `frozen` by position.
        let anchors = out.len() - frozen.len();
        for (descriptor, zone) in out[anchors..].iter_mut().zip(frozen) {
            let Descriptor::ModelAnchor { seat_zone, .. } = descriptor else {
                return Err(CcoError::InvalidGeometry(
                    "generate_descriptors: the seat-zone tail of the descriptor list is not \
                     entirely model anchors"
                        .to_owned(),
                ));
            };
            *seat_zone = zone;
        }
    }
    let hosts = cached_hosts
        .unwrap_or_else(|| HostCache::default().nearest(se, config.bifurcation_hosts, forest));
    out.extend(hosts.into_iter().map(|host| Descriptor::Bifurcate { host }));
    Ok(out)
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
        return Err(CcoError::InvalidGeometry(format!(
            "evaluate: candidate price is not finite (price_ml = {price_ml}) for family \
             {:?}, generation {generation_index}",
            descriptor.family()
        )));
    }
    let rows = clearance_rows(&candidate, snapshot);
    let anchor = if let Descriptor::ModelAnchor { seat_index, .. } = descriptor {
        let parent = candidate
            .parent_of(request.stub)
            .and_then(|key| candidate.member(key))
            .ok_or_else(|| {
                CcoError::InvalidGeometry(format!(
                    "evaluate: model-anchor candidate (seat index {seat_index}) stub \
                     {:?} has no live parent member in the candidate forest",
                    request.stub
                ))
            })?;
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
        Descriptor::ModelAnchor {
            seated_point_mm,
            seat_zone,
            ..
        } => {
            if !apply_model_anchor(candidate, request, *seated_point_mm, seat_zone, config)? {
                return Ok(false);
            }
            let strut = candidate.parent_of(request.stub).ok_or_else(|| {
                CcoError::InvalidGeometry(format!(
                    "apply_and_size: model-anchor stub {:?} has no parent member after \
                     apply_model_anchor placed it",
                    request.stub
                ))
            })?;
            size_tapered_without_tree_fanin(candidate, strut)?;
        }
    }
    Ok(true)
}

/// **F3 — PR-8/OD-9 at the CCO model anchor's SEAT.**
///
/// The anchor family is decided by a tournament, so the only honest moment to
/// attest it is while it is still a candidate — after the foot ball has
/// validated (so the row set is exactly the seats that could still win) and
/// before any winner is committed. One batched pair of native calls per
/// terminal, riding the same stage boundary as the foot `sphere_breach` batch
/// immediately above it. The anchor family's whole pipeline is already
/// per-terminal-batched (one `cast_rays` at discovery, one `sphere_breach` at
/// the foot); this keeps that granularity rather than adding a per-candidate
/// native call.
///
/// The provisional member is the strut `apply_model_anchor` WOULD mint:
/// terminalward at the stub end with the CCO terminal radius, rootward at the
/// seat with the FINAL sized radius, `model_contact_end = Rootward`, carrying
/// the terminal's real routed load. That load is what makes PR-4's area law
/// live for this family (MAJOR-7) — unlike a brace rung, a model anchor IS a
/// demand route, so it has an area demand to grade.
///
/// **Disposition — read this before changing it. PR-7 CHANGED IT.**
///
/// All three verdicts now refuse at CANDIDACY, through the existing
/// [`AnchorFootOutcome::Refused`] channel: the candidate leaves the tournament,
/// the terminal routes some other way, nothing hard-fails (OD-4/PR-9). The
/// mapping is [`SeatAttestationRefusalKind`], stated once and used by both the
/// terminal-growth and the rewire path.
///
/// * **OD-9 `BuriedSurface`** always refused here. Burial is a statement about
///   the SURFACE and is true whatever the seating rule is.
/// * **FU-88 `UnusableGeometry`** always refused here. It is not a verdict
///   about the seating rule at all — it says this candidate is not a printable
///   strut.
/// * **PR-8 `NotAttached` USED to be recorded here and enforced at commit**, by
///   `elect_committable_winner` on the terminal-growth path and
///   `RewireCandidateBatch::winning_generation_index` on the rewire path. The
///   reason was explicit and it was correct at the time: this family was
///   deliberately UNSEATED — `apply_model_anchor` set `p1` to the seat point
///   exactly and the anchor KISSED the surface — so `NotAttached` was a
///   statement about a rule that had not been landed, and refusing it at
///   candidacy would have delivered PR-7's family-killing consequence without
///   delivering PR-7. That is the accidental physics change OD-8 forbids.
///
///   The measurement it produced instead was not decorative, and it is what
///   licensed PR-7: on `make_overhang_table` all 1,498 attested anchor seats
///   reported `centreline = false` with 1-3 of 8 rim rays engaged — the
///   unseated-kiss signature, quantified — plus 19 PR-4 area deficits once
///   MAJOR-7 gave this family its real routed load.
///
///   `apply_model_anchor` now SEATS the rootward end (PR-1 depth along the
///   seat's own normal, `contact::seat_endpoint`, the same two functions the
///   brace model rung uses). `NotAttached` therefore stops being a statement
///   about a missing rule and becomes an ordinary defect — this seat does not
///   reach the solid — so it refuses at candidacy like the other two contact
///   families, and both commit-time tripwires came out in the same commit.
///
/// A PR-4 area deficit keeps its own disposition: measured, digested, referred
/// to the owner. It is not refused here.
///
/// The brace model rung was always dispositioned this way — both verdicts drop
/// the candidate — because PR-1 seats it. The corpus agrees: both surviving
/// rungs attest at 8/8 rim rays plus centreline. PR-7 makes the CCO anchor's
/// disposition the same as the rung's, which is the point.
fn attest_model_anchor_seats(
    surface: &NativeSurface,
    stub_end_mm: [f64; 3],
    carried_mm2: f64,
    terminal_radius_mm: f64,
    process_resolution_mm: f64,
    anchors: &[&ValidatedModelAnchor],
    seated_points_mm: &[[f64; 3]],
) -> Result<ModelAnchorSeatAttestation, CcoError> {
    if anchors.len() != seated_points_mm.len() {
        return Err(CcoError::InvalidGeometry(format!(
            "attest_model_anchor_seats: {} anchor(s) but {} seated endpoint(s); the \
             witness must measure the geometry `apply_model_anchor` will actually mint",
            anchors.len(),
            seated_points_mm.len()
        )));
    }
    let extent =
        crate::designed_embed::PositiveFiniteMm::new(process_resolution_mm).map_err(|error| {
            CcoError::InvalidGeometry(format!(
                "attest_model_anchor_seats: process resolution {process_resolution_mm} mm \
                 is not a positive-finite attestation extent: {error}"
            ))
        })?;
    if anchors.is_empty() {
        return Ok(ModelAnchorSeatAttestation {
            refusals: Vec::new(),
            digest: crate::designed_embed::seat_attestation_digest(
                crate::designed_embed::SEAT_TAG_CCO_MODEL_ANCHOR,
                extent,
                &[],
                &[],
            ),
        });
    }
    let seats = anchors
        .iter()
        .zip(seated_points_mm)
        .map(|(anchor, seated)| {
            let seat = anchor.seat().point_mm().map(Finite::get);
            let normal = anchor.seat().normal().map(Finite::get);
            crate::designed_embed::SeatAttestationRow {
                p0_mm: stub_end_mm,
                // **PR-7.** The witness measures the endpoint the strut will
                // actually be built at — the PR-1 SEAT — not the raw surface
                // point. Measuring the raw point is what made every attested
                // seat on the corpus report `centreline = false` with 1-3 of 8
                // rim rays: the witness was correctly reporting a kiss, because
                // a kiss is what the geometry was. The seated point travels on
                // the descriptor from `generate_descriptors`, so the witness and
                // `apply_model_anchor` cannot measure different geometry.
                p1_mm: *seated,
                r0_mm: terminal_radius_mm,
                r1_mm: anchor.foot().final_member_radius_mm().get(),
                end: ContactEnd::Rootward,
                // The snapped face normal is the OUTWARD sense: seat discovery
                // keeps only seats whose first hit is an EXTERIOR face, and a
                // `NativeSurface` normal points out of the solid.
                exterior_probe: std::array::from_fn(|axis| {
                    seat[axis] + normal[axis] * process_resolution_mm
                }),
                carried_mm2,
            }
        })
        .collect::<Vec<_>>();
    let majority_step_mm = extent.get() / 10.0;
    let majority_step =
        crate::designed_embed::PositiveFiniteMm::new(majority_step_mm).map_err(|error| {
            CcoError::InvalidGeometry(format!(
                "attest_model_anchor_seats: majority-vote sampling step {majority_step_mm} mm \
                 (one tenth of extent {} mm) is not positive-finite: {error}",
                extent.get()
            ))
        })?;
    let seat_count = seats.len();
    let limits = crate::designed_embed::EndpointMaskValidationLimits::for_batch(
        surface,
        seat_count,
        majority_step,
        MODEL_ANCHOR_ATTESTATION_SAMPLES_PER_SUBSEGMENT,
    )
    .map_err(|error| {
        CcoError::InvalidGeometry(format!(
            "attest_model_anchor_seats: endpoint-mask validation limits for {seat_count} \
             seat(s) at extent {} mm are invalid: {error}",
            extent.get()
        ))
    })?;
    let policy = seat_contact_policy(process_resolution_mm)?;
    // NOT `InvalidGeometry`. That variant reads "CCO input or derived geometry
    // is not finite and positive where required", and no batch-level refusal
    // this call can return means that: the numbers are finite, positive and
    // this model's own. `brace.rs` already routes the identical error to
    // `BraceError::NativeQuery`; this site was the outlier. #31 owns giving
    // these failures a topology name; this is the honest channel until then.
    let outcomes =
        crate::designed_embed::attest_seats_batch(surface, &seats, extent, limits, &policy)
            .map_err(|error| {
                CcoError::Native(format!(
                    "attest_model_anchor_seats: seat-attestation batch over {seat_count} \
                 seat(s) at extent {} mm failed: {error}",
                    extent.get()
                ))
            })?;
    let mut refusals = vec![None; seats.len()];
    for outcome in &outcomes {
        if outcome.member >= seats.len() {
            return Err(CcoError::InvalidGeometry(format!(
                "attest_model_anchor_seats: native batch returned outcome for member index \
                 {} but only {seat_count} seat(s) were sent",
                outcome.member
            )));
        }
        refusals[outcome.member] = outcome.verdict.as_ref().err().copied();
        crate::census_record!(crate::census::record_seat_attestation(
            "cco_model_anchor",
            outcome.engagement.centreline,
            outcome.engagement.rim_engaged,
            outcome.engagement.rim_total,
            outcome.attested_radius_mm,
            outcome.verdict.is_err(),
            outcome.area_deficit.is_some(),
        ));
    }
    Ok(ModelAnchorSeatAttestation {
        refusals,
        digest: crate::designed_embed::seat_attestation_digest(
            crate::designed_embed::SEAT_TAG_CCO_MODEL_ANCHOR,
            extent,
            &seats,
            &outcomes,
        ),
    })
}

/// The PR-7 seated endpoints of a set of model-anchor generations, read back
/// from the descriptors that minted them.
///
/// Reading them from the descriptor rather than re-deriving them is what makes
/// the witness and `apply_model_anchor` measure the SAME point: the descriptor
/// is the single place the seat is computed (`generate_descriptors`), and both
/// consumers read it.
fn seated_points_of(
    descriptors: &[Descriptor],
    generation_indices: &[usize],
) -> Result<Vec<[f64; 3]>, CcoError> {
    generation_indices
        .iter()
        .map(|index| match descriptors.get(*index) {
            Some(Descriptor::ModelAnchor {
                seated_point_mm, ..
            }) => Ok(*seated_point_mm),
            _ => Err(CcoError::InvalidGeometry(format!(
                "seated_points_of: generation {index} carries a validated model anchor \
                 but its descriptor is not a model anchor"
            ))),
        })
        .collect()
}

/// The three seat-attestation verdicts that refuse a model-anchor candidate,
/// with the single mapping from verdict to refusal reason.
///
/// **All three refuse at CANDIDACY since PR-7.** The mapping lives in one place
/// so the terminal-growth path and the rewire path cannot disagree about which
/// verdict means what — they used to differ, because `NotAttached` was enforced
/// at two different commit points while burial refused at candidacy in both.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeatAttestationRefusalKind {
    /// OD-9 — the surface this anchor proposes to attach to is buried.
    Buried,
    /// FU-88 — the candidate's own geometry could not carry the attestation.
    Unusable,
    /// PR-8 — no sampled part of the seated end reached the solid.
    NotAttached,
}

impl SeatAttestationRefusalKind {
    fn refusal(self, provenance: AnchorCandidateProvenance) -> AnchorFootRefusal {
        match self {
            Self::Buried => AnchorFootRefusal::seat_buried(provenance),
            Self::Unusable => AnchorFootRefusal::seat_geometry_unusable(provenance),
            Self::NotAttached => AnchorFootRefusal::seat_not_attached(provenance),
        }
    }

    const fn reason(self) -> AnchorFootRefusalReason {
        match self {
            Self::Buried => AnchorFootRefusalReason::SeatBuriedSurface,
            Self::Unusable => AnchorFootRefusalReason::SeatGeometryUnusable,
            Self::NotAttached => AnchorFootRefusalReason::SeatNotAttached,
        }
    }
}

/// One terminal's (or one rewire batch's) model-anchor seat attestation.
struct ModelAnchorSeatAttestation {
    /// Row-aligned with the anchors that were attested: `Some` where the seat
    /// was refused.
    refusals: Vec<Option<crate::designed_embed::ContactAttestationRefusal>>,
    digest: u64,
}

/// Matches `designed_embed::ENDPOINT_VALIDATION_OFFSET_RAYS`, for the same
/// reason `initial.rs` and `brace.rs` match it: the witness needs the eight rim
/// rays and the centreline resolved at the density it samples them.
const MODEL_ANCHOR_ATTESTATION_SAMPLES_PER_SUBSEGMENT: usize = 8;

/// The PR-1..PR-4 policy the CCO stage attests its seats under. Every number is
/// the registered one; only the process resolution is a property of the run,
/// which is why it arrives on the anchor route rather than from the registry.
fn seat_contact_policy(
    process_resolution_mm: f64,
) -> Result<crate::contact::ContactPolicy, CcoError> {
    let constants = m0_constants().map_err(|error| CcoError::Constants(error.to_string()))?;
    let p = constants.placeholders();
    Ok(crate::contact::ContactPolicy {
        floor_depth_mm: p.default_contact_depth_mm().diagnostic().get(),
        process_resolution_mm,
        standoff_mm: p.standoff_mm().diagnostic().get(),
        // PLACEMENT. `clearance_survival` grades a candidate tournament: the
        // demand PRUNES a search and refuses nothing final, so it reads
        // `STANDOFF_MM` and not the refusing `FINAL_CLEARANCE_GAP_MM`.
        demanded_gap_mm: p.standoff_mm().diagnostic().get(),
        min_free_span_mm: p.min_illegal_len_mm().diagnostic().get(),
        ball_factor_stub: p.ball_factor_stub().diagnostic().get(),
        tip_load: crate::perception::FrozenTipLoadPolicy::from_placeholders(p),
    })
}

/// **PR-7 `CCO_ANCHOR_MASK_AT_THE_SEAT` (design §3.1, Piece A).**
///
/// The strut's rootward endpoint is the PR-1 SEAT, not the raw surface point.
/// Before this, `p1_mm` was `point_mm` exactly — the anchor kissed the surface,
/// every one of the 3,186 attested seats on the corpus reported
/// `centreline = false` with 1-3 of 8 rim rays engaged, and the seat witness
/// could only be enforced at commit because refusing an unseated kiss at
/// candidacy would have killed the family without landing the seating rule.
/// Now the endpoint is pushed inward along the surface normal by
/// `contact::seat_depth_mm`, exactly as the brace model rung's is, through the
/// same two functions — so the family that CREATES a seated contact and
/// `contact::seating_of`, which RECOGNISES one, cannot drift.
///
/// `seated_point_mm` is computed ONCE per seat in [`generate_descriptors`],
/// where the qualified seat's snapped face normal and the run's own process
/// resolution both are, and travels on the descriptor. That placement is not
/// cosmetic:
///
/// * the descriptor is what the rewire batch STORES and what
///   `RewireCandidateBatch::replay` re-evaluates, so the seated point is a
///   frozen input to the replay rather than a quantity re-derived from a
///   resolution the replay would have to carry separately; and
/// * seat discovery already resolved the normal for every seat it kept, so no
///   fresh projection is needed here — re-projecting per candidate inside the
///   tournament would be the batch-first defect.
///
/// The `within_cone` screen is evaluated on the SEATED axis, because that is
/// the axis that will be printed. By the PR-1 monotonicity lemma seating can
/// only lengthen the member and raise its incidence against the surface normal;
/// it carries no such guarantee for the LEAN limit, so the screen is
/// re-evaluated rather than assumed.
fn apply_model_anchor(
    forest: &mut Forest,
    request: &TerminalCcoRequest,
    seated_point_mm: [f64; 3],
    seat_zone: &std::sync::Arc<[u32]>,
    config: &Config,
) -> Result<bool, CcoError> {
    let stub_end = request.stub_end_mm().map(Finite::get);
    let seated_point = seated_point_mm;
    if norm(sub(seated_point, stub_end)) <= DEGENERATE_MM
        || !within_cone(
            stub_end,
            seated_point,
            config.min_shoot_radius_mm.get(),
            config,
        )
    {
        return Ok(false);
    }
    crate::census_record!(crate::census::record_model_anchor(stub_end, seated_point));
    let strut = forest.add_member(Member::new(MemberSpec {
        p0_mm: request.stub_end_mm(),
        p1_mm: finite3(seated_point)?,
        r0_mm: TerminalEndRadius::new(cco_terminal_radius(request, config)?),
        r1_mm: RootwardEndRadius::new(config.min_shoot_radius_mm),
        kind: MemberKind::Strut,
        owner: request.connector_owner.clone(),
        model_contact_end: Some(ContactEnd::Rootward),
        carried_mm2: request.carried_mm2(),
    }));
    // Option A1's belt: `contact::seating_of` keys the seated CCO family on
    // `MemberKind::Strut` + `ContactEnd::Rootward`, which is only unambiguous
    // while this is the sole production mint of that pair.
    debug_assert!(
        forest.member(strut).is_some_and(|member| {
            matches!(member.kind(), MemberKind::Strut)
                && member.model_contact_end() == Some(ContactEnd::Rootward)
        }),
        "apply_model_anchor is the only production mint of a Rootward model \
         contact on a Strut; contact::seating_of keys PR-7 on that pair"
    );
    // **#36 creation site 3 of 4 — the freeze, at the mint.**
    //
    // The zone was computed once per seat in `generate_descriptors`, from THIS
    // anchor's creation radius and its own snapped seat face, and travels on
    // the descriptor. Installing it is an `Arc` clone: no native call, nothing
    // recomputed, and bit-identical between the candidate application and the
    // winner's replay against the live forest — which matters, because the
    // commit path re-runs `apply_and_size` on the live forest and throws the
    // candidate forest away. A zone derived from the forest being written into
    // could differ between those two runs; one carried on the descriptor
    // cannot.
    forest.set_seat_zone(strut, std::sync::Arc::clone(seat_zone));
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

/// OD-7 / PR-6 — which model-contact declaration the UPPER half of a
/// bifurcated host inherits.
///
/// `apply_bifurcation` splits a host at `junction` into a lower half
/// (`p0..junction`, which keeps the TERMINALWARD end) and an `upper` half
/// (`junction..p1`, which keeps the ROOTWARD end). The contact declaration
/// follows the end that PHYSICALLY owns the contact, so `upper` inherits only
/// a `Rootward` declaration.
///
/// `upper.p0` is the junction: a mid-air point on the host's own axis. It is
/// not on the model surface and carries no contact. Inheriting the host's
/// declaration verbatim -- as this did before Phase B-2 -- asserted a
/// Terminalward contact on BOTH halves, and `upper`'s claim was simply false.
///
/// Every downstream consumer then acts on that falsehood: `exact_clearance_rows`
/// projects `upper.p0` onto the surface and synthesises a mask from a normal at
/// a point the member never touches; `lower::is_embedded_taper` classifies
/// `upper` as a ContactTaper and stamps a contact depth into its wire record;
/// and the endpoint witness demands a connectivity proof for an endpoint
/// nowhere near the solid, turning a silent falsehood into the refusal of a
/// legitimate member.
///
/// The bug is latent rather than live on today's corpus: the only members
/// carrying `Terminalward` are initial contact stubs, and stubs are excluded
/// from bifurcation hosts by `HostCache::rebuild`.
const fn inherited_upper_contact_end(host: Option<ContactEnd>) -> Option<ContactEnd> {
    match host {
        Some(ContactEnd::Rootward) => Some(ContactEnd::Rootward),
        // The lower half keeps a Terminalward contact; the upper half has none.
        Some(ContactEnd::Terminalward) | None => None,
    }
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
        model_contact_end: inherited_upper_contact_end(host.model_contact_end()),
        carried_mm2: host.carried_mm2(),
    }));
    forest.set_parent(upper, old_parent)?;
    forest.set_endpoints(host_key, host.p0_mm(), finite3(junction)?)?;
    forest.set_radii(
        host_key,
        host.terminal_radius_mm(),
        RootwardEndRadius::new(interpolated),
    )?;
    if host.model_contact_end() == Some(ContactEnd::Rootward) {
        forest.set_model_contact_end(host_key, None)?;
        // **#36 creation site 4 of 4 — INHERIT, never recompute.**
        //
        // `upper` keeps the host's `p1`, which is the host's seat: it is the
        // SAME contact, not a new one, so it keeps the same frozen zone. Taking
        // it moves the entry rather than copying it, which is what the rule
        // requires in both directions — the upper half must forgive exactly
        // what the contact always forgave, and the lower half, which no longer
        // declares a contact, must forgive nothing.
        //
        // An `Arc` move. No native call, and emphatically no re-freeze from
        // `upper`'s own interpolated radius: that would be the recompute the
        // monotonicity theorem forbids (design §3).
        if let Some(zone) = forest.take_seat_zone(host_key) {
            forest.set_seat_zone(upper, zone);
        }
    }
    // The split partitions the declaration; it never duplicates it. `host_key`
    // keeps a Terminalward contact (it still owns p0), `upper` keeps a
    // Rootward one (it still owns p1), and neither keeps the other's.
    debug_assert!(
        !(forest
            .member(host_key)
            .is_some_and(|lower| lower.model_contact_end().is_some())
            && forest
                .member(upper)
                .is_some_and(|member| member.model_contact_end().is_some())),
        "a bifurcation must not leave both halves declaring a model contact"
    );
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
    let first = forest.parent_of(request.stub).ok_or_else(|| {
        CcoError::InvalidGeometry(format!(
            "size_plate_candidate: stub {:?} has no parent member after plate descent \
             was applied",
            request.stub
        ))
    })?;
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
    let column = forest.parent_of(diagonal).ok_or_else(|| {
        CcoError::InvalidGeometry(format!(
            "size_plate_candidate: diagonal member {diagonal:?} has no parent (column) \
             after plate descent was applied"
        ))
    })?;
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
    let upper = forest.parent_of(host).ok_or_else(|| {
        CcoError::InvalidGeometry(format!(
            "size_bifurcation_candidate: host member {host:?} has no parent (upper) after \
             bifurcation was applied"
        ))
    })?;
    let child = forest.parent_of(request.stub).ok_or_else(|| {
        CcoError::InvalidGeometry(format!(
            "size_bifurcation_candidate: stub {:?} has no parent (child) after \
             bifurcation was applied",
            request.stub
        ))
    })?;

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
        let no_fanin_mm = no_fanin.get();
        let radius = PositiveFinite::new(murray.max(no_fanin_mm)).map_err(|error| {
            CcoError::InvalidGeometry(format!(
                "uniformly_size_ancestor_path: Murray's-law radius blend at member {key:?} \
                 is not positive-finite (murray-law radius {murray} mm, minimum radius \
                 {no_fanin_mm} mm): {error}"
            ))
        })?;
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
        let existing_carried_mm2 = member.carried_mm2().get();
        let added_mm2 = added.get();
        let carried = Finite::new(existing_carried_mm2 + added_mm2).map_err(|error| {
            CcoError::InvalidGeometry(format!(
                "propagate_carried: adding carried area at member {key:?} (existing \
                 {existing_carried_mm2} mm2 plus added {added_mm2} mm2) produced a \
                 non-finite value: {error}"
            ))
        })?;
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
        model_contact_end: None,
        carried_mm2: request.carried_mm2(),
    }))
}

fn cco_terminal_radius(
    request: &TerminalCcoRequest,
    config: &Config,
) -> Result<PositiveFinite, CcoError> {
    let tip_radius_mm = request.tip_radius_mm().get();
    let min_shoot_radius_mm = config.min_shoot_radius_mm.get();
    PositiveFinite::new(tip_radius_mm.max(min_shoot_radius_mm)).map_err(|error| {
        CcoError::InvalidGeometry(format!(
            "cco_terminal_radius: tip radius {tip_radius_mm} mm blended with minimum shoot \
             radius {min_shoot_radius_mm} mm is not positive-finite: {error}"
        ))
    })
}

fn clearance_rows(forest: &Forest, snapshot: &Forest) -> Vec<ClearanceRow> {
    forest
        .live_tree_members()
        .into_iter()
        .filter_map(|key| {
            let member = forest.member(key)?;
            (snapshot.member(key) != Some(member)).then_some((key, member))
        })
        .map(|(key, member)| ClearanceRow {
            seat_zone: member
                .model_contact_end()
                .and_then(|_| forest.seat_zone(key))
                .map(std::sync::Arc::clone),
            p0: member.p0_mm().map(Finite::get),
            p1: member.p1_mm().map(Finite::get),
            r0: member.terminal_radius_mm().get().get(),
            r1: member.rootward_radius_mm().get().get(),
            contact: member.model_contact_end().map(|end| ClearanceContact {
                end,
                seating: crate::contact::seating_of(member.kind(), end),
                end_radius_mm: match end {
                    ContactEnd::Terminalward => member.terminal_radius_mm().get().get(),
                    ContactEnd::Rootward => member.rootward_radius_mm().get().get(),
                },
            }),
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
                .ok_or_else(|| {
                    CcoError::InvalidGeometry(format!(
                        "candidate_price: model-anchor stub {:?} has no live parent \
                         member to price",
                        request.stub
                    ))
                })?;
            return member_volume(strut);
        }
        Descriptor::Bifurcate { .. } => return Ok(ordinary_delta_ml),
        Descriptor::Plate { .. } => {}
    }
    let first = candidate.parent_of(request.stub).ok_or_else(|| {
        CcoError::InvalidGeometry(format!(
            "candidate_price: plate-descent stub {:?} has no parent member to price",
            request.stub
        ))
    })?;
    let (diagonal, column) = if candidate
        .member(first)
        .is_some_and(|member| member.p1_mm()[2].get() == request.plate_z_mm().get())
    {
        (None, first)
    } else {
        (
            Some(first),
            candidate.parent_of(first).ok_or_else(|| {
                CcoError::InvalidGeometry(format!(
                    "candidate_price: plate-descent diagonal member {first:?} has no \
                     parent (column) to price"
                ))
            })?,
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
            Finite::new(0.0).map_err(|error| {
                CcoError::InvalidGeometry(format!(
                    "candidate_price: brace-rod pricing constant carried area 0.0 mm2 \
                     failed Finite::new, which should be unreachable: {error}"
                ))
            })?,
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
        model_contact_end: None,
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
    volume.is_finite().then_some(volume).ok_or_else(|| {
        CcoError::InvalidGeometry(format!(
            "member_volume: computed volume is not finite (volume_ml = {volume}) for a \
             member of length {length} mm, terminal radius {r0} mm, rootward radius \
             {r1} mm"
        ))
    })
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
    let interpolated = r0 + t * (r1 - r0);
    PositiveFinite::new(interpolated).map_err(|error| {
        CcoError::InvalidGeometry(format!(
            "radius_along: radius interpolated at t={t} between terminal radius {r0} mm \
             and rootward radius {r1} mm is not positive-finite (interpolated \
             {interpolated} mm): {error}"
        ))
    })
}

fn finite3(values: [f64; 3]) -> Result<[Finite; 3], CcoError> {
    values
        .map(Finite::new)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CcoError::InvalidGeometry(format!(
                "finite3: point {values:?} has a non-finite coordinate: {error}"
            ))
        })?
        .try_into()
        .map_err(|_: Vec<Finite>| {
            CcoError::InvalidGeometry(format!(
                "finite3: converting the validated 3-coordinate Vec back to [Finite; 3] \
                 for point {values:?} failed length invariant, which should be unreachable"
            ))
        })
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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
            SpecializedLimits::production(TEST_KNN_LIMITS, crate::specialized::test_batch_work()),
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
        route_initial_build_steered(build, plan, loads, geometry, delicacy, frame, None)
            .expect("an unsteered cadence never reports its steering spent")
    }

    /// The same cadence, driven with a Stage B steering set.
    ///
    /// `Err(terminal)` is [`CcoError::SteeringSpent`] — every arm that terminal
    /// could still take carries a no-good, so the pass stops for a confirmation
    /// round instead of retiring a real contact on a count of refusals.
    fn route_initial_build_steered(
        build: &mut InitialContactBuild,
        plan: &SpecializedContactPlan,
        loads: &FinalizedContactLoads,
        geometry: &AnchorGeometryAuthority<'_>,
        delicacy: &[f64],
        frame: BuildFrame,
        steering: Option<&crate::legalization::LegalizationSteering>,
    ) -> Result<CompletedInitialRoutes, u32> {
        let pool = SurfaceAnchorPool::from_geometry(geometry, delicacy).unwrap();
        let authority = VerifiedRoutingAuthority::compute(plan, loads, &pool, geometry).unwrap();
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
                        authority: &authority,
                        contact,
                        contact_id: u64::from(*terminal),
                        config: AnchorSearchConfig::current().unwrap(),
                        process_resolution_mm: 0.05,
                    },
                    steering,
                )
                .map_err(|error| match error {
                    CcoError::SteeringSpent { terminal_id } => terminal_id,
                    other => panic!("steered cadence refused unexpectedly: {other}"),
                })?;
            outcomes.push((*terminal, outcome));
        }
        Ok((
            controller,
            expected,
            routed_request,
            skipped_ledger,
            outcomes,
        ))
    }

    /// FU-32 phase J1 — the hoisted routing authority is load-bearing, arm by
    /// arm, and the routing path still refuses a stale proof.
    ///
    /// # Why this test has to exist
    ///
    /// J1 removed the per-terminal re-hashing of `plan`, `loads`, `pool` and
    /// `geometry` and replaced it with digests hoisted once into
    /// [`VerifiedRoutingAuthority`].  If the sites that read those hoisted
    /// values had been left comparing the token against *itself*, the
    /// comparisons would have degraded into `field == field` — a guarantee
    /// regression dressed as a speed-up, exactly what the G2 record and FU-32
    /// design §F6 warn about.  So every site compares the hoisted value
    /// against the subject's own independently sealed witness, and this test
    /// corrupts the hoist — leaving every subject untouched — to prove each of
    /// those comparisons still bites.
    ///
    /// # The arms, and the site each one names
    ///
    /// | corrupted hoist | site that must refuse | error |
    /// |---|---|---|
    /// | `pool_digest` | `SurfaceAnchorPool::validate_against_hoisted`, reached from `validate_finalized_anchor_route` | `Anchor(StaleSurfaceAnchorPool)` |
    /// | `load_rows_fingerprint` | `FinalizedContactLoads::validate_binding_hoisted`, third arm | `FinalizedLoadBindingMismatch` |
    /// | `plan_digest` | `bind`, first arm, against `plan.finalized_load_plan_digest()` | `StaleFinalizedAnchorAuthority` |
    /// | `loads_digest` | `bind`, second arm, against `loads.digest()` | `StaleFinalizedAnchorAuthority` |
    /// | `geometry_fingerprint` | `bind`, fourth arm, against `geometry.fingerprint_ref()` | `StaleFinalizedAnchorAuthority` |
    ///
    /// Three distinct error types across the five arms, so no arm is satisfied
    /// by a delegate's refusal in the way `36d1a63` had to correct for G2.
    ///
    /// # Mutation transcript (2026-08-09, run by hand at this tip)
    ///
    /// Nine comparisons were deleted one at a time and the `fu32_*` tests
    /// re-run.  Every deletion failed a test, and every failure named the
    /// deleted arm:
    ///
    /// | deleted comparison | failing assertion |
    /// |---|---|
    /// | `SurfaceAnchorPool::validate_against_hoisted` content arm | this test's `PoolDigest` arm **and** `anchor::tests::surface_anchor_pool_is_exterior_face_ordered_…` |
    /// | `FinalizedContactLoads::validate_binding_hoisted` rows arm | this test's `LoadRowsFingerprint` arm **and** `perception::tests::finalized_contact_loads_fold_frozen_rows_…` |
    /// | `bind` arm 1 (plan) | "bind accepted a corrupted PlanDigest hoist" |
    /// | `bind` arm 2 (loads) | "bind accepted a corrupted LoadsDigest hoist" |
    /// | `bind` arm 4 (geometry) | "bind accepted a corrupted GeometryFingerprint hoist" |
    /// | `validate_replay` arms 0–3 | "validate_replay arm {0,1,2,3} stopped refusing a stale authority" |
    ///
    /// **The delegate observation, recorded rather than hidden.** Deleting any
    /// of `bind`'s three arms leaves
    /// [`fu32_a_corrupted_hoisted_authority_never_reaches_a_forest_mutation`]
    /// **green**, because that fixture's terminal takes the no-winner arm and
    /// `validate_replay` refuses on the same proof.  That is precisely the
    /// `36d1a63` vacuity shape, and it is why this test calls `bind` directly:
    /// so each of its arms can be satisfied only by the site that names it.
    ///
    /// # Why `bind` is exercised directly, and not only through the route
    ///
    /// The `36d1a63` vacuity lesson, met head-on.  On the **no-winner** arm
    /// `grow_terminal_with_finalized_anchor` calls `validate_replay`, whose
    /// quartet refuses on the same proof as `bind`'s — so deleting `bind`'s
    /// plan/loads/geometry arm and running only the end-to-end test leaves it
    /// **green**, satisfied by a delegate.  `bind`'s arms are the *only* gate
    /// on the winner arm, where no replay happens.  So the arm-naming test
    /// below calls `FinalizedAnchorValidationContext::bind` itself, and the
    /// end-to-end test in
    /// [`fu32_a_corrupted_hoisted_authority_never_reaches_a_forest_mutation`]
    /// separately proves the production path refuses before any mutation.
    ///
    /// Every corrupted arm must also leave the forest untouched: these are
    /// pre-mutation authority gates, so a refusal that had already grown a
    /// member would be a different defect.
    #[test]
    fn fu32_bind_refuses_a_corrupted_hoisted_routing_authority_arm_by_arm() {
        let (build, plan, loads, surface, topology, mesh, delicacy) =
            real_no_winner_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let pool = SurfaceAnchorPool::from_geometry(&geometry, &delicacy).unwrap();
        let frame = no_winner_frame();
        let terminal = build
            .routing_order()
            .specialized_terminals()
            .iter()
            .chain(build.routing_order().regular_terminals())
            .copied()
            .find(|terminal| {
                build.forest().terminal_ledger(*terminal).unwrap().state
                    != TerminalLedgerState::Quarantined
            })
            .expect("the fixture routes at least one live terminal");
        let request = TerminalCcoRequest::bind(build.forest(), terminal, frame).unwrap();

        for (field, expected) in corrupted_authority_arms() {
            let mut corrupted =
                VerifiedRoutingAuthority::compute(&plan, &loads, &pool, &geometry).unwrap();
            let contact = corrupted.contact(u64::from(terminal)).unwrap();
            corrupted.corrupt_for_test(field);
            let error = FinalizedAnchorValidationContext::bind(
                build.forest(),
                &request,
                FinalizedAnchorTerminalRoute {
                    authority: &corrupted,
                    contact,
                    contact_id: u64::from(terminal),
                    config: AnchorSearchConfig::current().unwrap(),
                    process_resolution_mm: 0.05,
                },
            )
            .err()
            .unwrap_or_else(|| panic!("bind accepted a corrupted {field:?} hoist"));
            assert!(
                expected(&error),
                "bind refused a corrupted {field:?} at the wrong site: {error}"
            );
        }

        let honest = VerifiedRoutingAuthority::compute(&plan, &loads, &pool, &geometry).unwrap();
        let contact = honest.contact(u64::from(terminal)).unwrap();
        FinalizedAnchorValidationContext::bind(
            build.forest(),
            &request,
            FinalizedAnchorTerminalRoute {
                authority: &honest,
                contact,
                contact_id: u64::from(terminal),
                config: AnchorSearchConfig::current().unwrap(),
                process_resolution_mm: 0.05,
            },
        )
        .expect("an honest hoisted authority binds exactly as it did before J1");
    }

    /// One corruption arm: the hoisted field to move, and the predicate the
    /// resulting refusal must satisfy.  FU-32 J1.
    type CorruptedAuthorityArm = (CorruptedAuthorityField, fn(&CcoError) -> bool);

    /// The five corruption arms and the error each must produce.  Shared by
    /// the `bind` arm-naming test and the end-to-end path test so the two
    /// cannot drift.  FU-32 J1.
    fn corrupted_authority_arms() -> [CorruptedAuthorityArm; 5] {
        [
            (CorruptedAuthorityField::PoolDigest, |error| {
                matches!(
                    error,
                    CcoError::Anchor(AnchorSearchError::StaleSurfaceAnchorPool)
                )
            }),
            (CorruptedAuthorityField::LoadRowsFingerprint, |error| {
                matches!(error, CcoError::FinalizedLoadBindingMismatch)
            }),
            (CorruptedAuthorityField::PlanDigest, |error| {
                matches!(error, CcoError::StaleFinalizedAnchorAuthority)
            }),
            (CorruptedAuthorityField::LoadsDigest, |error| {
                matches!(error, CcoError::StaleFinalizedAnchorAuthority)
            }),
            (CorruptedAuthorityField::GeometryFingerprint, |error| {
                matches!(error, CcoError::StaleFinalizedAnchorAuthority)
            }),
        ]
    }

    /// FU-32 phase J1 — the production routing path refuses a corrupted hoist
    /// before it touches the forest.  See
    /// [`fu32_bind_refuses_a_corrupted_hoisted_routing_authority_arm_by_arm`]
    /// for the arm-by-arm mutation transcript this complements.
    #[test]
    fn fu32_a_corrupted_hoisted_authority_never_reaches_a_forest_mutation() {
        let (mut build, plan, loads, surface, topology, mesh, delicacy) =
            real_no_winner_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let pool = SurfaceAnchorPool::from_geometry(&geometry, &delicacy).unwrap();
        let frame = no_winner_frame();

        let terminal = build
            .routing_order()
            .specialized_terminals()
            .iter()
            .chain(build.routing_order().regular_terminals())
            .copied()
            .find(|terminal| {
                build.forest().terminal_ledger(*terminal).unwrap().state
                    != TerminalLedgerState::Quarantined
            })
            .expect("the fixture routes at least one live terminal");
        let request = TerminalCcoRequest::bind(build.forest(), terminal, frame).unwrap();
        let before = build.forest().digest();

        for (field, expected) in corrupted_authority_arms() {
            let mut corrupted =
                VerifiedRoutingAuthority::compute(&plan, &loads, &pool, &geometry).unwrap();
            let contact = corrupted
                .contact(u64::from(terminal))
                .expect("the plan owns this terminal's contact");
            corrupted.corrupt_for_test(field);
            let result = CcoController::default().grow_terminal_with_finalized_anchor(
                build.forest_mut(),
                &request,
                FinalizedAnchorTerminalRoute {
                    authority: &corrupted,
                    contact,
                    contact_id: u64::from(terminal),
                    config: AnchorSearchConfig::current().unwrap(),
                    process_resolution_mm: 0.05,
                },
                None,
            );
            let error = result
                .err()
                .unwrap_or_else(|| panic!("a corrupted {field:?} hoist routed a terminal"));
            assert!(
                expected(&error),
                "corrupting {field:?} refused at the wrong site: {error}"
            );
            assert_eq!(
                build.forest().digest(),
                before,
                "corrupting {field:?} mutated the forest before refusing"
            );
        }

        // The same route, with an honest token, still routes: the arms above
        // refuse the corruption rather than the path.
        let honest = VerifiedRoutingAuthority::compute(&plan, &loads, &pool, &geometry).unwrap();
        let contact = honest.contact(u64::from(terminal)).unwrap();
        CcoController::default()
            .grow_terminal_with_finalized_anchor(
                build.forest_mut(),
                &request,
                FinalizedAnchorTerminalRoute {
                    authority: &honest,
                    contact,
                    contact_id: u64::from(terminal),
                    config: AnchorSearchConfig::current().unwrap(),
                    process_resolution_mm: 0.05,
                },
                None,
            )
            .expect("an honest hoisted authority routes exactly as it did before J1");
    }

    /// FU-32 phase J1 — `validate_replay`'s quartet still refuses a token that
    /// went stale *between* `bind` and the serial winner replay.
    ///
    /// `bind` copies the four hoisted values out of the token; `validate_replay`
    /// compares those copies against the subjects' own sealed witnesses.  This
    /// test corrupts the token after a successful `bind`-shaped construction
    /// and proves the replay comparison — not `bind`'s — is what refuses.
    #[test]
    fn fu32_validate_replay_still_refuses_a_token_corrupted_after_bind() {
        let (build, plan, loads, surface, topology, mesh, delicacy) =
            real_no_winner_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let pool = SurfaceAnchorPool::from_geometry(&geometry, &delicacy).unwrap();
        let frame = no_winner_frame();
        let terminal = build
            .routing_order()
            .specialized_terminals()
            .iter()
            .chain(build.routing_order().regular_terminals())
            .copied()
            .find(|terminal| {
                build.forest().terminal_ledger(*terminal).unwrap().state
                    != TerminalLedgerState::Quarantined
            })
            .expect("the fixture routes at least one live terminal");
        let request = TerminalCcoRequest::bind(build.forest(), terminal, frame).unwrap();

        let authority = VerifiedRoutingAuthority::compute(&plan, &loads, &pool, &geometry).unwrap();
        let contact = authority.contact(u64::from(terminal)).unwrap();
        let mut context = FinalizedAnchorValidationContext::bind(
            build.forest(),
            &request,
            FinalizedAnchorTerminalRoute {
                authority: &authority,
                contact,
                contact_id: u64::from(terminal),
                config: AnchorSearchConfig::current().unwrap(),
                process_resolution_mm: 0.05,
            },
        )
        .expect("an honest token binds");
        assert!(context.validate_replay(build.forest(), &request).is_ok());

        // Move each recorded copy away from its subject in turn.  Every one of
        // the four arms must refuse on its own.
        let honest = (
            context.plan_digest,
            context.loads_digest,
            context.pool_digest,
            context.geometry_fingerprint.clone(),
        );
        for arm in 0..4 {
            context.plan_digest = honest.0;
            context.loads_digest = honest.1;
            context.pool_digest = honest.2;
            context.geometry_fingerprint = honest.3.clone();
            match arm {
                0 => context.plan_digest ^= 1,
                1 => context.loads_digest ^= 1,
                2 => context.pool_digest ^= 1,
                _ => context.geometry_fingerprint.corrupt_mesh_sha256_for_test(),
            }
            assert!(
                matches!(
                    context.validate_replay(build.forest(), &request),
                    Err(CcoError::StaleFinalizedAnchorAuthority)
                ),
                "validate_replay arm {arm} stopped refusing a stale authority"
            );
        }
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
                        AnchorFootRefusalReason::SeatBuriedSurface => 7,
                        AnchorFootRefusalReason::SeatNotAttached => 8,
                        AnchorFootRefusalReason::SeatGeometryUnusable => 9,
                        AnchorFootRefusalReason::SeatGrazingIncidence => 10,
                        AnchorFootRefusalReason::SeatDegenerateIncidence => 11,
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
            NoWinnerEvidence::ConfirmationRoundExhausted {
                candidates_generated,
                candidates_feasible,
                round_arms,
                anchor_refusals,
                ..
            } => format!(
                "3:{candidates_generated}:{candidates_feasible}:{}:{}",
                refusals(anchor_refusals),
                round_arms
                    .iter()
                    .map(crate::legalization::RoundArm::render)
                    .collect::<Vec<_>>()
                    .join("|"),
            ),
        }
    }

    /// **The finiteness obligation, discharged as a test rather than as a
    /// paragraph — design §4 Stage B, revert proof 5.**
    ///
    /// §1.3's termination argument needs the context universe to be finite.
    /// That needs the no-good set, and everything derived from it, to be no
    /// input to any digest in the context tuple — above all
    /// `controller_digest_before`, because the loop that holds the set is
    /// exactly where a leak would happen.
    ///
    /// If it leaked, every context would be fresh on every pass, so every
    /// no-good would miss forever, so the same candidate would be chosen and
    /// condemned every pass — and each of those passes would satisfy the
    /// abandon rule with a genuinely new triple while making no progress. A
    /// loop that never ends and never trips the one rule meant to catch that.
    ///
    /// So: grow the set by an arbitrary amount with rows that match nothing,
    /// and assert every digest in the context tuple is byte-identical. Cheap,
    /// mechanical, and without it the termination argument is circular.
    #[test]
    fn a_growing_no_good_set_moves_no_context_digest() {
        let route_digests = |steering: Option<&crate::legalization::LegalizationSteering>| {
            let (mut build, plan, loads, surface, topology, mesh, delicacy) =
                real_mixed_initial_transcript_fixture();
            let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
            let (mut controller, _, _, _, _) = route_initial_build_steered(
                &mut build,
                &plan,
                &loads,
                &geometry,
                &delicacy,
                transcript_frame(),
                steering,
            )
            .expect("no-goods that match nothing suppress nothing");
            let transcript = controller
                .finalize_transcript(&build, &plan, &loads, &geometry)
                .unwrap();
            let rows = transcript
                .rows()
                .iter()
                .map(|row| {
                    (
                        row.terminal_id(),
                        row.route().map(|route| {
                            (
                                route.plan_digest,
                                route.loads_digest,
                                route.geometry_digest,
                                route.forest_digest_before,
                                route.controller_digest_before,
                            )
                        }),
                        row.candidates_ruled_out(),
                    )
                })
                .collect::<Vec<_>>();
            (
                transcript.final_controller_digest(),
                transcript.final_forest_digest(),
                transcript.ordered_disposition_digest(),
                transcript.digest(),
                rows,
            )
        };

        let quiet = route_digests(None);

        // 4 096 no-goods, none of which can match: the terminal ids are past
        // the fixture's namespace and the prefixes are invented. "Arbitrary
        // amount" means the set is big enough that any accidental fold would
        // be visible, not that the rows are plausible.
        let mut loud = crate::legalization::LegalizationSteering::default();
        for index in 0..4096_u64 {
            loud.learn(crate::legalization::NoGood::new(
                1_000_000 + u32::try_from(index % 97).unwrap(),
                crate::legalization::PrefixKey::of(&TerminalRouteEvidence {
                    physical_contact_id: index,
                    terminal_id: 0,
                    original_key: crate::forest::MemberKey::transit_test_fixture(0, 0),
                    plan_digest: index.wrapping_mul(0x9e37_79b9_7f4a_7c15),
                    loads_digest: index ^ 0xdead_beef,
                    geometry_digest: index.rotate_left(17),
                    forest_digest_before: index.wrapping_add(3),
                    controller_digest_before: index.wrapping_sub(11),
                }),
                crate::legalization::ArmIdentity::live(
                    CandidateFamily::Bifurcation,
                    usize::try_from(index).unwrap(),
                    f64::from_bits(index | 0x4000_0000_0000_0000),
                    usize::try_from(index % 13).unwrap(),
                ),
            ));
        }
        assert_eq!(loud.len(), 4096, "the set really did grow");

        let steered = route_digests(Some(&loud));
        assert_eq!(
            quiet, steered,
            "the no-good set reached a digest in the context tuple; the termination \
             argument in design §1.3 is circular until this is byte-identical"
        );
    }

    /// **No retirement without a completed round — design §1.3 verifier
    /// obligation 3, sealed rather than printed.**
    ///
    /// # Why this test has to exist
    ///
    /// After Stage B, `UnroutedNoWinner` can mean two different things: *"this
    /// pass's tournament produced no winner"*, which it always meant, and *"no
    /// arm of this terminal survived the exact post-brace check"*, which is
    /// new. Same row, same `UnroutedContactReason::NoWinner`, same SPEC §6
    /// removal, same load repartition — and every existing check would agree
    /// with a defective one, because every existing check tests CONSISTENCY,
    /// not whether the retirement was EARNED.
    ///
    /// That is a new print-safety hole: fewer supports, silently, under a clean
    /// seal. It is closed by making the retirement carry the round — per arm,
    /// the MEASURED clearance margin in millimetres and the radii the resize
    /// assigned, at ranks contiguous from zero in the tournament's own order.
    /// What a reader can then check is not "here are k negative numbers" but
    /// "here is a round in which nothing this terminal could have chosen was
    /// legal".
    ///
    /// **Sabotage-shaped:** seal the retirement as one of the three counting
    /// reasons, or seal it with an empty round, and this goes red.
    #[test]
    fn a_round_that_ran_out_of_arms_seals_the_round_that_earned_the_retirement() {
        // Route once unsteered to learn a real terminal and its real arm.
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
        let (terminal_id, arm) = transcript
            .rows()
            .iter()
            .find_map(|row| {
                controller
                    .sealed_routed_arm(row.terminal_id())
                    .map(|(_, winner)| {
                        (
                            row.terminal_id(),
                            crate::legalization::ArmIdentity::of(winner),
                        )
                    })
            })
            .expect("the fixture routes real terminals");

        // A round already driven past every arm this terminal has, carrying the
        // row it measured getting there. `usize::MAX` is not a cap and not a
        // constant — it is "a rank no slate reaches", which is what exhaustion
        // means when the round itself is uncapped.
        let measured = vec![crate::legalization::ArmRefusalPhysics {
            gap_mm: -0.05,
            distance_clearance_mm: 0.280_044,
            axis_transit_illegal: true,
            terminal_radius_mm: 0.372_136,
            rootward_radius_mm: 0.372_136,
        }
        .at_rank(0, arm)];
        let mut steering = crate::legalization::LegalizationSteering::default();
        steering.set_round(Some(crate::legalization::ConfirmationRound {
            terminal_id,
            arm_rank: usize::MAX,
            measured: measured.clone(),
        }));

        let (mut build, plan, loads, surface, topology, mesh, delicacy) =
            real_mixed_initial_transcript_fixture();
        let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
        let (mut controller, _, _, _, _) = route_initial_build_steered(
            &mut build,
            &plan,
            &loads,
            &geometry,
            &delicacy,
            transcript_frame(),
            Some(&steering),
        )
        .expect("a round forces a rank; it never reports its steering spent");
        let transcript = controller
            .finalize_transcript(&build, &plan, &loads, &geometry)
            .unwrap();

        let row = transcript
            .rows()
            .iter()
            .find(|row| row.terminal_id() == terminal_id)
            .expect("the terminal is still in the cadence");
        assert_eq!(
            row.kind(),
            FinalizedDispositionKind::UnroutedNoWinner,
            "a round driven past its last arm retires the terminal"
        );
        let Some(NoWinnerEvidence::ConfirmationRoundExhausted {
            round_arms,
            candidates_feasible,
            ..
        }) = row.no_winner_reason()
        else {
            panic!(
                "a retirement earned by a round must NOT be sealed as one of the three counting \
                 reasons: {:?}",
                row.no_winner_reason()
            );
        };
        assert_eq!(
            round_arms, &measured,
            "the seal must carry the round's own measured millimetres and radii, arm by arm"
        );
        assert!(
            *candidates_feasible > 0,
            "the round's verdict is about arms that DID exist and DID pass feasibility — that \
             is the whole difference between it and NoFeasibleCandidate"
        );
    }

    /// **Steering is live on the real routing path — design §4 Stage B revert
    /// proof 1's first half, and the honesty check the whole stage rests on.**
    ///
    /// # Why this test has to exist
    ///
    /// A no-good that MISSES on every pass is indistinguishable, from the
    /// outside, from a model that genuinely cannot be legalized: the second
    /// pass would learn nothing new, the abandon rule would fire, and the run
    /// would refuse by name for what looks like exactly the right reason. So
    /// the mechanism has to be shown changing a real tournament's mind on real
    /// geometry, not only on a synthetic slate.
    ///
    /// Three passes: route, learn the arm each terminal actually chose, route
    /// again, and assert the winners MOVED and the second counter says so. The
    /// exhaustion arm — every arm suppressed — is proved in microseconds
    /// instead of in seventy routing passes by
    /// `legalization::tests::a_slate_with_a_no_good_on_every_arm_is_spent_and_never_empty`;
    /// this test's job is the wiring, not the arithmetic.
    ///
    /// MEASURED at the full length once, for the record: this fixture's
    /// terminal 0 has 69 arms, and driving it to exhaustion took 70 passes and
    /// 101 s before reporting `SteeringSpent` and retiring nothing.
    #[test]
    fn steering_moves_a_real_tournaments_choice_and_retires_nobody() {
        let route_once = |steering: Option<&crate::legalization::LegalizationSteering>| {
            let (mut build, plan, loads, surface, topology, mesh, delicacy) =
                real_mixed_initial_transcript_fixture();
            let geometry = AnchorGeometryAuthority::new(&mesh, &topology, &surface).unwrap();
            let (mut controller, _, _, _, _) = route_initial_build_steered(
                &mut build,
                &plan,
                &loads,
                &geometry,
                &delicacy,
                transcript_frame(),
                steering,
            )
            .expect("this fixture's terminals have far more arms than these passes suppress");
            let transcript = controller
                .finalize_transcript(&build, &plan, &loads, &geometry)
                .unwrap();
            let arms = transcript
                .rows()
                .iter()
                .filter_map(|row| {
                    controller
                        .sealed_routed_arm(row.terminal_id())
                        .map(|(route, winner)| {
                            (
                                row.terminal_id(),
                                crate::legalization::PrefixKey::of(route),
                                crate::legalization::ArmIdentity::of(winner),
                            )
                        })
                })
                .collect::<Vec<_>>();
            let ruled_out = transcript
                .rows()
                .iter()
                .map(FinalizedCcoTranscriptRow::candidates_ruled_out)
                .sum::<usize>();
            let retired = transcript
                .rows()
                .iter()
                .filter(|row| row.kind() == FinalizedDispositionKind::UnroutedNoWinner)
                .count();
            (arms, ruled_out, retired)
        };

        let (first_arms, first_ruled_out, first_retired) = route_once(None);
        assert!(!first_arms.is_empty(), "the fixture routes real terminals");
        assert_eq!(
            first_ruled_out, 0,
            "an unsteered pass rules nothing out; the second counter is physics, not noise"
        );

        // **One no-good, on the LAST routed terminal — the production shape.**
        //
        // The prefix key is tight on purpose, and that has a consequence worth
        // stating rather than discovering: steering an EARLY terminal moves the
        // forest, so every later terminal's `forest_digest_before` and
        // `controller_digest_before` move too, and every one of their no-goods
        // misses. MEASURED on this fixture: seed all 139 routed terminals and
        // exactly 1 arm is ruled out.
        //
        // That is not a defect, it is §1.3's key doing its job — a refusal
        // measured in one world says nothing about a different one. And it is
        // the case the models are in: the S-1 measurement found the mismatch
        // class is ONE terminal per run and it is late in the cadence
        // (Deinonychus t=113 of 202, Rotting Warhorse t=177 of 317), so every
        // terminal before it routes identically and its own prefix is stable
        // pass to pass.
        let (last_terminal, last_prefix, last_arm) = *first_arms
            .last()
            .expect("the fixture routes real terminals");
        let mut steering = crate::legalization::LegalizationSteering::default();
        assert!(steering.learn(crate::legalization::NoGood::new(
            last_terminal,
            last_prefix,
            last_arm
        )));

        let (second_arms, second_ruled_out, second_retired) = route_once(Some(&steering));
        assert_eq!(
            second_ruled_out, 1,
            "the last routed terminal's own arm must be passed over and counted in \
             `candidates_ruled_out` — a miss here means the key does not match and the whole \
             mechanism is a silent no-op that would refuse looking exactly like an honest \
             refusal"
        );
        assert_eq!(
            second_arms.len(),
            first_arms.len(),
            "suppressing an arm must not cost a terminal its route while it has others"
        );
        assert_eq!(
            second_retired, first_retired,
            "steering retired a terminal; §1.3 forbids a retirement earned by a no-good"
        );
        let second_last = *second_arms.last().unwrap();
        assert_eq!(
            second_last.0, last_terminal,
            "the steered terminal still routes; it did not vanish from the record"
        );
        assert_ne!(
            second_last.2, last_arm,
            "the second pass chose the identical arm: the no-good did not steer anything"
        );
        assert_eq!(
            first_arms[..first_arms.len() - 1],
            second_arms[..second_arms.len() - 1],
            "one terminal's no-good disturbed another terminal's route"
        );
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
        assert!(
            transcript
                .rows()
                .iter()
                .all(|row| row.candidates_ruled_out() == 0),
            "nothing rules an arm out yet: the second counter is zero on every path until Stage B"
        );
        let replay = routed_request.unwrap();
        let replay_contact = plan
            .contacts()
            .iter()
            .find(|contact| contact.contact_id() == replay.physical_contact_id)
            .unwrap();
        let replay_pool = SurfaceAnchorPool::from_geometry(&geometry, &delicacy).unwrap();
        let replay_authority =
            VerifiedRoutingAuthority::compute(&plan, &loads, &replay_pool, &geometry).unwrap();
        assert!(matches!(
            controller.grow_terminal_with_finalized_anchor(
                build.forest_mut(),
                &replay,
                FinalizedAnchorTerminalRoute {
                    authority: &replay_authority,
                    contact: replay_contact,
                    contact_id: replay.physical_contact_id,
                    config: AnchorSearchConfig::current().unwrap(),
                    process_resolution_mm: 0.05,
                },
                None,
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
            confirmation_round: None,
            winner: None,
            disposition: None,
            candidates_generated: 0,
            candidates_feasible: 0,
            candidates_ruled_out: 0,
            anchor_ray_batches: 1,
            anchor_foot_batches: 0,
            clearance_batches: 0,
            validated_model_anchor: None,
            anchor_seat_refusals: vec![zero],
            anchor_refusals: Vec::new(),
            contact_attestation_digest: 0,
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

    /// OD-7 / PR-6: a bifurcation PARTITIONS the model-contact declaration
    /// between the two halves; it must never duplicate it.
    ///
    /// This pins `inherited_upper_contact_end`, which IS the production
    /// decision `apply_bifurcation` makes rather than a restatement of it. The
    /// rule is pinned here rather than through a full `apply_bifurcation` run
    /// because driving that path needs a host that is simultaneously
    /// bifurcation-feasible AND carrying a Terminalward declaration, and the
    /// engine's own invariants exclude that combination today: the only
    /// Terminalward members are initial contact stubs, and stubs are barred
    /// from being bifurcation hosts. That exclusion is exactly why the bug was
    /// latent, and it is equally why an end-to-end fixture cannot reach it.
    #[test]
    fn bifurcation_partitions_the_model_contact_declaration() {
        // The upper half owns junction..p1, i.e. the ROOTWARD end.
        assert_eq!(
            inherited_upper_contact_end(Some(ContactEnd::Rootward)),
            Some(ContactEnd::Rootward)
        );
        // The lower half owns p0..junction and keeps a Terminalward contact;
        // the upper half's p0 is mid-air and carries none.
        assert_eq!(
            inherited_upper_contact_end(Some(ContactEnd::Terminalward)),
            None
        );
        assert_eq!(inherited_upper_contact_end(None), None);

        // Partitioned, never duplicated: for every possible host declaration at
        // most one half ends up declaring a contact, and a contactless host
        // never gains one.
        for host in [
            None,
            Some(ContactEnd::Terminalward),
            Some(ContactEnd::Rootward),
        ] {
            let upper = inherited_upper_contact_end(host);
            let lower = match host {
                // Mirrors `set_model_contact_end(host_key, None)` in
                // `apply_bifurcation`, which clears Rootward from the lower
                // half because the lower half no longer owns p1.
                Some(ContactEnd::Rootward) => None,
                other => other,
            };
            assert!(
                !(upper.is_some() && lower.is_some()),
                "host {host:?} left both halves declaring a contact"
            );
            assert!(
                host.is_some() || (upper.is_none() && lower.is_none()),
                "a contactless host must not gain a declaration"
            );
        }
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
            model_contact_end: Some(ContactEnd::Terminalward),
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
            model_contact_end: Some(ContactEnd::Terminalward),
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
                model_contact_end: None,
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
                model_contact_end: None,
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
            model_contact_end: None,
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
                    model_contact_end: Some(ContactEnd::Terminalward),
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
                    candidates_ruled_out: 0,
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

    /// FU-32 J2 — scripted disposition covering every record shape the
    /// digest walks: all three disposition variants and all three
    /// `NoWinnerEvidence` variants, with non-empty seat-refusal lists (the
    /// Θ(N·R) term the accumulator exists to collapse).
    fn scripted_disposition(index: usize, entry: &TerminalLedgerEntry) -> TerminalCadenceRecord {
        let seed = index as u64 + 1;
        let refusals = |count: usize| -> Vec<AnchorSeatRefusal> {
            (0..count)
                .map(|row| AnchorSeatRefusal {
                    sample_index: index * 100 + row,
                    source_sample_face_id: (row as u32) * 7 + entry.contact,
                    reason: match row % 3 {
                        0 => crate::anchor::AnchorSeatRefusalReason::TooShort,
                        1 => crate::anchor::AnchorSeatRefusalReason::NoFirstHit,
                        _ => crate::anchor::AnchorSeatRefusalReason::NearerNonExteriorObstruction,
                    },
                    first_hit_t: (row % 2 == 0).then(|| Finite::new(0.25 + row as f64).unwrap()),
                })
                .collect()
        };
        let route = TerminalRouteEvidence {
            physical_contact_id: u64::from(entry.contact),
            terminal_id: entry.contact,
            original_key: entry.original_key,
            plan_digest: seed.wrapping_mul(3),
            loads_digest: seed.wrapping_mul(5),
            geometry_digest: seed.wrapping_mul(7),
            forest_digest_before: seed.wrapping_mul(11),
            controller_digest_before: seed.wrapping_mul(13),
        };
        let disposition = match index % 5 {
            0 => TerminalDisposition::Routed {
                route,
                winner: CcoCandidateEvidence {
                    family: CandidateFamily::PlateDescent,
                    generation_index: index,
                    price_ml: 0.5 + index as f64,
                    clearance_rows: index * 3,
                },
            },
            1 => TerminalDisposition::SkippedQuarantined {
                terminal_id: entry.contact,
                original_key: entry.original_key,
                forest_digest: seed.wrapping_mul(17),
                controller_digest_before: seed.wrapping_mul(19),
            },
            2 => TerminalDisposition::UnroutedNoWinner {
                route,
                reason: NoWinnerEvidence::NoCandidatesGenerated {
                    anchor_seat_refusals: refusals(3 + index % 4),
                },
            },
            3 => TerminalDisposition::UnroutedNoWinner {
                route,
                reason: NoWinnerEvidence::NoFeasibleCandidate {
                    candidates_generated: index + 1,
                    anchor_seat_refusals: refusals(2 + index % 3),
                    anchor_refusals: Vec::new(),
                },
            },
            _ => TerminalDisposition::UnroutedNoWinner {
                route,
                reason: NoWinnerEvidence::AllFeasibleCandidatesRefusedByClearance {
                    candidates_generated: index + 2,
                    candidates_feasible: index + 1,
                    anchor_seat_refusals: refusals(1 + index % 5),
                    anchor_refusals: Vec::new(),
                },
            },
        };
        TerminalCadenceRecord {
            disposition,
            candidates_ruled_out: 0,
        }
    }

    /// The J2 equivalence check, asserted after EVERY insertion: the cached
    /// accumulator pair equals the from-scratch rebuild bit-for-bit, and the
    /// incremental digest equals the digest over the rebuilt pair.
    fn assert_incremental_equals_rebuilt(controller: &CcoController) {
        let (records_acc, hosts_acc) = controller.rebuild_accumulators();
        assert_eq!(
            (records_acc, hosts_acc),
            (controller.records_acc, controller.hosts_acc),
            "cached accumulators drifted from the from-scratch rebuild"
        );
        assert_eq!(
            controller.digest(),
            controller.digest_with_accumulators(records_acc, hosts_acc),
            "incremental digest diverged from the full recompute"
        );
    }

    #[test]
    fn fu32_incremental_digest_equals_the_full_walk_after_n_terminals() {
        // Arm 1 -- GROWN on real geometry, committed winner.
        let (mut forest, request, surface) = growth_fixture();
        let mut grown = CcoController::default();
        assert_incremental_equals_rebuilt(&grown);
        grown
            .grow_terminal_for_test(&mut forest, &request, &surface, &growth_topology(), 0.05)
            .unwrap();
        assert_incremental_equals_rebuilt(&grown);

        // Arm 1b -- GROWN on real geometry, no winner: the refusal-carrying
        // evidence arm that dominates the treant (0 of 3429 model anchors
        // survive corpus-wide).
        let (mut forest, request, surface) = no_winner_fixture();
        let mut unrouted = CcoController::default();
        unrouted
            .grow_terminal_for_test(&mut forest, &request, &surface, &growth_topology(), 0.05)
            .unwrap();
        assert_incremental_equals_rebuilt(&unrouted);

        // Arm 2 -- SKIPPED-QUARANTINED via the production path, across the
        // KDTREE_REBUILD_EVERY boundary so the hosts-accumulator refresh is
        // exercised where hosts actually change.
        let (forest, skipped, _) = quarantined_cadence_fixture(26, 0);
        let mut quarantined = CcoController::default();
        for entry in skipped {
            quarantined
                .advance_skipped_terminal(&forest, entry)
                .unwrap();
            assert_incremental_equals_rebuilt(&quarantined);
        }

        // Scripted mix -- every disposition variant, every NoWinnerEvidence
        // variant, refusal lists attached, 30 records crossing the rebuild
        // boundary; checked after EVERY insertion, on two identical
        // trajectories that must agree step by step (oracle and new digest
        // alike).
        let (forest, skipped, _) = quarantined_cadence_fixture(30, 100);
        let mut first = CcoController::default();
        let mut second = CcoController::default();
        let mut oracle_values = vec![first.digest_full_walk_oracle()];
        let mut new_values = vec![first.digest()];
        for (index, entry) in skipped.iter().enumerate() {
            let record = scripted_disposition(index, entry);
            first.finish_terminal(&forest, entry.contact, record.clone());
            second.finish_terminal(&forest, entry.contact, record);
            assert_incremental_equals_rebuilt(&first);
            assert_eq!(first.digest(), second.digest());
            assert_eq!(
                first.digest_full_walk_oracle(),
                second.digest_full_walk_oracle()
            );
            oracle_values.push(first.digest_full_walk_oracle());
            new_values.push(first.digest());
        }
        // The retained pre-J2 walk is the PARTITION oracle -- its values
        // necessarily moved (F5 proved no value-preserving restructure
        // exists), so what it certifies is discrimination: every state the
        // old walk distinguished along this trajectory, the restructured
        // digest must distinguish too.
        for i in 0..oracle_values.len() {
            for j in (i + 1)..oracle_values.len() {
                assert_ne!(
                    oracle_values[i], oracle_values[j],
                    "oracle collision at ({i},{j})"
                );
                assert_ne!(
                    new_values[i], new_values[j],
                    "state distinguished by the retained walk collided under the incremental digest at ({i},{j})"
                );
            }
        }
    }

    /// FU-32 J2 (F6/§D2.2) — the stage-boundary tripwires
    /// (`validate_brace_preflight` / `validate_rewire_preflight` in
    /// `runner.rs`) compare the sealed controller digest through
    /// [`CcoController::sealed_digest_matches_rebuilt`].  This pins both of
    /// that method's teeth: a corrupted ACCUMULATOR is caught (the rebuilt
    /// pair is cross-checked against the cached pair), and a mutated STATE is
    /// caught by genuine recompute even when the cached digest still matches
    /// the seal — the trust-the-cache failure mode F6 forbids.
    #[test]
    fn fu32_a_corrupted_accumulator_is_caught_at_the_stage_boundary_tripwire() {
        let (forest, skipped, _) = quarantined_cadence_fixture(30, 200);
        let mut controller = CcoController::default();
        for (index, entry) in skipped.iter().enumerate() {
            controller.finish_terminal(&forest, entry.contact, scripted_disposition(index, entry));
        }
        // `finalize_transcript` seals with the digest over the REBUILT pair
        // (§D2.2); with a coherent cache that value equals `digest()`.
        let (records_acc, hosts_acc) = controller.rebuild_accumulators();
        let sealed = controller.digest_with_accumulators(records_acc, hosts_acc);
        assert_eq!(sealed, controller.digest());
        assert!(controller.sealed_digest_matches_rebuilt(sealed));

        // A corrupted records accumulator is CAUGHT at the tripwire.
        let pristine = controller.records_acc;
        controller.records_acc ^= 0x5eed_0bad_c0ff_ee00;
        assert!(
            !controller.sealed_digest_matches_rebuilt(sealed),
            "a corrupted records accumulator must fail the tripwire"
        );
        controller.records_acc = pristine;
        assert!(controller.sealed_digest_matches_rebuilt(sealed));

        // A corrupted hosts accumulator is CAUGHT at the tripwire.
        let pristine = controller.hosts_acc;
        controller.hosts_acc = controller.hosts_acc.wrapping_add(1);
        assert!(
            !controller.sealed_digest_matches_rebuilt(sealed),
            "a corrupted hosts accumulator must fail the tripwire"
        );
        controller.hosts_acc = pristine;
        assert!(controller.sealed_digest_matches_rebuilt(sealed));

        // A state mutation the cached digest CANNOT see (the record changes
        // behind the accumulator's back) is still caught, because the
        // tripwire genuinely recomputes from state.
        let tampered = skipped[0].contact;
        match &mut controller
            .terminals
            .get_mut(&tampered)
            .expect("scripted terminal present")
            .disposition
        {
            TerminalDisposition::Routed { winner, .. } => winner.price_ml += 1.0,
            _ => unreachable!("scripted index 0 is Routed"),
        }
        assert_eq!(
            controller.digest(),
            sealed,
            "the cached digest is blind to this tamper -- exactly why the tripwire must rebuild"
        );
        assert!(
            !controller.sealed_digest_matches_rebuilt(sealed),
            "a state mutation after seal must fail the genuine recompute"
        );
    }

    /// J2 mutation triad (a): a record folded twice must fail the
    /// equivalence harness.
    #[test]
    fn fu32_mutation_a_double_folded_record_fails_equivalence() {
        let (forest, skipped, _) = quarantined_cadence_fixture(8, 300);
        let mut controller = CcoController::default();
        for (index, entry) in skipped.iter().enumerate() {
            controller.finish_terminal(&forest, entry.contact, scripted_disposition(index, entry));
        }
        let honest = controller.digest();
        assert_incremental_equals_rebuilt(&controller);

        // Simulate the mutant: the first record's hash folded a second time.
        let doubled = terminal_record_hash(
            0,
            skipped[0].contact,
            controller
                .terminals
                .get(&skipped[0].contact)
                .expect("first scripted record present"),
        );
        controller.records_acc = controller.records_acc.wrapping_add(doubled);
        assert_ne!(controller.digest(), honest);
        let (records_acc, hosts_acc) = controller.rebuild_accumulators();
        assert_ne!(
            (records_acc, hosts_acc),
            (controller.records_acc, controller.hosts_acc),
            "the equivalence harness must catch a double fold"
        );
        assert_ne!(
            controller.digest(),
            controller.digest_with_accumulators(records_acc, hosts_acc)
        );
    }

    /// J2 mutation triad (b): skipping the hosts-accumulator refresh at a
    /// KDTREE_REBUILD_EVERY boundary must fail the equivalence harness.
    #[test]
    fn fu32_mutation_b_stale_hosts_accumulator_at_rebuild_boundary_fails_equivalence() {
        let (forest, skipped, _) = quarantined_cadence_fixture(25, 400);
        let mut controller = CcoController::default();
        // Hosts change only at insertion 25, so the pre-trajectory value IS
        // the stale value the (b) mutant would have kept.
        let stale_hosts_acc = controller.hosts_acc;
        for (index, entry) in skipped.iter().enumerate() {
            controller.finish_terminal(&forest, entry.contact, scripted_disposition(index, entry));
        }
        // The fixture forest has a live trunk, so the rebuild at 25 made the
        // cache non-empty and the refresh moved the accumulator.
        assert_ne!(controller.hosts_acc, stale_hosts_acc);
        assert_eq!(controller.rebuild_accumulators().1, controller.hosts_acc);
        assert_ne!(
            controller.digest_with_accumulators(controller.records_acc, stale_hosts_acc),
            controller.digest(),
            "the equivalence harness must catch a skipped hosts refresh"
        );
    }

    /// J2 mutation triad (c): with `cadence_index` dropped from the
    /// per-record hash, two identical-content records inserted at swapped
    /// positions collide under the commutative fold; with it bound, the two
    /// orders stay distinct.
    #[test]
    fn fu32_mutation_c_dropping_cadence_index_makes_swapped_records_collide() {
        let (forest, skipped, _) = quarantined_cadence_fixture(2, 500);
        let t1 = skipped[0].contact;
        let t2 = skipped[1].contact;
        // IDENTICAL record content for both terminals -- only the outer
        // terminal id and the cadence position differ.
        let record = TerminalCadenceRecord {
            disposition: TerminalDisposition::SkippedQuarantined {
                terminal_id: 9,
                original_key: skipped[0].original_key,
                forest_digest: 1234,
                controller_digest_before: 5678,
            },
            candidates_ruled_out: 0,
        };
        let mut forward = CcoController::default();
        forward.finish_terminal(&forest, t1, record.clone());
        forward.finish_terminal(&forest, t2, record.clone());
        let mut backward = CcoController::default();
        backward.finish_terminal(&forest, t2, record.clone());
        backward.finish_terminal(&forest, t1, record.clone());

        // Computed exactly as the (c) mutant would -- no cadence_index in the
        // per-record hash -- the two trajectories' accumulators are one and
        // the same sum, i.e. the swap is invisible.
        let no_index_hash = |terminal: u32| {
            let mut fnv = FnvWords::new();
            fnv.word(u64::from(terminal));
            let mut word = |value: u64| fnv.word(value);
            digest_terminal_record(&mut word, &record);
            fnv.finish()
        };
        assert_eq!(
            no_index_hash(t1).wrapping_add(no_index_hash(t2)),
            no_index_hash(t2).wrapping_add(no_index_hash(t1)),
            "the index-free mutant cannot tell the two orders apart"
        );
        // With cadence_index bound inside every record hash, the swap moves
        // the accumulator (and the digest), so the harness catches the mutant.
        assert_ne!(
            forward.records_acc, backward.records_acc,
            "cadence_index must bind insertion order into the accumulator"
        );
        assert_ne!(forward.digest(), backward.digest());
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
            .grow_terminal_for_test(
                &mut grown_forest,
                &request,
                &surface,
                &growth_topology(),
                0.05,
            )
            .unwrap();
        let grown_digest = grown.digest();
        let grown_forest_digest = grown_forest.digest();
        assert!(matches!(
            grown.grow_terminal_for_test(
                &mut grown_forest,
                &request,
                &surface,
                &growth_topology(),
                0.05
            ),
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
            skipped_first.grow_terminal_for_test(
                &mut live_forest,
                &request,
                &surface,
                &growth_topology(),
                0.05
            ),
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
            model_contact_end: None,
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
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: request.stub_snapshot.carried_mm2(),
        }));
        assert!(matches!(
            wrong_owner.register_terminal(7, stub),
            Err(ForestError::InvalidTerminalMember { terminal: 7 })
        ));
    }

    /// #36 — the face adjacency for the SAME tetrahedron `growth_fixture`
    /// builds its surface from, welded by the production policy
    /// (`WeldPolicy::ExactBits`) so `project`'s face IDs and the adjacency the
    /// seat-zone fill walks address the same faces.
    fn growth_topology() -> FaceTopology {
        let (vertices, triangles) = growth_fixture_parts();
        let mesh = plinth_mesh::Mesh::new(vertices, triangles).expect("growth fixture mesh");
        plinth_mesh::build_face_topology(&mesh, plinth_mesh::WeldPolicy::ExactBits)
            .expect("growth fixture topology")
    }

    fn growth_fixture_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        (
            vec![
                [100.0, 100.0, 100.0],
                [101.0, 100.0, 100.0],
                [100.0, 101.0, 100.0],
                [100.0, 100.0, 101.0],
            ],
            vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        )
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
            model_contact_end: Some(ContactEnd::Terminalward),
            carried_mm2: finite(2.0),
        }));
        forest.register_terminal(7, stub).unwrap();
        let request =
            TerminalCcoRequest::bind(&forest, 7, BuildFrame::new(finite(0.0), positive(5.0)))
                .unwrap();
        let (vertices, triangles) = growth_fixture_parts();
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

    /// A slab whose top face is the plane `z = 0`, wide enough in `xy` that the
    /// nearest surface point to anything above the origin is that face. The
    /// outward normal there is `+z`, so a seat at the origin seats along `-z`.
    fn planar_slab() -> NativeSurface {
        let (vertices, faces) = planar_slab_parts();
        NativeSurface::from_indexed(
            &vertices,
            &faces,
            geom_kernel::NativeSurfaceConfig {
                grid_pitch_mm: 0.5,
                max_grid_bytes: 16 * 1024 * 1024,
            },
        )
        .expect("planar slab")
    }

    /// #36 — the face adjacency for the SAME indexed mesh `planar_slab` builds
    /// its surface from, welded by the production policy.
    fn planar_slab_topology() -> FaceTopology {
        let (vertices, faces) = planar_slab_parts();
        let mesh = plinth_mesh::Mesh::new(vertices, faces).expect("planar slab mesh");
        plinth_mesh::build_face_topology(&mesh, plinth_mesh::WeldPolicy::ExactBits)
            .expect("planar slab topology")
    }

    fn planar_slab_parts() -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
        const H: f64 = 20.0;
        let vertices = vec![
            [-H, -H, -10.0],
            [H, -H, -10.0],
            [H, H, -10.0],
            [-H, H, -10.0],
            [-H, -H, 0.0],
            [H, -H, 0.0],
            [H, H, 0.0],
            [-H, H, 0.0],
        ];
        let faces = vec![
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
        (vertices, faces)
    }

    /// One candidate holding exactly one model-anchor strut, built directly so
    /// the geometry under test is stated rather than searched for.
    fn model_anchor_candidate(p0: [f64; 3], p1: [f64; 3]) -> (EvaluatedCandidate, Forest) {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let snapshot = Forest::new();
        let mut forest = snapshot.clone();
        forest.add_member(Member::new(MemberSpec {
            p0_mm: p0.map(finite),
            p1_mm: p1.map(finite),
            r0_mm: TerminalEndRadius::new(positive(0.45)),
            r1_mm: RootwardEndRadius::new(positive(0.35)),
            kind: MemberKind::Strut,
            owner: Owner::Contact(7),
            model_contact_end: Some(ContactEnd::Rootward),
            carried_mm2: finite(2.0),
        }));
        // **#36 — the helper freezes the seat zone exactly where production
        // does: BEFORE `clearance_rows` reads it.** `apply_model_anchor`
        // installs the descriptor's zone on the strut it mints, and `evaluate`
        // then builds the rows from that forest. A helper that skipped the
        // freeze would hand every test below an arm graded over its own
        // designed embed — which is a real refusal, but not the one those tests
        // are about.
        let key = *forest
            .live_tree_members()
            .first()
            .expect("the anchor is the one live member");
        let surface = planar_slab();
        let (_, _, face_ids, _) = surface.project(&[p1]);
        if let Ok(seat_face_id) = u32::try_from(face_ids[0]) {
            let standoff_mm = m0_constants()
                .unwrap()
                .placeholders()
                .standoff_mm()
                .diagnostic()
                .get();
            let zone = crate::seat_zone::freeze_production_seat_zones(
                &surface,
                &planar_slab_topology(),
                &[crate::seat_zone::SeatZoneRequest {
                    p0_mm: p0,
                    p1_mm: p1,
                    seat_face_id,
                    // The ROOTWARD end is the contact, so `r_seat` is `r1`.
                    band_radius_mm: 0.35 + standoff_mm,
                }],
            )
            .expect("the seat zone freezes")
            .remove(0);
            forest.set_seat_zone(key, zone);
        }
        let rows = clearance_rows(&forest, &snapshot);
        assert_eq!(rows.len(), 1, "exactly one changed member");
        assert!(
            rows[0].contact.is_some(),
            "the row declares its model contact"
        );
        (
            EvaluatedCandidate {
                forest: forest.clone(),
                family: CandidateFamily::ModelAnchor,
                generation_index: 0,
                price_ml: 1.0,
                rows,
                anchor: None,
            },
            forest,
        )
    }

    /// **#51, END TO END, AGAINST THE REAL KERNEL — the arithmetic case flips
    /// from refused to routed.**
    ///
    /// The theorem the design derived: with the flat rootward inset the last
    /// tested station sits exactly `standoff` from a point ON the surface, so
    ///
    /// ```text
    /// clearance = d_min - (r_max + standoff) <= standoff - r_max - standoff = -r_max
    /// ```
    ///
    /// unconditionally — **every model-anchor arm fails by exactly its own
    /// radius**, which is what the island diagnosis measured as `-0.350 mm`
    /// attained 53 times on both Warhorse islands.
    ///
    /// Both halves are asserted here, both from the kernel and not from
    /// arithmetic restated in the test:
    ///
    /// * **BEFORE** — the unseated arm with the retired flat `standoff_mm`
    ///   inset reads back **exactly `-r_max`**. This is the negative control:
    ///   it proves the fix is not passing because the fixture is easy.
    /// * **AFTER** — the PR-7-seated arm screened through `clearance_survival`
    ///   SURVIVES, and the kernel's number for it is **exactly MARGIN**, which
    ///   is PR-2's headline guarantee rather than mere slack.
    #[test]
    fn the_model_anchor_arm_that_failed_by_exactly_its_radius_now_routes() {
        let surface = planar_slab();
        let policy = seat_contact_policy(0.05).unwrap();
        let r_max = 0.45_f64;

        // BEFORE — `p1` is the RAW seat point, on the surface, and the inset is
        // the retired flat `standoff_mm`. This reproduces the pre-PR-7 geometry
        // and the pre-#51 inset pair, and hands them to the same kernel entry
        // point production uses.
        let p0 = [0.0, 0.0, 5.0];
        let raw_seat = [0.0, 0.0, 0.0];
        let (flat, _) = surface
            .segment_clearance(
                &[p0],
                &[raw_seat],
                &[r_max],
                &[0.35],
                policy.standoff_mm,
                &[0.0],
                &[policy.standoff_mm],
            )
            .expect("kernel clearance for the retired inset pair");
        assert!(
            (flat[0] + r_max).abs() < 1e-9,
            "the retired flat inset must fail by EXACTLY r_max = {r_max}; kernel said {}",
            flat[0]
        );

        // AFTER — PR-7 seats the endpoint at the PR-1 depth along the surface
        // normal, and `clearance_survival` masks it with the PR-2 extent.
        let seat_depth = crate::contact::seat_depth_mm(1.0, 0.35, &policy, false);
        let (seated, _, _) =
            crate::contact::seat_endpoint(raw_seat, [0.0, 0.0, 1.0], p0, seat_depth);
        let (candidate, _) = model_anchor_candidate(p0, seated);
        let verdict = clearance_survival(&surface, &policy, &[candidate]).unwrap();
        assert_eq!(
            verdict,
            vec![ClearanceVerdict::Survives],
            "the seated arm must route under the converged mask"
        );

        // ... and the RETIRED path's own guarantee, kept as the historical
        // control for #51 rather than as a live claim. This is a direct kernel
        // call with the PR-2 mask as an inset — the shape production used
        // before #36 step 5 — and on a locally planar seat it returned exactly
        // MARGIN. Production no longer makes this call: the exact pass and this
        // stage both test the full axis against `surface \ Z` now. It stays
        // because it is the "after" half of #51's before/after pair and
        // deleting it would erase the measurement that closed that task.
        let mask = crate::contact::placed_mask_inset_mm(
            crate::contact::ContactSeating::Seated,
            1.0,
            0.35,
            r_max,
            &policy,
        )
        .unwrap();
        let (masked, _) = surface
            .segment_clearance(
                &[p0],
                &[seated],
                &[r_max],
                &[0.35],
                policy.standoff_mm,
                &[0.0],
                &[mask],
            )
            .expect("kernel clearance for the converged mask");
        assert!(
            (masked[0] - policy.process_resolution_mm).abs() < 1e-9,
            "PR-2 returns exactly MARGIN = {}; kernel said {}",
            policy.process_resolution_mm,
            masked[0]
        );
    }

    /// **PR-3 (design review F1) — the enlarged exemption must not become an
    /// ERASURE.**
    ///
    /// A grazing model anchor's PR-2 mask diverges as `1/|cos|`. Once it
    /// exceeds the member's own length `seg_clear_one_excluding` returns `INFINITY`
    /// (`queries.rs:1786-1788`) and the arm is admitted having never been
    /// distance-tested at all. That is the hazard Piece B creates and PR-3
    /// closes, and it is the reason the two land in the same commit.
    ///
    /// NON-VACUITY is asserted first, from the kernel: the mask really does
    /// consume this segment and the kernel really does answer `INFINITY`.
    #[test]
    fn a_grazing_model_anchor_is_refused_not_admitted_with_infinity() {
        let surface = planar_slab();
        let policy = seat_contact_policy(0.05).unwrap();
        let r_max = 0.45_f64;

        // A nearly tangential approach: the arm runs 2 mm across the face while
        // descending 0.4 mm, so |cos| ~ 0.196 and the mask is ~7.1 mm against a
        // ~2.04 mm member.
        let p0 = [2.0, 0.0, 0.1];
        let seated = [0.0, 0.0, -0.3];
        let axis = sub(seated, p0);
        let length = norm(axis);
        let abs_cos = dot(scale(axis, 1.0 / length), [0.0, 0.0, 1.0]).abs();
        let mask = crate::contact::placed_mask_inset_mm(
            crate::contact::ContactSeating::Seated,
            abs_cos,
            0.35,
            r_max,
            &policy,
        )
        .unwrap();
        assert!(
            mask > length,
            "the fixture must be grazing enough that the mask consumes the member \
             (mask {mask} mm vs length {length} mm)"
        );
        let (erased, _) = surface
            .segment_clearance(
                &[p0],
                &[seated],
                &[r_max],
                &[0.35],
                policy.standoff_mm,
                &[0.0],
                &[mask],
            )
            .expect("kernel clearance for the over-long mask");
        assert!(
            erased[0].is_infinite(),
            "NON-VACUITY: an over-long mask must make the kernel answer INFINITY, \
             which is the admission PR-3 exists to prevent; kernel said {}",
            erased[0]
        );

        // PR-3 refuses it BY NAME instead.
        let (candidate, _) = model_anchor_candidate(p0, seated);
        assert_eq!(
            clearance_survival(&surface, &policy, &[candidate]).unwrap(),
            vec![ClearanceVerdict::GrazingIncidence]
        );
    }

    /// PR-2's degenerate case refuses rather than being floored, and refuses by
    /// its own name so a reader can tell it from a distance failure.
    #[test]
    fn a_degenerate_incidence_model_anchor_is_refused_by_name() {
        let surface = planar_slab();
        let policy = seat_contact_policy(0.05).unwrap();
        // Exactly tangential: the arm runs along the face, so |cos| = 0.
        let (candidate, _) = model_anchor_candidate([4.0, 0.0, -0.3], [0.0, 0.0, -0.3]);
        assert_eq!(
            clearance_survival(&surface, &policy, &[candidate]).unwrap(),
            vec![ClearanceVerdict::DegenerateIncidence]
        );
    }

    /// One candidate holding a single arm that declares NO model contact, so
    /// the insets are zero and the kernel grades its whole axis. This is the
    /// clean instrument for the DEMAND: nothing about a mask can move the
    /// answer.
    fn plain_candidate(p0: [f64; 3], p1: [f64; 3], radius_mm: f64) -> EvaluatedCandidate {
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let snapshot = Forest::new();
        let mut forest = snapshot.clone();
        forest.add_member(Member::new(MemberSpec {
            p0_mm: p0.map(finite),
            p1_mm: p1.map(finite),
            r0_mm: TerminalEndRadius::new(positive(radius_mm)),
            r1_mm: RootwardEndRadius::new(positive(radius_mm)),
            kind: MemberKind::Strut,
            owner: Owner::Contact(7),
            model_contact_end: None,
            carried_mm2: finite(2.0),
        }));
        let rows = clearance_rows(&forest, &snapshot);
        assert_eq!(rows.len(), 1, "exactly one changed member");
        assert!(
            rows[0].contact.is_none(),
            "this row declares no model contact, so no mask is in play"
        );
        EvaluatedCandidate {
            forest: forest.clone(),
            family: CandidateFamily::ModelAnchor,
            generation_index: 0,
            price_ml: 1.0,
            rows,
            anchor: None,
        }
    }

    /// **`clearance_survival` spends the PLACEMENT demand — as a test that goes
    /// RED on a sabotaged read** (challenge finding B3, 2026-08-22).
    ///
    /// Before this test, `cco.rs:3624` was one of two ruled tier reads with no
    /// test that could tell: the challenger rewrote `seat_contact_policy`'s
    /// `demanded_gap_mm` to `FINAL_CLEARANCE_GAP_MM` and the whole scoped
    /// estate — 590 lib tests plus five integration binaries — stayed GREEN.
    ///
    /// The fixture is the two-tier case §8 step 6 names, on the arm that
    /// declares NO contact so no mask can be blamed for the answer:
    ///
    /// * radius 0.45 mm, horizontal at `z = +0.90 mm` over a slab whose top
    ///   face is `z = 0`, so the TRUE gap is `0.90 - 0.45 = 0.45 mm` — read
    ///   back from the kernel at demand 0, not restated arithmetic;
    /// * `0.300 < 0.45 < 0.600` asserted from the REGISTRY, so the fixture is
    ///   provably a two-tier discriminator;
    /// * a control arm 2.0 mm up survives, so a uniformly refusing fixture
    ///   cannot pass this test by accident.
    ///
    /// The pin: production must return `Distance` for the middle arm. A site
    /// reading `FINAL_CLEARANCE_GAP_MM` would return `Survives`.
    #[test]
    fn clearance_survival_prunes_at_the_placement_demand_not_the_final_one() {
        let surface = planar_slab();
        let policy = seat_contact_policy(0.05).unwrap();
        let constants = m0_constants().unwrap();
        let placement_mm = constants.placeholders().standoff_mm().diagnostic().get();
        let final_mm = *constants.evidence().final_clearance_gap_mm().value();
        let radius_mm = 0.45_f64;

        // NON-VACUITY, from the kernel: at a demand of zero the number IS the
        // true gap, and it sits strictly between the two registered demands.
        let (true_gap, _) = surface
            .segment_clearance(
                &[[-2.0, 0.0, 0.9]],
                &[[2.0, 0.0, 0.9]],
                &[radius_mm],
                &[radius_mm],
                0.0,
                &[0.0],
                &[0.0],
            )
            .unwrap();
        assert!(
            (true_gap[0] - 0.45).abs() < 1e-9,
            "the fixture's TRUE gap must be 0.45 mm; the kernel said {}",
            true_gap[0]
        );
        assert!(
            final_mm < true_gap[0] && true_gap[0] < placement_mm,
            "the fixture must discriminate the two tiers: FINAL {final_mm} < \
             true gap {} < PLACEMENT {placement_mm}",
            true_gap[0]
        );

        let verdicts = clearance_survival(
            &surface,
            &policy,
            &[
                plain_candidate([-2.0, 0.0, 0.9], [2.0, 0.0, 0.9], radius_mm),
                plain_candidate([-2.0, 0.0, 2.0], [2.0, 0.0, 2.0], radius_mm),
            ],
        )
        .unwrap();
        assert_eq!(
            verdicts,
            vec![ClearanceVerdict::Distance, ClearanceVerdict::Survives],
            "this tournament prunes at PLACEMENT ({placement_mm} mm), so a \
             0.45 mm true gap is dropped here and a 1.55 mm one is kept. If \
             this site were switched to read FINAL ({final_mm} mm) the first \
             candidate would SURVIVE, which is exactly the sabotage this \
             assertion exists to catch."
        );

        // The read itself, stated separately and AFTER the behaviour, so the
        // behavioural half is what fails first under a sabotaged read and this
        // one still catches a demand that arrives by some other route.
        assert!(
            (policy.demanded_gap_mm - placement_mm).abs() < f64::EPSILON,
            "this stage's demand must BE `STANDOFF_MM` ({placement_mm} mm); the \
             policy carries {}",
            policy.demanded_gap_mm
        );
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
            .grow_terminal_for_test(&mut forest, &request, &surface, &growth_topology(), 0.05)
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
            controller.grow_terminal_for_test(
                &mut forest,
                &request,
                &surface,
                &growth_topology(),
                0.05
            ),
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
            model_contact_end: None,
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
                process_resolution_mm: 0.05,
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
        // Retirement refuses to strand live children as ungrounded roots, so
        // the leaf goes first; the root's slot is still the lowest free one.
        reused.retire(keys.child).unwrap();
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
        // **#51 — THIS ASSERTION FLIPPED, AND THE FLIP IS THE FIX.**
        // This fixture's model anchor used to be refused
        // `RewireCandidateRefusalReason::Clearance`, and it was refused for the
        // arithmetic reason, not a geometric one: with the retired flat
        // `standoff_mm` inset the last tested station sits exactly `standoff`
        // from a point ON the surface, so the clearance is at most `-r_max`
        // whatever the geometry is. With the PR-2 mask and the PR-7 seat the
        // same arm on the same surface routes.
        assert!(
            batch
                .ranked_candidates()
                .iter()
                .any(|candidate| candidate.evidence.family == CandidateFamily::ModelAnchor),
            "the model anchor must survive the converged mask"
        );
        assert!(
            !batch.refusals.iter().any(|refusal| {
                refusal.family == CandidateFamily::ModelAnchor
                    && refusal.reason == RewireCandidateRefusalReason::Clearance
            }),
            "and it must not still be refused for clearance"
        );
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
            contact_attestation_digest: 0,
            process_resolution_mm: 0.05,
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
        // 8.0, not 9.75. At 9.75 the anchor arm is 0.55 mm — a NUB, the class
        // #51's mask was the only reason ever won — and PR-3 now refuses it
        // because it cannot emerge from its own 1.6 mm contact mask. 8.0 puts
        // the face 2.3 mm from the stub end, which is a real arm.
        let detached_anchors = near_anchor_fixture(8.0);
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
        let root_anchors = near_anchor_fixture(13.0);
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

    /// A genuine [`ValidatedModelAnchor`] — minted by the real foot-ball
    /// validation, never assembled by the test, because its fields are private
    /// to `anchor.rs` precisely so a capability cannot be forged.
    fn genuine_validated_model_anchor() -> Box<ValidatedModelAnchor> {
        let anchors = near_anchor_fixture(8.0);
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        replay_family(&forest, &binding, &anchors, CandidateFamily::ModelAnchor)
            .validated_model_anchor
            .expect("a replayed model anchor carries its validated capability")
    }

    /// **#36 CREATION SITE 4 — a bifurcated upper half INHERITS the host's
    /// frozen zone; it never recomputes one** (§8 step 6, §9.4).
    ///
    /// `apply_bifurcation` splits a host at `junction`. The upper half keeps the
    /// host's `p1`, which IS the host's seat, so it is the same contact — and
    /// the zone belongs to the contact, not to the member. Recomputing it from
    /// the upper half's own interpolated radius would be the un-freezing the
    /// monotonicity theorem forbids (§3): a wider band swallows more surface, so
    /// a fatter half would be graded against strictly less of it.
    ///
    /// The assertion is `Arc::ptr_eq`, deliberately. Comparing CONTENTS would
    /// pass for a recompute that happened to land on the same faces; comparing
    /// ALLOCATIONS cannot. That is the whole difference between inheriting and
    /// re-deriving, and it is the sabotage
    /// `a_bifurcation_that_re_froze_the_upper_half_from_its_own_radius` this
    /// test exists to catch.
    ///
    /// The second half is hygiene with teeth: the lower half no longer declares
    /// a contact, so it must forgive NOTHING. A copy left behind would exempt
    /// faces for a member with no seat — the move is `take_seat_zone`, not a
    /// clone, for exactly that reason.
    #[test]
    fn a_bifurcated_upper_half_inherits_the_hosts_frozen_zone_rather_than_recomputing_it() {
        let (mut forest, request, _) = growth_fixture();
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let host = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(1.0), finite(0.0), finite(20.0)],
            p1_mm: [finite(1.0), finite(0.0), finite(0.0)],
            r0_mm: TerminalEndRadius::new(positive(0.45)),
            r1_mm: RootwardEndRadius::new(positive(0.65)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "bifurcation-inherit".to_owned(),
                ordinal: 0,
            },
            // The host's ROOTWARD end is its seat, and the upper half keeps it.
            model_contact_end: Some(ContactEnd::Rootward),
            carried_mm2: finite(0.0),
        }));
        let frozen: std::sync::Arc<[u32]> = std::sync::Arc::from(vec![3_u32, 11, 42]);
        forest.set_seat_zone(host, std::sync::Arc::clone(&frozen));

        let before = forest
            .live_tree_members()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let config = Config::current().unwrap();
        // Through the production entry point, so the SIZING pass runs too: the
        // upper half's radii are rewritten after the split, and the frozen zone
        // must not follow them.
        assert!(
            apply_and_size(
                &mut forest,
                &request,
                &Descriptor::Bifurcate { host },
                &config
            )
            .unwrap(),
            "NON-VACUITY: the fixture must actually bifurcate, or nothing below \
             is being measured"
        );

        // The upper half is the new member that kept the host's `p1`.
        let host_p1 = [finite(1.0), finite(0.0), finite(0.0)];
        let upper = forest
            .live_tree_members()
            .into_iter()
            .find(|key| {
                !before.contains(key)
                    && forest
                        .member(*key)
                        .is_some_and(|member| member.p1_mm() == host_p1)
            })
            .expect("the split mints an upper half that keeps the host's p1");
        assert_eq!(
            forest.member(upper).unwrap().model_contact_end(),
            Some(ContactEnd::Rootward),
            "NON-VACUITY: the upper half must be the one that inherited the \
             contact declaration"
        );

        let inherited = forest
            .seat_zone(upper)
            .expect("the upper half must carry the contact's frozen zone");
        assert!(
            std::sync::Arc::ptr_eq(inherited, &frozen),
            "the upper half must inherit the very same allocation — an `Arc` \
             move, not a recompute. Contents that merely MATCH would also pass a \
             re-derivation that happened to land on the same faces; identity \
             cannot. Sabotage: \
             `a_bifurcation_that_re_froze_the_upper_half_from_its_own_radius`."
        );
        assert!(
            forest.seat_zone(host).is_none(),
            "and the lower half, which no longer declares a contact, must \
             forgive NOTHING — the zone MOVES with the declaration. A copy left \
             behind would exempt faces for a member with no seat."
        );
    }

    /// **PR-7 acceptance — the seat witness now refuses at CANDIDACY.**
    ///
    /// This test used to be `a_winning_but_unattested_model_anchor_is_refused_rather_than_committed`,
    /// and it pinned the TRIPWIRE: the ruling then was that PR-8 `NotAttached`
    /// is RECORDED during candidacy for the CCO model-anchor family and
    /// ENFORCED at commit by `elect_committable_winner`, because the family was
    /// deliberately unseated and refusing an unseated kiss at candidacy would
    /// have landed PR-7's family-killing consequence without landing PR-7.
    ///
    /// **`cco::apply_model_anchor` now seats the rootward end, so that reason is
    /// gone and so is the gate.** `NotAttached` is an ordinary defect for a
    /// seated family, dispositioned exactly as OD-9 burial and FU-88 unusable
    /// geometry already were, and the winner election is pure price order
    /// again. What this test pins now is the two halves of that move:
    ///
    /// 1. the disposition table — all three attestation verdicts refuse, each
    ///    through the family's own `AnchorFootRefusal` channel, with the
    ///    provenance the seat was discovered at; and
    /// 2. NON-VACUITY, which is now the more important half: on the frozen
    ///    three-candidate tournament the model anchor is genuinely the winner,
    ///    so a refusal here really does remove a candidate that was going to
    ///    commit, and the election really is the last word.
    #[test]
    fn the_seat_witness_refuses_a_model_anchor_at_candidacy() {
        let (mut forest, request, _) = growth_fixture();
        let finite = |value| Finite::new(value).unwrap();
        let positive = |value| PositiveFinite::new(value).unwrap();
        let host = forest.add_member(Member::new(MemberSpec {
            p0_mm: [finite(1.0), finite(0.0), finite(20.0)],
            p1_mm: [finite(1.0), finite(0.0), finite(0.0)],
            r0_mm: TerminalEndRadius::new(positive(0.45)),
            r1_mm: RootwardEndRadius::new(positive(0.65)),
            kind: MemberKind::Trunk,
            owner: Owner::Generated {
                family: "tripwire-host".to_owned(),
                ordinal: 0,
            },
            model_contact_end: None,
            carried_mm2: finite(0.0),
        }));
        let config = Config::current().unwrap();
        let descriptors = [
            Descriptor::Plate {
                xy: [0.0, 0.0],
                top_z: 12.5,
            },
            Descriptor::ModelAnchor {
                seat_index: 0,
                point_mm: [3.0, 0.0, 5.0],
                // Hand-built descriptor: PR-7's seat is derived in
                // `generate_descriptors`, so a descriptor written here carries
                // whatever seated point the author states. `seated == raw` is
                // the UNSEATED geometry on purpose — it keeps this vector
                // pinning the family equations and the generation/price ledger
                // it was frozen from, rather than silently re-freezing them
                // against a new endpoint. PR-7's seating is exercised where it
                // lives, by `generate_descriptors` and its own tests.
                seated_point_mm: [3.0, 0.0, 5.0],
                // Hand-built descriptor: this vector pins the family equations
                // and the generation/price ledger, not the seat exemption, so
                // it forgives nothing. An empty zone is the STRICT direction.
                seat_zone: std::sync::Arc::from(Vec::new()),
            },
            Descriptor::Bifurcate { host },
        ];
        let candidates = descriptors
            .iter()
            .enumerate()
            .map(|(index, descriptor)| {
                evaluate(&forest, &request, descriptor, index, &config)
                    .unwrap()
                    .expect("every frozen candidate is feasible")
            })
            .collect::<Vec<_>>();
        let mut survivors = candidates.iter().collect::<Vec<_>>();
        survivors.sort_by(|left, right| compare_candidates(left, right));

        // NON-VACUITY, stated first: the model anchor really is this
        // tournament's winner. If it ever stops being, this test would be
        // asserting the gate on a candidate that was never going to commit.
        assert_eq!(survivors[0].family, CandidateFamily::ModelAnchor);
        assert_eq!(survivors[1].family, CandidateFamily::Bifurcation);

        // (1) The disposition table. Every verdict that refuses does so through
        // the family's EXISTING channel, carrying the seat's provenance,
        // exactly as a breached foot ball reports — and `NotAttached` is now
        // in that table rather than deferred to a commit gate.
        let anchor = genuine_validated_model_anchor();
        let provenance = anchor.seat().provenance();
        for (kind, expected) in [
            (
                SeatAttestationRefusalKind::Buried,
                AnchorFootRefusalReason::SeatBuriedSurface,
            ),
            (
                SeatAttestationRefusalKind::Unusable,
                AnchorFootRefusalReason::SeatGeometryUnusable,
            ),
            (
                SeatAttestationRefusalKind::NotAttached,
                AnchorFootRefusalReason::SeatNotAttached,
            ),
        ] {
            assert_eq!(kind.reason(), expected);
            assert_eq!(kind.refusal(provenance).reason(), expected);
            assert_eq!(kind.refusal(provenance).provenance(), provenance);
        }

        // (2) With no refusal in play the election is pure price order and the
        // model anchor takes it. There is no longer any gate between this and
        // `apply_and_size`, which is what makes (1) load-bearing.
        assert_eq!(survivors[0].family, CandidateFamily::ModelAnchor);
    }
    /// **PR-3 acceptance on the rewire path — the NUB class dies, by name.**
    ///
    /// This test used to be
    /// `a_winning_but_unattested_rewire_model_anchor_is_refused_rather_than_replayed`
    /// and pinned the rewire half of the TRIPWIRE:
    /// `RewireCandidateBatch::winning_generation_index` held a WINNING model
    /// anchor to the PR-8 seat witness at commit, because the family was
    /// unseated and `NotAttached` was a statement about a missing rule.
    ///
    /// PR-7 seats the family, so its unseated-kiss premise is gone: on this
    /// fixture the seat now attests. What refuses this candidate instead is
    /// PR-3, and the reason is worth stating plainly because it is the whole
    /// FU-87 relationship in one fixture:
    ///
    /// `near_anchor_fixture(9.75)` puts the face 0.25 mm from the stub end, so
    /// the anchor arm is **0.55 mm after seating**. Every model-anchor arm that
    /// ever won in corpus history was 0.054-0.596 mm, because the retired flat
    /// `standoff_mm` inset admitted NOTHING ELSE: an arm longer than 0.600 mm
    /// was measured and refused by exactly its own radius, and an arm shorter
    /// than it was never measured at all (`seg_clear_one_excluding` returns `INFINITY`).
    /// The mask convergence closes that escape hatch, and PR-3 refuses the nub
    /// **by name** — `SeatGrazingIncidence`, not an anonymous clearance
    /// failure — because a 0.55 mm arm cannot emerge from its own 1.6 mm
    /// contact mask.
    ///
    /// Nothing here is forged. The anchor clears the foot ball and the OD-9
    /// burial screen; the refusal is the real screen against the real surface.
    #[test]
    fn a_model_anchor_nub_is_refused_because_it_cannot_emerge() {
        let anchors = near_anchor_fixture(9.75);
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();

        with_rewire_batch(&binding, &anchors, |mut batch, geometry| {
            let refused = batch
                .refusals()
                .iter()
                .filter(|refusal| {
                    refusal.family == CandidateFamily::ModelAnchor
                        && refusal.reason
                            == RewireCandidateRefusalReason::AnchorFoot(
                                AnchorFootRefusalReason::SeatGrazingIncidence,
                            )
                })
                .map(|refusal| refusal.generation_index)
                .collect::<Vec<_>>();
            // NON-VACUITY: the fixture must actually produce the verdict.
            assert_eq!(
                refused.len(),
                1,
                "the nub fixture must produce exactly one PR-3 refusal, or this \
                 test asserts nothing"
            );
            // Refused at CANDIDACY: the candidate is not in the batch at all,
            // so no later caller can elect or replay it. Nothing hard-fails —
            // the move simply does not happen (OD-4/PR-9).
            let probe = refused[0];
            assert!(batch
                .ranked_candidates()
                .iter()
                .all(|candidate| candidate.evidence.generation_index != probe));
            assert_ne!(batch.winning_generation_index(), Some(probe));
            assert!(matches!(
                batch.replay_candidate(&binding, &forest, probe, geometry),
                Err(CcoError::RewireCandidateNotFound(index)) if index == probe
            ));
        });
    }

    /// **PR-7 acceptance — the seat witness stops reporting a kiss.**
    ///
    /// The measurement that licensed PR-7 was that every attested CCO anchor
    /// seat on the corpus reported `centreline = false` with 1-3 of 8 rim rays:
    /// the witness was correctly reporting that the arm KISSED the surface,
    /// because `apply_model_anchor` set `p1` to the raw seat point.
    ///
    /// With the endpoint seated at the PR-1 depth and the witness measuring the
    /// SAME point the strut will be minted at, a perpendicular approach into
    /// solid material attests. This is the direct behavioural consequence of
    /// Piece A, and it is what makes `NotAttached` an ordinary defect that can
    /// refuse at candidacy rather than a statement about a missing rule.
    #[test]
    fn a_seated_model_anchor_attests_instead_of_kissing() {
        let anchors = near_anchor_fixture(8.0);
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        with_rewire_batch(&binding, &anchors, |batch, _| {
            // NON-VACUITY: the anchor really is in this batch.
            assert!(batch
                .ranked_candidates()
                .iter()
                .any(|candidate| candidate.evidence.family == CandidateFamily::ModelAnchor));
            // ... and no seat verdict refused it.
            assert!(
                !batch.refusals().iter().any(|refusal| {
                    matches!(
                        refusal.reason,
                        RewireCandidateRefusalReason::AnchorFoot(
                            AnchorFootRefusalReason::SeatNotAttached
                        )
                    )
                }),
                "a seated perpendicular anchor must attest, not report a kiss"
            );
        });
    }

    /// The refusal ledger stays sorted by generation index, which is the order
    /// the stage's quarantine vocabulary is kept in.
    #[test]
    fn rewire_refusals_are_kept_in_generation_order() {
        let anchors = near_anchor_fixture(9.75);
        let (forest, keys) = rewire_fixture();
        let binding = RewireCcoBinding::bind_detached_subtree(
            &forest,
            keys.root,
            BuildFrame::new(Finite::new(0.0).unwrap(), PositiveFinite::new(5.0).unwrap()),
        )
        .unwrap();
        with_rewire_batch(&binding, &anchors, |batch, _| {
            assert!(batch
                .refusals()
                .windows(2)
                .all(|pair| pair[0].generation_index <= pair[1].generation_index));
        });
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

    /// A refused model anchor leaves no replayable capability behind.
    ///
    /// The fixture moved with #51: `rewire_anchor_fixture`'s anchor used to be
    /// refused for clearance and now routes, so the refused case is taken from
    /// the nub fixture, which PR-3 refuses. The property under test — a refused
    /// candidate is GONE, not merely unranked — is unchanged.
    #[test]
    fn refused_model_anchor_has_no_replay_capability() {
        let anchors = near_anchor_fixture(9.75);
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
            model_contact_end: None,
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
            model_contact_end: None,
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
                // Hand-built descriptor: PR-7's seat is derived in
                // `generate_descriptors`, so a descriptor written here carries
                // whatever seated point the author states. `seated == raw` is
                // the UNSEATED geometry on purpose — it keeps this vector
                // pinning the family equations and the generation/price ledger
                // it was frozen from, rather than silently re-freezing them
                // against a new endpoint. PR-7's seating is exercised where it
                // lives, by `generate_descriptors` and its own tests.
                seated_point_mm: [3.0, 0.0, 5.0],
                // Hand-built descriptor: this vector pins the family equations
                // and the generation/price ledger, not the seat exemption, so
                // it forgives nothing. An empty zone is the STRICT direction.
                seat_zone: std::sync::Arc::from(Vec::new()),
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
            .grow_terminal_for_test(&mut forest, &request, &surface, &growth_topology(), 0.05)
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
            .grow_terminal_for_test(&mut forest, &request, &surface, &growth_topology(), 0.05)
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
                .grow_terminal_for_test(&mut forest, &request, &surface, &growth_topology(), 0.05)
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

    fn routed_row(candidates_ruled_out: usize) -> FinalizedCcoTranscriptRow {
        let original_key = MemberKey::transit_test_fixture(4, 0);
        FinalizedCcoTranscriptRow {
            cadence_index: 0,
            physical_contact_id: 4,
            terminal_id: 4,
            original_key,
            kind: FinalizedDispositionKind::Routed,
            route: Some(TerminalRouteEvidence {
                physical_contact_id: 4,
                terminal_id: 4,
                original_key,
                plan_digest: 1,
                loads_digest: 2,
                geometry_digest: 3,
                forest_digest_before: 4,
                controller_digest_before: 5,
            }),
            skipped: None,
            no_winner_reason: None,
            candidates_ruled_out,
        }
    }

    /// **Craig's authorization item 5 — the second counter is really sealed.**
    ///
    /// Two rows that differ in nothing but the ruled-out count must not seal
    /// to the same fingerprint. If they did, the counter would be disclosure
    /// with no seal behind it, and a run could report any number it liked.
    #[test]
    fn the_ruled_out_counter_is_folded_into_the_sealed_transcript_fingerprint() {
        assert_ne!(
            transcript_rows_digest(&[routed_row(0)]),
            transcript_rows_digest(&[routed_row(1)]),
            "the ruled-out count must reach the sealed fingerprint"
        );
    }

    /// **The fold is UNCONDITIONAL, and this is the guard on the optimization
    /// that would undo the ruling.**
    ///
    /// The cheap-looking edit is `if row.candidates_ruled_out != 0 { word(..) }`
    /// — it would keep the fingerprint still for every model that rules nothing
    /// out, which is exactly what Craig declined, and it would make the
    /// one-time corpus-wide re-freeze this lane paid unrepeatable. The golden
    /// below is the digest of a single zero-count row: skip the zero fold and
    /// it moves.
    #[test]
    fn a_zero_ruled_out_count_still_occupies_its_place_in_the_sealed_stream() {
        assert_eq!(
            transcript_rows_digest(&[routed_row(0)]),
            11_947_743_038_542_081_092,
            "a zero count is folded like any other: the stream position is part of the seal"
        );
    }
}
