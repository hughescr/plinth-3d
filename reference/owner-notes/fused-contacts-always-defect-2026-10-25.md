---
name: fused-contacts-always-defect-2026-10-25
description: "Craig 2026-10-25 — any two supports whose model contact patches merge is a defect, even if the merged patch is under 0.8 mm; marks must stay separate for tool access"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-25T04:29:33.868Z
---

Asked whether a merged contact patch under 0.8 mm counts as a defect (12 of the 17 FU patches on Deinonychus are 0.17–0.64 mm), Craig chose "Any merge is a defect" over "under 0.8 mm is fine" and "needs a minimum gap between marks". His reason: two supports should always leave separate marks, so tool access between them is kept.

**Why:** cleanup needs a tool between marks; one blob from two supports is harder to separate and clean than two marks.
**How to apply:**
- All FU patches count, whatever their size.
- Planned joins (host plus lander) are not excused when their model contacts fuse.
- Fixes, in this order: move a tip apart, or replace two redundant contacts with one load-capable contact (within the 0.8 mm cap).
- No minimum gap beyond non-fusion is ruled. Don't invent one; the 0.05 mm reader link distance is resolution, not a gap rule.

Related: [[plate-packing-fine-model-end-access]], [[contact-cap-includes-feet-2026-10-23]], [[limit-means-hard-limit-in-file]].
