---
name: y-sensitivity-settled
description: "Front-to-back plate position is worth <=1 rung — stop spending towers measuring it, use Y as a randomization block instead"
metadata: 
  node_type: memory
  type: project
  originSessionId: 0010a938-3a51-4e43-8d1c-2db9590b4417
  modified: 2026-07-30T05:19:27.345Z
---

Settled 2026-07-29 from plate 4. **Stop designing experiments to measure Y-sensitivity.**

The tilt-geometry model (first moment of area about the tank hinge — see [[saturn4-tilt-peel-geometry]]) predicted a 2.1x-5.7x front-to-back release force. Under `L* ∝ p^-1/2` that is a **1.9 to 4.4 rung** boundary shift, which plate 4 had the power to see. It found nothing, and bounds the real effect at **<= 1 rung (~1.5x in p)**.

So the first-moment reasoning **over-predicts substantially** — the film is compliant and redistributes load, and progressive release means the instantaneous local force is far flatter than the geometry suggests. Craig's read ("some difference, but not a huge factor") is correct.

Also settled: the rear station is **flat in X** — 10 identical towers over 135 mm at Y +45 gave identical verdicts, killing the D5 local-panel-defect hypothesis. This is the one question plate 4 was well-built for and it is answered cleanly. What plate 4 can **never** answer is the rear-station *level*, because the X-replication row differs from every other row in five ways at once (10 towers vs 3, group b only, ascending only, 148 mm vs 43 mm of X, 1 ballast column vs 3-4). Do not re-analyse it hoping for a level; it needs a G-triplet at Y +45 plus an X-style row at mid-plate, ~13 towers.

**How to apply:**
- **Use Y as a randomization block, not a measurement.** Scatter every factor across Y stations. Putting a whole ladder at one Y confounds that ladder with a real ~1-rung effect. Costs zero towers and protects every other measurement.
- **Keep the `g(X, Y)` position hook in the oracle, defaulted to 1.** Don't model it; don't hard-code its absence.
- **Spend freed replication on build-to-build variance instead.** It is unmeasured and every error bar quoted so far silently assumes it is zero.

  Craig's rig is better controlled than a bare printer: **heated tank held at 30 °C** inside a hooded enclosure, in a dry modern-HVAC apartment. Resin viscosity runs ~2x per 10 °C and drives the squeeze-film/suction term, so tank heating removes the largest environmental driver. Humidity is **not** worth chasing — free-radical acrylates are not very water-sensitive during cure; no sensor needed. What remains uncontrolled is **resin batch (sets E_green and σ_z,green) and LCD panel drift** — small, but aimed at exactly the constants we are trying to measure.

  **Film age is NOT a variable and can be dropped from every error budget.** Chitubox reports the film's rating as **60,000 releases**; at plate 5 the counter read **770** — 1.3% of rated life. `D_film` is a constant across dozens of plates. Do not re-raise film age as an uncontrolled term.

  **Machine/material facts, settled 2026-07-29.** Film is stock Elegoo **PFA, 0.127 mm**, 2 prints old — the same thickness the analysis assumed for FEP, and PFA's modulus is within ~20% of FEP's, so since n* ∝ D_film^(1/3) the crossover stays at **n* ≈ 2.5–2.9 laminae**. The displacement-controlled regime is real and the "deflects out of the load path" escape mechanism stands. Resin: black, 2 L sealed desiccated jug, constant across the campaign. Craig's resin observations: clear cures similarly but ends stiffer/more brittle; gray is the most pliant with slightly longer cure; black and gray are close. **Constants are per-resin — this is why the oracle must store (E_green, D_film, G_film, G_th, m) and never a fitted scalar p_eff.**

  **Interface temperature is 30–40 °C**, not the 30 °C tank setpoint, because cure is exothermic and the rise scales with that layer's exposure. Two consequences: (a) the squeeze-film suction estimate drops from ~97 kPa at µ = 0.3 Pa·s to roughly **40–60 kPa** (viscosity ~2x per 10 °C) — still dominant for a large flat island, but quote a band; (b) **exotherm tracks cured area per layer, so uneven area between rows produces a thermal gradient as well as a mechanical one.** Plate 4's X row carried 2.56–4.06x the G rows' area and was therefore probably also running warmer — a mechanism that was not on any list. Equalising per-layer cured area by ballast equalises the thermal state too; state that as a second reason for the rule.
  - Read the **E-family check targets across builds** as the environmental gate. They integrate everything that actually reaches the part, and beat instrumenting the room.
  - **A cross-build bound already exists, unread:** plate 4's rungs 1-7 (0.7982-2.5920 mm) are exactly plate 1's rungs 9-15 at the same four finger heights — designed in deliberately. Bounds (not measures — pad size, row density and ballast differ between family A and A2) build-to-build today, from parts in hand.
  - **Make the lift-speed campaign four prints: 1x, 0.5x, 2x, 1x.** The repeated 1x bracketing the campaign gives build-to-build variance *and* campaign drift (film wear, resin depletion) for one extra print and no CAD work — strictly better than a standalone repeat.

See [[coupon-plate-design-rules]].
