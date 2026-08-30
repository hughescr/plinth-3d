# Is the testing doctrine in force?

Adjudication of the 2026-08-29 doctrine remediation. Branch under judgement:
`verify/doctrine-in-force` @ `eb224e0af5291ddf3b092394bc1b6e83c7987cd2`, off `develop` @ `d1f53ea`.
Every number below was produced in this lane unless it is explicitly attributed to another lane.

## Headline

**The doctrine is in force. A legitimate placement change now lands without a wall of red:
in my own run, reversing a support-routing tie-break broke exactly two tests — the perturbed
function's own contract test, and a quality ratchet reporting a real 7.5% resin regression in
a sentence a human can judge.** Not one test failed because a plan fingerprint moved, including
all four sites that previously pinned one; the retirement did land, and it landed complete.

Two things the headline does not cover, and the owner needs both. The second half of Ruling 1
never landed: the gate warns as ruled, but nothing yet stops the violation count getting worse.
And the retirement traded a blunt check for narrower ones in three places where nothing replaced
what was lost — I measured two of those myself, and one of them lets the engine ship a support
it cannot describe while two quality numbers *improve*.

---

## 1. What the argument is about, in plain terms

This project generates the scaffolding that holds a miniature to the build plate of a resin
printer. The scaffolding is computed, so "did the program produce the right answer" has no
single right answer: many different support layouts are acceptable, and improving the layout
algorithm changes the output **on purpose**, every time.

Five terms, defined once, used throughout:

- A **fingerprint** is a single number — a digest, a hash — that summarizes an entire computed
  support plan. It answers "did the output change" and nothing else. It cannot tell an
  improvement from a regression, because both change it.
- An **invariant** is a property true of *any* acceptable plan, whatever the layout: no two
  supports occupying the same point on the model, no support ending in mid-air. An invariant
  is never updated to match new output; a failing invariant is a bug.
- A **quality ratchet** is a recorded number, stored per test model, saying how *good* a plan
  is — grams of wasted resin, the worst-loaded support tip. It may improve freely and silently;
  making it worse requires a written argument in the same commit. It has a direction, which is
  what a fingerprint lacks.
- An **equivalence check** is a one-time claim — "this refactor did not change the output" —
  proved by running both versions at the time the claim is made, and recorded in that commit.
  It is not a standing expectation later commits must keep satisfying.
- A **gate** is a check run just before the program writes its output file; it can refuse to
  write. **Debug assertions** are internal consistency checks compiled into the slow
  development build and compiled out of the fast one, so the two builds can disagree about
  whether the program crashes.

The problem this work exists to fix: the test suite had frozen several support-plan
fingerprints as permanent expectations. A legitimate improvement to support placement changed
those fingerprints and therefore failed the suite — and, worse, the fingerprint assertions sat
*ahead* of genuine invariant checks in the same test functions, so when a fingerprint moved,
those invariants silently stopped running. The doctrine's rule is: fingerprints are
equivalence checks with a one-commit lifetime, not standing expectations.

The single question this workflow exists to answer is therefore: **can a legitimate placement
change now land without a wall of red?**

---

## 2. The acceptance test, run in this lane

The perturbation is the one the workflow specifies: reverse the final tie-break in
`routing_order` (`crates/plinth-support/src/initial.rs`, both sort comparators), which changes
the order supports are routed in when every earlier comparison ties. It is a *legal* change —
the resulting plan is still a valid plan — and it is exactly the kind of change a real
placement improvement makes.

Method: run the whole suite on a clean tree, apply the perturbation, run it again, revert.
Both runs used the mandated command, with the two known non-terminating tests skipped (see
§6):

```
cargo test --workspace --offline --no-fail-fast -- --skip a_spec6_removal --skip closure_repairs_censored_dust
```

Debug assertions ON. Perturbation confirmed applied at `initial.rs:1709` and `:1714`
(`.then_with(|| right.terminal.cmp(&left.terminal))`), and confirmed reverted afterwards.

### Result

| run | test binaries | passed | failed | ignored |
|---|---|---|---|---|
| clean tree (baseline) | 108 | 1547 | **1** | 7 |
| perturbed | 108 | 1546 | **2** | 7 |

**The clean baseline is not zero failures.** One test fails on an untouched tree:
`triage::tests::every_cap_minus_one_refuses_before_input_proportional_triage_work_starts` in
`plinth-mesh`. That is a pre-existing test-isolation defect, diagnosed in §8 as ruling A-2, and
it did *not* fire in the perturbed run — which is what a race looks like. So the honest reading
of the table is **one new failure under the perturbation, plus one flake that comes and goes.**

The two failures under perturbation, both named:

1. `initial::tests::routing_order_canonicalizes_signed_zero_before_terminal_tie_break`
   (`crates/plinth-support/src/initial.rs:1998`), `left: [9, 4], right: [4, 9]`. This is the
   perturbed function's **own contract test**: it asserts the tie-break's direction directly.
   A genuine change to that tie-break legitimately updates it, and it also proves the
   perturbation actually took effect rather than being compiled away.
