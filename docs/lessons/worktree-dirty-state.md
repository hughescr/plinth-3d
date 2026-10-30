# Worktree dirty state (read-only scan, 2026-10-29)

Method: `git worktree list --porcelain` in plinth-3d, then `git --no-optional-locks -C <worktree> status --porcelain` for all 53 entries (the main checkout plus 52 `wt-*`). Nothing was copied, staged, committed or modified. A `git bundle` of branches does NOT preserve the items below. No code from these worktrees is in the staging folder.

Result: 50 of 53 are clean. Three have uncommitted changes.

| Path | Branch | HEAD | Uncommitted state |
|---|---|---|---|
| `/Users/craig/code/hughescr/dnd-campaigns/wt-s0-mn` | detached | `17b543d9b282beb2305dee0f92414d5599d4730d` | 10 files STAGED (index only, not committed), +256 lines: `crates/plinth-cli/src/execute.rs`, `crates/plinth-sweep/src/footprint_normal.rs`, `docs/architecture.md`, `rust/geom_kernel/src/{lib,nearest,par,queries,segment_batch,slicing,winding}.rs`. Most of the change (+216 lines) is a new `par.rs` in the geometry kernel (parallel helper). |
| `/Users/craig/code/hughescr/dnd-campaigns/wt-s0-rr` | detached | `17b543d9b282beb2305dee0f92414d5599d4730d` | 1 file STAGED: `crates/plinth-sweep/src/demand_replay.rs` (+546 / -137 lines). |
| `/Users/craig/code/hughescr/dnd-campaigns/wt-trfile` | `diag/trfile-2026-10-28` | (branch tip) | 1 UNTRACKED file: `crates/plinth-pipeline/examples/trfile.rs` (diagnostic example). |

Both detached worktrees sit on `17b543d9`, which is reachable from branches `feature/capacity-placement-2026-10-28` and `feature/construction-s1-2026-10-28`, so their committed history is in the bundle; only the staged diffs are at risk. All 53 HEADs are reachable from at least one branch (no orphan commits).

Other state outside the bundle:
- Two `git stash` entries in plinth-3d: `stash@{0}` "On perf/bc-2026-10-15: bc par-times instrumentation (patch saved in evidence)" and `stash@{1}` "On feature/stage0-bytes-audit: dropped 2026-10-01: 17 old plinth-audit chitubox-to-stl files (copy in plinth-evidence/dropped-plinth-audit-files-2026-10-01)". Stashes are refs in the repo (`refs/stash` holds only the latest); they are lost with the repo unless exported.
- Ignored (never tracked) files: each worktree has `target/` (71 across the set), `exports/` (53) and `corpus/models/` (53, provisioned STLs from the owner's folders; the owner copies stay in `minis-to-process`). The main checkout also has ignored leftover Python directories (`scripts/support_placement/` 2.3 MB, `scripts/tools/` and `rust/chitubox_io/python/` which hold `__pycache__` only) and `.DS_Store` files. None was judged to carry unique data.
- The scan shows `exports/` directories in all 53 trees: plinth output files, which the task rules out of the carry-over.

Decision for the orchestrator: the only content that is not recoverable from the bundle is the staged diffs in `wt-s0-mn` and `wt-s0-rr` and the untracked `trfile.rs`. Export them (`git diff --cached` to a patch file in `plinth-evidence/` or inside the archive) before deleting if they might be wanted; the lessons judge the old code as not worth carrying, so deleting them is also defensible.
