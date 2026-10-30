---
name: h2-plate07-results
description: "2026-08-04 H2 home-calibration plate scored; key physics outcomes, fixture defects, and pending decisions"
metadata: 
  node_type: memory
  type: project
  originSessionId: d0490c5c-5036-418a-aec2-ba31ad38a4b6
  modified: 2026-08-04T18:51:48.844Z
---

Craig scored the H2 home-support-calibration plate (H2-exact-grounded-full-plate.chitubox, sha a33b2e10…) on 2026-08-04. Full scoring matrix lives in the plinth-triage workflow output; it is NOT yet committed to the repo as a plate-07 results doc.

Key outcomes (adversarially challenged, orchestrator-adjudicated):
- Head-loss boundary X* ∈ (25.00, 36.00) on this geometry (X = 2.25/r²) — consistent with plates 5/6; the ratio law now also survives the absolute-arm-area degeneracy test plate 6 couldn't exclude (thin arms 0.28–0.77 mm² carried 6.9 mm² heads intact).
- THE anomaly: V22, V32, BX50 lost heads at X = 9.00 — inside X_ALLOWED = 12. Rests on n=1-vs-1 cells; next plate must replicate r=0.50/18 mm vertical unbraced at n≥6.
- r=0.30 bend ladder fits a height-above-base threshold h* ∈ (9.7, 13.25) mm; three aliased mechanisms (z-band artifact, per-layer cure section, base moment M=F·h·cosθ) — all 76 bases at z=1.5 so the plate can't separate them. Decisive next-plate cells: stilted span-6 vertical AND 45° at matched height.
- Area-convention caveat: scoring the head's single-layer bottom step instead of max section gives X*_step ∈ (11.1, 14.4), bracketing X_ALLOWED=12 — margin claim is not robust to that modelling choice. Head's one-layer down-facing flange is a probable generator defect (plates 5/6 used flares).
- CM controls (r=0.25, X=36) are must-fail by X* but are policy-referenced (one rung below min_shaft_r_mm=0.3); all 4 died in print → no marginal control remains for the lever loading phase.
- NO engine constant changes from one plate (promotion needs 3 independent plates). No wash-basket/vat census was done → no failure formally dated print-vs-handling; census + break-surface forensics must precede cutting coupons.

Loading fixture (HOME-CAL-LEVER-4TO1) has three real defects: (1) carrier tab over pivot-tower footprint — 2.5 mm hard interference, lever "doesn't fit" until tab trimmed flush (one-line fix in loading_fixture.rs:135); (2) zero axial retention (14.0 mm axle == 14.0 mm cradle spacing); (3) seated paddle 11.5–14.5 mm above a normally-seated coupon terminal — reachable only via undocumented proud-seating workaround. Pivot = unlabelled 4 mm forks on short towers at x=0; the labelled 18/24 pockets are coupon receivers (decoy). Related: [[coupon-plate-design-rules]], [[saturn4-tilt-peel-geometry]], [[support-arm-load-physics]], [[resin-print-pipeline-goals]].
