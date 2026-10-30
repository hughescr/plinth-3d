# Calibration plate 5 — compliant vs stiff: results salvage

**This is a salvage record, not a complete one.** Plate 5 was designed and
scored out of a separate `calibration-coupons` working folder that is now
condemned. Its **geometry** was extracted before deletion and is reproduced here
in full. Its **per-specimen outcomes** were never written down anywhere but a
chat transcript, and only part of them could be recovered. Every recovered
outcome below is quoted or derived from the owner's own words; every
unrecovered one is marked **`read not recovered`** and is **not guessed**.

Read [`plate-06-results.md`](plate-06-results.md) first if you want the load
physics — plate 6 re-measured plate 5's headline on unambiguous geometry and is
the fuller record. This file exists so that plate 5's specimen table and the
owner's reads survive the folder's deletion.

---

## 1. Provenance

| Field | Value |
|---|---|
| Plate | **COUPON-PLATE-5-compliant-vs-stiff** |
| Build md5 | **`99f60cb820d60b32590bdd4b59b6e12b`** (`COUPON-PLATE-5-compliant-vs-stiff.chitubox`) |
| Printed / scored | **not recorded.** The derived constant `X_STAR_CONE` is dated **2026-07-30** in the migration guide, so scoring was on or before that date. |
| Printer | **Elegoo Saturn 4 Ultra**, tilt release, peel front sweeps −Y → +Y |
| Resin / ambient | not recorded |
| Layer height | **0.050 mm** |
| Specimens | **72** measuring specimens in six families, plus 13 ballast columns |
| Build | 127 166 triangles; tallest point 16.57 mm = **332 layers** (specimens alone reach 15.60 mm); measured resin draw ≈ **39.9 mL ≈ 44 g** |
| Cured area | z = 6.40…15.60 mm: **1280…1549 mm²**. At the critical layers: E 1365…1883 mm² / 6 layers (spread 35.4 %); LAD 1326…1454 / 8 (9.2 %); S 1296…1446 / 12 (10.9 %); **TIP 1296…1490 / 130 (14.1 %)** |
| Scoring | **partial** — see §4. No adversarial verification panel was run on plate 5 (that practice started with plate 6). |

**Layout — five rows, front to back, each one contiguous perforated slab:**

| row | Y (mm) | slab width | what it carries | owner's name for it |
|---|---|---|---|---|
| R1 | −45.19 | 154.100 | family **S** — splayed bridges (9 specimens) | "row 1" |
| R2 | −25.19 | 147.964 | family **TIP** row A (15) + witness pad `W5A` | **"row 2"** |
| R3 | +0.00 | 174.900 | family **LAD** (13 masts) + validity piers `EC1`/`EP1` + witness `Q2` | "row 3" |
| R4 | +25.19 | 148.036 | family **TIP** row B (15) + witness pad `W5B` | **"row 4"** |
| R5 | +45.19 | 158.685 | family **S** — splayed bridges (10) | "row 5" |

R1 and R5 are a **matched pair**: same specimens, opposite Y, so any R1-vs-R5
difference is position, not design.

---

## 2. What the plate was for

Verbatim from the plate's pre-registration (`PLATE-LAYOUT.md` §plate 5):

> Plates 1-4 measured **where** things fail. Plate 5 measures **why**: it puts
> the area-pressure, line-tension and squeeze-film readings of the same data
> against each other on one build, and it separates DELAMINATION (the layer
> interface lets go) from TEAR (the section breaks). Every earlier plate's
> number is conditional on which of those wins, so read this plate before
> re-fitting any of them.

The family that produced the surviving result is **TIP**:

> A cap of area `A_cap` on a slender tip of diameter `d_t`. Under an
> AREA-pressure law the load on the tip goes as `A_cap`; under a LINE-tension
> law it goes as the cap PERIMETER; under a squeeze film it goes as
> `A_cap^2 / d^3`. The three predict different columns of this grid failing,
> and the grid is built so they cannot be confused: 24 cells covering the full
> 8 tip rungs x 3 cap diameters exactly once, `r(log d_t, log A_cap) =
> 0.000000`, and constant column sums (9 / 4 / 13) for rung, cap and stagger
> level.
>
> The X column is the RELATIVE load each theory's discriminant puts on the tip;
> it spans 0.64 to 15625, a factor of 24000, which is what makes the exponent
> fittable from one plate.

