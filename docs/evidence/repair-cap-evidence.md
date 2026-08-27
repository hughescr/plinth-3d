# Where the coverage-repair search stops paying for itself — measured

**Scope discipline honoured:** this is a report only. No line in
`crates/plinth-support/src/coverage_repair.rs` or `runner.rs` was edited. Every number
below comes from running the repair controller exactly as it ships today, through
existing tests and the existing `plinth prepare` CLI, and reading its own printed
evidence (`coverage-repair progress:` / `coverage-repair:` lines and the
`CoverageRepairStepWire` list a refusal or panic carries). Machine: the same 16-core
Apple Silicon / 128 GB box as `build-cost.md`, one heavy process running at a time except
where noted.

## Method

For each model, the repair controller was run to its own natural stopping point (pool
exhaustion, zero-live-score refusal, or a committed seal) via the existing test suite or
CLI, with `--nocapture` / stdout capture, and the printed per-~10-30-trial progress line
(`trials=… commits=… remaining=… elapsed=…ms […]`) and the final `CoverageRepairStepWire`
list (trial index, `commit: Some(n)`/`None`, `objective_before/after`) were read directly —
no code change was needed because the loop already prints this on every path (ruling R10).
Two runs (the two largest models) had not reached a natural stopping point when this task's
own budget ran out; those are reported as **censored** observations — a lower bound on the
true tail — exactly as `FIX-AFTER-RUN.md` already did for the 887-trial case that
authorized this task ("did not finish… feed this to the repair-cap evidence lane").
Per-trial wall time was not available as a raw vector without editing `RepairLedger`
(`per_trial_ms` is a private field folded into min/median/max at `finish()`), so time-per-
trial is read off consecutive progress-line deltas, which is exact for the "elapsed at
trial N" question this task asks and coarse only inside a ~10-30-trial window — that
window is far smaller than the effects being measured (a 200x per-trial-cost spread, tail
streaks in the hundreds of trials).

## The five models measured

