# Corpus Provisioning: plinth-3d-pipeline-integration

Target worktree: `/private/tmp/plinth-3d-pipeline-integration`
Manifest: `corpus/manifest.json` (schema `plinth/corpus-manifest` v1, frozen_at 2026-08-01T08:20:35Z, provisional=false, baseline_git_rev `6eb7f6a65e1a3442aa860d63d920788da778faed`)
Test gate: `crates/plinth-decimate/tests/corpus_budgets.rs` (`const EXPECTED: [CorpusModel; 8]`)

All 8 required models were found in the durable checkout
`/Users/craig/code/hughescr/dnd-campaigns/plinth-3d/corpus/models/` and copied
(via `cp -p`, never `mv`) into the ephemeral worktree's `corpus/models/`. Every
digest matched the manifest/test expectation exactly — no model was missing or
regenerated, and no Python was run.

## Per-model record

| # | Name | corpus/models path | sha256 | Bytes | Reproduction |
|---|------|---------------------|--------|-------|--------------|
| 1 | Sphere | `corpus/models/Sphere.stl` | `48c27bb596a265900a614171d7ffe3a7a4ea80d95f1c620cf3ab00f67fe0a1d5` | 238084 | Copied from durable checkout `plinth-3d/corpus/models/Sphere.stl`; byte-identical duplicate also lives at `plinth-3d/scripts/tests/fixtures/chitubox/Sphere.stl` (documented upstream source, 04-migration-plan.md M0 step 6). |
| 2 | Cylinder | `corpus/models/Cylinder.stl` | `7bd60bd7fb83882f2bdc9c977e4f15e55f5cf0342c05c31584c586782505f741` | 12284 | Copied from durable checkout; byte-identical duplicate at `plinth-3d/scripts/tests/fixtures/chitubox/Cylinder.stl`. |
| 3 | make_box | `corpus/models/make_box.stl` | `aada5b79e87461c6f74afe4e16a5f651aed47fd6638a50a225e35760a8010c3b` | 684 | Copied from durable checkout. Frozen output of the sanctioned one-off generator run (04-migration-plan.md M0 step 6); do not regenerate. |
| 4 | make_overhang_table | `corpus/models/make_overhang_table.stl` | `a776132a2cc4a43298d9ca646a4265a7ba60ed7d37de7618147ac754dfe8c996` | 3084 | Copied from durable checkout. Frozen generator output, same provenance as above. |
| 5 | make_toed_slab | `corpus/models/make_toed_slab.stl` | `ceb70f2b549fb131b0f177033401c50879a8fbc6d575b27459c0f9574d9f1169` | 7084 | Copied from durable checkout. Frozen generator output, same provenance as above. |
| 6 | make_raking_blade | `corpus/models/make_raking_blade.stl` | `0979f5812c26b3bd88f6e36bb792573fbdc9c023656c46544f2bffdecddefb97` | 1284 | Copied from durable checkout. Frozen generator output, same provenance as above. |
| 7 | make_thin_fin | `corpus/models/make_thin_fin.stl` | `09bec5c6ff8ff169f91438c209b12c3e6d9eb0dbb7e2fd7611e98d1884516c4c` | 1284 | Copied from durable checkout. Frozen generator output, same provenance as above. |
| 8 | Weeping Treant v1.2 | `corpus/models/Weeping Treant v1.2.stl` | `4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552` | 21456084 | Copied from durable checkout. Two further byte-identical copies confirm provenance: `/Users/craig/code/hughescr/dnd-campaigns/Weeping Treant v1.2.stl` (canonical durable copy) and `/Users/craig/code/hughescr/dnd-campaigns/minis-to-process/Awakened Tree.stl` (original staging export) — all three hash identically, confirming "Awakened Tree" == "Weeping Treant v1.2" per 04-migration-plan.md. |

## Models still missing

None. All 8 required models were present and verified.

## Verification commands used

