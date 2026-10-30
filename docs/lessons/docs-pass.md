# plinth-3d/docs pass (2026-10-29): every file, KEPT or SKIPPED

Rule: keep documents that are factual (printer/resin data, coupon results, measured data, format facts, owner print feedback); skip design documents for the old implementation, plans, status, lane notes, logs and patches. KEPT files are copied verbatim into `reference/` (see `/MANIFEST.tsv`). The old repo is deleted after this pass, so SKIPPED means lost except through the git bundle.

| Status | File (under old plinth-3d/) | Reason |
|---|---|---|
| SKIPPED | `docs/.DS_Store` | finder junk |
| SKIPPED | `docs/architecture.md` | old implementation design |
| SKIPPED | `docs/datasheets/.DS_Store` | finder junk |
| KEPT | `docs/datasheets/printers/README.md` | vendor/printer datasheet |
| KEPT | `docs/datasheets/printers/saturn-4-ultra-16k.md` | vendor/printer datasheet |
| KEPT | `docs/datasheets/README.md` | vendor/printer datasheet |
| SKIPPED | `docs/datasheets/resins/.DS_Store` | finder junk |
| KEPT | `docs/datasheets/resins/elegoo.md` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/nova3d.md` | vendor/printer datasheet |
| SKIPPED | `docs/datasheets/resins/pdfs/.DS_Store` | finder junk |
| KEPT | `docs/datasheets/resins/pdfs/anycubic-abs-like-v2-tds.pdf` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/pdfs/anycubic-water-wash-abs-like-3.0-tds.pdf` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/pdfs/esun-water-washable-tds.pdf` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/pdfs/inslogic3d-water-washable-abs-like-tds.pdf` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/pdfs/phrozen-water-washable-tough-user-guide.pdf` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/pdfs/siraya-simple-v2-tds.pdf` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/RESIN-ENVELOPE.md` | vendor/printer datasheet |
| KEPT | `docs/datasheets/resins/sunlu.md` | vendor/printer datasheet |
| SKIPPED | `docs/development.md` | old dev procedure/workflow |
| SKIPPED | `docs/engineering-journal/03-physics-and-constants.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/04-migration-plan.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/06-conventions.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/constants.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/decisions/2026-07-28-rust-rewrite.md` | old rewrite decision record |
| KEPT | `docs/engineering-journal/decisions/2026-08-12-owner-rulings-fu-batch.md` | owner rulings 2026-08-12 |
| SKIPPED | `docs/engineering-journal/decisions/README.md` | index/readme of old journal |
| SKIPPED | `docs/engineering-journal/fu8f-g6-design-2026-08-07.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/fu8g-design-2026-08-08.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/m0-constants-source-ledger.json` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/README.md` | index/readme of old journal |
| SKIPPED | `docs/engineering-journal/reference/acceptance-run-2026-07-24-contactfix.md` | old acceptance fixture derivation |
| SKIPPED | `docs/engineering-journal/reference/physics_transient_peel.md` | audit of old code constants |
| KEPT | `docs/engineering-journal/reference/plate-01-results.md` | coupon plate results |
| KEPT | `docs/engineering-journal/reference/plate-02-and-03-status.md` | coupon plate results |
| KEPT | `docs/engineering-journal/reference/plate-04-results.md` | coupon plate results |
| KEPT | `docs/engineering-journal/reference/plate-05-results.md` | coupon plate results |
| KEPT | `docs/engineering-journal/reference/plate-06-results.md` | coupon plate results |
| SKIPPED | `docs/engineering-journal/reference/README.md` | index of plate records |
| SKIPPED | `docs/engineering-journal/specs/SPEC-EMBED-POLICY.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/specs/SPEC-P8-RELOCATE.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/specs/SPEC-V5-PERF.md` | old design/constants registry/specs |
| SKIPPED | `docs/engineering-journal/support-resource-caps-2026-08-05.md` | old design/constants registry/specs |
| SKIPPED | `docs/evidence/audit-baseline-2026-08-30.md` | old tool run on plinth outputs, not physics |
| SKIPPED | `docs/evidence/audit-baseline-hardened.md` | old tool run on plinth outputs, not physics |
| SKIPPED | `docs/evidence/audit-baseline-round3.md` | old tool run on plinth outputs, not physics |
| SKIPPED (copy kept) | `docs/evidence/chitubox-support-profiles-2026-09-30.cfgx` | byte-identical duplicate of the Chitubox Projects cfgx (kept there) |
| SKIPPED | `docs/evidence/derivations/low-point-reach-2026-08-19.md` | derivation for old engine criterion |
| SKIPPED | `docs/evidence/emission-canary.md` | ledger of old engine output values |
| SKIPPED | `docs/evidence/geometry-consolidation-result.md` | old implementation result record |
| KEPT | `docs/evidence/geometry-consolidation/triage-verdict-table-after-c5.md` | measured mesh classification of 21 models |
| KEPT | `docs/evidence/geometry-consolidation/triage-verdict-table-before-c5.md` | measured mesh classification of 21 models |
| SKIPPED | `docs/evidence/geometry-fix-result.md` | old implementation result record |
| KEPT | `docs/evidence/literature/auto-support-traction-force-constrained-optimization.pdf` | literature notes and primary PDFs |
| KEPT | `docs/evidence/literature/effects-of-curing-on-photosensitive-resins-sla.pdf` | literature notes and primary PDFs |
| KEPT | `docs/evidence/literature/lit-green-peel.md` | literature notes and primary PDFs |
| KEPT | `docs/evidence/literature/LITERATURE-NOTES.md` | literature notes and primary PDFs |
| KEPT | `docs/evidence/literature/mechanical-property-modeling-degree-of-cure.pdf` | literature notes and primary PDFs |
| KEPT | `docs/evidence/literature/separation-force-vs-speed-and-cross-section-fep-pp.pdf` | literature notes and primary PDFs |
| KEPT | `docs/evidence/literature/towards-scarless-support-removal-vat-photopolymerization.pdf` | literature notes and primary PDFs |
| SKIPPED | `docs/evidence/physics_contacts_tips.md` | audit of old code constants |
| SKIPPED | `docs/evidence/physics_coverage.md` | audit of old code constants |
| SKIPPED | `docs/evidence/physics_load_sizing.md` | audit of old code constants |
| SKIPPED | `docs/evidence/physics_printability.md` | audit of old code constants |
| KEPT | `docs/evidence/plate-06-verification-physicist.md` | plate 6 verification panel record |
| KEPT | `docs/evidence/plate-06-verification-protocol.md` | plate 6 verification panel record |
| KEPT | `docs/evidence/plate-06-verification-refutation.json` | plate 6 verification panel record |
| KEPT | `docs/evidence/plate-generator-manifest.json` | only geometry record of plates 1-4 |
| KEPT | `docs/evidence/plate-generator-PLATE-LAYOUT.md` | only geometry record of plates 1-4 |
| SKIPPED | `docs/evidence/remediation-2026-08-26/audit-constants.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/baseline-per-binary.txt` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/baseline-red-diagnosis.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/baseline.sh` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/build-cost.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/census/cast-audit.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/census/census-clippy.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/census/census-dead-code.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/census/census-docs-drift.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/census/census-duplication.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/census/census-inventory.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/census/census-test-shape.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/doctrine-in-force.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/EVIDENCE-treant-runaway.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/FIX-AFTER-RUN.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/gate-port.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/HANDOFF-kernel-to-rewire.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/incremental-integrity.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/load-model-design.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/patches/kn-fixture-repairs.patch` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/patches/LOST-drain-banding-drilling.patch` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/PLAN.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/README.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-cap-evidence.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-dust_shelf.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-dust_shelf.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-overhang_table.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-overhang_table.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-raking_blade.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-raking_blade.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-toed_slab.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-toed_slab.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-uncovered_rim_table.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/ctl-uncovered_rim_table.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-dust_shelf.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-dust_shelf.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-overhang_table.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-overhang_table.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-raking_blade.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-raking_blade.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-toed_slab.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-toed_slab.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-uncovered_rim_table.log` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-ranking-veto-measurement/veto-uncovered_rim_table.trials.json` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-regression-analysis.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/repair-search-diagnosis.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/REVIEW.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/SALVAGE-RECOVERY.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/STATUS.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/remediation-2026-08-26/testing-doctrine.md` | old code review/plan/logs/patches |
| SKIPPED | `docs/evidence/repair-cap-evidence.md` | old repair-search tuning measurements |
| KEPT | `docs/job-manifest.md` | list of minis to print |
| KEPT | `docs/physics.md` | owner rulings + measured boundary (mixed with old-engine rules) |
| SKIPPED | `docs/plans/bracing-and-shedding-2026-09-06.md` | plan for old implementation |
| SKIPPED | `docs/plans/coverage-repair.md` | plan for old implementation |
| SKIPPED | `docs/plans/geometry-consolidation.md` | plan for old implementation |
| SKIPPED | `docs/plans/hollow-first-2026-10-16.md` | plan for old implementation |
| KEPT | `docs/plans/home-support-calibration-p1-protocol.typ` | H2 preregistered coupon protocol (only H2 design record) |
| SKIPPED | `docs/plans/load-model-implementation-plan.md` | plan for old implementation |
| SKIPPED | `docs/plans/load-model-physics-first.md` | plan for old implementation |
| SKIPPED | `docs/plans/load-model-synthesis.md` | plan for old implementation |
| SKIPPED | `docs/plans/load-model-whole-pipeline-coherence.md` | plan for old implementation |
| SKIPPED | `docs/plans/ratchet-metrics.md` | plan for old implementation |
| SKIPPED | `docs/plans/slice-sweep-core-2026-09-02.md` | plan for old implementation |
| SKIPPED | `docs/plans/support-calibration.md` | plan for old implementation |
| SKIPPED | `docs/plans/support-clearance.md` | plan for old implementation |
| SKIPPED | `docs/plans/support-load-strictly-empirical.md` | plan for old implementation |
| SKIPPED | `docs/remediation-tracker.md` | status/tracker of old work |
| SKIPPED | `docs/specs/audit-connectivity-rule.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/audit-contract.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/audit-interval-policy.md` | spec/design of old implementation |
| KEPT | `docs/specs/audit-invariants.md` | independence principles (values obsolete) |
| SKIPPED | `docs/specs/audit-known-good.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/audit-span-criterion.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/audit-spread-criterion.md` | spec/design of old implementation |
| KEPT | `docs/specs/chitubox-format.md` | ChituBox format facts |
| KEPT | `docs/specs/chitubox-io.md` | ChituBox format facts |
| SKIPPED | `docs/specs/coverage-repair.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/embed-policy.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/geometry-adjudication.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/geometry-canon.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/kernel-geometry.md` | spec/design of old implementation |
| SKIPPED | `docs/specs/seal-semantics.md` | spec/design of old implementation |
| SKIPPED | `docs/status.md` | status/tracker of old work |
