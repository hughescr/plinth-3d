# Calibration plate 6 — support-arm capacity: results record

**This file is the record of record for plate 6.** It supersedes the interim
record in [`../rust-migration/03-physics-and-constants.md`](../rust-migration/03-physics-and-constants.md)
§4, which was written first and which says so itself. Where the two differ,
this file governs and the difference is flagged below (§9).

Everything a future reader needs is *in this file*. The plate was designed and
scored out of a separate `calibration-coupons` working folder holding
`manifest.json`, `PLATE-LAYOUT.md` and the generator; **that folder is
condemned and will not exist.** Every geometry number below was extracted from
`manifest.json` before deletion and is reproduced here verbatim. Do not write a
future document that cites that folder.

---

## 1. Provenance

| Field | Value |
|---|---|
| Plate | **COUPON-PLATE-6-support-arm-capacity** |
| Build md5 | **`2fff011d1cfee56ac0b49b3e759e5a60`** (`COUPON-PLATE-6-support-arm-capacity.chitubox`) |
| Printed | **2026-07-30** |
| Scored | **2026-07-31**, by the owner, after wash / dry / cure |
| Printer | **Elegoo Saturn 4 Ultra** — tilt release; the peel front sweeps **−Y → +Y**, so the front (−Y) edge of the plate travels furthest |
| Resin | **black** (vendor not recorded at print time — a gap; see §8) |
| Ambient | **≈ 30 °C** |
| Layer height | **0.050 mm** |
| Specimens | **34** measuring specimens in two families (A6 = 17, B6 = 17), plus 9 ballast columns that score nothing |
| Reading | strictly binary per specimen — **cap present** (a disc or strip sitting on top) / **cap gone** (a bare stalk on a plinth) |
| Scoring integrity | **the wash basket was empty.** Owner, 2026-07-31: *"The wash basket is empty - everything that failed, failed in printing not in the wash."* Every failure below is an **in-print** failure. |
| Verification | adversarial panel: **two blind analysts plus a refuter**, 2026-07-31. Memos preserved as the three appendices in §10. |

**Photographic backing.** Trains 1, 4 and 6 were photographed and 18 of the 34
specimens were spot-checked against the photos: `KS` confirmed as a bare stub
(the must-survive control genuinely failed in print), `L1` confirmed as a
headless stalk, and `M3` / `2WY` / `3TX` confirmed with caps intact.
**Trains 2, 3 and 5 were not photographed** — every outcome in those trains,
including `3TY` (a headline cell) and `KL` (the surviving validity control),
rests on a naked-eye read alone. Re-photographing trains 3 and 5 is an open
follow-up (§8).

---

## 2. What the plate was for

Verbatim from the plate's own pre-registration (`PLATE-LAYOUT.md` §plate 6):

> Plate 5 put the single-arm capacity boundary near `X = A_cap / A_arm ~= 25`,
> but on CONICAL support tips — the breaks were observed at the base of the
> cylindrical portion, wherever the cone happened to be thinnest, so the
> failing cross-section was never quite the nominal tip diameter, and the 2.5x
> ladder step could only say "somewhere near 25". Plate 6 re-measures the same
> boundary on CONSTANT-DIAMETER CYLINDERS, so the failing section is
> unambiguous and always equal to the nominal, stepping the ladder at ~1.25x
> instead of 2.5x (**family A6**). One further question rides the same build:
> does N support arms buy N times the capacity of one, and does the arm line's
> orientation to the peel front matter (**family B6**).

Two families, both scored on the same plate so that neither could be confounded
with row position:

- **A6** — one flat disc cap on **one** constant-diameter cylindrical arm,
  **6.00 mm** free length, at two cap sizes (`S` = D 4.997 mm / 19.5554 mm²,
  `L` = D 12.502 mm / 122.4075 mm² — a 6.26× change in absolute load at the
  same ratio) so the boundary could be shown to be the **ratio** and not the
  absolute size. `X_ratio = (D_cap/d_arm)²` exactly, since cap and arm are the
  same 48-gon.
- **B6** — the **same 30.00 mm² cap** on **N** constant-diameter 0.798 mm arms
  in a line, **4.00 mm** free length, with N, arm pitch (2.0 mm = `T`,
  2.8 mm = `W`) and the arm line's orientation to the peel front (`X` / `Y`)
  crossed. `M1`–`M4` put the same cap on **one** round arm as the matched
  single-arm calibration.
  - `X_tot = A_cap / (N · A_arm)` — what N arms sharing the load evenly see.
  - `X_arm = A_cap / A_arm` — what **one** arm alone sees if the cap shares
    nothing (the **zipper** failure). Every strip arm sits at
    `X_arm = 60.15`, which is `M1`'s own `X_tot`, so the zipper prediction is
    not a model output: *"M1 fails, therefore every strip cell must fail too."*

Two design facts that constrain how the plate can be read, both quoted from the
pre-registration:

