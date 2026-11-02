# Calibration print plan: Light on Round 25, and thin strips against compact bodies (plate CAL-1)

Status: full 16-piece plate assembled and written-mesh checks measured, 2026-11-02. A1 is corrected A1b; the owner's individual slice confirms zero islands and six qualifying necks. The retuned assembled project still needs ChituBox open/save/slice validation, actual sliced measurements, full load-model replay and independent review. **Not ready for a physical print.** Owner rulings are in §6.5; only resin lot and tank temperature remain open in §11. Nothing here is printed until the owner says yes. This informs the FR-20a load model (`docs/design/step11-construction.md` §4) and PRD §7 (P4, P5); it does not calibrate the whole model.

**Labels on numbers** (as in the step-11 design):

- **[O]** owner ruling or owner practice;
- **[S]** orchestrator starting value: a choice, not evidence;
- **[M]** measured (on this printer, or on a written file);
- **[C]** ChituBox convention or vendor data;
- **[U]** unmeasured estimate; never decides alone.

Written counts, footprints and sections labelled **[M]** are measurements of the emitted files. Predicted indices and the resin estimate remain **[U]**; they are not assembled-slice or physical-print measurements. The remaining feasibility work (§6.4) and replay replace those predictions before printing. Where measurements require a redesign, change the geometry, never silently change a rung ID or a rule.

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

**Aim (a):** does this exact Light layout survive on Round 25? Two A0 copies, one in each band. And does a neck-thinned copy (A1) survive? Two A1 copies, each beside an A0 in its band, made only if the thinning is real in the written mesh and the sliced layers (§6.4).

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

### 3.1 Round 25 copies (4: A0 ×2, A1 ×2)

| ID | Supports | Purpose |
|---|---|---|
| **A0** (front and back) | ChituBox Light exactly as ChituBox made it (17 contacts) | Aim (a): direct survival test. The compact full-area law makes a clear fail call (additive 313 ≥ 92). Nothing makes an opposing clear call |
| **A1** (front and back; corrected A1b) | A0 with only the 10 main cone start radii and their canonical meshes changed. Start diameter is normally 0.104 mm; A1b uses 0.140 mm at cone record 3 and 0.200 mm at record 5 to support the ball's first pixels. Balls, every endpoint, cone ends and lengths, columns, branches and raft unchanged | Aim (a): does a thinner exposed neck at the underside heads change survival? |

**What A1 changes [M].** A1 is the corrected A1b. The eight 0.052 mm start-radius heads have a written axis-normal minimum of about 0.038954 mm² against A0's 0.151371 mm² (×3.886); records 3 and 5 have wider starts for continuity. Axis-normal area is not the horizontal section the printer forms. The owner's A1b slice has **zero islands** and **six of ten** necks qualify by horizontal sliced area. The frozen qualifying cone records are **1, 11, 13, 15, 17, 19** (ratios ×2.396, ×3.089, ×2.124, ×2.131, ×3.349, ×3.341). Records **3, 5, 7, 9** are observations only (×1.905, ×1.380, ×1.478, ×1.494); they cannot decide C4. Lit-any and grey-weighted areas and ratios are identical for these full-intensity support blobs. The comparison takes the minimum isolated blob in the nearest labelled layer and its immediate neighbours at each file's written neck station, as specified by the slice-measurement protocol.

**A1b continuity [M].** The builder requires the written cone/ball pair to overlap below at every midpoint on the owner's 14×19 µm, 0.05 mm grid under both centre-filled and cell-touching boundaries. This selects start radii 0.070 mm at record 3 and 0.100 mm at record 5; the other eight remain 0.052 mm. All 525 layers of the complete A1b written mesh have zero islands under both checks; the owner's independent A1b slice also has zero islands. No tip moves and no ball changes. Placement preserves the source raster phase by moving only whole pixel steps; the written assembled plate also has zero midpoint-grid islands (§4). Its ChituBox slice is still required [U].

The lip contacts stay limited by their unchanged patches (0.03–0.04 mm²), so **A1 does not change the lip**. Under the orchestrator ruling in §5/§6.4, keep A1 if at least six of the ten necks qualify by horizontal sliced area. Otherwise both A1 slots become A0 copies and aim (a) reduces to "does A0 survive, n = 4".

### 3.2 Matched compact and strip coupons (3 per band)

**Bodies.** Both are 5.0 mm tall (100 layers), on 4 contacts, lift 5.0 mm. The nominal layer area is 18.0 mm²; the written block is 18.0625 mm² (4.25²), and the strip is 18.0 mm². The current written waist dimensions and predicted indices are recorded below; the measured body-slice areas are inputs, not assembled-slice results:

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

| ID | Body | Written waist diameter [M] | Written waist section [M] | Nominal index | Predicted as-sliced index [U], each band | Compact-law call, conditional on slice confirmation |
|---|---|---|---|---|---|---|
| **B-100** | block | 0.229 mm | 0.0401366 mm² | 100 | 95.84–99.79 | predicted clear fail (≥92); assembled slice Unverified |
| **S-100** | strip | 0.229 mm | 0.0401366 mm² | 100 | 95.67–101.32 | predicted clear fail (≥92); assembled slice Unverified |
| **S-324** | strip | 0.125 mm | 0.0119589 mm² | 324 | 314.10–335.66 | predicted clear fail; assembled slice Unverified |

