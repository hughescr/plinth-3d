# Plinth development loop and Rust skeleton: design (2026-10-30)

This design covers the edit-build-test loop, safe work by several agents at once, the crate layout, the build settings, the test tiers, how hard limits are built in, the performance hooks, the gates and hooks, and a step-by-step skeleton plan. It merges two independent proposals (called A and B in section 1). It follows PRD §6.1, §9, §10, §11 and §12.

**The success test (PRD §10).** The skeleton (read an STL, write a .chitubox, run the checker) must meet two budgets on real edits:

- Edit + targeted tests: **≤ 15 s**.
- Edit + the three Round bases end to end, with the checker: **≤ 90 s**.

If the skeleton misses either budget after one fix attempt, the orchestrator stops. It brings the evidence and a TypeScript-on-bun or Go alternative to the owner before any real engine code is written (PRD §10, §11).

Both budgets are PRD orchestrator starting values. Every other number in this design is an orchestrator starting value unless it is marked as measured. A starting value changes only by a recorded decision.

---

## 1. Decisions where the proposals differed

Proposal A built a throwaway prototype and measured it (section 2). Proposal B measured little but added several safety rules. Where they conflicted:

| Topic | A | B | Decision | Why (one line) |
|---|---|---|---|---|
| `dev` opt level | 2 | 1 | **2** | Measured: opt 1 costs the same compile time as opt 2 and its code runs 2.4x slower. |
| Checker location | Crate in the main workspace, with a dependency check | Own workspace and lockfile, run as a separate program | **Own workspace, separate program** | PRD §10 says the checker lives outside the generator's code base; a process boundary also makes in-memory hand-off impossible. |
| Loop tooling | Bash + jq scripts, ≤ 400 lines | Small Rust runner in its own workspace | **Small Rust runner `tools/devloop` (own workspace, std only, binary `devloop`) behind thin bash shims** | The bash version reached 511 lines, over its own 400-line cap, so the design's fallback applied. The runner owns process-group killing, timeouts, lock-wait detection, zero-tests detection and budget enforcement, with fast unit tests. The 400-line cap now applies to the bash shims only. |
| sccache | Not used | Used as `RUSTC_WRAPPER` | **Not used** | Measured: 0 hits across worktree paths, and workspace crates cannot be cached at all. |
| Construction types | Module inside `plinth-support` | Separate `plinth-construction` crate | **Module, split later by size trigger** | Module privacy seals the types equally well; an extra crate adds a rebuild step to every edit. |
| Pose, hollow, routing | New crates when their stage starts | Modules first | **Modules first; split by the measured size trigger** | Split on evidence, not on schedule. |
| Thread share per agent | Fixed 3 per slot (9 in total) | Shared total of 8 | **Shared total of 8, fixed per execution epoch, main included** | The owner's cap is 8 threads; 9 breaks it, and shares recomputed per command can overshoot (section 5.6). |
| Bench statistic | Median of 3 runs | Every run and the maximum | **Every run and the maximum; pass only if the maximum is in budget** | An agent feels the slow run, and a median of 3 hides it. |
| Bench edits | Canary constant flip | Five real recorded edits | **Both**: canaries for regression tracking, real edits at go/no-go and again when step 11 adds construction code | A one-constant edit can understate the cost of a real edit. |
| Go/no-go point | After construction types are built | After the STL → .chitubox → checker transport path works | **After the transport path** | PRD §10 defines the skeleton as that path; an empty skeleton proves nothing. |
| Contact admission API | One candidate per call (`try_place`) | Batch proposal and commit | **Batch** | Per-item calls in a hot path are a defect (PRD §10). |
| f32 rounding of contacts | Margin for later rounding | Round to file precision before admission | **Round before admission; margin covers checker uncertainty only** | Then the writer's numbers are exactly the numbers that were checked. |
| Allocator | mimalloc from the start | System allocator | **System; compare mimalloc only after the first credible Round 25, when profiling shows it can change a decision** | Change the allocator on a measurement, not a guess; speed work never delays the first print (PRD §8.1). |
| Release LTO | Thin | Off | **Off; same rule as the allocator** | Not measured yet. |
| Stage checkpoint budget | 10 min | 300 s | **300 s** | Stage-model runs must stay short enough to run at every slice end. |
| Linker re-check trigger | Link > 1 s | Link > 3 s | **> 1 s** | Measured links take 0.06 s, so 1 s already means something changed. |
| Memory measure | `getrusage` inside the program | Sampling supervisor | **Lifetime `getrusage` peak reported as a job figure only; per-model peaks from a short-lived worker or scoped measurement; admission budgets prevent overruns** | `ru_maxrss` is a lifetime high-water mark, and reporting is not prevention (section 8). |

---

## 2. What was measured, and what it does not show

**Setup (proposal A, 2026-10-30).** A throwaway prototype workspace in the scratchpad, built offline from the local cargo registry cache:

- 7 crates with the planned names and dependency directions.
- About 35,000 lines of synthetic Rust: a 110-line module (vector type, structure-of-arrays triangle batch, rayon parallel map, distance functions, 3 unit tests) copied many times.
- Dependencies: rayon 1.12, bytemuck, mimalloc, sha2.
- Toolchain: rustc and cargo 1.97.1, LLVM 22.1.6, Apple linker ld-27037.1, aarch64-apple-darwin. rustc names the host CPU `apple-m3`.
- Machine load average during the runs: 2.9 to 6.5 (the owner was using the Mac).
- Times are hyperfine medians over 5 to 7 runs. The "edit" was one change to a number inside one function body.

| # | Measurement | Result |
|---|---|---|
| M1 | Cold build of everything, `dev` at opt 2, -j 8 | 25.4 s (at -j 3: 58.6 s) |
| M2 | Dependencies only | 2.8 s |
| M3 | Edit in the 15k-line `support` crate → `cargo test -p support`, opt **1** | **3.77 s** |
| M4 | Same as M3, opt **0** | **1.22 s** |
| M5 | Edit in `support` → `cargo test -p support --lib`, opt 2 | 1.05 to 1.07 s (at -j 3: 1.09 s) |
| M6 | Edit in `support` → `cargo build -p plinth-cli`, opt 2 | 3.3 to 4.0 s (at -j 3: 5.3 s) |
| M7 | Edit in the 11k-line `geom` crate → tests of `support` | 3.5 s |
| M8 | Edit in `geom` → `cargo test --workspace` | 4.0 to 4.2 s |
| M9 | The same loops with 2.7k-line crates | 0.34 to 1.4 s |
| M10 | Run time of a heavy batched kernel test (450k triangles) | opt 0: 1.32 s · opt 1: 0.27 s · opt 2: 0.117 s · opt 3: 0.114 s. Same results at every level. |
| M11 | M10 at 1, 3 and 8 rayon threads | 0.164 / 0.129 / 0.116 s. Results identical to the last bit. |
| M12 | Link time for the 1 MB CLI binary and for a test binary | 0.060 s and 0.068 s |
| M13 | Two cargo commands on one build folder at once | The second waited ("Blocking waiting for file lock") |
| M14 | sccache 0.19, new build folder at a **different path**, warm cache | 0 hits, 23 misses |
| M15 | sccache, same path rebuilt after deleting it | 21 hits; cold build 22 s → 19 s; workspace crates "non-cacheable: incremental" |
| M16 | Same build folder, worktree copied to a new path | All 7 workspace crates rebuilt (18.5 s) |
| M17 | Clippy: cold workspace / after an edit / one crate | 5.3 s / 1.45 s / 1.27 s |
| M18 | `cargo fmt --all --check` on 35k lines | 0.62 s |
| M19 | Doctest pass per crate with no doc examples | 0.24 to 0.26 s per `cargo test` run |
| M21 | One build folder after about 15 builds | about 2 GB |
| M22 | Disk free | 187 GB (disk 90% full) |

**Also observed (proposal B):** rustfmt, clippy and hyperfine are installed. `cargo-nextest`, `mold` and `ld.lld` are not. The repository has no Rust or Cargo files yet. Eight ChituBox v1.4.0 sample projects exist in `test-data/chitubox-native/samples/`.

**What these numbers do not count:**

