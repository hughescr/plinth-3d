# The unsupported-span criterion — the gate that replaces I7's cantilever hole

Status: written 2026-08-30 for the standalone audit crate at `tools/plinth-audit/`.
Audience: whoever implements the replacement for `src/checks/island_coverage.rs`, and
the reviewer who attacks it afterwards.

**What this document is for.** `docs/specs/audit-invariants.md` §3.7 gates only
*detached* islands. A run-component attached to the layer below along even one pixel is
skipped entirely, so a 20 mm ledge, a wing that grows sideways, and a cavity ceiling
that closes over a hollow are all invisible to the current I7. §6.1 of that document
names this "the largest hole", and it is the gate closest to the owner's own complaint —
that the emitted files do not support the parts that need supporting. This document
supplies the missing quantity: **an unsupported-span criterion, derived from the two
calibration plates that measured it.**

It changes no emitted byte and no engine behaviour. It specifies a check.

**Three rules inherited from `docs/specs/audit-contract.md` and applied without
exception below.**

1. Anything the audit cannot establish is a hard failure. "Not measured" and
   "indeterminate" never map to pass.
2. Measurement uncertainty is subtracted from an allowance, never added to it. A value
   inside the uncertainty band **fails**. This document was written after a
   cross-family review found the opposite convention (`delivered >= required - tol`) in
   the existing checks; §7 states the direction for every tolerance here and shows its
   sign.
3. Limits come from measurement. Constants prune a search; they never decide an
   outcome (`docs/physics.md` §12, physics-first doctrine).

---

## 1. The question, stated precisely

> For a piece of down-facing model geometry with nothing under it, how far may it reach
> from the nearest thing that carries it before it stops printing acceptably?

The answer has to be a length in millimetres at the printer's own layer height, and it
has to come from the plates, because that is the only place it was ever measured.

---

## 2. The measurement

### 2.1 The two plates, and why they are the right evidence

Both plates are flat-overhang ladders in a **single 0.050 mm layer** — pure bending, no
support anywhere, each finger cantilevered from a rigid spine. That is exactly the
quantity this criterion needs, and nothing else in the plate programme measures it.

| | plate 1 | plate 4 |
|---|---|---|
| record | `docs/engineering-journal/reference/plate-01-results.md` | `docs/engineering-journal/reference/plate-04-results.md` |
| plate | `COUPON-PLATE-1-overhang-tilt` | `COUPON-PLATE-4-overhang-extended` |
| printer | Elegoo Saturn 4 Ultra 16K, tilt release, hinge at +Y | same |
| resin | Sunlu Water-Wash ABS-Like Black | same |
| layer height | 0.050 mm | 0.050 mm |
| film | 2nd print on that PFA film | 2nd print on that PFA film (plate 1 was the 1st) |
| supports | **none** | **none** |
| specimens | 17 family-A masts × 15 rungs, 5 Y stations | 25 towers × 4 shelves = 100 shelves |
| outcome | **nothing failed at any rung at any station** | 0 failures low, **15/15 failed at L ≥ 4.671 mm** |

The ladders are the same geometric series. From
`docs/evidence/plate-generator-PLATE-LAYOUT.md`: plate 4's rungs 1–7 (0.7982 … 2.5920
mm) **are exactly plate 1's rungs 9–15**, same ratio, same 0.050 mm critical layer, same
finger, same spine, same finger Z levels. The two plates are tied together by seven
shared nominal lengths, so they can be pooled on the length axis without a conversion.

The plate-4 ladder, recomputed here from its generator parameters (`L₁ = 0.7982 mm`,
ratio `1.2169`, 12 rungs) and checked against every length the two result files quote:

| rung | L, mm | quoted in the plate records as |
|---:|---:|---|
| 1 | 0.798200 | 0.7982 (ladder bottom) |
| 2 | 0.971330 | |
| 3 | 1.182011 | |
| 4 | 1.438389 | |
| 5 | 1.750376 | |
| 6 | **2.130032** | |
| 7 | **2.592036** | 2.5920 — plate 1's top rung, and `G1aU`'s failure |
| 8 | 3.154249 | 3.1543 — `G1bD`'s survival |
| 9 | 3.838405 | |
| 10 | **4.670956** | 4.671 — "15 of 15 failed at L ≥ 4.671 mm" |
| 11 | 5.684086 | |
| 12 | 6.916964 | 6.9169 (ladder top) |

Adjacent rungs differ by the ratio 1.2169. **No inference from these plates can localise
a boundary finer than one rung**, i.e. finer than 21.69 % in length. That is the
resolution of the instrument and it bounds everything below.

### 2.2 The per-rung tally, reconstructed — and a self-check that validates it

The plate-4 record carries the owner's verbatim read but no per-shelf table. The table
below is reconstructed here by combining two independent sources: the owner's words
(plate-04-results §2) and the per-tower rung assignment in
`docs/evidence/plate-generator-manifest.json`.

Rung sets from the manifest (`graduations.levels[].rung` per tower):

- column **a** towers (`G1aU`, `G2aD`, `G3aU`, `G4aD`, `G5aU`) carry rungs **{1, 4, 7, 10}**
- column **b** towers (`G1bD`, `G2bU`, `G3bD`, `G4bU`, `G5bD`) carry rungs **{2, 5, 8, 11}**
- column **c** towers (`G1cU`, `G2cD`, `G3cU`, `G4cD`, `G5cU`) carry rungs **{3, 6, 9, 12}**
- the ten rear-row towers `X01`…`X10` carry rungs **{2, 5, 8, 11}**

