# Weeping Treant v1 — Exact-Geometry Penetration Census

Ground-truth census of every illegal model crossing in the DELIVERED support forest, 
built from exact segment/sphere-vs-triangle geometry (not the sampled point cloud the 
engine used). This is the reference for the v5 exact-geometry core.

## Method

- **Members** (1625 live): each member's axis + 8 capsule-offset lines intersected with the mesh via Moller-Trumbore for EXACT crossing locations; the crossings partition each ray into sub-segments, each classified inside/outside by a dense-majority 3-axis ray-parity vote (robust to thin-feature flicker). Penetration depth via exact point-to-triangle distance.
- **Balls** (1102 joint balls): a ball breaches when its sphere spans a model feature THINNER than the ball can reach — the model's far wall lies within the ball radius, so the ball pokes out the far side (measured along the inward surface normal +/- a 25deg cone).
- Frame: mesh centered `mesh.apply_translation(-(bounds[0]+bounds[1])/2)`; verified aligned (stub tips sit the designed 0.3mm inside; strut/rung feet sit exactly on the surface).
- Sanctioned embed zones read from code: contact-tip stub embed `contact_depth_mm=0.3` (routing.py:61, clearance-exempt tree.py:368); designed-embed inset `contact_depth+pitch_bound+r_end+standoff` (tree.py:378-383, verify.py:110-115), pitch_bound(p99)=0.5706mm, standoff=0.6mm.
- **Validation**: heavy independent 9-ray parity agrees with **100%** of the worst offenders; all 44 breaching balls independently confirmed straddling the surface by 200-pt sphere sampling.

## Headline

| Quantity | Count |
|---|---|
| Illegal MEMBERS (penetrate beyond sanctioned embed) | **4** |
| — through-and-through (enter+exit, scar both sides) | 1 |
| — centerline (axis) transits | 4 |
| illegal member crossings (all rays) | 12 |
| Breaching BALLS (poke through thin feature) | **44** |
| — ball spheres reaching the surface (context) | 194 (so 150 are shallow anchors into thick surface = sanctioned) |
| — ball centers testing inside the model | 14 |
| **Upper-crown (z>35) illegal members** | 1 |
| **Upper-crown (z>35) breaching balls** | 29 of 44 |
| Delivered `verify` clearance_violations (sampled check) | 0 |

The delivered file's own sampled clearance check reported **0** violations. Exact geometry finds **44 ball breaches + 4 member penetrations** it missed — the direct fingerprint of under-sampling thin features.

Illegal members by kind: {'strut': 1, 'stub': 3}; through-and-through by kind: {'stub': 1}.
Breaching balls by owner: {'contact': 42, 'model_rung': 2}.

## Thin-feature correlation (the sampling-blindness signature)

Local feature thickness where the breach occurs (entry->exit distance / model wall spacing):

Ball breaches by feature thickness:
  [0-0.3) mm: 32
  [0.3-0.5) mm: 9
  [0.5-1) mm: 3
  [1-2) mm: 0
  [2-4) mm: 0
  [4-inf) mm: 0

**All 44 ball breaches occur at features < 1 mm thick; 32 of them at < 0.3 mm** — features far finer than the engine's ~0.3-0.6mm surface sample spacing, which is exactly why the sampled clearance check was blind to them.

Ball poke-out (visible protrusion past the far wall):
  [0-0.1) mm: 3
  [0.1-0.2) mm: 5
  [0.2-0.3) mm: 1
  [0.3-0.5) mm: 12
  [0.5-1) mm: 23
  [1-inf) mm: 0

Member illegal-transit length:
  [0-0.5) mm: 0
  [0.5-1) mm: 0
  [1-2) mm: 3
  [2-4) mm: 0
  [4-8) mm: 1
  [8-inf) mm: 0

Member max penetration depth:
  [0-0.5) mm: 1
  [0.5-1) mm: 2
  [1-2) mm: 0
  [2-4) mm: 1
  [4-8) mm: 0
  [8-inf) mm: 0

## Worst offenders — members (by transit length)

| member | kind | embed | owner | transit mm | depth mm | thru? | location (x,y,z) |
|---|---|---|---|---|---|---|---|
| 1166 | strut | p1 | ['contact', 270] | 5.078 | 2.405 | no | [29.431, -20.238, 2.587] |
| 403 | stub | p0 | ['contact', 529] | 1.764 | 0.316 | no | [24.483, 6.582, 49.971] |
| 357 | stub | p0 | ['contact', 455] | 1.55 | 0.595 | YES | [37.227, 6.273, 27.633] |
| 227 | stub | p0 | ['contact', 270] | 1.0 | 0.889 | no | [29.157, -18.359, 4.774] |

## Worst offenders — balls (by poke-out)

| ball | owner | r mm | feature thick mm | poke-out mm | buried frac | location (x,y,z) |
|---|---|---|---|---|---|---|
| 715 | ['contact', 607] | 1.061 | 0.0831 | 0.9779 | 0.017 | [10.339, -8.688, 45.354] |
| 1099 | ['contact', 1657] | 1.12 | 0.1779 | 0.9421 | 0.042 | [-2.256, 17.188, 40.743] |
| 808 | ['contact', 294] | 0.963 | 0.0504 | 0.9126 | 0.458 | [39.142, -2.587, 22.711] |
| 973 | ['contact', 1135] | 1.09 | 0.2618 | 0.8285 | 0.017 | [3.476, -11.338, 38.599] |
| 760 | ['contact', 693] | 0.854 | 0.0511 | 0.8031 | 0.2 | [0.851, -7.95, 52.501] |
| 762 | ['contact', 705] | 0.827 | 0.0516 | 0.7753 | 0.025 | [9.58, -7.979, 55.7] |
| 699 | ['contact', 554] | 0.808 | 0.0555 | 0.7522 | 0.217 | [25.955, 5.671, 44.093] |
| 754 | ['contact', 683] | 1.126 | 0.3844 | 0.7415 | 0.025 | [40.462, 17.882, 46.489] |
| 977 | ['contact', 1385] | 1.12 | 0.404 | 0.716 | 0.142 | [0.991, -5.57, -4.236] |
| 905 | ['contact', 126] | 0.853 | 0.1635 | 0.689 | 0.392 | [-13.559, -0.022, -35.699] |

## Verdict — what this means for printing the delivered v1 file

The damage is **cosmetic + finishing, not structural**. The dominant defect is **44 joint balls** (0.7-1.5mm radius) sitting on crown twigs finer than the balls themselves, so each sphere engulfs its twig and pokes out the far side by up to ~1.0mm — the gray lumps breaching thin blue branches in the screenshot. Because these are designed model-anchor feet (their placement genuinely wanted that surface, but the sampled cloud never saw how thin the twig was), the supports still hold; the model still slices and prints. The cost is at removal: a ball fused around and through a <0.3mm twig cannot be clipped without taking the twig with it — every one of the 29 crown breaches is a likely scar on a delicate branch. The member side is minor: only 4 members penetrate beyond their sanctioned embed and just 1 is through-and-through; the lone deep case is a strut buried ~2.4mm in the thick lower trunk (structurally harmless, a small scar). Net: the delivered v1 is printable as-is but will leave visible lumps and risk branch damage on support removal in the crown. The real lesson is for v5 — the sampled clearance core returned a clean bill (0 violations) on a file with 48 real illegal crossings, all at sub-millimeter features; the exact-geometry core must test the swept capsule/sphere against triangles, not a point cloud.
