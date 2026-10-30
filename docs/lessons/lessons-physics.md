# Plinth restart: what we actually know about the physical problem (2026-10-29)

Scope: read-only extraction from the memory notes, `plinth-evidence/`, Craig's ChituBox export
(`Chitubox Projects/chitubox-support-specs.cfgx`) and the repo's constants registry
(`plinth-3d/crates/plinth-core/src/constants/{evidence,placeholders}.rs`) plus the plate result records in
`plinth-3d/docs/engineering-journal/reference/plate-0[1-6]-results.md`. Nothing was re-run.

**Headline finding.** No physical print of any plinth-generated support structure appears anywhere in the
memory or evidence. Every "field feedback" item after 2026-08-04 is Craig LOOKING at a `.chitubox` in
ChituBox (render review), not a print result. The only real print evidence is (a) calibration coupons
(plates 1, 4, 5, 6 and the H2 plate, all cap-on-arm or unsupported-shelf specimens, no miniature),
and (b) Craig's own hand-support practice in ChituBox. The registry says "Measured" for several numbers
that are Craig's rulings (X_ALLOWED=18) or literature (peel exponents); treat registry labels with care.

Evidence classes. MEAS = measured on a print or coupon. OWN-EXP = Craig's own printing practice or
reasoning from it (no coupon). OWN-PREF = Craig's preference about what good looks like. IMPL = answers a
question the old code created, or came from old-code behaviour. Extra: VENDOR (datasheet), LIT (published
papers, other machines), MODEL (derived or placeholder, nothing printed). Confidence H/M/L is mine.

## 1. Physical claims and numbers

### 1a. Printer, material, process

| claim | value | source | class | conf | note |
|---|---|---|---|---|---|
| Printer and pixels | Saturn 4 Ultra 16K; 15120x6230 px; 211.68x118.37x220 mm; pitch 14 um (X) x 19 um (Y) | cfgx printerinfo; `docs/datasheets/printers/saturn-4-ultra-16k.md` | VENDOR | H | cfgx machine name matches. Anisotropic pixel is a real constraint. |
| Layer, resin, exposure | 0.05 mm; Sunlu Water-Wash ABS-Like Black; 1.2 s normal, 3x15 s bottom (+5 transition) | `plate-01-results.md` s1 | MEAS (setup) | H | One resin. Craig: clear is stiffer/brittle, grey most pliant, black~grey. Constants are per-resin. |
| Rig | heated tank held 30 C, hooded; interface runs 30-40 C (exotherm); PFA film 0.127 mm, rated 60k releases (770 used at plate 5) | memory `y-sensitivity-settled` | OWN-EXP + MODEL | M | Interface temp is an estimate. Film age ruled out as a variable. |
| Peel mechanism | tilt-release, hinge at back (+Y); front (-Y) travels furthest; layers release front to back | memory `saturn4-tilt-peel-geometry` (Craig 2026-07-28) | OWN-EXP | H | Vendor confirms tilt, not axis. Hinge location never measured (tank-edge measurement offered, Craig "not now"). |
| Position effect (front vs back) | <= ~1 rung (~1.5x in pressure); first-moment model predicted 2.1-5.7x and over-predicts | `y-sensitivity-settled`; `plate-04-results.md` | MEAS (non-detection) | M | A bound, not a measurement. Plate 1 D5 rear-station failure is still unexplained (LCD defect vs starvation vs peel). Treat Y as a randomization block. |
| Suction cup on closed faces | "not a huge problem" on a tilt printer | `saturn4-tilt-peel-geometry` (Craig 2026-10-12) | OWN-EXP | M | Closed cavities/flat faces to plate need at most a warning. |
| Cure uniformity and resolution | no front-to-back cure gradient; 0.10 mm walls (7.1 px) printed full height at all 5 Y stations | `plate-01-results.md` s4 | MEAS | H | 0.10 mm confirmed resolvable. Sub-0.10 mm not tested. |
| Gap features | 0.3 mm true gap is preserved in prints; "could probably go lower" | `two-tier-clearance-ruling` (Craig 2026-08-22) | OWN-EXP | M | No coupon; no cure-bleed data exists. Gap-ladder coupon designed, never printed. |
| Elephant's foot | long-exposure base layers swell; surfaces on the plate come out rough; always lift, never rest on plate | `lift-always-elephants-foot` (2026-09-02) | OWN-EXP | H | Lift value: owner ChituBox profile `zliftheightinsupportmode`=5.0; old engine used 3.0-5.0. |
| Raft/boss in ChituBox | raft 0.2 mm; boss 2.2 mm dia x 1.0 mm high; pad area ratio 110% | cfgx all 4 profiles | OWN-PREF (ChituBox defaults he kept) | M | Not tested by coupon. |
| Plate gap in ChituBox | "bases float"; adding a raft in ChituBox joined only SOME plate feet | `plinth-state-2026-09-28`; `sweep-field-feedback-2026-09-29` | OWN-EXP | L | Cause never found (tiny gap vs feet below plate); may be an old-code bug. |

