# Land the stub where it strikes the model — DESIGN

> **Provenance.** Written 2026-08-22 against `feature/a4-controller @ f50a45b`
> (main checkout `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, tree clean at the
> time of writing). **DOCS ONLY. No production code and no test code changed.**
>
> **No cap, gate, threshold, tolerance or registered constant is moved, widened or
> bypassed by anything proposed here.** Every number this design uses is already in the
> registry. Two behaviours change: a stub that strikes the model stops at the strike
> instead of being thrown away, and a stub's own seat stops being allowed to run deep.
> The second is a TIGHTENING.
>
> **Evidence labels.** **MEASURED** — a number a run produced. **DERIVED** — closed form
> from MEASURED or registered inputs. **INFERRED** — reasoning from a recorded fact, not
> itself measured. **ASSUMED** — a stated placeholder for a fact that does not exist yet.
>
> **Glossary, first use.** *Stub* — the short support member the engine mints at each
> contact point on the model; it starts 0.3 mm inside the model and points outward.
> *Island* — a patch of model surface that has no support under it. *Carried area* — the
> area of model a member holds up, in mm²; it sets the member's radius. *Scar* — the mark
> a support leaves on the model when you break it off. *Transit* — a member passing
> through model material it never declared. *Seat* — the end of a member that rests on
> something. *Foot* — a seat on the model rather than on the build plate.

---

## §0. DECISION MEMO FOR CRAIG

### The one-line version

The engine throws away every support stub that runs into the model. Your instruction is to
stop throwing them away and stand them on what they hit. On the measured evidence that
recovers **120 of 208 condemned stubs**, it recovers **all of island 74**, and it needs
**no new constant** — but every recovered stub is a small permanent bridge you will not be
able to cut off, and that is the thing to price.

### The recovery table, up front

208 stub records were measured on the Rotting Warhorse (7 re-plans of the same model, so
the same stub appears more than once; 78 distinct shapes). "Production plan" is the state
the run settles into after re-planning — 18 condemned stubs, and the plan that would
actually be printed.

| what the stub does | records | distinct shapes | carried area | production plan | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| **Strikes the model, and the strike is a valid foot** | **120** (57.7 %) | 51 | 465.10 mm² | **9 stubs, 64.65 mm²** | **RULE 1 recovers it** |
| Strikes the model, but what it hits is too thin to stand on | 32 (15.4 %) | 10 | 182.10 mm² | 3 stubs, 23.91 mm² | RULE 2 (re-aim) |
| Its own tip never reaches the model at all | 52 (25.0 %) | 14 | 107.50 mm² | 6 stubs, 13.05 mm² | a DIFFERENT defect (§7) |
| Its own seat runs deep through the model | 4 (1.9 %) | 3 | 3.42 mm² | **0 stubs** | stays condemned (§8) |
| **total** | **208** | **78** | **758.12 mm²** | **18, 101.61 mm²** | |

**Island 74 — the island this whole line of work is about.** At the production plan its
whole **13.019 mm²** rests on one condemned stub. That stub strikes the model 1.040 mm out
from its contact, the struck material is 0.463 mm thick, and a standard foot fits.
**RULE 1 recovers it, and island 74 becomes covered.** (One earlier variant of the same
island, a 3.288 mm² sub-patch present only in the first two re-plans, would land *above*
its own contact and must be refused — see the build-order test, §3.6.)

**The prize is not only this model.** On `zombie-3-a`, **52 of 92** coverage-repair trials
(56 %) fail in this same class — the stub is condemned before any arm is even proposed,
and that model records **zero** clearance refusals in the brace stage. MEASURED
(`.git/mask-honesty-evidence/zombie3a-clearance-evidence.txt`). We have not run the
interval probe on `zombie-3-a`, so we cannot yet say how many of its 52 pass the foot
tests. **INFERRED**: the class matches; the split does not.

### What you must authorize

**1. The rule itself.** A stub whose shaft runs into the model stops at the first strike,
with its foot standing on the struck surface. This is a support that runs model-to-model.
It is legal under your three-seat ruling (plate, model surface, or another support). It is
the whole design.

**2. That these bridges are permanent.** Measured: **120 of 120** recovered struts sit in
a crevice narrower than 2.5 mm — half of them narrower than 0.78 mm, sixteen of them
narrower than 0.2 mm. The narrowest is 0.116 mm. Nothing in the engine models a cutting
tool, so we cannot compute whether a blade reaches them; the only removal-ergonomics number
in the whole registry is `SPACING_FLOOR_MM = 2.5 mm`, and the registry says of it
*"selected without measurement"*. **On the honest reading, every one of these struts stays
on the model forever.** At the production plan that is **9 struts, 8.34 mm³ of resin
(0.0083 mL), and 9 new scars on the model totalling at most 11.68 mm²** — each individual
scar within the 2.0 mm² allowance you ratified, each roughly a 1 mm blemish. The design
reports these per strut. It does not gate on them. **You price this.**

**3. Declining a permission you already granted.** SPEC-EMBED-POLICY says a stub's own
seat may run as deep as it likes into the model. That permission was written on the
condition that a separate "thin-feature-span check" catches shells being perforated. **That
check does not exist in this engine.** Under RULE 1 no stub needs the permission — every
landed foot sits exactly 0.3 mm deep, by construction. So the ask is: **let us decline the
deep-seat permission for stubs.** Measured cost: 4 records, 3.42 mm², **zero at the
production plan**. Measured benefit: the shell-perforation hazard that has blocked this
work since 2026-08-21 (escalation E-C) goes away without building anything. **This closes
E-C.**

### What you are NOT being asked to authorize

* **No constant moves.** Not one. Everything used is registered.
* **No new constant is introduced.** If one seemed necessary the design would stop and
  ask; it did not become necessary.
* **The load-path question stays open, and this design does not widen it.** When a strut
  stands on the model, the model carries the load onward. Whether that is safe is
  escalation E-A, and it was opened on 2026-08-21 by the model-seated-arm work that already
  landed — not by this design. RULE 1 adds instances to an exposure that is already live.
  The design refuses to pretend otherwise.
* **Nothing is wired to the removal apparatus.** The six `GRIP_*` constants stay dead.

### What it costs if you say yes

The change is in initial contact minting, so **every model in the corpus is affected** —
that is the nature of the stage, and the design says so rather than implying a narrow blast
radius. The staged plan (§10) puts a **measure-only stage first**: the landing is computed
and recorded for every condemned stub on every corpus model, and nothing moves. You see the
real recovery table for the whole corpus before one triangle of geometry changes.

### The one number that would change the answer

The incidence angle where the shaft strikes the model. The probe recorded the angle where
each interval *ends*, not where it *starts*. Two of the three scar rules do not need it —
proved in §4 and swept over a million cases with zero failures. The third is graded here at
its worst possible value, so the verdicts above are conservative and can only improve. A
follow-up probe (§11, P1) captures it in minutes and costs no pipeline pass.

---

## §1. WHAT THE MEASUREMENT SAYS, AND WHAT IT REFUTES

### §1.1 The evidence

`.git/rw-embed-split-evidence/` — the `rw-embed-split-2026-08-22` probe, print-only,
reverted after the run. Rotting Warhorse, `feature/a4-controller @ 83e3215`, byte-identical
payloads across two binaries. 208 condemned stub records over 7 coverage-repair batches.
Full provenance in `distilled-numbers.txt`.

Per interval the probe records: `s_a`, `s_b` (start and end along the stub axis, measured
from the designed embed endpoint), `entered`, `exited`, `max_depth`, `ray`, `r_x` at both
ends, the embed depth in force, `cosang` and `exit_face` **at the interval's END station**,
`seat`, `p0`, `p1`, `carried_mm2`.

**MEASURED, and load-bearing for everything below:** all 1,926 interval rows carry
`ray = -1`. The sweep is **axis only**. `initial.rs:1358-1370` passes `n_offset_rays = 8`
but `rays_all = false`, and `queries.rs:90` reads
`offset_count = usize::from(rays_all && axis_length >= 1e-9) * n_offset_rays`, so the 8 is
inert. The stub is a tapered capsule and only its centreline is tested.

### §1.2 What §2.4 of the #52 derivation assumed, and what the measurement did to it

`model-seated-arms-2026-08-21.md` §2.4 priced the prize by treating each condemned stub's
deep run as an **anchored** seat — a legal deep plug — and grading it by scar area. The
measurement refutes that premise:

| §2.4 assumed | MEASURED |
| --- | --- |
| the deep run starts at the seat (anchored) | **204 of 208 do not.** Only 4 records have their condemning run starting at the seat |
| `r_x ≈ 0.30 mm` | 0.0867 – 0.5877 mm; island 74's production stub is **0.5877** |
| island 74 becomes routable by scar arithmetic | at the production plan the scar-dimension rule **fails** on the measured `r_x` (`L_scar = 2.659 > 2.500`) |

So §2.4's route to island 74 is closed, and §2.5's sequencing (aim the stub second) is
superseded. **The gap between the seat and the strike is air.** The stub leaves the model
cleanly, crosses a gap, and hits something else. That is not a deep seat and it is not a
scar problem. It is a stub that has arrived somewhere.

### §1.3 Craig's reading, restated as the design

> *"How can it be that the support stub traverses through the model? If it strikes the
> model in a line to where it was supposed to be, then just land it there? I suppose the
> place where it strikes the model on its path might not be an appropriate anchor spot? In
> that case we can re-orient, but if it's a valid spot to be the foot of the support we can
> just land it there?"*

Two rules, ordered:

> **RULE 1 (primary).** A stub whose outward shaft strikes the model **terminates at the
> first strike**, with its foot seated on the struck surface. The result is a support that
> runs model to model.
>
> **RULE 2 (fallback).** Only when the strike point fails the foot tests does the stub
> re-aim, by the owner-ratified cone search in
> `docs/engineering-journal/specs/SPEC-P8-RELOCATE.md` §3.1 R1.

This inverts the order `model-seated-arms-2026-08-21.md` §2.5 proposed, and it does so on
Craig's instruction and on the measurement. RULE 1 is cheaper than re-aiming in every
sense: it preserves the contact point exactly, it preserves the contact direction exactly,
it shortens the member, it needs no candidate search, and it needs no re-route.

---

## §2. RULE 1 FORMALIZED — when does a strike become a foot?

### §2.1 The geometry, named once

The engine mints a stub at `initial.rs:644-673`:

```rust
let p0 = position - unit * depth;   // depth = DEFAULT_CONTACT_DEPTH_MM = 0.3 mm
let p1 = position + unit * span;    // span  = STUB_LENGTH_MM = 2.5 mm
```

`p0` is the **terminalward** end — the contact, embedded 0.3 mm inside the model. `p1` is
the **rootward** end — the free end, out in the air, where routing continues
(`forest.rs:4`: *"`p0` end is terminal-ward and its `p1` end is root-ward"*). Let `s` run
along the axis from `s = 0` at `p0` to `s = L = 2.8` at `p1`. Write `δ = 0.3 mm` for the
designed embed depth. The nominal model surface is at `s = δ`.

Let `s*` be the **first strike**: the smallest `s > δ` at which the shaft re-enters solid.
Then:

* **free span** `= s* − δ` — the measured air the stub crosses.
* **landed length** `L' = s* + δ` — the strut runs from the island contact to a foot seated
  `δ` deep in the struck surface.
* **radii.** `r_tip = r0` (load-law sized, §2.6) and
  `r_foot = max(r0, MIN_SHOOT_R_MM)` — the identical rule `initial.rs:655-657` already
  applies, over the shorter span. The frustum is
  `r(s) = r_tip + (r_foot − r_tip)·s/L'`, monotone non-decreasing from tip to foot.
* everything beyond `s = s* + δ` is **cut away and does not exist**. Crossings out there
  vanish with it. MEASURED: 20 of the 120 recovered stubs had further crossings beyond the
  foot; all 20 disappear.

### §2.2 Print-floor moot strikes are ridden through, not landed on

A crossing whose chord is below `PRINT_FLOOR_CHORD_MM = 0.08 mm` is **MOOT** by the
owner-ratified 2026-07-24 amendment: *"chords < 0.08 mm are 2.6-4.1 px foliage knife-edges
that cannot print as coherent material."* Material that cannot print cannot be stood on.
So a moot crossing is **not** a strike; the shaft rides through it, and it is still counted
and reported in the moot bucket the amendment requires.

MEASURED: 7 of the 120 recovered struts carry a moot crossing inside their free span.

### §2.3 The six tests, and where each comes from

A strike becomes a foot when **all six** hold. Each is an existing predicate or an existing
formula; not one is new machinery.

| # | test | statement | existing machinery |
| --- | --- | --- | --- |
| **(a)** | **the struck surface admits a standard-depth foot** | strike chord `≥ DEFAULT_CONTACT_DEPTH_MM` (0.3 mm), and the foot ball is not breached | chord is `min(s_b, L) − s_a` from the interval sweep already run; the ball breach is `anchor.rs:1607-1630`, `NativeSurface::sphere_breach` with `ball_factor_foot = 1.4`, refusing as `AnchorFootRefusalReason::FootBallBreached` |
| **(b)** | **emergence at the strike** | free span `≥ ContactPolicy::min_free_span_mm` (0.05 mm, sourced from `MIN_ILLEGAL_LEN_MM`) | `contact::emerges` (`contact.rs:169`), re-instrumented — see §2.4 |
| **(c)** | **scar pricing at both ends** | SPEC-EMBED-POLICY rules 1-3 at the foot **and** at the contact tip, with `T = δ` at each | `transit_measurement::calculate_scar_diagnostic` (`transit_measurement.rs:301-368`) and `recorded_constraints` (`:371-386`) — the formulas already exist and already read the registered constants |
| **(d)** | **closed load path** | the foot interface carries the island's load: `contact::area_sufficient(carried_mm2, r_eff, policy)` with `r_eff = ball_cut_radius_mm(r_foot, seat_depth, 1.4)` | `contact.rs:442`, already called inside `contact::admit`. See §2.5 for the half that does **not** exist |
| **(e)** | **the strut's own physics** | the narrowest section carries the load under the area law | `FrozenTipLoadPolicy::tip_radius_mm` (`perception.rs:636-644`) already sized `r0` from `carried_mm2` at ratio 12. See §2.6 |
| **(f)** | **build order** | the foot must be **below** the contact in Z: `p1[2] < p0[2] − 1e-9` | the same inequality `cco.rs:4948 junction_feasible` already applies to a routing junction. **This one must be newly stated as a member rule** — see §3.6 |

### §2.4 Test (b): why `emerges` must be re-instrumented, not relaxed

`contact::emerges` (`contact.rs:169-171`) is:

```rust
total_mask_mm <= seated_length_mm - policy.min_free_span_mm
```

It asks whether enough of the member sticks out past its own contact masks to be
distance-tested at all. **For a landed strut the answer is always no**, and that is not a
defect. The mask at each end is `(h + r_max + standoff + MARGIN)/|cos| ≈ 1.3 mm`
(`contact.rs:400-408`), and the median landed strut is **1.38 mm long in total**. Two masks
of 1.3 mm on a 1.38 mm member leave nothing. Applied as written, `emerges` refuses **every**
landed strut, including island 74's.

This is the same shape of error as #51: a positive gap is demanded in a region that is, by
construction, contact. The honest fix is not to weaken `emerges` — it stays exactly as it is
for every other family — but to notice that for a member with **two declared model seats**
the quantity `emerges` is a proxy for has been **measured directly**:

> **The landed-strut form of the emergence test.** `free_span_mm ≥ min_free_span_mm`, where
> `free_span_mm = s* − δ` comes from the interval sweep.

Same constant. Same threshold. A more direct instrument: `emerges` asks "is some of the
member outside its masks, so that an unsigned distance query will run on it?", and the
interval sweep answers "here is exactly which arc-length of the member is outside the
solid." **This is not a relaxation for any other member kind, and it must not be offered as
one.**

**The price, stated plainly.** The sweep is axis-only (§1.1). A capsule of radius 0.35 mm
whose centreline crosses 0.155 mm of air can still have its flank inside the model.
**Therefore RULE 1 requires the sweep to be rim-swept for landed candidates** — flip
`rays_all` from `false` to `true` in the `penetration_intervals_batch_bounded_metered` call
at `initial.rs:1358-1370`, which turns the already-passed `n_offset_rays = 8` from inert to
live. That is an existing parameter of an existing call, not a new constant and not new
machinery. It is a strict tightening: more rays can only find more material. Its cost is
9× the ray work in that one batch, and it must be measured against
`PenetrationBatchLimits` before it is enabled (§10, gate G1).

**Without the rim rays, RULE 1 is not honest.** This design does not offer the axis-only
version as a fallback.

### §2.5 Test (d): what exists, what does not, and whose problem it is

Craig's ruling is that the test is a **closed load path per physics**. That splits in two.

**The half that exists.** Does the foot interface carry the load it is being handed? That
is the area law, and `contact::area_sufficient(carried_mm2, r_eff_mm, policy)`
(`contact.rs:442`) is exactly it, using `required_area_mm2` from `FrozenTipLoadPolicy`. It
is already called inside `contact::admit` for every other contact. Nothing new.

**The half that does not exist.** Does the *struck feature* carry that load onward to its
own supports? Nothing tests this. `propagate_carried` (`cco.rs:4703-4726`) walks parent-ward
and stops; a seat has no parent, so the accounting ends at the model face and no one charges
that face. This is **escalation E-A**, raised 2026-08-21, with tasks **#64** (narrow-neck
amplification) and **#65** (no connectivity requirement in the reach probe) governing the
receiving side.

