# Land the stub where it strikes the model — DESIGN

> **Provenance.** First written 2026-08-22 against `feature/a4-controller @ f50a45b` and
> committed as `aae1559`; that version was reviewed UNSOUND and re-written as `10b0bd8`,
> which the same reviewer graded SOUND-WITH-CHANGES and whose four required changes it
> carries. **This text is the AMENDMENT that folds Craig's four rulings of 2026-08-22
> (R1–R4, §0.1), and it moves the recovery table again — upward.** It is re-graded on the
> same measured evidence (probe P1, `.git/land-at-strike-p1-evidence/`) with a new grader
> (`regrade-r134.ts`, same directory) and re-anchored on the clearance work that has landed
> (`d6e4310`, `33f93f4`, `1c646aa`, `e74ab97`, `3fe332b`). **DOCS ONLY. No production code
> and no test code changed, and nothing under `crates/` was touched, built or staged — that
> tree belongs to the wire-clearance lane while this amendment was written.**
>
> **No cap, gate, threshold, tolerance or registered constant is moved, widened or
> bypassed by anything proposed here, and no constant is minted.** Every number this design
> uses is already in the registry, and the one derived quantity that is new to this text —
> the load-sized embed depth — is the engine's own `ball_seat_factor`
> (`contact.rs:186`, DERIVED, PR-1 clause (c)) evaluated at the registered
> `BALL_FACTOR_FOOT`.
>
> **What changed between `10b0bd8` and this text, in one paragraph.** `10b0bd8` graded every
> landed foot as a *standard* foot: 0.3 mm deep whatever it carried, with a ball 1.4 × its
> radius driven centre-deep into the struck wall. Craig ruled that wrong (R1): the foot's
> radius **and its depth** derive from the load, so a strut carrying a small load in a thin
> crevice gets a small anchor. He also ruled that a seat may go as deep as it likes but is
> **truncated at the opposite face** (R4), which makes punch-through impossible by
> construction and retires the foot-ball breach *refusal*. On those two rulings the rule
> recovers **129 of 208 measured records, not 80** — 467.07 mm² against 279.18 mm² — **and
> island 74, the island this lane was opened for, now lands.** No record that recovered at
> `10b0bd8` is lost: the change is a strict admission.
>
> **The one number a reader should carry away with a caveat attached.** Island 74 lands with
> **zero margin** — its anchor's capacity equals its demand to nine decimal places, because
> the load law sized the shaft and the seat law sizes the seat to match the shaft. 46 of the
> 129 recovered records are in that state, and it is a property of the engine's sizing law,
> not of this rule (§4.6, §6.5).
>
> **Evidence labels.** **MEASURED** — a number a run produced. **DERIVED** — closed form
> from MEASURED or registered inputs. **INFERRED** — reasoning from a recorded fact, not
> itself measured. **ASSUMED** — a stated placeholder for a fact that does not exist yet.
>
> **Glossary, first use.** *Stub* — the short support member the engine makes at each
> contact point on the model; it starts 0.3 mm inside the model and points outward.
> *Island* — a patch of model surface that has no support under it. *Carried area* — the
> area of model a member holds up, in mm²; it sets the member's radius. *Scar* — the mark a
> support leaves on the model when you break it off. *Seat* — the end of a member that rests
> on something. *Foot* — a seat on the model rather than on the build plate. *Crevice* — the
> gap of air a landed strut crosses. *Re-plan* — one pass of the coverage-repair controller;
> the same stub can appear once per re-plan, so 208 records cover 77 distinct stub shapes.
> *Grow-to-attest* — the step that widens a tip radius until the engine can prove the tip
> touches the model. *PR-2 mask* — the retired rule that exempted a length of a member's axis
> from its clearance test. *OD-8* — the standing order that a change which moves an output
> digest must stop and report the movement. *FU-87* — the ruling that a routed model-anchored
> strut must be at least 5 mm long. *E-A, E-B, E-C, E-D, E-E* — open escalations, each named
> where it is used. *Anchor* — the buried part of a seat: the end ball plus the depth it is
> driven to. *Truncation* — R4's rule that a seat stops at the model's opposite face.

---

## §0. DECISION MEMO FOR CRAIG

### The one-line version

The engine throws away every support stub that runs into the model. Your instruction was to
stop throwing them away and stand them on what they hit. Your four rulings of 2026-08-22 then
told the grading to stop sizing every landed foot as if it were a standard 0.3 mm foot, and to
stop condemning a seat for being deep when it can simply be cut off at the far face. On those
rulings the rule recovers **129 of the 208 records — 467.07 mm² — and island 74 lands.**

### The recovery table, up front, with island 74 first

**ISLAND 74 — RECOVERED, WITH ZERO MARGIN.** At the production plan its whole 13.019 mm² rests
on one condemned stub, terminal 162. That stub strikes the model 1.040 mm out from its contact,
in material **0.341 mm thick measured perpendicular to the face it enters**. Under R1 the foot
it needs is not a 0.3 mm-deep foot. Its load sizes the shaft at `r0 = 0.5877 mm`, and the depth
that makes the seat exactly as strong as that shaft is
`ball_seat_factor(1.4) × 0.5877 = 0.2469 mm` — the engine's own PR-1 clause (c), evaluated at
the registered foot ball factor. **0.2469 mm fits inside 0.3414 mm**, so R4's truncation does
not even bind, the seat never reaches the opposite face, and no ball can come out of it. The
seat's minimal separating cut is `0.5877 mm` and its capacity is
`12 × π × 0.5877² =` **13.018786 mm²**, against a demand of **13.018786 mm²**.

**Read that last line twice.** The capacity-to-demand ratio is **0.999999999** — it passes
under the same relative tolerance the area law itself is compared under, and it has **no
margin at all**. That is not a coincidence and not a fudge: the load law sizes the shaft to
exactly carry the load, and the seat law sizes the seat to exactly match the shaft. **46 of the
129 recovered records, and 7 of the 9 at the production plan, sit at exactly 1.000.** Every one
of them is a foot whose radius was not clamped up to `MIN_SHOOT_R_MM`. This is a property of
the engine's own sizing law, not of this rule — but this rule is the first thing that makes it
visible, and §6.5 discloses it rather than burying it.

**The second thing about island 74 you should see.** Its foot ball is 1.645 mm across and the
wall is 0.341 mm thick, so at the 42° the stub strikes at, the *ball* reaches 0.463 mm below
the entry face — past the opposite face. Under R4 it is truncated there. What is left is a plug
that spans the wall. **The area law prices the joint and prices it as sufficient. Nothing in
the engine prices the wall around the plug.** That is escalation **E-D** (§4.7), it is E-A
wearing a new hat, and it is the one place where this document's arithmetic stops.

208 stub records were measured on the Rotting Warhorse across 7 re-plans (**79** distinct stub
shapes — `10b0bd8` said 77; its shape key read a field its own grader never set and silently
degraded to counting distinct carried areas, of which there are exactly 77). "Production plan"
is the state the run settles into — 18 condemned stubs, and the plan that would actually be
printed.

| what the stub does | records | shapes | carried area | production plan | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| **Strikes, and the load-sized anchor carries** | **129** (62.0 %) | 57 | **467.07 mm²** | **9 stubs, 64.65 mm²** | **the rule recovers it** |
| Strikes, and the anchor cannot carry there | 25 (12.0 %) | 6 | 182.24 mm² | 3 stubs, 23.91 mm² | must re-aim (§5) |
| Its own tip never reaches the model | 52 (25.0 %) | 15 | 107.50 mm² | 6 stubs, 13.05 mm² | a different defect (§7) |
| Its own seat is truncated; nothing beyond it | 2 (1.0 %) | 1 | 1.31 mm² | **0 stubs** | legal outright under R4 (§8) |
| **total** | **208** | **79** | **758.12 mm²** | **18, 101.61 mm²** | |

**Against the table you were shown at `10b0bd8`.** That table was 80 records and 279.18 mm².
**All 80 still recover — not one is lost — and 49 more join them**, 187.89 mm². They arrive
from four places: 34 were refused by the flat 0.3 mm wall clause R1 abolishes (99.88 mm²), 11
by the foot-ball breach R4 makes impossible (85.21 mm²), 2 by both (2.74 mm²), and 2 by the
crevice-width cutoff R3 abolishes (0.07 mm²). Two more come out of the deep-seat class R4
dissolves. **Every one of your four rulings is an admission. None of them can newly refuse
anything, on this model or any other** — which is exactly what makes the staging in §10 cheap
(§13 item 4).

**The class that used to decide the answer is gone.** At `10b0bd8` one clause — "the struck
wall must be at least `DEFAULT_CONTACT_DEPTH_MM` thick" — refused 59 of 152 candidates and
278.28 mm², and it was machinery that existed nowhere in the engine, built here, reusing an
along-axis embed depth as a perpendicular wall minimum. **R1 deletes that clause outright.**
What replaces it is not a threshold at all: it is the load-sized anchor's own capacity, graded
by `contact::area_sufficient` on `ball_cut_radius_mm` — the engine's existing predicate on the
engine's existing formula. The wall requirement is now **load-dependent, as it should be**: a
**0.063 mm** wall carries a 0.818 mm² load and is admitted; a **0.122 mm** wall is refused for a
4.306 mm² load. No constant decides that. The physics does.

**The prize is not only this model.** On `zombie-3-a`, **52 of 92** coverage-repair trials
(56 %) fail in this same class — the stub is condemned before any arm is even proposed, and
that model records **zero** clearance refusals in the brace stage. MEASURED
(`.git/mask-honesty-evidence/zombie3a-clearance-evidence.txt`). We have not run the probe on
`zombie-3-a`, so we cannot say how many of its 52 pass the anchor test. **INFERRED**: the class
matches; the split does not.

---

## §0.1 THE RECORD OF RULINGS — all four decided, nothing pending

These four are the entire content of this amendment. They are recorded here verbatim, with
what each one changed and what it cost or bought, so that no later reader has to reconstruct
them from a transcript.

---

**R1 — LOAD-SIZED ANCHORS. RULED.**

> *"If these struts are all under 2.5mm inside crevices then they're likely extremely thin and
> don't need a full 0.3mm to land in. They won't likely be removed or even really very visible
> so I think that as long as the physics says they'll be supported they can have much smaller
> anchors."*

**What it changed.** The foot's radius **and its embed depth** both derive from the carried
load. The radius already did — `FrozenTipLoadPolicy` grows `r0` until the tip attests, and the
foot is `max(r0, MIN_SHOOT_R_MM)`. The depth did not: `10b0bd8` used
`DEFAULT_CONTACT_DEPTH_MM = 0.3 mm` for every foot and then demanded 0.3 mm of perpendicular
wall to put it in. **That clause is deleted.** The depth is now
`h_design = ball_seat_factor(BALL_FACTOR_FOOT) × r_foot = 0.4202041 × r_foot`, which runs
**0.147 – 0.247 mm** over the recovered population — the depth at which the seat's own minimal
separating cut is exactly the shaft it serves. The wall test becomes *"does this wall
accommodate the anchor THIS load requires"*, graded by `contact::area_sufficient` on
`ball_cut_radius_mm`. §2.3(a), §2.8.

**What it bought.** +34 records / 99.88 mm² directly, and it is what lets island 74 land. It
also removes escalation item 14 of `10b0bd8` — the "new machinery" the reviewer objected to —
because there is no longer a wall threshold to justify. **And it shrank every scar**: the
largest scar on the whole recovered population is now **1.478 mm²**, against 2.448 mm² before;
**0 of 129** recovered records carry a scar above the ratified `A_ABS_MM2 = 2.0 mm²`, against 12
before (§6.3). Escalation item 2 of `10b0bd8` is dissolved, not deferred.

**What it cost.** One disclosure: 46 of 129 recovered feet have capacity exactly equal to
demand (§6.5). And it opens E-D (§4.7).

---

**R2 — PERMANENCE. ACCEPTED AS DISCLOSED.**

No gate. The per-strut disclosure record of §6.2 stands as written, and the numbers move with
the population: at the production plan it is now **9 struts, 8.318 mm³ of resin (0.0083 mL), 18
new scars totalling 15.249 mm², none above 2.0 mm², reach `PermanentUnlessMeasured` on all 9**.
Nothing in the engine models a cutting tool, so "permanent" stays the honest label and
escalation E-B (measure a flush cutter) stays open. **The aggregate scar area is unchanged
while the strut count nearly doubled** — the load-sized anchors are that much smaller.

---

**R3 — SUB-0.3 mm CREVICES. RULED.**

> *"If resin seals anyways then the strut adds no extra resin. So this is moot I think."*

**What it changed.** Crevice width never refuses. Test (b) — `crevice ≥ MIN_ILLEGAL_LEN_MM` —
becomes a disclosure field. Each landed strut still reports its crevice; none is refused for it.
**The width-refusal class is gone entirely.**

**What it bought.** 2 records / 0.065 mm² directly — the two tangential grazes that emerge for
46 µm. The larger effect is that the 18 recovered records sitting below your 0.30 mm field line
are now **landed and disclosed** rather than left as an open question: 69.54 mm², **14.9 % of
recovery** (it was 24.8 % of a smaller recovery), and **1 of 9 at the production plan** — t157,
crevice 0.174 mm, 9.974 mm², **15.4 % of the production prize** (it was 27.2 %). §6.4 lists all
seven distinct widths.

---

**R4 — TRUNCATION. RULED. This replaces the whole E-C remove-versus-build question.**

> *"A seat should be allowed to go as deep into the model as it likes, but it should not come
> out the other side of the model so if the seat wants to go deeper than the thickness of the
> model, it should get truncated at the opposite face — it should not create a scar on the
> backside because that provides no additional support so there's no need for it to extend
> beyond the model anyway."*

**What it changed.** SPEC-EMBED-POLICY's no-depth-cap sentence is **rewritten, not withdrawn**
(§8.4). Depth stays unlimited; the seat is **truncated at the opposite face**. A seat or foot,
**ball included**, never exits. There is no backside scar. **Punch-through is impossible BY
CONSTRUCTION**, and the construction is computable from the penetration intervals already
measured — the interval *is* the model's extent along the seat's own line, so truncating at its
far end is a clamp, not a test.

**What it bought.** The foot-ball breach *refusal* is retired: +11 records / 85.21 mm², island
74 among them. The deep-seat class is dissolved: its 4 records become 2 legal outright and 2
that land. **And E-C's constructed regression stops existing.** E-C said a support could
perforate a thin shell and stay legal; under R4 the seat stops at the shell's far face, so the
perforation is not reachable. The Rotting Warhorse's one real instance — `t225`, a 0.139 mm
shell — needs only **0.063 mm** of that shell under R1, so it never gets near the far face at
all, and its truncated seat carries its load with the area law's own verdict (§8.3).

**What it cost.** E-D (§4.7): 65 of 129 recovered records have their load-designed ball clipped
by the opposite face, and the clipped anchor is a plug spanning the wall. The area law prices
the joint. Nothing prices the wall around the plug.

---

### What you are NOT being asked to authorize

* **No constant moves and no constant is minted.** `ball_seat_factor` is an existing DERIVED
  function of an existing registered constant, evaluated at another existing registered
  constant. `ball_breach_eps_mm` is no longer read at all on this path.
* **The load-path question stays open, and R4 makes it larger, which is said out loud.** When a
  strut stands on the model, the model carries the load onward. Whether that is safe is
  escalation E-A, opened 2026-08-21 by the model-seated-arm work that already landed. This rule
  adds 129 instances to it, and E-D is the sharpest of them.
* **Nothing is wired to the removal apparatus.** The six `GRIP_*` constants stay dead.

### What it costs to build

The change is in initial contact minting, so **every model in the corpus is affected**. The
staged plan (§10) still puts a **measure-only stage first**: the landing is computed and
recorded for every condemned stub on every corpus model, and nothing moves. You see the real
recovery table for the whole corpus before one triangle of geometry changes.

**Two staging facts that follow from the rulings.**

1. **All three behavioural rulings are admissions.** R1 relaxes a threshold to a physics test,
   R3 removes a refusal, R4 converts a refusal into a clamp. **None of them can newly refuse a
   stub on any model.** The only part of this work that can newly refuse anything is the rim
   sweep (`rays_all`), which is stage 1b, is separately gated, and is unchanged by the rulings.
2. **Implementation starts after the wire-clearance lane finishes.** That lane owns
   `crates/plinth-support/` right now — `brace.rs`, `cco.rs`, `contact.rs`, `designed_embed.rs`,
   `initial.rs`, `rewire.rs`, `runner.rs` are all live in its tree — and this rule's stage 2 and
   stage 3 both land in files it is editing. §10.0 states the sequence.

---

## §1. WHAT THE MEASUREMENT SAYS, AND WHAT IT REFUTES

### §1.1 The evidence

Two probes, both print-only, both reverted, both on the same input.

* `.git/rw-embed-split-evidence/` — the original `rw-embed-split-2026-08-22` probe. 208
  condemned stub records over 7 re-plans. It recorded, per penetration interval, the incidence
  and face at the interval's **exit**.
* `.git/land-at-strike-p1-evidence/` — **probe P1**, this repair's evidence. Same input
  (`minis-to-process/Rotting Warhorse.stl`, sha256 `165118a8…`, verified before the run), same
  stopping point, built in a throwaway detached worktree at `f50a45b` and removed after.
  Provenance on both axes: the binary was built after the probe source was edited, and the
  marker string `land-at-strike-p1-2026-08-22` is present in the binary.

**Every field the two probes share is byte-identical across both runs** — all 1,708 contact
rows and all 1,926 interval rows. P1 is the same measurement with more of it recorded.

P1 adds, per interval: the incidence, face id and **outward face normal at the interval's
ENTRY**; the proposed landed foot's signed distance to the surface; and the engine's own
`sphere_breach` outputs at that foot — `breached`, `nearest_wall_thickness_mm`, `poke_depth_mm`
— **plumbed verbatim from the same kernel call `anchor.rs:1610-1619` makes**, with the
registered ball factor, the registered epsilon and the same cone. Nothing in the probe
recomputes a kernel quantity. Per stub it also records the contact tip's own distance, face,
normal and ball. All native work is batched: four whole-batch projections and two whole-batch
breach calls per invocation.

**MEASURED, and still load-bearing:** all 1,926 interval rows carry `ray = -1`. The sweep is
**centreline only**. `initial.rs:1359` passes `n_offset_rays = 8` but `rays_all = false`, and
`queries.rs:90` reads `offset_count = usize::from(rays_all && axis_length >= 1e-9) *
n_offset_rays`, so the 8 is inert. The stub is a tapered capsule and only its centreline is
tested.

