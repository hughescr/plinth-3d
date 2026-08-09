> **Appendix to [`plate-06-results.md`](plate-06-results.md) — protocol / validity memo for calibration plate 6.** Preserved unmodified below this header.
> Produced 2026-07-31 by an adversarial-verification panel (two blind analysts plus a refuter) reading the plate-6 scoring; header added on filing, nothing else changed.

# PLATE 6 — PROTOCOL MEMO (blind read, protocol-analyst lens)

Sources read directly: `/Users/craig/code/hughescr/dnd-campaigns/calibration-coupons/PLATE-LAYOUT.md` (§plate 6, L1258+), `/Users/craig/code/hughescr/dnd-campaigns/calibration-coupons/NOTES.md` §13, `/Users/craig/code/hughescr/dnd-campaigns/calibration-coupons/verification.json`, `/Users/craig/code/hughescr/dnd-campaigns/calibration-coupons/manifest.json`, `/Users/craig/code/hughescr/dnd-campaigns/calibration-coupons/predictions.py`, and all three photos.

**Build provenance is clean.** `md5 COUPON-PLATE-6-support-arm-capacity.chitubox` = `2fff011d1cfee56ac0b49b3e759e5a60` — matches the printed record and `verification.json`. All 33 `design_identities` gates report `ok`. C6 is genuinely absent (17+17=34). NOTES.md §13 is **stale** (says 5 rows, A6=18, B6=19, C6 present, md5 `4efdc345…`); PLATE-LAYOUT.md + manifest.json are authoritative. The geometry that was printed is the geometry that was designed.

---

## 1. Validity ruling, family by family

### Family A6 — **VOID. Mechanically required, no discretion.**

The pre-registration: *"`KS`/`KL` (X ≈ 8) MUST SURVIVE — if either is gone, printing or handling destroyed the plate and nothing else on it can be trusted."*

`KS` is DEAD. Photo `train4.jpg` position 3 confirms it: a bare stub, no disc, ID `KS` legible on the apron. The gate fired. **A6 measures nothing. Every A6 reading — the nine-rung S ladder, the four-rung L ladder, the ratio-vs-absolute-size test — is discarded.**

Read the clause literally, as written: the consequent is *"the plate"* was destroyed and *"nothing else **on it**"* is trustworthy. **The gate is plate-wide, not family-scoped.** Under the pre-registration as written, B6 is voided too (see below).

Three independent confirmations that the void is real and not a technicality:

1. **KS cannot have failed by peel.** Using the repo's own numbers (`predictions.py`, plate-1 bound `p_eff ≤ 1.24 kPa`): F_peel = 1.24 kPa × 19.5554 mm² = **24.3 mN**; arm stress = 24.3 mN / 2.452 mm² = **0.0099 MPa** against σ_g = 4.8–10 MPa. Safety factor **485–1010×**. Nothing about the print killed KS.
2. **The L ladder is non-monotone.** `L1` (X=10.01) DEAD, `L3` (X=16.10) ALIVE. A capacity ladder in which the *easiest* rung dies and a harder rung lives is not a ladder. `L1` needs σ_g·πd³/(32·l) = **495 g of lateral force** at the cap to snap a 3.952 mm arm. That is a dropped-part event, not a wash current and not a peel.
3. **Outcome tracks cap size and row, not X.** All 11 cap-S specimens dead (11/11, including the control at X=8). Cap-L 2/6 alive — and **both survivors are in row 2**; every L specimen in rows 0 and 1 died regardless of X. Cap-S and cap-L at matched X are *not* scale models (arm free length is fixed at 6.00 mm for both), so at matched X the small cap is **15.7× more fragile in bending** (32l/πd³: KS 11.1 vs KL 0.70 per unit force). The design's own handling rider predicted exactly this: S9's lateral break force is **2.2 g**, KS's is **44 g**.

**The pre-registered procedure was also not completed.** The rider says: *"Check the wash basket for detached caps before concluding a count."* A count has been concluded and no basket inspection is reported. Formally, the A6 reading is unfinished as well as void.

**Root cause is a documented contradiction between the two protocol files.** NOTES.md §13: *"hand-lift every plate-6 specimen off the build plate individually and score it there **before it goes near the wash basket**"*. PLATE-LAYOUT.md scoring sheet: *"Score **after** washing, drying and curing — never on the still-wet green part."* The owner followed the newer file. The plate's own build-status notes flagged sub-10.5 mm pads and thin-arm fragility as unfixed risks; the scoring sheet then routed the plate through the exact hazard.

### Family B6 — **VOID under the literal gate; and it has no control that could have caught this.**

- `PB` MUST FAIL → failed. Behaved.
- No partial caps observed; photos show clean present/gone. The VOID-on-partial rule was not triggered.
- **B6 has no MUST-SURVIVE control.** Both of the plate's must-survive controls live in A6, and one died. `PB` is a *must-fail* control — it tests the wrong direction. A must-fail control firing is unfalsifiable evidence about a mechanism whose signature is *false failures*.

So B6 satisfies its own two rules and fails the plate-level rule. Strictly: **discard.**

