> **Evidence record — non-normative.** Relocated from `docs/engineering-journal/derivations/low-point-reach-2026-08-19.md` in the 2026-08 docs restructure; content preserved verbatim below.

# When is a low point adequately carried? A derivation, and what the models actually show

Task #62. Written 2026-08-19 against `develop` @ `b8edcb8`. Adversarially reviewed the same day
(verdict SOUND-WITH-CHANGES) and **corrected here**: this is the corrected copy, and it is the copy
Craig accepted. It is the recorded basis of a registry retirement, so it is committed rather than
left in a scratchpad.

**Owner ruling, 2026-08-19 (Craig).** Ship the derived criterion at the **conservative end**. A
non-birth local minimum is adequately carried if and only if its reach is at most `K · h` with
`K = sqrt(X_ALLOWED / 6)` — the peel-front-concentrated coefficient — and `h` the registered layer
height. At the registered values that is **0.0707 mm of reach**. Both inputs are read from the M0
registry at the site. Retire `MINIMA_SUPPORT_AREA_TIPS` entirely.

**Shipped in** `crates/plinth-support/src/perception.rs`, `find_local_minima`.

---

## 0. Corrections applied from the adversarial review

Eight corrections were required before signing. All eight are applied below, none softened.

| # | What it required | Where it landed |
|---|---|---|
| R1 | Replace the sag dismissal with the strain form; drop the 3,472× and the 7 nm | §2.7 rewritten, §5.2 re-labelled **marginal** |
| R2 | Withdraw "tilt-peel does not enter" and "peel uniformity: error none"; state the allowance as a range and ship the concentrated end | §2.3, §2.5, §2.6, §2.4 |
| R3 | Move "root fully clamped" out of the defended-simplification table into open debt, naming interlayer cleavage | §2.5 row removed, §5.1 added |
| R4 | Withdraw the "wide ledge on a narrow stalk" claim, or add the width term | §3.4 rewritten, §5.3 added |
| R5 | Add the missing connectivity requirement and the corresponding gap | §4, §5.4 added |
| C1 | Rewrite the "57/57 agree" reading — it is a metric that could not fail | §3.1, §3.3 |
| C3 | Fix the tip wording: a default is not a minimum, and 0.100 mm diameter is below the owner-authorized minimum | §1, §2.3, §6 |
| C5 | Do not let a guessed 64% carry the coupon-plate recommendation | §2.4, §5.6, §6 — **with a stated amendment**, see §5.6 |

Three recommended (not required) corrections are also applied because they touch the same
sentences: **C2** — two significant figures, and a beam-theory-validity row in §2.5; **C4** — §1
strengthened to the by-construction argument; **C6** — §2 now says which coupon `X` comes from and
why.

---

**Terms, defined once.**

- **Low point (local minimum)** — a patch of the model surface that is locally the lowest thing
  around it while the piece prints. `find_local_minima` (perception.rs) finds them as welded mesh
  vertices no higher than every neighbour, then floods outward for two layer heights to get the
  whole flat patch (the *region*).
- **Birth minimum** — a low point that starts a brand-new island: at that height a new, separate
  blob of cured resin appears with nothing beneath it anywhere. Always supported. Not in question.
- **Non-birth minimum** — a low point that is joined to material the printer has already made.
  This is the one in question.
- **Carried** — in the rule being replaced, "there is already cured material directly under me."
- **Peel** — every layer is cured against the clear film at the bottom of the vat and then pulled
  off it. That pull is the load that breaks parts during printing.
- **Peel front** — on a tilt-peel machine the film does not let go of the whole layer at once. A
  line of separation travels across the layer. That line is the peel front.
- **Green resin** — resin that has been cured by the screen but not yet post-cured. It is weak.
  All strength numbers here are green.
- **Reach** — how far the new material at a low point sticks out sideways past the material that
  is already holding it. Measured in mm.
- **Root** — where the overhanging shelf joins the material that is holding it.
- **Failure strain (`ε`)** — how much a material stretches, as a fraction of its own length, at the
  moment it breaks or at the moment it reaches its allowed stress. Written as a percentage.
- **Cleavage** — pulling a glued joint apart by prying at one edge rather than by pulling the whole
  joint straight apart. Cleavage always breaks a joint at a much lower total force.
- **Layer height `h`** — 0.050 mm. `LAYER_HEIGHT_DEFAULT_MM`, **MEASURED** (plate 6, 2026-07-30).
- **X_ALLOWED** — the registered area law: a section of green resin may carry at most 12 mm² of
  model area per 1 mm² of its own cross-section. **MEASURED**, `Derived` evidence grade:
  `floor(lower(X_STAR_CONE)/2) = floor(25.00/2) = 12`, from `COUPON-PLATE-5-compliant-vs-stiff`
  (`crates/plinth-core/src/constants/evidence.rs`). The 25.00 is the ratio at which a real arm
  snapped; the /2 is the design margin already committed to.
  **Which coupon, and why (C6).** The registry holds a second reading of the same law,
  `X_STAR_CYLINDER = (19.91, 30.48)` from plate 6, whose lower bound would give
  `floor(19.91/2) = 9` and an allowance of 0.0612 mm. The registry's own note demotes it to a
  margin check (`19.91/12 = 1.66×`) and commits to `X_ALLOWED = 12` as the derived constant. This
  derivation follows that commitment rather than re-opening it. The choice is recorded, not
  assumed away: the cone coupon is the committed one, and the cylinder reading is 1.66× looser,
  so the committed number is the conservative of the two.

