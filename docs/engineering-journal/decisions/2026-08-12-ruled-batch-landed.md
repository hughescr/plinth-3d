# Record — the ruled batch lands: twelve rows, four lanes, one corpus rerun

**Companion to [`2026-08-12-owner-rulings-fu-batch.md`](2026-08-12-owner-rulings-fu-batch.md).**
That record carries the owner's ten rulings as they were given. This one carries what
was actually built against them, what the corpus measured afterwards, the FU-61
verdict, and every question the four lanes filed rather than guessed at.

**What this record is, and is not.** It is a record. The commit that lands it touches
documentation only: **no gate, cap, threshold, bound, constant or registry entry moves
in it.** Where it names new work it files a new ledger row at the next free number
(FU-62 … FU-69) and **renumbers nothing**. Where a lane took a conservative path
because no ruling covered the question, that path is recorded here as taken — not as
authorized — and the question stays open.

**Nothing was pushed.** `develop` is local-only and ahead of `github/develop`; no
remote was touched at any point in this batch.

---

## 1. What landed, per row

Twelve rows were worked. Ten were ruled on 2026-08-12; FU-53 was mandated by the FU-55
ruling; FU-61 was filed by the FU-51 ruling and is the verification that unblocked it.

| FU | Ruling | State after this batch | Where it landed |
| --- | --- | --- | --- |
| FU-36 | §10 — fix the pole seam now, as principle | **LANDED** | `53d70d2`, docs `18f30e0`, retraction `7becf8e`; merged **`647c991`** |
| FU-37 | §8 — persist the *requested* wall on the top-level `HollowReport` | **LANDED**, then **COMPLETED** when review found the field died inside `seal_and_emit` | `679b78e`, plumbing `790fed7`; merged **`b71ddbc`** |
| FU-38 | §5 — all four decision-relevant instruments move to the real delicacy field | **LANDED** (last of the batch; see §5) | `fbbc099` directly on `develop` |
| FU-47 | §4 — a point touch is not a defect; connectivity means resin can flow | **LANDED**, all three phases | `9cb57df`, `e93a4e1`, review fixes `9e4317f`/`c90a996`; merged **`b9c8a6b`** |
| FU-50 | §3 — replace the frozen rasterizer, prove the new marked set first | **LANDED**, proof-before-ship satisfied | `6b4ed20`, tail removal `1ad34e6`; merged **`b71ddbc`** |
| FU-51 | §9 — verify the bores first, then re-record the Sphere baseline **once** | **RESOLVED** | `44b8df1` directly on `develop` |
| FU-53 | mandated by §1 — the shipped wall becomes the requested wall | **LANDED** | `c07f5a9`, `3a9b98b`, cross-constant test `385eaf9`; merged **`b71ddbc`** |
| FU-54 | §7 — bound the cavity-repair re-selection domain, at eight | **LANDED** | `13f68c4`; merged **`b71ddbc`** |
| FU-55 | §1 — an under-walled shell never ships; the solid reserve is the floor | **LANDED** | `a3ef816`; merged **`b71ddbc`** |
| FU-56 | standing doctrine — every residual refusal is an automation item | **LANDED** | `1d407a0`, fix `6ddf051`; merged **`b9decd5`**; record `4084c40` |
| FU-57 | §2 — sidecar, card and renders, all three | **LANDED** | `cc891a8`, fix `6ddf051`; merged **`b9decd5`**; record `4084c40` |
| FU-61 | filed by §9 — prove every bore fully penetrates | **CONFIRMED** (see §4) | harness committed at `44b8df1` |

Two further docs-only commits belong to the batch: **`4d5de5e`** corrects six stale
comments in `crates/plinth-mesh/src/repair.rs` that still called FU-46's scope question
open after §6 ruled it (deletion is out of scope, the doubled-sheet class stays refused),
and **`4084c40`** writes the dated journal section the coverage lane never wrote.

**Lane branch tips are unchanged and were not deleted:** `feature/hollow-walls@8818f7b`,
`feature/coverage-closure@6ddf051`, `feature/connectivity-doctrine@c90a996`,
`feature/sphere-generator@7becf8e`.

