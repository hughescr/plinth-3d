# Incremental integrity: making the per-trial proof cheaper without weakening it

Ruling on `feature/incremental-integrity` (6 commits, 7 source files, +942/-250),
measured and attacked 2026-08-28/29 against `develop` @ `0b885ba`.

**Verdict: MERGE, after two mandatory corrections to the rule document.**
No fingerprint moved. No check was deleted. The speed-up is real and is entirely
cost-per-trial. The corrections are cheap and are named in section 7.

---

## 1. What changed, in plain language

After supports are placed, a repair loop adds one support at a time, re-measures, and
keeps the change only if it helped. Before each measurement the code proves the plan has
not been tampered with, by walking the entire plan structure and hashing it into a single
64-bit fingerprint. That proof is honest, and it is expensive: profiling on 2026-08-28
showed the walk-and-hash was **over half of all work in the loop** (55.6% on `dust_shelf`,
66.0% on `raking_blade`), against under 8% for the actual geometry. The heaviest single
function in the whole loop was hashing one 3D point.

The loop cannot be made to take fewer trials — that was proved and reverted earlier, and
is recorded in `repair-ranking-veto-measurement.md`. The trial count is the size of the
positive-scoring pool, not a property of its order. So the only remaining lever is the
cost of each trial.

This change makes the fingerprint cheaper to serve, in two shapes:

- **Sealed values.** Two data structures — the generic contact plan and the selected-contact
  hand-off — never change after they are built. Their fingerprint is now computed once, at
  construction, from the very values being placed into the structure, and served from there.
  A parent hash that used to walk the whole child now folds the same eight bytes it would
  have arrived at anyway.

- **A sealed span.** The largest single stretch of the plan's hash stream is the repair pool,
  the islands, and the coverage samples. On `raking_blade` the coverage samples alone are
  16,640 records. Those three are written once when the plan is built and never touched
  again. The code now remembers the running hash accumulator on *both sides* of that stretch.
  On a later walk it refolds everything *before* the stretch the long way, and is allowed to
  skip the stretch **only if the accumulator arriving there is bit-for-bit the one the seal
  was taken at**. Any difference anywhere earlier, and it folds the stretch itself.

The reason this is bit-identical rather than merely equivalent: the fingerprint is a running
FNV-1a chain, `state = (state ^ byte) * prime`. Such a chain cannot be decomposed into
independent pieces, but its accumulator *is* a valid resume point — given the state after
byte *k*, the same bytes *k+1..n* in the same order yield the same final word, by
construction. I verified in the source that `Fnv::finish`, `Fnv::state` and `Fnv::resume`
are all bare `self.0` with no final mixing step, so resuming really is exact. No byte is
reordered, added, or dropped.

A new module, `digest_seal.rs`, is the single place this policy is written down, so a
reviewer can argue with the rule without reading the walks it governs. Sections 5 and 7
below are largely about whether that document tells the truth.

---

## 2. Per-fixture results

Digests compared before and after; trial counts and commit counts compared alongside, because
an unchanged digest with a changed search would still be a defect.

| Fixture | Digest before | Digest after | Identical | Trials before → after | Wall before → after | Speed-up |
|---|---|---|---|---|---|---|
| `toed_slab` | `13198418135389558006` | `13198418135389558006` | **yes** | 0 → 0 | 2 292 ms → 1 525 ms | 1.50× (no repair loop; whole-run only) |
| `overhang_table` | `572621525405166924` | `572621525405166924` | **yes** | 0 → 0 | 7 170 ms → 6 495 ms | 1.10× (no repair loop; whole-run only) |
| `uncovered_rim_table` | `10727171327057596482` | `10727171327057596482` | **yes** | 2 → 2 | 10 358 ms → 8 789 ms | 1.18× whole-run; loop 6 920 → 5 871 ms |
| `raking_blade` | `refusal:3153703192024778472` | `refusal:3153703192024778472` | **yes** | 45 → 45 | 63 727 ms → 44 162 ms | **1.44×** whole-run; loop 62 118 → 43 031 ms; median/trial 1 398 → 957 ms (−31%) |
| `dust_shelf` | `refusal:6678279176435689841` | `refusal:6678279176435689841` | **yes** | 274 → 274 | 2 220 028 ms → 1 831 746 ms | 1.21× whole-run; median/trial 8 053 → 6 523 ms (−19%) |
| `half_buried_table` | *not reached* | *not reached* | **untested** | 89 → 114 *(at the kill)* | 1 123 613 ms → 1 149 999 ms *(at the kill)* | 1.25× per trial (12.62 → 10.09 s/trial) |

