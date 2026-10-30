---
name: measure-from-emitted-file
description: "2026-10-07: v19 reported a 0.96 mm neck from planned parameters; Craig saw the drawn file had none (cone flush to the model). Every geometry number reported to Craig must be measured from the emitted .chitubox"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-07T22:57:44.196Z
---

v19's WHAT-CHANGED reported "visible neck median 0.96 mm". Craig looked in ChituBox: the
cylindrical neck was gone and some cones sat practically flush on the model — "I think this
part went wrong". The number came from planned head parameters, not from the drawing.

**Why:** planned values and drawn geometry diverge after later passes (frustum drawing, trims,
resizing); a number from the plan describes intent, not the file Craig opens.

**How to apply:** in every lane, review and milestone brief, demand that geometry numbers
(neck lengths, clearances, radii, penetrations) are MEASURED FROM THE EMITTED FILE, and let
reviewers reject evidence taken from planned parameters. Related: [[verify-against-the-real-consumer]],
[[stop-over-verifying]].