`U`/`D` reverses the order in z; it does not change the set. So each tower's *longest*
shelf is rung 10 (a), 11 (b, X) or 12 (c), and its *second-longest* is rung 7, 8 or 9.

The owner's read, applied to that map:

> "the longest shelves were all peeled … G1, G2 and G3 show some peeling in the 2nd
> longest shelves for G1A, G1C, G2B, G2C, G3C; G4 second shelves look fine; G5C shows
> some peeling in the second shelf. The Xs show second-shelf peeling on ALL of the
> towers."

| rung | L, mm | shelves at this rung | failures | who failed |
|---:|---:|---:|---:|---|
| 1–6 | 0.798 – 2.130 | 50 | **not read** | the two shortest shelves of every tower; the owner did not comment on them |
| 7 | 2.592036 | 5 (a) | **1** | `G1a` |
| 8 | 3.154249 | 15 (b + X) | **11** | `G2b`, `X01`…`X10` |
| 9 | 3.838405 | 5 (c) | **4** | `G1c`, `G2c`, `G3c`, `G5c` |
| 10 | 4.670956 | 5 (a) | **5** | all |
| 11 | 5.684086 | 15 (b + X) | **15** | all |
| 12 | 6.916964 | 5 (c) | **5** | all |

**The self-check.** This reconstruction says the owner examined exactly the longest and
second-longest shelf of each of 25 towers = **50 of 100 cells**, leaving 50 unread.
Plate 4's own adversarial audit, from a different route entirely, reports "**50 of 100
'owner' cells were imputed `OK` by code**". The two counts agree exactly. That agreement
is what licenses the column→rung-set map used above; without it this table would be a
guess.

The G-tower subtotal at `L ≥ 4.671 mm` is 5 + 5 + 5 = **15 of 15**, which is the figure
plate-04-results §3.1 calls "unassailable". Including the ten X towers it is 25 of 25.

### 2.3 Admissibility: which cells may be used

Plate 4 Rule 2 — *a reading is what the owner said, not what code filled in* — is
binding. **The 50 unread cells at rungs 1–6 supply no evidence in either direction.**
They are not zero-failure observations; they are absences.

The consequence is sharp and must not be softened: **plate 4 alone contains no rung with
zero recorded failures.** Its lowest examined rung is 7, and rung 7 failed once. Plate 4
brackets the boundary from above only. Every lower bound in this document comes from
plate 1.

### 2.4 The inversion, and what it costs

Plate 1 lost nothing at 2.592036 mm across 17 masts at 5 Y stations. Plate 4 lost
`G1aU` at exactly 2.592036 mm — while `G1bD` survived at 3.154249 mm, longer, same row,
same layer, 15 mm away.

Plate 4 §3.3 attributes the inversion to the ascending/descending assignment, which
carries **four aliased factors it cannot separate** (rung→z map, print order within the
tower, damage history, and a 9.75 mm X-column offset, with row parity on top). What is
proven is only the magnitude: at matched length, *some* factor carried by that
assignment shifts critical length by **1.31–1.53×**. Which factor, the plate cannot say.
Every L₅₀ figure from plate 4 is marked **do not use**.

Two things follow, and they are the whole substance of the derivation.

1. **Plate 1's clean result at 2.592036 mm does not transfer.** A failure was recorded
   at that exact length under a different setting of an unidentified factor. Any gate
   permitting 2.592036 mm permits a length whose own experiment recorded a failure.
2. **Plate 1's remaining rungs are favourable-condition observations.** At the one
   length where the two plates overlap and disagree, plate 1 is the side that survived.
   Its survivals below that length therefore carry the same unresolved factor, and the
   only measured magnitude for that factor on this printer is 1.31–1.53×.

### 2.5 The bracket

Pooling plates 1 and 4 on the shared length axis, at 0.050 mm layer height, this
printer, this resin:

```
no loss recorded at or below            2.130032 mm    (plate 1 rung 14 = plate 4 rung 6;
                                                        plate 4's own cells here are UNREAD)
first loss recorded at                  2.592036 mm    (plate 4, G1aU, 1 of 5)
loss universal at and above             4.670956 mm    (plate 4, 15 of 15 G-tower shelves;
                                                        25 of 25 including the X row)
```

**`L_crit ∈ [2.130032, 4.670956] mm`, with the onset bracketed to a single ladder rung,
`[2.130032, 2.592036]`, width 1.2169×.**

A second, weaker bracket exists on *quality* rather than loss. Plate 1 reported visible
corner sag "only on the top two" rungs of every group that was read — that is rungs 14
and 15, **2.130032 mm and 2.592036 mm**. Sag magnitude was never measured and there is
no positive control, so this bracket is qualitative. It says only: **the largest span
with neither loss nor reported sag anywhere is 1.750376 mm** (plate 1 rung 13 = plate 4
rung 5).

### 2.6 The conservative end, and the constant

Take the conservative end of the loss bracket, `2.130032 mm`. Its only admissible
evidence is plate 1, which §2.4 established is the favourable condition. Move it into
the unfavourable condition using the only measured condition-shift magnitude available,
taking the **larger** end of the 1.31–1.53× range because a larger divisor is the
conservative choice:

```
L_SURVIVED_MAX_MM     = 2.1300322500 mm       plate 1 rung 14 = 0.7982 * 1.2169^5,
                                              Measured; largest span with zero recorded loss
CONDITION_SHIFT_MAX   = 1.53                  plate 4 §3.3, upper end of the measured
                                              1.31-1.53x shift; Measured, cause unidentified

L_SPAN_CRIT_MM        = L_SURVIVED_MAX_MM / CONDITION_SHIFT_MAX
                      = 1.3921779412 mm       Derived
```

