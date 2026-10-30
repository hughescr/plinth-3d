---
name: rulings-2026-09-05-merging-detail
description: "Craig 2026-09-05 rulings: plate contact area is an acceptable merge-benefit term; segmented supports landing anywhere on a shaft should cut resin; sub-tip-thickness fragility rule DEFERRED; drain clearance is fine as built (leaning arms inside the hollow will handle it); detail = WIDE DISTRIBUTION of normal changes (small+medium+large), uniform-size change is just 'change'; try the basic greedy tree before optimising"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-09-05T22:15:52.392Z
---

Craig's rulings, 2026-09-05, answering the decision list:

1. **Merge benefit term:** "plate contact area is a good benefit term to
   potentially use." He expects segmented supports (a base landing ANYWHERE on
   a shaft, splitting it) to reduce total resin by themselves. Order of work:
   shaft landings under the existing objective first, measure; add the
   plate-contact-area benefit if merges still do not fire.
2. **Feature thinner than a support tip:** DEFERRED. "Wouldn't worry about
   that for now; move on." Do not build the fragility-saturation rule until he
   raises it.
3. **Drain clearance:** "good right now." With leaning arms allowed INSIDE the
   hollow he does not expect the enclosed-drain problem to persist. So: no
   warn-only switch; leave the keep-out as built; the gallery becomes a
   lean-around keep-out once shaft landings exist.
4. **Detail field:** to be folded in eventually. Refinement: it is not just
   "many small changes" — detail is a WIDE DISTRIBUTION of normal-change
   magnitudes (some small, some medium, some large) within the neighbourhood.
   If all changes are the same size it is "change", not "detail" (a crease, a
   regular ridge). So the statistic is the spread (variance / entropy) of the
   local normal-change distribution, not its mean. Not now.
5. **Tree optimality:** "try the basic approach and see how it looks before
   complicating it" — greedy bottom-up with shaft landings, no global tree
   optimisation.

**How to apply:** queue = tip seating fix (running) → shaft landings on the
Cylinder/Sphere fixtures → measure resin (support volume) and plate contacts
before/after → plate-contact benefit only if needed. Do NOT touch: sub-tip
fragility, detail-distribution statistic, drain warn-only.

Related: [[sweep-field-feedback-2026-09-05]], [[plinth-state-2026-09-04]].
