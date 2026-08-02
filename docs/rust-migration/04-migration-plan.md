# 04 — Migration Plan (NORMATIVE)

**Execute the revised critical path below.** The former M0–M9 DAG remains in this
document as detailed acceptance/reference material and a post-elimination backlog;
O-5 through O-8 supersede its sequencing, compatibility-retention, and
duplicate-implementation mandates.

Read `README.md`, `01-current-system.md`, `02-target-architecture.md` and
`03-physics-and-constants.md` first. Read `06-conventions.md` before your first commit.
`05-chitubox-and-corpus.md` is the format authority for every emitted `.chitubox`.

---

## Owner-approved critical path — O-5 through O-8 (2026-08-01)

The owner has prioritized Python elimination over simultaneous redesign. One
implementation owns each capability: `geom_kernel` owns spatial geometry;
`plinth-mesh` owns `Mesh`, topology, and delicacy; `plinth-metrics` is a disk-first
adapter using them. Adapters and explicit boundary conversions are allowed; second
algorithms are not. Batch-first remains law.
Frozen disk artifacts, not a runnable Python engine, are the oracle. Each Python,
PyO3, maturin, shim, and legacy Python-only control is deleted as soon as its native
Rust boundary lands. Protected error-bounded decimation is the first production stage;
its print-scale mesh is the sole mesh consumed downstream, and any orientation proxy
derives from it.

Execute this path in order:

1. **Consolidate.** Reconcile the existing Rust work around the single-owner boundaries;
   retain the frozen Python baseline and disk-first scoring.
2. **Native kernel/mesh and compatibility removal.** Complete only the batched geometry
   and mesh capabilities the pipeline needs; immediately remove their Python/PyO3/maturin
   and shim surfaces plus `GEOM_KERNEL_THREADS`.
3. **Protected decimation first.** Implement error-bounded, feature-protecting production
   decimation. Its print-scale output is the sole production mesh for every later stage;
   any additional orientation proxy is derived from it.
4. **Port current downstream preparation.** Reproduce orientation, hollowing, and drilling
   against that decimated production mesh without otherwise redesigning their policy.
5. **Port current support.** Reproduce the current support and raft behavior, fixing only
   the enumerated fix-don't-reproduce defects.
6. **Emit and CLI.** Produce one loadable supported-and-rafted `.chitubox` per input model
   through the single Rust command and audit the freshly reopened file.
7. **Accept and finish deletion.** Require essential artifact parity, ChituBox loadability,
   deterministic outputs/reports, and the owner's real print checkpoint; remove any remaining
   Python orchestration/tests and re-run those gates from a clean checkout with zero Python.
8. **Post-Python enhancements.** Only after deletion, pursue the dense orientation search,
   extra proxy tiers, CVT/lattice and load-model redesign,
   forest/repair redesign, calibration, GPU abstraction, and other quality improvements.

The following former requirements are explicitly superseded:

| Former detail | O-5–O-8 resolution |
| --- | --- |
| M1 requires independent geometry inside `plinth-metrics` | `plinth-metrics` uses the sole `geom_kernel` and `plinth-mesh` implementations after reopening files from disk; analytic, corrupt, and native fixtures supply independent evidence |
| M3 makes decimation late or optional while deferring protected QEM | O-8 makes protected, error-bounded production decimation the first production stage; only extra orientation proxy tiers are post-Python work |
| M4 deliberately emits naive throwaway supports before the real solver | Do not build a throwaway support algorithm; port current support/raft behavior once on the elimination path |
| M5–M8 redesigns and enhancements block deletion | Only behavior essential to reproduce the current pipeline and pass acceptance blocks deletion; the listed redesigns and enhancements move to step 7 |
| Python/PyO3 survives until M9 | O-7 removes each compatibility surface at its native handoff; the final gate proves zero Python after essential parity, loadability, determinism, and owner-print gates pass |
| M2 requires a premature `SurfaceOracle` abstraction | Add an abstraction only when a second real backend is implemented after Python deletion |

No numeric point estimate is normative here. The detailed material below continues to
define useful metrics, fixtures, hazards, and desired end-state behavior wherever it does
not conflict with this override.

---

## 0. Why this migration exists (state this, do not re-litigate it)

The goal is **elimination of all Python from this repository**, not speed. The owner's
operative rationale, near-verbatim:

> eliminate all python because it cripples Claude's ability to think well … every
> exposure to python actively destroys productivity.

The public Python corpus is dominated by unprofiled notebook and glue code, so it trains
coding agents into scalar-loop, unbatched habits. Removing Python from this repo is a
guardrail on all future agent work here, and it is why "just make the Python faster" is
not an acceptable outcome at any milestone. TypeScript/bun is permitted **only** if you
write down a concrete blocker naming what Rust cannot do; the expectation is pure Rust.

Speed is a welcome side effect (Python runs 86–872 s per model, median ~402 s), and it is
what makes a real orientation search affordable — but it is not the acceptance criterion.

### The acceptance criterion is NOT byte-identical output

The criterion is the owner's: **"rust is producing as-good-as or better output in chitubox
files"**, and *better* has a subjective element. Concretely:

1. Freeze the Python pipeline's outputs over the corpus **once**, as BASELINE artifacts (M0).
2. Score baseline artifacts and Rust artifacts with the **same objective metrics** (M1).
3. Gate on Rust being **as-good-or-better on every existing M1 metric on every model**
   at the revised acceptance/delete step.
4. At each milestone, hand the owner a named **eyeball checkpoint** for the subjective part.

### Executable essential parity for acceptance/delete

The acceptance/delete gate uses the already-ratified M1 table, not a reduced or
new metric set. For every baseline-success artifact, compare these 15 IDs in this
order: `islands`, `cavities`, `boundary_edges`, `negative_shells`, `winding`,
`max_x`, `support_count`, `resin_model`, `resin_support`, `resin_total`,
`scar_area`, `scar_worst_patch`, `scar_weighted`, `z_height`, `loadable`.
All numeric metrics are lower-is-better; `loadable` is pass/fail. The candidate
must be as-good-or-better on every row. For a corpus member where Python failed,
any complete Rust success wins on every metric, as the frozen manifest already records.

The revised path collapses the old milestone timing for existing hard bounds into
the acceptance/delete gate. At that gate the exact candidate hard bounds are
`boundary_edges = 0`, `negative_shells = 0`, `winding = 0`, `max_x ≤ 12`, and
`loadable = pass`. `cavities` remains a relative not-worse metric, not a newly
invented absolute bound. The candidate must also pass the ported current verifier
safety gates: uncovered islands, clearance, load, unsupported minima, island births,
regular-contact invariants, unrouted specialized contacts, connectivity,
printability, Murray, ball breach, and the engine-side physical-contact invariant.

Both sides are artifacts freshly reopened from disk; no repair may occur between
emit and audit. The owner must load the Rust artifact in ChituBox and print one model.
Only redesign-dependent additions such as `max_x_group`, dense orientation search,
and calibration are excluded until post-Python work; no existing M1 metric or current
verifier evidence is excluded.

**Byte-level identity applies in exactly one place, and nowhere else:** the `.chitubox`
record structure round-trip. That is a real format contract — ChituBox Basic 1.3.0 must load
the file. See `05-chitubox-and-corpus.md` and the existing suites in
`rust/chitubox_io/tests/`. (README.md §3, `02-target-architecture.md` §1 and
`06-conventions.md` §6 say the same thing; if you find a fourth count anywhere, it is a bug
in this guide — report it.)

M2's STL gate is **not** a file-byte gate and must not be written as one:

| Gate | What it actually asserts |
| --- | --- |
| STL read → write → read (M2) | **Array identity.** The vertex array, the triangle index array and their **order** are bit-identical across the round-trip: f64 coordinate bits preserved, no implicit weld, no reorder, no f32 downcast. The 80-byte free-form header, the per-facet normal vectors and the 2-byte attribute count are explicitly **not** part of the contract — a correct f64 loader/writer is under no obligation to reproduce them, and gating on them would fail for reasons unrelated to welding, reordering or downcasting. |

Everywhere else, "as-good-or-better on the metric" is the law. Do not write a diff-the-bytes
gate against the Python output; you will fail it for the wrong reasons and then loosen it.

### Four rules that bind every revised critical-path checkpoint

1. **Disk-oracle, incremental deletion.** Frozen artifacts remain the measuring stick.
   A runnable Python engine is not preserved for its own sake: delete each compatibility
   surface when its native boundary lands, then prove zero Python at the final gate.
2. **No new Python.** Not a script, not a helper, not a `conftest` addition. Reading the
   frozen Python is allowed and expected; writing it is not. The sanctioned exception is
   the **two** one-off `python -c` invocations in M0 — the `resolve_stl()` corpus probe in
   the owner prerequisite, and the synthetic-fixture STL export in work item 6. Neither may
   be committed as a `.py` file, and there is no third. (`06-conventions.md` §8 carries the
   same carve-out; the two must stay in sync.)
3. **Never bridge via PyO3 for comparison.** Both sides emit files; `plinth score` compares
   artifacts on disk. A comparison that runs Rust *through* Python measures the bridge.
4. **Gates measure the file AS WRITTEN TO DISK, with no repair pass.** This project once
   validated an in-memory mesh while shipping a different one. Every audit re-opens the
   artifact from the filesystem and re-derives its verdict from those bytes.

### Git discipline (binding, repeated from `06-conventions.md`)

Commit **per revised critical-path checkpoint** on `master`. Never use `git checkout --`, `git restore`,
`git reset --hard`, or `git clean` — they silently destroy uncommitted work. Never
force-push. Under O-6, Python deletion happens at the revised acceptance/delete step;
the old M7/M8 precondition chain below is superseded.

---

## 1. Former M0–M9 DAG (reference and post-elimination backlog)

The diagram and edge table in this section are retained for their dependency analysis,
not as the current execution order. The owner-approved critical path above controls.

```
M0 ──┬── M1 (parity harness) ─────────────┐
     │                                    │
     └── M2 (mesh/voxel) ──┬── M3 (decimate) ───────────┐
                           │                            │
                           └── M4 (vertical slice) ── M5 ── M6 ── M7 ──┬── M8 ── M9
                                                                       │    ↑
                                            (M1 gates M4+; M3 feeds M8)─┘────┘
```

| Edge | Meaning |
| --- | --- |
| M0 → M1, M0 → M2 | **Former edge:** baseline and shared-type dependency analysis only; revised steps 1–2 govern. |
| M1 ∥ M2 | **Former edge:** the harness/mesh parallelization is reference only. |
| M3 ∥ M4 | **Former edge:** O-8 supersedes this parallelism; protected production decimation blocks M4. Only extra L1/L2 orientation proxies and dense search are post-Python. |
| M1 → M4 | **Former edge:** retain the useful rule that disk scoring precedes emitted-artifact acceptance, without restoring old milestone order. |
| M4 → M5 → M6 → M7 | **Former edge:** the redesign chain is post-Python and does not block deletion. |
| M3 → M8 | **Former edge:** dense orientation depends on enhanced proxies after deletion. |
| M7 → M8 → M9 | **Former sequence only; O-6 supersedes it.** M8 redesign/calibration is post-Python and does not block the revised acceptance/delete step. |

**Former parallelization note, not current sequencing:** the old plan allowed (M1, M2)
and then (M3, M4) in parallel and chained M5/M6/M7. Execute the revised critical path
instead.

---

## 2. Milestone summary

| # | Name | Depends on | New crates | Gate |
| --- | --- | --- | --- | --- |
| Former M0 | Conventions, frozen-Python baseline, core types, solid spike | Historical only | `plinth-core`, `plinth-solid` | Useful completed foundation |
| Former M1 | The parity harness and Rust refreeze (`plinth score`) | Historical only | `plinth-metrics`, `plinth-cli` | M1 metric table remains the acceptance vocabulary |
| Former M2 | Mesh and geometry substrate | Historical only | `plinth-mesh`, `plinth-voxel` | Reuse completed substrate under revised steps 1–2 |
| Former M3 | Protected production decimation | Revised step 3; first production stage | `plinth-decimate` | Error-bounded feature protection blocks every downstream stage; only extra proxies are deferred |
| Former M4 | The thin vertical slice | Revised steps 4–6, after decimation | `plinth-cost`, `plinth-orient`, `plinth-hollow`, `plinth-emit`, `plinth-pipeline` | Port current downstream behavior once; no throwaway supports |
| Former M5 | Perception, placement, load model | Post-Python redesign | `plinth-support` | Does not block deletion except current behavior needed by parity |
| Former M6 | Forest, growth, sizing | Post-Python redesign | (extends `plinth-support`) | Does not block deletion except current behavior needed by parity |
| Former M7 | Verify + repair complete | Post-Python redesign | `plinth-verify` | Port current verifier safety behavior before deletion; redesign later |
| Former M8 | Search quality + calibration | Post-Python | (extends existing) | Enhancement work; pipeline determinism still blocks deletion |
| Former M9 | Final zero-Python proof | Revised step 6 plus executable essential parity | — | Compatibility deletes at native handoff; clean-checkout gate still re-fires |

---

## 3. The metric set (defined once — M1 builds it, every later milestone uses it)

Every metric is computed by `crates/plinth-metrics` on an artifact read from disk. Ids are
stable and are what you pass to `--allow-worse`.

| Id | Metric | Direction | Hard bound | Notes |
| --- | --- | --- | --- | --- |
| `islands` | Unsupported overhang islands remaining in the emitted scene | ↓ lower better | — | Recomputed from the reopened artifact through the sole geometry/mesh implementations, not read from a producer report |
| `cavities` | Closed interior cavities in the emitted solid union | ↓ | — | The defect ChituBox surfaced four times from coincident coplanar faces |
| `boundary_edges` | Edges with exactly one incident face | ↓ | **= 0** | Absolute at revised acceptance/delete; old M4 timing is historical |
| `negative_shells` | Connected shells with negative signed volume | ↓ | **= 0** | Absolute at revised acceptance/delete; old M4 timing is historical |
| `winding` | Faces whose winding disagrees with their shell | ↓ | **= 0** | Absolute at revised acceptance/delete; old M4 timing is historical |
| `max_x` | Max **per-arm** X = A_voronoi_cell / (π·r_tip²) | ↓ | **≤ 12** | **Definition pinned at M1 and never changed:** per-arm, exactly the `placement.load_partition` semantics in `03-physics-and-constants.md` §2.3. Relative not-worse plus the candidate absolute bound apply at revised acceptance/delete |
| `max_x_group` | Max arm-group effective X under the resolved `LoadSharing` | ↓ | — | New redesign-dependent metric excluded from acceptance/delete; add post-Python without redefining `max_x` |
| `support_count` | Number of support records | ↓ | — | |
| `resin_model` | Model resin, ml | ↓ | — | |
| `resin_support` | Support resin, ml | ↓ | — | |
| `resin_total` | Total resin, ml | ↓ | — | |
| `scar_area` | Total scar area, mm² | ↓ | — | Invariant ≈ A_face/X at fixed X; the lever is *where* |
| `scar_worst_patch` | Largest single scar patch, mm² | ↓ | — | |
| `scar_weighted` | Delicacy-weighted scar area | ↓ | — | The subjective-quality proxy |
| `z_height` | Print z-height, mm | ↓ | — | Drives time and layer count |
| `loadable` | ChituBox loadability | pass/fail | **pass** | Structural re-parse; see `05-chitubox-and-corpus.md` |

**Group aliases** accepted by `--allow-worse`: `scar` = {`scar_area`, `scar_worst_patch`,
`scar_weighted`}; `resin` = {`resin_model`, `resin_support`, `resin_total`}; `format` =
{`boundary_edges`, `negative_shells`, `winding`, `cavities`, `loadable`} — `format` is
listed for completeness and **must never be waived**.

**Waiving a component implies waiving its total.** `resin_total = resin_model +
resin_support` by construction, so any `resin_support` regression large enough to need a
waiver necessarily makes `resin_total` worse too. Waive the `resin` group, or name both
ids — never `resin_support` alone. The group aliases exist precisely to stop this.

