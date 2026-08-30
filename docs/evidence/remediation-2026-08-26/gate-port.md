# Two pre-write gates, ported and ruled

Ruling date: 2026-08-29.
Judged at `c86f065` (the two porting lanes, plus one verification lane and one
attack lane, merged for review). Nothing in this ruling was merged or pushed.

---

## The verdict

**Both gates refuse. Both accept a sound record. Neither re-runs the engine.**

I reproduced every refusal on this machine, from records that real support runs
wrote down, and I read every line of both judging functions and of the five
accessors they read from. The gates are not decoration.

**Recommended: merge both.** Do not weaken either one to make the test suite
green. Three things must happen by hand first, and one of them is a decision
only the owner can make. They are listed in section 5.

The honest headline underneath the verdict: the deleted auditing crate was
12,751 lines. This is two gates and about 700 lines. Section 4 says plainly
what is still unchecked, because the gap is far larger than what was restored.

---

## 1. What the two gates check, and what it means if one fires

Before this work, a model reached the printer file after **one** of twelve
named safety checks had actually run. The other eleven were named in the code,
reported as "not yet ported", and decided nothing. A separate crate used to
read the finished file back and audit it independently; that crate was deleted
on 2026-08-26 (commit `3913d50`). So the pipeline had no independent check of
its own output at all.

Two of the eleven now run.

### Gate `Load` — the support area law

**What it checks.** Every support arm the run will actually print must be thick
enough for the piece of the model it is charged with holding up. The law, as
ratified:

```
pi * (tip radius)^2  >=  (area that tip carries) / 12
```

The `12` is the registered allowance `X_ALLOWED`. It comes from a physical
test plate: arms broke at a ratio of about 25, and the design limit is set at
12 for roughly a factor-of-two margin. The gate reads the allowance out of the
constants registry, so no caller can hand it a softer number.

**What it means if it fires.** The run planned an arm too thin for its load.
On the printer, that arm is the narrowest section in the load path, and the
narrowest section is where a part breaks — during the peel of each layer, when
the plate is scraped, or in a player's hand. A refusal stops the model before
the printer file exists.

**What it does NOT mean.** A refusal can also mean the run's own record of
"which arm carries what" is wrong. Both readings demand the same answer: a run
that cannot state what its arms carry has not shown that they are safe. The
gate refuses either way. This is deliberate.

### Gate `UnroutedSpecializedContacts` — the section 6 hard failure

**What it checks.** Some contact points carry a *named obligation* rather than
just a share of the load. Two kinds exist:

* a **specialized minimum** — a place the planner decided must have a support,
  not merely one that would be nice to have;
* an **island birth** — a piece of the model that first appears in mid-air on
  some layer, with nothing below it. It must be caught on the layer it is born,
  or it is not attached to anything.

The routing stage sometimes fails to seat a support at such a point. The
ratified specification (`docs/specs/coverage-repair.md` section 6) calls that a
hard failure in as many words: *"An unrouted specialized minimum or island-birth
contact is a hard failure."* The run stage measures it and writes it into a
ledger, but it does not refuse. This gate is where the refusal now happens.

**What it means if it fires.** A part of the model is born in the resin with
nothing holding it. It falls away, sinks, and lands on the film at the bottom
of the vat, where the next layer presses the plate onto it. The usual outcomes
are a missing limb, a hole punched in the film, or both.

### Neither gate re-runs the engine

This was the mandatory condition on this work: a checker that calls the thing it
checks cannot catch that thing's mistakes, and re-running the slowest stage
would also double the suite's runtime.

The proof is in the two signatures. They are the whole input:

```rust
judge_emitted_load_law(&[EmittedContactDisclosure], usize)
judge_unrouted_specialized_contacts(&[UnroutedSpecializedContact], usize, usize, usize)
```

There is no mesh, no surface, no spatial index, no plan and no run in either
argument list. There is nothing present to re-run a stage *from*. The five
things the wired gate reads are a stored list, three `len()`/`count()` calls
over stored lists, and one stored count. The arithmetic is one multiply, one
divide and one compare per row for `Load`, and one addition plus one
"is this list empty" for section 6.

