# The honest contact exemption, and the two-tier demanded gap

**Date** 2026-08-22 · **Task** #36 · **Branch** `feature/a4-controller`
**Authority** the measured audit in `.git/mask-honesty-evidence` (Part A) and
Craig's ruling of 2026-08-22 (Part B).

---

## 1. What was wrong

A member seated into the model is legitimately inside the material it is seated
in. Something must be exempt from its clearance test or every seated arm refuses
itself. Until this change the exempt thing was a **length**: the PR-2 mask

```text
u* = (h + r_max + standoff + resolution) / |cos θ|      (contact.rs:391)
```

handed to `NativeSurface::segment_clearance` as an inset. The kernel skipped that
much axis from the contact end and read the verdict at the boundary station.

Three MEASURED facts retire it. All three are from the Giant Bat, and all three
are reproduced in `.git/mask-honesty-evidence/bat-analysis.txt`:

1. **The skip is proportional to the radius.** A fatter arm is tested over a
   *shorter* stretch of its own axis. On slot 250 the mask ran 4.030 mm at
   `r = 0.901736` and 6.568 mm at `r = 1.712694`.

2. **That inverted a verdict.** At `r = 1.712694 mm` the arm PASSED with
   `clearance = +0.336` while standing **0.242160 mm inside the model** at
   `u = 0.6903`, which its own mask excused. The same axis at
   `r = 0.901736 mm` was REFUSED at `clearance = −0.031` with **0.568799 mm of
   true gap** at its boundary station. Thinner, cleaner, refused; fatter,
   penetrating, passed.

3. **Both Bat refusals were foreign-feature proximity**, not the seat's own
   surface: the opposite wall of a crevice, at 133–151° from the seat's own
   closest point, across 11–24 distinct triangles.

Fact 3 is the design. On the seat's own tangent plane nothing beyond the natural
clearing station `x* = (h + r + standoff)/|cos|` can violate — that is what the
station is. So every millimetre the mask skipped **past the contact print**
bought no correctness and hid foreign geometry. The excess was 1.29–1.67 mm per
arm on the measured rows.

---

## 2. The rule

**EXEMPT A TARGET, NEVER A LENGTH.**

Test the FULL axis (`inset0 = inset1 = 0`) against the whole surface EXCEPT the
seat's own contact zone

```text
Z = the connected component, containing the seat's own face, of
    { x in surface : dist(x, axis) < r_seat + standoff_placement }
```

**frozen at contact creation, and RE-FROZEN once after each sizing pass that
moves the member's radius** (Craig's ruling of 2026-08-23, §11.9). `Z` is stored
with the contact. Nothing recomputes it from the radius being graded; the only
thing that recomputes it is the one explicit post-sizing pass named below.

`r_seat` is the member's radius at the contact end. `standoff_placement` is
`STANDOFF_MM`. Both are stored together as one `band_radius_mm`.

**The re-freeze rule, exactly.** After a sizing pass fixes radii, and before any
grading site reads a zone, one batched pass recomputes `Z` for every seated
member whose `r_contact + STANDOFF_MM` no longer equals its stored
`band_radius_mm` — then the zone is frozen again. This is
`seat_zone::refreeze_resized_seat_zones`, and its call sites are listed in
§11.10. It is never continuous, and it never runs at grading time.

---

## 3. Monotonicity — the theorem, not a measurement

Fix an axis and a frozen `Z`. The graded clearance is

```text
clearance = min over the whole axis of dist(x, surface \ Z) − (r_max + demanded_gap)
          = C − r_max − demanded_gap
```

`C` does not mention the radius: the tested **station** set is the whole axis
either way, and the tested **face** set is `surface \ Z`, which is frozen. So
clearance is monotone decreasing in `r_max`, and **a thinner arm can never grade
worse than a fatter one on the same axis.** Unconditional, for any geometry.

The freeze is what makes it unconditional, and the **hypothesis it needs is
narrower than "never recomputed, ever"**: `Z` must not move as a function of the
radius *being graded, within the grading pass that grades it*. Let `Z` follow the
radius at grading time and the theorem dies — a fatter arm claims a wider band, a
wider band swallows more faces, and the fatter arm is graded against strictly
less surface than the thin one. That is the skip-length's inversion rebuilt one
level up, and it is the sabotage
`the_zone_does_not_move_when_the_radius_does_because_it_is_never_recomputed`
reproduces (`seat_zone.rs`).

The 2026-08-23 re-freeze does not touch that hypothesis. It happens at a defined
point **between** a sizing pass and the next grading pass; inside any one grading
pass every zone is a constant, so `C` is still a constant of the axis and
clearance is still `C − r_max − demanded_gap`, monotone decreasing in `r_max`. The
re-freeze changes WHICH constant, once, at a moment no grading is in flight.

The kernel half of the same theorem is
`a_thinner_arm_never_grades_worse_than_a_fatter_one_on_a_frozen_face_set`
(`rust/geom_kernel/tests/face_excluded_clearance.rs`).

---

## 4. Two demands, one predicate (Craig's ruling, 2026-08-22)

| Stage | Reader | Demanded TRUE gap | Role |
|---|---|---|---|
| **Placement** | `cco::clearance_survival`, `rewire::validate_changed_geometry` | `STANDOFF_MM` = **0.600 mm** | search margin — **prunes**, refuses nothing final |
| **Final** | `brace::exact_clearance_rows`, grading at load-assigned radii (contact members AND non-contact members such as Shoots) | `FINAL_CLEARANCE_GAP_MM` = **0.300 mm** | the **only refusing number** |

Both stages call the SAME shared functions. The demanded gap is a
`ContactPolicy` field, and each builder (`brace.rs:70`, `cco.rs:4304`) sets its
stage's registered value. The math is never forked.

`FINAL_CLEARANCE_GAP_MM` provenance, verbatim as ruled: *"Craig ruling
2026-08-22, field experience: 0.3mm gap features are preserved fine in prints"*.

`STANDOFF_MM`'s ledger entry was relabelled in the same commit. Its history,
recovered by the audit: the value was chosen against a **v0 CENTERLINE** test,
where 0.6 of demand meant roughly 0.6 minus the arm radius of real
surface-to-surface gap. The kernel now subtracts the radius itself, so the same
0.6 demands a full 0.6 mm of TRUE gap. The number never moved; the quantity under
it did, and the demand tightened by one arm radius without a decision.

---

## 5. Where `mask_extent_mm` survives, and why

It leaves the clearance **distance** path entirely. Two readers remain, and both
are a different question from "how far is this arm from the surface":

1. **PR-3 emergence admissibility** (`contact::admit` → `contact::emerges`). A
   member must have enough free axis outside its own contact print to be
   gradeable at all. Under the honest rule this is not optional but *more*
   necessary: at grazing incidence the whole nearby surface falls inside
   `r_seat + standoff` of the axis and is connected, so `Z` swallows everything,
   nothing is left to grade, and the kernel returns `INFINITY` — a pass by
   erasure. PR-3 is the gate that refuses that member, and the mask is its
   yardstick.

2. **The axis-transit forgiveness window** (`brace::axis_illegal_rows`). This is
   the penetration-interval screen — "is the arm passing THROUGH material away
   from its seat" — and there a length measured from the seat is the right shape,
   because the seat embed *is* a length along the axis. Zeroing the insets here
   would condemn every seated contact for its own designed embed. That window is
   #52's lane (stub axis re-entry), and the MASKS it receives do not move.

   **CORRECTED 2026-08-22 (challenge finding F2).** This paragraph used to say
   "this change must not move it, and the 52 stub-re-entry condemnations on
   `zombie-3-a` stay untouched by design". Two things were wrong with that
   sentence and both are corrected here.

   *The claim.* "Must not move it" was true of the masks and FALSE as written of
   the screen, and nothing measured it. The screen ADMITS on the clearance the
   distance path produces — `[0, 0.1)`, or `≥ 0.1` with the midpoint inside —
   and step 5 is exactly a change to that number, on every contact-declaring
   row. So the admission input moves by construction, and any movement in the
   outcome is an OUTCOME the judge's re-baseline reads, not a claim this note may
   make in advance.

   *The label.* **"52 stub-re-entry condemnations" is not what the 52 counts,
   and the correct label is: 52 of `zombie-3-a`'s 92 repair trials had their own
   appended contact QUARANTINED — for any reason.** MEASURED in the producer:
   `runner.rs` sets `own_contact_quarantined` from
   `matches!(disposition, TrialContactDisposition::Quarantined { .. })`, which
   tests only the variant. The disposition does carry the one piece of
   re-entry-specific evidence — `remaining_inside_axis_mm`, `Some(depth)` when
   the initial build's condemned-stub ledger holds a row for that terminal and
   `None` when it was quarantined for something else — but the `matches!` pattern
   discards it before the step is written, and nothing else on
   `RepairStepRecord` carries it. So the persisted
   `*.coverage.repair.json` sidecars hold no per-trial re-entry evidence and a
   TRUE stub-re-entry count is **not recoverable at any commit**, before or
   after this change, without re-running with new instrumentation. The
   `runner.rs` unit pin
   `a_quarantine_with_no_measured_depth_does_not_become_task_52s_class` makes the
   same distinction for the defect-class read: a quarantine with no measured
   depth is not #52. A re-baselined 52 is therefore comparable as a
   quarantine count and as nothing narrower.

   What is now MEASURED is the property that matters: the screen still catches a
   buried arm.
   `brace::a_seated_arm_that_keeps_running_inside_the_model_is_still_condemned_by_the_transit_screen`
   drives production `exact_clearance_rows` on a seated arm that runs 2.7 mm
   INTO the slab past its designed embed — the honest rule makes its distance
   clearance read wide open (witness asserted `≥ 0.1`, so it is the
   midpoint-inside branch that admits it, the branch the finding-D pin cannot
   reach) — and the verdict is the screen's own `−min_illegal_len_mm`. Sabotage
   `an_exact_pass_that_skipped_the_transit_screen` turns it red.