**Reading this table honestly — three caveats that must travel with the numbers:**

- **`raking_blade` is the only clean paired measurement.** Both arms ran back-to-back on a
  quiet machine. It is the figure to quote: **−31% per trial**.
- **`dust_shelf`'s absolute walls are inflated about 4×.** Both arms ran at load average ~20,
  against a quiet-machine control of 578 s. Only the *ratio* is usable, and the ratio is
  **−19% per trial** — not the −36% the implementing lane claimed for loaded conditions. That
  claim does not reproduce. The lane's quiet-machine claim (−30%) does.
- **`half_buried_table` produced no digest on either arm.** Both were bounded at 1200 s and
  neither reached a verdict, so there is nothing to compare. Its identity is **untested, not
  unchanged** — see section 6. The trial counts differ (89 vs 114) *because* both arms were
  cut off by a clock: the faster arm simply got further in the same wall time. That the search
  trajectory is the same is confirmed by `trials + remaining = 3435` on both arms, and 2 commits
  on both.

**Beyond the digests.** The whole per-trial record stream was dumped on both arms
(`PLINTH_REPAIR_STEPS_OUT`) and byte-compared: identical on all five fixtures that finished —
`toed_slab` 2 B, `overhang_table` 2 B, `uncovered_rim_table` 3 417 B, `raking_blade` 81 242 B,
`dust_shelf` 541 571 B. So not just the outcome but every per-trial outcome class,
condemnation flag, and gate vector is unchanged. Secondary counters reproduce too
(`dust_shelf`: `diversity_demotions=699`, `of_which_tried=265`,
`gate_rises[quarantined=75 unrouted_no_winner=55]`, `own_contact_condemned=176`).

The five digests also match the independently recorded `develop` control from 2026-08-26 in
`repair-ranking-veto-measurement/`, and `toed_slab`'s word is byte-equal to the frozen
`coverage_gate_toed_slab.json` golden in `plinth-verify`.

**Headline, stated without a thumb on the scale:** cost per trial falls between **15% and 31%**
depending on fixture, best measured on the cleanest paired run. Trial counts are unchanged
everywhere. Every gain is cost-per-trial.

---

## 3. What I verified myself

I did not take the equivalence report or the attack reports on trust. Independently confirmed
by reading the two revisions:

- **The fold order is byte-for-byte develop's.** I diffed `develop`'s single walk
  (`specialized.rs:1242-1302`) against the branch's three pieces. The authority prefix
  (`hash_specialized_plan_authority:8642-8662`) folds semantic digest → nested generic digest →
  mesh fingerprint bytes → reservations → limits → grid pitch → max grid bytes → tagged
  `next_contact_id`, exactly as develop did. The frozen bulk
  (`hash_specialized_plan_frozen_bulk:8681-8694`) folds repair pool → islands → coverage
  samples, exactly as develop did. Nothing is added, dropped, or moved.
- **The resume is fail-safe.** `SealedSpan::resume` is
  `(arriving == self.entry.get()).then(|| self.exit.get())`. `None` means "fold it yourself",
  never "the value is wrong". A wrong seal costs time, it cannot forge quietly.
- **The accumulator is a legitimate resume point.** `Fnv::finish` (`:8900`), `Fnv::state`
  (`:8871`) and `Fnv::resume` (`:8864`) are all bare `self.0`. No final mix.
- **Nothing was refrozen.** `git diff develop..feature/incremental-integrity` touches zero
  files under `crates/plinth-support/tests/` and zero files under `crates/plinth-verify/`.
  Grepping the diff for removed `assert_eq!` lines or removed digest literals returns nothing.
  Assertion count in `specialized.rs` rises 571 → 583; `runner.rs` is unchanged at 402.
- **No check was deleted.** No function was removed. `validate_binding` and
  `validate_edit_binding` now route through a new `validate_sealed_digests()` that is
  **strictly stronger** than develop's: it first compares the memo-served digest against a
  deliberate full walk with the span bypassed (new refusal `StaleFrozenBulkSeal`), and only
  then does develop's original comparison of `finalized_load_plan_digest` against that walk.

**A moved digest would have ended this review. None moved.**

---

## 4. Every attack run, and my ruling on each

Four adversarial lenses were run. All four are recorded below, including the one that found
nothing, because a null result from a lens that was genuinely exercised is evidence the cache
is sound.

### Lens A — Order and identity

