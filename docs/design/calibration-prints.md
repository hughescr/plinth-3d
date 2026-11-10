# Calibration print plan: Light on Round 25, and thin strips against compact bodies (plate CAL-1)

Status: CAL-1 was printed on 2026-11-09. Results are in `docs/experiments/cal-1-results.md`. Owner ruling, 2026-11-09: keep P5 at 12 and keep A2 (§8.4). The sections below are the plan as frozen before printing and are not rewritten. It was released for the print request on 2026-11-02. Project SHA-256 `91611c85a661f5caf77704898ff8b04ff5ef69c05d5d6b1b024b65a72ceb28b1`; slice (GOO) SHA-256 `28d7e12515674824e6bb3fa5725a1df2de819886b04b1b72e14df5e9808b4b2c`. Replacement native slice measured and both hashes frozen. All 16 pieces start at layer 0, with zero raster islands; coupons, K and W pass the stated checks. Stored estimate: **5.862 mL / 1 h 1 min 25 s [M]**. The 14 unchanged pieces are pixel-identical to the previous slice. The two second-A0 copies are not exact translated native-raster matches (25,036 front / 65,852 back differing grey pixels); binary differences stay within one pixel row. The orchestrator accepted these differences as slicer rasterization variation under R-REP (§6.6), so the four A0 copies are replicates with this variation recorded. Final review: cross-family PASS (gpt-sol measured, Claude Opus reviewed), as ruled by the orchestrator. R-C4's preregistered fallback gives four A0 copies; C4 is deferred to §10. Written-file replay was cross-family reviewed (gpt-sol built, Claude Opus reviewed, PASS). Owner rulings are in §6.5; orchestrator rulings are separately in §6.6. This informs FR-20a (`docs/design/step11-construction.md` §4) and PRD §7 (P4, P5); it does not calibrate the whole model.

**Labels on numbers** (as in the step-11 design):

- **[O]** owner ruling or owner practice;
- **[S]** orchestrator choice or ruling, not owner approval or measured evidence;
- **[M]** measured (on this printer, or on a written file);
- **[C]** ChituBox convention or vendor data;
- **[U]** unmeasured estimate; never decides alone.

Written counts, footprints and sections labelled **[M]** are measurements of emitted files. Native coupon indices and stored volume/time are measured for the frozen replacement slice [M]; replay indices remain model estimates [U], not physical loads. The project and slice have separate frozen hashes; the translated-raster differences are accepted as slicer rasterization variation under R-REP (§6.4 item 5, §6.6). Never transfer a sliced hash or print measurement to changed geometry. Never silently change a rung ID or a preregistered rule.

---

## 0. Why this plate

The owner decided on 2026-11-01 to calibrate the load model with test prints before more sizing rounds **[O]**.

The owner judges that ChituBox's own auto-supports would print the Round 25 base well **[O]**. That is his judgement only: this 65° copy with these supports has never been printed (owner, 2026-11-01). A0 is therefore an untested layout, and this plate is its first test.

The replay adapter (`plinth-slots/bench/crates/cli/examples/magic.rs`, uncommitted) read the owner's file `round 25.chitubox-magic-supports.chitubox`. These are model outputs on the written file **[M]**, not physical loads:

| Quantity | ChituBox Light | ChituBox Better | Plinth design ratio (P5) |
|---|---|---|---|
| Contacts | 17: 10 main (ball 0.5 mm, cone start 0.40 mm, column 1.2 mm) + 7 branch (ball 0.2, rod 0.10) | 17: 10 main (ball 0.8, cone start 0.60, column 1.5) + 7 branch (ball 0.2, rod 0.10–0.15) | — |
| Largest rigid-lever ratio | **2,293**, on the 0.10 mm rods of two branch supports under the lower underside flank | **1,211**, on a patch at its first printed layer | 12 |
| Largest additive group ratio | **313**, bottom lip, about 2.8 mm above its lowest point; held by three patches of 0.03–0.04 mm² | **44** | 12 (P4 breaking about 25) |
| Layer groups the rigid-lever solve cannot balance | 125, layers 0–171 | 143, layers 0–171 | — |
| Plinth checker | H9: parts 0.098 mm across flats; H1, H2, H6 unverified (open joint-ball mesh) | H1, H6, H9 violations | — |

Two readings (not exclusive; both can be partly true):

1. **Load is overstated for thin strips.** The model loads contacts with the full cured layer area (assumption A2). P4 measured breaking near 25 on flat caps 2–12.5 mm across. Round 25 layers are strips 1–4 mm wide. Old evidence hints that thin walls peel with far less force per mm² (plate 1 slender-wall bound about 1 kPa against 67–101 kPa under flat discs, `docs/lessons/lessons-physics.md` §1b) **[M, low confidence]**.
2. **The sharing rule is wrong.** The rigid-lever rule gives small contacts far from stiff ones very large ratios, whatever their own size.

This plate tests the Light layout directly and looks at the first reading. It does not choose a sharing rule (§10).

---

## 1. Aim, and what the plate does not claim

**Aim (a):** does this exact Light layout survive on Round 25? Four A0 copies, two per band. The preregistered six-neck gate failed under R-C4, so both A1 slots become second A0 copies; the thinning contrast is deferred (§10).

**Aim (b):** do thin-strip coupons survive at an index where matched compact coupons fail? Compact and strip coupons with the same attachment, paired in each band. The plate-6 cap-on-arm pass/fail pair is included as a link to earlier plates.

**Not claimed:** a sharing rule, a first-exposure rule, an edge-reach rule, a slenderness rule, or a new design ratio. No design ratio is derived from this plate by arithmetic (§8.4). A survival is evidence about that configuration only.

---

## 2. Inference rules (fixed before printing)

1. **Mechanisms are not exclusive.** A result supports or contradicts a specific preregistered call. It does not select one mechanism over all others.
2. **Model comparison only on preregistered contrasts** (§5). Every other outcome is an observation, recorded and reported but not scored.
3. **Clear calls come only from a measured law.** The compact full-area law (P4 cylinder bracket 19.9–30.5 **[M]**) makes a clear call only at least ×3 past the bracket: fail at ≥ 92, hold at ≤ 6.6. Between those it makes no call.
   - The alternative "strips tolerate more" is an assumed scenario, not a calibrated law. It makes no clear calls.
   - The owner's judgement about Light is not a law. No margin is derived from it.
4. **Censored, not wrong.** A prediction whose mechanism was not exercised is censored. Examples: the piece failed for another reason first; the predicted site never formed. Censored calls are neither right nor wrong.
5. **Failed pieces stay at their own site (§7.3).** On this printer a piece that leaves the build plate stays on the vat film at its own spot until the job ends, and does not spoil other pieces **[O]**. Neighbours' outcomes therefore stay scorable.
6. **No global elimination.** One specimen never removes a predictor from the model. Results are stated per contrast.
7. **Intervals only when monotone.** A pass/fail interval is reported only when outcomes are monotone across rungs and replicates, and the failure modes match. Otherwise the raw outcomes are reported, with no interval.
8. **Survival is evidence for that configuration only.** If strips survive, the statement is "this strip configuration survived at index X on CAL-1", nothing wider.
9. **First exposure cannot be read from a pit or a clean ball.** A broken contact does not show when it broke. FIRST is deferred (§10).

---

## 3. The pieces

There are two bands, front and back. Each holds one complete set of 8 pieces. **Written plate total: 16 pieces [M].** This is a calibration plate, not a production print candidate; it has not been physically printed.

### 3.1 Round 25 copies (4: A0 ×4)

| ID | Supports | Purpose |
|---|---|---|
| **front-A0**, **back-A0** (original names unchanged) | ChituBox Light exactly as ChituBox made it (17 contacts) | Direct survival test; P4 additive clear fail (current replay 311.221 [U], historical rounded 313) |
| **front-A0-f2**, **back-A0-b2** | Exact A0 geometry and native support records, replacing A1 at the same XY translation and raster phase | Second A0 per band; four A0 copies in total, all C3 |

**R-C4 decision [S], with measured necks [M].** The rule was always the **exposed narrowest horizontal sliced section**. The ±1-layer station window was a measurement shortcut, not the criterion. In the owner's assembled slice, record **1** gives **761/387 = 1.966408 front** and **761/386 = 1.971503 back**, below ×2. Records **13/15 have no measurable A0 minimum**: their exposed section is still shrinking at layer 402 (20.10 mm) when it merges into the body at **layer 403**. A last isolated sample is not a measured minimum. Only **11, 17, 19** robustly qualify; records 3/5/7/9 remain below ×2. Lit-any and grey-weighted ratios agree on these full-intensity support blobs.

**Apply the preregistered fallback exactly:** fewer than six of ten means A1 becomes a second A0 in each band, not another thinning attempt. C4 moves to §10. The lip patches were unchanged by A1; replay already predicted the same lip first-failure site and no C4 survival difference.

**Continuity [M].** The replacement retains whole-pixel A0 placement at the former A1 translations. Both the 525-layer midpoint-grid witness and the new native GOO have zero raster islands. Exact native-raster comparison shows small one-row differences between the translated A0 copies (§6.4); continuity does not imply identical pixels or prove a physical print.