```sh
SRC="/Users/craig/code/hughescr/dnd-campaigns/plinth-3d/corpus/models"
DST="/private/tmp/plinth-3d-pipeline-integration/corpus/models"
mkdir -p "$DST"
cp -p "$SRC"/*.stl "$DST"/

cd /private/tmp/plinth-3d-pipeline-integration && shasum -a 256 -c - <<'EOF'
48c27bb596a265900a614171d7ffe3a7a4ea80d95f1c620cf3ab00f67fe0a1d5  corpus/models/Sphere.stl
7bd60bd7fb83882f2bdc9c977e4f15e55f5cf0342c05c31584c586782505f741  corpus/models/Cylinder.stl
aada5b79e87461c6f74afe4e16a5f651aed47fd6638a50a225e35760a8010c3b  corpus/models/make_box.stl
a776132a2cc4a43298d9ca646a4265a7ba60ed7d37de7618147ac754dfe8c996  corpus/models/make_overhang_table.stl
ceb70f2b549fb131b0f177033401c50879a8fbc6d575b27459c0f9574d9f1169  corpus/models/make_toed_slab.stl
0979f5812c26b3bd88f6e36bb792573fbdc9c023656c46544f2bffdecddefb97  corpus/models/make_raking_blade.stl
09bec5c6ff8ff169f91438c209b12c3e6d9eb0dbb7e2fd7611e98d1884516c4c  corpus/models/make_thin_fin.stl
4c027b7849019ab9598c85f285b9df7d480aacfd1733123a32a8a81239525552  corpus/models/Weeping Treant v1.2.stl
EOF
```

Result: all 8 lines `OK`.

## Ignore/staging confirmation

```
$ git status --short corpus/
(no output)

$ git check-ignore -v corpus/models/*.stl
.gitignore:44:corpus/models/*.stl	corpus/models/Cylinder.stl
.gitignore:44:corpus/models/*.stl	corpus/models/make_box.stl
.gitignore:44:corpus/models/*.stl	corpus/models/make_overhang_table.stl
.gitignore:44:corpus/models/*.stl	corpus/models/make_raking_blade.stl
.gitignore:44:corpus/models/*.stl	corpus/models/make_thin_fin.stl
.gitignore:44:corpus/models/*.stl	corpus/models/make_toed_slab.stl
.gitignore:44:corpus/models/*.stl	corpus/models/Sphere.stl
.gitignore:44:corpus/models/*.stl	corpus/models/Weeping Treant v1.2.stl
```

All 8 models are gitignored via `.gitignore` line 44 (`corpus/models/*.stl`).
`git status` (full, post test-run) reports `nothing to commit, working tree
clean` — no new tracked files were introduced by copying models or running
the test.

## Test gate result

```sh
cd /private/tmp/plinth-3d-pipeline-integration && cargo test --offline -p plinth-decimate --test corpus_budgets --release
```

```
running 2 tests
test report_writer_canonicalizes_arithmetic_floats_before_the_only_disk_write ... ok
test corpus_budgets ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.72s
```

Artifacts written under `target/plinth-decimate-corpus-budgets/`:
`corpus-budget-report.json`, `Weeping Treant v1.2.before.stl`,
`Weeping Treant v1.2.after.stl` (these are build/run outputs, not part of the
corpus, and remain untracked/ignored).

## Deviations from the plan

None. Executed exactly as specified: cp (not symlink, not mv), digest
verification via `shasum -a 256 -c`, git-ignore confirmation, then
`cargo test --offline -p plinth-decimate --test corpus_budgets --release`
(explicit `--test corpus_budgets` was used rather than the bare
`-p plinth-decimate` invocation named in the task instructions, since the
plan's own Step 4 specifies that exact flag and the crate has a second,
unrelated test file — `report_writer_canonicalizes_arithmetic_floats_...`
— which ran regardless as part of the same binary). No file was created or
edited inside the repo/worktree; this document lives only in the scratchpad
per instructions. No git commit was made anywhere.
