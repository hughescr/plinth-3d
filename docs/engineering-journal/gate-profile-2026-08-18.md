# The `gate` build profile — 2026-08-18

> **Provenance.** Written 2026-08-18 against `develop @ d28e35e`, on branch
> `feature/gate-profile`, in the primary checkout (no worktree — serial lane, reuses the warm
> `target/` and sccache). Task #44. **No cap, limit, threshold, tolerance, gate or registered
> constant is raised, lowered, widened or bypassed by anything in this change.** The only diff is
> a new, additive Cargo profile plus this note.

## Purpose

Full release test gates take minutes of max-CPU compile because cargo's `release` profile
disables incremental compilation, so every one-line change during a lane's iterate-and-test loop
recompiles whole local crates at `opt-level = 3`, then links the workspace's ~104 test binaries.
The heavy physics fixtures (e.g. the committed-treant tests) are `#[cfg_attr(debug_assertions,
ignore)]` — they only run with debug assertions off, which until now meant `--release`.

`gate` is a profile with release **semantics** — `debug-assertions` off, `overflow-checks`
matching release (both inherited unchanged from `[profile.release]`) — but cheaper compiles:
incremental compilation turned on, LTO off, more codegen units, and `opt-level = 2` (matching the
existing `[profile.test]` profile's choice). Because every numeric pin and digest frozen in the
suite is a test assertion, a green run under `gate` is itself the proof that no pinned value moved
under the cheaper codegen.

## The profile as committed

```toml
[profile.gate]
inherits = "release"
incremental = true
lto = "off"
codegen-units = 16
opt-level = 2
```

No `[profile.gate.package."*"]` override was added. `lto` and `incremental` are top-level-only
profile keys — Cargo does not allow overriding them per package — so there is no way to pin
dependency crates to reuse `release`'s cached rlibs while `gate` differs from `release` on `lto`
(`"off"` vs `"thin"`). That single difference already changes the rustc invocation (embed-bitcode
codegen) for every crate in the dependency graph, dependencies included, so a package override
would not have bought back any cache reuse — it would just have been dead configuration. **Verdict:
dependencies rebuild once into `target/gate` on first use, then stay warm for every subsequent
`gate` build**, same as any other custom profile's first run.

## Trust verification (the load-bearing step)

`cargo test --workspace --profile gate --offline`, run twice in the foreground (once cold, once
warm):

- **1,386 passed, 0 failed, 2 ignored** — both ignores are the local-STL harnesses
  (`local_minis_zero_volume_repair`, `round_50_and_round_100_rasterize_under_the_unchanged_production_cap`)
  that read uncommitted `minis-to-process` STLs and are skipped in every profile, not just `gate`.
- This is the exact release-gate result. No test needed retuning; nothing was papered over.

## Timing

All times are wall-clock, foreground, single machine, this session:

| Scenario | `--release` | `--profile gate` |
|---|---|---|
| Full workspace suite, cold (first build) | ~6 min (known baseline, not re-measured cold this run) | 5:22.57 |
| Full workspace suite, warm (nothing changed) | — | 2:17.04 |
| `-p plinth-hollow`, warm, no source change | 11.997s | 14.438s |
| `-p plinth-hollow`, after touching one file in `crates/plinth-hollow/src/lib.rs` (comment line added then removed, file byte-identical after) | 2:54.33 (174.33s) | 15.442s |

The touch case is the one that matters for a lane's iterate loop: because `release` has no
incremental compilation, a one-line change in a local crate forces cargo to recompile that crate
from scratch at `opt-level = 3` and relink its test binaries — **174s**. Under `gate`, the same
touch is an incremental recompile — **15s**, about **11x** faster. This is the whole point of the
profile: the expensive part of the release gate loop is the touch-rebuild-retest cycle, not the
once-per-lane cold build.

The `lib.rs` touch was `printf '\n// gate-profile timing probe\n' >> lib.rs`, timed, then reverted
with an `Edit` removing exactly those two appended lines; `md5` of the file before and after
matched (`ceff8a6dac0fe16c70a6f88d0280789b`), confirming byte-identical restoration.

## Clippy

`cargo clippy --workspace --all-targets --offline -- -D warnings` is clean (7.6s). Clippy is
profile-independent here, so this was a tree-hygiene check, not a `gate`-specific one.

## Standing rule

**Lane gates use `--profile gate`.** It has release semantics, so it enforces exactly the same
frozen numbers and ignore-gated fixtures as `--release`, at a fraction of the touch-rebuild cost.

**The merge gate to `develop` stays full `--release`.** `gate`'s cheaper codegen (`opt-level = 2`,
no LTO) is a compile-cost knob, not a semantics knob, and today's full verification run found no
divergence — but the merge gate is the one that has to catch a divergence if one is ever
introduced, so it keeps the codegen configuration that actually ships.

## What surprised me

- `profile.gate.package."*"` cannot rescue dependency-artifact reuse across profiles here, because
  the setting that actually forces a rebuild (`lto`) is a top-level-only key with no package-level
  override. This is worth remembering before reaching for package overrides to "keep deps cached"
  in a future profile — check which keys are even overridable per package first.
- The `-p plinth-hollow`, no-change, warm-vs-warm baseline was close either way (12.0s release,
  14.4s gate) — the profiles cost about the same when nothing needs rebuilding. The entire win is
  in the touch-rebuild path (174s vs 15s), not steady-state test execution.
