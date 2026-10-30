# Bracing mechanics: what loads a post and a brace, and a rule set (2026-10-02, revision 2)

This is a design study only. I changed no production code and made no commit. I did not need a worktree: every number
comes from the v11 files, the v11 diagnosis probe dumps, and small frame models that I wrote for this study.
Tools and data: `bracing-mechanics/` (`tools/run.sh` reproduces every table).

**Revision 2** answers a cross-family challenge (`bracing-mechanics-challenge.md`, recovered from the workflow journal
because the challenger wrote no file). The challenge was right on most points. Section 11 answers each point. The main
changes are:

- the brace peel pressure is now unknown over a range, not bounded at 1.24 kPa;
- the shaking check now uses a construction-stage frame of the real v11 geometry;
- the layouts are checked with the model seated as well as before it seats;
- band counts come from a frame where every band can sway;
- the predictions are a feasible remedy, not upper bounds.

The study is an exploratory design study. It is not an implementation-ready rule set until the coupon reads.

## Summary (10 lines)

1. **Braces carry load: their own peel while they grow.** A diagonal grows off its lower post like a diving board until it meets its twin. Each new layer's peel pulls on its tip. That is a torque on the post, and it bends everything below the root that the frame does not hold.
2. **The torque grows with the cube of the gap** when r_b = ℓ/S. The stilt plate's 17 mm brace puts about 250 times the torque of a typical Cylinder brace on its post.
3. **How hard a brace layer peels is NOT known.** 1.24 kPa is the measured value for a 0.05 mm wall. Brace layers (0.015-1.07 mm²) sit between that and the 67-101 kPa of flat discs. A joined brace root snaps only if p is above about 39-81 kPa. That is unlikely, because ChituBox crosses at a similar root load are part of your working practice, but it is not proven.
4. **Shaking governs long before snapping.** In a construction-stage frame of the real v11 files, 11 printing tops move more than 1 px at p = 1.24 kPa (worst 3.8 px, brace pair). On 6 of 7 models the worst top reaches 1 px at p = 0.33-1.8 kPa. So a small rise in brace peel above the wall value would make brace shake a real problem. Coupon F2 decides this.
5. **Rule R2, a design choice: a brace is never thicker than the posts it joins.** It stops the "fat post, thin top" steps and caps a brace's reach at S times the thinner post's radius. It is a cheap guard, not a proven strength rule.
6. **Directions: per post before the model seats, per frame after.** Before seating, each post needs two brace planes far enough apart. The angle needed comes from the run length; it is not a fixed window. After seating, the model ties the tops: the whole frame needs at least 3 brace planes that are not all parallel and do not all meet at one point.
7. **Your a-b / c-d square:** the outer four are best in every state (13-24 % of unbraced sway, torsion 43 %). The middle X is close after seating (27 %), but 35 % in its weak direction before seating. It gives almost no torsion help (92 %) and costs 40 % more resin. All six add 140 % resin for at most 10 % gain.
8. **Bands are not fixed points.** A two-post pair needs 2 bands at 2 times its limit, 3 at 2.5, and 5-6 at 3 (the engine places 1, 2 and 2). Above about 3-3.5 times, no number of bands between two posts works. So the band rule needs a frame model, and it adds resin: v11 was under-braced by its own law.
9. **Thicken or brace: brace in a window.** Bracing is cheaper from about 1.5 times the limit (a cross both posts need) or about 2.5 times (a cross only one post needs) up to the two-post ceiling of about 3.3. Thicken below that window, past the ceiling (or brace to a wider frame), and where there is no partner in the right direction.
10. **Coupon:** F1 diving boards (record the failure mode), F2 brace on a thin post (does brace shake mark the print?), F3 square layouts at three heights with unbraced controls, F4 single, unjoined pair and joined X, and G the stilt plate with and without braces. Nine decisions for you are in section 9.

---

## 0. Assumptions, units and sources

- **Frame.** z goes up from the plate. Layers grow in +z. Peel pulls the newest layer toward +z, so the members below it are in tension.
  Film release on the Saturn 4 travels front to back, along Y (memory: tilt-peel geometry). File Y is ChituBox Y and the printer's Y.
- **Peel pressure is a range, not a number** (`plinth-3d/docs/plans/load-model-physics-first.md` §(a)):
  - a 0.05 mm slender wall: **p ≤ 1.24 kPa** (plate 1, a saturated bound for that wall);
  - flat discs 5-12.5 mm across: about **67-101 kPa** (plate-6 memo). The area law X* ≈ 25 was measured on flat patches of 3.14-122.7 mm²;
  - a brace layer is an ellipse of 0.015-1.07 mm² (r_b 0.05-0.43 mm at 57°). That is **between the two anchors, and nobody has measured it**. In this study, p_b is a parameter. I quote results at p_ref = 1.24 kPa and give, for each result, the p_b at which it crosses its limit.
  - Green strength from the same sources: **σ_g ≈ 4.8-10 MPa** (an inference from X* ≈ 25, not a tensile test; plate-6 protocol).
- **Stiffness is not measured.** The S-law note derives S from drag q ≈ 5 mN/m and a 1-pixel limit (px = 0.019 mm). S = 40 is the 1-pixel ratio only if **E = q S⁴ / (2π px) = 0.107 GPa**. Both q and S are placeholders, so this E is a consistent unit, not a measured minimum. Every shake figure in pixels is proportional to p_b/E = (p_b/q)·2π·px/S⁴, so the real unknown is the ratio of brace peel to drag. Coupon F2 measures the effect directly.
- **Area-law units.** "X" is stress divided by the peel pressure of the section in question. X = 18 is the design limit and X ≈ 25 the snap line for flat-patch peel.
- **Measured brace angle.** Every v11 diagonal stands at **57.3° from vertical** (p10-p90 56.6-57.4°), not 45°. That is the overhang law's saturated angle. Joined diagonals meet at 0.50 of the gap (p90 0.505, max 0.56), so a lever of about d/2 is what v11 builds. The tools use each diagonal's measured meeting point.
- **Constants.** The only numbers used are already in the registry or the evidence: S = 40, X_ALLOWED = 18, X* ≈ 25, r_min 0.05 mm, r_max 1.0 mm, θ_sat, px, q and the two peel anchors. Wherever a magnitude is not known, it is marked **[coupon]**.