Then subtract the audit's own instrument uncertainty, per rule 2 of the preamble. The
raster fill rule is full containment (`audit-invariants.md` §1.6): a pixel is cured only
if the solid contains its whole footprint, so the cured region is a subset of the true
region and a **measured span understates the true span** by at most one pixel footprint
at each end of the path. One pixel footprint is bounded by the in-layer pixel diagonal
`d2 = sqrt(0.014² + 0.019²) = 0.0236008474 mm`:

```
d2                    = 0.0236008474 mm       derived from the printer pitches
L_SPAN_ALLOWED_MM     = L_SPAN_CRIT_MM - 2 * d2
                      = 1.3449762463 mm       Derived — THE GATE CONSTANT
```

**`L_SPAN_ALLOWED_MM = 1.3449762463 mm`.**

The implementation **must compute this from its four inputs at run time and must not
hard-code the result**, and must echo all four in `declared_inputs` with the provenance
column above. A constant whose derivation is not in the code has been converted into a
chosen number, which is precisely what physics-first doctrine forbids.

**Cross-check against the quality bracket.** `1.34498 mm < 1.750376 mm`, the largest
span with neither loss nor reported sag. The derived allowance therefore also sits below
the sag evidence, and this document does not have to arbitrate between the loss tier and
the quality tier — the conservative end of the loss tier already satisfies both. That
the two tiers agree is a check, not a coincidence to lean on: the sag reading is
qualitative and would not support a gate on its own.

### 2.7 What this number is not

- It is **not** an L₅₀ and must never be called one. Every L₅₀ from plate 4 is marked do
  not use; this is a bracket end with a measured discount, and the bracket is one rung
  wide.
- It is **not** valid at any other layer height. Both plates ran at 0.050 mm and neither
  varied it. Bending of a single lamina scales with its thickness; nothing here says how.
  A file whose declared `layer_height_mm` is not 0.050 must fail
  `FILE_DECLARED_INPUT_MISSING` reason `span_criterion_layer_height_uncalibrated`
  rather than silently reuse this number.
- It is **not** valid for another resin, printer, or film condition
  (`docs/physics.md` §11).

---

## 3. What "unsupported span" means on a rasterised layer stack

Everything below runs on the single layer raster the audit already builds
(`audit-invariants.md` §1.6): run-length rows per pixel row, `0.014 × 0.019` mm in
plane, `0.050` mm in Z, full-containment fill.

### 3.1 The down-face set — unchanged, and it is already proven

For layer `k` above the first cured layer:

```
model(k)       = cured model pixels at layer k                 (run rows)
attached(k)    = { p in model(k) : p's own column is cured at layer k-1 in model }
unattached(k)  = model(k) \ attached(k)
```

This is the identical per-column attachment predicate the current `island_coverage.rs`
uses (`is_attached`) and that `report.overhang` uses. It is not new and it is not
optional: `audit-invariants.md` §2.1 requires that the area of `unattached(k)` summed
over layers reconstructs the plate-projected down-facing area `D_total` to within the
stated quantisation bound, and that reconstruction is already a file-level gate. So the
set this criterion measures on is the same set the demand model is built from. **A
change to `is_attached` breaks I2, I3 and I4 as well.**

### 3.2 Carriers — what counts as "the thing underneath"

```
support(k)        = cured support pixels at layer k
support_borne(k)  = { p in support(k) : the 8-connected component of support(k)
                      containing p has at least one pixel whose own column is cured
                      in the support raster at layer k-1 }
below(k)          = model(k-1) union support(k-1)          (as column sets)

carrier(k)        = attached(k)
                    union { p in model(k) : p's column is in support_borne(k) }
                    union { p in model(k) : p's column is in support(k-1) }
```

Three clauses, each with a reason:

- **`attached(k)`** — model material directly under it. Uncontestable.
- **`support_borne(k)`** — a support occupying the same column at the same layer. This
  is the embedded-tip case the seat-depth policy produces, and it is what the current
  I7 tests. The `_borne` qualifier is new and closes a real hole: today's I7 accepts any
  support pixel, including one belonging to a blob that is itself born in mid-air at
  that layer. Requiring the support *component at this layer* to be attached to the
  support raster below costs one component pass and refuses that case. It is
  deliberately **component-level, not pixel-level**, because a tip ball is wider than
  its own taper and its rim pixels are legitimately unattached at their birth layer.
- **`support(k-1)`** — a support column that stops one layer short still carries the
  model material above it; the gap is zero and the two cure in one exposure
  (`audit-invariants.md` §1.3, owner ruling 2026-08-30).

**What `carrier` does not establish, and must be labelled as such in the output:**
that the support reaches the plate (I5), that it is big enough (I2/I3/I4), or that it is
live rather than dead geometry (I8). The eight invariants are read together. This
criterion answers "is there something carrying this, and how far away is it", not "is
that something adequate".

### 3.3 The span measure: geodesic reach to the nearest carrier

```
for each layer k, for each pixel p in unattached(k):

    reach(p) = the minimum, over all paths from p to any pixel of carrier(k) that stay
               inside model(k), of the path's in-plane length

    reach(p) = +infinity when no such path exists
```

`reach` is measured **in the layer plane**, in millimetres, on the anisotropic pixel
lattice.

