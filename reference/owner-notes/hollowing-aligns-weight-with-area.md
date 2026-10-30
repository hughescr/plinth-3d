---
name: hollowing-aligns-weight-with-area
description: "Craig 2026-08-29 — hollowing turns a model's mass from ~x^3 into ~n*x^2, so the measured area law stays valid on big models; weight only matters for thin arms or unusually heavy parts"
metadata: 
  node_type: memory
  type: project
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-08-29T20:18:38.214Z
---

Craig's reconciling insight, 2026-08-29, when asked whether the support load law should
be volume-based rather than area-based:

> "The weight is probably relatively unimportant unless the support is especially thin or
> the weight is especially heavy; but hollowing should make the weight be much more
> proportional to area for those more-substantial models where the difference between x^2
> and x^3 is more important — hollowing effectively reduces the weight to n*x^2 (assuming
> roughly constant wall thickness)."

**Why it matters.** The support area law (`pi*r^2 >= carried_area / X`, X_ALLOWED = 12) is
measured, not assumed: COUPON-PLATE-5 ran 72 specimens over a 24-cell grid whose
discriminant spans 24,000x, and area-pressure beat line-tension and squeeze-film outright.
But every specimen was a flat CAP, whose area and volume are proportional — so the plate
could not distinguish peel force from weight, and the law was never tested on a chunky
solid where the two diverge.

Craig's point closes most of that gap by construction rather than by experiment: a hollowed
shell of roughly constant wall thickness has mass proportional to its SURFACE AREA times
the wall, not to its bounding volume. So on exactly the models where x^2 vs x^3 would have
mattered most — Large+ creatures, which are the ones that get hollowed — weight scales the
same way peel force does, and one area law serves both.

**The residual, stated so it is not forgotten:** the law is still untested where weight
dominates and does not track area — a solid (unhollowed) heavy part, or an especially thin
arm. Those are the two cases Craig named as exceptions, and they remain unmeasured.

Also from the same exchange: this is resin-specific. The dominant force is suction at layer
peel, acting over AREA. A deposition printer would need a different model, and the project
does not implement one.

See [[support-arm-load-physics]] for the measured constants,
[[hollowing-is-for-large-minis]] for which models get hollowed at all, and
[[physics-first-constants-doctrine]] for why a measured basis is required.

---

## Refinement, 2026-08-29 (same day, later): hollowing DOES cut peel force — drainage is the discriminant

Craig, responding to the load-model design's ruling that "a hollowed model is charged by its
filled outline":

> "the peel forces for the first layers above the support will not change; but as we get
> deeper into the model, they WILL change as once we reach the hollow, the void is not
> causing peel forces; only the model walls are. So if you have a U-shaped model ... the peel
> forces at the bottom of the U will be the same, but once we reach the `| |` on either side,
> the peel forces are dramatically reduced since it's just `| |` and not `---`"

**Two cases, and the design only gets one of them right.**

1. *Open gap* (the `| |` arms of a U, separated by air open to the tank). Already correct by
   construction: the slice is two separate closed loops, filled-outline charging fills each
   loop, and the gap is outside every loop. Through-solid routing then sends each arm's load
   down its own arm. Costs nothing extra.
2. *Enclosed cavity* (the hollow inside each wall). Here filled-outline charging bills the
   void, and this is where Craig's objection bites.

**The measurement, stated correctly.** The one direct in-repo record is holes punched in a
layer at constant outer outline: **2.68x less cured area gave 1.26x less force** — sublinear,
but NOT the null result it was summarized as. And punched holes are the worst possible
drainage case: high flow resistance means liquid resin cannot feed the hole during the lift,
so the film still suctions across it.

**So Craig and the measurement are not in conflict** — they sit at opposite ends of an
unmeasured drainage axis. A hollowed mini has a large cavity with drain holes (the treant
drains through the feet); resin feeds freely, no trapped pocket, force should track cured
wall area. The discriminant is whether the void drains fast enough to keep up with the lift,
NOT solid-vs-ring.

**Consequence for the test plan:** coupon C2' as specified ("solid vs ring at fixed outline")
would likely reproduce the null result and MISS the effect. It must vary void width and
drainage, so it measures WHERE the credit turns on rather than whether it exists at one
unfavourable geometry.

**Why max-over-layers is the right sizing rule** (Craig supplied the mechanism the design
lacked): peel is per-layer and does not accumulate; weight DOES accumulate; hollowing is what
stops the accumulating term ever catching up. Residual, named honestly: the design has no
weight term at all, so a large SOLID model that skips hollowing has nothing that would notice.
That wants a named refusal, not a silent assumption.
