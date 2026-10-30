---
name: tip-size-field-data
description: "Craig's measured Chitubox practice (2026-08-19): 0.1mm DIAMETER cylindrical tips work for tiny islands if the thin run is short; heaviest ever used is 0.8mm diameter sphere. Ruled: min tip 0.1mm dia (radius 0.05), max stays 2.0mm dia (radius 1.0). UNITS TRAP: Chitubox UI speaks diameter, plinth code speaks radius"
metadata: 
  node_type: memory
  type: project
  originSessionId: 5769930f-658e-4eba-a883-d959c7bc2ab6
  modified: 2026-08-20T11:18:54.683Z
---

Craig, 2026-08-19: "in chitubox I use 0.1mm diameter cylindrical tip for tiny support elements
(as long as the 0.1mm isn't a long run which would itself fail) which can be helpful for tiny
islands in narrow gaps... my heaviest support in chitubox uses 0.8mm diameter sphere."

**Ruling (Craig, same day, after the units were corrected):** lower `tip_radius_default_mm`
(the min clamp AND the default) from 0.2 to **0.05 mm radius** (0.1 mm diameter); keep
`tip_radius_max_legacy_mm` at **1.0 mm radius** (2.0 mm diameter) — the max is a permission
that enables consolidation, not a target.

**Why it matters:**
- The measured crowded contacts each carry <1.5 mm² *because* the old fat minimum forced them
  to — thinner tips on tiny areas = smaller scars + less resin exactly where the overkill lives.
- His "not a long run" caveat is the member-sizing law's job (arm below the tip sized
  separately), not the tip clamp's.

**Second ruling (Craig, 2026-08-20): scar bound at 0.8 mm DIAMETER (0.4 mm radius)** — a
rescue-grown tip (grow-to-attest) may not exceed his measured field ceiling; beyond it the
contact refuses with a disclosed, located ABANDONED LOAD line. Registered as an honestly-labelled
external requirement (field data), not physics. He also accepted the resin price of rescue
(+6.3% corpus, +15.5% Deinonychus before the bound trims it) — honest carrying beats silent
abandonment; expected to shrink when overhang demand work lands.

**UNITS TRAP (how to apply):** Chitubox UI and Craig's speech use DIAMETERS; plinth code
(`FrozenTipLoadPolicy` in perception.rs, min_radius_mm/max_radius_mm) uses RADII. Always state
both when discussing tip sizes with Craig, and never transcribe his numbers into the registry
without the ×2 check. The old clamp [0.2, 1.0] radius = [0.4, 2.0] diameter — his original "lower
to 0.1" instruction assumed the min was comparable to 0.1; it was 4× fatter.

Related: [[support-arm-load-physics]], [[physics-first-constants-doctrine]].
