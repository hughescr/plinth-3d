<!-- Provenance: design-agent survey handoff, committed verbatim by E1 implementer 2026-08-07.
     Surveyed develop @ d711586 on 2026-08-07; merge b0324fd (FU-8f G3+G4+G5) landed mid-survey
     touching zero export-path files, so all file:line citations hold at both shas.
     Source: handoff-export-path-design-2026-08-07.md (orchestrator handoff). -->

# Export-path design: sealed support plan → .chitubox on disk

```
PROVENANCE
  Surveyed:   develop @ d711586904772bb295cd853f15ee58f3254d0482 (2026-08-07)
  Mid-survey: merge b0324fd (FU-8f G3+G4+G5) landed on develop while this survey ran.
              Full file list of d711586..b0324fd: README.md, crates/plinth-core/src/constants/{evidence,placeholders}.rs,
              crates/plinth-support/src/{perception,specialized}.rs, crates/plinth-support/tests/resource_cap_policy.rs,
              and six docs/rust-migration files. ZERO export-path files touched (chitubox_io, plinth-emit,
              plinth-cli, plinth-pipeline, plinth-verify all unchanged) — every file:line below is valid at both shas.
  Parked:     codex/chitubox-native-preview @ 9ffc829 (worktree /private/tmp/plinth-chitubox-preview, verified read-only)
  Author:     design agent, read-only survey; no repo edits, no builds run.
```

## Part 1 — Findings (numbered, cited)

### A. What chitubox_io can write today

**F1. The container is a proprietary flat binary — not zip, not sqlite.** One
file, little-endian, magic `0xAB231243` at offset 0
(`rust/chitubox_io/src/consts.rs:8`). Layout: 24-byte header, 396-byte opaque
settings blob, raw preview/thumbnail blob, n×680-byte model records, then
per-model mesh blobs (36 bytes/triangle, no normals) and optional support
sections (16-byte index → 400-byte group header → 72-byte segment records,
each pointing at its own embedded triangle-mesh blob)
(`docs/specs/CHITUBOX-FORMAT.md:20-36`, `consts.rs:31-64`). Reverse-engineered
2026-07-23 by byte-differential corpus analysis (`CHITUBOX-FORMAT.md:3-10`).

**F2. A written project contains: models, placements, per-model supports — and
no print settings.** Model records carry name + translation only; rotation and
scale are baked into stored vertices with no readable matrix
(`rust/chitubox_io/src/types.rs:43-55,73-86`). Supports are typed 72-byte
records (TIP=7, TAPER=1, SHAFT=9, JOINER=3, FLARE=4, PAD_OR_BRIDGE=6,
`types.rs:129-137`) each with start/end/radii and its own embedded triangle
soup (`types.rs:141-153`). The 396-byte settings blob is opaque and all-zero
across the corpus; mapping layer height/exposure/resin is an explicitly listed
unknown (`CHITUBOX-FORMAT.md:50,227-233,263-265`). **No machine profile is in
the file.**

**F3. ChituBox re-slices on open.** A `.chitubox` is a pre-slice *project*
(models + editable supports), not sliced output. The GUI acceptance checklist
is load → select/move → editable supports → **slice**
(`docs/rust-migration/05-chitubox-and-corpus.md:291-298`); the preview blob is
"Not needed for load/slice" (`CHITUBOX-FORMAT.md:263`). Machine/resin/layer
settings come from ChituBox's own profile at slice time.

**F4. The API surface is complete for single- and multi-model writes.**
`Project::new_empty()` (embedded 420-byte template, `project.rs:19,39`),
batch `add_models` (STL path or triangle soup, identity-centered,
`project.rs:76-120`), `set_placements`, `rotate_models_z` (relative Z-rot with
rigid support rebake), batch `attach_supports` (Replace/Append + lift,
`types.rs:195-211`), `save` (`project.rs:580`). Gold invariants: byte-identical
round-trip of unmodified files, every write re-parsed and structurally
verified before "done", opaque regions preserved verbatim
(`rust/chitubox_io/src/lib.rs:7-10`). Honesty clause: structural verify ≠
ChituBox will load it; the GUI is the only semantic oracle (`lib.rs:14-16`).
Asymmetric multi-model support attachment is tested
(`tests/support_parity.rs:56-68` and the replace/append round-trips).

**F5. The python-feature story.** The crate's `python` feature (off by
default) builds a PyO3 `chitubox_io._native` extension
(`rust/chitubox_io/Cargo.toml` `[features] python`; `src/py.rs:1-27` —
empty-template-only, batch-columnar numpy surface). What compares against it:

- `scripts/tests/test_chitubox_bindings.py` (marked `chitubox`; conftest
  **fails**, never skips, when the extension is missing/stale/API-mismatched —
  `scripts/tests/conftest.py:84-98`) exercises container writes through the
  binding.
