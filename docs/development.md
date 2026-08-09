# Development

Everything a change has to satisfy before it lands, and how to run the checks.

## Toolchain

The Rust toolchain pinned in [`../rust-toolchain.toml`](../rust-toolchain.toml) is the whole
build surface. There is no interpreter, no code generator and no external service in the
loop — `cargo build --workspace --offline` from a fresh checkout is the entire story.

Builds and tests run `--offline` by design: the dependency set is locked, and a build that
reaches the network is a build whose result depends on the day it ran.

## The gate: five commands, all of them, every time

```sh
cargo test   --workspace --offline --no-fail-fast
cargo test   --workspace --offline --no-fail-fast --all-features
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Run all five, and run them on a **clean tree** — before and after. Each one exists because
dropping it let something through:

- **Both test invocations.** They compile different sets. Running only the default one leaves
  feature-gated code untested; running only `--all-features` leaves the default build
  untested. If the two totals disagree in a way you cannot explain, that is a signal, not
  noise.
- **`--no-fail-fast`.** Without it, one red test masks the rest, and a single failure gets
  mistaken for a smaller or a larger problem than it is.
- **Clippy with `-D warnings`, over all targets and all features**, so lint coverage includes
  tests, examples and benches rather than just the library.
- **`cargo fmt --check`** as its own gate. Formatting violations otherwise ride in on the
  back of a green test run.
- **`git diff --check`** for whitespace damage, which nothing above catches.

Attribute every change in the test counts. A delta you cannot name is an unexplained change
in behaviour, whichever direction it moved.

## Determinism: the census check

The engine's central promise is that the same model produces the same output, so it is
checked directly rather than assumed.

The census harness runs the real support-preparation path over the whole test corpus and
prints, per model, a **snapshot digest** of the sealed support forest along with its routed
contact and live member counts. It lives behind a non-default cargo feature, so census
instrumentation can never reach a production build:

```sh
cargo build --offline --example embed_incidence_census -p plinth-support \
  --features census --profile dev-opt
PLINTH_CENSUS=1 ./target/dev-opt/examples/embed_incidence_census
```

Without `PLINTH_CENSUS=1` the identical binary produces an un-instrumented baseline run, so
an instrumented run can be diffed against it and the instrumentation proven result-inert.

**The frozen fingerprints.** Seven corpus models have a digest that is expected to be
**byte-identical on every run, forever**, and those values are recorded with the run that
established them. What they mean:

- A digest that has **not** moved is evidence your change is behaviour-neutral on real
  geometry — which is exactly what a refactor, an optimization or a batching fix is supposed
  to be.
- A digest that **has** moved means the geometry the engine produces has changed. That is
  not automatically wrong, but it is never something to accept quietly. Attribute the
  movement — which models, which member, why — decide deliberately whether the new geometry
  is more correct than the old, and only then re-freeze.
- Re-freezing without an attribution is how a regression becomes the new expected value.

Run the census whenever you touch anything the support path can see, including code you
believe is inert.

## Corpus provisioning

The test models are inputs, not source, and are not stored in git — the largest is over
20 MB. A fresh worktree therefore starts with an empty `corpus/models/`. Link them in from a
checkout that has them:

```sh
NEW_WT=/path/to/your/worktree
MAIN=/path/to/plinth-3d
mkdir -p "$NEW_WT/corpus/models"
ln -sf "$MAIN"/corpus/models/*.stl "$NEW_WT/corpus/models/"
```

Then verify against `corpus/manifest.json` with `shasum -a 256 -c`.

**The symlinks match the same ignore rule, so they are invisible to `git status`.** A
worktree can look perfectly clean and still be unable to run the gate. Two test targets
hard-fail without the corpus, with errors that do not name the corpus as the cause: the
decimation corpus budgets and the orientation scale audit. If those two are the only
failures in a fresh checkout, provision before investigating anything else.

`corpus/baseline/` and `corpus/manifest.json` are evidence. They are not regenerated
casually and never as a side effect of another change.

## Constants: the registry and the ledger

Every physical or policy value the engine reads is a **registered constant**, typed with its
provenance:

- **measured** — names the record it came from and its date.
- **derived** — names its inputs and the exact relation.
- **placeholder** — explicitly *not* safe to accept a print on.

Constants live in `crates/plinth-core/src/constants/` — `evidence.rs` for measured and
derived values, `placeholders.rs` for the rest. Beside the registry sit two things that must
agree with it: a generated table of every published value, and a hand-maintained JSON source
ledger giving each constant's prose provenance.

**To add a constant:**

1. Register it in the right module with an honest provenance class. If you do not have a
   measurement, it is a **placeholder** — say so. A number that is quietly typed as measured
   is worse than an obviously unmeasured one, because it can be quoted as a safety claim.
2. Add its ledger entry. A mechanical test asserts that the ledger and the registry name
   exactly the same set of ids, in both directions, and it will fail on the id you forgot.
3. Update the generated constants table; another test compares it against the live registry.
4. Run the gate. Several tests pin the registry's exact contents, so a new constant is never
   an accident.

**Never register the same threshold twice.** A duplicate is not redundancy, it is a
divergence hazard: a future recalibration moves one copy and silently leaves the other stale.
If two call sites need the same allowance, they read the same entry.

Resource caps are registered too, and the same honesty applies. A cap that fires is a finding
to investigate — make the bound honest, or fix the algorithm. Raising a cap so a quadratic
fits through it is not a fix.

## Conventions that tests enforce

- **Batch-first.** Gather, one batched call, scatter. A per-item call in a hot path is a
  defect, not a style preference.
- **f64 in the geometry path**, `f32` only at the output format's boundary, converted outward
  and never back inward.
- **No float reduction across parallel work items.** Index-parallel map-collect only. Every
  parallel path carries a thread-sweep test that runs it at several thread counts and asserts
  the outputs are *identical*. Thread count comes from one workspace-wide control,
  `PLINTH_THREADS` — a per-crate variable would make the sweep pass vacuously.
- **Refuse, don't degrade.** A stage that cannot do its job returns a typed refusal naming
  what it could not satisfy. Silently producing a worse answer is the one failure mode
  nothing downstream can detect.
- **Run outputs are not source.** Artifacts, censuses, profiles and screenshots are outputs
  of a particular build; they are not committed.

## Branching

Git flow. `develop` is the integration base and every change starts as a `feature/*` branch
cut from it, merging back with `--no-ff` so the shape of the work survives in history.
`main` is release-only. Nothing is pushed without the maintainer's say-so.

## Records

Design records and measurement notes are kept as an append-only engineering journal:
findings are recorded when they are made, and a later correction is added as a **dated
addendum beside** the original rather than an edit to it. Rewriting a record to match what
turned out to be true destroys the only evidence of what was believed at the time, and with
it the ability to audit a decision. Moving a record is fine; rewriting its content is not.
