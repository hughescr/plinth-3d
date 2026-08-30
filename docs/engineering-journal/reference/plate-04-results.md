# Calibration plate 4 — extended overhang: results salvage

**This is a salvage record, recovered 2026-08-30 from chat transcripts.** Plate 4
was printed, read by the owner, and then scored in depth by an adversarial
review — and none of it was ever written to a file. The geometry survives in
[`plate-generator-PLATE-LAYOUT.md`](../../evidence/plate-generator-PLATE-LAYOUT.md);
the owner's read and the review's findings were recovered from the session of
**2026-07-29**.

**Read §4 before using any number from this plate.** The experiment succeeded and
the inference built on it did not. That distinction is the most valuable thing
plate 4 produced, and it is the reason two standing project rules exist.

Anything not recovered is marked **`read not recovered`** and is **not guessed**.

---

## 1. Provenance

| Field | Value |
|---|---|
| Plate | **COUPON-PLATE-4-overhang-extended** |
| Printed / scored | **on or before 2026-07-29** |
| Printer | **Elegoo Saturn 4 Ultra 16K**, tilt release, vat hinged at the back (+Y) |
| Resin | **Sunlu Water-Wash ABS-Like Black** |
| Layer height | **0.050 mm** |
| Print order | **the 2nd print on that PFA film** — plate 1 was the 1st. Plates 2 and 3 were never printed (see [`plate-02-and-03-status.md`](plate-02-and-03-status.md)) |
| Supports | **none.** Every specimen on its own 2.00 mm pad |
| Specimens | **25 towers × 4 shelves = 100 measuring shelves**, plus family-E4 witness slabs `E4X1`…`E4X6` |
| Ladder | 12 rungs, geometric ratio **1.2169**, spanning **0.7982 – 6.9169 mm** |
| Scoring | **two independent passes** — the owner's visual read, and a multi-agent scoring workflow with an adversarial audit |

### The towers

| group | towers | position |
|---|---|---|
| **G1 – G5** | `a`, `b`, `c` each (15 towers) | distributed across the plate |
| **X01 – X10** | 10 towers, uniform 15.00 mm pitch over 135 mm of X | **rear row only** |

Tower suffixes carry rung order: `U` = ascending, `D` = descending. The owner's
photographs are labelled "A, B, C left-to-right" within each G group,
corresponding to the `a`/`b`/`c` towers.

The extended ladder is generator family **A2** — plate 1's family A re-centred
onto the pressure envelope that plate 1 bounded. The scoring workflow's report is
titled "PLATE 4 / FAMILY A2" for that reason.

---

## 2. The owner's read, verbatim

> "Ok, plate 4 printed. Everything more-or-less came out, but **the longest
> shelves were all peeled** - each layer not quite bonded to the layer below due
> to bending/peeling. **The 2nd-longest shelves all look quite good.** All of the
> EX and EY check targets came out perfect.
>
> All of these tower series are A, B, C left-to-right.
>
> [G1, G2, G3, G4, G5 tower photographs; X01–X05 and X06–X10 photographs]
>
> **G1, G2 and G3 show some peeling in the 2nd longest shelves for G1A, G1C,
> G2B, G2C, G3C; G4 second shelves look fine; G5C shows some peeling in the
> second shelf. The Xs show second-shelf peeling on ALL of the towers.**"

Two things follow directly and are not in dispute:

- **The family-E4 validity gate passed** — "all of the EX and EY check targets
  came out perfect", the same gate plate 1 passed.
- **The ladder was correctly centred.** The longest shelves all failed and the
  shorter ones did not, so the transition sits inside the rungs — which is
  exactly what plate 1's one-sided bound was meant to fix, and it worked on the
  first attempt.

He later added a design request for future plates of this shape:

> "print lines on the top of the rungs spaced evenly for measuring - if the tip
> fails, we can see how much did actually remain connected. So print lines every
> 0.5mm or something maybe 2 layers thick"

That became the graduation ticks — 58 per 12-rung ladder, 480 on the plate.

---

## 3. What the plate proves — five claims that survived adversarial audit

