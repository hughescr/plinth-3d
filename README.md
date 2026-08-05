# plinth-3d

An end-to-end resin-print preparation pipeline, in Rust: a raw mini STL goes in, a
print-ready `.chitubox` project comes out, with supports that are sized from measured
failure physics and then machine-verified rather than eyeballed.

## The goal

One command, for a folder of D&D minis, on a home resin printer:

```sh
plinth prepare <folder> --output <printer>
```

**This is the destination, not the current state.** The typed CLI surface exists today
(`crates/plinth-cli`), and its stable spelling is:

```sh
plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--no-cache]
```

What does not exist yet is the runner behind it: `plinth-pipeline` currently defines the
folder-in/one-artifact-per-model data boundary and its `prepare(PrepareRequest) ->
Result<PrepareReport, PipelineError>` signature, but carries no loader, cache, stage
runner, or emit loop. Wiring that chain is the top roadmap item — see
[`docs/rust-migration/CURRENT-STATUS.md`](docs/rust-migration/CURRENT-STATUS.md).

## Why it exists

Commercial slicers place supports by heuristic: a palette of tip sizes, a density slider,
angle rules. plinth measures instead.

**The area-ratio law.** Support-arm failure on this printer is governed by a ratio, not an
absolute force:

```
X = supported_area / arm_cross_section
```

The failure boundary was measured on printed calibration plates — conical tips bracket
`X* ∈ [25.00, 25.34]`, and the part breaks at the narrowest section just above the contact
(see [`docs/reference/plate-05-results.md`](docs/reference/plate-05-results.md)). The
engineering allowance the engine actually sizes to is

```
X_ALLOWED = floor(lower(X_STAR_CONE) / 2) = floor(25.00 / 2) = 12
```

so every arm satisfies `A_arm ≥ A_supported / X_ALLOWED`. Plate 6's independent cylindrical
bracket leaves that allowance `19.91 / 12 = 1.66×` of margin — a cross-check, deliberately
not a second derivation input. The engine **never computes a force**; it uses the ratio.

**Every constant carries its provenance.** The registry in `plinth-core` types each value
as `measured` (a named plate, record, locator and date), `derived` (with its chain of
inputs and the exact relation), or `placeholder` (explicitly non-accept-safe). As of the
contact-physics package that is 212 constants, of which 192 are still placeholders — and
saying so is the point: an unmeasured number is visibly unmeasured, and a placeholder can
never be quoted as a safety claim. Full table:
[`docs/rust-migration/constants.md`](docs/rust-migration/constants.md).

**Promotion is disciplined.** A tested calibration cell does not become a `measured`
engine constant off one plate. The home-calibration protocol's promotion gate requires
**three independently prepared plates** before an exact tested cell is promoted
([`docs/reference/home-support-calibration-p1-protocol.md`](docs/reference/home-support-calibration-p1-protocol.md)).
Duplicate thresholds are treated as a divergence hazard and deleted rather than kept in
sync by hand.

## What works today

Measured on the 8-model corpus, most recently by the unified census of 2026-08-05:

- **7 of 8 corpus models complete a sealed support run end to end**, each on its own pinned
  orientation — decimate, orient, hollow decision, perception/slicing, routing, CCO,
  bracing, contact verification, seal.
- **Contact attestation is live for all three families that mint a model contact** —
  initial-contact stubs (every routed support tip), brace model rungs, and CCO model
  anchors. It runs on real corpus geometry, not only on fixtures: 286 stub-terminal
  attestations, 2 brace model-rung seats, 3186 CCO model-anchor seats in that census.
  Every tip must be *proven* attached across its required contact area; a tip that engages
  nothing is quarantined, not shipped.
- **Attachment to buried surface is refused.** A support foot seated on a surface patch
  that lies *inside* the printed solid passes naive verification trivially — its whole
  neighbourhood is interior. An explicit exterior-ness screen catches that case, and it has
  fired on real corpus geometry, not just on the synthetic self-interpenetrating fixture
  built to test it. Refusals route to quarantine; they never hard-fail the pipeline.
- **The eighth model is the only real production mini in the corpus** — a 429,120-triangle
  tree. It now decimates, orients and gets through the slice seam that used to stop it, and
  is halted by a *resource cap*, not a geometry defect: the kNN stage requires 298,667,520
  units of work against a 100,000,000 limit. Raising those caps on measurement rather than
  by guess is in flight (FU-8), and it is the largest known blind spot in the engine —
  until it lands, every physics conclusion rests on procedural fixtures plus two
  primitives.