### 1b. Load capacity of a support (the "area law")

| claim | value | source | class | conf | note |
|---|---|---|---|---|---|
| Snap ratio X = A_supported / A_arm_section | X* ~25: cone [25.00, 25.34] (one inversion at 25.00); cylinder bracket (19.91, 30.48), n=1 per edge | `support-arm-load-physics`; `plate-05/06-results.md` | MEAS | M | Plate 5: 72 specimens, 30 cap-on-tip; Craig's naked-eye reads, print date/resin/ambient NOT recorded, row-4 map is inference. Plate 6 confirms on cylinders but one cap area (30 mm2), 4 mm arms. |
| Load scales with area, not perimeter or squeeze film | area law beat line-tension and A^2/d^3 | `plate-05-results.md` s5 | MEAS | M | Plate 5's cap-area spread is the only evidence X is a ratio not a total-area threshold. |
| Failure site | tensile snap at narrowest section just above contact (neck where cylinder meets cone), not the patch | Craig's read, plate 5 | MEAS (owner read) | H | Taper must flare fast. A necked tip becomes the failing section. |
| Tip diameter matters hugely | 25x tip-diameter range decided survival at fixed cap area | `support-arm-load-physics` | MEAS | H | |
| X is phenomenological | slender-wall peel bound 1.24 kPa vs flat-disc 67-101 kPa; X*~25 implies ~2-3 MPa at the neck | `support-arm-load-physics`; `bracing-mechanics.md` s0 | MODEL | L | Never compute a force. The load on a brace/branch layer (between the two anchors) is unmeasured. |
| Sharing | N arms under one patch is at least additive up to N=6 (all 12 strip cells lived at X_arm=60) | `plate-06-results.md` s5; `LoadSharing::additive(6.0)` | MEAS | M | ~20% fin-root bonus; no upper bracket; one cap area, one arm d, one length. Size on the SUM of arm areas. |
| Design ratio | X_ALLOWED 12 (2x margin), raised to 18 by Craig 2026-09-30 | `evidence.rs`; `rulings-2026-09-30-oversupport` | OWN-PREF on MEAS | L | 18/25 = 0.72 of snap. H2 plate: three heads lost at X=9; scored on bottom-step area the boundary is (11.1, 14.4), which brackets 12. The margin is NOT robust. |
| Heads/hair tips | 0.28-0.77 mm2 arms carried 6.9 mm2 heads intact; head-loss boundary X in (25, 36) | `h2-plate07-results` (scored 2026-08-04) | MEAS | L | n=1 cells; "no constant promotion without 3 plates". Bend ladder r=0.30: threshold height 9.7-13.25 mm, three aliased mechanisms. |
| Void family | plate 6 family A6 wiped 0/11 across X=8..409; cap z 10.95-12.7 mm confounded with family | `support-arm-load-physics` | MEAS (invalid) | H | Unidentified in-print mechanism. Coupon rule: never score features above z~9 mm until understood. Free length 4 to 6 mm never cleanly measured. |
| Long slow taper, collinear arms | taper is a liability; 3 collinear arms give no lateral stability | `support-arm-load-physics` (Craig spotted on plate-5 specimen PB) | OWN-EXP | M | X <= 12 is necessary, not sufficient: blind to arrangement. |
| Scarring is area-invariant | total scar area = A_face / X (8.3% of supported face at X=12) whatever the tip size | `support-arm-load-physics` | MODEL | M | Pure algebra. Only X and WHERE scars land move it. Hex lattice ~23% fewer points than square. |
| Weight vs peel | weight negligible unless arm very thin or part very heavy; hollow shells make weight ~ area | `hollowing-aligns-weight-with-area` (Craig 2026-08-29) | OWN-EXP (reasoning) | L | Untested for solid heavy parts and thin arms (the two named exceptions). |

