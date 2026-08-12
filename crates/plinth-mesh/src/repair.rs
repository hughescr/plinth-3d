//! The automatic, census-scoped repair pre-pass.
//!
//! **What overturned the old posture.** D-22 said the pipeline deletes the
//! Blender geometry sweep and *refuses* bad geometry, with the owner repairing
//! upstream by hand. The owner ruling of 2026-08-10 revised that: repair is
//! built into the Rust pipeline, fully automatic, no manual steps
//! (`docs/engineering-journal/decisions/2026-08-10-d22-revision-automatic-repair.md`).
//! What the ruling did **not** revise is the gate: repair earns no exemptions,
//! lands strictly upstream of the closed-solid gate, and its output has to pass
//! that gate unchanged.
//!
//! **Scope is the census, and nothing else.** A verified harness reproduced the
//! pipeline gate on all 21 refused/suspect models and found exactly three
//! defect classes across the whole corpus: 113 pinch vertices, 3
//! duplicate-face groups, and ~10 dust shards. Every model is watertight, every
//! model is consistently oriented, none has a non-manifold edge. So this module
//! repairs those three things and deliberately implements no speculative
//! fourth: hole filling, re-winding and degenerate collapse have no corpus
//! evidence behind them, and the edge-collapse-never-dissolve doctrine binds
//! any future degenerate work.
//!
//! **The three actions, in the one order that is safe.**
//!
//! 1. **Dust-shard strip**, delegated whole to [`crate::triage_components`] —
//!    the same six-criteria conjunction the hollow stage already runs, applied
//!    to raw input instead of only to a post-gate shell. Keep-largest stays
//!    forbidden and no criterion moves; the only new thing is *when* it runs.
//! 2. **Exact-duplicate-face removal**, because a duplicate severs the
//!    adjacency the pinch analysis is computed from. Running the split first
//!    would split fans that only look separate because a duplicate is standing
//!    between them.
//! 3. **Pinch-vertex split**, re-derived from a fresh topology whenever step 1
//!    or 2 changed a face, for the same reason.
//!
//! **The pass may decline; it may never refuse.** [`repair_mesh`] returns a
//! [`RepairedMesh`] and not a `Result`, and that signature is the invariant
//! rather than a note about one: "declining to repair is always available;
//! refusing a model never is" (the 2026-08-10 ruling, FU-45). This stage is
//! mandatory and unconditional on the `prepare` chain, so *any* internal
//! failure it could raise — a resource bound, a topology build, a triage
//! arithmetic path, the constants registry — would be a **new refusal class
//! invented by a repair**, which is the exact inversion of the ruling. So the
//! whole pass, all three actions, is gated on one admission check up front, and
//! every internal failure below it lands in
//! [`RepairEvidence::actions_skipped`]: the caller gets its own mesh back, bit
//! for bit, and the unchanged gates downstream judge exactly what they judged
//! before this stage existed.
//!
//! **Byte-identity is a contract, not an optimization.** A mesh with none of
//! the three defects is returned exactly as it arrived — same vertex order,
//! same triangle order, same bits — so a clean model cannot be perturbed by a
//! stage that had nothing to do. `repaired_clean_mesh_is_byte_identical` proves
//! it through a binary-STL round trip, not by inspection.
//!
//! **Determinism.** Nothing here reads a clock, an environment variable, or an
//! unseeded RNG, and nothing here is parallel: every collection is a `BTree*`
//! or an explicitly sorted `Vec`, and the fan order comes from
//! [`crate::FaceTopology::point_only_vertices`], which is totally ordered by
//! construction. There is therefore no thread-count sweep to run on this stage
//! — `repairing_twice_is_bit_identical` pins the property the sweep would be
//! testing for.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::{m0_constants, M0ConstantsError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::topology::{build_face_topology, TopologyError};
use crate::triage::{
    TriageAdmissionError, TriageError, TriageTopologyAdmission, TriageTopologyLimits,
};
use crate::{Mesh, MeshError, WeldPolicy};

/// One repaired mesh and the complete record of what was done to it.
#[derive(Clone, Debug, PartialEq)]
pub struct RepairedMesh {
    mesh: Mesh,
    evidence: RepairEvidence,
}

impl RepairedMesh {
    /// The mesh downstream stages should use. Bit-identical to the input when
    /// [`RepairEvidence::changed`] is false.
    #[must_use]
    pub const fn mesh(&self) -> &Mesh {
        &self.mesh
    }

    #[must_use]
    pub fn into_mesh(self) -> Mesh {
        self.mesh
    }

    #[must_use]
    pub const fn evidence(&self) -> &RepairEvidence {
        &self.evidence
    }

    /// Consumes the pair, for a caller that needs both halves by value.
    #[must_use]
    pub fn into_parts(self) -> (Mesh, RepairEvidence) {
        (self.mesh, self.evidence)
    }
}

/// Everything the pre-pass did to one model, per class and per action.
///
/// A count alone would not survive review here: "3 faces removed" cannot be
/// checked against the model, and cannot tell a later reader whether the
/// removal was a duplicate pair collapsing to one face or an
/// opposite-wound pair cancelling to none. Each class therefore carries its
/// own per-action rows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepairEvidence {
    pub source_faces: usize,
    pub source_vertices: usize,
    pub repaired_faces: usize,
    pub repaired_vertices: usize,
    pub shards: ShardStripEvidence,
    pub duplicates: DuplicateFaceEvidence,
    pub pinches: PinchSplitEvidence,
    /// The cut-flag census of the repaired mesh, measured only when something
    /// was repaired. `Some(x)` with a nonzero field is the honest statement
    /// that the pre-pass did not finish the job and the gate is about to say
    /// so; it is not an error and never suppresses the gate.
    pub residual: Option<ResidualTopology>,
    /// Set when the pass planned its way to a mesh with **no triangles at
    /// all** and refused to apply that plan.
    ///
    /// When this is `Some`, none of the rows above were applied: they record
    /// what the actions *found*, and the returned mesh is the input, bit for
    /// bit. See [`EmptyResultRefusal`].
    pub refused_to_empty: Option<EmptyResultRefusal>,
    /// Set when **no action ran at all**, with the reason.
    ///
    /// This is the one place a stage-internal failure can surface, because it
    /// is the only place it is allowed to: the raw input is over the pass's
    /// resource bound, or the topology, the triage or the constants registry
    /// failed. Every row above is then empty by construction — they record what
    /// the actions found, and no action looked — and the returned mesh is the
    /// input, bit for bit.
    ///
    /// **It is not a refusal and must never be read as one.** The model goes on
    /// to every stage below exactly as it did before this pre-pass existed, and
    /// the unchanged gates judge it on its own geometry. What is lost is
    /// *repair*, not admission: a model that would have been rescued here is
    /// refused downstream for the defect it still carries, which is a capability
    /// gap recorded on the card (FU-45) rather than a new refusal class.
    pub actions_skipped: Option<String>,
}

impl RepairEvidence {
    /// Whether any action was **applied**. False means the returned mesh is
    /// the input, bit for bit — which includes the refuse-to-empty case, where
    /// actions were planned and then deliberately not applied, and the
    /// actions-skipped case, where no action ran at all.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.refused_to_empty.is_none()
            && self.actions_skipped.is_none()
            && (!self.shards.faces_removed.is_empty()
                || !self.duplicates.groups.is_empty()
                || !self.pinches.splits.is_empty())
    }

    /// A one-line rendering for a run report.
    ///
    /// **The dispositions are on the card, not just the counts.** Every group
    /// keeps exactly one survivor — the first by deterministic order, which is
    /// the only rule the spec authorized — so the removal count cannot say
    /// whether the reduction actually *resolved* anything. `dup-keep-one`
    /// counts the groups where keeping the first left the group's three edges
    /// walked once in each direction; `dup-unresolved` counts the ones where it
    /// did not, and which therefore reach the unchanged gate carrying a
    /// residual. That distinction is the work item (FU-46), and it is invisible
    /// in a face count.
    #[must_use]
    pub fn summary_line(&self) -> String {
        let mut line = format!(
            "shard-faces={} dup-groups={} dup-faces={} dup-keep-one={} dup-unresolved={} pinch-vertices={} pinch-copies={} unrepaired-pinches={}",
            self.shards.faces_removed.len(),
            self.duplicates.groups.len(),
            self.duplicates.faces_removed(),
            self.duplicates.groups_with(DuplicateDisposition::KeepOne),
            self.duplicates
                .groups_with(DuplicateDisposition::KeptFirstUnresolved),
            self.pinches.splits.len(),
            self.pinches.copies_added(),
            self.pinches.unrepaired.len(),
        );
        if self.refused_to_empty.is_some() {
            line.push_str(" APPLIED=none (refused: the plan would have emptied the mesh)");
        }
        if let Some(reason) = &self.actions_skipped {
            line.push_str(&format!(" APPLIED=none (skipped: {reason})"));
        }
        line
    }
}

/// The plan the pass refused to apply because applying it would have left a
/// mesh with no triangles at all.
///
/// **A pre-pass may not empty a model.** Downstream, an empty face array is not
/// even a `Mesh`: construction fails and the operator gets a stage-internal
/// error attributed to the input, with no record of what was deleted or why.
/// More importantly it is the wrong answer — "every triangle in this file was
/// removable" is a finding about the *pass*, not a repaired model. So the
/// actions are rolled back whole, the input is returned untouched, and the
/// unchanged gate refuses the model with this evidence attached.
///
/// **Under the rules as landed this cannot fire, and it is still checked.** The
/// shard strip never strips the main component, the duplicate rule keeps the
/// first member of every group, and the pinch split only ever appends — so
/// every action leaves at least one face by construction. The check stays
/// because "by construction" is an argument about today's three actions and the
/// consequence of being wrong about it is a stage-internal error wearing the
/// operator's filename. FU-46's open question is precisely whether an action
/// that deletes a group with **no survivor** should exist; the day it is ruled
/// in, this guard is what stops it emptying a model, and it is far easier to
/// keep than to remember to add back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyResultRefusal {
    /// Faces the input arrived with.
    pub source_faces: usize,
    /// Faces the shard strip would have removed.
    pub shard_faces_planned: usize,
    /// Faces the duplicate action would have removed.
    pub duplicate_faces_planned: usize,
}