### §1.2 What the #52 derivation assumed, and what the measurement did to it

`model-seated-arms-2026-08-21.md` §2.4 priced the prize by treating each condemned stub's deep
run as an **anchored** seat — a legal deep plug — and grading it by scar area. The measurement
refutes that premise:

| §2.4 assumed | MEASURED |
| --- | --- |
| the deep run starts at the seat (anchored) | **204 of 208 do not.** Only 4 records have their condemning run starting at the seat |
| `r_x ≈ 0.30 mm` | 0.0867 – 0.5877 mm; island 74's production stub is **0.5877** |
| island 74 becomes routable by scar arithmetic | it becomes routable, but **not by scar arithmetic** — the scar rules refuse nothing anywhere in this population. It lands because R1 sizes its anchor from its load and R4 stops any seat exiting (§0, §4.6) |

**The gap between the seat and the strike is air.** The stub leaves the model cleanly, crosses
a gap, and hits something else. That is not a deep seat and it is not a scar problem. It is a
stub that has arrived somewhere. That reading is what this design keeps.

### §1.3 Craig's reading, restated as the design

> *"How can it be that the support stub traverses through the model? If it strikes the
> model in a line to where it was supposed to be, then just land it there? I suppose the
> place where it strikes the model on its path might not be an appropriate anchor spot? In
> that case we can re-orient, but if it's a valid spot to be the foot of the support we can
> just land it there?"*

Two steps, ordered:

> **LAND-AT-STRIKE (primary).** A stub whose outward shaft strikes the model **terminates at
> the first strike**, with its foot seated on the struck surface **at the depth its own load
> requires, truncated at the opposite face**, when all the tests of §2.3 pass. The result is a
> support that runs model to model.
>
> **Fallback.** When the strike point cannot carry the load the stub re-aims, by the
> owner-ratified cone search in `docs/engineering-journal/specs/SPEC-P8-RELOCATE.md` §3.1
> **R1**.

**A naming collision to keep straight.** Craig's rulings are numbered R1–R4 in this document
(§0.1). `SPEC-P8-RELOCATE`'s ratified ladder is also called R1 → R2 → R3, and its **R1 is the
re-aim**. Where this document says "the re-aim", it means the spec's R1; where it says "R1" on
its own it means Craig's load-sized-anchor ruling. Every occurrence below is written so the
sense is unambiguous from the sentence, and the fallback is always called *the re-aim*.

**A note on naming, and why the first version's names were withdrawn.** That version called
these "RULE 1" and "RULE 2". `SPEC-P8-RELOCATE` already has a ratified ladder called
R1 → R2 → R3, in which **R1 *is* the re-aim** and R2 is a contact re-seat this design never
mentions. Anyone tracking E-C across the two documents would have mis-mapped them. The rules
here are therefore named, not numbered, and the fallback is referred to by its spec name.

Landing is cheaper than re-aiming in every sense where it applies: it preserves the contact
point exactly, it preserves the contact direction exactly, it shortens the member, it needs no
candidate search, and it needs no re-route. **Under the rulings it applies to 62 % of the
population and 9 of the 18 production stubs**, which puts the two lanes back the way the first
version had them — landing is the larger prize and the re-aim is the residue (§5). That
reversal is a direct consequence of R1 and R4 and of nothing else.

---

## §2. THE RULE FORMALIZED — when does a strike become a foot?

### §2.1 The geometry, named once

The engine makes a stub at `initial.rs:644-673`:

```rust
let p0 = position - unit * depth;   // depth = DEFAULT_CONTACT_DEPTH_MM = 0.3 mm
let p1 = position + unit * span;    // span  = STUB_LENGTH_MM = 2.5 mm
```

`p0` is the **contact end** — embedded 0.3 mm inside the model. `p1` is the **free end**, out
in the air, where routing continues. Let `s` run along the axis from `s = 0` at `p0` to
`s = L = 2.8` at `p1`. Write `δ = 0.3 mm` for the designed embed depth. The nominal model
surface is at `s = δ`.

Let `s*` be the **first strike**: the smallest `s > δ` at which the shaft re-enters solid.
Then:

* **crevice** `= s* − δ` — the measured air the stub crosses.
* **radii.** `r_tip = r0` (load-law sized, §2.6) and `r_foot = max(r0, MIN_SHOOT_R_MM)` — the
  identical rule `initial.rs:655-657` already applies, over the shorter span.
* **the foot's advance, under R1 and R4.** The foot does **not** advance a fixed `δ` past the
  strike. It advances `t = h_eff / cosang_entry`, where `h_eff` is the load-sized, truncated
  perpendicular embed depth of §2.8. **MEASURED over the recovered population:** `t` runs
  0.148 – 1.503 mm, median 0.271 mm, and `t / chord` never exceeds **1.000** — R4's truncation
  in one number.
* **landed length** `L' = s* + t`. **MEASURED:** 0.574 – 2.800 mm, median 1.410 mm. The frustum
  is `r(s) = r_tip + (r_foot − r_tip)·s/L'`, growing from tip to foot.
* everything beyond `s = s* + t` is **cut away and does not exist**. Crossings out there
  vanish with it.

### §2.2 Print-minimum moot strikes are ridden through, not landed on

A crossing whose chord is below `PRINT_FLOOR_CHORD_MM = 0.08 mm` is **MOOT** by the
owner-ratified 2026-07-24 amendment: *"chords < 0.08 mm are 2.6-4.1 px foliage knife-edges that
cannot print as coherent material."* Material that cannot print cannot be stood on. So a moot
crossing is **not** a strike; the shaft rides through it, and it is still counted and reported
in the moot bucket the amendment requires.

**The mirror question, asked here because R3 (§0.1) answered it.** The project has that
minimum for solid and none for air. At 2.6-4.1 px per 0.08 mm, a 0.116 mm crevice is four to
six pixels of air between two curing walls. This design uses the solid minimum and **discloses**
that it has no equivalent for the void (§6.4).

### §2.3 The tests, after the rulings

A strike becomes a foot when **all of (a), (c), (e) and (f)** hold. **(b) is now a disclosure
and never refuses** (R3). **(d) is not a separate test any more**: it *is* (a), because R1 made
the anchor's own capacity the thing that decides (§2.5).

**Every test is an existing predicate or an existing formula. That was not true at `10b0bd8`,
and R1 is what made it true.** `10b0bd8`'s test (a)(i) compared a perpendicular wall thickness
to `DEFAULT_CONTACT_DEPTH_MM`. Nothing in the engine or in SPEC-EMBED-POLICY does that; the
reviewer's substantive required change was that the document say so. **R1 deletes the clause
instead.** The wall no longer faces a threshold — it faces the anchor the load asks for, and
that comparison is `contact::area_sufficient` on `contact::ball_cut_radius_mm`, both of which
the engine already has and already uses.

| # | test | statement | how it is measured |
| --- | --- | --- | --- |
| **(a)** | **the load-sized, truncated anchor carries the load** | one clause. Size the anchor from the load (`r_foot`, `h_design`), truncate it at the opposite face (`h_eff = min(h_design, W)`), and ask the area law whether the resulting minimal separating cut carries `carried_mm2` | `W = chord × cosang_entry` with `cosang_entry` **MEASURED by P1** at the interval's entry face; `h_design = ball_seat_factor(BALL_FACTOR_FOOT) × r_foot` (`contact.rs:186`, DERIVED); `a_eff = ball_cut_radius_mm(r_foot, h_eff, 1.4)` (`contact.rs:197`); the verdict is `contact::area_sufficient(carried_mm2, a_eff, policy)` (`contact.rs:485`). **§2.8 derives it.** |
| **(b)** | **the crevice** | **DISCLOSED, NEVER REFUSED (R3).** The crevice width is recorded per strut | measured directly on the interval sweep — see §2.4 |
| **(c)** | **scar pricing at both ends** | SPEC-EMBED-POLICY rules 1-3 at the foot **and** at the contact tip, at the R1 depths | `transit_measurement::calculate_scar_diagnostic` (`transit_measurement.rs:301-368`). Graded here on the MEASURED incidence and on the anchor's own cut disc, not on a worst case. Under R4 there is no backside patch to grade — the seat does not reach one |
| **(d)** | ~~the foot interface carries the island's load~~ | **FOLDED INTO (a).** It was `area_sufficient` on `ball_cut_radius_mm(r_foot, δ, 1.4)` with `δ` fixed at 0.3 mm; R1 makes the depth a variable and the same predicate becomes the whole of (a) | see §2.5 for the half of the load path that still does **not** exist |
| **(e)** | **the strut's own narrowest section carries the load** | the area law at the tip | `FrozenTipLoadPolicy` (`perception.rs:636-644`) already sized `r0` from `carried_mm2` at ratio 12. See §2.6 |
| **(f)** | **build order** | the foot must be **below** the contact in Z | the same sense `anchor.rs:1063` already applies to a model-anchor seat, and `cco.rs:4967` to a routing junction — see §3.6 |

**What the tests now refuse, MEASURED over the 154 landing candidates:**

```
  (a) load-sized truncated anchor capacity   refuses  23 / 154   175.662 mm2
  (b) crevice width          [R3]            refuses   0 / 154     0.000 mm2
  (c) scar rules, either end                 refuses   0 / 154     0.000 mm2
  (e) tip section area law                   refuses   0 / 154     0.000 mm2
  (f) foot above its own contact             refuses   2 / 154     6.576 mm2

  first-and-only failure:  a:anchor-capacity  23  175.662 mm2
                           f:foot-above        2    6.576 mm2
```

**Say it plainly, as the reviewer required of the previous version: (c) and (e) refuse nothing
on this model, and (b) can no longer refuse anything anywhere.** The rule is decided by (a),
with two hangers caught by (f). If the corpus numbers move at stage 1, (a) is where to look.

**The wall requirement is load-dependent, and that is the whole of R1.** There is no threshold
to quote. MEASURED, at the two ends of the overlap:

```
  narrowest wall that still carries : W = 0.0630 mm carrying 0.8178 mm2   (b0 t15,  r0 0.1473)
  widest wall that is still refused : W = 0.1220 mm for     4.3058 mm2   (b0 t14,  r0 0.3380)
```

A wall half as thick is admitted for a load five times smaller. `10b0bd8`'s flat 0.3 mm would
have refused both.

**What happened to the foot-ball breach check.** `10b0bd8` ran
`NativeSurface::sphere_breach` at the foot endpoint with `ball_factor_foot = 1.4`, which is what
`anchor.rs:1607-1630` does for a model anchor, and refused 13 of 152 records including island
74. **R4 retires that refusal**, and the reason is geometric, not a relaxation:

* `anchor.rs:1606-1608` centres the foot ball on `seat.snapped_point_mm` — **the surface point
  itself**. A ball centred on the surface is driven `R_b` deep, so it demands `R_b` of wall in
  every direction. For island 74 that is 0.823 mm of wall, and it had 0.341 mm. Of course it
  breached.
* Under R1 the foot is no longer driven centre-deep. It is driven to `h_design`, which for
  island 74 is 0.247 mm, and the ball's centre ends up **0.360 mm outside the model, in the
  crevice**. The ball is a bulb hanging in the air with a cap of it buried. That is the
  parameterization `contact.rs:186` already documents for a stub tip — *"a ball of radius
  `f*r_tip` whose south pole sits at `p0`"* — applied to a foot.
* Under R4 the seat is truncated at the opposite face, so **`breached` cannot be true**. There
  is nothing left for `sphere_breach` to detect. The predicate is not weakened; it is
  unreachable. `ball_breach_eps_mm` is no longer read on this path.

**What that leaves, and it is not nothing.** The buried cap's *lateral* reach at oblique
incidence is `h_eff + R_b·(1 − cosang_entry)`, which for 65 of the 129 recovered records exceeds
`W`. Those anchors are truncated into plugs that span the wall. **§4.7 is that disclosure and it
is escalation E-D.**

### §2.4 Test (b): what happened to `emerges`, and what replaced it

**This subsection is history now. It is kept because the reasoning in it is still the reason
the wider sweep is required, and because a reader of `aae1559` needs to know why the argument
moved.**

*Superseded.* `contact::emerges` (`contact.rs:169-171`) is
`total_mask_mm <= seated_length_mm - policy.min_free_span_mm`. The first version showed that
this refuses **every** landed strut, because two PR-2 masks of about 1.3 mm each sit on a member
whose median length is 1.4 mm — and understated it: the mask is
`(h + r_max + standoff + resolution)/|cos|`, so 1.3 mm is the best case at perpendicular
incidence, and at this population's median incidence each mask is nearer 2.4 mm. It then
proposed replacing the proxy with a direct measurement of the crevice at the same registered
threshold.

*Why it is superseded.* **The PR-2 mask is being retired.** `1c646aa` landed
`crates/plinth-support/src/seat_zone.rs`, whose rule is *"EXEMPT A TARGET, NEVER A LENGTH. Test
the FULL axis (`inset0 = inset1 = 0`) against the whole surface EXCEPT the seat's own contact
zone."* With `inset0 = inset1 = 0`, `emerges(0, seated_length, policy)` reduces to
`0 ≤ seated_length − 0.05`, which every landed strut passes — the shortest is 0.574 mm. The
defect §2.4 was written to fix stops existing at the final pass. **Landing a second
family-specific clearance predicate for no behavioural gain is exactly what
`retired_formula_guard.rs` exists to prevent**, and exactly what §10.3 proposes to extend that
guard to ban. So the predicate change is withdrawn.

*What survives, unchanged and strengthened.* **The crevice must be measured on the whole
capsule, not just its centreline.** A capsule of radius 0.35 mm whose centreline crosses
0.155 mm of air can still have its flank inside the model. The measurement to make is the
existing sweep with `rays_all` flipped from `false` to `true` at `initial.rs:1359`, which turns
the already-passed `n_offset_rays = 8` from inert to live.

The first version argued this as "a strict tightening, more rays can only find more material."
That is true and it is the weaker argument. **The stronger one is alignment, and it should
replace it:** stubs are the *only* member family swept centreline-only. `transit_measurement.rs`
splits the stub path (`0, false`) from the non-stub path (`8, true`) at `:3522/:3525` versus
`:3292`; `runner.rs:7187` does the same; `designed_embed.rs:98` and `:452` already run with all
rays. And `contact.rs:437-442` **already assumes the nine-ray witness** when it credits
`π·r_eff²` — its own comment says the area is *"WITNESSED … by the PR-5 endpoint witness: 8
capsule rim rays plus the centreline"*. The flip aligns the stub path with what the rest of the
engine does and with what test (d) already relies on.

**But it is not free, it is not neutral, and it does not belong in a measure-only stage.**
See §10.2 stage 1b and §12 item 7. **Without it, this rule exempts a region nothing has looked
at, and that is the one thing a reviewer should refuse** (§9.3).

### §2.5 Test (d): what exists, what does not, and whose problem it is

Craig's ruling is that the test is a **closed load path per physics**. That splits in two.

**The half that exists.** Does the foot interface carry the load it is handed? That is the area
law, and `contact::area_sufficient` (`contact.rs:485`) is exactly it. It is already called
inside `contact::admit` for every other contact. **At `10b0bd8` it refused 0 of 152, because
the depth was pinned at 0.3 mm and 0.3 mm is generous for every radius in this population.
Under R1 the depth is no longer generous — it is exactly what the load asks for — so the same
predicate now decides the whole rule and refuses 23 of 154.** That is the same code doing more
work, not new code. §2.8 is its derivation.

**The half that does not exist.** Does the *struck feature* carry that load onward to its own
supports? Nothing tests this. `propagate_carried` (`cco.rs:4703-4726`) walks parent-ward and
stops; a seat has no parent, so the accounting ends at the model face and no one charges that
face. This is **escalation E-A**, raised 2026-08-21, with tasks **#64** (narrow-neck
amplification) and **#65** (no connectivity requirement in the reach probe) governing the
receiving side.

**And it is the same gap test (f) proxies for.** §2.3 lists (d) and (f) as separate tests, which
reads as though (f) covers ordering and (d) covers load. It does not. (f) is a two-endpoint
proxy for the same missing connectivity query that (d)'s open half needs. **They are one gap,
and this document counts it once** (§3.6).

**The honest position.** E-A was opened by the model-seated-arm family that landed in `4c74f74`,
not by this design. Every model anchor committed today has the same open question. This rule
**adds instances** to a live exposure; it does not create one. §10's stage 1 counts the instances
before any geometry moves, so the size of the addition is a measured number when Craig rules
on E-A.

### §2.6 Test (e): what "by construction" does and does not cover

MEASURED, from the probe: the engine's implied area ratio, recovered from the data rather than
retyped, is **12.000000** — `tensile_pa / (pressure_pa · SF) = 0.288e6 / (3000 · 8)`. Tip radii
come out of the same law, so the test holds to rounding: **it refuses 0 of 154.** (An earlier
pass of this re-grade appeared to refuse 44; that was an absolute epsilon reading the probe's
nine-decimal rounding as a physics failure. The comparison is relative. Recorded so the number
is not rediscovered as a finding.)

The narrowest section of a landed strut is at the contact tip, because the frustum grows toward
the foot (§2.1). That section is unchanged by landing. And landing **shortens** the member —
median 2.8 mm to 1.4 mm — which can only reduce buckling and bending.

**What that argument does not cover, stated because the first version overstated it.** A stub is
a cantilever seated at one end. A landed strut is a member **seated at both ends, at different
heights, in the same resin**. The area law is a section law and it is the same law either way,
so the tip section is still fine. But a doubly-seated member is over-constrained: peel force
resolves into it as bending and shear at the seats, not only as tension. The field observation
the project has on arm loading (failure near `supported_area/arm_section ≈ 25`, design at 12) is
a *tensile* observation on a member seated at one end. **Nothing measured speaks to a 0.116 mm
doubly-seated bridge under tilt-peel.** It is probably fine — the member is tiny and short — but
"passes by construction, and this is a derivation, not an observation" was too strong and is
withdrawn. Test (e) passes; the load case is not the one the evidence covers.

Registry note: `X_ALLOWED = 12` is `Derived`, but it is **not** the value the sizing code reads —
the code reads `FrozenTipLoadPolicy` and the registry link is a one-sided test assertion. This
design changes neither and records the seam because it bears on how test (e) is argued.

### §2.7 What happens to the ARM requirement — and the two things that interact with it

**A landed strut needs no arm, and none is proposed.** Today the coverage stage takes a stub's
free end `p1` and runs a candidate tournament for an arm down to the build plate or to a model
anchor. Under this rule the free end is *already* a model seat, so the terminal's route is
complete at the stub. Consequences:

* **No candidate tournament, no clearance rows, no winner election** for these terminals.
* **The island's coverage is carried through the model.** That is the whole point, and it is
  the part E-A governs (§2.5).
