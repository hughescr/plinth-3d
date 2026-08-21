# Model-seated support arms — WHAT LANDED, and what did not

> **Provenance.** Written 2026-08-21 against `feature/a4-controller`, main checkout
> `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`. This amends
> `model-seated-arms-2026-08-21.md` with the CHALLENGE review's required changes folded
> in, and records what the implementation actually did.
>
> **No cap, gate, threshold, tolerance or registered constant is moved, widened or
> bypassed by this change.** Two new refusal reasons are added; both are strict
> tightenings and both read values already on `ContactPolicy`.
>
> Evidence labels: **MEASURED** (a number a run produced), **DERIVED** (closed form from
> MEASURED or registered inputs), **INFERRED**, **ASSUMED**.

---

## §A. What landed

**Pieces A + B, plus PR-3 for the CCO and rewire families, in one commit.** That
grouping is not a convenience: the design's §3.2 argues A and B must land together
(B alone moves the wall to the PR-7 kiss-catch and delivers nothing; A alone changes
geometry and commits nothing), and the review's F1 adds that B without PR-3 replaces an
*unsatisfiable* test with an *unrun* one.

| design section | what landed |
| --- | --- |
| §3.1 Piece A | `cco::apply_model_anchor` builds `p1` from `contact::seat_endpoint`. The seat is derived once per seat in `generate_descriptors` and travels on `Descriptor::ModelAnchor::seated_point_mm`. `contact::seating_of` gained the end argument and classifies `(Strut, Rootward)` as `Seated` (option **A1**). |
| §3.1 Piece A | `attest_model_anchor_seats` now measures the SEATED endpoint. The design named this; without it the witness keeps reporting the kiss it was correctly reporting before, and PR-7 delivers nothing. |
| §3.1 Piece B | `contact::placed_mask_inset_mm` is the single statement of the PR-2 inset. `cco::clearance_survival`, `rewire::validate_changed_geometry` and `brace::exact_clearance_rows` all call it. `cco::Config::contact_depth_mm` and `rewire::Config::contact_depth_mm` are **deleted** — they had no other reader. |
| review **F1** | PR-3 lands at CCO/rewire candidacy, as `contact::emerges` (the inequality now stated once, with `admit` delegating to it). Two new refusal reasons, `SeatGrazingIncidence` and `SeatDegenerateIncidence`, routed through the existing `AnchorFootRefusal` channel. **No new constant**: `min_illegal_len_mm` was already on `ContactPolicy`. |
| §3.2 | Both commit-time tripwires removed — `elect_committable_winner` and the `NotAttached` branch of `RewireCandidateBatch::winning_generation_index`. All three seat-attestation verdicts refuse at candidacy through one mapping, `SeatAttestationRefusalKind`. |
| §3.5 | `retired_formula_guard.rs` extended with the flat-inset ban **and** a positive convergence guard. |

### The batching

One `NativeSurface::project` per candidate tournament (`clearance_survival`) and one per
rewire changed-set (`validate_changed_geometry`), alongside the `segment_clearance` calls
that were already there. The projection count per tournament goes **0 → 1**. No
per-candidate or per-member native call is introduced. `clearance_rows` runs inside the
`rayon` map with no surface, so the row records WHAT it is (end, seating, end radius) and
the mask is derived at the batch site.

---

## §B. Deviations from the design, with reasons

### B1. The terminalward end keeps `OnSurface`, so its mask carries no `h` term

§1.3's table gives the terminalward honest exemption as
`(0.3 + r + 0.6 + MARGIN)/|cos|` — i.e. with `h = contact_depth_mm`. **That is not what
landed.** `contact::seating_of` returns `OnSurface` for a `Terminalward` contact on every
family except the brace model rung (which `seat_endpoint` genuinely seats at either end
and which never mints a terminalward contact in production), so for the initial contact
stub — the only production terminalward declarer — the mask is
`(r_max + 0.6 + MARGIN)/|cos|`.

