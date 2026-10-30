> **Evidence record — non-normative.** Relocated from `docs/engineering-journal/reference/plate-06-verification-physicist.md` in the 2026-08 docs restructure; content preserved verbatim below.

> **Appendix to [`plate-06-results.md`](../engineering-journal/reference/plate-06-results.md) — failure-mode physics memo for calibration plate 6.** Preserved unmodified below this header.
> Produced 2026-07-31 by an adversarial-verification panel (two blind analysts plus a refuter) reading the plate-6 scoring; header added on filing, nothing else changed.

# Plate 6 read — failure-mode physics memo

**Analyst lens:** most-parsimonious physical model that must account for all 34 specimens. Sources read: `PLATE-LAYOUT.md` §plate-6 (L1258+), `params.py` (P6 block, L946-1310), `spec_p6.py` (L540, L716), `predictions.py`, and the three train photos (18 of 34 specimens spot-checked).

**One design fact the summary omitted, and it turns out to be the load-bearing one.** `params.py:1152`:

```
P6_CAP_PLANE_MM = {"B6": 8.500, "A6S": 10.950, "A6L": 12.000}
```

Cap undersides sit in **three separate z bands**: B6 at 8.500 (top 9.200), A6-S at 10.950 (top 11.650), A6-L at 12.000 (top 12.700). Cap thickness is identical everywhere (`P6_CAP_T_MM = 0.50`, used by both families). So "family" is perfectly confounded with "cap-plane height", and A6 splits into two sub-populations that must be ruled on separately.

---

## 1. Validity ruling, family by family

### B6 — **VALID**. Clean, no caveats.
- `PB` (blind poscon) dead as pre-registered. ✔
- All three deaths (`M1` X_tot=60.15, `M2` X_tot=30.48, `PB` X_tot=209.29) are exactly the cells a ratio law condemns. **No B6 cell died that any model says should have lived, and no B6 cell lived that any model says should have died.** 17/17 explained by a single threshold.
- No partial caps observed in the photos (`2WY`, `3TX`, `M3`, `M4` all whole; `PB` has neither cap nor visible arms). The VOID rule was not triggered on the 18 specimens I could see.
- **Caveat to flag:** trains 2, 3, 5 were not photographed. That means the headline survivor `3TY` and the sole surviving validity control `KL` — the two most load-bearing scored outcomes on the plate — rest on naked-eye reads alone. Photograph trains 3 and 5 before publishing.

### A6-S (11 specimens, cap plane 10.950) — **VOID, and void for a stronger reason than the gate.**
The pre-registered gate fires verbatim: `KS` (X = 8.00, MUST SURVIVE) is gone. But the decisive argument is internal and does not need the gate:

> **A6-S is 0/11 alive across X = 8.00 → 409.28 (51×) and d_arm = 0.247 → 1.767 mm (7.2×).** An outcome that is *invariant* across a 51× range in the load ratio and a 7× range in the failing cross-section **cannot be a load-capacity outcome.** A capacity boundary is by definition a function of arm area. This one isn't. Confidence: **very high.**

It is also not a *stochastic* handling loss: the B6-calibrated law predicts 6-8 survivors on this ladder, and a random loss process would have to have per-specimen probability ≈ 1 to erase all of them, at which point it is not stochastic — it is systematic. Whatever removed these 11 caps is shared by all 11: the 4.997 mm disc, the 10.950 mm cap plane, or their common layer band. **A6-S contributes exactly zero information about X\*.**

