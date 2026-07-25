# Retired tools — P11 cross-examination (tombstone)

**Date:** 2026-07-24 · **Ruling:** P11 fixture ruling 1A (settled) · **Branch:** v5-perf

The P11 condemned-stub cross-examination was re-derived under the owner-ratified
predicate. The two tools below ran the **retired** CD+PB arc-inset window at
`h = 2.0` (contact_depth + pitch_bound + r + standoff on a dense ParityVote),
which is NOT the ratified embed policy. Their outputs were **stale-tool
artifacts** and have been removed to prevent them being mistaken for ground truth.

## What was retired

| Retired | Fate | Reason |
|---|---|---|
| `p11_crossexam.py` (arc-inset, h=2.0) | **Refreshed in place** to the ratified predicate | Graded 80 sites **55 REAL / 25 ARTIFACT** — the arc-inset over-exempted; superseded by SCAR-AREA-CAP + rule-4 + PRINT-FLOOR |
| `results/p11_crossexam.json` | **Deleted** | Stale 55/25 output |
| `results/rerun_p11_crossexam.json` | **Deleted** | Stale 55/25 output (rerun copy) |
| `validate_r4_embed_inset.py` (untracked) | **Deleted** | Stale labels: `LABELED_ART=[3,223,232]` and a `flagged==55` target. Under the ratified predicate **223/232 flip to REAL** (buried-root re-entry, three-oracle agreement) and the count is 77, so every assertion in it was wrong. Its purpose (corrected-inset == census REAL) is subsumed by the ratified predicate + `test_p11_fixture.py`. |

## Ratified successors (canonical)

- **Tool:** `p11_crossexam.py` — now runs the engine's ratified verify path
  (`geometry_exact.penetration_intervals` + `verify._embed_scar_verdicts`
  (SCAR-AREA-CAP) + `verify._transit_classify` (rule-4 + PRINT-FLOOR moot floor))
  and cross-checks three independent inside-oracles. Deterministic; RSS < 1.6 GB.
- **Result:** `results/p11_regrade_ratified.json` (a run output, archived outside
  git under `<project>/artifacts/audit-2026-07/`) — **77 REAL / 3 ARTIFACT**
  (ARTIFACTs `437/440/444` = single tip-anchored LEGAL deep plugs).
- **Phase-8 input:** `scripts/tests/fixtures/phase8_relocation_input_2026_07_23.json`
  (moved out of this folder; it is a runtime fixture) — the 77 relocation targets +
  the census 193 print-floor partition (**144 non-exempt / 49 moot**).
- **Fixture (ruling 1A):** `scripts/tests/fixtures/p11.json` — depth-aware ground
  truth, labels `{ARTIFACT: 3; REAL: 8,223,232,253,297,316,323,325,333}`.
- **Regression tests:** `scripts/tests/test_p11_fixture.py`.

## Ruling 1A notes

- **mi=3 stays ARTIFACT** as a *documented* known rule-4 shallow-graze false
  positive: a 0.406 mm-arc surface graze only 0.0574 mm deep. The RAW ratified
  predicate grades it REAL (it is in the Phase-8 relocation set); the fixture
  overrides it to ARTIFACT as depth-aware GT. Predicate refinement (a depth floor
  to complement the chord-based print-floor) is **deferred to the placement round**.
- **mi=3 × PRINT-FLOOR:** the moot floor is CHORD-based (`feature_thickness <
  0.08 mm`) and is **orthogonal** to mi=3's shallow-DEPTH disqualifier. mi=3 has
  five unanchored illegal crossings with chords `{0.063, 0.177, 0.106, 0.406,
  0.242} mm`; only one is sub-floor, so the member is **not** all-moot and stays
  NON-EXEMPT. The print-floor does not express the owner's depth intuition.