**On the revised path, all existing hard bounds are enforced on the CANDIDATE at
acceptance/delete:** `boundary_edges = 0`, `negative_shells = 0`, `winding = 0`,
`max_x ≤ 12`, and `loadable = pass`. They are never applied to the baseline — the frozen Python output
violates several by construction (the `tip_r_max_mm = 1.0` cap makes `max_x ≤ 12`
unreachable on a large flat down-face; see `03-physics-and-constants.md` §1.5(ii)) — and
never to a self-comparison. Without this rule M1's own acceptance
(`--baseline corpus/baseline --candidate corpus/baseline` → exit 0) could not pass.

**Python-failure rule.** `corpus/manifest.json` records each model's Python exit code. If
Python failed on a model, any Rust success on that model beats it, on every metric, at
every milestone. Do not exclude those models from the corpus; they are the easiest wins.

---

## M0 — Conventions, frozen-Python baseline, core types, solid spike

**Goal.** The measuring stick is frozen by the existing Python pipeline before any Rust output
exists; the passive types every crate shares exist, and the biggest schedule risk (the boolean
kernel) is spiked. Rust does not refreeze a baseline in M0.

**Former M0 dependency (reference only):** none; execute revised critical-path step 1.

**M0 constant-inventory boundary.** Inventory every live literal/default that can
change emitted geometry, printer framing/compatibility, a physical model,
acceptance/gate behavior, metric meaning, or deterministic tuning/search. Search
and repair caps that alter output are included. Exclude binary offsets/magic/schema,
test-only fixture values, explicitly retired/no-port behavior, and demonstrated
result-invariant resource/memory guards. `PLINTH_THREADS` remains documented runtime
control, not a ledger constant. M0 closes membership; M7 completes consumer
migration. Do not create the M1 decision gate table during this checkpoint.

### Owner prerequisite — read this before starting

**Historical completed-freeze instruction.** This bootstrap was used for the one M0
freeze. Under O-7, do not repeat or maintain it after the relevant native handoff; frozen
disk artifacts and native fixtures replace the runnable extension as oracle evidence.

First, the venv. Every `uv run --frozen` and every `@pytest.mark.kernel` test in this plan
needs `.venv/` (gitignored, so absent from a fresh checkout) with the compiled PyO3
extensions in it. Bootstrap once, before the commands below:

```
uv sync                                              # creates .venv from uv.lock
.venv/bin/python -m pip install "maturin==1.8.3"     # pinned; needs network
scripts/build_kernel.sh                              # builds geom_kernel + chitubox_io into .venv
```

`corpus/models/` must contain the exact eight-model local input set. **The Treant model exists.** As of 2026-07-31 the
owner confirms that **"treant" and "Awakened Tree" are the same model** — the source STL is
`Awakened Tree.stl` (21 MB), sitting beside the repo at

```
/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Awakened Tree.stl
```

An earlier pass recorded this file as absent everywhere; that search was for the string
`Weeping Treant` and therefore missed a file sitting in a directory it had already looked
in. Do not repeat that search — look for the **Awakened Tree** name.

`scripts/tools/_toolpaths.py::resolve_stl()` honours `$TREANT_STL`, then the repo root, then
the repo's parent directory. It does **not** descend into `minis-to-process/` and it does not
know the alternate name. **`minis-to-process/` is transient staging — the owner clears it after
printing.** Copy the source to the durable canonical home first, then set `$TREANT_STL`
explicitly; also copy it to the ignored local model-input set:

```
DIR="${CHITUBOX_CORPUS_DIR:-/Users/craig/code/hughescr/dnd-campaigns}"
cp "/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Awakened Tree.stl" \
   "$DIR/Weeping Treant v1.2.stl"
export TREANT_STL="$DIR/Weeping Treant v1.2.stl"
cp "$TREANT_STL" "corpus/models/Weeping Treant v1.2.stl"
```

The durable home is the same out-of-repo directory that holds
`treant_v5_candidate.chitubox` — `$CHITUBOX_CORPUS_DIR`, defaulting to the repo's parent
(`05-chitubox-and-corpus.md` §4). Its canonical path is
`${CHITUBOX_CORPUS_DIR:-/Users/craig/code/hughescr/dnd-campaigns}/Weeping Treant v1.2.stl`.
`TREANT_STL` points to that durable canonical copy;
`corpus/models/Weeping Treant v1.2.stl` is the ignored local copy for the exact input set.

**Never commit it or any other model input.** All `corpus/models/*.stl` files are
ignored local inputs. `corpus/manifest.json` and the frozen artifacts under
`corpus/baseline/` are committed; those are the reproducibility record and measuring stick.

Verify whichever route you took with:

```
uv run --frozen python -c "import sys; sys.path.insert(0,'scripts'); from tools._toolpaths import resolve_stl; print(resolve_stl())"
test -f "$(uv run --frozen python -c "import sys; sys.path.insert(0,'scripts'); from tools._toolpaths import resolve_stl; print(resolve_stl())")"
```

Note that `resolve_stl()` **never raises** — it falls through to
`return os.path.join(REPO_ROOT, name)`, a path that does not exist. So test the printed
path with `test -f`; a successful print proves nothing.

**M1's gate and every gate from M4 onward require `$TREANT_STL` to be set** (the file is
available — see above), not just the freeze. M1's parity gate is "the harness scores the
frozen baseline without error"; M4–M8 all take `--baseline corpus/baseline`; M9 re-fires
M7's gate. This is a **configuration** step, not a missing asset. Do not substitute a
different model silently.

**Second hazard on the same file: it may be refused rather than missing.** A run recorded
2026-07-25 had `Awakened Tree.stl` fail immediately with `PreHollowedError`, exit code 2 and
no output — `2 closed shell(s) sealed inside another component's material (largest: 9096
faces, 55.3067x24.3381x64.281mm)`. The model was hollowed at some earlier point and never
drilled, and `hollow.py` cannot process a pre-hollowed input because hollow and drill are one
welded stage (**DEFECT D-46**, `01-current-system.md` §11). **Check for this before assuming
the freeze will run**, and if it fires: the guard is correct, do **not** delete it, and do
**not** reach for `--no-hollow` (that emits an undrilled hollow model — sealed liquid resin,
a suction and burst risk during peel). Either source a solid export of the model, or take the
provisional-baseline route below and clear the flag once D-46's stage split lands at M4/M5.

**Sanctioned fallback if the file still cannot be placed:** freeze a *provisional*
baseline over exactly `Sphere`, `Cylinder`, `make_box`, `make_overhang_table`,
`make_toed_slab`, `make_raking_blade`, and `make_thin_fin` — **seven models, no
Cone** — and record
`"provisional": true` in `corpus/manifest.json`, and proceed through M1, M2 and M3 against
it. **M4 may not be called done on a provisional corpus**, because the Treant is the model
every physics constant in this repo was tuned against and a baseline without it is not the
corpus the gate assumes. Re-freeze in full when the Treant lands, and clear the flag.

