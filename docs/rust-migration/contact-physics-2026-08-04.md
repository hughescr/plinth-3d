# Contact physics — Phase B-2 design record (2026-08-04)

Status: **PARTIAL — PR-1/PR-2/PR-3/PR-4 (brace path) and PR-6 landed; PR-5,
PR-7, PR-8, PR-9 NOT yet implemented.** See *What is not done* at the end. The
owner's contact-attestation requirement is still **UNMET**.

Repair pass (2026-08-04, stage 1 of 2) resolved re-review findings BLOCKER-2,
BLOCKER-3, MAJOR-4, MAJOR-5, MAJOR-6 and MAJOR-8; see *Repair pass* near the
end. MAJOR-7 (PR-4 is inert — `contact::admit` is called with
`carried_mm2 = 0.0`) is stage 2 and remains OPEN.

Authority: owner ruling **OD-8** (2026-08-04) — behaviour preservation is
lifted, correctness and measured physics govern, and every behavioural
difference must be measured, reported and physics-justified. Silent geometry
change remains forbidden.

Evidence base: the corpus incidence census
(`census-embed-incidence-2026-08-04.md`) and the H2 calibration plate
(`docs/reference/home-support-calibration-p1-protocol.md`).

---

## The falsified premise this package is built on

OD-2 assumed the `.max(0.5)` cosine floor in `embed_inset_mm` was a
grazing-incidence **rejector**. It was the opposite — the **conservative
branch**:

* `seg_clear_one` returns `INFINITY` — candidate trivially **admitted**, no
  check at all — whenever the insets over-mask the segment.
* Flooring the cosine **up** at 0.5 makes the inset **smaller**, the mask
  **smaller**, and the check **stricter**.
* At `|cos| < 0.5` the floored model-rung inset is a constant
  `0.3 + (0.5 + 0.6)/0.5 = 2.50 mm`.

So the floor never rejected anything; it **hid a divergence**. The census
measured the consequence: as little as **19.1%** of a rung was actually
clearance-checked, the other 80.9% exempted by a mask derived from a cosine
that had been clamped away.

---

## The rules

### PR-1 `CONTACT_SEAT_DEPTH` — every model contact is seated, none kisses

```
h = max( default_contact_depth_mm,
         process_resolution_mm + r_end * sin(theta),
         BALL_SEAT_FACTOR * r_tip   [ball-tipped stub ends only] )
```

Three independent requirements, each binding in a different regime:

* **(a) slicer resolution.** A fused region thinner than one layer is not
  resolvable by the printer. `process_resolution_mm = max(LayerPlan heights)` —
  a datum the run already carries, not an invented number.
* **(b) full-cross-section engagement.** A frustum end at incidence `theta` has
  its end disc tilted; only `h >= r_end*sin(theta)` engages the entire end
  cross-section rather than smearing a partial bond. **This term belongs here
  and only here** — it is not a mask term.
* **(c) ball-tip cap geometry.** The plane at depth `h` cuts the emitted tip
  ball in a disc `a^2 = 2*f*r_tip*h - h^2`. Requiring that cut not to be the
  narrowest section (`a >= r_tip`) gives
  `BALL_SEAT_FACTOR = f - sqrt(f^2 - 1) = 0.3819660112501051` at `f = 1.5`,
  **derived in code, never hardcoded**.
  *Validity range:* the ball spans `3*r_tip` against a stub length `h + 2.5`,
  so a shaft exists only while `r_tip < 0.9549150281252627`. Above that the
  stub is ball-dominated and clause (c) is an interface-area maximiser rather
  than a narrowest-section criterion; both readings give the same `h`.

**Seating order (exactly one pass, nothing iterates).** Seat at the UNSEATED
incidence, then evaluate the mask and admissibility on the SEATED geometry.

