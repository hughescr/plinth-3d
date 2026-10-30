---
name: land-at-strike-rulings
description: "Craig's four land-at-strike rulings (2026-08-22): rule ratified WITH load-sized anchors (not fixed 0.3mm); permanence accepted as disclosed; sub-0.3mm crevices moot (weld adds no resin — land + disclose); deep seats allowed ANY depth but TRUNCATED at the opposite face (never exit, no backside scar) — replaces the E-C remove-or-build choice."
metadata: 
  node_type: memory
  type: project
  originSessionId: 5769930f-658e-4eba-a883-d959c7bc2ab6
  modified: 2026-08-22T18:14:57.004Z
---

Craig's rulings on the land-at-strike design (docs/engineering-journal/designs/
land-at-strike-2026-08-22.md @ 10b0bd8), 2026-08-22, all verbatim-anchored:

1. **Land-at-strike ratified, with LOAD-SIZED anchors.** "If these struts are all under
   2.5mm inside crevices then they're likely extremely thin and don't need a full 0.3mm to
   land in… as long as the physics says they'll be supported they can have much smaller
   anchors." → the foot radius AND embed depth derive from the carried load (area law /
   attestation), and the wall test becomes "fits the foot THIS load requires" — not the
   fixed DEFAULT_CONTACT_DEPTH_MM. Recovery rises above the 80/208; island 74 must be
   re-graded honestly (its 13mm² load still sizes a big foot — likely stays re-aim).
2. **Permanence accepted as disclosed** (5 struts / 0.005mL / 10 scars, 3 via the KAPPA
   branch — per-strut disclosure stands, no gate).
3. **Sub-0.3mm crevices: moot.** "If resin seals anyways then the strut adds no extra
   resin." Land them, disclose each; no width refusal.
4. **DEEP SEATS — the truncation ruling (supersedes the E-C remove-vs-build question):**
   "A seat should be allowed to go as deep into the model as it likes, but it should not
   come out the other side… if the seat wants to go deeper than the thickness of the model,
   it should get truncated at the opposite face… it should not create a scar on the
   backside because that provides no additional support." → SPEC-EMBED-POLICY's no-depth-cap
   sentence is REWRITTEN (not removed): depth unlimited, truncated at the far face; a seat/
   foot (ball included) never exits; punch-through impossible BY CONSTRUCTION (computable
   from the penetration intervals already measured); anchor capacity of the truncated embed
   graded against the load per ruling 1. E-C's hazard dissolves; the old S6 regression case
   becomes "truncated, never perforates".

Composition: 1+4 together = every anchor is load-sized and material-bounded. Related:
[[two-tier-clearance-ruling]] (0.6 place / 0.3 final + frozen-Z honest exemption),
[[supports-may-seat-on-model]] (three-seat ruling), [[tip-size-field-data]].