**Why.** The `h` term must equal the endpoint's ACTUAL depth below the tangent plane. For
the two seated families that depth is `contact::seat_depth_mm` by construction, because
`seat_endpoint` put it there. For the initial contact stub it is a flat
`contact_depth_mm` (`initial.rs`: `p0 = position - unit * contact_depth_mm`), which is a
DIFFERENT mechanism: at grazing incidence `seat_depth_mm` returns
`process_resolution + r*sin(theta) > contact_depth_mm`, so using it would exempt more of
the stub than its geometry earns. Modelling a third state — "seated at a known flat
depth" — is #52's rule, which is Piece D, which is not in this commit.

**Direction of the error, DERIVED.** `r_max + standoff + MARGIN >= 0.35 + 0.6 + 0.05 =
1.00 > 0.300 = contact_depth_mm`, so the terminalward mask still GROWS at every
incidence. Piece B remains an admission at both ends (pinned by
`contact::tests::the_converged_mask_exceeds_the_retired_flat_pair_at_every_incidence`).
What it does not do is make the terminalward row satisfiable: that end stays
**over-tested**, which is the conservative direction of error — it can refuse a stub
physics would allow, never admit an untested one. Piece D owns closing it.

### B2. Piece C (the scar bound) did not land

The design's §3.1 says Piece C is "the price of the exemption". The review re-scoped that
in F1: *"Piece C grades the seat scar, not a mid-arm collision"* — the guard the enlarged
exemption needs is PR-3, and PR-3 landed. With PR-3 in place the exempt region is bounded
by `mask <= length - min_free_span_mm`, so the member is always partly distance-tested,
and the exempt region is the same KIND of region the brace model rung has had exempt and
ungraded since Phase B-2. **No new class of unchecked region is created by this commit.**

Piece C is also a TIGHTENING, and §3.3 (endorsed by the review) puts the tightenings
behind #24/#39's zero-candidate handling. It stays there.

### B3. Pieces D, E and F did not land, and three escalations must be answered first

* **Piece D (#52)** is blocked on review **E-C**. SPEC-EMBED-POLICY's "no depth cap"
  permission is conditioned on a thin-feature-span check that **does not exist in this
  engine**, and the review constructs the regression: a stub through a 2.0 mm hollow
  shell at 72 degrees passes rules 1-3 and is anchored, so rule 4 is moot — a **6.7 mm
  shell perforation becomes legal**, where today's 0.05 mm cutoff refuses it. Building
  half an owner-ratified rule is not a decision an implementer may take.
* **Piece E** is blocked on review **E-A**. `propagate_carried` breaks at the seat; the
  model is a printed part held by its own supports, so a model-seated arm RELOCATES
  `carried_mm2` onto a model face and nothing charges that face. The design's predicate
  is topological; Craig's ruling says "closed load path per physics". Tasks **#64**
  (narrow-neck amplification) and **#65** (no connectivity requirement in the reach
  probe) govern the receiving side and become blockers under the stronger reading.
* **Piece F** (drain keep-out) gates interior arms only and is behind E/A anyway.

### B4. The removability question is recorded as a named limitation (review F6)

**This lane ships a support family whose removability has never been measured, onto a
corpus-wide tournament with no family bias.** In those words, as the review asked.