- Real algorithms. Real code with more generics may compile more slowly per line.
- cargo-nextest (not installed).
- Release builds with LTO.
- The run time of any real plinth step.
- Thread shares below 3 (`-j 2`).
- They come from one prototype on a loaded machine.

**What they do support:**

- opt 1 is a poor middle setting (M3, M10).
- Rebuild cost grows with the edited crate **and** every crate that depends on it (M7).
- The linker is not a bottleneck (M12).
- A shared build folder makes agents wait one after another (M13).
- sccache gives almost nothing to this loop (M14, M15).
- A new worktree path forces a rebuild of every workspace crate (M16).

---

## 3. Loop budgets per level

Every loop command goes through one wrapper, `scripts/t` (section 3.2). Every level has its own deadline; Lnoop and L0 to L3 stop at a nominal 120 s process-group bound (section 3.2). L4 and L5 never run inside an agent's inner loop.

| Level | What runs | Budget (a miss fails) | Target | Evidence |
|---|---|---|---|---|
| **Lnoop** | No change → `cargo build` | 2 s, and 0 `Compiling` lines | — | Proves there is no rebuild loop |
| **L0** check | Edit → `cargo clippy -p <crate> --all-targets` | 5 s | ≤ 2 s | M17: 1.27 s |
| **L1** one crate | Edit → build + unit and property tests of the edited crate | **15 s** (PRD §10) | ≤ 5 s | M5: 1.05 s; M3-type: 3.6 to 3.8 s |
| **L1d** dependent | Edit in `geom` → tests of one dependent crate | 15 s | ≤ 6 s | M7: 3.5 s |
| **L2** fast suite | Edit → all tests in both workspaces: unit, property (fixed seed), native-file tests, broken-file tests, compile-fail tests, independence check | 60 s | ≤ 30 s | Compile part 4.0 to 4.2 s (M8); run part not measured |
| **L3** Round bases | Edit → rebuild CLI and checker → Round 25, 50, 100 → .chitubox → checker → report and pictures; plus Round 25 at 1 thread and at the slot's allocation, bytes compared (the 8-thread case runs only in the bench epoch) | **90 s** (PRD §10) | ≤ 30 s | CLI rebuild 3.3 to 4.0 s (M6); run part not measured |
| **L4** stage checkpoint | Stage models run so far, random-seed property tests, thread-count checks, cancel and error cases | 300 s, in the background, at slice end only | — | — |
| **L5** corpus and timing | Models authorised by PRD §8.2 so far (treant only after its entry gate), release profile, alone on the machine, orchestrator only | 1,800 s for the whole run; per-model limits from PRD §8.5 (treant ≤ 120 s) | — | — |
| **Cold** | Locked fetch (reported separately), then cold build + L2 | 180 s | — | M1: 25.4 s for 35k lines |
| **Pre-commit** | Hooks (section 9) | 5 s | — | M18 + grep |
| **Gate** | `scripts/gate` (section 9) | 180 s, in the background | — | — |

The 1,800 s corpus budget bounds test feedback only. It never replaces a per-model speed limit from PRD §8.5.

L3 always processes Round 25, Round 50 and Round 100, but a committed **stage manifest** specifies each model's mode and exact expected checker statuses. Before support work, all three are **transport tests**: model only, explicitly not print candidates. The checker reports rules it cannot apply as "unverified" or "not applicable, with reason", never "pass". During the first Round 25 milestone, Round 25 uses the owner-approved tilted keep-up copy with real supports and requires every applicable rule to be verified; Round 50 and Round 100 remain explicitly labelled transport tests. They move to supported-output acceptance only after Round 25 has been offered for owner judgement. The 90 s L3 budget applies to the complete current manifest. A transport result never counts as a supported-output pass.

The budgets live in a committed file, `loop-budgets.tsv`, with columns `level, command, budget_s, target_s, source`. The source column says "PRD" or "orchestrator starting value".

### 3.1 Timing definition

"Edit to result" is wall time from starting the command, right after a saved edit, to its final line. It includes cargo planning, compiling, linking, running the tests and writing results. It does not include thinking time. Nothing may be pre-built outside this interval to make the number smaller.

### 3.2 `scripts/t <level> [crate]`

What it does:

- Dispatches by level. Every level has its own deadline and expected evidence. For Lnoop and L0–L3 the runner sends TERM after 115 s and KILL after a further 5 s, a nominal 120 s process-group bound. The runner starts the command as the leader of its own process group and signals the whole group; there is no dependency on GNU `timeout`. Longer levels use their declared deadlines and the background protocol.
- The supervisor handles interruption, reaps the command and confirms its recorded process group is gone. A project-wide `pgrep` match is not proof that this command's children are gone.
- Applies the slot's thread allocation (section 5.6).
- Prints at most the last 40 lines of a failure, then one verdict line. Example: `L1 plinth-support 3.4 s / budget 15 s / target 5 s / load 4.1 / PASS`.
- Appends a row to `target/loop-log.tsv` (local, never committed): time, slot, `git rev-parse --short HEAD`, hash of `git diff`, level, crate, thread allocation, seconds, budget, 1-minute load (`sysctl -n vm.loadavg`), test count, verdict.

Exit codes: 0 pass; 1 test failure; 3 `LOOP BUDGET EXCEEDED`; 4 setup error; 5 timeout. Budget failure and timeout are distinct outcomes.

Evidence each level must show:

- Lnoop: successful build completion and zero rebuilt targets.
- L0: Clippy completion.
- Test levels (L1, L1d, L2): a positive selected-test count.
- CLI levels (L3 and above): the expected model results and checker reports.

"Zero tests" is not an error for a command that is not a test command.

It fails loudly when:

- The budget is exceeded. A passing test result after the budget is still a budget failure.
- **A test level selects no tests.** That is a failure, never a pass.
- **Cargo waits for a lock on the slot's build directory.** It watches the streamed log for `Blocking waiting for file lock` lines. A line that does not name the package cache (for example `... on build directory`) is a hard failure: it terminates that invocation at once and fails with `LOCK WAIT: another cargo process is using this slot's build folder` (M13). It does not wait for the deadline.
- **Cargo waits for the shared package cache lock** (`Blocking waiting for file lock on package cache`; the lock lives in `~/.cargo` and is shared by every slot). Such waits are acceptable in any state, cold or steady. It is not a build-directory wait: it is counted and timed (from the `Blocking` line to the next line Cargo prints), both figures appear on the verdict line, and it fails only if the total package-cache wait in one command exceeds 2 s.
- `RUSTFLAGS` is set in the environment (it would throw away every build artifact).
- The level is not available yet. That is a setup error, never logged as a pass.

Budget, timeout and lock-wait failures never count as a successful gate.

It labels:

- A run with no `Compiling` line as `NOCHANGE`. Such a run never counts as an edit-loop result.
- A budget miss with load above 8 as `LOADED`, and ends this invocation with a nonzero budget result. The orchestrator may schedule exactly one fresh invocation as the repeat, never an automatic second run inside the same foreground command. The two log rows are linked; neither is deleted. The repeat determines the final loaded-run disposition, and a second miss fails regardless of load (PRD §8.5: marked and repeated). A linked passing repeat does not retroactively label the first invocation as passing. A budget is never raised to make a run pass.

### 3.3 `scripts/loop-bench` (bench epoch only)

- Every crate has a tiny `src/loop_canary.rs` (`#[inline(never)] pub fn loop_canary() -> u32 { N }`), called from the crate's production library or binary, so an edit to it drives the real rebuild chain.
- Run Lnoop separately on a settled build with no source edit. It must take at most 2 s and compile no crate.
- Before every timed repetition of every edit level, apply a fresh deterministic edit to a canary compiled into the relevant production library or binary. Do not run another build between that edit and the timed command. A repetition with no affected compilation is invalid evidence, not a passing edit-loop measurement.
- Record the edited file, edit diff and rebuilt targets. Include support-to-CLI, geometry-to-dependent and checker-only rebuilds. Each end-to-end repetition includes all builds needed after its own fresh edit. Run three valid repetitions per case and report every result and the maximum.
- Require a clean bench worktree before starting. Restore only the bench's own edits, including on failure, and confirm the original tree is restored. Real edits supplement the canaries at go/no-go (below); each starts from an explicitly recorded state and follows the same timing rule.
- It prints every run and the maximum against `loop-budgets.tsv`. It fails if any maximum is over budget or any repetition is invalid.
- It is a background operation with its own explicit aggregate deadline, recorded in `loop-budgets.tsv`. Derive that deadline from the committed case list, repetition count and each invocation's timeout, with bounded cleanup overhead. Individual loop budgets still decide whether each case passes. Benchmark duration is reported separately from gate duration. Its supervisor stops and reaps all children on interruption or aggregate timeout.