**MEASURED, from the audit itself:** non-contact members already grade
full-length against the whole surface and need no change. The Bat's Shoots
(slots 486 and 410) carry no mask line at all and were reproduced over
`s ∈ [0, 2.517132]` — their entire length. Only `model_contact_end().is_some()`
rows ever received an inset.

---

## 6. `Z` storage — schema and cost

`Z` is a set of **global face IDs**, sorted ascending and deduplicated, which is
the order `NativeSurface::segment_clearance_excluding` requires.

**Where it lives:** a side table on `ForestState`,
`seat_zones: BTreeMap<MemberKey, Arc<[u32]>>`, deliberately NOT part of
`Forest::digest()` and not part of `Member`.

Three reasons for the side table over a `Member` field:

* `Member` is `Serialize` and feeds the run's disclosed geometry. A frozen face
  list is an implementation detail of a verdict, not geometry, and putting it on
  `Member` would put it in front of every artifact consumer.
* `Forest::digest()` hashes explicit fields, so a side table cannot move a digest
  by accident. **Models with no affected arms stay bit-identical**, and that is
  provable by inspection rather than by running the corpus.
* `MemberKey` carries a generation alongside its slot, so a recycled slot yields
  a different key and a stale entry is unreachable rather than wrong.

**Memory.** One `Arc<[u32]>` per contact-declaring member: 16 bytes of `Arc`
header plus 4 bytes per face, plus a `BTreeMap` entry. A seat zone is the
material within `r_seat + 0.6 mm` of the axis and connected to the seat face —
tens of faces on the measured rows, not thousands. At the Weeping Treant's scale
(553 contacts) an average 200-face zone costs ≈ 0.45 MB; at 2,000 faces per zone
it is ≈ 4.4 MB. Both are noise against the 100 GB machine ceiling and against the
645 MB the same run already charges to construction scratch.

**Cost to compute.** One `faces_near_segments_batch` per row set (batch-first; no
per-item native call), then a parallel BFS per row over the retained
`FaceTopology`'s CSR adjacency, bounded by the component it finds rather than by
the mesh.

---

## 7. Landed so far

| Commit | Contents |
|---|---|
| `d6e4310` | **Part B registry.** `FINAL_CLEARANCE_GAP_MM` = 0.300 (new, Measured/owner_decision, EXTERNAL REQUIREMENT); `STANDOFF_MM` relabelled honestly, value untouched. Ledger, generated golden, §3.5. Registry 305 → 306. |
| `33f93f4` | **Kernel.** `segment_clearance_excluding` (per-row global-face exclusion; `segment_clearance` delegates to it with an empty set, so the demanded-gap arithmetic has ONE statement) and `faces_near_segments_batch` (the band, one batched call per row set, refuses rather than truncating). 9 tests. |
| `1c646aa` | **`plinth-support::seat_zone`.** `freeze_seat_zones` — one batched band call plus a parallel flood fill per row — and the freeze/monotonicity tests. |
| `e74ab97` | **Challenge finding A**, folded in ahead of the wiring it gates: the manifold-bridge fixture, and the rule's measured boundary. §9.1–9.2. |
| `3fe332b` | **§8 step 1.** `ForestState::seat_zones`, `set_seat_zone` / `seat_zone`, the resize-survival and digest-inertness tests. |
| `005d56b` | **§8 step 2.** `ContactPolicy::demanded_gap_mm` and the named `ClearanceStage`; all four production `segment_clearance` sites read their stage's registered demand. §9.3. |
| `8b6cb14` | **Challenge finding E**, note text only: the retired `seg_clear_one` name in the `STANDOFF_MM` provenance, its ledger twin, §3.5 and twelve live source comments. |
| `0a72bc8` | **§8 steps 3–4.** `AnchorGeometryAuthority` retains its `FaceTopology`; all four creation sites freeze `Z`. §11.1–11.3. |
| this pass | **§8 steps 5–6.** `inset0 = inset1 = 0` at all four grading sites, exempting the frozen face set; the trip-wire's successor; the sabotage estate and its red-runs. §11.4–11.7. |
| (earlier) | **Challenge findings B1–B4 and N1** — the Bat movement recorded and controlled, the parked state pinned by a test, the two unpinned tier reads pinned, and `retired_formula_guard` strengthened so a production fork cannot hide behind a test-module occurrence. §10. |

---

## 8. Remaining wiring, in dependency order

Written out because the next pass should not have to re-derive it.

**STATUS (2026-08-22, after the challenge).** Steps 1 and 2 have landed; steps
3–7 have not, and **this tip is NOT mergeable on its own** — §10 records why, in
what state it is parked, and what clears the park. Read §9 BEFORE step 3: it
records what the wiring pass measured, including two places where this list is
incomplete as written (§9.4 names a fourth creation site and a bifurcation
inheritance rule; §9.5 names a batch-limits question that needs an owner).

1. **`Forest`** — **DONE** (`3fe332b`). Add `seat_zones: BTreeMap<MemberKey, Arc<[u32]>>` to
   `ForestState`; `set_seat_zone` / `seat_zone`; leave `digest()` alone. Confirm
   `apply_resize` preserves keys (it must, or the freeze cannot survive a
   resize — this is the property under test).

2. **`ContactPolicy`** — **DONE** (`005d56b`). Add `demanded_gap_mm`. `brace.rs:70`
   sets `final_clearance_gap_mm`; `cco.rs:4304` sets `standoff_mm`. Every
   `segment_clearance*` call passes `policy.demanded_gap_mm` rather than
   `config.standoff_mm`. *As landed:* `brace.rs:70` takes a NAMED
   `contact::ClearanceStage`, because the brace pass is the one stage that reads
   BOTH demands — `evaluate_candidates` is PLACEMENT under ruling R-1495 and
   `exact_clearance_rows` is FINAL — and a bare number at those two sites could
   silently swap them. `standoff_mm` stays a separate field: the PR-2 mask's
   standoff term is a different quantity from the demand and did not move.

3. **`FaceTopology` threading** — `brace_fixed_point_current_parity` →
   `brace_pass_current_parity` → the model-rung builder; `cco` routing →
   `clearance_survival`; `rewire::validate_changed_geometry`. The runner already
   retains exactly one `FaceTopology` beside the mesh and the `NativeSurface`
   (`SupportRunContext::topology`), so nothing new is constructed.

4. **Creation sites freeze `Z`:**
   * `brace.rs` model rungs — the seat projection already runs
     (`surface.project(&probes)`); capture the face ID it currently discards,
     and freeze in the same batch boundary as the existing seat-attestation
     batch.
   * `cco::apply_model_anchor` — the seat and its snapped face normal are
     already resolved per seat in `generate_descriptors`; carry the face ID on
     the descriptor and freeze at commit.
   * CCO candidacy rows freeze from their OWN current radius, which IS their
     creation radius: nothing has resized at candidacy, so freezing there is the
     freeze, not a recompute.

5. **Grading sites** — `inset0 = inset1 = 0` into
   `segment_clearance_excluding` with the row's frozen `Z`; keep the PR-2 mask
   computation alive for `axis_illegal_rows` and PR-3 only (§5).

6. **Tests** (each with its named sabotage): monotonicity, blind spot,
   two-tier (0.45 mm true gap: refused at a single 0.6, legal under two-tier,
   still refused below 0.3), frozen-`Z`, registry-read.

7. **Scoped gates**, then per-model runs with `spec6` skipped BY NAME, then every
   moved pin/canary re-derived through its producer with from → to evidence and
   the ruling cited. `EMISSION-CANARY-LEDGER.md` gains new values of record
   marked *honest-exemption + two-tier re-baseline*.