2. `support_ratchets_do_not_regress`, reporting a quality regression in words:

```
RATCHET REGRESSION -- 1 metric(s) got worse than the recorded baseline:
model 'make_toed_slab', metric 'support_resin_volume_ml' (lower-is-better, mL):
0.550812399980 -> 0.592125137274, WORSE by 0.041312737294 (tolerance 1.000e-9)
```

That is a real 7.5% increase in support resin on one model. The perturbation is a legal
placement change that makes the plan *worse*, and the suite says which quantity, which
direction and by how much — instead of "the hash changed". That is the ruling working.

### The four former fingerprint sites, under the perturbation

All four passed. This is the specific thing the workflow existed to fix:

| binary | result under perturbation |
|---|---|
| `crates/plinth-support/tests/support_runner_integration.rs` | 6 passed, 0 failed, 2 filtered |
| `crates/plinth-support/tests/a4_transactional_controller.rs` | 8 passed, 0 failed |
| `crates/plinth-verify/tests/coverage_gate.rs` (incl. `public_coverage_report_is_frozen_without_evidence_loss`) | 4 passed, 0 failed |

**No test anywhere in the workspace failed because a support-plan fingerprint or a
contact/member count moved.** The fourth pin — a whole-report golden file in `plinth-verify` —
was retired by the integrator during integration rather than by any build lane, by redacting
only the digest field from both sides of the comparison and asserting every remaining byte. I
read that redaction: it is textual rather than a JSON round trip, so key order, number
formatting and leaf types stay frozen, and it fails loudly if the digest stops being emitted or
stops being a number. It is a sound retirement, not a thirteenth re-pin.

The perturbation was reverted; `git diff` over `crates/` against `eb224e0` is empty, and the two
integration binaries re-run green afterwards (8 passed / 6 passed, 0 failed).

---

## 3. What the corpus actually is

Every "corpus" figure in this report covers `corpus/models/`, which holds **8 meshes**, named
in full:

| model | size (mm) | what it is |
|---|---|---|
| `make_box` | 10 x 10 x 10 | procedural cube |
| `Sphere` | 20 x 20 x 20 | geometric primitive |
| `Cylinder` | 30 x 30 x 30 | geometric primitive |
| `make_overhang_table` | 30 x 35.4 x 34.3 | procedural fixture |
| `make_toed_slab` | 28 x 30 x 18.5 | procedural fixture |
| `make_thin_fin` | 19.3 x 20 x 21.6 | procedural fixture |
| `make_raking_blade` | 20 x 19.3 x 13.5 | procedural fixture |
| `Weeping Treant v1.2` | (a real model) | the only actual miniature |

**Exactly one of the eight is a real miniature, and it is the one that does not complete.**
The workflow brief describes "10 of 13 real miniatures" that never finish; there is no
13-model corpus in this repository, and the `measure:worst-layer` lane reported the same
discrepancy independently. Every claim in this document that says "the corpus" means these
eight geometric and procedural shapes plus one treant, not a set of miniatures.

---

## 4. Worst-layer demand — the measurement, and who it covers

### What was measured and why

A resin printer cures one thin layer at a time against a transparent film and must peel the
part off that film once per layer. Peel is the largest force in the process and it acts over
**area**, so the supports must be able to carry the area of the model's largest single
horizontal slice. Printed test coupons measured a green-resin cross-section failing at about
25 times its own area; the design allowance keeps a factor of two back, so the registered
constant `X_ALLOWED` is **12**: a support tip may carry at most 12 times its own
cross-sectional area.

The question was whether that model demands a plausible number of supports or an absurd one.

### Scope, stated before the numbers

- **Slice-derived demand: all 8 corpus models measured, none excluded.** Whole sweep 52.35 s.
- **Comparison against what the engine places today: 6 of 8 models.** Two excluded, by name:
  - `make_raking_blade` — the run *refuses* (coverage-closure failure, `island 5`,
    `uncovered_area_mm2 = 5.135`), in 19 s. A refusal, not a hang.
  - `Weeping Treant v1.2` — killed at 900 s inside the coverage-repair loop the owner's
    Ruling 2 says not to fix in this work.

### Demand, per model, 0.05 mm layers, pinned corpus orientations

`[1]` is the largest horizontal cross-section anywhere in the model. `[2]` is the tip area the
area law demands for it (`[1] / 12`). `[3]` converts that to a support count at two tip sizes.
One tip is 0.502655 mm² at 0.80 mm diameter and 3.141593 mm² at 2.00 mm.

