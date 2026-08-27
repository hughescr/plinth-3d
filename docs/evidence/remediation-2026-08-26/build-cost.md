# Build & test wall-clock cost — measured, plinth-3d, 2026-08-26

Machine: 16-core Apple Silicon, 128 GB RAM, rustc/cargo 1.97.1, sccache as `rustc-wrapper`.
All numbers are wall-clock unless marked "user" (summed CPU across threads). Method notes
and raw logs are in this directory (`timing-*.txt`, `timing-raw*/`, `opt*-build.log`).

## Headline

The test suite, not the build, is the iteration bottleneck — already established (184.4s
test run vs 4.15s incremental build) — and every measurement below reinforces it. Inside
the suite, one architectural defect, already flagged as REVIEW.md finding #12 but there
marked "nothing was profiled, so this is the last item on the list rather than a measured
hot spot", turns out on measurement to be the dominant cost by a wide margin: the
coverage-repair controller takes one full, unindexed measurement per trial and the trial
count is uncapped. It now has a measured hot-spot ranking — first. The other two items
this task asked about (the build-script directory watch, the test opt-level) are real,
measured effects, but small, and in one case pointing the opposite direction from the
hypothesis.

---

## 1. Where the 184s test suite goes — the ten most expensive tests

Per-binary and per-test timings taken with `-Z unstable-options --report-time`
(RUSTC_BOOTSTRAP=1; stable toolchain, this flag is otherwise nightly-only) against the
workspace's 87 test binaries, at the configured `opt-level = 2`. One binary
(`support_runner_integration`) was capped at 60s in this run because its cost is already
characterized in the task brief (874s uncapped, 213 trials, 210 of which improved
nothing) — re-running it to completion here would not have added information, only
~14 minutes of wall time.

| Rank | Test | Time | Binary |
|---|---|---|---|
| 1 | `public_runner_closes_overhang_table_only_after_appending_repair_contacts` | **≈874s** (measured previously; capped at 60s here) | `support_runner_integration` |
| 2 | `every_failure_disposition_clears_an_earlier_runs_artifact_at_its_target` | 31.15s | `prepare_folder` |
| 3 | `a_refusal_removes_an_earlier_runs_artifact_at_the_same_target` | 31.13s | `prepare_folder` |
| 4 | `a_coverage_closure_refusal_is_not_masked_by_an_earlier_load_failure` | 31.13s | `prepare_folder` |
| 5 | `a_sealed_run_refused_model_writes_no_artifact_and_exits_five` | 30.51s | `prepare_folder` |
| 6 | `the_raking_blade_refuses_with_a_measured_class_per_route` | 28.19s | `a4_transactional_controller` (plinth-support) |
| 7 | `attempt_zero_names_every_stage_it_enters_and_the_trials_stay_quiet` | 28.19s | `a4_transactional_controller` (plinth-support) |
| 8 | `procedural_raking_blade_is_refused_before_any_file_exists` | 26.76s | `first_artifact` (plinth-pipeline) |
| 9 | `every_pinned_orientation_record_rederives_its_committed_oriented_solid_artifact` | 17.88s | `orientation_rederivation` |
| 10 | `corpus_budgets` (the real-corpus decimation search) | 17.69s | `corpus_budgets` |

(`two_invocations_over_one_folder_print_byte_identical_reports` in the `process` binary
measures at 12.50s in isolation — an earlier draft of this table showed 91s for it, which
was measurement contamination from an unrelated `cargo build` running concurrently on
this machine; re-measured in isolation and corrected. This is noted here because it is a
real trap: never trust a test-binary timing taken while anything else is compiling.)