* **The E-E accounting seam gets *less* exposed.** E-E is the seam where a terminal's sealed
  transcript says `Routed` while its ledger says `Quarantined`. This rule moves terminals out
  of `Quarantined`, so it shrinks the population that can hit that seam.

**Two claims from the first version are corrected here.**

**(i) The FU-87 minimum-arm screen refuses 100 % of this design's output, and that is an
owner-visible ruling, not a definition.** `STRUT_MODEL_ANCHOR_MIN_LEN_MM = 5.0 mm` is applied at
`anchor.rs:1065` as `AnchorSeatRefusalReason::TooShort`. A landed strut is **0.72 – 2.41 mm**
long (MEASURED). Every one would be refused. The first version answered this by asserting that a
landed strut "is not an arm and must not enter that screen".

That is probably the right answer — a 5 mm minimum on a *routed* member that has to be reachable
and cuttable says nothing about a 1.4 mm bridge that is permanent by admission. But note where
the screen sits: **`anchor.rs:1063` and `:1065` are consecutive clauses of the same seat
qualification.** `:1063` is `AboveContact`, which is test (f)'s own idiom. So this design
proposes to adopt one clause of that chain and decline the next, for the same member. **That is a
ruling and it is now escalation §12 item 9**, with the honest resolution stated there: either the
screen gains an explicit exemption for members with no routed arm, with the reason recorded, or
FU-87 is re-derived to say what it is actually protecting. It must not be settled by declaring
the member out of scope.

**(ii) "`propagate_carried` terminates naturally" is true of that one function and false of the
code around it.** `cco.rs:4551` hard-errors with *"stub … has no parent member after plate
descent was applied"*; `cco.rs:4004` does the same on the model-anchor path; `cco.rs:282` makes
*"detached-subtree root has no parent"* a refusal reason; `cco.rs:4831` errors when pricing a
plate-descent stub with no parent; `initial.rs:1097` collects an ungradable committed member as
*"an orphan or out-of-order kernel interval"* into a total refusal. **A parentless terminal is a
third gap**, distinct from the family-keying check §10.2 stage 2 already names, and it is now
escalation §12 item 10.

### §2.8 THE LOAD → ANCHOR DERIVATION, in full, with no new constant

This is R1's whole content. It is written out because it is the one piece of machinery this
document adds, and because the previous version's equivalent piece was the thing the review
called out.

**What is already registered, and what is therefore not derived here.**

* The **foot radius is already load-designed.** `FrozenTipLoadPolicy::tip_radius_mm`
  (`perception.rs:636-644`) grows `r0` until the tip attests under the area law at the engine's
  implied ratio `X = tensile_pa / (pressure_pa × SF) = 0.288e6 / (3000 × 8) = 12`. MEASURED from
  the probe rather than retyped: **12.000000**. The foot is then
  `r_foot = max(r0, MIN_SHOOT_R_MM)`, the rule `initial.rs:655-657` already applies. So R1's
  first half — "the foot radius derives from the load" — is **already true in the engine** and
  nothing is proposed for it. The recorded `r_x` per record *is* the load-designed foot.
* The **ball is registered**: `R_b = BALL_FACTOR_FOOT × r_foot`, `BALL_FACTOR_FOOT = 1.4`.
* The **anchor's engaged area is registered**: `contact::ball_cut_radius_mm(r, h, f)` returns
  `sqrt(2·f·r·h − h²)`, documented at `contact.rs:191-196` as *"the ACTUAL minimal separating
  cut at the seat plane, which is what the area-ratio law must be evaluated on"*, for a ball of
  radius `f·r` **whose south pole sits at the member end** and a seat plane at depth `h`.
* The **verdict predicate is registered**: `contact::area_sufficient(carried, r_eff, policy)`
  (`contact.rs:485`) is `!violates_safety_factor`, i.e. `carried ≤ X·π·r_eff²`.

**The open half was the depth.** Given a load, how deep must the anchor go? The engine already
answers it, for the stub tip, and the answer has a name.

> `contact::ball_seat_factor` (`contact.rs:186`), **DERIVED, never hardcoded**:
> *"Requiring that cut disc to be no narrower than the shaft above it (`a >= r_tip`) gives
> `h >= r_tip * (f - sqrt(f^2 - 1))`."*

That is PR-1 clause (c). It is pinned by unit test at `f = BALL_FACTOR_STUB = 1.5`, where it is
`0.3819660112501051`. Evaluated at the foot's own registered factor `f = BALL_FACTOR_FOOT = 1.4`
it is **`0.4202041028867288`**. So:

> **`h_design = ball_seat_factor(BALL_FACTOR_FOOT) × r_foot = 0.4202041 × r_foot`**

**Why that is the right depth and not an arbitrary one.** Two conditions must hold at the seat,
and one of them implies the other:

1. **Capacity.** `carried ≤ X·π·a²`, i.e. `a ≥ r_req` where `r_req = sqrt(carried / (X·π))`.
   Solving `2·R_b·h − h² ≥ r_req²` gives `h ≥ R_b − sqrt(R_b² − r_req²)`.
2. **Neck consistency (PR-1 clause (c)).** `a ≥ r_foot`, giving `h ≥ ball_seat_factor(f)·r_foot`.

Grow-to-attest makes `r_req ≤ r0 ≤ r_foot`, so condition 2 always implies condition 1, and
`h_design` is the depth the engine would build. **When `h_design` fits, the anchor's capacity is
`X·π·r_foot²` — exactly the shaft's own capacity, which is exactly what the load required.** No
margin is added and none is invented. That equality is why 46 of 129 recovered records show
capacity/demand = 1.000 (§6.5).

**R4 supplies the cap.** The model's extent along the seat's own line is the penetration
interval, and its perpendicular projection is `W = chord × cosang_entry`. R4 truncates there:

> **`h_eff = min(h_design, W)`**, and then
> **(a) passes iff `carried ≤ X·π·ball_cut_radius_mm(r_foot, h_eff, 1.4)²`.**

`h_eff ≤ W` is a clamp, not a test, so **the seat cannot reach the opposite face, cannot exit,
and cannot leave a backside scar. Punch-through is impossible by construction**, and the
construction reads one measured interval. When truncation binds, the seat is shallower than the
design wants and the area law says whether what is left still carries. MEASURED: truncation
binds on **40 of 154** candidates (189.233 mm²) and **17 of those 40 still carry** their load.

**The two depths, and which one this design changes.** `h` above is the *perpendicular* depth of
the seat below the entry face, because that is the plane `ball_cut_radius_mm`'s closed form cuts.
The member advances along its own axis, so its axial advance is `t = h / cosang_entry`, and
`h_eff ≤ W` is the same inequality as `t ≤ chord`. Both are recorded per strut (§6.2).

**Scope, stated so it is not assumed wider than it is.** R1's law is applied here **at the
foot**. It applies equally at the contact tip — the tip's own embed is
`DEFAULT_CONTACT_DEPTH_MM = 0.3 mm` regardless of what it carries, and
`ball_seat_factor(1.5) × r0` would be 0.019 – 0.225 mm — but **this design does not change the
tip embed**, because `p0 = position − unit·depth` is where every contact in the corpus starts,
and moving it moves the condemnation window (`initial.rs:1542` measures the remaining overlap
*from* the designed embed) on every model. That is a separate change with a separate blast
radius and it is §12 item 15. The one place the tip's own load-sized depth is computed in this
document is §8.3, where it settles E-C's instance, and there the verdict is the same either way.

**Is a new physical relation needed? No — with one named exception.** Every step above is an
existing registered constant, an existing DERIVED function, or an existing predicate. The
exception is **E-D** (§4.7): when truncation clips the ball, the anchor stops being a
half-space seat, and `ball_cut_radius_mm`'s closed form is a half-space formula. That gap is
disclosed, bracketed with two conservative alternatives, and escalated — **not papered over,
and not closed by minting a constant.**

---

## §3. THE SCAR BOUND — what it buys, and what it does not decide

### §3.1 The claim, and what R1 did to its premise

`10b0bd8` claimed a closed-form bound:

> ~~**DERIVED.** A foot seated exactly `δ = DEFAULT_CONTACT_DEPTH_MM = 0.3 mm` into the struck
> surface … satisfies SPEC-EMBED-POLICY rules 1, 2 and 3 at every incidence angle and at every
> radius in the registered range.~~

**That proof's premise is gone.** Its whole argument (§3.2) turned on `T` being *pinned* at
0.3 mm, which let both `min()` terms be bounded by constants. Under R1 the seat's axial run is
`t = h_eff / cosang_entry`, which is not a constant — it is bounded above only by the
interval's own chord, which R4 supplies. **So the closed-form claim is withdrawn as a proof and
replaced by two things, one measured and one owed.**

**MEASURED, over the 129 recovered records, at the R1 depths:**

```
  seat axial run t          min 0.1475  p25 0.2170  med 0.2709  p75 0.3412  max 1.5031 mm
  t / chord                 min 0.0778  p25 0.1768  med 0.3690  p75 0.8286  max 1.0000
  foot L_scar               min 0.5955  p25 0.8468  med 0.9125  p75 1.2029  max 1.8810 mm   (cap 2.500)
  tip  L_scar               min 0.1768  p25 0.3785  med 0.5371  p75 0.9571  max 1.2645 mm   (cap 2.500)
  foot A_patch / A_max      min 0.1574  p25 0.2547  med 0.2895  p75 0.4630  max 0.5927
  tip  A_patch / A_max      min 0.0153  p25 0.0653  med 0.0960  p75 0.3854  max 0.5047
  foot A_anchor / neck      min 1.1691  p25 1.6737  med 1.8604  p75 2.0000  max 2.0000      (needs >= 1)
  tip  A_anchor / neck      min 1.1974  p25 2.0000  med 2.4665  p75 2.9508  max 16.4790     (needs >= 1)
```

**Test (c) refuses 0 of 154, with more headroom than before, not less.** Worst rule-1 ratio
0.593 (it was 0.841 at the old depths); worst `L_scar` 1.881 mm against a 2.500 mm cap (it was
2.300 mm in the sweep); worst rule-3 ratio 1.169 (it was 1.2105 over the recovered population,
so this one tightened slightly and is still clear). **The load-sized anchor is smaller in every
scar dimension, which is the direct reason no scar anywhere in the recovered population now
exceeds `A_ABS_MM2 = 2.0 mm²` (§6.3).**

**What is owed.** The falsification sweep of §3.3 was run over `(r0, r_x)` with `T` fixed. It
must be re-run over `(r0, r_x, T)` with `T` free in `[ball_seat_factor(1.4)·r_foot / 1,
chord]`, before any implementation claims a bound rather than a measurement. **This document
claims a measurement.** It is stage 1's job to produce the corpus version of it, and §10.4's
sabotage set is what stops the bound being asserted without one.

**And its role in this design is smaller than `aae1559` claimed**, which §3.4 already said and
which R1 makes sharper: the scar rules refuse nothing, before or after.

### §3.2 The proof — kept for its formulas, retired as a proof

**Read this subsection as the definition of the scar quantities, not as a live guarantee.**
Every step below assumes `T = δ = 0.3 mm`, which R1 replaced. The formulas are unchanged and
still normative; the conclusions are superseded by §3.1's measurement and by the re-run §3.1
names as owed.


Write `T` for the length of the seat interval — pinned at `δ` by construction — `r_x` for the
frustum radius at the surface crossing, `r_end` for the end radius, `R_b` for the ball radius,
and `d` for the perpendicular distance from the embed endpoint to the surface. The registered
formulas (`transit_measurement.rs:301-368`) are:

```
A_shaft  = min( π·r_x² / max(cosang, EPS_COS),  2·r_x·T )
A_ball   = π·max(R_b² − d², 0)
A_patch  = A_shaft + A_ball          A_max = max(A_ABS_MM2, KAPPA·π·R_b²)
L_scar   = min( 2·r_x / max(cosang, EPS_COS),  T + 2·r_x )
A_anchor = A_ball + π·r_x²           neck  = π·r_end²
```

**The `min()` in `A_shaft` and `L_scar` is the whole argument.** Both are capped by a term that
does not contain `cosang`. So `A_shaft ≤ 2·r_x·T` and `L_scar ≤ T + 2·r_x` hold at *every*
incidence, including perfectly grazing. With `T` pinned at 0.3 mm:

**Rule 2 (scar dimension).** `L_scar ≤ 0.3 + 2·r_x`. The largest registered radius is
`TIP_RADIUS_MAX_LEGACY_MM = 1.0 mm`, so `L_scar ≤ 2.3 mm < 2.5 mm = L_SCAR_MAX_MM`. ∎

**Rule 1 (scar area).** `A_ball ≤ π·R_b² = A_design`, and `A_max ≥ 1.25·A_design`, so the
headroom above the ball term is at least 0.4 mm². And `A_shaft ≤ 0.6·r_x ≤ 0.6·r_end`, because
the frustum grows toward the foot so `r_x ≤ r_end` there. Two cases: if `A_design ≤ 1.6` then
`r_end ≤ 0.510` and `0.6·r_end + A_design ≤ 1.906 < 2.0`; if `A_design > 1.6` then `r_end >
0.510` and `0.6·r_end ≤ 1.5395·r_end²` reduces to `r_end ≥ 0.390`, which holds. ∎

**Rule 3 (anchor minimum).** At the **contact tip** the frustum grows away from the tip, so
`r_x ≥ r_end` and `A_anchor ≥ π·r_x² ≥ π·r_end² = neck` with no help from the ball term. ∎ At
the **foot** `r_x ≤ r_end`, so the ball term makes up the difference. `d ≤ T = 0.3`, so
`A_ball ≥ π(1.96·r_end² − 0.09)` and rule 3 reduces to `0.96·r_end² + r_x² ≥ d²`, for which
`0.96·(0.35)² = 0.1176 ≥ 0.09` suffices. Since `r_foot ≥ 0.35` always, it holds. ∎

### §3.3 The falsification sweep, and the assumption inside it

**MEASURED** (`scratchpad/land-at-strike-out/theorem.ts`): 1,002,001 `(r0, r_x)` pairs over the
full registered radius range `[0.05, 1.0]`, both ends graded, worst case taken over the
incidence. Failures: 0. Worst rule 1 ratio 0.841. Worst `L_scar` 2.300 mm. Worst rule 3 ratio
1.000, attained exactly.

**The honest caveat, and a second one the first version did not state.** The rule-3 worst case at
ratio 1.000 is a sweep artifact: it needs `r_x = r_end` with the ball wholly proud of the
surface, which the real parameterization forbids. Evaluated over the real 129-record
parameterization at the R1 depths the minimum ratio is **1.1691**. An implementation **must**
compare with `>=`, as `recorded_constraints` already does, and **must** derive `r_x` from the
landed frustum rather than the original 2.8 mm one.

**The second caveat is the load-bearing one, and R4 answered half of it.** `A_ball` is a
**half-space formula**: it credits a spherical cap buried in one surface with no far side at
all. Over the recovered population that term supplies most of the anchor area.

* **Along the seat's own line, R4 makes the assumption true by construction.** The seat is
  truncated at the opposite face, so the cap's own depth never reaches a far side. At
  `10b0bd8` this was policed by the foot-ball breach check, which refused 13 of 152. It is now
  a clamp instead of a test.
* **Laterally, it is still an assumption, and that is E-D.** At oblique incidence the buried
  cap reaches `h_eff + R_b·(1 − cosang_entry)` below the entry face, which exceeds `W` on 65 of
  the 129 recovered records. Where it does, the half-space picture is wrong: the anchor is a
  plug spanning the wall, not a cap in a half-space. **Rule 3 must never be read as evidence
  that a foot fits.** §4.7 brackets what that costs.

### §3.4 What the bound does and does not buy

It buys: test (c) is not a binding test, so the recovery rate is decided by geometry — does the
stub strike thick enough material, at a good enough angle, in the right direction? — rather than
by scar arithmetic. That is the right shape.

It does not buy: **any claim that the landed struts are cheap.** `aae1559` drew that inference
and it does not follow. The bound proves each scar is *legal*. It says nothing about how large
it is. **R1 is what made them small** — at the load-sized depths not one scar in the recovered
population exceeds the flat 2.0 mm² allowance, where at `10b0bd8` twelve did and three of the
ten production scars cleared only through the radius-scaled `KAPPA` branch. That is a
measurement, not a bound, and it can move on another model.

It does not buy: **any claim about the aggregate scar.** The bound bounds one scar. Nine struts
make eighteen scars, and each pair sits within a fraction of a millimetre of the other on either
side of a crevice. **Nothing in the registry bounds a scar population**, and this design does not
invent one — it discloses (§6). Note the shape of the trade R1 produced: **the aggregate is
essentially unchanged (15.249 mm² against 15.256 mm²) while the number of struts went from 5 to
9.** Each mark is smaller; there are more of them.

**And it does not buy the sentence the first version drew from it.** That version wrote:
*"Landing does not merely rescue the stub; it makes the scar smaller."* Three things are wrong
with it and all three are withdrawn. The arithmetic it quoted did not close (`T + 2·r_x` with
`r_x = 0.5877` needs `T = 1.4836`, not the 1.184 mm it named). The baseline it compared against
— grading the stub as a deep plug — is one the same document's §1.2 had already retired. And the
real baseline is **zero**: today the stub is condemned, so it is not printed and leaves no mark.
**Landing makes the scar bigger than nothing.** That is a perfectly good trade to put to Craig,
and it is the one §0 puts.

### §3.5 Where the incidence actually bites, now that it is measured

Only rule 3 at the foot, and only through `d`. The first version graded `d` at its largest
possible value, which made the verdicts conservative. **P1 measures `d` directly**, so §4's
verdicts are exact rather than bounded. The measurement did not change any rule-1-to-3 verdict:
test (c) still refuses nothing. **The conservatism was never where the recovery rate was being
lost.** It was being lost in test (a), which was not being measured at all.

### §3.6 Test (f): build order, and what it is a proxy for

**Correcting the first version's audit.** It said there is no build-order, print-order or
Z-monotonicity rule anywhere in the support crates. That is not true. There are two:

* `cco.rs:4967`, inside `junction_feasible` (which opens at `:4948`): `j[2] < se[2] - 1e-9`. A
  routing junction must sit strictly below the stub end it routes from. It compares a **junction**
  to a **stub end**, not a member's own two ends, so it supports analogy, not reuse.
* `anchor.rs:1063`: `sample.point_mm[2] > stub_end[2] + BELOW_LEVEL_EPS_MM` →
  `AnchorSeatRefusalReason::AboveContact`, with `BELOW_LEVEL_EPS_MM = 1e-6`. **A model-anchor seat
  must already be at or below the stub end it serves.** This is the closer analogue: it is a
  model seat, graded against a contact, in the same sense.