---

## 1. The headline, before anything else: the premise of this task is wrong

The task was set on the understanding that lowering the support tip radius 0.200 → 0.050 mm
(0.400 → 0.100 mm diameter) loosened the low-point yardstick 16× and silently dropped **9 supports
on Deinonychus and 1 on Crawler Zombie**. That is not what happened.

**Naming, corrected (C3).** `tip_radius_default_mm = 0.200 mm radius / 0.400 mm diameter` is a
registered **Placeholder default**, read through `.diagnostic()`. It is not a minimum. The
registered minimum is `TIP_DIAMETER_MIN_ALLOWED_MM = 0.150 mm diameter`
(`TIP_RADIUS_MIN_ALLOWED_MM = 0.075 mm radius`), an owner decision whose note says in as many words
"do not substitute the 0.200 mm default". Task #61's proposed 0.100 mm diameter / 0.050 mm radius
is therefore **below the owner-authorized minimum of 0.150 mm diameter** *as `develop` stands*.

That conflict is **already closed** on the parked `feature/tip-min` branch, which is not merged into
`develop` and is not this task's to touch: Craig superseded the validator minimum on 2026-08-19,
`TIP_DIAMETER_MIN_ALLOWED_MM` 0.150 → 0.100 mm diameter (`TIP_RADIUS_MIN_ALLOWED_MM` 0.075 →
0.050 mm radius), on the grounds that he prints 0.100 mm diameter tips today and a desk number
should not refuse geometry the machine demonstrably renders. Recorded here so a reader of this
document does not re-raise a settled question. The naming correction stands regardless of the
values: a **default** is not a **minimum**, and this document had called one the other.

**MEASURED.** `find_local_minima` was instrumented in a throwaway worktree, every
non-birth minimum dumped with its carried area and its true reach, and the same two models run with
the tip default at 0.200 mm radius / 0.400 mm diameter and at 0.050 mm radius / 0.100 mm diameter,
same binary otherwise:

| | Deinonychus | Crawler Zombie |
|---|---|---|
| support records, tip 0.200 mm radius / 0.400 mm diameter | 1059 | 380 |
| support records, tip 0.050 mm radius / 0.100 mm diameter | 1004 | 375 |
| non-birth minima examined | 4 | 5 |
| minima whose verdict changed | **0** | **0** |

The two minima dumps are **byte-identical** between the two builds. The record counts reproduce the
judge's earlier pre/post runs exactly, so this is the same phenomenon, correctly attributed for the
first time.

Why no minimum moved: the yardstick is `minima_support_area_tips × π × tip_radius²` — 0.1257 mm² at
0.200 mm radius, 0.00785 mm² at 0.050 mm radius. A minimum flips only if its carried area lands
**between** those two numbers. Across 12 models, **57 non-birth minima** were measured. Their
carried areas are 0.000 mm² (one case) and then 1.777 – 200.8 mm². Nothing is anywhere near the
band. The nearest live value is 14× above the *old*, looser threshold and 226× above the new one.

**So the 55 and 5 missing support records come from somewhere else** — smaller terminals change the
junction solve and the segment mix downstream, not the low-point set. That is a separate question
and it was not chased. What is settled is that the low-point rule did not do it.

**The tip-area test is inert by construction, not merely on this corpus (C4).**
`carried_outer_area` returned the signed area of the whole **outer loop** of the slice solid
containing the point. The test therefore compared *an entire cross-section's area* against *a
support tip disc's area*. For the threshold to bind, the containing cross-section would have to be
under about 0.4 mm across — the model would have to be thinner at that point than the support tip
that would carry it. That is a scale mismatch of roughly three orders of magnitude for any
printable feature, on any model, not just these twelve.

**The real defect in the replaced rule is therefore not that it was loosened. It is that it never
decided anything, and could not.** What actually decided was the boolean underneath it: *is the
minimum's representative vertex inside solid material one layer down, yes or no.* And that boolean
measures the wrong thing, which is what the rest of this document is about.

---

## 2. The mechanics

### 2.1 What is physically happening at a non-birth low point

The printer builds upward one 0.050 mm layer at a time. At the height of the low point, a patch of
new resin cures with **nothing underneath it** — that is what makes it a low point — while being
joined sideways, within that same layer, to resin that *is* sitting on the previous layer. Then the
plate lifts and the film peels off. The peel pulls on the new patch. The pull has to travel
sideways through the join into the carried material.

So the object under load is a **shelf**: a tongue of green resin, one layer thick, sticking out
sideways from a root, with pressure over its face. That is a cantilever. The failure is bending at
the root, not tension.

### 2.2 Bending beats tension by a wide margin — this is why the tip-area yardstick is the wrong shape

For a tongue of reach `L`, thickness `t`, any width, carrying total force `F`:

- pull-through-the-neck stress `σ_tension = F/(W·t)`
- bending stress at the root `σ_bend = c·F·L/(W·t²)`, `c` = 3 for a force spread evenly over the
  shelf, `c` = 6 for a force concentrated at the free edge

