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

## The test loop: scripts, known reds, a warm new worktree

Measured 2026-10-13 on an M3 Max, gate profile, through the CPU lock (timing mode).

```sh
scripts/test-fast.sh                 # lane loop: changed crates + dependents, PASS / NEW-RED / FIXED-RED
scripts/test-fast.sh --base HEAD plinth-sweep -- some_filter   # one crate, one test-name filter
scripts/gate-full.sh                 # milestone: every test binary, ignored tests too, side by side
```

- **`scripts/test-fast.sh`** runs `cargo test --profile gate` for the crates you name, or by default
  for the crates changed against `develop` (`--base REF` changes the reference; `--base HEAD` means
  only your uncommitted edits) plus every workspace crate that depends on them
  (`--no-dependents` leaves those out). Test binaries run three at a time. A change to the root
  `Cargo.toml` or `Cargo.lock` counts as a change to every crate. Ignored tests do not run, and
  neither do plinth-sweep's slow integration tests (below); `--slow` adds those.
- **`scripts/gate-full.sh`** is the test half of the merge gate: every workspace test binary and
  doctest under `--profile gate` with `--include-ignored`, the separate `tools/plinth-audit`
  workspace (built `--release`: it has no gate profile), at most `-j` binaries at once. Use it at a
  milestone, not per lane. `--skip-ignored` leaves out the ignored tests (the grinders, and tests
  that read local minis); that is the routine milestone run, 24 minutes. The big binaries each
  use every core, so running them side by side saves little except around the single-threaded
  grinders. The ignored set is red where its inputs are absent; those tests are in
  `known-reds.txt` marked "ignored".
