---
name: round-bases-first-goal
description: "Craig 2026-09-02 goal ruling — get the flat 'Round <x>' base models (Round 25/50/100) working before ANYTHING else; failing on a flat disc means a basic flaw in the algorithm, and no other work is worth doing until it is found and fixed"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-09-02T16:37:20.319Z
---

Craig, 2026-09-02, after Round 50 ground for 30 min and Round 100 refused
under the Part 1 overhang-weight build:

> "Figure out why Round 50 and Round 100 are failing. That's simply insane.
> They're flat basic shapes, easily supportable. If you can't support those,
> forget even wasting time trying anything else because there's a very basic
> flaw in the algorithm."
>
> "Goal set: Get the 'Round <x>' models working before anything else."

**Why:** a flat disc with a recess is the simplest possible support job (lift,
flat or mild tilt, seats on the underside at spacing, nothing on the rim or
the display face). An engine that grinds, refuses, or inverts it is wrong at a
level below any tuning, and every result on complex minis is suspect until the
disc works. It is also the diagnostic he can judge instantly in ChituBox.

**How to apply:**
- Round 25/50/100 emitting correctly (recess UP, underside supported, no bare
  islands, seconds not minutes, no refusal) is the gate for everything else:
  corpus runs, support-thickness work, ratchets, T6, Part 2 reach.
- Treat any disc failure as a FLAW OF PRINCIPLE first (wrong quantity modelled,
  cost tied to the wrong thing, demand-blind stage), not a constant to tune.
- Diagnosis workflow wf_366f06c2-3ee (2026-09-02) → report at
  `../plinth-evidence/disc-diagnosis-2026-09-02/DISC-DIAGNOSIS.md`.
- Add Round 25/50/100 to the quality ratchet once they pass, so they never
  regress silently again.

Related: [[round-base-diagnosis-2026-09-02]], [[emit-and-warn-never-refuse]],
[[physics-first-constants-doctrine]], [[plinth-state-2026-09-02-evening]].
