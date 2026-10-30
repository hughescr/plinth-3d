---
name: crevice-contact-acceptable-ruling
description: "Craig's 2026-08-25 ruling: a model-seated support arm in a narrow crevice MAY touch the opposite wall — 'not ideal, but better than no support at all.' The connected-surface forgiveness class is ACCEPTED BEHAVIOR, not a defect. Ends the footprint-rule search; Rule Z rejected by measurement; authored-labels fix NOT pursued."
metadata: 
  node_type: memory
  type: project
  originSessionId: 5769930f-658e-4eba-a883-d959c7bc2ab6
  modified: 2026-08-25T15:02:44.651Z
---

Craig's ruling, 2026-08-25 (verbatim, via AskUserQuestion free-text answer): "It's fine
for a support arm in a narrow crevice to touch the opposite wall in those circumstances
— it's not ideal, but it's better than no support at all."

Context and consequences:
- The seat-zone exemption's connected-surface forgiveness (any model surface
  mesh-connected to a seated arm's attachment patch within the band cannot refuse the
  arm at the final 0.300mm check — measured 0→20 wall faces at 1.00mm on the
  raised-column fixture) is hereby ACCEPTED behavior, not a defect to fix. Per-arm
  disclosure stays.
- This ENDS the footprint-rule search: the receiver-partition candidate was refuted
  (needs labels unlabelled STL cannot supply), and Rule Z (buried-nearest-boundary) was
  REJECTED BY MEASUREMENT 2026-08-25 (self-refuses its own seat in 104/108 sweep cells
  worst −1.54mm, still forgives the deep wall, removed every refusal on Deinonychus;
  root cause: burial cannot distinguish own receiver from foreign feature on an
  unlabelled mesh; evidence .git/rule-z-2026-08-25/ + .git/rule-z-experiment-2026-08-25/;
  the experiment branch and worktree were DELETED at Craig's instruction 2026-08-25 —
  the .git evidence dirs are the only surviving record).
- The authored-receiver-labels future fix is NOT pursued (Craig's acceptance makes it
  unnecessary); the design memo seat-footprint-2026-08-23.md stands as the record of
  why no current-input rule can do better.
- #77 (re-size the exemption patch after thickening, feature/f1-refreeze @ 855a1b8,
  fully adjudicated ACCEPT) MERGES into feature/a4-controller per this ruling.

Related: [[two-tier-clearance-ruling]], [[supports-may-seat-on-model]],
[[land-at-strike-rulings]], [[plinth-restart-2026-08-23]].
