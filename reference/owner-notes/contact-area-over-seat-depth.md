---
name: contact-area-over-seat-depth
description: "Craig 2026-08-30 ruling — burial depth is not adhesion; only rasterisation gaps matter, so depth caps at 0.1mm, but rasterised CONTACT AREA still governs and never yields to depth"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-09-06T04:45:00.763Z
---

Craig ruled on 2026-08-30 that support tip **burial depth into the model is not an
adhesion requirement**. His words: *"if the tip is TOUCHING the model (within the
constraints of the printer resolution) then it's as good as 'buried'... cured voxels
are cured the same if they overlap or not."* Support and model cure in the same layer
exposure — there is no bond to develop between two solids. The only real failure is a
**rasterisation gap** opening between them, bounded by one pixel and one layer. Hence
`default_contact_depth` = **0.100 mm**, down from 0.300.

**Why:** the old 0.300 mm default was 3–6× what physics asks, and over-burial cuts a
bigger scar and makes support removal harder — two of Craig's own complaints about the
generated files. `docs/physics.md` §9 already had the right formula,
`h = max(default, resolution + r·sinθ, ball_seat_factor·r_tip)`; the oversized default
simply won every time, so the physical terms never bound.

**The constraint that travels with it, and must never be dropped:** Craig's follow-up
warning. The emitted tip is a **whole ball** (`sphere_triangles` emits every latitude,
no clipping plane) and seating pushes it inward, so a shallower seat is a **retracted**
ball, not a trimmed one, and the cut disc shrinks: `a = sqrt(2·f·r·h − h²)`. So:

- **Contact area is the end; depth is only a means.** The delivered contact patch —
  measured *after* rasterisation, not from ideal geometry — must satisfy the area law
  against the load that contact carries. Where it would fall short at 0.100 mm, the tip
  grows or the seat stays deep. **The reduced depth is a permission, never an override.**
- The formula's third term (`ball_seat_factor(1.5) = 0.38196601125`, derived not typed)
  already guarantees `a ≥ r_tip`, so contact area cannot fall below the load law's
  demand — but that guard must be verified as firing, not assumed.
- **Truncation is the change Craig actually wants**: keep the seating the load law
  demands and clip only the part of the ball that would exit a thin wall. Same contact
  area, no protrusion through thin sections. Open question is whether the `.chitubox`
  format re-renders a tip as a whole sphere regardless of the emitted triangles.

**Restated by Craig 2026-09-05** (after the sweep tip-seating fix 26eaab7):
"'contact depth' 0.1 mm max is a FAKE number... once they overlap with the
model, they're just the same voxels. We don't need to actually generate
geometry beyond the surface. The important thing is not 'penetration depth',
which is always effectively 0 mm; the important thing is contact surface
area. We should 'push' the support tip into the model as deeply as it needs
to go in order to reach the contact surface area that's needed, but then
truncate the geometry beyond the model's surface so that the tip cannot
protrude on the other side of any thin model layer." → (1) the depth minimum
constant should not decide anything: depth = whatever the area term needs
(+ at most a rasterisation allowance derived from pixel + layer, not typed);
(2) thin walls: push to the FULL needed depth and CLIP the ball beyond the far
face — never RETRACT it (26eaab7 retracts, which is why its below-guarantee
counts are nonzero on thin skins). The open question whether ChituBox
re-renders a tip as a whole sphere must be settled empirically (emit a
clipped ball, let Craig look).

**How to apply:** never present a depth reduction without the contact-area contract in
the same breath. Any future re-derivation that treats depth as the safety property has
lost the ruling. See [[print-candidate-metric]] and [[tip-size-field-data]] (the same
UNITS TRAP applies: this registry speaks radius, Craig and ChituBox speak diameter).
