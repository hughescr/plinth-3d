# 04 — Migration Plan (NORMATIVE)

**This is the document you execute.** It is a DAG of ten milestones, M0 through M9, that
takes `plinth-3d` from "a Python engine with two Rust crates bolted on via PyO3" to
"a pure-Rust workspace with zero Python."

Read `README.md`, `01-current-system.md`, `02-target-architecture.md` and
`03-physics-and-constants.md` first. Read `06-conventions.md` before your first commit.
`05-chitubox-and-corpus.md` is the format authority for everything M4 and later emits.

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
3. Gate on Rust being **as-good-or-better on EVERY metric on EVERY model** (M7).
4. At each milestone, hand the owner a named **eyeball checkpoint** for the subjective part.

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

### Four rules that bind every milestone

1. **Additive until M9.** The frozen Python engine stays runnable as the measuring stick.
   No Python file is deleted before M9. "Python mapped here" in a milestone means the Rust
   replacement lands and the Python file's *role* ends — the file stays on disk.
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

Commit **per milestone** on `master`. Never use `git checkout --`, `git restore`,
`git reset --hard`, or `git clean` — they silently destroy uncommitted work. Never
force-push. Python deletions happen only in M9, after the M7 gate passes with owner
sign-off and M8's determinism CI is green.

---

## 1. The DAG

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
| M0 → M1, M0 → M2 | Nothing starts until the baseline is frozen and the shared types exist. |
| M1 ∥ M2 | Fully parallel. The harness is written before the things it measures. |
| M3 ∥ M4 | Both depend on M2 and neither depends on the other. M4 uses `--orient pinned:` until M3's proxies exist; M3 gates at the library level so it does not wait on M4's `prepare` subcommand. |
| M1 → M4 | M4's first act is to run `plinth score`; the harness must already work. |
| M4 → M5 → M6 → M7 | Strictly sequential. M7's repair stage hard-depends on M6's forest and on M7's own certificate primitives. |
| M3 → M8 | The dense orientation sweep needs the decimated proxy ladder. |
| M7 → M8 → M9 | M8 may start its calibration plumbing after M3+M4, but its final gate needs M7. M9 is strictly last. |

**What two agents can work on at once:** (M1, M2) after M0; (M3, M4) after M2. Everything
else is a chain. Do not try to parallelize M5/M6/M7 — each one's fixtures are built on the
previous one's data model.

---

## 2. Milestone summary

| # | Name | Depends on | New crates | Gate |
| --- | --- | --- | --- | --- |
| M0 | Conventions, frozen-Python baseline, core types, solid spike | — | `plinth-core`, `plinth-solid` | Baseline frozen; trybuild compile-fail proof |
| M1 | The parity harness and Rust refreeze (`plinth score`) | M0 | `plinth-metrics`, `plinth-cli` | Self-comparison exact; injected defects detected |
| M2 | Mesh and geometry substrate | M0 | `plinth-mesh`, `plinth-voxel` | STL round-trip byte-identity; geom_kernel green |
| M3 | Decimation | M2 | `plinth-decimate` | Self-verified error budgets on every model, at the **library** level (`cargo test -p plinth-decimate --test corpus_budgets`); CLI wiring verified at M4 |
| M4 | The thin vertical slice | M1, M2 | `plinth-cost`, `plinth-orient`, `plinth-hollow`, `plinth-emit`, `plinth-pipeline` | Format metrics absolute; `islands`, `support_count`, `resin`, `scar` waived; `max_x` reported |
| M5 | Perception, placement, load model | M4 | `plinth-support` | Load gate becomes satisfiable |
| M6 | Forest, growth, sizing | M5 | (extends `plinth-support`) | Resin + clearance metrics at-or-better |
| M7 | Verify + repair complete — **THE GATE FIRES** | M6 | `plinth-verify` | Full gate, every metric, every model |
| M8 | Search quality + calibration + determinism CI | M3, M7 | (extends existing) | M7's gate held under dense sweep |
| M9 | Deletion | M7 sign-off, M8 green | — | Full gate re-fires from a clean checkout |