When it runs:

- At every slice end.
- Whenever `Cargo.toml`, `.cargo/config.toml`, `rust-toolchain.toml`, a lockfile or a dependency changes.
- Once a day while code is being written.
- Only in an exclusive bench epoch, when no writing slot holds an allocation (section 5.6).

At go/no-go (step 7 of section 10) it also times **real recorded edits** in the format reader/writer, geometry, emitter/CLI and checker code that exists then, including a library edit that rebuilds the CLI. When step 11 introduces construction and generator code, it repeats the go/no-go measurements with real edits there, before support generation expands. No dummy engine code is written to fill a category. Test-only edits are reported separately. It reports each result and the maximum, never a percentile.

**Reporting.** Every orchestrator status report shows the latest bench maxima next to the previous ones. A level over budget by 50% for two days in a row blocks feature work until it is fixed.

**Size trigger (from M3 to M9).** Split a crate when its L1 canary maximum goes over 5 s, or its library rebuild alone goes over 3 s. In the prototype's code style, that is about 15k lines.

---

## 4. Workspaces and crates

### 4.1 Layout

```
plinth-3d/
  Cargo.toml          main workspace: exact pins, [workspace.lints], profiles
  Cargo.lock          committed; gates use --locked
  rust-toolchain.toml channel = "1.97.1", components = ["rustfmt", "clippy"]
  .cargo/config.toml
  .config/nextest.toml
  clippy.toml  rustfmt.toml  loop-budgets.tsv  limits.toml
  .githooks/          pre-commit, pre-push
  scripts/            thin bash shims (≤ 10 lines each, ≤ 400 lines in total): devloop (builds the runner if
                      stale, then runs it), setup, t, loop-bench, slot; later gate, check-independence
  tools/devloop/      SEPARATE workspace: the loop runner, binary `devloop` (subcommands t, loop-bench, slot,
                      setup; later gate, check-independence). std only, no plinth crate, own Cargo.lock,
                      clippy.toml and target/. Its unit tests run in L0 scale time.
  crates/
    geom/        plinth-geom        mesh core
    chitubox/    plinth-chitubox    .chitubox and .cfgx read, write, round trip (FR-41 deliverable)
    support/     plinth-support     the generator: construction types, contacts, routing, raft, load model; later pose, hollow
    emit/        plinth-emit        support plan → native ChituBox records
    cli/         plinth-cli         binary `plinth`
    acceptance/  plinth-acceptance  tests only: broken files through the production writer; end-to-end tests
  checker/            SEPARATE workspace: its own Cargo.toml, Cargo.lock, rust-toolchain.toml, target/
    src/              binary `plinth-check`
```

The main workspace's `Cargo.toml` lists `checker` in `exclude`. The checker's own config uses the same profile settings and `target-cpu`.

### 4.2 What each crate holds

| Crate | Holds | Depends on | Expected edit rate |
|---|---|---|---|
| `plinth-geom` | Unit types (`Mm`, `Mm2`, `Deg`); structure-of-arrays indexed mesh; binary and ASCII STL reader; bounds; BVH; batched closest-point, ray and segment queries; slicing to layers; layer bitsets; robust predicates; `simd` module | none | High at first, then stable |
| `plinth-chitubox` | Byte-level records; reader that follows the pointer chain with bounds checks; writer; unknown bytes kept as they are; `.cfgx` profile reader; format docs that cite a native file for every claim | none (std only) | Medium |
| `plinth-support` | Construction types (section 7); contact set; routing; columns; raft; load model; warnings data. Takes `&Mesh`, produces a frozen `SupportPlan` | geom | **Highest** |
| `plinth-emit` | `SupportPlan` → records; mesh → triangles. Keep it ≤ 2k lines | support, chitubox | Low |
| `plinth-cli` | Flags (`--max-threads`, FR-6 marker forms); thread pool; per-model run and status; temp file → checker → publish; report | geom, support, emit, chitubox | Low; keep it thin |
| `plinth-acceptance` | Tests only (section 6) | dev-deps: cli library, emit, chitubox | Medium |
| `plinth-check` (own workspace) | Its own .chitubox reader written from `reference/formats/chitubox-format.md`; its own STL reader; patch area, gap, section and continuity measurements; H1–H7 and H9 verdicts with uncertainty; "unverified" for anything it cannot measure; a `dump` command that prints the numbers it read as JSON | **no plinth code** | Medium |

### 4.3 Dependency direction

```
geom ◀── support ◀── emit ──▶ chitubox
  ▲         ▲          ▲         ▲
  └──────── cli ───────┴─────────┘ ──runs──▶ plinth-check (separate program)
acceptance (tests only) ──▶ cli, emit, chitubox; runs plinth-check
```

Why this keeps rebuilds small:

- Most edits happen in `support`. It rebuilds support → emit → cli; emit and cli stay thin. M6 measured 3.3 to 4.0 s for this chain with a 15k-line support crate.
- A format edit never rebuilds the generator. A checker edit never rebuilds anything in the main workspace.
- `geom` is the only root. An edit there rebuilds most crates (M7, M8: 3.5 to 4.2 s). Settle its batch API early. Split it (for example into `geom` and `geom-query`) when the size trigger fires.
- One unit-test binary and at most one integration-test binary per crate (`tests/it/main.rs`). Never one binary per fixture.
- Keep large generic code out of public APIs. Prefer concrete batch interfaces.
- A native boolean library for hollowing, if needed, goes in its own stable leaf crate, with its cold build measured first.
- Changing the toolchain, `rustflags` or a profile throws away every build folder. Only the orchestrator does this, followed by a bench run.

### 4.4 Checker independence (NFR-6), enforced

1. **Separate program.** The CLI writes the file to disk, then runs `plinth-check` on those exact bytes. They exchange a versioned JSON report, never Rust types. The checker gets paths only: the .chitubox file, the input STL, the pose record and the permitted-change record, with their hashes. It treats those records as claims to verify, not as facts.
2. **`scripts/check-independence`** examines the checker's complete resolved dependency graph (`cargo metadata --locked --offline`, checked with jq), including normal, build and development dependencies. Every package must be either a path package whose canonical source lies inside `checker/`, or an explicitly approved registry package. Git sources and external path packages are rejected. The approved registry list, including transitive dependencies, is committed with exact package identity and source from the lockfile. Dependency changes require review of the complete changed graph; numerical geometry kernels shared with the generator are prohibited, so a bug in a shared kernel cannot hide on both sides. A dependency-free checker may start with an empty registry allowlist.
3. A grep rejects `#[path`, `include!` and `include_str!` in `checker/` that point outside `checker/`.
4. Copied code cannot be caught by a tool. Reviewers check that no generator routine was pasted into the checker.
5. **`limits.toml`** holds the limit values (H1 0.50 mm², H4 5.0 mm, H6 0.3 mm, H9 0.1 mm, region length 3.0 mm). It is data, not code. Each side parses it with its own code. The checker prints the values it used in every report. A checker test pins them to the PRD values, so a wrong edit to the file is caught.
6. **Readers agree.** A test in `plinth-acceptance` runs `plinth-check dump` and the `plinth-chitubox` reader on every native file. It compares a neutral list of numbers (record type, end points, radii). No shared types.

The independence check runs in pre-commit and in the gate. Its sabotage test adds a path dependency on `plinth-geom` to the checker; the check must fail. It also tests an indirect forbidden dependency and an unapproved source, and a positive test shows the approved transitive graph passes. Reader agreement (item 6) proves consistent decoding, not independent geometric correctness; native examples and independent measurement controls remain required.

---

## 5. Build settings

