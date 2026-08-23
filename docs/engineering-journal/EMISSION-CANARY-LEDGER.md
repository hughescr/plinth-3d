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

---

## 2026-08-23 — honest-exemption + two-tier re-baseline, §8 steps 3–7

New values of record for `feature/a4-controller` at `64edde4`, after §8 steps 3–6 of
`designs/honest-exemption-2026-08-22.md` landed the frozen seat zone and full-axis grading
(`0a72bc8`, `edcadd6`) and `64edde4` priced the round-5 challenge findings. Craig authorized
movement in BOTH directions for this lane. Every row is re-derived through its own producer.

MEASURED 2026-08-23 by the merge judge, `gate` profile, `saturn4-ultra`, `--no-cache`, one
machine, baseline and tip back to back, **one process at a time** so the wall times compare.
Models taken from `minis-to-process/`, sha256-verified.

### Binary provenance, both axes

| | commit | sha256 | seat-zone wiring in the LINKED binary |
| --- | --- | --- | --- |
| baseline | `56a8782` | `7d508ed689c49bbdaf7f424c05f04e249ab2471aee356129c2b12810904b6569` | **absent** — zero `seat_zone` symbols, no `faces_near_segments_batch` |
| tip | `64edde4` | `eef90622faa0e4282a88f218d7b1e48bf6210d94c9b946242e20eb89a3bf5015` | **present** — `freeze_seat_zones`, `SeatZoneRequest`, `faces_near_segments_batch` |

Both builds started at `2026-08-23T11:02:38Z`, after `64edde4` was authored
(`2026-08-23T10:58:54Z`). The baseline's *absence* of the wiring is the negative control: it is
§10's parked state, not a differently-configured tip.

**The baseline reproduces this ledger exactly**, so the comparison is against a verified
baseline rather than an assumed one — `Round 25` `fe9c716deb0805a3f0074b3e3dfe4bad` / 36 / 354,984;
`zombie-3-a` `518b346faec0d71b7cdaf05ebeacb89f` / 210 / 3,719,400; `Deinonychus`
`f2a471165be303ab360530e30fb781a5` / 1138 / 10,379,904; and `zombie-3-a`'s repair line
`trials=92 commits=3 remaining=433 diversity_demotions=2185 of_which_tried=91 [no_improvement=89]`
field for field.

### The new values of record

| model | baseline `56a8782` | tip `64edde4` | movement |
| --- | --- | --- | --- |
| `Round 25` | `fe9c716deb0805a3f0074b3e3dfe4bad`, 36 rec, 354,984 B, 1.10 s | `ee76f98a0e90ae59d0ef2762097ea7bb`, **41 rec**, 364,416 B, 1.21 s | MOVED, +5 records |
| `zombie-3-a` | `518b346faec0d71b7cdaf05ebeacb89f`, 210 rec, 3,719,400 B, 672.6 s | `c13ca5288e64203b134b1ea4887669f6`, **215 rec**, 3,750,864 B, **278.1 s** | MOVED, +5 records, 2.4× faster |
| `Tavey` | `7d724c7d9bba175fa41032f36e9f4908`, 664 rec, 8,260,224 B, 283.8 s | `01886cbd4a37bff1792b1d17600ada0a`, **697 rec**, 8,254,536 B, 286.8 s | MOVED, +33 records |
| `Crawler Zombie` | `eebefa541af2ccbc5fcfa786abbc1d48`, 373 rec, 3,743,232 B, 671.6 s | `ca11d626f4d85091f562120eb65eb441`, **376 rec**, 3,758,424 B, 504.6 s | MOVED, +3 records |
| `Skulker Zombie` | *no artifact* — still walking coverage repair when the operator stopped it at 1200 s | `c05d13cec10658e071e8ab7f6472260e`, **226 rec**, 7,448,640 B, **67.7 s** | **MOVED — now emits** |
| `Deinonychus` | `f2a471165be303ab360530e30fb781a5`, 1138 rec, 10,379,904 B, 30.1 s | *no artifact* — **exit 5** | **MOVED — REGRESSION** |
| `Rotting Warhorse` | *no artifact* — solid reserve carried it into coverage repair; stopped at 1200 s | *no artifact* — **exit 5**, 89.0 s | **MOVED — REGRESSION** |
| `Warhorse` | *no artifact* — stopped at 1200 s | *no artifact* — stopped at 1200 s | unchanged disposition |
| `Giant Bat` | *no artifact* — stopped at 1200 s | *no artifact* — stopped at 1200 s | unchanged disposition |