/// Action 1: the six-criteria shard strip, run on raw input.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShardStripEvidence {
    /// Welded component ids the triage criteria classified as dust.
    pub components_stripped: Vec<usize>,
    /// Source face indices removed, ascending.
    pub faces_removed: Vec<usize>,
    /// Components triage saw at all, for scale.
    pub components_seen: usize,
    /// Set when triage declined to classify this input rather than finding no
    /// shards: a pre-hollowed model is the hollow stage's refusal to make, not
    /// this pre-pass's, so the strip is skipped and said so.
    pub skipped: Option<String>,
}

/// Action 2: exact-duplicate-face removal, one row per duplicate group.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateFaceEvidence {
    pub groups: Vec<DuplicateFaceGroup>,
    /// Groups left alone because their welded triple repeats a vertex. That is
    /// a degenerate face, which is a different defect class with a different
    /// doctrine (edge-collapse-never-dissolve) and no corpus incidence.
    pub degenerate_groups_skipped: usize,
}

impl DuplicateFaceEvidence {
    #[must_use]
    pub fn faces_removed(&self) -> usize {
        self.groups
            .iter()
            .map(|group| group.removed.len())
            .sum::<usize>()
    }

    /// How many groups were decided by one particular rule.
    #[must_use]
    pub fn groups_with(&self, disposition: DuplicateDisposition) -> usize {
        self.groups
            .iter()
            .filter(|group| group.disposition == disposition)
            .count()
    }
}

/// One set of faces sharing a welded vertex triple, and what became of them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateFaceGroup {
    /// The shared welded triple, ascending.
    pub welded_triple: [u32; 3],
    /// Members wound the same way as the ascending triple.
    pub forward: usize,
    /// Members wound the opposite way.
    pub reverse: usize,
    /// The members that survive, earliest-first in source order.
    pub kept: Vec<usize>,
    /// The members dropped, ascending in source order.
    pub removed: Vec<usize>,
    /// Which rule decided `kept`, and whether the decision was reasoned or
    /// merely defaulted.
    pub disposition: DuplicateDisposition,
}

/// What keeping the first member of one duplicate group actually achieved.
///
/// **Both variants keep the same face.** The spec authorized exactly one rule —
/// "drop faces sharing an identical vertex triple, keeping the first by
/// deterministic order" — so this enum does not choose *which* member lives; it
/// records whether the one rule left the group's neighbourhood in a state a
/// closed solid can be in. A count of removed faces cannot say that, and it is
/// the difference between a group this pass finished and one the gate is about
/// to refuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateDisposition {
    /// The survivor leaves every edge of the triple walked exactly once in each
    /// direction — the coincident copies were sitting on real surface, and
    /// reducing them to one is the whole repair.
    KeepOne,
    /// The survivor does **not** leave the local edges clean, so the group is a
    /// residual: the plain rule still stood and the first member was still
    /// kept, but the shape wants a wider answer than the spec authorized and
    /// the unchanged gate gets to refuse it.
    ///
    /// **This is deliberately not a repair.** The two wider answers the review
    /// examined — deleting an edge-isolated coincident stack outright with no
    /// survivor, and keeping a *later*, opposite-wound member instead of the
    /// first — both delete or re-orient geometry on rules the spec did not
    /// authorize and the census found zero corpus incidence for. They are the
    /// open scope question on FU-46, **owner ruling requested**; until it lands
    /// the pass keeps the first, records the shape here, and the refusal is the
    /// work item.
    KeptFirstUnresolved,
}

/// Action 3: pinch-vertex splitting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinchSplitEvidence {
    pub splits: Vec<PinchSplit>,
    /// Pinches this pass could not open, with the reason. These are not
    /// errors: the gate will refuse the model and the refusal is the work item
    /// (the standing doctrine of the 2026-08-10 ruling).
    pub unrepaired: Vec<UnrepairedPinch>,
}

impl PinchSplitEvidence {
    /// New vertices introduced. One per fan beyond the first, per pinch.
    #[must_use]
    pub fn copies_added(&self) -> usize {
        self.splits
            .iter()
            .map(|split| split.fans.len())
            .sum::<usize>()
    }
}

/// One pinch vertex opened into per-fan copies.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinchSplit {
    pub welded_vertex: u32,
    /// The position every fan shared before the split.
    pub position_mm: [f64; 3],
    /// Faces left at the original position — fan 0, which never moves, so the
    /// repair is a strict addition to the vertex array.
    pub anchored_faces: usize,
    /// One row per moved fan, in fan order.
    pub fans: Vec<PinchFanMove>,
}

/// One moved fan: where it went, how far, and by how many ULP steps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinchFanMove {
    pub faces: usize,
    /// The fan's mean unit normal, the direction the copy moved along.
    pub direction: [f64; 3],
    /// How many f32-ULP steps the escalation needed. Always a power of two.
    pub ulp_steps: u32,
    /// The realized displacement. Bounded by `REPAIR_PINCH_SPLIT_MAX_MM`.
    pub displacement_mm: f64,
}

/// A pinch the pass declined to open, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum UnrepairedPinch {
    /// No displacement at or below the registered cap separated this fan under
    /// both the exact-bit weld and the f32 STL round trip. The cap is a
    /// boundary, not a knob: this is reported, never widened.
    ///
    /// `largest_attempt_mm` is the largest displacement actually *tried*, so
    /// it is nonzero whenever this variant is used: a refusal that reported
    /// `0.0` for a candidate it never tested would be misleading provenance on
    /// exactly the path the standing doctrine turns into work items. The
    /// "nothing could be tried at all" case has its own variant,
    /// [`Self::OneUlpExceedsCap`].
    DisplacementCapped {
        welded_vertex: u32,
        cap_mm: f64,
        largest_attempt_mm: f64,
    },
    /// One f32 ULP at this vertex's own coordinate magnitude is already larger
    /// than the registered cap, so **no candidate displacement exists** below
    /// the cap and none was tried.
    ///
    /// This is a structural statement about the coordinate scale, not a near
    /// miss: the minimum step that survives the exact-bit weld grows with the
    /// magnitude of the coordinate it is applied to (about 6.1e-5 mm for a
    /// coordinate in [512, 1024) mm), while the cap is absolute. A raw STL at
    /// that scale — repair runs before any scale sanity check — makes the
    /// pinch split a structural no-op, and it is named here rather than
    /// disguised as an ordinary capped attempt. The answer is a ruling about
    /// the input's units or about the cap's provenance, never a wider cap
    /// applied quietly.
    OneUlpExceedsCap {
        welded_vertex: u32,
        cap_mm: f64,
        ulp_mm: f64,
    },
    /// The fans at this vertex are held apart by an edge the topology could not
    /// accept — a non-manifold junction, a boundary, a duplicate — and not by a
    /// point contact.
    ///
    /// Splitting one of those apart does not open a pinch, it tears a junction
    /// into a crack: measured, a box with a fin sharing one edge goes
    /// `nonmanifold_edges` 1 -> 0 and `boundary_edges` 2 -> 3 while the
    /// evidence calls it an ordinary pinch split. The census found zero
    /// non-manifold edges in the corpus and the module's scope is the three
    /// classes it *did* find, so this is refused and reported: an
    /// edge-severed fan is a work item for whoever owns that defect class, not
    /// a fourth repair invented here. It becomes reachable the moment this
    /// pass is run post-decimation, where decimation is already recorded as
    /// manufacturing non-manifold edges.
    FanSeveredByEdge { welded_vertex: u32, edge: [u32; 2] },
    /// The fan's face normals sum to zero length, so it has no mean direction
    /// to move along.
    NoFanDirection { welded_vertex: u32 },
    /// A face reaches this vertex through more than one of its own corners, so
    /// the face is degenerate and belongs to a class this pass does not touch.
    DegenerateIncidence { welded_vertex: u32, face: u32 },
    /// Splitting this pinch would need a vertex index past `u32::MAX`. The
    /// pinch is left whole rather than split partway, because a vertex with
    /// some fans moved and some not is worse than the pinch was.
    VertexIndexExhausted { welded_vertex: u32 },
}

/// The repaired mesh's own cut-flag census, measured after the fact.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResidualTopology {
    pub boundary_faces: usize,
    pub nonmanifold_faces: usize,
    pub same_direction_faces: usize,
    pub duplicate_faces: usize,
    pub point_only_faces: usize,
    pub degenerate_faces: usize,
}

impl ResidualTopology {
    /// Whether the repaired mesh carries no cut flag at all — the same
    /// condition the closed-solid gate applies, measured here for evidence
    /// only. This is a diagnostic; the gate remains the authority and is run
    /// unchanged downstream.
    #[must_use]
    pub const fn is_clean(self) -> bool {
        self.boundary_faces == 0
            && self.nonmanifold_faces == 0
            && self.same_direction_faces == 0
            && self.duplicate_faces == 0
            && self.point_only_faces == 0
            && self.degenerate_faces == 0
    }
}

