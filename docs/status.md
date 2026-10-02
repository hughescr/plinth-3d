# Status

Current state of the pipeline and the ordered work ahead. History lives in git; this page
does not narrate how things got here, only where they are.

## What the pipeline does today

`plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR` walks a folder
of STLs and writes one `.chitubox` project per model that clears the pipeline's pre-write
physics gates. Orientation defaults to a dense per-model search (`--orient dense`); a
previously-searched rotation can be replayed instead with `--orient pinned:DIR`, and
`--orient none` uses every model exactly as drawn. A single model is used as drawn under any
mode when it is named in `--no-orient=<list>` (file names or `*`/`?` globs; an unmatched name
is a warning), carries the `plinth-no-rotate` tag (macOS Finder tag, or Linux `user.xdg.tags`),
or has a `<stem>.asdrawn` marker beside it (GitHub issue #1). See
[`README.md`](../README.md) for the full command and [`docs/architecture.md`](architecture.md)
for the stage-by-stage pipeline this drives.

## Corpus state

**24 of the 37 real corpus minis emit a ChituBox file end to end** (MEASURED 2026-08-25,
corpus re-baseline at `develop @ 5d546cb`, all 37 models run — see the
"2026-08-25 corpus re-baseline" section of
[`docs/evidence/emission-canary.md`](evidence/emission-canary.md) for the ledger entry and
`dnd-campaigns/plinth-rerun-2026-08-25/RERUN-REPORT.md` (one directory above the
repo, outside git) for the full report).

**The prior pending-recount caveat is resolved.** Task #77 (the crevice-contact
seat-footprint ruling, see [`docs/physics.md`](physics.md)) was measured against the
2026-08-24 pre-#77 census and produced zero disposition regressions; the +2 gain over that
census (Tavey Badger, Tavey Base) are first-ever measurements, not recoveries. 24/37 is the
number of record at the current tip.

Of the 13 real minis that do not emit, as measured at the 2026-08-25 re-baseline:

| outcome | count | cause |
| --- | --- | --- |
| hit the 1500 s harness ceiling | 10 | the zero/low-commit coverage-repair loop — tasks #24, #37, #39 below; now the sole systematic blocker, gating 10-11 models |
| ceiling with an evidenced refusal past the ceiling | 1 | Awakened Tree — at the 1500 s ceiling it reads CEILING like census; run unbounded it terminates at ~2997 s with an evidenced `CoverageClosureRefusal` |
| crash | 2 | Giant Toad (swallowing) — `RootRunTooShort` at Lowering, task #29; `zombie-2-a` fails the final ChituBox disk audit (untracked — no task number assigned yet) |

**Zero models are refused by clearance rules.** Sub-0.300 mm true-gap residue is **0 in all 33
measured rows; 12 of 45 unmeasured** — see
[`docs/evidence/emission-canary.md`](evidence/emission-canary.md) for the instrument and the
census this reading comes from.

## Test estate

The only documented pipeline test reds are task #76's four (`a4_production_door` x2,
`first_artifact` x2 — see #76 below). The zero-field-divergence fixture is green, at its
eighth re-record (`develop @ 7eb5f30`, attributing its `Sphere` flip to `#77`'s ruled
footprint re-size).

**Two corners of the seal-semantics fix are landed but structurally untested**, carried
forward rather than closed:

1. **The empty-round-at-rank-0 seal hole.** If a confirmation round's slate were ever empty
   at rank 0, the seal logic would retire the terminal on an empty round. Unreachable while
   the frozen-set determinism argument holds; no witness exists either way.
2. **The forced-arm-survives-bracing branch.** A confirmation round whose forced arm
   *survives* bracing (rather than being condemned) has no test at the tournament level; no
   corpus model has produced one.

Both live in the `cco` seal logic named in [`docs/specs/seal-semantics.md`](specs/seal-semantics.md).

## Known limitations, ordered by how much of the corpus they block

