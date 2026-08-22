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

**frozen at contact creation.** `Z` is stored with the contact and is never
recomputed from a later radius.

`r_seat` is the member's radius at the contact end at the moment the contact was
minted. `standoff_placement` is `STANDOFF_MM`. Both are frozen together as one
`band_radius_mm`.

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

The freeze is what makes it unconditional. Recompute `Z` from the final radius
and the theorem dies: a fatter arm claims a wider band, a wider band swallows
more faces, and the fatter arm is graded against strictly less surface than the
thin one. That is the skip-length's inversion rebuilt one level up, and it is the
sabotage `the_zone_does_not_move_when_the_radius_does_because_it_is_never_recomputed`
reproduces (`seat_zone.rs`).

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
   #52's lane (stub axis re-entry); this change must not move it, and the 52
   stub-re-entry condemnations on `zombie-3-a` stay untouched by design.

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
| this pass | **Challenge findings B1–B4 and N1** — the Bat movement recorded and controlled, the parked state pinned by a test, the two unpinned tier reads pinned, and `retired_formula_guard` strengthened so a production fork cannot hide behind a test-module occurrence. §10. |

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
