# Decision — the "23 open owner questions" reconciled: 13 were already answered, 10 are ruled here, 1 goes to Craig

**Companion to [`2026-08-12-ruled-batch-landed.md`](2026-08-12-ruled-batch-landed.md)
(whose §5.2 is the input list) and to
[`2026-08-12-drainage-doctrine-and-orchestrator-rulings.md`](2026-08-12-drainage-doctrine-and-orchestrator-rulings.md)
(whose Part A and Part B between them answered thirteen of them — eleven in Part B, and
questions 13 and 14 by the owner in Part A — before this pass began).**

**The headline.** §5.2 of the batch record was written at `5cca1e5` and lists twenty-three
open owner questions. Reconciled against the rulings recorded at `87f07c3` and the code at
`9e5dbb0`:

- **13 were already answered** — questions 1, 2, 4, 5, 6, 7, 8, 9, 13, 14, 15, 16 and 17. The
  record was stale, not the question open. Seven of the thirteen already have a ledger row
  carrying the work (six distinct rows).
- **10 are ruled here** — questions 3, 10, 11, 12, 18, 19, 20, 21, 22 and 23. **One by the
  owner** (question 3, the inscribed bore, §2 below); **nine as orchestrator rulings under
  co-owner authority**, per Craig, 2026-08-12.
- **1 genuine owner decision** was extracted from the residue of questions 2, 3 and 16 and
  is **open, with Craig** (§6).

13 + 10 = 23, with no question counted twice. **Two of the thirteen carried a residue** —
question 8's dead field and question 16's wrong justification — and those residues are ruled
here too (§3.1, §3.5); they add nothing to the count because the questions themselves were
already answered.

**What this record is, and is not.** It is a record. The commit that lands it touches
documentation only: **no gate, cap, threshold, bound, floor, tolerance, constant, test or
registry entry moves in it.** Where it names new work it files a new ledger row at the next
free number (FU-83 … FU-85) and **renumbers nothing**. Where it corrects an earlier record it
**annotates** — the recorded text stands as written, with the correction beside it, because
this project's convention is to say what was wrong and why rather than to quietly restate.

**Every factual claim below was checked against the code or the file at `9e5dbb0`**, not
carried over from the analysis that produced the reconciliation. **Two claims from that
analysis did not survive the check and are recorded as corrections rather than repeated as
findings:** the framing of question 20 is stale at HEAD (§4.4), and the stale baseline figures
it placed in §5.2 question 22 are in fact in §5.1 (§4.5). Everything else verified, including
the `flow.rs` threshold arithmetic, the corpus baseline field values, and that FU-82 was the
highest occupied row number.

---

## 1. The 13 that were already answered

The record was never reconciled after the drainage-doctrine pass landed at `87f07c3`. Each
row below names the ruling that answered it; **"filed"** means the work already has a ledger
row and this reconciliation adds nothing to it.