- **`scripts/known-reds.txt`** lists the tests that are red on `develop` today, one line each:
  `<package>::<test program>::<test path> => <signature> ;; <signature>  # reason` (`lib` for a
  library's unit tests). A signature is a fixed string from the test's panic message that its
  failure text must contain. Both scripts print each run against the list: `PASS` (nothing failed
  outside the list), `NEW-RED` (a failure that is not listed, or a test binary that did not
  complete: no libtest summary line, or an exit status other than 0 and 101; exit status 1),
  `CHANGED-RED` (a listed test failing with a different message: exit status 1, so a known name
  cannot hide a new defect), `FIXED-RED` (a listed test that passes: delete its line). A change
  that fixes a red removes its line in the same commit; nobody adds a line to hide a failure
  without the signature and the reason beside it. `scripts/test-units-selftest.sh` checks this
  verdict logic on synthetic logs (no cargo needed); run it after changing `scripts/lib`.
- **Slow legacy-runner tests are `#[ignore]`d** with the reason
  `slow legacy-runner test: run with scripts/gate-full.sh`. The legacy runner is not on the
  `--placement sweep` path; a regression in it shows at the milestone, not in the lane loop.
- **plinth-sweep's slow integration tests run only at the milestone.** About 130 of the 276 tests
  in `crates/plinth-sweep/tests/it` sweep a heavy fixture (the laid tube, the stilt plate, the
  Cylinder, a dense orientation search); one sweep of the laid tube is 90 s on one core. They are
  ignored (`#[cfg_attr(not(feature = "slow-tests"), ignore = "slow: ...")]`) or, for a file that
  is slow throughout, not built, unless `PLINTH_SLOW_TESTS=1` is set. `scripts/gate-full.sh`
  always sets it, also under `--skip-ignored`; `scripts/test-fast.sh --slow` sets it for one run.
  The remaining 142 run in about 70 s. Switching the variable rebuilds plinth-sweep and its
  dependents once. A new test that sweeps a heavy fixture joins the slow set the same way (see
  the header of `tests/it/main.rs`); a file that another test file imports from stays built and
  ignores test by test. The fast set still runs every plinth-sweep unit test (`--lib`) and the
  light integration tests: footprint, contact growth, head join and hookup, joint sizing on the
  brace pair, discs and the round bases.
- **plinth-sweep integration tests share their sweeps.** `tests/harness` computes each distinct
  sweep (same mesh, layer plan, policy and hole lists, compared by value) once per test program;
  `sweep`, `sweep_with`, `sweep_with_drains` and `memo_run_sweep` all go through it, and every
  caller gets its own copy. A test that grades the run itself (counters, timings) calls
  `sweep_with_uncached` or `plinth_sweep::run_sweep*`. A new test that sweeps a shared fixture
  must use the harness entry, not call `run_sweep` itself, or the suite pays the sweep again.
- **A new worktree starts warm.** The `post-checkout` hook (source `scripts/hooks/post-checkout`;
  install with `scripts/install-hooks.sh`, once per clone and after any change to the hook) clones
  `../.plinth-target`, a target directory holding only compiled registry dependencies, into the
  new worktree's `target/`, and clones the gitignored corpus meshes in from the main checkout. It
  writes no `.cargo/config.toml`: `incremental = false` doubled the cost of a gate rebuild, and the
  sccache wrapper never ran (the CPU lock sets `RUSTC_WRAPPER` empty for every locked job). Renew
  the seed with `scripts/refresh-seed.sh` when `Cargo.lock` changes (the hook says when it is
  stale). A stale seed is slower, never wrong.
- **Vendored C++ builds in parallel.** `plinth-solid` names `cc` with its `parallel` feature as a
  build dependency, so the 19 Manifold translation units compile together.

| step | before | after |
|---|---|---|
| plinth-solid build script, cold gate build (wall) | 19.6 s | 6.5 s |
| first `cargo test --profile gate --workspace --no-run` in a new worktree | 60.3 s | 32.4 s |
| edit `plinth-sweep/slenderness.rs` -> same build | 10.1 s | 2.8 s |
| edit `plinth-support/lower/junction.rs` -> same build | 18.1 s | 3.1 s |
| plinth-sweep `tests/it` under gate, test wall | 1565 s | 1142 s |
| `test-fast.sh --no-dependents plinth-sweep`, wall (lib + `tests/it`; slow tests moved to the milestone) | 1,202 s | 68 s |
| `test-fast.sh --base HEAD` after a `plinth-verify` edit | | 3 s (183 s with dependents: plinth-pipeline, plinth-cli) |
| `gate-full.sh --skip-ignored` (72 test binaries, 2,748 tests) | about 6,550 s one binary at a time (release, 2026-10-08) | 1,456 s |
| `gate-full.sh` with the ignored tests | | not finished in 140 min: `a_spec6_removal` alone needs 65-80 min on a quiet machine |

## Where integration tests go

Each crate's integration tests are ONE test program, `tests/it/main.rs`, with one module per
file in `tests/it/`. Cargo links every test program on its own against the whole dependency
graph, and under `--release` each link is a thin-LTO pass, so one program per file was the
largest single cost of a test build. Test names carry the file stem as a prefix:

```sh
cargo test -p plinth-sweep --profile gate --offline --test it orient_demand::   # one former file
cargo test -p plinth-sweep --profile gate --offline --test it                   # all of them
```

**A new test goes in `tests/it/<name>.rs` plus a `mod <name>;` line in `tests/it/main.rs`.**
Shared helpers (`tests/harness/`, `tests/common/`, `examples/shared/first_artifact_chain.rs`)
are declared once in `main.rs` and reached as `crate::harness` and so on. Cargo's automatic
test discovery stays on, so a new top-level `tests/<name>.rs` (from a branch cut before this
layout, say) still builds and runs as its own program: nothing is dropped, it only pays its
own link. Move it into `tests/it/` when convenient.

A test stays its own top-level program only when it needs a process to itself. Today that is
every test that re-runs its own executable with `--exact <test name>` under a changed
`PLINTH_THREADS` (inside `it` the name would gain a module prefix and the child would run
nothing): `plinth-decimate/tests/thread_sweep.rs`, `plinth-orient/tests/orient_thread_sweep.rs`,
`plinth-verify/tests/coverage_gate.rs`, and `geom_kernel/tests/{allowed_face_projection,
native_api, point_index}.rs`. A test that diffs a process-wide counter (for example
`geom_kernel/tests/allowed_face_projection_index_reuse.rs`, which reads the BVH and parity
build counters) also needs a process to itself: tests in one program run in parallel threads,
and their builds would land inside its window. The `trybuild` suites (`tests/compile_fail.rs`) and all of
`plinth-support/tests/` (whose slow suites are run one at a time on purpose) are also left as
they were. Before adding a test that sets environment variables, changes the working directory,
installs a panic hook or a global allocator, or times itself, keep it separate the same way:
every test in `it` shares one process with the rest of the crate's tests.

The clearance kernel has two checks of its own. `geom_kernel/src/kernel_speed_tests.rs` holds
verbatim copies of the code the 2026-10-15 speed lane replaced (eager `closest_on_tri`, the
stack walk, `seg_tri_dist`, the band walk, `seg_face_below`) and compares the live kernel with
them by `to_bits()`, winning face included; `KC_ORACLE_SEGMENTS` sets the segments per mesh,
`KC_STL` adds a binary STL, and `cargo test --profile gate -p geom_kernel --lib
kernel_speed_tests::million -- --ignored` runs a million segments. Keep those copies as they
are: they are the definition of the answer. `geom_kernel/tests/kc_clearance_bench.rs` times
the batched clearance entry points on public API only (`-- --ignored --nocapture`, run it solo
under the CPU lock); its `checksum` must be equal between revisions.

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
- `cargo fmt --all -- --check`: **7 diff locations, in 2 files** (measured first-hand,
  2026-08-29, on `develop@d1f53ea`, a clean worktree). Earlier records here said "104
  hunks," then "120 hunks" — both numbers were real counts at the time, but "hunk" was
  never the right word: `rustfmt --check` prints one `Diff in <file>:<line>:` block per
  differing location, with no `@@` hunk markers at all, so counting those blocks is
  counting locations, not diff hunks. All 7 are pre-existing drift, 6 in
  `crates/plinth-hollow/examples/wall_distribution.rs` and 1 in
  `crates/plinth-hollow/tests/repair_gate.rs`. This is standing debt in already-committed
  code, not a target: do not edit it down opportunistically inside an unrelated change
  (that produces an unreviewable diff), and do not let the count **grow** — a change that
  adds an 8th location is not gate-clean.

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

### Registered constants and format contracts

Some tests pin an exact expectation that is not a claim about where a support is placed at
all — a registered physical constant, a serialization format's exact byte layout, or a
per-fixture **verdict** about whether two specific runs of the algorithm agree with each
other (as opposed to a claim about how good either run's plan is). These are meant to stay
pinned indefinitely, the same way a public API signature is meant to stay pinned. When one
legitimately needs to change, the fix is never to quietly edit the expectation in place:

1. **Append, don't rewrite.** The prior record stays, verbatim, as a dated comment block; a
   new one is appended on top of it. `the_zero_field_divergence_is_measured_per_committed_fixture`
   (`crates/plinth-pipeline/src/lib.rs`) is on its **eighth** such re-record — none of the
   first seven was deleted or edited.
2. **State the attribution as `from → to`.** Name the old value, the new value, and the
   commit that legitimately caused the move. The eighth re-record: the Sphere fixture's
   verdict moved `true → false`, attributed to `a4f5f3f` (`feat(support): re-size a seated
   arm's exemption to its final footprint`, `#36` F1, landed via `#77`), which implements
   Craig's 2026-08-23 ruling that a seated arm's exemption always equals its true footprint.
3. **Measure both ends**, in the same process, same settings, before changing the
   expectation — not argue from the mechanism alone.

**This discipline does not apply to a support plan's own fingerprint** — the digest the
census tool below prints for a corpus model. Under this doctrine a placement-outcome digest
is not a value to maintain and individually re-pin forever; see
[Testing doctrine: three kinds of check](#testing-doctrine-three-kinds-of-check), which also
explains why the older regime that did pin it that way is being retired.

## Testing doctrine: three kinds of check

This section states the owner's 2026-08-29 ruling on how a check on this codebase should
work, and what a contributor does under it. It replaces the older regime of pinning a
support plan's digest as a permanent expectation, described further down.

A **fingerprint** — a single digest number, printed by the census tool below, that
summarizes an entire computed support plan — tells you *that* the output changed. It tells
you nothing about *whether it got better*. Those are different questions, and a check that
cannot tell them apart fails a genuine improvement and a silent regression identically. Since
almost every real improvement to how supports get placed changes the output on purpose, a
check built only around "did this change" ends up blocking exactly the work this project
most needs to do.

Every check in this codebase is one of three kinds. Each kind has its own lifetime — how
long it should stand unchanged, and what it takes to change it — and mixing them up is the
mistake to avoid: do not argue about a moved hash until you know which of the three you are
looking at.

### 1. Invariants — permanent, never refreshed

An invariant is a property true of **any** acceptable support plan, no matter where its
supports are placed. Some exist today; others are named here because they should exist and
do not yet. Examples:

- Every emitted contact (a support tip actually touching the model) satisfies the measured
  area law: `pi * r^2 >= carried_area / X_ALLOWED`, where `r` is the tip's radius,
  `carried_area` is the model area it is holding up, and `X_ALLOWED` (currently `12`, a
  registered constant) is the safety allowance under the field-measured failure ratio of
  about 25 (`docs/physics.md`).
- No demand-bearing contact — one actually carrying model area — is left unrouted.
- Every overhang island (a contiguous patch of the model's surface that needs support under
  it while the resin cures) is supported by something.
- Every unit of the model's load lands on exactly one support: never zero, never
  double-counted.
- No support intersects the model except at its own seat (the point where it touches).
- There is a closed load path from every support down to the build plate or to a model
  surface — never a support ending in mid-air.

**A broken invariant is always a bug.** No argument reinstates a failing invariant, and no
change may narrow, loosen, or delete one to make its own output pass — that turns a real
defect into an accepted one. An invariant only ever changes as part of a physics-model
change carrying the owner's ruling, exactly as changing `X_ALLOWED` itself would.

### 2. Quality ratchets — a recorded number that may only improve

A ratchet is a single measured number, recorded per corpus model (the fixed test meshes
under `corpus/models/`, identified by their SHA-256 digest — see below), that says how
**good** a support plan is, not merely whether it changed. A change may improve a ratchet
freely and silently — that is the iteration loop this project exists to run. Making one
worse requires a recorded argument in the same commit, because "the number changed" and "the
number changed for a stated, accepted reason" are different facts, and only the second is
allowed to ship. Improving a ratchet updates the recorded value in the same commit that earns
it; leaving the old, looser number on record would understate what the code now does.

**Implementation.** The comparison and the tolerance rule below are computed entirely from
evidence the public `plinth_support` API already discloses — `EmittedContactDisclosure`,
`FinalLiveMemberEvidence`, `CoverageMeasurement`, `OwnerlessIslandDisclosure` — with no new
field added to any production type. The harness lives in
`crates/plinth-support/tests/ratchet_harness.rs`; recorded values live beside it in
`crates/plinth-support/tests/fixtures/support_ratchets.json`, keyed to each corpus model's
own STL SHA-256 digest. To record an improvement, re-run with `PLINTH_RATCHET_UPDATE=1`; to
record a worsening that the "physics change" row below actually permits, re-run with
`PLINTH_RATCHET_ACCEPT_REGRESSION=1`. Either way the new value and the reason land in the
same commit — never a separate follow-up.

**The six adopted ratchets.** For each: what it measures, why a print cares, where the
underlying data comes from, how it can be gamed, and what stops the gaming.

1. **Worst tip area ratio** (mm² carried ÷ mm² tip cross-section; lower is better; take the
   **maximum** across all contacts, never the mean). This is the area law from "Invariants"
   above, turned into a number a plan can be graded on: a tip carrying more area than its
   cross-section can hold
   snaps under peel, and the print fails at its single narrowest section — an average would
   hide the one tip that snaps. Folded from per-contact data in
   `EmittedContactDisclosure` (`crates/plinth-support/src/runner.rs`).
   *Gameable by:* fattening every tip (ratchet 2 rises to catch it)
   or dropping the worst-loaded contact (blocked by the "no demand-bearing contact left
   unrouted" invariant, plus a zero-abandoned-load invariant).
2. **Total model-facing scar area** (mm²; lower is better; sum over all contacts). Every tip
   leaves a mark that has to be cut and sanded off by hand; on a face-sized miniature that is
   the difference between usable and ruined. Folded from the same source data as ratchet 1
   (`tip_radius_mm`). *Gameable by:* shrinking every tip
   (ratchet 1 rises) or placing fewer contacts (coverage ratchets 4–5 rise).
3. **Support resin volume** (mL; lower is better; sum of every live member's frustum
   volume). Resin spent on scaffolding is resin not in the model, plus print time and
   cleanup, and the saving multiplies across a multi-plate run. Folded from the per-member
   `frustum_volume_ml` (`crates/plinth-support/src/member_sizing.rs`), reached through
   `SupportRunSnapshot::final_live_members()` (`runner.rs`).
   *Gameable by:* thinning or shortening the scaffold until it cannot carry the model — the
   area-law invariant and the closed-load-path invariant both catch a tree that saves volume
   by failing to carry its load or never reaching the plate.
4. **Residual uncovered area at seal** (mm²; lower is better; sum over remaining patches). A
   "seal" is a plan the coverage-repair controller has accepted as finished; the small
   allowance it may still leave uncovered is bounded by a registered "censor" area, judged
   too small to sag or drag on the print film. Folded from the coverage report's
   `CoverageMeasurementPatchEvidence::uncovered_area_mm2`
   (`crates/plinth-support/src/coverage_measurement.rs`).
   *Gameable by:* loosening the censor so patches stop being counted — blocked because the
   sealed model is always re-measured against the *registered* censor independently of what
   the run under test claims (an existing invariant), so a censor change shows up as a
   deliberate re-baseline of that invariant, never as a ratchet improvement.
5. **Worst uncovered gap** (mm; lower is better; maximum over remaining patches). Sag scales
   with unsupported *span*, not unsupported *area*: one long thin uncovered strip and a
   scatter of dots can share the same total area and behave completely differently. Folded
   from `CoverageMeasurementPatchEvidence::worst_gap` (same file), recorded per patch.
   *Gameable by:* splitting one long strip into shorter ones with a
   single added contact — ratchet 4's total is unchanged by the split, and ratchets 2–3
   charge for the added contact, so the pair together says whether coverage actually improved.
6. **Ownerless model area** (mm²; lower is better; sum over disclosed islands). Model area
   the load repartition left with no support officially responsible for it — the area most
   likely to be found unsupported when a print fails. Folded from the per-island
   `OwnerlessIslandDisclosure::ownerless_face_area_mm2`, reachable through
   `SupportRunAuthority::ownerless_islands()` (`runner.rs`).
   *Gameable by:* declining to disclose a stranded island at all — blocked by two existing
   invariants: every ownerless island must be disclosed with finite, positive numbers, and
   the coverage measurement must always be allowed to run and rule rather than being
   pre-empted by an ownership tally.

**Rejected candidates**, kept here so they are not proposed again:

- *Support or contact count* — improves by deleting supports until the model falls off the
  plate, and it is seeded from per-vertex local surface minima, so it moves under a mesh
  re-tessellation with no change in plan quality.
- *Snapshot digest* — has no direction; a digest is only equal or not, which is exactly why
  it is an equivalence check (below) rather than a ratchet.
- *Coverage repair round count* — a compute-cost proxy, not a plate fact; whatever a repair
  round added is already charged to ratchets 2 and 3.
- *Abandoned contact count* — a count of refusals has no direction; refusing a contact that
  carries no load is free and correct. The quantity that matters, abandoned *carried area*,
  is already pinned at zero by an invariant.
- *Resource ceilings* (route memberships, indexed points, distance-work budget, peak
  memory) — real, but a budget fact rather than a print-quality fact; performance-lane
  territory.
- *Mean tip area ratio* — an average hides the one tip that snaps; the failure mode is the
  narrowest section, so the metric must be the maximum (ratchet 1).
- *Member count or total member length* — proxies for resin volume that move with how the
  support tree branches without any resin actually changing; ratchet 3 measures the real
  quantity directly.

**Tolerance rule.** An improvement of any size passes. A worsening fails unless it fits the
"physics change" row of the table below, and then only with the new value and the argument
recorded in the same commit.

- Integer-valued ratchets (none of the six above, but a future one might be) tolerate no
  drift at all — there is no floating-point wobble in a count.
- The six floating-point ratchets above fail if a value worsens by more than
  `max(1e-12 × recorded_value, 1e-9 in the metric's own unit)`.
  - The **relative** term, `1e-12`, is a naive-summation error bound: summing `n` terms in a
    fixed order accumulates worst-case relative error of about `n × eps`, with `f64`'s own
    precision `eps = 2.220446e-16`. The largest fold in the corpus runs over the Weeping
    Treant's contact and member sets, on the order of 10³ items; taking `4096` as headroom
    gives `4096 × 2.220446e-16 ≈ 9.1e-13`, rounded up to `1e-12`. A change larger than that
    is not summation noise, it is a real movement. Ratchets 1 and 5 are maximums rather than
    sums, so `1e-12` is generously loose for them.
  - The **absolute** term, `1e-9` (in mm², mL, or mm, matching the metric), exists because a
    relative tolerance means nothing near zero, and ratchets 4 and 6 are legitimately zero on
    a good plan. `1e-9 mm²` sits six orders of magnitude below the printer's own resolvable
    area (one 34.4 µm pixel is about `1.2e-3 mm²`), so a movement that small cannot
    correspond to anything that happens on the plate.
  - Both terms assume a **fixed summation order**, which the run already guarantees: the
    existing thread-sweep determinism check proves byte-identical output at 1, 4, and 16
    threads. That check must keep running for this tolerance rule to mean anything.
- **Ratchets are keyed to the corpus model's SHA-256 digest**, the same digests already
  pinned in `crates/plinth-decimate/tests/corpus_budgets.rs`. Contact candidates are seeded
  from per-vertex local surface minima, so a re-tessellated mesh yields a different candidate
  set and a plan of equal quality that scores differently — a corpus STL that changes
  re-baselines every ratchet recorded against it, and comparing a ratchet across two
  different meshes compares nothing.

### 3. Equivalence checks — a claim about one specific change

**A fingerprint is a claim about a specific change, not a permanent property of the
system.** Use one when the claim is "this change did not alter what supports get placed" —
a speed-up, a refactor, a caching change — and prove it by running the real pipeline before
and after and comparing the output.

The worked example is the 2026-08-28 incremental-digest change
(`docs/evidence/remediation-2026-08-26/incremental-integrity.md`): six fixtures run through
both the old and the new code, in the same process and the same settings; every digest,
refusal outcome, trial count, and commit count compared; the full before/after table recorded
in the ruling that makes the claim, including which fixture did **not** finish and was
therefore "untested, not unchanged" rather than silently counted as a pass. That is the shape
to imitate: control against change, every fixture named, the result recorded where the claim
is made.

An equivalence check is run **at the time of the change it is proving**, not carried forward
as a standing assertion every later commit must keep satisfying — the next placement
improvement is expected to move the same digest, and that is a different claim, not a
failure of this one. What stays permanent is the mechanism it is built on: the
same-input-same-output determinism guarantee (below), plus the invariants and ratchets that
keep policing plan quality regardless of what any one equivalence check once proved.

### Classify the change, then the regime follows

A contributor classifies what the change claims; the test regime follows from the row, not
from an argument about whether a particular hash "should" have moved.

| Change | Fingerprints | Ratchets | Invariants |
|---|---|---|---|
| Better placement | expected to move — no argument needed | must not worsen without a recorded argument | must hold |
| Performance / refactor (no placement change claimed) | must **not** move — proved by a before/after equivalence run made at the time | unchanged | must hold |
| Physics change (the load model or a physical constant changes, by owner ruling) | expected to move | may move, argued, by owner ruling | may change, only by owner ruling |

### Why a pinned support-plan digest is not an invariant

Earlier in this project, a corpus model's support-plan digest was pinned as a permanent
`assert_eq!`, expected to stay byte-identical forever; any change that moved it had to be
individually re-argued and re-pinned (the append-only `from → to` convention above) before
the test would go green again. That made every placement improvement expensive in direct
proportion to how many fixtures it touched, with no way to say "this moved and that is fine"
short of editing a test file. **A pinned digest found in history or on a stale branch is not
a model to copy.** Existing pins of that shape are being retired under this doctrine, not
extended to new fixtures — a fingerprint should answer only the narrow question one specific
commit is asking, while plan quality is judged by the ratchets and invariants instead.

### What none of this proves

Ratchets and invariants are cheap and run on every commit; a print is not. None of the
numbers above is a claim that a plan actually works in resin — a ratchet can improve and
every invariant can hold while a plan still fails on the plate for a reason nobody has
measured yet. What this doctrine buys is cheap regression detection, so the owner's limited
printer time answers the question only a real print can answer, instead of re-discovering a
defect this suite already knows how to catch.

## The census tool: determinism and equivalence checks

The engine's central promise is that the same model produces the same output, checked
directly rather than assumed. This is the permanent, standing half of the machinery above: a
**determinism** check compares the same code against itself (same input, different thread
counts, or instrumented against plain); an **equivalence** check (above) compares two
versions of the code against each other. Both use the same tool.

The census harness runs the real support-preparation path over the whole test corpus and
prints, per model, a **snapshot digest** — a single number folded from every routed contact,
seat, and live support member in the sealed plan — along with its routed contact and live
member counts. It lives behind a non-default cargo feature, so census instrumentation can
never reach a production build:

```sh
cargo build --offline --example embed_incidence_census -p plinth-support \
  --features census --profile dev-opt
PLINTH_CENSUS=1 ./target/dev-opt/examples/embed_incidence_census
```

Without `PLINTH_CENSUS=1` the identical binary produces an un-instrumented baseline run, so
an instrumented run can be diffed against it, proving the instrumentation itself
result-inert — a determinism check that is always current and never a stored constant.

**Using it to prove an equivalence claim:** run it at the tip of `develop` (or whatever the
control revision is), run it again on your branch, and diff every digest, count, and refusal
outcome per fixture. A digest that moves on a branch that claims no placement change is the
claim failing, not a test to fix. Run the census whenever you touch anything the support path
can see, including code you believe is inert — that belief is exactly the claim an
equivalence check exists to test.

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

## Build flags and allocator

Measured 2026-10-15 on an Apple M3 Max, 12 threads, untraced, solo (`--timing` lock), Deinonychus
and Bat_Cat_Owl (BCO) in the `perf-2026-10-09` input set, same session, against the plain
`release` binary of the same commit. Every run below produced the reference fingerprints
(first 8 hex digits of the `.chitubox` sha256): Bat_Cat_Owl `5132fb1a`, Cylinder `f442e573`,
Deinonychus `d73742b2`.

| Build | Deinonychus s | BCO s | Cylinder peak RSS | Clean build s |
|---|---|---|---|---|
| `release`, generic aarch64 (base) | 63.6 / 64.2 | 15.9 | 13.71 GB | 102 |
| `release` + `target-cpu=apple-m3` (the default now) | 64.2 | 16.4 | | |
| + mimalloc | 63.5 | 16.0 | 13.57 GB | |
| + jemalloc | 65.0 | 15.4 | 13.41 GB | |
| `release-max` (fat LTO, 1 codegen unit; M3, system allocator) | 64.2 | 15.7 | | 105 |
| PGO on `release-max` (`scripts/build-pgo.sh`, trained on BCO and Deinonychus) | 57.5 / 58.1 | 14.6 | 13.76 GB | 584 (instrumented build, training, final build; shared machine) |

Cylinder is not in the PGO training set: base 96.2 s, PGO 80.9 s (-16 %), same fingerprint.

The run to run spread of one binary is about 0.6 s on Deinonychus (two base runs: 63.6 and
64.2 s) and 0.5 s on BCO. A difference smaller than that is not a result.

- **Target CPU.** `.cargo/config.toml` sets `-C target-cpu=apple-m3` for `aarch64-apple-darwin`
  only, so every build on this Mac gets it and other targets are unchanged. Such a binary needs an
  M3-or-newer-compatible CPU (M3, M4, M5, ...) and stops with an illegal-instruction signal on an
  M1 or M2. For an older Mac set `RUSTFLAGS="-C target-cpu=apple-m1"`: the environment variable
  replaces the file. Never use `target-cpu=native` (it follows the build host). Output bytes do
  not change: Rust does not fuse `a * b + c` into a fused multiply-add unless the code calls
  `mul_add`. Measured gain: none that shows above the spread; the hot code is scalar.
- **Allocator.** The workload allocates a lot (Deinonychus: 386 M allocations, 153 GB, 94 s with
  the counting build) but about 6 M per second over 12 threads, so allocator cost is a few
  percent of the CPU time at most. `mimalloc` and `jemalloc` (`tikv-jemallocator`) changed user
  time by about -1.5 % and wall time by nothing measurable, and mimalloc raised Deinonychus peak
  RSS from 6.3 to 7.1 GB. The system allocator stays the default. The `mimalloc` feature of
  `plinth-cli` stays as an opt-in for a later retest (`cargo build --release -p plinth-cli
  --features mimalloc`); jemalloc was removed. Cylinder peak RSS (13.4 to 13.7 GB) is the same
  under all three, below the 20 GB mark.
- **Where the allocations are.** Build the counting binary and read its sites:
  `scripts/alloc-profile.sh build`, then `scripts/alloc-profile.sh run BIN OUTDIR -- prepare ...`,
  then `scripts/alloc-profile.sh report BIN OUTDIR/sites.tsv [FIRST_WINDOW LAST_WINDOW]`
  (feature `alloc-count`; never time that binary). On Deinonychus the top small-allocation sites
  are the lip-seat passes (`LipSeatRun::run`, `lip_seat::apply`, `trial_arm`, `propose`; 52 % of
  all allocations together), `slenderness::new_plane_holds` (10 %, 64 B each), `finite_array`
  (7 %, 32 B each), `NativeSurface::cast_rays_classified` (6 % on Deinonychus, 29 % on BCO: one
  `Vec<f64>` and one `Vec<i64>` per ray), `orient_layers_batch` (3 %) and
  `NativeSurface::collect_hits_with_stack` (3 %, 15 % on BCO). The sites that move the most bytes
  are `lip_seat::apply`, `propose` and `trial_arm` (33 % of all bytes), `tip_landing::seat_faces`
  (6 %, 0.9 MB each) and `PointCloudIndex::radius_neighbors_batch` (5.5 %, 5.6 MB each). They are
  the candidates for reused buffers or arenas; the allocator is not the lever.
- **Fat LTO.** `cargo build --profile release-max -p plinth-cli` (fat LTO, one codegen unit;
  binary in `target/release-max/plinth`). It built in the same time as `release` here (105 s
  against 102 s; both clean, shared machine) and gained nothing that shows (Deinonychus user
  time -1 %). It is opt-in: it does no harm, and `scripts/build-pgo.sh` uses it as its base.
  Check it again when the hot code changes shape, not on every build.
- **Profile-guided optimization (PGO).** The one flag that pays. `scripts/build-pgo.sh --train DIR
  [--train DIR ...]` builds an instrumented binary, runs the training models, merges the
  profiles and rebuilds with `-Cprofile-use`; it needs `llvm-profdata` (rustup `llvm-tools`).
  Trained on Bat_Cat_Owl and Deinonychus it retires 20 % fewer instructions on Deinonychus
  (3042 G to 2431 G) and uses 7 % fewer cycles; wall time falls 9 % on Deinonychus (second
  contact 31.6 to 28.0 s, production 25.3 to 22.9 s) and 8 % on BCO, and 16 % on Cylinder, which
  the profile never saw. Output is byte-identical. Not the default build (a ten minute build
  against under two); use it for a delivery run, and rebuild the profile after the hot code
  changes much. Wrap the build in the CPU lock as one shared job. The script only reads the
  `--train` folders (training output goes to a scratch folder inside the run's own profile
  directory). Each run makes a fresh `<pgo-data>/run.XXXXXX` folder (default
  `<target-dir>/pgo`), holds the merged profile there, and deletes nothing it did not create
  itself; remove old `run.*` folders by hand. Training must succeed: every `--train` folder
  must exit 0 (so it must hold only models that emit; exit 3 is not accepted, because a gate
  refusal hides other models' failures in the exit code) and leave a new `.profraw` file; any
  other status stops the script. **Compiler flags**, by cargo's precedence (a variable that is
  set counts even when empty): `CARGO_ENCODED_RUSTFLAGS` (elements split on `\x1f`), else
  `RUSTFLAGS` (split on whitespace; `RUSTFLAGS=''` means no flags and no config fallback; for an
  older Mac: `RUSTFLAGS="-C target-cpu=apple-m1" scripts/build-pgo.sh --train DIR`), else
  `target-cpu` from `.cargo/config.toml`. The script appends the `-Cprofile-generate` or
  `-Cprofile-use` flag (absolute path) as one element and passes cargo the list in
  `CARGO_ENCODED_RUSTFLAGS`, so a path with a space is safe. `--dry-run` prints the flags, one
  bracket per argument.
- **Feature combinations.** `mimalloc` and `alloc-count` each install a global allocator in
  `plinth-cli`'s binary; with both on (`--all-features`) `alloc-count` wins, so the all-features
  clippy and test gate compiles.
- **Where the time goes (Deinonychus).** Instructions and cycles come from the kernel's
  per-process counters (`proc_pid_rusage`, read every 0.5 s); the issue-slot split comes from
  Instruments "CPU Counters" (`xctrace record --template 'CPU Counters' --attach PID`, 15 s from
  the start of the second contact stage; a whole-run trace of Deinonychus is 37 GB and
  `xctrace` crashes saving it). Second contact (31.7 to 63.2 s of 64 s) retires 798 G
  instructions in 196 G cycles (IPC 4.07) but keeps only 1.6 performance-core equivalents busy
  (46 s user and 9.6 s system CPU time in a 31.6 s stage). Production: IPC 4.25 on 3.9 cores.
  Prefilter: IPC 3.3 on 6.4 cores. An IPC of 3.4 to 4.3 is high for this core, so the code is
  not waiting on memory. The issue-slot split of the second contact stage (measured on the PGO
  build) is back-end processing 52 %, useful work 22 %, work discarded by mispredicted branches 19 %, instruction
  delivery (front end) 6.5 %: **compute (back-end) bound, with mispredicts costing about a
  fifth of the slots, no front-end or memory wall.** Separate L1D and L2 miss rates and branch
  mispredict counts need a custom counter set that only the Instruments application can save,
  and were not measured. The binding fact is concurrency, not code generation: 31 s of the
  64 s run use less than two cores. A change in how much of the second contact stage runs in
  parallel is worth more than any compiler flag in the table.

## Measuring work: `--perf DIR`

A speed change is judged by exact counts of the work it removes, because a wall-clock gain
below about 5 % does not show above the noise. `plinth prepare … --perf DIR` (or
`PLINTH_PERF_DIR=DIR` when the flag is absent) writes `DIR/<model>.perf.json`
(`plinth-perf/1`): per phase (`Load`, `Repair`, `Decimate`, `OrientTable`, `Prefilter`,
`Production`, `Hollow`, `SealFields`, `FinalSweep`, `Emit`) the wall time, the counters and the
span times, plus `totals`. The file is rewritten at every phase change and every
`PLINTH_PERF_CHECKPOINT_S` seconds (default 60, by one plain checkpoint thread) with
`"partial": true`, so a killed run still says where it was and what it had finished, and
written last with `"partial": false`. A span still running at a checkpoint shows as `open`
and `open_ms` (approximate, never compared); the final file has none. The file name keeps the
model's name (`A B` gives `A B.perf.json`, distinct from `A_B.perf.json`); only characters a
file name cannot hold, and `%`, become `%XX`. It sits outside the model's output folder, so no
digest sees it.

- **Write-only.** `geom_kernel::perf` has no read API; no engine code can branch on a count or
  a clock. With the flag off, a charge is one `Relaxed` load. The emitted `.chitubox` is
  byte-identical with `--perf` off and on; check it when you touch a charge.
- **Batch-first.** A kernel counts into local integers and charges once per query, call or
  batch, never inside a per-triangle or per-cell loop. A brute-force scan is charged as
  `rows * faces` before its `par_iter`.
- **Classes.** `work` counters are exact logical work and must be equal at every thread
  count; `sched` counters (`frame.*.speculative`: the deferred-pair batch is sized by the
  thread count) are recorded and never compared. A `work` counter that differs between 4 and
  16 threads is a defect.
- **Adding a counter.** Append one line to `works!` in `rust/geom_kernel/src/perf.rs` and call
  `perf::add` from the file that owns the site (`plinth_sweep::perf` re-exports it for
  `plinth-pipeline`). `crates/plinth-sweep/tests/it/perf_counters.rs` runs a real sweep at 1
  and 8 threads and compares the sidecars.
- **Light mode: `PLINTH_PERF_LIGHT=1`** beside `--perf`. Counters, spans, pose frames, stage
  marks and the light row kinds stay on; the per-batch and per-layer trace (the `clear.batch`,
  `down.*`, `tip.batch`, `brace.partner_search` and `lip.round` rows, the reuse probe's
  hashing, `perf.count_overhead`) and the pose-search diagnostics are off, so the stage and
  pose times read at near-untraced speed (a full `--perf` run overstates the sizing about
  2.7x). `perf::enabled()` is the full-trace gate; `perf::on()` is light-or-full. The sidecar
  says `"light": true`.
- **P2 design measurements (S0, 2026-10-15).** Row kinds `s0.lip.apply`, `s0.lip.apply_b`,
  `s0.lip.apply_open` (one each per `lip_seat::apply` call: jobs, rounds, commits, the driver
  thread's time by part, the stored path tests' staleness today and under a per-cell /
  per-member read set, boundary-2 refusals by kind, rounds by open jobs) and
  `s0.removal.attempt` (one per removal attempt) are stored in light mode; `s0.tie.pass` (the
  dry conflict probe of a tie judge/commit split) and `s0.member.print` (per member at nine
  stage marks, to diff a second-contact re-plan against the winner) only in a full trace. The
  span `s0.finish_after_publish` times what a chooser early exit would skip. The columns are
  documented on `Rows::cols`.
- **Lip-seat deep scan (S2, 2026-10-15).** `lip_seat::apply` settles ahead the rounds in which
  no job's proposal passes the clearance test (`scan_open_jobs`: path tests and trial solids on
  the pool, the clearance reads in kernel batches, clear-site `lip.scan`); the row kind
  `s2.lip.scan` (light) counts per call the scans, the rounds they settled, the candidates,
  reads, stops and the scans' driver time. `PLINTH_SWEEP_LIPSEAT_SCAN=0` turns the scan off (a
  diagnostic: the output does not depend on it); `PLINTH_SWEEP_VERIFY_LIPSEAT=1` re-asks every
  verdict the scan settles one candidate at a time and panics on a difference.
  `PLINTH_S2_MEMO_PROBE=1` (with `--perf`) counts the movers' clearance reads by their exact
  kernel inputs and bound, and how many repeat within the call and within the sweep (the
  `probe_*` columns): the prize an exact read memo could take.

## Pose-search diagnostics: `DIR/<model>.orient.json`

With `--perf` on, each pose search also writes `DIR/<model>.orient.json`: `{"record": …,
"diagnostics": …}`. `record` is the model's `DenseSweepRecord`. `diagnostics`
(`plinth_orient::diag::SweepDiagnostics`) is a separate value that records every discrete
decision of the funnel, with the margin to its boundary, so a pose change can be traced to
the stage that made it. It decides nothing.

- **Identity.** `DenseSweepRecord` and `RankedCandidate` do not change, so `search_digest`
  (a hash of the whole serialized record) and the emitted `.chitubox` are identical with the
  diagnostics on and off. Provenance keys (`legacy:K`, `rest:BIN`, `fib:I`) exist only in the
  diagnostics: a candidate id is a position and moves when an earlier direction is dropped; a
  key does not.
- **What it holds.** The id-to-key map. G1 (face to reference bin): a SHA-256 of the whole
  face-to-bin vector and the 256 faces with the smallest gap between best and second bin.
  G2 (heaviest 128 bins): the bins either side of the cut and their area gap. G3 (dedupe):
  for every direction the grid tried, the earliest earlier candidate that covers it and the
  nearest earlier one, with the angle and dot margin to the radius. G4 (flat-face escort):
  per rest pose the flat share and a hash of its flat-face set, the 256 smallest facing
  margins, and the candidates covering each flat pose. The escort's decisions. C1 to C4:
  K, the rows either side of the cut and the next 8 rejected. Each row says whether its
  burden was measured at the production raster; the commit comparator puts production rows
  first before it reads a score, so a cut carries `score_gap` only when both rows are in one
  class (`decided_by: "score"`), and `decided_by: "production_fidelity"` with no gap when
  precedence decided. A certified runner-up (below) keeps its certificate on its row and
  gives `decided_by: "certified_bound"`, with `score_gap` its lower bound minus the winner's
  score. Three full copies of the commit ranking (exact table, after the
  prefilter, after production), because the record reprices it in place.
- **Certified losers (perf lane B1).** The finalists' production sweeps run as a race: when
  a finished pose scores strictly below another pose's lower bound, that pose cannot win or
  tie, and its sweep is stopped at its next safe point (`plinth_sweep::cancel`). The lower
  bound is the full score's own arithmetic with the three measured terms (burden, landing
  scar, tip scar) set to zero (`TermsRecord::production_lower_bound`); each is a sum of
  areas times non-negative field values, so the bound is never above the real score. A
  stopped pose has NO production score, and the record says so: its `RankedCandidate` and
  its `commit_tier_demand` row carry `certified_loser: {lower_bound_score}`, the row stays at
  `fidelity: "prefilter"`, and `terms` is the price before production. The set is recomputed
  after the batch from the winner's score alone (every finalist whose bound is strictly above
  it, whether its sweep stopped, finished or failed), so the record, the digest and the
  winner do not depend on thread count or timing; only WHEN a loser stopped does, and that
  is on stderr (`[orient] certified loser: …`), not in the record. Readers of a commit-tier
  score check `certified_loser` first: `runner_up` may be a certified loser (then
  `fidelity.winner_runner_up_margin` is `null` and `winner_runner_up_margin_lower_bound`
  is set), `weight_flips` is empty for such a runner-up, and `peel_robustness` lists the
  corners a certified loser might still have won in `undetermined_corners` (a corner it
  could not have won is decided as before). A search that certified nothing serializes
  exactly as before, `search_digest` included.
