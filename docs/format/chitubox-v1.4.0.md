# ChituBox v1.4.0 `.chitubox` format

This document says how ChituBox v1.4.0 stores a project in a `.chitubox` file. It is the format reference for plinth's reader and writer.

## 0. Evidence and markers

- The owner made eight sample files in ChituBox v1.4.0. They are in `test-data/chitubox-native/samples/`. Each sample differs from the one it builds on by one change. The folder README lists them.
- Seven older `.chitubox` files are in `test-data/chitubox-native/`. Their ChituBox version is not known. They are used only as extra checks: `basic-cube-supports-raft`, `cylinder-cube-sphere`, `cylinder-supported-bottom-connected`, `empty`, `morel`, `morel_mudskipper_supports` and `two-supports`.
- Every count of files in this document is over the 15 `.chitubox` files in this checkout: the seven above and the eight samples `01-round25-plain`, `02-round25-lifted`, `03-round25-supports`, `04-round25-supports-raft-connected`, `04-round25-supports-raft-hex`, `05-two-plates`, `06-round25-flipped` and `07-round25-on-edge-braced`. The `.ctp` file in the same folder has another magic number, `0xAB231253`, and is not this format. The `.cfgx` file is a settings export.
- One agent decoded each sample pair. A second agent, from a different model family, checked each decode with its own decoder. Where the checker found a claim wrong, this document uses the correction. Where a point could not be settled, it is in section 15.
- Every section has a marker:
  - **Verified from samples**: measured from the bytes of the samples and checked by the second decoder.
  - **Inferred**: fits all the samples, but the bytes do not prove it.
- One more ChituBox-written file, a re-save of a plinth-written file, is in `test-data/chitubox-native/load-test-2026-10-30/` (section 18). It is the 16th native file. Counts in sections 1 to 16 stay over the 15 files above.
- Plinth-written files opened in ChituBox: model-only files with no supports (section 18), and calibration plates with supports (section 8.3, preset ids). "Verified" still means "this is what ChituBox wrote". It does not mean "ChituBox needs this". For anything with supports, only a load test in ChituBox can show what ChituBox requires.

Conventions:

- All integers are little-endian. All floats are IEEE f32, little-endian.
- All offsets are absolute file offsets unless the text says otherwise.
- Units are millimetres.
- Sample names are short: `01` is `01-round25-plain.chitubox`, `04c` is `04-round25-supports-raft-connected.chitubox`, `04h` is `04-round25-supports-raft-hex.chitubox`.

## 1. Container layout

**Verified from samples.**

The regions follow each other in this order, with no gaps:

| Order | Region | Size |
|---|---|---|
| 1 | Header | 24 B |
| 2 | Zero block | 396 B, all zero |
| 3 | Preview header | 32 B |
| 4 | Preview data | variable (RLE image) |
| 5 | Model table | 680 B per model |
| 6 | Front block | 24 B descriptor per model, then a preset block per supported model |
| 7 | Per-model data, in model order | see below |

Per-model data, for each model in turn:

- Unsupported model: the model mesh only.
- Supported model: model mesh, support-mesh blob, support index (16 B), group header (400 B, zero), support records (72 B each), trailer (20 B, zero).

Facts:

- In 14 of the 15 files, every byte belongs to exactly one region. The region sizes add up to the file size. The exception is `morel_mudskipper_supports.chitubox`. It is not a clean ChituBox save (section 14).
- The file has no footer after the last model, no version token, and no compression.
- No checksum field was found. Every nonzero byte outside the bulk regions is explained by a known field. This shows that no separate checksum region exists in these files. It does not prove that ChituBox does no integrity check.
- `empty.chitubox` (420 B) is the header plus the zero block. It has no preview.

Baseline map of `01` (184,646 B, one model, no supports):

| Range | Region |
|---|---|
| [0, 24) | header |
| [24, 420) | zero block |
| [420, 452) | preview header: w=2103, h=1580, data_off=452, data_len=75634 |
| [452, 76086) | preview data |
| [76086, 76766) | model table, 1 record |
| [76766, 76790) | front block: one zero descriptor |
| [76790, 184646) | model mesh, 2996 triangles |

## 2. Header

**Verified from samples** for the values. **Inferred** for the names of f12 and f16.

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | Magic `0xAB231243` (bytes `43 12 23 ab`). |
| 4 | u32 | Model count. |
| 8 | u32 | `off8`: model-table offset. Always `452 + preview data_len`. In `empty.chitubox` it is 420. |
| 12 | u32 | `f12`: 420 in every file with a model, 0 in `empty.chitubox`. 420 is the offset of the preview header. Guess: "preview-header offset, 0 if no preview". |
| 16 | u32 | `f16`: 412 in all 15 files. Meaning unknown. |
| 20 | u32 | `off20`: front-block offset. Always `off8 + 680 × count`. 0 when the count is 0. |

The 396-byte zero block at [24, 420) is zero in all 15 files. That includes supported, rafted and two-plate files. The old docs called it a "settings blob". No print settings are stored in it in any sample.

## 3. Preview

**Verified from samples** for the encoding. **Inferred** for what ChituBox does with it.

Preview header (32 B at 420):

| Offset | Type | Meaning |
|---|---|---|
| 420 | u32 | width w |
| 424 | u32 | height h |
| 428 | u32 | data_off, always 452 |
| 432 | u32 | data_len, in bytes |
| 436..451 | 4 × u32 | zero |

Preview data is a run-length-encoded image of u16 words:

- Pixel colour is 15 bits in one u16: red = bits 15..11, green = bits 10..6, blue = bits 4..0.
- Bit 5 is the repeat flag. It is never part of a colour.
- A word with bit 5 clear is one pixel.
- A word with bit 5 set is followed by a second u16, `0x3000 | (run − 1)`. The run is 2 to 4095 pixels. A run of 4095 stores 4094.
- Runs are greedy over identical colour words.
- The pixel count equals w × h exactly, and the decoder uses every byte.
- Re-encoding with a maximum run of 4095 gives the original bytes in every file. A maximum of 4096 does not.
- The background word is `0x0841`, near black.

What the image shows:

- It is a screenshot of the 3D view. It uses an oblique camera, so the round model looks like an ellipse and its sides are visible. (The decode of 02 said "top-down". That was wrong.)
- Its size changes from save to save. Sizes seen run from 650 × 1945 to 3154 × 2250.
- In `05` it shows both plates' models in one image.
- A small change to the scene (01 → 02) changed 98 pixels. The image is rendered again on each save.

Load test 2026-10-30 (section 18): ChituBox opened and sliced plinth files whose preview is one flat colour (800 × 600, nothing of the scene in it), and on save it replaced that preview with its own render (1298 × 1140).

Not known:

- Whether ChituBox reads the preview on load at all. It did not need a correct one.
- Whether ChituBox accepts an empty preview (no preview data). The old project recorded that ChituBox refused a project with an empty preview (2026-08-03). Nobody checked that again with v1.4.0.

## 4. Model table

**Verified from samples.**

One 680-byte record per model, back to back from `off8`, in model order.

| Offset | Type | Meaning |
|---|---|---|
| 0..255 | bytes | Name, UTF-8, NUL-padded. |
| 256 | f32 | translate X |
| 260 | f32 | translate Y |
| 264 | f32 | translate Z |
| 268 | u32 | Offset of this model's support index. 0 if the model has no supports. |
| 272 | u32 | Offset of this model's mesh. |
| 276 | u32 | Mesh size in bytes, a multiple of 36. |
| 280..679 | bytes | Zero in all 15 files. |