### 3.2 Matched compact and strip coupons (3 per band)

**Bodies.** Both are 5.0 mm tall (100 layers), on 4 contacts, lift 5.0 mm. The nominal layer area is 18.0 mm²; the written block is 18.0625 mm² (4.25²), and the strip is 18.0 mm². Dimensions and replacement assembled-slice results are below. Coupon records and native raster pixels are identical to the prior slice; their minima were independently re-measured under R-S324.

- **Block** (`B`): 4.25 × 4.25 mm. Contacts at the corners of a 2.5 mm square.
- **Strip** (`S`): 1.5 × 12.0 mm, long side along X (like the Round 25 layers). Contacts in a row, 3.0 mm apart, 1.5 mm in from each end.

The two bodies differ in more than shape. Contact arrangement (square against row), spacing (2.5 against 3.0 mm), perimeter (about 17 against 27 mm) and width across the peel direction all differ together. A difference in outcome says "strip coupons of this type differ from compact coupons of this type". It does not isolate width as the cause.

**Waisted head (identical attachment, a real narrow section outside the ball).** A plain tip cone cannot do this: its thin start hides inside the ball. With a 0.5 mm ball and a cone flaring to 1.0 mm over 1.5 mm, the exposed section is at least about 0.16 mm across, whatever the cone start field. At the API's minimum start section it is about 0.24 mm, so a nominal 324 rung would really be about 106 (cross-family finding, `crates/support/src/construct/contact.rs` 249–277). Each coupon contact is therefore built as:

1. **Contact ball** 0.5 mm across, centre 0.12 mm outside the face. Patch about 0.15 mm² **[S]**, the same at every rung.
2. **Waist:** a constant-radius rod from the ball centre down to a point 0.71 mm below it. Its exposed part, between the ball surface and the junction ball, is about 0.25 mm long (5 layers). Constant-radius tip rods are native: Light's branch tips use them **[M]**.
3. **Junction ball** 0.416 mm across at that point (1.04 × the cone top radius), the same at every rung.
4. **Cone** from 0.40 mm across at the junction to 1.0 mm over 1.5 mm, the same at every rung.
5. **Joint ball, 1.0 mm column, base and raft:** the same at every rung (step-11 §5.7).

Only the waist diameter changes. At every rung the waist is the narrowest section on the load path, and its section is the 16-gon value 3.0615 r² **[M]**. Patch, junction, cone and column are all larger.

| ID | Body | Written waist diameter [M] | Written waist section [M] | Nominal index | Replacement as-sliced narrowest index: lit-any / weighted [M], both bands | Compact-law call |
|---|---|---|---|---|---|---|
| **B-100** | block | 0.229 mm | 0.0401366 mm² | 100 | 98.036 / 97.005 | clear fail (≥92) |
| **S-100** | strip | 0.229 mm | 0.0401366 mm² | 100 | 96.944–98.065 / 96.667–97.785 | clear fail (≥92) |
| **S-324** | strip | 0.125 mm | 0.0119589 mm² | 324 | 320.099–332.652 / 319.184–331.701 | clear fail; within nominal ±10% and predicted range |

**R-S324 arithmetic [S], applied to owner-sliced pixels [M].** For each contact use the **narrowest sampled waist across all six labelled layers 4.40–4.65 mm**, where it would break, not the mean or the widest waist reading. Apply the same rule to B-100 and S-100. Body maximum / four / that contact's minimum waist gives the table above. The equal quarter-share is P4 arithmetic, not measured sharing or a plinth load-model replay. Body maxima are B **18.254250 lit-any / 18.062417 weighted mm²**, S **18.051026 / 17.999449 mm²**. All 100-rung indices clear 92. S-324's 320.1–332.7 lit-any range passes nominal 324 ±10% (291.6–356.4) and the prediction envelope 314.10–335.66.

Prediction envelopes remain [U]: B 95.84–99.79, S-100 95.67–101.32, S-324 314.10–335.66. They combine measured old native slice/mesh inflation with midpoint cell-touch calibration, not certified error bounds. The full layer-by-layer B range included values below its prediction, retained in raw evidence; its narrowest-section result is inside the prediction. Retuning changed waist sizes, not IDs. Old-size inputs remain in `input-measurements/`; replacement native measurements are in §6.4. Both bands pass their declared index checks in the new slice.

**Formation load [M, not force].** The replacement slice's maximum isolated ball area / narrowest waist ratios are B-100 **4.400–4.411**, S-100 **4.349–4.399**, S-324 **14.283–14.902**. Four supported heads remain separate through 5.00 mm; the body starts at 5.05 mm. W directly tests whether balls physically form; pixels alone do not prove it.

**W, formation witness (1 per band).** Six free-standing waisted heads with no body on them, built exactly as above: three with the 0.125 mm waist, three with the 0.229 mm waist. Their written sampled minimum sections are 0.0119588–0.0119589 and 0.0401366 mm² [M]. They show whether the ball forms on each waist under its own formation load only.

Each coupon is read first as **whole-coupon survival** (present / partly present / gone). The break site is a second, preregistered reading (§7.4).

### 3.3 Plate-6 cap-on-arm controls (2 per band)

Rebuilt from `reference/experiments/plate-06-results.md` (family B6 monos):

| ID | Plate-6 twin | Arm (48-gon inscribed in the diameter) | Arm section **[M]** | Index | Plate-6 outcome **[M]** |
|---|---|---|---|---|---|
| **K-15** | `M4` | 1.596 mm, 4.00 mm free length | 1.994870 mm² | 15.04 | Present |
| **K-60** | `M1` | 0.798 mm, 4.00 mm free length | 0.498718 mm² | 60.15 | Gone |

Common to both, from the record:

- The cap is a flat 48-gon disc of 29.9978 mm² (about 6.19 mm across), 0.50 mm thick.
- It reaches full size through a flare of 0.20 mm per side per 0.05 mm layer. The flare count therefore differs by arm: K-15 needs 12 layers (0.60 mm) from radius 0.798 to the 3.09 mm cap radius; K-60 needs 14 layers (0.70 mm) from radius 0.399.

**Heights, reconstructed.** The record gives a 4.00 mm free arm, a cap plane of 8.50–9.00 mm and the flare rate. It does not say where the flare sat: a 0.60–0.70 mm flare does not fit inside the 0.50 mm cap plane. This reconstruction keeps the 4.00 mm free arm unchanged, because plate 6 conditions its bracket on it:

| Piece | Arm | Flare | Cap |
|---|---|---|---|
| K-15 | 4.50 → 8.50 mm | 8.50 → 9.10 | 9.10 → 9.60 |
| K-60 | 4.50 → 8.50 mm | 8.50 → 9.20 | 9.20 → 9.70 |

The cap therefore sits 0.60–0.70 mm above the recorded cap plane **[S]**. The arm stands on its own base, top at 4.50 mm, resting on the plate **[S]**. If a surviving plate-6 manifest or transcript shows the flare position, that replaces this choice.

These are replication controls. They establish only whether these two controls reproduce plate 6 on this plate. They do not validate or void the other pieces (§8.1).

---

## 4. Plate layout

**Plate:** Saturn 4 Ultra 16K, build area 211.68 × 118.37 mm, height 220 mm; pixel 14 × 19 µm **[C]** (PRD P1). The tank tilts; the front (−Y in ChituBox) releases first and moves farthest **[O]**.

**Blocking.**

- Front and back each hold the full set: two A0, B-100, S-100, S-324, K-15, K-60, W.
- The A0 pair occupies the original A0/A1 slots; B-100 stays beside S-100. Replacement does not reshuffle or translate any other piece.
- Units and single pieces are shuffled within each band by a seeded random order, and the seed is recorded. Left and right inside each pair are random too.
- Front-to-back position was bounded at about one ×1.5 step on plate 4 **[M]**; the duplicated bands show its effect here.

**Written layout [M].** Seed **20261102**; xorshift64 and Fisher–Yates over six units per band, with random left/right within both pairs before the unit shuffle. Left to right in the top-view map:

- **Front:** S-100, B-100, W, A0-f2, A0, K-60, K-15, S-324.
- **Back:** A0-b2, A0, K-60, W, S-324, B-100, S-100, K-15.

Measured footprints including every native support: A 24.9991 × 14.6856 mm, B 4.7000 × 4.7000 mm, S 12.0000 × 2.2000 mm, K 8.0000 × 8.0000 mm, W 15.0000 × 2.4000 mm. Band spans are 151.8880 mm (front) and 151.8988 mm (back). Every piece reaches the plate and fits within the build area. The minimum conservative bounding-box gap is **6.017721 mm**; it is a lower bound on mesh separation, not an exact nearest-triangle distance. Placement reserves two pixel widths beyond 6 mm, then quantizes translations to whole owner-grid pixel steps to preserve each source's raster phase. The final written plate has **zero islands in all 525 midpoint layers under both centre-filled and cell-touching checks [M]**. That is not a ChituBox slice or proof of a physical print.