**FU-8b compliance, checked independently of any lane's own claim.** Across the whole
batch (`0ee2814..develop`) the constants diff contains **additions only**:
`HOLLOW_CAVITY_MARCH_MARGIN_MM` (derived, one voxel body diagonal, with a cross-constant
test that fails if `HOLLOW_PITCH_MM` moves and it does not), `DEFAULT_MAX_CAVITY_REPAIR_PASSES`
= 8, `DEFAULT_CAVITY_DISTANCE_INDEX_BYTES`, `TOPOLOGY_CUT_CLOSED_SOLID_MASK`, and one
**tightening** — the emit closure bound from `1e-12` to `1e-15`. `max_raster_work_items`,
`HOLLOW_WALL_DEFAULT_MM` (2.5 mm), the support gate, the 6.0 mm contact radius, the
0.16 mm² island floor and the zero uncovered-area tolerance are all byte-unchanged. No
Python anywhere in the diff. `minis-to-process/` and `corpus/models/` untouched.

**Two merges resolved conflicts by measuring rather than text-merging**, and both are
recorded because the resolution changed a claim:

- `b9decd5` (coverage): `crates/plinth-pipeline/tests/prepare_folder.rs` carried two
  branches' competing narratives about `Sphere.prepped.stl`. Built and run under each
  candidate resolution, the measured truth is HEAD's: the fixture still fails during
  **Routing at attempt 0**, before the closure controller can look at it, so it still
  lands on the pre-existing `DrilledShellRefusedBySupport` reserve substitution. The
  coverage branch's assumption that FU-56 would intercept it was wrong and was not
  merged in.
- `b9c8a6b` (connectivity): two `plinth-hollow` integration tests and FU-54's dedicated
  pass-limit integration test lost their **premise**, not merely their text — flow-based
  cavity selection now excludes the far lobe and the non-well-composed pinch before
  `well_composed_cavity_component` ever runs. The superseded tests were replaced with
  doctrine-correct successors and the FU-54 bound is still proved directly by
  `well_composed.rs`'s own unit test. Two corpus fixtures were fully re-measured against
  the merged engine rather than guessed: the treant's cavity splits into **13** flow
  interiors, not 9, and every derived number was re-pinned with `from → to`.

---

## 2. The corpus rerun

**One `plinth prepare` run, 37 minis, on the fully-merged engine.**

```
plinth prepare --input-models-dir ../minis-to-process \
               --output saturn4-ultra \
               --out-dir <scratch>/ruled-batch-rerun
```

Release binary built from `develop` at **`647c991`** (all four lane merges in), default
dense orientation sweep, registered 2.5 mm wall, no cache, single instance, run to
completion. Output went to a scratch directory outside the repo and **is not committed**.

**Why the rerun is HEAD's behaviour even though it was built at `647c991`.** The four
commits since (`44b8df1`, `4d5de5e`, `4084c40`, `fbbc099`) touch `crates/*/src/` in three
files only, and the diff on all three is confined to a `#[cfg(test)]` module
(`plinth-hollow/src/drains.rs`) or to comments (`plinth-hollow/src/drilling.rs`,
`plinth-mesh/src/repair.rs`). Checked, not assumed: filtering the diff to non-comment,
non-test lines is empty for `drilling.rs` and `repair.rs`, and for `drains.rs` every
changed line is inside the test module's assertions.

### Summary line

```
SUMMARY  emitted=8  gate-refused=0  failed=29  stopped=0
```

### Deltas against the recorded baseline (the 2026-08-11/12 run FU-56 was opened from)

| Class | Baseline | This run | Delta |
| --- | ---: | ---: | ---: |
| Artifacts emitted | 1 | **8** | **+7** |
| `PreWriteGate::UncoveredIslands` refusals | 21 | **0** | **−21** |
| Coverage-closure controller refusals (FU-56, did not exist) | 0 | **19** | +19 |
| Rasterizer work-cap refusals on flat bases (FU-50) | 2 | **0** | **−2** |
| Sealed-support stage failures (`Failed(phase)`, FU-58) | 6 | 10 | +4 |
| Drain-planning cap refusals (FU-59, folded into a later class) | 2 | 2 | 0 |
| Models with **no disposition at all** (baseline run was killed) | 7 | **0** | **−7** |