**Nothing stayed byte-identical, and that is the change working.** The frozen-zone rule changes
the graded quantity on every contact-declaring member, so every model with seated arms moves.
Under rule 3 the load-bearing question is whether the TIP set moved: it did, and record counts
rose on all five emitting models, which is arms becoming legal — the authorized direction.

### `Skulker Zombie` — the clearest win

The baseline enters coverage repair at `remaining=1424` and after 36 trials and one commit still
stands at `remaining=1388`; the operator stopped it at 1200 s with no artifact. The tip's initial
plan needs **no repair at all** — `trials=0 commits=0 remaining=0` — and the model emits in
**67.7 s**. Arms the retired skip-length refused are legal, so coverage closes on the first pass.

### `Deinonychus` and `Rotting Warhorse` — two models lost to the Routing/Brace seal seam

`Deinonychus`, tip, verbatim:

```
model Deinonychus failed its sealed support run: sealed support run failed during Structure:
TerminalEvidenceMismatch { terminal: 113 } [faces=117198 max_grid_bytes=67108864
knn_max_work=240021504]
```

`Rotting Warhorse`, tip, **both attempts**: the drilled hollow shell (275,884 faces) fails
`TerminalEvidenceMismatch { terminal: 177 }`, the substituted oriented solid (260,906 faces)
fails `{ terminal: 156 }`. At the baseline the shell also failed at Structure, but the solid
reserve carried the model through; at the tip the reserve fails too, so there is nothing left.

**Neither is a clearance refusal.** `TerminalEvidenceMismatch` is Structure's cross-check between
the CCO transcript sealed during Routing and the forest's terminal ledger read after Brace
(`runner.rs`, `finalized_unrouted_terminals`). It fires on the arm where the transcript says
`Routed` — which admits only `Consumed` — and the ledger reads `Quarantined`.

**Mechanism, traced in source.** The brace fixed point refuses contact-declaring members on the
FINAL clearance pass and calls `quarantine_subtrees(&negative.contact_refusals)` (`brace.rs`).
`Forest::quarantine` retires the key, and `terminal_ledger` is *derived* from live topology
rather than stored, so that terminal retroactively re-reads as `Quarantined`. Nothing between the
seal and the Structure read re-seals or reconciles. The honest exemption does not create this
seam — it changes WHICH arms the final pass refuses, and so which terminals fall into it.

**This is the seam `designs/seal-semantics-2026-08-22.md` already root-causes and designs a fix
for (its Option 2), and that fix is UNBUILT.** The Giant Bat reached the same error class from
the same seam earlier (`{ terminal: 298 }` / `{ terminal: 187 }`, recorded above). What is new is
that two models which previously produced or progressed no longer do.

**Ownership: none.** `TerminalEvidenceMismatch` has no follow-up row and no task number anywhere
in the tree. `#47` owns the Bat's `ClearanceViolation` on the coverage-repair path and `#39` owns
the 0-commit walk; neither owns a failure AT Structure. The seam needs its own row.

### `Warhorse` — the two-tier ruling's first win survives the honest mask

No previous row exists for this model, so this one is new.

| | baseline `56a8782` | tip `64edde4` |
| --- | --- | --- |
| sealed support runs attempted | **1** | **1** |
| hollow shell clears Brace on the first attempt | **yes** | **yes** |
| oriented-solid substitution | never invoked | never invoked |
| support pass to `CoverageMeasurement` | 20,691 ms | **19,278 ms** |
| coverage repair entered at | `remaining=6356` | `remaining=6271` |
| state when the operator stopped it at 1200 s | `trials=51 commits=2 remaining=6235` | `trials=56 commits=0 remaining=6215` |

The shell cleared Brace on its first attempt at both ends and the oriented-solid reserve was
never reached, so the honest mask did not take back the two-tier ruling's first win. What neither
end does is finish: both walk the coverage-repair loop and both were stopped by the operator, not
by the engine. That walk is `#39`'s lane. The tip's support pass is 6.8 % faster and it enters
repair with less remaining work, but it committed nothing in 56 trials where the baseline
committed twice in 51. Recorded as measured; this lane does not explain it and does not tune it.

### `Giant Bat` — still the uncapped walk, entered from a better position