### Pre-registered TIP gates, verbatim

> **Score `F` if the cap is gone or hanging, `P` if the tip is bent or necked
> but holding, `-` if intact.** A cap found FUSED TO A CATCH POST voids that
> cell -- photograph it and strike it out.

> **What it looks like:** binary and visible from a metre — a mushroom cap
> either sits on its stalk or the cap is gone and only a pointed stalk remains.
> Note separately if the break surface is a **ring** part-way up the flare cone
> rather than a clean tip break: that invalidates the rung (the flare failed,
> not the tip).

> **Aux block** (both rows, blind): two POSITIVE CONTROLS at 0.85x and 0.70x
> nominal tip diameter, and one height-ladder cell per row (`h_tip` 0.20 mm
> front, 0.80 mm rear) against the grid's constant 0.40 mm. If the two PCs do
> not fail while their nominal-diameter twins survive, this family's
> sensitivity is unproven and the exponent fit is not admissible.

**How the aux gate fired: it did not, cleanly.** The positive controls `TAP1`,
`TAP2` (row A, X = 219 and 323) failed as required. But their nominal-diameter
twin is the rung-4 / cap-5.00 grid cell `TB10` at X = 158.4, and the boundary
turned out to sit near X ≈ 25 — an order of magnitude below the twin — so the
twin cannot have survived either. The second clause of the gate ("while their
nominal-diameter twins survive") therefore cannot have been met, which means
**the Q3 exponent fit was never admissible under its own pre-registration.**
The `X*` boundary reading in §5 is a different question and does not rest on
that clause. Confirm this against the row-4 read if it is ever recovered (§6).

---

## 3. Family TIP — the full 30-specimen table, as built and as scored

Straight from `manifest.json`, plate key `COUPON-PLATE-5-compliant-vs-stiff`,
family `TIP`. `pos` is the position counted left-to-right in increasing X, which
is how the owner read the rows. `X = A_cap / A_tip`. **Tips are cones**; `A_tip`
is computed from the nominal tip diameter, which is exactly the ambiguity plate
6 was built to remove.

### Row A — plate row R2, Y = −25.19 mm — the owner's "row 2"

| pos | id | rung | d_t | cap D | A_cap | A_tip | X | cap-top z | tip h | kerb | x | outcome |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `TA01` | 3 | 0.25085 | 2.0 | 3.1416 | 0.049421 | 63.568 | 15.6 | 0.4 | no | -70.509 | **GONE** |
| 2 | `TA02` | 7 | 1.57846 | 12.5 | 122.7185 | 1.956854 | 62.712 | 12.6 | 0.4 | yes | -59.119 | **GONE** |
| 3 | `TA03` | 1 | 0.10000 | 5.0 | 19.6350 | 0.007854 | 2500.000 | 9.6 | 0.4 | no | -47.569 | **GONE** |
| 4 | `TA04` | 5 | 0.62925 | 2.0 | 3.1416 | 0.310982 | 10.102 | 9.0 | 0.4 | no | -41.937 | **PRESENT** |
| 5 | `TA05` | 7 | 1.57846 | 5.0 | 19.6350 | 1.956854 | 10.034 | 15.0 | 0.4 | no | -36.305 | **PRESENT** |
| 6 | `TA06` | 1 | 0.10000 | 12.5 | 122.7185 | 0.007854 | 15625.000 | 12.0 | 0.4 | yes | -24.755 | **GONE** |
| 7 | `TA07` | 5 | 0.62925 | 12.5 | 122.7185 | 0.310982 | 394.616 | 10.2 | 0.4 | yes | -8.055 | **GONE** |
| 8 | `TA08` | 3 | 0.25085 | 5.0 | 19.6350 | 0.049421 | 397.299 | 11.4 | 0.4 | no | 3.495 | **GONE** |
| 9 | `TA09` | 1 | 0.10000 | 2.0 | 3.1416 | 0.007854 | 400.000 | 13.8 | 0.4 | no | 9.586 | **GONE** |
| 10 | `TA10` | 5 | 0.62925 | 5.0 | 19.6350 | 0.310982 | 63.139 | 13.2 | 0.4 | no | 15.677 | **GONE** |
| 11 | `TA11` | 3 | 0.25085 | 12.5 | 122.7185 | 0.049421 | 2483.117 | 14.4 | 0.4 | yes | 27.227 | **GONE** |
| 12 | `TA12` | 7 | 1.57846 | 2.0 | 3.1416 | 1.956854 | 1.605 | 10.8 | 0.4 | no | 38.194 | **PRESENT** |
| 13 | `TAP1` | 4 | 0.33770 | 5.0 | 19.6350 | 0.089570 | 219.214 | 15.6 | 0.4 | no | 44.011 | **GONE** (poscon 0.85×) |
| 14 | `TAP2` | 4 | 0.27811 | 5.0 | 19.6350 | 0.060746 | 323.228 | 15.6 | 0.4 | no | 50.411 | **GONE** (poscon 0.70×) |
| 15 | `TAH` | 4 | 0.39730 | 5.0 | 19.6350 | 0.123972 | 158.382 | 15.6 | **0.2** | no | 56.811 | **GONE** (height ladder) |

Row A is **fully recovered**: the owner named three survivors out of fifteen,
so the other twelve are failures by his own count. See §4 for the quotes and
the four independent cross-checks that pin the mapping.

### Row B — plate row R4, Y = +25.19 mm — the owner's "row 4"

| pos | id | rung | d_t | cap D | A_cap | A_tip | X | cap-top z | tip h | kerb | x | outcome |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | `TB01` | 6 | 0.99662 | 12.5 | 122.7185 | 0.780094 | 157.312 | 9.0 | 0.4 | yes | -51.830 | read not recovered |
| 2 | `TB02` | 2 | 0.15838 | 2.0 | 3.1416 | 0.019702 | 159.459 | 12.0 | 0.4 | no | -40.749 | read not recovered |
| 3 | `TB03` | 8 | 2.50000 | 5.0 | 19.6350 | 4.908739 | 4.000 | 15.0 | 0.4 | no | -34.818 | read not recovered |
| 4 | `TB04` | 4 | 0.39730 | 12.5 | 122.7185 | 0.123972 | 989.887 | 15.6 | 0.4 | yes | -23.268 | read not recovered |
| 5 | `TB05` | 2 | 0.15838 | 5.0 | 19.6350 | 0.019702 | 996.618 | 9.6 | 0.4 | no | -11.718 | read not recovered |
| 6 | `TB06` | 8 | 2.50000 | 2.0 | 3.1416 | 4.908739 | 0.640 | 12.6 | 0.4 | no | -5.732 | read not recovered |
| 7 | `TB07` | 4 | 0.39730 | 2.0 | 3.1416 | 0.123972 | 25.341 | 14.4 | 0.4 | no | -0.005 | read not recovered |
| 8 | `TB08` | 6 | 0.99662 | 5.0 | 19.6350 | 0.780094 | 25.170 | 13.2 | 0.4 | no | 6.137 | read not recovered |
| 9 | `TB09` | 8 | 2.50000 | 12.5 | 122.7185 | 4.908739 | 25.000 | 10.8 | 0.4 | yes | 17.687 | **GONE** — see §4 |
| 10 | `TB10` | 4 | 0.39730 | 5.0 | 19.6350 | 0.123972 | 158.382 | 11.4 | 0.4 | no | 29.237 | read not recovered |
| 11 | `TB11` | 6 | 0.99662 | 2.0 | 3.1416 | 0.780094 | 4.027 | 10.2 | 0.4 | no | 34.994 | read not recovered |
| 12 | `TB12` | 2 | 0.15838 | 12.5 | 122.7185 | 0.019702 | 6228.860 | 13.8 | 0.4 | yes | 45.901 | read not recovered |
| 13 | `TBP1` | 4 | 0.33770 | 5.0 | 19.6350 | 0.089570 | 219.214 | 15.6 | 0.4 | no | 57.451 | read not recovered (poscon 0.85×) |
| 14 | `TBP2` | 4 | 0.27811 | 5.0 | 19.6350 | 0.060746 | 323.228 | 15.6 | 0.4 | no | 63.851 | read not recovered (poscon 0.70×) |
| 15 | `TBH` | 4 | 0.39730 | 5.0 | 19.6350 | 0.123972 | 158.382 | 15.6 | **0.8** | no | 70.251 | read not recovered (height ladder) |

**What *is* recovered for row B**, from the owner directly: exactly **5 of 15
survived**; of those, **2 have medium (D 5.00 mm) caps and 3 have small
(D 2.00 mm) caps**; **every large (D 12.50 mm) cap failed**, snapping "at the
base of their cylinders, just above the cones". Individual IDs were never
written down. Units: mm, mm², dimensionless X.

---

## 4. The owner's reads — verbatim

These are the only outcome records that exist. Quoted from the session
transcript of the plate-5 scoring; **the transcript is not in git and should not
be assumed to survive.**

> […] pretty much every "mushroom cap" got plucked from its support post in the
> print. I don't need to count peel layers with a loupe (which I don't have) or
> try and discern fine nuance of detail in the prints to see that "every cap got
> ripped off its support except the really small caps". **Row 2 looks like it
> had *3* survivors out of 15 caps. Row 4 fared only slightly better at 5
> survivors out of 15.** Row 3 was kind of a mess, but rows 1 and 5 give a
> pretty good view of how long of a bridge can be effectively supported from its
> two ends without breaking or sagging […]