> Each cap reaches full diameter through a **flare** at 0.20 mm per side per
> 0.050 mm layer — the rate plate 5's TIP family printed successfully — not
> through a single-layer step. That does not move the measured boundary: the
> arm cylinder is still the minimum section at every height on the load path,
> so peak arm stress is `p_eff * A_cap / A_arm` however the load gets there.

> **Do not try to read anything from the break HEIGHT.** The arm is the minimum
> section everywhere, so the stub left on the plate is a bare full-length
> cylinder no matter when it broke; the informative truncated cone leaves with
> the cap and is lost in the vat. The reading is strictly binary.

**Family C6 (arm free length) was designed but dropped from the build.** Nothing
on the plate carries a C6 ID; every A6 specimen uses the same fixed 6.00 mm arm
free length. Free length is **not** a variable anywhere on this plate — which is
exactly why §7 cannot resolve the A6 anomaly.

### Build facts worth preserving

- 34 measuring specimens, 9 ballast columns (of 50 candidate slots), 0 corner
  fiducials; **45 034 triangles**; tallest point 13.68 mm = 274 layers
  (specimens alone reach 12.70 mm).
- Per-layer cured area over z = 4.50…12.70 mm: **256…755 mm²**.
- Per-layer cured area **at the critical layers** — the layers where a
  specimen's own failure section is being peeled, the only part of the profile
  that can bias a ladder: **A6 704…755 mm² over 43 layers (spread 6.9 %)**;
  **B6 658…744 mm² over 30 layers (spread 11.7 %)**. The ladders were not
  reading a cured-area gradient.
- Three rows front-to-back at **Y = −37.50 / 0.00 / +37.50 mm**, each row
  carrying **both families mixed**, so family is never a function of Y.
- Each row is split into two glued **trains** (six total), each under the 78 mm
  wash-station ceiling; a low web joins neighbours **within** a train only, so a
  train lifts off the plate as one piece.

| train | row | Y (mm) | X range (mm) | n | A6 | B6 | photographed |
|---|---|---|---|---|---|---|---|
| 1 | R0 | −37.50 | −67.99 … −1.19 | 6 | 4 | 2 | **yes** |
| 2 | R0 | −37.50 | +14.33 … +66.42 | 6 | 2 | 4 | no |
| 3 | R1 | +0.00 | −72.24 … −11.06 | 5 | 1 | 4 | no |
| 4 | R1 | +0.00 | +0.32 … +66.68 | 7 | 5 | 2 | **yes** |
| 5 | R2 | +37.50 | −60.68 … −7.17 | 5 | 3 | 2 | no |
| 6 | R2 | +37.50 | +4.63 … +58.10 | 5 | 2 | 3 | **yes** |

---

## 3. Pre-registered validity gates — verbatim, and how each fired

The gates below are quoted **exactly** as they stood in `PLATE-LAYOUT.md`
before the plate was printed. All controls were blind: nothing on the plate or
on the scoring sheet marked them.

### Gate A6-1 (must survive)

> `KS`/`KL` (X ~= 8) MUST SURVIVE — if either is gone, printing or handling
> destroyed the plate and nothing else on it can be trusted.

**FIRED.** `KS` (X = 7.997) is **GONE**; the photograph of train 4 position 3
shows a bare stub with the `KS` glyph legible on the apron. `KL` (X = 7.975)
survived. Read literally the consequent is *plate-wide* — "nothing else **on
it**" — so as written this gate voids **both** families. See the override
record, §6.

### Gate A6-2 (must fail)

> `ZS`/`ZL` (X ~= 400) MUST FAIL — if either survives, the boundary sits above
> this whole ladder and none of the rungs below bracket it.

**SATISFIED.** `ZS` (X = 409.284) GONE and `ZL` (X = 397.579) GONE.

### Gate B6-1 (blind positive control)

> `PB` is a BLIND positive control (`3TY`'s footprint on 0.247 mm arms,
> `X_tot` = 209) and MUST FAIL; do not tell the scorer which cell it is before
> scoring.

**SATISFIED.** `PB` (X_tot = 209.294, X_arm = 627.883) GONE. The refuter's
caveat travels with it: `PB` fails under *every* candidate model on the table
(group ratio, per-arm ratio, total arm area, slenderness, or simple
non-formation), so it discriminates none of them — it establishes only that the
scorer did not tick a bare plinth. The train-1 photograph shows `PB`'s plinth
with **no arms visible at all**, consistent with 0.247 mm arms never forming.

### Gate B6-2 (partial caps)

> **Any PARTIAL cap on ANY B6 specimen is scored VOID, never a survival** —
> especially on the headline contrast, whose entire value depends on a clean
> present/gone read.

**NOT TRIGGERED** on any of the 18 specimens visible in photographs. It could
not be checked on trains 2, 3 and 5, which were not photographed.

### Gate — handling rider

> **Handling rider — read before touching the plate.** The owner scores after
> washing, drying and curing — never on the still-wet green part […]. That means
> Family A6's thin-arm survivors, down to a 0.65 mm arm at roughly 3 g of break
> force, must survive the wash basket intact for the reading to mean anything: a
> wash-basket loss reads as a false failure at precisely the boundary the plate
> exists to measure. Handle post-wash trains gently, one at a time, and check
> the wash basket itself for any detached caps before concluding a count.

