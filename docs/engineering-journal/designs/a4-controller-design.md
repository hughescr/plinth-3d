# Design v2.1: the SPEC-A4-conformant coverage-repair controller (task #39)

Design only. No production code changed. **Written against `develop` @ `06cf21b`;
amended 2026-08-20 to be true on `develop` @ `a8feff4`**, which is the base this
build implements against. Read-only; nothing in this pass edited engine code.

**v2.1 is an amendment, not a rewrite.** It does three things and nothing else:

1. it folds in Craig's rulings of 2026-08-19/20 — **R10** (no failed-tries cap),
   **R11** (the registered bound counts committed steps only, and no new
   registered constant lands in this lane), **R12** (the O-7 flat-round
   investigation is the implementer's mandate, and its answer lands as **§B-O7**
   during Core), **R13** (spatial diversity on rejection), **R14** (termination
   reasons are measured defect classes) — into the sections that own them, not
   as an appendix. §K carries all five.
2. it re-walks §G's pin list and §H's assumptions against `a8feff4` and corrects
   every stale `file:line`, digest, count and claim. `a8feff4` moved a great deal
   under this document: **#49 landed** (`b8edcb8`, SPEC §6's unrouted-regular
   strip plus ownerless-island disclosure), **the reach rule landed**
   (`fe8fd3c`, task #62), and **the tip arc landed** (`a8feff4`, task #61 — a
   0.05 mm tip-radius minimum, grow-to-attest, the descending refinement scan
   and Craig's 0.4 mm scar bound). **§G.4 is new and lists every from→to.**
3. it folds the three corrections the stage-0 measurement pass produced against
   the design's own text: §F.2's treant paragraph, §F.5's 489, and §G.3's "does
   not touch #48".

**Everything §J says about v1→v2 still stands.** Where v2.1 changes a v2
statement, it says so in place, with the word **AMENDED (v2.1)**.

**v1 is kept beside this file as `design-v1.md`.** v2 replaces it. §J lists every
change and why.

v2 exists because v1 was challenged (`design-review.md`, verdict
SOUND-WITH-CHANGES) and because Craig then ruled on the challenge. Every ruling
recorded in §K is implemented in the body of this document, not appended to it.
Where a ruling turned out to have a cost or a limit that the ruling did not
anticipate, the cost is written down in the same place as the ruling, in plain
words, and Craig's override is left open. Nothing is fudged to make the ruling
read better than it measures.

Author's note on words. I use **censor** for the 0.16 mm² A3 uncertainty
threshold, **minimum** where a smallest allowed value is meant, and I never use
"floor" for anything but the bottom of a printed part. The code today spells the
censor `patch_area_floor_mm2` / `area_floor_mm2`; that is a misnomer (it is a
censor, not a smallest allowed value) and it belongs to the #12 naming pass, not
to this one.

---

## 0. Vocabulary, defined once

| term | meaning here |
| --- | --- |
| **sample** | one dyadic subdivision centroid of a named-island source triangle, carrying an area weight (SPEC §4). The sample set and its area weights are built once at attempt 0 and are **frozen for the run**; only which samples are uncovered changes. |
| **uncovered sample** | a sample whose nearest routed contact on the same island and the same exact topology component is farther than `R = 6.0 mm`. |
| **patch** | one connected group of source faces that each hold at least one uncovered sample. |
| **route** | one `(named island, exact topology component)` pair. Repair-pool rows are owned by a route; samples belong to a route. |
| **censor** | `voxel_pitch_mm²` = 0.16 mm². A patch at or below it is censored A3 uncertainty (SPEC §4). |
| **failing patch** | a patch whose uncovered area is *greater than* the censor, or that touches an ambiguous topology cut. This is SPEC §4's word and it is the censored set. |
| **repair worklist** | every route with any measured uncovered area, censored or not. Uncensored on purpose (the 2026-08-14 ruling, task #27). |
| **repair-pool row** | one frozen, immutable candidate seat in `RepairPool`, identified by `RepairRowId` (route-qualified: the same physical point can appear under several islands). |
| **global score** | for one candidate row: the summed area weight of **every uncovered sample within `R` of its seat, on any route**. New in v2 (§A.4, ruling R3). v1 scored only the candidate's own route. |
| **trial** | one repair-pool row proposed, built, routed, braced, lowered, measured, and then either committed or rolled back. The unit this design introduces. |
| **commit** | a trial that passed all three accept clauses (§B.5) and became the accepted plan. |
| **rejection** | a trial that did not. Its child plan is dropped; the row is ledgered. |
| **attempt** | today's unit: one measurement plus a batch of appended rows. This design deletes it. |

---

## 1. What the spec requires, clause by clause

SPEC-A3-A4-COVERAGE.md §6, quoted verbatim, with the design section that
satisfies each clause.

> A4 preserves exact candidate-pool row identity throughout.

Satisfied unchanged: rows stay `RepairRowId` values from the frozen
`RepairPool`; nothing in this design mints, merges, or renumbers a row. (§H)

> Before rewire, bracing, carried-load repartition, or coverage judgment, every
> unrouted regular contact is ledgered with its exact row/reason and removed
> from the accepted contact set. An unrouted specialized minimum or
> island-birth contact is a hard failure.

**AMENDED (v2.1) — this clause IS implemented at `a8feff4`, and the v2 text
below was true only at `06cf21b`.** Task #49 landed in `b8edcb8`
(`feature/spec6-removal`), which is ruling R8 discharged. What v2 asked for as a
precondition is now the base:

- **Production strips the unrouted regulars.** `build_accepted_contact_set`
  (`runner.rs:3200`–`3275`) collects every unrouted regular terminal into
  `UnroutedRegularRemoval` rows (`runner.rs:2937`) carrying `contact_id`,
  `terminal_id`, `route`, `reason` and `carried_area_mm2`, and removes them in
  **one** batched edit — `plan.begin_regular_edit()?.remove_demand_free_regular_rows(&ids)`
  at **`runner.rs:3264`–`3266`** — so `derive_child` replays the load finalizer
  once, and §6's carried-load repartition happens exactly once per run.
  `AcceptedContactSet` (`runner.rs:3029`) holds the reduced plan, the removal
  ledger, the unrouted-specialized list and its own sealed digest, re-checked at
  every downstream preflight (`runner.rs:3140`).
- **`remove_demand_free_regular` (singular) is still test-only.** It is defined
  at `specialized.rs:1263` as a one-element delegation to
  `remove_demand_free_regular_rows` (`specialized.rs:1289`), and its five callers
  are still all inside `mod tests` (`specialized.rs:12793, 12846, 13309, 13451,
  14375`; the module starts at `specialized.rs:8317`). **The production caller is
  the batched form.** v2 named the singular function and concluded "no production
  caller"; at `a8feff4` that reads as a false negative, and this paragraph is the
  correction.
- **The consequence v2 wanted is delivered.** §C's non-regression vector now
  reads out of a load partition that no longer holds contacts the spec says must
  be gone. `unrouted_no_winner` still counts terminals, but the plan they sat in
  has been reduced before rewire, brace and repartition, which is what R8 was
  for. **The re-freeze happens once, not twice**, and this design is no longer
  sequenced behind anything.
- **The forest path is unchanged and still there.** `retire_unrouted_terminal`
  (`cco.rs:1707`/`1793`, the FU-88 path) still retires the terminal in the
  forest; #49 added the *plan-side* removal beside it, it did not replace it.
- **NEW SURFACE the controller must carry: ownerless faces.** The repartition can
  now leave a model face charged to no contact at all — every load owner eligible
  for it lost every contact it had. `AcceptedContactSet::ownerless_faces`
  (`runner.rs:3051`) holds them as `perception::OwnerlessLoadFace`
  (`perception.rs:172`), and #49 ruled them a **disclosure, not a refusal**,
  precisely because the measurement that decides whether the face is unsupported
  is the routed-coverage judgment §6 sequences *after* the removal. That ruling
  is this controller's premise, written into the base: a set-membership tally may
  not pre-empt a physical measurement. §C.1 gains no component for it, and §C.1
  says why.

> **AND ONE THING #49 LEFT OPEN THAT §C.4 ASSUMES CLOSED — flagged, not silently
> fixed.** SPEC §6 calls an unrouted specialized minimum or island-birth contact
> "a hard failure". At `a8feff4` production **measures and ledgers it without
> refusing on it**: `unrouted_specialized_contacts()` (`runner.rs:1003`) exposes
> the list, and its own doc comment says "§6 calls this case a hard failure; this
> change measures and ledgers it without refusing on it". §C.4's rule — a
> nonzero count on the *accepted* state is `SupportRunError::Failed` — is
> therefore **not** the base's behaviour, and adopting it would be a behaviour
> change this lane has not measured. §C.4 and O-5 are amended to say so.

> Repair candidates come only from the failing named-island/topology-component
> route. Occupied rows and exact failed rows are excluded. After every accepted
> or rejected attempt, candidates are reranked by marginal uncovered sample
> area, with lower pool row as the deterministic tie-break. Candidate scoring
> tiles both candidates and samples under the same 65,536-entry matrix bound.

Satisfied by §A.4 with two readings recorded in §K:

- "failing route" is read as "route on the repair worklist" (ruling R2,
  unchanged from v1's gap 2), because the censor split (#27) is binding and
  censoring the worklist was measured to lose three models.
- "marginal uncovered sample area" is read as **global** — every uncovered
  sample within reach, not only the candidate's own route's (ruling R3, new in
  v2). §A.4 gives the mechanism and §B.6 walks the three at-risk models through
  it.

Exclusion, tie-break and tiling are already correct in
`coverage_closure::nominate_repair_rows` and are kept verbatim. **The per-key
pick cap is not** — see §A.4, rule N1.

> One winner at a time enters a full construct, rewire, brace/size, and verify
> transaction. It is accepted only when:
> 1. the coverage objective strictly improves lexicographically in this order:
>    total failing area, maximum failing-patch area, failing-patch count, worst
>    gap; and
> 2. every noncoverage hard-gate count is componentwise no worse.

Satisfied by §A (the transaction), §B (clause 1 plus the v2 anti-gaming clause)
and §C (clause 2). The lexicographic key clause 1 names **already exists in this
codebase** as `specialized::coverage_key`, and clause 2's shape already exists as
`specialized::gate_not_worse`. This design reuses both, with the conversion cost
priced honestly in §B.4.

**Where "verify" runs, and what it costs.** §6 does not say the measurement is
inside the transaction, nor at what radius it runs. This design commits to the
expensive reading and says so: **the verification is one full routed coverage
measurement, taken inside the transaction, after `lower`, against
`registered_coverage_request`** — the same request
`plinth_pipeline::gates::registered_coverage_request` hands the pre-write gate,
at `R = COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM = 6.0 mm` and the A3 censor
0.16 mm². A narrower reading (a cheap local re-measure around the new seat) is
not excluded by the text, and it is refused here for one reason: the controller
must close to exactly what the gate will re-measure (§H.3). Closing to a
narrower promise than the gate keeps is the forgery FU-8b's doctrine forbids.
The cost of that choice is §F.5, and it is the largest open risk in this design.

> Otherwise the accepted forest is restored and the exact row/reason is
> ledgered. `coverage_repair_max_attempts` defaults to 8. Attempt-cap
> exhaustion cannot convert residual uncovered area into success.

Satisfied by §A.5 (rollback) and §A.6 (the ledger). **Craig has ruled on what
the bound bounds** (ruling R6, §K): it bounds **committed steps only**.
Rejections are bounded by pool exhaustion plus the zero-score stop guard (§B.3).

**AMENDED (v2.1) — R6 is no longer provisional, and two further rulings close
it.** v2 left the override open at §F.5 and recommended registering a
trial-per-commit ceiling as open item O-1. Craig has ruled against that:

> **R10 — there is NO failed-tries cap.** The loop gives up only when no
> candidate can physically help: pool exhaustion, plus §B.3's zero-score guard.
> A counter may never decide an outcome — that is the physics-first doctrine, and
> a trial ceiling is exactly a counter deciding an outcome. **Every run reports
> its trial count and its wall time LOUDLY** (§A.8), so the cost is visible in
> the record instead of being hidden behind a cap.
>
> **R11 — `coverage_repair_max_attempts` counts COMMITTED steps only, and NO new
> registered constant lands in this lane.** No cap, gate, threshold or tolerance
> moves. O-1's recommendation is **withdrawn**, not deferred.

The worst case R10 admits is real and is measured in §F.5. What makes it bearable
is not a cap but **R13** (spatial diversity on rejection, §A.4 rule N4), which
attacks the measured mechanism that made the worst case worst: near-identical
seats tried consecutively at full pipeline cost.

"Cannot convert residual uncovered area into success" is what decides the
Warhorse in §E, and in v2 it is also what makes the anti-gaming clause (§B.5,
clause 3) mandatory rather than optional.

Also load-bearing here:

> **§4** A patch fails if and only if either: 1. its uncovered area is greater
> than `voxel_pitch_mm**2`; or 2. it touches an ambiguous topology cut. Area at
> or below `voxel_pitch_mm**2` is **censored A3 uncertainty**. It is diagnostic
> only … and it is not permission for another hard gate to fail.

This is where §B gets the word "failing", it is why the objective is censored
while the worklist is not, and — new in v2 — it is the exact hole the anti-gaming
clause closes: **the censor is per patch, and patch boundaries are something a
repair can move.** §B.5.

> **§7** An exception, unresolved cap, sweep/round cap exhaustion, or failed
> final gate discards the clone and restores the entire accepted state; no
> partially relocated or resized forest may escape.

Written for Phase-8 R2, but it is the same transaction discipline, and it is the
strongest textual support for reading a *stage refusal on a trial* as a
rollback rather than as a run failure (§A.5, §K ruling R5, task #47).

---

## 2. What is wrong with the loop today, stated in its own numbers

`run_support`, `runner.rs:1107` onward (**AMENDED (v2.1)** — v2 cited
`runner.rs:661–898` at `06cf21b`; #49 and the tip arc moved the whole file). Per
attempt: measure once, then `nominate_repair_rows`
(`coverage_closure.rs:530`) appends **one row per patch on the worklist** —
capped by `if picked >= patches.len() { break; }` at **`coverage_closure.rs:668`**
— all in one `append_regular_repair_rows` batch (`runner.rs:1467`), then re-plan
everything and measure again. There is no per-row accept test, so:

- **Nothing is attributable.** An attempt that appends 266 rows and moves the
  area by 0.4 mm² cannot say which row did it, or that 265 rows did nothing.
- **Nothing is reversible.** `plan = plan.append…(rows)` (`runner.rs:1467`) overwrites
  the parent binding; the previous plan is dropped on the same line. Every
  appended row is permanent, productive or not.
- **Nothing is protected.** A row that makes a noncoverage gate worse is kept.
  Task #47 is exactly this: the Giant Bat runs 8 attempts and dies with
  `RepairPlanRefused { phase: Brace, detail: Brace(ClearanceViolation(1)) }` —
  one violating arm in the repair plan kills the model, where a transaction
  would have rolled that one row back and tried the next.
- **The volume is the cost, and it lands on the wrong islands.** The Warhorse
  appends **609 rows across seven measurement passes** (attempts 0–6 append
  266, 187, 95, 44, 13, 4, 0; the refusal reports `attempts: 6`, meaning six
  repair rounds after attempt 0), of which
  **289 belong to island 5 and 176 to island 6** — the two islands that never
  moved. Every one of those contacts is a permanent member of the plan and a
  permanent entry in the routing cadence, which is what tasks #35 and #48 are
  paying for.
- **NEW IN v2 — the throttle is area-blind, and it is measured.** The cap is one
  seat per *patch* per attempt. On the Rotting Warhorse, island 68's whole
  **11.879 mm² of uncovered area is a single patch** and island 74's **13.041
  mm² is a single patch**. Each therefore received **one seat per attempt**, so
  after 8 attempts the loop had tried **8 seats out of 377** on island 68 and
  **8 out of 219** on island 74 — and refused with **571 usable, eligible,
  in-radius seats never tried** (361 + 210 eligible at the last attempt). A hole
  20× the size of another hole gets exactly the same attention. Source:
  `island-out/islands.json`, and the refusal itself at
  `endpoint-out/judge-rw/run.log` (`AttemptsExhausted`, 8 attempts, 718 rows
  appended in total).

The area-blindness is not a side effect of the batch loop; it is a rule in the
nomination function, and it survives into a one-winner controller unless it is
deleted deliberately. §A.4, rule N1.

- **NEW IN v2.1 — the SECOND throttle defect, and it is the one that decides the
  cost of R10.** Scores tie on absolute area, and the tie-break is pool order
  (`coverage_closure.rs:686`–`693`: `total_cmp().reverse().then_with(|| order())`).
  **Pool order is spatially contiguous**, so equal-scoring seats are walked as a
  contiguous run through one small neighbourhood. The measured shape of that
  defect is **63 seats inside a 1.04 mm box**: sixty-three consecutive candidates
  that are, physically, the same seat. Under batch nomination this cost nothing —
  they were appended together and measured once. **Under one-winner it costs one
  full pipeline pass each**, and it is why §F.5's worst case is a wall of
  identical rejections rather than a search. Ruling **R13** deletes it (§A.4 rule
  N4).

---

## A. The transaction

### A.1 What one repair step is

One repair step is: **one repair-pool row** appended to the accepted plan, the
resulting child plan taken all the way through
`prepared_run_for_plan → route_initial_cco → rewire → brace → structure → lower`,
and one routed coverage measurement taken against the lowered result at the
registered gate request (§1, "where verify runs"). The step then either commits
(the child plan becomes the accepted plan) or rolls back (the child plan is
dropped and the row is ledgered).

That is exactly the spec's "full construct, rewire, brace/size, and verify
transaction", and it is exactly the body of today's attempt loop — the change is
that the body runs on **one** row and its result is a decision instead of a
handover.

**AMENDED (v2.1) — the one-row append already exists and must be used.**
`SpecializedContactPlan::append_regular_repair_row` (`specialized.rs:1339`) is a
one-element delegation to the batched form, documented as byte-identical to the
batch on the same id. The controller appends through it, so nothing in this
design re-implements an append path, and the batch-first discipline is not
violated: a one-winner step has exactly one row by construction, which is the
batch.

### A.2 The accepted state, and what it costs to snapshot

```rust
/// The last state that passed every A4 acceptance test. Rollback restores
/// exactly this, and it is a value, not a mutation.
struct AcceptedRepairState {
    /// The immutable plan authority. `SpecializedContactPlan` is a persistent
    /// value: `append_regular_repair_rows` DERIVES a child and leaves the
    /// parent intact, so "snapshot" is "keep the binding".
    plan: SpecializedContactPlan,
    /// The measured objective of this plan, from its own routed measurement.
    objective: CoverageObjective,          // already exists, lib.rs:1665
    /// The UNCENSORED total uncovered area of this plan. New in v2: the
    /// anti-gaming quantity (§B.5 clause 3). One f64.
    uncensored_uncovered_mm2: f64,
    /// The noncoverage counts of this plan, same measurement point.
    gates: NoncoverageVector,              // new, §C.1
    /// Per-island evidence at this state, for the attribution deltas (§D).
    islands: Vec<IslandState>,
    /// How many steps have COMMITTED. This is what the registered bound
    /// bounds (ruling R6).
    committed: usize,
    /// How many trials have run, committed or not. Evidence and the §F.5
    /// instrument; NOT a bound today.
    trials: usize,
}
```

**The snapshot is nearly free, and v1 mispriced it by one copy.** The reviewer is
right (S5): v1's commit branch `accepted.plan = child_plan` needs `child_plan`
alive beside the copy the run owns *and* beside `accepted.plan` — **three**
`SpecializedContactPlan` values during a trial, against today's two. A plan owns
a whole `Mesh`, the coverage samples and the repair pool
(`specialized.rs:921` onward), and SPEC §8.2 caps retained coverage payload alone at
160 MiB. This design rejects "clone the context per trial" on memory grounds in
§A.3; the same standard applies here and v1 failed it.

**The fix, adopted in v2:** move the child into `prepared_run_for_plan` and
recover it from the lowered run, so only two plans are ever live.

```rust
impl<'a> LoweredSupportRun<'a> {
    /// Recover BOTH the plan and the context from a successful run. Today
    /// `into_context` (runner.rs:3907) drops the plan on the way out; the plan
    /// is reachable through `lowered.structured.braced.rewired.routed`
    /// (runner.rs:937, and the same chain at runner.rs:3932).
    ///
    /// AMENDED (v2.1) — #49 put an `AcceptedContactSet` in front of it
    /// (runner.rs:3029). The plan the controller must carry forward is
    /// `AcceptedContactSet::plan()` (runner.rs:3059): the REDUCED child when
    /// anything was stripped, otherwise the routed plan unchanged. Carrying the
    /// pre-strip plan would re-admit contacts SPEC §6 removed, and the
    /// accepted set's own sealed digest (runner.rs:3052) is what catches it.
    fn into_plan_and_context(self) -> (SpecializedContactPlan, SupportRunContext<'a>);
}
```

The three scalars (`objective`, `uncensored_uncovered_mm2`, `gates`) are tens of
bytes plus one small vector per island.

### A.3 The one hard problem: the context is single-copy

`SupportRunContext` holds the mesh, the topology, the `NativeSurface` grid and
the geometry authority. It is deliberately **moved** into each attempt
(`prepared_run_for_plan(context, plan)`, `runner.rs:4936`) and recycled out of the
lowered run (`LoweredSupportRun::into_context`, `runner.rs:3907`) so that no two
copies of the `NativeSurface` grid are ever on the machine
(`runner.rs:3896–3907` states this as the memory-critical property, and
`coverage_evidence.rs:159–161` records that a previous change was abandoned
because of it).

Consequence: **a stage that fails today consumes the context and the run dies.**
That is why `RepairPlanRefused` exists at all, and it is why #47 kills the Giant
Bat. A transactional controller cannot live with that: a trial that refuses at
Brace must roll back and try the next candidate, which means the context must
come back.

**The seam, corrected in v2 (finding S6).** v1 proposed returning the *receiver*
on the error path — `Result<T, (Self, E)>`. That is wrong, and the reviewer's
reason is checkable: `route_initial_cco_with_limits` destructures `self` on its
first statement (`runner.rs:2661–2665`) and then **mutates** `initial` in place
through the cadence (`initial.forest_mut()`, `runner.rs:2734`). Re-packing a
`PreparedSupportRun` after a mid-cadence failure would hand back a half-grown
forest that nothing but `into_context` may legally touch — a typestate that
type-checks and lies.

v2 returns the **context** instead:

```rust
// runner.rs, six signatures. Error types are unchanged; only the Err payload grows.
fn prepared_run_for_plan<'a>(context: SupportRunContext<'a>, plan: SpecializedContactPlan)
    -> Result<PreparedSupportRun<'a>, (SupportRunContext<'a>, SupportPreparationError)>;

impl<'a> PreparedSupportRun<'a> {
    fn route_initial_cco(self)
        -> Result<RoutedSupportRun<'a>, (SupportRunContext<'a>, SupportRoutingError)>;
}
impl<'a> RoutedSupportRun<'a> {
    fn rewire(self) -> Result<RewiredSupportRun<'a>, (SupportRunContext<'a>, SupportRewireError)>;
}
// … brace, structure, lower the same way.
```

This is the only thing the controller needs, it removes the six per-typestate
`into_context` additions v1 proposed, and it cannot leak an inconsistent
typestate. The controller then writes:

```rust
let routed = match prepared.route_initial_cco() {
    Ok(routed) => routed,
    Err((context, error)) => {
        return TrialOutcome::Rejected {
            context,
            reason: RepairStepOutcome::StageRefusal {
                phase: SupportRunPhase::Routing, detail: format!("{error:?}"),
            },
        };
    }
};
```

No error type changes, no new error vocabulary, `note_stage_failure` is
untouched, and the memory discipline is *strengthened* — the context is never
dropped by a stage, it is always handed back or consumed by success.

**And it is not "mechanical".** v1 used that word and it was wrong. Every `?`
inside these functions becomes a hand-written unwind that reconstitutes the
context from whatever the function had already destructured, and `geometry`,
`pool` and `authority` all borrow `context`, so each early return depends on the
borrow ending at the right point. It is doable, it is compiler-checked, and it is
the single largest piece of hand-written work in this change. Budget it as such.

Rejected alternatives, with their cost:

| alternative | cost | verdict |
| --- | --- | --- |
| Clone the context per trial | one extra `NativeSurface` grid live (`max_grid_bytes` = 64 MiB ceiling) plus the mesh and topology, on every trial | refused; it is the exact thing the split exists to prevent |
| Rebuild the context after a failed trial | a full `prepare_support_context`: topology validation, generic contacts, baseline selection, surface build — the most expensive phase in the run, per rejected trial | refused |
| Borrow the context (`&SupportRunContext`) through the typestates | `prepared_run_for_plan` mutates `context.digests`, so the plan-dependent digests would have to move onto `PreparedSupportRun`; cleaner long-term, but it touches every digest site and the frozen digest chain | deferred; noted as the better end state if the seam ever needs revisiting |

### A.4 Choosing the one winner — the nomination rules

This is the section v2 rewrites most. Three rules replace v1's "keep the
scoring, change the output".

#### Rule N1 — delete the per-patch pick cap

`coverage_closure.rs:666–679` picks candidates until
`picked >= patches.len()` (the break is at `coverage_closure.rs:668`). Under
one-winner transactions exactly one row is
appended per step, so the cap has no work left to do as a *nomination* limit.
What it still does is limit **attention**: it decides how many of a route's rows
are ever considered per round, and it is blind to how large the hole is.

It is deleted. That is the whole of the area-scaling fix on the supply side, and
it is measured: the Rotting Warhorse's two single-patch islands received 8 seats
each out of 377 and 219 because of this rule, and refused with 571 usable seats
untried (`island-out/islands.json`).

#### Rule N2 — score globally (Craig's ruling R3)

A candidate's score becomes:

> **score(r) = the summed area weight of every uncovered sample `s` — on any
> route — with `|seat(r) − point(s)| ≤ R`.**

Today the scoring samples are filtered to the candidate's own
`(island_id, component_id)` key and to that key's patch faces
(`coverage_closure.rs:626–637`). That filter is removed. The tiling discipline is
unchanged: one matrix over (all eligible candidates) × (all uncovered samples),
chunked so that `candidate_block.len() × sample_block.len() ≤
coverage_score_matrix_max_entries` (clamped to 65,536), exactly as today.

**Why global is right.** A contact does not know about island IDs. The physical
quantity §6 names — "marginal uncovered sample area" — is the area that contact
actually covers. Route-local scoring under-values precisely the rows the
2026-08-14 censor-split ruling (#27) exists to keep: a seat owned by a dust route
that physically closes the real hole next door scores the dust only. Under batch
nomination that was harmless, because every worklist key nominated at least one
row regardless of score. Under a single ranking it decides everything.

**The non-regression check is the backstop, not the mechanism** (Craig's ruling
R3). Global scoring changes *which* row is tried first; §B and §C still decide
whether it is kept. A globally-scored row that turns out to unroute a terminal
elsewhere is rejected by §C exactly as a route-locally-scored one would be. So
the ruling costs nothing in safety and buys the #27 behaviour back.

**Cost.** The matrix grows from per-key blocks to one whole-model block. Order of
magnitude on the worst measured models: the Rotting Warhorse's two big routes
alone carry 2,547 uncovered samples and 571 eligible candidates, so a rescore is
~1.5 × 10⁶ squared-distance tests. A single trial's routing charge on the Giant
Toad is 6.0 × 10⁸ seat evaluations. Rescoring is three orders of magnitude below
one pipeline pass; it is not the cost driver and it does not need a new limit.

#### Rule N3 — attention follows area, with exactly one guarantee

**Greedy on a global area score is already area-proportional**, and that is the
point worth stating rather than adding machinery for. The score *is* an area, so
a 13.04 mm² single-patch route's best seats outscore a 0.21 mm² route's by tens
of times, and with N1's cap gone and Craig's ruling R6 leaving rejections
unbounded by the attempt count, a large hole draws consecutive winners until its
marginal areas fall below the next route's. That is the "scale candidate
attention with patch area" the arbitration asks for, and it needs no quota, no
per-route budget, and no new registered number.

What greedy does **not** give is any guarantee for a small route. So add exactly
one, and no more:

> **First-trial guarantee.** Let the *serve set* be every worklist route that
> (a) has not yet had a trial since its own measured uncovered area last
> changed, **and** (b) has at least one eligible candidate whose **global** score
> is at or above the censor, 0.16 mm².
>
> While the serve set is non-empty, the winner is the best candidate of the
> serve-set route whose best candidate scores highest. Ties break on larger
> route uncovered area, then on lower pool row.
>
> When the serve set is empty, the winner is simply the globally
> highest-scoring eligible candidate. Ties break on lower pool row, as today.

**Why the censor is the right threshold, and why it is not a new number.** It is
the registered censor already. A route whose best global score is below 0.16 mm²
cannot, by §4, change the censored objective on its own — so a trial on it is
provably wasted. This is B2's zero-score stop guard (§B.3) generalized from
"covers nothing" to "cannot change the verdict". No constant is added, raised, or
relaxed.

**Checked against rule N2 (global scoring).** The guarantee is what makes the #27
population safe under a single ranking, and global scoring is what makes the
guarantee reachable: a dust route's seat qualifies for the serve set on the *real
hole's* area, not on its own dust. Walked model by model in §B.6.

**Checked against the small mini.** A 30 mm medium mini has few routes, so the
serve set is small and every route that can change the verdict gets its own trial
before the biggest route takes a second. The guarantee costs at most one trial
per worklist route, once per change in that route's area, and then greedy runs
unrestricted.

**The interaction that is not free, stated plainly.** A first-trial that *commits*
consumes committed-step budget. On a model with many worklist routes, the
guarantee can spend the whole registered bound on first trials and never let the
largest hole take its second contact. That is a real limit of the rule and I am
not going to argue it away; it is measured in stage 0 (§G.1) and it is open item
O-3 (§L).

#### Rule N4 — spatial diversity after a rejection (Craig's ruling R13). NEW IN v2.1

**The measured defect this closes.** Scores tie on absolute area. The tie-break
is pool order (`coverage_closure.rs:686`–`693`). **Pool order is spatially
contiguous**, because the repair pool is built by walking the surface. So a run
of equal-scoring candidates is a run of *neighbouring* seats, and the measured
worst case is **63 seats inside a 1.04 mm box** — sixty-three candidates that are
physically the same seat, ranked consecutively.

Under batch nomination this was invisible: they were appended together and the
model was measured once. **Under one-winner it is the dominant cost.** Rejecting
seat *k* and then trying seat *k+1*, 16 µm away, spends a second full
prepare → route → rewire → brace → structure → lower → measure pass to learn
what the first pass already proved. That is the mechanism behind §F.5's number,
and it is a *search-order* defect, not a budget problem — which is exactly why
ruling R10 refuses to answer it with a cap.

> **The rule.** After a trial on row `r` is **rejected**, every remaining
> candidate whose seat lies **inside `r`'s own coverage disc** — that is,
> `|seat(c) − seat(r)| ≤ R`, the same `R = 6.0 mm` the score is computed at — is
> **deprioritized**: it sorts after every candidate outside that disc, at equal
> score. Within each of the two groups the existing order is unchanged (score
> descending, then pool order ascending).
>
> Deprioritization is **not** exclusion. A deprioritized candidate stays
> eligible, stays in the pool, and is tried once every candidate outside the disc
> has been tried. Completeness under R10 is preserved exactly: the loop still
> ends only at pool exhaustion or the zero-score guard.
>
> Discs accumulate. A candidate inside *any* rejected seat's disc is
> deprioritized; the sort key is the **count** of rejected discs it falls inside,
> ascending, applied as the **first tie-break, after score**
> (**AMENDED 2026-08-20 — see §N.1**; this sentence originally read "applied
> before score", which contradicted the sentence above it and was measured doing
> harm). That makes "how many times has this neighbourhood already failed" the
> question that decides everything area has left tied, and keeps the rule total,
> deterministic and tie-free.

**No new constant.** `R` is `COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM`, already
registered, already the radius the score is computed at, and already the radius
the pre-write gate re-measures at. Nothing is registered, raised or relaxed
(ruling R11). **Derived from existing geometry**, as R13 requires: the disc is
the rejected seat's own coverage disc, not a tuning knob.

**Why the coverage disc is the right radius, physically.** Two seats within `R`
of each other cover overlapping sample sets by construction. If seat `r`'s trial
was rejected because its arm could not route, or because it worsened a
noncoverage count, a seat inside `r`'s disc is proposing a near-identical arm to
a near-identical set of samples. The disc is the exact boundary of "this is the
same proposal". Outside it, the candidate is reaching samples `r` could not, and
it deserves the pass.

**What it costs, honestly.** Two things, and neither is free:

- **It can delay a good seat.** A candidate 5 mm from a rejected seat may route
  perfectly and is now tried later than it would have been. It is still tried.
  Under R10 there is no budget for it to fall off the end of, which is the
  ruling's own consistency argument: diversity may reorder freely precisely
  because nothing truncates the order.
- **It is O(rejections) per candidate per rerank.** The disc test is one squared
  distance per (candidate, rejected seat) pair. On the Warhorse's 6,777 eligible
  rows against a few hundred rejections that is ~10⁶ squared distances per
  rerank, three orders of magnitude below one pipeline pass — the same accounting
  §A.4 rule N2 makes for global scoring. Compute it incrementally: one new disc
  per rejection, so each rerank adds one pass over the remaining candidates.

**The proof obligation.** One test that rejects a seat and asserts the next
winner lies outside that seat's coverage disc while an equal-scoring neighbour
inside it exists; and one that drains a fully-enclosed neighbourhood and asserts
every deprioritized candidate is eventually tried, so completeness is pinned and
not assumed.

#### What is unchanged

`RankedRepairCandidates` replaces "one row per patch, deduplicated":

```rust
/// Ranked candidates for one repair round, across every route on the worklist.
/// Score is GLOBAL marginal uncovered sample area in mm² (ruling R3), so the
/// ranking is comparable across routes; ties break on ascending pool order.
pub(crate) struct RankedRepairCandidates {
    ranked: Vec<RankedRepairRow>,          // descending score, then pool order
    per_key: BTreeMap<(u32, u32), KeyNomination>,   // unchanged evidence
}
pub(crate) struct RankedRepairRow {
    id: RepairRowId,
    key: (u32, u32),           // owner island_id, component_id
    seat_mm: [f64; 3],
    /// GLOBAL marginal uncovered area within R of this seat.
    marginal_area_mm2: f64,
    /// The routes whose uncovered samples contributed to the score, with each
    /// route's share. This replaces v1's `scored_for: PatchKey` (finding S11:
    /// a per-PATCH attribution is not producible without scoring per patch,
    /// which multiplies the scoring work by the patch count). Per ROUTE is
    /// free — it is one extra accumulator per route in the same matrix pass.
    scored_from: Vec<(u32, u32, f64)>,
}
```

The winner is `ranked[0]` after applying rule N3 and after removing occupied
rows, ledgered rows, and rows whose seat point is already claimed (all three
exclusions exist today and are kept verbatim, including the physical-seat
exclusion that mirrors `DuplicateChildPhysicalSeat`).

**Reranking. AMENDED (v2.1) — rule N4 changes this, and v2's claim is now
half-true.** SPEC §6 says candidates are reranked after every accepted *or*
rejected attempt.

- **Scores** after a rejection are still provably unchanged. The child plan was
  dropped, so the uncovered sample set and every candidate seat are bit-identical
  to before the trial. **No rescore is needed after a rejection**, and that half
  of v2's argument stands.
- **Order** after a rejection now **does** change, because rule N4 adds the
  rejected seat's disc and re-sorts on the disc count. That is the rerank SPEC §6
  asks for, and v2's "provably a no-op" is withdrawn: it was true of v2's rules
  and is false of v2.1's.

So the controller **re-sorts** after every rejection (cheap: one disc pass) and
**rescores** only after a commit (expensive: the whole matrix). The proof
obligation moves with the claim: one test that asserts the *scores* are
bit-identical across a rejection, and one that asserts the *order* moved exactly
as N4 says — an equal-scoring in-disc candidate demoted below an equal-scoring
out-of-disc one.

### A.5 Rollback

**Rollback of the plan is a non-event, by construction.** Because the plan is a
persistent value and the child is a *derived* value, rolling back is "do not
rebind `plan`". There is no undo log to replay and nothing to rebuild.

```rust
match trial {
    TrialOutcome::Committed { plan, context: returned, objective, uncensored, gates, islands } => {
        accepted.plan = plan;                // the ONLY line that mutates state
        accepted.objective = objective;
        accepted.uncensored_uncovered_mm2 = uncensored;
        accepted.gates = gates;
        accepted.islands = islands;
        accepted.committed += 1;
        if objective.failing_patch_count == 0 { return Ok(authority(...)); }
        context = returned;
    }
    TrialOutcome::Rejected { context: returned, reason } => {
        context = restore_digests(returned, &accepted);   // NEW IN v2, see below
        ledger.push(RepairLedgerRow { trial, row: winner, key, reason });
        // `accepted` is otherwise untouched: that IS the restore.
    }
}
```

**What v1 got wrong: the context is not restored (finding S4).**
`prepared_run_for_plan` mutates five fields on the context before returning
(**`runner.rs:4960–4964`**: `digests.plan`, `.loads`, `.initial_forest`,
`.contact_attestation`, `.prepared` — re-verified at `a8feff4`; the five fields
and their order are unchanged, only the line numbers moved). A trial rejected at Routing, Rewire, Brace,
Structure or Lowering hands back a context stamped with the **rejected child's**
digests. Today that self-heals only because the next call is always
`prepared_run_for_plan`. v1's claim "`accepted` is untouched: that IS the
restore" is true of the plan and **false of the context**.

v2 restores them explicitly rather than relying on a write-before-read
invariant, because under one-winner the controller assembles refusal evidence
*between* trials, and evidence read from a stale digest would describe a plan
that does not exist:

```rust
/// The five digest fields `prepared_run_for_plan` stamps. Captured from the
/// accepted state's own preparation and re-stamped on every rejection, so a
/// rejected trial can never leave its digests visible.
struct PreparedDigests { plan: u64, loads: u64, initial_forest: u64,
                         contact_attestation: u64, prepared: u64 }
```

with one test that rejects a trial at each of the five stages and asserts the
context's digests equal the accepted state's afterwards.

**What is genuinely clean, and v2 says so rather than leaving it to the reader:**

- Quarantine state lives in `InitialContactBuild` and is rebuilt per trial
  (`runner.rs:4962`, `initial.post_quarantine_forest_digest()`).
- Seat claims are derived from the plan's own contact list
  (`specialized.rs:1352` `append_regular_repair_rows`, which reads occupancy once
  through `occupied_repair_rows`; `coverage_closure.rs:554–566` seeds
  `claimed_points` from the plan's existing contacts), so they roll back with the
  plan.
- **NEW AT `a8feff4`, and it rolls back too:** the `AcceptedContactSet`
  (`runner.rs:3029`) — the #49 strip, its removal ledger, its ownerless faces and
  its sealed digest — is derived inside the routed run, so a dropped child plan
  drops its accepted set with it. Nothing extra is needed.
- `failed: BTreeSet<RepairRowId>` deliberately survives a rollback, which SPEC §6
  requires ("exact failed rows are excluded").

**The one exception that does not roll back: the census.** A rejected trial's
routing charge and its `note_stage_failure` row stay recorded. Under one-winner,
stage refusals on trials become routine rather than fatal, so the census gains
rows describing plans that were discarded. That is defensible as evidence — the
work really was done — but it means the census is a record of *work*, not of the
*accepted plan*, and §G.2's re-pinning must be read that way.

Cost comparison, as asked:

| mechanism | cost per rollback | why not chosen |
| --- | --- | --- |
| **keep-the-parent (chosen)** | zero for the plan; five `u64` writes for the digests. Two `SpecializedContactPlan` values alive during a trial, which today's `plan.clone()` per attempt already pays — provided `into_plan_and_context` lands (§A.2). | — |
| undo log | an inverse edit through `remove_demand_free_regular`, which calls `derive_child`, which replays `finalize_specialized_load_partition` over the whole contact set — the same cost as the forward edit, plus a new correctness burden (contact-id holes are observable). | rejected |
| rebuild from the parent + the committed row list | one `append_regular_repair_rows` replay per rollback, growing with the number of committed steps. | rejected |

**And the final authority never needs a rebuild.** The loop can only return
`Ok` from a state whose measurement showed zero failing patches, and that state
is always the *current* lowered run (attempt 0's, or the committing step's). A
run that stops for any other reason returns a refusal, which needs no sealed
authority. So there is no "restore the accepted lowered run at the end" problem,
and no extra pipeline pass is ever paid.

### A.6 What commits, and the ledger

A step commits when, and only when, all three clauses of §B.5 pass on the
trial's own measurement. Everything else rolls back and is ledgered:

```rust
enum RepairStepOutcome {
    Committed,
    /// Measured, but the censored objective did not strictly improve.
    NoImprovement { before: CoverageObjective, after: CoverageObjective },
    /// Measured, and the UNCENSORED total uncovered area did not strictly
    /// fall — the anti-gaming rejection (§B.5 clause 3). NEW IN v2.
    NoAreaCovered { before_mm2: f64, after_mm2: f64 },
    /// Measured, and a noncoverage count got worse.
    Regression { component: NoncoverageComponent, before: u64, after: u64 },
    /// The trial could not be built at all. Today this kills the model
    /// (`RepairPlanRefused`); here it rolls back. Task #47 lands here.
    StageRefusal { phase: SupportRunPhase, detail: String },
    /// `append_regular_repair_rows` refused the row (occupied, duplicate seat,
    /// unknown row). Today this kills the model too.
    AppendRefused { detail: String },
    /// The routed measurement itself refused on the trial.
    MeasurementRefused { detail: String },
}
```

Two notes on `StageRefusal`. First, it does **not** apply to attempt 0: attempt 0
runs the model's own plan and its stage failures stay
`SupportRunError::Failed { phase, detail }`, exactly as
`repair_plan_or_stage_failure` decides today and as
`a_stage_error_is_not_discarded_at_attempt_zero` (`runner.rs:8970`) pins.
Second, `CoverageClosureReason::RepairPlanRefused` survives only as a
*terminal* reason, for the case where every candidate refused a stage; the
common case becomes one ledger row and the next candidate.

### A.7 The loop

```
measure attempt 0                                  (unchanged: one full pass)
  ├─ no failing patch → Ok(authority)              (unchanged: zero-cost path)
  └─ failing patches → enter repair

repeat until stop:
  rank candidates globally over the worklist routes      (§A.4 N1+N2)
    · after a COMMIT   : rescore the whole matrix
    · after a REJECTION: scores unchanged; re-sort on the rejected discs (N4)
  if ranked is empty                → stop: NothingToTry
  apply the first-trial guarantee to choose the route    (§A.4 N3)
  winner = the chosen route's best candidate             (N4 order applies)
  if BEST LIVE candidate's score == 0.0 → stop: NoImprovingRepair (§B.3, guard B2)
        AMENDED 2026-08-20 (§N.1): this line read "winner.marginal_area_mm2",
        which contradicts §B.3's own wording and was measured abandoning a live
        13.68 mm² candidate on `make_overhang_table`.
  if committed == commit_budget      → stop: CommitBudgetExhausted (rulings R6+R11)
  trial = transact(winner)                               (§A.1, §A.3)
  if all three accept clauses hold (§B.5):
        commit; if failing_patch_count == 0 → Ok(authority)
  else: roll back, restore digests, ledger the row, exclude it,   (§A.5)
        add its coverage disc to the diversity set                (§A.4 N4)
  if every candidate on the ranking has been ledgered → stop: NoImprovingRepair

stop → Err(CoverageClosure { reason, per-route outcomes, trial stream, ledger,
                             trials, wall_time })                  (§A.8)
```

`failed: BTreeSet<RepairRowId>` already exists in the loop (**`runner.rs:1179`**)
and already carries the right meaning; today it is filled with *appended* rows
because nothing is ever retracted. Here it is filled with *ledgered* rows, which
is what its name always claimed.

**Note what is NOT in the stop list, and AMENDED (v2.1) why it never will be: a
trial cap.** Under rulings R6 and **R10** the registered bound counts commits
only, and rejections are bounded by pool exhaustion plus the zero-score guard.
v2 left Craig's override open at §F.5 and recommended a trial ceiling as O-1;
**R10 rules against it and O-1 is withdrawn.** A counter must never decide an
outcome. What bounds the cost instead is rule N4 (R13) attacking the measured
mechanism, and §A.8 making the residual cost loud rather than hidden.

### A.8 Loud reporting — the price of having no cap (ruling R10). NEW IN v2.1

R10 removes the cap and replaces it with visibility. That trade only holds if the
visibility is real, so it is specified here rather than left to the implementer.

> **Every run reports, on every path — sealed or refused, per model:**
>
> | field | meaning |
> | --- | --- |
> | `trials` | every transaction attempted, committed or not |
> | `commits` | how many became the accepted plan |
> | `rejections_by_class` | the `RepairStepOutcome` histogram (§A.6) |
> | `wall_time_total` | the repair loop's own elapsed wall time |
> | `wall_time_per_trial` | minimum / median / maximum, so one pathological trial is visible against many cheap ones |
> | `candidates_remaining` | eligible rows never tried when the loop stopped |
> | `diversity_demotions` | how many candidates rule N4 deprioritized, and how many were later tried anyway — **this is the instrument that says whether R13 worked** |
>
> It goes to **three** places, because one is not enough: the run log at
> INFO (an operator watching a long run must see it move), the FU-57 coverage
> sidecar (the machine-readable record), and the run report's per-model line (the
> owner's summary).

**These are counters that REPORT; none of them DECIDES.** No branch in §A.7 reads
any of them except `committed` against the registered bound, which is R6's and
R11's own rule. That distinction is the whole of the physics-first doctrine here:
a number that steers is a cap, a number that is printed is evidence.

**Wall time is evidence, never a bound.** It is measured and printed. Nothing
compares it to anything. A machine that runs the corpus slowly must produce the
same verdicts as one that runs it fast, or the verdicts were never about the
model.

---

## B. Strict improvement

### B.1 The measured quantity

The committed step must strictly improve the **censored** coverage objective,
lexicographically, in the spec's own order:

1. `failing_uncovered_area_mm2` — total area of failing patches, summed;
2. `max_patch_area_mm2` — the largest single failing patch;
3. `failing_patch_count`;
4. `worst_gap_mm` — the worst nearest-routed-contact distance over samples on
   failing islands, with `+inf` for "no routed contact at all".

**Censored, and the spec says so in the word "failing".** §4 defines a failing
patch as one above the censor or touching an ambiguous cut; §6 clause 1 names
"total failing area", "maximum failing-patch area", "failing-patch count". An
uncensored objective would make sub-censor dust able to justify committing a
contact, which contradicts §4's "diagnostic only". A censored objective is also
what makes the model's stopping condition and its improvement condition the same
quantity, so the loop cannot improve forever without approaching a verdict.

**Uncensored is retained for work selection**, unchanged (task #27's ruling, and
`coverage_patch_needs_repair`). The two are not in tension: the worklist decides
*which routes may nominate a candidate*, the objective decides *whether the
committed contact earned its place*. A candidate nominated for dust that also
closes the real hole beside it improves the censored objective and commits — the
`closure_repairs_censored_dust_because_that_repair_closes_a_real_hole` fixture
is exactly that case, and it still seals. A candidate that only tidies dust does
not improve the censored objective and is rolled back.

**And uncensored is now retained for a second reason: §B.5 clause 3.**

### B.2 It is already implemented, and what it actually costs to reuse

- The type: `plinth_support::CoverageObjective` (`lib.rs:1665`) has exactly
  those four fields.
- The order: `specialized::coverage_key` (`specialized.rs:3783`) is exactly that
  lexicographic tuple, with `OrderedF64`/`numeric_cmp` handling infinities.
- The producer: `assess_coverage_with_mode` (`lib.rs:1905`) already computes the
  objective from the routed nearest-distance array, censoring at
  `area_floor_mm2` with `patch_area > area_floor_mm2 || patch_ambiguous`
  (`lib.rs:2014`) — bit-identical to `coverage_patch_blocks_verdict`.
- And **`measure_coverage` already computes this objective on every attempt and
  throws it away.** It keeps `assessment.patches` and
  `assessment.uncovered_sample_indices` and drops `assessment.objective`
  (`runner.rs:2051` onward).

**v1 called this "zero cost" and that was overstated (finding S12).**
`coverage_key` and `gate_not_worse` take `FrozenSpecializedGateVector`
(`specialized.rs:3423, 3783`), not `CoverageObjective` (`lib.rs:1665`), and both
are private to `specialized.rs`. Reuse needs a `From<CoverageObjective>`
conversion plus `pub(crate)` on both. That is small, and it is not nothing; say
it once and price it.

What is genuinely free is the measurement side:

```rust
impl CoverageMeasurement {
    /// The censored A4 objective this measurement already computed.
    #[must_use] pub const fn objective(&self) -> CoverageObjective { self.objective }
    /// The UNCENSORED total uncovered area, for §B.5 clause 3. Also already
    /// computed: it is the sum of the same assessment's per-patch areas with
    /// no censor applied.
    #[must_use] pub const fn uncensored_uncovered_mm2(&self) -> f64 { self.uncensored_mm2 }
}
```

Zero new geometry work, zero new allocation, no new charge.

### B.3 The zero-score stop guard (blocking change B2)

`nominate_repair_rows`' picking loop has no score guard at all, and
`eligible_repair_rows`' own doc (`specialized.rs:1000`, the function at `:1001`) says so deliberately:
"zero-score candidates remain eligible". Under batch nomination that was harmless
padding — a row that covers nothing is a wasted contact and nothing more. Under
one-winner it is not padding, it is the budget: `ranked[0]` can have
`marginal_area_mm2 == 0.0`, and the controller would then spend a **full**
prepare → route → rewire → brace → structure → lower → measure pass to discover
that a contact covering nothing improves nothing.

> **Guard.** Stop with `NoImprovingRepair` when the best eligible candidate's
> **global** score is 0.00 mm². Mark every route whose eligible rows all score
> 0.00 mm² with the measured cause `NoCandidateCoversAnything` (§E.1).

This is in the loop at §A.7, before the transaction, not after it. It is also the
special case of rule N3's serve-set threshold: a route whose best global score is
zero is below the censor, so it never enters the serve set either.

### B.4 The accept test, in one place

```rust
/// SPEC §6 clause 1 + clause 2, plus the v2 anti-gaming clause.
/// Paired with `specialized::gate_not_worse` by a drift test, in the same
/// shape `plinth-verify/tests/coverage_gate.rs` pairs the verdict predicate.
///
/// Note the drift test pairs a STRICT rule (`<`, clause 1) against a
/// NON-STRICT one (`<=`, clause 2). What it asserts is that the two functions
/// agree on the ORDER, not on the strictness: for every pair of vectors,
/// `coverage_key(a) < coverage_key(b)` iff `a` sorts before `b` under
/// `gate_not_worse`'s componentwise order. Strictness is this design's, not
/// `gate_not_worse`'s, and the test says so.
fn repair_step_accepts(after: &RepairGateVector, before: &RepairGateVector) -> bool {
    coverage_key(after.coverage) < coverage_key(before.coverage)          // clause 1, STRICT
        && after.noncoverage.no_worse_than(&before.noncoverage)           // clause 2
        && after.uncensored_uncovered_mm2 < before.uncensored_uncovered_mm2 // clause 3
}
```

### B.5 Clause 3 — the anti-gaming guard (A7)

**The hole.** §4's censor is per patch. §6's objective sums over *failing*
patches. So a repair that splits one 0.30 mm² failing patch into two 0.15 mm²
censored patches drops total failing area to zero, drops the maximum
failing-patch area to zero, drops the failing-patch count to zero — and the model
**seals** — without covering one extra square millimetre. That is "converting
residual uncovered area into success" by a route §6 does not name, it is exactly
the move a greedy lexicographic search finds first, and one-winner makes it
reachable in a single step where batch nomination would have masked it inside a
48-row append.

**The exact rule.**

> **A repair step is accepted only if, in addition to clauses 1 and 2:**
>
> **Clause 3.** `uncensored_uncovered_area_total(after) < uncensored_uncovered_area_total(before)`,
> where `uncensored_uncovered_area_total` is the sum of the area weights of every
> uncovered sample in the measurement, over every route, with no censor applied
> and no patch grouping applied.

**Why this is Goodhart-robust, stated as a property and not as a hope.** The
quantity in clause 3 has **no free parameter a repair can move**:

- The sample set and its area weights are built once at attempt 0 and are frozen
  for the run. A repair changes *which* samples are uncovered; it can never
  change how many samples exist, where they are, or what they weigh.
- Patch boundaries do not enter it. Splitting, merging or re-labelling patches
  leaves the sum bit-identical, because the sum never groups.
- The censor does not enter it. Pushing area below 0.16 mm² leaves the sum
  bit-identical.
- Island and component identity do not enter it.

So the **only** way to reduce it is to move at least one sample from uncovered to
covered. That is coverage, by definition. The metric and the goal are the same
thing, which is precisely what a Goodhart-robust metric means.

**Why strict `<` on an f64 sum is safe here, with no epsilon and no new
constant.** The sum is over a subset of a fixed multiset of area weights. Any
real coverage change removes at least one whole weight from the subset, and the
smallest sample weight on any corpus model is many orders of magnitude above f64
summation noise at these magnitudes. There is no near-tie to adjudicate: the
difference is either zero (nothing was covered) or at least one sample's area.
Compare with `total_cmp`, the same discipline `coverage_key` already uses.

**What clause 3 does not reject.** Every legitimate repair reduces it, so clause
3 never costs a good step:

- A contact that closes a real hole: covers samples → sum falls → passes.
- A contact that closes dust and a real hole (the #27 fixture): passes clauses 1
  and 3 both.
- A contact that only tidies dust: passes clause 3, **fails clause 1** — rejected
  for the same reason as in v1, unchanged.
- A contact that covers nothing: fails clause 3 and clause 1 both, and never gets
  a trial at all because of §B.3's guard.
- A contact that covers a little but unroutes a terminal elsewhere: rejected by
  clause 2, not by clause 3.

**One scope note.** Clause 3 governs *committed repair steps*. It does not and
must not govern attempt 0: a model whose entire residual is sub-censor dust seals
at attempt 0 with uncovered area on the books, and that is the censor's own
intent under #27. The anti-gaming concern is only about a repair step
*manufacturing* that state, and clause 3 blocks that.

**Recommended as a spec amendment, not just a house rule.** §4's per-patch censor
and §6's summed objective together permit the split; the controller closes it,
but the next reader of the spec would not know why. One sentence in §6 —
"a committed step must also strictly reduce total uncovered sample area,
uncensored" — makes it contract instead of house rule. Open item O-6.

### B.6 Walking the #27 population through the revised rules (blocking change B3)

The three models the censor-split ruling exists to protect are named in the
code's own comment (`coverage_closure.rs:69–70` and `:722–723`,
`runner.rs:1289–1290`) and in
`CURRENT-STATUS.md:131–132`: they **refused with REAL, above-censor uncovered
area** when the worklist was censored — `Skulker Zombie` 0.35 mm², `Tavey Badger`
0.21 mm², `zombie-3-a` 12.65 mm². The mechanism of that regression: the row that
would have covered the real hole was owned by a sub-censor *dust* route, and
censoring the worklist stopped that route nominating.

Under v2's rules, model by model:

**`Skulker Zombie`, 0.35 mm² of real above-censor hole.** The row that closes it
is owned by a dust route. Under v1's route-local scoring that row would have
scored the dust only — a fraction of a censor — and sat below every row on every
larger route in a single global ranking, so with a bounded budget it would never
have been reached. That is the regression reproduced by a different mechanism,
and it is what B3 caught. Under **rule N2** the same row is scored on every
uncovered sample within 6.0 mm of its seat, which includes the 0.35 mm² hole, so
it scores at least 0.35 mm² and ranks on the hole's own area. Under **rule N3**
its owning route clears the 0.16 mm² serve-set threshold on that same global
score and is guaranteed its first trial before any route takes a second. The
model is protected twice over, and neither protection depends on the row's owner
being the hole's route.

**`Tavey Badger`, 0.21 mm².** Identical shape, and the closest to the line: 0.21
mm² is 1.3× the censor. Global scoring puts the closing row at ≥ 0.21 mm², which
clears the serve-set threshold, so it gets its guaranteed first trial. Two honest
caveats. (1) The margin is thin: if a partial repair ever leaves this route's
residual below 0.16 mm², it drops out of the censored objective entirely and
correctly stops asking for a contact — that is the censor working, not a
regression, but the fixture must be read that way. (2) `Tavey Badger` **closes
and emits today** (`CURRENT-STATUS.md:908`), so it is a *non-regression* case,
not a repair case: stage 0 must prove it still emits and still emits with a
contact set that is a subset of today's.

**`zombie-3-a`, 12.65 mm².** This is not a dust case and it should not be filed
as one. 12.65 mm² is 79× the censor — it is a large hole, in the same class as
the Rotting Warhorse's 13.04 mm² island 74. What made it regress under a censored
worklist is presumably that its closing rows were owned by sub-censor sibling
patches; what makes it hard under **any** one-winner controller is different and
larger: **one contact will not close 12.65 mm², and Craig's ruling R6 bounds
committed steps at 8.** Under rules N1+N2 it draws consecutive winners (its rows
score highest, and the per-patch pick cap that starved the Rotting Warhorse is
gone), so attention is no longer the problem. The commit budget may well be.

> **Named stage-0 obligation.** Measure, on `Skulker Zombie`, `Tavey Badger` and
> `zombie-3-a`: (a) the per-patch area breakdown of each route on the worklist,
> so the dust-versus-hole shape is recorded rather than presumed; (b) how many
> individually-improving contacts each model needs to close. If (b) exceeds 8 on
> any of them, the commit bound must be decided before stage 3 lands, not after.
> This is the FU-82 lesson — size the prize before building the instrument —
> applied to the instrument's own risk.
>
> **DISCHARGED. AMENDED (v2.1) — measured, and part (b) is comfortable.** All
> three seal today. Improving rounds: `Skulker Zombie` **1** (appending 36
> contacts across 3 rounds, 2 of them flat), `Tavey Badger` **1** (19 contacts,
> 1 round, 0 flat), `zombie-3-a` **3** (29 contacts across 6 rounds, 3 flat).
> **None exceeds the bound of 8**, and v2's worry that `zombie-3-a`'s 12.65 mm²
> "will not close in one contact" is answered — it does not need to; it closes in
> three improving steps.
>
> **What the measurement found instead is worse and is O-7, not O-2.** Two of the
> three cross flat rounds to get there (`Skulker Zombie` 2, `zombie-3-a` 3), so
> the #27 population is inside R12's population, and §B.9's investigation covers
> them. **Part (a), the per-patch breakdown, is still owed** — stage 0 recorded
> per-round totals, not per-patch shape — and it is folded into §B.9's code walk
> rather than left as its own errand. **And every number here was measured at
> `06cf21b`;** the tip arc and the reach rule have moved contact sets since, so
> the counts must be re-read at `a8feff4` before they are pinned (§G.1).

**And the non-regression check is the backstop** (ruling R3). Global scoring only
changes what is tried first. If a globally-scored winner turns out to help one
route and hurt another, §C rejects it and the ledger names the component that got
worse. There is no path by which global scoring commits something route-local
scoring would have refused.

### B.7 Honest termination

The loop stops, without a model-killing error, in these distinguishable ways:

```rust
enum CoverageClosureReason {
    /// No route on the worklist had a single eligible repair-pool row. The
    /// whole-model version of today's `NoEligibleRepairRows`, kept only for
    /// the case where the ranking is empty at the FIRST step.
    NothingToTry,
    /// Every candidate was tried and rolled back, OR the best remaining
    /// candidate's global score is 0.00 mm² (§B.3). The honest end of a
    /// greedy search.
    NoImprovingRepair { candidates_tried: usize, best_remaining_score_mm2: f64 },
    /// The registered bound on COMMITTED steps stopped the search with
    /// candidates left (ruling R6).
    CommitBudgetExhausted { committed: usize, candidates_remaining: usize },
    /// The routed measurement itself refused (unchanged).
    MeasurementRefused(String),
}
```

`RepairPlanRefused` narrows to "every remaining candidate refused a stage", and
`AttemptsExhausted` retires (its meaning is now `CommitBudgetExhausted`).

**None of these is an error about a route.** Which routes remain, and why each
one remains, is per-route evidence (§E.1) — that is the whole of group B's
dissolution, and in v2 those reasons are measured defect classes rather than
distance claims.

### B.8 The honest limitation of a greedy hill-climb

Strict per-step improvement is a hill-climb and cannot cross a plateau: if
closing one route needs two contacts and neither alone improves the objective,
A4 as written cannot get there. That is the spec's rule, not a choice this
design makes, and I am not inventing an exception to it. What the design does
instead is *record* it: a ledger full of `NoImprovement` rows on one route, with
all their global marginal areas well above zero, is a legible statement that the
route needs a pair, and it is the evidence a future spec amendment would be
argued from.

**AMENDED (v2.1) — O-7 is no longer an armchair caveat. It has a measured victim,
and Craig has ruled on how to resolve it. §B.9 is that mandate.**

### B.9 The O-7 mandate — investigate the flat rounds IN CODE before implementing §B. NEW IN v2.1

> **DISCHARGED 2026-08-20, before §B was implemented. The answer is §B-O7 below,
> and it is part of this design.** Hypothesis 2 holds: the flat rounds' contacts
> were condemned at **quarantine** and never routed, so no measured quantity
> could move and none did. Clause 1 stands unchanged. §B.9 is kept verbatim
> because §B-O7 corrects one of its own predictions and the reader needs both.

**What stage 0 measured, at `06cf21b`.** Of the 12 sealing models that repair at
all, **9 sealed only by passing through at least one round that improved nothing
measurable**. The worst, and the named victim:

> **Crawler Zombie** sits at exactly **16.625 mm² failing area and 16.825 mm²
> uncensored for five consecutive rounds** while 50 contacts are appended, and
> then the sixth round's 9 contacts take it from 16.625 mm² to **0**. Nothing in
> rounds 2–6 improved either quantity. **A strictly-improving greedy controller
> stops at round 2.** This model emits today.

Same shape, weaker, on Morel Mudskipper (4 flat rounds), Skeleton_2 (4), Tavey
(4), zombie-3-a (3), Skulker Zombie (2). This is **the single largest emission
risk in the change** — larger than the commit bound's value, which stage 0 found
comfortable at 8 (maximum improving rounds on any sealing model: 3).

**Craig's ruling R12, in full, because it is a mandate and not a decision:**

> Investigate in code **WHY** the flat rounds seal, before implementing §B.
>
> 1. **If a finer measured quantity moves during them** — something real changes
>    that the four-component censored objective is too coarse to see — then
>    define the strict-improvement objective **at that granularity**, using
>    **existing quantities only**. No new registered constant (R11).
> 2. **If the flat rounds were dead weight** — an artefact of per-patch
>    nomination throttling, where the batch appended rows that could not help
>    because the throttle would not let it append the rows that could — then
>    **clause 1 stands unchanged**, and the global scoring of rule N2 must
>    surface the sealing rows **immediately**, in the first trials. The
>    obligation is to **SHOW WHICH ROWS** — by `RepairRowId`, with their global
>    scores — not to assert that greedy would have found them.
>
> **Crawler Zombie must still emit.** That is proven in Prove, on the model, not
> argued in the design.
>
> **The resolution lands as design addendum §B-O7 during Core**, written against
> what the code says, and it is part of this design once written.

**The two hypotheses are distinguishable, and here is how.** They make opposite
predictions about the same telemetry:

| | hypothesis 1 (finer quantity moves) | hypothesis 2 (throttle dead weight) |
| --- | --- | --- |
| uncensored total uncovered area across a flat round | **falls**, below the objective's resolution | **flat**, bit-identical |
| uncovered sample **count** across a flat round | **falls** | **unchanged** |
| worst gap on some route | **falls** | unchanged |
| the rows appended in the flat round | at least one covers samples | none covers a sample |
| the rows that finally seal | were **not eligible or not nominated** earlier | were **eligible and unnominated**, throttled out |

Stage 0 already recorded the uncensored total per round, and on Crawler Zombie it
is **flat to the millimetre across all five rounds (16.825 mm²)** — which points
at hypothesis 2. **That is a pointer, not the answer**, because stage 0 did not
record the sample count, the per-route worst gaps, or which rows were eligible
but unnominated. Those three are what the Core investigation must read.

**Where to read them, named so the investigation is a code walk and not a
re-measurement:**

- the throttle itself — `coverage_closure.rs:666–679`, `picked >= patches.len()`,
  and the per-key `KeyNomination` evidence it fills at `coverage_closure.rs:460`;
- eligibility versus nomination — `SpecializedContactPlan::eligible_repair_rows`
  (`specialized.rs:1001`) against `Nominations::rows`, per key, per round;
- the finer quantities — `assess_coverage_with_mode` (`lib.rs:1905`) already
  computes the per-patch areas, the uncovered sample indices and the per-patch
  worst gaps; **all three exist and are discarded**, exactly as §B.2 found for
  the objective.

**The design's own position, stated so the investigation can falsify it.** I
expect hypothesis 2, and if it holds, clause 1 stands and rules N1+N2 are the
whole fix — the sealing rows on Crawler Zombie should be eligible from round 2
and should carry global scores that put them at the top of the first ranking.
**If they do not**, hypothesis 1 is live and the objective must be refined at
whichever existing quantity moves; the candidate with the strongest claim is the
**uncensored uncovered sample area** the anti-gaming clause 3 already carries
(§B.5), promoted from a veto to a lexicographic tie-break under the four censored
components. **That would be a change to clause 1**, which is a spec reading, and
it goes to Craig before it is implemented — it is not the implementer's to take.

**What is NOT permitted as a resolution**, so it cannot arrive by drift: a
tolerance on "improves", a permitted number of flat steps, a look-ahead depth, or
any other new registered value (R11). If neither hypothesis resolves it, the
answer is a ruling from Craig, not a knob.

---

## §B-O7. The flat-round resolution — measured in code and on the model (ruling R12)

**Status: RESOLVED. Hypothesis 2 (dead weight) is confirmed. Clause 1 stands
unchanged. No new registered constant, no tolerance, no permitted number of flat
steps, no look-ahead.** This section is the design addendum ruling R12 orders,
written during Core against what the code says and what the model measured, and
it is part of this design.

### B-O7.1 What was measured, and how

`Crawler Zombie` — R12's named victim — was re-run at this design's own base,
`develop` @ `a8feff4`, with a throwaway trace and nothing else changed. The
patch is kept at
`…/scratchpad/a4-build-out/o7-instrumentation-v2.patch`; it was never committed
and the tree it was measured from is byte-identical to `a8feff4` plus this
document.

- Binary: `target/gate/plinth`, gate profile (release semantics), built
  2026-08-20 07:41, **after** the lane's tip commit (07:29). Provenance checked
  on **both** axes required by this lane: mtime past the last commit, **and** the
  unique marker string `PLINTH-O7-TRACE-MARKER-a4o7-2026-08-20` present in the
  binary and in the trace's every row.
- Input: `Crawler Zombie.stl`, copied read-only out of `minis-to-process/`.
- Command: `plinth prepare --output saturn4-ultra --no-cache`, one model, one
  process. Outcome: `emitted=1` — the model still seals at this base, so the
  plateau reproduces on live geometry rather than from a stale record.
- Raw trace and the consolidated reading:
  `…/scratchpad/a4-build-out/o7/crawler2.jsonl`, `…/o7/o7-evidence.txt`.

Two things were recorded per attempt that stage 0 did not have: the **CCO
transcript's disposition counts** (routed / quarantined / unrouted-no-winner),
and, for **every row in the frozen repair pool**, its eligibility, its
route-local score as today's nomination computes it, and its **global** score
under rule N2.

### B-O7.2 The measurement

| attempt | plan contacts | **routed** | quarantined | unrouted-no-winner | blocking patches | failing area mm² | uncovered samples |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 75 | 58 | 10 | 7 | 3 | 17.019 970 362 675 23 | 2 658 |
| 1 | 89 | 63 | 19 | 7 | 2 | 16.625 364 190 080 948 | 2 615 |
| 2 | 99 | **63** | 29 | 7 | 2 | 16.625 364 190 080 948 | 2 615 |
| 3 | 109 | **63** | 38 | 8 | 2 | 16.625 364 190 080 948 | 2 615 |
| 4 | 119 | **63** | 47 | 9 | 2 | 16.625 364 190 080 948 | 2 615 |
| 5 | 129 | **63** | 55 | 11 | 2 | 16.625 364 190 080 948 | 2 615 |
| 6 | 139 | 64 | 64 | 11 | 2 | 16.625 364 190 080 948 | 2 614 |
| 7 | 148 | **65** | 72 | 11 | **0** | — sealed — | — |

Read the `routed` column. **Fifty repair contacts are appended across attempts
2–6 and the routed contact count does not move.** Every one of them is
condemned at **quarantine**, before any arm is ever proposed: the quarantined
count rises by exactly what the plan gained. A quarantined terminal is skipped
in the routing cadence (`CcoController::advance_skipped_terminal`,
`runner.rs:2723`), never becomes a routed contact, and coverage is measured over
**routed** contacts only — the rule this module's own header states. So the
appended rows could not move any measured quantity, and they did not.

### B-O7.3 Hypothesis 1 is falsified on every quantity the measurement produces

R12's clause 1 asks whether a finer measured quantity moves during the flat
rounds. Across attempts 1 → 5 the following are **bit-identical**, not merely
close:

| quantity | attempts 1–5 |
| --- | --- |
| `failing_uncovered_area_mm2` (objective component 1) | `16.625364190080948` |
| `max_patch_area_mm2` (component 2) | `16.433943572110365` |
| `failing_patch_count` (component 3) | `2` |
| `worst_gap_mm` (component 4) | `+inf` (`NoRoutedContact`) throughout |
| uncensored total uncovered area (clause 3's quantity) | `16.82511320562989` |
| **uncovered sample count** | `2615` |
| worklist patch count | `11` |

Every candidate quantity §B.9 named — including the two it did not have (the
sample count and the per-route worst gaps) — is unchanged. **There is no finer
granularity to refine the objective at, because nothing physical moved.** R12's
clause 1 therefore does not apply, and the escalation it reserved for Craig (a
change to clause 1, which is a spec reading) is **not needed and is not
requested**.

The one quantity that *did* move is the wrong direction and it matters:
`unrouted_no_winner` climbs **7 → 11** across the plateau. The dead weight did
not merely fail to help — it cost four terminals their winner. Under §C clause 2
each of those steps is a **regression** and would be rejected outright. The
one-winner controller is strictly better on this model than the batch loop, not
merely different.

### B-O7.4 Clause 2 — SHOW WHICH ROWS, as ruling R12 demands

The nine rows appended in the sealing round are, by exact pool identity:

> **island 9, component 0, pool orders 3976, 3977, 3978, 3979, 3980, 3981, 3982,
> 3983, 3984** — nine consecutive rows.

At the **first flat round** (round 1), every one of them was already:

| | value at round 1 |
| --- | --- |
| eligible | **true**, all nine |
| nominated | **false**, all nine |
| route-local score (today's rule) | `16.797131293215315` mm², all nine |
| **global score (rule N2)** | `16.82275036793747` mm² (eight of nine), `16.797131293215315` mm² (order 3977) |

They were **eligible and unnominated, throttled out** — precisely hypothesis 2's
prediction, and precisely the row R12 asked to be shown. The repair pool is
frozen for the run (`derive_child` clones `repair_pool` and `coverage_samples`
verbatim, `specialized.rs:1475`–`1477`), and the exclusion sets only ever grow,
so a row appended at round 6 was necessarily eligible at every earlier round.
That is a structural proof, not an inference from the trace.

**And the sealing contact is ONE contact.** Attempt 7 routes **65** where
attempt 6 routed 64: exactly one additional routed contact takes the failing
area from 16.625 mm² to zero. Crawler Zombie needs **one committed step**, not
nine and not six — comfortably inside the registered bound of 8 (R6, R11).

### B-O7.5 The honest correction to §B.9's own expectation

§B.9 predicted that if hypothesis 2 held, "the global scoring of rule N2 must
surface the sealing rows immediately, in the first trials". **That prediction is
half right and the design must say so rather than claim the win.**

- **Right:** the sealing rows do carry a top global score from round 1 —
  16.82 mm², which is the entire uncovered area of the model. Rule N2 ranks them
  at the very top.
- **Wrong:** so do 2,303 other rows. At round 1, of **5,831** eligible rows,
  **2,304 score at or above 16.0 mm²** and 4,584 score at or above the censor.
  The global score is an area, and every seat inside 6.0 mm of this model's one
  large hole reaches essentially all of it. **Global scoring cannot separate a
  seat that will route from a seat that will be quarantined**, because the score
  measures reach and quarantine measures the stub axis.

What the throttle actually did was pick, per round, a run of **spatially
contiguous** pool orders out of that tie — round 1 picked island 9 orders
3931–3939, round 6 picked 3976–3984 — and the first neighbourhood was condemned
while the second was not. That is **exactly ruling R13's mechanism**: scores tie
on absolute area, the tie-break follows pool order, and pool order is
spatially contiguous.

> **So the resolution is: clause 1 stands, rule N1 supplies the attention, rule
> N2 puts the sealing rows in the top tie, and rule N4 — spatial diversity on
> rejection — is what walks that tie into a routable neighbourhood instead of
> marching 3931, 3932, 3933 … through a condemned one.** N4 is not a cost
> optimisation on this model. It is the mechanism that closes it.

Under one-winner: the winner is drawn from the 2,304-way top tie; if its contact
is quarantined the trial covers nothing, clause 1 and clause 3 both fail, it is
rolled back and ledgered, and **N4 deprioritizes every remaining candidate
inside that rejected seat's own 6.0 mm coverage disc**. The next winner is a
seat outside it. Under R10 there is no failed-tries cap, so the search cannot be
truncated before it reaches a routable seat, and the first one that routes
covers ~16.8 mm² and seals the model in a single commit.

### B-O7.6 What this addendum does NOT claim

- It does not claim Crawler Zombie emits under the new controller. That is a
  **Prove** obligation on the model (§E.5 case 10) and it stays one. What is
  established here is that the model needs **one** improving contact, that the
  contact exists in the pool, that it is eligible from the first repair round,
  and that N2+N4 are the rules that reach it.
- It does not generalise the quarantine finding to the other eight flat-round
  models (Morel Mudskipper, Skeleton_2, Tavey, zombie-3-a, Skulker Zombie and
  the rest). Only Crawler Zombie — the worst measured case, and R12's named
  victim — was traced. The others' flat rounds are **presumed** to be the same
  dead weight and are **not measured**; Prove should read the same two columns
  (`routed`, `unrouted_no_winner`) on each of them, which §A.8's reporting now
  prints on every run without any instrumentation at all.
- It does not add a rejection class for quarantine. It was tempting: a trial
  whose own appended contact is quarantined cannot cover anything, so it could
  be rejected after Preparation without paying route → rewire → brace →
  structure → lower → measure. **It is not implemented, deliberately**, because
  the measurement above disproves the inertness that would justify it —
  `unrouted_no_winner` moved 7 → 11 while those contacts were being appended, so
  adding a quarantined contact demonstrably changes *other* terminals' outcomes.
  An early rejection would therefore be a guess about a plan that had not been
  routed. It is recorded as a **follow-up with its own measurement obligation**,
  not taken here. What is implemented is the evidence: every trial reports
  whether its own contact was quarantined, so the class is visible in the record
  and can be sized before anyone acts on it.

### B-O7.7 Consequences recorded against the body of this design

1. **§B.1 clause 1 is unchanged.** No refinement, no tie-break at a finer
   granularity, no new quantity in the lexicographic key.
2. **§B.9's "SHOW WHICH" obligation is discharged** by §B-O7.4 with exact pool
   identities and their global scores.
3. **§A.4 rule N4 is promoted from a cost mitigation to a correctness-relevant
   search rule** on at least one currently-emitting model. §F.5's framing —
   "N4 is expected to cut the worst case hard, and it is not measured yet" —
   stands for cost; for Crawler Zombie's emission it is now load-bearing.
4. **§E.5 case 10 keeps its Prove obligation**, and gains the specific thing to
   assert: the model seals in **one** commit, and the committing row's
   `scored_from` credits island 9 component 0.
5. **O-7 is closed as a mandate** and remains open only as the Prove-stage
   emission check.

---

## C. Non-regression

### C.1 The vector

```rust
/// The noncoverage counts a committed repair step may never worsen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoncoverageVector {
    /// Regular terminals the CCO transcript retired with no winner
    /// (`FinalizedDispositionKind::UnroutedNoWinner`). Uncovered area follows
    /// these, so a repair that unroutes two terminals to route one is a
    /// regression even when the area happens to fall.
    unrouted_no_winner: u32,
    /// Terminals skipped as quarantined (`SkippedQuarantined`).
    quarantined: u32,
    /// Specialized-minimum and island-birth contacts left unrouted. SPEC §6
    /// calls these a hard failure; see C.3 for why this design scopes the
    /// failure to the trial and what that costs.
    unrouted_specialized: u32,
    /// The plan's own frozen gate counts, carried from
    /// `SpecializedGateEvidence::accepted`: load violations and unsupported
    /// specialized contacts. Same two components `gate_not_worse` already
    /// compares.
    load_violations: u32,
    unsupported_specialized: u32,
}
```

**AMENDED (v2.1) — what #49 changed under this vector, and the one component it
deliberately does NOT gain.**

- `unrouted_no_winner` and `quarantined` are unchanged in definition and now
  read out of a plan that has already been stripped, which is what R8 was for.
- `unrouted_specialized` now has a first-class producer:
  `AcceptedContactSet::specialized_unrouted` (`runner.rs:3032`), surfaced
  publicly as `unrouted_specialized_contacts()` (`runner.rs:1003`). §C.1 counts
  that list rather than re-deriving it from the transcript. **Read §C.4 for what
  the count may and may not do.**
- `load_violations` and `unsupported_specialized` are read from
  `plan.gate_evidence().accepted` on the **accepted** (reduced) plan, not the
  routed one — `AcceptedContactSet::plan()` (`runner.rs:3059`) selects it.
  Reading the pre-strip plan would compare a partition SPEC §6 says must not
  exist.
- **NO component for ownerless faces, and that is deliberate.**
  `AcceptedContactSet::ownerless_faces` (`runner.rs:3051`) counts model faces the
  repartition charges to nobody. It is tempting to add it here: "a repair step
  may not increase the ownerless face count" reads like a safety rule. It is not
  one. #49's own ruling, written into `runner.rs:3037–3044`, is that load
  eligibility is **provenance, not reach**, so an ownerless face is not a
  measurement that anything is unsupported — the measurement that decides that is
  the routed-coverage judgment this controller runs. Making a set-membership
  tally a hard component would let a bare count veto a step the physical
  measurement approves, which is the physics-first doctrine's exact prohibition.
  **Ownerless faces travel as evidence** on every `CoverageRepairStep` (§D.1) so
  a step that creates them is visible, and they decide nothing.

Plus the rule that is not a count: **a trial that cannot be built is
infinitely worse.** A stage refusal at Routing, Rewire, Brace, Structure or
Lowering rejects the step outright (`RepairStepOutcome::StageRefusal`). That is
the whole of task #47: the Giant Bat's `Brace(ClearanceViolation(1))` becomes
one ledger row against one repair row, and the search continues.

### C.2 Where the components come from, and what the check costs

| component | source | cost |
| --- | --- | --- |
| `unrouted_no_winner`, `quarantined` | one pass over `transcript.rows()` (`cco.rs:908`) counting `FinalizedDispositionKind` (`cco.rs:719`, variants at `:721`/`:722`), reachable from the lowered run via `structured.braced.rewired.routed` (`runner.rs:937`) | O(cadence). Toad: ~1,200 integer compares. |
| `unrouted_specialized` | **AMENDED (v2.1):** no pass needed — #49 built the list. `AcceptedContactSet::specialized_unrouted` (`runner.rs:3032`), also public at `runner.rs:1003` | O(1), a length read |
| `load_violations`, `unsupported_specialized` | `plan.gate_evidence().accepted`, already computed and already frozen | O(1), a field read |

Against a trial that costs `cadence × pool_rows` seat evaluations — 6.0 × 10⁸ on
the toad — the check is ~10³ operations. It is free, and it needs no new
geometry call, no new allocation, and no new registered limit.

### C.3 The forgetting-proof, rebuilt where it can exist (blocking change B4)

**v1 put it in the wrong crate and it could not have compiled.** `PreWriteGate`
is defined in `plinth-pipeline` (`crates/plinth-pipeline/src/gates.rs:61–74`,
verified unchanged at `a8feff4`; `PreWriteGate::ALL` at `:77`).
`plinth-support`'s `[dependencies]` are `geom_kernel, plinth-core,
plinth-hollow, plinth-layers, plinth-mesh, rayon, serde, thiserror` — no
`plinth-pipeline`, and the dependency runs the other way
(`crates/plinth-pipeline/Cargo.toml` lists `plinth-support`). The controller
lives in `plinth-support/src/runner.rs`. **It cannot match on `PreWriteGate`, and
it must not be made able to** — that would invert the crate graph to buy a test.

**And there is almost nothing to match on yet.** Of the 12 gates, exactly one is
implemented — **re-verified at `a8feff4`, unchanged**. `judge` at `gates.rs:277`:
`UncoveredIslands` (line **284**) is the only arm
returning `Ok(GateVerdict::Passed(...))`; the other 11 — `Clearance, Load,
UnsupportedMinima, IslandBirths, RegularContactInvariants,
UnroutedSpecializedContacts, Connectivity, Printability, Murray, BallBreach,
PhysicalContactInvariant` — are ORed together at lines 302–312 and all map to
`Ok(GateVerdict::NotYetPorted)`. An exhaustive match today would send all eleven
to "no component", which is a wildcard wearing a different hat and precisely what
the idiom exists to prevent.

**What clause 2 actually covers today, stated plainly.** `UncoveredIslands` is
clause **1**'s own quantity, not clause 2's. Of the eleven clause-2 gates:

| gate | covered by | honestly |
| --- | --- | --- |
| `Clearance` | `StageRefusal { phase: Brace }` | indirectly. A clearance violation refuses the trial; it is not compared as a count. |
| `Load` | `load_violations` | partially, via `gate_evidence().accepted` — a house count, not the gate's. |
| `UnsupportedMinima` | `unsupported_specialized` | partially, same caveat. |
| `UnroutedSpecializedContacts` | `unrouted_specialized` | partially, same caveat. |
| `IslandBirths` | — | **nothing.** |
| `RegularContactInvariants` | — | **nothing.** |
| `Connectivity` | — | **nothing.** |
| `Printability` | — | **nothing.** |
| `Murray` | — | **nothing.** |
| `BallBreach` | — | **nothing.** |
| `PhysicalContactInvariant` | — | **nothing.** |

**Seven clause-2 gates have no component at all**, and the four that do are
matched by house proxies rather than by the gates' own counts, because those
gates are not ported and therefore produce no count to compare. So SPEC §6
clause 2 is satisfied **in part, by proxy** — and the design says so rather than
asserting coverage it does not have. The remaining eleven-twelfths is **task #42**
and this design does not claim it.

**The proof mechanism that can exist: a pipeline-side drift test.**

```rust
// crates/plinth-pipeline/tests/noncoverage_gate_drift.rs
// Legal here: this crate depends on BOTH plinth-pipeline's PreWriteGate and
// plinth-support's NoncoverageVector.

/// What the A4 non-regression vector claims about each pre-write gate.
/// EXHAUSTIVE over PreWriteGate, no wildcard arm: adding a 13th gate is a
/// compile error in this file.
fn a4_coverage_claim(gate: PreWriteGate) -> A4Claim {
    match gate {
        PreWriteGate::UncoveredIslands            => A4Claim::IsClauseOne,
        PreWriteGate::Clearance                   => A4Claim::ByStageRefusal,
        PreWriteGate::Load                        => A4Claim::ByComponent(Load),
        PreWriteGate::UnsupportedMinima           => A4Claim::ByComponent(UnsupportedSpecialized),
        PreWriteGate::UnroutedSpecializedContacts => A4Claim::ByComponent(UnroutedSpecialized),
        PreWriteGate::IslandBirths                => A4Claim::NotCovered,
        PreWriteGate::RegularContactInvariants    => A4Claim::NotCovered,
        PreWriteGate::Connectivity                => A4Claim::NotCovered,
        PreWriteGate::Printability                => A4Claim::NotCovered,
        PreWriteGate::Murray                      => A4Claim::NotCovered,
        PreWriteGate::BallBreach                  => A4Claim::NotCovered,
        PreWriteGate::PhysicalContactInvariant    => A4Claim::NotCovered,
    }
}

#[test]
fn a_ported_gate_must_have_an_a4_component() {
    for gate in PreWriteGate::ALL {
        let ported = !matches!(judge_for_drift(gate), GateVerdict::NotYetPorted);
        assert!(
            !ported || a4_coverage_claim(gate) != A4Claim::NotCovered,
            "{gate:?} moved off NotYetPorted but the A4 non-regression vector \
             still has no component for it — see task #42 and SPEC §6 clause 2",
        );
    }
}
```

This is the hand-off #42 needs: the day a gate is ported, this test fails until
someone decides what clause 2 compares for it. It is a real obligation instead of
a promise, and it lives where the crate graph allows it.

**And `plinth-support` stays independent of `PreWriteGate`.** `NoncoverageVector`
declares its own components with its own exhaustive `NoncoverageComponent::ALL`;
the pipeline-side test is the only place the two vocabularies meet.

### C.4 The downgraded hard failure, argued rather than assumed (finding S10)

SPEC §6: "An unrouted specialized minimum or island-birth contact is a hard
failure." §C.1 makes a nonzero `unrouted_specialized` on a **trial** a rejection,
and rejecting one candidate and continuing is not a hard failure. v1 did this
silently. v2 argues it:

The spec's "hard failure" is a statement about **the accepted plan**. A trial's
child plan is not an accepted plan — SPEC §7 says an exception "discards the
clone and restores the entire accepted state", and a discarded clone with an
unrouted specialized minimum never becomes the model's plan. So the trial-scoped
rejection is the correct scope, *provided* the accepted state's own count is
enforced as a hard failure at attempt 0 and after every commit.

> **The rule, stated in full.** `unrouted_specialized > 0` on the **accepted**
> state (attempt 0 or any committed step) is `SupportRunError::Failed`, a hard
> failure, exactly as §6 says. `unrouted_specialized > 0` on a **trial** rejects
> the trial and ledgers it, because the trial's plan is discarded and never
> becomes the accepted state.

That reading is derivable from §7 and it needs Craig's confirmation because v1
did not raise it. Open item O-5.

> **AMENDED (v2.1) — FLAGGED, NOT SILENTLY FIXED: `a8feff4` does not do the
> accepted-state half, and it says so deliberately.**
>
> Task #49 landed the measurement without the refusal.
> `SealedSupportAuthority::unrouted_specialized_contacts()`
> (`runner.rs:1003`) carries the list, and its own doc comment reads: *"§6 calls
> this case a hard failure; this change measures and ledgers it without refusing
> on it."* So at the base this design implements against, a nonzero
> `unrouted_specialized` on the accepted state is **evidence, not a failure**.
>
> **This makes the first half of §C.4's rule a behaviour change, not a
> restatement**, and this lane has not measured it. Turning it on would refuse
> every model that carries one such contact today — a set nobody has counted.
>
> **What v2.1 adopts, and what it defers:**
>
> - **The trial half is adopted** and is unambiguous: a trial whose child plan
>   leaves a specialized minimum or island-birth contact unrouted is **rejected
>   and ledgered**, because its plan is discarded and never becomes accepted.
>   Nothing in the base contradicts that, and §7 supports it directly.
> - **The accepted-state half is DEFERRED**, not implemented, not silently
>   dropped. §C.1 counts `unrouted_specialized` and compares it componentwise —
>   so a repair step may never *increase* it, which is clause 2 doing real work —
>   but a nonzero count inherited from attempt 0 does not by itself refuse the
>   model. That matches `a8feff4`'s behaviour exactly, so this design changes
>   nothing there and hides nothing.
> - **O-5 is rewritten as a two-part question for Craig**: (a) is #49's
>   measure-and-ledger the intended reading of §6's "hard failure", or a staging
>   step? and (b) if it is a staging step, how many corpus models does turning it
>   on refuse? Question (b) is one corpus run and must be answered before the
>   rule is adopted, not after.

---

## D. Exact attribution

### D.1 One step, one row, one delta

Because a step changes the plan by exactly one contact, the difference between
its before- and after-measurement *is* the contact's contribution. There is no
inference and no apportionment.

```rust
/// One repair transaction, as it goes into the evidence stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoverageRepairStep {
    pub trial: usize,
    /// Which commit this was, if it committed. `None` for a rejection.
    pub commit: Option<usize>,
    /// The winner, by exact pool identity, and where it sits.
    pub row_id: RepairRowId,
    pub seat_mm: [f64; 3],
    /// The route the row is drawn from …
    pub source_island_id: u32,
    pub source_component_id: u32,
    /// … and every route whose uncovered samples contributed to its GLOBAL
    /// score, with each route's share. Often the row's own route is NOT the
    /// largest contributor, and that cross-route effect is the uncensored
    /// worklist's whole justification. Per route, not per patch (finding S11).
    pub scored_from: Vec<CoverageScoreShare>,
    pub marginal_area_mm2: f64,
    /// Whether the first-trial guarantee (rule N3) chose this route rather
    /// than the global ranking. Makes the guarantee's cost measurable.
    pub chosen_by_first_trial_guarantee: bool,
    /// The objective on both sides of the transaction.
    pub objective_before: CoverageObjectiveWire,
    pub objective_after: Option<CoverageObjectiveWire>,   // None if never measured
    /// The anti-gaming quantity on both sides (§B.5 clause 3).
    pub uncensored_before_mm2: f64,
    pub uncensored_after_mm2: Option<f64>,
    pub gates_before: NoncoverageVector,
    pub gates_after: Option<NoncoverageVector>,
    /// NEW IN v2.1, and EVIDENCE ONLY (§C.1). How many model faces the #49
    /// carried-load repartition charged to no contact, on both sides. A step
    /// that creates ownerless faces is visible in the record; it is not
    /// rejected for it, because load eligibility is provenance and the routed
    /// coverage measurement is what rules on the face.
    pub ownerless_faces_before: usize,
    pub ownerless_faces_after: Option<usize>,
    /// NEW IN v2.1 (ruling R13). Whether rule N4 deprioritized this candidate
    /// when it was chosen, and how many rejected coverage discs its seat lay
    /// inside. This is the instrument that says whether diversity-on-rejection
    /// did any work on this model.
    pub diversity_discs_containing_seat: usize,
    /// Every route whose evidence moved, and by how much. Routes that did
    /// not move are omitted, so a committed step that touched one route
    /// carries one row.
    pub route_deltas: Vec<RouteDelta>,
    pub outcome: RepairStepOutcome,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteDelta {
    pub island_id: u32,
    pub component_id: u32,
    pub failing_area_before_mm2: f64,
    pub failing_area_after_mm2: f64,
    pub uncensored_area_before_mm2: f64,
    pub uncensored_area_after_mm2: f64,
    pub failing_patches_before: usize,
    pub failing_patches_after: usize,
    pub worst_gap_before: CoverageClosureGap,
    pub worst_gap_after: CoverageClosureGap,
}
```

`CoverageClosureAttempt` (attempt / blocking count / worklist count / nominated
rows) is replaced by this stream. It is **public API** (re-exported at
`lib.rs:130`), as is `coverage_repair_rounds()` (`runner.rs:974`), so replacing
it is a public break; §G.2 and §G.4 record the disposition.

**Note what `uncensored_area_before/after` buys the report:** the patch-splitting
move (§B.5) is now *visible* even when it is rejected — a rejected step whose
failing area fell to zero while the uncensored area did not move is exactly a
split attempt, and it is worth naming in the sidecar as evidence that the guard
is doing work.

### D.2 What the sidecar and the report gain

`CoverageRefusalEvidence` (`coverage_evidence.rs:119`) gains
`steps: Vec<CoverageRepairStep>` and each `UncoveredIslandSummary` gains its
per-route outcome (§E.1). Concretely, for the first time the sidecar can say:

- **which contact closed which hole** — "commit 3, row (island 3, component 0,
  candidate 118) at (10.4, 7.6, 19.0), scored 0.46 mm² of which 0.36 mm² came
  from island 5 and 0.10 mm² from its own island 3, closed island 5's 0.36 mm²
  patch" — the dust fixture's mechanism, today asserted only through a proxy, and
  now with the cross-route share printed rather than inferred;
- **which contacts did nothing** — "trials 1, 2, 4–8 each moved the objective by
  0.00 mm² and were rolled back", instead of today's "289 rows nominated, area
  unchanged";
- **which contact broke something** — "trial 5 rejected: `unrouted_no_winner`
  4 → 6", or "trial 6 rejected: Brace `ClearanceViolation(1)`" — #47's diagnosis
  written down instead of terminating the run;
- **which contact tried to game the censor** — "trial 9 rejected `NoAreaCovered`:
  failing area 0.30 → 0.00 mm² but uncensored area 0.30 → 0.30 mm²";
- **why a route was never tried, and what the measured cause is** — §E.1.

This is also what makes the controller falsifiable in tests: today
`public_runner_closes_overhang_table_only_after_appending_repair_contacts`
asserts that the blocking count is non-increasing and falls at least once, which
is a proxy for "a repair worked". With attribution the test asserts the thing
itself.

---

## E. Termination reasons — measured defect classes, not distance claims

**AMENDED (v2.1) — VERIFIED against Craig's ruling R14, and it holds.** R14
restates that termination reasons must be measured defect classes and never
distance claims, and names the five: `ClearanceMaskUnsatisfiable`,
`StubReentry`, `PoolExhausted`, `NoCandidateCoversAnything`, `NoPoolRows`. §E.1's
`BlockedCause` below already carries **exactly those five and no others**, each
with the counter that measures it named in its own doc comment, and
`NoRowWithinRadius` is already deleted. **No change was required to satisfy
R14.** Three points re-checked in the process, because "already correct" is a
claim and not an excuse:

1. **`NoCandidateCoversAnything` carries `nearest_seat_to_uncovered_mm`.** A
   distance in a measured class is not a distance *claim*: the class is decided
   by the score being 0.00 mm², and the distance rides along as context. The
   variant's own comment says so. It stays.
2. **`ClearanceMaskUnsatisfiable` and `StubReentry` are counted, not inferred.**
   The first reads the CCO transcript's per-terminal refusal reasons; the second
   reads the quarantine reason `overlap.exceeds_cutoff` with its recorded depths.
   Both are counters the run already keeps.
3. **`CoverageClosureGap::NoRoutedContact` is still a gap, never a cause.** It is
   reported beside the cause and never as one — the name collision that finding
   S1 caught. Verified at `a8feff4`: the variant lives at
   `coverage_closure.rs:297–299`, and the aggregation rule that makes it outrank
   any finite distance is at `coverage_closure.rs:451–452`.

### E.1 Craig's no-unreachable ruling, and what replaces v1's `UnreachableCause`

**v1's §E was wrong, and the island diagnosis proves it.** v1 assigned Warhorse
islands 5 and 6 the cause `NoRowWithinRadius`, defined as "the route has pool
rows, but none is within the coverage radius of any uncovered sample", inferred
from the islands' worst gaps of 7.09 mm and 15.29 mm against `R = 6.0 mm`. Those
are two different distances: worst gap measures the distance to the nearest
**routed contact**, and reach measures the distance to a **candidate seat**. The
reviewer said the inference was unsupported. The island diagnosis
(`island-out/islands.json`, built from the same binary at `c50563e`) says it is
**false**:

| | Warhorse island 5 | Warhorse island 6 | RW island 74 | RW island 68 |
| --- | --- | --- | --- | --- |
| pool rows for the route | 300 | 189 | 219 | 377 |
| rows whose seat is within `R` | **220** | **170** | **219** | **377** |
| nearest candidate seat to the uncovered area | **0.0972 mm** | **0.0478 mm** | **0.0050 mm** | **0.0171 mm** |
| worst gap (v1's quantity) | 7.09 mm | 15.29 mm | NoRoutedContact | NoRoutedContact |

Candidate seats sit **on the hole**, within a tenth of a millimetre, in all four
cases. "Nothing reached it" and "nothing could reach it" are not the same
statement, and v1 printed the second while measuring the first.

**Craig's ruling R7.** §E's honest-termination reasons become the **measured
defect classes**, never a distance claim. The classes, and where each is
measured:

```rust
/// Why one still-uncovered route is still uncovered, when the loop stopped.
/// Every variant is MEASURED from a counter the run already keeps. None of
/// them is a distance claim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RouteRepairOutcome {
    /// Rows existed, were tried, and none passed the accept clauses.
    TriedNoImprovement { rows_tried: usize, best_marginal_area_mm2: f64 },
    /// Rows existed and remained when the commit bound ended the search.
    NotReached { rows_remaining: usize },
    /// A measured defect class. The route cannot be repaired by ANY row in
    /// its pool under today's rules, and the counter that says so is named.
    Blocked(BlockedCause),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BlockedCause {
    /// TASK #51. Every candidate arm the router proposed for this route was
    /// refused by the support-to-model clearance standoff. Measured from the
    /// CCO transcript's per-terminal refusal reasons — the same counter
    /// islands.json reports as
    /// `terminals_where_every_candidate_was_clearance_refused`.
    ClearanceMaskUnsatisfiable {
        terminals: u32,
        best_shortfall_mm: f64,
        best_shortfall_by_family: Vec<(ArmFamily, f64)>,
        standoff_mm: f64,
    },
    /// TASK #52. Every contact placed on this route was condemned BEFORE any
    /// arm was tried, because the stub axis re-enters the model further than
    /// the allowance. Measured from the quarantine reason
    /// `overlap.exceeds_cutoff` plus the recorded re-entry depths.
    StubReentry {
        terminals_quarantined: u32,
        depth_min_mm: f64, depth_median_mm: f64, depth_max_mm: f64,
        allowance_mm: f64,
    },
    /// Every eligible row was tried; each was rolled back.
    PoolExhausted { pool_rows: usize, occupied: usize, ledgered: usize },
    /// No eligible row scores above 0.00 mm² against the current uncovered
    /// set. This is B2's guard, and it IS a measurement (the score), not an
    /// inference from a distance. Named so it cannot be read as a reach claim.
    NoCandidateCoversAnything {
        pool_rows: usize,
        rows_within_radius: usize,
        nearest_seat_to_uncovered_mm: f64,
    },
    /// The route never had a pool row of its own.
    NoPoolRows,
}
```

`NoRowWithinRadius` is **deleted**, and the name `NoRoutedContact` is not reused
as a cause — it already means something else as a `CoverageClosureGap` variant
(`coverage_closure.rs:297–299`), namely "no routed contact reached this patch",
and reusing it for "no pool row can reach it" would be read as the same thing
(finding S1). The gap keeps its own name and is reported *alongside* the cause,
as context, never as the cause.

The loop never refuses *because* a route has no rows. Such a route simply
contributes no candidates to the ranking; other routes keep their budget; the
loop ends when the ranking is empty or nothing improves; and each route reports
its own outcome. That is the whole of task #24's fix, folded in here as required.

### E.2 The Warhorse — cited evidence

**v1's citation was wrong and is fixed.** The figures in v1's §E come from the
endpoint-mask judge's scratch runs, not from `CURRENT-STATUS.md`. Cited by name:

| what | where |
| --- | --- |
| the run | `/private/tmp/claude-501/-Users-craig-code-hughescr-dnd-campaigns/5769930f-658e-4eba-a883-d959c7bc2ab6/scratchpad/endpoint-out/judge-warhorse/run.log` |
| the sidecar | `…/scratchpad/endpoint-out/judge-warhorse/Warhorse.coverage.json` |
| the input | `…/scratchpad/endpoint-out/in/Warhorse.stl`, sha256 `a7bf432938ec4fcc6a439de50c18cd76f6e7f6e8a675b60e400b7cfce46b5388` |
| the tip | `develop` @ `c50563e`, gate profile, `saturn4-ultra`, 2.500 mm wall, dense orientation sweep |
| identical reruns | `…/endpoint-out/fix-warhorse/`, `…/endpoint-out/run3/`, `…/island-out/wh-base/`, `wh-diag/`, `wh-diag2/` — all five reproduce the same figures byte for byte |
| the island diagnosis | `…/scratchpad/island-out/islands.json` (same binary, same tip) |

Measured refusal: `CoverageClosureRefusal { attempts: 6, reason:
NoEligibleRepairRows }`; island 5 `1.587115444957305 mm²`, worst gap
`Finite(7.0920667066057925)`, `nominated_repair_rows: 289`,
`eligible_repair_rows: 0`; island 6 `3.003881883037568 mm²`, worst gap
`Finite(15.29136858028533)`, `nominated_repair_rows: 176`,
`eligible_repair_rows: 0`; 609 rows appended in total across the six attempts.

**`CURRENT-STATUS.md:294`'s "Warhorse's 0.6505 mm² over 13 islands" is a
different, older run** — it predates the endpoint-mask work and it is not this
model's current state. v1 did not cite it, but the reviewer could not find any
other Warhorse figure and reasonably suspected a mismatch. It is a mismatch, and
the older number is the one to disregard.

### E.3 The Warhorse walk-through, both futures

The controller's behaviour on this model is **not what decides it**. Two
tasks do. Both futures are stated because Craig ruled that they must be.

#### Future A — #51 and #52 unfixed (today's rules)

1. **Attempt 0 measures.** Islands 5 and 6 fail: 1.587 mm² and 3.004 mm², which
   are 9.9× and 18.8× the 0.16 mm² censor. §4 says both fail.
2. **Ranking.** Both routes have plenty of in-reach seats (220 of 300, 170 of
   189; nearest 0.097 mm and 0.048 mm). Global scores are far above zero, so
   §B.3's guard does **not** fire and both routes clear rule N3's serve-set
   threshold.
3. **The first trial.** The winner is appended; routing proposes candidate arms
   for its terminal; **every one is refused by the 0.6 mm support-to-model
   clearance standoff.** On island 5, 253 of 253 terminals were clearance-refused
   with a best shortfall of −0.350 mm (model anchor), −1.4455 mm (plate),
   −1.0407 mm (bifurcate); on island 6, 176 of 176, best −0.2531 mm (plate),
   −0.350 mm (model anchor). Arm length is not the constraint: of island 5's
   31,627 feasible candidate arms, not one satisfies the mask.
4. **The trial is rejected, and the ledger names why.** The new terminal ends
   `UnroutedNoWinner`, so `unrouted_no_winner` goes up by one and §C's clause 2
   rejects the step as `Regression { component: UnroutedNoWinner, before: n,
   after: n+1 }`. That is a **precise, attributable statement of the #51 defect**
   at the controller's own level, which today's loop cannot produce at all.
5. **It repeats.** Under rulings R6 and R10 rejections are bounded only by pool
   exhaustion and the zero-score guard, and the scores never fall to zero, so the
   controller works down the worklist's eligible rows.
   **AMENDED (v2.1):** v2 wrote "island 5's 300 rows and island 6's 189 rows",
   489 in total, taken from `islands.json` for the two *blocked* islands. That
   undercounts. §H.1 keeps the worklist **uncensored**, so the pool that must be
   exhausted is every worklist route's eligible rows, and stage 0 measured
   **6,777 at one accepted state** on this model — 13.9× the design's figure. The
   corrected accounting is §F.5.
6. **It stops** with `NoImprovingRepair`, and each route reports
   `Blocked(ClearanceMaskUnsatisfiable { terminals: 253, best_shortfall_mm:
   -0.350, standoff_mm: 0.6 })` and `{ terminals: 176, best_shortfall_mm:
   -0.2531, standoff_mm: 0.6 })`.
7. **The verdict: refuse.** §6 forbids converting residual uncovered area into
   success; §5's fail-closed principle and §7's "no partially relocated forest
   may escape" agree. `run_support` returns
   `Err(SupportRunError::CoverageClosure(...))`, and
   `ChainStop::is_support_refusal()` returns **false** for `CoverageClosure`
   (`prepare.rs:541`, the `CoverageClosure` exclusion at `:546`), so the
   oriented-solid reserve is not substituted and
   `seal_and_emit` is never called. **No `.chitubox` file.** What it does write is
   the FU-57 evidence triple — `warhorse.coverage.json`, up to five island
   renders, and the refusal card — with `PipelineExitCode::Supports` (5) and
   `artifacts().count() == 0`.

**The cost of step 5, stated plainly and not argued away. AMENDED (v2.1) —
the number is bigger and the mechanism is different.** v2 wrote 489 full
pipeline passes against 6 today, and invited Craig's override at open item O-1.
Both halves are corrected:

- **The number.** The measured pool that must be exhausted is **6,777** eligible
  rows at one accepted state, not 489. Against 7 passes today that is **~970×**
  more routing work on this model, not 70×.
- **The mechanism, which is the part that matters.** Those 6,777 passes are not
  6,777 *different* proposals. Scores tie on absolute area and the tie-break is
  pool order, which is spatially contiguous — the measured shape is **63 seats
  inside a 1.04 mm box**. So the loop spends run after run of full pipeline
  passes re-proving the same physical seat. **That is a search-order defect, and
  ruling R13 fixes it at the defect** (§A.4 rule N4) rather than capping the
  symptom.
- **Craig has ruled out the cap.** R10: no failed-tries cap; the loop gives up
  only when no candidate can physically help. O-1's recommended
  `coverage_repair_max_trials_per_commit` is **withdrawn**, not deferred. What
  replaces it is N4 plus §A.8's loud reporting.

What changes versus today, even in Future A:

| | today | v2, Future A |
| --- | --- | --- |
| verdict | refuse | refuse (unchanged, and now derived) |
| refusal reason | `NoEligibleRepairRows` — sounds like a bug in the pool | `NoImprovingRepair`, with both routes `Blocked(ClearanceMaskUnsatisfiable)` and the shortfall in millimetres |
| contacts kept | 609 appended and kept, 465 of them on the two islands that never moved | **0 kept**, because none improves |
| what an owner learns | "the repair pool ran out" | "candidate seats sit on both holes; 429 of 429 proposed arms were refused by the 0.6 mm clearance standoff, missing by 0.25–0.35 mm; fix the model-anchor clearance mask (#51) and this model routes" |
| what #26/PR-7 receives | nothing usable | a measured, per-route case for model-seated arms with the shortfall attached |
| full pipeline passes | 7 | up to **6,777** — **worse**, see §F.5 (v2 said 489; corrected in v2.1) |

#### Future B — #51 and #52 fixed

**The same controller, unchanged, routes.**

The clearance evidence names the mechanism exactly: the best arm on island 5 is a
model anchor that misses by **exactly its own radius, 0.350 mm** — a fixed
consequence of masking only 0.6 mm of the arm at a seat that lies *on* the
surface while demanding radius + 0.6 mm of free air there. That is a mask defect,
not a physics limit, and it is task #51. Fix it and:

1. The first winner's arm routes instead of being refused.
2. It covers a large share of its route's uncovered samples — the seats sit on
   the hole, so the global score is real coverage, not an estimate.
3. Clause 1 improves (failing area falls), clause 3's uncensored total falls,
   clause 2 holds (a routed terminal does not increase `unrouted_no_winner`).
   **Commit.**
4. Rescore, repeat. Island 5's 1.587 mm² and island 6's 3.004 mm² are small
   relative to a 6.0 mm coverage radius, so a handful of committed contacts
   closes them.
5. `failing_patch_count == 0` → `Ok(authority)` → **the Warhorse emits.**

The island diagnosis quantifies how close this is: at a 0.20 mm standoff, 148 of
island 5's 253 terminals route and 84 of island 6's 176 do; at 0.10 mm, 222 and
158. Those numbers are diagnostic only — **the standoff is not to be reduced**,
FU-8b's doctrine applies verbatim, and #51 is the mask fix, not a standoff
change. They are quoted only to show that the routing is a hair's breadth away
and that the blocker is a rule, not the geometry.

**So the Warhorse's fate is decided by #51, not by this controller.** v1 said the
Warhorse "cannot emit" and treated that as a spec-level fact. That was wrong: it
cannot emit *under today's clearance mask*. The correct statement is that the
controller refuses honestly today and routes once #51 lands, and that is why both
futures are written down.

### E.4 The Rotting Warhorse — the throttle's own evidence

Run cited: `…/scratchpad/endpoint-out/judge-rw/run.log` and
`Rotting Warhorse.coverage.json`; input sha256
`165118a844bc25f2ba6b497deaf3c8215d668fb006f21d98078ce0ec583d96bb`; same tip.
Measured: `attempts: 8, reason: AttemptsExhausted`, 718 rows appended, four
uncovered islands (74 at 13.041 mm², 68 at 11.879 mm², 7 at 4.786 mm², 38 at
0.413 mm²).

| | island 74 | island 68 |
| --- | --- | --- |
| uncovered area | 13.041 mm² | 11.879 mm² |
| blocking patches at the last attempt | **1** | **1** |
| pool rows | 219 | 377 |
| seats actually tried in 8 attempts | **8** | **8** |
| eligible seats still untried at refusal | **210** | **361** |
| measured cause | stub re-entry 0.74–2.43 mm against a 0.05 mm allowance; 9 terminals quarantined `overlap.exceeds_cutoff`; **0 arms ever generated** | 9 of 9 terminals clearance-refused; best plate arm short by 0.2452 mm, best model anchor by 0.3555 mm |

**571 usable seats untried**, and the reason is arithmetic: one seat per patch
per attempt, and each of these 13 mm² holes is *one patch*. Rule N1 deletes that
cap; rule N2 makes the whole 13 mm² count toward each seat's score, so these
routes draw winners first; and ruling R6 means the search is not stopped at 8
tries. That is the throttle fix, end to end, and it is measured rather than
argued.

**Island 74 additionally never reaches routing at all.** Its contacts are
condemned at quarantine, before any arm is tried, because the 2.5 mm stub would
run 0.74–2.43 mm *inside* the model before emerging, against a 0.05 mm allowance
— the surface is nearly edge-on to the stub. That is task **#52**, and it is a
different defect from #51. Under v2 it is reported as
`Blocked(StubReentry { terminals_quarantined: 9, depth_min_mm: 0.7386,
depth_median_mm: 0.8768, depth_max_mm: 2.4294, allowance_mm: 0.05 })` — measured
from the quarantine reason, not inferred from a distance. **It costs zero trials
to detect**, because quarantine happens before routing: the controller can read
it at nomination time and report it without spending a pipeline pass.

**Render finding — island 68's surface is nearly vertical, not downward.** The
island's mean face normal in the print frame is
`(-0.7725, -0.6289, 0.0880)` (`island-out/rottingwarhorse-island68.meta.json`),
i.e. **5.05° above horizontal** — the patch faces almost sideways, not down. Two
consequences worth carrying into #26/PR-7:

- It explains the island's refusal histogram: `spot_is_above_the_contact` fired
  1,151,532 times, which is what a sideways-facing patch does to a landing-spot
  search that requires the spot to be at or below the contact.
- **A near-horizontal support arm may reach it under the current rules once the
  mask is fixed.** The arm rules that bind here are the 60° lean limit, the
  5–20 mm length window and the "spot is above the contact" test — none of which
  forbids a nearly horizontal arm to a sideways face, and the island has 377
  in-reach seats with the nearest at 0.017 mm. So island 68 is a #51 case, not a
  reach case, and the physics does not need a new seat rule to serve it.

### E.5 The other acceptance cases

| # | case | expectation |
| --- | --- | --- |
| 1 | **Warhorse** (group B, #24) | terminates honestly; both routes report `Blocked(ClearanceMaskUnsatisfiable)` with the millimetre shortfall; refusal; no file; no route's zero-row state ends the loop early for the others. Future B (with #51): emits. |
| 2 | **Rotting Warhorse** | island 74 `Blocked(StubReentry)` at zero trial cost; island 68 `Blocked(ClearanceMaskUnsatisfiable)`; 571 previously-untried seats become reachable. |
| 3 | **`make_overhang_table` with the FU-87 screen re-landed** (#41) | the screen creates zero-candidate terminals, FU-88 turns them into ordinary uncovered area, and the controller must decide the model on what remains rather than dying on a row-less route. This is the case that unblocks #41 → #26. |
| 4 | **`shelf_with_dust_beside_a_row_less_hole`** (#27's fixture) | still seals; the committed step that closes island 5's 0.36 mm² hole must be attributed to a row drawn from the dust route's pool, **and its `scored_from` must show the cross-route share** — asserted directly now, not by proxy. |
| 5 | **Giant Bat** (#47) | the `Brace(ClearanceViolation(1))` repair row is rolled back and ledgered; the run continues; the model's verdict is decided by coverage, not by one bad arm. |
| 6 | **`make_toed_slab`** | bit-identical. Zero repair trials, digest **`4_190_462_360_230_124_963`** unmoved at all **three** pin sites (§G.2, §G.4). The regression pin for the whole change. **AMENDED (v2.1):** v2 carried `15_556_275_517_430_246_244`, the value at `06cf21b`; three base moves have taken it since (§G.4 row 1). |
| 7 | **`make_raking_blade`** | still refuses. Island 5 still 5.135 mm², worst gap still `NoRoutedContact`, faces still `[14, 15, 20, 21]` (reproduced at `/private/tmp/claude-501/plinth-first-artifact-raking-blade-64750.coverage.json`). Its eligible rows are now **all** tried under rulings R6+R10, so the outcome is `Blocked(PoolExhausted)` or `TriedNoImprovement`, **not** `NotReached` — v1 predicted `NotReached { rows_remaining: 6 }` under a trial cap that R6 and R10 removed (finding S1 resolved in the other direction). **AMENDED (v2.1):** the row count is **18 eligible at one accepted state, 46 summed over the run** (stage 0), not the 14 v2 wrote from the procedural fixture's pool. |
| 8 | **Anti-gaming fixture (new)** | a synthetic patch of 0.30 mm² and a candidate seat that splits it into two 0.15 mm² patches without covering any sample: clause 1 passes, clause 3 rejects, the ledger row reads `NoAreaCovered { before_mm2: 0.30, after_mm2: 0.30 }`, and the model still refuses. Without this fixture clause 3 is untested. |
| 9 | **#27 population** | `Skulker Zombie`, `Tavey Badger`, `zombie-3-a`: measured in stage 0 (§B.6). Measured result: all three seal today, with 1, 1 and 3 improving rounds respectively; none needs more than the bound. |
| 10 | **Crawler Zombie — NEW IN v2.1, and it is the emission blocker** | it emits today only by crossing a **five-round plateau** at 16.625 mm² failing / 16.825 mm² uncensored, then closing in one round. **It must still emit.** Proven on the model in Prove, not argued in the design. This is ruling R12's named victim and §B.9's whole subject. |
| 11 | **Diversity fixture (new, ruling R13)** | two equal-scoring candidates, one inside the rejected seat's 6.0 mm coverage disc and one outside. After the rejection the winner must be the **outside** one, and the inside one must still be tried once the outside candidates are exhausted. Without this fixture rule N4 is untested and its completeness is unpinned. |

---

## F. Work accounting

### F.1 Where the routing charge sits

**`runner.rs:2708`**, inside `route_initial_cco_with_limits` (`runner.rs:2657`):

```rust
let charge = RouteCcoWorkCharge::compute(cadence.len(), pool.rows().len())?;  // :2708
charge.preflight(limits)?;                 // :2709 — refuses BEFORE the first terminal
crate::census_record!(crate::census::record_route_cco_charge(...));           // :2710
```

`cadence` = specialized terminals + regular terminals; `pool.rows()` = the
exterior anchor pool. Charge = `cadence × pool_rows` for both seat evaluations
and seat refusal rows. This runs **once per trial**, unchanged in position.

### F.2 Do one-winner rounds change the charge shape?

Yes — the *growth*, not the *base*.

- Today the cadence grows by `nominations.rows.len()` per attempt, whether or
  not the rows help, and permanently.
- Here the cadence grows by exactly **1 per committed step** and **0 per
  rejected step** (a rolled-back child plan is dropped, so its contact never
  enters a later cadence).

Against the measured numbers:

**Giant Toad (#35). ANSWERED — AMENDED (v2.1) with stage 0's measurement.**
Charged 604,916,591 = 1,223 cadence × 494,617 pool rows at
the failing attempt, against the registered limit 536,870,912. The largest
cadence that fits at that pool size is ⌊536,870,912 / 494,617⌋ = **1,085**. A
one-winner run adds at most `commit_budget` cadence entries over the whole
repair, so at a budget of 8 the toad stays inside the limit for every trial if
and only if its **attempt-0 cadence is ≤ 1,077**. v2 could not read that number
and §F.4 said why. **Stage 0 measured it: the toad's attempt-0 cadence is 273**,
charging 135,030,441 = **25.2% of the registered limit**. Its worst one-winner
charge is 281 × 494,617 = 138,987,377 = **25.9%**. **The toad's routing charge
stops being a blocker under this design, with 74% headroom.** Two honest caveats
stand unchanged: the toad's other blockers are untouched (#29
Lowering `RootRunTooShort`, island 2's 34.48 mm worst gap against a 20.0 mm
maximum strut length, three routes with no routed contact), so clearing the
charge does not deliver the toad — #35's own ruling says the same thing.

**Awakened Tree / treant (#48). AMENDED (v2.1) — v2's paragraph was WRONG, and
wrong in the design's favour.** v2 wrote: "1,130,320,880 = ~2,389 × 473,190,
**2.1× the limit on attempt 0**, before a single repair row exists. One-winner
rounds change nothing here." The 1,130,320,880 figure is exact. **It is not
attempt 0.** Stage 0's per-attempt census row measured both:

| attempt | cadence | pool rows | seat evaluations | vs 536,870,912 |
| ---: | ---: | ---: | ---: | --- |
| 0 | **754** | 409,240 | **308,566,960** | **57.5% — INSIDE the limit** |
| 1 | 2,762 | 409,240 | 1,130,320,880 | 210.5% — refuses |

Attempt 0 fits with **42% headroom**. The 2.1× charge is **attempt 1**, after the
batch loop appended **2,008 repair rows in one go** (754 + 2,008 = 2,762). Under
one-winner rounds the cadence grows by at most `commit_budget` = 8 over the whole
run, so the treant's worst charge is 762 × 409,240 = **311,840,880 = 58.1% of the
limit**.

> **Therefore #48 is not "untouched" by this controller — it is DISSOLVED by
> it.** The treant's `RepairPlanRefused` at `Routing` is caused by the batch
> nomination this design removes. §G.3's "Does not touch: #48" is corrected in
> place, and the design must not be read as fixing #48 by accident: it fixes it
> by mechanism, and the mechanism is rule N1 plus one-winner rounds.
>
> **What this does NOT claim:** that the treant emits. Clearing the routing
> charge lets it reach the stages after Routing for the first time; what it finds
> there is unmeasured. The claim is exactly that the charge stops being the
> blocker.

### F.3 The pipeline-pass count — a cost, not a saving (finding S2)

v1's table read "full pipeline passes: 6 → 1 + one per step actually tried" and
presented it as a win. It is not, and the correction matters more under ruling R6
than it did under v1's assumptions.

Taking `C0` as the attempt-0 cadence and `P` the anchor pool, today's Warhorse
routing work is about `(7·C0 + 3,073)·P` — seven routing passes (attempts 0–6),
carrying 0, 266, 453, 548, 592, 605 and 609 repair rows respectively, summing to
3,073 repair-row-passes. Under v2 with `T` trials the work is about `T·C0·P` plus
one cadence entry per commit. **AMENDED (v2.1):** at the measured `T = 6,777`
(§F.5, not v2's 489) that is **~970× more routing work than today on this
model**, before rule N4. N4 is what is expected to cut it, and §A.8's
`diversity_demotions` is what will say by how much — measured, not predicted.

**The real wins are elsewhere, and they are real:**

- **Contact volume.** 609 permanent contacts become at most `commit_budget`, and
  0 on a model that closes nothing. That is what #35 and #48 are paying for.
- **Attribution.** Every square millimetre is attributed to one row.
- **Recovery.** #47's model-killing stage refusal becomes a ledger row.
- **Diagnosis.** #51 and #52 get named, measured, per-route causes with the
  millimetre shortfall attached, which is what #26/PR-7 needs.

**AMENDED (v2.1) — what turns the pass count from a loss into a win.** v2 said
"a bound on trials, and ruling R6 deliberately does not provide one". Ruling R10
now forbids one outright. What turns it instead is **rule N4** (R13): stop
spending a full pipeline pass to re-prove a seat 16 µm from one that was just
rejected. §F.5 carries the accounting and §A.8 carries the instrument.

### F.4 Stage 0 cannot read the attempt-0 cadence as v1 described (finding S3)

v1 said "the census already records it (`census::record_route_cco_charge`)". It
records **one overwritten cell** — `census.rs:543–555` **assigns** four fields on
`census.route_cco_charge` rather than pushing a row — and it is called **after**
`charge.preflight(limits)?` (`runner.rs:2709` then `:2710`), so an attempt
that refuses records nothing. A multi-attempt run's census therefore holds the
**last admitted** attempt's cadence, which is attempt 0's only if the run refused
at attempt 1.

**Stage 0 must add a per-attempt census row**, not read the existing cell.
**AMENDED (v2.1) — DONE, and it paid for itself twice.** Stage 0 added exactly
that row and the two numbers it produced are §F.2's corrections: the toad's
attempt-0 cadence is **273** (not the ≤ 1,077 unknown), and the treant's
attempt-0 charge is **57.5%** of the limit, not the 210% v2 attributed to attempt
0. The row's measured shape, for whoever lands it permanently: `attempt`,
`cadence`, `pool_rows`, `charged_seat_evaluations`, `charged_seat_refusal_rows`,
recorded **before** `preflight` so a refused attempt still reports; plus, on the
measurement side, `blocking_patch_count`, `repair_target_patch_count`,
`uncovered_sample_count`, the four censored objective components,
`uncensored_uncovered_mm2`, `plan_contact_count`, `nominated_rows`,
`eligible_rows`, `nomination_keys`. A `Vec<Row>` on the census, not a struct of
scalars.

### F.5 The cost of no cap, measured — REWRITTEN IN v2.1

This is the most important number in the document and it belongs in the open.
**v2 got both the number and the mechanism wrong**, and Craig has since closed
the question v2 left open. Both corrections are here.

**The rulings.** R6: `coverage_repair_max_attempts` bounds **committed steps
only**. **R10: there is no failed-tries cap** — the loop gives up only when no
candidate can physically help (pool exhaustion plus §B.3's zero-score guard), and
every run reports its trials and wall time loudly. **R11: no new registered
constant lands in this lane.** These are settled, not provisional.

**What it costs, measured — and v2's 489 was 13.9× too small.** A rejection is a
full prepare → route → rewire → brace → structure → lower → measure pass (§1,
"where verify runs"). v2 took the Warhorse's worst case from `islands.json` for
the two *blocked* islands, 300 + 189 = 489. That undercounts, because §H.1 keeps
the repair worklist **uncensored** (#27), so the pool that must be exhausted is
**every worklist route's** eligible rows, not the blocked ones'. Stage 0 measured
the real quantity across the 45-model corpus:

| model | eligible rows at one accepted state | eligible summed over the run | full pipeline passes today | status today |
| --- | ---: | ---: | ---: | --- |
| Awakened Tree / Weeping Treant | **22,705** | 22,705 | 1 | refuses |
| Giant Toad | **16,325** | 31,172 | 2 | refuses |
| Rotting Warhorse | 7,666 | 49,474 | 9 | refuses |
| **Warhorse** | **6,777** *(v2 said 489)* | 13,600 | 7 | refuses |
| Giant Bat | 6,138 | 27,789 | 8 | refuses |
| zombie-3-a | 4,279 | 9,213 | 7 | **seals** |
| Plague Zombie | 4,018 | 5,131 | 4 | **seals** |
| Crawler Zombie | 2,959 | 18,611 | 8 | **seals** |
| `make_raking_blade` | 18 *(v2 said 14)* | 46 | 7 | refuses |

**And the healthy models pay too, which v2 never priced.** On models that seal,
the per-attempt shape is `nominated` out of `eligible`: Crawler Zombie attempt 1
nominates **10 of 2,945**; Plague Zombie attempt 0 nominates **5 of 4,018**;
Deinonychus attempt 1 nominates **1 of 114**. The per-patch throttle is the only
thing keeping today's trial count at one batch per attempt. Rule N1 deletes it,
and without something else in its place a *healthy* model pays hundreds to
thousands of passes.

**THE MECHANISM — this is the part v2 missed, and it is what ruling R13
attacks.** Those 6,777 passes are not 6,777 different proposals. Scores tie on
absolute area, and the tie-break is pool order
(`coverage_closure.rs:686–693`), which is **spatially contiguous** because the
repair pool is built by walking the surface. The measured shape of that defect is
**63 seats inside a 1.04 mm box**: sixty-three consecutive candidates that are
physically one seat. The controller spends sixty-three full pipeline passes to
learn what the first one proved.

> So the cost is not "no cap, therefore expensive". The cost is **a search-order
> defect that a cap would have hidden rather than fixed.** That is exactly why
> R10 refuses the cap and R13 orders the fix: deprioritize, after a rejection,
> every candidate inside the rejected seat's own coverage disc (§A.4 rule N4).
> No new constant, no completeness lost, and it attacks the measured mechanism.

**Three mitigations, all in the design, none of them a counter.**

1. **Rule N4 — spatial diversity on rejection (R13).** The primary fix. §A.4.
2. **Zero-cost detection of #52.** Quarantine happens before routing, so a route
   whose contacts are all condemned as `overlap.exceeds_cutoff` can be marked
   `Blocked(StubReentry)` and withdrawn from the ranking **at nomination time**,
   without spending any trial. This removes RW island 74's rows from the worst
   case outright.
3. **Deprioritization on a repeated identical rejection.** When a route's trial
   is rejected with `Regression { component: UnroutedNoWinner }` and the CCO
   transcript says every feasible arm for that terminal was clearance-refused,
   push that route's remaining candidates to the bottom of the ranking rather
   than removing them. Exact, no threshold, no completeness lost. It composes
   with N4: N4 diversifies **within** a route's neighbourhood, this one
   diversifies **across** routes.

**What is NOT in the design, and why. AMENDED (v2.1):** v2 recommended
registering `coverage_repair_max_trials_per_commit` and filed it as open item
O-1, "the one item that gates stage 3". **Craig ruled against it (R10, R11).** A
trial ceiling is a counter deciding an outcome, and the physics-first doctrine
forbids that; it would also have hidden the 1.04 mm box rather than fixing it.
**O-1 is withdrawn**, and §A.8's loud reporting is what stands in its place: the
residual cost is printed on every run, so it is visible rather than capped.

**The honest residual.** N4 is expected to cut the worst case hard, and **it is
not measured yet.** The instrument is `diversity_demotions` in §A.8, and the
number it produces on the Warhorse and the treant is a Prove-stage result, not a
design claim. If N4 does not cut it, the answer is another measured mechanism or
a ruling from Craig — not a cap arriving by the back door.

### F.6 The explicit slot for #35's memoization, plus the larger rebuild v1 missed

#35's audit (closed, ruling recorded): each trial re-routes the entire cadence
from scratch, and **47.6% of three attempts' work is bit-identical
re-execution** because `discover_finalized_anchor_seats`' walk inputs are
independent of anything that changes between attempts. Charged incrementally,
the same distinct work peaks at **51.9% of the unmoved limit**. The ruling
deferred remedy A to this rebuild. Here is its slot:

```rust
/// Work carried across repair transactions within ONE run. Empty at attempt 0.
/// Survives rollback deliberately: a rejected trial routes the same cadence as
/// the accepted state, so its entries stay valid and the next trial reuses
/// them. That is where the win is, and under ruling R6 there are many more
/// rejected trials to win on than v1 assumed.
pub(crate) struct RoutingReuse<'a> {
    /// FINDING S9, and it is bigger than the seat cache.
    /// `SurfaceAnchorPool::from_geometry(&geometry, delicacy)` depends only on
    /// `geometry` and `delicacy` (`anchor.rs:303-306`), both invariant for the
    /// whole run, and it is rebuilt inside EVERY trial. That is a 494,617-row
    /// build per trial on the toad. Hoist it first: it is simpler than the seat
    /// cache, it is a larger win, and it scales with the trial count that
    /// rulings R6 and R10 leave unbounded.
    ///
    /// AMENDED (v2.1): at `a8feff4` there are **TWO** such builds per trial, not
    /// one — `runner.rs:2686` in `route_initial_cco_with_limits` and
    /// `runner.rs:3383` on the brace/structure path. Both read the same two
    /// invariant inputs. The hoist is therefore worth twice what v2 priced, and
    /// whoever lands it must take both sites or the win is halved silently.
    anchor_pool: SurfaceAnchorPool<'a>,
    /// Key: (terminal id, digest of the inputs the seat walk actually reads).
    /// The digest is what makes reuse safe rather than assumed; a mismatch is
    /// a miss, never a silent stale hit.
    seats: BTreeMap<(u64, u64), FinalizedAnchorSeats>,
    hits: usize,
    misses: usize,
}

impl PreparedSupportRun<'_> {
    fn route_initial_cco_with_limits(
        self,
        limits: RouteCcoWorkLimits,
        reuse: &mut RoutingReuse<'_>,   // <-- the seam
    ) -> Result<RoutedSupportRun<'_>, (SupportRunContext<'_>, SupportRoutingError)>;
}
```

Note: `SurfaceAnchorPool`'s `impl` block carries
`#[allow(dead_code, reason = "the pending controller consumes this bounded
authority seam")]` (`anchor.rs:298–302`, verified at `a8feff4`) — the codebase
already anticipates this controller taking ownership of it.

Two obligations attach to the seat cache and must be stated now so the later
change cannot skip them:

1. **The charge must become incremental with it, not after it.** The chargeable
   shape stays knowable before the first terminal, because the cache keys are
   known before the cadence starts:
   `RouteCcoWorkCharge::compute(cadence_misses, pool_rows)`. A charge that stays
   at `cadence × pool_rows` while the work is memoized would be a charge that
   prices a fiction — the opposite failure from FU-8b, but the same class.
2. **The receipt still binds.** `charge.covers_execution(meter)` must still hold
   with the reduced charge, so a cache miss storm refuses rather than silently
   overspends.

#35's remedy B (spatial nearest-k) needs a replacement charge design and is out
of this scope; the slot above does not prejudge it.

### F.7 The other work rows this touches

- **#46** (full sort to keep 40 at `anchor.rs:1091`, verified at `a8feff4`;
  ~15 MB of retained refusal rows per no-winner terminal): both are per-trial
  costs. Under rulings R6 and R10 the trial count goes **up**, so #46 gets
  *worse*, not better. **AMENDED (v2.1):** v2 said it "should be scheduled with
  the trial-bound decision"; R10 removed that decision, so #46 is now scheduled
  against the **measured** trial count §A.8 prints, and it is a real cost row for
  this lane's Prove stage rather than a hypothetical one.
- **#48: DISSOLVED by this controller, not untouched.** §F.2, corrected in v2.1
  against stage 0's per-attempt census: the treant's attempt-0 charge is 57.5% of
  the limit, and the 210% figure belongs to attempt 1 after a 2,008-row batch
  append.
- **NEW IN v2.1 — the legacy pre-routing replay is a per-trial cost this design
  did not price** (O-12, and stage 0 measured it). Each trial pays ~3
  `validate_binding` calls, each re-running a full baseline selection plus a
  coverage-sample scan over the whole model: **1,383 legacy-loop executions
  across the 45-model sweep** at today's trial counts. It belongs on §F.6's hoist
  list beside `SurfaceAnchorPool::from_geometry`, and it is invariant for the
  whole run by exactly the same argument — `mesh`, `generic.candidates()`,
  `generic.islands()` and `selection_config` never change.

---

## G. Migration

### G.1 Staged landing

**Stage −1 — #49 lands first or alongside** (ruling R8). **DONE. AMENDED (v2.1):
#49 landed in `b8edcb8` and is in this design's base, `a8feff4`.** §6's
unrouted-regular removal changes what the load partition sees and what
`unrouted_no_winner` counts, which is what §C's accept rule compares; landing it
after this controller would have meant re-freezing every digest twice. It did
not. §1 and §C.1 are amended to describe the landed shape, and §1 flags the one
half of §6 that #49 measured without enforcing.

**Stage 0 — measure first, change nothing. DONE at `06cf21b`.** It added the
per-attempt census/telemetry (§F.4), ran all 45 corpus models one per process,
and answered the four questions. **AMENDED (v2.1) with the answers:**

1. **How many repair contacts do the currently-emitting models actually need?**
   **Maximum improving rounds on any sealing model: 3** (`zombie-3-a`). Against a
   committed-step bound of 8 that is at least 5 rounds of headroom. **8 stands and
   does not need to move** (O-2 closed on the evidence). But 10 of 27 sealing
   models *append* more than 8 contacts today, up to 107 — rows today keeps
   because it keeps everything. Their digests move, which is §G.3's predicted
   correction.
2. **The toad's attempt-0 cadence** (§F.2's ≤ 1,077 question). **273**, charging
   25.2% of the limit. Answered YES with room.
3. **The #27 population's patch breakdown and contact demand.** `Skulker Zombie`
   1 improving round, `Tavey Badger` 1, `zombie-3-a` 3. None exceeds the bound.
4. **The trial-count distribution.** Measured, and it is §F.5's corrected table —
   the Warhorse's 6,777, not 489. v2 wanted this to set O-1's trial bound; **R10
   withdrew O-1**, so the same measurement now sizes rule N4's job instead.

**Stage 0's own findings that changed the design**, so they are not lost: the
treant's attempt-0 charge (57.5%, §F.2), the Warhorse's true pool (§F.5), the
O-7 plateau with Crawler Zombie as its named victim (§B.9), and O-12's verdict
(§L). **One stage-0 caveat that binds the Core stage:** every number above was
measured at `06cf21b`. `a8feff4` has since changed contact sets corpus-wide (the
tip minimum, grow-to-attest, the refinement scan, the scar bound and the reach
rule all move which contacts exist). **Contact counts, appended-row counts and
digests from stage 0 must be re-measured at `a8feff4` before they are used as
pins.** The *shapes* — plateaus, throttle starvation, pool sizes — are structural
and are expected to survive; the *values* are not to be trusted without a rerun.

**Stage 1 — the recoverable-trial seam. Pure refactor, no behaviour change.**
The six stage signatures and `prepared_run_for_plan` return
`(SupportRunContext, E)` on the error path (§A.3); `LoweredSupportRun` gains
`into_plan_and_context` (§A.2); the controller's `stage!` macro maps
`Err((context, error))` back to today's identical outcome. Every existing test
stays green, every digest stays put. Lands behind the current loop and is
independently reviewable. **This is the hand-written work — budget it, and do not
call it mechanical.**

**Stage 2 — evidence, still multi-nominate.** `CoverageMeasurement::objective`
and `::uncensored_uncovered_mm2` are exposed; `NoncoverageVector` is computed per
attempt and recorded; the pipeline-side gate drift test (§C.3) lands. The loop
still appends batches, so attribution is not yet possible — the values are
recorded, not enforced. This proves the vector's producers against real corpus
runs before anything depends on them.

**Stage 3 — the transactional loop.** Added as a second function
(`run_support_transactional`) called by the corpus harness and the fixtures,
with `run_support` retained for exactly one comparison cycle. **This is a
code-level switch, not a registered knob** — no constant, no config field, and
it is deleted in stage 4 of the same series. (Adding a runtime setting here
would be a knob that lets a defect hide behind a default, which FU-8b's doctrine
forbids.)

**Stage 4 — replace and delete.** `run_support` becomes the transactional loop;
the old body, `CoverageClosureAttempt`, and the retired reason variants are
deleted; the tests move as below.

### G.2 The frozen-pin list, re-grepped (finding S13)

v1's list was incomplete. Every row below was verified at `develop` @ `06cf21b`;
corrections to the reviewer's own citations are marked.

> **AMENDED (v2.1) — EVERY ROW RE-VERIFIED AT `a8feff4`. Read §G.4 before this
> table.** The substance of every row holds; the *citations* moved a great deal,
> and **one pinned digest value moved**. §G.4 is the from→to record and it is
> authoritative where it disagrees with a cell below. The line numbers written in
> the cells below are `06cf21b`'s and are kept only so the two documents can be
> diffed; **do not grep for them.**

| pin | where | disposition |
| --- | --- | --- |
| toed-slab digest `15_556_275_517_430_246_244` | **three sites, not one and not two**: `support_runner_integration.rs:305` (inside the `(64, 170, digest)` tuple), `:580` (`assert_eq!(snapshot.digest(), …)`), and **`plinth-verify/tests/coverage_gate_toed_slab.json:1`** as the literal golden `"snapshot_digest"`. Doc-comment mentions at `support_runner_integration.rs:209, 535` and `coverage_gate.rs:88, 126` are prose. | **UNCHANGED and load-bearing.** A model that closes at attempt 0 never enters a transaction. If any of the three moves, the change is wrong. |
| raking-blade half of `support_runner_integration.rs:266` | `attempts == 8`, `reason == AttemptsExhausted`, `history.len() == 9`, island 5 = 5.135 mm² / `NoRoutedContact` / id 5 / component 1 / face 14, `nominated_repair_rows > 0` | **Moves on the controller vocabulary, not the geometry.** `attempts` → trials/commits; reason → `NoImprovingRepair`; history → the trial stream. Every island number must hold exactly, and gains `Blocked(PoolExhausted)` (see §E.5 case 7). |
| `support_runner_integration.rs:448` overhang-table closure test | closes only after repair; blocking count non-increasing and strictly falls once | **Strengthened.** Monotone assertions become guarantees by construction; replace with "every committed step strictly improved `coverage_key`", "no committed step worsened any `NoncoverageVector` component", "every committed step strictly reduced uncensored uncovered area", plus the attribution assertion. |
| `support_runner_integration.rs:818` dust fixture | `coverage_repair_rounds() == 3`; the 4/3/1 nomination shape; every remaining patch censored on the sealed model | **Round count re-measured** (trials and commits, not attempts). The load-bearing half is unchanged. The mechanism assertion gets stronger: name the committing trial and prove the winner's `scored_from` carries the cross-route share. |
| `plinth-verify/tests/coverage_gate.rs` + its golden | the verdict predicate against the verifier, and the toed-slab golden | **UNCHANGED.** §H: the verdict predicates do not move. Note the golden folds `final_live_members_digest` into `verification_snapshot_digest`, so any change to the final live member set moves it transitively. |
| `plinth-pipeline/tests/prepare_folder.rs:553` | `assert_eq!(evidence.attempts, Some(8))` | moves with the `attempts` re-meaning below |
| `plinth-pipeline/tests/prepare_folder.rs:596` | `assert_eq!(sidecar["attempts"], 8)` — the **top-level wire field** | **format change, not a rename** |
| **`attempts` is FOUR struct sites, not one field** | (1) `coverage_closure.rs:369–374` `CoverageClosureRefusal { pub attempts: usize }` — **not Option**, serialized verbatim into the sidecar's `closure` object; (2) `coverage_evidence.rs:119–125` `CoverageRefusalEvidence { attempts: Option<usize> }`; (3) `coverage_evidence.rs:405–422` `CoverageSidecarBody { attempts: Option<usize> }` — the actual top-level JSON field, **not** `skip_serializing_if`, so it is always present; (4) `coverage_evidence.rs:424–438` `SidecarWrite` (internal, never serialized). Assertions at `coverage_evidence.rs:1285` (field 2), `:1289` (field 3), `prepare_folder.rs:553, 596`. `coverage_evidence.rs:1290–1292` round-trips the whole `closure` value, transitively covering the nested field 1. | **All four must be re-decided together.** Field 1 is the ultimate source of the other two. Re-meaning "attempts" as "trials" or "commits" is a **sidecar format change**; the FU-57 schema string `plinth.coverage-refusal.v1` should go to `.v2`. |
| `CoverageClosureReason` exhaustive match | **exactly one in the workspace**: `coverage_evidence.rs:1035–1038`. Enum defined at `coverage_closure.rs:238–283`, four variants, re-exported at `lib.rs:132–133`. | Retiring `AttemptsExhausted`/`NoEligibleRepairRows` breaks this match. |
| construction sites of the retiring variants | `AttemptsExhausted`: `runner.rs:817` (production), `coverage_evidence.rs:1171, 1265` (test fixtures), **`prepare.rs:2010`** (test fixture). `NoEligibleRepairRows`: `runner.rs:862` only. `RepairPlanRefused`: `runner.rs:936` only. `MeasurementRefused`: `runner.rs:779` only. | **Reviewer's `prepare.rs:1984` is off by 26 lines** — line 1984 is a doc comment; the construction is at `:2010`. Substance correct, citation corrected. |
| public API break | `CoverageClosureAttempt` (`coverage_closure.rs:347–362`, re-exported `lib.rs:132`), `coverage_repair_rounds()` (`runner.rs:522–525`), `CoverageClosureIsland` (`:314–340`), `CoverageClosureRefusal` (`:364–374`) | **Take the break, do not project.** v1 offered a back-projection to the old shape for one migration stage; it would encode "attempts" in a format that no longer has attempts and would have to be un-encoded a release later. `support_runner_integration.rs:465–493` reads `CoverageClosureAttempt`'s fields across the crate boundary and must be rewritten. |
| **`plinth-metrics` — reviewer's claim CORRECTED** | `disk_reader.rs:78–95` and `authority.rs:54–61` pin `make_overhang_table`'s **name and ordinal position** in an 8-model array, **not** resin or scar values. `disk_reader.rs:97–141`'s sibling `expected_failures` array pins `(exit_code, ProducerStage)` = `(3, Some(Emit))` for it. I found **no** resin-volume or scar-count value in `plinth-metrics` keyed to this model. | Moves only if the model's exit code or producer stage changes. The reviewer's "resin and scar rows move" is not supported by the source. |
| **`resource_cap_policy.rs` — NEW, missed by both v1 and the review** | `:295` `CORPUS_GATE_BATCHES` contains `("make_overhang_table", 50_176, 84)` and `:351` `CORPUS_LOAD_BATCHES` contains `("make_overhang_table", 12, 84)` — both pin the model's **contact count (84)**. | **Moves.** A repair-closing model that keeps only rows that earned their place will have a different contact count. Must be re-pinned with the measurement beside it. |
| OD-8 census digest septet | produced by `examples/embed_incidence_census.rs:246–255` (prints, does not assert) and hand-copied into `CURRENT-STATUS.md`. `penetration_census_equivalence.rs:476–495` cites `make_overhang_table`'s OD-8 digest `16396858890798947253` **as a model identifier only** and its own docstring says it "pins nothing about it and asserts nothing on it" — correctly excluded. | **Must be re-read**, and now for a second reason: §A.5 records that a rejected trial's census rows persist, so the census becomes a record of *work done*, not of the *accepted plan*. Enumerate in stage 0, do not discover in stage 4. |
| `runner.rs:7309` `a_stage_error_is_not_discarded_at_attempt_zero` | attempt 0 keeps `Failed { phase, detail }`; attempt ≥ 1 becomes `RepairPlanRefused` | **Half unchanged, half moves.** Attempt 0 is unchanged. The attempt-≥1 half becomes "a stage refusal on a trial is a ledgered rollback carrying the same detail", which is #47's fix. |
| `specialized.rs:10877` | a test setting `coverage_repair_max_attempts = 1` inside `synthetic_selection_is_baseline_only_not_the_legacy_repair_adapter` | re-read under ruling R6's meaning of the bound. |
| `specialized.rs:8116` | `hash.usize(config.coverage_repair_max_attempts)` — the bound is folded into the specialized-plan config digest | **If the bound's meaning changes but its value stays 8, this digest does not move.** Good; note it so nobody re-freezes it unnecessarily. |
| `specialized.rs` `fu8f_prefix_acceptance_matches_the_full_evaluation`, `gate_not_worse` tests | the merge tournament's acceptance rule | **UNCHANGED**, and now doubly load-bearing: the A4 accept test is paired to `gate_not_worse` by the new drift test (§B.4, and note what that test actually asserts). |
| **the legacy pre-routing repair loop — NEW, missed by both** | `lib.rs:1346, 1370` hold a *second* repair loop reading the same registered bound, reachable from production: public `select_contacts` (`lib.rs:1180–1191`) calls `select_contacts_with_mode(…, legacy_pre_routing_repair = true)`, and `select_contacts` is called from `validate_regular()` (`specialized.rs:2217`), itself called from `prepare_specialized_contacts` (`:1826`) and `SpecializedContactPlan::validate_binding` (`:1113`) — both outside `mod tests`. Its error type is `ContactSelectionError::UnsatisfiedCoverage`, not `CoverageClosureReason`. | **Flagged, not touched.** If the A4 controller changes what "coverage satisfied" means, this replay-validation path could start raising `SpecializedError::StaleRegularContacts` on models whose baseline selection now needs its own internal repair — a failure mode entirely outside the pin machinery. It needs its own audit in stage 0. |

### G.3 Blast radius, honestly

- **`run_support` is the single door to the sealed run.** Every corpus model's
  support behaviour goes through the loop being replaced. There is no partial
  exposure.
- **Digests move for repair-closing models, and only for those.** The sealed
  digest is a function of the plan, and the plan is a function of which repair
  contacts it holds. A model that closes at attempt 0 is bit-identical (the
  toed slab, and every model whose baseline covers). A model that closes after
  repair will generally close with a *different, smaller* contact set — because
  today it keeps every nominated row and here it keeps only rows that earned
  their place — so its digest moves. That is a correction, not a regression, but
  it must be re-frozen deliberately with the measurement beside it. **And under
  ruling R8, #49 lands first, so this re-freeze happens once, not twice.**
- **The commit bound is a behaviour change with emission risk.** If any currently
  emitting model needs more than the bound's worth of *individually improving*
  contacts, it stops emitting. Stage 0 measures this before stage 3 lands. This
  is the single largest emission risk in the change and I am not going to argue
  it away from the armchair.
- **The trial count is a behaviour change with a cost risk**, and it is new in
  v2 because ruling R6 created it. §F.5, open item O-1.
- **Six stage signatures change** plus `prepare_support_run` and any test calling
  a stage directly. Compiler-checked, no error type changes, **not** mechanical.
- **The FU-57 sidecar schema changes.** `attempts` is a wire field in four struct
  sites; re-meaning it is a format change and the schema string should move to
  `plinth.coverage-refusal.v2`.
- **Unblocks, in order:** #24 dissolves into §E.1; #47 becomes a ledger row;
  #41 gets the group-B precondition it waits on; **#51 and #52 get named,
  measured, per-route causes with millimetre shortfalls**, which is the input
  #26/PR-7 needs; #35's deferred remedy A gets the seam it was deferred to, plus
  the larger `SurfaceAnchorPool` hoist it did not name.
- **AMENDED (v2.1) — #48 IS TOUCHED, and v2's line was wrong.** v2 wrote "Does
  not touch: #48 (attempt-0 charge)". Stage 0's per-attempt census shows the
  treant's attempt-0 charge is **57.5% of the limit, inside it**, and the 210%
  figure v2 attributed to attempt 0 is **attempt 1**, after a 2,008-row batch
  append. One-winner rounds grow the cadence by at most `commit_budget` = 8 over
  the whole run, so the treant's worst charge is 58.1%. **#48's Routing refusal
  is caused by the batch nomination this design removes, so this controller
  dissolves it** (§F.2). What it does not claim is that the treant emits.
- **Does not touch:** #29 (toad Lowering), #36, #34.
  **#46 gets worse**, not better (§F.7).
- **NEW IN v2.1 — the base moved under the whole pin list.** Between `06cf21b`
  and `a8feff4` three lanes landed that change contact sets corpus-wide: **#49**
  (`b8edcb8`, the §6 strip and ownerless disclosure), **the reach rule** (#62,
  `fe8fd3c`, low-point adequacy by reach rather than tip area, retiring
  `MINIMA_SUPPORT_AREA_TIPS`), and **the tip arc** (#61, `a8feff4` — a 0.05 mm
  tip-radius minimum, grow-to-attest, the descending refinement scan, and Craig's
  0.400 mm `TIP_RADIUS_MAX_GROWN_SCAR_MM` scar bound). **None is this design's
  doing and none is reversed by it**, but every corpus contact count, appended-row
  count and digest measured before them is stale. §G.4 records the pin values;
  §G.1 stage 0 records which measurements need a rerun.
- **Travels in a separate lane:** #51 + #26 together (ruling R9). #51 is the
  clearance-mask fix this controller diagnoses; #26/PR-7 is the model-seated-arm
  work that depends on it. Neither is in this design's scope, and this design
  does not block on either — it refuses honestly without them and routes with
  them (§E.3).

### G.4 The base-move audit — `06cf21b` → `a8feff4`, every pin re-walked. NEW IN v2.1

Every citation in §G.2 and every `file:line` in §A–§F was re-grepped at
`a8feff4`. **Substance holds everywhere. One pinned VALUE moved; everything else
is a citation move.** This table is authoritative where it disagrees with §G.2.

**What moved the base:** #49 (`b8edcb8`), the reach rule (#62, `fe8fd3c`), and
the tip arc (#61, merged as `a8feff4` itself) — together +6,896 / −232 lines
across 29 files, including +1,715 in `runner.rs` alone, which is why nearly every
`runner.rs` line number in this document moved by roughly +450 to +1,100.

#### G.4.1 The one moved VALUE — the toed-slab digest

| | value |
| --- | --- |
| v2 wrote (at `06cf21b`) | `15_556_275_517_430_246_244` |
| **a8feff4** | **`4_190_462_360_230_124_963`** |

It moved **three times** and landed on a value bit-identical to an intermediate
one, which is a trap for anyone diffing carelessly. The chain, from the goldens'
own attribution comments (`coverage_gate.rs:140–260`):

1. **#49** — the verification snapshot is now taken over the ACCEPTED contact set
   (routed plan minus unrouted contacts, load repartitioned over the survivors),
   so `plan_digest` and `load_digest` move:
   `15556275517430246244 → 9355852079468421123`. Measured cause on this fixture:
   3 of 68 accepted contacts never route, and two were charged 450.000 mm² each —
   **900 of the model's 967.689 mm², 93.0%, was charged to contacts with no
   support under them.** 169 scalar leaves, exactly 1 moved.
2. **Tip minimum** (#61) — `TIP_RADIUS_DEFAULT_MM` 0.200 → 0.050 mm RADIUS
   (0.400 → 0.100 mm DIAMETER) moves `load_digest`:
   `9355852079468421123 → 4190462360230124963`. 169 leaves, exactly 1 moved.
3. **Grow-to-attest** — one quarantined contact now attests and routes; this one
   **moves counts**: routed 64 → 65, final live members 170 → 184, digest
   `4190462360230124963 → 15045390433036889889`.
4. **Descending refinement scan** — emits the smallest attesting radius rather
   than the first ladder rung: `15045390433036889889 → 14374106896419012910`,
   180 leaves, exactly 1 moved.
5. **The scar bound** (Craig, 2026-08-20, `TIP_RADIUS_MAX_GROWN_SCAR_MM` = 0.400
   mm RADIUS / 0.800 mm DIAMETER) — the rescued contact needed 0.650630996 mm
   radius, 1.63× the ceiling, so it is refused again. Counts go **back**: routed
   65 → 64, live members 184 → 170, digest
   `14374106896419012910 → 4190462360230124963`, **bit-identical to step 2's
   value.** The load that contact carried is 0.000 mm²; the price is coverage on
   two toe islands.

**Consequence for §E.5 case 6 and §G.2 row 1:** the toed slab's counts are back
at **64 routed / 170 live**, so v2's `(64, 170, digest)` tuple shape is intact and
only the digest changed. **The three pin sites, at `a8feff4`:**

| site | v2 cited | **a8feff4** |
| --- | --- | --- |
| the `(64, 170, digest)` tuple | `support_runner_integration.rs:305` | **`:341`** |
| `assert_eq!(snapshot.digest(), …)` | `support_runner_integration.rs:580` | **`:797`** |
| the golden `"snapshot_digest"` | `plinth-verify/tests/coverage_gate_toed_slab.json:1` | **unchanged path, value moved** |
| prose mentions (assert nothing) | `support_runner_integration.rs:209, 535`; `coverage_gate.rs:88, 126` | **`support_runner_integration.rs:209, 271, 294, 400, 728, 741, 784, 796`; `coverage_gate.rs:88, 126, 148, 166, 190, 257`** |

#### G.4.2 Every other pin — citation moves only

| pin | v2 cited (`06cf21b`) | **a8feff4** | substance |
| --- | --- | --- | --- |
| raking-blade refusal test | `support_runner_integration.rs:266` | fn at **`:302`**; `attempts == 8` **`:531`**; reason `AttemptsExhausted` **`:534`**; `history.len() == 9` **`:539`**; island 5 = 5.135 mm² **`:591`**; `nominated_repair_rows > 0` **`:602`** | holds |
| overhang-table closure test | `support_runner_integration.rs:448` | fn at **`:641`**; `coverage_repair_rounds() >= 1` at **`:658`** | holds |
| dust fixture (#27) | `support_runner_integration.rs:818` | fn at **`:1061`**; `coverage_repair_rounds()` assert at **`:1079`** | holds |
| `CoverageClosureAttempt` cross-crate read | `support_runner_integration.rs:465–493` | around **`:531–545`** | holds; must be rewritten |
| `evidence.attempts == Some(8)` | `prepare_folder.rs:553` | **`:691`** | holds |
| `sidecar["attempts"] == 8` | `prepare_folder.rs:596` | **`:734`** | holds |
| `attempts` struct site 1 (source of truth) | `coverage_closure.rs:369–374` | **`:377–378`**, `CoverageClosureRefusal { pub attempts: usize }`, still **not** `Option` | holds |
| `attempts` struct site 2 | `coverage_evidence.rs:119–125` | **`:119`** / **`:125`** | holds |
| `attempts` struct site 3 (top-level wire field) | `coverage_evidence.rs:405–422` | **`:412`** | holds, still not `skip_serializing_if` |
| `attempts` struct site 4 (`SidecarWrite`, internal) | `coverage_evidence.rs:424–438` | **`:426`** / **`:431`** | holds |
| **`attempts` site 5 — MISSED BY v2** | — | **`coverage_evidence.rs:921`**, the `persist(…, attempts: Option<usize>, …)` parameter, threaded at **`:936`**, **`:951`**, **`:1047`** | a signature, not a struct, but it is on the same edit |
| FU-57 schema string | (named, not cited) | **`coverage_evidence.rs:52`**, `plinth.coverage-refusal.v1` | must go to `.v2` |
| `attempts` assertions | `coverage_evidence.rs:1285`, `:1289`, `:1290–1292` | **unchanged** | holds |
| `CoverageClosureReason` exhaustive match | `coverage_evidence.rs:1035–1038` | **unchanged**, still exactly one in the workspace | holds |
| the enum itself | `coverage_closure.rs:238–283` | **`:240`**; `AttemptsExhausted` **`:244`**, `NoEligibleRepairRows` **`:261`**, `MeasurementRefused` **`:271`**, `RepairPlanRefused` **`:285`** | holds |
| `AttemptsExhausted` construction (production) | `runner.rs:817` | **`:1402`** | holds |
| `NoEligibleRepairRows` construction | `runner.rs:862` | **`:1447`** | holds |
| `RepairPlanRefused` construction | `runner.rs:936` | **`:1521`** | holds |
| `MeasurementRefused` construction | `runner.rs:779` | **`:1348`** | holds |
| test fixtures constructing the retiring variants | `coverage_evidence.rs:1171`, `:1265`; `prepare.rs:2010` | **`coverage_evidence.rs:1170`, `:1264`; `prepare.rs:2038`** | holds; v2's `:1984 → :2010` correction was right for its base |
| `coverage_repair_rounds()` (public API) | `runner.rs:517–526` | **`:974`** | holds |
| `CoverageClosureAttempt` (public API) | `coverage_closure.rs:347–362` | **`:355`** | holds |
| `CoverageClosureIsland` | `coverage_closure.rs:314–340` | **`:322`** | holds |
| `CoverageClosureRefusal` | `coverage_closure.rs:364–374` | **`:377`** | holds |
| the re-export | `lib.rs:132` | **`lib.rs:130`** | holds |
| `plinth-metrics` name+ordinal array | `disk_reader.rs:78–95` | **`crates/plinth-metrics/tests/disk_reader.rs:95`** (stage 0's path correction: `tests/`, not `src/`); `expected_failures` at **`:129–133`** | holds; v2's correction of the reviewer's "resin and scar rows move" stands — **no** resin or scar value is keyed to this model |
| `plinth-metrics` authority array | `authority.rs:54–61` | **`crates/plinth-metrics/src/authority.rs:58`** | holds |
| contact-count pins (84) | `resource_cap_policy.rs:295`, `:351` | **`crates/plinth-support/tests/resource_cap_policy.rs:295`, `:351`** — line numbers **unchanged**; stage 0's path correction: `tests/`, not `src/` | holds. **Re-measure rather than assume it moves** — stage 0 found the *pipeline* path now seals `make_overhang_table` at attempt 0 with 0 repair rounds while the *fixture* path still repairs, so the two disagree about whether this model repairs at all |
| OD-8 census producer | `embed_incidence_census.rs:246–255` | **`:248–255`** (prints, asserts nothing) | holds |
| OD-8 identifier-only citation | `penetration_census_equivalence.rs:476–495` | **unchanged** | holds; correctly excluded |
| attempt-0 discard test | `runner.rs:7309` | **`:8970`** | holds |
| `coverage_repair_max_attempts = 1` in the legacy-adapter test | `specialized.rs:10877` | **`:10915`** | holds |
| the bound in the plan-config digest | `specialized.rs:8116` | **`:8154`** | holds. **The value stays 8 and only its meaning changes, so this digest does not move** |
| `fu8f_prefix_acceptance_matches_the_full_evaluation` | (named) | **`specialized.rs:9199`**, doc references at `:3446`, `:8879`, `:8884` | holds |
| `gate_not_worse` | `specialized.rs:3385` | **`:3423`** | holds |
| `coverage_key` | `specialized.rs:3745` | **`:3783`** | holds |
| legacy pre-routing repair loop | `lib.rs:1346, 1370` | **`lib.rs:1349`** (`for attempt in 0..=config.coverage_repair_max_attempts`), **`:1373`**; the guard that skips it at **`:1340`** | holds |
| `select_contacts` (public entry to the legacy loop) | `lib.rs:1180–1191` | **`:1183`**; `select_contacts_baseline` **`:1198`**; `select_contacts_with_mode` **`:1217`**, its `legacy_pre_routing_repair` parameter at **`:1222`** | holds |
| `validate_regular` | `specialized.rs:2217` | **`:2240`** | holds |
| `prepare_specialized_contacts` | `specialized.rs:1826` | **`:1840`** | holds |
| `SpecializedContactPlan::validate_binding` | `specialized.rs:1113` | **`:1080`** | holds |
| `mod tests` start in `specialized.rs` | `:8279` | **`:8317`** | holds |
| production `select_contacts_baseline` call | `runner.rs:3777` | **`:4863`** | holds — production still builds `regular` legacy-OFF and replays it legacy-ON (task #53) |
| `SolidDecisionReason::HollowStageRefused` (stage-0 new row) | `prepare.rs:263`, `first_artifact.rs:428–451` | **unchanged** (`prepare.rs:263`; `first_artifact.rs:428`, `:448`) | holds, does not move under this design |
| `RepairPlanRefused` doc reference (stage-0 new row) | `first_artifact.rs:181` | **unchanged** | joins the retirement list |

#### G.4.3 Body citations outside §G.2, corrected

| what | v2 cited | **a8feff4** |
| --- | --- | --- |
| the repair loop | `runner.rs:661–898` | `run_support` at **`:1107`** |
| `failed: BTreeSet<RepairRowId>` | `runner.rs:649` | **`:1179`** |
| the batch append in the loop | `runner.rs:880` | **`:1467`** |
| `measure_coverage` | `runner.rs:1601–1658` | **`:2051`** |
| `into_context` | `runner.rs:2925` | **`:3907`** |
| the memory-critical property | `runner.rs:2913–2927` | **`:3896–3907`** |
| the plan behind the lowered run | `runner.rs:1613` | **`:937`**, chain repeated at **`:3932`** |
| `prepared_run_for_plan` | (named) | **`:4936`** |
| the five digest stamps | `runner.rs:3874–3878` | **`:4960–4964`** |
| `post_quarantine_forest_digest` | `runner.rs:3877` | **`:4962`** |
| `route_initial_cco_with_limits` destructure | `runner.rs:2076–2080` | **`:2661–2665`** |
| `initial.forest_mut()` | `runner.rs:2145` | **`:2734`** |
| `SurfaceAnchorPool::from_geometry` per trial | `runner.rs:2101` | **`:2686` AND `:3383` — two sites now, not one** |
| the routing charge | `runner.rs:2123` / `:2124` / `:2125–2130` | **`:2708` / `:2709` / `:2710`** |
| `record_route_cco_charge` | `census.rs:549–553` | **`:543–555`** |
| the per-patch pick cap | `coverage_closure.rs:657–670`, break at `:658` | **`:666–679`**, break at **`:668`** |
| the route-local scoring filter | `coverage_closure.rs:610–627` | **`:626–637`** |
| **the pool-order tie-break (R13's subject)** | — | **`coverage_closure.rs:686–693`** |
| seeded seat claims | `coverage_closure.rs:546–550` | **`:554–566`** |
| `nominate_repair_rows` | (named) | **`coverage_closure.rs:530`** |
| `KeyNomination` evidence | (named) | **`coverage_closure.rs:460`** |
| `CoverageClosureGap::NoRoutedContact` | `coverage_closure.rs:289–292` | **`:297–299`**, aggregation rule at **`:451–452`** |
| `CoverageObjective` | `lib.rs:1662` | **`:1665`** |
| `assess_coverage_with_mode` | `lib.rs:1902` | **`:1905`** |
| the censor comparison | `lib.rs:2010` | **`:2014`** (and the same predicate at **`:2045`**) |
| `eligible_repair_rows` | `specialized.rs:1000` | doc at **`:1000`**, fn at **`:1001`** |
| `append_regular_repair_rows` | `specialized.rs:1338–1349` | **`:1352`**; the one-row form `append_regular_repair_row` at **`:1339`** |
| `derive_child` | (named) | **`specialized.rs:1418`** |
| `SpecializedContactPlan` | `specialized.rs:921–946` | **`:921`** |
| `remove_demand_free_regular` | `specialized.rs:1259` | **`:1263`**; the batched `remove_demand_free_regular_rows` at **`:1289`**; **production caller at `runner.rs:3266`** |
| its five test callers | `specialized.rs:12755, 12808, 13271, 13413, 14337` | **`:12793, 12846, 13309, 13451, 14375`** |
| `retire_unrouted_terminal` | `cco.rs:1707`/`:1793` | **unchanged** |
| `FinalizedDispositionKind` | `cco.rs:719` | **unchanged**; variants at **`:721`**/**`:722`**; `transcript.rows()` at **`:908`** |
| `PreWriteGate` | `gates.rs:61–74` | **unchanged**; `ALL` at **`:77`** |
| `judge`'s arms | `gates.rs:282–312` | fn at **`:277`**, `UncoveredIslands` at **`:284`**, the eleven `NotYetPorted` at **`:302–312`** |
| `registered_coverage_request` | (named) | **`gates.rs:259`**, delegating to **`coverage_closure.rs:188`** |
| `ChainStop::is_support_refusal` | `prepare.rs:553`, `:550–559` | **`:541`**, `CoverageClosure` excluded at **`:546`** and **`:561`** |
| `CoverageRefusalEvidence` | `coverage_evidence.rs:118` | **`:119`** |
| the abandoned-change note | `coverage_evidence.rs:158–168` | **`:159–161`** |
| `anchor.rs` dead-code allow | `:298–301` | **`:298–302`** |
| `anchor.rs` from_geometry | `:303–306` | **unchanged**; `from_geometry_with_limits` at **`:314`** |
| #46's full sort | `anchor.rs:1091` | **unchanged** |

#### G.4.4 New surfaces `a8feff4` created that this design must carry

| surface | where | why it matters here |
| --- | --- | --- |
| `AcceptedContactSet` | `runner.rs:3029`; `plan()` at `:3059`; sealed digest at `:3052`, re-checked at `:3140` | the plan the controller carries forward is the **reduced** one (§A.2, §C.1) |
| `UnroutedRegularRemoval` | `runner.rs:2937` | the #49 ledger, with `carried_area_mm2` per removed contact |
| `AcceptedContactSet::ownerless_faces` / `perception::OwnerlessLoadFace` | `runner.rs:3051`; `perception.rs:172`, accessor `:196` | evidence only — §C.1 explains why it gains no component |
| `unrouted_specialized_contacts()` | `runner.rs:1003`; backing field `runner.rs:3032` | §C.1's `unrouted_specialized` producer, and §C.4's flagged disagreement |
| `abandoned_contacts()` / `AbandonedContactDisclosure` | `runner.rs:1027`; struct at `:547` | the tip arc's disclosure of load a refused contact abandons — **not** read by this design, recorded so it is not disturbed |
| `TIP_RADIUS_MAX_GROWN_SCAR_MM` | registry, evidence at `constants/evidence.rs:699` | Craig's 0.400 mm RADIUS scar bound; §H.2 adds it to the untouched list |
| `MINIMA_SUPPORT_AREA_TIPS` | **retired** (`f104cda`, #62) | §H.2 must not name a constant that no longer exists |

---

## H. What does not change

1. **The verdict predicates.** `coverage_patch_blocks_verdict` and
   `coverage_patch_needs_repair` are untouched, including the exact
   greater-than comparison at the censor boundary and the four-call-site table.
   The censor split of 2026-08-14 (#27) stands: **verdict censored, worklist
   uncensored**, and §B.1 explains why a censored *objective* is consistent with
   an uncensored *worklist*. §B.5's clause 3 is an **additional** accept
   condition; it does not alter either predicate.
2. **Every registered constant and limit.** `COVERAGE_RADIUS_FACTOR`,
   `TARGET_SPACING_MM` (R = 6.0 mm), `VOXEL_PITCH_MM` and the censor derived
   from it, `coverage_score_matrix_max_entries`,
   `synthetic_coverage_mask_max_words`, the 512 MiB phase peaks, the
   seat-evaluation and seat-refusal-row caps, `coverage_repair_max_attempts` = 8
   as a registered value, and **the 0.6 mm support-to-model clearance standoff**.
   Nothing is raised, relaxed, or restated. §E.3's reduced-standoff figures are
   diagnostic and are **not** a proposal. FU-8b's doctrine applies verbatim: if a
   limit is hit, the work is audited, never the limit.

   **AMENDED (v2.1) — the registry moved under this list, and this design still
   touches none of it.** Between `06cf21b` and `a8feff4`:
   `TIP_RADIUS_DEFAULT_MM` went 0.200 → **0.050 mm RADIUS** (0.400 → 0.100 mm
   DIAMETER) on Craig's ChituBox field practice; `TIP_DIAMETER_MIN_ALLOWED_MM`
   was superseded to **0.100 mm DIAMETER** with its derived twin
   `TIP_RADIUS_MIN_ALLOWED_MM` at 0.050 mm RADIUS;
   **`TIP_RADIUS_MAX_GROWN_SCAR_MM` = 0.400 mm RADIUS (0.800 mm DIAMETER) was
   newly registered** as Craig's scar bound; `TIP_RADIUS_MAX_LEGACY_MM` stayed at
   1.0 mm RADIUS; and **`MINIMA_SUPPORT_AREA_TIPS` was RETIRED** by the reach
   rule (#62), which replaced a tip-area criterion with a reach criterion
   (`reach ≤ K·h`). **All five are base state, none is this design's, and none is
   read differently by it.** They are listed so nobody reads this section as a
   registry snapshot from a base that no longer exists. **Ruling R11 binds this
   lane: no new registered constant, and no cap, gate, threshold or tolerance
   moves.** Rule N4's coverage disc uses `R = COVERAGE_RADIUS_FACTOR ×
   TARGET_SPACING_MM`, already registered and already the score radius; it
   registers nothing.
3. **`registered_coverage_request`** stays the one production request shared with
   `plinth_pipeline::gates::registered_coverage_request`, so the controller
   still closes to exactly what the pre-write gate will re-measure. This is why
   §1's "verify" reading is the expensive one.
4. **The three-seat validity rule** (plate, model surface including a cavity
   interior, or another support; a closed load path is the test). This design
   neither widens nor narrows it. #26/PR-7 remains a separate lane (ruling R9).
5. **Repair-pool identity and route provenance.** SPEC §3's one-route rule and
   §6's exact row identity are preserved; rows are still route-qualified, still
   immutable, still lineage-validated. Rule N2 changes how a row is **scored**,
   never which route **owns** it.
6. **Non-substitutability of coverage refusals.** A `CoverageClosure` refusal
   still does not reach the oriented-solid reserve
   (`prepare.rs:541`, the exclusion at `:546` and `:561`), so a
   coverage-refused model still writes no artifact. **The spec is silent on
   this** — it is a pipeline rule, not an A4 clause — so I am flagging the
   silence (open item O-4). The rule itself is correct and I recommend keeping
   it: substituting a solid after "the plan does not adequately support this
   model" would hand back a different object than the one that was judged.
7. **Attempt-0 semantics.** A stage failure on the model's own plan is still
   `SupportRunError::Failed { phase, detail }` with the real error in the
   detail, not a coverage refusal.

---

## J. What changed, and why

### J.1 What changed from v1 to v2

| # | change | source |
| --- | --- | --- |
| 1 | `UnreachableCause::NoRowWithinRadius` deleted. Termination reasons become measured defect classes: `ClearanceMaskUnsatisfiable` (#51), `StubReentry` (#52), `PoolExhausted`, `NoCandidateCoversAnything`, `NoPoolRows`. Never a distance claim. | B1 + Craig's ruling R7 + `island-out/islands.json` |
| 2 | Zero-score stop guard added to the loop, before the transaction. | B2 |
| 3 | Candidate scoring becomes **global** — every uncovered sample within R, on any route. Non-regression is the backstop. | B3 + Craig's ruling R3 |
| 4 | The `PreWriteGate` forgetting-proof moves to a **pipeline-side drift test**; `plinth-support` stays independent of `PreWriteGate`; the 11-of-12 `NotYetPorted` gap is written down and tied to #42. | B4 + Craig's ruling R4 |
| 5 | Rollback explicitly restores the five `context.digests` fields, with a five-stage test. | S4 |
| 6 | Stage error payload becomes `(SupportRunContext, E)`, not `(Self, E)`; the word "mechanical" is withdrawn. | S6 |
| 7 | Pin list re-grepped: three digest sites not two; four `attempts` struct sites; `resource_cap_policy.rs`'s contact counts; the reason-enum match; `prepare.rs:2010` not `:1984`; the `plinth-metrics` claim corrected; the legacy pre-routing repair loop flagged. | S13 |
| 8 | **New accept clause 3** — a committed step must strictly reduce total *uncensored* uncovered sample area. Closes the patch-splitting hole. | A7 |
| 9 | §1 states that "verify" is a full routed measurement inside the transaction at the registered gate radius (6.0 mm), with the cost consequence. | A1 |
| 10 | The registered bound counts **committed steps only**; rejections bounded by pool exhaustion + the zero-score guard. Recorded as provisional; §F.5 gives the measured cost (489 Warhorse passes) and leaves the override open. | Craig's ruling R6 |
| 11 | The Warhorse walk-through is rewritten with **both futures** — honest refusal naming #51/#52 today, emission with the same controller once they are fixed. | Craig's ruling R7 |
| 12 | The per-patch pick cap (`picked >= patches.len()`) is deleted; rule N3's first-trial guarantee replaces it; attention follows area because the score *is* an area. | the nomination-throttle finding + `islands.json` (571 untried seats) |
| 13 | RW island 68's near-horizontal face normal recorded, with the consequence that a near-horizontal arm may reach it once the mask is fixed. | `rottingwarhorse-island68.meta.json` |
| 14 | #49 sequenced **before or with** this controller; #51 + #26 recorded as a separate lane travelling together. | Craig's rulings R8, R9 |
| 15 | Warhorse evidence cited by name (`endpoint-out/judge-warhorse/`), with the sha256 and the tip; `CURRENT-STATUS.md:294`'s 0.6505 mm² identified as an older, different run. | Part 4 of the review |
| 16 | Third plan copy priced; `into_plan_and_context` added. | S5 |
| 17 | `SurfaceAnchorPool` hoist named as the largest per-trial rebuild, ahead of #35's seat cache. | S9 |
| 18 | The spec's "hard failure" downgrade argued from §7 rather than assumed, and split into accepted-state (hard failure) versus trial (rejection). | S10 |
| 19 | `scored_for: PatchKey` becomes `scored_from: Vec<(route, share)>` — per route, which is free; per patch is not. | S11 |
| 20 | `coverage_key` reuse priced (conversion + `pub(crate)`); the drift test's actual assertion stated. | S12 |
| 21 | Pipeline-pass accounting corrected from a claimed saving to a measured cost, with the real wins named separately. | S2 |
| 22 | Stage 0 must add a **per-attempt** census row; the existing single overwritten cell cannot answer the cadence question. | S3 |
| 23 | §E.5 case 7 (raking blade) corrected: under ruling R6 all 14 rows are tried, so the outcome is `PoolExhausted`, not `NotReached { 6 }`. | S1, resolved by R6 |
| 24 | New acceptance case 8: an anti-gaming fixture, without which clause 3 is untested. | A7 |
| 25 | The public-API break is **taken** rather than projected back to the old shape. | S13 |
| 26 | v1's §I ("For Craig — where the spec is ambiguous or silent") is retired as a section. Its decided items became **§K** (the rulings), its undecided items became **§L** (open items), and the letter I is not reused. | housekeeping |

### J.2 What changed from v2 to v2.1 (2026-08-20, base `06cf21b` → `a8feff4`)

| # | change | source |
| --- | --- | --- |
| 27 | **No failed-tries cap**, stated in §1 and §A.7; the stop list gains nothing and loses nothing. O-1's trial ceiling **withdrawn**. | Craig's ruling **R10** |
| 28 | **§A.8 is new — the loud-reporting contract.** Trials, commits, rejection histogram, total and per-trial wall time, candidates remaining, diversity demotions, to the run log, the FU-57 sidecar and the run report. Explicitly: these counters report, none decides. | R10 |
| 29 | **No new registered constant in this lane**, restated in §H.2 with the registry moves that happened under it. | R11 |
| 30 | **§B.9 is new — the O-7 mandate.** Investigate the flat rounds in code before implementing §B; two hypotheses, the telemetry that separates them, the exact code sites; Crawler Zombie must still emit; resolution lands as **§B-O7** during Core. §B.8 now points at it, O-7 is promoted from "known limit" to top item, §E.5 gains case 10. | Craig's ruling **R12** + stage 0 |
| 31 | **§A.4 rule N4 is new — spatial diversity on rejection.** Deprioritize candidates inside a rejected seat's own coverage disc; no constant, no completeness lost. §2 gains the measured mechanism (63 seats in a 1.04 mm box), §A.7 the loop step, §D.1 and §A.8 the instruments, §E.5 case 11 the fixture. **v2's "the rerank after a rejection is provably a no-op" is withdrawn** — scores still are, order is not. | Craig's ruling **R13** |
| 32 | §E **verified** against R14 and found already conformant; the three re-checks (the distance-in-a-class question, the counters behind `ClearanceMaskUnsatisfiable`/`StubReentry`, and the `NoRoutedContact` name collision) are recorded rather than claimed as changes. | Craig's ruling **R14** |
| 33 | **§1's clause-2 paragraph rewritten: it is WRONG at `a8feff4`, not stale.** #49 landed; production strips unrouted regulars at `runner.rs:3264–3266`; the singular `remove_demand_free_regular` is still test-only but the **batched** form is production. §G.1's stage −1 is discharged. | `b8edcb8` |
| 34 | **§1 and §C.4 flag the half of §6 that #49 measured without enforcing** — an unrouted specialized minimum is ledgered, not refused. §C.4's accepted-state rule is **deferred, not adopted**; O-5 becomes a two-part question with a corpus run attached. | `runner.rs:1003` |
| 35 | §C.1 gains the post-#49 producers (`AcceptedContactSet::specialized_unrouted`, the reduced plan) and **deliberately gains no ownerless-face component**, with #49's own physics-first reason written out. §D.1 carries ownerless faces as evidence instead. | `b8edcb8` |
| 36 | **§F.2's treant paragraph corrected — v2 was wrong, in the design's favour.** Attempt-0 charge is **57.5%** of the limit, not 210%; the 210% is attempt 1 after a 2,008-row batch. **#48 is dissolved by this controller**, and §G.3's "Does not touch: #48" is corrected. §F.2's toad question is answered too: attempt-0 cadence **273**, 25.2% of the limit. | stage 0's per-attempt census |
| 37 | **§F.5 rewritten. The Warhorse worst case is 6,777, not 489** — v2 counted the two blocked islands, but the uncensored worklist means the whole worklist's eligible rows. **And the mechanism is the diversity defect, not the absence of a cap.** The healthy-model cost (10 nominated of 2,945 eligible) is priced for the first time. | stage 0 + R13 |
| 38 | **§G.4 is new — the complete `06cf21b` → `a8feff4` from→to record.** One pinned VALUE moved (the toed-slab digest, through five steps, landing bit-identical to an intermediate); every other row is a citation move; five new surfaces the design must carry are listed. | the base-move audit |
| 39 | O-2 **closed** (max improving rounds on any sealing model = 3; 8 stands) and O-3 **measured** (no sealing model exceeds 8 routes; every heavy refuser does, tree 103). | stage 0 |
| 40 | O-12 **audited**: v2's `StaleRegularContacts` worry does not materialise, but a different live trap does (production builds `regular` legacy-OFF and replays it legacy-ON, task #53), and the replay is a real per-trial cost §F.7 now carries. | stage 0 |
| 41 | §F.6: **two** `SurfaceAnchorPool::from_geometry` builds per trial at `a8feff4`, not one — the hoist is worth twice what v2 priced, and both sites must be taken. | `runner.rs:2686`, `:3383` |
| 42 | §A.1 notes that `append_regular_repair_row` (the one-row form) already exists at `specialized.rs:1339`, so the controller re-implements no append path. §A.2 notes the plan it must carry forward is the **reduced** one. | `a8feff4` |

---

## K. The rulings this design implements

Recorded so the next reader knows which decisions are Craig's and which are mine.

| id | ruling | status |
| --- | --- | --- |
| **R2** | §6's "failing route" reads as "route on the repair worklist". | carried from v1; unchallenged |
| **R3** | Candidate scoring credits cross-route coverage: **score globally**, with the non-regression check as the backstop. | Craig, on B3 |
| **R4** | The clause-2 forgetting-proof lives at the **pipeline layer**. `plinth-support` must not depend on `PreWriteGate`. The 11-of-12 `NotYetPorted` gap is recorded and tied to **#42**. | Craig, on B4 |
| **R5** | A stage refusal on a trial is a rejection — roll back, ledger the row and its stage, continue. | Craig, on v1 gap 5 / #47 |
| **R6** | `coverage_repair_max_attempts` bounds **COMMITTED steps only**. Rejections are bounded by pool exhaustion plus the zero-score guard. | Craig. **AMENDED (v2.1): no longer provisional.** R10 and R11 close it; §F.5 rewritten; O-1 withdrawn. |
| **R7** | No "unreachable" verdict. §E's honest-termination reasons are the **measured defect classes** — unsatisfiable clearance mask (#51), stub re-entry (#52), pool exhaustion — never a distance claim. Both futures stated. | Craig, on B1 + the island evidence |
| **R8** | **#49** (§6's unrouted-regular removal) lands **with or before** this controller. | Craig, on the review's Part 5 item 1 |
| **R9** | **#51 + #26** travel together, in a **separate lane** from this controller. | Craig |

**NEW IN v2.1 — Craig's rulings of 2026-08-19/20, which bind this build.** Each
one is implemented in the section named, not appended here.

| id | ruling | implemented at |
| --- | --- | --- |
| **R10** | **NO failed-tries cap.** The loop gives up only when no candidate can physically help — pool exhaustion plus the zero-score guard. **Counters must never decide outcomes** (physics-first doctrine). Every run reports its tries and its wall time **LOUDLY**. | §1 (the attempt-cap clause), §A.7 (no trial cap in the stop list), **§A.8** (the reporting contract), §F.5 (the rewritten cost), §L (O-1 withdrawn) |
| **R11** | `coverage_repair_max_attempts` counts **COMMITTED steps only**. **NO new registered constant in this lane.** No cap, gate, threshold or tolerance moves. | §1, §A.7, §F.5, §H.2, §L (O-1 withdrawn, O-2 closed on stage-0 evidence) |
| **R12** | **O-7, the flat rounds.** 9 of 12 repair-sealing models historically sealed only through rounds that improved nothing measurable. **Investigate in code WHY before implementing §B.** If a finer measured quantity moves during them, define the strict-improvement objective at that granularity, **existing quantities only**. If the flat rounds were dead weight from per-patch nomination throttling, clause 1 stands and the global scoring must surface the sealing rows immediately — **SHOW WHICH**. **Crawler Zombie must still emit** (proven in Prove). The resolution lands as design addendum **§B-O7** during Core. | **§B.9** (the mandate, the two hypotheses, the discriminating telemetry and where to read it) and **§B-O7** (the discharged resolution, measured 2026-08-20: hypothesis 2, quarantine, clause 1 unchanged, the nine sealing rows named by pool identity), §E.5 case 10, §L (O-7 closed as a mandate) |
| **R13** | **Spatial diversity on rejection.** Scores tie on absolute area and the tie-break follows pool order, which is spatially contiguous — the measured mechanism behind 63 seats in a 1.04 mm box. After a rejected trial the next candidate must be spatially diverse, by a rule derived from existing geometry (outside the rejected seat's coverage disc) and **not** a new constant. | **§A.4 rule N4**, §A.7 (the loop), §A.4 "Reranking" (v2's no-op claim withdrawn), §D.1 (`diversity_discs_containing_seat`), §A.8 (`diversity_demotions`), §2, §F.5, §E.5 case 11 |
| **R14** | **Termination reasons are measured defect classes** — `ClearanceMaskUnsatisfiable`, `StubReentry`, `PoolExhausted`, `NoCandidateCoversAnything`, `NoPoolRows` — **never distance claims**. Honest refusal beats fake progress. | **§E** (verified, already conformant; the three re-checks are recorded there), §E.1 |

**How R10–R14 relate to R6 and R7.** R10 and R11 finish R6 — they answer the
question §F.5 left open and close it against a cap. R14 restates R7 and this
design already satisfied it; §E records the verification rather than claiming
credit for a change it did not have to make.

---

## L. Open items for the implementation lane

Everything below is either a question the spec does not answer or a decision that
needs a measurement first. None of them is implemented on a guess.

**O-1 — the trial bound. WITHDRAWN by ruling R10. AMENDED (v2.1).** v2 called
this "the one item that gates stage 3" and recommended registering
`coverage_repair_max_trials_per_commit`. **Craig ruled against it.** There is no
failed-tries cap; a counter may not decide an outcome. Two corrections to v2's
framing go with the withdrawal: the worst case is **6,777** on the Warhorse, not
489 (§F.5), and the mechanism is a **search-order defect** — 63 near-identical
seats in a 1.04 mm box — that a cap would have hidden. **What replaces O-1:** rule
N4 (R13) at the mechanism, plus §A.8's loud reporting so the residual is visible.
**The residual is not yet measured**, and that is the honest open part: N4's
effect on the Warhorse and the treant is a Prove result. Recorded as **O-1′
(open, measurement)**: report `diversity_demotions` and the trial count on every
model, and bring the numbers back — not a proposal for a cap.

**O-2 — the commit bound's value. CLOSED on stage-0 evidence. AMENDED (v2.1).**
R6 fixed the bound's *meaning*; stage 0 measured whether 8 is right for it.
**Maximum improving rounds on any currently-sealing model: 3** (`zombie-3-a`),
so 8 has at least 5 rounds of headroom. **8 stands and does not move** (which R11
requires anyway). Two notes carried forward: v2's worry about `zombie-3-a`'s
12.65 mm² not closing in one contact is answered — it closes in 3 improving
rounds — and finding S7's "the dust fixture needs 8 committed rows with zero
margin" counted *appended* rows, not improving ones. **The emission risk was never
the bound's value; it is O-7's plateau** (§B.9).

**O-3 — the first-trial guarantee versus the commit bound. MEASURED, and it
splits. AMENDED (v2.1).** Stage 0 counted the distinct worklist routes at attempt
0. **No currently-emitting model exceeds the bound**: the maximum is 8 (Morel
Mudskipper), exactly at it, then 7, 6, 4, 3, 3, 3, 3, 2, 1, 1, 1. So rule N3's
guarantee starves nothing that emits today. **Every heavy refuser exceeds it
badly**: Awakened Tree **103** routes, Giant Toad **54**, Giant Bat **51**,
Rotting Warhorse **38**, Warhorse **27**, Cylinder 23, Morel base 19, Cat (wild
shape) 13. With a commit budget of 8, at most 8 of the tree's 103 routes can ever
receive a contact. **O-3 is therefore not an emission risk and IS a real limit on
exactly the models this controller is meant to unblock.** It stays a ruling for
Craig — a reserved share of the budget for the largest route, or an accepted
limit — and it is not an implementation detail. **It is not urgent**: it binds
only models that refuse today either way.

**O-4 — the spec is silent on what the pipeline does with a coverage refusal.**
Non-substitutability lives in `prepare.rs`, not in SPEC-A3-A4. It is currently
right and §H.6 keeps it, but it is house rule rather than contract. Worth one
clause if you want it protected.

**O-5 — the "hard failure" scope. AMENDED (v2.1) — the base has since answered
half of it, and the other way.** §C.4 reads SPEC §6's "an unrouted specialized
minimum or island-birth contact is a hard failure" as a statement about the
**accepted** plan, so a trial with one is a rejection and an accepted state with
one is a hard failure. **At `a8feff4`, #49 measures and ledgers it WITHOUT
refusing** (`runner.rs:1003`, in the function's own words). So:

- **the trial half is adopted** — nothing contradicts it, and §7 supports it;
- **the accepted-state half is DEFERRED**, because adopting it would be an
  unmeasured behaviour change on top of a deliberate #49 decision (§C.4).

**Two questions for Craig, in order.** (a) Is #49's measure-and-ledger the
intended reading of §6's "hard failure", or a staging step? (b) If it is a
staging step, **how many corpus models does turning it on refuse?** That is one
corpus run, and it must be answered before the rule is adopted, not after.

**O-6 — clause 3 as a spec amendment.** §B.5's anti-gaming clause is a house rule
that closes a hole §4 and §6 jointly leave open. One sentence in §6 — "a
committed step must also strictly reduce total uncovered sample area,
uncensored" — makes it contract. Recommended.

**O-7 — A4 is a greedy hill-climb and cannot cross a plateau. RESOLVED
2026-08-20 as design addendum §B-O7 — hypothesis 2, measured on Crawler Zombie
at `a8feff4`: the flat rounds' 50 contacts were condemned at quarantine and
never routed (`routed` pinned at 63 for four consecutive rounds), so clause 1
stands unchanged and no quantity is refined. The model needs exactly ONE
improving contact. What remains open is only the Prove-stage emission check.**
The v2.1 text below is kept for the record.

**AMENDED (v2.1) — this is the top item now, and it has a named victim.**
v2 filed it as "known limit, not a surprise". Stage 0 measured it and it is a
surprise: **9 of the 12 repair-sealing models sealed only by crossing at least
one round that improved nothing measurable**, and **Crawler Zombie plateaus for
five consecutive rounds at 16.625 mm² failing / 16.825 mm² uncensored before
closing in one**. A strictly-improving controller stops at round 2. **That model
emits today.**

**Craig's ruling R12 is the resolution path**, and it is the implementer's
mandate, not an open question for Craig: investigate **in code** why the flat
rounds seal, before implementing §B; refine the objective at a finer *existing*
quantity if one moves, or keep clause 1 and **show which rows** the global
scoring surfaces immediately if the flat rounds were throttle dead weight; and
**Crawler Zombie must still emit**, proven on the model. **The resolution lands
as design addendum §B-O7 during Core.** §B.9 carries the full mandate, the two
hypotheses, the telemetry that distinguishes them and the exact code sites to
read. **This is the single largest emission risk in the change.**

**O-8 — "worst gap" has no contract.** §6 clause 1 names it; §4 defines samples,
patches, coverage and failure but no gap. It is a house quantity end to end,
including that today it folds over **all** samples of failing islands rather than
uncovered ones (`lib.rs:2062–2071` at `a8feff4`: the fold filters on
`failing_islands.binary_search(&sample.island_id)`, not on the sample being
uncovered) — equivalent only because covered samples are
within R by definition. The `+inf` / `NoRoutedContact` ordering is consistent in
both implementations and is recorded as a house reading.

**O-9 — a failed row stays failed forever.** §6 excludes "exact failed rows", and
this design excludes them permanently, which is the plain text. But a row that
failed against accepted state A may improve accepted state B, so the search is
order-dependent in a way the spec never acknowledges. Recorded, not changed.

**O-10 — §7 gives Phase-8 R2 a global per-sweep commit bound; §6 gives A4 none.**
Whether A4 inherits any per-sweep bound is unstated, and it interacts with O-2.

**O-11 — the spec never says the repair pool is frozen for the run.** §7 says
Phase-8 "never reconstructs the regular pool implicitly"; A4's pool freeze is
house behaviour. It matters for #26: if model-seated arms add rows, the spec does
not say whether they may enter mid-run or only at the next run.

**O-12 — the legacy pre-routing repair loop. AUDITED IN STAGE 0. The stated risk
does NOT materialise; a different, live one does. AMENDED (v2.1).**
`lib.rs:1349, 1373` hold a second repair loop reading the same registered bound,
reachable from production through `validate_regular` (`specialized.rs:2240`),
itself reached from `SpecializedContactPlan::validate_binding`
(`specialized.rs:1080`) and `prepare_specialized_contacts`
(`specialized.rs:1840`). Measured: **1,383 executions across the 45-model sweep**,
roughly 7 replays of baseline selection per attempt.

- **No collision with this controller.** `validate_regular` compares its replay
  against `self.regular`, the **baseline** `SelectedContacts` frozen in
  `SupportRunContext`. The A4 controller never touches that value — its repair
  rows go into the specialized plan's repair pool. **It cannot start raising
  `StaleRegularContacts` because of anything this design does.** v2's stated
  worry is withdrawn.
- **A live latent defect it does have, which v2 did not name.** Production builds
  `regular` with `select_contacts_baseline` (legacy **off**, `runner.rs:4863`)
  and replays it here with legacy **on**. The two agree **only when the legacy
  loop takes zero steps** —
  `specialized.rs:10868`'s own test proves they diverge when it fires (baseline 1
  contact, legacy 2, `assert_ne!`). So **any model on which the legacy loop takes
  even one repair step dies at Preparation with
  `SpecializedError::StaleRegularContacts`.** Measured across all 45 models: **0
  legacy repair steps on every one of the 1,383 executions.** The loop is a no-op
  by survivorship, not by design, and nothing in the workspace pins the
  asymmetry. **This is task #53** and it is outside this lane.
- **It IS a per-trial cost this design must price** (§F.7): each trial pays ~3
  `validate_binding` calls, each a full baseline selection plus a coverage-sample
  scan, and the trial count goes up under R6+R10. It belongs on §F.6's hoist
  list; it is invariant for the whole run by exactly the same argument as
  `SurfaceAnchorPool::from_geometry`.

---

## M. What the Core build measured (2026-08-20)

Written by the implementer at the end of Core, against the code that landed.
**This section is measurement and disclosure only. It changes no rule.** Where
it corrects an expectation the body of this document set, it says so in place.

### M.1 The three procedural fixtures, gate profile

Every number below is from `cargo test -p plinth-support --profile gate --test
a4_transactional_controller`, one process, procedural meshes built from
`plinth_mesh::make_*` against the checked-in `corpus/baseline/*.pipeline.json`.
**No mini was read**, so this runs on a clean worktree (task #16).

| fixture | trials | commits | outcome | what it establishes |
| --- | ---: | ---: | --- | --- |
| `make_toed_slab` | **0** | **0** | seals, digest `4_190_462_360_230_124_963`, 64 routed / 170 live — all three unmoved | §E.5 case 6 and §G.3: a model that seals at attempt 0 never enters a transaction, so no rule in §A–§E can reach it |
| `make_overhang_table` | **1** | **1** | seals | §G.3's predicted correction, measured: the batch loop takes three rounds appending 4/3/1 — **eight contacts** — where one contact that earned its place is enough |
| `make_raking_blade` | **45** | 0 | refuses, pool drained, 26.2 s wall | §E.5 case 7. Refuses the SAME hole — island 5, one patch, 5.135 mm², 32× the censor — and now names a measured class |

**The raking blade corroborates stage 0.** Stage 0 measured 18 eligible rows at
one accepted state and **46 summed over the run** at `06cf21b`; the one-winner
controller tried **45** at `a8feff4`. The shape survived the base move, as §G.1
predicted the shapes would.

### M.2 The FIRST measurement of rule N4 on real geometry

§F.5 said: "N4 is expected to cut the worst case hard, and **it is not measured
yet**. The instrument is `diversity_demotions` in §A.8." Here is the first
reading, on the raking blade:

```
coverage-repair: trials=45 commits=0 remaining=0 wall_total=26189ms
  wall/trial min=575ms med=581ms max=586ms
  diversity_demotions=44 of_which_tried=44 [no_improvement=45]
```

**44 candidates were deprioritized and all 44 were tried anyway.** That is
ruling R13's completeness promise — "deprioritization is not exclusion" —
measured on a model rather than argued from the rule. It also says plainly what
N4 did NOT do here: this pool is 45 rows in one small neighbourhood, so
reordering it cannot avoid any of them and the trial count is the pool size. N4
changes the ORDER, and on a pool with nothing outside the discs there is no
cheaper order to find. **The Warhorse and the treant are where the cut, if any,
will show, and those are Prove-stage runs.**

### M.3 Corrections to this document's own expectations

1. **§A.4 rule N4's proof obligation was satisfiable by a fixture that proved
   nothing, and the first version of it was.** The obligation reads "one test
   that rejects a seat and asserts the next winner lies outside that seat's
   coverage disc while an equal-scoring neighbour inside it exists". Written
   with two candidates, the rejection leaves exactly one live row and the
   assertion passes whatever the sort key is. A sabotage run — zero out N4's
   contribution to the rank key, expect red — caught it green. **The fixture
   needs THREE rows so that two are live after the rejection.** Recorded here
   because the obligation as written does not say so, and the next reader of
   §A.4 would write the same broken fixture.
2. **§A.5's digest restore has no red run, and none is available today.**
   Removing `restore_prepared_digests` from the rejection path leaves every test
   green, because `prepared_run_for_plan` re-stamps all five fields
   unconditionally and nothing between trials reads them. §A.5's stated reason —
   "the controller assembles refusal evidence *between* trials" — is a property
   of a future reader, not of this controller. **The restore is implemented and
   is DEFENSIVE, not proven.** It is five `u64` writes; it stays.
3. **§C.3's clause-2 gap is now pinned at its measured size**: seven of the
   eleven clause-2 gates have no component at all, three are matched by a house
   count, and `Clearance` is covered only indirectly by the trial's stage
   refusing. `plinth-pipeline/tests/a4_noncoverage_gate_drift.rs` reads which
   gates are ported from a REAL run rather than mirroring `judge`'s match, so
   the two lists cannot drift.

### M.4 What Core did NOT do, and why

* **§G.1 stage 4 is not started.** `run_support` is untouched and still the
  production door; the transactional loop landed as `run_support_transactional`,
  which is stage 3 verbatim. The swap needs the `attempts` re-meaning at five
  struct sites, the FU-57 schema string to `plinth.coverage-refusal.v2`, and a
  deliberate digest re-freeze **with the measurement beside it** — a corpus-run
  obligation, not a code one.
* **The §D.1 trial stream is in memory, not on the wire.** Persisting it needs
  `RepairRowId` to gain a public wire encoding it has never had, which belongs
  to the same stage-4 format change. It is exposed on the sealed authority and
  is empty on the refused path, where §E.1's per-route outcomes and §A.8's
  report carry the evidence instead.
* **§A.8's third destination is not wired.** The operator line and the sidecar
  copy are here; the run report's per-model line is `plinth-pipeline` wiring and
  travels with stage 4. The operator line goes to **stderr**, because
  `plinth-support` has no logging dependency and adding one to print one line is
  not a trade this lane should make.
* **Two loop guards are unexercised by any fixture**: §B.3's zero-score stop
  (no procedural fixture has an all-zero-scoring pool) and
  `CommitBudgetExhausted` (none needs more than the bound). Both are reachable
  by construction and neither is proven. Prove should look for a model that
  reaches each.

---

## N. The challenger's REJECT, and what each finding cost the design (2026-08-20)

Written by the implementer after the adversarial review of `d3bdbb2` returned
**REJECT**. Three findings blocked and four asked to be fixed or disclosed. All
seven are addressed below, each with the sabotage run that now goes red — a
finding whose fix has no red run is called out as unproven rather than claimed.

### N.1 BLOCKING 1 — the search abandoned a 13.68 mm² candidate (ruling 1)

**MEASURED by the challenger** on `make_overhang_table`, gate profile, with the
first trials forced to reject: the loop stopped after three rejections reporting
`NoImprovingRepair { best_remaining_score_mm2: 0.0 }` while **1,008 rows were
still live and the best of them covered 13.68 mm²**. Craig's ruling 1 says the
loop gives up only when no candidate can physically help. Two composing causes,
both fixed:

**(a) The rank key put the disc count ahead of score.** §A.4 N4 states the rule
twice and the two sentences do not agree — "it sorts after every candidate
outside that disc, **at equal score**" and "the sort key is the count …
**applied before score**". Only the first is safe. **905 of 1,011 ranked rows on
the overhang table score 0.00 mm²** (measured), because a worklist route's pool
spans its whole island while the uncovered samples are local; with the count
leading, a seat covering **nothing** outranks a live 13.68 mm² seat the moment
the good seat falls inside one rejected disc, and the search then buys hundreds
of full pipeline passes that can improve nothing.

**Resolution: score leads, the disc count is the first tie-break.** Area is the
physical quantity; the disc count is a statement about redundancy, and
redundancy may decide only what area has left tied. This closes the whole of the
measured defect, because that defect *is* a tie: the 63-seats-in-a-1.04 mm-box
run ties on absolute area and is ordered by spatially contiguous pool order, and
§B-O7.5's Crawler Zombie mechanism is a **2,304-way exact tie**. It is the
weaker of the two readings in one respect, stated plainly: a run of seats whose
scores are *close but not equal* is not reordered, so N4 does not break spatial
contiguity there. No measurement shows that case; the measured one is the tie.

**(b) The §B.3 guard tested the WINNER, not the best eligible candidate.** §B.3's
own words are "when the **best eligible candidate's** global score is
0.00 mm²"; §A.7's pseudocode said "winner", and the code took the branch that
breaks. It now measures the best live score over the ranking, uses it for the
guard, and **reports it** — so `NoImprovingRepair` naming 0.00 mm² is a
measurement rather than a claim. Both §A.4 N4 and §A.7 are amended in place.

Red under sabotage: putting the disc count back ahead of score fails
`a_seat_that_covers_nothing_never_outranks_a_seat_that_covers_something` and
`while_any_live_row_covers_something_the_winner_covers_something`.

**What is NOT claimed.** `BlockedCause::NoCandidateCoversAnything` was
re-examined and is sound as it stands: it reads the best **global** score over
the route's own rows, so it is claimed only when that route's rows cover nothing
anywhere. §M.2's "44 of 44" reading is corrected in §N.6.

### N.2 BLOCKING 2 and BLOCKING 3 — rules N1 and N2 had no test at all

The challenger reverted **both** rules with the whole suite green: re-adding the
per-patch pick cap after `sort_ranked_rows` left 544 tests passing, and so did
re-adding the route-local sample filter. The cause was structural —
`rank_repair_candidates`, the function carrying N1 and N2, had **zero direct
tests**, and the N2 pair that existed called `score_candidates_globally` on
hand-built sample lists, where a filter added in the production caller is
invisible.

**Resolution: split eligibility from ranking.** `rank_repair_candidates` now
does only the one thing that needs a `SpecializedContactPlan` — decide which
pool rows are eligible — and its tail expression hands the whole eligible set,
unfiltered and untruncated, to `rank_eligible_candidates`, which is where N1 and
N2 live and which needs no plan. Both rules are now exercised on a fixture with
**two worklist routes and cross-route samples**, exactly what the challenger's
minimum-to-re-review asked for, and N1 is additionally read off the shipped
controller.

Red under sabotage:

| sabotage | red |
| --- | --- |
| `ranked.truncate(route_uncovered_mm2.len())` after the sort | `ranked_rows_are_never_capped_by_the_patch_or_route_count` **and** `the_production_ranking_keeps_every_eligible_row_and_not_one_per_patch` (measured `remaining=1011` clean, `remaining=1` sabotaged) |
| per-candidate route-local sample filter | `a_dust_routes_row_outranks_the_hole_owners_row_in_the_production_ranking` |

The N2 fixture also asserts the row's **own** route share, so a filter that
keeps one route's samples and drops another's is red whichever route it keeps.

### N.3 Finding 4 — the serve-set tie-break was dead code

`rank_key` carried `order` and `key`, unique per row, so §A.4 N3's "ties break on
larger route uncovered area" could never be consulted and the **smaller** route
won on pool order (challenger's probe: 13.041 mm² against 0.21 mm²). Fixed by
comparing serve-set routes on a key that deliberately omits the unique fields.
Red under sabotage: restoring `order` and `key` to that key fails
`the_serve_set_breaks_a_score_tie_on_the_larger_route_not_on_pool_order`.

### N.4 Finding 5 — N4's bookkeeping was O(rows × rejections)

§A.4 ordered "compute it incrementally: one new disc per rejection". The landed
code recomputed `discs_containing` **inside a `min_by` comparator** and then
swept the pool a second time per rejection; the challenger measured **1.10 s of
pure ordering** to drain a 2,000-row pool.

The count now lives on the row (`RankedRepairRow::diversity_discs`).
`DiversityDiscs::reject` charges exactly one new disc in one pass; the
comparator reads a `usize`. The only rows × discs sweep left is
`DiversityDiscs::recount`, which runs when a **commit** rebuilds the ranking and
is therefore bounded by the registered commit bound. Red under sabotage:
reporting a demotion per event rather than per candidate fails
`charging_one_disc_per_rejection_agrees_with_recomputing_every_disc`, which also
pins that the cheap incremental path and the full recount cannot drift.

### N.5 Findings 6 and 7 — the record

**Finding 6, §D.1's `RouteDelta`.** Two field pairs had been dropped and one
renamed, so §D.2's promised line — "closed island 5's 0.36 mm² patch" — could
not be produced from what was recorded. `failing_area_before/after_mm2`,
`failing_patches_before/after` and `worst_gap_before/after` are restored, folded
from `MeasuredState::blocking` (the CENSORED verdict set) beside the existing
uncensored pair. Red under sabotage: folding the failing area from the
uncensored side fails
`a_route_delta_states_the_censored_and_the_uncensored_side_separately`.
`worst_gap_*` is `Option`, deviating from §D.1's non-optional field, because
"this route carried no patch on that side" is a different statement from "the
gap was zero" and the design offers no value for the former.

**Finding 7, §A.8's "every path".** The attempt-0 `MeasurementRefused` return
and `rank_or_refuse`'s error return never printed the report, and the latter
discarded it entirely (`repair: None`) after trials may already have run. Both
now finish and print, and `rank_or_refuse` carries the report. On that path
`candidates_remaining` stays 0 and that is honest rather than convenient: the
ranking itself refused, so there is no ranking to count rows on.
**UNPROVEN — no fixture reaches either path**, in the same sense §M.3 records
for §A.5's digest restore. Both are exercised only by a plan whose repair pool
refuses, which no procedural fixture produces.

### N.6 Corrections to §M

1. **§M.2's `44 of 44` is not the completeness proof it was presented as.** The
   raking blade's pool has **0 zero-score rows out of 45**, which is why the
   unsafe rank-key reading did no harm there; on the overhang table, where 905
   of 1,011 rows score zero, the challenger's forced-rejection probe read
   `diversity_demotions=181 of_which_tried=0` from the same instrument. The
   completeness promise is pinned by
   `every_deprioritized_candidate_is_eventually_tried`, not by the blade's
   trial count.
2. **§M.4's "two loop guards are unexercised" is now one and a half.** §B.3's
   guard has unit coverage of its own quantity
   (`the_best_live_score_ignores_ledgered_rows_and_reaches_zero_only_when_nothing_covers`)
   and of the property that makes it agree with the search order
   (`while_any_live_row_covers_something_the_winner_covers_something`), but no
   procedural fixture reaches the stop itself. `CommitBudgetExhausted` remains
   entirely unexercised. Both stay Prove obligations.
3. **No procedural fixture separates the censored and uncensored sides of a
   `RouteDelta` end to end.** `make_overhang_table`'s route carries no
   sub-censor dust, so the integration assertion holds under a sabotage that
   folds the failing area from the uncensored side; the unit fixture in
   `runner.rs` is what pins the split. A model with dust beside a closing route
   is a Prove-stage read.

### N.7 What did NOT move

No constant was added, raised, relaxed or registered. No cap, gate, threshold or
tolerance moved. No counter decides an outcome: `committed >= commit_budget`
(R6) is still the only counter in a branch, and `best_live_mm2` is a measured
area, not a count. The rollback, clause 3, the strictness pin and §C.3's drift
test are untouched — the challenger verified all four and they are carried
forward unchanged.

**And §M.1's measurements are unmoved**, re-read after every fix on the same
gate profile:

```
make_overhang_table  coverage-repair: trials=1 commits=1 remaining=1011 wall_total=1199ms
                       diversity_demotions=0 of_which_tried=0 []
make_raking_blade    coverage-repair: trials=45 commits=0 remaining=0 wall_total=26117ms
                       wall/trial min=571ms med=581ms max=586ms
                       diversity_demotions=44 of_which_tried=44 [no_improvement=45]
```

`make_toed_slab` still seals at attempt 0 with the frozen digest
`4_190_462_360_230_124_963` unmoved. Every number matches §M.1 and §M.2, so the
seven fixes changed which searches are *possible*, not which results the
existing fixtures produce.
