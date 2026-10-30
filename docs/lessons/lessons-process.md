# Process and dev-loop lessons from plinth (read-only research, 2026-10-29)

Scope: how the work was run, what it cost, and how the next project's loop and method should start.
Sources: memory dir `~/.claude/projects/-Users-craig-code-hughescr-dnd-campaigns/memory/` (cited as `mem/<name>`),
`plinth-3d/` (Cargo.toml, `.cargo/config.toml`, `scripts/`, `docs/development.md`, git log), `plinth-evidence/`
(cited as `ev/`), and `wt-*` worktrees. Numbers marked (est) are my estimate. Nothing was built or run.

## 0. The project in numbers

- 2,098 commits in 100 days (2026-07-20 to 2026-10-27), 21 per day. 296 are merge commits. Peak days: 154 (08-01), 128 (10-15).
  Per ISO week: W30 157, W31 310, W32 331, W33 92, W34 150, W35 169, W36 30, W37 3, W40 183, W41 147, W42 250, W43 255 (`git log`).
- 443,624 lines of Rust in 476 files, 15 crates. 20 files hold 7,000 to 15,900 lines each (`crates/plinth-support/src/specialized.rs` 15,889;
  `perception.rs` 14,229; `slenderness.rs` 12,083). The placement core alone is about 118k lines (`ev/perf-2026-10-14/validator/RETHINK.md` section 3).
- 72 test binaries, 2,748 tests (`docs/development.md` line 144). `scripts/known-reds.txt` lists 37 tests that are red on develop.
- 52 `wt-*` worktrees, 519 GB on disk. The disk is 90 percent full (202 GB free of 1.9 TB). Main `plinth-3d/target` is 33 GB.
  `plinth-evidence/` is 75 GB, of which 45 GB is `perf-2026-10-14/validator`. There are 58 `for-craig*` review folders.
- Craig's anger ledger (all from mem): 09-30 "snail's pace", 10-06 "spinning wheels", 10-13 "TREMENDOUSLY slow", 10-14 "wtf",
  10-21 "THINK about efficiency", 10-22 "What the fuck are you doing", 10-28 "motherfucking jesus christ", 10-29 "completely unable to
  produce a working product after MONTHS". The complaint repeats. Each time the fix was a new rule in a brief, not a change in method.

## 1. What went wrong, with evidence

### 1.1 The product metric was a lie, then it was a different lie
- 08-30: "24 of 37 models emit" while about 0 would print (`mem/print-candidate-metric`). Spacing constant `TARGET_SPACING_MM = 4.0` governed
  nothing (median measured 1.03 mm). `MIN_SHOOT_R_MM = 0.35` was "an unmeasured placeholder" on 36 to 54 percent of arms.
- 10-29: "FU 0" counted only model-surface patch fusion. Craig asked "How can a tip be inside another support but there's no fusing?"
  A narrow counter stood in for the real requirement (`mem/stop-and-review-2026-10-29`).
- 10-22: a lane's "0.8 mm cap" left 1.70 and 2.19 mm contacts in the file. The orchestrator relayed it to Craig (`mem/limit-means-hard-limit-in-file`).
- 10-07: "0.96 mm neck" came from planned parameters; the file had none (`mem/measure-from-emitted-file`).
- Pattern: the orchestrator picked a count (CS, FU, TR, unbraced, arms, resin), steered lanes by it, and reported it. Craig's question is
  always "would I print this file". Nobody asked it before presenting.

### 1.2 Limits were retrofitted, not constructed (the largest single waste)
- Tip size came from load with no width limit (own rule of 09-30, `mem/rulings-2026-09-30-oversupport`). The 0.8 mm contact cap arrived
  on 10-22 and was bolted on late: pcap rounds 1 to 8, then CBT (0 of 88 fixes feasible), then cr1, then cpa1 to cpa6.
  `ev/perf-2026-10-14/validator/` holds pcap, pcap2..pcap8, cbt0, cbt0b, cbt0c, cr1, cpa, cpa2, cpa3, cpa4, cpa6 as separate lane folders.
