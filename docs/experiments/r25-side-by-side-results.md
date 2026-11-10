# Round 25 side-by-side print (PRD §8.4): record and frozen predictions

**Status:** Written on 2026-11-09, after the print started and before any result. The predictions below are frozen; the results section is a blank template. Later edits fill the template only. They do not change a prediction.

[O] is an owner observation or statement; [M] is measured from files; [S] is an orchestrator ruling or inference; [U] is an unverified model estimate. Support figures from the adapter (`plinth-review/2026-11-09-three-ways*/`) are [U] where noted: its notes say rotated ChituBox parts failed native validation, so fallback patch bounds are unverified.

## Print record

| Field | Record |
|---|---|
| Print | `round-25-three-ways.goo`, SHA-256 `9328d75967e49e20c75b26fcc158881bce692be1ac8722b58659214b523a6ec6` (`plinth-review/2026-11-09-final-plate/`) |
| Project | `round-25-three-ways.chitubox`, SHA-256 `66935f5163295c0d005edbdc0f337696f8ae4c628801c8a39e198f10283e74db` |
| Plinth source file | `Round 25.keepup.chitubox`, SHA-256 `04dfab93c46ca4a41e551682d34b81760140ed9b6735022b36e11f47c57776d5`, built by develop code (develop is now at 556a453) |
| Started | 2026-11-09 [O] |
| Printer, resin | Saturn 4 Ultra 16K; Anycubic WW ABS-Like Clear; lot not labelled [O] |
| Temperature | Tank 30 °C; 30–33 °C during print [O] |
| Vat film type | Not recorded |
| Slice settings | [M] 565 layers at 0.05 mm; exposure 2.35 s; 3 bottom layers at 20 s, 5 transition layers; lift and retract 0.05 mm; software 0.0.22; plate size 211.68 × 118.37 × 220 mm |
| Header estimates | [M] print time 3936 s (1 h 5 min 36 s); volume 7123.25 mm³; weight 8.19 g. These are the file's own numbers, not a measured run |
| Blinding | Not blind. The orchestrator told the owner the qualitative predictions and the per-version weak spots in chat, before and during the print. [O] His own prior was low confidence in both ChituBox variants. The predictions below were recorded before any result. |

## Plate map

[O] Plate = 6 copies of Round 25: 2 plinth (65°), 2 ChituBox "Better" at plinth's 65° orientation, 2 ChituBox "Better" after ChituBox auto-rotate (about 89°). Each version has one copy in the front half and one in the back half. The owner gave the pairs different Z rotations so the peel direction differs (the Saturn 4 Ultra tilts its vat). Front is −Y in ChituBox [O] (`docs/design/calibration-prints.md`).

Bounds are `goo-analysis.json` expected bounds [M], X and Y in mm on the plate. Height is above the plate and includes the lift. "Turn" is the rotation of that copy against its slice-reference model (`rigid-comparison.json` [M]); the absolute Z rotation is not in any file read. Fill it in from the project.

| Model | Version | Half | X range | Y range | Height | Lift | Turn vs reference | Absolute Z |
|---|---|---|---|---|---|---|---|---|
| 0 | Better 65° | Back (+Y) | 44.37 to 69.37 | 22.93 to 37.62 | 26.23 | 3 | 0° | fill in |
| 1 | Better 65° | Front (−Y) | 49.42 to 72.26 | −27.05 to −7.70 | 26.23 | 3 | 102.00° | fill in |
| 2 | Plinth 65° | Back (+Y) | −47.37 to −21.17 | 18.27 to 35.17 | 28.23 | 5 | 0° | fill in |
| 3 | Plinth 65° | Front (−Y) | −47.86 to −28.38 | −33.52 to −9.17 | 28.23 | 5 | 0° | fill in |
| 4 | Better auto | Back (+Y) | −3.65 to 15.83 | 18.87 to 38.28 | 28.00 | 3 | −153.47° | fill in |
| 5 | Better auto | Front (−Y) | −5.88 to 18.00 | −26.45 to −12.97 | 28.00 | 3 | 0° | fill in |

[M] The nearest two copies are 18.69 mm apart (models 2 and 4, convex-hull lower bound); no pair touches. The slice shows zero islands on models 0 to 3, and exactly one on each of models 4 and 5 (below).

## Support summary by version

Counted from the support records unless marked. Copies of one version agree except where stated. Rows marked [U] are adapter estimates, not measurements; "area" is the model's cross-section in the slab at the layer named. The plinth load model is uncalibrated between ratio 12 and about 97, and the adapter could not validate many rotated ChituBox supports.

| Version | Tips [M] | Ball diameters, mm [M] | Neck diameter estimate, mm [U] | Peak model area, mm² (Z) [U] | Active tips at peak | Area per tip, mm² [U] | Area per mm² of neck [U] |
|---|---|---|---|---|---|---|---|
| Better 65° | 17 | 0.20 × 7; 0.80 × 10 | 0.10 × 5; 0.15 × 2; 0.66–0.70 × 10 | 52.24 (17.5) | 10 | 5.22 | 23.87 |
| Plinth 65° | 39 | 0.54–0.83; 0.83 × 19 | 0.51–0.81; 0.79 × 20 | 52.24 (19.5) | 28 | 1.87 | 4.40 |
| Better auto | 21 | 0.20 × 15; 0.80 × 6 | 0.10 × 8; 0.15 × 7; 0.66–0.69 × 6 | 48.87 (12.9) | 13 | 3.76 | 22.12 |