| Model | Candidate pool (initial) | Trials run | Commits | Last commit trial | Wall time | Median trial cost |
|---|---:|---:|---:|---:|---:|---:|
| `make_raking_blade` (synthetic, 45 candidates) | 45 | 45 (to pool exhaustion) | **0** | never | 24.6 s | 551 ms |
| `make_uncovered_rim_table` (synthetic, the new SPEC-A4 witness) | 540 | 2 (sealed) | 1 | 2 (the last trial) | 3.0 s | ~1.5 s |
| `shelf_with_dust_beside_a_row_less_hole` (synthetic, `closure_repairs_censored_dust…`) | 1,034 | 274 (to zero-live-score refusal) | 3 | 55 | 670.5 s | 2.38 s |
| `half_buried_table` (synthetic, `a_spec6_removal…`) | 1,617 → 3,167+ (grows on each commit's rescore) | **268+, censored, still running** | 2 | ≤17 | **1,319 s+ and climbing** | ~4.7 s (rising) |
| **Weeping Treant v1.2** (real corpus flagship model, `plinth prepare`, pinned orientation) | 10,222 | **4+, censored, still running** | 1 | 2 | **439 s+ in the repair loop alone (454 s more in pre-repair legalization), climbing** | **~110 s** |

Raw evidence: `dust-run.log` (complete, includes the full step-by-step JSON from the panic),
`spec6-run.log` and `treant-run.log` (both still growing at the time this report was
written — re-`tail` them for anything past what is quoted here).

## 1. Last committing trial index, per model

- `make_raking_blade`: **no commit ever occurs.** The whole 45-trial run is rejections; the
  loop stops because the candidate pool drained (`remaining=0`), not because anything
  plateaued.
- `make_uncovered_rim_table`: **trial 2 of 2** — the run's last trial IS its only commit,
  and sealing follows immediately (`blocking.is_empty()`).
- `shelf_with_dust_beside_a_row_less_hole`: **trial 55 of 274.** Exact commit trials, read
  from the panic's own `CoverageRepairStepWire` list: `trial 47 → commit 1`,
  `trial 54 → commit 2`, `trial 55 → commit 3`. Nothing after trial 55 improves anything,
  for the remaining 219 trials.
- `half_buried_table`: both of its 2 commits land at or before trial 17 (the first progress
  line already shows `commits=2` at `trials=17`); nothing has improved since, through at
  least trial 268 (39+ minutes of continuous running) as of this report.
- Weeping Treant: 1 commit at trial 2, of only 4 trials observed before this task's time
  budget was spent. Too little of this run happened to say where ITS last useful trial is —
  which is itself the headline finding of the next section.

**There is no clean "last useful trial" number that holds across models.** It ranges from
"the final trial" (rim table) to "never" (raking blade) to "trial 55 of at least 274"
(dust/shelf) to "unknown, but past at least trial 268 and 39 minutes" (half-buried) to
"unknown, only 4 trials have run" (the real flagship model this whole architecture exists
to serve).

## 2. Distribution of no-improvement run-lengths before a commit

This is the number a cap would have to exceed to be free (to never remove a commit the
uncapped search would have found):

| Model | No-improvement streaks that precede a later commit |
|---|---|
| rim table | 1 (trial 1 rejects, trial 2 commits) |
| dust/shelf | **46** (trials 1–46, before commit at 47), then 6 (trials 48–53, before commit at 54), then 0 (commit 54 → commit 55 back to back) |
| half-buried | ≤15, split across at most 2 segments before trial 17 (both commits land inside trials 1–17; the coarse first progress line does not resolve the split further) |
| raking blade | N/A — no commit exists to protect |
| Treant | 1 (trial 1 rejects, trial 2 commits) — only one commit observed so far |

**The largest measured pre-commit streak across every model tried is 46**, on the model
that also has the *smallest* candidate pool of the three real "needs repair" fixtures
(1,034, versus 1,617 and 10,222). A cap below ~50 is already demonstrated unsafe on a model
in this small measured set; a cap of 50–100 is safe for everything reported here **only
because the two largest, most production-relevant models were not run to completion** —
see §4, this is exactly where a fixed number stops being trustworthy.

## 3. Wall-clock after the last useful commit — the prize

| Model | Total wall time | Wall time after last commit | Fraction wasted |
|---|---:|---:|---:|
| rim table | 3.0 s | 0 s | 0% |
| raking blade | 24.6 s | 24.6 s (no commit ever; all of it is spent proving nothing helps) | 100% |
| dust/shelf | 670.5 s | **≈522 s** (interpolated: elapsed at trial 55 ≈ 148 s, elapsed at 274 = 670.5 s) | **≈78%** |
| half-buried | 1,319 s+ and climbing | **≈1,257 s+ and climbing** (elapsed at trial ≤17 ≈ 63 s) | **≈95%+ and climbing** |
| Treant | 439 s+ so far | too early — only 2 trials past the one commit | n/a yet |

The prize is real and it is large on every model that ever commits at all: 78% of one
model's run and, so far, 95%+ of a bigger one's, is spent after the point where nothing
more will be learned. The two synthetic "needs repair" fixtures alone account for roughly
14.5 minutes of pure post-commit waste in this measurement session, and the real corpus
model's own tail has not even started to be visible yet.

## 4. Does the last-useful-trial index scale with anything? — the physics-first question

**No — not with candidate-pool size, and not monotonically with any size proxy measured.**
Ranking the three "needs repair" models by initial candidate pool:

| Candidate pool | Last commit trial |
|---:|---:|
| 1,034 (dust/shelf) | 55 |
| 1,617 (half-buried) | ≤17 |
| 10,222 (Treant) | 2 (so far) |

**The relationship runs backwards from what a size-derived cap would assume**: the model
with the *smallest* pool of the three took the *most* trials to find its last improving
move, and the two much larger models found theirs almost immediately. This is not a small
effect — it's a >25x spread in the wrong direction. A cap derived from patch count, contact
count, or candidate-pool size would therefore not merely be imprecise, it would point the
wrong way: sized to be safe on Treant (last-useful ≤ 2 so far) it would starve dust/shelf of
two of its three commits; sized to be safe on dust/shelf (last-useful = 55) it does nothing
useful for Treant, whose problem is not trial *count* at all.

**What DOES scale hard with model size is the cost of a single trial**, and this is the
real, measured, physics-first finding:

| Model | Candidate pool | Median/typical trial cost |
|---:|---:|---:|
| raking blade | 45 | 551 ms |
| rim table | 540 | ~1.5 s |
| dust/shelf | 1,034 | 2.38 s |
| half-buried | 1,617→3,167 | ~4.7 s and rising |
| Treant | 10,222 | **~110 s** |

That's a ~200x spread in per-trial cost across a ~230x spread in candidate-pool size —
close to linear in this sample, consistent with REVIEW.md finding #12's own complexity
claim (`O(M² + K(M+S))`, `S` ≈ 4×10⁵ samples on a flagship model dominating at this scale).
**The quantity that scales with model size is time-per-trial, not number-of-useful-trials.**
Any bound expressed as a trial *count* therefore has a completely different real-world cost
depending on which model it runs against: a cap of "50 trials since improvement" costs at
most ~28 s on the raking-blade-sized end of this sample and **at least 92 minutes** on the
Treant-sized end (50 × 110 s), for the same nominal number.

## 5. What would candidate cut-offs have cost — 10 / 25 / 50 / 100 trials since improvement

Applying "refuse after N consecutive no-improvement trials" retroactively to the runs that
finished, using each run's own `objective_before`/`objective_after`/`commit` stream:

| Cutoff N | rim table | raking blade | dust/shelf | half-buried (censored) |
|---|---|---|---|---|
| **10** | safe (streak never reaches 10 before its 1 commit) | no change (no commit exists to lose either way; only wall time changes) | **LOSES commits 1–3.** Streak reaches 10 at trial 10, 36 trials before the first commit at 47. Sealed/refused residual failing area would be **5.19873 mm²** (the attempt-0 state) instead of the measured **0.89978 mm²** — **≈5.8x worse**, and 2 of the run's 3 commits never happen. | cannot say — the pre-commit segments are ≤15 total but the split is unresolved; real risk of losing one or both of the 2 commits |
| **25** | safe | no change | **same loss as N=10** (streak of 46 still exceeds 25 well before trial 47) | same uncertainty as N=10 |
| **50** | safe | no change | **safe.** The 46-trial pre-commit streak is *just* under 50, and after each commit the streak resets to 0, so the cutoff never fires before trial 55. Refuses at trial ~105 instead of 274 — **identical final objective (0.89978 mm²)**, saves ≈169 trials ≈ 406 s (≈78% of the tail recovered) | both commits land by trial ≤17, comfortably inside 50 — safe for what has been observed so far, but the run has not been observed to stop improving forever, so "safe" here means "loses nothing we know about," not "loses nothing" |
| **100** | safe | no change | safe, refuses at trial ~155, saves ≈119 trials ≈ 285 s (≈55% recovered) | same caveat as N=50 |

**How many models would have sealed with a worse result:** at N=10 and N=25, **1 of the 5
measured models** (dust/shelf) seals materially worse — 2 of its 3 real improvements are
never found, and its reported residual defect is measured **5.8x larger**. At N=50 and
N=100, **0 of the models that finished** show any loss, but **2 of the 5 models — the
largest synthetic fixture and the only real corpus model tried — were never observed to
stop improving**, so "0 models lost" at N=50/100 is not the same claim as "N=50/100 is safe
in general." It is the claim this measurement session can support and no more.

## 6. Recommendation on the SHAPE of a bound

**A fixed trial-count cap is the wrong shape.** §4 shows the number of trials needed to
reach the last useful one does not scale with model size — it ran backwards in this sample
— so there is no way to pick one count that is both tight enough to save real time on a
flagship model and loose enough not to starve a smaller model's real commits. §5 already
demonstrates a concrete loss (dust/shelf, N≤25) inside the very range that looked
attractive before this measurement.

**A cap derived from patch count / contact count / candidate-pool size is also not
supported.** §4's own table is the evidence against it: ranked by candidate-pool size, the
last-useful-trial index goes 55, ≤17, ≤2 — decreasing, not increasing. Any formula of the
shape "cap = f(pool size)" fitted to this data would have to be a *decreasing* function of
model size, which is not what "physics-first, derived from a real quantity" is supposed to
mean here — it would be curve-fitting noise from a 3-point sample, which is the thing this
doctrine exists to prevent.

**What the evidence does support, if the owner wants a bound at all:** the one thing that
scales cleanly with model size is time-per-trial (§4), and the one thing that is genuinely
about the search having stopped learning, independent of scale, is trials-since-the-last-
COMMIT rather than trials-since-the-start (§2, §5 — resets on every commit, so it never
penalizes a model that keeps finding real improvements no matter how many trials that
takes). Between the two REPORT-only shapes on offer:

- **A wall-clock budget** (report, don't silently refuse, when a run's `wall_time_total_ms`
  crosses a threshold) matches the quantity that actually scales with model size, and
  costs the same real minutes regardless of which model triggers it — which a trial-count
  budget provably does not (§4's 28s-vs-92-minutes example for the same nominal "50").
- **A no-improvement-since-last-commit plateau**, expressed as a trial count reset on every
  commit rather than a total trial cap, is the only trial-count shape this data does not
  immediately falsify — but §5's own table shows it is currently only *demonstrated* safe
  at N≥50 on the 3 models that finished, and explicitly *not yet demonstrated* safe on the
  2 models — the largest synthetic fixture and the only real corpus model — where the
  answer matters most.

**The honest answer the evidence supports is: not yet.** Two of five measured models,
including the only real production-scale model tried, have not been observed to reach a
point where further trials stop helping. Craig's ruling R10 already made this trade-off
explicitly and on doctrine ("a number that steers is a cap and it must never decide an
outcome... R10 removes the cap and replaces it with visibility, and that trade only holds
if the visibility is real") — and every measured run in this report did have that
visibility: none of these six-figure-millisecond runs were silent. **Any of the shapes
above would decide outcomes** the moment its threshold is crossed on a model that would
have gone on to commit again — which is exactly the failure mode R10 was written to avoid,
and this measurement session cannot rule it out for the models where the wall-clock prize
is largest. If the owner wants to revisit R10 anyway, the evidence here says the number
should come from re-running `half_buried_table` and a real corpus model like Weeping Treant
to their own actual, uncapped, natural stop — which this session's time budget did not
reach — rather than from the 3 models that happened to finish inside it.

## Appendix: exactly what ran and where the evidence lives

- `dust-run.log` — `cargo test -p plinth-support --test support_runner_integration
  closure_repairs_censored_dust_because_that_repair_closes_a_real_hole -- --nocapture
  --test-threads=1`. Completed: 274 trials, 3 commits (trials 47/54/55), refused via the
  zero-live-score guard at `remaining=766`, `candidates_tried: 274`. This is the SAME
  pre-existing, deliberate red the code review's own `FIX-AFTER-RUN.md` already verified at
  `5d0420e` (`candidates_tried: 274` — reproduced bit-for-bit here); nothing about this run
  was caused by this task.
- `spec6-run.log` — `cargo test -p plinth-support --test support_runner_integration
  a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule --
  --nocapture --test-threads=1`. **Still running** when this report was written: 268+
  trials, 1,319+ seconds, commits stuck at 2 since trial ≤17.
- `treant-run.log` — `cargo run --release -p plinth-cli --bin plinth -- prepare
  --input-models-dir <scratch copy of corpus/models/Weeping Treant v1.2.stl> --output
  saturn4-ultra --out-dir <scratch> --orient pinned:corpus/baseline`. **Still running**: 454s
  of pre-repair legalization (a different subsystem, Stage D, not itself part of this
  finding) followed by 4+ repair trials at ~110s each, 1 commit at trial 2.
- `treant-in/`, `treant-out/` — scratch copies made for the CLI run; not part of the repo,
  safe to delete.
- No file under `crates/`, `rust/`, or any production path was modified. `git status` is
  clean of anything but this report and the scratch directories above, both outside the
  repository.
