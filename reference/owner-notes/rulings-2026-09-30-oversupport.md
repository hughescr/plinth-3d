---
name: rulings-2026-09-30-oversupport
description: "Craig 2026-09-30 rulings after the over-support attribution: X_ALLOWED 12 → 18; support geometry (ball ratio, base boss 2.2×1.0 mm, pad, raft 0.2 mm) comes from his ChituBox profile export at 'Chitubox Projects/chitubox-support-specs.cfgx'; build all four engineering fixes (boundary-aligned lattice, 8% reach-metric fix, cross-layer neighbour-cover removal, no brace inside a flare)"
metadata:
  node_type: memory
  type: feedback
  originSessionId: c28347f5-0287-4141-be62-5816b1a4e530
  modified: 2026-10-01T06:58:27.214Z
---

Craig, 2026-09-30, answering the over-support decision list
(`field-2026-09-28/oversupport-attribution.md`):

1. **X_ALLOWED = 18** (was 12 = floor(25/2), a ×2 design margin). 18 sits
   below the plate-6 bracket's lower edge (19.91) and at ~0.72 of the field
   snap ratio ~25. Provenance: his ruling. Predicted −25..−28% resin,
   −33% tip scar, same arm count.
2. **Support geometry comes from his ChituBox practice.** He exported his
   profiles to `/Users/craig/code/hughescr/dnd-campaigns/Chitubox Projects/chitubox-support-specs.cfgx`
   (JSON; profiles 0 Light, 1 Middle, 2 Heavy [currently selected],
   3 Ultralight). Key values: touch-tip/tip-up diameter 0.3/0.2, 0.5/0.4,
   0.8/0.6 (ball ratio 1.25–1.5); boss bottom diameter 2.2 mm, boss height
   1.0 mm (all profiles); raft thickness 0.2 mm, area ratio 110%; cross-brace
   diameter 0.5 (Light) / 0.8 (Middle, Heavy); tip spacing (touchtipdistance)
   6–8 mm; contact depth 0.1–0.3; spacing from model 0.6. Rule applied: import
   geometry RATIOS and fixed shapes only; radii stay load-sized by physics.
3. **Build all four engineering fixes:** lattice aligned to each island's
   boundary; exact reach metric (raster was ~8% short); cross-layer
   neighbour-cover removal with reach re-check; no brace tie inside a root
   flare.

**Later rulings 2026-09-30 (after rounds 6–7):**
4. **Support base:** boss height 1.0 mm, width = HEAVY profile ratio (2.2 / 1.5 ≈ 1.47× the root diameter) for every support. A fixed 2.2 mm boss raised resin +4–15% (our fields are 2–4 mm dense vs ChituBox 6–8 mm); profile-interpolated ratio still +5–26%.
5. **Lattice-align: LAND**, accepting the Sphere's uncarried rise 376→405 mm² for now; drain-rim routing is its own follow-up lane.
6. **45° creep rule ADOPTED:** faces at/steeper than the registered 45° overhang angle charge no creep (inside-hollow support demand was 95–99% this artefact). Trace the Cylinder +18 mm² bare-area side effect before landing.
7. **Reach stays 2.130 mm** (conservative end of the coupon bracket 2.130–2.592); near-idle outside-underside arms stay.
Orchestrator rulings same day: corner tip seats on the face it holds up (shallower than 57.5°, the steep end of the overhang band), never a wall; make_toed_slab flip = fixture defect (three overlapping shells) → repair fixture, don't touch the objective; Sphere +0.43 mm² from X18 accepted as negligible.

**Why:** the attribution showed sizing is exactly load/X on every arm; the
look came from the ×2 margin, ball/flare/pad overheads (50–72% of resin),
edge-fill arms and near-idle creep arms on curved flanks plus their braces.

**How to apply:** round 6 workflow wf_e0797d49-4b5 builds these. ChituBox's
6–8 mm tip spacing is NOT imported: our reach (~2.1–3.4 mm) is coupon physics.

Related: [[plinth-state-2026-09-28]], [[sweep-field-feedback-2026-09-29]],
[[physics-first-constants-doctrine]], [[tip-size-field-data]].
