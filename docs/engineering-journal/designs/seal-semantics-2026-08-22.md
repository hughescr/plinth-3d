# What sealed "Routed" must mean — the Routing/Brace seam (task #36, E-E)

> **Provenance.** Written 2026-08-22 against `feature/a4-controller` @ `5a8ade4`; **revised
> 2026-08-22 at `7e668e9`** to fold an adversarial review (verdict SOUND-WITH-CHANGES,
> findings C1–C11). Main checkout
> `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, working tree clean. Every `file:line`
> below was read at `7e668e9`, including the ones the review corrected. **Design only. No
> production code, no test code, and no constant is changed, added, moved or read differently
> by this record.**
>
> Evidence labels: **MEASURED** (a number a run produced), **DERIVED** (closed form from
> MEASURED or registered inputs), **INFERRED**, **ASSUMED**.
>
> Inputs this record answers to, in order: `candidacy-exact-mask-2026-08-21.md` (the #36
> refutation), the independent Codex opinion on this seam, the adversarial review of this
> record's first revision, the MEASURED waste of `wf_3b724805-678`, and `a4-controller-design.md`
> + `model-seated-arms-2026-08-21.md` with its amendment.
>
> **What the revision changed, so a reader of the first version is not misled.** The exclusion
> key moved from a tournament position to the context digest tuple the record already carries
> (§1.3, finding C1) and a stale key is now a **miss**, not an error; the termination argument
> is restated as finite backtracking with a combinatorial and unmeasured bound rather than a
> linear walk (§1.3, C1/C2); the per-pass cost is re-derived and corrected (§0, C2); the
> legalization loop is confined to **attempt 0 only** (§1.4, C3); a **third** direction —
> constrain the resize — is stated and argued down rather than omitted (§1.5, C4); the measured
> margins and radii move **inside** the sealed evidence (§1.3, C5); the stage order becomes
> **A → C → B** (§4, C6); two anchors are corrected and one "existing field" is re-labelled as
> new plumbing (§2.4/§4, C7); Stage A classifies on any sealed terminal and calls a planner, not
> a re-implemented walk (§4, C8); and the `candidates_feasible` overload is named as a choice
> Craig makes rather than an implementation detail (§4/§5, C9).

---

## §0. The answer in one page

**The defect.** The routing stage seals a record that says terminal 298 is routed. The brace
stage then measures that terminal's arm against the model with the arm's true radius, finds
it inside the model, and condemns it. Condemning it flips the terminal's live state to
`Quarantined` while the sealed record still says `Routed`. That pair is not one of the three
the accounting admits, so the run dies at the structure stage with
`TerminalEvidenceMismatch`. **MEASURED: the Giant Bat, exit 5, emitted=0, terminal 298 on the
drilled shell and terminal 187 on the substituted solid.**

**Where it dies, and this is new here.** MEASURED from `candidacy-out/runs/tip-Giant-Bat.log`
(28 lines): the stage stream is `Preparation → Routing → Rewire → Brace → Structure` **once**
per sealed support run, and there are two sealed support runs, each with exactly one
`Preparation`. The decisive evidence is an **absence with a known cause**: there is no
`CoverageMeasurement` line, and `runner.rs:1603` prints that line immediately after the
attempt-0 chain returns and before any repair trial can begin. Its absence therefore means the
attempt-0 chain never returned successfully. (Repair trials print no stage lines at all by
construction — they pass `None` for the clock at `runner.rs:1874` — so "no trial lines" would
have proved nothing; the missing `CoverageMeasurement` line does.) The result line
independently confirms it: `failed during Structure: TerminalEvidenceMismatch`. The Bat
therefore dies on **attempt 0 — the model's own baseline plan, before any repair contact
exists.** The A4 controller's try/measure/keep-or-roll-back machinery cannot reach it, by that
machinery's own rule (`runner.rs:1601`: an attempt-0 stage refusal is `SupportRunError::Failed`
via `RefusedStage::into_attempt_zero_failure` at `runner.rs:1332`, not a rollback). Any fix
that is only a controller fix does not touch the Bat.

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

**Why it terminates — stated honestly, because the first version of this record overstated
it.** Each abandoned pass records one exclusion, and an exclusion is keyed on the **exact
context** the tournament read: the digest tuple `TerminalRouteEvidence` (`cco.rs:642`) already
carries per terminal — `plan_digest`, `loads_digest`, `geometry_digest`,
`forest_digest_before`, `controller_digest_before` — plus the candidate's own identity. If the
context differs, the exclusion **does not match and does not apply**; the candidate is
regenerated and re-measured from scratch. A stale key is a miss, never an error and never a
silent hit. Each abandoned pass adds a `(terminal, context, candidate)` triple that was not in
the set before, and the reachable set of such triples is finite (MEASURED on `zombie-3-a`:
42–106 arms proposed per terminal, over a finite cadence of terminals). So the search
terminates. **But it is a backtracking search over a tree of contexts, not a walk down a list:
when two terminals fail in the same pass, the passes multiply rather than add.** The bound is
finite, combinatorial, and **not measured**. Stage A's corpus count is the first measurement
of how often more than one terminal is involved. When one terminal's candidate set is used up
**in the context it is standing in**, that terminal retires as `UnroutedNoWinner` — an admitted
pair — and the pass completes.

**What the loop can and cannot do to a run.** It adds **no new way to fail a run** for the
class it is built for: a legalizable brace refusal either converges or converts into an
honestly unrouted terminal. It names one refusal that is a **hard, unnamed engine error
today** — a condemnation that reaches a sealed terminal which is not `Routed`, where there is
no candidate to exclude and therefore no next pass to try (§1.3, "The one refusal the loop
keeps"). The first version of this record claimed flatly that "the retry loop has no give-up
state of its own"; that sentence is deleted. What is true is narrower and is stated wherever
the claim used to appear.

**What it costs.** One extra `route → rewire → brace` pass per refused arm. MEASURED on the
Bat, one refused arm per pass. **One pass is 17.8 s on the drilled shell and 15.1 s on the
oriented solid**, DERIVED from the log's cumulative stage clock:

| | drilled shell | oriented solid |
| --- | --- | --- |
| entering Preparation | 3 ms | 2 ms |
| entering Routing | 1,335 ms | 1,914 ms |
| entering Rewire | 9,048 ms | 9,358 ms |
| entering Brace | 16,401 ms | 14,177 ms |
| entering Structure | 19,112 ms | 17,012 ms |
| **Routing** (duration) | **7.71 s** | **7.44 s** |
| **Rewire** (duration) | **7.35 s** | **4.82 s** |
| **Brace** (duration) | **2.71 s** | **2.84 s** |
| **one legalization pass** = Routing + Rewire + Brace | **17.78 s** | **15.10 s** |
| Routing + Rewire as a share of one pass | 85 % | 81 % |

A legalization pass repeats Routing, Rewire and Brace. It does **not** repeat Preparation: the
plan does not change, so the prepared digests are restored rather than recomputed. Preparation
is therefore excluded from the per-pass figure (it is 1.33 s on the shell, 1.91 s on the
solid). **A note on a number that is easy to get wrong:** 16.4 s is the span from entering
Preparation to entering Brace on the shell. It counts a stage the pass does not repeat and
omits a stage it does, so it is not a per-pass figure. Use 17.8 s and 15.1 s.

The worst case is **not** "one terminal's candidate set is refused". That case is one terminal
using up its arms — up to ~100 passes, where 100 is the largest MEASURED arm count at one
terminal and it was measured on a **different** model (`zombie-3-a`, 42–106 arms per terminal),
not on the Bat. The real worst case is several terminals failing in one pass, where the passes
multiply over the backtracking tree. That number is finite and unmeasured. It is reported
loudly rather than capped, per the no-cap doctrine, and Stage A's corpus count is what sizes it.

---

## §1. The contract question

### §1.1 What sealed `Routed` means today, read from the code

Three functions carry the whole meaning, and all three were read at `7e668e9`.

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
arm excluded **in that exact context**.

**What changes in the accounting: nothing in the code.**

| site | change |
| --- | --- |
| `finalized_unrouted_terminals` (`runner.rs:5652`) | **Unchanged.** Three pairs, same catch-all error, same cardinality check. It becomes a tripwire that should never fire, because the brace stage refuses first. |
| `accepted_contact_set_after_routing` (`runner.rs:4346`) | **Unchanged.** One removal, one repartition, at the point SPEC §6 names. |
| `validate_accepted_contact_set` (`runner.rs:4242`) | **Unchanged**, all three call sites. |

**The #49 tripwire, re-run under this contract.** Regression #49 is "unrouted regular contacts
are never removed from the accepted set". Under Candidate 2 `accepted_contact_set_after_routing`
is byte-identical at its one call site (`runner.rs:3935`), and `validate_accepted_contact_set`
— which re-derives the removal ledger from the sealed record and the live tree rather than
trusting the stored value — is byte-identical at all three sites (`:5411`, `:5834`, `:5904`).
A regression that stopped removing unrouted regular contacts would be re-derived, compared and
refused exactly as it is today, by the same check, at the same three places. Under Option 1
that tripwire protects half of what it protects today unless it is built twice.

**But "nothing changes in the accounting" is true of the code and NOT true of the meaning, and
the difference is a real audit cost.** Today `UnroutedNoWinner` means "this pass's tournament
produced no winner". After the retry loop lands it can also mean "no winner survived once `k`
arms had been excluded by earlier passes of this plan". Same row, same
`UnroutedContactReason::NoWinner`, same SPEC §6 removal, same load repartition — but a **new
upstream way to produce it**. A defective exclusion (a wrong candidate identity, a key honoured
against a context it was not measured in) would remove a real contact from the accepted set and
repartition its carried area onto its neighbours, and every existing check would agree, because
every existing check tests **consistency**, not whether the exclusions were **earned**. That is
a new print-safety hole — fewer supports, silently, under a clean seal — where today a spurious
retirement can only come from routing itself. The answer is in the verifier obligations below:
the exclusions must be backed by measured millimetres inside the thing the seal reads, not by a
count and not by a sidecar that lands two stages later.

**What happens to `brace.rs:523`'s cascade.** It splits, and the split is decided by the
sealed record, not by a member's kind:

* The sweep's condemnation set is computed first, read-only, by the **planner half of the
  mutation itself** — not by a second walk that reproduces it (see §4 Stage A(b)).
* If that set contains **no** terminal the sealed record names, the sweep is applied exactly as
  today. This is the refutation's own carve-out (§B): brace rungs and model-anchor arms whose
  condemnation disturbs no contact accounting. **Their behaviour does not move.**
* If it contains a terminal the record sealed as **`Routed`**, nothing is quarantined. The pass
  is abandoned and the refusal travels out as evidence: for each disturbed terminal, the arm its
  tournament chose, the measured clearance margin in millimetres, and the radius the resize
  assigned.
* If it contains a terminal the record named in **any other** admitted pairing, nothing is
  quarantined and the pass refuses with a named brace-stage error. See "The one refusal the loop
  keeps", below.

**What the verifier must then enforce.** Two additions:

1. A terminal that retires `UnroutedNoWinner` because every candidate was refused by the exact
   post-brace check must show it **as measured physics, not only as arithmetic**. The new named
   variant beside the three that already exist in `NoWinnerEvidence` (`cco.rs:657`) carries, per
   excluded arm, **the MEASURED clearance margin in millimetres and the radius the resize
   assigned**, and the arithmetic that candidates refused by exact clearance plus candidates
   refused earlier accounts for candidates generated. Arithmetic alone is not enough:
   *arithmetic over a stale universe adds up perfectly*. With the margins inside the evidence,
   "exhausted" is backed by `k` measured negative numbers. This is the standard the refutation
   §F.1 held itself to, and it is why the margins belong **inside the sealed evidence** rather
   than in Stage D's sidecar — Stage D becomes disclosure of a record that already exists,
   rather than the only record.
2. A run that emitted must report its legalization pass count, and that count must equal the
   number of recorded exclusions plus one **for a single-terminal series**; for a multi-terminal
   backtracking series the invariant is the weaker and correct one: **every abandoned pass added
   at least one exclusion that was not already in the set** (see "The abandon rule", below). A
   pass that was abandoned may never reach output — true by construction here, because an
   abandoned pass produces no lowered run and therefore no sealed authority.

Everything else on Codex's list is **already enforced** and needs nothing: complete and
exclusive terminal accounting (the three-pair table plus the cardinality check), immutable
digests and lineage (`extend_contact_attestation_chain`, and the five prepared digests
restored on every rollback at `runner.rs:1888`), and "no selected terminal silently
disappears" — that is precisely the error the Bat raises today.

**The exclusion, and why it is exact rather than a guess.** An exclusion names one
`(terminal, context, candidate arm)` triple. It does **not** say "this arm can never work";
Codex is right that such a claim is unsound, because the same arm might be legal under a
different shared path. It says only: *in this exact tree and controller state, this candidate
was measured illegal at its true radius, and it will not be chosen again here.*

**Keying it, which is the one genuinely delicate part, and where the first version was wrong.**

MEASURED, from the refutation §C5: member slot numbers are **not stable across passes** (the
same −0.040503 mm strut is slot 486 in one pass and slot 410 in another, with identical
endpoints). So the exclusion cannot be keyed on a slot.

The first version keyed it on the terminal's identity plus **the candidate's position in that
terminal's tournament**, and justified the position by an induction: the cadence replays in the
same order, so a terminal's tournament is bit-identical as long as every terminal before it is
unchanged. **That induction's own precondition is what an exclusion breaks.** Excluding an arm
at terminal `m` makes `m` choose a different arm; every terminal after `m` then routes against a
different forest, so their candidate lists are different lists, and "position 3" names a
different arm — or no arm. Two ordinary paths reach this, both inside the class this design
exists for:

* **Several exclusions from one sweep.** One brace sweep can condemn several terminals. Excluding
  at `m` and at `n > m` in the same pass invalidates `n`'s position key on the very next pass.
* **A retirement is not neutral.** When a terminal retires, its stub is not built, so the braced
  forest changes, so `transient_report`'s path-level worst spans change, so
  `resize_forest_current_parity` assigns different radii to **other** arms. This is the same
  global coupling this record uses against Option 1's second repartition (§1.2). Candidate 2 has
  the coupling too. Candidate 2's honest defence is that it **re-measures** after the coupling
  fires and Option 1 does not — a good defence, and it is stated here rather than hidden, but it
  does not rescue a position key.

Under a position key a stale key would be the **expected** outcome, not an exotic one, and the
first version made a witness mismatch a hard named error — so the loop would have failed runs
routinely, and a position-keyed exclusion would have refused an arm in a context where the
physics was never re-measured. That is a cap in physics costume, on Codex's forbidden list ("no
context-free exclusion derived from a context-dependent failure") and on Craig's ("no count or
state deciding an outcome").

**The fix is this record's own §2.3 rule applied to this record's own key, and the material is
already in the tree.** `TerminalRouteEvidence` (`cco.rs:642`, the struct directly above
`NoWinnerEvidence`) already carries, per terminal disposition:

```
physical_contact_id, terminal_id, original_key,
plan_digest, loads_digest, geometry_digest, forest_digest_before, controller_digest_before
```

Key each exclusion on:

> `(terminal_id, plan_digest, loads_digest, geometry_digest, forest_digest_before,
> controller_digest_before, candidate identity)`

Then:

* **A stale exclusion misses. It does not apply, and it is not an error.** That is literally
  §2.3's rule: *a digest mismatch is a miss; it can never be a silent stale hit.* The candidate
  is regenerated and re-measured from scratch, so no stored answer is ever used in a situation
  where it was not measured.
* The no-good becomes **context-bound**, which is the shape Codex asked for and the shape the
  doctrine requires.
* **The Bat is unchanged.** One terminal failing repeatedly against an unchanged prefix produces
  identical contexts pass after pass, so §3's trace holds verbatim.
* Termination becomes a finite backtracking search over reachable `(context, choice)` states —
  sound, but combinatorial. Stated below, and priced in §0.

**Candidate identity is new plumbing, and the cost band must say so.** `CcoCandidateEvidence`
(`cco.rs:605`) is `{ family, generation_index, price_ml, clearance_rows }`. **There are no
endpoints in it.** The identity an exclusion needs is the arm's endpoint coordinates by exact
bit pattern — refutation §C5 ("cite endpoints, not slots, across runs") is precisely why — and
that field does not exist today. It is hand-written work in `cco.rs`, not a field that is
already there for free.

**The abandon rule, which is what makes the growth argument true.** A pass may be abandoned
**only if it carries at least one exclusion that was not already in the set**. Everything else
refuses by name. This is not bookkeeping hygiene; it is the invariant the termination argument
stands on. Without it, a pass that abandons while learning nothing would replay identically and
abandon identically, forever. With it, the case that learns nothing is the one named refusal
below.

**Termination, stated honestly.** Let a **context** be the digest tuple above at the moment a
terminal routes. A pass is a deterministic function of the plan and the exclusion set. Each
abandoned pass adds at least one `(terminal, context, candidate)` triple that was not in the set
— a triple is only added for a candidate that was **chosen**, and an excluded candidate is never
chosen at that context again, so no triple is ever added twice. The reachable triples are finite:
the cadence has finitely many terminals, each terminal proposes finitely many candidates
(MEASURED: 42–106 on `zombie-3-a`), and each context is generated by the finitely many choices at
earlier terminals. A strictly growing subset of a finite set cannot cycle, so **the search
terminates**.

What the first version got wrong is not whether it terminates but **by how much**. This is a
depth-first backtracking search over a tree of contexts, and its bound is a product across the
failing terminals, not the sum `|U|`. **Passes multiply, they do not add.** The bound is finite,
combinatorial and unmeasured; Stage A's corpus count is the first measurement of how many
terminals are involved at once, which is the exponent.

When a terminal's candidate set is used up **in the context it is standing in**, that terminal
retires `UnroutedNoWinner` rather than abandoning the pass, so the pass completes.

**The one refusal the loop keeps, stated because the blanket claim is deleted.** The sentence
"the retry loop has no give-up state of its own" is **removed from this record**. What is true:

* A brace refusal of a **sealed-`Routed`** terminal always yields a new exclusion, so it always
  abandons and always makes progress. This class — the Bat's class, the class the design exists
  for — gains **no new way to fail a run**.
* A condemnation that reaches a terminal the sealed record named in a **non-`Routed`** pairing
  has no chosen candidate to exclude. It cannot abandon under the abandon rule. It refuses, by
  name, at the brace stage. **This is strictly better than today**, where the same case reaches
  `require_active` inside `quarantine_subtrees` (`forest.rs:983`/`:990`) and the `?` at
  `brace.rs:523` turns it into a hard `BraceError` that reads as a broken engine (§4 Stage A(a)).
* The honest residual exposure is therefore **wall time, not failure**: a finite but
  combinatorially large number of passes on a pathological model. That is what is printed and
  not capped.

**No cap, and no counter deciding an outcome.** The loop stops when the physics stops producing
refusals, or when a terminal's declared candidate set is used up in its context. Both are
measured events. The pass count, the exclusion count and the wall time are printed and decide
nothing — the same rule `RepairRunReport` (`coverage_repair.rs:671`) already follows.

**Reversibility.** Good. Stage A (§4) alone is one classification and one named outcome, and
reverting it restores today's bytes. Stage B is one loop and one parameter; deleting the
parameter restores Stage A exactly.

### §1.4 The fit check: a generalization, or a second mechanism?

Craig asked this directly. The answer is **a generalization, with two genuinely new surfaces**,
and here is the line-by-line.

**Where the loop runs, stated first because everything else depends on it.**

> **The legalization loop runs at ATTEMPT 0 ONLY. It is never wrapped around a repair trial. A
> trial's brace refusal remains exactly what it is today: `restore_prepared_digests`
> (`runner.rs:1888`), `into_trial_rollback` (`runner.rs:1344`), one ledger row, next candidate.**

The first version of this record said in one place that the loop wraps "both" the attempt-0 chain
call (`runner.rs:1596`) **and** the trial chain call (`runner.rs:1874`). That was wrong and is
withdrawn. Three reasons, in order of force:

1. **Every trial changes the plan digest.** An exclusion is keyed on the context tuple, and
   `plan_digest` is in it. A trial appends a repair contact, so every trial starts with an
   exclusion set that matches nothing. The loop would therefore pay a full legalization walk per
   trial and carry nothing forward.
2. **The arithmetic is absurd.** MEASURED on `zombie-3-a`: 92 trials. At up to ~100 passes each
   and 17.8 s a pass, that is ≈ 45 hours for one model. It would also land inside
   `wall_time_per_trial_max_ms` (`coverage_repair.rs:685`) — the instrument the A4 lane built
   specifically to see one pathological trial — and bury it.
3. **A trial is discretionary work.** The cheapest correct action for a repair contact that
   cannot be legalized is to drop it, and that is what a ledgered rollback does. Wrapping
   `runner.rs:1874` would make discretionary work obligatory. The seam is
   `runner.rs:1874`–`:1891`, and it stays as it is.

This is also what makes the two-loop picture below true rather than decorative.

**What the A4 controller already has, that the retry loop reuses unchanged:**

| the controller has | the retry loop needs | same thing? |
| --- | --- | --- |
| `run_stage_chain` hands the context back on every stage refusal (`ChainOutcome::Refused { context, stop }`, `runner.rs:1298`–`:1409`) | a pass that abandons must hand the context back intact | **Yes.** The abandon path is that path with a payload. |
| `RepairLedger::ledgered` — a set of tried rows that deliberately survives a rollback (`runner.rs:2007`) | a set of excluded candidates that survives an abandoned pass | **Yes**, same idiom, different key. |
| `context.restore_prepared_digests(digests)` on every rollback (`runner.rs:1888`) | the same, on every abandoned pass | **Yes**, identical need, identical call. |
| counters that report and never decide (`RepairRunReport`, `coverage_repair.rs:671`) | pass count, exclusion count, wall time | **Yes**, the new counters go in the same report. |
| a monotone exclusion set over a finite pool | a monotone exclusion set over a finite universe of `(terminal, context, candidate)` triples | **Same idiom, weaker bound.** The controller's pool is walked; this one is searched with backtracking. |

**What it does not have, and this is the honest new work:**

1. **A retry at attempt 0.** `run_support` calls the stage chain once for the baseline plan
   (`runner.rs:1596`) and converts any refusal into `SupportRunError::Failed` via
   `RefusedStage::into_attempt_zero_failure` (`runner.rs:1332`). The retry wraps **that call
   only**. That is one new `loop` around an existing call — not a new subsystem.
2. **A channel for exclusions into the tournament.** `route_initial_cco_with_limits`
   (`runner.rs:3785`) gains one borrowed parameter.
3. **A stable candidate identity.** The arm's endpoints by exact bit pattern, recorded beside
   `CcoCandidateEvidence` (`cco.rs:605`), which does not carry them today. New plumbing, priced
   as such.

**The two exclusion sets have different lifetimes, and confusing them would be a defect.**
`ledgered` is per **run**: SPEC §6 says exact failed rows stay excluded across commits.
Exclusions are per **plan and per context**: the context tuple carries `plan_digest`, so a
plan change makes every exclusion miss automatically. Because the loop never wraps a trial,
this is belt and braces rather than the only guard — but it is stated so no implementer
carries an exclusion across a commit by accident.

So the shape is two loops with clean, different jobs, one **beside** the other rather than one
inside the other: the outer controller tries **different plans** (unchanged); the legalization
loop legalizes **the baseline plan** before the controller ever sees it (new). They share the
context handback, the digest restore and the report.

### §1.5 Candidate 3 — constrain the resize (the refutation's own preferred direction), and why it is declined

The refutation `candidacy-exact-mask-2026-08-21.md` §E names three directions, and closes:
*"The lane's own reading is that (1) is the only one that keeps both the seal and the
no-approximation rule."* Direction (1) is **not** the one this record recommends, and the first
version of this record omitted it. It is stated here so the choice is a genuine three-way one.

**The contract.** Leave routing, the seal and the accounting entirely alone. Instead, refuse the
**resize** that drives a contact-declaring member into the model: `resize_forest_current_parity`
(`transient.rs:428`) would be constrained so that a member which declares a model contact may
not be assigned a radius that its clearance cannot hold. This is task #25's resize-timing family.

**Why it is attractive.** It touches no seal, mints no pair, changes no accounting function,
and needs no retry loop at all. If it worked it would be the smallest possible answer.

**Why it is declined, in one paragraph.** The MEASURED evidence says the resize is not doing
anything wrong. On the drilled shell, slot 377's radius **grew** 0.949212 → 1.409608 mm, and it
grew **because the braced path demanded it**: the path-level worst span sets the radius, and that
is a strength requirement. Refusing the growth does not remove the defect; it **trades** a
clearance defect for an under-thick member — a strength defect for a geometry one — and the
strength defect does not announce itself. It prints, and it snaps. That is the worse of the two
trades on a support that carries load. And the constraint does not even cover the class: on the
oriented solid, slot 250's radius **shrank** 1.048241 → 0.901736 mm and the arm still failed the
exact clearance check. "Do not let the radius grow" says nothing about an arm that got thinner
and was still inside the model. A rule that fixes one of the two MEASURED cases by accepting a
silent strength defect, and does not address the other case at all, is not the answer here.
Direction (1) may still be right **as its own task** for the resize-timing family — this record
declines it as the answer to the Routing/Brace seam, not as a bad idea in general.

**Reversibility.** Not assessed. The direction is declined before costing, on the grounds above;
that decision is Craig's to overturn, and §5 puts it in front of him.

### §1.6 The three candidates side by side

| | Candidate 1: two seals | Candidate 2: strict, made reachable | Candidate 3: constrain the resize |
| --- | --- | --- | --- |
| meaning of sealed `Routed` | routing's decision only | unchanged: final admission | unchanged: final admission |
| fourth pair | never minted (a second seal instead) | never minted | never minted |
| `finalized_unrouted_terminals` | splits in two | unchanged | unchanged |
| `accepted_contact_set_after_routing` | provisional; a second removal after bracing | unchanged | unchanged |
| carried-load repartition | **twice**, and the second one is a SPEC §6 deviation | once, where §6 says | once, where §6 says |
| `validate_accepted_contact_set` | three call sites must choose a set; tripwire halves unless rebuilt twice | unchanged | unchanged |
| `brace.rs:523` cascade | stays whole, becomes the second stage's removal | splits by what the sweep would condemn | untouched |
| verifier | new attestation, new lineage link, golden moves corpus-wide | measured margins inside the sealed no-winner evidence; one arithmetic invariant | untouched |
| fixes the Bat on its own | **no** — still needs the retry loop | **yes** | **half**: fixes the grew-and-failed case by accepting a silent strength defect; does not address the shrank-and-failed case |
| what it costs | a spec deviation and an unstable second seal | extra passes, finite but combinatorial and unmeasured | a strength defect that does not announce itself |
| reversibility | poor (format and golden moves) | good (one classification, then one loop) | not assessed |

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

**This rule binds this record's own exclusion key.** The first version of §1.3 keyed exclusions
on a tournament position and made a mismatch a hard error — breaking both halves of the rule
above. §1.3 now keys them on the digest tuple `TerminalRouteEvidence` (`cco.rs:642`) already
carries, and a mismatch is a miss. A design record that states a rule and then violates it two
sections later is worse than one that never stated the rule.

### §2.4 Where the real savings are — three of them, all doctrine-clean

1. **Run-invariant work rebuilt inside every trial.** `SurfaceAnchorPool::from_geometry`
   depends only on `geometry` and `delicacy`, both fixed for the whole run, and is built at
   **two** production sites — `runner.rs:3844`, inside `route_initial_cco_state` (the
   **Routing** stage), and `runner.rs:4568`, inside `rewire_state` (the **Rewire** stage).
   (The first version cited `runner.rs:2686` and `:3383`; at `7e668e9` line 2686 is blank space
   inside the `RoutedLogicalRoute`/`RoutedRouteProvenance` enums and line 3383 is
   `#[cfg(test)] transit_test_stale_digest`. The claim was right and the anchors were wrong;
   correcting them is what makes §4's ordering argument visible, because it names the two
   stages.) MEASURED: a 494,617-row build per trial on the toad, paid twice. The legacy
   pre-routing replay adds ~3 `validate_binding` calls per trial, each re-running a full
   baseline selection and a coverage-sample scan; MEASURED at 1,383 executions across the
   45-model sweep. **These are the same function on arguments that provably never move.**
   Keying them on the geometry and configuration digests is memoization in the strict sense: it
   decides nothing, changes no verdict, and a sabotage test (corrupt the stored value, expect a
   different artifact) proves it.
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
   result, not a design claim. Note that the 52 are **trials**, and the legalization loop does
   not run on trials (§1.4); what changes for them is Stage A's classification, which makes a
   trial's disturbing brace refusal a named ledgered rollback instead of a hard error.

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