---

## 2. The refusal evidence

All of the following I ran myself in this worktree, at `c86f065`, and read off
the test output. The records come from real support runs on two procedural
fixtures, not from hand-written data.

### `Load`

**The sound record that must clear.** `make_box`'s own emitted-contact record —
90 rows off a real sealing run — with every row given exactly the tip radius the
area law demands for the area that row really carries. This clears, which is
what proves the gate is not simply refusing whatever it is handed.

**The sabotage.** That same sound record with a single row's tip cut to one
tenth. The gate refuses, and names that row and no other:

```
gate Load refused emission: 1 of 90 emitted contact(s) do not satisfy the
registered support area law pi * tip_radius_mm^2 >= carried_area_mm2 /
X_ALLOWED, evaluated per contact on the final numbers with the registered
X_ALLOWED = 12:
  contact 46 at (-7.500, -5.625, 0.000) mm: carries 450.0000 mm^2 through an
  emitted tip of radius 0.3455 mm (cross-section 0.375000 mm^2), but the area
  law demands a tip radius of at least 3.4549 mm (cross-section 37.500000
  mm^2). The emitted tip is 10.00x too small in radius
```

**The record as the engine really shipped it.** `make_box` is refused today, on
three rows:

```
gate Load refused emission: 3 of 90 emitted contact(s) ...
  contact 19 at (-15.000, -5.625, 15.938) mm: carries 450.0000 mm^2 through an
  emitted tip of radius 0.0500 mm (cross-section 0.007854 mm^2) ...
  The emitted tip is 69.10x too small in radius
  contact 46 at (-7.500, -5.625, 0.000) mm: ... 69.10x too small in radius
  contact 76 at (7.500, 3.750, 0.000) mm:  ... 69.10x too small in radius
```

Three arms, each 0.05 mm across at the tip, each charged with a whole 450 mm²
face. This is not a new defect the gate invented. It is task #68's
single-carrier load partition, and the accepted red test on `develop` already
says so in its own panic text: *"Seven contacts still carry more than the area
law allows one contact to carry."* The gate found, from records alone, a defect
that was measured and written down before it existed.

### `UnroutedSpecializedContacts`

**The real record.** `make_toed_slab`'s own section 6 disposition record, off a
real sealing run: 68 contacts planned, 64 routed, 3 ledgered as ordinary
removals, 1 ledgered as an unrouted contact that was still carrying an
obligation. I confirmed the section 6 identity — planned equals routed plus both
ledgers — holds exactly on that run before treating anything below as a test of
the ledger rather than of the record check.

**The refusal:**

```
gate UnroutedSpecializedContacts refused emission: 1 of the 68 contact(s) this
run planned carry a demand the routing cadence did not route. SPEC-A3-A4
section 6: "An unrouted specialized minimum or island-birth contact is a hard
failure." The run's own accepted-contact-set ledger names them:
  contact 64 (terminal 64) was carrying a specialized minimum, and it was
  quarantined before the routing cadence ran, so no seat was ever attempted
  for it
```

**The accept-when-sound proof.** The same record with that one contact's
disposition moved from "unrouted, carrying an obligation" to "routed" — which is
what actually routing it would have written — clears. The refusal was that row,
not the gate refusing everything.

No row here is forged, and none could be: `UnroutedSpecializedContact` has
private fields and no constructor, so a test cannot invent one. The refused row
is the row the engine itself wrote.

### The vacuity sabotage, on both gates

A gate whose test is "this list is empty" passes for two different reasons, and
only one of them is safety: the run met every obligation, or the run failed to
write the list. Both gates were attacked on exactly that.

* Drop one row from `Load`'s sound record: refused as `LoadRecordIncomplete`.
* Hand the section 6 gate an **empty** ledger on a record whose dispositions no
  longer add up: refused as `SpecializedDispositionUnaccounted`, with the
  message *"an empty unrouted-demand ledger cannot be read as 'every obligation
  was met'"*.

