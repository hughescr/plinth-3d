# Why the coverage-repair search wanders — the regression class, read

**Scope discipline honoured.** Report only. No file under `crates/`, `rust/`, or any
production path was modified; `git status` carried nothing but this file. No treant run was
started. Every fixture run below was started in the foreground, bounded with `gtimeout`,
and observed to completion or to its own timeout.

**Answer in one line:** the regression rate is **not** the expected cost of a search doing
its job, and it is **not** a wrong-objective bug either. `regression` does not mean what the
task brief (and the progress line) reads as; it means the trial **improved coverage** and
was vetoed by a second clause that the search does not model, cannot predict, and learns
nothing from. 119 of 224 treant trials found a strict lexicographic coverage improvement
and threw it away.

---

## 0. The headline correction

The brief's reading — "119 trials … were classified REGRESSION, meaning they made the
objective worse" — is not what the code does.
`crates/plinth-support/src/coverage_repair.rs:276` evaluates the three accept clauses **in
order and returns the FIRST failure**:

```rust
if after_key >= before_key { return NoImprovement { .. } }          // clause 1
if let Some(component) = after.noncoverage.first_regression(..) {   // clause 2
    return Regression { component, before, after }
}
if after.uncensored_uncovered_mm2 >= before.uncensored_uncovered_mm2 {
    return NoAreaCovered { .. }                                     // clause 3
}
Committed
```

`Regression` is **unreachable unless clause 1 strictly improved**. The censored
lexicographic objective — total failing area, then max failing-patch area, then failing-patch
count, then worst gap — got strictly better on every one of those 119 trials. What rejected
them is SPEC §6 clause 2: one of five noncoverage hard-gate counts went up
(`unrouted_no_winner`, `quarantined`, `unrouted_specialized`, `load_violations`,
`unsupported_specialized`).

So the treant run was not wandering in the sense of "half its moves were bad". It was
**finding improving moves at better than half the trials and being unable to keep any of
them.** That is a materially different failure, and it changes what a cap would hide.

The class string `"regression"` (`coverage_repair.rs:249`) is what caused the misreading.
It is the one word on the operator's only live instrument, and it reads as "made it worse"
when it means "improved coverage, blocked on a hard gate".

---

## 1. What the controller does with a regressing trial — code, `runner.rs:2494-2500`

A regressing trial is **rejected and rolled back**; it can never be committed. The plan is a
persistent value, the child is dropped, the context's prepared digests are restored
(`restore_prepared_digests`, guarding finding S4), and the loop continues. That half is
correct and there is no bug in it.

What the search **learns** from the rejection is `RepairLedger::reject`
(`crates/plinth-support/src/runner.rs:2602-2628`), and it is the same two things for every
rejection class — a clause-1 no-improvement, a clause-2 veto, a stage refusal, all
identical:

1. the row id goes into `ledgered`, so that one row is never tried again; and
2. a rule-N4 "diversity disc" of the registered coverage radius is placed at the rejected
   seat, incrementing a `diversity_discs` counter on every candidate inside it.

Nothing else. Not the outcome class, not the component that regressed, not the measured
before/after. The whole record is written to `steps` for the sidecar and consulted by no
branch.

**And the disc is inert.** `choose_winner`'s ordering key
(`crates/plinth-support/src/coverage_closure.rs:1136-1143`) is

```rust
(OrderedScore(row.marginal_area_mm2), row.diversity_discs, row.order, row.key)
```

— score **first**, disc count only as a tie-break. `marginal_area_mm2` is an `f64` sum of
sample areas (`score_candidates_globally`, `coverage_closure.rs:665`), so two candidates
generically have distinct scores and the disc count never gets a vote. The ordering
comment says so plainly, and it was chosen deliberately: disc-first was measured harmful on
`make_overhang_table`.

**Measured, today:** `the_raking_blade_refuses_with_a_measured_class_per_route`, 45 trials,
final summary line —

```
coverage-repair: trials=45 commits=0 remaining=0 wall_total=20432ms
  wall/trial min=448ms med=454ms max=457ms
  diversity_demotions=44 of_which_tried=44 [no_improvement=45]
```

**44 candidates deprioritized, 44 of them tried anyway.** The instrument §A.8 built to say
whether rule N4 does any work says it does none. A rejection therefore removes exactly one
row from a pool of ten thousand and changes nothing else about what gets tried next.

---

## 2. Is "regression" measured against the right thing?

