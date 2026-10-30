---
name: support-arm-load-physics
description: "Measured support-arm load rule: an arm snaps when supported area exceeds ~25x its own narrowest cross-section; design at 12"
metadata: 
  node_type: memory
  type: project
  originSessionId: c450934b-c7e3-49a5-9bcd-00cf2068505b
  modified: 2026-07-31T18:35:13.571Z
---

**The number the whole support generator was waiting on.** Measured from calibration
plate 5 (30 cap-on-tip specimens, Elegoo Saturn 4 Ultra, black resin, 30 C tank),
2026-07-30.

## The rule

Define **`X = A_supported / A_arm_cross_section`** (both mm^2).

- Every specimen with **X <= 25.3 survived**; every one with **X >= 25.0 failed**, one
  boundary inversion. Failures spanned X = 25 to 6229; survivors 0.64 to 25.3. Two
  independent rows agreed. **X\* ~ 25.**
- **Design target X <= 12** (2x safety factor).
- Load scales with supported **AREA** — not perimeter, not a squeeze-film law.
- Failure is a **tensile snap of the narrowest cross-section just above the contact**,
  NOT the contact patch. Craig observed breaks "at the base of the cylindrical portion
  where it meets the conical support".

## Consequences that are not obvious

- **Tip diameter matters enormously.** A 25x range in tip diameter decided survival at
  fixed cap area. Claude initially concluded "fine tips are free" from an incomplete
  read; Craig's detailed per-specimen read refuted it. Do not repeat that error.
- **A long slow taper is a liability** — the failing section is the narrowest point, so
  supports must flare fast above the contact.
- **Total scar area is invariant: `A_face / X`.** Capacity ~ d^2 so cell area ~ d^2 so
  count ~ 1/d^2 — the d^2 cancels. Thin-and-dense scars exactly as much as
  thick-and-sparse. At X = 12 that is **8.3% of the supported face area, always**. The
  only lever on scarring is X (i.e. the safety factor) and WHERE the scars land — not
  the support style. You also cannot cheat by necking the tip, because the neck then
  becomes the failing section.
- **Hexagonal/triangular lattice is the optimal placement**, ~23% fewer points than a
  square grid for the same worst-case gap. Voronoi is the analysis dual, not a rival
  pattern: **each arm's Voronoi cell area IS its supported area**, which gives per-arm
  sizing directly. Use Lloyd relaxation / CVT weighted by a scar-cost field for real
  curved surfaces.
- **X <= 12 is necessary but NOT sufficient.** It is blind to ARRANGEMENT. Three
  collinear arms give zero lateral stability perpendicular to the line (Craig spotted
  this on specimen PB). A hex lattice is inherently triangulated; a row of supports along
  a thin feature is not.

## Softer numbers

- **Max unsupported span ~3 mm at 0.300 mm section** (bridge/sag limit). Read at low
  photo resolution; treat as approximate.
- Squeeze-film suction **40-60 kPa** at 30-40 C interface temperature.
- Only the RATIO green-tensile/peel-pressure is measured (= 25). The individual Pa/MPa
  split is a modelling convenience. `plinth-3d` config now carries
  `tensile_green_mpa = 0.288` with `peel_pressure_pa = 3000`; the repo's own literature
  note (instrumented MSLA peel 12-13 kPa) independently implies ~0.33 MPa, same order.

## What this fixed in the code