A **down-face region** is an 8-connected component of `unattached(k)`. Each region's
score is `max reach(p)` over its pixels, and the location reported is the arg-max pixel.

### 3.4 Why reach, and not the other candidates

The coupon is the argument. A plate-1/plate-4 finger is anchored along one edge of a
rigid spine and projects `L`; every point of it lies at in-plane distance at most `L`
from its anchor, and the tip lies at exactly `L`. **`reach` is the coupon's own
independent variable**, measured on the printed part the same way it was measured on the
specimen. No other candidate is.

| candidate | why it is wrong |
|---|---|
| longest chord of the region | measures the region's size, not how far load must travel. A 20 mm ledge carried along its whole length by a row of supports would fail. |
| inscribed circle / erosion radius | measures thickness. A long thin unsupported ledge has a small inscribed circle and would **pass** — failure in the unsafe direction. |
| longest run of unattached pixels within one pixel row — **what `report.overhang` measures today** | anisotropic (it measures in X only, so a ledge running in Y reads as many short runs) and blind to where the carrier is. On `Bat Cat Owl` it returns worst run **1.582 mm** and worst run in a layer with no support pixel **0.266 mm** — both comfortably under any threshold derived here, on a model the owner would not print. Its "no support" clause asks only whether the *whole layer* contains any support pixel anywhere, which is not a locality test at all. **This number must not be reused as the span.** |
| 3D distance to the nearest support tip centre | wrong layer and wrong path. A tip 2 mm below in Z carries nothing at this layer. |
| straight-line (Euclidean) distance to the nearest carrier | cuts across voids. A ledge that wraps around a hole would be scored by a path through air. Geodesic-within-`model(k)` is the load's actual route. |

### 3.5 Partially supported regions — Finding 1's defect, closed explicitly

The defect being replaced is `island_coverage.rs`'s early exit:

```rust
if is_attached(&comp, model_raster, k) {
    // Continuing material, not a detached island at all.
    continue;
}
```

One attached pixel anywhere in a component skips the entire component. `reach` has no
such clause: it is per pixel, and attachment enters only as a **seed at distance zero**.
A ledge attached along one edge scores near zero at the root and `L` at the tip, and the
gate fires on the tip. A cavity ceiling continuous with the surrounding wall scores the
in-plane distance from its interior to that wall. A wing growing one pixel per layer
scores one pixel. All three are the cases §3.7 of the invariants document lists as
invisible; all three are now measured.

Nothing in the definition is component-scoped except the final grouping for reporting.

### 3.6 The metric, its error, and how it is computed

**Metric.** 8-neighbour chamfer with true step lengths: `0.014` mm horizontally,
`0.019` mm vertically, `d2 = 0.0236008474` mm diagonally. On this anisotropic lattice
that chamfer **overestimates** the true in-plane geodesic distance by at most
**12.0418 %**, attained at the direction `v/u = (d2 - 0.014)/0.019 = 0.505309`
(the other branch peaks at 5.2615 %). Both are computed, not quoted.

Overestimating reach makes the gate refuse *more* geometry, which is the failing
direction and therefore the safe one — but it is a real cost and it must be visible, not
buried. The implementation **must** report
`report.span_metric_conservatism.regions_passing_under_exact_metric`: the count of
regions that fail under the chamfer metric and would pass under exact Euclidean geodesic
distance. If an implementer replaces the 3×3 chamfer with a 5×5 (knight-move)
neighbourhood, the maximum overestimate must be **re-derived and stated in the code**,
not assumed to be the same.

**Computation — batch-first, and a per-pixel grid scan is a defect.** Because every
carrier pixel is a seed at distance zero, propagation never enters attached or
support-covered material. **The working set is exactly `unattached(k)` minus the pixels
carried by supports**, whose total area over the model is the down-face area `D_raster`
the audit already computes in §2.1. So:

- one multi-source shortest-path pass **per layer**, seeded from every carrier pixel
  adjacent to the working set simultaneously — never one pass per region and never one
  per pixel;
- a bucketed queue (Dial) with quantum `min(0.014, 0.019) / 2 = 0.007 mm`, or a binary
  heap; either is `O(n)` or `O(n log n)` in the working set, not in the grid;
- pixels are materialised only for the working set, from the run rows, inside the
  bounding box of `unattached(k)` — a 16K full-layer bitmap is never allocated;
- the allocation must be counted against the existing raster budget
  (`RasterError::BudgetExceeded`), and exceeding it returns **indeterminate**, never
  pass.

---

## 4. The self-supporting case, and `OVERHANG_ANGLE_DEG`

**This criterion applies no angle rule and does not read `OVERHANG_ANGLE_DEG`.** That is
a deliberate result, not an omission, and it is worth stating why.

A planar surface at angle `α` above horizontal recedes by `Δr = layer_height / tan α`
per layer. Every pixel of that surface's down-face is unattached, and every one of them
is within `Δr` of attached material on the layer's own plane. So for a uniform ramp:

```
reach = Δr = layer_height / tan(α)
```

The criterion therefore already **contains** an angle rule as a consequence:

```
alpha_min = atan(layer_height / L_SPAN_ALLOWED_MM)
          = atan(0.050 / 1.3449762463)
          = 2.1290 degrees above horizontal
```

Two things follow.

1. **The angle is derived, not declared.** `alpha_min` is an output of the measured
   span, not an input to it. Nothing in the gate reads a degree value.
