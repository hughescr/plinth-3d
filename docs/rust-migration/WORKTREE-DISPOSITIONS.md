# Worktree & Branch Dispositions — 2026-08-04

> **Execution status (final, 2026-08-04):** fully executed. WIP rescue commit `0384925`, all 39 archive tags, artifact backup to `plinth-3d-artifacts/2026-08-04-plate07/`, the 39 tagged branch deletions, and 6 worktree removals are done.
>
> **Owner decisions (2026-08-04, post-audit):** the five *RETAINED (unproven)* draft branches (`codex/support-perception`, `-specialized`, `-contact-pool`, `-anchor-search`, `-member-sizing`), the ASK item `codex/m3-decimate-proxy`, and `codex/shared-surface-authority` (including its rescued WIP `0384925`) were judged superseded/abandoned experiments and deleted **untagged** at the owner's direction — git gc reaps them; do not attempt to resurrect. `codex/normalization-controller` is **parked for independent review** as candidate coverage-repair machinery.
>
> **End state: 8 branches, 8 worktrees, each with a stated purpose** — master; integration; delicacy copy B (integrate first); embed-zones (review); prepare-interface (repair); home-calibration-plate (plate/fixture generator); chitubox-native-preview (one unlanded fix to cherry-pick); normalization-controller (parked for review).

Reference point for every verdict: **`codex/rust-pipeline-integration` @ `a3887a3`** (master `5e37d8c` is a strict ancestor of it).

Goal filter: keep only what serves *"Rust pipeline preps mini STLs, generates supports, emits .chitubox; eliminate all Python"*, the reviewed integration backlog, and protocol-record artifacts.

Every DELETE is archive-tagged as `archive/2026-08-04/<branch>` **before** deletion, so all 39 removals are recoverable with `git branch <name> archive/2026-08-04/<name>`.

Result: 54 branches -> 15; 14 worktrees -> 9.

## Standard of proof

A branch was deleted only when a fresh, rerun git command showed **zero surviving unique patches**. Plausibility arguments — "integration's version of the file is 10x bigger", "the API was renamed", "N of M public symbols are present" — were **not accepted**. Five branches originally proposed for deletion failed this bar and were retained; see *Retained: unproven supersession*.

| Proof type | Decisive command | Pass condition |
| --- | --- | --- |
| MERGED | `git merge-base --is-ancestor <branch> a3887a3` and `git cherry a3887a3 <branch>` | exit 0, and zero `+` lines |
| CONTENT-MERGED | `git cherry a3887a3 <branch>` | zero `+` lines (branch is not an ancestor, but every patch is matched) |
| PATCH-EQUIVALENT | `git range-diff <branch-range> <survivor-range>` | every pair reported `=`, no hunks |
| COVERED-BY-SURVIVOR | `git cherry <surviving-branch> <branch>` reduces the set, and each remainder is separately proven `=` or identical by `git patch-id` against a live carrier | zero unaccounted commits |

## Branch table