| model | worst layer z (mm) | [1] worst-layer mm² | [2] required tip mm² | [3] tips @0.80 mm | [3] tips @2.00 mm |
|---|---|---|---|---|---|
| `make_box` | 0.0250 | 100.0000 | 8.3333 | 16.58 | 2.65 |
| `make_overhang_table` | 19.2750 | 277.1254 | 23.0938 | 45.94 | 7.35 |
| `make_toed_slab` | 3.4750 | 250.5381 | 20.8782 | 41.54 | 6.65 |
| `Sphere` | 9.9750 | 313.3343 | 26.1112 | 51.95 | 8.31 |
| `Cylinder` | 9.9750 | 705.8586 | 58.8215 | 117.02 | 18.72 |
| `make_raking_blade` | 5.0750 | 160.2974 | 13.3581 | 26.58 | 4.25 |
| `make_thin_fin` | 8.8750 | 166.9282 | 13.9107 | 27.67 | 4.43 |
| `Weeping Treant v1.2` | 14.3250 | 544.8302 | 45.4025 | 90.33 | 14.45 |

### Demand against what the engine places today — 6 models

| model | supports today | tip area today (mm²) | required / today, by area | proposed @0.80 mm / today, by count |
|---|---|---|---|---|
| `make_box` | 11 | 9.4876 | 0.88x | 1.51x |
| `make_overhang_table` | 74 | 15.7514 | 1.47x | 0.62x |
| `make_toed_slab` | 64 | 4.2536 | 4.91x | 0.65x |
| `Sphere` | 9 | 7.8524 | 3.33x | 5.77x |
| `Cylinder` | 45 | 40.5804 | 1.45x | 2.60x |
| `make_thin_fin` | 17 | 3.2673 | 4.26x | 1.63x |

**The full ranges, both ends.** Required total tip cross-section is **0.88x to 4.91x** what the
engine places today. Required support count at 0.80 mm diameter is **0.62x to 5.77x** today's
count. On `make_box` the engine already places *more* tip area than the law would require; on
`make_overhang_table` and `make_toed_slab` the law asks for *fewer* supports than are placed
today. The largest absolute demand anywhere in the corpus is 18.72 tips at 2.00 mm diameter,
on `Cylinder`.

So the load model's central assumption survives contact with the corpus: the redesign is a
small-factor change, not an order-of-magnitude one. That is a statement about seven geometric
fixtures and one treant, not about a library of miniatures.

### Does any model demand more tip than its underside can physically hold?

Two different denominators, and the difference matters:

- Against the model's **whole** downward-facing projection, the worst case is exactly
  **1/12 = 0.0833** — `make_box`, `Sphere` and `Cylinder` all sit exactly on it, and the
  treant is lowest at 0.0108. That ceiling is structural, not luck: for a closed solid the
  worst layer's cross-section can never exceed the model's footprint, which is what its
  underside projects to.
- Against only the **supportable** subset (faces steep enough for the engine to seat a
  support on), `Sphere` is tightest at **0.4765** — 47.65% of its supportable underside would
  have to be tip. The theoretical ceiling for packing circles is 0.9069, so the margin is
  1.90x. The measurement lane reported the worst case here but not the best; the low end of
  this second bracket is not on record and I did not derive it.

### What I checked myself about this measurement

The instrument lives on `measure/worst-layer-demand` (`03b6d76`), which is **not merged into
the branch under judgement** — it is a measurement-only test binary, marked ignored, that
costs nothing in the normal suite. I did not re-run the sweep. I did independently verify its
calibration against the corpus's own recorded dimensions:

| model | analytic maximum cross-section | measured | agreement |
|---|---|---|---|
| `make_box`, 10 x 10 x 10 | 100.000 mm² exactly | 100.0000 | exact |
| `Sphere`, 20 mm across (r = 10) | pi x 10² = 314.159 mm² | 313.3343 | 99.74% |
| `Cylinder`, 30 mm across (r = 15) | pi x 15² = 706.858 mm² | 705.8586 | 99.86% |

The shortfalls are faceting — a triangle mesh inscribes the true circle. The instrument is
calibrated. Every other row in the tables above is quoted from that lane and **not**
independently re-derived here.

---

## 5. The area-law gate, landed in warning mode

Under the owner's Ruling 1, the gate that judges the area law records every violation with its
evidence and refuses nothing. It is the measuring instrument for the load-model redesign, not
yet a blocker. That is what shipped, and it works: on the branch under judgement,
`plinth prepare` on `make_toed_slab` emits its artifact and prints the violations, naming each
contact, its seat, the area it carries and how far under-sized its tip is.

Violation counts from the gate lane's own survey, **6 of 8 models**, with the two exclusions
named (`make_raking_blade` refuses before reaching the gate; `Weeping Treant v1.2` did not
finish in 150 s):

| model | contacts judged | contacts violating |
|---|---|---|
| `Sphere` | 24 | 0 |
| `Cylinder` | 82 | 10 |
| `make_box` | 11 | 3 |
| `make_overhang_table` | 74 | 6 |
| `make_toed_slab` | 64 | 3 |
| `make_thin_fin` | 17 | 2 |