2. **`OVERHANG_ANGLE_DEG = 45.0` is not merely unmeasured — the only relevant
   measurement contradicts it by a factor of 26.9 in reach.** At 45° a layer reaches
   0.050 mm past the layer below; the coupons say 1.34 mm reaches acceptably. Its
   registry provenance is `NONE`, with the source note *"Legacy tool convention lacking
   the required geometric-extent criterion"*
   (`crates/plinth-core/src/constants/placeholders.rs:1302`,
   `docs/evidence/remediation-2026-08-26/audit-constants.md`), and task #66
   (`docs/status.md:150`) exists to replace it by coupon measurement.

**Recorded uncertainty, because this criterion is not free of the problem — it relocates
it.** The coupon measured *one* layer reaching `L` past a **rigid spine**. A shallow ramp
is *every* layer reaching `Δr` past a root that is itself a cantilever, and each layer's
deflection accumulates into the next. The plates never tested that case: plate 1 finding
4 says only that high versus low position on a mast does not matter, which is about
where a finger sits, not about a compliant root. **The criterion is therefore optimistic
for long shallow ramps by an unmeasured amount**, and that is the single largest
weakness in this document. It is repeated in §8 and it is the first thing a reviewer
should attack.

This criterion does not block task #66 and #66 does not block it. But the note is worth
recording the other way round: the constant's own provenance string says what is missing
is "the required geometric-extent criterion", and **this document is that criterion**.
If #66 is ever printed, the right coupon is not an angle fan — it is a *ramp* ladder at
fixed layer height, which is what would close the compliant-root gap above.

---

## 5. Cavity ceilings

`docs/physics.md` §8, quoting the owner's ruling of 2026-08-13: *"We DO want supports
inside the model. The hollow must be supported… those supports will likely land on the
lower part of the interior of the cavity."* A support is legitimately seated on the
plate, on the model surface **including a cavity's own interior wall**, or on another
support, and a cavity ceiling must be supported from the cavity's lower interior.

**The criterion applies inside a cavity with no change of definition, and this is the
main reason to define reach on the raster rather than on the mesh.** The raster has no
notion of inside or outside a hollow: at the layer where a cavity closes over, the
ceiling pixels are simply model pixels whose own columns were void at `k-1`. They are
`unattached(k)` by the same predicate as any exterior ledge, and their reach is measured
by the same pass.

Three consequences, all of them free:

1. **A ceiling continuous with the cavity wall is scored by its distance to that wall.**
   The geodesic runs through the layer's own cured material, so a ceiling closing over a
   cavity of in-plane inradius `R` scores `reach ≈ R` at its innermost pixel. The gate
   therefore says: **an unsupported cavity may close over only if the closing
   cross-section's inradius is at most `L_SPAN_ALLOWED_MM`** — a maximum unsupported
   cavity width of `2 × 1.3449762463 = 2.6899524925 mm`. That number is a *consequence*
   of the span constant, not a second constant, and must not be registered as one.
2. **The support that rescues a wider cavity must be inside it.** A carrier pixel must
   share a column with the ceiling at layer `k` or `k-1` (§3.2), and `support_borne(k)`
   requires that support's component to be attached to support material at `k-1` — which
   inside a sealed cavity means it came up through the cavity. An exterior support
   cannot satisfy a ceiling pixel's reach from outside the wall, because the geodesic to
   it runs through the wall and out, which is a longer path, not a shorter one. Where the
   interior column actually seats, and whether its path is closed, remain I5's and the
   seating rule's business.
3. **Drain and vent bores are cavity ceilings too.** A bore's own roof closes over the
   same way, and the same 2.6900 mm ceiling width applies to it.

**This clause is unexercised by the corpus and that fact is itself a finding.** The
engine emits **zero hollow files** today (hollowing on the treant is a known regression),
so no real model in the 2026-08-30 baseline reaches this path. A criterion whose cavity
clause has never fired is a criterion whose cavity clause has never been shown to work.
§10 makes the cavity fixtures mandatory for exactly that reason.

---

## 6. The gate

### 6.1 Predicate

```
for every layer k above the first cured layer:
  for every down-face region C of unattached(k):
      max over p in C of reach(p)  must be  <= L_SPAN_ALLOWED_MM
```

with the one censor carried over unchanged from the current I7:

> A region whose `reach` is `+infinity` (no carrier reachable at all) and whose area is
> **at most one pixel** (`pixel_area = 2.66e-4 mm²`) is not counted. That is the
> printer's own noise level.

The censor applies **only** to the infinite-reach case, exactly as today. It does **not**
apply to regions with a finite reach: a region large enough to have a measurable reach is
never censored by area, because area is not what this gate measures.

### 6.2 Failure codes and lines

Format follows `audit-invariants.md` §3. Two codes, because the two failure modes are
physically different and the 2026-08-30 baseline rows must stay comparable.

```
ISLAND_UNSUPPORTED file=<path> model=<m> layer=<k> z_mm=<v> z_above_plate_mm=<v>
    area_mm2=<v> centroid_mm=<x,y> pixels=<n>
  a region of model material is born at this layer with no path to anything carrying it

SPAN_UNSUPPORTED file=<path> model=<m> layer=<k> z_mm=<v> z_above_plate_mm=<v>
    reach_mm=<v> allowed_mm=<v> at_mm=<x,y> nearest_carrier_mm=<x,y>
    carrier_kind=<model|support> region_area_mm2=<v> region_pixels=<n>
  down-facing material at (<x>, <y>) mm, <z_above_plate> mm above the plate, reaches
  <reach_mm> mm from the nearest thing carrying it; the measured allowance is
  <allowed_mm> mm

SPAN_INDETERMINATE file=<path> model=<m> layer=<k|n/a>
    reason=<slice_failed|raster_budget_exceeded|distance_budget_exceeded>
  the span of down-facing material could not be established here
```