| | baseline `56a8782` | tip `64edde4` |
| --- | --- | --- |
| disposition | passes `Structure`, `Lowering`, `CoverageMeasurement` | **same** — no refusal |
| coverage repair entered at | `remaining=6221` | **`remaining=6075`** |
| state when the operator stopped it at 1200 s | `trials=57 commits=0 remaining=6164` | `trials=48 commits=0 remaining=6027` `[stage_refusal=1]` |
| candidates consumed per trial | exactly 1 | exactly 1 |
| commits | **0** | **0** |
| MEASURED rate | 19.89 s per trial | 23.89 s per trial |
| INFERRED time to exhaust the ranking | ≈ 34.4 h | ≈ 40.3 h |

The `remaining=6221` entry count reproduces §10.1 exactly. **The honest exemption did not make
the Bat refuse and did not make it emit.** It enters the same walk with 146 fewer units of
uncovered work and consumes the ranking 20 % more slowly per trial, still committing nothing.
Both runs were **stopped by the operator at 1200 s** — ruling R10 removed the failed-trial cap
and nothing shorter bounds the walk, so a run that is allowed to finish is a day and a half of
machine time. The demand must NOT be tuned to make the Bat refuse again.

`[stage_refusal=1]` in the tip's histogram is one rolled-back repair trial whose stage refused.
It is the only channel by which a final-pass clearance violation count reaches disk, and it is
lost when the run is stopped before it seals — see the residue note below.

### The honest residue is NOT MEASURABLE from this tree

The parked seal-semantics decision waits on the count of final-pass refusals whose TRUE gap is
under 0.300 mm. **That number cannot be read out of any build of this tree**, and this is
recorded here because it is a gap in the evidence, not a result.

`brace::exact_clearance_rows` computes a real per-member mm clearance.
`exact_clearance_negatives` tests only its sign and **discards the value**. The negatives split
into `pipeline_violations` (no model contact — becomes `BraceError::ClearanceViolation(N)`) and
`contact_refusals` (declares a contact — quarantined, then hashed into a plan digest and
dropped). Passes 1 and 2 of the fixed point drop their negatives entirely. The census feature
records the batch's INSETS, which are now zeros, and never the resulting clearance.

Two consequences worth stating plainly:

1. **The residue is a property of the SEARCH, not of the artifact.** Convergence requires zero
   negatives, and every negative row is either fatal or quarantined and removed, so a shipped
   forest is ≥ 0 everywhere by construction. "How many arms were refused on the way" is the
   question, and nothing counts it.
2. **Every final-pass refusal at a demand of 0.300 mm has a true gap below 0.300 mm** by
   arithmetic — `clearance = true_gap − 0.300`. So the residue COUNT is exactly the refusal
   count, and only the magnitudes would need more than a counter.

**Unexplained: nothing.** Every row above has its cause; the two regressions have a mechanism
traced in source rather than an argument, and the one number this lane could not produce is
recorded as unmeasurable with the reason.

---

## 2026-08-23 — seal-semantics Stage A, and one AUTHORIZED sealed-fingerprint move

Branch `feature/a4-controller`. Design authority
`docs/engineering-journal/designs/seal-semantics-2026-08-22.md`; Craig authorized all seven
§5 items on 2026-08-23.

### The move, and it is deliberate

Authorization item 5 keeps `candidates_feasible` meaning exactly what it means today — *"how
many arms passed feasibility"*, the field refutation §F.1 reads to make its "arms existed,
clearance refused them" argument — and adds a **second counter** for ruled-out arms,
`FinalizedCcoTranscriptRow::candidates_ruled_out`. The cheaper alternative was to fold
ruled-out arms into the existing counter, which keeps the fingerprint still for models that
rule nothing out and leaves one sealed number with two meanings. Craig ruled for the honest
counter and accepted the price.

The counter is folded into the sealed CCO transcript **unconditionally** — not "only when
non-zero", because a conditional fold is exactly the shape that keeps the fingerprint still
today and makes the re-freeze unrepeatable later. So the sealed fingerprint moves **once, for
every model**, at the commit that introduces it.

| | before | after |
| --- | --- | --- |
| `make_toed_slab` `snapshot_digest` | `4_190_462_360_230_124_963` | **`12_136_443_226_163_403_608`** |
| routed identities / final live members | 64 / 170 | **64 / 170 — unchanged** |
| `coverage_repair_rounds()` | 0 | **0 — unchanged** |

