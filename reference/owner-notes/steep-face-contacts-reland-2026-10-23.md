---
name: steep-face-contacts-reland-2026-10-23
description: Craig 2026-10-23 — contacts meeting a steep face at a grazing angle are badly placed; re-land them rather than thicken or neck them
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-23T18:37:31.846Z
---

Context: lane cs necked steep-face arms to keep the real scar ≤0.8 mm; 16 of 32 Deinonychus necks were thinner than their load needs (worst arm 80, 83.8° approach, 6.1 mm² on a 0.05 mm neck). Asked "scar vs snap", Craig answered: "Steep faces seem like they'd need LESS support, since by definition the steepness is going to cause the next layer to just not be there on at least one side, so there's less peel area? That makes me think likely those supports are possibly not placed well, and that the best option would be to try and re-land them rather than thickening them, where the thickening's effectiveness is essentially reduced because the support is only provided on the opposite side from where the peeling force happens -- it's more or less the worst possible physics situation for providing support, geometrically?"

**Why:** a grazing contact pushes on the wrong side of the peel; big load on a steep-face contact means the demand was assigned to the wrong place.
**How to apply:** don't neck or thicken grazing contacts; re-land them on a seat facing the peel (down-facing area the demand belongs to), and check how demand got assigned to a steep face. Related: [[limit-means-hard-limit-in-file]], [[physics-first-constants-doctrine]], [[contact-area-over-seat-depth]].