**The location fields are the point of the check and are not optional.** The owner's
complaint is that specific parts are missing supports, not that a count is high. So:

- `at_mm` is the arg-max pixel's centre **in the model-bounding-box-centred frame**
  (`audit-invariants.md` §1.2) — the same frame the `.chitubox` stores, so the pair can
  be typed straight into ChituBox and found.
- `z_mm` is that layer's slice plane in the same frame; `z_above_plate_mm = z_mm -
  z_plate_mm` is the height on the actual print, which is how the owner looks at a part.
  **Both** are emitted; neither substitutes for the other.
- `nearest_carrier_mm` and `carrier_kind` say what *is* holding it, so "no support here"
  is distinguishable from "the support is 4 mm away".
- `region_area_mm2` distinguishes a missing support under a cloak from a speck.

One line per **region**, not per pixel. Regions are emitted worst-reach first, capped at
**64 per model**, with `measured.failures_truncated = <n>` carrying the remainder.
Truncation must never reduce `failed`.

### 6.3 Schema and counting

The check keeps the id **`i7_island_coverage`** and its JSON key. `audit-invariants.md`
§5 rule 1 pins the key set, and renaming it would silently orphan every consumer. Only
the title and headline change.

```
checked        = number of down-face regions examined across all layers
failed         = regions failing under either ISLAND_UNSUPPORTED or SPAN_UNSUPPORTED
indeterminate  = regions (or the whole model) whose reach could not be established;
                 indeterminate > 0 forces status = "fail"
```

**`checked` changes meaning.** It counted detached islands (1,344 across the corpus, 415
unsupported); it now counts down-face regions, a strict superset.
`docs/evidence/audit-baseline-2026-08-30.md` must record that its I7 column is not
comparable across this change, and the re-run must publish both numbers once.

`measured` must carry, per §5 rule 3, numbers for every region and not only failures:
`layers_scanned`, `regions_checked`, `regions_infinite_reach`, `max_reach_mm`,
`max_reach_location_mm`, `max_reach_layer`, a reach histogram in 0.05 mm bins, and the
four derived inputs of §2.6 with their provenance. **A passing invariant that reports no
numbers is not evidence** — and on this check specifically, "0 regions checked" is the
exact shape of the failure this project keeps reproducing (§10).

### 6.4 The non-gating companion

```
report.span_margin                                  verdict "disclosed", never "pass"
    regions whose reach lies in [L_SPAN_ALLOWED_MM, 2.592036) mm - refused by the gate,
    but below the shortest span at which a failure was actually recorded. This is the
    price of the 1.53x condition-shift discount of section 2.6, made countable, so that
    if the aliased factor is ever identified the refusals that would flip can be listed
    rather than argued about.

report.span_metric_conservatism                     verdict "disclosed", never "pass"
    regions_passing_under_exact_metric - the price of the chamfer overestimate (3.6).
