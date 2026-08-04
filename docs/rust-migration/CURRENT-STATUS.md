# Rust migration current status

Durable handoff recorded 2026-08-03; updated after the 2026-08-04 integration pass. This
file distinguishes integrated work from reviewed-but-branch-only work and from work still
awaiting review. A review result is not evidence that a commit has been merged.

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

The current integration HEAD is `a4aefe68c41a9644464d2175a63c6d8faddc51e4`
(`docs(corpus): record model provisioning sources and digests`). The last reviewed
*implementation* change in its ancestry is the canonical-delicacy merge
`240d144e264cbe9ba27d45f6ff0194eb83a36ced`; `a4aefe6` and the CURRENT-STATUS commit that
follows it are docs-only. The head also includes the independently accepted SCAR
remediation `b5a3290`, the preparation interface integration `92fa26a`, and the stable CLI
parser integration `2b933c`. Re-read the live head before resuming; never infer acceptance
merely from presence on the integration branch.

### 2026-08-04 integration pass {#integration-pass-2026-08-04}

Four backlog items were processed in order by a single writer on
`codex/rust-pipeline-integration`. Outcomes:

| Item | Outcome | Evidence |
| --- | --- | --- |
| canonical delicacy authority | **Integrated** as merge `240d144e264cbe9ba27d45f6ff0194eb83a36ced` (`merge: integrate canonical delicacy authority`), second parent `5f281d7e28f7761cca052f202a887e09bff05afb` | Independent re-review 2026-08-04 returned ACCEPT. `git diff 027a9ef 240d144 --stat` = 4 files, +474/-5: `crates/plinth-mesh/src/delicacy.rs` +372, `crates/plinth-mesh/src/lib.rs` +5, `crates/plinth-metrics/src/scar.rs` 100, `crates/plinth-metrics/tests/scar_metrics.rs` 2. Nothing else moved. |
| endpoint-connected embed zones | **Not integrated — BLOCKED** | Independent review 2026-08-04 returned BLOCKED on owner conditions 5 and 6; see below. `codex/embed-zones` @ `68c3097` and `/private/tmp/plinth-embed-zones` are untouched. |
| preparation resource policy | **Not integrated — still blocked** | The diagnosis chain never produced a review verdict, so the cherry-pick precondition was not met. No commit from `codex/prepare-interface` was picked. |
| corpus provisioning | **Recorded** as `a4aefe6` (docs only) | Reviewed ACCEPT: all 8 model digests re-hashed and matched `corpus/manifest.json`; `cargo test -p plinth-decimate --offline` green. See `corpus-provisioning.md`. The `.stl` models themselves stay gitignored and uncommitted. |

Gate run at `240d144` (before the docs commits), in
`/private/tmp/plinth-3d-pipeline-integration`:

- `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` — **clean**.
- `cargo fmt --all -- --check` — **clean**.
- `git diff --check` — **clean**.
- `cargo test --workspace --offline --no-fail-fast` — **one failure workspace-wide**:
  `public_coverage_report_is_frozen_without_evidence_loss`
  (`crates/plinth-verify/tests/coverage_gate.rs:138`), actual `snapshot_digest`
  `5304257786453935365` against the frozen golden `8022575652269967193` in
  `crates/plinth-verify/tests/coverage_gate_overhang_table.json`.

That failure is **pre-existing drift on the integration branch, not a regression from the
delicacy merge**. Proof: `git diff 027a9ef 240d144 -- crates/plinth-verify crates/plinth-support`
is empty, and every `crates/plinth-metrics/src/scar.rs` hunk in the merge (`@@ 2362, 2392,
2581, 2611, 2642, 3239`) lies strictly inside the file's single `#[cfg(test)] mod tests`,
which begins at line 2336. The mover is ancestor `908d6df` (*Bind final live members in
verification snapshot*), which added `final_live_members_digest` and
`final_live_members.len()` to the FNV chain in `verification_snapshot_digest`
(`crates/plinth-support/src/runner.rs`); the golden JSON has only ever been written by
`9e9d5ff` and was never regenerated afterwards. Because the gate was not fully green, the
delicacy branch and worktree were **left in place** rather than deleted — see *Active and
pending work*.

