# Mesh repair census (prose extract from the opening doc-comment of plinth-mesh repair.rs)

Extracted 2026-10-29 for the restart. Prose only; no code. Source: /Users/craig/code/hughescr/dnd-campaigns/plinth-3d/crates/plinth-mesh/src/repair.rs lines 1-69 (the //! comment, comment markers removed). Scope facts: 21 refused/suspect corpus models; three defect classes (113 pinch vertices, 3 duplicate-face groups, ~10 dust shards); all watertight and consistently oriented; no non-manifold edges; point-touch is legitimate; keep-largest forbidden.

---

The automatic, census-scoped repair pre-pass.

**What overturned the old posture.** D-22 said the pipeline deletes the
Blender geometry sweep and *refuses* bad geometry, with the owner repairing
upstream by hand. The owner ruling of 2026-08-10 revised that: repair is
built into the Rust pipeline, fully automatic, no manual steps
(`docs/architecture.md`).
What the ruling did **not** revise is the gate: repair earns no exemptions,
lands strictly upstream of the closed-solid gate, and its output has to pass
that gate unchanged.

**Scope is the census, and nothing else.** A verified harness reproduced the
pipeline gate on all 21 refused/suspect models and found exactly three
defect classes across the whole corpus: 113 pinch vertices, 3
duplicate-face groups, and ~10 dust shards. Every model is watertight, every
model is consistently oriented, none has a non-manifold edge.

**The 113 pinch vertices are no longer a defect class.** Per the owner
ruling of 2026-08-12
(`docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md`,
§4, FU-47): a point-touch is not a modelling defect — resin cannot flow
through a point, so two shells meeting at one vertex in two locally
disconnected fans is a legitimate fingertip touch, not a hole. The
closed-solid gate (`plinth_hollow::validate_closed_mesh`) no longer refuses
on it, so a repair action that existed only to make a pinched mesh pass
that gate is unnecessary — splitting the vertex would be an unjustified
mutation of the owner's geometry to satisfy a check that no longer runs.
**So this module repairs the two remaining census classes** and
deliberately implements no speculative third: hole filling, re-winding and
degenerate collapse have no corpus evidence behind them, and the
edge-collapse-never-dissolve doctrine binds any future degenerate work.

**The two actions, in the one order that is safe.**

1. **Dust-shard strip**, delegated whole to [`crate::triage_components`] —
   the same six-criteria conjunction the hollow stage already runs, applied
   to raw input instead of only to a post-gate shell. Keep-largest stays
   forbidden and no criterion moves; the only new thing is *when* it runs.
2. **Exact-duplicate-face removal**, re-derived from a fresh topology
   whenever step 1 changed a face, so a stripped shard cannot leave a
   duplicate-group decision looking at adjacency that no longer exists.

**The pass may decline; it may never refuse.** [`repair_mesh`] returns a
[`RepairedMesh`] and not a `Result`, and that signature is the invariant
rather than a note about one: "declining to repair is always available;
refusing a model never is" (the 2026-08-10 ruling, FU-45). This stage is
mandatory and unconditional on the `prepare` chain, so *any* internal
failure it could raise — a resource bound, a topology build, a triage
arithmetic path, the constants registry — would be a **new refusal class
invented by a repair**, which is the exact inversion of the ruling. So the
whole pass, both actions, is gated on one admission check up front, and
every internal failure below it lands in
[`RepairEvidence::actions_skipped`]: the caller gets its own mesh back, bit
for bit, and the unchanged gates downstream judge exactly what they judged
before this stage existed.

**Byte-identity is a contract, not an optimization.** A mesh with neither
defect is returned exactly as it arrived — same vertex order, same
triangle order, same bits — so a clean model cannot be perturbed by a stage
that had nothing to do. `repaired_clean_mesh_is_byte_identical` proves it
through a binary-STL round trip, not by inspection. A mesh whose only
defect is a point-touch is byte-identical too, for the same reason: FU-47
made the pinch not a defect this module has anything to do with.

**Determinism.** Nothing here reads a clock, an environment variable, or an
unseeded RNG, and nothing here is parallel: every collection is a `BTree*`
or an explicitly sorted `Vec`. There is therefore no thread-count sweep to
run on this stage — `repairing_twice_is_bit_identical` pins the property
the sweep would be testing for.
