---
name: overhang-limit-estimate-2026-09-02
description: "2026-09-02 best-guess resin overhang limit from existing data (no new coupon, Craig's call): soft transition centred ~45° spanning ~30–55°, multiplying plate-projected area, plus the already-specced per-layer reach ramp 1.345→2.592 mm; the arm-snap ratio does NOT transfer to strips"
metadata: 
  node_type: memory
  type: project
  originSessionId: 310c4c58-ad81-435c-8000-7c11d508a02a
  modified: 2026-09-02T09:06:00.545Z
---

Craig refused a new coupon plate (printer teardown not worth it) and asked for a
best guess from existing data. Fable derived, Sol verified adversarially; Sol's
corrections won on evidence. Cost ~290k tokens — do not re-derive.

## What the evidence actually is

- **No coupon in the repo ever varied overhang angle.** Plate 7 (0/45/70°
  tilted caps) is pre-registered only, no results
  (`docs/plans/load-model-whole-pipeline-coherence.md:581-599`).
- **Flat single-lamina reach is the direct datum:** one 0.050 mm layer
  cantilevered at 0° — first loss at 2.592 mm, universal loss at 4.671 mm,
  pooled `L_crit ∈ [2.13, 4.67]` mm; the audited span spec already derives a
  **1.344976 mm allowance** and a per-layer connected geodesic reach definition
  (`docs/specs/audit-span-criterion.md:175-233, 252-340`). Failure mode
  unmeasured (plate-04-results.md:201-204).
- **The arm-snap ratio X*≈25 / X_ALLOWED=12 does NOT transfer to overhang
  strips.** Arms were loaded by suction under flat caps (~0.1 MPa scale);
  strips see kPa film peel; the repo's own protocol memo puts the tensile
  reading of X*=25 at 155–320× the plate-1 peel bound
  (`plate-06-verification-protocol.md:67`). Any angle derived through p/σ =
  1/25 or 1/12 (19°, 26°, 35°) is algebra on an invalid transfer.
- **Per-layer strip bending never governs above ~1–2°** (flat lamina holds
  50–90 layer-heights of reach; a 45° face steps one layer-height). What
  governs at angle is cumulative deflection / interlaminar cleavage over
  reach; direction of the stacking correction is unknown (span spec :438-445
  calls the rigid-spine coupon optimistic by an unknown amount).
- **Only angle-varying evidence is literature:** e-Stage default 30° failed 4
  of 5 models; per-model criticals 30/41/46/49/55°, median 46°
  (`LITERATURE-NOTES.md:934-945`). The current 45° "mirrors CHITUBOX
  autosupportangle" (physics_contacts_tips.md:33) — convention, not physics.
- Front-to-back tilt factor ≤1.53× is a NON-DETECTION BOUND at ladder
  resolution, not a measurement (`docs/plans/load-model-physics-first.md:98-106`).

## The recommendation (Sol's, adopted)

- **Weight w(θ):** smooth logistic in face angle from horizontal, centre
  **45°**, 10→90 transition ~25° wide (≈ full below 30°, ≈ zero above 55°).
  Centre provenance: literature median, NOT the FDM convention it happens to
  equal. Label it a heuristic; the family is superseded by any coupon that
  varies angle at fixed reach.
- **Multiplies plate-projected down-face area `A·|nz|`** (= sum of all
  per-layer unsupported strips, layer-height independent), not facet area.
  Scar term keeps facet area × delicacy.
- **Extent rule:** per-layer connected geodesic reach from cured material per
  the span spec; demand ramps from zero at **1.345 mm** to full by **2.592
  mm**. No separate 4.671 mm warning tier.
- **Same function at both consumers** (`plinth-orient/src/score.rs` ~:262,
  :941-951 and `plinth-support/src/lib.rs` ~:902-914). Candidate admission
  keeps its current machinery with the soft tail (prune w≈0 faces — constant
  prunes search); do NOT go one-candidate-per-triangle (treant already 8,906
  eligible / 54,092 rows) and do NOT drop sub-minimum tips (a new cliff at
  0.094 mm² demand) — production clamps radius to the printable minimum
  (perception.rs:568-575, 641-647).
- Fable's proposed `w = D/(1+D), D = 6cot²θ/12` is REJECTED: built on the
  invalid ratio, and its own arithmetic was wrong (w=0.9 at 13.3° not 16°,
  w=0.1 at 64.8° not 61°).

Related: [[round-base-diagnosis-2026-09-02]], [[lift-always-elephants-foot]],
[[physics-first-constants-doctrine]], [[support-arm-load-physics]],
[[design-framing-optimism-bias]] (the proposer's centre came from the very
ratio it had just discredited — same pattern).
