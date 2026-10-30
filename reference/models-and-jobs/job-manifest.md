# Job manifest

This is the denominator the "print-candidate models" metric is measured against. It did
not exist before this document; without it, the corpus metric ("N of 37 emit") can go up
by producing models nobody asked for, which is exactly what happened — 4 of the 24 current
emitters (`Round 25`, `Round 50`, `Round 100`, `Tavey Base`) are plinths, not miniatures.

**The owner's goal:** four copies of a large treant, plus four plates — skeletons, wild
shapes, familiars, and a player-character miniature.

This document lists every model those five jobs require, its sha256 digest, its measured
disposition from the 2026-08-25 corpus re-baseline, and whether hollowing applies at its
size. It does not run the audit tool (`tools/plinth-audit/` does not exist yet) and makes
no printability claim — "emits a `.chitubox` file" and "is a print candidate" are different
claims, and this document only has evidence for the first one.

## Sources consulted

- `../plinth-rerun-2026-08-25/model_list.txt` — the 37-model corpus list.
- `../plinth-rerun-2026-08-25/RERUN-REPORT.md` and `RERUN-STATE.md` — measured dispositions,
  2026-08-25, `develop @ 5d546cb`.
- `corpus/models/` — 8 development fixtures, including a copy of the flagship treant.
- `/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/` — **located by matching file
  counts**: `model_list.txt` names 37 files; this directory holds exactly 37 `.stl` files,
  and a diff of the two (sorted) name lists is empty. This is the source directory the
  digests below were computed against.
- `/Users/craig/code/hughescr/dnd-campaigns/Spore Warden Build - Frog-Nereid Druid 12.md` —
  the player-character build document. Quoted verbatim below.
- `plinth-3d/docs/engineering-journal/04-migration-plan.md` and `docs/status.md` —
  prior owner rulings about the treant's source file and the hollow stage's known
  limitation, cited where they bear on this manifest.

All digests below were computed directly with `shasum -a 256` against the files named, in
this session. Where a disposition is reported, it is quoted from the 2026-08-25 rerun
artifacts, not re-measured — I did not re-run the pipeline.

## Correction to the task framing: the treant IS in the 37

The task brief describing this job says the flagship, `Weeping Treant v1.2.stl`, is "NOT
in the 37." **That is false, and the project's own records already say so.**

```
4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552  minis-to-process/Awakened Tree.stl
4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552  Weeping Treant v1.2.stl (repo-parent)
4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552  plinth-3d/corpus/models/Weeping Treant v1.2.stl
```

Byte-identical (same size too: 21,456,084 bytes, all three). This is not new information —
`docs/engineering-journal/04-migration-plan.md:352` already records, dated 2026-07-31:
**"the owner confirms that 'treant' and 'Awakened Tree' are the same model."** The 37-model
corpus list carries the treant under the name `Awakened Tree.stl`, and it already has a
measured disposition from the 2026-08-25 rerun: **CEILING/REFUSAL** (see below). This is
exactly the class of failure this whole audit stage exists to catch — a filename that
disagrees with the geometry — except here it is a *manifest* miscount, not an engine lie,
and the "flagship is untested" framing this task started from needed re-deriving from the
raw data rather than accepted at face value.

Separately, `docs/evidence/remediation-2026-08-26/EVIDENCE-treant-runaway.log` records a
second, later, unsupervised production run of the same file (6.7 hours, uncapped): 224
trials, 1 commit, 119 regressions, 9,374 patches still outstanding, **no artifact written**.
Different harness (no 1500 s ceiling, ran 2026-08-26/27, not part of the 37-model rerun
batch), but it corroborates the same disposition by an independent measurement: the treant
does not currently emit.

`docs/status.md` also records a second, independent blocker for this model even if the
repair-loop above is fixed: **hollowing does not yet handle multi-cavity models, and "the
flagship treant is one such model"** — so a treant that got past coverage-repair would
still be prepared solid, not hollowed, until that defect is fixed separately.

## Wild Shape known forms — quoted verbatim

From the build document, line 123:

> **Wild Shape known forms (8; swap 1/long rest; can speak while shaped in 2024; Hybrid
> Flora opens Plant forms):** Deinonychus (CR 1 melee), Giant Toad (CR 1, swallow/leap),
> Thorny (CR 1 plant, Regeneration — heals your own HP pool under 2024 rules; DM-confirm),
> Myconid Adult (CR ½ plant, Rapport Spores party telepathy + Pacifying Spores), Pteranodon
> (CR ¼ day flight; note 2025 MM re-typed Giant Owl/Eagle/Elk as Celestials — not
> shapeable), Giant Bat (CR ¼ night flight, Blindsight 120 per current MM), Warhorse (CR ½
> transport), Giant Wolf Spider (CR ¼ climb). Rotation bench: Ape (hands), Giant Badger
> (burrow), Awakened Shrub (CR 0 stakeout bush), Camel, Vine Blight, Constrictor Snake.

**8 known forms** are the ones treated as required below. The **6 rotation-bench** names
are alternates the player can swap in, not a standing requirement; only one of them
(`Giant Badger`, matched below to `Tavey Badger.stl`) has a corpus STL at all, and it is
called out as optional, not counted in the job total.

**`Pteranodon` has no STL anywhere in the 37, in `corpus/models/`, or at the top level of
the campaign directory.** This is the asset gap: a known, currently-prepared form the
character can take in play, with nothing to print. It cannot emit, ceiling, or crash — it
was never asked to run.

## The player character is the model, too

Line 7 of the build document: **"Name: Morel Mudskipper."** The player character is a
Frogfolk × Nereid druid/cleric named Morel Mudskipper — which is also the name of one of
the 37 corpus STLs. `docs/engineering-journal/04-migration-plan.md:428` independently
confirms this: *"`Morel Mudskipper.stl` is the other model the existing code and fixtures
reference by name and is the highest-value addition after the Treant."*

`Morel base.stl` and `Morel Mudskipper.stl` read as a two-part kit — a scenic base plus the
character sculpt that mounts on it (paralleling `Tavey` / `Tavey Base` below). **The base
emits (36 s, `md5 3a57d9f1…`) and the rider ceilings (18 trials / 0 commits / 613 patches
remaining).** A base with nothing to put on it is not a player-character miniature — this
counts as an emitter in the naive 24/37 metric and is unbuildable under this job manifest.

## Judgment calls

Each is stated here rather than resolved silently; the table below follows the call made,
but the alternative reading is not discarded.

1. **`Pteranodon` — no STL, asset gap.** Treated as required-and-missing, not skipped. It
   counts against M (does not emit) precisely because there is nothing to run.
2. **`Bat Cat Owl.stl` may be three familiars in one file, not one.** Bat, Cat, and Owl are
   all valid 5e familiar forms, and the filename names all three. I have not opened the
   mesh to check whether it is one chimeric sculpt or three separate prints fused in a
   single STL. Counted below as **one required model, one physical print** — if it is
   actually three familiars, the true familiar count on this plate is undercounted by up
   to 2, and someone should check the geometry before treating this plate as complete.