**Native supports [M].** The replacement is read back after writing. All **14 models outside the two former A1 slots are byte-identical as whole serialized model records**, including their names and placement fields. The two replacements clone the same band's A0, retain its geometry, support fields, meshes, attributes, raft and Z, and use the exact former A1 XY translations. `plate-check.json` records every model comparison and the whole-pixel phase check. Original A0 names are unchanged. The read-only individual directory includes the prior A1 source, but A1 is absent from the replacement plate.

**Combined layer profile [M].** `area-profile.json` records every written-mesh midpoint layer on the owner's grid, under both boundary conventions. The maximum total areas are 771.620771 mm² (centre) and 778.510171 mm² (touch), both at layer 4, midpoint 0.175 mm. These are mesh-grid witnesses, not ChituBox cured-pixel measurements or a peel/heat model. Coupon bodies end at 10 mm, cap controls at 9.6/9.7 mm and the A copies at 26.230127 mm [M]. The assembled GOO must replace the grid profile before printing.

**Labels and scale.** Engraved IDs and physical scale bars are **not implemented**: a safe editable native raft-tab cut has not been established by ChituBox round-trip. Use the labelled top-view `plate-map.png`; it marks the front edge in plain language. Its 10 mm drawing scale is not a printed part. The build plate's known dimensions and project positions give scale and identification before removal; known piece sizes give scale after washing. A ruler is optional for the tank-clean sheet.

**Real prints.** None are planned on this plate.

**Resin and time [M]:** the replacement header stores **5861.978515625 mm³ (5.862 mL) and 3685 s (1 h 1 min 25 s)**. These are slicer estimates, not physical consumption or duration measurements (§6.4).

---

## 5. Preregistered contrasts

Only these are scored.

**R-C4 orchestrator ruling (2026-11-02) [S].** Judge ×2 per neck at the **exposed narrowest horizontal sliced section**, not an axis-normal mesh section, radius field or station window. Only records **11, 17, 19** robustly qualify (3/10). The preregistered fallback therefore applies: both A1 slots become second A0 copies. **C4 is deferred to §10, not scored on CAL-1.** The evidence and distinction from owner rulings are in §6.4/§6.6.

| # | Contrast | Compact full-area law (P4) | Other reading | Scored as |
|---|---|---|---|---|
| **C1** | B-100 against S-100, same band, ×2 bands | Replacement native narrowest-section indices under R-S324 [M]: B 98.036 lit-any / 97.005 weighted; S 96.944–98.065 / 96.667–97.785. Both clear fail ≥92 | Assumed scenario, no clear call: the strip survives where the block fails | Per band: the (block, strip) outcome pair, with measured indices |
| **C2** | S-324, ×2 bands | Clear fail | None: a direct survival test | Outcome per band |
| **C3** | Four A0, two per band | Clear fail (additive replay 311.221 [U]; historical rounded 313) | None: a direct survival test | Outcome per copy and band |
| **C4** | **Deferred to §10 under R-C4; no A1 on CAL-1** | Not scored | Not scored | Preregistered six-neck gate failed; no post-hoc rule change |
| **K** | K-15 and K-60, ×2 bands | Control | Plate-6 outcomes: K-15 present, K-60 gone | Reproduced: yes / no, per piece |

Applied to every contrast:

- **Failed pieces (§2 rule 5, §7.3).** A failed piece stays on the film at its own site, so it does not change any other piece's outcome. Every piece is scored on its own.
- **Formation.** If W shows that the ball did not form on a waist size, no coupon failure at that waist counts as a load failure.

---

## 6. How the pieces are made, and what is proved before printing

### 6.1 Coupons and W: a calibration-only head builder

- A calibration entry point (a CLI example or binary, not a flag on the production command) reads a **specimen spec** (JSON): body, pose, lift, contact sites, the waisted-head dimensions (§3.2), the band and the shuffle seed.
- The step-11 construction API cannot build a waisted head. It allows one cone from the ball centre with end radius ≥ 0.25 mm. So this entry point writes the native records directly with the shape kernel in `geom`: ball, waist rod, junction ball, cone, joint ball, column, base, raft. Owner ruling R1 (§6.5) allows this.
- W is a solid body with no native supports, as for K. ChituBox v1.4.0 refused the native-support-only W with no body triangles. Its replacement is one closed, outward-oriented surface: six radial unions of contact ball, waist, junction ball, cone, joint ball, column and base, welded to a common 0.20 mm raft. The balls end free; there is no block above them. The waist remains at 4.40–4.60 mm above the plate. The solid construction opened and sliced in the feasibility check [M]. Both retuned W copies have six separate supported heads in the replacement native slice [M], with zero native and midpoint-grid islands [M]. Neither is a physical print result.
- The independent checker then reports H1, H2, H3, H4, H6 and H9 on the written file, with the K/W exceptions in §6.3. Its body checks do not measure the minimum section of an STL arm or waist: the calibration section measurement does that separately from the written triangles.
- R-C4 [S]: use the exposed narrowest horizontal sliced section, not the axis-normal mesh ratio. Only 3/10 robust necks qualify; both A1 slots become A0, and C4 is deferred (§5/§6.4).
- The planner, the P5 load check and the step-11 tip-family rule are not applied.
- Every file and report says "calibration piece, not a print candidate for any model".

**Reproducible entry point.** `crates/calibration/src/bin/cal1-plate.rs` accepts `<cal1.json> <read-only-individual-directory> <measured-summary.json> <empty-output-directory>`. It refuses to overwrite a nonempty directory. The measurement directory must also contain `method.json` and `a1b.json`; the individual directory must contain the original projects and `a1b.goo` for its exact grid. These owner inputs are not needed by ordinary unit tests.

Build and test through `scripts/t`. The initial assembly entry point is `cargo run -p plinth-calibration --bin cal1-plate -- <cal1.json> <read-only-individual-directory> <measured-summary.json> <empty-output-directory>`. For R-C4, preserve the frozen inputs first: move the project/GOO into `superseded-A1b/` and copy its original `plate-check.json` and `predictions.json` there. Run `cargo run -p plinth-calibration --bin cal1-replicates -- <plate-directory>`; it refuses an existing output project/GOO, preserves the other 14 records and writes the A0 replacement evidence. Render its map with `rsvg-convert --unlimited <output>/plate-map.svg -o <output>/plate-map.png`. Check a new owner slice with `cargo run -p plinth-calibration --bin cal1-slice -- <project> <cal1.goo> <new-measurement-directory>`; `narrowest-sections.json` uses R-S324/R-C4, not the station window. Compare all native pixels with `cargo run -p plinth-calibration --bin cal1-diff -- <replacement-project> <superseded-goo> <replacement-goo> <new-diff-directory>`; it retains exact changed intervals and compares each second A0 against the same-band original shifted by 2217 X pixels. Its one-row binary result is diagnostic, not a release criterion. Ordinary `scripts/t L1 calibration` needs no owner inputs.

### 6.2 Cap controls

The cap controls are solid bodies (STL) built to §3.3, with 48-sided sections. They have no native supports.

### 6.3 Hard limits, and the calibration-only exception

| Limit | Coupons | K controls | W witnesses | Round 25 copies (ChituBox-made) |
|---|---|---|---|---|
| H1 patch ≤ 0.50 mm² | Kept (about 0.15) | Not applicable (no contact) | Not applicable (no contact) | Reported, not enforced |
| H2 no fused contacts | Kept | Not applicable | Not applicable | Reported |
| H3 print-order continuity | Replacement native slice and grid: zero islands [M] | Replacement native slice and grid: zero islands [M] | Replacement native slice and grid: zero islands [M]; checker remains unverified | All four A0: replacement native slice and grid zero islands [M]; exact translated raster identity not achieved |
| H4 lift ≥ 5.0 mm | Kept | **Exception:** rests on the plate on its own base, as on plate 6 | **Exception:** solid body rests on its own raft on the plate, as for K | As made |
| H6 gap ≥ 0.3 mm | Kept | Not applicable | Not applicable (no native supports) | Reported |
| H9 ≥ 0.10 mm across | Kept: written minimum 16-gon caliper widths 0.122598 / 0.224600 mm [M] | **Measured** (thinnest arm 0.798 mm) | Written minimum sections measured for retuned 0.125/0.229 mm heads; checker H9 not applicable to body-only mesh | Reported (A0: 0.098 mm); R3, not a production exception |
| P5 and the "unprintable as is" class | **Deliberately exceeded** | Exceeded (K-60) | — | Exceeded as made |

**Calibration-only exception (K and W).** K and W are not support layouts for a model, so H1, H2 and H6 do not apply, and both break H4 by design. The W checker verdict is **FAIL** (H4); its report lists H3 and H8 as **Unverified**, H5 topology and H7 body preservation pass, and H1/H2/H6/H9 are not applicable. Both retuned W copies have zero islands in all 525 layers of the replacement native slice [M] and the midpoint-grid witness [M]. Neither result changes the checker's scope or proves a physical print. W is grey-AA solid geometry: its weighted waists differ from the full-intensity coupon support blobs despite matching written dimensions. This is not a print-candidate pass. The calibration checks and independent report must distinguish two things:

- the minimum load-bearing section along every load path (arm, flare and cap for K; raft through the contact-ball equator for W), measured from written triangles by the calibration section tool. W's free ball crown above its equator is not a loaded neck and tapers to a point. The six W paths meet ±10% of their design areas; the same minimum plateau includes 4.40–4.60 mm. This is sampled measurement, not a certified continuous lower bound;
- print-order continuity, reported independently; **Unverified** is not a pass. W's closed-surface topology test does not prove sliced-layer continuity.

The exception applies only to pieces labelled K or W in the calibration entry point.

### 6.4 Feasibility, proved before the owner is asked (mandatory)

**Evidence scope:** items 4–8 describe the replacement owner GOO `28d7e125…` and unchanged project `91611c85…`, measured read-only. Evidence is in `plinth-review/2026-11-02-cal1-plate/{slice-check.json,FROZEN.json,correction-slice-measurements/,correction-pixel-diff/}`. Small JSON snapshots for the repository are in `fixtures/calibration/cal1-frozen/`; they were committed in 1a7600c. The R-C4 fallback decision in item 2 uses the archived inputs and `rulings-slice-measurements/narrowest-sections.json`, not nonexistent A1 pieces on the replacement plate. No physical outcomes are measured.

1. **One coupon per waist size** (0.229 and 0.125 mm), built and written.
   - Current written 100-rung waists are **0.229 mm**, S-324 is **0.125 mm**; W uses those same sizes. Nominal rung IDs are unchanged. The replacement native S-324 narrowest-section index passes the declared ±10% criterion under R-S324; both 100 rungs clear 92.
   - Written sampled waist sections are 0.0401366 and 0.0119589 mm² [M], at the waist and within ±10% [S] of design. Written minimum opposing-face widths are 0.224600 and 0.122598 mm [M], both above H9's 0.10 mm. R1 permits calibration test pieces past production load limits; no new H9 exception is used. The four A0 copies stay under R3's comparison-copy rule.
   - Two prediction methods (§3.2) use the owner's measured waist inflation and the new written midpoint-grid areas. Per band: **B-100 95.84–99.79, S-100 95.67–101.32, S-324 314.10–335.66 [U]**. Thus both 100-rung predictions are above 92 with margin, and S-324 is near 324; this is not an assembled-slice measurement or a replay.
   - **Owner slice measured [M]; R-S324 [S] resolved:** use each contact's narrowest waist across every sampled waist layer, **4.40–4.65 mm inclusive**. Apply this to all coupons. S-324 gives **320.099–332.652 lit-any / 319.184–331.701 weighted**, inside ±10% and the prediction range; both 100 rungs clear 92. The independent first-layer registered symmetric one-row mask test passes **216/216**, over all six layers for 24 coupon and 12 W contacts. Evidence: `correction-slice-measurements/{decoded,narrowest-sections}.json` and `slice-check.json`.
   - If a check fails, redesign the head and re-measure. Never relabel the rung.
2. **A1 exposed-section gate: failed; fallback applied.** Plinth made the edit (§6.5); changing a field alone was never enough. The following preregistered checks explain the decision, not a request for further A1 work.
   - Measure the exposed narrowest section of the 10 edited tips on the written mesh and in the sliced layers, against A0.
   - A field-by-field comparison must show that only those 10 cone fields and their meshes changed.
   - ChituBox must open and save A1 with the fields intact. This depends on step-11 §2 item 1: whether ChituBox slices the stored mesh or rebuilds from the fields.
   - Judge ×2 per neck from the **horizontal ChituBox sliced area**, with lit-any and grey-weighted areas stated, not from axis-normal mesh diagnostics. Freeze the qualifying/excluded record list. Necks below ×2 are observations only and cannot decide C4. Keep A1 with at least **6 of 10** qualifying necks; with fewer than six, A1 becomes a second A0 (§3.1).
   - **R-C4 [S], 2026-11-02: exposed narrowest section governs; the ±1-layer window was only a shortcut.** Record 1 is **1.966408 front / 1.971503 back**, below ×2. Records 13/15 have **no measurable A0 minimum**: the section is still shrinking when it merges into the body at layer 403. They cannot qualify. Only **11, 17, 19** are robust: **3/10 < 6**. Apply the preregistered fallback: **A1 becomes a second A0 per band**. No further thinning and no post-hoc rule change. C4 is deferred to §10. Evidence: archived `slice-check.json`, re-measured `rulings-slice-measurements/narrowest-sections.json`. Replacement copies preserve exact former-A1 translations and whole-pixel A0 phase, with zero islands over all 525 midpoint layers under both boundaries. The replacement native slice has zero raster islands but is not an exact translated raster match (item 5); the differences are accepted under R-REP (§6.6). A1b save preservation no longer gates CAL-1 because A1b is not on this plate. The `slice_rules::measure` gate test now exercises six qualifying records, five-record failure, independent lit-any/weighted ratio sabotage, and an unmeasurable descending-before-merge minimum on either A0 or A1.
3. **W and K open and slice [O/M].** Their written minimum sections are as in §6.3. K is byte-preserved from the feasibility files; both copies of K and retuned W are present and continuous in the assembled owner slice (items 4–5).
4. **Replacement assembled plate sliced [O/M].** The owner generated the new `cal1.goo`; the expected project SHA-256 is unchanged. The replacement header stores **5861.978515625 mm³ (5.861979 mL) and 3685 s (1 h 1 min 25 s) [M]**, estimates rather than physical use or elapsed time. All 16 pieces, independently located by their translated XY footprints, first light at **index 0 (one-based layer 1), labelled Z 0.050000001 mm [M]**. **Front-W explicitly starts at layer 0**, as does back-W. The two W models have identical geometry and zero Z translations; only names and XY placement differ. The former transient display issue remains unexplained; no new native edit/save-preservation claim is made.
   - Header [M]: 15120 × 6230 pixels, pitch **0.013999999516 × 0.019000000441 mm**, layer height **0.050000000745 mm**, **525 layers**, normal exposure **2.35 s**, **3 bottom layers at 20 s**, **5 transition layers**, AA/grey/blur **4/3/2**, profile **Anycubic WW ABS-Like Clear**. All header settings match the superseded assembled slice; only volume, weight and price differ. All 525 layer-definition records, including Z and exposure, are byte-identical. Stored 0.05 mm lift/retract distances and speeds are not verified physical tilt kinematics.
   - **Zero islands [M]** across all 525 layers: 8-connected lit-any components, zero exact XY overlap with the preceding layer defines an island, first layer excluded. This proves the specified pixel-overlap check, not cured attachment or print success. Raw per-contact/per-layer measurements and all 16 first-layer checks are in `plinth-review/2026-11-02-cal1-plate/correction-slice-measurements/decoded.json`; `slice-check.json` is the compact measurement summary and points there, never to a temporary folder. `cal1-slice` reads the project/GOO without changing either input.
