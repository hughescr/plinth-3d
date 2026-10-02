# Architecture

How a model becomes a printable file, and which crate owns each part of it.

```
STL ─▶ repair ─▶ decimate ─▶ orient ─▶ hollow ─▶ supports ─▶ lower ─▶ gate ─▶ emit ─▶ .chitubox
                                                     │
                                   perception · specialized contacts · initial contacts
                                   routing · rewire · brace · structure
```

Every stage hands the next one a typed value, and a stage that cannot do its job returns a
typed refusal rather than a degraded result. Refusals are carried forward and recorded; they
never turn into silence.

---

## The stages

### Repair

An automatic pre-pass, upstream of every gate in the chain, that fixes two measured defect
classes on the loaded mesh: dust shards (delegated whole to the same six-criteria triage the
hollow stage runs) and exact-duplicate faces (re-derived after the strip, so a stripped shard
cannot leave a duplicate decision looking at adjacency that no longer exists). A third action,
splitting pinch vertices, was removed after a 2026-08-12 ruling reclassified a point-touch as a
legitimate contact rather than a defect (see [`physics.md`](physics.md)).

The pass may only **decline**, never refuse: it returns a mesh, not a `Result`, so a model it
cannot help — or that trips its own resource bound — comes back bit-identical with the reason
recorded, and every gate downstream judges exactly what it judged before this stage existed. A
mesh with neither defect passes through byte-identical, proved through a binary-STL round trip
rather than by inspection. No gate is softened by this stage; a model whose defects survive it
is refused exactly as it was before, and the refusal is an automation work item, not a
threshold to move.

### Decimate

Reduces the triangle count under an explicit, measured error budget, protecting the features
that matter. It runs immediately after repair, on purpose: its output is the *only* production
mesh every later stage may consume, so no stage is ever reasoning about a different mesh than
the one that will print.

Reduction is not guaranteed. Candidates that break mesh topology are rejected, and if no
candidate survives, the stage hands on the original mesh. That is a legitimate outcome, not
a failure — the guarantee is about what downstream sees, not about how small it is.

### Orient

Applies a rigid rotation and rebuilds the build frame: the transformed bounding box is
centred in XY and rests at Z = 0. The rotated mesh replaces the production mesh outright; no
untransformed second copy survives the stage, so there is nothing for a later stage to pick
up by mistake.

Orientation is the coupling hub of the whole problem — it decides which surfaces face down,
and therefore where the unavoidable support scars land (see [`physics.md`](physics.md)). By
default the stage runs a three-tier dense sweep over a candidate grid — coarse on a proxy
mesh, refine the leading candidates on a finer proxy, commit the survivors (plus a
permanently escorted legacy candidate block, so the sweep is never worse than the orientation
it replaced) at the full mesh — and commits the winner. It can instead replay a **pinned**
baseline: one prior rotation, applied verbatim, keyed by model name so a folder of different
minis is never given one model's answer. Either way, orientation is never inferred: the sweep
measures every candidate and records what it measured, including when the winner is the
identity rotation.

The operator can also say a model is to be used **as drawn** (GitHub issue #1): identity
rotation, nothing searched, still centred and lifted off the plate like any other model. That
holds when ANY of these does: `--orient none` (every model in the run); the model named in
`--no-orient=<list>` (comma-separated file names in the input folder, with or without `.stl`,
`*` and `?` globs; matched once over the whole folder, and a name that matches no model is a
WARNING line on the run report, never a refusal); the macOS Finder tag
`plinth-no-rotate`, read straight from the file's `com.apple.metadata:_kMDItemUserTags`
extended attribute, not through Spotlight; `plinth-no-rotate` in the Linux `user.xdg.tags`
attribute; or a marker file `<stem>.asdrawn` beside the STL, the portable form that survives
git. A symlinked STL is read through the link: the tag on the file it points at counts, and
so does a tag on the link itself; the marker is looked for beside the link. A mark beats
`dense` and `pinned:DIR` alike. The run-wide switches are checked first (`--orient none`,
then the list); one reason is recorded, the first that holds. The orientation record says which mark it was
(`OrientationProvenance::AsDrawn`), and the run report prints one line per model:
`as drawn (<reason>)` or `orientation searched`. A tag attribute that is present but unreadable
is a WARNING line and the model is treated as untagged, never refused.

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

### Gate

The sole, override-free door between a sealed support authority and a written file. It judges
the sealed verification snapshot — today, whether the model's surface is adequately covered —
before the authority is consumed by anything downstream. A refused model leaves no artifact on
disk at all: not a truncated one, not an empty one, and not a stale file left over from an
earlier run of the same model.

### Emit

The single boundary where f64 geometry becomes the `.chitubox` file's f32 records. Only a
passed authority reaches it. It writes one fresh project — never a template that already
contains a model — and then audits a **newly opened** copy of what it just wrote.

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
| `plinth-mesh` | The f64 mesh substrate — topology, delicacy, STL I/O, triage, and the automatic repair pre-pass (declines rather than refuses; a clean mesh passes through byte-identical). Loading keeps meshes exactly as their source supplied them: no implicit weld, reorder or f32 conversion. |
| `plinth-decimate` | The mandatory production decimation substrate: a narrow adapter over a locked QEM simplifier, with probing, protection halos and error-budget measurement around it. |
| `plinth-orient` | Orientation: the three-tier dense sweep and the rigid rotation it (or a pinned baseline, or an operator's as-drawn mark) commits, rebuilding the build frame; `as_drawn` reads the per-model marks (tags via the `xattr` and, on macOS, `plist` crates; the `<stem>.asdrawn` marker everywhere). |
| `plinth-hollow` | Shell creation, cavity field, drain and vent placement, drilling. |
| `plinth-solid` | Batch-shaped f64 boolean-solid operations. |
| `plinth-layers` | Per-layer polygon union with fill-rule normalization, so every downstream consumer of a slice is correct whichever fill rule it evaluates. |
| `plinth-support` | The engine: perception, contact physics, routing, rewiring, bracing, member sizing, coverage, raft, lowering, and the run sealer. |
| `plinth-verify` | The gate crate: `verify_coverage`, the coverage verdict the pre-write gate consults before any bytes are written, and `verify_artifact`, disk-first `.chitubox` container verification reading precisely the bytes on disk with no producer state, cache, or repair path. **`verify_artifact` is not currently called by the production `prepare` route** — the only format check that runs after a write today is `plinth-emit`'s own fresh-open disk audit; `verify_artifact` is exercised only by this crate's tests and one example, and is scheduled for wiring into the pre-write gate porting work. |
| `plinth-emit` | The single final emission boundary: f64 support geometry to f32 wire records, one fresh project, then an audit of a freshly opened copy. |
| `plinth-pipeline` | The folder-in, one-artifact-per-model runner: sequences repair → decimate → orient → hollow → support for every model in a directory, then owns the pre-write gate (§ Gate) that is the sole door between a sealed support authority and a written file, before handing a passed model to `plinth-emit`. |
| `plinth-cli` | The `plinth` binary. One subcommand, `prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--orient dense\|none\|pinned:DIR] [--wall-mm MM] [--no-cache]`: resolves the request against the registered printer profile and resource policy, runs the pipeline over every model in the directory, and reports each model's disposition. |
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