**The second half of Ruling 1 did not land.** The ruling requires the gate be "wired
immediately to the ratchet so the violation count cannot silently get worse." There is no
violation-count ratchet. Seven ratchet metrics ship; none of them counts violations, and the
nearest one cannot substitute for two measured reasons:

1. It is a **maximum** over contacts, so it is blind to how *many* contacts violate. A new
   violation at or below the current worst moves it by exactly zero.
2. Its recorded values are themselves already far past the law. From
   `crates/plinth-support/tests/fixtures/support_ratchets.json`, worst tip area ratio against
   an allowance of 12: `Sphere` 12.000000000000005, `make_box` 15.92, `Cylinder` 33.29,
   `make_thin_fin` 25464.79, `make_overhang_table` 57295.78, `make_toed_slab` 57295.78. Five
   of the six recorded values are area-law violations frozen as the accepted reference. On
   `make_toed_slab` a newly violating contact would have to exceed 57295.78 before anything
   noticed.

Violation counts are pinned in exactly two places in the whole workspace — one procedural
fixture in `load_gate_area_law.rs` and one small fixture in `prepare_folder.rs`. No corpus
model's violation count is checked anywhere that can fail. The survey that produced the table
above contains **zero assertions** and is marked ignored; it is a measurement script.

---

## 6. What this does not prove

**A ratchet number is not a print.** All seven ratchet metrics are computed by the same engine
that produced the plan, from the engine's own disclosures. They encode the engine's model of
what makes a plan good. If that model is wrong — if the load bookkeeping mis-attributes which
support carries which piece of the model, which the pre-existing measurements say it does on
most faces — then every metric can improve while the printed part gets worse on the plate. The
only ground truth is a printed plate, and no plate has been printed against these numbers.

**Four of the seven metrics are already zero on every measured model** (`residual uncovered
area`, `worst uncovered gap`, `ownerless model area`, `abandoned load`). A number pinned at
zero can only ever worsen, never improve, so those four are alarms rather than a gradient. Only
three of the seven — worst tip area ratio, total scar area, support resin volume — are actually
gradients today. And one of the four zeroes, abandoned load, can only move at all on
`make_toed_slab`, the one model that refuses any contact; on the other five it is a constant no
change to that code path can shift.

**The ratchet cannot see the raft.** The raft is the mat of pads and connecting bridges that
ties the supports to the plate. It is built separately and enters none of the seven metrics.
The `sabotage:retire` lane deleted every connecting bridge; the shipped artifact lost 23 of
402 support records and all seven metrics on all six models stayed bit-identical. I confirmed
the mechanism by reading the code rather than by re-running that sabotage: the ratchet's
evidence is built from emitted contacts, ownerless islands, abandoned contacts, live support
members and the coverage measurement, and no raft primitive appears in any of them.

**The acceptance test perturbs one tie-break.** It is the perturbation the workflow specifies
and it is legal, but it is gentle: it only reorders supports that tie on every earlier
comparison. The cross-family challenge lane applied a harsher legal reordering (reversing the
primary height ordering) and the suite did *not* tolerate it — three failures, none of them a
fingerprint: `Sphere` stopped sealing at all, a fixture's status changed, and recorded
no-winner evidence changed. Two of those are arguably correct refusals of a worse plan. What
this workflow establishes is that the suite no longer fails a legal change *because a hash
moved*; it does not establish that every legal change now passes.

**Every number in this report carries a `--skip`.** The mandated command
`cargo test --workspace --offline` does not terminate on this branch, or on `develop`. One
test (`a_spec6_removal`) runs an unbounded coverage-repair search that the `measure:baseline`
lane measured at 4+ hours, and one (`closure_repairs_censored_dust`) is a deliberate permanent
failure. Both are pre-existing. Until that is fixed, no claim about this workspace comes from
the gate as written.

**The clean tree is not green.** My clean baseline run failed one test — a `plinth-mesh`
test-isolation defect, described in §8 as ruling A-2 — so "1 failure under perturbation" is
measured against
a baseline of 1 failure, not zero.

---

## 7. What the owner must decide

1. **Does Ruling 1's ratchet wiring have to land before this merges?** The gate is in warning
   mode as ruled, but the "violation count cannot silently get worse" half does not exist. What
   turns on it: whether the load-model redesign starts with an instrument that can detect
   backsliding, or with one that only reports.
2. **Should an unreadable load record hard-refuse, or warn?** The gate's own documentation says
   it stays fail-closed on a record it cannot read; the shipped code downgrades those to
   warnings alongside genuine violations (ruling CF-1 below). What turns on it: whether the
   engine may ship a support whose carried area or tip size it could not describe — and,
   because the ratchet silently drops such rows, whether the quality numbers can improve while
   the safety record degrades.
3. **Must the raft be measured?** Nothing today checks that the raft stays connected or charges
   for its material. What turns on it: whether a change can quietly disconnect the plate
   attachment while every quality number improves.