1. **Preparation.** Unchanged, and run once. Legalization passes do not repeat it.
2. **Routing, legalization pass 1.** The exclusion set is empty. Terminal 298's tournament
   proposes its candidates; the winner becomes the arm at slot 377. The record seals
   298 ↔ `Routed` with its context tuple (`plan_digest`, `loads_digest`, `geometry_digest`,
   `forest_digest_before`, `controller_digest_before`).
   `accepted_contact_set_after_routing` finds nothing to remove for 298.
3. **Rewire.** Unchanged.
4. **Brace.** `resize_forest_current_parity` (`transient.rs:428`) assigns the arm
   1.409608 mm from the braced tree's path-level worst span. `exact_clearance_negatives`
   (`brace.rs:1854`) measures −0.160653 mm. The arm lands in `contact_refusals`.
5. **The new step.** The condemnation set is planned read-only. It contains the arm **and**
   terminal 298's stub, because a condemned carrier takes its dependant with it. Terminal 298
   is sealed `Routed`, so this refusal disturbs contact accounting. **Nothing is quarantined.**
   The pass is abandoned, carrying one exclusion: terminal 298's context tuple, the winning
   candidate's endpoint identity, its −0.160653 mm margin and its 1.409608 mm radius. The
   abandon rule is satisfied — this exclusion was not in the set.
