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

Every model in the test corpus reaches a verified artifact, including the flagship: a
**429,120-triangle** production miniature that runs the whole chain to a 27 MiB project with
2,585 support records, twice, byte for byte the same.

## Quick start

Requires only the Rust toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml).
There is no other build dependency and no interpreter in the loop.

```sh
cargo build --workspace --offline
```

The runnable end-to-end entry point today is an example that drives the real chain over the
test corpus and writes artifacts to `exports/`:

```sh
# every fast corpus model, one artifact each
cargo run --release --offline --example first_artifact -p plinth-pipeline -- --all

# one model by name (the flagship takes minutes, not seconds)
cargo run --release --offline --example first_artifact -p plinth-pipeline -- "Weeping Treant v1.2"
```

Each run prints the stage results, the emitted record counts, and the verdict of the format
gate that re-read the file, then leaves
`exports/<model>-first-artifact.chitubox` behind. Open it in ChituBox to look at the
supports and slice.

The corpus STLs are inputs rather than source and are not stored in git; see
[`docs/development.md`](docs/development.md) for how to provision them.

**The command-line front end is not wired up yet.** `plinth prepare` parses its flags and
stops there — the folder-level runner behind it is the next thing being built, and the flag
spelling is not settled.

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

**1. Support quality needs work.** The supports in a generated file are structurally sound —
each one is sized to the measured load rule and proven attached — but their quality as a
print aid has known issues that will be addressed. Expect to inspect the file before you
print it.

**2. Hollowing does not yet handle models with several internal cavities.** A shell that
encloses more than one separate void is currently refused by the hollow stage, and the model
is prepared **solid** instead. It still produces a correct, printable file; it just does not
get the resin saving. The flagship tree is one such model.

The planned fix keeps the cavities that are large enough to drain reliably, refills the rest
with resin, and prefers to place the drain holes on surfaces that will not be seen.

**3. The calibration is printer- and resin-specific.** Every measured constant comes from
plates printed on one machine in one class of resin. Another printer or another resin needs
its own plates before the numbers mean anything. Some of the engine's values are still
unmeasured placeholders, and they are labelled as such rather than quietly trusted —
[`docs/physics.md`](docs/physics.md) explains how to tell which is which.

**4. Slicing is still ChituBox's job.** plinth produces the project; you open it and slice.

## Where it is going

Four steps, in order. Each one is useful on its own.

| | |
| --- | --- |
| **Prepare a folder** | `plinth prepare <folder> --output <printer>` — walk a directory of STLs and write one prepared project per model, with no GUI in the loop. |
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