**Monotonicity lemma (proof).** With `v0 = p_s - q`, `d = v0.n < 0`:
`|v1|^2 = L0^2 + 2h|d| + h^2 >= L0^2`, and `c1 >= c0` because
`L0^2(|d|+h)^2 - |d|^2(L0^2 + 2h|d| + h^2) = (L0^2 - |d|^2)(2h|d| + h^2) >= 0`,
with equality only at normal incidence. Seating raises BOTH `|cos|` and length,
so the seat computed at the unseated incidence remains valid at the seated one.
Over-seating is safe; under-seating cannot occur. **This is what licenses the
single pass** — and it replaces the original design's false justification
("the mask depends on seat depth only additively", which is untrue because the
mask also depends on `1/|cos|` and `|cos|` moves when you seat).

### PR-2 `CONTACT_MASK_EXTENT` — the mask that solves the kernel's ACTUAL criterion

```
u* = ( h + r_max + standoff_mm + MARGIN ) / |cos(theta_seated)|
```

with `r_max = max(r0_mm, r1_mm)` and `MARGIN = process_resolution_mm`.

**What the kernel actually computes.** `seg_clear_one` builds `a` and `b` as
AXIS points at the mask stations, interpolates `r_lo`/`r_hi` there, and tests
`seg_surface_min(a, b) >= r_lo.max(r_hi) + standoff`. It is a **sphere-swept
capsule** criterion, not a tilted-end-disc criterion. The axis point at arc
distance `u` from an endpoint seated `h` below the tangent plane stands
`u*|cos| - h` above it, and on a planar seat that height IS its distance to the
surface. Requiring `u*|cos| - h >= r_max + standoff + MARGIN` gives the formula.

**The `h` term is present only for members that are actually SEATED.** The
depth term exists because the axis station at arc distance `u*` from an endpoint
seated `h` BELOW the tangent plane stands `u*|cos| - h` above it. For an
endpoint sitting ON the plane the station stands `u*|cos|` above it and the term
must not be there — including it over-masks by `h/|cos|`, roughly 0.6-1.1 mm of
the member silently exempted from the distance check. PR-1 seats exactly one
family today, the brace model rung; the CCO model-anchor strut's endpoint IS the
raw projected seat point, and a bifurcation half inherits its host's endpoint
and its host's kind. `contact::seating_of` recomputes that state per stage from
the design family (OD-3 — never stored on `Member`, so no digest moves) and is
the single place the decision lives, so when PR-7 seats the CCO family the mask
term becomes correct there by the same rule rather than by a second, divergent
edit. Pinned end-to-end through the kernel by
`brace::tests::an_unseated_contact_is_not_over_masked_by_the_seat_depth`.

**There is no `r_end*sin(theta)` term in the mask.** The original Phase B-2
design had one; it was wrong and is retracted. At `theta = 0` the sin-based
mask is SHORT by `r_max`, leaving the first unmasked axis point standing only
`standoff + MARGIN` above the seat against a required `r_max + standoff`, i.e.
`clr = MARGIN - r_max < 0` for **every** seated contact on the corpus
(`r_max` in `[0.35, 1.0]`). It would have broken every contact in the package
while every fixture stayed green.

**The planar-seat round trip is pinned against the real kernel.** On a locally
planar seat the kernel must return `clearance == MARGIN` exactly. That is a
statement ABOUT the kernel, so a kernel-free arithmetic restatement would prove
nothing. `contact::tests::planar_seat_round_trip_returns_exactly_margin_from_the_kernel`
builds a real `NativeSurface` half-space, seats a member at the PR-1 depth over
four incidences, hands the PR-2 mask to `segment_clearance` as `inset1` exactly
as `brace.rs` does, and asserts the returned clearance to within 1e-12 mm
(measured agreement: **|delta| <= 6.3e-16 mm**, i.e. a few ULP). A negative
control re-runs the same batch with the legacy under-mask `h*(1/|cos| - 1)`
removed from `inset1` and pins the kernel's degraded answer, so the assertion is
demonstrably not vacuous — and shows why the defect was invisible to flat
fixtures, since the shortfall vanishes at normal incidence.

**`r_max` is a sound, non-circular bound.** `r_lo.max(r_hi)` depends on the
mask, which would be circular; both are convex interpolations between `r0` and
`r1`, so `max(r0, r1) >= r_lo.max(r_hi)` always.

**What was wrong with the legacy formula — one critique, not two.** The
surviving critique is that the depth term must ALSO be divided by `|cos|`:
depth is measured along the normal, arc length along the axis. The legacy form
under-masked by `h*(1/|cos| - 1)`.

