# plinth-3d documentation/code drift census

Reviewed 2026-08-26 against the Rust workspace at `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`. This is a read-only source review. “Unverifiable” means source cannot establish the claim, not that the claim is necessarily false in the physical world.

## Executive result

The primary `prepare` order and pre-write gate ordering agree with source. The largest documentation problems are the non-functional documented cache switch, stale claims about retired Python material, an embed-policy implementation pointer that names the audit rather than the live enforcement path, and an architecture page that gives roles but not the actual Cargo dependency topology.

## Claim census

| Claim | document:line | code:line read | verdict |
|---|---:|---:|---|
| `prepare` runs `repair → decimate → orient → hollow → support`; only a passed authority reaches emission. | `docs/architecture.md:6, 146` | `crates/plinth-pipeline/src/prepare.rs:1037-1040, 1931-1944`; `crates/plinth-pipeline/src/gates.rs:459-504` | **VERIFIED.** |
| `--no-cache` disables cache use. | `docs/architecture.md:147`; CLI help `crates/plinth-cli/src/lib.rs:29` | `crates/plinth-cli/src/execute.rs:70-88`; `crates/plinth-pipeline/src/prepare.rs:903`; `rg no_cache` shows no consumer | **FALSE / documented workflow ineffective.** The flag is parsed and serialized into the run report, but no cache read/write branches on it. |
| `scripts/support_placement/` no longer exists. | `docs/specs/kernel-geometry.md:13` | `scripts/support_placement/__pycache__/rewire.cpython-310.pyc` (and 63 more files) | **FALSE.** No `.py` source remains, but the retired directory is present with 64 Python bytecode artifacts. |
| The two embed-policy implementations are an engine at `plinth-metrics/src/scar.rs` and an independent Python census. | `docs/specs/embed-policy.md:187-193` | `crates/plinth-metrics/src/scar.rs:1-6, 168-240`; `crates/plinth-support/src/transit_measurement.rs:51, 89`; `scripts/support_placement/` contains no `.py` | **FALSE / stale Python reference.** `scar.rs` is explicitly an independent reopened-artifact audit. Production support enforcement reads the scar-area policy through `transit_measurement`; the claimed independent Python implementation is absent. |
| `plinth-metrics` is an independent auditor which depends on none of the producing crates. | `docs/architecture.md:137` | `crates/plinth-metrics/Cargo.toml:8-13` | **VERIFIED narrowly.** It depends only on `plinth-core` and `chitubox_io`, not producing `plinth-*` crates. It is not dependency-free: it consumes the output format crate. |
| `plinth-verify::verify_artifact` is consulted after write. | `docs/architecture.md:144` | `crates/plinth-verify/src/lib.rs:505-522`; call sites only in `plinth-verify` tests and `crates/plinth-pipeline/examples/shared/first_artifact_chain.rs:370` | **FALSE for the production `prepare` route.** `plinth-emit` performs its own fresh-open audit (`crates/plinth-emit/src/lib.rs:197-236`), but `prepare` does not call `verify_artifact`. |
| Every `geom_kernel` public query takes a slice and returns a slice. | `docs/architecture.md:148` | `rust/geom_kernel/src/lib.rs:49-80`; e.g. `rust/geom_kernel/src/point_index.rs` public `PointCloudIndex` APIs | **FALSE.** The crate also exports scalar/configuration/value APIs (`V3`, `PointCloudIndex`, planning types, digest functions); the batch claim should be scoped to hot geometry queries. |
| `plinth-emit` is the final f64-to-f32 boundary, fresh-project writer, and fresh-open auditor. | `docs/architecture.md:145`; `README.md:25` | `crates/plinth-emit/src/lib.rs:1-7, 73-97, 120-176, 197-236` | **VERIFIED.** |
| A failure/refusal leaves no artifact at target. | `docs/architecture.md:107-110` | `crates/plinth-pipeline/src/prepare.rs:965-1007`; `crates/plinth-emit/src/lib.rs:60-95`; `crates/plinth-pipeline/src/gates.rs:435-450` | **VERIFIED for artifact target cleanup.** |
| Output is one `.chitubox` project per emitting model. | `README.md:22, 42-55`; `docs/status.md:8-13` | `crates/plinth-pipeline/src/prepare.rs:863-880`; `crates/plinth-emit/src/lib.rs:73-89` | **VERIFIED but incomplete.** |
| Pipeline output comprises only projects/reports stated in README/status. | `README.md:22, 42-55`; `docs/status.md:8-13` | `crates/plinth-pipeline/src/coverage_evidence.rs:221-253, 600-638, 1124-1218`; `crates/plinth-pipeline/src/prepare.rs:1923-1929` | **UNDOCUMENTED SIGNIFICANT BEHAVIOUR.** Every sealed run attempts `<stem>.coverage.repair.json`; coverage refusal can additionally create `<stem>.coverage.json` and per-island SVGs. Ledger write failure is deliberately non-blocking. |
| The five commands are all mandatory gates; formatting has exactly 104 hunks. | `docs/development.md:34-42, 63-74` | Current `cargo fmt --all -- --check`: 104 files with diffs; no hunk marker is emitted by rustfmt | **PARTIALLY UNVERIFIABLE / terminology drift.** The command works and currently reports 104 differing files, but source/tool output does not establish “104 hunks”; rustfmt’s output has no `@@` hunks. |
| `cargo build --workspace --offline` is the entire fresh-checkout build story. | `docs/development.md:7-12`; `README.md:35-40` | `Cargo.toml:1-20`; `rust-toolchain.toml`; Cargo build command not run | **UNVERIFIABLE from source alone.** Manifests specify a locked offline workspace, but a clean machine’s cached registry/toolchain state is environmental. |
| Corpus link provisioning and `shasum -a 256 -c corpus/manifest.json` work as written. | `docs/development.md:153-178` | `corpus/` examined only by source layout; corpus inputs intentionally absent | **UNVERIFIABLE here.** It depends on external corpus files and a populated source checkout. |
| 24/37 emit; named 1500 s outcomes, artifact sizes/counts, and all physical calibration statements. | `README.md:27-31`; `docs/status.md:17-41`; `docs/physics.md:1-...` | Runtime/corpus/evidence required | **UNVERIFIABLE from code.** These are measurements, historical results, or physical claims rather than compiler-enforced behaviour. |
| Support sizing is based on the stated failure physics rather than heuristics. | `README.md:9-13`; `docs/physics.md:21-...` | Registered policy flows from `plinth-core` into support code, e.g. `crates/plinth-support/src/transit_measurement.rs:51-89` | **UNVERIFIABLE as stated.** Source can show formulas and constant flow, not that calibration was measured or that the physical model is valid. |
| ChituBox format offsets/layout/confidence and GUI loading semantics. | `docs/specs/chitubox-format.md:11-398`; `docs/specs/chitubox-io.md:399-617` | `rust/chitubox_io/src/project.rs:19-45`; parser/writer source | **PARTLY VERIFIABLE, PARTLY UNVERIFIABLE.** Code confirms the embedded empty template and implementation intent; historical specimen counts, native/UI provenance, confidence grades, and GUI compatibility require fixtures or manual GUI evidence. |
| `chitubox_io` has only `thiserror`, no unsafe, hand-rolled LE codec. | `docs/specs/chitubox-io.md:63-64` | `rust/chitubox_io/Cargo.toml:17-23`; source imports / crate attributes | **FALSE in wording.** Runtime dependency is `thiserror`, but dev-dependencies include serde, serde_json, sha2. “No unsafe” and hand-rolled codec are source-verifiable separately. |
| `chitubox_io` uses embedded `empty.chitubox` for from-scratch projects. | `docs/specs/chitubox-format.md:252-257`; `docs/specs/chitubox-io.md:401-452` | `rust/chitubox_io/src/project.rs:19, 44-45` | **VERIFIED.** |
| Seal semantics describes currently landed routing/brace behaviour. | `docs/specs/seal-semantics.md:1-194` | `crates/plinth-support/src/cco.rs`, `coverage_repair.rs`, `forest.rs`, runner call paths; `docs/status.md:50-60` | **PARTLY UNVERIFIABLE.** Code types and names exist, but “unreachable,” historical owner rulings, and absence of witnesses cannot be proved by static reading. Status correctly discloses two structurally untested corners. |

