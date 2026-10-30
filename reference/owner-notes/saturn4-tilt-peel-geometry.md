---
name: saturn4-tilt-peel-geometry
description: "Saturn 4 Ultra peels by tilting — front (-Y in Chitubox) travels furthest, layers release front-to-back"
metadata: 
  node_type: memory
  type: project
  originSessionId: 0010a938-3a51-4e43-8d1c-2db9590b4417
  modified: 2026-10-13T05:32:50.291Z
---

Craig, 2026-07-28: the Saturn 4 Ultra uses a **tilt-release** system rather than
a straight vertical lift. The **front of the plate — negative Y in Chitubox —
moves furthest**; the rear moves least. So each layer **releases progressively
from front (-Y) to back (+Y)** as the tank tilts away from it.

**Why it matters:** peel is the dominant load in resin printing, and under tilt
it is not pressure x area. Release force is governed by the first moment of area
about the hinge, so **a part's Y position on the plate changes its release
load** — plausibly by ~2.5x front-to-back (PHYSICS-REMAINING B8 estimates the
range at 2.1x–5.7x depending on where the hinge truly is). Every paper in the
project's literature set assumes straight lift, so none of them model this.

**How to apply:** (1) Fragile geometry belongs at the **back**, near the hinge —
this is the basis for future multi-model plate packing. (2) Coupon calibration
plates must replicate at front/centre/rear **and** with beams both parallel and
perpendicular to the hinge axis, or the position factor cannot be fitted.
(3) The layer-physics oracle needs a peel-front sweep with a
distance-from-hinge scalar profile, not a uniform per-layer pressure.

**Craig 2026-10-12: "suction cup is not a huge problem for our tilt-release printer."** A closed cavity or flat face toward the plate is therefore no reason to skip hollowing or to change the pose. It deserves a small warning at most.

Two tank-height measurements (front and rear edge, at rest and fully tilted)
would collapse the hinge-location uncertainty to a single number — five minutes
of work that only Craig can do.

Related: [[resin-rust-rewrite-decision]]