The stages exist as libraries and the census harness runs the real chain from STL through
a sealed support run. The folder-level runner that chains them all the way to a written
`.chitubox` is roadmap, per [The goal](#the-goal) above.

## Pipeline architecture

```
STL ─▶ decimate ─▶ orient ─▶ hollow ─▶ perception / slicing ─▶ support generation ─▶ verify ─▶ emit ─▶ .chitubox
                                                                (routing, CCO,
                                                                 bracing, contact
                                                                 physics)
```

Decimation runs first by design: its error-bounded, feature-protecting output is the *sole*
production mesh every later stage may consume, so no stage is ever reasoning about a
different mesh than the one that will print.

The Cargo workspace (`crates/*`, `rust/chitubox_io`, `rust/geom_kernel`):

| crate | role |
| --- | --- |
| `plinth-core` | The stable, dependency-light vocabulary: data contracts, the constants registry and its provenance types, `DefectReport` as the one acceptance authority. No geometry. |
| `plinth-mesh` | The f64 mesh substrate — topology, delicacy, STL I/O, triage. Meshes are kept exactly as their source supplied them: no implicit weld, reorder, repair, or f32 conversion. |
| `plinth-metrics` | Independent, deliberately brute-force geometry and topology audit. Has no dependency on `geom_kernel`, `plinth-mesh`, or `plinth-verify` — that is what makes it an auditor. |
| `plinth-decimate` | The mandatory production QEM decimation substrate: a narrow adapter over `meshopt`, with probing, protection halos and error-budget measurement around it. |
| `plinth-orient` | Pinned-baseline orientation. Applies the selected rigid rotation and recreates the frame; it deliberately does not search yet. |
| `plinth-hollow` | Shell creation, cavity field, drain and vent placement, drilling. |
| `plinth-solid` | Batch-shaped f64 boolean-solid operations over a vendored manifold bridge. |
| `plinth-support` | The engine: perception and slicing, contact physics, initial routing, CCO, bracing, rewiring, specialized contacts, member sizing, coverage, raft, and the run sealer. |
| `plinth-verify` | Disk-first `.chitubox` container verification. Judges only the bytes freshly read from a path — no producer state, no cache, no repair. |
| `plinth-emit` | The single final emission boundary. Converts f64 support geometry to the f32 wire records, writes one fresh project, then audits a *newly opened* copy of it. It never uses a model-bearing template. |
| `plinth-pipeline` | The folder-in, one-artifact-per-model data boundary and stage-stop types. Signature landed; the runner behind it is roadmap. |
| `plinth-cli` | Typed parser for the stable `plinth` command line. Parsing stops before execution policy is assembled. |
| `rust/geom_kernel` | Native, batch-first triangle-soup geometry kernel: BVH, closest point, ray and segment batches, slicing, parity grids. Every public query takes a slice and returns a slice. |
| `rust/chitubox_io` | Read and write ChituBox Basic 1.3.0 `.chitubox` projects starting from an *empty* project — no pre-saved template containing a model. Every write is verified by structural re-parse. |

Three conventions bind all of it, and each exists because a specific bug shipped:

- **Batch-first is law.** Gather → one batched call → scatter. A per-item call in a hot path
  is a defect, not a style choice.
- **f64 everywhere** in the geometry data path. `f32` appears only at the `.chitubox` format
  boundary, converted outward and never inward.
- **Determinism by construction.** Index-parallel map-collect only, never a float
  reduce/fold across parallel work items, one workspace-wide thread variable
  (`PLINTH_THREADS`), and a 1/N thread sweep on every parallel path.

The full set is [`docs/rust-migration/06-conventions.md`](docs/rust-migration/06-conventions.md).

## Development

**Branching.** Git flow. `develop` is the integration base; work lands on `feature/*`
branches cut from it. Releases are the maintainer's, from `develop` to `main`.

**The gate — five commands, all of them, every time.** A four-command gate is how four
formatting violations reached the integration branch; `--no-fail-fast` is how a single red
test stops being mistaken for a smaller or larger problem than it is.

```sh
cargo test  --workspace --offline --no-fail-fast
cargo test  --workspace --offline --no-fail-fast --all-features
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Most recently green at 940 passing (941 with `--all-features`), clippy, fmt and whitespace
clean.

**Corpus provisioning.** The eight test models are inputs, not source, and are gitignored
(`corpus/models/*.stl` — the tree alone is 21 MB). A fresh worktree therefore starts with an
empty `corpus/models/`; symlink them from the canonical checkout:

```sh
NEW_WT=/path/to/your/worktree
MAIN=/path/to/plinth-3d
mkdir -p "$NEW_WT/corpus/models"
ln -sf "$MAIN"/corpus/models/*.stl "$NEW_WT/corpus/models/"
```

Then re-verify against `corpus/manifest.json` with `shasum -a 256 -c`. **The symlinks are
matched by the same gitignore rule, so they are invisible to `git status`** — a worktree can
look perfectly clean and still be unable to run the gate. Two test targets hard-fail
without the corpus, with an error that does not name the corpus as the cause:
`plinth-decimate`'s `corpus_budgets` and the `plinth-orient` scale audit. If those two are
the only failures in a fresh checkout, provision before investigating anything else.

**Measurement runs.** Census instrumentation lives behind a non-default feature so it can
never reach a production build:

```sh
cargo build --offline --example embed_incidence_census -p plinth-support --features census --profile dev-opt
PLINTH_CENSUS=1 ./target/dev-opt/examples/embed_incidence_census
```

Without `PLINTH_CENSUS=1` the identical binary produces an un-instrumented run; the
instrumentation is verified result-inert against pre-instrumentation digests.

## Documentation

| where | what |
| --- | --- |
| [`docs/rust-migration/CURRENT-STATUS.md`](docs/rust-migration/CURRENT-STATUS.md) | **The living authority.** What is integrated, what is branch-only, what is blocked, and the ordered follow-up list. Read this first; where it disagrees with anything else, it wins. |
| [`docs/rust-migration/`](docs/rust-migration/) | The migration guide itself — mission, target architecture, physics and constants, plan, ChituBox and corpus, and `06-conventions.md`. |
| [`docs/rust-migration/contact-physics-2026-08-04.md`](docs/rust-migration/contact-physics-2026-08-04.md) | The normative account of the contact physics: seating depth, mask extent, emergence admissibility, area sufficiency, attestation, and the exterior screen — each with its derivation and its counter-example. |
| [`docs/rust-migration/census-embed-incidence-2026-08-04.md`](docs/rust-migration/census-embed-incidence-2026-08-04.md) | The corpus census. **Read the erratum at the top first** — the original body measures a code path that has since been deleted; the live numbers are in the 2026-08-05 section. |
| [`docs/rust-migration/constants.md`](docs/rust-migration/constants.md) | The constants registry with every value's provenance class. |
| [`docs/reference/`](docs/reference/README.md) | Our own durable findings, fully indexed — including the calibration-plate results the load constants come from and the [home-calibration protocol](docs/reference/home-support-calibration-p1-protocol.md). Not normative. |
| [`docs/specs/`](docs/specs/) | The ratified rules code cites as law. |
| [`docs/decisions/`](docs/decisions/) | Dated decision records, never edited after landing. |
| [`docs/datasheets/`](docs/datasheets/README.md) | Printer and resin data somebody else published. |

## Limitations, honestly

- **The calibration is printer- and resin-specific.** Every measured constant comes from
  plates printed on an Elegoo Saturn 4 Ultra 16K — a tilt-peel machine with front-to-back
  release — in specific resins. `saturn4-ultra` is the only output profile the CLI accepts.
  The area law was also measured on coupons whose arm *and* head were both support
  material; applying it to a support-to-model interface transfers it across boundaries the
  plates did not test, and the design record says so explicitly. Another printer or resin
  needs its own plates.
- **Self-intersecting input is mid-transition.** Real mini meshes are watertight and
  manifold yet self-interpenetrating — the corpus tree carries 23,220 non-adjacent
  intersecting triangle pairs inside a single 2.5 × 1.8 × 0.8 mm region of a topologically
  perfect mesh. The decision of record is per-layer polygon union under a fill rule, not a
  3D mesh boolean, and that work is not done. Until it lands the 3D mesh retains buried
  surface, and the exterior-ness screen described above is the affirmative defence rather
  than a structural guarantee. Survivable today; not finished.
- **Some geometry is pinned provisionally.** One corpus model's required-table freeze is
  explicitly provisional pending a probe-parity investigation, and the CCO model-anchor
  support family is arithmetically dead on the current corpus — 0 surviving anchors from
  3429 attempts. Reviving it is a print-consequence decision reserved to the maintainer,
  and until then a commit-time tripwire makes it impossible for that family to land
  silently.
- **Python is not gone yet.** The frozen Python engine under `scripts/` remains as the
  oracle that the Rust ports are checked against, and its skills and tooling are still in
  the tree. Standing policy is elimination, not coexistence: each Python surface is deleted
  as soon as its native Rust boundary lands, no optional compatibility tail is kept, and the
  final gate requires zero Python. No new Python is written here — see
  [`docs/decisions/2026-07-28-rust-rewrite.md`](docs/decisions/2026-07-28-rust-rewrite.md)
  for the reasoning.

## License

Licensed under the Apache License, Version 2.0. The full text is in
[LICENSE.md](LICENSE.md).
