# The coverage-repair search: diagnosis

**Status: CAUSE ESTABLISHED.** The exact boundaries of that claim are in
[§10 The verdict](#10-the-verdict).

**Audience.** An experienced engineer who has not seen this codebase and does not work in
Rust. Every term specific to this project is explained where it first appears. **No fix is
proposed and no production behaviour was changed.** The only code added for this report is
print-only instrumentation, inert unless an environment variable turns it on.

**Where the author's own numbers come from.** Worktree
`.claude/worktrees/wf_8376a654-27e-5`, branch `measure/verify-diagnosis`: `develop` at
`2512347` with the three measurement lanes' instrument branches merged in, plus one new
probe described in §4.3. Compiled with the `gate` profile, which is optimised and has
**debug assertions switched off**. The earlier lanes measured on the `test` profile, which
has debug assertions **on**. Where the two agree, they agree across that difference.

**Measurement conditions.** A 16-core Apple Silicon box shared with other work. Load average
during my window ranged from 7 to 65. Absolute wall times below are therefore inflated by an
unknown factor; **shares, counts, and the coverage numbers are not affected** — the coverage
numbers are deterministic and reproduced to the last decimal digit across machines, profiles
and runs.

---

## 1. Lead: is this slow, or is it broken?

**It is broken: the number used to choose a candidate is not the number used to judge it,
and measurement says the two do not agree even in sign.** The slowness follows from that —
because almost nothing can pass the acceptance test, the loop does not converge; it
enumerates the whole candidate pool, one full six-stage re-plan per candidate.

**The disagreement has two different causes on two different models, and it matters that they
are different.**

- On the **dust fixture** — the plan behind the gate's one accepted failing test — the
  planner is **not idempotent**. Re-running it on a plan with **nothing changed** sheds 2
  contacts and loses coverage, and it keeps doing so: 110 → 108 → 106 → 104 → 102 contacts
  and 5.199 → 5.899 → 6.532 → 7.332 → 13.997 mm² of uncovered area over four successive
  passes. A trial is one such pass plus one added contact, so every candidate is charged a
  loss it did not cause and the score cannot see. 271 of my 274 recorded trials made the
  objective worse; in 46 of them the loss was the *entire* recorded difference and the
  appended contact changed nothing measurable.
- On **`make_raking_blade`** and on **`Weeping Treant v1.2`** there is **no such
  degradation**. I ran the same experiment on the treant and got a perfect fixed point: four
  re-plans, all returning 143.28101209495736 mm² and 482 contacts, unchanged to the last
  digit. There the disagreement has the opposite cause: the candidates the score ranks
  highest **cannot be built at all**. All 45 of the blade's candidates were appended and then
  failed to get an arm; the treant's first trial scored 100.956 mm² and moved the measured
  objective by exactly zero.

So the common defect is the ranking, and the two models fail it from opposite directions —
one because the objective moves for reasons unrelated to the candidate, the other because the
candidate never physically exists. **Do not carry a conclusion from one of these models to the
other.**

A **third, independent defect** costs time only on large meshes: every trial recomputes a
whole-mesh face classification seven times, purely to compare a hash in a staleness check. On
the small fixtures that costs about 1 ms per trial. On `Weeping Treant v1.2` (429,120 faces)
the same seven recomputations took 97.9 s and 110.1 s on the two trials I measured — 67.5 %
and 70.3 % of the whole trial. It multiplies the cost of everything above; it causes none of
it.

---

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **Contact** | One support attachment point on the model surface. A *plan* is a set of contacts. |
| **Arm / route** | The physical strut that carries a contact's load down to the build plate or to another support. **Routing** is the stage that tries to build them. |
| **Condemned contact** | A contact that is in the plan but that the planner could not build an arm for — *quarantined*, or *unrouted (no winner)*. It occupies the plan and carries nothing. |
| **Island / patch** | A connected piece of model surface the planner has decided needs support. |
| **Coverage sample** | A point on the surface carrying an area weight. It is *covered* if it lies within a fixed radius of a contact. |
| **Objective** | The number the search tries to reduce: total uncovered sample area on failing islands, tie-broken by patch count and worst gap. Lower is better. |
| **Repair pool / repair row** | The precomputed set of extra places a contact *could* be added. One row is one candidate. |
| **Trial** | Evaluating one candidate: copy the plan, append that one row, **re-run the whole six-stage planner on the copy**, measure the objective, compare against the stored one. |
| **Commit** | A trial whose result was accepted and became the new plan. |
| **Score / marginal area** | The number candidates are ranked by. §4.1 shows what it actually computes. |
| **Stage chain** | The six planner stages a trial re-runs: Preparation, Routing, Rewire, Brace, Structure, Lowering. |
| **The gate** | `cargo test --workspace --offline`, the mandated full test run. |

Two named test fixtures recur:

- **the dust fixture** — `shelf_with_dust_beside_a_row_less_hole`, exercised by
  `closure_repairs_censored_dust_because_that_repair_closes_a_real_hole`
  (`crates/plinth-support/tests/support_runner_integration.rs:1366`). This is the gate's one
  accepted failing test.
- **the removal fixture** —
  `a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule`, in the
  same file.

---

## 3. Which models this covers, by name

The corpus has eight models. **Six never run a single repair trial.** I measured this
myself, one process per model, reading the planner's own end-of-run summary:

| Model | Repair summary line | Wall |
|---|---|---|
| `Sphere` | `trials=0 commits=0 remaining=0` | 2,957 ms |
| `Cylinder` | `trials=0 commits=0 remaining=0` | 3,111 ms |
| `make_box` | `trials=0 commits=0 remaining=0` | 62 ms |
| `make_overhang_table` | `trials=0 commits=0 remaining=0` | 16,562 ms |
| `make_toed_slab` | `trials=0 commits=0 remaining=0` | 492 ms |
| `make_thin_fin` | `trials=0 commits=0 remaining=0` | 186 ms |
| `make_raking_blade` | `trials=45 commits=0 remaining=0` | 31,707 ms |
| `Weeping Treant v1.2` | entered the loop with `remaining=8906`; never observed finishing | — |

Every statement in this report about the search's *behaviour* therefore rests on:

- **`make_raking_blade`** — measured to completion by me.
- **the dust fixture** — a synthetic plan built inside the test, not a corpus model. Measured
  to completion by me (274 trials), reproducing the accepted red's refusal exactly.
- **the removal fixture** — likewise synthetic. Measured only by an earlier lane, where it
  did not terminate.
- **`Weeping Treant v1.2`** — the corpus's only real miniature and only large mesh. It has
  never been observed reaching the end of its repair search, by any lane including this one.

Nothing here has been measured on `Sphere`, `Cylinder`, `make_box`, `make_overhang_table`,
`make_toed_slab` or `make_thin_fin` beyond the fact that they never enter the loop.

---

## 4. The measured breakdown

### 4.1 The score is a distance test; the objective is a re-plan

*Method: reading the implementation.* `score_candidates_globally`
(`crates/plinth-support/src/coverage_closure.rs:684`) computes a candidate's score as:

> for every uncovered sample, if the straight-line distance from the candidate's proposed
> attachment point to the sample point is within the coverage radius, add that sample's area.

That is the entire computation. It does not consult routing, clearance, load, or whether an
arm can physically be built at that point, and it does not model the fact that adding a
contact makes the planner re-plan every *other* contact. The function's own doc comment
claims the opposite — "the physical quantity SPEC §6 names is the area the contact actually
covers" — and the arithmetic beneath it does not deliver that quantity.

The objective the trial is then judged on is measured after `run_stage_chain` re-runs all
six stages on the modified plan (`crates/plinth-support/src/runner.rs:2614` onward).

### 4.2 Claimed and delivered do not agree, even in sign

*Method: analysis of the production evidence stream* (`repair-trial-trail.v1` JSONL — a
stream the planner already writes when asked, not something added for this report). I ran the
dust fixture to completion myself — it reproduced the accepted red exactly, ending with

```
coverage-repair: trials=274 commits=3 remaining=766 wall_total=1274411ms
                 wall/trial min=1766ms med=2195ms max=20560ms
                 [no_improvement=271] own_contact_condemned=176
CoverageClosureRefusal { attempts: 3,
  reason: NoImprovingRepair { candidates_tried: 274, best_remaining_score_mm2: 0.0 }, … }
```

— the same 274, the same 0.0, the same island. The behaviour lane archived a trail from the
same fixture on the other build profile, and **the two files are byte-identical over all 169
overlapping records** (`cmp` exit 0).

Grouped by (score claimed, objective reduction delivered), all 274 trials — the eight largest
groups:

| Score claimed (mm²) | Objective reduction delivered (mm²) | Trials |
|---|---|---|
| 0.6998291015625 | **−1.6995849609375** (worse) | 62 |
| 0.199951171875 | **−1.6995849609375** (worse) | 47 |
| 2.599365234375 | **−0.6998291015625** (worse) | 46 |
| 0.199951171875 | −1.4996337890625 (worse) | 27 |
| 0.6998291015625 | −0.999755859375 (worse) | 7 |
| 0.6998291015625 | −0.733154296875 (worse) | 7 |
| 0.066650390625 | −1.6329345703125 (worse) | 5 |
| 0.2332763671875 | −1.46630859375 (worse) | 5 |

Totals over all 274 trials: **3 better, 271 worse, 0 unchanged; 3 commits.** The delivered
result does not rise with the score. The *highest*-scoring group, 2.599 mm², delivers
−0.6998 mm² 46 times; the 0.200 mm² group — thirteen times smaller a claim — delivers
−1.700 mm², a worse result, 47 times. The score does not order the outcomes even weakly.

### 4.3 Failure mode 1 (the dust fixture): re-planning an unchanged plan destroys support

This is the measurement the diagnosis of the **accepted red** turns on, and the part I
established myself. **It is specific to the dust fixture** — §4.4 shows the two corpus models
that reach the loop do *not* behave this way.

**Step 1 — the null trial** (an earlier lane's instrument, re-run by me). Before the first
real trial, take the plan the search is about to work from, **append nothing**, run the
identical six-stage chain and the identical measurement, and print the result beside what
the stored state claims. Twice: once with the legalisation steering the accepted run used,
once without (a real trial runs without).

My run, dust fixture, gate profile, fresh build — identical in both passes:

```
accepted    failing = 5.19873046875 mm²   contacts = 110
                      condemned = 22 (unrouted_no_winner 10, quarantined 12)
re-measured failing = 5.8985595703125 mm² contacts after = 108
                      condemned = 2  (unrouted_no_winner 2, quarantined 0)
```

This reproduces the earlier lane's reading to the last digit, across the debug-assertion
difference between profiles.

**Step 2 — the null iterate** (new; my own instrument, `PLINTH_REPAIR_NULL_ITERATE`). The
null trial shows *that* a re-plan loses coverage. It cannot say *why*, because two
explanations produce the same number: either the stored objective was already stale, or the
chain is not idempotent and each re-plan drifts again. So: run the chain on the accepted
plan, feed its **output** plan straight back in, and repeat. Unsteered on every pass, which
is how a trial routes.

| Pass | Contacts in | Uncovered area measured (mm²) | Contacts out |
|---|---|---|---|
| *(stored accepted state)* | — | 5.19873046875 | 110 |
| 1 | 110 | 5.8985595703125 | 108 |
| 2 | 108 | 6.53173828125 | 106 |
| 3 | 106 | 7.33154296875 | 104 |
| 4 | 104 | 13.99658203125 | 102 |

**The chain is not idempotent, and it had not settled after four passes.** Each pass sheds
exactly 2 contacts and loses further coverage — 0.700, 0.633, 0.800 and 6.665 mm² on passes
1 to 4. Over four passes on a plan nobody modified, uncovered area rises from 5.199 mm² to
13.997 mm² — a factor of 2.7 — and the plan loses 8 of its 110 contacts. The loss is
monotone but not smooth: the fourth pass costs more than the first three together, which is
why "it converges to something small" cannot be assumed from three passes. Every pass reports exactly `unrouted_no_winner: 2`, and exactly 2 contacts
disappear: the correlation is exact over all four passes, though the mechanism inside
Routing that produces the two fresh failures is not established here (§8.1).

**Why this is the cause.** A repair trial is precisely one of these passes with one contact
appended. Acceptance requires the objective to fall **strictly** (`repair_step_outcome`,
`crates/plinth-support/src/coverage_repair.rs:389`, read first-hand), and it compares the
trial's fresh measurement against the **stored** number from the previous state. So every
candidate must first pay for a degradation step it did not cause and cannot see — 0.6998 mm²
at the first state, larger later — and only then does its own contribution count. The
candidate ranking (§4.1) contains no term for any of this.

The confirmation is in the trial data: **46 trials measured exactly the null-trial number.**
Their appended contact changed the measured coverage by nothing at all; the entire recorded
difference was the degradation. Note also what the re-plan *improves*: condemned contacts
fall from 22 to 2. The stored "accepted" state is one the planner will not reproduce.

### 4.4 Failure mode 2 (the corpus models): the baseline is stable and the candidates cannot be built

**The degradation of §4.3 does not happen on either corpus model that reaches the loop.** I
ran the same null trial on `Weeping Treant v1.2`:

```
accepted    failing = 143.28101209495736 mm²  uncensored = 170.51240128699848  contacts = 482
                      condemned = 194 (unrouted_no_winner 40, quarantined 86,
                                       unrouted_specialized 68)
re-measured failing = 143.28101209495736 mm²  uncensored = 170.51240128699848
                      contacts after = 482
                      condemned = 136 (unrouted_no_winner 21, quarantined 47,
                                       unrouted_specialized 68)
```

Identical to the last digit, and not one contact lost — in **both** the steered and the
unsteered pass. I then ran the same four-pass null iterate on it:

| Pass | Contacts in | Uncovered area measured (mm²) | Contacts out |
|---|---|---|---|
| *(stored accepted state)* | — | 143.28101209495736 | 482 |
| 1 | 482 | 143.28101209495736 | 482 |
| 2 | 482 | 143.28101209495736 | 482 |
| 3 | 482 | 143.28101209495736 | 482 |
| 4 | 482 | 143.28101209495736 | 482 |

**A perfect fixed point.** Four re-plans, not one contact lost, not one digit changed. The
behaviour lane measured the same stability on `make_raking_blade`: its null trial reproduces
the accepted objective exactly, both ways.

(Note what still moves: condemned contacts fall from 194 to 136 on the treant, as they fall
from 22 to 2 on the dust fixture. So a re-plan does re-route on both. On the treant it
re-routes without losing anything the coverage measurement counts; on the dust fixture it does
not.)

So on the corpus models the trial comparison is sound and the failure is elsewhere: **the
candidates the score ranks highest cannot be built.**

- `make_raking_blade`: I ran it to completion and its own summary reports all 45 candidates
  condemned — 8 quarantined, 37 refused at routing. Recomputing from the behaviour lane's
  archived trail for that model, all 45 trials measured the objective **unchanged, bit for
  bit**, over 21 distinct physical seat points (several repair rows share a point, so 24 of
  the 45 trials re-tested a point an earlier trial had already condemned).
- `Weeping Treant v1.2`, trial 1: score 100.956 mm², contact quarantined and condemned,
  `objective_after` bit-identical to `objective_before`. **This one figure is the treant
  lane's, not mine** — I ran the treant without the trial trail enabled, so I did not
  reproduce the score itself, only the trial's timing and counters.

Both are the same defect as §4.3 seen from the other side. The score (§4.1) is a distance
test on a *proposed point*; nothing in it asks whether a strut can reach that point. On the
blade, where every candidate's disc covers every uncovered sample, the score is additionally
constant across all 45 rows — it cannot even order them.

**Two failure modes, one ranking.** Whether the objective moves for reasons unrelated to the
candidate (§4.3) or the candidate never physically exists (§4.4), the search is choosing by a
quantity that does not predict what it will measure. Nothing in this report licenses carrying
the dust fixture's degradation onto the corpus models, or the corpus models' stability onto
the dust fixture.

### 4.5 Where a trial's time goes — small fixtures

*Method: the planner's own stage lines*, which print a running elapsed value on entry to
each stage. I differenced consecutive entries across 30 dust trials in my own run:

| Stage | Total over 30 trials | Share of chain |
|---|---|---|
| **Rewire** | 82,017 ms | **68.75 %** |
| **Routing** | 34,134 ms | **28.61 %** |
| Structure | 1,235 ms | 1.04 % |
| Brace | 947 ms | 0.79 % |
| Preparation | 876 ms | 0.73 % |
| Lowering | 625 ms | 0.52 % |
| chain total | 119,304 ms | |

The attribution lane measured the same split with explicit timers on the other profile and
got Rewire 69.50 % / Routing 26.33 %; its sampling profiler put the same two call sites at
68.5 % / 28.4 %. Three instruments, two profiles, one answer.

The chain is essentially the whole trial. A typical dust trial line from my run:

```
begin_edit=5ms append=6ms chain=1930ms measure=33ms total=1974ms
```

The plan edit is 0.6 % of a trial and the coverage measurement is 1.7 %.

### 4.6 Where a trial's time goes — large meshes

*Method: reading the implementation, plus counters on the two operations concerned.*

`GenericContactPlan::validate_binding` (`crates/plinth-support/src/perception.rs:990`) is a
staleness check — it exists to answer "is this plan still bound to the same geometry and
configuration?". To answer it, it **builds a complete new surface object over the whole
mesh** and recomputes the full authority fingerprint, which folds in
`NativeSurface::exterior_face_mask()` (`rust/geom_kernel/src/queries.rs:3448`). That mask
performs, for **every selected face**, three barycentric sample points × two epsilon offsets
= six point-in-mesh containment tests. It is memoised — but the memo
(`exterior_mask: OnceLock<Vec<bool>>`, declared at `queries.rs:1024`, created empty at
`1131` and `1176`) lives **on the surface instance**, and the instance is brand new on every
call. The memo is therefore always cold.

Measured call counts, my own runs:

| Model | Faces | Surface builds / trial | Mask computations / trial | Mask time / trial |
|---|---|---|---|---|
| dust fixture | small synthetic | 7 | 7 | 1 ms |
| `make_raking_blade` | 24 | 7 | 7 | 1 ms |
| `Weeping Treant v1.2` | 429,120 | 7 | 7 | 97,895 ms and 110,050 ms (trials 2 and 3) |

**Seven per trial is a structural constant of a trial, not something specific to the
treant** — I measured 7 on two unrelated models. What changes is the price: 24 faces × 6
tests = 144 containment tests per computation; 429,120 faces × 6 tests = 2,574,720, seven
times per trial.

Trial 1's counters must never be read as a per-trial count: the counter is drained per trial
and trial 1 also drains attempt 0's work. I measured trial 1 at 29 builds / 30 masks on the
dust fixture, 15/16 on `make_raking_blade`, and 15 builds / 16 masks / 229,020 ms of mask
time on the treant — the last of which reproduces the treant lane's 15/16/228,878 ms to
within 0.06 %.

### 4.7 Why the search runs so many trials

*Method: ranking snapshots from the pool probe, taken in my own run, plus the code.*

On the dust fixture the initial ranking holds 1,034 rows with 54 distinct scores, of which
737 (71 %) score exactly 0; the top score, 2.599365234375 mm², is shared by 48 rows. After
the third commit: `trials: 55`, 985 rows, 19 distinct scores, 766 zeros — **219 non-zero
rows left**, with 76 of them tied at 0.6998291015625 mm². 55 + 219 = **274**, which is
exactly the `candidates_tried: 274` the refusal reports. The search tries every non-zero row
and then hits the guard that stops when the best remaining score is 0.00 mm².

(That same snapshot records `nominated_rows: 0, eligible_rows: 985` for the one route — the
two numbers the refusal prints, and the subject of §6.)

That is an enumeration, not a search. Nothing learns from 200 consecutive failures: the
ranking is recomputed only on a commit, and the quantity it recomputes has already been
shown not to predict the outcome.

`make_raking_blade` is the degenerate case. All 45 rows carry the **same** score
(5.194999999999992 mm² — the entire uncovered area of both routes, because every candidate
point's disc contains every uncovered sample), so the ranking carries no information
whatever. I measured its run to completion: `trials=45 commits=0 remaining=0`,
`[no_improvement=45]`, `gate_rises[quarantined=8 unrouted_no_winner=37]`,
`own_contact_condemned=45` — all 45 candidates were appended and then failed to get an arm
built.

---

## 5. What we ruled out

Three earlier theories about this area were each demolished by measurement. The table is the
accumulated record, including those three. **Everything here is excluded. A fourth theory
should not re-propose any of it.**

| # | Mechanism proposed | Verdict | Evidence |
|---|---|---|---|
| 1 | "Many trials improved coverage and were vetoed by a secondary rule" | **Ruled out** | That veto class fires zero times in 1,062 measured trials. In my own complete dust run all 271 rejections are `no_improvement` (clause 1) and all 45 of `make_raking_blade`'s are too; the noncoverage and anti-gaming clauses fire zero times on either. |
| 2 | "Scores are generically unique, so the tie-break never runs" | **Ruled out** | All 45 `make_raking_blade` rows carry one identical score; the tie-break voted 699 times. |
| 3 | "Each trial rebuilds a nearest-neighbour search over every indexed point" | **Ruled out** | Geometry queries are 4.7 %/7.9 % of trial time; `validate_integrity` / `measure_coverage` are 0.37 %/0.16 %. |
| 4 | "The coverage measurement is expensive" | **Ruled out** | My own per-trial lines: `measure=33ms` against `total=1974ms` — 1.7 % on the dust fixture. On the treant, 16.8 s of 145.1 s and 22.2 s of 156.5 s — 11.6 %–14.2 %, never dominant. |
| 5 | "Rebuilding the ranking is the cost" | **Ruled out** | The ranking is rebuilt only on a commit. My own treant rankings took 937, 890 and 825 ms against trials of 145,077–156,536 ms — 0.53 %–0.65 %; the treant lane measured 673–782 ms. On the dust fixture my initial ranking took 1 ms, and the attribution lane's rank timer totals 0.00 s over 274 trials. |
| 6 | "Work per trial grows as the search proceeds" | **Ruled out** | The rewire optimiser's own counters are flat: 4 sweeps on 274/274 dust trials; move evaluations 464–537; candidate batches 384–435. On the removal fixture, 379 of the first 385 trials record the bit-identical tuple 2/322/266/71. On the treant, my own trials 2 and 3 cost 145,077 ms and 156,536 ms while the ranked pool fell 8,906 → 8,696 → 8,068; the treant lane saw the same over 6 trials. My own dust trials 1–16 span 1,914–2,371 ms with no trend. |
| 7 | "Cost per trial is driven by the size of the candidate pool" | **Ruled out** | As #6, plus: the only pool-sized operation with a timer (ranking) is 0.4–0.5 % of a treant trial. This specifically refutes `docs/evidence/repair-cap-evidence.md` §4's "~linear in candidate-pool size" — in that data pool and mesh grew together, and the mesh is the term that matters. |
| 8 | "The plan edit — copying the plan, appending the row — is the cost" | **Ruled out** | 0.86 s of a ~150 s treant trial (0.6 %); 11 ms of a 1,974 ms dust trial (0.6 %). |
| 9 | "Loop overhead outside the trials" | **Ruled out** | 7.6 s of the dust test's 1,058 s serial run; unaccounted time inside a trial is 0.34 %. |
| 10 | "Trials that make things worse are being committed" | **Ruled out — structurally impossible** | `repair_step_outcome` returns `Committed` only after the coverage key strictly falls (`coverage_repair.rs:389`, read first-hand). The step quoted in the original problem statement carries `commit: None` — it is a *rejection*. Its being worse is the symptom, not a commit bug. |
| 11 | "The contacts are being condemned, and that explains the worse objective" | **Ruled out as a complete explanation** | In my own complete dust run, 98 of 274 trials had a contact that was **not** condemned, and 95 of those 98 still made the objective worse. Condemnation dominates on `make_raking_blade` (45 of 45) but does not explain the dust fixture. |
| 12 | "The search does not terminate" | **Ruled out** | Rejected rows are recorded and permanently excluded from later rankings; committed rows become occupied; commits are bounded at 8 (`coverage_repair_max_attempts`, `crates/plinth-core/src/constants/placeholders.rs:1082`). Trials are bounded by pool size + 8. It terminates; the problem is how long that takes. |
| 13 | "The stored baseline is merely stale — re-measure it once and the comparison is honest" | **Ruled out** | This was the leading hypothesis before §4.3 step 2. The null iterate refutes it: the chain does not settle after one pass. It sheds 2 more contacts and loses further coverage on passes 2, 3 and 4 as well. Re-measuring the baseline once would make the first comparison honest and leave every subsequent one exactly as broken. |
| 14 | "A trial cap would fix it" | **Ruled out as a fix, on the evidence** | On the dust fixture, trial 1 already contains the answer: its appended contact changed the measured coverage by nothing and the recorded difference was entirely the degradation. On `make_raking_blade`, all 45 trials leave the objective bit-identical. In both cases a cap converts a slow refusal into a fast refusal, changes no outcome, and deletes the evidence that the ranking is wrong. |
| 15 | "The instruments are perturbing the result" | **Ruled out** | My own dust trail is **byte-identical** to the behaviour lane's archived trail over all 169 overlapping records — different worktree, different build, different profile (debug assertions off versus on), different machine load. My null-trial numbers match the earlier lane's to the last digit, and my treant trial-1 mask time matches theirs to 0.06 %. Separately, with the probes on, the dust fixture's first four trials are bit-identical in seat, score and measured objective to a run without them. Instrument cost is of order 18 clock reads against a 1.9 s trial. |

| 16 | "The dust fixture's degradation explains the corpus models too" | **Ruled out** | I nearly wrote this report with that generalisation in it. On `Weeping Treant v1.2` the null trial *and* four successive null-iterate passes all return 143.28101209495736 mm² and 482 contacts — a perfect fixed point. The behaviour lane measured the same stability on `make_raking_blade`. The corpus models have no baseline defect, and a fix aimed at §4.3 alone would leave them untouched. |

One correction to the record — a code fact, not a cost. **The ranking is not the only
pool-sized operation per trial.** `best_live_score_mm2` (`coverage_closure.rs:1244`) and
`choose_winner` (`coverage_closure.rs:1144`) each scan every live row on every iteration.
Their cost is bounded by a trial's unaccounted residual, which is under 1 % on both
fixtures, so they are not material — but "ranking is the only pool-sized operation" should
not be repeated as though it were true.

---

## 6. A field in the refusal that lies

`CoverageClosureIsland::nominated_repair_rows` reads `0` in every refusal, including the
accepted red's. Its doc comment (`crates/plinth-support/src/coverage_closure.rs:440`) says:

> **Zero means this island was never even attempted** — it had no unoccupied, unfailed
> repair-pool row … on any attempt that reached nomination.

That reading is false. I checked every write to the field: `stamp_tally`
(`crates/plinth-support/src/runner.rs:3150`) copies only `eligible_rows`, and the counter
`KeyNomination::nominated_rows` is **never incremented anywhere in production code**. Its
one non-zero value in the whole tree is a literal inside a unit test
(`coverage_closure.rs:1468`). Under the current one-winner transaction design there is no
batch nomination step for it to count.

I also measured it rather than only reading it: my own run's ranking snapshot after the third
commit records `nominated_rows: 0, eligible_rows: 985` for the one route — the same pair the
refusal prints.

So the refusal that motivated this investigation reads `nominated_repair_rows: 0,
eligible_repair_rows: 985` and invites the reader to conclude the island was never
attempted, when 274 candidates were in fact tried against it. This causes nothing; it is a
reason previous diagnoses started in the wrong place.

---

## 7. The treant, specifically

`Weeping Treant v1.2` is the corpus's only real miniature: 429,120 triangles, 91.5 mm tall.
I confirmed first-hand that it reaches the repair loop and reports `remaining=8906`
candidates, and that its coverage census reads
`uncovered_samples=39906 blocking_patches=98 repair_targets=891 failing_uncovered_mm2=143.2810
uncensored_uncovered_mm2=170.5124` — reproducing the treant lane's figures exactly. Its
candidate pool holds 54,092 rows, of which 8,906 are eligible, spread over 25 failing routes.

Measured by me, trial 1 (which includes attempt 0's work in its counters):
`chain=137,728 ms measure=17,627 ms total=156,217 ms`, `surface_builds=15 mask_forces=16
mask_ms=229,020`. Its per-stage split, from the planner's own stage lines, is spread across
all six stages rather than concentrated in Rewire as on the small fixtures — Preparation
18.1 s, Routing 33.3 s, Rewire 25.6 s, Brace 18.9 s, Structure 20.5 s — which is what the
whole-mesh staleness check being paid once per stage would look like.

Steady-state trials, measured by me:

| Trial | Surface builds | Mask computations | Mask time | Trial total | Mask share |
|---|---|---|---|---|---|
| 2 | 7 | 7 | 97,895 ms | 145,077 ms | **67.48 %** |
| 3 | 7 | 7 | 110,050 ms | 156,536 ms | **70.30 %** |

So on this model, between two thirds and just over 70 % of every repair trial is spent
recomputing a single memoised value that is never actually memoised. My two trials give a
share of 67.48 %–70.30 %; the treant lane measured trials 2–6 at 148,102–156,817 ms each
with 100,442–107,906 ms of mask time, a share of 67.4 %–68.8 %. The two agree; my upper end
is higher, and my box was more heavily loaded.

I reached trials 2 and 3 only. The steady-state behaviour beyond trial 3, and the full
6-trial series, remain the treant lane's result.

Two multipliers act on the treant at once, and they are independent:

- **Candidate count tracks physical surface area, not triangle count.** The treant lane
  established this with a controlled sweep on `make_raking_blade`, transforming the same STL
  in memory: subdividing 16× (24 → 384 triangles, identical solid) *shrank* the pool from
  371 rows to 198, while scaling ×4 in each dimension (surface area ×16, triangle count
  unchanged) grew it from 371 to 4,532. I did not re-run that sweep. It supports "physical
  scale dominates"; it does not establish a proportionality, because scaling also moves
  contact count, reach relationships and route count together.
- **Cost per trial tracks face count** — the seven cold whole-mesh mask computations of §4.5,
  plus a stage chain over a larger plan.

Their product is why the treant is unmeasurable.

One claim about the treant is **not** accepted here. The treant lane argued that it cannot
possibly seal, because it enters with 98 blocking patches over 25 failing routes, a commit
closes about one route (observed twice), and the commit budget is 8 — so at least 15 routes
must remain uncovered. The observation is real but the inference is not established: two
commits each closing one route does not prove that no commit can close more, and the null
iterate shows a re-plan changes far more than the appended contact. Treat "the treant must
end in a refusal" as unproven.

The treant lane also estimated the run's length from a measured rate of 152.9 s per trial
over 6 trials: draining the 8,065 remaining ranked candidates would take about 1.23 × 10⁶ s,
about **14 days**, and the hard pool bound about **92 days**. Both are estimates extrapolated
from six trials, not measurements, and they assume the per-trial cost stays flat — which
finding #6 supports but which has only been observed over six trials.

---

## 8. What is still open

Listed honestly, including the parts it would be convenient to assume.

1. **The mechanism inside Routing that sheds 2 contacts per pass.** §4.3 establishes *that*
   an unchanged plan degrades, monotonically, and that every pass reports exactly
   `unrouted_no_winner: 2` while exactly 2 contacts disappear. It does not establish why two
   contacts that routed on the previous pass fail on the next one. **This is the first thing
   to instrument next**, and it is where a fix has to start.
2. **How far the dust fixture's decay goes.** I ran four passes and it had not settled — it
   accelerated (the pass-4 loss, 6.665 mm², is larger than passes 1–3 combined). Whether it
   converges to an empty plan, to a stable minimum, or to a refusal is not measured.
3. **Why the dust fixture degrades and the corpus models do not.** This is now a measured
   contrast rather than a guess (§4.3 versus §4.4), and it is unexplained. The dust fixture is
   a synthetic plan assembled inside the test; the corpus models arrive through the full
   pipeline. Whether the difference is the fixture's construction, its size, or something real
   about that geometry decides how much of §4.3 generalises — and therefore how much the Class
   A fix is worth. Nobody has looked.
4. **Whether the gate's 6,202 s for `support_runner_integration` decomposes the way the
   serial per-test numbers suggest.** I did not run the 105-minute gate. The gate runs a
   binary's tests concurrently, one per core, each saturating its own parallel pool, so the
   binary's wall time is not the sum of serial per-test times. The attribution lane flagged
   this and did not test it either. It remains the cheapest unrun experiment.
5. **Why the removal fixture behaves differently.** The attribution lane stopped it at 412
   trials / 1,302.7 s with 3,047 rows still eligible, on the debug-assertions-on profile. Its
   pool *grew* from 1,617 to 3,429 rows after two commits, then drained one row per trial. I
   did not re-run it. The test is not marked as skipped in the source, and the gate baseline
   quoted for `develop` @ `2512347` records exactly one failing test in the whole workspace —
   the dust one — which implies this test completed and passed there. That is inconsistent
   with a serial run that had not finished after 1,302.7 s, and nobody has explained it. I
   did not verify the gate baseline myself.
6. **How much of the 8,906-row treant pool is in the "will be condemned" condition.** Every
   rejected trial on both models that enter the loop had its own contact condemned (45 of 45
   on `make_raking_blade`; the treant's trial 1). Nobody has measured the proportion, and it
   decides whether the ranking is merely uninformative or is ranking things that can never be
   built.
7. **Descriptors per candidate batch inside the rewire optimiser.** Batches per trial are
   counted (384–435 on the dust fixture); the work inside a batch is not, so "fork/join
   overhead" versus "genuinely large batches" is unresolved for the 68.75 %–69.50 % of a
   small-fixture trial that Rewire takes.
8. **What the whole-mesh staleness check is protecting against.** This report establishes its
   cost and its call count. It does not establish what would break without it, and that has
   to be understood before anyone touches it.

---

## 9. What this implies for a fix

**No implementation is proposed here.** The evidence narrows the class of fix that can be
right, and rules out the class that is most tempting.

### 9.1 It is a correctness defect wearing a performance costume

**The chooser and the judge measure different things.** The chooser measures the
straight-line proximity of a proposed point to uncovered area. The judge measures the
coverage of a fully re-planned structure against a stored baseline. There is no reason those
two numbers correlate, and measurement says they do not — the highest-scoring group of trials
delivers a *better* result than the group scoring thirteen times less, 47 times over.

That single defect shows up as two different failures, and they need different fixes:

1. **On the dust fixture, the planner is not idempotent.** Re-running it on an unchanged plan
   loses support and keeps losing it (§4.3). Any procedure that evaluates a candidate by
   re-planning is therefore measuring the decay *plus* the candidate, never the candidate
   alone. The objective moved the wrong way in 271 of 274 trials, and in 46 of them the entire
   recorded difference was the decay.
2. **On the corpus models, the baseline is sound and the candidates are unbuildable.** The
   null trial reproduces the accepted state exactly on `make_raking_blade` and on
   `Weeping Treant v1.2` (§4.4). What fails there is that the score ranks seats no arm can
   reach — 45 of 45 on the blade, and the treant's 100.956 mm² first trial delivering zero.

If the problem were performance, the remedies would be caching, batching, parallelism or a
budget — all of which would make a search that cannot succeed fail faster. Because the problem
is that the ranking does not predict what will be measured, the only fixes that can work are
ones that change **what the planner does on a re-plan** or **what is scored**.

### 9.2 The classes of fix the evidence points at

**Class A — make the stage chain idempotent, or stop re-running it per candidate.** This is
what the dust fixture demands, and it is the term that dominates the accepted red. Two shapes
exist: make a re-plan of an unchanged plan reproduce that plan; or evaluate a candidate
without re-planning the whole structure. *For this to be the right fix it would have to be
true that* the two contacts lost per pass are lost for an identifiable and correctable reason
— open item 8.1, and the reason 8.1 has to be closed first. Choosing before it is closed is
how the previous three attempts were spent. Note the scope honestly: **Class A fixes nothing
on either corpus model**, because neither shows the degradation.

**Class B — make the score predict buildability.** This is what the corpus models demand, and
it is the half that would still be broken after Class A. The score ranks a *point*; nothing in
it asks whether a strut can reach that point, and on `make_raking_blade` the answer was no for
all 45 candidates. Either the score gains a buildability term, or the search stops trusting
the score to rank and learns from measured outcomes instead. *For this to be the right fix it
would have to be true that* enough of routing's verdict is predictable more cheaply than a
full trial. The blade is encouraging — 37 of its 45 refusals share one reason — but the
proportion across a real pool has never been measured (open item 8.6). On the blade, Class B
would also have to solve a second problem: all 45 rows carry an identical score, so a
buildability term is the only thing that could order them at all.

**A and B are independent and both are needed.** A fixes the fixture behind the accepted red;
B fixes the two models a user would actually print. Doing one and declaring victory would
leave the other exactly as it is.

**Class C — the whole-mesh staleness check.** Separable from the search entirely, and the one
place where a purely performance-shaped fix is defensible on the evidence: seven full-mesh
recomputations per trial of a memoised value whose memo is always cold because the object
holding it is always new. *For this to be the right fix it would have to be true that* the
check's guarantee survives whatever makes the value cheaper to obtain — and nobody has yet
established what the check protects against (open item 8.8). Note its limits: it costs about
1 ms per trial on the small fixtures, so it does not touch the gate's 6,202 seconds at all.
It changes only whether the treant can be observed.

### 9.3 What is explicitly not a fix

- **A trial cap or a time budget.** See ruled-out #14. It changes no outcome and hides the
  defect. The existing 120-second-per-model harness budget already does this, and is the
  reason the treant has never been observed to finish.
- **Re-measuring the stored baseline once.** See ruled-out #13. The null iterate shows the
  chain does not settle, so this would make the first comparison honest and leave the rest
  exactly as broken.

---

## 10. The verdict

### CAUSE ESTABLISHED

**The one cause, established:** the candidate score is a straight-line distance test between a
proposed point and uncovered surface samples (code, read directly), and the acceptance test is
a measurement of a completely re-planned support structure. They do not agree. Over my own
complete 274-trial run, 271 trials moved the objective the wrong way, and the highest-scoring
group of trials delivered a *better* result than a group scoring thirteen times less, 47 times
over.

**Established, measured, and reproduced first-hand — dust fixture (the accepted red):**

- **Re-planning an unchanged plan destroys support, repeatedly, and it had not settled after
  four passes.** Four successive chain runs, each fed the previous one's output: contacts
  110 → 108 → 106 → 104 → 102, uncovered area 5.19873046875 → 5.8985595703125 →
  6.53173828125 → 7.33154296875 → 13.99658203125 mm². Every pass reports exactly 2 unrouted
  contacts and sheds exactly 2.
- A repair trial is one such pass plus one appended contact, and acceptance compares its
  fresh measurement against the previous state's stored number — so every candidate pays the
  degradation before its own contribution counts. 46 of the 274 trials measured *exactly* the
  no-contact-added number: their candidate contributed nothing measurable at all.
- Condemnation does not explain it: 98 trials had a contact that routed successfully, and 95
  of those still made the objective worse.
- The search therefore does not converge; it enumerates. 55 trials to the third commit plus
  219 remaining non-zero rows = the 274 the refusal reports, which my run reproduced along
  with `best_remaining_score_mm2: 0.0`.

**Established, measured, and reproduced first-hand — the two corpus models that reach the
loop:**

- **They do not have the degradation.** On `Weeping Treant v1.2` the null trial and four
  successive null-iterate passes all return 143.28101209495736 mm² and 482 contacts — a
  perfect fixed point, where the dust fixture had lost 8 contacts and 8.8 mm² by the same
  point. The behaviour lane measured the same stability on `make_raking_blade`.
- **Their candidates cannot be built.** All 45 of `make_raking_blade`'s candidates were
  condemned (8 quarantined, 37 refused by clearance) and all 45 trials left the objective
  bit-identical. The treant's trial 1 scored 100.956 mm² and delivered zero.
- On `make_raking_blade` all 45 rows also carry one identical score, so the ranking cannot
  even order them.

**Established about cost:**

- Each trial re-runs the whole six-stage planner, which is 97.8 % of the trial; within that,
  Rewire is 68.75 % and Routing 28.61 % on the small fixtures (measured by me; corroborated
  by two other instruments on the other build profile).
- Independently, every trial recomputes a whole-mesh face classification seven times for a
  staleness hash, and the memo that should prevent this is always cold because the object
  holding it is rebuilt each time (code, read directly; the count of seven measured by me on
  three models). On `Weeping Treant v1.2` that is 67.48 %–70.30 % of a whole trial, measured
  by me on trials 2 and 3.

**Not established, and not to be assumed:**

- *Why* Routing loses two contacts on every re-plan of the dust fixture (open 8.1) — this
  gates the Class A fix.
- Whether that decay converges anywhere (open 8.2).
- What proportion of a real candidate pool is unbuildable (open 8.6) — this gates the Class B
  fix.
- That the gate's 6,202 s decomposes as the serial per-test times suggest (open 8.4). I did
  not run the gate.

**The accepted red is neither flaky nor merely slow.** It is the search correctly reporting
that it could not find an improving repair — from a starting position where re-planning alone
costs more coverage than most candidates could ever add, ranked by a score that cannot see
that cost. And the corpus's real model fails a different way for the same underlying reason,
which is why fixing only the red would not make the treant printable.

---

## Appendix A — ruling on each earlier finding

Four investigations fed this report: three measurement lanes and one cross-family challenge.
Each of their findings is ruled here. **ACCEPT** means I checked it first-hand.
**ACCEPT-AMENDED** means the substance holds but the wording or scope was wrong.
**REJECT** means the evidence does not support it. **UNVERIFIED-open** means I could not check
it and it stays open — plausibility never promoted anything.

### Attribution lane (where trial time goes)

| Finding | Ruling | Basis |
|---|---|---|
| Two tests dominate the binary; the other six finish in 13.03 s together | **ACCEPT-AMENDED** | Shape confirmed — my per-model census has every small model under 17 s and the dust test in the hundreds of seconds. The exact per-test wall times I did not re-measure, and they move with machine load. |
| ~99 % of the dust test is inside repair trials | **ACCEPT** | My per-trial lines account for the whole trial (`begin_edit + append + chain + measure = total`), and the only other per-iteration work, the ranking, measured 1 ms. |
| Per-trial cost is flat across the search | **ACCEPT** | My own trials 1–16 span 1,914–2,371 ms with no trend. I did not re-read the rewire counters themselves. |
| Rewire 69.50 % + Routing 26.33 % of trial time | **ACCEPT** | Independently reproduced: 68.75 % / 28.61 % over 30 trials on the other build profile, from the planner's own stage lines. |
| Ranking rebuild costs ~0 | **ACCEPT** | My pool probe records `rank_ms=1` for the initial ranking. |
| Sampling profile: 82.6 % of the main thread under CCO descriptor evaluation | **UNVERIFIED-open** | I did not run a sampler. Consistent with the stage split, which is not the same as checked. |
| Open: whether the gate's intra-binary parallelism explains the 6,202 s | **UNVERIFIED-open** | Still the cheapest unrun experiment. See open item 8.4. |

### Behaviour lane (what the search does)

| Finding | Ruling | Basis |
|---|---|---|
| Null trial: 5.19873046875 → 5.8985595703125 mm², 110 → 108 contacts, same steered and unsteered | **ACCEPT** | Reproduced bit-identically in two independent runs of my own, on the other build profile. |
| No committed trial ever made the objective worse; it is structurally impossible | **ACCEPT** | Verified in `repair_step_outcome` — a commit requires the coverage key to fall strictly. |
| Score distribution and the arithmetic of 274 | **ACCEPT** | Reproduced from my own pool probe: 1,034/54/737 initially; 985/19/766 and `trials: 55` after commit 3; 55 + 219 = 274. |
| `make_raking_blade`: 45 rows, one score, all contacts condemned | **ACCEPT** | Reproduced end to end by me: `trials=45 commits=0`, `quarantined=8 unrouted_no_winner=37`, `own_contact_condemned=45`. |
| `make_raking_blade` has no baseline defect — its null trial reproduces the accepted objective exactly | **ACCEPT, and extended** | I did not re-run the blade's null trial, but I ran the null trial *and* a four-pass null iterate on `Weeping Treant v1.2` and got a perfect fixed point. The lane found this on the blade and did not draw the consequence: **the degradation is specific to the dust fixture and does not affect either corpus model.** That is now §4.4 and ruled-out #16. |
| `nominated_repair_rows` is never written in production and its doc comment misleads | **ACCEPT** | Verified in code and observed as `0` beside `eligible_rows: 985` in my own probe output. |
| Trials whose contact routed still made the objective worse | **ACCEPT-AMENDED** | The lane counted 34 in a partial run; my own complete run gives 95 of 98 such trials over 274. Same conclusion, larger. |
| "Cannot succeed" rather than "slow" | **ACCEPT** | §4.3 now supplies the mechanism the lane could not. |
| Open: "the null trial cannot separate a stale stored measurement from an unstable planner" | **REJECT — resolved** | The null iterate separates them. The chain is not idempotent; a re-plan drifts again on every pass. |

### Treant lane (why the big model is unmeasurable)

| Finding | Ruling | Basis |
|---|---|---|
| Seven cold whole-mesh mask computations per trial, 67–69 % of trial wall | **ACCEPT** | Reproduced on trials 2 and 3: 7 builds, 7 computations, 67.48 % and 70.30 %. Also measured 7 per trial on two other models. |
| `validate_binding` builds a fresh surface, so the `OnceLock` memo is always cold; 6 containment tests per face | **ACCEPT** | Verified in code, both files. |
| Pool size does not drive per-trial cost; this refutes `repair-cap-evidence.md` §4 | **ACCEPT** | Trials 2 and 3 cost the same; the only pool-sized timed operation is 0.4–0.5 %. |
| Pool size tracks physical surface area, not triangle count | **ACCEPT-AMENDED** | I did not re-run the sweep. The data supports "physical scale dominates"; it does not establish proportionality, and subdivision at fixed scale also moved the pool. |
| The search terminates; trials bounded by pool rows + 8 commits | **ACCEPT** | Verified in code. |
| About 14 days to drain, about 92 days to the hard bound | **ACCEPT-AMENDED** | Both are extrapolations from six trials, not measurements. Labelled as estimates wherever quoted. |
| The treant cannot seal within its 8-commit budget | **REJECT as stated** | Two commits each closing one route does not establish that no commit can close more. The observation stands; the inference does not. |
| Open: how much of the pool is in the "will be condemned" condition | **UNVERIFIED-open** | Open item 8.6. |

### Cross-family challenge

| Finding | Ruling | Basis |
|---|---|---|
| "Total work per trial is flat" is unconfirmed — the counters cover rewire, not routing | **ACCEPT** | Correct. Per-trial *wall* is flat; total *work* is not instrumented. |
| "The score is the area the contact actually covers" is refuted | **ACCEPT** | Verified in code: it is a distance test over uncovered samples. |
| A trial is not an isolated intervention, so its before/after difference cannot be assigned to its contact | **ACCEPT-AMENDED** | Correct, and now quantified: §4.3 measures the confound directly, and in 46 trials it was the *entire* difference. |
| "Ranking is the only pool-sized per-trial operation" is refuted | **ACCEPT** | Verified in code — `best_live_score_mm2` and `choose_winner` also scan the pool. Immaterial in cost (under 1 % residual), and said so. |
| Pool size is not exactly proportional to physical area | **ACCEPT** | Reflected in §7. |
| The treant's 8-commit impossibility is unconfirmed | **ACCEPT** | Reflected in §7. |
| The mechanism behind the null mismatch is unproven | **ACCEPT-AMENDED** | It was, and is now half-resolved: non-idempotence is established (§4.3); the reason Routing loses two contacts is still open (8.1). |

---

## Reproducing these measurements

All instruments print only and are inert unless an environment variable turns them on. From
the worktree root, with `corpus/models` symlinked:

```
cargo test -p plinth-support --profile gate --offline --no-run \
      --test repair_probe --test support_runner_integration

# §4.3 and §4.4 null trial and null iterate, §4.5 stage split, §4.7 enumeration
PLINTH_REPAIR_NULL_TRIAL=1 PLINTH_REPAIR_NULL_ITERATE=1 PLINTH_REPAIR_TRIAL_TIMING=1 \
PLINTH_REPAIR_TRIAL_TRAIL_DIR=<dir> PLINTH_REPAIR_POOL_TRAIL_DIR=<dir> \
  ./target/gate/deps/support_runner_integration-* --test-threads=1 --nocapture \
  --exact closure_repairs_censored_dust_because_that_repair_closes_a_real_hole

# §3 per-model census, §4.4 treant null trial, §4.6 mask counters, §7 treant
PLINTH_PROBE_MODEL="make_raking_blade" PLINTH_REPAIR_TRIAL_TIMING=1 \
  ./target/gate/deps/repair_probe-* --ignored --nocapture --exact probe_one_model
```

The instruments live on branch `measure/verify-diagnosis` (this report's measurements) and on
the three lanes' branches `measure/repair-attribution`, `measure/repair-behaviour`,
`measure/repair-treant`. None of them is on `develop`, and none changes production behaviour.

Raw logs for this report are preserved outside the working tree at
`.git/repair-diagnosis-2026-08-30/`:

| File | What it holds |
|---|---|
| `dust2.log` | The complete 274-trial dust run: null trial, per-trial timings, per-stage lines, and the reproduced refusal. |
| `trail2/` | That run's trial trail (274 JSONL records) — the source of §4.2. |
| `pool2/` | That run's ranking snapshots — the source of §4.7. |
| `iterate.log` | The dust null iterate: four passes, §4.3 step 2. |
| `dust-null-only.log` | The first independent null-trial run and its per-stage lines (§4.5). |
| `treant-null.log` | The treant null trial and its four fixed-point iterate passes (§4.4). |
| `treant.log` | The treant timing run: trials 1–3, mask counters, repair-pool lines (§7). |
| `small-models.log` | The per-model census of all seven small corpus models (§3). |

Earlier lanes' raw evidence is at `.git/repair-attribution-2026-08-30/` and
`.git/worktrees/wf_8376a654-27e-2/repair-behaviour-2026-08-30/`.