4. **What replaces the retired completeness witness on refusals?** The old exact-vector
   assertion proved that both known refusals survived into the public disclosure; the
   replacement iterates over whatever the producer returns and passes on an empty list. What
   turns on it: whether a producer that silently drops refusals can ship.
5. **Correct the doctrine document now or later?** `docs/development.md` still says six
   ratchets where seven ship, still cites a "zero-abandoned-load invariant" that this very work
   removed, and still presents the area law as an invariant that no change may loosen while the
   shipped gate warns on it for five of six models. The phrase "warning mode" appears in
   neither doctrine document. What turns on it: the doctext lane existed to stop the codebase
   documenting a policy it is not running, and that failure has reappeared one level up.
6. **Is `cargo test --workspace` still "the mandated gate" while it cannot finish?** What turns
   on it: every provenance claim in this project currently has a `--skip` in it.
7. **Fix the `plinth-mesh` isolation flake now, or file it?** What turns on it: whether a
   red clean-tree run is treated as noise, which is how a real regression gets waved through.

---

## 8. Ruling record

Rulings are ACCEPT, ACCEPT-AMENDED (what is kept and what is discarded is stated), REJECT, or
UNVERIFIED (could not be checked first-hand; stays open). Everything marked "read at eb224e0"
was verified in this lane by reading the code at the named location.

### From the cross-family challenge

**CF-1 — Warning mode downgrades an unreadable load record, contrary to its own stated scope.
ACCEPT.**
Verified, read at `eb224e0`. `crates/plinth-pipeline/src/gates.rs:697-708` returns five
different faults — carried area missing, carried area unusable, tip radius missing, tip radius
unusable, and a genuine area-law violation — all inside the single refusal type
`LoadLawViolated`. The dispatch at `gates.rs:776-787` matches that type and converts it to a
warning **without inspecting which fault it is**. The comment immediately above it, at
`gates.rs:758-772`, states the opposite: "The gate stays fail-closed on a record it cannot read
at all." So does the producer's own documentation at `crates/plinth-support/src/runner.rs:3240-3244`,
which says inventing a zero "would convert a refusal into a pass — and, under warning mode, into
a silent pass … the gate is now what enforces it." It does not enforce it.
**Measured end-to-end in this lane, not inferred.** I made one contact's tip radius
undescribable in the producer (`make_toed_slab` contact 48 — the one carrying 450 mm² through a
0.05 mm tip), rebuilt, and ran `plinth prepare` against the pinned corpus orientation. The gate
recognised the fault in exactly the words it was written to say:

```
LOAD LAW WARNING (not blocking)  3 of 64 emitted contact(s) violate the registered support area law
  contact 48 ...: the run routed this contact and then could not state the radius of the tip it
  shipped, so the area law has no tip to judge. An emitted contact the run cannot describe is not
  an emitted contact this gate can clear
...
result  EMITTED  make_toed_slab.chitubox  (402 support records, 2110344 bytes)
SUMMARY  emitted=1  gate-refused=0  failed=0  stopped=0
```

The gate says the contact "is not an emitted contact this gate can clear", and the artifact is
written anyway.

**And the ratchet rewards it.** With the same sabotage in place I ran the ratchet harness:
`RATCHET GATE PASSED`, `2 passed; 0 failed`, 139.98 s. Total scar area *improved* on two models,
because the unreadable row is silently skipped — `make_toed_slab` 4.253576579819422 →
4.245722598185448 and `make_overhang_table` 15.751400068868165 → 15.743546087234190, each down by
0.0078540 mm², exactly the missing tip's own cross-section. Lower is better, so this reads as
progress. That is the full chain, measured: the record becomes unreadable on the worst-loaded
contact in the plan, the gate warns instead of refusing, the artifact ships, and the quality
instrument records an improvement and passes silently. All edits reverted; `git diff` empty.
*What changes:* separate record-integrity faults from genuine area-law violations — either a
distinct refusal type, or inspect the fault in the dispatch — so the four integrity faults hard-
refuse. Add an end-to-end test through the dispatch for each. Make the ratchet count dropped rows
rather than skip them silently.

**CF-2 — The retirement lost the completeness witness on abandoned-contact disclosures.
ACCEPT.**
Verified, read at `eb224e0`. The replacement invariant at
`crates/plinth-support/tests/support_runner_integration.rs:1883-1960` iterates over
`authority.abandoned_contacts()` and checks each returned row. It asserts nothing about whether
*every* refusal reached that list, so it passes vacuously on an empty list. The retired
assertion (`abandoned.len() == 2` plus an exact vector) mixed two things: contact identities,
which were placement fingerprints and were correctly retired, and cardinality, which proved the
projection was total. Only the first was replaced. I grepped every reader of
`abandoned_contacts()` in the workspace — `gates.rs:970` and four test sites — and none
reconciles the list against its source.