Both guards work as written. Section 4 records where they are weaker than their
own documentation claims.

### Test result

`verify_gate_port` 2 passed, `load_gate_area_law` 3 passed,
`unrouted_specialized_gate` 1 passed, `gate_port_adversarial` 11 passed;
0 failed across all four.

---

## 3. How many gates remain unported, and what the disclosure now says

**Nine of twelve.** Before this work it was eleven of twelve.

Still returning "not yet ported", and therefore deciding nothing:

`Clearance`, `UnsupportedMinima`, `IslandBirths`, `RegularContactInvariants`,
`Connectivity`, `Printability`, `Murray`, `BallBreach`,
`PhysicalContactInvariant`.

Now reaching a verdict: `UncoveredIslands` (measures coverage; the one that
already ran), plus `Load` and `UnroutedSpecializedContacts` (read records).

**What the disclosure says.** Every run report carries a `pending_gates` list.
It is not written by hand and it is not a copy of anything: it is filled at run
time from the branch the gate dispatcher actually took, so it states what ran
rather than what a comment claims ran. No count of the ported set is now
written in prose anywhere in the production code — three separate comments used
to state a count, and all three were wrong by one after the second gate landed.
They were corrected on the verification branch (`f7d0b03`).

The end-to-end test that pins this list was changed from a **count** to the
**names**, which is the right change: a count cannot tell "one gate was ported"
apart from "one was ported and another was lost".

**A deviation from the plan, and why it is correct.** The remediation plan
(`docs/evidence/remediation-2026-08-26/PLAN.md`, item 7.1) named `Load` and
`BallBreach` as the two to port. `BallBreach` asks whether a support ball
breaks through the surface of the model — a question that cannot be answered
from a recorded number, only by querying geometry. Porting it would have
re-entered the engine, which the plan's own mandatory condition forbids. The
lanes substituted `UnroutedSpecializedContacts`, which is answerable from the
ledger the run already writes. I rule the substitution correct, and `BallBreach`
stays on the list of nine.

---

## 4. What is still not checked — the size of the remaining gap

Be clear about the arithmetic. The deleted crate was **12,751 lines**. This
work adds about **700**. Two gates of twelve is not a replacement for an
independent auditor, and nothing in this ruling should be read as closing that
hole.

### The deleted auditor read the FILE. These gates read the PLAN.

This is the most important thing missing and it is easy to overlook. The
deleted crate opened the finished `.chitubox` file, rebuilt the geometry out of
the bytes, and audited that. These two gates read the run's own in-memory
record just before the file is written.

So there is now **no check at all that the bytes written match the plan that was
judged.** A defect between the gate's verdict and the file on disk — in
lowering, in serialisation, in the writer — is invisible to everything the
project still has.

### Named things the deleted crate checked that nothing checks now

* **The independent scar audit** (3,482 lines). It owned its own contact
  reconstruction and its own delicacy field, with no dependency on the producer's
  mesh or verify crates, and graded every embed against `SCAR-AREA-CAP`.
* **`SCAR-AREA-CAP` itself.** That ratified specification names two
  implementations that must agree. Both are gone. This work does not restore
  either one. The ported `Load` gate restores only the auditor's single support
  hard bound — per-arm `max_x <= 12` — and restores it on the plan rather than
  on the file.
* **Independent mesh topology audit** (804 lines): shells, boundary edges,
  signed shell volumes, orientation. A hollow shell wound the wrong way was
  caught there and is caught nowhere now.
* **The M1 metric set**: `max_x`, `support_count`, `resin_model`,
  `resin_support`, `resin_total`, computed from the written artifact. The resin
  numbers — the whole point of the hollowing work — are no longer measured
  independently at all.
* **Corrupt-artifact detection** and the independent disk reader (290 + 233
  lines of tests over 684 lines of reader).

### Named weaknesses inside the two gates that DID land

I verified each of these against the source myself. None of them stops the
gates from biting; all of them bound how much they are worth.