6. **Legalization pass 2.** The whole chain runs again on the same plan. Terminal 298 routes
   against an unchanged prefix, so its context tuple is **the same tuple**; the exclusion
   therefore matches and applies. Its tournament regenerates, the excluded candidate is refused
   with the named reason, and the next-best feasible candidate wins. The record seals
   298 ↔ `Routed` on a **different** arm.
7. **Outcome.** Either the brace stage is clean — and the run proceeds to Structure, Lowering
   and the coverage measurement — or another arm is refused, another exclusion is added, and
   step 6 repeats. The search terminates (§1.3).

**What would happen if a second terminal also failed** — the case the first version of this
record did not price. Say terminal 298 and terminal 512 are both condemned in pass 1. Both are
excluded. In pass 2, 298 picks a different arm, and that changes the forest 512 routes against,
so 512's context tuple is **different** — its exclusion **misses**, and its arm is regenerated
and re-measured from scratch. That is the correct behaviour, and it is also why the passes
multiply: the search explores combinations of choices rather than walking one list. It still
terminates, because no `(terminal, context, candidate)` triple is ever excluded twice.

**Every terminal state the Bat can reach, and the accounting rows each writes.**

| # | state | how it is reached | accounting rows written |
| --- | --- | --- | --- |
| **T1** | **Emits.** | Legalization converges; Structure and Lowering pass; the coverage measurement finds no blocking patch, or the A4 loop closes it inside the committed-step bound. | Record row 298 = `Routed`, ledger = `Consumed` — the first admitted pair. `finalized_unrouted_terminals` returns 298 in neither list. `accepted_contact_set_after_routing` writes no removal for it. The report carries `legalization_passes` and `exclusions`, with each exclusion's terminal, context tuple, margin and radius. |
| **T2** | **Emits with terminal 298 honestly unrouted.** | Terminal 298 uses up its whole declared candidate set in the context it is standing in; every one was refused by the exact post-brace check. | Record row 298 = `UnroutedNoWinner` with the new evidence variant — `candidates_generated`, `refused_by_exact_clearance`, the arithmetic that they account for each other, **and the measured margin in millimetres and assigned radius for every excluded arm** — ledger = `Retired`, the third admitted pair. `accepted_contact_set_after_routing` writes one `UnroutedRegularRemoval` row (contact id, terminal id, route, reason, carried area) and the single batched removal runs. `NoncoverageVector.unrouted_no_winner` rises by one. The coverage measurement then judges the hole; if it is below the 0.16 mm² censor or covered by neighbours, the model seals and emits. |
| **T3** | **Honest coverage refusal.** | As T2, but the hole is above the censor and the repair loop cannot close it. | Everything T2 writes, plus `SupportRunError::CoverageClosure` carrying the per-route outcome `Blocked(ClearanceMaskUnsatisfiable { terminals, best_shortfall_mm, standoff_mm })`, the trial stream and the legalization ledger. Exit is non-zero and nothing is emitted — **but every number is named**, which is what today's `TerminalEvidenceMismatch` does not do. |
| **T4** | **A genuine clearance violation.** | The refused member declares **no** model contact, so `pipeline_violations > 0` at `brace.rs:554`. | Unchanged from today: `BraceError::ClearanceViolation`. This path is not touched. |
| **T5** | **Some other stage fails.** | For example Lowering's `RootRunTooShort` (task #29). | Unchanged: `SupportRunError::Failed { phase, detail }` on attempt 0, or a ledgered rollback on a trial. |
| **T6** | **A named brace refusal with nothing to retry.** | The condemnation set reaches a terminal the sealed record named in a **non-`Routed`** pairing, so there is no chosen candidate to exclude and the abandon rule forbids abandoning. | A new named `SupportBraceError`, carrying the terminal, the member and the measured margin. **This is not a new failure**: today the same case reaches `require_active` (`forest.rs:983`/`:990`) and becomes a hard `BraceError` from the `?` at `brace.rs:523`. T6 replaces an unnamed engine error with a named physical one. |