1. **The zero-commit coverage-repair loop (tasks #24, #37, #39).** The dominant obstacle to
   emission today. A support-needing patch smaller than the seat-placement grid gets every
   candidate support thrown out before any is tried, so the repair search walks for the
   full harness ceiling committing nothing. See
   [`docs/plans/coverage-repair.md`](plans/coverage-repair.md) for the fix in progress and
   [`docs/plans/support-clearance.md`](plans/support-clearance.md) for the clearance-side
   mechanism (task #52) it depends on.
2. **Giant Toad (swallowing) — task #29.** Fails at Lowering: `RootRunTooShort` at slot 280.
   Root cause and fix not yet done.
3. **`zombie-2-a` fails the final ChituBox disk audit.** A crash with no owning task in the
   open backlog; needs one.
4. **Task #76's four pipeline reds.** Silently failing across multiple commits because the
   test suite they live in was never in any gate's checklist; each needs an owner or a fix.
5. **The two seal-semantics untested corners** named above — not known to be reachable, not
   proven unreachable.
6. **Hollowing does not yet handle multi-cavity models.** A shell enclosing more than one
   separate void is refused by the hollow stage and the model is prepared solid instead —
   correct and printable, just without the resin saving. The flagship treant is one such
   model.
7. **Support quality needs work** — every arm is structurally sound and proven attached
   (see [`docs/physics.md`](physics.md)), but quality as a print aid (support placement,
   scarring, ease of removal) has known issues. Inspect a file before printing it.

## Future work

Ordered from the open backlog (36 tasks; anomalies and full detail at the reconciled backlog
JSON this page was built from). Grouped by theme; within a group, dependency order.

### Coverage-repair and clearance (the corpus-blocking path)

| task | what |
| --- | --- |
| #52 | Land the stub-axis edge-on-face fix (land-at-strike). Design finished, blocker cleared, implementation next. |
| #24 | Fix fault group B (island with no eligible repair rows kills the model), via #52's mechanism. Dominant open corpus problem. |
| #41 | Re-land the FU-87 minimum-arm-length screen, blocked on #24 (landing it earlier crashes other models). |
| #37 | Fix the load-sensitive gate test that spuriously fails under CPU load — CI reliability, not a corpus blocker itself. |
| #39 | Reconcile coverage repair with binding SPEC A4: the controller is live on develop, but its own open questions (worst-case models, a deliberately-left-red test case) are unresolved. See `docs/plans/coverage-repair.md` for the itemized open rulings. |
| #47 | Re-check whether the Giant Bat now seals under the current repair controller — unmeasured since #39 landed. |
| #53 | Fix the legacy-replay asymmetry (`StaleRegularContacts` trap): baseline built legacy-OFF, validated legacy-ON; the two agree only by luck today. |

### Safety gates and correctness

| task | what |
| --- | --- |
| #17 | Refuse a requested wall below the 2.0 mm minimum by a named error, not a silent clamp. |
| #19 | Drilling drainability must respect the flow-based connectivity rule. |
| #38 | Continuous realized-wall proof — acceptance blocker for hollow output. |
| #42 | O-5 ownership consolidation and the remaining pre-write gates before final acceptance. |
| #50 | ~~Audit plinth-metrics' own `ray_triangle_distance` for the behind-origin defect already fixed elsewhere.~~ **MOOT 2026-08-26** — `plinth-metrics` was deleted by owner ruling, and the suspect copy went with it. Nothing was fixed and nothing was measured: the defect is simply no longer reachable because the code is gone. The corrected implementation in `plinth-mesh` is unaffected. |
| #52 | (see above — also a correctness fix, contacts condemned before any arm is tried) |
| #59 | Port `PreWriteGate::Load` — plans are never re-checked for load violations right before write. |
| #64 | Reach-rule gap: narrow-neck amplification unmodelled (a wide ledge on a thin stalk can pass the reach check). |
| #65 | Reach-rule gap: the reach probe has no connectivity requirement. |
| #69 | Multi-carrier load partition: let a face be carried by more than one contact. |
| #70 | Carriable partition at the creation seam — foundational for #69. |
| #72 | Some support stub tips never achieve their designed 0.3 mm embed. |
| #78 | Retire the owner-namespace collision in the rewire mechanism. |

### CI, tooling, and process

| task | what |
| --- | --- |
| #16 | Make the test gates run on a clean worktree (no `corpus/models`). |
| #30 | Wire the emission-regression canary and release-only treant tests into the CI gate suite. |
| #31 | Pin the H1b topology proof's strictness and stop mislabeling kernel failures as topology failures. |
| #54 | Ship `binary-provenance.sh` and make it mandatory before model runs. |
| #76 | Give each of the four silently-failing pipeline tests an owner or a fix. |
| #20 | Clear the `cargo fmt` debt (104 hunks, 24 files). |
| #12 | Naming pass: say "minimum", never "floor". |

### Performance and cost accounting

| task | what |
| --- | --- |
| #46 | Price the two unpriced Routing costs: the full sort to keep 40 candidates, and ~15 MB of retained refusal rows per terminal. |
| #48 | Audit why the Weeping Treant's attempt-0 Routing charge runs 2.1x the intended budget, on a wider model class. |
| #58 | Derive contact spacing from the arm area law instead of the fixed `TARGET_SPACING_MM` constant. |
| #57 | Registry bypass: 7 registered constants have no production reader — wire them in or retire them. |
| #73 | Fix the rung-outcome diagnostic instrument so it can reach real corpus models. |
| #40 | FU-83 production diagnostics — keep open; the cancelled `fu82-census` branch is reference-only and must not merge. |

### Physics measurement (blocked on physical prints)

| task | what |
| --- | --- |
| #60 | Build the missing sag/peel demand model (what a support's coverage radius should be) — blocked on a physical coupon-print calibration. See `docs/plans/support-calibration.md`. |
| #66 | Design the underside-angle coupon plate to replace the guessed `OVERHANG_ANGLE_DEG` with a measured value. Sized; needs Craig's go-ahead to print. See `docs/plans/support-calibration.md`. |
| #63 | Registry inconsistency: `PEEL_PRESSURE_PA` and `MODULUS_GREEN_GPA` jointly imply failure at ~0.007% strain, about 1000x off plausible — at least one value is wrong. |

### Not yet a task — needs Craig's ruling

Task #39's own O-3 finding (heavy refusers exceed `commit_budget=8` badly — up to 103 routes
on the Awakened Tree) is an open ruling request that has never been surfaced as its own
backlog entry; it currently lives only inside #39's description. See
`docs/plans/coverage-repair.md`.