*Does a cached subtree's value depend on anything outside itself — a global index, a sibling
count, a traversal position, a renumbering? Can a seal be read after the bytes it covers moved?*

**Method:** built a differential instrument. Added a `PLINTH_SEAL_PARANOID` switch making all
three memoized readers compare the memo against a fresh full walk on *every call* and panic on
divergence, then ran four fixtures through the real repair loop (~350 s of live search) and the
entire 652-test library suite under it. **Reverted afterwards; worktree left clean.**

**Result:** in every state the repair loop can actually produce, the seal is true — no paranoid
assertion fired in live search, and all four fixture digests matched a `develop`-built control
binary exactly. The instrument did surface three divergences in the library suite, all in
deliberate-sabotage tests; one of those was not accounted for by the branch (finding **F1**).

Attacks within this lens that *failed*, each of which is positive evidence:
renumbering and re-parenting (removal keeps `next_contact_id` verbatim, survivors are never
renumbered, appends bump it *ahead* of the span so an inherited seal can never unlock);
seal transplant (`derive_child_from` recomputes, never copies); interior mutability (none in
any sealed type); deserialization bypass (none of the three types derives serde, and
`SealedDigest` implements neither trait so a derive would not compile); `Forest` (deliberately
left uncached — correct, because slot recycling really can splice a member into the middle of
the order).

**My ruling: sound method, and I accept the null on the order/identity hypothesis.** I
independently re-derived the fold order (section 3) and reached the same conclusion. Finding F1
is confirmed below.

### Lens B — Rollback and reuse

*After a rejected repair trial, is every cached fingerprint restored to its pre-trial state, or
does a cache entry survive a rollback the structure did not?*

**Method:** a second, independently written differential (`PLINTH_SEAL_DIFFERENTIAL` and a
`_DEEP` variant that un-`cfg(test)`s the two value oracles), run over five fixtures including
`raking_blade` — the ideal fixture for this lens, since all 45 of its trials are rejected and
rolled back. Also a full static audit of the rollback path.

**Result: no stale entry found, and the reason is structural, not merely empirical.** There is
no cache entry that can outlive its structure, because **every memo on this branch is a field of
the value it describes, not a side table**. On rejection the controller drops the child plan
whole and never rebinds the parent; the plan moves only on commit. The only per-context digest
state is five `u64`s in `PreparedDigestSnapshot`, captured before the trial and restored on every
rejection arm. Nothing is invalidated on the way in, so nothing can fail to be restored on the
way out. The pre-existing test `a_rejected_trial_does_not_leave_its_digests_on_the_context`
already drives that cycle on real geometry.

This lens also mechanically diffed develop's three inline walk bodies against the branch's three
free functions, normalising only `self.x` → `x`: **textually identical** — 80 statements /
61 fold operations in the same sequence for the plan walk, 75 for the generic plan, 32 for the
selected contacts.

**My ruling: null accepted, and it is the strongest null of the four** — it is backed by a
structural argument (memo-as-field) that does not depend on any fixture happening to exercise the
path. Its four secondary findings are handled below (F2, F5).

**One caveat I record against this lane's own numbers:** it reported that four other lanes were
compiling concurrently during its run, and correctly declared all of its wall/trial figures void
as performance data. None of section 2's numbers come from this lane. It also did not run the
library suite or clippy under its armed differential, on the grounds that the fixture differential
covers strictly more production paths. I accept that trade — Lens A *did* run the full suite armed.

### Lens C — The other writers

*Every path besides the repair loop that can modify a sealed value's folded bytes, and whether it
invalidates.*

**Method:** instrumented all three memo readers to assert memo == full re-derivation on every call,
ran the full `plinth-support` library suite (652 tests, 256 s), then wrote a self-contained
reproducer, ran it green, and removed it. Plus a complete static writer inventory for all three
sealed types.

**Result: found the same defect as Lens A, independently, from a different direction** — the
`SelectedContacts` visibility hole (F1). Its writer inventory is the most valuable artifact here:
`SelectedContacts` has one constructor, no `&mut self` method, and exactly one field writer;
`GenericContactPlan` has one construction site, zero `&mut self` methods, and zero field
assignments anywhere in `perception.rs`; the specialized plan's frozen bulk has exactly two struct
literals, both re-sealing from the same locals the literal takes, and all four public edit APIs
funnel through `derive_child_from`.

**My ruling: sound, and the convergence matters.** Two lenses built independent instruments and
landed on the same finding. That is the opposite of a fluke.

### Lens D — Forest and the verification snapshot