**Prediction arithmetic [U], grounded in owner-sliced measurements [M].** Retune the heads, not the IDs. The measured native waist inflation is 11–20% by area; the 100-rung targets must remain above 92 with margin after that inflation. The predictions use two methods: (1) apply the measured native slice/mesh inflation range at the matching waist size to the new written mesh section; (2) calibrate the original midpoint cell-touching grid area to that contact's native sliced waist area, then apply that factor to the new written grid area. The table gives the envelope of both methods over all five labelled layers 4.40–4.60 and all four contacts. The mesh-inflation-only estimates are B 96.76–99.79, S-100 98.17–101.32 and S-324 314.10–324.40. These are predictions, not measured assembled-slice indices or certified error bounds. Minimum predicted margins above 92 are 3.84 (B) and 3.67 (S).

The measured unchanged body maxima used in both methods are 18.254250 mm² (B) and 18.051026 mm² (S). Each is divided by four and the predicted waist area. The equal quarter-share is P4 arithmetic, not measured sharing or a plinth load-model replay. The 324 rung also needed retuning: its measured slice was 14% below its nominal value, outside a declared ±10% closeness criterion [S]. The retuned prediction is near 324 by that criterion. The old-size measurements remain raw inputs in `input-measurements/`, not current rung results. Replace these predictions with the assembled slice's lit-any and grey-weighted measurements before printing.

**Formation load.** The ball forms on the waist before the body starts. Formation-load ratios for the retuned heads remain Unverified until measured in the assembled slice. W directly tests whether the balls form.

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

- Front band and back band each hold the full set: A0, A1, B-100, S-100, S-324, K-15, K-60, W.
- Matched pairs stay together as a unit: A0 with A1, and B-100 with S-100.
- Units and single pieces are shuffled within each band by a seeded random order, and the seed is recorded. Left and right inside each pair are random too.
- Front-to-back position was bounded at about one ×1.5 step on plate 4 **[M]**; the duplicated bands show its effect here.

**Written layout [M].** Seed **20261102**; xorshift64 and Fisher–Yates over six units per band, with random left/right within both pairs before the unit shuffle. Left to right in the top-view map:

- **Front:** S-100, B-100, W, A1, A0, K-60, K-15, S-324.
- **Back:** A1, A0, K-60, W, S-324, B-100, S-100, K-15.

Measured footprints including every native support: A 24.9991 × 14.6856 mm, B 4.7000 × 4.7000 mm, S 12.0000 × 2.2000 mm, K 8.0000 × 8.0000 mm, W 15.0000 × 2.4000 mm. Band spans are 151.8880 mm (front) and 151.8988 mm (back). Every piece reaches the plate and fits within the build area. The minimum conservative bounding-box gap is **6.017721 mm**; it is a lower bound on mesh separation, not an exact nearest-triangle distance. Placement reserves two pixel widths beyond 6 mm, then quantizes translations to whole owner-grid pixel steps to preserve each source's raster phase. The final written plate has **zero islands in all 525 midpoint layers under both centre-filled and cell-touching checks [M]**. That is not a ChituBox slice or proof of a physical print.

**Native supports [M].** Each of the 16 written models is compared byte-for-byte with its written individual project after undoing only the model name and XY translation. Body triangles, support fields and meshes, attributes, presets, raft records and Z translation are retained. A1's source is A1b. The eight individual projects are included with the plate; only B/S/W waist sizes differ from the feasibility set.

**Combined layer profile [M].** `area-profile.json` records every written-mesh midpoint layer on the owner's grid, under both boundary conventions. The maximum total areas are 771.620771 mm² (centre) and 778.510171 mm² (touch), both at layer 4, midpoint 0.175 mm. These are mesh-grid witnesses, not ChituBox cured-pixel measurements or a peel/heat model. Coupon bodies end at 10 mm, cap controls at 9.6/9.7 mm and the A copies at 26.230127 mm [M]. The assembled GOO must replace the grid profile before printing.

**Labels and scale.** Engraved IDs and physical scale bars are **not implemented**: a safe editable native raft-tab cut has not been established by ChituBox round-trip. Use the labelled top-view `plate-map.png`; it marks the front edge in plain language. Its 10 mm drawing scale is not a printed part. Use a ruler in print photos.

**Real prints.** None are planned on this plate.

**Resin, planning estimate [U]:** Round 25 copies 4 × (0.85 mL model **[M]** + about 0.2 mL supports) = 4.2 mL; coupons, controls, witnesses and their supports about 1.1 mL; **total about 5–6 mL**. ChituBox's sliced volume replaces this (§6.4).

---

## 5. Preregistered contrasts

Only these are scored.

**C4 orchestrator ruling (2026-11-01) [S].** Judge ×2 **per neck on the horizontal sliced-layer area**, which is what the printer forms, not the axis-normal mesh section or the cone radius field. Exclude a neck below ×2 from C4 and report it only as an observation. Keep A1 as A1 if at least **6 of 10** main necks qualify; with fewer than six, replace both A1 slots by A0. The corrected A1b owner's slice qualifies exactly six; the frozen qualifying records are **1, 11, 13, 15, 17, 19**. Records **3, 5, 7, 9** are excluded from C4 and retained only as observations. A whole-piece outcome or a failure at an excluded neck does not become a scored qualifying-neck effect.