The other source models sit beside the repo in `minis-to-process/` (transient staging — the
owner clears it after printing, so copy anything you need out). `Morel Mudskipper.stl` is
the other model the existing code and fixtures reference by name and is the highest-value
addition after the Treant. **`Awakened Tree.stl` is not a separate model — it *is* the
Treant** (see above); do not add it to the corpus twice under two names.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/support_placement/chord_canon.py` | Semantic algorithm and report intent ported → `plinth-core::chord`; invalid-input and schema defects are deliberately hardened, including computed `matches` and no `ok` wire key. |

(`scripts/support_placement/config.py` **begins** its conversion here and is retired at M7;
see the audit table in §M9.)

### Rust deliverables

**Workspace surgery.**

- Rewrite root `Cargo.toml` as a **virtual workspace** (no `[package]`, no `[[bin]]`):
  `members = ["crates/*", "rust/chitubox_io", "rust/geom_kernel"]`.
- Delete `src/main.rs` (it is a stub that prints `not yet implemented`). Delete `src/`.
  The `plinth` binary is reborn in M1 as `crates/plinth-cli`.
- Add `[workspace.package]` and `[workspace.dependencies]` per `06-conventions.md` so
  every crate pins one rayon, one serde, one thiserror.

**`crates/plinth-core`** — the vocabulary every other crate speaks. No geometry, no IO.

| Module | Contents |
| --- | --- |
| `defect.rs` | `DefectReport`, `Defect`, `Severity`, `SiteId`, `Termination`. **`DefectReport` is the sole acceptance authority** — commitment 1. Nothing else may say "ok". |
| `violation.rs` | The passive exhaustive `ViolationKind` vocabulary. Separately, core's passive ID vocabulary includes exhaustive `FixerId`, because `Termination` serializes it; `plinth-support::repair` later re-exports it. Only `Response`, `response_for`, and concrete fixer execution remain M7 `plinth-support::repair` behaviour. Commitment 3 is enforced there. |
| `provenance.rs` | Exactly four wrappers: `Measured<T> { value, source: EvidenceSource, err, note }`, `Derived<T>`, `Placeholder<T>`, `Pending<T>`, plus `AcceptSafe`. `EvidenceSource` is tagged and carries repo record/locator plus `EvidenceDate::{Exact, OnOrBefore, Unknown}`; no direct plate/date fields remain on `Measured`. `FiniteInterval` carries finite ordered endpoints and explicit inclusive/exclusive `BoundKind`. Source and err are required; interval/categorical values serialize explicit `"err": null`; duplicate/unknown fields fail. Commitment 2. |
| `constants.rs` | M0 implements the typed constants registry: declarations, a fallible batch-built `LazyLock`, an independent expected-ID Rust test, and the generated `constants.md` golden test. The hand-maintained ledger is additional source-membership/disposition evidence, never a replacement or golden input. Named milestones migrate consumers; M7 closes every remaining consumer. |
| `layer.rs` | `LayerPlan` — layer height is a **per-layer array from day one**, commitment 5. Ships `constant(0.050)` until calibrated. |
| `frame.rs` | Frame typestate marks (`Source`, `Centered`, `Plated`) so a mis-framed point set will not compile. |
| `stage.rs` | The `Stage` trait every pipeline step implements. |
| `digest.rs` | Content digests over typed arrays (not over allocator arithmetic — see the M2 note on `topology.py`). |
| `chord.rs` | `canon_chord_micron`, `chords_match_micron`, `chord_parity_report`, ported from `chord_canon.py`'s executable behavior. Integer-micron ties-to-even canonicalization; dependency-free by design. Invalid-input and schema defects are deliberately hardened: its computed diagnostic parity key is `matches`, never `ok`, and it is never an acceptance decision. |
| `printer.rs` | `PrinterProfile`. Ship `saturn4-ultra`: known vendor-order volume 211.68 × 118.37 × 220 mm and pitch 14 × 19 µm mapped to project X/Y/Z and X/Y; local `leading_release_edge = NegativeY`, `release_front_sweep_direction = PositiveY` (front moves `-Y → +Y`); layer 0.050 mm and `LIFT_MM = 5.0` are local defaults, not vendor claims. |
| `cost.rs` | Passive shared cost/report DTOs: `CostVector`, `Fidelity`, `Resin`, `Scar`, `Risk`, `OrientationCost`, `ParetoPoint`, `OptimizationSummary`, and `SearchEvidence`. Pre-orientation reports use `SearchEvidence::NotEvaluated`, never a sentinel cost. Evaluation, scalarization, batching, Pareto algorithms, and memoization remain future `plinth-cost` behaviour. |
| `metric.rs` | Passive shared `MetricId`, `MetricValue`, `MetricSet`, and report DTOs. Metric directions, disk-first metric orchestration, comparison, and waiver behaviour remain `plinth-metrics`; geometry and mesh capabilities are reused under O-5. |

**`crates/plinth-solid`** — a **spike only** at this milestone.

- `trait SolidBoolean { fn union(..); fn difference(..); }`
- One backend: `cxx` binding to a **vendored** C++ manifold (f64 kernel — commitment 6).
- Prove exactly two operations: one union, one difference, each asserting watertightness.
  Do not build the audit or the emit integration yet; that is M4.

### Work items

1. Commit the `docs/rust-migration/` guide (this file and its siblings) before writing code.
2. Workspace surgery as above; `cargo build --workspace` must succeed with the legacy crates and
   the M0 crates present.
3. Write `plinth-core` modules in the order listed — `defect` → `violation` → `provenance`
   → everything else. `violation.rs` must exist before any gate anywhere.
4. **The trybuild proof.** Add `crates/plinth-core/tests/compile_fail/` with a case that
   routes a `Placeholder` constant onto the accept path and a `trybuild` harness that
   asserts it fails to compile. This is the enforcement mechanism for commitment 2; without
   it the provenance types are decoration.
5. **Implement and prove the typed registry, then close the source inventory.** Implement
   `constants.rs`; declare the M0 constants; construct the registry in one fallible batch;
   test exact membership against an independent hand-maintained expected-ID set; and render
   and diff generated `docs/rust-migration/constants.md` in the M0 golden test. Also commit
   `docs/rust-migration/m0-constants-source-ledger.json` as a closed inventory: every
   live output/physical/gate/metric/search literal accounted as registry, alias,
   retired, or excluded, and every registry row pointing to a normative locator in
   `03-physics-and-constants.md`. The ledger is hand-maintained and never generated.
   The ledger is an additional fourth artifact and never golden input; M7 migrates
   remaining consumers, not discovery, declarations, registry membership, or this test.
6. Create ignored `corpus/models/` **inside the repo** and populate it:
   - `Weeping Treant v1.2.stl` — i.e. `Awakened Tree.stl`, the same model (prerequisite
     above). Reference it via `$TREANT_STL` or copy it in; either way **do not commit it**,
     it is 21 MB;
   - `scripts/tests/fixtures/chitubox/Sphere.stl` and `Cylinder.stl` (copy, do not move —
     `rust/chitubox_io/tests/synth_compare.rs` consumes them where they are);
   - STL exports of the five synthetic fixtures, via the single sanctioned one-off:
     ```
     uv run --frozen python -c "
     import sys; sys.path.insert(0,'scripts')
     from support_placement import mesh_io as M
     for n in ('make_box','make_overhang_table','make_toed_slab','make_raking_blade','make_thin_fin'):
         getattr(M,n)().export('corpus/models/%s.stl' % n)
     "
     ```
     Run it once. Do not commit it as a file.
   The complete binding order is `Sphere`, `Cylinder`, `make_box`,
   `make_overhang_table`, `make_toed_slab`, `make_raking_blade`,
   `make_thin_fin`, `Weeping Treant v1.2`; there is no Cone. Do not commit any STL.
7. Freeze the baseline (see acceptance). Commit `corpus/manifest.json` and the
   baseline artifacts/reports, not the source STLs.
8. Spike `plinth-solid`.
9. **Adopt the fail-not-skip fixture rule as a workspace convention.**
   `scripts/tests/conftest.py:158` documents it — "missing/stale/version-mismatch => FAIL,
   never silently skip". Carry it into the Rust harness: a test whose required fixture,
   corpus model or baseline artifact is missing **fails with a named reason**; it does not
   `return` early, and it does not print a skip notice and pass. The one sanctioned skip in
   the whole tree is `chitubox_io`'s `treant_v5_candidate.chitubox` (see
   `05-chitubox-and-corpus.md` §4), which is uncommittable by size.

### `corpus/manifest.json` schema

Fix this during the later M0 baseline-freeze work item; M1 reads it and every later
milestone depends on it. If the manifest or baseline is absent during this documentation
checkpoint, that state is **pending M0 work** — do not fabricate or refreeze either artifact.

```json
{
  "schema": "plinth/corpus-manifest",
  "version": 1,
  "frozen_at": "<ISO8601>",
  "provisional": false,
  "baseline_tool": "scripts/tools/prep_one.py",
  "baseline_command": "uv run --frozen python scripts/tools/prep_one.py --p8-relocate --out-dir corpus/baseline --skip-existing corpus/models/*.stl",
  "baseline_git_rev": "<sha of the commit the baseline was frozen at>",
  "metrics": [
    {"id": "islands", "direction": "lower_is_better", "hard_bound": null},
    {"id": "boundary_edges", "direction": "lower_is_better", "hard_bound": 0}
  ],
  "models": [
    {
      "name": "Weeping Treant v1.2",
      "stl": "corpus/models/Weeping Treant v1.2.stl",
      "source_digest": {"algorithm": "sha256", "hex": "<64 lowercase hex digits>"},
      "report": "corpus/baseline/Weeping Treant v1.2.pipeline.json",
      "baseline_artifact": "corpus/baseline/Weeping Treant v1.2.chitubox",
      "python_exit_code": 0,
      "python_seconds": 402.1,
      "python_failed_stage": null
    }
  ],
  "waivers": []
}
```

`metrics` is the pre-committed metric table from §3 — **the owner signs it at M1 and it
cannot be retrofitted later.** `waivers` stays empty until the owner signs an entry.
`baseline_command` records the exact freeze invocation verbatim, because a baseline is
only meaningful with its configuration attached; `provisional` is true only for the
Treant-less interim corpus described above.

Every model row records its ignored input's `source_digest` and the committed pipeline
`report`. The digest is identity metadata, not a clean-checkout fixture requirement:
clean CI validates its typed SHA-256 shape but does not require a local source STL.
`baseline_artifact` is the emitted `.chitubox` path only when Python exit code is `0`;
it is explicitly `null` for a nonzero Python result, whose committed report remains the
baseline record.

### Acceptance — run these

```
uv run --frozen python scripts/tools/prep_one.py --p8-relocate \
    --out-dir corpus/baseline --skip-existing corpus/models/*.stl
jq '.models | length' corpus/manifest.json          # equals the model count
jq '[.models[].python_exit_code] | length' corpus/manifest.json   # every model has one
cargo build --workspace
cargo test -p plinth-core                            # includes the trybuild compile-fail case
cargo test -p plinth-core --test constants_golden    # renders and diffs constants.md
cargo test -p plinth-solid                           # spike union + difference, watertight
jq -e 'has("registry_entries") and has("aliases") and has("retired") and has("excluded")' \
    docs/rust-migration/m0-constants-source-ledger.json
```

**`--p8-relocate` is mandatory on the freeze.** `prep_one.py:73` makes Phase-8 relocation
an opt-in flag (`p8_relocate_enable=args.p8_relocate`, line 120) that **overrides**
`config.py:744`'s `p8_relocate_enable: bool = True`. Freeze without it and the measuring
stick never runs the relocation/repair stage — the ~4,850-LOC stage M7 spends its largest
deliverable redesigning as `plinth-support::repair` — so every clearance- and
ball-breach-derived metric would compare Rust's repaired output against a baseline that
skipped repair entirely. Record the flag in `manifest.baseline_command` so the freeze is
reproducible from the manifest alone.

`prep_one.py` writes one JSON line per input to stdout (keys `input_stl`, `out_path`,
`prepped_stl`, `status`, `exit_code`, `failed_stage`, `error`, `seconds`, `settings`,
`report_path`, `stages[]`) and a `<stem>.pipeline.json` report beside each output. Build
the manifest from those reports. **A non-zero exit code is data, not a failure** — record
it and move on.

**Parity gate:** none yet. The frozen baseline *is* the deliverable.
**Owner eyeball:** confirm `corpus/models/` is the right corpus — the right models, at the
right count, with the Treant present. Everything downstream is measured against this list.

---

## M1 — The parity harness and Rust refreeze (`plinth score`)  ∥ M2

**Goal.** The harness is written before the things it measures and is validated against
known-good output before it ever judges Rust. The first Rust CLI also owns the future baseline
refreeze path; M0's baseline remains frozen by Python only.

**Former M1 dependency (reference only):** M0; reuse its completed scoring work in revised step 1.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/tools/census_core.py` | Superseded → `plinth-metrics` disk-first adapter plus analytic/corrupt/native fixtures |
| `scripts/tools/census_run.py` | Superseded → `plinth-metrics` |
| `scripts/tools/p11_crossexam.py` | Superseded → `plinth-metrics` + M7's fixer-efficacy tests |
| `scripts/tools/validate_buried_end_fix.py` | Superseded → `plinth-metrics` |
| `scripts/tools/validate_r3_ball_gate.py` | Superseded → `plinth-metrics` |

### Rust deliverables

**`crates/plinth-metrics`** — the disk-first scoring adapter.

> **O-5 supersedes the former duplicate-auditor requirement.** `plinth-metrics`
> reopens artifacts from disk and uses `geom_kernel` and `plinth-mesh`; it does not
> carry second geometry, topology, `Mesh`, or delicacy implementations. Independence
> comes from disk provenance and analytic, corrupt, and native fixtures.

| Module | Contents |
| --- | --- |
| `read.rs` | Re-open a `.chitubox` from disk, extract models + support records + meshes. |
| `topology.rs` | Disk-first topology metric adapter over `plinth-mesh`. |
| `supports.rs` | Support count, per-record resin volume, arm cross-sections, effective X. |
| `scar.rs` | Scar area, worst patch, and delicacy scoring using the sole mesh/kernel capabilities. |
| `score.rs` | Disk-first metric orchestration plus the as-good-or-better comparator and waiver handling, all over `plinth-core`'s passive `MetricSet` and report DTOs. |

**`crates/plinth-cli`** — the `plinth` binary is reborn here.

- `[[bin]] name = "plinth"`.
- Subcommand `score` only at this milestone. `prepare` lands at M4, `calibrate` at M8.
  M3 therefore gates itself at the **library** level, not through the CLI — see M3's
  acceptance.
- `plinth score --baseline DIR --candidate DIR --manifest PATH [--allow-worse IDS] [--json OUT]`
- Exit 0 iff candidate is as-good-or-better on every non-waived metric on every model.
  Exit non-zero otherwise, printing **which metric on which model regressed and by how
  much** — never a bare exit code (commitment 4 applies to the harness too).
- Internal `refreeze` path: the Rust re-expression of `scripts/tests/_refreeze_v5.py`.
  After a physics change it writes the new artifacts, rewrites `manifest.baseline_command`,
  `frozen_at`, and `baseline_git_rev`, and refuses to run without an explicit reason string
  recorded in the manifest. Re-freeze before comparing, or the comparison measures the physics
  change rather than the port.

### Work items

1. Scaffold `plinth-metrics` as a disk-first adapter over `geom_kernel` and
   `plinth-mesh`. It must not duplicate their geometry, topology, `Mesh`, or delicacy
   algorithms. Add analytic and corruption fixtures that prove failures independently
   of producer reports.
2. Implement each metric in §3, in table order. Each gets a unit test on a hand-built
   fixture. **`max_x` is per-arm** (`A_voronoi_cell / (π·r_tip²)`) and that definition is
   frozen at this milestone; the arm-group form arrives at M5 as the separate id
   `max_x_group`. Implementing `max_x` as a group metric here is impossible anyway —
   `ArmGroup` and `LoadSharing` are M5 deliverables — and silently changing its meaning at
   M5 would invalidate every M4 comparison scored under the old definition.
3. Implement the comparator: per-model, per-metric, direction-aware, with the
   Python-failure rule (Rust success beats Python failure unconditionally).
4. **Build the harness self-validation fixtures.** In `crates/plinth-metrics/tests/fixtures/`,
   take a known-good artifact and produce three corrupted copies:
   - triangles deleted (must raise `boundary_edges`),
   - winding flipped on a shell (must raise `winding` and `negative_shells`),
   - an interior cavity injected (must raise `cavities`).
   A test asserts each corruption is detected. **A harness that cannot fail cannot gate.**
5. Write the metric table and directions into `corpus/manifest.json`.
6. Implement and test `plinth-cli`'s internal `refreeze` path. It is unavailable in M0.
7. Create the decision gate table in `06-conventions.md` now — not in M0 — and
   backfill every guide `D-*` decision, `O-*` override, and historical owner
   ratification with `EvidenceSource` and `EvidenceDate`. Enumerate, rather than
   compressing into “all”: the required `err: Option<T>` policy; passive
   `LoadSharing` core ownership versus `plinth-support` behavior; passive `FixerId`
   core ownership; cost DTO versus cost behavior and metric DTO versus independent
   metric behavior; Python-only M0 freeze followed by the Rust M1 refreeze path;
   every `D-*` and `O-*`; the EvidenceSource/EvidenceDate/FiniteInterval policy; and
   a distinct record superseding the initial generic-Unknown Saturn disposition with
   known official-vendor evidence. Then make the table dated and append-only.

### Acceptance — run these

```
cargo run -p plinth-cli --release -- score \
    --baseline corpus/baseline --candidate corpus/baseline --manifest corpus/manifest.json
# self-comparison: every metric exactly equal on every model; exit 0

cargo test -p plinth-metrics                         # per-metric units + corrupt-fixture suite
cargo test -p plinth-metrics --test corrupt_detection # each injected defect is flagged
```

**Parity gate:** the harness scores the frozen baseline without error, and every injected
defect is detected.
**Owner eyeball:** the owner reviews the metric table and its directions of goodness and
**signs it**. This gate cannot be retrofitted later — from M4 onward, this table is what
"better" means. Record the sign-off with the committed metric manifest.

---

## M2 — Mesh and geometry substrate  ∥ M1

**Goal.** The f64 substrate every stage stands on.

**Former M2 dependency (reference only):** M0; complete only revised step 2 needs.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/support_placement/mesh_io.py` | Reimplemented → `plinth-mesh::io` (+ the five synthetic fixtures as Rust builders) |
| `scripts/support_placement/topology.py` | Reimplemented → `plinth-mesh::topology`, **digest redefined, schema bumped** |

`perception.py`'s delicacy-field half and the scipy/skimage voxel half of `hollow.py` /
`internal.py` are *implemented* here as primitives; those files retire at M5 and M4/M5
respectively (see the audit table).

### Rust deliverables

**`crates/plinth-mesh`**

| Module | Contents |
| --- | --- |
| `io.rs` | STL read/write with an **explicit `WeldPolicy`**. This is load-bearing: `mesh_io.load_stl` uses `process=False` to avoid trimesh's implicit weld/reorder so vertex indices stay meaningful, while `hollow.triage_components` deliberately *does* weld (an unwelded STL triangle soup reads as thousands of one-face components). Two different policies for two different purposes — preserve the divergence explicitly, do not unify it by default. |
| `fixtures.rs` | Native `make_box`, `make_overhang_table`, `make_toed_slab`, `make_raking_blade`, `make_thin_fin`. |
| `triage.rs` | Shard-strip and pre-hollow refusal (from `hollow.triage_components`). Refuse, do not repair. |
| `topology.rs` | Face adjacency, cut flags, connected components, canonicalized fingerprint. |
| `delicacy.rs` | Curvature + thinness `FaceTable`. |
| `slice.rs` | Exact z-slicing to **closed 2D loops with winding**. There is no Rust equivalent in-repo for `trimesh.section_multiplane`; this is real algorithm work, not translation. |
| `proxy.rs` | `ProxyLadder` container (levels filled by M3). |

**`crates/plinth-voxel`** — voxelize, fill, signed EDT, 3D connected labels, morphology
(erosion/dilation), marching cubes. Replaces `scipy.ndimage` + `skimage.measure` +
`trimesh.voxelized`.

**`rust/geom_kernel`** — extend natively, then remove its compatibility surface.

- **O-7 correction to prior planning notes:** do not make `pyo3`/`numpy` optional and do
  not preserve a maturin-enabled `python` feature. Once native Rust callers use the rlib,
  delete the inline PyO3 surface, shim, `pyproject.toml`, `cdylib` crate type, Python-only
  dependencies, and `GEOM_KERNEL_THREADS` in that same checkpoint.
- Do not add `SurfaceOracle` on the elimination path. Introduce a backend trait only
  when a second real backend is implemented after Python deletion.
- Add **point-kNN** and **radius-pairs** queries. These are the `scipy.spatial.cKDTree`
  replacements that M5 and M6 need (`query_pairs` in `joints.compute_joint_balls`,
  ball-point queries in `engine._ensure_minima_contacts`).
- Do not leave the PyO3 surface working after the native handoff; disk artifacts carry
  the oracle role.

### Work items

1. `plinth-mesh::io` first, then round-trip byte-identity, then everything else.
2. **Redefine the topology digest.** `topology.py`'s `content_digest` currently hashes
   hand-counted NumPy/SciPy allocator scratch coefficients (`48f+88v`, `84f+32v`, …) —
   numbers that are meaningless outside CPython and that make the sidecar's identity depend
   on Python's allocator arithmetic. In Rust the digest is over the **topology arrays
   only** (indptr, indices, component ids, cut flags) plus the mesh fingerprint. Bump
   `FACE_TOPOLOGY_SCHEMA_VERSION` from 3 to 4. Do **not** attempt byte-parity with the
   Python digest.
3. **Canonicalize the fingerprint.** `topology.py` welds vertices by raw float64 **bit
   pattern**, so `-0.0` and `0.0` are distinct vertices and any NaN is its own vertex — on
   a mesh that has been through a float32 round-trip this silently fragments adjacency,
   raises `TOPOLOGY_CUT_BOUNDARY`, and then hard-fails coverage patches. In Rust:
   normalize `-0.0 → 0.0`, and treat any non-finite coordinate as a **`Defect`**, not as a
   vertex.
4. Build once, validate once. `topology.py` builds and then *immediately fully
   revalidates* (recomputes components, re-lexsorts every edge, re-bincounts flags), and
   five other call sites re-run that O(E log E) proof opportunistically. In Rust,
   construction establishes the invariant; validation is a debug-assert and an opt-in
   diagnostic.
5. `plinth-voxel` primitives with property tests against brute force.
6. Extend `geom_kernel` as above; keep its native suites green, then remove the complete
   Python compatibility surface in the same checkpoint. Do not rebuild it through maturin
   or retain `GEOM_KERNEL_THREADS`; use frozen disk/analytic/native fixtures for parity and
   `PLINTH_THREADS` for every live thread sweep.

### Acceptance — run these

```
cargo test -p plinth-mesh -p plinth-voxel
# property tests: sphere/torus slicing at known z closes to the analytic loop;
# signed EDT matches brute force on random grids; topology cut-flag fixtures

cargo test -p plinth-mesh --test stl_roundtrip
# Self-contained binary-STL regression: source-order triangle soup, repeated
# coordinates, signed zero, malformed input and lossy-f32 handling. It must
# pass from a clean checkout without ignored corpus/models inputs.

cargo run -p plinth-mesh --locked --example corpus_gate
# The explicit provisioned-corpus audit. It structurally checks the committed
# manifest's exact ordered eight-member binding and SHA-256s, then re-opens
# fresh round-trip files from disk for every member; f64 vertex bits and
# triangle-index order must match. It also wire-normalizes every native
# synthetic builder and compares its complete ordered arrays to the
# authoritative Python-generated STL. It intentionally fails by named reason
# when ignored corpus/models inputs have not been provisioned.

cargo test -p geom_kernel
# rust/geom_kernel/tests/geometry.rs stays green, including
# deterministic_build_bit_identical and r6_no_hashmap_in_src

# no maturin/PyO3 rebuild: O-7 deletes that compatibility surface at native handoff
# parity evidence comes from frozen disk artifacts and native/analytic fixtures
```

**Parity gate:** STL round-trip **array** identity over the corpus; `geom_kernel` suites
green; frozen artifact, analytic, and native-fixture parity evidence remains green after
the compatibility surface is removed. No stale-extension or Python-side parity gate remains.
**Owner eyeball:** a delicacy heat-map render of the Treant. Sanity: thin foliage hot,
trunk cold. If the trunk is hot, the curvature/thinness blend is wrong and M5's scar
weighting will be wrong with it.

---

## M3 — Protected production decimation; blocks M4

**O-8 disposition:** the protected, staged, error-bounded decimation contract below is
the mandatory first production stage and blocks every downstream production stage. Its
print-scale output is the sole production mesh consumed by orientation through emit.
Only extra L1/L2 orientation-proxy tiers and their dense-search ladder are post-Python
enhancements; those proxies must derive from the production-decimated mesh.

**Goal.** Convert `scripts/tools/decimate_pipeline.py`'s logic to native Rust —
**not reinvent it** (owner's explicit instruction). Blender is retired.

**Former M3 dependency (reference only):** M2. The production contract below is current;
only extra L1/L2 orientation-proxy tiers are post-Python.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/tools/decimate_pipeline.py` | Logic converted → `plinth-decimate`; Blender retired |
| `scripts/tools/geometry_sweep.py` | **Deleted with reason** — Blender-only degenerate-repair sweep; not ported. M2's triage + topology cut flags *refuse* what this repaired. The owner repairs pathological meshes upstream. |

`skills/decimate-for-print/SKILL.md` describes the *historical interactive Blender workflow*,
not the target. Only its algorithm is in scope; the Blender/MCP delivery mechanism is not.
Mark it superseded when `plinth-decimate` lands; do not wait for the final gate.

### Rust deliverables

**`crates/plinth-decimate`** — QEM edge collapse with the **exact staged contract** below.
These numbers are the contract, not suggestions.

| Stage | Contract |
| --- | --- |
| 1. Scale sanity | Longest bbox axis in **[10, 300] mm**, else SKIP with a reason. |
| 2. Probe | One aggressive decimation at **ratio 0.2** to build a per-vertex backward-error map. |
| 3. Feature seeds | Vertices with probe error **> 0.05 mm** are seeds. |
| 4. Protection halo | **0.4 mm** radius around each seed, capped at **12 %** of vertices, with **3 backoff iterations** (`seed_thr ×= 2`, `halo ×= 0.6`) if the cap is exceeded. **Protection is a hard constraint — a protected vertex never collapses.** Blender expresses it as the vertex group `decimate_freedom` with `vertex_group_factor = 10.0`, where **weight 1 = decimated MORE and weight 0 = protected** (counterintuitive, verified empirically, `decimate_pipeline.py:164-170`). In Rust, express it directly: protected vertices are excluded from the collapse candidate set, not merely down-weighted. |
| 4b. Budget precondition | **One probe at `RATIO_HI = 0.50` before any bisection.** If p99 or p99.9 already exceeds the budget at the loosest ratio, the model **FAILS** with a named `DecimationBudgetUnreachable` defect (Python: `status="FAIL_even_at_0.50"`, `decimate_pipeline.py:175-179`) — do not bisect, do not apply, do not export. This is a hard FAIL, distinct from stage 1's SKIP. |
| 5. Bisect | **5 iterations** in **[0.05, 0.50]** for the *lowest* ratio whose two-sided sampled **p99** error ≤ `pixel_um/2000` mm and **p99.9** ≤ `pixel_um/1000` mm. `pixel_um` defaults to **19.0** (Saturn 4 Ultra 16K, 14 × 19 µm — the coarser axis). |
| 6. Re-verify | **Full population**, both directions, with **1.1× slack** on both tolerances. The slack is a **final check only** — intentional, absorbing sample-vs-population drift. It is not a loosened spec and must not be applied to the bisection probes. |
| 7. Topology guard | Non-manifold and boundary edge counts **must not increase**. |

"Two-sided" means both directions: **forward** = decimated face centres → original surface
(sample cap 150 000 centres); **backward** = original vertices → decimated surface (sample
cap 120 000 vertices, seed 42).

Also deliver the **search-proxy path** for `ProxyLadder`: L1 ≈ 10 % of faces, L2 ≈ 2 %.
These are for M8's dense orientation sweep and are allowed to be far cruder than the
production decimation.

### Work items

1. Port the staged contract in order. Each stage gets a unit test that pins its constant.
2. **All nearest-point probing is batched.** `decimate_pipeline.probe()` calls
   `bvh.find_nearest` **one point at a time** in a Python for-loop, up to 7 times per model
   (one feature-map probe plus up to six bisection probes). That per-point loop is the
   defect, not the design. Gather all query points, issue **one** batched `geom_kernel`
   call, scatter the results. A per-item call in a hot path is a defect in every language.
3. Register the constants in `plinth-core::constants` with provenance. `pixel_um` is
   **Measured** (printer datasheet). `SEED_ERR`, `HALO`, `PROT_CAP`, `RATIO_LO`, `RATIO_HI`
   are **Placeholder** — the Blender script cites no measurement for them. Do not promote
   them to Measured because they look physical.
4. Deliver the stage as a **library entry point** (`plinth_decimate::run(&Mesh, &Budget)`)
   with no CLI dependency. M4 wires it behind `plinth prepare --stop-after decimate`; M3
   does not, because `prepare` does not exist until M4 and M3 must be gateable on its own.

### Acceptance — run these

```
cargo test -p plinth-decimate                        # budget-contract unit tests, one per stage row

cargo test -p plinth-decimate --test corpus_budgets
# reads corpus/models/ DIRECTLY (no CLI). Every model: decimation report passes the
# p99 / p99.9 budgets; non-manifold and boundary edge counts not increased; any model
# outside [10,300] mm is SKIPPED with a named reason, not silently passed; any model that
# blows the budget even at RATIO_HI = 0.50 FAILS with DecimationBudgetUnreachable and
# exports nothing (stage 4b) -- a SKIP and a FAIL are different outcomes and the test
# asserts which one each model gets.
```

The CLI wiring (`plinth prepare --stop-after decimate`) is verified at **M4**, which owns
the `prepare` subcommand. Do not make M3's gate depend on it.

**Parity gate:** self-verified error budgets on every corpus model, at the library level.
The historical Python tool was manual/out-of-band, but O-8 makes this Rust stage mandatory
production policy. The budget is the gate, and no downstream production stage may accept
the pre-decimation mesh.
**Owner eyeball:** before/after renders of the Treant at production ratio, plus the chosen
ratio per model. The owner is checking that foliage survived.

---

## M4 — The thin vertical slice (the most important milestone) after M3

**Goal under O-6.** A real `.chitubox` end-to-end from Rust using the minimally
ported current support/raft behavior. Do not build deliberately naive throwaway
supports; implement the capability once while proving the contract closes: pipeline,
`DefectReport`, boolean union, on-disk audit, format byte-contract, one CLI.

The former M4 plan expected naive supports to score worse than baseline. O-6 supersedes
that expectation: port current support behavior once and measure the resulting artifact.

**Former M4 dependency (reference only):** M1/M2; revised steps 3–6 replace it, with
protected decimation complete before any downstream production work.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/support_placement/pipeline.py` | Reimplemented → `plinth-pipeline` + `plinth-cli` |
| `scripts/support_placement/hollow.py` | Reimplemented → `plinth-mesh::triage`, `plinth-orient`, `plinth-hollow` |
| `scripts/support_placement/emit.py` | Reimplemented → `plinth-emit` |
| `scripts/support_placement/chitubox_emit.py` | Reimplemented in **f64** with native signed-volume winding → `plinth-emit` |
| `scripts/support_placement/prepared_validation.py` | **Deleted with reason** — replaced by ownership. Its unpicklable capability token exists only to avoid re-validating huge sidecars; in Rust that is a `Validated<T>` newtype constructible only by the validator. The whole module, including its `__reduce_ex__`/`__copy__`/`__deepcopy__` traps, disappears. |
| `scripts/support_placement/chitubox_inject.py` | **Not ported** (write side). Its own `write_replace` docstring condemns it: it "staples correct supports onto the WRONG MODEL, silently, with a file that parses and verifies." The read-side role already lives in `rust/chitubox_io`. |
| `scripts/tools/prep_one.py` | Superseded → `plinth prepare` (new report contract) |
| `scripts/tools/hollow_drill.py` | Superseded → the single CLI |
| `scripts/tools/emit_supports.py` | Superseded — **bypass path eliminated** |
| `scripts/tools/run_v5.py` | Superseded — direct-engine bypass path |
| `scripts/tools/run_engine_full.py` | Superseded — direct-engine bypass path |
| `scripts/tools/recheck_rewire_cap.py` | Superseded — direct-engine bypass path |
| `scripts/tools/recon_chitubox.py` | **Deleted with reason** — analysis-only, "NOT product code" by its own header; its findings are captured in `docs/specs/CHITUBOX-FORMAT.md` |

### The defect this milestone exists to kill

There are **five** entry points into the engine today. `pipeline.prep_one` is the real one;
`emit_supports.py`, `run_v5.py`, `run_engine_full.py` and `recheck_rewire_cap.py` all call
`engine.run` directly. `emit_supports.py` in particular runs `engine.run` then
`emit.emit_project` **on the source mesh** — no hollow, no drain drilling, and **no
orientation search**, because orientation lives *inside* `hollow.run` and `pipeline` only
invokes it when `hollow=True`. `pipeline.prep_one` documents this itself: passing
`hollow=False` records `reason='hollow=False (orientation search skipped too)'`.

Orientation is the coupling hub of the whole optimization triad. In Rust it is **its own
stage**, never a side effect of hollowing, and there is **exactly one** path from STL to
`.chitubox`.

### Rust deliverables

**`crates/plinth-cost`** — behaviour over `plinth-core`'s passive `CostVector`, `Fidelity`,
and Pareto DTOs: `CostProvider`, `Evaluator`, batching, scalarization, Pareto algorithms, and
per-search memoization. It implements the joint cost function over the triad (fast / minimum
resin / high quality); it does not define a competing report vocabulary.

**`crates/plinth-orient`** — a real stage.
- `--orient pinned:<baseline-dir>` reads the orientation the Python baseline chose, so M4
  through M7 compare like with like and orientation is not a confound. **This mechanism is
  load-bearing for three milestones' gates, so its contract is specified here exactly:**

  1. Read `<baseline-dir>/<stem>.pipeline.json`. Require exactly one `stages[]` entry named
     `hollow`, require its status to be `ok`, and take the frozen report field
     `report.orientation.{axis, deg}`. A competing `log` field is ambiguous and invalid.
  2. **Identity must be explicit.** A valid X/Y-axis orientation record with `deg: 0`
     selects identity. A missing stage, missing orientation, skipped/non-`ok` status, or malformed
     record is an invalid pinned baseline and must not silently select identity.
  3. Apply the rotation, then **reproduce the Python re-centering**, which the
     `{axis, deg}` pair does not capture: XY-center the bounding box and translate `zmin`
     to 0 (`hollow.py:472-474`). Skip this and every support lands offset.
  4. A model whose required baseline fixture or valid orientation record is absent cannot
     participate in pinned comparison. Fail with the named unreadable/invalid-baseline reason;
     do not guess.
- A coarse sweep driver over the `FaceTable`, including the legacy 13 candidates
  (identity + `±10/20/30°` about X and Y, exactly as `hollow.orient_search` enumerates them)
  so the old objective remains reproducible for comparison.
- The dense sweep is **M8**, not here.

**`crates/plinth-hollow`** — eligibility, cavity extraction, drains and vents,
priority-flood drainability. Port from `hollow.py` steps 2–8. The internal-cavity planner
(`internal.py`) lands at M5.

**`crates/plinth-solid`** — promote the M0 spike to the full backend, plus the **audit**.

**`crates/plinth-emit`** — all geometry synthesis from `chitubox_emit.py`, in **f64**:
frustum meshes, UV spheres, pads, bridges, the deterministic MST raft topology, native
root-boss sizing. Cast to f32 **only** at the record-encode boundary, because that is the
wire format — never upstream.

**`crates/plinth-pipeline`** — typestate sequencing, content-addressed cache, report.

**`crates/plinth-cli`** — add `prepare`.

### The four commitments this milestone must physically enforce

1. **Boolean-union at emit (commitment 7).** Naive triangle-soup concatenation of touching
   primitives created coincident coplanar faces and ChituBox cavity defects. That bug was
   fixed **four times at individual sites** before being fixed at source. Emitted solids
   are **unioned**, not concatenated.
2. **The disk re-read audit.** After writing, re-open the artifact **from the filesystem**
   and fail on boundary edges, negative shells, or inconsistent winding. Not the in-memory
   result — the bytes.
3. **The frame invariant.** `emit.py` checks `zmin_centered - plate_z == lift_mm` to
   `1e-6` mm against a **freshly reloaded and independently re-centered** STL, not against
   the caller's claim. Preserve that shape exactly. Related live hazard to *not* reproduce:
   `pipeline.prep_one` builds `SupportConfig(lift_mm=3.0)` while `config.SupportConfig`
   defaults to `5.0`, and `plate_z = bounds[0][2] - cfg.lift_mm` is computed from the
   *engine* cfg — so an emit-time `lift_mm` override silently disagrees with the plate the
   supports were built against. One typed value, one place.
4. **One entry point.** `plinth prepare` is the only route. There is no `--skip-hollow`
   that also skips orientation.

### Work items

1. `plinth-emit` geometry first — it is the piece with a real external contract. Validate
   against the **native** ChituBox fixtures (see acceptance).
2. `plinth-solid` full backend + audit.
3. `plinth-hollow` steps 2–8. Replace `hollow.py`'s `exit_ray` per-direction/per-step
   Python raymarch (5 directions × up to 90 steps for drains; 4 × 90 × up to 40 sampled
   points for vents) with one batched ray query.
4. `plinth-orient` with `pinned:` mode working end to end before the coarse sweep.
5. `plinth-pipeline` typestate, using **02 §3.2's mark names verbatim**:
   `Raw → Triaged → Decimated → Oriented → Hollowed → Supported → Emitted`, each
   transition consuming the previous state by value. The `Decimated` mark and its
   transition are filled by M3's `plinth-decimate`; identity pass-through is forbidden.
   (`Raw`, not `Loaded`: one name for one thing, and 02 §3.2 is the declaration site.)
6. `plinth prepare` with **naive supports**: hex-lattice contacts, straight X-rule pillars,
   no rewire, no bracing. Naive is the point.
7. Do **not** reproduce `hollow.py`'s mid-run artifact rewrite: on internal-verifier refusal
   `pipeline.prep_one` `shutil.copyfile`s a backup **over** the already-written prepped STL
   and re-runs the exterior engine, so the file on disk changes identity under a path other
   stages already consumed. In Rust, a refusal is a `DefectReport`, and no stage overwrites
   an artifact another stage has read.

### Acceptance — run these

```
cargo test -p plinth-emit
# The NATIVE ChituBox-authored fixtures get writer-BEHAVIOUR tests. Three are
# UI-authored:
#   scripts/tests/fixtures/chitubox/basic-cube-supports-raft.chitubox
#   scripts/tests/fixtures/chitubox/cylinder-supported-bottom-connected.chitubox
#   scripts/tests/fixtures/chitubox/two-supports.chitubox
# plus the original morel_mudskipper_supports.chitubox (243 records).
# ONLY THE FIRST TWO carry native PAD/BRIDGE (type-6) geometry, so they are the
# only valid oracle for pad and bridge synthesis.
#
# The five sphere-*/cylinder-* specimens AND pad_bridge_donor.chitubox were
# REGENERATED BY THIS LIBRARY at commit aa08704 after the ChituBox-authored
# originals were lost on 2026-07-24, and are therefore CIRCULAR for writer bugs:
# round-trip tests only, never ground truth. pad_bridge_donor in particular was
# built by re-hosting two records lifted from morel_mudskipper_supports.chitubox
# -- it is library output, not a ChituBox artifact.
# scripts/tests/fixtures/chitubox/README.md is authoritative on provenance.