**SATISFIED, though out of order.** The protocol analyst correctly flagged that
the count was concluded before any basket inspection was reported. The owner
then inspected: **the basket was empty.** That closes the rider and is what
licenses "every failure was an in-print failure" everywhere in this document.

---

## 4. The full specimen table — all 34, as built and as scored

Every column is straight from `manifest.json` for the plate key
`COUPON-PLATE-6-support-arm-capacity`; `x`/`y` are the actual pixel-snapped
build positions, not design targets. `train`/`pos` is the physical train and
the specimen's position within it counted left-to-right in increasing X, which
is the order the owner scored in.

For A6 (N = 1) the two X columns are equal by construction and both hold
`X_ratio`. For B6, `X_tot = A_cap/(N·A_arm)` and `X_arm = A_cap/A_arm`.
`z_cap` is the cap plane: underside → top of the disc/strip.

| id | train | pos | fam | role | N | axis | pitch | d_arm | cap area | X_tot | X_arm | arm L | z_cap0 | z_cap1 | row | x | y | outcome |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `S8` | 1 | 1 | A6 | ladder r8 | 1 | — | — | 0.722 | 19.5554 | 47.901 | 47.901 | 6.0 | 10.95 | 11.45 | 0 | -67.987 | -37.5 | **GONE** |
| `S1` | 1 | 2 | A6 | ladder r1 | 1 | — | — | 1.577 | 19.5554 | 10.04 | 10.04 | 6.0 | 10.95 | 11.45 | 0 | -58.99 | -37.5 | **GONE** |
| `M3` | 1 | 3 | B6 | mono | 1 | — | — | 1.387 | 29.9978 | 19.911 | 19.911 | 4.0 | 8.5 | 9.0 | 0 | -48.197 | -37.5 | **PRESENT** |
| `PB` | 1 | 4 | B6 | BLIND MUST FAIL | 3 | Y | 2.0 | 0.247 | 30.0 | 209.294 | 627.883 | 4.0 | 8.5 | 9.0 | 0 | -36.024 | -37.5 | **GONE** |
| `L7` | 1 | 5 | A6 | ladder r7 | 1 | — | — | 1.995 | 122.4075 | 39.271 | 39.271 | 6.0 | 12.0 | 12.5 | 0 | -20.494 | -37.5 | **GONE** |
| `L1` | 1 | 6 | A6 | ladder r1 | 1 | — | — | 3.952 | 122.4075 | 10.007 | 10.007 | 6.0 | 12.0 | 12.5 | 0 | -1.192 | -37.5 | **GONE** |
| `6TY` | 2 | 1 | B6 | strip | 6 | Y | 2.0 | 0.798 | 30.0 | 10.026 | 60.154 | 4.0 | 8.5 | 9.0 | 0 | 14.326 | -37.5 | **PRESENT** |
| `3WX` | 2 | 2 | B6 | strip | 3 | X | 2.8 | 0.798 | 30.0 | 20.051 | 60.154 | 4.0 | 8.5 | 9.0 | 0 | 25.994 | -37.5 | **PRESENT** |
| `S6` | 2 | 3 | A6 | ladder r6 | 1 | — | — | 0.893 | 19.5554 | 31.312 | 31.312 | 6.0 | 10.95 | 11.45 | 0 | 37.292 | -37.5 | **GONE** |
| `ZS` | 2 | 4 | A6 | MUST FAIL | 1 | — | — | 0.247 | 19.5554 | 409.284 | 409.284 | 6.0 | 10.95 | 11.45 | 0 | 46.289 | -37.5 | **GONE** |
| `4TY` | 2 | 5 | B6 | strip | 4 | Y | 2.0 | 0.798 | 30.0 | 15.039 | 60.154 | 4.0 | 8.5 | 9.0 | 0 | 55.961 | -37.5 | **PRESENT** |
| `3WY` | 2 | 6 | B6 | strip | 3 | Y | 2.8 | 0.798 | 30.0 | 20.051 | 60.154 | 4.0 | 8.5 | 9.0 | 0 | 66.418 | -37.5 | **PRESENT** |
| `4WY` | 3 | 1 | B6 | strip | 4 | Y | 2.8 | 0.798 | 30.0 | 15.039 | 60.154 | 4.0 | 8.5 | 9.0 | 1 | -72.242 | 0.0 | **PRESENT** |
| `2TY` | 3 | 2 | B6 | strip | 2 | Y | 2.0 | 0.798 | 30.0 | 30.077 | 60.154 | 4.0 | 8.5 | 9.0 | 1 | -60.008 | 0.0 | **PRESENT** |
| `4WX` | 3 | 3 | B6 | strip | 4 | X | 2.8 | 0.798 | 30.0 | 15.039 | 60.154 | 4.0 | 8.5 | 9.0 | 1 | -44.441 | 0.0 | **PRESENT** |
| `ZL` | 3 | 4 | A6 | MUST FAIL | 1 | — | — | 0.627 | 122.4075 | 397.579 | 397.579 | 6.0 | 12.0 | 12.5 | 1 | -26.59 | 0.0 | **GONE** |
| `3TY` | 3 | 5 | B6 | strip | 3 | Y | 2.0 | 0.798 | 30.0 | 20.051 | 60.154 | 4.0 | 8.5 | 9.0 | 1 | -11.061 | 0.0 | **PRESENT** |
| `S4` | 4 | 1 | A6 | ladder r4 | 1 | — | — | 1.121 | 19.5554 | 19.87 | 19.87 | 6.0 | 10.95 | 11.45 | 1 | 0.316 | 0.0 | **GONE** |
| `S9` | 4 | 2 | A6 | ladder r9 | 1 | — | — | 0.646 | 19.5554 | 59.835 | 59.835 | 6.0 | 10.95 | 11.45 | 1 | 9.313 | 0.0 | **GONE** |
| `KS` | 4 | 3 | A6 | MUST SURVIVE | 1 | — | — | 1.767 | 19.5554 | 7.997 | 7.997 | 6.0 | 10.95 | 11.45 | 1 | 18.31 | 0.0 | **GONE** |
| `2WY` | 4 | 4 | B6 | strip | 2 | Y | 2.8 | 0.798 | 30.0 | 30.077 | 60.154 | 4.0 | 8.5 | 9.0 | 1 | 29.418 | 0.0 | **PRESENT** |
| `3TX` | 4 | 5 | B6 | strip | 3 | X | 2.0 | 0.798 | 30.0 | 20.051 | 60.154 | 4.0 | 8.5 | 9.0 | 1 | 42.027 | 0.0 | **PRESENT** |
| `S2` | 4 | 6 | A6 | ladder r2 | 1 | — | — | 1.406 | 19.5554 | 12.631 | 12.631 | 6.0 | 10.95 | 11.45 | 1 | 52.525 | 0.0 | **GONE** |
| `L5` | 4 | 7 | A6 | ladder r5 | 1 | — | — | 2.508 | 122.4075 | 24.849 | 24.849 | 6.0 | 12.0 | 12.5 | 1 | 66.675 | 0.0 | **GONE** |
| `S3` | 5 | 1 | A6 | ladder r3 | 1 | — | — | 1.254 | 19.5554 | 15.879 | 15.879 | 6.0 | 10.95 | 11.45 | 2 | -60.682 | 37.5 | **GONE** |
| `3WX2` | 5 | 2 | B6 | strip | 3 | X | 2.8 | 0.798 | 30.0 | 20.051 | 60.154 | 4.0 | 8.5 | 9.0 | 2 | -49.383 | 37.5 | **PRESENT** |
| `KL` | 5 | 3 | A6 | MUST SURVIVE | 1 | — | — | 4.427 | 122.4075 | 7.975 | 7.975 | 6.0 | 12.0 | 12.5 | 2 | -32.932 | 37.5 | **PRESENT** |
| `3WY2` | 5 | 4 | B6 | strip | 3 | Y | 2.8 | 0.798 | 30.0 | 20.051 | 60.154 | 4.0 | 8.5 | 9.0 | 2 | -17.474 | 37.5 | **PRESENT** |
| `S5` | 5 | 5 | A6 | ladder r5 | 1 | — | — | 1.007 | 19.5554 | 24.624 | 24.624 | 6.0 | 10.95 | 11.45 | 2 | -7.168 | 37.5 | **GONE** |
| `M4` | 6 | 1 | B6 | mono | 1 | — | — | 1.596 | 29.9978 | 15.037 | 15.037 | 4.0 | 8.5 | 9.0 | 2 | 4.625 | 37.5 | **PRESENT** |
| `M1` | 6 | 2 | B6 | mono | 1 | — | — | 0.798 | 29.9978 | 60.15 | 60.15 | 4.0 | 8.5 | 9.0 | 2 | 17.214 | 37.5 | **GONE** |
| `L3` | 6 | 3 | A6 | ladder r3 | 1 | — | — | 3.116 | 122.4075 | 16.098 | 16.098 | 6.0 | 12.0 | 12.5 | 2 | 33.16 | 37.5 | **PRESENT** |
| `S7` | 6 | 4 | A6 | ladder r7 | 1 | — | — | 0.798 | 19.5554 | 39.211 | 39.211 | 6.0 | 10.95 | 11.45 | 2 | 47.309 | 37.5 | **GONE** |
| `M2` | 6 | 5 | B6 | mono | 1 | — | — | 1.121 | 29.9978 | 30.481 | 30.481 | 4.0 | 8.5 | 9.0 | 2 | 58.102 | 37.5 | **GONE** |