**Why MARGIN is not `THRESHOLD_BAND_MM`.** That constant (1e-6) is documented
as result-inert telemetry whose whole purpose is to flag knife-edge clearance
decisions. Sizing every production contact to land one ULP outside it converts
a rare-event diagnostic into a systematic condition, destroys its diagnostic
value, and leaves zero real engineering margin. `MARGIN` is instead the run's
own process resolution: a clearance shortfall thinner than one layer cannot be
rendered by the slicer and so cannot become a fusion defect.

**The divergence is real.** As `|cos| -> 0`, `u*` diverges, because a nearly
tangential member genuinely stays within the standoff band for an unbounded
run. The correct disposition is refusal (PR-3), not a floor.

### PR-3 `CONTACT_EMERGENCE_ADMISSIBILITY`

Admissible iff `u*_terminalward + u*_rootward <= L1 - min_illegal_len_mm`.

Two parameter-free floors, both already named by the engine: the member must
EMERGE (otherwise it is a rod lying inside the standoff band along its whole
length — after union it is absorbed into the mini and disfigures it rather than
supporting it), and the distance check must be able to return a verdict at all
(`axis_illegal_rows` cannot flag a free span shorter than `min_illegal_len_mm`).

**There is no `|cos| >= 0.5` rule anywhere in this design.** The effective
incidence limit is a DERIVED consequence of `r_end`, `r_max`, `h`, `standoff`,
`MARGIN` and `L`, and differs for every candidate.

**Reconciliation with `bracing_model_rung_min_length_mm` = 1.0.** The derived
floor at `r = 0.5`, normal incidence, 0.05 mm layers is **1.50 mm**, so the
registered band `[1.0, 1.50)` is structurally unreachable. The registered
constant is deliberately **not raised**: it is a candidate-GENERATION floor
applied before any incidence is known, not a claim of admissibility, and
raising a registered M0 value would be a corpus-wide behaviour change this
package has no measurement for. The unreachability is pinned by test instead.

### PR-4 `CONTACT_AREA_SUFFICIENCY`

Enforced through the engine's EXISTING predicate rather than a duplicate:

```
sufficient  iff  NOT FrozenTipLoadPolicy::violates_safety_factor(carried_mm2, r_eff)
```

evaluated on the **minimal separating cut** (`r_eff` = the ball cut disc for a
ball-tipped stub end, `r_end` otherwise). Delegation inherits the engine's 1e-6
tolerance and makes the two checks agree at the `[0.2, 1.0]` clamp boundaries
by construction rather than by inspection; a strict reimplementation would
refuse a contact one ULP low and vanish a support that prints fine today.

**The allowance anchor is the registered `X_ALLOWED`.** There is exactly one
registered area-ratio allowance in this engine: the Derived evidence constant
`X_ALLOWED = floor(lower(X_STAR_CONE)/2) = 12`, chained `from = ["X_STAR_CONE"]`
(`crates/plinth-core/src/constants/evidence.rs`). PR-4 reads THAT entry. An
earlier revision of this package registered a second, hand-typed
`CONTACT_AREA_RATIO_ALLOWED = 12.0` Placeholder carrying the same number; that
duplicate has been **deleted**. A duplicate is not a redundancy but a
divergence hazard — a future plate recalibration would move `X_STAR_CONE` and
`X_ALLOWED` with it while the hand-typed contact threshold silently stayed at
the stale 12. The registry is back to **212 constants (192 placeholders)** and
the one-sided engine-vs-allowance gate now reads `X_ALLOWED`.

**`X = 12` is the H2 engineering allowance.** The engine's implied ratio
`tensile_green_pa / (peel_pressure_pa * tip_safety_factor) = 12` is an internal
CONSISTENCY identity — a function of the arbitrary Python-era
`tip_safety_factor = 8`; had it been 6 the identity would read 16. It is **not**
independent corroboration. What matters is only the safety direction, so the
constants gate asserts a ONE-SIDED bound plus a soft review trigger, never an
exact-equality coupling that a future recalibration would silently propagate.