- **Cost.** G1 and G2 are read off the grid's own binning pass through an observer, not
  rescanned; with the diagnostics off that pass carries no observer. The collection and the
  `orient.json` write run in the sidecar's `Diagnostic` phase (the `OrientTable` phase
  excludes them), and `prod-all`/`extra=` work counters land there too.
- **`PLINTH_ORIENT_DIAG`** (comma-separated; unset means on when `--perf` is on):
  `off` collects nothing; `digest` prints `[orient] search_digest <sha256>` to stderr, with
  no `--perf` needed (the check that the digest did not move); `prod-all` also measures every
  commit-admitted pose at production pitch; `extra=X,Y,Z;X,Y,Z` (last word) scores and
  measures these directions the same way. The extra measurements run after the record is
  final, through a separate non-retaining measurer
  (`SupportDemandMeasurer::diagnostic_measurer`), never the retaining one, and are written to
  the diagnostics only. They add to the perf counters, so such a run is not a timing run.
  `extra=` directions are scored on the exact table first; with no separate measurer the
  rows keep those scores, with `production_terms: null` and the reason in `error`.
- **`PLINTH_ORIENT_ABLATE`** (comma-separated; unset means every word off; pose-stability Lanes A and C).
  An ablation measures one change at a time on one binary and is deleted when the change is
  promoted or dropped. With every word off the search, the record, `search_digest` and the
  emitted bytes are the recorded ones. `exact-grid` builds the candidate grid from the EXACT
  table instead of the coarse proxy's and lists the rest poses in ascending reference-bin
  order instead of heaviest-first (the kept bins are the same rule; only the proxy
  collapse order, which follows the input's last float bits, and the area order are gone);
  the coarse tier still scores on its proxy. It CHANGES the search: use `--no-cache`, and do
  not read its output as a production run. `exact-shadow` is out of band: after the record
  is final it scores the SAME C1 and C2 input key sets on the exact table and repeats each
  cut, into `orient.json` (`diagnostics.exact_shadow`, `C1x`/`C2x`); it needs `--perf` and
  changes nothing. `exact-score` (Lane C) retires the proxy tiers: the coarse, refine and
  commit tiers all score on the exact table and the cuts keep their widths. With `exact-grid`
  too, the L1 and L2 proxy meshes are not built at all; alone, the proxy rung is still built
  because the grid reads it. It CHANGES the search: use `--no-cache`.

## Surface fields: parallel walks, serial oracle

`plinth-mesh` computes two per-face fields with a surface walk per vertex or face: delicacy's
curvature (`delicacy_eval::curvature_by_face`) and the display field's neighbourhood pass
(`display_eval::neighbourhood_pass`). Both run in parallel inside `geom_kernel::run_in_pool`
and give the same bits at every thread count.

- **Curvature** is a scatter (`charge[face] += value * inside`), so the serial addition
  order is part of the answer. Fixed vertex blocks (`surface_par.rs`: 512 vertices, 32 blocks
  per wave) each return their `(face, value * inside)` pairs in walk order; a serial replay
  applies the blocks in ascending order. A block that reaches its pair cap stops after a
  finished vertex and its remainder is replayed in place, so the cap changes only when an
  addend is computed, never the addend or its place.
- **The protected proxy (default for the orientation table; `--exact-delicacy` opts out).**
  `plinth prepare` evaluates the orientation table's delicacy curvature with
  `DelicacyEvaluation::ProtectedProxy` unless `--exact-delicacy` is given (`PrepareRequest::exact_delicacy`,
  `SweepRequest::delicacy`; `OrientFaceTable::build` and `evaluate_delicacy` stay exact). A mesh with
  fewer than `PROXY_MIN_CURVED_VERTICES` (20 000) curved welded vertices is walked exactly. On a larger
  mesh `protected_proxy` keeps every vertex whose normalized angle defect is at least 0.1 as its own walk,
  and merges the rest per cell of `radius / 4` (0.375 mm) AND per facing (one of six axis classes of the
  area-weighted vertex normal, so the two sides of a thin sheet never merge) AND per locally connected
  piece (union-find over the mesh edges whose both endpoints are members of the same cell-and-facing
  group, so two nearby fins that are not edge-connected inside the cell keep one walk each; a global
  component label would not do, since two members of one component can be linked only by a remote
  path). A piece is spread once, from the member nearest its value-weighted centre, with the SUM of its
  members' values, so curvature mass is conserved. `SweepRequest::registered()` defaults to the proxy;
  a diagnostic that compares against the exact field sets `request.delicacy = DelicacyEvaluation::Exact`. The walk and the replay are the exact path's, over fewer items, so the field is
  still bit-identical at every thread count; it is not equal to the exact field, so the chosen pose can
  differ on a dense model. The thinness proxy and the display field are untouched (the display pass is a
  per-face gather, not a vertex scatter, so the merge does not apply to it).
  `examples/delicacy_proxy_probe.rs` prints the exact-versus-proxy difference on STL files.
- **The display pass** is a gather (face `f` writes only its own slots), so fixed 1024-face
  blocks are concatenated in block order. Scratch is pooled per worker, and the visited stamps
  are keyed by the global face index, which is unique, so stale stamps never collide.
- **Oracle.** `curvature_by_face_serial` and `neighbourhood_pass_serial` are the walks as they
  were before the split. The unit tests compare the parallel code against them bit for bit
  (pools of 1, 3, 4, 8, 16 workers; block lengths and caps that force the overflow path) and
  run a sabotage for each hazard. `examples/surface_field_witness.rs` (feature `serial-oracle`)
  does the same on real STL files; run it under the CPU lock:

  ```sh
  PLINTH_WITNESS_WIDTHS="1 4 16" cargo run --release --offline -p plinth-mesh \
      --features serial-oracle --example surface_field_witness -- model.stl
  ```
- **Counters** (`--perf`): `delicacy.blocks`, `delicacy.waves`, `delicacy.overflow_splits`,
  `delicacy.faces_multiply_charged` (faces with two or more addends, the only faces where order
  can matter), `display.faces` (walks) and `display.members_total` (always larger when any
  walk expanded).
- **The sweep placement computes no seal field.** `prepare` forks to `sweep_and_emit` before
  `prepared_delicacy_field`: nothing on the sweep path reads that field, so the legacy
  placement alone pays for it (phase `SealFields`). A mesh whose seal field cannot be measured
  therefore no longer refuses a sweep run.

## Corpus provisioning

The test models are inputs, not source, and are not stored in git — the largest is over
20 MB. A fresh worktree therefore starts with an **empty** `corpus/models/`, because
`.gitignore` keeps the STLs out of git. The `post-checkout` hook clones them in from the main
checkout when it creates a worktree (see "The test loop"). For a worktree made before the hook, or
by hand, link them in from a checkout that has them:

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
(`crates/plinth-orient/tests/it/orientation_rederivation.rs`), which falsifies every pinned
baseline orientation by re-deriving it from the corpus source STL. If those are the only
failures in a fresh checkout, provision before investigating anything else.
`plinth-orient`'s `as_drawn_corpus` tests also read the provisioned sources: they regenerate
every committed `corpus/as-drawn/` file from its source and compare byte for byte.

`corpus/as-drawn/` IS committed: the test models the owner wants used exactly as drawn, each
with its `<stem>.asdrawn` marker (git keeps no Finder tag). They are made only from the recipe
`corpus/as-drawn/sources.tsv`, never by hand:
`cargo run --profile gate --offline -p plinth-mesh --example as_drawn_corpus` writes them, and
`-- --check` verifies them (`corpus/as-drawn/README.md`).

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
  added as a **dated addendum beside** the original rather than an edit to it (the
  registered-value append-only discipline above is the same idea applied to a pinned test).
  Rewriting a record to match what turned out to be true destroys the only evidence of what
  was believed at the time, and with it the ability to audit a decision.
- **Branching is git flow.** `develop` is the integration base and every change starts as a
  `feature/*` branch cut from it, merging back with `--no-ff` so the shape of the work
  survives in history. `main` is release-only. Nothing is pushed without the maintainer's
  say-so.
