# Calibration print plan: Light on Round 25, and thin strips against compact bodies (plate CAL-1)

Status: individual owner slices measured, 2026-11-01; corrected A1b awaits the owner's new slice. The full plate is not assembled. Owner rulings are in §6.5. Nothing here is printed until the owner says yes. Remaining owner questions are in §11. It informs the FR-20a load model (`docs/design/step11-construction.md` §4) and PRD §7 (P4, P5). It does not calibrate the whole model.

**Labels on numbers** (as in the step-11 design):

- **[O]** owner ruling or owner practice;
- **[S]** orchestrator starting value: a choice, not evidence;
- **[M]** measured (on this printer, or on a written file);
- **[C]** ChituBox convention or vendor data;
- **[U]** unmeasured estimate; never decides alone.

Piece counts, sizes, sections and resin in this document are planning values **[S]/[U]**. The feasibility step (§6.4) and the replay of the written files replace them before printing. Where the measured values differ, the design is changed openly and the change is recorded; ratios are never changed silently.

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

There are two bands, front and back. Each holds one complete set of 8 pieces. **Planning total: 16 pieces.**

### 3.1 Round 25 copies (4: A0 ×2, A1 ×2)

| ID | Supports | Purpose |
|---|---|---|
| **A0** (front and back) | ChituBox Light exactly as ChituBox made it (17 contacts) | Aim (a): direct survival test. The compact full-area law makes a clear fail call (additive 313 ≥ 92). Nothing makes an opposing clear call |
| **A1** (front and back; current candidate A1b) | A0 with only the 10 main cone start radii and their canonical meshes changed. Start diameter is normally 0.104 mm; A1b uses 0.140 mm at cone record 3 and 0.200 mm at record 5 to support the ball's first pixels. Balls, every endpoint, cone ends and lengths, columns, branches and raft unchanged | Aim (a): does a thinner exposed neck at the underside heads change survival? |

**What A1 changes [M].** The written axis-normal minimum with the initial 0.052 mm start radius was 0.038954 mm² against A0's 0.151371 mm² (×3.886). That is not the horizontal section the printer forms. In the owner's slices, eight of ten necks meet ×2; cone records 7 and 9 give only ×1.478 and ×1.494 and are observations, not C4 scores. The sampled comparison uses the minimum isolated blob in the nearest labelled layer and its immediate neighbours at each file's written neck station. Grey-weighted and lit-any areas are identical for these full-intensity support blobs.

**A1b continuity correction [M/U].** The inclined thin cone let the low cap of the unchanged contact ball at record 4 start separately at layer 62 (label 3.10 mm; midpoint 3.075 mm), near cone record 5. A0's wider cone carries this cap. The builder now requires the written cone/ball pair to overlap below at every midpoint on the 14×19 µm, 0.05 mm grid before emitting each thinned cone. It tests both centre-filled and cell-touching raster boundaries. This selects start radii 0.070 mm at record 3 and 0.100 mm at record 5; the other eight remain 0.052 mm. All 525 layers of the complete A1b written mesh have zero islands under both checks **[M]**. The local raster is not ChituBox: its cell-touching baseline finds the same original island layer and bounds but eight pixels instead of ChituBox's six. **A1b needs the owner's new slice to confirm continuity and re-qualify each neck [U].** No tip moves and no ball changes.

The lip contacts stay limited by their unchanged patches (0.03–0.04 mm²), so **A1 does not change the lip**. Under the orchestrator ruling in §5/§6.4, keep A1 if at least six of the ten necks qualify by horizontal sliced area. Otherwise both A1 slots become A0 copies and aim (a) reduces to "does A0 survive, n = 4".

### 3.2 Matched compact and strip coupons (3 per band)

**Bodies.** Both are 5.0 mm tall (100 layers), on 4 contacts, lift 5.0 mm. The nominal layer area is 18.0 mm²; the written block is 18.0625 mm² (4.25²), and the strip is 18.0 mm². Their measured sliced areas and indices are recorded below:

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

| ID | Body | Waist diameter | Design waist section | Nominal index | As-sliced index [M] | Compact-law call at measured index |
|---|---|---|---|---|---|---|
| **B-100** | block | 0.242 mm | 0.0448 mm² | 100 | 88.43–89.36 | no clear call (<92) |
| **S-100** | strip | 0.242 mm | 0.0448 mm² | 100 | 89.76–90.72 | no clear call (<92) |
| **S-324** | strip | 0.135 mm | 0.0139 mm² | 324 | 278.12 (about 280) | clear fail |