### 1c. Overhang, span, islands

| claim | value | source | class | conf | note |
|---|---|---|---|---|---|
| Flat single-lamina overhang reach | no loss up to 2.130 mm; first loss 2.592 mm (1 of 5); 15/15 failed at >= 4.671 mm | `evidence.rs` L_SURVIVED_MAX; plates 1 and 4 | MEAS | M | Cantilevers on a rigid spine (conservative as covering radius). Failure mode (peel vs tear) unmeasured; ascending towers fail 1.31-1.53x earlier, cause unknown; no usable L50; reader effect +-0.8 mm. |
| Corner sag | longest shelves sag slightly at corners at every station | plate 1 reads (2026-07-29) | MEAS (visual) | M | Never quantified; no positive control. |
| Max unsupported span | ~3 mm at 0.300 mm section | `support-arm-load-physics` (plate 5 rows 1 and 5) | MEAS (low-res photo) | L | "Treat as approximate." No per-bridge scores were recorded. |
| Overhang angle threshold | 45 deg logistic, 30-55 deg | `overhang-limit-estimate-2026-09-02`; `OVERHANG_WEIGHT_TRANSITION_DEG` | LIT + convention | L | NO coupon ever varied angle (plate 7 pre-registered, unprinted). 45 equals ChituBox `autosupportangle`=45 and `supporthiddenangle`=45 in the cfgx. Arm-snap ratio does not transfer to strips. |
| 45 deg cliff flipped discs | engine tilted Round 25/50 to 44.9 deg to dodge a step function | `round-base-diagnosis-2026-09-02` | IMPL | H | Artifact of a hard step. Lesson only: printability is continuous in angle. |
| Tiny islands | an island that joins supported structure within ~3 layers (~0.15 mm) while <= 25 px across needs no support | `rulings-2026-10-20-islands-knees` | OWN-EXP | M | Named settings, unverified by coupon. Islands carry NO load for later layers. Loose 1-px flecks: no support. |
| Island birth | contact must catch an island within ~2 layers (~0.1 mm); "unsupported for 20 layers is a lump floating in the tank" | `resin-v4-phase4-decisions` (2026-07-22) | OWN-EXP | M | |
| Sub-resolution features | drop sub-pixel / sub-layer pieces; list them in a warning | `pause-2026-10-20-dnd-night` | OWN-PREF | M | Witness: thin flap on a treant foot. |
| Treant crown self-supports | Craig printed the treant ~vertical; branches self-support; plinth wanted ~1620 crown tips | `rulings-2026-10-16-hollow-first-tip-cover` | OWN-EXP | M | Disagreement between his hand practice and the engine, never resolved by a print. |
| Slenderness S = L/r | 50 to 30 to 40; derived from drag order-of-magnitude (bracket 40-70); ChituBox ties columns at L/r~13 | `SLENDERNESS_LIMIT_RATIO` | MODEL (placeholder) | L | NO coupon. Craig: unbraced long thin posts "will just not print" (Bat_Cat_Owl 10-09, treant 10-21), a prediction not a test. Slenderness coupon designed, never built. |
| Brace behaviour | braces carry their own peel while growing; shake can matter before snap; brace angle 57.3 deg | `bracing-mechanics.md` | MODEL | L | Brace peel pressure unknown over a ~50x range. 57.3 deg is an old-code overhang-law artifact (IMPL). |
| Bracing practice | one-direction crosses are fine; diagonals not "every direction" | `rulings-2026-10-01-orientation-bracing` | OWN-EXP | M | |