**The state that is deliberately absent, restated correctly.** There is no *"legalization gave
up after trying"* run failure: a terminal that uses up its candidates in its context retires
(T2/T3) rather than abandoning the pass. T6 is not that state — T6 never had a candidate to try,
and it is a case that hard-fails today. The blanket claim in the first version of this record
("the retry loop has no give-up state of its own") overstated this and is withdrawn.

**What this record does not promise.** It does not claim the Bat emits. It claims the Bat
reaches T1, T2 or T3 — an emission or a disclosed refusal — instead of an accounting-integrity
error. Which of the three it reaches is a measurement, and the measurement needs the code.

---

## §4. Staged landing plan

**Order: A → C → B → D.** Stage C was previously marked "independent of A and B; can land in
any order". That was a sequencing error. The two production `SurfaceAnchorPool::from_geometry`
sites are in `route_initial_cco_state` (**Routing**, `runner.rs:3844`) and `rewire_state`
(**Rewire**, `runner.rs:4568`) — the two stages a legalization pass repeats, and 85 % of a pass
on the drilled shell (§0). **Stage C is the only lever on Stage B's named cost risk that is not
"wait for task #51".** It lands before B. The determinism replay measurement (S-1) is likewise a
precondition of B, not an option.

**No stage needs a new registered constant.** The exclusion key is an exact bit comparison; the
coverage radius, the censor and the clearance standoff are all read from the registry exactly
as they are read today. `FROZEN_PASS_CAP = 3` (`brace.rs:434`) is pre-existing and untouched by
this plan. If any implementer finds themselves wanting a tolerance, that is an escalation, not a
design detail — say so and stop.

