# Testing doctrine: three kinds of check

**Status: the doctrine is written and instrumented. It is NOT yet delivered, and the work
that carries it is NOT landable as it stands.** Sections 3 and 7 say exactly why, with the
measurements behind both statements.

Work branches described here: `docs/testing-doctrine` (`be7d28e`, the doctrine text),
`feature/ratchet-harness` (`0a39ca8`), `feature/invariant-checks` (`5def17e`),
`feature/reclassify-pins` (`0ba8019`, the retirements), all merged for verification at
`f335fbc` on `verify/testing-doctrine`. Every measurement below was re-run first-hand at
`f335fbc` for this report; none of it is quoted from a lane's own account.

---

## 1. What changed, and why

The engine plans the scaffolding that holds a miniature to the build plate. The single most
valuable thing the project can improve is **where those supports go**. The tests made that
improvement expensive.

Many tests pinned a **fingerprint** of a support plan: a 20-digit digest of the sealed
support forest, plus exact counts of routed contacts and live members. A fingerprint answers
one question — *did the output change?* It cannot answer the question that matters — *did
the output get better?* A deliberate placement improvement and a silent regression move the
digest in exactly the same way, so every placement change arrived as a wall of failing
tests that a human had to read a diff to adjudicate, and then re-pin by hand.

The owner ruled on 2026-08-29:

> "Pinning support location is I think generally bad policy for tests — most of the
> iteration/improvement that we'll ever want to do is related to 'better placement of
> supports'. On the other hand, sometimes we want to make changes like 'make this run faster
> without changing the supports' in which case we do want to evaluate non-regression. Having
> a hard-and-fast 'never let supports move' ruling is counter-productive most of the time."

So the fingerprint is not abolished. It is demoted from a permanent property of the system
to a claim about one specific change, and two other kinds of check take over the jobs it was
doing badly.

The scope was measured before any edit: 37 pinned large integers across 5 test files,
5 `assert_eq` comparisons on a `.digest()`, and exactly one real golden file
(`coverage_gate_toed_slab.json`). Not all of that is wrong — a constants registry is
supposed to freeze its constants — so the job was to classify by intent, not to delete
numbers.

---

## 2. The three kinds of check

| Kind | Lifetime | What it asserts | May it be refreshed? |
|---|---|---|---|
| **Invariant** | Permanent | A property true of ANY acceptable plan, whatever the placement: the area law, a closed load path to the plate or model, no demand-bearing contact left unrouted, no two contacts sharing one physical seat | **Never.** A broken invariant is always a bug |
| **Quality ratchet** | Long-lived, one-way | A recorded number, per corpus model, saying how GOOD a plan is: worst tip area ratio, total scar area, resin volume, residual uncovered area, worst uncovered gap, ownerless model area | Improves freely and silently. Worsening needs an argument |
| **Equivalence check** | One change | Run before, run after, compare byte for byte, record the result in the commit that makes the claim | Not stored at all — it is a procedure, not a constant |

The regime follows from what a change **claims**:

| Change | Fingerprints | Ratchets | Invariants |
|---|---|---|---|
| Better placement | Expected to move. No argument needed | Must not worsen | Must hold |
| Performance or refactor | Must NOT move — checked at the time, in that commit | Unchanged | Must hold |
| Physics change | Expected to move | May move, argued | May change, but only by owner ruling |

### What a contributor does now

- **Improving placement.** Run the ratchets. If every metric is equal or better, that is the
  whole argument; record the improvement with `PLINTH_RATCHET_UPDATE=1`. If a metric got
  worse, say which one, by how much, and why the trade is right; record it with
  `PLINTH_RATCHET_ACCEPT_REGRESSION=1` alongside, so the worsening is written down rather
  than worked around by hand-editing the recorded values. Never touch an invariant.
- **Making it faster.** Do not change the recorded numbers at all. Run the plan before and
  after in the same process and settings, compare byte for byte, and put that comparison in
  the commit message. That is what a fingerprint is for, and its whole life is that commit.