### 5.1 Profiles (one inner-loop profile)

```toml
[profile.dev]                 # THE inner-loop profile: plain cargo build / cargo test / nextest use it
opt-level = 2                 # M3, M5, M10: same rebuild cost as opt 1, code 2.4x faster; opt 2 runs as fast as opt 3
debug = "line-tables-only"    # backtraces with line numbers, small debug info
split-debuginfo = "unpacked"  # no dsymutil step on macOS
incremental = true
codegen-units = 256
debug-assertions = true
overflow-checks = true
lto = false

[profile.dev.package."*"]     # registry dependencies only: built once per slot
opt-level = 3
debug = false

[profile.dev.build-override]  # build scripts and proc macros: compile fast
opt-level = 0
debug = false

[profile.release]             # L5 timing and corpus, owner-facing binaries; bench slot only
opt-level = 3
debug = "line-tables-only"    # for profiling
split-debuginfo = "unpacked"
codegen-units = 16
incremental = false
lto = false                   # thin LTO compared only after the first credible Round 25, if profiling shows it matters
overflow-checks = true        # re-measured at the first L5 run
```

Why one inner-loop profile, at opt 2:

- An agent who types the plain command gets the right build folder. A second profile doubles the build artifacts, and a forgotten `--profile` causes a cold build.
- opt 0 saves about 2.5 s per library rebuild (M4 against M3), but makes geometry 11.6x slower (M10). That would break L3 on real models.
- For a debugger at opt 0, use `CARGO_PROFILE_DEV_OPT_LEVEL=0` with a scratch `CARGO_TARGET_DIR` under `$TMPDIR`. Never in the slot's build folder.
- Correctness must not depend on debug assertions. L5 runs the release profile.

**Doctests.** Set `doctest = false` in every `[lib]` except `plinth-support`, which holds the compile-fail proofs (section 7.4). M19 measured 0.25 s per crate per run even with no examples. nextest does not run doctests, so L2 runs `cargo test --doc -p plinth-support` explicitly.

### 5.2 `.cargo/config.toml` (same in `checker/`)

```toml
[build]
jobs = 8                                   # scripts/t overrides with CARGO_BUILD_JOBS
[target.aarch64-apple-darwin]
rustflags = ["-C", "target-cpu=apple-m3"]  # explicit, not "native": same code on any future Mac
[env]
RUST_BACKTRACE = "1"
```

- Never set `RUSTFLAGS` in the environment. `scripts/t` refuses to run if it is set.
- `target-cpu=apple-m3` does not change floating-point results. Rust never fuses multiply-add on its own; write `mul_add` where it is wanted. M10 and M11 gave identical results at every opt level and thread count.

### 5.3 Linker

Keep Apple's default linker (M12: 0.06 to 0.07 s per link). Do not install lld or mold. Re-measure with an 8-line `cc` timing wrapper in `scripts/` only if the bench shows a link over 1 s or the CLI binary grows past about 20 MB.

### 5.4 sccache

Not used, and not set in committed config, so a clean checkout never depends on it (NFR-8). Reasons: it hits only on the same build-folder path (M14, M15); it cannot cache incremental workspace crates (M15); slot build folders persist, so dependencies build once per slot anyway.

### 5.5 Parallel agents: fixed slots

Measured facts: a shared build folder serialises agents (M13); a new worktree path rebuilds every workspace crate (M16); sccache cannot bridge paths (M14). So: **a fixed set of worktrees at fixed paths, each with its own persistent build folders inside it.**

```
dnd-campaigns/plinth-3d/            main checkout: orchestrator (slot "main")
dnd-campaigns/plinth-slots/s1/      git worktree, writing agent 1
dnd-campaigns/plinth-slots/s2/      writing agent 2
dnd-campaigns/plinth-slots/s3/      writing agent 3   (PRD §11: at most 3 writing tasks at once)
dnd-campaigns/plinth-slots/bench/   loop-bench, L5 timing, clean-build checks
```

Slot rules:

- `target/` and `checker/target/` stay inside each worktree. No `CARGO_TARGET_DIR` is needed, so two slots cannot share a build folder by mistake.
- **Start a task:** `scripts/slot s2 start lane/<name>`. It refuses if the slot's tree is dirty, then runs `git -C plinth-slots/s2 switch -c lane/<name> main` and writes a slot-active marker. Only changed files rebuild. The allocation check, the marker and the branch switch run under one exclusive lock (`plinth-slots/.slot.lock`, a kernel file lock that dies with its process), so two simultaneous starts cannot together exceed the 8-thread total.
- **End a task:** after all slot processes have stopped, the orchestrator reviews, signs and merges the work. Confirm the slot is clean, then switch that slot to detached HEAD at the merged main commit. Only then delete the merged lane branch and clear the slot-active marker. If the slot is dirty or still has a running command, stop retirement and report it; never discard changes. The fixed worktree path and both warm build folders remain. No new worktree is made per agent.
- **Do not use Claude Code's automatic `isolation: worktree` for Rust work.** It makes a new path every time, which forces a full rebuild (M16).
- **No seeding.** Never copy, hard-link or pre-seed a build folder. (The first attempt lost 6 days to a stale seed.)
- **One cargo command at a time per slot.** Run clippy and tests one after the other. A slot does not build while its previous test process still runs.
- **Disk.** Each slot's build folders are about 2 to 8 GB (estimate from M21). `scripts/slot` warns above 12 GB and runs `cargo sweep --time 7`. Five slots: about 40 GB at most.
- **rust-analyzer.** If it runs, set `cargo.targetDir = true` in `rust-analyzer.toml` so it builds in `target/rust-analyzer/` and does not take the slot's lock.
- **Lockfiles.** Only the orchestrator adds dependencies, on main, merged before any task needs them. This avoids lockfile conflicts between slots.

### 5.6 Thread allocation: 8 in total

The owner's cap is 8 project threads in total, not 8 per agent.

**Thread allocation is fixed for an execution epoch, not recomputed independently by running commands.** The orchestrator records one allocation covering main, all writing slots and the bench slot before commands start. Allocations sum to at most eight compute workers (for example main 2, s1 3, s2 3; or the bench slot alone with 8). A new slot starts only when an existing allocation can accommodate it; changing allocations requires affected commands to finish first. No inner-loop command waits for a worker allocation: it exits with a setup diagnostic and is rescheduled by the orchestrator.

Each command uses its allocation for Cargo jobs (`CARGO_BUILD_JOBS`), test concurrency (`NEXTEST_TEST_THREADS`) and its application worker pool (`--max-threads`). Tests normally use one Rayon worker each; a parallel test runs alone and uses at most its allocation. Explicit eight-worker determinism tests run only in the exclusive bench epoch. Concurrent writing slots run determinism checks at counts no greater than their allocations.

Cargo jobs, test processes and application workers are separate controls, not proof of a literal OS-thread ceiling. Measure runnable compute concurrency and total process/thread counts separately during the two-slot and three-slot checks (steps 2 and 7). Account for main, checker, renderer and background tooling. If compiler or library parallelism exceeds the intended compute budget, reduce the allocations or configure that parallelism before accepting the check. Report what the cap controls rather than claiming that `-j` caps every OS thread.

- Neither the job count nor the Rayon thread count is part of cargo's rebuild check, so a changed allocation causes no rebuild. Step 2 of section 10 confirms this.
- Measured cost: L1 does not change at -j 3 (M5: 1.09 s against 1.05 s); a CLI rebuild is 31% slower (M6). -j 2 is **not measured**; step 2 measures L1 and L3 at -j 2.
- Library code never builds its own pool and never reads `RAYON_NUM_THREADS`. Avoid native libraries that start their own thread pools.
- The bench slot and L5 run only in an exclusive bench epoch.

---

## 6. Test tiers