The stale golden is now the top blocker for every downstream item: it makes
`cargo test --workspace` red for reasons unrelated to whatever is being merged, so it
masks real regressions. Regenerate it deliberately, with the digest change attributed to
`908d6df` in the commit message, before the next integration pass.

### Embed-zones blocking findings {#embed-zones-blocked-2026-08-04}

`codex/embed-zones` @ `68c3097` (1 file, `crates/plinth-support/src/forest.rs`, +753/-0)
satisfies owner conditions 1-4 structurally and fails 5 and 6:

1. **Condition 6 unmet — the retired formula is still live in Rust.**
   `crates/plinth-support/src/brace.rs` still defines
   `embed_inset_mm(...) = contact_depth_mm + (radius_mm + standoff_mm) / cosine` with
   `cosine = dot(axis, normal).abs().max(0.5)`, a line-for-line port of
   `_embed_inset_arc_exact` in `scripts/support_placement/verify.py`. It is not merely
   retained; it is the operative designed-embed self-mask feeding `segment_clearance` and
   `axis_illegal_rows`. The branch deletes nothing (zero-deletion diff).
2. **Condition 5 unmet — absence is guessed, not rejected.** `exact_clearance_rows` in
   `brace.rs` selects every member with `embed_end().is_some()`, projects onto the surface,
   and synthesizes the inset from the observed normal. The new `MissingRequiredExtent`
   refusal never guards it.
3. **Root cause — the new module is dead code.** Every reference to
   `DesignedClearanceMasks`, `DesignedEndMask`, `validated_extent_for`,
   `EndpointMaskValidationLimits`, and `PositiveFiniteMm` is inside `forest.rs` itself. No
   production caller mints a mask; no verification path reads one. The types are `pub` in a
   `pub mod`, so dead-code lints do not fire and a green build proves nothing.
4. **Batch-first defect.** `validate_endpoint_connected` batches at most 2 rows (one member,
   two ends) per native call, so a forest of N embedded members forces N native calls. The
   same crate already batches all designed members into one call in
   `transit_measurement.rs`; that shape was available and not used.
5. **A determinism test that does not test determinism.**
   `endpoint_validator_handles_reversed_root_coordinates_and_is_deterministic` writes
   `for _ in [1, 4, 16]` and discards the values, never setting `PLINTH_THREADS`, while the
   kernel re-reads that variable per batch call.

Positive and worth keeping when this is repaired: `DesignedEndMask` is genuinely sealed
(private fields, no public constructor, no `Serialize`, single reader), the mask binding
digests the full member including `certificate.geom_version` so any mutation invalidates,
and the endpoint-connectivity proof is exact against the kernel's own `capsule_rays`.

Verified integrated M7 evidence in the ancestry of the current head `a4aefe6`
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

None of the commits in this table is integrated at the current HEAD `a4aefe6`. The
canonical delicacy row was removed on 2026-08-04 because that work is now **Integrated**
via merge `240d144`; see the *2026-08-04 integration pass* table above.