**MEASURED at the re-freeze, and this is the whole point of recording it.** Diffing the
frozen `crates/plinth-verify/tests/coverage_gate_toed_slab.json` before and after: the
11-tuple resource plan, all 9 islands, all 9 patches and all 7 defects are **byte-identical**.
The only field that differs anywhere in that artifact is `snapshot_digest`. The counter's
value is **zero on every path today** — nothing rules an arm out until Stage B's steering
channel lands — so this is a change to the SHAPE of the digest stream and to nothing else.

Re-frozen in two places, both with the reason written beside them:
`crates/plinth-verify/tests/coverage_gate_toed_slab.json` and the two goldens in
`crates/plinth-support/tests/support_runner_integration.rs`.

`Round 25.chitubox` itself was **not** re-measured at this commit. The canary's own value of
record therefore still stands at `fe9c716deb0805a3f0074b3e3dfe4bad`, 36 support records,
354,984 bytes, and a run at this tip should be expected to move it for the same reason the
toed-slab digest moved. Recorded as **not measured**, not as unchanged.

### The honest residue is NOW MEASURABLE — the 2026-08-22 note below is superseded

The section "The honest residue is NOT MEASURABLE from this tree" recorded a gap in the
evidence: `brace::exact_clearance_rows` computes a real per-member clearance in millimetres
and `exact_clearance_negatives` tested only its sign and discarded the value, so a run could
say "one arm was refused" and never say by how much.

**That gap is closed.** Craig's refusal-accounting rider (2026-08-23) adds
`FinalPassRefusalAccounting`: the final bracing pass keeps the true gap of every negative row,
in millimetres, with the two radii the resize assigned, and streams them through
`emit_run_line` as they are measured:

```
brace: final-pass refusals pass=3 refused_arms=1 pipeline_violations=0 gaps: slot1g0=-0.800000mm@r0.500000/0.500000mm
```

Two things the earlier note said remain true and are worth keeping: the residue is a property
of the SEARCH rather than of the artifact, and — because the demanded gap is 0.300 mm and
`clearance = true_gap − 0.300` — every final-pass refusal already has a true gap below
0.300 mm by arithmetic, so the COUNT was never the missing half. The MAGNITUDES were, and they
are now recorded. What is still **not measured** is the corpus-wide distribution of them:
that needs a corpus run at this tip, which this entry does not claim to have done.

The accounting is disclosure only. It is deliberately **not** folded into
`brace_fixed_point_plan_digest` — every row is a pure function of the forest and surface that
digest already covers, and this lane is authorized to move exactly one fingerprint.

**Unexplained: nothing.**

### `Deinonychus` — the seam now REFUSES HONESTLY, and the residue has its first number

MEASURED 2026-08-23 at `3311374` (`feature/a4-controller`), binary
`target/gate/plinth` sha256 `c7d15bd3b7d112722e237ac252279d0bcbe8e7c870300ac4ec9c787836270913`,
built 13:30 — after both fix commits (`80671e4` 13:01, `3311374` 13:30) — from a clean tree.
Input `Deinonychus.stl` sha256 `d1ac66b1…e72e72b9`. Run under a 25-minute HARNESS wall-clock
timeout, which **did not fire**: the run finished in 22 s.

| | before Stage A (2026-08-23 morning) | at `3311374` |
| --- | --- | --- |
| stage it dies at | **Structure** | **Brace** |
| error | `TerminalEvidenceMismatch { terminal: 113 }` | `SealedRoutedTerminalCondemned` |
| terminal named | 113 | **113 — the same one** |
| its arm | not reported | `slot 401, generation 0` |
| true gap | **not measurable** | **−0.050000 mm** against the 0.300 mm final demand |
| radii the resize assigned | not reported | 0.372136 / 0.372136 mm |
| cascade victims | not reported | **none** |
| emitted | 0 | 0 |

Verbatim, streamed while the run was happening:

```
brace: final-pass refusals pass=3 refused_arms=1 pipeline_violations=0 gaps: slot401g0=-0.050000mm@r0.372136/0.372136mm
brace: sealed terminals disturbed=1 measured refusals=[slot401g0=-0.050000mm@r0.372136/0.372136mm] class=[terminal 113 sealed=Routed stub=slot113g0 own_arms=slot401g0 cascade_carriers=none]
```

