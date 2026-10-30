---
name: sweep-field-feedback-2026-09-05
description: "Craig's ChituBox verdicts 2026-09-05 on the overnight files: sweep tips make POINT CONTACT (ball pole sits exactly on the model's lowest lamina plane, zero seating, contact area << arm section — a defect at every scale); drain keep-out works and un-clustered the Sphere's pole; trunk merging must land ANYWHERE on another arm's shaft (segments that split the landed arm), not only on its top"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-09-05T22:10:33.355Z
---

Craig in ChituBox, 2026-09-05, on `drain-and-cluster/`, `merge-and-shed/`:

1. **Tip contact is a point.** "The spherical tip of the support does not seem
   to penetrate the model AT ALL... basically just a point contact... true at
   every scale." Contact area ≪ arm cross-section, so the joint is weaker than
   the arm — "doesn't make any sense". CAUSE (verified in code): the sweep
   adapter sets seat = plate_z + k_top·h (the bottom plane of the first cured
   model lamina) and `lower_sweep_arms` puts the ball's TOP POLE at the seat
   (`centre = seat + r_ball·unit`, unit pointing down the arm). Zero seating
   depth. The legacy seating law (physics.md §9: h = max(0.100, resolution +
   r·sinθ, ball_seat_factor(0.382)·r_tip), guaranteeing cut-disc radius a ≥
   r_tip) was never carried into the sweep adapter, which calls itself
   "deliberately thin frame arithmetic". Fix = apply that law along the arm
   axis into the model; truncate rather than exit thin walls. See
   [[contact-area-over-seat-depth]].
2. **Drain keep-out works** and, on the Sphere, "dramatically eliminated the
   clustering" of thick overlapping base arms. Cylinder still clusters.
3. **Trunk merging, corrected model:** the one Morel merge landed ON THE TOP of
   the other arm. Craig wants tree-branch structures: a base arm rises, SPLITS,
   heads to multiple tips. A descending arm's base must be able to land
   ANYWHERE on another arm's shaft (splitting it into above/below segments),
   each segment carrying its own load. The algorithm for optimising bases /
   splits / number of ultimate bases is open ("I don't know what kind of
   algorithm"). The lane's "merge target = topmost arm's TOP in a column"
   restriction (2dce1a6 deviation) is therefore WRONG and is likely why merges
   almost never fire: a gentle lean to a shaft far below is cheap under the
   angle law, a lean to a neighbour's top is not.

**How to apply:** seat-depth fix in the sweep adapter first (it makes every
emitted file physically wrong); then shaft landings (ColumnHistory must hold
per-column shaft intervals, not just the top); the "merge benefit term"
question I raised is probably moot once shafts are landable.

Related: [[sweep-field-feedback-2026-09-03]], [[land-at-strike-rulings]],
[[plinth-state-2026-09-04]].