| Work | Branch/worktree | Commit(s) | Review state | Integration constraint |
| --- | --- | --- | --- | --- |
| grounded calibration artifacts | `codex/home-calibration-plate` at `/private/tmp/plinth-home-calibration-plate` | `634abdb`, `1541379` | **ACCEPT** and owner GUI-checked | Branch-only; independently review the two-commit series and its base before cherry-picking. |
| preparation resource policy and entry validation | `codex/prepare-interface` at `/private/tmp/plinth-prepare-interface` | `e4db5d08fca8b4739e137f13bbf7694c0b94ca57`, `7b35c49ababa867d8fa93a1d13454ee26e6afa7a` | **BLOCKED / incomplete** | Do not cherry-pick. Add no-work tests for invalid decimation configuration; malformed support zero/MAX/nested-cap refusal; and external compile-fail tests proving `SupportPreparationInput` fields cannot be constructed directly. Note the 2026-08-04 diagnosis: the frozen-coverage failure this branch was blamed for is **branch-independent** pre-existing drift from ancestor `908d6df`, and the branch's own two commits are result-inert (`SupportPreparationLimits::production()` reproduces the former inline test caps value-for-value; the forced `max_grid_bytes` change is a construction-refusal cap only). Fix the golden on integration first, then re-review. Also note the branch is based on `5260679` and is missing the three SCAR-hardening commits `b692678`, `6d7a046`, `b5a3290`. |
| endpoint-connected embed zones | `codex/embed-zones` at `/private/tmp/plinth-embed-zones` | `68c3097` (4 commits, 1 file, +753/-0) | **BLOCKED** (2026-08-04) | Do not integrate. Owner conditions 5 and 6 are unmet and the new module is unreferenced dead code; full findings under *Embed-zones blocking findings*. |

Delicacy follow-up still open after the merge: the requested explicit **M0-value and
signed-zero** equality/digest regressions were *not* added by `c2927a8`/`5f281d7` — those
commits touch `plinth-metrics` only. `DelicacySettings::production()` has zero callers
workspace-wide and `crates/plinth-mesh/src/delicacy.rs` contains no `-0.0` anywhere, so
`canonical_zero()` is untested for the case it exists to handle. Non-blocking, still open.

The `move_characterization_` prefix introduced by those commits marks move oracles that
must stay bit-exact across the planned `plinth-metrics` -> `plinth-mesh` delicacy move.
Do not "fix" one of their pinned digests; investigate instead. The pins transitively depend
on libm `acos`, so a cross-platform CI difference there is a portability artifact, not a
regression.

## Active and pending work

This table is the live inventory as of the 2026-08-04 integration pass: **8 branches, 8
worktrees**. The rows for `codex/canonical-delicacy`, `codex/home-test-protocol`,
`codex/migration-handoff-2026-08-03`, `codex/prepare-cli`, `codex/chitubox-diagnosis`,
`codex/shared-surface-authority`, `codex/m3-decimate-proxy`, and the five
`codex/support-*` drafts were **removed** because those branches and their worktrees no
longer exist. See `WORKTREE-DISPOSITIONS.md` for the per-branch proof, archive tags, and
the owner's untagged-deletion decision.