- The outside review (`validator/RETHINK.md` section 1.3): "Moving a contact under a finished tree is the wrong order; ChituBox never does it."
  Round_50 went from 198 arms to 340 and 466 (physics needs about 223), down passes 4 to 24, run 10.7 s to 23.9 s (`validator/RESULTS-footprint.md`)
  and 11 s to 102 s (`mem/stop-and-review`). About 15 "contact mover" passes still existed after creation (`RETHINK.md` section 1.2).
- Astra's own verdict on the 8 rounds (`validator/CAP-CONVERGENCE.md`): three constraints were solved separately although they depend on each other.
- The rule Craig stated: a hard limit is an input at the step that creates the quantity (`mem/limits-are-construction-rules-2026-10-28`).
  The same lesson was in `mem/physics-first-constants-doctrine` (08-19) and `mem/emit-and-warn-never-refuse` (09-01). Rules were recorded; the design did not change.

### 1.3 Stop-and-respawn loops and lane sprawl
- Lanes were narrow, stopped at the first miss, and were respawned with a new brief: pcap 1 to 8, cpa 1 to 6, up1 a to f, s1b with 2 gates and
  2 re-reviews, val/val-gate/val-review/val-challenge, wall x5, gap x6. The live agent list in this session alone has 55 names.
- Each lane adds code and a signed WIP/rejected branch (`mem/plinth-state-2026-10-21`: trd PARKED, hcl REJECTED as WIP, contact-union PARKED on a wip ref,
  cr1 "reference material, not the base"). Parked and rejected work is still carried: 59 branches, 52 worktrees.
- Worktree sprawl: of 52 `wt-*`, 41 have a last commit on 10-15 or 10-16, 14 days ago (30 on 10-15, 11 on 10-16). Largest: wt-cs 29 GB, wt-cpa 22 GB,
  wt-spec 20 GB, wt-pcap 19 GB, wt-cr1 18 GB. This repeats the 07-24 ruling "worktrees are ephemeral, merge and delete" (`mem/worktree-lifecycle-and-project-layout`);
  on 07-24 48 worktrees already inflated the sandbox deny-list until every Bash call failed with E2BIG.
- 10-21: five lanes ran at once, each running treant (17 to 40 min), Giant_Bat (6 to 30 min), solo timing, and the full gate; load reached about 100
  (`mem/iterate-small-verify-at-end`). 10-14: Deinonychus ran 11 times in one round, 58 to 75 percent of lane wall time was waiting (`mem/lane-model-run-budget`).
- Review spirals: plate-packing took 4 reviewer rejections over edge cases; seven-tip and hidden-cavity loops took 4 to 5 rounds each (`mem/manage-what-not-to-do`, `mem/stop-over-verifying`).
- Commit churn shows the loop: 76 of 2,098 subjects contain revert/drop/park/retire/remove-pass/delete; 339 commits in the last 14 days; "WIP: preserve rejected
  head-clearance candidate", "Revert the up-front pins: too broad", "contact union parked" (`git log`).

### 1.4 Reading summaries instead of the work
- A repair lane reported 13 commits; git held zero (`mem/agent-reports-need-git-ground-truth`). A prove lane measured a stale binary; `find -newer` passed on a
  stale binary (`mem/prove-runs-verify-binary-provenance`, `mem/binary-provenance-mtime-insufficient`). A witness model that never reached the changed path
  carried a "byte-identical" claim through two review rounds (`mem/witness-must-exercise-the-path`). 14 adversarial agents cleared 3 defects that one `/code-review` found
  (`mem/sabotage-proves-firing-not-sufficiency`). Design memos leaned toward the favourable reading in every round (`mem/design-framing-optimism-bias`).
