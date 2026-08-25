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

### The canary, re-measured — and this lane moved it by ZERO bytes

| recorded | commit | branch | md5 of `Round 25.chitubox` | support records | bytes |
| --- | --- | --- | --- | --- | --- |
| 2026-08-23 | `466c4c4` | `feature/a4-controller` (seal-semantics Stage A tip) | `ee76f98a0e90ae59d0ef2762097ea7bb` | 41 | 364,416 |
| 2026-08-23 | `5347901` | `feature/a4-controller` (this lane's BASE) | `ee76f98a0e90ae59d0ef2762097ea7bb` | 41 | 364,416 |

**The new value of record is `ee76f98a0e90ae59d0ef2762097ea7bb`, 41 support records, 364,416
bytes**, superseding `fe9c716deb0805a3f0074b3e3dfe4bad` / 36 / 354,984 from `eb64c83`.

**The movement is attributed, and it is NOT this lane's.** Both rows above were produced by
building `plinth` at the named commit and running the same `Round 25.stl` through the same
`saturn4-ultra` profile. The two artifacts are **byte-identical** — `cmp` returns clean, not
merely the same md5. So the whole `36 → 41` record / `354,984 → 364,416` byte movement
happened between `eb64c83` and `5347901`, in commits this lane did not write, and the
seal-semantics work adds **zero bytes** on top of its own base.

That is the strongest form of the design's blast-radius claim available on this model: not
"no digest is expected to move" asserted, but the emitted artifact measured at both ends of
the lane and found identical. Round 25 never trips Stage A — its run emits no
`brace: sealed terminals` and no `brace: final-pass refusals` line at all — so bit-identity is
what Stage A's carve-out predicts for it, and it is now measured rather than predicted.

**What this row does NOT establish.** One model is not the corpus. Bit-identity across all 45
models, and the S-2 class count Stage B waits on, are **not measured** — see the open item
below.

### Still open after this lane — the Stage A corpus run

Craig's authorization item 2 is "prove bit-identity on the corpus, and count the class across
all 45 models **before building anything else**", and the design repeats that the blast-radius
claim "must be **proved** by a corpus run, not asserted". **That run has not happened.** What
exists now is the instrument it needs: every disturbed sealed terminal is named in one refusal,
split into own-chosen arms and cascade carriers, and every final-pass refusal is classified as
a distance shortfall or an axis-transit sentinel with its millimetres. The two numbers the
corpus run must bring back are how many models hit an accounting-disturbing brace refusal, and
how many terminals are disturbed in one sweep split by own-arm versus cascade — the second is
what says whether the reachable T6 is a corner or a common outcome.

---

## 2026-08-23 — Stage C (the run-invariant anchor pool): zero bytes, on three models

| recorded | commit | branch | md5 of `Round 25.chitubox` | support records | bytes |
| --- | --- | --- | --- | --- | --- |
| 2026-08-23 | `ab81ff4` | `feature/a4-controller` (seal-semantics Stage C tip) | `ee76f98a0e90ae59d0ef2762097ea7bb` | 41 | 364,416 |

**The value of record does not move.** `cmp` returns clean against the `466c4c4` artifact —
byte-identical, not merely the same md5. Stage C hoists `SurfaceAnchorPool::from_geometry`
onto the run context so the Routing and Rewire stages stop rebuilding rows that are a
function of two inputs neither of which moves during a run. It changes no verdict, so the
canary is expected to be identical and now is measured identical.

### The two models that exercise what the canary cannot

Round 25 runs one attempt and never enters the coverage-repair trial loop, so it cannot show
the reuse path at all. Two models were run at both ends — `plinth` built at `eb29ead` and at
`ab81ff4`, from clean trees, same inputs, same `saturn4-ultra` profile.

| model | pre `eb29ead` | post `ab81ff4` | verdict |
| --- | --- | --- | --- |
| `zombie-3-a.chitubox` | 3,750,864 bytes, 215 records | 3,750,864 bytes, 215 records | **byte-identical**, md5 `c13ca5288e64203b134b1ea4887669f6` |
| `zombie-3-a.coverage.repair.json` | — | — | four wall-clock fields differ, every decision and count identical |
| Deinonychus | refuses at Brace, terminal 113 | refuses at Brace, terminal 113 | both `brace:` lines byte-identical |

The four fields that differ in the repair sidecar are `wall_time_total_ms`,
`wall_time_per_trial_min_ms`, `wall_time_per_trial_median_ms` and
`wall_time_per_trial_max_ms`. They record how long the run took, so they cannot be identical
between two runs and their movement is not evidence of anything. The repair trajectory
itself is identical on both sides: `trials=35 commits=3 remaining=485
diversity_demotions=2185 of_which_tried=34 no_improvement=32`.

### What the hoist is worth, and why the design memo over-priced it

MEASURED on `zombie-3-a` with a throwaway instrument (built, run, reverted — no production
diagnostic was added), 69,552 pool rows:

| quantity | value |
| --- | --- |
| pool builds before Stage C | **72** (36 attempts x 2 sites) |
| pool builds after Stage C | **1** |
| cost of one REPEAT build | **3.55 ms** median, n=66, min 3.535 ms, max 3.840 ms |
| of which: the content digest | 3.37 ms (95 %) |
| of which: the row gather | 0.16 ms (4 %) |
| total recovered | **254 ms of a 262 s run — 0.10 %** |

On the Deinonychus (115,349 rows, one attempt) the single avoided build is 5.87 ms of a
7.3 s support run: 0.08 %.

**Design memo §2.4 item 1 prices this work as "a 494,617-row build per trial, paid twice",
and that is not what repeats.** The expensive part of a build is
`NativeSurface::exterior_face_mask` — MEASURED 354 ms on `zombie-3-a`, 322 ms on the
Deinonychus — and it sits behind a `OnceLock` in `geom_kernel` (`queries.rs:3093`). It is
paid once per surface, by whichever caller asks for it first, and it was never being repaid
per build. What actually repeated was the row gather and the pool digest. The digest is not
even removed by the hoist: both production sites still run `validate_against`, which hashes
the same rows again — 3.37 ms on `zombie-3-a`, 5.58 ms on the Deinonychus, at every stage.

So the honest sentence has two halves and they travel together: **71 of 72 builds are gone,
and a build is 3.6 ms of which 95 % is a hash.**

**What this means for the staging argument, which is a disclosure and not a decision.**
§4 lands C before B because "Stage C is the only lever on Stage B's named cost risk". The
lever is real and it is 0.1 % of a run. Whoever sizes Stage B's extra confirmation passes
must price them against an unchanged pass cost, not against a pass Stage C made cheap.

**Unexplained: nothing. Superseded: nothing.**

---

## 2026-08-23 — Stage B (the legalization retry): zero bytes moved, and two blocked models EMIT

**Design authority:** `docs/engineering-journal/designs/seal-semantics-2026-08-22.md` §1.3 and
§4 Stage B, authorized by Craig 2026-08-23. Lane tip `ab0c3d7`; the model runs below were
produced by a `target/gate/plinth` that `cmp` proves byte-identical to the binary rebuilt at
that commit from a clean tree.

### The canary does not move, and this time that is a claim about a NEW mechanism

| recorded | commit | branch | md5 of `Round 25.chitubox` | support records | bytes |
| --- | --- | --- | --- | --- | --- |
| 2026-08-23 | `ab0c3d7` | `feature/a4-controller` (Stage B tip) | `ee76f98a0e90ae59d0ef2762097ea7bb` | 41 | 364,416 |

**The value of record does not move.** `cmp` returns clean against the Stage C artifact —
byte-identical, not merely the same md5.

Round 25 runs **one legalization pass with an empty no-good set and no confirmation round**,
which is the blast-radius claim §4 Stage B makes for every model with no exact-clearance
refusals. Its run log now says so in as many words:

```
legalize: pass 1 entering no_goods=0 round=none
legalize: pass 1 LOWERED in … ms — passes=1 no_goods=0 per_terminal=[none]
          confirmation_rounds=0 voided_rounds=0 round_arms_measured=0
```

### `zombie-3-a` — a model that enters the repair loop, and still moves zero bytes

| artifact | Stage C `6a2c2e3` | Stage B `ab0c3d7` | verdict |
| --- | --- | --- | --- |
| `zombie-3-a.chitubox` | 3,750,864 bytes, 215 records | 3,750,864 bytes, 215 records | **byte-identical** (`cmp` clean) |
| `zombie-3-a.coverage.repair.json` | — | — | exactly four wall-clock fields differ |

The four are `wall_time_total_ms` and the min/median/max per trial. They record how long the
run took, so they cannot be identical between two runs and their movement is evidence of
nothing. The repair trajectory is identical on both sides: `trials=35 commits=3
remaining=485 diversity_demotions=2185 of_which_tried=34 no_improvement=32`. `zombie-3-a`
also runs **one** legalization pass with an empty set.

### `Deinonychus` — the seam is CLOSED, and the model emits for the first time on this lane

Stage A left it refusing honestly at Brace. Stage B legalizes it. MEASURED, 67 s wall under
a 1500 s harness ceiling that did not fire:

| pass | ms | what happened |
| --- | --- | --- |
| 1 | 8 424 | terminal 113's own arm `ModelAnchor/gen98` condemned → no-good |
| 2 | 8 074 | terminal 113's own arm `ModelAnchor/gen123` condemned → no-good |
| 3 | 3 034 | **steering spent** — terminal 113 has exactly 2 arms → round OPENS |
| 4 | 8 090 | round rank 0 of 2 measured illegal |
| 5 | 8 282 | round rank 1 of 2 measured illegal |
| 6 | 10 554 | round rank 2 of 2 → terminal 113 retires on the completed round; **LOWERED** |

`passes=6 no_goods=2 per_terminal=[t113=2] confirmation_rounds=1 voided_rounds=0
round_arms_measured=2`. Result **EMITTED**, 1 149 support records, 10 390 488 bytes.

**The residue, arm by arm, measured inside a sealed record for the first time.** Both of
terminal 113's arms are `ModelAnchor`, and both were refused by the **axis-transit** screen,
not by distance:

```
rank0  gap=-0.050000mm (sentinel)  distance_clearance=+0.280044mm  r=0.372136/0.372136mm
rank1  gap=-0.050000mm (sentinel)  distance_clearance=+0.280044mm  r=0.372136/0.372136mm
```

`+0.280 mm` of true distance clearance on both. **These arms are not too close to the model;
their axes pass through it.** That is task #52's class, and the `-0.050000` is the
`min_illegal_len_mm` sentinel the 2026-08-23 correction above identified — it is not a gap,
and it is labelled as one here so nobody reads it as the sub-0.3 mm residue a second time.

### `Rotting Warhorse` — the seam is CLOSED at attempt 0, same shape, same round length

MEASURED under a 6 000 s harness ceiling. Terminal 177, exactly as the S-1 pre-measurement
predicted:

| pass | ms | what happened |
| --- | --- | --- |
| 1 | 22 975 | terminal 177's own arm `ModelAnchor/gen113` condemned → no-good |
| 2 | 22 318 | terminal 177's own arm condemned → no-good |
| 3 | 5 505 | **steering spent** — 2 arms → round OPENS |
| 4 | 22 416 | round rank 0 of 2 |
| 5 | 22 283 | round rank 1 of 2 |
| 6 | 29 110 | round rank 2 of 2 → terminal 177 retires; **LOWERED** |

`passes=6 no_goods=2 per_terminal=[t177=2] confirmation_rounds=1 voided_rounds=0
round_arms_measured=2`. The six passes cost **123 567 ms** of legalization, summed from the
per-pass lines above.

**THE ROTTING WARHORSE DID NOT EMIT, and the row above is about attempt 0 only.** Stage B
closed the seam it was blocked at; the run then entered the coverage-repair loop and ground
there at `commits=0` until the harness ceiling stopped it. That is a pre-existing condition
of that model and not a Stage B outcome, and neither fact substitutes for the other. The
fired-timeout line is quoted in the run's own log and in the Stage B evidence index.

### The cost, and the memo's named risk re-priced against measurement

§4 Stage B named the risk as "up to ~100 passes at 17.8 s per retirement, where 100 is the
largest arm count MEASURED at one terminal". **Both models that needed the mechanism have
TWO arms at the failing terminal, not 100.** Six passes each. The 100-arm terminal was
measured on `zombie-3-a`, which needs no passes at all.

That is a measurement of two models and it is not a corpus claim. The mechanism remains
UNCAPPED in engine semantics, per authorization item 4, and the only ceiling anywhere is the
test/eval harness's wall clock.

### The sealed-fingerprint budget: ONE move, and Stage B does not spend a second

Authorization item 5 bought one corpus-wide transcript re-freeze, paid at Stage A
(`3311374`). Stage B's `NoWinnerEvidence::ConfirmationRoundExhausted` is a **new
discriminant**: the three existing variants fold exactly as they did, so every model that
retires nothing through a round keeps its transcript fingerprint byte for byte. The
toed-slab sealed-digest goldens and `coverage_gate_toed_slab.json` are green and unmoved.

**No pin moved in this lane. Unexplained: nothing. Superseded: nothing.**


### CORRECTION to this entry's own headline — one model emitted, not two

**Rule 1 of this ledger is that a correction is a new row, not an edit.** The heading and the
commit subject of `cfc1e3d` both say *"both blocked models emit"*. That is true of the
Deinonychus (`emitted=1`, RC=0, 1 149 records) and **NOT** true of the Rotting Warhorse,
whose ATTEMPT 0 lowered and whose RUN did not emit.

The two claims are different and the one that reached the headline was the favourable one.
The body of the entry hedged it correctly from the start, which is precisely why it is worth
recording: the arithmetic was sound and the framing was one step ahead of it, which is the
failure mode this project has already named for itself.

**The accurate sentence:** Stage B closed the sealed-`Routed` seam on both models —
Deinonychus terminal 113 and Rotting Warhorse terminal 177, each in six passes with a
two-arm confirmation round. **One of the two models then emitted.** The other was stopped by
the harness while the coverage-repair loop was still at `commits=0`, which is a separate
open condition of that model and is neither caused by nor fixed by this stage.

### SECOND CORRECTION — how the Rotting Warhorse run actually ended

The correction above says the run "ground there at `commits=0` until the harness ceiling
stopped it". **The harness ceiling did not fire.** The supervising shell was terminated by
the environment first, so `harness.sh` never wrote its own
`HARNESS-TIMEOUT FIRED at Ns; last state: …` line and never wrote a `STOP` line at all.

**The last state the run actually recorded**, read off `rw-round/run.log`:

```
coverage-repair progress: trials=109 commits=0 remaining=6204 elapsed=3438728ms
```

So: **3 438 728 ms (57.3 min) into a 6 000 s ceiling, 109 trials, ZERO commits**, and then
the log stops. The `plinth` child did not outlive its supervisor — the log stopped growing.

Three things this does and does not license:

* It does **not** license "the timeout fired". Nothing was measured about where the ceiling
  would have landed, and the harness's loud-stop path is unwitnessed on this model.
* It does **not** change the Stage B result. Attempt 0 lowered at pass 6, and everything
  that claim rests on — six pass lines, the round's two measured arms, the retirement — was
  written to the log more than 55 minutes before the run was cut short.
* It does mean the Rotting Warhorse has **no terminal disposition on this lane**. It neither
  emitted nor refused; it was interrupted. The open question is the coverage-repair loop's
  convergence on that model, which is a pre-existing condition Stage B neither caused nor
  addressed, and `trials=109 commits=0 remaining=6204` is the measurement of record for it.

---

## 2026-08-23 — Stage D (the rescue reaches the sidecar): zero bytes, three models, two binaries

| recorded | commit | branch | md5 of `Round 25.chitubox` | support records | bytes |
| --- | --- | --- | --- | --- | --- |
| 2026-08-23 | `5f5c2d6` | `feature/a4-controller` (seal-semantics Stage D tip) | `ee76f98a0e90ae59d0ef2762097ea7bb` | 41 | 364,416 |

**The value of record is unchanged.** Stage D is disclosure: `LegalizationDisclosure`
travels on `RepairRunReport`, which feeds no digest, no fingerprint and no byte-pinned
golden. No pin moved. Authorization item 5's one-time sealed-fingerprint spend was paid at
Stage A and is not spent again.

### The proof: two binaries, three models, `cmp` on every artifact

Both binaries built at a CLEAN tree, `gate` profile, and both provenance files record
`tree modified : 0 path(s)`.

| | binary sha256 | commit |
| --- | --- | --- |
| Stage D | `e2fd972d0b64e1ef218d4b479794e57fbdcb4b444cb57e41cb453bc761b5747f` | `5f5c2d6` |
| baseline | `afb8d347068f6ad0ffa91ef254a53563bed847a027a323c80b1cc683632ca7a9` | `5f12e11` (Stage B tip, built in a detached worktree) |

| model | outcome | elapsed (D / base) | artifact bytes | `cmp` |
| --- | --- | --- | --- | --- |
| Deinonychus | emitted | 77 s / 65 s | 10,390,488 | **CLEAN** |
| Round 25 (canary) | emitted | 2 s / 2 s | 364,416 | **CLEAN** |
| zombie-3-a | emitted | 271 s / 256 s | 3,750,864 | **CLEAN** |

Harness ceiling **1 500 s** per model, in the harness and never in the engine. **It did not
fire on any of the six runs**, so the loud-stop path is still unwitnessed on real geometry —
carried forward from Stage B unchanged.

**The sidecar diff is the whole change, and it was stated before it was measured.** Compared
field by field (`jq paths(scalars)`), each `.coverage.repair.json` differs from its baseline
in exactly the new `report.legalization` object — and, on zombie-3-a alone, in the four
wall-clock fields, which are evidence and not a verdict and differ run to run by
construction. Deinonychus and Round 25 spend no repair trial, so their wall times are zero
on both sides and the legalization object is their **only** difference.

### What the Deinonychus now ships beside its artifact

MEASURED, read out of `Deinonychus.coverage.repair.json`:

```
passes 6 · no_goods 2 · no_goods_by_terminal {113: 2}
confirmation_rounds 1 · voided_rounds 0 · round_arms_measured 2
rounds[0] = terminal 113, opened_at_pass 3, arms_measured 2,
            no_goods_at_open 2, outcome terminal_unrouted_at_lowering
disturbances = pass 1 own [113] learned 1 · pass 2 own [113] learned 1
             · pass 3 steering_spent 113 · pass 4 own [113] round_arm_measured
             · pass 5 own [113] round_arm_measured
```

Every one of those numbers matches the run log's own `legalize:` lines for the same run,
which is the point: the log and the sidecar are one source, and the sidecar is the half that
survives the terminal window closing.

**The round OUTCOME is named for what was observed, not for what it implies.**
`terminal_unrouted_at_lowering` says the lowered run's sealed transcript carries terminal 113
as `UnroutedNoWinner`. It deliberately does not say *"this round retired it"* —
`UnroutedNoWinner` has four sealed causes and only one of them is a round, so that sentence
is an inference and a field name is the wrong place to keep one. The mechanism record stays
where design §1.3 put it: inside `NoWinnerEvidence::ConfirmationRoundExhausted`, with the
measured millimetres.

### The quiet models print zeros, not silence

Round 25 and zombie-3-a never legalize, and both ship
`legalization: {passes: 1, no_goods: 0, confirmation_rounds: 0, rounds: [], …}`. `passes: 1`
is load-bearing: the attempt-0 loop runs at least once on every live run, so **`passes: 0`
means the file predates Stage D**, and that is the only thing telling an old sidecar from a
model that needed no rescue. It is the same rule `persist_sealed_repair_ledger` already
follows when it writes a ledger of zeros rather than no file at all.

The ledger's schema tag stays `plinth.coverage-repair.v1`. The field is additive and no
existing field changes meaning, which is that file's own documented versioning rule.

### Gates at `5f5c2d6`

lib **638** (632 at Stage B, 613 at the lane base) · clippy **0 warnings, COUNTED** ·
fmt **104 hunks, UNCHANGED** · 1 554 tests passed workspace-wide.

Reds, and every one of them matches the Stage B baseline name for name:
`a4_production_door` 2 (`the_production_door_reports…`, `the_sealed_ledger_on_disk_names…`),
`first_artifact` 2 (`a_coverage_refusal_removes…`, `corpus_sphere_drilled…`) — the four task
**#76** names, all still failing on `trials:0 commits:0` and none digest-related —
and `a4_transactional_controller` 4, the same four names as Stage B and Stage C.

`support_runner_integration` ran **2 ok / 2 FAILED**, which is its known 2-of-4, and its
fifth test `a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule`
was **STOPPED BY ME at 22.6 minutes** while its coverage-repair loop was still grinding
(`trials=407 commits=2 remaining=3028 elapsed=1355252ms`). That is a loud recorded outcome
and not a pass: **this stage has no verdict on that test.** Stage B did not run this binary
at all, so there is no earlier verdict to compare against either. The condition is the
repair loop's convergence — tasks #24/#37 — which Stage D neither causes nor addresses.

### Stage D's own unmet clause

§4 Stage D reads: *"The no-good ledger, the pass count and the confirmation-round record
join the coverage refusal sidecar, **and the corpus counts are pinned**."* The first half is
landed and measured above. **The second half is not done.** Pinning corpus counts needs
authorization item 2's 45-model run, which has now been escalated un-done through Stages A,
C, B and D. Stage D should not be read as complete while that clause is open.

---

## 2026-08-23 — Challenge response: the retry could loop forever, and now it refuses by name

The challenge returned **REJECT** on one blocking finding, and the finding is worth stating
in the form it arrived in: **the seal did not leak.** Sealed `Routed` still means final
admission; the challenger built the smuggle in both realistic shapes — the repair-trial path
and a round re-taking an arm that carries a no-good — and found no path. The lane failed on a
**termination invariant it wrote down itself and enforced on one branch only.**

### What was wrong

`LegalizationTally::apply` (`runner.rs`) states the abandon rule in its own doc: *"A pass
that would abandon while learning nothing new must refuse by name rather than loop"*, and its
`Condemned` arm says *"what is forbidden is a pass that did neither"*. The `SteeringSpent`
arm never evaluated it. A second terminal reporting its steering spent while another
terminal's round was open **voided that round, re-opened at rank 0 with `measured` empty, and
returned `Ok`** — a pass that learned no no-good AND advanced no round, accepted as progress.

**It is an exact fixed point, not a slow path.** With `T` before `U` in the cadence and both
carrying a no-good on every arm:

| pass | state entering | what happens |
| --- | --- | --- |
| A | no round | `T` spent → round(`T`) opens at rank 0 |
| B | round(`T`) | `T` forced, routes; `U` spent → void round(`T`), open round(`U`)@0 |
| C | round(`U`) | `T` not forced → `T` spent → void round(`U`), open round(`T`)@0 |
| D | = pass B | the same frozen set, the same round position, the same world |

Routing cadence is MEASURED bit-identical across replays (precondition S-1), so B and D are
the same pass. The set never grows and no round ever measures an arm. `plinth prepare` has no
wall clock: such a model **hangs with no artifact and no named reason** — the exact failure
the uncapped-round authorization (item 4) was paid for on the promise that the abandon rule
would stop it.

The design memo does not contain this branch. §1.3 admits two ways into a round — *"a context
whose steering set is full, or re-entered after the set grew, both of which are finite
events"* — and the void rule it states covers only *"any **other** terminal's arm is
**refused** during the round"*. "Another terminal reports its steering **spent**" was an
unflagged implementer addition. Its removal needs no constant and is therefore not a §4
escalation.

### What landed — two commits, one rule

**`d09f6df` — the finding.** A spent report that would displace an open round **refuses by
name**: the pass number, the held round and its rank, and both terminals. It refuses BEFORE
it touches anything — no `VOID` log line, no counter, no round row, no disturbance row — which
is the `Condemned` arm's own stated discipline: a pass that never continued leaves no row.

**`c2834d7` — the same root, one arm over, found while fixing it.** The `Condemned` arm's
rule read `learned_count == 0 && !round_advanced && !voided`. That third clause let a pass
whose **only** effect was destroying a round count as progress, and it cycles the same way:
void, re-enter against the unchanged set, void again. Voiding is not progress — it destroys
work rather than making any. The clause is gone and the rule is now evaluated **before** the
void, so a refusing pass destroys nothing on its way out.

**Read this commit against the challenge report, because it looks like a contradiction and is
not.** The challenger wrote that *"the Condemned-side void is correct and matches §1.3"*, and
it is: what was blessed is the **void rule** — another terminal's own arm condemned during a
round voids it — and that rule is untouched. What changed is the **acceptance disjunct**, a
separate clause the challenge did not analyse, which decided whether such a pass may run
another. Reaching the changed behaviour needs a defect rather than hard physics: a terminal's
own arm condemned while carrying the exact `(terminal, prefix, arm)` triple that should have
suppressed it, which `TerminalSteering::probe` makes impossible today. That is the argument
for refusing rather than for leaving it — a defect that names itself costs one refusal, and a
defect that spins looks like a slow model. The change can only refuse where the old code
looped; it cannot loop where the old code refused.

Moving the rule ahead of the void puts three mutations in one pass in a fixed order, so the
MIXED pass — a round advancing **and** another terminal's arm condemned in the same pass — is
pinned rather than left to chance by
`a_pass_that_both_advances_a_round_and_voids_it_keeps_the_arm_it_measured`: the arm stays on
the round's row, the round is still voided, and the pass still goes on. Its behaviour is
pre-existing and did not move.

The rule is now written once, on `apply`, and reads identically on both arms:

> Progress is a no-good entering the set, a round advancing its rank, or a round OPENING
> where none was open. Nothing else. Voiding is not progress.

`ConfirmationRoundOutcome::Voided` loses its stale second clause; a spent report can no longer
void a round.

### The evidence is unit-scale BY NECESSITY, and here is the measurement that says so

MEASURED, from this ledger's own Stage B/D rows: **`voided_rounds = 0` on all three model
runs** — Deinonychus (line: `passes=6 … confirmation_rounds=1 voided_rounds=0`), Rotting
Warhorse shell (same), and the Stage D sidecar (`voided_rounds 0`). The mismatch class is
MEASURED at **exactly one terminal per failing run on 3/3 runs**, so **no corpus model reaches
a two-round situation at all.** A model run would prove nothing here and cost 25–100 minutes
proving it. The synthetic fixture is the instrument, and both new tests are sabotage-verified:

* `a_second_spent_terminal_refuses_by_name_instead_of_displacing_an_open_round` — asserts the
  `Err`, that it names who held whom, and that the tally and the open round are **unmoved**.
  Verified RED against the restored void-and-re-open.
* `a_pass_that_only_voids_a_round_refuses_and_leaves_the_round_standing` — verified RED in
  both directions: restoring `&& !voided` reds the `expect_err`; moving the rule back below
  the void block reds `voided_rounds`.
* `a_voided_round_keeps_its_arms_and_the_re_entered_round_starts_from_zero` — see below.

### The challenger's second symptom is DEFENDED, not fixed — and is now pinned

The challenger reported, as a consequence of the same root, that after a void the sealed
`ConfirmationRoundExhausted { round_arms }` can name 0 arms while `round_arms_measured` says
2, and asked whether §1.3 obligation 3 is violated. **It is not, and the three readings are
each honest about a different thing.** Obligation 3 is PER ROUND, and after the fix the only
surviving void is the design-sanctioned one (another terminal's arm refused, set grew):

| reading | value after `open(t113) → 2 arms → t177 condemned voids → t113 re-enters` | what it means |
| --- | --- | --- |
| `rounds[0].arms_measured` | **2**, outcome `voided` | the voided round keeps the arms it drove; they were paid for |
| `rounds[1].arms_measured` | **0**, `no_goods_at_open` **1** | the re-entered round has measured nothing YET, and it is the only thing a retirement of t113 could rest on |
| `round_arms_measured` | **2** | the RUN's cost across all rounds, voided ones included — commit `e6ec99e`'s ruling, and no retirement may read it |

`no_goods_at_open = 1` is the load-bearing one: it proves the re-entry was earned by the set
**growing**, which is what makes it one of §1.3's finite events rather than a second way to
spin. All three are asserted simultaneously in
`a_voided_round_keeps_its_arms_and_the_re_entered_round_starts_from_zero`. The open question
is now a pinned invariant, which is a better answer than a fix would have been.

### Gates at `c2834d7`

plinth-support lib **642** (638 at Stage D — four tests added, none removed) ·
clippy **0 warnings, COUNTED** · fmt **104 hunks, UNCHANGED, none in either touched file** ·
workspace lib tests **1 086 passed, 0 failed, 1 ignored** across all 14 crates
(`cargo test --profile gate --workspace --lib`) — **no new red anywhere.** The known reds
named in the brief live in integration binaries, are untouched by these commits, and were not
re-run: nothing in this response can reach them, since both changes are refusals on paths the
`voided_rounds = 0` measurement shows no corpus model takes.

**No sealed pin moves, MEASURED rather than inferred.** The pins the challenge tracked — the
four that moved to `12136443226163403608` under producer commit `3311374` — are asserted by
`cco` tests inside the plinth-support lib, and that binary ran 641/641 green on both commits.
The reasoning agrees with the measurement (neither commit adds a digest input, touches a
constants or registry file, or changes an emitted field), but the pins are pinned by tests
that RAN. The one authorized fingerprint move (item 5, `candidates_ruled_out`) is still the
lane's only one, at Stage A.

### What this response did NOT touch, named rather than left silent

* **The empty-round-at-rank-0 seal hole** the challenger flagged as latent, not blocking, with
  no witness: if a round's slate were ever empty at rank 0, `forced_rank(0) >= survivors.len()
  (0)` would seal `ConfirmationRoundExhausted { round_arms: [] }` — an empty round earning a
  retirement. Unreachable while the frozen-set determinism argument holds; it lives in
  `cco.rs` seal logic where the pins are; left alone deliberately.
* **The uncovered branch** the challenger named: a round whose forced arm **survives** bracing
  has no test at the `cco` tournament level. The Stage D *disclosure* of that outcome
  (`terminal_routed_at_lowering`) is covered by
  `the_round_row_carries_the_terminal_the_length_and_where_it_ended`; the tournament path is
  not, and no model has produced one.
* **Authorization item 2's 45-model corpus run**, still un-done through Stages A, C, B, D and
  now this response. Stage D's second clause stays open.
* The `ptr::eq` arm of `validate_against_hoisted` being vacuous on the memoized path
  (challenger's item 5 note, harmless — the fingerprint carries the load).

---

## 2026-08-24 — seal-semantics 45-model census [33/45 measured]

**This was not a completed 45-model census.** It measured 33 rows and left 12 unmeasured:
`Giant Bat`, `Tavey Badger`, `Tavey Base`, `Warhorse`, `corpus/models/Cylinder`,
`corpus/models/make_box`, `corpus/models/make_overhang_table`,
`corpus/models/make_raking_blade`, `corpus/models/make_thin_fin`,
`corpus/models/make_toed_slab`, `corpus/models/Sphere`, and
`corpus/models/Weeping Treant v1.2`. Counts below are measured counts and lower bounds;
no missing row is inferred clean, byte-identical, or outside the seal class. The label is
requested provenance, not a claim of completion.

| recorded | census summary | value |
| --- | --- | --- |
| 2026-08-24 | seal class fired | **5 / 33 measured** (at least 5 / 45) |
| 2026-08-24 | observed class size | all observed classes size **1** |
| 2026-08-24 | cascade carriers | **0** observed |
| 2026-08-24 | sub-0.300 mm residue | **0** in measured corpus evidence; 45-model result unknown |
| 2026-08-24 | printed artifacts | **21** byte-identical |

| recorded | commit range | model | baseline | tip | printed-artifact verdict | cause / label |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-08-24 | `5347901..fe91555` | `Deinonychus` | ERROR exit 5 at Structure, `TerminalEvidenceMismatch { terminal: 113 }`, 24 s, no artifact | EMITTED, 69 s, md5 `e25966a4975b4e62640bcc3e96dc452f`, 1,149 records, 10,390,488 B | **MOVED — no artifact to emitted artifact** | `seal-semantics 45-model census`; terminal 113 rescued in 6 passes, 1 completed confirmation round, 2 arms measured |
| 2026-08-24 | `5347901..fe91555` | `Rotting Warhorse` | ERROR exit 5 at Structure, 87 s, no artifact | **HARNESS CEILING**, loud 486 s run, no artifact; seam closed before coverage repair | no artifact at either end; endpoint/stage **MOVED** | `seal-semantics 45-model census`; terminal 177 legalized in 6 passes, 1 completed confirmation round, 2 arms measured; ceiling is not emission |
| 2026-08-24 | `5347901..fe91555` | `zombie-4-a` | ERROR exit 5 at Structure, `TerminalEvidenceMismatch { terminal: 23 }`, 106 s, no artifact | **25-MINUTE HARNESS CEILING**, last state `stage: support entering Brace elapsed=1453197ms`, no artifact | no artifact at either end; endpoint/stage **MOVED** | `seal-semantics 45-model census`; terminal 23, 33 abandoned passes, 2 rounds started, 11 of 22 round arms observed before ceiling |
