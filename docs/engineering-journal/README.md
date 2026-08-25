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

**Two of `evidence.rs`'s narrative strings cite retired documents.** The constants registry
(`crates/plinth-core/src/constants/evidence.rs`) is frozen by the constants provenance ruling —
its prose is not edited to chase a doc reorganization. Two of its citations name documents this
consolidation retired: the `MAX_DECIMATION_DEVIATION_MM` entry cites
`docs/engineering-journal/decimation-design-2026-08-06.md` §8.3, and the
`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` entry cites `docs/engineering-journal/CURRENT-STATUS.md`'s
follow-ups ledger. Neither document survives in the tree; both records live in git history only.
The frozen strings are left exactly as written rather than repointed.