- Fixtures built by agents passed the audit's own parser and all failed in ChituBox (`mem/verify-against-the-real-consumer`).
- The orchestrator relayed agent framing (cs "works on flat undersides, fails on single points") and passed through numbers it had not checked against the file.

### 1.5 Gate and test cost
- Whole-suite reruns about 85 times in 57.6 h; median code-change to model-file 32 min; 55 percent of wall time had a job running and no agent thinking;
  about 25 h of agent lock-waits in 2 days (`mem/fast-iteration-loop`, ledger 10-13).
- 10-13 baseline: `plinth-sweep` tests/it took 1,202 s in the lane loop; `gate-full.sh --skip-ignored` took 1,456 s; one ignored test (`a_spec6_removal`) needs 65 to 80 min
  on a quiet machine; the gate with ignored tests "did not finish in 140 min" (`docs/development.md` lines 137 to 145).
- Pinned-output tests drift on every behaviour change (head_carrier_tests 3,378 lines, lip_draw_tests, validator_tests). 10-23: the full slow gate found 17 tests already red on
  develop; "anti-vacuity: sweep paths went quiet" (`mem/plinth-state-2026-10-21`). 10-28: two new reds in twin_merge found only by the full gate. The lane loop could not see them.
- Policy flip-flop (all from Craig): test every lane (08), then per round (09-30), then builder-only (10-04), then verify at the end (10-21). Each retune cost a memory file and a brief.

### 1.6 CPU lock and tooling that grew into a project of its own
- `cpu-lock.sh` has 316 lines and five archived predecessors (`.pre-flock` 568, `.pre-rw2` 504, `.pre-rw` 330, `.pre-shared-default` 259, `.pre-flock2` 238) in
  `ev/slice-sweep-2026-09-02/field-2026-09-28/perf-2026-10-09/`. It went through mkdir, then flock/perl plus a turnstile, with two GPT review rounds (`mem/cpu-lock-timing-solo`).
  Only timing runs needed exclusion; the first design locked every job exclusively and wave 1 lost hours.
- The lock blanks `RUSTC_WRAPPER`, so locked builds bypass sccache (`mem/cpu-lock-timing-solo`, `docs/development.md` line 95). The one cache that could speed up builds is off exactly where builds run.
- TaskStop left cargo children holding the lock for 40 min; `cpu_lock_status` showed it free, only `lsof` found it (`mem/taskstop-leaves-children-running`).
- Agents ended their turn "waiting" on backgrounded commands at least 5 times, two with 0-byte logs marked verified (`mem/agents-poll-dont-wait-protocol`).
- A seed target for new worktrees was silently stale for 6 days because `-C target-cpu=apple-m3` changed every dependency hash; every new worktree built all deps cold
  (`docs/development.md` lines 100 to 110). The fix was another script plus a fingerprint selftest.
- Workflow runtime traps: resume skipped an interrupted agent; resume rewound to Plan; SendMessage forked a racing copy; gpt-sol in a verify slot spawned `/code-review` children and was retried 4 times
  (`mem/workflow-resume-skips-interrupted-agent`, `mem/workflow-resume-verify`, `mem/sendmessage-forks-workflow-agents`).

### 1.7 Communication with the owner
- Jargon (FU, CS, TR, HS, SL, M92, "twin raise", "pcap round 6") with no definitions. Craig asked for ASD-STE100 on 08-13 (`mem/ste-simplified-technical-english`); the lesson was "drop insider vocabulary".
  By 10-28 the state notes in `mem/plinth-state-2026-10-21` are again a stream of codes, shas and lane names.
- Wall-of-PNG reviews that "meant nothing" to him (`mem/review-pictures-self-explanatory`); X/Y coordinates he cannot read in ChituBox (`mem/locate-by-picture-or-anatomy`).
- 58 `for-craig*` review sets. He said on 10-24: "I don't care about interim snapshots" (`mem/iterate-small-verify-at-end`).
- Rulings from Craig were stored but not turned into design. The 10-28 tip-angle ruling and the 10-28 limits ruling came after weeks of the opposite design (`mem/tip-approach-angle-ruling-2026-10-28`).

