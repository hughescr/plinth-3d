# The red baseline: one cause, nine tests, one commit

**Verdict:** possibility **(b)** — *something upstream changed so the fixtures no longer HAVE
the coverage hole they were built to have, and the tests are now vacuous rather than wrong.*

**The commit:** `3a6526f` — `fix(support): converge the clearance mask and seat the model
anchor (#51,#36)`, Craig R. Hughes, 2026-08-21 11:04:27 -0700. It is on the
`feature/a4-controller` lane, merged to `develop` at `225b6a7`.

**All nine baseline failures have this single cause.** Not four fixes, not nine — one fact.

**There is no production regression to repair.** The behaviour change `3a6526f` made is a
fix, argued from arithmetic and measured at both ends by its author. What broke is the
*fixtures*: two procedural models that were chosen precisely because they needed coverage
repair stopped needing it, and every test that reads a committed repair step lost its
witness. `3a6526f` says so in its own commit message and left the tests red on purpose.

**Nothing here belongs to short-list 5 or theme 3.6.** See "What this is NOT", below. The
strong hint in the task brief is wrong, and it is worth saying plainly so the later stage
that owns `runner.rs:7320` / `specialized.rs:2405` does not go looking for this there.

---

## 1. What actually happens

`plinth_support::run_support` measures routed coverage once at attempt 0
(`crates/plinth-support/src/runner.rs:2134`, `measure_for_controller` at
`runner.rs:1245`). Every measured patch that needs repair becomes a `BlockingPatchKey`.
Then, at `runner.rs:2159`:

```rust
if measured.blocking.is_empty() {
    // The zero-cost path: a model whose baseline layout already covers
    // never enters a transaction ...
    let report = crate::coverage_repair::RepairRunReport {
        legalization: legalization_disclosure,
        ..Default::default()
    };
    report.log_summary();
    return Ok(seal_authority(*attempt_zero, &measured.repair_targets, report, Vec::new()));
}
```

`make_overhang_table` and the corpus `Sphere` now take that branch. `blocking` is **empty** —
their baseline support layout covers every island at the registered radius, so no
transaction is opened, no trial is run, no step is committed, and the report is a true
report of zero. The tests assert `commits >= 1`, so they fail.

The most direct proof is the ninth failure's own panic text, which prints the blocking list:

```
crates/plinth-support/tests/support_runner_integration.rs:586:
this fixture must NOT close on its baseline layout -- that is what makes it the
success-after-repair demonstration: []
```

`[]`. Zero blocking patches. The fixture's premise is gone.

And `a4_transactional_controller.rs:202` states the diagnosis in advance, in the words of
whoever wrote it:

```
this fixture is here BECAUSE it needs repair; if it stopped needing any, the test has
stopped testing the repair path: RepairRunReport { trials: 0, commits: 0, ... }
```

## 2. The bisect

Test binary `crates/plinth-pipeline/tests/a4_production_door.rs`, run as
`cargo test -p plinth-pipeline --test a4_production_door` in a detached worktree sharing the
main `target/`. 57 code-touching commits in the search range; six builds.

| commit | | `a4_production_door` |
|---|---|---|
| `1509752` | the door test lands | **3 passed** |
| `b5dc266` | `test(load): prove contract (2) is DELIVERED …` | **3 passed** |
| `f9c7d07` | `docs(support): derive the model-seated arm clearance rule …` (parent) | *(see below)* |
| **`3a6526f`** | **`fix(support): converge the clearance mask and seat the model anchor`** | **2 FAILED** |
| `3fe332b` | `feat(support): carry the frozen seat zone on a Forest side table` | 2 FAILED |
| `bcc84f7` | `test(support): make a sabotaged tier read loud …` | 2 FAILED |
| `94057f0` | tip of the a4 lane before the f1 fold | 2 FAILED |
| `f3e2258` | this branch, today | 2 FAILED |

`3a6526f`'s immediate parent is `f9c7d07`. The Sphere pair was bisected across exactly that
parent/child pair, with `corpus/models` symlinked into the worktree (the STLs are gitignored,
so a bare worktree fails those two tests at `first_artifact.rs:61`,
`RequiredCorpusModelMissing` — a different failure that must not be mistaken for this one):