So test (f) is not a new idiom. It is the third instance of a rule the engine already applies in
two places, extended for the first time to a member's own two ends.

What is still true is that the engine has no *general* rule. `Forest::validate`
(`forest.rs:1198+`) audits graph topology only and reads no geometry fields at all;
`Forest::set_parent` (`forest.rs:822-860`) checks self-parenting, cycles and brace exclusion and
never compares Z; there are zero matches for `build_order`, `print_order`, `z_order`, `gravity`;
and the repo says the hole out loud at `forest.rs:872-881`.

**Why the rule is necessary.** The printer builds bottom-up. A strut running *down* from the
island to a feature already printed is a support. A strut running *up* to a feature that does not
exist yet is a **hanger**: at the moment the island prints, the strut holds nothing, and all it
contributes is a second scar. Nothing in the scar rules, the area law or the crevice measurement
can see the difference.

**MEASURED, over the 154 landing candidates, at the R1 foot positions:**

```
foot BELOW contact : 152 / 154
foot ABOVE contact :   2 / 154
```

Both hangers are island 74's small sub-patch, terminals 189 (re-plan 0) and 167 (re-plan 1),
`carried = 3.288 mm²` each. The load-sized foot advances less far along the axis than the old
0.3 mm foot did, so their `dz` moved from `+0.441 mm` to **`+0.355 mm`** — smaller, still
positive, still hangers. **They must be refused, and test (f) is what refuses them.** They are
also the only two records in the whole population that any test other than (a) refuses.

**Statement, using the existing idiom and no new constant:**

> A landed strut is legal only if its foot end is strictly below its contact end:
> `p1[2] < p0[2] − 1e-9`.

The `1e-9` is a hardcoded epsilon, not a registry entry — as are `BELOW_LEVEL_EPS_MM` at
`anchor.rs:26` and `FOOT_BREACH_HALF_DEG` / `FOOT_BREACH_CONE_RAYS` at `anchor.rs:32-34`.
Reusing an existing hardcoded epsilon is recorded here as small, known debt.

**What test (f) cannot see, named honestly, with the one part P1 could settle now settled.**

1. **Whether the struck face is itself an unsupported overhang at that layer.** The foot is
   below the contact, so (f) passes, but the surface it lands on may be downward-facing with
   nothing under it when it prints. The strut then props one island on another. **P1 records the
   struck face's outward normal, so this is now MEASURED**: of the 129 recovered records, **6
   land on downward-facing material** (`normal_z < 0`), carrying 3.143 mm². **At the production
   plan it is 0 of 9.** So on this model the concern is real but small. It is reported, not gated: whether a
   downward-facing struck face is disqualifying is a question about the *receiving* feature's own
   support, which is E-A's territory, and inventing a gate here would pre-empt Craig's ruling.
2. **Whether the foot lands on a different connected component.** Nothing in §2.3's tests asks
   whether the struck surface belongs to the same body as the contact. `NativeSurface` is the
   whole scene. On a multi-part plate — the stated goal of this project — a landed strut would
   weld two separate minis together and every test in §2.3 would pass. **P1 cannot see this**;
   face ids do not carry component identity. This is task **#65**'s gap and it is escalation §12
   item 11.
3. **Connectivity in general** — the same gap as (d)'s open half, per §2.5. Counted once.

---

## §4. THE 208-RECORD RE-GRADING

### §4.1 Method

`.git/land-at-strike-p1-evidence/regrade-r134.ts`, run against that directory's raw JSONL — the
same evidence `10b0bd8` used, re-graded under R1, R3 and R4. `regrade.ts`, the `10b0bd8`
grader, is kept beside it unmodified so the two tables can be diffed record by record, and
`compare-r134.ts` does exactly that.

For each of the 208 condemned records: sort its intervals; find the seat interval (start within
`TOL_MM = 0.05` of `p0`); apply R4 to that seat; then find the first firm strike beyond `δ`
(skipping print-minimum moot crossings) and apply the tests of §2.3.

Four method points, three carried forward and one new:

* **Every test is in the classifier.** `ok = tA && tB && tC && tE && tF`, with `tB ≡ true` by
  R3 and `tD` folded into `tA` by R1. Nothing is reported in prose that the table does not
  count.
* **A stub whose own tip never reaches the model is not a landing candidate whatever its shaft
  does further out.** Unchanged, and it still holds 52 records out of the rule's reach (§7).
* **A stub whose own seat runs deep IS now a candidate.** This is R4's doing: the seat is
  truncated at the opposite face rather than condemned, so the record rejoins the population.
  Four records move; two of them then need no landing at all.
* **Every constant is read at its registered value and none is moved or minted.** The engine's
  implied area ratio is recovered from the data (12.000000) rather than retyped, and
  `ball_seat_factor` is computed from its registered factors rather than transcribed.

**One correction to `10b0bd8`'s own bookkeeping.** Its shape count was defined as the number of
distinct `(carried area, condemning overlap)` pairs, but its grader never wrote the overlap onto
the row, so the key silently degraded to carried-area-only. Under the stated definition the
population has **79** shapes, not 77. (77 is the number of distinct carried areas, which is why
the wrong number looked plausible.) The corrected counts are in §4.2 and they sum.

### §4.2 The recovery table under R1 + R3 + R4

| class | records | shapes | carried mm² | share |
| --- | ---: | ---: | ---: | ---: |
| **LANDS — every test passes** | **129** | 57 | **467.067** | 62.0 % |
| **STRIKES, ANCHOR CANNOT CARRY — must re-aim** | 25 | 6 | 182.239 | 12.0 % |
| **TIP NEVER EMBEDS — a different defect** | 52 | 15 | 107.498 | 25.0 % |
| **SEAT TRUNCATED, NOTHING BEYOND — legal outright** | 2 | 1 | 1.314 | 1.0 % |
| **total** | **208** | **79** | **758.118** | |

**Against `10b0bd8`'s 80 / 279.176, record by record (`compare-r134.ts`):**

```
  LOST   (recovered at 10b0bd8, refused now) :  0 records,   0.000 mm2
  GAINED (refused at 10b0bd8, recovered now) : 49 records, 187.891 mm2
     refused by (a)(i), the flat 0.3 mm wall  : 34 records,  99.881 mm2   <- R1
     refused by (a)(ii), the foot-ball breach : 11 records,  85.207 mm2   <- R4
     refused by both                          :  2 records,   2.738 mm2
     refused by (b), crevice < 0.05 mm        :  2 records,   0.065 mm2   <- R3
     classed DEEP_PLUG, condemned outright    :  2 records,   2.106 mm2   <- R4
```

**This is a strict admission, and that is the single most important structural fact about the
rulings.** Nothing that recovered before is refused now. Nothing that any test refused before is
refused by a *different* test now. Each ruling deletes a refusal and nothing replaces it.

### §4.3 Why the residue fails

```
per-test refusals over the 154 landing candidates
  (a)  load-sized truncated anchor capacity   23   175.662 mm2
  (b)  crevice width           [R3: inert]     0     0.000 mm2
  (c)  scar rules, either end                  0     0.000 mm2
  (e)  tip section area law                    0     0.000 mm2
  (f)  foot above its own contact              2     6.576 mm2

first-and-only failure, by combination
  anchor capacity only         23   175.662 mm2
  build order only              2     6.576 mm2
```

**The partition is now clean in a way it was not before.** Every re-aim is a re-aim for one of
exactly two reasons: the material where the stub struck cannot hold an anchor that carries the
load, or the stub is a hanger. There is no residual "thin wall" class, because thin is no longer
a category — it is an input to a capacity calculation.

**And the walls that refuse are genuinely knife-edges.** MEASURED over the 23 capacity failures:
`W` runs min 0.014, p25 0.014, median 0.037, p75 0.049, **max 0.122 mm**. Every one is under
twice the ratified print minimum for solid material. The reviewer's grazing-incidence
justification for `10b0bd8`'s wall clause is preserved in full — those knife-edges are still
refused — but it is now delivered by physics rather than by a borrowed constant.

**Say it plainly, as the reviewer required: (c) and (e) refuse nothing, and (b) can no longer
refuse anything.** Test (a) decides 23 of the 25 re-aims on its own; (f) decides 2. The scar
rules and the tip area law decide nothing on this model.

### §4.4 Per re-plan

| re-plan | condemned | lands | re-aim | tip never embeds | seat truncated, no landing | carried | landed carried |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 59 | 42 | 5 | 11 | 1 | 128.07 | 69.47 |
| 1 | 57 | 40 | 5 | 11 | 1 | 120.41 | 72.79 |
| 2 | 18 | 9 | 3 | 6 | 0 | 101.61 | 64.65 |
| 3 | 18 | 9 | 3 | 6 | 0 | 101.61 | 64.65 |
| 4 | 19 | 10 | 3 | 6 | 0 | 102.77 | 65.81 |
| 5 | 19 | 10 | 3 | 6 | 0 | 102.03 | 65.06 |
| **6 (production plan)** | **18** | **9** | **3** | **6** | **0** | **101.61** | **64.65** |

The production plan is stable across re-plans 2-6, as before. The seat-truncated class still
exists only in the first two re-plans.

### §4.5 The production plan, stub by stub

```
LANDS  (9 stubs, 64.647 mm2)                         W = model thickness along the seat's line
  t87  isl 69  carried 11.879  W 0.895  h_des 0.236  h_eff 0.236   cap 11.879  crevice 1.585  L' 2.126
  t100 isl  8  carried  0.715  W 0.094  h_des 0.147  h_eff 0.094*  cap  3.137  crevice 0.782  L' 1.464
  t142 isl 70  carried  5.984  W 1.887  h_des 0.167  h_eff 0.167   cap  5.984  crevice 0.419  L' 0.904
  t157 isl 71  carried  9.974  W 0.302  h_des 0.216  h_eff 0.216   cap  9.974  crevice 0.174  L' 0.839
  t162 isl 74  carried 13.019  W 0.341  h_des 0.247  h_eff 0.247   cap 13.019  crevice 0.740  L' 1.375  <- ISLAND 74
  t173 isl  0  carried  0.825  W 0.701  h_des 0.147  h_eff 0.147   cap  4.618  crevice 1.364  L' 1.902
  t186 isl  0  carried  6.306  W 0.177  h_des 0.172  h_eff 0.172   cap  6.306  crevice 1.156  L' 1.799
  t215 isl 48  carried  8.016  W 0.374  h_des 0.194  h_eff 0.194   cap  8.016  crevice 1.004  L' 1.614
  t281 isl 68  carried  7.929  W 0.210  h_des 0.193  h_eff 0.193   cap  7.929  crevice 0.763  L' 1.352
                                                      * = truncated at the opposite face by R4

MUST RE-AIM  (3 stubs, 23.914 mm2)
  t154 isl 66  carried 12.661  W 0.037  h_eff 0.037  capacity  2.228  -- anchor cannot carry
  t229 isl 65  carried  6.800  W 0.014  h_eff 0.014  capacity  0.621  -- anchor cannot carry
  t231 isl  3  carried  4.453  W 0.049  h_eff 0.049  capacity  1.727  -- anchor cannot carry

TIP NEVER EMBEDS  (6 stubs, 13.053 mm2) — see §7, unchanged
  t115 isl 47  1.962     t256 isl  7  1.419     t273 isl 51  3.258
  t275 isl 72  3.434     t277 isl 51  2.742     t284 isl  7  0.238
```

**Four stubs moved from re-aim to lands: t100, t162, t186, t281**, 27.969 mm² between them.
t100 is the only production strut whose seat R4 truncates — it wants 0.147 mm and the wall gives
it 0.094 mm, which still carries its 0.715 mm² with a capacity of 3.137 mm².

Note t157 again, because `10b0bd8` made it the marginal case: at the old flat depth its wall
cleared 0.300 mm by 2 µm. Under R1 it asks for 0.216 mm of a 0.302 mm wall and clears by 86 µm.
**It is no longer marginal on wall thickness at all** — what it still is, is the production
strut sitting in a 0.174 mm crevice, and R3 is why it lands and is disclosed rather than left
open.

### §4.6 Island 74, in full

Nine condemned records touch island 74 across the seven re-plans. **Seven of them land.** Two
distinct shapes:

| shape | re-plans | carried | `r0 = r_foot` | `R_b` | axial chord | cos at entry | `W` | `h_design` | `h_eff` | `a_eff` | capacity | verdict |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| t184 / t162 | 0-1 | 8.996 | 0.4885 | 0.6839 | 0.4629 | 0.7374 | 0.3413 | 0.2053 | 0.2053 | 0.4885 | 8.9955 | **LANDS** |
| t162 | **2-6** | **13.019** | 0.5877 | 0.8227 | 0.4629 | 0.7374 | 0.3413 | 0.2469 | 0.2469 | 0.5877 | **13.0188** | **LANDS** |
| t189 / t167 | 0-1 only | 3.288 | 0.2953 | 0.4900 | 0.7386 | 0.9627 | 0.7110 | 0.1471 | 0.1471 | 0.3500 | 4.6181 | **foot ABOVE contact — refused by (f)** |

**Answer to the question that started this lane: yes, by landing — and by exactly the two
rulings that were made for it.** `10b0bd8` refused island 74 because the foot the load required
put a 1.645 mm ball centre-deep into a 0.341 mm wall and the engine's own `sphere_breach`
reported it 0.241 mm out the far side. R1 says the foot is not driven centre-deep; it is driven
`ball_seat_factor(1.4) × 0.5877 = 0.2469 mm`, which is 72 % of the wall it hit. R4 says that
even if it wanted more, it would stop at the far face rather than come through it. On those two
rulings the stub lands, and the arithmetic is:

```
  demand    carried_mm2                                = 13.018786 mm2
  shaft     r0 from FrozenTipLoadPolicy at X = 12       =  0.587651 mm
  foot      r_foot = max(r0, MIN_SHOOT_R_MM)            =  0.587651 mm   (not clamped)
  ball      R_b = BALL_FACTOR_FOOT * r_foot             =  0.822712 mm
  wall      W = chord 0.462902 * cos_entry 0.737412     =  0.341350 mm
  depth     h_design = ball_seat_factor(1.4) * r_foot   =  0.246933 mm
  R4        h_eff = min(h_design, W)                    =  0.246933 mm   NOT TRUNCATED
  cut       a_eff = ball_cut_radius_mm(r_foot, h_eff)   =  0.587651 mm
  capacity  X * pi * a_eff^2                            = 13.018786 mm2
  ratio     capacity / demand                           =  0.999999999
```

**The margin is zero, and the design says so rather than rounding it away.** `a_eff` comes out
exactly equal to `r_foot` because that is what `ball_seat_factor` is *for* — it is the depth at
which the seat matches the shaft. And the shaft was sized to exactly carry the load. So the
anchor carries the load exactly, no more, and the pass is delivered by the same relative
tolerance the area law is compared under everywhere else in the engine (test (e) needs it for
the same reason: the probe records `r0` to nine decimals). **If a reviewer wants a margin here,
the place to put it is the load law's own safety factor, which is already inside `X = 12`, not a
new number bolted onto this rule.** §6.5 discloses the whole zero-margin population.

**What island 74 does NOT get from this.** Its foot ball is 1.645 mm across in a 0.341 mm wall,
so at 42° the buried cap reaches 0.463 mm below the entry face and R4 truncates it: **the anchor
is a plug that spans the wall.** The joint is priced. The wall around the plug is not. That is
E-D (§4.7), and island 74 is its largest instance. Under the two conservative brackets of §4.7
island 74 does **not** land (capacity 7.181 mm² or 4.889 mm² against 13.019 mm² demanded), so
**this is the record whose verdict the gap actually decides**, and the design reports both
numbers rather than choosing the flattering one.

### §4.7 E-D — the derivation gap, bracketed

**Where the arithmetic stops.** `ball_cut_radius_mm` is a half-space formula. It prices a cap
buried in one surface with no far side. R4 guarantees no far side **along the seat's own line**.
It does not guarantee one **laterally**: at incidence `c` the buried cap reaches
`h_eff + R_b·(1 − c)` below the entry face, and where that exceeds `W` the anchor is truncated
into a plug spanning the wall rather than a cap in a half-space.

**MEASURED:**

```
  recovered records whose foot ball is clipped by the opposite face : 65 / 129   314.362 mm2
  at the production plan                                            :  6 / 9      45.959 mm2
                                                (t100 t157 t162 t186 t215 t281; t87 t142 t173 are not)
```

**Why the design grades the way it does, and why that is not the flattering choice by default.**
Truncating a ball at the far face removes material on the deep side. It does **not** reduce the
minimal separating cut at the entry plane, which is what the area law is evaluated on — if
anything the entry-plane cut of an obliquely driven ball is *larger* than the closed form
credits. So a "conservative" grading that docks capacity for clipping would be **inventing a
penalty**, not applying an existing law conservatively. What clipping really does is move the
failure mode: from the joint, which the area law prices, to **the wall around the plug, which
nothing prices**. That is E-A's question — *does the struck feature carry the load onward?* —
with a sharper instance and a bigger count.

**The bracket, so the exposure is a number and not a worry.** Two alternative truncation
geometries, both computable from the same measured evidence, both stricter than the one this
design uses:

| grading | what it assumes | records | carried |
| --- | --- | ---: | ---: |
| **as graded** (§2.8) | the seat is capped at `W`; the entry-plane cut is what carries | **129** | **467.067 mm²** |
| ball must fit uncut | credit only the depth whose whole ball stays inside `W` | 70 | 154.654 mm² |
| inscribed sphere | cap the depth at the interval's MEASURED `max_depth` — the largest sphere that fits inside the struck feature anywhere on the strike interval | 93 | 177.092 mm² |

**So E-D is worth up to 59 records and 312 mm², and it is the widest open number in this
document.** It is escalation §12 item 16. Note that both brackets fall *below* `10b0bd8`'s
80 / 279.18, so this is not a case where the answer is "somewhere between the old and the new
table" — the gap is genuinely two-sided and only a physical answer closes it.

**What would close it.** Not a constant. Either (i) a kernel query for the minimal separating
cut of a truncated seat, which is a real geometric quantity the kernel could compute, or
(ii) Craig's ruling that a plug spanning a wall is acceptable — which is the same ruling E-A
wants and which stage 1's corpus instance count is meant to inform.

### §4.8 Spreads over the recovered population