3. **`Tavey Badger.stl` is ambiguous between the familiars plate and the wild-shape
   rotation bench.** Two readings:
   - *Familiars reading:* `Tavey` / `Tavey Base` / `Tavey Badger` share a name prefix and
     read as one kit — creature options for a shared base.
   - *Wild-shape reading:* the build document's rotation bench names `Giant Badger
     (burrow)` verbatim, and Badger is not one of the ~15 standard 5e familiar forms (bat,
     cat, crab, frog, hawk, lizard, octopus, owl, poisonous snake, fish, rat, raven, sea
     horse, spider, weasel) — so "Tavey" reading as a badger-form familiar has no rules
     support, while "Giant Badger" as a rotation-bench wild shape has direct textual
     support.
   - **Call made:** wild-shape rotation bench, matched to `Giant Badger`, and marked
     **optional** — it is a bench alternate, not one of the 8 known forms, so it is not
     counted in the required total. Nothing in this manifest depends on the other reading
     being wrong.
4. **`Cat (wild shape - Cat Lord).stl`** — the filename asserts "wild shape," but "Cat"
   appears in neither the 8 known forms nor the 6-name rotation bench quoted above, and no
   campaign document in this session's search turned up a "Cat Lord" patron or feature tied
   to this or any other party member. **Call made: excluded** — the verbatim source list is
   authoritative per the task's own instruction ("quote the line rather than inferring"),
   and this name is not on it. Flagged rather than silently dropped in case another party
   member's sheet (not located in this session) explains it.
5. **`Giant Toad (swallowing).stl` and `Rotting Warhorse.stl`** are excluded as alternate
   poses / different creatures from the required `Giant Toad.stl` and `Warhorse.stl` — a
   swallowing-pose variant and an undead horse are not the CR 1 Giant Toad or CR ½ Warhorse
   the build document names. Not part of this job.
6. **`Zombie Cat.stl` and every `zombie-*`, `Crawler Zombie`, `Decomposing Zombie`, `Plague
   Zombie`, `Skulker Zombie`** — 15 of the 37 models are zombies. None of the five jobs
   (treant ×4, skeletons, wild shapes, familiars, PC mini) is a "zombies" plate. Excluded
   entirely; they inflate the naive 24/37 count without serving any stated job.
7. **`Round 25.stl` / `Round 50.stl` / `Round 100.stl`** — plain display bases, not
   miniatures. Excluded, and are 3 of the 4 base-only files the task's opening framing
   already flagged.
8. **Copy counts for the four plates.** The owner's goal states 4 copies for the treant
   explicitly; it says nothing about how many copies of each skeleton, wild-shape form, or
   familiar are wanted. **Call made: 1 copy per distinct required model**, i.e., one
   specimen of each named creature/form per plate, not an army. If the owner wants multiples
   of any plate item, the copy column below needs revising — this is a guess, not a
   measurement.
9. **Hollowing relevance is inferred from D&D creature size category, not measured
   geometry.** I did not measure any model's bounding box in millimeters — no tool was
   built or run for it in this session, and the project's own hollowing threshold is
   properly a physical-size question, not a size-category one (see `docs/physics.md`).
   Sizes below are D&D 5e statblock size categories, used only as a coarse proxy, and
   marked as such.

## Full manifest

sha256 digests computed by `shasum -a 256` directly against
`/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/<name>` in this session (row 1
also cross-checked against the two other locations named above). Disposition quoted from
`plinth-rerun-2026-08-25/RERUN-REPORT.md` / `RERUN-STATE.md`, measured 2026-08-25 at
`develop @ 5d546cb`, 1500 s ceiling per model unless noted.

**1 — `Awakened Tree.stl` = `Weeping Treant v1.2.stl`** · Treant · **4 copies**
sha256 `4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552`
**CEILING/REFUSAL.** 1500 s ceiling: still grinding, unchanged vs census. Run unbounded
(watcher failure let it continue): terminated ~2997 s with an evidenced
`CoverageClosureRefusal` (`CommitBudgetExhausted` committed=8, candidates_remaining=7709).
Corroborated by a separate 6.7 h unsupervised production run 2026-08-26/27: 224 trials, 1
commit, 119 regressions, 9,374 patches outstanding, no artifact written. Does not emit.
Hollowing: REQUIRED (Huge) — and independently blocked: the hollow stage refuses
multi-cavity geometry, of which this model is a named instance (`docs/status.md` item 6).

**2 — `Skeleton_1.stl`** · Skeletons · 1 copy
sha256 `c6b026d4666d599238c9375f9485d6e94cc6c8f4caca3c1efd950d2fb6276367`
**CEILING** [1500 s]. 36 trials / 1 commit / 7,421 patches remaining. Same as census; does
not emit. Hollowing: not measured (Medium humanoid by size category — likely IRRELEVANT).

**3 — `Skeleton_2.stl`** · Skeletons · 1 copy
sha256 `3b94a6f2aa2ce8a2697e2f749df53128b10a7c99b267cc7a6cac24f60917eab1`
**EMIT** [37 s]. Bytes moved vs census (427→433 records, new md5 `64163937…`).
Hollowing: not measured (Medium — likely IRRELEVANT).

**4 — `Skeleton_3.stl`** · Skeletons · 1 copy
sha256 `323a04cb20b577ae79f3fdf1fa16a7c278910285629e66d072ad9a1c760c6868`
**CEILING** [1500 s]. 26 trials / 0 commits / 5,629 remaining. Does not emit.
Hollowing: not measured (Medium — likely IRRELEVANT).

**5 — `Deinonychus.stl`** · Wild shapes (known form) · 1 copy
sha256 `d1ac66b13d039376b0df2b95ee56be1081346033357ef7fcec7564fce72e72b9`
**EMIT** [79 s]. Rescue structurally identical to prior record (terminal 113, 6 passes, 2
arms). Hollowing: not measured (Medium — likely IRRELEVANT).

**6 — `Giant Toad.stl`** · Wild shapes (known form) · 1 copy
sha256 `9d125b080fa65e5fe0067c07654122bb18512a2c4fef89a2ae31cc07d35be86d`
**CEILING** [1500 s, wrapper-killed]. 4 trials / 1 commit / 14,224 remaining — worst repair
backlog measured in the corpus. Does not emit.
Hollowing: not measured (Large by statblock — plausibly REQUIRED).

**7 — `Thorny Vegepygmy.stl`** · Wild shapes (known form, "Thorny") · 1 copy
sha256 `41d59839ff88b93b80c41a0ad217dac3f48860780c6c34d3b3c7935ea22d82c0`
**EMIT** [14.2 s]. Bytes moved, same forest shape as census.
Hollowing: not measured (Small — likely IRRELEVANT).

**8 — `Myconid Adult.stl`** · Wild shapes (known form) · 1 copy
sha256 `a2e5f783e44760b15b92dcb32618a4161125299920c882a592e305db79c7afc2`
**EMIT** [6 s]. Byte-identical to census artifact (md5 `501613f2…`).
Hollowing: not measured (Medium — likely IRRELEVANT).

**9 — *(no file)* — Pteranodon** · Wild shapes (known form) · 1 copy wanted
No sha256 — no STL exists anywhere searched. **Asset gap** — cannot run, cannot emit. Not
the same failure class as a crash or ceiling; there is nothing to feed the pipeline.
Hollowing: N/A, no geometry to assess.

**10 — `Giant Bat.stl`** · Wild shapes (known form) · 1 copy
sha256 `cd0a931e1f8435b3ee1b8206e1f9efc08072ed3d704ebb91ffe2748a28131ec7`
**CEILING** [1501 s], first measurement. Placement clean (12 legalize passes, zero crevice
refusals); sole blocker is repair grind (46 trials / 1 commit, 6066→6007 remaining). Does
not emit. Hollowing: not measured (Large by statblock — plausibly REQUIRED).

**11 — `Warhorse.stl`** · Wild shapes (known form) · 1 copy
sha256 `a7bf432938ec4fcc6a439de50c18cd76f6e7f6e8a675b60e400b7cfce46b5388`
**CEILING** [1501 s], first measurement. Placement perfect (1 legalize pass, zero
refusals); sole blocker is repair grind (63 trials / 3 commits, 6269→6102 remaining). Does
not emit. Hollowing: not measured (Large by statblock — plausibly REQUIRED).

**12 — `Giant Wolf Spider.stl`** · Wild shapes (known form) · 1 copy
sha256 `b314c9c9b8ff8beb2ca28325d1a759b81229b13f5b5b56bb0cb080ae2c62ee36`
**EMIT** [174 s]. Byte-identical to census artifact (md5 `02cbb785…`).
Hollowing: not measured (Medium — likely IRRELEVANT).

**— `Tavey Badger.stl`** (optional — wild-shape rotation bench "Giant Badger", judgment
call 3; **not counted in the required total**)
sha256 `4cf21f1ed35085391583e1886ed0c6c6919c18c9248733ee2cebda46c6eca77c`
**EMIT** [12 s], first measurement, clean single pass.
Hollowing: not measured (Medium — likely IRRELEVANT).

**13 — `Tavey.stl`** · Familiars · 1 copy
sha256 `6685d19798df0827320d3c69315995013bb01e3d3c02ac64fed7475a169a2af0`
**EMIT** [358 s]. Same disposition as record, +14% wall time; new baseline md5
`e33348f2…`. Hollowing: not measured (Tiny familiar — IRRELEVANT).

**14 — `Tavey Base.stl`** (base for #13) · Familiars · 1 copy
sha256 `a250f32c604478b9cbd55cb6112baa36bd561312bddd5a861cc75f9ecccc5b8f`
**EMIT** [112 s], first measurement, clean (5 sub-mm² ownerless islands noted).
Display-base geometry, not a creature. Hollowing: N/A, base not a creature sculpt.

**15 — `Bat Cat Owl.stl`** (judgment call 2 — may be 3 familiars, not 1) · Familiars ·
1 copy (possibly represents 3)
sha256 `739978789e15c1167933cb810a697a73790f416bd52abe6869b5a955a4166ca9`
**EMIT** [29 s]. Byte-identical to census artifact (md5 `51577d1d…`).
Hollowing: not measured (Tiny familiars — IRRELEVANT).

**16 — `Morel Mudskipper.stl`** · PC miniature (rider) · 1 copy
sha256 `d393dd622225ace1b7de32bfd02f6f74b8073e17940301b116623d8b8b60bf04`
**CEILING** [1500 s]. 18 trials / 0 commits / 613 remaining; legalize clean 1 pass. Does
not emit. Hollowing: not measured (Medium humanoid — likely IRRELEVANT).

**17 — `Morel base.stl`** (base for #16) · PC miniature · 1 copy
sha256 `e9b9901fdb4e8a67f74a3082ab26760cff0d6a976abbcb9f5fae6a4deb1d9712`
**EMIT** [36 s]. Bytes moved, 17× faster than census (2 trials/17 s vs 74 trials/634 s).
Base emits; see "the player character is the model, too" above — the rider does not, so
the miniature as a whole is unbuildable. Hollowing: N/A, base not a creature sculpt.

## Excluded from this manifest (present in the 37, not required by any of the five jobs)

Listed by name only — no digest computed, since they are not required:

- **Zombies (15):** `zombie-1-a` … `zombie-9-b` (9 files), `Crawler Zombie`,
  `Decomposing Zombie`, `Plague Zombie`, `Skulker Zombie`, `Zombie Cat`, `Rotting Warhorse`.
  None of the five jobs is a zombie plate.
- **Alternate poses of a required creature:** `Giant Toad (swallowing).stl` (alt pose of
  #6), already excluded above.
- **Unattested wild shape:** `Cat (wild shape - Cat Lord).stl` — see judgment call 4.
- **Display bases:** `Round 25.stl`, `Round 50.stl`, `Round 100.stl` — plinths, not
  miniatures. (Together with `Tavey Base.stl`, which IS required as part of the familiars
  kit above, these are the 4 base-only files among the naive 24-emitter count the task
  brief refers to.)

That reconciles against the actual 37 files (Pteranodon has no file, so it does not appear
in this count): **16 required rows that have an STL** (17 required rows above, minus the
Pteranodon slot which has none) + **1 optional** (`Tavey Badger`) + **20 excluded**
(15 zombie-family + 1 alt-pose `Giant Toad (swallowing)` + 1 unattested `Cat (wild shape -
Cat Lord)` + 3 display bases `Round 25/50/100`) = **37**. Pteranodon is a required model
with no corresponding file among the 37 — it is the 17th required row precisely because it
is missing, not because it is present and excluded.

## The count that becomes the metric

**17 models required, 9 emitting today, 0 print-candidates.**

- **17 required** — one row per distinct creature/character/kit-part named by the five
  jobs (treant; 3 skeletons; 7 wild-shape forms with an STL + 1 with none; 3 familiar-plate
  files; 2 PC-miniature parts), counting the treant once as a distinct model even though 4
  physical copies are wanted. `Tavey Badger` (optional rotation-bench alternate) is
  excluded from this count by the judgment call above; including it would make it 18
  required / 10 emitting.
- **9 emitting** — `Skeleton_2`, `Deinonychus`, `Thorny Vegepygmy`, `Myconid Adult`,
  `Giant Wolf Spider`, `Tavey`, `Tavey Base`, `Bat Cat Owl`, `Morel base`. Two of these nine
  (`Tavey Base`, `Morel base`) are bases, not creatures — `Tavey Base` completes a working
  kit (both `Tavey` and `Tavey Base` emit), `Morel base` does not (`Morel Mudskipper` does
  not emit, so the kit is incomplete despite the base counting as an emitter).
- **8 not emitting** — the treant (CEILING/REFUSAL), `Skeleton_1` and `Skeleton_3`
  (CEILING), `Giant Toad`, `Giant Bat`, `Warhorse` (all CEILING), `Morel Mudskipper`
  (CEILING), and Pteranodon (no STL — an asset gap, not an engine failure).
- **0 print-candidates** — "emits a `.chitubox` file" is not "is worth printing." No
  independent byte-level audit of any emitted file exists yet (`tools/plinth-audit/` is not
  built). Until that audit runs against these 9 emitters and actually inspects support
  geometry, seat depth, and clearance in the emitted bytes, none of them has been shown
  printable, and this manifest asserts print-candidate status for none of them.