### Stage A — name the class; abandon instead of minting the illegal pair

**What lands.** `brace_fixed_point_current_parity` (`brace.rs:427`) takes one added, borrowed
argument: **the terminals the sealed record names, with the disposition it named for each** —
not only the `Routed` ones. Before the sweep at `brace.rs:523` it plans the condemnation set
read-only, intersects it with that set, and:

* empty intersection → applies the sweep exactly as today, returns
  `BraceFixedPointTermination::ContactRefused` exactly as today;
* intersection contains a **sealed-`Routed`** terminal → applies nothing, returns a new named
  termination carrying the disturbed terminals with their measured margins and radii;
* intersection contains only terminals sealed in **other** dispositions → applies nothing,
  returns a new named termination that says so (T6 in §3).

`brace_state` (`runner.rs:4718`) supplies the set from `state.routed.transcript`, which it
already holds, and converts the new terminations into named brace-stage refusals.

**Three details that decide whether this is sound or merely plausible.**

**(a) Classify on any sealed terminal, not on `Routed` alone.** The tripwire Stage A stands in
front of refuses *every* pair outside the three, plus a ledger cardinality mismatch
(`runner.rs:5693`). A sweep that reaches a terminal the record called `UnroutedNoWinner`
(ledger `Retired`) slips past a `Routed`-only filter and meets `require_active` inside
`quarantine_subtrees` (`forest.rs:983`/`:990`), whose `Err` the `?` at `brace.rs:523` turns into
a hard `BraceError` — the exact "broken engine" message Stage A exists to retire. A `Routed`-only
filter would leave that hole open.

**(b) The read-only query must be an exact prefix of the mutation, or it is production logic
re-implemented one layer up.** `quarantine_subtrees` (`forest.rs:973`–`:996`) is: `require_active`
per key → `collect_subtree_leaf_first` with union dedup → `require_active` over the whole order →
`quarantine` each. Exposing only `collect_subtree_leaf_first` (`forest.rs:1004`) would reproduce
the walk but neither `require_active` pass nor the dedup contract, and the two would drift.
**Refactor instead:** split `quarantine_subtrees` into
`plan_quarantine_subtrees(&self, keys) -> Result<Vec<MemberKey>, ForestError>` — everything up to
and including the second `require_active` sweep — plus an apply step that quarantines the planned
order. `quarantine_subtrees` becomes plan-then-apply and keeps its signature and behaviour. The
classifier calls the **planner**. Drift then becomes impossible by construction rather than by a
test, which is the standing lesson that a defect fixed at one layer reappears one layer up.

**(c) Use the existing map.** The member→terminal side already exists as
`Forest::state.terminal_of: BTreeMap<u32, MemberKey>` (`forest.rs:366`, read at `:625` via
`terminal_key`). **Invert it**; do not add a second lookup and do not hand-roll a key comparison.
Member generations move — the Bat's arm is `MemberKey { slot: 377, generation: 2 }` — so a
hand-rolled comparison is a bug waiting.

**What it buys.** The Giant Bat stops dying with `TerminalEvidenceMismatch`, an error that
reads as a broken engine, and starts refusing at the brace stage with the physics: which
terminal, which arm, how many millimetres inside the model, at what radius. The class becomes
countable across the corpus — which is the measurement that sizes the prize before anyone
builds the instrument, and which supplies the exponent §1.3's bound is missing. **And task #47
arrives for free:** the same refusal on a repair *trial* is already a ledgered rollback, because
`run_stage_chain` returns the context on a brace refusal (`runner.rs:1405`) and
`into_trial_rollback` (`runner.rs:1344`) turns it into one ledger row.

