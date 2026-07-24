# Phase-8 frozen acceptance fixture — re-derived at HEAD (2026-07-24)

**Branch:** `v5-perf` @ `e023b0e` (minima contact fix: filter A
`minima_drop_interior_faces` + refined filter B `minima_require_island_birth`
AND `minima_support_area_tips`).
**Supersedes:** `../acceptance-run-2026-07-23/phase8_relocation_input.json`.

The predicate is **unchanged** — SPEC-EMBED-POLICY SCAR-AREA-CAP
(`A_ABS_MM2 = 2.0`, `KAPPA = 1.25`, `L_SCAR_MAX_MM = 2.5`), rule-4
through-transit, the PRINT-FLOOR moot downgrade (strict `chord < 0.08 mm`, moot
bucket separately auditable, hard gate fails only on non-exempt), and the
quarantine REAL/ARTIFACT regrade. Only the **population** it is applied to moved:
the contact fix removes 195 of 753 contacts and re-indexes every downstream
member, so every member id in the 2026-07-23 snapshot is invalid here.

## Derivation chain (identical to the original, same tools, same order)

| Step | Tool | Output |
|---|---|---|
| 1 | `../acceptance-run-2026-07-23/run_v5.py cfix1` — default cfg `SupportConfig(lift_mm=3.0)`, `p8_relocate_enable=False`, i.e. the **pre-relocation Phase-8 INPUT** geometry | `scratchpad/v5p_treant_cfix1.pkl`, `results/run_results.json` |
| 2 | `../acceptance-run-2026-07-23/census_run.py` (independent Python census, never calls the kernel), `CENSUS_STATE` = that pickle | `results/penetration_census.json`, `results/census_summary.json` |
| 3 | `../acceptance-run-2026-07-23/p11_crossexam.py` (engine's ratified verify path + three independent inside-oracles) | `results/p11_regrade_ratified.json` |
| 4 | assemble | `phase8_relocation_input.json` |

The moot/non-exempt split is read off the **census's own full-precision `moot`
flag**, not the 4-decimal JSON, so the mi=420-class rounding artifact that forced
the 2026-07-24 CORRECTION on the old snapshot cannot recur. No member's worst
firm chord sits within 1e-4 mm of the 0.10 mm band edge either.

## Provenance

| | value |
|---|---|
| `state_pkl` | `v5p_treant_cfix1.pkl` |
| `state_md5` | `762b6679fa53b52061967237d97653cb` |
| members `combined_sha` | `cceeac80d4196b9202571e6dd9f9c2a99794ed5581fa4df911fefd189e220d0e` |
| pre-relocation geometry digest (`TREANT_PRERELOCATE_DIGEST`) | `fc3fb20add04cbb12136a2adc4ff9191a73c984a69a66b54013020dc2efbe79c` |
| forest | 1226 members, 877 balls, 558 contacts, 39 quarantine sites |

## Partition — new vs superseded

| bucket | 2026-07-23 | 2026-07-24 |
|---|---|---|
| non-exempt (hard gate) | 143 | **72** |
| moot (report-only) | 50 | **37** |
| quarantine REAL | 77 | **39** |
| quarantine ARTIFACT | 3 | **0** |
| ball breaches | 15 | **7** |
| illegal members (total) | 193 | 109 |
| firm ≥ 0.10 / band 0.08–0.10 | 118 / 25 | 53 / 19 |
| relocation target set | — | 105 = \|72 ∪ 39\| (=107, 4 in both) − 2 Q-REAL read moot |

## What moved, and why

Correspondence is **geometric**, not by index: members are keyed by
`(p0, p1, r0, r1, kind, embed_end)` rounded to 1e-4 mm. 928 of the old 1476
member geometries survive.

- **Old non-exempt 143** → 81 member geometries **gone**, 62 survived, and
  **all 62 are still non-exempt**. Zero flips.
- **Old moot 50** → 17 gone, 33 survived, and **all 33 are still moot**. Zero
  flips.
- **New non-exempt 72** = those 62 survivors + 10 members with no surviving old
  counterpart; **new moot 37** = 33 + 4 likewise. The novel members are
  **re-routed geometry** — removing contacts changes rewire/brace routing — not
  newly-broken old members.
- **Quarantine 80 → 39:** all 39 are geometric survivors of the old 77-REAL set;
  **no site changed verdict and no new site appeared**. The ARTIFACT bucket is
  empty because the old trio 437/440/444 (single tip-anchored LEGAL deep plugs)
  lost their contacts — not a regrade flip.
- **Ball breaches 15 → 7:** only 4 of the old 15 breaching balls survive
  geometrically and all 4 still breach; the other 3 are novel geometry. Engine
  gate `ball_breach_violations` = 7 = census `n_breaching_balls` (R3 parity
  holds on the new state).

So the drop in every bucket is **removal, not reclassification**: the contact fix
deleted the contacts whose stubs were transiting, and nothing that survived was
re-judged.

## Relationship to the superseded snapshot

The 2026-07-23 file is retained **unchanged**. It is the regression pair for the
archived forest `v5e_run1.pkl` and is still exercised by
`scripts/tests/test_phase8_driver.py` (snapshot cross-check, quarantine regrade)
and `scripts/tests/test_p11_fixture.py`; it loads through the same
`phase8.load_frozen_snapshot()` via `phase8._ARCHIVED_SNAPSHOT_2026_07_23`.

`phase8_driver.TREANT_PRERELOCATE_DIGEST` moves with the snapshot, so **neither
snapshot can ever be applied to the other's geometry** — a mismatched digest
falls back to live-gate target selection. That is pinned by
`test_superseded_archived_forest_falls_back_to_live_gate`.
