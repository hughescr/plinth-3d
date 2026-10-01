# plinth-3d

**plinth** turns a miniature STL into a print-ready resin job.

Point it at a model and it orients the mesh, optionally hollows it, generates a support
forest sized from measured failure physics, and exports a `.chitubox` project you can open,
inspect and slice — no manual support painting, no density slider, no eyeballing.

The difference from a commercial slicer is where the support geometry comes from.
Commercial slicers place supports by heuristic: a palette of tip sizes, a density slider,
angle rules. plinth sizes every arm from a failure boundary that was measured on printed
calibration plates, and then proves each support is actually attached to the model before it
will ship the file. See [`docs/physics.md`](docs/physics.md).

---

## What it does today

| | |
| --- | --- |
| **Input** | A triangle-mesh STL of one miniature. |
| **Output** | One `.chitubox` project per model, with the model and its supports as **separate** records — so you can look at the supports as supports, and edit them. |
| **Printer** | Elegoo Saturn 4 Ultra 16K. It is the only output profile, because it is the machine the physics was measured on. |
| **Determinism** | The same input produces a byte-identical file, run after run, at any thread count. |
| **Self-check** | Every artifact is judged by re-opening the bytes on disk — a fresh parse, a fresh winding computation, a structural format gate — never by trusting what the producer thought it wrote. |

Not every model in the corpus reaches an artifact yet — see
[`docs/status.md`](docs/status.md) for the current count and what still blocks the rest. Every
model that *does* reach one is verified byte-identical on repeat runs, including the
flagship: a **429,120-triangle** production miniature that runs the whole chain to a 27 MiB
project with 2,585 support records, twice, byte for byte the same.

## Quick start

Requires only the Rust toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml).
There is no other build dependency and no interpreter in the loop.

```sh
cargo build --workspace --offline
```

`plinth prepare` walks a folder of STLs and writes one `.chitubox` project per model that
passes the pipeline's pre-write physics gates:

```sh
cargo run --release --offline -p plinth-cli -- prepare \
  --input-models-dir path/to/stls --output saturn4-ultra --out-dir prepared/
```

Orientation defaults to a dense per-model search (`--orient dense`); pass
`--orient pinned:DIR` to replay a rotation a previous run recorded instead of searching again,
or `--orient none` to use every model exactly as drawn (no rotation; it is still centred and
lifted off the plate). One model can be used as drawn under any mode by marking it: the Finder
tag `plinth-no-rotate` on macOS, `plinth-no-rotate` in the file's `user.xdg.tags` attribute on
Linux (`setfattr -n user.xdg.tags -v plinth-no-rotate model.stl`), or, on any platform, an
empty marker file `<stem>.asdrawn` beside `<stem>.stl` (the only form that survives git or a
zip). A tag on the file a symlinked STL points at counts. The report says per model `as drawn (<reason>)` or `orientation searched`.
`corpus/as-drawn/` holds the Round bases drawn glue face up and marked this way.
`--wall-mm` overrides the hollow shell's wall thickness. Each run prints a per-model report —
stage results, emitted record counts, the size the pipeline assumed the model was — and exits
`3` if any model was refused by a physics gate, so a calling script can tell a refusal from a
crash. Open the resulting `.chitubox` in ChituBox to look at the supports and slice.

For driving the engine directly over the test corpus instead of a folder of your own STLs
(useful for development), there is also an example:

```sh
cargo run --release --offline --example first_artifact -p plinth-pipeline -- --all
```

The corpus STLs are inputs rather than source and are not stored in git; see
[`docs/development.md`](docs/development.md) for how to provision them.

## How it fits together

```
STL ─▶ decimate ─▶ orient ─▶ hollow ─▶ supports ─▶ lower ─▶ emit ─▶ .chitubox
                                          │
                        perception · specialized contacts · initial contacts
                        routing · rewire · brace · structure
```

Each stage's job, and the crate that owns it, is in
[`docs/architecture.md`](docs/architecture.md). The measured physics the support stage sizes
from is in [`docs/physics.md`](docs/physics.md).

## Known limitations

Not every model in the corpus emits, and the current count, causes, and the work in progress
against them are tracked in [`docs/status.md`](docs/status.md) rather than repeated here.
Limitations that are architectural rather than a specific open task:

**1. Hollowing does not yet handle models with several internal cavities.** A shell that
encloses more than one separate void is currently refused by the hollow stage, and the model
is prepared **solid** instead. It still produces a correct, printable file; it just does not
get the resin saving. The flagship tree is one such model. The planned fix keeps the cavities
large enough to drain reliably, refills the rest with resin, and prefers to place the drain
holes on surfaces that will not be seen.

**2. The calibration is printer- and resin-specific.** Every measured constant comes from
plates printed on one machine in one class of resin. Another printer or another resin needs
its own plates before the numbers mean anything. Some of the engine's values are still
unmeasured placeholders, and they are labelled as such rather than quietly trusted —
[`docs/physics.md`](docs/physics.md) explains how to tell which is which, and
[`docs/plans/support-calibration.md`](docs/plans/support-calibration.md) tracks the plates
still queued.

**3. Slicing is still ChituBox's job.** plinth produces the project; you open it and slice.

## Where it is going

`plinth prepare` — folder in, one project per model out, no GUI in the loop — is the working
entry point today. Beyond it, in order:

| | |
| --- | --- |
| **Plate packing** | Pack several models onto one build plate, laid out against that printer's real plate geometry. |
| **Native slicing** | Slice directly to whatever the machine eats, so the automated path no longer needs ChituBox at all. `.chitubox` stays a first-class output for anyone who wants to look before printing. |
| **Send to the printer** | Deliver the sliced job over the network. |

Other project formats and a plain STL export are possibilities rather than plans. The bar
for any new output format is that it must keep the model separable from its supports —
that separability is what makes inspection and re-editing worth anything.

## Working on it

[`docs/development.md`](docs/development.md) has the build and test discipline: the five
commands that gate every change, the determinism check that pins the engine's output, how
constants are registered, and how to provision the test corpus.

## License

Licensed under the Apache License, Version 2.0. The full text is in
[LICENSE.md](LICENSE.md).