**`A_contact` is WITNESSED, not proven.** Eight capsule rim rays plus the
centreline penetrating from `s = 0` witness full-cross-section engagement. That
inference is EXACT for locally convex or planar seats and is a SAMPLING BOUND
otherwise. The concave regime is unmeasured on this corpus.

**Transfer statement (required by the mandate).** The H2 law was measured on
head/shaft area ratios of purpose-built coupons under peel plus gravity on a
Saturn 4 Ultra tilt-peel printer, front-to-back release, with arm and head both
support material. Applying it to a support-to-model interface transfers it
across three boundaries: (1) the interface is a virtual cut through one
continuous cured solid rather than a machined section — justified because in
resin printing the union is monolithic and there is no bond line, so a
minimal-cut criterion is the correct generalisation of "narrowest section";
(2) the loaded material is model resin rather than support resin — same resin,
same green state, so sigma is unchanged; (3) `X = 12` sits a factor of ~2-3
below the measured boundary `X*` in (25, 36), and that margin is what absorbs
the mode difference between the coupon's tension/peel and the interface's mixed
tension-shear at incidence. No tension/shear interaction criterion is attempted
because `tau/sigma` for green resin is unmeasured; **that is the place to spend
the next calibration plate.**

### PR-6 `CONTACT_DECLARATION_FOLLOWS_ITS_END` (OD-7)

A bifurcation PARTITIONS the declaration between the halves and never
duplicates it. `upper` (junction..p1) inherits only `Rootward`; the lower half
keeps `Terminalward`. `upper.p0` is a mid-air point on the host's own axis and
carries no contact.

---

## MEASURED CORPUS DELTA

Harness: `crates/plinth-support/examples/embed_incidence_census.rs`, verified
result-inert and now behind `--features census`. Baseline captured at `50b325c`
before any change; re-measured after the repair pass below.

### Placed geometry — unchanged on every reachable model

| model | routed_contacts | final_live_members | snapshot_digest | changed? |
| --- | ---: | ---: | ---: | --- |
| Sphere | 7 → 7 | 15 → 15 | 17667468971446251870 | **no** |
| make_box (substituted orientation) | 8 → 8 | 19 → 19 | 8247399005871372676 | **no** |
| make_overhang_table | 73 → 73 | 261 → 261 | 6686599816688114065 | **no** |
| make_thin_fin | 17 → 17 | 41 → 41 | 7016743535301716631 | **no** |

Cylinder, make_toed_slab, make_raking_blade and the Weeping Treant remain
UNREACHABLE (blocked upstream at orientation/specialized/perception — separate
work in flight). **4 of 8 models reach the support engine.**

### Candidate set — changed, attributed, and physics-justified

`make_overhang_table` is the only model generating model-rung candidates.

| measure | before | after | rule |
| --- | ---: | ---: | --- |
| model-rung candidates | 8 | **2** | PR-3 refusal |
| rows built from a floored cosine | 7 | **0** | PR-2 (floor deleted) |
| model-rung candidates revalidated viable (T1a) | 3 | **2** | PR-3 refusal |
| min checked fraction, `brace_candidate_clearance` | 0.191090 | **0.339004** | **PR-3** (set composition) |
| median checked fraction | 0.472499 | **0.621539** | **PR-3** (set composition) |

**Attribution correction (MAJOR-5).** An earlier revision of this table credited
the checked-fraction movement to PR-2. That is wrong, and in the flattering
direction. Both fractions are aggregates over a set whose MEMBERSHIP changed:
the 0.191090 minimum belonged to the extreme grazing row
(`|cos| = 0.003789`, `L = 3.090579`, floored inset 2.50 mm, usable 0.590579 mm),
and PR-3 **deletes that row**. The aggregate improved because the worst rows are
gone, not because any surviving row is better checked.

Per-row, PR-2 moves the checked fraction in the OPPOSITE direction wherever the
floor was active — which is the honest consequence of deleting a floor that was
making the mask artificially small:

| survivor | before (floored) | after (PR-1+PR-2) | direction |
| --- | ---: | ---: | --- |
| row 0, raw `\|cos\|` = 0.431225 — floor WAS active | 0.460113 | **0.339004** | **down** |
| row 5, raw `\|cos\|` = 0.862506 — floor never active | 0.615757 | **0.621539** | ~flat (up) |

