# Adversarial verification of the 4 action-changing mesh-forensics calls

Independent re-check of the four claims that gate the owner's pre-print manual-support
decisions on the v4 treant. I did **not** reuse the prior agent's `classify_mesh.py`.
All probes below are my own code (`/Users/craig/.claude/jobs/0ddf6d41/tmp/vindep_*.py`).

## Method (independent of the prior pass)
- Mesh: `audit/treant_real_copy.stl`, 576,026 faces, loaded `process=False`,
  centered `-(bounds_min+bounds_max)/2`. Bottom of model at z=-63.98; engine
  `plate_z=-66.976` (= bottom - lift 3.0). Members/balls from `engine_run_state.pkl`
  (frame confirmed to coincide with centered-mesh frame: member p0 inside bounds;
  P2 "55mm above plate" reproduces to 55.32mm above engine plate).
- **Winding is inconsistent**, so I never trusted face-normal SIGN and never used
  `contains()`. Down/up orientation was determined by a **sign-agnostic air-side
  probe** (which side of a surface point is open air).
- **Ray engine:** trimesh's built-in `ray_triangle` intersector blew past the 2 GB
  RSS cap on my machine too (2.17-2.44 GB on a single 64-256 ray batch; my guard
  aborted it) — I reproduce the prior pass's OOM warning. I therefore wrote my own
  **vectorized Möller-Trumbore intersector against a locally-cropped submesh**
  (crop by face-centroid box). Memory bounded by (1 ray x local-tri-count);
  **peak RSS across every real run = 0.49 GB.** This is a *different* method than
  the prior pass (which used full-mesh trimesh casting + a KDTree crop).
- Enclosure metric: an "escape" = a Fibonacci-sphere direction whose ray hits **zero**
  faces within the crop. Because I crop locally, my test is *more conservative* than a
  full-mesh test (distant blockers are ignored) — 0 escapes therefore means the point
  is blocked within the crop radius in **every** direction (true local burial).

## Controls (method discriminates correctly)
Same metric, crop R=20, 800 dirs: `CTRL_EXT_x60` → 793/800 escapes,
`CTRL_EXT_y45` → 800/800 (outside model). Claimed-external claim points P2 → 280,
P4 → 374 escapes. Claimed-internal P1 → 0. Clean separation.

---

## CLAIM 1 — RETRACTION: crown-tip (36.2, 3.9, 52.7) is INTERIOR, needs NO support
**VERDICT: CONFIRMED (interior; the retraction is correct).**

- Full-sphere escapes (crop R=20): **0/300, 0/800, 0/2000.**
- Crop-radius robustness at 800 dirs: R=10 → 0/800 (56,038 local faces),
  R=20 → 0/800, R=40 → 0/800. Blocked within 10mm in every direction.
- **Lower hemisphere** escapes from a point 0.2mm *below* the surface: **0/300** —
  there is no exposed underside on any side. (Contrast P2=188, P3-from-below small,
  P4=154.)
- Shell-crossing evidence (rays out of P1): +z_up crosses **9 shells within 0.43mm**;
  +radialXY **7 crossings within 1.0mm**; +x_out 5 within 0.97mm. Straight-down also
  hits at 2.05/2.08/2.68mm (shells below too).
- Local face density: **5,461 faces inside a 7x7x3mm box** — stacked
  self-interpenetrating shells, exactly as claimed.
- The closest face has |nz|=0.875 (looks down-facing) but it is an **internal shell
  surface**, not an exposed overhang — 0 escapes full-sphere AND 0 lower-hemi prove
  nothing there is exposed. The "minima" is a perception artifact between overlapping
  primitives. **No support needed; supporting it is impossible (buried).**

## CLAIM 2 — NEW RISK: broad overhang (-19.5, 13.2, -11.7) NEEDS a manual support
**VERDICT: CONFIRMED.**

- Point is on the surface (claim→surface 0.05mm). Closest face **|nz|=0.928**
  (≈ claimed z-normal -0.95): steep, near-horizontal **down-facing**.
- Height **55.32mm above plate** (claim ~55mm ✓).
- **Corridor / air below:** straight-down ray from just below S hits **NOTHING** —
  open all the way to the plate (corridor clear ✓). Straight-up hits solid at 4.86mm.