Their ratio is `c·L/t`. With `t` = one layer = 0.050 mm, bending exceeds tension as soon as the
reach exceeds `t/c` — 0.017 mm at worst. Every real case is bending-governed: the ratio is 6× at
the optimistic limit (`c` = 3, `L/t` = 2.0) and 8.5× at the shipped limit (`c` = 6, `L/t` = 1.41).

An **area** test — which is what `carried ≥ N × π r_tip²` is — is a tension test. It cannot see the
mode that actually breaks these features. **INFERRED** from the two formulae; the formulae
themselves are standard beam theory.

### 2.3 The derivation

A shelf of reach `L` (measured from the root), thickness `t`, width `W`, root fully restrained,
carrying a total peel force `F = p·W·L`.

```
root bending moment          M = F · L / n        n = 2 (force spread evenly)
                                                  n = 1 (force at the free edge)
root section modulus         Z = W · t² / 6
root bending stress          σ = M / Z = c · p · L² / t²      c = 6/n
```

The width `W` cancels — but only when the shelf and its root are the same width; see §5.3. Now the
allowable. `X_ALLOWED = 12` says a green section may be stressed to whatever `12 × p` is — that is
exactly what the area law means, restated as a stress: `σ_allow = X_ALLOWED · p`. Set
`σ ≤ σ_allow`:

```
c · p · L² / t²  ≤  X_ALLOWED · p
```

**The peel pressure cancels.** Both the load and the allowable are proportional to it, because
`X_ALLOWED` was measured as a *ratio* on a coupon and never as a stress in Pascals. What is left is
a pure number:

```
    L_max  =  t · sqrt( X_ALLOWED / c )
    K      =  sqrt( X_ALLOWED / c )      dimensionless
```

**Which `c`? The distribution of the peel force does not cancel, and this is R2's correction.**
Peel is by definition a force concentrated in a narrow zone at the separation line — that is what
`SATURN_LEADING_RELEASE_EDGE = NegativeY` and `SATURN_RELEASE_FRONT_SWEEP_DIRECTION = PositiveY`
describe. As the front travels across a shelf its distance from the root sweeps from 0 to `L`, and
the root moment peaks when the line reaches the free edge. So:

| Load model | `c` | `K` | Allowance at `h` = 0.050 mm |
|---|---|---|---|
| spread evenly over the shelf | 3 | 2.0 | 0.100 mm reach — **optimistic bound** |
| concentrated at the peel front, front at the free edge | 6 | 1.41 | **0.0707 mm reach — shipped** |

The honest statement is that the criterion is a **range**, `K ∈ [1.41, 2.0]`, allowance
0.0707 – 0.100 mm, with the concentrated end the more physical default. **Craig ruled the
conservative end**, so the engine ships `c = 6`:

> ### **A non-birth low point is adequately carried if its reach is at most `K · h` = 0.0707 mm, with `K = sqrt(X_ALLOWED / 6)` = 1.41.**

A second, smaller way the same geometry enters, which the first draft also missed: the
cancellation above cancels the coupon's `p` against the shelf's `p`. If peel pressure varies with
plate position then those are two different numbers and plate position re-enters as
`sqrt(p_coupon / p_shelf)`. Craig's settled Y ruling — front-to-back is ≤ 1 rung, randomize over Y
rather than measure it — bounds that, so the conclusion survives. But "plate position cannot change
this limit" is *not* what was proved.

For reference in tip language, since tips are the only other thing in this pipeline measured in
these units: 0.0707 mm of reach is the radius of a 0.141 mm diameter disc. On `develop` the
registered tip **minimum** is 0.075 mm radius / 0.150 mm diameter and the registered tip
**default** is 0.200 mm radius / 0.400 mm diameter; both move on the parked `feature/tip-min`
branch (§1). The comparison is offered only to give the number a familiar scale — after this change
**no tip number can move a low-point verdict**, which is the specific accident that started this
task.

### 2.4 Two consequences of the same number, which are worth having

**A shelf held at both ends (a bridge) tolerates more.** Clamped both ends, peel front at midspan,
worst moment `F·S/8`: `S_max = 4·t` = 0.200 mm span, 2.83× the single-rooted reach. Use the
single-rooted form for low points — a low point's region is generally rooted on one side only.

**The same number is a self-supporting angle.** Take a straight sloped underside leaning `θ` above
horizontal. Each layer it advances sideways by `h/tanθ`. After `n` layers the shelf reaches
`L = n·h/tanθ` and its root is `t = n·h` thick, so `L/t = 1/tanθ` — *layer-invariant*. The stress
is largest at `n = 1` and falls after, so the first layer governs and the criterion becomes:

```
    tan θ  ≥  sqrt( c / X_ALLOWED )
      c = 3:  tan θ ≥ 0.500  →  θ ≥ 26.6° above horizontal, normal nz < −0.894
      c = 6:  tan θ ≥ 0.707  →  θ ≥ 35.3° above horizontal, normal nz < −0.816   (shipped end)
```