**Six of the top ten trace to one root cause.** Ranks 1 and 6–8 all drive the same
architectural path — the coverage-repair controller — on different fixtures
(`make_raking_blade`, `hollowed_overhang_table` / `make_overhang_table`). Ranks 2–5 and
the rest of `prepare_folder`'s slow tests are full `plinth prepare` CLI round-trips that,
for the failing-refusal fixtures, also route through the repair controller before
refusing. Read together, roughly **190 of the ~330 seconds** of per-binary run time in
this measurement (excluding the capped outlier) is repair-controller cost, and the
excluded outlier alone is **874s**, more than the rest of the suite combined.

**What the other four of the top ten are doing — real work, not a defect:**
- `corpus_budgets` (rank 10) runs a real bisection search for a decimation budget against
  actual corpus STLs (e.g. `Tavey Base.stl`, 73,582 faces) and validates six topology
  invariants at each step. This is inherent cost of testing against real geometry, not
  test-suite waste.
- `every_pinned_orientation_record_rederives_its_committed_oriented_solid_artifact`
  (rank 9) rederives every pinned orientation baseline across the corpus — same category.
- `finalized_transcript_thread_sweep_is_exactly_identical` and
  `actual_one_four_sixteen_thread_complete_lowered_lineage_is_digest_identical`
  (16.87s, 16.49s, just outside the top 10) are determinism proofs that spawn the test
  binary itself as a **child process three times** (1/4/16 threads) to get a real,
  separate-process thread count — 3x the underlying real computation, by design, to prove
  what they claim to prove. Not obviously wasteful; the cost is the point.
- `the_zero_field_divergence_is_measured_per_committed_fixture` (16.37s) iterates the full
  `prepare` pipeline over every committed fixture — same pattern as `prepare_folder`.

**Not found:** no test in the top ranks is sleeping, and no test is paying a
"rebuild a fixture per test" tax at meaningful scale — `real_mixed_initial_transcript_fixture`
(the most-reused synthetic fixture builder, 9 call sites in `cco.rs`) constructs a small
synthetic overhang-table mesh, and its own construction cost does not show up as a
distinguishable line item next to the real-corpus and repair-controller costs above.

---

## 2. Does `plinth-solid`'s build script re-run when nothing changed?

**Measured: no.** The hypothesis in the task brief — that `cargo:rerun-if-changed` on two
directories is fragile and re-triggers the build script "on every build" — does not
reproduce under either of the two realistic scenarios tested:

- **Repeat identical build**, `cargo build -p plinth-solid` run three times in a row with
  no edits: run 1 (immediately following an unrelated `-p`/`--workspace` invocation-style
  switch) showed `Dirty … info of dependency changed` and recompiled (1.9s); runs 2 and 3
  were `Fresh`, 0.06s and 0.057s, **with no `Running .../build-script-build` line** — the
  build script did not execute.
- **Unrelated content edit**, editing a leaf module in `plinth-support` (which does not
  depend on `plinth-solid`) and running `cargo build --workspace --all-targets -v`: the
  verbose log shows exactly one dirty crate, `plinth-support` ("the file
  `crates/plinth-support/src/seat_zone.rs` has changed"), and **no** `Dirty plinth-solid`,
  `Compiling plinth-solid`, or build-script-build line at all.

The one dirty/rebuild event actually observed was not from the directory watch: it came
from switching cargo's invocation style (`-p plinth-pipeline` immediately after a
`--workspace` run), which cargo's fingerprint tracks as a different unit-graph fingerprint
("info of dependency `plinth-solid` changed") and which forced `plinth-core`,
`plinth-solid`, and nine other crates to recompile once. That is a real, separate cost
(it recurs each time a developer alternates between `-p <crate>` and `--workspace`
invocations) but it is not the mechanism the task asked about, and it is not attributable
to the two watched directories.

**Verdict: not worth doing.** The 7.5s build-script cost recorded in the earlier
clean-build census is a one-time, clean-checkout cost, not a recurring per-edit tax. There
is nothing to fix here — the directories are watched correctly and reliably in the
scenarios that matter for iteration.

---

## 3. Cost of `[profile.test] opt-level = 2`