---

## 3. The metric set (defined once — M1 builds it, every later milestone uses it)

Every metric is computed by `crates/plinth-metrics` on an artifact read from disk. Ids are
stable and are what you pass to `--allow-worse`.

| Id | Metric | Direction | Hard bound | Notes |
| --- | --- | --- | --- | --- |
| `islands` | Unsupported overhang islands remaining in the emitted scene | ↓ lower better | — | Recomputed by plinth-metrics' own geometry, not read from a report |
| `cavities` | Closed interior cavities in the emitted solid union | ↓ | — | The defect ChituBox surfaced four times from coincident coplanar faces |
| `boundary_edges` | Edges with exactly one incident face | ↓ | **= 0** | Absolute from M4 onward |
| `negative_shells` | Connected shells with negative signed volume | ↓ | **= 0** | Absolute from M4 onward |
| `winding` | Faces whose winding disagrees with their shell | ↓ | **= 0** | Absolute from M4 onward |
| `max_x` | Max **per-arm** X = A_voronoi_cell / (π·r_tip²) | ↓ | — | **Definition pinned at M1 and never changed:** per-arm, exactly the `placement.load_partition` semantics in `03-physics-and-constants.md` §2.3. Arm-**group** effective X under the resolved `LoadSharing` is a *separate* metric id, `max_x_group`, added at M5 when `ArmGroup` exists. Target is `≤ 12`; gated **relatively** (at-or-better vs baseline) from M5, **absolutely** on the candidate from M7 |
| `max_x_group` | Max arm-group effective X under the resolved `LoadSharing` | ↓ | — | Added at **M5** (it needs `ArmGroup` + `LoadSharing`, which do not exist before then). Reported-only until M7, then gated relatively. Adding it at M5 is not a retrofit of the signed table: it is a *new* id, and `max_x` keeps its M1 meaning untouched |
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

**Hard bounds are enforced on the CANDIDATE only, and only from the milestone named in
the `Notes` column.** They are never applied to the baseline — the frozen Python output
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

**Depends on:** nothing. **Parallel with:** nothing. **Unblocks:** M1, M2.

### Owner prerequisite — read this before starting

First, the venv. Every `uv run --frozen` and every `@pytest.mark.kernel` test in this plan
needs `.venv/` (gitignored, so absent from a fresh checkout) with the compiled PyO3
extensions in it. Bootstrap once, before the commands below:

```
uv sync                                              # creates .venv from uv.lock
.venv/bin/python -m pip install "maturin==1.8.3"     # pinned; needs network
scripts/build_kernel.sh                              # builds geom_kernel + chitubox_io into .venv
```

`corpus/models/` must contain the Treant STL. **The model exists.** As of 2026-07-31 the
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
know the alternate name, so **set `$TREANT_STL` explicitly**:

```
export TREANT_STL="/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Awakened Tree.stl"
```

**`minis-to-process/` is transient staging — the owner clears it after printing**, so that
path is not durable. The durable home is the same out-of-repo directory that holds
`treant_v5_candidate.chitubox` — `$CHITUBOX_CORPUS_DIR`, defaulting to the repo's parent
(`05-chitubox-and-corpus.md` §4). Copy the STL there under the name the manifest and
`resolve_stl()` expect (`Weeping Treant v1.2.stl`) and point `$TREANT_STL` at the copy.

