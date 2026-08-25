//! The automatic, census-scoped repair pre-pass.
//!
//! **What overturned the old posture.** D-22 said the pipeline deletes the
//! Blender geometry sweep and *refuses* bad geometry, with the owner repairing
//! upstream by hand. The owner ruling of 2026-08-10 revised that: repair is
//! built into the Rust pipeline, fully automatic, no manual steps
//! (`docs/architecture.md`).
//! What the ruling did **not** revise is the gate: repair earns no exemptions,
//! lands strictly upstream of the closed-solid gate, and its output has to pass
//! that gate unchanged.
//!
//! **Scope is the census, and nothing else.** A verified harness reproduced the
//! pipeline gate on all 21 refused/suspect models and found exactly three
//! defect classes across the whole corpus: 113 pinch vertices, 3
//! duplicate-face groups, and ~10 dust shards. Every model is watertight, every
//! model is consistently oriented, none has a non-manifold edge.
//!
//! **The 113 pinch vertices are no longer a defect class.** Per the owner
//! ruling of 2026-08-12
//! (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
//! §4, FU-47): a point-touch is not a modelling defect — resin cannot flow
//! through a point, so two shells meeting at one vertex in two locally
//! disconnected fans is a legitimate fingertip touch, not a hole. The
//! closed-solid gate (`plinth_hollow::validate_closed_mesh`) no longer refuses
//! on it, so a repair action that existed only to make a pinched mesh pass
//! that gate is unnecessary — splitting the vertex would be an unjustified
//! mutation of the owner's geometry to satisfy a check that no longer runs.
//! **So this module repairs the two remaining census classes** and
//! deliberately implements no speculative third: hole filling, re-winding and
//! degenerate collapse have no corpus evidence behind them, and the
//! edge-collapse-never-dissolve doctrine binds any future degenerate work.
//!
//! **The two actions, in the one order that is safe.**
//!
//! 1. **Dust-shard strip**, delegated whole to [`crate::triage_components`] —
//!    the same six-criteria conjunction the hollow stage already runs, applied
//!    to raw input instead of only to a post-gate shell. Keep-largest stays
//!    forbidden and no criterion moves; the only new thing is *when* it runs.
//! 2. **Exact-duplicate-face removal**, re-derived from a fresh topology
//!    whenever step 1 changed a face, so a stripped shard cannot leave a
//!    duplicate-group decision looking at adjacency that no longer exists.
//!
//! **The pass may decline; it may never refuse.** [`repair_mesh`] returns a
//! [`RepairedMesh`] and not a `Result`, and that signature is the invariant
//! rather than a note about one: "declining to repair is always available;
//! refusing a model never is" (the 2026-08-10 ruling, FU-45). This stage is
//! mandatory and unconditional on the `prepare` chain, so *any* internal
//! failure it could raise — a resource bound, a topology build, a triage
//! arithmetic path, the constants registry — would be a **new refusal class
//! invented by a repair**, which is the exact inversion of the ruling. So the
//! whole pass, both actions, is gated on one admission check up front, and
//! every internal failure below it lands in
//! [`RepairEvidence::actions_skipped`]: the caller gets its own mesh back, bit
//! for bit, and the unchanged gates downstream judge exactly what they judged
//! before this stage existed.
//!
//! **Byte-identity is a contract, not an optimization.** A mesh with neither
//! defect is returned exactly as it arrived — same vertex order, same
//! triangle order, same bits — so a clean model cannot be perturbed by a stage
//! that had nothing to do. `repaired_clean_mesh_is_byte_identical` proves it
//! through a binary-STL round trip, not by inspection. A mesh whose only
//! defect is a point-touch is byte-identical too, for the same reason: FU-47
//! made the pinch not a defect this module has anything to do with.
//!
//! **Determinism.** Nothing here reads a clock, an environment variable, or an
//! unseeded RNG, and nothing here is parallel: every collection is a `BTree*`
//! or an explicitly sorted `Vec`. There is therefore no thread-count sweep to
//! run on this stage — `repairing_twice_is_bit_identical` pins the property
//! the sweep would be testing for.

use std::collections::{BTreeMap, BTreeSet};