*Bounded review of the named `Forest` and `SupportVerificationSnapshot` paths.*

**Result: no findings.**

**My ruling: a genuine but NARROWER examination than the other three — recorded as partial
coverage, not as a clean bill of health.** It is not an empty return: it names specific
inspected sites (`Forest::validate` at `forest.rs:1350`, `Forest::digest` at `:1468`,
`measure_coverage` at `runner.rs:3739`, `validate_integrity`'s re-derivations at `:4038` and
helpers at `:7713/7727/7988/8017/8051`, snapshot sealing at `:6564`) and states a correct
substantive conclusion: `Forest::digest` builds a fresh `DigestWriter` and has no memoized
result, so members, quarantine, owners and certificates have **no staleness surface at all** —
which independently corroborates the branch's decision to leave `Forest` uncached. But this
lane explicitly did not build, profile, run tests, or construct a differential. It is a
static read only. Its null is worth less than Lens B's null, and I do not count it as an
adversarial test of the seal.

---

## 5. Findings, ruled

I checked every finding against the source myself. All six below **reproduce**. None of them
moves a digest, and none is a live defect in a production run today. What they are is a set of
places where the *document* claims a guarantee the *compiler* does not provide.

### MUST FIX BEFORE MERGE

**F1 — The `SelectedContacts` seal has no compiler barrier, and a writer that leaves it stale
already exists in the tree.**
`lib.rs:1027-1038`, writer at `specialized.rs:15452`, false claim at `digest_seal.rs:79`.

**Confirmed by me.** `SelectedContacts` is declared in the **crate root module** (`lib.rs`), so
its unmarked fields `contacts`, `coverage_samples` and the `digest` seal itself are visible to
**every module in `plinth-support`**, not private. The proof is already committed:
`specialized.rs:15452` does `tampered_regular.contacts[0].point_mm[0] += 0.01;` from another
module, and nothing re-seals. I read both sites. That line compiles, which is a demonstration
that the barrier does not exist.

The field's own doc-comment says *"Both fields above are private, this type has no `&mut self`
method"*, and the rule table at `digest_seal.rs:79` says *"private fields, no mutator, replaced
when selection re-runs"*. The second and third clauses are true. **The first is factually false**,
and it is the clause that carries the whole safety argument.

The measured consequence: the seal serves `1991229210673782323` while a full re-derivation of the
same value gives `4940958918354116667`. That stale word then folds into `bound_authority_digest`,
so the source-authority word no longer moves when the hand-off is written to. On develop it did.

**What still saves it, and why this is not a merge-stopper on behaviour:** `validate_regular`
(`specialized.rs:2509`) is a disjunction — `replay.digest() != regular.digest() || replay != *regular`
— and I confirmed the **structural** half still fires, so `StaleRegularContacts` is still raised and
the sabotage test still passes. The digest half is dead for exactly the sabotage it was written for.
The writer is `#[cfg(test)]` (inside `mod tests` at `specialized.rs:8906`); **no production writer
exists.**

**Ruling: must fix, and the required fix is mechanical.** I am not blocking on behaviour — behaviour
is unchanged. I am blocking because this branch's entire safety argument is prose, and shipping a
sentence that says "private fields" three lines from committed code that writes those fields is
precisely the failure this project exists to refuse. The contrast proves it is fixable:
`GenericContactPlan` is declared in `perception.rs`, a non-root module, so **its** identical claim
is genuinely true — I verified its fields are unmarked and it has no `&mut self` method. Same
sentence, one true, one false, purely because of which file the type lives in.

*Fix:* move `SelectedContacts` into its own module so the fields become genuinely private, making
the existing comment true. (`SealedDigest::seal` is `pub(crate)`, so a forged
`x.digest = SealedDigest::seal(0)` also compiles today; the module move closes that too.) If the
module move is judged too invasive for this branch, the *minimum* acceptable alternative is to
correct both the comment and the table row to state what is actually enforced — but the module
move is better and costs nothing at runtime.

**F2 — Obligation 3 is overstated: the "full walk" is not full, and two checkers became
tautologies.**
`digest_seal.rs` obligation 3, against `specialized.rs:1289/8646`, `perception.rs:972`, and
`runner.rs:6826-6827/6913-6914`.

**Confirmed by me, in three parts.**