Units: `d_arm` mm, cap area mm², arm L (free length) mm, `z_cap*` mm, `x`/`y` mm,
pitch mm. `X_tot` / `X_arm` are dimensionless ratios.

Arm cross-sections as built, straight from the manifest. Arms are 48-gons
inscribed in `d_arm`, so each is very slightly below `π d_arm²/4`:
0.247 → 0.047780; 0.627 → 0.307882; 0.646 → 0.326824; 0.722 → 0.408247;
0.798 → 0.498718; 0.893 → 0.624528; 1.007 → 0.794160; 1.121 → 0.984147;
1.254 → 1.231527; 1.387 → 1.506613; 1.406 → 1.548173; 1.577 → 1.947656;
1.596 → 1.994870; 1.767 → 2.445243; 1.995 → 3.116985; 2.508 → 4.926109;
3.116 → 7.604030; 3.952 → 12.231587; 4.427 → 15.348572 mm².

### 4.1 The owner's scoring, verbatim

Recorded exactly as read, train by train, on 2026-07-31. The owner read some
IDs off the embossed aprons with glyph substitutions; the mapping used above is
`5x → Sx`, `5B → S8`, `K5 → KS`, `Z5 → ZS`, `3WX3 → 3WX2`, and each is
unambiguous given the train roster.