```

`report.overhang` **survives unchanged in its numbers** but its prose does not: the
sentence describing it as "the cantilever exposure I7 does not gate" is now false and
must be replaced. Its `max_unattached_run_span_mm` must be relabelled a row-run
diagnostic with an explicit note that **it is not the span this gate measures** (§3.4).

---

## 7. Tolerance direction — Finding 3 conformance, sign by sign

The review found `delivered >= required - tol` in the existing checks, which passes a
value inside the uncertainty band. Every tolerance in this document is stated with its
sign and its direction of safety.

| quantity | uncertainty | direction it is applied | why |
|---|---|---|---|
| raster erosion (full containment) | `2 * d2 = 0.0472016949 mm` | **subtracted from the allowance** (§2.6) | cured ⊆ true, so measured reach understates true reach; the allowance absorbs it |
| chamfer metric | up to **+12.0418 %** on reach | left in, **not** removed | it inflates reach, which fails more; removing it would pass values it inflated |
| condition shift (aliased factor) | **1.31–1.53×** | **divided by the larger, 1.53** (§2.6) | a larger divisor is the smaller allowance |
| ladder resolution | **1.2169×** between rungs | taken at the **lower** rung, 2.130032 not 2.592036 | the upper rung is a length whose own experiment recorded a failure |
| unread cells (plate 4 rungs 1–6) | 50 of 100 | **excluded, not imputed** (§2.3) | plate 4 Rule 2 |
| comparison at the boundary | — | the gate is `reach <= L_SPAN_ALLOWED_MM`, with **no epsilon added to the right-hand side** | an epsilon there would re-create exactly the defect being fixed |

There is no tolerance anywhere in this document whose effect is to let a value pass that
would otherwise fail.

---

## 8. What this criterion cannot see

Required section. A reviewer should start here.

1. **Cumulative deflection on a shallow ramp — the biggest hole.** The coupon measured
   one lamina reaching `L` from a rigid spine. A ramp is every lamina reaching `Δr` from
   a root that is itself a cantilever, and deflection accumulates. Nothing on either
   plate tested a compliant root. **The criterion is optimistic for long shallow ramps by
   an unmeasured amount** (§4).
2. **The 1.53 divisor is a proxy.** It is a *within-plate* contrast (ascending versus
   descending on plate 4) used as a stand-in for a *between-build* difference (plate 1
   versus plate 4). The factor is unidentified — four aliased candidates, none
   eliminated. If it is orientation-in-peel, a miniature's overhangs point in every
   direction and the worst case may be worse than 1.53×.
3. **One layer height, one resin, one printer, one film age, one ambient** — the last of
   which was never recorded on either plate (§2.7, `docs/physics.md` §11).
4. **Peel direction and plate position are invisible.** Plate 1's single rear-station
   failure (`D5`, at the hinge) is unexplained with three live causes, and its
   discriminating test — one Y, spread across X — has never been run. Plate 4's X row
   tests X-uniformity at one Y only. This criterion scores a ledge facing −Y at the plate
   front identically to one facing +Y at the rear.
5. **Adequacy of the carrier is not checked here.** A single support pixel touching the
   far tip of a ledge collapses reach along its whole length. Whether that support
   reaches the plate is I5; whether it is thick enough is I2/I3/I4; whether it is live is
   I8. **Reading this check alone will overstate the plan's quality**, and the output
   must say so in its headline.
6. **A bridge is scored as two cantilevers.** Material carried at both ends spanning `S`
   scores `reach ≈ S/2` and passes up to `S = 2.69 mm`. A bridge is genuinely stiffer
   than a cantilever of half its span, so this is physically defensible — but the coupon
   measured a cantilever and **the extrapolation is untested and runs in the passing
   direction.**
7. **Width is only covered over 1–4 mm.** Plate 1 varied finger width (`AW1` at 1.00 mm,
   `AW4` at 4.00 mm, the rest 2.00 mm) and lost nothing, which is why the criterion is
   width-blind. Nothing was tested below 1.00 mm. A 0.2 mm filament reaching 1.3 mm is
   outside the evidence.
8. **Sag is not gated at all.** The plates measured *loss*. Sag magnitude was never
   measured and no positive control exists on either plate. A region that reaches 1.3 mm
   passes here and may still print visibly drooped — which is the owner's complaint, and
   this criterion does not close it.
9. **The failure mode was never identified.** Plate 4 §3.4: the graduation ticks sit on
   the last-printed top face while every crack plane is between the first-printed bottom
   laminae, so the ruler is on the lamina that did not fail. Peel versus tear is
   unmeasured. This criterion inherits that: it predicts *whether*, never *how*.
10. **Full-containment erosion is not sign-uniform.** It shortens a region (which
    shortens reach — optimistic) while also removing its rim (which can disconnect a thin
    neck and *raise* reach — pessimistic). §2.6 absorbs the first with `2 * d2`; the
    second is not bounded here.
11. **It is a per-layer, in-plane measure.** It cannot see a region that passes at every
    layer while the assembly progressively droops across many layers, and it cannot see
    out-of-plane load paths through neighbouring layers.
12. **It says nothing about supports that are present but wrong.** A support seated on a
    scar-forbidden face, or one that will not release, satisfies `carrier` completely.

---

## 9. What of `island_coverage.rs` is superseded, and what must survive

### Superseded — delete or replace

| item | disposition |
|---|---|
| the early exit `if is_attached(&comp, ...) { continue; }` | **Deleted.** This is Finding 1's defect in one statement: one attached pixel skips the whole component. Attachment becomes a distance-zero seed (§3.5). |
| "detached island" as the *whole* of the invariant | **Demoted** to the special case `reach = +infinity` (§6.1). |
| `is_supported()` — bare pixel overlap with the support raster at layer `k` | **Replaced** by `carrier(k)` (§3.2), which adds the `support(k-1)` clause and the `support_borne` component-attachment requirement. |
| the headline `"…cantilevers are disclosed, not gated, in report.overhang"` | **Replaced.** It is now false. |
| the module doc's "What this predicate does NOT gate" paragraph naming the 20 mm ledge and the cavity ceiling | **Replaced** by a pointer to this document and to §8 for what remains uncovered. |
| `audit-invariants.md` §3.7's paragraph rejecting the one-pixel-per-layer rule as "far too strict" (74.36° / 69.19°) | **Superseded.** The rule is not needed: the criterion is not angle-based, and §4 derives the implied angle instead of declaring one. |
| §3.7's sentence "Gating that needs a measured self-support span coupon the project does not have" | **Superseded.** It does have one — two, pooled — and this document derives it. §6.1 of the invariants document ("I7's cantilever hole is the largest") must be rewritten to point here. |
| `report.overhang`'s prose framing as the ungated cantilever exposure | **Relabelled** (§6.4). Its numbers stay; its `max_unattached_run_span_mm` must be marked as not the span this gate uses. |

### Must survive — do not touch

| item | why |
|---|---|
| `is_attached()`, the per-column attachment predicate | It defines `unattached(k)`, and the same set is `audit-invariants.md` §2.1's down-face reconstruction feeding I2, I3 and I4. Changing it breaks four checks. |
| the first-layer-above-plate exemption, computed as `grid.layer_at_or_above(grid.z_plate_mm)` rather than hard-coded `0` | Correct today and correct after a frame-convention change. Keep it computed. |
| the one-pixel censor, and its strict-boundary semantics | Keep, scoped to the infinite-reach case only (§6.1). The existing test `the_one_pixel_censor_is_a_strict_boundary` stays valid. |
| `ISLAND_UNSUPPORTED` as a code, and its existing field set | Keeps the 2026-08-30 baseline rows readable and separates "born in mid-air" from "reaches too far". |
| `raster_unavailable()` and the whole-model indeterminate path | "Unmeasurable never passes" is contract rule 3. The new `distance_budget_exceeded` reason joins it; it does not replace it. |
| `component_area_mm2` / `component_centroid_mm` / `component_pixel_count` | Reused verbatim for the region reporting of §6.2. |
| the run-length representation and its interval intersection | `audit-invariants.md` §1.6: a per-pixel loop over a 16K grid is a defect, not a choice. §3.6 keeps that binding. |
| all four existing unit tests | `an_unsupported_floating_island_fails_with_a_named_location` (including its pixel-exact 0.949620 mm² assertion), `a_column_reaching_into_the_island_makes_it_supported`, `a_model_resting_directly_on_the_plate_is_exempt_at_its_own_birth_layer`, `an_unmeasurable_model_fails_indeterminate_never_pass`. All four assert behaviour this criterion must keep. They are necessary and, as §10 says, nowhere near sufficient. |
| the JSON key `i7_island_coverage` | §6.3. |

---

## 10. Required fixtures, and the sabotage test

**Read this before implementing.** In the run that built this audit tool, the fixture
harness reported GREEN because it had discovered **zero fixtures** — it globbed one path,
the agents wrote to another, and it printed "no fixtures yet" and returned ok. The gate
the whole stage rested on had run nothing. Assume that defect is present here.

The fixtures below are **mandatory and named**, and the harness must assert their
presence by name.

| id | geometry | required verdict |
|---|---|---|
| `span_ledge_long` | flat ledge 3.00 mm long, 2.00 mm wide, one layer thick, attached along one edge to a block; no supports | **FAIL** `SPAN_UNSUPPORTED`, `reach_mm` within `2*d2` of 3.00, `at_mm` within `2*d2` of the ledge tip |
| `span_ledge_short` | the same ledge 1.20 mm long | **PASS**, `max_reach_mm` within `2*d2` of 1.20 |
| `span_ledge_supported` | `span_ledge_long` with one support column embedded under its tip | **PASS**, `max_reach_mm` ≈ 1.50 (the midpoint between root and tip) |
| `span_cavity_open` | closed hollow box, interior 5.00 mm across, ceiling closing over it, no interior support | **FAIL** with `at_mm` inside the cavity footprint and `z_above_plate_mm` at the closing layer |
| `span_cavity_supported` | the same cavity with a column from the cavity floor to the ceiling | **PASS** |
| `span_cavity_narrow` | the same cavity 2.00 mm across, no support | **PASS** (guards against a gate that refuses every hollow) |
| `span_floating_island` | the existing two-box model | **FAIL** `ISLAND_UNSUPPORTED`, `reach = +infinity` — the old behaviour, preserved |
| `span_ramp_shallow` | a plane at 3.0° above horizontal | **PASS**, and `max_reach_mm` within 5 % of `0.050 / tan(3.0°) = 0.954057 mm` — this is the fixture that proves §4's angle derivation is what the code actually computes |

**The sabotage tests. A check that cannot be made to change its answer has not been shown
to be watching anything.**

- **S1** — take `span_ledge_supported`, delete its support record, re-run: the verdict
  **must flip to FAIL**. If it does not, `carrier` is not being read.
- **S2** — take `span_ledge_long`, set `L_SPAN_ALLOWED_MM` to 10.0 mm, re-run: the
  verdict **must flip to PASS**. If it does not, the constant is not being read.
- **S3** — take `span_ledge_long` and move it 7 mm in +X: the reported `at_mm` **must move
  by 7 mm**. If it does not, the location is being computed from something other than the
  failing pixel, which is the whole value of the failure line.
- **S4** — take `span_cavity_supported` and move its interior column outside the cavity
  wall: the verdict **must flip to FAIL**. This is the clause §5 says the corpus cannot
  exercise, and it is the only thing that will exercise it.

**Harness assertions, stated because the harness is what failed last time.**

- The fixture runner must assert `discovered_fixture_count >= 8` **and** that every id in
  the table above was discovered **by name**, and must **fail loudly** on a shortfall.
  "No fixtures found" must never return ok.
- Every sabotage test must assert a **flip**, never merely a status.
- A corpus run over the 24 real miniatures must **report** the number of models failing
  this criterion and the worst `reach_mm` per model. **If that count is zero, that is a
  finding to investigate, not a pass** — 20 of 24 of those files pass none of the eight
  existing gates, and the owner's judgement is that approximately none is worth printing.
  A brand-new gate that greets that corpus with silence is far more likely to be broken
  than to be right.

---

## 11. What would retire the guesswork

In priority order, because the discount in §2.6 is where all the imprecision lives.

1. **A ramp ladder at fixed layer height** — a fan of planar ramps from 1° to 20° above
   horizontal, scored for loss and for sag, printed with the counterbalanced assignment
   plate 4 Rule 1 requires. This closes §8 item 1, the compliant-root gap, and it is a
   better use of a plate than the angle fan proposed for task #66.
2. **A repeat of the plate-1/plate-4 shared rungs on one plate, counterbalanced**, which
   identifies or kills the 1.31–1.53× factor and would remove the `CONDITION_SHIFT_MAX`
   divisor outright. That single change would move the allowance from 1.3450 mm to
   2.0828 mm, a 54.86 % relaxation, so the plate pays for itself in refusals avoided.
3. **A positive control for sag** — a specimen drooped by a known amount — without which
   §8 item 8 cannot be closed at all.
4. **A layer-height cross** (0.030 / 0.050 / 0.080 mm) on the same ladder, which is the
   only thing that would let this criterion travel to another print profile.

Until then this document states a bracket, takes its conservative end, discounts it by a
measured factor it cannot identify, subtracts its own instrument error, and says so in
every place a reader might otherwise take the number for a measurement.
