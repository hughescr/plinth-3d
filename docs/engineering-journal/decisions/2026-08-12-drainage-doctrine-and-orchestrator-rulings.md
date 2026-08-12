# Decision — the drainage physics doctrine (owner), and twenty-three orchestrator rulings on the ruled batch's open questions

**Two classes of decision are recorded here, and they do not carry the same weight.**

**(A) Owner rulings.** Given 2026-08-12 by the owner (Craig Hughes) on drainage
physics. These carry full owner authority — the same weight as the ten rulings in
[`2026-08-12-owner-rulings-fu-batch.md`](2026-08-12-owner-rulings-fu-batch.md).

**(B) Orchestrator rulings.** Made under co-owner authority the owner granted the
same day: *"You are a co-designer/owner as the orchestrator."* They resolve the
twenty-three open questions the four lane planners filed rather than guessed at,
stage the two-tier wall doctrine, and dispose of the journal-hygiene workflow's
questions. They are recorded **as orchestrator rulings**, with that attribution,
and they are distinguishable from owner rulings on sight: every one of them is in
Part B.

**The escalation boundary the authority came with**, stated by the orchestrator when
it was accepted and binding on every later orchestrator: **physical-doctrine
expansions, reliability-vs-savings tradeoffs, new-scope spend, and anything
cap-adjacent still go to Craig.** Part A exists because the drainage questions hit
that boundary and were escalated rather than ruled.