**The honest position.** E-A was opened by the model-seated-arm family that landed in
`4c74f74`, not by this design. Every CCO model anchor committed today has the same open
question. RULE 1 **adds instances** to a live exposure; it does not create one. The design
states this rather than arguing its way around it, and §10's stage 1 counts the instances
before any geometry moves, so the size of the addition is a measured number when Craig rules
on E-A.

### §2.6 Test (e): the strut's own physics is satisfied by construction

MEASURED, from the probe: for the first contact record, `carried_mm2 = 0.114574371` and
`r0 = 0.055128755`, and `carried / (π·r0²) = 12.000` exactly. The tip radius is already
sized by the area law at the implied ratio 12
(`FrozenTipLoadPolicy::implied_area_ratio()`, `perception.rs:587-593` — `tensile_pa /
(pressure_pa · SF) = 0.288e6 / (3000 · 8) = 12`).

The narrowest section of a landed strut is at the contact tip, because the frustum is
monotone non-decreasing toward the foot (§2.1). That section is unchanged by landing. And
landing **shortens** the member — median 2.8 mm to 1.38 mm — which can only reduce buckling
and bending. **Test (e) passes by construction for every landed strut, and this is a
derivation, not an observation.**

Registry note: `X_ALLOWED = 12` is `Derived` (`floor(lower(X_STAR_CONE)/2)`), but it is
**not** the value the sizing code reads — the code reads `FrozenTipLoadPolicy` and the
registry link is a one-sided test assertion
(`constants_placeholders.rs::engine_implied_tip_area_ratio_does_not_exceed_the_h2_allowance`).
This design changes neither and records the seam because it bears on how test (e) is
argued.

