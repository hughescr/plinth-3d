# plinth-3d

An end-to-end resin-print preparation pipeline, in Rust: raw mini STLs go in, and supports
come out that are sized from measured failure physics and then machine-verified rather than
eyeballed. Today the output is a print-ready `.chitubox` project per model; the destination
is a sliced job delivered to the printer over the network, with `.chitubox` kept as the
output you reach for when you want to look before you print.

## The goal

Point it at a folder of mini STLs. Get sliced files on the printer, over the network,
without a slicer GUI anywhere in the loop.

```sh
plinth prepare <folder> --output <printer>     # illustrative, not a settled spelling
```

End to end, that means: **STLs in → physics-verified supports → several models packed onto
one build plate by that printer's plate geometry → sliced natively into whatever the
printer eats (`.goo`, or whatever the machine takes) → sent to the printer over the
network.**

Today the pipeline stops at a `.chitubox` project per model, which you open and slice by
hand. **That is a feature as well as a stepping stone.** Right now it is the visual
inspection checkpoint — how you see the generated model and its supports and confirm the
algorithm got it right. Long term it stays a first-class output for anyone who wants a
manual review, or a manual confirmation, before slicing and printing. What the stages below
remove is the *requirement* to go through ChituBox on the automated path, not the ability
to.

### The ladder

Four stages, each independently shippable and useful on its own. Everything past stage 0 is
roadmap.