| Tier | Contents | Ground truth | Runs in | Per-test cap |
|---|---|---|---|---|
| **T0 unit** | `#[cfg(test)]` in each crate. Small synthetic shapes built in code (flat plate, 30/45/60° plates, ridge, cone, sphere, two close faces), each < 2,000 triangles. Tests normally < 100 ms | Hand-derived values | L1, L2 | killed at 6 s |
| **T0p property** | proptest in `tests/it/main.rs`. Fast: fixed seed, 64 cases. Checkpoint: random seeds, 1,024 cases. Failures saved in `proptest-regressions/` (committed). See the list below | The property | L1, L2 (fixed); L4 (random) | killed at 6 s |
| **T1 native** | `plinth-chitubox` and `plinth-check` read every file in `test-data/chitubox-native/`, including samples 01–06 and the braced on-edge sample. See the list below | **Only files ChituBox wrote** (NFR-5). Plinth output is never ground truth | L2 | killed at 30 s |
| **T1n broken files** | One or more per rule H1–H7 and H9, made through the production writer in `plinth-acceptance`. See the list below | The rule in PRD §6 | L2 | killed at 30 s |
| **T1c compile-fail** | ≤ 10 `compile_fail` doctests in `plinth-support` (section 7.4) | rustc | L2 | — |
| **T2 end to end** | Round 25/50/100 → CLI → .chitubox → checker, with per-model transport or supported-output expectations taken from the committed stage manifest described in section 3. Asserts: zero violations; zero unverified for the rules that apply (supported-output tests; transport tests assert the exact expected unverified set); job peak memory ≤ 1 GB; Round 25 at 1 thread and at the slot's allocation gives identical bytes, and at 8 threads in the bench epoch (NFR-2) | The checker reading the written file | L3 (L2 runs Round 25 only) | killed at 90 s |
| **T3 checkpoint** | Stage models through CLI + checker; random-seed properties; cancel during parse, generate, write and check | Checker + owner review | L4 | 300 s in total |
| **T4 corpus and timing** | Models authorised by PRD §8.2 so far (treant only after its entry gate), release profile, alone on the machine. Records time per step, peak memory and machine load | Checker; PRD §8.5 | L5 | per PRD |

Test expectations are explicit by implementation stage. Transport tests assert diagnostic status and the exact expected unverified rules; they never assert print-candidate status. Supported-output acceptance tests require all applicable measurements to be implemented and verified. Advancing the stage changes the committed expected rule set; the runner must not infer applicability from whichever checks happen to exist.

**Key properties (T0p):**