- `scripts/tests/test_chitubox_emit.py` tests the **Python** support-mesh
  synthesis (`support_placement.chitubox_emit`) — frustum/pad/bridge
  watertightness via trimesh.
- `rust/chitubox_io/tests/frozen_baseline.rs` does **not** invoke Python at
  test time: it validates the frozen corpus manifest whose pinned freeze
  command is `uv run --frozen python scripts/tools/prep_one.py --p8-relocate
  --out-dir corpus/baseline --skip-existing corpus/models/*.stl`
  (`frozen_baseline.rs:551`), over the 8 corpus models
  (`frozen_baseline.rs:588-597`).

**Retiring Python entirely** means: (a) pass the M1 parity/acceptance gate
(Rust as-good-or-better on every M1 metric on every model, then delete —
`docs/rust-migration/04-migration-plan.md:148-156`); (b) delete `src/py.rs`,
the `python` feature + pyo3/numpy deps, `rust/chitubox_io/python/`, the
maturin config in `pyproject.toml`, `scripts/support_placement/` (36 modules),
`scripts/tools/prep_one.py` and friends, and the pytest suite. The frozen
baseline **artifacts** stay (they are evidence files; `frozen_baseline.rs`
reads JSON + bytes, not Python). The Python engine is today the only path that
has ever produced an end-to-end artifact (`corpus/baseline/Sphere.chitubox`
exists; `prep_one.py:1-11` is "STL in, .chitubox out").

### B. The parked branch codex/chitubox-native-preview

**F6. Exactly one commit, small, and load-bearing.** Merge-base with develop
is `e69b94b` ("size front descriptors per model"), which **is** on develop;
the branch adds only `9ffc829` "fix(chitubox): embed native preview fallback"
(2026-08-03): +178/−17 across 7 files — a committed 26,996-byte opaque preview
asset (bytes `[420,27416)` of the UI-authored `two-supports.chitubox`, SHA-256
pinned `e7c0f5…ffde`), installed by `add_models` only when the **first** model
is added to an empty-preview project (parsed previews stay verbatim), plus a
118-line structural test `tests/preview_fallback.rs` and spec updates
(diff of `rust/chitubox_io/src/project.rs` and
`docs/specs/SPEC-CHITUBOX-RS.md` Appendix A.0 on the branch).

**F7. Why it matters: ChituBox REFUSED a from-scratch project without it.**
The 2026-08-03 GUI attempt failed to open a from-scratch populated project,
logging `open file fail, path CTTestProDir/collision_0.png` (R1 in
`SPEC-CHITUBOX-RS.md` risk table, branch version). The response was split:
front-descriptor fix (landed on develop as `e69b94b`) then this preview
fallback (never landed). **develop today has no preview fallback** —
`grep NATIVE_PREVIEW_FALLBACK` on develop's `project.rs` is empty and
`assets/two-supports-preview.bin` exists only on the branch. A first
openable-in-ChituBox artifact almost certainly requires this commit.

**F8. Salvage verdict: fully salvageable, near-clean cherry-pick.** Develop's
chitubox_io moved past the merge-base by only two commits — `3838aea`
(+3 lines `error.rs`, +4 lines `project.rs`, adds crates/plinth-verify) and
`84cc6e9` (verify-side tests). Nothing on the branch is superseded by two
months of engine evolution because the engine evolution never touched
chitubox_io's write path. The branch's own spec text is candid that GUI
re-test remains required — the commit is structural-verified, not
GUI-verified.

### C. The seal → geometry gap