The supporting facts, RE-DERIVED by the review: there is no removability, access,
approach-angle or tool model anywhere in `crates/`; the registry's one
removal-ergonomics constant is `SPACING_FLOOR_MM = 2.5 mm`
(`m0-constants-source-ledger.json:2164-2172`, *"non-AcceptSafe removal-ergonomics policy
selected without measurement"*), read only by `specialized.rs`, so it does not space a
model-anchor seat at all; and `compare_candidates` ranks purely by added resin volume
with no family preference, so once admitted this family wins broadly rather than
marginally. **Escalation E-B** asks whether the flush-cutter measurement is a
precondition for enabling the family or a disclosed limitation the lane ships with. It is
Craig's call, and it is not answered by "the scar area is bounded" — area does not imply
access.

### B5. Prose corrections (review F8-F11)

* **F8** — §0's "first tested station" is the LAST station for a `Rootward` contact
  (`b = p0 + u*(l - inset1)`). The theorem is unaffected; the sentence would mislead the
  implementer who has to get `inset0`/`inset1` the right way round. §6 item 3's
  terminalward clause is garbled; the correct statement is
  `u*_term >= h + r_max + standoff + MARGIN >= contact_depth_mm + r_max + standoff + MARGIN`.
* **F9** — §2.3 over-credits rule 3. Per the spec's own formula rule 3 is satisfied
  automatically whenever the interval exists and the member does not taper below `r_end`
  at the exit; it is a detached-tip catcher, not a grazing-tear guard. Rules 1-2 bound
  grazing chords, which is what §2.4's table already computes.
* **F10** — Piece A/C must state `ball_tipped` for the seated anchor. **It is `false`,
  and it landed as `false`.** PR-1(c) exists for a `lower.rs` stub end emitted as a ball
  of radius `ball_factor * r_tip`; the model anchor's rootward end is a frustum end sized
  by `size_tapered_without_tree_fanin`. `brace.rs` passes `false` at its own seat for the
  same reason.
* **F11** — §1.3's "brace model rung (both) — correct" row is true of `brace.rs`'s two
  sites and FALSE of the rewire replay, which re-tested brace model rungs with the flat
  pair (`rewire.rs:889-948`). #36 was wider than the design's table showed. The
  convergence fixes it: `validate_changed_geometry` now masks every contact-declaring
  member, brace model rungs included.

### B6. §3.3's dependency reorder is what this commit acts on, with F7's amendment

The reorder — #51 Pieces A+B ahead of #24/#41 — is an ESCALATION the design raised and
the review endorsed. **It is acted on here**, on the design's reasoning (the two screens'
admissible sets are disjoint, so the mask fix is what makes FU-87's set non-empty) plus
F7's addition: FU-87's `>= 5.0 mm` screen was **incidentally covering F1's untested-arm
window**, so landing #51 first removes an accidental guard. That strengthens the case
that PR-3 had to land WITH A+B rather than wait for #41 — which is what happened — and it
is the question Craig should rule on, rather than on a reorder that looks free.

---

## §C. Measured movements

Every number below is MEASURED by `cargo test --profile gate` on this checkout.

### C1. The #51 theorem reproduces exactly, and the fix flips it

`cco::tests::the_model_anchor_arm_that_failed_by_exactly_its_radius_now_routes`, against
the real kernel on a planar slab, `r_max = 0.45 mm`:

| case | inset | kernel clearance |
| --- | --- | --- |
| unseated arm, retired flat `standoff_mm` | 0.600 mm | **exactly `-r_max` = -0.450 mm** |
| PR-7-seated arm, PR-2 mask | 1.400 mm | **exactly MARGIN = +0.050 mm** |

The first row is the design's headline theorem, reproduced with no free parameter. The
second is PR-2's own guarantee — not slack, exactly MARGIN.

### C2. Fixture-level flips

| fixture | before | after |
| --- | --- | --- |
| `rewire_anchor_fixture` model anchor | refused `Clearance` | **survives** |
| `near_anchor_fixture(9.75)` model anchor (0.55 mm arm) | refused `SeatNotAttached` at commit | refused **`SeatGrazingIncidence` at candidacy** |
| `near_anchor_fixture(8.0)` model anchor (2.3 mm arm) | n/a | **survives, seat attests** |
| required runner corpus brace termination | `Unplaceable` | **`Converged`** |
| `make_toed_slab` sealed digest | `4_190_462_360_230_124_963` | **bit-identical** |
| `make_raking_blade` refusal | 45 trials, 0 commits, island 5 at 5.135 mm² | **unchanged** |
| `make_overhang_table` coverage repair | commits >= 1, 1,011 eligible rows | **0 trials, 0 commits, 0 remaining — seals at attempt 0** |

**The nub result is the FU-87 relationship in one fixture.** `near_anchor_fixture(9.75)`
puts the face 0.25 mm from the stub end, so after seating the arm is 0.55 mm — inside the
0.054-0.596 mm band every model-anchor arm that ever won in corpus history sat in. It sat
there because the retired inset admitted nothing else: longer arms were measured and
refused by exactly their radius, shorter ones were never measured at all. PR-3 refuses the
nub BY NAME. The class dies; the real arms live.

**The brace termination move is ATTRIBUTED, not assumed.** Re-running the fixture with
Piece B in place and Piece A's seat suppressed still reads `Converged`; the cause is the
mask, not the seating.

### C3. The Sphere's drilled shell now SEALS AND EMITS

**MEASURED**, `plinth-pipeline/tests/first_artifact.rs`. The Sphere's drilled plan was
expected to be REFUSED — the tests are named
`corpus_sphere_drilled_plan_is_refused_before_any_file_exists` and
`a_coverage_refusal_removes_a_stale_artifact_already_at_the_target`. It now reaches
emission:

```
hollow_disposition: HollowedAndDrilled
supported_geometry:  DrilledShellSealed
ownerless_islands:   []
repair_report:       trials 0, commits 0, candidates_remaining 0
routed_contacts:     24
final_live_members:  57
emit:                support_record_count 138, artifact_bytes 1_270_824
```

A model that did not emit now emits, with **zero uncovered islands and zero repair
trials**. That is the strongest single piece of evidence for the lane's thesis, and it is
the opposite failure direction from the acceptance bar (§3.6 asks that no model which
emits today stops emitting; this is a model that starts).

Note this is a DIFFERENT path from `plinth-pipeline/src/lib.rs`'s `outcome()` helper,
which runs `prepared_hollow` under the two delicacy fields and still stops the Sphere at
Lowering with `RootRunTooShort` (task #29, untouched, re-recorded in C2's row).

### C4. THE GATE, AND THE STOP-AND-REPORT

`cargo test --workspace --profile gate --offline --no-fail-fast`, skipping the known-slow
`a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule` by name
(noted, not deleted):

> **1,485 passed, 10 failed, 2 ignored, 1 skipped.**

Clippy is clean on the workspace at `--all-targets`. `rustfmt` adds no new debt: the four
`plinth-support` files this change touches were clean at `f9c7d07` and are clean now, and
`cco.rs` has the same 14 pre-existing hunks it had before (task #20).

**All 10 failures are one fact, and the fact is that coverage now closes where it did
not.** Not one of them is a physics assertion, a digest, a registered constant, or a
refusal that should have fired and did not.

| test | file | what it asserts, and what happened |
| --- | --- | --- |
| `public_runner_closes_overhang_table_only_after_appending_repair_contacts` | `plinth-support/tests/support_runner_integration.rs` | *"this fixture must NOT close on its baseline layout — that is what makes it the success-after-repair demonstration"*. **It now closes on its baseline layout.** This is the root fact behind six of the ten. |
| `every_committed_step_on_the_overhang_table_proved_all_three_accept_clauses` | `plinth-support/tests/a4_transactional_controller.rs` | *"this fixture is here BECAUSE it needs repair"*. 0 trials, 0 commits, 0 remaining. |
| `a_committed_step_names_the_row_that_closed_the_hole_and_the_routes_it_covered` | same | "at least one step committed" — none does. |
| `the_production_ranking_keeps_every_eligible_row_and_not_one_per_patch` | same | ranked 1,011 eligible rows; now 0, because nothing is uncovered. |
| `the_loop_streams_progress_before_it_prints_its_summary` | same | no progress line, because there are no trials to stream. |
| `the_production_door_reports_what_the_one_winner_controller_spent` | `plinth-pipeline/tests/a4_production_door.rs` | *"this fixture reaches its artifact THROUGH repair"*; it now reaches it without. |
| `the_sealed_ledger_on_disk_names_the_row_that_closed_the_hole` | same | ledger holds 0 commits, 0 trials, 0 steps. |
| `corpus_sphere_drilled_plan_is_refused_before_any_file_exists` | `plinth-pipeline/tests/first_artifact.rs` | expects a REFUSAL; the Sphere now seals and emits (C3). |
| `a_coverage_refusal_removes_a_stale_artifact_already_at_the_target` | same | same cause. |
| `closure_repairs_censored_dust_because_that_repair_closes_a_real_hole` | `plinth-support/tests/support_runner_integration.rs` | pins an EXACT committed-step sequence. Repair still commits on this fixture, but on a different routed forest, so the step list moved. Distinct from the other nine and worth its own look: the recorded first trial now shows `objective_after` WORSE than `objective_before` (5.199 -> 5.899 mm²) on a step marked `chosen_by_first_trial_guarantee`. That is #39's first-trial guarantee interacting with a changed forest, not a #51 physics claim, but it should not be waved through. |

**They are left failing, deliberately and unmodified.** Re-freezing acceptance tests
against a new expectation is precisely the edited expectation OD-8 forbids, and neutering
them would delete #39's only proof that a committed repair step proves all three accept
clauses. Replacing the fixture means a new procedural mesh plus a checked-in
`corpus/baseline/*.pipeline.json` — a corpus change, in #39's lane and #29's, not #51's.

**This is the decision the judge must take, and it is the reason this report stops here
rather than making it.**

---

## §D. The disproof tests the review required

| id | claim | where |
| --- | --- | --- |
| **D1** | a grazing model anchor is REFUSED, not admitted with `INFINITY` | `cco::tests::a_grazing_model_anchor_is_refused_not_admitted_with_infinity` — asserts NON-VACUITY first, from the kernel: an over-long mask really does make `seg_clear_one` answer `INFINITY`. Sabotaging `contact::emerges` to always admit turns the verdict from `GrazingIncidence` to `Survives`, which is F1's hazard demonstrated. |
| **D2** | the F4 shell perforation is refused | **NOT BUILT.** Piece D is not in this commit; see B3 and escalation E-C. |
| **D3** | Piece C refuses a nonzero count on the corpus | **NOT BUILT.** Piece C is not in this commit; see B2. |
| **D4** | tournament bias measured BEFORE judging | the `cco_winner_model_anchor` / `cco_model_anchor` counters are unchanged and live. The corpus measurement is a prove-phase run, not an implementation step; C2's fixture table is the unit-scale version of the same signal. |
| **D5** | **Warhorse islands 5 and 6 both route after A+B** | **NOT RUN HERE.** This is the headline falsifiable claim and it needs a pipeline pass on the Rotting Warhorse — the prove phase's job. If they do not route, §5.3's `STANDOFF_MM` question returns to Craig. |
| re-derive (a) | longest historical winning model-anchor arm `< 0.600 mm` | supported at fixture scale: the 0.55 mm nub is refused by PR-3 and the 2.3 mm arm routes, which is the same boundary from both sides. The corpus figure is prove-phase. |
| re-derive (b) | anchored/unanchored split on RW island 74 | **NOT DONE.** All of Piece D is conditional on it; it costs zero pipeline passes and should precede any Piece D work. |

---

## §E. Escalations, collected

The design's own six stand as recorded, and the review endorsed all six. Item 1 (the
dependency reorder) is the one this commit ACTS on; the others are recorded, not acted on.

Added by the review, all three still open and all three blocking the pieces named:

* **E-A (doctrine).** Does "closed load path per physics" mean an attested contact with a
  legal scar, or that the receiving model region demonstrably carries the transferred
  `carried_mm2` down to its own supports? Decides Piece E's size and whether #64/#65 are
  blockers or neighbours. **Ask before Piece E is built.**
* **E-B (precondition).** Is the flush-cutter measurement a precondition for enabling this
  family on the corpus, or a disclosed limitation? See B4. The tournament has no family
  bias, so this is not a marginal exposure.
* **E-C (scope).** SPEC-EMBED-POLICY's "no depth cap" permission is conditioned on a
  thin-feature-span check that does not exist here. Which half do we build?

Added by this implementation:

* **E-D (test estate).** `make_overhang_table` no longer needs coverage repair, so the
  SPEC-A4 commit-path acceptance tests have no procedural fixture, and the Sphere's
  drilled plan now seals where two tests expect a refusal. See C3 and C4. Both are
  fixed-defect movements, both are owner-visible, and both are in #39's and #29's lanes
  rather than #51's. **The judge rules on how they are re-recorded; this lane does not
  re-freeze them.**