### A6-L (6 specimens, cap plane 12.000) — **formally void, physically corroborating.**
Void by the pre-registered rule ("if *either* KS or KL is gone…" — KS is gone, so the family is untrusted; KL's survival does not rescue it). But unlike A6-S, this ladder *does* depend on arm diameter and is monotone except for one cell:

| d_arm | X | outcome |
|---|---|---|
| 4.427 (`KL`) | 7.97 | ALIVE |
| **3.952 (`L1`)** | **10.01** | **DEAD ← the one inversion** |
| 3.116 (`L3`) | 16.10 | ALIVE |
| 2.508 (`L5`) | 24.85 | DEAD |
| 1.995 (`L7`) | 39.27 | DEAD |
| 0.627 (`ZL`) | 397.58 | DEAD |

Drop `L1` as a single stochastic loss (it is the end specimen, pos 6 of 6, of train 1 — the first train hand-lifted, and the biggest paddle on the plate) and the ladder brackets **X\* ∈ [16.1, 24.85) at 6.00 mm free length** — overlapping B6's bracket. I report this as *supporting, not admissible*.

### Controls, honestly scored
`ZS`/`ZL` "behaving" is worth nothing here — A6 killed everything, so the must-fail controls could not have failed to behave. `PB` behaving is worth something (it proves B6 could produce a failure), but `PB` fails under *both* competing models (X_tot = 209 ≫ X\*), so it is a validity control, not a mechanism control. The entire mechanism discrimination rests on the 12 strip cells vs `M1`.

---

## 2. What the plate MEASURED

**(a) Single-arm capacity, constant-diameter cylinder, 4.00 mm free length, 30.08 mm² flat cap.**
`M3` (19.91) ALIVE, `M2` (30.48) DEAD ⇒ **X\* ∈ (19.91, 30.48], point estimate 24.6** (geometric mean), ladder resolution 1.53×. **Confidence: high.** This independently reproduces plate 5's X\* ≈ 25 on a different tip geometry (cylinder vs cone) where the failing section is unambiguous. Two plates, two geometries, same number — this is now a solid constant.

**(b) N arms share load AT LEAST ADDITIVELY. The zipper is dead.**
Every strip arm carries X_arm = 60.15, identical to `M1`'s X_tot, and `M1` died. The zipper model therefore made **12 point predictions and got 12 of 12 wrong** — 2TY, 2WY, 3TX, 3TY, 3WX, 3WX2, 3WY, 3WY2, 4TY, 4WX, 4WY, 6TY all ALIVE. Under the pre-registered null this is p ≈ 2⁻¹² ≈ 2.4 × 10⁻⁴. **Confidence: highest on the plate.** The headline (`3TY` vs `M3`, `4TY` vs `M4`) is technically a tie-with-both-alive because M3/M4 sit inside the boundary; the *discriminating* comparison is the strips vs `M1`, and it is unambiguous.

**(c) Possible small super-additive bonus. Confidence: low-moderate.**
`2TY`/`2WY` at X_tot = 30.08 ALIVE while `M2` at X_tot = 30.48 DEAD — a 1.3 % gap. Either X\* sits precisely inside that 1.3 % window (prior odds ≈ 3 %) or the two-arm group genuinely beats the equal-area single arm. Physically plausible: a line of arms has a group section modulus 40-100× a single arm of equal total area, so it eats the eccentric peel moment for free. **But the pair is not perfectly matched** (rectangular 8.333 × 3.600 cap vs a 6.189 disc), so I will not claim more than "≥ N×, possibly a little more". Do not design on the bonus.

**(d) Arm-line orientation to the peel front: NULL, with real but limited power.**
4 matched pairs (3TX/3TY, 3WX/3WY, 3WX2/3WY2, 4WX/4WY), 8 cells, all alive. Those cells sat at X_tot = 15.0-20.1 against X\* ≈ 25, so an orientation penalty larger than ~1.25-1.7× would have shown. **Conclusion: no orientation effect larger than ~1.5× in capacity.** Confidence: moderate.

**(e) Load magnitude — the repo's p_eff envelope is wrong by two to three orders of magnitude for flat caps.**
`predictions.py` corners (p_eff 0.4-3.0 kPa, σ_g 4.8-10 MPa) predict X\* = σ_g/p_eff = **1 600 to 25 000** — i.e. that *everything* survives, including `ZS`/`ZL` at X ≈ 400. Measured X\* ≈ 25. The self-consistent reading uses `params.py`'s own `SQZ_TERMS`: a flat disc's squeeze pressure is **cavitation-capped at p_cav = 101 kPa** (the quoted 12.395 N on a 12.50 mm cap is exactly 101 kPa; the 5.00 mm cap computes to 67 kPa). Then σ_g,effective = X\*·p = **2.0-3.1 MPa at the arm/flare notch** — a sane green-resin number. **So: for a flat contact patch ≳ 5 mm across, the release load is atmospheric-scale (~0.1 MPa), not kPa-scale.** Confidence: high on the *ratio* (that is what was measured); moderate on the p/σ split (that is a model choice).

**(f) Front-to-back peel severity gradient: bounded, not detected.**
B6 row0 = 5/6 alive (only the designed `PB` died), row1 = 6/6, row2 = 3/5 (both deaths designed). `M3` survives at X = 19.91 in the *front* row while `M2` dies at 30.48 in the *back* row ⇒ **row0/row2 severity ratio ≤ 1.53×.**

**(g) Exposure / LCD spatial non-uniformity in X: rejected.**
Alive and dead alternate over tens of mm inside single trains. Train 6: `M4` alive (x = +4.6), `M1` dead (+17.2), `L3` alive (+33.2), `S7` dead (+47.3), `M2` dead (+58.1). Train 1: five dead flanking one alive (`M3`). Geometry, not position, is the discriminator.

---

## 3. Models REJECTED, with the number that kills each

| Model | Killed by | Margin |
|---|---|---|
| **Zipper (X_arm governs)** | 12 strip cells alive at X_arm = 60.15; `M1` dead at 60.15 | 12/12 wrong |
| **Pure tensile ratio law as a *plate-wide* law** | `KS` dead at X = 8.00; `S3` (15.88) dead while `L3` (16.10) alive at matched ratio **and matched row** | inversion |
| **Pure bending / slenderness, demand ∝ A_cap·L/d³** | `KS` index 21.3 → DEAD; `M3` index 45.1 → ALIVE | 2.1× backwards |
| **Any tip-loaded cantilever law, demand ∝ A_cap·L^n/d^m** | For `KS` to out-rank `M3` needs n ≥ 2.26 (m=2) or n ≥ 2.85 (m=3). Tension gives n=0, bending n=1, tip deflection n=3/m=4 (which also inverts: 433 vs 520) | no mechanism supplies n ≥ 2.3 |
| **Mixed tension+bending, demand = X(1 + k·L/d)** | `M3` alive + `KS` dead simultaneously require **k < −0.39** — bending would have to *reduce* demand | absolute |
| **Absolute cap area / squeeze-film / any F ∝ A^p, p ≥ 1** | At matched X and matched row the **6.26× larger** cap survived (`L3` alive, `S3` dead). `params.py`'s own squeeze model puts cap-S at 67 kPa and cap-L at the 101 kPa cap, i.e. predicts S is 1.5× *safer* | ≥ 2.3× backwards |
| **Cap-plate bending as the A6 killer** | The 4:1 flare (0.20 mm/side per 0.050 mm layer) makes every cap a near-uniform-strength shallow cone, σ ≈ 3p(R−r)²/h(r)², size-insensitive. `2TY`'s cap cantilevers 4.17 mm off its arm line — worse than any A6-S geometry — and survived | inverted |
| **Row / peel-severity gradient as the A6 killer** | Would need ≥ 1.6× (tension) or ≥ 2.0× (bending) front-to-back to explain `L1`/`L3`, but B6 caps it at ≤ 1.53×; and it cannot touch `S3` vs `L3` (same row) or the 0/11 S wipeout (all three rows) | over-constrained |
| **Wash-basket / handling loss as the *systematic* A6 cause** | Handling force is size-independent, so it should kill the *weakest-in-bending* first. `M3` breaks at ≈ 0.16 N lateral, `KS` at ≈ 0.23 N; drag×lever is 30×4 = 120 for B6 vs 19.6×6 = 117 for A6-S — **identical exposure, and M3 is 30 % weaker.** M3 lived, KS died | inverted |
| **Exposure/LCD non-uniformity in X** | alive/dead alternate within trains over 55 mm | — |

That last row matters: **handling is rejected as the systematic explanation** by exactly the same argument that rejects bending. It survives only as the explanation for one-off inversions (`L1`).

---

## 4. The surviving model: one law + one artifact

**LAW (governs 17/17 of B6 and all of A6-L but `L1`).** Tensile failure at the arm/flare junction — the photos show bare, essentially full-length stalks with clean flat tops (`KS`, `M1`, `S7`, `M2`, `L1`, `L7`), i.e. separation at the top of the cylinder where the flare begins, not mid-arm bending snaps.

> demand = p_eff · A_cap / **Σ**A_arm  ·  capacity = σ_g · **X\* ≈ 25 (20-30)**
> Arms add. Orientation is irrelevant. Free length 4 → 6 mm is not a first-order term.

Zero free parameters beyond the threshold. Nothing in B6 needs anything else.

**ARTIFACT (governs A6-S, and `L1`).** A mechanism that removed caps independently of arm area, from the entire 4.997 mm-cap population at cap plane 10.950 (0/11, all three rows, all nine ladder rungs plus both controls). It is **z-band selective**: every well-behaved specimen on the plate has its cap at z ≤ 9.200; every anomalous loss has its cap at z ≥ 10.950. That is the one variable that partitions all 34 outcomes with no exceptions — but it is 100 % confounded with family, so this plate cannot promote it from correlation to cause.

Two candidates, and the data already discriminate:
1. **A late-print, high-z process event** (film/vat condition above z ≈ 9-11 mm; a cured slug on the FEP after the first cap let go; resin starvation over the largest flat areas — note the ballast terminates at `P6_BALLAST_TERMINATE_Z_MM` = 12.000 and `P6_BALLAST_TARGET_MM2` = 734.445 = 6 × 122.4075, so the final 14 layers cure *only* the six 12.5 mm discs). **Explains a systematic 0/11 with q ≈ 1.**
2. **Post-print handling/wash.** **Rejected as the systematic cause** (see table above: B6 offers equal drag×lever exposure on weaker arms and survived). Retained only for `L1`.

**I commit: ≈ 80/20 in favour of a print-process event above z ≈ 9-11 mm, not a handling loss.** The single most damaging consequence: this is a *printer/process* fact, and it will corrupt any future coupon that puts a scored feature above z ≈ 9 mm.

---

## 5. What the plate FAILED to measure, and why

1. **X\* at 6.00 mm free length.** A6 was the family that was supposed to deliver it at 1.25× resolution; 11 of its 17 specimens are uninterpretable and the family is void by its own gate. The 6.00 mm number survives only as A6-L's inadmissible [16.1, 24.85).
2. **Whether free length matters at all.** Family **C6 — the one family designed to vary `arm_l_free` — was dropped from this build**, and A6-vs-B6 differs in free length (6.00 vs 4.00) *and* cap plane (10.950/12.000 vs 8.500) *and* cap area simultaneously. Three-way confound, no control. A6-L accidentally supplies a one-rung length check and finds no effect, but it is not admissible. Dropping C6 is the direct cause of the plate's biggest interpretive hole.
3. **The exact location of X\*.** Bracketed only to 1.53× (19.91-30.48). B6's M-ladder spent its rungs bracketing, not resolving.
4. **Orientation, super-additivity, and pitch, at any resolution.** Every strip cell survived, so all three contrasts are ceiling-limited. 12 specimens bought one bit of information (additive ≫ zipper) and nothing else. Putting a few strips *above* the boundary (X_tot ≈ 35-60) would have bought the whole sharing curve for the same 12 cells.
5. **Anything from break height.** Confirmed uninformative as designed — the photos show full-length stubs everywhere (the one apparent exception, `S8` at ~2 mm, is a designed failure at X = 47.9 and may be parallax).

---

## 6. Rules a support-generation engine should adopt (plates 5 + 6)

1. **Sizing.** Total support-arm cross-section under one contact patch ≥ **A_contact / 12**. Boundary measured at X\* ≈ 25 twice (plate 5 cones, plate 6 cylinders), so X ≤ 12 is a clean 2× margin. Confidence: high.
2. **Sharing — compute X on the SUM.** N arms of area *a* under one patch are worth N·*a*. Verified to N = 6, pitches 2.0 and 2.8 mm, both axes, at X_arm = 60 (2.4× past the single-arm boundary). **No zipper, no orientation term, no per-arm check.** Corollary the engine should exploit: splitting one arm of diameter *d* into N arms of *d*/√N is capacity-neutral and strictly better for mark size and removal force. Confidence: very high.
3. **Peel orientation is not a design variable.** Do not spend geometry on aligning arm lines to ±Y. Bounded to < 1.5×. Confidence: moderate.
4. **Free length 4-6 mm is not a first-order variable.** The bending amplification coefficient in X_eff = X(1 + k·L/d) is bounded to **k ≤ 0.18** by B6 alone. Spend margin on total area, not on shortening arms. Confidence: moderate (single-family bound; C6 never printed).
5. **Load magnitude — replace the legacy pressure.** For a **flat** contact patch ≳ 5 mm across, budget **p_eff ≈ 100 kPa** (cavitation-capped), i.e. F ≈ 0.1 MPa × A_contact — 12 N under a 12.5 mm disc. The 0.4-3.0 kPa envelope in `predictions.py` is a *slender-wall* number (plate 1) and is 30-250× too low here; it predicts X\* ≈ 1 600-25 000 and would have passed `ZS` at X = 409. p_eff is geometry-dependent, exactly as `NOTES.md:655` suspected. Practical form for the engine: **just use X ≤ 12 and never compute a force.**
6. **Keep the 0.20 mm/side/layer flare.** It printed everywhere, and it makes the cap a near-uniform-strength cone. Second-order watch item, not measured here: at p ≈ 100 kPa that cone computes to σ ≈ 1-2 MPa on the largest caps, within ~2× of green strength — so if cap-side (rather than arm-side) failures ever appear, *halve* the flare rate (σ_cap scales as flare-slope²).
7. **Coupon-design rule, not an engine rule:** never score a feature above z ≈ 9 mm on this printer until item 5 below is closed, and never put a validity control in a different z band from the specimens it validates.

---

## 7. The single most important follow-up, and the cheapest way to get it

**#1 — free, do it today, before anything else: a vat-and-basket census.**
Strain the vat and inspect the wash basket, and count loose caps by size. The prediction is sharp and binary:
- **11 five-mm discs (and ~4 twelve-mm discs) recovered from the vat / cured on the FEP ⇒ in-print, high-z loss.** Also inspect the film for a cured slug or a cloudy patch, and confirm the resin level never dropped below the top of the print.
- **Loose caps found in the wash basket ⇒ handling loss**, and the pre-registered rider was correct.

This costs zero print time and decides which of the two surviving artifact hypotheses is real. Everything downstream — whether the printer has a z > 9 mm problem, or the coupon handling protocol needs fixing — hangs on it. While the basket is out, **photograph trains 3 and 5**: `3TY` and `KL` are currently unverified and they carry the headline and the sole surviving control.

**#2 — the one print worth making (≈ 2 h, one small plate, ~15 specimens).**
A single-row plate that de-confounds the three variables the dropped C6 left tangled, using B6's known-good geometry as the anchor:
- the A6-S ladder (X = 8, 12.6, 19.9, 24.6, 31.3) at **cap plane 8.500 with 4.00 mm arms** — if these survive to X ≈ 25, A6-S was a z-band artifact and X\* is confirmed cap-size-independent;
- the same five rungs at **cap plane 10.950 with 4.00 mm arms** — isolates height from free length;
- the same five rungs at **cap plane 8.500 with 6.00 mm arms** — this *is* the dropped C6, finally run against a valid control.

**#3 — cheap add-on to any future plate.** Three single-arm cells at X = 22 / 25 / 28 in triplicate on B6 geometry pins X\* from ±22 % to ±6 %, and three strip cells at X_tot = 35 / 45 / 60 finally give the sharing curve a ceiling to bump against instead of 12 cells all reporting "alive".