**3.1 A critical unsupported length exists and is inside the ladder.**
n = 100 shelves / 25 towers / 1 build. **Zero failures at the bottom of the
ladder; 15 of 15 failed at L ≥ 4.671 mm**, under every scoring rule proposed,
including a hostile one counting all 7 `DROOPED` calls as failures. Described by
the audit as "the plate's core success and it is unassailable."

**3.2 The X row is flat in X — the cleanest result on the plate.**
10 towers × 4 heights over 135 mm of X, identical rung set, identical
orientation, uniform pitch: `INTACT ×10 / [split] / DELAM ×10 / DELAM ×10`. No
outlier, no gradient, **no local panel defect**.

This bears directly on plate 1's open `D5` question. The one apparent variation
at z = 8 (`INTACT ×5 / DROOPED ×5`) falls *exactly* on the boundary between two
photographs scored by two different readers, and the second reader recorded that
`DROOPED` was "the conservative verdict" because they could not resolve a
separation line. **A physical defect that respects a photographic frame boundary
is not credible.** So a dead-LCD-region hypothesis finds nothing here — though
note this row is at one Y, so it tests X-uniformity, not front-to-back position.

**3.3 Length alone does not determine survival — but the cause is not identified.**
At matched length (ascending mean L = 3.200 mm against descending 3.189 mm,
balanced by construction), **ascending towers fail more than descending ones**.
n = 15 towers, p ≈ 0.005–0.10 depending on scoring rule. All six length-stratified
sub-comparisons point the same way. The clean instance:

> **`G1aU` z = 10, L = 2.5920 mm — FAILED**, while
> **`G1bD` z = 8, L = 3.1543 mm — SURVIVED.** Same row, same layer, 15 mm apart.

The shorter finger failed and the longer one did not.

**But "ascending" is four things at once**, and the plate cannot separate them:
the rung→z map; print order within the tower; damage history (on a descending
tower, the j = 2 shelf prints *after* its own longest shelf has already failed 40
layers below and is hanging free); **and a 9.75 mm X-column offset**, because
ascending and descending towers sit in different X columns in every group.
Ascending j = 2 towers are also 75 % odd-row against 43 % for descending.

**There is no tower on this plate where orientation varies with column and row
parity held fixed.** What is proven: at matched length, *some* factor carried by
the ascending/descending assignment shifts the critical length by **1.31–1.53× in
L**. Which factor, the plate cannot say.

**3.4 Every specimen's last-printed lamina retained full nominal length — but
this is weaker than "delamination".**
100/100 shelves, tick counts matching geometric prediction, seven independent
readers. That proves no gross material loss on the *top* face. It does **not**
establish the failure mode: the ticks sit on the last-printed face, and every
observed crack plane is between the **first-printed bottom** laminae — so **the
ruler is on the one lamina that did not fail.** A bottom lamina that ruptured at
its root and curled back would leave the top-face measurement unchanged; at
22–34 px/mm a missing 0.05 mm lamina is 1–2 pixels. There is **no positive
control** — no specimen known to be short — so an instrument that returned the
same answer on all 100 specimens has never been shown able to return a different
one.

The owner's word was "peeled", which is agnostic between modes. He confirmed
**failure, not mode**. Correct statement: the feature stayed attached at the
root, the top lamina is full length, and failure is confined to the lower
laminae. Which of them separated versus tore is **unmeasured**.

**3.5 In-print cycling after completion is excluded as the driver of the height
effect — on a sign argument.**
A z = 8 finger completes ~40 lift/recoat cycles earlier than a z = 10 finger, so
it rides ~40 *more* cycles afterwards. It fails **less**. Any accumulation model
predicts the opposite sign.

This does **not** exclude wash, cure, drying-shrinkage or handling accumulation —
all strongly length-dependent through compliance, and identical for z = 8 and
z = 10 by construction. A purely post-process model still reproduces the entire
monotone-in-length pattern.

---

## 4. What the plate does NOT prove — and the two rules that came from it

The audit killed or wounded every headline number the first analysis produced.
This section is the plate's most durable output.

