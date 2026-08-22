# Emission canary — the values of record

A live ledger, appended to rather than rewritten. It exists because task **#30** records
that the emission-regression canary **never runs in CI**, and until 2026-08-21 it also had
**no value of record anywhere in the tree** — so a run could move the artifact and nobody
could say by how much, or against what.

`Round 25.stl` is the canary: a 3.5 mm plain display base, ~2 s per pass, emitted solid by
the hollow stage's own `BelowThreshold` decision. It is the cheapest model in the corpus
that produces a real support artifact, which is exactly what a canary needs to be.

**This file freezes nothing and gates nothing.** It records what the artifact WAS at a named
commit, so the next movement can be attributed instead of discovered.

---

## Values of record

| recorded | commit | branch | md5 of `Round 25.chitubox` | support records | bytes |
| --- | --- | --- | --- | --- | --- |
| 2026-08-22 | `eb64c83` | `feature/a4-controller` (tip, two-tier demanded gap) | `fe9c716deb0805a3f0074b3e3dfe4bad` | 36 | 354,984 |
| 2026-08-21 | `192f9d6` | `feature/a4-controller` (tip) | `fe9c716deb0805a3f0074b3e3dfe4bad` | 36 | 354,984 |
| 2026-08-21 | `e9e160f` | `feature/a4-controller` (lane base) | `fe9c716deb0805a3f0074b3e3dfe4bad` | 36 | 354,984 |
| 2026-08-21 | `a8feff4` | `develop` | `9709f3cb05b599a68ec77911e3863f3b` | 47 | 377,376 |

**The current value of record is `fe9c716deb0805a3f0074b3e3dfe4bad`, 36 support records,
354,984 bytes.**

MEASURED 2026-08-21 by the consolidation judge of `feature/a4-controller`, and
independently re-measured from the same persisted artifacts when this ledger was written.
The `#71` per-contact disclosure lane moved the canary by **zero bytes**: base and tip are
the same file. The 47 → 36 movement is older branch history.

---

## What the develop → branch movement actually was

MEASURED by reading the records out of both artifacts, by segment type:

| segment type | `develop` `a8feff4` | branch `192f9d6` |
| --- | --- | --- |
| TAPER | 7 | 9 |
| JOINER | 14 | 5 |
| FLARE | 3 | 1 |
| PAD_OR_BRIDGE | 5 | 1 |
| SHAFT | 11 | 13 |
| **TIP** | **7** | **7** |
| total | 47 | 36 |

**All seven `TIP` records are byte-identical between the two artifacts** — same seven seats,
same seven radii, same seven directions, to every digit the dump prints. Not one contact
with the mini moved, and not one scar changed size. What changed is only the structure
between tip and plate: eleven junction and pad records were replaced by more direct
taper-and-shaft runs.

That distinction is the whole reason to keep a canary. "The artifact moved" and "the
supports touch the mini differently" are different claims, and only the second one is a
print-safety claim.

---

## Rules for this ledger

1. **A new row, never an edited row.** A moved canary is a new dated row with its commit,
   its md5, its record count and its byte count. Rows are not corrected in place; a wrong
   row gets a following row that says so.
2. **A movement is recorded with its cause or it is recorded as unexplained.** "Unexplained"
   is an admissible entry. Inventing a cause is not.
3. **The TIP records are the load-bearing half.** Any movement should say whether the TIP
   set moved, because a structure-only movement and a contact movement are not the same
   event.
4. **This ledger is not a gate.** Nothing in the test estate reads it. Wiring the canary
   into CI is task #30's work, and when that lands the frozen value belongs in the test,
   with this file as the history behind it.

---

## How the value was measured

Build the pipeline binary at the commit under test, run
`plinth prepare` over `Round 25.stl` from `minis-to-process/` (sha256-verified), then take
`md5` of the emitted `Round 25.chitubox` and read the record count from the run's own
`SUMMARY` line. The per-segment-type histogram and the TIP dump come from a small
throw-away reader over the artifact's records; it is not production code and is not
committed.

**Check the binary postdates the commit under test before believing any of it.** A stale or
probe-instrumented binary produces a confident wrong answer that `git status` cannot see —
see task #54.