| # | Contrast | Compact full-area law (P4) | Other reading | Scored as |
|---|---|---|---|---|
| **C1** | B-100 against S-100, same band, ×2 bands | Predicted clear fail: retuned envelopes 95.67–101.32; confirm from the assembled slice before making a clear-call claim | Assumed scenario, no clear call: the strip survives where the block fails | Per band: the (block, strip) outcome pair, with measured indices |
| **C2** | S-324, ×2 bands | Clear fail | None: a direct survival test | Outcome per band |
| **C3** | A0, ×2 bands | Clear fail (additive 313) | None: a direct survival test | Outcome per band |
| **C4** | A0 against A1, same band, ×2 bands; at least 6 of 10 main necks must qualify (§6.4) | Clear fail for both | None | Per band: outcome pair and failure site, scored only at qualifying necks; excluded necks are observations |
| **K** | K-15 and K-60, ×2 bands | Control | Plate-6 outcomes: K-15 present, K-60 gone | Reproduced: yes / no, per piece |

Applied to every contrast:

- **Failed pieces (§2 rule 5, §7.3).** A failed piece stays on the film at its own site, so it does not change any other piece's outcome. Every piece is scored on its own.
- **Formation.** If W shows that the ball did not form on a waist size, no coupon failure at that waist counts as a load failure.

---

## 6. How the pieces are made, and what is proved before printing

### 6.1 Coupons and W: a calibration-only head builder

- A calibration entry point (a CLI example or binary, not a flag on the production command) reads a **specimen spec** (JSON): body, pose, lift, contact sites, the waisted-head dimensions (§3.2), the band and the shuffle seed.
- The step-11 construction API cannot build a waisted head. It allows one cone from the ball centre with end radius ≥ 0.25 mm. So this entry point writes the native records directly with the shape kernel in `geom`: ball, waist rod, junction ball, cone, joint ball, column, base, raft. Owner ruling R1 (§6.5) allows this.
- W is a solid body with no native supports, as for K. ChituBox v1.4.0 refused the native-support-only W with no body triangles. Its replacement is one closed, outward-oriented surface: six radial unions of contact ball, waist, junction ball, cone, joint ball, column and base, welded to a common 0.20 mm raft. The balls end free; there is no block above them. The waist remains at 4.40–4.60 mm above the plate. The solid construction opened and sliced in the feasibility check [M]. The current retuned six waists have connected pixel columns in the written plate's midpoint-grid witness [M]; their ChituBox slice is Unverified. Neither is a physical print result.
- The independent checker then reports H1, H2, H3, H4, H6 and H9 on the written file, with the K/W exceptions in §6.3. Its body checks do not measure the minimum section of an STL arm or waist: the calibration section measurement does that separately from the written triangles.
- Orchestrator ruling: the A1 '≥ ×2 thinner' rule is per-neck **horizontal sliced area**, with at least six qualifying main necks (§5/§6.4). The axis-normal mesh ratio does not decide C4.
- The planner, the P5 load check and the step-11 tip-family rule are not applied.
- Every file and report says "calibration piece, not a print candidate for any model".

**Reproducible entry point.** `crates/calibration/src/bin/cal1-plate.rs` accepts `<cal1.json> <read-only-individual-directory> <measured-summary.json> <empty-output-directory>`. It refuses to overwrite a nonempty directory. The measurement directory must also contain `method.json` and `a1b.json`; the individual directory must contain the original projects and `a1b.goo` for its exact grid. These owner inputs are not needed by ordinary unit tests.

Build and test through `scripts/t`. Run the assembly evidence job explicitly with `cargo run -p plinth-calibration --bin cal1-plate -- <cal1.json> <read-only-individual-directory> <measured-summary.json> <empty-output-directory>`. It writes the project, JSON evidence and SVG map; render the latter to PNG with `rsvg-convert --unlimited <output>/plate-map.svg -o <output>/plate-map.png`. Ordinary `scripts/t L1 calibration` runs the source-controlled tests without owner inputs.

### 6.2 Cap controls

The cap controls are solid bodies (STL) built to §3.3, with 48-sided sections. They have no native supports.

### 6.3 Hard limits, and the calibration-only exception

| Limit | Coupons | K controls | W witnesses | Round 25 copies (ChituBox-made) |
|---|---|---|---|---|
| H1 patch ≤ 0.50 mm² | Kept (about 0.15) | Not applicable (no contact) | Not applicable (no contact) | Reported, not enforced |
| H2 no fused contacts | Kept | Not applicable | Not applicable | Reported |
| H3 print-order continuity | Retuned plate grid: no islands [M]; new owner slice required [U] | Individual owner slices and plate grid: no islands [M] | Retuned plate grid: no islands [M]; checker unverified and new owner slice required [U] | A1b owner slice: no islands [M]; assembled slice required [U] |
| H4 lift ≥ 5.0 mm | Kept | **Exception:** rests on the plate on its own base, as on plate 6 | **Exception:** solid body rests on its own raft on the plate, as for K | As made |
| H6 gap ≥ 0.3 mm | Kept | Not applicable | Not applicable (no native supports) | Reported |
| H9 ≥ 0.10 mm across | Kept: written minimum 16-gon caliper widths 0.122598 / 0.224600 mm [M] | **Measured** (thinnest arm 0.798 mm) | Written minimum sections measured for retuned 0.125/0.229 mm heads; checker H9 not applicable to body-only mesh | Reported (A0: 0.098 mm); R3, not a production exception |
| P5 and the "unprintable as is" class | **Deliberately exceeded** | Exceeded (K-60) | — | Exceeded as made |

