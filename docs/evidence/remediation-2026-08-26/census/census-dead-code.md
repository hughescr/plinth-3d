# Dead/orphaned code census — plinth-3d

Method: `grep -rn "dead_code|unused"` across `crates/` and `rust/` (excluding
`target/`), then for each hit read the guarded item and traced every call site
across the workspace (crate-internal callers, `cargo`-visible re-exports in
`lib.rs`, and downstream crates via `grep -rn plinth_support::`/etc.) to decide
if the item is reached from `plinth-cli`'s actual pipeline path. For
`legacy_compat.rs` and `placeholders.rs` specifically, traced every symbol
individually. Ran targeted `grep`, no full recompile (clippy.txt/pedantic
already supplied and clean/unrelated to dead-code). Did not exhaustively
cross-reference every `pub` item in the four largest crates (plinth-hollow,
plinth-support, plinth-metrics, plinth-mesh — 130k+ lines combined); scope was
prioritized per the task's explicit hot spots (items 1 and 4) plus items 5–7.

## Headline finding: systemic stale `#[allow(dead_code, reason=...)]` in plinth-support

`crates/plinth-support/src` carries 75 dead_code-related allow sites (grep
count). Every one of the ~10 I traced end-to-end turned out to be **already
live**, called from genuinely non-test production code — not from the "future
support runner"/"pending controller" the `reason=` string names. The whole
crate was built module-by-module behind these allows before `runner.rs` (the
orchestrator, `pub fn run_support`, re-exported at `lib.rs:172`) was wired in;
`run_support` is now called from `crates/plinth-pipeline/src/lib.rs` and
`prepare.rs` (grep-confirmed), so the modules it reaches are live, but nobody
removed the allows once wiring landed.

Verified LIVE despite "pending" reason text:
- `crates/plinth-support/src/lib.rs:33-142` — `mod brace`, `mod cco`,
  `pub(crate) mod contact`, `pub(crate) mod designed_embed`,
  `pub(crate) mod initial`, `mod joints`, `mod lower`, `pub mod raft`,
  `pub(crate) mod runner`, `pub(crate) mod seat_zone`,
  `pub(crate) mod transit_measurement` are each annotated
  `reason = "...consumed by the pending support runner..."`. `runner.rs`
  (line 71-96) directly imports from `brace::`, `cco::`, `initial::`,
  `joints::`, `lower::`, `raft::`, and uses `crate::designed_embed::`,
  `crate::transit_measurement::` at lines 740-3379. `runner::run_support` is
  re-exported (`lib.rs:172-173`) and called from
  `crates/plinth-pipeline/src/lib.rs` (multiple sites) and `prepare.rs:144`.
  Verdict: **LIVE, mislabeled** — the "pending" module allows in `lib.rs`
  should be deleted; leaving them masks any *actually new* dead code the lint
  would otherwise catch in these modules.
- `crates/plinth-support/src/legacy_compat.rs:39` `o10_live_current_support_and_raft()`
  and `:78` `size_live_current_support_member()` — reason claims "the future
  current-support controller is the sole O-10 caller." Called from production
  (non-test) code in `rewire.rs:609,697,797,843`, `brace.rs:867,1068,2096`,
  `cco.rs:5607,5665` (inside `candidate_price`/`o10_uniform_radius_for_span`),
  and `transient.rs:434`. Verdict: **LIVE, mislabeled** — this is the current,
  not future, caller.
- `crates/plinth-support/src/anchor.rs:33,35` `FOOT_BREACH_HALF_DEG`,
  `FOOT_BREACH_CONE_RAYS` (`#[cfg_attr(not(test), allow(dead_code))]`) — used
  in production at `anchor.rs:1744-1745` (inside
  `validate_anchor_feet_bound`) and `brace.rs:1412-1413`. Verdict: **LIVE,
  mislabeled**.
- `crates/plinth-support/src/anchor.rs:1660` `validate_finalized_anchor_feet`
  (reason: "the pending controller consumes this finalized authority seam") —
  called from `crates/plinth-support/src/cco.rs:3749` inside
  `grow_terminal_inner`, itself reached from the live `run_support` path.
  Verdict: **LIVE, mislabeled**.
- `crates/plinth-support/src/perception.rs:286` `from_partition` (reason:
  "the sealed support runner consumes the specialized plan's canonical ids")
  — called from `perception.rs:272`, `initial.rs:1901,2080`,
  `specialized.rs:12967,13903` outside its own test module. Verdict: **LIVE,
  mislabeled**.