*Part one:* obligation 3 promises the checking site *"calls the FULL WALK with the memo bypassed"*.
`SpecializedContactPlan::digest_full_walk` (`:1289`) is just `self.fold(SpanPolicy::AlwaysFold)` —
it bypasses the **span** memo only. Its authority prefix still executes
`hash.u64(generic_authority.digest())` at `:8646`, which on this branch is a memo. So
`validate_sealed_digests` compares memo-with-nested-memo against full-walk-with-the-same-nested-memo.
For the nested generic value seal, the checker is not honest.

*Part two:* `GenericContactPlan::digest_full_walk` is `#[cfg(test)]` (`perception.rs:972`), and so is
`SelectedContacts::digest_full_walk` (`lib.rs:1081`). **In a release build, nothing anywhere
re-derives either of the two value seals from its fields, ever.** The span, by contrast, gets an
honest full walk about four times per trial. The backstop is asymmetric and the asymmetry is not
stated.

*Part three:* `runner.rs:6826-6827` and `:6913-6914` test
`self.generic.digest() != self.digests.generic || self.regular.digest() != self.digests.regular`.
I traced both operands: `digests.generic`/`.regular` are stamped exactly once, at `runner.rs:7463-7464`,
from the very objects moved into the same context, and I grepped — **`self.generic` is never
reassigned anywhere in `runner.rs`**. So both sides now resolve to the same sealed word. These are
**self-comparisons that cannot fail**. On develop each was a live re-walk against a recorded word.
Obligation 3's own text says *"Serving a memo to a checker would make the comparison a tautology and
silently retire the check"* — which is exactly what happened at four sites the branch's audit never
enumerated.

**Ruling: the document must be corrected before merge; the code hardening can be tracked.**
No digest moves and nothing is reachable today — `GenericContactPlan` genuinely has no mutation path
(verified), and `validate_binding` still does a deep structural `generic != &self.generic_authority`
compare at `specialized.rs:1203`. But an *incomplete list of the places a check went memo-served* is
the one thing this doctrine cannot ship, because the list is the artifact the next author trusts.
The branch declared one exception openly and well — the hot borrow gate in `finalized_loads()`, whose
comment at `specialized.rs:1135-1138` I read and which is honest about itself. It must declare the
rest.

*Fix (mandatory):* correct obligation 3 to say the full walk bypasses the **span** memo and folds the
nested generic seal; add the two runner preflight sites, the nested generic seal, and
`validate_accepted_contact_set` (F5) to the disclosed exception list.
*Fix (optional, track):* make `digest_full_walk` release-available on the two value types and call it
at the checker sites — two extra walks per stage preflight, against the ~80 per trial the branch
removed.

### CAN MERGE AND TRACK

**F3 — The two new value-seal oracle tests are structurally incapable of failing.**
`perception.rs:10046`, `lib.rs:3859`.
Confirmed: `assert_eq!(plan.digest(), plan.digest_full_walk())` runs microseconds after the seal was
folded from the same immutable data, then repeats on a `Clone`. No mutation happens in between, so
both sides are the same pure function over the same bytes. These prove the walk is deterministic;
they cannot prove a seal is fresh, and they would stay green under both F1 and F2. Contrast the
span's two tests — `tampering_inside_the_sealed_span_is_refused_by_the_binding_check` and
`a_derived_child_seals_its_own_span` — which do have teeth because they force the memo and the walk
apart. **Track:** give the value seals tests of that shape — mutate the owner through a `cfg(test)`
hook, then assert a *checking site* refuses.

**F4 — `derive_child_from` seals and stores the span from two independent reads.**
`specialized.rs:1631-1649`.
Confirmed: the seal takes `&self.plan.repair_pool / .islands / .coverage_samples`, and the struct
literal separately clones the same three. Today the two reads agree, so nothing is wrong.
`prepare_specialized_contacts` deliberately hoists its authority values into locals precisely so its
seal and its literal share one binding; the child path does not do the same for the span. A future
child whose span legitimately differs from its parent's would be sealed over the parent's bytes with
no compiler complaint — it fails closed at the next binding check, but the reported digest is wrong
in between. **Track:** bind the three cloned vectors to locals and pass `&local` to the seal, matching
the other literal. Cheap hardening, no behaviour change.

**F5 — `validate_accepted_contact_set` is a third undisclosed memo-served checker.**
`runner.rs:4980-4981`.
Confirmed: `accepted.digest() != state.accepted.digest || accepted.finalized_load_plan_digest() != accepted.digest()`
— all three words are now memo-served. On develop all three were full walks. Behaviour is unchanged
because the per-trial `validate_sealed_digests` covers it. This is a completeness defect in the
exception list, folded into F2's mandatory documentation fix.