**What it does not buy.** The Bat still does not emit. If Craig's "stops the hard failure"
means "the Bat emits", the smallest stage is A **and** B together, and §5 prices both.

**Files.** `crates/plinth-support/src/brace.rs`, `crates/plinth-support/src/forest.rs`
(the plan/apply split), `crates/plinth-support/src/runner.rs` (`brace_state`, two
`SupportBraceError` variants).

**Tests that pin today's behaviour, and how each moves.**

| test | pins | moves |
| --- | --- | --- |
| `brace.rs:2865` `a_refused_model_seated_arm_condemns_its_stub_instead_of_failing_the_run` | the cascade for an arm carrying a registered, consumed stub: `ContactRefused`, both members gone, a quarantine record on the arm | **Stays green unchanged** if it passes an empty sealed-terminal set — which is the honest call, because the fixture has no sealed record. A second test is added beside it that supplies terminal 7 as sealed-`Routed` and asserts the pass is abandoned with nothing quarantined. A third supplies terminal 7 as sealed-`UnroutedNoWinner` and asserts the named T6 refusal rather than a `ForestError`. |
| `contact_regression_gate.rs:294` (structural needle `BraceFixedPointTermination::ContactRefused`) | OD-4: a refused contact must not hard-error the pipeline | **Stays green.** The variant remains and still carries the brace-rung and model-anchor cases the refutation §B carves out. |
| `contact_regression_gate.rs:324` `an_attestation_refusal_routes_to_quarantine_rather_than_a_pipeline_error` | the initial-build refusal path | **Untouched** — different stage, different path. |
| `runner.rs:7809`/`:7821`/`:7837` brace fixed-point unit tests | convergence, unplaceable, linked-plan outcomes | Signature-only change; pass an empty set. |
| `forest.rs` quarantine-sweep tests | the union dedup and the all-or-nothing `require_active` contract | **Stay green.** The plan/apply split is required to preserve them exactly; a test asserting `plan_quarantine_subtrees` returns precisely the order `quarantine_subtrees` removes is added. |

**Blast radius.** Models whose brace refusals disturb no contact accounting are **expected to be
bit-identical** — that must be **proved** by a corpus run, not asserted. Models that die at
Structure today die at Brace instead, with a better message; they were already dying. No digest
is expected to move, because nothing that is folded into a digest changes value on a surviving
model — again, expected, and the corpus run is the proof.

**Revert proof.** A test that condemns a carrier whose stub's terminal is sealed `Routed` and
asserts (a) the tree is untouched and (b) `finalized_unrouted_terminals` is never reached with
the illegal pair. If someone later "fixes" this by admitting a fourth pair, that test goes red.

### Stage C — the run-invariant memo (§2.4 item 1) — lands SECOND, before the retry

`SurfaceAnchorPool::from_geometry` hoisted at **both** sites — `runner.rs:3844` in
`route_initial_cco_state` and `runner.rs:4568` in `rewire_state`; take both or the win is
silently halved — and the legacy pre-routing replay hoisted on the same argument. Keyed on the
geometry and configuration digests. Two obligations that must land with it, not after it: the
work charge becomes incremental with the memo rather than pricing a fiction, and the receipt
`charge.covers_execution(meter)` must still hold.

**Why it is second and not "any order".** Those two sites are the Routing and Rewire stages,
which are 85 % of a legalization pass on the drilled shell and 81 % on the oriented solid (§0).
Landing C before B means every extra pass B introduces is paid at the reduced price. Landing it
after means the first measurement of B's cost measures a cost we already knew how to cut.

**Proof.** Every corpus artifact bit-identical, and a sabotage test: corrupt a stored value and
the artifact must change.

### Stage B — the legalization retry, at attempt 0 only

**Preconditions, both of which are measurements rather than opinions.** Stage A's corpus count
(S-2), so the class is sized before the instrument is built; and the determinism replay (S-1),
because the whole loop rests on a plan replaying identically — route the same plan twice and
compare the transcripts bit for bit.

**What lands.** Four pieces.

1. **The exclusion channel.** `route_initial_cco_with_limits` (`runner.rs:3785`) takes a borrowed
   exclusion set keyed as §1.3 states. The tournament refuses an excluded candidate with a new
   named reason. A terminal whose candidates are all excluded **in its context** retires with a
   new `NoWinnerEvidence` variant beside the three at `cco.rs:657`, carrying the arithmetic of
   §1.3 **and the measured margin and radius per excluded arm**.
2. **Candidate identity.** The arm's endpoint coordinates by exact bit pattern, recorded beside
   `CcoCandidateEvidence` (`cco.rs:605`), which does not carry them today. This is hand-written
   plumbing, not an existing field.
3. **The abandon outcome.** `ChainOutcome` (`runner.rs:1298`) gains a third arm carrying the
   context back plus the exclusions the pass learned. The stage macro at `runner.rs:1374` is
   untouched; the new arm is produced only by the brace stage, and only when the abandon rule is
   satisfied (§1.3).
4. **The loop.** One `loop` wrapping the **attempt-0** chain call (`runner.rs:1596`) — and
   **not** the trial chain call at `runner.rs:1874`, which keeps today's
   `restore_prepared_digests` / `into_trial_rollback` path exactly (§1.4). It holds the exclusion
   set, restores the five prepared digests on each abandoned pass exactly as the trial path does
   at `runner.rs:1888`, and matches each exclusion's context tuple before honouring it — a
   mismatch is a **miss**, not an error.

**The `candidates_feasible` question, named rather than decided in passing.** An excluded
candidate must be counted somewhere. The first version counted it **out of**
`candidates_feasible`, chosen so the CCO transcript digest (which folds `candidates_generated`
and `candidates_feasible`, `cco.rs:2393`–`:2411`) moves only for excluding models. That buys
digest stability with the meaning of a sealed, physics-named counter: `candidates_feasible`
today answers *"how many arms passed feasibility"* and would afterwards answer *"how many passed
feasibility and were not refused in an earlier pass"*. Refutation §F.1 reads that exact field to
make its "arms existed, clearance refused them" argument, and after the overload that reading
needs a footnote nobody will have.

* **This record's recommendation: keep `candidates_feasible` as physics** and add a separate
  folded `candidates_excluded`. Feasibility keeps one meaning. **The price is one deliberate,
  corpus-wide transcript re-freeze**, paid once, with the reason written down.
* **The alternative** is the overload, which keeps the digest still for non-excluding models. It
  is coherent, and it is cheaper today.

This is a choice about what a sealed number means, so it goes to Craig as authorization item 5
(§5) rather than arriving as an implementation detail.

**Reporting, per the no-cap doctrine.** `RepairRunReport` (`coverage_repair.rs:671`) gains
`legalization_passes`, `exclusions`, and per-terminal exclusion counts, printed on every path
and read by no branch.

**Files.** `cco.rs` (the exclusion parameter, the candidate identity, the refusal reason, the
evidence variant), `runner.rs` (the outcome arm, the loop, the report fields),
`coverage_repair.rs` (report fields).

**Tests that pin today's behaviour, and how each moves.**

| test | pins | moves |
| --- | --- | --- |
| `runner.rs` `a_stage_error_is_not_discarded_at_attempt_zero` | an attempt-0 stage failure stays `Failed { phase, detail }` | **Half unchanged.** A brace refusal that is *legalizable* no longer reaches attempt-0 failure; every other stage failure does. The test gains a sibling that pins "an abandoned pass is not a failure". |
| `a4_transactional_controller.rs` trial-rollback tests | a trial's stage refusal is one ledger row and the next candidate | **Untouched by design.** The loop does not wrap `runner.rs:1874`. A test asserting a trial's disturbing brace refusal is still a rollback pins the C3 precedence rule. |
| the sealed-record digest goldens (the toed-slab digest at `support_runner_integration.rs:305`, `:580`, and `plinth-verify/tests/coverage_gate_toed_slab.json:1`) | a model that closes at attempt 0 is bit-identical | **Stay green under the overload option.** Under the recommended `candidates_excluded` option they re-freeze once, corpus-wide, deliberately. |
| the CCO transcript digest (`cco.rs:2393`–`:2411`) | the tournament's own accounting | Depends on the authorization above. Either it moves only for excluding models (overload) or once for every model (new counter). |