- **Train 1** (`S8`, `S1`, `M3`, `PB`, `L7`, `L1`): *"only M3 survived"*
- **Train 6-M** (`M4`, `M1`, `L3`, `S7`, `M2`): *"M4 and L3 survived, rest failed"*
- **Train 5** (`S3`, `3WX2`, `KL`, `3WY2`, `S5`): *"3WX3, KL, and 3WY2 survived, rest failed"*
- **Train 2** (`6TY`, `3WX`, `S6`, `ZS`, `4TY`, `3WY`): *"56 and Z5 failed, the rest survived"*
- **Train 4** (`S4`, `S9`, `KS`, `2WY`, `3TX`, `S2`, `L5`): *"2WY and 3TX survived, rest failed"*
- **Train 3** (`4WY`, `2TY`, `4WX`, `ZL`, `3TY`): *"ZL failed, rest survived"*
- Later: *"The wash basket is empty - everything that failed, failed in printing not in the wash."*

### 4.2 Outcome summary

| group | n | cap present | cap gone |
|---|---|---|---|
| **All specimens** | 34 | **16** | **18** |
| **B6** (4.00 mm arms, caps at z 8.50–9.00) | 17 | **14** | 3 (`PB`, `M1`, `M2`) |
| B6 strips at `X_arm` = 60.15 | 12 | **12** | 0 |
| B6 monos (`M1`–`M4`) | 4 | 2 (`M3`, `M4`) | 2 (`M1`, `M2`) |
| **A6** (6.00 mm arms, caps at z 10.95–12.70) | 17 | 2 (`KL`, `L3`) | **15** |
| A6 cap-**S** (19.5554 mm²) | 11 | **0** | **11** |
| A6 cap-**L** (122.4075 mm²) | 6 | 2 (`KL`, `L3`) | 4 |

Ordered by X, the two families could not look less alike:

| family | alive at X | dead at X |
|---|---|---|
| B6 mono | 15.04, 19.91 | 30.48, 60.15 |
| B6 strip (`X_tot`) | 10.03, 15.04 ×3, 20.05 ×6, 30.08 ×2 | 209.29 (`PB`, and 0.247 mm arms) |
| A6 cap-S | *(none)* | 7.997, 12.63, 15.88, 19.87, 24.62, 31.31, 39.21, 47.90, 59.84, 409.28 |
| A6 cap-L | 7.975, 16.10 | 10.01, 24.85, 39.27, 397.58 |

B6 fits a single threshold with zero anomalies. A6 cap-S is an invariant wipe
across a **51× load range**, and A6 cap-L is **non-monotone** — the easiest rung
(`L1`, X = 10.01, a 3.952 mm arm) died while a harder rung (`L3`, X = 16.10,
a 3.116 mm arm) lived. That is not a capacity ladder. See §5 and §7.

---

## 5. Findings

Numbered as they were agreed by the verification panel. **Every caveat here is
load-bearing; do not quote a finding without them.**

### Finding 1 — Load sharing is AT LEAST additive; the zipper model is refuted

All **12** strip cells survived at per-arm **`X_arm = 60.15`** — N ∈ {2, 3, 4, 6},
pitches 2.0 and 2.8 mm, arm lines along **both** X and Y, both replicates
included — while the matched **mono** `M1` **died at exactly the same
`X = 60.15`**. Survival tracks the **group** ratio `X_tot = A_cap/(N·A_arm)`,
not the per-arm ratio. N arms genuinely co-carry one region.
`LoadSharing::Zipper` is refuted as a description of this printer's behaviour.

Caveats, all mandatory:

1. Validated at **one** cap area (30.00 mm²), **one** arm diameter (0.798 mm)
   and **one** arm length (4.00 mm).
2. **No strip ever failed.** There is therefore **no measured upper bracket on
   sharing** — the result bounds sharing from below only. The largest group
   measured is N = 6, so `n_eff_cap = 6.0`; past that it is extrapolation.