- `crates/plinth-support/src/coverage_closure.rs:601` `RankedRepairRow`
  (reason: "...lands in the next commit of this lane and the allow goes with
  it") — already consumed by `runner.rs:2606,2994`. Verdict: **LIVE,
  mislabeled; the "next commit" already happened.**
- `crates/plinth-support/src/transient.rs:26,31` `MIN_GEOMETRY_LENGTH_MM`,
  `MIN_Z_EXTENT_MM` — used at `transient.rs:484,490,494,762`, same file, same
  function scope as their declaration. Verdict: **LIVE, mislabeled** (these
  should never have needed `allow` at all — they're referenced two lines
  down in the same module).

**Recommendation**: sweep all 75 dead_code allows in `plinth-support/src`
(list: `anchor.rs`×12, `lib.rs`×12, `perception.rs`×15, `runner.rs`×10,
`coverage_closure.rs`×9, `transient.rs`×6, `coverage_measurement.rs`×4,
`cco.rs`×2, `legalization.rs`×2, `member_sizing.rs`×2, `legacy_compat.rs`×3,
`census.rs`×2, `brace.rs`/`forest.rs`/`joints.rs`/`rewire.rs`×1 each) with
`cargo +nightly rustc -- -W dead_code` per item, or simply delete each `allow`
and let the compiler re-flag whatever is genuinely still unreached. Based on
the 100% stale-hit rate in this sample, expect most to be removable outright.

## Genuinely dead / narrowable items found

| Item | Location | Verdict | Evidence |
|---|---|---|---|
| `ActualLateralLoad::from_tip_load_path_and_moment_arm` | `crates/plinth-support/src/member_sizing.rs:116` | **DEAD (honestly labeled)** | Doc comment above it (line 87-88) states "No such production derivation exists yet, so this remains unused" — self-aware scaffolding, only referenced from a `compile_fail` doctest at line 84. This is the one accurate `#[allow(dead_code)]` in the file; contrast with `LegacyCompatibilityO10` two lines below it, whose identical-looking reason string is false (see above — it's live via `member_sizing.rs:131 legacy_compatibility_o10_frozen_shaft_drag`, called by `legacy_compat.rs`, itself heavily live). |
| `M0PlaceholderConstants::perf_anchor_cap`, `perf_beam_k`, `perf_fix_iter_caps`, `perf_seed_spacing_factor` | `crates/plinth-core/src/constants/placeholders.rs:1342-1348` | **NARROWABLE (inert feature flags)** | All four are declared `TrialSetting::disabled()` (never enabled anywhere) and their only readers workspace-wide are `crates/plinth-core/tests/constants_placeholders.rs:933-937`, which assert `.is_disabled()` — a tautology test per the project's own doctrine #4 (it would pass identically if the feature were deleted). No production call site branches on `.is_enabled()` for any of the four. Compare `rewire_work_budget` (line 1370, same `TrialSetting` pattern), which IS read in production at `crates/plinth-support/src/rewire.rs:156`. These four are dead performance-tuning knobs; either wire them in or delete the fields, the registry `descriptors()` entries, and the tautology asserts. |
| `crates/plinth-metrics/src/authority.rs:193,228,242` `Manifest`, `ManifestModel`, `SourceDigest` | LIVE (not a finding) | Deserialized via `serde_json::from_str::<Manifest>` at `authority.rs:327` with `#[serde(deny_unknown_fields)]` — the `#[allow(dead_code)]` on the struct is the standard "parse-then-partially-consume, deny_unknown_fields does the real work" pattern, not dead code. Checked so it doesn't get miscounted. |
| `crates/plinth-core/src/constants/placeholders.rs` (1,546 lines) overall | LIVE (not a finding) | Every one of its 21 top-level `pub` types is used as a field type inside the `placeholder_declarations!` macro invocation (lines 988-1546) that builds the live `M0PlaceholderConstants`, which is read via `m0_constants()` from `plinth-pipeline`, `plinth-orient`, `plinth-decimate`, `plinth-support`, etc. (20+ call sites). The apparent "0 external users" from a plain symbol grep is an artifact of Rust type inference on accessor methods (`.a_abs_mm2()` etc.) — not evidence of dead code. Retracted after tracing. |

## scripts/ and exports/ — orphaned build artifacts

- `scripts/__pycache__/`, `scripts/tools/__pycache__/`,
  `scripts/tests/__pycache__/`, `scripts/support_placement/__pycache__/`,
  `scripts/.pytest_cache/` — **156 files total**, zero corresponding `.py`
  source files anywhere under `scripts/` (`find scripts -iname "*.py" | grep
  -v __pycache__` returns nothing). These are leftover bytecode caches from
  the retired Python codebase (per project doctrine: "Resin Rust rewrite
  decision... no new Python"). Safe to delete; nothing reads compiled
  bytecode.
- `scripts/tests/fixtures/*.json` and `scripts/tests/fixtures/chitubox/*` —
  **checked, LIVE, not dead**: referenced by
  `crates/plinth-orient/tests/orientation_rederivation.rs`,
  `crates/plinth-verify/tests/{no_python_guard,format_gate}.rs`,
  `crates/plinth-emit/tests/native_winding_oracle.rs`,
  `rust/chitubox_io/tests/{frozen_baseline.rs,common/mod.rs}`. Do not delete.
- `exports/*.chitubox` — referenced from
  `crates/plinth-pipeline/examples/shared/first_artifact_chain.rs` (which is
  itself the file carrying the `#![allow(dead_code)]` at line 25 — an
  example/shared-helpers module, conventionally over-permissive since example
  binaries commonly only use a subset of shared helpers; not flagged
  further given scope).

## Not completed (scope/time-boxed)

- Full pub-item cross-reference for `plinth-hollow` (16,950 lines),
  `plinth-mesh` (6,920), `plinth-metrics` (9,176 beyond `authority.rs`),
  `plinth-pipeline` (7,682), `plinth-orient` (4,901), `rust/geom_kernel`
  (8,931) — not exhaustively walked; the one `dead_code` hit in
  `rust/geom_kernel/src/queries.rs:2868` and `plinth-support/src/cco.rs:896`
  were not individually traced.
- Feature-flag/cfg-gate sweep (item 5) beyond the `census` feature already
  documented inline at `plinth-support/src/lib.rs:38-61` (verified
  self-describing and quarantined correctly — not a finding).
- Struct-field/enum-variant reachability (item 6) beyond the specific cases
  above.

## Summary (≤20 lines)

- **plinth-support**: headline finding. 75 `dead_code`-related allows in
  `src/`; every one of ~10 traced (anchor.rs, legacy_compat.rs, lib.rs
  module list, perception.rs, coverage_closure.rs, transient.rs) was
  **stale** — the guarded code is live production, reached via
  `runner::run_support` which `plinth-pipeline` calls. One honest exception
  found: `member_sizing.rs:116 from_tip_load_path_and_moment_arm` is truly
  dead and says so in its own doc comment.
- **plinth-core**: `placeholders.rs` (1,546 lines) is live end-to-end, not
  scaffolding — retracted initial suspicion after tracing the
  `placeholder_declarations!` macro. Narrow exception: 4 `TrialSetting`
  perf-tuning fields (`perf_anchor_cap`, `perf_beam_k`, `perf_fix_iter_caps`,
  `perf_seed_spacing_factor`) are never read in production, only
  tautologically asserted disabled in a test.
- **plinth-metrics**: `authority.rs` `Manifest`/`ManifestModel`/`SourceDigest`
  dead_code allows are legitimate (deny_unknown_fields validation pattern),
  not findings.
- **scripts/**: 156 files of orphaned Python `__pycache__`/`.pytest_cache`
  bytecode with zero surviving `.py` source — safe deletion candidate.
  `scripts/tests/fixtures/*` are live (used by 6+ Rust integration tests) —
  do not delete.
- **Five largest dead/mislabeled regions by line count** (all
  plinth-support unless noted):
  1. `lib.rs:33-142` — 11 module-level allows spanning the crate's entire
     tree-building pipeline (brace/cco/initial/joints/lower/raft/runner/
     seat_zone/transit_measurement/designed_embed/contact) — mislabeled, not
     dead; largest by blast radius even though each site is only 1-6 lines.
  2. `legacy_compat.rs` (391 lines) — whole-file mislabeling; two guarded
     methods (39, 78 lines of a 391-line file) both live.
  3. `coverage_closure.rs` — 9 allow sites across a 1,500+-line module,
     `RankedRepairRow` (604-650, ~46 lines) confirmed live/mislabeled.
  4. `perception.rs` — 15 allow sites in a 7,600+-line module;
     `from_partition` (289-310, ~21 lines) confirmed live/mislabeled.
  5. `placeholders.rs:1342-1348` — 7 lines, the one confirmed genuinely dead
     region (4 unread `TrialSetting` fields) outside the honestly-labeled
     `from_tip_load_path_and_moment_arm`.
- **Not covered**: exhaustive pub-item audit of plinth-hollow, plinth-mesh,
  plinth-pipeline, plinth-metrics (beyond authority.rs), plinth-orient,
  geom_kernel — flagged above as out of time-box, not asserted clean.
