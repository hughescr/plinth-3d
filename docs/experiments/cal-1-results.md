# CAL-1 print results

**Status:** Transcript of the owner's report on 2026-11-09. The owner has not yet confirmed it. Scoring below is proposed, pending owner confirmation and an independent review.

## Print record

| Field | Record |
|---|---|
| Print | `cal1.goo`, SHA-256 `28d7e12515674824e6bb3fa5725a1df2de819886b04b1b72e14df5e9808b4b2c` |
| Project | SHA-256 `91611c85a661f5caf77704898ff8b04ff5ef69c05d5d6b1b024b65a72ceb28b1`; seed `20261102` |
| Printer | Saturn 4 Ultra 16K |
| Resin | Anycubic WW ABS-Like Clear; label shows no lot number [O] |
| Temperature | Tank heated to 30 °C; 30–33 °C during print [O] |
| Vat film type | Not recorded |
| Blinding | The orchestrator disclosed the predictions to the owner before printing. The owner was asked to mark only what he saw. |

[O] is an owner observation or statement; [M] is measured from files; [S] is an orchestrator ruling or inference; [U] is an unverified model estimate. The [plan](../design/calibration-prints.md) defines the pieces and the reading rules.

## Observation record

[O] The plate was photographed with its front edge toward the camera. The pieces kept their map positions through washing and curing, but their rotation was not preserved. Linear pieces may have turned 180°; four-contact pieces may have turned 90°, 180° or 270°. Close inspection was after washing, drying and curing.

The outcome is the body or main piece, not the raft or supports. For W it describes the six-head witness as a whole. Break sites and outcomes are [O], except the waist classification, which is [S] from the observed remnants.

| Band | Piece | Outcome | Break site | Notes |
|---|---|---|---|---|
| Front | S-100 | Gone | Waist [S] | Raft and four cones remain. |
| Front | B-100 | Gone | Waist [S] | Raft and four cones remain. |
| Front | S-324 | Gone | Waist [S] | Raft and four cones remain; no ball formed on this waist; see W below. |
| Front | W | Present | Cannot tell | Before removal: balls on the 3 heads with 0.229 mm waists, none on the 3 with 0.125 mm waists (photo 14). One ball was lost in washing (photo 09). |
| Front | K-15 | Present | None recorded | Complete. |
| Front | K-60 | Present | None recorded | Complete. |
| Front | A0 | Partly present | Lower supports / bottom lip first | Disc present on top of support tree, deformed. |
| Front | A0-f2 | Partly present | Lower supports / bottom lip first | Disc present on top of support tree, deformed. |
| Back | S-100 | Gone | Waist [S] | Raft and four cones remain. |
| Back | B-100 | Gone | Waist [S] | Raft and four cones remain. |
| Back | S-324 | Gone | Waist [S] | Raft and four cones remain; no ball formed on this waist; see W below. |
| Back | W | Present | Cannot tell | Before removal: balls on the 3 heads with 0.229 mm waists, none on the 3 with 0.125 mm waists (photo 14). |
| Back | K-15 | Present | None recorded | Complete. |
| Back | K-60 | Present | None recorded | Present; the cap looks tilted in photo 07 — owner to check whether the arm is bent or cracked. |
| Back | A0 | Gone | Cannot tell | Disc fully detached; supports remain on plate. |
| Back | A0-b2 | Gone | Cannot tell | Disc fully detached; supports remain on plate. |

### Coupon contacts

[O] The owner said: “none of the bodies for S and B were attached to the supports on the plate - came out with supports only.” Each of the 24 cones ends in a short cylinder, the waist remnant, with no ball. The owner described the failure as “between the cylinder and the ball above in each case”; all S and B pieces in both bands look alike. [S] Score all 24 contacts as **broken at the waist**, the designed weakest section. A broken contact does not show when it broke (plan §2 rule 9).

### W formation witnesses

[M] Each W has three heads with 0.125 mm waists on one half and three with 0.229 mm waists on the other; the 0.229 mm heads are at the +X end (`crates/calibration/src/spec.rs`, `expected("W")`). [O] Photo 14 was taken from the back edge before any piece was removed. The order of the pieces in both rows matches the plate map turned 180°, which fixes each W's orientation. In both bands the three balls are at the +X end.

[O] Formation is observed, not inferred: balls formed on all six 0.229 mm heads (front 3/3, back 3/3) and on none of the six 0.125 mm heads (front 0/3, back 0/3) before removal. One front 0.229 mm ball broke off during washing (photo 09 shows 2). This matches the formation-load ratio (isolated ball area ÷ narrowest waist section, plan §3.2 [M]): about 4.4 for the 0.229 mm waist and about 14 for the 0.125 mm waist.

[S] Consequence: the formation rule censors C2 (no ball formed on the 0.125 mm waist) and C1 is scored (balls formed on the 0.229 mm waist in both bands).

### A0 and clean-out sheet

[O] On both front A0 copies the lower supports snapped. Each disc kept curing flat on the film as a glob. Upper supports later reached it and pulled the stack off the film at the end. A downward “chin” hangs below the level of the upper supports' layers. Its curvature identifies it as the disc's own bottom edge, not the rib ring; it formed while the lower supports held the disc.

[O] Calipers: front A0 flat part 1.45 mm, chin 4.55 mm; subtraction gives about 3.10 mm built before detaching, about 62 layers. Front A0-f2 flat part 1.49 mm, chin 3.78 mm; subtraction gives about 2.29 mm, about 46 layers. The owner notes that repeated exposures thicken the flat part, so these subtractions are approximate.

