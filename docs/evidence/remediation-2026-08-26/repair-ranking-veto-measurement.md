# Making the repair ranking model the veto — measured, and rejected

**Date** 2026-08-28. **Branch** `feature/repair-ranking-models-veto`, off `develop`
(`f685aac`) with `feature/repair-ledger-records-why` merged in first.
**Binary** `--release`, built after the ranking commit and verified by timestamp
(commit 12:47, binary 12:49). **Machine** 16-core Apple Silicon / 128 GB.
**Harness** `crates/plinth-support/examples/repair_baseline.rs`, taken verbatim from
`measure/repair-baseline` and deliberately left untracked.

## The verdict

The veto-aware ranking was built, measured on every fixture that enters the repair
loop, and **reverted**. It moved a committed contact on the dust fixture and made
that search **3.7× longer**. It saved nothing anywhere.

The deeper result is that no re-ordering could have done much better, and that is
worth more than the change was.

## What was built

`choose_winner`'s key went from

    (marginal_area_mm2, diversity_discs, order, key)

to

    (score tier, condemned vetoes near the seat, score, discs, order, key)

where the tier counts whole censor-widths of marginal area, the censor being the
registered A3 censor the function already takes, and the veto count comes from
`RepairVetoIndex` at the same radius the diversity discs use. `own_contact_condemned`
is the column the 2026-08-28 baseline singled out: 222 condemned trials on the dust
fixture, 222 rejected, against 4 commits none of which was condemned.

Tiering was the answer to the reverted disc-first rule: a count that can only
re-order inside one censor-width cannot lift a 0.00 mm² seat above a 13.68 mm² one,
because those two are about seventy tiers apart. That argument holds — it was pinned
by a test and the test passed. The rule is still wrong, for a different reason.

## Measured, per fixture

Trials and digests, control (this branch with the ranking change reverted) against
the veto-aware ranking. The control reproduces the 2026-08-28 baseline exactly on
all five fixtures, which is what makes the comparison a comparison.

| fixture | control trials | veto trials | control digest | veto digest |
|---|---|---|---|---|
| `toed_slab` | 0 | 0 | `13198418135389558006` | `13198418135389558006` |
| `overhang_table` | 0 | 0 | `572621525405166924` | `572621525405166924` |
| `uncovered_rim_table` | 2 | 2 | `10727171327057596482` | `10727171327057596482` |
| `raking_blade` | 45 | 45 | `refusal:3153703192024778472` | `refusal:3153703192024778472` |
| `dust_shelf` | 274 | **1017** | `refusal:6678279176435689841` | `refusal:4490223906269365525` |

`half_buried_table` was not run: the control alone censors at 2,100 s, and the dust
fixture had already settled the question.

### The outcome moved

On `dust_shelf` the three committed contacts are not the same three:

| | control | veto ranking |
|---|---|---|
| commit 1 | trial 47, seat `17.0625, 1.625, 24.0`, 2.599 mm² | trial 47, **same seat** |
| commit 2 | trial 54, seat `19.6875, -4.875, 24.0`, 2.399 mm² | trial 55, seat `19.6875, -3.25, 24.0`, **2.266 mm²** |
| commit 3 | trial 55, seat `6.5625, 2.4375, 24.0`, 1.333 mm² | trial 56, same seat |

A different second contact is a different support, built at a different place on the
model. That alone disqualifies the change under the standing constraint, before any
argument about cost. The refusal digest moving is consistent with it, though the
digest is not the proof: `candidates_tried` lives inside the refusal reason, so a
search of a different length moves that digest by itself. The proof is the seat.

### And it cost more, not less

    control       274 trials   578,341 ms   exits with 766 live rows all at 0.00 mm²
    veto ranking 1017 trials 2,032,143 ms   exits with  23 live rows all at 0.00 mm²

Median trial cost is unchanged (1,988 ms against 1,990 ms), so the whole 3.5× wall is
extra trials. The plan the veto ranking accepted covers less, which leaves far more of
the 1,040-row pool still scoring above zero, which is what the search then has to
sweep. Demoting whole neighbourhoods on measured evidence spread the search out, and
spreading it out is what cost the better second contact.

## Why no re-ordering was going to help

The repair loop's cost is not order-limited. It is pool-limited.

Read the loop (`crates/plinth-support/src/runner.rs`, the `loop` at ~2290): every
iteration increments `trials` exactly once and then either commits or calls
`ledger.reject`, which inserts exactly one row id into `ledgered`. The loop returns
only when `choose_winner` finds no live row, or when `best_live_score_mm2` — the
maximum score over every row not yet ledgered — is `0.0`. So a run with `C` commits
performs `C + |ledgered|` trials, and at exit every live row scores zero.

