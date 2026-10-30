---
name: contact-cap-includes-feet-2026-10-23
description: "Craig 2026-10-23 — the 0.8 mm contact limit applies to every mark on the model, including feet of supports that stand ON the model, not only tips"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-23T19:59:21.037Z
---

Asked whether model-landed feet (a support's bottom end resting on a lower part of the model) fall under the 0.8 mm contact cap, Craig chose "Yes, feet too" over "tips only" (my recommendation) and over a separate foot limit.

**Why:** every mark on the model is a scar to clean up; it doesn't matter which end of the support made it.
Craig added: "If necessary, feet can be split too on those model-model contacts to distribute the load at the base to reduce scarring and improve separability from the model".

**How to apply:** the final audit counts feet on the model against 0.8 mm (warnings first). Fixes, in any order that the physics allows: split a foot into several small feet (each ≤ 0.8 mm, spaced for tool access) that share the load; give it a thin neck at the foot; or re-land it on the plate. Related: [[limit-means-hard-limit-in-file]], [[checks-default-on-opt-out-2026-10-23]].