The baseline's 7 no-disposition models make a strict per-class subtraction unsound for
the two failure classes, so the honest comparison is per model over the 21 the baseline
names by name.

### The eight artifacts

`Bat Cat Owl`, `Plague Zombie`, `Round 25`, `Round 50`, `Round 100`, `Skulker Zombie`,
`Tavey Badger`, `zombie-3-a`.

- **`Round 50` and `Round 100` are FU-50's acceptance criterion, closed end to end.**
  FU-50's implementer recorded as an open question that the two flat bases had been
  proved to pass *rasterization* but never run through the full pipeline. They now emit
  artifacts. That question is answered by measurement.
- **Four of the 21 gate-refused minis now emit** — `Bat Cat Owl` (1 blocking defect at
  baseline), `Plague Zombie` (3), `Skulker Zombie` (1), `Tavey Badger` (1). This is the
  first time in the project's record that any real mini has produced an artifact.
- **`Round 25` was the baseline's only artifact** and still emits.
- **Every one of the eight is `Solid { reason: BelowThreshold }`.** Not one shell shipped.
  The hollow path — FU-53's wall fix, FU-55's realized-wall check, FU-47's flow
  connectivity, FU-61's bore guarantee — is **unexercised end to end on the mini corpus**.
  Filed as **FU-62**.

### The 29 failures

| Class | Count | Models |
| --- | ---: | --- |
| `CoverageClosure { NoEligibleRepairRows }` | 10 | `Cat (wild shape - Cat Lord)`, `Deinonychus`, `Morel Mudskipper`, `Myconid Adult`, `Tavey`, `Tavey Base`, `Warhorse`, `zombie-4-a`, `zombie-8-a`, `zombie-9-b` |
| `CoverageClosure { AttemptsExhausted }` | 5 | `Decomposing Zombie`, `Giant Wolf Spider`, `Skeleton_1`, `Zombie Cat`, `zombie-7-a` |
| `CoverageClosure { RepairPlanRefused { Routing, Cco(InvalidGeometry) } }` | 4 | `Giant Toad`, `Skeleton_3`, `Thorny Vegepygmy`, `zombie-5-a` |
| `Failed(Routing)` | 7 | `Awakened Tree`, `Crawler Zombie`, `Giant Toad (swallowing)`, `Morel base`, `Rotting Warhorse`, `Skeleton_2`, `zombie-6-a` |
| `Failed(Brace)` | 3 | `Giant Bat`, `zombie-1-a`, `zombie-2-a` |

### Regression check, per model, against the 21 the baseline names

- 4 now **emit** (above).
- 16 still refuse on coverage, now through the controller instead of the gate.
- **1 moved backwards: `Crawler Zombie`** — 2 blocking defects at the pre-write gate in
  the baseline, `Failed(Routing)` now. It is the **only** model in the 37 that lost ground
  across this batch. Filed as **FU-65**; FU-58's one-word phase is why it cannot be
  attributed further without instrumentation.

**No refusal class present in the baseline grew** other than the stage-failure class,
whose growth is not separable from the 7 models the baseline never dispositioned.

### FU-56's own first action, answered by measurement

FU-56's row prescribed measuring the three cheapest refusals — `Tavey Badger` (1 defect),
`Thorny Vegepygmy` (1), `Myconid Adult` (1) — to decide whether the controller faces a
placement problem or a candidate-pool problem. The run answers it, and the answer is
**both, model by model**:

| Model | Outcome | Attempts | Worst island |
| --- | --- | ---: | --- |
| `Tavey Badger` | **closed and emitted** | — | — |
| `Myconid Adult` | `NoEligibleRepairRows` | **0** | 0.0092 mm², island 8 |
| `Thorny Vegepygmy` | `RepairPlanRefused { Routing, Cco(InvalidGeometry) }` | 2 | 0.4528 mm², island 0 |

