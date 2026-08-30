# Calibration plate 1 — overhang tilt: results salvage

**This is a salvage record, recovered 2026-08-30 from chat transcripts.** Plate 1
was printed and scored, and the owner's reads were never written to a file — the
same failure that produced [`plate-05-results.md`](plate-05-results.md). The
geometry survives in
[`plate-generator-PLATE-LAYOUT.md`](../../evidence/plate-generator-PLATE-LAYOUT.md);
the outcomes below were recovered from the session of **2026-07-29** together
with six photographs, which are embedded in that transcript as JPEGs and were
extracted alongside this write-up.

Every outcome here is **quoted from the owner's own words** or derived from them
with the derivation shown. Anything not recovered is marked
**`read not recovered`** and is **not guessed**.

Two findings from this plate were already folded into the layout document at the
time and are reproduced here with their provenance, so they stop being orphaned
remarks inside a design file: the family-E validity gate (§4) and the derived
bound on peel pressure (§5).

---

## 1. Provenance

| Field | Value |
|---|---|
| Plate | **COUPON-PLATE-1-overhang-tilt** |
| Printed / scored | **on or before 2026-07-29** (the date of the transcript carrying the reads) |
| Printer | **Elegoo Saturn 4 Ultra 16K**, tilt release, vat hinged at the **back** (+Y); peel front sweeps −Y → +Y |
| Resin | **Sunlu Water-Wash ABS-Like Black** |
| Layer height | **0.050 mm**; 1.2 s normal, 3 × 15.0 s bottom + 5 transition |
| Film age | **2nd print on that PFA film** — "practically brand new" (owner) |
| Supports | **none.** Every specimen stands on its own 2.00 mm pad; support generation off |
| Specimens | **17** family-A masts, **5** family-D towers, **5** family-E witness slabs, plus row headers and corner fiducials |
| Scoring | **partial** — qualitative visual read, no per-rung table. No adversarial verification panel (that practice started with plate 6) |

**Plate 1 is the only plate that kept its row-marker tiles and corner
fiducials.** The owner read it and reported the markers "completely superfluous
— I didn't even use them just tossed them in the trash after curing", so they
were removed from every later plate: 17 tiles, **8.63 mL** of resin carrying
nothing the specimen IDs did not already carry.

### Station map — five Y rows

| row | Y (mm) | family A | family D | family E |
|---|---|---|---|---|
| front | −44.0 | `A1aU` `A1bD` `A1cU` | `D1` | `E1X1` |
| | −22.0 | `A2aD` `A2bU` `A2cD` | `D2` | `E1X2` |
| middle | 0.0 | `A3aU` `A3bD` `A3cU` `AW1` `AW4` | `D3` | `E1X3` |
| | +22.0 | `A4aD` `A4bU` `A4cD` | `D4` | `E1X4` |
| **back (hinge)** | **+44.0** | `A5aU` `A5bD` `A5cU` | **`D5`** | `E1X5` |

`U` = ascending rung order, `D` = descending. Finger width `b` = 2.00 mm except
`AW1` (1.00 mm) and `AW4` (4.00 mm).

---

## 2. What the plate was for

A flat-overhang ladder in a single 0.050 mm layer — **pure bending**, no support
anywhere. Each mast carries a ladder of cantilevered fingers of increasing
length; the length at which a finger stops surviving brackets the ratio of peel
pressure to green-resin strength.

Family D is the **peel-direction control**: one tower per Y station carrying
fingers in several directions, to test whether the tilt release makes direction
or front-to-back position matter. Family E is a **thin-wall resolution and
cure-quality witness**, repeated unchanged on every later plate as the same
validity gate.

---

## 3. Family A — the reads, verbatim

The owner photographed the masts group by group. His words, in order:

> **A1aU / A1bD / A1cU** (left-to-right): "they all seem to have come out pretty
> well; **high vs low doesn't seem to matter for overhang**, but the corners of
> the longest shelves all seem to be slightly sagging - just barely, but
> definite visual change at the very corners."

