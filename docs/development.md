# Development

Everything a change has to satisfy before it lands, and how to run the checks.

## Toolchain

The Rust toolchain pinned in [`../rust-toolchain.toml`](../rust-toolchain.toml) is the whole
build surface. There is no interpreter, no code generator and no external service in the
loop — `cargo build --workspace --offline` from a fresh checkout is the entire story.

Builds and tests run `--offline` by design: the dependency set is locked, and a build that
reaches the network is a build whose result depends on the day it ran.

## Gate profile: cheap in the lane, exact at the merge

Two build profiles exist in `Cargo.toml`, and they answer different questions:

- **`--profile gate`**, used for every iterate-and-test loop inside a lane. It has release
  **semantics** — `debug-assertions` off, `overflow-checks` matching release, so it exercises
  every fixture that only runs with debug assertions off — at a much cheaper compile cost:
  incremental compilation on, LTO off, more codegen units, `opt-level = 2`. A one-line change
  to a local crate is an incremental recompile under `gate`; the same change forces a full
  `opt-level = 3` rebuild of that crate under `--release`.
- **`--release`**, used for the merge gate into `develop` and nowhere else. `gate`'s cheaper
  codegen is a compile-cost knob, not a semantics knob — the two profiles are expected to
  agree on every pinned number — but the merge gate is the one that has to catch it if they
  ever do not, so it keeps the exact codegen configuration that ships.

```sh
cargo test --workspace --profile gate --offline    # lane loop
cargo test --workspace --release      --offline    # merge gate
```

## The gate: five commands, all of them, every time

```sh
cargo test   --workspace --offline --no-fail-fast
cargo test   --workspace --offline --no-fail-fast --all-features
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all -- --check
git diff --check
```

Run all five (substituting `--profile gate` for `--release`'s cost inside a lane, per above),
and run them on a **clean tree** — before and after. Each one exists because dropping it let
something through:

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

### The expected numbers, current (MEASURED at `develop@7e11319`/`225b6a7`, the `#77` merge)

- `plinth-support` lib tests: **653/653**.
- `retired_formula_guard` (`crates/plinth-support/tests/retired_formula_guard.rs`):
  **13/13**.
- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`: **0**
  warnings.
- `cargo fmt --all -- --check`: **exactly 104 hunks** (re-confirmed directly, 2026-08-25,
  at the current tip `7eb5f30`). This is standing fmt debt in already-committed code, not a
  target — it must not be edited down opportunistically inside an unrelated change (that
  produces an unreviewable diff), and it must never **grow**. A change that adds a 105th
  hunk is not gate-clean.

A change that moves any of these numbers without explaining why is not done; attribute the
movement in the commit, the way every re-pin below has to.

### The four documented reds (task `#76`)

`crates/plinth-pipeline/tests/a4_production_door.rs` and
`crates/plinth-pipeline/tests/first_artifact.rs` carry four tests that are **known red**,
not ignored — they run, and they fail, on `trials:0 commits:0` (no coverage-repair pass ever
triggers on their fixtures), and none of the four is digest-related:

- `a4_production_door::the_production_door_reports_what_the_one_winner_controller_spent`
- `a4_production_door::the_sealed_ledger_on_disk_names_the_row_that_closed_the_hole`
- `first_artifact::corpus_sphere_drilled_plan_is_refused_before_any_file_exists`
- `first_artifact::a_coverage_refusal_removes_a_stale_artifact_already_at_the_target`

These four are the only documented pipeline test reds in the tree. A gate run that turns up
a red test outside this set is a new finding, not this one.

### The golden re-pin rule, with `from → to` attribution

Some tests pin an exact expectation against real geometry — a support digest, or (in
`crates/plinth-pipeline/src/lib.rs`,
`the_zero_field_divergence_is_measured_per_committed_fixture`) a per-fixture **verdict**
about whether two support runs diverge. When a legitimate behaviour change moves what the
pinned run produces, the fix is never to quietly edit the expectation in place:

1. **Append, don't rewrite.** The prior record stays, verbatim, as a dated comment block; a
   new one is appended on top of it. `the_zero_field_divergence_is_measured_per_committed_fixture`
   is on its **eighth** such re-record — none of the first seven was deleted or edited.
2. **State the attribution as `from → to`.** Name the old value, the new value, and the
   commit that legitimately caused the move. The eighth re-record: the Sphere fixture's
   verdict moved `true → false`, attributed to `a4f5f3f` (`feat(support): re-size a seated
   arm's exemption to its final footprint`, `#36` F1, landed via `#77`), which implements
   Craig's 2026-08-23 ruling that a seated arm's exemption always equals its true footprint.
3. **Measure both ends**, in the same process, same settings, before changing the
   expectation — not argue from the mechanism alone.

A digest or verdict that moved without an attributed `from → to` re-pin is exactly how a
regression becomes the new expected value; the census below states the same rule for the
support-forest fingerprints.

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

**The frozen fingerprints.** Corpus models each have a digest that is expected to be
**byte-identical on every run, forever**, and those values are recorded with the run that
established them. What they mean:

- A digest that has **not** moved is evidence your change is behaviour-neutral on real
  geometry — which is exactly what a refactor, an optimization or a batching fix is supposed
  to be.
- A digest that **has** moved means the geometry the engine produces has changed. That is
  not automatically wrong, but it is never something to accept quietly. Attribute the
  movement — which models, which member, why — decide deliberately whether the new geometry
  is more correct than the old, and only then re-freeze, the same `from → to` way described
  above.
- Re-freezing without an attribution is how a regression becomes the new expected value.

Run the census whenever you touch anything the support path can see, including code you
believe is inert.

## Corpus provisioning

The test models are inputs, not source, and are not stored in git — the largest is over
20 MB. A fresh worktree therefore starts with an **empty** `corpus/models/`, because
`.gitignore` keeps the STLs out of git. Link them in from a checkout that has them:

```sh
NEW_WT=/path/to/your/worktree
MAIN=/path/to/plinth-3d
mkdir -p "$NEW_WT/corpus/models"
ln -sf "$MAIN"/corpus/models/*.stl "$NEW_WT/corpus/models/"
```

Then verify against `corpus/manifest.json` with `shasum -a 256 -c`.

**The symlinks match the same ignore rule, so they are invisible to `git status`.** A
worktree can look perfectly clean and still be unable to run the gate. At least two test
targets hard-fail without the corpus, with errors that do not name the corpus as the cause:
`plinth-decimate`'s `corpus_budgets` test and `plinth-orient`'s
`every_pinned_orientation_record_rederives_its_committed_oriented_solid_artifact`
(`crates/plinth-orient/tests/orientation_rederivation.rs`), which falsifies every pinned
baseline orientation by re-deriving it from the corpus source STL. If those are the only
failures in a fresh checkout, provision before investigating anything else.

`corpus/baseline/` and `corpus/manifest.json` are evidence. They are not regenerated
casually and never as a side effect of another change.

## Conventions

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
- **Records are append-only.** Design records and measurement notes are kept as a growing
  engineering journal: findings are recorded when they are made, and a later correction is
  added as a **dated addendum beside** the original rather than an edit to it (the golden
  re-pin rule above is the same discipline applied to a pinned test). Rewriting a record to
  match what turned out to be true destroys the only evidence of what was believed at the
  time, and with it the ability to audit a decision.
- **Branching is git flow.** `develop` is the integration base and every change starts as a
  `feature/*` branch cut from it, merging back with `--no-ff` so the shape of the work
  survives in history. `main` is release-only. Nothing is pushed without the maintainer's
  say-so.
