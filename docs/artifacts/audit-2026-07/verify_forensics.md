# Adversarial Verification — Unrouted-Contact Forensics (Weeping Treant v1.2)

**Verifier verdict: PASS (not a blocker).** Every claim the forensics deliverable
actually makes is pickle-derived, and all of it reproduces to the digit against an
independent recompute written from scratch (no reuse of `forensics.py`). The
mesh-dependent four-class classification was **honestly marked BLOCKED** in the
deliverable and is genuinely unverifiable in this environment — see §4.

Source of truth: `/Users/craig/.claude/jobs/0ddf6d41/tmp/engine_run_state.pkl`
(the engine's own `engine.run()` output per `run_engine_full.py`). My probes:
`/Users/craig/.claude/jobs/0ddf6d41/tmp/verify_{probe,recompute,final}.py`.

---

## 1. The STL blocker is REAL and reproducible (not a forensics excuse)
- `~/Documents/.../Weeping Treant v1.2.stl` and its scratchpad symlink both raise
  `PermissionError: [Errno 1] Operation not permitted` on `open()` — **with the
  sandbox on AND with `dangerouslyDisableSandbox: true`**. Confirmed independently.
- No readable copy exists (the requested `/Users/craig/.claude/jobs/0ddf6d41/tmp/treant.stl`
  was never dropped; every scratchpad STL path is a symlink into `~/Documents`).
- Consequence: I, like the forensics agent, **cannot** run the mesh-based
  INTERNAL (ray-visibility) / GROOVE (clearance-envelope) / REACH
  (candidate-regeneration) tests, nor resolve the 195 individual contact
  positions, nor locate the 144 islands. This half was never producible here.

## 2. Pickle contents independently confirmed
195 unrouted · 28 unsupported_minima · 21 uncovered_island_births · 144
uncovered_islands · plate_z = −66.9757 · 1625 members · 1102 balls. All match the
deliverable `counts` block exactly.

## 3. Independent recompute of the 49 residuals — EXACT match
Recomputed nearest-live-member (point-to-segment over all 1625 members, minus
member radius) and nearest-ball surface distance for every residual with my own
geometry code:

- **Top-10 cross-check: max error = 0.0** on both `nearest_member_surface_mm` and
  `nearest_ball_surface_mm` across all 10 rows.
- **Redundancy tally reproduces exactly:** ≤0.5mm = **19**, 0.5–1 = **9**,
  1–2 = **15**, >2 = **6** (deliverable: 19/9/15/6). ✓
- **Births:** 15/21 within 0.5mm, all 21 within 2mm. ✓
- **Minima:** 22/28 within 2mm of a live member, **6/28 >2mm** (the unsupported
  tail). ✓ min_gap range 1.10–10.65mm, median 2.22. ✓
- Distribution checks: birth areas 0.001–0.054 mm² (median 0.0045); birth
  gap_layers 3.5–49.2 (median 13.25) — all reproduce. ✓

### Severity ranking — reproduced and defensible
- Deliverable severity list is **monotonic non-increasing** ✓.
- Minima severity ≈ `min_gap × nearest_member_surface` (exact for #2; a small
  extra term on some rows, maxerr 2.5 — does not change ordering).
- Birth severity = `area × gap_layers × f(nearest_member_surface)` where f grows
  with member distance (the documented "discount when a member is essentially
  touching"). This is why the crown-tip birth at (−7.64, 9.99, 52.6) — tiny
  0.0034 mm² but 49-layer float, member 1.87mm off — ranks above the larger-area
  births at (35.52,…) and (9.32,…) that a stub nearly touches. **Defensible, not
  an error.**
- **No top-10 item is wrong:** none is fabricated, none dropped, and the two the
  prose flags as the only non-trivial risks — #1 (largest-footprint birth,
  0.054 mm²/27.5 layers) and #2 (the genuinely stranded 10.65 mm overhang at
  (32.1,−12.1,43.7), nearest support 10.09 mm) — are both correct and correctly
  called out for manual support. The #1-vs-#2 numeric ordering (a 0.054 mm² speck
  scored above a 10.65 mm stranded overhang) is an **arguable** cross-metric
  comparison, but both are surfaced as the top concerns, so it does not mislead.

## 3b. The 5 most-dangerous island births — layers-to-merge re-measured
`gap_layers` is the engine's own verify output copied faithfully; re-derived
independently from the pickle `gap_mm` at the confirmed 0.05 mm layer height:

| birth (x,y,z) | deliverable layers | pickle layers | gap_mm |
|---|---|---|---|
| (−37.56, 12.44, −20.00) | 27.53 | 27.53 | 1.376 |
| (17.69, 12.59, 23.55) | 39.80 | 39.80 | 1.990 |
| (−7.64, 9.99, 52.60) | 49.15 | 49.15 | 2.458 |
| (35.52, −1.22, −61.55) | 28.81 | 28.81 | 1.440 |
| (9.32, −12.90, 1.85) | 27.98 | 27.98 | 1.399 |

All exact; layer height 0.05 mm confirmed (gap_mm/gap_layers = 0.0500 on every row). ✓

## 4. The four-class classification — nothing to overturn
The task asked to sample ≥20 contacts across INTERNAL / GROOVE-BLOCKED /
OPEN-BUT-UNREACHED / EFFECTIVELY-REDUNDANT and re-derive each class. **This is not
possible and, critically, was never delivered:** `unrouted_forensics.json` contains
`meta/counts/redundancy_tally/top10/all_residuals_ranked` only — **no per-contact
four-class field.** The deliverable explicitly lists INTERNAL/GROOVE/REACH under
`BLOCKED_mesh_tests`. Therefore:
- **Class-disagreement rate = N/A** (0 contacts were classified; there is nothing
  to disagree with). On the metric that matters — accuracy of what was actually
  claimed — the disagreement rate is **0%** (every claim reproduced).
- The deliverable did **not** overclaim: it did not present unverified mesh
  classifications as done. That is the correct, honest posture.

## 5. Caveats to carry forward (limitations, not defects)
These are disclosed in the forensics md but worth flagging to Craig:
1. **"Mostly groove/neighbor-held (redundant)" rests on a proxy** — nearest-member
   distance. A strut within 0.5mm does not *prove* it supports that specific
   downface (it may pass nearby unrelated). "Low risk" is a reasonable inference,
   not a mesh-confirmed fact.
2. **"Not internal parts" is inferred** from tiny birth areas, not a ray test.
   Reasonable, but the airtight INTERNAL test remains unrun.
3. **The 144 uncovered islands** include a few with large `n_faces` (up to 22467).
   The md concedes these *could* be broad genuine overhangs rather than
   grown-from-speck detail — and distinguishing them **needs the mesh**. This is
   the one place a real risk could hide that the pickle-only analysis cannot rule
   out. If the mesh ever becomes readable, this is the first thing to run.

## Bottom line
- Pickle-derived forensics: **fully verified, 0 numeric disagreement, no mis-ranked
  or dropped top-10 item.** NOT A BLOCKER.
- Mesh-derived four-class half: never produced (honest BLOCKED), and
  independently confirmed unproducible here (STL EPERM with sandbox on and off).
- Craig's practical guidance stands: manually support the crown-tip cluster
  (z≈+53, right/front) and the single 10.65 mm stranded overhang at
  (32.1,−12.1,43.7); the rest is micro-detail either merging within ~1–2mm or
  already touched by a neighbor. Treat "held/redundant" and "not internal" as
  proxy-based conclusions pending a readable mesh.