> **A2 a, b, c**: "again the corners of the longest shelves sag slightly, but
> otherwise looking good."

> **AW1 and AW4**: "Both look pretty decent. Corners on the long shelves again
> sagging slightly."

> **A3 a, b, c**: "looking good but for the long shelf on C's corners sagging
> again"

> **A4 a, b, c**: "same again."

> **A5 a, b, c** — **`read not recovered`.**

**Result: no family-A finger was lost at any station.** The only defect reported
anywhere in family A is slight corner sag on the longest shelves, present in
every group that was read.

**The A5 gap matters and is called out rather than buried.** A5 sits at Y = +44.0
— the same rear station as `D5`, the one specimen the owner reported as having
struggled (§6). The layout document asserts family A "survived every rung at
every station", which would include A5; the individual read backing that claim
for A5 is not in the recovered transcript. Treat the A5 row as **asserted, not
quoted**.

---

## 4. Family E — the validity gate, PASSED

> "Here's 2 shots of the 'E' series - they all appear to have printed fairly
> well – **every wall is present and intact** as far as I can tell."

> "The walls are all full height, visually inspected. The glossy spots are I
> think a resin residue thing where spots of resin were perhaps not fully washed
> off before drying and curing, so they formed a smooth flat sheeny surface on
> top of the print."

Recorded in the layout document at the time as:

> **PLATE 1 RESULT: this gate PASSED.** Every wall printed full height at all
> five Y stations, including the thinnest at 0.10 mm (7.1 px in X).

**Two consequences, both carried into every later plate:**

1. **There is no cure-quality gradient front to back.** Any position effect this
   plate family measures is therefore **mechanical, not chemical**. This is what
   makes the family-D result in §6 interesting rather than explicable.
2. **0.10 mm is a confirmed resolvable feature size**, and became the hard
   minimum on every graded dimension in the plate programme.

The glossy patches are the owner's own diagnosis — wash residue cured flat — and
are **not** a print defect. Noted here because a later reader looking at the
photographs will see them.

---

## 5. The derived bound on peel pressure

Recorded in the layout document at the time, reproduced with its reasoning:

> Family A survived every rung at every station, with corner sag only on the top
> two, so `L_crit >= 2.5920 mm` and therefore `p_eff / sigma_green <= 1.24e-04`
> — against `3.0e-4` assumed in `config.py` and `2.5e-3` in the active-layer LP.
> In pressure terms that is `p_eff <= 1.24 kPa` at `sigma_g = 10 MPa`, best
> estimate **0.6–1.2 kPa**.

Read the direction carefully: this is a **bound, not a measurement**. Nothing
failed, so the plate says the critical length is *at least* 2.5920 mm and the
pressure ratio *at most* 1.24e-04. It brackets from one side only.

Its practical consequence was that **plates 2, 3 and 4 were re-centred on it** —
every later ladder was rebuilt to bracket `p_eff` in [0.4, 3] kPa crossed with
`sigma_g` in [4.8, 10] MPa, rather than repeating a ladder the material walks
straight through.

**Superseded for support sizing.** The area law now in force
(`pi*r^2 >= carried_area / X_ALLOWED`, `X_ALLOWED = 12`) rests on plates 5 and 6,
not on this bound. Plate 1's number remains the record of where the pressure
envelope was first bounded, and of why the later ladders sit where they sit.

---

## 6. Family D — the one thing that struggled, and it is still unexplained

The owner's read, verbatim and complete:

> "**The one that struggled was at the back, nearest the hinge.** I don't know
> why it failed. We might went to re-test that back spot on the plate, or other
> spots in the same Y but spread across X in case there's a problem with LCD in
> that location or something? It might have just been experimental error.
>
> I also do note that the tips of the top overhang on all of the towers looks
> like the corners might be sagging a little - more so on D1 and D2 than on D3
> and D4. I think though that they all look reasonably decent!"

**The specimen is `D5`, at Y = +44.0** — the rear-most station, nearest the vat
hinge. That is where the tilt gap opens *least*.