| commit | `corpus_sphere_drilled_plan_is_refused_before_any_file_exists` | wall time |
|---|---|---|
| `1509752` | **ok** (refused on coverage, as the test demands) | 458.36 s |
| `f9c7d07` | **ok** (refused on coverage) | 458.15 s |
| `3a6526f` | **FAILED** — the Sphere seals at attempt 0 and **emits a 1,270,824-byte artifact** | 3.37 s |

The 458 s → 3.4 s collapse is the same fact seen from the other side: at `f9c7d07` the
Sphere spent seven and a half minutes in the repair loop and still could not close; at
`3a6526f` it has nothing to close.

The f1 re-freeze lane (`a4f5f3f` and the rest of `94057f0..7e11319`) is **not** the cause —
the failures predate the fold. That lane is the attribution for a *different* Sphere move,
the zero-field divergence re-record at `7eb5f30`, and conflating the two is easy.

## 3. Why the behaviour change is right, and the fixtures are what broke

`3a6526f`'s message carries the derivation. `cco::clearance_rows` masked the seat end of a
model-seated anchor arm by a flat `standoff_mm`, then demanded of the kernel
`distance >= r_max + standoff`. The seat is **on** the surface, so the last tested station is
at most `standoff` from the surface while `standoff` plus the arm's own radius is required:

```
clearance = d_min - (r_max + standoff) <= -r_max <= -0.35 mm,  on any geometry
```

The test was unsatisfiable as arithmetic. Measured at the time: `-0.350 mm` exactly, attained
53 times; all 31,627 feasible candidate arms refused on Warhorse island 5, 253 of 253
terminals. The commit replaces the flat inset with the single `contact::placed_mask_inset_mm`
(`crates/plinth-support/src/contact.rs:142`) that `brace.rs` already used, and pairs it with
PR-3 (`contact.rs:154`) so that a mask growing past the member's own length cannot hand the
kernel a degenerate segment and get `INFINITY` — an admission with no distance test at all.
That guard is the difference between a loosening and an erasure, and it is present.

The consequence is that the model-seated anchor family can route at all, for the first time.
The overhang table and the Sphere are covered by arms that were previously refused by
arithmetic. That is the lane's stated prize.

Three controls in the commit show the change is targeted rather than a blanket "everything
covers now", and all three still hold on this branch today:

* `make_toed_slab`'s sealed digest `4_190_462_360_230_124_963` is bit-identical
  (`toed_slab_seals_with_zero_trials_and_the_frozen_digest_does_not_move` — passes);
* `make_raking_blade` still refuses on coverage, and still runs a real loop — 30 trials, 0
  commits, 15 rows remaining at `develop`
  (`procedural_raking_blade_is_refused_before_any_file_exists`,
  `the_raking_blade_refuses_with_a_measured_class_per_route` — both pass);
* the report wire is intact: `a_model_that_needs_no_repair_still_reports_that_it_spent_nothing`
  passes, and every failing test prints a `Some(RepairRunReport { … })` carrying real
  `legalization` data, not `None`.

`3a6526f` touches `anchor.rs`, `brace.rs`, `cco.rs`, `contact.rs`, `rewire.rs`, a new
`tests/retired_formula_guard.rs`, and 21 lines of `runner.rs` **which are inside
`mod tests`** — one re-pinned brace-termination expectation, `Unplaceable` -> `Converged`,
attributed by control. It does not touch `coverage_measurement.rs`, `coverage_closure.rs`,
`coverage_repair.rs`, or any production line of `runner.rs`.

### The other two possibilities, ruled out by evidence

* **(a) the coverage-repair controller regressed and no longer runs when it should.** No.
  It runs on `make_raking_blade` at `develop`: 30 trials, 15 rows still ranked, a measured
  rejection class per route. A controller that had stopped running would not do that.
* **(c) the work happens but is not disclosed on this path.** No. The report is `Some` on
  every failing run, and it carries non-default content (`legalization: { passes: 1, … }`)
  which can only have come from the sealed authority one frame before it is consumed — the
  exact wire `a4_production_door` exists to guard. The counters are zero because the loop
  was never entered, not because the read was cut.