**Reproduced in this lane.** I filtered `BuriedSurface` refusals out of the producer and ran the
two binaries that read the disclosure. The dropped path was exercised — with output uncaptured,
`SABOTAGE: dropping buried_surface refusal 25` printed on **4** separate runs inside the binary —
and every witness passed anyway: `support_runner_integration` 6 passed / 0 failed / 2 filtered,
`a4_transactional_controller` 8 passed / 0 failed. A real refusal disappeared from the public
record and nothing in the workspace noticed. Reverted; both binaries green again afterwards.
*What changes:* assert a one-to-one reconciliation between the producer's refusal list and the
projected disclosures, by source-row count or stable source identity, never by expected contact
IDs.

**CF-3 — The ratchet does not measure raft connectivity or raft material. ACCEPT.**
Verified, read at `eb224e0`. `ratchet_harness.rs:389-425` builds its evidence from emitted
contacts, ownerless islands, abandoned contacts, live member volumes and the coverage
measurement. The raft is built independently at `crates/plinth-support/src/raft.rs:290-319` as
minimum-spanning-tree bridges plus one pad per root; no raft primitive enters any of the seven
metrics. Independently corroborated by measurement in the `sabotage:retire` lane: deleting every
bridge shipped an artifact 23 records smaller and moved no metric by one bit.
*What changes:* raft connectedness and root-to-raft attachment become invariants; raft material
enters the resin metric if that metric is meant to describe the whole printed support system.

**CF-4 — The rebuilt-round work meter under-states its own execution. ACCEPT-AMENDED.**
Verified, read at `eb224e0`. `crates/plinth-support/src/specialized.rs:5305-5339` assigns the
preflight estimate to *both* the charged and the executed figure on the rebuilt branch, so the
census under-states rebuilt work by the same per-cell term the retained branch was tripping
over. The code's own comment says so.
*Kept:* the defect and the recommendation to meter execution separately from admission.
*Discarded:* any implication that this makes the Cylinder fix wrong or unsafe. It is a reporting
inaccuracy in census output; the full-build path performs an exact resource check including the
missing term before it allocates, and the `fix/cylinder-load-accounting` lane disclosed the
deferral explicitly rather than hiding it.

**CF-5 — The workflow's perturbation is gentle; a harsher legal reordering still fails.
ACCEPT as a limit on scope, not as a defect.**
UNVERIFIED in this lane — I ran the specified perturbation, not the harsher one. The challenge
lane's three reported failures are all substantive (a model stops sealing, evidence changes, a
fixture's status changes) and none is a fingerprint, which is consistent with the doctrine
working. Recorded in §6 as a bound on what the acceptance test establishes.

### From the sabotage lanes

**SR-2 — The six "resource ceiling" assertions are decorative for the defect they name.
ACCEPT.**
Verified, read at `eb224e0`. `crates/plinth-support/src/runner.rs:3543-3552` refuses the
measurement the instant the route-membership counter passes its ceiling, and `runner.rs:3949`
reports that same variable as `route_membership_sum`. A measurement that returned successfully
cannot violate the ceiling, so the assertion is tautological for that field. Residual reach: a
divergence between the gating site and the reporting site, roughly 400 lines apart — narrow, not
"a guarantee about the allocator."

**SR-3 — The patch-verdict check restates production's formula, and restates it wrong.
ACCEPT.**
Verified, read at `eb224e0`. Production, `crates/plinth-support/src/lib.rs:2059`:
`assessment_failed = patch_area > area_floor_mm2 || patch_ambiguous`. The test,
`support_runner_integration.rs:1122-1123`, asserts `assessment_failed == uncovered_area > floor`
and omits the `|| patch_ambiguous` term entirely. It bites a one-sided edit to production today
only because `cut_ambiguous` is false on all nine frozen patches. The first legal change that
makes one patch ambiguous turns this test red for a correct plan — the exact failure the
doctrine was adopted to remove, reintroduced by the replacement.
*What changes:* derive the verdict from production's own predicate rather than transcribing the
formula, or state the full rule.

**SR-4 — A seat-uniqueness assertion was deleted without being declared. ACCEPT-AMENDED.**
Verified, read at `eb224e0`.
*Kept:* the process point. The commit message enumerates what was retired and does not mention
this deletion.
*Discarded:* the coverage concern. The assertion survives verbatim at
`support_runner_integration.rs:1743-1753`, in a test that runs regardless of any digest. Nothing
was lost.

**SR-5 — A comment claims a bound the code does not assert. ACCEPT.**
Verified, read at `eb224e0`. `support_runner_integration.rs:498-516` says the check is "the
bound that is true of any plan: a refusal may not abandon MORE load than the plan ever routed,
and the disclosure may not invent a negative or non-finite area." Only the second clause exists
in code. Same shape as the defect the same lane corrected elsewhere: documentation asserting an
invariant the code does not check.