**Blast radius.** Any model with zero exact-clearance refusals runs exactly one legalization
pass with an empty exclusion set, so it is unaffected by the loop itself — and that must be
proved by a corpus run. Models that do exclude pay extra passes; the count and the wall time are
printed. **The named risk, stated plainly:** a model where the clearance mask is unsatisfiable
can exhaust a terminal's whole candidate set — up to ~100 passes at 17.8 s, where 100 is the
largest arm count MEASURED at one terminal and it was measured on `zombie-3-a`, not on the Bat —
and, if several terminals fail together, the passes **multiply** over the backtracking tree
rather than adding. That combined bound is finite and **not measured**; Stage A's corpus count is
the first measurement. It is loud, and it is not capped. If it proves intolerable the answer is
#51's mask fix or a ruling from Craig — not a cap arriving by the back door.

**Revert proof.** A test with a fixture whose first-choice arm is exactly illegal and whose
second is legal, asserting: the first pass abandons, the second seals a different arm, the
final accounting shows `Routed`↔`Consumed`, and the run emits. Plus a **stale-key test**: change
the context by one bit and assert the exclusion **misses** — the candidate is regenerated,
re-measured and re-refused on its own physics, and no error is raised. Plus an **abandon-rule
test**: a pass that would abandon while learning no new exclusion must refuse by name (T6)
rather than loop.

### Stage D — carry the legalization record into the evidence sidecar

The exclusion ledger and the pass count join the coverage refusal sidecar, and the corpus counts
are pinned. **This is disclosure, not the record**: the measured margins and radii already live
inside the sealed `NoWinnerEvidence` from Stage B (§1.3), so a defective sidecar cannot hide a
missing measurement. This is the schema move, and it should ride whatever other schema move the
A4 lane already owes rather than minting a second one.

---

## §5. THE DECISION MEMO — for Craig

*Self-contained. Read this alone if you read nothing else.*

### Words this memo uses

* **Seal** — a record written once during a run and never changed afterwards. Later stages must
  agree with it; if one disagrees, the run stops.
* **The audit** — the check that every support point's sealed record and its live state in the
  tree agree.
* **Making the plan legal** — throwing away a planning pass that produced an illegal support and
  planning again with that support ruled out. The design calls this "legalization".

### The dilemma, in two sentences

The routing stage seals a record saying a support point is routed, but the brace stage later
measures that support's true thickness and finds it inside the model, and condemning it makes
the sealed record and the live tree disagree — which the audit correctly refuses, killing the
Giant Bat. We already proved we cannot predict the brace stage's answer earlier, because the
number it depends on does not exist yet and moves in both directions, so we must either weaken
what "routed" promises, or make the promise come true, or stop the thickness from changing.

### The three coherent directions

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
planning pass away and plan again with that exact arm ruled out.

* *Where the retry runs:* **only on the model's own first plan.** A repair trial that fails
  bracing is still dropped and the next candidate tried, exactly as today. The retry is never
  wrapped around a trial.
* *Result:* nothing is admitted late, no fourth pair is ever minted, the audit is untouched,
  and the Bat gets a second choice of arm. If a support point runs out of arms, it retires as
  "no winner" — one of the three pairs the audit already admits — and the run either emits with
  an honest hole or refuses with the hole named in millimetres.
* *Why it stops:* each thrown-away pass records one refusal of the form "this support point, in
  this exact tree state, must not use this arm again". "This exact tree state" is five
  fingerprints the record already keeps. If the tree state is not the same next time, the
  refusal **does not apply** and the arm is measured again from scratch — so no answer is ever
  reused where it was not measured. Each thrown-away pass adds a refusal that was not there
  before, and the possible combinations are finite, so the search ends.
* *Trade-off:* one extra plan-and-brace pass per illegal arm. Measured on the Bat that is one
  arm per pass at about **18 seconds** on the drilled shell and **15 seconds** on the solid.
  Roughly 100 passes is one support point using up its arms, and 100 is the largest number of
  arms we have counted at one point — **on a different model** (zombie-3-a, 42 to 106 arms
  proposed per point), not on the Bat. **If several points fail at once the passes multiply
  rather than add**, and we have not measured how often that happens. The first stage's corpus
  count is that measurement. All of it is printed, not capped.
* *Reversibility:* good. The first stage is one classification and reverts to today's bytes.
  The last stage is one loop and one parameter; delete the parameter and you are back at the
  first stage.
* *Why it is not the direction you already refused:* that direction asked us to **guess** the
  brace stage's answer while choosing arms. This one **measures** the answer exactly, then
  chooses again. No approximation, no bound, no new number.

**Option 3 — stop the thickness from changing (declined, and it is the direction our own
refutation preferred).** Leave routing, the seal and the audit alone. Instead refuse the resize
step that makes a model-touching support thick enough to end up inside the model.

* *Why it is attractive:* it is the smallest possible change. Nothing about the seal moves.
* *Why I decline it:* the thickness grew for a reason. On the drilled shell the arm went from
  0.949 mm to 1.410 mm **because the load path demanded it** — that number is a strength
  requirement, not a mistake. Refusing the growth does not remove the defect; it swaps a
  clearance defect for a support that is too thin for its load. A clearance defect announces
  itself in the audit. A too-thin support does not: it prints, and it snaps. And it does not
  even cover the class — on the solid the arm **shrank**, 1.048 mm to 0.902 mm, and it was
  **still** inside the model. "Do not let it grow" says nothing about that one.
* *If you disagree, say so and I will cost it properly.* It may still be right as its own task
  for the resize-timing family; I am declining it as the answer to this seam.

### My recommendation, and the reason

**Option 2.** The reason is one sentence: **Option 1 spends the audit seal and still does not
fix the Bat; Option 3 trades a defect that announces itself for one that does not, and only
covers half the measured cases; Option 2 fixes the Bat while leaving the seal exactly as it
is.** Every one of the three accounting functions — the three-pair check, the removal at the
point the spec names, and the recompute-from-state tripwire — is unchanged, at every call site,
under Option 2. Under Option 1 all three change and one of them (the repartition) deviates from
the binding spec.

Two supporting reasons. First, the retry is a **generalization of machinery the coverage
controller already has** — the context handback on refusal, the exclusion set that survives a
rollback, the digest restore, the counters that report and never decide — not a second parallel
mechanism. The genuinely new work is two things: a parameter passed into the routing tournament,
and recording each arm's endpoint coordinates so a refusal can name the same arm on the next
pass. That second one is new plumbing, not an existing field, and I have priced it as such.

Second, the loop **ends** rather than running forever, and I want to be exact about how strong
that is. It is a search with backtracking, not a straight line. It ends because it never repeats
a combination, and the combinations are finite. It is **not** "at most 100 passes". I do not
know the real number and neither does anyone else until the first stage counts the class. An
earlier draft of this memo claimed the stronger, simpler bound; that claim was wrong and I have
withdrawn it.

### The cost band

I am giving you work shape, not calendar time.

**First stage (name the class, abandon instead of quarantining):** three files
(`brace.rs`, `forest.rs`, `runner.rs`), one added parameter, one read-only query — which is a
small refactor of an existing function into "work out what to remove" plus "remove it", so the
query cannot drift from the removal — and two new named outcomes. Three new tests, one existing
test gains siblings. **No digest, golden or schema move is expected — and the corpus run in this
stage is what proves it.** One corpus run to prove bit-identity on every model that is not
affected. This is the small, safe, revert-proof one.