### 1d. Contacts, tips, clearance, removal

| claim | value | source | class | conf | note |
|---|---|---|---|---|---|
| Min tip | 0.1 mm DIAMETER (0.05 radius) works if the thin run is short | `tip-size-field-data` (2026-08-19) | OWN-EXP | M | Below 0.1 mm uncalibrated. UNITS TRAP: ChituBox and Craig speak diameter, old code radius. |
| Max contact on model | 0.8 mm diameter; hard limit, incl. feet | `tip-size-field-data`, `limit-means-hard-limit-in-file`, `contact-cap-includes-feet` | OWN-EXP -> OWN-PREF | M | Origin: his heaviest ChituBox sphere (Heavy profile touch tip 0.8, up 0.6, down 1.5 in cfgx). As a CAP it is cleanup preference. On 2026-10-01 he called a 1.6 mm Sphere "great"; reversed 10-22. |
| Merged contact patches | any merge of two supports' marks is a defect, whatever the size | `fused-contacts-always-defect` (2026-10-25) | OWN-PREF | H | Reason: a tool must fit between marks. No minimum gap ruled. |
| Contact depth | touching = as good as buried; area, not depth, governs; 0.1 mm; keep area by pushing deeper then truncate at the far face | `contact-area-over-seat-depth` (2026-08-30/09-05), `land-at-strike-rulings` | OWN-EXP | M | ChituBox contact depth 0.1-0.3 (cfgx). Open: does ChituBox re-render a clipped ball as whole? |
| Point-contact seat bug | tip ball touched the model at a point | `sweep-field-feedback-2026-09-05` | IMPL | H | Bug in old adapter; the principle (patch >= arm section) stands. |
| Approach angle | cone poking into the model is a placement fault; pick an angled head that avoids the model, else minimise overlap | `tip-approach-angle-ruling-2026-10-28` | OWN-PREF | H | "Straight head drop is almost always wrong." Partly IMPL (the TR metric came from old code). |
| Steep-face contacts | grazing contacts push on the wrong side of the peel; re-land, do not thicken | `steep-face-contacts-reland-2026-10-23` | OWN-EXP (reasoning) | M | Not measured. Consistent with the physics of peel direction. |
| Head shape | ball -> short bare cylindrical neck (snip point, ~0.3-0.6 mm) -> cone; hairs have no ball; no stepped discs; hull-shaped joints | `rulings-2026-10-01...` (10-06, 10-07), `rulings-2026-10-16...`, v18/v19 feedback | OWN-PREF | M | Neck bending never coupon-tested (101 heads warn at median 40x over the bending law). |
| Clearance | placement margin 0.6 mm; final true gap 0.3 mm | `two-tier-clearance-ruling` | 0.3: OWN-EXP; 0.6: IMPL | M / L | 0.6 had empty provenance; the cfgx has `supportspacingfrommodel`=0.6 in all 4 profiles, likely its true origin. Near its own contact the model is judged by "departure" (arm must only stop getting closer). |
| Tool access | thin forest pries off with a pick; thick posts need cutters; remove outside-in; plate end needs no access; MODEL end needs tool room | `plate-packing-fine-model-end-access` (2026-10-20) | OWN-EXP | H | Plate roots may overlap/fuse. Craig: thinning supports mostly dissolves access problems. |
| Interior supports | inside a hidden cavity they are never removed; scars invisible; cavity ceilings need support from below | `support-quality-rulings-2026-08-31`, `supports-may-seat-on-model` | OWN-PREF | H | Drainage is the one surviving constraint. Valid seat = plate, model surface, or another support. |
| Crevice contact | arm touching the far wall in a narrow crevice is acceptable ("better than no support") | `crevice-contact-acceptable-ruling` | OWN-PREF | M | |
| Spacing in ChituBox | tip distance 4 mm (Ultralight/Light 6), 8 mm (Middle/Heavy); cross diameter 0.3-0.8 | cfgx | OWN-PREF | M | Old engine ran 1-3 mm tip spacing ("possibly over-supported", median NN 1.03 mm at one point). Reach from his 4 mm hex practice = 2.31 mm; his 47-tip Cylinder print = 2.41 mm (registry). |