**SR-6 — Ratchet metric 7 is sensitive on exactly one model. ACCEPT.**
Verified, read at `eb224e0`. `support_ratchets.json` records `abandoned_load_mm2 = 0.0` for all
six measured models, and only `make_toed_slab` abandons any contact at all, so on the other five
the value is a constant no change to that code path can move. The module's claim that the metric
"tracks every corpus model rather than one fixture" is true of the recording and false of the
coverage.

**SG-D1 — Warning mode's scoping decision is untested and its refusal arm is unreachable from
any test. ACCEPT.** Same root cause as CF-1. The unit tests that exercise record-integrity
refusals call the inner judging function directly and never go through the dispatch where
warning mode's scope actually lives, so an edit that downgrades integrity refusals lands green.

**SG-D2 — The corpus violation survey is not a check. ACCEPT.**
Verified, read at `eb224e0`.
`crates/plinth-pipeline/tests/load_gate_warning_mode_corpus_survey.rs` contains zero assertions
and is marked `#[ignore]` at line 140. The six violation counts in §5 are therefore recorded
nowhere that can fail.

**SG-D3 — The operator-facing warning is unasserted. ACCEPT.**
Verified, read at `eb224e0`. The string `LOAD LAW WARNING` occurs exactly once in the workspace,
at `crates/plinth-cli/src/execute.rs:276`, and nothing in `crates/plinth-cli/tests/` references
it. Under Ruling 1 that printed warning *is* the deliverable, and it is the one part of it
nothing guards.

**SG-D4 — Ruling 1's ratchet wiring does not exist. ACCEPT.** See §5 for the measured
substantiation, including the six recorded worst-ratio values against an allowance of 12.

**SG-S — The area-law comparison's arithmetic slack is enforced twelve orders of magnitude
looser than it is documented. ACCEPT.**
Verified, read at `eb224e0`. `AREA_LAW_ARITHMETIC_SLACK` is `8.0 * f64::EPSILON`, about
1.8e-15 relative, and its comment says it "cannot mask a defect." Its boundary test at
`gates.rs:1151-1175` probes with `exact * 0.999`, a 0.1% radius reduction — about 2e-3 in area.
Any slack up to ~2e-3 passes that test. The sabotage lane measured exactly that: 1e-3 passes,
1e-2 fails. The constant is right; the test that guards it is loose.

**SC-1 — The corrected Cylinder comparison no longer constrains the term the Cylinder defect
was about. ACCEPT.**
Verified, read at `eb224e0`. `full_build_equivalent_operations` is the preflight estimate plus
`retained_scan_operations()`, and the executed figure contains the same
`retained_scan_operations()` term, so the per-cell term — roughly 95% of the meter on Cylinder —
appears on both sides and cancels. After the fix no check compares that term against an
independently derived figure. The `sabotage:cylinder` lane measured this by multiplying the term
by 1000 and watching the whole `plinth-support` library suite stay green. The retargeting was
still correct: the old comparison was arithmetically unprovable, which the Cylinder proves. The
net is one false-but-strong check removed and one true-but-weak check added.
*What changes:* pin the per-cell rate against an independently derived count, so the dominant
term is measured by something.

**SC-2 — The affordability anchor is unguarded. ACCEPT.**
Verified, read at `eb224e0`. `absorbed_cells` is written at `perception.rs:4539` and `:4599` and
read only at `:4260`. Nothing else in `crates/` mentions it. The sabotage lane set it to the
maximum representable value, rendering the anchor permanently vacuous, and the whole library
suite passed (653 tests). The value it guards is checked; the anchor itself is not.

**SC-3 — The retained-tail check has an order of magnitude of slack. ACCEPT.**
Reported as measured by the `sabotage:cylinder` lane (a full quadratic blow-up of the retained
tail passed the 2000-shape sweep at factor 1 and failed at factor 10). UNVERIFIED in this lane —
I did not re-run the sweep. It is consistent with the arithmetic I read: once the cell term
cancels, the surviving comparison is the retained tail against the whole preflight, which is
much larger.

### Raised in this lane

**A-1 — The doctrine document describes a policy the code is not running. ACCEPT (new).**
Verified, read at `eb224e0`. `docs/development.md` says "**The six adopted ratchets**"
(line 240) and "The six floating-point ratchets" (line 320) where **seven** ship. Line 252
defends ratchet 1 against gaming by citing "a zero-abandoned-load invariant", and line 304
rejects abandoned-contact count as a metric because abandoned carried area "is already pinned at
zero by an invariant" — that pin is precisely what this work removed and replaced with ratchet 7.
And line 200 lists the area law among the invariants that "no change may narrow, loosen, or
delete", above a gate that now warns on it for five of six measurable models; the phrase
"warning mode" appears **zero** times in `docs/development.md` and zero times in
`docs/engineering-journal/06-conventions.md`. The doctext lane reconciled the two doctrine texts
with each other but not with the code that shipped beside them.