**Never commit it.** At 21 MB it follows the same rule as `treant_v5_candidate.chitubox`:
out of the repo, located by environment variable. `corpus/` is **not** in `.gitignore`
today, so whoever first creates `corpus/models/` adds `corpus/models/*.stl` to it in the
same commit.

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
baseline over the seven repo-obtainable models — the five synthetic fixtures plus
`scripts/tests/fixtures/chitubox/Sphere.stl` and `Cylinder.stl` — record
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
| `scripts/support_placement/chord_canon.py` | Ported verbatim → `plinth-core::chord` |

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
| `violation.rs` | The passive exhaustive `ViolationKind` vocabulary only. `Response`, `FixerId`, and `response_for` are M7 `plinth-support::repair` behaviour, when concrete fixers exist. Commitment 3 is enforced there. |
| `provenance.rs` | `Measured<T> { plate: PlateId, value: T, err: Option<T> }`, `Derived<T>`, `Placeholder<T>`, `Pending<T>`, and the `AcceptSafe` marker trait. Categorical measurements serialize `err: None` explicitly as `"err": null`; no numeric pseudo-error or omitted field. Commitment 2. |
| `constants.rs` | The typed constants registry. Every physical constant in the system, each carrying its provenance. |
| `layer.rs` | `LayerPlan` — layer height is a **per-layer array from day one**, commitment 5. Ships `constant(0.050)` until calibrated. |
| `frame.rs` | Frame typestate marks (`Source`, `Centered`, `Plated`) so a mis-framed point set will not compile. |
| `stage.rs` | The `Stage` trait every pipeline step implements. |
| `digest.rs` | Content digests over typed arrays (not over allocator arithmetic — see the M2 note on `topology.py`). |
| `chord.rs` | `canon_chord_micron`, `chords_match_micron`, `chord_parity_report`, ported from `chord_canon.py`. Integer-micron canonicalization; dependency-free by design. |
| `printer.rs` | `PrinterProfile`. Ship `saturn4-ultra`: peel direction −Y (tilt release), pixel 14 × 19 µm, layer 0.050 mm. |
| `cost.rs` | Passive shared cost/report DTOs: `CostVector`, `Fidelity`, `Resin`, `Scar`, `Risk`, `OrientationCost`, `ParetoPoint`, `OptimizationSummary`, and `SearchEvidence`. Pre-orientation reports use `SearchEvidence::NotEvaluated`, never a sentinel cost. Evaluation, scalarization, batching, Pareto algorithms, and memoization remain future `plinth-cost` behaviour. |
| `metric.rs` | Passive shared `MetricId`, `MetricValue`, `MetricSet`, and report DTOs. Metric directions, independent metric computation, comparison, and waiver behaviour remain M1 `plinth-metrics`. |

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
5. **The constants golden test.** A test that renders the registry to
   `docs/rust-migration/constants.md` and fails if the committed file differs. The doc is
   generated, never hand-edited.
6. Create `corpus/models/` **inside the repo** and populate it:
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
7. Freeze the baseline (see acceptance). Write `corpus/manifest.json`.
8. Spike `plinth-solid`.
9. **Adopt the fail-not-skip fixture rule as a workspace convention.**
   `scripts/tests/conftest.py:158` documents it — "missing/stale/version-mismatch => FAIL,
   never silently skip". Carry it into the Rust harness: a test whose required fixture,
   corpus model or baseline artifact is missing **fails with a named reason**; it does not
   `return` early, and it does not print a skip notice and pass. The one sanctioned skip in
   the whole tree is `chitubox_io`'s `treant_v5_candidate.chitubox` (see
   `05-chitubox-and-corpus.md` §4), which is uncommittable by size.

### `corpus/manifest.json` schema

Fix this now; M1 reads it and every later milestone depends on it.

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