cargo run -p plinth-cli --release -- prepare \
    --input-models-dir corpus/models --output saturn4-ultra \
    --out-dir corpus/candidate-m4 --orient pinned:corpus/baseline

cargo run -p plinth-cli --release -- score \
    --baseline corpus/baseline --candidate corpus/candidate-m4 \
    --manifest corpus/manifest.json \
    --allow-worse islands,support_count,resin,scar

cargo test --workspace                               # all format and audit gates green
```

**Parity gate:** format metrics are **absolute** — `boundary_edges = 0`,
`negative_shells = 0`, `winding = 0`, `cavities` not worse, `loadable = pass`. Support
metrics are **explicitly waived this milestone only** via `--allow-worse`, and the waiver
is spelled in the command, not hidden in a config. What is waived and why:

- `support_count`, `scar*` — historical M4 waiver rationale only; O-6 replaces
  the naive-support stage with one minimal port of current support behavior.
- **`resin`, the whole group** (`resin_model`, `resin_support`, `resin_total`) — waiving
  `resin_support` alone is unsatisfiable arithmetic: `resin_total = resin_model +
  resin_support`, so any support-resin regression big enough to need a waiver also makes
  the total worse and the gate exits non-zero anyway.
- **`islands`** — waived at M4 **only**. Island perception (`perceive.rs`) is an M5
  deliverable, and `scripts/support_placement/perception.py` retires at M5; a down-face hex
  lattice with no island detection will miss the small foliage islands the Python baseline
  supports on the Treant. From M5 `islands` is gated.

`max_x` is **reported, not gated** at M4 for the same reason: `ArmGroup` lands at M5, and
per §3 hard bounds bind the candidate only from the milestone the Notes column names.
**Owner eyeball:** the owner loads **two** emitted files in ChituBox Basic 1.3.0 — one
simple model and the Treant — and performs load → select → move → slice preview. This is
the first moment the migration produces something the owner can hold.

---

## M5 — Perception, placement, and the load model

**Goal.** Support demand and placement land, and **the load gate becomes satisfiable** —
the first milestone where a large flat down-face can pass, which Python cannot do at all.

**Former M5 dependency (reference only):** M4; redesign is post-Python.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/support_placement/perception.py` | Reimplemented → `plinth-mesh::delicacy` (M2) + `plinth-support` islands/minima/births (here) |
| `scripts/support_placement/placement.py` | Reimplemented → `plinth-support` pool / lattice / sizing |
| `scripts/support_placement/coverage.py` | Reimplemented → `plinth-support`, **one** generic dyadic sample-builder |
| `scripts/support_placement/specialized_contacts.py` | **Redesigned batched** → `plinth-support` |
| `scripts/support_placement/internal.py` | Reimplemented batched → `plinth-hollow` (the A5 internal-cavity planner) |