---

## 1. Loads on a post while it prints, and how a column fails

### Loads

| load | when | direction | size | what it threatens |
|---|---|---|---|---|
| peel of the post's own newest layer | every layer of the post | along the post (pull) | p·πr² | nothing |
| tilt shear at the printing front | at each release; the front (-Y) posts first | sideways, **mostly ±Y** | μ·p·πr², μ unknown **[coupon]** | sway. A force at the tip gives L_max ∝ r^(2/3), not ∝ r |
| resin drag (squeeze flow) | when the plate moves | sideways | q ≈ 5 mN/m (registry estimate) | sway: the S law |
| **peel of a brace or branch growing off the post** | while that member grows, before it closes | **a torque on the post at the root** | F = p_b·πr_b²·secφ at lever x. Median Cylinder brace at p_ref: 0.035 mN at 1.1 mm. Stilt 17 mm brace: 1.3 mN at 7.2 mm | the post shakes while it prints; bending at the joint |
| peel of the model face (suction) | every model layer, after the seat prints | along the post (pull) | up to about 0.1 MPa × the face it carries (X ≤ 18) | snapping at the narrowest section (X* ≈ 25) |
| squeeze as the plate comes down for a model layer | every model layer | along the post (**push**) | fraction c of the pull, unknown **[coupon]** | buckling of tall posts that carry a face |
| tilt shear on the model's own layer | every model layer | sideways on all seated tops at once, and twisting (the front reaches some tops first) | μ × model-layer peel | the whole forest sways and twists under the model |

Points from this table:

- **The S law sees only drag.** A brace's peel torque acts at the same time as drag (plate motion and release), so the two must share one 1-pixel budget. At p_ref, the median brace's torque at the joint is 2-7 times the drag torque the S law budgets for that post on the stilt plate, the brace pair, the Cylinder, the laid tube and the overhang table.
- **Pull and torque at one section.** On a straight post, its own seat is above all its brace roots, so the model's pull comes after the brace torque. On a shared trunk the two can coexist: a branch seated low on a trunk pulls the trunk while a higher brace on the same trunk still grows. I checked the v11 files for this: **no diagonal's chain carries a seat below its root** (`data/pi2.out`). The rule set still checks the sum (R9), because nothing prevents it in principle.
- **Once the seat prints, the model holds every seated top.** The forest then works as a frame under the model's own loads, which includes twisting. The S law does not model this (section 3).

### How a column fails

| mode | formula | what decides it | evidence |
|---|---|---|---|
| sway (layers land off-centre: wobble lines) | drag: δ = qL⁴/(8EI), so L ≤ S·r. Tip force H: δ = HL³/(3EI) | the 1-pixel limit, if the deflection does not spring back before exposure | S has never been measured **[coupon A]** |
| set or creep (a lasting lean from repeated bending) | grows with the transient δ | how fully green resin recovers between layers | none **[coupon]** |
| buckling under the squeeze | P_cr = π²EI/(KL)², K set by the end restraints (below) | approach squeeze against P_cr | none **[coupon]** |
| snapping | σ = X·p ≤ σ_g at the narrowest section | the area law | X* ≈ 25, measured on flat patches |
| fracture at a brace joint | σ = 4M/(πr³) at the brace root or the post section | brace peel torque | section 2; depends on p_b |

**Buckling is conditional on restraints that I have not shown exist.** For an r 0.4 mm, 35 mm post at E 0.107 GPa (EI = 2.15 N·mm²):

- **K = 2** (free top, or a top that can sway): P_cr = 4.3 mN.
- **Sweep tips are point contacts** (field feedback 2026-09-05). A model therefore holds a seated top against moving but hardly against rotating. So a post under a model that can sway as a whole is still close to **K = 2**, not K = 1 as revision 1 said.
- **K ≈ 0.7** applies to a segment between two points that a stiff frame holds against moving (pinned ends). **K = 0.5** needs both ends held against moving and rotating. A brace band that sways with its posts gives neither.
- At X = 18 the post may carry about 9 mm² of face, so up to about 0.9 N of pull. If the squeeze on the way down is more than about 0.5 % of the pull, a post that can sway bows. That is a candidate mechanism, not a finding.
- The frame-buckling modes of a real braced cluster are not computed here. Bracing helps only to the extent that it stops sway at the band points. I make no general claim of a "4-16 times" gain.
- A bow at mid-height with a level pad is consistent with buckling, but lateral load, crookedness and creep can make the same shape. The coupon reads buckling only by comparison with controls (section 8).

---

## 2. Braces as loaded members while they print

### The mechanics

Diagonal 1 starts on post A at the band bottom. It grows up and across toward post B. Diagonal 2 does the same from B. Until they meet, each one is a cantilever from its root.

- Each layer of a diagonal is an ellipse of area **A = πr_b²·secφ** (φ from vertical). Its peel F = p_b·A acts at the horizontal distance x from the root.
- Torque at the root: **M = p_b·A·x**. Below the root, M bends the post and everything under it that the frame does not hold.
- **Joining.** Two diagonals that fuse at a fraction s of the gap leave a lever of max(s, 1-s)·d. v11 builds s = 0.50 (max 0.56). A single diagonal, or a pair that does not fuse, keeps the full d.
- In area-law units: brace root X_root = 4·secφ·x / r_b; post below the joint X_post = X_root·(r_b/r_p)³.
- With r_b = ℓ/S and x = d/2: **X_root = 2·S·tanφ**, the same at every length: 124 at 57.3°, 80 at 45°.
- Torque with r_b = ℓ/S: **M = (π p_b d³ / 2S²)·1/(sin²φ·cosφ)**, the **cube of the gap**.
- **Shake of a printing top.** For a moment M applied at height h on a straight cantilever chain, the point at height H above it moves **δ = M·h·(H − h/2)/(EI)**. That is more than the joint's own M·h²/(2EI). In a real cluster the lower bands, partners and trunks share the load, so I solve the frame (below).