**As-sliced arithmetic [M], not a measured load law.** The native coupon waists are full-intensity and slice about 11–20% larger than their design/written sections: B 0.051072–0.052668 mm², S-100 0.049742–0.051338 mm², S-324 0.016226–0.016758 mm² over labelled layers 4.40–4.60. Lit-any equals intensity-weighted area there. Each index above is the largest lit-any body-layer area divided by four and by that contact's minimum waist area across those five layers; the range is over the four contacts. Sliced body areas are 18.254250 mm² (B) and 18.051026 mm² (S). The equal quarter-share is P4 arithmetic, not measured sharing. Keep the nominal IDs, but state the measured index beside them in predictions and results. At these indices C1 has no compact-law clear-fail call under the fixed ≥92 rule; do not claim a reversal of a call that was not made.

**Formation load.** The ball's widest layer (0.196 mm², its equator, 0.12 mm below the body) prints on the waist before the body starts. That puts a formation index of about 4.4 on the 0.242 mm waist and about 14 on the 0.135 mm waist **[U]**. The W witness measures whether that forms.

**W, formation witness (1 per band).** Six free-standing waisted heads with no body on them, built exactly as above: three with the 0.135 mm waist, three with the 0.242 mm waist. They show whether the ball forms on each waist under its own formation load only.

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

**Space [U].** Footprints with supports: Round 25 about 30 × 22 mm; block about 8 × 8; strip about 16 × 6; cap control about 10 × 10; W about 10 × 4. One band needs about 140 mm of the 211.68 mm, with 6 mm between pieces.

**Combined layer profile.** All pieces cure in one exposure per layer, so the plate-wide cured area per layer changes the peel and the heat at each height. The replay records the plate's total cured area for every layer. Coupon bodies end near plate z 10 mm and cap controls near 9.7 mm; the Round 25 copies run to about 28 mm. Both bands see the same profile. It is reported, not modelled.

**Labels and scale.** IDs and one 10 mm scale bar per band are engraved on raft tabs, inside the first 1.2 mm above the plate. That is outside every measured layer. A top-view plate-map picture is the main key.

**Real prints.** None are planned on this plate.

**Resin, planning estimate [U]:** Round 25 copies 4 × (0.85 mL model **[M]** + about 0.2 mL supports) = 4.2 mL; coupons, controls, witnesses and their supports about 1.1 mL; **total about 5–6 mL**. ChituBox's sliced volume replaces this (§6.4).

---

## 5. Preregistered contrasts

Only these are scored.

**C4 orchestrator ruling (2026-11-01) [S].** Judge ×2 **per neck on the horizontal sliced-layer area**, which is what the printer forms, not the axis-normal mesh section or the cone radius field. Exclude a neck below ×2 from C4 and report it only as an observation. Keep A1 as A1 if at least **6 of 10** main necks qualify; with fewer than six, replace both A1 slots by A0. The initial owner slice qualifies eight: records 7 and 9 (×1.478/×1.494) are excluded. The current corrected A1b must be sliced and its qualification list frozen before printing; do not reuse the earlier list without checking. A whole-piece outcome or a failure at an excluded neck does not become a scored qualifying-neck effect.

| # | Contrast | Compact full-area law (P4) | Other reading | Scored as |
|---|---|---|---|---|
| **C1** | B-100 against S-100, same band, ×2 bands | No clear call: measured indices 88.43–90.72 are below 92 | Assumed scenario, no clear call: the strip survives where the block fails | Per band: the (block, strip) outcome pair, with measured indices |
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
- W is a solid body with no native supports, as for K. ChituBox v1.4.0 refused the native-support-only W with no body triangles. Its replacement is one closed, outward-oriented surface: six radial unions of contact ball, waist, junction ball, cone, joint ball, column and base, welded to a common 0.20 mm raft. The balls end free; there is no block above them. The waist remains at 4.40–4.60 mm above the plate. The owner opened and sliced the replacement; its six waists and free tips form connected pixel columns in w.goo **[M]**. This is not a physical print result.
- The independent checker then reports H1, H2, H3, H4, H6 and H9 on the written file, with the K/W exceptions in §6.3. Its body checks do not measure the minimum section of an STL arm or waist: the calibration section measurement does that separately from the written triangles.
- Orchestrator ruling: the A1 '≥ ×2 thinner' rule is per-neck **horizontal sliced area**, with at least six qualifying main necks (§5/§6.4). The axis-normal mesh ratio does not decide C4.
- The planner, the P5 load check and the step-11 tip-family rule are not applied.
- Every file and report says "calibration piece, not a print candidate for any model".