### The headline defect: the unsatisfiable load gate

This is the single most important thing M5 fixes, so understand it exactly.

- `verify.self_verify` (`verify.py:1666`) ANDs `len(load) == 0` into `ok`.
- `load` comes from `verify.load_report` (`verify.py:1071`), which flags any contact whose
  realized safety factor falls short after the tip radius was clamped at
  `cfg.tip_r_max_mm = 1.0` mm.
- **Nothing in `engine.py` ever adds a contact in response to a load violation.** The fix
  loop's only continuation condition is `if not report['uncovered_islands']: break`
  (`engine.py:1370`), and the only repair-candidate selector,
  `engine._rank_coverage_repair_rows`, ranks purely by marginal **uncovered coverage-sample
  area**. Load violations appear only as a non-increase constraint in
  `_coverage_repair_trial_verdict` — they can *reject* a repair but never *request* one.
- `engine._add_fix_contacts` (`engine.py:2009`), where the load→fix intent was written
  down, is **dead code in production**: its only callers are `scripts/tests/test_engine.py`
  and `scripts/tests/test_placement.py`. Even it selects by coverage distance, never by load.
- A tip capped at 1.0 mm covers at most **π × 1.0² × 12 ≈ 37.7 mm²** of supported area at
  X = 12. A large flat down-face therefore fails `ok` **forever**.

The missing concept is **multi-arm load sharing**. One arm cannot carry the face; N arms
can. M5 introduces `ArmGroup` and makes `effective_x` an input to lattice density, so the
gate becomes satisfiable by construction.

### Rust deliverables (in `plinth-support`, plus `plinth-hollow`)

| Module | Contents |
| --- | --- |
| `perceive.rs` | Overhang islands, local z-minima, island births (uses `plinth-mesh::slice` + `LayerPlan`). |
| `coverage.rs` | Coverage samples via **the ONE generic dyadic sample-builder**. `placement.candidate_pool` and `coverage._build_coverage_samples_inner` are two parallel ~300-line implementations of the same two-pass dyadic refine/preflight algorithm with different byte-per-row constants (64 vs 49). Factor to one generic builder parameterized by row type. |
| `pool.rs` | Deterministic legal candidate seat pool. **Trap, stated because it has been walked into:** once contacts sit on vertices it is tempting to give each contact the vertex's *averaged* normal. Do not. The contact normal must come from **the specific face that contact serves** — a vertex shared between faces of differing orientation has an average normal pointing where *neither* face faces, which aims the support wrongly. |
| `lattice.rs` | Hex/triangular lattice with **delicacy-weighted Lloyd (CVT) relaxation**. Hex beats a square grid by ~23 % fewer points for the same worst-case gap; each arm's Voronoi cell area **is** its supported area, which gives per-arm sizing directly. |
| `load.rs` | X-rule sizing with `ArmGroup` and `enum LoadSharing { Zipper, Additive }`. |
| `specialized.rs` | Specialized-contact consolidation, redesigned **batched**, with **one** acceptance predicate. |

**`plinth-hollow::internal`** — the A5 internal-cavity planner, batched. Its design
constraints are **owner-ratified and normative**; the Python implementation satisfies them and
the port must too:

- **Same physics as external supports**, but the **removability and anchoring constraints are
  dropped** — internal supports are never removed and never scar a visible surface, so they may
  be as robust as needed.
- **They must not pierce the model.** Entirely within the void.
- The radius cap, where one applies, applies **only to the short contact terminal**. The
  canonical contact sphere and its short taper feed a **separately represented structural
  carrier**; carriers, shared trunks and lower-wall roots stay **uncapped** and keep the full
  tension, deflection, transient-span, propagated-load and Murray sizing result.
- **Never clamp an overloaded contact down to the cap.** A bounded contact refinement may be
  tried; otherwise A5 **fails safely** and the oriented solid is retained.
- **Keep-out zones around drain holes** and their approach paths, so drainage is not blocked.
- **Refuse, loudly, on a trapped lobe** — never silently emit. One narrow owner override: after
  conservative coarse drainability alone fails, a single fixed pitch/3 physical audit may accept
  **support-induced** trapped pockets when each is ≤ 0.005 mL and their total is ≤ 0.01 mL.
  Anything larger keeps the oriented-solid fallback, and this override never permits an
  originally sealed or unvented cavity.

⚠ **The hard sub-problem is telling a cavity from a crevice**, and the obvious method does not
work: a point inside a sealed cavity and a point in a deep surface crevice both read as "not
solid", and straight-line ray occlusion measured **0.001 for a crevice versus 0.014 for a
cavity** — not a usable margin. The method that does work: **virtually re-plug the drill holes,
then flood-fill the empty space from a far corner of the bounding box; anything the fill cannot
reach is a genuine cavity.** It degenerates correctly on an undrilled model (the plug list is
simply empty). In Rust the drill stage and the planner are both inside `plinth-hollow`, so the
hole geometry passes in memory as a typed artifact — Python needed a sidecar only because STL
cannot carry it.

### The physics you must encode (see `03-physics-and-constants.md` for full provenance)

- **X = A_supported / A_arm_cross_section**, both mm². Measured failure boundary
  **X\* ≈ 25** (plate 5, 30 specimens: every X ≤ 25.3 survived, every X ≥ 25.0 failed, one
  boundary inversion), reproduced on **cylindrical** arms at **(19.91, 30.48)** (plate 6,
  n = 1 per bracket edge). Design target **X ≤ 12** (2× safety factor, 1.66× against the
  binding plate-6 edge).
- Load scales with supported **AREA** — not perimeter, not a squeeze-film law.
- Failure is **tensile snap of the narrowest cross-section just above the contact**. A long
  slow taper is a liability; supports must **flare fast** above the tip.
- **Total scar area is invariant at A_face/X** — the d² cancels between capacity and cell
  area — i.e. **8.3 % of face area at X = 12**, regardless of thin-dense vs thick-sparse.
  The only scar levers are **X** and **WHERE the scars land**. Do not build a knob that
  claims to reduce total scar area by changing spacing; it cannot.
