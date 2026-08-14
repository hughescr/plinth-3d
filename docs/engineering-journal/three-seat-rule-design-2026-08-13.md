# The three-seat rule — DESIGN, 2026-08-13

> **Provenance.** Written 2026-08-13 against `develop @ 7f8e4dd`, on branch
> `feature/three-seat-rule` (worktree `plinth-wt-three-seat`). Every `file:line` citation below is
> at that tip. **This document and two follow-up rows
> ([FU-86](CURRENT-STATUS.md#follow-ups-2026-08-04),
> [FU-87](CURRENT-STATUS.md#follow-ups-2026-08-04)) are the whole diff — no production code
> changes on this branch.** **No cap, limit, threshold, tolerance, gate or registered constant is
> raised, lowered, widened or bypassed by anything proposed here**, and the design deliberately
> proposes none.
>
> **Amended 2026-08-13 by an implementation pass**, which independently re-verified every citation
> in §1 against the code, corrected two claims it found wrong (§6's lean/max-length claim, and
> §7's third reviewer check), and **built, ran and then withdrew** the §6 fix after measuring that
> it regresses a shipping corpus model. The measurement is §6.1; the patch is preserved in §6.2.
> The gate is green on this branch because the withdrawal is complete: **1354 passed / 0 failed**,
> identical to the unmodified tip.
>
> **Verdict: this is architecture, not wiring.** The scoping brief offered two paths — "the seam
> exists, so wire it up" or "this is real architecture, so design it and do not half-build it".
> The honest answer is the second, and §1 is the evidence.

---

## The owner's ruling, verbatim, which is the authority for everything below

> "We DO want supports inside the model. The hollow must be supported. But those supports will
> likely land on the lower part of the interior of the cavity, not directly to the base plate. The
> connection to the base plate would be via the model itself and its outer supports. In general
> supports should count as valid if they land on the model, on the base plate, or on other
> supports as long as the entirety is adequately supported per physics."
>
> — Craig, 2026-08-13

---

## §0. The answer in one paragraph

Two of the three seats already work. A support can stand on the **base plate** (137 wins across
the corpus) and on **another support** (121 wins). The third — standing on the **model's own
surface** — is built end to end as *candidate machinery* and is **switched off at the last
statement before it becomes geometry**. Across the whole corpus, **0 of 3,429 attempted
model-seated arms have ever survived**. They do not lose a fair contest. They are held out by a
deliberate, recorded safety catch, because the arm as built does not actually *seat into* the
model — it stops exactly at the surface and kisses it. The missing piece has a name in the
project's own record, **PR-7 `CCO_ANCHOR_MASK_AT_THE_SEAT`**, and it was left unbuilt on purpose,
waiting for an owner decision recorded as **OQ-10**. **The ruling quoted above is the answer to
OQ-10.** So the work is not "let cavity islands nominate cavity seats" — they already can. The
work is to give a model-seated arm a real, proven interface with the model, retire the safety
catch that stands in for that proof today, and only then ask whether the cavity ceiling gets
supported. §5 sizes that job.

**§6 was written as "the one small step that ships value now". It is not one — and finding that
out is the most useful thing this pass did.** The step (apply the minimum arm length to the seat
the router actually proposes, not to the sample that merely aimed the ray) was built and run. It
does exactly what it promised for the hollow toad, and it **regresses `make_overhang_table` from
shipping to failing**, taking the gate from 1354/0 to 1349/6. It is a **provable pure tightening**
that broke a working model anyway, because refusing a candidate pushes the terminal onto the
engine's **zero-candidate path, and that path aborts instead of routing**. That path is fault
group B, which must therefore land first. The fix is withdrawn, measured, and preserved in
§6.1–§6.2 for the lane that owns group B. **There is no step that ships value now.**

---

## §1. What exists today — the four questions, answered from the code

### §1.1 What seat targets does the router support?

All three, as first-class members of one tournament. The router (the "CCO" stage, which grows one
support path per hanging contact) builds a list of *descriptors* — proposals — for each hanging
point, then prices them and elects the cheapest:

`crates/plinth-support/src/cco.rs:977-999`

```rust
enum Descriptor {
    Plate      { xy: [f64; 2], top_z: f64 },   // stand on the build plate
    ModelAnchor{ seat_index: usize, point_mm: [f64; 3] },  // stand on the model
    Bifurcate  { host: MemberKey },            // stand on another support
}
```

`generate_descriptors` (`cco.rs:3559`) emits, for every hanging point: one plate descent directly
below it, optionally a spiral of further plate descents, **one `ModelAnchor` per discovered seat**,
and one `Bifurcate` per nearby existing support. `compare_candidates` (`cco.rs:3309`) ranks purely
by price — the resin volume the proposal adds — with the generation index as the tie-break. There
is no family preference and no plate bonus in the ranking.

**Standing on another support is not theoretical: it ships.** The bifurcation family wins 121 of
the 258 committed routes in the corpus census
([`census-embed-incidence-2026-08-04.md`](census-embed-incidence-2026-08-04.md), tournament
counters table). The brace stage independently builds cross-links between supports and those are
its normal product.

### §1.2 Where does the code assume "a support reaches the plate"?

**Nowhere as a requirement.** Four places are *aware* of the plate; none of them demands that a
support reach it:

| Site | What it does | Is it a "must reach plate" rule? |
| --- | --- | --- |
| `cco.rs:3559` `generate_descriptors` | always offers a plate descent as one proposal | No — one candidate among many |
| `cco.rs:4068` `size_plate_candidate`, `cco.rs:4297` `candidate_price` | special-case the plate family's two-member column-and-diagonal shape when sizing and pricing it | No — family-specific arithmetic, the other two families have their own |
| `raft.rs:176` `plan_bottom_connected` | collects members whose lower end lies within the plate band and rafts them | No — it is a **filter**. A model-seated root is simply not a raft candidate |
| `lower.rs:412` `is_plate_root` | decides which member gets a root boss and flare | No — same filter shape |

**But there is a hole, and it is the crux of this design.** The forest arena holds no plate
reference at all, so `Forest::validate` **cannot tell a legitimately seated root from one hanging
in mid-air**, and the lowering stage **silently omits the root boss** for any root outside the
plate band rather than objecting. The code says so itself, at `forest.rs:872-881`:

> *"a promoted child keeps the rootward endpoint it had while it hung off its parent, so its `p1`
> sits wherever the retired member's terminal-ward end was — in the air, not on the plate. Nothing
> downstream catches it. The arena holds no plate reference, so `Forest::validate` cannot tell that
> root from a legitimately grounded one, and `lower` simply omits the root boss for a root outside
> the plate band rather than objecting."*

So the absence of a "must reach the plate" rule is not evidence that the three-seat rule is
supported. It is evidence that **nothing verifies that a non-plate root is attached to anything at
all.** That verification is what PR-7 is for, and it is why the safety catch in §1.5 exists.

### §1.3 How does the load-path check work, and does it already say "carried by whatever I stand on"?

Yes — structurally, this part is already right.

Load is an **area**: how many square millimetres of hanging model surface one arm has been made
responsible for. `propagate_carried` (`cco.rs:4201`) adds a newly routed area to every member from
the contact **up the parent chain to the root, then stops**. A model-seated arm has no parent — it
*is* the root of its chain — so the load terminates at the seat. That is precisely "carried by
whatever I stand on".

Each member is then sized against the area it carries by `o10_uniform_radius_for_span`
(`cco.rs:4374`), and every model contact is graded by `contact::area_sufficient`
(`contact.rs:352`) against the registered allowance `X_ALLOWED = 12` — the design target in the
owner's physics, chained from the measured plate interval, never hand-typed. **This design
proposes no change to that law and no change to that number.**

What the load path does **not** do is prove the seat exists. `area_sufficient` grades the area a
contact *claims*; the witness that the claimed area is really in contact with solid material is a
separate, batched geometric test (`designed_embed::attest_seats_batch`, `designed_embed.rs:831`).
For model-seated arms that witness **fails on every seat**: see §1.5.

### §1.4 Is the cavity interior visible as a place to stand?

**Yes, and it already is being used as a place to hang from.**

Every stage that looks at model surface filters through `NativeSurface::exterior_face_mask`
(`rust/geom_kernel/src/queries.rs:2780`). That mask is not "outside the model's silhouette"; it is
"a step along this face's own normal lands in empty space rather than buried in printed
material". A cavity wall's normal points into the cavity void, so **a cavity wall face is
`exterior` by that test**. Three consequences, all confirmed by measurement:

* `perceive_contacts_on_surface` (`lib.rs:824`) keeps down-facing exterior faces as places needing
  support — which is why the drilled Giant Toad's **cavity ceiling was detected as an island at all**
  (135.58 mm², 1,388 faces).
* The seat pool `SurfaceAnchorPool` (`anchor.rs:275`) is built from exactly the same mask
  (`anchor.rs:320`), so **cavity wall faces are already in the pool of places an arm may stand.**
* The measured run agrees. On the drilled hollow toad, the closure evidence records island 0 with
  **13,457 eligible repair rows** and the router reports **39 qualified model-anchor seats** at the
  failing terminal.

So the perception and candidacy answer to "can an arm stand inside the cavity?" is **already yes**.
Nothing needs unblocking there.

### §1.5 So why has a model-seated arm never once survived?

Because the arm does not seat. `apply_model_anchor` (`cco.rs:3871`) sets the arm's lower end to the
seat point **exactly**:

```rust
p1_mm: finite3(point_mm)?,        // the seat point itself, not seated below it
```

`contact.rs:83` `seating_of` classifies every family except the brace model rung as
`ContactSeating::OnSurface`, and its own comment states the plan: *"PR-7 will seat the CCO family;
when it lands, that family moves to `Seated` here."* Until then the arm rests on the surface
instead of biting into it, and the geometric witness records that as `NotAttached`. The corpus
measurement is unambiguous: **all 3,186 attested model-anchor seats report `centreline = false`
with 1–3 of 8 rim rays engaged** — the signature of a kiss, not a joint. In a real print those arms
would fuse to the mini only by voxel rounding, which is the exact failure mode the owner named when
the deferral was recorded.

The safety catch is `elect_committable_winner` (`cco.rs:3345`). It runs at the last statement
before the winner becomes geometry and **refuses any model-seated winner whose seat witness said
`NotAttached`** — which is all of them. The refusal is graceful: the candidate leaves, the next
survivor is elected, the hanging point routes some other way, nothing hard-fails.

**This is the whole gap.** It is recorded, deliberate, and reserved to the owner:

* **PR-7 `CCO_ANCHOR_MASK_AT_THE_SEAT` — NOT landed, deliberately**
  ([`contact-physics-2026-08-04.md`](contact-physics-2026-08-04.md) *What is not done*).
* **OQ-10 — "revive the CCO model-anchor family?"** — recorded as *"a print-consequence tradeoff,
  not a physics question"*, reserved to the owner, and carried live in the ledger as
  [FU-17](CURRENT-STATUS.md#follow-ups-2026-08-04).

**The ruling at the head of this document answers OQ-10 in the affirmative.**

---

## §2. What the ruling changes, and what it does not

**It changes:** the answer to OQ-10. Model-seated arms are wanted. Interior arms are wanted. A
permanent, unremovable arm inside a cavity is accepted, with its scar invisible and its weight
small.

**It does not change:** the physics. The area law stays at the recorded design target of 12, the
0.050 mm arm requirement stays, brace clearance stays non-negative, and the seat witness is not
weakened — the ruling says *"as long as the entirety is adequately supported per physics"*, which
is a demand for proof, not a waiver. Reviving the family by deleting the safety catch would be the
opposite of what was ruled: it would ship arms that *look* attached and are not.

---

## §3. The design

Five pieces. They are ordered; each is measurable on its own; **the first two must land together**
because either alone leaves the engine in a worse state than today.

### §3.1 Piece A — seat the arm (PR-7)

**What changes.** `apply_model_anchor` (`cco.rs:3871`) stops putting the arm's lower end *on* the
surface and puts it *below* the surface, by the depth the project's existing contact physics
already derives — the same call the brace model rung already uses:

`contact.rs:247` `seat_endpoint(p_surface, normal, other_end, seat_depth_mm)`, with the depth from
`contact.rs:219` `seat_depth_mm(abs_cos_incidence, end_radius, policy, ball_tipped)`.

**Why this is not a new rule.** Every number it needs is registered and already read by the brace
family: the seat depth regimes, the ball-tip seat factor (`contact.rs:114` `ball_cut_radius_mm`),
the clearance mask that a seated end requires (`contact.rs:276` `mask_extent_mm`, whose `h` term
exists *specifically* for seated ends), and the emergence rule. Nothing is invented and no constant
moves.

**Types touched.** `contact.rs:83` `seating_of` gains the CCO strut family, which is the single
place the seated/unseated decision lives — its doc comment already names this as the intended
edit, so the mask term becomes correct by the same rule rather than by a second, divergent
decision. `attest_model_anchor_seats` (`cco.rs:3758`) keeps its shape; its provisional member is
built from the seated endpoint instead of the surface point.

**What it costs.** This moves printed geometry. Every model-seated arm that becomes committable
changes the emitted support forest, so **goldens and census digests move**. Per the project's OD-8
protocol that is a stop-and-report movement with an attributed from→to record, not a silent
side effect.

### §3.2 Piece B — retire the safety catch, in the same commit

Once Piece A gives the arm a real interface, `NotAttached` stops being a statement about a missing
rule and becomes an ordinary defect. It then belongs at **candidacy**, refusing the individual
candidate the same way a breached foot ball does — the disposition the brace model rung already
has — and the commit-time tripwire in `elect_committable_winner` (`cco.rs:3345`) and its rewire
sibling comes out.

**These two pieces are one commit.** Piece A without Piece B changes geometry and still commits
nothing (the catch keeps refusing). Piece B without Piece A ships kisses as if they were joints —
the exact outcome the catch was built to prevent.

### §3.3 Piece C — the tournament-bias measurement

The record asks a question this design cannot answer by reading: with the family actually
committable, **do model-seated arms win, and where?** The counters already exist
(`cco_winner_model_anchor`, `cco_model_anchor`, the attempted/surviving census). The measurement is
a corpus run plus the seven large minis, reporting per model: attempted, qualified, attested,
committed, and the family split of every committed route. **Do this before tuning anything.** If
model anchors still lose everywhere, the cavity ceiling gets no interior arms and the rest of this
design is moot until that is understood.

### §3.4 Piece D — keep interior arms clear of the drain

**This does not exist anywhere today.** `crates/plinth-support` contains **no reference to drains
or vents at all** — the support stage does not know a drain hole exists. An arm standing near the
lowest point of a cavity can dam the resin that has to flow out of it, and the drain and vent bores
are only 2.5–3 mm across.

The design is a **keep-out**, expressed the way the anchor module already expresses refusals:

* The drilled shell's planned openings are already known — `plinth-hollow`'s drilling stage plans
  each drain and vent with a site, an axis and a bore radius, and refuses openings that are not
  clear passages.
* Pass those planned openings into the support stage as a read-only list, and add one refusal
  reason to `AnchorSeatRefusalReason` (`anchor.rs:868`) — a seat is refused when its foot ball,
  at the radius the foot will actually have, comes within the bore radius plus the registered
  standoff of a planned opening's axis, or lies below the opening's mouth in the drain's own flow
  direction.
* It is a **new refusal**, i.e. strictly a tightening. It admits nothing that is refused today, so
  it cannot be a cap relaxation. It needs **no new constant**: the bore radius comes from the
  opening the driller planned and the clearance from the registered `standoff_mm`.
* It is batched: one pass over the seat rows against a handful of openings, in the same shape as
  the existing foot-ball breach batch. There is no per-seat call in a hot path.

**Open question this piece raises and does not answer:** an arm may also stand *on the far side*
of the flow path and still be legal. The refusal above is a cylinder-and-mouth keep-out, which is
conservative. If it refuses too much on real cavities, the honest next step is to measure the
drainable interior with the arms in place — the machinery for that (`plinth-hollow`'s flow
component selection) already exists — not to shrink the keep-out by feel.

### §3.5 Piece E — prove the load path closes, and say so in the evidence

Structurally the load already terminates at the seat (§1.3). What is missing is that **nothing
records it**. Two additions, both reporting only:

* The support verification snapshot should name each root's seat kind — plate, model, or support —
  so the question "does the load path close?" is answerable from the evidence sidecar instead of by
  reading geometry. Today `Forest::validate` cannot distinguish them at all (§1.2).
* The coverage evidence should count interior arms separately: how many arms were committed inside
  a cavity, at what seat positions, and how much cavity-ceiling area they carry.

Without Piece E, the deliverable the brief asks for — *"islands covered, supports placed inside the
cavity (count, seat positions)"* — cannot be reported at all, because no counter in the engine
distinguishes an interior arm from an exterior one.

---

## §4. What is explicitly NOT in this design

* **No relaxation of anything.** The area ratio stays at 12. The 0.050 mm arm requirement stays.
  Brace clearance stays non-negative. The Weeping Treant's memory limit is untouched and out of
  scope.
* **No new "interior support" member kind.** The three families in `Descriptor` are sufficient; a
  cavity seat is an ordinary `ModelAnchor` whose seat happens to lie on a cavity wall. Adding a
  fourth family would duplicate the physics.
* **No change to how the cavity is perceived.** Cavity faces already reach the seat pool; §1.4
  shows the perception side needs nothing.

---

## §5. Rough size of the job

| Piece | Shape | Blast radius |
| --- | --- | --- |
| A — seat the arm (PR-7) | small edit at 2–3 sites, all physics already registered | **large**: moves printed geometry, moves goldens and census digests, OD-8 stop-and-report |
| B — retire the catch | delete two guards, move one disposition | none on its own; inseparable from A |
| C — bias measurement | corpus run + census reporting, no rule change | none |
| D — drain keep-out | new refusal reason + one batched screen + a new input into the support stage from the hollow stage | crosses a crate boundary (`plinth-hollow` → `plinth-support`); tightening only |
| E — evidence | reporting fields on the verification snapshot and coverage sidecar | serialized-evidence change, owes a re-pin plan |

**The dependency order is A+B → C → D → E**, with one exception: **D can and should be designed
before C is run**, because a bias measurement taken with no drain keep-out will report interior
arms that the keep-out would later refuse.

**The critical fact for scheduling: the hollow Giant Toad cannot exercise any of this today**,
because it fails before the tournament is reached. That is §6.

---

## §6. The smallest honest step that ships value now — and it is NOT part of the design above

**Separated deliberately.** This is a defect fix in the seat discovery screens. It has nothing to
do with the three-seat rule except that it currently prevents the hollow Giant Toad from ever
reaching the tournament, so it blocks the measurement in §3.3.

**The defect.** Seat discovery (`anchor.rs:1049-1200`) screens each candidate seat **twice**, and
the two screens are applied to **two different points**:

1. Against the sampled face centroid, it enforces the registered minimum arm length —
   `anchor.rs:1065`, `length < config.min_length_mm` → `TooShort`, with
   `STRUT_MODEL_ANCHOR_MIN_LEN_MM = 5.0` — and the maximum length and maximum lean.
2. It then casts a ray at that centroid and **snaps the seat to the first surface hit**, which can
   be far nearer than the centroid. The snapped point is screened only against
   `BELOW_LEVEL_EPS_MM = 1e-6` mm (`anchor.rs:1150`).

So the arm that is actually proposed can be **anywhere from 0.000001 mm to 5 mm long** while the
5 mm minimum reports as satisfied.

> **CORRECTION, 2026-08-13, from the implementation pass — the minimum length is the ONLY screen
> affected.** This section first claimed the maximum-lean screen "has the same shape". **That is
> wrong, and the two other screens need no post-snap re-test at all:**
>
> * **Lean cannot change.** The snapped point is `stub_end + direction * t` — it lies on the SAME
>   ray as the centroid — and `lean_deg` (`anchor.rs:1800`) is
>   `hypot(dx, dy).atan2(|dz|)`, which is invariant under scaling the delta by any `t > 0`. The
>   snapped lean is therefore **bit-identical** to the lean already screened.
> * **Maximum length cannot be exceeded.** `RAY_T_MAX = 1.0` (`anchor.rs:28`) caps the ray at the
>   sample point itself, so the snapped arm is never LONGER than the centroid arm already screened.
>
> Length can only shrink, and only downward past the minimum. Re-testing `TooLong` or `TooLean`
> after snapping would be dead code pretending to be a safeguard. **The fix is one screen, not
> three.**

**The consequence is a whole-model abort, not a lost candidate.** The seat attestation batch
requires each provisional arm to be longer than the process resolution, 0.050 mm
(`designed_embed.rs:1177` `validate_extent`, reached from `prepare_rows` at `designed_embed.rs:306`,
whose own comment reads *"one bad row refuses the whole batch"*). One sub-0.050 mm arm therefore
takes down every model that produces one:

| Model | Measured arm length (mm) | Short of 0.050 mm by |
| --- | ---: | ---: |
| Giant Toad (solid) | 0.049966772 | 33 nanometres |
| Rotting Warhorse | 0.042948868 | 7.05 µm |
| Giant Toad (swallowing) | 0.036538831 | 13.5 µm |

And the drilled hollow Giant Toad, at `develop`, fails exactly here:

```
sealed support run failed during Routing: Cco(InvalidGeometry)
```

> **Provenance note on that error text, 2026-08-13.** An earlier draft quoted this failure with a
> named cause attached — `Cco(InvalidGeometry("attest_model_anchor_seats: native seat-attestation
> batch over 39 seat(s) at extent 0.05 mm failed: designed extent must be shorter than its
> member"))`. **That string cannot be produced on this branch.** `CcoError::InvalidGeometry` is a
> bare unit variant at `7f8e4dd` (`cco.rs:962`); it only carries a cause on
> `feature/truthful-support-errors` (FU-58, commit `aac205b`), which is **not merged into
> `develop`**. The named form is the correct diagnosis and is what a reader should expect once
> FU-58 lands — but the text reproduced on this branch is the bare one above, and it is quoted
> that way here so the record matches what the tree actually prints.

**The fix, stated as a tightening.** Re-apply the screens the code already performs, to the point
it actually proposes: after snapping, refuse the seat as `TooShort` when the snapped length is
below the registered `STRUT_MODEL_ANCHOR_MIN_LEN_MM`, and as `TooLean` when the snapped lean
exceeds the registered maximum. Both refusals already exist; both are per-candidate, so the
candidate leaves the tournament and the hanging point routes some other way, which is the
engine's normal behaviour everywhere else.

**This raises, lowers, widens or relaxes nothing.** It applies an existing registered minimum where
it was always meant to apply, and it can only refuse candidates that are admitted today — never
admit one that is refused.

### §6.1 IT WAS IMPLEMENTED AND MEASURED — and it must NOT land yet

> **This subsection replaces an earlier paragraph headed "What it is expected to buy".** That
> paragraph was a prediction. The implementation pass of 2026-08-13 **built the fix and ran it**,
> and the prediction was half right in a way that matters more than the half that was right.
> **The patch is NOT in this branch's diff. It is preserved verbatim in §6.2.**

**What the fix bought — real, and exactly as predicted.** On the drilled hollow `Giant Toad`, the
one-line screen converts a hard abort into the engine's ordinary graceful behaviour:

| | attempts | outcome | wall clock |
| --- | ---: | --- | ---: |
| Before (HEAD) | 1 | hard abort at Routing, `Cco(InvalidGeometry)` | 1 min 48 s |
| After (screen applied) | 8 | `AttemptsExhausted`, coverage loop ran to its own limit | ~26 min |

The closure loop actually **worked** for the first time: blocking patches fell 240 → 168 → 153,
then plateaued at 166 and ran out of attempts. The toad still does not ship, but it now fails
**for an honest reason at a later stage** instead of one bad candidate killing the model. That is
the fault-group-A mechanism fixed, and it confirms the diagnosis above completely.

**What the fix cost — and this is why it must not land.** Measured on the full gate, with the
change as its only variable:

| Gate run | passed | failed |
| --- | ---: | ---: |
| HEAD, unmodified | **1354** | **0** |
| HEAD + the FU-87 screen | 1349 | **6** |

The six are not fixture noise. Three are `cco` tests whose fixtures no longer produce a surviving
`ModelAnchor` (fixable — they are geometry, like the three `anchor` fixtures the pass did correct).
**The other three are the blocker:**

* **`make_overhang_table` REGRESSES from shipping to failing.** A corpus fixture that emits today
  can no longer close its coverage: it aborts at Routing after 2 attempts with 13.68 mm²
  uncovered. It is a **net loss of a working model.**
* **`the_zero_field_divergence_is_measured_per_committed_fixture` flips `true` → `false`** on that
  same fixture — and that test's own message states that a flip *"means the record is now wrong and
  the recorded re-pin protocol is owed a fresh from→to attribution rather than an edited
  expectation here."* Editing the expectation to make the gate green would be exactly the thing
  the record forbids.

**The finding, which is the real product of this pass.** A change that is a **pure tightening** —
one that provably can only move a candidate from admitted to refused, never the reverse — **still
regressed a shipping model.** The reason is that refusing more candidates pushes terminals onto the
engine's **zero-candidate path, and that path is itself broken**: it aborts with
`Cco(InvalidGeometry)` instead of routing some other way. That is precisely **fault group B** in the
support brief (`Warhorse` 0.651 mm² over 13 spots with zero candidates; `Deinonychus` one triangle
with zero candidates).

**So FU-87 is BLOCKED ON fault group B, and the two must land in that order.** Fixing the
zero-candidate path first is what makes the minimum-length screen safe; landing the screen first
just converts one class of abort into another and loses a working model on the way.

**This also falsifies §7's third check as written** (below): "candidates move from admitted to
refused and none the other way" was **verified true** and the model regressed anyway. A tightening
is not automatically safe here. The check must be *"no model that emits today stops emitting"*.

### §6.2 The measured patch, preserved

The change is one `else if` arm in `discover_anchor_seats_bound`, between the existing
`ZeroLengthSnappedAnchor` arm and the seat-construction arm (`anchor.rs`, ~line 1150):

```rust
} else if length < config.min_length_mm.get() {
    // Screen the point we actually PROPOSE, not the sampled centroid
    // that merely aimed the ray. `RAY_T_MAX == 1.0` means the snapped
    // arm is never longer than the screened one, and `lean_deg` is
    // scale-invariant along the ray, so the minimum length is the only
    // screen that can be violated by snapping.
    Some(AnchorSeatRefusalReason::TooShort)
} else {
```

Three `anchor` unit-test fixtures also need their box tops lowered so they exercise a legitimately
seatable arm rather than a sub-minimum one — `frozen_qualification_and_stable_cap_vectors`,
`rewire_seat_is_bound_to_exact_root_checkpoint_and_validated_in_one_batch` and
`stale_and_rebound_authorities_refuse_before_any_sphere_query`, all of which put the stub end
2.5 mm above a box top and so admitted only sub-minimum arms. **With those three corrected, all 15
`anchor` tests pass**, including a new regression test pinning the defect directly. Those fixture
corrections are sound and should be carried forward with the fix when group B unblocks it.

**Caution on lanes:** this defect is fault group A in the current support brief and may already be
assigned. The diagnosis, the patch and the blast radius are all recorded here so whoever owns that
lane inherits the measurement rather than repeating it.

---

## §7. What a reviewer should check first

1. That §1.5 is right — run the corpus and confirm `cco_winner_model_anchor` is still 0 and every
   attested seat still reports `centreline = false`. If that has changed, this design's premise
   has moved.
2. That the cavity claim in §1.4 holds on the drilled toad specifically — dump
   `exterior_face_mask` over the 1,388 cavity-ceiling faces and confirm they are all `true`. The
   island detection implies it; a direct count proves it.
3. ~~That §6's fix is a tightening in fact and not only in intent — the refusal counts per model
   before and after should show candidates moving from "admitted" to "refused", and none moving the
   other way.~~ **RETIRED 2026-08-13 by the implementation pass: this check is not sufficient and
   must not be relied on.** It was verified TRUE for the §6 fix and the fix regressed
   `make_overhang_table` from shipping to failing anyway (§6.1). A pure tightening is not
   automatically safe in this engine, because refusing a candidate can push a terminal onto the
   zero-candidate path, which aborts instead of routing. **Replace it with: no model that emits
   today stops emitting, measured on the full gate and on the seven large minis.** The gate
   baseline to compare against is **1354 passed / 0 failed** at `7f8e4dd`.