**Which finger direction failed is `read not recovered`.** It was asked for at
the time and the answer is not in the transcript. The distinction was the whole
point of family D: a **+Y** (rear-pointing) failure is the predicted
peel-direction effect, while a **+X** failure would be a position effect that
points *against* the tilt model.

### Three live explanations, none eliminated

1. **A dead or weak LCD region at that spot.** The owner named this first and
   designed the discriminating test in the same breath: re-print at the same Y,
   **spread across X**. A panel defect is local in X; a position effect is not.
2. **Peel-direction effect**, which is what family D was built to find.
3. **Resin starvation at the hinge end.** The rear is where the gap opens least
   during tilt, so fresh resin refills slowest there — producing under-filled,
   weak features at high +Y entirely independently of release force. This was
   raised at the time and never tested. **Family E cannot see it**: E rules out a
   *cure* gradient, and starvation is a different mechanism.

If both (2) and (3) are live they push in **opposite directions** — peel load
highest at the front, refill worst at the rear — so the failure boundary against
Y would not be monotone, and a straight-line hinge fit would be the wrong model.

### Why this is still open

Family A was expected to settle it: 17 masts across the same five stations, with
a full rung ladder each, is a far larger dataset than five D towers. But family A
**lost nothing anywhere**, so it returned no boundary to compare across Y — and
the one A group at the failing station (`A5`) is the group whose read was not
recovered.

**Status: one failed specimen at the rear station, cause unknown, with a named
confound and a cheap discriminating test that has never been run.** That test —
a strip at one Y spread across X — is much less than a full plate.

---

## 7. Findings carried forward

| # | Finding | Strength |
|---|---|---|
| 1 | The family-E thin-wall gate passes at every Y station; **no cure gradient front to back** | Measured, all 5 stations |
| 2 | **0.10 mm is a resolvable feature size**; hard minimum on every graded dimension after this plate | Measured |
| 3 | `L_crit >= 2.5920 mm`; `p_eff <= 1.24 kPa` at `sigma_g = 10 MPa`, best estimate 0.6–1.2 kPa | **Bound from one side only** — nothing failed |
| 4 | High versus low position on a mast does not affect overhang survival | Owner's read, qualitative |
| 5 | Corner sag on the longest shelves, everywhere it was read | Owner's read, qualitative; no measurement of sag magnitude |
| 6 | `D5`, rear station nearest the hinge, had one finger fail. Direction unknown, cause unknown | **One specimen.** Three explanations live |
| 7 | Row markers and corner fiducials are waste (8.63 mL); removed from all later plates | Owner's ruling |

## 8. What this plate cannot tell you

- **Nothing about the failure boundary in family A**, because nothing in family A
  failed. The ladder was too easy for the material; that is what the re-centring
  of plates 2–4 fixed.
- **Nothing quantitative about corner sag.** "Slightly sagging" was never
  converted to a measurement, and no positive control exists — no specimen known
  to sag a known amount.
- **Nothing conclusive about plate position**, for the reason in §6.
- **Nothing about any resin but Sunlu Water-Wash ABS-Like Black**, on a
  practically new film, at one ambient condition that was not recorded.

## 9. Sources

- Reads and photographs: session `0010a938-3a51-4e43-8d1c-2db9590b4417`,
  2026-07-29. Six photographs are embedded in that transcript as JPEGs.
- Geometry, station coordinates, rung ladders:
  [`plate-generator-PLATE-LAYOUT.md`](../../evidence/plate-generator-PLATE-LAYOUT.md)
  and [`plate-generator-manifest.json`](../../evidence/plate-generator-manifest.json).
- The `PLATE 1 RESULT` gate note and the `p_eff` derivation were recorded inline
  in the layout document at the time; they are quoted here with that attribution.
- The generator that built this plate (`calibration-coupons/`, Python) was
  deliberately deleted 2026-07-31 and the language is now barred by a build
  guard. **The plate cannot be rebuilt from this repository.**