**Two further gate problems the strict read must record:**

4. `verification.json` → `row_vs_factor_correlations`: `b6_N_vs_row = -0.4619`, `b6_pitch_vs_row = +0.3578`, `a6_logX_vs_row = -0.2425`, `b6_axis_vs_row = -0.2209` — *"4 of 6 exceed the plate's own |r| ≤ 0.2 '(factor × position)' gate … flagged, not silently passed."* It is recorded `ok: true`. **A gate whose stated threshold is exceeded is a FAIL.** N is confounded with row at r = −0.46, and row demonstrably matters: row 2 holds 2/2 of A6's survivors *and* 2/3 of B6's non-control deaths. Rows 0+1: A6 0/12 alive, B6 11/12 alive. Row 2: A6 2/5 alive, B6 3/5 alive. Row 2 is anomalous **in both directions at once** — no single mechanism produces that.
5. **The failure mode that killed KS also predicts B6's death pattern.** Lateral break force for the M ladder at l=4.00 mm: M1 **6.1 g**, M2 **16.9 g**, M3 **32 g**, M4 **49 g** — monotone, and it splits dead/alive between M2 and M3 exactly where X does. And a strip of N arms on a 2.0–2.8 mm pitch is one to two orders of magnitude stiffer in bending about the line axis than one equivalent arm. **Handling fragility and peel capacity are perfectly collinear on this plate.** Nothing in B6 distinguishes them.

---

## 2. What the plate MEASURED

**Under the pre-registration: nothing. The measurement is void and must be reported as void.**

Below is off-protocol reasoning, explicitly labelled. It rests on one asymmetry: **the implicated failure mode (post-print cap loss) manufactures false DEATHS, never false SURVIVALS.** ALIVE readings are therefore conservative-safe; DEAD readings are not.

