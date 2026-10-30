---
name: hollowing-is-for-large-minis
description: "Hollowing targets Large-size and bigger D&D creatures; medium 30mm minis are correctly refused — thin walls there cause support problems, and the resin saved isn't worth chasing"
metadata: 
  node_type: memory
  type: project
  originSessionId: 5769930f-658e-4eba-a883-d959c7bc2ab6
  modified: 2026-08-13T15:48:09.019Z
---

Craig, 2026-08-13, ruling on why 32 of the 37-mini corpus refuse to hollow: **that is correct
behaviour, not a defect.** Medium-size D&D minis are ~30 mm tall — there is not enough material inside
to be worth removing, and hollowing them would leave walls thin enough to cause support issues.

Hollowing is for minis representing **Large-size creatures and bigger**. That is where the resin
savings live.

**Why:** several rounds of FU-82 work treated "almost nothing hollows" as a symptom to investigate.
It is the design working. Reading a high refusal rate on a corpus of small minis as a bug wastes
effort and risks someone loosening the bulk gate to "fix" it — which would produce fragile prints.

**The corollary, and it is the sharp end** (Craig, same day): *"The one thing that does matter is that
the large minis, which should be hollowed, should actually be hollowed. We can't gate those."* He named
Giant Toad ("effectively a sphere with some texture on the outside"), the Weeping Treant, and the
Warhorse as models that must hollow. **A Large-size mini that ships solid is a DEFECT.** As of
2026-08-13 both Giant Toad (drain-planning cap, needed 9,116,309,310 bytes against 1,073,741,824 — 8.5x
over) and the Treant (ships solid) fail. That is the live bug.

"We can't gate those" is not permission to loosen a gate — see FU-8b. An 8.5x cap overage on a
roughly spherical model is an algorithmic/resource defect to fix at the charge, not a ceiling to raise.

**How to apply:**
- When measuring hollowing savings or coverage, judge it on the large models. A corpus-wide hollow
  rate over mixed sizes is a meaningless denominator.
- A refusal at the bulk/thickness gate on a **small** mini is an expected outcome to report, not a
  finding to chase. A refusal on a **large** one is a bug until proven otherwise.
- Never relax the bulk floor to raise the hollow rate — see [[plinth-git-flow-convention]]'s sibling
  discipline, FU-8b: fix the charge, not the cap. Here there is no charge to fix.
- Related: [[resin-print-pipeline-goals]] (savings multiply across plates) — the multiplier comes from
  the big models, not from squeezing the small ones.