- **Changing the physics.** Everything above may move, and none of it moves without an owner
  ruling quoted in the commit.

The recorded ratchet values live in
`crates/plinth-support/tests/fixtures/support_ratchets.json`, keyed to each corpus model's
own STL digest, so re-tessellating a model re-baselines it rather than comparing two
different meshes as if they were one plan. The harness is
`crates/plinth-support/tests/ratchet_harness.rs`; its module doc is the reference for the
metric set, the tolerance rule and the two environment variables.

---

## 3. The proof — both halves, re-run for this report

A doctrine like this is unproven unless **both** of these are true at once: supports move and
the suite tolerates it, AND the plan is made worse and something fails. Each half alone is
worthless — a suite that tolerates everything passes the first, and the old pinned suite
passes the second.

### Half one: supports moved, and the placement-independent checks tolerated it

Perturbation: reverse the final tie-break in the routing order
(`crates/plinth-support/src/initial.rs`, the two `terminal.cmp` comparators). This is a legal
selection change — a different, equally admissible ordering — not a degradation.

Measured effect on `make_toed_slab`: routed contacts unchanged at 64, final live members
170 → 174, digest `13198418135389558006` → `9878193565529525115`.

| Check | Result |
|---|---|
| `toed_slab_contacts_stay_pairwise_distinct_and_under_the_scar_ceiling_regardless_of_the_digest` (invariant) | **PASSED** — placement moved, the property held, nothing to re-pin |
| `public_runner_seals_toed_slab_with_its_frozen_digest_and_abandoned_load` (fingerprint, still pinned) | **FAILED** at `support_runner_integration.rs:369`: `left: (64, 174, 9878193565529525115)`, `right: (64, 170, 13198418135389558006)` |
| Ratchets | **FAILED**, and said something useful: `model 'make_toed_slab', metric 'support_resin_volume_ml' (lower-is-better, mL): 0.550812399980 -> 0.592125137274, WORSE by 0.041312737294` |

Read that carefully. The invariant behaved exactly as designed. The ratchet behaved exactly
as designed and produced a sentence a person can reason about, rather than a moved hash. But
**the fingerprint still failed**, because nobody retired it. The perturbation is a fair
stand-in for the placement work the owner wants unblocked, and that work is still blocked
today by the two pins in `support_runner_integration.rs` (a digest-plus-counts triple at
line 369, and a bare digest equality at line 785). The doctrine is documented and
instrumented; it is not yet in force.

### Half two: the plan was made worse, and named checks failed

Sabotage: halve the tip radius of every model-touching member at construction
(`crates/plinth-support/src/forest.rs`, `Member::new`). Halving a tip quadruples the load per
unit of tip area — the exact defect the area law exists to catch.

What failed, by name:

- `toed_slab_contacts_stay_pairwise_distinct_and_under_the_scar_ceiling_regardless_of_the_digest`
  — the new placement-independent invariant test — **FAILED** at
  `support_runner_integration.rs:1587`: `make_toed_slab must close coverage on its first
  closure attempt: Failed { phase: Routing, detail: "Cco(FinalizedStubLoadMismatch)" }`.
- `public_runner_seals_toed_slab_with_its_frozen_digest_and_abandoned_load` **FAILED** on the
  same refusal.

An honest limit on this half: the engine's own internal consistency check refuses the
sabotaged run before a sealed-but-worse plan ever exists, so this sabotage proves the
regression is *caught and named*, not that the ratchet numbers would have caught it. The
ratchet's ability to catch a *sealed* worsening is proved separately by half one, where a
legal but worse plan sealed cleanly and the resin-volume ratchet named it. Between them,
both halves are covered.

Every measurement above was taken under `--profile gate`, the profile the repository
mandates for a lane loop.

---

## 4. Checks removed, and what covers them now

