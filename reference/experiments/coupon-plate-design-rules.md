---
name: coupon-plate-design-rules
description: "Standing design rules for calibration coupon plates — contiguous row bases, no factor assigned by position formula, mandatory controls"
metadata: 
  node_type: memory
  type: project
  originSessionId: 0010a938-3a51-4e43-8d1c-2db9590b4417
  modified: 2026-07-29T18:35:59.827Z
---

Accumulated design rules for the resin calibration coupon plates, from Craig's directives plus what the plate-4 post-mortem proved. Apply these to every future plate.

## Layout

**Merge same-row specimens onto a single contiguous base plate** (Craig, 2026-07-29). Rationale is experimental, not just ergonomic:
- Releases from the build plate more easily.
- Bigger/heavier parts bang around less in the wash basket — reduces the wash/cure/handling damage channel, which plate 4 could **not** exclude as a failure cause and which is L-dependent like every mode under test.
- One rigid row photographs in one frame at one angle. Plate 4's X-row `z = 8` split (INTACT x5 / DROOPED x5) fell exactly on a boundary between two photos scored by two readers — a pure artifact. The **reader effect is the largest term in the error budget** (±0.8 mm, non-overlapping CIs, and *not* sampling error), so photographic repeatability buys down the dominant uncertainty.

**Ballast columns stay on separate individual bases** for resin reduction. (Craig said "tension-control towers" — read as ballast, the things that flatten per-layer cured area and hence peel tension. Confirm if he meant the E-family check targets instead.)

Merging bases changes the cured-area profile at low z, so **ballast must be re-solved** — but fix the `field.area_at()` discrepancy first (`verification.json -> critical_layer_area.A` 653.1-708.9 mm² vs `area_profile` flat 587.6 mm² over the same bands, same field, 0.025 mm apart) or the re-solve inherits the bug. No effect on the measurement itself: base changes sit at z 0-2 mm, every critical layer is at z 6-12 mm.

## Experiment design — the rule plate 4 broke

**Let no factor be a deterministic function of position.** Every alias that crippled plate 4 traces to a formula: `A2_GROUPS` welded rung length to z, `asc = ((si+gi) % 2 == 0)` made orientation identical to the station x group interaction, and groups a/b/c occupied disjoint X windows so any X gradient reads as a length effect. **The fix is a counterbalanced table, not a formula** — Latin-square the rung↔z map, orientation, X column and row parity independently.

**Keep row population and cured area equal across rows by ballast design.** Plate 4's X row carried 2.56-4.06x the G rows' area and is unusable for the question it was built for.

**Y-station is a randomization block, not a measurement** — see [[y-sensitivity-settled]].

## Mandatory controls

1. **Positive controls** — shelves deliberately short by known tick counts, scattered, blind to the scorer. Without them a null result has no demonstrated sensitivity.
2. **Ungraduated twins** at matched lengths. Tick emboss is 0.15 mm minor / 0.30 mm major on a 0.05 mm lamina — local bending stiffness x64 to x343 over ~20% of shelf length, so the emboss may manufacture the "top lamina stays straight" morphology.
3. **A printed scale bar on every coupon.** Non-negotiable — tick-pitch calibration caught a ~20% px/mm error from mast-width calibration that would have produced a false TORN verdict and flipped the mechanical conclusion.
4. **Ruler on the failing face** (model-bottom or side wall), not just the model-top face.
5. **A green handling control** — photograph one row wet, in machine orientation, before wash and cure. Only way to split print-time failure from post-process accumulation.
6. **Put emboss height/width in the manifest.** They were absent and blocked a conclusion.

## Labels

Pad-top emboss is **dead** — 0 of 25 readable across seven photos. Wall IDs on the mast's -Y face are marginal (4/25 fully legible) and readable only as confirmation of an ID you already know. **Put the discriminating character first, not last** — on the X row the leading "X0" read fine and the distinguishing final digit did not. Light the labelled wall directly; several scorers said it sat in shadow, so this may be a photography problem before it is a design one. Place label ink outside every critical slab.

See [[resin-print-pipeline-goals]] and [[workflow-granularity]].