```
crevice                    min 0.046  p25 0.432  median 0.782  p75 1.162  max 2.344 mm
landed length L'           min 0.574  p25 0.991  median 1.410  p75 1.799  max 2.800 mm
seat depth h_eff           min 0.063  p25 0.147  median 0.147  p75 0.193  max 0.247 mm
wall W at the strike       min 0.063  p25 0.210  median 0.399  p75 0.895  max 1.891 mm
entry incidence cosang     min 0.084  p25 0.486  median 0.617  p75 0.737  max 0.997
foot scar A_patch          min 0.315  p25 0.509  median 0.579  p75 0.926  max 1.478 mm2
tip  scar A_patch          min 0.031  p25 0.131  median 0.192  p75 0.771  max 1.463 mm2
carried area               min 0.002  p25 0.475  median 1.187  p75 7.929  max 13.019 mm2
capacity / demand          min 1.000  p25 1.000  median 3.466  p75 9.718  max 2760.9

crevice >= SPACING_FLOOR_MM (2.5 mm)       :   0 / 129
crevice <  0.30 mm (Craig's weld line)     :  18 / 129   69.535 mm2   [R3: landed and disclosed]
records carrying a scar above 2.0 mm2      :   0 / 129
records landing on downward-facing material:   6 / 129    3.143 mm2
records whose foot ball is clipped (E-D)   :  65 / 129  314.362 mm2
records at capacity/demand = 1.000         :  46 / 129  405.076 mm2
```

---

## §5. THE RE-AIM FALLBACK, NOW THE SMALLER JOB AGAIN

### §5.1 What is left for re-aim

**25 records, 6 distinct shapes, 182.239 mm² carried; 3 stubs and 23.914 mm² at the production
plan — and island 74 is no longer among them.** They divide by first-and-only failure into
exactly two rows:

| reason | records | carried | reading |
| --- | ---: | ---: | --- |
| the anchor the load requires cannot be built in the material struck | 23 | 175.662 | `W` runs 0.014 – 0.122 mm: firm material above the 0.08 mm print minimum, but knife-edges. The refusal is a capacity calculation, not a threshold |
| the foot would sit above its own contact | 2 | 6.576 | the hangers of §3.6 |

**This is a cleaner partition than `10b0bd8`'s five rows**, and the reason is R1: "the wall is
too thin" and "the ball does not fit" and "the crevice is too narrow" were three different
proxies for one question, which is whether the load can be carried where the stub arrived.
There is now one question and it is answered in the units of the thing being decided.

**Re-aim carries far less than landing does** — 182 mm² against 467 mm², and 3 of 18 production
stubs against 9. `10b0bd8` had it at 7 of 18 and called re-aim the larger lane; the rulings
reverse that back. §10 restages accordingly, and the practical consequence is that **stage 5
(re-aim) is now a residue lane, not the main event.**

### §5.2 The dependency on SPEC-P8-RELOCATE's R1 (the re-aim), stated honestly

**Throughout this subsection "R1" means the SPEC's re-aim ladder step, not Craig's load-sized
anchor ruling** (§1.3). The re-aim is adopted unchanged in its search, its pinned enumerations (`P8_REAIM_POLAR_DEG =
(0, 10, 20, 30)`, `P8_REAIM_AZIMUTH_DEG = (0..315 step 45)`, `P8_REAIM_LENGTH_FACTORS =
(1.0, 0.75, 0.5)`, ≤ 75 candidates), its global lexicographic minimum, and its resin guards. All
five `P8_REAIM_*` values and the pocket pad stay exactly where they are.

**The first version claimed more than that, and the claim is withdrawn.** It said *"every R1
candidate is itself landed at its own first strike, so `T = δ` for every candidate"*, and then
used that to argue that re-aim can never produce a deep plug, and then used *that* to argue E-C
was closed.

`SPEC-P8-RELOCATE.md:401-418` pins the opposite. R1 fixes the contact surface point, puts
`p0 = c − n̂·δ` and `p1 = c + n̂·L` with `L = stub_length_mm × f`, and then:

> "The SCAR-AREA-CAP inputs are RE-DERIVED per candidate, never carried over: `T` = tip-anchored
> interior interval length on the axis ray from `p0` along `u` … Re-aim changes `cosang, T, r_x,
> d`, so the verdict MUST be recomputed."

**R1 does not land its candidates.** It keeps the full span and re-sweeps `T`, which grows
without limit if you aim into thick material. That is why R1 has a legality clause (a) at all.
Landing R1's candidates would replace R1's pinned `p0` formula, change `L`, change `r(s)` and
the frustum for every candidate, and therefore change the score tuple's `length_reduction_step`
term.

> **The honest dependency, stated once.** *If* R1's candidates are also landed at their first
> strike, then R1's clause (a) collapses to §3's bound and the search gets strictly smaller. That
> is an attractive simplification. **It is an amendment to a spec this design does not own, it is
> not ratified, and nothing in this design may depend on it.** In particular, §8's E-C
> disposition is written so that it does **not** depend on it (§8.4).

**Not settled here:** whether R1's `n_moot_crossings` score term should count moot crossings in
the crevice. The design's position is that it should, unchanged. R1 is owner-ratified, so this is
recorded, not amended.

### §5.3 What stays honestly condemned

**Nothing, on this model, for a seat reason.** R4 dissolved the deep-seat class: its 4 records
become 2 that are legal outright and 2 that land (§8.3). After landing and re-aim, and setting
aside the tip defect of §7, the honestly condemned residue on the Rotting Warhorse is **empty at
the production plan** — but only if re-aim succeeds on all three of its remaining stubs, which
is not measured and must not be assumed.

The `BlockedCause::StubReentry` class (`coverage_closure.rs:375-381`) must therefore still
**survive** — it stays the right name for whatever the re-aim cannot place, and stage 2's gate
requires it to fire non-zero on this model until stage 5 lands.

---

## §6. REMOVAL AND RESIDUE — the honest disclosure

### §6.1 Why this is a disclosure and not a gate

**There is no removability model in this engine.** The audit is unambiguous:

* Six registered constants exist for it — `GRIP_CLEAR_MM = 5.0`, `GRIP_HANDLE_LEN_MM = 10.0`,
  `GRIP_HANDLE_R_MM = 1.0`, `GATE_ON_REMOVAL_DEFAULT = false`, `PULL_DIST_MM = 15.0`,
  `PEEL_ELEVATIONS_DEG` — and **each appears exactly once outside its own declaration, in a
  registry-parity test.** Zero readers. Their `source_locators` point at
  `scripts/support_placement/config.py`, a file that no longer exists in this checkout.
* `tool_access`, `tweezer`, `snip`, `pliers`: **zero hits** anywhere in `crates/` or `docs/`.
* The only wired constant wearing a removal label is `SPACING_FLOOR_MM = 2.5 mm`, whose registry
  note reads *"Removal-ergonomics spacing floor selected without measurement"*. It merges
  specialized contacts closer than 2.5 mm. It measures no graspable length, no clearance, and no
  approach direction.

So a "reachable" flag cannot be **derived**. Any derivation would be inventing a tool. What the
design can do is **report the geometry and name the one number the project has**, exactly as the
registry describes it. **The GRIP apparatus stays unwired.**

### §6.2 The per-strut disclosure record

Emitted for every landed strut, alongside its existing condemnation/route record:

| field | value | derived from |
| --- | --- | --- |
| `crevice_width_mm` | `s* − δ` | MEASURED, the interval sweep. **R3: disclosed, never gates** |
| `strut_length_mm` | `L' = s* + t` | DERIVED |
| `strut_r_tip_mm`, `strut_r_foot_mm` | `r0`, `max(r0, MIN_SHOOT_R_MM)` | DERIVED, load-sized |
| `struck_wall_perpendicular_mm` | `W = chord × cosang_entry` | MEASURED |
| `anchor_depth_design_mm` | `h_design = ball_seat_factor(1.4) × r_foot` | DERIVED, §2.8. **R1** |
| `anchor_depth_effective_mm`, `anchor_truncated` | `h_eff = min(h_design, W)`, and whether the `min` bound | DERIVED. **R4** |
| `anchor_cut_radius_mm`, `anchor_capacity_mm2`, `anchor_margin` | `a_eff`, `X·π·a_eff²`, `capacity / carried` | `ball_cut_radius_mm` + `area_sufficient`. **`anchor_margin = 1.000` is expected, not an error — §6.5** |
| `struck_face_normal_z` | the struck face's outward normal, Z component | MEASURED, P1 |
| `foot_ball_radius_mm` | `BALL_FACTOR_FOOT · r_foot` | registered, 1.4 |
| `foot_ball_clipped`, `ball_reach_mm` | whether `h_eff + R_b(1 − cosang_entry) > W`, and by how much | DERIVED. **This is E-D's per-record disclosure and it must be emitted** |
| `strut_volume_mm3`, `in_crevice_volume_mm3` | frustum volumes | DERIVED |
| `tip_scar_mm2`, `foot_scar_mm2`, and which allowance each clears through | §3's formulas on measured inputs at the R1 depths | DERIVED |
| `reach` | `Unknown` if `crevice_width_mm ≥ SPACING_FLOOR_MM`, else `PermanentUnlessMeasured` | **the only registered removal-ergonomics distance, used for disclosure only** |

**`foot_ball_breached` and `foot_poke_depth_mm` are gone from this record**, because R4 makes
them constants: `false` and `0.0`. Emitting a field that can only take one value is how a
retired check comes back to life by accident.

**`reach` never gates anything.** It is a label on a report. The two values are deliberately
named so that neither reads as a clean bill of health: even the wide case is `Unknown`, because
2.5 mm is a contact-spacing number, not a tool-reach number.

### §6.3 What the measurement says

**MEASURED, production plan (9 landed struts), at the R1 depths:**

```
 t87  crevice 1.585  L' 2.126  r 0.561 -> 0.561  h_eff 0.236  foot scar 1.261 [KAPPA]  tip scar 1.333 [KAPPA]  across 1.267 / 1.303 mm
 t100 crevice 0.782  L' 1.464  r 0.181 -> 0.350  h_eff 0.094  foot scar 0.487 [A_ABS]  tip scar 0.163 [A_ABS]  across 0.787 / 0.455 mm
 t142 crevice 0.419  L' 0.904  r 0.398 -> 0.398  h_eff 0.167  foot scar 0.646 [A_ABS]  tip scar 0.762 [A_ABS]  across 0.907 / 0.985 mm
 t157 crevice 0.174  L' 0.839  r 0.514 -> 0.514  h_eff 0.216  foot scar 1.207 [KAPPA]  tip scar 1.152 [KAPPA]  across 1.240 / 1.211 mm
 t162 crevice 0.740  L' 1.375  r 0.588 -> 0.588  h_eff 0.247  foot scar 1.478 [KAPPA]  tip scar 1.463 [KAPPA]  across 1.372 / 1.365 mm
 t173 crevice 1.364  L' 1.902  r 0.180 -> 0.350  h_eff 0.147  foot scar 0.540 [A_ABS]  tip scar 0.170 [A_ABS]  across 0.829 / 0.466 mm
 t186 crevice 1.156  L' 1.799  r 0.409 -> 0.409  h_eff 0.172  foot scar 0.805 [A_ABS]  tip scar 0.771 [A_ABS]  across 1.013 / 0.991 mm
 t215 crevice 1.004  L' 1.614  r 0.461 -> 0.461  h_eff 0.194  foot scar 0.954 [A_ABS]  tip scar 0.945 [A_ABS]  across 1.102 / 1.097 mm
 t281 crevice 0.763  L' 1.352  r 0.459 -> 0.459  h_eff 0.193  foot scar 0.926 [A_ABS]  tip scar 0.186 [A_ABS]  across 1.086 / 0.487 mm

 total resin           8.318 mm3 = 0.0083 mL
 new model scars       18 scars (a foot and a tip per strut), 15.249 mm2
 above A_ABS_MM2 = 2.0 0 of 18            <- it was 3 of 10 at 10b0bd8
 largest single scar   1.478 mm2, 1.372 mm across   <- it was 2.448 mm2, 1.77 mm across
 reach                 9 / 9  PermanentUnlessMeasured
 ball clipped (E-D)    6 / 9, carrying 45.959 mm2
```

**The trade R1 produced, stated plainly for pricing.** The strut count went from 5 to 9 and the
carried area from 36.68 mm² to 64.65 mm². **The aggregate scar area did not move** — 15.249 mm²
against 15.256 mm² — because every anchor is shallower and narrower. **Not one of the 18 scars
needs the `KAPPA` allowance to be legal**, where three of ten did before, and the largest is now
1.37 mm across against the 1.60 mm equivalent diameter Craig ratified `A_ABS_MM2` on. Escalation
item 2 of `10b0bd8` — *"three of ten production scars exceed the 2.0 mm² allowance"* — is
**dissolved by R1**, and over the whole 129-record recovered population the count above 2.0 mm²
is **0**.

Over the whole 129-record recovered population: **0 of 129** crevices reach 2.5 mm. The widest is
2.344 mm. **On the honest reading every landed strut on this model is permanent**, which is what
R2 accepts as disclosed.

### §6.4 The four things a reader should not conclude

1. **Not "the scars are fine because they are all under 2.0 mm² now".** They are — 0 of 18 at
   production, 0 of 129 over the whole recovered population, largest 1.478 mm² and 1.372 mm
   across, comfortably inside the *"1.60 mm equivalent diameter… inside the buff/fill/paint
   class"* rationale Craig ratified `A_ABS_MM2` on. **But that is a measurement on one model at
   one set of loads, not a bound.** §3.1 withdrew the bound; a fatter load on another model
   raises `r_foot`, raises `R_b`, and can put a scar back over 2.0 mm² through the `KAPPA`
   branch. Stage 1 is what turns this into a corpus number.
2. **Not "0.0083 mL is negligible".** The resin volume is negligible. The visual result of nine
   permanent bridges across nine crevices on a mini is a different question, and it is not a
   volume question. And **nothing registered bounds a scar population.** There are now 18 marks
   where `10b0bd8` costed 10.
3. **Not "we can measure our way out of this cheaply".** The flush-cutter measurement is
   escalation **E-B**, still open.
4. **Not "the crevice is definitely there" — but R3 has ruled that it does not matter.** 18 of
   129 recovered records sit in crevices below Craig's 0.3 mm field line, 69.535 mm² — **14.9 %
   of the recovered area** — and 1 of 9 at the production plan carrying **15.4 % of the prize**.
   R3's ruling is that if the resin seals, the strut adds no resin, so the case is moot: **land
   them, disclose each, refuse none.** Both shares fell relative to `10b0bd8` (24.8 % and 27.2 %)
   because the recovered population grew around them, not because any of them went away. The
   seven distinct widths are:

   | shape | crevice | wall `W` | carried | present at production? |
   | --- | ---: | ---: | ---: | --- |
   | t147 / t128 | 0.0463 | 1.351 | 0.033 / 0.033 | no — **and these two are what R3 admits outright**, below `MIN_ILLEGAL_LEN_MM` |
   | t82 / t73 (island 0) | 0.1156 | 1.281 | 2.589 / 0.256 | no |
   | t156 / t136 (island 25) | 0.1173 | 1.463 | 0.084 / 0.084 | no |
   | t64 / t57 | 0.1181 | 0.203 | 0.050 / 0.050 | no |
   | t178 / t157 (islands 70, 71) | 0.1741 | 0.302 | 7.940 → **9.974** | **yes — t157, 5 re-plans** |
   | t117 (island 16) | 0.1798 | 1.891 | 0.340 | no |
   | t119 / t105 (islands 19, 17) | 0.1963 | 1.120 | 0.132 / 0.132 | no |

5. **Not "island 74 is safely recovered".** It lands, at a capacity-to-demand ratio of
   0.999999999, with its anchor truncated into a plug spanning a 0.341 mm wall. Two of the three
   gradings in §4.7 refuse it. **It is the record E-D decides.**

### §6.5 The zero-margin disclosure — new, and owed

**MEASURED: 46 of the 129 recovered records, 405.076 mm², have `capacity / demand` equal to
1.000 to nine decimal places. At the production plan it is 7 of 9** — every strut except t100
and t173, which are exactly the two whose foot radius was clamped up to `MIN_SHOOT_R_MM`.

This is not a defect in the grading and it is not a coincidence. It is the composition of two
laws the engine already has:

* `FrozenTipLoadPolicy` grows `r0` until `X·π·r0² ≥ carried` and stops. **The shaft carries the
  load exactly.**
* `ball_seat_factor` sets the seat depth so the seat's minimal cut equals the shaft's radius.
  **The seat carries what the shaft carries, exactly.**

So an unclamped landed strut has **no margin anywhere along its load path beyond the safety
factor already inside `X = 12`** (`tensile / (pressure × SF)`, `SF = 8`). Whether that is
enough is not this design's question — it is the same question every contact in the engine
already answers the same way — **but this rule is the first thing that makes the equality
visible at both ends of one member, and it must be said rather than discovered later.**

Two consequences an implementer must not get wrong:

1. **The comparison must be relative, not absolute.** An absolute epsilon reads the ninth
   decimal as a physics failure and refuses 46 records. §2.6 records the same trap for test (e);
   this is the second instance of it.
2. **Nothing may quietly add margin here.** Rounding `h_design` up, or padding `a_eff`, changes
   the recovery table while looking like a robustness fix. §10.4's sabotage set pins it.

---

## §7. THE CLASS THE MEASUREMENT EXPOSED — 52 stubs whose tip never reaches the model

Not part of Craig's question. Recorded because the grading found it, because it is a quarter of
the population, and because P1 has now settled the part the first version left open — in the
direction the first version did not expect.

**MEASURED.** 52 of 208 records (25 %; 15 distinct shapes; 107.498 mm²; 6 stubs and 13.053 mm² at
the production plan) have **no interval starting at the designed embed endpoint**. The stub's `p0`
is supposed to sit 0.3 mm inside the model. It sits in air.

```
distance from p0 to the nearest model surface (MEASURED by P1, exact projection):
   min 0.0431   median 0.1111   max 0.1561 mm
tip ball radius R_b = BALL_FACTOR_STUB x r0:
   min 0.2043   max 0.6874 mm
```

**A reading note the first version owed the reader.** The same record can show a large
`max_depth` and still be classed here. `max_depth` is measured perpendicular to the surface,
while the class is decided by where the interval *starts* along the axis. A stub whose tip sits
in air can still have its flank graze deep material further along.

**INFERRED cause.** `initial.rs:646` steps inward along `unit`, the contact normal, which is an
interpolated or averaged direction. On a sharp convex feature the averaged normal does not point
into the solid, so a 0.3 mm step along it lands outside.

**What P1 settles, and it is bad news.** The first version said the honest test already exists
and is already ratified: SPEC-EMBED-POLICY rule 3 is written for exactly this — *"also catches
detached tips — `T = 0` and `d ≥ R_b` gives `A_anchor = 0`, ILLEGAL."* It said probe P1 would
settle whether the 52 fail it.

