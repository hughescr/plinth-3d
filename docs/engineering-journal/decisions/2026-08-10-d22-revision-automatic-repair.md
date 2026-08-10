# Decision — mesh repair is built into the pipeline, and D-22 is revised

**Ruling given 2026-08-10 by the owner (Craig Hughes).** Mesh repair **is** built
into the Rust pipeline, fully automatic, with no manual steps. The old posture —
the pipeline refuses bad geometry and the owner repairs it upstream in Blender —
is overturned.

## What this overturns

`06-conventions.md`'s decision-gate table records **D-22**, guide-ratified
2026-08-01:

> Delete Blender geometry sweep and refuse bad geometry

Half of that stands and half of it does not. **The Blender sweep stays deleted**
— nothing here restores a Python or Blender path, and §8's absolute prohibition
on new Python is untouched. What is revised is the second half: "refuse bad
geometry" was, in practice, "refuse bad geometry *and leave the owner to fix it
by hand*", and that manual step is now the pipeline's own job. The refusal
itself is not softened; it moves to being the **last** resort rather than the
first response.

## The standing doctrine this establishes

Three sentences, and they bind future work, not just this change:

1. **Every refusal the pipeline still emits becomes an automation work item.**
   Analyze the class, automate its repair, or prove the input is genuinely
   unprintable. A refusal is a to-do, not a resting state.
2. **Refusal gates stay strict.** Automation lands *upstream* of gates. A gate
   threshold is never raised, loosened, or bypassed to make a model pass — that
   is FU-8b's standing lesson and this ruling does not touch it.
3. **The edge-collapse-never-dissolve doctrine** from the legacy playbook binds
   any future degenerate-repair work.

## The census that motivated the scope

The scope of the first pre-pass is not a guess about what meshes might be wrong.
It is the measured defect surface of the 21 refused/suspect models, taken by a
harness that reproduced the pipeline gate **21/21** using
`build_face_topology(WeldPolicy::ExactBits)` — the exact call
`plinth_hollow::validate_closed_mesh` makes.

**What the census found present:**

| Defect class | Corpus incidence |
| --- | --- |
| Pinch vertices (`TOPOLOGY_CUT_POINT_ONLY`, self-contacts) | **113**, at most 25 in one model |
| Exact duplicate-face groups | **3** (Giant Bat 4 faces / 2 groups, Plague Zombie 2 / 1) |
| Dust shards (4–46 faces, span ≤ 0.26 mm, \|vol\| ≤ 1.2e-4 mm³, kissing the body at one vertex) | **~10** |

**What the census found absent** — and this is why the pre-pass implements no
hole filling, no re-winding, and no degenerate collapse:

- every one of the 21 is **watertight**: 0 boundary edges;
- every one is **consistently oriented**: 0 same-direction edges;
- every one has **0 non-manifold edges**;
- every one has **one positive-signed-volume shell**;
- an epsilon weld at 1e-4 mm changes none of it.

**Where the damage is born.** 13 of the 21 trip the gate at RAW. The other 7 —
Cat wild shape, Morel base, Rotting Warhorse, Tavey Base, Thorny Vegepygmy,
zombie-8-a, zombie-9-b — are **clean raw**, and the **decimator manufactures**
their damage: Cat 82,564 → 53,150 faces gains 2 duplicate faces and 31
point-only-cut faces; zombie-9-b 373,748 → 51,974 gains 8 duplicate faces. The
Crawler Zombie's existing damage is *amplified* 50 → 634 cut faces by decimation.
Where the decimator declines to reduce, meshes pass through byte-identical.
`Warhorse.stl` is pristine at every stage and its hollow refusal is born inside
the hollowing voxel/marching-cubes/Boolean path, not in its input.

That split is why the diagnostic that carries these numbers forward
(`cargo run --release -p plinth-pipeline --example repair_probe`) reports four
columns per model — raw, raw+repair, decimated, decimated+repair — rather than a
before/after pair. The pre-pass runs on the loaded input, so the decimator-borne
class is **out of its reach by construction**, and that is filed as FU-44 rather
than papered over.

## What the pre-pass actually does to that corpus (measured at this tip)

Not a projection from the census — a run of the landed diagnostic against the
production gate, 2026-08-10, over all 37 STLs in `minis-to-process`:

| | |
| --- | --- |
| Models refused by the gate at RAW | **14** |
| …of those, rescued by the pre-pass | **14 — all of them** |
| Pinches the pre-pass could not open, on raw input | **0** |
| Duplicate groups reaching action 2 on raw input | **0** — the shard strip removes the census's three first |
| Models CLEAN raw but refused after decimation | **11** |
| Models still refused after repair on the probe's *decimated* column | **14** (the 11 above, plus Crawler Zombie, Giant Wolf Spider, zombie-4-a) |

Re-measured 2026-08-11, over all 37 STLs, after the review **removed** the
duplicate action's keep-none disposition rather than merely bounding it (FU-46)
and made the pinch split test its candidates against every position in the mesh
(not just the pinch's own copies). The raw column — the only one the pre-pass
runs on in production — is **unchanged by both**: 14 refused, 14 rescued, 0
pinches capped, 0 duplicate groups reaching action 2, 0 actions skipped anywhere
in the corpus, and 118 shard faces plus 94 pinch vertices repaired in 96 fan
copies. No group is deleted outright anywhere at either stage, because no rule
that could do so exists. The decimated column shows 14 still refused after
repair, because those models were previously passing *by deleting the
decimator's manufactured coincident pairs whole* (`dup-faces = 2 × dup-groups`,
no survivor) — precisely the deletion FU-46 puts to the owner. That column is
diagnostic, not production.

The eight corpus models are all PASS at every stage, including the
429,120-face Weeping Treant, with zero repair actions between them — which is
why the frozen-digest composition can safely stay out of this.

Two rows are worth reading individually. **Giant Toad** (459,072 faces) and
**Tavey Badger** flip CUT → PASS on the shard strip alone, exactly as the
census predicted. **Deinonychus** carries 25 pinch vertices — the census's
worst case — and all 25 split, in 27 fan copies, none capped. **zombie-2-a**
reports `n/a` in the decimated columns because `plinth-decimate` errored on it;
that is a separate refusal, honestly reported by the probe rather than hidden,
and it belongs to the decimation stage rather than to this work.

The 11 clean-raw/refused-decimated models are more than the 7 the census named.
Applying the pre-pass *after* decimation does **not** rescue them under the
landed rule: their defects are coincident pairs the decimator manufactured, and
clearing them would need a group deleted with **no survivor** — an action this
pass does not have and will not have before the owner rules on FU-46. What the
probe measures is therefore the shape of the problem, not a fix waiting to be
bolted on. See FU-44.

## What was built

`plinth_mesh::repair_mesh`, run in `plinth-pipeline`'s `prepare_one`
immediately after `load_binary_stl` and therefore upstream of every gate in the
chain. Three actions, in this order:

1. **Dust-shard strip** — delegated whole to the existing `triage_components`
   six-criteria conjunction, now run on raw input instead of only on a post-gate
   shell. Keep-largest stays forbidden; no criterion moved. On the corpus this
   rescues Giant Toad and Tavey Badger outright and materially reduces six
   others. A raw input that triages as **pre-hollowed** is skipped and recorded,
   because that refusal belongs to the hollow stage where it already lives and
   this pre-pass must not invent a new one at a new place in the chain.
