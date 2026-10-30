---
name: sweep-field-feedback-2026-09-29
description: "Craig's ChituBox verdicts 2026-09-29 on field-2026-09-28/for-craig (develop 75c52c8): Round 25/50/100 broadly good but thin braced arms on the near-vertical inner rim wall are unneeded; ChituBox raft attaches to only SOME feet; stilt braces slice through the model + a brace 'starburst'; masts only for bracing still odd; brace_pair mast too thick; overhang table legs-up (orientation) → issue #1; Cylinder quills gone but bottom still overpacked, thin arms on self-supported upper region, top arms pierce the model"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-05T21:03:15.733Z
---

Craig in ChituBox, 2026-09-29, on `field-2026-09-28/for-craig/` (develop 75c52c8):

- **Round 25/50/100:** "clean", "broadly good". Defect: small number of skinny
  cross-braced arms landing on the inner rim of the edge wall — nearly vertical,
  fully supported from below, so unnecessary (= the unlanded wall-creep lane's
  target). Possibly over-supported overall, but "stick with assuming that is
  correct" until print testing.
- **Raft:** files carry no raft; adding a raft in ChituBox connects only SOME
  supports — he suspects a tiny gap between plate and foot, or feet below the plate.
- **make_tall_stilt_plate:** braces sometimes SLICE THROUGH THE MODEL; a
  "starburst" of cross-braces gone nuts in one spot; a parallel post that exists
  only to brace a nearby post (mast) is still present; long posts mostly get one
  X about halfway up — maybe physics-correct, looks unusual (fixture is weird).
- **make_overhang_table:** good, but oriented face UP / legs up; he accepts
  auto-orient may not fix this → asked for a GitHub issue "CLI flag to skip
  auto-rotation step" (created: hughescr/plinth-3d#1). make_brace_pair orients
  legs DOWN — he wonders why they differ.
- **make_brace_pair:** "quite good"; one bracing-only post is VERY thick for
  what it provides.
- **Cylinder:** quills GONE (good). Still "a shit-show" on overpacking: lowest
  supports packed tight, overlapping and melting together → heavy scarring,
  hard removal. Higher up: many very thin arms on regions that should
  self-support. The highest supports still seem to PIERCE the model.

**Later 2026-09-29, on for-craig-v3 (develop 0f70cf2):** Round 25 "quite good", Round 50 "overall very good" — only oddity: a few FAT side-branches landing on the edge of the inner-wall lip are almost completely shaved off by the seat trim (he thinks it probably works, but looks off). Stilt plate: cats-cradle and model-penetrating braces RESOLVED, "pretty good". Brace pair "very good; no notes". Sphere "very good" (odd mix of fat and very skinny arms, plausible). Cylinder: fewer spidery high arms, but the 4 HIGHEST arms taper extremely thin at the top and land near the TANGENT side of the curve where no support looks needed — "something about the cylinder is just really throwing the code". (Hypothesis, unverified: these are congestion-lane flank arms, placed to shed load, not demand.) He did not yet comment on the raft-in-ChituBox test.

**2026-10-01, on for-craig-v8 (develop 247b8d5):** Round_50 "looks great"; Sphere "very good/great" (so fat tips up to ~1.6 mm across accepted in practice — no 0.8 mm cap); Cylinder "way way better", maybe slightly crowded at the very bottom but "likely actually printable/usable now". Stilt plate good, BUT two very long forked branches run far in −x to land on a trunk when a much closer trunk in +y was available. Brace pair: NO cross-bracing now; very long free-standing columns look like they could bend ("suspicious") — thickening replaced bracing. Morel_base and make_laid_tube in v8: "pretty good, likely printable".

**2026-10-02 evening, on for-craig-v11 (develop f19ba85):** Morel_base "looks good now — nothing obviously wrong". Overhang table (legs down) "reasonable"; thick 45° braces plausible for their long runs. Cylinder: near the drain, fat supports overlap practically on the same spot ("can't possibly need more support immediately next to another post — why do they overlap?"). Cylinder + Sphere: fat lower posts that step abruptly to a thin member that actually touches the model ("why so thick, then a very rapid step?"). Cylinder + Sphere: clusters of 3–5 posts all cross-braced PAIRWISE — looks like overkill. make_lonely_peak: not convinced its long thin unbraced posts (esp. the thinnest of 3) will print.

**2026-10-05, v16 Cylinder:** "maybe a bit better than v15 but hard to tell" — he defers to the metrics for subtle Cylinder changes.

**Cylinder load idea (Craig, 2026-09-29 evening):** shift MORE load onto the outer/flank supports so bottom supports thin and flank ones thicken. His physics point: the Cylinder is HOLLOW, so each layer is ~2 parallel strips (the walls), roughly constant peel per layer (varies with wall angle), near-linear load vs height, upper half symmetric to lower. Queued: after round 4, MEASURE per-layer peel area, how the engine distributes each layer's peel among supports attached at that layer (all attached vs "lowest line" — congestion README says the body routes its whole load to the lowest line), and what flank sharing / a tilted pose would do. Only the first few layers (tangent bottom strip) must go through the bottom row alone.

**How to apply:** wall-creep lane (pending contact-sizing fix) covers the rim
arms. New work: raft/foot compatibility with ChituBox; brace-model collision;
starburst; mast necessity/sizing; Cylinder top piercing; Cylinder bottom
congestion is the standing big problem. Not yet reviewed by him: Sphere,
Morel_base, make_toed_slab (now flipped), make_laid_tube, make_raking_blade,
make_thin_fin.

Related: [[sweep-field-feedback-2026-09-28]], [[plinth-state-2026-09-28]].