Row 0's mask grows from the floored 2.500 mm to the honest 3.218 mm, so 12.1
percentage points MORE of that rung is now exempt from the distance check than
before. That is not a regression in checking quality: the floored 2.500 mm was
never a claim about that rung's geometry, it was a constant substituted for one.
The correct comparison is that the row is now masked by the extent its own
incidence actually requires. Row 5, where the floor was never active, is
essentially unchanged — seating raises `|cos|` (0.862506 → 0.879755), which
grows length slightly faster than mask.

**T1a `admitted_viable` (restored).** The pre-package baseline was 3 of 8 rung
candidates revalidated viable. Under PR-3, 6 of those 8 candidates never reach
revalidation, so the reachable maximum is 2 — and both survivors are viable, so
the measure now reads **2**. It read **1** between the PR-1 seating commit and
the BLOCKER-3 repair, because seating had silently converted the tautological
printability check into `|normal_z| * h <= 1e-9`. The 3 → 2 step is PR-3; the
2 → 1 → 2 excursion was the defect and its repair.

The two survivors are exactly the two the design predicted, and their seated
values reproduce the golden table:

| i | raw \|cos\| | seated \|cos\| | L1 | mask u* | free span | verdict |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 0 | 0.431225 | 0.513166 | 4.867688 | 3.217523 | 1.650165 | ADMIT |
| 1 | 0.862506 | 0.879755 | 4.364047 | 1.651622 | 2.712425 | ADMIT |

**Why the candidate set moves but no geometry does.** The census established
that model rungs never win their tournament against cross-links on this corpus
(`brace_append_model_rung` fires zero times). Refusing six of eight rung
candidates therefore changes no placed member. This is the expected OD-8
outcome: a measured, attributed change in what the engine *considers*, with
zero silent change to what it *builds*.

**PR-6 delta: none.** Census output is byte-identical, confirming the OD-7 bug
is latent on today's corpus exactly as predicted — zero non-stub members carry
any contact end.

### Divergence from the design's published golden table (recorded)

The implementation reproduces the design's 8-row table to within measured
tolerances, with two causes of divergence, both in the design record rather
than the implementation:

1. **Input rounding** (7 rows). The table reports raw `|cos|` and `L0` rounded
   to 6 dp while its derived columns came from full-precision census values;
   `d(c1)/d(c0) > 1` amplifies that. Measured: `c1 <= 4.2e-5`, `L1 <= 2.3e-5`,
   `mask <= 4.7e-4`. Row 5 agrees to 3e-6 throughout.
2. **An internal slip in row 2's mask column.** Row 2's own published `h`
   (0.549996) and `c1` (0.178818) give `9.50685`, not the printed `9.508010` —
   the table is inconsistent with itself by 1.16e-3 there, independently of
   this code.

**No verdict moves.** The nearest row to its boundary (row 6) misses admission
by 0.398 mm, nearly three orders of magnitude clear of the worst divergence.

### Instrument correction

The census's `floored=` and `floored_cos=` columns were renamed `raw_grazing=`
and `seated_grazing_cos=`. Nothing is floored any more, and `cos_used` now
differs from `abs_cos_raw` because SEATING raises the incidence. Reporting that
as "floored" would have made the OD-8 delta instrument lie about the very
change it measures.

---

## Repair pass (2026-08-04, stage 1 of 2)

Six findings from the independent Phase B-2 re-review, resolved on the same
branch. Every behavioural item was measured with the same census harness before
and after.

| finding | disposition | corpus delta |
| --- | --- | --- |
| **BLOCKER-2** duplicate constant | `CONTACT_AREA_RATIO_ALLOWED` deleted; `X_ALLOWED` is the sole anchor and the gate reads it | none (nothing read it) |
| **BLOCKER-3** rung printability | evaluated at the UNSEATED projection point; PR-3 is the explicit incidence gate | T1a `admitted_viable` 1 → **2** |
| **MAJOR-4** false evidence claim | the planar-seat round trip is now pinned against the real kernel, with a negative control | none (test-only) |
| **MAJOR-5** delta misattribution | checked-fraction movement re-attributed to PR-3 set composition; per-row direction stated | none (record-only) |
| **MAJOR-6** over-mask for unseated families | `contact::seating_of` supplies the mask's depth term per family | none (latent — no non-stub member on the corpus declares a contact) |
| **MAJOR-8** merge hygiene | census quarantined behind `--features census` | none (byte-identical instrumented run) |

