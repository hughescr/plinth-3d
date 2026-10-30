---
name: treant-hollowing-is-a-regression
description: "The Weeping Treant and Warhorse USED to hollow — body plus both legs, drains through both feet, vent on the head. Something changed and broke it. Do not accept 'the gate is correct' as the answer."
metadata: 
  node_type: memory
  type: project
  originSessionId: 5769930f-658e-4eba-a883-d959c7bc2ab6
  modified: 2026-08-13T16:35:27.121Z
---

Craig, 2026-08-13: *"The treant did used to hollow. I have no idea why it's not hollowing now nor when
it stopped. The original hollowing encompassed most of the body and ran down both legs to drains
through the base of the feet. The air hole inlet was I believe on the side or back of the head. The
warhorse also hollowed. At some point the hollowing algorithm or constants changed somehow and broke
it."*

**This is a REGRESSION, not correct behaviour.** A diagnosis that concludes "the savings gate is
measuring correctly, the treant genuinely has too little inside" is measuring today's cavity and
missing the point — today's cavity is wrong.

Measured on develop @ 7f8e4dd: treant cavity 1,556 mm³ = 6.3% of a 24,568 mm³ model, against a 25%
floor. Raw cavity 25,123 voxels, of which **48.8% is discarded** by the single-largest-component flow
rule (trunk kept, limbs thrown away). Keeping every chamber still only gives 12.8% — **so the
component rule alone does not explain it; something else shrank the cavity too.** Prime second
suspect: `HOLLOW_CAVITY_MARCH_MARGIN_MM` (0.866) inflating a requested 2.5 mm wall to an actual
3.366 mm, which on branchy geometry decides whether a limb has any cavity at all.

The old behaviour Craig describes — one cavity per limb, a drain per foot, a vent at the top — is
**multi-chamber hollowing with multiple drains**. Today's engine keeps one chamber reachable through a
single drain passage. Craig had already ruled (recorded as FU-70) that multi-cavity is allowed and
encouraged; it was never built. Suspect the Rust rewrite (2026-07-28,
[[resin-rust-rewrite-decision]]) lost capability the deleted Python engine had.

**How to apply:**
- Treat the named large minis as a regression suite: Giant Toad, Weeping Treant, Warhorse — plus Giant
  Bat and Rotting Warhorse, which also measure Large. See [[hollowing-is-for-large-minis]].
- Craig is producing **manually hollowed** versions of these models as ground truth to compare
  against. He will say where the files are. Compare cavity volume, wall thickness, drain count and
  position, vent position.
- Do NOT "fix" this by lowering the savings floor or the march margin — see FU-8b. Restore the cavity,
  don't move the bar.
- Distrust in-repo claims that the treant hollows: `first_artifact.rs` and `prepare.rs` assert it has
  done since 2026-08-11 and it does not. The `exports/weeping-treant-v1.2-first-artifact.chitubox`
  file is from 08-09 and is no longer reproducible.
- Separately: **"Awakened Tree.stl" and "Weeping Treant v1.2.stl" are the same file, byte for byte**
  (SHA-256 4c027b78…). One model, not two.
- Also open: Giant Toad fails for a different reason — a drain-planning memory estimate that charges
  every ray against every triangle (184 × 490,986), 95% of a 9.1 GB request. Real need ~443 MB against
  a 1 GiB cap. ~40 lines to fix, no cap change, unlocks 38% savings (9.5 mL) on the best model in the
  set.
- And: **no large mini produces a printable file at all today** — all six die in the support stage
  (exit 5). Hollowing savings are theoretical until that is fixed.
