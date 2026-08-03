# Rust migration current status

Durable handoff recorded 2026-08-03. This file distinguishes integrated work from
reviewed-but-branch-only work and from work still awaiting review. A review result is
not evidence that a commit has been merged.

## Status vocabulary

- **Integrated:** the commit is an ancestor of the integration branch.
- **Accepted, branch-only:** an independent review accepted the commit, but it is not
  an ancestor of the integration branch. Integrate the named commit deliberately after
  inspecting its patch; do not merge the branch head blindly.
- **Blocked:** review found required changes. Do not integrate it.
- **Pending review:** implementation exists, but acceptance has not been recorded.
- **Uncommitted:** worktree-only state. It may be lost and is never integration evidence.

## Integration authority and verified snapshot

The integration authority is the clean worktree
`/private/tmp/plinth-3d-pipeline-integration` on branch
`codex/rust-pipeline-integration`.

The current reviewed implementation integration HEAD is
`2b933c5526760b5123d55005ecf596e66d8ed162` (`feat(cli): parse stable prepare
command`). It includes the independently accepted SCAR remediation `b5a3290`, the
preparation interface integration `92fa26a`, and the stable CLI parser integration
`2b933c`; the latter two are integrated, not branch-only. Re-read the live head before
resuming; never infer acceptance merely from presence on the integration branch.

Verified integrated M7 evidence in the ancestry of the current reviewed head `2b933c`
(the format, coverage, and pre-remediation SCAR commits are also in the earlier handoff
base):

| Gate slice | Integrated evidence | State |
| --- | --- | --- |
| format | merge `72a0bfb3582517c5b2c1f3bcb75ebb172333ea2f` | reviewed and integrated |
| coverage | `8aa940673991711a0fbbe0f8c4420437df8b98ca` and test follow-up `9e9d5ff1f68c3b6a613d23523afa07c8c9931360` | reviewed and integrated |
| clearance/transit SCAR chain | `7bac735` through reviewed remediation `b5a3290` | private batched SCAR diagnostic accepted and integrated; the rest of the clearance/transit gate remains unfinished |

The format and coverage rows are evidence for those slices only. They do not mean the
whole M7 gate has passed.

## Owner architecture decision {#owner-architecture-2026-08-03}

The owner approved this binding clearance-mask architecture on 2026-08-03:

1. Each member end may independently carry an optional, finite, designed clearance-mask
   extent. Neither, either, or both ends may have one.
2. A supplied extent is valid only after construction proves that its mask is connected
   to the corresponding endpoint. This is construction validation, not an inference from
   later penetration observations.
3. Observed penetration thickness `T` is separate evidence. It must not be substituted
   for, or used to synthesize, a designed mask extent.
4. Any mutation that could change the member or mask invalidates the prior construction
   validation. The mutated value cannot enter verification until freshly validated.
5. A path that requires a designed extent rejects when that extent is absent. It must not
   guess or recover one from observed geometry.
6. The retired `_embed_inset_arc_exact` `cos.max(0.5)`/60-degree-floor formula is deleted.
   It is not ported, retained as fallback, or used to weaken the new design.

This decision authorizes the embed-zone implementation. It does not accept any particular
implementation in advance.

## ChituBox GUI checkpoints

These are owner-observed GUI checkpoints, not automated test results and not evidence that
their generating commits are integrated.

### H2 exact-grounded full plate {#gui-checkpoint-h2}

- Project: `/private/tmp/plinth-chitubox-diagnosis/H2-exact-grounded-full-plate.chitubox`
- SHA-256: `a33b2e10c1293b322f89fe2910c79055fa59b2d8b7bf3b2d4df64c804a53a679`
- Size: `26,101,476` bytes
- Owner checkpoint: the main calibration project opened, every model was grounded, and
  it sliced cleanly in ChituBox.
- Related calibration commit: `634abdbebbca54cd622b571a609cca7bd9c64a61`
  (`fix(calibration): ground ordinary ChituBox models`), **accepted but branch-only**.