**Second stage (the invariant-work saving):** one function that is rebuilt on every pass but
depends only on the model and the settings, hoisted at both of the two places it is built. Those
two places are the routing and rewiring stages, which are **85 % of a pass** on the Bat's shell.
This lands **before** the retry, so every extra pass the retry adds is paid at the reduced price.
It changes no verdict, and a sabotage test proves it: corrupt the stored value and the output
must change.

**Third stage (the retry itself):** four files plus the evidence record. The genuinely
hand-written seam is the exclusion channel into the routing tournament, the arm identity, and the
context match — the same class of work as the context-handback seam the A4 lane already did, and
like that one it should not be called mechanical. Two corpus runs: one to prove bit-identity for
unaffected models, one to measure the extra pass count and wall time on the Bat, the Warhorse and
the treant. The named cost risk is the extra passes on a model whose clearance mask is
unsatisfiable, and the answer to that is task #51, not a cap.

**Fourth stage (disclosure):** the pass record joins the existing evidence file. This is the only
schema move, and it should ride a schema move the A4 lane already owes.

### What I am asking you to authorize

1. **The contract.** Sealed `Routed` keeps its current final-admission meaning. Option 1 is
   declined. Option 3 is declined on the reasoning above. Say yes, or name one of the other two.
2. **The first stage on its own, first.** Land the classification and the named brace-stage
   refusal with no retry, prove bit-identity on the corpus, and count the class across all 45
   models before building anything else. This is the "size the prize before building the
   instrument" step, and it is also what tells us how many support points fail at once — the
   number that decides whether the retry's cost is small or large.
3. **The invariant-work saving before the retry, not after.** It cuts most of the cost of the
   thing we are about to do more of.
4. **The retry after that measurement, not before.** With the cost printed and uncapped. I would
   like your explicit agreement that the bad case — a support point that uses up every arm, and
   the worse case where several points fail together and the passes multiply — is reported
   loudly and **not** capped, because that is your no-cap doctrine applied to a number I expect
   someone to want to cap.
5. **One sealed counter's meaning.** The tournament records "how many arms passed feasibility".
   A ruled-out arm has to be counted somewhere. I want to **keep that counter meaning exactly
   what it means today** and add a second counter for ruled-out arms. The price is that one
   sealed fingerprint changes for every model, once, on purpose. The cheaper alternative is to
   fold ruled-out arms into the existing counter, which keeps the fingerprint still for models
   that never rule anything out but leaves one number with two meanings. I recommend paying the
   one-time cost. Your call, because it is a sealed number.
6. **One thing I am NOT asking for**, so it is on the record: no new registered constant, no
   fourth admitted pair, no second removal point, and no resurrection of the ranking rule.

### What I am running before I ask you for the retry

The design assumes the routing cadence is deterministic given the tree state — that is what
makes a refusal mean the same thing on the next pass. **I do not assume it. I will measure it**
before the retry is built: route the same plan twice and compare the transcripts bit for bit.
This is one cheap run, the whole retry rests on it, and an earlier draft of this memo offered it
to you as an option instead of simply running it. That was the wrong shape for a precondition.
If the replay is not bit-identical, the retry does not get built and I come back to you with
that result instead.

---

## §6. What this record does NOT claim

* **Not** that the Giant Bat emits. It claims the Bat reaches an emission or a disclosed
  refusal instead of an accounting-integrity error, and that which one is a measurement.
* **Not** that the refutation is wrong. It accepts §C entirely: the brace verdict cannot be
  predicted when candidates are chosen. It uses the measured verdict instead of predicting it.
* **Not** that the retry loop is bounded by ~100 passes. That claim was in the first version and
  is withdrawn. The bound is finite, combinatorial and unmeasured (§1.3, §0).
* **Not** that the retry loop can never end a run. It introduces no new failure for the class it
  is built for, and it names one case (T6) that hard-fails without a name today (§3).
* **Not** that the 89 rejected trials can be saved by storing results. §2 argues the opposite,
  precisely.
* **Not** that Option 1 is incoherent. It is coherent and Codex is right about its shape; it is
  declined on cost, on the spec deviation it forces, and because it does not fix the Bat alone.
* **Not** that Option 3 is a bad idea in general. It is declined as the answer to this seam, on
  the evidence in §1.5, and it may still be right for the resize-timing family.
* **Not** that the extra passes are cheap. §4's named risk is real and unmeasured.
* **Not** a proposal to admit a fourth pair, add a second removal point, move any registered
  number, or resurrect the ranking rule.

## §7. Open items and escalations

| # | item | owner |
| --- | --- | --- |
| S-1 | Is the routing cadence bit-reproducible on a replayed identical plan? One cheap measurement. **Precondition of Stage B, not an option** — it runs whether or not Craig asks for it. | this lane |
| S-2 | Stage A's corpus count: how many of the 45 models hit an accounting-disturbing brace refusal, and **how many terminals are disturbed in one sweep**? The second number is the exponent in §1.3's bound. Unknown today. | Stage A |
| S-3 | The extra-pass cost on a mask-unsatisfiable model, measured **after** Stage C's hoist. Depends on #51's landing state. | Stage B Prove |
| S-4 | Task #47 is discharged by Stage A for the trial path; its title should be re-read once Stage A lands. | #47 |
| S-5 | No new registered constant is needed anywhere in this plan. If an implementer wants one, that is an escalation to Craig, not a design detail. | binding on all stages |
| S-6 | `candidates_feasible` keeps its physics meaning and a new `candidates_excluded` is added, at the price of one deliberate corpus-wide transcript re-freeze — **or** the existing field is overloaded. Memo authorization item 5. | Craig |
| S-7 | Direction (1), constrain the resize, is declined for this seam in §1.5 and never costed. If Craig overturns the decline, it needs its own design pass in the resize-timing family (#25). | Craig, then #25 |

---

## §8. Review response — where each finding landed

This record's first revision was reviewed adversarially at `7e668e9` with the verdict
SOUND-WITH-CHANGES. Every finding is folded; this table says where, so a reader can check the
fold rather than trust it.

| finding | where it landed |
| --- | --- |
| C1 — exclusion key and the termination proof | §1.3 (context digest tuple from `cco.rs:642`, miss-not-error, the abandon rule, backtracking termination, the withdrawn give-up claim), §0, §2.3, §3, §6 |
| C2 — worst-case arithmetic and per-pass cost | §0 (derivation table, 17.8 s / 15.1 s, the 16.4 s trap), §1.3, §4 Stage B, §5 |
| C3 — the loop runs at attempt 0 only | §1.4 (stated first, three reasons), §4 Stage B item 4, §4 Stage B tests, §5 |
| C4 — the third direction | §1.5 (new), §1.6, §5 Option 3, §6, S-7 |
| C5 — measured margins inside the sealed evidence | §1.3 (the meaning-change paragraph and verifier obligation 1), §3 T2, §4 Stage B item 1, Stage D |
| C6 — stage order A → C → B | §4 header, §4 Stage C, §5 cost band and authorization item 3 |
| C7 — corrected anchors and new plumbing | §2.4 item 1 (`runner.rs:3844`/`:4568`), §1.3 and §1.4 item 3 (endpoints are new), §0 (the absent `CoverageMeasurement` line at `runner.rs:1603`) |
| C8 — Stage A classification, planner prefix, `terminal_of` | §4 Stage A (a), (b), (c); §3 T6 |
| C9 — `candidates_feasible` meaning | §4 Stage B (named as a choice with a recommendation), §5 authorization item 5, S-6 |
| C10 — doctrine, clean conditional on C1 | discharged by C1's fold; `FROZEN_PASS_CAP` noted as pre-existing in §4 |
| C11 — the eight memo rewrites | §5 throughout: glossary, three options, where the loop runs, the corrected worst case, the cost parts, "expected and proved" rather than asserted, the fifth authorization item, and the determinism check as a precondition |