## 2. What worked (keep)

1. Measure only from the emitted file (`mem/measure-from-emitted-file`), with an independent reader that shares no types with the engine: `tools/plinth-audit` is a separate
   workspace by design (`Cargo.toml` lines 2 to 10). The audit lane's S1 (file read-back of CS/FU/DK, 10-23) is what exposed the late-fix problem.
2. Negative controls and sabotage tests, plus the second question "what is the smallest real defect this check cannot see" (`mem/defects-climb-the-stack`, `mem/sabotage-proves-firing-not-sufficiency`).
3. Binary provenance by a second no-op `cargo build` with zero `Compiling` lines (`mem/binary-provenance-mtime-insufficient`).
4. Cross-family challenge on designs. Opus REJECTED astra's late-repair design on 10-28 and set the S1a to S6 slice order. The outside RETHINK (Fable) found the actual root cause in one read-only pass
   with no builds, which eight lanes had not found. Pattern worth keeping: a read-only rethink by someone who did not build the code, after two failed rounds.
5. Known-reds with failure signatures (`scripts/known-reds.txt`): NEW-RED / CHANGED-RED / FIXED-RED verdicts stop a known name from hiding a new defect.
6. One-test-binary-per-crate (`tests/it/main.rs`), memoised sweeps per test program, slow tests behind an env flag. These cut `plinth-sweep` tests from 1,565 s to 68 s in the lane loop (`docs/development.md` line 142).
7. Gate profile: incremental, LTO off, release semantics. Incremental edits build in 1 to 15 s (section 3).
8. Reading the user's real tool profile: Craig's ChituBox export (`chitubox-support-specs.cfgx`) described a sphere of fixed diameter along the normal. That one file explained the whole contact design (`RETHINK.md` 1.1).

## 3. Measured dev-loop costs

| item | value | source |
|---|---|---|
| Cold `gate` build of the workspace, lane worktree | 1 m 39 s (`apc-build.log`); clippy on gate 1 m 00 s | scratchpad `apc-build.log`, `apc-clippy.log` |
| First `cargo test --profile gate --workspace --no-run` in a new worktree | 60.3 s, then 32.4 s after seeding | `docs/development.md` line 138 |
| Cold build of the vendored C++ (Manifold, 19 translation units) | 19.6 s, then 6.5 s with parallel `cc` | `docs/development.md` line 137 |
| Incremental gate rebuild after a one-file edit | 2.8 s (slenderness.rs), 3.1 s (lower/junction.rs); was 10.1 s and 18.1 s | `docs/development.md` lines 139, 140 |
| Incremental gate rebuild plus targeted tests, observed in lane logs | 0.15 s (no change) to 15.7 s; typical 4.9 to 9.4 s | scratchpad `up1b-*.log`, `upf-*.log` ("Finished `gate`") |
| A rebuild with a wide dependent set (fusion tests) | 59.3 s | scratchpad `up1b-fusion.log` |
| Full `--release` rebuild | 49 s cold, 7 to 20 s incremental (10-13 ledger); 102 to 105 s clean on a shared machine for release and for release-max | `mem/fast-iteration-loop`; `docs/development.md` line 682 |
| `cargo test --release` rebuild | 57 to 92 s versus 3 to 5 s under gate | `mem/fast-iteration-loop` |
| `test-fast.sh --base HEAD` after a `plinth-verify` edit | 3 s alone, 183 s with dependents (pipeline, cli) | `docs/development.md` line 143 |
| `test-fast.sh --no-dependents plinth-sweep` | 68 s (was 1,202 s) | `docs/development.md` line 142 |
| `gate-full.sh --skip-ignored`, 72 binaries, 2,748 tests | 1,456 s (24 min); was about 6,550 s serial | `docs/development.md` lines 54, 144 |
| Gate with the ignored tests | not finished in 140 min; `a_spec6_removal` alone 65 to 80 min | `docs/development.md` line 145 |
| Heavy test fixture | one sweep of the laid tube 90 s on one core; about 130 of 276 sweep tests are that heavy | `docs/development.md` lines 73 to 78 |
| Deinonychus (mid-size mini), production | about 145 s on 10-13; 64 s after perf work; 40.6 s shared on 10-24; dense with pose search 769 s, pinned 696 s (different config) | `mem/fast-iteration-loop`, `mem/lane-model-run-budget`, `validator/RESULTS.md` |
| BCO | 113 s to 34 s after R0+R1 | `mem/performance-goal-2026-10-09` |
| Round_50 disc | 10.7 s before the cap work, 23.9 s to 102 s after | `validator/RESULTS-footprint.md`, `mem/stop-and-review` |
| Treant (429,120 faces) | 17 to 40 min per run | `mem/iterate-small-verify-at-end` |
| CPU lock | about 25 h of agent lock-wait in 2 days; model runs held 79 percent of lock time; 4 shared slots | `mem/fast-iteration-loop` |
| Time with a job running and no agent thinking | 55 percent of 57.6 h | `mem/fast-iteration-loop` |
| Median code change to model file | 32 min (10-13) | `mem/fast-iteration-loop` |
| Stale seed cost | 6 days of cold dependency builds in every new worktree | `docs/development.md` lines 100 to 110 |
| Run-to-run spread of one binary | 0.6 s on Deinonychus (63.6 vs 64.2 s) | `docs/development.md` line 639 |
| Disk | 519 GB in worktrees, 33 GB main target, 5 GB seed, 75 GB evidence | `du`, 2026-10-29 |

