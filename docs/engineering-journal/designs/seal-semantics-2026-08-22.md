# What sealed "Routed" must mean — the Routing/Brace seam (task #36, E-E)

> **Provenance.** Written 2026-08-22 against `feature/a4-controller` @ `5a8ade4`, main
> checkout `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, working tree clean. Every
> `file:line` below was read at that commit. **Design only. No production code, no test
> code, and no constant is changed, added, moved or read differently by this record.**
>
> Evidence labels: **MEASURED** (a number a run produced), **DERIVED** (closed form from
> MEASURED or registered inputs), **INFERRED**, **ASSUMED**.
>
> Inputs this record answers to, in order: `candidacy-exact-mask-2026-08-21.md` (the #36
> refutation), the independent Codex opinion on this seam, the MEASURED waste of
> `wf_3b724805-678`, and `a4-controller-design.md` + `model-seated-arms-2026-08-21.md`
> with its amendment.

---

## §0. The answer in one page

**The defect.** The routing stage seals a record that says terminal 298 is routed. The brace
stage then measures that terminal's arm against the model with the arm's true radius, finds
it inside the model, and condemns it. Condemning it flips the terminal's live state to
`Quarantined` while the sealed record still says `Routed`. That pair is not one of the three
the accounting admits, so the run dies at the structure stage with
`TerminalEvidenceMismatch`. **MEASURED: the Giant Bat, exit 5, emitted=0, terminal 298 on the
drilled shell and terminal 187 on the substituted solid.**

**Where it dies, and this is new here.** MEASURED from `candidacy-out/runs/tip-Giant-Bat.log`:
the stage stream is `Preparation → Routing → Rewire → Brace → Structure` **once** per sealed
support run, with no repair-loop entry line and no second `Preparation`. The Bat therefore
dies on **attempt 0 — the model's own baseline plan, before any repair contact exists.** The
A4 controller's try/measure/keep-or-roll-back machinery cannot reach it, by that machinery's
own rule (`runner.rs:1601`: an attempt-0 stage refusal is
`SupportRunError::Failed`, not a rollback). Any fix that is only a controller fix does not
touch the Bat.

**The recommendation, in one sentence.** Keep sealed `Routed` meaning exactly what it means
today — the terminal's contact is in the final accepted set and its terminal is consumed in
the final live tree — and make that meaning *reachable* by letting the brace stage's exact
verdict be **fed back into a fresh routing pass** instead of being written on top of a sealed
record.

**Why that is not the "predict it at candidacy" direction the refutation killed.** The
refutation proves the brace verdict cannot be **predicted** when candidates are chosen,
because the deciding input (the radius) does not exist yet and its movement is two-sided
(MEASURED: 0.949212 → 1.409608 mm grew and failed; 1.048241 → 0.901736 mm shrank and failed).
It does not forbid **measuring** the verdict exactly and then choosing again with that
measurement in hand. Nothing is approximated, no bound is guessed, and no constant appears.

**Why it terminates, which is the thing naive repetition lacks.** Each failed pass adds at
least one `(terminal, candidate arm)` pair to a set that only grows, inside a universe that is
finite and enumerable (MEASURED on `zombie-3-a`: 42–106 arms proposed per terminal). A
strictly growing subset of a finite set cannot cycle. When one terminal's whole declared
candidate set is used up, that terminal retires as `UnroutedNoWinner` — **an admitted pair**
— and the pass completes. The retry loop therefore cannot fail a run on its own; it either
converges or converts into honestly unrouted terminals.

**What it costs.** One extra `route → rewire → brace` pass per refused arm. MEASURED on the
Bat: one refused arm per pass, and one pass is ~16 s. The worst case is a terminal whose
whole candidate set is refused — up to ~100 passes for that terminal — and that worst case is
reported loudly rather than capped, per the no-cap doctrine.

---

## §1. The contract question

### §1.1 What sealed `Routed` means today, read from the code

Three functions carry the whole meaning, and all three were read at `5a8ade4`.

* `finalized_unrouted_terminals` (`runner.rs:5652`) walks every sealed record row against the
  **live** tree ledger and admits exactly three pairs: `Routed`↔`Consumed` (`:5671`),
  `SkippedQuarantined`↔`Quarantined` (`:5672`), `UnroutedNoWinner`↔`Retired` (`:5681`).
  Everything else is `Err(TerminalEvidenceMismatch)` at `:5690`. A cardinality check at
  `:5693`–`:5698` refuses a ledger holding terminals the record never named.
* `accepted_contact_set_after_routing` (`runner.rs:4346`), called **once** at
  `runner.rs:3935`, applies SPEC §6's removal at the point the clause names, and does the
  carried-load repartition exactly once per pass.
* `validate_accepted_contact_set` (`runner.rs:4242`) re-derives that removal ledger from the
  sealed record and the live tree rather than trusting the stored value, at three later
  points: `runner.rs:5411`, `:5834`, `:5904`.

Read together, the meaning of a sealed `Routed` row is:

> **This terminal's contact is in the final accepted contact set, and this terminal is
> `Consumed` in the tree the run finally ships.**

That is a claim about the **end** of the pipeline, made at the routing stage. The brace stage
falsifies it. Specifically, `brace.rs:523` calls `working.quarantine_subtrees(&negative.contact_refusals)`
inside the brace stage, after the record is sealed. The comment at `brace.rs:498`–`:517` says
why the sweep must cascade: a model-seated arm is minted as a root with the contact stub
re-parented onto it, so condemning the arm condemns the stub, and the stub is the terminal.

**The illegal pair is not a bookkeeping nit.** If it were admitted, the accepted contact set
would keep a contact whose support was condemned, and the load partition would keep charging
model faces to it. The structure-stage error is the audit catching a real print-safety defect.
Any answer that makes the check lenient is wrong.

### §1.2 Candidate 1 — Codex's hybrid: two seals

**The contract.** The routing seal becomes "routing's decision, recorded". A second,
final-commit seal is created after the brace stage has legalized the tree exactly. Only a
finally-committed pass may emit. A pass that cannot be legalized is **aborted** as a whole,
and a new pass starts with a sound conflict recorded.

**What changes in the accounting.**

| site | change |
| --- | --- |
| `finalized_unrouted_terminals` (`runner.rs:5652`) | Splits in two. A routing-time form compares the record with the tree **as routing left it**; a new final-commit form compares a new post-brace attestation with the final tree. The three-pair table moves to the final form. **It must not gain a fourth pair** — E-E forbids it — so the split is mandatory, not optional. |
| `accepted_contact_set_after_routing` (`runner.rs:4346`) | Stops being the accepted set and becomes a provisional set. A **second** removal pass must run after bracing, with a **second carried-load repartition**. |
| `validate_accepted_contact_set` (`runner.rs:4242`) | Each of its three call sites must say which of the two sets it validates. Its recompute-from-state tripwire must be built twice or it protects half of what it protects today. |
| `brace.rs:523` cascade | Stays, and becomes legitimate: it is the second stage's own removal mechanism. |

**What the verifier must then enforce**, adapting Codex's list to this codebase's names:

1. Complete and exclusive terminal accounting **within** each pass, at both seals
   (`finalized_unrouted_terminals` in both forms).
2. Lineage: the final attestation names the routing attestation it was built from, and the
   existing chain (`extend_contact_attestation_chain`, `runner.rs:3908`, stage tags
   `0x10/0x11/0x12`) gains a fourth link.
3. `verification_snapshot_digest` in `plinth-verify/tests/coverage_gate_toed_slab.json` folds
   the new attestation, so the golden moves for every model.
4. Aborted passes never reach output — a new property, because today no pass can be aborted.

**The honest cost.** SPEC §6 sequences the carried-load repartition **before** rewire,
bracing and coverage judgment. A second repartition after bracing is a deviation from the
spec that needs Craig's ruling. Worse, it is not stable: the repartition changes the load on
every surviving contact, which changes every path-level worst span, which changes every
radius the resize assigned — so the tree the second seal certifies is not the tree the second
seal measured. Closing that gap means bracing again, which is exactly the fixed point Codex
warns has no termination proof.

**And it still does not fix the Bat.** The Bat aborts on attempt 0, and there is no attempt
1: `run_support` calls the stage chain once for the baseline plan (`runner.rs:1596`) and turns
any refusal into `SupportRunError::Failed` (`:1601`). The hybrid therefore needs the retry
loop **as well as** the second seal.

**Reversibility.** Poor. Once the evidence sidecar and the verifier golden carry two
attestations, backing out is a second format move.

### §1.3 Candidate 2 — strict, made reachable (RECOMMENDED)

**The contract.** Sealed `Routed` keeps today's meaning, word for word. Nothing is admitted
late, no fourth pair is minted, and the brace stage never writes on a sealed terminal's
ledger. What changes is that a brace-stage refusal of a **sealed-routed** terminal's support
**abandons the pass** instead of quarantining, and the next pass routes again with that exact
arm excluded.

**What changes in the accounting: nothing.**

| site | change |
| --- | --- |
| `finalized_unrouted_terminals` (`runner.rs:5652`) | **Unchanged.** Three pairs, same catch-all error, same cardinality check. It becomes a tripwire that should never fire, because the brace stage refuses first. |
| `accepted_contact_set_after_routing` (`runner.rs:4346`) | **Unchanged.** One removal, one repartition, at the point SPEC §6 names. |
| `validate_accepted_contact_set` (`runner.rs:4242`) | **Unchanged**, all three call sites. |

**What happens to `brace.rs:523`'s cascade.** It splits, and the split is decided by the
sealed record, not by a member's kind:

* The sweep's condemnation set is computed first, read-only. The walk that computes it already
  exists as `Forest::collect_subtree_leaf_first` (`forest.rs:1004`), a `&self` method, so
  "what would this sweep condemn?" is a query, not a mutation.
* If that set contains **no** terminal the sealed record called `Routed`, the sweep is applied
  exactly as today. This is the refutation's own carve-out (§B): brace rungs and model-anchor
  arms whose condemnation disturbs no contact accounting. **Their behaviour does not move.**
* If it **does** contain one, nothing is quarantined. The pass is abandoned and the refusal
  travels out as evidence: for each disturbed terminal, the arm its tournament chose, the
  measured clearance margin, and the radius the resize assigned.

**What the verifier must then enforce.** Two additions, both arithmetic:

1. A terminal that retires `UnroutedNoWinner` because every candidate was refused by the exact
   post-brace check must show that **as a count**: candidates refused by exact clearance plus
   candidates refused earlier equals candidates generated. Exhaustion is arithmetic, never a
   claim. This is one new named evidence variant beside the three that already exist in
   `NoWinnerEvidence` (`cco.rs:657`).
2. A run that emitted must report its legalization pass count, and that count must equal the
   number of recorded exclusions plus one. A pass that was abandoned may never reach output —
   which is true by construction here, because an abandoned pass produces no lowered run and
   therefore no sealed authority.

Everything else on Codex's list is **already enforced** and needs nothing: complete and
exclusive terminal accounting (the three-pair table plus the cardinality check), immutable
digests and lineage (`extend_contact_attestation_chain`, and the five prepared digests
restored on every rollback at `runner.rs:1888`), and "no selected terminal silently
disappears" — that is precisely the error the Bat raises today.

**The exclusion, and why it is exact rather than a guess.** An exclusion names one
`(terminal, candidate arm)` pair. It does **not** say "this arm can never work"; Codex is
right that such a claim is unsound, because the same arm might be legal under a different
shared path. It says only: *in the pass series this deterministic router produces, this
candidate was measured illegal at its true radius, and it will not be chosen again.*

**Keying it, which is the one genuinely delicate part.** MEASURED, from the refutation §C5:
member slot numbers are **not stable across passes** (the same −0.040503 mm strut is slot 486
in one pass and slot 410 in another, with identical endpoints). So the exclusion cannot be
keyed on a slot. It is keyed on the terminal's own stable identity plus the candidate's
position in that terminal's tournament — and that position is stable by induction:

> The cadence replays in the same order, and the tree state a terminal sees depends only on
> the outcomes of the terminals before it. Every earlier terminal's outcome is fixed by the
> exclusion set, which is unchanged for them. So the tournament that regenerates for terminal
> T is bit-identical to the one that produced the excluded candidate, and the position means
> the same thing it meant.

That induction is an assumption about determinism, so it is **checked, not trusted**. Each
exclusion carries a witness — the candidate's own recorded evidence (`CcoCandidateEvidence`,
`cco.rs:605`, and the arm's endpoint coordinates by exact bit pattern). On replay the witness
is re-derived and compared. A mismatch is a hard, named error meaning "the replay diverged",
never a silent skip. This is the same recompute-from-state idiom
`validate_accepted_contact_set` already uses, in the same house style. No tolerance, no
rounding, no constant.

**Termination, stated as a proof rather than a hope.** Let `U` be the set of all
`(terminal, candidate)` pairs the router declares in a pass. `U` is finite and MEASURED
(42–106 candidates per terminal on `zombie-3-a`). The exclusion set `X ⊆ U` grows by at least
one element per abandoned pass and never shrinks within one plan. Therefore the number of
abandoned passes is at most `|U|`, and a cycle is impossible. When a terminal's own slice of
`U` is exhausted, that terminal retires `UnroutedNoWinner` rather than abandoning the pass, so
the pass completes. **The retry loop has no give-up state of its own.**

**No cap, and no counter deciding an outcome.** The loop stops when the physics stops
producing refusals, or when a terminal's declared candidate set is used up. Both are measured
events. The pass count, the exclusion count and the wall time are printed and decide nothing —
the same rule `RepairRunReport` (`coverage_repair.rs:671`) already follows.

**Reversibility.** Good. Stage A (§4) alone is one classification and one named outcome, and
reverting it restores today's bytes. Stage B is one loop and one parameter; deleting the
parameter restores Stage A exactly.

### §1.4 The fit check: a generalization, or a second mechanism?

Craig asked this directly. The answer is **a generalization, with one genuinely new surface**,
and here is the line-by-line.

**What the A4 controller already has, that the retry loop reuses unchanged:**

| the controller has | the retry loop needs | same thing? |
| --- | --- | --- |
| `run_stage_chain` hands the context back on every stage refusal (`ChainOutcome::Refused { context, stop }`, `runner.rs:1298`–`:1409`) | a pass that abandons must hand the context back intact | **Yes.** The abandon path is that path with a payload. |
| `RepairLedger::ledgered` — a set of tried rows that deliberately survives a rollback (`runner.rs:2007`) | a set of excluded candidates that survives an abandoned pass | **Yes**, same idiom, different key. |
| `context.restore_prepared_digests(digests)` on every rollback (`runner.rs:1888`) | the same, on every abandoned pass | **Yes**, identical need, identical call. |
| counters that report and never decide (`RepairRunReport`, `coverage_repair.rs:671`) | pass count, exclusion count, wall time | **Yes**, the new counters go in the same report. |
| termination by a monotone exclusion set over a finite pool | termination by a monotone exclusion set over a finite candidate universe | **Yes**, the same argument one level down. |

**What it does not have, and this is the honest new work:**

1. **A retry at attempt 0.** `run_support` calls the stage chain once for the baseline plan
   (`runner.rs:1596`) and converts any refusal into `SupportRunError::Failed` via
   `RefusedStage::into_attempt_zero_failure` (`runner.rs:1332`). The retry must wrap **both**
   that call and each trial's call (`runner.rs:1874`). That is one new `loop` around an
   existing call — not a new subsystem.
2. **A channel for exclusions into the tournament.** `route_initial_cco_with_limits`
   (`runner.rs:3785`) gains one borrowed parameter. This is the one real new surface.

**The two exclusion sets have different lifetimes, and confusing them would be a defect.**
`ledgered` is per **run**: SPEC §6 says exact failed rows stay excluded across commits.
Exclusions are per **plan**: a trial's appended contact changes the cadence, so a candidate
position from the previous plan means something else. **Exclusions are keyed by the plan
digest and discarded whenever the plan changes.** Stated here so no implementer carries them
across a commit by accident.

So the shape is two nested loops with clean, different jobs: the outer loop tries **different
plans** (the A4 controller, unchanged); the inner loop legalizes **one plan** (new). They
share the context handback, the digest restore and the report.

### §1.5 The two candidates side by side

| | Candidate 1: two seals | Candidate 2: strict, made reachable |
| --- | --- | --- |
| meaning of sealed `Routed` | routing's decision only | unchanged: final admission |
| fourth pair | never minted (a second seal instead) | never minted |
| `finalized_unrouted_terminals` | splits in two | unchanged |
| `accepted_contact_set_after_routing` | provisional; a second removal after bracing | unchanged |
| carried-load repartition | **twice**, and the second one is a SPEC §6 deviation | once, where §6 says |
| `validate_accepted_contact_set` | three call sites must choose a set; tripwire halves unless rebuilt twice | unchanged |
| `brace.rs:523` cascade | stays whole, becomes the second stage's removal | splits by what the sweep would condemn |
| verifier | new attestation, new lineage link, golden moves corpus-wide | two arithmetic checks added |
| fixes the Bat on its own | **no** — still needs the retry loop | **yes** |
| reversibility | poor (format and golden moves) | good (one classification, then one loop) |

---

## §2. The waste question

### §2.1 What the 89 actually are

MEASURED, `zombie-3-a`, run `wf_3b724805-678`, verified twice: 92 trials, 3 commits, 89
rejections, **all 89 are clearance kills**.

* **52** were condemned after the resize — the trial's own appended contact ended quarantined
  or unrouted.
* **37** ended at routing with no winning arm. Every one of the 37 is
  `all_feasible_candidates_refused_by_clearance`: 42–106 arms proposed, 37–102 of them
  feasible, **zero** trials with no feasible arm. Arms existed; the clearance standoff refused
  every one.
* **29 of those 37 are retries on island 8.**

Rejected trials roll back, so between two commits the accepted plan is identical.

### §2.2 Is a refusal against an unchanged world sound to reuse?

**Craig's question, answered precisely: yes in principle, and it buys nothing here, because
the 89 are 89 different computations rather than one computation paid 89 times.**

Reuse of a stored result is sound **if and only if** the key covers every input the
computation reads. Get that right and it is not a screen and not a heuristic: it is the same
function on the same arguments, and a stored answer is the answer.

Now apply it. A trial computes `f(P + r_i)` where `P` is the accepted plan and `r_i` is the
one appended repair row. Between commits `P` is fixed, but `r_i` differs on every trial, so
the **inputs differ on every trial**. A stored refusal for `r_i` reused for `r_j` is not a
stored answer; it is a prediction that `r_j` will behave like `r_i`. That is the guess the
no-cap doctrine forbids, and it is exactly the ranking rule that was already built, MEASURED
HARMFUL and reverted.

And the reuse that *would* be sound is already in place. `RepairLedger::ledgered`
(`runner.rs:2007`) holds every tried row, survives rollback, and is honoured across commits —
so a row is never re-tried against any plan. **The identical computation is already never
paid twice.**

### §2.3 What a sound key is — and it is not a world epoch

An epoch counter is a **proxy** for the inputs. It is sound only if you can prove nothing
outside the counter can move an input, and that proof is the thing that goes stale. This
codebase already has the honest key in hand, and it is not a counter: the five prepared
digests (`plan`, `loads`, `initial_forest`, `contact_attestation`, `prepared` — captured and
restored at `runner.rs:1816` and `:1888`) plus the geometry digest. Those **are** the pass's
own inputs, recomputed from state rather than asserted.

> **Rule.** Key any stored result on the digests of the inputs the computation reads. Never on
> an epoch, a counter or a generation number. A digest mismatch is a miss; it can never be a
> silent stale hit.

Under that rule the trial-level memo has **zero hits**, because every trial's plan digest
differs. That is the proof, rather than the assertion, that the 89 are not a memoization
problem.

### §2.4 Where the real savings are — three of them, all doctrine-clean

1. **Run-invariant work rebuilt inside every trial.** `SurfaceAnchorPool::from_geometry`
   depends only on `geometry` and `delicacy`, both fixed for the whole run, and is rebuilt at
   **two** sites per trial (`runner.rs:2686` and `:3383`). MEASURED: a 494,617-row build per
   trial on the toad, paid twice. The legacy pre-routing replay adds ~3 `validate_binding`
   calls per trial, each re-running a full baseline selection and a coverage-sample scan;
   MEASURED at 1,383 executions across the 45-model sweep. **These are the same function on
   arguments that provably never move.** Keying them on the geometry and configuration digests
   is memoization in the strict sense: it decides nothing, changes no verdict, and a sabotage
   test (corrupt the stored value, expect a different artifact) proves it.
2. **The 37 die when the mask is honest, not when the controller is clever.** They are
   terminals where dozens of feasible arms were all refused by the model-anchor clearance
   standoff. That is task #51's finding — the model-anchor clearance mask is unsatisfiable —
   and the amendment records that the fix flips the theorem. No controller change reaches
   them. §F.1 of the refutation already named and counted the class; **naming it was the
   deliverable, and a screen is not.**
3. **The 52 stop being dishonest under §1.3, which is worth more than the passes it costs.**
   Today a trial whose own contact is condemned after the resize carries a plan whose
   accounting disagrees with its tree. Under the recommended design that contact retires at
   the seal, SPEC §6's strip removes it from the accepted set, the load repartition stops
   charging faces to it, and the coverage measurement judges a true state. Some of those 52
   may then commit. **That is unmeasured and this record does not claim it** — it is a Prove
   result, not a design claim.

### §2.5 Reconciling "29 on island 8" with the no-cap doctrine

The loop is not re-paying the same answer. It is asking 29 different questions and getting the
same answer, because the underlying cause is one route whose clearance mask refuses everything.
Suppressing the 30th question would be guessing.

The doctrine-clean way to spend less on island 8 is to **make the cause visible and let the
existing order rules act on it**, and both mechanisms are already designed:

* Rule N4's spatial diversity, which after a rejection sorts every remaining candidate inside
  the rejected seat's own coverage disc **after** every candidate outside it. It reorders; it
  never excludes; completeness is untouched.
* The named per-route cause `Blocked(ClearanceMaskUnsatisfiable)`, which the refusal already
  reports with the terminal count and the millimetre shortfall.

Both put island 8 last and say why. Neither is a counter deciding an outcome. Whether they cut
the 29 is a measurement, not an argument.

---

## §3. The Giant Bat, traced step by step

MEASURED starting facts, from the refutation §C5 and the run log: two sealed support runs (the
drilled hollow shell first, then the substituted oriented solid), each dying at the structure
stage on attempt 0. Drilled shell: terminal 298, arm at slot 377, brace clearance
−0.160653 mm, radius 0.949212 mm at candidacy and 1.409608 mm at bracing. Oriented solid:
terminal 187, slot 250, −0.031388 mm, 1.048241 → 0.901736 mm.

Under the recommended design, the drilled shell walks like this.

1. **Preparation.** Unchanged.
2. **Routing, legalization pass 1.** The exclusion set is empty. Terminal 298's tournament
   proposes its candidates; the winner becomes the arm at slot 377. The record seals
   298 ↔ `Routed`. `accepted_contact_set_after_routing` finds nothing to remove for 298.
3. **Rewire.** Unchanged.
4. **Brace.** `resize_forest_current_parity` (`transient.rs:428`) assigns the arm
   1.409608 mm from the braced tree's path-level worst span. `exact_clearance_negatives`
   (`brace.rs:1854`) measures −0.160653 mm. The arm lands in `contact_refusals`.
5. **The new step.** The condemnation set is computed read-only. It contains the arm **and**
   terminal 298's stub, because a condemned carrier takes its dependant with it. Terminal 298
   is sealed `Routed`, so this refusal disturbs contact accounting. **Nothing is quarantined.**
   The pass is abandoned, carrying one exclusion: terminal 298's winning candidate, with its
   witness, its −0.160653 mm margin and its 1.409608 mm radius.
6. **Legalization pass 2.** The whole chain runs again on the same plan. Terminal 298's
   tournament regenerates identically; the witness is re-derived and matches; the excluded
   candidate is refused with the named reason and drops out of the feasible count; the
   next-best feasible candidate wins. The record seals 298 ↔ `Routed` on a **different** arm.
7. **Outcome.** Either the brace stage is clean — and the run proceeds to Structure, Lowering
   and the coverage measurement — or another arm is refused, another exclusion is added, and
   step 6 repeats. The loop cannot cycle (§1.3) and cannot fail the run on its own.

**Every terminal state the Bat can reach, and the accounting rows each writes.**

| # | state | how it is reached | accounting rows written |
| --- | --- | --- | --- |
| **T1** | **Emits.** | Legalization converges; Structure and Lowering pass; the coverage measurement finds no blocking patch, or the A4 loop closes it inside the committed-step bound. | Record row 298 = `Routed`, ledger = `Consumed` — the first admitted pair. `finalized_unrouted_terminals` returns 298 in neither list. `accepted_contact_set_after_routing` writes no removal for it. The report carries `legalization_passes = k`, `exclusions = k − 1`, with each exclusion's terminal, margin and radius. |
| **T2** | **Emits with terminal 298 honestly unrouted.** | Terminal 298 uses up its whole declared candidate set; every one was refused by the exact post-brace check. | Record row 298 = `UnroutedNoWinner` with the new evidence variant (`candidates_generated`, `refused_by_exact_clearance`, and the arithmetic that they account for each other), ledger = `Retired` — the third admitted pair. `accepted_contact_set_after_routing` writes one `UnroutedRegularRemoval` row (contact id, terminal id, route, reason, carried area) and the single batched removal runs. `NoncoverageVector.unrouted_no_winner` rises by one. The coverage measurement then judges the hole; if it is below the 0.16 mm² censor or covered by neighbours, the model seals and emits. |
| **T3** | **Honest coverage refusal.** | As T2, but the hole is above the censor and the repair loop cannot close it. | Everything T2 writes, plus `SupportRunError::CoverageClosure` carrying the per-route outcome `Blocked(ClearanceMaskUnsatisfiable { terminals, best_shortfall_mm, standoff_mm })`, the trial stream and the legalization ledger. Exit is non-zero and nothing is emitted — **but every number is named**, which is what today's `TerminalEvidenceMismatch` does not do. |
| **T4** | **A genuine clearance violation.** | The refused member declares **no** model contact, so `pipeline_violations > 0` at `brace.rs:554`. | Unchanged from today: `BraceError::ClearanceViolation`. This path is not touched. |
| **T5** | **Some other stage fails.** | For example Lowering's `RootRunTooShort` (task #29). | Unchanged: `SupportRunError::Failed { phase, detail }` on attempt 0, or a ledgered rollback on a trial. |

**The state that is deliberately absent: there is no "legalization gave up" run failure.** A
terminal that uses up its candidates retires (T2/T3); it does not abandon the pass. That is
what makes the loop safe to add.

**What this record does not promise.** It does not claim the Bat emits. It claims the Bat
reaches T1, T2 or T3 — an emission or a disclosed refusal — instead of an accounting-integrity
error. Which of the three it reaches is a measurement, and the measurement needs the code.

---

## §4. Staged landing plan

**No stage needs a new registered constant.** The exclusion key is an exact bit comparison; the
coverage radius, the censor and the clearance standoff are all read from the registry exactly
as they are read today. If any implementer finds themselves wanting a tolerance, that is an
escalation, not a design detail — say so and stop.

### Stage A — name the class; abandon instead of minting the illegal pair

**What lands.** `brace_fixed_point_current_parity` (`brace.rs:427`) takes one added, borrowed
argument: the set of terminals the sealed record called `Routed`. Before the sweep at
`brace.rs:523` it computes the condemnation set read-only (the walk exists at
`forest.rs:1004`), intersects it with that set, and:

* empty intersection → applies the sweep exactly as today, returns
  `BraceFixedPointTermination::ContactRefused` exactly as today;
* non-empty → applies nothing, and returns a new named termination carrying the disturbed
  terminals with their margins and radii.

`brace_state` (`runner.rs:4718`) supplies the set from `state.routed.transcript`, which it
already holds, and converts the new termination into a named brace-stage refusal.

**What it buys.** The Giant Bat stops dying with `TerminalEvidenceMismatch`, an error that
reads as a broken engine, and starts refusing at the brace stage with the physics: which
terminal, which arm, how many millimetres inside the model, at what radius. The class becomes
countable across the corpus — which is the measurement that sizes the prize before anyone
builds the instrument. **And task #47 arrives for free:** the same refusal on a repair *trial*
is already a ledgered rollback, because `run_stage_chain` returns the context on a brace
refusal (`runner.rs:1405`) and `into_trial_rollback` (`runner.rs:1344`) turns it into one
ledger row.

**What it does not buy.** The Bat still does not emit. If Craig's "stops the hard failure"
means "the Bat emits", the smallest stage is A **and** B together, and §5 prices both.

**Files.** `crates/plinth-support/src/brace.rs`, `crates/plinth-support/src/forest.rs` (expose
the read-only closure query), `crates/plinth-support/src/runner.rs` (`brace_state`, one
`SupportBraceError` variant).

**Tests that pin today's behaviour, and how each moves.**

| test | pins | moves |
| --- | --- | --- |
| `brace.rs:2865` `a_refused_model_seated_arm_condemns_its_stub_instead_of_failing_the_run` | the cascade for an arm carrying a registered, consumed stub: `ContactRefused`, both members gone, a quarantine record on the arm | **Stays green unchanged** if it passes an empty routed-terminal set — which is the honest call, because the fixture has no sealed record. A second test is added beside it that supplies terminal 7 as sealed-routed and asserts the pass is abandoned with nothing quarantined. |
| `contact_regression_gate.rs:294` (structural needle `BraceFixedPointTermination::ContactRefused`) | OD-4: a refused contact must not hard-error the pipeline | **Stays green.** The variant remains and still carries the brace-rung and model-anchor cases the refutation §B carves out. |
| `contact_regression_gate.rs:324` `an_attestation_refusal_routes_to_quarantine_rather_than_a_pipeline_error` | the initial-build refusal path | **Untouched** — different stage, different path. |
| `runner.rs:7809`/`:7821`/`:7837` brace fixed-point unit tests | convergence, unplaceable, linked-plan outcomes | Signature-only change; pass an empty set. |

**Blast radius.** Models whose brace refusals disturb no contact accounting are **bit-identical**
— that must be proved by a corpus run, not asserted. Models that die at Structure today die at
Brace instead, with a better message; they were already dying. No digest moves, because nothing
that is folded into a digest changes value on a surviving model.

**Revert proof.** A test that condemns a carrier whose stub's terminal is sealed `Routed` and
asserts (a) the tree is untouched and (b) `finalized_unrouted_terminals` is never reached with
the illegal pair. If someone later "fixes" this by admitting a fourth pair, that test goes red.

### Stage B — the legalization retry

**What lands.** Three pieces.

1. **The exclusion channel.** `route_initial_cco_with_limits` (`runner.rs:3785`) takes a
   borrowed exclusion set. The tournament refuses an excluded candidate with a new named
   reason and **counts it out of `candidates_feasible`** — which is honest, because an
   excluded candidate genuinely is not feasible in that pass. A terminal whose candidates are
   all excluded retires with a new `NoWinnerEvidence` variant beside the three at `cco.rs:657`,
   carrying the arithmetic of §1.3.
2. **The abandon outcome.** `ChainOutcome` (`runner.rs:1298`) gains a third arm carrying the
   context back plus the exclusions the pass learned. The stage macro at `runner.rs:1374` is
   untouched; the new arm is produced only by the brace stage.
3. **The loop.** One `loop` wrapping the attempt-0 chain call (`runner.rs:1596`) and the trial
   chain call (`runner.rs:1874`), holding the exclusion set keyed by plan digest, restoring the
   five prepared digests on each abandoned pass exactly as the trial path already does at
   `runner.rs:1888`, and verifying each exclusion's witness before it is honoured.

**Reporting, per the no-cap doctrine.** `RepairRunReport` (`coverage_repair.rs:671`) gains
`legalization_passes`, `exclusions`, and per-terminal exclusion counts, printed on every path
and read by no branch.

**Files.** `cco.rs` (the exclusion parameter, the refusal reason, the evidence variant),
`runner.rs` (the outcome arm, the loop, the report fields), `coverage_repair.rs` (report
fields).

**Tests that pin today's behaviour, and how each moves.**

| test | pins | moves |
| --- | --- | --- |
| `runner.rs` `a_stage_error_is_not_discarded_at_attempt_zero` | an attempt-0 stage failure stays `Failed { phase, detail }` | **Half unchanged.** A brace refusal that is *legalizable* no longer reaches attempt-0 failure; every other stage failure does. The test gains a sibling that pins "an abandoned pass is not a failure". |
| the sealed-record digest goldens (the toed-slab digest at `support_runner_integration.rs:305`, `:580`, and `plinth-verify/tests/coverage_gate_toed_slab.json:1`) | a model that closes at attempt 0 is bit-identical | **Stay green.** A model with no exact-clearance refusal runs exactly one legalization pass with an empty exclusion set, and `candidates_feasible` is unchanged. |
| the CCO transcript digest (folds `candidates_generated` and `candidates_feasible`, `cco.rs:2393`–`:2411`) | the tournament's own accounting | **Moves only for models that actually exclude a candidate.** This is the design choice that avoids a corpus-wide re-freeze: no new digest field is added, and the existing feasible count carries the change. |

**Blast radius.** Deliberately narrow. Any model with zero exact-clearance refusals is
bit-identical by construction, and that must be proved by a corpus run. Models that do exclude
pay extra passes; the count and the wall time are printed. **The named risk, stated plainly:**
a model where the mask is unsatisfiable can exhaust a terminal's whole candidate set, ~100
extra `route → rewire → brace` passes for that terminal, ~16 s each on the Bat. That is a
measured defect-class cost, it is loud, and it is not capped. If it proves intolerable the
answer is #51's mask fix or a ruling from Craig — not a cap arriving by the back door.

**Revert proof.** A test with a fixture whose first-choice arm is exactly illegal and whose
second is legal, asserting: the first pass abandons, the second seals a different arm, the
final accounting shows `Routed`↔`Consumed`, and the run emits. Plus a witness-divergence test:
corrupt a witness, expect the named replay-divergence error, never a silent skip.

### Stage C — the run-invariant memo (§2.4 item 1)

`SurfaceAnchorPool::from_geometry` hoisted at **both** sites (`runner.rs:2686` and `:3383` —
take both or the win is silently halved), and the legacy pre-routing replay hoisted on the same
argument. Keyed on the geometry and configuration digests. Two obligations that must land with
it, not after it: the work charge becomes incremental with the memo rather than pricing a
fiction, and the receipt `charge.covers_execution(meter)` must still hold.

**Proof.** Every corpus artifact bit-identical, and a sabotage test: corrupt a stored value and
the artifact must change. Independent of A and B; can land in any order.

### Stage D — carry the legalization record into the evidence sidecar

The exclusion ledger and the pass count join the coverage refusal sidecar, and the corpus
counts are pinned. This is the schema move, and it should ride whatever other schema move the
A4 lane already owes rather than minting a second one.

---

## §5. THE DECISION MEMO — for Craig

*Self-contained. Read this alone if you read nothing else.*

### The dilemma, in two sentences

The routing stage seals a record saying a support point is routed, but the brace stage later
measures that support's true thickness and finds it inside the model, and condemning it makes
the sealed record and the live tree disagree — which the audit correctly refuses, killing the
Giant Bat. The refutation proved we cannot predict the brace stage's answer earlier, because
the number it depends on does not exist yet and moves in both directions, so we must either
weaken what "routed" promises or find a way to make the promise come true.

### The two coherent contracts

**Option 1 — two seals.** "Routed" is downgraded to "routing's decision". A second, final seal
is created after bracing, and only that one may be emitted from.

* *Result:* the disagreement stops being a contradiction, because the first seal no longer
  claims final admission.
* *Trade-off:* the load repartition has to run a second time after bracing, which the binding
  spec sequences before bracing — a spec deviation. Worse, the second repartition changes every
  support's load, which changes every thickness the brace stage just assigned, so the tree the
  second seal certifies is not the tree it measured. And it still does not fix the Bat: the Bat
  fails on its first pass, and there is no mechanism to try a second one.
* *Reversibility:* poor. The evidence sidecar and the verifier golden would carry two seals;
  backing out is a second format change.

**Option 2 — keep "routed" strict, and make it reachable (RECOMMENDED).** "Routed" keeps
today's meaning exactly. When bracing finds a routed support illegal, we throw that whole
planning pass away and route again with that exact arm excluded.

* *Result:* nothing is admitted late, no fourth pair is ever minted, the audit is untouched,
  and the Bat gets a second choice of arm. If a support point runs out of arms, it retires as
  "no winner" — which is one of the three pairs the audit already admits — and the run either
  emits with an honest hole or refuses with the hole named in millimetres.
* *Trade-off:* one extra route-and-brace pass per illegal arm. Measured on the Bat that is one
  arm per pass at about 16 seconds. The bad case is a support point where every arm is illegal
  — roughly 100 extra passes for that one point. That cost is printed, not capped.
* *Reversibility:* good. The first stage is one classification and reverts to today's bytes.
  The second stage is one loop and one parameter; delete the parameter and you are back at the
  first stage.
* *Why it is not the direction you already refused:* that direction asked us to **guess** the
  brace stage's answer while choosing arms. This one **measures** the answer exactly, then
  chooses again. No approximation, no bound, no new number.

### My recommendation, and the reason

**Option 2.** The reason is one sentence: **Option 1 spends the audit seal and still does not
fix the Bat, and Option 2 fixes the Bat while leaving the seal exactly as it is.** Every one of
the three accounting functions — the three-pair check, the removal at the point the spec names,
and the recompute-from-state tripwire — is unchanged, at every call site, under Option 2. Under
Option 1 all three change and one of them (the repartition) deviates from the binding spec.

Two supporting reasons. First, the retry loop is a **generalization of machinery the coverage
controller already has** — the context handback on refusal, the exclusion set that survives a
rollback, the digest restore, the counters that report and never decide — not a second parallel
mechanism; the one genuinely new surface is a parameter passed into the routing tournament.
Second, the loop has a **termination proof** rather than a hope: the exclusion set only grows
inside a finite, countable set of arms, so it cannot cycle, and running out of arms produces an
honest retirement rather than a failure.

### The cost band

I am giving you work shape, not calendar time.

**First stage (name the class, abandon instead of quarantining):** three files
(`brace.rs`, `forest.rs`, `runner.rs`), one added parameter, one read-only query that already
exists as a private walk, one new named outcome. Two new tests, one existing test gains a
sibling. **No digest moves, no golden moves, no schema moves.** One corpus run to prove
bit-identity on every model that is not affected. This is the small, safe, revert-proof one.

**Full plan (first stage plus the retry loop, plus the invariant-work memo and the sidecar
record):** four files plus the evidence sidecar. The genuinely hand-written seam is the
exclusion channel into the routing tournament and the witness check — the same class of work as
the context-handback seam the A4 lane already did, and like that one it should not be called
mechanical. The transcript digest moves **only for models that actually exclude an arm**, by
design, so there is no corpus-wide re-freeze. Two corpus runs: one to prove bit-identity for
unaffected models, one to measure the extra pass count and wall time on the Bat, the Warhorse
and the treant. The named cost risk is the extra passes on a model whose clearance mask is
unsatisfiable, and the answer to that is task #51, not a cap.

### What I am asking you to authorize

1. **The contract.** Sealed `Routed` keeps its current final-admission meaning. Option 1 is
   declined. Say yes, or name the other one.
2. **Stage A on its own, first.** Land the classification and the named brace-stage refusal
   with no retry, prove bit-identity on the corpus, and count the class across all 45 models
   before building anything else. This is the "size the prize before building the instrument"
   step.
3. **Stage B after A's measurement, not before it.** The retry loop, with the cost printed and
   uncapped. I would like your explicit agreement that the bad case — a support point that
   exhausts every arm at roughly 100 extra passes — is reported loudly and **not** capped,
   because that is your no-cap doctrine applied to a number I expect someone to want to cap.
4. **One thing I am NOT asking for**, so it is on the record: no new registered constant, no
   fourth admitted pair, no second removal point, and no resurrection of the ranking rule.

**One thing I want to flag rather than decide.** The design assumes the routing cadence is
deterministic given the tree state — that is what makes an exclusion mean the same thing on
replay. I do not trust that assumption; the design checks it with a witness on every exclusion
and turns a mismatch into a named, hard error. If you would rather the assumption be proved
before the loop is built, that is a separate, cheap measurement (route the same plan twice,
compare the transcripts bit for bit) and I will run it first on your word.

---

## §6. What this record does NOT claim

* **Not** that the Giant Bat emits. It claims the Bat reaches an emission or a disclosed
  refusal instead of an accounting-integrity error, and that which one is a measurement.
* **Not** that the refutation is wrong. It accepts §C entirely: the brace verdict cannot be
  predicted when candidates are chosen. It uses the measured verdict instead of predicting it.
* **Not** that the 89 rejected trials can be saved by storing results. §2 argues the opposite,
  precisely.
* **Not** that Option 1 is incoherent. It is coherent and Codex is right about its shape; it is
  declined on cost, on the spec deviation it forces, and because it does not fix the Bat alone.
* **Not** that the extra passes are cheap. §4's named risk is real and unmeasured.
* **Not** a proposal to admit a fourth pair, add a second removal point, move any registered
  number, or resurrect the ranking rule.

## §7. Open items and escalations

| # | item | owner |
| --- | --- | --- |
| S-1 | Is the routing cadence bit-reproducible on a replayed identical plan? One cheap measurement, and it should precede Stage B. | this lane, on Craig's word |
| S-2 | Stage A's corpus count: how many of the 45 models hit an accounting-disturbing brace refusal? Unknown today. | Stage A |
| S-3 | The extra-pass cost on a mask-unsatisfiable model. Depends on #51's landing state. | Stage B Prove |
| S-4 | Task #47 is discharged by Stage A for the trial path; its title should be re-read once Stage A lands. | #47 |
| S-5 | No new registered constant is needed anywhere in this plan. If an implementer wants one, that is an escalation to Craig, not a design detail. | binding on all stages |