### §2.7 What happens to the ARM requirement

**A landed strut needs no arm, and none is proposed.**

Today a stub is a stub: the coverage stage takes its rootward end `p1` and runs a candidate
tournament for an arm down to the build plate or to a model anchor. Under RULE 1 the
rootward end is *already* a model seat, so the terminal's route is complete at the stub.
Consequences, all of them simplifications:

* **No candidate tournament, no clearance rows, no winner election** for these terminals.
  The #51 clearance-mask question does not arise for a member that has no arm.
* **The FU-87 minimum-arm screen (`STRUT_MODEL_ANCHOR_MIN_LEN_MM = 5.0 mm`) does not
  apply** — there is no arm to screen. This matters: a landed strut is 0.72 – 2.58 mm long
  (MEASURED), so if it were ever routed through the arm screen it would be refused outright.
  The design's position is that it is not an arm and must not enter that screen.
* **`propagate_carried` terminates naturally** — a seated member has no parent, which is the
  behaviour it already has.
* **The island's coverage is carried through the model.** That is the whole point, and it is
  the part E-A governs (§2.5).
* **The E-E accounting seam gets *less* exposed, not more.** E-E is about a terminal whose
  sealed transcript says `Routed` while its ledger says `Quarantined`. RULE 1 moves
  terminals out of `Quarantined`, so it reduces the population that can hit that seam.

---

## §3. THE SCAR THEOREM — why a standard-depth foot cannot be priced illegal

### §3.1 The claim

> **DERIVED.** A foot seated exactly `δ = DEFAULT_CONTACT_DEPTH_MM = 0.3 mm` into the
> struck surface, on a member whose foot radius is at least `MIN_SHOOT_R_MM = 0.35 mm`,
> satisfies SPEC-EMBED-POLICY rules 1, 2 and 3 **at every incidence angle and at every
> radius in the registered range**. The same holds at the contact tip.

This is why test (c) never fails in the measurement, and it is the reason the deep-embed
permission is not needed (§8).

### §3.2 The proof

Write `T` for the length of the seat interval — pinned at `δ` by construction — `r_x` for
the frustum radius at the surface crossing, `r_end` for the end radius, `R_b` for the ball
radius, and `d` for the perpendicular distance from the embed endpoint to the surface. The
registered formulas (`transit_measurement.rs:301-368`) are:

```
A_shaft  = min( π·r_x² / max(cosang, EPS_COS),  2·r_x·T )
A_ball   = π·max(R_b² − d², 0)
A_patch  = A_shaft + A_ball          A_max = max(A_ABS_MM2, KAPPA·π·R_b²)
L_scar   = min( 2·r_x / max(cosang, EPS_COS),  T + 2·r_x )
A_anchor = A_ball + π·r_x²           neck  = π·r_end²
```

**The `min()` in `A_shaft` and `L_scar` is the whole argument.** Both are capped by a term
that does not contain `cosang`. So `A_shaft ≤ 2·r_x·T` and `L_scar ≤ T + 2·r_x` hold at
*every* incidence, including perfectly grazing. With `T` pinned at 0.3 mm:

**Rule 2 (scar dimension).** `L_scar ≤ 0.3 + 2·r_x`. The largest registered radius is
`TIP_RADIUS_MAX_LEGACY_MM = 1.0 mm`, so `L_scar ≤ 2.3 mm < 2.5 mm = L_SCAR_MAX_MM`. ∎

**Rule 1 (scar area).** `A_ball ≤ π·R_b² = A_design`, and `A_max ≥ 1.25·A_design`, so the
headroom above the ball term is `A_max − A_design ≥ max(2.0 − A_design, 0.25·A_design) ≥
0.4 mm²`. And `A_shaft ≤ 0.6·r_x ≤ 0.6·r_end`, because the frustum is monotone
non-decreasing toward the foot so `r_x ≤ r_end` there. Two cases: if `A_design ≤ 1.6` then
`r_end ≤ 0.510` and `0.6·r_end + A_design ≤ 0.306 + 1.6 = 1.906 < 2.0`; if `A_design > 1.6`
then `r_end > 0.510` and `0.6·r_end ≤ 1.5395·r_end²` reduces to `r_end ≥ 0.390`, which
holds. ∎

**Rule 3 (anchor area).** At the **contact tip** the frustum grows away from the tip, so
`r_x ≥ r_end` and `A_anchor ≥ π·r_x² ≥ π·r_end² = neck` with no help from the ball term. ∎
At the **foot** `r_x ≤ r_end`, so the ball term must make up the difference. `d ≤ T = 0.3`
(a perpendicular distance can never exceed the along-axis distance), so
`A_ball ≥ π(1.96·r_end² − 0.09)` and rule 3 reduces to `0.96·r_end² + r_x² ≥ d²`, for which
`0.96·(0.35)² = 0.1176 ≥ 0.09` suffices. Since `r_foot = max(r0, MIN_SHOOT_R_MM) ≥ 0.35`
always, it holds. ∎

### §3.3 The falsification sweep

**MEASURED** (`scratchpad/land-at-strike-out/theorem.ts`): 1,002,001 `(r0, r_x)` pairs over
the full registered radius range `[TIP_RADIUS_DEFAULT_MM, TIP_RADIUS_MAX_LEGACY_MM] =
[0.05, 1.0]`, both ends graded, worst case taken over the unmeasured incidence.

```
failures: 0
worst rule 1  A_patch / A_max  = 0.841356   (must be <= 1)
worst rule 2  L_scar           = 2.300000 mm (must be <= 2.5)
worst rule 3  A_anchor / neck  = 1.000000   (must be >= 1)
rule 1 is tightest at r_end = 0.9425 mm, ratio 0.841
```

**Honest caveat.** The rule-3 worst case is attained *exactly*, at ratio 1.000. It occurs at
the contact tip when the sweep is allowed to set `r_x = r_end` with the ball wholly proud of
the surface. The real parameterization forbids that combination — when `r0 < 0.2` the ball
is proud but the frustum must taper up to `MIN_SHOOT_R_MM`, so `r_x > r_end` strictly — so
the margin is positive in the constrained space. But an implementation **must** compare with
`>=`, as `recorded_constraints` already does (`anchor_area_mm2 >= neck_disc_area_mm2`), and
**must** derive `r_x` from the landed frustum rather than the original 2.8 mm one.

### §3.4 What the theorem does and does not buy

It buys: test (c) is not the binding test, so RULE 1's recovery rate is decided by geometry
(does it strike thick enough material, in the right direction?) rather than by scar
arithmetic. That is the right shape — the scar rules exist to catch long grazing gashes, and
a 0.3 mm foot cannot make one.

It does not buy: any claim about the *aggregate* scar. The theorem bounds one scar. Nine
struts make eighteen scars, and two of them sit within a fraction of a millimetre of each
other on either side of a crevice. **Nothing in the registry bounds a scar population**, and
this design does not invent one — it discloses (§6).

### §3.5 Where the unmeasured incidence actually bites

Only rule 3 at the foot, and only through `d`. The grading above sets `d = min(T, R_b)`,
the largest value it can take, which makes `A_ball` as small as possible and rule 3 as hard
as possible. So **every verdict in this document is conservative in the same direction and
can only improve** when probe P1 (§11) supplies the real `d = δ·cosang_entry`.

### §3.6 Test (f): build order does not fall out of anything — it must be stated

**The audit is decisive.** There is no build-order, print-order or Z-monotonicity rule
anywhere in the support crates:

* `Forest::validate` (`forest.rs:1198+`) audits graph topology only and **reads no geometry
  fields at all** — `p0_mm` and `p1_mm` never appear in its body.
* `Forest::set_parent` (`forest.rs:822-860`) checks self-parenting, cycles and brace
  exclusion. It never compares the two ends' Z.
* Zero matches for `build_order`, `print_order`, `z_order`, `gravity`.
* The repo says the hole out loud, at `forest.rs:872-881`: *"The arena holds no plate
  reference, so `Forest::validate` cannot tell that root from a legitimately grounded one,
  and `lower` simply omits the root boss for a root outside the plate band rather than
  objecting."*

