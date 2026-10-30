# Lessons challenge (gpt-astra high, 2026-10-29) — orchestrator copy of the handback
Verdict: HOLD deletion until the data omissions below are copied or their loss is explicitly accepted.

## 1. Factual errors / overstatements in LESSONS.md
- F1: "no print" is absence of a record, not confirmation; distinguish generated miniature supports from generated calibration geometry; Q6 pending.
- F2: ChituBox "meets limits by construction" is unverified (profile settings ≠ placement algorithm); RETHINK-CHALLENGE: normal stubs are candidates; "physics needs ~223" is a model-derived bound.
- F3: tiny-island rule = 25 pixels in EITHER dimension, limited growth before joining, and unsupported islands provide NO support (rulings-2026-10-20-islands-knees).
- F4: contact depth is not a design parameter: required rasterized contact AREA governs; truncate, don't retract (contact-area-over-seat-depth). Open: does ChituBox reconstruct a clipped ball.
- F5: 0.10 mm walls resolved; smaller untested — not a universal minimum feature.
- F6: Y-position: memory says ≤1 rung and keep a hook (don't hard-code absence); plate-04-results §6 says nothing about Y (confounded). Record discrepancy.
- F7: H2 X=9 losses: no wash-basket/vat census; alternative bottom-step convention; probable one-layer flange defect.
- F8: plate 5 conditions unrecorded; plate 6 printed 2026-07-30, scored 07-31, ~30 °C, black resin no vendor. Sharing at least additive in tested geometry; no strip upper bracket; ~20% fin-root bonus is a confound.

## 2. Missing knowledge/data (A copy, B summarize, C drop after summary)
- A essential: plinth-3d/docs/evidence/plate-generator-manifest.json + plate-generator-PLATE-LAYOUT.md (only surviving geometry record of plates 1–4).
- A: docs/engineering-journal/reference/plate-01,04,05,06-results.md + plate-02-and-03-status.md (complete tables, verbatim).
- A+B: memory h2-plate07-results.md; raw H2 matrix never committed (in plinth-triage workflow output) — recover or label summary-only.
- A if recoverable: plate photos in session 0010a938-3a51-4e43-8d1c-2db9590b4417; plate-4 analysis lost.
- A+B: docs/datasheets/resins/sunlu.md, RESIN-ENVELOPE.md, pdfs/ (vendor properties are post-cured, not first-peel).
- A+B: docs/evidence/literature/LITERATURE-NOTES.md + five primary PDFs (green dogbone vs just-exposed layer); lit-green-peel.md is weaker.
- B: campaign setup (anti-aliasing, 5 transition layers, film) as historical conditions.
- B: departure-rule-2026-10-21, supports-may-seat-on-model (explicit).
- B: plinth-mesh repair.rs census + geometry-consolidation/triage-verdict-table-after-c5.md (21-model scope, point-touch geometry legit, no keep-largest).
- B: perf hf/DESIGN.md, RESULTS.md (score drilled shell; metadata across booleans; memory bound across parallel poses; per-pose hollowing is implementation, not physics).
- B: lessons-tech §2 + profile5/PROFILE5.md methods (batched queries, sparse data, negative caching, deterministic reductions; counters changed timings; tiny parallel tasks slower).
- A+B: docs/specs/chitubox-format.md, chitubox-io.md, scripts/tests/fixtures/chitubox/README.md (unknown fields, pointer chains, association, baked transforms, open soups, pad/bridge winding, edit-save-reopen-slice tests; pad dims observed, not proven limits).

## 3. Native provenance
Six fixtures (two-supports, basic-cube-supports-raft, cylinder-supported-bottom-connected, morel, morel_mudskipper_supports, empty) explicitly native per README. Chitubox Projects/cylinder-cube-sphere.chitubox corroborated (m0-constants-source-ledger.json). The .ctp has no provenance note — ask Craig. cfgx byte-identical to repo copy.

## 4. Quarantine leaks
- Section 5 numbers (crate count, 1,500 lines, 8 args, 5 switches, ≤11 report lines, timing tiers) are PROPOSALS, not owner requirements.
- D21 f64/f32 is an old choice; lesson is error control + reproducibility. D2 "0 repair passes" → no late mutation that invalidates guarantees (input-mesh repair allowed).
- corpus/manifest.json carries old goals (fewer cavities better, Python pipeline): keep names/digests only. audit-invariants.md declares X=12 and seating depth: keep independence principles only.
- cfgx is evidence of settings, not of unedited auto-support success.
- X [12,18] is two decisions, not a confidence interval; 45° convention ≠ logistic.
- "Closed cavities" → "inside drained cavities"; drainage/venting not waived.

## 5. Interview changes
Q6 early, allow "unknown", ask for files/photos/setup/edits. Q10 → how to measure the 0.8 mm mark on irregular contacts, why it matters, stricter surfaces (the 10-28 ruling stands). Q11 → demonstration, not mm. Q9/Q12 leading → ask what uncertainty blocks the next real print and how much calibration he wants. Q16 assumes Heavy. Q21 false binary. Q25 → delegated authority. Add: acceptance lifecycle (slicing, detail, removal tools/time, scar standard by face, wash/cure/drain, trapped resin, damage before/after handling); input behaviour (units, pre-hollowed/pre-supported, multi-part kits, as-drawn marker, ChituBox version, supports individually editable); priority when constraints conflict; partial output; speed target vs gate.

## 6. Exit check
File-level copy manifest with hashes, destinations outside the deletion set; expand shorthand. Bundle doesn't preserve dirty worktree content — inspect worktrees' uncommitted state.