**F9. The chain exists at the type level — further than expected.**
`run_support(HollowedModel, SupportRequest, SupportPreparationInput) →
FinalSupportAuthority` (`crates/plinth-support/src/runner.rs:438,408`);
`FinalSupportAuthority::into_supported_model() → SupportedModel`
(`runner.rs:419-423`). The sealed forest is lowered to semantic f64
`SupportPrimitive`s by a dedicated parity pass
(`crates/plinth-support/src/lower.rs:1-6` — "pure, serial lowering …
preserves the frozen member/ball/raft record order; mesh construction and
ChituBox record identities remain owned by `plinth-emit`"). `SupportedModel`
= mesh + primitives + audited history, constructor-validated
(`crates/plinth-support/src/lib.rs:462-500`); primitives carry segment kind,
endpoints, radii, contact depth, ring-normal provenance, sphere authority
(`lib.rs:386-403`).

**F10. Support members ARE materialized as printable triangle meshes — in
plinth-emit, already written.** `crates/plinth-emit/src/lib.rs` (1,633 lines)
is "the single final ChituBox emission boundary" (`lib.rs:1-6`):
`frustum_triangles` (:583), `sphere_triangles` (:623), `raft_pad_triangles`
(:658), `raft_bridge_triangles` (:676), with outward winding enforced by
signed-volume check (`wind_outward` :737, `signed_volume` :747). 24-sided
frusta, 8×16 spheres, 20-sided raft pads (`lib.rs:21-24`). `write()`
(`lib.rs:55-116`) converts a `SupportedModel` into one fresh project (empty
template, one model named `plinth-model`, `lib.rs:20`), attaches records with
the seal's lift, saves, then **audits a freshly reopened disk copy**
(`audit_disk` :135) including build-volume validation against the
`PrinterProfile` (`validate_against` :356,:386). The plan does NOT stop at
abstract members.

**F11. Coordinate frames and units — the emitter owns the conversion.** The
seal speaks f64 millimetres in the oriented, un-centred model frame; the plate
plane is derived as `plate_z = min_z − lift` (`runner.rs:445-452`), and the
`BuildFrame` (lift + plate_z) is the sole shared authority
(`lib.rs:445-460` SupportHistory accessors). chitubox_io speaks f32 mm in a
bbox-centered model frame plus a `Placement{x,y,z_lift}` translation
(`types.rs:46-55`). plinth-emit bridges: bbox-center subtraction
(`mesh_bbox_center` :410, `sub_center` :533), checked f64→f32 (`f32_of` :561),
lift assigned exactly once to the placement (`lib.rs:52-54` doc comment).

**F12. THE GAP: nothing connects seal to emit end-to-end.** No test, example,
or binary calls `run_support → into_supported_model → plinth_emit::write`.
Evidence: repo-wide grep — `into_supported_model` is called only inside
plinth-support (runner internals + its own integration tests at
`tests/support_runner_integration.rs:157,203`, which stop at verification
snapshots); `EmitRequest` has zero consumers outside plinth-emit;
plinth-pipeline imports only the `EmitReport`/`EmitError` **types**
(`crates/plinth-pipeline/src/lib.rs:15`). plinth-emit's own tests build
hand-constructed fixture primitives (`lib.rs:873 fixture_model`), never a
sealed run. **A .chitubox file from the native engine's own sealed plan has
never been produced.**

**F13. plinth-pipeline is deliberately an empty shell.** "This checkpoint
intentionally contains no loader, cache, stage runner, or emit loop"
(`crates/plinth-pipeline/src/lib.rs:3-4`); the promised signature is
`prepare(PrepareRequest) -> Result<PrepareReport, PipelineError>` (`lib.rs:6`)
with `PrepareRequest` fully typed (input dir, output dir, orientation policy,
output profile, decimation budget, hollow + support requests, no_cache,
stop_after — `lib.rs:217-228`) and per-stage `PipelineError → PipelineExitCode`
mapping (`lib.rs:262-314`). Only `hollow_stage` is a real function
(`lib.rs:35-40`).

**F14. Hollow-stage caveat.** The 7-of-8 sealed-run evidence was produced with
`HollowedModel::solid_decision(...)` handoffs, bypassing the real hollow run
(`support_runner_integration.rs:36-38,60-62`). Whether `plinth_hollow::run`
completes on corpus models inside `prepare()` is unproven.

### D. The runner/CLI surface

**F15. The CLI exists as a parser-only milestone.** `crates/plinth-cli`
(binary `plinth`) parses the ratified surface `plinth prepare
--input-models-dir DIR --output saturn4-ultra --out-dir DIR [--no-cache]`
into a typed `PrepareSelection` (`crates/plinth-cli/src/lib.rs:17,37-44`);
execution prints "execution is not implemented in this parser milestone"
(`src/main.rs:20-22`, pinned by `tests/process.rs`). The output profile
resolves to the M0 Saturn 4 Ultra authority (`plinth-pipeline/src/lib.rs:106`,
test :374-379). **The CLI layout question is settled: crates/plinth-cli is the
home; the missing piece is the resolver from `PrepareSelection` →
`PrepareRequest` (`plinth-cli/src/lib.rs:33-36` names this seam) plus
`prepare()` itself.** Everything else under examples/ is instrumentation
(`crates/plinth-support/examples/embed_incidence_census.rs`;
`rust/chitubox_io/examples/acceptance_artifacts.rs` is the GUI-gate artifact
generator, `regen_corpus.rs` the fixture regenerator).

### E. Print settings

**F16. Saturn 4 Ultra parameters live in the M0 constants registry — hardcoded
profile with typed provenance, not config files.** `PrinterProfile` carries
build volume, pixel pitch x/y, default layer height, lift (Placeholder-grade),
leading release edge, sweep direction, film thickness
(`crates/plinth-core/src/printer.rs:156-166`), built from evidence-grade
constants in `crates/plinth-core/src/constants.rs:267-281`. Emit uses it only
for bounds auditing (F10). Per-file print settings are unnecessary for the
terminal goal (F2/F3): ChituBox applies its own machine profile and slices in
the GUI. **Recommendation: keep the settings blob opaque; do not
reverse-engineer it for v1** (it is unknown #3 in `CHITUBOX-FORMAT.md:263-265`
and would need a purpose-built GUI-authored specimen sweep).

**F17. Reference files to reverse against exist and are committed.** 14
specimens under `scripts/tests/fixtures/chitubox/` (native UI-authored
`two-supports.chitubox`, `morel.chitubox`, real Python-emitter outputs incl.
`morel_mudskipper_supports.chitubox` with 243 records), the embedded
`assets/empty.chitubox`, the Python-baseline `corpus/baseline/Sphere.chitubox`,
and the optional oversized `treant_v5_candidate.chitubox` via
`CHITUBOX_CORPUS_DIR` (`rust/chitubox_io/tests/common/mod.rs:1-27`).

### F. Digest safety (house rule: census digests untouched)

**F18. The export path is downstream of every frozen digest — with evidence.**
Seal digests (`forest_digest`, `live_view_digest`) are computed during
lowering, before a `SupportedModel` exists (`lower.rs:97-99`); OD-8's seven
frozen digests are decimation-stage; plinth-emit consumes `SupportedModel`
read-only and touches only the output file. Emission cannot move a digest
because no digest is computed after emission. Live corroboration: the FU-8f
merge that landed mid-survey (d711586..b0324fd) changed plinth-support,
plinth-core constants, and docs — and not one export-path file (provenance
banner). The two work streams are physically disjoint at the file level.

---

## Part 2 — Design: the minimal honest chain

### The chain (what a first artifact requires — nothing more)

```
corpus/models/Sphere.stl
  → plinth_mesh::load_binary_stl
  → plinth_decimate::run (ratified budget)              [exists, frozen]
  → plinth_orient::run (Pinned corpus baseline)         [exists, frozen]
  → plinth_pipeline::hollow_stage                       [exists; viability OQ-1]
  → plinth_support::run_support (production limits)     [exists, seals 7/8]
  → FinalSupportAuthority::into_supported_model()       [exists, never emitted]
  → plinth_emit::write (Saturn 4 Ultra profile)         [exists, fixture-only]
  → plinth_verify format gate on the disk artifact      [exists]
  → owner opens it in ChituBox Basic                    [NEVER DONE from a sealed run]
```

Every arrow except the last two joins already compiles against the one before
it. The work is (1) the preview fallback so ChituBox will open the file at
all, (2) one integration seam proving the arrows compose on a real corpus
model, (3) the owner GUI gate, then (4) the `prepare()` loop and CLI wiring
that make it a product. Batch-first is preserved throughout: `add_models`,
`attach_supports`, `set_placements` are already batch APIs (F4), lowering is
one pass (F9), and the emitter builds one file per call with columnar record
assembly (F10). No Python appears anywhere in the chain; Python deletion is a
separate terminal phase gated on M1 parity (F5).

### Phases

**E1 — Land the preview fallback on develop.**
Cherry-pick `9ffc829` onto a `feature/chitubox-preview-fallback` branch;
reconcile with `3838aea`/`84cc6e9` context (7 lines of drift, F8); run the
five gates. Deliverable: `Project::add_models` installs the pinned 26,996-byte
native preview on first-model-add; `preview_fallback.rs` green.
*Verification:* full chitubox_io suite incl. byte-identity corpus round-trips
(the fallback fires only on empty-preview from-scratch builds, so all parsed
fixtures stay byte-identical). **3 pts. p50 1h / p90 3h.** Parallel-safe: YES
(touches `rust/chitubox_io` only).

**E2 — First-artifact integration seam (the load-bearing phase).**
New integration test + tiny example binary (suggested home:
`crates/plinth-pipeline/tests/first_artifact.rs`, since plinth-pipeline
already depends on every stage crate — F13 — and plinth-support must not be
edited) that runs the full chain above on Sphere (and one procedural model,
e.g. make_overhang_table) and writes
`<tmp>/Sphere.chitubox`, then passes it through the plinth-verify format gate.
Assert: sealed run completes, emit report record count > 0, disk audit green,
format gate green. **Do NOT pin new digests** — under concurrent FU-8f work,
completion-level assertions only (existing goldens in
`support_runner_integration.rs` already own digest pinning). Also run the
example to produce a committed-nowhere artifact file for E3.
*Verification:* the test itself + `cargo test --workspace` + five gates.
**5 pts. p50 3h / p90 8h** (risk: production `SupportPreparationLimits`
configuration and sealed-run wall-clock for Sphere — both already exercised by
census runs, so p50 is low). Parallel-safe: MOSTLY — read-only consumer of
plinth-support's public API; a concurrent gate-machinery change can alter
member counts but not break compilation of the public seam; completion-level
assertions absorb that.

**E3 — Owner GUI acceptance gate (the only semantic oracle).**
Hand the E2 artifact + checklist (load, select/move, editable supports,
slice — `05-chitubox-and-corpus.md:294`) to Craig. This is the acceptance
authority per the honesty clause (F4) and per §7.6/§11 of SPEC-CHITUBOX-RS;
no self-verify substitutes. On failure: byte-diff the refused artifact against
the nearest native specimen (F17) — this is the phase with fat tails.
*Verification:* the owner's recorded ruling.
**2 pts agent-side. p50 0.5h + owner latency / p90 8h** (failure-loop
investigation). Parallel-safe: YES.

**E4 — `plinth_pipeline::prepare()` executor.**
Implement the documented signature (F13): enumerate `*.stl` in
`input_models_dir` (deterministic order), per model run the stage chain,
produce one artifact per model in `output_dir`, assemble `PrepareReport`
(serde types already exist, `lib.rs:230-260`), honor `stop_after` and
`no_cache` (v1: `no_cache` is accepted and a cache simply doesn't exist yet —
document that honestly rather than faking one). Batch-first: per-model outer
loop is I/O-bound orchestration, not a hot path; all hot paths stay inside
the stage crates. Orientation policy: `Pinned` for corpus models,
`LegacyCoarse` default per `lib.rs:88-95` (OQ-2).
*Verification:* integration test over a 2-model folder; stop_after tests per
stage; exit-code mapping tests against `PipelineExitCode`.
**13 pts. p50 8h / p90 20h.** Parallel-safe: MOSTLY (new code in
plinth-pipeline; consumes plinth-support public API only).

**E5 — CLI execution wiring.**
The resolver seam named at `plinth-cli/src/lib.rs:33-36`:
`PrepareSelection` → `PrepareResourcePolicy::for_saturn4_ultra` →
`PrepareRequest` → `prepare()`; replace the `main.rs:20` stub; JSON report to
stdout (one line per model, matching the Python contract's JSONL spirit,
`prep_one.py:21-25`); map `PipelineExitCode` to process exit.
*Verification:* extend `tests/process.rs` with a real tiny-model run.
**5 pts. p50 3h / p90 8h.** Parallel-safe: YES.

**E6 — Multi-model plates (packing).**
Extend plinth-emit from its one-model assumption (`MODEL_NAME`/index-0,
`lib.rs:20,68`) to N `SupportedModel`s per project — chitubox_io already
handles multi-model natively (F4). Add plate packing: bbox-footprint shelf
packing inside the Saturn build volume with randomized-Y placement (per the
settled Y-sensitivity ruling: randomize over Y, don't optimize it). Names from
source stems, not the shared const.
*Verification:* asymmetric two-model plate test mirroring
`support_parity.rs`'s pattern; GUI re-acceptance of one packed plate.
**13 pts. p50 8h / p90 20h.** Parallel-safe: YES (plinth-emit + a small
packing module; no support-crate contact).

**E7 — The treant plate.** NOT export work. The treant stops inside the
specialized gate on wall-clock (FU-8f G4: EXCEEDS-TIMEBOX at 602s,
`CURRENT-STATUS.md` FU-8f row) — the export path should list it as a
downstream beneficiary, not a dependency. 0 pts here; tracked under FU-8f.

**E8 — Python retirement.**
Gated on the M1 parity table (F5, `04-migration-plan.md:148-156`): baseline
artifacts vs `plinth prepare` artifacts, as-good-or-better on every metric on
every model, then the deletion sweep (py.rs + feature + deps, python package,
maturin config, support_placement, scripts/tools pipeline entry points,
pytest suite; keep frozen baseline artifacts as evidence). The E1–E5 chain is
the prerequisite that makes this phase *possible*; the parity scoring is its
own work.
*Verification:* M1 metric table green; `grep -r python` clean in build
surfaces; five gates; frozen-baseline manifest test still green (it reads
frozen JSON, F5).
**13 pts. p50 10h / p90 24h.** Parallel-safe: NO for the deletion sweep
(repo-wide); schedule after E5 lands and during a quiet window.

> **LANDED 2026-08-08 — dated note; the phase text above is left standing as
> written.** The deletion sweep is done on `feature/python-elimination`: the
> `chitubox_io` PyO3 binding (`py.rs`, the `python` feature, optional
> `pyo3`/`numpy`, the maturin `pyproject.toml`, the `python/` package, and the
> `build.rs` whose only consumer was that binding), `scripts/support_placement`,
> `scripts/tools`, the pytest suite, and the venv. 101 `.py` files, 13 crates out
> of `Cargo.lock`. Frozen baseline artifacts kept, as specified, along with
> `scripts/tests/fixtures/` and the `prep_one.py` provenance strings.
> `crates/plinth-verify/tests/no_python_guard.rs` now enforces the end state.
>
> **Two things above did not happen the way this phase text predicted, and both
> are deliberate.** First, the sequencing: E8 landed *before* E4/E5, not after.
> Second, and more importantly, **the M1 parity table was never scored, because
> the gate it was designed to protect had already been satisfied by other
> means.** The premise of that gate was that the Python engine was still the
> oracle, so deleting it before scoring `plinth prepare` against it would destroy
> the measuring stick. That stopped being true: the parity knowledge was ported
> into Rust-native tests — `frozen_baseline`, `roundtrip`, `synth_compare` and
> the support-parity suite — which check against **committed** artifacts under
> `corpus/baseline/`, not against a running interpreter. The measuring stick is
> the frozen artifacts, and they are still here; the Python that once produced
> them is not needed to read them. Its provenance is recorded in the strings the
> sweep kept on purpose.
>
> The consequence to be honest about: **E8 landing early does not mean the M1
> as-good-or-better comparison has been made.** It has not. `plinth prepare` has
> not been scored against the baselines on every metric on every model; that work
> is still owed, it is still E4/E5's prerequisite for calling the native path a
> replacement, and it is now to be done Rust-against-artifacts. What E8 settled is
> narrower and should not be quoted as more: the Python is gone, nothing in the
> build or test surface needs an interpreter, and no coverage was lost in the
> removal.
>
> *Verification as run:* zero `.py` in the tree and no `pyo3`/`numpy` manifest
> declaration (both machine-checked by the new guard, with a negative control);
> five gates green; the frozen-baseline manifest test still green, reading the
> same frozen JSON it always did.

**(Declined) E9 — Settings-blob mapping.** Recommend NOT doing it (F16).
If ever wanted: 8 pts, blocked on owner-authored GUI specimen sweep.

### Phase / point table

| Phase | What | Pts | p50 | p90 | Parallel-safe vs plinth-support work |
|---|---|---|---|---|---|
| E1 | Preview fallback → develop | 3 | 1h | 3h | YES |
| E2 | First-artifact integration seam | 5 | 3h | 8h | mostly (public-API consumer, completion-level asserts) |
| E3 | Owner GUI acceptance | 2 | 0.5h + owner | 8h | YES |
| E4 | `prepare()` executor | 13 | 8h | 20h | mostly |
| E5 | CLI wiring | 5 | 3h | 8h | YES |
| E6 | Multi-model plates + packing | 13 | 8h | 20h | YES |
| E7 | Treant plate | — | tracked under FU-8f | — | n/a |
| E8 | Python retirement | 13 | 10h | 24h | NO (repo-wide sweep; sequence after E5) — **LANDED 2026-08-08**, ahead of E4/E5; see the dated note on the phase entry above for why the M1 gate did not apply |

Dependency order: E1 → E2 → E3 (gate) → {E4 → E5} → E6; E8 after E5.
E1, E2 can start immediately and E1‖E2 in parallel (E2's artifact for E3
should be rebuilt after E1 merges). Total to first openable artifact:
E1+E2+E3 = **10 pts**, p50 ≈ 4.5h of agent work plus one owner GUI session.

## Part 3 — Open questions (with recommendations)

**OQ-1. Does `plinth_hollow::run` complete on corpus models?** The sealed-run
evidence bypassed it via `solid_decision` (F14). *Recommendation:* E2 uses the
real `hollow_stage`; if a corpus model refuses, fall back to its typed
solid-decision path (that path is a legitimate `HollowStageOutput`, not a
cheat) and record the refusal as a finding.

**OQ-2. Orientation for non-corpus minis.** `Pinned` baselines are
Python-era artifacts; `LegacyCoarse` is the default policy
(`plinth-pipeline/src/lib.rs:88-95`) but its native implementation status
inside `plinth_orient::run` was not proven by this survey.
*Recommendation:* E4 ships with corpus-model Pinned parity as its test bed;
verify LegacyCoarse against one novel STL as an explicit E4 acceptance item.

**OQ-3. Digest pinning in the export tests.** *Recommendation:* export-path
tests assert completion + audit + format-gate, never new frozen digests
(F18, E2). Digest authority stays with `support_runner_integration.rs`.

**OQ-4. What if ChituBox still refuses after E1?** The preview asset is a
compatibility guess with direct-failure evidence behind it, not proof.
*Recommendation:* do NOT reintroduce a model-bearing template path (the
"wrong model under right supports" failure the empty-template doctrine
structurally removed, `py.rs:5-11`); instead byte-diff against
`two-supports.chitubox` and iterate on front-descriptor/preview bytes with
`acceptance_artifacts.rs` as the harness.

**OQ-5. `no_cache` semantics before a cache exists.** *Recommendation:*
accept the flag, report `no_cache` in `PrepareReport` (field exists,
`lib.rs:248`), build no cache in v1 — honest and forward-compatible.

## Compact summary

**Exists today:** chitubox_io is a complete, verified pure-Rust reader/writer
for the proprietary flat-binary `.chitubox` container (models, placements,
per-model support records with embedded meshes; byte-identity round-trips;
audit-the-file-as-written). plinth-emit already materializes sealed support
primitives into printable frustum/sphere/raft triangle meshes and writes +
disk-audits one project per model against the M0 Saturn 4 Ultra profile. The
parked preview branch is one small salvageable commit (native preview
fallback) that develop needs before ChituBox will open a from-scratch file.

**Biggest gap:** not geometry — **composition**. No code path has ever run
seal → `into_supported_model` → `plinth_emit::write` on a real model;
`plinth_pipeline::prepare()` is a documented signature with no body and the
CLI stops at its parser. First openable artifact = E1 (preview fallback,
3 pts) + E2 (integration seam, 5 pts) + E3 (owner GUI gate, 2 pts); the
product path adds E4 executor (13) + E5 CLI (5) + E6 plates (13), with Python
retirement (13) gated on M1 parity. Export work is downstream of all frozen
digests and file-disjoint from the active plinth-support gate work — proven
live by the FU-8f merge that landed mid-survey touching zero export-path
files.

---

## E2 LANDED 2026-08-07 — the first end-to-end composition

*Everything above this line is the original survey and is left standing
verbatim. This section records what happened when E2 was executed, including
the two places where measurement contradicted the survey's expectations.*

```
Branch:   feature/export-first-artifact (E1 landed at 18aef8e)
Base:     develop @ b0324fd
Artifact: exports/sphere-first-artifact.chitubox (gitignored run output)
```

### What landed

Three files, all test/example scope; **no library code changed anywhere**:

| Path | What |
|---|---|
| `crates/plinth-pipeline/examples/shared/first_artifact_chain.rs` | the composition, shared verbatim by the test and the example |
| `crates/plinth-pipeline/tests/first_artifact.rs` | the integration gate (2 tests) |
| `crates/plinth-pipeline/examples/first_artifact.rs` | writes the owner's E3 artifact into `exports/` |

`chitubox_io` and `plinth-verify` became **dev-dependencies** of
`plinth-pipeline`; its library boundary still imports only `plinth-emit`'s
report types, exactly as F13 describes.

The shared module lives under `examples/shared/` because cargo's example
auto-discovery only claims `examples/*.rs` and `examples/*/main.rs`, so a file
one directory down is not a build target. Both consumers reach it by `#[path]`.
The point is that the artifact handed to the owner and the artifact the test
gates are produced by the same code, not by two copies that drift.

### F12 is closed

`run_support` → `FinalSupportAuthority::into_supported_model` →
`plinth_emit::write` now runs on real models. The corpus Sphere produces a
416,256-byte `.chitubox` with 47 support records, and the emitted file survives
an independent re-read.

The `SupportedModel` handed to `plinth-emit` came from a sealed run whose
snapshot digest is **17667468971446251870** — bit-for-bit the frozen OD-8
Sphere digest. That is F18's downstream claim demonstrated rather than argued:
the export path consumed the identical sealed plan the census measures. It is
deliberately **not** asserted in the test (OQ-3: digest authority stays with
`plinth-support`'s own goldens); it is recorded here as evidence.

### What the artifact contains, re-derived from the bytes on disk

| Quantity | Value |
|---|---|
| model triangles | 4,760 |
| support records | 47 |
| by wire type | TAPER 7, JOINER 8, FLARE 6, PAD_OR_BRIDGE 11, TIP 7, SHAFT 8 |
| frustum / sphere / raft families | 21 / 15 / 11 — all three exercised |
| outward-wound record meshes | 47 of 47 |
| smallest record signed volume | 0.7064 mm³ |
| model mesh signed volume | 4,174.75 mm³ |
| embedded preview | 26,996 B, byte-equal to E1's native fallback asset |
| `plinth-verify` format gate | passed |
| file size | 416,256 B |

Winding is re-derived from the binary32 vertices *on disk*, not trusted from
the producer: for a closed surface the signed volume is translation invariant,
so a strictly positive value is exactly the outward-winding property
`wind_outward` enforces, independently recomputed after a round trip through
the container.

### OQ-1 is answered, and the survey's guess was wrong in an interesting way

OQ-1 asked whether `plinth_hollow::run` completes on corpus models, and
recommended falling back to the typed solid decision *if a model refuses*.

**It does not refuse.** The corpus Sphere hollows, plans drains, and drills
successfully, returning `AwaitingInternalSupport` — a proved drilled shell
paired with the untouched oriented solid.

**The refusal is one stage later.** The sealed support run rejects that
14,526-face drilled shell during `Preparation`
(`max_grid_bytes=67108864`, `knn_max_work=100000000`). So the drilled hollow is
*producible but not supportable* by today's engine. Recorded as **FU-28**.

The seam therefore tries the real thing and reports which geometry won, as a
typed `SupportedGeometry` value rather than a comment:

1. `HollowStageDecidedSolid` — support planning consumed the hollow stage's own
   output; no fallback involved. This is `make_overhang_table`.
2. `DrilledShellSealed` — the drilled shell sealed. Nothing reaches this today.
3. `DrilledShellRefusedBySupport { drilled_faces, refusal }` — the Sphere.
   The oriented-solid reserve carried the model instead.

Three honesty notes on that third path, since it is the one that ships:

- The fallback uses the **same** `solid_decision(BelowThreshold)` handoff that
  every existing sealed-run gate and the entire OD-8 census already use. That
  is F14's caveat, inherited deliberately, so the Sphere artifact sits on
  exactly the same evidentiary footing as the seven frozen digests — not on a
  new and weaker one.
- It is reported, not hidden: the enum variant carries the drilled face count
  and the runner's own refusal string with the caps it was measured against.
- The two alternatives were rejected as fabrications. `into_solid_fallback`
  stamps a named **A5** refusal code into the permanent record and A5 never
  ran; `approve_hollow` asserts A5 approved. A5 internal-support planning does
  not exist anywhere in this workspace.

The test pins the third variant by equality, so the day the drilled shell
becomes supportable — or the hollow stage changes its verdict — the gate fails
and says so, rather than silently changing what the owner is handed.

### What is deliberately not asserted

No new frozen digest, per OQ-3. Concurrent `plinth-support` gate work
legitimately moves member counts; every assertion here is a completion,
structural or self-consistency property that a changed member count cannot
break. `support_runner_integration.rs` keeps digest authority.

### Still open

E3 remains the only semantic oracle. A structural pass is not a claim that
ChituBox will load the file (`chitubox_io/src/lib.rs:14-16`), and this artifact
is the first from-scratch project the native engine has ever produced from its
own sealed plan. On refusal, follow OQ-4.

## Addendum 2026-08-07 — corpus sweep landed

`examples/first_artifact.rs` was extended with a `--all` mode that runs the
same `first_artifact_chain` composition over every frozen-digest corpus model
except the Weeping Treant (its support tournament cannot yet complete). Five
of the seven reached a written, re-parsed, format-gate-passed `.chitubox`;
each was run twice and the bytes were byte-identical both times. Two refuse
at the hollow stage, before support planning is ever reached — reported below
rather than worked around.

**Per-model stats** (model triangles / support records = the reopened file's
own counts, not producer state; sha256 over the full artifact bytes):

| model | triangles | support records | size (bytes) | sha256 |
|---|---:|---:|---:|---|
| Sphere | 4,760 | 47 | 416,256 | `a8bfa4d051c4ac811e5a5a16bcd1d9e7c0b704431a2b9e2e8f115c5b3d62691a` |
| make_overhang_table | 60 | 544 | 2,826,024 | `9aea36488f308f3f6c3b4b0eff60d726148dd71be6902da409d226d303c4b504` |
| make_toed_slab | 140 | 407 | 2,161,824 | `0eebfab356c9dfcac0563eca4ea369f262065eb77d1e3636bbb8b408ec8e7076` |
| make_raking_blade | 24 | 202 | 1,091,544 | `1d27dccb435fe6d7bc691599e78e8a18bfa71748aeee0e6ee34ef886c2c3a6c4` |
| make_thin_fin | 24 | 109 | 560,688 | `b73377981c3ecebb421f75702912cf4741bffba224a37cfea969f81675f94c1a` |
| Cylinder | — | — | — | refused before emit (below) |
| make_box | — | — | — | refused before emit (below) |

Sphere's bytes are identical to the E2 artifact already checked in at
`exports/sphere-first-artifact.chitubox` — the sweep did not change it, only
reproduced it.

**Refusal finding.** `Cylinder` and `make_box` both refuse at
`plinth_hollow`'s cavity check, before the composition ever reaches
`run_support`:

```
hollow: native shell did not contain one negative cavity enclosed by
positive outer material
```

This is the real `hollow_stage` decision path (`HollowRequest` /
`HollowStageOutput`), not the OD-8 census's shortcut — the census forces
every model through `HollowedModel::solid_decision(BelowThreshold)` directly
and never calls the cavity check at all, which is why both models still
produce a frozen OD-8 digest despite refusing here. No fix was attempted:
`plinth-hollow`'s cavity decision is out of this task's scope, and the
instruction was to report a refusal, not work around it. Filed as a finding
for follow-up, not a blocker for the five artifacts that did emit.

**OD-8 census cross-check.** `PLINTH_CENSUS=1` census runs (which bypass the
hollow-stage cavity check) reproduced all seven frozen digests unchanged,
Cylinder and make_box included — confirming the two hollow-stage refusals are
new information from this sweep's deeper chain, not a regression in anything
the census already gated.
