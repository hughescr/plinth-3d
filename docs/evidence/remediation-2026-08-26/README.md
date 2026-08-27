# Remediation of 2026-08-26/27 — review, plan, evidence, and what did not survive

Everything here was produced on a scratch filesystem that this project has already lost
once to an OS update. It is committed so it cannot be lost again.

## The deliverables

- `REVIEW.md` — the whole-codebase review. 203 findings, each verified by a cross-family
  adversarial pass. Its own "Method and limits" section is honest about what it did not
  cover; read that before treating any absence as a clean bill of health.
- `PLAN.md` — the sequenced remediation plan the review fed into.
- `FIX-AFTER-RUN.md` — the working list of damage and open items. Section 5 is struck
  through: it recorded two defects as open that were already fixed. Kept struck rather
  than deleted, because a stale "still broken" note costs as much as a stale "fixed" one.

## Measured evidence

- `build-cost.md` — where build and test wall-clock actually goes. The headline correction:
  incremental builds are ~4s and were never the problem; the test suite is.
- `repair-cap-evidence.md` — where the coverage-repair search stops paying. Concludes that
  a fixed trial-count cap is the WRONG SHAPE and refuses to fit a number to three data
  points. Recommends a shape (trials-since-last-commit, or wall-clock) and no value.
- `EVIDENCE-treant-runaway.log` — the same question answered at production scale, by
  accident. An orphaned `plinth prepare` on the treant ran 6.7 hours unsupervised:
  **224 trials, 1 commit, 119 regressions, 9,374 patches still outstanding, no artifact
  written.** More than half of all trials made the result WORSE. This reframes the cap
  question: the search does not plateau on a real model, it wanders. A cap would stop the
  bleeding; it would not explain the 53% regression rate.
- `baseline-per-binary.txt` + `baseline.sh` — the per-binary test harness and its output.
  NOTE the harness's own trap: running a test binary directly with `--test-threads=4`
  breaks the trybuild `compile_fail` tests. They pass under `cargo test`. Two "failures"
  in any output from this script are that artifact, not a defect.

## Diagnosis

- `baseline-red-diagnosis.md` — why nine tests were red since 2026-08-21. One cause, and
  not a regression: commit `3a6526f` fixed the clearance mask, the fixtures stopped needing
  coverage repair, and the tests lost their witness. The commit said so at the time and
  left them "for the judge to rule on". Ruled 2026-08-26: build a synthetic model that
  genuinely needs repair. Done; the cluster is closed.
- `census/` — the six mechanical censuses the review was built on: inventory, dead code,
  duplication, clippy triage, documentation drift, test shape.
- `audit-constants.md` — every outcome-deciding constant, with its derivation status.

## What did not survive, and how to recover it

Eleven of twelve correctness lanes are on branch `wip/correctness-salvage` (`623efec`).
That commit DOES NOT COMPILE and is preserved rather than repaired. The lanes half-applied
changes that crossed each other's files — a struct gained a field its constructors never
learned about, and one lane's work was silently reverted underneath it by a stale write
from a duplicate of itself.

- `patches/LOST-drain-banding-drilling.patch` — a properly reasoned fix for the
  `cavity_id: 1` collapse in the shipped hollow report. Blocked only because the functions
  it calls lived in `drains.rs` edits that were destroyed. Worth reviving.
- `patches/kn-fixture-repairs.patch` — already applied; kept for provenance.
- `HANDOFF-kernel-to-rewire.md` — 458 lines. The next actionable piece: the typed clearance
  API shapes, the exact drop-in for `rewire.rs:1143-1152` and the duplicate epsilon at
  `:1202`, all 13 lossy-shim call sites, and the mutation-teeth table including one
  honestly-declared gap.

## The process lesson, recorded because it cost the most

Twelve mutating agents were partitioned BY FILE and run in ONE shared checkout. That fails
two ways and both happened: file partitioning does not survive changes that legitimately
cross files, and it does not survive duplicate lanes at all. Any future run of this shape
needs one git worktree per lane.