use plinth_core::M0ConstantsError;
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
            && (!self.shards.faces_removed.is_empty() || !self.duplicates.groups.is_empty())
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
            "shard-faces={} dup-groups={} dup-faces={} dup-keep-one={} dup-unresolved={}",
            self.shards.faces_removed.len(),
            self.duplicates.groups.len(),
            self.duplicates.faces_removed(),
            self.duplicates.groups_with(DuplicateDisposition::KeepOne),
            self.duplicates
                .groups_with(DuplicateDisposition::KeptFirstUnresolved),
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
/// shard strip never strips the main component, and the duplicate rule keeps
/// the first member of every group — so every action leaves at least one face
/// by construction. The check stays because "by construction" is an argument
/// about today's two actions and the consequence of being wrong about it is a
/// stage-internal error wearing the operator's filename. FU-46 asked whether
/// an action that deletes a group with **no survivor** should exist and the
/// owner answered NO on 2026-08-12
/// (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
/// §6), so no such action is coming and this guard is not waiting on one. It
/// stays because the thickening repair the ruling names as the future
/// candidate still changes the face set, and because a guard is far easier to
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
    /// authorize and the census found zero corpus incidence for.
    ///
    /// **RULED 2026-08-12 by the owner**
    /// (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
    /// §6): **deletion is OUT OF SCOPE for automatic repair.** The scope
    /// question this comment used to call open is closed — a model carrying
    /// zero-volume doubled geometry **stays refused until a real fix exists**,
    /// keep-none stays forbidden in every form, and the conservative
    /// keep-first rule stands exactly as landed. **Thickening** is the named
    /// future candidate: give a zero-volume double-sided sheet a printable
    /// thickness rather than silently delete the owner's geometry. So the pass
    /// keeps the first, records the shape here, and the refusal is the resting
    /// state by ruling rather than by default.
    KeptFirstUnresolved,
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
    // `BTreeMap` edge index over ~3x the face count, which is *heavier* than
    // the triage it was standing in for. A raw STL over the ceiling therefore
    // skipped the cheap action and performed the expensive one anyway, and the
    // bound's own ledger row (FU-45) claimed a protection it did not provide.
    // So admission is proven once, here, before any input-proportional work at
    // all, and the capability it returns is what action 1 consumes.
    let admission = TriageTopologyAdmission::from_raw_mesh(mesh, limits)?;

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
    let topology = build_face_topology(stage.as_ref().unwrap_or(mesh), WeldPolicy::ExactBits)?;

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
            stage = Some(reduced);
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
    //
    // `changed()` is true exactly when one of the two actions edited a face,
    // and each of them writes `stage` when it does.
    let (verts, tris) = match &stage {
        Some(stage) => compact(stage.verts(), stage.tris()),
        None => unreachable!("a recorded change wrote no geometry"),
    };
    // Both removing actions already refused to take the last face -- so this
    // is a net under the two of them and under any action added later,
    // checked before the mesh is built rather than discovered as
    // `MeshError::NoTriangles` two stages downstream.
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
/// no per-item topology rebuild — and then read by the duplicate disposition.
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
/// landed — and the first of them is now **refused by name** by the owner's
/// 2026-08-12 ruling
/// (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
/// §6), not merely deferred:
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
/// Neither was a thing to decide inside an implementation, and the owner has
/// now decided the one that mattered. **RULED 2026-08-12 (§6): deletion is out
/// of scope for automatic repair.** The conservative rule stands as landed, and
/// the shapes it cannot finish — a free-floating coincident stack, a doubled
/// sheet — reach the unchanged gate and are refused there, permanently, until a
/// repair exists that keeps the owner's geometry. **Thickening** is the named
/// future candidate for building that repair; **keep-none is not, in any
/// form.** Under the standing doctrine of the 2026-08-10 ruling the refusal
/// *is* the correct outcome: it becomes the work item. The ruling also records,
/// as fact, that **no real corpus model exhibits the doubled-sheet case** — the
/// reproductions are synthetic and the raw corpus's three coincident-face
/// groups are single-pair slips the shard strip removes before this action ever
/// sees them — so the class refused here is, today, empty on real input.
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
/// choose, and neither is landed behaviour: keeping a later opposite-wound
/// member (an orientation repair for a defect class the census found zero of)
/// and keeping **none** at all for an edge-isolated stack. The second is the
/// one that mattered — it is the only rule in the pass that could delete
/// geometry with no survivor, it had no size bound, and it could flip a gate
/// refusal into a pass by deleting the geometry the gate was refusing. A
/// pre-pass that can do that is not upstream of the gate, it is a hole in it —
/// and that is the reasoning the owner adopted in the **2026-08-12 FU-46
/// ruling** (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
/// §6), which puts deletion **out of scope for automatic repair** rather than
/// leaving the question open. Nothing here is waiting on a ruling any more.
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

    /// The empty-result guard, exercised directly because no action this pass
    /// performs can reach it any more — every one of the two leaves at least
    /// one face by construction. FU-46's ruling (2026-08-12, §6) refused the
    /// no-survivor rule outright, so the guard is not held open for that; it is
    /// held open for the thickening repair the same ruling names as the future
    /// candidate, and for the general principle that a face-set-changing action
    /// must never empty a model. What it *does* is pinned either way: roll
    /// everything back, hand the caller its own mesh, and say so on the card.
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
    /// authorized keep-the-first and nothing else, and the owner's **FU-46
    /// ruling of 2026-08-12**
    /// (`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`
    /// §6) put deletion **out of scope for automatic repair**, so this test
    /// pins a ruled outcome rather than an interim one.
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
    /// the census counted zero of, so it is not made: the first is kept and the
    /// group is recorded as unresolved. FU-46's 2026-08-12 ruling (§6) closed
    /// the scope question against widening — the conservative keep-first rule
    /// stands as landed — so this is pinned behaviour, not a placeholder.
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
    /// topology, no duplicate index.
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

}