| Branch | Verdict | Reason | Archive tag |
| --- | --- | --- | --- |
| `master` | KEEP | Protected trunk. | — |
| `codex/rust-pipeline-integration` | KEEP | Protected. Integration head `a3887a3`; the yardstick for this whole audit. | — |
| `codex/integrate-canonical-delicacy-o5` | KEEP | Protected — delicacy copy B, reviewed backlog. Contains both of copy A's commits (`580d9e6`, `619df1d` confirmed ancestors) plus 2 further commits. | — |
| `codex/embed-zones` | KEEP | Protected — pending review, owner-authorized. 4 unmatched commits, 1 file, +753. | — |
| `codex/prepare-interface` | KEEP | Protected — BLOCKED, needs repair per CURRENT-STATUS.md. Confirmed sole surviving carrier of `e4db5d0` and `4809e36` (both re-verified as ancestors of this branch). | — |
| `codex/home-calibration-plate` | KEEP | Protected — generator authority for the printed H2 plate and loading fixture, owner GUI-checked. Also the carrier of 32 of `codex/chitubox-diagnosis`'s 34 commits. | — |
| `codex/chitubox-native-preview` | KEEP | Only live carrier of unlanded `9ffc829` *embed native preview fallback*. Sits on the integration lineage, so it cherry-picks cleanly. | — |
| `codex/normalization-controller` | KEEP | New subsystem `crates/plinth-support/src/normalization.rs` — absent from `a3887a3` entirely — with CCO wiring and refusal-evidence tests (4 commits, +1890). Awaiting a merge decision. | — |
| `codex/shared-surface-authority` | KEEP | 1 unmatched commit `2969ad4` **plus 4 uncommitted files** in its worktree (`crates/plinth-support/src/{anchor,lib,perception,specialized}.rs`). WIP-committed on 2026-08-04 before anything else ran. Not mergeable as-is. | — |
| `codex/m3-decimate-proxy` | **ASK** | `f4e3e40` adds `crates/plinth-decimate/src/proxy.rs` (12,969 B, deterministic orientation proxy ladder). Integration has no `proxy.rs`. Sibling `codex/m3-decimate-adapter` took a different route and landed. **Owner decides.** Untouched and untagged. | — |
| `codex/support-perception` | **RETAINED (unproven)** | Delete proposal **vetoed**. `git cherry a3887a3` yields 2 `+` commits (`8bde2f4`, `08cab05`), and `git cherry codex/support-perception-v2 codex/support-perception` yields the *same two* — the named successor does not contain them. Size/API-rename evidence is not proof. | — |
| `codex/support-specialized` | **RETAINED (unproven)** | Delete proposal **vetoed**. `git cherry a3887a3` yields 1 `+` commit (`f7bcf34`); `git cherry codex/support-specialized-v2 codex/support-specialized` also yields it. | — |
| `codex/support-contact-pool` | **RETAINED (unproven)** | Delete proposal **vetoed**. 6 `+` commits vs integration (`e7d70c1`, `34f32fb`, `85fb08e`, `6be1a69`, `557d520`, `fc378b3`). Public-symbol survey (51 of 52) is not patch containment, and it concedes one gap. | — |
| `codex/support-anchor-search` | **RETAINED (unproven)** | Delete proposal **vetoed**. 8 `+` commits, including `43c558b` and `bc70ed0`. Proposer's own evidence concedes 2 of 27 `anchor.rs` public items are absent from integration. | — |
| `codex/support-member-sizing` | **RETAINED (unproven)** | Delete proposal **vetoed**. 8 `+` commits, including `d7e25c1` and `1834e72` (*preserve diagnostic sizing authority boundaries*). A public-symbol superset says nothing about that behaviour. | — |
| `codex/canonical-delicacy` | DELETE | PATCH-EQUIVALENT. `git cherry codex/integrate-canonical-delicacy-o5 codex/canonical-delicacy` -> both commits `-`, zero `+`. `git range-diff b5d2397^..aae61ee 580d9e6^..619df1d` -> both pairs `=`. Survivor is protected. | `archive/2026-08-04/codex/canonical-delicacy` |
| `codex/prepare-cli` | DELETE | COVERED-BY-SURVIVOR. `git cherry codex/prepare-interface codex/prepare-cli` leaves exactly one commit; `git patch-id --stable` on `b07d445` and on integrated `2b933c5` both return `ea7eb83…`, and `2b933c5` is an ancestor of `a3887a3`. `e4db5d0` and `4809e36` are ancestors of `codex/prepare-interface`. | `archive/2026-08-04/codex/prepare-cli` |
| `codex/home-test-protocol` | DELETE | CONTENT-MERGED — all 5 commits `-` in `git cherry a3887a3`, zero `+`. | `archive/2026-08-04/codex/home-test-protocol` |
| `codex/migration-handoff-2026-08-03` | DELETE | CONTENT-MERGED — both commits `-`, zero `+`. | `archive/2026-08-04/codex/migration-handoff-2026-08-03` |
| `codex/chitubox-diagnosis` | DELETE | COVERED-BY-SURVIVOR (two protected/KEEP branches). `git cherry codex/home-calibration-plate codex/chitubox-diagnosis` leaves exactly 2 commits; `git range-diff 9ea0e19^..a373788 e69b94b^..9ffc829` marks both `=`. `e69b94b` is an ancestor of `a3887a3`; `9ffc829` is the head of `codex/chitubox-native-preview`. The other 32 commits are reachable from `codex/home-calibration-plate`. | `archive/2026-08-04/codex/chitubox-diagnosis` |
| `codex/anchor-provenance` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/anchor-provenance` |
| `codex/bounded-penetration-batch` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/bounded-penetration-batch` |
| `codex/cco-handoff-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/cco-handoff-parity` |
| `codex/cco-rewire-replay` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/cco-rewire-replay` |
| `codex/cco-rewire-seam` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/cco-rewire-seam` |
| `codex/cco-terminal-dispositions` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/cco-terminal-dispositions` |
| `codex/emit-chitubox` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/emit-chitubox` |
| `codex/final-load-leaf` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/final-load-leaf` |
| `codex/finalized-cco-route` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/finalized-cco-route` |
| `codex/geom-batched-slices` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/geom-batched-slices` |
| `codex/hollow-boolean-limits` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/hollow-boolean-limits` |
| `codex/hollow-shell` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/hollow-shell` |
| `codex/hollow-triage-admission` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/hollow-triage-admission` |
| `codex/initial-stub-quarantine` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/initial-stub-quarantine` |
| `codex/m3-decimate-adapter` | DELETE | MERGED (ancestor, 0 unmatched). The decimation route that landed. | `archive/2026-08-04/codex/m3-decimate-adapter` |
| `codex/native-surface-identity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/native-surface-identity` |
| `codex/orient-pinned` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/orient-pinned` |
| `codex/repair-pool-authority` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/repair-pool-authority` |
| `codex/support-anchor-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-anchor-parity` |
| `codex/support-brace-current-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-brace-current-parity` |
| `codex/support-cco-current-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-cco-current-parity` |
| `codex/support-dyadic` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-dyadic` |
| `codex/support-joints-current-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-joints-current-parity` |
| `codex/support-lowering-current-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-lowering-current-parity` |
| `codex/support-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-parity` |
| `codex/support-parity-policy` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-parity-policy` |
| `codex/support-perception-v2` | DELETE | MERGED (ancestor, 0 unmatched). Safe independently of the `support-perception` veto — v2 is fully contained in integration. | `archive/2026-08-04/codex/support-perception-v2` |
| `codex/support-raft` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-raft` |
| `codex/support-rewire-current-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-rewire-current-parity` |
| `codex/support-sizing-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-sizing-parity` |
| `codex/support-specialized-v2` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-specialized-v2` |
| `codex/support-transient-current-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/support-transient-current-parity` |
| `codex/verify-current-parity` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/verify-current-parity` |
| `codex/vertical-slice-api` | DELETE | MERGED (ancestor, 0 unmatched). | `archive/2026-08-04/codex/vertical-slice-api` |