**Expected outcomes** (the judge verifies; the implementer does not tune toward
them): the Bat's crevice arms grade against TRUE gaps ≈ 0.291 mm (slot 250 final
radius) and < 0.439 mm (slot 377 final radius) and are LIKELY STILL REFUSED at
0.300 — which is honest; its island needs the land-at-strike fix, a different
lane. Do not try to make the Bat emit.

---

## 9. Wiring progress, and what the wiring pass MEASURED

Steps 1 and 2 have landed. Steps 3–7 have not. Everything below is a MEASURED
fact from the pass that landed them, recorded here so the next pass inherits it
rather than rediscovering it.

### 9.1 Finding A — the gate held

The challenge required the connectivity separator to be tested where it matters
BEFORE the wiring landed: on a MANIFOLD bridging fixture rather than on the two
disjoint sheets `a_zone_stops_at_the_crevice_and_never_crosses_to_the_foreign_wall`
uses, which cannot cross because nothing joins them.

`a_zone_stops_at_the_crevice_on_a_manifold_bridge_where_the_walls_really_are_connected`
(`seat_zone.rs`) is that fixture: a U-channel — near wall, floor, far wall — one
connected sheet, 0.4 mm seat radius (so a 1.0 mm band) across a 1.2 mm crevice.
Each ingredient is asserted, not assumed: an unrestricted fill from the seat face
reaches the far wall (it IS a bridge); the band contains far-wall faces (the test
is not vacuous); the connecting path is 5 mm below the arm and outside the band.

**VERDICT: the flood fill HELD.** The frozen zone is near-wall faces only — no
floor face, no far-wall face. Deleting the one line that keeps the fill inside
the band turns this test RED while the old disjoint-sheet test stays GREEN, which
is the finding demonstrated rather than argued.

### 9.2 The rule's boundary, measured

`the_zone_does_span_the_channel_when_the_band_itself_contains_the_connecting_path`
is the opposite pole of the same fixture: with the arm lying along the bottom of
the crevice, the band swallows the corner where floor meets far wall, and the
zone is then the WHOLE band — 13 near + 16 floor + 11 far = 40 faces.

This is §2 read literally and it is the rule's boundary: the exemption is bounded
by the band and by nothing else. An arm jammed into the bottom of a 1.2 mm
crevice forgives both its walls. It is pinned so that any later narrowing of the
zone — a normal test, a geodesic radius, a floor-facing exclusion — has a test it
must revisit BY NAME.

### 9.3 `axis_illegal_rows`' admission window moves with the demand

MEASURED while landing step 2. `brace::axis_illegal_rows` admits a row to the
penetration-interval batch only when its clearance sits in `[0, 0.1)` or its
midpoint is inside the model. That window is expressed in CLEARANCE, so it MOVES
when the demanded gap moves: at `STANDOFF_MM` it meant "true gap in
[0.600, 0.700)", and at `FINAL_CLEARANCE_GAP_MM` it means "true gap in
[0.300, 0.400)".

On the required runner corpus that emptied the batch: it had occupants of the
first band and none of the second, so the transit screen now examines nothing
there, and `brace_penetration_cap_minus_one_fails_closed_at_runner_boundary` went
VACUOUS rather than red. It was replaced by two tests that cannot
(`brace::a_capped_penetration_batch_refuses_rather_than_skipping_the_transit_screen`
constructs its own admitted row;
`runner::the_callers_brace_penetration_authority_is_the_one_the_brace_stage_spends`
reads the authority back out).

The 0.1 is the same defect SHAPE as challenge finding D one level over — a screen
expressed in a quantity that moves when a demand moves. NAMED, NOT FIXED: it is
not this change's to move, and no constant moved for it.

### 9.4 Step 4's creation-site inventory is INCOMPLETE as written

§8 step 4 names two mints and the candidacy rows. The honest inventory of members
that reach a clearance grading site carrying `model_contact_end().is_some()` is
larger, and step 5 must not land until every one of them freezes a `Z` — a
contact-declaring row with no frozen zone exempts NOTHING and is then graded over
its own designed embed, which refuses it:

* `brace.rs` model rungs (`append_candidate`, `MemberKind::Other(MODEL_RUNG_FAMILY)`,
  `Rootward`) — as written;
* `cco::apply_model_anchor` (`MemberKind::Strut`, `Rootward`) — as written;
* `cco::apply_bifurcation` — the upper half INHERITS the host's `Rootward`
  declaration and the host's `p1`, i.e. the same seat. It must inherit the host's
  frozen zone (an `Arc` clone, no native call); the contact was created once and
  the freeze belongs to the contact, not to the member;
* `initial.rs::stub_member` (`MemberKind::Stub`, `Terminalward`) — NOT in §8 and
  it must be. Stubs are excluded from `brace::exact_clearance_rows` by kind, but
  NOT from `cco::clearance_rows`, which walks `live_tree_members` and keeps every
  CHANGED member. A stub is a designed 0.3 mm embed; graded full-length with no
  zone it refuses itself, and CCO growth would lose its candidates wholesale.

### 9.5 The batch limits for `freeze_seat_zones` need an owner, not an invention

`NearFaceBatchLimits` is caller-owned by construction, and this lane is under a
standing "no new constant, cap, gate, threshold or tolerance" restriction. The
two honest options are (a) a mesh-derived bound — `max_faces_per_row =
topology.n_faces()`, which cannot refuse a legitimate band but is only the
trivial bound — or (b) a real production authority beside
`brace_penetration_limits` on `SupportPreparationLimits`, which is a caller
authority rather than a registered constant but is still a NUMBER someone must
own. RAISED, NOT DECIDED.

---

## 10. The parked state — what this tip IS, and what it is not

The challenge of 2026-08-22 rejected the tip on four blocking findings. All four
are answered here. Everything in this section is MEASURED at this branch tip
unless it says INFERRED.

**THE ORDER QUESTION, SETTLED.** Step 2 landed the FINAL demand of 0.300 mm
ahead of steps 3–5, which are what make the predicate it feeds grade a TRUE gap.
That order is not a merge-ready state and is not claimed to be one: **this tip
must not merge until §8 steps 3–5 land.** It is parked rather than reverted
because the pieces already in — the registry entries, the kernel's
face-excluding clearance, the seat-zone module, the one-predicate-two-demands
wiring — are the ones steps 3–5 build on, and unwinding them would cost the
whole lane to buy back a state nobody is shipping from either. The judge holds
the merge; this section is what the judge holds it against.

### 10.1 Finding B1 — the Giant Bat moved, with its control

MEASURED 2026-08-22 on `Giant Bat.stl`, `saturn4-ultra`, `--no-cache`, `gate`
profile, one machine, back to back. The two binaries differ by exactly one line —
`BraceConfig::contact_policy`'s `ClearanceStage::Final` arm reading
`self.final_clearance_gap_mm` (tip) versus `self.standoff_mm` (control):

| | control — FINAL = `STANDOFF_MM` 0.600 | tip — FINAL = `FINAL_CLEARANCE_GAP_MM` 0.300 |
| --- | --- | --- |
| outcome | REFUSED at `Structure` | passes `Structure`, `Lowering`, `CoverageMeasurement` |
| detail | hollow shell `TerminalEvidenceMismatch { terminal: 298 }`; substituted solid `{ terminal: 187 }` | enters coverage repair at `remaining=6221` |
| exit | 5 | did not terminate |
| wall | 75.1 s | stopped by the operator at 915.4 s |

At 915.4 s the tip run stood at `trials=42 commits=0 remaining=6179
[no_improvement=42]`. The rate is exact and flat: **one ranking candidate
consumed per trial, 21.79 s per trial, zero commits**. INFERRED from that rate,
exhausting the 6,221-candidate ranking would take **≈ 37.4 hours**, and ruling
R10 removed the failed-trial cap, so nothing shorter bounds it.