- Names seen: `Round 25.stl`, and `Round 25.stl #1`, `Round 25.stl #2` for duplicates. The longest name seen has 23 characters. The real capacity is not proven.
- No other field in the file refers to the name.
- Bytes after the name's NUL, up to 256, are zero.
- In `05` the second record's `+272` is 987950. Only the first model's mesh starts at the end of the front block. Each later mesh starts after the previous model's data.

## 5. Mesh

**Verified from samples.**

- Raw triangle soup: 9 f32 per triangle (three vertices, xyz each), 36 B per triangle.
- No normal, no count, no header, no compression.
- Winding is counter-clockwise seen from outside. The signed volume is positive.
- The mesh is always centred on its bounding box. The bounding-box centre is (0, 0, 0).
- In `01` the mesh equals the source `minis-to-process/Round 25.stl` minus its f32 bounding-box centre, bit for bit, triangle for triangle.
- `test-data/models/as-drawn/Round_25.stl` is NOT the source. It is flipped in z.

Changes between saves:

- The triangle order stays the same.
- The first vertex of a triangle can change. In `03`, 2179 of 2996 triangles have their three vertices rotated (same winding). In `07`, 1854 do.
- Coordinates can change by up to about 4e-6 mm. Some of these changes touch sign or exponent bits of values near zero (for example −3.2e−16 became 0). They are small coordinate changes, not only low-bit changes.
- A tool that compares meshes must allow cyclic vertex rotation and a small tolerance.
- A writer can copy mesh bytes unchanged.

## 6. Transforms: position, rotation, scale

### 6.1 Storage

**Verified from samples.**

- The only transform field is translate (X, Y, Z) at record +256, +260, +264.
- Rotation is baked into the mesh vertices. No angle, matrix or quaternion field exists in any sample.
  - `06` (01 flipped top-up): the mesh equals 01 under (x, y, z) → (x, −y, −z), a 180° turn about X. All 2996 triangles match at the same index and vertex order, within 2.9e-6 mm. X values are bit-identical. Y and Z are not bit-exact, so it is not a plain sign flip. How ChituBox computes the rotation is not known.
  - `07` (on edge): the mesh equals 03 turned 84.3463° about the axis (−0.7071, 0.7071, 0). Residual is about 1e-6 mm after cyclic vertex rotation.
  - Winding is kept after rotation.
- Scale is baked into the vertices (old corpus evidence; no v1.4.0 sample scales a model).
- After rotation the mesh is centred on its bounding box again.
- Support data is in the same model-local frame as the mesh. Translate does not shift it. In `05`, model 1 has translate X = 254.99997 and support coordinates identical to model 0's.

### 6.2 Translate Z (lift)

**Verified from samples** for the values. **Inferred** for the meaning.

- `01`: Z = 0. `02` (raised by hand, no supports): Z = 5.0. `06`: Z = 0. Every supported model: Z = 3.0.
- Z is a free field. It is not tied to supports. (The old claim "Z is 3.0 if and only if there are supports" is wrong.)
- Lifting the model (01 → 02) changed only this field. The mesh did not move.
- ChituBox kept the lift on save.
- In every supported file, the lowest support point is at local z = −(Z + half the model height):
  - `03`: −4.75 = −(3 + 1.75).
  - `07`: −15.4389 = −(3 + 12.4389).
  - `cylinder-cube-sphere` cube: −13 = −(3 + 10).
- Inferred meaning: Z is the height of the model's lowest point above the build plate.
- Inferred world height: `world_z = local_z − local_min_z + Z`. For a centred mesh, `local_min_z` is −(half height). So the plate is at `local_z = local_min_z − Z`.
- Not proven: the samples cannot tell "lowest point above plate" from "offset of the centre from the seated position". A file with two Z values on a model whose bounding box is not centred would settle it.
- The 3 mm lift in supported files matches ChituBox's automatic lift for supports. Which action set it is not recorded in the file.
- The old line `z_plate = −translate_z` (old `audit-invariants.md`) is wrong by half the model height.

### 6.3 Translate X and Y

**Verified from samples** for the values. **Inferred** for the rule.

- Inferred: `world_xy = local_xy + (X, Y)`. This fits `05` (second plate at X ≈ 255). The plate origin is not stored.
- `07`: translate (0.35530806, 0.35539436). This equals the shift of the bounding-box centre caused by the rotation (best-fit translation of the rotated 03 mesh is (−0.35531, −0.35539, −0.17250)). So in `07`, ChituBox moved the bbox shift from the mesh into translate X and Y.
- `06`: translate Y = −0.35355282 (about √2/4). The fitted translation of the flip is zero, so the 07 rule does not explain it. Cause unknown.
- A writer's rule "set translate XY to the bbox shift" is therefore not universal. It fits 07 only.
- Rotation pivot: not known. A centred result cannot show where ChituBox put the pivot.

## 7. Plates and multiple models

**Verified from samples** for the layout. **Inferred** for plate meaning.

- No record, table or field says "plate".
- `05` (two plates) is one container with model count 2. The second plate's model is a second model with translate X = 254.99997 (`0x437efffe`, two f32 steps below 255). Both have Y = 0 and Z = 3.
- How ChituBox got 254.99997 is not known. It does not match the Saturn 4 Ultra build size in `reference/datasheets/printers`. No printer profile is stored in the file.
- Guess: 255 mm is the plate pitch (plate width plus gap). One sample only.
- `cylinder-cube-sphere` has three models on one plate at (0,0,3), (24,0,3), (−30,4.5,3). It uses the same record type. So models on one plate and on different plates differ only in translate.
- Guess: ChituBox decides plate membership from X position. The samples cannot tell this from "model order = plate order".
- Supports belong to a model only through that model's +268 pointer and through position in the file. Support records have no model id.
- `05` byte checks:
  - Model 0's mesh, support blob, attr array and records equal `04c`'s, apart from moved offsets.
  - Model 1's equal model 0's, apart from moved offsets, the name, translate X and the order of its preset table.

Layout of `05` (1,931,514 B):

| Range | Region |
|---|---|
| [41022, 42382) | model table, 2 records |
| [42382, 42430) | 2 descriptors |
| [42430, 43408) | model 0 preset table, names, attr array (107 entries) |
| [43408, 44386) | model 1 preset table, names, attr array (107 entries) |
| [44386, 987950) | model 0: mesh, support blob, index, group header, records, trailer |
| [987950, 1931514) | model 1: same parts |

A mix of supported and unsupported models in one file is not in any sample.

## 8. Front block: descriptors, presets, attr array

**Verified from samples** for the layout. **Inferred** for the meaning of the attr bits.

This block sits between the model table and the first mesh. The old docs called it an "opaque front remainder". It has a clear structure.

### 8.1 Descriptors

- At `off20`: one 24-byte descriptor per model, in model order: `u64 preset_count, u64 table_ptr, u64 attr_ptr`. The two pointers are absolute.
- An unsupported model has an all-zero descriptor and no preset block (`01`, `02`, `06`).
- All descriptors come first. Then one preset block per supported model, in model order.

### 8.2 Preset block (per supported model)