5. **Critical sliced layers measured [M].** Areas below are horizontal pixel areas, not axis-normal strength or cured-material measurements. **All layer numbers here and generic `layer` fields in `slice-check.json` are one-based; only explicitly named `layer_0based` fields are zero-based.** Z values are native labels, not certified mesh intersection planes. As-sliced indices use each contact's narrowest waist across **all six sampled layers 4.40–4.65 mm** under R-S324. The table reports those minima, not the obsolete full-window range. Front/back values can differ; both bands are included.

   | Piece, each band | Waist lit-any = weighted area (mm²) | Body maximum lit-any / weighted (mm²) | As-sliced index lit-any / weighted | Check |
   |---|---|---|---|---|
   | B-100 | 0.046550 | 18.254250 / 18.062417 | 98.036 / 97.005 | Both copies clear 92; inside prediction under R-S324 |
   | S-100 | 0.046018–0.046550 | 18.051026 / 17.999449 | 96.944–98.065 / 96.667–97.785 | Both copies clear 92; inside prediction under R-S324 |
   | S-324 | 0.013566–0.014098 | 18.051026 / 17.999449 | 320.099–332.652 / 319.184–331.701 | Passes nominal ±10% and prediction under R-S324 |

   - **R-S324 [S] recorded:** body maximum / four / **narrowest sampled waist per contact** is equal-quarter P4 arithmetic, not measured load sharing or a replay. S-324 lit-any indices are **320.1, 332.7, 326.3, 320.1 front; 320.1, 332.7, 332.7, 320.1 back**. These pass nominal ±10% and the prediction range. The same minimum-section principle applies to B/S-100 and C4; the latter fails its preregistered gate under R-C4. Full layer scans remain raw evidence, not alternate verdicts chosen after measurement.
   - **Registered one-row agreement: 216/216 pass [M], all six waist layers.** Explicit operational test: midpoint centre-filled written waist mask and native lit-any mask must each lie within the other's one-pixel Chebyshev dilation (±1 X pixel and ±1 Y pixel, approximately 14 × 19 µm). Register each piece once from its first-layer native lit-any bounding-box centre against its continuous written first-midpoint section centre; round the offset to whole pixels, retain the residual, then use that same shift for every layer. This fit uses no waist or tip pixels. Native-minus-predicted shifts: **−1 X** for front-B-100/front-S-324; **−1 X, −1 Y** for back-S-100/back-S-324; **−1 Y** for back-W; **zero** for front-S-100/back-B-100/front-W. Unregistered masks compare different positions; they do not diagnose a ChituBox section-plane defect. No native or mesh pixels remain beyond the opposite mask's allowance, and the tolerance is unchanged. Tests reject a missing stripe even after registration and a two-pixel residual displacement. This operational definition is explicit, not retroactively an owner-agreed error bound or a cure certificate.
   - Coupon waists and balls remain four separate supported blobs through labelled **5.00 mm**. At the ball equator label **4.90 mm**, B ball areas are **0.204820–0.205352 mm²**, S ball areas **0.201362–0.202426 mm²** (lit-any = weighted). Maximum isolated formation-area / minimum waist-area ratios are **4.400–4.411 (B-100), 4.349–4.399 (S-100), 14.283–14.902 (S-324)**, not measured forces. First body pixels appear at **layer 101, label 5.05 mm**, with one body blob; all first three body layers overlap below. No head is missing or prematurely merged with another head in those inspected layers.
   - Both W copies retain six separate heads through 5.00 mm and end at **label 5.15 mm**. Thin waists: lit-any **0.015428–0.015960 mm²**, weighted **0.0119564–0.0119606 mm²**. Thick waists: lit-any **0.047614–0.048944 mm²**, weighted **0.0401295–0.0401410 mm²**. Equator balls are **0.206150–0.208544 mm² lit-any**, approximately **0.191186–0.191191 mm² weighted**. Formation-area ratios are **12.917–13.517 lit-any / 15.985–15.991 weighted** for thin waists and **4.261–4.341 / 4.763–4.764** for thick waists. W has no body index. **W is grey-AA solid geometry; coupon support blobs are full intensity. Their weighted waist sections are therefore not identical despite matching written geometry.** W observations cannot silently certify identical coupon raster or strength.
   - K arms, flares and first full cap layers are present, one connected blob with overlap below. K-15 arm: **2.045806 mm² lit-any, 1.994746–1.994884 mm² weighted** (written 1.994870). K-60: **0.524552–0.525084 mm² lit-any, 0.498660–0.498988 mm² weighted** (written 0.498718). Flares first appear at **label 8.55 mm**; full caps first appear at **9.15 (K-15)** and **9.25 mm (K-60)**, about **30.194458 lit-any / 29.99778–29.99780 weighted mm²**. Last labels are 9.60 and 9.70 mm. No missing or unexpectedly thin K section is detected by this pixel inspection.
   - **C4 deferred under R-C4 [S].** There are no A1 pieces in this slice and no scored neck-thinning rows. The archived evidence that required the fallback is stated in item 2 and §6.6; it is not a replacement-slice A1 measurement.
   - Both original A0 lip trios have lit overlap through their formation/joining layers. Their probed blobs join the body by **label 3.10 mm** (centre lip) and **3.40 mm** (side lips). A binary merged mask cannot measure the interface patch area: that remains Unverified. The two highest-ratio unchanged branch rods, records **31/33**, have no missing axis-probe blobs and sampled isolated horizontal areas **0.021812 / 0.023142 mm²** (lit-any = weighted) in each original A0 copy; their pixels are unchanged from the archived measurement. These are the already thin native 0.10 mm rods, not newly edited necks; sampled horizontal areas do not certify their strength.
   - Replacement native lit-any area peaks at **layer 2, label 0.10 mm: 777.349879 mm²**, weighted **775.860974 mm²**. Integrating lit-any areas gives **5861.980527 mm³**, within **0.002011 mm³** of the header; weighted integration gives **5806.966093 mm³**, a different metric, not missing resin. No archived mesh-envelope count is claimed for this replacement.
   - **Exact raster comparison [M]:** all **525 layers** were compared as greyscale pixels, not only layer areas. Outside front-A0-f2/back-A0-b2 there are **zero differing pixels**, so all 14 unchanged pieces are exactly identical to the prior native slice. Prior-to-replacement differences are **163,044 front / 163,058 back**, confined to those slots, across **176 layers**. In the new slice, translating the same-band A0 by **+2217 X, 0 Y pixels** gives **25,036 front / 65,852 back** differing grey pixels, including **8,021 / 12,491** lit-any occupancy changes. Each pair differs on **524 layers (zero-based 0–523)**; last layer 524 matches. Native difference bounds are front **X [7643,9428), Y [4308,5061)**, back **X [11199,12985), Y [1152,1918)**. Every changed interval with its layer, row, X range and old/new grey value is preserved in `correction-pixel-diff/locations.json`.
   - **Disposition R-REP [S], 2026-11-02: accepted as slicer rasterization variation.** Exact translated-replicate equality is not achieved. Both directions of the symmetric one-row binary diagnostic have **zero pixels beyond the allowance**; grey differences are **0.03238% front / 0.08517% back** of source lit pixel-layer samples. The independent same-family reviewer recommended accepting this as slicer noise: unchanged translated pairs also differ, including front-A0/back-A0 by **90,998 grey / 5,154 lit-unlit pixels**, more grey differences than either replacement pair. Its unchanged controls give B-100 **817/817**, S-100 **1,075/1,075**, S-324 **690/690**, W **3,929/43**, K-15 **9,621/16**, K-60 **6,737/227** grey/lit-unlit differences [M, reviewer evidence]. The reviewer also records a one-layer branch split delay (zero-based 125 to 126) in both replicas, and up to **±4 pixels (~5%)** area variation on the ~85-pixel thin rods. No other component-count difference was found. Float-coordinate rounding is a proposed cause, not proven. The orchestrator's acceptance is R-REP (§6.6). These diagnostics are not proof of strength or of a physical print outcome.
6. **Hashes frozen [M], 2026-11-02; seed 20261102.** Replacement `.chitubox`: `91611c85a661f5caf77704898ff8b04ff5ef69c05d5d6b1b024b65a72ceb28b1` (expected hash verified unchanged); replacement `cal1.goo`: `28d7e12515674824e6bb3fa5725a1df2de819886b04b1b72e14df5e9808b4b2c`. Superseded GOO SHA-256: `fc7540e071e0a16f2e8dc179af5c640f2793b281067862dab09697912f313d94`, read-only. `FROZEN.json` records the date, seed and both replacement hashes. Both prediction formats carry both hashes; small JSON snapshots of the freeze, prediction table and measurement summary are in `fixtures/calibration/cal1-frozen/`. No binary project or printer file is copied into the repository. The decoder losslessly roundtrips the replacement GOO in memory. Fourteen unchanged model records are byte-identical; the replacements clone A0 at the former A1 translations. Written-model equality does not imply exact translated native-raster equality.
7. **Replay every piece** and write the prediction table (§7.1). **Computed on the frozen written project:** `crates/calibration`'s `cal1-replay` uses the unchanged support load API for all 16 pieces, including extra waisted-head junctions and the solid K/W paths. `predictions.json` and `predictions.md` contain all four sharing maxima, section, layer/Z, contact, warnings and preregistered calls. Front/back geometry is byte-identical after removing only names and XY placement [M], so their results match. Waists limit the coupons; omitting them makes the sabotage test fail its expected load check. Ratios are model estimates [U], not native sliced measurements or physical loads. A0 gives rigid-carrier **2292.948 [U]** and additive **311.221 [U]**; a rerun of the historical binary gives **2292.951 / 312.672 [U]**, reproducing the earlier rounded **2293 / 313**. The additive difference is explained by the current nominal patch area instead of the checker's shrunken lower patch area (same peak peel area; 0.466% larger denominator). Equal-share and tributary do not reproduce: historical **9669.53 / 42945.54 [U]**, current **3171.32 / 14084.87 [U]**. These unscored first-exposure artefacts depend on clipped patch history versus the API's tilted-disc approximation (about 3.05× difference [U]); they do not set the additive call. The API's patch exposure, support self-peel and sharing remain assumptions [U]. The summary index/contact/layer explicitly use additive. The replacement table has four A0 rows with **identical replay JSON**, reusing the A0 result after exact geometry preservation, and marks C4 deferred; no C4 neck rows are scored. Native measurements in items 4–6 do not validate replay load assumptions. **The replay WAS cross-family reviewed: gpt-sol built it; Claude Opus reviewed it; PASS.** `cal1-replay` was rebuilt from current source with `serde_json` **float_roundtrip enabled**, then rerun on a scratch copy of the frozen A1-era project (`7d507a3d…`); **all 11,664 numeric leaves match the superseded replay output bit-for-bit**. Each replacement row now uses that actual output (A0 for all four A0 copies), including every `replayed_*` field and call index. The corrected full rows match **10,244 replay-derived numeric leaves** bit-for-bit; the compact table's source SHA-256 is `e92631233b1917ead9b7c7ad5b81697f870dc124fa9a42524021f49c2340ccc7`. Correct values include B-100 **112.50639479508857 [U]**, K-15 **15.037470363267387 [U]** and K-60 layer-top **9.200000000000001 mm [M]**. A small `replay-maxima.json` snapshot anchors all 16 frozen rows to rebuilt replay output, (the frozen files are the record of the model at CAL-1 release, 1a7600c; a test pins their sha256, and the replay of the repo-constructed specimen types on later load models is checked for finite outputs only, not against the record); A0's owner mesh is not a repo fixture. A sabotage that changes every B-100 index/max/call consistently by one ULP fails the frozen-row test. JSON round-tripping alone is not the evidence. The frozen compact JSON table includes all 16 rows, additive section/index/layer/contact, all four sharing maxima, calls, contrasts and native coupon minima. Both input hashes are stamped. Replay alone is not a print release. The final review and the disposition of the native replicate differences are recorded in item 8 and §6.6.
8. **Native-slice review received; cross-family final review PASS and orchestrator disposition R-REP recorded 2026-11-02; released for the print request** (PRD §7 rule 2). The written-file replay review is complete: **gpt-sol implementation, Claude Opus review, PASS**. The independent **same-family** native-slice review found the plate sound, reproduced all raster checks and verified the C4 gate with two code sabotages. Its replay precision and K provenance defects are corrected: replay regenerated from a rebuilt executable, all frozen rows checked bit-for-bit against output, one-ULP sabotage rejected, and K-15/K-60 given their own unchanged replication-control provenance. The reviewer recommended accepting the translated-replicate differences based on unchanged-piece controls (item 5); the orchestrator accepted them under R-REP (§6.6). The corrected frozen table still needs committing. No new tolerance or owner consent is inferred. The print request comes from the orchestrator in chat, not from this document.