| §5.2 Q | Answered by | Note |
| --- | --- | --- |
| 1 | Part B §B.1 #2 — `Refused { gate: Some(UncoveredIslands) }`, exit 3 | filed [FU-76](../CURRENT-STATUS.md#follow-ups-2026-08-04); the work is open, the question is not |
| 2 | Part B §B.3 #8 — radius, not diameter | **PARTIAL.** The premise is wrong (§4.2); the residue is the escalated decision in §6 |
| 4 | Part B §B.5 #20 — M0 Placeholder + private const mirror + equality test | filed [FU-78](../CURRENT-STATUS.md#follow-ups-2026-08-04) |
| 5 | Part B §B.5 #21 — keep 0.866 mm; only a proof may tighten it | filed [FU-75](../CURRENT-STATUS.md#follow-ups-2026-08-04) |
| 6 | Part B §B.5 #19 — record the measured figure; never adjust the counting rule to reach a claimed one | |
| 7 | Part B §B.2 #7 — presentation bounds are not M0 physics constants | |
| 8 | Part B §B.1 #3 — the refusal class is unified, so one evidence schema covers both paths | **PARTIAL.** The dead-field residue is ruled in §3.1 |
| 9 | Part B §B.2 #6 — add the read-only `sorted_face_ids` accessor | filed [FU-77](../CURRENT-STATUS.md#follow-ups-2026-08-04) |
| 13 | **OWNER**, Part A §A.1(a) — the drilling-side floods obey the throat rule too | filed [FU-70](../CURRENT-STATUS.md#follow-ups-2026-08-04) |
| 14 | **OWNER**, Part A §A.6 — multi-cavity drainage is allowed and encouraged | filed [FU-70](../CURRENT-STATUS.md#follow-ups-2026-08-04) |
| 15 | Part B §B.3 #11 — leave it registered; build a retirement mechanism | filed [FU-74](../CURRENT-STATUS.md#follow-ups-2026-08-04) |
| 16 | Part B §B.3 #9 — `>=` **STANDS** | the ruling holds; its stated reason is wrong and is corrected in §4.3 |
| 17 | Part B §B.4 #15 — longitude snapping is not needed | |

---

## 2. OWNER RULING — the inscribed bore stands; the record gets corrected

**Ruled 2026-08-12 by Craig (owner)**, verbatim:

> *"The difference between 2.974mm and 3.000mm is 'it doesn't fucking matter'. So leave it as
> is and update the docs/constants/recorded test values/rulings/etc as needed."*

**Answers** §5.2 question 3.

### The question

The drill cutter is a 24-sided polygon **inscribed** in the nominal circle
(`CYLINDER_SECTIONS = 24` in `crates/plinth-hollow/src/drilling.rs`, vertices placed at
`radius` on `TAU * section / 24`), so its inradius is `cos(π/24) = 0.9914448614` of nominal.
A registered 3.0 mm drain therefore opens **2.974335 mm** and a 2.5 mm vent opens
**2.478612 mm**. FU-47's throat doctrine cites "the 3.0 mm bore" as the minimum-throat rule,
so the doctrine's stated figure and the realized geometry disagree by 0.026 mm.
Circumscribing the cutter would close the gap — and is exactly the widening
[FU-61](../CURRENT-STATUS.md#follow-ups-2026-08-04) forbids.

### The ruling

**The geometry does not change. The record does.**

1. The cutter stays **inscribed**. No circumscription, no widening, no compensation factor
   applied to the cut. FU-61's prohibition on widening the bore is untouched and remains in
   force.
2. **No threshold, cap, gate, floor or tolerance moves** — in either direction. In particular
   `HOLLOW_DRAIN_R_MM` (1.5 mm) and `HOLLOW_VENT_R_MM` (1.25 mm) keep their registered
   values, and no check is relaxed to "match" the realized figure. **This ruling is
   explicitly NOT an FU-8b event:** nothing is being loosened to make a measurement fit.
3. Every place in the line of record that asserts a **nominal** figure where the **realized**
   figure is what the geometry produces is corrected to state the realized figure, with the
   0.9914449 inscription factor named as the reason. Documentation, constant provenance
   notes, recorded test values and the doctrine text all say what is true.
4. The inscription factor becomes a **recorded fact** rather than folklore rediscovered by
   each lane. Adding that record is additive and permitted.

### Rationale

0.026 mm is roughly a quarter of a layer height and an order of magnitude below the
dimensional noise of resin shrinkage, exposure bleed and film release on this printer. It
cannot be the difference between a passage that drains and one that does not. The defect was
never physical — it was that two parts of the record stated different numbers for the same
geometry, which is a credibility problem for everything else the record asserts.

### Standing consequence

**A doctrine sentence that cites a bore diameter is citing the *realized* bore from here on.**
If a future lane needs the nominal figure it must say "nominal" explicitly.

### What the owner's ruling exposed, verified in the code

The batch record's framing of this question is **inverted**, and correcting it is part of
carrying out clause 3. See §4.1.

---

## 3. The orchestrator rulings

**Nine of them dispose of questions from the ten** — 10, 11, 12, 18, 19, 20, 21, 22 and 23
(question 3, the tenth, was ruled by the owner in §2). **Two more dispose of residues** left by
questions 8 and 16, which are counted among the thirteen already answered and are recorded here
because their answers were partial.

Each is an **orchestrator ruling (co-owner authority, per Craig 2026-08-12)**, recorded with
the same rigour as an owner ruling and distinguishable from one on sight. The escalation
boundary the authority came with still binds: physical-doctrine expansions,
reliability-vs-savings tradeoffs, new-scope spend and anything cap-adjacent go to Craig. §6 is
what hit that boundary on this pass.

### 3.1 Q8 — the dead `evidence` field on the `Failed` variant

**RULED (orchestrator ruling, co-owner authority, per Craig 2026-08-12): delete it in the
same commit as [FU-76](../CURRENT-STATUS.md#follow-ups-2026-08-04), not as its own row.**

FU-57 added the evidence field to **both** `Refused` and `Failed` because ground truth at
implementation time was that the closure refusal arrives as `Failed`. Once FU-76 projects the
closure refusal to `Refused { gate: Some(UncoveredIslands) }`, the `Failed`-side `evidence`
field is **unreachable**. A field that advertises evidence a variant can no longer carry is
precisely the misleading surface FU-58 condemns. **One commit, not two** — a window in which
the field exists and cannot be populated is a window in which the record lies.

### 3.2 Q10 — renders omit routed contact points

**STANDS (orchestrator ruling, co-owner authority, per Craig 2026-08-12).** The trim is
documented in code and was anticipated by the plan. The render answers *"where is the gap and
how big"*; the sidecar answers the rest. **Revisit only** if a human reading the renders
reports they cannot separate a real overhang from a modelling artifact — which is the job the
owner's FU-57 ruling actually assigned them. No row.

### 3.3 Q11 — `RenderMesh::from_mesh` clones on every emit

**STANDS, pending measurement (orchestrator ruling, co-owner authority, per Craig
2026-08-12).** Append the measured bytes and milliseconds for the largest corpus model to the
[FU-57](../CURRENT-STATUS.md#follow-ups-2026-08-04) row, then decide. Refactoring costs a
`run_support` signature change that was deliberately settled during the batch; that is not
spent on an unmeasured cost. **Measure first** is the whole ruling.

### 3.4 Q12 — `H0ResourceDiagnostics` was not extended with the flow-selection fields

**RULED (orchestrator ruling, co-owner authority, per Craig 2026-08-12): extend it, and treat
it as a prerequisite rather than a nicety.** Filed as **FU-83**.

The blast-radius argument that justified deferring it — a committed-golden collision with
FU-54/FU-55's concurrent edits to the same code — **expired when those lanes merged**.

The decisive new reason is not tidiness. **[FU-82](../CURRENT-STATUS.md#follow-ups-2026-08-04)'s
own first action — counting how many real cavity throats fall inside the pose-ambiguity band —
cannot be produced by a pipeline that computes `FlowStats` and drops it.** Verified at
`9e5dbb0`: `flow::select_largest_flow_component` returns `(selected, FlowStats)` at every call
site, and both production sites discard the second element — `Some((selected, _stats))` in
`crates/plinth-hollow/src/lib.rs`'s `create_shell` path, and `_flow_stats` in
`crates/plinth-hollow/src/well_composed.rs`'s re-selection loop. `H0ResourceDiagnostics`
carries **sixteen** fields and not one of them is a flow-selection field. Every read of
`flow_components`, `core_voxels` or `unreached_voxels` anywhere in the workspace is inside
`plinth-hollow`'s own `#[cfg(test)]` modules. **Schedule FU-83 AHEAD of FU-82's census.**

### 3.5 Q16 — `>=` vs `>` at the throat comparison

**The ruling STANDS (`>=`); its reason is corrected.** See §4.3 for the correction. Nothing
in the code changes: `clearance >= core_threshold_mm` is what ships and what the ruling
mandates. This entry exists so that the next reader does not rediscover the wrong
justification and act on it.

### 3.6 Q18 — FU-38's missing second J4 digest

**STANDS (orchestrator ruling, co-owner authority, per Craig 2026-08-12).**

The FU-38 ruling asked for two J4 digests to be re-pinned; only one existed to re-pin, because
`make_raking_blade`'s `74030269957898085` was retired by FU-56's landing — a refusal produces
no sealed authority. The lane measured the blade under the real field and proved the refusal
**byte-identical under both fields** instead. That is **strictly better evidence** than
manufacturing a digest or reviving a retired pin, and **a literal instruction overtaken by
events is not a ruling violated**. **Add one annotation to the FU-38 record naming what was
substituted and why** — part of the record-keeping pass filed as **FU-84**.

### 3.7 Q19 — `make_overhang_table`'s unattributed census movement

**RULED (orchestrator ruling, co-owner authority, per Craig 2026-08-12): record it.** Part of
**FU-84**, and written into the [FU-56](../CURRENT-STATUS.md#follow-ups-2026-08-04) row.

FU-38 **correctly** refused to charge another lane's movement to itself — that is the
attribution discipline working. But the consequence is that the movement is charged to
**nobody**. Verified at `9e5dbb0`: the FU-38 record's zero-field column reads
`make_overhang_table` = `11652461737312715963` with **74** routed contacts
([the FU-38 section](../CURRENT-STATUS.md#fu38-real-delicacy-repin-2026-08-12)), while the
J4-recorded value still quoted in `crates/plinth-support/tests/penetration_census_equivalence.rs`
and `crates/plinth-verify/tests/coverage_gate.rs` is `16396858890798947253` with **73**.

The movement is therefore:

> `make_overhang_table` OD-8 census digest, zero field:
> `16396858890798947253` (73 routed contacts) → `11652461737312715963` (74 routed contacts),
> charged to **FU-56's coverage-closure controller** — commits `1d407a0` (the controller) and
> `6ddf051` (the review fixes that made the repairs actually close coverage), merged to
> `develop` in `b9decd5`.

It costs one sentence and it is the only thing between this project and a digest chain of
custody with a hole in it. **This re-pins nothing** — the J4 comments cited above are
model-identifier prose, not assertions.

### 3.8 Q20 — FU-53's zero-field divergence test was edited by a different lane

**RULED (orchestrator ruling, co-owner authority, per Craig 2026-08-12): correct as done;
close it.** Part of **FU-84**.

A cross-lane verdict flip applied with **full attribution under the file's own re-pin
protocol** is the protocol working, not a breach of it. The test
(`the_zero_field_divergence_is_measured_per_committed_fixture`,
`crates/plinth-pipeline/src/lib.rs`) is designed to be re-recorded with a `from → to`
attribution rather than to be frozen, and every flip in it carries one.

**The substantive sub-question as §5.2 framed it is moot at `9e5dbb0`, and the framing is
itself stale — see §4.4.** All three recorded verdicts read `false` at HEAD. The question
"is it acceptable that two fixtures now refuse *identically* under both fields?" describes a
state that the FU-56 review fixes already reverted.

### 3.9 Q21 — `raster_delta.rs`'s "approx min wall" proxy

**STANDS (orchestrator ruling, co-owner authority, per Craig 2026-08-12).** A labelled
diagnostic, in an example, on no decision path. **Optional cheap follow-on** now that FU-55's
realized-wall primitive exists: have the example call it, so the tree holds one definition of
"wall" instead of two. **Low priority; no row.**

### 3.10 Q22 — the Sphere baseline's field naming

**RULED (orchestrator ruling, co-owner authority, per Craig 2026-08-12): do not re-cut the
file; add a reader's note.** Filed as **FU-85**.

Regenerating `corpus/baseline/Sphere.pipeline.json` to rename a field would violate the
owner's FU-51 ruling that this baseline gets **one** deliberate recorded regeneration — which
it already had, at `44b8df1`.

A finding that strengthens the case for the note, verified against the file at `9e5dbb0`:
`drains[0].wall_mm` reads **4.0** and `vent.wall_mm` reads **5.0**, which are **exactly the
registered `after` keepout-extension values** the FU-61 verdict names (`before` 4.0 mm, drain
`after` 4.0 mm, vent `after` 5.0 mm; `HoleRole::Drain => 4.0` / `HoleRole::Vent => 5.0` in
`crates/plinth-hollow/src/drilling.rs`). So a reader **cannot tell** whether the field means a
sampled exit distance, a wall, or a keepout extension — three different quantities that
coincide numerically on this one file. **Name each surviving field in the corpus
documentation and say which have no Rust reader.**

### 3.11 Q23 — is Blender exempt from "no Python anywhere"?

**RULED (orchestrator ruling, co-owner authority, per Craig 2026-08-12): no exemption, and
none is needed.**

Part B §B.2 #5 already settled the live case in favour of deterministic Rust-generated SVG.
Blender buys shaded views nobody has requested, at the price of an external toolchain,
non-deterministic output bytes and a Python surface — against a standing decision that no new
Python enters this project. If a future need arises that deterministic SVG genuinely cannot
serve, that is a one-line owner question **at that time, on its own merits**.

**Scope note:** this governs **project** output — pipeline artifacts, evidence renders,
committed figures. It does not govern throwaway explanatory pictures drawn for the owner
outside the repository.

---

## 4. Corrections the record needs

Stated as corrections. The recorded text stands as written in its own file, annotated in
place; nothing is silently restated.

### 4.1 §5.2 Q3's "0.026 mm under" is INVERTED

**What the record says.** That the realized 2.9743 mm throat is "0.026 mm under the number
FU-47's throat doctrine cites".

**Why it is wrong.** **The engine never compares against 3.0 mm.** Verified at `9e5dbb0`:

```
core_clearance_threshold_mm(throat_r_mm, pitch_mm) = throat_r_mm + 0.5 * pitch_mm
                                                     (crates/plinth-hollow/src/flow.rs:147-149)
```

applied as `clearance >= core_threshold_mm` in `flow_labels`. At the registered values —
`HOLLOW_DRAIN_R_MM` = 1.5 mm, `HOLLOW_PITCH_MM` = 0.5 mm — that is a **1.75 mm clearance
radius**, i.e. the engine admits a passage only if it is at least **3.5 mm** wide.

Against a **2.974335 mm** realized bore, 3.5 mm is **0.53 mm of margin on the SAFE side**
(3.5 − 2.974335 = 0.525665), not a deficit. The engine demands *more* room than the drill
actually needs. Nothing is "under" anything.

**The correction changes no code and no number.** It changes what the record claims about the
direction of an error, which is the difference between a reader reaching for a widening and a
reader leaving it alone.

### 4.2 §5.2 Q2 and Part B §B.3 #8 both quote 1.5 mm as the shipped threshold

**What they say.** §5.2 Q2: *"Implemented as passage radius ≥ `HOLLOW_DRAIN_R_MM` = 1.5 mm."*
Part B ruling #8: *"Throat RADIUS >= `HOLLOW_DRAIN_R_MM` (1.5 mm) = the passage admits the
3.0 mm bore."*

**Why it is wrong.** **The shipped threshold is 1.75 mm, not 1.5 mm.** The half-voxel
correction (`throat_r_mm + pitch/2`) is not an implementation detail sitting beside the rule —
it *is* the comparison. It was an adversarial-review tightening, it is **correct in itself**,
it moves no registered constant, and it is documented both in `flow.rs`'s own module and
function docs and in the FU-47 ledger row.

**The point of the correction is the process, not the arithmetic.** **The orchestrator ruled
on a description of the code that was already out of date.** Ruling #8's *verdict* — radius,
not diameter — is unaffected and stands. But a ruling recorded against a stale description is
a ruling a later reader will apply to the wrong number. The residue is what §6 escalates.

Corroboration in the existing record, written independently and correct:
[the dual-cavity fixture section](../CURRENT-STATUS.md#dual-cavity-fixture-2026-08-12) states
the 1.75 mm threshold explicitly and builds its AT_BORE setting to be **refused** by it, and
[FU-82](../CURRENT-STATUS.md#follow-ups-2026-08-04) reasons throughout in 1.75 mm.

### 4.3 §5.2 Q16's justification is BACKWARDS

**What the record says.** *"On a 0.5 mm lattice `>= 1.5` is exactly attainable, so the choice
is not measure-zero."*

**Why it is wrong.** At the real threshold of **1.75 mm** the boundary is lattice-**UN**attainable.
`frozen_distance_transform` is an exact Euclidean distance transform over an isotropic 0.5 mm
lattice, so every clearance it can report has the form `0.5·√n` for a non-negative integer
`n` that is a sum of three integer squares. Exact equality with 1.75 requires
`1.75 = 0.5·√12.25`, i.e. `n = 12.25` — **not an integer at all**, so trivially not a sum of
three integer squares. **At production values the `>=` / `>` choice is measure-zero.**

The old reasoning is right about 1.5 (= `0.5·√9`, attainable — which is exactly why the
in-crate boundary test at `flow.rs`'s
`the_core_test_admits_clearance_exactly_equal_to_the_corrected_threshold` has to pick a
non-registered throat of 1.25 mm to land the threshold on 1.5 and hit the boundary at all).
It is wrong about the threshold the engine actually applies.

**The ruling (`>=`) still stands; only the reason changes.** `>=` costs nothing either way,
and it is what the FU-47 doctrine's wording specifies.

### 4.4 §5.2 Q20's premise is stale at HEAD — CORRECTION FOUND ON VERIFICATION

**What the record says.** That the coverage lane flipped two recorded verdicts `false → true`
in `the_zero_field_divergence_is_measured_per_committed_fixture`, so that
`Sphere.prepped.stl` and `make_overhang_table` "now produce byte-identical outcomes under
measured and zero fields, both refusing identically".

**Why it is wrong.** Verified in `crates/plinth-pipeline/src/lib.rs` at `9e5dbb0`: **all three
recorded verdicts read `false`.**

| fixture | recorded verdict at `9e5dbb0` |
| --- | --- |
| `Sphere.prepped.stl` | `false` |
| `make_toed_slab.prepped.stl` | `false` |
| `make_overhang_table (procedural)` | `false` |

The `false → true` flips §5.2 describes were **reverted `true → false` by the FU-56 review
fixes**, in the same file, with their own attribution — a **fourth** re-record on the Sphere
and a **second** on the overhang table. Neither fixture refuses identically any more: the
Sphere's fifth routing attempt still ends in `Cco(InvalidGeometry)` under both fields but the
per-island evidence diverges (measured field: island 1 ends with 477 nominated repair rows
and 26 eligible; zero field: 478 and 27), and the overhang table now **closes and seals under
both fields**, producing two sealed digests that differ.

**Consequence for the ruling.** §3.8's verdict is unaffected and is in fact strengthened: the
protocol re-recorded these verdicts **six times across three fixtures**, every time with a
`from → to` attribution, and each flip was measured rather than assumed. What is withdrawn is
the *supporting argument* — "identical refusal is the strongest available form of neutrality"
— because at HEAD there is no identical refusal to argue from.

### 4.5 §5.1 (not §5.2) quotes pre-FU-51 baseline values

**What the record says.** The §5.1 row *"FU-51: are the Sphere baseline's `wall_mm` fields
really walls?"* quotes *"The Python-era record's `drains[0].wall_mm = 1.0` / `vent.wall_mm =
4.0`"*.

**Why it is wrong.** Verified against `corpus/baseline/Sphere.pipeline.json` at `9e5dbb0`:
`drains[0].wall_mm` = **4.0** and `vent.wall_mm` = **5.0**. The quoted pair is the
pre-FU-51 state; the file was deliberately re-recorded once at `44b8df1` under the owner's
FU-51 ruling and those are not its values any more.

**A location correction too:** the reconciliation input placed this quote in §5.2 question 22.
It is not there — §5.2 Q22 raises only the *naming* question. The stale figures are in the
**§5.1** table row, and that is where the annotation goes.

The substance of §5.1's answer is untouched and remains right: these fields are **sampled exit
distances**, not walls, carrying the quantity `HoleGeometry::sampled_exit_mm` carries.

### 4.6 `CURRENT-STATUS.md`'s dual-cavity section says the work is not on `develop`

**What it says**, in bold, at the head of
[the dual-cavity fixture section](../CURRENT-STATUS.md#dual-cavity-fixture-2026-08-12):
*"this work is on `feature/dual-cavity-fixture` and is NOT on `develop`"*, and in the State of
the System summary: *"on a branch"*.

**Why it is wrong.** At `9e5dbb0` **`develop` IS that work.** `9e5dbb0` is the merge commit
(parents `87f07c3` and `0fe4c65`), and the feature branch and its worktree are both gone —
the ref store holds only `develop` and `main`.

This is the correction with the shortest fuse of the six: left standing, the next reader draws
the wrong conclusion about what ships.

---

## 5. Dependencies — do not work these apart

| Coupling | Why |
| --- | --- |
| **Q2 + Q3 + Q16 are ONE question** | They are three faces of *"what does 'wide enough for resin to flow' mean in the shipped engine"*. Escalated to the owner as **one** decision, not three — §6. |
| **Q1 + Q8 are ONE commit** | [FU-76](../CURRENT-STATUS.md#follow-ups-2026-08-04) removes the `Failed` path; the dead `evidence` field dies with it. Two commits leave a window in which the record lies. |
| **FU-76 + [FU-79](../CURRENT-STATUS.md#follow-ups-2026-08-04) are ONE design** | Both are about a disposition naming the stage that actually decided. Fixing the projection while `BelowThreshold` stays a catch-all just moves the lie. |
| **FU-83 GATES [FU-82](../CURRENT-STATUS.md#follow-ups-2026-08-04)'s census** | The census cannot be produced from a pipeline that computes `FlowStats` and drops it. **Build FU-83 first.** |
| **Q13 + Q14 are ONE build** | The two halves of [FU-70](../CURRENT-STATUS.md#follow-ups-2026-08-04): the drilling-side floods obeying the throat rule, and multi-cavity drainage. |
| **[FU-75](../CURRENT-STATUS.md#follow-ups-2026-08-04) is coupled to FU-82, census first** | Tightening `HOLLOW_CAVITY_MARCH_MARGIN_MM` moves the cavity boundary and therefore **moves the ambiguity band** FU-82's census counts. Run the census against today's margin, then let the proof move it. |
| **Q18 + Q19 + Q20 are ONE record-keeping pass** | All three are digest chain of custody. Filed together as **FU-84**. |

---

## 6. The one escalated owner decision — OPEN, with Craig

**Question.** *What does "a passage wide enough for resin to flow" mean in the shipped
engine — the nominal 3.0 mm bore the doctrine sentence cites, the 2.974 mm bore the cutter
realizes, or the 3.5 mm passage the code actually requires?*

**Why it is one decision and not three.** §5.2 questions 2, 3 and 16 each poke at a different
part of the same sentence, and each was answered in isolation against a different number:

- **Q3** was ruled by the owner (§2): the geometry stands, the record says *realized*.
- **Q2**'s ruling (radius, not diameter) was made against **1.5 mm** — a description of the
  code that was already out of date by the half-voxel tightening (§4.2).
- **Q16**'s ruling (`>=`) was justified by lattice attainability **at 1.5 mm**, which is the
  wrong threshold and reverses the conclusion (§4.3).

Put together: the doctrine sentence says the throat rule is the drain bore; the drill realizes
0.9914449 of that bore; and the connectivity test requires a passage **17 %** wider than the
nominal bore and **18 %** wider than the realized one. Three numbers, one doctrine.

**What is NOT in question**, so that no reader mistakes this for an opening:

- **Nothing may be loosened.** The half-voxel correction is a tightening and FU-8b binds. This
  is a question about what the doctrine *says*, not about relaxing what it *does*.
- The cutter stays inscribed (§2, clause 1).
- `HOLLOW_DRAIN_R_MM` and `HOLLOW_VENT_R_MM` keep their registered values (§2, clause 2).
- `>=` stands (§3.5).

**The shapes an answer could take**, offered as the decision surface and not as a
recommendation:

1. **The doctrine text is the thing that is wrong.** Restate FU-47's rule in terms of the
   *shipped comparison* — "a passage whose free clearance admits the registered bore beside
   the realized cavity wall", 1.75 mm at registered values — and let the 3.0 mm figure appear
   only as the bore that gets drilled. No code moves.
2. **The extra margin is deliberate and should be named.** The 0.53 mm the engine demands over
   the realized bore is a discretization allowance; register it as such, with provenance,
   instead of leaving it implicit in `+ pitch/2`.
3. **The extra margin is unintended savings loss.** The half-voxel correction refuses passages
   whose realized aperture would in fact drain, and every refusal is cavity volume left solid.
   Measuring that cost is [FU-82](../CURRENT-STATUS.md#follow-ups-2026-08-04)'s census — which
   is itself gated on **FU-83** (§5).

**Status: OPEN. With Craig.** It is cap-adjacent and it is a physical-doctrine question, which
is exactly the escalation boundary the co-owner authority came with. No lane should guess at
it, and no lane is blocked by it: every ruling in §3 stands on its own.

---

## 7. New rows filed by this record

Three, at the next free numbers under the recorded collision protocol. **Highest occupied
number before this record: FU-82** (filed by the dual-cavity fixture pass). **No existing row
was renumbered, edited or displaced.**

| # | Row | From |
| --- | --- | --- |
| **FU-83** | `H0ResourceDiagnostics` carries no flow-selection evidence: `FlowStats` is computed on every production hollow and dropped at both call sites — **and it GATES FU-82's census** | §3.4 |
| **FU-84** | The digest chain of custody has a hole: `make_overhang_table`'s OD-8 movement is charged to no lane, FU-38's substituted evidence is unannotated, and FU-53's cross-lane verdict flips are unclosed | §3.6, §3.7, §3.8 |
| **FU-85** | `corpus/baseline/Sphere.pipeline.json`'s surviving fields are unnamed and unattributed: `wall_mm` is a sampled exit distance whose value coincides with a registered keepout extension | §3.10 |

Six existing rows are **annotated rather than duplicated**: FU-47 (the 1.5 vs 1.75 mm
discrepancy and the open owner decision), FU-56 (the Q19 movement with its commits), FU-57
(Q11's measurement, Q10 standing), FU-75 (sequence after FU-82's census), FU-76 (delete the
dead `evidence` field in the same commit) and FU-82 (gated on FU-83).

**No cap, gate, floor, tolerance or registered constant is touched by this record.**