- No admitted contact has a patch above H1 minus the margin, by a brute-force sampled area (a test oracle, separate from the generator's fast path).
- No two admitted patches are fused.
- No admitted support shape breaks the 0.3 mm gap.
- Write → read is lossless for any valid record set.
- Batched geometry kernels equal their scalar reference.
- 1 thread and the slot's allocation give identical output; 3 and 8 threads are compared in the bench epoch.
- The reader survives malformed input: offset overflow, truncation, huge counts (no allocation bomb), non-finite numbers, bad pointers. It returns an error, never panics.

**Native tests (T1):**

- Parsing succeeds and structural rules hold.
- An unmodified native file reads and rewrites byte for byte (lossless round trip, unknown bytes kept).
- Between sample pairs (01→02 lift, 01→06 rotation, 03→04 raft), only the expected fields change.
- The checker's patch measurement on ChituBox's own contacts meets its stated uncertainty bound (PRD §6.2).
- An edited or generated file is checked by field relations, pointer bounds, ownership and decoded geometry, not by byte identity to a native file.
- A native shape the code does not yet understand reads as "unverified", never as an invented meaning.
- At most 3 named pinned plinth outputs in the whole repo, and they are never ground truth.

**Broken files (T1n).** Each one must be rejected with the right rule ID, the right location and a nonzero exit. A rule not yet built must report "unverified", never "pass". Examples:

- Oversized patch; fused patches; oversized foot on the model.
- A brace that is connected in the graph but first appears in a layer before its carrier.
- Lift below 5.0 mm; model touching the raft.
- Cavity with no vent; support in a drain bore.
- A rod closer than 0.3 mm to the model.
- An undeclared model change, a missing part, a scale change.
- A thin join between rods that are each thick enough.

A second test runs the normal CLI path on a broken plan and checks that a failed or unverified checker result **blocks print-candidate status**. Otherwise the rejection tests could pass while the product ignores the checker. Each broken file proves the check fires; it does not prove the check is complete. Record the smallest defect each check can see.

**Which tier runs when:**

- Every edit: L1 for the edited crate.
- Before handing work back: L2.
- Any change that touches placement or writing: L3.
- End of a slice: L4.
- L5, by the orchestrator only, runs only when the current development stage justifies a corpus or timing decision. Its model list is limited to stages currently authorised by PRD §8.2; the treant is excluded until its entry gate passes. First Round 25 owner review requires its checker results, report, pictures and H8, not a corpus run. No full-corpus run is required merely because the owner will open a file.
- L4 and L5 never run per change, and never from several slots at once (PRD §11).

### 6.1 nextest settings (`.config/nextest.toml`)

- Unit and property tests: `slow-timeout = { period = "2s", terminate-after = 3 }` (killed at 6 s).
- Native and broken-file tests: period 10 s, killed at 30 s.
- End-to-end tests (`e2e_*`): period 30 s, killed at 90 s. `threads-required = "num-test-threads"`, so each runs alone and uses the slot's allocation.
- Tests that build a multi-thread pool (thread-count checks) also run alone, by the same setting, at counts no greater than the slot's allocation. The 8-thread checks run only in the bench epoch.
- All other tests run with `RAYON_NUM_THREADS=1` per test process. Eight test processes must never each start eight worker threads. nextest has no plain environment table, so `.config/nextest.toml` sets it with a setup script, which nextest 0.9.150 still marks experimental (`experimental = ["setup-scripts"]`).
- Output: `--status-level fail --final-status-level slow`, so passing tests do not fill agent context.
- nextest runs each test in its own process, so a fixture parsed once is not shared between tests. Group a batch of native files into one bounded test where parsing once matters.

### 6.2 No hidden skips

- No `#[ignore]`. "Slow" means a level, not an ignore.
- No environment flag that makes a test return success early.
- No known-red allowance in a gate.
- L4 and L5 are not `cargo test` targets. They are CLI runs over a model list, driven by `scripts/t L4|L5`. A plain `cargo test` is always the fast suite.
- A test must show that the path it claims to test actually ran (for example, an admission test asserts the rejection path was hit).

### 6.3 Agents never stall

- Agents run only `scripts/t Lnoop..L3` in the foreground. Each complete wrapper invocation, including setup, child execution, log handling and cleanup, has a nominal 120 s bound. Its child deadline leaves room for cleanup. The wrapper never starts an automatic retry inside that interval. Longer operations and linked repeats are separate invocations with explicit outcomes.
- L4, L5, the gate and cold builds follow the backgrounded-command protocol: start in the background, note the output file and PID, poll with short foreground calls that tail the output and check the process is alive, read the exit code, confirm no children remain. An agent never ends its turn with a command still running.
- After any manual stop, confirm the command's recorded process group is gone; a stopped parent can leave children running. `pgrep -f "plinth|cargo"` is an extra look, not proof.

---

## 7. Hard limits built in at construction

**Principle (PRD §6.1, ruling 2026-10-28).** The step that creates a quantity owns its limit. The API makes a value that breaks a limit impossible to build. The independent checker still measures every written file. A checker violation is a generator defect to fix at its source, never something a later pass repairs. A rare minor repair pass is named in the report with its reason (PRD §10).

**What types can and cannot prove.** Private fields and missing constructors stop unauthorised creation and later edits. They do not prove geometry. A `PatchArea` capped at 0.50 mm² proves nothing unless the code that computes it is right. The geometric facts come from conservative geometry code inside the constructors; the checker tests those assumptions from the file.

### 7.1 Contacts: H1 area, H2 fused, H6 region

```rust
// plinth-support::construct  (fields private; the only place contacts and columns are made)
pub struct ContactId { plan: PlanId, ix: u32 }   // no public constructor; carries its plan identity
pub struct PatchArea(f64);   // no public constructor; read-only .mm2()
pub struct Contact { /* centre, normal, approach, ball_d, depth, patch: PatchArea, region: RegionCapsule — all at file precision */ }

pub struct PlanBuilder<S> { /* plan id; owned immutable PosedModel (file precision) + hash; limits; reservation state + revision */ }
impl PlanBuilder<Contacts> {
    pub fn new(model: PosedModel, limits: Limits) -> Self;   // fixes model hash, pose, limits and plan identity
    pub fn admit(&mut self, cands: &[Candidate], out: &mut Vec<Result<ContactId, Reject>>);  // no model argument
    pub fn get(&self, id: ContactId) -> Result<&Contact, ForeignHandle>;  // no get_mut, iter_mut or remove
}
pub enum Reject { AreaOverLimit { upper: Mm2, max: Mm2 }, WouldFuse { with: ContactId }, RegionBlocked { by: ContactId }, NonFinite, Unverified, /* … */ }
```

A plan builder owns one immutable posed, file-precision model and one reservation state. Its constructor fixes the model hash, pose, limits and plan identity; admission calls cannot supply another model. Contact and column handles are checked against that plan identity before use. All numeric inputs must be finite, and every limited quantity is checked after conversion to the native representation.

Contact admission is batched against that owned model and reservation state. Raw indices, copied handles from another plan, and handles to objects not admitted in this plan are rejected before indexing or construction. Tests exercise cross-plan handles and attempted model substitution.

What `admit` does, in order:

1. **Native shape only.** Use only contact shapes whose ChituBox form the native samples have settled (PRD §6.2). Round each candidate's numbers to the file's precision **before** any check. The written numbers are then the checked numbers.
2. **Broad phase in parallel.** Compute patches and nearby candidates for the whole batch with batched geometry queries.
3. **Commit in a fixed order** (by candidate ID) against one reservation state. This keeps the output the same at any thread count.
4. **Patch area from geometry.** Compute a conservative upper bound for every connected model-surface patch the native shape makes, including a foot on the model and a touch on a second sheet. Never infer area from a diameter field. Admit only if the generator's upper bound is at most H1 minus a documented acceptance margin. Define separately the checker's error bound and the maximum excess of its reported upper bound over the true area. The acceptance margin covers the latter, including numerical representation effects. For a checker estimate with a proven symmetric error radius ε and acceptance upper bound estimate + ε, this can require 2ε; a smaller margin needs a stronger established bound. Establish this relation on analytic near-limit cases and native shapes before claiming checker acceptance by construction. If a bound cannot be established, refine within bounded work or reject as `Unverified`. Never round an unclear area down.
5. **No fused patches (H2).** Reject a candidate whose patch is closer than the measurement resolution to any admitted patch. H2 is about patches on the model. Supports may still merge away from the model and near the plate. No minimum spacing beyond this is invented.
6. **Reserved region.** Reject a candidate whose contact-surrounding region (ball, neck, tip cone; ≤ 3.0 mm) would cut into another contact's region.
7. **Approach direction** is fixed here (FR-26).
8. **A rejection is a value.** If the admitted contacts cannot carry the load, the planner asks for more contacts that do not touch (FR-22) or raises a located warning. It never grows or moves an admitted contact.

### 7.2 Structure: H3, H4, H5, H6, H7, H9

| Rule | Owning operation |
|---|---|
| **H9** thickness | `Section::new(d: Mm) -> Result<Section, TooThin>` refuses < 0.1 mm. Every rod, neck and join takes a `Section`. Joins are checked on their real native shape after rounding; a nominal diameter does not prove a narrowed join is thick enough. |
| **H6** gap | Owned by batched admission of complete proposed native support shapes, including their radii, necks, joins and contact regions—not by centreline routing alone. A clearance result is bound to the exact rounded shape, model and reservation revision it checked. Commit either consumes it against that same revision or rechecks against the current state. A caller cannot pair a certified path with a different section or later widening. Tests include a clear centreline whose thickened rod violates H6, and an addition that invalidates an earlier reservation. |
| **H3** continuity | `Anchor::on_plate() \| on_model(ContactId) \| on_column(ColumnId, z)`. `connect` requires the carrier to be printed no later than the member, checked layer by layer at the printer's layer height, including horizontal braces. A graph with no cycles is needed but is not enough. |
| **H4** lift | Owned by construction of the posed model. Given the immutable input mesh and rotation, compute the translation from the rotated bounds, round to the native representation, and verify the actual lowest emitted model point is at least 5.0 mm above the plate. Admission of raft and support bases also preserves the prohibition on model contact with them. A scalar translation of 5.0 mm is not itself proof of lift. Nothing downstream can lower the model. |
| **H5** drains | Pose, hollow and drain planning returns a proven drain-and-vent result, or an explicit failed or unverified result. Every support candidate is tested against the exact drain-bore keep-out shape. |
| **H7** fidelity | The input mesh is kept unchanged with its hash. Only approved repairs run, before planning, each recorded. The model is frozen before contacts are made. A later model change means a new plan, not moved contacts. |

- **Additions keep earlier guarantees.** A branch, brace or join is admitted only if its full native shape keeps every admitted patch, reserved region and gap. A refused addition never enters the plan.
- **FR-19 exemptions** (sealed cavities) come from a checked cavity classification, never from a caller's `inside = true`. Those contacts still obey H2, H3, H5, H9 and the load model.
- **The plan moves only forward.** `PlanBuilder<Contacts>` → `PlanBuilder<Routed>` → `SupportPlan` (frozen). Each step consumes the last. `plinth-emit` takes `&SupportPlan` and invents no geometry. There is no API to edit a finished plan, so a late repair pass cannot be written without changing these types, and that shows in review.
- **Low-level records.** `plinth-chitubox` still needs raw records for reading any native file and for writing broken test files. The generator's normal path never builds raw support records outside `plinth-emit`; a grep in the gate enforces this.

### 7.3 Failure and publishing (FR-31, FR-42)

- If no valid support can be built for a region, leave it out and report the located gap. Never break a limit to look complete.
- A model that cannot be made printable gets a clearly named **diagnostic** file, listing every failed and unverified rule.
- Write to a temp file on the output volume, close it, run the checker on those bytes, then rename it into place with its pass or diagnostic status. Finished earlier models are kept. After cancel, crash or disk error, no file with a final name looks complete.
- The format library never reports "ChituBox-valid" because its own reader accepted a file. That takes H8.

### 7.4 Proof and guards

- **Compile-fail doctests (T1c)** show these do not compile: `PatchArea(0.9)` outside its module; `plan.get_mut(id)`; a column built without an admitted clearance result; `Section(0.05)`; editing a `SupportPlan` after `freeze`.
- **Property tests (T0p)** feed random candidate batches on Round 25 and synthetic shapes near the limits: tangent faces, close parallel sheets, narrow crevices, shared feet, branch joins, f32 rounding edges. Late joins that would enlarge an admitted patch must be refused.
- **Lints** (`clippy.toml`, `[workspace.lints]`), both workspaces:
  - `too-many-arguments-threshold = 6`, `too-many-lines-threshold = 120` (deny).
  - `disallowed-methods`: `std::env::var`, `std::env::var_os`. Only the CLI's argument module may read the environment. This stops hidden settings like the old 407 `PLINTH_*` variables.
  - `disallowed-types`: std `HashMap` and `HashSet` in generator and checker. Use `BTreeMap`, sorted `Vec` or index maps, so iteration order cannot change output.
  - `unsafe_code = "forbid"` everywhere except `geom::simd`.
- **Review signal.** The gate flags any new function in `support` named like `*repair*`, `*fix_up*`, `*grow*` or `*move_contact*`. The reviewer must then name the step that creates the quantity.

### 7.5 H8 stays separate

H8 is a record that ChituBox v1.4.0 opened, showed and sliced the **exact file** (by hash). Tests, round trips, our own pictures and archived samples never pass H8. Record the app version, file hash, what was seen and the outcome. Ask the owner before driving ChituBox by screen control (PRD §11).

---

## 8. Performance hooks (PRD §9, §10)

**Batch-first.**

- Every public geometry query takes slices and fills a caller-owned output slice, for example `closest_points(&bvh, &queries, &mut out)`. There is no single-point public query.
- In debug-assertion builds, each batch function counts calls and items. T2 fails if Round 25 averages under 64 items per call. This catches per-item calls in a hot path.

**Parallel work with a thread cap (NFR-9).**

- The CLI takes `--max-threads` (default 8). It builds one Rayon pool and runs each model inside it. No global pool, no nested pools.
- Work is split into coarse tasks only: ≥ 1,000 items or one layer band per task. (The first attempt measured tiny tasks running 1.9x slower.) Tune on measurement.
- Models run one at a time, each with the full pool.
- Generation, checking and rendering run one after another. The generator drops its large data before the checker starts.

**Same output at any thread count (NFR-2).**

- Parallel steps write to indexed slots and collect in order. Float sums are reduced in a fixed order (M11: identical results at 1, 3 and 8 threads).
- Fixed model order, stable candidate IDs, sorted commits, fixed tie-breaks.
- No hash-order iteration (lint above), no wall-clock cut-offs, no unseeded randomness, no "first result wins".
- Records are sorted before writing.
- Timings and other run data go in a separate record, never in the output file or the measured part of the report.
- T2 compares Round 25 bytes at 1 thread and the slot's allocation, and at 8 threads in the bench epoch.
- A speed change that changes output is accepted by a similarity check on the written file, not byte identity with the old version (PRD §10).

**Cache locality.**

- Structure-of-arrays storage (`xs`, `ys`, `zs`) with `u32` indices.
- Triangles and vertices reordered along a Morton (Z-order) curve after loading.
- A flat BVH of 32-byte nodes, built once per frozen mesh.
- Layers are bitsets, streamed through a bounded window. No dense full-printer 3D grid, and no full raster per layer kept for the whole model.
- One arena per step, freed when the step ends. Large sizes are checked with overflow-safe arithmetic before allocating.
- System allocator. Compare mimalloc only after the first credible Round 25, and only when profiling shows the result can change a decision. (The first attempt measured 386 million allocations on one model; cut allocation counts first.)

**SIMD / NEON.**

- `target-cpu=apple-m3` plus structure-of-arrays loops gets auto-vectorisation.
- Hand-written `core::arch::aarch64` code is allowed only in `geom::simd`, only for a measured hot kernel, each with a scalar reference function and a property test that they agree. No `std::simd` (needs nightly).
- f64 for every decision. f32 only at the writer (after rounding in section 7.1), or in a bulk kernel with a written error bound.

**Memory (NFR-4).**

- Report the long-lived CLI's lifetime peak (`getrusage`; on macOS `ru_maxrss` is in bytes) as a job-level quantity, never as an individual model's peak. Obtain a fresh per-model measurement with a short-lived model worker or an explicitly scoped external measurement. (`/usr/bin/time -l` printed no memory line inside the sandbox.) If using process high-water marks, state the process tree and combine them conservatively: include the coordinator and every process that can coexist. A "largest child" value is sufficient only where child execution is strictly sequential and grandchildren are either absent or separately accounted for.
- Keep memory reporting separate from prevention. Constructors and workspace allocators check byte counts with overflow-safe arithmetic and admit work only within the job's remaining budget; unknown or unbounded allocations cannot be treated as covered by that accounting. Test a deliberately small budget through parse, generation, checking and rendering; each oversize request fails cleanly with a located diagnostic. Where total process memory is not strictly enforceable, report that limitation explicitly rather than claiming the post-run `getrusage` check enforces the ceiling.
- T2 fails above 1 GB job peak for the Round bases. L5 reports against the 4 GB per-model target and the 100 GB job ceiling. The 100 GB ceiling is a defect line, not an allowance.
- Limit work that grows faster than model size, not large models.

**Cancel and progress (NFR-3).** A cancel token is checked between batches; target under 1 s. Progress runs on a timer, never a log line per triangle or contact.

**Counters before tuning.** Each step records its time and item counts (queries, candidates, rejections) through thread-local counters summed at step end, kept out of the hot path. A profiler (samply) is used at L5 only. Before the treant, time and memory are measured while varying triangle count, height, cavities, narrow gaps and support count separately (PRD §8.2 stage 6 gate).

---

## 9. Lint, format, hooks, gate, signing

**Committed config:**

- `rustfmt.toml`: edition 2024, defaults otherwise.
- `[workspace.lints]`: rust `unsafe_code = "forbid"`, `unused_must_use = "deny"`, `missing_debug_implementations = "warn"`; clippy `all = "deny"`, `float_cmp = "deny"`, `cast_possible_truncation = "warn"` in `plinth-chitubox` and the checker.
- `clippy.toml`: the thresholds and disallowed items in section 7.4.
- One default path, which is the tested path. No feature flags without a named owner and a removal date (PRD §10).

**Hooks.** `scripts/setup` sets `core.hooksPath = .githooks`. It is the documented first command after a clone. Hooks are a convenience; the gate works without them.

- **pre-commit** (≤ 5 s; fails loudly):
  - `cargo fmt --all --check` in both workspaces (M18: 0.6 s);
  - `scripts/check-independence`;
  - a grep of the staged diff for: `#[ignore]`; `env::var` outside the CLI; `allow(clippy::too_many_arguments)`; `todo!()` or `unimplemented!()` outside tests; any `.rs` file over 1,500 lines; any change under `test-data/` or `reference/` (read-only evidence).
- **pre-push:** passes only if `target/gate-log.tsv` has a passing gate row for the exact tree being pushed (`git rev-parse HEAD^{tree}`, clean tree). Otherwise it runs `scripts/gate`. A stale pass does not count. It also checks signatures (below).

**`scripts/gate`** (≤ 180 s, background protocol), run by the orchestrator before every merge, for both workspaces:

1. `cargo clippy --workspace --all-targets --locked -- -D warnings`.
2. L2 fast suite, including doctests run explicitly.
3. L3 Round bases.
4. Independence check, provenance check of test-data, and the raw-record grep (section 7.2).
5. When build configuration changed, require a passing loop-bench record produced separately in an exclusive bench epoch for the exact source and configuration under review. The gate verifies that record's tree, toolchain, configuration, fixture and stage-manifest identities; it does not run the benchmark inside its 180 s deadline. Missing or stale evidence is a setup failure naming the benchmark that must be scheduled.
6. Signature check: `git log --format='%G? %h %s' main..HEAD`. Any `N` fails. `gpg.ssh.allowedSignersFile` is set so good signatures show as `G`.

There is no hosted CI: GitHub's macOS runners are not this M3 Mac. At milestones, `scripts/gate --clean` clones into `$TMPDIR` and runs the Cold level with fresh build folders, to prove a clean checkout works (NFR-8).

Every input required by L2 and L3 is available inside a fresh clone. Before enabling L3, add byte-identical copies of the three original Round STLs under a new committed `fixtures/round/original/` directory, with source names and SHA-256 hashes. Store the owner-approved tilted copies under `fixtures/round/approved/`, with the rotation, original and derived hashes, keep-up markers and approval record. Do not alter `test-data/` or `reference/`, and do not substitute the old as-drawn copies (PRD FR-11a excludes them). Tests resolve these files relative to the checkout, never through the owner's sibling directories. The clean-clone gate must succeed with those sibling directories unavailable. Later-stage corpus inputs receive the same explicit provisioning before entering a required clean-checkout test.

Dependency licence and advisory checks (`cargo-deny`) run only when a dependency is added or changed, outside the offline inner loop.

**Signing.** `commit.gpgsign = true` and `gpg.format = ssh` (1Password) are set. Agents do not commit; the orchestrator signs and merges. No script or brief may use `--no-gpg-sign`, `-c commit.gpgsign=false` or `--no-verify`. If 1Password is locked, stop and ask the owner.

---

## 10. Skeleton build plan

Each step ends with a measured check. A miss stops the step.

**Tools.** Already present: hyperfine, cargo-sweep, cargo-nextest 0.9.150, jq, rustfmt, clippy. The crates `proptest`, `lexopt` and a JSON crate need one network fetch. Later, only if needed: `cargo-deny`, `cargo-machete`, `cargo-llvm-lines`, `samply`. Do **not** install lld, mold, PGO tools, Python environments or build-folder seeding tools.

| Step | Build | Check (budget) |
|---|---|---|
| 1 | Root files: `Cargo.toml` with profiles and lints, `rust-toolchain.toml`, `.cargo/config.toml`, `.gitignore`, `loop-budgets.tsv`, `limits.toml`, `scripts/setup`. The 6 main crates and the `checker/` workspace, empty, each with `loop_canary.rs` | Cold build + tests of both workspaces ≤ 60 s at -j 8, fetch timed separately. Lnoop: ≤ 2 s and 0 `Compiling` lines. Apple linker in use. |
| 2 | `scripts/slot`; worktrees s1, s2, s3, bench in `plinth-slots/`, checked out at committed `develop` (or a branch from it), never at an untracked copy | s1 and s2 build at once with **no** wait on a slot's build-directory lock (no `LOCK WAIT`). Waits on Cargo's shared package-cache lock are acceptable in any state: they are counted, timed and reported, and fail only if the total in one command exceeds 2 s. A slot switched to a new branch rebuilds incrementally in ≤ 10 s, not cold. Changing the thread allocation causes 0 rebuilds. L1 at -j 2 measured. Compute concurrency and process/thread counts measured with two and three slots (section 5.6). |
| 3 | `tools/devloop` (Rust runner: `t`, `loop-bench`, `slot`, `setup`; unit tests, clippy and fmt clean) behind the bash shims `scripts/t`, `scripts/loop-bench`, `scripts/slot`, `scripts/setup`; `.config/nextest.toml`; install nextest | All currently implemented levels pass with real evidence. At this step, validate Lnoop, L0 and L1 plus wrapper failure handling. L2 is enabled as its suites arrive; L3 is enabled only when step 7 provides the real transport path. An unavailable level exits with a setup error and is never logged as a pass. **Sabotage:** a 20 s sleep in a unit test: nextest kills it, `scripts/t L1` exits nonzero. A 130 s sleep in a non-test child process (so nextest does not kill it first): the supervisor stops it at the 120 s bound, reports a timeout, and its recorded process group is gone. A test filter that selects 0 tests: fails. A held Cargo lock: reported promptly as LOCK WAIT. Remove all four. |
| 4 | `plinth-chitubox` reader and writer from the native samples. The old `chitubox_io` may be reused only choice by choice: it targeted v1.3.0, the samples are v1.4.0, and its lift, raft and support guesses need native proof | T1 on every native file: parse, byte-for-byte rewrite, sample-pair field changes, malformed-input properties. L1 for chitubox ≤ 5 s. L2 ≤ 30 s. |
| 5 | `plinth-check` skeleton in `checker/`: own reader, own STL reader, `dump`, JSON report with every rule "unverified" until built; `scripts/check-independence` | Readers-agree test passes on native files. **Sabotage:** a path dependency on `plinth-geom` in the checker makes the independence check and pre-commit fail. L1 for the checker ≤ 5 s. |
| 6 | `plinth-geom`: STL read, structure-of-arrays mesh, bounds, Morton reorder, first batched query (closest point), batch counters | T0 + T0p (batch equals scalar reference; same output at 1 thread and the slot's allocation; at 3 and 8 in the bench epoch). L1d (geom edit → support tests) ≤ 6 s. |
| 7 | `plinth-emit` + `plinth-cli` transport path: original Round STLs (committed under `fixtures/round/original/`, section 9) → lifted model → .chitubox (model only) → temp file → `plinth-check` → publish; `--max-threads`; report with time per step and peak memory; interrupted-write, full-disk, bad-input and name-collision tests | **Go/no-go (PRD §10).** L3 ≤ 90 s from edit (target ≤ 30 s). L1 ≤ 15 s. Real recorded edits in the format reader/writer, geometry, emitter/CLI and checker that exist at this stage, including a library edit that rebuilds the CLI (section 3.3), all within budget; no dummy engine code is created to fill a category. Identical bytes at 1 and 8 threads (bench epoch). Job peak memory printed and ≤ 1 GB. Compute concurrency and process/thread counts measured and reported per section 5.6. Checker says "unverified" or "not applicable" for unbuilt rules, never "pass". Transport files are labelled "not a print candidate". Report all runs and maxima to the owner. **If missed after one fix attempt, stop and propose a language switch.** |
| 8 | `plinth-acceptance` broken files (rules not built yet must report unverified); CLI-blocks-candidate test; `scripts/gate`; `.githooks` | Each broken file is rejected by its rule, or reported unverified. Pre-commit ≤ 5 s. Gate ≤ 180 s. `scripts/gate --clean` passes within the Cold budget. |
| 9 | **Release-path correctness.** Build and run the transport path in release mode | Release build time recorded. Keep the system allocator and LTO off. Defer allocator and LTO comparisons until after the first credible Round 25, and perform them only when profiling identifies a decision they can change. They do not block contact work or owner review. |
| 10 | Native contact shape from the samples (ball, neck, cone, joins, raft, rounding). Tilted keep-up copies of the Round bases and labelled underside and rim pictures; **owner confirms the angle before support work** (FR-11a; Round bases confirmed 2026-10-30 at 65°) | Shapes not yet settled by a native file fail as unverified, never pass a guessed reconstruction. L1 and L2 still in budget. |
| 11 | **Round 25 construction and independent checking.** Implement the native shapes established in step 10, the construction operations in section 7, and the load model required by FR-20a. Independently implement checker measurements for H1–H4, H6, H7 and H9 on those shapes. For H5, verify that the emitted solid Round base has no enclosed cavity before reporting "not applicable"; do not infer this from its filename or generator status | Measurement uncertainty established on native examples and analytic cases. For every applicable rule: a positive control, a production-writer negative control, and a normal-CLI test proving rejection blocks candidate status. An unimplemented measurement is unverified and cannot complete this step. T1c compile-fail doctests fail to compile as intended. T0p: 0 admitted patches above H1 minus margin, 0 fused pairs, every rejection path hit. Go/no-go measurements repeated with real construction and generator edits (section 3.3). L1 for support ≤ 5 s. |
| 12 | **First printable Round 25.** Generate the approved tilted keep-up Round 25 with real supports and a connected raft | The emitted-file checker reports zero violations and zero unverified applicable rules; the FR-20a/FR-21 report and review pictures exist. L3 ≤ 90 s. The orchestrator inspects the written output and pictures, then seeks H8 and the owner's judgement immediately (PRD §8.1). Ask before printing. Round 50 and Round 100 follow this first milestone; they do not delay offering a credible Round 25. |

**Standing rules after the skeleton:**

- Bench at every slice end; report maxima with the previous values.
- Split a crate when the size trigger fires (section 3.3).
- Toolchain and profile changes go through main, then a bench run.
- At most 3 writing slots.
- The bash shims stay ≤ 400 lines in total (≤ 10 lines each). Logic belongs in `tools/devloop`, which keeps its own unit tests and stays clippy and fmt clean. Growing a shim past its cap is a stop-and-review signal.

---

## 11. Open risks and guesses

- **Compile cost per line comes from synthetic code (guess).** Real generic geometry code may be slower. The bench from step 4 onward replaces the prototype numbers.
- **-j 2 cost is not measured.** With 3 writing slots each gets 2 threads. Step 2 measures it; if L1 misses at -j 2, cap writing slots at 2.
- **nextest process start cost on macOS is not measured** (guess: 5 to 20 ms per test). If L2 suffers, group tiny tests.
- **Two workspaces mean two build folders per slot** (more disk, a second lock). Not measured; step 2 checks the sizes.
- **The H1 acceptance margin depends on the maximum excess of the checker's reported upper bound over the true area** (up to 2ε for a symmetric error radius ε; section 7.1), which must first be shown on native contacts and analytic cases (PRD §6.2). Until then it is a labelled orchestrator starting value.
- **Budgets and guards are design, not demonstrated.** No Rust code exists yet. Do not describe any budget, wrapper bound or construction guard as operationally enforced until the skeleton steps and their sabotage checks show it.
- **The real risk is measuring native support–model intersections conservatively,** not the Rust types. Settle it on the native samples and Round 25 before generalising.
- **Thin LTO, release `overflow-checks` and mimalloc are not measured.** Compared only after the first credible Round 25, when profiling shows a decision depends on them.
- **The 90 s end-to-end budget is unproven.** The prototype timed no real generation, independent geometric checking or rendering; the 15 s targeted-build budget is plausible from the synthetic results only.