## Architecture layering assessment

`docs/architecture.md:131-149` is useful as a role catalogue but is **not a faithful crate-layering description** because it does not state direct Cargo edges and implies a cleaner hierarchy than exists. Actual direct workspace edges from manifests are:

```text
plinth-cli      -> core, hollow, orient, pipeline
plinth-pipeline -> core, decimate, emit, hollow, mesh, orient, support, verify
plinth-emit     -> chitubox_io, geom_kernel, core, mesh, support
plinth-verify   -> chitubox_io, core, support
plinth-support  -> geom_kernel, core, hollow, layers, mesh
plinth-hollow   -> core, mesh, orient, solid, geom_kernel
plinth-orient   -> geom_kernel, core, decimate, mesh
plinth-decimate -> geom_kernel, core, mesh
plinth-mesh     -> geom_kernel, core
plinth-layers   -> geom_kernel
plinth-metrics  -> chitubox_io, core
core / solid / geom_kernel / chitubox_io -> no workspace crate dependencies
```

Consequences for the document:

1. The stated “independent” metrics auditor is correctly insulated from producers, but it directly depends on the container parser (`chitubox_io`), which should be named as an audit-input boundary.
2. The graph is not a simple stage stack: `hollow -> orient`, `orient -> decimate`, and `support -> hollow` are real reverse-looking stage edges. The page should explicitly label these as typed substrate/history dependencies, or readers will infer an acyclic stage-layer architecture that Cargo does not enforce.
3. `emit` and `verify` both depend on `support`; `pipeline` is therefore an orchestration apex rather than the sole route by which code reaches emission/verification. In particular, the architecture claim that `verify_artifact` is post-write production validation is wrong for `prepare`.
4. `plinth-cli` directly imports `plinth-hollow` and `plinth-orient` in addition to `plinth-pipeline` (`crates/plinth-cli/Cargo.toml:15-18`) to resolve wall and pinned orientation policy. This is a documented role omission, not necessarily a layering defect.