**Calibration-only exception (K and W).** K and W are not support layouts for a model, so H1, H2 and H6 do not apply, and both break H4 by design. The W checker verdict is **FAIL** (H4); its report lists H3 and H8 as **Unverified**, H5 topology and H7 body preservation pass, and H1/H2/H6/H9 are not applicable. The owner-sliced feasibility W had no unsupported blobs [M]. The retuned W has zero islands in the assembled midpoint-grid witness [M], but its new ChituBox slice is still required [U]. Neither result changes the checker's scope or proves a physical print. This is not a print-candidate pass. The calibration checks and independent report must distinguish two things:

- the minimum load-bearing section along every load path (arm, flare and cap for K; raft through the contact-ball equator for W), measured from written triangles by the calibration section tool. W's free ball crown above its equator is not a loaded neck and tapers to a point. The six W paths meet ±10% of their design areas; the same minimum plateau includes 4.40–4.60 mm. This is sampled measurement, not a certified continuous lower bound;
- print-order continuity, reported independently; **Unverified** is not a pass. W's closed-surface topology test does not prove sliced-layer continuity.

The exception applies only to pieces labelled K or W in the calibration entry point.

### 6.4 Feasibility, proved before the owner is asked (mandatory)

1. **One coupon per waist size** (0.229 and 0.125 mm), built and written.
   - Re-tuned by reducing only waist diameters; nominal rung IDs unchanged. The 100 waists are 0.229 mm, and W uses the same two retuned sizes. S-324's measured index was 14% low, outside the declared ±10% closeness criterion [S], so its waist is 0.125 mm.
   - Written sampled waist sections are 0.0401366 and 0.0119589 mm² [M], at the waist and within ±10% [S] of design. Written minimum opposing-face widths are 0.224600 and 0.122598 mm [M], both above H9's 0.10 mm. R1 permits calibration test pieces past production load limits; no new H9 exception is used. A0/A1b stay under R3's comparison-copy rule.
   - Two prediction methods (§3.2) use the owner's measured waist inflation and the new written midpoint-grid areas. Per band: **B-100 95.84–99.79, S-100 95.67–101.32, S-324 314.10–335.66 [U]**. Thus both 100-rung predictions are above 92 with margin, and S-324 is near 324; this is not an assembled-slice measurement or a replay.
   - **Assembled owner slice measured [M]:** lit-any and intensity-weighted waist area at every labelled layer 4.40–4.60, formation layers and native indices are in items 4–5. The explicit symmetric one-row mask test passes **180/180** after independent first-layer registration; no percentage tolerance is substituted. S-324's aggregation rule and C4's section/window rule still need an orchestrator/owner ruling before print release.
   - If a check fails, redesign the head and re-measure. Never relabel the rung.
2. **A1, exposed-section check.** Plinth makes the edit (§6.5). It is not enough that a field changed.
   - Measure the exposed narrowest section of the 10 edited tips on the written mesh and in the sliced layers, against A0.
   - A field-by-field comparison must show that only those 10 cone fields and their meshes changed.
   - ChituBox must open and save A1 with the fields intact. This depends on step-11 §2 item 1: whether ChituBox slices the stored mesh or rebuilds from the fields.
   - Judge ×2 per neck from the **horizontal ChituBox sliced area**, with lit-any and grey-weighted areas stated, not from axis-normal mesh diagnostics. Freeze the qualifying/excluded record list. Necks below ×2 are observations only and cannot decide C4. Keep A1 with at least **6 of 10** qualifying necks; with fewer than six, A1 becomes a second A0 (§3.1).
   - **A1b owner slice [M]: zero islands; 6/10 qualify under the binding ±1-layer station window.** Frozen window-qualified cone records **1, 11, 13, 15, 17, 19**; observations only **3, 5, 7, 9**. Ratios and lit-any/grey-weighted equality are in §3.1 and the raw input measurements. A1b is the A1 piece in both bands. The assembled native slice confirms zero islands and the same six under that window, but a broader isolated-neck scan gives only **5/10** (item 5). **C4 scoring and the keep-A1 decision need a ruling on that sensitivity.** Placement preserves stored geometry/support records under translation; whole-pixel steps do not guarantee identical native raster pixels or an exact raster phase. A1b-specific open/save field preservation remains Unverified until a new saved project is compared; the earlier A1 round-trip is not silently transferred to A1b.