[U] For comparison only, not scoring: replay's worst additive overload is at the bottom lip, about 2.8 mm above its lowest point (layer 116 / contact 0; additive 311.221). This lies inside the 2.29–3.10 mm span of the two caliper estimates. That span is about as wide as the measurement uncertainty, so this is an observation, not agreement.

[O] The tank-clean sheet was photographed. It has one large blob near front/left, probably one or both back-band A0 discs that drifted, and smaller lumps. The owner says thin fragments are contamination from the tray. Nothing can be matched to a particular piece. The sheet is **inconclusive** and is not used for scoring.

[S] If the blob is a back A0 disc, the plan's premise that a failed piece stays at its own spot (plan §2 rule 5, §7.3) may not have held for that disc. No score here depends on it: the back-band coupons, which a front-left drift would not reach, failed the same way as the front ones; K is present in both bands; all A0 copies failed anyway. The front A0 discs, which stayed on the film at their own spot, agree with §7.3.

[O] Support removal by zone, easy / normal / hard (PRD §6.4), was not recorded. The front A0 discs were deformed failures; this removal assessment is not applicable / not recorded.

## Proposed contrasts

**Control context for every result below:** [O] K-15 was present and complete in both bands, reproducing plate 6. K-60 was present and complete in both bands, so it did **not** reproduce plate 6, where it was gone. [S] This does not void the A0 or coupon results (plan §8.1).

- **C1 — B-100 versus S-100:** [O] Front (B-100, S-100) = (gone, gone); back = (gone, gone); [S] their contacts broke at the waist. [O] W shows balls formed on the 0.229 mm waist in both bands (3/3 each, photo 14). [M] Assembled as-sliced indices, lit-any / weighted: B 98.036 / 97.005; S 96.944–98.065 / 96.667–97.785. [S] There is no survival separation at this tested level. The compact-law clear-fail calls at ≥92 are confirmed on this plate for both shapes in both bands (one piece of each per band), conditional on the waist-break classification [S].
- **C2 — S-324:** [O] Both bodies were gone. [O] W shows no ball formed on the 0.125 mm waist in either band, before removal (photo 14). [S] Under the formation rule (plan §§3.2, 5 and 8.3), these failures do not count as load failures. They are censored, not scored (plan §2 rule 4). A later plate is needed.
- **C3 — four A0 copies:** [O] All four failed: the front copies at the lower supports / bottom lip first; the back discs were fully detached, and their failure sites cannot be determined from the remains. [S] The compact-law clear-fail call is confirmed for 4/4 copies on this plate; the additive replay value 311.221 is [U]. The owner's 2026-11-01 judgement that Light prints this base fine [O] is contradicted for this layout on this plate only (plan §2 rule 8). The two front copies are marked partly present in the table because the disc came away from its lower supports and was deformed; for C3 that counts as failed.
- **C4:** Deferred in the plan; no A1 pieces or C4 score on CAL-1.

**Intervals:** None. No strip coupon survived in either band, so there is no pass/fail edge to state (plan §8.2). C2 is also censored. No design ratio is derived (plan §8.4).

## What this does not show

- This plate does not establish a global rule or eliminate a predictor. It does not isolate strip width from the other differences between the coupons (plan §§2 and 3.2).
- The W pieces show whether a ball forms under its own load only. They do not show at what load a formed ball's waist breaks.
- The sheet does not identify the lost discs. The measurements do not establish when any coupon contact broke.
- K-60 survived here although it failed on plate 6. That suggests this plate was not harsher than plate 6 at that index. It does not explain away the A0 or coupon failures, and it is reported beside every result (plan §8.1). This plate does not replace the PRD §8.4 side-by-side print.

## Photos

Copies are in `plate-photos/cal-1/` (HEIC originals converted to JPEG, longest side 2000 px; the 14a/14b crops are cut at full resolution).

- Plate: `plate-photos/cal-1/01-plate-top.jpeg`, `plate-photos/cal-1/02-plate-angled.jpeg`.
- Front: `plate-photos/cal-1/03-front-S100-B100-W.jpeg`, `plate-photos/cal-1/04-front-A0f2-A0-K60.jpeg`, `plate-photos/cal-1/05-front-K60-K15-S324.jpg`.
- Back: `plate-photos/cal-1/06-back-A0b2-A0.jpg`, `plate-photos/cal-1/07-back-K60-W-S324-B100-S100.jpg`, `plate-photos/cal-1/08-back-K15.jpg`.
- Close-ups: `plate-photos/cal-1/09-front-W-closeup.jpg`, `plate-photos/cal-1/10-back-W-closeup.jpg`, `plate-photos/cal-1/11-S100-tips-closeup.jpg`, `plate-photos/cal-1/12-front-A0-profile-chin.jpg`.
- Clean-out: `plate-photos/cal-1/13-clean-out-sheet.jpeg`.
- Before removal, from the back edge: `plate-photos/cal-1/14-plate-from-back-before-removal.jpg`; W crops from the full-resolution original: `plate-photos/cal-1/14a-back-W-crop.jpg`, `plate-photos/cal-1/14b-front-W-crop.jpg`.

## Open / owner actions

- Owner: confirm or correct this transcript. Independent review of the proposed scoring remains open.
- Any ruling on A2 (full-area load for strips) or P5 belongs to the owner (plan §8.4). None is proposed here.