3. Strip arms carry a **~20 % fin-root section bonus** the mono cells lack
   (generator parameter `B6_FIN_ROOT_SIDE_F = 0.97` gives every strip arm a
   square fin root of area ≈ 1.1975 × `A_arm`; `M1`–`M4` get a direct arm→cap
   flare with no such root). Treat sharing as **"at least N×"** and **never
   design on a super-additive margin** — part of the observed headroom is
   section, not sharing.
4. On this plate a single cap area was used, so `X_tot` is **algebraically
   degenerate with total arm cross-section**: all 17 B6 outcomes are fit equally
   well by "sum of arm areas ≥ ~1.0 mm²" (dead: 0.143 `PB`, 0.499 `M1`,
   0.984 `M2` mm²; alive: 0.997–2.992 mm²). The **ratio** form rests on
   **plate 5's** cap-area spread,
   not on plate 6. The A6 S-vs-L contrast that was supposed to prove
   ratio-not-size on this plate is void (§7).
5. The strips-vs-mono flip at N = 2 is uncomfortably tight: `2TY` and `2WY`
   alive at `X_tot` = 30.077 against `M2` dead at 30.481 forces any
   single-boundary additive model into a **1.33 % window**. The parsimonious
   reading is that strips are genuinely stronger per unit total arm area —
   i.e. caveat 3's section bonus — rather than that additivity is exact.

### Finding 2 — Mono cylindrical `X*` brackets (19.91, 30.48)

| cell | X | verdict |
|---|---|---|
| `M4` | 15.04 | PRESENT |
| **`M3`** | **19.911** | **PRESENT** (front row) |
| **`M2`** | **30.481** | **GONE** |
| `M1` | 60.15 | GONE |

**`X*_cylinder ∈ (19.91, 30.48)`**, geometric centre 24.6, **n = 1 at each
bracket edge**, conditional on **4.00 mm** arms and the **30 mm²** cap.
**Quote the bracket, never the centre.** This plate brackets the transition far
more loosely than plate 5's 30-specimen ladder did.

The bracket **contains** plate 5's conical `X* ∈ [25.00, 25.34]`. That is what
retires the cone-artifact worry: `X* ≈ 25` reproduces on a second, unambiguous
geometry, so the failing-section ambiguity of a cone was not what plate 5 was
measuring. The two remain **separately named constants** (`X_STAR_CONE`,
`X_STAR_CYLINDER`) because they were measured on different geometries at very
different precision, and merging them would launder that difference away.

Engine consequence: `X_ALLOWED = 12.0` keeps **1.66×** margin (19.91/12) to the
binding lower edge. That is a **margin check**, not a re-derivation — with n = 1
per edge, `X*/2` is not a defensible derivation. **No arm in the engine
resizes** because of plate 6.