**The entire measured delta of this repair pass is one census cell.** Diffing the
full instrumented census before and after the six commits yields exactly one
changed line — `T1a_rung_outcome make_overhang_table admitted_viable 1 -> 2` on
`make_overhang_table`. Every `snapshot_digest`, `routed_contacts` and
`final_live_members` on all four reachable models is unchanged, so no placed
geometry moves.

**MAJOR-7 is deliberately NOT in this pass.** `contact::admit` has one
production call site and it passes `carried_mm2 = 0.0`, so PR-4's area law gates
nothing today. Wiring the real per-tip loads (`FrozenTipLoadPolicy` is the load
authority) belongs to the Attestation stage together with PR-8, and documenting
which families legitimately carry zero load is part of it. `admit`'s signature is
left stable so that stage can extend it compatibly.

---

## What is not done

**The owner's contact-attestation requirement remains UNMET.** These rules are
designed (full text in the Phase B-2 design record) but NOT implemented:

* **PR-5 `MASK_INTERIOR_SINGLE_CONTACT`** — the masked interval is currently
  still an unchecked blind spot. `designed_embed` does not yet carry two
  extents, and the witness lacks clauses (c) and (d).
* **PR-7 `CCO_ANCHOR_MASK_AT_THE_SEAT`** — the CCO model-anchor family remains
  arithmetically dead (`clr <= 0.6 - 0.95 = -0.35` unconditionally). The
  corrected PR-2 mask is what would revive it; that revival is **not** landed,
  and it carries a genuine print-consequence tradeoff (scarring the mini with a
  family that has never once fired) that is reserved to the owner.
* **PR-8 `AFFIRMATIVE_CONTACT_ATTESTATION`** — **the headline gap.**
  `designed_embed`'s batched verifier still has no production caller, and there
  is still no touching-foot contact verification of any kind. This was the
  Phase B re-review's UNMET finding and it is still UNMET.
* **PR-9 `CONTACT_CLEARANCE_REFUSAL_IS_NOT_A_PIPELINE_VIOLATION`** — a seated
  contact reading negative clearance still flows into `exact_clearance_violations`
  and can hard-error via `pass_three_termination`, which OD-4 forbids.
* **MAJOR-7 — PR-4 IS INERT.** `contact::admit` has exactly one production call
  site (`brace.rs` model-rung gather) and it passes `carried_mm2 = 0.0`,
  `ball_tipped = false`, `tip_clamped_at_max = false`. The H2 area law therefore
  gates nothing today: with zero carried load every candidate is sufficient. The
  Attestation stage must wire the real per-tip loads (`FrozenTipLoadPolicy` is
  the load authority) into that call and document which families legitimately
  carry zero. `admit`'s signature is deliberately left stable so that wiring can
  extend it compatibly.

Also still absent, and named by **OD-9**: the batched EXTERIOR-NESS screen (no
attachment to buried surface). OD-9 postdates this design, and its adjudication
hook says the screen lands on this branch before it retires. Nothing in this
package subsumes it — a touching foot on a buried patch is genuinely "on the
surface" and an embedded tip there passes an endpoint-connectivity proof
trivially.

Consequently `initial.rs` still seats stubs at a flat depth, `rewire.rs` and
the CCO clearance rows still use flat cosine-free insets, `lower.rs` still
stamps a global contact depth, and no verification gate guards against a
contact refusal silently deleting a support.

**Follow-ups recorded:** FU-CONTACT-1 (the `tip_radius_max_legacy_mm` clamp is
the real limit on the 10 largest-tip contacts, not a contact defect);
FU-CONTACT-2 (`is_embedded_taper`'s `r0 != r1` clause is no longer a proxy for
anything under PR-1).