**What this is and is not.** It is NOT the Deinonychus emitting — Stage A does not retry, and
the design says so (§4 Stage A, "what it does not buy"). It is the model reaching a **named
physical refusal instead of an accounting-integrity error that reads as a broken engine**, with
the terminal, the arm, the millimetres and the radii on the line. Stage B is what gives that
terminal a second arm to try.

**MEASURED / DERIVED, kept apart.** MEASURED: the clearance row reads −0.050000 mm. DERIVED,
by `clearance = true_gap − 0.300` (the final-stage demanded gap): the arm's true gap to the
model is **0.250 mm**. That is the sub-0.300 mm residue the seal decision originally waited on,
now a number rather than a sign — one arm, on one model. The corpus-wide count and distribution
are **not measured** here; that is Stage A's corpus run.

The mismatch class is **exactly one terminal with zero cascade victims**, which reproduces the
S-1 pre-measurement of 2026-08-22 (Deinonychus t=113 of 202, 3/3 runs) on production code
rather than on diagnostic instrumentation.

### CORRECTION to the `Deinonychus` row above — I read a SENTINEL as a MEASUREMENT

Per rule 1 of this ledger, a wrong row gets a following row that says so. The
`Deinonychus` row above is wrong in its headline number **and in its mechanism**, and this
row supersedes it. What is right in it: the stage moved from Structure to Brace, the error
moved from `TerminalEvidenceMismatch` to `SealedRoutedTerminalCondemned`, terminal 113 and
arm `slot 401 generation 0` are correctly named, and the class is one terminal with zero
cascade victims.

**What was wrong.** That row read the arm's clearance as `−0.050000 mm` and DERIVED a true
gap of 0.250 mm, calling it "the sub-0.300 mm residue the seal decision waited on". The
arithmetic `clearance = true_gap − 0.300` is right, and I checked it against
`geom_kernel::seg_clear_one_excluding` — `clr = best_distance − (r_max + demanded_gap)`. What
I did not check was **which of two paths wrote that number**, and it was the wrong one.

`brace::exact_clearance_rows` writes one signed number per member from two screens:

1. the **distance** screen, whose negative value IS a shortfall in millimetres; and
2. the **axis-transit** screen, which takes a row the distance test found **legal**
   (`axis_illegal_rows` filters to `clearance >= 0.0`) and stamps
   `min(clearance, −min_illegal_len_mm)` — **exactly `−0.050 mm`** — so a single sign can
   carry the verdict.

`−0.050000` is `−min_illegal_len_mm` to the digit. It was a sentinel. A sentinel that happens
to look like a plausible sub-0.300 mm residue is the worst possible shape for this mistake,
and it is the shape I fell into.

**The corrected measurement**, from a binary rebuilt at the fix and run the same way:

```
brace: final-pass refusals pass=3 refused_arms=1 pipeline_violations=0 distance=0 axis_transit=1 rows: slot401g0=axis-transit(distance_clearance=+0.280044mm)@r0.372136/0.372136mm
```

| | wrongly recorded above | **corrected** |
| --- | --- | --- |
| which screen refused the arm | distance | **axis transit** |
| distance clearance | −0.050 mm (a sentinel) | **+0.280044 mm — it PASSED** |
| DERIVED true surface gap | 0.250 mm | **0.580044 mm** (`+0.280044 + 0.300`) |
| what the refusal is about | a clearance shortfall | **the arm's AXIS passing through material away from its seat** |

So terminal 113's arm is **not** a near-miss on clearance. It clears the demanded gap by
0.280 mm and is condemned because its axis transits the model — which is task **#52**'s class
(*stub axis re-enters the model on edge-on faces*), not a clearance-margin residue at all.

**The residue count stands at ZERO measured so far.** The one model examined at this tip
produced no distance refusal, so this lane has still not observed a single sub-0.300 mm
clearance residue. That is a smaller claim than the row above made and it is the true one.

**The production fix that makes this impossible to repeat.** `RefusalGapRow` now carries
`axis_transit_illegal` and `distance_clearance_mm` — the distance screen's own value **before**
any transit stamp — and the streamed line counts `distance=` and `axis_transit=` separately and
renders a transit row as `axis-transit(distance_clearance=+…)` rather than as a gap. A
sabotage-shaped unit test asserts the sentinel is never printed as a magnitude.

**Unexplained: nothing. Superseded: the `Deinonychus` row immediately above.**