The one existing descend rule is `cco.rs:4948-4973 junction_feasible`, `j[2] < se[2] - 1e-9`
— a routing junction must sit strictly below the stub end it routes from. It is scoped to
one candidate-acceptance step and is never restated as a member invariant.

**Why the rule is necessary.** The printer builds bottom-up. A strut running *down* from the
island to a feature already printed is a support. A strut running *up* from the island to a
feature that does not exist yet is a **hanger**: at the moment the island prints, the strut
holds nothing, and all it contributes is a second scar. Nothing in the scar rules, the area
law, or the emergence test can see the difference — they are all Z-blind.

**MEASURED, on the 120 recovered stubs:**

```
foot BELOW contact : 118 / 120
foot ABOVE contact :   2 / 120
dz (foot_z - contact_z): min -1.908  p25 -1.447  median -1.039  max +0.266 mm
```

Both hangers are island 74's small sub-patch, terminals 189 (batch 0) and 167 (batch 1),
`dz = +0.2655 mm`, `carried = 3.288 mm²` each, contact normal Z-component 0.5835 — a
stub aimed upward and outward off a sideways-facing cheek. **They must be refused, and test
(f) is what refuses them.** They do not exist at the production plan; the re-plan
consolidated island 74 onto terminal 162, whose foot is below its contact.

**Statement, using the existing idiom and no new constant:**

> A landed strut is legal only if its foot end is strictly below its contact end:
> `p1[2] < p0[2] − 1e-9`, the same comparison and the same guard `cco.rs:4948` already
> applies to a routing junction.

The `1e-9` is a hardcoded epsilon, not a registry entry — as are `FOOT_BREACH_HALF_DEG` and
`FOOT_BREACH_CONE_RAYS` at `anchor.rs:32-34`. Reusing an existing hardcoded epsilon is
recorded here as a known, small piece of debt rather than smuggled in as a constant.

---

## §4. THE 208-RECORD GRADING

### §4.1 Method

`scratchpad/land-at-strike-out/grade.ts`. For each of the 208 condemned records: sort its
intervals; find the seat interval (start within `TOL_MM = 0.05` of `p0`); test whether the
seat ends at the nominal surface; find the first firm strike beyond `δ` (skipping
print-floor moot crossings, §2.2); apply tests (a), (b), (c) with the conservative incidence
of §3.5. Tests (d), (e), (f) are reported separately: (e) passes by construction (§2.6), (f)
is measured in §3.6, (d)'s open half is E-A (§2.5).

Every constant is read at its registered value and none is moved.

### §4.2 The recovery table

| class | records | distinct shapes | carried mm² | share |
| --- | ---: | ---: | ---: | ---: |
| **RULE 1 — lands at the strike** | **120** | 51 | **465.100** | 57.7 % |
| **RULE 2 — needs re-aim** | 32 | 10 | 182.099 | 15.4 % |
| **TIP NOT EMBEDDED — a different defect** | 52 | 14 | 107.498 | 25.0 % |
| **DEEP PLUG — stays condemned** | 4 | 3 | 3.421 | 1.9 % |
| MOOT ONLY / NO CROSSING | 0 | 0 | 0.000 | 0 % |
| **total** | **208** | **78** | **758.118** | |

### §4.3 Per re-plan

| batch | condemned | RULE 1 | RULE 2 | tip-not-embedded | deep plug | carried | RULE 1 carried |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 59 | 37 | 9 | 11 | 2 | 128.07 | 67.57 |
| 1 | 57 | 36 | 8 | 11 | 2 | 120.41 | 72.73 |
| 2 | 18 | 9 | 3 | 6 | 0 | 101.61 | 64.65 |
| 3 | 18 | 9 | 3 | 6 | 0 | 101.61 | 64.65 |
| 4 | 19 | 10 | 3 | 6 | 0 | 102.77 | 65.81 |
| 5 | 19 | 10 | 3 | 6 | 0 | 102.03 | 65.06 |
| **6 (production plan)** | **18** | **9** | **3** | **6** | **0** | **101.61** | **64.65** |

The production plan is stable across batches 2-6. Note the deep-plug class **vanishes** once
the controller re-plans: it exists only in the first two batches.

### §4.4 The production plan, stub by stub

```
RULE 1  (9 stubs, 64.647 mm2)
  t87  isl 69  carried 11.879  strike 1.885  chord 0.915  crevice 1.585  L' 2.185
  t100 isl  8  carried  0.715  strike 1.082  chord 0.382  crevice 0.782  L' 1.382
  t142 isl 70  carried  5.984  strike 0.719  chord 2.081  crevice 0.419  L' 1.019
  t157 isl 71  carried  9.974  strike 0.474  chord 0.510  crevice 0.174  L' 0.774
  t162 isl 74  carried 13.019  strike 1.040  chord 0.463  crevice 0.740  L' 1.340   <- ISLAND 74
  t173 isl  0  carried  0.825  strike 1.664  chord 1.136  crevice 1.364  L' 1.964
  t186 isl  0  carried  6.306  strike 1.456  chord 0.354  crevice 1.156  L' 1.756
  t215 isl 48  carried  8.016  strike 1.304  chord 0.599  crevice 1.004  L' 1.604
  t281 isl 68  carried  7.929  strike 1.063  chord 0.315  crevice 0.763  L' 1.363

RULE 2  (3 stubs, 23.913 mm2) — all three: the struck feature is thinner than 0.3 mm
  t154 isl 66  carried 12.661  strike 1.232  chord 0.087
  t229 isl 65  carried  6.800  strike 2.575  chord 0.225
  t231 isl  3  carried  4.453  strike 2.660  chord 0.140

TIP NOT EMBEDDED  (6 stubs, 13.053 mm2) — see §7
  t115 isl 47  carried  1.962   air gap p0->model 0.192   (only moot crossings beyond)
  t256 isl  7  carried  1.419   air gap 0.152   would land at 2.292, chord 0.508
  t273 isl 51  carried  3.258   air gap 0.143   would land at 0.942, chord 1.858
  t275 isl 72  carried  3.434   air gap 0.234   would land at 0.518, chord 2.282
  t277 isl 51  carried  2.743   air gap 0.175   would land at 0.975, chord 1.825
  t284 isl  7  carried  0.238   air gap 0.204   would land at 0.920, chord 1.881
```

### §4.5 Island 74, in full

Nine condemned records touch island 74 across the seven batches. **All nine pass RULE 1's
scar and foot tests.** Two distinct shapes:

| shape | batches | carried | strike | chord | crevice | `L'` | `r_x` | `A_patch ≤` / `A_max` | `L_scar ≤` | `A_anchor` / neck | (f) build order |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | --- | --- |
| t184 / t162 | 0-6 | 8.996 → **13.019** | 1.0398 | 0.4629 | 0.7398 | 1.3398 | 0.4885 → 0.5877 | 1.480/2.000 → **2.196/2.658** | 1.277 → **1.475** | 1.936/0.750 → **2.929/1.085** | **below — PASS** |
| t189 / t167 | 0-1 only | 3.288 | 0.4550 | 0.7386 | 0.1550 | 0.7550 | 0.3283 | 0.669 / 2.000 | 0.957 | 0.810 / 0.385 | **above — REFUSED** |

**Answer to the question that started this lane: yes.** At the production plan island 74's
whole **13.019 mm²** rests on terminal 162; that stub strikes 1.040 mm out, the struck
material is 0.463 mm thick along the axis, a 0.3 mm foot fits, all three scar rules pass with
`A_patch ≤ 2.196` against an allowance of 2.658 and `L_scar ≤ 1.475` against 2.500, and the
foot is below the contact. **Island 74 becomes covered.**

Note what changed the scar verdict versus `distilled-numbers.txt`: as a *deep plug* graded on
the whole 1.184 mm run, t162 fails with `L_scar = 2.659 > 2.500`. As a *landed foot* with `T`
pinned at 0.3 mm it passes with `L_scar ≤ 1.475`. **Landing does not merely rescue the stub;
it makes the scar smaller.** That is the physical content of the rule.

### §4.6 Spreads over the recovered population

```
crevice (free span)        min 0.116  p25 0.432  median 0.782  p75 1.262  max 1.979 mm
landed length L'           min 0.716  p25 1.032  median 1.382  p75 1.862  max 2.579 mm
struck wall along axis     min 0.315  p25 0.505  median 0.915  p75 1.503  max 2.383 mm
foot A_patch upper bound   min 0.606  p25 0.653  median 0.665  p75 1.288  max 2.196 mm2
foot L_scar upper bound    min 0.749  p25 0.903  median 0.944  p75 1.217  max 1.475 mm
foot radius r_end          min 0.350  median 0.350  max 0.588 mm   (74/120 sit at MIN_SHOOT_R_MM)
carried area               min 0.002  median 1.446  max 13.019 mm2

crevice  [0.0, 0.2) : 16      [0.2, 0.5) : 20      [0.5, 1.0) : 38      [1.0, 2.0) : 46
crevice >= SPACING_FLOOR_MM (2.5 mm)      :   0 / 120
struck chord >= foot ball radius          :  77 / 120
carries a moot crossing in the free span  :   7 / 120
```