> **The caps which survived, reading left-to-right on row 2 (starting with #1 on
> far left): #4, #5, #12 survived** - 4 and 12 are small, but #5 is "medium" it
> had enough support to survive though. **#1 and #9 are also small caps, but
> failed** (probably the supports were too small there). **Number 2 was probably
> the most-likely to succeed of the large-caps**, but it looks like the whole
> angled-slope mushroom head *and* the short cylinder was pulled off the conical
> support below it, that is it looks like the break happened around layer #195
> (though the break probably happened at a much later layer once the surface
> area of the cap flared out to be large enough for a big peel force. **The same
> thing appears to have happened on all of the large caps - the base of the
> cylindrical portion where it meets the conical support show breaks.** Most of
> these are *extremely* thin points – the remaining supports that are there look
> visually like they are just cones coming to their full points; they're just
> WAY too skinny to survive.

> **Looking at row 4 more closely by eye, 2 survivors are medium sized and 3
> "small"**, and the same pattern: the supports for the large caps all snapped at
> the base of their cylinders, just above the cones. Only one of these was even
> plausibly visually thick enough of a support to hold and that was **#9 or the
> 3rd of the large caps in that row**, which looks like it was the thickest of
> all of the supports. It also, visually shows the strongest evidence of having
> been just snapped off - it has a faceted angled "cleaved" looking surface.

### Why the row-2 mapping is certain

Four independent checks, all from the manifest, all consistent:

1. **#4, #5, #12 → `TA04`, `TA05`, `TA12`** by position in increasing x.
2. **"4 and 12 are small, but #5 is medium"** — `TA04` cap D = 2.00 (small),
   `TA12` cap D = 2.00 (small), `TA05` cap D = 5.00 (medium). ✔
3. **"#1 and #9 are also small caps, but failed"** — `TA01` and `TA09` are cap
   D = 2.00. ✔ Row A contains **exactly four** small caps (`TA01`, `TA04`,
   `TA09`, `TA12`) and the owner accounted for all four.
4. **"Number 2 was […] the most-likely to succeed of the large-caps"** —
   `TA02` is cap D = 12.50 with `d_t` = 1.578, the thickest tip of any large-cap
   cell in the row. ✔

The same position convention independently checks out in row 4: the owner's
**"#9 or the 3rd of the large caps in that row"** resolves to `TB09`, which is
both position 9 *and* the third large cap by x *and* the thickest tip on the
plate (`d_t` = 2.500). That is why `TB09` is the one row-B cell scored **GONE**
above — the owner described its break surface directly.

### The row-4 reconstruction — an inference, NOT a read

Recorded separately, and deliberately kept out of the outcome column. If the
row-B outcomes are assumed monotone in X (survivors are the lowest-X cells
within each cap size), the owner's three constraints — 5 survivors, 2 medium +
3 small, 0 large — have a **unique** solution:

| survivors (inferred) | cap | X |
|---|---|---|
| `TB06` | small | 0.640 |
| `TB11` | small | 4.027 |
| `TB03` | medium | 4.000 |
| `TB08` | medium | 25.170 |
| `TB07` | small | 25.341 |

with `TB09` (large, X = 25.000) dead — the recorded **boundary inversion**.
This reconstruction reproduces the published `X_STAR_CONE = [25.00, 25.34]`
exactly, including "one boundary inversion at 25.00", which is strong evidence
that it *is* what the original analysis did. **It is still an inference.** Do
not promote it into the table without recovering the read.

---

## 5. Findings carried forward

1. **The failure boundary is a ratio near X ≈ 25.** `X_STAR_CONE = [25.00,
   25.34]` — **store the interval, never a midpoint** — with **one boundary
   inversion at 25.00** (`TB09`, the largest cap on the thickest tip, dead,
   while X = 25.17 and 25.34 lived). Plate 6 later reproduced the boundary on
   constant-diameter **cylinders** at `X* ∈ (19.91, 30.48)`, which contains this
   interval; that is what retired the "cone artifact" worry. **Keep the two
   constants separate** — different geometry, very different precision.
2. **The area law wins.** X is `A_cap / A_tip`; survival tracks it across a
   24 000× spread of the discriminant and across three cap diameters. Line
   tension (perimeter) and squeeze film (`A_cap²/d³`) do not fit the pattern.
   Plate 5's **cap-area spread is the only evidence the engine has that X is a
   ratio rather than a total-area threshold** — plate 6 used a single cap area
   and is degenerate on that point.
3. **The failure section is the narrowest section just above the contact, not
   the nominal tip disc.** The owner observed the break at *"the base of the
   cylindrical portion where it meets the conical support"* on every large cap.
   On a cone that is not the tip diameter — which is the measurement ambiguity
   plate 6 was built to eliminate, and why the engine keeps a
   minimum-section locator rather than assuming the tip disc.
4. **Small caps survive, large caps do not, at fixed tip geometry** — the
   plate's blunt headline, in the owner's words: *"every cap got ripped off its
   support except the really small caps."*
5. **Rows 1 and 5 (bridges) are readable; row 3 is not.** Owner: *"Row 3 was
   kind of a mess, but rows 1 and 5 give a pretty good view of how long of a
   bridge can be effectively supported from its two ends."* No per-bridge scores
   were ever recorded.

The engine-facing constants derived from these findings live in
[`../03-physics-and-constants.md`](../03-physics-and-constants.md)
§3 and §4 (`X_STAR_CONE`, `X_ALLOWED`, `tensile_green_mpa`).

---

## 6. What is missing, and where it would have to come from

1. **Row 4's per-specimen outcomes** (10 of the 15 unknown individually). The
   only source is the session transcript of the plate-5 scoring — not in git.
   If the physical plate still exists, re-reading it is faster than archaeology.
2. **Rows 1, 3 and 5 outcomes** — families S (19 bridge specimens × 4 bridges =
   68 bridges), LAD (13 masts) and E (2 piers). Never scored per specimen; the
   only record is the qualitative sentence in §4. The elaborate scoring rubric
   written for this plate was never executed.
3. **Print date, resin and ambient temperature.** Not recorded anywhere.
4. **Whether any cap was found fused to a catch post** (the pre-registered VOID
   condition for cap-L cells) — never checked.
5. **The aux-block sensitivity gate** (§2) — almost certainly failed its second
   clause; confirm if row 4 is ever recovered.

---

## 7. The rest of the plate — geometry index

No outcomes were recorded for any of these. The geometry is preserved here
because it is the only copy that will exist.

### Family S — splayed bridges, rows R1 / R5 (19 specimens, 68 bridges)

Two towers carrying **four** bridges each, at undersides z = 7.00 / 9.00 /
11.00 / 13.00 **+ dz**. Every bridge is 2.00 mm wide and 0.300 mm thick
(6 laminae of 0.050 mm); only the SPAN changes, as `S = g + a·k` with k the
level index. `sum(a) = 0` over all 17 factor-bearing specimens and
`Cov(S, z) = 0` exactly over the 68 bridges, so span and height cannot be
confused. Side-wall rulers at 0.250 mm pitch on both ±Y faces, with double-major
marks at 0.2113 S and 0.7887 S — the two stations where a clamped-clamped beam's
bending moment vanishes.

| id | row | x | key | role | g | a | dz | spans k0/k1/k2/k3 | tower (mm) |
|---|---|---|---|---|---|---|---|---|---|
| `SD2` | R1 | -70.567 | D2 | core-div | 2.45 | +0.55 | 0.0 | 2.45 / 3.0 / 3.55 / 4.1 | 4.0 × 3.0 |
| `ST1` | R1 | -55.467 | T1 | gauge | 6.1 | 0.0 | 0.6 | 6.1 / 6.1 / 6.1 / 6.1 | 4.0 × 3.0 |
| `SP2` | R1 | -40.217 | P2 | **poscon** | 4.4 | -0.65 | 0.0 | 4.4 / 3.75 / 3.1 / 2.45 | 4.0 × 3.0 |
| `SD5` | R1 | -21.667 | D5 | core-div | 5.05 | +2.55 | 0.0 | 5.05 / 7.6 / 10.15 / 12.7 | 4.0 × 3.0 |
| `SC6` | R1 | +1.933 | C6 | core-cnv | 14.5 | -2.8 | 0.3 | 14.5 / 11.7 / 8.9 / 6.1 | 4.0 × 3.0 |
| `SC2` | R1 | +21.483 | C2 | core-cnv | 4.6 | -0.8 | 0.6 | 4.6 / 3.8 / 3.0 / 2.2 | 4.0 × 3.0 |
| `SD6` | R1 | +41.133 | D6 | core-div | 2.7 | +4.0 | 0.6 | 2.7 / 6.7 / 10.7 / 14.7 | 4.0 × 3.0 |
| `SX1` | R1 | +59.083 | X1 | cross-tie (4 cantilevers) | — | — | — | — | — |
| `SC1` | R1 | +71.083 | C1 | core-cnv | 2.8 | -0.45 | 0.3 | 2.8 / 2.35 / 1.9 / 1.45 | 4.0 × 3.0 |
| `SD1` | R5 | -73.687 | D1 | core-div | 1.4 | +0.35 | 0.6 | 1.4 / 1.75 / 2.1 / 2.45 | 4.0 × 3.0 |
| `SC4` | R5 | -58.317 | C4 | core-cnv | 8.65 | -2.1 | 0.6 | 8.65 / 6.55 / 4.45 / 2.35 | 4.0 × 3.0 |
| `SX2` | R5 | -43.577 | X2 | cross-tie (4 cantilevers) | — | — | — | — | — |
| `SD3` | R5 | -29.987 | D3 | core-div | 3.5 | +0.95 | 0.0 | 3.5 / 4.45 / 5.4 / 6.35 | 4.0 × 3.0 |
| `SC5` | R5 | -11.297 | C5 | core-cnv | 11.4 | -2.6 | 0.3 | 11.4 / 8.8 / 6.2 / 3.6 | 4.0 × 3.0 |
| `SC3` | R5 | +7.493 | C3 | core-cnv | 6.55 | -1.35 | 0.3 | 6.55 / 5.2 / 3.85 / 2.5 | 4.0 × 3.0 |
| `SP1` | R5 | +22.783 | P1 | **poscon** | 2.45 | +0.65 | 0.0 | 2.45 / 3.1 / 3.75 / 4.4 | 4.0 × 3.0 |
| `SD4` | R5 | +38.173 | D4 | core-div | 1.65 | +1.7 | 0.0 | 1.65 / 3.35 / 5.05 / 6.75 | 4.0 × 3.0 |
| `ST2` | R5 | +56.913 | T2 | gauge | 11.1 | 0.0 | 0.3 | 11.1 / 11.1 / 11.1 / 11.1 | 4.0 × 3.0 |
| `ST3` | R5 | +73.523 | T3 | gauge | 2.5 | 0.0 | 0.6 | 2.5 / 2.5 / 2.5 / 2.5 | 4.0 × 3.0 |

`D` = spans widen with height, `C` = spans narrow, `T` = constant span with
gauge pillars, `X` = cross-tie mast, `P` = plain constant-`a` specimen. The
letter is bookkeeping, never a treatment label.

### Family LAD — ladder masts, row R3 (13 specimens)

Shelf reach 7.7841 mm on every mast; `n` laminae of 0.050 mm each give shelf
thickness `t`. `square` A/B is the crossed factor, `phase` the stagger,
`graduated` whether the shelf is stepped.

| id | station | n | laminae | t (mm) | square | phase | graduated | direction | x | y |
|---|---|---|---|---|---|---|---|---|---|---|
| `L6a` | 1 | 6 | 6 | 0.30 | A | 4 | yes | +X | -73.924 | 0.0 |
| `L1a` | 2 | 1 | 1 | 0.05 | A | 0 | no | +X | -62.924 | +2.4 |
| `P1` | 3 | 6 | 6 | 0.30 | A | 3 | no | −X | -58.209 | -2.4 |
| `P2` | 5 | 6 | 6 | 0.30 | A | 5 | yes | +X | -29.924 | 0.0 |
| `L6b` | 6 | 6 | 6 | 0.30 | B | 3 | no | +X | -18.924 | 0.0 |
| `L12b` | 7 | 12 | 12 | 0.60 | A | 3 | no | +X | -7.924 | 0.0 |
| `L2a` | 8 | 2 | 2 | 0.10 | A | 2 | yes | +X | +3.076 | 0.0 |
| `L3a` | 9 | 3 | 3 | 0.15 | A | 1 | no | +X | +14.076 | 0.0 |
| `P3` | 10 | 6 | 6 | 0.30 | B | 5 | yes | +X | +25.076 | 0.0 |
| `L12a` | 12 | 12 | 12 | 0.60 | B | 4 | no | +X | +47.076 | 0.0 |
| `L2b` | 13 | 2 | 2 | 0.10 | B | 0 | no | +X | +58.076 | 0.0 |
| `L3b` | 14 | 3 | 3 | 0.15 | B | 2 | yes | +X | +69.076 | -2.4 |
| `L1b` | 15 | 1 | 1 | 0.05 | B | 1 | no | −X | +73.791 | +2.4 |

Note the two `n = 1` masts (`L1a` station 2, `L1b` station 15) have built shelf
thickness 0.0500 mm on every level and are ungraduated; the clean ungraduated
`n = 6` twin for comparison is `L6b` (station 6).

### Validity piers and witness pads (5 specimens)

| id | family | kind | x | y | top z | purpose |
|---|---|---|---|---|---|---|
| `EC1` | E | curecomb | -88.017 | 0.0 | 5.70 | measures Δ, cure-through overshoot. Gate: *"the n = 1 reversal claim is void unless Delta >= 0; the ramp magnitude is quoted 2.25-3.5x, never 6x"* |
| `EP1` | E | epier | -44.067 | 0.0 | 14.15 | band pier; 5 bands at z (2.00,3.50) (6.70,7.90) (8.30,9.80) (10.10,11.35) (12.90,14.15), all clear of R3's eight critical layers by ≥ 0.075 mm |
| `W5A` | W | witness | +67.063 | -25.19 | 3.80 | printed-vs-nominal at all eight `TIP_DT_MM` rungs, plus the parallax key |
| `W5B` | W | witness | -67.232 | +25.19 | 3.80 | as `W5A`, opposite row |
| `Q2` | W | witness | +32.933 | 0.0 | 2.60 | as above, single-axis |

Plus 5 row slabs (`BASE1`…`BASE5`, one per row, plan areas 2700.9 / 881.8 /
2647.7 / 882.2 / 2783.7 mm²) and 13 ballast columns that carry no data.

**Known residual from the build audit:** the 13 ballast columns are split
2/4/1/4/2 across the rows, so the fragile R3 got only one; per-row cured-area
spread reaches 142.7 % on R4; `TB01`'s first 5 laminae (z 6.175–6.375) and 2 E
critical layers sit below the 6.40 mm area-flattening window. None of these
affects the TIP reading in §5.

---

## 8. Pointers

- [`plate-06-results.md`](plate-06-results.md) — the successor plate; cylindrical
  arms, load sharing, and the record of record for support-arm capacity.
- [`../03-physics-and-constants.md`](../03-physics-and-constants.md)
  — engine-facing constants derived from both plates.