### 6.2 Cap controls

The cap controls are solid bodies (STL) built to §3.3, with 48-sided sections. They have no native supports.

### 6.3 Hard limits, and the calibration-only exception

| Limit | Coupons | K controls | W witnesses | Round 25 copies (ChituBox-made) |
|---|---|---|---|---|
| H1 patch ≤ 0.50 mm² | Kept (about 0.15) | Not applicable (no contact) | Not applicable (no contact) | Reported, not enforced |
| H2 no fused contacts | Kept | Not applicable | Not applicable | Reported |
| H3 print-order continuity | No islands in the owner's slices **[M]** | No islands in the owner's slices **[M]** | Checker unverified; all 103 owner-sliced layers have no islands **[M]** | Reported; A1b awaits a new owner slice |
| H4 lift ≥ 5.0 mm | Kept | **Exception:** rests on the plate on its own base, as on plate 6 | **Exception:** solid body rests on its own raft on the plate, as for K | As made |
| H6 gap ≥ 0.3 mm | Kept | Not applicable | Not applicable (no native supports) | Reported |
| H9 ≥ 0.10 mm across | Kept (thinnest waist 0.135 mm) | **Measured** (thinnest arm 0.798 mm) | Written waist sections measured separately (0.135/0.242 mm design); checker H9 not applicable to body-only mesh | Reported (A0: 0.098 mm) |
| P5 and the "unprintable as is" class | **Deliberately exceeded** | Exceeded (K-60) | — | Exceeded as made |

**Calibration-only exception (K and W).** K and W are not support layouts for a model, so H1, H2 and H6 do not apply, and both break H4 by design. The W checker verdict is **FAIL** (H4); its report lists H3 and H8 as **Unverified**, H5 topology and H7 body preservation pass, and H1/H2/H6/H9 are not applicable. Separately, the owner has opened and sliced W and its written GOO has no unsupported blobs **[M]**; this does not change the checker's scope or prove a physical print. This is not a print-candidate pass. The calibration checks and independent report must distinguish two things:

- the minimum load-bearing section along every load path (arm, flare and cap for K; raft through the contact-ball equator for W), measured from written triangles by the calibration section tool. W's free ball crown above its equator is not a loaded neck and tapers to a point. The six W paths meet ±10% of their design areas; the same minimum plateau includes 4.40–4.60 mm. This is sampled measurement, not a certified continuous lower bound;
- print-order continuity, reported independently; **Unverified** is not a pass. W's closed-surface topology test does not prove sliced-layer continuity.

The exception applies only to pieces labelled K or W in the calibration entry point.

### 6.4 Feasibility, proved before the owner is asked (mandatory)

1. **One coupon per waist size** (0.242 and 0.135 mm), built and written.
   - On the written mesh, measure the minimum load-bearing section along the path. It must be at the waist and within ±10 % **[S]** of the design section.
   - Open and slice in ChituBox. Measure both lit-any and intensity-weighted waist area in the sliced layers. It must agree with the mesh within one pixel row; define that boundary tolerance explicitly rather than replacing it with a percentage. The measured native waists are 11–20% larger by area (§3.2); the one-row criterion is not yet declared passed.
   - Compute each rung's as-sliced index from its sliced body and waist areas and state it next to the nominal ID/index. Current individual files give B-100 88.43–89.36, S-100 89.76–90.72, S-324 278.12 **[M]**. Recompute for the assembled plate.
   - If either check fails, redesign the head and record the change. Do not relabel the rung.
2. **A1, exposed-section check.** Plinth makes the edit (§6.5). It is not enough that a field changed.
   - Measure the exposed narrowest section of the 10 edited tips on the written mesh and in the sliced layers, against A0.
   - A field-by-field comparison must show that only those 10 cone fields and their meshes changed.
   - ChituBox must open and save A1 with the fields intact. This depends on step-11 §2 item 1: whether ChituBox slices the stored mesh or rebuilds from the fields.
   - Judge ×2 per neck from the **horizontal ChituBox sliced area**, with lit-any and grey-weighted areas stated, not from axis-normal mesh diagnostics. Freeze the qualifying/excluded record list. Necks below ×2 are observations only and cannot decide C4. Keep A1 with at least **6 of 10** qualifying necks; with fewer than six, A1 becomes a second A0 (§3.1).
   - A1b's own midpoint-grid witness reports no islands, but the original A1 island proves that mesh/field agreement alone is insufficient. The owner must slice A1b to confirm no island and re-measure all ten necks before this step passes.