**The cause is the demand and only the demand** — the control is the same tree
with that one line reverted and it reproduces the refusal exactly. The direction
is Craig-authorized ("corpus artifacts, pins and canaries may move BOTH
directions; 0.3 final legalizes"), and the movement itself is the authorized
kind: arms the old 0.600 refused are now legal, so the model gets past
`Structure` instead of dying there. What is NOT acceptable is the landing place.
A 75-second honest refusal became a 37-hour walk that commits nothing, inside the
coverage-repair subsystem whose controller tests are 4-of-8 known red (`#39`).

**This is a NEW value of record, not a fix.** `EMISSION-CANARY-LEDGER.md` carries
it. Task `#47` already owns "Giant Bat: `Brace(ClearanceViolation(1))` on the
coverage-REPAIR path"; the run above is the same model reaching the same
subsystem from the other side, and the 0-commit walk belongs to `#39`'s
transactional-repair lane, not to this one. Do not tune the demand to make the
Bat refuse again — the design note's own instruction stands: **do not try to make
the Bat emit, and do not try to make it refuse either.** Report what IS.

The other three models of the run set are **byte-identical** to the lane base;
see the ledger rows.

### 10.2 Finding B2 — the honesty precondition is NOT met, and that is now pinned

`FINAL_CLEARANCE_GAP_MM`'s provenance registers an HONESTY PRECONDITION: the
number only means what it says because the test it feeds grades a TRUE gap. At
this tip it does not. `brace.rs` `exact_clearance_rows` still computes the
retired radius-proportional PR-2 mask and still hands it to the kernel as insets
(that is §8 step 5, unlanded), so the tested STATION SET still moves with the
radius.

MEASURED through the production final pass, one axis at `|cos| = 0.2`, one
foreign panel, both radii on the SAME axis against the SAME surface:

| | thin `r = 0.20 mm` | fat `r = 0.90 mm` |
| --- | --- | --- |
| FINAL = 0.300 (this tip) | **−0.45 → REFUSED** | **+0.05 → LEGAL** |
| FINAL = 0.600 (pre-ruling) | −0.75 → refused | −0.25 → refused |

The thinner, cleaner arm is refused and the fatter, closer one is legal. And the
loosening is not order-neutral: at the pre-ruling single demand both rows
refused, so **step 2 landing alone converts a both-refused pair into an inverted
accept of the fatter arm.**

This is pinned in the tree, not argued in prose:
`brace::tests::the_final_pass_still_grades_a_thinner_arm_worse_until_the_zones_are_wired`
asserts the inversion and prints both readings. **It is expected to go RED when
steps 3–5 land** — under the frozen-zone rule monotonicity is a THEOREM (§3), so
the correct successor assertion is `thin >= fat` on the same fixture. The test's
own message says so. That makes the parked state a trip-wire rather than a note
somebody has to remember to re-read.

The registry says the same thing where a reader of the constant will meet it:
`FINAL_CLEARANCE_GAP_MM`'s provenance note and §3.5's row both now carry the
precondition's STATUS and these numbers. No VALUE moved for it, and none should
— the value is not the defect, the predicate is.

### 10.3 Finding B3 — the two unpinned tier reads, pinned

Ruling R-1495 and the two-tier split name four production reads. Two were
already loud under sabotage; two were silent — the challenger switched them and
the entire scoped estate stayed green. Both now have a test that goes red, and
each red-run was performed rather than assumed:

| site | tier | pinned by | sabotage that turns it RED |
| --- | --- | --- | --- |
| `brace::exact_clearance_rows` | FINAL | pre-existing (2 tests) | Final arm → `standoff_mm` |
| `rewire::validate_changed_geometry` | PLACEMENT | pre-existing (1 test) | `demanded_gap_mm` → 0.300 |
| `cco::clearance_survival` | PLACEMENT | **NEW** `cco::tests::clearance_survival_prunes_at_the_placement_demand_not_the_final_one` | `seat_contact_policy`'s `demanded_gap_mm` → `FINAL_CLEARANCE_GAP_MM`; verdict flips `Distance` → `Survives` |
| `brace::evaluate_candidates` (R-1495) | PLACEMENT | **NEW** `brace::tests::evaluate_candidates_prunes_at_the_placement_demand_not_the_final_one` | `ClearanceStage::Placement` → `Final`; viability flips `false` → `true` |

Both new tests use the two-tier fixture §8 step 6 names, built so the answer is
strictly between the two demands and cannot be reached by either by accident: a
0.45 mm TRUE gap read back FROM THE KERNEL at demand 0, asserted to satisfy
`0.300 < 0.45 < 0.600` against the REGISTRY rather than against literals, plus a
control row that survives at both tiers so a uniformly-refusing fixture cannot
pass by accident. The `cco` fixture deliberately declares NO model contact, so
its insets are zero and no mask can be blamed for the verdict — the demand is the
only thing under test.

### 10.4 Finding B4 — the convergence guard could be satisfied by a test

`retired_formula_guard`'s positive half scanned the whole file. `cco.rs`'s own
`#[cfg(test)] mod tests` calls `placed_mask_inset_mm` twice, so a production fork
could pass the guard on the strength of a test occurrence. MEASURED both ways on
the same sabotage — the production call at `cco.rs` replaced by a locally
re-implemented mask built from `seat_depth_mm` + `mask_extent_mm`:

* the OLD whole-file scan: **GREEN** (the guard saw the test module's calls);
* the NEW `production_only` scan: **RED**, naming `cco.rs`.

`production_only` cuts the source at the module-level `#[cfg(test)] mod …`. It
is deliberately narrow — it matches only an UNINDENTED `#[cfg(test)]` whose next
line is an unindented `mod `, because `cco.rs` carries two item-level
`#[cfg(test)]` attributes at lines 41 and 50, in the middle of production text,
and cutting at the first attribute of any kind would amputate the file. A file
with no inline test module is scanned whole, which is the safe direction.

Two tests keep the strengthening honest: a synthetic fixture that is the defect
in miniature (production body forks the mask, `mod tests` still names the helper)
and a check that every `CONVERGED` file really does have a test module for the
cut to bite on — otherwise the fix would be inert.

**This had to land BEFORE §8 step 5**, which deletes exactly those production
calls. Step 5 must delete the `CONVERGED` rows deliberately and say why, not have
the guard quietly keep passing.

### 10.5 Finding N1 — the R-1495 site was absent from all three records

`brace::evaluate_candidates` is a placement reader by ruling and named none of
the three places the placement demand is recorded. All now name it, with the
ruling and with the test that pins it: `STANDOFF_MM`'s provenance note
(`placeholders.rs`), its ledger `normative` twin
(`m0-constants-source-ledger.json`), §3.5's row, and the same omission in
`FINAL_CLEARANCE_GAP_MM`'s note. The generated Markdown golden was re-emitted by
its producer and the diff is exactly two rows, with both VALUES untouched at
0.600 and 0.300.

### 10.6 Inherited from the challenge, for step 4

The challenger verified independently — record it here so step 4 does not have to
re-derive it, because the flood fill rests on it entirely:
`plinth_support::validate_perception_input` builds
`build_face_topology(mesh, WeldPolicy::ExactBits)` and refuses any closed-solid
cut flag, and `NativeSurface::from_indexed(mesh.verts(), mesh.tris())` indexes
faces in the same triangle order. So `surface.project`'s face ID and
`topology.neighbors` address the SAME faces, and adjacency is not fragmented.

### 10.7 What clears the park

§8 steps 3–5, in that order, with §9.4's full creation-site inventory and §9.5's
batch-limits question answered by an owner. When they land:

1. `brace::tests::the_final_pass_still_grades_a_thinner_arm_worse_until_the_zones_are_wired`
   goes red — flip it to `thin >= fat` and say so in the commit message.
2. `FINAL_CLEARANCE_GAP_MM`'s precondition STATUS paragraph and §3.5's row come
   out, replaced by the commit that made the precondition true.
3. The Giant Bat is re-run and re-recorded. It may still refuse, it may still
   walk; either is an honest answer and neither is a target.

---

## 11. Steps 3-6, as landed

Everything in this section is MEASURED at this branch tip unless it says
INFERRED.

### 11.1 Step 3 — the `FaceTopology` threading, and what it cost

`AnchorGeometryAuthority` already TOOK a topology: `new` validated it, bound its
content digest into the fingerprint, and dropped the reference. It now retains
it behind `topology()`. That was cheaper than threading a second, unbound
`&FaceTopology` down three call chains, and it makes the surface and the
adjacency provably the same pair the fingerprint attests rather than two
arguments a caller could mismatch.

| chain | how the topology arrives |
| --- | --- |
| `brace_state` → `brace_fixed_point_current_parity` → `brace_pass_current_parity` → `gather_candidates` | `geometry.topology()` |
| `grow_terminal_with_finalized_anchor` → `grow_terminal_inner` → `generate_descriptors` | `context.route.geometry().topology()` |
| `evaluate_rewire_candidates` → `generate_descriptors` | `anchor_route.geometry.topology()` |
| `rewire::validate_changed_geometry` | already held the authority |

The runner builds no second topology. The adjacency identity the flood fill
rests on is RECORDED rather than assumed, as the challenger required:
`validate_perception_input` builds it with `WeldPolicy::ExactBits`, and
`NativeSurface::from_indexed` indexes faces in the same triangle order, so
`surface.project`'s face IDs and `topology.neighbors` address the same faces.
Every test fixture in this change builds its topology with the same policy, so
no fixture can pass on a welding production would not do.

### 11.2 Step 4 — the creation-site inventory, AS LANDED

§9.4 named four sites. The tree confirms four production mints, plus one site
examined and excluded:

| # | site | kind / end | how it freezes |
| --- | --- | --- | --- |
| 1 | `initial.rs` stub mint | `Stub` / `Terminalward` | `freeze_stub_seat_zones`, after grow-to-attest so the band radius is the EMITTED tip. One batched `project` plus one batched band call for the whole run. |
| 2 | `brace.rs` model rungs | `Other(model_rung)` / `Rootward` | The seat projection already ran and DISCARDED its face ID; capturing it costs no native call. One batched freeze per pass, riding the seat-attestation batch boundary. The zone travels on `PlacementCandidate`, so `evaluate_candidates` and `append_candidate` read the same face set. |
| 3 | `cco::apply_model_anchor` | `Strut` / `Rootward` | Frozen once per seat in `generate_descriptors`, from the seat's already-snapped face; carried on the descriptor as a finished `Arc<[u32]>` and installed by an `Arc` clone. |
| 4 | `cco::apply_bifurcation` | inherited `Rootward` | `Forest::take_seat_zone` MOVES the host's zone to the upper half. Never recomputed. |

**Why site 3 carries the ZONE and not the face ID**, which §8 step 4 had not
worked out: the commit path re-runs `apply_and_size` against the LIVE forest and
throws the candidate forest away
(`forest.digest() == winner.forest.digest()`, `grow_terminal_inner`). A zone
derived at application time from the forest being written into could differ
between the candidacy run and the replay. One carried on the descriptor cannot.

**Site 4 MOVES rather than copies.** When the host's `Rootward` declaration is
cleared, a copy left behind would exempt faces for a member that no longer has a
seat. `Forest::take_seat_zone` exists for that, and the pin asserts `Arc::ptr_eq`
— comparing CONTENTS would pass for a recompute that happened to land on the
same faces; comparing ALLOCATIONS cannot.

**Examined and EXCLUDED:**
`designed_embed::SeatAttestationRow::provisional_member` mints a member with
`model_contact_end: Some(..)`, but it lives only inside the attestation batch,
never enters a forest, and never reaches a clearance grading site. Every other
`model_contact_end: Some(..)` in the tree is test code.

### 11.3 §9.5 — the batch limits, RESOLVED (Craig may veto)

`seat_zone::production_limits`, a MESH-DERIVED bound:
`max_faces_per_row = topology.n_faces()`, `max_rows = rows`,
`max_total_faces = rows * n_faces` (saturating).

The orchestrator's ruling was to MIRROR `PenetrationBatchLimits` /
`ContactConstructionLimits::production()`, deriving values the way theirs were
derived, and to STOP rather than invent a new number. Reading their provenance
settles it: every one is registered as an *"Inherited unmeasured cap … Codex-era
`73e7c13` … records the inherited number without ratifying it"*. Their
derivation is INHERITANCE, and this batch family has nothing to inherit — there
is no near-face number anywhere in the tree. Mirroring literally would mean
typing three fresh numbers and registering them as unmeasured, which is exactly
the invention the ruling forbids. So the ruling's own escape clause applies and
§9.5's option (a) is what landed.

It invents nothing. A band cannot contain more faces than the mesh has, so the
bound is the exact worst case: a TRUE STATEMENT of the ceiling rather than a
lever, which cannot refuse a legitimate band and cannot decide an outcome. It
admits `rows × n_faces × 4` bytes — 949 MB worst case at the Weeping Treant's
553 contacts and 429,120 faces, against §6's measured realistic zone of tens of
faces (≈ 0.45 MB) and the 100 GB machine ceiling. The kernel's
refuse-rather-than-truncate semantics are inherited untouched.

**RAISED FOR CRAIG.** If he wants a real numeric authority beside
`brace_penetration_limits`, it is a one-line change at one call site.

**STATED PLAINLY, after challenge finding F3 (2026-08-22).** Two things this
subsection implied rather than said:

1. This is a RATIFICATION item, not a resolved one. The orchestrator's ruling
   said mirror-or-STOP-AND-REPORT; the escape clause was read as licensing
   option (a) and option (a) landed instead of the lane stopping. The provenance
   argument above is why, and nothing was invented — but the call is still one
   Craig may veto, and it is listed with the other open items in §11.9.
2. The bound is INERT in production. `max_rows = rows`,
   `max_faces_per_row = n_faces`, `max_total_faces = rows × n_faces`
   (saturating) — a batch cannot exceed a limit computed from the batch itself.
   So `faces_near_segments_batch`'s fail-closed refuse-rather-than-truncate
   behaviour can never fire on this path in production, and the 949 MB worst
   case is bounded by nothing except the realistic zone size (§6: tens of
   faces). The refusal semantics are still exercised — `seat_zone`'s own tests
   pass tightened limits deliberately — but as a contract, not as a live
   production guard. If Craig wants a real ceiling that can bite, that is
   option (b) and it needs a number.

### 11.4 Step 5 — the four grading sites

| site | stage | what changed |
| --- | --- | --- |
| `brace::exact_clearance_rows` | FINAL | zero insets + frozen `Z`; the PR-2 mask survives as `mask_inset0/1` and goes ONLY to `axis_illegal_rows` |
| `brace::evaluate_candidates` | PLACEMENT (R-1495) | zero insets + the candidate's frozen `Z`; `Rod::inset0/1` stay the mask, for `candidate_revalidation`'s transit screen |
| `cco::clearance_survival` | PLACEMENT | zero insets + each row's frozen `Z`; the mask survives as `mask_inset0/1` for PR-3 `emergence_ok` |
| `rewire::validate_changed_geometry` | PLACEMENT | zero insets + `after.seat_zone(key)`; the mask survives for PR-3 |

Every site looks the zone up ONLY for a row that declares a contact. A member
with no seat forgives nothing, and a contact-declaring row with no frozen zone
gets an EMPTY exclusion — graded over its own designed embed and refused. That
is the strict direction, chosen deliberately: a missed creation site surfaces as
a refusal, never as a silent pass.

The census `record_segment_batch` calls now record ZEROS, because those are the
insets the batch actually receives. Recording the mask would describe a call the
site no longer makes.

**CORRECTED 2026-08-23 (judge's evidence-integrity finding).** That sentence was
true of three of the four sites and FALSE of the fourth: `brace_candidate_clearance`
still handed the census `inset0`/`inset1`, the retired PR-2 mask, while handing
the kernel zeros. No verdict anywhere read it — the census is an instrument, and
the corpus tables it feeds were all produced before this change — but it is
exactly the kind of evidence this note forbids. The site now records the SAME
`zero` vector the distance call receives, so the two cannot drift, and the two
`Vec`s that existed only to feed the census are gone.

`retired_formula_guard`'s three `CONVERGED` rows all STAY, and §10.4's
instruction to "delete them deliberately and say why" is answered by the
opposite finding: all three files still call `placed_mask_inset_mm` in
production, for §5's two surviving readers. The guard's own prose was corrected
so it no longer claims to be about insets.

### 11.5 The trip-wire successor, MEASURED

`the_final_pass_still_grades_a_thinner_arm_worse_until_the_zones_are_wired` is
replaced by
`a_thinner_arm_never_grades_worse_than_a_fatter_one_through_the_production_final_pass`
— same fixture, same axis, same surface, one frozen `Z`, two radii.

| | thin `r = 0.20 mm` | fat `r = 0.90 mm` |
| --- | --- | --- |
| FINAL = 0.300, at this tip | **−0.45 → REFUSED** | **−1.15 → REFUSED** |
| FINAL = 0.600 (pre-ruling) | −0.75 → refused | −1.45 → refused |
| the retired rule, for contrast | −0.45 → refused | **+0.05 → LEGAL** |

`thin >= fat` at both demands, which is the theorem. **The old inversion pair is
now BOTH-REFUSED.** The thin arm's number did not move at all — the retired rule
already graded it honestly, because its mask was small. The fat arm moved from
`+0.05` to `−1.15`: that is the blind spot closing, and it is the whole change
in one number. The frozen zone is 2 faces (the table's own top), so the foreign
panel is graded and the seat is not.

### 11.6 The sabotage estate, with every red-run PERFORMED

Six named sabotages, each applied to production text, run, and reverted.

| sabotage | applied to | RED |
| --- | --- | --- |
| **A** `an_exact_pass_that_kept_the_retired_mask_as_an_inset` | `exact_clearance_rows` hands `mask_inset0/1` to `segment_clearance_excluding` | 4 tests: monotonicity, blind spot, seated legality, seating-independence |
| **B** `a_final_pass_that_exempts_nothing` | `exact_clearance_rows` passes an empty exclusion per row | 3 tests: blind spot, seated legality, seating-independence |
| **G** `an_exact_pass_that_zeroed_its_transit_masks` | `exact_clearance_rows` hands zeros to `axis_illegal_rows` | 1 test: `the_exact_pass_hands_the_transit_screen_its_masks_and_not_the_zeroed_insets` |
| **C** `a_bifurcation_that_dropped_the_hosts_zone` | `apply_bifurcation` skips the inherit | 1 test: `a_bifurcated_upper_half_inherits_the_hosts_frozen_zone_rather_than_recomputing_it` |
| **F** `a_placement_screen_that_exempts_nothing` | `cco::clearance_survival` passes an empty exclusion | 5 tests, including `the_model_anchor_arm_that_failed_by_exactly_its_radius_now_routes` |
| **D** `a_mint_that_skipped_the_stub_freeze` | the `freeze_stub_seat_zones` call deleted | 2 tests: both stub pins |

Two results are recorded because they are NEGATIVE, and hiding them would be the
defect this estate exists to prevent:

* **Sabotage B leaves monotonicity GREEN**, and that is correct rather than a
  hole. The theorem does not require `Z` to be non-empty: with no exemption at
  all both arms are graded over their own embeds and `thin >= fat` still holds.
  Monotonicity's sabotage is A, and A turns it red.
* **Sabotage G left the ENTIRE brace suite green when first run.** Finding D had
  no production-path pin at all — `the_transit_screen_keeps_the_seat_mask_…`
  calls `axis_illegal_rows` directly, so a fork at `exact_clearance_rows` was
  invisible to it. That is the same shape of hole challenge finding B3 named for
  the two unpinned tier reads. It is now closed by
  `the_exact_pass_hands_the_transit_screen_its_masks_and_not_the_zeroed_insets`,
  which drives the production function and asserts (with a stated non-vacuity
  check) that the row really did land in the screen's `[0, 0.1)` admission
  window — MEASURED at `clearance = 0.020`.

A seventh sabotage, `a_band_frozen_from_the_law_radius` (`seed.law_r0` for
`seed.r0`), is **SILENT on the available fixture** and says so in the test: the
overhang table grows no tip, so `law_r0 == r0` for all 91 stubs and the two
bands are the same number. The test prints the growth population, so the day a
fixture grows one the sabotage becomes loud automatically.

### 11.7 Moved pins, and one cost the judge must price

Two lib pins moved, both expected, both re-derived through their producer:

* `brace::tests::an_unseated_contact_is_not_over_masked_by_the_seat_depth`
  asserted `clearance = MARGIN + (STANDOFF_MM − FINAL_CLEARANCE_GAP_MM) = 0.35`
  and now reads `−0.8`. That number was an ARTIFACT of the retired rule: it
  measured where the mask placed the exact pass's first tested station, and the
  exact pass no longer has such a station. Replaced by
  `the_exact_pass_no_longer_reads_the_seat_depth_that_the_mask_still_carries`,
  which keeps MAJOR-6's actual rule (the mask carries the depth term for a
  seated end and not an unseated one — asserted at `placed_mask_inset_mm`, where
  the mask still lives) and adds the stronger new statement: the same geometry
  and the same frozen zone, graded once as an unseated family and once as a
  seated one, now read the SAME clearance (`9.2` both ways; masks `1.3` vs
  `1.6`). `seating_of`'s two directions remain pinned by
  `seating_follows_the_design_family`, so nothing was lost.
* `cco::tests::the_model_anchor_arm_that_failed_by_exactly_its_radius_now_routes`
  read `[Distance]` instead of `[Survives]`. Its helper built the candidate's
  rows by hand and never froze a zone, so the seated arm was graded over its own
  embed — the honest rule working exactly as designed. The helper now freezes
  where production does, BEFORE `clearance_rows` reads the forest, and #51's
  before/after pair is intact. The trailing direct-kernel assertion is relabelled
  as the RETIRED path's historical control rather than a live claim.

**THE COST, MEASURED — one test, 5.5x.** Per-test wall clock, `gate` profile,
this machine, tip against the lane base at `562d829`:

| test | base | tip | verdict at both ends |
| --- | --- | --- | --- |
| `closure_repairs_censored_dust_because_that_repair_closes_a_real_hole` | 133.7 s | **738.6 s** | FAILED (inherited `#39`) |
| `public_runner_closes_overhang_table_only_after_appending_repair_contacts` | 2.10 s | 1.46 s | FAILED (inherited `#39`) |
| `public_runner_measures_sealed_coverage_…` | 0.90 s | 0.85 s | ok |
| `public_runner_seals_toed_slab_…` | 0.94 s | 0.87 s | ok |

The whole movement is one already-red coverage-repair test, and it is the SAME
SHAPE as §10.1's Giant Bat finding: the honest exemption legalizes arms the
retired skip-length refused, so the repair loop — the subsystem `#39` owns and
whose controller tests are 4-of-8 known red — explores far more before giving
up. INFERRED, not measured: nothing here says the walk terminates sooner or
later than it did; only that it is longer. This is `#39`'s lane, not this one's,
and it is recorded so the judge prices it rather than discovers it.

### 11.8 What this pass did NOT do

The corpus re-baseline is the judge's (§8 step 7). No model was run, no canary
row was written, and `EMISSION-CANARY-LEDGER.md` is untouched by this pass. The
inherited `#39` reds are byte-identical in name and count between the lane base
and this tip: `a4_transactional_controller` 4-of-8 and
`support_runner_integration` 2-of-4, VERIFIED by running both binaries at
`562d829` in a linked worktree.

### 11.9 Open items, and the one that needs Craig

The round-5 challenge returned ACCEPT with four non-blocking findings. Three are
folded in above and in the test estate; one is a decision this lane may not take
for itself.

#### F1 — RULED BY CRAIG, 2026-08-23. RE-SIZE THE EXEMPTION TO THE ARM'S FINAL FOOTPRINT

**The ruling, verbatim:** when the sizing pass thickens a seated arm past its
creation radius, the frozen exemption patch (seat zone) must be RE-SIZED to match
the arm's FINAL footprint — *"the exemption always equals the arm's true
footprint, no slack."*

**Rejected, for the record:** accept-as-is (disposition 3 below) and
pad-at-creation (disposition 2 below).

**Craig explicitly confirmed the foreign-gap case is intended and untouched.** An
arm placed with 0.6 mm of true gap to a foreign feature and then thickened to
0.35 mm of remaining gap is still graded against that foreign feature at the
final tier and refuses if the gap is short. The re-size forgives the arm's own
seat and nothing else; it does not buy clearance from anybody else's geometry.

**What the ruling changes.** The band radius is no longer "the radius at the
moment of the mint" — it is "the radius at the contact end, now". One batched
recompute runs after each sizing pass and before the next grading pass
(§2, §11.10). Because every production sizer leaves the contact end at least as
fat as the other end (`legacy_compat.rs:116`,
`transient.rs`'s non-uniform branch, `cco::uniformly_size_ancestor_path`), the
contact-end radius IS `r_max` for every seated production member, so the graded
clearance after a re-size is

```text
clearance = (r_final + STANDOFF_MM + s) − (r_max + demanded_gap)
          = STANDOFF_MM − demanded_gap + s
```

with `s` the tessellation slack — strictly positive, because a face that straddles
the band boundary is admitted to the band whole, so every RETAINED face is
strictly farther than the band from the axis. At FINAL that is `+0.300 + s`; at
PLACEMENT it is `+s` exactly. **The placement margin is tessellation slack and
nothing else.**

**MEASURED, and the number is much smaller than the first rung suggests.** On the
`fine_seat_slab` fixture (0.25 mm triangles, 2,182 faces), across the six-rung
ladder `r_max` = 0.35 → 1.00 mm, by
`brace::a_seated_arm_that_grows_is_re_sized_to_its_final_footprint_and_stays_legal`:

| `r_max` mm | 0.35 | 0.40 | 0.50 | 0.65 | 0.80 | 1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| zone faces | 116 | 128 | 154 | 200 | 232 | 304 |
| FINAL mm | +0.350 | +0.331 | +0.318 | +0.325 | +0.314 | +0.301 |
| PLACEMENT | LEGAL | LEGAL | LEGAL | LEGAL | LEGAL | LEGAL |
| PLACEMENT margin mm (INFERRED) | 0.0500 | 0.0308 | 0.0180 | 0.0248 | 0.0142 | **0.0008** |

`s` does not fall smoothly — it wobbles, because it is mesh aliasing rather than a
quantity — but on this fixture it ranges from 0.050 mm down to **0.0008 mm**,
0.8 micrometres. So the honest statement is: **after the re-size the placement
tier stops refusing a grown arm, but the margin it keeps is whatever the
tessellation happens to give, and that can be a fraction of a micron.** Quoting
the 0.050 mm first rung alone would be the favourable reading. The SIGN is
structural; the SIZE is not a guarantee of anything.

For contrast, the same ladder with no re-size refuses four of the six rungs at
PLACEMENT and two at FINAL — the table further down.

The rest of this subsection is the measurement that RAISED F1, kept because it is
the before-picture the flipped test's sabotage half still reproduces.

#### F1, as measured before the ruling

The band is frozen at the arm's CREATION radius, and every face outside the band
is by construction farther than that radius plus `STANDOFF_MM` from the axis. So
on a real, finely tessellated seat the arm's OWN surface is what grades it, with
no foreign geometry anywhere in the mesh, and the graded clearance is

```text
clearance ≈ (r_create + STANDOFF_MM) − (r_max + demanded_gap)
```

Every millimetre the arm grows after its freeze is a millimetre off that number,
one for one. `cco::generate_descriptors` freezes the model anchor's band at
`min_shoot_radius_mm` and its own comment records that
`size_tapered_without_tree_fanin` may raise the radius afterwards;
`cco::clearance_survival` then grades at PLACEMENT with the SIZED radius. That is
`#51`'s shape — "unsatisfiable by exactly the arm's own radius" — one level up.

**MEASURED**, by `brace::a_seated_arm_that_grows_after_its_freeze_spends_its_headroom_one_for_one`,
through production `exact_clearance_rows` (FINAL) and production
`evaluate_candidates` (PLACEMENT), on a 0.25 mm-triangle slab of 2,182 faces
with a 116-face zone and a 0.95 mm band, frozen at the registry's
`min_shoot_r_mm` = 0.35 mm:

| `r_max` mm | 0.35 | 0.40 | 0.50 | 0.65 | 0.80 | 1.00 |
| --- | --- | --- | --- | --- | --- | --- |
| FINAL (0.300 demanded) | +0.35 | +0.30 | +0.20 | +0.05 | **−0.10** | −0.30 |
| PLACEMENT (0.600 demanded) | LEGAL | LEGAL | **REFUSED** | REFUSED | REFUSED | REFUSED |

**PLACEMENT refuses above `r_max` = 0.400 mm — 0.05 mm of headroom. FINAL
refuses above `r_max` = 0.700 mm.** Both crossings are probed a thousandth
either side, so they are kernel verdicts rather than arithmetic, and the
0.300 mm between them is exactly the two-tier gap of §4.

**Why no existing pin caught it.** The production pin
`a_seated_arm_stays_legal_against_its_own_designed_embed_and_refuses_without_its_zone`
uses `flat_table`, whose top face is TWO triangles 40 mm across. The zone is
therefore the whole tabletop and the nearest non-exempt face is the 10 mm-away
underside, so the same arm reads **+8.75 mm at PLACEMENT even at
`r_max` = 3.0 mm** (MEASURED by the challenger). That pin proves the exemption
EXEMPTS and nothing more; it is now annotated to say so and to name its
successor. The new fixture `fine_seat_slab_parts` exists because of this: a
seat whose zone is a disc of the band's own size, which is the geometry a real
mini presents.

**What is NOT claimed.** Nothing here says a production arm is in fact refused.
`zombie-3-a` gained records rather than losing a family, and its placement
refusal rate HALVED on this lane (0.402 → 0.200 per trial). This is a
sensitivity, measured and pinned, not an observed regression.

**The three dispositions that were put to Craig:**

1. **Freeze at `max(r_create, final r_max)`** — re-freeze the band after sizing.
   Correct in shape, but it means a second freeze pass and a zone that is no
   longer a creation-time constant. **THIS IS WHAT HE RULED**, in its exact form
   rather than its `max(...)` form: the band equals the FINAL contact-end radius,
   whether that is larger or smaller than the creation radius. No slack means no
   slack in either direction.
2. **Widen the seat's exemption by the growth** — keep one freeze, add the
   growth to the band radius. **REJECTED** — the band stops being "the arm's own
   footprint" and becomes a number with slack in it.
3. **Accept the cost** — 0.05 mm is the headroom, and the sizing pass must not
   exceed it. **REJECTED.**

The pin `a_seated_arm_that_grows_after_its_freeze_spends_its_headroom_one_for_one`
was flipped by the ruling into
`a_seated_arm_that_grows_is_re_sized_to_its_final_footprint_and_stays_legal`, and
the numbers above survive inside its sabotage half, which skips the re-size and
watches the 0.400 mm crossing come back.

#### F2 — CLOSED by measurement

§5's claim that this change "must not move" the axis-transit screen was true of
the MASKS and false as written of the ADMISSION rule. The paragraph is corrected
in place, and the property that matters is now MEASURED by
`brace::a_seated_arm_that_keeps_running_inside_the_model_is_still_condemned_by_the_transit_screen`,
which drives a seated arm 2.7 mm into the slab past its designed embed, asserts
the distance path reads it wide open (so the midpoint-inside branch is what
admits it — the branch finding D's pin cannot reach) and pins the verdict at the
screen's own `−min_illegal_len_mm`. Any movement in `zombie-3-a`'s counters
remains an outcome for the judge's re-baseline to read, not a claim this note
makes in advance.

**The LABEL on the 52 is corrected too, and this is the second half of F2.**
The 52 is not a re-entry count. It is the number of `zombie-3-a`'s 92 repair
trials whose own appended contact was quarantined, for ANY reason:
`runner.rs` writes `own_contact_quarantined` from
`matches!(disposition, TrialContactDisposition::Quarantined { .. })`, a
variant test. The re-entry-specific evidence the disposition carries —
`remaining_inside_axis_mm`, which is `None` precisely when the quarantine was
for something other than stub re-entry — is discarded by that pattern and is
written nowhere on the step, so no persisted `*.coverage.repair.json` sidecar
at any commit lets a true re-entry count be recovered. Comparing 52 against a
re-baselined 52 compares quarantine counts and nothing narrower.

#### F3 — RATIFICATION, stated plainly

§11.3 now says outright that this was an escape-clause reading rather than a
resolved question, and that the bound is INERT in production
(`max_rows = rows`, `max_faces_per_row = n_faces`, saturating product), so the
kernel's refuse-rather-than-truncate can never fire on this path. Craig may veto;
a ceiling that can actually bite needs a number, and this lane declines to invent
one.

#### F4 — CLOSED

`the_exact_pass_hands_the_transit_screen_its_masks_and_not_the_zeroed_insets`
asserted its fixture check before its claim, so the sabotage red-run read "the
fixture must land the clearance in [0, 0.1)" — true, but it names the fixture
when the defect is the production fork. The condemnation now fails first and
names itself; the admission-window check stays as the non-vacuity guard it was
always meant to be.

---

## 11.10 The re-size, AS LANDED (Craig's ruling, 2026-08-23)

`seat_zone::refreeze_resized_seat_zones` is the ONE recompute. It scans the
forests it is handed, finds every seated member whose stored `band_radius_mm`
no longer equals `r_contact + STANDOFF_MM`, and re-cuts exactly those in ONE
`faces_near_segments_batch` call for the whole slice — however many forests and
however many members that is.

`SeatZoneRecord` is what makes that possible. It carries the frozen zone plus the
two facts a re-size needs and a member cannot supply: the seat's own face ID (the
contact's identity on the mesh, resolved once by the creation site's projection —
re-projecting could walk the seat onto a different face and quietly redefine
which contact it is) and the band radius the zone was cut at. The AXIS is
deliberately not stored: the re-size reads `p0`/`p1` off the live member, which is
the same axis every grading site reads, so a stored copy could only go stale.

**Why the change test is `!=` and not a tolerance.** `band_radius_mm` is a value
this module itself wrote from `r_contact + STANDOFF_MM`. Asking whether the same
arithmetic on today's radius yields the same bits is a comparison, not a
threshold, and it introduces no number. Gating on the sizing plan's own `changed`
list instead would inherit that list's `TRANSIENT_TIE_TOLERANCE_MM` and would
miss a sub-tolerance drift that `set_radii` nevertheless wrote.

### The four call sites — every one between a sizing pass and the next grading pass

| # | flow | after | before | batch |
| --- | --- | --- | --- | --- |
| 1 | `cco::grow_terminal_inner` | `evaluate_descriptors` + the anchor-foot retain — every candidate applied AND sized | `clearance_survival` (PLACEMENT) | ONE call across EVERY candidate forest in the tournament. Covers the model anchor (`size_tapered_without_tree_fanin`) and the bifurcation upper half (`size_bifurcation_candidate`) in the same call. `ClearanceRow` snapshots the zone, so touched candidates' rows are re-read — pure CPU, no native call. |
| 2 | `cco::grow_terminal_inner`, commit path | `apply_and_size` on the LIVE forest | `forest.commit` | ZERO native calls. The winner's candidate forest already holds the re-cut record and the digests just proved both forests have the same live members under the same keys, so the commit COPIES rather than recomputing — bit-identical by construction, the same argument `apply_model_anchor` makes for carrying the zone on the descriptor. The zone table is invisible to `Forest::digest` (pinned by `freezing_a_seat_zone_does_not_move_the_forest_digest`), which is what lets the equality check run first. |
| 3 | `brace::brace_fixed_point_current_parity` | `apply_resize` | `exact_clearance_negatives` (FINAL) | ONE call per controller iteration — there is one sizing pass per iteration. |
| 4 | `brace::pending_triggers_after_trial` | the trial `apply_resize` | the trial `exact_clearance_rows` | ONE call, on the batch boundary this function already declares; the count is now reported as `seat_zone_refreeze_batches` and pinned at 1. Missing this site would grade the trial against pre-resize bands, leave the trigger unrelieved, and place a rung nobody needed. |
| 5 | `rewire::validate_changed_geometry` | every caller's sizing (`try_slide`'s junction sizing, `reconcile_ancestors`' path walk) | the PLACEMENT screen | ONE call. `after` became `&mut` for this and nothing else. |

### The two flows that CANNOT resize a seated member — proved, not instrumented

Neither gets its own call site, and neither is dead code: both are covered by the
same uniform scan wherever it runs, so the scan is what would notice if either
stopped being true.

* **Model rungs.** `brace::append_candidate` mints them with `Forest::add_brace`,
  which puts them in `state.braces`. `Forest::live_tree_members` filters that set
  out (`forest.rs:743-748`). `transient::postorder` walks only
  `live_tree_members` (`transient.rs:689-714`), and `postorder` is the only thing
  `resize_forest_current_parity` iterates. A rung is never reached.
* **Stubs.** `postorder` additionally skips `MemberKind::Stub` when pushing to its
  order (`transient.rs:702-707`), and while a stub is a registered terminal
  `Forest::set_radii` refuses it outright via
  `require_terminal_mutation_allowed`. Two independent guards.

### Why the ruled behaviour is structural rather than fixture-luck

Every production sizer leaves the CONTACT end at least as fat as the other end:

* `legacy_compat::size_live_current_support_member` clamps
  `rootward = required.max(terminal)` (`legacy_compat.rs:116`), and that is what
  `cco::size_tapered_without_tree_fanin` returns through
  `o10_uniform_radius_for_span`;
* `transient::resize_forest_current_parity`'s non-uniform branch does the same
  (`rootward = max(required, old_terminal)`), and its uniform branch writes both
  ends equal;
* `cco::uniformly_size_ancestor_path` writes both ends equal.

The seated production families declare `Rootward` (CCO anchor, bifurcation upper
half, model rung) or are never resized at all (`Terminalward` stubs). So
`r_contact = r_max` for every seated member a re-size can touch, and the graded
clearance after a re-size is `STANDOFF_MM − demanded_gap + s`, independent of the
radius. Were a future family to end up thinner at the contact end than at the far
end, the ruling would leave it a residual self-refusal — the band would be its
true footprint at the seat, which is the ruling, but `r_max` would exceed it.
Nothing in the tree does that today; the shape of the exposure is recorded here so
it is a known edge rather than a surprise.

### Test estate

* `brace::a_seated_arm_that_grows_is_re_sized_to_its_final_footprint_and_stays_legal`
  — the ruling, through the PRODUCTION helper (not a hand-rolled re-freeze) and
  both production grading sites. Non-vacuity: the un-grown rung keeps exactly its
  created patch and every grown rung claims a strictly larger one, which is what
  replaces the retired pin's own "the ladder must span LEGAL to REFUSED" guard.
* `brace::a_seat_zone_that_was_not_re_sized_after_the_growth_refuses_the_arm_again`
  — the sabotage `a_sizing_pass_whose_seat_zones_were_never_re_cut`, and the
  place F1's original measurement is KEPT rather than deleted.
* `brace::shared_trigger_relief_uses_one_resize_and_one_clearance_batch` now also
  pins `seat_zone_refreeze_batches == 1`.
* `forest::a_frozen_seat_zone_survives_the_resize_that_changes_its_member_radius`
  — narrowed, not weakened: the side table is PASSIVE, so a re-size has to be
  asked for, and it must not move the band radius either.

**RED RUN, PERFORMED.** With `refreeze_resized_seat_zones` neutered to return
`Ok(0)`, the new pin fails twice over: first on its re-cut count (`0 of 6 rungs
re-cut`), and with that guard lifted, on the clearances themselves — reproducing
the pre-ruling ladder `[+0.350, +0.300, +0.200, +0.050, −0.100, −0.300]` exactly,
which is F1's original table row for row.

### Three facts the `!=` change test depends on, checked rather than assumed

1. **`STANDOFF_MM` reaches every site as the same bits.** The `!=` only means
   "the sizer moved this" if the `standoff_mm` a re-size adds is bit-identical to
   the one the creation site added. All six reads are the same expression on the
   same registered placeholder — `p.standoff_mm().diagnostic().get()` —
   at `initial.rs:628`, `brace.rs:139` (carried through `contact_policy` at
   `brace.rs:97`), `cco.rs:1336` and `cco.rs:4884`, and `rewire.rs:144`/`1047`.
   No site recomputes or rescales it. If any of them ever diverges by one ULP the
   `!=` fires for every seated member on every pass; the cheap runtime witness is
   that the brace fixed-point site's re-cut count is 0 on a model where nothing
   grows.
2. **The re-size reads the CONTACT end, not whichever end is handy.** The
   production case is TAPERED — `cco::apply_model_anchor` puts
   `cco_terminal_radius` at the free end and `min_shoot_radius` at the seat, and
   the sizer raises the seat end alone. Swap the two `ContactEnd` arms and the
   band would be cut at the thin free end, bringing F1 back in exactly the flow
   §11.9 measured. Every other test of the re-size uses uniform radii and a
   `Terminalward` contact, so none of them could catch that.
   `brace::a_tapered_arm_is_re_sized_from_its_contact_end_and_not_its_free_end`
   is the one that can: MEASURED, the free end's band cuts 116 faces, the contact
   end's cuts 270, and the re-size produces 270.
3. **The side table is checkpointed.** `Forest::checkpoint` clones the whole
   `state`, which contains `seat_zones`, and `rollback` restores it
   (`forest.rs:1305-1326`). So the CCO commit path's `NonDeterministicWinner`
   rollback restores the pre-`apply_and_size` table exactly, and a re-cut record
   cannot survive a rolled-back candidate.

### The corpus sanity run: `zombie-3-a` is BYTE-IDENTICAL, and here is why

Both runs at the `gate` profile, `saturn4-ultra`, `--no-cache`, one model in the
input directory.

| | binary sha256 | records | bytes | md5 | wall |
| --- | --- | --- | --- | --- | --- |
| 600be86 | `5f50fec3…` | 215 | 3,750,864 | `c13ca528…` | 276 s |
| this branch | `c7e7cb4a…` | 215 | 3,750,864 | `c13ca528…` | 285 s |

Both trees were clean at their commit before building, and the baseline
reproduces the ledger's own Stage D row for `5f5c2d6` exactly — which is the
binary-provenance check: the baseline binary is the commit it claims to be, and
`600be86` is docs-only against `5f5c2d6`.

`<stem>.coverage.repair.json`: 1,948 scalar fields, **4 differ**, and all four are
`wall_time_*`. That is the ledger's own criterion for "moved nothing".

**Byte-identical was written down beforehand as a FINDING TO CHASE, not a pass**,
because §11.9's premise is that the model anchor's band is frozen at
`min_shoot_radius` and the sizer raises it immediately after. So it was chased,
with a temporary instrumented build (a print at the re-size, reverted after):

* **1,311 CCO tournaments** ran on this model.
* **Zero** of them re-cut a single band.
* **Zero** `refreeze_resized_seat_zones` calls anywhere — brace controller, brace
  trial, CCO, rewire — found anything to re-cut, across the whole run.
* Every tournament reports every candidate carrying a seat zone (13/13, 14/14,
  26/26, 39/39 …). Those zones are the STUBS: `initial.rs` freezes one per stub
  and every candidate forest inherits them.

**The reason, from the run's own evidence.** `zombie-3-a` never places a model
anchor. Its `own_contact_no_winner` steps record
`anchor_seat_refusals_recorded` = 69,211–69,552 with
`anchor_foot_refusals_recorded` = 0: every anchor seat is refused at
QUALIFICATION, before any candidate exists, so `generate_descriptors` mints no
`ModelAnchor` and the tournament is Plate + Bifurcate only. A `Bifurcate` can
only inherit a zone from a host that declares `Rootward`, which only a committed
model anchor creates. So the only seated members on this model are stubs — and
stubs are exactly the flow §11.10 proves cannot be re-sized.

**What this does and does not establish.** It establishes that the change is
INERT on a model with no placed model anchor, that the wiring runs on the real
pipeline without erroring, and that the `!=` change test does NOT spuriously fire
(1,311 chances, zero false re-cuts — which is the runtime witness the `standoff_mm`
provenance argument asked for). It does NOT exercise the F1 path on the corpus,
because this model never reaches it. A model that DOES place a model anchor is
the run that would; `zombie-3-a` is not it, and saying so is more useful than
reporting "no change" and stopping.