| Removed | Where it was | What covers it now | Proof |
|---|---|---|---|
| `assert_eq!(seats.len(), identities.len())` — no two routed contacts share one physical seat | `support_runner_integration.rs:454`, written **after** that function's own `digest()` equality | `toed_slab_contacts_stay_pairwise_distinct_and_under_the_scar_ceiling_regardless_of_the_digest`, which asserts the same property with no digest, contact ID or member count in sight | The reclassification lane aliased one contact's disclosed seat onto another's and recorded both the old assertion and the new test failing identically (`left: 63 right: 64`); the new test's text was read for this report and asserts exactly that property |
| The loop over abandoned contacts checking that each quotes `TIP_RADIUS_MAX_GROWN_SCAR_MM` and that no growth rung exceeds it | `support_runner_integration.rs:517–529`, also after the `digest()` equality | Same standalone test, and more generally — it checks *whichever* contacts a placement change happens to abandon, not the two IDs the old loop hard-coded | Zeroing the disclosed ceiling failed the old assertion and the new test with the matching message |
| `assert_eq!(outcome.pending_gates.len(), 11)` | `plinth-pipeline/tests/first_artifact.rs` | An explicit named vector of the nine gates that are still pending, plus three separate "this ported gate must NOT be listed as pending" assertions | Read side by side against `develop` for this report: a count of 11 is satisfied by any 11 gates; the named set is not. Strictly stronger |

Both removals sat **after** an `assert_eq!` on a digest in the same function. `assert_eq!`
aborts on the first mismatch, so those two invariants stopped running entirely the moment a
placement change moved the digest — the pin was not merely expensive, it was silently
disabling the checks that mattered. That is the strongest single argument for the new
doctrine, and it was found by doing the reclassification.

No other check was removed anywhere in the workspace.

---

## 5. What is still pinned, and why that is correct

- **`constants_placeholders.rs`.** A constants registry is supposed to freeze its constants;
  that is its whole job. Correct as an invariant.
- **`corpus_budgets.rs`** (decimation budgets) and **`penetration_census_equivalence.rs`.**
  Equivalence and budget claims, not placement claims. Correct.
- **`coverage_gate_toed_slab.json`**, the one real golden, and the coverage-gate resource and
  censor-count pins. Nothing yet re-derives their content placement-independently, so under
  the governing rule — never delete a check before proving its replacement — they stay.
- **The two toed-slab digest/count pins** in `support_runner_integration.rs` (lines 369 and
  785) and the repair-round, abandoned-count and refusal-classification pins beside them.
  These are the ones the doctrine says should go, and they are still here. They are *correct
  to keep until* an invariant or ratchet covers each property they carry; they are *not
  correct to keep indefinitely*, because half one of section 3 shows them refusing a
  legitimate placement change. This is unfinished work, not a settled state.

---

## 6. What none of this proves

A ratchet number is a model of print quality, not print quality. Every metric here is
computed from the plan the engine produced, using the engine's own understanding of load,
area and coverage — the same understanding that produced the plan. If that understanding is
wrong, all six metrics can improve while the part gets worse on the plate.

The ground truth is a print. Nothing in this document, and nothing in the harness, replaces
putting a plate through the printer and looking at what came off it. The owner said as much
when ruling on this work: every placement improvement stays hypothetical against a new
support-model theory until printing proves it. The value of the doctrine is that it stops the
static fixtures from *blocking* those experiments — not that it decides them.

Two further limits worth stating plainly:

- The invariant test added by this work covers one fixture (`make_toed_slab`) and two
  properties. The larger invariant set the doctrine describes — a closed load path from
  every support, every overhang island supported, every unit of load landing on exactly one
  support — is described but not yet implemented as placement-independent checks.
- Two of eight corpus models have no ratchet baseline at all: `make_raking_blade` does not
  close coverage, and `Weeping Treant v1.2` exceeds the 120-second per-model budget. Both are
  disclosed and skipped by name rather than silently passed, but neither is ratcheted, and
  the treant is the model the whole hollowing case rests on.