### Grounded loading fixture {#gui-checkpoint-loading-fixture}

- Project: `/private/tmp/plinth-home-loading-fixture-inspection/home-support-loading-fixture.chitubox`
- SHA-256: `bef224e1d9634971827e0e989f50b7bbdc288f08dd244e301ad0d7ef27ba1e1f`
- Size: `56,516` bytes
- Owner checkpoint: the fixture opened and sliced cleanly in ChituBox; the owner called
  it ready to print.
- Related calibration commit: `1541379c1063ca9a41ca2761a189779a8f715a2e`
  (`feat(calibration): restore grounded loading fixture`), **accepted but branch-only**.
  Its parent is `634abdb`; preserve that dependency when reviewing integration.

## Reviewed branch-only work

None of the commits in this table is integrated at the current reviewed HEAD `2b933c`.

| Work | Branch/worktree | Commit(s) | Review state | Integration constraint |
| --- | --- | --- | --- | --- |
| canonical delicacy authority | `codex/canonical-delicacy` at `/private/tmp/plinth-canonical-delicacy` | `b5d2397e3b36c1469d46d37edc60d9749dd4609c` then `aae61ee5327b45244836466df7f428aeb2c7195a` | **ACCEPT** for the coherent final state | `aae61ee` directly depends on the canonical field contract introduced by `b5d2397`; its remediation acceptance covers that required predecessor content, not a standalone `aae61ee` patch. Either create and independently review a squashed final delicacy patch against current integration, or independently review and integrate the coherent two-commit series. Non-blocking follow-up: explicit M0-value and signed-zero equality/digest regressions. |
| grounded calibration artifacts | `codex/home-calibration-plate` at `/private/tmp/plinth-home-calibration-plate` | `634abdb`, `1541379` | **ACCEPT** and owner GUI-checked | Branch-only; independently review the two-commit series and its base before cherry-picking. |
| preparation resource policy and entry validation | `codex/prepare-interface` | `e4db5d08fca8b4739e137f13bbf7694c0b94ca57`, `7b35c49ababa867d8fa93a1d13454ee26e6afa7a` | **BLOCKED / incomplete** | Do not cherry-pick. Add no-work tests for invalid decimation configuration; malformed support zero/MAX/nested-cap refusal; and external compile-fail tests proving `SupportPreparationInput` fields cannot be constructed directly. Re-run the frozen coverage test without changing its golden. |

Resource-branch test evidence is partial: decimation and pipeline library tests and the
support runner integration test passed; the coverage gate had two of three tests pass and
the frozen JSON test fail. The unchanged sealed snapshot digest observed in that work was
`5304257786453935365`. Strict Clippy was then blocked by the transit lint later addressed
on integration; it must be rerun after remediation.

## Active and pending work

| Worktree/branch | Observed head | State at handoff |
| --- | --- | --- |
| `/private/tmp/plinth-3d-pipeline-integration`, `codex/rust-pipeline-integration` | `2b933c` | clean; includes integrated preparation interface `92fa26a` and CLI parser `2b933c` |
| `/private/tmp/plinth-canonical-delicacy`, `codex/canonical-delicacy` | `aae61ee` | clean; accepted branch-only |
| protocol PDF/source | integrated at `0ac30f1` | home calibration protocol Markdown, Typst source, and PDF are integrated; the reviewed Typst 0.13.1 artifact is recorded separately from a byte-different Typst 0.15.1 reproduction |

There are additional retained specialist worktrees. Use `git worktree list --porcelain`
as the authoritative inventory before assigning or removing any of them. Do not delete or
move a worktree merely because it is not listed in the focused table above.

Cleanup reduced worktrees from 26 to 12 after adding the protocol worktree and removing
merged and superseded work. Branches reduced from 66 to 52. Both exact stale `--list/`
directories were removed, approximately 278,241,280 bytes of generated-only output; they
are regeneration-only and must never be committed. Do not use Python for migration
implementation or ad-hoc verification.

