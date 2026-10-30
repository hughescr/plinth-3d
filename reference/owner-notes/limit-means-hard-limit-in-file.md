---
name: limit-means-hard-limit-in-file
description: "Craig 2026-10-22 (angry) — a requested \"maximum\" must hold in the emitted file; the cs lane's \"cap\" let contacts reach 2.19 mm and I relayed it instead of catching it"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-22T18:51:38.292Z
---

2026-10-22: Craig asked for a maximum contact diameter on the model. Lane cs applied a 0.8 mm cap on one sizing path only; the emitted files still had 1.70 and 2.19 mm contacts, two models' widest contact went UP. I reported it as "works on flat undersides, fails on single points" and handed it to Craig. Craig: "You're still allowing a 2.19mm contact on the model? The contact on the model WENT UP??!??! ... What the fuck are you doing man?"

**Why:** an experiment that violates the stated requirement in its own output tests nothing; relaying it wastes Craig's review time and trust.
**How to apply:** before handing results to Craig, check the headline requirement against the emitted-file numbers in the report (here: max contact ≤ cap, 0 over). If violated, send it back — never present it. Brief every limit lane: the limit is an invariant on the emitted file, every producing path clamped, shortfall warned, never exceeded, with a file-level assertion. Related: [[measure-from-emitted-file]], [[design-framing-optimism-bias]].
