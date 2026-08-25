# Emission canary — the values of record

A live ledger, appended to rather than rewritten. It exists because task #30 records that the
emission-regression canary never runs in CI, so until this ledger existed a run could move the
emitted artifact and nobody could say by how much, or against what. **This file freezes
nothing and gates nothing** — it records what the artifact WAS at a named commit, so the next
movement can be attributed instead of discovered. Wiring the canary into an actual CI gate is
task #30's work; when that lands, the frozen value belongs in the test, with this file as the
history behind it.

## The instrument

`Round 25.stl` is the canary: a 3.5 mm plain display base, emitted **solid** by the hollow
stage's own `BelowThreshold` decision, ~2 s per pass. It is the cheapest model in the corpus
that produces a real support artifact.

**How the value is measured.** Build the pipeline binary at the commit under test, run
`plinth prepare` over `Round 25.stl` from `minis-to-process/` (sha256-verified), take the md5
of the emitted `Round 25.chitubox`, and read the record count from the run's own `SUMMARY`
line. **Check the binary postdates the commit under test before believing any of it** — a
stale or probe-instrumented binary produces a confident wrong answer that `git status` cannot
see (task #54).

**Rules for this ledger:**

1. A new row, never an edited row. A moved canary gets a new dated row with its commit, md5,
   record count, and byte count.
2. A movement is recorded with its cause, or recorded as unexplained. Inventing a cause is not
   admissible.
3. The **TIP** segment-type records are the load-bearing half — any movement should say
   whether the TIP set moved, because a structure-only movement (junctions and pads
   rearranged) and a contact movement (the mini touched differently) are not the same event.

## Current value of record

**`Round 25.chitubox`: md5 `ee76f98a0e90ae59d0ef2762097ea7bb`, 41 support records, 364,416
bytes.** MEASURED 2026-08-23 (seal-semantics Stage D, commit `5f5c2d6`, `feature/a4-controller`),
`cmp`-clean back through Stage B and Stage C. Unchanged through the 2026-08-25 `f1-refreeze`
merge (task #77) into `feature/a4-controller`, which performed no corpus run.

## Latest census — 2026-08-24, seal-semantics class, 33 of 45 models measured

**Not a completed 45-model census.** 12 models unmeasured: Giant Bat, Tavey Badger, Tavey
Base, Warhorse, and the procedural fixtures (`Cylinder`, `make_box`, `make_overhang_table`,
`make_raking_blade`, `make_thin_fin`, `make_toed_slab`, `Sphere`, `Weeping Treant v1.2`). No
missing row is inferred clean or unaffected — this is the seal-semantics class specifically
(the `TerminalEvidenceMismatch` / sealed-`Routed`-terminal-condemned seam this instrument was
built to watch), not the full corpus emission census in
[`docs/status.md`](../status.md).

| reading | value |
| --- | --- |
| seal class fired | 5 / 33 measured (at least 5 / 45) |
| observed class size | all observed classes size 1 (one terminal, zero cascade victims) |
| cascade carriers | 0 observed |
| sub-0.300 mm true-gap residue | 0 in measured evidence; 45-model result unknown |
| printed artifacts, byte-identical to the previous stage | 21 |

| model | baseline | tip | verdict | cause |
| --- | --- | --- | --- | --- |
| Deinonychus | ERROR exit 5 at Structure, `TerminalEvidenceMismatch{113}`, 24 s, no artifact | EMITTED, 69 s, 1,149 records, 10,390,488 B | MOVED — no artifact to emitted artifact | terminal 113 rescued in 6 passes, 1 completed confirmation round, 2 arms measured |
| Rotting Warhorse | ERROR exit 5 at Structure, 87 s, no artifact | HARNESS CEILING, loud 486 s run, no artifact; seam closed before coverage repair | endpoint/stage MOVED, no artifact at either end | terminal 177 legalized in 6 passes, 1 completed confirmation round, 2 arms measured; the ceiling is not an emission verdict |
| zombie-4-a | ERROR exit 5 at Structure, `TerminalEvidenceMismatch{23}`, 106 s, no artifact | 25-minute HARNESS CEILING, last state `entering Brace elapsed=1453197ms`, no artifact | endpoint/stage MOVED, no artifact at either end | terminal 23, 33 abandoned passes, 2 rounds started, 11 of 22 round arms observed before the ceiling |

Commit range for the census: `5347901..fe91555`, `feature/a4-controller`.

## 2026-08-25 — f1-refreeze merge (task #77) and the crevice-acceptance ruling

**No corpus run was performed in this merge.** `feature/f1-refreeze @ 855a1b8` merged into
`feature/a4-controller` (`94057f0..7e11319`) with no conflicts; gates after merge:
plinth-support lib 653/653, `retired_formula_guard` 13/13, clippy 0 warnings, `cargo fmt`
unchanged at 104 hunks. The merge landed Craig's 2026-08-25 crevice-acceptance ruling — see
[`docs/physics.md`](../physics.md) §6 for the ruling itself and its consequences. No emission
verdict is claimed for any model at this commit; the last measured emission evidence for the
seal-semantics class remains the 2026-08-24 census above, and the last measured evidence for
the footprint-search closure (Rule Z's rejection) is `.git/rule-z-2026-08-25/`.

## Gap in the evidence, still open

The count of final-pass clearance refusals whose true gap is under 0.300 mm is a real,
computed-but-discarded quantity in most of this ledger's history (`brace::exact_clearance_rows`
computes it; `exact_clearance_negatives` tested only the sign until Craig's 2026-08-23
refusal-accounting rider added `FinalPassRefusalAccounting`, which streams the magnitude). One
model examined under that instrument (Deinonychus, terminal 113) produced **zero** distance
refusals — its condemnation was an axis-transit refusal (task #52's class), not a clearance
shortfall. The corpus-wide distribution of true-gap magnitudes at refusal is still not
measured; that needs a corpus run at the current tip.