---

## 2026-08-22 — honest-exemption + two-tier re-baseline

New values of record for the four-model run set of `feature/a4-controller` at `eb64c83`,
after §8 steps 1–2 of `designs/honest-exemption-2026-08-22.md` landed the two-tier demanded
gap (`STANDOFF_MM` 0.600 mm for placement, `FINAL_CLEARANCE_GAP_MM` 0.300 mm for the final,
refusing pass). Craig authorized movement in BOTH directions for this lane; every row below
is re-derived through its own producer rather than carried over.

MEASURED 2026-08-22, `gate` profile, `saturn4-ultra`, `--no-cache`, one machine, back to
back. Binary `sha256 f8274722864888e7ff26213b74fe590e0eecc8613dd22b032c6652cd653164b3`,
built from `eb64c83` plus this lane's test-and-note commits (no production behaviour in
them).

| model | md5 of `<model>.chitubox` | support records | bytes | movement |
| --- | --- | --- | --- | --- |
| `Round 25` | `fe9c716deb0805a3f0074b3e3dfe4bad` | 36 | 354,984 | **none — byte-identical to the lane base** |
| `Deinonychus` | `f2a471165be303ab360530e30fb781a5` | 1138 | 10,379,904 | **none — byte-identical** |
| `zombie-3-a` | `518b346faec0d71b7cdaf05ebeacb89f` | 210 | 3,719,400 | **none — byte-identical** |
| `Giant Bat` | *no artifact* | — | — | **MOVED — see below** |

Three of the four are byte-identical to the lane base, so the TIP set — the load-bearing
half of this ledger under rule 3 — did not move on any model that emits. One model moved.

`zombie-3-a`'s coverage-repair ledger is identical field for field as well, not only its
artifact bytes: `trials=92 commits=3 remaining=433 diversity_demotions=2185
of_which_tried=91 [no_improvement=89]`. Wall times are evidence and never a bound, so they
are not compared.

### The Giant Bat's movement, with its control

**FROM** (lane base, and reproduced at this tip by reverting one line): REFUSED at
`Structure`. The hollow shell fails `TerminalEvidenceMismatch { terminal: 298 }`, the
substituted oriented solid fails `{ terminal: 187 }`, exit 5, **75.1 s**.

**TO** (this tip): passes `Structure`, then `Lowering`, then `CoverageMeasurement`, and
enters coverage repair at `remaining=6221`. It did not terminate. Stopped by the operator
at **915.4 s** standing at `trials=42 commits=0 remaining=6179 [no_improvement=42]`.

**CONTROL** — the cause is the demand and nothing else. The control binary
(`sha256 1915dc2f6f1f38b0582f9dc0635051bd75383e2d3c64ad1e1c5a37acd96c6ba9`) is the SAME
tree with exactly one line changed: `BraceConfig::contact_policy`'s `ClearanceStage::Final`
arm reading `self.standoff_mm` instead of `self.final_clearance_gap_mm`. It reproduces the
refusal exactly, to the same two terminal numbers and the same exit code.

**RATE, MEASURED**: one ranking candidate consumed per trial, **21.79 s per trial, zero
commits** across all 42 trials. **INFERRED** from that rate: exhausting the 6,221-candidate
ranking would take **≈ 37.4 hours**. Ruling R10 removed the failed-trial cap, so nothing
shorter bounds it.

Cause: `FINAL_CLEARANCE_GAP_MM` = 0.300 mm legalizes arms the single pre-ruling demand of
0.600 mm refused, so the model now gets past `Structure` instead of dying there. That
direction is authorized. Where it lands is not a result this lane produced deliberately: a
75-second honest refusal became a 37-hour walk that commits nothing, inside the
coverage-repair subsystem whose controller tests are 4-of-8 known red (`#39`). Recorded as
what it is. Task `#47` already owns the Giant Bat on the coverage-repair path and `#39` owns
the 0-commit walk; neither is this lane's to fix, and the demand must NOT be tuned to make
the Bat refuse again.

**Unexplained: nothing.** Every row above has its cause, and the one that moved has a
control experiment rather than an argument.