Worst additive layer [U] (design ratio 12): Better 65° ratio 43.63 at Z 12.3 mm (layer 185), 8 active tips, 6.40 mm² per tip; plinth 17.33 at Z 6.65 mm (layer 32), 4 active tips, 5.80 mm² per tip, lowest lip, median over all contacts 6.95; Better auto 124.44 at Z 11.4 mm on one tip of a 0.95 mm² component the adapter did not assess, and 24.02 on the main body there, median 43.92. Raft-layer lit area [M]: Better 65° 89.5 and 90.0 mm²; plinth 181.0 mm² each; Better auto 80.7 mm² each. Largest lit area above the lift [M]: 59.3 and 59.1; 71.2 and 71.2; 49.1 and 49.2 mm².

[M] `goo-analysis.json` counts islands of 2 pixels or more with no overlap in the layer below. Better auto copies (models 4 and 5) have one each: layer 228, Z 11.40 mm, 73 and 72 pixels (about 0.02 mm²), at the disc's local Y about +1.4 mm, 0.23 mm from support records 147 and 148. That is about 8.4 mm above the disc's lowest edge (11.40 minus the 3 mm lift). The slice shows lit material there with nothing under it in the layer below; it does not show whether a support catches it.

## Frozen predictions

Qualitative on purpose. The numbers above are inputs, not calibrated forecasts. [S] for all.

1. **Plinth prints best.** Both plinth copies should print whole. The load estimate is far lower than for either ChituBox version (about 4.4 mm² of model per mm² of neck, against 22 to 24). If a plinth copy weakens at all, the first place is the **lowest lip supports** (worst additive about 17 against the design 12, at Z 6.65 mm with only 4 tips active). This ratio lies in the range plate CAL-1 did not test (12 to about 97), so I do not predict whether 17 holds.
2. **Better 65° is the most likely to fail or deform.** Its median additive ratio is about 43 across tips (3.6 times the design value) and 7 of its 17 tips have 0.10 to 0.15 mm necks. Expected first weak spot: the **lowest lip supports**, as for the Light copies on CAL-1 (4 of 4 failed at 65°). The contact count equals the CAL-1 Light piece (17); I did not compare the supports.
3. **Better auto is also at risk, at different places.** Expected first weak spots: the **lower rim** (15 of its 21 tips have 0.10 to 0.15 mm necks), and the **small inner-face component** (the unassessed 0.95 mm² piece with additive 124). Expected slice defect: the **0.02 mm² island at Z 11.40 mm** on each auto copy. I expect it to be a small blemish, or to be missing, rather than to bring the disc down. I make no prediction on whether the same feature is the island and the component; they sit at the same height.
4. **No ranking of the two ChituBox versions.** The load model puts their medians at about 43 to 44 for both. The owner's prior is low confidence in both. I only predict that neither beats plinth.
5. **No prediction on front against back.** The front (−Y) releases first and moves farthest [O], and the Z rotations differ. One copy per cell, so any front/back difference is a lead, not a result.
6. **Run time** should be near the header's 3936 s. Plate fit is not in doubt.

## Results (fill in after the print)

Fill in every cell, including "none". Do not rewrite the predictions. Date and time of inspection: ______. Inspector: ______.

Per copy, for models 0 to 5 in turn (6 blocks of this form; the plan shows block 0):

| Field | Model 0: Better 65°, back |
|---|---|
| Printed or failed; where | |
| Supports snapped; where (tip / mid / base) | |
| Removal ease (easy / normal / hard) and damage | |
| Extra smoothing, and time | |
| Manual edits, and time | |
| Resin; vat film | |
| Print height measured | |
| Run time | |
| Was the failure or weak spot at a spot the file warned about? | |
| Did the auto-copy island at Z 11.40 mm show in the part? (models 4, 5) | |
| Photos | |

Counts by version (PRD §8.4): printed ___ / failed ___ / waived ___; failures at a warned spot ___; failures at an unwarned spot ___; warnings that needed action ___; warnings on regions that printed fine ___. A blanket of warnings does not prove prediction.

## Contrasts to score

Each version has two copies. Score a contrast only on outcomes both copies of a version share; a split pair is reported as a split (1 of 2).

- **A. Plinth against Better 65°.** Same model, same tilt; isolates the support design. Confounds: lift 5 against 3 mm, different Z rotations, one copy per half.
- **B. Plinth against Better auto.** Isolates support design plus ChituBox's own choice of orientation.
- **C. Better 65° against Better auto.** The orientation effect on ChituBox's own supports. Confounds: both orientation and its generated supports change together.
- **D. Front against back**, within each version. Peel direction and position only; one copy per cell.

Score each contrast: plinth better / no difference / ChituBox better / cannot tell. For each result, say whether plinth warned about the spot, and tick which prediction above it supports or breaks.

## What this will not show

- Evidence about Round 25 on this resin, this tank and this plate only (PRD §8.4). It is not a global rule and not a calibration of the load model.
- Two copies per version, one run. It cannot separate resin, film, position and rotation from the support design.
- The adapter figures for ChituBox's supports are estimates; a result that disagrees with them is a finding about the adapter first.
- The plinth copies' pass on PRD §8.3 items 1 to 7 is not restated here; this document records the print, not the sign-off.

## Photos

Planned folder: `plate-photos/r25-side-by-side/` (JPEG, longest side 2000 px). Take all of these; mark any not taken.

- Plate from above and from the front edge, before removal and before washing.
- Plate from the back edge, before removal.
- One close-up of each copy's lowest lip supports and lower rim, as printed, with the model number visible.
- Models 4 and 5: the disc at Z 11.40 mm (inner face), with a coin or ruler for scale.
- Each copy after support removal, and any smoothed spot.
- Any snapped support, and the tank-clean sheet.