Caveats: the bracket is unreplicated at both edges; `M2` (a 1.121 mm stalk under
a 6.189 mm disc) is one of the more handling-fragile items in B6, though the
empty wash basket removes the handling escape route; and the bracket is
conditional on the 4.00 mm arm length, so it is **not** a general
length-independent constant (§7's length bound is what is available instead).

### Finding 3 — Peel-axis orientation: no effect at this load, and nothing smaller measured

Strips with arm lines along **X** and along **Y** both survived at
`X_arm = 60.15`, so an **axis-dependent zipper collapse at X_arm = 60 is
excluded for both axes**. But every strip survived, so the orientation contrast
has **zero variance** and a smaller orientation effect *near the boundary* is
**untested**. Four of the twelve strips are X-axis cells, where the peel front
meets all arms simultaneously and the zipper's stated premise does not even
apply — those four are not zipper tests at all.

Engine consequence: **no orientation term** in sizing or placement. Keep
`peel_dir` plumbed through so a term can be added later without a signature
change, and say in the doc comment that it is unused *by measurement*.

### Finding 4 — Family A6 is VOID; the mechanism is unidentified

See §7 — it is long enough to need its own section.

### Finding 5 — The B6 admissibility ruling is an explicit override of the pre-registration

See §6 — recorded as an argued override, never as a silent re-scope.

### Finding 6 — X is a phenomenological index, not a force model

The engine must **never compute a force** from X. Two independent routes show
why the index cannot be reconciled with a quasi-static peel-stress calculation:

- Measured slender-wall peel pressure (~1 kPa, from plate 1's bound
  `p_eff ≤ 1.24 kPa`) against green tensile strength predicts `X*` in the
  **thousands** (4.8 MPa / 1.24 kPa ≈ 3900). The observed boundary is **~25** —
  about 150× lower.
- The self-consistent reading is a **cavitation-capped flat-patch squeeze** of
  order **100 kPa** under the cap, concentrating to roughly **2–3 MPa** at the
  arm/flare notch. That reconciles the numbers, but it is a reading, not a
  measurement.

Whatever sets this boundary, it is not quasi-static green tensile stress.
**Use the ratio; never recombine `peel_pressure_pa` and `tensile_green_mpa` to
re-derive X.**

### Finding 7 — Two coupon-design rules, born here, binding on every future plate

1. **Never score a feature above z ≈ 9 mm on this printer** until the A6-S
   artifact is understood. Above that band the printer is doing something this
   plate could not explain, and any number measured up there is
   uninterpretable.
2. **Never place a validity control in a different z band than the specimens it
   validates.** A6's control sat in the anomalous band *with* its specimens,
   which is why the family self-voided cleanly — the same mistake with the
   control in the *good* band would have certified an anomalous family as valid.

These join the standing coupon rules already on record: the mandatory control
set; contiguous row bases; no factor ever assigned by a position formula;
Y position randomized rather than measured.

### Finding 8 — The owner's blind personal predictions went 4/6

Recorded because pre-registered predictions are part of the audit trail, not
because they carry weight.

| cell | owner predicted | outcome | verdict |
|---|---|---|---|
| `ZL` | fail | GONE | right |
| `ZS` | fail | GONE | right |
| `PB` | fail | GONE | right |
| `KL` | survive | PRESENT | right |
| `L1` | survive | GONE | **wrong** |
| `KS` | survive | GONE | **wrong** |

Both misses are in the void family, and both are exactly the anomaly of §7.

---

## 6. The override record — B6 was admitted against a pre-registered plate-wide void

**This must never be quietly forgotten.** The A6-1 gate, read literally, does
not void family A6 — it voids **the plate**: *"printing or handling destroyed
the plate and nothing else **on it** can be trusted."* `KS` died. Under the
pre-registration as written, **B6's results are void too**, and everything in
Findings 1–3 is inadmissible.

The panel — and then the owner — **overrode that**, and admitted B6 on the
following argued grounds:

1. **B6 carried its own blind control and it behaved.** `PB`, a blind
   must-fail positive control at `X_tot` = 209, failed as pre-registered. A6's
   gate is a statement about A6's controls; B6's own control did not fire.
2. **B6's 17 outcomes fit a single threshold with zero anomalies.** No B6 cell
   died that any model on the table says should have lived, and none lived that
   any model says should have died.
3. **The failure that fired the gate is confined to a z band B6 does not
   occupy.** Every anomalous loss sits at z ≥ 10.95 mm; every B6 cap sits at
   z ≤ 9.00 mm. The gate's stated cause — "printing or handling destroyed the
   plate" — is contradicted by the empty wash basket and by 14/17 B6 specimens
   surviving intact.

**The counter-argument, recorded because it is not answered:** re-scoping a
pre-registered void after seeing the data is precisely the move pre-registration
exists to block, and points 2 and 3 are both post-hoc. Point 1 is the only
pre-registered element in the override, and the refuter's assessment is that
`PB` carries little validity information because it fails under every candidate
model.

**Standing rule from this episode:** the override is admissible *only* because
it is written down, argued, and attributable. A future plate must not repeat the
pattern. The fix is structural, not rhetorical — Finding 7 rule 2 exists so this
choice never has to be made again.

---

## 7. The A6 anomaly — an unexplained in-print loss above z ≈ 9 mm

Family A6 (6.00 mm arms, caps at z 10.95–12.70 mm) is **VOID by its own
pre-registered validity gate**: the must-survive control `KS` at X = 8.0 died.
**Nothing in A6 may be quoted as a load measurement.** Finding 2's bracket comes
entirely from the B6 mono cells, which sit in the well-behaved z band.

The void is not a soft one:

- **A6 cap-S was wiped 0/11 across X = 8…409** — a **51× load range** with an
  identical outcome at every rung. An outcome invariant across 51× of load is
  **not a load-capacity outcome**.
- **The A6 cap-L sub-ladder is non-monotone.** `L1` (X = 10.01, arm 3.952 mm)
  GONE; `L3` (X = 16.10, arm 3.116 mm) PRESENT. A ladder whose *easiest* rung
  dies while a harder rung lives is not a ladder.
- **Both A6 survivors are in row 2.** Every cap-L specimen in rows 0 and 1 died
  regardless of X.

### Mechanism: UNIDENTIFIED. What the panel eliminated

- **Post-print handling — eliminated.** The wash basket was empty, so every
  loss happened in-print. An independent quantitative inversion argument rules
  handling out as well: at matched X the small cap is ~15.7× more fragile in
  bending than the large one (32ℓ/πd³: `KS` 11.1 vs `KL` 0.70 per unit force),
  so a handling mechanism predicts a size gradient the data does not show in
  the direction required, and `L1` would need ≈ 495 g of lateral force at the
  cap to snap its 3.952 mm arm — a dropped-part event, not a wash current.
- **Pure bending / slenderness — eliminated.** Every standard cantilever law
  misses by **≥ 2×**, and the cap-L ladder is non-monotone in the direction a
  bending law requires.
- **Row gradient — eliminated.**
- **Cap area — eliminated.**

### Leading candidate: an in-print, family-confounded artifact, possibly cap-plane z

Every well-behaved cap on this plate sits at **z ≤ 9.00 mm** (B6: underside
8.500, top 9.000). Every anomalous loss sits at **z ≥ 10.95 mm** (A6-S plane
10.950, A6-L plane 12.000). That split is **100 % confounded with family**, so
this plate **cannot** separate "z band" from "family A6". It is a candidate, not
a finding.

### Arm free length is bounded, not measured

The 4 → 6 mm arm-length change is bounded to a **≤ ~18 %** effect but was never
cleanly measured: the **C6** family that would have isolated it was dropped from
the build. **Do not derive an arm-length term from this plate.** Note also the
consequence if A6 were taken at face value — `KL` alive at X = 7.98 with `L1`
dead at 10.01 would put the 6 mm boundary near 8–10, which would give a design
rule of X ≤ 12 *negative* margin at 6 mm. That is one more reason the void must
be respected rather than partially quoted, and one more reason C6 needs to be
rebuilt (§8).

---

## 8. Follow-ups on record

1. **Vat / FEP census of the lost caps.** Every failure was in-print, so the
   caps are in the vat or on the film. Recovering them dates the losses and may
   discriminate the A6 mechanism.
   *Bookkeeping note:* the follow-up was minuted as "the 17 lost caps", but the
   scored count in §4.2 is **18** failures. The likely reconciliation is `PB`,
   whose 0.247 mm arms may never have formed at all (the train-1 photograph
   shows a plinth with no arms), so it may never have had a cap to lose. Resolve
   this at census time; **do not silently adopt either number**.
2. **Photograph trains 3 and 5.** `3TY` (a headline cell) and `KL` (the
   surviving validity control) currently rest on naked-eye reads.
3. **Rebuild C6-style length rungs.** Arm free length is the one factor this
   plate needed and did not have.
4. **M-ladder replicates ×3.** Finding 2's bracket is n = 1 at each edge.
5. **Strips above `X_tot` = 30.** No strip ever failed, so sharing has no
   measured upper bracket.
6. **A second B6 cap area.** Until then `X_tot` stays degenerate with total arm
   area on this plate (Finding 1 caveat 4).
7. **X = 22 / 25 / 28 triplets** to narrow the (19.91, 30.48) bracket.
8. **Record the resin vendor and batch at print time.** "Black resin, ≈ 30 °C"
   is all that was captured for plate 6; the resin datasheets in
   [`../datasheets/resins/`](../datasheets/resins/) cannot be matched to this
   plate without it.

---

## 9. Relationship to the migration guide

[`../rust-migration/03-physics-and-constants.md`](../rust-migration/03-physics-and-constants.md)
§4 is the interim record written before this file existed, and it carries the
**engine-facing resolution** of each constant — `X_STAR_CYLINDER`,
`X_STAR_CONE`, `X_ALLOWED`, `LOAD_SHARING`, `n_eff_cap`, `N_eff`, the
orientation term, `tip_r_max_mm`. **Go there for what the Rust constants must
say; come here for what the plate actually measured.**

Nothing in this file contradicts §4. Three points where this file is more
precise, flagged rather than silently diverging:

1. **The well-behaved z band.** §4.3 states it as "z ≤ 9.2 mm". The manifest
   gives every B6 cap a top face at **z = 9.00 mm** (underside 8.50). The two
   are consistent — 9.2 is simply the looser statement — and the anomalous band
   (≥ 10.95 mm) is unaffected either way.
2. **The override.** §4.3 states the A6 void but does not record that admitting
   B6 required overriding a gate whose literal text is plate-wide. §6 above is
   that record. This is an addition, not a contradiction.
3. **The still-owed pointer.** §4.4 says the results note "has not been written
   yet" and §4's header block says that section is the record of record "until
   it exists". **It now exists — this file.** Those two sentences in §4 are
   stale as of this commit; the file paths they name have been repointed here,
   but the surrounding prose was deliberately left alone (the migration guide is
   Codex's working document and is out of scope for this change). Treat this
   file as authoritative wherever the two disagree.

---

## 10. Appendices — the verification panel's memos

Three memos from the adversarial verification of 2026-07-31, preserved
unmodified beside this file. Each carries a two-line provenance header and
nothing else was changed.

| file | what it is |
|---|---|
| [`plate-06-verification-physicist.md`](plate-06-verification-physicist.md) | Failure-mode physics memo — the most-parsimonious model that has to account for all 34 specimens. This is where the cap-plane-z confound was first identified. |
| [`plate-06-verification-protocol.md`](plate-06-verification-protocol.md) | Protocol memo — build provenance, gate-by-gate validity ruling, and the literal reading of the plate-wide void that §6 overrides. |
| [`plate-06-verification-refutation.json`](plate-06-verification-refutation.json) | The refuter's per-claim verdicts (`CONFIRMED` / `WEAKENED` / …) with reasoning. Every caveat in §5 traces to a verdict here. **The file carries its two provenance lines as `//` comments, so it is JSONC, not strict JSON — pipe it through `tail -n +3` before `jq`.** |

Companion record: [`plate-05-results.md`](plate-05-results.md) — the conical-arm
plate whose `X* ∈ [25.00, 25.34]` this plate reproduced.