3. **W and K** open and slice. Their minimum sections and continuity are as in §6.3.
4. **The assembled plate** opens and slices (H8). Record ChituBox's resin volume and print time.
5. **Inspect the critical sliced layers:**
   - each coupon's waist layers, ball equator layer and first body layers;
   - the K arms, flares and first cap layer;
   - A0 against A1 at the edited tips;
   - the A0 lip trio and the two branch rods with the highest rigid-lever ratio.
6. **Freeze the SHA-256 hashes** of the `.chitubox` project and of the sliced printer file. Results are valid only for those two files.
7. **Replay every piece** and write the prediction table (§7.1).
8. **Cross-family review** (PRD §7 rule 2) of the spec, the feasibility measurements and the replay.

### 6.5 Owner rulings for this plate (2026-11-01) [O]

These apply only to the labelled calibration plate CAL-1, never to the production path:

- **R1.** Test pieces may exceed P5 and the "unprintable as is" class, and may use the waisted calibration tip.
- **R2.** Test pieces may be meant to break. That does not conflict with the PRD §7 "never doomed to fail" rule for this plate.
- **R3.** The four Round 25 copies are ChituBox comparison copies. Plinth's limits are reported for them, not enforced.
- **A1 edit.** Plinth makes the A1 edit to the support records, not the owner in ChituBox. The §6.4 item 2 checks still apply: the edit must survive a ChituBox save, and the exposed neck must be thinner in the sliced pixels.
- **Light.** "Light prints fine" is the owner's judgement, not a print. A0 is untested.

---

## 7. Printing and recording

### 7.1 Before printing

Commit the prediction table. Per piece: the measured narrowest section, the replayed index, layer and contact, the compact-law call (fail / hold / no call), and the contrast it belongs to. Stamp it with both frozen hashes. The owner scores before seeing it.

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
- The tank-clean sheet shows which pieces left remnants on the film, and where. That helps locate where a piece separated: at the waist, at the patch, or higher in the body. **The owner photographs the tank-clean sheet, with the scale bar or a ruler, before discarding it.**

### 7.4 Recording, kept small

- **Before removing anything:**
  - One photo per band from the front and one from above, with the scale bar in view, before washing.
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
| C1: block gone, strip present (and the strip failure not in question) | The strip at index 89.76–90.72 survived where the block at 88.43–89.36 did not. Neither has a compact-law clear call under the ≥92 rule, so this is not a clear-call reversal. Cause not isolated (§3.2) |
| C1: both gone | No survival separation at this tested level, conditional on formation (W) and on the failure mode at the waists |
| C1: both present | Both configurations survived at their measured indices near 89–91. Neither had a compact-law clear call at this level |
| C2: S-324 present | This strip configuration survived at measured index 278.12 (nominal 324) on CAL-1. Observed survival for this configuration only |
| C3: A0 present | This Light copy survived on this plate. The compact full-area clear call is reversed for it |
| C3: A0 failed | Record where, by anatomy, and compare with the replay's top list as an observation. The tank-clean sheet may show where it separated |
| C4: A0 and A1 present | Both tested configurations survived. Mechanism not established |
| C4: A0 present, A1 failed at a qualifying edited neck | Survival differed with the exposed section at that qualifying underside neck, subject to §7.3; a failure at an excluded neck is an observation only |
| Bands disagree | Report both; no interval; replicate on a later plate |

### 8.2 Intervals

An interval for strips (for example "survived near measured index 90, failed at 278") is stated only under §2 rule 7: monotone across both bands, with waist breaks at the failed rung. State the as-sliced indices beside the nominal rung IDs. Each edge rests on 2 pieces from one plate and one resin.

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

The answers of 2026-11-01 are recorded in §6.5 and §7.3. Still open:

1. **Size and resin:** is 16 pieces at about 5–6 mL acceptable?
2. **Settings:** which resin and lot? The default ChituBox profile at 0.05 mm, with the tank at 30 °C?
