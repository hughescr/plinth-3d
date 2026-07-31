# plinth-3d Rust Migration Guide

You are the engineer executing this migration. This directory is the authoritative
plan. Read it before touching code. Where it conflicts with any older document in
this repo, **this guide wins** — see [What this guide supersedes](#what-this-guide-supersedes).

---

## 1. Mission

**Eliminate all Python from plinth-3d and re-implement it in Rust.**

This is not primarily a performance decision. The owner's rationale, near-verbatim:

> eliminate all python because it cripples Claude's ability to think well... every
> exposure to python actively destroys productivity.

The reasoning behind that: the public Python corpus is dominated by unprofiled
notebook and glue code, so any coding agent trained on it drifts toward scalar
loops and unbatched hot paths. Removing Python from this repo is a **guardrail on
all future agent work here**, not a micro-optimization. Speed is a welcome side
effect (the Python pipeline runs 86–872 s per model, median ~402 s), but it is not
the acceptance criterion.

Consequences of the mission, all binding:

| Rule | Detail |
|---|---|
| No new Python | Not in `scripts/`, not for scratch, not for debug one-offs. See 06-conventions.md. **One named exception:** the two one-off `python -c` invocations in 04-migration-plan.md M0 (the `resolve_stl()` probe and the synthetic-fixture STL export), run once from a shell and never committed as a `.py` file. |
| Pure Rust expected | TypeScript/bun is permitted **only** with a concrete blocker named in writing (see §6). |
| One named exception exists | The C++ `manifold` boolean kernel behind FFI in `plinth-solid`. Justified in 02-target-architecture.md; no pure-Rust f64 manifold-grade boolean kernel is mature enough, and this project has been burned four times by coincident-coplanar-face defects. |
| PyO3 goes away | `rust/chitubox_io/src/py.rs`, the **inline** PyO3 surface in `rust/geom_kernel/src/lib.rs` (geom_kernel has no `py.rs`), and the `rust/*/python/` shim dirs are deleted once nothing consumes them — in the final milestone, not before. |
| Blender is retired | Its scripting surface is Python and it segfaults headless in sandboxes. The decimation logic it hosts is converted to native Rust, not re-invented. |

---

## 2. What plinth-3d is for

**End-state CLI (the someday goal, not this migration's scope):**

```
plinth --input-models-dir DIR --output PRINTER
```

Process every model in a folder: prep, smart-decimate, auto-rotate, hollow with
drain holes where appropriate, auto-support, lay out plates sized to the printer,
dynamic-layer-height slice, and send to the printer.

**Scope of THIS migration — only what exists today:**

1. Ingest models from an input directory.
2. Prep, smart-decimation, auto-orientation, hollow/drill, auto-supports, and emit
   one `.chitubox` file per model with supports and rafts.

Plates, slicing, and sending to the printer are **roadmap**. Do not build them. But
the architecture must let them bolt on without restructuring — that is why the data
model carries a per-layer `LayerPlan` and a Pareto front of orientations from day
one even though nothing consumes them yet (02-target-architecture.md).

### The optimization triad

Everything the pipeline does serves three coupled goals:

- **fast** — minimize z-height via orientation; later, variable layer heights (thicker
  layers where there is less z-axis detail).
- **minimum resin** — good orientation enables good hollowing and fewer supports.
- **high quality** — orientation enables dynamic layer thickness; support placement
  avoids scarring delicate regions.

**Every stage couples through orientation.** That is the central architectural fact
of this project. Cross-stage optimization is a first-class design concern, not an
afterthought — see 02-target-architecture.md for the three-tier orientation funnel,
the orientation-invariant `FaceTable`, and the surrogate-scoring design.

---

## 3. Acceptance strategy (read this twice)

The frozen Python engine stays runnable as the **measuring stick** until the Rust
core beats it. The migration is therefore **additive** until the final milestone,
which is the only milestone that deletes Python.

**The acceptance criterion is NOT byte-identical output.** The owner's words: the
criterion is *"rust is producing as-good-as or better output in chitubox files"*,
and "better" has a subjective element. Concretely:

1. **Freeze once.** Run the Python pipeline over the corpus and commit its outputs
   as BASELINE artifacts (`corpus/baseline/`). Never regenerate them casually.
2. **Score both sides with the same metrics.** `plinth-metrics` reads emitted files
   from disk and computes: island count, closed cavities, watertightness / boundary
   edges, X-rule load compliance, support count and resin volume, scar area and
   placement, ChituBox loadability.
3. **Gate:** Rust must be **as-good-or-better on every metric**, per model.
4. **Plus a named human checkpoint** at each milestone listing exactly what the
   owner should eyeball, for the subjective part of "better".

Hard rules around the harness:

- **Never bridge via PyO3 for comparison.** Both sides emit files; the comparison is
  file-to-file. A bridge would let a shared kernel bug hide in both.
- **Gates measure the file AS WRITTEN TO DISK, with no repair pass.** This project
  once validated an in-memory mesh while shipping a different one. The audit re-reads
  the artifact from disk and fails on boundary edges, negative shells, or inconsistent
  winding. Repairing the artifact to pass the audit is forbidden — the only correct
  fix is fixing the producer.
- **`plinth-metrics` is independent.** It links neither `plinth-verify` nor
  `geom_kernel`, so a kernel bug cannot corrupt producer and judge together. Slow
  brute-force f64 primitives are fine there. The one exception: it may link the
  `chitubox_io` **reader**, anchored by the native ChituBox fixtures (05-chitubox-and-corpus.md).

**Byte-level identity applies in exactly one place:** the `.chitubox` record
structure, which must round-trip and load in ChituBox. The existing corpus
round-trip tests are the gate for that. Python digests and mesh fingerprints are
explicitly *not* byte-compatible — they are redefined (schema bump, `-0.0 → 0.0`
canonicalization, topology-arrays-only digest) because the Python versions bake
NumPy allocator arithmetic and raw-f64-bit welding into wire hashes.

**Three authorities must not be collapsed.**
`03-physics-and-constants.md` is the normative authority for a runtime constant's
value, shape, unit and provenance grade. `m0-constants-source-ledger.json` is the
hand-maintained authority for whether a legacy literal is in the future runtime
registry at all, and for its source/provenance disposition. The future
`constants.md` is generated from the runtime registry and is only a checked view
of code. The source ledger is never generated and is never input to that golden
test.

The frozen model baseline has exactly **eight** members: `Sphere`, `Cylinder`,
`make_box`, `make_overhang_table`, `make_toed_slab`, `make_raking_blade`,
`make_thin_fin`, and `Weeping Treant v1.2` (the same model as
`Awakened Tree.stl`). The sanctioned provisional baseline has the same list minus
the Treant, exactly **seven** members. There is no `Cone`. Source STLs under
`corpus/models/` are ignored, local inputs; `corpus/manifest.json` and frozen
`corpus/baseline/` artifacts are committed.

---

## 4. Reading order

| # | Doc | What it gives you |
|---|---|---|
| 0 | **README.md** (this file) | Mission, scope, acceptance strategy, how to work. |
| 1 | **01-current-system.md** | What exists in Python today, module by module, plus the known defects the port must **fix, not reproduce**. |
| 2 | **02-target-architecture.md** | The Rust crate graph, the typestate stage spine, `DefectReport`, `ViolationKind`, the orientation funnel, `FaceTable`, `CostVector`, `LayerPlan`, and the `plinth-solid` FFI decision. |
| 3 | **03-physics-and-constants.md** | The numeric authority: the measured physics (X-rule, scar invariance, hex lattice, release-front kinematics), every runtime constant's value/shape/unit/grade, and the **plate-6 results (scored 2026-07-31)**: `X*` reproduced on cylindrical arms, multi-arm load sharing measured as **at least additive**, and the coupon-design rules that came out of the voided A6 family. Its stable locators are referenced by the separate M0 source ledger. |
| 4 | **04-migration-plan.md** | Milestones M0…M9 in dependency order, with the gate that must pass before each one is called done; M9 is the Python-deletion milestone. |
| 5 | **05-chitubox-and-corpus.md** | The `.chitubox` format-fixture corpus and which fixtures are native ground truth vs. regenerated (and therefore circular for writer bugs), the distinct eight-model baseline corpus, and the baseline freeze procedure. |
| 6 | **06-conventions.md** | Standing rules: batch-first, f64, rayon, errors, typed-provenance constants, testing standards, git discipline, no-new-Python, no stray artifacts. |

Read 06-conventions.md before writing your first line of code. Read
03-physics-and-constants.md before writing anything that computes a support radius.
Read `m0-constants-source-ledger.json` before porting a literal: presence in old
source does not imply membership in the future registry.

**Release-front vocabulary is exact.** For the Saturn profile,
`leading_release_edge = NegativeY` and
`release_front_sweep_direction = PositiveY`: the release front moves from `-Y`
to `+Y`. Never shorten that to a “-Y sweep” or to generic “peel direction.” The
axis input remains plumbed through load sharing but has no measured effect in the
current formula.

**Conflict rule for fixture provenance.** Where two guide docs disagree about whether a
`.chitubox` fixture is native ChituBox-authored ground truth or was regenerated by this
library, [../../scripts/tests/fixtures/chitubox/README.md](../../scripts/tests/fixtures/chitubox/README.md)
is authoritative. Follow it and report the conflict.

---

## 5. How to work

**Commit per milestone, on `master`.** The repo is local-only git. Never
force-push. The destructive commands listed in 06-conventions.md (`git checkout` with
a double-dash, `git restore`, `git reset --hard`, `git clean`) are forbidden outright —
they silently destroy uncommitted work. Undo your own edits by editing.

**Run the quality gate before moving on.** A milestone is not done because the code
compiles. It is done when:

1. `cargo test --workspace` passes and `cargo clippy --workspace -- -D warnings` is clean.
2. The milestone's gate in 04-migration-plan.md passes — for any milestone that emits
   artifacts, that means `plinth score` over the corpus shows Rust as-good-or-better
   than the frozen baseline on every metric.
3. The milestone's human checkpoint is written into the commit message so the owner
   knows what to eyeball.
4. The cache-disabled run produces identical reports (see 06-conventions.md).

**Do not delete any Python until the final milestone**, and then only after the
parity gates pass. The deletion set is: `scripts/` Python, `rust/chitubox_io/src/py.rs`,
the **inline** PyO3 surface in `rust/geom_kernel/src/lib.rs` (the `#[pymodule]` for
`geom_kernel._native` plus every `#[pyfunction]`/`#[pyclass]` item — geom_kernel has no
`py.rs`), the `rust/*/python/` shim dirs and their `pyproject.toml` files, the top-level
`pyproject.toml`, `uv.lock`, and `.venv/`.

**If you hit ambiguity, stop and ask** rather than guessing. A wrong guess encoded
into the physics or the acceptance path costs more than a round-trip.

### The TypeScript/bun escape hatch

The expectation is pure Rust. If you believe a task genuinely cannot be done in
Rust, you must:

1. Name the concrete blocker — the specific capability, the crates you evaluated,
   and why each fails.
2. Write it into a new section at the bottom of **06-conventions.md** titled
   `## Escape-hatch justifications`, dated, with the blocker and the bounded scope
   of the exception.
3. Get owner sign-off before writing the non-Rust code.

Precedent for what a valid blocker looks like: the C++ `manifold` FFI in
`plinth-solid`, documented in 02-target-architecture.md. Note its bounds — it sits
behind a `trait SolidBoolean` so a native kernel can replace it, and the
read-back-from-disk audit gates its output regardless of backend. An escape hatch
without those bounds is not acceptable. Python is **never** an escape hatch.

---

## 6. What this guide supersedes

These repo documents predate this guide. Read them for depth on the subsystems they
describe, but where they conflict with this guide, **this guide is correct**:

| Document | Superseded claim |
|---|---|
| [../specs/SPEC-RUST-KERNEL.md](../specs/SPEC-RUST-KERNEL.md) §6 | "the Python `ExactSurface` stays permanently" — **overridden.** Total Python elimination is the mission. The differential-oracle role transfers to the frozen baseline artifacts plus `plinth-metrics`' independent implementation. |
| [../reference/port_plan.md](../reference/port_plan.md) | Its "stay Python" verdicts and its milestone numbering. Superseded by the owner's 2026-07-23 ruling and by 04-migration-plan.md. |
| [../reference/port_plan_codex_review.md](../reference/port_plan_codex_review.md) | Still useful — its finding that wall-clock caps make baselines non-reproducible is **binding on all new code**. Its plan-level recommendations are superseded. |
| Python CLI contract (`scripts/tools/prep_one.py` exit codes, JSONL stdout) | Retired. The new contract is one `DefectReport` JSON per model with the exit code derived from it (06-conventions.md). The Python tests that pin the old contract retire with their subject. |
| Any wall-clock budget (e.g. `rewire_time_cap_s`) | Banned outright. Replaced by deterministic work-unit budgets, with no legacy wall-clock mode retained. |

Supplemental depth, none of it prerequisite reading:
[../reference/physics_load_sizing.md](../reference/physics_load_sizing.md),
[../reference/physics_coverage.md](../reference/physics_coverage.md),
[../reference/physics_printability.md](../reference/physics_printability.md),
[../reference/physics_transient_peel.md](../reference/physics_transient_peel.md),
[../reference/physics_contacts_tips.md](../reference/physics_contacts_tips.md),
[../specs/SPEC-CHITUBOX-RS.md](../specs/SPEC-CHITUBOX-RS.md),
[../specs/CHITUBOX-FORMAT.md](../specs/CHITUBOX-FORMAT.md),
[../datasheets/README.md](../datasheets/README.md),
[../../scripts/tests/fixtures/chitubox/README.md](../../scripts/tests/fixtures/chitubox/README.md).

**The port does not start from zero.** `rust/chitubox_io` is a working `.chitubox`
reader/writer and `rust/geom_kernel` provides BVH and geometry queries with parity
tests. Both stay; only their PyO3 surfaces go.