1. **`Load` has no provenance binding.** The only thing tying the record it
   judges to the plan it is judging is a row **count**. One safe row repeated 90
   times clears. Another model's rows, sized to the law, clear. This is not
   reachable in the wired pipeline today — the record and the count both come
   from the same sealed run — but both judging functions are now public API, so
   the next caller gets no protection. The cheap fix is to compare the set of
   contact ids, not the length.

2. **`Load`'s completeness check cannot fire in the wired path.** Its
   documentation calls the two counts *"two independent walks of the same
   finalized transcript"*. They are not independent. I traced both:
   `emitted_contact_disclosures` emits one row per transcript row whose kind is
   `Routed`, and the sealed snapshot's routed-contact projection filters the same
   rows on the same kind. The counts are equal by construction on every real
   run. A lost `Routed` row disappears from both sides at once, and the gate
   clears. **The documentation claim is false as written and must be corrected**
   — see section 5. The check should stay; what it actually binds is the record
   to the transcript, which is worth having. What it does not bind is the record
   to the supports that are really in the emitted scene, and nothing else binds
   that either.

3. **Nothing checks that carried area is conserved.** `Load` judges each row's
   own ratio and never the sum. On `make_box`, 87 of the 90 rows record a
   carried area of exactly `0.0` and clear at any positive tip; the whole 1350
   mm² is charged to the remaining three. That concentration is what the gate
   catches. A defect that *loses* area instead of concentrating it would leave
   every row clearing. The engine has a conservation assertion, but only inside
   one unit test on one fixture — not at the gate.

4. **The section 6 gate never opens the ordinary-removal ledger.** Unrouted
   contacts are sorted into two ledgers by asking whether the contact carries any
   obligation. The gate refuses on one ledger and never reads the other. Any
   upstream defect that empties a contact's obligations before the sort files a
   real hard failure as an ordinary removal, where this gate is structurally
   blind to it. The counts still add up, so the vacuity guard does not fire.

5. **The section 6 identity is a checksum, not a set of counts.** It constrains
   one difference, so any compensating pair of errors preserves it. Planned 68 /
   routed 65 / removals 3 adds up as readily as the true 68 / 64 / 3 + 1.

6. **A cleared gate leaves no record that it read anything.** The "cleared"
   verdict carries no evidence, and the run report keeps nothing about what
   either gate judged — no rows-judged count, no allowance, no ledger sizes. A
   reader downstream cannot tell "90 rows judged against the area law" from "0
   rows judged".

7. **A documented arithmetic tolerance of 8 units in the last place.** A tip
   sized to sit exactly on the law lands a few bits under it when the gate
   squares it back up, so the comparison carries about 1.8e-15 of relative
   slack. This is real — a cross-section exactly at the threshold clears — and it
   is harmless: the smallest genuine violation the gate has been measured to
   refuse is thirteen orders of magnitude above it. It is pinned by a test, so
   any widening moves an assertion.

### And the nine that still decide nothing

`Clearance` is what would catch a support arm passing through the model.
`BallBreach` is what would catch a support ball breaking the surface.
`Printability` is what would catch a feature below the printer's resolution.
`Connectivity` is what would catch a support tree not joined to the plate.
None of them runs. A model that fails any of the nine still reaches disk.

---

## 5. Merge recommendation, and what a merger must do by hand

**Recommendation: merge both gates. Do not weaken either.**

They do the thing this task set out to prove, on real records, without
re-entering the engine. The area-law gate found a live defect that had already
been measured and written down independently, which is the strongest evidence
available that it is judging something real.

Three items must be handled by hand.

### A. Correct the false independence claim — required, cheap, before merge

The documentation on `GateRefusal::LoadRecordIncomplete` says its two counts
come from "two independent walks of the same finalized transcript". They come
from two walks of the same list, filtered on the same condition, and are equal
by construction. Correct the comment to say what the check actually binds — the
record to the transcript — and say plainly that nothing yet binds either to the
supports in the emitted scene. Do not delete the check.