**The accept test is honest: one function, one instant, one lowered run.** `before` is the
accepted state's own vector (`accepted_objective` / `accepted_gates` /
`accepted_uncensored_mm2`, rebound only on a commit — `runner.rs:2524-2528`), `after` is the
trial's, both read off `SupportRun` by the same methods. There is no proxy at the accept
site and no stale-baseline bug. Clause 3's strict `>=` on an `f64` is argued from a fixed
multiset of area weights and is sound.

**The search is judged on one number and steered by another, and that is the defect.** The
only quantity the ranking optimizes is `marginal_area_mm2`: the sum of *uncovered sample
areas within the coverage radius of a seat*. Pure geometric proximity. It contains no model
of routing feasibility, no clearance, no load partition, and — the point here — **no term
for any of clause 2's five counts.** The ranking is a predictor of clause 1 alone, and 119
times it predicted correctly. Clause 2 is a veto the search cannot see coming and cannot
remember having seen.

**One structural asymmetry inside clause 2, which may or may not be firing here.**
`SupportRun::noncoverage_vector` (`runner.rs:6149`) counts transcript rows over the **whole
plan**, one pass over every finalized disposition. A trial's plan has **one more contact**
than the accepted plan it is compared against. So a candidate whose own appended contact is
itself `SkippedQuarantined` or `UnroutedNoWinner` adds **+1 to the vector by its own
existence**, with nothing pre-existing having got worse. The controller measures exactly
that fact on every trial — `own_contact_quarantined`, `own_contact_condemned`,
`own_contact_no_winner` (`runner.rs:2444-2465`) — labels the fields EVIDENCE ONLY, and the
accept clause does not consult them. The module's own doc records that on `zombie-3-a`
**89 of 89** rejected trials had a condemned own contact.

Whether that asymmetry accounts for any of the treant's 119 is the one question this
investigation cannot close; see §6.

---

## 3. Is the pool re-ranked, or ranked once and drained?

**Ranked once, drained one row per trial. Re-ranked only on a commit** — `rank_or_refuse` is
called at entry and inside the commit arm only (`runner.rs:2202`, `runner.rs:2533`), with
the file's own justification: "A COMMIT is the only thing that moves the uncovered sample
set … A rejection re-sorts instead (rule N4)" — and §1 shows that re-sort changes nothing.

Measured, straight off the log, one progress line per trial (the pacer emits every 30 trials
**or** every 60 s, and every treant trial exceeded 60 s, so the classification below is
exact and complete, not sampled):

| trial | remaining | move |
|---:|---:|---|
| 0 (entry) | 10,222 | — |
| 1 | 10,221 | −1, no_improvement |
| 2 | **9,596** | **commit** → full rescore, −625 |
| 3 … 224 | 9,595 … 9,374 | **−1 on every single trial** |

222 consecutive trials against a ranking last rebuilt at trial 2. "9,374 remaining after 224
trials" is therefore not a search narrowing — it is a queue being served.

And `remaining` is not a countdown to anything. On `a_spec6_removal…` (measured today, §5)
the pool **grew** 1,617 → 3,416 across two commits, because each rescore re-admits rows.

---

## 4. Is 53% consistent with an effectively random draw?

**No — it is worse than random. The outcomes are significantly clustered.**

Extracting each trial's class from the per-trial deltas of the two histogram counters over
the full 224-trial log:

- **Wald–Wolfowitz runs test** over the 223 non-commit trials: **94 runs observed, 112.0
  expected under independence, sd 7.42, z = −2.43.** Significantly *fewer* runs than an
  i.i.d. Bernoulli draw would give — positive autocorrelation.
- **Longest streaks: 21 consecutive regressions, 28 consecutive no-improvements.**
- **The rate is not stationary and it does not decay:** trials 1–56 → 26.8 % regression;
  57–112 → 62.5 %; 113–168 → 64.3 %; 169–224 → 58.9 %. The first regression is at trial 30;
  trials 1–29 (after the trial-2 commit) are 29 straight no-improvements.