**A permutation of the trial order cannot delete a row from that set.** It decides
which row is spent when, and nothing else. The only lever on the count is where the
commits land, because a commit is the one thing that rebuilds and rescores the pool.

Measured, this is not a subtle effect:

- **`raking_blade`.** The pool holds 45 rows; the run performs 45 trials and exits
  with `remaining=0`. It drains the pool completely. There is no ordering of 45 rows
  that tries fewer than 45 of them. Confirming this, the veto ranking's trial stream
  is **byte-identical** to the control's — same rows, same order, same 45 trials.
- **`dust_shelf`.** The pool holds 1,034 rows. The control commits at trials 47, 54
  and 55 and then performs **219 more trials, 80% of the run**, after the accepted
  plan is final. Those 219 rows are determined by the accepted plan alone. No
  ordering removes one of them.

So the addressable prize on the dust fixture is at most the 52 rejections that
precede the last commit — 19% — and only for an oracle that picks all three
committers first *and* only if all 52 of those rows then score zero. On
`half_buried_table`, where both commits land by trial 20 and 721 exhaustion trials
follow, the ceiling is 2.6%.

**The brief's headline prize is not a re-ordering prize.** "78% of the run happened
after the last change that mattered" is the cost of *proving* that no further repair
helps, and that proof is a full sweep of every row that still scores above zero.
Re-ordering the sweep does not shorten it.

## Two premises in the brief that do not reproduce

Both are worth recording because the next lane will otherwise design against them.

**1. There is no thrown-away-improvement population.** The brief says "119 of 224
trials found a strict improvement and threw it away" on a clause-2 hard-gate veto.
Measured on every fixture that enters the loop, `clause1_improved_then_thrown_away`
is **0** and the clause-2 veto class fires **0** times — matching the 2026-08-28
baseline's own finding across 1,062 trials. Every rejection on this corpus is
clause 1: coverage did not improve. The 119/224 figure looks like the *latent*
hard-gate rise (130 of 274 on the dust fixture), which is a rise measured on trials
that had **already failed clause 1**. A ranking cannot rescue moves that were never
good, and there is no fixture on which a change to clause 2's ranking can be
validated in either direction.

**2. Scores are not generically unique, and the discs already vote.** The brief
argues the later key terms are unreachable because `marginal_area_mm2` is an `f64`
sum. It is a sum over a quantized sample grid, and it ties constantly. All 45
`raking_blade` rows score **exactly** 5.194999999999992 mm². The 274 `dust_shelf`
trials cover **24 distinct score values**. The run reports confirm the discs are
consulted and are simply not decisive: `raking_blade` records
`diversity_demotions=44 of_which_tried=44`, and `dust_shelf`
`diversity_demotions=699 of_which_tried=265`. Rule N4 is not starved of a vote; it
casts one, and on these fixtures there is nothing better to promote.

## Where the cost actually is, for whoever takes this next

Not in the ranking. On the dust fixture, 176 of the 274 trials had their own appended
contact condemned — quarantined or retired with no winner — so no arm was ever built
at the seat, and all 176 were rejected. Each still paid a full prepare → route →
rewire → brace → structure → lower → **measure** pass. `own_contact_condemned` is
known after routing and *before* `measure_for_controller` runs. Whether a condemned
own contact can ever commit is a physics question with a testable answer, and if it
cannot, the measure on those trials is provably dead work that no candidate ordering
can reach. That is a cheaper-trial lane, not a fewer-trials lane, and it does not
have to move a single outcome to pay.

## Reproduce

    cargo build --release --offline --example repair_baseline -p plinth-support
    PLINTH_REPAIR_STEPS_OUT=<dir>/<name>.steps.json \
      ./target/release/examples/repair_baseline <name>

one fixture per process, nothing else on the machine. `dust_shelf` takes 578 s on the
control and 2,032 s under the reverted ranking.

**Disclosed overlaps.** A `cargo check`, a `cargo clippy` and one `--lib` test run
overlapped the control `dust_shelf` run; a `cargo clippy`, a `rustfmt` and one
`--lib` test run overlapped the veto `dust_shelf` run. Trial counts, committed seats
and digests are unaffected by machine load, and the two runs' median trial costs
(1,988 ms and 1,990 ms) agree to 0.1%, so the wall-time comparison survives it too.