Today `OVERHANG_ANGLE_DEG = 45.0` gives `nz < −0.707`. **The legacy 45° rule flags more faces as
overhangs than this physics demands** — but by how much is a *guess*, not a measurement. On an
isotropic surface the flagged area fraction would fall from `(1−0.707)/2 = 14.7%` to
`(1−0.816)/2 = 9.2%` at the shipped end, a **37% reduction**; at the optimistic end it would be
`(1−0.894)/2 = 5.3%`, a 64% reduction. Both figures are **INFERRED** under an **ASSUMED** isotropic
surface. Neither is measured. See §5.6 — that guess is not allowed to spend a coupon plate.

`OVERHANG_ANGLE_DEG` is out of scope for task #62. It is the same number, and it is where the money
is (§6).

### 2.5 Simplifications, each stated and defended

| Simplification | Why it is acceptable | Direction of error |
|---|---|---|
| Beam formula, not plate | For a shelf rooted along one edge, a wide plate in cylindrical bending has the *same* root moment per unit width. Only the transverse stress differs (`ν·σ`). | none for this geometry |
| Worst case is the shelf's own first layer | The sloped-underside case in §2.4 peaks at `n = 1` and decreases monotonically. | none |
| One layer height matters (`h` at the minimum's own z) | The shelf is one layer thick when it is first loaded, and it is never thinner. | none |
| Peel pressure **magnitude** uniform over the shelf | The magnitude cancels; see §2.3. The **distribution** does not cancel and is no longer assumed away — it is the `c = 6` choice. | none in this branch |
| `X_ALLOWED` transferred from an arm coupon | The arm failed across **layer bonds** (build-direction tension). The shelf root is **in-plane**, which is stronger. | conservative in this branch; but see §5.1, which is a different section |
| Reach measured on the region's sampled points | Cheap, and the flood-filled region is exactly the near-flat down-facing patch. A slice-based measurement is stricter; see §4. | possibly unconservative, small |
| **Euler–Bernoulli beam theory at `L/t ≈ 1.4`** | There is no better closed form available, and the qualitative result (bending dominates tension) is robust. But beam theory is a slender-member approximation, trustworthy above `L/t ≈ 5–10`, and by St. Venant's principle its stress solution is invalid within about one thickness of a restrained end. **The whole structure here is 1.4 thicknesses long, so essentially all of it lies inside the root disturbance zone.** | **unknown, tens of percent** |

That last row is why nothing in this document is quoted to four significant figures. `K = 1.41`,
`0.0707 mm`, `35.3°` — two or three figures, and the third is carried only so the arithmetic can be
checked, not because it is earned.

**"Root fully clamped" has been removed from this table.** It is not a defended simplification. It
is open debt; see §5.1.

### 2.6 Does the Saturn tilt-peel geometry enter? Yes — the first draft said no, and that was wrong

`SATURN_LEADING_RELEASE_EDGE = NegativeY`, `SATURN_RELEASE_FRONT_SWEEP_DIRECTION = PositiveY` and
`SATURN_RELEASE_FILM_THICKNESS_MM = 0.127` are registered and read by nothing in the support crates.
The first draft argued they could not matter because `p` cancels. That argument covers only the
*magnitude* of the peel pressure. The *concentration of that pressure at the travelling separation
front* is precisely what the leading-edge and sweep-direction constants describe, and it does not
cancel — it is the factor of two between `c = 3` and `c = 6`, and therefore the 41% between
`K = 2.0` and `K = 1.41`. The shipped criterion is the one that takes that concentration seriously.

What still does **not** enter is plate position: a model at the front of the plate and a model at
the back get the same admissible reach, up to the `sqrt(p_coupon/p_shelf)` term of §2.3, which
Craig's settled Y ruling bounds at ≤ 1 rung. **Recorded, not chased.**

### 2.7 The other failure mode: sag. It does not govern — but by about a factor of two, not by three thousand

A shelf can also simply bend too far, curing out of position, without breaking. The first draft
computed a crossover modulus of 144,000 Pa, declared a 3,472× margin, and quoted a deflection of
7 nanometres. **Those numbers are withdrawn.** They were computed from `PEEL_PRESSURE_PA = 3000` and
`MODULUS_GREEN_GPA = 0.5`, and those two registry values cannot both be right: together with
`X_ALLOWED` they assert an allowable stress of `12 × 3000 = 36,000 Pa` and therefore a strain of
`36,000 / 0.5×10⁹ = 7.2×10⁻⁵`, i.e. **0.0072%**. Nothing solid fails at 0.0072% strain — window
glass manages about 0.1%, and a green photopolymer is a soft rubbery solid that manages several
percent to tens of percent. The registry is internally inconsistent by two to three orders of
magnitude here; that is filed as **task #63** and is not this task's to fix.

Set the sag question up properly and both offending constants cancel out of it, exactly as the peel
pressure did. Substitute `p = σ·t²/(c·L²)` into the matching cantilever deflection — `1.5pL⁴/(Et³)`
for the evenly spread load, `4pL⁴/(Et³)` for the load at the free edge:

```
    c = 3 (spread):        δ / t  =  0.500 · (σ/E) · (L/t)²  =  0.500 · ε · K²
    c = 6 (concentrated):  δ / t  =  0.667 · (σ/E) · (L/t)²  =  0.667 · ε · K²
```

Take "too far" as half a layer, `δ = t/2` — the next layer then lands more than half a layer off.
Sag governs when:

```
    c = 3:  ε ≥ 1.00 / K²  =  3.0 / X_ALLOWED  = 0.25    (25% strain at the allowable stress)
    c = 6:  ε ≥ 0.75 / K²  =  4.5 / X_ALLOWED  = 0.375   (37.5% strain at the allowable stress)

    equivalently, at the un-margined rupture ratio X_STAR_CONE = 25.00:
      c = 3:  elongation at break ≥ 3.0/25 = 12%
      c = 6:  elongation at break ≥ 4.5/25 = 18%       (shipped end)
```

No pressure, no modulus, no layer height — just the area law and the load model.

But look at what that says. **Green rigid photopolymers commonly break somewhere in the 5–20%
elongation range.** 18% sits inside that range. So the honest statement is:

> **Sag does not govern this criterion provided green elongation at break is under about 18%. That
> is plausible but not established, and it is roughly a factor of two away, not a factor of 3,472.**

The missing number is green elongation at break. It is a resin datasheet lookup or a trivial
coupon — not a plate. **INFERRED**, from registry values and standard beam deflection.

---

## 3. Evaluation on real models

**MEASURED.** `develop` @ `b8edcb8` instrumented in a throwaway worktree, `gate` profile, dumping
every non-birth minimum reaching the adequacy filter: position, carried area, reach (max over the
region's sampled points of the distance to the carried footprint one layer below, 0 if inside),
region size, and face normal. 12 models attempted, 8 produced minima (4 failed at later stages for
reasons unrelated to this filter). **57 non-birth minima total.**

Allowance under the shipped rule: `K·h = 1.41 × 0.050 = 0.0707 mm`.

### 3.1 The two low points that are not trivially inside the carried footprint

| Model | z (mm) | carried area (mm²) | reach (mm) | reach / 0.0707 mm | face normal z | replaced verdict | **shipped verdict** |
|---|---|---|---|---|---|---|---|
| Crawler Zombie, minimum 13 | 0.855 | **0.000** | **0.1043** | **1.48** | −0.981 (down-facing) | support demanded | **support demanded** |
| Thorny Vegepygmy, minimum 17 | 2.355 | 5.579 | 0.0200 | 0.28 | −0.974 (down-facing) | adequate | **adequate** |

The Crawler Zombie case is the interesting one: a 0.20 × 0.18 mm down-facing patch, half of it
hanging past the edge of the solid below, reaching 0.1043 mm — **48% over** the shipped allowance,
and 4% over the optimistic one. Both rules demand a support, and they demand it for different
reasons: the replaced rule because the representative vertex happens to sit outside the footprint,
the shipped rule because the shelf is too long.

**This is not a validation, and the first draft read it backwards (C1).** Calling a 4% overshoot
against the optimistic coefficient "a satisfying validation" inverted what it is: that case is
**the single tightest call in the entire corpus**, it was being decided by 4% using a coefficient
that is itself uncertain by 41%, on a beam formula that §2.5 shows is at its least reliable at
exactly this aspect ratio. Under the shipped concentrated model the same case reads 148% and the
coincidence evaporates. What can be said is that the case demands a support at **either** end of
the `K` range, so the shipped verdict does not depend on the coefficient choice.

### 3.2 The other 55

All 55 have **reach = 0.000 mm** — every sampled point of the region is inside the carried footprint
one layer below. These are dimples and concave saddles in the side or belly of solid material, not
overhangs; 55 of the 57 have an **upward**-pointing normal (`nz` from +0.71 to +1.00), which is the
signature of a bowl bottom, not a shelf. Only the two rows in §3.1 face downward at all. Their
carried areas run 1.777 – 200.8 mm².

Both rules call all 55 adequate. Under the shipped rule they are adequate with **zero** reach.

### 3.3 Score — and what it is worth, which is less than it looks (C1)

| | agree | disagree |
|---|---|---|
| shipped rule vs replaced rule, 57 minima | **57** | **0** |
| minima whose verdict the tip-radius change moved | — | **0** |

**"57 out of 57 agree" is a metric that could not have failed.** The corpus contains exactly three
distinct reach values, and here is everything it constrains:

| Minimum | reach (mm) | `h` (mm) | reach / `h` | constrains `K` to |
|---|---|---|---|---|
| 55 records | 0.000000 | 0.050 | 0.000 | nothing — adequate for any `K ≥ 0` |
| Thorny Vegepygmy 17 | 0.019988 | 0.050 | 0.400 | `K ≥ 0.400` |
| Crawler Zombie 13 | 0.104343 | 0.050 | 2.087 | `K < 2.087` |

**Any `K` in [0.400, 2.087) reproduces all 57 verdicts — a 5.2× window.** In `X_ALLOWED` terms
(`X = 6K²` at the shipped `c`) that is any value from about 0.96 to 26.1: most of the plausible
range would have scored 57/57.

The honest sentence is therefore: **the corpus constrains `K` to a factor-of-five window, the
derived value falls inside it, and the corpus does not test the value.** That is still the result
recommendation 1 needs — it means shipping is safe — but it is not evidence that the number is
right.

What the change *is*, is **grounded**: 0.0707 mm traces to `X_ALLOWED` (measured, plate 5) and
`LAYER_HEIGHT_DEFAULT_MM` (measured, plate 6) and nothing else, and it no longer moves when someone
changes a tip radius.

### 3.4 Where it would demand more than the replaced rule — and where it would not

The replaced rule tested **one point** (the representative vertex) for containment and then checked
an area. It could not see a low point whose vertex sits over solid while the rest of the region
extends past the edge. The shipped rule tests the **whole region's** reach and catches exactly that.
No such case exists in these 57; that is a real gap in the replaced rule that this corpus does not
exercise.

**The first draft advertised this as catching "a wide flat ledge springing off a narrow stalk". It
does not, and that claim is withdrawn (R4).** A wide shelf on a narrow root is a *width*
amplification, not a reach one — see §5.3 — and a reach-only criterion is blind to it. The case the
shipped rule really adds is the region-extends-past-the-edge case, which is worth having and is not
the same thing.

---

## 4. What shipped

**Site.** `crates/plinth-support/src/perception.rs`, `find_local_minima`: the
`minima_support_area_tips × π × tip_radius²` threshold and the `carried >= threshold` verdict are
both gone, together with the `carried_outer_area` helper, which had no other reader.

**The verdict.**

```
adequate(minimum)  ⇔  reach(minimum)  ≤  K · h
     K  =  sqrt( X_ALLOWED / 6 )                    // 1.41, evidence registry
     h  =  LAYER_HEIGHT_DEFAULT_MM                  // 0.050 mm, evidence registry
  reach  =  max over p in sampled_region_points(minimum) of
            distance( p, solid footprint at z − h ) // 0 when p is inside a solid
```

**What it reads.** `plinth_core::m0_constants().evidence().x_allowed()` — **this is X_ALLOWED's
first production reader.** It was previously read only by `crates/plinth-core/tests/`. That closes
one line of the registry-bypass finding (task #57). And
`evidence().layer_height_default_mm()`, likewise registry-sourced. Neither number is written into
the code.

**What it stops reading.** `tip_radius_default_mm` and `minima_support_area_tips` at this site.
`minima_support_area_tips` then has no reader at all and is **retired**, not re-tuned (§6).

**The probe slice** at `z − h` is unchanged and still batched with the print-layer slice — the
reach measurement reuses the probe rows that the replaced rule already computed.

**Cost.** One point-to-polygon distance scan per sampled region point:
`O(region points × probe boundary edges)`. `birth_xy_ok` already does the identical edge-distance
scan for the birth test one loop earlier, so this is the same order of work the function already
spends, with the same shape of budget (`limits.max_overlap_edge_tests`) available to bound it.
Measured population: ≤ 392 region points (≤ 65 after sampling) and ≤ 21 probe solids per minimum,
57 minima across 8 models. This is not a hot path.

**What it deliberately does not do, and R5's correction.** The measurement is
`distance to the nearest solid in the probe slice`. That is **not** a load path. The mechanics
require two things the measurement does not check: that the shelf is in-plane continuous with
carried material *inside its own layer*, and that the carried material is itself seated on the
solid directly beneath it. A shelf whose nearest `z − h` solid belongs to a **different,
disconnected component** — a neighbouring limb, a separate island, or *another model on a crowded
plate* — scores a short reach and is declared adequate with no load path at all. Crawler Zombie and
Morel Mudskipper were run together as `in/crowded`, and the corpus run put multiple models through
at once, so this is reachable geometry, not bookkeeping. The replaced `carried_outer_area` had
exactly the same hole (`point_in_polygon` over every solid, `fold(f64::max)`, no component
identity), so this is **disclosed, not worsened**. Carrying component identity through a
slice-at-`z` measurement is the natural fix and is the reason to prefer that stricter form; see
§5.5.

Both gaps are stated in the shipped code, at the function that measures reach.

---

## 5. Honest debt

### 5.1 The root restraint is not modelled, and assuming it away is not conservative (R3)

The first draft listed "root fully clamped" as a defended simplification, on the grounds that the
root is an in-plane section of one continuously cured layer and therefore stronger than a layer
bond. **That checks the wrong section.** A cantilever root moment has to be *reacted* by something.
The only thing available is the bond between the carried part of that same layer and the layer
underneath it — and a moment reacted across a bonded interface loads that interface in
**cleavage**, prying it open at one edge. Cleavage is the weakest way there is to load a bonded
joint, and it is emphatically not what plate 5's arm coupon measured (that was straight
build-direction tension).

So there is a second candidate failure — the carried material peeling off the layer below, just
inboard of the root — which is (a) not modelled at all, (b) plausibly *weaker* than the mode that
is modelled, and (c) carries a geometric parameter the criterion does not have: how far inboard the
bonded region extends. **ASSUMED, direction of error unknown.** The claim that the `X_ALLOWED`
transfer is conservative therefore holds for the in-plane root section and **not** for the
structure as a whole.

### 5.2 Sag is marginal, not bounded (R1)

`MODULUS_GREEN_GPA = 0.5` is a placeholder with no measurement behind it, and §2.7 shows it drops
out of the sag question entirely once that question is set up properly. What replaces it is a
number nobody has: **green elongation at break, which needs to be under about 18%.** That is
plausible for a rigid photopolymer and it is not established. Sag is **marginal**, roughly a factor
of two from governing. New debt, small and cheap to retire: a datasheet lookup or a trivial coupon.

Separately, the mutual inconsistency of `PEEL_PRESSURE_PA`, `MODULUS_GREEN_GPA` and the area law
(§2.7) is filed as task #63.

### 5.3 Width does not cancel when the root is narrower than the shelf (R4) — KNOWN GAP, open

§2.3 cancels the width `W`. That cancellation is valid only when the shelf and its root are the
same width. When a wide patch hangs off a narrow neck:

```
    M ∝ W_shelf ,  Z ∝ W_root      →      σ = (c·p·L²/t²) · (W_shelf / W_root)
```

The amplification is **unbounded** and appears **nowhere** in the shipped criterion, which is
reach-only. A wide flat ledge on a thin stalk — cloak hems, wing membranes, tabletops — has a short
reach and passes.

The same gap swallows the **birth / non-birth boundary** case. A patch joined to carried material
by a hair-thin in-plane neck is classified non-birth, has a short reach, and is waved through as
adequate — while physically it is a birth island hanging on a thread, and that neck is the
narrowest section, which is exactly what the area law exists to police.

**Filed as task #64 (narrow-neck width amplification), not fixed here.** Stated in the shipped
code. Closing it needs a root-width measurement the perception stage does not currently make.

### 5.4 The reach probe has no connectivity requirement (R5) — KNOWN GAP, open

Stated in full in §4. Distance to *any* solid in the probe slice is not a load path; on a crowded
plate the nearest solid below can belong to a different model. Same hole as the predicate it
replaces, so nothing regressed — but a criterion sold as physics-derived must say where it departs
from its own physics. **Filed as task #65 (connectivity requirement), not fixed here.** Stated in
the shipped code.

### 5.5 What was not measured

- Reach is measured on sampled region points rather than on the slice at `z`. The flood-filled
  region spans about two layer heights (`region_dz` runs 0.093 – 0.0995 mm in the dumps), so points
  a layer higher are being compared against a slice two layers below them; the direction of error
  depends on whether the feature flares or tapers. The slice-at-`z` form fixes both this and, with
  component identity carried through, §5.4.
- Four of the 57 minima have degenerate regions (two with one point, two with two points). For
  those the whole-region reach test collapses back to a single-point test, so the §3.4 improvement
  does not reach them.
- The cause of the 55 and 5 missing support records is identified as *not* the minima filter, but
  it is not positively diagnosed.

### 5.6 The overhang-face fraction is a guess, and it is not allowed to spend a plate (C5)

The review's correction stands and is applied: **the 64% candidate-face reduction is INFERRED under
an ASSUMED isotropic surface, it is the number that would justify a coupon plate on
`OVERHANG_ANGLE_DEG`, and presenting it as future work while recommending the plate is lazy debt.**
Per R2 the shipped end predicts 37%, not 64% — a materially different case for spending a plate.
Recommendation 4 is therefore **conditional**: do not authorize the coupon plate until the
overhang-candidate face fraction is measured. That is Craig's own settled rule — size the prize
before building the instrument (the FU-82 ruling). Filed as task #66 (angle coupon).

**The demanded measurement is now DONE, 2026-08-19.** The overhang-candidate face fraction was
measured on the production-orientation path (`prepare_one` → `orient_model` →
`DenseSweep` → `face_normals_z`) across 10 models: moving the cutoff to the conservative band
edge (θ* = 35.3°) unflags 32% of flagged area (required-8 aggregate; 38% across all 10), and the
loose band edge (26.6°) unflags 58% (required-8; 64% across all 10), with the smallest per-model
effect still 24.6% of area. Evidence: `scratchpad/overhang-out/results.md`, cited in full in the
provenance appendix below. This satisfies the condition; it does not itself reopen recommendation
4, which is for the task that spends the plate.

**Stated amendment to the correction.** The review called the measurement "a single mesh pass over
data already in hand", half an hour of work. That is not right, and saying so does not soften the
conclusion. The overhang test runs on the **prepared and oriented** face set, after repair,
decimation and orientation; raw mesh normals from `minis-to-process/` are in the wrong frame, and
`nz` is exactly the quantity orientation changes. Measuring the fraction honestly needs the same
instrumented-build-and-run rig the minima dumps needed, on 8 models. It is a small measurement task
and it is not free. **The binding part of the correction is unchanged: no plate until it is
measured.** It is not folded into this task.

---

## 6. What was recommended, and what happened

1. **Ship the reach criterion now, without a coupon.** It needs nothing the registry does not
   already hold at evidence grade — `X_ALLOWED` and `LAYER_HEIGHT_DEFAULT_MM`, both measured. It
   reproduces every verdict on the corpus, it retires a placeholder, and it makes the tip radius
   structurally unable to move a low-point verdict again. **DONE**, at Craig's conservative end,
   `K = sqrt(X_ALLOWED/6)`.
2. **Retire `MINIMA_SUPPORT_AREA_TIPS`;** do not re-tune it. It has been inert by a factor of 14 on
   every model measured, and §1 shows it is inert by construction. **DONE** — registry declaration
   removed, ledger entry moved to `retired`, this document cited as the basis.
3. **Correct the record on task #61.** The 0.050 mm tip radius / 0.100 mm diameter dropped no
   low-point support on Deinonychus or Crawler Zombie; the record-count change is a downstream
   junction effect and wants its own look. The tip-minimum half of §1 needs no action: Craig
   superseded the validator minimum to 0.100 mm diameter / 0.050 mm radius on 2026-08-19, on the
   parked `feature/tip-min` branch. The record-count question is still open, and is for task #61.
4. **Take `OVERHANG_ANGLE_DEG` next,** with an angle-fan coupon: a vertical wall with wide
   unsupported undersides springing from it at θ ∈ {10, 15, 20, 25, 30, 35, 40, 45}° above
   horizontal, ~5 mm of run each, contiguous row bases, no factor assigned by position, the
   mandatory control set, three copies randomized over Y. Read the shallowest angle that prints
   clean and in register; that is θ*, and `K = 1/tan θ*`. **The prediction is a band, θ* ∈
   [26.6°, 35.3°]** — a reading of 35° confirms the concentrated load model, it does not falsify
   the derivation. The angle form is deliberate: a reach ladder would need features at
   0.025 – 0.30 mm, which is 2 – 15 printer pixels and unreadable, while the angle form is
   scale-free and reads at millimetre scale.
   **BLOCKED** on §5.6: measure the overhang-candidate face fraction first.

---

## Appendix: measurement provenance

- Worktree: `scratchpad/plinth-wt-minima`, detached at `b8edcb8`, one instrumentation edit to
  `perception.rs` (an `eprintln` behind `PLINTH_MINIMA_DUMP` plus one distance helper). Removed on
  completion. The main checkout was never modified and `git status` was clean throughout.
- Binaries: one built with the tip default at 0.200 mm radius / 0.400 mm diameter, one at 0.050 mm
  radius / 0.100 mm diameter. Both `gate` profile, both postdating the instrumentation edit.
- Runs: per-model stderr dumps and stdout run reports, retained in the task scratchpad. Deinonychus
  and Crawler Zombie were run twice, once per binary; dumps byte-identical, record counts
  1059/1004 and 380/375.
- Models producing minima: Deinonychus, Crawler Zombie, Bat Cat Owl, Morel Mudskipper, Plague
  Zombie, Skulker Zombie, Tavey Badger, Thorny Vegepygmy. Failing at later stages: Cat (wild shape
  — Cat Lord), Decomposing Zombie, Giant Wolf Spider, Skeleton_1.
- Sources read-only from `minis-to-process/`; every output under the task scratchpad.
- Review: independent adversarial review the same day confirmed every arithmetic step, both
  code-reader claims by grep, the byte-identical dumps, the record counts, and the 55/2 normal
  split; it re-derived the headline formula from scratch and reached the same result. Its verdict
  was SOUND-WITH-CHANGES and its eight required changes are applied in this copy (§0).

**Corpus-parity evidence for the shipped reach criterion (task #62).** §3's fixture pins reach
margins and comparison direction on three literal values (see the test-level disclosure at
`the_corpus_reach_table_reproduces_at_the_registered_allowance`); it does not re-derive them from
a mesh. The actual parity check is emission-level, in `scratchpad/reach-out/`:

- Four models with a non-trivial minima history (Crawler Zombie, Deinonychus, Morel Mudskipper,
  zombie-3-a) were run through `plinth prepare` twice: once on a binary built at `develop`
  (`b8edcb8`, pre-retirement, the control) and once on a binary built at this branch's tip
  (post-retirement, `K = sqrt(X_ALLOWED/6)` shipped). All four `.chitubox` outputs are
  byte-identical between the two builds (`cmp` exit 0 on all four), same support-record counts on
  each side. Logs: `logs/build-baseline.log`, `logs/build-shipped.log`, `logs/n-base.txt`,
  `logs/n-ship.txt`, `runs/baseline/`, `runs/shipped/`.
- Two further single-model shipped-only runs (`runs-cz-shipped/`, `runs-tv-shipped/`) reproduce
  the exact support-record counts (380 Crawler Zombie, 314 Thorny Vegepygmy) that the perception.rs
  reach fixture's comment block cites, tying the fixture's literals back to a real run.
- Scoped gate reproduced clean on the shipped build: `logs/gate-workspace.log`,
  `scoped-gate.log`, `scoped-gate-verify.log`, `scoped-gate-pipeline.log`.

Together these are the evidence that the reach criterion changes **zero** corpus verdicts and
**zero** corpus bytes at the registered allowance — the fixture in §3 is a margin/direction pin,
not this proof.

**Overhang-candidate face-fraction measurement (§5.6, task #66 precondition).** Measured
2026-08-19 in a separate detached worktree (`plinth-wt-overhang`, removed on completion) on the
production `prepare_one` → `orient_model` → `DenseSweep` → `face_normals_z` path, across 10
models (8 required + 2 optional, all measured, none failed). Full method, per-model table,
confidence checks and exact commands: `scratchpad/overhang-out/results.md` (raw data in the
adjacent `raw.json` / `raw-repeat.json` / `raw-required.json` / `results.json`). Headline:
required-8 aggregate unflags 32.02% of flagged area at θ* = 35.3° and 57.81% at θ* = 26.6%;
all-10 aggregate 38.27% and 64.36% respectively (dominated by the treant, 47% of all flagged
area). This satisfies the FU-82 sizing precondition in §5.6; it does not itself authorize the
coupon plate.
