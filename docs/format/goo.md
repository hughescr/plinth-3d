# Elegoo GOO reader

The read-only CAL-1 reader is `crates/calibration/src/goo.rs`. The measurement
command is `cal1-goo READ_ONLY_INPUT NEW_OUTPUT`; it refuses an existing output
directory. It does not change projects or slices. This is not a slicer or a GOO
writer for printing.

## Sources and scope

- [Elegoo's published GOO repository](https://github.com/elegooofficial/GOO),
  [Goo Format Spec V1.2.pdf](https://github.com/elegooofficial/GOO/blob/main/Goo%20Format%20Spec%20V1.2.pdf),
  retrieved 2026-11-01. The document revision is 1.2; the file version is `V3.0`.
  Pages 1–7 define the header, 7–9 the layer envelope, and 9–11 the RLE.
- [msla_format reference decoder](https://docs.rs/crate/msla_format/0.2.0/source/src/goo/encoded_layer.rs)
  helps disambiguate the length field: the tag holds the **low** four bits.
  The implementation here uses checked bounds and checked grey deltas instead
  of unchecked indexing/arithmetic.

Only V3.0 with the documented two fixed-size previews is accepted. This was
measured against the owner's nine CAL-1 slices, including the later W slice,
on the Saturn 4 Ultra 16K. Other printers and variants are unverified. The
reader rejects bad magic, delimiters, checksums, endings, nonfinite settings,
invalid header bounds, truncated data, and decoded pixel-count mismatches.
The exact original header, preview and layer envelopes are retained for a
lossless envelope round trip. `to_bytes` is not a semantic editing interface:
changing the public decoded header does not rewrite the retained raw header.

## Byte layout

Multibyte header/layer scalars are **big endian**, including IEEE-754 f32.
All offsets below are zero based. Strings are fixed-size NUL-padded fields.

| Offset | Size | Content |
|---:|---:|---|
| 0 | 4 | `V3.0` |
| 4 | 8 | `07 00 00 00 44 4c 50 00` |
| 12 | 32+24+24 | software, version, creation time |
| 92 | 32+32+32 | printer name, type, resin profile |
| 188 | 2+2+2 | anti-aliasing, grey-level, blur settings |
| 194 | 26912+2 | 116×116 RGB565 preview and CRLF |
| 27108 | 168200+2 | 290×290 RGB565 preview and CRLF |
| 195310 | 4+2+2 | layer count and X/Y resolution |
| 195318 | 1+1 | X/Y mirror flags |
| 195320 | 12 | X/Y/Z build dimensions, mm |
| 195332 | 4+4+1 | layer thickness, normal exposure, delay mode |
| 195341 | 32 | off time; bottom/normal before-lift, after-lift, after-retract; bottom exposure |
| 195373 | 4 | bottom layer count |
| 195377 | 64 | bottom/normal lift and retract distances/speeds, first and second stage |
| 195441 | 2+2+1 | bottom/normal PWM and advanced-mode flag |
| 195446 | 4+4+4+4+8 | print seconds, volume mm³, weight g, price, price unit |
| 195470 | 4+1+2 | layer-content offset, grey-scale flag, transition layer count |
| 195477 | variable | sequential layer records |

Each layer has a 66-byte definition: pause flag u16; pause Z, layer Z,
exposure, off time, three rest times, lift distance/speed, second-stage lift
distance/speed, retract distance/speed, second-stage retract distance/speed
(all f32); PWM u16; CRLF. A u32 encoded-data length follows. Encoded data
includes initial `55` and final checksum. CRLF follows the encoded data.
After the last layer, the exact 11-byte ending is
`00 00 00 07 00 00 00 44 4c 50 00`.

The specification's image checksum is the one's complement of the wrapping
sum of encoded payload bytes, excluding `55`. Equivalently payload plus
checksum sums to `ff` modulo 256. It is **not** the two's complement.

## Image decoding

For each tag, bits 7–6 choose black (00), explicit grey byte (01), grey delta
(10), or white (11). Except delta, bits 5–4 give the number (0–3) of additional
length bytes. They form a big-endian integer; shift it left four and OR the
tag's low nibble. For explicit grey, the value byte comes before length bytes.
A delta changes the previous grey by the low nibble, with bit 5 selecting
subtraction. Delta bit 4 selects a one-byte run length; otherwise length is 1.
Zero-length runs and out-of-range grey deltas are rejected.

Grey-scale flag 1 means 0–255. Flag 0 means 0–15 grey values, normalized by 17;
explicit white remains 255. CAL-1 uses flag 1. Decoding emits only nonzero
row spans; the sum of *all* decoded runs must equal X resolution × Y resolution.
No resolution-sized bitmap is allocated per layer.

## Measurement conventions and limits

Pixel pitch is build dimension / resolution, independently on each axis.
CAL-1 is **14 × 19 µm**, not a square-pixel approximation. Lit-any area is
`count(value > 0) × pitchX × pitchY`; weighted area is
`sum(value/255) × pitchX × pitchY`. Weighted area is an image intensity
integral, **not** a prediction of cured resin area or load-bearing strength.
The header's grey setting is not the number of distinct intensities observed:
these files contain up to all 255 nonzero values.

Layer components use 8-connectivity, treating any nonzero grey as lit. A
component is an island if it has zero exact same-XY lit-pixel overlap with
the preceding layer. First-layer components touch the plate and are excluded
from island counts. There is no dilation in the overlap test. Thus a diagonal
within-layer bridge is accepted; a merely adjacent pixel in the layer below
is not support. The output includes per-blob pixel count, weighted count,
centroid, bounding box and overlap count. This is not a cured-contact,
minimum-load-area, or physical-printability check.

For these owner files, measured plan-view coordinates use raster row-major
X/Y with mesh-local XY plus the project's XY translation. Positive mesh X
and Y both run toward smaller raster coordinates. Continuous pixel-centre
coordinates are approximately `u = Xres/2 + 0.5 - worldX/pitchX` and
`v = Yres/2 + 0.5 - worldY/pitchY`; integer indices are their containing cells.
The negative X agrees with mirror X=1; the negative Y is the image row
convention, not mirror Y=1 (that flag is 0). W's asymmetric thin/thick ends
and A's off-centre contacts establish the signs; symmetrical coupons alone
cannot. Mesh-to-raster edge agreement is limited by rasterization and AA.

GOO layer Z is reported unchanged, as a layer's upper height. For CAL-1,
first coupon body pixels appear at label 5.05 mm, not label 5.00 mm. The
PDF does not specify ChituBox's exact plane used inside each layer slab.
Do not silently substitute a mesh station or a half-layer offset for this
measurement. Station comparisons must state which labelled layers were used.

The CAL-1 command also reads the two A1 projects with the existing ChituBox
reader. It compares every typed scalar by f32 bits, every stored triangle
coordinate by bits, record counts, preset entries, attrs, model names,
translations, preview and header. It reports pointer/length changes through
the parsed structure rather than pretending different file offsets are
support-field edits.

## A1b written-mesh midpoint witness

`crates/calibration/src/goo_grid.rs` intersects the written triangles with
all midpoint planes on the GOO grid. It batches the triangle/plane work,
rasterizes nonzero winding, unions row runs, then uses the same component
and exact-overlap check as the GOO reader. Two boundary rules are checked:
pixel centres inside the section, and any pixel cell touching its boundary
or interior. This is not a full AA or image-blur model.

The original A1's first low ball cap (support record 4, paired cone 5) starts
at label 3.10 mm, midpoint 3.075 mm. The thin inclined cone leaves it outside
the supported cross-section. The A0 cone covers it. The cell-touching witness
reproduces its layer and bounding box, but counts eight pixels rather than
ChituBox's six. The centre-only rule additionally flags the other paired
contact (cone 3). This disagreement is retained, not called exact emulation.

The CAL-1 builder selects a canonical cone from a bounded radius list starting
at 0.052 mm. It emits that cone only when its ball/cone pair has no new island
under **both** rules; the lower end is carried by the unchanged lower support.
For this owner pose that selects r1=0.070 mm at cone 3, 0.100 mm at cone 5,
and 0.052 mm at the other eight. No endpoint, ball, r2 or other field changes.
The command `cal1-a1b INPUT NEW_OUTPUT` writes and re-reads the new project,
compares it field-by-field against A0, and checks all 525 layers of the complete
body and supports. Both rules give zero A1b islands. Its report preserves the
A0/A1 baseline failures and per-part onset slices. **ChituBox must still slice
A1b to confirm continuity and measure its C4 horizontal-area qualification.**

Tests include the exact record-5 frame as a regression: restoring the unsafe
0.052 mm cone must reproduce its midpoint island. Independent checker results
remain separate; eliminating this island does not waive lift or branch-thickness
violations, and does not approve a print.

## Reproduction and tests

Build using `scripts/t Lnoop` (its no-change assertion requires an already
built tree), then run the built `target/debug/cal1-goo` with an input directory
and a **new** output directory. Run lint with `scripts/t L0 calibration` and
unit tests with `scripts/t L1 calibration`. For the optional external owner
fixtures, set `CAL1_GOO_INPUT` to their read-only folder before the test command.
Without that variable only the synthetic tests exercise the reader; the
external-fixture test returns without reading files.

Tests cover known absolute/delta RLE, the published length-nibble ordering,
checksum corruption, a wrong decoded pixel count, component connectivity and
island detection. The owner-fixture test checks all nine files' known
resolution and layer thickness, lossless envelope round trips, and decoding
every layer. This does not prove the pixel coordinates against an independent
printer or that a lit pixel cures. Those are separate consumer/physical checks.
