# Land the stub where it strikes the model — DESIGN

> **Provenance.** First written 2026-08-22 against `feature/a4-controller @ f50a45b` and
> committed as `aae1559`. **That version was reviewed UNSOUND and its recovery table is
> withdrawn.** This is the repair. It is re-graded on new measured evidence (probe P1,
> `.git/land-at-strike-p1-evidence/`) and re-anchored on the clearance work that has since
> landed (`d6e4310`, `33f93f4`, `1c646aa`). **DOCS ONLY. No production code and no test code
> changed. Nothing was built in the main checkout; the probe lived in a throwaway detached
> worktree that was removed after the run.**
>
> **No cap, gate, threshold, tolerance or registered constant is moved, widened or
> bypassed by anything proposed here.** Every number this design uses is already in the
> registry.
>
> **What changed between `aae1559` and this text, in one paragraph.** The old grading
> measured the struck wall along the stub's own axis and never ran the foot-ball check it
> named. Probe P1 supplies the incidence where the shaft *enters* the material and asks the
> engine's own breach machinery whether the foot fits. On those numbers the rule recovers
> **80 of 208 measured records, not 120**, and **island 74 — the island this lane was opened
> for — is refused**. The rule is still worth having. The document that priced it was not
> honest, and this one says so at the top rather than in a footnote.
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
> strut must be at least 5 mm long. *E-A, E-B, E-C, E-E* — open escalations, each named where
> it is used.

---

## §0. DECISION MEMO FOR CRAIG

### The one-line version

The engine throws away every support stub that runs into the model. Your instruction is to
stop throwing them away and stand them on what they hit. That instinct is right, and the
measurement supports it: **204 of 208 condemned stubs leave the model cleanly, cross air,
and hit something else.** But when the six tests are applied in honest units, the rule
recovers **80 of the 208 records, not 120**, and **it does not recover island 74**. Island 74's
stub would have to plant a foot ball 1.65 mm across in a wall 0.34 mm thick; the engine's own
check says that ball comes out the far side by 0.24 mm.

### The recovery table, up front, with island 74 first