- **X ≤ 12 is necessary, NOT sufficient.** It is blind to arrangement: three collinear arms
  have zero lateral stability perpendicular to their line, whatever their X. The lateral
  non-degeneracy gate ships regardless. `leading_release_edge = NegativeY` and
  `release_front_sweep_direction = PositiveY` (front moves `-Y → +Y`), which *predicted*
  that Y-arranged arms meet the front one at
  a time (zipper) while X-arranged arms meet it together (sharing) — **plate 6 measured no
  such axis dependence** (03 §4.2's separate orientation limitation), so arrangement enters through `ArmGroup`
  membership, **not** through a peel-axis term.

**`LoadSharing` is a `Measured` constant as of 2026-07-31 — both variants stay
code-complete.** Plate 6 (printed 2026-07-30, **scored 2026-07-31**) resolved two things:
(a) X\* under **constant-diameter cylindrical** arms brackets **(19.91, 30.48)**, which
*contains* plate 5's conical `X* ≈ 25`, so the cone was **not** the confound and
**`X_ALLOWED` stays 12.0 — no arm resizes**; (b) multi-arm load is **at least Additive** —
12/12 strip cells survived `X_arm = 60.15` at N = 2,3,4,6 along **both** axes while the
matched single arm died at the same X. **`Zipper` is refuted** and ships only as the
strictly-conservative fallback and the A/B arm. Ship
**`Additive { n_eff_cap: 6.0 }`** — `n_eff_cap` is the largest group measured — with
`N_eff` counting **all** arms rigidly co-carrying the region: plate 6 found **no axis
dependence**, so there is **no front-band filter and no orientation term**. The five
  core caveats (one cap/diameter/length; no strip failure, no upper bracket and no
  N>6 extrapolation; a ~19.75% fin-root section bonus and no super-additivity;
  one-cap degeneracy with the ratio dependent on plate 5; and the tight 1.33% N=2
  window) are in `03-physics-and-constants.md` §4.2 and must travel in the constant's
  note. The separate orientation limitation and B6 post-hoc plate-wide-gate override,
  including its counter-argument, cite `docs/reference/plate-06-results.md`. M8's A/B
measures the corpus-wide delta of the switch — **no redesign**.

### Work items

1. `perceive.rs`, then `coverage.rs` (one builder), then `pool.rs`.
2. `lattice.rs` with CVT/Lloyd relaxation weighted by a **scar-cost field on real curved
   surfaces** — not on a flat approximation.
3. `load.rs`: `ArmGroup`, `LoadSharing`, `effective_x`. Wire `effective_x` back into
   lattice density so the two co-solve.
4. `specialized.rs` batched. Today `engine._consolidate_specialized_contacts` is a nested
   per-contact Python loop that recomputes the **full** coverage report, load partition and
   specialized gates for **every candidate merge**, inside a `while True`. And it calls
   `_candidate_specialized_merge` **without** minima/island_births/baseline_specialized
   while `specialized_physical_invariant_report` calls the same helper **with** them —
   two different acceptance predicates for the same merge, so the invariant report can flag
   a pair the consolidator legitimately declined. **One predicate. One batched pass.**
5. Replace `placement.sample_candidates`'s `np.vstack` growth-per-accepted-point loop with
   a grid- or kd-tree-backed incremental Poisson-disk selection.
6. Preserve every tie-break rule exactly: lowest pool row, lowest face id, sorted row
   tuples. Parallelism must not reorder reductions.
7. Add the **`max_x_group`** metric to `plinth-metrics` (§3) — arm-group effective X under
   the resolved `LoadSharing`. It is a **new id**, not a redefinition: `max_x` keeps the
   per-arm meaning the owner signed at M1. `plinth-metrics` computes it from the
   reopened artifact using the sole `geom_kernel`/`plinth-mesh` capabilities.

### Acceptance — run these

```
cargo test -p plinth-support --test load_gate_satisfiable
# flat-down-face fixture: an N-arm ArmGroup passes X<=12 where a single arm cannot.
# This test FAILS on the Python semantics by construction; that is the point.

cargo test -p plinth-support --test load_sharing_both
# Zipper and Additive both green; the corpus A/B lands at M8.

cargo run -p plinth-cli --release -- prepare \
    --input-models-dir corpus/models --output saturn4-ultra \
    --out-dir corpus/candidate-m5 --orient pinned:corpus/baseline

cargo run -p plinth-cli --release -- score \
    --baseline corpus/baseline --candidate corpus/candidate-m5 \
    --manifest corpus/manifest.json --allow-worse resin_support,resin_total
```

**Parity gate:** X-rule compliance (`max_x`, per-arm) and coverage metrics (`islands`)
at-or-better — `islands` becomes gated **here**, having been waived at M4 only; `scar*`
at-or-better; `support_count` **reported but not yet gated** (it should be approaching
baseline); `resin_support` waived one last time (no rewire yet — that is M6), and
`resin_total` waived with it, for the same reason and the same duration, because the total
carries the support term by construction. The new `max_x_group` metric is **reported, not
gated** at M5; it becomes gated at M7 with everything else. Format metrics remain absolute.
**Owner eyeball:** scar heat map + lattice overlay per corpus model. The owner is checking
that scars are landing on the bland surfaces and avoiding the delicate ones — the
`scar_weighted` metric is the number, but this is the picture.

---

## M6 — Forest, growth, sizing

**Goal.** The structural solver.

**Former M6 dependency (reference only):** M5; redesign is post-Python.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/support_placement/tree.py` | Reimplemented → `plinth-support::forest` (arena) |
| `scripts/support_placement/cco.py` | Reimplemented → `plinth-support::cco` |
| `scripts/support_placement/rewire.py` | Reimplemented, **deterministic work budget only** |
| `scripts/support_placement/bracing.py` | Reimplemented → `plinth-support::brace` |
| `scripts/support_placement/transient.py` | Reimplemented (sizing, **pinned semantics**) |
| `scripts/support_placement/joints.py` | **Ported as coded** → `plinth-support::joints` |
| `scripts/support_placement/routing.py` | Exact-oracle role → `geom_kernel`; the **sampled legacy path is not ported** |
| `scripts/support_placement/struts.py` | **One function** ported (`find_model_anchor_candidates`); the rest deleted — v3 dead |
| `scripts/support_placement/trunks.py` | **Deleted with reason** — dead v3 architecture, replaced by Forest/CCO; not imported by production Python today |
| `scripts/support_placement/braces.py` | **Deleted with reason** — dead v3 (and its `networkx` dependency goes with it) |

`trunks.py` and `braces.py` are not called by `engine.py` at all; their only importers are
each other and their own tests. `tree.py`'s own module docstring says the Member/Forest
model **replaces** "v3's Trunk/Route/riser/brace collections." Do not port them as a second
competing structure-growth path.

### Rust deliverables (all in `plinth-support`)

| Module | Contents |
| --- | --- |
| `forest.rs` | `Forest` arena with **generational indices**. Checkpoint/rollback as **data**, not as `__dict__` surgery. **End-radius newtypes** — see below. |
| `cco.rs` | CCO insertion: Xia (α = ½) closed-form bifurcation angles, Weiszfeld weighted-Fermat junction polish, Murray radius propagation. |
| `rewire.rs` | Subtree reattach, junction slide, twig prune, degenerate retire — under a **deterministic work budget only**. |
| `brace.rs` | Cross-link and model-rung bracing with **activation triggers**. |
| `transient.rs` | Build-time transient sizing with **pinned semantics** (see below). |
| `joints.rs` | Joint balls with the **enclosure certificate ported as coded**. |
| `anchor.rs` | Model-anchor candidate search, from `struts.find_model_anchor_candidates` **only**. |

### Five things that must be right

1. **End-radius newtypes.** `Forest`'s convention that a member's `p1` is the root-ward end
   and `p0` the terminal-ward end is load-bearing across the whole cluster —
   `verify.murray_report` checks `parent.r0**4 >= sum(child.r1**4)` on exactly that
   convention. A previously-fixed real bug (`cco._propagate_murray`'s "B2 fix") combined the
   wrong end (`max(r0,r1)` instead of `r0`) and silently produced **2.3×-oversized
   members**. Make the convention structurally enforced with distinct newtypes for exit
   radius and entry radius; a comment already failed to prevent this once.
2. **Murray must filter dead members.** `verify.murray_report` (`verify.py:1058–1063`)
   iterates `enumerate(forest.children)` and sums over every child index with **no
   `c not in forest.dead` test**, while `transient.resize_forest` and `_postorder` **do**
   filter. So a retired child's `r1` still inflates `sum_child⁴` and can fail a hard gate on
   geometry that will never print. Fix it; add the test.
3. **Deterministic work budget only.** `rewire.optimize` currently has a wall-clock cap
   (`cfg.rewire_time_cap_s = 30.0`). Both the earlier port plan and its Codex review
   independently flagged this as the most serious determinism defect in the codebase: a
   faster implementation completes more moves before the cap fires and **silently changes
   topology**, which means the "frozen serial baseline" is not reproducible. Rust ships a
   move-evaluation counter. There is no wall-clock mode, not even as a legacy option.
4. **Sizing returns values; it does not mutate what verify measures.**
   `transient.resize_forest` mutates member radii in place and returns a diff list while
   explicitly *not* revalidating certificates ("the CALLER revalidates"). In Rust, sizing
   returns a new radius array. Preserve the pinned semantics: sort by **`active_z`, not
   `point_z`**; **clamp** out-of-range restraints into `[base_z, z_top]` rather than
   dropping them; keep the closed-interval overlap gate alongside the `a - max(z_min, R)`
   arm formula; convert the worst free gap from z-span to 3D length by
   `own_length / z_extent` (a leaning member is undersized otherwise — caught on the toed
   slab at 56–60° lean); keep the shoot-count exponent `max(n_terminals, 1) ** 0.125`; and
   tie-break to the lexicographically smallest root-first member-index tuple.
5. **Port `joints.py` as coded, not from its prose.** The enclosure certificate (exact
   signed distance to a tapered frustum via a 2-D (axial, radial) trapezoid reduction, then
   rigorous recursive spherical-patch subdivision — never sampling) went through multiple
   rounds of adversarial review with documented wrong-then-fixed intermediate states. The
   code carries post-review corrections the docstring does not fully capture, notably the
   star-shapedness credited-member restriction and the direct-witness short-circuit in
   `_certify_sphere_enclosed`. Do not re-derive it from the math summary. Its doctrine —
   *uncertified anywhere ⇒ KEEP the ball* — is conservative on purpose.
   `joints._batch_dist_and_local_radius` also documents that its numpy expression must use
   component-wise `x*x + y*y + z*z` rather than `@`/`einsum` to stay bit-identical to a
   scalar reference (einsum differed in 3275 / 240000 cases). Pin the arithmetic order in
   Rust; do not hand it to a reduction whose associativity is unspecified.

### Work items

1. `forest.rs` arena first — generational indices let interior removal be safe, which the
   Python append-only member list could not do (hence `mark_dead`, quarantine, and
   dead-exclusion filters scattered through three modules).
2. `cco.rs` insertion with the candidate-family builders. Follow the **serial-generate /
   parallel-evaluate / serial-commit** discipline: generate candidate descriptors serially,
   evaluate them against **one immutable snapshot**, store results by original index, apply
   the existing tie-break, commit exactly one winner serially. This discipline is the one
   piece of `docs/reference/port_plan.md` that survives intact.
3. `rewire.rs` with the work budget.
4. `transient.rs`, then `brace.rs` (bracing consumes the transient span report), then the
   `brace_and_size` fixed point. Emit a **named termination** — never fall out of a `for`
   loop with no record of non-convergence, which is what `engine.brace_and_size` does today,
   and never `assert not violations` (`engine.py:739`) as an acceptance mechanism.
5. `joints.rs` ported as coded, batched **across balls** as well as within one.
6. Drop the CPython-specific caches. `cco`/`tree`'s per-content-key dicts keyed on numpy
   `.tobytes()` exist to avoid redundant FFI calls from a Python hot loop. With the
   algorithms themselves in Rust there is no FFI. Keep the **batched-call discipline**
   (gather misses → one batch call → scatter); drop the `_cap_evict` / `_VERDICT_CACHE_CAP`
   machinery.

### Acceptance — run these

```
cargo test -p plinth-support
# includes:
#   murray_dead_member_filter   -- fixes the live Python bug above
#   enclosure_certificate_*     -- joint-ball fixtures ported as coded
#   determinism_under_threads   -- identical forest at 1/4/16 threads
#   rewire_work_budget          -- no wall-clock path exists

cargo run -p plinth-cli --release -- prepare \
    --input-models-dir corpus/models --output saturn4-ultra \
    --out-dir corpus/candidate-m6 --orient pinned:corpus/baseline

cargo run -p plinth-cli --release -- score \
    --baseline corpus/baseline --candidate corpus/candidate-m6 \
    --manifest corpus/manifest.json
```

**Parity gate:** no waivers. `resin_model`, `resin_support`, `resin_total` and the
clearance-related metrics must be **at-or-better**, on top of everything M5 gated. If
`resin_support` is still worse here, rewire or bracing is under-performing — do not waive it,
fix it.
**Owner eyeball:** side-by-side support-forest renders, baseline vs Rust, for the Treant and
the toed slab. The owner is looking for structure that reads as *deliberate* — trunks that
consolidate, braces where spans are long — not a thicket.

---

## M7 — Verify + repair complete (post-Python reference/backlog)

**O-6 status:** the full fixer/repair redesign is post-Python backlog. Before deletion,
port current verifier safety behavior required by executable essential parity; the full
redesign is not a deletion precondition.

**Former dependency:** M6 → M7 → M8. This does not unblock or block revised deletion.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/support_placement/verify.py` | Reimplemented → `plinth-verify` |
| `scripts/support_placement/removal.py` | Reimplemented as an **opt-in diagnostic** → `plinth-verify` |
| `scripts/support_placement/phase8.py` | **Redesigned from invariants**; proofs ported as coded → `plinth-support::repair` |
| `scripts/support_placement/phase8_driver.py` | **Redesigned from invariants** → `plinth-support::repair` |
| `scripts/support_placement/p8_accept.py` | Gates wired as `ViolationKind`s and tests into the shipped accept path |
| `scripts/support_placement/engine.py` | Reimplemented — the fix loop is the last engine role |
| `scripts/support_placement/config.py` | Conversion to the typed provenance registry **completes** here |

### The defect class this milestone exists to make impossible

`verify.self_verify` ANDs **ten** gate lists into `ok` unconditionally (verify.py:1666-1671),
an eleventh (`ball_breach_violations`) under `gate_on_ball_breach` — which **defaults True**
(verify.py:1691) — and `removal_violations` under `gate_on_removal`, which defaults False
and which no shipped caller ever sets. A **twelfth** gate lives outside `verify.py`
entirely: `physical_contact_invariant_violations` is produced and applied by
`engine._apply_physical_contact_gate` (engine.py:1017-1024), which sets
`report["ok"] = False` *after* `self_verify` has returned. So the effective acceptance set
is twelve gates — and **eleven of the twelve have no fixer that could ever satisfy them.**
Only `uncovered_islands` has one. The rest —
`clearance_violations` (indirect only, and `brace_and_size` ends with a bare
`assert not violations` that crashes the run instead of reporting a defect),
`load_violations` (none), `unsupported_minima` (demand created only up front),
`uncovered_island_births` (same), `regular_contact_invariant_violations` (none),
`unrouted_specialized_contacts` (just sets `ok=False`), `connectivity_violations` (none),
`printability_violations` (none), `murray_violations` (none),
`ball_breach_violations` (Phase-8 SM-BALL only), `physical_contact_invariant_violations`
(none) — are used **only** as non-regression tripwires during coverage repair.

In Rust this is a **compile error**: `ViolationKind` is exhaustively matched by
`response_for`, so adding a gate without adding a `Repair` or proving a `Terminal` will not
build. That is commitment 3, and M7 is where it pays.

### Rust deliverables

**`crates/plinth-verify`** — complete.

- Every gate is a **pure function** `(&Plan, &Geometry) -> Vec<Defect>`. No mutation, no
  side effects, no logging into the thing it measures.
- **SCAR-AREA-CAP + PRINT-FLOOR certificate**, bit-faithful to `docs/specs/SPEC-EMBED-POLICY.md`:
  `A_shaft = min(π·r_x²/cos, 2·r_x·T)`, `A_ball = π·max(R_b² − d², 0)`,
  `A_patch ≤ max(A_ABS_MM2, KAPPA·π·R_b²)`, `L_scar ≤ L_SCAR_MAX_MM`,
  `A_anchor ≥ π·r_end²`. Constants: `A_ABS_MM2 = 2.0` mm², `KAPPA = 1.25`,
  `L_SCAR_MAX_MM = 2.5` mm, `PRINT_FLOOR_CHORD_MM = 0.08` mm (owner-ratified 2026-07-24),
  `MIN_ILLEGAL_LEN_MM = 0.05` mm, `TOL_MM = 0.05` mm.
- **THE ONE interval / tip-anchor algorithm.** This is the most important correctness rule
  in the gate. `verify._batch_axis_intervals` requests penetration intervals at **full
  resolution** (`min_illegal_len_mm = 0.0`) and applies the length filter **AFTER** tip
  anchoring — its docstring says the `0.0` *is the law*. `ExactSurface.penetration_intervals`
  defaults to `0.05` and disagrees. Inverting the order **promotes an unanchored transit
  into the tip slot**, where a legal SCAR verdict exempts it; that single ordering bug hid
  5 non-exempt members and misfiled 3. **In Rust the kernel offers no pre-anchoring filter
  at all** — the parameter does not exist, so the ordering cannot be inverted.
- Ray-set law: **stubs get axis-only** intervals; **non-stubs get axis + 8 capsule-offset
  rays**; tip anchoring is evaluated **per ray against that ray's own arc length**.
- `removal_report` becomes an **opt-in diagnostic**. Today it runs on every verify pass
  (an O(trees²) `_min_gap_to_members` sweep plus a full surface-distance sweep per tree)
  while `gate_on_removal` defaults `False` and no shipped caller ever sets it. Dead compute
  on every pass. Either it gates or it is opt-in — in Rust, opt-in.
- One clearance gate. Today the exact path returns dicts carrying `source` and fills three
  out-params; the sampled legacy path returns dicts with **no `source` key**, leaves the
  out-params untouched, and has no rule-4 transit concept at all. Two behaviourally
  different gates behind one function name. Ship the exact one.
- Delete `certify_members_full_resolution` as a separate path — its own docstring says it
  is now the same computation as the ordinary certificate since the filter ordering was
  fixed, and the census-boundary class is "EMPTY by construction." **One certificate.**
- Do not port `_embed_inset_arc_exact`'s arc formula (retired as a sanction, magic `cos`
  floor of 0.5, pitch term hardcoded to 0, "slated for replacement" by its own docstring).
  The member's designed embed zone is known exactly; derive the mask from that.
- Pass the SCAR-standalone census through to the acceptance report. `self_verify` never
  passes `scar_illegal_out` to `clearance_report`, so the class that catches a collapsed
  embed anchor with **no** through-transit interval is invisible in the report today.

**`plinth-support::repair`** — the dispatcher.

- Exhaustive `response_for` table over `ViolationKind`.
- A **fixer-efficacy fixture test per `Repair` kind**: on a fixture exhibiting that defect,
  the fixer must strictly **reduce its measure**. A fixer that cannot demonstrate efficacy
  is not a fixer.
- A **`Terminal`-proof fixture per `Terminal` kind**: a fixture where the defect genuinely
  cannot be repaired, proving the terminal classification is honest.
- **Cap exhaustion names WHICH defect failed to improve** (commitment 4). Never
  `termination='attempt-cap'` with no subject, never
  `RuntimeError('… fixed point exhausted N round(s)')`, never falling out of a loop silently.

**The relocation ladder — redesigned from stated invariants, not transliterated.**
`phase8.py` + `phase8_driver.py` are ~4 800 lines of accreted amendment-over-amendment
patches (D6 zero-new-violation invariant, outer round loop re-arming per-call memory,
progress-based retirement, dual-certificate boundary carry, ban sets), each individually
justified by a measured regression but cumulatively a sequence of patches over patches.
Extract the **invariants** and implement a fresh, unified ladder:

| Invariant | Rust expression |
| --- | --- |
| GLOBAL lexicographic-min over the **full legal set** (never first-legal-wins) | One `select_min` over an evaluated candidate vector |
| D6: strict decrease, zero new violations | The commit predicate, checked once |
| Deterministic total-order tie-breaks everywhere | Explicit sort keys; parallelism never reorders |
| Batch-first legality (gather → one kernel call → scatter) | Already the Python spirit; keep the shape |
| Shaft-before-joint ordering | Joint-ball legality depends on **final** shaft geometry — this ordering is load-bearing, not stylistic |
| Unified `MemberKind` | Stub and strut currently duplicate the whole module (`reaim_stub`/`reaim_foot`, `stub_candidates`/`strut_candidates`, `_resin_guards_ok`/`_strut_guards_ok`) differing mainly in ray-set and ball factor |
| `SiteId` as a sum type | Replaces `Q_SITE_OFFSET = 1_000_000`, an integer-space partitioning hack that multiplexes two site-id namespaces through a magic offset |

Invoke the ladder **once**, post-fixed-point — it is a distinct final stage, not a per-kind
fixer inside the generic loop.

**Not ported at M7:**
- The frozen-treant snapshot special-casing (`phase8_relocation_input.json`,
  `TREANT_PRERELOCATE_DIGEST`, magic member indices 72/37/39/7) — this is acceptance-fixture
  logic embedded in production driver code. Keep it, if at all, as a **test-fixture
  comparison outside the pipeline**.
- The **concession ladder** (`SPEC-P8-REACH` Feature B, standoff `(0.3, 0.1)` mm) — it
  locally weakens a member's clearance envelope on the expectation that a human buffs the
  print afterwards. That is a workaround for insufficient support quality baked into the
  acceptance criteria. **Decision point:** if any corpus model fails without it, stop and
  raise it with the owner rather than reintroducing it silently.
- Pickle as a serialization contract. `p8_accept.final_geometry_verify` round-trips the
  `Forest` through `pickle` in a tempfile to catch serialize drift. Keep the **intent** —
  an explicit versioned serialization with a round-trip drift check — drop the mechanism.

### Work items

1. `plinth-verify` gates, one at a time, each with fixtures, in this order: format →
   coverage → clearance/transit → load → connectivity → printability → murray → ball breach.
2. The certificate primitives (intervals, tip anchoring, SCAR verdicts) **before** the
   repair ladder — the ladder consumes them.
3. `response_for` exhaustive table; make the compiler enforce it.
4. Fixer-efficacy suite, then `Terminal`-proof suite.
5. The relocation ladder from the invariant table.
6. **Incremental re-verify ≡ full re-verify** as a CI check. This is what makes the M8 dense
   sweep affordable and it must be proven, not assumed.
7. Finish the `config.py` → typed registry conversion. Every remaining constant gets
   `Measured` / `Derived` / `Placeholder` / `Pending`. The trybuild proof from M0 now
   guards a real accept path. Kill the `getattr(cfg, name, default)` pattern — `engine.run`
   reads ~8 config fields that way with defaults that **disagree** with `config.py` (e.g.
   `getattr(cfg, 'p8_relocate_enable', False)` vs a `True` default), so behaviour differs by
   call path. Typed config; no defaults at the read site.

### Acceptance — run these