---

## 7. What the owner must decide

1. **The ported `Load` gate refuses the entire corpus, and the product now emits nothing.**
   This is the blocker. The gate reads the area law off the record and refuses when a contact
   carries more than `X_ALLOWED = 12` times what its tip can hold. On `make_toed_slab`,
   contact 48 carries about 450 mm² through a 0.05 mm radius tip — a ratio of 57,295. The
   gate is arithmetically right; the plans are genuinely illegal. Re-run first-hand for this
   report: `plinth-pipeline`'s `first_artifact` suite alone goes from green to
   `LoadLawViolated` on `make_toed_slab` (64 violations), `make_overhang_table` (74),
   `make_box` (90) and the shell-under-walled fixture (11), and `plinth prepare` reports
   `GATE-REFUSED Load`. Seventeen tests across `plinth-cli` and `plinth-pipeline` are red for
   this one cause. **Nobody may accept a pipeline that refuses every model except the owner.**
   The choice is: turn the gate on and stop shipping until the placement work fixes the
   plans; turn it on as a warning that records the violation without refusing; or hold the
   gate on its own branch until the load-model redesign lands.
2. **Finish the retirement, or the ruling is not in force.** The two placement-blocking pins
   at `support_runner_integration.rs:369` and `:785` survived. Retiring them needs the
   invariant and ratchet coverage for the properties they carry (contact and member counts,
   abandoned-load classification) built first, per the never-delete-first rule. Until then a
   placement improvement still lands as a red suite.
3. **A pre-existing engine defect, newly exposed — the ratchet harness is red under the
   default test profile.** The mandated gate command `cargo test --workspace --offline`
   builds with debug assertions ON. Under it, `Cylinder` trips a `debug_assert!` in
   `crates/plinth-support/src/specialized.rs:5217` — *"a retained round executed 18874 load
   operations against a charge of 4038"* — the worker thread dies, and the harness correctly
   reports a `REACHABILITY REGRESSION` for a model that has a recorded baseline. Under
   `--profile gate` (debug assertions off, per the repository's own lane-loop instruction)
   the same model seals and its six recorded values reproduce bit-exactly. Reproduced three
   times, deterministic, and present at the harness's own commit, which changes no production
   file at all — so the defect is in `develop`, and this work only exposed it. Two decisions follow: whether
   the charged-versus-executed accounting for a retained round is wrong or the charge is, and
   whether a harness that passes in one mandated profile and fails in the other may land in
   that state.
4. **The doctrine text and the code are on different branches.** `docs/testing-doctrine`
   (`be7d28e`) rewrites `docs/development.md` and `docs/engineering-journal/06-conventions.md`
   around the three kinds; the code branch independently added a "Three kinds of check, three
   lifetimes" section to `docs/development.md` while leaving the old "The frozen fingerprints"
   text in place above it. Both readings of the doctrine currently exist. They must be
   reconciled into one before either lands, and the old text must go — a document that states
   the retired policy and the adopted policy on the same page teaches the retired one.
5. **Ratchet coverage of the treant.** `Weeping Treant v1.2` is unratcheted because it needs
   more than 120 seconds. It is also the model that carries the hollowing case. Raising the
   budget, or measuring it out of band, is a spend decision.

## Verdict

The machinery is sound and demonstrably load-bearing. The ratchet harness is the first check
in this tree that says how good a plan is rather than whether it changed, and the
reclassification found two invariants that a moved digest had been silently switching off.
Neither is decorative: each was made to fail on a deliberate defect and then made to pass
again.

But the branch as it stands is **not landable**, for two independent reasons: the experiment
the ruling exists for still fails at the last step, and the ported `Load` gate turns a live
area-law defect into a refusal that stops the product emitting anything. The right response
is to finish it, not to revert it.