**F6 — Entry-state collision (raised and dismissed).**
Two different prefixes colliding at the 64-bit entry accumulator would resume a stale exit. This is
the same 64-bit strength as the digest it protects, and develop had the identical exposure, so it is
**not a regression**. Recorded only because the rule text's confidence slightly outruns what the
mechanism delivers. No action.

---

## 6. Lenses NOT covered

Named plainly, so nobody reads this document as broader than it is.

1. **`half_buried_table`'s digest identity is untested.** Both arms were censored at a 1200 s bound
   and neither produced a digest. The largest fixture in the set has *no* equivalence evidence. It is
   untested, not unchanged. This is the single largest hole in section 2.
2. **Concurrency and thread-count variation.** Every measurement was taken at one thread count. The
   plan digest folds `slice_effective_threads` and `slice_participating_threads`, so a run at a
   different thread count is a *different input* to the fingerprint. Nobody checked whether the seal
   behaves under a changed thread count, and nobody exercised the seal from more than one thread.
3. **Cross-platform / cross-architecture reproduction.** All runs were on one macOS machine. FNV over
   f64 bit patterns should be architecture-independent, but nothing here demonstrates it. If digests
   are ever compared across machines, that is unproven.
4. **Peak memory.** Profiling attributed 14.3% / 9.7% to allocation and copying, and the span's three
   sources are still cloned per derived plan. Nobody measured whether peak memory moved. Against this
   project's 100 GB working ceiling that is worth one measurement, though it is not a correctness lens.
5. **Lens D's boundary was read, not attacked.** Section 4 records why its null carries less weight
   than the other three.

---

## 7. Merge recommendation, and what a merger must do by hand

**Recommendation: MERGE, after the two corrections below.**

The constraint this task was given was: every fingerprint bit-identical, no check deleted, no test
refrozen. All three hold, and I verified all three myself rather than accepting them:

- Five of six fixtures reached a verdict and produced **bit-identical** digests, identical trial
  counts, identical commit counts, and **byte-identical per-trial record streams** (541 KB of them on
  `dust_shelf`). The sixth produced no digest on either arm.
- **No function was removed**, no assertion was deleted, and assertion count rose 571 → 583 in
  `specialized.rs`. The stage-boundary check is **strictly stronger** than develop's.
- **Zero test files and zero goldens are in the diff.** Nothing bought its green by moving a number.
- Suite is green against the one accepted by-design red
  (`closure_repairs_censored_dust_because_that_repair_closes_a_real_hole`): 831 passed across other
  crates, 651 in `plinth-support --lib`, all integration targets green, `support_runner_integration`
  showing zero failures before its expected bounded kill. Clippy exits 0; every file the change
  touches is `fmt`-clean.

**Before merging, by hand:**

1. **Fix F1.** Move `SelectedContacts` into its own module so its fields are genuinely private, making
   the existing doc-comment true and closing the `SealedDigest::seal` forging path. If that is deferred,
   the comment at `lib.rs:1030-1036` and the table row at `digest_seal.rs:79` **must** be corrected to
   state what is actually enforced. Do not ship the sentence as written.
2. **Fix F2's documentation half.** Correct obligation 3 in `digest_seal.rs` to say the full walk
   bypasses the *span* memo and still folds the nested generic seal, and extend the exception list from
   one entry to four: the `finalized_loads()` borrow gate (already honest), the two runner preflights at
   `runner.rs:6826/6913`, the nested generic seal at `specialized.rs:8646`, and
   `validate_accepted_contact_set` at `runner.rs:4980`.
3. **Keep the measurement harness out.** `crates/plinth-support/examples/repair_baseline.rs` lives on
   `measure/repair-baseline` and must not enter the merge. It is untracked in every verification
   worktree; confirm before merging.
4. **Note the environment fix, which is not part of this change.** A fresh worktree does not carry
   `corpus/models`, so `plinth-decimate`'s `corpus_budgets` test fails with *"required corpus/models
   directory is missing"* until it is linked from the main checkout. Unrelated to this branch; do not
   let it read as a regression.
5. **Two pre-existing `fmt` failures are not yours.** `crates/plinth-hollow/examples/wall_distribution.rs`
   and `crates/plinth-hollow/tests/repair_gate.rs` are dirty on develop and untouched here.

**Track after merge:** F3 (give the value seals tests with teeth), F4 (single-read the child span), and
optionally F2's code half (a release-available full walk at the value-seal checker sites).

**What would have stopped this:** a moved digest. None moved.
