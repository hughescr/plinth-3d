---
name: thin-rod-brace-ruling-2026-10-22
description: Craig 2026-10-22 — a long thin rod standing right beside thick columns should simply be braced to them; a 0.05 mm tip must not block the brace
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-22T21:23:14.863Z
---

Craig 2026-10-22, on for-craig-nb picture 2 (lying treant arm 699: 28 mm free rod, 0.05 mm tip, thick post 2.1 mm away; lane nb left it unfixed because the brace rule refused a brace on a 0.05 mm member): "the red one in the treant looks like it ought to be simple to brace it to the other columns that are immediately next to it". He also accepted review folders with pictures only, no .chitubox files.

**Why:** the thin part that matters is only the contact tip; the shaft below it can be as fat as the brace needs.
The blocker is rule R2 (Craig 2026-10-03, interim: a brace is never thicker than the thinner post it joins, reach ≤ S × that post's radius; came from the deleted "brace-junction raise" that drew a fat post with a sudden thin top). R2's shoulder concern is valid AT THE JOINT; the defect is computing reach from the 0.05 mm tip radius and never thickening the shaft first. Craig questioned the rule 2026-10-22 ("Where did that rule even come from?").
Craig then (2026-10-22): "The rules should be based on physics - if bracing is going to cause physical problems then it shouldn't be allowed, or other adjustments need to be made to make it physics-compliant. But arbitrary rules like 'just never do something' seem overly strict". So R2 as a blanket cap is retired. It is replaced by: the brace's reach from its own slenderness; at the joint, no section jump between layers beyond the lean-law step (taper to fit); the tied run passing run shake. Fail → adjust (thicken, taper, move the joint), else warn.
**How to apply:** keep the tip thin (contact cap 0.8 mm still binds), thicken the shaft below the tip-down length, and tie it to the neighbouring column(s). Never let tip radius block bracing of the shaft. Witness on models Craig prints (upright treant, Giant_Bat), since the lying treant is dropped. Related: [[limit-means-hard-limit-in-file]], [[departure-rule-2026-10-21]].
