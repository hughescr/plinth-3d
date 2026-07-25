# Reference — durable written findings

Analyses that are still true and still worth reading. They are **not**
normative: `docs/specs/` holds the rules we ratified and that code cites as
law; this directory holds the reasoning and the measurements several of
those specs were built on.

- `penetration_census.md` — exact-geometry census of illegal model crossings
  in the delivered forest. `SPEC-V5-GEOM.md` names it as its authority for
  the v5 exact-geometry core.
- `port_plan.md`, `port_plan_codex_review.md` — the Rust-kernel port strategy
  and its independent adversarial review. `SPEC-RUST-KERNEL.md` cites both as
  its own basis.
- `physics_contacts_tips.md`, `physics_load_sizing.md`,
  `physics_printability.md`, `physics_transient_peel.md` — the four physics
  audits of the sizing model. `SPEC-EMBED-POLICY.md` cites the first as
  evidence.
- `TREANT-V4-PRINT-AUDIT.md` — the synthesis report over the resin research,
  literature review, and physics audits, plus its completion,
  `AUDIT-ADDENDUM-mesh-classification.md` (what the 195 unreachable contacts
  actually were).
- `RETIRED-TOOLS.md` — why the retired audit tools were retired. Referenced
  by `scripts/tools/p11_crossexam.py`.
- `acceptance-run-2026-07-24-contactfix.md` — how the live Phase-8 acceptance
  fixture (`scripts/tests/fixtures/phase8_relocation_input.json`) was
  derived, and in what order.

The underlying run outputs these were computed from — result JSONs, censuses,
CPU profiles, determinism/parity comparisons, screenshots — are **not** in
git. See `../datasheets/README.md`.