- Preset table at `table_ptr`: `preset_count` entries of 24 B: `u64 id, u64 name_ptr, u64 name_len`.
- Names follow the table directly, joined with no NULs, in table order. Example: `UltralightMiddleHeavyLight`.
- Attr array at `attr_ptr`, directly after the names: one u64 per support record, in record order. No field stores its length. Its length is the support index's record count.
- The first model's mesh starts directly after the last attr array.
- Names seen: Ultralight, Middle, Heavy, Light. (The owner's notes say "Medium". The file says `Middle`.)
- The table lists all four presets even when a model uses only one (`07` uses only Ultralight).
- Ids are per file and per model. Table order is not the id:

| File | Ids |
|---|---|
| 03, 04c, 05 model 0 | Ultralight=1, Middle=3, Heavy=2, Light=4 |
| 04h, 07 | Ultralight=1, Middle=2, Heavy=3, Light=4 |
| 05 model 1 | same ids as model 0, table order Light, Heavy, Ultralight, Middle |

### 8.3 Attr values

- In samples `03`–`07`, low byte = preset id << 2. Verified: in `04h`, Middle and Heavy ids swap, and exactly the attr low bytes of the Middle and Heavy supports swap (0x108 ↔ 0x10c). Bits 0..1 are zero in that family. This is not a universal decoding: the owner's Magic-supports Light instance uses `0x0c01` on every non-raft record; its meaning is not established. Preserve it rather than applying the manual-support formula. Field evidence is in [calibration-light-attrs.md](calibration-light-attrs.md).
- In the `03`–`07` decoding, byte 1 = support kind code. Codes seen: 1, 2, 3, 4, 5, 6, 8. Code 7 was never seen. Do not infer a kind 12 from the Magic-supports value without a UI experiment.
- Inferred kind names, from the owner's placement order in `03`: 1 adaptive, 2 vertical, 3 upright, 4 forked branch, 5 forked joint, 6 forked small, 8 forked tree. No UI test confirmed them.
- `0x1e00` is on every type-6 (raft) record: kind byte 0x1e, preset byte 0.
- **Preset id 0 is not drawn (load test 2026-11-10, ChituBox v1.4.0).** A supported model whose preset table holds id 0 and whose non-raft attrs have preset byte 0 (`0x0400`, `0x0100`) loads, counts the support volume, and draws no supports. The same records with table id 1 and attr `0x0104` or `0x0404` draw in full. Kind 1 and kind 4 both draw. Preset ids must be 1 or more, and each attr's preset byte must name an id in the table. `plinth_emit::constructed` (the Round 25 diagnostic path) still writes id 0 / `0x0400`; the calibration crate rewrites it to id 1 / `0x0104` for CAL-2.
- All records of one support carry the same attr. Base pieces carry their parent support's attr.
- A brace between two supports of different kinds carries both values, one in each u32 half. In `07`, records 38 and 39 carry `0x0000020400000304`: low half 0x0304 (upright), high half 0x0204 (vertical). Which half is which support is not known.
- Two m2m supports in `03` both carry kind 6 but have different shapes (section 10.4). So the kind byte does not fix the shape.
- `cylinder-cube-sphere` also shows values like `0x0c01` and `0x4105`. The manual-support decoding does not apply to those values, nor to the owner's Magic-supports file. No field-to-mesh or slicing behaviour follows from these words alone.

## 9. Support section

**Verified from samples.**

### 9.1 Support index (16 B, at model record +268)

`u32 n_records, u32 records_off, u32 support_mesh_off, u32 support_mesh_size`

- `support_mesh_off` = end of the model mesh.
- `support_mesh_off + support_mesh_size` = offset of the index.
- `records_off` = index offset + 16 + 400.
- `support_mesh_size` = sum of all records' mesh lengths.

### 9.2 Group header

400 bytes, zero in every supported model of every file.

### 9.3 Support record (72 B)

| Offset | Type | Meaning |
|---|---|---|
| 0 | u32 | Magic `0xEA342389` |
| 4 | u32 | Type |
| 8 | 3 × f32 | start xyz |
| 20 | 3 × f32 | end xyz |
| 32 | f32 | r1 |
| 36 | f32 | r2 |
| 40 | u32 | Mesh offset. **Absolute file offset.** It is not relative to the blob. |
| 44 | u32 | Mesh length in bytes |
| 48 | 6 × u32/f32 | Tail words. Zero except as listed in section 10.1. |

- Record meshes lie in the support-mesh blob in record order. They touch with no gaps and no overlaps.
- A writer must change every record's +40 when anything before the blob moves.
- No record has an id, parent pointer or link. Supports connect only by geometry.

### 9.4 Trailer

20 zero bytes after each supported model's records. An unsupported model has none. (The old docs said this "footer" was not seen. It is in every supported file.)

## 10. Native supports

### 10.1 Record types

**Verified from samples** for shapes and fields. **Inferred** for roles.

Two mesh shapes are used:

- **Frustum** (types 1, 2, 3, 4, 5): closed 16-sided frustum, 32 vertices, 60 triangles, 2160 B. Its axis runs from start to end.
- **Sphere** (types 7, 8, 9): closed UV sphere, 242 vertices (2 poles + 15 rings of 16), 480 triangles, 17280 B. Start and end are the two poles, so the centre is (start + end)/2 and |end − start| = 2r. r1 = r2 = r.

For types 1 to 4, the mesh radius at start is r1 and at end is r2.

| Type | Shape | Role (inferred from position) | Tail word +48 |
|---|---|---|---|
| 7 | sphere | Upper tip ball at the model contact. | 0 |
| 1 | frustum | Tip cone from tip-ball centre to first joint centre. r1 = tip radius, r2 = arm radius. Can be a cylinder (forked small: r 0.05 → 0.05). | f32. Values seen: 0.1, 0.2, 0.3, 0.0404. |
| 9 | sphere | Joint ball. Poles vertical. | 0 |
| 3 | frustum | Rod: pillar, trunk, branch, link, leg, post or brace. | 0 |
| 4 | frustum | Base flare: pillar radius → 1.1, 1.0 mm tall. | 0 |
| 5 | frustum | Base pad of the "no raft" default mat. **Radii are reversed** (see below). | 0 |
| 2 | frustum | Lower foot cone of a model-to-model support. start = lower contact end, r1 = small end. | 0.15 in all records |
| 8 | sphere | Lower foot ball of a model-to-model support, r 0.125. Follows a type 2. | 0 |
| 6 | prism / box / mat | Raft part (section 11). | 0.0 pad, 1.0 bridge; +52 = 4.0 |

Type 5 exception (all ten pads in `03`): start z = −3.95 (top), end z = −4.75 (bottom), r1 = 3.64, r2 = 5.0. The mesh radius is **5.0 at the top (start)** and **3.64 at the bottom (end)**. A writer that maps r1 to start would make the pad upside down. A bounding-box check cannot see this.

Type 1 tail word: for the four plate-support presets in `03`, it equals the tip-ball radius minus the distance from the ball centre to the model surface (Heavy 0.3, Middle 0.2, Light 0.1, Ultralight 0.1). Record 85 does not fit (0.0404 stored, about 0.058 by this rule). So the meaning "penetration depth" is a guess.

The numeric type does not fix the shape. Type 1 can be a cone or a cylinder. Type 9 radii vary. Type 3 can be a cylinder or a tapered frustum. A writer must use each record's own fields.

### 10.2 Connections

**Verified from samples.**

- Parts connect by geometry only. A tip ball's centre is the tip cone's start. A cone's end is the joint ball's centre. A tree branch ends at the other support's joint centre.
- These points match within float noise (for example 2.4e-7 mm), not exactly.
- Base pieces overlap by about 0.002 mm: in a Heavy base, pillar end z −2.954, flare start z −2.952, flare end −3.952, pad top −3.950.
- A reader must match points with a tolerance. A writer may make them exact.

### 10.3 Plate supports in `03` (records 0 to 65)

**Verified from samples** for the geometry. **Inferred** for the kind names.

Head chain for every plate support: tip ball (7), tip cone (1), joint ball (9). Each support's base is pad (5), flare (4), pillar (3), at the joint's x,y.

| Records | Attr | Kind (inferred) | Notes |
|---|---|---|---|
| 0–2 | 0x0108 | Heavy adaptive | tip r 0.4; cone 0.3 → 0.75; arm 3.0; joint r 0.78 |
| 3–5 | 0x010c | Middle adaptive | tip r 0.25; cone 0.2 → 0.6; arm 2.0; joint r 0.624 |
| 6–8 | 0x0110 | Light adaptive | tip r 0.15; cone 0.1 → 0.3; arm 2.0; joint r 0.312 |
| 9–11 | 0x0104 | Ultralight adaptive | tip r 0.1; cone 0.075 → 0.15; arm 1.5; joint r 0.26 |
| 12–14 | 0x0204 | Ultralight vertical | arm about 15.95° below horizontal |
| 15–17 | 0x0304 | Ultralight upright | tip, arm and joint all vertical |
| 18–20 | 0x0404 | Ultralight forked branch | same as adaptive; it did not fork |
| 21–23 | 0x0504 | Ultralight forked joint | same as adaptive |
| 24–26 | 0x0604 | Ultralight forked small | arm cone is a cylinder r 0.05 |
| 27–31 | 0x0804 | Forked tree, support A | 7, 1, 9, then trunk (3, r 0.25) and a second joint (9) |
| 32–35 | 0x0804 | Forked tree, support B | 7, 1, 9, then branch rod (3, r 0.15 → 0.25) ending at A's lower joint centre. No base of its own. |
| 36–65 | parent's | Bases | ten triples of 5, 4, 3 |

Preset geometry seen in `03` (not stored as settings anywhere; only in the geometry):

| Preset | Tip ball r | Tip cone r1 → r2 | Arm length | Joint ball r | Pillar r |
|---|---|---|---|---|---|
| Heavy | 0.4 | 0.3 → 0.75 | 3.0 | 0.78 | 0.75 |
| Middle | 0.25 | 0.2 → 0.6 | 2.0 | 0.624 | 0.6 |
| Light | 0.15 | 0.1 → 0.3 | 2.0 | 0.312 | 0.3 |
| Ultralight | 0.1 | 0.075 → 0.15 | 1.5 | 0.26 | 0.25 |

- Joint ball radius = 1.04 × pillar radius.
- Flare: pillar radius → 1.1 over 1.0 mm.
- Distance from tip-ball centre to the model surface: 0 (Ultralight, ball half buried), 0.05 (Middle, Light), 0.1 (Heavy).
- Arms point straight out from the model centre (within about 1°) and slope 20° down.
- The two 15.95° arms (vertical kind, tree B) point exactly along the outward normal of the nearest model face (dot product 1.00000000). So ChituBox probably aims these along the surface normal. That is inferred, not decoded.
- These sizes are what ChituBox wrote in one session. They are not proven fixed constants.

### 10.4 Model-to-model supports in `03` (records 66 to 97)

**Verified from samples.**

| Records | Attr | Chain | Notes |
|---|---|---|---|
| 66–71 | 0x0104 | 7, 1, 9, 9, 3, 2 | thin: arm r 0.05 at 45° down; joints r 0.052; post 0.1 long; foot cone r 0.05 |
| 72–77 | 0x0204 | 7, 1, 9, 9, 3, 2 | thin: 0.2 mm horizontal stub; post 0.892 long |
| 78–83 | 0x0604 | 7, 1, 9, 9, 3, 2 | thin, like 66–71 |
| 84–90 | 0x0804 | 7, 1, 9, 9, 3, 2, 8 | thick: arm 0.075 → 0.15 at 45°; joints r 0.26; post r 0.25; foot cone 0.075 → 0.15; foot ball r 0.125 |
| 91–97 | 0x0604 | 7, 1, 9, 9, 3, 2, 8 | thick, but keeps a r 0.05 cylinder arm |

The owner's note says "one of each type". The file shows kinds 1, 2, 6, 8, 6. This is not explained (section 15).

### 10.5 Record order

**Verified from samples.** Order differs by file:

- `03`: heads (0–35), plate bases (36–65), m2m (66–97).
- `04c`: heads (0–35), m2m (36–67), raft bridges (68–76), raft pads (77–86), flare/pillar pairs (87–106).
- `07`: the first ten supports' bodies, then a raft block, then legs and flares, then later supports each with their own bridge and pad.

The order carries no data beyond the attr array being parallel to it. Whether ChituBox needs a particular order is not known.

### 10.6 Long supports and articulation (`07`)

**Verified from samples.** All supports are Ultralight (attr low byte 4).

Logical chain of a plate support (not the file order): tip ball (7) → tip cone (1) → joint (9) → optional 45° link rod (3) → second joint (9) → long leg (3) → flare (4) → raft pad (6).

- 45° link rods: records 22, 27, 36, 43, 48. r 0.25 → 0.25. Horizontal run equals vertical drop. They move the column sideways before it drops to the plate. The next leg starts exactly at the link's end.
- Long legs: 12 records, r 0.25 → 0.35 (wider at the bottom), vertical, all ending at z −14.2429. Lengths 6.71 to 16.56.
- Untapered rods also occur at lengths up to 6.14 (links) and 3.52 (record 16, an m2m post). So the taper is not decided by length alone. Its trigger is not known.
- Flares: 12 records, r 0.35 → 1.10 over 1.0 mm, z −14.241 to −15.241.
- Record 143 is a short rod (0.829 long) that lands on its pad with no flare.
- Type 1 in `07`: 15 records are 0.075 → 0.15; records 51 and 54 are cylinders 0.05 → 0.05. All have +48 = 0.1.
- Type 9 in `07`: 24 records r 0.26; records 67, 71, 125 r 0.15.
- Tip-ball centres sit 0.0014 mm (mean) from the model surface in the local frame. This confirms that supports use the local frame, with no translate added.

## 11. Rafts

The raft style is not stored as a setting. It shows only as record content. The zero block and model record do not change between raft styles.

### 11.1 "No raft" default mat (`03`)

**Verified from samples.**

- Ten type-5 pads, one under each plate support, centred at that support's joint x,y.
- Each is 0.8 mm thick, z −4.75 to −3.95, bounding box 10 × 10 × 0.8. The pads overlap each other.
- Radii are reversed relative to the fields (section 10.1).
- The flare ends at −3.952, 0.002 mm below the mat top.
- No type-6 record is present.

### 11.2 Connected raft (`04c`)

**Verified from samples.**

03 → 04c: the ten type-5 records go. 19 type-6 records come in: 9 bridges, then 10 pads. Size change +9,690 B = −2,118 preview + 72 attr + 11,088 blob + 648 records.

All 19 type-6 records:

- attr `0x1e00`.
- start z = end z = 0.0 in the record. The real z is only in the mesh (z −4.75 to −4.55).
- r1 = 2.2, r2 = 0.2.
- Tail words: pad `[0, 4.0, 0, 0, 0, 0]`; bridge `[1.0, 4.0, 0, 0, 0, 0]`. +48 tells pad from bridge. The meaning of 4.0 is not known.

Pad:

- start xy = end xy = support base xy.
- Mesh: 20-sided prism, 80 triangles (20 top, 20 bottom, 40 side), 42 vertices. Rim radius 1.1 (r1/2). Rim vertices at 0°, 18°, 36° … counter-clockwise. Top at −4.55, bottom at −4.75.

Bridge:

- start xy = earlier pad centre, end xy = later pad centre.
- Mesh: box, 12 triangles, 8 vertices. Half-width 1.1 across the segment. Flat ends at the pad centres. Same z range as pads.

Foot pieces under a connected raft:

- Each type-4 flare moves down 0.6 mm as a whole (−2.952..−3.952 → −3.552..−4.552). It stays 1.0 mm tall.
- Each type-3 pillar grows longer downward. Its start stays. Its end moves from −2.954 to −3.554.
- The pillar end is 0.002 mm **below** the flare start (an overlap).
- The flare bottom is 0.002 mm below the raft top.
- In `04c` these pairs moved to the end of the record list, flare then pillar per support.

Bridge choice:

- In `04c`, the 9 bridges link pads k to k+1 in record order, which is also the Euclidean minimum spanning tree (MST) of the 10 pad centres. The bridge records appear in ascending length (1.550, 1.724, 1.741, 1.760, 1.872, 2.358, 2.444, 3.049, 3.900 mm), the order Kruskal's method accepts them.
- In `07`, 13 pads and 12 bridges form a tree that is not the final MST. It fits exactly this: the MST of the first ten pads, then each later pad joined to its nearest earlier pad (10→9 at 6.431, 11→10 at 4.906, 12→5 at 3.514). Inferred: ChituBox builds the tree as supports are added. Braces are not needed to explain it.

Constants seen in every connected-raft file: pad diameter 2.2, thickness 0.2, 20 sides, bridge width 2.2. No sample varies them.

### 11.3 Hex raft (`04h`)

**Verified from samples** for the bytes. **Inferred** for the sentinel meaning.

04c → 04h: the 19 type-6 records are replaced by one type-6 record (index 68, same place in the list). Size change +97,840 B = +32,176 preview − 144 attr + 67,104 blob − 1,296 records.

The hex record:

- type 6, attr `0x1e00`.
- start = (0, 0, −1.0e8), end = (0, 0, −1.0e9). These values appear in no other file. Guess: placeholders.
- r1 = r2 = 1.0. All tail words zero.
- Mesh: 2772 triangles (99,792 B). z only at −4.75 and −4.55 (0.2 thick). Bounding box (−15.63, −15.54) to (16.71, 16.70). 894 upward flat triangles, 697 downward, 1181 side. Upward triangle area sums to 565.85 mm² (overlaps not removed).
- The mat goes 3.1 mm past the model on the minimum sides and 4.2 mm on the maximum sides. Cause not known.

The hex mat mesh is not welded:

- 1,722 undirected edges are used once and 3 are used more than twice (exact vertex match).
- After merging vertices closer than 0.01 mm (distance test), 34 edges stay open. (An f32 grid test gives 46; an f64 grid test gives 1,346. The count depends on the method.)
- The connected-raft pads, bridges and the type-5 mats have no open edges.

Supports over the hex mat:

- 4 of the 10 supports have their base x,y over a hole (no upward mat triangle covers that point). The other 6 sit on solid mat.
- On solid mat: flare bottom −4.552, 0.002 mm below the mat top. Same as connected.
- Over a hole: flare moves to −3.750..−4.750 and the pillar end to −3.752. The flare bottom equals the plate level (the mat bottom), −4.750. There is no extra extension below the plate.
- Of the 88 non-raft records, 80 are unchanged apart from +40, and 8 (these four flare/pillar pairs) changed.
- Inferred: ChituBox ends each support on the surface under it. Whether it tests the centre point or the whole base is not known.

## 12. Bracing (`07`)

**Verified from samples** for the geometry. **Inferred** for how ChituBox chooses braces.

- Braces are ordinary type-3 records. No brace table, flag or field exists. A brace is known only by its shape.
- The 20 braces are the type-3 records with r1 = r2 = 0.15: records 6–11, 29–32, 38–39, 56–59, 119–120, 134–135.
- Each is a straight cylinder from start to end, 60 triangles, with no extension past the ends. All tail words zero.
- Every brace slopes exactly 45° (horizontal run equals vertical drop within 5e-7 mm).
- Both ends lie on the vertical axes of two neighbouring supports.
- Braces come in crossing pairs (an X). Ten pairs. Direction and order inside a pair vary.
- The z spacing between X levels is irregular. The rule is not known.
- In the file, each brace pair comes after the later of the two supports it joins.
- A brace has the same attr as the supports it joins. A brace between two different kinds has both attrs (section 8.3).
- Six bridges join the same support pairs as braces (records 74, 75, 76, 77, 121, 136).
- The owner's README says 07 has auto-bracing. The bytes do not say which braces were automatic.

## 13. What a writer must keep consistent

These are true in every clean sample. They are **not proven to be requirements** of ChituBox. Copying them is the safe choice until a load test shows otherwise.

Header and preview:

1. Magic, model count, `f12` = 420 (0 when empty), `f16` = 412.
2. Preview header: data_off = 452, data_len = exact RLE length, 4 trailing zero words.
3. Preview RLE as in section 3, with exactly w × h pixels.
4. `off8` = 452 + data_len. `off20` = `off8` + 680 × count.
5. Zero block [24, 420) all zero.

Model table:

6. One 680-byte record per model. Name NUL-padded. Translate at +256/+260/+264. +268 = support index offset or 0. +272 = this model's mesh offset (from the real layout). +276 = 36 × triangles. +280..679 zero.

Front block:

7. One 24-byte descriptor per model. Zero for an unsupported model.
8. For a supported model: preset table, names joined in table order (each `name_ptr`, `name_len` right), attr array with one u64 per record.
9. For the manual-support family (`03`–`07`), attr low byte = (preset id << 2) for an id in that model's own table, with kind code in byte 1. `0x1e00` for raft records. Preserve unknown attrs such as the Magic-supports `0x0c01`; section 8.3 is not a universal decoding.
10. Each block's pointers move when an earlier block grows. The first mesh starts right after the last attr array.

Per supported model:

11. Mesh, then support blob, then index, then 400 zero bytes, then records, then 20 zero bytes. The next model starts right after.
12. Index: n = record count = attr array length; records_off = index + 416; support_mesh_off = mesh end; support_mesh_size = sum of record mesh lengths; index offset = support_mesh_off + support_mesh_size.
13. Each record: magic, type, start, end, r1, r2, absolute mesh offset, mesh length. Record meshes in record order, no gaps.
14. Tail words: type 1 +48 = contact value; type 2 +48 = 0.15; type 6 pad `[0, 4.0, 0…]`, bridge `[1.0, 4.0, 0…]`, hex mat all zero; others zero.
15. Type-6 records: start z = end z = 0.0 (connected) or the hex sentinels. The mesh carries the real z.
16. Record mesh shapes match the record fields: 16-sided frusta for types 1–5 (with type 5's radii reversed), 242-vertex UV spheres for 7, 8, 9.

Geometry:

17. Mesh centred on its bounding box. Rotation and scale baked into vertices. Winding counter-clockwise outward.
18. All support data in the model-local frame. Plate level at local z = local_min_z − translate Z.
19. Raft 0.2 thick on the plate. Feet end 0.002 mm below the raft top, or at the plate over a hex hole.
20. The type-6 record list and its attr entries change size together with the record count.

If any record is added or removed: the attr array, the support index, the record region, every later pointer and every record's +40 must change with it.

## 14. Corrections to older docs and to the decode notes

- Preview is a 32-byte header plus 15-bit RLE, not "raw, unmapped".
- `f12` is not "settings size". The 396 bytes are zero in all samples; no print settings are stored there.
- The front block has a clear structure (section 8). It is not opaque. The old line that the preset tables were "stale / not reproduced" is wrong: they are in the front block, not in the group header.
- Triangles are not reordered on save. Only the first vertex of a triangle can change.
- Translate Z is a free field, not "3.0 if and only if supports".
- The 20-byte trailer is present in every supported model.
- Type-6 tail words are not zero. Types 2, 5 and 8 exist.
- Record mesh offset (+40) is absolute, not relative to the blob.
- `morel_mudskipper_supports.chitubox` is not a native save. It is byte-identical to `morel.chitubox` up to offset 7,790,753, except for the 3 bytes of the support-index pointer. Its front block still has morel's 13-entry attr array, but it has 243 records. So the attr array reads into the mesh. Its old support blobs are still there, unreferenced. Guess: the old plinth writer made it by replacing the records and keeping the front block unchanged. The old project says ChituBox loaded it. Nobody checked that here.
- Rotation is not "the writer applied a float32 rotation matrix". The bytes show only that it is not a plain sign flip.
- The format has length and id fields (preview length, name lengths, preset ids). What was not found is a checksum.

## 15. Open questions

Ordered by what they unblock. Each needs a load test in ChituBox or a new owner sample.

1. Does ChituBox accept a file whose attr array length is not equal to the record count? (Blocks any writer shortcut that keeps the front block.)
2. Does ChituBox read the record mesh blobs, or rebuild them from the record fields? (A file with one radius field changed and the blob unchanged would tell.)
3. Does ChituBox accept an empty preview (zero bytes of preview data, `f12 = 0`)? (Answered for a written, flat-colour, wrong-scene preview: it opens and slices, and ChituBox re-renders the preview on save. Section 18.)
4. Is translate Z "lowest point above plate" or "centre offset from seated position"?
5. Does ChituBox accept a file with a supported and an unsupported model together? What is the unsupported model's descriptor?
6. Does ChituBox need a particular record order, bridge order or bridge tree?
7. What do the hex sentinels (−1e8, −1e9, r 1.0) mean? Does ChituBox accept a welded hex mat, or a plain slab?
8. What does `f16` = 412 mean? What does type-6 +52 = 4.0 mean?
9. What is attr kind 7? Which UI actions made the five m2m supports in `03` (kinds 1, 2, 6, 8, 6)?
10. In a two-kind brace attr, which u32 half is which support?
11. What sets the 255 mm plate pitch? Is it tied to the printer profile?
12. Why does `06` have translate Y = −0.35355 when its mesh needed no shift?
13. What triggers leg taper, flare omission, brace placement and X-pair spacing?
14. Is the name capacity 256 bytes? What happens to a long name?
15. Are pad size 2.2, raft thickness 0.2, the 0.6 mm flare shift and the 0.8 mm default mat fixed, or driven by a raft setting not varied in the samples?
16. Does ChituBox rebuild the front block from the support state on save, so that a writer may also rebuild it freely? (Answered for an unsupported model only: the re-save has the same all-zero descriptor at the new offset, so the block is rebuilt or copied with the same result. Nothing is known for supported models. Section 18.)

## 16. Review of the old chitubox_io reader/writer

Source reviewed: the old `rust/chitubox_io` crate (about 2,500 lines of library code, 3,300 lines of tests and examples), restored to the session scratch area. It was written for ChituBox Basic 1.3.0.

Test run: I built the old reader and ran parse then save on every native file. All parsed, and every save was byte-identical to the input. So the old reader reads v1.4.0 files and keeps them intact when nothing is changed.

### 16.1 Choices the samples confirm

- Pointer-chain parse (header → model table → +268/+272/+276 → index → records). No scanning for magic values.
- Every byte must belong to one region, or the parse fails.
- `off8` = 420 + preview length (the old crate counted the preview header as part of the preview), `off20` = `off8` + 680 × count, `off20` = 0 when empty.
- `f12` = 0 when empty and 420 once a model is added. `f16` = 412.
- Model record offsets +256/+260/+264/+268/+272/+276. Bytes 280..679 kept.
- Mesh as 36-byte triangles, no normals, centred on the bounding box.
- Rotation and scale baked into vertices. Translate is the only transform field.
- Support index layout and relations. 400-byte zero group header.
- Record layout, and +40 written as an absolute offset (relative in memory, rebased on write).
- Record blobs in record order, no overlap.
- Type-6 start/end z = 0.0, pad 2.2 diameter, 0.2 thick, 20 sides; feet 0.002 mm into the pad; MST bridges (true for `04c` and `05`, not the whole story for `07`).
- Checked arithmetic, and checking every input before changing anything.

### 16.2 Choices the samples contradict

- **Front block treated as opaque.** The old crate keeps it byte for byte and cannot resize it. Every supported model needs a preset table and an attr array with one entry per record. So:
  - Adding supports to a new project writes a zero descriptor and no attr array for a supported model. No native file looks like that.
  - Changing supports in an opened project keeps the old attr array. That gives exactly the broken `morel_mudskipper_supports` layout (section 14).
  - It refuses to add a model to an opened project at all.
- **No 20-byte trailer on new support sections.** The old crate keeps trailers it parsed, but builds none for new supports. Every native supported model has one.
- **Preview treated as raw and unknown.** New projects get a copy of another file's preview (from `two-supports.chitubox`). The preview can now be decoded and written (section 3).
- **Support type names.** It named type 9 `SHAFT`; type 9 is a joint ball. Types 2, 5 and 8 have no names. Unknown types do pass through, which is good.
- **Tail words default to zero.** Types 1, 2 and 6 need nonzero tail words (section 10.1). The caller had to know this.
- **Type 5 radii.** The old encoder maps r1 to start. Native type-5 pads have the mesh radii the other way round.
- **Lift doc.** It says Z is 3.0 only with supports. Z is a free field.
- **Z rotation.** It turns the mesh and supports about the local origin and does not centre the mesh again or change translate. Native files keep the mesh centred after a rotation (`06`, `07`), and `07` moves the shift into translate. Whether ChituBox needs this is not known.
- **Record order.** The old plinth emitter that fed this crate put the raft after all other records (old format doc, section 5). Native files put the raft between the m2m supports and the foot pieces. Whether this matters is not known.

### 16.3 Recommendation

**Do not restore it. Write a new reader/writer, and use the old crate only as a read-only reference.** (Done: `crates/chitubox` is the new crate; section 17.)

Reasons:

- The main defect is in the core design. The "keep unknown regions verbatim" model is why the front block, the trailer and the preview are wrong. Fixing it means changing almost every file: `document`, `parse`, `write`, `support`, `project`, `types`, `verify` and `consts`.
- Much of the old surface is not needed now: the opaque-region API, the rotation snapshot state, the API version constant, and the Python-era history.
- The format is now known well enough that every region can be typed. A new crate can model the header, preview (with an RLE codec), model records, front block (descriptors, presets, attrs), support sections and trailers as plain data. It can then rebuild every offset and count on save. That is smaller and easier to test than patching the old design.

Keep these ideas from the old crate (not its code): pointer-chain parse, the every-byte-in-one-region check, checked arithmetic, validate-all-then-change, and absolute-offset rebasing. Use the 15 files as the round-trip test set: the new reader must save each one byte-identical, as the old one did.

The first output of the new writer must still go through the owner's ChituBox load test (PRD §12). Questions 1 to 3 in section 15 decide how much of section 13 the writer must copy exactly.

## 17. How `plinth-chitubox` reads and writes this format

**Verified by tests on the 16 ChituBox-written `.chitubox` files in `test-data/chitubox-native/`** (the 15 of section 0 and the load-test re-save). Every one parses and writes back byte for byte. The plinth-written file in the load-test folder is not in this set. This says nothing about what ChituBox accepts (section 0).

Typed data: `Project { header_f16, preview, models }`. A `Model` holds name, translate, mesh and optional `Supports { presets, attrs, unreferenced_blob, records }`. No offset, count or length is stored. The writer rebuilds all of them (sections 13 and 9).

The reader follows the pointer chain in file order. Each pointer must equal the position where the previous region ended, so every byte is in one region and regions cannot overlap. It rejects:

- a region that is zero in every native file but holds a nonzero byte (zero block, preview header padding, model record bytes 280..679, group header, trailer, bytes after a name's NUL);
- a name without a NUL (the name must be shorter than 256 bytes), or a name or preset name that is not UTF-8;
- a NaN or infinite float in a translate, mesh vertex, record start, end or radius;
- preview data that is not the canonical greedy run-length encoding of exactly `w × h` pixels, or a preview over 33,554,432 pixels;
- a support record whose mesh does not start where the previous record's mesh ended;
- bytes after the last region.

Two points where the reader differs from a plain reading of sections 8 and 9, so that `morel_mudskipper_supports.chitubox` (section 14) parses:

- The attr array has no length field. The reader takes it as the bytes from the attr pointer to the next preset block, or to the first mesh. A clean save has one entry per record. `morel_mudskipper_supports` has 13 entries and 243 records.
- The support mesh blob may start with bytes that no record refers to (`unreferenced_blob`). The record meshes follow them with no gap. A clean save has none. `morel_mudskipper_supports` has 88,272: morel's old blob. The new record meshes start where morel's index was.

`Project::deviations()` lists both cases. It is empty for every file except `morel_mudskipper_supports`. A writer that builds a project from nothing leaves it empty.

The writer refuses before it writes a byte: a name of 256 bytes or more or with a NUL, a non-finite float, a preview whose pixel count is not `w × h` or has bit 5 set in a pixel, and any size or offset that does not fit 32 bits. It does not check that the attr array has one entry per record, that preset ids in attrs exist, or any other meaning (sections 8.3, 10, 13). Those are the caller's job until a load test shows what ChituBox needs.

## 18. Load test 2026-10-30

**Verified** for the facts below. Guesses are marked **Guess**.

### 18.1 Record (H8 style)

- Application: ChituBox v1.4.0 (the owner's install; no other build string recorded).
- Date: 2026-10-30. Reported by the owner.
- What was tested: six plinth-written Round files. All are model-only "transport" files: the approved 65°-tilted copy, lifted 5 mm, no supports. The owner opened all six in ChituBox and sliced all six. All opened and sliced fine. The names and hashes of the other four files are not in this checkout.
- What was recorded for two files (`test-data/chitubox-native/load-test-2026-10-30/`):
  - `Round 50.keepup.chitubox`, written by plinth, 218,132 B, SHA-256 `d6ef89c07cd98007d1c91c916f305eabcddede7151f8c3a6c01337d685efbb83`. Opened and sliced.
  - `Round 50 resaved.keepup.chitubox`, written by ChituBox when the owner saved the file above again, 309,740 B, SHA-256 `257578c0ae822e4bfea2bb2b322b94acf9a06a915a97ebf024db52fa2b67a4d6`.
- What this is not: an H8 pass for a printable file. These files have no supports, so nothing here shows ChituBox accepting supports that plinth wrote. It shows only that the container, the preview, the model record and the mesh that plinth writes are accepted.

### 18.2 What ChituBox changed in the re-save

Compared byte by byte, and with the reader and the checker's `dump`. The only differences are the preview and the offsets that follow it.

| Part | Plinth-written | ChituBox re-save | Same? |
|---|---|---|---|
| Magic, model count, `f12` (420), `f16` (412) | as in section 2 | same | yes |
| Preview size | 800 × 600, 472 B, one colour (`0x0841`, the background word) | 1298 × 1140, 92,080 B, 45 colours, a render of the scene (the round plate seen from an oblique camera, with the three small hollow bosses) | **no** |
| Preview header: data_off (452), padding | 452, zero | same | yes |
| Header `off8`, `off20` | 924, 1604 | 92,532, 93,212 | no, but both are `452 + data_len` and `off8 + 680`, as section 2 says |
| Model record: name | `Round 50.keepup [transport, not a print candidate]` | same, byte for byte | yes |
| Model record: translate | (0, 1.3747296, 5) | same bits | yes |
| Model record: +268 (support index) and +276 (mesh size) | 0, 216,504 | same | yes |
| Model record: +272 (mesh offset) | 1628 | 93,236 | no, follows from the preview size |
| Model record: bytes 280..679 | zero | zero | yes |
| Front block | one all-zero 24-byte descriptor | same bytes, at the new offset | yes |
| Mesh | 6014 triangles, 216,504 B | the same 216,504 bytes, identical | yes |
| Anything after the mesh | nothing | nothing (no footer, no trailer) | yes |

Facts that follow:

- The file grew by 92,080 − 472 = 91,608 B, which is exactly the preview. Nothing else was added.
- Vertex order, triangle order, triangle count and vertex values are unchanged. ChituBox did not re-centre the mesh and did not change translate X, Y or Z. The 65° tilt and the 5 mm lift survived a load and a save.
- Section 14 says only the first vertex of a triangle can change on save. In this re-save none did. **Guess:** plinth already wrote the form ChituBox writes, so nothing needed to change.
- ChituBox did not add presets, an attr array, a support index, a group header or a trailer, because the model has no supports. This test says nothing about those parts.
- The re-saved preview is canonical (greedy runs, longest 4095): the reader accepts it and rewrites it byte for byte, as for every other native file. Both files have no anomalies in the checker's `dump`.

### 18.3 What plinth's writer should do

- **Preview: change nothing for acceptance.** ChituBox opens and slices a flat, wrong-scene preview. The writer does not need to render a scene preview for the file to load. A real preview is for the owner's file browser and nothing else is known to depend on it. **Guess:** the 800 × 600 solid image is safe for model-only files. Whether an empty preview (`f12 = 0`, no data) is safe is still open (section 15, Q3). Keep the non-empty solid preview, because the old project saw an empty one refused (2026-08-03).
- **Preview size: do not copy.** 1298 × 1140 matches nothing in the file. The samples show sizes from 650 × 1945 to 3154 × 2250. **Guess:** ChituBox sizes it from the 3D view window at save time.
- **Header, offsets, model record, front block, mesh: nothing to change.** Plinth's output already equals ChituBox's, byte for byte, apart from the preview and the offsets that depend on it. This covers the model name with brackets, the name length, translate with a nonzero Y, and Z = 5.
- **Translate Y of 1.3747296:** both files carry it, and ChituBox kept it. Plinth's choice of Y is accepted. It does not answer section 15 Q4 or Q12 (what Z and Y mean).
- **Supports: no evidence.** Plinth's supported output still needs its own load test (sections 15, Q1, Q2, Q5, Q6).

### 18.4 Questions this answers

- Q3 (preview): partly. A written, flat, wrong-scene preview opens and slices. ChituBox renders a new preview on each save. Empty preview still open.
- Q16 (front block rebuilt on save): partly. For an unsupported model the re-saved front block equals the input. Whether ChituBox rebuilds it from state or copies it is not known, and nothing is known for supported models.
- Section 3 "whether ChituBox needs the preview to match the scene": no, it does not (for opening and slicing).

## 19. Support part shapes that `plinth-emit` builds (step 10)

**Verified from samples** by `crates/emit/tests/it/native_shapes.rs`, which re-measures every number below on each run. Nothing here says ChituBox accepts a file that plinth wrote with these parts: no load test with supports has been done (section 15, Q1, Q2, Q5, Q6).

### 19.1 What was compared

`plinth_emit::shapes::build` takes the parameters of one part and returns the 72-byte record fields and the part's mesh. The test reads the parameters out of each native record (its own start, end, r1, r2 and tail word 0; for a raft part also the top z, which only the mesh holds), builds the part, and compares it with the native record:

- the record fields (type, start, end, r1, r2, six tail words), bit for bit;
- every mesh coordinate, in triangle order, so winding is compared too.

It covers every support record of every supported model in samples 03, 04-connected, 04-hex, 05 (both plates) and 07: 654 records. Records shared by several samples count once per sample. A second test swaps the built records into the model, writes the project, and compares the file with the native one: same length; same bytes before the support-mesh blob and after it (support index, group header, all 72-byte records with their absolute mesh offsets, trailer); blob floats within the tolerance below.

### 19.2 How each part is built

All support data is in the model-local frame (section 13, item 18).

- **Rotation.** Every mesh is built for an axis pointing straight down (-z) and then turned onto the real axis by the shortest-arc rotation from -z (Qt's `QQuaternion::rotationTo`, single precision). For an axis that points straight up, the turn is a half turn about y. This reproduces the ring order of all 653 reproducible records. A tilted part therefore has ring vertex 0 on the side the shortest-arc turn gives, not on a fixed world direction.
- **Frustum (types 1 to 5).** Two rings of 16 vertices at 22.5 degrees x j, counter-clockwise from +x in the straight-down frame; start ring radius r1, end ring radius r2. Triangles: for j = 0 to 15, `[S_j, E_j, E_(j+1)]` then `[S_j, E_(j+1), S_(j+1)]` (32), then 14 start-cap triangles, then 14 end-cap triangles. The cap triangle order is a table; it is the same in all 326 frustum records of the samples, and the end cap is the start cap with ring index i replaced by (16 - i) mod 16. ChituBox's triangulator made the table and no rule for it is known.
- **Type 5 (default-mat pad).** The record's start is the top and its end the bottom, but the mesh is built from the end (ring radius r1 = 3.64) up to the start (ring radius r2 = 5.0). Seen from the record fields, the mesh is wide at the top (section 10.1).
- **Ball (types 7, 8, 9).** The two poles are the record's start and end, bit for bit. Centre = (start + end) / 2, radius = r1 (= r2). 15 rings at 11.25 degrees x k from the start pole, 16 vertices each at 22.5 degrees x j. Triangles: 16 start-pole fan `[P, R1_j, R1_(j+1)]`, 14 bands of 32 (`[Rk_j, R(k+1)_j, R(k+1)_(j+1)]`, `[Rk_j, R(k+1)_(j+1), Rk_(j+1)]`), 16 end-pole fan `[R15_j, Q, R15_(j+1)]`. Frame as above, with the start pole at +z in the straight-down frame.
- **Raft pad (type 6, tail `[0, 4.0]`).** 20-sided prism. Rim radius r1 / 2, rim vertex i at 18 degrees x i from +x. Top (at z_top) a fan from the top centre, then the bottom (z_top - r2) a fan with reversed order, then 40 side triangles. The record stores start = end = (x, y, 0), r1 = diameter, r2 = thickness. The real z exists only in the mesh.
- **Raft bridge (type 6, tail `[1.0, 4.0]`).** Box between the two pad centres, half width r1 / 2 to each side, flat ends at the centres, 12 triangles. The record stores the two centres at z = 0, r1 = width, r2 = thickness.
- **Tail words.** Type 1: word 0 is the contact value (a parameter; its meaning is still a guess, section 10.1). Type 2: 0.15. Type 6: word 1 is 4.0, word 0 is 0.0 for a pad and 1.0 for a bridge. All other words are 0.
- **Attr for the manual-support family reproduced here (`03`–`07`).** `kind << 8 | preset_id << 2`; raft records 0x1e00; a brace between two kinds `high << 32 | low` (`shapes::attr`, `attr_pair`). This builder is not a decoder for the Magic-supports `0x0c01` value (section 8.3).

### 19.3 Accuracy, and why the tolerance is what it is

Record fields: bit for bit in every reproducible record. Meshes:

- 91.8 percent of all mesh coordinates (1,165,451 of 1,269,612) match bit for bit; 169 of 653 meshes match in every coordinate.
- Every mesh is within **1.5 ULP** of the native one, where one ULP is the spacing of f32 at the part's largest coordinate (the errors are measured against that coordinate, not each one, because a native vertex is the f32 sum "part position + rotated offset" and so carries the rounding of the position even when its own value is near 0). The stated tolerance is **2 ULP**.
- Why not exact: the native vertices come out of single-precision arithmetic whose exact order of operations is not known. Several orders of the quaternion product, the normalisation and the ring sine and cosine were tried; the best matched about 83 percent of ring vertices, and none matched all.
- Why 2 ULP is acceptable: for coordinates below 32 mm one ULP is at most 1.9e-6 mm, so 2 ULP is under 4 nanometres, about 25,000 times finer than the smallest limit the checker applies (H9, 0.1 mm). It is the measured maximum (1.5) plus half a step; a wrong shape is far outside it (a ball radius off by 0.01 mm, an end point off by 1e-4 mm, swapped radii and a wrong type all fail the sabotage test).

### 19.4 Result by support kind

Records are summed over the six supported models of the five samples.

| Kind (record type) | Records | Fields exact | Mesh exact (all coords) | Mesh within 2 ULP | Max error (ULP) | Status |
|---|---|---|---|---|---|---|
| Tip ball (7) | 97 | 97 | 0 | 97 | 1.0 | reproduced within tolerance |
| Tip cone (1) | 97 | 97 | 6 | 97 | 1.0 | reproduced within tolerance |
| Joint ball (9) | 137 | 137 | 33 | 137 | 1.0 | reproduced within tolerance |
| Rod: pillar, link, leg, brace (3) | 131 | 131 | 44 | 131 | 1.5 | reproduced within tolerance |
| Base flare (4) | 62 | 62 | 50 | 62 | 0.5 | reproduced within tolerance |
| Default-mat pad (5) | 10 | 10 | 0 | 10 | 1.0 | reproduced within tolerance |
| Foot cone (2) | 26 | 26 | 5 | 26 | 1.0 | reproduced within tolerance |
| Foot ball (8) | 11 | 11 | 0 | 11 | 1.0 | reproduced within tolerance |
| Raft pad (6) | 43 | 43 | 1 | 43 | 1.0 | reproduced within tolerance |
| Raft bridge (6) | 39 | 39 | 30 | 39 | 0.5 | reproduced within tolerance |
| Hex raft mat (6) | 1 | not built | not built | not built | - | **unverified**: the builder refuses it |

No kind is reproduced fully bit for bit across all its records.

### 19.5 What is not established

- **Hex raft mat: unverified.** One native record (2,772 triangles, hole layout, start and end of -1e8 and -1e9). No rule for the holes, the 3.1 / 4.2 mm margin or the sentinels is known, so `build(&Spec::HexMat)` returns `ShapeError::Unverified` and `native_unverified_hex_raft_mat_is_refused_not_guessed` checks that it does and prints a `PLINTH-UNVERIFIED:` line, which `scripts/t` counts as `unverified 1` on its verdict line (design section 6.2). It is never built from a guess.
- **Sizes never varied in the samples:** pad diameter 2.2, thickness 0.2, bridge width 2.2, 16 and 20 sides. The builder takes other values, but nothing checks them against a native file. Neither is the sphere ring count (15) or any radius outside those in sections 10.3 to 10.6.
- **Only the shape is built here.** Not part of step 10 and not tested against the samples: how parts are placed (heads, bases, the 0.002 mm overlaps, the 0.6 mm flare shift under a raft, what ends a foot over a hex hole), which bridges join which pads (section 11.2 gives an inferred rule), brace placement and the attr kind code of a given UI action.
- **Not a load test.** Whether ChituBox opens and slices a file whose support meshes plinth wrote is open (H8; section 15).
- **Older native files** (the seven in `test-data/chitubox-native/`, version unknown) are not part of this comparison.