## 4. The nine failures, and the one thing they cost

| # | test | file |
|---|---|---|
| 1 | `the_production_door_reports_what_the_one_winner_controller_spent` | `crates/plinth-pipeline/tests/a4_production_door.rs:105` |
| 2 | `the_sealed_ledger_on_disk_names_the_row_that_closed_the_hole` | `crates/plinth-pipeline/tests/a4_production_door.rs:218` |
| 3 | `every_committed_step_on_the_overhang_table_proved_all_three_accept_clauses` | `crates/plinth-support/tests/a4_transactional_controller.rs:202` |
| 4 | `a_committed_step_names_the_row_that_closed_the_hole_and_the_routes_it_covered` | `crates/plinth-support/tests/a4_transactional_controller.rs:315` |
| 5 | `the_production_ranking_keeps_every_eligible_row_and_not_one_per_patch` | `crates/plinth-support/tests/a4_transactional_controller.rs:705` |
| 6 | `the_loop_streams_progress_before_it_prints_its_summary` | `crates/plinth-support/tests/a4_transactional_controller.rs:781` — the sealed run emits no progress line at all, because it never enters the loop; its `commits >= 1` assertion at `:874` is never reached |
| 7 | `corpus_sphere_drilled_plan_is_refused_before_any_file_exists` | `crates/plinth-pipeline/tests/first_artifact.rs:191` |
| 8 | `a_coverage_refusal_removes_a_stale_artifact_already_at_the_target` | `crates/plinth-pipeline/tests/first_artifact.rs:224` |
| 9 | `public_runner_closes_overhang_table_only_after_appending_repair_contacts` | `crates/plinth-support/tests/support_runner_integration.rs:569` |

1–6 and 9 read `make_overhang_table`. 7–8 read the corpus `Sphere`.

**The real cost, and it is worth stating loudly.** Every assertion in the workspace that a
repair step was ever COMMITTED lives in one of these tests:

```
crates/plinth-pipeline/tests/a4_production_door.rs:106        repair.commits >= 1
crates/plinth-pipeline/tests/a4_production_door.rs:219        ledger.commits >= 1
crates/plinth-support/tests/a4_transactional_controller.rs:203  report.commits >= 1
crates/plinth-support/tests/a4_transactional_controller.rs:874  sealed_report.commits >= 1
crates/plinth-support/tests/support_runner_integration.rs:587   coverage_repair_rounds() >= 1
```

All five are red. **Since 2026-08-21 the repair-COMMIT path has had no passing test in this
workspace at all.** The refusal path is still covered (`make_raking_blade`); the zero-cost
path is still covered (`make_toed_slab`); the path where a trial wins, a contact is appended,
the plan changes and §D.2's attribution is written to disk is covered by nothing. That is
the finding, not the red count.

## 5. Where the fix belongs — and why it is not this task's to make

The fix is a **fixture**, not a code change: a procedural model that still needs repair AND
can close from its own repair pool. `3a6526f`'s own message names the constraint:

> …it leaves the repair-COMMIT path without a procedural fixture. Replacing it needs a new
> checked-in corpus baseline, which is #39's lane, not #51's. Re-freezing four acceptance
> tests against a new expectation is the edited expectation OD-8 forbids, so they are left
> failing for the judge to rule on.

Three reasons this stops here rather than being made green now:

1. **OD-8.** Tests 3, 5, 6 and 9 carry frozen measured numbers pinned to
   `make_overhang_table` — "measured at 1,011 on this fixture", island 5 at 5.135 mm², a
   commit count that a streamed progress line must agree with. Re-pinning those to a new
   fixture is editing an acceptance expectation, which this project's OD-8 forbids without a
   ruling. The author deliberately deferred it to the judge; that deferral is still open.
2. **`corpus/baseline/*.pipeline.json` cannot be regenerated.** Those files are output of the
   retired Python engine (`"error": "refusing to emit: engine verify.ok is False…"`).
   `plinth_orient::selection_from_report` reads only the hollow stage's orientation axis and
   angle out of them, so a hand-authored pin is technically possible — but minting a
   "ratified" baseline for a fixture nobody has ratified is a decision for the owner, not a
   safety-net task.