| claim | status | why |
|---|---|---|
| "The two readings agree on 93 of 100 shelves" | **KILLED** | **50 of 100 "owner" cells were imputed `OK` by code.** The owner commented only on the longest and second-longest shelves; the rest were filled in by rule and then treated as his reading. Real overlap is 45/50 on cells he actually examined. Two of the seven "disagreements" are shelves he never looked at |
| "L₅₀ = 2.72 mm, CI [2.34, 2.97]" | **WOUNDED** | Under a policy-consistent imputation rule the fits move to 2.49 mm (z = 10) and 4.65 mm (z = 8, CI blowing out to [3.07, 288]). Treating the X row as one cluster rather than 10 independent trials widens z = 10 to [2.39, 3.43] |
| "Confidence grading validated at p = 1.2 × 10⁻⁵" | **WOUNDED** | 0.0054 with imputations excluded, post-hoc, and not identified — "medium calls are photo false positives" and "medium calls are genuine but mild" predict the same table |
| "z explains 100 % of the non-monotonicity" | **KILLED** | At least one inversion z cannot explain |

### Rule 1 — no factor may be assigned by a position formula

> "The *design method* has one systemic defect that produced every alias in the
> plate: **every factor is assigned by a deterministic formula rather than
> counterbalanced.**"

This is the origin of the standing coupon-plate design rule. Because orientation,
X column, row parity and print order were all derived from the same index, every
one of them is aliased with every other, and §3.3 is unidentifiable as a result.
The ladder was fine; the *assignment* was not. **Randomize or counterbalance;
never assign by formula.**

### Rule 2 — a reading is what the owner said, not what code filled in

Half the "owner reading" was manufactured by an imputation rule and then used as
ground truth, including in an agreement statistic that measured the imputation
against itself. **Score only the cells that were actually examined, and mark the
rest unread.**

The audit's own summary of the position: *the experiment is sound and worked; the
inference model is not sound and cannot be repaired by more plates of this type.*
Keep the ladder, replace the criterion, randomize the assignment, formalize the
scoring.

---

## 5. Findings carried forward

| # | Finding | Strength |
|---|---|---|
| 1 | A critical unsupported length exists and is bracketed: 0 failures low, 15/15 at L ≥ 4.671 mm | **Strong** — survives every scoring rule |
| 2 | The plate is flat in X across 135 mm — no local panel defect at that Y | **Strong** — 10 towers, no outlier |
| 3 | At matched length, the ascending/descending assignment shifts critical length by 1.31–1.53× | **Effect real, cause unidentified** — four aliased factors |
| 4 | Failure is confined to the lower laminae; the feature stays attached at the root | **Mode unmeasured**; no positive control |
| 5 | Post-completion lift/recoat cycling does not drive the height effect | Sign argument; does not exclude post-processing |
| 6 | Every quantitative L₅₀ figure from this plate | **Do not use.** See §4 |
| 7 | Family E4 validity gate passed | Owner's read |

## 6. What this plate cannot tell you

- **No usable L₅₀.** The fits are imputation-dependent and move by more than
  their own confidence intervals under a defensible change of rule.
- **No failure mode.** Peel versus tear is unmeasured, and the instrument that
  would distinguish them sits on the wrong face.
- **No attribution for the orientation effect**, per §3.3.
- **Nothing about front-to-back position.** The X row tests X-uniformity at one
  Y. Plate 1's `D5` question is untouched by this plate.
- **Nothing about any resin but Sunlu Water-Wash ABS-Like Black**, on a nearly
  new film, at an unrecorded ambient.

## 7. Sources

- Owner's read and photographs: session
  `0010a938-3a51-4e43-8d1c-2db9590b4417`, 2026-07-29.
- Adversarial scoring: workflow "Score plate-4 coupon failures from macro photos,
  decode confounds, re-derive the governing failure criterion", same session. Its
  synthesis is quoted throughout §3 and §4. Its own output file lived under
  `/private/tmp` and **did not survive**; the transcript is the surviving copy.
- Geometry and rung ladders:
  [`plate-generator-PLATE-LAYOUT.md`](../../evidence/plate-generator-PLATE-LAYOUT.md),
  [`plate-generator-manifest.json`](../../evidence/plate-generator-manifest.json).
- The generator (`calibration-coupons/`, Python) was deleted 2026-07-31 and the
  language is barred by a build guard. **The plate cannot be rebuilt from this
  repository.**