**MEASURED: 0 of 52 fail it.** Every one of the 52 keeps `d < R_b`, so the ball term alone
carries the anchor minimum and rule 3 passes. **The ratified detached-tip test does not catch
this defect.** The distances are 0.043 – 0.156 mm and the ball radii are 0.204 – 0.687 mm; the
tips are outside the model but well inside their own designed balls.

**Disposition.**

* **This is not what the landing rule fixes**, and the rule must not be allowed to paper over it.
  A stub whose contact end is 0.11 mm off the model is not attached to the island; landing its
  far end changes nothing about that. §4.1's classifier enforces this.
* **The repair now needs its own test, because the ratified one does not bite.** That is a real
  finding and a real cost, and it is escalation §12 item 12.
* **The prize is real but it is an upper bound, not a number.** Re-measured under R1 and R4:
  **38 of the 52 (79.991 mm²)** would carry a load-sized truncated anchor *where their shaft
  currently is* — it was 34 at `10b0bd8`. The first version reported the analogous
  figure as *"the prize is real and conditional"*. It is weaker than that. The repair for an
  averaged normal that points the wrong way is to change `unit` — which changes the **direction
  of the ray**, which voids every interval in the record: the strike station, the chord, the
  incidence and the crevice all move. **INFERRED, upper bound only, conditional on the repair
  being a pure translation along the same axis, which the stated cause says it is not.** 7 of the
  52 (17.572 mm²) have no firm crossing beyond the embed at all, so they become legal outright
  under the print-minimum amendment with no landing needed.
* **It is a separate lane.** It touches seat selection and attestation, not transit. It is listed
  as stage 5 in §10 and is not bundled.

---

## §8. E-C DISPOSITION — R4 REWRITES THE PERMISSION. It is not withdrawn.

### §8.1 The question, restated, and the ruling that answers it

SPEC-EMBED-POLICY says: *"There is NO depth cap on the tip-anchored interval itself: a deep
perpendicular plug into thick solid has a small crossing patch and, per the owner's idempotence
ruling, no further physical consequence; thin walls are protected by rule 4 and the
thin-feature-span check."*

**The thin-feature-span check does not exist in this engine.** Escalation **E-C**
(`model-seated-arms-2026-08-21-amendment.md` §B3) constructs the regression: a stub through a
2.0 mm hollow shell at 72° passes rules 1-3, is anchored so rule 4 is moot, and a **6.7 mm shell
perforation becomes legal** where today's 0.05 mm cutoff refuses it.

`10b0bd8` asked Craig to **withdraw** the permission at spec scope, for all three families it
names. **He did not withdraw it. He rewrote it (R4):**

> *"A seat should be allowed to go as deep into the model as it likes, but it should not come
> out the other side of the model so if the seat wants to go deeper than the thickness of the
> model, it should get truncated at the opposite face — it should not create a scar on the
> backside because that provides no additional support so there's no need for it to extend
> beyond the model anyway."*

**That is a better answer than withdrawal, and the reason is worth stating.** Withdrawal would
have banned deep seats in thick solid, where they are harmless and where the permission's own
idempotence argument is correct. What E-C actually feared was **exiting** — the shell
perforation. R4 bans exactly that, and bans it as a construction rather than a test: the seat is
clamped at the far face, so there is no depth at which the hazard exists. **The permission keeps
what it was for and loses what it was dangerous for.**

### §8.2 What the rewritten sentence says

> **SPEC-EMBED-POLICY, rewritten sentence.** *There is NO depth cap on the tip-anchored interval
> itself: a deep perpendicular plug into thick solid has a small crossing patch and, per the
> owner's idempotence ruling, no further physical consequence. **The interval is TRUNCATED at
> the model's opposite face. A designed embed end — seat, foot, ball included — never exits the
> model and never creates a patch on a face other than the one it entered. Punch-through is
> therefore impossible by construction, not by check, and the construction is the penetration
> interval itself: the interval IS the model's extent along the seat's own line, so the clamp is
> `depth <= interval`.** The thin-feature-span check named by the previous text is not built and
> is not needed for this family; thin walls are protected by rule 4, by this truncation, and —
> for a landed foot — by the anchor's own capacity test.*

**Three consequences, in order of how much they matter.**

1. **E-C's constructed regression stops existing.** There is no legal 6.7 mm shell perforation,
   because there is no legal perforation of any depth. The `t225` instance the Rotting Warhorse
   actually contains is graded in §8.3 and it does not perforate either.
2. **The scope objection is answered in full.** `10b0bd8` conceded that declining for stubs alone
   answers *"a third of one half"* of E-C, because the permission's scope line names three
   families — *"contact-stub tips, strut/rung feet, model-rung ends"*. **R4 is written at the
   permission, so it reaches all three.** It is not a stub-family rule.
3. **On the stub path it is still not a runtime tightening** — and that remains worth saying
   plainly rather than selling it as safety. `initial.rs:1542`'s `remaining_illegal_axis_overlap`
   against the 0.05 mm cutoff at `:1553` already condemns this population, and the engine's
   implementation of rules 1-3 rolls into
   `diagnostic_constraints_hold_under_recorded_policy` (`transit_measurement.rs:900`), **whose
   getter has zero callers**, and whose entry point `runner.rs:2819-2821` documents itself as
   *"intentionally not an acceptance, clearance, or defect boundary."* R4 is a **spec repair plus
   a geometry clamp**. Its runtime effect on this model is to make four previously condemned
   records legal, not to refuse anything.

**A spec edit must reach both implementations.** There is a second implementation of the scar
formulas in `plinth-metrics/src/scar.rs`, with its own pinned test
`closed_form_patch_uses_anchored_length_and_never_applies_an_area_cap`
(`plinth-metrics/src/scar.rs:3426`). If the edit does not reach it, engine and auditor diverge.
This was true of the withdrawal ask and it is true of the rewrite.

**A constants note owed under the physics-first doctrine.** R4 mints nothing. The clamp's right
operand is the measured interval; there is no tolerance and no new epsilon. `MIN_ILLEGAL_LEN_MM`
is *not* reused a third time as a seat-depth tolerance — `10b0bd8`'s ask needed that reuse and
carried a derivation for it. **The rewrite does not need it, and the reuse is withdrawn with the
ask.**

### §8.3 The four deep seats, re-graded under R4 — and E-C's one real instance

They are the one place in the measurement where the recorded incidence is at exactly the station
SPEC-EMBED-POLICY wants, so they can be graded **exactly**.

| record | carried | seat run `T` | exited | wall along the seat | `r0` | R1 depth it needs | verdict under R4 |
| --- | ---: | ---: | --- | ---: | ---: | ---: | --- |
| r0 t96 / r1 t88 | 0.657 | 2.487 | yes | 0.474 | 0.1320 | **0.0504** | **LEGAL. Nothing firm beyond it — no landing needed.** Class `SEAT_TRUNCATED_NO_LANDING` |
| r0 t225 / r1 t200 | 1.027 / 1.080 | 0.849 | yes | **0.139** | 0.1650 | **0.0630** | **LEGAL, and it does not perforate.** It then strikes again at `W = 0.484` and **LANDS** |

**Read the second row twice, because it is E-C's hazard and it has stopped being one.** `t225`
runs 0.849 mm through a shell that is **0.139 mm thick** at `cosang = 0.164`, and it exits. That
is the shell perforation E-C constructed, present in a real model. Two independent things now
prevent it:

* **R1 makes it unnecessary.** `t225` carries 1.027 mm². The anchor that load requires is
  `ball_seat_factor(1.5) × 0.1650 = 0.0630 mm` deep. **It needs 45 % of the shell and never goes
  near the far face.** The 0.849 mm run was never a load requirement — it was a fixed 0.3 mm
  embed along an axis that happened to graze a shell.
* **R4 would stop it even if R1 did not.** Had the load wanted more than 0.139 mm, the seat would
  truncate at the far face. Its capacity there would be
  `12 × π × ball_cut_radius_mm(0.165, 0.139, 1.5)² = 1.865 mm²`, still above the 1.027 mm² it
  carries. **Either reading gives the same verdict, which is why this document does not need to
  settle whether R1 also applies at the contact tip** (§2.8, scope).

The first row matters in the other direction, and its `10b0bd8` reading is superseded: it was
graded *"already ILLEGAL under the existing rules"* because `L_scar = 3.138 > 2.500` at a 2.487 mm
seat run. **Under R4 the seat is not 2.487 mm.** It is `0.0504 mm` deep — 4 % of what it was —
so `L_scar` collapses and the scar-dimension rule stops binding. `10b0bd8` was pricing a seat
nobody would build.

**Measured cost of R4 on this model: none. Measured benefit: 4 records, 3.421 mm², from
condemned to legal**, of which 2 land and 2 need nothing at all. **Zero at the production plan
either way.**

### §8.4 What is now closed, and what is not

**Closed by R4:**

* E-C's regression, at spec scope, for all three designed-embed-end families.
* The stub half of E-C, which `10b0bd8` could only answer for one family.
* The `MIN_ILLEGAL_LEN_MM` third reuse, withdrawn with the ask that needed it.

**NOT closed, and it must never be claimed as closed:**

* **The PR-2 placed-mask exemption** (§8.5). It has six live consumers, it decides outcomes
  today, and R4 does not touch it. This is the larger half of E-C and it belongs to the
  clearance bundle.
* **E-D** (§4.7). R4 stops a seat *exiting*; it does not price the wall around a seat that has
  been truncated into a plug. That is new with the rulings and it is escalation §12 item 16.

**And one thing `10b0bd8` claimed that is now simply gone.** It argued that its test (a)(i) was
*"the thin-feature-span check E-C names, built here under another name"*, refusing 59 of 152
candidates on wall thickness. **R1 deleted that clause.** So the design no longer builds a
thin-feature-span check under any name — it builds a capacity test, which is a different thing
and answers a different question. E-C's residual scope is therefore exactly the PR-2 mask, and
nothing else in this document.

### §8.5 What R4 does NOT reach, named for the bundle lane

A larger, parallel exemption is untouched by anything here. The **PR-2 placed mask**
(`contact.rs:391-407`) exempts an anchored region of a member's axis from the distance test with
*"no cosine floor, no clamp, and no arbitrary maximum"*, bounded only by `emerges` — that is, up
to the member's own length minus 0.05 mm. Unlike the scar rules it **decides outcomes today**. A
deep anchored run through a thin wall on an arm is exempted there with no scar rule and no depth
cap applied at all.

**Its six live consumers, at `1c646aa`:**

1. `contact.rs:486` — inside `contact::admit`, the general contact path
2. `contact.rs:151` — `placed_mask_inset_mm`, the shared entry point
3. `brace.rs:1246` / `brace.rs:1279` — the brace gather, as `inset0` / `inset1`
4. `brace.rs:1983` — the brace placed-mask path
5. `cco.rs:3592` — the candidate path
6. `rewire.rs:942` — the rewire path