`Myconid Adult` refuses at attempt **zero**: the pool offered nothing eligible for its one
0.0092 mm² island. That is a candidate-pool problem. `Thorny Vegepygmy` had rows, appended
them, and the router then refused the densified plan outright. That is a third thing
neither option anticipated, and it is filed as **FU-64**.

### FU-57 evidence completeness, verified by enumeration

All **19** coverage refusals carry a `<stem>.coverage.json` sidecar (schema
`plinth.coverage-refusal.v1`) — 19 of 19, no gaps. **53** island SVGs were written,
between 1 and 5 per model, capped exactly at `COVERAGE_RENDER_MAX_ISLANDS` = 5 for the
seven models with more islands than that. Every CLI card ranks its worst islands
worst-first with uncovered mm² and the worst-gap kind, showing 3 and pointing the
remainder at the sidecar. Nothing was written at any `.chitubox` target path.

**What the rerun does NOT claim.** It does not claim any of the 29 refusals is closable,
that the four newly-emitted minis print well, or that a shell would survive the pipeline
if one were ever produced. It claims what it measured.

---

## 3. Rows that stay open despite landing

**FU-56 and FU-57 are LANDED but the residual is unchanged in size.** 19 of 37 models
still cannot be supported, and the controller's refusals are now the dominant class in
place of the gate's. The rows close the *mechanism* the rulings named; they do not close
the physics.

**FU-38's landing is the batch's one recovered non-compliance.** The adversarial critic
found FU-38 present only as uncommitted working-tree changes that moved the delicacy feed
without re-pinning a single digest, leaving `develop`'s tree red on three tests. It was
completed at `fbbc099` with every digest re-pinned and measured at this tip (both columns
re-measured by building the harness twice, once with the zero feed restored verbatim), and
the section its code comments promised now exists. `develop` is green. **No row in this
batch is marked done on a critic finding that was not cured.**

---

## 4. The FU-61 verdict

**CONFIRMED. Every drain and vent bore fully penetrates, and the overshoot the ruling
hypothesised already exists and is already registered.**

The ruling's premise — that bores are drilled to the nominal wall depth and might need
~0.87 mm of overshoot added — does not describe this engine. `drains.rs` builds every hole
over `[origin − before·dir, origin + (sampled_exit + after)·dir]` with `before`/`after`
drawn from the registered `HOLLOW_EXTRA_DRAIN_POLICY.extensions_mm` ladder
(`before` 4.0 mm, drain `after` 4.0 mm, vent `after` 5.0 mm), `drilling.rs` cuts a
full-radius prism over that whole span in one Boolean, and `sampled_exit` is measured
against **source-solid** occupancy with `validate_frozen_hole_geometry` enforcing
`length_mm <= sampled_exit_mm` — so the far cap can only move further out, never nearer.

**Measured** by an independent harness
(`crates/plinth-pipeline/examples/fu61_bore_penetration.rs`, committed at `44b8df1`,
sharing no code with `prove_centerlines`; production chain, own Möller–Trumbore caster,
45,316 probe rays):

| hole | R (mm) | clear radius | fraction of nominal | outer margin (mm) |
| --- | ---: | ---: | ---: | ---: |
| Sphere drain | 1.50 | 1.4865 | 0.99100 | 4.5001 |
| Sphere vent | 1.25 | 1.23875 | 0.99100 | 4.8351 |
| Cylinder drain | 1.50 | 1.4865 | 0.99100 | 4.5000 |
| Cylinder vent | 1.25 | 1.23875 | 0.99100 | 5.2249 |

Worst outer margin **4.5000 mm** against a voxel body diagonal of 0.86603 mm — **5.20×**
headroom. **No new margin constant is needed**, and none was registered.

**The voxel stepping takes nothing off the opening.** The measured clear radius is
0.99100 of nominal on every hole and the 24-gon cutter's own inradius is
cos(π/24) = 0.9914449; the scan brackets it exactly (clear at 0.991 R, blocked at 0.992 R).
The entire width deficit is the inscribed-polygon cutter, not the lattice.

**The worst case was actually exercised**, not merely argued: 20,563 of 45,316 probes
(45%) — 86% of them on the Sphere drain — started already inside shell material at full
bore offset, i.e. the cavity is simply absent 1.5 mm off-axis there. Every one still
opened at full width.