### 6.5 Owner rulings for this plate (2026-11-01/02) [O]

These apply only to the labelled calibration plate CAL-1, never to the production path:

- **R1.** Test pieces may exceed P5 and the "unprintable as is" class, and may use the waisted calibration tip.
- **R2.** Test pieces may be meant to break. That does not conflict with the PRD §7 "never doomed to fail" rule for this plate.
- **R3.** The four Round 25 copies are ChituBox comparison copies. Plinth's limits are reported for them, not enforced.
- **A1 edit responsibility.** Plinth, not the owner, makes support-record edits. This does not waive the exposed-section or save-preservation checks. CAL-1 now uses the preregistered A0 fallback under R-C4; no A1 edit remains to be made for this plate.
- **Recording (2026-11-02).** Photograph the whole build plate from above before removal, without a ruler, and mark each piece by eye on the plate map. Keep pieces identified through washing; photograph them after washing (§7.4).
- **Light.** "Light prints fine" is the owner's judgement, not a print. A0 is untested.
- **Resin.** Anycubic WW ABS-Like Clear, using the ChituBox profile already used for the owner's feasibility slices.
- **Plate size.** Sixteen calibration pieces are accepted. The archived assembled slice measured **5.858 mL and 1 h 1 min 25 s [O]**; header **5857.640625 mm³ / 3685 s [M]**. Acceptance is not physical consumption measurement or replacement-slice validation.

### 6.6 Orchestrator rulings (2026-11-02) [S] — not owner rulings