**Depends on:** M0. **Parallel with:** M2. **Unblocks:** M4.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/tools/census_core.py` | Superseded → `plinth-metrics` (independent-audit role transfers) |
| `scripts/tools/census_run.py` | Superseded → `plinth-metrics` |
| `scripts/tools/p11_crossexam.py` | Superseded → `plinth-metrics` + M7's fixer-efficacy tests |
| `scripts/tools/validate_buried_end_fix.py` | Superseded → `plinth-metrics` |
| `scripts/tools/validate_r3_ball_gate.py` | Superseded → `plinth-metrics` |

### Rust deliverables

**`crates/plinth-metrics`** — the independent auditor.

> **Independence is a hard requirement.** `plinth-metrics` must **not** depend on
> `plinth-verify` and must **not** depend on `geom_kernel`. It carries its **own slow
> brute-force f64 geometry**. It may depend on `chitubox_io` for the *reader* only.
> This preserves the property that made the Python census valuable: two implementations
> that disagree reveal a bug, and one implementation cannot audit itself. `census_core.py`
> exists today precisely because `geometry_exact.py` was ported from it — do not collapse
> them back together.

| Module | Contents |
| --- | --- |
| `read.rs` | Re-open a `.chitubox` from disk, extract models + support records + meshes. |
| `topology.rs` | Boundary edges, shells, signed volumes, winding consistency, closed cavities. Brute force; no BVH. |
| `supports.rs` | Support count, per-record resin volume, arm cross-sections, effective X. |
| `scar.rs` | Scar area, worst patch, delicacy weighting. |
| `geometry.rs` | Brute-force closest-point, ray parity, triangle intersection. Slow on purpose. |
| `score.rs` | Independent metric computation plus the as-good-or-better comparator and waiver handling, all over `plinth-core`'s passive `MetricSet` and report DTOs. |

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

1. Scaffold `plinth-metrics` with its own geometry. Resist reaching for `geom_kernel`.
   It also carries its **own independent delicacy implementation** for `scar_weighted`,
   built from the weights in `03-physics-and-constants.md` §3.8
   (`delicacy_curvature_weight = 0.6`, `delicacy_thinness_weight = 0.4`,
   `thin_feature_mm = 1.5`). It may **not** link `plinth-mesh::delicacy` — the harness
   sharing the producer's delicacy field would let one field bug make producer and judge
   agree, which is exactly the independence this crate exists for. Slow is fine.
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

**Depends on:** M0. **Parallel with:** M1. **Unblocks:** M3, M4.

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

**`rust/geom_kernel`** — extend, do not rewrite.

- **Correction to prior planning notes:** `geom_kernel` **already declares
  `crate-type = ["cdylib", "rlib"]`** in `rust/geom_kernel/Cargo.toml`. The rlib target
  exists. What is actually missing is that `pyo3` and `numpy` are **non-optional**
  dependencies there, so any crate linking `geom_kernel` drags in libpython. Fix that by
  mirroring what `rust/chitubox_io/Cargo.toml` already does: make them
  `optional = true` behind a `python` feature (`python = ["dep:pyo3", "dep:numpy"]`,
  `default = []`), and have `rust/geom_kernel/pyproject.toml`'s maturin config enable it.
- Add `trait SurfaceOracle` so `plinth-*` crates depend on the trait, not the crate.
- Add **point-kNN** and **radius-pairs** queries. These are the `scipy.spatial.cKDTree`
  replacements that M5 and M6 need (`query_pairs` in `joints.compute_joint_balls`,
  ball-point queries in `engine._ensure_minima_contacts`).
- **Leave the PyO3 surface working.** Python still runs until M9.

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
6. Extend `geom_kernel` as above; keep its existing suites green.
7. **Rebuild the extension before the pytest parity run — this will otherwise fail for a
   reason unrelated to numerics.** `rust/geom_kernel/build.rs` bakes
   `GEOM_KERNEL_SOURCE_DIGEST` over the sorted `src/*.rs` **plus `Cargo.toml`,
   `Cargo.lock`, `build.rs` and `pyproject.toml`** (build.rs:29), and
   `scripts/tests/conftest.py:71-80` recomputes the same digest and hard-**FAILS** every
   `@pytest.mark.kernel` test on a mismatch ("missing/stale/version-mismatch => FAIL, never
   silently skip", conftest.py:158). Work item 6 edits exactly those two config files, so
   after editing them run:
   ```
   scripts/build_kernel.sh geom_kernel
   ```
   `scripts/build_kernel.sh` requires `.venv/bin/maturin` and errors out without it; if
   `.venv/` is absent from your checkout, run M0's venv bootstrap first. (`conftest.py`
   also honours `GEOM_KERNEL_ALLOW_STALE=1` as a dev escape — **do not use it here**: the
   whole point of this gate is to prove the feature change did not move the numerics.)

### Acceptance — run these

```
cargo test -p plinth-mesh -p plinth-voxel
# property tests: sphere/torus slicing at known z closes to the analytic loop;
# signed EDT matches brute force on random grids; topology cut-flag fixtures

cargo test -p plinth-mesh --test stl_roundtrip
# ARRAY identity over every file in corpus/models/: read -> write -> read must return a
# bit-identical vertex array and triangle index array IN THE SAME ORDER (f64 bits
# preserved, no implicit weld, no reorder, no f32 downcast). NOT file-byte identity: the
# 80-byte header, the per-facet normals and the attribute-count bytes are not part of the
# contract. See section 0 above.

cargo test -p geom_kernel
# rust/geom_kernel/tests/geometry.rs stays green, including
# deterministic_build_bit_identical and r6_no_hashmap_in_src

scripts/build_kernel.sh geom_kernel      # MANDATORY after work item 6 edits Cargo.toml /
                                         # pyproject.toml -- both are in build.rs's digest
uv run --frozen python -m pytest scripts/tests/test_kernel_parity.py scripts/tests/test_kernel_parity_baseline.py
# the Python-side parity oracle still agrees; it runs until M9
```

**Parity gate:** STL round-trip **array** identity over the corpus; `geom_kernel` suites
green; Python kernel-parity suites still green (you changed its Cargo features — prove you
did not change its numerics). A stale-binary FAIL from `conftest.py` is a rebuild, not a
numerics regression — clear it with `scripts/build_kernel.sh`, never with
`GEOM_KERNEL_ALLOW_STALE=1`.
**Owner eyeball:** a delicacy heat-map render of the Treant. Sanity: thin foliage hot,
trunk cold. If the trunk is hot, the curvature/thinness blend is wrong and M5's scar
weighting will be wrong with it.

---

## M3 — Decimation  ∥ M4

**Goal.** Convert `scripts/tools/decimate_pipeline.py`'s logic to native Rust —
**not reinvent it** (owner's explicit instruction). Blender is retired.

**Depends on:** M2. **Parallel with:** M4. **Unblocks:** M8.

### Python retired here

| Path | Disposition |
| --- | --- |
| `scripts/tools/decimate_pipeline.py` | Logic converted → `plinth-decimate`; Blender retired |
| `scripts/tools/geometry_sweep.py` | **Deleted with reason** — Blender-only degenerate-repair sweep; not ported. M2's triage + topology cut flags *refuse* what this repaired. The owner repairs pathological meshes upstream. |

`skills/decimate-for-print/SKILL.md` describes the *current interactive Blender workflow*,
not the target. Only its algorithm is in scope; the Blender/MCP delivery mechanism is not.
Mark it superseded in M9.

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

**Parity gate:** self-verified error budgets on every corpus model, at the library level. There is no Python
baseline to compare against here — `decimate_pipeline.py` was never wired into the
automated pipeline (grep confirms no module imports it), so decimation has always been a
manual out-of-band step. The budget *is* the gate.
**Owner eyeball:** before/after renders of the Treant at production ratio, plus the chosen
ratio per model. The owner is checking that foliage survived.

---

## M4 — The thin vertical slice (the most important milestone)  ∥ M3

**Goal.** A real `.chitubox` end-to-end from Rust with **deliberately naive** supports —
proving the contract closes: typestate pipeline, `DefectReport`, boolean union, on-disk
audit, format byte-contract, one CLI.

**It will score worse than baseline on support metrics. That is expected and stated in the
gate command.** The point of M4 is that the *pipe* works.

**Depends on:** M1, M2. **Parallel with:** M3. **Unblocks:** M5.

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

  1. Read `<baseline-dir>/<stem>.pipeline.json`. Walk `stages[]` to the entry named
     `hollow` and take `log.orientation.{axis, deg}`.
  2. **Absence of the `orientation` key means the identity rotation won.**
     `hollow.orient_search` writes the key **only inside** its `if s < best[0]` branch
     (`hollow.py:470`), so a model whose identity orientation was already best has no key
     at all. Absent ⇒ identity. Do not treat it as an error.
  3. Apply the rotation, then **reproduce the Python re-centering**, which the
     `{axis, deg}` pair does not capture: XY-center the bounding box and translate `zmin`
     to 0 (`hollow.py:472-474`). Skip this and every support lands offset.
  4. A model whose baseline `hollow` stage is missing or `SKIPPED` has no pinned
     orientation — `pipeline.py:379` records
     `reason='hollow=False (orientation search skipped too)'` for that case. Treat it as
     identity and record a non-blocking note in the `DefectReport`; do not emit a `Defect`
     and do not guess.
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
   `Raw → Triaged → [Decimated] → Oriented → Hollowed → Supported → Emitted`, each
   transition consuming the previous state by value. The `Decimated` mark and its
   transition are **declared here and filled by M3's `plinth-decimate`** — M4 passes
   through it identity-style until M3 lands, which is what lets M3 and M4 run in parallel.
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

- `support_count`, `scar*` — the supports are deliberately naive.
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

**Depends on:** M4. **Parallel with:** nothing. **Unblocks:** M6.

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
  non-degeneracy gate ships regardless. The peel front sweeps front-to-back in **−Y** on
  this printer (tilt release), which *predicted* that Y-arranged arms meet the front one at
  a time (zipper) while X-arranged arms meet it together (sharing) — **plate 6 measured no
  such axis dependence** (03 §4.2 caveat 5), so arrangement enters through `ArmGroup`
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
scope caveats (one cap area, one arm diameter, one arm length; no upper bracket on
sharing; a ~20 % fin-root section bonus on the strip arms, so never credit more than N;
`X_tot` degenerate with total arm area on this plate) are in
`03-physics-and-constants.md` §4.2 and must travel in the constant's `note`. M8's A/B
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
   per-arm meaning the owner signed at M1. `plinth-metrics` computes it with its own
   independent geometry, as it does every other metric.

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

**Depends on:** M5. **Parallel with:** nothing. **Unblocks:** M7.

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

## M7 — Verify + repair complete: THE GATE FIRES

**Goal.** Every gate, every fixer, the full repair loop, full parity.

**Depends on:** M6. **Parallel with:** nothing. **Unblocks:** M8 (and, with owner sign-off, M9).

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

**Parity gate — THE full gate.** Every metric, every model, **no waivers** except entries
the owner has signed into the gate table. This is the milestone the whole plan exists to
reach.
**Owner eyeball — the owner pass.** Hand over: Pareto sheets; scar heat maps; per-model
before/after for (support count, resin, z-height); and every emitted file loaded in
ChituBox. **The owner's sign-off, recorded in the gate table, is the precondition for M9.**
Nothing gets deleted on a green `score` alone.

---

## M8 — Search quality + calibration + determinism CI

**Goal.** The migration is now allowed to be *better*, not merely equal — and determinism is
proven continuously rather than assumed.

**Depends on:** M7 (final gate) and M3 (proxy ladder). Calibration plumbing may start after
M3 + M4. **Unblocks:** M9.

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
| Exact | top 3 | full mesh | Pareto front recorded |

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
- Corpus at **1, 4, 16 threads** → identical artifact digests, driven by the single
  workspace-wide `PLINTH_THREADS` (06 §3). Do **not** sweep `GEOM_KERNEL_THREADS`: it sets
  only `geom_kernel`'s pool, leaving `plinth-support`, `plinth-orient` and `plinth-cost` —
  the crates whose determinism is actually at issue here — at their defaults, so the sweep
  would pass vacuously for everything that matters.
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

**Goal.** Zero Python.

**Preconditions, both required:** M7's gate passed **with owner sign-off recorded in the
gate table**, and M8's determinism CI is green. Neither alone is sufficient.

**Depends on:** M8. **Parallel with:** nothing.

### Delete

| Path | Note |
| --- | --- |
| `scripts/support_placement/` | The entire package, all 35 `.py` files including `__init__.py`, `geometry_exact.py` and `geometry_kernel.py` |
| `scripts/tools/` | All 19 `.py` files, plus `host.blend` |
| `scripts/tests/` | All `test_*.py`, all `manual_*.py`, `conftest.py`, `_refreeze_v5.py`, `_perf_fingerprint.py`. **Keep `scripts/tests/fixtures/chitubox/`** — move it to `corpus/fixtures/chitubox/` and repoint `rust/chitubox_io/tests/common/mod.rs::corpus_path()`. Those are the native ChituBox ground-truth specimens; they are not test infrastructure, they are irreplaceable data. |
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

This is the **one sanctioned write outside the repository**, and it is named as such in
`06-conventions.md` §7. It reads nothing back into the working tree and the clone is
deleted after the run. Do not generalize it into a habit of working in `$TMPDIR`.

**Parity gate:** the full M7 gate **re-fires post-deletion, from a clean checkout**. If
removing PyO3 changed a number, you find out here.
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
| 52 | `scripts/support_placement/__init__.py` | **M9** | Deleted with the package |
| 53 | `scripts/support_placement/geometry_exact.py` | **M9** | Already ported (`geom_kernel`); its differential-oracle role transfers to `corpus/baseline` + `plinth-metrics` |
| 54 | `scripts/support_placement/geometry_kernel.py` | **M9** | Deleted — PyO3 wrapper; native callers use `geom_kernel` directly |
| 55 | `rust/chitubox_io/python/chitubox_io/__init__.py` | **M9** | Deleted with the PyO3 surface |
| 56 | `rust/geom_kernel/python/geom_kernel/__init__.py` | **M9** | Deleted with the PyO3 surface |

**Test infrastructure** (`scripts/tests/*.py` — all `test_*.py`, all `manual_*.py`,
`conftest.py`, `_refreeze_v5.py`, `_perf_fingerprint.py`) is exempt from the one-milestone
rule: it keeps the frozen Python runnable until M9 and then deletes with it. Its **pinned
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
| `docs/specs/SPEC-RUST-KERNEL.md` | Binding 2026-07-23 ruling that the kernel goes to Rust. Scope: a **PyO3 hybrid** — a Rust kernel under a Python orchestrator. States the Python `ExactSurface` stays **"permanently"** as the differential-test oracle, and lays out Stages 2–5, none of which started. | **Superseded on scope, adopted on substance.** A reader coming to it cold would reasonably conclude the Python orchestrator stays forever; it does not — M9 deletes it, including `geometry_exact.py`. The "permanent" differential oracle is **replaced, not abandoned**: `corpus/baseline/` (frozen artifacts) plus `plinth-metrics` (an independent second implementation) preserve the same property without an interpreter. Its Stage 2–5 roadmap is the direct ancestor of M4–M8 and its numerical hazard notes — argmin tie-breaking is unspecifiable in the Python oracle, so pick a deterministic rule (lowest face index) and accept **consequence-based** parity; pin the pyo3/rust-numpy version pair first; never hold a numpy borrow across `allow_threads` — remain correct and worth reading. |
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