```
cargo test -p plinth-verify -p plinth-support
# fixer-efficacy suite: every Repair kind strictly reduces its measure on a fixture
# terminal-proof suite: every Terminal kind proven unrepairable on a fixture
# incremental_reverify_equals_full

cargo run -p plinth-cli --release -- prepare \
    --input-models-dir corpus/models --output saturn4-ultra \
    --out-dir corpus/candidate-m7

cargo run -p plinth-cli --release -- score \
    --baseline corpus/baseline --candidate corpus/candidate-m7 \
    --manifest corpus/manifest.json
# exit 0 == as-good-or-better on EVERY metric on EVERY model.
# Models where Python failed: a Rust success suffices.
```

Note the `prepare` command **drops `--orient pinned:`**. From M7 the Rust orientation stage
runs on its own (coarse sweep). If that costs you the gate, pin it, pass the gate, and
record the pin as a known deviation to close in M8 — do **not** waive a metric to hide it.

**Historical M7 gate:** every then-defined metric and every model, with only signed
waivers. The current deletion gate is the executable essential-parity contract above.
**Owner eyeball — the owner pass.** Hand over: Pareto sheets; scar heat maps; per-model
before/after for (support count, resin, z-height); and every emitted file loaded in
ChituBox. Under O-6, the binding human precondition is loading the Rust artifact in
ChituBox and printing one model at acceptance/delete; full redesign artifacts are post-Python.

---

## M8 — Search quality + calibration + determinism CI

**O-6 status:** dense search, calibration, and related quality enhancement are
post-Python backlog. Determinism of the minimally ported pipeline remains an essential
pre-deletion gate.

**Former dependency:** M7 plus the M3 proxy ladder. This section does not unblock or
block revised deletion.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/support_placement/corpus.py` | Reimplemented → `plinth calibrate` (plate-analysis role) |
| `scripts/tools/bench_kernel.py` | Superseded → criterion benches |
| `scripts/tools/replay_parity.py` | Superseded → determinism CI |
| `scripts/tools/instrument.py` | Superseded → determinism CI |
| `scripts/tools/common.py` | Superseded → cargo/CLI conventions |
| `scripts/tools/_toolpaths.py` | Superseded → cargo/CLI conventions |

### The defect this milestone fixes

`hollow.orient_search` evaluates exactly **13** candidate orientations — identity plus
`±10/20/30°` about X and Y only (**no Z rotation, no combined-axis candidates**) — scored by
a hand-tuned weighted sum: `0.5·support_area_cm² + 3·peak_crosssection_cm² + 0.3·height_mm`,
plus `orient_scar_weight·scar/100` when `cfg.orient_scar_weight` (default 0.4) is set. That
is a local heuristic, not a search. There is no feedback at all from the actual support
stage — no support count, no realized scar placement.

One further hazard to retire with it: the scar term is scored on the **90 %-decimated
proxy** while the delicacy field is built on the **full pre-decimation mesh**, so the term's
magnitude is decimation-dependent and not comparable across models. (The Rust fix is
structural — 02 §3.3's `FaceTable` makes the delicacy field a property of the LOD being
scored, so the field and the evaluation mesh are one object.)

Orientation is the coupling hub of the triad: it sets z-height (**fast**), it enables good
hollowing and fewer supports (**minimum resin**), and it aids dynamic layer thickness and
lets support placement avoid scarring delicate regions (**high quality**). It deserves a
real search, and M4–M7 made per-candidate re-evaluation cheap enough to afford one.

### Rust deliverables

**Dense sweep becomes the default orientation mode:**

| Tier | Candidates | Fidelity | Output |
| --- | --- | --- | --- |
| Coarse | ~512 quasi-uniform + convex-hull-face-aligned + the legacy 13 | L2 proxy (~2 %) | ranked |
| Refine | top 24 | L1 proxy (~10 %) | ranked |
| Exact | top 3 | protected decimated production mesh | Pareto front recorded |

The legacy 13 stay in the candidate set permanently so the old objective's choice is always
visible in the Pareto record.

**`plinth calibrate`** (new `plinth-cli` subcommand):
- Surrogate-vs-exact **Spearman correlation**, **regret**, and an **exact-best-in-top-K**
  gate.
- **`LoadSharing` Zipper/Additive corpus A/B delta report.** Plate 6 scored 2026-07-31 and
  selected `Additive { n_eff_cap: 6.0 }` (`03-physics-and-constants.md` §4.2); the A/B stays
  because the size of that switch must be **measured**, not assumed, and because `Zipper`
  remains the strictly-conservative fallback.
- The plate-analysis role of `corpus.py` (nearest-neighbour spacing distribution, tip-radius
  histogram, z-quartile share) reading `.chitubox` through `chitubox_io`, not through the
  Python `chitubox_inject.Project`.

**CI jobs** (`06-conventions.md` has the runner config):
- Corpus at **1, 4, 16 threads** → identical artifact digests, driven by the sole
  workspace-wide `PLINTH_THREADS` (06 §3), including `geom_kernel`. O-7 removes
  `GEOM_KERNEL_THREADS`; it is not swept or retained as an exclusion.
- **Cache-disabled ≡ cache-enabled** reports, using the one sanctioned spelling
  `plinth prepare --no-cache` (06 §6). There is no `PLINTH_CACHE` environment variable.
- **The exact-bytes determinism fingerprint**, the Rust re-expression of
  `scripts/tests/_perf_fingerprint.py`: canonical exact-byte serialization (f64 as its raw
  big-endian bit pattern, arrays as dtype + shape + raw bytes), never a tolerance
  comparison. It is what the thread-sweep and cache-sweep digests are computed with.
- **criterion benches** replacing `bench_kernel.py`.

### Work items

1. Proxy-ladder integration: L2 → L1 → Exact, with the cost model deciding tier promotion.
2. Pareto front recording per model; the chosen point plus its neighbours are the eyeball
   artifact.
3. `plinth calibrate` with the three surrogate-quality statistics.
4. The `LoadSharing` A/B.
5. Determinism CI. Parallel reductions must be order-pinned; this is where you find out if
   any rayon fold slipped through.
6. Criterion benches for the kernel hot paths.

### Acceptance — run these

```
cargo run -p plinth-cli --release -- calibrate --manifest corpus/manifest.json
# GATE: the exact-best orientation must fall within the top-24 refine set.
#   If it does not, K doubles and the run repeats.
#   If it falls outside top-48, fall back per the ladder in 02-target-architecture.md §5
#   and report it -- do not widen K indefinitely.

cargo run -p plinth-cli --release -- prepare \
    --input-models-dir corpus/models --output saturn4-ultra \
    --out-dir corpus/candidate-m8                      # dense sweep is now the default

cargo run -p plinth-cli --release -- score \
    --baseline corpus/baseline --candidate corpus/candidate-m8 \
    --manifest corpus/manifest.json

PLINTH_THREADS=1  cargo run -p plinth-cli --release -- prepare --input-models-dir corpus/models --output saturn4-ultra --out-dir corpus/det-t1
PLINTH_THREADS=4  cargo run -p plinth-cli --release -- prepare --input-models-dir corpus/models --output saturn4-ultra --out-dir corpus/det-t4
PLINTH_THREADS=16 cargo run -p plinth-cli --release -- prepare --input-models-dir corpus/models --output saturn4-ultra --out-dir corpus/det-t16
# artifact digests identical across all three, computed with the exact-bytes fingerprint.
# PLINTH_THREADS is the ONE workspace-wide thread variable (06-conventions.md section 3):
# it sets every plinth-* rayon pool, not just geom_kernel's.

PLINTH_THREADS=1  cargo run -p plinth-cli --release -- prepare --no-cache --input-models-dir corpus/models --output saturn4-ultra --out-dir corpus/det-t1-nocache
PLINTH_THREADS=4  cargo run -p plinth-cli --release -- prepare --no-cache --input-models-dir corpus/models --output saturn4-ultra --out-dir corpus/det-t4-nocache
PLINTH_THREADS=16 cargo run -p plinth-cli --release -- prepare --no-cache --input-models-dir corpus/models --output saturn4-ultra --out-dir corpus/det-t16-nocache
# cache-off must reproduce cache-on byte for byte (06-conventions.md section 6)

cargo bench --workspace                                # criterion, replaces bench_kernel.py
```

**Parity gate:** M7's full gate **maintained** under the dense sweep. Orientation-coupled
metrics (`z_height`, `support_count`, `resin_*`, `scar*`) are expected to **improve** — that
is the point of the milestone — but improvement is not the gate; not-worse still is.
**Owner eyeball:** for each model, the chosen orientation rendered against its **3 nearest
Pareto neighbours**. The owner is adjudicating the subjective half of "better": sometimes
the second-best point on the numbers is the right print.

---

## M9 — Deletion

**O-6 precedence:** this deletion work moves to the revised acceptance/delete step
and is not blocked by post-Python M5–M8 enhancements. Its clean-checkout re-fire and
zero-Python assertions remain binding.

**O-7 timing:** the inventory below is also the final zero-Python checklist. Each
compatibility row should already have been deleted at its native handoff; delete any
remainder here, and treat retention for optional compatibility as a failed checkpoint.

**Goal.** Zero Python.

**Current preconditions under O-6:** essential artifact parity, ChituBox loadability,
deterministic artifacts/reports, and the owner-print checkpoint have passed. The old
M7/M8 dependency below is reference material, not a deletion blocker.

**Depends on:** revised critical-path step 6. **Parallel with:** nothing.

### Delete

| Path | Note |
| --- | --- |
| `scripts/support_placement/` | The entire package, all 35 `.py` files including `__init__.py`, `geometry_exact.py` and `geometry_kernel.py` |
| `scripts/tools/` | All 19 `.py` files, plus `host.blend` |
| `scripts/tests/` | All `test_*.py`, all `manual_*.py`, `conftest.py`, `_refreeze_v5.py`, `_perf_fingerprint.py`. **Keep `scripts/tests/fixtures/chitubox/`** — move it to `corpus/fixtures/chitubox/` and repoint `rust/chitubox_io/tests/common/mod.rs::required_fixture_path()` plus the sole optional `optional_treant_candidate_path()` fallback. Those are the native ChituBox ground-truth specimens; they are not test infrastructure, they are irreplaceable data. |
| `scripts/build_kernel.sh` | Builds both PyO3 extensions into the venv; nothing to build |
| `rust/chitubox_io/src/py.rs` | PyO3 binding |
| `rust/chitubox_io/python/` | Package shim (`chitubox_io/__init__.py`) |
| `rust/geom_kernel/src/lib.rs` | **Strip the inline PyO3 surface — this crate has NO `py.rs`.** Remove the `use numpy::…` / `use pyo3::…` imports (lines 31-35), the `#[pyclass]` `NativeSurface` wrapper and its `#[pymethods]` block (line 70ff), the `#[pymodule]` for `geom_kernel._native` (line 347), and the `#[pyfunction] current_threads` (line 356). ~330 lines. Do this **in the same commit** that drops the pyo3/numpy dependencies below, or `cargo test --workspace` fails to compile. |
| `rust/geom_kernel/python/` | Package shim (`geom_kernel/__init__.py`) |
| `rust/chitubox_io/pyproject.toml`, `rust/geom_kernel/pyproject.toml` | maturin configs |
| `pyproject.toml`, `uv.lock`, `.venv/` | The Python project itself |
| `__pycache__/`, `.pytest_cache/` | Anywhere in the tree |

Also, in both `rust/chitubox_io/Cargo.toml` and `rust/geom_kernel/Cargo.toml`: drop the
`cdylib` crate type (leave `rlib`), remove the `pyo3` and `numpy` dependencies, and remove
the `python` feature. Both crates then build with no interpreter anywhere in the picture.

### Keep

- **`corpus/baseline/`** — it remains the frozen differential oracle. Deleting Python does
  not delete the measuring stick. `corpus/manifest.json` records the git revision it was
  frozen at; that history is now unreproducible, which is exactly why the artifacts stay.
- `docs/reference/*.md` and `docs/specs/*.md` — historical reasoning and format law. Add a
  supersession banner where §5 below says to.

### Docs and skills to update

