---
name: limits-are-construction-rules-2026-10-28
description: "Craig 2026-10-28 (furious): every hard limit must be a construction rule at the step that creates the geometry, never a late check-and-repair pass; contact size and spacing were retrofitted for weeks"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-29T04:59:54.377Z
---

After the contact-budget transaction (CBT) step 0 found 0 of 88 fixes feasible, I explained that plinth sizes tips from load with no width limit, then tries to impose the 0.8 mm contact limit and the no-fusion rule late, across 10+ passes that move and grow tips. ChituBox builds to fixed tip size and spacing, so its output is correct by construction. Craig: "you DIDN'T make the rules limits ALL ALONG but only apply some limits, iteratively, LATE in the process. motherfucking jesus christ."

My own 2026-09-30 rule "radii stay load-sized by physics" (see [[rulings-2026-09-30-oversupport]]) started this; the 0.8 mm cap ruling ([[limit-means-hard-limit-in-file]], [[contact-cap-includes-feet-2026-10-23]]) was then bolted on instead of moving it into placement. 8 pcap rounds plus CBT were wasted on retrofit.

**Why:** a limit enforced after the layout is set leaves no room to satisfy it; each later pass must be taught the limit, and every repair knocks on to neighbours.
**How to apply:**
- When a new hard limit arrives, first ask: which step CREATES this quantity? Put the limit there as an input. Later steps may only stay inside it.
- Tip sizing rule (Craig, restated angrily, "what I've been saying"): load physics sets tip size, but ONLY UP TO THE LIMIT. size = smallest that carries its load, never above the cap. If a tip can be smaller, make it smaller. If it is at the cap and more support is needed, ADD MORE SUPPORTS that do not touch each other. NOT a fixed tip size.
- Never answer a hard limit with a late shrink/repair loop or a transaction over existing passes; if a design proposes that, reject it.
- Tell Craig early when a limit is being checked rather than built in.
