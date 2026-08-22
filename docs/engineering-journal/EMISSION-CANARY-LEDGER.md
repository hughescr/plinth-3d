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