| Claim | Value | Confidence |
|---|---|---|
| The literal zipper prediction is refuted | 13/13 strip cells ALIVE at X_arm = 60.15, the value at which the matched single arm M1 died | **HIGH as stated; MEDIUM that it is about peel** |
| Single-arm survival floor, 30 mm² cap, 4.00 mm arm | **X* > 19.91** (M3 alive, in row 0 — a "bad" row) | **HIGH** |
| Single-arm ceiling | X* < 30.48 (M2 dead) → **X* ∈ (19.9, 30.5)**, geometric centre **24.6** | **MEDIUM** — rests entirely on deaths |
| Agreement with plate 5's X* ≈ 25 | 24.6 vs 25 | **MEDIUM**, and it is not independent confirmation — same possible artefact |
| Multi-arm cells survive to at least | X_tot = **30.08** (2TY, 2WY), and X_arm = 60.15 at every N from 2 to 6 | **HIGH** |
| Proven load-sharing credit | worst-arm fraction f < X*_max/60.15 = **0.507** → **≥ 1.97× benefit**, at every N | **MEDIUM** (inherits M2's death) |
| A6 large-cap survivals | KL alive at X = 7.97; **L3 alive at X = 16.10** on an unambiguous 3.116 mm cylinder | **HIGH** (survivals only) |

Note the one genuinely awkward pair: **2TY/2WY at X_tot = 30.08 ALIVE vs M2 at X_tot = 30.48 DEAD** — same cap area, same total arm area, N=2 vs N=1. Under strict additivity these should behave identically. Either M2's death is an artefact, or splitting beats area. On this plate you cannot tell, and the strip's ~50× bending advantage makes the artefact reading the more parsimonious one.

**A separate arithmetic problem the plate does not touch:** for X* ≈ 25 to be a *tensile* boundary you need p_eff = σ_g/25 ≈ **192–400 kPa**, i.e. **155–320× the plate-1 upper bound of 1.24 kPa**. X = A_cap/A_arm is therefore a phenomenological index, not a stress ratio that closes against the programme's own measured peel pressure. Plate 6 was designed to sharpen X without ever checking whether X is the right variable, and it explicitly throws away the break height that might have told you.

---

## 3. What the plate FAILED to measure, and why

1. **X\* on constant-diameter cylinders — the plate's stated primary purpose (a).** A6 is void. Zero rungs survive scoring. **This family answers nothing.**
2. **"Boundary is the ratio, not the absolute size."** The S/L design existed solely to prove this. 11/11 S dead against KL alive at the *same* X refutes it — but under a mechanism that is not peel. So the claim is neither confirmed nor refuted: **untested.**
3. **Peel-front orientation (X vs Y arm line).** 3TX/3TY, 3WX/3WY, 3WX2/3WY2, 4WX/4WY — **all eight ALIVE.** Total ceiling effect. Zero discrimination. **This contrast answers nothing.**
4. **Pitch (T=2.0 vs W=2.8).** Same: every cell alive. **Nothing.**
5. **The degree of load sharing.** Every N ≥ 3 strip cell sits at X_tot ≤ 20.05 — *below* the single-arm boundary. They would survive even at zero sharing credit beyond ~2×. The N ladder was placed entirely inside the safe region and can only kill the zipper, never measure the exponent. **The ladder was mis-sited.**
6. **The headline itself, 3TY vs M3.** The pre-registration: *"If sharing is additive the strip survives where the single arm does not; if it is a zipper they fail together."* Both survived. Neither branch. **The headline contrast is uninformative** — it presupposed M3 would fail, and X* landed above M3. The N=4 "insurance rung" (4TY vs M4) collapsed the same way. **Both headline pairs answer nothing.**
7. **Arm free length.** C6 was dropped. A6 (6.00 mm) vs B6 (4.00 mm) is perfectly confounded with family, cap size, cap height (11.45 vs 9.0 mm) and cap shape. The generator's whip-deflection rule — which its own config calls *"a ROUGH ESTIMATE, order-of-magnitude guess"* — remains **completely uncalibrated.**
8. **Row/train effects.** Confirmed present, unmodelled, and confounded with N at r = −0.46 in violation of the plate's own gate.

---

## 4. Rules a support-generation engine should adopt (plate 5 + plate 6)

**Adopt:**

1. **Size arms by X = A_supported / A_min-section, target X ≤ 12.** Unchanged. Plate 6's only firm survival evidence (M3 at 19.91) leaves a **1.66× margin** on the target; the best estimate of X* (24.6) leaves 2.05×. No reason to loosen the target and no evidence to tighten it.
2. **Never exceed X = 20 on a single arm.** Firmly demonstrated survival stops at 19.91; the next rung up died. Treat 20 as a hard cap, not a soft one.
3. **Multiple arms share load — do not size per-arm.** The zipper model as written (one arm carries the whole cap) is dead: 13/13 strips survived at X_arm = 60.15 where the matched single arm died. Any generator computing per-arm capacity as A_cap/A_arm is over-building by at least 2×.
4. **Credit the sharing at 2×, not N×.** A_eff = min(N, 2) · A_arm; i.e. require A_cap / (min(N,2)·A_arm) ≤ 12. The plate proves ≥ 1.97× and cannot prove more, because no multi-arm cell was ever placed above X_tot ≈ 30. Using full N× credit is an extrapolation this plate does not license.
5. **No orientation penalty.** Do not spend geometry aligning arm lines to the peel axis. This is a **null with a ceiling**, so state it as "no detectable effect at X_tot ≤ 30", not "proven irrelevant."
6. **Prefer constant-diameter arms over cones when capacity must be predictable** — plate 5's ±2.5× ambiguity came entirely from not knowing which section failed.
7. **Freeze the whip/length rule.** Do not tune it on plate 6 data. It has none.
8. **Add a post-print handling check, separate from the peel check.** This is plate 6's most valuable real deliverable, and it is a process finding, not a capacity finding: F_lateral,break = σ_g·π·d³/(32·l). At σ_g = 4.8 MPa that is **2.2 g for a 0.646 mm arm at 6 mm free length, 6.1 g for 0.798 mm at 4 mm, 44 g for 1.767 mm at 6 mm.** Supports that pass the peel check by a 500× margin still lose their payload in a wash basket. Shorten free lengths and fatten arms near any surface whose loss is unacceptable — and wash gently.

**Do NOT adopt from this plate:** any cap-size scaling law, any pitch rule, any orientation rule, any length rule, or a revised numeric X* claimed as independent confirmation of plate 5.

---

## 5. Single most important follow-up

**Reprint the identical file and take two readings: on the build plate before wash, and again after wash+dry+cure.**

- Cost: **one print, zero CAD, zero re-slice.** `COUPON-PLATE-6-support-arm-capacity.chitubox`, md5 `2fff011d1cfee56ac0b49b3e759e5a60`, is already on disk and already printed once — 254 specimen layers.
- Procedure: drain and drip the plate, score every specimen *in situ* on the build plate under bright light (present/gone, same binary), record; then hand-lift trains, wash, dry, cure, and score again.
- Why this and not a new design: it recovers all 17 A6 specimens including the KS/KL gate, re-tests M1/M2 (the only two readings carrying the upper bound on X*), and **the difference between the two readings is a direct per-geometry measurement of the wash-loss rate** — which is the confound that currently makes handling fragility and peel capacity indistinguishable. One print converts every MEDIUM in §2 to HIGH and un-voids the plate's primary purpose.
- The protocol change is a one-line documentation fix: PLATE-LAYOUT.md's scoring sheet must adopt NOTES.md §13's pre-wash reading. The two files currently contradict each other, and that contradiction is the proximate cause of this void.

**Do first, free, and required by the pre-registration before any count is concluded:** inspect the wash basket, the basket mesh and the vat filter for detached caps. Count and photograph any loose D 4.997 discs. If S caps are in the basket, the handling diagnosis is confirmed outright rather than inferred.

**Second follow-up, only after the reprint:** re-site the B6 strip ladder above the single-arm boundary — N = 3/4/6 cells at X_tot ≈ 30/45/60 (thinner arms or a larger cap) — so the family can actually measure the sharing exponent instead of merely refuting the zipper. As built, every N ≥ 3 cell was inside the safe region and could not have said anything.