## Worktree table

All five removal targets were re-verified clean (`git -C <path> status --porcelain` empty) immediately before removal. No `--force` was used.

| Path | Action | Reason |
| --- | --- | --- |
| `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d` | RETAIN | Main checkout, `master`. Ref store for all tags and deletions. |
| `/private/tmp/plinth-3d-pipeline-integration` | RETAIN | Integration worktree; this document is committed here. |
| `/private/tmp/plinth-canonical-delicacy` | REMOVE | Branch DELETE, worktree verified clean. |
| `/private/tmp/plinth-canonical-delicacy-integration` | RETAIN | Branch KEEP (delicacy copy B). |
| `/private/tmp/plinth-chitubox-diagnosis-wt` | REMOVE | Branch DELETE, verified clean. Distinct from the artifact directory `/private/tmp/plinth-chitubox-diagnosis`, which is backed up, not removed. |
| `/private/tmp/plinth-chitubox-preview` | RETAIN | Branch KEEP (unlanded preview fallback `9ffc829`). |
| `/private/tmp/plinth-embed-zones` | RETAIN | Branch KEEP (protected). |
| `/private/tmp/plinth-home-calibration-plate` | RETAIN | Branch KEEP (protected plate authority). |
| `/private/tmp/plinth-home-test-protocol` | REMOVE | Branch DELETE, verified clean. |
| `/private/tmp/plinth-migration-handoff` | REMOVE | Branch DELETE, verified clean. |
| `/private/tmp/plinth-normalization-controller` | RETAIN | Branch KEEP (unique subsystem). |
| `/private/tmp/plinth-prepare-cli` | REMOVE | Branch DELETE, verified clean. |
| `/private/tmp/plinth-prepare-interface` | RETAIN | Branch KEEP (protected, BLOCKED). |
| `/private/tmp/plinth-shared-surface-authority` | WIP-COMMIT-THEN-RETAIN | Branch KEEP; 4 modified files under `crates/plinth-support/src/` committed as WIP before anything else ran. |

The five vetoed draft branches (`support-perception`, `support-specialized`, `support-contact-pool`, `support-anchor-search`, `support-member-sizing`) have no worktrees.

## Artifacts preserved

Copied to `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d-artifacts/2026-08-04-plate07/` — a **sibling of** the repo root, verified outside the git worktree. Run outputs are never committed.

- `chitubox-diagnosis/` — from `/private/tmp/plinth-chitubox-diagnosis` (907 M on disk): all `*.chitubox`, `*.txt`, `*.md`, and `h2_audit/`. The `F-descriptors-only-full-plate/` and `G-descriptors-plus-preview-full-plate/` subdirectories **are** traversed, so their `home-support-calibration-plate.chitubox` files are included; other payload inside them is not. Excluded: `target/` (the bulk of the 907 M) and `src/`.
- `home-loading-fixture-inspection/` — all of `/private/tmp/plinth-home-loading-fixture-inspection` (198 M).

Backup integrity was checked with a checksum dry-run (`rsync -a -n -c -i` using the identical filter set, `--prune-empty-dirs` included) which must list no transfers. Source directories under `/private/tmp` were left in place by this pass.

## Follow-ups left open

1. **Five vetoed draft branches** — `support-perception`, `support-specialized`, `support-contact-pool`, `support-anchor-search`, `support-member-sizing` survive untagged and undeleted. To retire them, someone must produce a real containment proof (per-file `git diff` of each `+` commit against `a3887a3`, or an explicit owner decision to abandon the work). Two of them have commits their *named successor branch* provably does not contain.
2. `codex/m3-decimate-proxy` — ASK, untouched and untagged pending the owner's call.
3. `codex/chitubox-native-preview` — `9ffc829` is the last unlanded ChituBox fix; cherry-pick onto integration.
4. `codex/normalization-controller` — complete-looking subsystem awaiting a merge/reject decision.
5. `codex/shared-surface-authority` — now WIP-committed; needs finishing or an explicit abandon.
6. `codex/prepare-interface` — still BLOCKED; the resource-policy commits it carries are now unduplicated, so it is the only copy.