`plinth-3d`'s support engine permitted **X = 417** — `tensile_green_mpa 10.0 /
(peel_pressure_pa 3000 x tip_safety_factor 8)`. That is ~17x too permissive even after
its own safety factor, 133x before it. On Skeleton_1, **16 of 66 tips were sized past the
measured snap point and the checker reported them all clean.** Now `tensile_green_mpa =
0.288`, giving X_allowed = 12.0, worst tip exactly on target, +24% support volume.

**Still unfixed (structural):** nothing in the engine adds contacts in response to a LOAD
violation — `engine._add_fix_contacts` reacts only to COVERAGE gaps, while
`verify.load_report` is ANDed into `verify.ok`. With `tip_r_max_mm = 1.0` capping one tip
at 37.7 mm^2, **any large flat down-face makes the load gate unsatisfiable by
construction.** Skeleton_1 escapes at 15.5 mm^2; a base, shield or cloak will not.

## Plate 6 results (scored 2026-07-31, wash basket empty = all failures in-print; adversarially verified: 2 blind analysts + refuter)

**Record of record: `plinth-3d/docs/reference/plate-06-results.md`** (full 34-row
specimen table + verification appendices; plate 5 salvage in `plate-05-results.md`;
engine-facing constants in `docs/rust-migration/03-physics-and-constants.md` §4).

**ZIPPER IS DEAD; sharing is AT LEAST additive.** All 12 strip cells survived at
X_arm = 60.15 — the exact load that killed the matched mono M1. N = 2..6, both pitches,
both axes, both replicates. Survival tracks X_tot = A_cap/(N x A_arm). Engine rule:
**size on the SUM of arm areas under a contact patch, X_tot <= 12**; splitting one arm
into N of d/sqrt(N) is capacity-neutral and better for scarring. This unblocks the
unsatisfiable load gate on flat down-faces. Caveats: one cap area (30 mm^2), one arm
d (0.798), one length (4 mm); no strip ever failed so there is NO upper bracket on
sharing; strips carry a ~20% fin-root section bonus (params B6_FIN_ROOT_SIDE_F) so do
NOT design on any super-additive margin.

**Mono cylinder boundary: X\* in (19.9, 30.5)** — M3 (19.9) lived in the FRONT row,
M2 (30.5) died. Geometric center 24.6; plate 5's ~25 reproduced on a second tip
geometry. Cone-artifact worry retired. Each bracket edge is n=1; X <= 12 keeps >=1.66x
margin to the worst edge.

**X is a phenomenological index, not a closed stress ratio.** Measured slender-wall
peel pressure (~1 kPa, plate 1) predicts X\* ~ thousands; observed ~25. Self-consistent
reading: flat contact patches >= ~5 mm see cavitation-capped squeeze ~100 kPa, giving
~2-3 MPa at the arm/flare notch. Engine posture: never compute a force — use the ratio.

**Family A6 is VOID by its own pre-registered gate** (KS, must-survive at X = 8.0,
died) — and A6-S was WIPED, 0/11 across X = 8..409, all rows: an outcome invariant
across 51x in load ratio is not a load-capacity outcome. Mechanism UNIDENTIFIED but
narrowed: not handling (basket empty; inversion argument — B6 offered weaker-in-bending
arms with equal exposure and lived), not pure bending/slenderness (all standard
cantilever laws miss by 2x+; A6-L is non-monotone: L1 d=3.95 died while L3 d=3.12
lived), not row, not cap area (bigger L caps did better at matched X). Leading
candidate: an in-print, family-confounded artifact — A6 caps sit at z 10.95-12.7 vs
B6's 8.5-9.0 (cap-plane z is 100% confounded with family). **Coupon rules: never score
a feature above z ~ 9 mm until this is understood; never put a validity control in a
different z band than the specimens it validates.** My slenderness hypothesis was
RETRACTED after the refuter's numbers; free length 4->6 mm is bounded to a <= ~18%
effect by B6 but was never cleanly measured (family C6 was dropped from the build).

**Explicit pre-registration override, recorded:** the KS gate as written voids the
whole plate. Ruled B6 admissible anyway because its own blind control (PB) behaved,
its 17/17 outcomes fit one threshold with zero anomalies, and the destruction
signature is entirely inside A6 — an argued override, not a silent re-scope.

**Peel-axis orientation: untested at the boundary** (every strip lived — zero
variance), but axis-dependent zipper collapse at X_arm = 60 is excluded. Not a design
variable until contradicted.

Craig's predictions went 4/6; both misses (KS, L1) are inside the void family.

**Follow-ups if ever needed:** vat/FEP census of the 17 lost caps (dates the A6-S
event); photograph trains 3 and 5 (3TY and KL rest on naked-eye reads); a future
plate wants C6-style length rungs, M-ladder replicates x3, strips ABOVE X_tot = 30,
and a second B6 cap area (this plate cannot distinguish the ratio law from
"total arm area >= 1 mm^2" — plate 5's cap-size spread is what carries ratio-form).

## Caveat on plate 5's own data

All 37 of plate 5's ballast frustum junctions land exactly in a slicing plane, 12 with
locally constant section — the mechanism that dropped a 2-layer ballast band on plate 6.
So plate 5's per-layer cured area was plausibly **less flat than its design assumed**.
Plates 1-4 are unaffected (junctions land on layer boundaries). Separately,
`verification.json`'s area profile is an ANALYTIC model, not the mesh — it overstates one
band by up to 119 mm^2, so the ballast solver has been flattening a declared profile
rather than the real one on every plate.

See [[coupon-plate-design-rules]], [[y-sensitivity-settled]],
[[saturn4-tilt-peel-geometry]], [[resin-rust-rewrite-decision]], [[plinth-3d-cli-vision]].
