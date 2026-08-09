# Architecture

How a model becomes a printable file, and which crate owns each part of it.

```
STL ─▶ decimate ─▶ orient ─▶ hollow ─▶ supports ─▶ lower ─▶ emit ─▶ .chitubox
                                          │
                        perception · specialized contacts · initial contacts
                        routing · rewire · brace · structure
```

Every stage hands the next one a typed value, and a stage that cannot do its job returns a
typed refusal rather than a degraded result. Refusals are carried forward and recorded; they
never turn into silence.

---

## The stages

### Decimate

Reduces the triangle count under an explicit, measured error budget, protecting the features
that matter. It runs **first** on purpose: its output is the *only* production mesh every
later stage may consume, so no stage is ever reasoning about a different mesh than the one
that will print.

Reduction is not guaranteed. Candidates that break mesh topology are rejected, and if no
candidate survives, the stage hands on the original mesh. That is a legitimate outcome, not
a failure — the guarantee is about what downstream sees, not about how small it is.

### Orient

Applies the chosen rigid rotation and rebuilds the build frame: the transformed bounding box
is centred in XY and rests at Z = 0. The rotated mesh replaces the production mesh outright;
no untransformed second copy survives the stage, so there is nothing for a later stage to
pick up by mistake.

Orientation is the coupling hub of the whole problem — it decides which surfaces face down,
and therefore where the unavoidable support scars land (see
[`physics.md`](physics.md)). The stage currently applies a selected orientation rather than
searching for one.

### Hollow

Creates the shell, computes the interior cavity field, places drains and vents, and drills
them. Hollowing is what turns a solid mini into a print that uses a fraction of the resin.

A shell the stage cannot validate is refused, and the model continues to support planning as
a **solid**. That path is honest rather than hidden: the refusal travels with the model and
is visible in the run's outcome. Shells enclosing several separate voids are the known case
that takes it.

### Supports

The engine. It runs as one sealed sequence whose authority is sealed together at the end,
and every leaf inside it is batch-shaped — gather, one batched call, scatter.

- **Perception** slices the model and reads the geometry: down-facing regions, islands,
  delicacy, the surfaces a support may legally touch.
- **Specialized contacts** plan the contacts that need geometric reasoning of their own,
  batching projections and gates rather than asking per candidate.
- **Initial contacts** construct the physical seats, in one axis-interval batch — every tip
  seated to a computed depth, masked for clearance, and checked for area sufficiency.
- **Routing** builds the support tree, merging contacts into shared arms where the load
  arithmetic allows it.
- **Rewire** re-attaches members that a better parent has become available for.
- **Brace** adds cross-links and rungs where a member would otherwise be too long to stand.
- **Structure** finalizes member sizing, coverage and the raft.

A support that cannot be proved attached to the model is quarantined, not shipped, and a
support seated on a surface that lies *inside* the printed solid is refused outright. The
physics behind each of those checks is in [`physics.md`](physics.md).

### Lower

Turns the final live forest into semantic emit primitives — frustums, spheres, raft pads —
in a fixed order. This pass is deliberately pure and serial: it performs no geometry queries
and makes no decisions. One structural member becomes several wire segments here, which is
why the emitted record count is larger than the member count.

### Emit

The single boundary where f64 geometry becomes the `.chitubox` file's f32 records. It writes
one fresh project — never a template that already contains a model — and then audits a
**newly opened** copy of what it just wrote.

The model and its supports stay separable all the way through: supports are attached as
per-model records carrying their own semantic kinds, not welded into one mesh. That is what
makes opening the file for inspection worth doing, and it is the property any future output
format has to preserve.

### Verify

Judges a `.chitubox` container from the bytes on disk and nothing else — no producer state,
no cache, no repair. A structural pass means the file parses, re-serializes and compares
equal; it is not a claim about how any particular GUI will render it.

---

## The workspace

| crate | role |
| --- | --- |
| `plinth-core` | The stable, dependency-light vocabulary: data contracts, the constants registry and its provenance types, and the single acceptance authority every stage reports through. No geometry. |
| `plinth-mesh` | The f64 mesh substrate — topology, delicacy, STL I/O, triage. Meshes are kept exactly as their source supplied them: no implicit weld, reorder, repair or f32 conversion. |
| `plinth-metrics` | Independent, deliberately brute-force geometry and topology audit. It depends on none of the producing crates — that is what makes it an auditor. |
| `plinth-decimate` | The mandatory production decimation substrate: a narrow adapter over a locked QEM simplifier, with probing, protection halos and error-budget measurement around it. |
| `plinth-orient` | Orientation: applies the rigid rotation and rebuilds the build frame. |
| `plinth-hollow` | Shell creation, cavity field, drain and vent placement, drilling. |
| `plinth-solid` | Batch-shaped f64 boolean-solid operations. |
| `plinth-layers` | Per-layer polygon union with fill-rule normalization, so every downstream consumer of a slice is correct whichever fill rule it evaluates. |
| `plinth-support` | The engine: perception, contact physics, routing, rewiring, bracing, member sizing, coverage, raft, lowering, and the run sealer. |
| `plinth-verify` | Disk-first `.chitubox` container verification. |
| `plinth-emit` | The single final emission boundary: f64 support geometry to f32 wire records, one fresh project, then an audit of a freshly opened copy. |
| `plinth-pipeline` | The folder-in, one-artifact-per-model data boundary and its stage-stop types. The signature is defined; the runner behind it is the next thing being built. |
| `plinth-cli` | Typed parser for the `plinth` command line. Parsing stops before execution policy is assembled. |
| `rust/geom_kernel` | The batch-first triangle-soup geometry kernel: BVH, closest point, ray and segment batches, slicing, parity grids. Every public query takes a slice and returns a slice. |
| `rust/chitubox_io` | Reads and writes `.chitubox` projects starting from an *empty* project. Every write is verified by structural re-parse. |

---

## Three rules that bind all of it

**Batch-first.** Gather, one batched call, scatter. A per-item call in a hot path is a
defect, not a style choice — it is the single most common cause of a stage that works on a
test fixture and falls over on a real mini.

**f64 everywhere** in the geometry data path, including intermediate buffers and test
fixtures. `f32` appears only at the `.chitubox` format boundary, converted outward and never
back inward.

**Determinism by construction.** Index-parallel map-collect only; never a float reduction
across parallel work items, because float addition is not associative and a parallel sum
makes the answer depend on the thread count. One workspace-wide thread control,
`PLINTH_THREADS`, and every parallel path carries a test that runs it at several thread
counts and asserts the outputs are *identical*, not close.