| stage | what | state |
| --- | --- | --- |
| **0** | Working support generation: one `.chitubox` per model, opened and sliced by hand — and inspected, which is the point of this stage. | **In progress** — the physics and the support engine work (see [What works today](#what-works-today)); the folder-level runner that chains the stages to a written artifact is not wired yet. |
| **1** | Multiple models per build plate, packed against the printer's real build-plate geometry. | Roadmap |
| **2** | Native slicing, so the automated path no longer depends on ChituBox. `.chitubox` output remains supported for manual inspection and confirmation. | Roadmap |
| **3** | Send the sliced job to the printer over the network. | Roadmap |

**Someday/maybe, not planned stages:** other slicer project formats such as Lychee — gated
on whether the format can keep the model separable from its supports, which is the property
inspection and re-editing depend on — and a plain STL export.

Stage 0's remaining work is the runner: `plinth-pipeline` currently defines the
folder-in/one-artifact-per-model data boundary and its `prepare(PrepareRequest) ->
Result<PrepareReport, PipelineError>` signature, but carries no loader, cache, stage
runner, or emit loop. That is the top item in
[`docs/rust-migration/CURRENT-STATUS.md`](docs/rust-migration/CURRENT-STATUS.md).

**Stage 3 rests on four assumptions that are genuinely open** — none of them is designed,
and naming them is not the same as having answered them:

- how a printer *name* maps to a printer *type*;
- what network protocol the printer speaks;
- how the printer's address is discovered or configured;
- which file formats a given printer actually accepts.

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

  > **Corrected 2026-08-06 — dated addendum; the bullet above is left standing as written.**
  > **"It now decimates" is an overstatement.** The eighth model runs the decimation *search*
  > and gets the original mesh back: every reduced candidate fails the topology gate
  > (`nonmanifold_edges` goes 0 → non-zero at every attempted ratio), so decimation is a
  > **no-op** on this model and its 429,120 triangles are the raw STL count, not a reduced
  > one. **The stopping point has also moved.** The kNN cap named above was raised on
  > measurement and is cleared; as of 2026-08-06 the model clears perception, the
  > candidate-surface grid, the specialized gate's proposal-free batch and the entire load
  > stage in 81.5 s at 2.84 GB peak RSS, and is halted further downstream — at the *merge*
  > gate, which re-evaluates every contact against every coverage sample once per merge
  > proposal and so states a demand of 21,755,124,103,146 operations against a measured
  > bound of 35,445,500,736. That is tracked as FU-8c, and it is an algorithm defect, not a
  > cap to raise. The rest of the bullet stands: this is still the largest known blind spot
  > in the engine, and until it lands every physics conclusion rests on procedural fixtures
  > plus two primitives. See `docs/rust-migration/CURRENT-STATUS.md` (*FU-8b resolved; the
  > treant's stopping point moved*) and
  > `docs/rust-migration/decimation-design-2026-08-06.md`.

  > **FU-8c fixed 2026-08-06 — dated addendum; the two blocks above are both left standing
  > as written.** The mechanism the addendum above describes — "re-evaluates every contact
  > against every coverage sample once per merge proposal" — **is gone**. The merge gate now
  > builds its proposal-invariant state once per batch (`GateBatchIndex`: samples by route
  > key, contacts by route key, three satisfying contacts per demand) and each proposal
  > re-answers only the route keys its own merged pair serves, so the plan is repriced from
  > `(samples + demands + 1) × (proposals × (contacts − 1) + contacts)` to
  > `(samples + demands + 1) × contacts + measured proposal work`. Equivalence was proved
  > before the hot path was swapped: the predecessor's per-variant sweep is retained as a
  > `#[cfg(test)]` reference and compared row by row on `f64::to_bits`. **No cap was raised,
  > lowered or otherwise touched.** The treant's stated demand fell **108×**, from
  > 21,755,124,103,146 to **201,480,535,329**, against the **same unchanged** 35,445,500,736.
  > **The stopping point did not move.** The treant **still refuses at that same merge
  > gate**, now 5.68× over rather than 613× over, and there is still no treant digest, no
  > routed-contact count and no live-member count — the merge tournament, the PENETRATION
  > and BRACE stages, the CCO anchors, attestation and the seal never execute. The run is
  > **124 s at 2.50 GB peak RSS**, superseding the 81.5 s / 2.84 GB above as the current
  > measurement (that figure stands as measured pre-fix; it is not being corrected). FU-8c
  > is **closed** on the defect it named, not on the treant reaching a seal; the residual
  > 196,264,386,588 — **97.4 %** of the demand — is per-proposal `sources × samples` work on
  > the perturbed route key, and is opened as **FU-8d**. It is still an algorithm defect and
  > still not a cap to raise, and the eighth model is still the largest known blind spot in
  > the engine. See `docs/rust-migration/CURRENT-STATUS.md` (*FU-8c fixed 2026-08-06*, and
  > the FU-8c/FU-8d follow-up rows) and
  > `docs/rust-migration/support-resource-caps-2026-08-05.md` §7.

  > **FU-8d phase F1 landed 2026-08-06 — dated addendum; everything above stands, and the
  > numbers in it are the reason this note is short.** F1 is the first phase of FU-8d's
  > part (1) and it deliberately changes **no answer and no charge**: the merge gate's
  > baseline coverage sweep now retains the three nearest sources per coverage sample, which
  > the per-proposal delta will read in phase F2, and nothing reads them yet. The treant's
  > demand is **still 201,480,535,329** against the **still unchanged** 35,445,500,736, at
  > the same gate, 5.68× over; all seven frozen corpus digests are byte-identical. **No cap
  > was raised or touched.** One `KnnBatchLimits` shape field moved — the coverage-route
  > batch's neighbour ceiling, 1 → 3 — and it is explicitly **not** a cap: the charged work
  > `source_count × query_count` is k-independent, and the bytes the raise admits are
  > charged per route and per batch. See `docs/rust-migration/CURRENT-STATUS.md` (*FU-8d
  > phase F1 landed 2026-08-06*) and `support-resource-caps-2026-08-05.md` §8.

  > **FU-8d phase F2 landed 2026-08-06 — dated addendum; every block above stands as
  > written, and the treant's stopping point has moved for the first time since FU-8b.**
  > F2 finishes FU-8d part (1). A merge proposal no longer re-sweeps the route keys its
  > pair perturbs against those keys' whole surviving source sets: the survivors' answer is
  > **read** from the witnesses F1 retained, and the merged seat's distances come from
  > **one** kNN batch covering every perturbed row at once, with the square root taken once
  > after the minimum. Equivalence was proved before the swap, against the same retained
  > `#[cfg(test)]` full sweep, on a fixture grown until a narrow key, a total removal, a
  > displaced baseline holder, a **steal** and an **exact distance tie** each occur.
  > **No cap was raised, lowered or otherwise touched.** The per-proposal nearest term fell
  > **94.2×**, from 196,264,386,588 to 2,083,167,818, and the merge gate's stated demand
  > fell **27.6×**, from 201,480,535,329 to **7,299,316,559** against the **same unchanged**
  > 35,445,500,736 — 20.6 % of it, so that check now **passes**. **The stopping point moved
  > one site.** The treant still refuses at the same merge gate, but now inside the
  > `exact_operations` fold, where the per-variant owner-face descriptor scan crosses the
  > limit at **variant 275 of 39,224**: `required: 35445618202, limit: 35445500736`. That
  > residual is **FU-8d part (2)**, the per-variant load/owner-face path — over those 275
  > variants alone the exact finalizer term is already 3.04 × 10¹², which extrapolates to
  > ~4.3 × 10¹⁴ over the full batch — and it is what phase F4 addresses. There is still no
  > treant digest, no routed-contact count and no live-member count; the run is **129 s at
  > 2,641,504 KB (2.52 GiB) peak RSS**, watchdog armed and never fired, superseding the
  > 124 s / 2.50 GB above. All seven frozen corpus digests are byte-identical. FU-8d stays
  > **open** in parts (2) and (3), it is still an algorithm defect and still not a cap to
  > raise, and the eighth model is still the largest known blind spot in the engine. See
  > `docs/rust-migration/CURRENT-STATUS.md` (*FU-8d phase F2 landed 2026-08-06*) and
  > `support-resource-caps-2026-08-05.md` §9.

  > **Amended 2026-08-06 — FU-8d phase F3 landed; the blocks above are left standing exactly
  > as measured.** *The treant's merge gate no longer refuses.* The load partition is
  > finalized **once** per gate batch instead of once per variant — a proposal re-answers
  > only the rows its own merged pair serves, from three retained witnesses plus one
  > distance, and the merged seat serves *exactly* that union by construction — and with it
  > go the per-variant window matrices, a tens-of-gigabyte byte wall that had never fired
  > only because the operations check refused first. The four per-proposal planning passes
  > become one behind a pre-check stated from the batch header, and the per-variant contact
  > and demand rescans are derived rather than repeated. The gate's hand-built
  > `variants × faces` model of the load phase is replaced by what the load planner itself
  > reports: the honest figure is 6,592,813,742 where the model said 4,618,272,984, so the
  > **stated demand rose** to **9,273,857,317** — and that is **26.2 %** of the **same
  > unchanged** 35,445,500,736, so the batch is admitted and **executes**. What collapsed is
  > what actually refused: the per-variant owner-face descriptor scan, 35,445,417,382 at
  > variant 275 → **136,791,451** charged once, and the ~4.3 × 10¹⁴ finalizer term to nothing
  > at all. **No cap was raised, lowered or otherwise touched.** Equivalence was proved
  > before the swap, against the whole production finalizer run on hand-renumbered variant
  > inputs, row by row and on carried area by `f64::to_bits`, over a fixture that asserts it
  > reached an exact survivor tie, an exact merged-seat tie, a row with no survivor, a
  > displaced holder and a steal. The treant now evaluates 39,224 variants, accepts a merge,
  > opens the next batch, and planned **eight batches inside the 600 s timebox** without
  > finishing the tournament — an **EXCEEDS-TIMEBOX with counters**, not a refusal, at
  > ~2.4–2.5 GB peak RSS. There is still no treant digest. The successor defect is the
  > per-variant `assess_coverage` — 1.59 × 10¹⁰ uncharged sample visits per batch — opened as
  > **FU-8e**; FU-8d is **resolved** on the defect it named. All seven frozen corpus digests
  > are byte-identical. See `docs/rust-migration/CURRENT-STATUS.md` (*FU-8d phase F3 landed
  > 2026-08-06*) and `support-resource-caps-2026-08-05.md` §9.

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

**The model and its supports stay separable all the way to the emit boundary.** The support
scene carries semantic support kinds and f64 geometry; `plinth-emit` alone maps them onto
ChituBox record identities and f32, attaching supports as per-model records rather than
welding them into one mesh. That separability is what makes the `.chitubox` inspection step
meaningful — you can look at the supports as supports, and edit them — and it is the
property any future output format has to preserve to be worth supporting.

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

**The CLI surface as it stands.** `crates/plinth-cli` is a typed parser, and the spelling it
accepts today is:

```sh
plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--no-cache]
```

Parsing deliberately stops before execution policy is assembled, and nothing runs behind it
yet — so this is the current shape of the flags, not a working command. The flags are not
settled; they will follow the ladder above rather than constrain it.

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
| [`docs/rust-migration/decimation-design-2026-08-06.md`](docs/rust-migration/decimation-design-2026-08-06.md) | The decimation measurement record. Why the production mini does **not** decimate today (it is the topology gate, not the utility gate), what its ratified error budget actually admits, and the design for fixing it. Evidence behind the 2026-08-06 ruling that demoted decimation. |
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
