# RETHINK challenge (gpt-astra high, 2026-10-28, read-only at develop db93198b / wt-cr1 63c6e8dd)
Orchestrator condensed from the handback (the lane may not write report files).

## Verdict: REJECT as implementation-ready; ACCEPT the direction
Direction kept: capacity-aware placement; every seat evaluates approaches; contact geometry and reservations immutable after construction. Rejected: claimed guarantees, one-wave completion, deletion readiness.

## 1. Arithmetic
- Diagnosis confirmed (up_sweep.rs:718, 3725–3727; down_sweep.rs:4998–5007; lib.rs:1408). Correction: production snapped pitch 3.628 mm → 11.40 mm²/cell (registry 11.79 uses unsnapped 3.689; evidence.rs:488–491), load radius 0.449, diameter 0.898. Both exceed Q = 9.048.
- p_cap = r_cap·√(2πX/√3) = 3.232 mm at r 0.4 is derived (no constant); use the run's writer-safe radius (contact_cap.rs:138–148).
- It is a DENSITY prescription, not a per-contact bound (snapping, exclusions, boundaries, greedy cover, unequal events, model-mediated load). XY peel vs surface area on tilted faces.
- p_cap is NOT a minimum separation: small forked feet may be closer than p_cap if their patches don't touch (RETHINK.md:65 wrong).
- Forecasts: recompute bounds from the pinned run (cr1: 48/204/842). "Round_100 ~2×" not implied (baseline 745 arms vs bound 842).

## 2. Stage A needs a real capacity-aware cover
- choose_plan picks the fewest seats regardless of capacity (up_sweep.rs:1644–1692).
- trial_seat does not call root_and_clear (1982–2008); seeds cleared earlier skipped (1872–1880). Changing only commit leaves trials picking under-dense plans.
- Limiting cleared cells doesn't limit the later Voronoi catchment (3565–3588; 1207–1208). Keep reach and capacity allocation as distinct state with shared trial/commit/undo. Validate against the production router.

## 3. Normal stubs: a candidate, not a universal answer
- abs_cos=1 removes r·sinθ but the flank term remains the layer height; depth = max(allowance, layer_height, ball_term) limited by equator (contact.rs:179–200, 442–460; lower.rs:3495–3606). Diameter = 2r only when the ball term dominates.
- Heads can fail JointOverMax/NoPost (heads.rs:1668–1715, 2287–2348). Census: 42 normal-head refusals = 33 HitsMember, 6 HitsModel, 2 HitsHead, 1 NoPost; none of 17 strip cases had an admissible normal head; 5 strips were joiners (RESULTS-apc.md:15–30).
- Owner-compatible: plain, normal and tilted at every seat; rank zero overlap, least overlap, support. Normal-first search order OK; normal-preferred regardless of support is not. K must be a real safe boundary.

## 4. Routing
- Down-sweep routing reads live arm paths and landings (down_sweep.rs:2818–2835, 3214–3270); per-event trigger needs a prior routing pass (3770–3799). Build provisional complete routes/landings before load analysis, then certify.
- One resample wave can under-cover; emit legal geometry + precise deficit; not success. Shrink is not coverage-monotone; freeze only after sizing and coverage certification (second_contact.rs:19–34). Reservations at r_cap can block jointly smaller families; prove envelopes.

## 5. Before deleting movers
Lip/graze/head joins → construction alternatives with certificates. Growth/second-contact → retained-area measurement, extra seats when short (contact_growth.rs:8–16, 28–39, 67–76); keep second_contact read-only checks. Slides/normal rewrite → final face facts before admission; keep birth/ground coverage (lib.rs:5681–5717, 5766–5777). Feet → full foot geometry admission (model_feet.rs:56–95). Bore relocation: keep until construction meets the bore hard limit (drain.rs:450–492; lib.rs:5747–5760).

## 6. Revised plan
- A0: flat contract fixtures: reach vs capacity allocation separated, identical trial/commit, production-router catchment check; reuse cr1 cap + writer fixtures; light-load, raster phase/edge/greedy cases, late-mutation witness. No model runs.
- A: discs only. Capacity-aware placement; cap ownership through the pipeline (later edits cannot invalidate accepted contacts); no broad deletions. Three pinned disc runs, ≤1 named corrective rerun each. Accept: file CS=FU=0, overload/uncarried/residual 0, arms ≤1.3× run-derived bound, down passes ≤4; report time/resin and excess-arm source. Miss → diagnose same trace, stop before B.
- B: constructor with provisional routes, complete seat/approach/foot candidates, retained area, reservations, drain clearance, coverage; replay loads, finalize smallest sufficient, certify, freeze. Fixtures then one Deinonychus run.
- C: delete repair paths one responsibility at a time with path-hit witness + sabotage; drained model before bore relocation removal; landing gate once.
## 7. Falsifier fixes
No "CS=0 OR Unresolved": over-cap is never legalized. Sabotages: cap → CS; reservations → FU; capacity pitch off → overload/insertion/deficit. Add light-load, weighted multi-event, edge/raster-shift, concave neighbour near a normal stub, thin ridge, close small feet, late mutation and removal, bore/rim/drained. Stable seat/face/event provenance; full subject/severity comparison.
