# CAL-2 print results

**Status:** Written 2026-11-11 from the owner's observation of the same day: the plate was inspected after the print and before washing. Wash, cure and support-removal results are not in yet (see Pending). Scoring follows the decision reads pre-registered in [`docs/design/calibration-prints.md` §12](../design/calibration-prints.md); no new criterion is added here. The counts below are read from `cal2-report.json` (the coupon list) and the owner's two failures.

## Print record

| Field | Record |
|---|---|
| Print | `cal2-plate.goo`, SHA-256 `38a739fc42984b475a2f8a1841702b85949f4d7717e5025c3b495abc4978a48a` (`plinth-review/2026-11-10-cal-2/`) |
| Project | `CAL2-plate-calibration-not-print-candidate.chitubox`, SHA-256 `7260e91904c8ce59c6b1cf011f523b28f1e2a0901b6e991c670a40a944a09be1`; layout seed `20261110`; 67 coupons, nothing cut for fit |
| Printer | Saturn 4 Ultra 16K |
| Resin, temperature, vat film type | Not recorded for this print |
| Slice settings | [M] Identical to CAL-1 in every setting (`goo-check.md`): 0.05 mm layers, 2.35 s exposure, 3 bottom layers at 20 s, 5 transition layers, lift and retract 0.05 mm. 208 layers; header print time 1694 s (the file's own estimate, not a measured run) |
| File check before printing | [M] `goo-check.md`: PASS on all 7 checks; 67 of 67 coupons present, 0 islands, 0 supports ending early, last lit layer exact on all 67 |
| Inspected | 2026-11-11, by the owner, on the plate, before washing |
| Blinding | Not recorded. The decision reads in §12 were written before the print. |

[O] is an owner observation or statement; [M] is measured from files; [S] is an orchestrator ruling or inference. The plan and the reading rules are in `docs/design/calibration-prints.md` §12.

"Ratio" in this document is the label ratio of §12: plinth's load model, at the peak layer, against the certified patch. The ratio against the written ball's own section is 2.6 to 3.0% lower for plinth's heads, and the ratio against the section where the cone leaves the ball is 8 to 11% lower (both are in `cal2-report.json`). The labels are used throughout, as the limits (12 and 40) are set on the same definition.

## Observation record

[O] 65 of 67 coupons printed. Everything adhered to the plate. Exactly two coupons failed. Both detached from their supports: the support columns stayed on the plate, and the break is at the support-to-body connection. The three formation witnesses (C65, C66, C67) formed on the plate.

"Printed" in this document means: the whole coupon was present on the plate at this inspection, before washing. It does not yet include losses in washing or curing.

| Coupon | Row | Variant | Heads × neck | Target / achieved ratio | Zone | Copy | Outcome |
|---|---|---|---|---|---|---|---|
| C57 | R6 | standard | 4 × 0.80 mm | 70 / 69.95 | back | 1 | **Failed: detached at the support-to-body connection** |
| C58 | R6 | standard | 4 × 0.80 mm | 70 / 69.95 | front | 2 | Printed (twin of C57) |
| C32 | R3a | short hold | 9 × 0.55 mm | 50 / 49.90 | back | 2 | **Failed: detached at the support-to-body connection** |
| C31 | R3a | short hold | 9 × 0.55 mm | 50 / 49.90 | middle | 1 | Printed (twin of C32) |

[O] All other coupons, C01 to C56, C59 to C64 and the witnesses C65 to C67, were present and attached to their supports. No other break site was reported.

[M] C57 sits at the far left (−X) end of the back row (centre x −92.2 mm, y +30.9 mm). C32 sits at x +45.5 mm in the same row. They are not neighbours.

## Tally

### By target ratio

| Target ratio | Coupons | Printed | Failed |
|---|---|---|---|
| 6 (witnesses) | 3 | 3 | 0 |
| 25 | 6 | 6 | 0 |
| 30 | 4 | 4 | 0 |
| 35 | 6 | 6 | 0 |
| 40 | 20 | 20 | 0 |
| 45 | 4 | 4 | 0 |
| 50 | 16 | 15 | 1 (C32) |
| 70 | 8 | 7 | 1 (C57) |
| **All** | **67** | **65** | **2** |

Seen together: ratio 25 to 45, 40 coupons, 40 printed, 0 failed. Ratio 50 and 70, 24 coupons, 22 printed, 2 failed. Ratio 40 or higher, 48 coupons, 46 printed, 2 failed.

### By row

Printed / failed. Every cell is two copies (the witnesses are one each).

| Row | What it is | Coupons | Printed | Failed |
|---|---|---|---|---|
| R1 | 0.40 mm neck, 16 heads, standard | 14 | 14 | 0 |
| R2 | 0.55 mm neck, 9 heads, standard | 14 | 14 | 0 |
| R3a | 0.55 mm, 9 heads, short hold | 4 | 3 | 1 (C32) |
| R3b | 0.55 mm, 9 heads, long hold | 4 | 4 | 0 |
| R4 | 0.55 mm, 9 heads, late load | 4 | 4 | 0 |
| R5a | 0.55 mm, 8 heads, strip | 4 | 4 | 0 |
| R5b | 0.55 mm, 12 heads, ring | 4 | 4 | 0 |
| R6 | 0.80 mm neck, 4 heads, standard | 10 | 9 | 1 (C57) |
| R7 | 1.20 mm waisted column, 4 heads | 4 | 4 | 0 |
| R8 | 0.55 mm, 4 heads, control | 2 | 2 | 0 |
| R9 | formation witnesses (0.40, 0.55, 0.80 mm) | 3 | 3 formed | 0 |

### By row and ratio

Printed / failed, two copies per cell.

| Row | 25 | 30 | 35 | 40 | 45 | 50 | 70 |
|---|---|---|---|---|---|---|---|
| R1 | 2/0 | 2/0 | 2/0 | 2/0 | 2/0 | 2/0 | 2/0 |
| R2 | 2/0 | 2/0 | 2/0 | 2/0 | 2/0 | 2/0 | 2/0 |
| R3a | | | | 2/0 | | **1/1** | |
| R3b | | | | 2/0 | | 2/0 | |
| R4 | | | | 2/0 | | 2/0 | |
| R5a | | | | 2/0 | | 2/0 | |
| R5b | | | | 2/0 | | 2/0 | |
| R6 | 2/0 | | 2/0 | 2/0 | | 2/0 | **1/1** |
| R7 | | | | 2/0 | | | 2/0 |
| R8 | | | | 2/0 | | | |

No cell lost both copies.

### By plate zone

Excluding the three witnesses (64 coupons). Each cell has copies in different zones, so a zone is not tied to a row.

| Zone | Coupons | Printed | Failed |
|---|---|---|---|
| Front | 20 | 20 | 0 |
| Middle | 20 | 20 | 0 |
| Back | 24 | 22 | 2 (C57, C32) |

The coupons at ratio 50 or higher, by zone:

| Zone | Coupons at ratio ≥ 50 | Printed | Failed |
|---|---|---|---|
| Front | 9 | 9 | 0 |
| Middle | 7 | 7 | 0 |
| Back | 8 | 6 | 2 |

The eight back-zone coupons at ratio 50 or higher:

| Coupon | Row | Ratio | Outcome |
|---|---|---|---|
| C57 | R6 | 70 | Failed |
| C13 | R1 | 70 | Printed |
| C28 | R2 | 70 | Printed |
| C32 | R3a | 50 | Failed |
| C26 | R2 | 50 | Printed |
| C55 | R6 | 50 | Printed |
| C36 | R3b | 50 | Printed |
| C44 | R5a | 50 | Printed |

So 6 of the 8 back-zone coupons at 50 or higher survived, including a 0.40 mm and a 0.55 mm coupon at 70. See "What this sample can and cannot show" for how far to read this.

## Decision reads (§12)

Each item gives the registered read, what the plate shows, and the score. "Survives" means the whole coupon is intact, as in CAL-1 §7.4. All outcomes are pre-wash.

### Limits read: where do head breaks start (R1, R2, R3 to R8)

- Registered: any head break at ratio 40 or lower means lower the unprintable limit (40). All copies intact at 50 and none failing at 40 means 50 is a candidate for the limit, pending a second plate. Between those, the break ratio is the first rung that fails in both copies.
- Observed: at ratio 40 or lower, 0 failures (39 coupons including the witnesses; 36 without them). At 50, 15 of 16 survived; the one failure is C32 (R3a, short hold). No cell lost both copies at any rung up to 70.
- Score: **No break at or below 40, so the limit is not lowered. "All copies intact at 50" is not met (15 of 16), so 50 does not become a candidate on this plate. No rung failed in both copies, so no break ratio is located; it lies above 70 for R1, R2 and R7 (survived 2 of 2 at 70), and is not located for R6 (1 of 2 at 70).** In the standard rows alone (R1, R2, R6) all six coupons at 50 survived, but the registered read is on all copies at 50, and that is not met.

### Fatigue (R3b long hold against R2 standard and R3a short hold)

- Registered: the long hold (R3b) fails where the standard (R2) and short hold (R3a) survive at the same ratio, 40 or 50, means the limit depends on how many layers sit near the peak load. All three alike means no fatigue effect at these layer counts.
- Observed: R3b (100 layers at full area) survived 4 of 4: 2 of 2 at 40, 2 of 2 at 50. R2 survived 4 of 4 at 40 and 50. R3a (5 layers at full area) survived 3 of 4: 2 of 2 at 40, 1 of 2 at 50 (C32 failed, C31 survived).
- Score: **The fatigue trigger (R3b fails where R2 and R3a survive) did not happen. R3b did as well as R2. The three are not strictly alike, because R3a lost one coupon at 50, but that failure is on the short-hold, the variant with the least repeated strain. It goes against the fatigue direction.** Read as: no fatigue effect seen at 100 layers (5 mm) at ratio 50. The R3a failure is scatter or an unrelated cause, not evidence for or against strain accumulation (see the next section).

### Cure age (R4 late load against R2 standard)

- Registered: the late-load coupon (R4) survives where the standard (R2) fails at the same ratio means a neck loaded right after it forms is weaker. Both alike means no cure-age effect at 2 mm of age. A late-load survival is stronger evidence than a late-load failure, because the late-load pads are separate until the slab.
- Observed: R4 survived 4 of 4 (2 of 2 at 40, 2 of 2 at 50). R2 survived 4 of 4 at 40 and 50.
- Score: **Both alike. No cure-age effect seen at 2 mm of age.** R2 never failed at 40 or 50, so this plate could not show a benefit from ramping the load. It can only show that the late-load design did no harm.

### Shape (R5a strip and R5b ring against R2 square)

- Registered: the strip or the ring differs from the square at the same ratio (40 or 50) means shape matters beyond area and section.
- Observed: R5a (strip, 8 heads) survived 4 of 4. R5b (ring, 12 heads) survived 4 of 4. R2 (square, 9 heads) survived 4 of 4 at the same ratios.
- Score: **No difference. No shape effect seen.** The strip and the ring have 8 and 12 heads, the square has 9; the registered read does not separate that from shape.

### Large sections (R6 0.80 mm and R7 1.20 mm against R1 and R2)

- Registered: the neck sizes (0.40, 0.55, 0.80, 1.20 mm) break at different ratios means the ratio-only model does not scale with section size.
- Observed: R1 (0.40 mm) 14 of 14 and R2 (0.55 mm) 14 of 14, both including 2 of 2 at 70. R7 (1.20 mm waisted column) 4 of 4, including 2 of 2 at 70. R6 (0.80 mm) 9 of 10: 8 of 8 at 25, 35, 40 and 50, and 1 of 2 at 70 (C57 failed, C58 survived).
- Score: **No neck size has a rung that failed in both copies, so a different break ratio for different sizes is not shown.** The one large-section failure is R6 at 70, in one copy of two; the other three sizes survived 6 of 6 at 70. That is a difference in outcome at 70 between R6 and the rest; one coupon of two cannot show it is real. The large-section coupons at 40 and below (R6: 6 of 6; R7: 2 of 2) all survived, so the plate gives no sign that thick sections carry load worse than the rule says at those ratios.

### Controls (R8 four-head 0.55 mm against R2 nine-head 0.55 mm at 40)

- Registered: R8 differs from R2 at ratio 40 means load is not shared evenly across heads, so the additive rule over-counts the heads.
- Observed: R8 survived 2 of 2. R2 at 40 survived 2 of 2.
- Score: **Alike. No sign that the additive rule mis-counts heads at ratio 40.** Nothing at 40 failed, so there was nothing to differ from.

### Other registered reads

- **Rectangles at 0.40 mm, R35 and R40 (C05, C06, C07, C08) against the squares at R30 and R45.** Registered: a difference from the neighbours' trend means suspect body shape or head pitch. Observed: all eight R1 coupons at 30 to 45 survived, rectangles and squares alike. **No difference.**
- **Formation witnesses (R9, ratio about 6, read on the plate before washing).** Registered: a witness that does not form means that head size cannot be made, and the other cells of that size are untested. Observed [O]: C65 (0.40 mm), C66 (0.55 mm) and C67 (0.80 mm) all formed. **All three head sizes can be made, so the R1, R2 and R6 cells are valid tests and none is marked untested.** (R7 is a plain column and has no witness.)

## What this sample can and cannot show

- **Two copies per cell.** One copy lost out of a pair is not a rate. C32 and C57 each have a surviving twin (C31, C58) made the same way. With one failure and one survivor in the pair, the plate cannot say whether the true failure chance of that cell is high, low or zero. A cell needs both copies to fail before §12 reads it as a break.
- **The short-hold failure runs the wrong way for a strain effect.** R3a (5 layers at full area) has the least repeated strain of the three 0.55 mm 9-head variants at ratio 50. The long hold (R3b, 100 layers) survived both copies. If strain accumulation or cure age were the cause, the long hold should fail before the short one. So C32 reads as scatter. It is not evidence for fatigue, and it is not evidence against fatigue or cure age either. This plate cannot tell the two readings apart with one failure.
- **Both failures are in the back zone; do not over-read it.** Of the 24 coupons at ratio 50 or higher, 8 are in the back zone and 6 of those 8 survived (see the table above). The front and middle zones had 16 such coupons and 0 failures. If two failures fell at random among those 24 coupons, both would land in the back zone about 1 time in 10 (28 of 276 pairs). That is a look at one grouping chosen after the fact (zone), with 24 coupons; copy number, row, and position along X gave no pattern at all (copy 1 and copy 2; R6 and R3a; x −92 mm and +45 mm). Back-zone coupons at 70 include two survivors (a 0.40 mm and a 0.55 mm coupon). The back zone may be harsher at high ratio; this plate does not show it. A later plate with more copies in the back zone would.
- **Failures are only at ratio 50 and 70.** Neither failure is below the unprintable limit. A single break at 50 or 70 does not by itself define a break ratio.
- **This is the state before washing.** "Printed" means on the plate at inspection. Washing and curing may remove more coupons, and scarring and removal ease are not known yet. The counts may change.
- **The three zones do not isolate position.** Each cell has its copies in different zones, so zone is not tied to a cell, but zone, copy and position along X were not designed to be tested separately.
- **Label ratio only.** The labels are against the certified patch. Against the section where the cone leaves the ball, every ratio is 8 to 11% lower for plinth's heads, so an "R70" coupon is about R62 to R64 on that definition. This does not change the reads above.
- **One plate, one run.** It does not separate resin, vat film, or temperature from the support design, and none of those is recorded for this print. This plate is not a print candidate for any model and is not a global rule.

## Bearing on the limits

The limits are not changed here. The unprintable limit stays at 40 (owner ruling, 2026-11-10) and the design limit stays at 12. This is what the evidence says.

- **Unprintable limit, 40.** At ratio 40 exactly: 20 printed, 0 failed, across ten cells, with every variant and every neck size from 0.40 to 1.20 mm. At ratio 25 to 45: 40 printed, 0 failed. Above 40 (45, 50, 70): 28 coupons, 26 printed, 2 failed, and both failures are at 50 or higher. So no coupon failed at or below the limit, and nothing here pulls the limit lower. The rule in §12 for moving the limit up (every copy at 50 intact) is not met, so nothing here supports raising it either. Ratio 70 saw 7 of 8 survive, so 40 leaves margin below where this plate found any break, and the plate does not locate a break ratio.
- **Design limit, 12.** The plate does not test 12. The lowest ratio carrying supported peel load on this plate is 25 (6 of 6 printed). The witnesses at ratio about 6 formed, but they show only that the head forms, not how it carries load. Everything at 25 and above survived except the two coupons at 50 and 70. The design limit therefore sits well below every ratio tested, and the plate cannot say whether 12 is the right amount of margin.
- **Resin.** PRD P4 notes that older losses at lower ratios (plate 6 on black resin; H2, resin not recorded) conflict with newer survivals on ABS-like clear, and that CAL-2 was meant to help settle it. The resin for this print is not recorded in this document. If it was the same ABS-like clear as CAL-1 and the Round 25 print, these survivors add to the newer side. This plate does not show resin as the cause.

## Pending

Not in yet. Fill these in when the owner reports; do not change anything above except the "Printed" counts if washing or curing removes coupons.

- Wash and cure: coupons lost or damaged in washing or curing (which numbers), and the witnesses' fate after removal. A witness lost after it was read on the plate counts as nothing (§12).
- Scarring on the body faces where supports were removed, by coupon, for the necks (0.40, 0.55, 0.80, 1.20 mm) and the ratios.
- Support removal ease by zone and neck size (easy, normal, hard), as in PRD §6.4.
- The break site on C57 and C32, close up: whether it is at the cone tip, the ball, or the body face. The owner's report is "at the support-to-body connection".
- Resin, lot (if labelled), tank temperature, vat film type.

## Photos

Copies are in `plate-photos/cal-2/`.

- Plate from above, after printing and before washing: `plate-photos/cal-2/plate-above-after-print.jpg` (2400 × 1391 px). Two positions in this photo show faint support columns with no body on them; the owner's report names C57 and C32 as the two detached coupons. The photo is not yet matched to the plate map (`plate-map.png` in `plinth-review/2026-11-10-cal-2/`), so these positions are not tied to coupon numbers here.
- Not yet taken: close-up of the C57 and C32 support columns, and the twins C58 and C31 for comparison.

## Owner ruling

None yet for CAL-2. No change to the unprintable limit (40) or the design limit (12) is made or proposed here.