- **R-S324:** a coupon's as-sliced index uses each contact's **narrowest sampled waist**, across every waist layer including **4.65 mm**. S-324 **320.1–332.7 [M]** passes nominal ±10% and its prediction range. Apply identically to B-100/S-100; both clear 92.
- **R-C4:** §6.4 item 2 judges the **exposed narrowest section**; the ±1-layer window was only a measurement shortcut. Record 1 is **1.966/1.972 [M]**, below ×2. Records 13/15 have no measurable A0 minimum before body merge at layer 403. Only **11,17,19** are robust: **3/10 <6**. Apply the preregistered fallback exactly: A1 becomes a second A0 per band. No further thinning or post-hoc rule change; C4 moves to §10. Replay already predicted no C4 difference at the unchanged lip first-failure site.
- **R-REP [S], 2026-11-02:** the pixel differences between the translated A0 copies (front-A0-f2 and back-A0-b2, each against its band's A0 shifted +2217 X px: 25,036 / 8,021 front and 65,852 / 12,491 back grey / lit-unlit pixels; branch split one layer later, zero-based 125 to 126) are **accepted as slicer rasterization variation, not geometry change**. Evidence: unchanged translated pairs differ by similar or larger amounts (front-A0 vs back-A0: 90,998 / 5,154); no ±1 px shift explains them; one-row waist comparisons pass 216/216; zero islands; all 16 pieces start at layer 0. Consequence: the four A0 copies are replicates, with this variation recorded.
- **Evidence:** owner slice `fc7540e0…` of project `7d507a3d…`, preserved in `superseded-A1b/`, its `slice-check.json`, and `rulings-slice-measurements/narrowest-sections.json`. These are orchestrator interpretations of measured evidence, not new owner permissions.

---

## 7. Printing and recording

### 7.1 Before printing

Commit the prediction table. Per piece: the measured narrowest section, the replayed index, layer and contact, the compact-law call (fail / hold / no call), and the contrast it belongs to. Stamp it with both frozen hashes. The owner scores before seeing it.

**Status:** all 16 prediction rows and the replacement native measurements are frozen, dated 2026-11-02, seed 20261102, with both input hashes (§6.4 item 6). Full review outputs are in `plinth-review/2026-11-02-cal1-plate/predictions.{json,md}`; the small repository table, freeze and measurement summary are in `fixtures/calibration/cal1-frozen/`. Committed in 1a7600c. All four A0 rows retain identical reviewed replay numbers (additive **311.221 [U]**, rigid-carrier **2292.948 [U]**); **C4 is deferred**, not scored. C1–C3 receive the additive P4 clear-fail prediction [U], conditional on formation and failure site; native coupon indices meet their declared checks. K expects K-15 hold / K-60 fail as replication, not a P4 clear call; W is unscored. Full JSON retains all four sharing models, sections and warnings; unscored equal-share/tributary A0 maxima remain patch-model-sensitive first-exposure artefacts. `serde_json` float_roundtrip is enabled and replay was regenerated from the rebuilt executable: **11,664 numeric leaves** match the superseded output exactly; corrected full rows match **10,244 replay-derived numeric leaves**, and every compact row is tested against rebuilt-output maxima (§6.4 item 7). **Written-file replay review passed** (gpt-sol built, Claude Opus reviewed); **same-family native-slice review found the plate sound and recommended accepting slicer noise**. Its precision and K provenance defects are corrected. Exact translated raster copies differ (§6.4 item 5); the orchestrator accepted that variation under R-REP (§6.6), and the plate is released for the print request on 2026-11-02. The owner scores before seeing these predictions; no physical results exist.

### 7.2 Print record (PRD §7 rules)

Both file hashes; resin type and lot; resin and tank temperature (warmed; heater at 30 °C in his setup **[O]**); profile name; layer height; normal and bottom exposure; bottom and transition layer counts; anti-aliasing and image blur settings; release mode and speeds (tilt); rest times before exposure and after release; film; date; anything unusual.

### 7.3 Failed pieces (owner's printer facts, 2026-11-01) [O]

Coupons and K-60 are meant to fail. On this printer, as the owner describes it:

- A job runs to completion. It can be aborted but not paused and resumed. There is no mid-print inspection.
- A piece that does not stay on the build plate stays on the vat film at its own spot until the job ends.
- It does not fall onto other pieces or spoil them. The plate never goes lower than it rose, so it cannot press loose material into the film. The film is not damaged.
- After the job, a "tank clean" exposure cures one full-screen layer. The owner peels it out, and it collects the debris.

The owner accepts this risk as minimal. Consequences for scoring:

- A failed piece stays at its own site, so neighbouring outcomes stay scorable. No outcome is marked uncertain because another piece failed.
- The tank-clean sheet shows which pieces left remnants on the film, and where. That helps locate where a piece separated: at the waist, at the patch, or higher in the body. **The owner photographs the tank-clean sheet on a flat surface before discarding it; a ruler is optional.**

### 7.4 Recording, kept small

- **Recording and washing:**
  - Before removal, with the plate still on the printer or on paper towel and gloves on, photograph the whole build plate from above. Do not use a ruler: the plate's known dimensions and the known piece positions in the project give scale and identification. Excess resin will cover the plate. By eye, mark each piece present / partly present / gone on a printed plate map.
  - On removal, keep each piece on its own spot on a second printed map or in labelled cups by band, so washing does not mix them up.
  - After washing, photograph the pieces laid out on the map. Known piece sizes replace a ruler.
  - Run the tank-clean exposure and photograph the cured sheet on a flat surface before discarding it (§7.3); a ruler is optional. Note which remnants match which pieces, using the plate map.
  - After washing, check the basket for lost pieces and flakes.
- **One overview sheet** (the plate-map picture). For every piece, one mark: **present / partly present / gone**.
- **Close inspection only of preregistered contacts:**
  - Coupons: their 4 contacts. Each is intact, broken at the waist, or broken elsewhere / cannot tell.
  - W: each head's ball is formed or not formed.
  - All four A0 copies: the bottom lip, two branch rods under the lower underside flanks, underside heads and central rib. Each is intact, broken, or part damaged there, marked on the picture by anatomy.
- **Removal (Round 25 only, separate):** easy / normal / hard per zone, and any damage, with a photo. This is the PRD §6.4 record and is never mixed with print outcomes.
- The owner sends photos and the marked sheet, or dictates short lines. The orchestrator transcribes them into `docs/experiments/cal-1-results.md` and asks him to confirm the transcript.

---

## 8. Reading the result

### 8.1 Per contrast

| Outcome | Reading |
|---|---|
| K-15 present, K-60 gone (per band) | Those two controls reproduced plate 6. Nothing more |
| K does not reproduce | Reported beside every result. It does not void the four A0 or coupon survivals. Direct A0 evidence stands even if K-60 survives |
| C1: block gone, strip present (and the strip failure not in question) | State both assembled as-sliced indices. If both are ≥92 as predicted, the strip survival reverses its compact-law clear-fail call; if not, report only the calls actually made at measured indices. Cause not isolated (§3.2) |
| C1: both gone | No survival separation at this tested level, conditional on formation (W) and on the failure mode at the waists |
| C1: both present | Both configurations survived at their assembled measured indices. State which compact-law calls those measured indices actually support; do not use predicted values as measurements |
| C2: S-324 present | This strip configuration survived at its assembled measured index (nominal ID S-324) on CAL-1. Observed survival for this configuration only |
| C3: A0 present | This Light copy survived on this plate. The compact full-area clear call is reversed for it |
| C3: A0 failed | Record where, by anatomy, and compare with the replay's top list as an observation. The tank-clean sheet may show where it separated |
| C4 | Deferred to §10 under R-C4; no A1 outcomes or neck-effect scoring on CAL-1 |
| Bands disagree | Report both; no interval; replicate on a later plate |

### 8.2 Intervals

An interval for strips, stated using the two assembled measured indices, is given only under §2 rule 7: monotone across both bands, with waist breaks at the failed rung. State the as-sliced indices beside the nominal rung IDs. Each edge rests on 2 pieces from one plate and one resin.

### 8.3 Validity

- If W shows the ball did not form on a waist size, no failure at that waist counts as a load failure.
- If everything fails, including K-15, W and all four A0 copies: fit nothing. The cause is unresolved.

### 8.4 What changes after the plate

- No design ratio is computed from the plate. In particular, no "lower edge ÷ 2": with n = 1 or 2 per edge, that is not a defensible derivation (`plate-06-results.md`, Finding 2).
- The owner receives the evidence per contrast: outcomes, reversed or confirmed clear calls, and any monotone interval.
- A change to A2 (full-area load for strips) or to P5 is the owner's ruling (PRD §7). If he rules a change, plinth re-runs Round 25. The new layout is reported beside Light as a measurement, not a target.
- Owner ruling, 2026-11-09, after CAL-1: P5 stays 12 and A2 stays, as the conservative choice even if this resin could take thinner or sparser supports. Round 25 is not re-run for this.
- The PRD §8.4 side-by-side print still follows. CAL-1 does not replace it.

---

## 9. Order of work (dependencies, no dates)

1. Owner answers to §11.
2. Apply R-C4's preregistered second-A0 fallback; C4 is deferred, no further A1 edit.
3. Build the calibration entry point and the waisted-head builder; write the coupons, W and K.
4. Feasibility (§6.4 items 1–3). Redesign and re-measure any piece that fails, recording the change.
5. Assemble the plate with the seeded shuffle; §6.4 items 4–8; commit the prediction table.
6. Ask the owner to print. He prints, then runs the tank-clean exposure.
7. Photos, film, vat and basket checks, overview sheet, preregistered contacts, removal notes (§7.4).
8. Transcribe; score C1–C3 and K under §2 and §7.3; C4 is deferred. A different-family reviewer checks scoring against raw marks before the owner sees it.
9. Owner rulings, if any. Update PRD §7 and step-11 §4 by replacing the superseded values.

---

## 10. Deferred to later plates

Each needs its own controlled pair, with a common formed carrier and a common activation stage, in which the predictors being compared make opposite clear calls:

- **C4, exposed-neck thinning:** CAL-1's A1 gate failed under R-C4 (3/10 robust necks). No further thinning on CAL-1. A later controlled pair must meet its exposed-section rule; unchanged lip patches still limit this Light layout.
- **Sharing rule** (additive, rigid-lever, tributary): lever and outrigger pieces.
- **First-exposure patch:** a matched early/late activation pair.
- **Edge reach** past the last contact, and **column slenderness** (P12).
- **Patch-limited contacts on sloped faces** (the Light lip is patch-limited; A1 does not reach it).
- **Fewer contacts on Round 25.**
- **Two-contact strip challenges** at the H9 minimum, strip orientation, other tilt angles, wider strips, and replicates at any interval edge found here.

---

## 11. Questions for the owner

The answers of 2026-11-01/02 are recorded in §6.5 and §7.3. The last two were answered on 2026-11-09: the resin label shows no lot number, and the printer holds the tank at 30 °C (30–33 °C while printing). No questions remain open.

---

## 12. Plate CAL-2: where load paths break against the additive ratio

Status: generated 2026-11-10 (second matrix, after the owner and orchestrator rulings of the same day) and not yet printed. Command: `cal2-plate fixtures/calibration/cal2.json <empty-directory>` (crate `plinth-calibration`; spec and rules in `cal2_spec.rs`, shapes in `cal2_shape.rs`, separate from the CAL-1 spec, which is unchanged). Output: one `.chitubox`, `cal2-report.json`, `plate-map.svg`.

**Question.** The additive ratio is R = mm² of model area that peels in one layer ÷ mm² of total limiting section. Where do real coupons break between R 25 and R 70, and does the answer depend on body size, body shape, how many layers sit near the peak load, or how old a neck is when it is first loaded? The design limit is 12 and the unprintable limit is 40 (`limits.toml`, `warn.rs`).

**Ratio definition: plinth's load model, exactly.**

1. The **peel area** of a layer is the model's slab envelope in that layer (`LayerArea::model_envelope`, "the peel area convention", `crates/support/src/load/data.rs`), sliced by `plinth_geom::slice` from the written body. It is not the change in area from the layer below. Every layer of a flat slab is therefore a loaded layer, with the same ratio.
2. The **section** is each contact's minimum section along its path, as the load model takes it (`analyse.rs`: the capacity is the minimum of the printed patch and every section of the path). For plinth's heads that is the **face patch**, 6 to 8% below the cone-exit section. Bodies are sized to the sum of the measured per-head minimum sections, not to the cone exit. The report gives both ratios per coupon: `achieved_ratio_min_section` (the label R) and `achieved_ratio_cone_exit` (about 0.92 to 0.95 of it for plinth heads; equal for the 1.20 mm columns, whose waist is the minimum).
3. The label R is the **peak layer** of the coupon. Bodies are sized at 99.95% of the target and every dimension is rounded down to 0.01 mm, so no layer is above the target, and the generator refuses any coupon with a layer over it or a peak more than 3% under it. Achieved peaks are 24.9 to 25.0 for R 25, and so on up to 69.9 for R 70 (witnesses: 5.9 to 6.0).

**Two owner directions (2026-11-10) that shape the plate.**

1. Plinth should use more, smaller supports, not fewer large ones. So the main coupons raise R by adding heads at a fixed neck size, and the body grows with the head count.
2. Plinth will count on the model's own thicker parts to carry peel load. So some coupons carry large sections (0.80 mm heads and 1.20 mm columns) to test whether the ratio rule still holds when the section is large.

**Owner rulings for this matrix (2026-11-10).** Dense coverage of R 25 to 50 across size and shape, and two hypotheses: **fatigue** (many consecutive layers near the peak load weaken a support) and **cure age** (a neck loaded right after it forms is weaker than one loaded after intervening layers).

**Standard coupon.** A flat-bottomed, flat-topped square slab, sides parallel to the vat tilt axis (the front edge, −Y, peels first). The target ratio is held for 40 layers (2.0 mm), loaded immediately above the heads. Every other variant is a change to this one.

**Matrix** (priority order; R as labelled, all 0.55 mm rows are 9 heads unless stated):

| Row | Section | Heads | Variant | R | Copies | Coupons |
|---|---|---|---|---|---|---|
| R1 | 0.40 mm neck | 16 (4×4) | standard | 25, 30, 35, 40, 45, 50, 70 | 2 | 14 |
| R2 | 0.55 mm neck | 9 (3×3) | standard | same seven | 2 | 14 |
| R3a | 0.55 mm neck | 9 | **short hold** (fatigue): 5 layers at full area, then a stem of at most 40% of that area to 40 layers | 40, 50 | 2 | 4 |
| R3b | 0.55 mm neck | 9 | **long hold** (fatigue): 100 layers (5 mm) at full area | 40, 50 | 2 | 4 |
| R4 | 0.55 mm neck | 9 | **late load** (cure age): starts at 40% of the area right above the heads, ramps linearly to full area over 40 layers, then holds 40 layers | 40, 50 | 2 | 4 |
| R5a | 0.55 mm neck | 8, one row | **strip**: body 5:1, long side along X | 40, 50 | 2 | 4 |
| R5b | 0.55 mm neck | 12 | **ring**: annulus 3 mm wide, heads on its centre line (like the Round 25 disc rim) | 40, 50 | 2 | 4 |
| R6 | 0.80 mm neck | 4 (2×2) | standard | 25, 35, 40, 50, 70 | 2 | 10 |
| R7 | 1.20 mm plain waisted column | 4 (2×2) | standard | 40, 70 | 2 | 4 |
| R8 | 0.55 mm neck, control | 4 (2×2) | standard | 40 | 2 | 2 |
| R9 | formation witness | 1 | standard | 6 | 1 per neck size | 3 |

67 coupons fit with the 6 mm gap; nothing is cut. If a future change makes the plate too full, the generator cuts copies from the lowest-priority row first (R9 first), never below two copies for rows of priority 1 to 5 (R1 to R5b), and records the cut in the report and on the map.

**Body sizes** (min-section R, mm; full detail in `cal2-report.json`). R1: 6.80² (R25), 7.44² (R30), 7.03 × 9.19 (R35), 7.40 × 9.99 (R40), 9.12² (R45), 9.61² (R50), 11.37² (R70). R2: 6.97², 7.64², 8.25², 8.82², 9.36², 9.86², 11.67² for R25 to R70. R3a, R3b, R4 at R40 and R50: 8.82² and 9.86² (stems 5.57² and 6.23²). R5a: 18.6 × 3.72 (R40), 20.8 × 4.16 (R50). R5b: outer diameter 14.02 (R40) and 16.78 (R50), 3 mm wide. R6: 6.70² to 11.22². R7: 13.27² (R40), 17.56² (R70). R8: 5.88².

**Rectangles at 0.40 mm, R35 and R40.** Plinth's raft flares (1.1 mm) cannot be joined at a head pitch between about 1.96 and 2.19 mm: the 16-head grid fails with `NarrowJoin` at every ratio from 34 to 41 (found by building it at each ratio from 28 to 47). A square 4×4 at R35 (pitch 2.0) and R40 (pitch 2.14) falls inside that window. These two bodies are rectangles of the same area with the head pitch 1.76 × 2.30 mm (R35) and 1.85 × 2.50 mm (R40), outside the window on both axes. Load per head is still uniform; the body is not square. Read these two cells with that in mind.

**Coupon construction.** Every step in height is a whole number of 0.05 mm layers, moved 0.5 µm off the layer boundary so that plinth's slab envelope, which counts a face lying on a boundary in both layers it touches, sees the footprint the design says (growing steps up, shrinking steps and tops down). The body bottom is 5.0 mm above the plate (H4 lift, as in CAL-1), so the loaded layer is layer 101 and the layers below it hold supports only. Body area at full load = R × heads × the measured minimum section of one head (trial build of one head, sliced from its written records), less 0.05%, dimensions rounded down to 0.01 mm. Heads sit at the centres of equal cells (grid), along the centre line (strip, ring), vertical, with columns to a raft. The short-hold stem is concentric, with its sides 63.2% of the body's (floored, so at most 40% of the area). The late-load body is, until the ramp ends, one stepped pad per head (each pad grows from 40% to 100% of its cell, one step per layer, as one closed solid with no internal faces), then one slab over all of them. The pads are separate islands for the first 40 layers, each held by one head, so a late-load neck carries only its own pad. This differs from a connected slab; read it as a test of neck age under ramped load, not of sharing. A raised corner tab (front-left of the top face) and the raised coupon number sit above every loaded layer, so they add no area to a layer that peels in the body (rings and bodies under 5 mm, except strips, carry none). Raft IDs are not made: a native raft cut is not proved safe in ChituBox, so the map is the key.

**Tallest part.** The long hold: 5.0 mm lift + 5.0 mm hold + 0.4 mm raised features = 10.4 mm, 208 layers.

**Heads are plinth's own.** `head_candidate` (public wrapper of the planner's `candidate()` in `crates/support/src/place/search.rs`) gives the ball, standoff, cone and joint. `PlanBuilder` admits them, `freeze` routes columns and the raft, and `plinth_emit` writes them. The only free input is the cone-start radius, solved so the section where the cone leaves the ball has the designed diameter (0.40, 0.55, 0.80 mm; the "neck" in a row's name). Plinth's head path makes 0.40 and 0.80 mm: the 0.40 head is below the planner's current smallest (0.55), and 0.80 mm is at the top of the ball-radius limit (0.42 mm). It cannot make 1.20 mm (the ball would be 0.64 mm, over the 0.42 mm limit and the H1 patch maximum). So **row R7 is a plain waisted column**: base flare, pillar, taper, 1.20 mm waist cylinder, taper, ending 0.1 mm inside the body. It is calibration-only; its top is wider than H1 allows for a real contact. Every column, flare and raft section is larger than the neck.

**Placement.** Three front-to-back zones. The copies of one cell are in different zones, so the front-to-back position of a copy is not confounded with the cell. Coupons are packed on shelves; the order within a shelf and the zone assignment come from the recorded seed (20261110, xorshift64, as in CAL-1). Moves are whole owner-grid pixel steps. Clearance (at least 6 mm, bounding boxes, conservative) is verified on the re-read file.

**What the generator checks, from the re-read file.** For every coupon: the slab envelope of every body layer (plinth's slicer on the written body mesh); the section where every cone leaves its ball (sliced from the written cone mesh), the ball face section, the column, the flare and the smallest horizontal section above the raft, per head, and from them each contact's minimum section; the peak layer ratio against the sum of minimum sections (rejected if more than 3% under the target, or any amount over it); the same ratio against the cone-exit sections; the ratio of every layer. Profile rules, each an error if broken: standard, strip and ring have exactly 40 layers within 2% of the target (the first, right above the heads, among them); short hold has 5, every later layer at or under 40% of the target; long hold has 100; late load has its first layer at or under 40% of the target, a strictly rising ramp over 40 layers, and 41 layers at the target (the last ramp layer and the 40 of the hold). The project must parse and write back byte for byte, and clearance (at least 6 mm, bounding boxes) is verified on the re-read file. `plinth-check` does not apply: it measures files with exactly one model and returns *unverified* for every rule on this 67-model plate.

**Known property of plinth's heads (finding).** The ball's section in the plane of the body face is 6 to 8% smaller than the section where the cone leaves the ball (ratio 0.92 to 0.94). Plinth sizes the planar patch at the cone-start radius, and the cone widens before it leaves the ball. So the strictly smallest section of a head is the face patch, not the neck, and the load model takes it. The labels are against the patch; the cone-exit ratio is 5 to 8% lower, and is in the report for comparison with the first CAL-2 matrix and with anything defined at the neck.

**Decision reads** (per coupon, copies compared; "survives" means whole coupon intact, as in CAL-1 §7.4):

- Any head break at R ≤ 40: lower the unprintable limit (40) in `warn.rs`. All copies intact at 50 and none failing at 40: 50 is a candidate for the limit, pending a second plate. Between: the break ratio is the first rung that fails in both copies.
- **Fatigue.** The long hold (R3b) fails where the standard (R2) and short hold (R3a) coupons survive at the same R, 40 or 50: the limit depends on how many layers sit near the peak load, and plinth should count layers at the peak, not only the peak. All three alike: no fatigue effect at these layer counts.
- **Cure age.** The late-load coupon (R4) survives where the standard coupon (R2) fails at the same R: a neck loaded right after it forms is weaker, and plinth could ramp the load instead of stepping it (thin first layers, wider later ones). Both alike: no cure-age effect at 2 mm of age. The late-load pads are separate until the slab; a difference from R2 could also come from that, so read a late-load survival as stronger evidence than a late-load failure.
- **Shape.** The strip (R5a) or ring (R5b) differs from the square at the same R (R2 at 40 or 50): shape matters beyond area and section, and the ratio alone does not set the limit.
- The 4-head 0.55 controls (R8) differ from the 9-head 0.55 standard coupon (R2) at R 40: load is not shared evenly across heads, so the additive rule over-counts the heads.
- The neck sizes (0.40 mm R1, 0.55 mm R2, 0.80 mm R6, 1.20 mm R7) break at different R: the ratio-only model does not scale with section size. That matters for counting on thick model parts to carry peel load.
- The two 0.40 mm rectangles (R35, R40 in R1) differ from their square neighbours' trend (R30, R45): suspect the body shape or head pitch, not the ratio.
- Witnesses (R9) that do not form say that head size cannot be made; read the other cells of that size as untested, not as passing.

**Not claimed.** This plate is not a print candidate for any model. It has not been sliced or printed. No ChituBox open, save and slice check has run; the repo's round trip (parse, then write, byte-identical) has.