3. **W and K open and slice [O/M].** Their written minimum sections are as in §6.3. K is byte-preserved from the feasibility files; both copies of K and retuned W are present and continuous in the assembled owner slice (items 4–5).
4. **The assembled plate opens and slices (H8) [O/M].** The owner reopened the project, found front-W on the plate, and re-sliced/re-saved `cal1.goo`. The frozen project is unchanged. ChituBox reports **5.858 mL and 1 h 1 min 25 s [O]**; the GOO header stores **5857.640625 mm³ and 3685 s [M]**. All 16 pieces, individually located by their translated XY footprints, first light at **index 0 (one-based layer 1), labelled Z 0.050000001 mm [M]**. Front-W and back-W both start there, not 3 mm above the plate. Their model records have identical zero Z translations and identical geometry/support absence; only names and XY translations differ. The strict reader validates the reserved model-record bytes as zero; there is no placement-field difference in this project that explains the transient display incident. Its cause remains **Unverified**, not attributed to a file defect.
   - Header [M]: 15120 × 6230 pixels, pitch **0.013999999516 × 0.019000000441 mm**, layer height **0.050000000745 mm**, **525 layers**, normal exposure **2.35 s**, **3 bottom layers at 20 s**, **5 transition layers**, AA/grey/blur **4/3/2**, profile **Anycubic WW ABS-Like Clear**. Exposure, pitch, layer height, profile and other slicing settings match the individual slices; their layer counts, volumes, weights, prices and shorter-piece print times differ as expected. Stored 0.05 mm lift/retract distances and speeds are not verified physical tilt kinematics.
   - **Zero islands [M]** across all 525 layers: 8-connected lit-any components, zero exact XY overlap with the preceding layer defines an island, first layer excluded. This proves the specified pixel-overlap check, not cured attachment or print success. Raw per-contact/per-layer measurements, all 16 first-layer checks and the full 525-layer area comparison are in `plinth-review/2026-11-02-cal1-plate/slice-check.json` (sibling of the slot root's parent). `cal1-slice` reads the project/GOO without changing either input.
5. **Critical sliced layers measured [M]; not a print release.** Areas below are horizontal pixel areas, not axis-normal strength or cured-material measurements. **All layer numbers here and generic `layer` fields in `slice-check.json` are one-based; only explicitly named `layer_0based` fields are zero-based.** Z values are native labels, not certified mesh intersection planes. Waist ranges retain all four contacts and all five labelled layers **4.40–4.60 mm** in each band. Each row applies to front and back independently; their extrema agree, but individual contact/layer values can differ.

   | Piece, each band | Waist lit-any = weighted area (mm²) | Body maximum lit-any / weighted (mm²) | As-sliced index lit-any / weighted | Check |
   |---|---|---|---|---|
   | B-100 | 0.046550–0.048412 | 18.254250 / 18.062417 | 94.265–98.036 / 93.274–97.005 | **Both copies clear 92 at every contact/layer**; lit-any minimum is below predicted 95.84–99.79 |
   | S-100 | 0.046018–0.047082 | 18.051026 / 17.999449 | 95.849–98.065 / 95.575–97.785 | **Both copies clear 92 at every contact/layer**; lit-any range is inside predicted 95.67–101.32 |
   | S-324 | 0.013566–0.015960 | 18.051026 / 17.999449 | 282.754–332.652 / 281.946–331.701 | Some readings outside predicted 314.10–335.66 and nominal-324 ±10% (291.6–356.4); **aggregation ruling required**, not an unconditional failure or pass |

   - The index uses body maximum / four / individual waist area: equal-quarter P4 arithmetic, not measured load sharing or a load-model replay. **S-324 aggregation sensitivity:** requiring every reading to clear 291.6 fails: five of 20 front and three of 20 back contact/layer values are below it by both area measures. Using each contact's narrowest sampled waist instead gives lit-any indices **320.1, 332.7, 326.3, 320.1 front; 320.1, 332.7, 332.7, 320.1 back**, all within ±10% and the prediction range. Means of the 20 lit-any indices are **303.1 front / 305.5 back**, within ±10% but below the prediction range. The plan fixed ±10% closeness, not how to combine readings; no unconditional S-324 verdict is assigned until ruled. If narrowest section governs S-324, apply the same principle to C4, where the broader sampled neck minimum removes record 1. Do not relabel S-324 or choose different rules just to make both pass. Every 100-rung reading still clears 92; the missed B-100 prediction envelope remains reported.
   - **Registered one-row agreement: 180/180 pass [M].** Explicit operational test: midpoint centre-filled written waist mask and native lit-any mask must each lie within the other's one-pixel Chebyshev dilation (±1 X pixel and ±1 Y pixel, approximately 14 × 19 µm). Register each piece once from its first-layer native lit-any bounding-box centre against its continuous written first-midpoint section centre; round the offset to whole pixels, retain the residual, then use that same shift for every layer. This fit uses no waist or tip pixels. Native-minus-predicted shifts: **−1 X** for front-B-100/front-S-324; **−1 X, −1 Y** for back-S-100/back-S-324; **−1 Y** for back-W; **zero** for front-S-100/back-B-100/front-W. Unregistered masks compare different positions; they do not diagnose a ChituBox section-plane defect. No native or mesh pixels remain beyond the opposite mask's allowance, and the tolerance is unchanged. Tests reject a missing stripe even after registration and a two-pixel residual displacement. This operational definition is explicit, not retroactively an owner-agreed error bound or a cure certificate.
   - Coupon waists and balls remain four separate supported blobs through labelled **5.00 mm**. At the ball equator label **4.90 mm**, B ball areas are **0.204820–0.205352 mm²**, S ball areas **0.201362–0.202426 mm²** (lit-any = weighted). Maximum isolated formation-area / minimum waist-area ratios are **4.400–4.411 (B-100), 4.349–4.399 (S-100), 14.283–14.902 (S-324)**, not measured forces. First body pixels appear at **layer 101, label 5.05 mm**, with one body blob; all first three body layers overlap below. No head is missing or prematurely merged with another head in those inspected layers.
   - Both W copies retain six separate heads through 5.00 mm and end at **label 5.15 mm**. Thin waists: lit-any **0.015428–0.015960 mm²**, weighted **0.0119564–0.0119606 mm²**. Thick waists: lit-any **0.047614–0.048944 mm²**, weighted **0.0401295–0.0401410 mm²**. Equator balls are **0.206150–0.208544 mm² lit-any**, approximately **0.191186–0.191191 mm² weighted**. Formation-area ratios are **12.917–13.517 lit-any / 15.985–15.991 weighted** for thin waists and **4.261–4.341 / 4.763–4.764** for thick waists. W has no body index. **W is grey-AA solid geometry; coupon support blobs are full intensity. Their weighted waist sections are therefore not identical despite matching written geometry.** W observations cannot silently certify identical coupon raster or strength.
   - K arms, flares and first full cap layers are present, one connected blob with overlap below. K-15 arm: **2.045806 mm² lit-any, 1.994746–1.994884 mm² weighted** (written 1.994870). K-60: **0.524552–0.525084 mm² lit-any, 0.498660–0.498988 mm² weighted** (written 0.498718). Flares first appear at **label 8.55 mm**; full caps first appear at **9.15 (K-15)** and **9.25 mm (K-60)**, about **30.194458 lit-any / 29.99778–29.99780 weighted mm²**. Last labels are 9.60 and 9.70 mm. No missing or unexpectedly thin K section is detected by this pixel inspection.
   - A0/A1 **binding-window** isolated-neck ratios, records **1,3,5,7,9,11,13,15,17,19**: front **2.390,1.915,1.382,1.470,1.503,3.089,2.119,2.128,3.353,3.346**; back **2.402,1.913,1.382,1.477,1.494,3.093,2.117,2.128,3.353,3.346**. Lit-any and weighted ratios agree. The frozen list **1,11,13,15,17,19** exceeds ×2 in both copies **under the nearest-station ±1-layer protocol**; 3,5,7,9 remain observations only. Against individual A1b, the largest absolute ratio change is **0.010122**; no window-list membership changes. Body-merged candidates at records 13/15 are excluded, not counted as necks. **C4 and keep-A1 are not released:** scanning the broader isolated axis-probed neck instead finds A0 record 1 at **layer 60, label 3.00 mm: 761 px / 0.202426 mm²**, two layers above the binding window. A1's minimum is **387 px front / 386 px back**, both at **layer 54, label 2.70 mm**. Ratios become **1.966408 front / 1.971503 back**, below ×2. Only **11,13,15,17,19 (5/10)** then qualify, which fails the six-neck keep rule. The broader scan also gives front record 3 **1.7186** (still excluded), records 13/15 **2.3166 / 2.2926** (A1 minima 578/581 px at layer 400, label 20.00 mm), and unchanged 11/17/19. It samples the full isolated blobs along each tracked cone axis (fraction −0.1–1.1 at labelled Z; body-width exclusions unchanged), not a certified continuous or axis-normal minimum. The orchestrator/owner must rule on the section/window definition before this list can score C4; the binding result is retained, not silently replaced.
   - Both A0 lip trios have lit overlap through their formation/joining layers. Their probed blobs join the body by **label 3.10 mm** (centre lip) and **3.40 mm** (side lips). A binary merged mask cannot measure the interface patch area: that remains Unverified. The two highest-ratio unchanged branch rods, records **31/33**, have no missing axis-probe blobs and sampled isolated horizontal areas **0.021812 / 0.023142 mm²** (lit-any = weighted) in each A0 copy. These are the already thin native 0.10 mm rods, not newly edited necks; sampled horizontal areas do not certify their strength.
   - Total native lit-any area peaks at **layer 2, label 0.10 mm: 777.349879 mm²**, weighted **775.860974 mm²**. The mesh-grid maximum is **771.620771 (centre) / 778.510171 (touch) mm²**. The native profile is inside those two per-layer masks' area envelope on **411/525 layers**, not all layers. Maximum absolute lit-any differences are **5.750920 mm² versus centre at layer 192 (9.60 mm)** and **2.100336 mm² versus touch at layer 238 (11.90 mm)**. Integrating native lit-any areas gives **5857.643370 mm³**, agreeing with the stored volume to **0.002745 mm³**; weighted integration gives **5802.628936 mm³**, a different metric, not missing resin.
   - **Not ready to print:** resolve the S-324 aggregation and C4 section/window rulings consistently; keep the raw measurements. The registered boundary check now passes all 180 comparisons. A1b save-field preservation, both-hash stamping/committing the prediction table, revised cross-family review (item 8) and physical outcomes remain Unverified. Stale assembled-slice status outside §6.4 still needs an orchestrator-authorized documentation update; this measurement task's plan write set is §6.4 only. No physical print is requested by this check.
6. **Both SHA-256 hashes frozen [M].** Unchanged `.chitubox`: `7d507a3dfa3128278d4717ce132de3128341d9e1aa044d2ca8b7df3d4df358cc`. Owner's re-sliced/re-saved `cal1.goo`: `fc7540e071e0a16f2e8dc179af5c640f2793b281067862dab09697912f313d94`. The GOO reader also reproduces every input byte on a lossless round trip in memory. Results apply only to those two files; neither was modified by the measurement tools.
7. **Replay every piece** and write the prediction table (§7.1). **Computed on the frozen written project:** `crates/calibration`'s `cal1-replay` uses the unchanged support load API for all 16 pieces, including extra waisted-head junctions and the solid K/W paths. `predictions.json` and `predictions.md` contain all four sharing maxima, section, layer/Z, contact, warnings and preregistered calls. Front/back geometry is byte-identical after removing only names and XY placement [M], so their results match. Waists limit the coupons; omitting them makes the sabotage test fail its expected load check. Ratios are model estimates [U], not native sliced measurements or physical loads. A0 gives rigid-carrier **2292.948 [U]** and additive **311.221 [U]**; a rerun of the historical binary gives **2292.951 / 312.672 [U]**, reproducing the earlier rounded **2293 / 313**. The additive difference is explained by the current nominal patch area instead of the checker's shrunken lower patch area (same peak peel area; 0.466% larger denominator). Equal-share and tributary do not reproduce: historical **9669.53 / 42945.54 [U]**, current **3171.32 / 14084.87 [U]**. These unscored first-exposure artefacts depend on clipped patch history versus the API's tilted-disc approximation (about 3.05× difference [U]); they do not set the additive call. The API's patch exposure, support self-peel and sharing remain assumptions [U]. The summary index/contact/layer now explicitly use additive; the table includes contrast, sampled narrowest mesh section and C4 qualifying-neck diagnostics, not substitutes for horizontal sliced qualification. Assembled native slice measurements and both hashes are in items 4–6 [M]; they do not validate the replay's load assumptions. C4/S-324 rule decisions, both-hash stamping, cross-family review and committing the table remain unresolved; no print request follows from this replay alone.
8. **Cross-family review** (PRD §7 rule 2) of the revised spec, feasibility measurements and replay remains a pre-print requirement. This is GPT/sol measurement work with per-piece registration and C4/S-324 sensitivity results; the revised result has not been independently reviewed. The review packet and unresolved questions are in `slice-check.json`.

### 6.5 Owner rulings for this plate (2026-11-01/02) [O]

These apply only to the labelled calibration plate CAL-1, never to the production path:

- **R1.** Test pieces may exceed P5 and the "unprintable as is" class, and may use the waisted calibration tip.
- **R2.** Test pieces may be meant to break. That does not conflict with the PRD §7 "never doomed to fail" rule for this plate.
- **R3.** The four Round 25 copies are ChituBox comparison copies. Plinth's limits are reported for them, not enforced.
- **A1 edit.** Plinth makes the A1 edit to the support records, not the owner in ChituBox. The §6.4 item 2 checks still apply: the edit must survive a ChituBox save, and the exposed neck must be thinner in the sliced pixels.
- **Light.** "Light prints fine" is the owner's judgement, not a print. A0 is untested.
- **Resin.** Anycubic WW ABS-Like Clear, using the ChituBox profile already used for the owner's feasibility slices.
- **Plate size.** Sixteen calibration pieces at about 5–6 mL is accepted. Actual assembled resin volume and print time must still come from ChituBox; acceptance of the estimate is not a volume measurement.

---

## 7. Printing and recording

### 7.1 Before printing

Commit the prediction table. Per piece: the measured narrowest section, the replayed index, layer and contact, the compact-law call (fail / hold / no call), and the contrast it belongs to. Stamp it with both frozen hashes. The owner scores before seeing it.

**Status:** written-project replay tables exist for all 16 pieces in `plinth-review/2026-11-02-cal1-plate/predictions.{json,md}`. All C1–C4 members receive the P4 additive clear-fail prediction [U], conditional on assembled slice confirmation and the contrast's formation/site rules; K's preregistered replication expectation remains K-15 hold / K-60 fail, not a P4 clear call. W is unscored. The summary replayed index/contact/layer fields explicitly use additive, the model behind the P4 call. The Markdown table includes contrasts and sampled narrowest sections [M], the K replication expectations [U], and per-neck C4 diagnostics with axis-normal mesh sections distinguished from the horizontal sliced qualification measure. A0/A1 equal-share and tributary maxima are labelled patch-model-sensitive first-exposure artefacts [U], with the historical disagreement disclosed. Written-project hash is frozen [M]; sliced-file hash and native sliced indices remain **Unverified**. A same-family reviewer checked the replay; its labelling/disclosure corrections are applied, but cross-family review, committing the table and physical testing remain **Unverified**.

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
- The tank-clean sheet shows which pieces left remnants on the film, and where. That helps locate where a piece separated: at the waist, at the patch, or higher in the body. **The owner photographs the tank-clean sheet with a ruler before discarding it.**

### 7.4 Recording, kept small

- **Before removing anything:**
  - One photo per band from the front and one from above, with a ruler in view, before washing.
  - Run the tank-clean exposure and photograph the cured sheet before discarding it (§7.3). Note which remnants match which pieces, using the plate map.
  - After washing, check the basket for lost pieces and flakes.
- **One overview sheet** (the plate-map picture). For every piece, one mark: **present / partly present / gone**.
- **Close inspection only of preregistered contacts:**
  - Coupons: their 4 contacts. Each is intact, broken at the waist, or broken elsewhere / cannot tell.
  - W: each head's ball is formed or not formed.
  - A0 and A1: the bottom lip, the two branch rods under the lower underside flanks, the underside heads and the central rib. Each is intact, broken, or part damaged there, marked on the picture by anatomy.
- **Removal (Round 25 only, separate):** easy / normal / hard per zone, and any damage, with a photo. This is the PRD §6.4 record and is never mixed with print outcomes.
- The owner sends photos and the marked sheet, or dictates short lines. The orchestrator transcribes them into `reference/experiments/cal-1-results.md` and asks him to confirm the transcript.

---

## 8. Reading the result

### 8.1 Per contrast

| Outcome | Reading |
|---|---|
| K-15 present, K-60 gone (per band) | Those two controls reproduced plate 6. Nothing more |
| K does not reproduce | Reported beside every result. It does not void A0, A1 or coupon survivals. The direct A0 evidence stands even if K-60 survives |
| C1: block gone, strip present (and the strip failure not in question) | State both assembled as-sliced indices. If both are ≥92 as predicted, the strip survival reverses its compact-law clear-fail call; if not, report only the calls actually made at measured indices. Cause not isolated (§3.2) |
| C1: both gone | No survival separation at this tested level, conditional on formation (W) and on the failure mode at the waists |
| C1: both present | Both configurations survived at their assembled measured indices. State which compact-law calls those measured indices actually support; do not use predicted values as measurements |
| C2: S-324 present | This strip configuration survived at its assembled measured index (nominal ID S-324) on CAL-1. Observed survival for this configuration only |
| C3: A0 present | This Light copy survived on this plate. The compact full-area clear call is reversed for it |
| C3: A0 failed | Record where, by anatomy, and compare with the replay's top list as an observation. The tank-clean sheet may show where it separated |
| C4: A0 and A1 present | Both tested configurations survived. Mechanism not established |
| C4: A0 present, A1 failed at a qualifying edited neck | Survival differed with the exposed section at that qualifying underside neck, subject to §7.3; a failure at an excluded neck is an observation only |
| Bands disagree | Report both; no interval; replicate on a later plate |

### 8.2 Intervals

An interval for strips, stated using the two assembled measured indices, is given only under §2 rule 7: monotone across both bands, with waist breaks at the failed rung. State the as-sliced indices beside the nominal rung IDs. Each edge rests on 2 pieces from one plate and one resin.

### 8.3 Validity

- If W shows the ball did not form on a waist size, no failure at that waist counts as a load failure.
- If everything fails, including K-15, W and both A0 copies: fit nothing. The cause is unresolved.

### 8.4 What changes after the plate

- No design ratio is computed from the plate. In particular, no "lower edge ÷ 2": with n = 1 or 2 per edge, that is not a defensible derivation (`plate-06-results.md`, Finding 2).
- The owner receives the evidence per contrast: outcomes, reversed or confirmed clear calls, and any monotone interval.
- A change to A2 (full-area load for strips) or to P5 is the owner's ruling (PRD §7). If he rules a change, plinth re-runs Round 25. The new layout is reported beside Light as a measurement, not a target.
- The PRD §8.4 side-by-side print still follows. CAL-1 does not replace it.

---

## 9. Order of work (dependencies, no dates)

1. Owner answers to §11.
2. Settle step-11 §2 item 1 (plinth makes the A1 edit, §6.5).
3. Build the calibration entry point and the waisted-head builder; write the coupons, W and K.
4. Feasibility (§6.4 items 1–3). Redesign and re-measure any piece that fails, recording the change.
5. Assemble the plate with the seeded shuffle; §6.4 items 4–8; commit the prediction table.
6. Ask the owner to print. He prints, then runs the tank-clean exposure.
7. Photos, film, vat and basket checks, overview sheet, preregistered contacts, removal notes (§7.4).
8. Transcribe; score C1–C4 and K under §2 and §7.3. A different-family reviewer checks the scoring against the raw marks before the owner sees it.
9. Owner rulings, if any. Update PRD §7 and step-11 §4 by replacing the superseded values.

---

## 10. Deferred to later plates

Each needs its own controlled pair, with a common formed carrier and a common activation stage, in which the predictors being compared make opposite clear calls:

- **Sharing rule** (additive, rigid-lever, tributary): lever and outrigger pieces.
- **First-exposure patch:** a matched early/late activation pair.
- **Edge reach** past the last contact, and **column slenderness** (P12).
- **Patch-limited contacts on sloped faces** (the Light lip is patch-limited; A1 does not reach it).
- **Fewer contacts on Round 25.**
- **Two-contact strip challenges** at the H9 minimum, strip orientation, other tilt angles, wider strips, and replicates at any interval edge found here.

---

## 11. Questions for the owner

The answers of 2026-11-01/02 are recorded in §6.5 and §7.3. Only these remain open:

1. **Resin lot number.**
2. **Tank temperature for this job.**