2. **Exact-duplicate-face removal** — faces sharing a welded vertex triple
   reduce to their earliest member, **and to nothing else**. That is the one
   rule the scope authorized ("drop faces sharing an identical vertex triple,
   keeping the first by deterministic order") and the pass implements exactly
   it: the survivor is the first by source order regardless of winding, and no
   group is ever deleted outright. Two wider rules were built, measured, and
   **reverted to this one** rather than landed — deleting an edge-isolated
   coincident stack with no survivor, and promoting a later opposite-wound
   member when it leaves the local edges cleaner. The first is the dangerous
   one: it removes geometry with nothing in its place, has no size bound, and
   can take a model from gate-`Err` to gate-`Ok` **by deleting the geometry
   being refused** — a pre-pass that can do that is not upstream of the gate,
   it is a hole in it. The second is an orientation repair for a defect class
   the census counted zero of. Both are the open scope question on FU-46, owner
   ruling requested, and the shapes the conservative rule cannot finish (a
   halved doubled sheet, a lone coincident flap) reach the unchanged gate and
   are refused — which under the standing doctrine is the correct outcome and
   the work item. What the pass *does* record per group is whether keeping the
   first left the group's three edges walked once in each direction
   (`dup-keep-one`) or not (`dup-unresolved`); that is evidence about the
   result, never a choice about which face lives. This action runs *before* the
   pinch split, because a duplicate severs the adjacency the pinch analysis is
   computed from.
3. **Pinch-vertex split** — each pinch vertex is opened into one copy per fan,
   fan 0 staying exactly where it was. The census's own caveat drove the design:
   `exact_weld` keys on `to_bits()`, so a displacement that rounds away is
   re-fused, and `write_binary_stl` **refuses** a coordinate that is not exactly
   f32. Each moved copy is therefore snapped to the f32 grid and *proved* to be
   distinguishable under both the weld and the round trip before it is accepted,
   escalating in powers of two from one f32 ULP and bounded above by the new
   registered `REPAIR_PINCH_SPLIT_MAX_MM` (5e-5 mm — 280× below the 14 µm narrow
   pixel pitch). At mini scale the realized displacement is about 2 nanometres.
   **Distinguishable from what** is the load-bearing word, and it means every
   position in the mesh, not just the pinch's own copies: a copy that lands
   bit-exactly on some unrelated vertex is re-fused by the very weld the nudge
   exists to survive, creating a fresh point contact at the destination while
   the evidence reports an unqualified success. Every candidate is therefore
   tested against one gathered registry of all mesh positions (D-37: one batched
   pass, not a scan per candidate), and each committed copy joins it.

## What was deliberately not done

- **The gate is byte-identical.** `validate_closed_mesh` gained a doc comment and
  `pub`; not one of its three checks changed. Repair earns no exemption, and
  `crates/plinth-hollow/tests/repair_gate.rs` requires each fixture to be
  *refused* by the unmodified gate before repair and to pass it after.
- **No cap, floor, or threshold was raised, lowered, or bypassed.** A pinch that
  cannot be separated below the registered ceiling is recorded as unrepaired and
  handed to the gate. The ceiling is a refusal boundary, not a knob.
- **A model needing no repair passes through byte-identical**, proved through a
  binary-STL round trip rather than by inspection
  (`a_clean_mesh_passes_through_byte_identical`).
- **The pre-pass has no failure disposition at all.** `repair_mesh` returns a
  mesh, not a `Result`, and `PipelineError` has no `Repair` variant — the type
  is the invariant rather than a note about one. This stage is mandatory and
  unconditional on the `prepare` chain, so *any* internal failure it could raise
  would be a new hard-refusal class invented by a repair, which is the inversion
  of this ruling. So the whole pass is gated on one resource admission up front,
  and every internal failure below it — the topology build, the triage, the
  constants registry — lands in `RepairEvidence::actions_skipped` with the
  caller's own mesh handed back bit for bit. The bound covers **every** action,
  not one of them: it originally guarded only the shard strip while the welded
  face-topology build ran unconditionally on the same unbounded mesh one line
  later — the heavier of the two — so an over-large input skipped the cheap work
  and paid for the expensive work anyway (FU-45). A >8M-face raw STL now gets
  *less repair* and exactly the same admission it had before this stage existed.
  Declining to repair is always available to this pass; refusing a model never
  is.
- **And it never empties one.** A plan that would leave a mesh with no triangles
  at all is rolled back whole and the input returned unchanged with typed
  `EmptyResultRefusal` evidence, rather than applied and then discovered
  downstream as a stage-internal `MeshError` attributed to the operator's file.
  Under the conservative duplicate rule this guard cannot fire — the strip never
  strips the main component, the duplicate rule always keeps a survivor, the
  split only appends — and it is kept anyway, because "by construction" is an
  argument about today's three actions and FU-46's open question is precisely
  whether a fourth that deletes with no survivor should exist.
- **Four rows were filed, and two questions in them are not this change's to
  answer.** FU-43 asks the hollowing owner whether pinch vertices harm
  voxelization at all — the gate folds `TOPOLOGY_CUT_POINT_ONLY` into a refusal
  whose message claims only "closed, consistently oriented solid", which the
  census proves these meshes *are*. FU-44 records the decimator-manufactured
  class. FU-45 records the shard strip's raw-input resource asymmetry. FU-46
  records the duplicate shapes that stay refusals and asks the owner whether
  deleting larger zero-volume double-sided geometry — a sheet, a cape, a
  doubled shell — is in scope at all, since that is a decision about what may be
  silently removed from a customer's model rather than a technical one.

## Recording note

This entry is the dated record of the D-22 revision, and the decision-gate table
in `06-conventions.md` carries it as correction row **C-7**, appended after C-6
in that table's append-only order and pointing here. No historical row was
renumbered, edited, or displaced, and D-22's original row stands unaltered
beside it — which is the whole point of a correction table: the revision is
readable *as* a revision, not as a rewrite of what 2026-08-01 believed.