**ISLAND 74 — REFUSED.** At the production plan its whole 13.019 mm² rests on one condemned
stub, terminal 162. That stub strikes the model 1.040 mm out from its contact. The material it
strikes is **0.341 mm thick measured perpendicular to the surface it enters** (the earlier
document said 0.463 mm, which was the run along the stub's own slanted axis, not a thickness).
A 0.3 mm-deep foot would fit that depth. **The foot's ball does not fit.** The load island 74
carries sizes the foot at 0.588 mm radius, which gives a designed foot ball of 0.823 mm radius
— 1.65 mm across. The engine's own `sphere_breach` measures the far wall at 0.581 mm from the
ball centre and reports **BREACHED, poke depth 0.241 mm**. The ball goes through.

*Does any foot fit?* **No — not one that can carry island 74's load.** DERIVED from the
MEASURED far-wall distance: the breach clears only if the foot ball radius drops to about
0.63 mm, which caps the foot radius at about 0.451 mm, which caps what that foot can carry at
about **10.9 mm²**. Island 74 needs 13.019 mm². **So island 74 moves to the re-aim class.** It
has to be turned to strike something thicker, or seated somewhere else. This design does not
recover it and must not be read as recovering it.

208 stub records were measured on the Rotting Warhorse across 7 re-plans (77 distinct stub
shapes). "Production plan" is the state the run settles into — 18 condemned stubs, and the
plan that would actually be printed.

| what the stub does | records | shapes | carried area | production plan | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| **Strikes, and a foot genuinely fits** | **80** (38.5 %) | 36 | 279.18 mm² | **5 stubs, 36.68 mm²** | **the rule recovers it** |
| Strikes, but no foot fits where it hit | 72 (34.6 %) | 24 | 368.02 mm² | 7 stubs, 51.88 mm² | must re-aim (§5) |
| Its own tip never reaches the model | 52 (25.0 %) | 15 | 107.50 mm² | 6 stubs, 13.05 mm² | a different defect (§7) |
| Its own seat runs deep through the model | 4 (1.9 %) | 3 | 3.42 mm² | **0 stubs** | stays condemned (§8) |
| **total** | **208** | **77** | **758.12 mm²** | **18, 101.61 mm²** | |

**Against the table you were shown on 2026-08-22.** That table claimed 120 records and
465.10 mm². Of those 120, **80 still recover and 40 do not**: 29 strike walls thinner than the
foot they would plant, 9 plant a foot ball that comes out the far side, and 2 would put their
foot above their own contact. **No record the old grading refused is recovered by the new
one** — the six-test grading is a strict tightening of the three-test one, not a re-shuffle.

**The prize is not only this model.** On `zombie-3-a`, **52 of 92** coverage-repair trials
(56 %) fail in this same class — the stub is condemned before any arm is even proposed, and
that model records **zero** clearance refusals in the brace stage. MEASURED
(`.git/mask-honesty-evidence/zombie3a-clearance-evidence.txt`). We have not run the probe on
`zombie-3-a`, so we cannot say how many of its 52 pass the foot tests. **INFERRED**: the class
matches; the split does not, and on this model the split turned out to be much worse than the
first grading claimed.

### What you must authorize — four things

**1. The rule itself.** A stub whose shaft runs into the model stops at the first strike, with
its foot standing on the struck surface, when a foot genuinely fits there. This is a support
that runs model to model. It is legal under your three-seat ruling (plate, model surface, or
another support). *This ask is unchanged from the first version and it is the sound part.*
One clause of one test decides most of that outcome: the perpendicular wall minimum — test
(a)(i), which exists nowhere in the engine until this design (§2.3) — is what turns 135
recoverable records into 80; drop it and the rule recovers 546.11 mm² instead of 279.18 mm².
**Ratifying this ask ratifies that clause.**

**2. That these bridges are permanent, priced honestly.** MEASURED: **0 of 80** recovered
struts sit in a crevice as wide as 2.5 mm, the only removal-ergonomics number the project has —
and the registry says of that number *"selected without measurement"*. Nothing in the engine
models a cutting tool, so we cannot compute whether a blade reaches them. **On the honest
reading every one of these struts stays on the model forever.**

At the production plan that is **5 struts, 4.79 mm³ of resin (0.0048 mL), and 10 new
scars — a foot scar and a tip scar each — totalling 15.256 mm².** Both scars are new: today
the stub is condemned, so it is not printed and leaves no mark at all. The honest baseline is
zero, not "a smaller scar".

| strut | foot scar | tip scar | across (foot / tip) | inside the 2.0 mm² allowance? |
| --- | ---: | ---: | --- | --- |
| t87 | 2.110 | 2.448 | 1.64 / 1.77 mm | **no — both clear only via the KAPPA branch** |
| t142 | 0.964 | 1.369 | 1.11 / 1.32 mm | yes |
| t157 | 1.936 | 2.041 | 1.57 / 1.61 mm | foot yes; **tip no — KAPPA branch** |
| t173 | 0.917 | 0.167 | 1.08 / 0.46 mm | yes |
| t215 | 1.553 | 1.750 | 1.41 / 1.49 mm | yes |
| **total** | **7.480** | **7.775** | **15.256 mm² over 10 scars** | **3 of 10 exceed 2.0 mm²** |

The scar rule has two allowances and takes the larger: a flat `A_ABS_MM2 = 2.0 mm²`, and
`KAPPA × the designed ball area`, which grows with the radius. **Three of these ten scars are
above the 2.0 mm² you ratified and are legal only because the second allowance is larger for a
fat foot.** You ratified 2.0 mm² with the reason *"caps the scar at 1.60 mm equivalent
diameter… inside the buff/fill/paint class"*. Two of these are 1.77 mm and 1.61 mm across.
**You price this.** The design reports it. It does not gate on it.

**3. Decline a permission you already granted — and understand what declining does.**
SPEC-EMBED-POLICY says a designed embed end may run as deep as it likes into the model. That
permission was written on the condition that a separate "thin-feature-span check" catches
shells being perforated. **That check does not exist in this engine.** That is escalation E-C,
and it has blocked this work since 2026-08-21.

Two corrections to how this was put to you the first time:

* **On the stub path, declining changes nothing at runtime.** `initial.rs:1542`'s
  0.05 mm cutoff already condemns exactly the population the proposed rule would condemn.
  This is a **spec correction**, not a tightening. It is still worth doing — a spec that grants
  a permission its guard was never built for is a trap for the next implementer — but it should
  be sold as what it is.
* **Declining it for stubs alone does not close E-C.** The permission's own scope line names
  three families: *"contact-stub tips, strut/rung feet, model-rung ends"*. Declining it for one
  answers a third of one half of E-C's question. **The ask is therefore to decline it at spec
  scope, for all three families.** Measured cost on this model: 4 records, 3.42 mm², **zero at
  the production plan**, and two of those four are already illegal under the existing
  scar-dimension rule. The true cost is one shape, 1.03 mm², and that shape is a support
  through a 0.139 mm shell, which we should not want.

* **The thin-wall guard E-C wanted already exists — for landed feet.** Test (a)(i) (§2.3) is
  that check, built here under another name: it refuses 59 of the 152 landing candidates on wall
  thickness alone. E-C's residual scope after this design is not stub tips — it is the *other*
  consumers of the no-depth-cap permission, above all the PR-2 placed-mask exemption's six
  consumers, already handed to the bundle lane below.

  **Out of scope here, named so it is not lost:** this does not touch the PR-2 placed-mask
  exemption that arms and braces use, which is larger, has no depth cap at all, and has six
  live consumers. That belongs to the clearance bundle lane (§8.5).

**4. NEW ASK — rule on the sub-0.3 mm crevices.** Your own field line says gaps below 0.3 mm
may weld shut in the cured part. MEASURED: **14 of the 80 recovered records sit in crevices
narrower than that** — 69.37 mm², **24.8 % of everything the rule recovers** — and at the
production plan it is **1 of the 5**, t157, crevice 0.174 mm, carrying 9.974 mm², **27.2 % of
the production prize**. Five distinct shapes, at 0.116, 0.117, 0.174, 0.180 and 0.196 mm.

If those crevices weld shut, then for those struts the welded crevice is what holds the island
up, the strut would have been unnecessary, **and it is still permanent and still leaves two
scars.** Cost with no benefit. The project has a ratified print minimum for solid material
(`PRINT_FLOOR_CHORD_MM = 0.08 mm`, *"2.6-4.1 px foliage knife-edges that cannot print as
coherent material"*) and this design uses it. It has **no** print minimum for a void, so it has
no basis for claiming a 0.116 mm crevice exists in the cured part at all.

**Refuse there, or report and land anyway?** This design does not decide it. You are the only
person with the field data that answers it.

### What you are NOT being asked to authorize

* **No constant moves.** Not one. Everything used is registered.
* **No new constant is introduced.**
* **The load-path question stays open, and this design does not widen it.** When a strut
  stands on the model, the model carries the load onward. Whether that is safe is escalation
  E-A, opened 2026-08-21 by the model-seated-arm work that already landed — not by this
  design. This rule adds instances to an exposure that is already live.
* **Nothing is wired to the removal apparatus.** The six `GRIP_*` constants stay dead.

### What it costs if you say yes

The change is in initial contact minting, so **every model in the corpus is affected**. The
staged plan (§10) puts a **measure-only stage first**: the landing is computed and recorded for
every condemned stub on every corpus model, and nothing moves. You see the real recovery table
for the whole corpus before one triangle of geometry changes.

One correction to the first version's staging. It asked for stage 1 to be **bit-identical on
every model** *and* to sweep the stub's whole surface rather than just its centreline. Those two
cannot both hold: sweeping more of the surface finds more material, more material condemns more
stubs, and the output moves. **The production sweep stays centreline-only through stage 1**, and
the wider sweep runs beside it as a report that decides nothing. Turning the wider sweep on for
real is its own stage with its own gate, because it is the one part of this work that can newly
refuse a stub **on a model that has no condemned stub today** — and its cost has never been
measured against the cap it would charge (§10.2, stage 1b).

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
| island 74 becomes routable by scar arithmetic | it does not become routable at all — the foot ball breaches (§0) |

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
> the first strike**, with its foot seated on the struck surface, **when all six tests of §2.3
> pass**. The result is a support that runs model to model.
>
> **Fallback.** When the strike point fails a foot test the stub re-aims, by the
> owner-ratified cone search in `docs/engineering-journal/specs/SPEC-P8-RELOCATE.md` §3.1
> **R1**.

**A note on naming, and why the first version's names were withdrawn.** That version called
these "RULE 1" and "RULE 2". `SPEC-P8-RELOCATE` already has a ratified ladder called
R1 → R2 → R3, in which **R1 *is* the re-aim** and R2 is a contact re-seat this design never
mentions. Anyone tracking E-C across the two documents would have mis-mapped them. The rules
here are therefore named, not numbered, and the fallback is referred to by its spec name.

Landing is cheaper than re-aiming in every sense where it applies: it preserves the contact
point exactly, it preserves the contact direction exactly, it shortens the member, it needs no
candidate search, and it needs no re-route. **On the corrected grading it applies to fewer than
half of the population**, which makes the fallback the larger job, not the smaller one (§5).

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
* **landed length** `L' = s* + δ` — the strut runs from the island contact to a foot seated
  `δ` deep in the struck surface.
* **radii.** `r_tip = r0` (load-law sized, §2.6) and `r_foot = max(r0, MIN_SHOOT_R_MM)` — the
  identical rule `initial.rs:655-657` already applies, over the shorter span. The frustum is
  `r(s) = r_tip + (r_foot − r_tip)·s/L'`, growing from tip to foot.
* everything beyond `s = s* + δ` is **cut away and does not exist**. Crossings out there
  vanish with it.

### §2.2 Print-minimum moot strikes are ridden through, not landed on

A crossing whose chord is below `PRINT_FLOOR_CHORD_MM = 0.08 mm` is **MOOT** by the
owner-ratified 2026-07-24 amendment: *"chords < 0.08 mm are 2.6-4.1 px foliage knife-edges that
cannot print as coherent material."* Material that cannot print cannot be stood on. So a moot
crossing is **not** a strike; the shaft rides through it, and it is still counted and reported
in the moot bucket the amendment requires.

**The mirror question, asked here because §0 ask 4 depends on it.** The project has that
minimum for solid and none for air. At 2.6-4.1 px per 0.08 mm, a 0.116 mm crevice is four to
six pixels of air between two curing walls. This design uses the solid minimum and **discloses**
that it has no equivalent for the void (§6.4).

### §2.3 The six tests, in honest units

A strike becomes a foot when **all six** hold. Five of the six are an existing predicate or an
existing formula — clause (a)(i) is not.

**Test (a)(i) is new machinery, not a citation.** Nothing in the engine or in SPEC-EMBED-POLICY
compares a perpendicular wall thickness to anything: `sphere_breach` returns
`nearest_wall_thickness_mm`, but before this design no caller reads it, and SPEC-EMBED-POLICY's
rules 1-4 contain no wall minimum. **(a)(i) is the thin-feature-span check escalation E-C names,
built here for landed feet under another name.** Its derivation is direct, not borrowed: the foot
embeds `DEFAULT_CONTACT_DEPTH_MM = 0.3 mm` perpendicular into the struck face, so the face must
itself hold at least 0.3 mm of perpendicular material for the foot to seat inside it rather than
pass through it. The constant is reused in its own native meaning — an embed depth demanding its
own depth of material to embed into — not repurposed as a new number.

| # | test | statement | how it is measured |
| --- | --- | --- | --- |
| **(a)** | **a foot genuinely fits the struck material** | **two clauses, both required.** (i) the **perpendicular** wall thickness at the strike is at least `DEFAULT_CONTACT_DEPTH_MM` (0.3 mm); (ii) the foot ball is not breached | (i) is `chord × cosang_entry`, with `cosang_entry` **MEASURED by P1** at the interval's entry face — which is exactly the crossing SPEC-EMBED-POLICY names for a foot-anchored interval. (ii) is `NativeSurface::sphere_breach` at the foot endpoint with `ball_factor_foot = 1.4`, the same call `anchor.rs:1607-1630` makes and the same refusal `AnchorFootRefusalReason::FootBallBreached` |
| **(b)** | **the stub really left the model** | crevice `≥ ContactPolicy::min_free_span_mm` (0.05 mm, sourced from `MIN_ILLEGAL_LEN_MM`) | measured directly on the interval sweep — see §2.4 |
| **(c)** | **scar pricing at both ends** | SPEC-EMBED-POLICY rules 1-3 at the foot **and** at the contact tip | `transit_measurement::calculate_scar_diagnostic` (`transit_measurement.rs:301-368`). Graded here on the MEASURED incidence and the MEASURED distance `d`, not on a worst case |
| **(d)** | **the foot interface carries the island's load** | `contact::area_sufficient(carried_mm2, r_eff, policy)` with `r_eff = ball_cut_radius_mm(r_foot, δ, 1.4)` | `contact.rs:442`, already called inside `contact::admit`. See §2.5 for the half that does **not** exist |
| **(e)** | **the strut's own narrowest section carries the load** | the area law at the tip | `FrozenTipLoadPolicy` (`perception.rs:636-644`) already sized `r0` from `carried_mm2` at ratio 12. See §2.6 |
| **(f)** | **build order** | the foot must be **below** the contact in Z | the same sense `anchor.rs:1063` already applies to a model-anchor seat, and `cco.rs:4967` to a routing junction — see §3.6 |

**Why test (a) is stated in two clauses and both are run.** The first version stated both and
graded only the first, and graded that one on the run of the axis inside the material rather
than on a thickness. Those are different numbers whenever the shaft is not perpendicular, and
they differ by `1/cos`. On this population the median entry incidence is 0.592, so the axial
run overstates the wall by about 70 % at the middle of the distribution. The correction is
decisive:

```
MEASURED over the 152 landing candidates
  axial chord >= 0.3 mm                     122 / 152
  PERPENDICULAR wall >= 0.3 mm               93 / 152     -> (a)(i) refuses 59, 278.281 mm2
  foot ball not breached                    139 / 152     -> (a)(ii) refuses 13,  94.450 mm2
  perpendicular wall  min 0.014  p25 0.136  med 0.365  p75 0.863  max 1.891 mm
  entry incidence     min 0.062  p25 0.383  med 0.592  p75 0.714  max 0.997
```

**Clause (ii) is not a refinement of clause (i); it is a different question.** Clause (i) asks
whether the material is deep enough to seat 0.3 mm into. Clause (ii) asks whether the *ball* on
the end of the foot comes out the other side, in **any** direction within 25° of the inward
normal — which is not the stub's direction. Island 74 passes (i) at 0.341 mm and fails (ii)
with a 0.241 mm poke. Nine of the thirteen breaches pass (i).

**Why (a)(i) is not redundant with the engine's own ball check.** Of the 55 records that (a)(i)
alone refuses, `sphere_breach` returns **no finite wall at all on 49 of the 55** (0.61 mm on the
other 6) — the breach cone, cast narrow from the ball centre, misses a knife-edge struck at
grazing incidence. Those 55 records have entry cosang median 0.352 (min 0.062) and perpendicular
wall median 0.094 mm (min 0.014 mm). The ball check catches a fat foot breaching a nominal wall
(island 74); the thickness clause catches a foot on a knife-edge the cone cannot see. The two
clauses cover different blind spots; both are needed.

**A caution about `nearest_wall_thickness_mm`.** The kernel measures it from the **ball centre**
to the far wall along that cone, not from the near face and not along the stub axis, and it is
**infinite** when no far wall lies within the ball's reach. It is the right number for the
breach question and the wrong number for "how thick is this wall". This design uses each for
what it measures and never swaps them.

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
`0 ≤ seated_length − 0.05`, which every landed strut passes — the shortest is 0.716 mm. The
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
law, and `contact::area_sufficient` (`contact.rs:442`) is exactly it. It is already called
inside `contact::admit` for every other contact. **MEASURED: it refuses 0 of 152.** DERIVED, and
worth recording because it explains why: for a foot at depth `δ`, `r_eff² = 2·1.4·r_foot·δ − δ²`,
and with `r_foot = max(r0, 0.35)` the test holds for every `r0` up to about 0.714 mm. The
largest `r0` in this population is 0.588. The test would bite on a fatter tip, and it is not
vacuous — it simply does not bind here.

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
come out of the same law, so the test holds to rounding: **it refuses 0 of 152.** (An earlier
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

---

## §3. THE SCAR BOUND — what it buys, and what it does not decide

### §3.1 The claim

> **DERIVED.** A foot seated exactly `δ = DEFAULT_CONTACT_DEPTH_MM = 0.3 mm` into the struck
> surface, on a member whose foot radius is at least `MIN_SHOOT_R_MM = 0.35 mm`, satisfies
> SPEC-EMBED-POLICY rules 1, 2 and 3 **at every incidence angle and at every radius in the
> registered range**. The same holds at the contact tip.

**MEASURED confirmation, now on real incidences rather than a worst case: test (c) refuses 0 of
152.** The bound holds and the sweep that proved it was not optimistic.

**But its role in this design is smaller than the first version claimed**, and that matters.
§3.4 says what it is.

### §3.2 The proof

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
surface, which the real parameterization forbids. Evaluated over the real 120-record
parameterization the minimum ratio is 1.2105. An implementation **must** compare with `>=`, as
`recorded_constraints` already does, and **must** derive `r_x` from the landed frustum rather
than the original 2.8 mm one.

**The second caveat is the load-bearing one.** `A_ball = π(R_b² − d²)` is a **half-space
formula**. It credits a spherical cap buried in one surface with no far side at all. Over the
recovered population that term supplies most of the anchor area, so rule 3 leans on a formula
that assumes the ball does not come out the other side. **That assumption is exactly what test
(a)(ii) checks, and test (a)(ii) refuses 13 of 152 records.** The theorem is correct arithmetic;
it is only applicable where the breach check has already passed. **Rule 3 must never be read as
evidence that a foot fits. That is the breach check's job, and it is now run.**

### §3.4 What the bound does and does not buy

It buys: test (c) is not a binding test, so the recovery rate is decided by geometry — does the
stub strike thick enough material, at a good enough angle, in the right direction? — rather than
by scar arithmetic. That is the right shape.

It does not buy: **any claim that the landed struts are cheap.** The first version drew that
inference and it does not follow. The bound proves each scar is *legal*. It says nothing about
how large it is, and §0's table shows three of ten scars above the flat 2.0 mm² allowance,
legal only through the radius-scaled second allowance.

It does not buy: **any claim about the aggregate scar.** The bound bounds one scar. Five struts
make ten scars, and each pair sits within a fraction of a millimetre of the other on either side
of a crevice. **Nothing in the registry bounds a scar population**, and this design does not
invent one — it discloses (§6).

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

**MEASURED, over the 152 landing candidates:**

```
foot BELOW contact : 150 / 152
foot ABOVE contact :   2 / 152
dz (foot_z - contact_z): min -2.713  p25 -1.831  median -1.372  max +0.441 mm
over the 80 RECOVERED: dz min -2.023  max -0.520 mm   (no near-degenerate cases)
```

Both hangers are island 74's small sub-patch, terminals 189 (re-plan 0) and 167 (re-plan 1),
`dz = +0.441 mm`, `carried = 3.288 mm²` each. **They must be refused, and test (f) is what
refuses them.**

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
   struck face's outward normal, so this is now MEASURED**: of the 80 recovered records, **4
   land on downward-facing material** (`normal_z < 0`), carrying 3.120 mm²; the struck-face
   normal z runs min −0.130, p25 0.418, median 0.638, max 0.984. **At the production plan it is
   0 of 5.** So on this model the concern is real but small. It is reported, not gated: whether a
   downward-facing struck face is disqualifying is a question about the *receiving* feature's own
   support, which is E-A's territory, and inventing a gate here would pre-empt Craig's ruling.
2. **Whether the foot lands on a different connected component.** Nothing in the six tests asks
   whether the struck surface belongs to the same body as the contact. `NativeSurface` is the
   whole scene. On a multi-part plate — the stated goal of this project — a landed strut would
   weld two separate minis together and every test in §2.3 would pass. **P1 cannot see this**;
   face ids do not carry component identity. This is task **#65**'s gap and it is escalation §12
   item 11.
3. **Connectivity in general** — the same gap as (d)'s open half, per §2.5. Counted once.

---

## §4. THE 208-RECORD RE-GRADING

### §4.1 Method

`.git/land-at-strike-p1-evidence/regrade.ts`, run against that directory's raw JSONL. For each
of the 208 condemned records: sort its intervals; find the seat interval (start within
`TOL_MM = 0.05` of `p0`); classify the record by what its **seat** does; then, for the records
whose seat is sound, find the first firm strike beyond `δ` (skipping print-minimum moot
crossings) and apply **all six** tests.

Three method corrections against the first version:

* **All six tests are in the classifier.** The first version's classifier was
  `ok = tA && tB && tC`, with (d), (e) and (f) reported separately in prose. Its headline table
  therefore counted two records that its own §4.5 said were refused.
* **A stub whose own tip never reaches the model, or whose own seat runs deep, is not a landing
  candidate whatever its shaft does further out.** The first version's classifier and this one
  agree on that; it is restated because a mid-repair version of this re-grade briefly let a
  strike re-classify such a record, and the resulting partition was wrong. Recorded so the error
  is not repeated.
* **Every constant is read at its registered value and none is moved.** The engine's implied area
  ratio is recovered from the data (12.000000) rather than retyped.

### §4.2 The corrected recovery table

| class | records | shapes | carried mm² | share |
| --- | ---: | ---: | ---: | ---: |
| **LANDS — all six tests pass** | **80** | 36 | **279.176** | 38.5 % |
| **STRIKES BUT MUST RE-AIM** | 72 | 24 | 368.023 | 34.6 % |
| **TIP NEVER EMBEDS — a different defect** | 52 | 15 | 107.498 | 25.0 % |
| **SEAT RUNS DEEP — stays condemned** | 4 | 3 | 3.421 | 1.9 % |
| **total** | **208** | **77** | **758.118** | |

A *shape* here is one (carried area, condemning overlap) pair, defined once over the whole
population so the per-class counts sum to the total. The first version reported 78 by summing
per-class sets; the population has 77.

### §4.3 Why the strikers fail

```
per-test refusals over the 152 landing candidates
  (a)(i)  perpendicular wall < 0.3 mm      59   278.281 mm2
  (a)(ii) foot ball breaches               13    94.450 mm2
  (b)     crevice < 0.05 mm                 2     0.065 mm2
  (c)     scar rules, either end            0     0.000 mm2
  (d)     foot interface area law           0     0.000 mm2
  (e)     tip section area law              0     0.000 mm2
  (f)     foot above its own contact        2     6.576 mm2

first-and-only failure, by combination
  wall only                    55   266.932 mm2
  breach only                   9    83.100 mm2
  wall AND breach               4    11.349 mm2
  crevice only                  2     0.065 mm2
  build order only              2     6.576 mm2
```

**Read that as the design's own summary of itself.** The rule is decided almost entirely by
test (a) — whether the struck material is thick enough, and whether the foot's ball fits inside
it. The scar rules, both area laws and build order together refuse 4 records. The first version
graded on the three tests that refuse nothing and omitted the one that refuses everything.

**Say it plainly: tests (c), (d) and (e) each refuse 0 of the 152 landing candidates.** Test (a)
decides 68 of the 72 re-aims on its own (55 wall-only, 9 breach-only, 4 both); (b) and (f) decide
2 each. The scar rules, the foot area law and the tip area law decide nothing on this model.

### §4.4 Per re-plan

| re-plan | condemned | lands | re-aim | tip never embeds | seat runs deep | carried | landed carried |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 59 | 28 | 18 | 11 | 2 | 128.07 | 49.06 |
| 1 | 57 | 26 | 18 | 11 | 2 | 120.41 | 46.31 |
| 2 | 18 | 5 | 7 | 6 | 0 | 101.61 | 36.68 |
| 3 | 18 | 5 | 7 | 6 | 0 | 101.61 | 36.68 |
| 4 | 19 | 5 | 8 | 6 | 0 | 102.77 | 36.68 |
| 5 | 19 | 6 | 7 | 6 | 0 | 102.03 | 37.09 |
| **6 (production plan)** | **18** | **5** | **7** | **6** | **0** | **101.61** | **36.68** |

The production plan is stable across re-plans 2-6. The seat-runs-deep class **vanishes** once the
controller re-plans: it exists only in the first two.

### §4.5 The production plan, stub by stub

```
LANDS  (5 stubs, 36.678 mm2)
  t87  isl 69  carried 11.879  wall 0.895  cos_in 0.978  crevice 1.585  L' 2.185  no breach
  t142 isl 70  carried  5.984  wall 1.887  cos_in 0.907  crevice 0.419  L' 1.019  no breach
  t157 isl 71  carried  9.974  wall 0.302  cos_in 0.592  crevice 0.174  L' 0.774  no breach
  t173 isl  0  carried  0.825  wall 0.701  cos_in 0.617  crevice 1.364  L' 1.964  no breach
  t215 isl 48  carried  8.016  wall 0.374  cos_in 0.625  crevice 1.004  L' 1.604  no breach

MUST RE-AIM  (7 stubs, 51.882 mm2)
  t100 isl  8  carried  0.715  wall 0.094   too thin
  t154 isl 66  carried 12.661  wall 0.037   too thin
  t162 isl 74  carried 13.019  wall 0.341   FOOT BALL BREACHES, poke 0.241   <- ISLAND 74
  t186 isl  0  carried  6.306  wall 0.177   too thin
  t229 isl 65  carried  6.800  wall 0.014   too thin
  t231 isl  3  carried  4.453  wall 0.049   too thin
  t281 isl 68  carried  7.929  wall 0.210   too thin

TIP NEVER EMBEDS  (6 stubs, 13.053 mm2) — see §7
  t115 isl 47  1.962     t256 isl  7  1.419     t273 isl 51  3.258
  t275 isl 72  3.434     t277 isl 51  2.742     t284 isl  7  0.238
```

Note t157: it lands, and its struck wall is **0.302 mm** — 2 µm above the requirement — while
its crevice is **0.174 mm**, inside Craig's weld line. It carries 9.974 mm², 27.2 % of the
production prize. It is the single most marginal recovered strut in the plan and it is the one
§0 ask 4 is really about.

### §4.6 Island 74, in full

Nine condemned records touch island 74 across the seven re-plans. **None of them lands.** Two
distinct shapes:

| shape | re-plans | carried | strike | axial chord | cos at entry | **wall** | foot ball `R_b` | engine far wall | **poke** | verdict |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| t184 / t162 | 0-6 | 8.996 → **13.019** | 1.0398 | 0.4629 | 0.7374 | **0.3414** | 0.6839 → **0.8227** | 0.5815 | 0.102 → **0.241** | **BREACHED — refused by (a)(ii)** |
| t189 / t167 | 0-1 only | 3.288 | 0.4550 | 0.7386 | 0.9632 | 0.7113 | 0.4900 | 0.4429 | 0.000 | **foot ABOVE contact — refused by (f)** |

The scar rules **pass** on the production shape: `A_patch 2.460 / A_max 2.658` (through the KAPPA
branch), `L_scar 1.475 / 2.500`, `A_anchor 3.192 ≥ neck 1.085`. Build order passes,
`dz = −1.339`. Test (d) and test (e) pass. **It is refused by one test, and that test is the one
the first version did not run.**

**Answer to the question that started this lane: no, not by landing.** Island 74's stub strikes
material 0.341 mm thick, and the foot the load requires puts a 1.65 mm ball into it that comes
out the far side by 0.241 mm. DERIVED from the MEASURED far-wall distance of 0.5815 mm, plus the
registered breach epsilon `ball_breach_eps_mm = 0.05` (`placeholders.rs:991`): the breach clears
only for a foot ball radius `R_b ≤ 0.5815 + 0.05 = 0.6315 mm`, which caps the foot radius at
`r_foot ≤ 0.451 mm`, which gives a ball-cut radius of `0.5375 mm` and a capacity of about
**10.888 mm²**. Island 74 needs 13.019 mm². **No foot that can carry the island fits in the wall
the stub hit. Island 74 must be re-aimed or re-seated, and that is SPEC-P8-RELOCATE's job, not
this rule's.**

### §4.7 Spreads over the recovered population

```
crevice                    min 0.116  p25 0.419  median 0.815  p75 1.270  max 1.807 mm
landed length L'           min 0.716  p25 1.019  median 1.415  p75 1.870  max 2.407 mm
PERPENDICULAR struck wall  min 0.302  p25 0.399  median 0.807  p75 1.069  max 1.891 mm
axial chord                min 0.510  p25 0.857  median 1.184  p75 1.814  max 2.383 mm
entry incidence cosang     min 0.204  p25 0.591  median 0.625  p75 0.907  max 0.997
foot scar A_patch          min 0.642  p25 0.795  median 0.884  p75 1.553  max 2.110 mm2
tip  scar A_patch          min 0.033  p25 0.101  median 0.167  p75 1.625  max 2.448 mm2
carried area               min 0.002  p25 0.413  median 0.915  p75 7.940  max 11.879 mm2

crevice >= SPACING_FLOOR_MM (2.5 mm)      :   0 / 80
crevice <  0.30 mm (Craig's weld line)    :  14 / 80   69.369 mm2
records carrying a scar above 2.0 mm2     :  12 / 80
records landing on downward-facing material:  4 / 80    3.120 mm2
```

---

## §5. THE RE-AIM FALLBACK, AND WHY IT IS NOW THE LARGER JOB

### §5.1 What is left for re-aim

**72 records, 24 distinct shapes, 368.023 mm² carried; 7 stubs and 51.882 mm² at the production
plan — including island 74.** They divide by first-and-only failure as follows, and not one of
them is a scar failure:

| reason | records | carried | reading |
| --- | ---: | ---: | --- |
| the struck material is thinner than a standard foot | 55 | 266.932 | firm material, above the 0.08 mm print minimum, but under the 0.3 mm foot depth: foliage knife-edges |
| the foot ball would come out the far side | 9 | 83.100 | the depth fits; the ball does not. **Island 74 is here** |
| both of the above | 4 | 11.349 | |
| the crevice is under 0.05 mm | 2 | 0.065 | the stub grazes tangentially, emerges for 46 µm, then plunges into thick solid |
| the foot would sit above its own contact | 2 | 6.576 | the hangers of §3.6 |

**This is a clean, physical partition**, and it is what the first version got wrong twice over:
it put all 32 of its re-aim class in the "too thin" row when 2 were the tangential graze, and it
had 40 records in the wrong class entirely.

**Re-aim now carries more of the load than landing does** — 368 mm² against 279 mm², and 7 of 18
production stubs against 5. The first version projected 3 of 18. That reverses the two lanes'
priority and §10 restages accordingly.

### §5.2 The dependency on SPEC-P8-RELOCATE R1, stated honestly

R1 is adopted unchanged in its search, its pinned enumerations (`P8_REAIM_POLAR_DEG =
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

**Four records, three distinct shapes, 3.421 mm², zero at the production plan.** These are the
deep seats of §8. After landing and re-aim, and setting aside the tip defect of §7, the honestly
condemned residue on the Rotting Warhorse is **empty at the production plan** — but only if
re-aim succeeds on all seven of its stubs, which is not measured and must not be assumed.

The `BlockedCause::StubReentry` class (`coverage_closure.rs:375-381`) must therefore **survive**
— it stays the right name for the residue.

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
| `crevice_width_mm` | `s* − δ` | MEASURED, the interval sweep |
| `strut_length_mm` | `L' = s* + δ` | DERIVED |
| `strut_r_tip_mm`, `strut_r_foot_mm` | `r0`, `max(r0, MIN_SHOOT_R_MM)` | DERIVED |
| `struck_wall_perpendicular_mm` | `chord × cosang_entry` | MEASURED |
| `struck_face_normal_z` | the struck face's outward normal, Z component | MEASURED, P1 |
| `foot_ball_radius_mm` | `BALL_FACTOR_FOOT · r_foot` | registered, 1.4 |
| `foot_ball_breached`, `foot_poke_depth_mm` | the engine's own verdict | MEASURED, plumbed |
| `strut_volume_mm3`, `in_crevice_volume_mm3` | frustum volumes | DERIVED |
| `tip_scar_mm2`, `foot_scar_mm2`, and which allowance each clears through | §3's formulas on measured inputs | DERIVED |
| `reach` | `Unknown` if `crevice_width_mm ≥ SPACING_FLOOR_MM`, else `PermanentUnlessMeasured` | **the only registered removal-ergonomics distance, used for disclosure only** |

**`reach` never gates anything.** It is a label on a report. The two values are deliberately
named so that neither reads as a clean bill of health: even the wide case is `Unknown`, because
2.5 mm is a contact-spacing number, not a tool-reach number.

### §6.3 What the measurement says

**MEASURED, production plan (5 landed struts):**

```
 t87  crevice 1.585  L' 2.185  r 0.561 -> 0.561  wall 0.895  foot scar 2.110  tip scar 2.448
 t142 crevice 0.419  L' 1.019  r 0.398 -> 0.398  wall 1.887  foot scar 0.964  tip scar 1.369
 t157 crevice 0.174  L' 0.774  r 0.514 -> 0.514  wall 0.302  foot scar 1.936  tip scar 2.041
 t173 crevice 1.364  L' 1.964  r 0.148 -> 0.350  wall 0.701  foot scar 0.917  tip scar 0.167
 t215 crevice 1.004  L' 1.604  r 0.461 -> 0.461  wall 0.374  foot scar 1.553  tip scar 1.750

 total resin           4.790 mm3 = 0.0048 mL
 new model scars       10 scars (a foot and a tip per strut), 15.256 mm2
 above A_ABS_MM2 = 2.0 3 of 10, clearing only through the KAPPA allowance
 reach                 5 / 5  PermanentUnlessMeasured
```

Over the whole 80-record recovered population: **0 of 80** crevices reach 2.5 mm. The widest is
1.807 mm. **On the honest reading every landed strut on this model is permanent.**

### §6.4 The four things a reader should not conclude

1. **Not "the scars are fine because each is under 2.0 mm²".** Three of ten are not. Those clear
   only through `KAPPA × A_design`, which grows with the foot radius, and the 2.0 mm² allowance
   was ratified for *one* scar with the reason *"caps the scar at 1.60 mm equivalent
   diameter… inside the buff/fill/paint class"*. t87's tip scar is 1.77 mm across.
2. **Not "0.0048 mL is negligible".** The resin volume is negligible. The visual result of five
   permanent bridges across five crevices on a mini is a different question, and it is not a
   volume question. And **nothing registered bounds a scar population.**
3. **Not "we can measure our way out of this cheaply".** The flush-cutter measurement is
   escalation **E-B**, still open.
4. **Not "the crevice is definitely there".** 14 of 80 recovered records sit in crevices below
   Craig's 0.3 mm field line, 69.369 mm² — 24.8 % of the recovered area — and 1 of 5 at the
   production plan carrying 27.2 % of the prize. The project has a ratified print minimum for
   solid and none for air, so it has no basis for claiming a 0.116 mm crevice survives curing. If
   it welds shut, the weld holds the island up, the strut was unnecessary, **and the strut is
   still permanent and still leaves two scars.** That is §0 ask 4 and this design does not decide
   it. Per strut, the sub-0.3 mm cases are:

   | shape | crevice | struck wall | carried | present at production? |
   | --- | ---: | ---: | ---: | --- |
   | t82 / t73 (island 0) | 0.1156 | 1.281 | 2.589 / 0.256 | no |
   | t156 / t136 (island 25) | 0.1173 | 1.463 | 0.084 | no |
   | t178 / t157 (islands 70, 71) | 0.1741 | 0.302 | 7.940 → **9.974** | **yes — t157** |
   | t117 (island 16) | 0.1798 | 1.891 | 0.340 | no |
   | t119 / t105 (islands 19, 17) | 0.1963 | 1.120 | 0.132 | no |

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
* **The prize is real but it is an upper bound, not a number.** 34 of the 52 (77.885 mm²) also
  pass all six tests *where their shaft currently is*. The first version reported the analogous
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

## §8. E-C DISPOSITION — decline the permission, at spec scope, and say what declining does

### §8.1 The question, restated

SPEC-EMBED-POLICY says: *"There is NO depth cap on the tip-anchored interval itself: a deep
perpendicular plug into thick solid has a small crossing patch and, per the owner's idempotence
ruling, no further physical consequence; thin walls are protected by rule 4 and the
thin-feature-span check."*

**The thin-feature-span check does not exist in this engine.** Escalation **E-C**
(`model-seated-arms-2026-08-21-amendment.md` §B3) constructs the regression: a stub through a
2.0 mm hollow shell at 72° passes rules 1-3, is anchored so rule 4 is moot, and a **6.7 mm shell
perforation becomes legal** where today's 0.05 mm cutoff refuses it.

### §8.2 Is the deep-embed grading exercised at all after the rule lands?

**Almost never, and here is the honest accounting of "almost".**

1. **A landed foot has `T = δ = 0.3 mm` exactly.** The landing rule pins it: the member ends at
   `s* + δ`, below `s*` it is in air, so the solid interval anchored to the foot endpoint runs
   exactly δ. **DERIVED, not assumed.**
2. **The contact tip has `T = δ` too**, whenever the seat interval ends at the nominal surface —
   which is MEASURED true for 152 of the 156 records that have a seat at all. **That is 97.4 % of
   one model, which is a measurement, not a construction.** It becomes a construction after
   stage 4 (§10.2), which is the right thing to say and the first version did not say it.
3. **Re-aim candidates are NOT landed** — see §5.2. The first version's third "by construction"
   was an unratified amendment to SPEC-P8-RELOCATE R1. **It is withdrawn, and §8.4's ask is
   written so that nothing depends on it.**

**The only population with `T > δ` today is the stub whose own seat runs deep.** 4 records, 3
distinct shapes, 3.421 mm², **zero at the production plan**.

### §8.3 Do the 4 deep seats need the permission?

They are the one place in the measurement where the recorded incidence is at exactly the station
SPEC-EMBED-POLICY wants, so they can be graded **exactly**.

| record | carried | `T` | exited | `r_x` | `cosang` | rule 1 | rule 2 | rule 3 | verdict |
| --- | ---: | ---: | --- | ---: | ---: | --- | --- | --- | --- |
| r0 t96 / r1 t88 | 0.657 | 2.487 | yes | 0.326 | 0.191 | 1.743 / 2.000 pass | **`L_scar = 3.138 > 2.500` FAIL** | pass | **already ILLEGAL under the existing rules** |
| r0 t225 / r1 t200 | 1.027 / 1.080 | 0.849 | yes | 0.221 | 0.164 | 0.568 / 2.000 pass | 1.291 / 2.500 pass | pass | **LEGAL under rules 1-3 — E-C's hazard, one instance** |

**Read the second row carefully.** `exited = true` on the seat interval means the shaft went in
one side and out the other. Chord 0.849 mm at `cosang = 0.164` gives a perpendicular wall
thickness of **0.139 mm**. That is a 0.139 mm shell, perforated by a support, and rules 1-3 wave
it through because the interval is anchored so rule 4 does not apply. **E-C's constructed
regression is real, and the Rotting Warhorse contains it — one shape, twice, and not in the
production plan.**

The first row matters just as much in the other direction: the existing scar-dimension rule
**already catches the deeper plug** without any thin-feature check.

### §8.4 The ask, corrected in two ways

> **Ask.** SPEC-EMBED-POLICY's no-depth-cap permission is **withdrawn at spec scope, for all
> three designed-embed-end families it names** (contact-stub tips, strut/rung feet, model-rung
> ends). A designed embed end's own anchored interval must end within `MIN_ILLEGAL_LEN_MM` of the
> designed embed depth; if it runs deeper the member is illegal. The thin-feature-span check is
> **not built**, and does not need to be, because the permission it was written to guard is gone.

Why this shape of ask, in four lines:

* **On the stub path it is a spec correction, not a runtime tightening.** `initial.rs:1542`'s
  `remaining_illegal_axis_overlap`, compared against the 0.05 mm cutoff at `:1553`, already
  condemns exactly this population. The engine's implementation of rules 1-3 rolls into
  `diagnostic_constraints_hold_under_recorded_policy` (`transit_measurement.rs:900`), **whose
  getter has zero callers**, and whose entry point `runner.rs:2819-2821` documents itself as
  *"intentionally not an acceptance, clearance, or defect boundary."* The first version's own
  bullet conceded the point — *"at worst identical and never wider"* — while the memo sold it as
  a safety tightening. Say it plainly. It is still worth doing: a spec that grants a permission
  whose guard was never built is a trap for the next implementer, and a spec correction is the
  cheapest possible way to defuse it.
* **Declining it for stubs alone does not close E-C**, because the permission's scope line names
  three families and E-C's own text is labelled `(scope)` and asks *"Which half do we build?"*.
  Declining for one family answers a third of one half. Hence "at spec scope".
* **It costs a measured 4 records, 3.42 mm², and zero at the production plan.** Two of those four
  are already illegal under the existing scar-dimension rule. The true cost of declining is **one
  shape**, `t225`, 1.03 mm² — a 0.139 mm shell perforation which we should not want.
* **It does not depend on anything unratified.** §5.2's R1 amendment is not part of this ask.
* **The thin-wall guard E-C wanted already exists — for landed feet.** Test (a)(i) (§2.3) is
  that check, built here under another name, and it refuses 59 of 152 landing candidates on wall
  thickness alone. E-C's residual scope after this design is not stub tips; it is the other two
  named families this ask still reaches in name only (strut/rung feet, model-rung ends) and,
  more concretely, the PR-2 placed-mask exemption's six consumers named in §8.5, already handed
  to the bundle lane.

**A spec edit must reach both implementations.** There is a second implementation of these
formulas in `plinth-metrics/src/scar.rs`, with its own pinned test
`closed_form_patch_uses_anchored_length_and_never_applies_an_area_cap`
(`plinth-metrics/src/scar.rs:3426`). If the edit does not reach it, engine and auditor diverge.

**A constants note owed under the physics-first doctrine.** `MIN_ILLEGAL_LEN_MM` is registered as
a *sliver-length* cutoff. It is already reused as `min_free_span_mm`. This ask reuses it a third
time, as a *seat-depth* tolerance. The derivation is: all three ask the same physical question —
"is this length of axis long enough to be a real feature rather than a numerical sliver?" — and
the seat-depth use is the same question about the same axis in the same units. That sentence is
the whole derivation and it is stated here rather than left implicit.

### §8.5 What this ask does NOT reach, named for the bundle lane

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

**Not yet wired.** At `1c646aa` `seat_zone` is reached only by `lib.rs`'s `mod` line,
`FINAL_CLEARANCE_GAP_MM` has no reader outside the registry, and `brace.rs:1246/1279` still set
their insets from `contact::mask_extent_mm`. So this section describes the mechanism as
committed, and the wiring it needs is that lane's remaining work, not this one's.

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
strut total length      0.716 - 2.407 mm   (median 1.415), of which 0.3 mm is inside the model at EACH end
crevice                 0.116 - 1.807 mm   (median 0.815)
crevice < 0.6 mm        37 / 80
crevice < 0.3 mm        14 / 80
crevice >= 2.5 mm        0 / 80
```

At the median the strut crosses 0.82 mm of air with a body 0.35 – 0.59 mm in radius. There is no
station in that span from which a 0.3 mm true gap to *both* crevice walls could be measured,
because the walls are what the strut is standing on.

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
2. **The scar bound at both ends (§3), plus the breach check (§2.3(a)(ii)).** The bound is
   unconditional for a standard-depth foot; the breach check is what makes the bound's half-space
   ball term applicable (§3.3).
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

**No new registered constant is proposed at any stage.** Every value is registered:
`DEFAULT_CONTACT_DEPTH_MM`, `MIN_ILLEGAL_LEN_MM`, `MIN_SHOOT_R_MM`, `BALL_FACTOR_FOOT`,
`BALL_FACTOR_STUB`, `A_ABS_MM2`, `KAPPA`, `L_SCAR_MAX_MM`, `PRINT_FLOOR_CHORD_MM`, `TOL_MM`,
`TIP_RADIUS_MAX_LEGACY_MM`, `ball_breach_eps_mm`, and `SPACING_FLOOR_MM` for disclosure only.
Three hardcoded epsilons are reused rather than registered — `1e-9` for the Z comparison,
`BELOW_LEVEL_EPS_MM`, and `FOOT_BREACH_HALF_DEG` / `FOOT_BREACH_CONE_RAYS`. All are recorded as
existing debt, not introduced.

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
strike station, axial chord, **perpendicular wall**, entry incidence, entry face normal, crevice,
landed length, both radii, **the foot-ball breach verdict and poke depth**, both scar verdicts and
which allowance each clears through, the build-order verdict, and the §6.2 disclosure fields. The
stub is **still quarantined**. `BlockedCause::StubReentry` still fires.

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

**Stage 2 — land it (geometry moves; OD-8 stop-and-report).** The stub whose landing passed all
six tests is shortened to `L'`, its free end is seated in the struck surface, and it is **not**
quarantined. Minted as `MemberKind::Strut` so that `contact::seating_of` (`contact.rs:104-110`)
classifies its rootward end as `Seated` with no change to that function. **Implementation check,
not asserted here:** confirm no lowering, emit or brace path keys on `MemberKind::Stub` in a way
that a `Strut` would miss — and, separately, that no parentless-terminal path errors (§2.7(ii)).
**Gate G2:** no model that emits today stops emitting; every moved digest carries a from→to
attribution; `make_toed_slab` bit-identical or attributed; `StubReentry` reports the expected
non-zero residue on the Warhorse production plan — **not zero**, because 7 of its 18 stubs need
re-aim and stage 4 has not landed.

**Stage 3 — the clearance-bundle extension (§9.2).** Two seat zones and their union, owned by #36.
**Stage 2 cannot be accepted without it**, because a doubly-seated member with one zone is refused
by the very exemption that is supposed to admit it. Sequenced here so the dependency is visible;
the work is that lane's.

**Stage 4 — withdraw the deep-embed permission at spec scope (§8).** A spec edit plus the matching
edit in `plinth-metrics/src/scar.rs`. Behaviourally a no-op on the stub path. It is what makes
step 2 of §8.2 a construction rather than a 97.4 % measurement.

**Stage 5 — re-aim, per SPEC-P8-RELOCATE §3.1 R1.** Behind stage 2, because until stage 2 lands we
do not know how many stubs still need steering. **The measurement says 7 of 18 at the production
plan, including island 74 — not 3.** This is now the larger of the two lanes and should be
resourced as such.

**Stage 6 — the tip-embed defect (§7).** A separate lane, not bundled, not blocked by any of the
above. **It now needs its own test**, because the ratified detached-tip rule does not catch it.

### §10.3 Which pins move, and how

| pin | file:line | what it pins | movement |
| --- | --- | --- | --- |
| `terminalward_embed_classifies_clean_transit_and_buried_boundaries` | `initial.rs:1897` | the scalar overlap rule, including a `{0.2, 0.6}` case called "transit" | **MOVES at stage 2.** That case is a strike at `s = 0.2`, inside the designed embed, so it is a deep seat and stage 4 condemns it. The test becomes the classification pin for seat / strike / deep seat |
| `real_stacked_obstruction_accepts_cutoff_and_condemns_above_it` | `initial.rs:2461` | the 0.05 mm cutoff against a real obstruction below a downward stub | **MOVES at stage 2, and it is the acceptance test.** The obstruction is exactly a strike. **Must be re-derived under all six tests and its expectation re-argued, never edited** |
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
| **S2** | grade test (a)(i) on the axial chord instead of the perpendicular wall | a fixture that strikes a 0.15 mm wall at a shallow angle lands. **This is the review's F1, made into a test**, and it is the single most load-bearing guard in the set |
| **S3** | remove test (a)(ii), the foot-ball breach | **island 74's own t162 shape lands**, and a separate assertion catches the foot ball 0.241 mm out the far side. This is the finding that turned the recovery table over; it must not be able to come back silently |
| **S4** | remove test (f) | island 74's `t189` shape lands with its foot **above** its contact |
| **S5** | set `rays_all = false` after stage 1b | a fixture whose axis crosses air but whose flank does not must stop being refused. **Requires a purpose-built fixture; none exists** |
| **S6** | grade the landed foot with the *original* 2.8 mm frustum instead of `L'` | `r_x` is read at the wrong station and at least one scar verdict changes |
| **S7** | restore the deep-seat permission (undo stage 4) | the `t225` shape — a 0.139 mm shell perforated at `cosang = 0.164` — becomes legal. **This is E-C's regression, made into a test** |
| **S8** | give a doubly-seated member only one seat zone | every landed strut is refused. Proves §9.2's union is load-bearing rather than tidy |

**S2, S3 and S7 must not be skipped.** S2 and S3 are the two defects that made the first version
of this document unsound. S7 is the only durable record of why the deep-seat permission was
withdrawn.

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
| **P4 — does a sub-0.3 mm crevice survive curing?** | §0 ask 4. 24.8 % of the recovered area and 27.2 % of the production prize sit below Craig's field line | Craig's own field data, or one printed coupon with a graded slot series. The project has a print minimum for solid and none for air | Decides whether the rule refuses or merely reports below 0.3 mm |
| **P5 — why 52 stub tips sit in air (§7)**, now that rule 3 does not catch them | 25 % of the population, and the ratified test that was supposed to cover it does not | Record the face normal at the contact position and compare it with the averaged `unit` the mint uses | Settles the averaged-normal cause and tells the tip lane what test it needs |
| **E-A — does the struck feature carry the load onward?** | Not measurable by probe; it is a doctrine question (§2.5) | none — it is Craig's ruling, informed by stage 1's instance count | Decides whether tasks #64 / #65 are blockers or neighbours |

---

## §12. ESCALATIONS, COLLECTED

1. **The permanence of every landed strut (§6).** 0 of 80 crevices reach the only
   removal-ergonomics number the project has. Pricing decision, not technical.
2. **Three of ten production scars exceed the 2.0 mm² allowance (§0 ask 2, §6.4).** They clear
   only through the radius-scaled `KAPPA` allowance. The 2.0 mm² was ratified with a stated
   1.60 mm-diameter rationale, and two of these are wider than that.
3. **The sub-0.3 mm crevices (§0 ask 4, §6.4).** 14 of 80, 24.8 % of the recovered area, 1 of 5
   at production carrying 27.2 % of the prize. **A new ask, not in the first version.**
4. **Withdrawing the deep-seat permission at spec scope (§8.4).** It closes the stub half of E-C
   and is a spec correction, not a runtime tightening. **The design recommends withdrawing.**
5. **E-C is NOT closed by this design alone (§8.5).** The PR-2 placed-mask exemption, with six
   live consumers, is untouched and belongs to the clearance bundle.
6. **E-A remains open and this design adds instances to it (§2.5).** Not opened by this design.
   Counted at stage 1 before it is committed.
7. **`rays_all = true` is a real cost, a two-site flip, and a refusal change (§10.2 stage 1b).**
   Its binding cap is `ContactConstructionLimits::production().max_work_items = 4_000_000`, which
   doctrine forbids raising, and **the charge has never been measured**. Failure refuses the whole
   model at initial contact construction.
8. **Build order becomes a member rule for the first time (§3.6).** Two of 152 measured landings
   need it. It is the third instance of an idiom the engine already applies at `cco.rs:4967` and
   `anchor.rs:1063`, but promoting it to a member rule is an owner-visible move.
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
14. **Test (a)(i), the perpendicular wall minimum, is new machinery (§2.3).** It does not exist in
    the engine or in SPEC-EMBED-POLICY. It is a thin-feature-span check for landed feet, built
    here. It refuses 59 of 152 candidates (278.281 mm²), 55 of them where the engine's own ball
    check passes, and it reuses `DEFAULT_CONTACT_DEPTH_MM` — an along-axis embed depth — as a
    perpendicular wall minimum. Whether that is the right constant is an owner question. It may
    also be partial progress on E-C's other half, which should be recorded rather than discovered
    later.

---

## §13. WHAT A REVIEWER SHOULD CHECK FIRST

1. **Re-derive test (a) in both clauses.** Take one recovered record and one refused one from
   `.git/land-at-strike-p1-evidence/full-intervals.jsonl` and confirm that
   `chord × entry_cosang` is the perpendicular wall and that `foot_ball_breached` came out of the
   kernel rather than out of the script. **If the grading is in the wrong units again, everything
   downstream is wrong again.**
2. **Re-derive island 74's refusal.** `t162`: axial chord 0.462902, entry incidence 0.737412,
   wall 0.341356; foot ball radius 0.822711; engine far wall 0.581480; poke 0.241231.
   That is the whole verdict.
3. **Confirm the sweep is centreline-only.** `initial.rs:1359` passes `rays_all = false`;
   `queries.rs:90` makes `n_offset_rays` inert when it is; every one of the 1,926 probe interval
   rows carries `ray = -1`.
4. **Check that the rule is an admission and only an admission for stubs.** MEASURED: of 1,500
   surviving contact records, 7 have a crossing beyond the embed and **0** have a firm one — all
   seven chords are 0.0293 mm, under the 0.08 mm print minimum. **Then check that this is false of
   stage 1b**, which is the point of separating it.
5. **Check that the scar bound is not doing work it cannot do.** §3.3: `A_ball` is a half-space
   formula, and it supplies most of the anchor area. It is only applicable where the breach check
   has passed.
6. **Re-derive the E-C arithmetic (§8.3).** `t225`'s seat interval has `exited = true` and
   `0.849 × 0.164 = 0.139 mm`.
7. **Check the staging.** Stage 1 is measure-only **only if** the production sweep stays
   centreline-only. If stage 1 moves one digit of one digest, the stage boundary is wrong.

---

## APPENDIX — how the adversarial review's findings were folded

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