The 21-run is the signature. `sort_ranked_rows` orders by score descending, and seats inside
one physical cluster sit **0.03–0.46 mm apart** (the module's own measurement) with nearly
identical but distinct scores — so a cluster sorts adjacently and is drained end to end. The
search asked the same physical question of 21 near-identical seats, got the same clause-2
veto 21 times, paid **≈37 minutes** for it, and had no mechanism to stop after the first.

**What differs from the small fixtures: not the pool size — the class itself.** Measured
today, foreground, `gtimeout`-bounded:

| Model | pool | trials seen | commits | no_improvement | **regression** |
|---|---:|---:|---:|---:|---:|
| `make_raking_blade` | 45 | 45 (natural stop) | 0 | 45 | **0** |
| `make_uncovered_rim_table` (cap evidence) | 540 | 2 (sealed) | 1 | 1 | **0** |
| `shelf_with_dust…` | 1,034 | 110 (timeout) | 3 | 107 | **0** |
| `half_buried_table` | 1,617→3,416 | 81 (timeout) | 2 | 79 | **0** |
| **Weeping Treant v1.2** | 10,222 | 224 (killed) | 1 | 104 | **119** |

**238 fixture trials, zero regressions.** The clause that costs the real model 53 % of its
trials — and every improving move it ever found — **has no fixture coverage at all**. It is
not exercised once by the committed corpus. That is why the cap evidence, which was sound
for what it saw, could not see this.

---

## 5. Where the ~105 s per trial goes — REVIEW.md #12 is right about the defect and wrong about the term

Per-trial wall time from consecutive `elapsed=` deltas across all 224 trials:

| min | p25 | median | p75 | max |
|---:|---:|---:|---:|---:|
| 101,693 ms | 105,544 ms | **107,817 ms** | 110,081 ms | 135,705 ms |

Flat: first-20 mean 114,168 ms, last-20 mean 104,608 ms. A regression costs 107,567 ms and a
no-improvement 109,324 ms — **identical**, because every trial pays the whole pipeline
before anything is judged.

The same log times **one full stage chain on this model** during attempt-0 legalization
(pass 6, the pass that lowered — `EVIDENCE-treant-runaway.log:128-138`):

| Preparation | Routing | Rewire | Brace | Structure | Lowering | **total** |
|---:|---:|---:|---:|---:|---:|---:|
| 10,364 ms | 33,927 ms | 17,411 ms | 14,839 ms | 15,246 ms | 14,361 ms | **106,148 ms** |

One chain ≈ 106 s; one trial ≈ 105–108 s. **The trial cost IS the chain rebuild.**
`run_stage_chain` (`runner.rs:1396`) reruns `prepared_run_for_plan` → `route_initial_cco` →
rewire → brace → structure → lower from scratch for a plan that differs by **one row**,
and nothing between trials is reused. The coverage measurement REVIEW #12 names is the
residual, and the residual is small.

**Caveat, stated rather than smoothed over:** a trial's chain runs without legalization
steering, so it may be somewhat cheaper than pass 6's, and the measurement's true share is
bounded by that difference — which this log cannot resolve, because trials pass `None` for
the stage clock and print no stage lines. The claim that survives is the ordering: the
six-stage rebuild dominates, and fixing #12's superlinear island re-validation, while
correct and worth doing, **would not have saved this run.**

Recomputation between trials that could not have changed: the mesh, the topology, the
hollow, the island sidecar, the coverage sample set (frozen at attempt 0 by design), the
candidate seats, and the routing of every contact except the one appended — recomputed 224
times at ~105 s each. That is the physics of the 107-day estimate.

---

## 6. Verdict, and the one thing I cannot tell you

**Defect, not the cost of doing business.** Three findings, in the order they should be
acted on. Only the third is the cap question, and it is the owner's.

### D1 — the search does not model the clause it is being judged on. *The defect.*
**Site:** ranking key `crates/plinth-support/src/coverage_closure.rs:1136-1143`, and
`RepairLedger::reject` `crates/plinth-support/src/runner.rs:2602-2628`.
The ranking is a function of predicted clause-1 area alone; rejection feeds back one spent
row and a disc that is measured inert (44 demoted / 44 tried). So a clause-2 veto is
information the controller pays 105 s to obtain and then discards, and it re-obtains it 21
times in a row from adjacent seats. This is a real, sited, physics-first-legal defect: it is
about **what the search is allowed to remember**, not about inventing a threshold.

### D2 — clause 2 compares a count over N contacts against a count over N+1. *Candidate defect.*
**Site:** `SupportRun::noncoverage_vector` `crates/plinth-support/src/runner.rs:6149`,
consumed by `NoncoverageVector::first_regression`
`crates/plinth-support/src/coverage_repair.rs:147`.
A candidate whose own contact is condemned increments the vector by its own existence. The
run already measures the discriminator (`own_contact_condemned`, `runner.rs:2451`) and the
accept clause ignores it. **Whether this is what rejected the treant's 119, I cannot say**,
and I will not guess: it requires clause 1 to have improved *while* the trial's own contact
was condemned, which is possible through the load repartition but not demonstrated.

### D3 — the cap. Deliberately last.
A "50 trials since last commit" bound would have stopped this run at trial 52, about 52
minutes into the loop, and saved ≈5.0 hours. Nothing observable would have been lost — no
commit ever came. It would also have hidden D1 and D2 completely, exactly as the brief
anticipated. The cap evidence's refusal to fit a number still stands; nothing here changes
it.

### What settles D2 — and why no run you told me not to make would settle it either

**There is no run available today that settles D2, and the reason is itself the first thing
to fix.** The per-trial `outcome` (with its named component) and the `own_contact_*` flags
exist only in `RepairLedger::steps`, and `steps` reaches disk only through
`persist_sealed_repair_ledger` at a seal or `CoverageClosureRefusal::repair` at a refusal.
**Both are terminal.** The treant loop does not terminate, so the 6.7-hour run wrote nothing
— and a `gtimeout`-bounded treant run would write nothing either, no matter how long I let
it go. The committed fixtures cannot substitute: §4 shows they never produce the class.

So the recommendation on **what to fix first** is not D1 and not the cap:

> **Make the per-trial record survive a killed run.** Put the rejecting clause-2 component
> and `own_contact_condemned` on the progress line the loop already prints unconditionally
> — or flush the step to an incremental sidecar. It is disclosure only: no accept clause
> changes, no constant is introduced, nothing decides an outcome. It is precisely the trade
> ruling R10 already made ("removes the cap and replaces it with visibility, and that trade
> only holds if the visibility is real"), and the treant run is the proof that the
> visibility is not yet real — 224 lines of stderr that cannot say *why* a single trial was
> rejected.

**Then the run that settles it:** `plinth prepare` on Weeping Treant v1.2 at the pinned
baseline orientation, supervised, `gtimeout`-bounded to **≈45 minutes past the start of the
repair loop** (≈25 trials past trial 30, where the regressions begin). With the component on
the line that is enough to read the answer directly:

- component is `quarantined` or `unrouted_no_winner` **and** the own contact was condemned →
  **D2 is real**, clause 2 is punishing candidates for their own failure to route, and the
  accept clause is the fix site.
- component names a count whose rise is not the trial's own contact → clause 2 is doing its
  job, SPEC §6 is being honoured, and **D1 is the whole story**: the ranking must learn
  which neighbourhoods cost a terminal.

Either way the ruling becomes possible on 45 supervised minutes instead of 107 days.

**One fixture is also missing and should be built regardless:** a model where a repair
contact's arm displaces an already-routed terminal, so that `repair_step_outcome` returns
`Regression` in the committed suite. Today that path — the one that consumed the flagship
model's entire run — is exercised by nothing.

---

## Appendix: exactly what was run

- `gtimeout 300 cargo test -p plinth-support --test a4_transactional_controller
  the_raking_blade_refuses_with_a_measured_class_per_route -- --nocapture --test-threads=1`
  — completed in 20.95 s, natural pool exhaustion, summary line quoted verbatim in §1.
- `gtimeout 290 cargo test -p plinth-support --test support_runner_integration
  a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule --
  --nocapture --test-threads=1` — timed out by design at 81 trials / 2 commits /
  `[no_improvement=79]`.
- `gtimeout 290 cargo test -p plinth-support --test support_runner_integration
  closure_repairs_censored_dust_because_that_repair_closes_a_real_hole -- --nocapture
  --test-threads=1` — timed out by design at 110 trials / 3 commits /
  `[no_improvement=107]`.
- Both timeouts are the harness stopping a run that had not stopped itself; neither is a
  test failure and neither was left running.
- All statistics in §4 and §5 are derived from
  `docs/evidence/remediation-2026-08-26/EVIDENCE-treant-runaway.log` alone (225 progress
  lines, one per trial plus the entry line), by differencing the two histogram counters and
  the `elapsed=` field. No new instrumentation was added to produce them.
- No treant run was started. Debug profile throughout, so fixture per-trial times are not
  comparable to the treant's release-profile numbers; only the **class mix** is compared
  across profiles, and that is a count, not a time.