---

## §5. RULE 2's SCOPE AFTER RULE 1

### §5.1 What is left for re-aim

**32 records, 10 distinct shapes, 182.099 mm² carried; 3 stubs and 23.913 mm² at the
production plan.** They divide into exactly two reasons, and neither is a scar failure:

| reason | records | carried | chord range | reading |
| --- | ---: | ---: | --- | --- |
| **the struck feature is thinner than a standard foot** — test (a) | 30 | 182.033 | 0.087 – 0.299 mm | firm material (above the 0.08 mm print floor) but under the 0.3 mm foot depth: foliage knife-edges |
| **the free span is under 0.05 mm** — test (b) | 2 | 0.065 | 2.454 mm | the stub grazes tangentially, emerges for 46 µm, then plunges into thick solid |

**This is a clean, physical partition, and it is what makes RULE 2 a small job.** Re-aim
exists for stubs that strike foliage too thin to stand on, and for stubs that leave the
surface so shallowly they never really left. Both are exactly the cases where turning the
stub is the right answer.

### §5.2 How RULE 1 changes SPEC-P8-RELOCATE §3.1 R1

R1 is adopted unchanged in its search, its pinned enumerations
(`P8_REAIM_POLAR_DEG = (0, 10, 20, 30)`, `P8_REAIM_AZIMUTH_DEG = (0..315 step 45)`,
`P8_REAIM_LENGTH_FACTORS = (1.0, 0.75, 0.5)`, ≤ 75 candidates), its global lexicographic
minimum, and its resin guards (`P8_REAIM_VOLUME_DELTA_MAX_ML = 0.02`,
`P8_REAIM_ROUTE_GROWTH_MAX_FACTOR = 1.50`, `P8_POCKET_CORRIDOR_PAD_MM = 0.5`). All five
`P8_REAIM_*` values and the pocket pad stay exactly where they are.

**One amendment, and it is a simplification.** R1's legality clause (a) grades each
candidate's *tip* SCAR verdict on re-derived inputs, where `T` is "the tip-anchored interior
interval length" and can be arbitrarily long. **Under RULE 1, every R1 candidate is itself
landed at its own first strike**, so `T = δ` for every candidate, and clause (a) collapses to
the theorem of §3 — it passes unconditionally. What actually selects among R1 candidates
becomes tests (a), (b) and (f): does this direction strike something thick enough, far
enough out, and below the contact?

That is a strictly smaller search problem than R1 as written, and it removes the only path by
which R1 could have produced a deep plug (§8).

**Not settled here:** whether R1's `n_moot_crossings` score term should count moot crossings
in the free span (7 of 120 recovered stubs carry one). The design's position is that it
should, unchanged — a candidate that rides through a knife-edge is worse than one that does
not — but this is R1's tuple and R1 is owner-ratified, so it is recorded, not amended.

### §5.3 What stays honestly condemned

**Four records, three distinct shapes, 3.421 mm², zero at the production plan.** These are
the deep plugs of §8. Nothing else. After RULE 1 and RULE 2, and setting aside the
tip-embed defect of §7, the honestly-condemned residue on the Rotting Warhorse is **empty at
the production plan**.

The `BlockedCause::StubReentry` class (`coverage_closure.rs:375-381`) must therefore
**survive** — it stays the right name for the residue — but on this model it should report
zero. That is the acceptance signal, and it is the same shape of signal
`a4_transactional_controller.rs:465-476` already uses.

---

## §6. REMOVAL AND RESIDUE — the honest disclosure

### §6.1 Why this is a disclosure and not a gate

**There is no removability model in this engine.** The audit is unambiguous:

* Six registered constants exist for it — `GRIP_CLEAR_MM = 5.0`, `GRIP_HANDLE_LEN_MM =
  10.0`, `GRIP_HANDLE_R_MM = 1.0`, `GATE_ON_REMOVAL_DEFAULT = false`, `PULL_DIST_MM = 15.0`,
  `PEEL_ELEVATIONS_DEG` — and **each appears exactly once outside its own declaration, in a
  registry-parity test.** Zero readers. Their `source_locators` point at
  `scripts/support_placement/config.py`, a file that no longer exists in this checkout.
* `tool_access`, `tweezer`, `snip`, `pliers`: **zero hits** anywhere in `crates/` or
  `docs/`.
* The only wired constant wearing a removal label is `SPACING_FLOOR_MM = 2.5 mm`, whose
  registry note reads *"Removal-ergonomics spacing floor selected without measurement"*. It
  merges specialized contacts closer than 2.5 mm (`specialized.rs:7731-7738`). It measures no
  graspable length, no clearance, and no approach direction.

So a "reachable" flag cannot be **derived**. Any derivation would be inventing a tool. What
the design can do is **report the geometry and name the one number the project has**, exactly
as the registry describes it.

**The GRIP apparatus stays unwired. This design does not wire it and does not propose to.**

### §6.2 The per-strut disclosure record

Emitted for every landed strut, alongside its existing condemnation/route record:

| field | value | derived from |
| --- | --- | --- |
| `crevice_width_mm` | the free span `s* − δ` | MEASURED, the interval sweep |
| `strut_length_mm` | `L' = s* + δ` | DERIVED |
| `strut_r_tip_mm`, `strut_r_foot_mm` | `r0`, `max(r0, MIN_SHOOT_R_MM)` | DERIVED, `initial.rs:655-657` rule |
| `foot_ball_radius_mm` | `BALL_FACTOR_FOOT · r_foot` | registered, 1.4 |
| `strut_volume_mm3` | frustum volume over `L'` | DERIVED |
| `in_crevice_volume_mm3` | frustum volume over the free span — the part that ends up in the gap | DERIVED |
| `tip_scar_upper_bound_mm2`, `foot_scar_upper_bound_mm2` | `A_patch` bounds from §3 | DERIVED |
| `reach` | `Unknown` if `crevice_width_mm ≥ SPACING_FLOOR_MM`, else `PermanentUnlessMeasured` | **the only registered removal-ergonomics distance, used for disclosure only** |

**`reach` never gates anything.** It is a label on a report. The two values are deliberately
named so that neither reads as a clean bill of health: even the wide case is `Unknown`,
because 2.5 mm is a contact-spacing number, not a tool-reach number.

### §6.3 What the measurement says

**MEASURED, production plan (9 landed struts):**

```
 t87  crevice 1.585  L' 2.185  r 0.561 -> 0.561  vol 2.163 mm3 (in crevice 1.569)  foot scar <= 1.994
 t100 crevice 0.782  L' 1.382  r 0.138 -> 0.350  vol 0.274      (in crevice 0.155)  foot scar <= 0.654
 t142 crevice 0.419  L' 1.019  r 0.398 -> 0.398  vol 0.508      (in crevice 0.209)  foot scar <= 0.934
 t157 crevice 0.174  L' 0.774  r 0.514 -> 0.514  vol 0.643      (in crevice 0.145)  foot scar <= 1.655
 t162 crevice 0.740  L' 1.340  r 0.588 -> 0.588  vol 1.454      (in crevice 0.803)  foot scar <= 2.196
 t173 crevice 1.364  L' 1.964  r 0.148 -> 0.350  vol 0.403      (in crevice 0.280)  foot scar <= 0.663
 t186 crevice 1.156  L' 1.756  r 0.409 -> 0.409  vol 0.923      (in crevice 0.608)  foot scar <= 0.993
 t215 crevice 1.004  L' 1.604  r 0.461 -> 0.461  vol 1.072      (in crevice 0.671)  foot scar <= 1.303
 t281 crevice 0.763  L' 1.363  r 0.459 -> 0.459  vol 0.901      (in crevice 0.504)  foot scar <= 1.288

 total resin           8.341 mm3 = 0.0083 mL
 new model scars       9 foot scars, 11.680 mm2 in total (upper bounds)
 reach                 9 / 9  PermanentUnlessMeasured
```

Over the whole 120-record recovered population: **0 of 120** crevices reach 2.5 mm. The
widest is 1.979 mm. **On the honest reading every landed strut on this model is permanent.**

### §6.4 The three things a reader should not conclude