## Retired Python references

- **False path-state claim:** `docs/specs/kernel-geometry.md:13` says `scripts/support_placement/` no longer exists. It exists with stale `.pyc` cache artifacts; no `.py` sources remain.
- **Stale normative implementation claim:** `docs/specs/embed-policy.md:10, 43, 187-193` requires a Python census/identical Rust-Python bits, but no Python implementation remains. It also identifies the metrics audit as the engine.
- **Historical references that are not themselves false:** `docs/specs/kernel-geometry.md:6-12` and `docs/specs/chitubox-io.md:138,456,533,594` frame Python as retired historical/provenance material. They should be kept only if the absent-source distinction is made clear; they do not prove a runtime Python dependency.

## Commands/workflows that fail or mislead today

| Command/workflow | status | evidence |
|---|---|---|
| `plinth prepare ... --no-cache` | **Misleading.** Accepted but currently changes no execution path. | `crates/plinth-cli/src/execute.rs:70-88`; no pipeline source consumer beyond request/report propagation. |
| `cargo fmt --all -- --check` | **Fails today by documented design debt.** It exits nonzero and currently prints 104 file diffs. | Direct foreground run during review. |
| `cargo build/test --offline` from a truly fresh checkout | **Conditional.** It requires the pinned toolchain and all Cargo cache entries to be locally available; code cannot prove that environment. | `Cargo.toml`, `rust-toolchain.toml`; docs explicitly require offline. |
| Corpus-dependent tests/census/provisioning | **Conditional.** Cannot run until non-git corpus files are supplied. | `docs/development.md:153-178`; corpus design. |
| `first_artifact --all` | **Not a clean success workflow today.** The example itself says gate-refused models produce no artifact and returns failure if any attempted model fails. | `crates/plinth-pipeline/examples/first_artifact.rs:21-33, 84-105`. README accurately calls it a development corpus driver, but does not warn its all-model run can exit failure. |

## Significant code behaviour not surfaced in the public README/status

1. The per-model output directory is an evidence bundle, not just projects: sealed repair ledger JSON is attempted for every sealed support run; coverage-refusal JSON and SVG island renders are persisted on the relevant refusal paths. See `coverage_evidence.rs:221-253, 600-638, 1124-1218`.
2. Before doing work, `prepare_one` clears the prior artifact and stale coverage evidence. A cleanup failure becomes a model failure, preventing a stale file from being mistaken as this run’s result. See `prepare.rs:965-1007`.
3. The actual final disk check used by `prepare` is `plinth-emit`’s bespoke fresh-open audit, not the public `plinth-verify::verify_artifact` utility. See `plinth-emit/src/lib.rs:197-236` and `plinth-verify/src/lib.rs:505-522`.
4. The CLI supports `--no-cache`, but it is not implemented as an execution control. This should either be wired to a real cache or removed from documentation/help/API.

## Checks run

- Parsed Cargo manifests and direct path dependencies with `cargo metadata --offline --no-deps --format-version 1`.
- Ran `cargo fmt --all -- --check` in the foreground. It failed as the documentation predicts with 104 files reported; no repository files were modified.
- Read pipeline sequencing, CLI parse/execute, gate, emitter audit, verifier, sidecar, metrics scar, support transit, geometry-kernel, and all requested top-level/spec documentation source named above.

