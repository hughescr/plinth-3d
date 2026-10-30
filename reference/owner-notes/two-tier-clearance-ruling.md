---
name: two-tier-clearance-ruling
description: "Craig's 2026-08-22 ruling: clearance goes two-tier — 0.6mm demanded at PLACEMENT (search margin, steers toward headroom), 0.3mm true-gap hard limit AFTER brace thickening (the only refusing number). Provenance: Craig field experience, 0.3mm gap features print safely. Dissolves the Giant Bat crash (arms at 0.439/0.569mm become legal)."
metadata: 
  node_type: memory
  type: project
  originSessionId: 5769930f-658e-4eba-a883-d959c7bc2ab6
  modified: 2026-08-22T16:33:59.889Z
---

Craig's ruling, 2026-08-22 (explicit, via AskUserQuestion): adopt the two-tier clearance rule
on plinth-3d.

- **Placement (candidacy/tournament/rewire grading): 0.6 mm demanded gap.** Role: search
  margin — prefers arms with headroom so later load-driven thickening has room to spend.
  Doctrine label: prunes search, does not decide outcomes. (The old single 0.6 had EMPTY
  provenance — "legacy"; this relabel is its first honest job description.)
- **Final (brace exact pass, at the load-assigned radius): 0.3 mm true-gap hard limit.** The
  only number that refuses. Provenance: **Craig field experience — 0.3 mm gap features are
  preserved fine in prints** (integrates cure bleed + peel sweep + everything, measured by the
  real printer). He noted it could probably go lower; 0.3 is the safe pin. No cure-bleed data
  exists anywhere in the project (confirmed). DEFERRED instrument (Craig 2026-08-22: "I don't
  think we do unless we need to reduce the 0.3mm"): a gap-ladder coupon —
  0.30/0.25/0.20/0.15/0.10mm rungs × orientations {horizontal, vertical, 45°} × plate placement
  {front-to-back, side-to-side} — because the peel sweep is directional (front travels furthest,
  [[saturn4-tilt-peel-geometry]]) and layer orientation changes how a gap closes. Shelve next to
  the #66 angle-coupon plate; print only on his authorization.
- Both checkpoints must keep SHARING one predicate ([[defects-climb-the-stack]]); only the
  demanded gap differs by stage, both registry-read.

**BUNDLED (Craig, same day):** lands together with the HONEST EXEMPTION from the mask audit
(.git/mask-honesty-evidence/): exempt a frozen TARGET (seat patch Z = connected component of
the surface within r_seat+standoff of the axis, containing the seat face, frozen at contact
creation), never a skip-length; full-axis testing; monotonicity theorem. The audit's smoking
gun: the old skip-length excused an arm physically penetrating the model by 0.242mm.
CORRECTED expectations (audit-measured; earlier claims WRONG): the Bat's arms grade against
TRUE gaps ~0.291mm and <0.439mm → likely STILL honestly refused at 0.3 (its crevice island
needs land-at-strike); zombie-3-a's 89 wasted trials move ZERO under two-tier (52 = stub
re-entry class → land-at-strike's prize; 37 = placement-time at the kept 0.6). 0.6's history:
chosen against a v0 CENTERLINE test (≈0.6−r true gap); the kernel later measured true swept
surface, silently ~doubling strictness — 0.3 restores original intent. Tank-edge peel
measurement (front/rear, rest/tilted — collapses the 2.1–5.7× hinge uncertainty): Craig said
NOT NOW; per-arm peel formula g ≥ k·F·L³/(E·r⁴) parked as the named successor to any scalar.
Precedent note: retired P8 concession ladder said fattened arms keep full 0.6 — two-tier
knowingly inverts it. Lane wf_39a0ee85-3cd running.

Still standing after the ruling: (1) the seal-contradiction design
(seal-semantics-2026-08-22.md) now covers only the rare below-0.3 thickening residue — lower
urgency, memo re-scope needed; (2) Craig's same-day mask finding — the skip-length exemption
makes legality NON-MONOTONE in radius (thinner same-axis arm must never grade worse) — a
correctness defect regardless of buffer values; honest fix = target-specific exemption
(own-seat contact zone only), audit lane owns the derivation.

Related: [[physics-first-constants-doctrine]], [[tip-size-field-data]] (same provenance class),
[[plinth-pause-2026-08-21]].