| Worktree/branch | Observed head | State |
| --- | --- | --- |
| `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, `master` | `5e37d8c` | main checkout and ref store; strict ancestor of integration |
| `/private/tmp/plinth-3d-pipeline-integration`, `codex/rust-pipeline-integration` | `a4aefe6` | clean; integration authority. Adds the delicacy merge `240d144` and the corpus docs commit on top of the prior `2b933c` lineage |
| `/private/tmp/plinth-canonical-delicacy-integration`, `codex/integrate-canonical-delicacy-o5` | `5f281d7` | **merged into integration** at `240d144`; branch and worktree retained only because the workspace gate is red on the pre-existing frozen golden. Delete both once that gate is green |
| `/private/tmp/plinth-embed-zones`, `codex/embed-zones` | `68c3097` | **BLOCKED**; retained for repair |
| `/private/tmp/plinth-prepare-interface`, `codex/prepare-interface` | `7b35c49` | **BLOCKED**; sole carrier of `e4db5d0` and `4809e36` |
| `/private/tmp/plinth-home-calibration-plate`, `codex/home-calibration-plate` | `1541379` | accepted branch-only; plate and loading-fixture generator authority |
| `/private/tmp/plinth-chitubox-preview`, `codex/chitubox-native-preview` | `9ffc829` | only live carrier of the unlanded embed-native-preview fallback; cherry-pick onto integration |
| `/private/tmp/plinth-normalization-controller`, `codex/normalization-controller` | `fc5a60d` | parked for independent review as candidate coverage-repair machinery |
| protocol PDF/source | integrated at `0ac30f1` | home calibration protocol Markdown, Typst source, and PDF are integrated; the reviewed Typst 0.13.1 artifact is recorded separately from a byte-different Typst 0.15.1 reproduction |

Use `git worktree list --porcelain` and `git branch --list` as the authoritative inventory
before assigning or removing anything. Do not delete or move a worktree merely because it
is not listed above.

The 2026-08-04 hygiene pass took branches from 54 to 8 and worktrees from 14 to 8; 39
deletions are recoverable via `archive/2026-08-04/<branch>` tags, while the five vetoed
`codex/support-*` drafts, `codex/m3-decimate-proxy`, and `codex/shared-surface-authority`
were deleted **untagged** at the owner's direction and must not be resurrected. Earlier
cleanup removed both stale `--list/` directories, approximately 278,241,280 bytes of
generated-only output; they are regeneration-only and must never be committed.

Corpus models are provisioned but deliberately uncommitted: all 8 files live in
`corpus/models/` matching `corpus/manifest.json` digests, and `.gitignore` line 44
(`corpus/models/*.stl`) keeps them out of git. `docs/rust-migration/corpus-provisioning.md`
records their sources and SHA-256 values so a clean checkout can re-provision them. Do not
use Python for migration implementation or ad-hoc verification.

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

2. **Regenerate the frozen coverage golden first.** `cargo test --workspace` is red on
   `public_coverage_report_is_frozen_without_evidence_loss` for reasons unrelated to any
   pending branch, which masks real regressions in every later step. Regenerate
   `crates/plinth-verify/tests/coverage_gate_overhang_table.json` to `snapshot_digest`
   `5304257786453935365`, verify by inspection that only that one integer moves and all
   island/patch/defect evidence is byte-identical, and attribute the change to `908d6df` in
   the commit message. Obtain independent review of that one-line diff. Once the workspace
   gate is green, delete `codex/integrate-canonical-delicacy-o5` and its worktree
   `/private/tmp/plinth-canonical-delicacy-integration`, which are retained only for that
   reason.

   The accepted preparation interface and stable CLI parser are already integrated as
   `92fa26a` and `2b933c`. Canonical delicacy is integrated as merge `240d144`; the
   M0-value and signed-zero regressions in `crates/plinth-mesh/src/delicacy.rs` remain an
   open non-blocking follow-up.

3. Re-review the blocked resource-policy commits once step 2 is done. The 2026-08-04
   diagnosis cleared them of causing the golden failure, so the surviving objection is the
   missing test coverage: no-work tests for invalid decimation configuration; malformed
   support zero/MAX/nested-cap refusal; and external compile-fail tests proving
   `SupportPreparationInput` fields cannot be constructed directly. Rebase the branch onto
   current integration (it is missing `b692678`, `6d7a046`, `b5a3290`), re-run the full
   gate, obtain a fresh independent review, and only then cherry-pick the commits
   individually.

4. Repair `codex/embed-zones`. Conditions 1-4 of the O-11 decision hold and the sealing
   work is sound; the required work is to (a) delete `embed_inset_mm` and both
   `.max(0.5)` cosine sites in `crates/plinth-support/src/brace.rs` and route the embed
   self-mask through a validated `DesignedEndMask`, (b) make `exact_clearance_rows` refuse
   when a designed extent is absent instead of projecting one from the surface normal,
   (c) wire `validate_endpoint_connected` into a real consumer so the module stops being
   dead code, and prove consumption with a test, (d) add a whole-forest entry point that
   gathers up to 2N rows into one native call, with a call-census test asserting 1 call for
   N > 1 members, and (e) make the 1/4/16 determinism test a real `PLINTH_THREADS` sweep in
   the style already used in `brace.rs` and `rewire.rs`. Obtain independent review before
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