Measured with a scoped `cargo clean --profile test` between runs (removes only
test-profile artifacts, not `dev`, so third-party deps compiled under `dev` were
untouched) and `CARGO_PROFILE_TEST_OPT_LEVEL=0` as an env override (no file edited).

| | Build (clean, `--all-targets`) | Run: `a4_transactional_controller` (repair-loop-heavy) | Run: fast/normal tests |
|---|---|---|---|
| **opt-level 2 (as configured)** | 198.4s wall / 772.2s user | 28.2s | fast (sub-second to a few seconds) |
| **opt-level 0** | 48.4s wall / 135.8s user | 156.0s (5.5x slower) | measurably slower per test (e.g. `a4_noncoverage_gate_drift` 0.9s→6.8s, 7.5x; `a4_production_door` 2.7s→18.7s, 6.9x; `corpus_budgets`, a pure-Rust decimation bisection over real STLs, was still running past 90s — already ≥5x and climbing — when the sweep was stopped to bound this task's own wall-clock cost) |

**Build is 4.1x faster at opt-level 0; the run of the repair-loop-heavy binary is 5.5x
slower.** This is a real, symmetric trade-off, and the run-time side dominates:
`a4_transactional_controller` alone loses 128s at opt-level 0 — more than the entire
198s-vs-48.4s build-time saving (150s) would be worth if it had to be paid back on every
one of the workspace's ~343 test-profile compilation units on every clean build, which it
does not (a clean build happens once per checkout; the test run happens every iteration).
Scaled to the whole suite (excluding the one already-capped outlier), the geometry- and
repair-heavy tests that dominate the top-10 list above would all move into this same
5–7x-slower regime, very plausibly erasing the entire suite-run time saved by not doing
this and then some.

**Verdict: do not touch this setting.** It is already tuned correctly for this workload —
the workspace's dominant cost is a small number of tests doing real, opt-level-sensitive
geometry and C/C++ (manifold) work many times over (routing, boolean ops, decimation,
repair trials), run far more often (every `cargo test`) than the workspace is built clean.
Lowering test opt-level would trade a one-time, already-small build cost for a recurring,
large run-time cost on exactly the tests already identified as the bottleneck.

---

## 4. Do test-only edits inside large `#[cfg(test)]` modules force a full production-module recompile?

**Measured directly**, with a real (reverted) one-line comment edit in each case, using
`cargo build -p plinth-support --lib --tests -v` at a warm baseline
(`CARGO_PROFILE_TEST_OPT_LEVEL=0`, so both edits are measured on the same, already-warm
profile — the absolute seconds differ from the opt-level-2 numbers elsewhere in this
report, but the *comparison* between the two files is apples-to-apples and that
comparison is what section 4 is actually asking):

| Edit | File size | Rebuild time |
|---|---|---|
| One comment line inside `mod tests` in `runner.rs` (13,326 lines; `mod tests` starts at line 8016, ~5,300 lines of test code) | 13,326 lines | **2.267s** |
| One comment line inside the (already-measured) `mod tests` in `seat_zone.rs` | 1,155 lines | **2.177s** |

Both edits produce **exactly one dirty crate** (`plinth-support`) and **exactly one
`Compiling plinth-support` line** — confirming `#[cfg(test)]` is a conditional
*compilation* attribute, not a module or crate boundary: the whole crate is one
`rustc` invocation regardless of which file inside it changed. And the two rebuild times
are the same within measurement noise (2.267s vs 2.177s, a 4% difference on a ~2.2s
build) even though one file is **11.5x larger** than the other. Rust's incremental
compiler is doing exactly what it should: caching per-item codegen so that file size,
not just crate size, stops mattering once nothing outside the touched items changed.

**Verdict: no — not a material lever.** A test-only edit inside the 13k-line `runner.rs`
does not cost more to rebuild than the same edit inside a 1k-line file in the same crate.
Splitting the ~26,000 lines of inline `#[cfg(test)]` code out of `specialized.rs`,
`runner.rs`, `perception.rs`, and `cco.rs` into separate files would not change this
number — it is the same crate, same test-profile compilation, either way. The real cost
of these huge files is unrelated to build time: it is that ~26,000 lines of test code are
hard to navigate and review. That is a maintainability finding, not a build-cost one, and
this task should not be read as license to split them for speed.

---

## 5. Ranked levers

| # | Lever | Measured saving | Verdict |
|---|---|---|---|
| 1 | Cap the coverage-repair controller's trial count / measurement cost (REVIEW.md #12) | **≥190s of the ~330s measured per-binary run time**, plus the isolated 874s outlier — this is the large majority of the entire 184s+ full-suite run time | **Do this first.** By far the largest, and now measured rather than inferred. |
| 2 | Everything else in the test suite (real corpus geometry work: decimation, orientation rederivation, thread-count determinism sweeps) | Legitimate, ~140s of the ~330s | Not a defect. Leave alone. |
| 3 | `plinth-solid` build-script directory watch | **0s** — does not reproduce as a spurious-rebuild cost in either tested scenario | Not worth doing. No fix needed. |
| 4 | Lower `[profile.test] opt-level` | **Build −150s, but repair-heavy test run +128s on one binary alone**, net negative once the suite's actual test mix is considered | Do not touch. Keep opt-level 2. |
| 5 | Split `#[cfg(test)]` out of the four largest `plinth-support` modules | **Measured 0s** — a test-only edit in the 13k-line `runner.rs` (2.267s) costs the same as one in the 1k-line `seat_zone.rs` (2.177s), same crate either way | Not a build-cost lever. If pursued, justify it as a readability change, not a speed change. |

**Sizing the prize:** the total addressable build/test wall-clock saving found in this
task is almost entirely lever #1. Levers #3 and #4 are confirmed to be worth nothing or
negative; #5 is unmeasured but has no plausible mechanism to matter. The sequencing
implication for the later stage this measurement feeds: **spend the engineering effort on
the repair-controller's per-trial cost and trial cap (already scoped as REVIEW.md finding
#12, `runner.rs:7724` and `runner.rs:3412-3488`), and do not spend any of it on the build
system.** There is no meaningful build-system prize here to size — the build is already
fast (4.15s incremental, confirmed still true throughout this task), and the two
build-configuration levers this task was asked to check both come back at zero or negative.

---

## Appendix: raw evidence

- `timing-per-binary.txt`, `timing-raw/*.out` — opt-level 2 per-binary/per-test timings.
- `timing-per-binary-opt0.txt`, `timing-raw-opt0/*.out` — opt-level 0 per-binary/per-test timings.
- `opt2-build.log`, `opt0-build.log` — clean `cargo test --workspace --all-targets --no-run` timings at each opt-level (`/usr/bin/time -p`).
- `edit-rebuild.log` (captured via tool output, see transcript) — verbose log of the unrelated-edit rebuild used for section 2.
- `test-only-edit.log` — verbose log of the `runner.rs` test-only-edit rebuild used for section 4.
- No source files were left changed; the temporary edits made to
  `crates/plinth-support/src/seat_zone.rs` (twice, sections 2 and 4) and
  `crates/plinth-support/src/runner.rs` (once, section 4) were each reverted immediately
  after timing, and `git diff --stat` was empty before this report was written.
- Note: `crates/plinth-hollow/tests/_scratch_cascade_probe.rs` and
  `crates/plinth-hollow/tests/fixtures/` are untracked files that pre-existed this task
  and appear to be under active use by other work in this same checkout (the probe file's
  target fixture was briefly missing, then present, between two of this task's
  measurement passes). Not created, edited, or relied on by this task; flagged here only
  because a shared, non-worktree checkout can otherwise make an unrelated change look like
  something this task did.