Reading the table: the compile was never the problem (Craig's guess, 09-30, was wrong; the ledger of 10-13 proved it). The costs were (a) waiting on whole-model runs, (b) test binaries that link and run for
minutes, (c) serialisation on a lock, (d) reruns of the same thing by several lanes. Build config now in place: `[profile.test] opt-level = 2`, `gate` = release minus LTO with 16 codegen units and incremental,
`-C target-cpu=apple-m3`, `RUST_MIN_STACK=8 MiB` (`Cargo.toml`, `.cargo/config.toml`). There is no linker override and no per-package `opt-level` split; link time was not measured (unmeasured).

## 4. Dev-loop design for the new Rust project

### 4.1 Workspace layout (fast incremental builds)
- 7 to 9 crates, cut by compile boundary, with a one-way dependency order. No file above 1,500 lines (plinth has 20 files above 7,000; one edit recompiles one codegen unit of that size). Suggested:
  `geom` (mesh, vectors, robust predicates, BVH, batched queries; no heavy dependencies), `field` (slicing, raster, demand), `contacts` (the contact constructor: pitch, cap, reservation, approach),
  `structure` (posts, braces, routing), `emit` (the `.chitubox` writer), `cli`, and, **outside the workspace**, `check` (independent reader and audit, no shared types, like `tools/plinth-audit`).
- Put the vendored C++ (Manifold via `cxx`) in its own leaf crate that nearly never changes, or drop it if the new design does not need booleans. It cost 19.6 s cold and `cxx` pinning; do not make it a dependency of the crate you edit most.
- Shared `[workspace.dependencies]`, exact pins, `Cargo.lock` committed, `--offline` builds (keeps the day-independent property that plinth had).

### 4.2 Profiles
- `[profile.dev] opt-level = 1, debug = "line-tables-only", incremental = true, split-debuginfo = "unpacked"` for own code.
- `[profile.dev.package."*"] opt-level = 3` for registry dependencies (rayon, kiddo, i_overlay, meshopt), and a per-package `opt-level = 3` for `geom`. Cargo allows `opt-level` per package; plinth put `opt-level = 2` on the whole `test` profile instead (`Cargo.toml` line ~47) and so paid for it everywhere (est).
- One lane profile, `gate` as now (release semantics, incremental, LTO off, codegen-units 16). Keep `release` = thin LTO only for milestone and timing builds. `release-max`/PGO: build it once at the end, never per lane.
- Target CPU: `apple-m3` as plinth (Craig's ruling, `mem/build-target-m3-allocator`). Add a global allocator experiment (mimalloc or similar) early; plinth saw 386 M allocations and 153 GB allocated in one Deinonychus run (`docs/development.md` line 649). Design data flow batch-first from day one (`CLAUDE.md` rule).
- Linker: default Apple linker; measure `cargo build --timings` once. Only if link is more than 3 s on `cli`, try `-C link-arg=-fuse-ld=lld` (unmeasured; do not assume).
- sccache: do not blank `RUSTC_WRAPPER` under any lock. Cacheable dependency builds should hit it. Start the daemon unsandboxed (`mem/sccache-daemon-sandbox-gotcha`).

### 4.3 Test runner and fixture sizes
- Use `cargo nextest` (not measured here; adopt and measure in week 1). Reasons: per-test timeouts, per-test process isolation, parallelism across binaries, JUnit/JSON output for a known-reds-style verdict.
- One integration-test binary per crate (`tests/it/main.rs`), as plinth did. Fewer binaries means fewer links (72 binaries in plinth).
- Fixture tiers, each with a hard wall-time budget (the budget is a test: a tier that exceeds it fails):
  - T0 synthetic, built in code: a flat plate, 30/45/60 degree plates, a ridge, a cone, a sphere, two close faces. Under 2,000 triangles. Each under 100 ms.
  - T1 the owner's discs (Round_25/50/100 equivalents) and one small figure. Each under 10 s in `gate`.
  - T2 two mid-size minis (a Deinonychus-class, a hollow Sphere-class). Each under 60 s.
  - T3 the big corpus (treant, Giant_Bat, BCO). Landing gates only.
- Test properties, not bytes. Plinth pinned outputs of the old pipeline; each behaviour change re-pinned thousands of lines. Pin a SHA for at most 3 models (T1 and one T2) and keep that list short and named.
- Every test that sweeps a shared fixture goes through one memoised harness (plinth did this; keep it).
- No test may be `#[ignore]` forever. Slow means a tier (T2/T3), not an ignore. A test that no run executes is deleted.

### 4.4 Per-agent target dirs and worktrees
- At most 3 concurrent writing lanes (`mem/manage-what-not-to-do`). Each lane gets one worktree and `CARGO_TARGET_DIR` under `~/.cache/<project>-targets/<lane>`, created from a seeded, dependency-only target
  (plinth's `.plinth-target`, 5 GB, via APFS clone). Delete the worktree and its target when the lane merges. A weekly script lists worktrees older than 3 days and fails loudly.
- The seed fingerprint check (`scripts/seed-check.sh`, `seed-fingerprint.sh`) was right; run it as the first command in the lane and make it abort on `!!` lines.
- Budget disk: worktree plus target is about 18 to 29 GB each in plinth. At 3 lanes that is under 100 GB, not 519 GB.

### 4.5 Edit-to-test budget (measurable, enforced)
| step | budget |
|---|---|
| Edit one crate, build `gate`, run that crate's T0 tests | 15 s |
| Plus T0 acceptance check on the written file | 20 s |
| Plus T1 (discs, one small figure) with the written-file check | 90 s |
| T2 (two mid-size minis) | at a checkpoint only, 3 min |
| Full workspace tests, no T3 | milestone, 10 min |
| T3 corpus, timing, release | landing only, 1 hour, solo |
Write the budget into a `scripts/loop-budget.sh` that prints the actual times; a run over budget by 50 percent for two days is a finding to fix before any feature work. Report these times in every status.

### 4.6 Tiny always-run acceptance suite on the written file
- A separate `check` binary that reads only the emitted `.chitubox`, shares no code with the engine, runs on every emit (CLI default-on, opt-out explicit and loud, `mem/checks-default-on-opt-out`).
- It prints one fixed summary, 11 lines or fewer. Each line states what it counts and what it does not (the "FU 0" lesson). The file-level hard limits are the headline:
  max model contact diameter at most 0.8 mm (feet included); any two contacts merged into one patch = defect (not "under 0.8"); minimum spacing; tip inside another member; tip cone inside the model;
  every member rooted on plate, model, or a member that is rooted; drain keep-out clear; support area against a load law; no member below the printer's resolution; the file opens (a golden file round-trip; ChituBox itself cannot be driven by agents).
- Exit non-zero on any violation. Emit-and-warn stays for the file; the check fails the run. Negative controls: each check has a hand-broken fixture that must fail it, built by the real writer (`mem/verify-against-the-real-consumer`).
- Runs in under 5 s on T1. This is the metric the orchestrator reports. The "would Craig print it" question maps to this list plus a picture (4.7).

### 4.7 Pictures and the owner's view
- One renderer in the `check` crate that draws a ChituBox-like view (supports in ChituBox colours, model grey) for a named region, before and after side by side, caption on the image.
  No picture for sub-visible changes (`mem/review-pictures-self-explanatory`).

### 4.8 Full-corpus run cadence
- T1 on every change that touches placement (about 90 s). T2 at the end of a slice (about daily). T3 and timing only when the owner will look at the files or before a merge to develop that changes output.
  Never from a lane per pass. One timing run at a time, run by the orchestrator, not by lanes. No "before" rebuilds.
- Replace the 316-line perl lock by a simpler rule: lanes may share the CPU; a timing run is requested from the orchestrator, who checks `uptime` load and runs it alone. If a lock is needed, 30 lines of `flock(1)` with `RUSTC_WRAPPER` untouched.

## 5. Working-method charter for the orchestrator

### 5.1 Never again
1. Never steer by a count I invented. A target needs an owner ruling, a physics derivation, or the file itself as its source; cite it. Before using a metric, write what it counts and what it does not.
2. Never add a hard limit as a later pass. Ask first: which step creates this quantity? Put the limit there. If a design adds a "repair" pass or a transaction over existing passes, reject it (`mem/limits-are-construction-rules-2026-10-28`).
3. Never present a result before I have checked the headline requirement against the emitted file myself (`mem/limit-means-hard-limit-in-file`). If it fails, send it back; do not relay.
4. Never relay an agent's framing. Re-derive the headline number from raw output, or say that I did not.
5. Never spawn the next narrow lane after a miss without asking "what do the last two misses have in common". Two misses in the same defect class = stop and ask for a design review (read-only, fresh eyes), not a third lane.
6. Never run more than 3 writing lanes, or leave a worktree older than 3 days. Never keep parked or rejected branches as a "reference base".
7. Never run the full corpus or timing per pass. Never run before-builds. Never wait on a backgrounded command by ending the turn (`mem/agents-poll-dont-wait-protocol`).
8. Never edit a test's pinned value to make it pass without naming the behaviour change that moved it.
9. Never write a state note in lane codes. State = what Craig can open, what it does, what is broken, in STE.
10. Never grow a tool (lock, seed, workflow script) past its need. Count the lines; the lock script took six rewrites.

### 5.2 How to report numbers
- Every number: value, unit, what it counts, what it does NOT count, the denominator, the source (emitted file, path, sha), and the same number for the previous state. Headline and bracket travel together
  (`mem/design-framing-optimism-bias`).
- One table per report with the same rows each time: the checker's 11 lines, times of the loop (4.5), size of the code (lines, files), count of open worktrees. Worse numbers go first.
- Report code size and run time with each slice. Code that grows while the file quality does not improve is a finding.

### 5.3 When to step back (any one of these)
- Craig expresses frustration about direction or speed. Stop all new lanes, read the evidence, and write a plain review (as on 10-29). Do not start the next lane first.
- Two lanes in a row on the same defect class, or 3 review rounds on one change.
- Run time of the standard witness rises more than 25 percent with no named cause (Round_50 10.7 s to 102 s should have stopped the work at 2x).
- Lines of code or pass count rises while the checker's headline does not move.
- A new rule from Craig contradicts an existing design assumption: the first action is "which design decision does this overturn", before any lane.
- A lane's report disagrees with git, the file, or the binary provenance.
- Per slice, define the stop rule in advance (as CAP-CONVERGENCE did: "if either round misses its boundary, STOP").

### 5.4 Method
- Start with the owner's real consumer. Read the ChituBox profile and the printer's behaviour first; the sphere-along-the-normal model is the design (`RETHINK.md` 1.1).
- Build the smallest vertical slice that writes a file the checker passes: a flat disc, then a slanted plate, then a figure. Every hard limit is a constructor argument from slice 1.
- Oracle before engine for physics only where a measurement exists; otherwise take the owner's tool as the baseline and justify each departure with a physics number.
- Delegation: one implementer lane (sonnet-high or gpt-sol) per slice, one verifier who runs the checker and the sabotage control, one read-only rethink after two failed slices. Challenger from the other family (CLAUDE.md pairing). Reviewers read the diff and the emitted file, and run only the lane's tests.
- Read the work, not the summary: open the output file or picture myself before each owner report; check `git log`, binary provenance, and the lane's own run count.
- Decide what NOT to run before each spawn: "what decision does this result change?" (`mem/manage-what-not-to-do`).

## 6. Ten-line summary

1. The process failed on method, not on compile speed: the 10-13 ledger shows 55 percent of 57.6 h with a job running and no agent thinking, and the compile (3 to 15 s incremental) was never the cost.
2. The biggest waste was retrofitting hard limits (0.8 mm cap, no fusion) as late passes: pcap 1 to 8, CBT, cr1, cpa1 to 6, then a Fable read-only review found the root cause (contact is an output, not a primitive).
3. The orchestrator steered by self-invented counts (24 of 37 emit, CS/FU/TR, "FU 0", resin) and relayed agent claims; the owner's question ("would I print this") was never the gate.
4. Lane sprawl is measurable: 2,098 commits in 100 days, 296 merges, 52 worktrees (519 GB, 41 untouched for 14 days), 59 branches, 58 review folders, 55 agents in this session alone.
5. Dev-loop costs measured: cold gate build 1 m 39 s, incremental 3 to 15 s, `test-fast` 68 s (sweep), `gate-full --skip-ignored` 24 min, one ignored test 65 to 80 min, treant 17 to 40 min, Deinonychus 40 to 145 s.
6. A 316-line lock script (five predecessors) plus a stale-seed incident (6 days cold builds) show tooling growing into its own project; the lock also disabled sccache where builds run.
7. Keep: emitted-file measurement, an independent checker, sabotage controls, no-op-build provenance, cross-family design challenge, known-reds signatures, gate profile, one test binary per crate, ephemeral worktrees.
8. New loop: 7 to 9 crates with files under 1,500 lines, dependency opt-level 3 and own-code opt-level 1, nextest, T0 to T3 fixtures, edit-to-test 15 s (crate) and 90 s (discs), at most 3 lanes with per-lane target dirs deleted on merge.
9. An always-on standalone checker reads only the written file, prints at most 11 lines with what each counts and does not count, fails the run on any violation, and is the one number the orchestrator reports.
10. Charter: limits are constructor inputs; no invented targets; verify against the file before presenting; stop after two misses in the same class; step back at any owner frustration; full corpus only at checkpoints.