3. Tests 7 and 8 are a separate, smaller call. The Sphere is no longer a coverage-refusal
   fixture. `make_raking_blade` still is, and `procedural_raking_blade_is_refused_before_any_
   file_exists` already covers test 7's claim on it. Test 8's claim — *a refusal unlinks a
   stale artifact sitting at the target* — is covered nowhere else and should be re-pinned to
   `make_raking_blade`, which needs no corpus provisioning. That is a one-fixture change with
   no frozen number attached, and it is the one piece of this that a later lane can land
   cheaply.

**Recommended route for whoever picks this up (#39's lane, or the fixture lane):**

* Cheapest first: re-pin test 8 to `make_raking_blade`. Restores the only guard on the
  stale-artifact unlink. No baseline needed, no frozen number moves.
* Then: find a repair-COMMITTING procedural fixture. `plinth_mesh::make_overhang_table_with(
  leg, top, z0)` (`crates/plinth-mesh/src/fixtures.rs`) is already parameterised, and one
  `run_support` call on it costs ~2 s, so the parameter space is searchable in minutes. Prove
  the fixture has teeth the way this project demands: show it commits, then show the test
  goes red when the commit path is suppressed.
* Then, and only with the owner's ruling: re-record the four frozen numbers in tests 3, 5, 6
  and 9 against that fixture, with the attribution written down, exactly as `7eb5f30`'s
  eighth re-record does it.
* Test 7 can then be deleted or re-pinned as the judge prefers; it is the redundant one.

## 6. What this is NOT

**Short-list 5** (`runner.rs:7320` / `specialized.rs:2405`, two definitions of "the regular
contact set", `validate_regular` comparing one against the other) is real, but it is not this.
That defect refuses a run with `StaleRegularContacts` or kills it with `UnsatisfiedCoverage`.
Every one of the nine failing runs seals successfully and emits — six of them write a
complete artifact to disk. The validator never fires on these fixtures. `3a6526f` does not
touch either function. **Do not look for this cause when fixing short-list 5, and do not
expect fixing short-list 5 to turn any of these nine green.**

**Theme 3.6** (`coverage_closure.rs:600`, dead-code allows saying the one-winner controller
"lands in the next commit" when it landed) is a mislabelling, not a behaviour. Removing those
`#[allow(dead_code)]` attributes changes no run.

**The two `compile_fail` tests** (`placeholder_cannot_reach_acceptance`,
`support_domain_invariants_cannot_be_fabricated_outside_the_crate`) are a harness artifact of
running test binaries bare, as the task brief already records. They are not in this cluster.

**The 240 s timeout on `support_runner_integration`** is REVIEW.md short-list 12 (uncapped
trial count, a full measurement per trial), a separate live finding. Note that failure 9
inside that binary now runs in **2.32 s** when filtered to itself — its own fixture stopped
doing any work. The binary is slow because of its *other* tests.

## 7. Reproduction

```bash
cargo test -p plinth-pipeline --test a4_production_door      # 1 passed, 2 failed, ~3 s
cargo test -p plinth-pipeline --test first_artifact          # 5 passed, 2 failed, ~27 s
cargo test -p plinth-support  --test a4_transactional_controller   # 4 passed, 4 failed, ~28 s
cargo test -p plinth-support  --test support_runner_integration \
    public_runner_closes_overhang_table_only_after_appending_repair_contacts   # 1 failed, ~2 s
```

The Sphere pair needs `corpus/models/Sphere.stl`, which is gitignored; without it those two
tests fail at `first_artifact.rs:61` with `RequiredCorpusModelMissing` instead, which is a
provisioning failure and not this defect.

`cargo build --workspace --all-targets` is green. No file in the repository was changed by
this diagnosis.

---

*Diagnosed on `feature/remediation-safety-net` at `f3e2258`, 2026-08-26. Six worktree builds
across `1509752..94057f0` plus the `f9c7d07`/`3a6526f` parent-child pair; the bisect worktree
was removed afterwards.*