### 1e. Hollowing and drains

| claim | value | source | class | conf | note |
|---|---|---|---|---|---|
| What to hollow | Large-size and up; ~30 mm medium minis are correctly left solid; a Large mini shipped solid is a defect | `hollowing-is-for-large-minis` (2026-08-13) | OWN-EXP | H | Named: Giant Toad, Weeping Treant, Warhorse. |
| Wall | min 2.0 mm, target 2.5 mm ("matching ChituBox") | `HOLLOW_WALL_MIN_MM` doc | OWN-EXP | M | No coupon; the "~1.5 mm necks carried by struts" idea is explicitly provisional. |
| Drain/vent | drain bore 2.5 mm dia (radius 1.25), "proven in manual prints at 2.0-2.5 mm"; every cavity needs a downward drain AND an air inlet; chains of cavities allowed | `HOLLOW_DRAIN_R_MM` doc; `physics.md` s10 | OWN-EXP | M | One of the few items from real hand prints. Treant used to hollow through both legs with drains in the feet. |
| Drain keep-out | only the bore cylinder under the hole is forbidden; thin supports/hair tips may touch the rim; Cylinder drain "boxed in" is fine | `drain-rim-ruling-2026-10-22`, `r5-field-feedback` | OWN-PREF | M | Sphere test (2026-09-03): supports completely covered the drain; Craig did not notice it was hollow. Invariant he wants: outlet connected to the outside through a passage >= drain width. |
| Hollowing and peel | peel falls once the slice reaches the hollow (walls only), if the void drains; one lit datum: 2.68x less area gave 1.26x less force | `hollowing-aligns-weight-with-area` | OWN-EXP + LIT | L | Punched holes are the worst drainage case. Discriminant is drainage speed. Never measured on this printer. |
| Peel scaling in literature | exponent 0.24-0.46; ref pressure 9-19.6 kPa at 314 mm2 | `evidence.rs` ORIENT_PEEL_* | LIT | L | Different resin, film, machine (one on PDMS-glass). Labelled "Measured" in the registry; it is not. |

## 2. Field feedback from Craig (what failed, what looked bad, what worked)

### 2a. Real prints (coupons and hand practice)