This is a comment fix, not a gate change. It is item A because a claim of
independence that does not hold is exactly the class of defect this whole
remediation exists to remove.

### B. The suite goes red, and only the owner can rule on landing it — required

Landing these gates refuses every current fixture, because every current
fixture really does violate the area law. The lanes report the workspace going
from 3 red to 20 red in a fresh worktree, all with the same cause; I did not
re-measure the full suite and do not vouch for the exact numbers, but the cause
is not in doubt — I reproduced the refusal on both fixtures directly.

There are three ways forward and the choice is not a verification lane's:

1. Fix task #68's single-carrier load partition first, then land the gates onto
   a green suite.
2. Land the gates now, with the red recorded as a ruling and #68 named as the
   one blocker to going green.
3. Land `UnroutedSpecializedContacts` alone and hold `Load` until #68 is fixed.

My recommendation is (2): the red is the gate reporting a defect that is
already known, already measured, and already has an accepted red test naming
it. Hiding it costs more than carrying it. But (1) is defensible and the choice
is the owner's.

**Whatever is chosen, do not take the fourth option.** Do not loosen the area
law, do not raise the allowance, and do not disconnect the `Load` gate from the
dispatcher to get green. That ships the decoration this task exists to prevent.

### C. Re-point the masked alarm — required before merge

`a4_noncoverage_gate_drift::a_ported_gate_must_have_an_a4_component` is the
alarm that fires when a gate starts reaching a verdict without anywhere for its
quantity to go. It reads the ported set out of a real pipeline run — and that
run now refuses, so the alarm panics before it can read anything.

I checked the answer it would have given: both newly ported gates **do** have a
component in the non-regression vector (`LoadViolations` and
`UnroutedSpecialized`), so the alarm would pass. It must be made able to say so
again, by fixing the fixture's underlying defect or by re-pointing it at a model
that emits. **It must not be deleted or ignored.**

### Smaller notes for the merger

* `Load` ships before `UnroutedSpecializedContacts` in the gate order and
  refuses every model today, and the dispatcher stops at the first refusal. So
  the second gate's wiring is currently unreachable at run time. Its judging
  function is proven in both directions and its dispatch branch is proven only
  by the compiler's exhaustiveness check. The lanes disclosed this and pinned
  the ordering, which is the right handling — but the gate is one refactor away
  from being disconnected with nothing failing.
* A negative zero passes the "is this a real area" guard, because the guard
  tests `< 0.0` and `-0.0 < 0.0` is false. It then behaves identically to a
  positive zero, so this is cosmetic, not a hole. Fix it when the conservation
  check in section 4 item 3 is added.
* Do not wait for a `test result` line from
  `plinth-support --test support_runner_integration`. It contains an unbounded
  search and never prints one. Bound it with `gtimeout` and read what reported
  before the kill.
* `docs/remediation-tracker.md` rows 7.1 and 7.6 should be written back:
  7.1 is now partly done with a recorded substitution (section 3 above), and
  7.6 — schedule the remaining gates — is now a set of **nine**, not eleven.

---

## What I checked, and how

* Read both judging functions, the gate dispatcher, the emission door, and every
  accessor they read from (`crates/plinth-pipeline/src/gates.rs`,
  `crates/plinth-support/src/runner.rs`).
* Traced the two "independent" transcript walks to their sources and found them
  to be the same walk — finding 2 in section 4 is mine, from that read.
* Ran the four gate test binaries at `c86f065` and read the refusal text off the
  output rather than off a report: 17 tests, 0 failed.
* Counted the zero-area rows in `make_box`'s real record directly from the test
  output: 87 of 90.
* Confirmed the pending-gate list is nine at `c86f065` and eleven at `develop`,
  by reading both dispatchers.
* Read the deleted crate at `3913d50^` to size what is not restored.
* Ran `cargo clippy --workspace --all-targets --all-features --offline -- -D
  warnings` at `c86f065`: exit 0, no warnings.

I did not re-run the full workspace suite, and I did not run any corpus model.