**Coverage, stated honestly.** 45 models attempted; exactly **2** reached drilling, and
both are corpus fixtures. **Zero real minis reach the drilling stage on `develop` today**
— which the corpus rerun independently confirms (all 8 artifacts are
`Solid { BelowThreshold }`). FU-61's own first action asked for "the Sphere and one
branched mini"; the second half was not satisfiable. Filed as **FU-62**, with the
structural argument (the ladder is unconditional and geometry-independent) recorded as
what carries the verdict beyond the two fixtures.

Renders are exact Rust-generated cross-sections through each bore axis. **Blender was not
used**: its only scripting interface is Python and the no-Python constraint is absolute.
Confirmation of that reading is an open question below.

---

## 5. Every question the lanes filed, consolidated

Thirty-odd questions were filed across four planners, eight implementers, the four
mergers, the FU-61 verifier, the FU-51 re-recorder and the gap-closing pass. They are
deduplicated here and each is either **resolved by what landed** or **carried as an open
owner question**. Nothing was guessed at.

### 5.1 Resolved by what landed

| Question | Resolution |
| --- | --- |
| FU-56's blocking-patch predicate: one implementation, or mirror-plus-equivalence-test? | **One implementation.** `plinth_verify::defects_for_patches` now *calls* `plinth_support::coverage_patch_is_blocking`. The planner's conservative choice was the mirror; the refactor is what landed, with the same registered numbers and no verdict change. Worth the owner's eye because it edits the gate crate the ruling said "does not move one byte" — the bytes that moved are a delegation, not a rule. |
| FU-56: may the controller nominate `SyntheticIsland` repair rows? | **Both kinds allowed.** Nomination is confined to the uncovered patch's own `(island_id, component_id)`, so a synthetic row can only serve a synthetic island's own area. |
| FU-56: typed refusal, or let the gate speak? | **Typed refusal**, the ruling read literally. Consequence: the gate never sees these models (see FU-68). |
| FU-57: render format? | **Deterministic pure-Rust SVG.** No new dependency, no encoder, no Blender, no Python. |
| FU-57: sidecar and render naming? | `<stem>.coverage.json` and `<stem>.coverage.island-<id>.svg`, beside the would-be artifact. Confirmed against 19 sidecars and 53 renders in the corpus rerun. |
| FU-57 vs FU-38: two lanes re-pinning the same golden. | **Sequenced correctly.** FU-56 landed first and renamed `coverage_gate_overhang_table.json` → `coverage_gate_toed_slab.json`; FU-38 re-pinned on top with attribution that **spans the rename** rather than citing the retired file. |
| FU-36: what value for the tightened closure bound? | **`1e-15`**, ~65× over the measured 1.52e-17 residual and three orders tighter than `1e-12`. A tightening, never a widening. |
| FU-36: does "symmetric pole to pole" authorize snapping the equator and mirroring southern latitudes? | **Yes, as landed** — pole-snap alone provably cannot satisfy an exact symmetry invariant. The full 224-triangle multiset is deliberately *not* asserted mirror-closed (band diagonals are a pre-existing tessellation property), and two overclaims in the record were **retracted** rather than reworded. |
| FU-36: A/B scope and run-output location. | Both sides, all eight models including the treant leg; artifacts and hashes copied to scratch and `exports/` left clean. |
| FU-47: what is the new refusal called? | `HollowError::IneligibleCavityBelowDrainThroat`, classified as a **solid-reserve** outcome — never a hard refusal, the model still prints. |
| FU-37: does `wall` carry the requested or the realized figure? | **Two fields, and both reach the run's own output.** `wall` (requested, FU-37) and `realized_wall_mm` (FU-55) live side by side on the top-level `HollowReport`, and a review finding then caught that both died inside `seal_and_emit`; `ChainOutcome` and `ModelPreparationOutcome::Artifact` now carry them and the CLI prints both. |
| FU-37: which goldens re-pin together as one change? | **Vacuously satisfied and verified.** Nothing on disk serializes a `HollowReport`; the affected-golden set was empty, confirmed empirically rather than by inspection. |
| FU-50: delete `RasterSubdivisionLimitExceeded`? | **Deleted.** The variant is unreachable once the recursion is gone; removing it loosens nothing, because the cap that binds (`max_raster_work_items`) is untouched. |
| FU-50: are the two flat bases fixed end to end? | **Yes** — `Round 50` and `Round 100` emit artifacts in the corpus rerun. |
| FU-51: are the FU-38 WIP files in the shared tree finished? | **Yes, at `fbbc099`.** They were mid-edit when FU-51 landed and were left untouched then, correctly. |
| FU-51/61: is the bore harness committed, and by whom? | **Committed at `44b8df1`** as `crates/plinth-pipeline/examples/fu61_bore_penetration.rs`, following the `repair_probe` precedent for a permanent diagnostic example. The journal cites its exact output, so it had to be in the tree. |
| FU-51: are the Sphere baseline's `wall_mm` fields really walls? | **No — they are sampled exit distances**, and the re-record says so. The Python-era record's `drains[0].wall_mm = 1.0` / `vent.wall_mm = 4.0` carry the same quantity Rust's `HoleGeometry::sampled_exit_mm` carries. The naming question is still open below. |
| The intermittent 16-thread `plinth-support` failure. | **Already filed** — this is [FU-27](../CURRENT-STATUS.md#follow-ups-2026-08-04), sighted again on 2026-08-12. No new row; the sighting is appended to FU-27. |
| FU-61's blocker: why do no minis reach drilling? | Two causes, both measured: `HOLLOW_SAVE_MIN_FRAC` = 0.25 sends thin-limbed minis solid at the registered wall, and at thinner walls the drain planner's own preflight refuses. The second is **not a new row** — it is [FU-59](../CURRENT-STATUS.md#follow-ups-2026-08-04)'s over-charge seen at a second site (`planner_preflight`'s `lobe_capacity = low_members / minimum_lobe_voxels`, a partition bound that ignores cavity connectivity), and FU-59's own numbers (Warhorse 4.06 GB, Giant Toad 8.92 GB) are the same refusal FU-61 re-measured at 4.20 GB and 9.12 GB under stress walls. Recorded against FU-59; **the cap was not touched.** |

### 5.2 Open owner questions

Numbered for reference; each is also carried in the affected ledger row.

1. **FU-56's exit-code and disposition projection.** A coverage-closure refusal lands as
   `ModelPreparationOutcome::Failed` with `PipelineExitCode::Supports` (5), not as
   `Refused`. The planner's conservative choice was to project it to
   `Refused { gate: None }` at the same exit code precisely so the residual stayed one
   class; what landed is the smaller-diff option. **Measured consequence:** the run
   summary now reads `gate-refused=0` where the baseline read 21, so the two runs'
   summary lines are not directly comparable and any downstream reader keying on
   `gate-refused` sees the class vanish. No ruling covers this.
2. **FU-47's throat rule: radius or diameter?** Implemented as passage radius ≥
   `HOLLOW_DRAIN_R_MM` = 1.5 mm — i.e. the passage admits the 3.0 mm bore — per the
   recorded ruling's wording. The spawn brief said "3.0 mm". Under the stricter reading
   every cavity splits far more aggressively and savings fall.
3. **The realized bore is 0.9914449 of nominal.** The 24-gon cutter is *inscribed*, so a
   registered 3.0 mm drain opens 2.9743 mm and a 2.5 mm vent opens 2.4786 mm. FU-47's
   throat doctrine cites the 3.0 mm bore as the minimum-throat rule, which the realized
   throat is 0.026 mm under. Circumscribing the cutter would fix it — and is exactly the
   widening FU-61 forbids. Not changed. Owner call.
4. **FU-54's registration route.** The pass limit of 8 landed as a private const plus a
   caller-owned `H0AdmissionLimits` field, matching every sibling H0 admission limit —
   **none** of which is a registered M0 constant. The ruling said "registered with a
   proper provenance class like every other admission limit", which is satisfied by the
   sibling pattern but not by `docs/development.md`'s "resource caps are registered too".
   The planner recommended the registry route; the sibling route is what landed.
5. **FU-53's marching margin could be smaller.** `HOLLOW_CAVITY_MARCH_MARGIN_MM` = 0.866 mm
   (one voxel body diagonal) is derived and provably safe, and it costs real cavity volume
   on every model — enough to push some under the savings gate into a solid decision. A
   worst-case construction suggests ~0.44 mm may suffice but could not be proved airtight.
   Trading proof for savings is a ruling, not an implementer's call.
6. **FU-50's measured headroom is far below the ruling's expectation.** The ruling
   asserted "roughly three orders of magnitude of headroom"; the landed rasterizer
   measures the two flat bases at **~2.5% of the unchanged cap**, i.e. ~40×. Recorded as
   measured. The counting rule was **not** adjusted to reach the ruling's figure — that
   would be the redefinition the ruling forbids.
7. **FU-57's four evidence bounds are the implementation's, not the owner's.**
   `COVERAGE_RENDER_MAX_FACES` = 20,000, `COVERAGE_RENDER_MAX_ISLANDS` = 5,
   `MAX_HIGHLIGHT_FACES_PER_ISLAND` = 5,000, `COVERAGE_CARD_MAX_ISLANDS_SHOWN` = 3. All
   are presentation bounds in the pipeline/CLI crates and none is in the M0 registry. The
   corpus rerun exercised all four (7 models hit the island cap; every mini over 20,000
   faces rendered as silhouette-plus-highlight). Owner may want them registered or ruled.
8. **FU-57's evidence rides on both `Refused` and `Failed`.** The planner assumed both
   refusal sources would be `Refused`; ground truth is that the closure refusal is
   `Failed`, so the evidence field was added to **both** variants under the ruling's plain
   text. Interpretation, not ruling.
9. **FU-57 recovers island face ids by walking the serialized report** rather than through
   new `CoverageReport` accessors, so `plinth-verify/src` is untouched. Uglier than the
   accessor; reversible by ruling.
10. **FU-57's renders omit routed contact points**, so a render shows *where* the gap is
    but not *what the router did near it*. The plan anticipated this trim.
11. **`RenderMesh::from_mesh` clones the prepared mesh on every `seal_and_emit`**, success
    path included, because deferring it needs a signature `run_support` had just settled.
    A real per-model cost, stated rather than hidden.
12. **`H0ResourceDiagnostics` was not extended** with FU-47's flow-selection transient-byte
    fields (EDT array, i32 label array), deliberately, to avoid a committed-golden blast
    radius colliding with FU-54/FU-55's concurrent edits to the same code.
13. **The drilling-side floods remain plain 6-connectivity.** `drainability_map_admitted`
    and `trapped_components` are literally "can resin flow from here to a drain", so the
    most physically complete reading of FU-47's doctrine applies the throat rule there
    too. Out of the ruling's named scope; not applied either way.
14. **Multi-cavity drainage is unbuilt.** FU-47's ruling says separated interiors are
    separate cavities each needing its own drainage, but the engine cuts exactly one
    (`cavity_id` 1 is hardcoded; `validate_shell_topology` requires one cavity component).
    The interim: cut the largest flow component, leave the rest solid — never a trap,
    savings left on the table. Genuine multi-cavity hollowing is an architectural change
    that wants its own row and its own ruling.
15. **`REPAIR_PINCH_SPLIT_MAX_MM` stays registered and is now read by nothing.** Retiring
    it means coordinated edits to the hand-maintained 303-name inventory, `constants.md`
    and the source ledger. The registry has no retirement mechanism; inventing one is not
    an implementer's call.
16. **FU-47's lattice comparison uses `clearance >= throat`** per the ruling's wording,
    while pre-existing cavity selection uses strict `>` (`edt > wall`). On a 0.5 mm lattice
    `>= 1.5` is exactly attainable, so the choice is not measure-zero. Unreconciled; no
    margin was invented either way.
17. **FU-36 did not snap the longitude quadrants.** `cos(π/2)` = 6.12e-17 leaves the
    longitude ring slightly off-axis. Not needed for manifoldness or N/S symmetry and not
    mentioned by the ruling, so deliberately not done — but a reader of "properly manifold
    object" might expect the same exactness in longitude.
18. **FU-38's ruling asked for two J4 digests; only one existed to re-pin.**
    `make_raking_blade`'s `74030269957898085` was retired by FU-56's landing, because a
    refusal produces no sealed authority. The conservative path taken: measure the blade
    under the real field, prove the refusal **byte-identical** under both fields, and
    record that — rather than manufacture a digest or revive a retired pin. Flagged
    because the ruling's literal instruction could not be executed as written.
19. **`make_overhang_table`'s census movement has no attribution.** At `develop` HEAD under
    the *zero* field its OD-8 digest is `11652461737312715963` with 74 routed contacts, not
    the J4-recorded `16396858890798947253` / 73. FU-56 moved it and never re-pinned the
    census record; FU-38 correctly recorded the current value as its own "from" rather than
    charging another lane's movement to itself. The FU-56-side attribution does not exist
    anywhere. Owner may want it recorded.
20. **FU-53's `the_zero_field_divergence_is_measured_per_committed_fixture` was edited by a
    different lane.** The coverage lane flipped two recorded verdicts `false → true`
    (`Sphere.prepped.stl` and `make_overhang_table` now produce byte-identical outcomes
    under measured and zero fields, both refusing identically). Applied with full
    attribution under the file's own re-pin protocol rather than left red — but it is
    FU-38's row and the owner should look at it explicitly.
21. **FU-50's `raster_delta.rs` "approx min wall" column is a local proxy**, not FU-55's
    landed measurement primitive, because FU-55 was a parallel lane and parity could not be
    confirmed at implementation time. Labelled as an approximation in code and prose.
22. **The Sphere baseline's re-record deliberately left the retired-Python-era fields
    untouched** (`solid_ml`, `final_ml`, `decision`, fingerprints, transform — no Rust
    reader exists for any of them). Reproducing them would require re-running the retired
    Python pipeline. A recorded deviation from a literal whole-file regeneration; and the
    record should probably say **"sampled exit"** where it says `wall_mm`.
23. **Is Blender exempt from "no Python anywhere"?** Blender's only scripting interface is
    `bpy`. FU-61 produced exact Rust-generated SVG cross-sections instead of Blender
    renders on the strict reading. If Blender scripts count as external-tool glue rather
    than project Python, shaded views can be added.

---

## 6. New rows filed by this record

Eight, at the next free numbers, under the recorded collision protocol. **No existing row
was renumbered, edited or displaced.** Highest occupied number before this record: FU-61.

| # | Row |
| --- | --- |
| **FU-62** | Zero of 37 real minis reaches the drilling stage — the whole hollow path is proved on corpus fixtures only |
| **FU-63** | `Weeping Treant v1.2` no longer seals in the OD-8 census: `Failed(Preparation)` where it sealed at FU-32 J6 |
| **FU-64** | The coverage-closure controller's own repair plan can be refused by the router: `Cco(InvalidGeometry)` on 4 of 37 minis |
| **FU-65** | `Crawler Zombie` regressed from a pre-write-gate refusal to `Failed(Routing)` — the batch's only backwards model |
| **FU-66** | The drain/vent keepout extension ladder is restated as bare literals in `drilling.rs` and cross-asserted nowhere |
| **FU-67** | `PrepareReport::exit_code()` masks a coverage-closure (`Supports`, 5) refusal behind an earlier `Input` failure (1) |
| **FU-68** | `Refused { gate: UncoveredIslands }` is now structurally unreachable in production |
| **FU-69** | Repo-wide rustfmt drift, and no decision on which rustfmt is canonical |

Each carries its full evidence in the ledger row rather than here.

---

## 7. Quality gate at this record's commit

`cargo test --workspace` green across every crate including doc-tests;
`cargo clippy --workspace --all-targets` zero warnings; working tree clean.
Nothing pushed. `minis-to-process/` and `corpus/models/` untouched. Every run output
from the corpus rerun and the FU-61 harness lives in a scratch directory outside the
repository and is not committed.