**A-2 — A `plinth-mesh` test fails on a clean tree, from a test-isolation defect. ACCEPT
(pre-existing, not a merge regression).**
Measured in this lane. My clean baseline run failed
`triage::tests::every_cap_minus_one_refuses_before_input_proportional_triage_work_starts` at
`crates/plinth-mesh/src/triage.rs:1350`, `left: 2, right: 1`, under debug assertions — the
integrator saw the same failure only in the fast build, so it is not profile-specific. The
mechanism is in the source: `INPUT_PROPORTIONAL_TRIAGE_WORK_STARTED` (`triage.rs:790`) is a
**process-global** counter incremented by production code, and the test asserts it advanced by
exactly one while sibling tests in the same binary run in parallel and also increment it.
`git diff develop..eb224e0 -- crates/plinth-mesh/` is empty, so the crate is byte-identical to
`develop`. It did not fail in my perturbed run, which is what a race looks like.

**A-3 — Minor, pre-existing: "floor" names smallest-allowed values in production code.**
`spacing_floor_mm`, `patch_area_floor_mm2`, `area_floor_mm2`. The house rule reserves "floor" for
the bottom of a printed part. Not introduced by this work; noted so it is not inherited by the
load-model redesign.

### Rejected

None. Every finding put to me was substantiated either by my own reading of the code at
`eb224e0` or by a measurement another lane recorded and I could trace to its mechanism. Two
findings were narrowed (CF-4, SR-4) and two are recorded as UNVERIFIED-but-open (CF-5, SC-3).

---

## 9. Verdict

# LANDABLE

**Reasons for.**

1. **The thing this workflow existed to fix is fixed, measured in this lane.** A legal
   placement change costs two failures: the perturbed function's own contract test, and a
   ratchet naming a real 7.5% resin regression in a sentence. All four sites that previously
   pinned a plan fingerprint pass. The retirement landed, and it landed complete — the fourth
   site, which no build lane owned, was finished during integration.
2. **It removes a defect from `develop`, and adds none I could find.** The `Cylinder` model
   killed a worker thread under debug assertions on `develop`; on this branch the whole suite
   runs with it measured. The 17 tests the hard-refusal Load gate had turned red are green. No
   lane, and no check of mine, found a behaviour regression: the one red on a clean tree is in a
   crate byte-identical to `develop` and is a pre-existing test-isolation race.
3. **Every accepted finding names a check that is weak or missing, not a plan that is wrong.**
   None of them says the engine now produces a worse plan.

**Reasons against, stated at full strength rather than as follow-ups.**

Two of the accepted findings are coverage this merge **removes from `develop`**, and neither is
optional polish:

- **CF-2.** On `develop` the retired exact-vector assertion proved both known refusals survived
  into the public record. I deleted one refusal from the producer and every replacement witness
  passed. A producer that silently drops refusals now ships.
- **CF-3 / SR-1.** On `develop` the retired `(contacts, members, digest)` triple would have
  caught the raft — the mat tying the supports to the plate — losing its connecting bridges. The
  `sabotage:retire` lane deleted every bridge: 23 of 402 records left the shipped file and all
  seven quality metrics on all six models stayed bit-identical.

And **Ruling 1 is half-implemented.** The gate warns as ruled; nothing yet stops the violation
count getting worse, and the nearest existing metric cannot substitute because it is a maximum
and its recorded values already sit up to 57295.78 against an allowance of 12.

**Why those do not block.** The retirements themselves were correct under the adopted doctrine —
those were fingerprints, and fingerprints are equivalence checks. What went wrong is that the
coverage riding on them was not carried across, which is an unfinished job rather than a wrong
one. Holding the merge does not restore that coverage; it only keeps a known worker-killing
defect on `develop` and leaves the gate work stranded. The three changes below restore more than
was lost, and each is small and well specified.

**Required before the load-model redesign begins** — these are the instrument it will be steered
by, and it cannot be steered by an instrument with these holes:

1. Assert that every refusal the producer holds reaches the public disclosure — by source-row
   count or stable source identity, never by expected contact IDs (CF-2).
2. Make raft connectedness an invariant and put raft material into the resin metric (CF-3).
3. Add the violation-count ratchet Ruling 1 asks for, computed independently of the gate's own
   verdict (SG-D4).

**Should be settled before the gate flips from warning to refusal**, because after the flip it is
a safety decision rather than a measurement one: whether an unreadable load record refuses or
warns (CF-1) — and, either way, the ratchet must stop silently skipping the rows it cannot read.

**Not a merge condition, but do not lose them:** the doctrine document currently describes a
policy the code is not running (A-1), the mandated gate cannot run to completion (§6), and a
`plinth-mesh` test fails at random on a clean tree (A-2).

---

*Adjudicated 2026-08-29. All measurements in this document were taken in the worktree at
`verify/doctrine-in-force` @ `eb224e0af5291ddf3b092394bc1b6e83c7987cd2`; every temporary edit was
reverted and `git diff` over `crates/` against that commit is empty. Raw logs:
`scratchpad/adj/clean.log`, `scratchpad/adj/perturbed.log`.*