1. **Not "the scars are fine because each is under 2.0 mm²".** Each *is* under the
   allowance — that is the §3 theorem. But the allowance was ratified for *one* scar
   (*"A_ABS = 2.0 mm² caps the scar at 1.60 mm equivalent diameter... inside the
   buff/fill/paint class"*). Nine struts put eighteen scars on the model, two per crevice
   and within a millimetre of each other. **No registered rule bounds a scar population.**
2. **Not "0.0083 mL is negligible".** The resin volume is negligible. The visual result of
   nine permanent 1 mm bridges across nine crevices on a mini is a different question, and
   it is not a volume question.
3. **Not "we can measure our way out of this cheaply".** The flush-cutter measurement is
   escalation **E-B**, still open. This design does not resolve it and does not need it
   resolved to land — but it makes the exposure larger and concrete, which is a reason to
   answer E-B rather than a reason to delay RULE 1.

---

## §7. THE CLASS THE MEASUREMENT EXPOSED — 52 stubs whose tip never reaches the model

Not part of Craig's question, recorded because the grading found it and because it is a
quarter of the population.

**MEASURED.** 52 of 208 records (25 %; 14 distinct shapes; 107.498 mm²; 6 stubs and
13.053 mm² at the production plan) have **no interval starting at the designed embed
endpoint**. The stub's `p0` is supposed to sit 0.3 mm inside the model. It sits in air.

```
air gap from p0 to the nearest model material, along the stub axis:
   min 0.136   p25 0.152   median 0.192   max 0.653 mm
achieved embed depth where the shaft does graze material:
   min 0.0028  median 0.0259 mm      (designed: 0.300 mm)
```

`distilled-numbers.txt` spotted two of these (terminals 130 and 89) and recorded them as an
incidental finding. There are 52.

**INFERRED cause.** `initial.rs:646` steps inward along `unit`, the contact normal, which is
an interpolated or averaged direction. On a sharp convex feature the averaged normal does not
point into the solid, so a 0.3 mm step along it lands outside. The probe does not record the
face normal at `p0`, so this is reasoning from the geometry, not a measurement.

**Disposition.**

* **This is not what RULE 1 fixes**, and RULE 1 must not be allowed to paper over it. A stub
  whose contact end is 0.19 mm off the model is not attached to the island; landing its far
  end changes nothing about that.
* **The honest test already exists and is already ratified.** SPEC-EMBED-POLICY rule 3 is
  written for exactly this: *"also catches detached tips — `T = 0` and `d ≥ R_b` gives
  `A_anchor = 0`, ILLEGAL. Depth checks were structurally blind in this direction."* Whether
  each of the 52 fails it depends on `d`, the perpendicular distance from `p0` to the
  surface, which the probe did not record. **Probe P1 settles it (§11).**
* **The prize is real and conditional.** 45 of the 52 (89.926 mm²) have a firm strike that
  passes tests (a) and (b), so **once the tip defect is repaired they land as well**. The
  other 7 (17.572 mm²) have only moot crossings beyond the embed, so they become legal
  outright under the print-floor amendment with no landing needed.
* **It is a separate lane.** It touches seat selection and attestation, not transit. It is
  listed as stage 4 in §10 and is not bundled.

---

## §8. E-C DISPOSITION — the deep-embed permission is not needed, so decline it

### §8.1 The question, restated

SPEC-EMBED-POLICY says: *"There is NO depth cap on the tip-anchored interval itself: a deep
perpendicular plug into thick solid has a small crossing patch and, per the owner's
idempotence ruling, no further physical consequence; thin walls are protected by rule 4 and
the thin-feature-span check."*

**The thin-feature-span check does not exist in this engine.** Escalation **E-C**
(`model-seated-arms-2026-08-21-amendment.md` §B3) constructs the regression: a stub through a
2.0 mm hollow shell at 72° passes rules 1-3, is anchored so rule 4 is moot, and a **6.7 mm
shell perforation becomes legal** where today's 0.05 mm cutoff refuses it. That has blocked
Piece D since 2026-08-21.

### §8.2 After RULE 1, is the deep-embed grading exercised at all?

**No — by construction, in three places:**

1. **A landed foot has `T = δ = 0.3 mm` exactly.** The landing rule pins it. There is no
   deep interval to grade.
2. **The contact tip has `T = δ` too**, whenever the seat interval ends at the nominal
   surface — which is the definition of "emerged" and is true for 152 of the 156 records
   that have a seat at all.
3. **Every RULE 2 re-aim candidate is itself landed** (§5.2), so its `T` is `δ` as well. R1
   was the one remaining path by which a re-aimed stub could have plunged deep, and RULE 1
   closes it.

**The only population with `T > δ` is the stub whose own seat runs deep — the deep-plug
class.** 4 records, 3 distinct shapes, 3.421 mm², **zero at the production plan**.

### §8.3 Do the 4 deep plugs need the permission?

They are the one place in the whole measurement where the recorded `cosang` is at exactly the
station SPEC-EMBED-POLICY wants (the seat interval's exit), so they can be graded **exactly**,
not conservatively.

| record | carried | `T` | exited | `r_x` | `cosang` | rule 1 | rule 2 | rule 3 | verdict |
| --- | ---: | ---: | --- | ---: | ---: | --- | --- | --- | --- |
| b0 t96 / b1 t88 | 0.657 | 2.487 | yes | 0.326 | 0.191 | 1.743 / 2.000 pass | **`L_scar = 3.138 > 2.500` FAIL** | pass | **already ILLEGAL under the existing rules** |
| b0 t225 / b1 t200 | 1.027 / 1.080 | 0.849 | yes | 0.221 | 0.164 | 0.568 / 2.000 pass | 1.291 / 2.500 pass | pass | **LEGAL under rules 1-3 — this is E-C's hazard, and it has exactly one instance** |

**Read the second row carefully.** `exited = true` on the seat interval means the shaft went
*in one side and out the other*. Chord 0.849 mm at `cosang = 0.164` gives a perpendicular
wall thickness of **0.139 mm**. That is a 0.139 mm shell, perforated by a support, and rules
1-3 wave it through because the interval is anchored so rule 4 does not apply. **E-C's
constructed regression is real, and the Rotting Warhorse contains it — one shape, twice, and
not in the production plan.**

The first row matters just as much in the other direction: **the existing scar-dimension rule
already catches the deeper plug** without any thin-feature check. `L_scar = 3.138 > 2.500`.

### §8.4 The minimal honest ask

> **Ask.** Piece D declines SPEC-EMBED-POLICY's no-depth-cap permission for initial contact
> stubs. A stub's own seat interval must end within `MIN_ILLEGAL_LEN_MM` of the designed
> embed depth; if it runs deeper, the stub stays condemned, exactly as today. The
> thin-feature-span check is **not built**, and does not need to be, because nothing invokes
> the permission it guards.

Why this is the honest minimum, in four lines:

* **It is not a loosening for any stub.** Today's rule condemns every stub whose axis stays
  inside more than 0.05 mm past the embed. The proposed rule condemns exactly the same
  seat-interval population. It is at worst identical and never wider.
* **It costs a measured 4 records, 3.42 mm², and zero at the production plan.** Two of those
  four are already illegal under the existing scar-dimension rule anyway. The true cost of
  declining is **one shape**, `t225`, 1.03 mm² — and that shape is a 0.139 mm shell
  perforation which we should not want.
* **It closes E-C without building anything**, which is the only outcome that does not put an
  implementer in the position of building half an owner-ratified rule.
* **It leaves the permission intact in the spec** for the other member families it was written
  for (strut feet, model rung ends). This declines it for one member kind, in one stage, on
  the recorded reason that that member kind no longer needs it.

**If Craig prefers to keep the permission**, then the thin-feature-span check must be built
before Piece D lands, and the design's staged plan gains a stage that is larger than every
other stage put together — for a measured prize of 1.03 mm² on this model. That comparison is
the whole argument.

---

## §9. INTERACTION WITH THE RULED CLEARANCE OVERHAUL

Craig has since ruled the bundled clearance overhaul: an **honest target-specific exemption**,
plus a **two-tier demand — `STANDOFF_MM = 0.6 mm` as the placement-family search margin, and a
final acceptance demand of 0.3 mm.** RULE 1 must state how a landed strut grades under it,
because a landed strut is the extreme case.

> **Provenance caution.** At the time of writing, the overhaul's registration
> (`FINAL_CLEARANCE_GAP_MM = 0.300`, `Measured`, owner decision
> `final-clearance-demanded-gap-owner-field-data`, 2026-08-22) is **in flight in another
> lane's working tree and is not at `f50a45b`**. This section is written against the ruling
> as relayed and against that in-flight text, and **must be re-checked once it lands**.

### §9.1 The quantity the final tier demands, and what it means for a landed strut

The ruling makes the demanded quantity precise, and the precision matters here more than
anywhere else: it is a **true surface-to-surface gap** — the shortest distance from the
arm's own **solid surface** to the model surface, that is, the kernel's axis-to-surface
distance **minus the arm radius at the nearest station**. It is not a centreline distance
and not a radius.

Apply that to a landed strut. Inside its free span the strut is *emerging from* the model at
one end and *entering* it at the other, so its surface touches the model at both ends of the
span by construction, and the minimum true gap over the span is **zero**. Not small — zero.
It is zero for the same reason `contact::mask_extent_mm` has to exist at all
(`model-seated-arms-2026-08-21.md` §1.1): *"at the seat the arm touches the model by
definition, so the gap grows from zero continuously; demanding a finite gap at the point of
departure demands a step discontinuity, which no straight arm can supply."*

**A landed strut is that argument with no middle.** The measured geometry leaves no room for
one:

```
strut total length      0.716 - 2.579 mm   (median 1.382), of which 0.3 mm is inside the model at EACH end
free span (crevice)     0.116 - 1.979 mm   (median 0.782)
free span < 0.6 mm      41 / 120
free span < 0.3 mm      16 / 120
free span >= 2.5 mm      0 / 120
```

At the median the strut crosses 0.78 mm of air with a body 0.35 – 0.59 mm in radius. There is
no station in that span from which a 0.3 mm true gap to *both* crevice walls could be
measured, because the walls are what the strut is standing on.

**Demanding a positive gap there is the #51 error again**, in a new place: a finite clearance
is demanded in a region that is contact by construction. The two-tier ruling does not create
this problem and does not solve it — the exemption does, and it is why the exemption has to be
target-specific rather than a length.

### §9.2 Why the target-specific exemption is exactly the right mechanism

The overhaul's exemption is **target-specific**: a member is exempt from the distance test
only where it is in contact with a target it has **declared**, not by a blanket arc-length
inset. A landed strut declares **two** targets — the island face and the struck face. So:

> **A landed strut's clearance grades against true gaps outside its own two frozen seat
> patches.** The crevice between its declared targets is inside the union of those patches
> and is not distance-tested. What remains distance-tested is clearance to every body the
> strut did **not** declare — a third feature intruding into the crevice, a neighbouring
> support, the model elsewhere.

That residual test is not vacuous. It can fail, and it should. And it is the honest one: the
strut has said what it is touching, so it is graded on everything else.

**Why the exemption has to be target-specific and not a length**, in one measured fact from
the mask-honesty audit (`.git/mask-honesty-evidence`): the retired length-based exemption
excused an arm **0.242 mm inside the model** while refusing a same-axis thinner arm that had
**0.569 mm of real gap**. A length-based exemption on a landed strut would be that failure at
its worst — the strut is *entirely* inside the exempt length, so a length rule cannot
distinguish "seated on the two faces it declared" from "buried in a third one". Declaring the
targets is what makes the difference visible.

### §9.3 What that leaves ungraded, and what replaces it

The exemption means the crevice itself gets no distance test. Three things stand in its
place, and RULE 1 depends on **all three**:

1. **The rim-swept interval evidence (§2.4).** The free span must be measured outside the
   solid on the axis **and all 8 rim rays** — `rays_all = true`. This is signed
   inside/outside evidence over the exact region the exemption covers, which is strictly more
   informative than an unsigned distance would have been. Without it, RULE 1 exempts a region
   nothing has looked at, and that is the one thing a reviewer should refuse.
2. **The scar bound at both ends (§3).** Proved unconditional for a standard-depth foot, and
   it is the exemption's price in exactly the sense `model-seated-arms-2026-08-21.md` §3.1
   Piece C meant.
3. **The residue disclosure (§6).** What the exemption declines to gate on, the report must
   name.

### §9.4 One thing the overhaul makes easier, recorded

Under the two-tier rule, placement prunes at 0.6 mm and acceptance grades at 0.3 mm. A landed
strut has **no arm**, so it never enters a candidate tournament and never pays the 0.6 mm
placement tier at all. The overhaul and RULE 1 do not compete for the same members. That is
worth stating because it means the two can land in either order without a merge hazard in the
clearance path — the shared surface is `contact.rs`, and RULE 1's only requirement there is
that `emerges` is not called on a doubly-seated member (§2.4).

---

## §10. THE STAGED LANDING PLAN

**No new registered constant is proposed at any stage.** Every value is registered:
`DEFAULT_CONTACT_DEPTH_MM`, `MIN_ILLEGAL_LEN_MM`, `MIN_SHOOT_R_MM`, `BALL_FACTOR_FOOT`,
`BALL_FACTOR_STUB`, `A_ABS_MM2`, `KAPPA`, `L_SCAR_MAX_MM`, `PRINT_FLOOR_CHORD_MM`, `TOL_MM`,
`TIP_RADIUS_MAX_LEGACY_MM`, and `SPACING_FLOOR_MM` for disclosure only. Two hardcoded
epsilons are reused rather than registered — `1e-9` for the Z comparison (already at
`cco.rs:4948`) and `FOOT_BREACH_HALF_DEG` / `FOOT_BREACH_CONE_RAYS` (already at
`anchor.rs:32-34`). Both are recorded as existing debt, not introduced.

### §10.1 Blast radius, honestly, before the stages

**This touches initial contact minting, which every model in the corpus goes through.** There
is no narrow version of this change. `build_initial_contacts` runs once per model per re-plan,
on every terminal. Any change to which stubs survive it changes the forest, the digests, and
the emitted artifact for every model that has a condemned stub today.

That is why stage 1 moves no geometry.

Specifically:
* **Moves geometry:** every model with a condemned stub — MEASURED on the Rotting Warhorse
  (18 at the production plan) and on `zombie-3-a` (52 of 92 repair trials), unmeasured
  elsewhere.
* **Moves candidate outcomes but not geometry:** models where landing removes a terminal from
  the coverage-repair queue, changing the repair sequence without changing the final forest.
  `make_overhang_table` is the fixture most likely to move this way; it already seals at
  attempt 0 since `4c74f74`.
* **Does not move:** the area law, `STANDOFF_MM`, every SCAR constant, the minimum tip, the
  brace family, the plate family, hollowing, drilling, lowering.

### §10.2 The stages

**Stage 0 — probe P1 (measure only, no behaviour, revertable).**
Extend the `rw-embed-split` probe to record, at each interval's **entry** station: the
projected face id, its outward unit normal, `cosang_entry`, and the exact signed distance `d`
from the proposed foot point to the surface. Also record the rim-ray intervals
(`rays_all = true`) so the free span is measured on the capsule, not the centreline. Run on
the Rotting Warhorse **and on `zombie-3-a`** (probe P2). Print-only; reverted after the run.
**Gate G0:** the `cosang_entry` data reproduces §4's conservative verdicts or improves them,
and the rim-swept free spans do not close any of the 120 crevices below
`MIN_ILLEGAL_LEN_MM`. If rim rays close a material number of crevices, RULE 1's recovery
table shrinks and this design must be re-priced **before** stage 1.

**Stage 1 — compute the landing, record it, land nothing (no geometry moves).**
`initial.rs` computes the RULE 1 candidate for every stub it condemns and records it on the
existing `CondemnedInitialStub` row: strike station, chord, free span, landed length, both
radii, both scar verdicts, build-order verdict, and the §6.2 disclosure fields. The stub is
**still quarantined**. `BlockedCause::StubReentry` still fires. The emitted artifact is
**bit-identical on every model** — that is the gate.
**Gate G1:** (i) every corpus digest bit-identical, including `make_toed_slab`'s
`4_190_462_360_230_124_963`; (ii) the recovery table is produced for the whole corpus, not
just the Warhorse; (iii) the rim-swept sweep stays inside `PenetrationBatchLimits` — measured,
not assumed, since it is 9× the ray work.
**This stage is the whole reason the plan is staged.** It converts this document's arithmetic
into a corpus measurement at zero risk, and Craig's authorization of stage 2 can be given
against real numbers instead of one model's.

**Stage 2 — land it (geometry moves; OD-8 stop-and-report).**
The stub whose landing passed all six tests is shortened to `L'`, its rootward end is seated
in the struck surface, and it is **not** quarantined. Minted as `MemberKind::Strut` so that
`contact::seating_of` (`contact.rs:104-110`) classifies its `Rootward` end as `Seated` with no
change to that function — a landed stub *is* a model-to-model strut, and the family change is
real, so the digest movement it causes is attributable rather than incidental. The alternative,
`MemberKind::Other("landed_stub")`, is cleaner to read and moves the same digests; recorded,
not chosen. **Implementation check, not asserted here:** confirm no lowering, emit or brace
path keys on `MemberKind::Stub` in a way that a `Strut` would miss.
**Gate G2:** no model that emits today stops emitting; every moved digest carries a from→to
attribution; `make_toed_slab` bit-identical or attributed; island 74 covered on the Rotting
Warhorse; `StubReentry` reports zero on the Warhorse production plan.

**Stage 3 — decline the deep-seat permission (a tightening; §8).**
A stub whose own seat interval runs past `δ + MIN_ILLEGAL_LEN_MM` stays condemned. Measured
cost: 4 records, 0 at the production plan. Lands with or just after stage 2; it is what makes
stage 2's exemption honest.

**Stage 4 — RULE 2 (re-aim), per SPEC-P8-RELOCATE §3.1 R1, with §5.2's simplification.**
Behind stage 2, because until stage 2 lands we do not know how many stubs still need
steering — and the measurement says the answer is 3 at the production plan, not 18.

**Stage 5 — the tip-embed defect (§7).** A separate lane. Not bundled, not blocked by any of
the above, and worth 45 further landings (89.926 mm²) once repaired.

### §10.3 Which pins move, and how

| pin | file:line | what it pins | movement |
| --- | --- | --- | --- |
| `terminalward_embed_classifies_clean_transit_and_buried_boundaries` | `initial.rs:1897` | the scalar overlap rule, including a `{0.2, 0.6}` case called "transit" | **MOVES at stage 2.** That case is a strike at `s = 0.2`, which is *inside* the designed embed, so it is a deep seat and stage 3 condemns it. The test becomes the classification pin for seat / strike / deep plug |
| `real_stacked_obstruction_accepts_cutoff_and_condemns_above_it` | `initial.rs:2461` | the 0.05 mm cutoff against a real `NativeSurface` obstruction below a downward stub | **MOVES at stage 2, and it is the acceptance test.** The obstruction is exactly a strike: real material, below the contact, thick. Today the stub is condemned; under RULE 1 it lands on it. **This must be re-derived under the six tests and its expectation re-argued, never edited** |
| `real_table_build_materializes_exact_registered_stub_geometry_and_ledger` | `initial.rs:2024` | the exact minted `p1` from `position + unit * span` | **UNCHANGED** for stubs that do not strike; a landed stub needs its own sibling assertion |
| `buried_free_ends_quarantine_and_retain_both_ledger_records_and_loads` | `initial.rs:2562` | quarantine keeps the ledger row and the abandoned load | **UNCHANGED** — the residue still quarantines |
| `a_route_whose_every_trial_was_quarantined_reports_task_52s_class` | `runner.rs:10831` | `BlockedCause::StubReentry` construction | **UNCHANGED as a unit contract.** The class survives for the residue. On the Warhorse it should report zero, which is the behavioural signal |
| `contact.rs::planar_seat_round_trip_returns_exactly_margin_from_the_kernel` | `contact.rs` tests | PR-2 against the real kernel | **UNCHANGED** — RULE 1 does not touch the mask |
| `retired_formula_guard.rs` | source-text bans over `brace/cco/rewire/initial` | the flat-inset idiom | **EXTENDED** — add a ban on any *new* arc-length inset in `initial.rs`, so RULE 1's exemption cannot be re-implemented as a mask |
| `make_toed_slab` digest `4_190_462_360_230_124_963` | three pin sites | the whole-change regression pin | **MEASURE.** Bit-identical at stage 1 by construction. At stage 2, bit-identical if the slab has no condemned stub, otherwise attributed |
| `make_overhang_table` | integration | emits today | **must still emit.** The 2026-08-13 acceptance bar, unchanged |

### §10.4 Revert-proof tests

Per the standing rule that a test must never re-implement production logic, and that every
new guard owes a sabotage proof:

| id | sabotage | required consequence |
| --- | --- | --- |
| **S1** | make the landing function always return `None` | a named fixture flips from LANDED back to CONDEMNED, and `StubReentry` fires. Proves the landing is what recovers it, not something else |
| **S2** | remove test (a) (the chord ≥ 0.3 mm check) | a fixture whose struck feature is 0.15 mm thick lands, and a separate assertion catches the foot poking out the far side. Proves (a) is load-bearing, and pins the RULE 2 boundary from the illegal side |
| **S3** | remove test (f) (build order) | island 74's `t189` shape lands with its foot **above** its contact. This is the *only* proof that (f) bites, because nothing else in the engine is Z-aware (§3.6) |
| **S4** | set `rays_all = false` | a fixture whose axis crosses air but whose flank does not must stop being refused. Proves the rim sweep is load-bearing and not decoration. **Requires a purpose-built fixture; none exists** |
| **S5** | grade the landed foot with the *original* 2.8 mm frustum instead of `L'` | `r_x` is read at the wrong station and at least one scar verdict changes. Proves §3.3's caveat is respected |
| **S6** | allow the deep-seat permission (undo stage 3) | the `t225` shape — a 0.139 mm shell perforated at `cosang = 0.164` — becomes legal. **This is E-C's regression, made into a test.** It is the disproof `D2` that `model-seated-arms-2026-08-21-amendment.md` §D records as NOT BUILT |

**S4 and S6 are the two that must not be skipped.** S4 guards the region the clearance
exemption stops looking at. S6 is the only durable record of why the deep-seat permission was
declined.

**Non-vacuity first, in every case.** Each sabotage test asserts the *un*-sabotaged fixture
reaches the state under test before it asserts the sabotage changes it — the shape
`cco::tests::a_grazing_model_anchor_is_refused_not_admitted_with_infinity` already uses.

---

## §11. FACTS THAT DO NOT EXIST, AND THE PROBES THAT SETTLE THEM

Sized before building, per the FU-82 lesson.

| gap | why it matters | minimal experiment | prize |
| --- | --- | --- | --- |
| **P1 — the incidence at the strike.** The probe recorded `cosang` at each interval's *end*, not its *start*, so the foot's `cosang_entry` and its exact signed distance `d` are unknown | It is the only input rule 3 needs and the only reason §4's verdicts are conservative rather than exact | Extend the existing probe by one projection per interval start. Print-only, reverted after the run, ~7 s per pass. **Zero pipeline passes** | Turns every verdict in §4 from a lower bound into a number. Can only improve them (§3.5) |
| **P1b — the rim-swept free span.** The sweep is axis-only (`rays_all = false`) | RULE 1 is not honest without it (§2.4). It could close crevices and shrink the recovery table | The same probe with `rays_all = true`. Measure the cost against `PenetrationBatchLimits` at the same time | Gate G0. Decides whether the 120 is really 120 |
| **P2 — `zombie-3-a`'s 52.** Its 52 stub-reentry quarantines have no interval data | It is 56 % of that model's repair-trial waste, and it is the evidence that this class is not one model's quirk | Run the same probe on `zombie-3-a`. It already has a bit-identical reference artifact to check against | Grades the second model. Extends or refutes the 58 % recovery rate |
| **P3 — the flush-cutter.** No tool model exists anywhere (§6.1) | Decides whether "permanent" is a real word for these struts or a conservative label | Not a print. Measure blade thickness, included angle, and the narrowest crevice the tool actually enters, on an existing printed part. One afternoon | Escalation **E-B**. Retires or grounds a `reach` flag that is currently prose |
| **P4 — why 52 stub tips sit in air (§7)** | 25 % of the population, and it is a contact-quality defect, not a transit one | P1 records the face normal at `p0`, which settles the averaged-normal hypothesis at no extra cost | 45 further landings, 89.926 mm² |
| **E-A — does the struck feature carry the load onward?** | Not measurable by probe; it is a doctrine question (§2.5) | none — it is Craig's ruling, informed by stage 1's instance count | Decides whether tasks #64 / #65 are blockers or neighbours |

---

## §12. ESCALATIONS, COLLECTED

1. **The permanence of every landed strut (§6).** 120 of 120 crevices are narrower than the
   only removal-ergonomics number the project has. This is the design's largest owner-visible
   consequence and it is a pricing decision, not a technical one.
2. **Declining the deep-seat permission (§8.4).** Closes E-C. Measured cost 1.03 mm² of real
   exposure on this model, and that exposure is a 0.139 mm shell perforation. **The design
   recommends declining.**
3. **E-A remains open and this design adds instances to it (§2.5).** Not opened by this
   design. The addition is counted at stage 1 before it is committed.
4. **E-B (the flush-cutter measurement) gets materially larger (§6.4).** Nine permanent
   bridges on one mini is a different exposure from a support family that *might* be hard to
   remove.
5. **`emerges` is not applicable to a doubly-seated member (§2.4).** The design replaces the
   proxy with a direct measurement at the same registered threshold, for that member class
   only. It is an owner-visible change to a predicate that landed six days ago, and it must
   not be read as a general relaxation.
6. **Build order becomes a member rule for the first time (§3.6).** The engine has never had
   one. Two of 120 measured landings need it. It reuses `cco.rs:4948`'s exact inequality, but
   promoting a candidate-step check to a member rule is an owner-visible move.
7. **`rays_all = true` is a real cost, not free (§2.4).** 9× the ray work in the initial
   sweep. Gate G1 measures it. If it does not fit inside `PenetrationBatchLimits`, this design
   needs a different instrument and Craig should hear that before stage 2, not after.
8. **`MemberKind::Stub` → `MemberKind::Strut` moves member digests (§10.2 stage 2).** The
   family change is real, so the movement is attributable, but it is a digest movement and OD-8
   applies.

---

## §13. WHAT A REVIEWER SHOULD CHECK FIRST

1. **Re-derive the scar theorem (§3.2) from SPEC-EMBED-POLICY's formulas alone.** If
   `A_shaft ≤ 2·r_x·T` and `L_scar ≤ T + 2·r_x` are not unconditional upper bounds, §3 is
   wrong and the recovery table with it. One document, ten minutes.
2. **Confirm the sweep is axis-only.** `initial.rs:1358-1370` passes `rays_all = false`;
   `queries.rs:90` makes `n_offset_rays` inert when it is. Then check that every one of the
   1,926 probe interval rows carries `ray = -1`. If rim rays *were* swept, §2.4's requirement
   is already met and gate G0 is free.
3. **Check that RULE 1 is an admission and only an admission for stubs.** Every stub that
   survives today has no crossing beyond its embed, so it has no strike and RULE 1 does not
   see it. If a stub that is legal today can be *changed* by RULE 1, the change is not an
   admission and inherits the FU-87 hazard.
4. **Check that test (f) bites.** Sabotage S3. Two of 120 is not many, and a rule that
   refuses nothing is a comment.
5. **Re-derive the E-C arithmetic (§8.3) from the raw JSONL.** Specifically that `t225`'s seat
   interval has `exited = true` and that `0.849 × 0.164 = 0.139 mm`. That single number is the
   whole argument for declining the permission.
6. **`make_overhang_table` still emits, and stage 1 is bit-identical everywhere.** If stage 1
   moves one digit of one digest, the stage boundary is wrong.