### Snapping: conditional on p_b

| brace | X_root (area units) | real root stress | snaps if p_b above (σ_g 4.8-10 MPa) |
|---|---|---|---|
| joined, r_b = ℓ/S, 57.3° | 124 | 124·p_b | **39-81 kPa** |
| not joined (v11 max) | 274 | 274·p_b | **18-36 kPa** |
| ChituBox cross, joined, 45°, d 9-30 mm, ⌀ 0.5-0.8 mm | 2.83·d/r_b = 64-212 | | 23-156 kPa |

- At p_ref the joined root sees 0.15 MPa, 30-65 times below σ_g. At the flat-disc pressure (67-101 kPa) it would snap. **So "no snapping hazard" is not established.** It holds only if brace layers peel well below flat-disc pressure.
- **Evidence that they do, but comparative only:** you print ChituBox crosses (`chitubox-support-specs.cfgx`: crosses on, ⌀ 0.3-0.8 mm, spacing up to 15-30 mm). They carry the same root load in these units, and you have not reported cross roots snapping. The engine's leaning branches are in the same range: X_root p50 109 on the Sphere and 88 on the Cylinder, up to 210 (`data/crossset.out`). This says engine braces are no worse than crosses that work in practice. It does not measure p_b.

### Shaking: a construction-stage frame of the real v11 files

`tools/stage.ts` builds each v11 cluster as a 3-D beam frame from `members.jsonl`, `bdump2.jsonl` and `braceends.jsonl`. It does this at the last layer before each cross closes, where the lever is largest. In that frame:

- members are cut at that layer, with radii from the drawn profile;
- finished crosses are frame members, so every lower band is compliant and nothing is a clamp;
- every diagonal still growing is a cantilever with its newest layer's peel on its tip, all at once and each in its own direction;
- branches stand on their trunks; members with no trunk start on the model and are fixed there;
- post tops are free (model seats ignored), which is softer than the real state.

The tool reproduces a closed-form cantilever exactly (8.75 px against 8.75 px, synthetic check). The output is the largest sideways move of any printing top:

| model | diagonals (not joined, strict test) | worst printing-top move at p_ref | tops over 1 px | p_b at which the worst top reaches 1 px |
|---|---|---|---|---|
| Cylinder | 324 (26) | 2.5 px | 3 | 0.50 kPa |
| Sphere | 82 (8) | 1.4 px | 3 | 0.91 kPa |
| stilt plate | 54 (0) | 1.8 px | 1 | 0.70 kPa |
| brace pair | 18 (2) | **3.8 px** | 2 | **0.33 kPa** |
| laid tube | 202 (16) | 2.0 px | 2 | 0.62 kPa |
| overhang table | 24 (0) | 0.7 px | 0 | 1.8 kPa |
| Morel_base | 8 (0) | 0.04 px | 0 | 33 kPa |

