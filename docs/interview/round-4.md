# Owner interview — round 4 (2026-10-29)

**Corrections to round 3 notes:**
- Not "snips can reach every contact": sometimes he uses the hook tool or has to get creative to free a support from an extra-thin part. Requirement: contacts near fragile parts must be separable without torque on the part, by some tool he has (snips, hook, pointed pick).
- The treant taking ~5 min to auto-support in ChituBox is UNACCEPTABLE, and a large part of why the project exists: ChituBox is basically incapable of handling models like the Awakened Tree, which he very much wants to print for his game.

1. **Priority order when goals conflict:** (a) it prints; (b) the model is usable after removing supports; (c) the whole process does not take longer than doing it in ChituBox would have; (d) looks; (e) resin use not excessive.
2. **0.8 mm limit — why:** all of: hard to separate, damages detail in ways that show after painting, hard to remove. Thinner parts should probably get stricter limits — though thin parts rarely need 0.8 mm of contact for the physics anyway.
3. **Support style:** his "light" vs "better" choice exists only because ChituBox has no physics model (it can't tell when peel force will rip a model off its supports), so he guesses from model size. A real physics model should replace that choice — no style preset needed.
4. **Hints:** one option is enough: "this model is pre-oriented: keep up upward, do not change orientation". Sensitive models get pre-oriented by hand and marked. Automatic detection of display faces would be great but is more than v1 should take on (no vision model for now).
5. **ChituBox format code:** the old reader/writer MAY be adopted — cautiously, reviewing its choices. Craig will generate new ChituBox sample files on request (e.g. several separate models, several plates in one file). Wanted: a complete understanding, documentation, and read-write library for the `.chitubox` format. Note: the old code survives only in the break-glass archive (git bundle); restoring that one component is an explicit owner exception to "no old code".
6. **Reporting/authority:** trial and error. He wants to give high-level instructions and have the work proceed. The orchestrator self-polices: come to him when needed; if coming too often, scale back; if drifting too far before a course-correction, ask sooner.