/// Why the pre-pass ran **no** action on a mesh.
///
/// Deliberately private and deliberately not a `RepairError`: there is no such
/// thing as a repair failure the caller has to handle, because there is no
/// disposition this stage is allowed to produce other than "here is a mesh".
/// Every variant below is rendered into [`RepairEvidence::actions_skipped`] and
/// the caller's own mesh is handed back untouched.
#[derive(Clone, Debug, Error, PartialEq)]
enum RepairSkipped {
    #[error("the raw input is over the pre-pass's resource bound ({0})")]
    Admission(#[from] TriageAdmissionError),
    #[error("the shard triage failed internally ({0})")]
    Triage(TriageError),
    #[error("the welded topology could not be built ({0})")]
    Topology(#[from] TopologyError),
    #[error("an intermediate mesh could not be constructed ({0})")]
    Mesh(#[from] MeshError),
    #[error("the M0 constants registry could not be published ({0})")]
    Constants(&'static M0ConstantsError),
}

/// Repair one loaded mesh, before anything downstream judges it.
///
/// Returns the input unchanged, bit for bit, when the census's three defect
/// classes are all absent — and **also** when the pass could not run at all, in
/// which case [`RepairEvidence::actions_skipped`] says why. There is no error
/// return, and that is the invariant, not an omission: this stage is mandatory
/// on the `prepare` chain, so any failure it could raise would be a new hard
/// refusal invented by a repair. A defect this pass cannot fix is likewise not
/// an error — it is recorded in [`RepairEvidence`] and left for the unchanged
/// gate.
#[must_use]
pub fn repair_mesh(mesh: &Mesh) -> RepairedMesh {
    repair_mesh_within(mesh, TriageTopologyLimits::production())
}

/// [`repair_mesh`] with the resource bound named, so the skip path can be
/// exercised by a test without moving the production ceiling.
fn repair_mesh_within(mesh: &Mesh, limits: TriageTopologyLimits) -> RepairedMesh {
    match run_repair(mesh, limits) {
        Ok(repaired) => repaired,
        Err(reason) => RepairedMesh {
            mesh: mesh.clone(),
            evidence: RepairEvidence {
                source_faces: mesh.tris().len(),
                source_vertices: mesh.verts().len(),
                repaired_faces: mesh.tris().len(),
                repaired_vertices: mesh.verts().len(),
                actions_skipped: Some(format!(
                    "{reason}; all three repair actions are skipped, no model is refused for it, and every stage below runs exactly as it did before this pre-pass existed"
                )),
                ..RepairEvidence::default()
            },
        },
    }
}

/// The pass proper. Every `?` in here is a **skip**, never a refusal: the one
/// caller above turns each of them into evidence and hands back the input.
fn run_repair(mesh: &Mesh, limits: TriageTopologyLimits) -> Result<RepairedMesh, RepairSkipped> {
    // **The resource bound gates the whole pass, not one action of it.**
    //
    // It used to guard only the shard strip, and the line after it built the
    // welded face topology unconditionally on the same unbounded mesh -- a
    // `BTreeMap` edge index over ~3x the face count plus a per-welded-vertex
    // incidence map plus materialized pinch fans, which is *heavier* than the
    // triage it was standing in for. A raw STL over the ceiling therefore
    // skipped the cheap action and performed the expensive one anyway, and the
    // bound's own ledger row (FU-45) claimed a protection it did not provide.
    // So admission is proven once, here, before any input-proportional work at
    // all, and the capability it returns is what action 1 consumes.
    let admission = TriageTopologyAdmission::from_raw_mesh(mesh, limits)?;
    let cap_mm = pinch_split_cap_mm()?;

    let mut evidence = RepairEvidence {
        source_faces: mesh.tris().len(),
        source_vertices: mesh.verts().len(),
        ..RepairEvidence::default()
    };

    // 1 -- shards.
    //
    // `stage` is the geometry the next action sees. It stays `None` -- meaning
    // "still the caller's own arrays, untouched" -- until some action actually
    // removes a face, so the ~23 of 37 corpus models that need nothing never
    // pay for a copy of a mesh nothing is going to edit.
    evidence.shards = strip_dust_shards(admission)?;
    let mut stage: Option<Mesh> = if evidence.shards.faces_removed.is_empty() {
        None
    } else {
        let dropped = evidence
            .shards
            .faces_removed
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let tris = mesh
            .tris()
            .iter()
            .enumerate()
            .filter_map(|(face, triangle)| (!dropped.contains(&face)).then_some(*triangle))
            .collect::<Vec<_>>();
        // Checked here rather than at the end: `Mesh::new` refuses an empty
        // face array, so a removal that takes the last face has to be caught
        // before it is built, not after.
        if tris.is_empty() {
            return Ok(refuse_to_empty(mesh, evidence));
        }
        Some(Mesh::new(mesh.verts().to_vec(), tris)?)
    };
    let mut topology = build_face_topology(stage.as_ref().unwrap_or(mesh), WeldPolicy::ExactBits)?;

    // 2 -- duplicates, against a topology built on the post-strip mesh.
    //
    // **The topology already answered whether there is anything to do.** Its
    // one canonical construction groups every welded vertex triple and counts
    // the groups with more than one member, which is the same grouping
    // `plan_duplicate_removal` would rebuild; zero groups is therefore proof,
    // not a guess, and the whole action -- the per-edge `WeldedEdges` index
    // and the per-face-keyed group map, two heap `BTreeMap`s at raw size, one
    // `Vec` per edge -- is skipped rather than run to discover it has nothing
    // to remove. On the 459k-face Giant Toad that index alone is ~650k
    // allocations, built pre-gate, on the production `prepare` path.
    if topology.counts().duplicate_face_groups > 0 {
        let current = stage.as_ref().unwrap_or(mesh);
        // One gathered edge index for this action, read by every group's
        // disposition -- not one lookup per group.
        let edges = WeldedEdges::build(current.tris(), topology.source_vertex_to_welded());
        evidence.duplicates =
            plan_duplicate_removal(current.tris(), topology.source_vertex_to_welded(), &edges);
        if evidence.duplicates.faces_removed() > 0 {
            let dropped = evidence
                .duplicates
                .groups
                .iter()
                .flat_map(|group| group.removed.iter().copied())
                .collect::<BTreeSet<_>>();
            let tris = current
                .tris()
                .iter()
                .enumerate()
                .filter_map(|(face, triangle)| (!dropped.contains(&face)).then_some(*triangle))
                .collect::<Vec<_>>();
            if tris.is_empty() {
                return Ok(refuse_to_empty(mesh, evidence));
            }
            let reduced = Mesh::new(current.verts().to_vec(), tris)?;
            // A removal merges fans that the duplicate was severing, so the
            // pinch analysis is re-derived rather than reused. Reusing it
            // would split vertices that are no longer pinched and open a
            // boundary.
            topology = build_face_topology(&reduced, WeldPolicy::ExactBits)?;
            stage = Some(reduced);
        }
    }

    // 3 -- pinches.
    //
    // Same argument as above, from the other flag: `point_only_vertices` is
    // empty exactly when no face carries `TOPOLOGY_CUT_POINT_ONLY`, so an
    // empty list means there is no pinch to split and no fan whose severance
    // needs checking. The edge index is not built to find that out.
    let mut split = None;
    if !topology.point_only_vertices().is_empty() {
        let current = stage.as_ref().unwrap_or(mesh);
        let edges = WeldedEdges::build(current.tris(), topology.source_vertex_to_welded());
        let plan = split_pinch_vertices(&topology, &edges, current.verts(), current.tris(), cap_mm);
        evidence.pinches = plan.evidence;
        if !plan.appended.is_empty() {
            let mut verts = current.verts().to_vec();
            let mut tris = current.tris().to_vec();
            verts.extend(plan.appended);
            for (face, corner, vertex) in plan.rewrites {
                tris[face][corner] = vertex;
            }
            split = Some((verts, tris));
        }
    }

    if !evidence.changed() {
        // Nothing was done, so nothing is rebuilt: the caller gets its own
        // arrays back, in their own order.
        evidence.repaired_faces = evidence.source_faces;
        evidence.repaired_vertices = evidence.source_vertices;
        return Ok(RepairedMesh {
            mesh: mesh.clone(),
            evidence,
        });
    }

    // A stripped shard leaves its vertices behind, unreferenced. They are
    // harmless to the gate and untidy for everything else, so the changed path
    // ends in one compaction into first-use order. The unchanged path never
    // reaches here, which is what keeps byte-identity a contract rather than a
    // near miss.
    let (verts, tris) = match (&split, &stage) {
        (Some((verts, tris)), _) => compact(verts, tris),
        (None, Some(stage)) => compact(stage.verts(), stage.tris()),
        // `changed()` is true exactly when one of the three actions edited a
        // face, and each of them writes `split` or `stage` when it does.
        (None, None) => unreachable!("a recorded change wrote no geometry"),
    };
    // Both removing actions already refused to take the last face, and the
    // pinch split only ever appends -- so this is a net under the two of them
    // and under any action added later, checked before the mesh is built
    // rather than discovered as `MeshError::NoTriangles` two stages downstream.
    if tris.is_empty() {
        return Ok(refuse_to_empty(mesh, evidence));
    }
    let repaired = Mesh::new(verts, tris)?;
    evidence.repaired_faces = repaired.tris().len();
    evidence.repaired_vertices = repaired.verts().len();
    evidence.residual = Some(measure_residual(&build_face_topology(
        &repaired,
        WeldPolicy::ExactBits,
    )?));
    Ok(RepairedMesh {
        mesh: repaired,
        evidence,
    })
}

/// Roll every planned action back and hand the caller its own mesh, because
/// applying them would have left no triangles at all.
///
/// **This is a refusal by the pass, not a refusal of the model.** The evidence
/// rows keep what each action found — they are now a record of a plan, which is
/// what [`RepairEvidence::changed`] returning `false` here means — and the
/// unchanged gate goes on to judge the original geometry with that record
/// attached. Recorded as FU-46.
fn refuse_to_empty(mesh: &Mesh, mut evidence: RepairEvidence) -> RepairedMesh {
    evidence.refused_to_empty = Some(EmptyResultRefusal {
        source_faces: evidence.source_faces,
        shard_faces_planned: evidence.shards.faces_removed.len(),
        duplicate_faces_planned: evidence.duplicates.faces_removed(),
    });
    evidence.repaired_faces = evidence.source_faces;
    evidence.repaired_vertices = evidence.source_vertices;
    RepairedMesh {
        mesh: mesh.clone(),
        evidence,
    }
}

/// The registered ceiling on one pinch-split displacement.
fn pinch_split_cap_mm() -> Result<f64, RepairSkipped> {
    Ok(m0_constants()
        .map_err(RepairSkipped::Constants)?
        .placeholders()
        .repair_pinch_split_max_mm()
        .diagnostic()
        .get())
}

/// Action 1, delegated in full to the existing triage machinery.
///
/// Nothing about the six criteria, the keep-largest prohibition, or the
/// containment refusal is restated here.
///
/// **The verdict is read, not the mesh.** `triage_summary` runs the same six
/// criteria as `triage` and stops one step short of compacting a mesh this
/// pass would never use: the pre-pass edits its *own* arrays by source face
/// index, because byte-identity for a clean model means the surviving faces
/// keep the caller's numbering. Calling `triage` here allocated a welded,
/// compacted `TriagedMesh` at raw size — ~459k faces on the largest corpus
/// model — on every call and dropped it unread.
///
/// **The resource bound is proven by the caller, and it covers the whole
/// pass.** This action runs *pre-gate*, on every raw input the pipeline is
/// handed — up to ~459k faces in the corpus and unbounded for arbitrary input —
/// so it needs a bound before it starts input-proportional work. It does not
/// prove that bound itself: [`run_repair`] does, once, ahead of *every* action,
/// and passes the admission capability in. That is not tidiness. The bound used
/// to live here, and the topology build one line below it ran unconditionally
/// on the same unbounded mesh — heavier work than the triage the bound was
/// skipping — so an over-large input skipped the cheap action and paid for the
/// expensive one. Taking the capability by value makes that shape
/// unrepresentable: this function cannot run without one.
///
/// Limits derived from the mesh's own measured counts would make every
/// comparison `n <= n` and could not refuse anything at all; the workspace has
/// already found and fixed that exact defect shape once, and
/// `PrepareResourcePolicy`'s own doc records the conclusion: a cap that scales
/// with its own charge is not a cap. So the ceilings are the absolute ones from
/// [`TriageTopologyLimits::production`], and what an input over them gets is
/// **less repair, never less admission** — the pass is skipped and recorded,
/// the model reaches every stage below exactly as it did before this pre-pass
/// existed, and the hollow stage's own H0 boundary still applies where it
/// always did. Recorded as FU-45.
fn strip_dust_shards(
    admission: TriageTopologyAdmission<'_>,
) -> Result<ShardStripEvidence, RepairSkipped> {
    match admission.triage_summary(WeldPolicy::ExactBits) {
        Ok(summary) => Ok(ShardStripEvidence {
            components_stripped: summary.stripped_components().to_vec(),
            faces_removed: summary.stripped_faces().to_vec(),
            components_seen: summary.component_count(),
            skipped: None,
        }),
        // A sealed cavity inside the raw input is a pre-hollowed model. That
        // is a real refusal, and it is the hollow stage's to make where it
        // already lives -- this pre-pass must not invent a new refusal at a
        // new place in the chain. The strip is skipped and recorded.
        Err(TriageError::PreHollowed(pre_hollowed)) => Ok(ShardStripEvidence {
            skipped: Some(format!(
                "raw input triages as pre-hollowed ({} sealed cavity component(s)); shard strip skipped for the hollow stage to judge",
                pre_hollowed.cavity_components().len()
            )),
            ..ShardStripEvidence::default()
        }),
        Err(error) => Err(map_triage_error(error)),
    }
}

fn map_triage_error(error: TriageError) -> RepairSkipped {
    match error {
        TriageError::Admission(admission) => RepairSkipped::Admission(admission),
        TriageError::Topology(topology) => RepairSkipped::Topology(topology),
        TriageError::Mesh(mesh) => RepairSkipped::Mesh(mesh),
        TriageError::Constants(constants) => RepairSkipped::Constants(constants),
        other => RepairSkipped::Triage(other),
    }
}

/// Every welded undirected edge of one face array, with the direction each
/// incident face traverses it in.
///
/// Built once per stage from `tris` and the weld map — one gather, one pass,
/// no per-item topology rebuild — and then read by both the duplicate
/// disposition and the pinch severance check.
struct WeldedEdges {
    occurrences: BTreeMap<(u32, u32), Vec<EdgeUse>>,
}

/// One face's use of one welded edge. `forward` means the face walks the edge
/// from its lower welded id to its higher one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EdgeUse {
    face: usize,
    forward: bool,
}

impl WeldedEdges {
    fn build(tris: &[[u32; 3]], weld_map: &[u32]) -> Self {
        let mut occurrences: BTreeMap<(u32, u32), Vec<EdgeUse>> = BTreeMap::new();
        for (face, triangle) in tris.iter().enumerate() {
            let triple = welded_triple(*triangle, weld_map);
            for (from, to) in [
                (triple[0], triple[1]),
                (triple[1], triple[2]),
                (triple[2], triple[0]),
            ] {
                occurrences
                    .entry((from.min(to), from.max(to)))
                    .or_default()
                    .push(EdgeUse {
                        face,
                        forward: from < to,
                    });
            }
        }
        Self { occurrences }
    }

    fn uses(&self, edge: (u32, u32)) -> &[EdgeUse] {
        self.occurrences
            .get(&edge)
            .map_or(&[][..], |uses| uses.as_slice())
    }

    /// How many times `edge` is walked in each direction by faces that are
    /// **not** members of `group` — `(low -> high, high -> low)`.
    fn outside(&self, edge: (u32, u32), group: &BTreeSet<usize>) -> (usize, usize) {
        let mut counts = (0_usize, 0_usize);
        for use_ in self.uses(edge) {
            if group.contains(&use_.face) {
                continue;
            }
            if use_.forward {
                counts.0 += 1;
            } else {
                counts.1 += 1;
            }
        }
        counts
    }
}

fn welded_triple(triangle: [u32; 3], weld_map: &[u32]) -> [u32; 3] {
    [
        weld_map[triangle[0] as usize],
        weld_map[triangle[1] as usize],
        weld_map[triangle[2] as usize],
    ]
}

/// Action 2: drop every duplicate of a welded vertex triple, keeping the first
/// by source order, and record what that achieved.
///
/// **One rule, because exactly one was authorized.** The spec's words are "drop
/// faces sharing an identical vertex triple, keeping the first by deterministic
/// order", and that is the whole action: the survivor is always the earliest
/// member, whatever its winding and whatever the neighbourhood looks like. Two
/// wider rules were tried and are *not* here, both filed on FU-46 rather than
/// landed:
///
/// * *Delete the group outright when it is edge-isolated.* This removes
///   geometry with nothing left in its place — a 60 mm free-floating doubled
///   triangle vanishes — and it can turn a gate **refusal into a pass** by
///   deleting the geometry that was being refused. Reproduced on a box + flap
///   model: `validate_closed_mesh` went `Err` to `Ok` with the owner's
///   triangles gone.
/// * *Keep a later, opposite-wound member when that leaves the local edges
///   cleaner.* This is an orientation repair wearing a duplicate's label, for a
///   defect class the census counted **zero** of (0 same-direction edges across
///   all 21 models).
///
/// Neither is a thing to decide inside an implementation. Until the owner rules
/// on FU-46 the conservative rule stands, and the shapes it cannot finish —
/// a free-floating coincident stack, a doubled sheet — reach the unchanged gate
/// and are refused there. Under the standing doctrine of the 2026-08-10 ruling
/// that refusal *is* the correct outcome: it becomes the work item.
///
/// **What the local-edge check is still for.** Keeping the first member is not
/// always enough to close the group's neighbourhood, and a face count cannot
/// say which case a reader is looking at. So each group is measured against the
/// faces *outside* it: the disposition is [`DuplicateDisposition::KeepOne`]
/// when the survivor leaves all three edges of the triple walked exactly once
/// in each direction (or not at all), and
/// [`DuplicateDisposition::KeptFirstUnresolved`] when it does not. That is
/// evidence, not a decision — the same face is kept either way — and it is what
/// makes FU-46's residual class countable in a run report.
///
/// The counts are measured against the mesh as it arrives at this action, so a
/// group whose neighbours are themselves about to be reduced is judged against
/// the geometry that actually exists rather than against a projection of it.
///
/// **What the corpus actually exercises**, measured 2026-08-11 by
/// `examples/repair_probe` over all 37 STLs in `minis-to-process`: on **raw**
/// input, where this pre-pass runs in production, **no** duplicate group
/// reaches this action at all — the shard strip removes the census's three
/// groups first, and all 14 raw-refused models are rescued without it. On the
/// probe's **decimated** column, which the pre-pass does not run on in
/// production (FU-44), 14 models carry between 1 and 14 decimator-manufactured
/// groups each. So the rule's evidence is the fixtures in `tests/repair.rs`,
/// which is why they cover the load-bearing and free-floating shapes of both
/// `f = 2, r = 0` and `f, r >= 1`, plus the doubled sheet, rather than a bare
/// two-triangle universe.
fn plan_duplicate_removal(
    tris: &[[u32; 3]],
    weld_map: &[u32],
    edges: &WeldedEdges,
) -> DuplicateFaceEvidence {
    let mut groups_by_key: BTreeMap<[u32; 3], Vec<usize>> = BTreeMap::new();
    let mut welded = Vec::with_capacity(tris.len());
    for (face, triangle) in tris.iter().enumerate() {
        let triple = welded_triple(*triangle, weld_map);
        welded.push(triple);
        let mut key = triple;
        key.sort_unstable();
        groups_by_key.entry(key).or_default().push(face);
    }

    let mut evidence = DuplicateFaceEvidence::default();
    for (key, members) in groups_by_key {
        if members.len() < 2 {
            continue;
        }
        if key[0] == key[1] || key[1] == key[2] {
            evidence.degenerate_groups_skipped += 1;
            continue;
        }
        let mut forward = Vec::new();
        let mut reverse = Vec::new();
        for face in &members {
            if is_forward(welded[*face], key) {
                forward.push(*face);
            } else {
                reverse.push(*face);
            }
        }
        let group: BTreeSet<usize> = members.iter().copied().collect();
        let (kept, disposition) =
            choose_duplicate_disposition(key, &forward, &reverse, edges, &group);
        let mut removed = members
            .iter()
            .copied()
            .filter(|face| !kept.contains(face))
            .collect::<Vec<_>>();
        removed.sort_unstable();
        evidence.groups.push(DuplicateFaceGroup {
            welded_triple: key,
            forward: forward.len(),
            reverse: reverse.len(),
            kept,
            removed,
            disposition,
        });
    }
    evidence
}

/// The survivor — always the earliest member — and what keeping it achieved.
///
/// **This function chooses nothing.** `kept` is a fixed function of the group's
/// source indices; only the [`DuplicateDisposition`] varies, and it is a
/// measurement of the result rather than an input to it. Two earlier drafts did
/// choose, and both are now FU-46's un-ruled scope question rather than landed
/// behaviour: keeping a later opposite-wound member (an orientation repair for
/// a defect class the census found zero of) and keeping **none** at all for an
/// edge-isolated stack. The second is the one that mattered — it is the only
/// rule in the pass that could delete geometry with no survivor, it had no size
/// bound, and it could flip a gate refusal into a pass by deleting the geometry
/// the gate was refusing. A pre-pass that can do that is not upstream of the
/// gate, it is a hole in it.
fn choose_duplicate_disposition(
    key: [u32; 3],
    forward: &[usize],
    reverse: &[usize],
    edges: &WeldedEdges,
    group: &BTreeSet<usize>,
) -> (Vec<usize>, DuplicateDisposition) {
    let earliest_overall = forward
        .first()
        .into_iter()
        .chain(reverse.first())
        .copied()
        .min()
        .expect("a duplicate group has at least two members");
    let (keep_forward, keep_reverse) = if forward.first() == Some(&earliest_overall) {
        (1, 0)
    } else {
        (0, 1)
    };
    let disposition = if locally_clean(key, keep_forward, keep_reverse, edges, group) {
        DuplicateDisposition::KeepOne
    } else {
        DuplicateDisposition::KeptFirstUnresolved
    };
    (vec![earliest_overall], disposition)
}

/// Whether keeping `keep_forward` forward and `keep_reverse` reverse members of
/// the group leaves all three of its welded edges in a state a closed,
/// consistently oriented surface can be in.
///
/// For an ascending triple `[a, b, c]`, a forward-wound member walks `a -> b`,
/// `b -> c`, `c -> a`: low-to-high on `(a, b)` and `(b, c)`, high-to-low on
/// `(a, c)`. A reverse member walks the mirror of that.
fn locally_clean(
    key: [u32; 3],
    keep_forward: usize,
    keep_reverse: usize,
    edges: &WeldedEdges,
    group: &BTreeSet<usize>,
) -> bool {
    let [a, b, c] = key;
    // (edge, does a forward member walk it low -> high?)
    for (edge, forward_walks_up) in [((a, b), true), ((b, c), true), ((a, c), false)] {
        let (outside_up, outside_down) = edges.outside(edge, group);
        let (added_up, added_down) = if forward_walks_up {
            (keep_forward, keep_reverse)
        } else {
            (keep_reverse, keep_forward)
        };
        let up = outside_up + added_up;
        let down = outside_down + added_down;
        let clean = (up == 1 && down == 1) || (up == 0 && down == 0);
        if !clean {
            return false;
        }
    }
    true
}

/// Whether `triple` walks `ascending` in its own cyclic order.
fn is_forward(triple: [u32; 3], ascending: [u32; 3]) -> bool {
    [
        [ascending[0], ascending[1], ascending[2]],
        [ascending[1], ascending[2], ascending[0]],
        [ascending[2], ascending[0], ascending[1]],
    ]
    .contains(&triple)
}

/// Action 3: open every pinch into one copy of the vertex per fan.
///
/// Fan 0 never moves, so the source arrays are only ever appended to and every
/// untouched corner keeps its original index. Each later fan gets a fresh
/// vertex displaced along that fan's own mean unit normal by the smallest
/// power-of-two multiple of one f32 ULP that is *proved* — not assumed — to
/// survive both gates it has to survive: the exact-bit weld, which keys on
/// `to_bits()` and would re-fuse a displacement that rounds away, and the f32
/// binary-STL round trip, which would undo one that survives in f64 alone.
///
/// **A pinch is only split when the fans are genuinely held apart by a point.**
/// Fan connectivity is computed across *accepted* edges, and an edge is not
/// accepted when it is non-manifold, a boundary, degenerate, or duplicated —
/// so a vertex sitting on such an edge appears in `point_only_vertices` even
/// though its sheets meet along a line, not at a point. Splitting one of those
/// tears the junction into a crack under the pinch label. Every incident edge
/// at the vertex is therefore checked first, and a vertex with any unaccepted
/// one is refused as [`UnrepairedPinch::FanSeveredByEdge`] and left to the
/// gate.
///
/// **A copy has to land somewhere nothing already is — anywhere in the mesh,
/// not just at this pinch.** The distinguishability test used to run against
/// the pinch's own position and its own already-moved copies alone, which is a
/// test of the wrong set: a copy nudged onto a bit-exact match with some
/// *unrelated* vertex re-fuses under `WeldPolicy::ExactBits` and creates a
/// fresh point contact at the destination, while [`PinchSplitEvidence`] records
/// an unqualified success. Reproduced: `pinch-vertices=1 unrepaired-pinches=0`
/// on a mesh whose residual still carried seven point-only faces. The residual
/// and the gate stayed honest, but the split's own provenance did not — and
/// false provenance sits on exactly the path the standing doctrine turns into
/// work items. So one [`PositionRegistry`] of every position in the mesh is
/// gathered up front (D-37: one batched pass, not a scan per candidate) and
/// every candidate is tested against it, with each committed copy joining the
/// registry so later pinches cannot land on earlier ones either.
fn split_pinch_vertices(
    topology: &crate::FaceTopology,
    edges: &WeldedEdges,
    verts: &[[f64; 3]],
    tris: &[[u32; 3]],
    cap_mm: f64,
) -> PinchPlan {
    let mut plan = PinchPlan::default();
    // Everything below reads the *pre-split* arrays and writes only into the
    // plan. A face can be incident to two different pinches, so mutating in
    // place would let the second pinch read a corner the first one already
    // repointed -- an index past the end of this topology's weld map.
    let mut evidence = PinchSplitEvidence::default();
    // One gather over the whole vertex array, read by every candidate of every
    // pinch. The alternative -- re-scanning `verts` per candidate -- is the
    // per-item hot-path call D-37 forbids, and at 459k vertices times 113
    // pinches it is not a micro-optimization.
    let mut registry = PositionRegistry::build(verts);
    for pinch in topology.point_only_vertices() {
        let welded_vertex = pinch.welded_vertex();
        // Gather each fan's corner sites once, and refuse the whole pinch if
        // any face reaches the vertex twice: that face is degenerate, which is
        // a class this pass does not touch, and half-splitting around it would
        // be a guess.
        let mut fan_sites: Vec<Vec<(usize, usize)>> = Vec::with_capacity(pinch.fans().len());
        let mut degenerate = None;
        for fan in pinch.fans() {
            let mut sites = Vec::new();
            for &face in fan {
                let triangle = tris[face as usize];
                let mut corners = (0..3).filter(|corner| {
                    topology.source_vertex_to_welded()[triangle[*corner] as usize] == welded_vertex
                });
                let Some(corner) = corners.next() else {
                    continue;
                };
                if corners.next().is_some() {
                    degenerate = Some(face);
                    break;
                }
                sites.push((face as usize, corner));
            }
            if degenerate.is_some() {
                break;
            }
            fan_sites.push(sites);
        }
        if let Some(face) = degenerate {
            evidence
                .unrepaired
                .push(UnrepairedPinch::DegenerateIncidence {
                    welded_vertex,
                    face,
                });
            continue;
        }

        // Is this a point contact at all? An edge at this vertex that the
        // topology could not accept is a different defect wearing the pinch's
        // label, and splitting it would tear a junction rather than open one.
        if let Some(edge) = severing_edge(topology, edges, tris, &fan_sites, welded_vertex) {
            evidence.unrepaired.push(UnrepairedPinch::FanSeveredByEdge {
                welded_vertex,
                edge,
            });
            continue;
        }

        let Some(&(anchor_face, anchor_corner)) = fan_sites.first().and_then(|sites| sites.first())
        else {
            continue;
        };
        let position = verts[tris[anchor_face][anchor_corner] as usize];
        let next_vertex_id = |plan: &PinchPlan| verts.len() + plan.appended.len();

        // Plan every fan before touching the arrays: a pinch is repaired whole
        // or not at all, so a fan that cannot be separated cannot leave the
        // vertex half-split.
        // `tentative` holds this pinch's own copies until the pinch is applied
        // whole. They cannot go straight into the registry, because a pinch
        // that fails on a later fan is abandoned entirely -- and a registry
        // holding positions no vertex ever took would make a later pinch step
        // around geometry that is not there.
        let mut planned: Vec<(usize, [f64; 3], PinchFanMove)> = Vec::new();
        let mut tentative: Vec<[f64; 3]> = Vec::new();
        let mut failure = None;
        for (index, sites) in fan_sites.iter().enumerate().skip(1) {
            let Some(direction) = fan_mean_normal(verts, tris, sites) else {
                failure = Some(UnrepairedPinch::NoFanDirection { welded_vertex });
                break;
            };
            match separate(position, direction, &tentative, &registry, cap_mm) {
                Ok((moved, steps, displacement)) => {
                    tentative.push(moved);
                    planned.push((
                        index,
                        moved,
                        PinchFanMove {
                            faces: sites.len(),
                            direction,
                            ulp_steps: steps,
                            displacement_mm: displacement,
                        },
                    ));
                }
                Err(SeparationRefusal::OneUlpExceedsCap { ulp_mm }) => {
                    failure = Some(UnrepairedPinch::OneUlpExceedsCap {
                        welded_vertex,
                        cap_mm,
                        ulp_mm,
                    });
                    break;
                }
                Err(SeparationRefusal::Capped { largest_attempt_mm }) => {
                    failure = Some(UnrepairedPinch::DisplacementCapped {
                        welded_vertex,
                        cap_mm,
                        largest_attempt_mm,
                    });
                    break;
                }
            }
        }
        if let Some(reason) = failure {
            evidence.unrepaired.push(reason);
            continue;
        }

        // A pinch is applied whole or not at all, so the index headroom for
        // *every* copy this pinch needs is checked before the first one is
        // written. Checking inside the loop could leave a vertex half-split
        // between two positions, which is worse than the pinch.
        if u32::try_from(next_vertex_id(&plan) + planned.len()).is_err() {
            evidence
                .unrepaired
                .push(UnrepairedPinch::VertexIndexExhausted { welded_vertex });
            continue;
        }
        let mut fans = Vec::with_capacity(planned.len());
        for (index, moved, record) in planned {
            let new_vertex =
                u32::try_from(next_vertex_id(&plan)).expect("headroom checked for this pinch");
            // Committed now, and only now: this position is real geometry from
            // here on, so no later pinch may land a copy on it.
            registry.insert(moved);
            plan.appended.push(moved);
            for &(face, corner) in &fan_sites[index] {
                plan.rewrites.push((face, corner, new_vertex));
            }
            fans.push(record);
        }
        evidence.splits.push(PinchSplit {
            welded_vertex,
            position_mm: position,
            anchored_faces: fan_sites[0].len(),
            fans,
        });
    }
    plan.evidence = evidence;
    plan
}

/// The pinch splits decided against the pre-split arrays, ready to apply.
///
/// `rewrites` are `(face, corner, new vertex id)`, and every id is an index
/// into `verts` extended by `appended` in order — so applying the plan is one
/// extend plus one scatter, with no aliasing between what is read and what is
/// written.
#[derive(Clone, Debug, Default)]
struct PinchPlan {
    appended: Vec<[f64; 3]>,
    rewrites: Vec<(usize, usize, u32)>,
    evidence: PinchSplitEvidence,
}

/// The unit mean of a fan's face normals, or `None` when they cancel.
///
/// Unnormalized face normals are summed, so a fan's larger faces weigh more —
/// the direction that opens the widest part of the pinch first.
fn fan_mean_normal(
    verts: &[[f64; 3]],
    tris: &[[u32; 3]],
    sites: &[(usize, usize)],
) -> Option<[f64; 3]> {
    let mut sum = [0.0_f64; 3];
    for &(face, _) in sites {
        let [ia, ib, ic] = tris[face];
        let a = verts[ia as usize];
        let b = verts[ib as usize];
        let c = verts[ic as usize];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        sum[0] += ab[1] * ac[2] - ab[2] * ac[1];
        sum[1] += ab[2] * ac[0] - ab[0] * ac[2];
        sum[2] += ab[0] * ac[1] - ab[1] * ac[0];
    }
    let length = sum[0].hypot(sum[1]).hypot(sum[2]);
    if !length.is_finite() || length == 0.0 {
        return None;
    }
    let unit = [sum[0] / length, sum[1] / length, sum[2] / length];
    unit.iter().all(|value| value.is_finite()).then_some(unit)
}

/// The smallest power-of-two ULP multiple that moves `position` along
/// `direction` to a point distinguishable from **every position the mesh
/// already has** — those in `registry` plus this pinch's own not-yet-committed
/// `tentative` copies — with a realized displacement at or below `cap_mm`.
///
/// **Both sets are load-bearing.** Testing only against the pinch's own copies
/// lets a candidate land bit-exactly on an unrelated vertex, where the
/// exact-bit weld re-fuses it and the evidence reports a split that did not
/// happen; testing only against the registry lets two fans of the same pinch
/// stack, since neither is committed until the pinch is applied whole.
///
/// **The moved copy lands on an exact f32 grid point.** `write_binary_stl`
/// refuses outright to encode a coordinate that is not exactly representable
/// as f32 (`MeshIoError::LossyF32Encoding`), so a split that produced an
/// arbitrary f64 would make the repaired mesh unwritable — and a split that
/// merely *survives* narrowing is not the same claim as one that is already
/// narrow. Snapping first makes the round trip a no-op by construction rather
/// than a hope, and the displacement reported is the realized one, measured
/// after the snap.
fn separate(
    position: [f64; 3],
    direction: [f64; 3],
    tentative: &[[f64; 3]],
    registry: &PositionRegistry,
    cap_mm: f64,
) -> Result<([f64; 3], u32, f64), SeparationRefusal> {
    let step = f32_ulp_at(position);
    // The floor and the ceiling are set by different things: the floor is one
    // f32 ULP *of this vertex's own coordinate magnitude*, the ceiling is an
    // absolute registered millimetre. Above roughly 512 mm the floor overtakes
    // the ceiling and there is no candidate to test at all -- said outright
    // here rather than reported as an attempt that was never made.
    if !step.is_finite() || step > cap_mm {
        return Err(SeparationRefusal::OneUlpExceedsCap { ulp_mm: step });
    }
    let mut steps = 1_u32;
    let mut largest = 0.0_f64;
    loop {
        let requested = step * f64::from(steps);
        if !requested.is_finite() || requested > cap_mm {
            return Err(SeparationRefusal::Capped {
                largest_attempt_mm: largest,
            });
        }
        let candidate = snap_to_f32([
            position[0] + requested * direction[0],
            position[1] + requested * direction[1],
            position[2] + requested * direction[2],
        ]);
        let realized = ((candidate[0] - position[0]).powi(2)
            + (candidate[1] - position[1]).powi(2)
            + (candidate[2] - position[2]).powi(2))
        .sqrt();
        // The attempt is recorded before it is judged, so a refusal reports
        // the displacement that overshot rather than the last one that fit.
        if realized.is_finite() {
            largest = largest.max(realized);
        }
        if !realized.is_finite() || realized > cap_mm {
            return Err(SeparationRefusal::Capped {
                largest_attempt_mm: largest,
            });
        }
        if candidate.iter().all(|value| value.is_finite())
            && !registry.occupied(candidate)
            && tentative
                .iter()
                .all(|other| distinguishable(candidate, *other))
        {
            return Ok((candidate, steps, realized));
        }
        let Some(next) = steps.checked_mul(2) else {
            return Err(SeparationRefusal::Capped {
                largest_attempt_mm: largest,
            });
        };
        steps = next;
    }
}

/// Why no displacement was accepted, told apart by whether anything could be
/// tried in the first place.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SeparationRefusal {
    /// The minimum step -- one f32 ULP at this coordinate magnitude -- is
    /// already above the cap. Nothing was attempted, and the reported number is
    /// the step, not an attempt.
    OneUlpExceedsCap { ulp_mm: f64 },
    /// Candidates were tried and none separated at or below the cap.
    /// `largest_attempt_mm` is the largest displacement actually realized by an
    /// attempt, including the one that overshot.
    Capped { largest_attempt_mm: f64 },
}

/// The first edge at `welded_vertex` that the topology did **not** accept as a
/// manifold pair, if any.
///
/// An accepted edge has exactly two occurrences, on two distinct faces, walked
/// in opposite directions — *and* those two faces are neighbours in the built
/// topology, which is the part a recount alone would miss: the topology also
/// refuses an edge whose faces are duplicate-flagged, and a fan separation
/// caused by that is not a point contact either.
fn severing_edge(
    topology: &crate::FaceTopology,
    edges: &WeldedEdges,
    tris: &[[u32; 3]],
    fan_sites: &[Vec<(usize, usize)>],
    welded_vertex: u32,
) -> Option<[u32; 2]> {
    let weld_map = topology.source_vertex_to_welded();
    for sites in fan_sites {
        for &(face, corner) in sites {
            let triple = welded_triple(tris[face], weld_map);
            // The two edges of this face that touch the pinch vertex.
            for other in [triple[(corner + 1) % 3], triple[(corner + 2) % 3]] {
                let edge = (welded_vertex.min(other), welded_vertex.max(other));
                let uses = edges.uses(edge);
                let accepted = welded_vertex != other
                    && uses.len() == 2
                    && uses[0].face != uses[1].face
                    && uses[0].forward != uses[1].forward
                    && topology
                        .neighbors(uses[0].face)
                        .is_some_and(|neighbors| neighbors.contains(&(uses[1].face as u32)));
                if !accepted {
                    return Some([edge.0, edge.1]);
                }
            }
        }
    }
    None
}

/// The nearest exactly-f32-representable point, which is the only kind of
/// point a binary STL will accept.
fn snap_to_f32(point: [f64; 3]) -> [f64; 3] {
    point.map(|coordinate| f64::from(coordinate as f32))
}

/// Whether two positions survive as two positions: distinct under the
/// exact-bit weld's own key — which canonicalizes signed zero, so a sign flip
/// is not a separation — and still distinct after the f32 round trip a binary
/// STL will perform on them.
fn distinguishable(left: [f64; 3], right: [f64; 3]) -> bool {
    exact_key(left) != exact_key(right) && narrowed_key(left) != narrowed_key(right)
}

/// The exact-bit weld's own key, with signed zero canonicalized the way
/// `WeldPolicy::ExactBits` canonicalizes it.
fn exact_key(point: [f64; 3]) -> [u64; 3] {
    point.map(|coordinate| {
        if coordinate == 0.0 {
            0.0_f64.to_bits()
        } else {
            coordinate.to_bits()
        }
    })
}

/// The same key after the f32 narrowing a binary STL performs, which is the
/// second way a split can be undone.
fn narrowed_key(point: [f64; 3]) -> [u32; 3] {
    point.map(|coordinate| {
        if coordinate == 0.0 {
            0.0_f32.to_bits()
        } else {
            (coordinate as f32).to_bits()
        }
    })
}

/// Every position the mesh already holds, keyed both ways a split can be
/// undone, so a candidate can be tested in a lookup instead of a scan.
///
/// **Why a set and not a slice.** The question a candidate has to answer is
/// "does *anything* in this mesh already sit here", and answering it by walking
/// the vertex array once per candidate is the per-item hot-path call D-37
/// forbids: 459k vertices × 113 pinches × the ULP escalation. One gather up
/// front, `O(log n)` per candidate, and the two keys are exactly the two
/// equalities [`distinguishable`] tests — no third notion of "same point"
/// exists here to drift.
struct PositionRegistry {
    exact: BTreeSet<[u64; 3]>,
    narrowed: BTreeSet<[u32; 3]>,
}

impl PositionRegistry {
    /// One batched pass over the vertex array.
    fn build(verts: &[[f64; 3]]) -> Self {
        let mut exact = BTreeSet::new();
        let mut narrowed = BTreeSet::new();
        for vertex in verts {
            exact.insert(exact_key(*vertex));
            narrowed.insert(narrowed_key(*vertex));
        }
        Self { exact, narrowed }
    }

    fn insert(&mut self, position: [f64; 3]) {
        self.exact.insert(exact_key(position));
        self.narrowed.insert(narrowed_key(position));
    }

    /// Whether `candidate` would collide with something already here — under
    /// the exact-bit weld, or after the STL round trip. Either one re-fuses it.
    fn occupied(&self, candidate: [f64; 3]) -> bool {
        self.exact.contains(&exact_key(candidate))
            || self.narrowed.contains(&narrowed_key(candidate))
    }
}

/// One f32 ULP at this position's largest coordinate magnitude — the finest
/// step a binary STL can still represent there.
fn f32_ulp_at(position: [f64; 3]) -> f64 {
    let magnitude = position
        .iter()
        .fold(0.0_f64, |largest, coordinate| largest.max(coordinate.abs()))
        as f32;
    if magnitude == 0.0 || !magnitude.is_finite() {
        return f64::from(f32::MIN_POSITIVE);
    }
    let next = f32::from_bits(magnitude.to_bits() + 1);
    let ulp = f64::from(next) - f64::from(magnitude);
    if ulp > 0.0 && ulp.is_finite() {
        ulp
    } else {
        f64::from(f32::MIN_POSITIVE)
    }
}

/// Drop unreferenced vertices, renumbering in first-use order.
fn compact(verts: &[[f64; 3]], tris: &[[u32; 3]]) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    let mut renumbered = vec![u32::MAX; verts.len()];
    let mut compacted = Vec::new();
    let mut output = Vec::with_capacity(tris.len());
    for triangle in tris {
        let mut remapped = [0_u32; 3];
        for (corner, vertex) in triangle.iter().enumerate() {
            let slot = &mut renumbered[*vertex as usize];
            if *slot == u32::MAX {
                *slot = compacted.len() as u32;
                compacted.push(verts[*vertex as usize]);
            }
            remapped[corner] = *slot;
        }
        output.push(remapped);
    }
    (compacted, output)
}

fn measure_residual(topology: &crate::FaceTopology) -> ResidualTopology {
    use crate::topology::{
        TOPOLOGY_CUT_BOUNDARY, TOPOLOGY_CUT_DEGENERATE, TOPOLOGY_CUT_DUPLICATE,
        TOPOLOGY_CUT_NONMANIFOLD, TOPOLOGY_CUT_POINT_ONLY, TOPOLOGY_CUT_SAME_DIRECTION,
    };
    let count = |mask: u8| {
        topology
            .cut_flags()
            .iter()
            .filter(|flags| **flags & mask != 0)
            .count()
    };
    ResidualTopology {
        boundary_faces: count(TOPOLOGY_CUT_BOUNDARY),
        nonmanifold_faces: count(TOPOLOGY_CUT_NONMANIFOLD),
        same_direction_faces: count(TOPOLOGY_CUT_SAME_DIRECTION),
        duplicate_faces: count(TOPOLOGY_CUT_DUPLICATE),
        point_only_faces: count(TOPOLOGY_CUT_POINT_ONLY),
        degenerate_faces: count(TOPOLOGY_CUT_DEGENERATE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_ulp_is_the_finest_step_a_binary_stl_can_still_carry() {
        // At mini scale the step is nanometres, which is the whole point: the
        // separation is four orders below one printer pixel.
        let step = f32_ulp_at([30.0, 0.0, 0.0]);
        assert!(step > 0.0 && step < 1.0e-5, "{step}");
        assert!(distinguishable([0.0; 3], [step * 2.0, 0.0, 0.0]));
        // A displacement below one f32 ULP survives in f64 and is erased by
        // the STL round trip, which is exactly the trap this guards.
        assert!(!distinguishable(
            [30.0, 0.0, 0.0],
            [30.0 + step / 8.0, 0.0, 0.0]
        ));
    }

    /// The empty-result guard, exercised directly because no action this pass
    /// performs can reach it any more — every one of the three leaves at least
    /// one face by construction. It is kept for the rule FU-46 might rule in,
    /// so what it *does* is pinned: roll everything back, hand the caller its
    /// own mesh, and say so on the card.
    #[test]
    fn the_empty_result_guard_rolls_back_whole_and_returns_the_input() {
        let mesh = crate::make_box_default().expect("box");
        let evidence = RepairEvidence {
            source_faces: mesh.tris().len(),
            source_vertices: mesh.verts().len(),
            shards: ShardStripEvidence {
                faces_removed: (0..mesh.tris().len()).collect(),
                ..ShardStripEvidence::default()
            },
            ..RepairEvidence::default()
        };
        let refused = refuse_to_empty(&mesh, evidence);
        let refusal = refused
            .evidence()
            .refused_to_empty
            .expect("the guard records itself");
        assert_eq!(refusal.source_faces, 12);
        assert_eq!(refusal.shard_faces_planned, 12);
        assert!(!refused.evidence().changed(), "nothing was applied");
        assert_eq!(refused.mesh().verts(), mesh.verts());
        assert_eq!(refused.mesh().tris(), mesh.tris());
        assert!(refused.evidence().summary_line().contains("APPLIED=none"));
    }

    #[test]
    fn signed_zero_is_not_a_separation() {
        // The exact-bit weld canonicalizes signed zero, so a "move" that only
        // flips a sign bit must not be accepted as having split anything.
        assert!(!distinguishable([0.0, 1.0, 2.0], [-0.0, 1.0, 2.0]));
    }

    fn plan(tris: &[[u32; 3]], weld_map: &[u32]) -> DuplicateFaceEvidence {
        let edges = WeldedEdges::build(tris, weld_map);
        plan_duplicate_removal(tris, weld_map, &edges)
    }

    /// A box with `face` duplicated by `copies`, each either the face itself or
    /// its reverse. The box is what makes the test mean anything: a duplicate
    /// group is judged by what the *rest of the mesh* does with its three
    /// edges, so a two-triangle universe cannot distinguish the rules.
    fn box_with_copies(face: usize, copies: &[bool]) -> (Vec<[u32; 3]>, Vec<u32>) {
        let mesh = crate::make_box_default().expect("box");
        let mut tris = mesh.tris().to_vec();
        let original = tris[face];
        for forward in copies {
            tris.push(if *forward {
                original
            } else {
                [original[0], original[2], original[1]]
            });
        }
        let weld_map = (0..mesh.verts().len() as u32).collect::<Vec<_>>();
        (tris, weld_map)
    }

    /// The plain case, and the spec's plain rule: a face present twice on real
    /// surface reduces to the first of the two.
    #[test]
    fn a_repeated_face_on_real_surface_keeps_the_first() {
        let (tris, weld_map) = box_with_copies(0, &[true]);
        let evidence = plan(&tris, &weld_map);
        assert_eq!(evidence.groups.len(), 1);
        assert_eq!(evidence.groups[0].kept, vec![0]);
        assert_eq!(evidence.groups[0].removed, vec![12]);
        assert_eq!(
            evidence.groups[0].disposition,
            DuplicateDisposition::KeepOne
        );
    }

    /// **The case a blanket `|f - r|` cancellation gets wrong.** These copies
    /// sit on a load-bearing box face: cancelling the group takes the face out
    /// of a watertight box and leaves three boundary edges. One member has to
    /// survive, and it has to be the one wound with its neighbours.
    #[test]
    fn a_mixed_winding_group_on_real_surface_keeps_one_of_the_majority() {
        for copies in [
            &[false][..],              // f = 1, r = 1
            &[true, false][..],        // f = 2, r = 1
            &[true, false, false][..], // f = 2, r = 2
        ] {
            let (tris, weld_map) = box_with_copies(0, copies);
            let evidence = plan(&tris, &weld_map);
            assert_eq!(evidence.groups.len(), 1, "{copies:?}");
            let group = &evidence.groups[0];
            assert_eq!(
                group.disposition,
                DuplicateDisposition::KeepOne,
                "{copies:?}: cancelling would open a hole in the box"
            );
            assert_eq!(
                group.kept,
                vec![0],
                "{copies:?}: the survivor is the original face, wound with its neighbours"
            );
            assert_eq!(group.removed.len(), copies.len(), "{copies:?}");
        }
    }

    /// **The shape the pass deliberately does not finish.** Nothing outside the
    /// group walks any of its edges, so the survivor is an unattached triangle
    /// holding three boundary edges the input did not have — which the gate
    /// refuses, correctly, and which is the work item.
    ///
    /// Deleting the group outright instead would "fix" it, and that is exactly
    /// the rule this pass must not have: it is the one disposition with no
    /// survivor, it has no size bound (a 60 mm free-floating doubled triangle
    /// goes the same way as a 0.2 mm one), and it can take a mesh from gate
    /// `Err` to gate `Ok` by deleting the geometry being refused. The spec
    /// authorized keep-the-first and nothing else; widening it is FU-46's
    /// **owner ruling requested**, not an implementation's call.
    #[test]
    fn a_free_floating_coincident_group_keeps_the_first_and_goes_to_the_gate() {
        for tris in [
            vec![[0_u32, 1, 2], [0, 2, 1]], // the zero-volume flap
            vec![[0_u32, 1, 2], [0, 1, 2]], // and a coincident same-wound pair
        ] {
            let evidence = plan(&tris, &[0, 1, 2]);
            assert_eq!(evidence.groups.len(), 1, "{tris:?}");
            assert_eq!(
                evidence.groups[0].disposition,
                DuplicateDisposition::KeptFirstUnresolved,
                "{tris:?}"
            );
            assert_eq!(evidence.groups[0].kept, vec![0], "{tris:?}");
            assert_eq!(evidence.groups[0].removed, vec![1], "{tris:?}");
        }
    }

    /// **The winding does not buy a later member the survivorship either.** The
    /// earliest face here is reverse-wound against its neighbours, so keeping
    /// it leaves the local edges dirty and keeping face 12 instead would leave
    /// them clean. That preference is an orientation repair for a defect class
    /// the census counted zero of, so it is not made: the first is kept, the
    /// group is recorded as unresolved, and FU-46 carries the question.
    #[test]
    fn a_later_member_is_not_promoted_for_having_the_better_winding() {
        let mesh = crate::make_box_default().expect("box");
        let mut tris = mesh.tris().to_vec();
        // Flip face 0 in place and append a correctly wound copy after it.
        let original = tris[0];
        tris[0] = [original[0], original[2], original[1]];
        tris.push(original);
        let weld_map = (0..mesh.verts().len() as u32).collect::<Vec<_>>();
        let evidence = plan(&tris, &weld_map);
        assert_eq!(evidence.groups.len(), 1);
        let group = &evidence.groups[0];
        assert_eq!(group.kept, vec![0], "the first by source order, full stop");
        assert_eq!(group.removed, vec![12]);
        assert_eq!(
            group.disposition,
            DuplicateDisposition::KeptFirstUnresolved,
            "and the pass says outright that keeping it did not close the box"
        );
    }

    /// A whole shell present twice has no locally clean disposition — every
    /// edge carries four occurrences whichever member is kept — so the plain
    /// rule stands, is *recorded* as having stood, and the reduction still
    /// comes out right because every group keeps one.
    #[test]
    fn a_doubled_shell_falls_back_to_the_plain_rule_and_says_so() {
        let mesh = crate::make_box_default().expect("box");
        let mut tris = mesh.tris().to_vec();
        tris.extend_from_within(..);
        let weld_map = (0..mesh.verts().len() as u32).collect::<Vec<_>>();
        let evidence = plan(&tris, &weld_map);
        assert_eq!(evidence.groups.len(), 12);
        for (index, group) in evidence.groups.iter().enumerate() {
            assert_eq!(
                group.disposition,
                DuplicateDisposition::KeptFirstUnresolved,
                "group {index}"
            );
            assert_eq!(group.kept.len(), 1, "group {index}");
            assert_eq!(group.removed.len(), 1, "group {index}");
        }
    }

    /// **The cascade any keep-none rule has to survive, kept as a regression
    /// even though no keep-none rule is landed.** Two triangles of a
    /// double-sided sheet, each present forward and reverse. Judged by local
    /// cleanliness, deleting a group outright reads "clean" for *both* of them
    /// — each one's shared edge is walked once up and once down by the other
    /// group's two faces, and each rim edge by nobody — so both would cancel
    /// and the whole sheet would go. Every group here keeps its first member;
    /// the sheet is halved, its extent survives, and the open boundary that
    /// results is the unchanged gate's to refuse (FU-46).
    #[test]
    fn a_doubled_sheet_is_halved_and_never_cancelled() {
        let tris = vec![[0_u32, 2, 1], [0, 1, 2], [1, 2, 3], [1, 3, 2]];
        let evidence = plan(&tris, &[0, 1, 2, 3]);
        assert_eq!(evidence.groups.len(), 2);
        for group in &evidence.groups {
            assert_eq!(
                group.disposition,
                DuplicateDisposition::KeptFirstUnresolved,
                "a neighbouring duplicate group is not a licence to delete this one: {group:?}"
            );
            assert_eq!(group.kept.len(), 1, "{group:?}");
            assert_eq!(group.removed.len(), 1, "{group:?}");
        }
        assert_eq!(
            evidence.faces_removed(),
            2,
            "half the sheet, not all of it: {evidence:?}"
        );
    }

    #[test]
    fn a_degenerate_welded_triple_is_left_to_its_own_doctrine() {
        // Two corners weld together: a degenerate face, not a duplicate. The
        // edge-collapse-never-dissolve doctrine owns this class; the pass
        // reports it and keeps its hands off.
        let evidence = plan(&[[0, 1, 2], [0, 1, 3]], &[0, 1, 1, 1]);
        assert_eq!(evidence.degenerate_groups_skipped, 1);
        assert!(evidence.groups.is_empty());
    }

    /// **An input over the resource bound skips the WHOLE pass, and is never
    /// refused for it.**
    ///
    /// Two claims, and the second is the one the review found broken. (1) A
    /// raw input larger than the triage admission ceiling used to reach the
    /// decimator and prepare successfully — `prepare_one` runs load → repair →
    /// decimate → hollow and no raw face-count admission existed on that chain
    /// — so turning the ceiling into a repair *error* would create a refusal
    /// class that never existed. It does not: the mesh comes back bit-identical
    /// with a reason on the card. (2) The bound has to cover every action, not
    /// just the triage: it used to guard the shard strip alone while the line
    /// below built the welded face topology on the same unbounded mesh, which
    /// is the heavier of the two, so an over-large input skipped the cheap work
    /// and did the expensive work anyway. Nothing runs now — no strip, no
    /// topology, no duplicate index, no pinch fan.
    ///
    /// The production ceilings are far too large to build a fixture for, so the
    /// boundary is exercised through the parameter production passes: nothing
    /// is loosened, because [`repair_mesh`] still hands
    /// [`repair_mesh_within`] the [`TriageTopologyLimits::production`] values.
    #[test]
    fn an_input_over_the_resource_bound_skips_every_action_instead_of_refusing() {
        let mesh = crate::make_dust_shard_box().expect("shard fixture");
        let refused_everything = TriageTopologyLimits::new(1, 1, 1, 1, 1, 1);
        let repaired = repair_mesh_within(&mesh, refused_everything);
        let evidence = repaired.evidence();
        let skipped = evidence
            .actions_skipped
            .as_ref()
            .expect("an over-large input is not an error, it is a recorded skip");
        assert!(skipped.contains("resource bound"), "{skipped}");
        assert!(
            skipped.contains("no model is refused"),
            "the card has to say so in words too: {skipped}"
        );
        // Not one action ran, and the shard the fixture actually has is still
        // in the mesh -- which is the difference between "the strip was
        // skipped" and "the pass was skipped".
        assert!(!evidence.changed());
        assert!(evidence.shards.faces_removed.is_empty());
        assert!(evidence.shards.skipped.is_none());
        assert!(evidence.duplicates.groups.is_empty());
        assert!(evidence.pinches.splits.is_empty());
        assert!(evidence.pinches.unrepaired.is_empty());
        assert!(evidence.residual.is_none());
        assert_eq!(repaired.mesh().verts(), mesh.verts());
        assert_eq!(repaired.mesh().tris(), mesh.tris());
        assert!(evidence.summary_line().contains("APPLIED=none"));
        // That the *same* fixture has its shard stripped under the production
        // ceilings is `tests/repair.rs`'s
        // `a_dust_shard_is_stripped_and_the_result_passes_the_gate`, which runs
        // in its own process: triage keeps a process-global counter that
        // `triage`'s own "refusal precedes input-proportional work" test reads,
        // so a second in-process triage call here would race it.
    }

    /// The floor is one f32 ULP of the vertex's own magnitude and the ceiling
    /// is an absolute millimetre, so far enough from the origin the floor
    /// overtakes the ceiling and *nothing can be attempted*. That is a
    /// different finding from "everything attempted was too big", and it is
    /// reported as one rather than as a capped attempt of 0.0 mm.
    #[test]
    fn a_cap_below_one_ulp_is_named_rather_than_reported_as_a_zero_attempt() {
        let cap_mm = pinch_split_cap_mm().expect("registered cap");
        let far = [600.0, 0.0, 0.0];
        let refusal = separate(
            far,
            [1.0, 0.0, 0.0],
            &[],
            &PositionRegistry::build(&[far]),
            cap_mm,
        )
        .expect_err("one ULP at 600 mm is above the registered cap");
        let SeparationRefusal::OneUlpExceedsCap { ulp_mm } = refusal else {
            panic!("expected the structural refusal, got {refusal:?}")
        };
        assert!(ulp_mm > cap_mm, "ulp {ulp_mm} cap {cap_mm}");

        // And where candidates *are* tried, the reported attempt is a
        // displacement that actually happened, never 0.0. One ULP is already
        // taken here, so the escalation doubles into a cap that admits only
        // the first step.
        let near = [10.0, 0.0, 0.0];
        let step = f32_ulp_at(near);
        let taken = [near[0] + step, 0.0, 0.0];
        let refusal = separate(
            near,
            [1.0, 0.0, 0.0],
            &[],
            &PositionRegistry::build(&[near, taken]),
            step * 1.5,
        )
        .expect_err("a cap between one and two ULP admits one attempt and refuses it");
        let SeparationRefusal::Capped { largest_attempt_mm } = refusal else {
            panic!("expected a capped attempt, got {refusal:?}")
        };
        assert!(largest_attempt_mm > 0.0, "{largest_attempt_mm}");
    }

    /// **A moved copy may not land on somebody else's vertex.** The first
    /// candidate here is bit-identical to a vertex belonging to unrelated
    /// geometry: accepting it re-fuses under `WeldPolicy::ExactBits` and
    /// creates a *new* point contact at the destination while the evidence
    /// reports an unqualified split. The escalation has to step past it, under
    /// the same registered cap — the cap is never what gives way.
    #[test]
    fn a_split_steps_past_a_vertex_that_is_not_part_of_the_pinch() {
        let cap_mm = pinch_split_cap_mm().expect("registered cap");
        let position = [10.0, 0.0, 0.0];
        let step = f32_ulp_at(position);
        let unrelated = [position[0] + step, 0.0, 0.0];
        let registry = PositionRegistry::build(&[position, unrelated, [0.0, 0.0, 30.0]]);

        let (moved, steps, displacement) =
            separate(position, [1.0, 0.0, 0.0], &[], &registry, cap_mm)
                .expect("two ULP is still far below the registered cap");
        assert_eq!(steps, 2, "one ULP was taken by an unrelated vertex");
        assert!(
            distinguishable(moved, unrelated),
            "{moved:?} landed on {unrelated:?}"
        );
        assert!(!registry.occupied(moved), "{moved:?}");
        assert!(
            displacement > 0.0 && displacement <= cap_mm,
            "{displacement}"
        );

        // Without the registry the very first candidate would have been taken,
        // which is the bug this test exists for.
        assert!(!distinguishable([position[0] + step, 0.0, 0.0], unrelated));
    }
}
