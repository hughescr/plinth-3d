# Magic-supports Light records

Calibration piece, not a print candidate. Written-file observations [M], 2026-11-01.
No ChituBox opening, saving, mesh regeneration or slicing was exercised [U].

## Source and preservation

Source (read only):
`/Users/craig/code/hughescr/dnd-campaigns/plinth-review/2026-11-01-round25-first-supported/round 25.chitubox-magic-supports.chitubox`.
SHA-256: `6353ef51969166dad4e6dccac8db08260c8cc05bd78bdacecfec34da70dfd4a1`.

Zero-based model index **2**, whose native name ends in `#2`, is Light. The selection
is guarded by ten type-7 balls of radius 0.25, ten type-1 cones with radii 0.20 → 0.60,
and type-3 columns of radius 0.60. Index 1 is Better, not Light.

The calibration program is `crates/calibration` (`plinth-calibration`). Its extraction
retains the native model mesh, translate, presets, attrs and all support records. It
labels the model name `A0/A1; calibration piece, not a print candidate` and replaces
the misleading three-model preview with the native 800 × 600 background preview.
That labelled single-instance extraction is A0. A0 is not byte-identical to the
three-instance source container; its geometry and support data are unchanged.

A0 and A1 use **the same model name and preview**, so the comparison between those
two projects has no label or preview differences.

## What `0xc01` does and does not show

The native Light instance has 121 records. Its preset table in file order is:
Ultralight id 3, Middle id 2, Light id 1, Heavy id 4.

| Attr | Type | Records [M] |
|---|---|---:|
| `0xc01` | 1 (tip cone/rod) | 17 |
| `0xc01` | 3 (rod) | 31 |
| `0xc01` | 4 (base flare) | 12 |
| `0xc01` | 7 (tip ball) | 17 |
| `0xc01` | 9 (joint ball) | 21 |
| `0x1e00` | 6 (raft) | 23 |

All **98 non-raft** records carry `0xc01`, including main tips, branch tips, columns,
joints and bases. Its little-endian u64 bytes are `01 0c 00 00 00 00 00 00`.
Bits 0..1 are not zero. Thus the manual-support decoding in the main format guide
(samples 03–07: preset id shifted left two bits, kind in byte 1) is **not universal**.
The observations do not identify which bits mean preset, generation mode or kind.
In particular, do not call byte 1 a proven kind 12. There is no UI/save experiment
here to establish those meanings. Extraction and editing preserve every attr word.

## A0 → A1 field-by-field evidence

Evidence files live in
`/Users/craig/code/hughescr/dnd-campaigns/plinth-review/2026-11-01-cal1-feasibility/`:
`A0-A1-field-diff.json`, `A0-A1-tips.tsv`, and the two labelled `.chitubox` files.
Both projects are **945,274 bytes**; **8,512 byte positions** differ [M].

Exactly ten records differ: zero-based indices **1, 3, 5, 7, 9, 11, 13, 15, 17, 19**.
For each, the comparison of the parsed WRITTEN projects establishes:

| Field | A0 → A1 [M] |
|---|---|
| type, record +4 | 1 → 1, unchanged |
| start XYZ, +8..19 | all three f32 words unchanged |
| end XYZ, +20..31 | all three f32 words unchanged |
| r1, +32 | 0.20 → 0.052 mm; bits `0x3e4ccccd` → `0x3d54fdf4` |
| r2, +36 | 0.60 mm, unchanged |
| mesh pointer, +40 | unchanged (same layout and mesh sizes) |
| mesh length, +44 | 2,160 bytes / 60 triangles, unchanged |
| six tail words, +48..71 | unchanged |
| parallel attr word | `0xc01`, unchanged |
| stored mesh | rebuilt with geom's native 16-sided frustum from the unchanged endpoints and r2, and the new r1 |

Every other record, including its mesh, is unchanged. Model mesh, name, translate,
presets, attr array and preview are unchanged between A0 and A1. The program replaces
only those ten r1 fields and meshes in a clone of parsed A1 with their A0 values,
serialises it, and requires **byte-identical A0**. This checks the complete typed
project, not only a preselected list of fields. Reading/writing a container rebuilds
pointers; equal mesh lengths and counts mean A0/A1 pointers do not move.

SHA-256 [M]:

- A0: `a162762813a9574b6dbf83e4367924735f1ea43f15e83ebca2d7d85bd08353e0`
- A1: `1526c573339a29c585de99ead2e557116ea58e6fa84f6cd66be4c3ffeaaf936d`

## Exposed section, not the hidden cone start

The measurement cuts the written ball and cone triangles normal to the cone axis,
forms each convex section from triangle/plane intersections, and takes their **polygon
union**, not the cone alone or a hull that fills absent material. It searches the
ball-to-cone transition on 10,001 stations, spacing about 0.0001000 mm. These are
sampled minima, not certified continuous lower bounds. The ten per-tip values and
world positions are in `A0-A1-field-diff.json`.

| Written neck measurement | A0 [M] | A1 [M] |
|---|---:|---:|
| Minimum union area, range over ten tips | 0.151370991–0.151371259 mm² | 0.038954010–0.038954104 mm² |
| Axial distance below contact-ball centre | about 0.1118 mm | about 0.2219 mm |

A0/A1 area ratio is **3.885880–3.885896**. The CAL-1 criterion is section **area**
(the design's estimate is ×3.9); this is not a claim of ×3.886 smaller diameter.
The corresponding diameter ratio would be about 1.971. The mesh criterion is met;
A1 is not designated a second A0 at this stage. Slice and save checks remain
Unverified. These neck measurements do not measure the model-contact patch or the
minimum over every branched load path. The original thin branch tips remain:
plinth-check measures a narrowest width of **0.098079686 mm** in both files.

**Both representations are written consistently.** Whether ChituBox slices stored
meshes or regenerates them from fields is unknown. Changing both cannot establish
which representation it uses. Open A1, save a separate copy, re-read the fields,
and compare sliced neck pixels before treating it as a usable A1 contrast.

## Calibration-only construction notes

Coupons use native fields/meshes in the centred model frame, built directly there;
translating a finished f32 support mesh can exceed the canonical-shape ULP allowance.
The fixed taper is a type-3 frustum under a type-9 junction ball; the constant-radius
waist is type 1 under the contact ball, as in native branch tips. The normal manual
Light attr `0x0104` is used for newly built heads, not a guessed Magic-supports attr.
Raft attrs are `0x1e00`. Pads and MST strips use native type-6 records, with record
endpoint z=0 and the actual pad/strip heights only in the meshes.

The checker requires a preview on nonempty model containers. Use the valid native
background preview rather than omitting it. W has no model triangles by design;
its checker geometry verdict is Unverified, not made favourable with a dummy body.
The current checker cannot establish K continuity without native support records,
or apply the calibration H4 exception. Its raw results are retained without changes
to checker code. No inference here changes the production construction rules.
