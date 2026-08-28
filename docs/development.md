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
  tests, examples and benches rather than just the library. **The workspace lint table names
  every lint it enables and enables no GROUPS** — see the rule written at
  `[workspace.lints.clippy]` in the root `Cargo.toml`. `clippy::pedantic` and
  `clippy::nursery` gain members between clippy releases, so enabling either signs this gate
  up for lints that do not exist yet: measured on this workspace, the two groups raise 4,256
  warnings across 68 lints, every one of them an error here, and no `allow` list can win that
  race because the next release adds the next batch. The enabled set is the signal set from
  `docs/evidence/remediation-2026-08-26/census/census-clippy.md`; what is suppressed at a site
  is suppressed with `#[expect]`, named lints and a reason, and is inventoried in
  `docs/evidence/remediation-2026-08-26/census/cast-audit.md`.
- **`cargo fmt --check`** as its own gate. Formatting violations otherwise ride in on the
  back of a green test run.
- **`git diff --check`** for whitespace damage, which nothing above catches.

Attribute every change in the test counts. A delta you cannot name is an unexplained change
in behaviour, whichever direction it moved.

### The expected numbers, current (MEASURED at `develop@7e11319`, the `#77` merge, and `225b6a7`, the `a4-controller` merge)

- `plinth-support` lib tests: **653/653**.
- `retired_formula_guard` (`crates/plinth-support/tests/retired_formula_guard.rs`):
  **13/13**.
- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`: **0**
  warnings. Held by the named-lints rule above; re-measured 2026-08-27 on
  `feature/remediation-kernel`.
- `cargo fmt --all -- --check`: **exactly 104 hunks** (re-confirmed directly, 2026-08-25,
  at the current tip `7eb5f30`). This is standing fmt debt in already-committed code, not a
  target — it must not be edited down opportunistically inside an unrelated change (that
  produces an unreviewable diff), and it must never **grow**. A change that adds a 105th
  hunk is not gate-clean.
  - **This number is stale and the drift is not attributed.** Measured 2026-08-27 against a
    clean `git archive` of `feature/remediation-kernel`'s tip, the count is **120**, so 16
    hunks arrived between `7eb5f30` and here without being named. Left as found rather than
    re-pinned: moving the number to 120 would be exactly the re-freeze this section forbids,
    and the 16 belong to whichever change introduced them. What the lint-reconciliation
    change asserts is only that it added none of them — it leaves the count at 120.

A change that moves any of these numbers without explaining why is not done; attribute the
movement in the commit, the way every re-pin below has to.

### The four documented reds (task `#76`) — CLOSED 2026-08-26

**All nine are now green, and no test in the tree is knowingly left red for a judge.** Two
tests in `crates/plinth-support/tests/support_runner_integration.rs` are outside that
statement and were outside the recorded baseline too, because that binary exceeds the
baseline harness's 240 s budget and was never observed to completion:

- `closure_repairs_censored_dust_because_that_repair_closes_a_real_hole` fails, by design and
  with its own `expect` message saying so — it reports FINDING 1, a live support-planning
  defect. VERIFIED unchanged at `5d0420e` in a detached worktree on 2026-08-26: identical
  panic, identical `candidates_tried: 274`. It is a pre-existing deliberate red, not a
  regression, and nothing in the fixture work below touches it.
- `a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule` did not
  finish: 887 trials over 47 minutes with 2,548 candidate rows still ranked. That is
  REVIEW.md short-list 12, the uncapped trial count, and it is why the binary blows every
  budget. Its verdict is unmeasured, before this change and after it.

The binary's other three tests, including the re-pinned one, pass in 3.7 s together. A red
test outside those two is a new finding.

The record of what they were, because the fix is a re-pin and re-pins are attributed here:
`crates/plinth-pipeline/tests/a4_production_door.rs` and
`crates/plinth-pipeline/tests/first_artifact.rs` carried four tests that ran and failed on
`trials:0 commits:0` — no coverage-repair pass triggered on their fixtures — and
`crates/plinth-support/tests/a4_transactional_controller.rs` (4) and
`crates/plinth-support/tests/support_runner_integration.rs` (1) carried five more of the
same cluster, nine in all. None was digest-related.

- `a4_production_door::the_production_door_reports_what_the_one_winner_controller_spent`
- `a4_production_door::the_sealed_ledger_on_disk_names_the_row_that_closed_the_hole`
- `first_artifact::corpus_sphere_drilled_plan_is_refused_before_any_file_exists`
- `first_artifact::a_coverage_refusal_removes_a_stale_artifact_already_at_the_target`

**One cause, and it was a fixture, not a defect.** `3a6526f` (`fix(support): converge the
clearance mask and seat the model anchor`) let the model-seated anchor family route for the
first time. `make_overhang_table` and the corpus `Sphere` had both been chosen BECAUSE their
baseline layouts did not cover; afterwards they do, they seal at attempt 0, and every test
that reads a COMMITTED repair step or a coverage REFUSAL from them was left with nothing to
observe. The tests were vacuous, not wrong, and `3a6526f`'s own message says so and leaves
them red for a judge.

**The fix, per the owner's ruling of 2026-08-26: a new fixture, no edited expectation.**
`plinth_mesh::make_uncovered_rim_table` — the `make_overhang_table` generator at
25 x 25 x 4 mm on 2 x 2 x 20 mm legs — still measures a blocking patch at attempt 0
(0.553 mm² uncovered, 6.901 mm worst gap against the registered 6.0 mm radius, above the
0.16 mm² censor) and closes it from its own repair pool: `trials=2 commits=1 remaining=539`.
The committed-repair tests were re-pointed at it; the stale-artifact refusal test was
re-pointed at `make_raking_blade`, which still refuses; and
`corpus_sphere_drilled_plan_is_refused_before_any_file_exists` was DELETED, because its
claims are all made by `procedural_raking_blade_is_refused_before_any_file_exists` on a
fixture that still refuses. No assertion was weakened and no expected value was re-frozen.

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

## Reading a repair search that has not finished

The coverage-repair controller writes its trial stream — `steps` in
`<model>.coverage.repair.json` — only when the search reaches a seal or a refusal. A run
that is killed, or that is still going, therefore leaves no stream at all, which is the
one run you most want to read.

Point `PLINTH_REPAIR_TRIAL_TRAIL_DIR` at a directory and each support run appends its
trials there as they land, one JSON row per line, flushed:

```sh
PLINTH_REPAIR_TRIAL_TRAIL_DIR=/tmp/trail cargo run --release -- prepare …
jq -s '.' /tmp/trail/repair-trial-trail.v1.*.jsonl        # the same array as `steps`
```

Each line is one `CoverageRepairStepWire`, so a `jq` written against a finished run's
sidecar reads a killed run's trail unchanged. One file per support run, named for the
process and the run within it, so concurrent models never interleave.

With the variable unset no file is opened and nothing is written — the trail is
instrumentation, and it changes no verdict either way. The run summary names the file and
the row count when one is open.

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