**What this record is, and is not.** It is a record. The commit that lands it
touches documentation only: **no gate, cap, threshold, bound, floor, tolerance,
constant or registry entry moves in it.** FU-8b's lesson binds everyone here,
orchestrator included — nothing below loosens anything. Where a ruling spawns work
it is filed as a **new** ledger row at the next free number
([`CURRENT-STATUS.md#follow-ups-2026-08-04`](../CURRENT-STATUS.md#follow-ups-2026-08-04)),
never by renumbering or reusing an existing one.

**Source of record.** The lane planners' `openQuestions`, the owner's drainage
conversation of 2026-08-12, an Explore audit of `develop` performed the same day,
and the journal-hygiene workflow's close-out. The batch that these questions came
out of is recorded in
[`2026-08-12-ruled-batch-landed.md`](2026-08-12-ruled-batch-landed.md).

---

# Part A — OWNER RULINGS, 2026-08-12: drainage physics

Two questions were escalated as genuinely owner-scoped (E1 and E2 below). Both were
**ruled by Craig on 2026-08-12**, and the rulings came back wider than the questions
asked.

## A.1 — E1: the throat rule extends to the drilling side, and the doctrine is wider than throats

**The question escalated.** Should the resin-flow throat rule (FU-47's
`throat radius >= HOLLOW_DRAIN_R_MM`) also govern the *drilling-side*
drainability and trapped-pocket floods, which today are face-adjacency-only?

**RULED BY CRAIG 2026-08-12: YES, and the doctrine is wider than throats.**

Verbatim intent, as the owner gave it: interior connecting passages and drains must
be wide enough (the throat rule) **and** oriented such that resin can actually
flow — *"they must be at the 'bottom' as the model lifts off the film so there is no
pooling in small hollows which will never be able to drain. Resin must always be
able to flow 'down' out of any cavity and have an inlet somewhere above for
replacement air to flow in (it's possible that this rule is already in place)."*

**The doctrine components, for the FU spec:**

**(a)** The drilling-side drainability and trapped-pocket checks **obey the
`>= bore` throat rule**. No more face-adjacency-only floods: a flood that walks
through a neck narrower than the bore is claiming drainage the resin cannot perform.

**(b)** Drainage is **ORIENTATION-AWARE**, in the **print orientation** — the
orientation in which the model lifts off the film. A passage only counts if resin
**gravity-flows** through it; drain bores sit at each cavity's **local low point**;
and **any local pocket of the cavity that would pool** — a local minimum with no
downward path to a drain — **is a defect**: refill it solid, or split it per
multi-cavity (A.6).

**(c)** Every admitted cavity needs **BOTH** a downward resin outflow **AND** an air
inlet above it for replacement air (a vent). The owner explicitly flagged that this
rule may already exist — **verify before building it** (see A.7: it exists, but once
per model, not once per cavity).

**(d)** Design consequence: **drains are planned after orientation is chosen**, and
**if orientation changes, drain planning re-runs.** This is its own measured FU with
before/after corpus evidence, built **together with** E2 multi-cavity — the two are
halves of the same physics.

## A.2 — (e) cavity-to-cavity drainage is legitimate

**RULED BY CRAIG 2026-08-12.** A cavity may drain **either to the exterior OR into
another cavity that itself drains** — in the owner's words, *"red can drain to green
or to the exterior."*

Drainage therefore forms a **chain/DAG that must terminate at the exterior**. Every
link in the chain obeys **both** the throat rule and the gravity rule; a chain that
does not reach the exterior is not drainage.

## A.3 — (f) bore-target preference: prefer the interior route

**RULED BY CRAIG 2026-08-12.** When drilling a drain for a cavity, **PREFER boring
into a neighbouring cavity over boring through to the outside**, to avoid scarring
the visible model surface.

This is a **soft preference**: an exterior bore stays fully legal when no interior
route works. The owner's own assessment: probably rare in practice, but worth
encoding.

## A.4 — (g) throat-widening is an admissible repair, behind two checks

**RULED BY CRAIG 2026-08-12.** A **sub-bore neck between cavities may be drilled out
to full bore diameter**, making the passage drainable. This is admitted **ONLY**
behind two checks:

1. **Wall check.** The **realized wall at the widened neck stays at or above spec**
   (the FU-53/FU-55 wall doctrine) — no unprintably thin neck walls.
2. **Structural/physics check.** The model, **including its supports**, still holds
   up through printing **and peel**.

**If either check fails**, fall back to treating the far side as its own cavity per
A.2, or refill solid. Widening is never a way around the wall spec.

## A.5 — (g2) the two-tier wall doctrine, and its staging

**Owner ruling (Craig, 2026-08-12).** The **normal wall spec serves GREEN-state
self-support during printing**. A neck that is carried by adequate external struts
only needs a **CURED-strength minimum** — **~1.5 mm proposed** — so the model does
not break at support removal. Two tiers, two different physical jobs.

**STAGING — this half is an ORCHESTRATOR ruling**, recorded here beside the owner
ruling it stages because they are unreadable apart:

- **Phase 1.** The drainage FU ships **throat-widening gated by the NORMAL wall spec
  only**, and **COUNTS how often that conservative gate refuses a widening**. That
  counter is **the demand signal**. Filed as **FU-70**.
- **Phase 2.** Its own FU, **built only if the counter says it is material**:
  region-tagged neck sections get the lower cured-minimum floor, admitted via a
  **drilling → support obligation channel verified at the gate** — new machinery, the
  same class as the coverage-closure controller. Filed as **FU-71**.
- **The 1.5 mm figure is PROVISIONAL** and **must get coupon-plate provenance before
  going live**: neck articles at **1.0 / 1.25 / 1.5 / 2.0 mm**, **supported vs bare**,
  **print + peel survival** and then a **post-cure break test**. It becomes a
  **measured owner constant, never an intuition**. It is **not registered** today and
  must not be registered on the strength of this record.
- **Nothing in Phase 1 loosens any existing check.**

## A.6 — E2: multi-cavity hollowing is allowed, and encouraged

**RULED BY CRAIG 2026-08-12**, verbatim: *"Multi-cavity is allowed (and encouraged!)
as long as each cavity is large enough and properly drainable."*

**What that authorizes.** The engine's **one-cavity hard-coding is lifted**
(`drilling.rs`'s `cavity_id: 1`, and `validate_shell_topology`'s single-component
requirement). **Every flow-separated interior component becomes a candidate cavity.**
A candidate is admitted only if:

- **(a) it is large enough** — per-cavity admission against the **existing registered**
  savings/admission machinery. **No new physics constant without escalation.**
- **(b) it is properly drainable** — its own drain and vent bores satisfying the FU-47
  throat doctrine, as extended by A.1–A.4.

**Candidates failing either condition are left solid — never a trap.** The FU-47
interim (largest interior component only) **stands until this FU is built**.

## A.7 — Engine reality check: what already exists on `develop`, and what does not

An Explore audit of `develop` on 2026-08-12 measured the gap between the doctrine
above and the code. **More of this exists than expected**, and a future implementer
should start from these facts rather than from the doctrine alone.

**ALREADY IN PLACE:**

- The mesh is **in final print orientation before hollowing** — orientation is baked
  in (`crates/plinth-pipeline/src/prepare.rs:811` and `:830`), so A.1(b)'s
  "print orientation" is already the frame the hollow stage works in.
- Drains are placed **per low lobe, at each lobe's own min-z**, with **downward-only
  search rays**.
- The **drainability proof is monotone-in-z** (`crates/plinth-hollow/src/drilling.rs`
  ~`1497-1563`): resin is never required to climb — a voxel counts as drainable only
  via a path that never rises — with a **hard `TrappedCavity` refusal**.
- **Gravity-trapped pockets get rescue drains** at their own low points (**capped at
  3**, each **>= 0.5 mL**), and the remainder is **refilled solid**.
- **A vent is MANDATORY** — refusal without one — placed at the **global cavity top**.

**GAPS vs the ruling:**

- **Exactly ONE vent per model, not per cavity**, and **none added for rescue drains**.
  A.1(c) requires one per admitted cavity.
- **Vent-above-drain is true by construction but never asserted as an invariant.**
- **Only ONE cavity is ever retained** — `cavity_id` hard-coded to `1` — the known gap
  A.6 lifts. **The code itself already documents this gap**: `plinth-metrics`'s
  topology module states that `validate_shell_topology` proves *exactly one* negative
  shell and that every further cavity is outside what it proves
  (`crates/plinth-metrics/src/topology.rs` ~`332`, `351`, `711-717`).
- **No geometric pocket detector.** Pockets are found only as *residue* after drain
  placement, never predicted from the geometry — so A.1(b)'s "would pool" test does
  not exist as a test.
- **The rescue-drain cap (3) means overflow pockets silently refill.**
- **No drainage term in orientation scoring.** `HOLLOW_ORIENT_ENABLE_DEFAULT` is
  registered but **unread**.
- **No cavity-to-cavity bores** (A.2), **no interior-target preference** (A.3), **no
  throat-widening repair** (A.4).

**Constants inventory for the drainage work:** `HOLLOW_DRAIN_R_MM` **1.5 mm**;
`HOLLOW_VENT_R_MM` **1.25 mm**; lobe band **15 mm**; lobe minimum **0.5 mL**; vent top
band **2 mm**. `AUDIT_DIRECTIONS` and the drill-through extensions exist as
**unregistered literals duplicating registry values** — that hygiene item is **already
filed as FU-66** (the `drilling.rs` literal ladder cross-asserted nowhere) and is
**not** re-filed here; FU-66 carries a pointer to this record.

---

# Part B — ORCHESTRATOR RULINGS, 2026-08-12

**Authority.** Craig, 2026-08-12: *"You are a co-designer/owner as the orchestrator."*
Technical questions are ruled here directly. **Physical-doctrine expansions,
reliability-vs-savings tradeoffs, new-scope spend, and anything cap-adjacent still
escalate to Craig** — which is why A.1 and A.6 are in Part A and not this part.
**Nothing here loosens any cap, gate, floor or tolerance.**

**Source.** The four lane plans' `openQuestions`. **Format:** **RULED** = decided now
(some create follow-up code work); **STANDS** = the lane's conservative choice is
confirmed as the permanent answer.

## B.1 — FU-56 coverage closure

**1. Gate-predicate duplication (Option A extract vs B mirror + equivalence test) —
STANDS (B).** The owner ruling says the gate does not move one byte; a **mirrored
predicate pinned by an equivalence test** in `plinth-verify/tests` cannot alter gate
behaviour even by accident. Revisit only if the predicate itself ever changes.

**2. Refusal disposition / exit code — RULED (c), follow-up code change.** The three
candidates were (a) `Failed`/5, (b) `Refused { gate: None }`/5, (c)
`Refused { gate: Some(UncoveredIslands) }`/**3**. **(c) is the ruling.** The physical
fact — coverage not closable — **is the same fact the gate refused on** before the
controller existed. The controller applies the gate's own predicate (mirrored and
equivalence-tested), so projecting to `Refused { gate: Some(UncoveredIslands) }` with
exit **3** is **honest**, keeps the residual **one class end to end**, keeps the CLI
saying **GATE-REFUSED for a gate-shaped fact**, and keeps **rerun metrics comparable
across batches**. **The lane's interim (b) must be migrated to (c)** — filed as
**FU-76**.

**3. Typed refusal vs letting the gate speak — STANDS (typed refusal), combined with
ruling 2.** The serialized `CoverageClosureRefusal` carries the evidence; with (c)'s
projection the refusal class is unified, so **FU-57's single evidence schema covers
both paths**.

**4. Synthetic-island repair rows nominated by the controller — STANDS (allowed).**
The pool was built to span both row kinds; **lineage/contact validation plus
confinement to the patch's own `(island_id, component_id)`** is sufficient containment.

## B.2 — FU-57 evidence

**5. SVG vs PNG renders — STANDS (SVG).** Zero new production dependencies,
deterministic bytes, opens everywhere Craig looks at files. PNG buys nothing here.

**6. Face-id access: `serde_json` walk vs read-only accessor — RULED (add the
accessor), follow-up code change.** *"The gate does not move"* is about **verdict
behaviour**, not a freeze on **additive read-only API**. A public `sorted_face_ids`
accessor on `CoverageReport` is additive, testable, and removes a fragile
serialization walk. **Reverse the lane's workaround** — filed as **FU-77**.

**7. Sidecar/render naming and counts — STANDS.** `<model>.coverage.json` with island
SVGs **beside it, not in a subdirectory**; **5 on the card / 3 rendered**, as named
presentation constants. **Presentation bounds are not M0 physics constants**;
unregistered named constants are correct here.

## B.3 — FU-47 connectivity

**8. Radius vs diameter threshold — STANDS (the recorded ruling wins).** Throat
**RADIUS >= `HOLLOW_DRAIN_R_MM` (1.5 mm)** = the passage admits the **3.0 mm bore**.
The spawn brief's *"3.0 mm radius"* was **the orchestrator's transcription error**;
the lane correctly followed the recorded ruling.

> **CORRECTION 2026-08-12 — this ruling quotes 1.5 mm; the shipped threshold is 1.75 mm,
> and the ruling was therefore made against a description of the code that was already out
> of date.** Left standing rather than reworded, per this project's convention. Verified in
> `crates/plinth-hollow/src/flow.rs:147-149` at `9e5dbb0`:
> `core_clearance_threshold_mm(throat_r_mm, pitch_mm) = throat_r_mm + 0.5 * pitch_mm`,
> applied as `clearance >= core_threshold_mm`. At the registered values
> (`HOLLOW_DRAIN_R_MM` 1.5 mm, `HOLLOW_PITCH_MM` 0.5 mm) the engine requires **1.75 mm of
> clearance radius — a 3.5 mm passage**, not 3.0 mm. The half-voxel correction is an
> adversarial-review **tightening**: it is correct in itself, it moves no registered
> constant, and it is documented in `flow.rs` and in the FU-47 ledger row. **The verdict
> above — radius, not diameter — is unaffected and stands.** What is corrected is the
> number the ruling states, and the fact that a ruling recorded against a stale description
> is one a later reader will apply to the wrong threshold. The residue — *what "wide enough
> for resin to flow" means in the shipped engine, given three different numbers for one
> doctrine* — is escalated to the owner as a single decision and is **OPEN**; see
> [`2026-08-12-reconciliation-and-rulings.md`](2026-08-12-reconciliation-and-rulings.md)
> §4.2 and §6. **Nothing is loosened by this correction and no constant moves.**

**9. `>=` vs `>` at the lattice-attainable boundary — STANDS (`>=`).** A throat
**exactly** admitting the bore **physically drains**; the ruling's wording is the spec.

**10. New typed refusal `IneligibleCavityBelowDrainThroat` — STANDS (keep the
variant).** A record that cannot distinguish **"no interior"** from **"no *drainable*
interior"** is exactly the kind of evidence-destroying shortcut FU-58 exists to
condemn.

**11. `REPAIR_PINCH_SPLIT_MAX_MM` left registered-but-unread — RULED.** **Leave it
registered this batch.** File a follow-up for a **registry retirement mechanism**
(retired-class **provenance, not deletion**): the 303-name inventory should be able to
say *"this constant's reader was removed by FU-47"* **without surgery**. Filed as
**FU-74**.

**12. FU-48 predicate untouched — STANDS.** Correct reading; **FU-48 remains unruled**
and the decimate gate is **not weakened by prose corrections**.

## B.4 — FU-36 sphere generator

**13. Closure bound value (1e-15 vs 1e-16) — STANDS (1e-15).** ~**65x** headroom over
the measured post-fix worst case (**1.52e-17**) vs **6.5x** for 1e-16. **Robustness
beats maximal tightness**; any tightening complies with the ruling.

**14. Symmetry scope (poles only vs + equator snap + mirrored southern latitudes) —
STANDS (full scope).** The ruling's **principle clause** — symmetric north vs south
pole — **is unsatisfiable as an exact invariant with pole snapping alone**; the
byte-movement note was **illustrative, not limiting**. Exact invariant tests are the
point.

**15. Longitude quadrants left unsnapped — STANDS.** Not needed for manifoldness or
N/S symmetry; fewer bytes moved. Revisit only if it ever bites a test.

**16. A/B scope keeps the treant leg — STANDS.** Parity with the recorded A/B process;
**don't shave measured process for cost without a recorded reason**.

**17. Gitignored in-repo `exports/` example output — STANDS (acceptable).** The
constraint is that **run outputs never enter git**; a **gitignored** example dir plus
copy-out satisfies it. **No authorized-change needed.**

## B.5 — FU-50 / FU-53 / FU-54 / FU-37 hollow lane

**18. Deleting unreachable `RasterSubdivisionLimitExceeded` +
`MAX_RASTER_SUBDIVISION_ROUNDS` — STANDS (delete).** Removing an **unreachable variant
of the machinery FU-50 mandates replacing** loosens nothing; the **binding cap
(`max_raster_work_items`) is untouched**. Internal workspace API may delete freely.

**19. FU-50 headroom: measured 10–40x vs the ruling note's "~3 orders of magnitude" —
FINDING, no ruling needed.** **Record the MEASURED figure** in the ledger and
**correct the expectation prose** in the rulings record — **annotate, don't rewrite
history**. **The counting rule must not be adjusted to reach the claimed figure.**
Reported to Craig as an FYI in the batch report.

**20. FU-54 registration route — RULED (b), follow-up code change.** The choice was
(a) private const vs (b) **M0 Placeholder + const mirror + equality test**.
`docs/development.md` says **resource caps are registered**; the cavity-repair pass
limit becomes a **registered Placeholder as single source of truth**, `production()`
stays `const fn` via a **private mirror**, and a **mechanical test pins mirror ==
registry**. **Registering the same threshold once and reading it twice is not double
registration.** The lane shipped route (a) — `H0AdmissionLimits::max_cavity_repair_passes`,
production default **8**, unregistered — so the follow-up applies. Filed as **FU-78**.

**21. FU-53 march margin 0.866 mm vs a possibly-provable ~0.44 mm — RULED (keep 0.866
now).** The conservative margin **only thickens walls — never a safety risk**. File a
follow-up: **produce an airtight proof of the tighter bound** (a Fable-grade proof
task), and **only a proof — not an argument — may tighten the registered constant**.
**Note for Craig:** this margin **costs cavity volume and will push some marginal
models to solid**; if the savings loss shows up in practice, **the proof task is the
lever, not the constant**. Filed as **FU-75**.

**22. FU-37 requested-wall vs FU-55 realized-wall as two fields — STANDS (two
fields).** `wall` = **requested** (per the recorded §8 ruling); `realized_wall_mm` =
**FU-55's mandated evidence**. They answer **different questions** — *"what did the run
ask for"* vs *"what did it get"* — and **merging them would destroy exactly the
discrepancy FU-53 was about**. The orchestrator's own lane brief wording (*"realized on
FU-37"*) **was wrong**; the lane's split is the design.

## B.6 — Resolved by construction

**23. FU-38 re-pin sequencing vs FU-56 golden churn; cross-lane `lib.rs`
interleaving — resolved, no ruling needed.** Lanes merged **sequentially** with a
conflict-resolving merger, and **FU-38 re-pins run last** (Integrate phase) against the
**final** engine, owning the from→to attribution.

## B.7 — Journal hygiene

**The finding (Craig 2026-08-12 + orchestrator ruling).** CURRENT-STATUS.md was
**well-maintained but never curated**: **8,782 lines**, **68 FU rows** (~45+ closed),
narrative back to 2026-08-03, **no archive**. **Every spawned agent reads it**, so dead
history taxes every task and superseded rows risk misleading agents.

**RULED:** after the batch lands, a **docs-only** pass splits it — CURRENT-STATUS.md
keeps **open/blocked/in-progress rows, standing doctrine, and a short system
summary**; **closed rows + old narrative move verbatim** to dated archive files with
anchor stubs or rewritten cross-references, and **a verifier agent must check every
journal link before commit**. **Standing scribe rule thereafter: closing a row moves
it to the archive in the same commit.**

**The split landed** on `feature/journal-hygiene` (`db640ea` + fix `0da7562`):
**8,782 → 2,112 lines**, **53 rows/sections archived verbatim** to 4 dated archive
files, **38 link repairs**, verbatim preservation **mechanically proven**.

### Dispositions on the hygiene workflow's own open questions (orchestrator, post-workflow)

- **System summary — RULED: add it.** The auditor was right that it is a **4th
  addition class** beyond the original three; the orchestrator **sanctions** it, because
  it directly serves the "sharply focused" goal. Use the fixer's corrected text (five
  LANDED: FU-36/37/50/54/55; *"on and before 2026-08-09"*).
- **Stale `CURRENT-STATUS.md:NNN` line-number citations — RULED: leave them honestly
  dangling.** The fixer **proved most were already wrong pre-split**, so **guessing
  produces silently-wrong citations**, which is worse than a visibly stale one. File a
  **citation-audit follow-up** doing **git-history bisection per citation** — filed as
  **FU-72**. **NEW CONVENTION**, to land in `06-conventions.md` at merge time: **never
  cite CURRENT-STATUS.md by line number — cite anchors; the file churns.** *(That
  convention is now in force; see `06-conventions.md` §12, "Journal hygiene".)*
- **`docs/rust-migration/` stale paths — RULED: separate mechanical follow-up**, not
  that branch's scope. Filed as **FU-73**.
- **A row for FU-53's unmeasured raw minis (Warhorse / Giant Toad / Awakened Tree) —
  file POST-batch at the then-next free number**, because the main batch's scribe was
  filing FU rows concurrently and filing in parallel guarantees a collision. **This is
  now discharged without a new row: the batch scribe's own FU-62 covers exactly this
  substance** (zero of 37 minis reaches drilling; FU-53's test item 3 named as
  outstanding). See "Folded in, not filed" below.
- **FU-36 archival + FU-49 closure** — handled by the **next closing commit under the
  new standing scribe rule**; no special action.
- **Archived critical-path item 6 (tournament-bias + `NotAttached` tripwire)** — the
  post-batch scribe **re-names the live remnants** so the work items are findable by
  name again. FU-17 now carries that pointer.
- **Plain-text anchor mentions (6 sites)** — stay as-is; the stubs keep them
  resolvable.
- **FU-2 / FU-9 / FU-34 ambiguous rows** — **kept**, per default-to-keep; **queued for
  Craig's next briefing session** (owner closures, cheap to batch with OQ-8/9/10).
- **Merge ordering** — `feature/journal-hygiene` merges **only after** the batch
  workflow lands its scribe commit; the merge agent reconciles the scribe's row edits
  into the slimmed structure and adds the no-line-number-citations convention.

## B.8 — Follow-up code work created by these rulings

| Ruling | Work | Filed as |
| --- | --- | --- |
| B.1 #2 (R2) | Migrate the FU-56 refusal projection from (b) to (c): `Refused { gate: Some(UncoveredIslands) }`, exit **3** | FU-76 |
| B.2 #6 (R6) | Add a read-only face-id accessor to `CoverageReport`; remove the `serde_json` walk | FU-77 |
| B.3 #11 (R11) | Registry **retirement mechanism** (file, don't build yet) | FU-74 |
| B.5 #20 (R20) | Register the cavity-repair pass limit as an **M0 Placeholder** + const mirror + equality test | FU-78 |
| B.5 #21 (R21) | **Proof task** for the tighter march margin (file, don't build yet) | FU-75 |

---

# Part C — what this record files

**Nine new ledger rows, FU-70 through FU-78**, at the next free numbers under the
recorded collision protocol. The ledger ran through **FU-69** before this record;
**no existing row was renumbered, reused or displaced.**

| # | Covers | From |
| --- | --- | --- |
| **FU-70** | **The drainage architecture** — multi-cavity hollowing, per-cavity drains and vents, cavity-to-cavity drainage, interior-bore preference, orientation-aware low-point drains and pooling detection, throat-rule-obeying drilling-side floods, and **phase-1 throat-widening gated by the normal wall spec with a refusal counter** | A.1–A.4, A.6, A.7 |
| **FU-71** | **Phase 2 of the two-tier wall doctrine** — the cured-strength floor at region-tagged necks, **gated on FU-70's refusal counter being material** and on **coupon-plate provenance** for the PROVISIONAL ~1.5 mm | A.5 |
| **FU-72** | **Citation audit** — stale `CURRENT-STATUS.md:NNN` line-number citations, repaired by **git-history bisection per citation** | B.7 |
| **FU-73** | **Stale `docs/rust-migration/` path references** — mechanical repo-wide cleanup | B.7 |
| **FU-74** | **Registry retirement mechanism** — record a retired constant's provenance without surgery (`REPAIR_PINCH_SPLIT_MAX_MM` is the first case) | B.3 #11 |
| **FU-75** | **Proof task for the FU-53 march margin** — only an airtight proof may tighten `HOLLOW_CAVITY_MARCH_MARGIN_MM` | B.5 #21 |
| **FU-76** | **FU-56 refusal projection** to `Refused { gate: Some(UncoveredIslands) }` / exit 3 | B.1 #2 |
| **FU-77** | **`CoverageReport` read-only face-id accessor**, replacing the `serde_json` walk | B.2 #6 |
| **FU-78** | **Register the cavity-repair pass limit** as an M0 Placeholder with a const mirror and an equality test | B.5 #20 |

## Folded in, not filed

Three items in this record's scope were **already covered by existing rows**, and are
**annotated there rather than duplicated**:

- **The unregistered `drilling.rs` literals** (`AUDIT_DIRECTIONS`, the drill-through
  extensions) named in A.7's constants inventory are **FU-66** — the same defect class,
  already filed 2026-08-12 by the FU-61 verification. FU-66 now points here.
- **FU-53's unmeasured raw minis**, which B.7 deferred to a post-batch row, are
  **FU-62** — the batch scribe's row already claims exactly this (zero of 37 minis
  reaches drilling; every hollow guarantee is fixture-proved; FU-53's test item 3 named
  as outstanding since that landing). Filing a tenth row would have been a duplicate.
- **The refusal-class fragmentation in the run summary** that B.1 #2's projection
  partly answers is **FU-68**'s second claim. FU-68 now points at FU-76 as the ruled
  answer to that half of its first action.

## What is deliberately NOT decided here

- **FU-48** remains **unruled** (B.3 #12).
- **The ~1.5 mm cured-strength minimum is PROVISIONAL and unregistered** (A.5). It
  must not be registered, read, or relied on until the coupon plate measures it.
- **`HOLLOW_CAVITY_MARCH_MARGIN_MM` stays at 0.8660254037844386 mm** (B.5 #21). The
  conservative value stands until a proof — not an argument — replaces it.
- **No per-cavity admission physics constant is authorized** (A.6): per-cavity
  admission runs against the **existing registered** machinery, and a new physics
  constant is an **escalation to Craig**.