## Ordered resumption checklist

1. Reconcile the live integration head and preserve the recorded SCAR acceptance:

   ```sh
   git -C /private/tmp/plinth-3d-pipeline-integration status --short --branch
   git -C /private/tmp/plinth-3d-pipeline-integration log -3 --oneline --decorate
   git -C /private/tmp/plinth-3d-pipeline-integration diff --check
   ```

   The independent ACCEPT for `b5a3290` and its integrated successors are recorded above. If the live head has advanced,
   inspect its ancestry and require independent review for each new change before extending
   that acceptance.

2. The accepted preparation interface and stable CLI parser are already integrated as
   `92fa26a` and `2b933c`, respectively. Retain execution-runner and resource-admission
   work as separate follow-up work; the blocked resource-policy commits remain excluded.

   Handle canonical delicacy separately. Do **not** cherry-pick `aae61ee` alone: it is the
   direct child of `b5d2397` and relies on that predecessor's canonical field contract.
   The recorded remediation ACCEPT covers the required predecessor content in the coherent
   final state. A successor must either create and independently review one squashed final
   delicacy patch against the current integration head, or independently review and
   integrate the ordered `b5d2397` then `aae61ee` two-commit series.

3. Repair the blocked resource-policy commits in their existing worktree. Add all three
   missing refusal/no-work/compile-fail test classes listed above. Re-run the focused tests,
   frozen coverage gate, strict Clippy, format check, and diff check. Obtain a fresh
   independent review. Only then cherry-pick the repaired commits individually.

4. Implement the now-authorized per-end embed-zone design from O-11. Keep designed mask
   extent and observed `T` separate; enforce endpoint-connected construction validation;
   invalidate it on mutation; reject missing required extents; delete rather than port the
   retired cosine-floor formula. Use one batched gather/native-call/scatter path and add
   refusal/no-native-work plus exact call-census tests. Obtain independent review before
   integration.

5. Complete former M7 in its documented gate order:

   ```text
   format [integrated]
   -> coverage [integrated]
   -> clearance/transit [in progress]
   -> load
   -> connectivity
   -> printability
   -> murray
   -> ball breach
   -> response_for exhaustive table
   -> fixer-efficacy and Terminal-proof suites
   -> relocation ladder
   -> incremental re-verify == full re-verify
   -> remaining typed-registry conversion
   ```

   The preparation interface and CLI parser are integrated. Continue the execution runner:
   resolve a complete `PrepareRequest`, load/cache inputs, run the move-only stage chain,
   emit the artifact/report once, and keep resource admission at stage boundaries. The
   blocked resource patch is not authorization to bypass its tests.

6. Run the M7 acceptance commands in `04-migration-plan.md` only after every slice above is
   integrated. Record full-corpus score evidence and owner ChituBox review separately; the
   two GUI checkpoints in this file do not by themselves satisfy all M7 acceptance.

7. Continue M8: dense L2/L1/exact orientation, Pareto records, `plinth calibrate`,
   LoadSharing A/B, exact-byte 1/4/16-thread determinism, cache-on/off equality, and kernel
   benchmarks. O-6 makes redesign/calibration post-Python backlog, while determinism of the
   minimally ported pipeline remains a deletion gate.

8. Complete M9/native-handoff deletion under O-6/O-7: eliminate every remaining Python,
   PyO3, maturin, shim, and Python-only control; update docs/skills; run workspace test,
   Clippy, format, zero-Python searches, prepare/score, and the documented clean-checkout
   re-fire. The owner-print checkpoint remains binding. Never claim M9 from an existing
   dirty or artifact-bearing worktree.

9. Before any final merge, re-run:

   ```sh
   git status --short --branch
   git diff --check
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace
   ```

   Use an independent reviewer on the exact candidate diff. Cherry-pick reviewed commits
   into the clean integration worktree and merge only after verifying ancestry and the
   resulting diff. An agent's clean branch, passing focused test, or ACCEPT label is not a
   substitute for integration verification.