**This is out of scope for this design and in scope for the clearance bundle (#36)**, which is
already retiring the mask in favour of the frozen seat zone (§9). It is named here so that
"E-C is closed" is never claimed on the strength of the stub half alone.

---

## §9. INTERACTION WITH THE CLEARANCE BUNDLE, AS IT NOW STANDS

**Re-anchored.** The first version carried a provenance caution saying the clearance overhaul was
"in flight in another lane's working tree and is not at `f50a45b`". That is no longer true. Three
commits have landed on `feature/a4-controller`:

* `d6e4310` registers the two-tier demand: `FINAL_CLEARANCE_GAP_MM = 0.300` (the acceptance
  demand) and `STANDOFF_MM = 0.600` relabelled as the placement-family search margin.
* `33f93f4` adds the kernel side: `NativeSurface::segment_clearance_excluding` and
  `faces_near_segments_batch`.
* `1c646aa` adds `crates/plinth-support/src/seat_zone.rs` — the frozen seat zone — and its rule.

Two more have landed since: `e74ab97` proves the seat zone stops at a crevice on a manifold
bridge, and `3fe332b` carries the frozen seat zone on a `Forest` side table (#36).

**Still being wired, by that lane, right now.** At the time this amendment was written the lane
owned `crates/plinth-support/` with `brace.rs`, `cco.rs`, `contact.rs`, `designed_embed.rs`,
`initial.rs`, `rewire.rs` and `runner.rs` all modified in its tree. **Nothing here was built,
edited or staged against that tree.** So this section describes the mechanism as committed, the
wiring it needs is that lane's remaining work, and **the sequencing consequence is in §10.0: this
rule's implementation starts after that lane finishes.**

### §9.1 The quantity the final tier demands, and what it means for a landed strut

The demanded quantity is a **true surface-to-surface gap** — the shortest distance from the
member's own **solid surface** to the model surface, that is, the kernel's axis-to-surface
distance **minus the member radius at the nearest station**. It is not a centreline distance and
not a radius.

Apply that to a landed strut. Inside its crevice the strut is *emerging from* the model at one
end and *entering* it at the other, so its surface touches the model at both ends of the span by
construction, and the minimum true gap over the span is **zero**. Not small — zero. It is zero
for the same reason the contact mask had to exist at all: *"at the seat the arm touches the model
by definition, so the gap grows from zero continuously; demanding a finite gap at the point of
departure demands a step discontinuity, which no straight arm can supply."*

**A landed strut is that argument with no middle.** The measured geometry leaves no room for one:

```
strut total length      0.574 - 2.800 mm   (median 1.410), with a LOAD-SIZED anchor at each end
seat depth h_eff        0.063 - 0.247 mm   (median 0.147)   <- R1: no longer a fixed 0.3 mm
crevice                 0.046 - 2.344 mm   (median 0.782)
crevice < 0.3 mm         18 / 129
crevice >= 2.5 mm         0 / 129
```

At the median the strut crosses 0.78 mm of air with a body 0.35 – 0.59 mm in radius. There is no
station in that span from which a 0.3 mm true gap to *both* crevice walls could be measured,
because the walls are what the strut is standing on. **R1 makes this worse, not better**: the
anchors are shallower, so a larger fraction of each strut's length is out in the crevice where
the gap is zero at both ends.

**Demanding a positive gap there is the #51 error again**, in a new place: a finite clearance is
demanded in a region that is contact by construction. The two-tier ruling does not create this
problem and does not solve it — the target-specific exemption does.

### §9.2 Why the target-specific exemption is the right mechanism — and what it needs that it does not have

The exemption is **target-specific**: a member is exempt from the distance test only where it is
in contact with a target it has **declared**, not by a blanket arc-length inset. A landed strut
declares **two** targets — the island face and the struck face. So:

> **A landed strut's clearance grades against true gaps outside its own two frozen seat zones.**
> The crevice between its declared targets is inside the union of those zones and is not distance
> tested. What remains distance tested is clearance to every body the strut did **not** declare —
> a third feature intruding into the crevice, a neighbouring support, the model elsewhere.

That residual test is not vacuous. It can fail, and it should.

**Why the exemption has to be target-specific and not a length**, in one measured fact from the
mask-honesty audit (`.git/mask-honesty-evidence`): the retired length-based exemption excused an
arm **0.242 mm inside the model** while refusing a same-axis thinner arm that had **0.569 mm of
real gap**. A length-based exemption on a landed strut would be that failure at its worst — the
strut is *entirely* inside the exempt length, so a length rule cannot distinguish "seated on the
two faces it declared" from "buried in a third one".

> **REQUIRED BUNDLE-SIDE EXTENSION, owned by #36, not by this design.** `SeatZoneRequest`
> (`seat_zone.rs:77-84`) carries **one** `seat_face_id` and **one** frozen `band_radius_mm`, and
> `freeze_seat_zones` returns **one zone per request**, flood-filled from that single face. A
> member seated at both ends needs **two** zones, with **different** band radii — the frustum
> grows toward the foot — merged into **one sorted, deduplicated slice** for its row.
>
> **This dependency is now two-sided and both sides know about it.** #36's own wiring work needs
> the same two-zone shape for members it is already routing, and this design's doubly-seated
> struts cannot be admitted without it. It should therefore be built **once, in #36, before this
> rule's stage 2**, and not twice. §10.0 sequences it. Recording it here so that neither lane
> can land a single-zone `SeatZoneRequest` and discover the other's requirement afterwards.
>
> **R1 sharpens the band-radius half of the requirement.** The two band radii are no longer
> "`r_tip` and `r_foot` at a fixed 0.3 mm depth". The foot's zone is frozen around an anchor
> whose depth is `h_eff` — 0.063 to 0.247 mm, per strut, load-sized and possibly truncated — so
> the band radius must be derived from the same `h_eff` the anchor test used, not from a
> constant. An implementation that freezes the foot zone at `DEFAULT_CONTACT_DEPTH_MM` will
> exempt more surface than the seat occupies.
>
> The kernel side is already general: `segment_clearance_excluding`
> (`queries.rs:1974-1984`) takes `excluded_faces: &[&[u32]]`, one slice per row, so a union is
> expressible there today. **The support side is what must change**, either by allowing two
> `(face, radius)` pairs per request or by adding an explicit union step over two requests.
>
> **Without it every landed strut is refused.** `seat_zone.rs`'s own test says why —
> `a_zone_stops_at_the_crevice_and_never_crosses_to_the_foreign_wall`: *"only the seat's own wall
> is CONNECTED to the seat face. The far wall is a foreign feature and must stay graded."* For a
> landed strut the far wall is **not** foreign; it is the declared struck target. With one zone
> the struck wall stays graded, the true gap there is zero by construction, and the member is
> refused. That is §2.4's defect, relocated from the mask to the exclusion set.

### §9.3 What the exemption leaves ungraded, and what replaces it

The exemption means the crevice itself gets no distance test. Three things stand in its place,
and the rule depends on **all three**:

1. **The rim-swept interval evidence (§2.4).** The crevice must be measured outside the solid on
   the axis **and all 8 rim rays**. This is signed inside/outside evidence over the exact region
   the exemption covers, which is strictly more informative than an unsigned distance would have
   been. **The exemption removes more distance testing from the crevice than the retired mask did,
   so this requirement is stronger now, not weaker. Without it, the rule exempts a region nothing
   has looked at, and that is the one thing a reviewer should refuse.**
2. **The scar rules at both ends (§3), plus R4's truncation.** The bound is no longer
   unconditional — R1 freed the depth, so §3.1 downgraded the closed form from a proof to a
   measurement with a re-run owed. What makes the half-space ball term applicable along the
   seat's own line is now **R4's clamp**, not a check (§3.3). Laterally it is still an
   assumption, and that is E-D.
3. **The residue disclosure (§6).** What the exemption declines to gate on, the report must name.

### §9.4 What the bundle makes easier, and the requirement restated

Under the two-tier rule, placement prunes at 0.6 mm and acceptance grades at 0.3 mm. A landed
strut has **no arm**, so it never enters a candidate tournament and never pays the 0.6 mm
placement tier at all. The bundle and this rule do not compete for the same members, so they can
land in either order.

The first version ended this section by saying the rule's only requirement in `contact.rs` is
that `emerges` is not called on a doubly-seated member. **That requirement is satisfied for free
once the seat zone lands** (§2.4). **The real requirement, restated, is §9.2's**: the exclusion
set for a doubly-seated member must be the **union of both seats' zones**, and nothing today
builds it.

---

## §10. THE STAGED LANDING PLAN

**No new registered constant is proposed at any stage, and none is minted.** Every value is
registered: `DEFAULT_CONTACT_DEPTH_MM` (now only the *tip's* designed embed), `MIN_ILLEGAL_LEN_MM`,
`MIN_SHOOT_R_MM`, `BALL_FACTOR_FOOT`, `BALL_FACTOR_STUB`, `A_ABS_MM2`, `KAPPA`, `L_SCAR_MAX_MM`,
`PRINT_FLOOR_CHORD_MM`, `TOL_MM`, `TIP_RADIUS_MAX_LEGACY_MM`, and `SPACING_FLOOR_MM` for
disclosure only. **`ball_breach_eps_mm` drops off this list** — R4 makes the breach check
unreachable on this path, so nothing reads it here. The one derived quantity is
`ball_seat_factor(BALL_FACTOR_FOOT)`, an existing DERIVED function of an existing registered
constant (§2.8). Two hardcoded epsilons are reused rather than registered — `1e-9` for the Z
comparison and `BELOW_LEVEL_EPS_MM` — and `FOOT_BREACH_HALF_DEG` / `FOOT_BREACH_CONE_RAYS` are no
longer reached. All are recorded as existing debt, not introduced.

### §10.0 SEQUENCING — this work starts after the wire-clearance lane

**The wire-clearance lane (#36) owns `crates/plinth-support/` and is actively committing to it.**
Nothing in this design may be implemented until that lane completes, for three reasons, and the
first is the one that would actually break something:

1. **Stage 3 is that lane's work, and stage 2 cannot be accepted without it.** A doubly-seated
   member with one frozen seat zone is refused by the very exemption meant to admit it (§9.2).
   The two-zone `SeatZoneRequest` extension has to exist first. It is also wanted by #36's own
   wiring, so building it there is cheaper than building it twice.
2. **The files collide directly.** Stage 1 and stage 2 both edit `initial.rs`; the disclosure
   record touches `runner.rs`; the seat-zone union touches `contact.rs` and `seat_zone.rs`.
   Every one of those is live in the other lane's tree.
3. **The measure-only stage is only honest against a settled clearance model.** Stage 1's whole
   value is a corpus recovery table that does not move; recording it against a half-wired
   two-tier clearance rule would produce a table that has to be re-measured anyway.

**Order:** #36 completes and merges → stage 3's two-zone extension is available → stage 1
(measure only) → stage 1b (rim sweep, separately gated) → stage 2 (geometry moves) → stage 4
(the SPEC-EMBED-POLICY sentence rewrite, which is doc-only and may go earlier) → stage 5 (re-aim)
→ stage 6 (the tip defect, independent throughout).

**The measure-only stage stays first among this design's own stages.** That was true at
`10b0bd8` and the rulings do not change it: R1, R3 and R4 all move the *verdict*, and a verdict
change is exactly what a measure-only stage is for.

### §10.1 Blast radius, honestly, before the stages

**This touches initial contact minting, which every model in the corpus goes through.**
`build_initial_contacts` runs once per model per re-plan, on every terminal. There is no narrow
version of this change.

* **Moves geometry:** every model with a condemned stub — MEASURED on the Rotting Warhorse (18 at
  the production plan) and on `zombie-3-a` (52 of 92 repair trials), unmeasured elsewhere.
* **Moves candidate outcomes but not geometry:** models where landing removes a terminal from the
  coverage-repair queue, changing the repair sequence without changing the final forest.
* **Moves geometry on models that have NO condemned stub today:** only the rim-sweep stage, and
  only because it can newly condemn (stage 1b).
* **Does not move:** the area law, `STANDOFF_MM`, every SCAR constant, the minimum tip, the brace
  family, the plate family, hollowing, drilling, lowering.

### §10.2 The stages

**Stage 0 — probe P1. DONE.** `.git/land-at-strike-p1-evidence/`. Print-only, run in a throwaway
detached worktree, reverted. It produced this document's grading. **Its remaining half is P2:
run the same probe on `zombie-3-a`.** Gate G0 for P2: does the recovery split hold on a second
model, or is 38.5 % a Warhorse number?

**Stage 1 — compute the landing, record it, land nothing.** `initial.rs` computes the landing
candidate for every stub it condemns and records it on the existing `CondemnedInitialStub` row:
strike station, axial chord, **wall `W`**, entry incidence, entry face normal, crevice,
**`h_design`, `h_eff`, whether truncation bound, `a_eff`, the anchor capacity and the
capacity/demand margin**, landed length, both radii, **whether the foot ball is clipped and by
how much (E-D)**, both scar verdicts and which allowance each clears through, the build-order
verdict, and the §6.2 disclosure fields. The stub is **still quarantined**.
`BlockedCause::StubReentry` still fires.

**Stage 1 now carries three questions it did not carry at `10b0bd8`**, and they are the reason
it is worth running before anything moves: (i) does the corpus reproduce the 62 % recovery, or
is that a Warhorse number; (ii) how many corpus records sit at capacity/demand = 1.000 (§6.5);
(iii) how many have a clipped ball, which is the instance count E-D and E-A both need.

> **The production condemnation sweep stays CENTRELINE-ONLY through this stage.** This is the
> correction that makes stage 1 honest. The first version's gate asked for the emitted artifact to
> be bit-identical on every model **and** for the rim sweep to be measured inside its limits. Both
> cannot hold: the rim sweep finds more material, more material raises
> `remaining_illegal_axis_overlap`, and more stubs are condemned — so the forest, the digests and
> the artifact all move. Rim-ray intervals join axis intervals in one flat vector and the fold is
> `f64::max`, so a flank-only crossing newly condemns a stub that passes today.

**Gate G1:** (i) every corpus digest bit-identical, including `make_toed_slab`'s
`4_190_462_360_230_124_963`; (ii) the recovery table is produced for the whole corpus, not just
the Warhorse. **This stage is the whole reason the plan is staged.** It converts this document's
arithmetic into a corpus measurement at zero risk.

**Stage 1b — the rim sweep, as its own stage with its own gate.** Flip `rays_all` from `false` to
`true`. **It runs first as a side channel that decides nothing** — its intervals land on the
record beside the centreline ones and never reach `remaining_illegal_axis_overlap` — so its cost
and its refusal delta are both measured before either binds.

Three things this stage owes, none of which the first version named:

* **It is a two-site flip, not one.** The sweep is at `initial.rs:1359`. The **charge** is computed
  from a separate census at `initial.rs:751` and `:761`, which also passes `false`, twice. Flip
  only the sweep and `check_penetration_limits` refuses the batch, because the kernel re-counts
  requirements from the actual `rays_all` and compares them against limits charged at one ray per
  row.
* **The binding cap is not the one the first version named.** It named `PenetrationBatchLimits`.
  The tighter and differently-owned cap is upstream: `initial.rs:765` charges `preflight_work`
  against `ContactConstructionLimits::production()`, whose pinned literal is
  `max_work_items = 4_000_000` (`initial.rs:67`). `max_total_work` sums over rays, so nine times
  the rays is about nine times that term — today's stub sweep would have to sit under roughly
  444,000 counted traversals to survive. **That number has not been measured on any model, and
  this design does not measure it.** The failure mode is `InitialContactError::Limit` — the
  **entire model refuses at initial contact construction**, with no plan at all — and FU-59 /
  FU-8b doctrine forbids raising the cap as a response.
* **It can newly refuse a stub on a model that has no condemned stub today.** So the "this rule is
  an admission and only an admission" claim (§13.3) is true of the rule and **false of the
  bundle**. Stage 1b carries a refusal change and must be gated as one.

**Gate G1b:** the measured charge fits inside `ContactConstructionLimits::production()` on every
corpus model, with margin, unmodified; the newly-condemned population is enumerated per model with
a from→to attribution; `make_overhang_table` still emits; `make_toed_slab` is bit-identical or
attributed.

**Stage 2 — land it (geometry moves; OD-8 stop-and-report).** The stub whose landing passed the
tests is shortened to `L' = s* + h_eff / cosang_entry`, its free end is seated in the struck
surface **at the load-sized, truncated depth `h_eff`**, and it is **not** quarantined. Minted as
`MemberKind::Strut` so that `contact::seating_of` (`contact.rs:104-110`) classifies its rootward
end as `Seated` with no change to that function. **Implementation checks, not asserted here:**
confirm no lowering, emit or brace path keys on `MemberKind::Stub` in a way that a `Strut` would
miss; that no parentless-terminal path errors (§2.7(ii)); and **that the emitted foot geometry
uses `h_eff`, not `DEFAULT_CONTACT_DEPTH_MM`** — the single most likely way for R1 to be
implemented in the report and lost in the geometry.
**Gate G2:** no model that emits today stops emitting; every moved digest carries a from→to
attribution; `make_toed_slab` bit-identical or attributed; `StubReentry` reports the expected
non-zero residue on the Warhorse production plan — **not zero**, because 3 of its 18 stubs need
re-aim and 6 belong to the tip defect, and neither stage 5 nor stage 6 has landed.

**Stage 3 — the clearance-bundle extension (§9.2). PRECEDES stage 2 and is owned by #36.**
Two seat zones with per-end band radii derived from each end's own `h_eff`, and their union.
**Stage 2 cannot be accepted without it**, because a doubly-seated member with one zone is refused
by the very exemption that is supposed to admit it. §10.0 puts it in the order.

**Stage 4 — rewrite the deep-embed permission (§8.2).** A spec edit plus the matching edit in
`plinth-metrics/src/scar.rs`. **The sentence is rewritten, not deleted**: depth stays unlimited,
truncation at the opposite face is added, punch-through becomes impossible by construction. It is
doc-plus-clamp, it is behaviourally an admission on the stub path (4 records here move from
condemned to legal), and it may land before stage 2 because it depends on nothing.

**Stage 5 — re-aim, per SPEC-P8-RELOCATE §3.1 R1.** Behind stage 2, because until stage 2 lands we
do not know how many stubs still need steering. **The measurement now says 3 of 18 at the
production plan, and island 74 is no longer among them** — `10b0bd8` said 7 including island 74.
This is back to being the smaller of the two lanes and should be resourced as such.

**Stage 6 — the tip-embed defect (§7).** A separate lane, not bundled, not blocked by any of the
above. **It now needs its own test**, because the ratified detached-tip rule does not catch it.

### §10.3 Which pins move, and how

| pin | file:line | what it pins | movement |
| --- | --- | --- | --- |
| `terminalward_embed_classifies_clean_transit_and_buried_boundaries` | `initial.rs:1897` | the scalar overlap rule, including a `{0.2, 0.6}` case called "transit" | **MOVES at stage 2.** That case is a strike at `s = 0.2`, inside the designed embed, so it is a deep seat — and under R4 stage 4 **truncates** it rather than condemning it, which is a different expectation from the one `10b0bd8` recorded. The test becomes the classification pin for seat / strike / truncated seat |
| `real_stacked_obstruction_accepts_cutoff_and_condemns_above_it` | `initial.rs:2461` | the 0.05 mm cutoff against a real obstruction below a downward stub | **MOVES at stage 2, and it is the acceptance test.** The obstruction is exactly a strike. **Must be re-derived under the tests of §2.3 and its expectation re-argued, never edited.** Note that under R1 its verdict depends on what the fixture carries, so the fixture must state its load |
| `real_table_build_materializes_exact_registered_stub_geometry_and_ledger` | `initial.rs:2024` | the exact minted `p1` | **UNCHANGED** for stubs that do not strike; a landed stub needs its own sibling assertion |
| `buried_free_ends_quarantine_and_retain_both_ledger_records_and_loads` | `initial.rs:2562` | quarantine keeps the ledger row and the abandoned load | **UNCHANGED** — the residue still quarantines |
| `a_route_whose_every_trial_was_quarantined_reports_task_52s_class` | `runner.rs:10831` | `BlockedCause::StubReentry` construction | **UNCHANGED as a unit contract.** The class survives for the residue |
| `contact.rs::planar_seat_round_trip_returns_exactly_margin_from_the_kernel` | `contact.rs` tests | PR-2 against the real kernel | **UNCHANGED** — this rule does not touch the mask, and the bundle is retiring it |
| `closed_form_patch_uses_anchored_length_and_never_applies_an_area_cap` | `plinth-metrics/src/scar.rs:3426` | the auditor's own copy of the scar formulas | **MOVES at stage 4** — a spec edit must reach both implementations or engine and auditor diverge |
| `retired_formula_guard.rs` | source-text bans over `brace/cco/rewire/initial` | the flat-inset idiom | **EXTENDED** — ban any *new* arc-length inset in `initial.rs`, so this rule's exemption cannot be re-implemented as a mask |
| `ContactConstructionLimits::production()` | `initial.rs:63-70`, `max_work_items = 4_000_000` | the initial-contact work charge | **MEASURED AGAINST at stage 1b, never moved.** The rim sweep charges roughly nine times the current term against it. Doctrine forbids raising it |
| `make_toed_slab` digest `4_190_462_360_230_124_963` | three pin sites | the whole-change regression pin | **MEASURE.** Bit-identical at stage 1 by construction. At stage 1b and stage 2, bit-identical if the slab has no condemned stub, otherwise attributed |
| `make_overhang_table` | integration | emits today | **must still emit.** The 2026-08-13 acceptance bar, unchanged |

### §10.4 Revert-proof tests

Per the standing rule that a test must never re-implement production logic, and that every new
guard owes a sabotage proof:

| id | sabotage | required consequence |
| --- | --- | --- |
| **S1** | make the landing function always return `None` | a named fixture flips from LANDED back to CONDEMNED, and `StubReentry` fires. Proves the landing is what recovers it |
| **S2** | grade the wall `W` on the axial chord instead of `chord × cosang_entry` | a fixture that strikes a 0.15 mm knife-edge at a shallow angle lands. **This is the review's F1, made into a test.** R1 changed what `W` is compared against; it did not change what `W` is, so this guard survives the rulings unchanged and is still the most load-bearing one in the set |
| **S3** | size the anchor depth at `DEFAULT_CONTACT_DEPTH_MM` instead of `ball_seat_factor(1.4) × r_foot` | **at least one fixture's verdict changes in each direction**: a small-load strut in a thin wall stops landing (the depth is now too big for the wall), and the recovery count for the fixture set moves. **This is R1 made into a test**, and it is the guard against R1 being implemented in the disclosure record and forgotten in the geometry |
| **S4** | remove test (f) | island 74's `t189` shape lands with its foot **above** its contact |
| **S5** | set `rays_all = false` after stage 1b | a fixture whose axis crosses air but whose flank does not must stop being refused. **Requires a purpose-built fixture; none exists** |
| **S6** | defeat R4's truncation — let the seat take `h_design` regardless of `W` | **a deep seat perforates.** A fixture whose seat wants more depth than the model is thick must, un-sabotaged, be TRUNCATED with `exits = false` and no backside patch; sabotaged, it must produce a seat that exits the opposite face. **This is R4 made into a test, and it replaces `10b0bd8`'s S7** ("restore the deep-seat permission"), which no longer has a meaning because the permission was rewritten rather than withdrawn |
| **S7** | grade the landed foot with the *original* 2.8 mm frustum instead of `L'` | `r_x` is read at the wrong station and at least one scar verdict changes. *(This was S6 at `10b0bd8`; the ruling names the truncation case S6, so the two swapped ids. Nothing outside this document referenced either.)* |
| **S8** | give a doubly-seated member only one seat zone | every landed strut is refused. Proves §9.2's union is load-bearing rather than tidy |
| **S9** | compare the anchor capacity to the demand with an **absolute** epsilon | **46 of the 129 recovered records stop landing**, island 74 among them. Proves §6.5's zero-margin population is real and that the relative comparison is load-bearing rather than sloppy |
| **S10** | add any margin to `h_design` or `a_eff` — round up, pad, clamp to a minimum | the recovery count *rises or falls* against the recorded table. **A margin here is a silent policy change wearing the costume of a robustness fix** (§6.5) |

**S2, S3, S6 and S9 must not be skipped.** S2 is the defect that made `aae1559` unsound. S3 and
S6 are the only durable record of what R1 and R4 actually changed. S9 is the only thing standing
between §6.5's disclosure and a future "harmless" epsilon change that quietly deletes a third of
the recovery.

**Non-vacuity first, in every case.** Each sabotage test asserts the *un*-sabotaged fixture
reaches the state under test before it asserts the sabotage changes it — the shape
`cco::tests::a_grazing_model_anchor_is_refused_not_admitted_with_infinity` already uses.

---

## §11. FACTS THAT DO NOT EXIST, AND THE PROBES THAT SETTLE THEM

| gap | why it matters | minimal experiment | prize |
| --- | --- | --- | --- |
| ~~**P1 — the incidence at the strike**~~ | **DONE.** It turned the recovery table over: 120 → 80, and island 74 from recovered to refused | — | — |
| **P1b — the rim-swept crevice.** The sweep is centreline-only | The rule is not honest without it (§9.3). It could close crevices and shrink the recovery table further | The same probe with `rays_all = true`, **run beside the production sweep, not instead of it**. Measure the charge against `ContactConstructionLimits::production()` at the same time | Gate G1b. Decides whether the 80 is really 80, and whether the flip is affordable at all |
| **P2 — `zombie-3-a`'s 52.** Its 52 stub-reentry quarantines have no interval data | It is 56 % of that model's repair-trial waste, and it is the evidence that this class is not one model's quirk | Run probe P1 on `zombie-3-a`. It already has a bit-identical reference artifact to check against | Grades the second model. Extends or refutes the 38.5 % recovery rate |
| **P3 — the flush-cutter.** No tool model exists anywhere (§6.1) | Decides whether "permanent" is a real word for these struts or a conservative label | Not a print. Measure blade thickness, included angle, and the narrowest crevice the tool actually enters, on an existing printed part | Escalation **E-B**. Retires or grounds a `reach` flag that is currently prose |
| ~~**P4 — does a sub-0.3 mm crevice survive curing?**~~ | **RULED (R3), not measured.** Craig's answer: *"If resin seals anyways then the strut adds no extra resin. So this is moot."* Land, disclose, never refuse on width | — | — |
| **P6 — the truncated anchor's real separating cut (E-D).** New, and the widest open number in the document | 65 of 129 recovered records have their load-designed ball clipped by the opposite face; the two conservative brackets put recovery at 70 / 154.65 mm² or 93 / 177.09 mm² against the graded 129 / 467.07 mm². **Island 74's verdict is inside that band** | Not a print. A kernel query for the minimal separating cut of a seat truncated by a second surface — the same machinery `sphere_breach` already casts, asked a different question — or Craig's ruling that a plug spanning a wall is acceptable, which is E-A's ruling | Escalation **E-D**. Decides up to 59 records and 312 mm², and decides island 74 |
| **P5 — why 52 stub tips sit in air (§7)**, now that rule 3 does not catch them | 25 % of the population, and the ratified test that was supposed to cover it does not | Record the face normal at the contact position and compare it with the averaged `unit` the mint uses | Settles the averaged-normal cause and tells the tip lane what test it needs |
| **E-A — does the struck feature carry the load onward?** | Not measurable by probe; it is a doctrine question (§2.5) | none — it is Craig's ruling, informed by stage 1's instance count | Decides whether tasks #64 / #65 are blockers or neighbours |

---

## §12. ESCALATIONS, COLLECTED

**Items 1–4 and 14 are CLOSED by Craig's four rulings.** They are kept, struck through, with
what closed them, because an escalation that vanishes without a recorded disposition is how the
same argument gets had twice.

1. ~~**The permanence of every landed strut (§6).**~~ **CLOSED by R2: accepted as disclosed.**
   0 of 129 crevices reach `SPACING_FLOOR_MM`. The per-strut disclosure record stands; no gate.
   E-B (measure a flush cutter) stays open as a *measurement*, not as a blocker.
2. ~~**Three of ten production scars exceed the 2.0 mm² allowance.**~~ **DISSOLVED by R1.** The
   load-sized anchors are smaller: **0 of 18 production scars and 0 of 129 over the whole
   recovered population exceed `A_ABS_MM2 = 2.0 mm²`**, largest 1.478 mm² / 1.372 mm across,
   inside the 1.60 mm equivalent-diameter rationale the allowance was ratified on. **This is a
   measurement, not a bound** (§3.1, §6.4 item 1) — stage 1 must reproduce it on the corpus.
3. ~~**The sub-0.3 mm crevices.**~~ **CLOSED by R3: land, disclose, never refuse on width.**
   18 of 129, 14.9 % of the recovered area, 1 of 9 at production carrying 15.4 % of the prize
   (§6.4 item 4).
4. ~~**Withdrawing the deep-seat permission at spec scope.**~~ **SUPERSEDED by R4: the permission
   is REWRITTEN, not withdrawn** (§8.2). Depth stays unlimited; the seat truncates at the
   opposite face; punch-through is impossible by construction. It reaches all three families the
   permission names, so the scope objection is answered in full.
5. **E-C is NOT closed by the rulings alone (§8.5).** The PR-2 placed-mask exemption, with six
   live consumers, is untouched and belongs to the clearance bundle. **This is now E-C's entire
   residual scope**, because R1 also deleted the thin-feature-span check `10b0bd8` had built
   under another name.
6. **E-A remains open and the rulings add instances to it (§2.5).** Not opened by this design.
   Counted at stage 1 before it is committed. **The count went from 80 to 129, and E-D is the
   sharpest 65 of them.**
7. **`rays_all = true` is a real cost, a two-site flip, and a refusal change (§10.2 stage 1b).**
   Its binding cap is `ContactConstructionLimits::production().max_work_items = 4_000_000`, which
   doctrine forbids raising, and **the charge has never been measured**. Failure refuses the whole
   model at initial contact construction.
8. **Build order becomes a member rule for the first time (§3.6).** Two of 154 measured landings
   need it, and after the rulings they are **the only two records any test other than the anchor
   capacity refuses**. It is the third instance of an idiom the engine already applies at
   `cco.rs:4967` and `anchor.rs:1063`, but promoting it to a member rule is an owner-visible
   move.
9. **The FU-87 minimum-arm screen refuses 100 % of this design's output (§2.7(i)).** Answered in
   the first version by declaring landed struts out of scope. It is a ruling, and the screen sits
   one line below the build-order clause this design *does* adopt.
10. **A parentless terminal is treated as an error in five places (§2.7(ii)).** A landed strut has
    no parent by design.
11. **Nothing asks whether the struck surface is the same body as the contact (§3.6).** On a
    multi-part plate a landed strut would weld two minis together and every test would pass. Task
    #65's gap.
12. **The ratified detached-tip test does not catch the 52 (§7).** MEASURED, 0 of 52. The tip lane
    needs a test that does not exist.
13. **`MemberKind::Stub` → `MemberKind::Strut` moves member digests (§10.2 stage 2).** OD-8
    applies.
14. ~~**Test (a)(i), the perpendicular wall minimum, is new machinery (§2.3).**~~ **DELETED by
    R1.** There is no wall minimum any more, so there is no borrowed constant to justify and no
    thin-feature-span check built under another name. The reviewer's substantive required change
    against `10b0bd8` was that this clause be named and escalated; the ruling removed the clause
    instead. **Every test in §2.3 is now an existing predicate or an existing formula.**
15. **R1's law is applied at the foot and NOT at the contact tip (§2.8, scope).** The tip's embed
    stays `DEFAULT_CONTACT_DEPTH_MM = 0.3 mm` regardless of load, where the same law would give
    0.019 – 0.225 mm. Moving it moves `p0` for every contact in the corpus and widens the
    condemnation window at `initial.rs:1542` on models that have no condemned stub today, so it
    is a separate change with a separate blast radius. **Named so that "R1 was implemented" is
    never read as covering both ends.** On this model both readings give the same verdict for
    every affected record (§8.3).
16. **E-D — the truncated anchor's separating cut is not priced (§4.7). NEW, and the widest open
    number in the document.** 65 of 129 recovered records (314.362 mm²), 6 of 9 at the production
    plan, have their load-designed ball clipped by the opposite face, making the anchor a plug
    spanning the wall. `ball_cut_radius_mm` is a half-space formula. The joint is priced; the
    wall around the plug is not. The two conservative brackets put recovery at 70 / 154.65 mm² or
    93 / 177.09 mm², **both below `10b0bd8`'s 80 / 279.18**, and **island 74's verdict is inside
    the band.** Closing it needs a kernel query or Craig's E-A ruling — **not a constant.**

---

## §13. WHAT A REVIEWER SHOULD CHECK FIRST

1. **Re-derive test (a) end to end on island 74.** `t162` at re-plan 6: axial chord 0.462902,
   entry incidence 0.737412, so `W = 0.341350`. `r0 = r_foot = 0.587651`,
   `R_b = 1.4 × r_foot = 0.822712`, `h_design = (1.4 − sqrt(0.96)) × r_foot = 0.246933`,
   `h_eff = min(0.246933, 0.341350) = 0.246933`,
   `a_eff = sqrt(2 × 1.4 × 0.587651 × 0.246933 − 0.246933²) = 0.587651`,
   `capacity = 12 × π × 0.587651² = 13.018786` against `carried = 13.018786`. **If that does not
   reproduce, the whole table is wrong.** Then confirm `ball_seat_factor` is read from
   `contact.rs:186` and not transcribed, and that `0.4202041028867288` is what it returns at 1.4.
2. **Check that `a_eff = r_foot` is a construction and not a bug.** It is what
   `ball_seat_factor` is defined to produce (§2.8). If a reviewer sees the two numbers agree to
   six decimals and assumes a copy-paste error, they will "fix" the grading and lose 46 records.
   §6.5 exists for this reason and S9 pins it.
3. **Confirm the sweep is centreline-only.** `initial.rs:1359` passes `rays_all = false`;
   `queries.rs:90` makes `n_offset_rays` inert when it is; every one of the 1,926 probe interval
   rows carries `ray = -1`. **Unchanged by the rulings** — none of R1, R3 or R4 touches the sweep.
4. **Check that all three behavioural rulings are admissions.** `compare-r134.ts` reports
   **0 records lost** against `10b0bd8` and 49 gained, each attributed to the test that refused
   it. MEASURED, separately: of 1,500 surviving contact records, 7 have a crossing beyond the
   embed and **0** have a firm one — all seven chords are 0.0293 mm, under the 0.08 mm print
   minimum. **Then check that this is false of stage 1b**, which is the point of separating it,
   and note that stage 1b is now the *only* part of this work that can newly refuse anything.
5. **Check that the scar formulas are not doing work they cannot do.** §3.1 withdrew the closed
   form as a proof — its `T = δ` premise is gone — and replaced it with a measurement plus an
   owed re-run over `(r0, r_x, T)`. If a later text quotes §3.2's conclusions as a bound, it is
   quoting a retired argument.
6. **Re-derive E-C's instance under R4 (§8.3).** `t225`'s seat interval has `exited = true` and
   `0.849 × 0.164 = 0.139 mm` of shell. Then check that its load-sized tip anchor needs
   `ball_seat_factor(1.5) × 0.165 = 0.0630 mm` of that 0.139 mm, so it does not reach the far
   face at all — and that even at the far face its capacity, 1.865 mm², exceeds the 1.027 mm² it
   carries.
7. **Check the staging, in both directions.** Stage 1 is measure-only **only if** the production
   sweep stays centreline-only; if stage 1 moves one digit of one digest, the stage boundary is
   wrong. And §10.0: **nothing may be implemented while the wire-clearance lane owns
   `crates/`**, because stage 3 is that lane's work and stage 2 cannot be accepted without it.
8. **Check E-D honestly (§4.7).** The graded table (129 / 467.07 mm²) and both conservative
   brackets (70 / 154.65, 93 / 177.09) come from the same evidence and the same script. A
   reviewer who wants to test whether this document is arguing for its own conclusion should
   start by checking that the brackets are reported at all, and that island 74 is named as the
   record they decide.

---

## APPENDIX A — how the adversarial review's findings were folded

> **Read this appendix as history.** It records how the review of `aae1559` was folded into
> `10b0bd8`, and **every number in it is `10b0bd8`'s.** Craig's four rulings then moved most of
> them. Appendix B carries the forward view; where the two disagree, Appendix B is current.

The review of `aae1559` is at `scratchpad/land-at-strike-out/review.md`. Its verdict was UNSOUND
with the recommendation *"do not ratify this document; do not abandon the rule."* This is the fold.

| # | finding | disposition |
| --- | --- | --- |
| **F1** | Test (a) graded on the axial chord; wall thickness is `chord × cosang` | **ACCEPTED, and it decides the document.** Probe P1 measures the incidence at the interval's ENTRY — the station SPEC-EMBED-POLICY itself names for a foot-anchored interval. §2.3, §4.3. It refuses 59 of 152. Note: the review estimated with the EXIT incidence, the only one then available, and got 0.259 mm for island 74; the measured entry incidence gives 0.341 mm, so the thickness clause alone would have passed it. F2 is what refuses it |
| **F2** | The foot-ball-breach half of test (a) is cited and never run | **ACCEPTED.** P1 plumbs `sphere_breach`'s own outputs; the check is now run for real. §2.3(a)(ii). It refuses 13 of 152, including island 74 (poke 0.241 mm). The review's related point — that rule 3 leans on a half-space ball term the breaches invalidate — is folded into §3.3 |
| **F3** | Three of six tests in the classifier; the headline over-counts | **ACCEPTED.** All six are in the classifier (§4.1). The §0 headline is 80 records / 279.18 mm² and names island 74's refusal first |
| **F4** | "Landing makes the scar smaller" compares to a baseline that never prints; its arithmetic does not close | **ACCEPTED, sentence withdrawn.** §3.4. The honest baseline is zero and §0 says so |
| **F5** | (f) is a proxy for connectivity; the struck face may be a downward overhang; a different component; (d) and (f) are one gap counted twice | **ACCEPTED.** §3.6 rewritten. The overhang half is now **MEASURED** (P1 records the struck face normal): 4 of 80 recovered, 0 of 5 at production. The component half is §12 item 11. The double count is fixed in §2.5 |
| **F6** | Parentless terminals are an error in five places | **ACCEPTED.** §2.7(ii), §12 item 10 |
| **F7** | The FU-87 5.0 mm arm screen refuses 100 % of the output, answered by definition | **ACCEPTED.** §2.7(i), §12 item 9, with the honest resolution named — and with the new fact that the screen sits one line below the build-order clause this design adopts |
| **F8** | The load case changes for a doubly-seated member; "by construction" overstates | **ACCEPTED.** §2.6's overstatement withdrawn |
| **F9** | 16 struts below Craig's 0.3 mm weld line, undisclosed | **ACCEPTED and promoted to an ASK.** Re-measured on the corrected population: **14 of 80, 69.369 mm², 24.8 % of the recovered area; 1 of 5 at production carrying 27.2 % of the prize.** §0 ask 4, §6.4, per shape |
| **F10** | Declining the permission for stubs does not close E-C: not stub-scoped; the placed mask is untouched; it is a behavioural no-op | **ACCEPTED, all three.** §8.4 asks at spec scope; §8.5 names the placed mask's six consumers and hands them to #36; §8.4 states plainly that on the stub path it is a spec correction. The second implementation in `plinth-metrics/src/scar.rs` is now a §10.3 pin |
| **F11** | Re-aim CAN produce deep feet; the closure depended on an unratified R1 amendment; RULE 1/2 collides with R1/R2/R3 | **ACCEPTED.** §5.2 states the dependency and refuses to lean on it; §8.4 is rewritten so nothing depends on it. The rules are renamed: LAND-AT-STRIKE, and the fallback by its spec name (§1.3) |
| **F12** | `rays_all` is a two-site flip, refuses more, and the binding cap is `ContactConstructionLimits`, not `PenetrationBatchLimits` | **ACCEPTED.** §10.2 stage 1b names both sites, the real cap and its literal, the refusal change, and that the charge is unmeasured. The review's stronger alignment argument replaces §2.4's weaker one |
| **F13** | §2.4's fix targets a mechanism being retired; §9.2's replacement is unbuildable as written | **ACCEPTED.** §2.4 is marked superseded, its rim-ray requirement kept and promoted to §9.3. §9 is re-anchored on the landed commits, and §9.2 states the two-zone union as a **required bundle-side extension** with the exact struct and call named |
| **F14** | "45 of 52 also land once repaired" is derived under an assumption the repair invalidates | **ACCEPTED.** §7 labels it INFERRED, upper bound only, with the condition stated. Re-measured under all six tests: 34 of 52, not 45 |
| **F15** | Stage 1 cannot be both bit-identical and rim-swept | **ACCEPTED.** §10.2 splits stage 1b out with its own gate, and §0 tells Craig |
| **F16** | Three §0 numbers wrong in the ask's favour: 9 scars not 18; "each within 2.0 mm²"; "roughly a 1 mm blemish" | **ACCEPTED.** §0 ask 2 now carries the full per-strut table. On the corrected population it is **10 scars / 15.256 mm²** (the review's 18 / 23.785 was correct for the 9 struts the old grading recovered; only 5 recover now). **3 of 10 exceed 2.0 mm² and clear only via KAPPA**, and the equivalent diameters are given per scar |
| **F17** | Smaller memo defects | **ACCEPTED.** The re-aim class is split by its real reasons (§5.1); `max_depth` versus interval-start is explained once (§7); §8.2's "by construction in three places" is corrected to one construction, one 97.4 % measurement and one withdrawal; the 1.184 mm arithmetic is gone with F4; citations corrected (`queries.rs:90`, `cco.rs:4967` inside `junction_feasible` at `:4948`, comparing a junction to a stub end); test (c)'s wiring is priced as real wiring, not as "the formulas already exist" (§8.4); `MIN_ILLEGAL_LEN_MM`'s third reuse carries its derivation (§8.4); the insider terms are in the header glossary |

**One correction to the review, recorded for symmetry.** Its §5(a) and the first version both said
the engine has no Z-ordering rule anywhere. There are two — `cco.rs:4967` and `anchor.rs:1063`
(`AboveContact`, `BELOW_LEVEL_EPS_MM = 1e-6`). That makes test (f) a third instance of an existing
idiom rather than a novelty, which strengthens it, and it is what makes finding F7 sharper than
the review knew: the screen the design proposes to decline is the next clause of the same
qualification chain.

---

## APPENDIX B — the re-verdict's four required changes, and what the rulings did to them

The re-verdict of `10b0bd8` (`scratchpad/land-at-strike-out/review.md`, "RE-VERDICT — 2026-08-22")
graded the repair **SOUND-WITH-CHANGES (4)** and recommended *"ratify after change (1); the other
three are edits, not blockers."* All four were folded at `10b0bd8`. **Three of them have since
been overtaken by Craig's rulings, and the record should say so rather than leave a reader
looking for machinery that no longer exists.**

| change | as folded at `10b0bd8` | after R1–R4 |
| --- | --- | --- |
| **(1)** name test (a)(i) as new machinery, escalate it, and give it the grazing-incidence justification | §2.3 rewritten, §12 item 14 added, the 55-of-55 blind-spot analysis included | **OVERTAKEN by R1. The clause is deleted.** There is no wall minimum, no borrowed constant, and no thin-feature-span check under another name. §12 item 14 is struck through with its disposition. The grazing-incidence *outcome* survives: knife-edges are still refused, by a capacity calculation, with `W` from 0.014 to 0.122 mm across the 23 refusals |
| **(2)** add (a)(i) to §12 | added verbatim | **struck through, with why** |
| **(3)** show the `ball_breach_eps_mm` step in §8's capacity derivation | added | **OVERTAKEN by R4.** The breach check is unreachable, `ball_breach_eps_mm` is no longer read on this path, and the derivation it belonged to — "what is the largest foot that clears the breach" — has no question left to answer |
| **(4)** say that three of the six tests refuse nothing | said in §4.3 | **STILL SAID, and it is now four of five.** (b) cannot refuse anything at all under R3; (c) and (e) refuse 0 of 154; (a) decides 23 and (f) decides 2 |

**One thing the re-verdict got right that the rulings did not change.** Its §1 reproduced the
`10b0bd8` grading independently, from the raw JSONL, without importing the grader — and caught
its own `kind` filter bug in the process. **This amendment's table has been diffed record by
record against that reproduced grading** (`compare-r134.ts`): 0 lost, 49 gained, each gain
attributed to the specific test that refused it before. That is the check a reader should run
first if they doubt the 129.
