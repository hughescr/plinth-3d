Remnant of the engineering journal, reduced to what code still reads at a hard-coded path
(`06-conventions.md`, `constants.md`, `m0-constants-source-ledger.json`,
`03-physics-and-constants.md`, `support-resource-caps-2026-08-05.md`,
`specs/SPEC-EMBED-POLICY.md`) plus `decisions/`, `reference/`, and `specs/` — current status
and plans now live under [`../status.md`](../status.md) and [`../plans/`](../plans/).

Two of those are frozen-path citations from `crates/plinth-core/src/constants/evidence.rs`
(mirrored by `crates/plinth-core/tests/constants_evidence.rs` and
`crates/plinth-core/tests/scar_area_cap_policy.rs`), so their path cannot move without a code
change:

- `support-resource-caps-2026-08-05.md` is the full record — `docs/evidence/` has no copy.
- `specs/SPEC-EMBED-POLICY.md` is a 3-line stub; the authoritative text lives at
  [`../specs/embed-policy.md`](../specs/embed-policy.md).