- 2026-07-22: Weeping Treant is "print #1" of a queue; Craig deleted his ChituBox-supported treant himself. The earlier engine hollowed it through both legs (2026-08-13 memory).
- 2026-07-29, plate 1 (unsupported masts, 17 A + 5 D + 5 E): no finger lost anywhere in family A; longest-shelf corners sag slightly; E walls full height down to 0.10 mm; glossy patches = wash residue, not defect; D5 at the rear (+44 mm, hinge) "struggled", cause unknown; row markers a waste (8.63 mL).
- 2026-07-29, plate 4 (25 towers, 100 shelves): unsupported length 0 failures low, 15/15 failed at >= 4.671 mm; X row flat across 135 mm (no dead LCD zone); ascending towers fail earlier than descending; failures confined to lower laminae ("peeled").
- ~2026-07-30, plate 5 (cap on support arms): "pretty much every mushroom cap got plucked from its support, except the really small caps"; row 2: 3 of 15 survived, row 4: 5 of 15; large caps snapped "at the base of the cylindrical portion where it meets the conical support"; the survivor had the thickest support; rows 1 and 5 (bridges) readable, row 3 "a mess".
- 2026-07-31, plate 6: sharing works (all 12 strip cells survive at X_arm 60 where a matched mono dies); cylinder boundary 19.9-30.5; family A6 wiped out (17 lost caps), cause unknown; wash basket empty so loss was in-print.
- 2026-08-04, H2 plate (heads): see 1b; three heads lost inside the design ratio; all four CM controls (r=0.25, X=36) died as predicted; loading fixture had three geometry defects.
- 2026-08-19: his working ChituBox practice: 0.1 mm dia cylindrical tips on tiny islands; heaviest tip 0.8 mm dia sphere.
- 2026-08-22: 0.3 mm gap features "preserved fine in prints".
- 2026-09-02: rejected resting a base on the plate (elephant's foot, rough edges).
- 2026-10-01 to 10-22: printed the treant ~vertical (branches self-support; drains under feet hidden by base). Predicted (not tested) that long thin unbraced posts will not survive.
- No print of a plinth-generated file is recorded. 2026-08-30: of 37 emitted files "approx. 0 would actually print"; 2026-10-20: "this doesn't seem printable"; 2026-10-29 "unable to produce a working product after months".

### 2b. ChituBox review of generated supports (screen only, not prints)

- 2026-08-31: five "known-good" fixtures heavily over-supported (0.977 vs 0.651 mm needed), lumps at tips; prefers short model-anchored arms to plate descents.
- 2026-09-03: Round 50/100 "look good", maybe dense; Round 25 NOT upside down (stiffener ring is on the underside); Morel base flipped wrong way (textured ground took the supports); Cylinder/Sphere: fat overlapping arms at the bottom, supports covering the drain; toe supports "ridiculously overly thick"; all supports straight down, no merging.
- 2026-09-05: tip is a point contact; merges landed only on top of another arm (he wants trees: a base rises, splits, heads to several tips); drain keep-out "dramatically eliminated the clustering".
- 2026-09-28: Round 25 good; defects: "porcupine quills" (arms seated on the model connected to nothing), trunk thinner than branch at joints, double X braces, brace to nowhere, Cylinder underside dense; Cylinder drain clear.
- 2026-09-29: quills gone; Cylinder bottom still "a shit-show" of overlapping melted roots; braces slicing through the model (stilt plate); skinny cross-braces on the Round rim; later "pretty good", Brace pair "very good"; 10-01 Cylinder "likely printable now".
- 2026-10-02: Cylinder fat overlapping posts at the drain, fat lower post stepping abruptly to a thin tip, pairwise-braced clusters look like overkill; lonely peak thin posts doubtful.
- 2026-10-07/08: angled heads "great"; objections: joint ball far too big, stepped disc tips, rings poking from joint balls, staircase junctions; v21 "looks very good", Cylinder "posts appropriately sized". 0.4 mm contact cap by adding same-level contacts: "a shit-show of overlapping contacts", +22-28% resin, rejected.
- 2026-10-09 Bat_Cat_Owl "a mess": long thin unbraced posts, huge fat low columns. 2026-10-10 Deinonychus: thin unbraced rods 20-30 deg, a long 45 deg support for no reason. 2026-10-11/12: "All three look pretty good", "looking excellent"; model-end feet were plain cylinders, ball cut flat outside the model, tiny ball on a fat post.
- 2026-10-14 (r5): wing posts "clearly not right" (wants two cones meeting at a ball joint); ball diameter equal to shaft diameter (worse removal); tiny ball on fat post. By 10-14 "it all looks pretty good" (tag `seems-printable` set on that look; 10-20 he said it did not seem printable after all).
- 2026-10-20 (r8): Deino 2 offset-foot leaning diagonals, Morel "a bunch" of diagonals on a flat underside he knew had been good; treant: 714 arms over the slenderness limit, crazy 45 deg diagonals across wide swathes, "will never possibly succeed".
- 2026-10-21/22/23/25/28/29: diagonal drop that should be vertical (Deino throat feather); thin red rod should brace to adjacent columns; 2.19 mm contacts after a "0.8 mm cap" (angry); fused contacts always a defect; tip cone through the model is a placement fault; "tip inside another support, but no fusing?".

## 3. Owner goals and preferences about the PRODUCT

- What he prints: D&D minis, single-colour resin, many per plate. Queue (2026-07-22): 4x Weeping Treant (Grove Guardians), 4 plates of skeletons/zombies, wild-shape animals, familiars, his PC mini. Large minis (treant, Giant Toad, Warhorse, Giant Bat) are the hollowing prize; skeletons and thin weapons are the hard support regime.
- Workflow wanted: point at a folder of STLs; get supported, hollowed, plate-packed files; eventually auto-slice and send to the printer (`plinth prepare <folder> --output <printer>`, flag spelling is pseudo-code). Staged: files per model, then packed plates, then own slicer (variable layer height is plate-global), then network send (his `elegoo-cli`).
- Output format: `.chitubox` is the permanent review checkpoint; model and supports must stay separable in the file (STL cannot; 3MF can). ChituBox drag-and-drop merges projects, so plate packing is a convenience, not a blocker.
- Success bar (2026-08-30): "does editing the generated supports beat deleting them and rebuilding in ChituBox?" Judge every step by "would Craig print this file?" Count print-candidates, never emitted files.
- Emit and warn, never refuse: partial supports plus a located warning beat nothing; the file is evidence. Warnings must be actionable (where, how big, how many rejected).
- Pose: he decides for flat-face questions; no flat-face heuristic; `--orient none`, a Finder tag `plinth-no-rotate`, or a marker file mean "as drawn". Always lift off the plate. Hollow before rotation (weight right), drill drains per pose.
- What good looks like: thin supports sized by physics, short model-anchored arms, tree-shaped merging (base splits into branches), hair tips with no ball, angled short necks, ball joints (two cones meeting at a ball) where members meet at an angle, nothing poking through the model, minimal visible scarring on display faces, room for a tool between marks, forests of thin supports on organic models.
- Hard limits are construction rules, not late repairs: the 0.8 mm contact cap, no fusing, 0.3 mm gap, drain bore must exist from the start. When the cap is reached, ADD more non-touching supports. Resin and run time are reported, not gated ("no limit", "physics wins, report time") but waste is still a defect.
- Physics over magic numbers: every accept/reject traces to the physics or to a labelled owner requirement; constants only prune search. Gentle on false precision: bracket, do not midpoint.
- Quality bar from his own eye: Round 25/50/100 discs must work first ("simplest possible support job"); Giant_Bat and treant are witnesses he cares about; he cannot read X/Y in ChituBox, so point with pictures (ChituBox-style supports, before/after side by side, caption on the image) or body parts.
- Speed: <60 s per complex mini, a few seconds for small ones; correctness is the acceptance bar, speed is a debug instrument. Laptop 128 GB, memory ceiling ~100 GB. Python only as debt (decision 2026-07-28: restart core in Rust).
- Process: one verification at the end, no per-pass full gates; do not chase small prizes (FU-82 closed); small bounded lanes, no month-long ratchets.

## 4. Open physical unknowns worth testing (cheapest and most decisive first)

1. Print an actual plinth-generated miniature (Round 25 and a small hollowed Large mini) next to a hand-ChituBox version. Nothing here has ever been tried on a real support forest.
2. Snap ratio X* on a second resin/cap area, with replicate rows (n>=3), and a set at X_ALLOWED=18 with heads vs shafts. The H2 anomaly (heads lost at X=9, n=1) decides whether 18 is safe.
3. Neck/link bending: print contact ball + 0.3-0.6 mm link + cone heads at lean angles 0/30/60. 101 heads were warned 40x over the bending law.
4. Overhang angle coupon at fixed reach (0/30/45/60/70 deg) on this printer; tests whether 45 deg and the logistic are real, plus the continuous reach ramp.
5. Slenderness coupon: one arm per (length, radius), plus braced vs unbraced, to replace S=40 (L/r 13 in his ChituBox practice, 40 in the code). Include z < 9 mm to avoid the A6 confound.
6. Brace/branch growth: diving-board cantilevers (F1-F4 designed, never printed); also record failure mode (peel vs tear).
7. Tilt direction: strip at one Y spread across X (the cheap test for the D5 rear failure) and a tank-edge height measurement to place the hinge.
8. Gap ladder 0.30/0.25/0.20/0.15/0.10 mm x orientation x plate position; Craig thinks 0.3 can go lower.
9. Tiny-island rule: how big and how many layers before an unsupported island really fails, across 15-30 layers; check the "joins in 3 layers" claim.
10. Hollow peel credit: U-shape vs ring vs drained cavity; vary void width and drain speed; confirm the walls-only peel reduction.
11. Treant crown: does an upright crown with far fewer tips print? Compare with the engine's 1620-tip plan.
12. Contact area vs depth and truncation: print three tip seatings (0.1 mm deep, deeper, clipped ball) and check scar area and adhesion; test whether ChituBox renders a clipped ball whole.
13. Supports under the plate gap: raft/pad behaviour in ChituBox (why only some feet join); elephant's foot at 1-3 raft layers.
14. Drain size ladder 1.5/2.0/2.5/3.0 mm bore on real hollow bodies; verify the 2.0-2.5 "manual prints" claim at a few resin viscosities.
15. Scar visibility/removal: remove supports at 0.3, 0.5, 0.8, 1.2 mm contacts and judge cleanup; ties to the 0.8 mm cap preference.

## Ten-line summary

1. Only coupon plates (1, 4, 5, 6, H2) and Craig's hand ChituBox practice are real print evidence. No plinth-generated mini has ever been printed.
2. Strongest numbers: snap ratio X*~25 (bracket 19.9-30.5), shelf survival to 2.13 mm and failure by 4.67 mm, 0.1 mm walls resolve, additive sharing to N=6, neck-of-cone failure site.
3. Weakest "measured" labels: X_ALLOWED=18 (his call, H2 had heads lost at X=9), peel exponents (literature), 45 deg overhang (convention), S=40 (derived), brace load (model only).
4. Owner field practice (not coupons): 0.1 mm min tip, 0.8 mm max sphere, 0.3 mm gaps, 2.0/2.5 wall, 2.5 mm drain, tool-access reasoning, elephant's foot, tiny islands self-adhere, upright treant crown self-supports.
5. Old-code artifacts to drop: 45 deg step flip, point-contact seats, 57.3 deg brace angle, 0.6 mm standoff provenance, congestion leans, moved_onto_face rods, TR/FU metrics, R2 brace cap.
6. Preferences that stay: hard limits built in, fused contacts always bad, angled heads, hair tips, tree merging, emit-and-warn, always lift, Large-only hollowing, pictures not coordinates.
7. Everything after 2026-08 labelled field feedback is screen review in ChituBox, driven by what looks wrong, not by failures.
8. Biggest skeptic flags: ChituBox profiles use 4-8 mm tip spacing while plinth ran 1-3 mm; registry "Measured" tags on literature and rulings; plates have one resin, partial scoring, missing print dates.
9. Cheapest decisive tests: print real minis vs hand ChituBox; X=9..25 replicate ladder with heads; angle coupon; slenderness coupon; link-bending coupon.
10. Skeptical reading: the physics is thin (one area law, one reach), the preferences are rich; the PRD should store the preferences as requirements and the physics as bracketed, replaceable parameters.