- Exposure: **188/300 lower-hemisphere directions open** below; 1/300 above (solid
  above). Genuine broad exposed overhang.
- Down/up-facing patch area within 7x7x3mm = **83.1 mm²** (claim island ~77.3 mm² —
  same scale ✓).
- **Nearest support member surface-distance = 3.70mm** (stub, contact 60) — matches
  claim "no member within 3.7mm" exactly. Members that do sit in the 35° downward
  cone (m#56 @3.71mm, m#54 @5.94mm) belong to *adjacent* contacts and are all beyond
  the ~1.5mm coverage radius. **This island is uncovered → needs a manual support.**

## CLAIM 3 — NEW RISK: second overhang (3.8, -6.9, -10.2) likely needs one
**VERDICT: CONFIRMED (genuine unsupported overhang).**

- On surface (0.026mm). Closest face **|nz|=1.000** — perfectly horizontal
  **down-facing**. Straight-up hits solid at 2.51mm (a slab underside); straight-down
  has a **5.2mm void** before the next geometry.
- This point sits in a busier region than P2 (full-sphere 0 escapes; lower-hemi from
  below only 5/300 open — lots of nearby side geometry) — but that geometry is
  **lateral / adjacent-contact supports, not support beneath the overhang**:
  - Nearest members: m#254 stub (contact 314) 2.38mm at **dz=0 (lateral)**;
    m#1186 strut 2.51mm dz=-1.0 but offset; m#170/m#169 stubs (contacts 181/180).
  - **Members inside a 35° downward support cone within 6mm beneath the overhang:
    NONE.** The nearby stubs support neighboring contacts, not this island.
- So it is a real horizontal down-facing island with a ~5.2mm clear void below and no
  support in its supporting cone → **needs a support.** (Weakest of the four, and the
  claim already hedges "likely," but it holds up.)

## CLAIM 4 — CONFIRMATION: stranded overhang (32.1, -12.1, 43.7), external, fine-tip
**VERDICT: CONFIRMED.**

- **External:** full-sphere 374/800 escapes; lower-hemi from below 154/300 open;
  8.2mm void straight down. Not buried (unlike P1).
- **Stranded:** nearest support member surface-distance = **10.22mm** (stub, contact
  458) — matches claim ~10.2mm exactly. Nearest ball 11.54mm.
- **Thin feature / fine-tip needed:** local down/up-facing patch = **1.0 mm² (19
  faces)** — a small protruding spike (contrast P1's 5,461 faces / P3's dense region).
- **Tight clearance:** descending from the feature, nearest lateral model surface over
  48 azimuths = **0.25mm (z-1), 0.50mm (z-2), 0.51mm (z-3), 0.37mm (z-4)** —
  **tighter than the claimed ~0.8mm** envelope collision. A standard 0.95mm strut
  envelope collides almost immediately; only a **fine-tip** support at a favorable
  angle fits. Fully consistent with the claim (and the "groove-blocked / tight" note).

---

## Bottom line
All four action-changing calls **survive independent adversarial re-testing** with a
different intersector, different enclosure geometry (local crop vs full mesh), and a
sign-agnostic orientation test:
1. Retract crown-tip (36.2,3.9,52.7) — CONFIRMED interior, no support.
2. Add manual support at broad overhang (-19.5,13.2,-11.7) — CONFIRMED (~83mm²,
   corridor open to plate, nearest support 3.70mm).
3. Add manual support at (3.8,-6.9,-10.2) — CONFIRMED (horizontal down-face, 5.2mm
   void, no member in downward cone).
4. Add fine-tip manual support at stranded (32.1,-12.1,43.7) — CONFIRMED (external,
   10.22mm from support, lateral clearance 0.25-0.5mm → fine tip only).

Numbers reproduced the prior pass's key figures independently (3.70mm, 10.22mm,
|nz| 0.928/1.000, 0/2000 escapes, ~83 vs 77.3 mm²). Memory discipline held: peak RSS
0.49 GB; the trimesh built-in path was confirmed unsafe (2.2 GB on one batch) and
abandoned in favor of the bounded crop-MT method.