(`data/stage.out`. "Strict test" means the partner's centreline passes inside the brace body; see R1.)

How to read this:

- **The brace-shake verdict depends on p_b.** At the wall value, 11 printing tops over the whole set move more than 1 px. If brace layers peel even 2-4 times harder than a 0.05 mm wall, which is likely for sections 3-17 times wider, most models exceed 1 px. Revision 1's "small on v11" held only at p_ref, and I withdraw it as a general claim.
- **Two cheaper estimates bracket this one.** The joint-only model of revision 1 (the next lower band taken as a clamp) gave 8 braces over 1 px, worst 6.9 px. A chain with no band credit at all (`tools/pi2.ts`) gives 94 tops over 1 px, worst 99 px. That second estimate is too soft to use, but it shows that the lower bands carry most of the brace torque. So the band layout below a long brace matters as much as the brace.
- **Thick long braces on thin posts are still the worst cases:** the brace pair's 9.5 mm unjoined diagonals and the stilt plate's 7-17 mm braces lead their models.
- **Overhang of a 57.3° brace.** Each layer hangs h·tanφ = 0.078 mm past the one below (0.05 mm at 45°). The brace's own tip deflection as a cantilever is δ_tip = (p_b/E)·tanφ·S²·ℓ/6 for r_b = ℓ/S, which is about 4 px for a 17 mm brace at p_ref. Whether a brace curls at its tip is not measured **[coupon F1]**.

### What should limit brace length and thickness

1. **A brace is never thicker than the members it joins (R2, a design choice).** It makes the post section at least as strong as the brace root for the brace's own torque, and caps reach at ℓ ≤ S·min(r_A, r_B). It does not bound the sum of several braces' moments, sway, the capacity of the chain below, the joint's strength, or forces in the finished frame. Those are checked by R9. Ratio 1 is the simplest choice and is not forced by statics.
2. **Joined crosses (R1)** keep the lever near d/2. A pair that does not fuse doubles both lever and torque.
3. **Root strength** limits only if p_b approaches 18-81 kPa (above). F1 measures it.
4. **Shaking (R9)** is the binding limit. Use a construction-stage frame check with p_b as a stated parameter, and warn. F2 sets p_b.

---

## 3. Frame layout: 2, 3, 4 and n posts

### Model

`tools/frame.ts` is a 3-D frame solver: Euler-Bernoulli beams with 6 degrees of freedom per node. Posts are fixed at the boss top. Each cross is two diagonals that meet at mid-gap. Band depth follows the overhang law at φ, with r_b = max(r_min, ℓ/S). `tools/layouts2.ts` evaluates each layout in **two construction states**:

- **Before the model seats:** post tops are free. This is the state in which the supports themselves print.
- **After the model seats:** the tops are tied by a stiff cap (the model). Loads: a shared push in every direction; one force at the front corner alone, because release starts at -Y; and a twist about z.

Values are relative to the same state with no braces: 1.0 means no help, smaller is stiffer. Thin geometry: r 0.15, 9 mm, 2.2 mm apart (Cylinder and Sphere median). The r 0.4, 30 mm, 5 mm case is in `data/layouts2.out` and ranks the same.

| layout (one band) | brace mm³ | before seating, shared: worst / X / Y | before, front post alone, Y | after seating, shared: worst / X / Y | after, front corner alone, Y / X | after, **twist** |
|---|---|---|---|---|---|---|
| 2 along X, 1 cross | 0.07 | 1.00 / 0.13 / 1.00 | 0.71 | 1.00 / 0.21 / 1.00 | 1.00 / 0.21 | 0.98 |
| 3 in a line along X (the brace pair) | 0.14 | 1.00 / 0.16 / 1.00 | 0.68 | 1.00 / 0.21 / 1.00 | 1.00 / 0.21 | 0.98 |
| 3 triangle, path (2 crosses) | 0.14 | 0.63 / 0.50 / 0.63 | 0.57 | 0.53 / 0.36 / 0.50 | 0.63 / 0.27 | 0.85 |
| **3 triangle, cycle (3)** | 0.21 | **0.21** / 0.21 / 0.19 | 0.18 | 0.25 / 0.25 / 0.25 | 0.29 / 0.26 | 0.53 |
| **4 square, outer four** | 0.28 | **0.13** / 0.13 / 0.13 | 0.15 | **0.21** / 0.21 / 0.21 | 0.24 / 0.24 | **0.43** |
| 4 square, **middle X only** (a-d, b-c) | 0.40 | 0.35 / 0.24 / 0.24 | 0.21 | 0.27 / 0.27 / 0.27 | 0.36 / 0.36 | **0.92** |
| 4 square, all six | 0.68 | 0.14 / 0.12 / 0.12 | 0.12 | 0.20 / 0.20 / 0.20 | 0.23 / 0.23 | 0.42 |
| 4 square, Y sides only (a-c, b-d) | 0.14 | 1.00 / 1.00 / 0.13 | 0.15 | 1.00 / 1.00 / 0.21 | 0.26 / 0.95 | 0.61 |
| 4 square, one X side + one Y side (a-b, a-c) | 0.14 | 1.00 / 1.00 / 1.00 | 0.15 | 0.62 / 0.48 / 0.48 | 0.31 / 0.60 | 0.82 |
| 4 square, 3 sides (a-c, c-d, d-b) | 0.21 | 0.61 / 0.61 / 0.13 | 0.15 | 0.44 / 0.44 / 0.21 | 0.26 / 0.30 | 0.57 |
| 4 square, outer four in every band (ChituBox-like lattice, revision 1 `study.out`) | 6-9× one band | 0.007 | | | | |

What this shows:

1. **Before seating, directions are per post.** A cross holds its own plane. Across that plane it helps a post 0-1 % when every post is pushed. A post with crosses in two planes α apart has a worst-direction ratio of about **1 / (1 + (1/ρ − 1)(1 − |cos α|))**, where ρ is the one-plane ratio (`tools/angle.ts`: within 4 % of the frame at 30-150°, and 15 % optimistic at 90°).
2. **After seating, directions are per frame.** The cap shares stiffness between posts: "one X side + one Y side" holds the whole square at 0.48 in both X and Y. The frame then needs what any stiff deck on posts needs: **at least 3 brace planes, not all parallel and not all through one point.** With fewer, the frame can twist freely. The middle X has two planes that both pass through the centre, so it barely resists twist (0.92). The outer four, a triangle cycle, and 3 sides all qualify.
3. **Your square:**
   - **Outer four** is best or close to best in every state and load, at the lowest torque per joint.
   - **Middle X** is nearly as good once the model seats (0.27 against 0.21). But it is 2.7 times worse before seating in its weak direction (0.35 against 0.13), and it does little against twist (0.92). It also costs 40 % more brace resin and puts 2.8 times the torque on each post (d³ with r_b = ℓ/S). Your intuition is partly right: after seating it does work. It is not the better choice.
   - **All six** adds 140 % resin for at most 0.01-0.02.
   - **Y sides only** hold Y in both states for half the resin, and twist at 0.61. X is unheld.
4. **Posts in a line** (the brace pair, stilt rows along X) can never be held across the line by crosses between themselves, before or after seating. That direction needs a post off the line, thickening, or a tie to something stiff.
5. **A lattice** (ChituBox style) is 8-20 times stiffer than one band at 6-9 times the brace resin. It is the right answer only if the coupon shows loads well above the S model.

These rankings come from equal loads and a rigid cap. Model faces are not rigid, and real loads move with the release front. So the rankings are evidence for the rules below, not proof of them. F3 tests them.

### Which angles count? Derived, not chosen

For a run at λ = L/(S·r) with n bands, ρ(λ, n) comes from the two-post frame below. Before seating, the run passes every direction if its two planes are at least **α_min = arccos(1 − (λ⁴ − 1)/(1/ρ − 1))** apart. It passes Y-only if one plane is within **β_max = arccos √((1 − λ⁻⁴)/(1 − ρ))** of Y.

| λ | bands | ρ | α_min (every direction) | β_max off Y (Y only) |
|---|---|---|---|---|
| 1.25 | 1 | 0.094 | 32° | 36° |
| 1.5 | 1 | 0.099 | 56° | 19° |
| 1.5 | 2 | 0.034 | 31° | 24° |
| 1.75 | 1 | 0.102 | 87° | 4° |
| 1.75 | 2 | 0.034 | 45° | 16° |
| 2 | 2 | 0.036 | 64° | 9° |

(`data/angle.out`.) Revision 1's fixed windows (60-120° and ±45° of Y) were too loose for longer runs. A cross 45° off Y halves Y sway at best, so it cannot carry a run much past 1.2 times its limit.

### How many bands? Every band compliant

Two equal posts with n evenly spaced bands (`tools/bands2.ts`). Sway is divided by the 1-pixel limit and passes at ≤ 1; drag / tip force:

| λ | 1 band | 2 | 3 | 4 | 5 | 6 | fewest bands (drag / tip) | engine's ⌈λ⌉−1 |
|---|---|---|---|---|---|---|---|---|
| 1.25 | 0.23 / 0.24 | 0.08 | | | | | 1 / 1 | 1 |
| 1.5 | 0.50 / 0.43 | 0.17 | | | | | 1 / 1 | 1 |
| 1.75 | 0.96 / 0.71 | 0.32 | | | | | 1 / 1 (mid geometry: 2 / 1) | 1 |
| 2 | **1.68** / 1.09 | 0.58 / 0.37 | | | | | **2 / 2** | **1** |
| 2.5 | 4.27 / 2.19 | **1.49** / 0.75 | 0.78 / 0.37 | | | | **3 / 2** | **2** |
| 3 | 9.11 / 3.87 | 3.23 / 1.35 | **1.70** / 0.67 | 1.10 | 0.82 | 0.67 | **5 / 3** (mid: 6 / 3) | **2** |
| 3.5 | | | | | | 1.28 (8 bands: 0.88) | 8 / 6 (mid: none) | 3 |
| 4 | | | | | | 2.25 | **none** / 5 | 3 |

- **The engine credits a band as a fixed point, so it under-braces from λ ≈ 2.** At λ = 3 it places 2 bands where 5-6 are needed. Revision 1's "add one band" was wrong at λ = 3: three bands still give 1.70.
- **A two-post ceiling.** However many bands they share, two posts bend together as one beam. That beam's sway sets a minimum: **λ_ceiling ≈ (1 + s²/r²)^(1/4) ≈ √(s/r)** under drag. It is 3.8 for the thin pair and 3.5 for the mid pair; with finite bands the practical value is 3.3-3.6. Past it, only thickening, more posts or a wider frame help.
- The engine's mast model (`panel_sway_over_pixel`, slenderness.rs:2643) clamps both posts at the run base and ends the mast at the tie. Its own comment says it is optimistic above a lower tie. **It cannot be reused unchanged for two continuing posts with several bands.** R6 needs a multi-band chain model.

---

## 4. Junction sizing: should the post be thicker below a brace joint?

- **The torque flows down until the frame takes it.** In the staged frame, most of a brace's torque goes into the lower bands and the partner post, not down to the plate. A local collar helps only the section it covers, and the shaft below a collar still sees whatever the frame does not carry. A collar is never the right extent.
- **The vertical part is small.** The brace's peel adds 1.85·πr_b²·p_b of pull to the post, at most 1.85 in X units on a post sized for 18.
- **R2 removes the brace-made raise.** If a brace is never thicker than the post's own radius, the post section is at least as strong as the brace root for that brace's torque. The 26 brace-made "fat post, thin top" steps in the v11 diagnosis then disappear. R2 is a design choice (section 2), and R9 checks what it does not cover.
- **Combined loads.** Where a shared trunk carries a seated contact while a higher brace grows (none on v11), the section must pass the sum: σ = X_model·p_cap + 4M/(πr³) ≤ X_ALLOWED·p_cap, with M from p_b. R9 includes it.
- **Shape.** Load steps at branch landings stay (they are load). Draw them as a cone above the joint, the v11 diagnosis rule.

Verdict for the three options you named:

- **Neither** for the brace itself, under R2.
- **Raise to the next band or the plate** wherever a raise remains for a load step.
- **Local collar:** never.

---

## 5. Thicken or brace

Per post and per run over its limit: λ = L/(S·r), n = bands needed (section 3), P = planes needed (2 for every direction, 1 for Y-first), and c = 1 if the partner also needs that cross, 2 if only this post does.

- **Thicken:** hold-then-taper to r_eq = λ·r. Cost about **π·r²·L·(λ² − 1)**.
- **Brace:** **c·P·n·π·ℓ³/S²** with r_b = ℓ/S. Under R2, no junction raise.
- **Break-even reach: ℓ*/(S·r) = [λ(λ² − 1)/(P·n·c)]^(1/3).** R2 caps ℓ at S·r, so bracing wins for every allowed partner when ℓ*/(S·r) ≥ 1.

| λ (bands n) | every direction, shared (c = 1) | every direction, alone (c = 2) | Y-first, shared | Y-first, alone |
|---|---|---|---|---|
| 1.25 (1) | 0.71 | 0.56 | 0.89 | 0.71 |
| 1.5 (1) | 0.98 | 0.78 | 1.23 | 0.98 |
| 1.75 (1) | 1.22 | 0.97 | 1.53 | 1.22 |
| 2 (2) | 1.14 | 0.91 | 1.44 | 1.14 |
| 2.5 (3) | 1.30 | 1.03 | 1.64 | 1.30 |
| 3 (5) | 1.34 | 1.06 | 1.69 | 1.34 |
| above about 3.3-3.6 | past the two-post ceiling: no number of bands works | | | |

**A principled rule: brace in a window.**

- **Below λ ≈ 1.5** (shared, every direction), about 2.5 (alone, every direction), or about 1.3-1.5 (Y-first), thickening is cheaper. A small overshoot does not leave a "long skinny post": the post is already near its limit.
- **From there to the ceiling**, a brace to an allowed partner in the right direction is cheaper. This answers your doubt: long skinny posts get braced.
- **Past the ceiling**, two thin posts cannot hold each other. Thicken until λ drops into the window and then brace, or brace to a wider frame (the ceiling grows as √(s/r)).
- **No partner** in a needed direction within S·r: thicken.

The law caveat stands: the table assumes L ∝ r. If the coupon finds L ∝ r^β, thickening costs λ^(2/β) − 1. β = 2/3 (tip-force law) makes thickening much dearer, and β = 4/3 cheaper.

---

## 6. The proposed rule set

Candidate policies for the coupon to test. Each rule says where its numbers come from.

| # | rule | source |
|---|---|---|
| **R1** | **Joined crosses.** Both diagonals share one band, and each centreline passes within **max(r_b1, r_b2)** of the other, so one axis lies inside the other brace. For equal radii, the overlap is then at least 39 % of the section. Revision 1 used r_b1 + r_b2, which allows mere touching. Check that the joint exists in the layer where the second diagonal reaches it. Plan bands on the drawn axes where posts lean. A pair that cannot be joined is refused (another partner or rung is tried). | geometry; the max(r) cutoff is a design choice **[coupon F4]** |
| **R2** | **A brace is never thicker than either member it joins at its band:** r_b ≤ min(r_A, r_B), using the members' own radii. With r_b = max(r_min, ℓ/S), reach is ℓ ≤ S·min(r_A, r_B). The brace junction raise is deleted. | design choice (equal-moment stress comparison); S |
| **R3** | Brace radius stays **r_b = max(r_min, ℓ/S)**. Log X_root. No limit until F1 measures root failure. | S **[coupon F1]** |
| **R4** | Brace angle stays at the saturated angle. 45° would cut root bending and the overhang step by 35 % and add 68 % brace resin. | overhang law **[decision D3, coupon F1]** |
| **R5** | **Directions.** (a) Before seating, per post: each band on a requesting run has two crosses at least α_min(λ, n) apart (every direction), or one cross within β_max(λ, n) of Y (Y-first). (b) After seating, per cluster: at least 3 brace planes among the posts under one model region, not all parallel and not all through one point. | `angle.ts` formulas from the frame **[decision D2, coupon F3]** |
| **R6** | **Count bands through a multi-band frame, not as fixed points.** For each run, place n evenly spaced bands and solve the two-post (or cluster) chain; raise n until sway ≤ 1 px. Stop at the two-post ceiling, then fall to R7's other remedies. | frame model; S |
| **R7** | **Thicken or brace by resin** with the corrected n, P and c (section 5). Brace only inside the window, otherwise thicken (hold-then-taper), then a mast, then a warning. | resin; S; the law's exponent **[coupon A/B]** |
| **R8** | No brace raise (R2). Load steps at landings are drawn as a cone above the joint. Any raise that remains runs to the next band or the plate, never as a collar. | statics |
| **R9** | **Construction-stage check.** At the last layer before each cross closes, solve the cluster frame with every growing diagonal's peel on its tip (stage.ts method, a sparse solve in the engine, one solve per stage for all loads). Add the drag sway for the same tops. **Warn** where a printing top exceeds 1 px, or a section exceeds X_ALLOWED with model pull plus brace bending. p_b = 1.24 kPa until F2 replaces it, and the warning states p_b. Refuse nothing. | S-law placeholders; F2 |

Emit and warn, never refuse: every rule picks another remedy or warns on the emitted file.

---

## 7. Predicted counts, resin and warnings

**A feasible remedy, not an upper bound and not an engine run.** `tools/predict3.ts` takes the crosses v11 chose (`v11-feedback-diagnosis/data/{sph,cyl}.ties.jsonl`). It applies R2, the corrected band counts (R6) and the derived angles (R5a). Each cross is charged in full to the run that uses it. Runs that fail are thickened to λ·r. The engine could choose other partners and add bands, so this is one feasible answer under the model, not the best one. A second column estimates adding each failing run's missing bands at its median cross cost (partner availability not checked).

| model | v11: braces, brace resin (+ raise) mm³ | revised, every direction | revised, Y-first | if failing runs get their missing bands instead | warnings (thickening past r_max) |
|---|---|---|---|---|---|
| Sphere (31 runs) | 82, 9.0 (+2.3) | **0 braces qualify**; thicken 31 runs on 18 posts: **4.5 mm³** | 4 braces + thicken 27 runs: 4.6 mm³ | 3.6 / 2.3 mm³ | 0 |
| Cylinder (118 runs) | 324, 43.5 (+5.9) | 92 braces (13.7 mm³) + thicken 85 runs on 42 posts (188 mm³) = **202 mm³** | 36 braces + thicken 95 runs = 214 mm³ | **74 / 45 mm³** | 0; 1 run (λ 3.48) is past the two-post ceiling |
| stilt plate | 54, 57.4 | all 27 crosses meet R2; 1 top over 1 px in R9 | 31 Y-ish diagonals kept, 23 dropped; the β_max test is not applied to the file geometry | | 1 (arm 11, from v11) |
| brace pair | 18, 20.6 | R1 re-plans the 2 unjoined diagonals; 2 tops over 1 px in R9 | Y needs masts or thickening (posts in a line) | | 4 until Y is held |
| lonely peak | 0 | 0; the model column blocks every band | | thickening or masts (v11 diagnosis +115 to +190 mm³) | 3 now; the fix is not re-verified here |

Readout:

- **v11's own crosses mostly do not pass the corrected test.** As the requesting post, v11 gets one cross per band level on 21 of 31 Sphere runs and 77 of 118 Cylinder runs. Those runs have one plane where the per-post test needs two, and the engine's band count is short from λ ≈ 2.
- **The corrected rules cost resin on the Cylinder.** The cheapest estimate here is 45-74 mm³ against v11's 49 mm³, and it is higher if partners are missing. The extra goes mainly to long runs (40 Cylinder runs are above λ = 1.75 and 14 above 2.5), where v11 placed fewer bands than the frame needs. On the Sphere the corrected rules cost less (2.3-4.5 against 11.3 mm³), because thickening its short overshoots is cheap.
- **Revision 1's "Sphere 82 → 26, Cylinder 324 → 124" and its zero-warning claims are withdrawn.** They counted any two crosses as two directions and credited bands as fixed points.
- **R9 at p_ref** flags 11 printing tops: Cylinder 3, Sphere 3, laid tube 2, brace pair 2, stilt plate 1. The count changes with p_b.

---

## 8. What the slenderness coupon should add

These go on the same plate as `slenderness-coupon-design.md`, under its 65 mm height cap.

| family | specimens | what each one is | what it answers, and how to read it |
|---|---|---|---|
| **F1 diving boards** | 8 | A stub post (r 1.0, 6 mm). One diagonal leaves it at φ 45° and 57.5°, with r_b 0.05, 0.10, 0.20 and 0.40. It runs out 15 mm sideways and meets nothing. | **Record the failure mode first:** root fracture, curl or sag, tip detachment, or none. Only root fractures say anything about peel: lever at fracture ∝ r_b means peel scales with area, ∝ r_b² means it scales with width. Curl and sag answer R4 (57.5° or 45°). |
| **F2 brace on a thin post** | 6 + 2 controls | Post r 0.25, joint at 20 mm. A joined |X| to a stiff anchor (r 1.0) at gap d 3, 6 and 9 mm, r_b = ℓ/S (d 9 breaks R2 on purpose). Repeat at r 0.5. Controls: the same posts with no brace. | **Does brace shake mark the print?** Predicted at p_ref: about 3, 22 and 74 px at r 0.25, and 16 times less at r 0.5. Marks above the joint that the control lacks give a lower bound on p_b. No marks means the shake is harmless at those sizes. It does **not** prove p_b < 1.24 kPa, because green resin can spring back before exposure. Both outcomes set R2 and R9. |
| **F3 square layouts** | 15 squares (60 posts) | r 0.40, 7 mm squares, one band at mid-height. Layouts: outer four; middle X only; Y sides only; X sides only; **unbraced control**. Each at three heights, 1.2, 1.4 and 1.8 times S·r: 19, 22 and 29 mm at S = 40. | Before seating, the model predicts these layouts hold up to: outer four about 1.6 times the unbraced limit L_u; middle X about 1.3 times in its weak direction; Y sides 1.0 times in X. So at 1.2 times only the one-side layouts should mark (Y sides in X, X sides in Y). At 1.4 times the middle X should mark too. At 1.8 times all should mark. Read the lean direction of each column against the unbraced controls at the same height. If family A puts L_u away from S·r = 16 mm, read each height relative to the measured L_u. Revision 1's single 32 mm height sat at or past the outer four's predicted limit (1.2 px under a tip force, 2.4 px under drag), so it could not discriminate. |
| **F4 joined or not** | 6 | Pairs (r 0.4, L 32, 7 mm apart): a single diagonal; **two diagonals offset sideways so they pass without fusing**; a joined |X|. Two of each. | Separates the halved lever (R1) from the extra stiffness of a second diagonal. Also check the joint cross-section at the meeting point for R1's max(r) cutoff. |
| **G stilt plate fixture** | 2 prints | `make_tall_stilt_plate` as emitted, and the same with every brace deleted | **Does the forest sway under the 30 × 30 mm plate's own peel?** Read the layer lines on the plate's side walls. This is the seated-frame load the S law does not see. |

Readouts to add to families A and B: a bow at mid-height with the pad still level is **consistent with** buckling under the squeeze. A lean from the root is consistent with a cantilever. Neither shape proves its mechanism on its own. Compare against the pad-weight variation in B: if a lighter pad moves the failing rung, the pad's load is involved.

---

## 9. Decisions for you

| # | decision | my recommendation |
|---|---|---|
| D1 | **R2: a brace is never thicker than the posts it joins** (deletes the brace junction raise) | **Yes, as a cheap guard.** It is a design choice, not a proof; F2's d = 9 specimen tests it. |
| D2 | **Directions: (a) every direction or Y-first before seating; (b) the 3-plane frame rule after seating** | Keep every direction before seating until F3 reads. The after-seating rule then holds without extra crosses wherever per-post cycles exist. |
| D3 | **Brace angle: the saturated 57.5° or 45°** | Keep 57.5° until F1 shows whether it curls. 45° costs 68 % more brace resin. |
| D4 | **R6: count bands through a multi-band frame model** | **Yes.** v11 under-braces runs above about 2 times their limit by its own law. Expect more Cylinder resin (section 7). |
| D5 | **R7: brace in a window, thicken outside it** (partly reverses the 2026-09-05 rung order) | Yes, with R2 and the corrected n. |
| D6 | **R9: a construction-stage shake check that warns, with p_b stated** | Yes, as a warning only. 11 tops warn at 1.24 kPa; more if F2 shows a higher p_b. |
| D7 | **Coupon families F1 to F4 and G as revised** | Add them. F1, F2 and F4 are under 1 mL. F3 is about 0.75 mL. G is a second stilt plate print. |
| D8 | (Optional) **Ties to the model's own columns** for the brace pair and the lonely peak | Your call. Otherwise those runs go to masts or thickening, or they warn. |
| D9 | **The two-post ceiling:** past about 3 times its limit, a post pair is thickened into the window or braced to a wider frame, never given ever more bands | Yes. It follows from the frame and has no new number. |

---

## 10. Method, limits and files

- **Brace geometry and post radii** are read from the v11 files: `for-craig-v11/measure/*.bdump2.jsonl`, `*.braceends.jsonl` and `*.members.jsonl`.
- **The staged frame** (`stage.ts`): members are straight lines between their drawn ends, and radii follow a linear profile through each member's base radius and every brace-end radius. Branches attach to the trunk whose axis passes within 0.08 mm; the result does not change at 0.4 mm. Members with no trunk start on the model and are fixed there: 22 of 75 Cylinder branches, 44 on Morel_base. Seated tops are left free (softer). The upper halves of a fused cross that are still growing are not loaded. The tool matches a closed-form cantilever exactly.
- **The layout models** use equal loads and a cap 10 times the post radius as the "rigid" model. Real model faces bend, and real loads follow the release front.
- **Own (ledger) radii, cross sets, runs and planes** come from the v11 diagnosis probe dumps (the final serial sweep). The kept counts match the log: 41 and 162.
- **Not measured, marked [coupon]:** p_b on brace and branch sections; S and its exponent β; q; E; the tilt-shear fraction μ; the squeeze fraction c; recovery between layers; end restraint for buckling; ChituBox's own cross geometry.
- **Predictions in section 7 are arithmetic, not runs.** No engine binary was built or run.
- **Files:**
  - `bracing-mechanics/tools/`: revision 1 `run.sh`, `braces.ts`, `pi.ts`, `frame.ts`, `layouts.ts`, `study.ts`, `bands.ts`, `xset.ts`, `predict2.ts`, `branch.ts`; revision 2 `stage.ts`, `pi2.ts`, `layouts2.ts`, `angle.ts`, `bands2.ts`, `predict3.ts`;
  - `bracing-mechanics/data/`: revision 1 `braces.jsonl`, `braces.summary.txt`, `pi.out`, `study.out`, `bands.out`, `crossset.out` (kept for comparison); revision 2 `stage.out`, `pi2.out`, `layouts2.out`, `angle.out`, `bands2.out`, `predict3.out`.

---

## 11. Challenge response

The critique is in `bracing-mechanics-challenge.md`. Each of its points is answered below.

| # | challenge | verdict | what changed |
|---|---|---|---|
| 1 | Exploratory, not implementation-ready | **Accepted** | Stated at the top. The rules are candidate policies for the coupon to test. |
| 2 | Arithmetic largely passes | Noted | No change. The recomputed numbers (E 107 MPa, X_root 124.6, Euler 4.34/17.4/69.5 mN, +68.6 % at 45°, F2 2.75/22/74 px) agree. |
| 3 | 1.24 kPa is the bound for a 0.05 mm wall, not every brace; E is not a measured minimum; withdraw "does not snap", "no hazard" and "upper bounds" | **Accepted** | p_b is now a parameter (section 0). Snapping is conditional: it needs p_b above 39-81 kPa joined and 18-36 kPa unjoined. The ChituBox comparison is evidence, but only comparative. Every shake result gives the p_b at which it reaches 1 px. Shake in px is proportional to p_b/q, so E is a unit, not a bound. All three phrases are removed. |
| 4 | pi.ts clamps at the next band; loads were isolated; use M·h·(H − h/2)/EI; delete "pull and torque never add" | **Accepted, and the correction changes the result** | New `stage.ts` builds the real v11 cluster at each closing layer: every band is compliant, every growing diagonal is loaded at once, and the printing-top move is reported. Result: 11 tops over 1 px at p_ref, worst 3.8 px. The new `pi2.ts` uses the critique's formula on a chain with no band credit (94 tops, too soft to use). The "never add" sentence is replaced: sequential on a straight post, possible on a shared trunk, none on v11 (checked), and R9 checks the sum. One part is rebutted: the joint-only figures already used each diagonal's measured meeting point, not d/2. |
| 5 | r₁ + r₂ allows tangency; joints need not be at half-gap; R2's ratio of 1 is a design choice | **Accepted** (meeting point partly rebutted) | R1 now needs each axis within max(r₁, r₂) of the other, at least 39 % overlap for equal radii. Under it, 26 Cylinder, 8 Sphere, 16 laid-tube and 2 brace-pair diagonals are not joined (revision 1: 4, 6, 10, 2). On meeting point: measured s is 0.50 at p50-p90 and 0.56 at most, and the tools used the measured lever. The rule now says max(s, 1 − s)·d. R2 is relabelled a design choice and lists what it does not bound. |
| 6 | Buckling K values need restraints a floating cross does not give; no universal 4-16×; a bow is not diagnostic | **Accepted, with one further correction** | Point-contact tips make a seated top nearly pinned, so "K = 1 under a model" was wrong too: a model that can sway leaves K close to 2. The 4-16× claim and the 2 % threshold are withdrawn (it is about 0.5 % at K = 2, still conditional). The coupon reads shape against controls. |
| 7 | layouts.ts had no rigid cap; test asymmetric loads and twist; "frames cannot pool", "cycles are minimum" and "never middle X" overgeneralize; the angle windows are heuristics | **Accepted** | `layouts2.ts` adds the seated state with a cap, a front-corner load and twist. **After seating, frames do pool directions.** The rule becomes per post before seating and, after seating, 3 planes per frame, not all parallel and not all through one point. "A cycle is the minimum" now holds only for the per-post state. The middle X is close in translation after seating (0.27 against 0.21). It is still worse before seating and against twist, and costs more, so "not recommended" stands with reasons. The windows are replaced by α_min and β_max from directional compliance (formula within 4-15 % of the frame). |
| 8 | R6 needs a coupled frame; λ = 3 with 3 bands is still 1.70; panel_sway_over_pixel cannot be reused unchanged | **Accepted** | `bands2.ts`: λ = 2 needs 2 bands, 2.5 needs 3, 3 needs 5-6. There is a ceiling near √(s/r) past which no band count works. R6 is now an iterated multi-band frame. The note on the engine's model is in section 3. |
| 9 | Break-even assumed shared half-crosses; predict2 counted any two crosses as two directions; counts are not upper bounds; zero warnings unsupported | **Accepted** | The break-even now carries c (shared or alone) and the corrected n. `predict3.ts` enforces R2, the angles and the band counts, and charges crosses in full. It reports a feasible remedy, labelled as such. The Sphere goes to thickening (4.5 mm³). The Cylinder needs more resin than v11 (45-202 mm³ depending on remedy). The old counts and the zero-warning lines are withdrawn. |
| 10 | Coupon: F1 exponent only if root fracture controls; F2 "no lines" cannot prove p < 1.24 kPa; F4 needs an unjoined two-diagonal control; F3 at about 1.00 px cannot discriminate | **Accepted** | F1 records the failure mode first. F2 reads "harmless or not", and marks give only a lower bound on p_b. F4 adds the offset unjoined pair. F3 now uses three heights (1.2, 1.4 and 1.8 times S·r) with unbraced controls, chosen so that the layouts' predicted limits fall between them. |

What the challenge did not overturn:

- braces load their posts while they print, as a cube-of-gap torque;
- the outer four beat the middle X and all six on Craig's square, in every state tested;
- posts in a line cannot be held across the line by each other;
- long skinny posts should be braced rather than only thickened, inside the window;
- a local collar is never the right junction fix.