| File | Action |
| --- | --- |
| `skills/prep-mini/SKILL.md` | Mark **superseded**; point to `plinth prepare` |
| `skills/decimate-for-print/SKILL.md` | Mark **superseded**; point to `plinth prepare`; note Blender is retired |
| `skills/support-placement/SKILL.md` | Mark **superseded**; point to `plinth prepare` |
| `skills/chitubox-support-injection/SKILL.md` | Mark **superseded**; the write path it fronts (`chitubox_inject.py`) is condemned and gone |
| Repo-root `README.md` (not this guide's) | Rewrite the build/run sections: `cargo build --release`, `plinth prepare`, `plinth score`, `plinth calibrate`. No `uv`, no `pytest`, no `build_kernel.sh`. |
| `docs/specs/SPEC-RUST-KERNEL.md` | Add a supersession banner (see §5) |
| `docs/reference/port_plan.md`, `port_plan_codex_review.md` | Add supersession banners (see §5) |
| `docs/rust-migration/constants.md` | Regenerated by the M0 golden test; confirm it is current |
| `.gitignore` | Remove the Python/maturin rules and their comment: `__pycache__/`, `*.pyc`, `.pytest_cache/` (lines 1-3) and the maturin block whose comment reads "maturin drops the compiled PyO3 extension into the crate source tree" (line 17ff). Left in place they are the last Python residue in the tree, and they make the acceptance grep below noisy for no reason |
| `skills/prep-mini/references/*.md`, `skills/support-placement/references/*.md` | The `SKILL.md` supersession banners cover their directories; these reference docs stay as historical reasoning and are **not** rewritten. They legitimately still mention `.py` paths — that is why the acceptance grep excludes `skills/` |

### Acceptance — run these

```
test ! -d scripts && test ! -f pyproject.toml && test ! -f uv.lock && test ! -d .venv
test ! -f rust/chitubox_io/src/py.rs && test ! -d rust/chitubox_io/python
test ! -d rust/geom_kernel/python

# geom_kernel has NO py.rs -- its PyO3 surface is inline in lib.rs, so a `test ! -f`
# assertion on a path that never existed passes trivially on an untouched tree. Check
# CONTENT, not existence:
rg -n 'pyo3|pymodule|pyfunction|pyclass|numpy' rust/geom_kernel/src/    # no hits
rg -n 'pyo3|pymodule|pyfunction|pyclass|numpy' rust/chitubox_io/src/    # no hits

rg -l "pyo3|rust-numpy|numpy" rust/*/Cargo.toml       # no hits
rg -n --glob '!docs/**' --glob '!corpus/**' --glob '!skills/**' \
      '\.py\b|uv run|pytest|maturin' .
# no hits outside docs/, corpus/ and skills/. skills/ is excluded because M9 KEEPS those
# files and only banners them superseded -- they legitimately still describe the retired
# Python workflow. .gitignore is not excluded: its Python/maturin rules are deleted above,
# so a hit there is a real miss.

test -z "$(find . -name '*.py' -not -path './target/*')"   # zero .py files remain

cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check

cargo run -p plinth-cli --release -- prepare \
    --input-models-dir corpus/models --output saturn4-ultra \
    --out-dir corpus/candidate-final

cargo run -p plinth-cli --release -- score \
    --baseline corpus/baseline --candidate corpus/candidate-final \
    --manifest corpus/manifest.json
```

Then prove it from a clean checkout, because "works in my tree" is not done:

```
CLEAN="${TMPDIR:-/tmp}/plinth-clean-$$"
git clone . "$CLEAN" && (cd "$CLEAN" && cargo test --workspace) ; rm -rf "$CLEAN"
```

This M9 clean-checkout clone is the **only additional out-of-repo write** beyond the
explicitly authorized M0 durable corpus/oversized-fixture carve-out in
`06-conventions.md` §7. It reads nothing back into the working tree and the clone is
deleted after the run. Do not generalize either carve-out into permission to work in
`$TMPDIR`.

**Parity gate:** the full M7 gate **re-fires post-deletion, from a clean checkout**. If
an earlier native-handoff compatibility removal changed a number, you find out here.
**Owner eyeball:** the owner **prints one model** from a Rust-emitted file. That is the
real acceptance test and always was.

---

## 4. Disposition audit — every non-test `.py` file, exactly once

Each file appears in exactly **one** milestone's retired-or-deleted list. Where work spans
milestones, the row records the milestone at which the file's role **ends**, and the span in
the note. **56 files**: 35 in `scripts/support_placement/`, 19 in `scripts/tools/`, plus the
two Rust-crate Python package shims. (Repo total is 102 `.py`, of which 38 are `test_*.py`;
the remaining 8 non-test files under `scripts/tests/` are covered by the test-infrastructure
paragraph below, giving 56 + 8 = 64 non-test files — the number `01-current-system.md` §13
audits.)

| # | File | Retires at | Disposition |
| --- | --- | --- | --- |
| 1 | `scripts/support_placement/chord_canon.py` | **M0** | Ported → `plinth-core::chord` |
| 2 | `scripts/tools/census_core.py` | **M1** | Superseded → `plinth-metrics` (independence preserved) |
| 3 | `scripts/tools/census_run.py` | **M1** | Superseded → `plinth-metrics` |
| 4 | `scripts/tools/p11_crossexam.py` | **M1** | Superseded → `plinth-metrics` + M7 fixer-efficacy tests |
| 5 | `scripts/tools/validate_buried_end_fix.py` | **M1** | Superseded → `plinth-metrics` |
| 6 | `scripts/tools/validate_r3_ball_gate.py` | **M1** | Superseded → `plinth-metrics` |
| 7 | `scripts/support_placement/mesh_io.py` | **M2** | Reimplemented → `plinth-mesh::io` + `fixtures` |
| 8 | `scripts/support_placement/topology.py` | **M2** | Reimplemented → `plinth-mesh::topology`; digest redefined, schema 3→4 |
| 9 | `scripts/tools/decimate_pipeline.py` | **M3** | Logic converted → `plinth-decimate`; Blender retired |
| 10 | `scripts/tools/geometry_sweep.py` | **M3** | Deleted with reason — Blender-only repair; triage + cut flags refuse instead |
| 11 | `scripts/support_placement/pipeline.py` | **M4** | Reimplemented → `plinth-pipeline` + `plinth-cli` |
| 12 | `scripts/support_placement/hollow.py` | **M4** | Reimplemented → `plinth-mesh::triage`, `plinth-orient`, `plinth-hollow` |
| 13 | `scripts/support_placement/emit.py` | **M4** | Reimplemented → `plinth-emit` |
| 14 | `scripts/support_placement/chitubox_emit.py` | **M4** | Reimplemented f64 + native winding → `plinth-emit` |
| 15 | `scripts/support_placement/prepared_validation.py` | **M4** | Deleted — replaced by ownership (typestate / `Validated<T>`) |
| 16 | `scripts/support_placement/chitubox_inject.py` | **M4** | Not ported — write path condemned by its own docstring; read role in `chitubox_io` |
| 17 | `scripts/tools/prep_one.py` | **M4** | Superseded → `plinth prepare` |
| 18 | `scripts/tools/hollow_drill.py` | **M4** | Superseded → single CLI |
| 19 | `scripts/tools/emit_supports.py` | **M4** | Superseded — bypass path eliminated |
| 20 | `scripts/tools/run_v5.py` | **M4** | Superseded — direct-engine bypass |
| 21 | `scripts/tools/run_engine_full.py` | **M4** | Superseded — direct-engine bypass |
| 22 | `scripts/tools/recheck_rewire_cap.py` | **M4** | Superseded — direct-engine bypass |
| 23 | `scripts/tools/recon_chitubox.py` | **M4** | Deleted — analysis-only; findings in `CHITUBOX-FORMAT.md` |
| 24 | `scripts/support_placement/perception.py` | **M5** | Delicacy half → `plinth-mesh` (M2); islands/minima/births → `plinth-support` (M5) |
| 25 | `scripts/support_placement/placement.py` | **M5** | Reimplemented → `plinth-support` pool/lattice/sizing |
| 26 | `scripts/support_placement/coverage.py` | **M5** | Reimplemented → one generic dyadic builder |
| 27 | `scripts/support_placement/specialized_contacts.py` | **M5** | Redesigned batched → `plinth-support` |
| 28 | `scripts/support_placement/internal.py` | **M5** | Reimplemented batched → `plinth-hollow` |
| 29 | `scripts/support_placement/tree.py` | **M6** | Reimplemented → `plinth-support::forest` arena |
| 30 | `scripts/support_placement/cco.py` | **M6** | Reimplemented → `plinth-support::cco` |
| 31 | `scripts/support_placement/rewire.py` | **M6** | Reimplemented — deterministic work budget only |
| 32 | `scripts/support_placement/bracing.py` | **M6** | Reimplemented → `plinth-support::brace` |
| 33 | `scripts/support_placement/transient.py` | **M6** | Reimplemented — sizing returns values, pinned semantics |
| 34 | `scripts/support_placement/joints.py` | **M6** | Ported as coded → `plinth-support::joints` |
| 35 | `scripts/support_placement/routing.py` | **M6** | Exact-oracle role → `geom_kernel`; sampled path not ported |
| 36 | `scripts/support_placement/struts.py` | **M6** | One function ported; rest deleted (v3 dead) |
| 37 | `scripts/support_placement/trunks.py` | **M6** | Deleted — dead v3, replaced by Forest/CCO |
| 38 | `scripts/support_placement/braces.py` | **M6** | Deleted — dead v3 (+`networkx` dep) |
| 39 | `scripts/support_placement/verify.py` | **M7** | Reimplemented → `plinth-verify` |
| 40 | `scripts/support_placement/removal.py` | **M7** | Reimplemented as opt-in diagnostic → `plinth-verify` |
| 41 | `scripts/support_placement/phase8.py` | **M7** | Redesigned from invariants → `plinth-support::repair` |
| 42 | `scripts/support_placement/phase8_driver.py` | **M7** | Redesigned from invariants → `plinth-support::repair` |
| 43 | `scripts/support_placement/p8_accept.py` | **M7** | Gates wired as kinds + tests into the shipped accept path |
| 44 | `scripts/support_placement/engine.py` | **M7** | Reimplemented → `plinth-pipeline` (orchestration, M4) + `plinth-support` (fix loop, M7) |
| 45 | `scripts/support_placement/config.py` | **M7** | Reimplemented → `plinth-core` typed constants registry; conversion spans M0–M7 |
| 46 | `scripts/support_placement/corpus.py` | **M8** | Analysis half → `plinth-metrics` (M1); plate-analysis role → `plinth calibrate` (M8) |
| 47 | `scripts/tools/bench_kernel.py` | **M8** | Superseded → criterion benches |
| 48 | `scripts/tools/replay_parity.py` | **M8** | Superseded → determinism CI |
| 49 | `scripts/tools/instrument.py` | **M8** | Superseded → determinism CI |
| 50 | `scripts/tools/common.py` | **M8** | Superseded → cargo/CLI conventions |
| 51 | `scripts/tools/_toolpaths.py` | **M8** | Superseded → cargo/CLI conventions |
| 52 | `scripts/support_placement/__init__.py` | **Native handoff/final remainder** | Deleted when the package's remaining native boundaries land |
| 53 | `scripts/support_placement/geometry_exact.py` | **Native kernel handoff** | Already ported (`geom_kernel`); oracle role transfers to frozen disk artifacts + `plinth-metrics` |
| 54 | `scripts/support_placement/geometry_kernel.py` | **Native kernel handoff** | Deleted with the PyO3 wrapper; native callers use `geom_kernel` directly |
| 55 | `rust/chitubox_io/python/chitubox_io/__init__.py` | **Native IO handoff** | Deleted with the PyO3 surface |
| 56 | `rust/geom_kernel/python/geom_kernel/__init__.py` | **Native kernel handoff** | Deleted with the PyO3 surface |

**Test infrastructure** (`scripts/tests/*.py` — all `test_*.py`, all `manual_*.py`,
`conftest.py`, `_refreeze_v5.py`, `_perf_fingerprint.py`) is retired piecemeal when its
Rust test boundary lands; no exemption exists merely to keep Python runnable. Any remainder
deletes at the final gate. Its **pinned
semantics** — determinism contracts, wall-time pins, not-worse-than-baseline gates — are
re-expressed as Rust tests at the milestone that ports each subject. Three of those
re-expressions are **assigned deliverables**, not aspirations, because
`01-current-system.md` §12 marks their concepts keep-as-Rust-dev-tool:

| Python concept | Rust re-expression | Lands at |
| --- | --- | --- |
| `conftest.py`'s **fail-not-skip** rule (missing/stale fixture ⇒ FAIL, never a silent skip) | A workspace test convention | **M0** work item 9 |
| `_refreeze_v5.py`'s **baseline refreeze** (the tool that re-freezes after a physics change, `03-physics-and-constants.md` §4.3 step 3) | `plinth-cli`'s internal `refreeze` path | **M1** work item 6 |
| `_perf_fingerprint.py`'s **exact-bytes determinism fingerprint** (f64 as raw bit pattern, arrays as dtype + shape + raw bytes; never tolerance comparison) | The digest the thread- and cache-sweeps are computed with | **M8** determinism CI |

Two more worth naming:
`test_engine.py::test_run_bit_identical_across_thread_counts_on_table` becomes M6's
`determinism_under_threads`, and
`test_engine.py::test_table_and_toed_slab_full_run_not_worse_than_baseline` is the closest
thing in-repo to the parity harness — **read it before finalizing M1's scoring gate.**

---

## 5. Where this plan supersedes prior repo documents

Prior art in this repo is worth reading and is wrong in specific, identifiable ways. **This
plan supersedes it where they conflict.** Add a banner to each file named below when you
reach M9.

| Document | What it says | Why this plan supersedes it |
| --- | --- | --- |
| `docs/reference/port_plan.md` | Central recommendation: **"stay Python"**; a full Rust port is "not recommended." Also proposes tactics **C2** (independent-tree parallelism) and **C3** (whole-sweep speculative replay). | **Superseded on the language question.** The verdict was reached on a performance cost/benefit basis, and this migration is not a performance decision — it exists to remove Python's influence on future agent work in this repo (§0). `SPEC-RUST-KERNEL.md` already overrode it once on language grounds after a 576 k-face run took 2 h 22 m without finishing; this plan completes that override. **C2 and C3 are separately dead** — the Codex review adjudicated them unsound (trees are not independent: root reanchor and bifurcation cross trees, the KD cache is forest-wide, and the convergence test is global) and Rust's cheap threading does not make an unsound decomposition sound. **What survives intact:** §5's serial-generate / parallel-evaluate / serial-commit discipline (M6 uses it verbatim) and its identification of `rewire_time_cap_s` as a determinism defect (M6 fixes it). |
| `docs/reference/port_plan_codex_review.md` | Adversarial review of the above; confirms the wall-clock-cap determinism defect and rejects C2/C3. | **Not superseded on findings — superseded only on its "stay Python" conclusion**, for the same reason. Its determinism finding is *adopted* as a binding M6 requirement. Read it. |
| `docs/specs/SPEC-RUST-KERNEL.md` | Binding 2026-07-23 ruling that the kernel goes to Rust. Scope: a **PyO3 hybrid** — a Rust kernel under a Python orchestrator. States the Python `ExactSurface` stays **"permanently"** as the differential-test oracle, and lays out Stages 2–5, none of which started. | **Superseded on scope, adopted on substance.** The revised acceptance/delete step removes the Python orchestrator and `geometry_exact.py`. The oracle role transfers to `corpus/baseline/`, fresh disk reads, and analytic/corrupt/native fixtures using the sole geometry implementation. Its numerical hazard notes remain useful; its redesign roadmap is post-Python work under O-6. |
| `docs/specs/SPEC-EMBED-POLICY.md` | SCAR-AREA-CAP and PRINT-FLOOR normative constants and formulas. | **Not superseded.** M7 implements it bit-faithfully. The one carve-out: the retired `COS_FLOOR = 0.5` (60° clamp) is *not* reintroduced — it conceals grazing growth, and the spec already abolished it. |
| `docs/specs/SPEC-P8-RELOCATE.md`, `SPEC-P8-REACH.md` | The relocation ladder, as amended repeatedly. | **Superseded as an implementation blueprint; adopted as an invariant source.** M7 re-derives a unified ladder from the stated invariants (see the M7 invariant table) rather than transliterating ~4 800 lines of patch history. `SPEC-P8-REACH`'s **concession ladder** is explicitly not ported. |
| `docs/specs/SPEC-CHITUBOX-RS.md`, `docs/specs/CHITUBOX-FORMAT.md` | The `.chitubox` container and record format. | **Not superseded — this is the format authority.** `rust/chitubox_io` already implements it. See `05-chitubox-and-corpus.md`. |
| `docs/specs/SPEC-V5-GEOM.md`, `SPEC-V5-PERF.md`, `SPEC-A3-A4-COVERAGE.md` | Surface-exact geometry law, perf levers, coverage gate. | **Not superseded on the geometry law** (exact Möller-Trumbore boundaries + multi-axis ray-parity majority; never trust a nearest-face signed-distance sign). **Superseded on the perf levers** — `SPEC-V5-PERF`'s CPython-specific caches, memory budgets and byte-accounting are artifacts of running numpy under an interpreter and do not carry over. |
| `docs/reference/physics_*.md`, `docs/datasheets/**` | Physics reasoning and external material/hardware reference data. | **Not superseded.** They are the provenance sources for `plinth-core::constants`; `03-physics-and-constants.md` is their index for this migration. Note that `docs/datasheets/README.md` already documents that `config.py`'s `peel_pressure_pa` and `shaft_drag_force_n` admit in their own comments to being unmeasured guesses — that is the in-repo evidence for why `Placeholder` must fail the build on the accept path. |
| `skills/*/SKILL.md` | Blender-MCP-driven operator playbooks. | **Superseded as workflow** (M9). Only the algorithm inside `decimate-for-print` is in scope, and M3 carries it. Blender is retired: its scripting is Python and it segfaults headless in sandboxes. |

---

## 6. Standing hazards — re-read this list at every milestone

These are the failure modes this project has actually experienced. They are not
hypothetical, and every one of them recurred more than once.

1. **Validating something other than what you shipped.** The project once validated an
   in-memory mesh while writing a different one to disk. Every audit re-reads the file.
2. **Fixing at the site instead of at the source.** The coincident-coplanar-face defect from
   concatenating touching primitives was fixed at four individual sites before anyone fixed
   the concatenation. When a bug appears twice, stop fixing instances.
3. **A gate with no fixer.** Eleven of twelve acceptance gates in the Python engine can fail
   with nothing anywhere that could satisfy them. The `ViolationKind` exhaustive match is
   the structural answer; do not route around it with a catch-all arm.
4. **Anonymous failure.** `termination='attempt-cap'`, bare `RuntimeError`, `assert not
   violations`, and exit code 5 with a stringified message are four different vocabularies
   for "it did not work." One `DefectReport`, always naming the subject.
5. **A second entry point.** Five of them exist today, and one of them silently skips
   orientation and hollowing entirely. Every new CLI flag is a chance to make a sixth.
6. **A per-item call in a hot path.** It is a defect, not a style choice, in every language.
   Gather, one batched call, scatter. The Python fix loop rebuilds the **entire forest** per
   repair attempt (up to nine full construct + rewire + brace passes) — that single choice is
   the largest driver of the 86–872 s runtime and the reason orientation search was cut to
   13 candidates in the first place.
7. **Non-determinism smuggled in as a performance lever.** A wall-clock cap, an unordered
   parallel reduction, a hash-map iteration order. `geom_kernel` already bans `HashMap` in
   `src/` with a test (`r6_no_hashmap_in_src`) — that is the standard.
8. **Promoting a Placeholder to Measured because it looks physical.** `peel_pressure_pa =
   3000` and `tensile_green_mpa = 0.288` are a **single measured ratio split three ways for
   modelling convenience**. Only `X_allowed = 12` is measured. Store the ratio; keep the
   split honestly labelled.
