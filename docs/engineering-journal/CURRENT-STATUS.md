# Rust migration current status

Durable handoff recorded 2026-08-03; updated after the 2026-08-04 integration pass, and
extended 2026-08-06 with the FU-8b resolution and owner ruling R6. This
file distinguishes integrated work from reviewed-but-branch-only work and from work still
awaiting review. A review result is not evidence that a commit has been merged.

## 2026-08-09 — the pre-write gate is wired, and it refuses three corpus models

<a id="pre-write-gate-2026-08-09"></a>

Until this landing, **nothing consulted a physics verdict before a `.chitubox` was
written**. `plinth_emit::write` took no verdict, `plinth_verify::verify_coverage` had no
non-test caller, `EMIT_REQUIRE_PASSED_GATES_DEFAULT` had zero readers, and
`PipelineExitCode::Gates = 3` was reserved and unproduced. The only verification that ran
was the format gate, *after* the write, on the bytes.

`plinth_pipeline::gates` closes that. `judge_then_emit` judges the sealed verification
snapshot while the authority is still un-lowered, and only a passing verdict reaches
`into_supported_model` and `plinth_emit::write`. A refusal returns
`PipelineError::Gates` (exit code 3) with the full `CoverageReport` attached and **no file
at the target path**. There is no override parameter and there will not be one:
`EMIT_REQUIRE_PASSED_GATES_DEFAULT` is read at the gate and can only ever refuse, which is
the standing retirement of `PERF_GATE_EMIT_ON_VERIFY` made executable. The mandate is
structural rather than documentary: `judge_pre_write_gates`, which takes a caller-supplied
coverage request and is therefore an override in all but name (a large enough radius makes
every layout "cover"), is **private**, and `PreWriteGateReport` is constructible only
through it — so `judge_then_emit` is the sole public door to the gate set.

**"No file at the target path" is a claim about the path, not about one call**, and it is
enforced as one. `plinth_emit::write` saves to a staging sibling and renames into place
only *after* the disk audit accepts, so an audit rejection can no longer leave the file it
just refused; and `judge_then_emit` removes anything at the target on every refusal,
including an artifact an *earlier* run left there, which a gate refusal never reaches
`write` to clean up. The acceptance test — "the output path does not exist" — is pinned in
both crates, and one of the `plinth-emit` pins used to be a `remove_file(...).unwrap()` on
the refused artifact.

**What it measures.** Gate 1 of the twelve in `00-migration-guide.md` is ported —
uncovered islands, through `verify_coverage`. Its radius and patch-area floor are the
*planner's own*, read from the same registered constants `plinth-support` builds its
selection config from (`COVERAGE_RADIUS_FACTOR × TARGET_SPACING_MM` = 6.0 mm, and
`VOXEL_PITCH_MM²` = 0.16 mm²), so the gate measures the promise the planner made rather
than inventing a second coverage physics. Its resource ceilings are the support policy's
own registered coverage caps, through
`SupportPreparationLimits::coverage_measurement_limits`. **No new constant was
registered and no cap was raised, lowered or bypassed.** The other eleven gates are named
in the same enum and reported through `PreWriteGateReport::pending`, so the incompleteness
is a value a caller can read rather than eleven silent omissions.

**The finding.** At the planner's own radius, three of the six reachable corpus models
leave uncovered islands and therefore no longer reach disk:

| model | verdict |
|---|---|
| `Sphere` (drilled shell) | refused — 2 blocking island defects |
| `make_overhang_table` | refused — 1 blocking island defect |
| `make_raking_blade` | refused — 2 blocking island defects |
| `make_box` | passes, emits |
| `make_toed_slab` | passes, emits |
| `make_thin_fin` | passes, emits |

Those artifacts were being produced and handed to the owner before this landing. The
uncovered islands are a **support-planning finding**, not an export-seam one; what the
export seam owed was that they stop the file, and now they do. A gate refusal is
deliberately *terminal* in the first-artifact chain: it does not fall back to the
oriented-solid reserve, because the reserve answers "this geometry could not be sealed",
not "the sealed plan does not adequately support the model" — substituting a different
geometry there would hand the owner a plausible artifact for a question the engine just
failed.

Landed alongside, in the same branch:

- **`negative_shells` corrected.** The metric counted the mandated hollow cavity, which
  made its `= 0` acceptance bound unsatisfiable for every correctly hollowed model. The
  bound is unchanged; the counter now exempts a sealed shell nested at odd depth inside a
  positive shell — **at most one of them**, because the rule the exemption rests on
  (`plinth-hollow::validate_shell_topology`) is *exactly one*. Every surplus cavity is
  counted back in, so a genuine cavity paired with a nested inverted duplicate still fails
  the bound; `cavities` carries no absolute bound, so this independent disk auditor is the
  only thing that would catch it. See `04-migration-plan.md` §3.
- **Plate-bound ulp false reject fixed.** `plinth-emit`'s disk audit measured world Z
  against a second, separately rounded derivation of the plate; when `lift` and
  `lift + centre_z` straddle a binade boundary it double-rounds and a raft pad sitting
  exactly on the plate reads 1–2 ulp below it. The audit now measures against the binary32
  the pads were actually emitted at; the frame check still binds the two derivations
  together.
- **The gate's coverage preflight is batch-shaped (D-37).** Wiring the gate put
  `SupportVerificationSnapshot::measurement_preflight` and `measure_coverage` on every
  production emission, and both answered two per-sample questions with scalar scans — a
  rescan of the sample prefix per sample for the distinct route keys, and a linear island
  walk ending in a linear `Vec::contains` per sample for island membership. That is
  `O(samples²)`: at the Weeping Treant's ~4×10⁵ coverage samples, ~10¹⁰–10¹¹ comparisons
  per artifact, and the branch's corpus evidence only covered second-scale models. Both
  are now one gathered pass — a sort/dedup for the keys, and a sorted island index
  answering each sample by two binary searches, which the sidecar's already-proved
  invariants (unique island ids, ascending face ids) support. A 200,000-sample fixture
  pins it: the shapes it replaced cannot finish that test.
- **The coverage gate's kNN ceiling has one owner.** `coverage_measurement_limits` fed the
  specialized stage's coverage *scratch* ceiling (512 MiB) into the coverage-route kNN
  batch, whose registered owner is `coverage_route_knn.max_logical_peak_bytes` (2 GiB) —
  one cap doing two jobs at a quarter of the second job's ceiling, contradicting the
  function's own doc. The batch now gets its own registered ceiling; the measurement's
  *retained peak*, which legitimately includes that peak as one summand, keeps the scratch
  ceiling. **No cap was raised**: the aggregate bound is unchanged, and what moved is which
  cap a refusal is attributed to.

## 2026-08-05 — the line of record moved to `develop`

Everything below this section was written while the engine line lived on a topic branch in
a temporary worktree. That is no longer true, and where the older text names that branch or
that path as "the integration authority", read it as history rather than as instruction.

**The single line of record is now `develop` in the definitive checkout
`/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`.** The repository follows git flow:
`main` and `develop` are the long-lived branches, `main` is release-only and moves only when
the owner releases `develop` into it, and every new piece of work starts as a `feature/*`
branch cut from `develop` and merges back into `develop` with `--no-ff`.

`codex/rust-pipeline-integration` was merged into `develop` in full — a true two-parent
merge of its tip `cef27a1`, no squash and no rebase, so the whole engine arc including the
caps landing (`3d91390`, `e3670ab`, `ef88ab0`) and owner rulings R1–R5 is preserved commit
for commit in `develop`'s history. The README overhaul (`feature/docs-readme-overhaul`,
tip `207f84d`) was merged immediately after so its documentation links resolve against the
engine tree. **With those merges recorded, the integration branch and its worktree
`/private/tmp/plinth-3d-pipeline-integration` are retired and deleted as part of this
commit.** Containment is the merge itself: `cef27a1` is an ancestor of `develop`.

The five gates were run on the merged `develop` tip and are green. The authority records for
the caps landing are this file and
[`support-resource-caps-2026-08-05.md`](support-resource-caps-2026-08-05.md), both now on
`develop`.

Practical consequences for anyone resuming:

- Work in `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, not in `/private/tmp`.
- Read the live head with `git -C /Users/craig/code/hughescr/dnd-campaigns/plinth-3d log`,
  not from the retired integration worktree, which no longer exists.
- "Integrated" now means **an ancestor of `develop`**. The status vocabulary below still
  says "the integration branch"; substitute `develop` throughout.
- Start new work on `feature/<topic>` off `develop`. Never push to the `github` remote
  without the owner's explicit instruction.

## Status vocabulary

- **Integrated:** the commit is an ancestor of the integration branch.
- **Accepted, branch-only:** an independent review accepted the commit, but it is not
  an ancestor of the integration branch. Integrate the named commit deliberately after
  inspecting its patch; do not merge the branch head blindly.
- **Blocked:** review found required changes. Do not integrate it.
- **Pending review:** implementation exists, but acceptance has not been recorded.
- **Uncommitted:** worktree-only state. It may be lost and is never integration evidence.

## Integration authority and verified snapshot

> **Superseded 2026-08-05 — historical.** The integration authority *was* the clean worktree
> `/private/tmp/plinth-3d-pipeline-integration` on branch `codex/rust-pipeline-integration`.
> That branch has been merged into `develop` in full and both the branch and the worktree
> are deleted; see *The line of record moved to `develop`* at the top of this file. The
> commits and acceptances recorded in this section remain valid — they are all ancestors of
> `develop` — but the paths and branch name in it are no longer live. Read the heads named
> below as history.

The current reviewed integration HEAD is the **contact-physics package merge**
`81ec3d446f9429a5ca5cade6b957164f386134e5` (`merge: contact-physics package complete -
PR-1..9 (PR-7 deferred w/ tripwire), OD-1..9 discharged, attestation for all three
families`), plus the 2026-08-05 commits that follow it: `6fe750b` (delete the census
substitute-orient mechanism), `41f1801` (census erratum + first unified corpus census),
and this docs update. See *Embed/contact package complete (2026-08-05)* below, and then
*Post-merge interaction delta — ATTRIBUTED and re-pinned (2026-08-05, late)*, which is the
closing pass and supersedes the red-gate section.

`81ec3d4` was reached from `1b6f2e47270de4373763d41b1d3f423078b50eee`
(`fix(support): coalescing continues the shared merge-iteration budget`), which was reached
from `a3c7f6464dd219da7b7628183e96c740d5e194d6` (embed-zones Phase B safe groundwork,
itself reached by the second 2026-08-04 integration pass described below) via three
docs-only commits recording the embed-zones stop-by-design outcome and owner rulings
OD-8/OD-9 (`8756a65`, `dec7152`, `ff96774`), then the **corpus defect fixes merge**
`65582556d2b748b8467a7cd651e3bb4eca659e56` (baselines data repair, terminal
exact-coincident seat coalescing, perception unowned-hole fallback — merged from
`codex/corpus-defect-fixes` off `dec7152`) and the reviewer-mandated follow-up fix
`1b6f2e4` (shared whole-consolidate merge-iteration budget). See *Corpus defect fixes
landed (2026-08-04, late)* below for full evidence.

The second-2026-08-04-integration-pass table below records how `a3c7f64` itself was
reached, from `1673a1628b1af04ec180fc204db84675eec8fc10`, plus the embed-zones Phase B
safe-groundwork merge:

| Commit | What |
| --- | --- |
| `1eb538c72b957606e48ba665df609a0e657d1485` | merge of `codex/delicacy-regressions` — M0-value and signed-zero delicacy regressions (test-only, +69/-0 inside `delicacy.rs`'s `mod tests`) |
| `4c8881eaf4186b55a9f05254dde80769daad69bc` | comment correction on that test's sole-reader claim |
| `c6a0884a19c591f010d073b38e48e60495a469c9` | merge of `codex/prepare-interface-v2` — preparation resource policy plus three sealed-input test classes |
| `029ced3f97ccebb8ba6593efba2d732493c3c625` | `refactor(support): rename EmbedEnd to ContactEnd` from `codex/embed-zones` — safe groundwork only; see *Embed-zones Phase B outcome (2026-08-04)* |
| `a3c7f6464dd219da7b7628183e96c740d5e194d6` | merge of the rename above — `merge: embed-zones Phase B safe groundwork` |

The full gate was **green** at `a3c7f64`: `cargo test --workspace --offline` **889 passed /
0 failed**; clippy `-D warnings` clean; `cargo fmt --all -- --check` clean; `git diff --check`
clean. (Full gate was previously green at `c6a0884` with 877 passed; the higher count is
the embed-zones Phase A test surface — including `designed_embed.rs`'s own suite — entering
the workspace via the `e64f0ba` merge, not new tests from the rename itself.) That was the
first pass in which the workspace gate was fully green — the frozen coverage golden was
regenerated in the `1673a16` lineage, so it no longer masks regressions.

The full gate is **green again** at the current head `1b6f2e4`: `cargo test --workspace
--offline --no-fail-fast` **903 passed / 0 failed**; clippy `-D warnings` clean; `cargo fmt
--all -- --check` clean; `git diff --check` clean. See *Corpus defect fixes landed
(2026-08-04, late)* below for what moved between these two green gates.

Earlier ancestry still holds: the canonical-delicacy merge
`240d144e264cbe9ba27d45f6ff0194eb83a36ced`, the independently accepted SCAR remediation
`b5a3290`, the preparation interface integration `92fa26a`, and the stable CLI parser
integration `2b933c`. Re-read the live head before resuming; never infer acceptance merely
from presence on the integration branch.

### 2026-08-04 integration pass {#integration-pass-2026-08-04}

Four backlog items were processed in order by a single writer on
`codex/rust-pipeline-integration`. Outcomes:

| Item | Outcome | Evidence |
| --- | --- | --- |
| canonical delicacy authority | **Integrated** as merge `240d144e264cbe9ba27d45f6ff0194eb83a36ced` (`merge: integrate canonical delicacy authority`), second parent `5f281d7e28f7761cca052f202a887e09bff05afb` | Independent re-review 2026-08-04 returned ACCEPT. `git diff 027a9ef 240d144 --stat` = 4 files, +474/-5: `crates/plinth-mesh/src/delicacy.rs` +372, `crates/plinth-mesh/src/lib.rs` +5, `crates/plinth-metrics/src/scar.rs` 100, `crates/plinth-metrics/tests/scar_metrics.rs` 2. Nothing else moved. |
| endpoint-connected embed zones | **Not integrated — Phase A landed on branch; re-review still BLOCKED** | Branch head is now `4b1d4b3` (batching fixed, real `PLINTH_THREADS` sweep, module sealed `pub(crate)`). **Phase B is now authorized** under *Owner embed-zones rulings 2026-08-04*. Re-review remains BLOCKED until Phase B completes. |
| preparation resource policy | **INTEGRATED** (second pass, same day) as merge `c6a0884a19c591f010d073b38e48e60495a469c9` via `codex/prepare-interface-v2` | See *Second 2026-08-04 integration pass* below. The first pass left it blocked because the diagnosis chain produced no verdict; the v2 branch supplied the missing test classes and earned an independent review ACCEPT. |
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

That stale golden was the top blocker for every downstream item. It has since been
regenerated in the `1673a16` lineage, and the workspace gate is green as of `c6a0884`; the
paragraph above is retained as the historical diagnosis, not a live condition.

### Second 2026-08-04 integration pass {#integration-pass-2-2026-08-04}

A second single-writer pass on `codex/rust-pipeline-integration` ran from `1673a16` to
`c6a0884`. Outcomes:

| Item | Outcome | Evidence |
| --- | --- | --- |
| canonical delicacy follow-up (M0-value + signed-zero regressions) | **CLOSED** | Merged as `1eb538c` from `codex/delicacy-regressions`. Diff is test-only: +69/-0 in `crates/plinth-mesh/src/delicacy.rs`, entirely inside the `#[cfg(test)] mod tests` that begins at line 315 (new hunk `@@ -352,0 +353,69 @@`). Mutation-verified by independent review 2026-08-04. One reviewer-mandated comment correction landed as `4c8881e`: the test claimed the production path was "the sole reader of these three registry entries", which is false — `crates/plinth-metrics/src/scar.rs:183-186` and `:889-891` read the same three entries — so the claim was softened to *sole reader in this crate*. Branch and worktree deleted after the green gate. |
| preparation resource policy | **INTEGRATED** as merge `c6a0884` via `codex/prepare-interface-v2` | Independent review ACCEPT 2026-08-04. The policy commits `e4db5d0`/`7b35c49` were replayed on the regenerated-golden base as `73e7c13`/`0b0f27a`; three test classes were added (execution-budget refusal, malformed-cap refusal, sealed-input compile-fail) and three latent `usize::MAX` fixture defects fixed. The 64 MiB grid cap at the fallout sites was confirmed by the orchestrator. One reviewer-mandated test correction landed as `457c106` before the merge (see below). Merge diff: 23 files, +985/-152. |

**Reviewer-mandated correction in `457c106`.** The test formerly named
`refusal_precedes_any_probe_surface_construction`
(`crates/plinth-decimate/tests/execution_budget_refusal.rs`) asserted a false mechanism: its
degenerate collinear fixture never reaches surface construction at all. It dies in
`Mesh::audit_strict` inside `simplify_locked`
(`StrictMeshAuditError::ZeroAreaOrCollinear`, surfaced as
`FeatureProtectionError::ProbeSimplify`) **before** any `NativeSurface` is built. The test
was renamed `refusal_precedes_the_feature_probe`, its comment reworded to the true
mechanism, and its baseline `matches!` pinned from the blanket
`Err(DecimationError::FeatureProtection(_))` to the exact
`FeatureProtectionError::ProbeSimplify { .. }` variant.

**Old-branch retirement with containment proof.** `codex/prepare-interface` @ `7b35c49` was
deleted as superseded. `git cherry HEAD codex/prepare-interface` reported:

```text
- 4809e367c723faedc2aa3b4393a906c38d2e69c5
- e4db5d08fca8b4739e137f13bbf7694c0b94ca57
+ 7b35c49ababa867d8fa93a1d13454ee26e6afa7a
```

`4809e36` is base content already integrated and `e4db5d0` is patch-identical to its replay
`73e7c13`. The `+` on `7b35c49` is a patch-id artifact, not missing content: comparing only
the changed lines of `7b35c49` against its replay `0b0f27a` shows them **byte-identical**
(same 10 files, same +74/-91). Only blob hashes, hunk offsets, and context lines differ,
because the replay sits on the regenerated-golden base where `coverage_gate.rs` carries
extra explanatory comment lines. The same comparison for `e4db5d0` vs `73e7c13` is likewise
identical. Content containment is therefore complete.

### Owner embed-zones rulings {#owner-embed-rulings-2026-08-04}

The following are **binding owner decisions of 2026-08-04**, constituting the approved
remediation package for `codex/embed-zones`. They supersede the corresponding parts of the
earlier blocking findings.

- **OD-1 — touching-foot families are not embeds.** The touching-foot families (brace model
  rungs, CCO model-anchor struts, CCO upper inheritance) are **not** embeds. They stop
  declaring `embed_end` and keep today's geometry unchanged.
- **OD-2 — clearance cushion becomes a named design allowance.** The clearance cushion those
  families relied on becomes an explicitly named, **separately tested** design allowance for
  the distance check, preserving `axis_illegal_rows` semantics. The retired formula's
  grazing-incidence rejection becomes an **explicit angle limit at the same `|cos| >= 0.5`
  threshold** rather than an emergent side effect.
- **OD-3 — designed extents are recomputed, never stored.** Designed extents are recomputed
  per stage from the design family (kind/owner) plus policy. They are **never** stored on
  `Member`, so there is no digest or golden churn.
- **OD-4 — requested embed extents carry a margin.** Requested embed extents carry a margin
  below placement depth, with the numbers fixed in design and covered by convex-fixture
  tests; the connectivity sampling step must be able to resolve that margin. A refused
  genuine-contact stub routes to the existing `initial.rs` quarantine — **never** a hard
  pipeline failure.
- **OD-5 — single-declared-end is an accepted temporary limitation.** The `EndNotDesigned`
  refusal tests are **temporary pins**, to be revisited if `Member::embed_end` widens.
- **OD-7 — CCO inheritance bug is fixed.** The cco upper/host embed-end inheritance bug is
  fixed so the declaration follows the end that owns the embedded geometry.

**Owner's binding clarification, verbatim:**

> Well, the tip is never "embedded" as such - when the mesh gets unioned it's all just
> continuous surface! I guess if you track it during the build as embedded, that's what we
> want - but the important element to track is the surface where the support tip actually
> contacts the pre-existing mesh. The volume inside the mini will "disappear" because the
> mini already had voxels there! So it's important for any geometry which is *not* embedded
> in the mini mesh that we do actually get the proper adhesion/contact all along the area of
> the mesh that requires the contact. [...] we can go with your recommendation, but be sure
> that the new code properly attaches the tips to the mini for all cases!

**Normative reading.** Post-union there is no interior "embed volume" — it is absorbed into
the mini's own voxels. The tracked and verified quantity is therefore the **contact
surface**. Every support tip, embedded or merely touching, must be **affirmatively verified
to attach across its required contact area, in all cases**. Design and tests must measure
contact area, not embedded volume.

## Embed-zones Phase B outcome (2026-08-04) {#embed-phase-b-outcome-2026-08-04}

Phase B ran as the authorized four-Opus-agent workflow — design, adversarial challenge,
implement, independent re-review — against the *Owner embed-zones rulings* above. It
**stopped by design**: only safe groundwork landed on `codex/embed-zones`, because the
redesign itself surfaced a falsified premise that blocks further implementation until the
owner rules on it.

Two commits landed and are now merged to `codex/rust-pipeline-integration` as recorded
under *Integration authority and verified snapshot* above: merge `e64f0ba` (bringing
`codex/embed-zones` current with integration `5f006d4`, zero conflicts) and rename
`029ced3` (`EmbedEnd` -> `ContactEnd`, `Member::embed_end` -> `model_contact_end`;
`DigestWriter` tag bytes 0/1/2 unchanged, so every digest and golden is bit-identical). The
independent re-reviewer verified the diff residue is identifier substitution plus doc
comments only, and independently re-ran all four gates green at `029ced3` — **889 passed /
0 failed**. `029ced3` is now merged to integration as `a3c7f64` by this commit.

### The falsified premise

OD-2 assumed the retired `.max(0.5)` cosine floor in `embed_inset_mm` (`brace.rs:1420-1422`,
floors at `brace.rs:1381` and `:902`) was a grazing-incidence **rejector**. It is the
opposite — the **conservative branch**. `seg_clear_one` returns `INFINITY` (candidate
trivially **admitted**) when insets over-mask the segment
(`rust/geom_kernel/src/queries.rs:1462-1466`), and `axis_illegal_rows` cannot flag an
over-masked row (`brace.rs:1110` gate). Flooring cosine **up** at 0.5 makes the inset
**smaller**, the mask smaller, the check **stricter**. At `|cos| < 0.5` the floored
model-rung inset is a constant `0.3 + (0.5+0.6)/0.5 = 2.50 mm` against the legal rung range
`[1.0, 5.0] mm` (`brace.rs:895-898`), so every grazing rung with `L <= 2.50 mm` is admitted
unconditionally today.

Consequences: an explicit `|cos| >= 0.5` rejection is strictly **more restrictive** than
shipping behavior — it deletes candidates admitted today; the brief's fallback (true
cosine, no floor) is **more permissive** — larger insets, more over-mask `INFINITY` admits,
fewer `exact_clearance_violations` at `brace.rs:1295-1307`, fewer `pass_three_termination`
hard errors at `brace.rs:385-391`. **Neither replacement is behavior-preserving**; every
existing fixture stays green either way.

Found by the implement agent, independently re-derived from source by the re-reviewer,
constants confirmed at `placeholders.rs:1097`/`:1003`/`:1338`/`:1014`/`:1241`.

### Re-review verdict: BLOCKED

Contact-surface requirement (owner ruling 2026-08-04): **UNMET**. The Phase A
endpoint-connectivity proof exists (`designed_embed.rs:519-590`) but has no production
caller, and no touching-foot contact verification of any kind exists yet. Nothing was
built beyond the safe groundwork above — the redesign correctly stopped rather than build
on a falsified premise.

### Open owner questions (OQ-1..OQ-7) — Phase B blocked on these

- **OQ-1.** BLOCKING, carried forward and independently confirmed — OD-2 needs owner RE-APPROVAL, not revision. The retired `.max(0.5)` floor was never a grazing rejector. `seg_clear_one` returns INFINITY, i.e. trivially clear and candidate ADMITTED, whenever the insets over-mask the segment (rust/geom_kernel/src/queries.rs:1462-1466), and `axis_illegal_rows` cannot flag such a row either (brace.rs:1110). At |cos| < 0.5 the floored model-rung inset1 is a constant 0.3 + (0.5+0.6)/0.5 = 2.50 mm, so every grazing rung with L <= 2.50 mm inside the legal [1.0, 5.0] mm range (brace.rs:895-898) is admitted unconditionally today. Clamping cos UP made the mask SMALLER and the check STRICTER — the floor was the conservative branch. An explicit |cos| >= 0.5 rejection is therefore a NEW, strictly more restrictive rule that deletes candidates today's code admits. Does the owner want (a) the honest allowance with NO incidence rule, replacing the floor's real job with an explicit MASK BOUND (refuse or clamp the row when inset0+inset1 >= length) rather than an angle limit; (b) the angle limit anyway, with the corpus delta measured and owner-reviewed before it lands; or (c) the angle limit installed only where a corpus census first proves it inert?

- **OQ-2.** BLOCKING, and mine to add — the brief's own fallback is not preserving either, so option (a) above does not escape the problem. The brief permits the allowance to 'reproduce the old arithmetic with the explicit true cosine, no floor'. Below |cos| = 0.5 that is arithmetically different in the OPPOSITE direction: the true cosine is smaller, so inset is LARGER, so MORE rows over-mask into INFINITY, so `exact_clearance_violations` (brace.rs:1295-1307) counts FEWER violations and `pass_three_termination` (brace.rs:385-391) hard-errors LESS often. That is a permissive change to a hard-error count. Does the owner accept a measured net delta in the surviving set (loosened by the allowance, tightened by any incidence rule), or must the replacement be arithmetically identical to the floored formula wherever the floor was active — which would mean keeping the clamp under a new name and honest documentation rather than deleting the arithmetic?

- **OQ-3.** BLOCKING, carried forward and confirmed — MODEL_CONTACT_MIN_CONVEX_R_MM needs an owner number, and the design's 0.02 mm margin is calibrated at the wrong radius. The embedded tip radius is `loads.tip_radii_mm()[slot]` (initial.rs:355), clamped to [0.200, 1.0] mm by `FrozenTipLoadPolicy::tip_radius_mm` (perception.rs:455-462, placeholders.rs:1367/:1369), not min_shoot_r_mm = 0.35. Rim rays start at p0 + unit*r_tip (queries.rs:144-146) and exit a convex feature early by ~r_tip^2/(2R), so the coverage boundary R >= r_tip^2/(2*margin) runs from 1.0 mm at r = 0.2 to 25 mm at r = 1.0. No single value works: a conservative R_min quarantines fingers and blade edges (supports vanish, a printed-geometry change the contract forbids), while R_min = 0.5 forces a 1.0 mm allowance at the largest tip, exceeding the 0.30 mm placement depth entirely. Does the owner want (a) one conservative R_min with large tips refused on tight features, (b) a per-tip cap so an unprovable stub is placed but classified Touching rather than Embedded, or (c) placement depth increased for large tips?

- **OQ-4.** BLOCKING, carried forward and confirmed — does the incidence limit govern CCO model-anchor struts? `cco::apply_model_anchor` (cco.rs:2745-2750) screens only `within_cone`, a lean check against vertical, never incidence against the surface normal, so a grazing strut is reachable and lands in `exact_clearance_rows` where the only consequence available is the violation count that `pass_three_termination` (brace.rs:385-391) converts into `Err(BraceError::ClearanceViolation)` at the third pass. That makes the limit enforceable only where enforcing it hard-fails the pipeline, contradicting OD-4's 'never a hard pipeline failure'. Does OD-2 authorize an incidence screen at the CCO model-anchor seat, a site that has never carried a screening rule and where refusal would be an ordinary candidate rejection, or are struts exempt?

- **OQ-5.** OD-1 confirmation needed before the rename can be credited. The landed change keeps ONE field, renamed `model_contact_end`, with the embed-vs-touch split deferred to a recomputed OD-3 property that does not exist. Literal deletion is genuinely not implementable without changing printed geometry — `lower::is_embedded_taper` (lower.rs:356-359) gates `SupportSegmentKind::ContactTaper` and its `contact_depth` extension on this field, and the CCO strut is genuinely tapered (cco.rs:2755-2756) — but a single field means `validate_members_batch` still reads the contact declaration to decide who owes an embed extent (designed_embed.rs:335, :359), which is the conflation OD-1 orders removed. Does the owner want a SECOND field carrying the touching contact so the embed claim can be recomputed independently, or does the rename plus a classifier-gated totality predicate satisfy OD-1?

- **OQ-6.** The contact-surface standard needs a definition before it can be built. A touching foot seated at the incidence limit passes any seating-plus-overhang test, since the tangential threshold r_T*sqrt(1-cos^2) tends to r_T as cos -> 0. If 'attach across its required contact area' means a measured mm^2 threshold, that is a physics number no ruling fixes and it needs its own calibration evidence. Confirm that seating-plus-non-overhang is the intended Phase B standard with a measured area threshold deferred, or supply the threshold.

- **OQ-7.** Process question for the next pass. The corpus is provisioned (all 8 models including Sphere.stl and the Weeping Treant, symlinked into corpus/models/), so the census that would answer question 1 empirically — how many model-rung rows on the corpus actually have |cos| < 0.5, and how many of those are in the L <= 2.50 mm unconditional-admit band — is available today and requires no design decision. Should the next agent be directed to produce that census FIRST, as a standalone measurement commit, before any design revision is written?

A corpus incidence census (branch `census/embed-incidence-2026-08-04` off `029ced3`) is
being run to answer OQ-1/OQ-2 empirically and inform OQ-3; results will be recorded here.

### Owner ruling OD-8 (2026-08-04, evening) — correctness supersedes behavior preservation {#owner-ruling-od8}

1. The owner's verbatim ruling, delivered 2026-08-04 after reading the Phase B stop report:

   > "I have no idea what that Max 0.5 thing is or where any rule not to change it came
   > from. You should figure out what the correct process/math/physics should be and
   > implement that please."

2. Binding interpretation:

   - The behavior-preservation constraint on the embed/contact package is LIFTED. It was
     integration scaffolding (verify-the-refactor-by-identical-output), imposed by the
     orchestration layer — never owner policy and never physics. It died when the Phase B
     agents proved the `.max(0.5)` floor is load-bearing for candidate survival, making
     "clean AND identical" impossible.
   - New standard for this package: derive contact, clearance-mask, and incidence rules
     from correct geometry and measured physics (the calibration-plate area-ratio law and
     its X_ALLOWED=12 design margin are the measured evidence base), implement them, and
     MEASURE the resulting corpus delta (supports added/removed/moved, per model) instead
     of requiring zero delta.
   - Silent geometry change remains forbidden: every behavioral difference must be
     measured, reported, and physics-justified. The owner's 2026-08-04 contact-attachment
     requirement ("be sure that the new code properly attaches the tips to the mini for
     all cases") stands unchanged and is the design's governing requirement.
   - OQ-1 through OQ-7 are hereby DELEGATED by the owner to the orchestrator, to be
     resolved on physics/geometry grounds with proposer-challenger adjudication. None of
     them requires a further owner ruling unless a print-consequence tradeoff emerges that
     has no physics-preferred answer.

3. Next step is Phase B-2 (physics-first redesign), launching once the corpus incidence
   census (branch `census/embed-incidence-2026-08-04`) reports; census numbers feed the
   design as its empirical baseline.

### Owner requirement OD-9 (2026-08-04, evening) — no attachment to buried surface {#owner-requirement-od9}

1. The owner's verbatim statement, delivered 2026-08-04 evening:

   > "If we don't smoosh before slicing then we also need to be sure that we're never
   > trying to attach a support to something inside the model"

2. Binding interpretation:

   - **Context.** Input meshes for real minis are watertight and manifold but
     SELF-INTERPENETRATING (established by the 2026-08-04 Weeping Treant diagnosis: 23,220
     non-adjacent intersecting triangle pairs in one 2.5x1.8x0.8 mm region of a
     topologically perfect mesh). The decision of record is per-layer polygon union under a
     fill rule (owner-endorsed, filed as its own work item), NOT a 3D mesh boolean.
     Consequence: the 3D triangle mesh retains BURIED surface — surface patches lying
     inside the printed solid.
   - **Requirement.** No support may attach to buried surface, for any contact kind. Note
     the trap explicitly: buried surface passes naive verification — a touching foot seated
     on a buried patch is genuinely "on the surface", and an embedded tip there passes an
     endpoint-connectivity embed proof TRIVIALLY (its whole neighborhood is interior). The
     distinction between "designed embed" and "attachment to surface that does not exist in
     the print" therefore requires an explicit exterior-ness test; no existing check
     subsumes it.
   - **Two mandated defense layers.** (a) STRUCTURAL — support-candidate nomination must
     consume union-normalized layer data, in which buried contours never appear (delivered
     by the per-layer union work item); (b) AFFIRMATIVE — the contact-verification
     machinery (embed Phase B-2 package) must include a batched exterior-ness screen: for
     every proposed attachment point, offset epsilon outward along the surface normal and
     test inside/outside (parity/winding query against the model); exterior surface tests
     outside, buried surface tests inside. Refusals route to quarantine per OD-4, never a
     hard pipeline failure. One batched native query per stage, per the batch-first rule.
   - **Applicability note.** No currently-reachable corpus model self-intersects, so this
     requirement is not yet exercisable in tests against real geometry; it MUST be verified
     against the Weeping Treant when it reaches the support stages (after the resource-caps
     follow-up). Until then, a synthetic self-interpenetrating fixture (two overlapping
     primitives) is the acceptance vehicle.
   - **Adjudication hook.** When the in-flight Phase B-2 package is reviewed for
     integration, its contact_surface machinery is to be audited against OD-9; if the
     exterior-ness screen is absent (expected — OD-9 postdates the B-2 design), it is
     landed as an immediate follow-up on the same branch before the branch retires.

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

## Corpus defect fixes landed (2026-08-04, late) {#corpus-fixes-2026-08-04}

Merged to integration as `65582556d2b748b8467a7cd651e3bb4eca659e56` (`merge: corpus defect
fixes - baselines, seat coalescing, treant seam fallback (7 of 8 models reach sealed
support runs)`), the SHA-verified tip of `codex/corpus-defect-fixes` (`4a9bc37`, based on
`dec7152`; merge was zero-conflict), plus the reviewer-mandated follow-up fix
`1b6f2e47270de4373763d41b1d3f423078b50eee` (`fix(support): coalescing continues the shared
merge-iteration budget`).

### Three fixes

1. **Baselines data repair + orient reader + machine-provable rederivation gate.** Fixes
   the two identity-orientation baseline records to be explicit and provable, and a failed
   hollow stage now carries its ratified orientation forward rather than losing it. Adds a
   machine-provable rederivation gate: residuals `<= 3.9e-6 mm` across all four artifact
   models.
2. **Terminal exact-coincident seat coalescing.** Unions exactly-coincident demand-free
   regular seats that the existing merge machinery could not reach (`attach_exact_mixed_contacts`
   only pairs regular against specialized; the gated tournament refuses demand-free pairs
   outright), preventing a spurious `DuplicatePhysicalSeat`. Reported through a new
   `coalesced_seat_count` evidence field on `SpecializedGateEvidence`, deliberately excluded
   from the plan digest to keep already-covered fixtures byte-identical.
3. **Perception unowned-hole non-fatal fallback.** An unclaimable slice hole is now treated
   as survivable rather than a fatal open seam, with the perception error type split into
   distinct variants so the non-fatal and fatal cases are no longer conflated.

### Reviewer-measured reachability: 7 of 8 corpus models (was 3 of 8)

Before this merge, 3 of the 8 corpus models completed a sealed support run under their own
ratified orientation. After, **7 of 8** do:

- **Sphere, make_overhang_table, make_thin_fin** — byte-identical digests (unaffected,
  confirming no regression).
- **Cylinder, make_box** — unblocked on native identity orientation.
- **make_toed_slab, make_raking_blade** — now go through specialized and sealed
  end-to-end.
- **Weeping Treant** — now gets past perception, and is stopped by the kNN resource cap:
  `KnnBatchLimits.max_work = 100_000_000` against a required `298_667_520`. The
  `CandidateSurfaceGridLimit` (`67_108_864`) and the Specialized `GateOperationLimit`
  (`2_000_000_000`) caps sit behind that same wall and have not yet been exercised.

### Provisional freeze: make_toed_slab

Containment-probe hardening was **deliberately deferred mid-implementation** because it
moves the `make_toed_slab` digest (member count `72 -> 42`): the stop-on-digest-movement
rule took priority over the ship-it instruction. Consequently the `make_toed_slab`
required-table freeze (digest `11_859_964_443_161_510_103`) is **PROVISIONAL**, pending the
parity investigation (vertex vs. interior probe) recorded as a follow-up below.

## Embed/contact package complete (2026-08-05) {#contact-package-complete-2026-08-05}

**STATE: complete, with the post-merge interaction delta attributed below.** The
embed-zones/contact-physics package that was BLOCKED on 2026-08-04 (OQ-1..OQ-7) is closed
in scope, and the one behavioural difference the merge surfaced — on two models the package
itself could never see — is measured, named and re-pinned in
*Post-merge interaction delta — ATTRIBUTED and re-pinned (2026-08-05, late)*. Read that
section before treating any claim here as final. It merged to integration as
`81ec3d446f9429a5ca5cade6b957164f386134e5`. Its full design record is
`docs/rust-migration/contact-physics-2026-08-04.md`; that file, not this section, is the
normative account of the physics.

This section supersedes the BLOCKED verdict in *Embed-zones Phase B outcome (2026-08-04)*
and *Embed-zones blocking findings*. Those sections are retained as the historical record
of why the package stopped; do not read them as current state.

### Owner decisions OD-1..OD-9: all discharged

OD-8 (correctness supersedes behavior preservation) and OD-9 (no attachment to buried
surface) were the two that unblocked the package, and both are now implemented rather than
merely ruled. OD-9 in particular is enforced by an exterior screen at the contact sites and
has an acceptance fixture — a synthetic self-interpenetrating mesh — proving the screen
fires. OQ-1..OQ-7 were delegated to the orchestrator under OD-8 and resolved as physics;
OQ-11 was ruled and closed by striking an unreproducible figure. **OQ-8, OQ-9 and OQ-10
remain open owner questions** and are carried into the follow-ups below — they are design
questions the package deliberately did not answer, not unfinished implementation.

### PR-1..PR-9: landed, with PR-7 deferred behind a tripwire

PR-1, PR-2, PR-3, PR-4, PR-6, PR-8 and PR-9 are landed. The headline change is `ef081a2`
(`feat(support)!: delete embed_inset_mm and both cosine floors; seat model rungs`), which
deleted `fn embed_inset_mm` and *both* `.abs().max(0.5)` cosine floors — a source-text
guard now enforces their absence — and replaced them with seated geometry and derived
physics from `crate::contact`. The floor was found to have had the opposite of its believed
effect: it made the mask *smaller* and the check *stricter*, hiding the real divergence
behind a constant 2.50 mm inset.

**PR-5 `MASK_INTERIOR_SINGLE_CONTACT` is not landed** and its normative text is not
recoverable from the branch (OQ-9).

**PR-7 `CCO_ANCHOR_MASK_AT_THE_SEAT` is DEFERRED, deliberately, with a commit-time
tripwire.** The rationale is a print-consequence tradeoff reserved to the owner, not an
implementation gap:

- The CCO model-anchor family is arithmetically dead today
  (`clr <= 0.6 - 0.95 = -0.35` unconditionally). The corrected PR-2 mask is exactly what
  would revive it.
- Reviving it is not a marginal change: it introduces a support class that has **fired zero
  times on the entire corpus**, and that class attaches to — scars — the mini itself. The
  2026-08-05 unified census puts the stake at **0 surviving anchors out of 3429 attempted**
  across three models. Landing that inside this stage would have been precisely the silent
  geometry change OD-8 forbids.
- The measurement now says the family is not merely unproven but measurably a *kiss*: all
  **3186** attested anchor seats corpus-wide report `centreline = false` with 1–3 of 8 rim
  rays engaged. Without PR-7 those struts would fuse to the mini by voxel rounding, which is
  the exact failure the owner named.
- **The tripwire (`6a35d45`) is what makes deferral safe.** `NotAttached` is RECORDED during
  candidacy (and digested) but ENFORCED at commit: both CCO winner-commit paths refuse an
  unattested model anchor. Refusing during candidacy would have landed PR-7's
  family-killing consequence without landing PR-7. So the family stays dead by arithmetic,
  and on the day it stops being dead it cannot land silently — it hard-refuses instead.
  Measured cost of the tripwire: **zero**, a byte-identical census (`dd505aa`), because
  neither winner-commit gate is ever reached.

Retiring the tripwire is part of the model-anchor bundle, not a separate errand: it comes
out when PR-7 lands and the seating is real.

### Attestation is live for all three contact families

Contact attestation executes on real corpus geometry, not only on fixtures, for **stub
terminals**, **brace model rungs**, and **CCO model anchors**. The 2026-08-05 unified census
records 286 stub-terminal attestations, 2 brace model-rung seats, and 3186 CCO model-anchor
seats. Each family also has interpenetration-fixture coverage.

### Reachability and digest continuity

The 2026-08-05 unified census — the first run with the corpus fixes and the contact-physics
package both in the tree — confirms **7 of 8 corpus models complete a sealed support run
natively**, exactly as the reachability table above predicts. Nothing that was expected to
pass failed. The Weeping Treant stops at the `KnnBatchLimits.max_work` cap
(`298667520` required against `100000000`), a resource cap and not a geometry defect (FU-8).

> **Superseded later on 2026-08-05 by the support resource-cap policy.** The kNN
> work and candidate-grid caps are now model-scaled — `max(floor, per_face × faces)`
> — on the treant's own measurement, so the treant clears both walls and now stops
> at the Specialized `GateOperationLimit`. Every digest recorded in this section is
> unchanged by that change: the floors govern for all seven reachable models. See
> `docs/rust-migration/support-resource-caps-2026-08-05.md` and §3.10.8 of
> `03-physics-and-constants.md`.

All five previously recorded snapshot digests reproduce byte-identically, so **the
contact-physics package moved no geometry on any model that was already reachable before
it**. Two models *did* move — see the next subsection. Full numbers:
`docs/rust-migration/census-embed-incidence-2026-08-04.md`, section
*Unified census — 2026-08-05*.

### GATE WAS RED at this head: two stale goldens — SUPERSEDED {#stale-goldens-2026-08-05}

**SUPERSEDED 2026-08-05 (late) by *Post-merge interaction delta — ATTRIBUTED and
re-pinned*.** Retained as the historical record of how the failure was first read; two
claims in it are wrong and are corrected there. Do not read it as current state.

`cargo test --workspace --offline --no-fail-fast` at that head was **939 passed / 1 failed**.
The failure was `public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing`
(`crates/plinth-support/tests/support_runner_integration.rs:130`):

| model | pinned (pre-contact-physics) | now | delta |
| --- | --- | --- | --- |
| `make_toed_slab` | 65 routed, 184 live, digest `15554091706332339621` | 64 routed, 179 live, digest `4826202191472851835` | -1 routed, -5 live |
| `make_raking_blade` | 35 routed, 93 live, digest `6761406004527881770` | 32 routed, 89 live, digest `1709218010932846012` | -3 routed, -4 live |

This is **not a regression and not a surprise** — it is the known deferred re-pin. Those
goldens were last written at `414e13f` (corpus-fixes era), i.e. in the window after those
two models first became reachable but before contact physics existed, so they have never
described the current engine.

> **CORRECTED.** The sentence that stood here — "the two models are exactly the two carrying
> PR-9 contact refusals (`make_toed_slab` 6, `make_raking_blade` 5)" — misnames the rule.
> **PR-9 fires zero times on either model.** Those 6 and 5 are PR-8/OD-9 *attestation*
> refusals in `initial.rs`, a different rule with a different disposition. The correct
> attribution is below.

Re-pinning is a digest-moving change and needed this census as its evidence, so it was left
for a deliberate pass (FU-19).

## Post-merge interaction delta — ATTRIBUTED and re-pinned (2026-08-05, late) {#post-merge-delta-2026-08-05}

This is the closing pass on the contact-physics merge. It discharges FU-18, FU-19 and
FU-20, corrects the misattribution above, and records the two process rules the episode
put in force.

### Why the package's zero-delta claim never covered these two models

`make_toed_slab` and `make_raking_blade` were UNREACHABLE on `codex/embed-zones` — blocked
upstream at specialized/perception — for the entire life of that branch. Every "MEASURED
CORPUS DELTA — ZERO" statement in `contact-physics-2026-08-04.md` was measured over the four
models that *were* reachable there. The **corpus defect fixes** unblocked these two, and
froze their seal tuples at `414e13f`, in a tree that had no contact physics in it. The two
efforts were never in the same tree until merge `81ec3d4`, so the interaction could not have
been measured before it. Nothing was missed; the intersection did not exist to measure.

### Attribution: one rule, four contacts

Under OD-8 a golden is not re-pinned until every missing member is attributed. Every unit of
the delta is **PR-8 `NotAttached`** quarantining four initial-contact stubs. `NotAttached`
fires only when nothing at all is engaged — centreline false AND 0 of 8 rim rays reaching
one process resolution (0.05 mm) contiguously from the tip end. All four measure exactly
that. An independent parity sweep along the contact normal at 0.0025 mm confirms the
witness rather than restating it (`t` measured inward from the seat; the tip sits at
`t = 0.30`):

| model | contact | routed load | solid along the normal | verdict |
| --- | --- | ---: | --- | --- |
| `make_toed_slab` | 68 | 0.000 mm² | none in `[-0.01, +0.39]`; tip 0.265025200 mm clear of the nearest surface | PR-8 `NotAttached` |
| `make_raking_blade` | 7 | 0.060 mm² | `[0.0025, 0.0300)` only — a 0.0275 mm skin; tip 0.151863225 mm clear beyond it | PR-8 `NotAttached` |
| `make_raking_blade` | 12 | 0.000 mm² | 0.0275 mm skin at the seat, then void; body resumes at `t = 0.3300`, 0.03 mm past the tip | PR-8 `NotAttached` |
| `make_raking_blade` | 13 | 0.000 mm² | 0.03 mm skin at the seat; body resumes at `t = 0.2750`, so the tip enters it by 0.025 mm | PR-8 `NotAttached` |

Combined routed demand deleted: **0.060 mm²**, three of the four carrying none at all.
PR-1(a)'s own argument governs the disposition: a fused region thinner than one layer is
not resolvable by the printer, so there is no interface for PR-4's area law to grade and
nothing to keep. Each refusal is inside PR-8's stated scope and each measured value
genuinely violates its bound.

### The other seven refusals contribute ZERO

Eleven attestation refusals fire across the two models; seven land on stubs the pre-existing
initial-overlap cutoff already condemned, so they move nothing:

- `make_toed_slab` **#22 #26 #31 #59 #62 — OD-9 `BuriedSurface`.** The probe one process
  resolution OUTWARD tests INSIDE, and the parity sweep is solid continuously from
  `t = -0.01` through `t = +0.39`. Their connectivity evidence is **perfect** (8/8 rim rays
  plus centreline, `r_eff` = the full 0.200 mm) — the exact trap OD-9 exists to catch, seen
  on real corpus geometry for the first time rather than on the synthetic fixture. All five
  also carry overlap 0.444907426 .. 2.500000000 mm against a 0.050000000 mm cutoff.
- `make_raking_blade` **#8 #21 — `NotAttached`**, overlap 2.118262048 and 2.432244414 mm.

### No other rule in the package fires here

Measured on both models, not assumed:

| rule | occurrences |
| --- | ---: |
| PR-3 incidence admissibility (`contact::admit` on rung candidates) | **0** — no rung candidate is generated at all |
| brace model-rung seat witness (F3) | **0** rows |
| PR-9 contact-clearance refusal / pipeline violation | **0** / **0** |
| CCO model anchor, incl. the PR-7 commit-time tripwire | **0** — `T2_cco_anchor_attempted = 0` |

### The control that closes the attribution

Suppressing **only** the attestation-refusal quarantine — leaving the attestation itself,
its digest, every other rule and every other stage untouched — reproduces both frozen tuples
**bit-for-bit, digests included**. That is what licenses the re-pin: the delta is that one
rule and nothing else, so no missing member is unaccounted for.

The member counts fall by more than the four stubs (-5 and -4 against -1 and -3 contacts)
because CCO re-solves its tournament over the smaller contact set: on `make_toed_slab`,
Stub 65→64, Shoot 61→55, Trunk 57→58, Brace 0→1. **Zero contacts are ADDED** on either
model, so the movement is a re-solve, not a substitution.

> **CORRECTED (2026-08-05):** two cells above are wrong. The true `make_toed_slab` per-kind
> movement is Stub 65→64, Shoot 62→55, Trunk 57→58, Brace 0→2 (sums: 184 → 179, matching the
> pinned tuple `64 / 179 / 4826202191472851835`). Commit `f77dfe1`'s commit message carries
> the superseded figures (Shoot 61→55, Brace 0→1); commit messages are immutable, so this
> note is the correction of record.

Re-pinned at `f77dfe1`; the attribution above is reproduced in that commit message and in
the test's own doc comment, so a future reader does not have to come here for it.

### Instrument note: the census and the seal test are NOT interchangeable

The census example reports `make_toed_slab` digest `11199296506741562956` while the seal
test computes `4826202191472851835` for the same `(64 routed, 179 live)`. **The cause is the
mesh source, not the layer plan** — the layer-plan hypothesis was tested and is false. Both
build the identical `LayerPlan::new(0.0, vec![0.05; 1000])` (the census's
`max(ceil(span/pitch), 1000)` evaluates to 1000 on both models and its `MODEL` row prints
`layers=1000`). The seal test uses the **procedural** `make_toed_slab()` mesh in f64; the
census loads **`corpus/models/make_toed_slab.stl`**, a binary STL, whose coordinates are
f32. Measured: identical topology and triangle order (140 and 24 triangles), 740 of 1260 and
144 of 216 coordinates bit-identical, max |Δ| **4.35e-7 mm** and **2.98e-7 mm**, and
`f64::from(procedural as f32) == stl` for **every** coordinate. Feeding the STL mesh through
the seal test's own code path reproduces the census digests exactly
(`11199296506741562956`, `13278215294653054002`), which isolates the mesh as the only
variable.

Sub-micron coordinate differences move a bit-exact geometry digest, and here they also move
a member count: on the pre-refusal baseline the STL-sourced run yields `make_toed_slab`
live=181 against the procedural 184. **So the two instruments agree on physics and disagree
on bits by construction.** Compare census-to-census and seal-test-to-seal-test; never read a
digest from one against the other and conclude a regression.

### Two process rules now in force

1. **`cargo fmt --all -- --check` is part of every gate run.** The four inherited violations
   (FU-18) reached the integration branch because the embed work's gate set was four
   commands, not five. The gate set is: `cargo test --workspace --offline --no-fail-fast`;
   the same with `--all-features`; `cargo clippy --workspace --all-targets --all-features
   --offline -- -D warnings`; `cargo fmt --all -- --check`; `git diff --check`.
2. **Tests always run with `--no-fail-fast`.** A bare `cargo test` stops at the first failing
   target and under-reports the total, which is how a single red integration test can be
   mistaken for a smaller or larger problem than it is. Every count quoted in this file is a
   `--no-fail-fast` count.

### Gate counts

The five-command gate set referenced above (**Gate GREEN**) as run at head `65c132a`:

| command | result |
| --- | --- |
| `cargo test --workspace --offline --no-fail-fast` | 940 passed / 0 failed |
| `cargo test --workspace --offline --no-fail-fast --all-features` | 941 passed / 0 failed |
| `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `git diff --check` | clean |

This correction commit is the new integration head as of this write.

### Task-tracker cross-reference: the `make_toed_slab` freeze is still PROVISIONAL

FU-7 (`make_toed_slab` parity investigation, vertex-probe vs interior-probe) is **not**
settled by this pass and the model's required-table freeze stays **PROVISIONAL** — now on
this new baseline. The seal tuple re-pinned here (`64 / 179 / 4826202191472851835`) will
move again when FU-7 lands, and that is expected, not a regression. Nothing in this pass
touched the containment probe.

Note the adjacency without conflating them: the PR-8 refusals attributed above were reached
through the *attestation* witness (`penetration_intervals_batch_bounded`), and the OD-9
screen through `inside`. Both agreed with the independent parity sweep at every one of the
eleven refused seats, including `make_raking_blade` #13 where the tip point tests inside
while the interface is only 0.025 mm deep. That is evidence about these seats, not a
resolution of FU-7, which is about a different probe on a different question.

### Area deficits: RECORDED as data, not adjudicated

**12 stub-terminal + 554 CCO model-anchor** contacts carry a contact-area deficit — attached
over measurably less interface than their tip radius claims. These are **recorded as data
for the print-quality follow-up** (FU-17) and nothing in this package acts on them. The
in-flight figure of "4 stub + 19 CCO" was `make_overhang_table` alone, which was the only
model reaching CCO when it was taken; the corpus figures are larger purely because
`Cylinder` and `make_box` now reach the CCO stage natively. No rule changed, the denominator
did. OQ-8 is the owner question these numbers feed.

### Known hygiene debt at this head — CLEARED

Four `rustfmt` violations inherited from the embed tip were in `plinth-support`
(`cco.rs:1425`, `cco.rs:2485`, `census.rs:852`, `designed_embed.rs:788`). They were
formatting-only and were not introduced by the 2026-08-05 census work.
**Fixed at `cf2c2ab` (`cargo fmt --all`), FU-18 resolved**; the diff is those four sites and
nothing else. See process rule 1 in *Post-merge interaction delta* for why they got through.

## Owner caps-landing rulings R1–R5 (2026-08-05, late) {#owner-caps-rulings-2026-08-05}

The support resource-cap landing (`3d91390`, `e3670ab`, `ef88ab0`) was reviewed **ACCEPT
with findings**. The following are **binding owner decisions of 2026-08-05**, delegated by
the owner with veto retained, adjudicating those findings. The measurement record they act
on is `docs/rust-migration/support-resource-caps-2026-08-05.md`, which carries the long
form of R1–R4 at the exact numbers they correct; this section is the index.

- **R1 — the undisclosed third scaled cap is PINNED BACK.** `SupportPreparationLimits::input`
  scaled **three** caps where the doc comment, the caps record and the M0 registry all
  declared two: `SpecializedLimits::coverage_route_knn.max_work` rode the same
  `2048 × faces` rule purely for sharing the `KnnBatchLimits` shape. That coefficient was
  measured on the **minima-to-face** batch alone; the coverage-route batch has no
  measurement of record, no accessor and no test. The scaling is **removed** — the cap stays
  at the flat `SUPPORT_KNN_MAX_WORK_FLOOR` (100,000,000) until someone measures that batch.
  Behaviorally inert on every model: the treant's instrumented coverage-route demand is
  **15,282,619**, 6.5× under the floor, and the treant stops at the specialized gate before
  coverage routing runs at all. Landed `2aec85a`, pinned across 0…10⁸ faces by
  `the_coverage_route_knn_cap_is_the_flat_floor_at_every_face_count`.
- **R2 — 512 B/face is RATIFIED.** The landed parity-grid coefficient is 512 where the
  proposal said 256. Ratified as written, **no code change**: the deviation buys **one**
  uniform `next_power_of_two(2 × observed)` rule for both coefficients, and the wider margin
  it leaves (4.2× admitted-over-observed refs/face against kNN's 1.7×) is earned by the
  corpus showing refs/face varying 526 ↔ 2.83. Recorded `354c27d`.
- **R3 — the census headline is CORRECTED.** The caps record claimed the before/after census
  "differs in exactly one line out of 3,399". The true diff is **9 lines**: 8 additive
  harness fields on the `MODEL` rows plus the treant's own stage-error row. What the
  measurement supports — **all 3,390 instrumented `CENSUS` body lines byte-identical, all 7
  reachable digests unmoved** — is the zero-behavioral-delta claim, and it survives intact.
  Written as a **visible dated addendum** below the original per this project's
  honest-addendum convention, not a silent rewrite. Recorded `354c27d`.
- **R4 — both scaled coefficients are MARKED n = 1.** The two Measured observations and the
  two Derived coefficients above them rest on a single specimen: **Weeping Treant v1.2,
  429,120 faces, 2026-08-05**, the only corpus model above 4,760 faces. All four M0 registry
  notes now say so and say the consequence: **a second large-model measurement triggers
  re-derivation**, never silent inheritance of a treant-shaped constant. The `Measured`
  classification of the refs-per-face observation **stands** — the inversion through
  `axis_memory_bounds` is disclosed in its own note. Notes changed at the producer
  (`constants/evidence.rs`) and `constants.md` regenerated; the generated file was never
  hand-edited. Recorded `354c27d`.
- **R5 — the orphan execution caps are REGISTERED.** The support pipeline's remaining
  execution authority had **no registry entry at all**. Every cap in the four named
  authority types is now registered — **42** of them, not the ruling's estimated ~29, the
  count reported as enumerated rather than trimmed to the estimate — all
  `runtime_resource_cap`, all wrapped `Placeholder`, each with `source_locators` and a
  `provenance_detail` naming the Codex-era commit that introduced it (`73e7c13` / `a8acfb9`
  / `c2b3b98`, no measurement of record). **Registration is disclosure, not ratification**,
  and the `Placeholder` wrapper is what keeps them off any accept path. The four gate/load
  caps measured to be refusing algorithm defects carry that prohibition in their own notes:
  they must **NOT** be raised (see FU-8b). Nested authorities under
  `SupportPreparationLimits::production` — `CandidatePayloadConfig`,
  `ContactConstructionLimits`, `PenetrationBatchLimits`, `RaftLimits` — are explicitly **out
  of scope** and need their own pass; the boundary is stated in
  `03-physics-and-constants.md` §3.10.9. Landed `e6994f9`; registry moved **218 → 260**
  constants, classes `[18 Measured, 6 Derived, 194 Placeholder, 0 Pending]` →
  `[18, 6, 236, 0]`, `constants.md` regenerated via the producer example.

**OD-8 delta discipline.** The expected behavioral delta across all five rulings is
**ZERO**, and it was measured, not assumed: R1 removes a scaling no model reaches, R2 is a
no-op, and R3/R4/R5 move note text and registry membership, which are not accept-path
inputs. See *Caps-rulings OD-8 delta* immediately below.

### Caps-rulings OD-8 delta — MEASURED ZERO {#caps-rulings-od8-delta-2026-08-05}

Re-run at the rulings tip over all 8 corpus models:

```
PLINTH_CENSUS=1 cargo run --release --offline --features census \
  --example embed_incidence_census -p plinth-support
```

The transcript is **3,399 lines**, the same length as the caps-landing run, and **all seven
reachable digests are byte-identical to the values recorded in
`support-resource-caps-2026-08-05.md` §4**:

| Model | faces | `snapshot_digest` at rulings tip | vs recorded |
|---|---|---|---|
| `Sphere` | 4,760 | `17667468971446251870` | unmoved |
| `Cylinder` | 244 | `13310185350867878190` | unmoved |
| `make_box` | 12 | `9657051231007502658` | unmoved |
| `make_overhang_table` | 60 | `6686599816688114065` | unmoved |
| `make_toed_slab` | 140 | `11199296506741562956` | unmoved |
| `make_raking_blade` | 24 | `13278215294653054002` | unmoved |
| `make_thin_fin` | 24 | `7016743535301716631` | unmoved |

The treant still refuses at the same place, with the same typed error:

```
MODEL  Weeping Treant v1.2  UNREACHABLE  faces=429120
       max_grid_bytes=219709440  knn_max_work=878837760
       support: sealed support run failed during Preparation
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 2000000001, limit: 2000000000 })
```

Note that `knn_max_work=878837760` on that row is the **minima** batch, which R1 did not
touch and which stays scaled. R1 removed the scaling on the *coverage-route* batch, which
is not printed and which the treant never reaches. **Behavioral delta: zero, measured.**

**The R-numbered ledger continues.** Owner ruling **R6** (2026-08-06) — the decimation-first
premise is falsified, FU-8b runs first, decimation is demoted — is recorded in *FU-8b
resolved; the treant's stopping point moved*, immediately below.

## FU-8b resolved; the treant's stopping point moved (2026-08-06) {#fu8b-resolution-2026-08-06}

**Dated addendum, not a rewrite.** Everything above stands as the record of what was
believed when it was written. This section states what the FU-8b fixes actually measured,
what cap policy that measurement bought, and where the **Weeping Treant v1.2** stops now.
Branch `feature/fu8b-gate-load-quadratics`, tip `d82c895`.

### What was quadratic — three defects, one shape

Each is a per-item rescan of a whole batch standing in for one batched evaluation.

- **(A) The distinct-face gathers were selection sorts.** `unique_island_face_count` and
  `visit_served_owner_faces` (`crates/plinth-support/src/specialized.rs`) found distinct
  face ids by repeated-minimum selection sort and charged one work unit per inner
  comparison — a predicted **16,795,425,332** charged operations on the treant. That is why
  the pre-fix refusal read `required: 2000000001, limit: 2000000000`: `limit + 1` is the
  charge counter tripping mid-sort, **not a demand**. The gathers now collect each island
  once and `sort_unstable` + `dedup` in one batch, preserving both outputs exactly — the
  count and the ascending distinct visit order. Landed `0bdec0c`.
- **(B) Load eligibility was sized by owner-face pair, not by face.** `build_load_eligibility`
  produced **7,988,078** rows for 429,120 faces — 18.6 per face — because the `face_id` path
  deduped through a `BTreeMap` while ~7.56 M `specialized_owner_faces` **pairs** were
  appended ungrouped. The pairs are now grouped by face once (`GroupedOwnerFaces`) and the
  pair count and the distinct-face count are stated separately (`SpecializedLoadCounts`).
  Rows collapsed **59.5×**, to the 134,368 distinct faces they always were. Landed `1706c5d`.
- **(C) The extended-eligibility builder rescanned per row and per pair, uncharged.**
  `finalize_specialized_load_partition` (`crates/plinth-support/src/perception.rs`) ran, per
  eligibility row, two full linear scans of `specialized_owner_faces` plus a per-pair
  `.any()` over `load_eligibility` — on the order of **6 × 10¹³** scalar comparisons,
  single-threaded and charged by no cap at all. This was the 16-hour hang, with 4,127 of
  4,127 stack samples in that one frame. It now joins base rows to the existing grouping in
  one linear merge, proved against a verbatim copy of the replaced algorithm over a 4 × 5
  cross product of shapes. Landed `e0a4bed`.

### What the fixes measured

Every row is an observation at the stated tip, not a projection. Caps in the "cap" column
are the bound in force at the time of the observation.

| quantity | pre-fix | post-fix | cap | verdict |
|---|---|---|---|---|
| gate ops, proposal-free batch | 2,000,000,001 (counter tripping at `limit + 1`, not a demand) | 12,714,828,544 | 2,000,000,000 flat → 35,445,500,736 materialized | over the flat cap with an **honest linear demand** → cap rule set; now **admitted**, 2.79× headroom |
| load ops | ~2.95 × 10¹¹ predicted over 7,988,078 rows | 5,402,535,619 over 134,368 rows | 2,000,000,000 flat → 11,421,470,912 materialized | over the flat cap with an **honest demand** → cap rule set; now **admitted**, 2.1× headroom |
| load eligibility rows | 7,988,078 (18.6/face) | 134,368 (distinct faces) | n/a | 59.5× collapse from fix (B) |
| load scratch bytes | over the 512 MB cap | 144,753,256 | 536,870,912 | **under** cap — no cap change made |
| gate peak bytes | unmeasured | 215,061,920 | 1,073,741,824 | **under** cap — no cap change made |
| gate ops, 39,223-proposal MERGE batch | never reached (the 16-hour hang was defect (C)) | 21,755,124,103,146 | 35,445,500,736 materialized | **still refused**, 613× over — superlinear in the batch, opened as **FU-8c** |
| `make_raking_blade` gate ops (24 faces) | 612,669,596 | 612,887,212 | 2,000,000,000 (floor governs) | proof that gate work does **not** track face count: 20× spread against an 18,000× face spread |
| `Cylinder` gate batch product (widest corpus) | n/a | 27,998,135 → ×64 = 1,791,880,640 | 2,000,000,000 floor | the near case, at **89.6 %** of the floor and bit-identical |
| peak RSS, treant run | — | 2.84 GB | 90 GB hard kill / ~100 GB ceiling | far under; no memory finding |
| wall clock, treant to refusal | ~16 h (hang) | 81.5 s | 600,000 ms timebox | inside the timebox |

*Scope note added 2026-08-06 (record fix F7).* Every row of the table above is an
observation at tip `d82c895`, as this section's header already states. Making the
supersession explicit: the **gate peak bytes** row is superseded at `23d929d`, which made
the gate's shared distinct-face buffer a term in that bound. See the F7 addendum below.

*Scope note added 2026-08-06 (FU-8c landed at `a3308ec`).* In the same spirit, three more
rows of the table above are superseded, and the table is left standing as the `d82c895`
observation it is. The **gate ops, 39,223-proposal MERGE batch** row's 21,755,124,103,146 is
now **201,480,535,329** against the same unchanged 35,445,500,736 — 108× less, and **still
refused**, 5.68× over rather than 613×; the verdict column is unchanged in kind and the
successor defect is **FU-8d**. The **peak RSS** row's 2.84 GB is now **2.50 GB** and the
**wall clock** row's 81.5 s is now **124 s**, both for the merge-gate run and both still far
inside the 90 GB kill line and the 600,000 ms timebox. Every other row, both operation caps,
both coefficients and all seven frozen digests are unmoved. See *FU-8c fixed 2026-08-06*.

> **CORRECTED 2026-08-06 (record fixes F1, F2, F3) — dated addendum; the bullet (B) and the
> three load rows above are left standing as written.** A policy review of this section
> found one false claim about the code and one mismatched pair of numbers. Both are stated
> here; neither changes a cap, a refusal or a digest.
>
> **F3 — the treant never *built* 7,988,078 load-eligibility rows.** Bullet (B) above, and
> the "load eligibility rows" table row, say `build_load_eligibility` produced them "because
> the `face_id` path deduped through a `BTreeMap` while ~7.56 M `specialized_owner_faces`
> **pairs** were appended ungrouped." There were no such appends. At `eb36358` that function
> was already a pure `BTreeMap<u32, BTreeSet<LoadOwner>>` collapse keyed on `face_id` —
> **one row per distinct face, always**. **7,988,078 was a *predicted* row count**, computed
> in `specialized_load_resource_plan` as `plan.load_eligibility.len() +
> specialized_face_count` with the pair count standing in for the face count: 16,627 +
> 7,971,451. Fix (B) corrected the *prediction*; the row set it describes never existed and
> so never collapsed. The (B) implementer said exactly this at the time — "the defect was in
> the prediction, not the data" — and the docs phase dropped it. The fix, the commit and the
> caps' verdict are unaffected: an inflated plan is precisely what a resource plan exists to
> catch.
>
> **F1/F2 — the load-ops headline pairs a real demand with the wrong batch.** "5,402,535,619
> over 134,368 rows … 2.1× headroom" divides a demand measured on one batch by a cap
> materialized on another. The landed census (below, *Treant outcome at `d82c895`*) prints
> `rows=130548`, and one scratch-instrumented treant run printed `resource.rows = 130548`
> beside `resource.operations = 5402535619` at `finalize_specialized_load_partition`. So:
>
> | quantity | corrected | note |
> |---|---|---|
> | load ops | **5,402,535,619 over 130,548 rows** × 1,366 contacts | demand unchanged; the row count was wrong |
> | load rows, composition | 16,627 generic + 113,921 distinct pair faces = **130,548** | from 7,971,451 `(owner, face)` pairs |
> | materialized load cap | 11,421,470,912 = 64 × 130,549 × 1,367 | unchanged — it was always computed from 130,548 |
> | headroom | **2.11×** | now a ratio of two numbers over the same batch |
> | predicted-row collapse from fix (B) | 7,988,078 → 130,548, **61.2×** | not 59.5×, and it is a prediction that collapsed |
>
> **No 134,368-row load batch is planned anywhere in the corpus.** Had one been, it would
> have materialized 64 × 134,369 × 1,367 = **11,755,675,072** and taken the census's
> widest-per-kind slot from 11,421,470,912, because `record_scaled_cap` keeps only the widest
> materialization per kind. It does not appear.
>
> **What moved, and what did not.** `SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED` is restated
> from 30 to 31 (`5,402,535,619 / 178,460,483 = 30.28`, ceiling).
> `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` is **unchanged at 64**, because
> `next_power_of_two` rounds 60 and 62 alike — so no cap materializes differently, every
> corpus model stays at its floor, the seven frozen census digests stay byte-identical and
> the treant's refusal line is unchanged. See
> [`support-resource-caps-2026-08-05.md`](support-resource-caps-2026-08-05.md) §2, §7.1,
> §7.2 and §7.4 for the same corrections against that document's own wording.

> **CORRECTED 2026-08-06 (record fix F7) — dated addendum; the "gate peak bytes" row above
> is left standing as the observation at `d82c895`.** That row states the treant's measured
> gate peak as **215,061,920 B**, which was true of the bound as it stood at `d82c895` and
> is **no longer the peak this branch measures**. Record fix F6 (`23d929d`) made the gate
> batch's shared distinct-face `Vec<u32>` — allocated by FU-8b fix (A), reused by every
> gather in the batch, and until then held but never charged — a term of `GatePeakUsage`.
>
> | quantity | at `d82c895` | at `23d929d` (governs) |
> |---|---|---|
> | treant gate peak bytes | 215,061,920 | **216,203,088** = 215,061,920 + 1,141,168 face-scratch |
> | face-scratch term | not charged | 570,584 B live face ids, charged as 1,141,168 B (doubled for `Vec` growth slack) |
> | cap | 1,073,741,824 | 1,073,741,824, **unchanged** |
> | verdict | under cap | still **under** cap — 20.1 % of the bound; the term is 0.53 % of the peak and 0.11 % of the cap |
>
> **Nothing else moved.** The census was re-run at `23d929d`: the seven frozen OD-8 digests
> stayed byte-identical, the treant's refusal line stayed
> `Preparation: Specialized(GateOperationLimit { required: 21755124103146, limit: 35445500736 })`,
> and every corpus scaled-cap row stayed `at_floor=true`. The term is charged because the
> gate must bound what it holds, not because anything was near a limit.
>
> **Note added 2026-08-06 (FU-8c landed):** the refusal line quoted above is the one that
> held *at `23d929d`*, and it is left standing as the record of that re-run. It moved when
> FU-8c landed — to `required: 201480535329, limit: 35445500736` — and the seven digests
> stayed byte-identical across that landing too. See *FU-8c fixed 2026-08-06*.

> **ADDED 2026-08-06 (record fixes F8–F11) — dated addendum; nothing above is rewritten.**
> A second independent re-verification of this section's landing found four more defects
> beyond F7. None changes a cap, a coefficient, a refusal or a digest.
>
> - **F8 — the F6 doc comment claimed something false about the code.** It said the
>   face-scratch term "is priced before any gather runs". It is not: all three production
>   sites fill the buffer via `unique_island_face_count` *before* reaching their byte
>   check. The honest claim is the one now written — the term is priced from the islands'
>   `face_ids` rather than from the buffer's capacity, so it over-states rather than
>   under-states — and the ordering claim is dropped.
> - **F9 — F6 fixed only two of the three uncharged buffers.** FU-8b fix (A) created a
>   third shared distinct-face `Vec<u32>` in `working_load_inputs`, the accepted-load path,
>   and it was charged against **no** byte bound: that check priced only the retained
>   contacts and pairs plus the finalizer's scratch against `max_load_scratch_bytes`. It is
>   now a term there too, on the same honest pricing. **Measured, then proved:** each of the
>   seven reachable corpus models enters that function exactly once, and the widest
>   requirement is Sphere's **78,408 B** (new term 4,592 B) against the unchanged
>   **536,870,912 B** cap — 0.015 % of it; the other six sit between 1,344 B and 11,680 B.
>   The **Weeping Treant never reaches this function**, refusing at the merge gate first, so
>   the **144,753,256 B** "load scratch bytes" row above is *not* this check's — it is
>   `finalize_specialized_load_partition`'s, which F9 does not touch — and is **unchanged**.
> - **F10 — a re-attested clause named the wrong model.** `a1b0a55` restated
>   `SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED`'s single-specimen note as "the next largest
>   corpus model plans 66 rows". 66 is the *Cylinder's* load batch; the next largest corpus
>   model is **Sphere** (4,760 faces), which plans **574** rows. Corrected in `evidence.rs`,
>   the ledger and the regenerated `constants.md`. The **n = 1 conclusion stands** — 574 is
>   still 227× below the treant's 130,548, so the corpus offers no second specimen at this
>   batch scale.
> - **F11 — an observation, documented rather than changed.** `CORPUS_LOAD_BATCHES` said its
>   shapes were "from the same run", which reads as "the census row"; three of seven differ
>   from the census. Instrumenting `record_scaled_cap` over the whole census showed why:
>   every model records several load checkpoints, all materializing **at the floor**, and on
>   that tie `record_scaled_cap` keeps the **first** rather than the widest. So the census
>   prints the first checkpoint and the table pins the widest — Cylinder 66 vs 61,
>   `make_overhang_table` 12 vs 10. Both are now documented at both ends. **One row is not
>   explained by the tie:** `make_thin_fin` pins 6 rows where the census and all five of its
>   observed checkpoints plan 3. It is conservative for what the table asserts and nothing
>   downstream reads it, so it is left standing and recorded as **unreconciled** rather than
>   silently re-pinned.

### The cap policy that measurement bought

The two operation caps were the deferred half of FU-8b, and they were set on **post-fix**
data only. Both are now batch-product rules:

```text
max_gate_operations = max(2_000_000_000, 64 x (coverage_samples + 1) x (contacts + 1))
max_load_operations = max(2_000_000_000, 64 x (rows + 1)             x (contacts + 1))
```

The scale is the **batch**, not the mesh, and that choice is measured rather than assumed:
24-face `make_raking_blade` charges 612,887,212 gate operations against the 429,120-face
treant's 12,714,828,544. Scaling on faces would refuse the small model and admit anything on
the large one — the exact failure mode a cap exists to prevent. Coefficients registered
`44dc8ec` (raw observations `Measured`, coefficients `Derived` under the stated
`next_power_of_two(2 x observed)` rule, floors `Placeholder`-inherited, n = 1 markers per R4
and FU-23); the bounds themselves landed `d82c895`. `max_gate_operations` and
`max_load_operations` were renamed to `..._operations_floor` because that is what they now
are.

Per the R1 lesson, **neither bound is a silent rider**: each has a doc comment naming it, an
accessor, a pinning test in `crates/plinth-support/tests/resource_cap_policy.rs`, a
census-printed materialized value, and a row in
[`support-resource-caps-2026-08-05.md`](support-resource-caps-2026-08-05.md) §7 and
`03-physics-and-constants.md` §3.10.10.

**Corpus cap identity.** All 14 corpus materializations (7 reachable models × 2 caps) print
`at_floor=true` in the census and are asserted bit-identical by
`the_batch_scaled_caps_are_the_flat_floor_for_every_corpus_batch_shape`. The floors are
unchanged at 2,000,000,000 and remain the whole cap for every corpus model.

### Treant outcome at `d82c895` — STOPPED, with a stated demand {#treant-stop-2026-08-06}

Run isolated to the treant, RSS polled every 2 s with a hard kill armed at 90 GB, threads
pinned to 4:

```sh
PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2" PLINTH_THREADS=4 \
  cargo run --release --offline --features census \
    --example embed_incidence_census -p plinth-support
```

```
MODEL  Weeping Treant v1.2  UNREACHABLE  faces=429120
       max_grid_bytes=219709440  knn_max_work=878837760
       support: sealed support run failed during Preparation
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_gate_operations
       floor=2000000000 per_batch_unit=64 rows=405146 columns=1366
       materialized=35445500736 at_floor=false
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_load_operations
       floor=2000000000 per_batch_unit=64 rows=130548 columns=1366
       materialized=11421470912 at_floor=false
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 21755124103146, limit: 35445500736 })
```

**This is progress, and it is bounded progress.** The treant now clears perception, the
candidate-surface grid, the specialized gate's proposal-free batch and the **entire load
stage**, and refuses honestly at the *merge* gate on a demand it states. It is **not**
sealed: there is still no treant census digest, no routed-contact count and no live-member
count, and this record does not invent one.

- **Wall clock:** 127 s to refusal at `PLINTH_THREADS=4`; 81.5 s unpinned in the FU-8b
  measurement pass. Both far inside the 600,000 ms timebox — this is a refusal, not an
  EXCEEDS-TIMEBOX.
- **Peak RSS:** 2,669,776 KB = **2.55 GB**, 35× under the 90 GB kill line. The watchdog
  never fired.
- **Memory hazard not reached.** The `pen_intervals_one` eager reservation
  (`rust/geom_kernel/src/queries.rs`, predicted 14–32 GB on the treant against a
  `brace_penetration_limits.max_scratch_bytes` of 1 × 10¹¹) is downstream of the gate. The
  treant never enters a penetration or brace stage, so that hazard remains **unmeasured**,
  not cleared.

**The refusal derives exactly.** `gate_resource_plan`
(`crates/plinth-support/src/specialized.rs`) prices a gate batch at

```text
operations = (samples + demands + 1) x (proposals x (contacts - 1) + contacts)
           + (proposals + 1) x unique_faces
```

and the treant's merge batch carries samples = 405,146, demands = 1,095, contacts = 1,366,
proposals = **39,223**, unique_faces = 117,741:

```text
contact_visits = 39,223 x 1,365 + 1,366            =         53,540,761
routed_rows    = 406,242 x 53,540,761              = 21,750,505,830,162
face_rows      = 39,224 x 117,741                  =      4,618,272,984
operations                                         = 21,755,124,103,146   <- the printed demand
```

Strike the `proposals` factor out of `contact_visits` and the same expression costs
406,242 × 1,366 = 554,926,572 — **39,209× less**, and comfortably inside the materialized
cap. The entire refusal is that one factor: the merge gate re-evaluates every contact
against every coverage sample once per merge proposal. That is FU-8c, and it is the same
batch-first violation as (A)/(B)/(C) in a fourth dress.

**It is not a cap to raise.** Admitting it needs a per-batch-unit coefficient near 39,300,
which multiplies *every* corpus model's cap by the same factor and takes the Cylinder
(already at 89.6 % of the floor), `make_raking_blade` and others off their floors — i.e. it
would admit real quadratics on small models to accommodate one large one.

### FU-8c fixed 2026-08-06 — the proposal factor is gone; the treant's demand fell 108× and still refuses {#fu8c-fixed-2026-08-06}

> **Dated addendum. Everything above this heading is left standing as the finding it was.**
> The formula block above described the code as it stood at `d82c895`; it no longer
> describes the code, and the paragraphs below state what replaced it. Nothing above is
> rewritten.

**What changed.** A merge proposal replaces two contacts with one merged seat and leaves
every other contact's position and routes verbatim. Two locality facts follow:

- a coverage sample's nearest distance is a minimum over the contacts serving **its own**
  route key, so a proposal can only move rows whose key its own pair serves. For every
  other key the variant's source set is bit-identical to the baseline's — same members,
  same gather order — so the baseline's answer already *is* the variant's answer;
- a demand is satisfied by **some** contact, so removing two can only unseat it if one of
  them was a witness, and three retained witnesses per demand answer that without a scan.

`GateBatchIndex` (`crates/plinth-support/src/specialized.rs`) builds that
proposal-invariant state once per batch. Each proposal then re-answers only its own route
keys and re-tests each demand once against the merged seat. `gate_resource_plan` is
repriced to describe the new work:

```text
routed_rows = (samples + demands + 1) x contacts + measured proposal work
```

**No cap was changed.** The 35,445,500,736 the treant is measured against is the same
`gate_operation_limit(405,146, 1,366)` it was measured against before, and no cap code is
touched by the fix.

**Equivalence was proved before the hot path was swapped.** The predecessor's per-variant
sweep is retained as a `#[cfg(test)]` reference and compared with the delta path row by row
using `f64::to_bits` on a seven-proposal fixture
(`fu8c_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit`), which also asserts
that every row the reference moves was touched, so a right answer cannot be reached by
leaving a row alone. `fu8c_witness_demand_counts_reproduce_the_full_sweep` does the same
for the demand counts, variant 0 included.

**The treant's new stopping point, measured 2026-08-06 at `a3308ec`, reported exactly:**

```
MODEL  Weeping Treant v1.2  UNREACHABLE  faces=429120
       max_grid_bytes=219709440  knn_max_work=878837760
       support: sealed support run failed during Preparation
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_gate_operations
       floor=2000000000 per_batch_unit=64 rows=405146 columns=1366
       materialized=35445500736 at_floor=false
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_load_operations
       floor=2000000000 per_batch_unit=64 rows=130548 columns=1366
       materialized=11421470912 at_floor=false
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 201480535329, limit: 35445500736 })
```

- 129 s wall clock at `PLINTH_THREADS=4` (127 s before the fix), peak RSS **2,650,256 KB =
  2.53 GB**, 35× under the 90 GB kill line. The watchdog never fired. This is a refusal,
  not an EXCEEDS-TIMEBOX.
- **It still refuses, at the same gate, and this record does not dress that up.** The
  demand fell from 21,755,124,103,146 to 201,480,535,329 — **108× less** — but the cap is
  35,445,500,736, so it is still **5.68× over**. There is still no treant census digest, no
  routed-contact count and no live-member count.

**Where the residual is, derived from the printed demand.** The four terms of the repriced
plan on the treant's shape:

```text
baseline_rows = 406,242 x 1,366                    =         554,926,572
face_rows     = 39,224 x 117,741                   =       4,618,272,984
demand_rows   = 39,223 x 1,095                     =          42,949,185
residual (per-proposal route-local nearest work)   =     196,264,386,588
                                                     -----------------
operations                                         =     201,480,535,329   <- the printed demand
```

The residual is **97.4 % of the demand** and averages 5,003,809 units per proposal. Route
locality is real and bought 108×, but the treant's coverage samples are concentrated in a
few very large route keys and its merge proposals sit on them: a perturbed key carrying
~10² sources and ~10⁴–10⁵ samples costs ~5 × 10⁶ on its own.

The refusal is `gate_resource_plan`'s, not the exact-operations fold's, and that is
arithmetic rather than assertion: the fold's `unique_face_finalizer_work` term alone is at
least `face_count x physical_contacts x proposals` = 117,741 × 1,365 × 39,223 ≈ 6.3 × 10¹²,
so a fold that had run to completion could not have produced 2.0 × 10¹¹. It also means
**that term is itself superlinear and the plan under-charges it** — the plan charges
`variants x faces`, the fold charges `variants x faces x contacts`.

**Opened as FU-8d, two parts, both measured above and neither speculative:**

1. **Per-proposal nearest is still `sources x samples` on the perturbed key.** It need not
   be. Retaining the *three* nearest sources per sample in the baseline sweep answers
   "nearest among the survivors" for any two-contact removal by inspection, leaving one
   distance test per touched sample against the merged seat: `sources x samples` becomes
   `samples` on the perturbed key. That is the same top-`k` argument FU-8c used for the
   demand witnesses, applied to the nearest.
2. **`unique_face_finalizer_work` is `variants x faces x contacts` and the plan charges it
   as `variants x faces`.** The under-charge is why it has never been the refusing term;
   the superlinearity is why it will be once (1) lands.

The per-variant `assess_coverage` call is a third candidate — it is `O(samples)` per
variant and is bounded by its own separate `max_coverage_operations` cap rather than by the
gate plan — but the treant has never reached it, so it is recorded here as **unmeasured**,
not as a finding.

> **Third part added to FU-8d 2026-08-06 — dated addendum; parts (1) and (2) above are left
> standing exactly as opened.** Deliberately **not coded now**: it is recorded so FU-8d is
> scoped honestly, and it is the cheapest of the three. (The paragraph immediately above
> calls the per-variant `assess_coverage` "a third candidate"; that sentence stands, and it
> is now the **fourth**, still recorded as unmeasured rather than as a finding. The ledger
> row numbers them that way.)
>
> 3. **The per-proposal planning passes are themselves uncharged, and there are two of
>    them.** `evaluate_gate_batch` calls `plan_proposal_nearest_delta` once per proposal
>    inside `plan_gate_nearest_batch` (`specialized.rs:3553`, to build the plan) and then
>    **again** once per proposal inside the `exact_operations` fold
>    (`specialized.rs:3571`, to charge it). On the treant that is **2 × 39,223 ≈ 78,446
>    delta passes**, and the cap check that refuses the batch —
>    `if exact_operations > gate_operation_limit` — is at `specialized.rs:3629`, **after
>    both**. So the whole cost of deciding that the treant may not proceed is paid before
>    the refusal, and none of it is charged against `gate_operation_limit`: what the plan
>    charges is the delta's *result* (`nearest_work`), never the cost of computing the
>    delta. That is why the treant burns 124 s to state a demand it is then refused on, and
>    it is a hole in the cap's meaning as much as a cost — a resource plan that cannot be
>    afforded until after it has been executed is not bounding anything.
>
>    The second pass is a **free 2×** and should be reclaimed as part of FU-8d: the fold
>    recomputes a delta the planning pass already computed for the same proposal, so
>    retaining the planning pass's per-proposal deltas turns two passes into one with no
>    change of result. Do it with (1) rather than before it, because (1) rewrites what a
>    delta pass *is* — `sources × samples` on the perturbed key becomes `samples` — and
>    halving the count of a pass that is about to change shape optimizes the wrong thing.
>    The corollary matters for FU-8d's design and is the reason this is a part rather than a
>    footnote: after (1) and (3) the planning work becomes small enough to charge, so
>    **FU-8d should charge it** instead of leaving the gate able to spend unbounded effort
>    deciding it cannot afford to proceed.

**Independent re-measurement 2026-08-06 at `8695398`.** The records tip — `a3308ec` plus
documentation and thirteen lines of doc comment in `specialized.rs` — was re-run from a
freshly rebuilt `--release --features census` binary, same command, `PLINTH_THREADS=4`, RSS
watchdog armed at 90 GB and polling every 2 s:

```
MODEL  Weeping Treant v1.2  UNREACHABLE  faces=429120
       max_grid_bytes=219709440  knn_max_work=878837760
       support: sealed support run failed during Preparation
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_gate_operations
       floor=2000000000 per_batch_unit=64 rows=405146 columns=1366
       materialized=35445500736 at_floor=false
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_load_operations
       floor=2000000000 per_batch_unit=64 rows=130548 columns=1366
       materialized=11421470912 at_floor=false
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 201480535329, limit: 35445500736 })
```

Byte-identical to the `a3308ec` transcript above, the stated demand included:
**201,480,535,329** against **35,445,500,736**, still **5.68× over**, refusing at the same
gate for the same reason. 124 s wall clock, peak RSS **2,618,464 KB = 2.50 GB** at t+122 s
over 61 samples; the process exited on its own refusal and the watchdog never fired. The
`a3308ec` figures of 129 s and 2,650,256 KB **stand as measured** — 124 s and 2,618,464 KB
are a *second* measurement, not a correction of the first. Wall clock varies run to run, and
a 2 s poller's maximum is a lower bound on the true peak, so two RSS figures 1.2 % apart are
the same measurement to the precision either sampler has.

**What the treant did not reach — stated because it is what arming the watchdog was for.**
The refusal is raised in Preparation, so the merge tournament and commit, the PENETRATION and
BRACE stages, the CCO anchors, attestation and the seal **did not execute**. The two known
memory hazards that a first-ever large-model run would have exercised therefore remain
**unexercised and unmeasured**: `pen_intervals_one`'s eager `24 × (T + 1) × cap` reservation
per ray per thread (`rust/geom_kernel/src/queries.rs`), and
`brace_penetration_limits.max_scratch_bytes = 100,000,000,000` (`runner.rs`), which sits at
the owner's 100 GB ceiling. Nothing in this run bears on either; at a 2.50 GB peak it never
came within 36× of the 90 GB kill line, and no reduction of that scratch cap can be proposed
on evidence this run does not contain. **There is still no treant census digest, no routed
contact count, and no live member count** — those numbers are not born here.

#### Does any reachable model exercise the proposal-delta path? — MEASURED, and one does {#fu8c-proposal-coverage-2026-08-06}

> **Dated addendum 2026-08-06. Nothing above is rewritten; this answers a question the
> record could not previously answer with data.**

The equivalence proof for the per-proposal delta path is a seven-proposal fixture, and the
treant — the only model known to offer proposals at scale — **refuses before the gate
completes**, so it contributes a demand transcript and no digest. That left an obvious hole:
if all seven *reachable* corpus models offer **zero** merge proposals, then their seven
byte-identical digests are evidence of nothing whatever about the delta path, because a
model with no proposals never enters the loop. Nothing counted proposals, so the question
was open in both directions.

**Harness change, disclosed as additive.** `census.rs` gained two counters
(`GateProposalCensus`: `batches`, `proposals`), recorded at the top of `evaluate_gate_batch`
beside the existing scaled-cap row and before any charge site can refuse, and printed as two
additive cells — `gate_batches=`, `merge_proposals=` — on the `T0_scaled_cap` row of kind
`specialized_gate_operations`. It is `census`-feature-gated and result-inert like the rest of
that module: no production code reads it, no existing cell moved, and **no census line was
added or removed** — the transcript is still 3,415 lines. The counters are **totals over
every gate batch a model runs**, not the retained widest batch that `rows=`/`columns=` come
from, because the delta path runs once per proposal per batch; `gate_batches=` rides along so
`merge_proposals=0` can be told apart from a model that never reached the gate at all (which
records no gate row).

**Full 8-model census at the O1 tip, 3,415 lines, all seven reachable digests byte-identical
to the frozen OD-8 values and the treant's stage-error line byte-identical to `8695398`:**

| Model | `gate_batches` | `merge_proposals` | outcome |
|---|---:|---:|---|
| Sphere | 2 | **0** | OK, digest 17667468971446251870 |
| Cylinder | 2 | **0** | OK, digest 13310185350867878190 |
| `make_box` | 2 | **0** | OK, digest 9657051231007502658 |
| `make_overhang_table` | 2 | **0** | OK, digest 6686599816688114065 |
| `make_toed_slab` | 2 | **0** | OK, digest 11199296506741562956 |
| `make_raking_blade` | **23** | **1,402** | OK, digest 13278215294653054002 |
| `make_thin_fin` | 2 | **0** | OK, digest 7016743535301716631 |
| Weeping Treant v1.2 | 2 | **39,223** | REFUSED, `required: 201480535329, limit: 35445500736` |

**The answer is `make_raking_blade`.** It runs a real merge tournament — 23 gate batches,
**1,402 merge proposals** — so it drives `plan_proposal_nearest_delta` on real corpus
geometry, 1,402 times per planning pass, and its sealed digest **13278215294653054002 is
byte-identical across the FU-8c landing**. That is a second and independent line of evidence
for the delta path beyond the seven-proposal fixture, and an end-to-end one: a
digest-preserving pass through a whole merge tournament rather than a unit comparison.

**The limits of it, stated because they are real.** Six of the seven reachable models
(Sphere, Cylinder, `make_box`, `make_overhang_table`, `make_toed_slab`, `make_thin_fin`)
offer **zero** proposals and never enter the loop, so their unchanged digests say nothing
about this path and must not be cited as though they did — of the reachable seven, only
`make_raking_blade`'s does. `make_raking_blade` is also 24 faces against the treant's
429,120, and its 1,402 proposals are 28× fewer than the treant's 39,223, so it exercises the
path's **correctness** at small scale and says nothing about its **cost** at large scale.
Cost is what FU-8d is for, and the treant's demand transcript is still the only measurement
of it.

#### FU-8d phase F1 landed 2026-08-06 — witness retention, and nothing else moved {#fu8d-f1-2026-08-06}

> **Dated addendum. Nothing above is rewritten.** FU-8d parts (1), (2) and (3) stay open as
> written; this is the first phase of (1), and it deliberately buys no speed at all.

The merge gate's **baseline** coverage sweep now asks the kernel for
`k = min(NEAREST_WITNESS_SLOTS, source_count)` neighbours per sample instead of one, and
keeps them — `(source_slot, squared_distance)` in the kernel's own emission order, one
fixed-width row per coverage sample, `NEAREST_WITNESS_SLOTS = 3`. **Nothing reads them yet.**
Phase F2's per-proposal delta will, and that is what turns `sources × samples` on a perturbed
route key into one distance test against the merged seat. F1 exists to put the witnesses in
place under proof, so the phase that *changes* answers arrives with the retention already
pinned.

**Two claims were pinned first, before the retention was written:**

1. **`fu8d_knn_row_zero_is_k_independent_bit_for_bit`** (`rust/geom_kernel/tests/point_index.rs`)
   — the design's single riskiest claim, that raising `k` moves no answer. It is structural,
   not statistical: `knn_batch_planned` takes its boundary as the `max` over the `k`
   candidates (non-decreasing in `k`), refetches ties *inclusively* with
   `tie_cap = source_count` (so a larger `k` can only *add* candidates), and sorts the row by
   the total order `(distance.total_cmp, item)` before truncating — so element 0 of the
   `k = 3` candidate superset is element 0 of the `k = 1` subset, bit for bit. Exercised over
   an engineered cloud whose candidate distances are exact ties at the boundary *and* over
   deterministic pseudo-random clouds at four shapes, compared by `f64::to_bits`.
2. **`fu8d_three_retained_witnesses_answer_any_two_removal`** (`specialized.rs`) — the
   retention lemma. Over the FU-8c fixture *densified* past three servers on one route key
   (its widest key carried exactly three, where the lemma is trivially true), for every
   sample and every contact pair, the first retained witness outside the pair equals a fresh
   kernel query over exactly the surviving sources, by `f64::to_bits`. It refuses to pass
   vacuously — it asserts a route wider than the slot count, a case answered from the *last*
   retained slot, and a case where the removal leaves no survivor. Setting the slot count to
   2 makes it fail, which is the check that it is load-bearing rather than decorative.

**What is charged, and what deliberately is not.**

| term | moves? | why |
|---|---|---|
| gate operations (`gate_resource_plan`) | **no** | `KnnBatchPlan`'s work is `source_count × query_count`, k-independent, and that is what `plan_route_points_nearest` charges. **Ruling R-a**, recorded in the `NEAREST_WITNESS_SLOTS` doc comment so the silence is not read as an oversight. |
| per-route kNN logical peak | yes, charged | the CSR pair arrays are `query_count × k`: `+2 × query_count × 16 B` per route, inside each route's `KnnBatchPlan::logical_peak_bytes`, against that batch's unchanged 2 GB logical-peak limit. Single-digit MB on the treant's widest route. |
| per-batch retained table | yes, charged | new `GatePeakUsage::nearest_witness_bytes` = `samples × 3 × 16 B`. On the treant's 405,146 samples that is **19,447,008 B ≈ 19 MB**, against the unchanged 1,073,741,824 B gate-peak cap whose last measured value was 216,203,088 B. An allocation this stage holds and never charges for is the defect record fixes F6 and F9 each caught once already; it is charged from the first batch that builds one. |
| `coverage_route_knn.max_neighbors` | **1 → 3** | **Ruling R-b.** Stated once, in `SpecializedLimits::production`, so it governs the production and every test limit set alike and cannot leak into the generic perception limits (which still ask for one neighbour). It is a `KnnBatchLimits` **shape field, not an FU-8 cap** — the two rows above are exactly what it admits. |

**No cap was raised and no cap code was touched.** The record sites that carried the
now-false claim "the support pipeline only ever asks for the single nearest neighbor" —
`SUPPORT_KNN_MAX_NEIGHBORS` in `constants/placeholders.rs`, its
`m0-constants-source-ledger.json` entry, the regenerated `constants.md`, and §3.10.9's table
in `03-physics-and-constants.md` — each carry the same dated amendment. The registered value
stays **1**, because it describes `SupportPreparationLimits::production`'s own
`KnnBatchLimits`, which did not change.

**Proof of the no-op is the immobility, not any new number.** Full five gates green; full
8-model census at the F1 tip with all seven frozen OD-8 digests **byte-identical**
(`Sphere 17667468971446251870`, `Cylinder 13310185350867878190`,
`make_box 9657051231007502658`, `make_overhang_table 6686599816688114065`,
`make_toed_slab 11199296506741562956`, `make_raking_blade 13278215294653054002`,
`make_thin_fin 7016743535301716631`), all 14 corpus `T0_scaled_cap` rows `at_floor=true`, and
the treant's stage error verbatim:

```
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 201480535329, limit: 35445500736 })
```

`make_raking_blade`'s digest carries the real weight here, for the reason the section above
gives: it is the one reachable model that runs a genuine merge tournament (23 gate batches,
1,402 proposals) through these exact paths, so its unchanged digest is a live corpus witness
rather than a silent one.

**One consequence worth stating plainly, because it looks like a gap.** The treant refuses at
`exact_operations` *before* `whole_gate_route_preflight_then_execute` runs, so on the treant
the retention allocates nothing and the witness table is never built. Its ~19 MB and its
gate-peak term are a **bound this run did not exercise**, and the refusal line's immobility
proves F1 moved no *charge* — it is not evidence that the retention executes at treant scale.
What proves the retention executes at all is `make_raking_blade` plus the two tests above.

#### FU-8d phase F2 landed 2026-08-06 — the nearest delta, and the first stopping point to move since FU-8b {#fu8d-f2-2026-08-06}

> **Dated addendum. Nothing above is rewritten.** This closes FU-8d **part (1)**. Parts (2)
> and (3) stay open exactly as written, and part (2) is now the treant's wall.

**What F2 did.** A merge proposal's nearest phase no longer re-sweeps the route keys its
pair perturbs against those keys' whole surviving source sets. The variant's answer for a
perturbed row is the minimum over `survivors(key) ∪ {merged seat}`, and that minimum splits:

- **the survivors' side** is *read* from the witnesses F1 retained, as a value —
  `NearestWitnessTable::surviving_squared_distance` returns the bits the kernel emitted
  during the baseline sweep, and three retained slots answer any two-contact removal exactly
  (F1's lemma, `fu8d_three_retained_witnesses_answer_any_two_removal`);
- **the merged seat's side** is **one** `KnnBatchPlan(1 source, touched_rows queries, k = 1)`
  per proposal, covering every perturbed key's rows in a single batch — the kernel's own
  `SquaredEuclidean`, never a hand-computed distance.

The two are combined by `f64::total_cmp` and the square root is taken **once, after** the
minimum. That is why an exact tie between a survivor and the merged seat cannot move the
answer: the two candidates are bit-identical `f64`, so the direction the tie breaks is
unobservable. The gather buffer is hoisted to the batch — one allocation, refilled per
proposal — rather than allocated per proposal.

**Equivalence first, as in F1 and FU-8c.** `fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit`
runs the **retained `#[cfg(test)]` reference** — `plan_variant_nearest_routes` /
`fill_variant_nearest` with `Some(proposal)`, literally the code the hot path ran per variant
before FU-8c — against the new delta, row by row on `f64::to_bits`, and re-checks the
touched-superset and restore invariants. The fixture is grown past FU-8c's until every arm is
reached, and the test **refuses to pass vacuously**: it asserts that a perturbed key with
fewer than three servers occurred, that some proposal removed **every** server of a key it
perturbs, that some row's baseline nearest was merged away, that the merged seat **stole** a
row from a surviving holder, and that a survivor and the merged seat tied **exactly**.
Truncating the witness scan to slot 0, or dropping the merged seat from the minimum, each
make it fail.

**Repricing — `gate_resource_plan` is the one authority, and no cap moved.**

| term | at F1 | at F2 | shape |
|---|---:|---:|---|
| `baseline_rows` | 554,926,572 | 554,926,572 | `(samples + demands + 1) × contacts` |
| proposal `nearest_work` | 195,222,841,902 | **1,041,544,686** | was `Σ_keys s × q`; now `Σ 1 × touched_rows` |
| proposal `touched_rows` | 1,041,544,686 | 1,041,544,686 | write, bounded witness scan, restore |
| proposal `key_visits` | — (uncharged) | **78,446** | `routes(l)+demands(l)+routes(r)+demands(r)` |
| proposal `demand_rows` | 42,949,185 | 42,949,185 | `proposals × demands` |
| `face_rows` | 4,618,272,984 | 4,618,272,984 | `variants × distinct faces` |
| **stated demand** | **201,480,535,329** | **7,299,316,559** | |
| **cap — UNCHANGED** | 35,445,500,736 | 35,445,500,736 | |

The F1 column reconciles exactly with the residual FU-8c recorded:
`195,222,841,902 + 1,041,544,686 = 196,264,386,588`. So the per-proposal nearest term fell
**94.2×** and the stated demand fell **27.6×**, to **20.6 % of the unchanged cap** —
`gate_resource_plan` now **passes**. `key_visits` is newly charged because after F2 the cost
of *deciding* a proposal's shape is no longer lost in the noise of evaluating it, which is
the hole part (3) names. **No cap was raised, lowered or touched, and no cap code was
edited.** Per ruling **R-e** no literal of the post-fix total is pinned in a test; the
count-derived terms keep derivation-shaped pins
(`fu8c_repriced_plan_admits_the_treant_gate_shape_under_the_unchanged_cap` gained
`key_visits: 8` so its assertion stays a derivation, and
`merge_gate_resource_caps_are_exact_and_cap_minus_one_refuses` re-split `20` into `14 + 6`
so its pinned total of 140 is deliberately unmoved).

**The stopping point moved, and that is the phase's real result.** The treant still refuses
at the merge gate, still with `GateOperationLimit`, but no longer at the same check:

```
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 35445618202, limit: 35445500736 })
```

**That number is not a total.** `variant_specialized_owner_face_count` refuses the instant
its running counter crosses, which it does at **variant 275 of 39,224** — 0.70 % of the
batch — so `35,445,618,202` is "the cap plus one variant's descriptor work". Measured at the
crossing: `descriptor_operations` 35,445,417,382 and `unique_face_finalizer_work`
**3,036,066,151,957** over those 275 variants, extrapolating to ~5.06 × 10¹² and
**~4.33 × 10¹⁴** over the full batch, against routed (nearest) work of 3,912,181 — about one
part in a million of what the variant fold costs. **The wall is entirely FU-8d part (2)**,
the per-variant load and owner-face path, and phase F4 is what addresses it. It is not a cap
to raise, on the grounds §5 and §7.2 of the caps doc already set out.

**Rulings recorded here because this is where they bite.**

- **R-c — the radius-bounded steal path was NOT built, and the measurement says leave it
  out.** F2's scratch pass measured, over all 39,223 proposals, `|{s : d(merged, s) ≤
  R_max(key)}|` with `R_max(key)` the largest baseline nearest distance the key carries:
  mean 15,399.5, p50 13,270, p99 29,303, max 38,679, sum 604,013,078 against 1,041,544,686
  touched rows. The bound removes **42 %** of rows — a 1.7× constant, not an order — and the
  adoption rule was "only if the measured margin is under 2×". The margin is **4.9×**, so
  R-c is **not adopted**.
- **R-d — proposal batching by key: precondition met, deliberately not built.** 28 of the 48
  perturbed keys carry ≥ 10 proposals and one carries 26,543, so the ≥ 10 test passes; but it
  is a constant-factor cleanup on a term that is now some parts per million of the wall.
- **R-f — FU-23 is NOT re-derived, and must not be.** `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED`
  waits for phase F3 **and** a treant run that **completes** the gate. This run does not.

**What was re-measured.** Full 8-model census at the F2 tip, release, `--features census`,
`PLINTH_THREADS=4`, 3,415 lines: all seven frozen OD-8 digests **byte-identical**
(`Sphere 17667468971446251870`, `Cylinder 13310185350867878190`,
`make_box 9657051231007502658`, `make_overhang_table 6686599816688114065`,
`make_toed_slab 11199296506741562956`, `make_raking_blade 13278215294653054002`,
`make_thin_fin 7016743535301716631`), all 14 corpus `T0_scaled_cap` rows `at_floor=true`, and
the treant stage error above. The treant run is **129 s at 2,641,504 KB (2.52 GiB) peak
RSS**, `PLINTH_THREADS=4`, watchdog armed at 90 GB and never fired — superseding F1's
124 s / 2.50 GB. Five gates green.

**`make_raking_blade` again carries the corpus weight**, and here it carries more than it did
at F1: F1 changed no answer, so its unchanged digest proved retention was harmless. F2
changes *how* every merge proposal's nearest is computed, and `make_raking_blade` is the one
reachable model that runs 1,402 real merge proposals through exactly that code. Its digest
being unmoved is a live witness that the new delta reproduces the old sweep on real geometry,
not only on the fixture.

**One limit of this phase, stated plainly.** The treant still refuses before
`whole_gate_route_preflight_then_execute`, so on the treant the witness table is still never
built and the new delta still never executes at treant scale. What F2's treant run proves is
that the *charge* fell as designed and that the refusal moved; what proves the delta executes
is `make_raking_blade` plus the equivalence test.

#### FU-8d phase F3 landed 2026-08-06 — the load partition delta, and the treant enters its merge tournament {#fu8d-f3-2026-08-06}

> **Dated addendum. Nothing above is rewritten.** This closes FU-8d parts **(2)** and **(3)**.
> Part (1) was closed by F2. The fourth candidate F2 recorded as *unmeasured* — the
> per-variant `assess_coverage` — is now the treant's stopping point, and is opened as
> **FU-8e** on the evidence below.

**The headline.** *The Weeping Treant's merge gate no longer refuses.* Its first merge batch
states **9,273,857,317** operations against the **unchanged** 35,445,500,736 cap — **26.2 %**
of it — and proceeds into execution, where it evaluates all 39,224 variants, accepts a merge,
and opens the next batch. That is the first time in this record that the treant has *executed*
a gate batch rather than been refused by one. **No cap was raised, lowered or touched, and no
cap code was edited.**

**What F3 did — three changes, one theme.** Each removes work the gate repeated per variant
or per proposal, and each replaces a hand-built model of a cost with the cost itself.

- **Part (2), the load partition (P2).** `finalize_specialized_load_partition` ran once per
  variant: a dense `contacts × rows` argmin over the whole extended eligibility, preceded by a
  per-variant rebuild of the `(owner, face)` pair list. On the treant that is
  `(117,741 + 7,971,451) × 1,365 ≈ 1.10 × 10¹⁰` per variant and **~4.33 × 10¹⁴** over the
  batch. It now runs **once**, retaining three nearest eligible contacts per row, each
  contact's won rows, its carried area, and the baseline violation count; a proposal
  re-answers only `elig(left) ∪ elig(right)` against that.
- **The locality argument, in both directions, and it is an identity rather than a bound.**
  The merged seat's `(owner, face)` pairs come from `visit_served_owner_faces` over
  `ContactParts::new(left, Some(right))`, and `parts_serves_owner` is true for an island
  exactly when `left` or `right` serves it — so the merged seat serves **exactly**
  `elig(left) ∪ elig(right)`. A row outside that union loses no candidate (neither member was
  eligible) and gains none (the merged seat is not eligible), so its candidate set is
  bit-identical to the baseline's *in the same order* and its argmin is the baseline's. A row
  inside is answered from three retained witnesses plus one distance. The row **set** is
  variant-invariant for the same reason.
- **The renumbering invariant — the highest wrong-but-plausible risk, and it is pinned.**
  `append_variant_load_inputs` renumbers contacts and owners per variant, and the finalizer's
  argmin tie-breaks on `contact_id`. The delta is sound only because the renumbering is
  **order-preserving on the survivors** and the **merged seat is last**, so it carries the
  largest id in its variant and loses every exact distance tie.
  `fu8d_load_delta_matches_the_per_variant_finalize` builds each variant's inputs exactly as
  `append_variant_load_inputs` builds them, runs the *whole* production finalizer on them, and
  compares row by row, on `load_violation_count`, and on each touched contact's carried area
  by `f64::to_bits`. It refuses to pass vacuously: it asserts it reached an exact
  survivor/survivor tie, an exact survivor/merged-seat tie, a row with no survivor, a
  displaced holder, a steal, and a row outside the union.
- **`f64` addition is not associative, and the delta does not pretend otherwise.** A touched
  contact's carried area is **re-summed** over its variant rows in the fold's own ascending
  row order rather than adjusted incrementally. That costs a `carried_rows` term, and it is
  why the violation counts are bit-identical rather than nearly so.
- **The second wall, behind the operations wall, is structurally gone.** The gate
  materialized, per window of up to `max_gate_window_variants` variants, one
  `PhysicalContact` matrix and one `(owner, face)` matrix **per variant in the window** — on
  the treant 7,971,451 pairs × 8 B *each*. `gate_window_matrix_bytes` therefore stated a
  figure in the tens of gigabytes against `max_gate_contact_matrix_bytes` = 512 MiB, and the
  only reason that check never fired is that `exact_operations` refused the batch first.
  Removing the operations wall without removing the matrices would have moved the refusal onto
  a byte limit. There is now one physical list and one pair list for the whole batch;
  `gate_window_matrix_bytes` and `traverse_whole_gate_windows` are **deleted**, and
  `GatePeakUsage::matrix_bytes` is replaced by `load_baseline_bytes`, the batch's one retained
  load state.
- **Part (3), the planning reclaim (4 → 1) and its pre-check.** `plan_proposal_nearest_delta`
  ran four times per proposal — batch plan, `exact_operations` fold, preflight re-plan,
  execution — while the check that refuses sits after the first two. It runs **once** now; the
  plans are retained in `GatePlannedNearest` and read by the other three sites, and the
  retention is charged as `GatePeakUsage::proposal_plan_bytes`. Before the loop,
  `proposal_planning_precheck` states the planning pass's own charged term —
  `Σ_proposals routes(left) + demands(left) + routes(right) + demands(right)` — from the batch
  header alone and checks it against the same materialized cap, so the gate can no longer
  spend the whole planning pass discovering it may not proceed.
- **Part P5, the per-variant rescans.** `variant_operation_count` walked every contact and
  every demand for each of 39,224 variants to re-derive two proposal-invariant totals.
  `GateVariantTotals` computes them once, and a proposal variant's logical contact work is the
  baseline's **minus one, exactly**: writing `n` for contacts, `R` for `Σ routes` and `D` for
  `Σ demands`, the baseline is `n + R + D` and the variant is
  `(n − 2) + (R − r_l − r_r) + (D − d_l − d_r) + (1 + r_l + r_r + d_l + d_r) = n − 1 + R + D`.
  Every proposal-specific term cancels, so the derivation never asks which pair merged.

**Repricing — `gate_resource_plan` is the one authority, and the load term is now honest.**
Measured on the treant's first merge batch (samples 405,146; demands 1,095; contacts 1,366;
proposals 39,223; variants 39,224; distinct faces 117,741):

| term | at F2 | at F3 | shape |
|---|---:|---:|---|
| `baseline_rows` | 554,926,572 | 554,926,572 | `(samples + demands + 1) × contacts` |
| proposal `nearest_work` | 1,041,544,686 | 1,041,544,686 | `Σ 1 × touched_rows` |
| proposal `touched_rows` | 1,041,544,686 | 1,041,544,686 | write, witness scan, restore |
| proposal `key_visits` | 78,446 | 78,446 | now **pre-checked** before it is spent |
| proposal `demand_rows` | 42,949,185 | 42,949,185 | `proposals × demands` |
| load term | 4,618,272,984, as the model `variants × faces` | **6,592,813,742** | 5,543,831,115 baseline + 1,048,982,627 proposal bound |
| **stated demand** | **7,299,316,559** | **9,273,857,317** | 26.2 % of the cap |
| **cap — UNCHANGED** | 35,445,500,736 | 35,445,500,736 | |

The stated demand **rose**, and that is the phase working rather than failing. FU-8d part (2)
recorded that the gate charged the load phase `variants × faces` while the fold really did
`variants × faces × contacts` — an under-charge, and the reason that term had never been the
refusing one. There is one authority now: `specialized_load_resource_plan`'s **own** figure
for the batch's single baseline, plus a count-derived bound for the deltas, and the honest
number is 1.43× the model it replaced. What collapsed is what actually refused:

| term | at F2 | at F3 |
|---|---:|---:|
| `descriptor_operations` (`variant_specialized_owner_face_count`, × 4 × variants) | 35,445,417,382 **at variant 275 of 39,224**, extrapolating to ~5.06 × 10¹² | **136,791,451**, charged once |
| `unique_face_finalizer_work` | 3,036,066,151,957 over 275 variants, extrapolating to ~**4.33 × 10¹⁴** | **gone** — the term no longer exists |
| `exact_operations` | refused at the crossing | **10,457,910,996** against 35,445,500,736, **29.5 %** |

> **Dated addendum 2026-08-07 — FU-8f phase G5. Both tables above and the headline that
> opens this section are left standing exactly as measured; four of their figures have since
> moved, downward, and none of them because a bound was relaxed.** `load term`
> **6,592,813,742 → 1,844,748,658**, its baseline component **5,543,831,115 → 795,766,031**,
> `stated demand` **9,273,857,317 → 4,525,792,233** (26.2 % → **12.8 %** of the same unchanged
> 35,445,500,736), `exact_operations` **10,457,910,996 → 5,709,845,912** (29.5 % → **16.1 %**).
> `baseline_rows`, `demand_rows`, the proposal terms and `descriptor_operations` are all
> byte-identical. The cause is `load_resource_plan_from_counts` ceasing to price the load scan
> as `rows × contacts × (owner_log + 4)` — a model of a dense scan phase G1 had already
> replaced. **The sentence "the stated demand rose, and that is the phase working rather than
> failing" still holds and now runs in both directions:** a cap should price what a stage
> really does, and what this stage really does turned out to be *less* than what it was
> charged. **No cap moved.** Full record: *FU-8f phase G5 landed 2026-08-07*.

Per ruling **R-e** no literal of the post-fix total is pinned in a test; the dated transcript
above is the record, and the count-derived pins keep their derivation shape
(`merge_gate_resource_caps_are_exact_and_cap_minus_one_refuses` still pins 140, with the
struck `face_rows = 3 × 7 = 21` supplied as
`GateLoadWork { baseline_operations: 21, proposal_bound: 0 }`).

**`GateRoutePlanMismatch` is re-founded, and it is weaker — said plainly rather than left to
be discovered.** It used to compare a plan against a **second plan**: the preflight planned
every proposal's delta, execution planned them all again, and the two were compared. With
execution reading the batch's one plan that comparison would be a tautology, so the check now
compares what execution *did* against what was planned — the peak bytes it measured, and
`touched_rows` counted from `touched.len()` as each delta filled. It attests "the batch
executed the work it planned, once per proposal and in order" rather than "two identical
computations agreed". The predecessor's extra strength was bought by paying for the whole
planning pass twice, which is exactly the defect part (3) opened; the trade is deliberate.

**Where the treant stands now.** It clears the merge gate's resource plan, executes the
39,224-variant batch, accepts a merge, and opens the next at 1,365 contacts — and then the
next. Under `PLINTH_THREADS=4`, release, `--features census`, one model per invocation, it
planned **eight gate batches inside the 600 s timebox** — the proposal-free batch plus seven
merge batches at 1,366 → 1,360 contacts — and did not finish the tournament, so this run is
recorded as **EXCEEDS-TIMEBOX with counters**, not as a completion:

| batch | contacts | proposals | stated demand | cap | share |
|---|---:|---:|---:|---:|---:|
| proposal-free | 1,366 | 0 | 6,098,757,687 | 35,445,500,736 | 17.2 % |
| merge 1 | 1,366 | 39,223 | 9,273,857,317 | 35,445,500,736 | 26.2 % |
| merge 2 | 1,365 | 39,212 | 9,269,546,195 | 35,419,571,328 | 26.2 % |
| merge 3 | 1,364 | 39,209 | 9,265,128,598 | 35,393,641,920 | 26.2 % |
| merge 4 | 1,363 | 39,203 | 9,260,793,845 | 35,367,712,512 | 26.2 % |
| merge 5 | 1,362 | 39,189 | 9,256,095,303 | 35,341,783,104 | 26.2 % |
| merge 6 | 1,361 | 39,180 | 9,251,710,261 | 35,315,853,696 | 26.2 % |
| merge 7 | 1,360 | 39,174 | 9,247,211,887 | 35,289,924,288 | 26.2 % |

The demand falls monotonically as the tournament shrinks the contact set and the cap falls
with it, holding the share flat at 26.2 % — nothing is diverging; there is simply a great deal
of tournament. Peak RSS sampled once per second held around **2.4–2.5 GB** across the run,
watchdog armed at 90 GB and never fired, far under the 100 GB machine ceiling.

**The next defect is the one F2 recorded as unmeasured, and this run measures it by
implication.** `assess_coverage` runs `O(samples)` per variant with about six
`Vec::with_capacity(405,146)` allocations per call: `39,224 × 405,146 ≈ 1.59 × 10¹⁰` sample
visits and of order a terabyte of allocation churn **per gate batch**, none of it charged
against `gate_operation_limit`. That is what the 600 s is being spent on. It is opened as
**FU-8e** on exactly the footing the deferral ruling required — F4's measured evidence, which
this is — and it is **not** a cap to raise.

**Ruling R-f is still NOT satisfied.** `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` waits for a
treant run that **completes** the gate. This run completes seven merge rounds of a tournament
and then hits the timebox, so FU-23 is not re-derived and must not be.

**What was re-measured.** All seven frozen OD-8 digests **byte-identical** at the F3 tip
(`Sphere 17667468971446251870`, `Cylinder 13310185350867878190`,
`make_box 9657051231007502658`, `make_overhang_table 6686599816688114065`,
`make_toed_slab 11199296506741562956`, `make_raking_blade 13278215294653054002`,
`make_thin_fin 7016743535301716631`). **`make_raking_blade` carries more weight here than in
any earlier phase**: it is the one reachable model that runs a real merge tournament — 23 gate
batches, 1,402 proposals — and F3 rewrote what a proposal variant's *load partition* is. Its
unchanged digest is a live witness that the load delta reproduces the per-variant finalize on
real geometry, not only on the fixture. All five gates green; all instrumentation used for the
figures above is scratch, `PLINTH_FU8D_MEASURE`-gated, and reverted before the gates.

> **Correction added 2026-08-06 (phase F4 measured it directly) — the section above is left
> standing exactly as written, and one of its inferences is now known to be wrong in
> proportion.** The paragraph beginning *"The next defect is the one F2 recorded as
> unmeasured, and this run measures it by implication"* attributes the 600 s to the
> per-variant `assess_coverage` and says "that is what the 600 s is being spent on". **It is
> not what most of it is being spent on.** F4 sampled the running process instead of inferring
> from counts, and `assess_coverage` is the *second* frame, at roughly a quarter of
> main-thread time; the largest is `fill_proposal_nearest_delta` — F2's own nearest delta — at
> about three fifths. The mechanism is not its arithmetic but its *construction*: the delta
> builds a fresh `PointCloudIndex` kd-tree over a **one-point** cloud, once per proposal. The
> `1.59 × 10¹⁰`-sample-visit arithmetic for `assess_coverage` stands and is real; what does
> not stand is the claim that it is the whole of the time. **FU-8e is opened on both, in
> measured order.** See *FU-8d phase F4 measured 2026-08-06*.

> **APPENDED 2026-08-07 (FU-8f phase G3) — `GateRoutePlanMismatch` is re-founded a second
> time, and this section's statement of it is narrowed by exactly one clause. Nothing above is
> rewritten.** F3's sentence *"`GateRoutePlanMismatch` is re-founded as plan-vs-execution and
> is explicitly weaker; the record says so"* stands as the description of what F3 built. What
> G3 does to it is stated here, in F3's own section, so a reader who arrives at F3's
> attestation is told what became of it:
>
> > `GateRoutePlanMismatch` is re-founded for the second time, and it is weaker again — said
> > plainly rather than left to be discovered. At F1–F2 it compared a plan against a **second
> > plan**. FU-8d phase F3 re-founded it as plan-vs-execution, attesting *"the batch executed
> > the work it planned, once per proposal and **in order**"*, counting `touched_rows` from
> > `touched.len()` as each delta filled and comparing measured peak bytes. FU-8f severs
> > charged-batch from executed-batch, so *"every planned proposal was executed"* is no longer
> > true and **must not be claimed**.
> >
> > What it attests now: **"the batch executed a prefix of the work it planned — the first
> > `executed_variant_count` variants, in plan order, each exactly once, at the peak the plan
> > stated"**, together with `executed_variant_count <= plan.variants` and the equality case.
> > The three fingerprints are therefore compared **prefix-wise**: the executed nearest plan
> > must equal the plan's own first `executed_variant_count` records, not its total.
> >
> > What is lost is the attestation that the tail was executed at all — and the tail is
> > precisely what the owner's ruling authorises not executing:
> >
> > > *"2.1 hours down to 28 minutes seems worth it; anywhere else that we can also still
> > > shave more time off?"* — Craig, 2026-08-07
> >
> > The trade is deliberate and is the ruling's, not the implementation's. What is **not**
> > weakened: the batch is still admitted on the whole batch's charge, so nothing is executed
> > that was not first afforded; and the no-safe-proposal path still evaluates every variant,
> > so the refusal path's attestation is exactly F3's.
>
> Full record: *FU-8f phase G3 landed 2026-08-07*.

### FU-8d phase F4 measured 2026-08-06 — the treant is sampled, not inferred; EXCEEDS-TIMEBOX twice {#fu8d-f4-2026-08-06}

> **Dated addendum. Nothing above is rewritten.** F4 lands **no engine code change**: its
> deliverables are a clean measurement at the committed F3 tip and the record sweep the
> every-record-site rule still owed F3. **No cap was raised, lowered or touched.**

**The outcome, pre-committed among three, is (c) EXCEEDS-TIMEBOX** — reported with counters
and a stack sample naming the frame, which is what makes it a measurement rather than a
failure. It is not (a): the treant reaches no seal and there is still no treant digest. It is
emphatically not (b): **no typed refusal fired, at any cap, in either attempt.**

**Conditions (ruling R-g — the entire run *and* its watchdog ran unsandboxed, because `ps` is
blocked in the sandbox and a blind watchdog is decoration).** Release, `--features census`,
`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, `PLINTH_THREADS=4`, RSS polled
every 2 s with a hard kill armed at 90 GB, 600 s timebox, **two attempts, the maximum
allowed**. The binary at this tip carries **no** `PLINTH_FU8D_MEASURE` instrumentation —
verified absent by grep before building — which is exactly why this section reports frames,
wall-clock and bytes but **no batch counters**. The per-batch table in *FU-8d phase F3 landed
2026-08-06* remains the counter record; F4 does not restate it as though independently
reproduced.

| attempt | verdict | wall clock | peak RSS | watchdog | typed refusal |
|---|---|---:|---:|---|---|
| 1 | EXCEEDS-TIMEBOX | 607 s | 2,606,256 KB (2.49 GiB) | armed 90 GB, **never fired** | none |
| 2 | EXCEEDS-TIMEBOX | 602 s | 2,600,960 KB (2.48 GiB) | armed 90 GB, **never fired** | none |

Peak RSS is **2.5 GiB against a 90 GB kill line and a 100 GB machine ceiling** — four orders
of margin. Memory is not this stage's problem and has not been since FU-8b.

**Attempt 2 exists because attempt 1's evidence was a single 5-second window at t = 600.** One
window cannot distinguish "this frame dominates the run" from "this frame happened to be
running at the end", so attempt 2 re-ran the same binary and sampled it at six points across
the timebox. That is the only difference between the attempts.

**Where the time goes — sampled, main-thread samples per window:**

| t (s) | main-thread samples | `fill_proposal_nearest_delta` | `assess_coverage_with_mode` | `merged_violation_count` |
|---:|---:|---:|---:|---:|
| 60 | 2,033 | — still in preparation (`ParityVote::inside`) | — | — |
| 180 | 2,268 | 1,386 (61.1 %) | 554 (24.4 %) | 115 (5.1 %) |
| 300 | 2,278 | 1,366 (60.0 %) | 574 (25.2 %) | 112 (4.9 %) |
| 420 | 2,275 | 1,382 (60.7 %) | 579 (25.4 %) | 104 (4.6 %) |
| 540 | 2,278 | 1,393 (61.2 %) | 563 (24.7 %) | 95 (4.2 %) |
| 597 | 2,264 | 1,402 (61.9 %) | 542 (23.9 %) | 95 (4.2 %) |
| **gate phase, pooled** | **11,363** | **6,929 (61.0 %)** | **2,812 (24.7 %)** | **521 (4.6 %)** |

Attempt 1's independent window at t = 600 gives 2,217 / 3,787 = **58.5 %** and
1,293 / 3,787 = **34.1 %** for the same two frames. Read the split as **≈ 59–62 % / 24–34 %**
over two runs and seven windows, not as a pinned pair (ruling R-e: no literal is pinned in a
test; this dated transcript is the record).

**The design predicted the wrong frame, and that is the phase's real result.** The deferral
ruling opened P4 — the per-variant `assess_coverage` — as the expected timebox frame, and F3's
record repeated it. Measured, `assess_coverage` is real but *second*. The leader is **F2's own
nearest delta**, and its cost is not arithmetic but construction:

```text
fill_proposal_nearest_delta (specialized.rs:~5702), once per proposal:
    let merged_seat = [proposal.position_mm];               // ONE point
    PointCloudIndex::new_for_knn_batch(&merged_seat, ...)   // build a kd-tree over it
    point_index.knn_batch_planned(query_scratch, ...)       // query it, then drop it
```

Beneath that frame attempt 1's call graph shows `knn_batch_planned` and, under it,
`_xzm_free` / `_free` / `_xzm_xzone_malloc` — **allocator traffic** — plus `kiddo`'s
`nearest_n_within_recurse` descending a tree with a single node. The batch-first reading is
exact: the delta is properly batched *within* a proposal (one plan covers all its touched
rows) and wholly unbatched *across* proposals — **39,223 kd-tree constructions and teardowns
per gate batch, each to answer distances to one point**. A per-item call in a hot path is a
defect, and this is one. **It is not a cap to raise**; none of it is charged against
`gate_operation_limit` in the first place, and the fix removes work rather than admitting more.

**Two further observations the sample makes for free, recorded rather than acted on:**

1. **`PLINTH_THREADS=4` buys the merge gate nothing.** In every window all four rayon workers
   sit in `__psynch_cvwait` — 15,148 of attempt 1's samples — while the entire gate runs on
   the main thread. Preparation *is* parallel (at t = 60 the workers are busy in
   `ParityVote::inside`); the gate stage is not. This is **not** an invitation to raise thread
   counts: under FU-8e's reading the work should cease to exist rather than be spread over
   more cores. It does mean the treant's gate wall-clock is a single-core number.
2. **`SpecializedLoadBaseline::merged_violation_count` is a steady third at ~4.6 %** — too
   small to be the wall, large enough to surface once the first two are fixed. Named here so a
   later phase does not meet it as a surprise.

**Ruling R-f is STILL NOT satisfied, and this was the run with the best chance of satisfying
it.** `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` (FU-23) is re-derived only from a treant run
that **completes** the gate. Neither attempt completes it. **Nothing was re-derived and the
coefficient is untouched.**

**Rulings, recorded where each bites.** **R-a** lives in the `NEAREST_WITNESS_SLOTS` doc
comment (landed F1) and is unaffected. **R-b**'s byte arithmetic lives in the
`SUPPORT_KNN_MAX_NEIGHBORS` registry note, explicitly labelled "not a cap" (landed F1), and is
unaffected. **R-c** and **R-d** are measured and left unbuilt in
`support-resource-caps-2026-08-05.md` §9.5 (landed F2) — and F4 **revives R-c as a question**,
because the frame a radius bound would have shrunk is now the leading one; that is recorded as
FU-8e input, not adopted here. **R-e** is honoured above. **R-f** is unsatisfied, stated twice.
**R-g** governed both attempts.

**What F3 owed the record, and F4 pays.** F3's sweep reached `CURRENT-STATUS.md`, `README.md`
and the caps doc, but its landing also falsified "the treant is refused at the merge gate" in
four sites it did not reach: `03-physics-and-constants.md` §3.10, the
`SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` and `SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS` registry
notes (and therefore `constants.md`, regenerated by its producer and never hand-edited), and
the `gate_operation_limit` doc comment in `specialized.rs`. All now carry dated F3/F4 addenda
in this commit set. The caps doc's phase-F3 section had also landed as a **second `## 9`**; it
is renumbered **§10** with a note, and the pointers that meant it now say §10.

**OD-8 at the F4 tip — MEASURED ZERO.** All seven reachable digests re-run one model per
invocation from a freshly built `--release --features census` binary, byte-identical to the
frozen values: `Sphere 17667468971446251870`, `Cylinder 13310185350867878190`,
`make_box 9657051231007502658`, `make_overhang_table 6686599816688114065`,
`make_toed_slab 11199296506741562956`, `make_raking_blade 13278215294653054002`,
`make_thin_fin 7016743535301716631`. Routed contacts and final live member counts are also
unmoved. This is the expected result and the phase would be suspect without it: **F4 changes
no engine code**, only doc comments, registry note strings and prose, so any digest movement
would have meant something unintended had been edited. `constants.md` was **regenerated by its
producer** (`cargo run -p plinth-core --offline --example render_constants_golden`), never
hand-edited, and the regeneration touched exactly the two lines whose registry notes changed.
**All five gates green at this tip.**

### FU-8e phases E1–E4 landed 2026-08-07 — the merge round falls 4.35×, and ruling R-f is still unsatisfied {#fu8e-e1-e4-2026-08-07}

> **Dated addendum. Nothing above is rewritten.** This closes **FU-8e** on the defect it
> named and opens **FU-8f** as a *recorded finding awaiting owner ruling*, not as work in
> progress. No cap, floor or coefficient was raised, lowered or touched by any of E1–E4.

**RULING R-f REMAINS UNSATISFIED, and this phase was never going to satisfy it.** Stated
before any measurement below, because the previous phase's record over-claimed in exactly this
direction and the discipline exists to stop that repeating.
`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` (FU-23) is re-derived **only** from a Weeping
Treant run that **COMPLETES** the gate stage. FU-8e's clean run at its own tip is an
**EXCEEDS-TIMEBOX at 601 s**, and the arithmetic says why no single-timebox run at this tip
could be otherwise:

- The merge tournament commits **exactly one** merge per round, so the round count is bounded
  by the contact count: **1,366 contacts ⇒ at most 1,365 rounds**.
- `make_raking_blade` — the only other proposal-carrying corpus model — completes its
  tournament in **21 rounds, 62 → 41 contacts** (33.9 % of its contacts). At that ratio the
  treant needs about **460** rounds.
- The measured round is **13.8 s** inside the gate batch and **14.4 s** end to end. So the
  post-fix gate stage is **≈1.8 h at the blade ratio and ≈5.5 h at the contact bound** —
  **11× to 33× the 600 s box**.
- The run reached **33 merge rounds**: **7.2 %** of the blade-ratio estimate, **2.4 %** of the
  contact bound. That is not a completed gate, and nothing here re-derives anything.

**What landed, in four phases.**

- **E1 (`13454dc`, `0d048f1`) — the kernel gains a one-source fast path, and nothing calls
  it.** `knn_single_source_planned_into` answers a `source_count == 1, neighbors == 1` batch
  without building a tree, computing with the kernel's own `SquaredEuclidean` so the kernel
  stays the distance authority. Bit-identity is *forced*, not hoped for, and the five-clause
  argument is in the code: `KnnBatchPlan::new` requires `neighbors <= source_count` so `k = 1`
  is the only legal `k`; one point means one leaf, no stems, no descent, no pruning; the leaf
  remainder loop is the **same expression, same term order, same `0.0` seed** as
  `SquaredEuclidean::dist`; `kiddo` is pinned `=5.3.3` with `default-features = false` so the
  SIMD paths are both compiled out **and** unreachable at zero full chunks; and the boundary
  in `knn_batch_planned` is that same distance, so the row survives `take(1)` by identity.
  Pinned by `fu8e_single_source_is_the_tree_answer_bit_for_bit` over engineered edge cases
  (coincident, ±0, subnormal, 1e150, epsilon-apart, sign-mixed, mixed-magnitude) plus ≥20,000
  random pairs, on `to_bits`. **E1 landed with nothing calling it: no charge moved, no digest
  moved, no timing moved. The immobility is the phase's proof.**
- **E2 (`af9e1d0`) — the gate consumes it.** `fill_proposal_nearest_delta` drops the
  per-proposal `PointCloudIndex::new_for_knn_batch` + `knn_batch_planned` for the E1 call into
  a batch-retained `Vec<f64>`. `plan_proposal_nearest_delta` is **unchanged**, so `work()` is
  still `1 × touched_rows` and **the treant's demand stays byte-identical at 9,273,857,317**.
  The proof is a test nobody rewrote:
  `fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit` compares every row
  against the retained `#[cfg(test)]` full sweep on `to_bits`, its body is byte-identical to
  the pre-change file, and it passes.
- **E3 (`dcca4e9`, `e5b9fd8`) — the gate stops re-assessing coverage it cannot have changed.**
  The batch's one baseline `CoverageObjective` is hoisted out of the variant loop (it *is*
  variant 0's own call), `fill_proposal_nearest_delta` folds a two-counter probe into the row
  loop it already runs, and a variant whose rewritten rows cannot have moved the objective is
  handed the retained baseline objective — no scan, no allocation, no assessment. Any other
  variant calls `assess_coverage` **verbatim**, through the same `GateCoverageQuestion`, so
  the two arms cannot drift into asking different questions.
- **E4 (this commit set) — measure and record.** The optional scratch-buffer fold-in was
  **measured and declined**; the treant was run clean under R-g; the record was swept.

**THE DESIGN'S FRAMING WAS WRONG TWICE, AND THE CORRECTIONS ARE THE REASON THE PHASES WORK.**
Both are recorded in full in `support-resource-caps-2026-08-05.md` §12.2 and in the E2/E3
commit messages.

1. **The 62 % was never index construction.** One one-point `PointCloudIndex` costs **83.3 ns**
   and the 39,223 of them total **≈3.27 ms** per batch — the trees were free. The cost
   was the **query loop**: ~1.04 × 10⁹ query iterations and ~2.08 × 10⁹ heap allocations per
   batch, from two tree descents and a `sort_by` per query.
   **[Arithmetic corrected 2026-08-07 (records fix): this total landed as "16.7 microseconds",
   an arithmetic slip in the design pass and not a second measurement — 39,223 × 83.3 ns =
   3.267 ms, where 16.7 µs would have implied 0.426 ns per construction. The per-build
   **83.3 ns is retained** as the design pass's measurement of record; only the product is
   corrected, and the conclusion it was quoted for is unchanged — 3.27 ms is **0.009 %** of the
   37.33 s `fill_proposal_nearest_delta` frame, so construction stays negligible and the fix
   still targets the query loop. Primary note: `support-resource-caps-2026-08-05.md` §12.2.]**
2. **The 28 % was never allocation churn.** The FU-8e finding as opened said "~7
   `Vec::with_capacity(405,146)` per call … of order a terabyte of allocation churn". It is
   **six** vectors in `RepairNoEvidence` mode, the traversal between them visits only
   *uncovered* rows, and the cost is the **two unconditional full scans** bracketing it —
   ≈254 GB of reads per batch, ≈17 s at memory bandwidth. **The fix is to not scan, not to not
   allocate**, and E3's fallback allocates exactly as it always did.

**A third correction, inside E3, was a soundness fix and not a framing one.** The design
specified the reassessment trigger as a status-**flip** count. That is **wrong**:
`worst_gap_mm` is a `max` over the nearest *values* of a failing island's samples, so a row
that stays uncovered and moves 12 mm → 11 mm lowers it while flipping nothing, and a
flip-counting trigger would have handed the gate the stale 12. The shipped trigger is the
wider "an uncovered row **moved**", which strictly contains the flip test. That is a wrong
answer prevented, silently, on a field the gate compares — not a missed speedup.

**E4 part (a) — the optional change, measured and DECLINED.** Folding
`merged_violation_count`'s two per-proposal `Vec`s into `SpecializedLoadDeltaScratch` was
measured at **6.7 ms per round: 0.13 %** of that function's cost and 0.05 % of the round.
`touched_count` is not the contact count — a merge proposal touches a mean of **3.77
contacts**, so each allocation is about fifteen bytes. Recorded at the allocations, in the
`SpecializedLoadDeltaScratch` doc comment (`8e134a7`).

**THE RUN (ruling R-g — the entire run *and* its watchdog ran unsandboxed, because
`ps -o rss=` is blocked in the sandbox).** 2026-08-07, `PLINTH_CENSUS=1
PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, release, `--features census`, `PLINTH_THREADS=4`,
RSS polled every 2 s, watchdog armed at **90 GB**, 600 s timebox, **one attempt** of a
permitted two. Timers were env-gated scratch counters at the branch tip, reverted by editing
afterwards (`git status` clean against the commit; a grep for every scratch identifier returns
nothing). Per **R-e** no total is pinned in a test — the dated transcript is the record. Full
per-round table in `support-resource-caps-2026-08-05.md` §12.3.

- **Outcome: EXCEEDS-TIMEBOX at 601 s** with **33 merge rounds** completed, 1,366 → 1,334
  contacts. **No typed refusal at any cap.** Peak RSS **2,565,232 KiB = 2.446 GiB**; the 90 GB
  watchdog **never fired**.
- **Round mean 13.818 s, median 13.398 s, range 12.078–20.794 s** (batch preparation 3.884 s
  + variant loop 9.934 s). Against the design pass at `7aa6b73`: **63.83 s → 14.68 s on the
  first merge round, 4.35×**; the variant loop **60.23 s → 10.73 s, 5.6×**.
- **`fill_proposal_nearest_delta` 37.33 s → 4.67 s. Coverage 17.05 s → 0.001 s.**
  `merged_violation_count` (with its `merged_rows` gather) **4.66 s → 4.98 s mean — unmoved**,
  because FU-8e never touched it.
- **`touched_rows` = 1,041,544,686** on merge round 1, **to the row**, in two independent runs
  — the same figure F2 recorded and E2 reproduced. That is the arithmetic proof that the same
  distances were asked for by a path that no longer builds a tree.
- **Flipped rows: 0. Moved uncovered rows: 0. Coverage fallbacks: 0** — across 33 rounds and
  ~1.29 million proposal variants, not one variant needed its own `assess_coverage`. With
  `make_raking_blade`'s 1,402 proposals, the fast path is what runs on both proposal-carrying
  models. **The speedup is model-dependent; the answer never is** — a model whose merges churn
  uncovered geometry falls back to `assess_coverage` verbatim, at the pre-FU-8e cost, never on
  a different objective.
- **Where the other ~142 s went**: ~127 s of pre-gate pipeline plus the proposal-free baseline
  batch, then ~474 s of tournament — **14.4 s** per round end to end, of which the
  `O(contacts²)` proposal gather is **30 ms**.

**THE SUCCESSOR FRAME, NAMED PLAINLY.** Inside the post-FU-8e variant loop the leader is
**`merged_violation_count` (with its `merged_rows` gather) at 4.98 s — 50.1 % of the loop and
36.0 % of the round**; `fill_proposal_nearest_delta` is second (4.44 s, 44.7 % / 32.1 %) and
batch preparation third (3.88 s, 28.1 % of the round). F4 sampled this function at "a steady
~4.6 %" and FU-8e's design predicted ~29 % after the fix; **both are low — it is 36 %**. It is
named so the next phase does not meet it as a surprise, and it is **not** what to fix next
(see FU-8f).

**Rulings, recorded where each bites.** **R-c** (radius-bounded steal): measured and **NOT
adopted** — post-fix it can save at most ~42 % of a 4.44 s frame, **1.9 s of a 13.8 s round
(13.5 %)**, and it saves nothing at all if FU-8f lands; re-measure after the tournament's
shape settles. **R-d** (route-key concentration): **NOT adopted** — its win is amortising a
per-key sweep, and the post-fix row body is 6.30 ns and gather-dominated, so there is no
per-key sweep left to amortise. **R-e** honoured throughout. **R-f** unsatisfied, stated first
and repeated. **R-g** governed the run.

**OD-8 at this tip.** `make_raking_blade` drives 1,402 real merge proposals through every path
E1–E3 rewrote, and all seven frozen digests are byte-identical. **All five gates green at the
final tip.**

### FU-8f opened 2026-08-07 as a RECORDED FINDING — AWAITING OWNER RULING, deliberately not built {#fu8f-2026-08-07}

> **This is a finding, not work in progress. Nothing in it is implemented, and the agent that
> measured it was explicitly not authorised to implement it.** It is recorded here so the next
> phase inherits the measurement rather than rediscovering it, and so the decision it needs is
> visible as a decision.

**The observation.** `evaluate_gate_batch` returns a `vectors` array with one
`FrozenSpecializedGateVector` per variant — **39,224** of them on the treant — and the
tournament consumes it through exactly **two lines**: `let baseline = vectors[0]` and
`first_safe_proposal(&vectors, baseline)`, which is
`vectors.iter().skip(1).position(|v| gate_not_worse(*v, baseline))`. **It is a `position`, not
a scan for a best.** Everything after the first safe index is built and discarded.

**The measurement.** `first_safe_proposal` returned **`Some(0)`** in **every round measured**:
4/4 rounds at this branch's tip in a supplementary run, and 28/28 rounds across two models in
the phase's design pass. The first proposal offered was accepted every single time.

**The arithmetic.** Evaluating lazily — baseline, then variants until one is safe — would take
the round from 13.8 s to about **3.9 s** (batch preparation 3.88 s + two variants of loop work
+ a 30 ms gather), i.e. **~30 min for 460 rounds** against the current ~1.8 h. It is
observationally identical downstream: `SpecializedGateEvidence` retains only `baseline`,
`accepted` and counts, and `evaluated_pair_count` is assigned `total_proposals` — the number
**gathered**, not the number evaluated.

**Why it is not built.** It **severs charged-batch from executed-batch**. FU-8d phase F3
re-founded `GateRoutePlanMismatch` as a plan-vs-execution attestation — that every planned
proposal was executed, once and in order — and lazy evaluation would break that invariant by
construction, requiring the attestation to be re-founded a **second** time on a weaker claim.
Weakening the same attestation twice in two phases is an owner's call, not an agent's.
**AWAITING OWNER RULING.**

**And it is not sufficient on its own.** Getting the treant's gate stage under 600 s needs a
second, independent change: **batch preparation must become incremental across rounds.** It is
3.88 s per round — the 554,926,572-row baseline coverage sweep plus the `GateBatchIndex` —
rebuilt from scratch every round, and **460 × 3.88 s = 1,785 s is already ~3× the box** with
the variant loop at zero. A merge changes **2 contacts of 1,366**; preparation redoes all of
it. Both parts would have to land, and the second is the larger one.

### FU-8f phase G1 landed 2026-08-07 — the load finalize is transposed; one new charged byte term, no operation count moved {#fu8f-g1-2026-08-07}

**Scope, stated first, because this phase is deliberately narrow.** G1 is *the transpose, and
nothing else moves*. It touches `finalize_load_rows` and its resource plan. It does **not**
touch the variant loop, `fill_proposal_nearest_delta`, the `assess_coverage` call sites, or
`first_safe_proposal` / tournament acceptance — those are FU-8e's and phase G3's, after the
merges. The spec is the FU-8f design record of 2026-08-07 (owner ruling in its banner,
Craig 2026-08-07: *"2.1 hours down to 28 minutes seems worth it; anywhere else that we can
also still shave more time off?"*); this section is the in-repo landing of its D2a / D5-G1
content.

**The defect (design record finding F-3).** `finalize_load_rows` was
`for contact { for row { if eligible_owners.binary_search(contact.owner).is_err() { continue } … } }`
— a `contacts × rows` scan that visits every cell to discard most of them. Measured on the
Weeping Treant v1.2's merge-gate batch at `7aa6b73`, in a detached instrumented worktree
(five runs, all scratch reverted, nothing committed):

| counter | value |
|---|---:|
| cells visited (`rows × contacts`) | **160,834,206** |
| cells that are actually eligible | **8,210,643** — 19.6× fewer; **94.9 %** of the visits were the `continue` |
| `Σ\|eligible_owners\|` over rows | **7,988,078**, max **568** on one row |
| load rows / contacts in that batch | 117,741 / 1,366 |

That scan was ~3,090 ms of a 3,899.6 ms batch preparation — **79 % of the round** — and it is
a batch-first defect of the same family as FU-8b, FU-8c and FU-8d.

**The fix.** `finalize_load_rows` builds a **contact-major CSR transpose** of the eligibility
it is handed — self-contained, deliberately *not* reusing the gate's `pair_starts`/`face_row`,
which would couple this phase to the gate — and runs the **identical loop body** over the
eligible cells only.

**Bit-identity is by construction, and it was measured before it was written.** The buckets are
filled by walking rows **ascending** and appending each row to every eligible contact's bucket,
so each bucket comes out ascending with **no sort**; consuming contacts **ascending** therefore
reproduces the old visit sequence cell for cell, so the running argmin (with its
`(distance, contact_id)` tie-break) and `insert_load_witness` see an identical
`(contact_index, distance)` sequence. The in-process replica on the treant's real batch
reported `nearest_bit_identical = true` and `buckets_already_sorted = true` — the defensive
sort the first draft ran was provably a no-op — at a **53×** scan speedup.

**Charged: one new byte term, and nothing else.** `cells × 4 B + (contacts + 1) × 4 B` =
**32,848,040 B** on the treant, a **new** `LoadResourcePlan::scratch_bytes` term — **6.1 %** of
the unchanged 512 MiB `max_load_scratch_bytes`, 3.1 % of the 1 GiB gate peak. **No cap was
raised, lowered or touched. No charged operation count moved.** The operation model is left
standing and **over-stating**: `load_resource_plan_from_counts` still prices the scan as
`rows × contacts × (owner_log + 4)` ≈ 4.34 × 10⁹ where the executed work is now ~10⁸. That is
the FU-8e Q5 precedent — a plan that admits the general path *a fortiori* admits its
specialisation — and repricing is sequenced as its own later phase (G5), after G4's
completed-gate run, because G4's transcript is the last measurement stated in the old numbers.

> **Dated addendum 2026-08-07 — FU-8f phase G5. The paragraph above stands, its last two
> sentences have come due, and its stated precondition was NOT met.** G5 retired
> `rows × contacts × (owner_log + 4)` and replaced it with the transpose's own terms, so
> "no charged operation count moved" is now history: on the treant's gate batch that term goes
> **4,342,523,562 → 207,690,028 + 65,685,144** and the finalizer's whole exact plan goes
> **4,714,113,843 → 644,970,917**. The 32,848,040 B byte term is unchanged **to the byte**.
> **The precondition — "after G4's completed-gate run" — did not happen, and G5 ran anyway.**
> Stated plainly rather than glossed: G4's two attempts are both EXCEEDS-TIMEBOX, so no gate
> was completed, and the precondition belongs to **ruling R-f**, which governs *re-deriving
> the batch-product coefficients from a completed run*. **G5 re-derives neither coefficient.**
> What G5 does is correct a charge that over-stated executed work by ~21× on the term
> concerned; that is a falling demand, it can create no refusal, and it needs no completed
> run to be true. The other half of the sentence *was* honoured: G4 ran first and its
> transcript is deliberately left in the old numbers. Full record: *FU-8f phase G5 landed
> 2026-08-07*.

**One asymmetry, stated rather than left to be discovered.** The term is added in
`load_resource_plan`, the finalizer's own **exact** plan, and **not** in
`specialized_load_resource_plan`, the preflight. The term *is* the exact eligible-cell count,
and every bound a preflight could state without the eligibility (`owner_count × contact_count`,
`rows × contacts`) over-states it by two to three orders of magnitude and would refuse batches
this build admits. So the preflight under-states this finalizer's scratch by the transpose
term. That costs a **later** refusal, never an unafforded allocation: the buffer is allocated
only after the exact check passes, and the exact check is the binding one.

**Tests.** `fu8f_transposed_finalize_reproduces_the_scan_bit_for_bit` compares the production
pass against the **retained `#[cfg(test)]` `contacts × rows` scan** — the replaced
implementation itself, kept compilable as the oracle, the same shape FU-8c and FU-8d used —
row by row on `to_bits`, on the argmin's contact index *and* squared distance and on all three
retained witness slots. Three datasets, all on `make_overhang_table`'s mesh: its own generic
load authority (the **accepted-load** path's shape, 2 cells of 2 — dense by construction and
carried for equivalence, not sparsity); the FU-8d phase F3 load fixture's extended eligibility
(17 cells of 70, 4.1×, where the exact survivor/survivor tie lives); and a wide specialized fan
(50 cells of 1,250, **25×**) that carries the design record's *>10× sparsity* non-vacuity
assertion. **The record asked for that assertion on the two named fixtures and neither can
carry it** — the treant's 19.6× comes from many contacts each eligible for a small fraction of
the rows, a shape those fixtures do not have — so the fan reproduces the shape, and the
assertion is landed rather than quietly weakened.

**Two record notes, so nothing is "corrected" back.**

1. **The 7,988,078 coincidence** (design record open question 8). `GroupedOwnerFaces`'s doc
   comment records 7,988,078 as a **falsified** predicted-row figure, corrected by record fix
   F4. It is *also*, exactly, the measured `Σ|eligible_owners|` over the treant's load rows.
   Same digits, unrelated quantities, both correct in their own sentence. A note now sits at
   that doc comment saying so.
2. **Rows are not uniformly small** (open question 7). One treant row carries **568** eligible
   owners of 1,366 contacts. The transpose handles it identically and nothing assumes short
   buckets.

**Record sites swept in the same commit set** (the addition is dated at every one):
`crates/plinth-core/src/constants/placeholders.rs` (the
`SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES` registry note), `constants.md` **regenerated by
its producer**, `support-resource-caps-2026-08-05.md`, `03-physics-and-constants.md` §3.10.8,
this section, and the `LoadEligibilityTranspose` / `load_resource_plan` / `GroupedOwnerFaces`
doc comments in `perception.rs`.

**No wall-clock figure from this phase is recorded as a landed number.** Other workflows built
and ran on this machine throughout; the millisecond figures quoted above are the design
record's own contended measurements, and the counters and the bit-identity are the proofs.
The phase's speedup is **not** re-measured here — that is G4's job, on a run that completes
the gate.

**Left VERIFIED-NOT-MERGED** on `feature/fu8f-load-transpose`; this branch queues behind the
FU-8e merge, which owns the variant loop.

### FU-8f phase G2 landed 2026-08-07 — the mesh is hashed once per gate stage; no charged number moves {#fu8f-g2-2026-08-07}

**Scope, stated first.** G2 is the design record's D2a′ and touches exactly one thing: *how many
times the gate hashes the mesh*. It moves no charged operation count, adds no byte term, raises
or lowers no cap, and changes no answer. It does not touch anything FU-8e owns (the variant
loop, `fill_proposal_nearest_delta`, the `assess_coverage` call sites) or anything G3 owns
(`first_safe_proposal`, tournament acceptance).

**The defect (design record F-4).** `mesh_fingerprint` walks every triangle. The merge gate paid
for that walk **twice per batch** — once in `finalize_specialized_load_partition`, again in the
`finalize_load_rows` it immediately calls — over a mesh that **cannot change inside a
tournament**, because the tournament merges *contacts* and never touches geometry. Measured on
the Weeping Treant v1.2's 429,120-triangle mesh at `7aa6b73`: 85.1 ms + 81.5 ms = **166.6 ms per
merge round**, contended. After G1 that was ~40 % of the remaining round, which is why the design
record sequenced it immediately after the transpose rather than deferring it with D3.

**The fix.** `VerifiedMeshFingerprint` (`perception.rs`) is the digest computed once and carried
together with the mesh it was computed from. `create_specialized_contact_plan` computes **one**
for the whole specialized-plan build and threads it through `consolidate` → `evaluate_gate_batch`
→ `build_gate_load_state` → `SpecializedLoadBaseline::build` → both finalizers, plus the rare
`variant_load_violation_count_in_full` path, and the accepted plan's own final load finalize
reuses the same one. Per gate round the hash count goes 2 → 0; per plan build it goes
`2 × rounds + 2` → 1.

**What did NOT change, and must not.** The staleness **comparison stays at every site**. This
hoists the hash, not the check: each consumer still compares the threaded digest against its own
`plan.authority.mesh` and still returns `StaleAuthority` when they disagree. *Verify once, thread
the proof* — never *stop verifying*. Nothing anywhere is verified less than it was.

**Why the mesh travels inside the proof.** The consumers need the geometry as well as the digest.
Two separate arguments would admit a proof computed from one mesh being presented alongside a
different one — a fingerprint that attests nothing. `VerifiedMeshFingerprint::mesh()` is the only
mesh a holder can reach through it, so digest and geometry cannot drift apart **by construction
rather than by discipline**.

**The non-gate paths keep their own single hash, deliberately.** `finalize_load_partition` and
the specialized-plan **edit** replay (`derive_child`) are one-shot, outside the gate's per-round
loop; threading a proof into them would move a public edit boundary for no saving. G2's claim is
about the **per-round double hash inside the tournament**, and that is the whole of it. Said
plainly here so a later reader does not mistake the omission for an oversight.

**The test.** `fu8f_hoisted_fingerprint_still_refuses_a_foreign_mesh_at_both_finalizers` pins the
comparison at **both** sites *independently*, by handing each a proof that is honestly computed
and perfectly valid — **for a different mesh**. That is the exact failure a caller mixing two
meshes would produce, and the exact failure a site that stopped comparing would miss. The
positive arms assert the same inputs are admitted under the mesh's own proof, so no refusal
passes for the trivial reason that the inputs are refused anyway.

*Amended 2026-08-07 — the original G2 test pinned the sites by delegation, and was
strengthened.* As first landed, the test asserted `StaleAuthority` from each finalizer with empty
specialized pairs, and the G2 verifier measured that deleting **only**
`finalize_specialized_load_partition`'s own comparison left it green: the call fell through to
the `finalize_load_rows` it invokes, which refused on the same proof. The second assertion was
satisfied by *delegation*, not by the site it named. The test now runs three arms, each
satisfiable only by the site it names, each handed a specialized pair that makes the delegate
unreachable with that site's check gone — and non-vacuity is measured by mutation rather than
argued: deleting `finalize_load_rows`'s comparison fails arm 1 (`None` against
`Some(StaleAuthority)`) while arms 2 and 3 still pass; deleting
`finalize_specialized_load_partition`'s fails arm 2 with
`Some(SpecializedFaceOutOfRange { face_id: 12 })` while arm 1 still passes. The mutation
transcript lives in the test's doc comment.

**The dangerous direction is now covered too.** The original fixture's foreign mesh was
`make_box_default` (12 triangles) against `make_overhang_table` (60) — **smaller**, which is the
harmless side. The specialized site's check is not purely fail-fast in effect: past it,
`extend_load_eligibility` derives its face-range guard from `verified_mesh.mesh()` while
`surface` remains an independent argument, so a **larger** foreign mesh would carry a face-range
bound past the surface it indexes. Arm 3 uses `make_toed_slab` (140 triangles) with a pair
naming face 60 — in range for the proof's mesh, out of range for the surface — and asserts
`StaleAuthority` at both finalizers. Under the mutation it fails by panic, `index out of bounds:
the len is 60 but the index is 60`, at `surface.geometry().centroids()[face_id]`: the
out-of-bounds read is real and reachable, and the comparison's *position* ahead of the range
guard is what prevents it. That ordering is now stated at the code as well.

**Nothing to sweep.** No charged number moves, so no dated addendum is owed at
`placeholders.rs`, `constants.md`, `support-resource-caps-2026-08-05.md` or
`03-physics-and-constants.md` — the every-record-site rule is satisfied by this section and the
`VerifiedMeshFingerprint` doc comment, which carries the same argument at the code.

**No wall-clock figure from this phase is recorded as a landed number.** The 166.6 ms is the
design record's own contended measurement at `7aa6b73`, quoted as the motivation, not re-measured
here; other workflows built and ran on this machine throughout. The proofs are the hash **count**
and the bit-identity of the corpus digests. Re-measurement is G4's job.

**OD-8 at this tip — MEASURED ZERO.** All seven frozen census digests are byte-identical:
`Sphere` 17667468971446251870, `Cylinder` 13310185350867878190, `make_box`
9657051231007502658, `make_overhang_table` 6686599816688114065, `make_toed_slab`
11199296506741562956, `make_raking_blade` 13278215294653054002, `make_thin_fin`
7016743535301716631. `Sphere`/`Cylinder`/`make_box` exercise the **accepted-load** finalize path
this change touches with no tournament at all; `make_raking_blade` is the live merge witness, 21
committed merges over 23 gate batches, every one of which now runs on the threaded proof.

**The Weeping Treant is excluded from this census**, which is why it was invoked per model with
`PLINTH_CENSUS_ONLY` rather than as one 8-model sweep. Stated as observed and no further: a full
sweep started at this tip was still running after ~25 minutes and was cancelled **without a
transcript**, so nothing about where it was is claimed here. That is expected and is not a
finding — G1+G2 shorten batch *preparation*, while the per-round variant loop D1/G3 targets is
still standing (G3 requires the FU-8e merge first), so a long treant run is exactly what this tip
should produce. Running the treant to a conclusion is **G4's job by design**, under G4's timebox,
thread pinning and RSS watchdog. It is not a gate on this phase and was not treated as one.

**Flake observed, not a regression.**
`specialized::tests::full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads`
failed once in a full-workspace run at this tip and passed on immediate re-run and in isolation.
It is a 1/4/16-thread determinism test run while other workflows were saturating the machine.
Recorded rather than silently re-run.

**Left VERIFIED-NOT-MERGED** on `feature/fu8f-load-transpose` alongside G1; the branch queues
behind the FU-8e merge.

> **Appended 2026-08-07 (FU-8f phase G4) — the line above is superseded and is left standing.**
> G1 and G2 are no longer VERIFIED-NOT-MERGED and no longer queue behind anything: they were
> merged into `develop` at **`d711586`** ("Merge FU-8f G1+G2: load-finalize transpose +
> mesh-fingerprint hoist"), after the FU-8e merge at `d17efb6` that the queue was waiting on.
> Every FU-8f phase from G3 onward branches from that merge and therefore carries both. Read
> the closing sentence above as the disposition **at the time G2 was written**, which is what
> this file's append-only convention preserves; the current disposition is *merged at
> `d711586`*. Nothing else in the G1 or G2 sections is affected — no number, digest or claim in
> either moves.

### FU-8f phase G3 landed 2026-08-07 — the gate evaluates a prefix; charged and executed are severed, and both are stated {#fu8f-g3-2026-08-07}

**Scope, stated first.** G3 is the design record's D1, and it is the phase the owner ruled on.
It changes *how many gate variants are evaluated* and nothing else: **no charged number moves,
no cap, floor or coefficient is raised, lowered or touched, no byte term moves, and no answer
changes.** All seven frozen OD-8 digests are byte-identical at this tip, and
`make_raking_blade`'s 21 committed merges over 23 gate batches — every one of them on the new
prefix path — are the live witness for that. It does not touch the load transpose (G1), the
fingerprint hoist (G2), the treant measurement (G4), the load repricing (G5), or cross-round
retention (G6).

**The owner ruling this phase executes**, quoted verbatim because it is the authority for the
attestation trade below:

> *"2.1 hours down to 28 minutes seems worth it; anywhere else that we can also still shave
> more time off?"* — Craig, 2026-08-07

**The defect (design record D1 / FU-8f part (1)).** `evaluate_gate_batch` returned a
`Vec<FrozenSpecializedGateVector>` with one entry per variant — 39,224 on the Weeping Treant
v1.2 — and the tournament consumed it through exactly two lines, `vectors[0]` and
`first_safe_proposal(&vectors, baseline)`. `first_safe_proposal` is
`vectors.iter().skip(1).position(gate_not_worse)`: a **`position`, not a search for a best**, so
nothing after the returned index was ever read. Everything after it was built and discarded.
`first_safe_proposal` returned `Some(0)` in **every round ever measured** — 7 on the treant and
21 on `make_raking_blade` in the design pass, 33 more at the FU-8e tip.

**The fix.** `GateBatchAcceptance` names the two modes; `GateBatchOutcome` replaces the vector
with what the callers actually read.

- `EvaluateAll` — the two proposal-free calls (initial baseline, terminal audit) and the tests.
  Behaviour unchanged; those calls want a vector, not a decision.
- `StopAtFirstNotWorse` — the tournament's merge rounds. The loop breaks at the first variant
  that is `gate_not_worse` than the batch baseline.

The index is the oracle's index **by the definition of `position`**: evaluating in gather order
and stopping at the first hit inspects exactly the entries `position` would have inspected, in
the same order, reading the same predicate on each. `first_safe_proposal` is **retained
`#[cfg(test)]`** as that oracle — the shape FU-8c used for the retained full sweep and G1 for
the retained `contacts × rows` scan — and the prefix rule is proved against it rather than
argued.

**Charging: the charged plan does NOT move, and that is deliberate (design open question 3,
answered NO).** `gate_resource_plan` still prices `plan.variants = proposals + 1`; the
`exact_operations` fold still folds all 39,224; the refusal at `exact_operations >
gate_operation_limit` still runs **before** the loop. The batch is admitted on what it *may*
spend, and it may still spend the whole batch — the no-safe-proposal case does exactly that.
Charging the prefix would be a bound not knowable until after the work that determines it,
which is precisely the hole FU-8d part (3) closed. `plan.vector_bytes` likewise stays charged
for a vector that is no longer materialised (**open question 4, answered NOT IN G3**): moving a
charged byte term alongside a control-flow change mixes two record sweeps, and over-charging
refuses nothing that was not already refused.

**Appended 2026-08-07 (post-verification record):** lazy acceptance also shrinks a **metered
mid-loop refusal**, not only the executed work — `load_delta_limit` is checked inside the
variant loop against the running `executed_load_work + executed_coverage_work`, and under
`StopAtFirstNotWorse` that sum accumulates over the executed prefix only, so a batch that would
previously have refused `GateOperationLimit` at a variant *after* the accepted index now
succeeds; the direction is safe — the prefix's spend is a subset of the full evaluation's spend,
and the limit bounds *executed* spend — and OD-8 measured zero.

**The executed spend is metered, and reported.** `GateBatchOutcome::executed_variant_count`
joins `executed_touched_rows` and `executed_load_work` — the same "state a bound from the plan,
meter the real spend as it runs" shape `load_delta_limit` uses. It is asserted
`executed_variant_count <= plan.variants`, and asserted *exactly*: baseline plus proposals
`0..=accepted` when a proposal is accepted, the whole plan otherwise. **One correction to the
design record, recorded rather than quietly fixed:** the record states the equality case as
"exactly when no safe proposal exists". That is right except at one boundary — when the accepted
proposal is the **last** one, stopping at it and evaluating everything are the same prefix, so
equality holds *with* an acceptance. The shipped assertion states the exact length and covers
both. **Appended 2026-08-07 (post-verification record):** the `executed_variant_count` invariant
is a `debug_assert` (dev/test builds); in release the enforcement is the `GateRoutePlanMismatch`
prefix comparison itself.

**`GateRoutePlanMismatch` is re-founded a second time.** The full text is landed verbatim on the
error variant's doc comment, appended to *FU-8d phase F3 landed 2026-08-06* (whose attestation
it narrows), and in `support-resource-caps-2026-08-05.md` §13. In one line: it now attests
**"the batch executed a prefix of the work it planned — the first `executed_variant_count`
variants, in plan order, each exactly once, at the peak the plan stated"**, and the three
fingerprints are compared **prefix-wise**. What is lost is the attestation that the tail ran at
all, and the tail is exactly what the ruling authorises not running. What is **not** weakened:
the batch is still admitted on the whole batch's charge, so nothing executes that was not first
afforded, and the no-safe-proposal path still evaluates every variant, so **the refusal path's
attestation is exactly F3's** — `GateWholeRoutePlan::executed_prefix` short-circuits to the
whole plan there, which is F3's comparison verbatim.

**Telemetry: two fields, one renamed and one new (design open question 2, answered YES).**
`SpecializedGateEvidence::evaluated_pair_count` → **`gathered_pair_count`**. It has always been
the running sum of `gather_pair_proposals`' output, checked against
`max_merge_total_proposals` — proposals *offered to* the gate, never proposals evaluated. That
mis-statement predates FU-8f and was merely harmless while the gate evaluated everything it was
handed; after G3 the gap on the treant is **39,223 gathered against 1 evaluated**. Beside it,
new **`evaluated_variant_count`**: variants the stage really evaluated, summed over every
`evaluate_gate_batch` call. **Nothing frozen moves with either**: the digest hashes the renamed
field's *value*, not its identifier, and `evaluated_variant_count` is **deliberately not
hashed** — the digest seals the plan the stage *produced*, not how it reached it, and hashing a
meter would make every frozen corpus digest a function of an acceptance optimisation. The
precedent is `coalesced_seat_count`, reported and unhashed. The census re-run below is the proof
that nothing moved.

**Equivalence, in two halves, because the corpus can only prove one of them.**

1. *By construction, on the corpus.* Where `safe = Some(0)` the executed prefix is
   `{baseline, proposal 0}`, byte-for-byte the full batch's first two variants. That covers all
   28 observed rounds. `make_raking_blade` is the live witness — 21 committed merges, digest
   `13278215294653054002` unmoved.
2. *By fixture, for what the corpus cannot supply.* `fu8d_delta_fixture` is **extended** (not
   duplicated) with four lonely-demand pairs: a server sitting on a demand of its own island, so
   the baseline index records it as that demand's only witness, and a partner 40 mm away.
   Merging them puts the seat 20 mm from the demand, outside the attach reach, so
   `variant_unsupported_count` loses the witness and gains nothing — the variant is strictly
   worse by `unsupported_specialized_count`, `gate_not_worse`'s cheapest monotone component to
   move deliberately. Three such proposals sit at gather positions 0–2, the safe one at 3, and a
   fourth worse one at 4 so the prefix is **strictly shorter than the plan**.
   `fu8f_forced_late_safe_proposal_stops_at_the_same_index` pins `first_safe_proposal` and the
   prefix rule to the same index (`Some(3)`), the same accepted vector on `to_bits`, and
   `executed_variant_count == 5` against `plan.variants == 6`. **The design record pins this as
   "4 against 5"; that pairing does not close with its own `k = 3` at positions 0..2** — a safe
   proposal at gather index 3 forces baseline + four proposals = 5 evaluated variants — so the
   shipped pin is 5 against 6, the same claim with the arithmetic corrected.
   `fu8f_a_batch_with_no_safe_proposal_evaluates_every_variant` is the negative twin: all worse,
   `accepted == None`, `executed_variant_count == plan.variants`, the round breaks.
   `fu8f_prefix_acceptance_matches_the_full_evaluation` runs both modes against the oracle on
   FU-8d's own proposals.

**Non-vacuity is measured by mutation, not argued.** Replacing the fold's stop signal with
`false` — "someone reverted the `break`" — fails the forced test at `executed_variant_count`,
`left: 6 right: 5`, and nothing else. Dropping the `accepted.is_none()` first-hit guard leaves
that test green (its fixture has one safe proposal) and fails
`fu8f_prefix_acceptance_matches_the_full_evaluation` at `left: Some(9) right: Some(0)` instead.
Forcing `executed_prefix` to compare against the **whole** plan — i.e. reverting to F3's
comparison — fails **five** tests with `GateRoutePlanMismatch`, among them
`oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing` and
`public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing`. The prefix attestation
is exercised by real corpus geometry on every run, not only by fixtures.

**Correction 2026-08-08 (owed from the FU-8f closer, landed by G6c):** the paragraph above
over-claims. Mutation (a) — replacing the fold's stop signal with `false` — also fails
`fu8f_prefix_acceptance_matches_the_full_evaluation`, at `left: 12 right: 2` via its
`executed_variant_count == index + 2` pin, so "fails the forced test ... and nothing else" is
wrong: mutation (a) is caught by BOTH tests, mutation (b) only by the second. The suite is
STRONGER than this paragraph stated, not weaker; nothing here was re-verified vacuous. The same
correction already stands on `fu8f_forced_late_safe_proposal_stops_at_the_same_index`'s own doc
comment (`specialized.rs`), dated 2026-08-07 at the verification re-run; this is that correction's
landing in the record it was owed to, not a second finding.

**OD-8 at this tip — MEASURED ZERO.** All seven frozen census digests byte-identical, each from
its own `PLINTH_CENSUS_ONLY` invocation of a fresh release `--features census` build: `Sphere`
17667468971446251870, `Cylinder` 13310185350867878190, `make_box` 9657051231007502658,
`make_overhang_table` 6686599816688114065, `make_toed_slab` 11199296506741562956,
`make_raking_blade` 13278215294653054002, `make_thin_fin` 7016743535301716631. Five gates green;
the test count is **989/990 (default / `--all-features`), 0 failed**, against develop's 986/987
— exactly the three tests this phase adds.

**The treant, measured on a QUIET machine — and it is still EXCEEDS-TIMEBOX.** This is stated
here as an observation and **not** as G4's deliverable, which it is not. Conditions: release,
`--features census`, `PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, `PLINTH_THREADS=4`, RSS
watchdog unsandboxed (ruling R-g) armed at 90 GB, 600 s timebox. **No other workflow was running
on this machine** — unlike every FU-8d/FU-8e/FU-8f measurement before it, all of which carried a
contention caveat, so the wall-clock below is a clean number rather than an inflated one.
Outcome, **both permitted attempts: EXCEEDS-TIMEBOX at 600 s, no transcript** — the census prints
its rows when the model finishes, so a killed run yields no counters. Attempt 2 sampled RSS once
a second throughout: **peak 3,479,600 KB (3.32 GiB)**, watchdog armed at 90 GB and **never
fired**, far under the 100 GB machine ceiling. That peak is **higher than FU-8e's 2.446 GiB**
and is recorded as observed rather than explained: G1's transpose is a charged ~33 MB and cannot
account for ~0.9 GB, and the likeliest reading — that this run simply got further into the
tournament than any before it — is a hypothesis, not a measurement, because there are no
counters. Attributing it is **G4's**, and it is flagged here so G4 does not meet it as a
surprise. **What the timebox does and does not say.** It does not
contradict the design's prediction: the D4 table predicts ~3 min at the blade-ratio ~463 rounds
and ~9 min at the 1,365-round hard bound *for the gate stage alone*, while this figure is the
whole model pipeline, and the round count remains **unmeasured** — no run has ever completed the
treant's tournament. It does say plainly that **G3 alone does not put the treant inside the box
on this machine**, and that counting the rounds is now the load-bearing measurement. That
measurement is **G4's, by design**, and re-deriving
`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` remains blocked by **ruling R-f**, which is still
NOT satisfied.

### FU-8f phase G4 measured 2026-08-07 — the rounds are counted at last, and the gate still does not fit {#fu8f-g4-2026-08-07}

**Scope, stated first. G4 changed no engine code at all.** It is a measurement phase: two treant
runs under the timebox, per-round counters from env-gated scratch instrumentation that was
reverted by editing before the gates, and this record. `git diff` over `crates/` at this tip is
**empty**. No cap, floor, coefficient, charged number or byte term is raised, lowered or touched
by this phase; nothing in it could move a digest, and nothing did.

**THE HEADLINE, plainly: the gate did NOT complete, and the answer is still EXCEEDS-TIMEBOX —
but for the first time the rounds are counted, timed and attributed frame by frame.** Both
permitted attempts stopped at the 600 s timebox with the tournament still committing one merge
per round.

**No caveat to declare, for once.** Conditions: release, `--features census`,
`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, `PLINTH_THREADS=4`, RSS watchdog
unsandboxed (ruling R-g) polling every 2 s, armed at 90 GB and **never fired**, 600 s timebox,
two attempts. **The machine was quiet: no concurrent workflow, no other build, nothing else
running.** Every FU-8d, FU-8e and FU-8f measurement before the G3 tip carried a contention
caveat inflating its milliseconds 5–30 %; this section carries none, and its two attempts
reproduce each other to within 2 % round for round. **These are the first uncontended
frame-level measurements of this stage.**

#### G4.1 What the two runs did

| | attempt 1 | attempt 2 |
|---|---:|---:|
| outcome | EXCEEDS-TIMEBOX at 602 s | EXCEEDS-TIMEBOX at 602 s |
| **merge rounds completed** | **436** | **430** |
| contacts, first → last | 1,366 → 931 | 1,366 → 937 |
| proposals gathered, first → last | 39,223 → 28,146 | 39,223 → 28,565 |
| pre-gate wall-clock (process start → gate stage open) | ≈121 s | ≈118 s |
| gate stage wall-clock at the kill | 480.6 s | 483.4 s |
| mean round | 1,099.4 ms | 1,121.4 ms |
| peak RSS | 2,677,888 KB (2.554 GiB) | 2,650,144 KB (2.528 GiB) |
| watchdog | armed 90 GB, never fired | armed 90 GB, never fired |

Attempt 2 carried the extra per-frame instrumentation; the ~2 % round difference is that, plus
run-to-run noise. **One merge commits per round, exactly**, confirmed by `contacts` falling by
one every round in both runs across 866 rounds with no exception.

Sampled per-round table (attempt 1; the full 436 rows are in the run transcript, not committed):

| round | contacts | proposals | gather ms | evaluate ms | round ms | stage s |
|---:|---:|---:|---:|---:|---:|---:|
| 0 | 1,366 | 39,223 | 29.7 | 1,190.4 | 1,220.1 | 2.4 |
| 50 | 1,316 | 39,032 | 29.3 | 1,178.0 | 1,207.2 | 63.7 |
| 100 | 1,266 | 38,398 | 28.9 | 1,148.0 | 1,176.9 | 123.6 |
| 150 | 1,216 | 37,913 | 27.8 | 1,144.3 | 1,172.0 | 180.8 |
| 200 | 1,166 | 37,319 | 27.1 | 1,084.7 | 1,111.8 | 238.1 |
| 250 | 1,116 | 36,046 | 28.4 | 1,107.7 | 1,136.2 | 293.6 |
| 300 | 1,066 | 33,952 | 29.5 | 1,001.8 | 1,031.3 | 347.1 |
| 350 | 1,016 | 32,384 | 29.1 | 949.1 | 978.2 | 398.5 |
| 400 | 966 | 29,885 | 27.8 | 922.3 | 950.1 | 447.5 |
| 435 | 931 | 28,146 | 27.2 | 882.7 | 909.9 | 480.6 |

Least-squares over all 436 rounds: **`round_ms = 304.17 + 0.6924 × contacts`** (R² is not quoted
because the residual is dominated by the systematic decline, not scatter). The round cost falls
because the contact count does; the row count does not move at all.

#### G4.2 D1 delivered exactly what it promised; the D4 projection was wrong somewhere else

Mean over attempt 2's 430 rounds, every frame timed, summing to the measured round:

| frame | ms | % of round |
|---|---:|---:|
| `gather_pair_proposals` (outside `evaluate_gate_batch`) | 28.56 | 2.5 % |
| `unique_island_face_count` | 1.44 | 0.13 % |
| `GateBatchIndex::build` | 6.64 | 0.59 % |
| `plan_gate_nearest_batch` | 9.98 | 0.89 % |
| **`build_gate_load_state`** | **958.37** | **85.5 %** |
| `gate_resource_plan` + `exact_operations` fold | 0.56 | 0.05 % |
| baseline nearest sweep (plan + fill) | 114.39 | 10.2 % |
| **the variant loop — what D1/G3 changed** | **0.52** | **0.046 %** |
| **total** | **1,120.5** | vs 1,121.4 measured |

- **G3 is vindicated beyond its own prediction.** The variant loop was **60.23 s** at `7aa6b73`
  and **10.73 s** post-FU-8e. It is now **0.52 ms** — the design predicted ~1.6 ms. Two to four
  variants are evaluated where 39,224 were planned.
- **G2 is vindicated and is now invisible.** `mesh_fingerprint` does not appear anywhere in the
  frame table because it runs once per gate stage rather than twice per batch: **166.6 ms/round
  → 0**.
- **The D4 "largest unattributed residual" is settled and was over-stated.** `gather_pair_proposals`
  was flagged at "≤40 ms, not isolated, up to 10 % of a 0.39 s round". Measured: **28.56 ms,
  2.5 %** of the real round. It is not a target.
- **The baseline nearest sweep is where the design said it was**: 114.4 ms measured against the
  design's 128.3 ms, unchanged by anything in G1–G3, as intended.

#### G4.3 Where D4 was wrong, exactly: G1's transpose *build*, not its scan

D4 predicted a **0.393 s** round post-G1+G2+G3. Measured: **1.121 s**, 2.85× the prediction.
A bounded sub-frame probe — the same env-gated scratch, 32 rounds, deliberately stopped at 161 s
and **not** a third completion attempt — resolves the whole miss into one place:

| sub-frame of `build_gate_load_state` | measured ms | design's figure |
|---|---:|---:|
| `append_variant_load_inputs` | 13.6 | 12.3 |
| `GroupedOwnerFaces::from_pairs` (sorting 7,971,451 pairs) | 136.0 | 138.3 |
| `extend_load_eligibility` | 17.1 | 21.2 |
| **`LoadTransposeCounts::count` (the CSR sizing pass)** | **406.4** | *not modelled* |
| **`LoadEligibilityTranspose::finish` (the CSR fill pass)** | **410.5** | ~15 (or ~315 "if built naively") |
| **`scan_transposed_load` (the pass G1 existed to shrink)** | **47.4** | 58.3 measured in the replica |
| row fill + contact hash + `FinalizedContactLoads` post-pass | ~5.5 | ~5.0 |
| `face_row`/`row_face` | 0.1 | 0.1 |
| `contact_rows` | 8.1 | 8.3 |

**Every design figure is reproduced within noise except the two the design did not measure.**

- **G1's scan is better than promised.** The pre-G1 `contacts × rows` scan cost ~3,090 ms; the
  transposed scan costs **47.4 ms** — a **65×** fall against the 53× the in-process replica
  predicted. That part of D2a is a complete success.
- **G1's transpose is built twice, in two full passes, for 816.9 ms.** `count` walks the whole
  eligibility structure to size the buffer; `finish` walks it again to fill it. The design
  assumed the build would "ride the existing `contact_rows` walk" at ~15 ms, and warned that a
  naive build would add ~315 ms. It is built naively **and** doubled, so it costs **816.9 ms**.
- **The doubling is not a defect of discipline — the discipline is right.** `count` exists so
  the cells-sized buffer's bytes can be *charged before they are allocated*; that ordering is
  the property FU-8d part (3) established and G1 deliberately preserved. What is wrong is only
  that the count is a second full traversal rather than a quantity the batch already holds.
- **Net, G1 is still a large win on its own frame**: 3,090 ms of scan replaced by 816.9 + 47.4 =
  864.3 ms, a 3.6× fall. It is simply a quarter of what D4 banked, and D4's whole margin was in
  that quarter.

Arithmetic of the miss, stated so it can be checked: predicted 393 ms, measured 1,121 ms, gap
728 ms. Transpose build over budget by 816.9 − 15 = **+802 ms**; transposed scan under budget by
**−11 ms**; variant loop under budget by **−1 ms**; the rest within a few tens of ms of the
design's own figures. **The miss is the CSR build and nothing else.**

#### G4.4 A finding the design said the corpus could not supply: `first_safe` is NOT always 0

The design record's D1 equivalence strategy states that `safe = Some(0)` "covers all 28 observed
rounds" and that a `k > 0` case "is the one thing the corpus cannot supply", which is why phase
G3 built a forced fixture for it. **On the treant that is true only for the first 46 rounds.**

| rounds | `first_safe` | `executed_variant_count` | attempt 1 | attempt 2 |
|---|---|---:|---:|---:|
| 0–45 | `Some(0)` | 2 | 46 | 46 |
| 46 onward | `Some(2)` | 4 | 390 | 384 |

The transition is **sharp, at round 46, and identical in both runs** — 866 rounds of agreement,
with no round anywhere in either run at any other index. Every prior measurement stopped at or
before 33 rounds, which is precisely why `Some(0)` looked universal.

**Nothing about G3 is invalidated, and one sentence of the design is corrected.** The prefix path
is proved against the retained `#[cfg(test)]` `first_safe_proposal` oracle by
`fu8f_prefix_acceptance_matches_the_full_evaluation`, which is an equivalence for every index,
not for index 0; and `fu8f_forced_late_safe_proposal_stops_at_the_same_index` pins exactly this
`k > 0` shape. What changes is the *evidence status*: **the Weeping Treant is a live corpus
witness for `k > 0`, 390 real rounds of it**, and the forced fixture is no longer the only one.
The claim "the corpus cannot supply it" was true of the reachable corpus at the time it was
written — no run had ever reached round 46 — and is recorded here as superseded rather than
quietly dropped. The prefix is still **4 variants of 39,224 planned**, a 9,806× reduction.

#### G4.5 Projections, from measured coefficients rather than modelled ones

Using `round_ms = 304.17 + 0.6924 × contacts` from 1,366 contacts downward:

| | gate stage alone | + pre-gate ≈121 s | D4 predicted (gate stage) |
|---|---:|---:|---:|
| 463 rounds (`make_raking_blade`'s completion ratio) | **504.7 s (8.41 min)** | 626 s (10.4 min) | 182 s (3.0 min) |
| 1,365 rounds (the contact hard bound) | **1,061.6 s (17.69 min)** | 1,183 s (19.7 min) | 536 s (8.9 min) |

**D4 is out by 2.8× at the blade ratio and 2.0× at the hard bound**, and G4.3 says where.
Neither figure fits a 600 s box for the gate stage alone, let alone the model.

**How close attempt 1 came, stated because it is tantalising and must not be over-read.** 436
rounds is **31.9 %** of the treant's 1,366 contacts; `make_raking_blade` completes at **33.9 %**,
which would be 463. The run stopped **27 rounds short** of the blade-ratio prediction, with 28,146
proposals still on offer and a merge committing every round. **Whether the tournament would have
ended at 463 is not measured and is not claimed.** It may end at 463; it may run to 1,365; the
blade ratio is a single 62-contact model's ratio and always was.

#### G4.6 Ruling R-f: STILL NOT SATISFIED, and nothing was re-derived

R-f requires a treant run that **completes** the gate. Neither attempt did.
`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is **not** re-derived here and must not be until a
run completes; that re-derivation remains its own follow-up task, per the design's explicit
instruction. What G4 supplies is the first measured round *count* and *cost*, which is the input
the next phase needs — not the precondition R-f names.

#### G4.7 What did NOT happen, recorded so it is not mistaken for a result

**The pipeline never left the gate stage.** Tournament commit, penetration/brace, the CCO anchors,
attestation and a first treant seal were all **unexercised** — the run is still inside
`prepare_support_run`. So this phase reports **no seal, no digest, no `routed_contacts`, no
`final_live_members`, no per-stage table beyond the gate**, and it makes no claim whatever about
`pen_intervals_one`'s predicted 14–32 GB at `T = 4` or about the brace's `max_scratch_bytes` of
1e11 sitting above the 90 GB watchdog line. Those remain untested predictions.

**No new typed refusal appeared.** Neither run raised any `SpecializedError`; both were killed by
the timebox with the loop healthy.

**No cap came near binding, and none was touched.** `max_merge_iterations` is 100,000 against a
1,365-round hard bound. `max_merge_total_proposals` is **100,000,000**; the cumulative gather was
**15,544,641** over attempt 1's 436 rounds and projects to **≈33.0 M** at the hard bound — a 3×
margin. The stated demand is unmoved at **9,273,857,317** against the unchanged
**35,445,500,736**, because nothing in G4 executes. *(Updated 2026-08-07: that figure is exact
**as of the G4 tip**, and G4 was sequenced before the repricing precisely so this transcript
would be the last measurement stated in the old numbers. Phase G5 then moves it to
**4,525,792,233** against the same unchanged cap. **§ FU-8f phase G4 is not retrofitted** — every
number in it is correct at its own tip.)*

**Peak RSS, and G3's unexplained 3.32 GiB.** The G3 section recorded a peak of 3,479,600 KB
(3.32 GiB) as observed-and-unexplained, and flagged attributing it as G4's. G4 does not reproduce
it: two independent instrumented runs at the same tip peak at **2.554 GiB** and **2.528 GiB**,
both *below* FU-8e's 2.446 GiB neighbourhood rather than 0.9 GB above it, and both far under the
90 GB watchdog and the 100 GB machine ceiling. **This is recorded as a non-reproduction, not as
an explanation.** G1's charged transpose is 32,848,040 B and is present in all three runs, so it
explains neither figure. The honest statement is that the G3 run's peak has not been reproduced
under counters and its cause is unknown; it is not a cap question in either direction.

**Instrumentation hygiene.** All per-round, per-frame and per-sub-frame timers were env-gated
(`PLINTH_FU8F_ROUNDS`, `PLINTH_FU8F_LOAD`), never reached without those variables, and were
removed by editing before the gates. `git diff` over `crates/` at this tip is empty and the five
gates below were run on that tree. The three run transcripts are scratch and are **not**
committed, per the standing rule that run outputs never enter git.

#### G4.8 Gates and OD-8 at this tip

**Five gates green**, verbatim, tests with `--no-fail-fast`: **989 / 990 passed
(default / `--all-features`), 0 failed** — identical to the G3 tip, as it must be, since G4 adds
no test and changes no code. Clippy clean at `-D warnings`, `cargo fmt --all -- --check` clean,
`git diff --check` clean.

**OD-8 — MEASURED ZERO.** All seven frozen census digests byte-identical, each from its own
`PLINTH_CENSUS_ONLY` invocation of a fresh release `--features census` build at this tip:
`Sphere` 17667468971446251870, `Cylinder` 13310185350867878190, `make_box` 9657051231007502658,
`make_overhang_table` 6686599816688114065, `make_toed_slab` 11199296506741562956,
`make_raking_blade` 13278215294653054002, `make_thin_fin` 7016743535301716631.

**An INTERMITTENT FAILURE was seen twice and its name was NOT captured — recorded as an open
item rather than waved away.** Two separate full-workspace `cargo test --workspace --offline
--no-fail-fast` passes exited non-zero with `error: 1 target failed:` `-p plinth-support --lib`,
both while the machine was still hot from the measurement runs. **Neither run's `failures:` block
was retained**, so *which* test failed is unknown, and this section does not guess. What is
known: **four consecutive full-workspace runs after them are clean at 989 / 0**, the
`--all-features` gate has never failed, and
`specialized::tests::full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads` —
the 1/4/16-thread determinism test the G2 section already recorded flaking under load, and the
only known load-sensitive test in that binary — passes in isolation. It is the obvious
suspect and it is **not** identified as the cause, because the evidence to identify it was lost.

**This is opened as a follow-up, FU-27: an unidentified intermittent failure in
`plinth-support`'s lib tests under machine load.** Two sightings in one session is past the
threshold where "flake, re-ran, green" is an adequate record. The first action is instrumentation
of the gate procedure itself — always capture the run to a file so the `failures:` block survives
— not a guess at the test. Until it is identified, **no claim is made that the two sightings and
the G2 sighting are the same defect.**

### FU-8f phase G5 landed 2026-08-07 — the honest load charge; the demand falls 4.75 × 10⁹ and no cap moves {#fu8f-g5-2026-08-07}

**Scope, and the direction, stated first.** G5 changes **no executed work** and **no
geometry**. It changes one **charge**: the operation model that prices the load finalizer's
nearest-and-witness scan. Phase G1 had already replaced the dense `contacts × rows` scan with
a contact-major CSR transpose and had **deliberately left the model standing and
over-stating**, with the repricing sequenced here by design. This phase pays that debt.
**No cap, floor or coefficient is raised, lowered or touched, and none is re-derived.**

**What was retired, and what replaced it.** The term
`face_count × contact_count × (owner_log + 4)` — one eligibility binary search per
`(row, contact)` cell, which is exactly what the pre-G1 code did — is gone. In its place, in
the same elementary-comparison unit the rest of the model counts:

- `Σ|eligible_owners| × (log + 2) × 2` in `load_resource_plan_from_counts` —
  `LoadTransposeCounts::count` and `::finish` each walk the row-major eligibility once,
  running one forward `partition_point` over the contact order per `(row, eligible owner)`
  entry. **This is the term that replaces the quadratic**, and its shape is the point.
- `contact_count × (log + 6)` — the contact-order sort's comparison bound (`log + 2`, already
  carried) plus the transpose's four contact-sized passes.
- `cells × (LOAD_WITNESS_SLOTS + 5)` in `LoadTransposeCounts::operations`, added by
  `load_resource_plan` — 1 per cell for the fill's store and cursor bump, 4 for the distance
  and the argmin comparison (precisely the allowance the retired `owner_log + 4` carried for
  everything that was *not* the binary search), and `LOAD_WITNESS_SLOTS` for
  `insert_load_witness`, which FU-8d had charged *inside* that binary-search allowance.
- `Σ|eligible_owners| × owner_log × 2` and `face_count × (face_log × 2 + 1)` are **unchanged**;
  they price the eligibility's own ordering and the row emit, neither of which is the scan.

**The `cells` term sits in the exact plan, not the preflight — the same asymmetry G1
established for the transpose's bytes.** `cells` is knowable only after
`LoadTransposeCounts::count` has walked the eligibility, so `specialized_load_resource_plan`
under-states this finalizer's operations by that term exactly as it already under-states its
scratch bytes. **That costs a later refusal, never unafforded execution**: the fill and the
scan run only after `finalize_load_rows`'s own exact check, and that check carries the term.

#### G5.1 The treant's batch header, old against new

Conditions: release, `--features census`,
`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2" PLINTH_THREADS=4`, RSS watchdog
unsandboxed (ruling R-g) polling every 2 s, armed at 90 GB and **never fired**. **The machine
was quiet — no concurrent workflow**, as it was for G4. One instrumented release run, stopped
at 260 s once the gate's first two batches had printed; the instrumentation was env-gated
(`PLINTH_G5`), never reached without it, and removed by editing before the gates.
**This was not a completion attempt and is not offered as one.**

Round-1 merge-gate batch — samples 405,146, demands 1,095, contacts 1,366, proposals 39,223,
variants 39,224:

| term | before G5 | after G5 |
|---|---:|---:|
| `baseline_rows` | 554,926,572 | 554,926,572 |
| load baseline (`specialized_load_resource_plan`) | **5,543,831,115** | **795,766,031** |
| load proposal bound | 1,048,982,627 | 1,048,982,627 |
| **gate `load` term** | **6,592,813,742** | **1,844,748,658** |
| `demand_rows` | 42,949,185 | 42,949,185 |
| proposal work | 2,083,167,818 | 2,083,167,818 |
| **stated demand** | **9,273,857,317** — 26.2 % | **4,525,792,233** — **12.8 %** |
| `exact_operations` | **10,457,910,996** — 29.5 % | **5,709,845,912** — **16.1 %** |
| **cap — UNCHANGED** | 35,445,500,736 | 35,445,500,736 |

**The fall is 4,748,065,084 at both sites, to the unit, and it is entirely the load term.**
Every other line is byte-identical, which is the check that the repricing touched what it
claimed to touch and nothing else. Arithmetic, so it can be verified rather than believed: at
the 134,368-row preflight the retired term was `134,368 × 1,366 × 27 = 4,955,760,576`; what
replaces it is `7,988,078 × 13 × 2 = 207,690,028` plus the `1,366 × 4 = 5,464` contact-linear
widening. Net **−4,748,065,084**, exactly.

Two further figures from the same run:

- **The accepted-load path**, 130,548 rows: **5,402,535,619 → 795,359,775**, a 6.79× fall.
  This is the figure `TREANT_LOAD_OPERATIONS_REQUIRED` pins.
- **The finalizer's own exact plan**, 117,741 rows / 1,366 contacts / `Σ|eligible_owners|`
  7,988,078 / **8,210,643 cells**: **4,714,113,843 → 644,970,917**, of which 207,690,028 is
  the search term, 65,685,144 the cells term, and 371,595,745 the two untouched terms. So the
  **scan** term falls **21×** while the **total** falls 7.3× — a gap stated rather than
  smoothed over, because what is left is dominated by
  `Σ|eligible_owners| × owner_log × 2 = 367,451,588`, which is the eligibility's ordering and
  not the scan.

#### G5.2 The corpus — and the one place the demand does NOT fall

Every reachable corpus model was re-measured under the same instrumentation, one
`PLINTH_CENSUS_ONLY` invocation each. **Every model's widest load demand falls**: `Sphere`
86,709 → 61,481, `Cylinder` 42,078 → 26,509, `make_box` 658 → 445, `make_overhang_table`
11,690 → 3,354, `make_toed_slab` 10,704 → 3,291, `make_raking_blade` 12,426 → 5,696,
`make_thin_fin` 595 → 480.

**"A falling demand can create no refusal" is true of every model's widest demand and of every
treant site, and it is NOT literally true checkpoint by checkpoint — recorded rather than
glossed.** On `make_box` and `make_thin_fin` three individual load checkpoints **rise**, by at
most 56 units: `make_box` 409 → 445 at (3 rows, 11 contacts, 8 owner entries, 20 cells) and
`make_thin_fin` 424 → 480 at (2 rows, 18 contacts, 6 owner entries, 19 cells). The cause is
the new model's *shape*: it is `Σ|eligible_owners|`- and `cells`-shaped, and on a three-row
batch those counts are large relative to `rows × contacts` in a way they never are on real
work. Every one of those checkpoints is more than four million times under the
**2,000,000,000** flat floor.

#### G5.3 The finding: the treant no longer leaves the load floor

`SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS` is a flat floor of **2,000,000,000** lifted by
`max(floor, 64 × (rows + 1) × (contacts + 1))`. The Weeping Treant was **the only model that
ever left that floor**. At **795,359,775** it no longer does, so the batch-product scaling at
the load stage is now load-bearing for nothing in the corpus.

**It is kept, unchanged, and that is a decision rather than inertia.** Applying the derivation
rule to the repriced observation gives a coefficient of 8 where the registered value is 64 —
a cap **reduction**, which is the direction that creates refusals. FU-23/R4 trigger
re-derivation on a **second specimen**, not on the same specimen getting cheaper, and G5's
mandate is that no cap is touched. `crates/plinth-support/tests/resource_cap_policy.rs` now
pins `TREANT_LOAD_OPERATIONS_REQUIRED` (the 2026-08-06 derivation input, retained) and
`TREANT_LOAD_OPERATIONS_REPRICED` (what the treant now asks) side by side, and asserts the
second falls, is admitted, and sits below the floor.

#### G5.4 Open question 4 — `plan.vector_bytes` — deliberately LEFT

The design record's open question 4 asked whether `plan.vector_bytes` should shrink under
`StopAtFirstNotWorse`, answered "not in G3; move the charge in G5 if at all". **G5 leaves it,
on a distinction rather than a deferral.** G5's subject is a model that was *always* an
over-statement of what the code did — a modelling error, corrected. `vector_bytes` is not
that: it is a **charged batch bound**, the byte twin of `plan.variants`, and open question 3
already settled the principle — *charge the batch, meter the prefix, state both*, because a
bound not knowable until after the work that determines it bounds nothing. Shrinking it to the
executed prefix would re-open that hole on the byte side.

**What is left undone, named so it is not mistaken for settled:** `check_gate_peak` reports
`result_bytes: plan.vector_bytes` as the **executed** peak, which now over-states executed
bytes because the vectors are never materialised. Metering the executed prefix's real bytes
beside the charged bound — D1's "state a bound, meter the spend", applied to bytes — is a
**follow-up**, and a byte-term record sweep of its own.

#### G5.5 What this licenses, what it does not, and what was observed

- **Licensed:** reading the treant's merge gate as sitting at 12.8 % of its cap rather than
  26.2 %, and the load stage as no longer needing its batch-product scaling.
- **NOT licensed:** re-deriving either coefficient. **Ruling R-f is STILL NOT satisfied** —
  G5 ran no completion attempt, and its measurement run was stopped at 260 s having reached
  the gate stage, which is not a completed tournament. G4's two attempts remain the round
  record, both EXCEEDS-TIMEBOX at 602 s.
- **NOT licensed:** reading any of this as a wall-clock improvement. G5 changes no executed
  work; the treant's stopping point is unchanged and still wall-clock.
- **G4's transcript is deliberately left in the OLD numbers**, which is why G5 was sequenced
  after it. Nothing in the G4 section is retrofitted.

**Memory, observed and not explained.** The G5 run peaked at **3,538,624 KB (3.375 GiB)** at
≈154 s, shortly after the gate stage opened, falling back to ≈2.4 GiB for the remainder. That
is a **third** data point beside G3's unreproduced 3.32 GiB and G4's 2.554/2.528 GiB, and it
is recorded as an observation only: G5 moves no byte term, the transpose's charged 32,848,040 B
is present in all four runs and explains none of the spread, and the honest statement is that
a short-lived transient near gate-stage entry has now been seen twice under 2 s sampling. It
is 26× under the 90 GB watchdog and 29× under the 100 GB machine ceiling, and it is not a cap
question in either direction.

**Gates and OD-8 at this tip.** Five gates green, verbatim, tests with `--no-fail-fast`:
**990 / 991 passed (default / `--all-features`), 0 failed**, against the G4 tip's 989/990 —
exactly the one test this phase adds
(`fu8f_g5_load_charge_is_no_longer_quadratic_in_rows_times_contacts`). **All seven frozen
census digests byte-identical**, each from its own `PLINTH_CENSUS_ONLY` invocation of a fresh
release `--features census` build: `Sphere` 17667468971446251870, `Cylinder`
13310185350867878190, `make_box` 9657051231007502658, `make_overhang_table`
6686599816688114065, `make_toed_slab` 11199296506741562956, `make_raking_blade`
13278215294653054002, `make_thin_fin` 7016743535301716631. That is the check that matters for
this phase: **repricing changes charges, never geometry.** Full derivation:
`support-resource-caps-2026-08-05.md` §15.

### FU-8f phase G6a landed 2026-08-07 — the retention carrier and its four guards, rebuild-always; no charged number moves {#fu8f-g6a-2026-08-07}

**Scope, stated as a limit first.** G6a lands the *plumbing* for cross-round retention of the
merge gate's load transpose and nothing else. Every round still executes the **full build**
(rebuild-always), so the phase is **byte-identical in behavior by construction** — there is no
fast path to be wrong. What it lands is the carrier, its lifecycle, and the four guards that
will police the fast path when G6b flips it in, each **mutation-proven** before it is trusted.
Design record: [`fu8f-g6-design-2026-08-07.md`](fu8f-g6-design-2026-08-07.md), landed verbatim
on this branch with a provenance banner. **No cap, floor or coefficient is raised, lowered or
touched. No charged number moves.**

**What exists now.** `RetainedLoadTranspose` in `perception.rs`: an append-only arena of
bucket payloads plus per-contact `(offset, len)` refs, the merge-invariant row invariants
(`face_id`, `area_mm2`, `centroid_mm`), a `(owner, point_mm)` contact witness, and the mesh
digest the build ran under. `consolidate` owns exactly one `Option<RetainedLoadTranspose>` on
its stack, threads it as `&mut` through `evaluate_gate_batch` into `build_gate_load_state`, and
drops it on return — **retention never escapes the tournament**, and every non-gate load
finalize is untouched. `record_commit` synchronizes it after each committed merge by an
**identity walk** over positions on bits, never by comparator re-derivation; anything other
than "survivors in preserved relative order plus exactly one insertion" **poisons** the carrier
rather than guessing, and the next round pays one counted rebuild.

**The four guards, and the measurement that says they work.** Census gains
`retained_load_rounds` / `retained_load_rebuilds` / `retained_load_rebuild_<cause>` on the gate
row. A guard miss is never silent. Measured on the corpus at this tip
(`PLINTH_CENSUS=1`, release, `--features census`):

| model | gate batches | retained rounds | counted rebuilds | cause |
|---|---:|---:|---:|---|
| `Sphere` | 2 | 2 | 0 | — |
| `Cylinder` | 2 | 2 | 0 | — |
| `make_box` | 2 | 2 | 0 | — |
| `make_overhang_table` | 2 | 2 | 0 | — |
| `make_toed_slab` | 2 | 2 | **1** | `contact_witness` |
| `make_raking_blade` | 23 | 23 | **1** | `contact_witness` |
| `make_thin_fin` | 2 | 2 | 0 | — |

**`make_raking_blade` is the live witness, and it is a stronger one than G6a needed.** Because
this phase rebuilds unconditionally, the *only* consumer of `record_commit`'s remap is the
**next** round's guard chain — so a wrong remap could not corrupt geometry, it could only show
up as a counted rebuild. It did not. Across 23 gate batches (one initial proposal-free
evaluate, **21 committed merges**, one terminal audit) the remapped witness matched
`append_variant_load_inputs`'s own freshly-derived `(owner, point_mm)` list on bits **every
time**, and every specialized bucket length matched the gate's own `contact_rows` every time.
That is 21 consecutive committed merges of positional contact/owner renumbering reproduced
exactly, on a real model, as a *counter* rather than an argument.

**The one rebuild on each of two models is the predicted one, not a defect.** Its cause is
`contact_witness`, and it falls on the **terminal audit** — `coalesce_exact_regular_seats`
mutates contacts after the tournament loop and behind the retention slot, exactly the hazard
design finding F-G6-4 named and exactly the reason guard 2 is written as *equality against the
fresh list* rather than as a round counter that trusts callers. The design predicted this site
in advance and declined to teach `record_commit` about coalescing (open question 4); the
measurement confirms the prediction and the cost is one rebuild per tournament.

**A design error was found and corrected at implementation — recorded, not silently fixed.**
Design finding F-G6-5 states that `build_gate_load_state`'s `contact_rows[c]` is "each
contact's eligible-row count", making guard 4 a total comparison. **It is not.**
`contact_rows[c]` is derived from contact `c`'s own `(owner, face)` run, and
`append_variant_load_inputs` emits such pairs *only* on the branch that resolves a contact
specialized — a generic seat contributes an empty run, while its transpose bucket is not empty
at all, because eligibility is a predicate on the `LoadOwner` and a generic island owner is
eligible for the generic authority's rows. Taken literally, guard 4 would have reported a miss
on **every round of every batch carrying a generic seat** — which is every real gate batch,
since the finalizer refuses with `MissingPhysicalContact` unless the generic authority's rows
have holders — and the rebuild counter would have fired always, destroying the very
measurement above. Guard 4 is therefore **restricted to the specialized contacts**, which
keeps every part of F-G6-5 that is true (still `O(contacts)`, still reading a `contact_rows`
the round computes for `proposal_load_bound` regardless, so still **zero added derivation**)
and still covers the contacts the update acts on. Restoring totality would require deriving
each generic island's eligible-row count fresh — an `O(rows × owners)` walk over the row-major
eligibility, precisely the walk G6 exists to delete — so it is **declined on the design's own
terms**. The deviation, its justification and both halves of it are recorded in full on
`RetainedLoadTranspose::cardinality_rebuild_cause` and pinned by
`fu8f_retained_transpose_rebuilds_when_a_bucket_length_drifts`, whose positive control is an
**independent** derivation of `contact_rows` — which is what caught this. The design file
itself is left standing verbatim, per the append-only rule.

**Non-vacuity: the guards are measured, not argued.** Six deliberate mutations were applied at
this tip, each run, each required to be caught, transcripts recorded in the named test's doc
comment: **M1** delete guard 1's fingerprint compare; **M2** delete guard 2's witness compare;
**M2b** replace guard 2's bit comparison with `==` (fails the `-0.0` arm alone, which is that
arm's whole job); **M3** delete guard 3's merged-resolves-specialized refusal; **M4** delete
guard 4's cardinality compare; **M6** insert the merged bucket at `p + 1` — the G6a-reachable
member of the design's mutation triad, member (a). **All six failed the named test.** Also
proven: **M5**, zeroing the rebuild counter's increment, fails
`fu8f_retained_transpose_counts_every_guard_miss_as_a_rebuild`, so "a guard miss is a *counted*
rebuild" is itself pinned. Stated honestly against overclaim: the terminal `survivor.is_some()`
check in the identity walk is **defense in depth and is not separately reachable** — with the
list length already pinned to "old minus one", a list leaving a survivor unmatched must also
diverge twice, which the second-divergence arm refuses first — and it is recorded as unproven
rather than claimed as proven. Triad members (b) and (c) act on the retained fast path, which
does not exist until G6b, and land with it.

**Verification.** Five gates green at this tip, tree clean before and after: **998** tests
default (base `b0324fd` 990) and **999** `--all-features` (base 991), 0 failed — the **+8** in
both is exactly the eight new tests this phase adds, attributed one for one. Clippy clean with
`-D warnings`, `cargo fmt --all -- --check` clean, `git diff --check` clean. **All seven frozen
OD-8 census digests byte-identical**: `Sphere` 17667468971446251870, `Cylinder`
13310185350867878190, `make_box` 9657051231007502658, `make_overhang_table`
6686599816688114065, `make_toed_slab` 11199296506741562956, `make_raking_blade`
13278215294653054002, `make_thin_fin` 7016743535301716631 — together with `routed_contacts`
and `final_live_members` unmoved on all seven. That is the check that matters for a phase whose
whole claim is that it changed nothing.

**Two debts carried forward deliberately, so neither is discovered later.** (1) **Held bytes
are not yet charged.** Retention extends the transpose's lifetime across rounds (~66 MB arena
ceiling on the treant, plus the row invariants), and `gate_load_state_bytes` does not yet
carry it — D-G6-4 sequences that into **G6c**, with G6a's rebuild-always meaning the stated
demand is otherwise unchanged. No cap is approached and none is touched, but the stated peak
currently understates the held peak by the retained term, and that is a known gap, not an
oversight. (2) **No wall-clock number is recorded here at all.** Concurrent workflows were
running on this machine throughout, so per the standing rule only counters and bit-identity are
landed. G6a is expected to be *slower* than base, not faster — it does the full build **and**
repopulates the carrier — and the treant's behavior at this tip is **not** measured or claimed.
**G6d is the measuring phase**; nothing in this section may be read as a performance result.

**A third debt, found by adversarial re-verification of this phase after it was written, and a
hard precondition for G6b: the identity walk keys on positions alone, and positions are not a
key.** `record_commit` derives the insertion point `p` by walking the old witness against the
new contact list comparing **only** `position_bits`. But `compare_contacts`
(`specialized.rs:7683`) orders by position *and then* by four tie-breakers — primary-regular
identity, regular routes, normal, demands — so two **distinct** contacts may legitimately carry
bit-identical `position_mm`. If the committed seat's position coincides on bits with the
survivor it sorts against at `p`, the greedy walk consumes that survivor at slot `p`, then
accepts the survivor's own later slot as the insertion (its bits equal the committed position by
assumption), and derives **`p + 1` instead of `p`** — silently, without poisoning. Neither
guard necessarily catches it: guard 2 compares the *witness sequence*, which is identical under
the swap precisely because the two positions agree on bits and owners are recomputed
positionally; guard 4 catches it only when the two buckets' lengths happen to differ. This is
the natural-path twin of mutation **M6**, which the test suite catches only because M6 is
injected.

**In G6a this is provably harmless and is not a defect in what landed** — rebuild-always means
the *only* consumer of the remap is the next round's guard chain, so a mis-derived `p` can
produce a counted rebuild and nothing else; geometry cannot move, which is why all seven digests
are byte-identical above. It is recorded here because **G6b's fast path would consume the remap
directly**, and there it would serve wrong buckets. It was **not observed** on the corpus: every
one of `make_raking_blade`'s 21 committed merges remapped exactly, so this is a latent hazard
argued from the comparator, not a measured failure. **G6b must close it before flipping the fast
path in** — the cheap fix is to make the walk's survivor test the same total identity
`compare_contacts` uses (or to carry a stable per-contact identity through the walk) rather than
position bits alone, and to add a fixture with two tie-broken contacts sharing a position.

### FU-8f phase G6b landed 2026-08-07 — the retained fast path; the gate stops rebuilding what a merge did not change {#fu8f-g6b-2026-08-07}

**What flips.** A gate round every guard accepts no longer executes the full load build. It
runs `RetainedLoadTranspose::finalize_retained_round` instead: the **same** scan loop body
(`scan_transposed_load_core`, now shared verbatim between the fresh CSR and the retained
arena — one implementation, two monomorphizations, so bit-identity travels with the code
rather than with an argument), the `finalize_load_rows` prologue and tail verbatim (staleness
comparison, finite/duplicate contact refusals, row fill from the retained
`row_face_id`/`row_area_mm2`, contact FNV hash), and the **same** baseline post-pass
(`SpecializedLoadBaseline::assemble`, extracted from `build` so both paths run one code path
— carried areas re-summed fresh in the fold's own order every round, never adjusted
incrementally). What falls off retained rounds entirely, per design findings F-G6-2/F-G6-3:
`GroupedOwnerFaces::from_pairs`, `extend_load_eligibility`, and both
`LoadTransposeCounts` passes. `face_row`/`row_face` derive from the retained `row_face_id`
(identical by row-set invariance), `contact_rows` from the same fresh pair walk as before,
and guard 4 runs against them **before** the branch is taken. Any guard miss is the counted
full rebuild, unchanged. `record_commit` gains the D-G6-2 step-4 **arena compaction** at the
2x-waste trigger (one O(cells) copy in bucket order, amortized; trigger and preservation
pinned by `fu8f_retained_arena_compacts_at_twice_the_live_cells`).

**Charging is deliberately untouched, and stated so.** Retained rounds still charge the full
build's own figures (`specialized_load_resource_plan` over the fresh pair counts) — D-G6-4's
"charge the admissible worst case", because no plan can know at plan time whether a guard will
miss. The full build's *internal* resource refusals do not run on retained rounds, and the
soundness argument is recorded on `finalize_retained_round` itself: the work a retained round
executes is a strict subset of what the tournament's first build (or last guard-miss rebuild)
stated and was admitted for — cells, rows and contacts only shrink under committed merges —
the FU-8e Q5 a-fortiori shape; every guard-miss round re-enters the full path with every check
intact. **No cap, floor or coefficient is raised, lowered or touched; no charged number moves.
G6c lands the retained-round metering, the arena byte ceiling and the record sweep.**

**The G6a position-key hazard is CLOSED, by refusal rather than by re-derivation — a recorded
deviation from the record's suggested fix.** The paragraphs above proposed making the walk's
survivor test the comparator's total identity. That would require the witness to retain
per-contact route/normal/demand state it does not hold, for a case never observed on any
corpus commit. What landed instead: the walk now **detects** the ambiguity — committed
position bits equal to the survivor's at a slot where the insertion is still unplaced, which
is exactly the condition under which "survivor" and "committed seat" are indistinguishable
from position bits — and **poisons**, one counted rebuild, never a guess. The fast path can
therefore never consume a mis-derived `p`. Pinned both ways by
`fu8f_retained_commit_poisons_on_a_position_bit_tie_at_the_insertion_point`: both sort orders
of the tied pair poison, a distinct position does not; mutation-proven (refusal deleted, both
tied arms fail, positive arm keeps passing).

**The equivalence proof, landed as specified.**
`fu8f_retained_transpose_matches_a_fresh_rebuild_after_n_commits` drives the carrier through
**four committed merges on real fixture geometry** (`make_overhang_table`, the shared
retention fixture) with **no repopulation between rounds** — the carrier crosses all four on
`record_commit` alone, which is the retention claim itself. After every commit the retained
path's full output is compared on `to_bits`, field by field — transpose buckets (payload and
order, against the fresh CSR), all three witness slots (contact and distance bits), partition
rows (`face_id`, `contact_id`, `owner`, `carried_area_mm2` bits), `contacts_fingerprint`,
`winner`/`won_starts`/`won_rows`, per-contact `carried_area_mm2` bits, `violation_count`, row
invariants — against BOTH the production fresh build AND the retained
`reference_contacts_x_rows_scan` oracle (the pre-FU-8f `contacts x rows` scan itself). Per
verifier finding **F-3** (the treant's `safe = Some(0) -> Some(2)` transition at round 46),
the schedule merges parents at **nonzero** indices ((2, 4) and (1, 3)), inserts at strictly
interior positions, exercises real dedup unions, and merges already-merged seats so retained
payloads feed later merges. The **post-sort renumbering arm**
(`fu8f_committed_renumbering_matches_append_variant_load_inputs`, in `specialized.rs`) runs
the production commit shape end to end — demand-carrying seats, `commit_merge`,
`consolidate`'s own splice-and-sort, the guard-3 predicate — and pins the identity-walk `p`
and remapped witness against the **actual** `append_variant_load_inputs` output over the
committed list, with the merged seat strictly interior: the *committed* renumbering, distinct
from FU-8d's merged-LAST *variant* pin, and both stay pinned.

**The mutation triad — applied, caught, reverted; transcripts in the harness doc comment.**
(a) merged bucket inserted at `p + 1`: caught by **guard 2** (the remapped witness diverges
from the fresh physical list) — the harness's "no guard fired" assertion fails with
`Some(ContactWitness)`. (b) `right`'s bucket left in place: caught by **guard 2's length
arm**, same failure shape. (c) last row dropped from the merged bucket: caught by **guard 4**
— `Some(BucketCardinality)`. All three failed the harness; a harness green under any of them
would be vacuous and did not land.

**The live witness: the retained path now actually retains, and geometry does not move.**
Census at this tip (`PLINTH_CENSUS=1`, release, `--features census`, one
`PLINTH_CENSUS_ONLY` invocation per model):

| model | rounds | counted rebuilds | retained-path rounds (rounds − first build − rebuilds) |
|---|---:|---:|---:|
| `Sphere` | 2 | 0 | 1 |
| `Cylinder` | 2 | 0 | 1 |
| `make_box` | 2 | 0 | 1 |
| `make_overhang_table` | 2 | 0 | 1 |
| `make_toed_slab` | 2 | **1** (`contact_witness`) | 0 |
| `make_raking_blade` | 23 | **1** (`contact_witness`) | **21** |
| `make_thin_fin` | 2 | 0 | 1 |

`make_raking_blade`'s **21 committed merges all rode the retained path** — under G6a these
same 21 rounds each executed a full build; now they execute none — and its end-to-end digest
is `13278215294653054002`, **byte-identical**. The no-merge models bank the design's promised
free round: the terminal audit reuses the initial build when the coalesce changed nothing,
and `make_toed_slab`'s coalesce-perturbed audit takes the predicted `contact_witness` rebuild
(F-G6-4), exactly as in G6a. **All seven frozen OD-8 digests byte-identical**: `Sphere`
17667468971446251870, `Cylinder` 13310185350867878190, `make_box` 9657051231007502658,
`make_overhang_table` 6686599816688114065, `make_toed_slab` 11199296506741562956,
`make_raking_blade` 13278215294653054002, `make_thin_fin` 7016743535301716631, with
`routed_contacts` and `final_live_members` unmoved on all seven.

> **[Dated addendum 2026-08-08 — the retention table above is a `develop` `0f24b79`
> measurement and four of its rows MOVE at the FU-10 phase-C cutover. The table is left
> standing exactly as measured; this note records what re-measurement at the
> `feature/slicer-union` tip (`b93b95f`) returns, and why.** Re-run model by model under
> `PLINTH_CENSUS=1` + `PLINTH_CENSUS_ONLY`, `make_raking_blade` now reports
> **`retained_load_rounds` 23 → 5**, with `retained_load_rebuilds` **1** and
> `retained_load_rebuild_contact_witness` **1** — the `contact_witness` half reproduces
> **exactly**, and so does `make_toed_slab`'s (2 rounds, 1 `contact_witness` rebuild). The
> other six models' round counts are unmoved at 2.
>
> **This is not a retention regression; it is the cutover's arithmetic, and it reconciles.**
> The table's 23 rounds decompose as 21 committed merges + 1 proposal-free batch + 1 terminal
> audit. At the re-pinned tip the decomposition is **3 + 1 + 1 = 5**, and the 18 merges that
> disappeared are the ones the union now performs upstream: `make_raking_blade`'s rake tooth
> was a *separate* solid re-born on each of print layers 90–109 under the legacy stitcher, and
> the merge gate spent a round per layer reuniting it with the body. Phase C fuses it in the
> per-layer union — the 0.029886 mm-deep tooth-body fusion the owner ruling accepted on
> 2026-08-08 under the PR-1(a) sub-layer precedent — so those rounds have no work left to do.
> The contact movement recorded at [`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)
> (`27 / 72`, was `32 / 89`) is the same fact counted at the contact level.
>
> **The G6 property the table was written to prove is preserved exactly.** Retained-path
> rounds are `5 − 1 first build − 1 rebuild = 3`, which is precisely the 3 committed merge
> rounds: **every committed merge round still rides the retained path**, as the 21 did. The
> machinery is alive post-cutover — the retained path is exercised and F-G6-4's predicted
> terminal-audit `contact_witness` rebuild still fires on both models that took it.
>
> **What is honestly weaker: the retention path's corpus exercise shrinks with the round
> count** — 21 retained rounds → 3 on the only model that ever had more than one. That is the
> same class of coverage reduction as [FU-30](#follow-ups), recorded here rather than opened
> as its own row because no behaviour changed and the treant (823 rounds, 804 retained) is the
> load-bearing witness for this machinery, not `make_raking_blade`. No cap, floor or
> coefficient is touched, and no G6 code is changed by the cutover.]**

**Verification.** Five gates green at this tip, tree clean before and after: **1002** tests
default (G6a tip 998) and **1003** `--all-features` (G6a tip 999), 0 failed — the **+4** in
both is exactly this phase's four new tests (the harness, the position-bit-tie refusal, the
compaction pin, the committed-renumbering arm), attributed one for one. Clippy clean with
`-D warnings`, `cargo fmt --all -- --check` clean, `git diff --check` clean. No wall-clock
number is recorded anywhere in this section: concurrent workflows were running on this
machine throughout, so counters and bit-identity are the only landed measurements.
**G6d remains the measuring phase**; the ~741 ms/round removal predicted by D-G6-5 is a
prediction until G6d measures it.

### FU-8f phase G6c landed 2026-08-08 — charging honesty under retention; the meter beside the charge, never instead of it {#fu8f-g6c-2026-08-08}

**What lands.** Charging itself is UNCHANGED from G6b: every gate round is still charged the
full build's own figure (`baseline_resource.operations` from `specialized_load_resource_plan`),
the admissible worst case, because no plan can know at plan time whether a retention guard
will miss (D-G6-4, the G3/FU-8d charged/executed severance shape carried over). What G6c adds
is the OTHER number, stated beside the charge and never fed back into it:

- **`GateLoadState` gains `executed_load_build: ExecutedLoadBuild` (`Rebuilt` | `Retained`) and
  `executed_operations: usize`.** On a `Rebuilt` round the two are the same figure by
  construction — the full build IS what `specialized_load_resource_plan` prices, so charged and
  executed coincide there, not by coincidence. On a `Retained` round
  `RetainedLoadTranspose::executed_operations` prices only the work the retained path actually
  performs: the G5-repriced per-cell scan allowance (`LOAD_WITNESS_SLOTS + 5` per retained
  cell, `LoadTransposeCounts::operations`'s own constant) over `cells`, plus the O(rows) fill
  and O(contacts) hash `finalize_retained_round` runs, plus the most recent `record_commit`'s
  own O(|l| + |r| + contacts) remap (`last_update_operations`, `0` on a round that reused a
  build with no intervening commit). The two eligibility traversals a full build pays twice
  (`LoadTransposeCounts::count`/`::finish`, F-G6-2) are not in this figure at all — a retained
  round never runs them.
- **The invariant `executed_operations <= baseline_operations` is checked twice per round** —
  once inside `build_gate_load_state` on the locals it just computed, once again in
  `evaluate_gate_batch` on the `GateLoadState` it was handed back — both `debug_assert!`s,
  compiled out of release the same way the G3 executed/charged assert is (its own "Appended
  2026-08-07" note explains the disposition). Pinned as a real test too:
  `fu8f_g6c_retained_round_executes_no_more_than_it_is_charged` (`perception.rs`) drives one
  committed merge through the retained path and compares `RetainedLoadTranspose::executed_operations`
  against `specialized_load_resource_plan` — the SAME function `build_gate_load_state` charges
  from — on the round's own shape.
- **The census meters the run total, not just one round.** `RetainedLoadCensus` gains
  `charged_operations`/`executed_operations` (`u128` running sums, printed on the gate's
  `T0_scaled_cap` row as `retained_load_charged_operations=`/`retained_load_executed_operations=`),
  accumulated on both `Rebuilt` and `Retained` rounds via `record_load_build_meter` so a run's
  rebuild rounds and retained rounds land in the same total.
- **The retained arena's held bytes join `gate_load_state_bytes`**
  (`RetainedLoadTranspose::retained_bytes`, a new `Option<&RetainedLoadTranspose>` parameter),
  charged at the 2×-compaction CEILING (`2 x cells x 4 B`, never the arena's live length —
  `record_commit` only compacts once waste crosses that bound, so a live-size charge would
  under-state the peak the arena is actually allowed to reach between compactions) plus the real
  `buckets`/`contact_witness`/row-invariant sizes (not the design record's `(contacts + 1)`
  CSR-splice approximation — this carrier is the arena, D-G6-1's recommended shape, not the
  memcpy-splice fallback, so its own real layout is charged). This term feeds
  `check_gate_peak`'s `load_baseline_bytes` addend, checked against `max_gate_peak_bytes`
  (`SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES`, 1 GiB, unchanged) — **not**
  `max_load_scratch_bytes` (`SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES`, 512 MiB), which the
  design record's own ~66 MB estimate compares itself against for scale but which is a
  DIFFERENT cap, checked only inside `SpecializedLoadBaseline::build`'s own internal
  `finalize_load_rows` check — a check a retained round never reaches, which is the next point.
- **G1's per-round 32,848,040 B transpose-scratch term is retired on retained rounds — already
  true by construction since G6b, stated here rather than left implicit.** That term is charged
  inside `load_resource_plan`, called only from `SpecializedLoadBaseline::build`
  (`finalize_load_rows`'s own internal check against `max_load_scratch_bytes`). The retained
  path calls `finalize_retained_round` instead and never reaches `SpecializedLoadBaseline::build`
  at all, so that internal check — and the term it charges — simply does not run on a retained
  round. G6c's own addition (the arena's `retained_bytes`, above) is a DIFFERENT term, at a
  DIFFERENT cap (`max_gate_peak_bytes`), charged only while the carrier is live across rounds;
  the two are not in competition and neither substitutes for the other's cap.
- **The FU-8f closer's owed correction is landed**, on the G3 section above (`{#fu8f-g3-2026-08-07}`):
  the "Non-vacuity is measured by mutation" paragraph's claim that mutation (a) "fails the
  forced test ... and nothing else" over-claimed — verification proved
  `fu8f_prefix_acceptance_matches_the_full_evaluation` also fails it. The same correction has
  stood on `fu8f_forced_late_safe_proposal_stops_at_the_same_index`'s doc comment
  (`specialized.rs`) since 2026-08-07; this is that correction landing in the record it was
  owed to.

**No cap, floor or coefficient is raised, lowered or touched — the standing owner ruling holds.**
Nothing charged moves either: `baseline_operations` is untouched, and every existing pinned
figure (G5's repriced 795,766,031 / 1,844,748,658 / 4,525,792,233 / 5,709,845,912, and the
32,848,040 B transpose-bytes term wherever it is quoted for a FULL build) stands exactly as G5
left it — this section adds a meter and a new byte term beside those figures, it does not
re-sweep them. `docs/rust-migration/support-resource-caps-2026-08-05.md` §16 carries the same
account; `crates/plinth-core/src/constants/placeholders.rs`'s
`SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES` and `SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES`
registry notes each carry a dated addendum; `constants.md` is regenerated by its producer
(`cargo run --example render_constants_golden -p plinth-core --offline`) and only note text
moves, no value column.

**Verification.** Five gates green at this tip, tree clean before and after: **1003** tests
default (G6b tip 1002) and **1004** `--all-features` (G6b tip 1003), 0 failed — the **+1** in
both is exactly this phase's one new test, attributed one for one. `make_raking_blade`'s 21
retained rounds and its terminal `contact_witness` rebuild all still ran under the new
`debug_assert!`s during this run (`support_runner_integration.rs`'s
`public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing`, debug profile, asserts
live) without tripping either — the charge-vs-meter invariant held on real corpus geometry, not
only on the fixture. Clippy clean with `-D warnings`, `cargo fmt --all -- --check` clean,
`git diff --check` clean. No wall-clock number is recorded anywhere in this section: concurrent
workflows were running on this machine throughout, so counters and bit-identity are the only
landed measurements. **G6d remains the measuring phase.**

### FU-8f phase G6d measured 2026-08-08 — THE GATE COMPLETES; the treant's tournament is 823 rounds; the wall moves to the initial-contact preflight {#fu8f-g6d-2026-08-08}

**Scope, stated first. G6d changed no engine code at all**, exactly as G4 did not. It is a
measurement phase: two treant runs under the 600 s timebox, per-round counters from env-gated
scratch instrumentation reverted by editing before the gates, and this record. `git diff` over
`crates/` at this tip is **empty**. No cap, floor, coefficient, charged number or byte term is
raised, lowered or touched by this phase.

**THE HEADLINE, plainly: the gate completes, and for the first time in this project the Weeping
Treant's merge tournament runs to its own end.** Both attempts finished the whole model run in
**339 s against the 600 s box** — a **1.77x whole-run margin** — with the tournament committing
**823 merges**. The gate is no longer the treant's stopping point. What stops it now is a
**typed refusal in previously-unexercised code**, recorded below and opened as **FU-8g**, and
deliberately not fixed.

#### G6d.1 Conditions, and exactly how quiet the machine was

Release, `--features census`, `PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`,
`PLINTH_THREADS=4`, RSS watchdog unsandboxed (ruling R-g) sampling every 2 s, armed at 90 GB and
**never fired**, 600 s timebox, two attempts, branch tip `fe79124` (the G6c tip).

**The quiet claim is stated with its evidence rather than asserted.** Two sibling workflows hold
worktrees on this machine. `ps` was polled for `cargo`/`rustc` thirteen times over the twelve
minutes before attempt 1 and again at each launch: **zero at every poll**, and CPU idle was
85–92 % throughout. Two `corespotlightd` (OS indexer) spikes were seen *during the polling
window* and are recorded rather than hidden. The load-bearing evidence is not the polling
though — it is that **the two attempts agree to 0.51 % on the tournament wall-clock and are
bit-identical on every counter** (G6d.2). Wall-clock here is therefore landed, not marked
CONTENDED.

#### G6d.2 What the two runs did — and they are the same run twice

| | attempt 1 | attempt 2 |
|---|---:|---:|
| outcome | **gate COMPLETED**; run stops on a typed refusal in `Initial` | identical |
| whole-run wall-clock (watchdog, 2 s sampling) | **339 s** | **339 s** |
| merge rounds committed | **823** | **823** |
| gate batches (census) | **826** | **826** |
| contacts, first → tournament exit | 1,366 → 543 | 1,366 → 543 |
| physical contacts after `coalesce_exact_regular_seats` | **542** | **542** |
| proposals gathered, first round → last committed round | 39,223 → 3 | 39,223 → 3 |
| cumulative proposals gathered (census) | **20,059,412** | **20,059,412** |
| process start → gate stage open | 123.0 s | 124.3 s |
| tournament wall-clock | **183.29 s** | **184.22 s** |
| gate close → process exit | ≈31 s | ≈30 s |
| mean round | 222.6 ms | 223.8 ms |
| peak RSS | 2,851,632 KB (2.719 GiB) | 2,653,888 KB (2.530 GiB) |
| watchdog | armed 90 GB, never fired | armed 90 GB, never fired |

**Determinism is measured, not assumed: over all 823 rounds, `contacts`, `proposals` and the
retained-load rebuild count are identical between the two attempts — zero differing rounds.**
Every census counter matches to the unit, including `retained_load_charged_operations` and
`retained_load_executed_operations`. Only wall-clock differs, by 0.51 % on the tournament.

Sampled per-round table (attempt 1; the full 823 rows are in the run transcript, not committed):

| round | contacts | proposals | gather ms | round ms | stage s |
|---:|---:|---:|---:|---:|---:|
| 0 | 1,366 | 39,223 | 28.6 | 242.7 | 0.2 |
| 100 | 1,266 | 38,398 | 27.9 | 241.5 | 28.6 |
| 200 | 1,166 | 37,319 | 26.9 | 231.0 | 52.9 |
| 300 | 1,066 | 33,952 | 28.4 | 228.3 | 81.2 |
| 400 | 966 | 29,885 | 27.1 | 215.8 | 106.8 |
| 500 | 866 | 24,826 | 25.7 | 208.8 | 128.1 |
| 600 | 766 | 13,536 | 22.7 | 183.9 | 148.3 |
| 700 | 666 | 3,035 | 13.9 | 156.1 | 165.5 |
| 800 | 566 | 65 | 5.6 | 232.9 | 180.4 |
| 822 | 544 | 3 | 5.7 | 132.0 | 183.3 |

#### G6d.3 WHY the tournament ends — a first, and not for the reason anyone predicted

**The tournament ends because no proposal in the final batch is safe.** It does not run out of
proposals and it does not meet a cap. The last committed round (822) gathered 3 proposals and
accepted one; the loop then gathered **2 more** and `GateBatchAcceptance::StopAtFirstNotWorse`
returned **no accepted index**, so `let Some((proposal_index, _)) = outcome.accepted else
{ break; }` took the break. Derived to the unit: census `merge_proposals` 20,059,412 minus the
transcript's sum over the 823 *committed* rounds, 20,059,410, is exactly the 2 proposals of that
final, non-committing batch.

**This retires a standing claim.** The G6 design record states that "no round has yet failed to
find a safe proposal", and FU-8f's D1 called a `k > 0` prefix "the one thing the corpus cannot
supply". The treant has now supplied both: the `Some(0) → Some(2)` transition at round 46 that
G4 found, and now a **`None`** — the acceptance fold's terminating arm, exercised on real corpus
geometry for the first time.

**Both prior round predictions are falsified, in opposite directions.** The blade-ratio 463 was
too low; the 1,365-contact hard bound was too high. The answer is **823**, which is 60.2 % of
the treant's 1,366 contacts, against `make_raking_blade`'s 33.9 %. The hard bound was never
reached because the gate stops offering safe merges 542 contacts short of it.

**And G4's round model does not extrapolate, which is why G4's projections were wrong.** G4 fitted
`round_ms = 304.17 + 0.6924 × contacts` over rounds spanning contacts 931–1,366 and projected
504.7 s at 463 rounds and 1,061.6 s at the hard bound. Inside its fitted range that model is
excellent (G6d.5). Outside it, it fails: **proposals collapse superlinearly**, 39,223 → 3 over
the run and 24,826 → 65 over the last 300 rounds, so the round's cost falls far faster than
contacts do. Nothing in G4 was wrong at its own tip; the extrapolation simply had no evidence
under it, and now it does.

#### G6d.4 Retention over the whole tournament: 97.3 % of load builds are retained

From the census, identically in both attempts:

| counter | value |
|---|---:|
| `retained_load_rounds` (gate-batch load builds) | **826** |
| retained (derived) | **804 — 97.3 %** |
| `retained_load_rebuilds` | **22** |
| `retained_load_rebuild_poisoned` | **21** |
| `retained_load_rebuild_contact_witness` | **1** |
| `retained_load_charged_operations` | **433,886,641,066** |
| `retained_load_executed_operations` | **48,520,980,855 — 11.18 % of charged** |

- **The single `contact_witness` miss is the one G6a designed for.** It is the terminal
  proposal-free audit, after `coalesce_exact_regular_seats` mutates contacts behind the carrier —
  exactly finding **F-G6-4**, exactly guard 2, once per tournament, at a measured ≈0.2 s of a
  339 s run. Design open question 4 ("should the terminal audit ride retention?") is answered by
  measurement: **no, and declining it costs nothing.**
- **The 21 `poisoned` misses are a FINDING and are recorded as one.** They occurred at rounds 36,
  60, 65, 80, 190, 213, 214, 234, 260, 261, 264, 307, 308, 341, 348, 558, 742, 743, 748, 749 and
  800 — identically in both attempts, so they are deterministic properties of the geometry, not
  noise. **The G6b record's parenthetical that the position-bit tie is "a case no corpus commit
  has ever produced (all 21 `make_raking_blade` remaps were exact)" was true of the blade and is
  NOT true of the treant**, and that sentence is corrected here rather than rewritten in place.
  One caveat is owed and stated plainly: **the census cannot say which poison fired.**
  `RetainedLoadRebuildCause::Poisoned` collapses guard 3 (a generic-resolving merged seat) and
  every `record_commit` identity-walk refusal — including the position-bit tie — into one label,
  so this record does **not** claim the tie is the cause. Splitting that label is named as
  follow-up work in the FU-8g row; it is a one-counter change and would make the next run's
  transcript self-explaining.
- **Retention behaved exactly as designed under every miss**: a poison costs one counted full
  rebuild and the carrier repopulates, so misses are isolated single rounds, never a run of them.
- **The G6c charge-vs-meter severance holds run-wide.** Executed is 11.18 % of charged over 826
  rounds and `executed ≤ charged` held every round; no round executed unafforded work.

#### G6d.5 The round, measured — with the pre-G6 round as an internal control

**The 21 rebuild rounds are the pre-G6 round, measured at this tip, in the same process.** That is
what makes the retention factor a within-run measurement rather than a cross-run comparison. Of
those 21, the 15 that fall inside G4's fitted contact range reproduce G4's model to a mean of
**−0.97 %** (worst −3.2 %, best +0.0 %):

| contacts | rebuild round measured (ms) | G4 model (ms) | delta |
|---:|---:|---:|---:|
| 1,330 | 1,214.3 | 1,225.1 | −0.9 % |
| 1,306 | 1,204.7 | 1,208.4 | −0.3 % |
| 1,301 | 1,203.8 | 1,205.0 | −0.1 % |
| 1,176 | 1,114.5 | 1,118.4 | −0.4 % |
| 1,102 | 1,067.5 | 1,067.2 | +0.0 % |
| 1,018 | 983.3 | 1,009.0 | −2.6 % |

Against that control:

| | measured |
|---|---|
| retained round, least squares over all 802 retained rounds | **`round_ms = 71.46 + 0.1405 × contacts`** |
| retained round mean, whole tournament | **206.5 ms** (attempt 2: 206.4 ms) |
| rebuild round mean, whole tournament | **879.6 ms** (attempt 2: 887.8 ms) |
| retained vs the G4 model at 1,366 / 1,000 / 800 contacts | **5.15× / 4.50× / 4.43×** |
| work removed per round at ≈940 contacts | ≈752 ms |

#### G6d.6 The prediction scorecard — the first in this chain to land, stated against the two that missed

D-G6-5 opened with an honest accounting of why D4 missed, and predicted a ~0.19 s retained round,
~741 ms removed, and a **350–450 s whole run**. Measured:

| predicted (D-G6-5) | measured | verdict |
|---|---|---|
| retained round ~0.19 s | 0.206 s mean | **+8.5 %, lands** |
| ≈741 ms removed per round | ≈752 ms at ≈940 contacts | **+1.5 %, lands** |
| whole run 350–450 s | **339 s** | **better than the band** |
| gate stage 200–260 s at the bound | 183 s for the whole tournament | **better, but for the wrong reason** |

The last row is where the honesty is owed. The gate stage came in under its predicted band
**not** because the round was cheaper than predicted — it was 8.5 % dearer — but because the
tournament is **823 rounds, not the 1,365 the design priced**. Two errors of opposite sign
partly cancelled. The round model was right; the round *count* was not, and no prediction in
this chain has yet got the count right.

#### G6d.7 Memory: the 3.32 GiB observation is attributed NEGATIVELY, and that is a real narrowing

Peak RSS **2,851,632 KB (2.719 GiB)** at t=243 s (attempt 1, mid-tournament) and **2,653,888 KB
(2.530 GiB)** at t=134 s (attempt 2, ≈10 s after gate-stage entry). Watchdog armed at 90 GB,
never fired; 2.7 % of the 100 GB machine ceiling.

The G3 run recorded 3,479,600 KB (3.32 GiB) and the G5 run 3,538,624 KB (3.375 GiB), both
observed-and-unexplained; G4 recorded 2.554/2.528 GiB and called it a non-reproduction. The
standing hypothesis — written in the G3 section as "the likeliest reading, that this run simply
got further into the tournament than any before it" — is **now falsified rather than merely
unconfirmed.** These two runs go **823 rounds: the entire tournament, 1.9× G4's depth and further
than any run in this project's history** — and they peak **below** both G3 and G5. **Tournament
depth is not the explanation, and that hypothesis is retired.**

What is left, stated as observation rather than mechanism: the two G6d attempts do not agree on
*when* the peak falls (mid-tournament vs gate-stage entry) or on its size (a 7.4 % spread) while
being bit-identical on every counter, which is the signature of **2 s sampling landing luckily or
unluckily on a transient of a few hundred MB**. No run has ever exceeded 3.4 GiB. G6's own
retained arena is charged, is ~66 MB-class, and cannot account for any of the spread. **This is
not a cap question in either direction, and no memory claim in the G6 record depends on it.**

#### G6d.8 THE NEW WALL — a typed refusal in previously-unexercised code, opened as FU-8g

Both attempts, identically, after the gate completes:

```
CENSUS  T0_stage_error  Weeping Treant v1.2
  Preparation: Initial(Limit { resource: "work items", required: 232585208, limit: 4000000 })
```

**Derived to the unit from source, not guessed.** `initial::preflight`
(`crates/plinth-support/src/initial.rs`) computes
`work = contact_count × 4 + penetration.max_total_work`, where `penetration` is
`surface().penetration_intervals_batch_worst_case_limits(contact_count, 8, interval_cap, false)`.
Inside `penetration_requirements` (`rust/geom_kernel/src/queries.rs`) with `rays_all = false`:
`rays = 1`, `hits_per_ray = mesh.nfaces()`, and `work = rows × rays × hits_per_ray`. So

> `work = contact_count × (4 + nfaces)` = **542 × (4 + 429,120) = 542 × 429,124 = 232,585,208**

which reproduces the printed figure exactly, and independently confirms the surviving contact
count is 542. It is checked against `ContactConstructionLimits::production().max_work_items =
4_000_000` (`runner.rs`) — a **flat** cap. The demand is **58.1×** the cap.

**[SUPERSEDED AS THE ENGINE'S CHARGE, 2026-08-09 by FU-8g phase H1/H2
(**[`#fu8g-h2-2026-08-09`](#fu8g-h2-2026-08-09)**). The formula above is no longer what
`initial::preflight` computes.** `max_total_work` is now the census pass's counted `Σ tests`
(executed traversal effort), not `rows × nfaces`, and the treant's charge at 553 contacts is
**108,378** — **2.71 %** of the same, untouched 4,000,000 cap, not 58.1× it. The record above
stands as measured: it is what the engine charged at `fe79124`, and the derivation from source
was correct for that tip. **The cap itself was not raised, lowered or touched.**]**

**The shape is the FU-8b / FU-8c family a fourth time: a `contacts × faces` product measured
against an unscaled flat bound.** At this mesh's 429,120 faces the cap admits at most
`4,000,000 / 429,124 = 9` physical contacts. The corpus passes this preflight only because every
corpus mesh is two to four orders of magnitude smaller in face count; nothing about the treant is
special except its size. **And the quantity is a preflight *bound*, not a measured demand:**
`hits_per_ray = nfaces` is the "every face could be hit" worst case, while the executed query is
BVH-bounded — the same over-statement G5 corrected on the load term, in the same direction.

**It is NOT a cap to raise, and nothing here proposes raising it.** The standing owner ruling
holds and no cap is touched by this phase. The remedy, when it is designed, is the one FU-8b,
FU-8c and G5 all took: make the *bound* honest, not the cap bigger. Recorded as **FU-8g**.

**[ADDENDUM 2026-08-09 — `ContactConstructionLimits::production().max_work_items`, the cap named
in this section, now has an M0 registry entry.** FU-8g phase H3 registered it as
`SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS` (`Placeholder`, unchanged at 4,000,000), alongside its
three siblings and ten brace/raft caps. Registration did not touch the number this section names
or the charge H1/H2 rebased it onto. See *FU-8g phase H3 landed 2026-08-09*
([`#fu8g-h3-2026-08-09`](#fu8g-h3-2026-08-09)).]**

#### G6d.9 Caps: none binds, and two projections can now be checked against truth

- **`max_merge_total_proposals` = 100,000,000** against a whole-tournament cumulative gather of
  **20,059,412** — a **4.99× margin**, and this is the real total rather than an extrapolation.
  G4 projected "≈33.0 M at the hard bound"; the true figure is 20.1 M, so that projection
  over-stated by 64 % — for the same reason its wall-clock projections did (G6d.3). Recorded as a
  correction of a *projection*, not of a measurement: G4's own 15,544,641 over 436 rounds stands.
- **`max_merge_iterations` = 100,000** against 823 tournament merges plus the coalesce pass's one.
  Two orders of magnitude of headroom, now measured rather than bounded. Untouched.
- **`specialized_gate_operations`** materialized **35,445,500,736**, `at_floor=false` — unchanged.
  **`specialized_load_operations`** materialized **11,421,470,912** — unchanged.
- Every other cap was admitted every round of both runs. **No cap, floor or coefficient is
  raised, lowered or touched.**

#### G6d.10 Ruling R-f is now SATISFIABLE — and G6d deliberately does not act on it

**R-f's precondition is met: a treant run has completed the gate, twice, with counters.**
Re-deriving `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is **explicitly not this phase's work** —
the G6 design's D-G6-6 says so in the G6d row itself — and it is **not** done here. It is
recorded as **now unblocked**, with the inputs it will need (826 gate batches, 20,059,412
proposals, the materialized 35,445,500,736, all reproduced twice) sitting in this section for
whoever takes it. **No coefficient moved.**

#### G6d.11 G6e is NOT opened, and open question 3 is declined again — both on measurement

- **G6e (retained baseline nearest sweep) does not open.** D-G6-6 makes it contingent on a
  whole-run margin below ≈1.5×. The measured margin is **600 / 339 = 1.77×**. The stronger reason
  is that it would now buy nothing that matters: **the gate is no longer the binding stage.** The
  run stops on a typed refusal ≈30 s after the tournament ends, so removing the ≈117 ms/round
  sweep would move the stopping point not at all.
- **Design open question 3** (retire the per-round `owner_faces`/`pair_starts` rebuild) was to be
  re-measured here. Measured: `gather_pair_proposals` runs 28.6 ms early and 5.7 ms late, and the
  whole tournament is 183 s of a 339 s run. **Declined again, measured** — the ruling R-c shape.

#### G6d.12 What did NOT happen, recorded so it is not mistaken for a result

**There is no seal. The treant still has no first seal, and this phase does not deliver one.**
`prepare_support_run` returns `Err` at the initial-contact preflight, so **routing, rewire,
penetration/brace, the CCO anchors, attestation, lowering, `routed_contacts`,
`final_live_members` and a snapshot digest are all still unexercised.** No claim whatever is made
about `pen_intervals_one`'s predicted 14–32 GB at `T = 4`, or about the brace's
`max_scratch_bytes` of 1e11 sitting above the 90 GB watchdog line. Those remain untested
predictions, exactly as G4 left them. What changed is only that the pipeline now reaches a
*later* refusal than it ever has.

**Instrumentation hygiene.** All per-round and stage-boundary timers were env-gated on
`PLINTH_G6D_ROUNDS` (plus one `#[cfg(feature = "census")]` read-only counter snapshot), were
never reachable without that variable, and were removed by editing before the gates. `git diff`
over `crates/` at this tip is empty and the five gates below ran on that tree. The two run
transcripts and the runner script are scratch and are **not** committed, per the standing rule
that run outputs never enter git.

**Verification hygiene, recorded 2026-08-08 as an infrastructure lesson from this phase's own
verification cycle:** two isolated verifiers were launched onto the *same* worktree path and the
collision destroyed one verifier's in-flight run — so **isolated verifiers must use unique
per-agent worktree paths**. No branch state was harmed and no measurement above is affected.

#### G6d.13 Gates at this tip

**Five gates green**, verbatim, tests with `--no-fail-fast`: **1003 / 1004 passed (default /
`--all-features`), 0 failed** — identical to the G6c tip, as it must be, since G6d adds no test
and changes no code. Clippy clean at `-D warnings`, `cargo fmt --all -- --check` clean,
`git diff --check` clean, tree clean before and after. Full derivation:
`support-resource-caps-2026-08-05.md` §17.

### OD-8 delta at this tip — MEASURED ZERO {#fu8b-od8-delta-2026-08-06}

Full 8-model census re-run at `d82c895`, **3,415 lines**. All seven reachable digests are
byte-identical to the frozen values, census-to-census:

| Model | `snapshot_digest` at `d82c895` | vs frozen |
|---|---|---|
| `Sphere` | `17667468971446251870` | unmoved |
| `Cylinder` | `13310185350867878190` | unmoved |
| `make_box` | `9657051231007502658` | unmoved |
| `make_overhang_table` | `6686599816688114065` | unmoved |
| `make_toed_slab` | `11199296506741562956` | unmoved |
| `make_raking_blade` | `13278215294653054002` | unmoved |
| `make_thin_fin` | `7016743535301716631` | unmoved |

All 14 corpus `T0_scaled_cap` rows print `at_floor=true`. **No geometry moved.** The charged
work changed — that was the point — but no test pinning a charged-work value fell on the
corpus, because every corpus batch is still governed by the unchanged floor.

### Owner ruling R6 (2026-08-06) — FU-8b runs first; decimation is demoted {#owner-ruling-r6-2026-08-06}

Continuing the R-numbered rulings ledger from *Owner caps-landing rulings R1–R5*. This is a
**binding owner decision of 2026-08-06**, delegated with veto retained.

- **R6 — the decimation-first premise is FALSIFIED; FU-8b runs first and decimation is
  demoted to a diagnosis + witness follow-up.** The original priority rested on decimation
  shrinking the treant enough to compound through every downstream stage. Measurement says
  otherwise, on four counts: the treant's reduced candidates are discarded by the
  **topology** gate, not the utility gate — `nonmanifold_edges` goes 0 → non-zero at every
  attempted ratio and `geometrically_eligible_candidate_count` is **0**, so granting
  `DownstreamUseful` changes the treant by exactly zero faces; even ignoring topology the
  ratified geometric budget admits only **~6–9 %** reduction, which would leave the gate
  charge ~7× over the flat cap and refuse in the same place for the opposite reason; the
  treant has a **tracked** 21,456,084-byte sealed artifact and a test
  (`crates/plinth-orient/tests/orientation_rederivation.rs`) that explicitly forbids
  decimating it; and two *small* models (`Sphere`, `make_toed_slab`) would decimate the
  instant a witness is granted, moving two frozen digests. The measurement record is
  [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md), committed verbatim
  below its provenance banner. Decimation is tracked as **FU-25**; the ordered critical path
  keeps FU-8c first.

  > **Note added 2026-08-06 (FU-8c closed) — dated addendum; the ruling above is left
  > standing exactly as the owner made it.** FU-8c landed at `a3308ec` and is now closed in
  > the follow-ups ledger. R6's ordering decision is unaffected in substance — the treant's
  > support-gate defect still runs ahead of decimation, and decimation is still demoted on
  > the four measured counts above — but the head of that path is now **FU-8d**, the residual
  > FU-8c's own measurement exposed. Read "keeps FU-8c first" as "keeps the treant gate chain
  > first"; nothing in R6 turned on which member of the chain was current.

  > **Note added 2026-08-07 (FU-25 Phase 2 landed) — dated addendum; the ruling above is left
  > standing exactly as the owner made it.** R6's four counts are unaffected: the treant is
  > still discarded by the topology gate, `geometrically_eligible_candidate_count` is still 0,
  > and the witness that now exists moves it by exactly zero faces. One clause of the fourth
  > count has stopped being true as written. "Two *small* models (`Sphere`,
  > `make_toed_slab`) would decimate the instant a witness is granted, moving two frozen
  > digests" was the reason the size threshold was called necessary; the threshold was
  > therefore registered (`DECIMATION_UTILITY_MIN_INPUT_FACES = 32,768`, `Placeholder`) as the
  > witness's **first clause**, so both models refuse before any geometry is examined. Read
  > that sentence as the justification for the threshold rather than as a live risk: the
  > digests `17667468971446251870` and `11199296506741562956` were re-measured unmoved at the
  > Phase-2 tip, structurally rather than by luck. See the FU-25 row and
  > [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md) §9.

## Owner ruling: the FU-10 phase-C census is RE-PINNED, all four (2026-08-08) {#fu10-repin-2026-08-08}

Asked *"How should the frozen digests be re-pinned?"*, the owner selected **"Re-pin all 4,
accept merge (Recommended)"** on 2026-08-08 — accepting the `make_raking_blade` tooth-body
merge, because **a 0.029886 mm-deep fusion prints as fused at 0.05 mm layers, so the merged
solid matches physical reality** (PR-1(a) precedent: sub-layer features are not resolvable).
This closes the OD-8 stop-and-report opened by
[`slicer-union-design-2026-08-07.md`](slicer-union-design-2026-08-07.md) §8.5. The evidence
record the ruling was made against is
[`attribution-slicer-census-2026-08-07.md`](attribution-slicer-census-2026-08-07.md), landed
verbatim with a provenance banner; it was adversarially verified by an independent exact GEOS
overlay of all 196 differing layers before the owner ruled.

**Everything written before this section stands unamended.** Every earlier citation of
`6686599816688114065`, `11199296506741562956`, `13278215294653054002` or
`7016743535301716631` remains correct *as a dated record of what was frozen at the time it
was written*. The live values are the table below.

### The re-pinned census, re-measured after merging develop

`develop` `0f24b79` (FU-8f G6) was merged into `feature/slicer-union` **before** anything was
re-pinned, and all seven digests were then re-measured at the merged tip with a fresh
`cargo run --release --offline --features census --example embed_incidence_census`, one model
per process. **G6 moved none of them** — all seven reproduce the pre-merge phase-C
measurement exactly, so the pinned values are the merged tip's own measurement:

| Model | live digest (2026-08-08) | previous | routed / live |
|---|---|---|---|
| `Sphere` | `17667468971446251870` | unmoved | 7 / 15 |
| `Cylinder` | `13310185350867878190` | unmoved | 54 / 119 |
| `make_box` | `9657051231007502658` | unmoved | 11 / 25 |
| `make_overhang_table` | `1875674282253663076` | `6686599816688114065` | 73 / 261 (unchanged) |
| `make_toed_slab` | `2823659516373762066` | `11199296506741562956` | 64 / 170 (was 64 / 179) |
| `make_raking_blade` | `9567737006650358297` | `13278215294653054002` | 27 / 72 (was 32 / 89) |
| `make_thin_fin` | `8797083662933568276` | `7016743535301716631` | 17 / 41 (unchanged) |

`make_overhang_table` and `make_thin_fin` produce **byte-identical support** — same contacts,
members, island births and local minima. Their digests moved only because the plan digest
hashes slice-layer loop vertices directly, and a per-layer (not per-solid) pass-through
requantized bystander loops by **five picometres**.

The whole of the remaining delta is the removal of **24 phantom island births**: 20 on
`make_raking_blade` (layers 90–109, one per layer, the same fused rake tooth re-born on every
one; its 8 genuine free-standing births at layers 82–89 survive bit-identically) and 4 on
`make_toed_slab` (layers 49, 52, 226, 228 — the layer immediately after each of the four runs
where a boundary-degenerate parity probe had labelled a solid toe a void). No material is lost
anywhere in the corpus; **87.9727 mm²** of real toe material is restored on `make_toed_slab`.

### Three goldens re-pinned from measurement at the merged tip

| Golden | from | to |
|---|---|---|
| `public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing` | toed `(64, 179, 4826202191472851835)`, raking `(32, 89, 1709218010932846012)` | toed `(64, 170, 13346779153602682749)`, raking `(27, 72, 8660653339934460766)` |
| `specialized::tests::oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing` | toed `(72, 50688, 9)` digest `18042644323102122916`, shared 4; raking `(40, 35008, 11)` digest `208221774708787011`, shared 3 | toed `(42, 34304, 8)` digest `5397140793948005744`, shared 3; raking `(33, 34176, 8)` digest `9036091150423983742`, shared 3 |
| `public_coverage_report_is_frozen_without_evidence_loss` | `snapshot_digest` `5304257786453935365` | `5470664714994646368` — **regenerated through the test's own producer path**, then diffed field by field: exactly one field differs, no evidence lost |

`make_toed_slab`'s `plan.contacts().len()` **72 → 42** is FU-7's parked number, digit for
digit, arrived at from the other direction: FU-7 proposed to *harden* the boundary-degenerate
containment probe; phase C **retires** it, replacing parity with the union engine's structural
shape hierarchy. Both land on 42. **The freeze-blocking, digest-moving half of FU-7 is
discharged here.** The OD-9 `BuriedSurface` 3D `surface.inside` question that shares the FU-7
row is *not* — that is a different probe on a different question, its code is untouched by
this cutover, and it should be split out rather than closed by association.

The *Provisional freeze: make_toed_slab* item is therefore retired as of this ruling, on the
72 → 42 ground it named.

### A movement neither §8.5 nor the attribution named

The specialized golden also asserted `coalesced_seat_count == 1` for **both** models, and that
number moved: `toed slab` **1 → 0**, `raking blade` **1 → 1**. With the corrected slice
geometry `make_toed_slab` no longer produces a co-located regular seat pair at all, so the
terminal coalescing pass is a measured no-op on it. It is now pinned per model. The gate's own
health predicates are unchanged and still green on both (non-worsening; every contact on a
distinct physical position, 42 of 42 and 33 of 33). Corpus coverage of
`coalesce_exact_regular_seats` halves — see **FU-30**. The disappearance is *consistent with*
the four removed phantom births but was **not** independently proven to be caused by them.

### Three ruling conditions, all discharged

1. **§8.5's "abutting, not overlapping … within 2e-8 mm²" claim is FALSIFIED for
   `make_raking_blade`** and is corrected by dated addendum
   ([§8.6](slicer-union-design-2026-08-07.md), original text left standing). The tooth is
   embedded 0.029886 mm into the body over ~0.172 mm; shared-boundary length is **zero**; the
   overlap is **2.5718e-3 mm²**, five orders of magnitude outside the stated bound, verified
   independently twice. Secondary: the 2e-8 mm² bound is too tight even for the true
   abutments (`make_toed_slab` L46 shows +1.03e-7 mm²); the honest bound is
   ≈ perimeter × quantum, ≈ 4e-7 mm² for a 40 mm loop.
2. **§8.5 measurement (3) is a COVERAGE GAP, not a measurement.** The OQ-3 adequacy consumer
   ran **zero rows corpus-wide** (2,956 print-layer rows against 0 adequacy rows;
   `probe_heights` empty on every model), so "moves zero rows" was vacuous, and the newly
   binding `max_loop_vertices` cap on that site is an **unexercised refusal path**. Dated note
   in §8.6; carried as **FU-29**.
3. **Errata against the adversarial verification record**, all in the conservative direction:
   the restored `make_toed_slab` material is **87.9727 mm²** (exactly the sum of the seven
   false-hole areas), *not* 88.043086 mm² (a 600×600 grid reading); the shared-edge floor
   across all merges is **0.7249 mm** (`make_toed_slab` L244), *not* "≥ 1.0 mm"; and that grid
   carries a **~1e-3 mm²/cell noise floor** — the same order as the raking-blade overlap
   sliver — so the zero-material-lost conservation claim rests on the **exact polygon overlay
   plus a 2e-8 mm erosion probe**, with the grid as corroboration only. Wherever the
   conservation claim is stated, the noise floor and the real instrument must be stated with
   it.

### Gate at the re-pinned tip

`cargo test --workspace --offline --no-fail-fast`: **1040 passed / 0 failed / 0 ignored**.
The arithmetic closes exactly against develop `0f24b79`'s 1011: `1011 + 7` (phase-A
orientation) `+ 23` (phase-B `plinth-layers`) `− 2` (deleted with the Tier-1 stitcher) `+ 1`
(named successor) `= 1040`. Gate 2 (`--all-features`) is 1041, the same set plus one
feature-gated test the default run does not build. Gates 3–5 green, tree clean before and
after.

## Python is eliminated; the two gate counts converge (2026-08-08) {#python-eliminated-2026-08-08}

Export-path phase **E8** landed on `feature/python-elimination`, cut from `develop`
`8041828`. **The repository contains no Python.** This section is the record of what went,
what stayed and why, and the gate arithmetic — which has a property this project has never
had before.

### What was deleted

| Surface | What it was |
|---|---|
| `rust/chitubox_io/src/py.rs` | the `#[cfg(feature = "python")]` PyO3 module — binding glue |
| `rust/chitubox_io` `python` feature + `pyo3`/`numpy` | the optional deps it activated |
| `rust/chitubox_io/pyproject.toml`, `python/chitubox_io/` | maturin config and the mixed-layout package `_native` was injected into |
| `rust/chitubox_io/build.rs` | baked `CHITUBOX_IO_SOURCE_DIGEST`, read in exactly one place: `py.rs` |
| the crate's `cdylib` crate-type | existed only so maturin could package an extension module |
| `scripts/tests/*.py` | the pytest suite and `conftest.py` (the extension-staleness gate) |
| `scripts/tools/` | the legacy pipeline entry points — `prep_one.py`, `run_v5.py`, census tooling, the Blender-driven sweeps — plus `host.blend`, the Blender host document for `geometry_sweep.py` |
| `scripts/support_placement/` | the 35-module frozen engine those entry points drove |
| `scripts/build_kernel.sh` | a maturin driver that built the extension into `.venv` |
| `pyproject.toml`, `uv.lock` | the tracked config that declared the interpreter and environment serving all of it |

> **Correction, 2026-08-08 (same day, before merge).** The row above originally listed
> `.venv/` alongside `pyproject.toml` and `uv.lock`. That overstated what the branch did.
> `.venv/` is gitignored and was never tracked, so no commit could delete it: what E8
> deleted is the *tracked* Python config — `pyproject.toml`, `uv.lock`, and
> `scripts/build_kernel.sh`. The
> physical `.venv/` directory (243 MB) existed only in the definitive checkout, and was
> removed there by hand, separately, on 2026-08-08. Both things happened; crediting the
> branch with the second would claim a deletion git is not capable of making. The
> distinction is exactly why the guard below refuses to skip ignored paths — a re-`uv sync`
> would put a `.venv` back in a checkout without touching a single tracked file.

101 `.py` files. `Cargo.lock`, regenerated offline, loses **13 crates**: `pyo3` and its four
support crates, `numpy`, `ndarray`, `heck`, `indoc`, `memoffset`, `rustc-hash`,
`target-lexicon`, `unindent`.

`build.rs` deserves its own line, because deleting a build script is not obviously in scope
for "delete the Python". Its sole product was a source digest read by `py.rs` and recomputed
by `conftest.py`; with both ends gone it hashed 200 lines of hand-rolled SHA-256 into an
env var no code could observe. It went the same way, and for the same reason, that
`rust/geom_kernel`'s `build.rs`, `pyproject.toml` and `py.rs` went together in `4c01975` when
that crate was de-Pythonised. The precedent is the repository's own.

### What was deliberately KEPT

- **`scripts/tests/fixtures/` — entirely.** Binary `.chitubox` and `.stl` specimens plus
  frozen JSON, loaded today by Rust tests: `rust/chitubox_io/tests/common/mod.rs:15`,
  `crates/plinth-verify/tests/format_gate.rs:64`, `crates/plinth-emit/src/lib.rs`,
  `crates/plinth-support/src/perception.rs`. Only the `.py` files beside them were removed;
  every one of those paths still resolves, proven by the gates below.
- **`corpus/baseline/` artifacts**, and the **`scripts/tools/prep_one.py` provenance
  strings** in `rust/chitubox_io/tests/frozen_baseline.rs:532-551` and `corpus/manifest.json`.
  These name a tool that no longer exists **on purpose**: they are the frozen baselines'
  birth certificate, recording which tool at which git revision produced the artifacts the
  Rust suite is checked against. Deleting the tool does not un-produce the artifacts, and
  erasing its name would destroy the provenance they are trusted on. Both still pass
  verbatim.
- **`chitubox_io::CHITUBOX_API_VERSION`.** A real writer-compatibility contract, matched
  against `plinth-core`'s `EXPECTED_CHITUBOX_API_VERSION` registry entry. Only its doc
  comment changed — from describing the pytest staleness gate in the present tense, to
  recording it as history.
- **The `.gitignore` Python patterns**, now with a comment saying why: nothing here produces
  `__pycache__` or a `.venv` any more, but a stray interpreter run should not be able to
  leave droppings in a checkout.

Nothing was lost with the engine. Its role was oracle-of-record for the Rust ports, and that
role was already discharged: `frozen_baseline`, `roundtrip`, `synth_compare` and the
support-parity suite are Rust-native and check against **committed** artifacts, not against a
running interpreter. The engine remains recoverable from git history — the right place for a
measuring stick nothing measures against.

### The guard

`crates/plinth-verify/tests/no_python_guard.rs` makes the end state durable rather than
merely current. It walks the tree from `CARGO_MANIFEST_DIR/../..` (path-portable per the
hygiene-batch precedent — no hardcoded `/tmp`), skipping only `.git`, `target` and `exports`,
and asserts zero `*.py` files and no `Cargo.toml` declaring `pyo3` or `numpy` (bare key,
`[dependencies.<dep>]` header, or feature activation). Ignored paths are deliberately **not**
skipped: a `.venv` is invisible to git but is still Python in the checkout.

It is one `#[test]`, and deliberately not a naive one — a tree guard that walks the wrong
directory passes forever. Before asserting anything it proves it can see the repository (six
anchor paths, and a floor on the file count) and proves both detectors fire on synthetic
inputs. `#`-comments in manifests are stripped before matching, following
`plinth-support/tests/retired_formula_guard.rs`, so prose naming the retired binding stays
searchable while a live declaration does not. Negative control run before committing: a
one-line `scripts/*.py` dropped into the tree fails the test with that path named.

### Gates at the E8 tip — and the convergence

| Gate | Result |
|---|---|
| `cargo test --workspace --offline --no-fail-fast` | **1041 passed / 0 failed** |
| `cargo test --workspace --offline --no-fail-fast --all-features` | **1041 passed / 0 failed** |
| `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` | clean, exit 0 |
| `cargo fmt --all -- --check` | clean |
| `git diff --check` | clean |

Tree clean before and after. Attribution against `develop` `8041828`'s 1040 / 1041:

- **default: 1040 + 1 = 1041.** The one addition is the no-Python guard.
- **all-features: 1041 + 1 − 1 = 1041.** Plus the guard, minus
  `chitubox_io::py::tests::every_format_gate_error_has_a_stable_python_kind`, deleted with
  the module that held it. Verified by name: the guard appears exactly once in each log, the
  py-glue test appears in neither, and `chitubox_io`'s lib-unit section now reports
  `running 0 tests` where the all-features run previously reported 1.

**The two numbers are now the same number, for the first time in this project's history.**
The *Test-count counting recipe* section below documents at length why "the test count" was
never one number but a pair — `N` default, `N+1` all-features — and that the pair was real
and both halves correct. The whole of that gap was one feature-gated test behind
`chitubox_io`'s sole optional feature. That feature is deleted, so both invocations now
compile an identical test set. Proven stronger than by totals: the two runs' per-binary
`Running`/`Doc-tests` lines and all **90** `test result: ok. N passed` summaries are
identical line for line.

That recipe section stays as written — it is the correct history, and its counting method is
still the canonical one. What changes from this commit forward is only the expectation: a
default and an `--all-features` run that now *disagree* is a signal worth chasing, not the
expected ±1. Keep running both; they still differ in what they compile everywhere else, and
the workspace can grow another optional feature at any time.

### OD-8 corpus census

`cargo run --release --offline --features census --example embed_incidence_census` over the
provisioned corpus. All **seven** reachable models seal, and every `snapshot_digest` is
byte-identical to the pre-E8 values — Sphere `17667468971446251870`, Cylinder
`13310185350867878190`, make_box `9657051231007502658`, make_overhang_table
`1875674282253663076`, make_toed_slab `2823659516373762066`, make_raking_blade
`9567737006650358297`, make_thin_fin `8797083662933568276`. Expected, and worth stating: E8
deleted no Rust that runs, so a digest move would have meant something went wrong.

### What E8 does NOT claim

The export-path design gated E8 on the M1 parity table — baseline artifacts vs
`plinth prepare` artifacts, as-good-or-better on every metric on every model — and sequenced
it after E5. **Neither happened, and the record should not be read as if they had.** The
sequencing changed because the gate's premise did: it existed to stop the measuring stick
being destroyed before the comparison, and the measuring stick is now the committed frozen
artifacts, which are still here. But **the M1 as-good-or-better comparison has still not been
made.** `plinth prepare` has not been scored against the baselines; that work is still owed,
is still E4/E5's prerequisite for calling the native path a replacement, and is now to be
done Rust-against-artifacts. E8 settled something narrower: the Python is gone, no build or
test step needs an interpreter, and no coverage was lost in the removal. See the dated
LANDED note on the E8 phase entry in
[`export-path-design-2026-08-07.md`](export-path-design-2026-08-07.md).

## FU-8g phase H0 measured 2026-08-08 — the corpus's honest demand, and a NEW blocker upstream of the wall H0 was sent to measure {#fu8g-h0-2026-08-08}

**Scope, stated first, exactly as G4/G6d's precedent.** H0 changed no engine behaviour.
Env-gated (`PLINTH_FU8G_CENSUS`) counters were added at two sites — `collect_hits_with_stack`'s
own traversal (`rust/geom_kernel/src/queries.rs`, mirrored rather than modified, so the real
production path is untouched) and `build_initial_contacts` just before its `preflight` call
(`crates/plinth-support/src/initial.rs`) — then exercised against the seven corpus models and
the Weeping Treant, and **reverted by editing before the gates**. `git diff` over `crates/` and
`rust/` is empty at the tip this section describes. The design this phase measures against is
[`fu8g-design-2026-08-08.md`](fu8g-design-2026-08-08.md), landed verbatim at H0 with its own
provenance banner.

### H0.1 The seven corpus models: honest crossings and traversal tests, at the real BVH, in-process

`cargo run --release --offline --features census --example embed_incidence_census -p
plinth-support`, once per model via `PLINTH_CENSUS_ONLY`, `PLINTH_FU8G_CENSUS=1` added. Every
row below is `rows = contact_count` real physical contact stubs — not the design's seat-proxy
sample — walked through `capsule_rays` and `collect_hits_with_stack` exactly as
`pen_intervals_one` walks them, so this is the executed path, not an estimate of it.

| model | faces | rows | crossings mean/p50/p95/p99.9/max | tests mean/p50/p95/p99.9/max |
|---|---:|---:|---|---|
| `make_box` | 12 | 11 | 1.273 / 1 / 2 / 2 / **2** | 14.636 / 15 / 17 / 17 / **17** |
| `make_overhang_table` | 60 | 84 | 1.167 / 1 / 2 / 3 / **3** | 20.571 / 19 / 35 / 37 / **37** |
| `make_toed_slab` | 140 | 68 | 2.147 / 1 / 2 / 32 / **32** | 21.794 / 19 / 49 / 69 / **69** |
| `make_raking_blade` | 24 | 33 | 1.515 / 1 / 3 / 4 / **4** | 25.545 / 27 / 31 / 31 / **31** |
| `make_thin_fin` | 24 | 18 | 1.333 / 1 / 2 / 2 / **2** | 18.333 / 19 / 21 / 21 / **21** |
| `Sphere` | 4,760 | 7 | 6.000 / 6 / 6 / 6 / **6** | 51.000 / 49 / 62 / 62 / **62** |
| `Cylinder` | 244 | 54 | 2.056 / 1 / 2 / 46 / **46** | 42.685 / 40 / 64 / 122 / **122** |

**All seven OD-8 snapshot digests reproduced byte-identical** with the instrumentation live —
Sphere `17667468971446251870`, Cylinder `13310185350867878190`, make_box
`9657051231007502658`, make_overhang_table `1875674282253663076`, make_toed_slab
`2823659516373762066`, make_raking_blade `9567737006650358297`, make_thin_fin
`8797083662933568276` — confirming the census pass reads geometry without perturbing it.

Charged-vs-honest, at the *unchanged* charge formula (`work = rows × nfaces`,
`scratch_bytes` = today's `row_scratch` formula evaluated at the honest per-row crossing
count instead of `nfaces`, D2/F4's arithmetic, not the F10 bound — H1's territory, not H0's):

| model | charged work | honest work (Σ crossings) | honest work (Σ tests, D3) | charged scratch | honest scratch (Σ rows) |
|---|---:|---:|---:|---:|---:|
| `make_box` | 132 | 14 (**9.4×**) | 161 (**0.82×**) | 391,523 | 69,501 (**5.6×**) |
| `make_overhang_table` | 5,040 | 98 (**51.4×**) | 1,728 (**2.9×**) | 14,007,588 | 520,870 (**26.9×**) |
| `make_toed_slab` | 9,520 | 146 (**65.2×**) | 1,482 (**6.4×**) | 26,203,460 | 621,814 (**42.1×**) |
| `make_raking_blade` | 792 | 50 (**15.8×**) | 843 (**0.94×**) | 2,258,421 | 233,503 (**9.7×**) |
| `make_thin_fin` | 432 | 24 (**18.0×**) | 330 (**1.31×**) | 1,231,722 | 118,290 (**10.4×**) |
| `Sphere` | 33,320 | 42 (**793×**) | 357 (**93.3×**) | 90,961,171 | 145,509 (**625×**) |
| `Cylinder` | 13,176 | 111 (**118.7×**) | 2,305 (**5.7×**) | 36,139,662 | 485,277 (**74.5×**) |

**Two findings the design's out-of-repo estimate could not see, both stated because they are
measured, not guessed:**

1. **`make_box` and `make_raking_blade` under-charge today's `work` if it were repriced to
   traversal tests verbatim (D3), by 0.82× and 0.94×.** On these two tiny meshes the BVH's own
   node-visit overhead exceeds `nfaces`: a 12- or 24-face tree still walks a handful of internal
   nodes per ray, and at single-digit face counts that overhead is not amortized away. This does
   not touch the design's thesis (the treant's ratio, whatever it turns out to be, is not going
   to invert), and it changes nothing charged today (no cap moves in H0), but D3's own severance
   (§F9) should carry this caveat forward: "traversal tests" is the honest quantity, and honest
   is not always smaller.
2. **The `Sphere`'s real crossing count is 6, uniformly, not the design's proxied 1.** The design's
   §M caveat 1 said the seat proxy (downfacing faces, stride-sampled) is not the real contact set
   and warned exactly this could happen. It did: the corpus `Sphere` fixture hollows before
   support placement (`plinth-hollow`), so a real outward stub ray now crosses an outer shell and
   an inner shell — several thin walls, not one convex surface — and every one of the 7 real
   contacts sees exactly 6 crossings (p50 = p95 = p99.9 = max = 6, zero spread). The
   `nfaces/mean_crossings` over-statement ratio this produces (793×) is materially different from
   the design's own proxy figure (4,760× at `mean = 1`) — six times lower — precisely because the
   proxy could not see the hollow shell. This is the caveat firing as designed, not a defect in
   the design.

### H0.2 THE BLOCKER — the Weeping Treant does not reach `initial::preflight` at `develop @ e3168c5`; it fails one stage earlier, on a pre-existing defect the design's premise did not anticipate

**The design's entire F1–F12 / D1–D7 / M analysis is built on one fact taken from *FU-8f phase
G6d measured 2026-08-08* ([`#fu8f-g6d-2026-08-08`](#fu8f-g6d-2026-08-08)): "the Weeping Treant
now stops after its tournament with `Preparation: Initial(Limit { resource: "work items",
required: 232585208, limit: 4000000 })`."** That fact was measured on branch
`feature/fu8f-lazy-acceptance` at tip `fe79124` — the G6c tip, **before** FU-10's slicer-union
work merged into `develop`. `fe79124` is an ancestor of `e3168c5` (`git merge-base
--is-ancestor fe79124 e3168c5` confirms it), and the design's own provenance banner states it
was designed against `develop @ e3168c5` itself, treating the G6d fact as still current there.

**It is not current there.** Run three times at `e3168c5` (`cargo run --release --offline
--features census --example embed_incidence_census -p plinth-support`, `PLINTH_CENSUS_ONLY="Weeping
Treant v1.2"`, `PLINTH_THREADS=4`, RSS watchdog unsandboxed at 90 GB / 2 s sampling / 600 s
timebox / max 2 attempts per the standing memory rule — all three finished well inside both
limits: 206–210 s wall-clock, peak RSS 2.53–3.35 GiB, watchdog never armed) — twice with
`PLINTH_FU8G_CENSUS=1` (H0's own instrumentation active) and once with it **unset** (a clean
control, to rule out H0's own code as the cause, since H0's block sits inside
`build_initial_contacts`, downstream of the actual failure site) — **all three attempts fail
identically, before `build_initial_contacts` is ever called**:

```
CENSUS  T0_stage_error  Weeping Treant v1.2  Preparation: Specialized(DuplicatePhysicalSeat { left: 163, right: 164 })
```

`Specialized(...)` is a distinct error variant from `Initial(...)`; this is the specialized
contact-plan construction stage, which runs and completes **before** `build_initial_contacts`
is called at all (`runner.rs:3070`). H0's own instrumentation, gated inside
`build_initial_contacts`, therefore never executes on the treant at this tip — not because of a
bug in H0, but because the pipeline never reaches it. **No honest in-engine crossings/traversal-
test measurement for the Weeping Treant is possible today, and none is claimed.**

**Root cause, to the commit.** `git diff --stat fe79124 e3168c5 -- crates/ rust/` shows
`crates/plinth-support/src/perception.rs` (+1,291/−… lines) and
`crates/plinth-support/src/specialized.rs` (48 lines) changed between the G6d measurement and
this tip, carrying **FU-10's phase-C polygon-union cutover** — merged via `0f24b79`/`8041828`.
That cutover's own re-pinned test comment
(`crates/plinth-support/src/specialized.rs`,
`oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing`) states it plainly
for the two corpus models it touches: `make_toed_slab`'s coalesced-seat count moved **1 → 0**
and its contact count moved **72 → 42**; `make_raking_blade`'s contact count moved **40 → 33**.
**This is exactly [FU-30](#follow-ups-2026-08-04)'s own finding, restated in that row's own
words:** "the `DuplicatePhysicalSeat` failure mode [`coalesce_exact_regular_seats`] was built to
prevent now has a single corpus witness" (`make_raking_blade`), and FU-30's own "First action"
asked whether one corpus witness is adequate. **It is not: the treant is the second witness, and
on the treant the pass does not catch the collision.** Physical seats 163 and 164 on the
429,120-face mesh's specialized contact plan now land at the same position under the corrected
slice geometry, and nothing in the current `coalesce_exact_regular_seats` pass merges them, so
`Specialized`'s hard-refusal fires before the tournament — before routing, before the gate, and
three stages before the `initial::preflight` wall this entire follow-up was opened to fix.

**Consequence for FU-8g.** The design's premise is falsified at its own stated base commit: the
treant does not currently reach `initial::preflight`, so the 232,585,208-vs-4,000,000 wall F1–F5
describe is **not the treant's current stopping point**. It may still be the *next* stopping
point once the new upstream defect is fixed — nothing here says F1–F12's analysis of
`penetration_requirements` is wrong, only that it cannot be exercised on the treant until
something else is fixed first. **Comparison against the design's §M treant figures (62.03 mean
crossings/ray, 6,918× over-statement) is therefore BLOCKED, not confirmed and not refuted** — no
in-engine number exists to compare. The corpus comparison in §H0.1 stands on its own and is not
affected by this blocker (corpus models never go near `DuplicatePhysicalSeat` on this
geometry).

**This is not FU-8g's defect to fix.** It sits in `Specialized` seat construction, three stages
before anything FU-8g's design touches, and H0's mandate is measurement only — no engine
change. It is opened as **FU-31** in the ledger below, cross-referenced to FU-30, and named as
FU-8g's own critical-path blocker: H1–H6 all require exercising the treant past the point it
currently fails, so FU-31 (or an equivalent fix inside `Specialized`) is now on FU-8g's own
critical path, ahead of H1.

**Verification.** Five gates green at this tip (default and `--all-features`), tree clean
before and after; `cargo fmt --all -- --check` and `git diff --check` clean; all seven OD-8
digests re-confirmed byte-identical (§H0.1). No cap, floor, constant or charged number is
raised, lowered or touched. `git diff` over `crates/` and `rust/` is empty — the two
instrumentation sites were reverted by editing, not left behind, and this section is the only
change this phase makes.

## FU-31 diagnosed 2026-08-08 — one island birth, two owning islands, two bit-identical seats, and the one merge the load gate refuses {#fu31-diagnosed-2026-08-08}

**Scope, stated first.** Diagnosis only — no engine behaviour changed, no fix landed, no cap,
floor, constant or charged number touched. Scratch `eprintln!` instrumentation (env-gated
`PLINTH_FU31_DUMP` / `PLINTH_FU31_GATE_X`) was added at seven sites in
`crates/plinth-support/src/specialized.rs`, exercised, and **reverted by editing**: `git diff`
over `crates/` and `rust/` is empty at this tip and `grep FU31` over the sources returns
nothing. Two instrumented reproductions at `develop @ e3168c5` (same harness and env as H0:
`cargo run --release --offline --features census --example embed_incidence_census -p
plinth-support`, `PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, `PLINTH_THREADS=4`, 90 GB RSS
watchdog, 600 s timebox) both failed identically with
`Specialized(DuplicatePhysicalSeat { left: 163, right: 164 })` and produced byte-identical seat
dumps, consistent with H0's three uninstrumented reproductions. All seven corpus models were
also run with the instrumentation live and reproduced their OD-8 snapshot digests
byte-identical (§F31.4), so the census read geometry without perturbing it.

### F31.1 The seat pair, exactly

At the failing `canonicalize_and_validate` call the working vector holds **554** contacts and
exactly **one** `same_point` pair — 163/164 is the whole problem, not the first of many:

| | seat 163 | seat 164 |
|---|---|---|
| position_mm | (−11.691493872, 13.001936906, 79.675) | **identical, all 64 bits × 3** |
| position bits | `c027620b7c1b3fb3, 402a00fddfc7d9d2, 4053eb333333328c` | same |
| normal | (0.060714352, 0.986249148, 0.153708772) | identical bits |
| class | pure specialized (`primary_regular=None`, 0 routes) | pure specialized (same) |
| demands | 1: `IslandBirth` owner=**111**, face=137983, comp=0, birth_layer=**1593**, birth_z=79.67499999999762, footprint 5 pts | 2: `IslandBirth` owner=**113**, face=137983 — **the same birth event** — plus `Minimum` owner=113, face=78817, nominated (−11.662340164, 13.016237652, 79.626805368), 0.058 mm away |

The positions are not a near-tie and not the G6b bit-tie phenomenon (independent computations
landing on equal bits): they are **one value copied twice**. Both `IslandBirth` demands carry
the same nominated position because they were minted from the *same* `IslandBirth` record by
the per-owner fan-out in demand seeding, and `WorkingContact::specialized` copies the demand's
nominated bits into the seat position verbatim. Same face (137983), same normal, same
footprint; the only field distinguishing the two birth demands is `owner_island_id` (111 vs
113 — face 137983 belongs to both islands' `face_ids`). Seat 164 is itself a tournament
product: the commit log shows the owner-113 birth seat merging with the owner-113 `Minimum`
seat (gate-accepted, seat kept at the birth position), which is why it carries two demands.

### F31.2 Upstream provenance, and what FU-10 did and did not change

The birth: print layer **1593** (census layer plan, 0.05 mm pitch from 0.0), z_mm
79.67499999999762, footprint area **0.001078581 mm²** — a 5-point twig-tip island, projected
to exterior face 137983, component 0. Treant totals at `e3168c5`: 216 island births + 351
local minima → 567 seeds → **808** demands after per-owner fan-out, disposed as 36
`exact_regular` + 55 covered + **717 pushed** as pure specialized seats, of which **198
groups** share identical position bits (multi-owner fan-outs). The tournament committed **552**
merges and resolved **197 of the 198** groups; 163/164 is the sole survivor.

**The collision is pre-existing; FU-10 did not create it.** At `fe79124` (control, §F31.3) the
*same* birth exists — layer 1593, z bits identical (`4053eb333333328c`), area identical to all
printed digits, same projected face 137983 — with the centroid differing only in the last ~3
ULPs (`c027620b7c1e0fe8` vs `c027620b7c1b3fb3`, ≈1e-10 mm): the union cutover moved the
centroid *arithmetic*, not the event. The same two-owner fan-out happens there (owners 113/115
in that tip's island numbering), both demands are pushed as bit-identical seats, and the
control's tips hold **375** such identical-position groups (244 births / 408 minima / 1095
demands / 987 pushed) — **all of which the tournament merged**, this pair included. What FU-10
changed is the *load landscape* those merges are gated against, and for exactly one pair the
merge became load-adverse.

### F31.3 Why every dedup machine misses the pair — each for a stated, local reason

1. **`canonicalize_and_validate`'s duplicate check** defines "physical duplicate" as
   `same_point` = exact position-bits equality (`left == right` on `[f64; 3]`), unconditional
   over all contacts. It fires. (Indices 163/164 are post-sort positions — FU-13's complaint;
   this diagnosis needed a full dump run to learn what the payload could have said.)
2. **`coalesce_exact_regular_seats`** admits a pair only when positions are `same_point` AND
   **both sides are demand-free** (regular seats; the doc comment's dyadic-vertex case). 163/164
   fail by **class** — both are demand-carrying specialized seats — not by tolerance and not by
   pass ordering. No widening of its *position* criterion could ever catch them.
3. **`attach_exact_mixed_contacts`** requires exactly one demand-free side (regular ×
   specialized). Both sides carry demands → skipped every round.
4. **The gated tournament** is the only machine whose admission covers this class, and it
   *did* gather the pair — in **492** rounds, first as the pure birth-111 × birth-113 pair
   (one proposal; the two demands share one representative key) and after the 113-merge as
   163 × 164 with two variants (seat at the birth position, seat at the `Minimum` position).
   The gate refused **every** variant, always for the same measured reason:
   `load_violation_count` **3 vs baseline 2**, all four coverage components and
   `unsupported_specialized_count` identical. `StopAtFirstNotWorse` accepted other pairs for
   552 rounds; in the terminal round 3 proposals remained (this pair's two variants among
   them) and none was accepted.

**The load mechanism, named.** Two co-located seats split the nearest-won load rows between
them, each staying under `tip_policy`'s safety factor; the merged seat wins the union of both
row sets, and its summed carried area **violates the tip safety factor** (the ratified area
law). The duplicated seat pair is, in load-model terms, a physically-single tip evading the
per-seat load cap by splitting itself across two model contacts. The gate, applying not-worse
honestly, refuses to expose that overload; `canonicalize_and_validate`, applying the
one-seat-per-position invariant honestly, refuses the un-merged pair. Between the two correct
local refusals the pipeline has no third path, and the model errors out.

### F31.4 The `fe79124` control, and the corpus cross-check

**Control.** A detached scratch worktree at `fe79124` (the G6d tip, pre-union), corpus models
provisioned from the local copy, same instrumentation, same harness/env: the treant's
specialized stage **passes** — the terminal duplicate scan reports **542 contacts, zero
`same_point` pairs** — and a second control run with `PLINTH_CENSUS=1` prints the stopping
point itself: `CENSUS T0_stage_error Weeping Treant v1.2 Preparation: Initial(Limit {
resource: "work items", required: 232585208, limit: 4000000 })` — the wall G6d pinned, digit
for digit, measured again at G6d's own commit. The introduction
of the `DuplicatePhysicalSeat` failure is thereby bracketed to the FU-10 cutover merge
(`fe79124` → `e3168c5`, the `perception.rs`/`specialized.rs` delta H0 already attributed), and
the mechanism in §F31.2/§F31.3 says *how*: not by minting the colliding seats — `fe79124`
mints 375 such groups and merges them all — but by shifting the load numbers under exactly one
of them so its merge stopped clearing the gate.

**Corpus.** All seven corpus models, run at `e3168c5` with the dump live: every model passes,
all seven OD-8 digests byte-identical (Sphere `17667468971446251870`, Cylinder
`13310185350867878190`, make_box `9657051231007502658`, make_overhang_table
`1875674282253663076`, make_toed_slab `2823659516373762066`, make_raking_blade
`9567737006650358297`, make_thin_fin `8797083662933568276`). Across the whole corpus there are
**6** pushed specialized seats and **zero** identical-position groups — no corpus model ever
creates the colliding configuration. Every corpus `same_point` pair the merge machinery sees
is demand-free regular (coalesce's ratified territory). One more treant datum for
[FU-30](#follow-ups-2026-08-04): the treant's working set contains a **27-seat co-located
demand-free regular cluster** at (16.045028687, −27.403488016, 11.745975015) which
`coalesce_exact_regular_seats` merges at the terminal pass (26 coalesces, before the error
fires downstream of it) — once FU-31 is fixed, the treant is a far richer second corpus
witness for that pass than `make_raking_blade`'s single pair.

### F31.5 Fix options, weighed on this evidence

**(a) Widen `coalesce_exact_regular_seats` to demand-carrying seats — NOT recommended.** The
unconditional union would merge 163/164, but the terminal audit `gate_not_worse(accepted,
initial_baseline)` then fails on the same 3-vs-2 load count and the model dies as
`MergeGateRegression` — the same wall with a different name. Landing (a) therefore also means
exempting canonicalization merges from the terminal audit, i.e. teaching the stage to absorb a
real measured load regression silently, and it touches the pass whose evidence field
(`coalesced_seat_count`) is owner-ratified and whose corpus coverage FU-30 is already
litigating, with mutation-proven fixtures required. High blast radius, and it repairs the
wrong invariant: the duplicate should never have existed.

**(b) Stop double-emitting: aggregate uncovered demands sharing identical nominated position
bits into ONE seat at push time — RECOMMENDED.** In the disposition loop, an uncovered demand
whose nominated bits equal an already-pushed specialized seat's position joins that seat's
`demands` vector instead of minting a sibling (sorted/deduped exactly as `commit_merge`
already unions demand lists; key on position bits, not seed identity, so a `Minimum`
coinciding with a birth centroid is also covered). The data model already says one tip may
serve several islands — seat 164 itself carries two demands today. The invariant
`canonicalize_and_validate` enforces then holds **by construction from birth**; the
tournament's baseline and the terminal audit both price the same seat topology, so the 3-vs-2
wedge cannot form — the initial baseline honestly reports the tip's summed load from the
start instead of discovering it at merge time. **The corpus is provably untouched:** zero
identical-position pushed groups exist on all seven models (§F31.4), so the aggregation is a
no-op there and no frozen digest can move — structurally, not by re-measurement hope. The
treant carries no frozen digest. Cost: a focused change in one loop plus a unit fixture
(two same-position uncovered demands from two owners → one seat, two demands) — a new,
mutation-provable test in `specialized`'s own suite, nothing touching coalesce's evidence
field. Risk: on multi-owner-rich models (today: the treant alone) the tournament's merge
history shifts (fewer initial contacts), and the reported `load_violation_count` rises where
the split pair previously hid an overload — that is the fix being *honest*, and violations
are recorded gate evidence the plan already tolerates (the treant's working baseline carries
2 today), not refusals.

**(c) "The refusal is correct; two distinct seats are real" — NOT supported.** The two seats
are one physical tip: same bits, same face, same normal, one birth event. The ratified data
model, the other 197 treant groups, and the same pair's own merge at `fe79124` all resolve
this configuration to one seat. What (c) would actually assert is that a duplicated seat is a
legitimate load-splitting device, which contradicts the load model's own physics (one arm
cannot occupy two model rows' worth of tip). One genuine physics question *does* fall out of
this diagnosis, for the owner rather than for FU-31: after (b), the twig tip at
(−11.69, 13.00, 79.68) will be honestly flagged as violating the tip safety factor — whether
a flagged tip should eventually hard-refuse, thicken, or spawn relief support is a question
about violation semantics that exists today (the treant already carries 2 recorded
violations) and is unchanged by this fix.

**Recommendation: (b)**, with the unit fixture named above, a treant re-run as the acceptance
check (expected: specialized passes, the run advances to the `initial::preflight` work-items
wall, unblocking FU-8g H0's honest treant measurement), and the (c) physics note recorded for
the owner's queue rather than folded into the fix.

**Verification.** Five gates green at this tip; tree clean; the instrumentation reverted by
editing (this section is the branch's only delta beyond H0's). The `fe79124` scratch worktree
was removed and pruned after its control runs.

## FU-31 FIXED 2026-08-08 — one seat per position by construction, and the treant walks to the wall {#fu31-fixed-2026-08-08}

**What landed (commit `55b4d99`, option (b) as adjudicated).** The demand-disposition loop in
`crates/plinth-support/src/specialized.rs` is extracted into `dispose_demands`, which seats
uncovered demands in **one grouping pass** keyed on the nominated position's f64 bits
(negative zero normalized to positive zero per coordinate, so key equality coincides exactly
with the `same_point` `==` test `canonicalize_and_validate` applies at its
`DuplicatePhysicalSeat` scan — both sites cite each other, making the by-construction claim
checkable in source). A demand whose key matches an already-pushed specialized seat joins that
seat's demand vector — which stays `compare_demands`-sorted and duplicate-free, matching
`commit_merge`'s union postcondition — instead of minting a bit-identical sibling. Seat order
is first-occurrence order, so any model with no same-position pushed group is behaviorally
untouched; the seat representative is the group's `compare_demands`-minimal demand,
`gather_pair_proposals`' own `selected_demand` convention. No per-demand scan of the physical
vector exists (one map lookup per demand). `coalesce_exact_regular_seats`, its owner-ratified
`coalesced_seat_count`, and every cap, floor and constant are untouched. Downstream was
verified from source before landing: the tournament (`demands()` chained iterators), load-row
construction, census hashing and evidence all consume multi-demand seats as first-class —
seat 164 itself was one before this fix.

**Mutation-proven fixture** (`one_birth_fanned_to_two_owners_seats_once_with_both_demands`):
one birth fanned to owners 111/113 produces ONE seat carrying both demands and canonicalizes
clean; with the aggregation disabled (the `Occupied` arm mutated to an unconditional push) it
fails `called `Result::unwrap()` on an `Err` value: DuplicatePhysicalSeat { left: 0, right:
1 }` — the treant's exact failure at unit scale. Applied, observed, reverted; transcript in
the test and function doc comments.

**One honest unit-fixture re-pin, attributed exactly.** The raking-blade fixture in
`required_table_raking_and_thin_fin_fixtures_finalize_full_evidence` runs the model in its
raw (non-census) orientation, and there the blade genuinely mints **four** island births each
fanned to two owners — confirmed by scratch instrumentation before re-pinning. Born
aggregated, those four pairs vanish from the tournament (gathered 10 → 0, accepted 8 → 4) and
the pinned digest moves with those gate-evidence numbers (`12073756428966718576` →
`13905373635452952930`). Contacts (40), coverage samples (40,192), load rows (13),
`load_violation_count` (3) and `coalesced_seat_count` (0) are all unchanged. This is not a
corpus, baseline, physics, or reference re-freeze.

**ACCEPTANCE — the treant, attempt 1 of 2, wall reached.** Same harness and env as H0/F31
(`cargo run --release --offline --features census --example embed_incidence_census -p
plinth-support`, `PLINTH_CENSUS=1 PLINTH_FU8G_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant
v1.2"`, `PLINTH_THREADS=4`, RSS watchdog unsandboxed at 90 GB / 2 s sampling / 600 s
timebox): **202 s wall-clock, peak RSS 2,776,144 KB (2.65 GiB), watchdog never fired.**
Specialized contact-plan construction **passes** — the working set that held 554 contacts
with the un-merged pair now finalizes **553** — and the run advances through routing, the
gate, and load finalization to exactly the wall FU-8g was opened to measure:

```
CENSUS  T0_stage_error  Weeping Treant v1.2  Preparation: Initial(Limit { resource: "work items", required: 237305572, limit: 4000000 })
```

The required number is the charge formula at the new honest contact count: 553 × 4 + 553 ×
429,120 = **237,305,572** — the same wall G6d pinned at `fe79124` (232,585,208 at 542
contacts), now measured in-engine at this tip with 553. The two scaled-cap census lines the
run also printed are recorded for FU-8f's file: `specialized_gate_operations` materialized
19,562,287,872 (340 gate batches, 1,294,392 merge proposals, retained-load charged
66,731,232,175 vs executed 6,634,686,434) and `specialized_load_operations` materialized
4,746,695,936 — neither at floor.

**FU-8g H0 treant leg — the honest in-engine numbers, captured in the same attempt.** The H0
instrumentation (env-gated `PLINTH_FU8G_CENSUS`, the same two sites: a mirrored
`collect_hits_with_stack` walk in `rust/geom_kernel/src/queries.rs` and a block in
`build_initial_contacts` immediately before its `preflight` call) was reimplemented from the
H0 record's recipe, **validated for metric fidelity on `make_box` — every H0.1 figure
reproduced exactly** (rows 11, charged 132, honest crossings 14, honest tests 161, charged
scratch 391,523, honest scratch 69,501, both distributions digit-for-digit) — exercised on
the treant, and **reverted by editing before the gates** (`git diff` over `crates/` and
`rust/` is empty at this tip; `grep -ri fu8g_census crates/ rust/` returns nothing). All
seven corpus OD-8 digests were re-confirmed byte-identical after the fix, with and without
the instrumentation live.

| quantity | Weeping Treant v1.2, in-engine at this tip |
|---|---|
| rows (real physical contact stubs) | **553** (faces 429,120) |
| crossings mean/p50/p95/p99.9/max | **10.906 / 7 / 26 / 298 / 298** |
| traversal tests mean/p50/p95/p99.9/max | **191.982 / 81 / 654 / 3,236 / 3,236** |
| charged work (rows × nfaces) | 237,303,360 |
| honest work (Σ crossings) | **6,031** (**39,347×** over-charged) |
| honest work (Σ tests, D3) | **106,166** (**2,235×** over-charged) |
| charged scratch | 647,733,598,841 B (603 GiB) |
| honest scratch (Σ row_scratch at honest crossings) | **149,188,000 B (142 MiB)** (**4,342×**) |

Against the design's §M out-of-repo proxy (62.03 mean crossings/ray, 6,918× over-statement):
the honest mean is **10.906**, so the real over-statement (nfaces / mean crossings) is
**~39,347×** — the design's proxy *under*-stated the over-charge by 5.7× on the treant, the
same direction §M's caveat 1 anticipated. Adjudicating what H1's bound does with these
numbers is FU-8g's work, not this fix's; they are recorded so H1 starts from measurement.

**The honest note, stated because the fix makes it visible.** The 0.001 mm² twig tip at
(−11.69, 13.00, 79.68) — one physical tip, one seat now — carries the summed load of both
owning islands' rows and reports its tip-safety-factor violation honestly in the specialized
gate evidence, within plan tolerance (violations are recorded evidence the plan already
tolerates — the treant's baseline carried 2 before this fix — not refusals; the stage
completes and the pipeline advances). Whether a flagged tip should eventually hard-refuse,
thicken, or spawn relief support remains the owner-queue physics question F31.5(c) recorded,
unchanged by this fix.

**Verification.** All seven frozen OD-8 census digests byte-identical after the fix: Sphere
`17667468971446251870`, Cylinder `13310185350867878190`, make_box `9657051231007502658`,
make_overhang_table `1875674282253663076`, make_toed_slab `2823659516373762066`,
make_raking_blade `9567737006650358297`, make_thin_fin `8797083662933568276`. Five gates
green at this tip (counts recorded in the commit); tree clean before and after; no cap,
floor, constant or owner-ratified evidence field touched; `/private/tmp/plinth-chitubox-preview`
untouched.

## FU-8g phase H2 landed 2026-08-09 — the charge IS the counted demand, the receipt is checked against it, and the refusal path is proven at caps nobody touched {#fu8g-h2-2026-08-09}

**Scope, stated first.** H2 is the charging phase of
[`fu8g-design-2026-08-08.md`](fu8g-design-2026-08-08.md) (its H2 row: preflight rebase,
charged-vs-executed meter, no cap raised, lowered or touched). Two of its four listed items were
already landed by H1 at `08db663`, which said so in its own commit body; H2 verifies them,
records their measured effect, adds the receipt the design asked for, and proves the refusal
path still fires at the **unchanged** production caps. **No cap, floor, constant or
owner-ratified evidence field is raised, lowered or touched by this phase.**
`ContactConstructionLimits::production()` is byte-identical to `develop @ e3168c5`:
`max_work_items = 4_000_000`, `max_scratch_bytes = 512 × 1024 × 1024 = 536,870,912`, and both
literals are now pinned by an assertion in the refusal fixture below, so a future cap move has
to come and change them there.

### H2.1 What `initial::preflight` charges now, before and after, per model

The preflight charge is `work items = contact_count × 4 + penetration.max_total_work`.
`max_total_work` **before** H1 came from `penetration_intervals_batch_worst_case_limits`, i.e.
`rows × nfaces`; **after** H1 it is the census pass's `Σ row.tests`, summed over rows, where
`tests` is executed traversal effort (BVH nodes popped plus Möller evaluations). The two
`work` columns below are therefore that formula evaluated on the **measured** numbers of record:
the seven corpus rows come from *FU-8g phase H0 measured 2026-08-08*
([`#fu8g-h0-2026-08-08`](#fu8g-h0-2026-08-08)) §H0.1, and the treant row from the FU-31
acceptance run recorded in *FU-31 FIXED 2026-08-08*
([`#fu31-fixed-2026-08-08`](#fu31-fixed-2026-08-08)). **This is arithmetic on measurements
already in this file, not a new run** — H1 changed no geometry (all seven OD-8 digests are
byte-identical at this tip, re-confirmed below), and `Σ tests` is precisely what
`penetration_intervals_batch_census_limits` sums into `max_total_work`.

| model | rows | faces | charged BEFORE (worst case) | charged AFTER (counted) | change | after, as % of the 4,000,000 cap |
|---|---:|---:|---:|---:|---:|---:|
| `make_box` | 11 | 12 | 176 | **205** | **×0.86 — RISES** | 0.005 % |
| `make_overhang_table` | 84 | 60 | 5,376 | **2,064** | ×2.60 fall | 0.05 % |
| `make_toed_slab` | 68 | 140 | 9,792 | **1,754** | ×5.58 fall | 0.04 % |
| `make_raking_blade` | 33 | 24 | 924 | **975** | **×0.95 — RISES** | 0.02 % |
| `make_thin_fin` | 18 | 24 | 504 | **402** | ×1.25 fall | 0.01 % |
| `Sphere` | 7 | 4,760 | 33,348 | **385** | ×86.6 fall | 0.010 % |
| `Cylinder` | 54 | 244 | 13,392 | **2,521** | ×5.31 fall | 0.06 % |
| **Weeping Treant v1.2** | 553 | 429,120 | **237,305,572** | **108,378** | **×2,189 fall** | **2.71 %** |

**FINDING — the design's D4 claim is falsified as written, and its conclusion survives for a
different reason.** D4 states: *"No new refusal is possible on any corpus model: every corpus
demand falls (M1), and a falling demand can create no refusal."* **The demand does not fall on
every corpus model.** On `make_box` the honest charge is **1.16× higher** than the worst-case
one it replaced (205 vs 176) and on `make_raking_blade` **1.06× higher** (975 vs 924), exactly
as H0's finding 1 warned when it measured 0.82× and 0.94× on the `work` term alone: on a 12- or
24-face mesh the BVH's own node-visit overhead exceeds `nfaces`, and honest is not always
smaller. The conclusion — no new refusal on any corpus model — nevertheless holds, but it must
be argued from the margin rather than from monotonicity: the two risen charges are **205** and
**975** against a **4,000,000** cap, four orders of magnitude of headroom. The gates prove the
same thing operationally (1048/1048, both universes, no corpus model refusing anywhere).

**The treant fits the unchanged cap with 37× of margin.** 108,378 work items against 4,000,000
is **2.71 %** of the floor, so D5.3's contingency (the FU-8b batch-product rule with a measured
coefficient) is **NOT triggered and is NOT added** — adding scaling no measurement requires is
the same error as raising a cap. That contingency stays written down and unexercised.

**What is deliberately NOT claimed here: the scratch-bytes charge, per model.** H0.1's honest
scratch column was `row_scratch` evaluated at the honest crossing count; H1 charges something
different in form — per-ray buffers sized at the row's **worst single ray**, samples bounded by
the F10 closed form, totals summed per row — so H0's scratch arithmetic is not this tip's
scratch charge and is not presented as it. The mechanism that made `work` fall (counted
crossings instead of `nfaces`) drives the byte charge down by the same route, and the treant's
pre-H1 honest-scratch figure (149,188,000 B ≈ 142 MiB, against a 536,870,912 B cap) is the best
available indication that it fits, **but the definitive per-model byte charge is H6's to
measure and no number for it is claimed at this tip.**

### H2.2 The receipt — charged vs executed, asserted rather than assumed

The design's H2 row asks for "a charged-vs-executed meter beside the charge (the G3/G6c idiom)
so `executed ≤ charged` is asserted, not assumed". It exists now, in two independent places:

- **In the kernel.** `collect_hits_with_stack` — the fill pass's own walk — now returns the
  traversal effort it executed, counted in exactly the units `count_hits_with_stack` counts, and
  `pen_intervals_one` accumulates it with the hits pushed, samples placed and intervals emitted.
  `penetration_intervals_batch_bounded_metered` returns those totals as a public
  `PenetrationBatchMeter` and refuses with the new typed
  `PenetrationBatchError::ChargeDrift { resource, charged, executed }` if the batch outruns its
  own counted requirements. `penetration_intervals_batch_bounded` is now a thin delegation to it
  that drops the receipt, so no existing caller changed shape.
- **At the one site that spends a cap.** `build_initial_contacts` takes the receipt back from
  `classify_initial_overlaps` and checks it against `penetration_limits` — the very numbers
  `preflight_work` charged against `ContactConstructionLimits::production()` a few lines above —
  refusing with `InitialContactError::ChargeDrift` if execution outran the charge. The kernel's
  own check compares the receipt to the kernel's own counting pass; this one compares it to the
  number a cap was spent in, which is a different claim and the one a resource policy is written
  in.

**`work` and `hits` meter EQUAL to the charge, not merely under it** — they are the same two
quantities measured by the two walks — and that equality is what the new kernel test asserts;
`samples` and `intervals` are bounded quantities and land at or under their counted budgets.

### H2.3 The refusal path, proven at the unchanged caps

The obligation that comes with a charge that falls is that a mesh whose **honest** demand is
genuinely too large is still refused. `honest_over_demand_still_refuses_at_the_unchanged_
production_caps` (`crates/plinth-support/src/initial.rs`) builds a 8,000-slab, 96,000-face
lamella stack, sends 100 stub-shaped rows down the column in the seed loop's own batch shape
(`n_offset_rays = 8`, `rays_all = false`), and charges the counted demand against
`ContactConstructionLimits::production()` **verbatim**:

| quantity | value |
|---|---:|
| counted work (`Σ tests`) | **4,223,900** |
| the cap it is checked against | 4,000,000 (unchanged) |
| what the worst-case shape would have said | 9,600,000 |
| counted hits (`Σ crossings`) | 1,600,000 |
| counted penetration scratch | 356,484,600 B, against the unchanged 536,870,912 B cap |

So the refusal is a **work** refusal on counted traversal, with the byte charge still inside its
own cap, and the honest number that refuses is **2.3× smaller** than the fiction it replaced and
still over the line. `Err(InitialContactError::Limit { resource: "work items", required:
4,224,300, limit: 4_000_000 })` is matched with the cap as a literal.

### H2.4 Deviations from the design's H2 row, recorded (never silent)

1. **`initial::preflight` and `required_scratch_bytes` were already on census inputs before this
   phase started.** H1 (`08db663`) landed the D4 split — `preflight_contact_caps` (logical caps,
   still refused before any contact-sized allocation) and `preflight_work` (work items and
   scratch bytes, charged from the census totals) — and said in its own body that it had done
   so. H2 did not redo it; H2 measured its effect (§H2.1), added the receipt (§H2.2) and proved
   the refusal path (§H2.3).
2. **`designed_embed::for_batch` and the brace/transit limit derivations are NOT re-derived from
   the census, deliberately.** Those three surfaces hand the kernel a caller-owned **ceiling**,
   not a cap-backed charge: `for_batch` derives its ceiling from the surface itself, so it can
   refuse nothing; brace's is the flat `brace_penetration_limits` production cap; and transit's
   `TransitIntervalLimits` has **no production constructor at all** at this tip — every
   construction of it is test code. Since H1 the shared bounded entry point counts before it
   charges and allocates at every one of those call sites, so their allocations and their
   admission arithmetic are already counted; re-deriving their ceilings from a second census pass
   would add a full extra BVH descent per attestation batch for **zero** change in behaviour
   (counted ≤ worst-case always, proven relationally by the re-pinned exact-cap test). What H2
   does instead is state the ceiling-versus-charge distinction in the bounded entry point's own
   doc comment and give the one surface that really does spend a cap — `initial::preflight` —
   the receipt. If a later phase turns any of those three into a real charge, the metered entry
   point is already there to check it.
3. **The meter covers rows, work, hits, samples and intervals — not scratch bytes.** Bytes are
   not executed, they are reserved, and every reservation on this path is a fallible
   `try_reserve_exact` at exactly the counted budget, so the byte plan is enforced by an
   allocator refusal at the moment of spending rather than by a counter afterwards. Recorded so
   that the absence is visibly deliberate.

### H2.5 The every-record-site sweep for the moved charged number

The charge formula behind the treant's stopping point moved at H1, so every record that states
the old number as *what the engine computes* gains a dated pointer here rather than a rewrite.
Amended in this commit: **§G6d.8** of *FU-8f phase G6d measured 2026-08-08*, the **resumption
checklist item** that repeats it, the **FU-8g follow-up row**, and **§17.5** of
[`support-resource-caps-2026-08-05.md`](support-resource-caps-2026-08-05.md). The design doc
[`fu8g-design-2026-08-08.md`](fu8g-design-2026-08-08.md) is **not** amended: it was landed
verbatim under a provenance banner, its 232,585,208 and its §M projections are its premises as
designed, and this section is where measurement supersedes them. **Still owed by H3**, and
deliberately not done here: M0 registration of the four `ContactConstructionLimits` numbers, the
six `brace_penetration_limits` fields and `raft_limits` with this work's measurement of record;
caps-doc §18; `03-physics-and-constants.md`; `constants.md` regenerated by its producer; the
`m0-constants-source-ledger.json` entry and the FU-24 ledger gate.

### H2.6 Verification

Five gates green at this tip, tree clean before and after: (1) `cargo test --workspace --offline
--no-fail-fast` **1048 passed, 0 failed**; (2) the same `--all-features` **1048 passed, 0
failed**; (3) `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`
clean; (4) `cargo fmt --all -- --check` clean; (5) `git diff --check` clean. **Test delta
attributed exactly: 1045 → 1048, +3, all new, no re-pins and no deletions** — one kernel test
(the receipt matches the counted charge, `work`/`hits` by equality, geometry bit-identical to
the unmetered entry point) and two support tests (the over-demand refusal at the unchanged
production caps; the receipt tripwire, one case per checked resource so deleting any row from
the check fails it).

All seven frozen OD-8 census digests **byte-identical** with the meter live (`cargo run
--release --offline --features census --example embed_incidence_census -p plinth-support`,
`PLINTH_CENSUS=1 PLINTH_THREADS=4`, once per model): Sphere `17667468971446251870`, Cylinder
`13310185350867878190`, make_box `9657051231007502658`, make_overhang_table
`1875674282253663076`, make_toed_slab `2823659516373762066`, make_raking_blade
`9567737006650358297`, make_thin_fin `8797083662933568276`. **No treant run this phase** — H6
owns it, under the standing memory protocol. `/private/tmp/plinth-chitubox-preview` untouched.

## FU-8g phase H3 landed 2026-08-09 — the four `ContactConstructionLimits`, six brace penetration limits and four raft limits are registered; nothing is raised, lowered or touched {#fu8g-h3-2026-08-09}

**Scope, stated first.** H3 is the registration phase of
[`fu8g-design-2026-08-08.md`](fu8g-design-2026-08-08.md) (its H3 row: M0 registration + record
sweep). It changes **no engine code** — `git diff` over `crates/` and `rust/` at this tip is
empty. Its whole diff is the M0 placeholder registry table (`crates/plinth-core/src/constants/
placeholders.rs`), the three consuming test fixtures (`constants_placeholders.rs`,
`constants_golden.rs`, both bumping their handwritten id/row lists and entry counts from 239/270
to 253/284), `docs/rust-migration/m0-constants-source-ledger.json` (14 appended rows),
`docs/rust-migration/constants.md` (regenerated by its producer, 14 additive rows), and this
document plus `support-resource-caps-2026-08-05.md` §18 and
`03-physics-and-constants.md` §3.10.9. **No cap, floor, coefficient or byte term is raised,
lowered or touched.**

### H3.1 What was unregistered

F1/F2 of the design found that `ContactConstructionLimits::production()` (`initial.rs:63–70`)
and `SupportPreparationLimits::production`'s `construction_limits` (`runner.rs`, the same four
numbers hard-coded a second, independent time) had **no** M0 registry entry, and neither did
`brace_penetration_limits` (`PenetrationBatchLimits`, six fields) or `raft_limits`
(`RaftLimits`, four fields). Owner ruling R5 (2026-08-05) registered 42 sibling caps in the same
family and named these four nested authorities **explicitly out of scope**
(`03-physics-and-constants.md` §3.10.9), deferring them to "their own pass" — which is this one.
Until now, H1's and H2's own "no cap was raised" claims rested on a number with no lookup entry:
honest in fact, unverifiable from the registry.

### H3.2 What is registered

All 14 numbers, as `Placeholder`, classification `runtime_resource_cap`, on exactly R5's terms —
**registration is disclosure, not ratification**:

| Stable ID | Value | Unit |
|---|---:|---|
| `SUPPORT_CONSTRUCTION_MAX_CONTACTS` | 1,000,000 | count |
| `SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS` | 10,000,000 | count |
| `SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS` | 4,000,000 | count |
| `SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES` | 536,870,912 | B |
| `SUPPORT_BRACE_PENETRATION_MAX_ROWS` | 100,000 | count |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_WORK` | 1,000,000,000 | count |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_HITS` | 1,000,000,000 | count |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_SAMPLES` | 10,000,000,000 | count |
| `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_INTERVALS` | 1,000,000,000 | count |
| `SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES` | 100,000,000,000 | B |
| `SUPPORT_RAFT_MAX_CANDIDATES` | 100,000 | count |
| `SUPPORT_RAFT_MAX_ROOTS` | 100,000 | count |
| `SUPPORT_RAFT_MAX_PRIMITIVES` | 200,000 | count |
| `SUPPORT_RAFT_MAX_BYTES` | 134,217,728 | B |

Every value is byte-identical to `develop @ e3168c5` and to `git blame`'s `73e7c132` on
`runner.rs` — the same Codex-era commit R5's sibling caps already cite. Registering a number does
not move it; H2's own fixture already pins `ContactConstructionLimits::production()`
byte-identical, independent of this section.

### H3.3 Provenance: measured where measured, disclosed as absent where not

Per R4's precedent (single-specimen, n = 1, on measurements resting on the treant alone) and the
standing rule against inventing a demand figure no run has produced: `SUPPORT_CONSTRUCTION_
MAX_WORK_ITEMS` and `SUPPORT_CONSTRUCTION_MAX_CONTACTS` carry FU-8g H1/H2's measured treant demand
(108,378 work items, 2.71 % of the unchanged cap; 553 contacts, 0.055 %); `SUPPORT_CONSTRUCTION_
MAX_SCRATCH_BYTES` carries H0's pre-H1 estimate (149,188,000 B, ≈142 MiB, 27.8 %) with H2.1's own
disclaimer restated — the post-H1 figure is H6's to measure, not claimed here. `SUPPORT_
CONSTRUCTION_MAX_LOAD_ROWS` and all ten brace/raft constants carry **no** measured demand,
disclosed as such: the brace and raft stages are unexercised on every corpus model and on the
treant (§17.7) — no run has ever reached them. The design's F7 finding for the six brace fields
is registered as a **derived** worst-case ceiling from the pre-fix sizing formula (production
admits at most 85 clearance rods on the treant mesh once `max_scratch_bytes` is accounted for),
never as a measured demand, and `SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES`'s note additionally
discloses that this cap sits above the 90 GB watchdog line and the 100 GB machine ceiling — a
standing hazard this registration surfaces, not resolves. `CandidatePayloadConfig` remains
genuinely out of scope, exactly as R5 left it.

### H3.4 Records swept

`support-resource-caps-2026-08-05.md` gains **§18** (the full record) and dated addenda at §17.5
(the debt this section pays), §14.6 and §17.7 (both still-unexercised-remainder sections, pointed
at §18). `03-physics-and-constants.md` §3.10.9 gains a dated addendum correcting its own "not
covered" sentence for three of the four named nested authorities, with the full 14-row table.
`constants.md` was regenerated **by its producer** (`cargo run -p plinth-core --offline --example
render_constants_golden`); the diff is 14 additive rows, nothing else moved.
`m0-constants-source-ledger.json` gains 14 `registry_entries` rows, appended after the existing
tail — the same shape R5's own 42 additions used, not a whole-file resort — and
`constants_ledger_consistency`'s FU-24 gate is green. The **FU-8g follow-up row** below gains a
dated bracket; the resumption checklist's item 5 gains a dated addendum.

### H3.5 Verification

Five gates green at this tip, tree clean before and after: (1) `cargo test --workspace --offline
--no-fail-fast` **1048 passed, 0 failed**; (2) the same `--all-features` **1048 passed, 0
failed**; (3) `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings`
clean; (4) `cargo fmt --all -- --check` clean; (5) `git diff --check` clean. **No engine test
count moves**: H3 adds no test and changes no code, so the pass/fail counts are identical to the
H2 tip's 1048/1048, to the unit, on both universes. The FU-27 flake did not fire in either run.
The seven frozen OD-8 census digests are unaffected (this phase touches no code the census
exercises) and are not re-run for a docs-and-registry-only phase; H2's digests above remain the
values of record. `/private/tmp/plinth-chitubox-preview` untouched.

## FU-8g phase H4 landed 2026-08-09 — the rider: the `Poisoned` label splits three ways, and a committed-round counter with its exit reason rides beside it {#fu8g-h4-2026-08-09}

**Scope, stated first.** H4 is the owner-adjudicated rider
[`fu8g-design-2026-08-08.md`](fu8g-design-2026-08-08.md) named independent of the kernel work (its
H4 row): (1) split the census `Poisoned` label three ways so G6d's collapsed attribution of 21
poison-driven rebuilds becomes measurable, and (2) add a committed-round counter and tournament
exit-reason to the census output so a future treant transcript proves its own round count without
scratch instrumentation. **No cap, floor, coefficient or digest is touched.** The whole diff is
`crates/plinth-support/src/perception.rs` (D6), `crates/plinth-support/src/census.rs` (both), and
`crates/plinth-support/src/specialized.rs` (the D7 call sites) — `rust/` and every M0/constants
file are untouched.

### H4.1 D6 — the `Poisoned` label, split three ways

`RetainedLoadRebuildCause::Poisoned` (`perception.rs`) becomes `Poisoned(PoisonCause)`, where
`PoisonCause` is `Guard3Generic | CommitShape | IdentityWalk` — the owner's mandated two groups
(guard 3 vs. the identity walk) plus the design's recommended third (the four "commit shape"
sites: out-of-range/equal `left`/`right`, wrong-length committed list, a parent that is not
`LoadOwner::Specialized`, and the `first_specialized_owner` underflow check), per D6's own
argument that folding those into the identity walk would recreate the exact ambiguity the rider
exists to remove. `RetainedLoadTranspose::poison()` takes the cause as a parameter; every one of
its call sites in `record_commit` now names its own group. The census label surface
(`RetainedLoadRebuildCause::label()`, consumed by `census.rs`'s `by_cause: BTreeMap<&'static str,
usize>`) needed no structural change, exactly as D6 predicted — the split is three new label
strings, `"poisoned_guard3"`, `"poisoned_commit_shape"`, `"poisoned_identity_walk"`, in place of
the one `"poisoned"` string.

**A deviation from F11's inventory, found and recorded rather than absorbed.** F11 counted **nine**
`poison()` call sites and bounded its own source read to `record_commit`'s `:3378–3480`. Source
review at H4 found a **tenth**, one line past that bound: a `u32::try_from(merged_offset).is_err()`
guard protecting the arena-offset arithmetic from overflowing `u32` after the identity walk has
already succeeded — unreachable on any mesh this engine has ever run (the arena would need to
exceed `u32::MAX` live+dead cells), and defensive rather than caller-shape or position-matching in
character. Classified `CommitShape` by the same precedent F11 itself used for the
`first_specialized_owner` underflow check, and documented in full at the `PoisonCause` enum's own
doc comment in source, so the next reader does not have to re-derive the reasoning.

### H4.2 D7 — the committed-round counter and exit reason

`census::MergeTournamentCensus { tournaments, rounds_committed, contacts_first, contacts_last,
exit_reason }` is new in `census.rs`, printed on the same `specialized_gate_operations`
`T0_scaled_cap` row that already carries `gate_batches`/`retained_load_rounds` — the established
"same batches, same row" idiom. `consolidate` (`specialized.rs`) records one commit
unconditionally beside (not gated on) the existing `RetainedLoadTranspose::record_commit` call —
counting every round the tournament actually committed, not only the rounds retention observed —
and records one exit at each of the loop's three named stopping points: `proposals.is_empty()` →
`"no_proposals"`, `outcome.accepted` is `None` → `"no_safe_proposal"`, and
`required_iterations > limits.max_merge_iterations` → `"iteration_cap"` (a typed refusal, still
recorded before the `Err` return, because it is one of the tournament's own three stopping modes
in F12's sense). `MergeTotalProposalLimit` and `GateResourceOverflow` are deliberately **not** a
fourth reason, matching D7 exactly.

### H4.3 Census digest discipline — determined from source, proven by a corpus run

The rider's own instruction was to check whether the new counters feed the digest hash before
landing them, on the `gathered_pair_count` precedent (`specialized.rs`'s `SpecializedContactPlan::
digest()` hashes `self.gate_evidence.gathered_pair_count`, `accepted_pair_count`,
`deep_clone_count` and `max_commit_bytes` explicitly, by name, one `hash.usize()` call per field —
nothing generic or derive-based). Neither `PoisonCause` nor `MergeTournamentCensus` is a field of
any struct that digest function reads: both live entirely inside `census.rs`, itself
`#[cfg(feature = "census")]`-gated and additionally runtime-gated on `PLINTH_CENSUS=1`, printed as
additive `CENSUS` lines never consumed by production code. **Digests hold, by construction — and
this was proven, not merely argued**: a release build (`--features census`) run over all seven
reachable corpus models reproduced all seven frozen `snapshot_digest` values byte-identical to the
ones this document's spawn brief states (`Sphere 17667468971446251870`, `Cylinder
13310185350867878190`, `make_box 9657051231007502658`, `make_overhang_table 1875674282253663076`,
`make_toed_slab 2823659516373762066`, `make_raking_blade 9567737006650358297`, `make_thin_fin
8797083662933568276`), with `PLINTH_CENSUS=1` live and the new counters printing on every row.

The same run is the fixture-tournament proof D7's own verification bar names, on real geometry
rather than only the isolated unit fixture in H4.4: six of the seven models gather no merge
proposals at all (`merge_rounds_committed=0`, `merge_contacts_first == merge_contacts_last`,
`merge_exit_reason=no_proposals`), and `make_raking_blade` — the one model whose specialized gate
does real merge work — printed `merge_contacts_first=37`, `merge_contacts_last=34`,
`merge_rounds_committed=3`, `merge_exit_reason=no_proposals`: `37 − 34 = 3`, the committed-round
invariant D7 states, holding on the corpus's own numbers, not just a hand-built fixture.

### H4.4 Verification

**Mutation-proven**, per the phase's own bar. Six assertions across three existing D6 fixtures now
name the `PoisonCause` each poison site must produce
(`fu8f_retained_transpose_invalidates_on_a_generic_resolving_merge` → `Guard3Generic`;
`fu8f_retained_commit_poisons_when_it_cannot_name_the_remap` → `CommitShape` (wrong length) and
`IdentityWalk` (foreign inserted seat) and `CommitShape` (generic parent);
`fu8f_retained_commit_poisons_on_a_position_bit_tie_at_the_insertion_point` → `IdentityWalk`), each
with a doc-comment naming the exact `poison(...)` call site to delete to fail it — verified by
hand for the `Guard3Generic` case (deleting the argument's effect makes the new assertion fail
while the pre-existing `is_poisoned_for_test()` assertion still passes, proving the new assertion
is load-bearing, not redundant). Two new unit tests in `census.rs`
(`rounds_committed_matches_the_scripted_commits`, `exit_reason_and_contact_counts_are_the_most_
recent_exits_own`) pin `MergeTournamentCensus`'s pure accumulation directly — deliberately
decoupled from the process-global `Mutex<CensusData>` singleton and the `PLINTH_CENSUS` runtime
gate, because `cargo test` runs this crate's tests in parallel in one process and dozens of them
call `consolidate()`, so driving the real recorders through the shared singleton from a unit test
would let concurrently running tests' own tournaments pollute the count under test; both were
confirmed to fail when their respective accumulation logic was neutered, then restored. The
call-site wiring in `specialized.rs` is verified by the H4.3 corpus run instead, on real geometry.

Five gates green at this tip, tree clean before and after: (1) `cargo test --workspace --offline
--no-fail-fast` **1048 passed, 0 failed** (unchanged from the H3 tip — H4 adds no test under
default features, only new assertions inside existing tests); (2) the same `--all-features`
**1050 passed, 0 failed** (+2, exactly the two new `census.rs` unit tests, gated behind the
`census` feature that `--all-features` turns on); (3) `cargo clippy --workspace --all-targets
--all-features --offline -- -D warnings` clean; (4) `cargo fmt --all -- --check` clean; (5) `git
diff --check` clean. The FU-27 flake did not fire in either test run. `/private/tmp/plinth-
chitubox-preview` untouched.

## FU-8g phase H5 landed 2026-08-09 — the equivalence proof and its non-vacuity: census equals execution bit for bit, and the mutation triad says the harness is not decoration {#fu8g-h5-2026-08-09}

**Scope, stated first.** H5 is the design's separated proof phase
([`fu8g-design-2026-08-08.md`](fu8g-design-2026-08-08.md), H5 row — "folding the proof into the
phase that writes the code is how vacuous harnesses land"). **The whole diff is ONE new
integration-test file**, `crates/plinth-support/tests/penetration_census_equivalence.rs` (placed
in `plinth-support` because the harness needs both the `geom_kernel` batch API and the
`plinth-mesh` corpus fixture generators), plus this record. No production code, cap, floor,
constant, registry entry or digest input is touched; `rust/` is byte-identical to the H4 tip.

### H5.1 The equivalence harness

Four tests, one shared harness (`assert_census_matches_execution`), each run under both the
production batch shape (`n_offset_rays = 8`, `rays_all = false`) and the multi-ray shape
(`rays_all = true`):

- **`fu8g_h5_raking_blade_census_equals_execution_bit_for_bit`** — real corpus fixture
  (`make_raking_blade`). One row crosses the raked 0.03 mm blade AND the slab, so counted
  crossings exceed emitted intervals on real geometry: the count-vs-interval severance is
  exercised, not stipulated.
- **`fu8g_h5_overhang_table_hole_row_census_equals_execution_bit_for_bit`** — the corpus model
  with holes (`make_overhang_table`): two rows run horizontally at leg height through two legs
  each, and the harness asserts the axis ray yields exactly two DISJOINT intervals — a genuine
  hole, plus a leg/top coincident-face row exercising dedup identically on both sides.
- **`fu8g_h5_dense_stack_exceeds_one_hundred_crossings_on_one_ray_and_matches`** — the synthetic
  dense-crossing fixture: a 60-slab lamella stack whose full column carries **120 crossings on
  one ray** (asserted `> 100` — §M2's treant-tail regime, present by measurement, not by
  intent).
- **`fu8g_h5_over_demand_is_refused_typed_at_preflight_before_any_row_executes`** — for each of
  the six limit resources in turn, granting exactly the counted demand minus one refuses with
  the TYPED `ResourceLimitExceeded` naming that resource and the exact counted `required` —
  never an `AllocationFailure`, never a mid-execution `CensusDrift` — and granting exactly the
  counted demand admits the batch (the charge is a boundary, not a margin). The caps-level
  sibling at the UNCHANGED `ContactConstructionLimits::production()` values remains H2's
  `honest_over_demand_still_refuses_at_the_unchanged_production_caps`, green at this tip.

What the harness asserts, in order: (1) every census-derived limit is admitted by the
worst-case-derived limit for the same batch (the counted charge never exceeds the ceiling);
(2) execution under census limits is `to_bits`-identical to execution under worst-case limits,
field by field — `s_a`, `s_b`, `entered`, `exited`, `max_depth`, `ray` — so a `-0.0`/`0.0` or
NaN divergence cannot pass; (3) the batch receipt's `work` and `hits` EQUAL the census's summed
`tests` and `crossings` (the two walks measure the same quantities); (4) per row,
`census.crossings == executed hits` and `census.tests == executed work`, proven by single-row
metered calls that must also reproduce the batch row's intervals bit for bit. Every batch is
built with row 0 strictly the least-demanding row and the harness asserts that precondition —
non-vacuity for triad (c) below is a measured property of the fixture, not a hope.

### H5.2 The mutation triad — applied, observed, reverted (transcripts in the test file's doc comments)

| mutation (`rust/geom_kernel/src/queries.rs`) | observed refusal | who catches it |
|---|---|---|
| (a) `census_one` under-counts a row's `max_ray_crossings` by 1 | `CensusDrift { row: 1, resource: "hits", executed == counted + 1 }` — blade 1→2, table 7→8, dense stack 119→120 | all 4 harness tests, plus 11 of `native_api`'s 32 |
| (b) the F10 sample bound drops its `3 x` term | `CensusDrift { row: 1, resource: "samples", counted: 140, executed: 365 }` on the dense stack (121 subsegments x 3-sample floor vs a bound funding only `subsegments + ceil(seg/step)` = 140); `native_api`'s closed-form pin fails `left: 30, right: 40` | ONLY the dense-stack harness test + the one numeric pin — the blade and table tests stay green, which is exactly why the design demands the dense fixture |
| (c) every row budget replaced by row 0's budget | `CensusDrift { row: 1, resource: "hits", counted: 0, executed: 2 / 8 / 120 }` | **the harness alone: `native_api` passes 32 of 32 under this mutation.** H5's file is the sole witness for per-row budget integrity — the vacuity the H5 row was scheduled to rule out was real |

Each mutation was applied singly, observed failing with exactly the messages above, and reverted
by editing; `git diff` over `rust/` is empty at this tip. The transcripts live permanently in the
test file's module and per-test doc comments (house standard, same shape as FU-8f's and FU-31's).

### H5.3 Gates and census

Five gates green at this tip, tree clean before and after: (1) `cargo test --workspace --offline
--no-fail-fast` **1052 passed, 0 failed** (+4 from the H4 tip's 1048 — exactly the four new
harness tests); (2) the same `--all-features` **1054 passed, 0 failed** (+4 from 1050, same
four; the default/all-features gap stays 2, H4's two feature-gated census tests); (3) `cargo
clippy --workspace --all-targets --all-features --offline -- -D warnings` clean; (4) `cargo fmt
--all -- --check` clean; (5) `git diff --check` clean. The FU-27 flake did not fire in either
test run. OD-8: all seven frozen census digests reproduced **byte-identical** from a fresh
release `--features census` build (Sphere 17667468971446251870, Cylinder 13310185350867878190,
make_box 9657051231007502658, make_overhang_table 1875674282253663076, make_toed_slab
2823659516373762066, make_raking_blade 9567737006650358297, make_thin_fin 8797083662933568276).
No treant run this phase — H6 owns that, under the standing memory protocol.
`/private/tmp/plinth-chitubox-preview` untouched. **Still open: H6 only.** The treant still has
no seal.

## FU-8g phase H6 measured 2026-08-09 — the initial-contact wall is broken and the treant reaches routing for the first time; the run then EXCEEDS-TIMEBOX inside a per-terminal whole-transcript re-digest {#fu8g-h6-2026-08-09}

**Scope, stated first.** H6 is the design's measurement-and-record phase
([`fu8g-design-2026-08-08.md`](fu8g-design-2026-08-08.md), H6 row — "**no engine code**"). **No
file under `crates/` or `rust/` is touched by this phase**; `git diff` over both is empty at this
tip, and no cap, floor, constant, registry entry or digest input is raised, lowered or touched.
The whole diff is this record and its two sibling record sites. What follows is a run transcript
and its diagnosis.

### H6.1 The invocation and the protocol, verbatim

Both attempts, at branch tip `2297dae` (`feature/fu8g-penetration-census`), from
`/private/tmp/plinth-fu8g`:

```sh
PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2" PLINTH_THREADS=4 \
  cargo run --release --offline --features census \
    --example embed_incidence_census -p plinth-support
```

Standing memory protocol, applied without deviation: release build, `--features census`,
`PLINTH_THREADS=4`, RSS watchdog running **unsandboxed** and polling every 2 s with a hard kill
armed at 90 GB, a 600 s timebox, **two attempts and no more**. Machine-quiet evidence: `ps`
showed **no other `cargo`, `rustc` or `embed_incidence_census` process** at either launch; load
averages were 2.44 (attempt 1) and 3.26 (attempt 2) one-minute, driven by the terminal emulator
alone. The binary was the fingerprint-verified release artifact at this tip (cargo reported
`Finished` with nothing to rebuild). Machine: 16 cores, 128 GB.

**One deliberate, recorded deviation, confined to attempt 2.** Attempt 1 ran the protocol
untouched and produced a stopping mode but **no counters at all** (§H6.6). Because the design's
own H6 row requires the record to "diagnose where the time went", attempt 2 — the last attempt
permitted — ran the *identical* invocation under the *identical* watchdog and added **external
stack sampling only**: nine 3-second `sample(1)` windows at t = 20, 60, 120, 200, 280, 360, 440,
520 and 575 s. `sample` observes the process from outside; **no instrumentation entered the tree**
and the `git diff` claim above is unaffected. The perturbation is ≤27 s of light sampling over
600 s, and it can only make a timebox verdict *later*, never earlier — both attempts hit the same
wall, so the verdict is not at risk. Attempt 1 is therefore the clean wall-clock of record and
attempt 2 is the diagnosis.

### H6.2 The outcome, named: EXCEEDS-TIMEBOX, twice, reproducibly

| | attempt 1 | attempt 2 |
|---|---|---|
| stopping mode | **EXCEEDS-TIMEBOX (600 s)** | **EXCEEDS-TIMEBOX (600 s)** |
| wall-clock at kill | 600 s | 601 s |
| exit | SIGKILL (137), watchdog | SIGKILL (137), watchdog |
| peak RSS | **3,693 MiB** at t = 594 s | **3,721 MiB** at t = 599 s |
| watchdog 90 GB arm | **never approached** | **never approached** |
| stdout census rows | **none** (§H6.6) | **none** (§H6.6) |
| stage at kill | routing (inferred from RSS shape) | **routing, sampled directly** |

This is outcome (3) of the four the phase was briefed for, and the one the design ranked **most
likely at ~45 %**. It is a valid, recordable result. It is **not** a seal, **not** a typed refusal
and **not** a watchdog kill.

### H6.3 The historic part: `build_initial_contacts` COMPLETES, and the treant reaches routing

**No treant run has ever done this before.** Every prior attempt died upstream:

| tip | treant stopping point |
|---|---|
| `fe79124` (FU-8f G6c) | merge gate, then `Initial(Limit { resource: "work items", required: 232585208, limit: 4000000 })` — **58.1× the cap** |
| `develop @ e3168c5` | `Specialized(DuplicatePhysicalSeat { left: 163, right: 164 })`, three stages earlier (FU-31) |
| `55b4d99` (FU-31 fixed) | `Initial(Limit { resource: "work items", required: 237305572, limit: 4000000 })` at 553 contacts |
| **`2297dae` (this run)** | **Preparation completes. Routing does not.** |

At t = 200 s the main thread is **100 % inside `plinth_support::initial::build_initial_contacts`**
(`run_support + 4240` → `build_initial_contacts + 132` → `SpecializedContactPlan::validate_binding`
→ `GenericContactPlan::validate_binding` → `geom_kernel::queries::NativeSurface::exterior_face_mask`,
1997 of 1997 main-thread samples). By t = 280 s it is 100 % inside
`PreparedSupportRun::route_initial_cco`. Between those two windows, at **t = 267 s** (attempt 2)
and **t = 272 s** (attempt 1), RSS releases ≈250 MiB in one 2 s poll (2,380 → 2,130 MiB and
2,382 → 2,156 MiB) — the initial-contact scratch being freed. That is the
**Preparation → Routing boundary**, reproduced in both attempts within 5 s.

**So the FU-8g preflight passed on honest numbers, on the treant, at 429,120 faces.** H2 measured
the charge at **108,378 against the untouched 4,000,000 floor (2.71 %)**; this run is the
end-to-end confirmation that the number is not merely smaller but *sufficient* — the stage
executed to completion, `CensusDrift` never fired, and nothing downstream of the census refused.
Contact attestation, which the same stage carries, also completed. **D5.3's cap-scaling
contingency is therefore NOT triggered and is NOT added** — exactly as the design demanded, and
for the reason it demanded: no measurement requires it.

### H6.4 Where the time went — a per-terminal re-digest of the whole accumulated transcript

Main-thread sample attribution, sibling frames summed, attempt 2:

| t (s) | total | dominant stage | `grow_terminal_with_finalized_anchor` | `CcoController::digest` | `validate_replay` |
|---:|---:|---|---:|---:|---:|
| 20 | 2283 | decimate / hollow / topology | — | — | — |
| 60 | 1992 | `perception::prepare_generic_contacts` **100 %** | — | — | — |
| 120 | 1948 | `specialized::prepare_specialized_contacts` **100 %** | — | — | — |
| 200 | 1997 | `initial::build_initial_contacts` **100 %** | — | — | — |
| 280 | 2254 | `route_initial_cco` **100 %** | 2200 (97.6 %) | 443 (**19.7 %**) | 699 (31.0 %) |
| 360 | 2265 | `route_initial_cco` **100 %** | 2265 (100 %) | 1458 (**64.4 %**) | 312 (13.8 %) |
| 440 | 2268 | `route_initial_cco` **100 %** | 2268 (100 %) | 1751 (**77.2 %**) | 203 (9.0 %) |
| 520 | 2278 | `route_initial_cco` **100 %** | 2278 (100 %) | 1758 (**77.2 %**) | 204 (9.0 %) |
| 575 | 2263 | `route_initial_cco` **100 %** | 2263 (100 %) | 1744 (**77.1 %**) | 204 (9.0 %) |

**The quadratic is visible in the profile itself, not merely argued from source.**
`CcoController::digest`'s share of the routing stage **rises** 19.7 → 64.4 → 77.2 → 77.2 → 77.1 %
as the transcript accumulates, while `validate_replay`'s share **falls** 31.0 → 13.8 → 9.0 → 9.0
→ 9.0 %. That is a growing term overtaking a constant one — the measured signature of
`Θ(N²)` beside `Θ(N)` — and the plateau near 77 % is the asymptote, not a sampling artefact: it
reproduces across three windows 135 s apart.

**Derived to the line from source, independently of the profile:**

- `runner.rs:1650–1666` — routing is a plain `for terminal in cadence` loop, one
  `controller.grow_terminal_with_finalized_anchor(...)` per terminal.
- `cco.rs:1397` — the **first statement** of that function is
  `let controller_digest_before = self.digest();`, once per terminal.
- `cco.rs:1775–1862` — `CcoController::digest` is **not** the cached `const fn` at `:863`; it
  walks the entire accumulated controller state every call: `self.terminal_cadence` (every
  terminal so far, `:1786`), `self.terminals` (every record so far, `:1790`), and for each
  `UnroutedNoWinner` record it calls `digest_anchor_seat_refusals` (`:1818`, `:1824`, `:1834`)
  and `digest_anchor_refusals` over **that terminal's entire refusal list**, then
  `self.hosts.indexed_members` (`:1851`) and `self.hosts.samples` (`:1856`) — the whole kd-tree
  host sample set.
- `cco.rs:2128–2149` — `digest_anchor_seat_refusals` emits five words per refusal, linearly.

So the routing stage costs

```
Σ_{k=1..N} [ k  +  Σ_{j<k} (seat_refusals_j + anchor_refusals_j)  +  |hosts.indexed_members|  +  |hosts.samples| ]
  =  Θ(N²)  +  Θ(N · R)  +  Θ(N · H)
```

for `N` terminals in the cadence (553 specialized physical contacts survive the tournament per
FU-31, plus the regular terminals appended at `runner.rs:1649`), `R` the total accumulated
anchor-seat and anchor refusals, and `H` the host kd-tree sample count. The dominance of
`digest_anchor_seat_refusals` in every routing window says the `Θ(N · R)` term is the large one on
this model — consistent with the standing record that **0 of 3429 attempted model anchors survive
corpus-wide**, which makes refusal lists long and `UnroutedNoWinner` common.

**This is the batch-first violation family a sixth time** — after FU-8b, FU-8c, FU-8d, FU-8e and
FU-8g's own `contacts × faces` bound. It is the same defect shape in a new dress: *a per-item call
over a whole accumulated structure inside a hot loop*. It is **not** a cap, nothing refuses, and
**no cap would fix it**.

**A second, independent per-terminal re-digest sits in the same loop** and accounts for the ~9 %
constant term: `FinalizedAnchorValidationContext::validate_replay` →
`validate_finalized_anchor_route` → `BoundPhysicalAnchorContact::new` →
`SpecializedContactPlan::digest` → `GenericContactPlan::digest` → `ContactCandidatePool::digest`;
and `SurfaceAnchorPool::validate_against` → `anchor::surface_anchor_pool_digest`; and
`FinalizedContactLoads::validate_binding` → `perception::finalized_load_rows_fingerprint`. Every
one of those structures is **immutable for the entire routing stage**, and every one is re-hashed
from scratch once per terminal. Together the two re-digest families account for **≈86 %** of the
routing stage's wall-clock at t = 280 s and **≈86 %** at t = 575 s.

**Opened as [FU-32](#follow-ups-2026-08-04).** Naming the remedy is not this phase's business, but
the shape of the defect bounds it: both terms are re-derivations of state whose deltas are already
known at the call site, and the house precedent (G3's prefix evaluation, G6's cross-round
retention) is incremental maintenance, not a bigger budget.

### H6.5 Memory — H1's reserve collapse, confirmed at treant scale

The design predicted `pen_intervals` alone at **14–32 GB under the old reserves** and **~MB under
H1's**, and instructed that RSS climbing toward 90 GB would mean something in the new reserve
logic was wrong. **Nothing climbed.** The *entire process* peaked at **3,721 MiB** — 4.0 % of the
90 GB watchdog arm, 3.7 % of the ~100 GB working ceiling, 2.9 % of the 128 GB machine. The
watchdog never fired and was never close to firing. H1's per-row census budgets are confirmed on
the treant, end to end, at 429,120 faces.

The one genuine memory growth is in routing and is honestly rows-scaled, not a reserve: RSS rises
from 2,130 MiB at the Preparation→Routing boundary to 3,721 MiB at kill, **+1,591 MiB over 332 s
≈ 4.8 MiB/s**, as per-terminal transcript rows and their refusal vectors accumulate. That is the
same structure the `Θ(N · R)` term walks. It is the design's third-ranked risk ("memory in
brace/transit") appearing in a stage A3 did not name, at roughly a sixtieth of the feared
magnitude, and it binds nothing.

### H6.6 FINDING — an EXCEEDS-TIMEBOX run is structurally counter-free, so H4's rider could not fire

The design's H6 row and §A3 both treat "**EXCEEDS-with-counters**" as the valid degraded outcome,
and H4 landed its rider (`rounds_committed`, `tournaments`, `contacts_first`, `contacts_last`,
`exit_reason`, and the three-way `PoisonCause` split) precisely so that "the next transcript
answers a question two records currently have to disclaim". **On this run it could not, and the
reason is structural, not incidental:**

- `plinth_support::census` prints **only** from `census::finish` (model completed) and
  `census::report_failure` (model refused with a typed error), invoked at
  `crates/plinth-support/examples/embed_incidence_census.rs:56–62` and `:154`.
- There is **no signal handler** anywhere in `census.rs` or in the example — verified by grep for
  `signal` / `SIGTERM` / `SIGINT` / `ctrlc`; the census accumulates in memory and is flushed once,
  at the end.
- The watchdog's kill is a `SIGKILL`. **Both attempts therefore emitted zero `CENSUS` rows** — the
  entire stdout is `Finished ... / Running target/release/examples/embed_incidence_census`.

Consequences, recorded plainly: the treant's **21 poison-driven rebuilds remain unattributed**;
`rounds_committed` and `exit_reason` remain uncaptured on the treant; and the honest per-cap demand
this phase was sent to report exists only for the caps H0/H1/H2 already measured offline. **"EXCEEDS
with counters" is not currently achievable with this harness** — it is achievable only for outcomes
that end in a typed refusal or a seal. This is a defect in the measurement apparatus, not in the
engine, and it should be the *first* thing fixed before the next treant run, because the outcome it
blinds is the one the design itself ranked most likely. Recorded on **FU-32** as its first action.

### H6.7 The design's next-binder prediction, scored

§A3 ranked four binders. Scored against this run:

| # | A3's binder | confidence | outcome |
|---|---|---:|---|
| 1 | **Wall-clock — the 600 s timebox** | ~45 % | **LANDED.** Both attempts EXCEEDS at 600 s. |
| 2 | A typed refusal in an unregistered brace/joints/raft cap | ~30 % | **NOT REACHED** — the run never left routing; brace was never entered. Untested, not falsified. |
| 3 | Memory in brace/transit | ~15 % | **NOT REACHED**, and its premise is contradicted at every stage that *did* run (peak 3.7 GiB). |
| 4 | A clean seal | ~10 % | **NO.** |

**The verdict landed; the mechanism did not.** §A3 predicted the timebox would bind *because* "the
brace/transit penetration batches are the expensive `rays_all = true` shape". The measured cause
involves **no penetration at all** — it is a whole-transcript re-digest in CCO routing, a stage A3
listed by name but never suspected. Scoring the prediction as a hit on the outcome alone would be
the dishonest reading; it is a **hit on the call and a miss on the cause**, and the miss is the
useful half. §A2's running scorecard becomes **2 landed of 5**.

### H6.8 Gates, census and containment

Five gates green at this tip, verbatim, tree clean before and after:
(1) `cargo test --workspace --offline --no-fail-fast` — **1052 passed, 0 failed**;
(2) the same `--all-features` — **1054 passed, 0 failed**;
(3) `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` — clean;
(4) `cargo fmt --all -- --check` — clean;
(5) `git diff --check` — clean.
**The FU-27 environmental flake (`full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads`) did not fire in either test run**, so no re-run was needed. **Count attribution, exactly:** 1052/1054 is unchanged from the H5 tip, and 1052 = 1042 (the FU-31 tip named in this branch's standing protocol) + 6 (H4's `PoisonCause` and `MergeTournamentCensus` assertions) + 4 (H5's equivalence harness); all-features 1054 = 1042 + 8 + 4, the default/all-features gap of 2 still being H4's two feature-gated `census.rs` unit tests. **H6 adds zero tests and moves zero counts**, which is what a no-engine-code phase must do.
OD-8: all seven frozen census digests reproduced **byte-identical** from the release `--features census` build at this tip, one `PLINTH_CENSUS_ONLY` invocation each — Sphere 17667468971446251870, Cylinder 13310185350867878190, make_box 9657051231007502658, make_overhang_table 1875674282253663076, make_toed_slab 2823659516373762066, make_raking_blade 9567737006650358297, make_thin_fin 8797083662933568276. The Weeping Treant is deliberately excluded from the digest set: it has never sealed and produces no snapshot digest.
`git diff` over `crates/` and `rust/` is empty at this tip — H6 changed no engine code, by design.
`/private/tmp/plinth-chitubox-preview` untouched. Nothing was pushed.

**Where FU-8g stands.** H0–H5 are complete and H6 is measured. The follow-up's own thesis — honest
work, honest charge, honest reserve — is confirmed end to end on the treant: the wall FU-8g was
opened to fix is **gone**, the charge that replaced it is **sufficient and 2.71 % of an untouched
cap**, and the reserves that were predicted at tens of gigabytes are **megabytes**. **The treant
still has no seal**, and what stops it now is named, derived to the line, and opened as FU-32.

## FU-32 phase J3 attributed 2026-08-09 — the controller-digest movement is DECISION-NEUTRAL, proved from source AND parent-vs-tip on 687,288 dumped routing-output lines; NOTHING is re-pinned {#fu32-j3-2026-08-09}

**No engine code.** This phase adds one document section and one ledger addendum. `git diff` over
`crates/` and `rust/` is empty at this tip, and the two digest-pinned tests J2 left failing are
still failing, still by design, still with their old expectations untouched. **This is FU-10
protocol step 2 and nothing else**: *"a golden is not re-pinned until every missing member is
attributed"* (`CURRENT-STATUS.md:620`), attribution lands as evidence **before** the ruling is
sought, and the ruling is sought **before** anything is re-pinned
([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)).

### J3.1 — The claim, stated so it can be falsified

> J2 (`c14754e`) moves exactly three named u64 quantities and everything downstream of them:
> `TerminalRouteEvidence::controller_digest_before`,
> `TerminalDisposition::SkippedQuarantined::controller_digest_before` (and its sealed copy
> `FinalizedSkippedEvidence::controller_digest_before`), and
> `FinalizedCcoTranscript::final_controller_digest`. **No other observable of a support run
> changes on any corpus model** — not one routed contact, seat position, route membership, live
> member, coverage sample, named island, load, refusal, disposition or `NoWinnerEvidence` payload.

Falsifiable two ways, and both were run: (a) if any production site *compared* a moved value, the
movement could steer a decision — §J3.2 audits all 30 occurrences; (b) if any output field other
than the named ones differed parent-vs-tip, the claim is simply false — §J3.3 dumps and diffs every
one of them on all seven models.

### J3.2 — Proof (a), from source at THIS tip: the value is an input to no decision

The design pre-argued this at `bfa5b3b` (§F3, 24 grep hits). **Re-verified at `c14754e`, not
carried forward.** `grep -rn controller_digest_before crates/ rust/` now returns **30** hits, all
in `crates/plinth-support/src/cco.rs`; the +6 are entirely J2's own test apparatus (the retained
`digest_full_walk_oracle` and the mutation triad). Classified by enclosing item:

| class | count | sites (`cco.rs`) | what it does |
|---|---:|---|---|
| field / variant declarations | 3 | `:635`, `:691`, `:718` | storage |
| accessor `FinalizedSkippedEvidence::controller_digest_before()` | 2 | `:728`, `:729` | read-only; **its only caller in the workspace is a test** (`:5508`) |
| write sites (production) | 4 | `:1659`, `:1699`, `:1712`, `:1751` | `self.digest()` captured once per cadence entry and stored, on both the grown and the quarantined arm |
| constructor plumbing `route_evidence` | 2 | `:2806`, `:2816` | stores |
| copy-forward into the sealed row | 2 | `:2497`, `:2518` | copies |
| **reads — every one a hash fold, none a comparison** | 4 | `:2338`, `:2345` (`digest_terminal_record` → `records_acc`), `:2416` (`digest_route_evidence`), `:2578` (`transcript_rows_digest`) | `word(...)` into an FNV stream |
| doc comment | 1 | `:2155` (on `sealed_digest_matches_rebuilt`, `:2160`) | prose |
| `#[cfg(test)]` items inside the module body | 4 | `:1599`, `:1625` (`grow_terminal_for_test`), `:2209`, `:2216` (`digest_full_walk_oracle`) | test-only |
| `mod tests` (`:4452`+) | 8 | `:5418`, `:5500`, `:5508`, `:5548`, `:5890`, `:5961`, `:5977`, `:6238` | test-only |

**Zero equality tests, zero ordering, zero branching, zero arithmetic on the value outside
`#[cfg(test)]`.** The decisive site is `finalize_transcript_row`'s `checked_route` closure
(`cco.rs:2457–2466`), which is the transcript's own admission check: it validates
`physical_contact_id`, `terminal_id`, `original_key`, `plan_digest`, `loads_digest` and
`geometry_digest` — **six fields, and deliberately neither `controller_digest_before` nor
`forest_digest_before`.** A row is admitted or refused without ever reading the moved value.

**Extended, as the task required, to every consumer of the moved values** — the design's §F4 chain,
re-walked at this tip. Every one is a fold, a same-run recompute-and-compare, or a printed record:

| consumer | site | what it does with the moved value | can it move a decision? |
|---|---|---|---|
| `records_acc` / `digest()` | `cco.rs:2324`, `:2089`, `:2097` | folds it into the controller digest | no — output only |
| `transcript_rows_digest` → `ordered_disposition_digest` | `cco.rs:2547`, `:1562` | folds | no |
| `finalized_transcript_digest` → `transcript_digest` | `cco.rs:2617` | folds `final_controller_digest` + `ordered_disposition_digest` | no |
| `FinalizedCcoTranscript::validate_integrity` | `cco.rs:868–869` | recomputes both from the same rows and compares | no — self-consistency at one tip |
| **brace tripwire** | `runner.rs:2775` → `sealed_digest_matches_rebuilt` `cco.rs:2160` | `rebuild_accumulators()` (`cco.rs:2128`) from scratch, compared to the sealed value **and** to the cached pair | no — compares a value to itself recomputed in the same run |
| **rewire tripwire** | `runner.rs:2848` → same | same | no |
| `SupportVerificationSnapshot.transcript_digest` → `verification_snapshot_digest` → `snapshot_digest` | `runner.rs:2560`, `:3459` | folds (1 of 17 inputs) | no |
| `SupportVerificationSnapshot::validate_integrity` | `runner.rs:1342` | recomputes and compares | no |
| `plinth_verify::verify_coverage` | `plinth-verify/src/lib.rs:153` | `measurement.snapshot_digest() != snapshot.digest()` — binds a measurement to the snapshot it was taken from | no — same run, same tip |
| transit-measurement bindings | `transit_measurement.rs:983`, `:988`, `:1711` | same-run binding checks | no |
| `RewireCcoBinding` | `cco.rs:3400` | same-run binding check | no |
| census / `first_artifact` reporting | `examples/embed_incidence_census.rs:206`, `plinth-pipeline/examples/first_artifact.rs:106` | prints | no |
| test literals + one golden JSON | see §J3.6 | pins | **these are the re-pin sites, and they are the entire cost** |

Two riders worth stating because a reader will ask:

- **The PR-8/OD-9 seat-attestation chain is untouched and provably independent.**
  `extend_contact_attestation` (`cco.rs:2015–2022`) folds only its own prior head, the terminal id
  and the stage — never the controller digest — so `contact_attestation_digest` cannot move with
  it. The live witness is that the rewire preflight's attestation comparison
  (`runner.rs:2830`) passes on all seven models at the tip.
- **The F6 tripwires still genuinely recompute.** `sealed_digest_matches_rebuilt` calls
  `rebuild_accumulators()` and never reads the cached fields, and additionally cross-checks
  cached-vs-rebuilt, so it now catches a *second* failure mode (a drifted accumulator) that the
  pre-J2 walk could not express. The design cited these sites as `runner.rs:2763` / `:2832` at
  `bfa5b3b`; at this tip they are `runner.rs:2775` / `:2848`. **The semantics are unchanged —
  recompute-from-state and compare — which the standing invariant requires.**

### J3.3 — Proof (b), measured: parent-vs-tip routing-output dumps, all seven corpus models

**Method (the design's §M6 recipe, ephemeral by construction).** A throwaway
`git clone --shared` was checked out at the **parent** (`059b444`, J1) and at the **tip**
(`c14754e`, J2), instrumented at both with byte-identical dump code — a full field-by-field
`{:?}` of every finalized transcript row and every transcript header digest at
`finalize_transcript`, plus every one of `verification_snapshot_digest`'s inputs and a `{:?}` of
every routed contact, route membership, final live member, coverage sample and named island at
the seal — and run over the corpus with `PLINTH_CENSUS_ONLY` per model, release,
`--features census`, `PLINTH_THREADS=4`. The scratch tree, its binaries and the dumps are **not
in git** and never will be (the standing G4 rule); the recipe above is the durable artefact.
The parent build reproduced the seven **frozen OD-8 digests byte-identical**, and the tip build
reproduced J2's seven **new** digests byte-identical — so both sides of the diff are the values of
record, independently re-measured, not values copied from a commit message.

| model | rows | Routed | Skipped | NoWinner | retained seat refusals | content lines compared | **row diffs after redacting `controller_digest_before` only** | **content-line diffs** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Sphere | 7 | 7 | 0 | 0 | 0 | 9,214 | **0** | **0** |
| Cylinder | 54 | 54 | 0 | 0 | 0 | 509,285 | **0** | **0** |
| make_box | 11 | 11 | 0 | 0 | 0 | 12,337 | **0** | **0** |
| make_overhang_table | 84 | 73 | 4 | 7 | **293** | 50,592 | **0** | **0** |
| make_toed_slab | 68 | 64 | 2 | 2 | **152** | 50,995 | **0** | **0** |
| make_raking_blade | 33 | 27 | 4 | 2 | **46** | 34,308 | **0** | **0** |
| make_thin_fin | 18 | 17 | 0 | 1 | **22** | 20,557 | **0** | **0** |
| **total** | **275** | **253** | **10** | **12** | **513** | **687,288** | **0** | **0** |

Read that as three separate findings:

1. **Only three line kinds differ at all.** Across all seven dumps the differing lines are
   exclusively `ROW`, `TRANSCRIPT` and `SNAPSHOT`. **Not one** `CONTACT`, `MEMBERSHIP`, `LIVE`,
   `COVSAMPLE` or `ISLAND` line differs — 687,288 lines of actual routing output, byte-identical.
2. **Within the rows, the redaction is the whole delta.** Replacing `controller_digest_before: N`
   with a placeholder — a mechanical substitution whose scope is exactly the two named fields —
   makes all 275 transcript rows byte-identical parent-to-tip, with their winners, families,
   prices, clearance rows, dispositions and complete `anchor_seat_refusals` lists intact. The
   refusal path is genuinely exercised (513 retained refusals across four models, reproducing the
   design's §M5 counts 293 / 152 / 46 / 22 exactly), so this is not a vacuous diff over models
   that never build refusal evidence.
3. **Within the headers, exactly the predicted fields move.** Of the transcript's 10 header
   digests, **3** move on every model — `final_controller_digest`, `ordered_disposition_digest`
   (which folds the rows, and the rows carry the moved field) and `transcript_digest` (which folds
   both). `plan_digest`, `finalized_load_digest`, `load_partition_digest`, `geometry_digest`,
   `initial_forest_digest`, `initial_post_quarantine_forest_digest` and `final_forest_digest` are
   **unmoved on all seven models — the forest itself is bit-identical.** Of
   `verification_snapshot_digest`'s **17** inputs, **exactly 1** moves (`transcript_digest`);
   `mesh_digest`, `topology_digest`, `plan_digest`, `load_digest`, `geometry_digest`,
   `forest_digest`, `regular_digest`, `coverage_digest`, `named_islands_digest`,
   `lowered_primitives_digest`, `routed_projection_digest`, `final_live_members_digest` and all
   four count fields are unmoved.

### J3.4 — Proof (c), an in-workspace witness that needs no scratch tree

The failing `public_coverage_report_is_frozen_without_evidence_loss` prints the frozen golden and
the tip's report in full. Flattened to scalar leaves and diffed mechanically: **185 leaves, exactly
1 differs — `snapshot_digest`.** Every `request` bound, every one of the 11 `resources` counters,
all 9 islands with their face ids, all 9 patches (`uncovered_area_mm2`, `cut_ambiguous`,
`assessment_failed`, `worst_gap`) and all 9 defects are byte-identical. This is an *independent*
witness on a different path — the procedural `make_overhang_table` through coverage measurement and
`plinth-verify` — and it reproduces §J3.3's conclusion inside the committed test suite, with no
instrumentation at all.

### J3.5 — Why no value-preserving alternative was sought again

Not re-litigated, and not asserted on authority either: the design proved it twice over (§F5). FNV-1a
mixes GF(2)⁶⁴ XOR with Z/2⁶⁴ multiplication and therefore admits no composition law, so no rolling
combination of a retained tail with a new prefix exists **for this algorithm**; and the stream's
first segment is `(inserted, |terminals|, |cadence|)`, which takes a new value at the **front** on
every call, while `terminals` is a `BTreeMap` iterated in terminal-id order against cadence-order
insertion, so records splice mid-stream. Either obstruction alone kills retention. The movement is
not a choice about how to write the fix; it is a property of the digest that was there before.

### J3.6 — The from→to table, and the per-model routing-output verdict

Measured at `c14754e` from a release `--features census` build, one `PLINTH_CENSUS_ONLY`
invocation per model; the tip figures below were reproduced twice (J2's own sweep and this
phase's independent instrumented sweep) and agree byte for byte.

| model | frozen OD-8 `snapshot_digest` (base) | measured at `c14754e` | routed contacts | final live members | **routing-output identity verdict** |
|---|---|---|---:|---:|---|
| Sphere | `17667468971446251870` | `2297157889062365577` | 7 → 7 | unchanged | **IDENTICAL** — 9,214 content lines, 7 rows, 0 diffs |
| Cylinder | `13310185350867878190` | `6853793710942395608` | 54 → 54 | unchanged | **IDENTICAL** — 509,285 content lines, 54 rows, 0 diffs |
| make_box | `9657051231007502658` | `8709076999423814412` | 11 → 11 | unchanged | **IDENTICAL** — 12,337 content lines, 11 rows, 0 diffs |
| make_overhang_table | `1875674282253663076` | `16396858890798947253` | 73 → 73 | unchanged | **IDENTICAL** — 50,592 content lines, 84 rows incl. 7 no-winner and 293 refusals, 0 diffs |
| make_toed_slab | `2823659516373762066` | `133000971864625681` | 64 → 64 | unchanged | **IDENTICAL** — 50,995 content lines, 68 rows incl. 152 refusals, 0 diffs |
| make_raking_blade | `9567737006650358297` | `18388631460456455184` | 27 → 27 | unchanged | **IDENTICAL** — 34,308 content lines, 33 rows incl. 46 refusals, 0 diffs |
| make_thin_fin | `8797083662933568276` | `14656364680512457190` | 17 → 17 | unchanged | **IDENTICAL** — 20,557 content lines, 18 rows incl. 22 refusals, 0 diffs |

**All seven move. All seven models produce bit-identical support.** The `routed_contacts` and
`final_live_members` counts in the snapshot dumps are unmoved on every model, which is the same
claim the census line makes and is stated here from the dump rather than from the census.

### J3.7 — Every site that will need re-pinning at J4, enumerated

**Code — three pinned values in two files, plus one provenance comment.** The enumeration is not a
guess: gates 1 and 2 ran the entire workspace with `--no-fail-fast` and **exactly two tests fail**,
so the set of digest pins this movement disturbs is closed by measurement. Corroborated by sweeping
every ≥18-digit literal in `plinth-support`, `plinth-verify` and `plinth-pipeline` — the remaining
hits are `f64` `to_bits` geometry fixtures (`support_runner_integration.rs:331–411`),
plan/source-authority digests in `specialized.rs` (upstream of routing, unmoved and green),
`penetration_census_equivalence.rs:421`/`:467` which cite two census digests **in doc comments as
model identifiers only**, and `plinth-pipeline`'s `first_artifact` test, which pins **no** digest by
deliberate design (`crates/plinth-pipeline/tests/first_artifact.rs:12`):

| # | site | pinned now | measured at `c14754e` | test that fails today |
|---|---|---|---|---|
| 1 | `crates/plinth-support/tests/support_runner_integration.rs:137` (`make_toed_slab`, procedural) | `13_346_779_153_602_682_749` | **`2198459883570957390`** | `public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing` |
| 2 | `crates/plinth-support/tests/support_runner_integration.rs:144` (`make_raking_blade`, procedural) | `8_660_653_339_934_460_766` | **`74030269957898085`** | same test |
| 3 | `crates/plinth-verify/tests/coverage_gate_overhang_table.json` → `"snapshot_digest"` | `5470664714994646368` | **`3706765241747593242`** | `public_coverage_report_is_frozen_without_evidence_loss` |
| 4 | `crates/plinth-verify/tests/coverage_gate.rs:16–37` — the golden's provenance comment block | (prose) | needs a **new dated paragraph**, appended below the existing two refresh records, which stand unamended | — |

The counts those two tests assert alongside the digests are **unchanged and must stay unchanged**:
`make_toed_slab` 64 / 170, `make_raking_blade` 27 / 72, and the coverage report's 185-leaf body.
**A J4 re-pin that also moves a count is not this movement and must stop.**

**One pinned digest that must NOT be re-pinned, and the reason matters.**
`verification_snapshot_digest_input_list_is_pinned` (`runner.rs:4976–5014`) asserts
`snapshot.digest() == 11_833_545_896_309_364_349` over a synthetic fixture whose
`transcript_digest` is `0`. It is **green at this tip**, and that is a finding rather than an
accident: it is the guard that fires when the snapshot digest's *input list* widens, and its
staying green is independent confirmation that J2 changed the **value of one input**, not the shape
of the digest. If a J4 re-pin ever needs to touch this test, the change being re-pinned is not this
one.

**Doc record sites — dated addenda only; the FU-10 rule is that no earlier citation is rewritten**
(`CURRENT-STATUS.md:3697`: *"Everything written before this section stands unamended"*). The seven
base values occur **107 times across 9 files** (the ninth being `coverage_gate.rs`'s provenance
comment, item 4 above); every one of them is a *dated measurement of its own date* and stays
exactly as written. The sites that a J4 sweep must visit — because a reader would otherwise take
them for a current pin rather than a historical one — are:

| file | occurrences | what J4 owes it |
|---|---:|---|
| `docs/rust-migration/CURRENT-STATUS.md` | 64 | the FU-32 ledger row (`:5244`) and a new dated re-pin section carrying the from→to table; **no existing line edited** |
| `docs/rust-migration/support-resource-caps-2026-08-05.md` | 8 | a dated addendum at §4's canonical table (`:209–216`) and at the OD-8 restatement (`:2347`) |
| `docs/rust-migration/slicer-union-design-2026-08-07.md` | 15 | a dated addendum noting the FU-10 phase-C from→to tables (`:599–605`, `:700–706`) have been superseded by a second, unrelated movement |
| `docs/rust-migration/decimation-design-2026-08-06.md` | 10 | dated addendum (`:510`, `:798`, `:895` cite `Cylinder`) |
| `docs/rust-migration/census-embed-incidence-2026-08-04.md` | 5 | dated addendum (`:406`, `:429`) |
| `docs/rust-migration/export-path-design-2026-08-07.md` | 1 | `:525` calls Sphere's digest "bit-for-bit the frozen OD-8" — needs the dated note |
| `docs/rust-migration/contact-physics-2026-08-04.md` | 1 | `:261` table row |
| `crates/plinth-support/tests/penetration_census_equivalence.rs` | 2 | `:421`, `:467` cite two digests **in doc comments as model identifiers**; prose only, no assertion |
| `docs/rust-migration/fu32-design-2026-08-09.md` | — | **NOT amended.** It is a design landed verbatim under a provenance banner and is a record of 2026-08-09 at `bfa5b3b`. |

**Goldens: there is exactly one golden file in the workspace**
(`crates/plinth-verify/tests/coverage_gate_overhang_table.json`) and it is item 3 above.

### J3.8 — Gates at this tip

Tree clean before and after. (1) `cargo test --workspace --offline --no-fail-fast` — **1058
passed / 2 failed**; (2) the same `--all-features` — **1063 passed / 2 failed**; (3)
`cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` — clean; (4)
`cargo fmt --all -- --check` — clean; (5) `git diff --check` — clean. **The two failures are
exactly the two J2 recorded, by name and no others** —
`public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing` and
`public_coverage_report_is_frozen_without_evidence_loss` — which is the invariant this phase was
required to hold: **a new failure would have been a blocker, and there is none.** The FU-27
environmental flake did not fire, so no re-run was needed. **J3 adds zero tests and moves zero
counts**, which is what a no-engine-code phase must do.

### J3.9 — What this attribution does NOT claim

Stated plainly, because an attribution that overreaches is worse than none:

- **The Weeping Treant is not part of this proof.** J2's run sealed it for the first time
  (`snapshot_digest=423276951048565625`), but that is a **first measurement, not a pin** — no
  golden for that model has ever existed, and there is no before-value to diff against. J6
  measures the treant; it does not prove J2.
- **The corpus is the witness, deliberately** (design OQ-2). All seven models exercise
  `route_initial_cco` non-trivially and four of them exercise the refusal path, which is what the
  claim needs; a 600 s treant run is kept off the verification critical path on purpose.
- **This is a J1→J2 diff.** J1 was separately proved byte-identical to the base septet, and the
  parent side of this diff re-measured those seven frozen values byte-identical, so the chain back
  to `bfa5b3b` is closed — but the field-by-field dump comparison itself is parent-to-tip.
- **No behavioural claim is made about the digest's *purpose*.** It remains a hash-chain link that
  binds prior history into each transcript row; it binds a *different* stream now, and whether that
  is acceptable provenance is precisely the owner's question, not this document's.

**NO CAP, floor, constant, registry entry or digest input was raised, lowered or touched. No
golden re-pinned, no doc table rewritten, no test expectation relaxed.**

**AWAITING OWNER RULING — nothing re-pinned.**

## Owner ruling: the FU-32 J2 controller-digest movement is RE-PINNED, all seven (2026-08-09) {#fu32-j4-2026-08-09}

Asked whether the value-changing digest fix may proceed given that **all seven OD-8
fingerprints move**, the owner ruled on 2026-08-09. Verbatim, in full:

> The fingerprints aren't sacred  if things change slightly that's to be expected (because the
> algorithm wasn't right when the fingerprints were generated!)

The ruling was sought **after** the attribution and **before** any re-pin, which is FU-10
protocol order ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). The evidence it was made
against is [`#fu32-j3-2026-08-09`](#fu32-j3-2026-08-09), and the one-line version of that
evidence is the whole reason this ruling is a small one: **687,288 dumped routing-output
content lines and 275 transcript rows are byte-identical parent-to-tip across all seven corpus
models — zero diffs — and the only quantities that move are observational digests** (the three
transcript headers that fold the moved controller digest, and the one snapshot input that folds
those). Not one routed contact, seat position, route membership, live member, coverage sample,
named island, load, refusal or disposition differs on any model. The owner's phrase "change
slightly" is, in this instance, an over-statement of the delta: what changed is the *value of a
hash*, not the support.

### J4.1 — The new OD-8 authority table

**Everything written before this section stands unamended.** Every earlier citation of
`17667468971446251870`, `13310185350867878190`, `9657051231007502658`, `1875674282253663076`,
`2823659516373762066`, `9567737006650358297` or `8797083662933568276` — 107 occurrences across
9 files — remains correct **as a dated record of what was frozen at the time it was written**,
including the 2026-08-08 table at [`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08), which was
the authority until this line. The same holds for the three in-workspace pins that moved with
them — the seal-test tuples `13346779153602682749` / `8660653339934460766` and the frozen
coverage report's `5470664714994646368` — wherever they are cited, including that section's
re-pin table and the verbatim-landed evidence record
[`attribution-slicer-census-2026-08-07.md`](attribution-slicer-census-2026-08-07.md), which is
**not** amended (it sits under a provenance banner and is a record of 2026-08-07).
**The live values are the table below.**

Re-measured at this tip from a release `--features census` build, one `PLINTH_CENSUS_ONLY`
invocation per model, `PLINTH_CENSUS=1 PLINTH_THREADS=4` — **not copied from J2's or J3's
commit messages, and independently reproducing both of them byte for byte** (this is the third
concordant measurement of the septet):

| Model | live digest (2026-08-09) | previous (2026-08-08) | routed / live |
|---|---|---|---|
| `Sphere` | `2297157889062365577` | `17667468971446251870` | 7 / 15 (unchanged) |
| `Cylinder` | `6853793710942395608` | `13310185350867878190` | 54 / 119 (unchanged) |
| `make_box` | `8709076999423814412` | `9657051231007502658` | 11 / 25 (unchanged) |
| `make_overhang_table` | `16396858890798947253` | `1875674282253663076` | 73 / 261 (unchanged) |
| `make_toed_slab` | `133000971864625681` | `2823659516373762066` | 64 / 170 (unchanged) |
| `make_raking_blade` | `18388631460456455184` | `9567737006650358297` | 27 / 72 (unchanged) |
| `make_thin_fin` | `14656364680512457190` | `8797083662933568276` | 17 / 41 (unchanged) |

**All seven digests move and NOT ONE count moves.** That is the discriminator: the 2026-08-08
re-pin moved counts on two models (`make_toed_slab` 179 → 170 live, `make_raking_blade` 32 → 27
routed) because real island births were removed; this one moves none, on any model, because no
geometry, no routing decision and no seat was touched. A future re-pin that claims this
movement and also moves a count is a different movement and must stop.

The Weeping Treant remains outside the digest set (it has no frozen value; J2's first seal is a
measurement, not a pin — see [`#fu32-j3-2026-08-09`](#fu32-j3-2026-08-09) §J3.9), and no eighth
row is added here.

### J4.2 — Every site re-pinned, and how

The set was closed by measurement before the ruling was sought (§J3.7): the workspace was run
with `--no-fail-fast` and **exactly two tests failed**. Both now pass, and nothing else changed.

| # | site | from | to | how |
|---|---|---|---|---|
| 1 | `crates/plinth-support/tests/support_runner_integration.rs:137` (`make_toed_slab`) | `13_346_779_153_602_682_749` | **`2_198_459_883_570_957_390`** | re-measured at this tip from the failing test's own printout, not copied |
| 2 | `crates/plinth-support/tests/support_runner_integration.rs:144` (`make_raking_blade`) | `8_660_653_339_934_460_766` | **`74_030_269_957_898_085`** | same |
| 3 | `crates/plinth-verify/tests/coverage_gate_overhang_table.json` → `"snapshot_digest"` | `5470664714994646368` | **`3706765241747593242`** | **regenerated through the golden's own producer path** — never hand-edited; see below |
| 4 | `crates/plinth-verify/tests/coverage_gate.rs` provenance comment | (prose) | new dated paragraph | appended below the existing two refresh records, which stand unamended |
| 5 | `crates/plinth-support/tests/support_runner_integration.rs` doc comment | (prose) | new dated paragraph | appended below the 2026-08-05 and 2026-08-08 records, which stand unamended |

**The golden was regenerated, not edited.** `coverage_gate.rs` carries its own producer: the
`PLINTH_VERIFY_COVERAGE_THREAD_CHILD` arm of
`public_coverage_report_is_frozen_without_evidence_loss` prints the live report as a
`PLINTH_VERIFY_COVERAGE_JSON=` line, which is the same string the frozen constant is compared
against. That line was captured verbatim and written to the golden. The regenerated document
was then diffed field by field against the old one: **exactly one field differs,
`snapshot_digest`** — 185 scalar leaves, 1 moved, and the file is the same 5,271 bytes because
both digests are 19 digits. Every `request` bound, all 11 `resources` counters, all 9 islands
with their face ids, all 9 patches and all 9 defects are byte-identical. **No evidence was
lost**, which is the clause the test's own name asserts.

**The digest that stayed pinned is the load-bearing control.**
`verification_snapshot_digest_input_list_is_pinned` (`runner.rs:4976–5014`) was **not** touched
and is **green**. It fires when the snapshot digest's *input list* widens; its staying green
across a movement of all seven observational digests is the independent confirmation that J2
changed **the value of one of 17 inputs, not the shape of the digest**. Had this re-pin needed
to touch it, the change being re-pinned would not have been this one.

### J4.3 — The record sweep: dated supersession, no earlier line rewritten

All seven superseded values were grepped repo-wide. Every survivor is now unambiguously
historical, either because it sits under a date in its own section or because a dated
supersession note was appended beside it. **Not one earlier citation was edited or deleted.**

| file | occurrences of the old septet | what J4 did |
|---|---:|---|
| `docs/rust-migration/CURRENT-STATUS.md` | 64 | this section, carrying the new authority table; the FU-32 ledger row gains its J4 bracket; **no existing line edited** |
| `docs/rust-migration/support-resource-caps-2026-08-05.md` | 8 | dated supersession addendum at §4's canonical table and at the OD-8 restatement |
| `docs/rust-migration/slicer-union-design-2026-08-07.md` | 15 | dated supersession addendum noting the FU-10 phase-C from→to tables have been superseded by a second, unrelated movement |
| `docs/rust-migration/decimation-design-2026-08-06.md` | 10 | dated supersession addendum |
| `docs/rust-migration/census-embed-incidence-2026-08-04.md` | 5 | dated supersession addendum |
| `docs/rust-migration/export-path-design-2026-08-07.md` | 1 | dated note where Sphere's digest is called "bit-for-bit the frozen OD-8" |
| `docs/rust-migration/contact-physics-2026-08-04.md` | 1 | dated supersession addendum at the table |
| `crates/plinth-support/tests/penetration_census_equivalence.rs` | 2 | dated note on each doc comment; both cite a digest as a **model identifier only** and assert nothing on it |
| `docs/rust-migration/fu32-design-2026-08-09.md` | — | **NOT amended.** Landed verbatim under a provenance banner; it is a record of 2026-08-09 at `bfa5b3b`. |
| `docs/rust-migration/attribution-slicer-census-2026-08-07.md` | 0 of the septet; cites the two superseded seal-test tuples and the old coverage digest | **NOT amended.** Landed verbatim under a provenance banner as the evidence record the 2026-08-08 ruling was made against; it is a record of 2026-08-07 and is superseded by reference from this section, not by edit. |

### J4.4 — Gates and census at this tip

Tree clean before and after. (1) `cargo test --workspace --offline --no-fail-fast` — **1060
passed / 0 failed**; (2) the same `--all-features` — **1065 passed / 0 failed**; (3)
`cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` — clean; (4)
`cargo fmt --all -- --check` — clean; (5) `git diff --check` — clean. **All five green.** The
counts are the pre-movement 1052 / 1057 plus J1's +3 and J2's +5, exactly as predicted; **J4
adds zero tests and moves zero counts**, which is what a re-pin phase must do. The FU-27
environmental flake did not fire, so no re-run was needed.

**Census.** The new septet reproduces — the table in §J4.1 *is* that measurement, taken at this
tip after the re-pin, and it agrees byte for byte with J2's original sweep and J3's independent
instrumented sweep. `routed_contacts` and `final_live_members` are unchanged on every model.

**NO cap, floor, constant or registry entry was raised, lowered or touched.** No count moved.
No test was relaxed, deleted or ignored. **Still open: J5** (the honest routing charge, design
§D3 — an *addition*, not a move) **and J6** (the treant measurement and the record). **[J5 LANDED; ROUTING IS CHARGED FOR THE FIRST TIME — 2026-08-09 (**[`#fu32-j5-2026-08-09`](#fu32-j5-2026-08-09)**).** F8's debt is paid: the sealed CCO routing cadence, which had **no `Limit`, no counter and no clock** before this tip, now carries a counted preflight charge for the real Θ(N·P) work it does — `cadence × pool_rows` seat evaluations and the same bound on retained `AnchorSeatRefusal` evidence rows — against **two NEW registered caps**, `SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS` and `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS`, both `536,870,912` by one stated rule, `next_power_of_two(2 × 553 × 364,223)`, single specimen n = 1 per R4/FU-23. Both worst cases are **exact and provable**, not estimates. A meter fed by the discovery walk's own counts sits **beside** the charge (the G6c idiom) and the receipt is held before the transcript is sealed: `executed ≤ charged` or `SupportRoutingError::ChargeDrift`. **Measured, every model** (new census table `T0_route_cco_charge`): treant charged **201,415,319** and executed **164,993,019** evaluations / **162,719,910** retained rows — 37.5 % and 30.7 % of the cap — against a largest-corpus charge of 33,320 (Sphere, 0.0062 %). The design's M2 refusal-row **projection** (≈70.1 M) is corrected to the measured 162,719,910; the **cap does not move**, because it was derived from the worst case, which reproduced exactly. The digests J1/J2 made cheap are deliberately **not** charged (design D3), and the charge landed **after** the fix, never before it. Registry 284 → **286**; FU-24 ledger rows, caps-doc §21, physics-doc §3.10.11 and a producer-regenerated `constants.md` all in the same commit. **The ≈16.3 GB of retained refusal evidence the second cap admits is DISCLOSED, NOT RATIFIED** at every record site. Gates **1066 default / 1071 all-features, ALL GREEN** (+6, each attributed); OD-8 seven **byte-identical** with `routed_contacts`/`final_live_members` unmoved — a result-inert charge, which is what it must be. The treant seal **reproduces byte-identically** (`snapshot_digest=423276951048565625`) though **without the watchdog or timebox armed**, so it carries no wall-clock or peak-RSS number of record; **J6 still owes that measurement.** **NO EXISTING CAP, floor, constant or registry entry RAISED, LOWERED OR TOUCHED** — the two caps are additions to a stage that had none. `rewire::optimize`'s absent budget is **deferred, not skipped**, and opened as **FU-33**. **Still open: J6** (the treant measurement and the record).]** **[J6 MEASURED; THE TREANT SEALS TWICE UNDER THE PROTOCOL — 2026-08-09 (**[`#fu32-j6-2026-08-09`](#fu32-j6-2026-08-09)**). FU-32 IS COMPLETE, J0–J6.** The measurement J5 left owing is paid. Two attempts, standing memory protocol without deviation, both **COMPLETED**: **470.83 s and 470.64 s of the 600 s box**, exit code 0, peak RSS **5,585.6 / 5,610.9 MiB — 6.06 % / 6.09 % of the 90 GB arm, which never fired**. Seal tuple identical in both and identical to J2's: `routed_contacts=306`, `final_live_members=1383`, `snapshot_digest=423276951048565625`. **Determinism to the G6d standard: 13,441 terminal census rows per run, SHA-256 identical (`81b45113…`), zero differing rows**; wall-clock agrees to 0.04 %. **The cadence loop that consumed ≥335 s WITHOUT FINISHING at `bfa5b3b` completes in ≤20.7 s**, bracketed by J5's own preflight-charge and receipt rows — the number FU-32 was opened to produce. Charge/receipt reproduce J5 to the unit (164,993,019 and 162,719,910 executed, 30.7 % / 30.3 % of the untouched cap). **Prediction §A2 scored: the ~55 % call (rewire binds) MISSED — rewire cost 41.5–46.5 s, 8.8–9.9 % of the run, and the census bounds its evaluated candidates at 619 clearance rows and 3,076 anchor attempts; the shape was right, the quantity was wrong, and FU-33 is NOT closed by this.** The ~10 % call (a clean seal) **LANDED, on cause as well as outcome**. **The brace and raft stages RAN for the first time and none of the ten registered brace/raft caps refused — but none of them is metered, so there is still NO demand of record for any of the ten**, and H3's "no run has ever reached them" is superseded while its "no measured demand" stands. **Design §M3's 30.3 B/refusal is corrected DOWNWARD to ≤8.8–9.3 B/row at completion, and the cap does NOT move in either direction.** **NO cap, floor, constant, coefficient or registry entry raised, lowered or touched; NO engine code changed** (`git diff` over `crates/`/`rust/` empty); the treant is **still not in the frozen digest set** and `423276951048565625` is **still not a pin**. What stands between this seal and the `.chitubox` checkpoint is the **emit path** — `first_artifact.rs --all` still excludes the treant on a reason that is no longer true — and that is export work, not FU-32's.]**

## FU-32 phase J5 landed 2026-08-09 — routing is charged for the first time; two NEW caps, nothing existing touched {#fu32-j5-2026-08-09}

**What this phase is, in one sentence.** J1 and J2 removed ≈843 GB of per-terminal re-hashing
from the sealed CCO routing cadence; J5 pays F8's debt by putting an honest, counted charge on
what is left, so the stage can no longer hang unpriced.

### J5.1 The debt

Design finding **F8** (`fu32-design-2026-08-09.md`): `route_initial_cco` had **no `Limit`, no
`required`/`limit` pair, no counter and no clock** anywhere in `cco.rs` or routing's half of
`runner.rs`; its only preflight was digest and binding validation; and neither
`SupportRoutingError` nor `CcoError` had a limit-shaped variant. That is the pre-FU-8b
naked-hang shape exactly. **A 600 s wall inside an uncharged loop is a missing charge, not a
slow success** — which is precisely how H6's EXCEEDS-TIMEBOX presented, and why no cap could
have caught it.

### J5.2 The charge, the meter and the receipt

Two resources, charged from the instance shape **before the first terminal runs**:

| resource | worst case | new registered cap |
|---|---|---|
| seat evaluations | `cadence × pool_rows` | `SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS` = 536,870,912 |
| retained refusal rows | `cadence × pool_rows` | `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS` = 536,870,912 |

Both worst cases are **exact and provable**, not estimates: every grown terminal's
`discover_finalized_anchor_seats` classifies every exterior anchor-pool row exactly once, and a
sample can enter one terminal's refusal list at most once. Both are knowable before any work is
done, which is what makes them chargeable at a preflight.

**Meter beside the charge, never instead of it** (the G6c idiom): `CcoController` carries a
`RouteCcoWorkMeter` fed by the discovery walk's **own counts** — `AnchorSeatQueryCounts` grew a
`seat_evaluations` field so the count comes from the walk rather than being re-derived from
`pool.rows().len()` at the caller — and `route_initial_cco` holds the receipt before the
transcript is sealed: `executed ≤ charged` on both resources or
`SupportRoutingError::ChargeDrift { resource, charged, executed }`. The meter is **not hashed**
and is a member of no digest.

**Deliberately NOT charged:** the controller digest and the immutable-authority digests. After
J1/J2 they are O(1) and O(delta), and pricing them would price an implementation detail (design
D3 — the same call G3 made in declining to charge the merge gate's executed prefix). **The
sequencing was not negotiable:** charging first would have written the defect into a budget.

### J5.3 Derivation, one rule for both

`next_power_of_two(2 × the Weeping Treant v1.2's charged demand)` =
`next_power_of_two(2 × 553 × 364,223)` = `next_power_of_two(402,830,638)` = **536,870,912**.
Single specimen, n = 1, per R4/FU-23. Registered as `Placeholder`, classification
`runtime_resource_cap`, with the FU-24 ledger rows and the caps-doc and physics-doc records
written in the same commit (`m0-constants-source-ledger.json`,
`support-resource-caps-2026-08-05.md` §21, `03-physics-and-constants.md` §3.10.11); the
`constants.md` table was regenerated **through its own producer**, never hand-edited. Registry
size 284 → **286**.

### J5.4 Every model's stated demand, charge beside meter

New census table `T0_route_cco_charge` (`PLINTH_CENSUS=1`, `PLINTH_THREADS=4`, this tip,
2026-08-09). "Before" is not a smaller number: **before this phase there was no charge at all.**

| model | cadence | pool_rows | charged | executed evaluations | executed refusal rows |
|---|---:|---:|---:|---:|---:|
| Sphere | 7 | 4,760 | 33,320 | 33,320 | 33,320 |
| Cylinder | 54 | 244 | 13,176 | 13,176 | 13,095 |
| make_box | 11 | 12 | 132 | 132 | 125 |
| make_overhang_table | 84 | 52 | 4,368 | 4,160 | 3,578 |
| make_toed_slab | 68 | 76 | 5,168 | 5,016 | 5,016 |
| make_raking_blade | 33 | 23 | 759 | 667 | 667 |
| make_thin_fin | 18 | 22 | 396 | 396 | 396 |
| **Weeping Treant v1.2** | **553** | **364,223** | **201,415,319** | **164,993,019** | **162,719,910** |

The treant's charge is **37.5 %** of the cap, its executed spend **30.7 %**; the largest corpus
charge is **0.0062 %**. `executed ≤ charged` holds on every model and both resources. The
treant's 81.9 % execution-to-charge ratio is its quarantined cadence entries, which a preflight
cannot know about in advance.

**One projection is corrected, and the cap is not.** The design's M2 projected ≈70.1 M retained
refusal rows over the treant's full cadence from a growth law measured over a partial run; the
completed run retains **162,719,910**, 2.3× that. The projection is corrected here as a dated
record; the cap does not move, because it was derived from the worst case (201,415,319, which
reproduced exactly), not from the projection.

### J5.5 The disclosed hazard

At the 30.3 B per retained refusal measured in the design's M3,
`SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS` admits ≈**16.3 GB** of retained refusal evidence —
above the ≈4.9 GB the treant's rows imply, and below but **not comfortably below** the 90 GB
watchdog arm and the ~100 GB machine ceiling. **Disclosed, not ratified**, in the registry note
and in both doc records. Design open question 4 (*is evidence proportional to the anchor pool,
or to the decision?*) is left untouched on purpose: summarising the refusal list would move
every transcript digest and mix two digest movements into one ruling.

### J5.6 Tests — six added, every one attributed, and the refusal is proven on real geometry

`route_initial_cco` was split so its caps arrive as a **parameter** (`route_initial_cco_with_limits`),
exactly as every other stage's limits do — which is what lets two of these tests drive the REAL
routing path into the new refusal on the required corpus rather than proving arithmetic in
isolation and trusting the wiring by inspection.

| test | what fails if it is deleted |
|---|---|
| `fu32_j5_routing_charge_prices_the_measured_treant_shape_under_the_registered_caps` | the treant's stated demand and both registry values drift apart from the notes that quote them |
| `fu32_j5_routing_refuses_an_over_demand_seat_search_on_the_required_corpus` | the seat-evaluation arm stops refusing |
| `fu32_j5_routing_refuses_an_over_demand_refusal_evidence_budget_on_the_required_corpus` | the refusal-row arm stops refusing — proven **independently** of the arm above, so neither is carried by the other |
| `fu32_j5_the_routing_meter_counts_the_real_pool_walk_and_stays_under_its_charge` | the meter is left unwired and reads zero on a real routing run |
| `fu32_j5_the_routing_receipt_refuses_execution_that_outruns_its_charge` | either `ChargeDrift` arm stops firing |
| `fu32_j5_an_unrepresentable_routing_product_refuses_instead_of_wrapping` | the product wraps to a small, admissible charge |

### J5.7 Verification

**Gates 1066 default / 1071 all-features, all green**, zero failures (1060/1065 at J4, plus
J5's six); clippy `-D warnings`, `cargo fmt --check` and `git diff --check` clean; tree clean
before and after. **OD-8: the seven frozen digests are byte-identical** at this tip —
Sphere `2297157889062365577`, Cylinder `6853793710942395608`, make_box `8709076999423814412`,
make_overhang_table `16396858890798947253`, make_toed_slab `133000971864625681`,
make_raking_blade `18388631460456455184`, make_thin_fin `14656364680512457190` — with
`routed_contacts` and `final_live_members` unchanged on every model, which is what a
result-inert charge must look like.

**The treant seal reproduces at this tip**, byte-identically to the J2 measurement the FU-32 row
records: `routed_contacts=306`, `final_live_members=1383`,
`snapshot_digest=423276951048565625`. It is recorded here as a **reproduction**, not a new
birth, and with one protocol caveat stated plainly: this run was launched unsandboxed but
**without the 90 GB RSS watchdog and without the 600 s timebox**, because it was a corpus census
that happened to include the treant rather than a J6 measurement. **It therefore carries no
wall-clock or peak-RSS number of record.** J6 still owes the measured run under the standing
memory protocol.

**NO existing cap, floor, constant or registry entry was raised, lowered or touched.** The two
caps here are **additions** to a stage that had none.

### J5.8 What J5 does not bound — `rewire::optimize`, opened as FU-33

`REWIRE_WORK_BUDGET` is `TrialSetting::disabled()`, so `rewire::optimize` is an **unbounded
loop** with no work budget and no wall-clock deadline. The design names it as prediction A2 #1,
the most likely next binder (~55 %). It is **deferred, not skipped**, and the reason is a rule
rather than a preference: bounding a search is **result-changing** — a budget that stops early
changes which forest is returned and therefore every downstream digest — where J5's charge is
result-inert by construction. Landing both in one phase would put a value-moving ruling inside
a resource-charging phase, which is the exact mixing FU-10 protocol exists to prevent. See
**FU-33**.

## FU-32 phase J6 measured 2026-08-09 — THE WEEPING TREANT SEALS, TWICE, UNDER THE STANDING MEMORY PROTOCOL {#fu32-j6-2026-08-09}

> **This section is the birth protocol** (FU-8g §A4, invoked unchanged): the treant sealed, so
> the record is written as one, in one place, before anything else is touched. **Nothing was
> tuned, no coefficient re-derived, no follow-up opened, no cap moved.** J6 changes **no engine
> code at all**: `git diff` over `crates/` and `rust/` is empty at this tip, and the only delta
> in this commit is documentation.

### J6.0 What happened, in one sentence

At `e173597` — J0's flush, J1's hoist, J2's incremental digest, J3's attribution, J4's re-pin and
J5's charge, in that order — **`prepare_support_run` returned `Ok` on the Weeping Treant v1.2 at
429,120 faces, twice, in 470.83 s and 470.64 s of a 600 s timebox, exit code 0 both times, with
the 90 GB RSS watchdog armed and never fired**, and the two runs are **bit-identical on every
terminal census row**.

### J6.1 THE SEAL TUPLE — the `MODEL` line, verbatim, identical in both runs

```
MODEL	Weeping Treant v1.2	OK	faces=429120	max_grid_bytes=219709440	knn_max_work=878837760	orientation=own_pinned	pitch_mm=0.05	layers=1951	routed_contacts=306	final_live_members=1383	snapshot_digest=423276951048565625
```

| field | value |
|---|---:|
| `routed_contacts` | **306** |
| `final_live_members` | **1,383** |
| `snapshot_digest` | **423276951048565625** |
| faces | 429,120 |
| `max_grid_bytes` (model-scaled) | 219,709,440 |
| `knn_max_work` (model-scaled) | 878,837,760 |
| layer pitch / count | 0.05 mm / 1,951 |
| orientation | `own_pinned` |

**A deviation from §A4 step 2, stated rather than papered over.** The birth protocol asks for all
nine `PreparedSupportDigests` fields (`mesh`, `generic`, `regular`, `plan`, `loads`, `geometry`,
`initial_forest`, `contact_attestation`, `prepared`). **They are not recorded here, because they
are not reachable:** `PreparedSupportDigests` is `pub(crate)` in `runner.rs`, the census harness
is an external example, and printing them would require engine code — which this phase is
forbidden to add and which would make the measurement no longer a measurement of this tip. What
*is* recorded is the public seal the snapshot exposes, `snapshot_digest`, which is the digest
every downstream consumer and every frozen golden in this repository actually binds. Exposing the
nine is a follow-up for whoever wants them, not a silent omission.

### J6.2 Provenance, and who owns the word "first"

**J2's run is the first treant seal in project history, and this record does not take that from
it.** J2 sealed the treant at `c14754e` (`snapshot_digest=423276951048565625`, 463 s, RC = 0), and
J5 reproduced it byte-identically at `e173597` — but **both were corpus census runs that happened
to include the treant, launched without the 90 GB watchdog and without the 600 s timebox**, so by
this project's own standard they carry no wall-clock or peak-RSS number of record. J5 said so in
its own commit and left the measurement owing.

So, precisely:

| run | tip | what it is |
|---|---|---|
| J2 | `c14754e` | **THE FIRST TREANT SEAL.** 463 s, RC = 0, no watchdog, no timebox — a first measurement, not a pin |
| J5 | `e173597` | the same seal reproduced byte-identically; still no protocol, still no number of record |
| **J6 (this)** | `e173597` | **the first seal measured under the standing memory protocol, and the first seal of a CHARGED routing stage** — J5's caps are live and the receipt is held |

The digest is the same value in all three. **No golden for this model has ever existed and none is
created here**: the Weeping Treant remains outside the frozen OD-8 digest set, exactly as §J3.9
and §J4.1 left it. This is a measurement, twice reproduced. It is not a pin.

### J6.3 The invocation, and the machine

Both attempts, verbatim and without deviation:

```
cargo build --release --offline --features census --example embed_incidence_census -p plinth-support
PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2" PLINTH_THREADS=4 \
  target/release/examples/embed_incidence_census
```

launched **unsandboxed** (a sandboxed launch EPERM-blocks at 0 s), with an **unsandboxed RSS
watchdog polling `ps` every 2 s, hard-armed at 90 GB**, sending `SIGTERM` first — so J0's
signal-triggered flush would fire — then `SIGKILL`, and a **600 s timebox** on the same
discipline. **Two attempts permitted; two used; neither needed a retry.**

**Machine-quiet evidence, recorded at both launches** (`ps -Ao pcpu,rss,comm -r`, unsandboxed,
polled three times before the first launch and once immediately before the second): no `cargo`,
no `rustc`, no sibling `embed_incidence_census`, and no process above ~30 % CPU — the top rows
were the terminal emulator, `WindowServer` and the wallpaper extensions in every sample. The
`blender-mcp` names that match a naive `pgrep` are idle MCP stubs, absent from the CPU top in all
four samples. 128 GB machine; the ~100 GB working ceiling was never approached (see §J6.6).

### J6.4 THE TIMELINE — per stage, from the committed census counters

Every boundary below is **counter-proven from `T0_progress`/`T0_route_cco_charge` rows that J0
made survive**, bracketed by the two consecutive flushes that straddle it; the flush cadence is
**5.18 s** (`DEFAULT_FLUSH_INTERVAL_SECS = 5` plus the 250 ms poll), so every boundary carries
**±5.2 s** and is written as an interval, not a point. 90 flush rows in each run. Stage names come
from the design's AFTERMATH stage table and from the census call sites, not from guesswork:
`contact_rows`/`attestation_rows` are `initial.rs`, `anchor_rows` are `cco.rs`,
`rung_rows`/`rung_outcome_rows`/`embed_rows` are `brace.rs`, `gate_batches`/`merge_*` are
`specialized.rs`.

| # | stage | boundary evidence | run 1 | run 2 |
|---|---|---|---:|---:|
| 1 | decimate → orient → slice → support preparation | first non-zero `gate_batches` | **≈134.7–139.9 s** | **≈129.4–134.6 s** |
| 2 | specialized **merge-gate tournament** | `merge_tournaments` 0 → 1 | ≈41.4 s | ≈41.5 s |
| 3 | post-tournament coalesce + `initial::build_initial_contacts` | `contact_rows` 0 → **553**, `attestation_rows` 0 → **553** | ≈31.1 s | ≈31.1 s |
| 4 | routing **setup**: `SurfaceAnchorPool::from_geometry` (364,223 rows) + J1's `VerifiedRoutingAuthority::compute` | last flush with **no** `T0_route_cco_charge` row → first flush with one | ≈57 s | ≈62 s |
| 5 | **`route_initial_cco` — THE CADENCE LOOP** | charge row present (preflight) → meter non-zero (receipt) | **≥10.3 s, ≤20.7 s** | **≥10.4 s, ≤20.8 s** |
| 6 | `rewire::optimize` | last `anchor_rows` movement (`discover_rewire_anchor_seats`) | ≈46.5 s | ≈41.5 s |
| 7 | brace + transit | `rung_rows` 0 → **748** | ≈36.2 s | ≈36.3 s |
| 8 | structure (joints + raft) + lowering + seal + snapshot digest + census `finish` | to the `MODEL` line | ≈103.2 s | ≈107.0 s |
| | **whole run** | process exit | **470.83 s** | **470.64 s** |

**Read row 5 twice.** The charge row is emitted immediately after `charge.preflight(limits)?` and
**before the first terminal** (`runner.rs:1683–1689`); the meter row is emitted immediately after
the last terminal and **before `finalize_transcript`** (`runner.rs:1728–1734`). The two together
bracket the cadence loop — the loop this whole follow-up exists about — at **no more than 20.7 s**,
and the bracket is the same in both runs to within one flush. Against the record it replaces:

| the cadence loop | wall |
|---|---:|
| at `bfa5b3b` (design §M2, scratch-instrumented) | **≥335 s and NOT FINISHED** — killed at 396 of 553 entries, 71.6 % |
| design §M4's projection for the hashing alone, over the full cadence | **≈691 s** |
| **at `e173597`, complete, 553 of 553 entries** | **≤20.7 s** |

That is the number FU-32 was opened to produce. **The design projected a ≈296× fall in bytes
hashed; the wall-clock evidence is consistent with it and does not, by itself, measure it** — this
run carries no per-site byte counters, because carrying them would have meant engine code.

### J6.5 EVERY CENSUS COUNTER, at the seal

All values identical in both runs unless stated. `cadence = 553`, `pool_rows = 364,223`,
`faces = 429,120`.

**The routing charge and its receipt (`T0_route_cco_charge`) — J5's caps, first exercised under
the protocol:**

| | charged | executed | % of charge | % of the 536,870,912 cap |
|---|---:|---:|---:|---:|
| seat evaluations | 201,415,319 | **164,993,019** | 81.92 % | **30.73 %** |
| retained refusal rows | 201,415,319 | **162,719,910** | 80.79 % | **30.31 %** |

`executed ≤ charged` held on both resources; `SupportRoutingError::ChargeDrift` did not fire.
These reproduce J5's uncharged-protocol figures **to the unit**.

**The merge gate (`T0_scaled_cap`, `kind=specialized_gate_operations`):** floor 2,000,000,000,
`per_batch_unit` 64, rows 342,668, columns 891, materialized **19,562,287,872**, `at_floor=false`.
`gate_batches` **340**, `merge_proposals` **1,294,392**, `merge_tournaments` **1**,
`merge_rounds_committed` **337**, `merge_contacts_first` **891** → `merge_contacts_last` **554**,
`merge_exit_reason` **`no_safe_proposal`**. Retained load transpose: `retained_load_rounds` **340**,
`retained_load_rebuilds` **1**, and the one rebuild is
`retained_load_rebuild_contact_witness=1` — **99.7 % retained, with ZERO `Poisoned` rebuilds**.
Charged **66,731,232,175** against executed **6,634,686,434** — **9.94 %**.

**Load (`T0_scaled_cap`, `kind=specialized_load_operations`):** rows 83,146, columns 891,
materialized **4,746,695,936**, `at_floor=false`.

**The forest that sealed:**

| table | value |
|---|---|
| `T5_final_members` | Brace/none **450**, `model_rung`/rootward **8**, Shoot/none **363**, Strut/rootward **4**, Stub/terminalward **306**, Trunk/none **252** |
| `T6_commits` | `brace_append_crosslink` **238**, `brace_append_model_rung` **9**, `cco_winner_bifurcate` **199**, `cco_winner_model_anchor` **4**, `cco_winner_plate` **103** |
| `T2_cco_anchor` | attempted **16,504**, surviving **4** |
| `T4_tip_radii` | 553 tips; min 0.200000, median 0.277087, max 1.000000, mean 0.318805 mm |
| `T4_curvature` | flat 19, convex 213, concave 50, saddle 271 |
| `T1_rung_cos` | 748 rungs; grazing < 0.5 **45**, all with L > 2.50 mm; legal band (1.0, 5.0) mm |
| `T1a_rung_outcome` | 650 rows, admitted viable **577**, **over-masked 0**, infinite clearance 0 |
| `T3_over_mask` | `cco_candidate_clearance` 91,046 rows / 36 over-masked; `brace_exact_member_clearance` 56,183 / 248; `brace_candidate_clearance` 5,521 / 0; `rewire_clearance` **619** / 0 |
| `T7_attestation` (stubs) | 553: fully engaged **247**, centreline outside 162, min-rim engaged 0, refused 79, area deficit 186 |
| `T8_seat_attestation` | `brace_model_rung` 748 (engaged 407, refused 98); `cco_model_anchor` 11,609 (centreline outside 11,609, refused 411, area deficit 3,986) |

**Four identities the transcript proves against itself** — this is what D7's "self-proving"
requirement buys, and all four hold:

1. `cco_winner_bifurcate` 199 + `cco_winner_model_anchor` 4 + `cco_winner_plate` 103 = **306** = `routed_contacts`.
2. `T2_cco_anchor` surviving **4** = `T6` `cco_winner_model_anchor` **4**.
3. `T5_final_members` sums to 450 + 8 + 363 + 4 + 306 + 252 = **1,383** = `final_live_members`.
4. `T5` Stub/terminalward **306** = `routed_contacts`.

### J6.6 MEMORY — per stage, and the arm that was never approached

Whole-process peak RSS, unsandboxed `ps` polled every 2 s:

| | run 1 | run 2 |
|---|---:|---:|
| peak RSS | **5,585.6 MiB (5.455 GiB)** | **5,610.9 MiB (5.479 GiB)** |
| at | t = 401.5 s | t = 399.3 s |
| **% of the 90 GB watchdog arm** | **6.06 %** | **6.09 %** |
| % of the ~100 GB machine working ceiling | 5.5 % | 5.5 % |

**The watchdog never fired and was never close.** The 20 s RSS envelope agrees between the two
runs to within **1.3 % in 22 of 24 windows** (worst case 8.5 %, inside the merge gate, where
allocator timing moves). The shape:

| window | peak RSS | what is running |
|---|---:|---|
| 0–140 s | ≈1.6 → 2.7 GiB | preparation |
| 140–260 s | ≈2.3 → 2.4 GiB, flat | gate tournament, initial contacts, routing setup |
| 260–285 s | 2.37 → **3.73 / 3.79 GiB** | **the cadence loop** — the retained refusal evidence |
| 285–300 s | → **5.32 / 5.35 GiB** | transcript finalization and rewire entry (one 2 s poll apart; **not attributed further, because two things happen in that window**) |
| 300–470 s | ≈5.52 → 5.61 GiB, flat | rewire, brace, structure, lowering, seal |

**Two projections are corrected here, both downward on the hazard side, and NEITHER moves a cap.**

- **Design §M3's ≈4.3 GiB whole-process projection was for a completed *routing stage*, and for
  routing it holds**: routing exits at 3.73 / 3.79 GiB. The whole-process peak is **5.46–5.48 GiB**
  because three stages M3 had never seen run — rewire, brace, structure/lower — run after it. The
  projection was not wrong; it was answering a smaller question, and this is the first run that
  could ask the bigger one.
- **Design §M3's 30.3 B per retained refusal is an over-estimate at completion.** The routing
  stage's whole RSS growth is 1,359 MiB (run 1) and 1,436 MiB (run 2) across **162,719,910**
  retained rows — **≤8.8 and ≤9.3 bytes per row**, and those are *upper* bounds, since every other
  allocation in the stage shares the window. §21.5's disclosed ≈16.3 GB implied by
  `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS` at 30.3 B/row is therefore closer to **≈4.4–4.7 GB** at
  the measured rate. **The cap does not move, in either direction.** It was derived from the worst
  case, which reproduced exactly (201,415,319); lowering it on a friendlier observation is the same
  error as raising it on an unfriendly one, wearing a nicer suit. The disclosure in §21.5 stands,
  with this correction appended to it.

### J6.7 DETERMINISM — the G6d standard, met

The G6d standard is that identical counters across two attempts is what makes a measurement a
measurement rather than an anecdote. Applied here:

- The two `MODEL` lines are **character-identical**, including `routed_contacts`,
  `final_live_members` and `snapshot_digest`.
- Every **terminal** census row — every `T1`–`T8` table, every per-row detail line, the final
  `T0_scaled_cap` and the `T0_route_cco_charge` receipt: **13,441 rows in each run, sorted, SHA-256
  `81b45113ff17fa876d5f57f9f2accdc4e8d7d9b55da1d783507039189a506702` in BOTH.** Zero differing rows.
- Wall-clock agrees to **0.04 %** (470.83 vs 470.64 s); peak RSS to **0.45 %**.
- **What does differ, and why it is not a determinism failure:** the *mid-flight* periodic-flush
  snapshots of `T0_scaled_cap`, which capture whatever the tournament counters happened to hold at
  a 5.18 s tick (run 1 caught batch 127/164/205/249/297; run 2 caught 131/169/210/255/304). Those
  rows are a stopwatch reading, not a result, and they are excluded from the comparison above by
  construction — J0 introduced them precisely so that a *killed* run would still report. Every
  terminal value they converge to is identical.

**This is the same run twice.**

### J6.8 THE PREDICTION SCORECARD — scored on outcome AND on cause

The design's §A2 ranked what binds next after FU-32 lands. Scored against two completed runs:

| # | prediction | confidence | outcome |
|---|---|---:|---|
| 1 | `rewire::optimize` binds — same defect shape, worse, and unbounded | **~55 %** | **MISS** |
| 2 | a typed refusal in one of the ten unexercised brace/raft caps | ~25 % | **MISS** — no refusal fired anywhere |
| 3 | wall-clock again, inside routing's remaining honest work | ~10 % | **MISS** — the cadence loop is ≤20.7 s |
| 4 | **a clean seal — the first treant seal** | **~10 %** | **LANDED — and landed on the cause too** |

**#1, the 55 % call, missed — and the cause is determinable from the counters.** The design's
per-move analysis was not refuted: `rewire::optimize` really does do a whole-forest `clone()`, two
full `Forest::digest()` walks, `derive_brace_attachments`, a `total_volume_ml` fold, a whole-forest
`validate()` per candidate and a `discover_rewire_anchor_seats` pass per move, with
`REWIRE_WORK_BUDGET` still `TrialSetting::disabled()`. **What the design had no estimate of was the
number of moves the sweep actually proposes on this instance**, and the census bounds it hard:
rewire's own clearance site, `T3_over_mask site=rewire_clearance`, records **619 rows** in the whole
stage, and rewire's share of `T2_cco_anchor_attempted` is **3,076** (16,504 total less the 13,428
standing at the end of routing) against a 364,223-row pool. A stage that expensive per move, run
that few times, costs **41.5–46.5 s — 8.8–9.9 % of the run**. **The shape was right and the
quantity was wrong**, which is the honest way to score it: `rewire` is still an unbounded loop and
**FU-33 is not closed, downgraded or answered by this run** — one specimen that happens to propose
few moves is not a budget. What is now known is that it does not bind *on this specimen*.

A second, quieter thing to note about #1: rewire is nevertheless the **memory**-dominant transition
of the run (the 3.7 → 5.3 GiB step lands at its entry, shared with transcript finalization). The
design's instinct that rewire is where the remaining cost lives was not baseless — it named the
wrong resource.

**#4, the 10 % call, landed on cause as well as outcome.** The design's stated reason was "the
memory ceiling is 20× clear and the only known Θ(N²) terms in routing are the ones this design
removes". Measured: the ceiling is **16.4× clear** (6.06 % of the arm) and routing fell from
"≥335 s and unfinished" to "≤20.7 s and complete". Both halves of the reason held.

**The running scorecard, updated honestly.** §A1 recorded "two high, one low, two landed of five"
before this design. §A2 adds **one landed of four — and it is the lowest-confidence one, while the
highest-confidence one missed.** That is the second time in this chain that the confident
prediction lost to the long shot (FU-8f G6d falsified *both* standing round predictions, in
opposite directions). The pattern is worth naming rather than averaging away: **this project
predicts the *shape* of the next cost well and the *quantity* badly**, and the quantity is what
decides whether something binds.

### J6.9 EVERY STANDING HAZARD, scored

- **The 90 GB watchdog arm — NEVER APPROACHED.** Peak 6.06 % / 6.09 %. The machine's ~100 GB
  working ceiling was never a factor. The `machine-memory-ceiling` finding is untouched and
  unchallenged.
- **The ten unexercised brace/raft constants — THE STAGES RAN, AND THERE IS STILL NO METERED
  DEMAND. Both halves matter.** The seal proves the brace and raft stages executed on the treant
  for the first time: `run_support` cannot return `Ok` without `BracedSupportRun::brace`,
  `structure` (joints + raft) and `lower` all returning `Ok`, and the census shows brace working at
  scale — 748 rung rows, 650 rung outcomes, 603 embed rows, 56,183 + 5,521 brace clearance rows,
  238 + 9 brace commits, and **450 Brace members in the sealed forest**. **No typed refusal fired at
  any of the ten**, which a seal necessarily implies. **But no demand figure of record exists for
  any of the ten**, because nothing in the committed census meters them: `record_scaled_cap` is
  called only from `perception.rs` and `specialized.rs`, never from `brace.rs`, `joints.rs` or
  `raft.rs`. So H3's registry note — "a plain statement of absence for all ten brace/raft
  constants" — **remains correct as written about *demand*, and is now wrong as written about
  *reach***: the phrase "no run has ever reached them" is superseded by this run. Both corrections
  are recorded at every record site (§J6.11). **Turning "did not refuse" into a number needs a
  metering phase, and this phase does not do it and does not licence anyone to invent one.**
- **`SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES = 100,000,000,000` above the arm — STRUCTURALLY
  UNCHANGED, EMPIRICALLY NOT REALISED.** The cap still sits above both the 90 GB watchdog and the
  ~100 GB machine ceiling, so it still *can* arm the watchdog before it refuses. On the largest
  specimen this project has, it did not: brace ran to completion inside a 5.48 GiB whole-process
  peak. **Disclosed, not cleared** — the demand behind that peak is unmetered, so nothing here
  proves the cap could not bite on a bigger mesh.
- **`joints`' hard-coded, M0-unregistered literals** (`max_depth 22`, `max_patches 2,000,000`,
  `max_work_items 50,000,000`, `max_scratch_bytes 512 MiB`, `joints.rs:96–106`) were likewise
  executed without refusing and are likewise unmetered. Recorded, not acted on.
- **FU-28 / cavity-seam findings do not apply**, per the design's own verified exclusion: the
  harness builds every model through `HollowedModel::solid_decision(…, BelowThreshold)`, so there
  is no drilled hollow anywhere in this run.
- **The merge gate is not G6d's instance and no claim is made about G6d's 823 rounds.**
  `merge_contacts_first` is **891** here against G6d's 1,366 — FU-31's slicer-union geometry
  correction landed in between — so 337 committed rounds is not comparable to 823 and is not
  presented as an improvement. What *is* comparable is the exit: `no_safe_proposal`, the same
  acceptance-fold `None` arm G6d first exercised. And the retention improved from 97.3 % to
  **99.7 %**, with G6d's **21 `Poisoned` rebuilds now zero** and only F-G6-4's predicted terminal
  `contact_witness` miss remaining — so **H4's `PoisonCause` rider had no subject in this run.**

### J6.10 What stands between this seal and the `.chitubox` checkpoint — the emit path, and it is NOT this phase's work

Stated precisely so nobody reads a seal as an artifact. **A seal is a `SupportVerificationSnapshot`,
not a file.** This run calls `authority.verification_snapshot()`; the artifact chain calls
`into_supported_model()` and then the emit path. Concretely, what remains:

1. `examples/first_artifact.rs`'s `--all` mode **excludes the Weeping Treant by name**, and the
   stated reason — "its support tournament cannot yet complete" (`export-path-design-2026-08-07.md`,
   the 2026-08-07 corpus-sweep addendum) — **is no longer true as of this run**. Removing that
   exclusion is the first step and it is export work, not support work.
2. The export design's **E7 row** ("The treant plate. NOT export work… the export path should list
   it as a downstream beneficiary, not a dependency", 0 pts) was written when the treant stopped in
   the gate. **The dependency it was waiting on is discharged.**
3. `first_artifact_chain` must then run the treant end to end and produce a written, re-parsed,
   format-gate-passed `.chitubox`, twice, byte-identical — the standard the other five corpus
   artifacts met.

**None of that is J6's, and J6 does not start it.** The `CHECKPOINT: treant first .chitubox`
backlog item is where it belongs, and per §A4 step 5 **a first seal is a decision point for the
owner, not for the implementing agent.**

### J6.11 What this phase does NOT claim

- **No cap, floor, constant, coefficient or registry entry was raised, lowered or touched.** Not
  one. J6 adds no constant and no test.
- **No engine code changed.** `git diff` over `crates/` and `rust/` is empty at this tip; the
  delta is documentation only.
- **The treant is still not in the frozen digest set** and `423276951048565625` is still not a pin.
  Two byte-identical measurements are evidence, not a golden; making it one is a separate,
  deliberate act with its own review.
- **Nothing is claimed about print quality, printability, or whether this forest is a *good*
  support forest.** The seal says the pipeline's own invariants hold. It says nothing about resin.
- **Nothing is claimed about the nine `PreparedSupportDigests` fields** (§J6.1), about brace/raft
  *demand* (§J6.9), or about how the byte counts fell inside routing (§J6.4) — three places where
  the honest answer is "this run could not see it".
- **FU-33 stays open at full strength.** `rewire::optimize` is still an unbounded loop.

### J6.12 Gates and OD-8

**Docs-only delta.** Five gates, verbatim, at this tip, tree clean before and after: (1)
`cargo test --workspace --offline --no-fail-fast` — **1066 passed / 0 failed**; (2) the same
`--all-features` — **1071 passed / 0 failed**; (3) `cargo clippy --workspace --all-targets
--all-features --offline -- -D warnings` — clean; (4) `cargo fmt --all -- --check` — clean;
(5) `git diff --check` — clean. **Identical to J5's counts, as they must be: J6 adds no test and
changes no code.** The FU-27 environmental flake did not fire, so no re-run was needed.

**OD-8 — measured, not asserted.** The seven frozen digests could be argued untouched by
construction, since no engine code changed between J5's measurement of them and this one; they
were re-measured anyway, from the same release `--features census` build that produced the seal,
one `PLINTH_CENSUS_ONLY` invocation each, and **all seven reproduce byte-identical**: Sphere
`2297157889062365577`, Cylinder `6853793710942395608`, make_box `8709076999423814412`,
make_overhang_table `16396858890798947253`, make_toed_slab `133000971864625681`,
make_raking_blade `18388631460456455184`, make_thin_fin `14656364680512457190`. The Weeping
Treant is, as always, deliberately outside this set.

## CHECKPOINT 2026-08-09 — THE WEEPING TREANT'S FIRST `.chitubox`, TWICE, BYTE-IDENTICAL {#checkpoint-treant-artifact-2026-08-09}

> **This is the owner-defined milestone**: *"treant first successful .chitubox output generation"*,
> full end to end. It is satisfied. The artifact below was produced by the real chain — corpus STL
> in, `.chitubox` out — and then judged by re-reading its own bytes, twice.

### C.0 What happened, in one sentence

At `feature/checkpoint-treant-artifact` off `develop @ 063115f`, the Weeping Treant v1.2 ran the
whole `first_artifact_chain` composition to a written, re-parsed, format-gate-passed `.chitubox`
of **28,670,352 bytes**, **twice, in 339.68 s and 336.80 s**, exit code 0 both times, with the
90 GB RSS watchdog armed and never fired — and the two files are **byte-identical**.

### C.1 THE ARTIFACT

| field | value |
|---|---|
| path (durable) | `exports/weeping-treant-v1.2-first-artifact.chitubox` (main checkout; gitignored) |
| **sha256** | **`fb4594b623ad6ef0ea48541c9024dd8cc1dfd2b2bef44975e0ee3f73f2b9c7d1`** |
| size | **28,670,352 bytes** (27.34 MiB) |
| produced at | `feature/checkpoint-treant-artifact`, base `063115f` |
| invocation | `target/release/examples/first_artifact "Weeping Treant v1.2"` |

**Every number below is re-derived from the bytes on disk by `audit_disk_artifact`, not read back
from producer state** — a fresh `Project::open`, a fresh winding computation over the binary32
disk vertices, a raw-header read of the preview span, and the `plinth-verify` format gate, none of
which consult the producer.

| re-derived from disk | value |
|---|---:|
| model record name | `plinth-model` |
| **model triangles** | **429,120** |
| model signed volume | 24,568.377661863808 mm³ |
| **support records** | **2,585** |
| outward-wound records | **2,585 / 2,585** |
| min record signed volume | 0.004450599450726456 mm³ |
| preview span | **26,996 bytes, native fallback = true** |
| **format gate** | **PASSED** (`plinth-verify`, structural status `Passed`) |
| file bytes on disk | 28,670,352 |

**Support records by wire segment type, and the three mesh families they make up:**

| family | segment type | records |
|---|---|---:|
| frustum | `TAPER` (1) | 306 |
| frustum | `JOINER` (3) | 1,077 |
| frustum | `FLARE` (4) | 86 |
| **frustum subtotal** | | **1,469** |
| sphere | `TIP` (7) | 306 |
| sphere | `SHAFT` (9) | 643 |
| **sphere subtotal** | | **949** |
| raft | `PAD_OR_BRIDGE` (6) | 167 |
| **raft subtotal** | | **167** |
| **total** | | **2,585** |

**All three of the emitter's mesh families are exercised**, and 1,469 + 949 + 167 = 2,585 closes
against the record count and against `primitive_count`, both independently.

### C.2 THE SEAL-TUPLE CORRESPONDENCE — the artifact carries exactly the forest J6 measured

This is the claim that makes the artifact worth the name, and it is an equality, not a
resemblance. The checkpoint run's own `SEAL` line against *FU-32 phase J6 measured 2026-08-09*
([`#fu32-j6-2026-08-09`](#fu32-j6-2026-08-09)):

| field | J6's seal | this artifact | |
|---|---:|---:|---|
| `routed_contacts` | 306 | **306** | identical |
| `final_live_members` | 1,383 | **1,383** | identical |
| `snapshot_digest` | 423276951048565625 | **423276951048565625** | identical |
| faces | 429,120 | **429,120** | identical |

`primitive_count` = 2,585, `lift_mm` = 5, `plate_z_mm` = −5. **The 1,383 live members become 2,585
emitted records** because a member lowers to several wire segments; that ratio is reported, not
asserted, and is visible in the family table above.

**Why the equality was not guaranteed in advance, and therefore means something.** J6's seal came
from `embed_incidence_census`, which calls `verification_snapshot()`; this artifact comes from
`into_supported_model()` and the emit path — a different consumer of the same authority, reached
through a different composition, at a different thread count (see C.3). The digest reproducing to
the unit across all of that is evidence the export path and the census see the same forest.

### C.3 THE TWO RUNS

Both unsandboxed (a sandboxed launch EPERM-blocks at 0 s), each with an unsandboxed RSS watchdog
polling `ps` every 2 s, hard-armed at 90 GB, `SIGTERM` then `SIGKILL`, under a **1200 s** timebox.
Machine quiet at both launches: no `cargo`, no `rustc`, nothing above ~38 % CPU.

| | run 1 | run 2 |
|---|---:|---:|
| exit code | **0** | **0** |
| wall clock | **339.68 s** | **336.80 s** |
| peak RSS | 5,776,432 KB (5.51 GiB) | 5,717,312 KB (5.45 GiB) |
| peak as share of the 90 GB arm | 6.12 % | 6.06 % |
| RSS samples | 162 | 161 |
| watchdog | armed 90 GB, **never fired** | armed 90 GB, **never fired** |
| timebox | never approached | never approached |
| sha256 | `fb4594b6…` | `fb4594b6…` |

**BYTE-IDENTITY: verified two independent ways** — equal sha256, and `cmp` over the two files
returning clean. **Every field of both runs' stdout is identical**, `SEAL`, `EMIT`, `DISK`,
`WINDING`, `PREVIEW` and `GATE` alike. The determinism property that held for all five earlier
corpus artifacts holds for the treant too, and the runs are 0.85 % apart in wall clock.

**A conditions difference, stated rather than buried: this is NOT a speed claim against J6.** J6's
470.83 s / 470.64 s were measured with `PLINTH_THREADS=4`; these runs used the machine default and
carry the emit stage on top. **The two numbers are not comparable and no improvement is claimed
from them.** What the thread-count difference does buy is a free determinism observation: the
snapshot digest reproduced J6's to the unit at a different pool size.

### C.4 HOW THE TREANT REACHED THE EMIT PATH — the hollow stage refused it

**The treant does not hollow, and that is the only reason this checkpoint needed any code at all.**
Run end to end at `063115f` before any change, twice, it exits 1 at 22.29 s / 22.42 s with the
byte-identical `hollow: native shell did not contain one negative cavity enclosed by positive
outer material` — `plinth_hollow::HollowError::InvalidShellTopology`. **The emit path was never
implicated and was never reached.** That refusal is **FU-34**, opened by this checkpoint, and it
is a property of the model, not of the export seam.

`first_artifact_chain::run_from_oriented` treated a hollow-stage `Err` as fatal; its oriented-solid
reserve existed only for a *support* refusal of a drilled shell. It now carries **OQ-1's own
recommendation**, built at last: the oriented solid is pre-cloned before the hollow stage consumes
it, and on `Err` it is handed to support planning through the same
`HollowedModel::solid_decision(BelowThreshold)` route the support-refusal path,
`embed_incidence_census` and every sealed-run gate already use, with the typed refusal recorded
verbatim in `SupportedGeometry::HollowStageRefused`.

**The provenance caveat, stated here as well as at the variant.** `SolidDecisionReason` has exactly
two values, `NoCavity` and `BelowThreshold`, and **neither means "the hollow stage errored"**. So
this artifact's `hollow_disposition` reads `Solid { reason: BelowThreshold }` — a verdict no stage
in this run actually reached. That is the F14 imprecision, inherited knowingly rather than hidden:
`SupportedGeometry::HollowStageRefused { refusal: "native shell did not contain one negative cavity
enclosed by positive outer material" }` is where this artifact's true provenance lives, it is
carried in the outcome, and a new test pins it. **Read the two fields together; the disposition
alone will mislead.**

### C.5 THE CHANGE IS INERT FOR EVERY MODEL THAT DOES NOT TAKE THE NEW PATH — proved, not argued

Control experiment in one worktree at one tip, same target directory, the diff stashed and restored
as the only variable. The five models that already emitted produce **byte-identical sha256 with and
without the change**:

| model | sha256 (control = with change) |
|---|---|
| Sphere | `d59044b82caeb169f77e6bc8778661491afe6f11ca2a8d4169a9118aad2cc9b8` |
| make_overhang_table | `9aea36488f308f3f6c3b4b0eff60d726148dd71be6902da409d226d303c4b504` |
| make_toed_slab | `12ebdcdee51ece68bfddad741998746d9dbc9eb89a08d055fb7e67f1be751a0d` |
| make_raking_blade | `9acfa2703bbd74fd21a7b718ba55b41605b2c432e684fbdf57a66db382da84ab` |
| make_thin_fin | `b73377981c3ecebb421f75702912cf4741bffba224a37cfea969f81675f94c1a` |

**A side effect that is reported rather than left to be discovered.** The fallback lives in the
shared composition, not behind a model-name special case, so `Cylinder` and `make_box` — the two
models `export-path-design-2026-08-07.md` filed as an unfixed "Refusal finding" with this same
message — now reach artifacts too. **`--all` is 7 of 7, exit code 0, where it was 5 of 7.** This
was not the goal of the change. Whether those two artifacts are wanted in `exports/` is an owner's
call; **neither was placed in the main checkout by this work.**

### C.6 WHAT THIS CHECKPOINT DOES NOT CLAIM

- **Nothing about ChituBox.** The format gate is structural — a fresh parse, reserialize and
  byte-compare — and is **not** a claim that the ChituBox GUI will open or slice this file
  (`chitubox_io/src/lib.rs:14-16`). The owner's GUI check is the semantic oracle and has not
  happened. This section records an artifact, not an acceptance.
- **Nothing about print quality, printability, or whether 1,383 members is a *good* forest.** The
  seal says the pipeline's own invariants hold. It says nothing about resin.
- **Nothing about hollowing the treant.** It is emitted **solid**. The resin saving hollowing exists
  to deliver is exactly what FU-34 is about, and this artifact does not have it.
- **No new pin.** `423276951048565625` is still not a frozen digest and the treant is still outside
  the OD-8 set. Two byte-identical measurements are evidence, not a golden.
- **No cap, floor, constant, coefficient or registry entry was raised, lowered or touched**, and no
  engine crate was modified: the diff is `crates/plinth-pipeline/examples/`, that crate's
  `tests/first_artifact.rs`, and docs.

### C.7 Gates

Five gates verbatim on the branch, tree clean after: (1) `cargo test --workspace --offline
--no-fail-fast` — **1067 passed / 0 failed**; (2) the same `--all-features` — **1072 passed / 0
failed**; (3) `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` —
clean; (4) `cargo fmt --all -- --check` — clean; (5) `git diff --check` — clean.

**Delta attributed: +1 / +1** against `063115f`'s 1066 / 1071, and it is exactly one test —
`hollow_stage_refusal_falls_back_to_the_oriented_solid_and_still_emits`, which gates the new
fallback on `make_box` and pins the refusal string, the `BelowThreshold` provenance stamp and the
face-preserving reserve. A new branch in the shared composition with no gate on it is what drifts.

**That test could not use `Cylinder`, and the reason is a second finding: FU-35.** `Cylinder` trips
FU-8f phase G6c's `debug_assert` in `specialized.rs:4858` — a retained round executes **23,203**
load operations against a charge of **4,146** — so any debug-profile test carrying it into
`run_support` panics. **This is not the fallback's doing and it is not new:**
`embed_incidence_census`, which shares none of this code, panics identically on `Cylinder` at this
tip in a debug build. See FU-35.

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

### Weeping Treant first artifact {#gui-checkpoint-treant-first-artifact}

- Project: `exports/weeping-treant-v1.2-first-artifact.chitubox` (main checkout; gitignored)
- SHA-256: `fb4594b623ad6ef0ea48541c9024dd8cc1dfd2b2bef44975e0ee3f73f2b9c7d1`
- Size: `28,670,352` bytes
- Owner checkpoint: **the artifact exists and satisfies the milestone's generation half; the GUI
  half is NOT yet done.** It has not been opened in ChituBox. The structural format gate passed and
  the bytes re-parse, which is not the same claim — record it as observed only once the owner has
  actually opened and sliced it.
- Full record: *CHECKPOINT 2026-08-09*
  ([`#checkpoint-treant-artifact-2026-08-09`](#checkpoint-treant-artifact-2026-08-09))
- Produced by branch `feature/checkpoint-treant-artifact`, base `063115f`, **VERIFIED-NOT-MERGED**.

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

None of the commits in this table is integrated at the current 2026-08-05 head. The
canonical delicacy row was removed on 2026-08-04 because that work is now **Integrated**
via merge `240d144`; see the *2026-08-04 integration pass* table above.

| Work | Branch/worktree | Commit(s) | Review state | Integration constraint |
| --- | --- | --- | --- | --- |
| grounded calibration artifacts | `codex/home-calibration-plate` at `/private/tmp/plinth-home-calibration-plate` | `634abdb`, `1541379` | **ACCEPT** and owner GUI-checked | Branch-only; independently review the two-commit series and its base before cherry-picking. |
The endpoint-connected-embed-zones row was **removed** on 2026-08-05: that package is now
**INTEGRATED and COMPLETE** as merge `81ec3d4`, and the census work it spawned is
integrated with it. See *Embed/contact package complete (2026-08-05)*. Both
`codex/embed-zones` (`dd505aa`) and `census/embed-incidence-2026-08-04` (`50b325c`) are
**fully contained** in the integration head — verified with
`git merge-base --is-ancestor <branch> HEAD` for each — so both are cleared for retirement.
See the inventory below for the retirement's execution state.

The preparation-resource-policy row was **removed** on 2026-08-04: that work is now
**INTEGRATED** as merge `c6a0884` via `codex/prepare-interface-v2`, and the superseded
branch `codex/prepare-interface` @ `7b35c49` was deleted with the containment proof recorded
under *Second 2026-08-04 integration pass*.

The canonical-delicacy follow-up is **CLOSED**. The requested explicit **M0-value and
signed-zero** equality/digest regressions — absent from `c2927a8`/`5f281d7`, which touch
`plinth-metrics` only — were added on `codex/delicacy-regressions` and merged as `1eb538c`,
mutation-verified by independent review.

The `move_characterization_` prefix introduced by those commits marks move oracles that
must stay bit-exact across the planned `plinth-metrics` -> `plinth-mesh` delicacy move.
Do not "fix" one of their pinned digests; investigate instead. The pins transitively depend
on libm `acos`, so a cross-platform CI difference there is a portability artifact, not a
regression.

## Active and pending work

Updated 2026-08-05 (late): after the merge to `develop` and the retirement of the two merged
branches, the live inventory is **`main`, `develop`, and three parked `codex/*` branches;
one definitive checkout plus three parked worktrees**. The paragraph and table that follow
describe the earlier pass and are kept for provenance; the struck-through rows record what
was retired and why.

This table is the live inventory as of the 2026-08-05 pass: **5 branches, 5 worktrees**.
`codex/embed-zones` and `census/embed-incidence-2026-08-04` were retired-in-substance
(content fully contained in integration); both refs and the one worktree between them are
now deleted — containment was proven before deletion, recorded in the rows beneath. Since the
first 2026-08-04 pass, `codex/integrate-canonical-delicacy-o5`,
`codex/delicacy-regressions`, `codex/prepare-interface-v2`, and `codex/prepare-interface`
and their worktrees were deleted after their content was verified integrated. The rows for `codex/canonical-delicacy`, `codex/home-test-protocol`,
`codex/migration-handoff-2026-08-03`, `codex/prepare-cli`, `codex/chitubox-diagnosis`,
`codex/shared-surface-authority`, `codex/m3-decimate-proxy`, and the five
`codex/support-*` drafts were **removed** because those branches and their worktrees no
longer exist. See `WORKTREE-DISPOSITIONS.md` for the per-branch proof, archive tags, and
the owner's untagged-deletion decision.

| Worktree/branch | Observed head | State |
| --- | --- | --- |
| `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, `develop` | merged 2026-08-05 | **The single line of record.** Definitive checkout and ref store. Carries the whole engine arc and the README overhaul as true merges. Gates green at the merged tip. New work branches `feature/*` from here |
| `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, `main` | `02e2471` | release branch; moves only when the owner releases `develop` into it |
| ~~`/private/tmp/plinth-3d-pipeline-integration`, `codex/rust-pipeline-integration`~~ | was `cef27a1` | **RETIRED AND DELETED 2026-08-05.** Merged into `develop` in full with `--no-ff` (two-parent merge, no squash, no rebase), so `cef27a1` is an ancestor of `develop` and every engine commit survives. Worktree clean at removal; branch deleted with plain `git branch -d`, which git accepted precisely because it was fully merged. No archive tag: an ancestor of `develop` is not recoverable-only-via-tag |
| ~~`/private/tmp/plinth-readme`, `feature/docs-readme-overhaul`~~ | was `207f84d` | **RETIRED AND DELETED 2026-08-05.** Merged into `develop` with `--no-ff` after the engine line so its links resolve; every relative link in the final README was checked against the develop tip. Worktree clean at removal; branch deleted with plain `git branch -d` |
| ~~`/private/tmp/plinth-embed-zones`, `codex/embed-zones`~~ | was `dd505aa` | **RETIRED AND DELETED 2026-08-05.** `git merge-base --is-ancestor dd505aa HEAD` passed first, so nothing was carried there alone; the worktree was clean (`git status --porcelain` empty, no `--force` needed) and was removed, then `git worktree prune`, then `git branch -D codex/embed-zones`. No archive tag: an ancestor of the integration head is not recoverable-only-via-tag |
| ~~`census/embed-incidence-2026-08-04` (branch only, no worktree)~~ | was `50b325c` | **RETIRED AND DELETED 2026-08-05.** `--is-ancestor` passed; the census is maintained in-tree at `crates/plinth-support/examples/embed_incidence_census.rs` behind the `census` cargo feature |
| `/private/tmp/plinth-home-calibration-plate`, `codex/home-calibration-plate` | `1541379` | accepted branch-only; plate and loading-fixture generator authority |
| `/private/tmp/plinth-chitubox-preview`, `codex/chitubox-native-preview` | `9ffc829` | only live carrier of the unlanded embed-native-preview fallback; cherry-pick onto integration |
| `/private/tmp/plinth-normalization-controller`, `codex/normalization-controller` | `fc5a60d` | parked for independent review as candidate coverage-repair machinery |
| protocol PDF/source | integrated at `0ac30f1` | home calibration protocol Markdown, Typst source, and PDF are integrated; the reviewed Typst 0.13.1 artifact is recorded separately from a byte-different Typst 0.15.1 reproduction |

Use `git worktree list --porcelain` and `git branch --list` as the authoritative inventory
before assigning or removing anything. Do not delete or move a worktree merely because it
is not listed above.

The 2026-08-04 hygiene pass took branches from 54 to 8 and worktrees from 14 to 8; the 39
deletions were originally archive-tagged via `archive/2026-08-04/<branch>` tags, but those
tags were removed at owner instruction on 2026-08-04 ("let git gc clear it out"). No
`archive/2026-08-04/*` tags exist; the deleted branches' commits are unreferenced and
unrecoverable once `git gc` runs, by deliberate owner ruling — same disposition as the five
vetoed `codex/support-*` drafts, `codex/m3-decimate-proxy`, and `codex/shared-surface-authority`,
which were deleted **untagged** at the owner's direction from the start and must not be
resurrected. Earlier
cleanup removed both stale `--list/` directories, approximately 278,241,280 bytes of
generated-only output; they are regeneration-only and must never be committed.

Corpus models are provisioned but deliberately uncommitted: all 8 files live in
`corpus/models/` matching `corpus/manifest.json` digests, and `.gitignore` line 44
(`corpus/models/*.stl`) keeps them out of git. `docs/rust-migration/corpus-provisioning.md`
records their sources and SHA-256 values so a clean checkout can re-provision them. Do not
use Python for migration implementation or ad-hoc verification.

## Follow-ups ledger {#follow-ups-2026-08-04}

Items raised by the 2026-08-04 and 2026-08-05 reviews. Each should be scheduled
deliberately. FU-18, FU-19 and FU-20 — the three that made the workspace gate red and the
inventory stale — were **resolved on 2026-08-05 (late)**; see *Post-merge interaction delta*.
Everything else here is non-blocking.

| # | Follow-up | Note |
| --- | --- | --- |
| FU-1 | Mechanize the class-(b) cap-table tie to `SupportPreparationInput::validate` | Currently the cap table and the validating boundary agree by inspection only. Needs a small production affordance to make the tie mechanical. |
| FU-2 | Decide `validate()` width vs per-type constructor contracts | Defer until the execution runner lands, when the real call sites exist to judge against. |
| FU-3 | Six sites still construct `NativeSurfaceConfig`/`ProbeConfig` with `usize::MAX` outside the validating boundary | The three latent fixture defects on the merged path were fixed; these six remain outside it. |
| FU-4 | Commit a corpus provisioning script, or make the corpus-dependent targets loud-skip | Today a bare checkout hard-fails two test targets with no explanation. See `corpus-provisioning.md`. |
| FU-5 | Pin the `scar.rs` duplicate registry-reader (`crates/plinth-metrics/src/scar.rs:183-186`) the way the new delicacy M0 test pins the `plinth-mesh` reader | The duplication is now documented in the delicacy test comment but is still unpinned on the `plinth-metrics` side. |
| FU-6 | `chitubox-native-preview` fix `9ffc829` still awaits review and cherry-pick | Sole live carrier of the unlanded embed-native-preview fallback. |
| FU-7 | `make_toed_slab` parity investigation (vertex vs. interior containment probe) | The digest-moving containment-probe hardening was deferred; the current `make_toed_slab` required-table freeze (digest `11_859_964_443_161_510_103`) is **still PROVISIONAL** until this lands, and so is the 2026-08-05 seal re-pin (`4826202191472851835`) — the freeze is now provisional **on the new baseline**. See *Corpus defect fixes landed (2026-08-04, late)* and the cross-reference in *Post-merge interaction delta*. **CLOSED 2026-08-08, solved digit for digit, by the FU-10 phase-C re-pin** ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). The old vertex-parity probe was evaluated at the loop's minimum-`PointKey` vertex — for an abutting toe, exactly the touching point — at a measured distance of ~1e-15 mm from the neighbour's boundary; it was deciding SOLID vs. HOLE by f64 round-off, coin-flipping between adjacent 0.05 mm layers on a continuously-growing feature (evidence: [`attribution-slicer-census-2026-08-07.md`](attribution-slicer-census-2026-08-07.md) §1.3). FU-7 proposed to *harden* that probe; phase C instead *retires* it, replacing parity with the union engine's structural shape hierarchy. Both land on the same number: `plan.contacts().len()` **72 → 42**, digit for digit. This closes only the freeze-blocking, digest-moving containment-probe half of the row; the OD-9 `BuriedSurface` 3D `surface.inside` question that shares this row is a different probe on a different question, its code untouched by the cutover, and remains open — split out rather than closed by association. |
| FU-8 | ~~Treant resource caps defect — "production caps do not admit a real mini"~~ | **PARTLY RESOLVED 2026-08-05, and the brief's framing was corrected by measurement.** Two of the three named caps were genuinely too small and are now model-scaled on the treant's own measurement: `KnnBatchLimits.max_work` → `max(100_000_000, 2048 × faces)` = 878,837,760, and `CandidateSurfaceGridLimit` → `max(67_108_864, 512 × faces)` = 219,709,440. The treant clears both. The third, Specialized `GateOperationLimit` (`2_000_000_000`), **was falsely accused and is deliberately NOT raised**: its reported `required: 2000000001` was `limit + 1`, a charge counter tripping on a selection sort in `unique_island_face_count`. That sort is gone as of 2026-08-06 (FU-8b fix (A)) and the treant now refuses with a *stated* demand, `required: 12714828544`, on gate work the fix does not touch; the cap was still not raised: FU-8b closed 2026-08-06 and it became the FLOOR of a measured batch-product rule (§3.10.10), unchanged at 2,000,000,000 and still the whole cap for every corpus model. Three further caps nobody had seen (`max_load_operations`, `max_load_scratch_bytes`, `max_gate_peak_bytes`) were correctly refusing an 8.0 M-row load-eligibility explosion on a 429k-face mesh; FU-8b fix (B) collapsed those rows to the 134,368 distinct faces they always were, after which the two byte caps are met with room to spare and only the operation cap needed the measured rule. **[Corrected 2026-08-06 (record fixes F1, F3): the preceding sentence is left standing but is wrong twice — it was a *predicted* row count that collapsed, not a row set (`build_load_eligibility` was already a per-face `BTreeMap` collapse at `eb36358`), and the post-fix figure is 130,548, not 134,368. See the dated addendum under *FU-8b resolved; the treant's stopping point moved*.]** Remaining work is algorithmic, tracked as FU-8b. See `docs/rust-migration/support-resource-caps-2026-08-05.md`, and *Owner caps-landing rulings R1–R5 (2026-08-05, late)* for the five adjudicated findings from that landing's review — R1 in particular removed a **third**, undisclosed scaled cap that this row's "two of three" framing did not count. |
| FU-8b | ~~Treant support gate/load stage is quadratic~~ | **RESOLVED 2026-08-06.** All three defects fixed and the deferred cap bound then set on post-fix measurement. **(a)** (`0bdec0c`) `unique_island_face_count` / `visit_served_owner_faces` found distinct face ids by repeated-minimum selection sort; they now gather each island once and `sort_unstable` + `dedup` in one batch. **(b)** (`1706c5d`) every consumer that had to predict the extended load row count took the `(owner, face)` **pair** count instead of the distinct **face** count — 7,988,078 against 134,368, 59.5× inflated; the pairs are now grouped by face once (`GroupedOwnerFaces`) and the two counts are stated separately (`SpecializedLoadCounts`). **(c)** (`e0a4bed`) `finalize_specialized_load_partition` rescanned the pair list per base row and per pair (~6e13 uncharged comparisons, the 16-hour hang, 4,127/4,127 stack samples in that one frame); it now joins base rows to the existing grouping in one linear merge, proved against a verbatim copy of the replaced algorithm over a 4×5 cross product of shapes. **Then measured, not assumed:** the treant's honest post-fix demands are 12,714,828,544 gate operations on its proposal-free batch and 5,402,535,619 load operations over 134,368 rows **[corrected 2026-08-06 (record fixes F1, F2): 130,548 rows — the demand is right, the row count paired with it was not; and in (b) above, 7,988,078 → 130,548 is 61.2×, and it was a *prediction* that was inflated, not a row set]**, with load scratch (144,753,256 B) and gate peak (215,061,920 B) comfortably under their unchanged caps **[corrected 2026-08-06 (record fix F7): the gate peak is **216,203,088 B** = 215,061,920 + 1,141,168 face-scratch as of `23d929d`, which charged the batch's shared distinct-face buffer against the bound; 215,061,920 was the peak at `d82c895` only. The 1,073,741,824 B cap is unchanged and the batch is still at 20.1 % of it]**, at 2.84 GB peak RSS and 81.5 s to refusal. Both operation caps therefore became `max(floor, 64 × batch_product)` on measurement — **not** on face count, since 24-face `make_raking_blade` charges 612,887,212 gate operations against the 429,120-face treant's 12,714,828,544. Every corpus model stays bit-identical at the floor and the census prints each materialization. See `03-physics-and-constants.md` §3.10.10 and `support-resource-caps-2026-08-05.md` §7. The treant now clears the gate's proposal-free batch and the whole load stage; its remaining refusal is a **new** defect, FU-8c. |
| FU-8c | ~~Specialized **merge** gate re-evaluates the whole batch per proposal~~ | **RESOLVED 2026-08-06** (`a3308ec`); the finding as first written is left standing below and the resolution is appended to it, not substituted for it. Opened 2026-08-06 by the FU-8b post-fix measurement, and the successor stopping point for the Weeping Treant. `gate_resource_plan` prices a merge batch at `(samples + demands + 1) × (proposals × (contacts − 1) + contacts)`: every merge proposal re-evaluates the entire contact set against every coverage sample. The treant offers **39,223 proposals** over 1,366 contacts and 405,146 samples, so the plan states **21,755,124,103,146** operations against a materialized cap of 35,445,500,736 — 613× over, and refused. This is the same batch-first violation as FU-8b (a)/(b)/(c) in a fourth dress: a per-item rescan of a whole batch standing in for one batched evaluation. It is **not** a cap to raise — admitting it would need a per-batch-unit coefficient near 39,300, which would multiply every corpus model's cap by the same factor and take the Cylinder (already at 89.6% of the floor), `make_raking_blade` and others off their floors, i.e. it would admit real quadratics on small models. The fix is to price and evaluate the proposal variants incrementally against the base batch rather than re-running it, then re-measure. Note for FU-23: fixing it will change the gate's charged work, so `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` should be re-derived afterwards rather than inherited. **[Resolution appended 2026-08-06 (`a3308ec`).** The proposal factor is gone. `GateBatchIndex` builds the proposal-invariant state — samples by route key, contacts by route key, three satisfying contacts per demand — once per batch; each proposal re-answers only the route keys its own merged pair serves and re-tests each demand once against the merged seat. `gate_resource_plan` is repriced to `(samples + demands + 1) × contacts + measured proposal work`, and the predecessor's per-variant sweep is retained `#[cfg(test)]` as the reference the delta path is compared against row by row on `f64::to_bits`. **No cap was changed** and no cap code was touched. The treant's stated demand fell from 21,755,124,103,146 to **201,480,535,329** — 108× — against the same 35,445,500,736. **It still refuses**: 5.68× over, at the same gate, re-measured byte-identically at `8695398` (124 s, 2.50 GB peak, watchdog never fired). The residual is 97.4 % of the demand and is a *different* defect, opened as **FU-8d**; FU-8c is closed on the defect it named, not on the treant reaching a seal. The FU-23 note above still stands and is now due. See *FU-8c fixed 2026-08-06*.**] |
| FU-8d | Per-proposal nearest is still `sources × samples` on the perturbed key, `unique_face_finalizer_work` is under-charged, and the per-proposal planning passes are uncharged and doubled | Opened 2026-08-06 by the FU-8c post-fix measurement; the successor stopping point for the Weeping Treant, and both parts are measured rather than predicted. **(1)** After FU-8c the per-proposal work is route-local but still a full `sources × samples` sweep of the perturbed key. On the treant that residual is **196,264,386,588** of the printed 201,480,535,329 — **97.4 %**, averaging 5,003,809 per proposal — because its coverage samples concentrate in a few very large route keys and its merge proposals sit on exactly those. It need not be `sources × samples`: retaining the **three** nearest sources per sample during the baseline sweep answers "nearest among the survivors" for any two-contact removal by inspection, leaving one distance test per touched sample against the merged seat, so the term becomes `samples` on the perturbed key. That is FU-8c's own top-`k` witness argument applied to the nearest instead of the demands. **(2)** The exact fold's `unique_face_finalizer_work` is `variants × faces × contacts` while `gate_resource_plan` charges it as `variants × faces`. On the treant the true term is ≥ 117,741 × 1,365 × 39,223 ≈ 6.3 × 10¹², which is also the arithmetic proving the refusal came from the plan and not from a completed fold. The under-charge is why this term has never been the refusing one; its superlinearity is why it will be as soon as (1) lands, so the two parts should be fixed together. **(3)** *[Third part added 2026-08-06; (1) and (2) are left standing as opened. Deliberately NOT coded yet — recorded so FU-8d is scoped honestly, and the cheapest of the three.]* **The per-proposal planning passes are themselves uncharged, and there are two of them.** `evaluate_gate_batch` runs `plan_proposal_nearest_delta` once per proposal inside `plan_gate_nearest_batch` (`specialized.rs:3553`, to build the plan) and **again** once per proposal inside the `exact_operations` fold (`specialized.rs:3571`, to charge it) — on the treant **2 × 39,223 ≈ 78,446 delta passes** — while the check that refuses the batch, `if exact_operations > gate_operation_limit`, is at `specialized.rs:3629`, **after both**. The entire cost of deciding the treant may not proceed is therefore paid *before* the refusal and **none of it is charged**: the plan charges the delta's *result* (`nearest_work`), never the cost of computing the delta. That is why the treant burns 124 s to state a demand it is then refused on, and it is a hole in what the cap means as much as a cost — a resource plan that cannot be afforded until after it has been executed bounds nothing. The second pass is a **free 2×** to reclaim: the fold recomputes a delta the planning pass already computed for the same proposal, so retaining the planning pass's per-proposal deltas turns two passes into one with no change of result. Sequence it **with (1), not before it** — (1) rewrites what a delta pass *is* (`sources × samples` on the perturbed key becomes `samples`), and halving the count of a pass about to change shape optimizes the wrong thing. Corollary, and the reason this is a part rather than a footnote: after (1) and (3) the planning work is small enough to charge, so **FU-8d should charge it**, rather than leaving the gate free to spend unbounded effort discovering it cannot afford to proceed. A fourth candidate — the per-variant `assess_coverage`, `O(samples)` per variant under its own `max_coverage_operations` cap — is recorded as **unmeasured**, not as a finding, because the treant has never reached it. As with FU-8c none of this is **a cap to raise**. Note for FU-23: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` should be re-derived after FU-8d lands, not after FU-8c. **[Phase F1 landed 2026-08-06 — the finding stays OPEN in all three parts; F1 changes no answer and buys no speed.** The baseline coverage sweep now retains `min(3, source_count)` nearest sources per sample — `(source_slot, squared_distance)` in the kernel's emission order — which is the witness table part (1)'s delta will read; nothing reads it yet. Pinned first by two tests: `fu8d_knn_row_zero_is_k_independent_bit_for_bit` (the kernel's row 0 is bit-identical at k = 1 and k = 3, over engineered exact ties and random clouds — the design's riskiest claim) and `fu8d_three_retained_witnesses_answer_any_two_removal` (three retained slots answer any two-contact removal exactly, against a kernel query over the survivors; it fails if the slot count is set to 2). Charges: **no operation charge moves** — `KnnBatchPlan`'s work is `source_count x query_count`, k-independent (ruling R-a, recorded in the `NEAREST_WITNESS_SLOTS` doc comment). Bytes do move and are charged: `+2 x query_count x 16 B` per route inside `KnnBatchPlan::logical_peak_bytes`, and `samples x 3 x 16 B` per batch as the new `GatePeakUsage::nearest_witness_bytes` (~19 MB on the treant, against the unchanged 1 GiB gate peak). `coverage_route_knn.max_neighbors` goes 1 -> 3 (ruling R-b), stated once in `SpecializedLimits::production`: a `KnnBatchLimits` **shape field, not a cap**, and the `SUPPORT_KNN_MAX_NEIGHBORS` registry note, ledger entry, `constants.md` and §3.10.9 table each carry the dated amendment. **No cap raised, no cap code touched.** Five gates green; all seven frozen OD-8 digests byte-identical; the treant's refusal line unchanged at `required: 201480535329, limit: 35445500736` — that immobility is the phase's proof. See *FU-8d phase F1 landed 2026-08-06*.]** **[Phase F2 landed 2026-08-06 — part (1) is CLOSED; parts (2) and (3) stay OPEN, and part (2) is now the treant's wall.** A merge proposal's nearest is now the minimum of two things: the first surviving retained witness, read as a *value*, and ONE `KnnBatchPlan(1 source, touched_rows queries, k = 1)` over the merged seat covering every perturbed key at once — combined by `f64::total_cmp` with the square root taken once *after* the minimum, so an exact survivor/merged tie is a tie between bit-identical `f64`. Pinned by `fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit` against the retained `#[cfg(test)]` full sweep, on a fixture that asserts it reached a narrow key, a total removal, a displaced holder, a steal, and an exact tie; truncating the witness scan to slot 0, or dropping the merged seat from the minimum, each make it fail. Repriced in `gate_resource_plan`: `nearest_work` becomes `Σ 1 × touched_rows`, `key_visits` is newly charged. Measured on the treant: the per-proposal nearest term **196,264,386,588 → 2,083,167,818 (94.2×)** — reconciling exactly as `195,222,841,902 + 1,041,544,686` — and the stated demand **201,480,535,329 → 7,299,316,559 (27.6×)**, **20.6 % of the unchanged 35,445,500,736**, so that check now PASSES. **No cap raised, no cap code touched.** The refusal moved one site: `variant_specialized_owner_face_count`'s descriptor budget crosses at **variant 275 of 39,224**, printing `required: 35445618202, limit: 35445500736` — a crossing, not a total. Over those 275 variants `unique_face_finalizer_work` is already 3,036,066,151,957, extrapolating to ~4.33 × 10¹⁴, against routed work of 3,912,181: **part (2) is the whole remaining wall.** Rulings recorded: **R-c** measured and NOT adopted (the radius bound removes 42 % of rows, a 1.7× constant, and the margin is 4.9× — over the 2× threshold); **R-d** precondition met (28 of 48 perturbed keys carry ≥ 10 proposals, one carries 26,543) but deliberately not built; **R-f** NOT satisfied — `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` waits for F3 *and* a treant run that COMPLETES the gate, which this is not. Five gates green; all seven frozen OD-8 digests byte-identical, `make_raking_blade`'s 1,402 real merge proposals through the rewritten code being the live witness; treant run 129 s at 2,641,504 KB (2.52 GiB) peak RSS. See *FU-8d phase F2 landed 2026-08-06*.]** **[Phase F3 landed 2026-08-06 — parts (2) and (3) are CLOSED, and with (1) closed at F2 this finding is RESOLVED on the defect it named. The treant's merge gate no longer refuses.** The load partition is finalized **once** per batch, retaining three nearest eligible contacts per row plus each contact's won rows, carried area and violation state; a proposal re-answers only `elig(left) ∪ elig(right)`, which the merged seat serves *exactly* because `parts_serves_owner` is true for an island precisely when `left` or `right` serves it. Rows outside the union keep a bit-identical candidate set in the same order, so their argmin is the baseline's. Carried areas are **re-summed** in the fold's own ascending row order rather than adjusted incrementally, because `f64` addition is not associative. Pinned by `fu8d_load_delta_matches_the_per_variant_finalize` against the whole production finalizer run on hand-renumbered variant inputs, row by row, on `load_violation_count`, and on carried area by `f64::to_bits`; it asserts it reached an exact survivor/survivor tie, an exact survivor/merged-seat tie, a row with no survivor, a displaced holder, a steal, and a row outside the union. The **per-variant window matrices are deleted, not repriced** — `gate_window_matrix_bytes` stated tens of gigabytes against the 512 MiB `max_gate_contact_matrix_bytes` and would have become the next refusal the moment the operations wall fell. Planning is 4 → 1 with a count-derived pre-check (`proposal_planning_precheck`) run *before* the loop, and the per-variant contact/demand rescans are derived (`GateVariantTotals`; a proposal variant's logical contact work is the baseline's minus one, exactly). Repriced to ONE authority: the `variants × faces` load model is replaced by `specialized_load_resource_plan`'s own figure — 4,618,272,984 → **6,592,813,742**, i.e. the honest number is 1.43× the under-charge part (2) named — while `descriptor_operations` falls from 35,445,417,382 at variant 275 (~5.06 × 10¹² extrapolated) to **136,791,451** charged once, and `unique_face_finalizer_work` (~4.33 × 10¹⁴ extrapolated) ceases to exist. Stated demand **9,273,857,317** and `exact_operations` **10,457,910,996** against the **unchanged** 35,445,500,736 — 26.2 % and 29.5 % — and the batch **executes**. `GateRoutePlanMismatch` is re-founded as plan-vs-execution and is explicitly **weaker**; the record says so. **No cap raised, lowered or touched.** The treant planned 8 gate batches (proposal-free + 7 merge rounds, 1,366 → 1,360 contacts, share flat at 26.2 %) inside the 600 s timebox and did **not** finish the tournament: recorded as EXCEEDS-TIMEBOX with counters, ~2.4–2.5 GB peak RSS, watchdog armed at 90 GB and never fired. The successor is the per-variant `assess_coverage` — 39,224 × 405,146 ≈ 1.59 × 10¹⁰ uncharged sample visits per batch — opened as **FU-8e** on this measured evidence. **R-f still NOT satisfied**: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` waits for a run that COMPLETES the gate. Five gates green; all seven frozen OD-8 digests byte-identical, with `make_raking_blade`'s 23 batches and 1,402 proposals through the rewritten *load* path the live corpus witness. See *FU-8d phase F3 landed 2026-08-06*.]** **[Phase F4 measured 2026-08-06 — FU-8d stays RESOLVED; F4 lands no engine code and closes the finding's record rather than its algorithm.** F4's deliverables are a clean treant run at the committed F3 tip and the record sweep F3 still owed. **Outcome (c) EXCEEDS-TIMEBOX, twice** — 607 s / 2.49 GiB and 602 s / 2.48 GiB, `PLINTH_THREADS=4`, watchdog armed at 90 GB and **never fired**, **no typed refusal at any cap in either attempt**, no batch counters (the tip carries no `PLINTH_FU8D_MEASURE` instrumentation, verified absent by grep, so §10's per-batch table stays the counter record). Attempt 2 sampled six windows across the timebox because attempt 1's single window at t = 600 could not separate "dominates the run" from "was running at the end". **The design predicted the wrong frame.** `assess_coverage` (part (4)/P4, the deferred candidate) is real but **second**, at 24–34 % of main-thread samples; the leader at **59–62 %** is **F2's own `fill_proposal_nearest_delta`**, and its cost is construction rather than arithmetic — it builds a fresh `PointCloudIndex` kd-tree over a **one-point** cloud once per proposal, 39,223 constructions and teardowns per gate batch, its call-graph children being `knn_batch_planned` over `_xzm_free`/`_free`/`_xzm_xzone_malloc` allocator traffic. Batched *within* a proposal, unbatched *across* proposals. Both go to **FU-8e**, in measured order. Also recorded, not acted on: `PLINTH_THREADS=4` buys the merge gate nothing (all four rayon workers in `__psynch_cvwait` for the whole gate stage while preparation is genuinely parallel — a note, **not** a reason to raise thread counts), and `merged_violation_count` is a steady third at ~4.6 %. **R-f STILL NOT satisfied** (neither attempt completes the gate, so `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is not re-derived); **R-c revived as a question** — the frame a radius bound would have shrunk is now the measured leader — recorded as FU-8e input and not adopted. **No cap raised, lowered or touched.** F4 also pays what F3's every-record-site sweep missed: `03-physics-and-constants.md` §3.10, the `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` and `SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS` registry notes (hence a regenerated `constants.md`), and `specialized.rs`'s `gate_operation_limit` doc comment all still said the treant was refused at the merge gate; and the caps doc's phase-F3 section had landed as a duplicate `## 9`, now **§10**. See *FU-8d phase F4 measured 2026-08-06* and `support-resource-caps-2026-08-05.md` §11.]** |
| FU-8e | The merge gate's per-proposal kd-tree construction, and the per-variant `assess_coverage`, are both uncharged and both unbatched across proposals | **Opened 2026-08-06 on FU-8d phase F4's measured evidence**, which is the footing the deferral ruling required — and opened on **two** parts in the order the sampler found them, not the one the design predicted. The treant's merge gate no longer refuses anything: since F3 it states 9,273,857,317 against an unchanged 35,445,500,736 and is admitted. What stops the treant is **wall-clock**: two 600 s attempts, both EXCEEDS-TIMEBOX, at 2.5 GiB peak RSS with the 90 GB watchdog never firing. **(1) `fill_proposal_nearest_delta` — 59–62 % of main-thread samples, the leader.** F2 correctly batched a proposal's nearest into one `KnnBatchPlan` covering all its touched rows; it did not batch *across* proposals. Each of the treant's 39,223 proposals constructs a `PointCloudIndex` kd-tree over a **one-point** cloud (`[proposal.position_mm]`), queries it, and drops it — 39,223 constructions and teardowns per gate batch, whose sampled children are `knn_batch_planned` over `_xzm_free` / `_free` / `_xzm_xzone_malloc` and `kiddo`'s `nearest_n_within_recurse` descending a single-node tree. A distance to one point needs no spatial index at all; the batch-first fix is to answer every proposal's touched rows without per-proposal index construction. **(2) `assess_coverage` — 24–34 %, second.** `O(samples)` per variant with ~7 `Vec::with_capacity(405,146)` allocations per call: `39,224 × 405,146 ≈ 1.59 × 10¹⁰` sample visits and of order a terabyte of allocation churn per gate batch, none charged against `gate_operation_limit`. This was the *only* part the design predicted, and F3's record asserted it was "what the 600 s is being spent on"; F4 measured it at about a quarter. Both parts are **uncharged**, so **neither is a cap to raise** — a cap cannot be too tight for work it never prices, and the remedy removes work rather than admitting more. Third and named so it is not a later surprise: `SpecializedLoadBaseline::merged_violation_count` at a steady ~4.6 %. Two riders from FU-8d: **ruling R-c is revived as a question** (§9.5 declined the radius-bounded steal path at a 4.9× margin against a 1.7× constant — F4 makes the frame it would have shrunk the measured leader, so re-measure before deciding), and **`PLINTH_THREADS=4` buys the merge gate nothing** (all four rayon workers sit in `__psynch_cvwait` for the whole gate stage while preparation is genuinely parallel), which is a note about the wall-clock being single-core and **not** a licence to raise thread counts. **Ruling R-f still gates FU-23**: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is re-derived only from a treant run that COMPLETES the gate, and no run yet has. See *FU-8d phase F4 measured 2026-08-06* and `support-resource-caps-2026-08-05.md` §11. **[Phases E1–E4 landed 2026-08-07 — RESOLVED on the defect this row names, and NOT on the treant reaching a seal; every sentence above is left standing, and the two it gets wrong are corrected rather than rewritten.** The gate no longer builds a `PointCloudIndex` per proposal (E1's `knn_single_source_planned_into`, whose bit-identity with the tree's answer is argued in five clauses and pinned by `fu8e_single_source_is_the_tree_answer_bit_for_bit`), and no longer re-runs `assess_coverage` for a variant that cannot have moved the objective (E3's retained baseline objective plus a two-counter probe folded into the delta's own row loop). **First correction: the 62 % was never index construction.** One one-point `PointCloudIndex` costs 83.3 ns and the 39,223 of them total **≈3.27 ms** per batch — the trees were free. **[Arithmetic corrected 2026-08-07 (records fix): this total was published as "16.7 microseconds", an arithmetic slip in the design pass — 39,223 × 83.3 ns = 3.267 ms, and 16.7 µs would have implied 0.426 ns per construction. The per-build 83.3 ns stands as measured; only the product is corrected, and the conclusion is unchanged — 3.27 ms is 0.009 % of the 37.33 s `fill_proposal_nearest_delta` frame, so construction remains negligible and the fix still targets the query loop. Primary note: `support-resource-caps-2026-08-05.md` §12.2.]** The cost was the query loop, ~1.04 × 10⁹ query iterations and ~2.08 × 10⁹ heap allocations per batch across two tree descents and a `sort_by` per query. **Second correction: the 28 % was never allocation churn.** "~7 `Vec::with_capacity(405,146)` per call … of order a terabyte" is six vectors in `RepairNoEvidence` mode over a traversal that visits only uncovered rows; what costs is the two unconditional full scans bracketing it, ≈254 GB of reads per batch ≈ 17 s at memory bandwidth. **The fix was to not scan, not to not allocate.** A third correction was a soundness fix: the design's status-**flip** trigger is unsound because `worst_gap_mm` is a `max` over the nearest *values* of a failing island's samples, so the shipped trigger is the wider "an uncovered row moved". **Measured 2026-08-07 under ruling R-g** (`PLINTH_THREADS=4`, RSS watchdog armed at 90 GB and never fired, one attempt of a permitted two): merge round **63.83 s → 14.68 s (4.35×)**, variant loop **60.23 → 10.73 s (5.6×)**, `fill_proposal_nearest_delta` **37.33 → 4.67 s**, coverage **17.05 → 0.001 s**, `merged_violation_count` **unmoved** at 4.98 s mean. `touched_rows` **1,041,544,686 to the row** in two independent runs; **zero** flipped rows, **zero** moved uncovered rows, **zero** coverage fallbacks across 33 rounds and ~1.29 M variants. **The stated demand did NOT move: 9,273,857,317 against the unchanged 35,445,500,736**, because `plan_proposal_nearest_delta` is untouched — no record site that quotes it moves, and that immobility is the phases' proof. Peak RSS 2.446 GiB. **The outcome is still EXCEEDS-TIMEBOX, at 601 s with 33 merge rounds**, and the arithmetic says it had to be: one merge commits per round, ≤1,365 rounds by the contact bound and ~460 at `make_raking_blade`'s completion ratio, so the gate stage is ≈1.8–5.5 h, 11–33× the box. **Ruling R-f is therefore STILL NOT satisfied and nothing was re-derived.** **R-c NOT adopted** (post-fix it saves at most 1.9 s of a 13.8 s round, and nothing at all if FU-8f lands); **R-d NOT adopted** (no per-key sweep left to amortise; the 6.30 ns row body is gather-dominated); the optional `merged_violation_count` buffer fold-in was **measured at 0.13 % and declined**. The post-fix leader is **`merged_violation_count` at 36 % of the round** — F4 sampled it at 4.6 % and the design predicted 29 %, both low — and the successor is opened as **FU-8f**, a recorded finding awaiting owner ruling. Five gates green; all seven frozen OD-8 digests byte-identical with `make_raking_blade`'s 1,402 real proposals the live witness. See *FU-8e phases E1–E4 landed 2026-08-07* and `support-resource-caps-2026-08-05.md` §12.]** |
| FU-8f | Lazy first-safe gate acceptance, and non-incremental batch preparation | **RECORDED FINDING, 2026-08-07 — AWAITING OWNER RULING. Deliberately NOT implemented.** Opened on FU-8e phase E4's measurement as the treant's successor wall, in two parts, of which only the second is unambiguously an agent's to build. **(1) The gate evaluates 39,224 variants to answer a question that stopped at the first one.** `evaluate_gate_batch` returns a `vectors` array with one `FrozenSpecializedGateVector` per variant, and the tournament consumes it through exactly two lines — `let baseline = vectors[0]` and `first_safe_proposal(&vectors, baseline)`, which is `vectors.iter().skip(1).position(|v| gate_not_worse(*v, baseline))`. It is a `position`, not a search for a best, so everything after the accepted index is built and discarded. **`first_safe_proposal` returned `Some(0)` in every round measured** — 4/4 at the FU-8e tip, 28/28 across two models in the design pass. Lazy evaluation would take the round from 13.8 s to ~3.9 s (≈30 min for 460 rounds against ~1.8 h) and is observationally identical downstream: `SpecializedGateEvidence` retains only `baseline`, `accepted` and counts, and `evaluated_pair_count` is `total_proposals`, the number **gathered** rather than evaluated. **Why it is not built: it severs charged-batch from executed-batch.** FU-8d phase F3 re-founded `GateRoutePlanMismatch` as a plan-vs-execution attestation — every planned proposal executed, once and in order — and lazy evaluation breaks that invariant by construction, requiring the attestation to be re-founded a **second** time, on a weaker claim, two phases running. That is an owner's call, not an agent's, and it is what this row awaits. **(2) Batch preparation is rebuilt from scratch every round, and part (1) is not sufficient without it.** Preparation is **3.88 s of the 13.8 s round** — the 554,926,572-row baseline coverage sweep plus the `GateBatchIndex` — while a merge changes **2 contacts of 1,366**. At 460 rounds that is 1,785 s with the variant loop at zero, already **~3× the 600 s box**, so getting the treant under the timebox needs preparation to become incremental across rounds whatever is ruled on part (1). Part (2) touches no attestation and is the larger win. **Neither part is a cap to raise**: as with FU-8c, FU-8d and FU-8e, none of this work is charged against `gate_operation_limit`, and the remedy removes work rather than admitting more. See *FU-8f opened 2026-08-07* and `support-resource-caps-2026-08-05.md` §12.6. **[Status note 2026-08-07 — the row above was written while FU-8f was design-only and is left standing exactly as written: since it was recorded, phases G1 (the load-partition transpose) and G2 (the fingerprint hoist) have been implemented on the sibling branch `feature/fu8f-load-transpose` and are pending the merge queue, while G3 and later are pending this branch's own merge.]** **[Status note 2026-08-07 (later) — the row above and the note above it are both left standing exactly as written; two of their claims have stopped being true and are corrected here rather than rewritten. **This row is no longer AWAITING OWNER RULING and part (1) is no longer "deliberately NOT implemented".** The ruling came — *"2.1 hours down to 28 minutes seems worth it; anywhere else that we can also still shave more time off?"*, Craig, 2026-08-07 — and **phase G3 landed part (1)**: `GateBatchAcceptance` / `GateBatchOutcome`, the tournament breaking at the first `gate_not_worse` variant, `first_safe_proposal` retained `#[cfg(test)]` as the reference oracle it is proved against, `GateRoutePlanMismatch` re-founded a **second** time on a prefix-wise comparison with the ruling quoted verbatim at every record site, and the telemetry corrected (`evaluated_pair_count` → `gathered_pair_count`, plus a new unhashed `evaluated_variant_count`). **No charged number, cap, floor or coefficient moved**, and the plan still prices — and the refusal still runs against — every variant: charged and executed are severed, and both are stated. Seven frozen OD-8 digests byte-identical, `make_raking_blade`'s 21 real merges the live witness. Part (1)'s own sentence *"evaluated_pair_count is total_proposals, the number gathered rather than evaluated"* is the defect G3's rename fixes. **Part (2) stays OPEN**, and G1+G2 addressed only its cheapest components: a treant run at the G3 tip on a **quiet, uncontended machine** is still **EXCEEDS-TIMEBOX at 600 s**, so the round count remains unmeasured and **ruling R-f remains unsatisfied**. Counting the rounds is G4's. See *FU-8f phase G3 landed 2026-08-07* and `support-resource-caps-2026-08-05.md` §13.]** **[Status note 2026-08-07 (G4 measured) — everything above is left standing; G4 changed no engine code, so nothing above can have moved, and what follows is measurement rather than correction. **The rounds are counted at last, and the gate still does not fit.** Two attempts on a **quiet, uncontended machine** — the first frame-level measurements of this stage without a contention caveat — both **EXCEEDS-TIMEBOX at 602 s**, at **436** and **430** merge rounds, contacts 1,366 → 931/937, one merge committed per round without exception, peak RSS 2.554 / 2.528 GiB with the 90 GB watchdog never firing. Mean round **1,099–1,121 ms**, fitting `round_ms = 304.17 + 0.6924 x contacts`, which projects the **gate stage alone** to **8.4 min** at `make_raking_blade`'s 463-round completion ratio and **17.7 min** at the 1,365-round hard bound — **2.8x and 2.0x the design's D4 projection**, and neither inside the box. **Part (1) is confirmed delivered by measurement:** the variant loop is **0.52 ms of a 1,121 ms round**, from 60.23 s at `7aa6b73`, better than the ~1.6 ms D1 predicted; G2's fingerprint hoist is likewise invisible at 0 ms from 166.6. **Part (2) is now attributed to a single frame.** `build_gate_load_state` is **85.5 %** of the round, and inside it the miss against D4 is entirely **G1's transpose *build*, not its scan**: the transposed scan costs **47.4 ms** where the old `contacts x rows` scan cost ~3,090 ms (a **65x** fall, better than the 53x predicted), while sizing and filling the CSR cost **406.4 + 410.5 = 816.9 ms** against a design assumption of ~15 ms. The design's own "largest unattributed residual", `gather_pair_proposals`, is settled at **28.6 ms / 2.5 %** and is not a target. **One design claim is superseded by evidence:** D1 states `first_safe = Some(0)` in every observed round and that a `k > 0` case "is the one thing the corpus cannot supply" — the treant returns `Some(0)` for rounds 0–45 and **`Some(2)` for every round from 46 onward**, identically in both runs, so the treant is now a live 390-round corpus witness for the `k > 0` prefix and the forced fixture is no longer the only one. Nothing in G3 is invalidated: the oracle equivalence is proved for every index. **No cap came near binding and none was touched** — cumulative gather 15,544,641 over 436 rounds against `max_merge_total_proposals` = 100,000,000, projecting ~33.0 M at the hard bound. **No new typed refusal; the pipeline never left the gate stage**, so tournament commit, penetration/brace, CCO anchors, attestation and a first treant seal are all still unexercised and nothing is claimed about them. **Ruling R-f remains UNSATISFIED and `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` was not re-derived.** The next lever is named by measurement rather than by design: **make the CSR sizing pass stop being a second full traversal**, which the design's own D2a notes the gate already nearly holds in `contact_rows`. See *FU-8f phase G4 measured 2026-08-07* and `support-resource-caps-2026-08-05.md` §14.]** **[PHASE G5 LANDED 2026-08-07 — the honest load charge; everything above stands and four quoted numbers move DOWNWARD.** G5 retired `load_resource_plan_from_counts`'s `rows × contacts × (owner_log + 4)` scan term — the model of the dense scan G1 had already replaced, and which G1 deliberately left standing and over-stating — in favour of the transpose's own `Σ\|eligible_owners\| × (log + 2) × 2` search term plus a `cells × (LOAD_WITNESS_SLOTS + 5)` fill-and-scan term charged from the exact cell count. Measured on the treant's round-1 gate batch, quiet machine, one instrumented release run reverted by editing before the gates: load baseline **5,543,831,115 → 795,766,031**, gate load term **6,592,813,742 → 1,844,748,658**, stated demand **9,273,857,317 → 4,525,792,233** (26.2 % → 12.8 %), `exact_operations` **10,457,910,996 → 5,709,845,912** (29.5 % → 16.1 %). The fall is 4,748,065,084 at both, to the unit, and is entirely the load term. **No cap raised, lowered or touched; all seven frozen OD-8 digests byte-identical.** Two findings recorded rather than glossed: (1) the fall is not universal at every checkpoint — `make_box` and `make_thin_fin` each have three tiny load checkpoints that RISE by ≤56 units, though every model's *widest* demand falls and all are >4 million× under the 2,000,000,000 floor; (2) at 795,359,775 the treant's load demand is now **below** that flat floor, so no corpus model leaves it any more — the batch-product scaling is kept unchanged regardless, because re-deriving the coefficient from the cheaper observation would **lower** a cap. **G5 changes no executed work**, so the treant's stopping point is unchanged and still wall-clock; **R-f remains UNSATISFIED** and G5 ran no completion attempt. Open question 4 (`plan.vector_bytes`) is deliberately LEFT, with the follow-up named: metering `check_gate_peak`'s executed `result_bytes` beside the charged bound. See *FU-8f phase G5 landed 2026-08-07* and `support-resource-caps-2026-08-05.md` §15.]** **[PHASES G6a–G6c LANDED 2026-08-07/08 and PHASE G6d MEASURED 2026-08-08 — part (2) is CLOSED for the gate stage, and the treant's stopping point has moved out of the gate for the first time.** Everything above is left standing; G6a–G6c landed the cross-round retention of the load transpose (carrier + four mutation-proven guards, the incremental arena update with the bit-identity harness and its mutation triad, and the charge-vs-meter severance) and **G6d changed no engine code at all** — it is the measurement. **THE GATE COMPLETES.** Two attempts on a machine polled clean of sibling `cargo`/`rustc` thirteen times before launch, both finishing the whole model run in **339 s against the 600 s box (1.77× margin)** with the tournament committing **823 merge rounds** over **826 gate batches** — and the two runs are **bit-identical on every counter, with zero differing rounds across all 823 on contacts, proposals and rebuilds**, agreeing to 0.51 % on wall-clock. **804 of 826 load builds are retained (97.3 %)**; the 22 counted rebuilds are 21 `poisoned` and the single `contact_witness` miss at the terminal audit that F-G6-4 predicted by design. Executed work is **11.18 %** of charged run-wide and `executed ≤ charged` held every round. **The tournament ends because no proposal in the final batch is safe** — the acceptance fold's `None` arm, never before exercised — which falsifies **both** standing round predictions in opposite directions (463 too low, the 1,365 hard bound too high; the answer is 823). The retained round is **206.5 ms** against the pre-G6 round measured **in the same process** by the 21 rebuild rounds, which reproduce G4's own model to a mean of −0.97 % — a **4.4–5.2×** fall, and the first prediction in this chain to land (D-G6-5 said 0.19 s and 350–450 s; measured 0.206 s and 339 s). **G6e is NOT opened** (margin 1.77× > 1.5×, and the gate is no longer the binding stage). **Ruling R-f's precondition is now MET and re-deriving `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is UNBLOCKED — G6d deliberately does not do it**, per D-G6-6. **No cap, floor or coefficient is raised, lowered or touched by any G6 phase.** **What stops the treant now is a typed refusal in previously-unexercised code, opened as FU-8g**, and there is still **no seal**. See *FU-8f phase G6d measured 2026-08-08* and `support-resource-caps-2026-08-05.md` §17.]** |
| FU-8g | The initial-contact preflight bounds work at `contacts x faces` against a **flat** 4,000,000 cap | **Opened 2026-08-08 by FU-8f phase G6d's completed-gate measurement — the treant's successor stopping point, and the first one that is not the merge gate.** With the gate stage no longer binding, the Weeping Treant now stops **after** its tournament with `Preparation: Initial(Limit { resource: "work items", required: 232585208, limit: 4000000 })`, identically in both G6d attempts. **Derived to the unit from source.** `initial::preflight` (`crates/plinth-support/src/initial.rs`) checks `work = contact_count x 4 + penetration.max_total_work` against `ContactConstructionLimits::production().max_work_items = 4_000_000` (`runner.rs`). `penetration` comes from `penetration_intervals_batch_worst_case_limits(contact_count, 8, interval_cap, false)`, and inside `penetration_requirements` (`rust/geom_kernel/src/queries.rs`) with `rays_all = false` that is `rays = 1`, `hits_per_ray = mesh.nfaces()`, `work = rows x rays x hits_per_ray`. So `work = contact_count x (4 + nfaces)` = **542 x (4 + 429,120) = 542 x 429,124 = 232,585,208**, reproducing the printed figure exactly and independently confirming that 542 physical contacts survive the tournament and its terminal coalesce. The demand is **58.1x** the cap. **The shape is the FU-8b / FU-8c family a fourth time: a `contacts x faces` product measured against an unscaled flat bound.** At 429,120 faces this cap admits at most `4,000,000 / 429,124 = 9` physical contacts; the corpus passes only because every corpus mesh is two to four orders of magnitude smaller in face count, so nothing about the treant is special except its size. **And the quantity is a preflight *bound*, not a measured demand** — `hits_per_ray = nfaces` is the "every face could be hit" worst case while the executed query is BVH-bounded, the same over-statement G5 corrected on the load term and in the same direction. **This is NOT a cap to raise**, and nothing in G6d proposes raising it: as with FU-8b, FU-8c, FU-8d, FU-8e and G5, the remedy is to make the bound honest, not the cap bigger, and it is an owner's call whether and when to open it. **Rider, recorded here because G6d found it and it is one counter wide:** `RetainedLoadRebuildCause::Poisoned` collapses guard 3 and every `record_commit` identity-walk refusal — including the G6b position-bit tie — into a single census label, so G6d could count 21 poison-driven rebuilds on the treant but could **not** say which refusal fired. Splitting that label would make the next run's transcript self-explaining. See *FU-8f phase G6d measured 2026-08-08* §G6d.8 and `support-resource-caps-2026-08-05.md` §17. **[Post-verification adjudication 2026-08-08 — the records verifier asked whether a headline resting on uncommitted scratch transcripts is landable; the orchestrator ruled that it is, on the **G4 scratch-transcript precedent** (run outputs never enter git, and G4's own 436/430-round headline landed the same way), and **WIDENED the rider above**: the `Poisoned` split must **also emit a committed-round counter**, so that a future run's transcript proves its own round count rather than resting on scratch.]** **[H0 MEASURED, H1 FIXED, H2 CHARGED — 2026-08-09 (**[`#fu8g-h2-2026-08-09`](#fu8g-h2-2026-08-09)**).** The `contacts × faces` bound is gone from the charge. H1 (`08db663`) landed the census pass — one allocation-free counting walk per ray, per-row budgets, `work` (traversal tests) severed from `hits` (crossings), the F10 closed-form sample bound, `CensusDrift` as the typed refusal, and all five production call sites moved through the one shared entry point, which also killed the ~67 GB attestation eager reserve. H2 added the charged-vs-executed receipt (`PenetrationBatchMeter`, checked in the kernel against its own count **and** at `initial::preflight` against the number it spent), proved with a fixture that an over-demand mesh (counted work 4,223,900) is **still refused at the unchanged 4,000,000 cap**, and recorded the per-model before/after charge. **The treant's charge is 108,378 — 2.71 % of the untouched cap** (was 237,305,572). **NO CAP RAISED, LOWERED OR TOUCHED**, and D5.3's cap-scaling contingency is not triggered and not added. **Finding: the design's D4 claim that every corpus demand falls is FALSE** — `make_box` (205 vs 176) and `make_raking_blade` (975 vs 924) charge *more* honestly than they did by worst case, exactly as H0's finding 1 warned; no new refusal is possible anyway, four orders of magnitude under the cap. **Still open: H3** (M0 registration of the four `ContactConstructionLimits` numbers, the six `brace_penetration_limits` fields and `raft_limits`, caps §18, `constants.md` by its producer, the FU-24 ledger gate), **H4** (the rider above), **H5** (the equivalence harness and its mutation triad) and **H6** (the treant run under the memory protocol, which is where this row's stopping point is finally re-measured). The treant has **no seal**.]** **[H3 REGISTERED — 2026-08-09 (**[`#fu8g-h3-2026-08-09`](#fu8g-h3-2026-08-09)**).** All 14 numbers this row's own caps were unregistered under — the four `ContactConstructionLimits` fields, brace's six `PenetrationBatchLimits` fields, raft's four — are now `Placeholder` in the M0 registry, classification `runtime_resource_cap`, with honest provenance (single-specimen markers per R4/FU-23 where H0/H1/H2 measured a demand; a plain statement of absence for `SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS` and all ten brace/raft fields, which no run has ever exercised). **NO CAP RAISED, LOWERED OR TOUCHED** — registration only. `constants.md` regenerated by its producer; the FU-24 ledger gate (`constants_ledger_consistency`) is green. `CandidatePayloadConfig` remains genuinely out of scope, per R5. **Still open: H4, H5, H6** — the treant still has **no seal**.]** **[H4 LANDED — 2026-08-09 (**[`#fu8g-h4-2026-08-09`](#fu8g-h4-2026-08-09)**).** The owner-adjudicated rider: `RetainedLoadRebuildCause::Poisoned` now carries a `PoisonCause` (`Guard3Generic` | `CommitShape` | `IdentityWalk`), and `census::MergeTournamentCensus` adds `rounds_committed`/`tournaments`/`contacts_first`/`contacts_last`/`exit_reason` (`"no_proposals"` | `"no_safe_proposal"` | `"iteration_cap"`) beside the existing `specialized_gate_operations` census row. **Neither is hashed into any digest** — determined from source (both live only inside the feature-and-runtime-gated `census.rs`, printed as additive lines) and proven by a fresh corpus census run: all seven frozen digests reproduced byte-identical with the new counters live, and `make_raking_blade`'s own numbers (`contacts_first=37`, `contacts_last=34`, `rounds_committed=3`) satisfy D7's committed-round invariant on real geometry. **NO CAP RAISED, LOWERED OR TOUCHED.** Mutation-proven: six assertions across three existing D6 fixtures now name the exact `PoisonCause` each poison site must produce, and two new isolated `census.rs` unit tests pin `MergeTournamentCensus`'s accumulation. Gates 1048/1050 (unchanged default, +2 all-features — the two new census tests), 0 failed. **Still open: H5, H6** — the treant still has **no seal**.]** **[H5 PROVEN — 2026-08-09 (**[`#fu8g-h5-2026-08-09`](#fu8g-h5-2026-08-09)**): the separated equivalence proof, one new test file (`crates/plinth-support/tests/penetration_census_equivalence.rs`) and no engine code. Census-budgeted execution is `to_bits`-identical to worst-case-budgeted execution on `make_raking_blade`, `make_overhang_table` (a genuine two-interval hole row) and a synthetic 120-crossings-on-one-ray dense stack; every row's counted `crossings`/`tests` equal its executed `hits`/`work`, row by row; over-demand refuses as the typed preflight at the exact counted number, resource by resource, and exactly the counted demand is admitted. Mutation triad applied, observed, reverted — under-count-by-one, the F10 bound without its `3 x` term, and row-0 budget broadcast — with transcripts in the test file's doc comments; the broadcast mutation is caught by NOTHING else in the workspace (`native_api` 32/32 green under it), so the harness is load-bearing, not decorative. **NO CAP RAISED, LOWERED OR TOUCHED.** Gates 1052/1054, 0 failed; all seven OD-8 digests byte-identical. **Still open: H6 only** — the treant still has **no seal**.]** **[H6 MEASURED — 2026-08-09 (**[`#fu8g-h6-2026-08-09`](#fu8g-h6-2026-08-09)**): the treant run under the standing memory protocol, no engine code, `git diff` over `crates/` and `rust/` empty. THIS ROW'S OWN WALL IS GONE.** `initial::build_initial_contacts` **ran to completion on the Weeping Treant at 429,120 faces** — the first time any treant run has passed it — sampled 100 % of the main thread at t = 200 s and complete by t = 267 s, with `CensusDrift` never firing and no downstream consumer of the census refusing. The `contacts × faces` bound this row was opened against is not merely smaller but **sufficient**. **D5.3's cap-scaling contingency is NOT triggered and NOT added**; **NO CAP RAISED, LOWERED OR TOUCHED.** Both attempts then ended **EXCEEDS-TIMEBOX at 600 s** (600 s / 601 s wall, SIGKILL by the watchdog), peak RSS **3,693 / 3,721 MiB — 4.0 % of the 90 GB arm**, which confirms H1's per-row reserve collapse at treant scale against a design prediction of 14–32 GB. **The successor stopping point is not a cap and no cap would fix it:** from t ≈ 267 s to the kill the main thread is 100 % inside `route_initial_cco`, where `CcoController::digest` re-walks the whole accumulated transcript once per terminal — its share of the stage **rises** 19.7 → 64.4 → 77.2 → 77.2 → 77.1 % while the constant term's **falls** 31.0 → 13.8 → 9.0 → 9.0 → 9.0 %. Opened as **FU-32**. **Second finding — about the apparatus, not the engine:** the census flushes only on completion or typed refusal and there is no signal handler, so a watchdog `SIGKILL` emits **zero `CENSUS` rows**; the design's "EXCEEDS-**with counters**" outcome is unachievable with this harness, **H4's rider could not fire**, and the **21 poison rounds remain unattributed**. Gates 1052/1054, 0 failed; all seven OD-8 digests byte-identical. **H0–H6 complete. The treant has NO SEAL**, and what stops it next is named, derived to the line, and owned by FU-32.]** |
| FU-9 | Hollow EDT margin defect | `grid_spec_from_bounds`'s no-padding path plus its all-foreground special case fabricate a distance ramp; the committed Sphere hollow goldens are wrong today. Est 8/16 pts; regenerates four `plinth-hollow` oracles. |
| FU-10 | Per-layer polygon union under a fill rule | Owner-endorsed; supersedes the Tier-1 fallback. Candidate nomination must consume normalized layers per OD-9. **LANDED 2026-08-08** — phases A–C shipped, the census gate moved and stopped-and-reported per design, and the owner re-pinned all four moved digests ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). Closed. |
| FU-11 | Opt-in SDF/remesh repair mode | Roadmap item. |
| FU-12 | ~~Census doc erratum~~ | **RESOLVED 2026-08-05** (`893dac3`). The erratum is written at the top of `census-embed-incidence-2026-08-04.md`: the Reachability table is marked superseded, the `make_box` rows are struck as invalid (borrowed -30deg pose), the deleted `embed_inset_mm`/cosine-floor formula is called out, and the replacement numbers are appended as *Unified census — 2026-08-05*. |
| FU-12b | ~~`PLINTH_CENSUS_SUBSTITUTE_ORIENT` is dead code~~ | **RESOLVED 2026-08-05** (`6fe750b`). The env var and its `SUBSTITUTE_ORIENT` donor table are deleted from the census example. `make_box` and `Cylinder` now orient natively from their repaired baselines, so no donor-substitution path is reachable; the `MODEL` row still prints `orientation=own_pinned` so the transcript states on its face that no pose was borrowed. |
| FU-13 | `DuplicatePhysicalSeat` error payload should carry position+routes instead of post-sort indices | Fold into any future `SpecializedError` change. |
| FU-14 | `coalesced_seat_count` is deliberately NOT in the plan digest | Kept fixtures byte-identical. Fold it in at the next authorized digest-moving change. |
| FU-15 | Corpus-gated end-to-end test tier, with loud skip for gitignored STLs | So the Cylinder and `make_box` sealed runs (digests `13310185350867878190` / `9657051231007502658`), currently test-uncovered, get pinned. |
| FU-16 | Measure the bbox-claimed vs. discarded unowned-hole split on the treant once caps lift | bbox claiming can under-report a solid's area; the Tier 2 union supersedes it. **MOOT, closed 2026-08-08 — no phase-D treant measurement was ever taken, and none is owed.** Phase C's own record states it plainly: *"FU-16's closure measurement ('bbox claiming can under-report a solid's area; the Tier 2 union supersedes it') loses its subject"* ([`slicer-union-design-2026-08-07.md`](slicer-union-design-2026-08-07.md) §8.1). Phase C deleted `hole_owner`'s whole widening chain — vertex probe → interior probe → any-vertex → bounding-box containment — and the silent unowned-hole drop with it; on normalized output a hole's owner is unique by construction, so there is no bbox-claim query left to measure and nothing it could be shown to under- or over-claim against. The design's phase-D row ("census + goldens gate and treant measurement... FU-16 measurement on the treant's reachable layers") did not execute as a separate measurement pass because the census gate moved at phase C itself (§8.5) and went straight to owner ruling; that ruling supersedes phase D's exit criterion for this item. |
| FU-17 | Contact-area deficits: 12 stub + 554 CCO model-anchor | RECORDED as data by the 2026-08-05 unified census, not adjudicated. These contacts attach over measurably less interface than their tip radius claims. This is the print-quality question OQ-8 asks the owner; the 554 CCO figure is moot unless PR-7 revives that family. |
| FU-18 | ~~4 `rustfmt` violations inherited from the embed tip~~ | **RESOLVED 2026-08-05** (`cf2c2ab`). `cargo fmt --all` touched exactly `cco.rs:1425`, `cco.rs:2485`, `census.rs:852`, `designed_embed.rs:788` and nothing else. `cargo fmt --all -- --check` is now part of every gate run — see process rule 1 in *Post-merge interaction delta*. |
| FU-19 | ~~Re-pin the two stale `support_runner_integration` goldens~~ | **RESOLVED 2026-08-05** (`f77dfe1`). Re-pinned to `make_toed_slab` 64/179/`4826202191472851835` and `make_raking_blade` 32/89/`1709218010932846012`, with the whole delta attributed to PR-8 `NotAttached` on four named contacts and a control run proving nothing else moved. See *Post-merge interaction delta*. FU-7 was NOT settled first, deliberately: it is a separate, currently unscheduled investigation, and the tuple is re-pinnable independently — but `make_toed_slab` **will** need re-pinning again when FU-7 lands. |
| FU-20 | ~~Delete the retired `codex/embed-zones` branch + worktree and the `census/embed-incidence-2026-08-04` ref~~ | **RESOLVED 2026-08-05.** Both were proven contained in integration by `git merge-base --is-ancestor` before deletion; see *Post-merge interaction delta* and the worktree table. |
| FU-21 | Measure the **coverage-route kNN batch** | Opened by owner ruling R1 (2026-08-05). `SpecializedLimits::coverage_route_knn.max_work` now sits at the flat `SUPPORT_KNN_MAX_WORK_FLOOR` (100,000,000) precisely because **nothing has measured it** — the 696 work-units/face figure is the minima-to-face batch's, and borrowing it was the defect R1 removed. The treant's instrumented coverage-route demand is 15,282,619, but that is one point from a model that never reaches the stage in production. Measure the batch properly (it needs the FU-8b gate fix first, so the treant can get there), then either scale it on its **own** coefficient or ratify the floor on evidence. Until then the floor stands and `the_coverage_route_knn_cap_is_the_flat_floor_at_every_face_count` pins it. |
| FU-22 | Classify the **nested support authorities** R5 left out of scope | `CandidatePayloadConfig`, `ContactConstructionLimits`, `PenetrationBatchLimits` and `RaftLimits` — ~19 further values reachable from `SupportPreparationLimits::production` — still have **no registry entry**. They were excluded from R5 deliberately, not by oversight: `CandidatePayloadConfig` in particular mixes **result-changing refinement policy** with execution caps, so a blanket `runtime_resource_cap` sweep would misclassify them. This needs a per-field pass that decides, for each value, whether it can alter an admitted plan. See `03-physics-and-constants.md` §3.10.9. |
| FU-23 | Re-derive the two scaled coefficients when a **second large model** arrives | Standing trigger from owner ruling R4 (2026-08-05). `SUPPORT_KNN_MAX_WORK_PER_FACE` (2048) and `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` (512) rest on **n = 1**, the Weeping Treant v1.2 alone; the next largest corpus model is 90× smaller and sits under both floors. When a second model above ~4,760 faces is measured, re-run `next_power_of_two(2 × observed)` over **both** specimens rather than declaring the new model an outlier. The refs-per-face input is the one measured to vary 526 ↔ 2.83, so it is the likelier of the two to move. |
| FU-24 | ~~`m0-constants-source-ledger.json` is hand-maintained and had drifted from the registry~~ | **RESOLVED 2026-08-07.** The ledger's `registry_entries` array is prose provenance for the live `M0PlaceholderConstants`/`M0EvidenceConstants` registry (264 published ids as of this row); nothing mechanically tied the two together, so a new registry constant could land without its ledger entry and nobody would notice. Diffing ledger `id`s against the registry's own published id set found exactly two drifted: `INITIAL_AXIS_MAJORITY_CAP` and `INITIAL_AXIS_MAJORITY_STEP_MM`, added to the registry 2026-08-02 (commit `3d79a07`, "Add initial contact stub quarantine leaf") with no matching ledger entry ever added. Both are now entered with honest provenance — `Placeholder`, classification `runtime_search_cap`/`runtime_search`, a `normative` field naming the `04-migration-plan.md` §O-10 **regime** (uncalibrated legacy compatibility, not safety evidence) rather than any doc-fixed number, since §O-10 states verbatim that "No numeric point estimate is normative here" (neither value is normative in any document; each is normative only where it is declared, in `placeholders.rs`, and there is no §3.x entry in `03-physics-and-constants.md`), and source locators pointing at the Rust declaration and consumer sites rather than a nonexistent Python legacy source, since this pair was a native-Rust O-10 declaration from the start. The drift was silent because nothing checked it: `crates/plinth-core/tests/constants_ledger_consistency.rs` now parses the ledger's `registry_entries` and asserts its id set equals the registry's published id set in both directions, cheaply (one JSON parse against the one registry construction every other constants test already pays for). Verified to fail on reintroduced drift before landing. `constants.md` was unaffected throughout — it renders from the registry alone, never from the ledger, so it already carried both ids; only the ledger and this row were behind. |
| FU-25 | Decimation — topology diagnosis, then a real downstream-utility witness | **DEMOTED 2026-08-06 by owner ruling R6**, from "runs first" to a diagnosis + witness follow-up, because measurement falsified the compounding-speedup premise. The blocker is `topology_not_increased`, not `assess_downstream_utility`: `nonmanifold_edges` goes 0 → non-zero at every attempted treant ratio and `geometrically_eligible_candidate_count` is **0**, so minting a witness today moves the treant by zero faces. The ratified geometric budget admits only ~6–9 % reduction anyway (p99 0.00643 mm at r = 0.9406), which would still leave the gate charge ~7× over the flat floor. Sequenced as: diagnose the 3 non-manifold edges of the 425,934-face candidate → register constants and mint the witness (zero corpus delta) → make the reduction reachable and re-pin what it moves. Note the two live constraints: `crates/plinth-orient/tests/orientation_rederivation.rs` hard-asserts **no** reduction for the treant against its tracked 21,456,084-byte artifact, and `Sphere` + `make_toed_slab` would decimate the instant a witness exists, moving frozen digests `17667468971446251870` and `11199296506741562956` — hence the size threshold is proven necessary, not merely prudent. Full design, evidence and open questions Q1–Q8: [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md). **[Phase-1 diagnosis measured 2026-08-07 on `feature/decimation-followup`; the row above is left standing as written. Phase 1's exit criterion is answered NO: the treant cannot be usefully decimated, and Phase 3 is recommended AGAINST.** No engine code changed; all seven frozen OD-8 digests byte-identical. **(a)** `Prune`/`LockBorder` are **inert** — identical output faces, topology and p99/p99.9/max at three targets — so R-1 option (b) is closed: `LockBorder` has nothing to lock (boundary edges are 0 before and after) and `Prune` removes whole disconnected components while this damage is interior to one. **(b)** The mechanism is **coincident-sheet fusion**, not a lock artifact and not an f32 collision: every offending edge has exactly four incident faces whose normals are near-parallel (|cos| 0.919–1.000000), 24 of 25 have neither endpoint locked, `max_input_quantization_delta` is exactly 0 with 214,562 welded vertices at 214,562 distinct f32 positions, and 12 of 25 sit in one 2.9 mm twig column at (−11.4…−9.4, ≈−6.3, 88.8…91.7) — the locality of the existing `captured_treant_locked_fan` fixture. The original is clean: 0 boundary, 0 non-manifold, **0 duplicate faces**. **(c)** The new physics is `max_mm`, and it is a **staircase**: 0.0566 mm at 520 faces removed (0.12 % reduction) while p99.9 is 0.000073 mm — 776× — so the ratified budget is blind to the worst point. Every candidate U3 constant caps the treant at nothing: 0.080 mm (`PRINT_FLOOR_CHORD_MM`) admits (2.125 %, 4.453 %], 0.050 mm (`LAYER_HEIGHT_DEFAULT_MM`) admits ≈0.12 %, 0.007 mm admits ≈0.016 %. The topology gate independently stops it between 720 and 920 faces removed (0.17 %–0.21 %). **Q2's framing is superseded** — the curve does not discriminate between the two candidate constants, so the choice must be made on the physics argument alone. **Phase 2 stays worth doing** (it turns a blanket refusal into a real predicate) but will refuse *this* model on U3 regardless of topology; the witness's value is on the next large model. A ≤2 % face cut also cannot move FU-8e's wall-clock frames, which need an order of magnitude. Evidence, tables and the go/no-go: [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md) §8.]** **[Phase-2 LANDED 2026-08-07 on `feature/decimation-followup`; everything above is left standing and remains true. The witness is registered and mintable: `assess_downstream_utility` is no longer an unconditional refusal but the predicate the design record specified, minting `DownstreamUseful { witness_id: "support-face-scaled-resource-demand" }` only when all of a face threshold, U1 genuine reduction, U2 p99/p99.9 against **`Derived`** bounds with no ×1.1 slack, U3 worst-case `max_mm`, U4 `topology_not_increased`, and U5's two exactly face-scaled registered demands hold. Four constants registered with full discipline (registry 264 → **268**, classes `[20, 8, 236, 0]` → **`[20, 11, 237, 0]`**, `constants.md` regenerated by the producer example, ledger edits additive): `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM` 0.08 mm (`Derived` from `PRINT_FLOOR_CHORD_MM` — **Q2 answered**, on provenance rather than on the non-discriminating Phase-1 curve), `DECIMATION_MAX_P999_DEVIATION_MM` 0.019 mm and `DECIMATION_MAX_P99_DEVIATION_MM` 0.0095 mm (`Derived` from `PIXEL_PITCH_Y_UM`, pinned bit-for-bit against the `Placeholder` search products), and `DECIMATION_UTILITY_MIN_INPUT_FACES` 32,768 (`Placeholder`, n = 1 R4 trigger — **Q3 answered**). **Zero corpus delta, and structurally so:** the threshold is the witness's first clause rather than a short-circuit in front of the search, so every below-threshold trace is byte-identical, and all seven frozen OD-8 digests were re-measured unmoved alongside `faces`, `routed_contacts` and `final_live_members`. The treant is unchanged for the reason Phase 1 gave — the topology gate refuses every candidate before the witness is consulted. **One measured behavioural delta:** the 65,024-face `thread_sweep` UV sphere now reduces to 18,694 faces (−71.25 %; p99 0.009404 mm, p99.9 0.012589 mm, max 0.018350 mm against 0.0095 / 0.019 / 0.08), which turns `protected_budget_search_is_bit_identical_at_1_4_and_16_threads` into a determinism test of the **reduced** path at no frozen-digest cost — exactly the coverage the 32,768 choice was made to buy. Five gates green, 979 tests. **Phase 3 remains recommended AGAINST**; Phase 2 re-measured nothing in the §8.7 go/no-go. Caveat carried forward: U4 reads the counter FU-26 says under-counts, so its topology clause is a floor on the damage until FU-26 lands. Full landing record: [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md) §9.]** **[Verifier findings CLOSED 2026-08-07 on the same branch; nothing above is retracted.** Six fixes, no change to what the engine decides. **(1)** The witness id `support-face-scaled-resource-demand` appeared in **zero** registry artifacts, so §2's bidirectional-registration requirement was unmet; both `SUPPORT_KNN_MAX_WORK_PER_FACE` and `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` now carry a dated *Witness registration* clause naming it, `constants.md` regenerated by the producer, registry unmoved at 268 / `[20, 11, 237, 0]`. **(2)** §6 Phase 1's serialization deliverable is executed: report schema **v4** with `max_mm` on every probe, the registry-read U3 bound, and a real `native_surface_builds` counter (20 on the treant, 21 per run with `run`'s feature-protection map); per-phase wall-clock is deliberately **not** serialized under the contention caveat, stated as a rule honored rather than a deliverable dropped (§9.8). **(3)** FU-26's headline is re-derived from that instrument — 390,898 faces, not 390,896, topology conclusion unchanged (§8.9, and the FU-26 row below). **(4)** U5 was one comparison written twice (`out < in` **and** `81·out < 81·in`); the two registered demands are proportional by construction, so the clause is stated once and the proportionality is documented and machine-checked instead of restated. **(5)** U4 is documented as a defence-in-depth clause **unreachable** via `run_budget_search`'s current ordering — the search gates topology and `continue`s before the witness is consulted — kept because the tests pin it and a future reordering would otherwise lose the guard silently. **(6)** §9.1's stated clause order is corrected (the `max_mm` presence-and-finiteness guard runs first) and `DownstreamUseful::witness`'s doc comment now says what crate-privacy actually withholds: **minting**, not naming — the id is exported and callers may compare against it.]** |
| FU-26 | `nonmanifold_edges` under-counts, and duplicate faces are counted nowhere | **Opened 2026-08-07 by the FU-25 Phase-1 diagnosis** (`decimation-design-2026-08-06.md` §8.6), deliberately **not** fixed there because Phase 1 must not move behaviour. Two parts. **(1)** `build_face_topology` reads `cut_flags` for its duplicate-face exclusion *inside the same loop that writes it* (`crates/plinth-mesh/src/topology.rs:339-341` reads, `:371-373` writes), so a face that shares an edge with a genuine duplicate face acquires `TOPOLOGY_CUT_DUPLICATE` part-way through and any **later** edge key touching that face is dropped from the count. Deterministic (`BTreeMap` key order) but order-dependent, and it under-counts: on decimated treant candidates 24 against a frozen-snapshot 25 at r = 0.911, and 170 against 208 at r = 0.50. **(2)** Larger: duplicate faces are excluded from `nonmanifold_edges` by design and counted **nowhere else**, so `topology_not_increased` (`crates/plinth-decimate/src/lib.rs:1843`) cannot see them at all — 664 duplicate-face groups at r = 0.50 pass that gate unremarked. Together these make the gate's number a **floor** on the damage, and the gap widens with reduction. This is a *counting* defect, not a licence to relax the gate: `stitch_layer` genuinely requires degree-2 adjacency. Fixing (1) will move the reported count on any decimated mesh, so it needs the usual re-pin discipline — no corpus digest depends on it today, because no corpus model decimates. Note: the number `FU-26` was assigned on `feature/decimation-followup` while `FU-24` was in flight elsewhere; renumber at merge if it collides — the finding, not the number, is the record. **[Headline evidence RE-DERIVED 2026-08-07 from a committed instrument; everything above stands except one face count.** The corpus gate's report schema is bumped to `plinth/m3-decimation-corpus-budget-report/v4` and now serializes `max_mm` on every probe, so this finding no longer rests on scratch instrumentation that was reverted. Re-run at the `feature/decimation-followup` tip, the r ≈ 0.911 treant candidate is **390,898 faces, not 390,896** — the search's bisection asks meshoptimizer for 390,901 faces (`429,120 × 0.91093749999999996`) where the reverted scratch asked for 390,898, and the two undershoot by 3 and 2 faces respectively. §0(b)'s original live report already said 390,898; the scratch was the odd one out. **The topology conclusion is unchanged:** `build_face_topology` reports **24** non-manifold edges and 0 boundary edges at that candidate, exactly as recorded, and the disposition is still `TopologyIncreased` before the witness is consulted. Attribution, for whoever lands the fix: the **24** and the **170** are engine output and are re-derivable from the v4 report by anyone with a clean checkout; the **25**, the **208** and every duplicate-face-group count (109 at r = 0.911, 664 at r = 0.50) are scratch measurements of 2026-08-07 and are *not* re-derivable, because no committed instrument counts them — which is this finding. Land the counter first, then the comparison becomes reproducible rather than remembered. Correction and re-derivation: [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md) §8.9.]** |
| FU-27 | An **unidentified intermittent failure** in `plinth-support`'s lib tests under machine load | **Opened 2026-08-07 by FU-8f phase G4.** Two separate `cargo test --workspace --offline --no-fail-fast` passes exited non-zero with `error: 1 target failed:` `-p plinth-support --lib`, both while the machine was still hot from G4's two 600 s treant runs. **Neither run's `failures:` block was retained, so the failing test is UNKNOWN**, and G4 deliberately does not guess. Four consecutive full-workspace runs afterwards are clean at **989 / 0**, the `--all-features` gate has never failed, and `specialized::tests::full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads` — the 1/4/16-thread determinism test the FU-8f G2 section already recorded flaking under load, and the only known load-sensitive test in that binary — passes in isolation. **It is the obvious suspect and is NOT identified as the cause.** Two sightings in one session is past the point where "flake, re-ran, green" is an adequate record: a determinism test that only fails under load is either a real ordering dependence or a real resource sensitivity, and both matter. **First action is procedural, not diagnostic** — every gate run should tee to a file so the `failures:` block survives the next sighting; only then is there evidence to name the test. Until then **no claim is made that the two G4 sightings and the G2 sighting are the same defect.** See *FU-8f phase G4 measured 2026-08-07* §G4.8. **[Datum 2026-08-07 (FU-8f G3–G5 close-out verification):** five-gate workspace test runs came back clean **twice on a quiet machine AND twice while a concurrent `cargo test --workspace` ran in another worktree** — a non-reproduction under load, recorded as a datum, **not** as evidence the flake is gone.**]** **[CAPTURED 2026-08-08 (`feature/slicer-union` close-out) — the `failures:` block this row asked for, plus a `develop` control. The procedural first action is discharged and the flake is caught with its assertion intact rather than inferred.** **The failing test is `specialized::tests::full_specialized_evidence_is_identical_at_one_four_and_sixteen_threads`** — the obvious suspect this row named. Parent panic at `crates/plinth-support/src/specialized.rs:13692` (*"specialized 4-thread child failed"*); child panic at `specialized.rs:13654`, the guard `assert!(effective > 1 && participating > 1)`: **`required fixture corpus must exercise more than one worker at 4 threads`** (line numbers are `develop` `0f24b79`'s). **It reproduces ON `develop` `0f24b79` ITSELF**, in a detached scratch control worktree: **1 failure in 8 iterations** under 48 busy-loop CPU burners plus a concurrent `cargo build -p plinth-support` churn on a 16-core machine, where the same binary on the same tree passes **solo in 0.54 s**. The flake is therefore **pre-existing and environmental — NOT introduced by `feature/slicer-union`**, which does not touch this test at all (the branch's only `specialized.rs` change is the seat-coalescing gate's re-pinned expectations). **Load-sensitivity mechanism, stated as the hypothesis the capture is consistent with, not as proof:** the child asserts on `generic_slice_thread_evidence`'s *participating* worker count, and under CPU starvation an N-thread `rayon` pool can effectively serialize — the spawning thread drains every work item before any other worker steals one — so `participating` returns 1 and the guard fires. **The capture refines this row's assumed shape in one respect: the failure landed on the 4-thread child, not the 16-thread one**, so starvation does not need a 16-wide pool to produce it. **This capture is the LIKELY IDENTITY of FU-27 — hypothesis, not proof.** It is the same binary (`-p plinth-support --lib`), the same load-only failure, and the same cleanliness when quiet; but the two G4 sightings' `failures:` blocks remain lost, so they **cannot** be matched to this one directly, and the claim that they are the same defect is still **not** made. **At the `feature/slicer-union` tip (`d219d7b`), gate 2 (`--all-features`) ran TWICE on a quiet machine and was fully green both times — 1041 / 0 / 0, exit 0, no `failures:` block** — and the `develop` control's own gate 2 is **1012 / 0**, both consistent with the G3–G5 datum above. **No code change is made and none is proposed here**; the remedy (make the guard tolerate a starved pool, or pin the pool size the evidence is measured against) is a follow-up. **This row stays OPEN.**]** |
| FU-28 | The **drilled hollow is producible but not supportable**: `run_support` refuses `plinth_hollow::run`'s own output | **Opened 2026-08-07 by export phase E2** (`export-path-design-2026-08-07.md`, *E2 LANDED* section), which was the first code anywhere to run the real hollow stage and the sealed support run back to back on a corpus model. The export design's OQ-1 asked whether `plinth_hollow::run` *completes* on corpus models and planned a typed solid-decision fallback for a refusal. It completes: the corpus Sphere hollows, plans drains and drills, returning `AwaitingInternalSupport`. **The refusal lands one stage later** — `run_support` rejects that 14,526-face drilled shell during `Preparation` (against `max_grid_bytes=67108864`, `knn_max_work=100000000`, so this is not obviously a resource-cap stop; the phase is `Preparation`, and the underlying refusal code was not extracted because doing so needs the `census` feature and E2 must not move engine behaviour). The oriented solid is 4,760 faces and seals fine — digest `17667468971446251870`, unmoved. **Consequence today:** every artifact the export path can emit for the Sphere is the *solid*, so hollowing buys the corpus nothing downstream, and the whole 7-of-8 sealed-run evidence base remains what F14 already said it was — `solid_decision` handoffs. **Consequence for E4:** `prepare()` cannot simply chain `hollow_stage` into `run_support` and expect the hollow to survive; it needs this diagnosed first or an explicit, recorded policy for which geometry ships. **First action is diagnostic, not a fix:** re-run the drilled shell under `--features census` and read `stage_errors` to name the actual `Preparation` refusal, then decide whether it is a mesh-property refusal (the drilled shell has an interior surface the oriented solid does not) or a resource stop. Pinned by `crates/plinth-pipeline/tests/first_artifact.rs`, which asserts the `DrilledShellRefusedBySupport` variant by equality, so the day this changes the gate fails and says so. Note also that **A5 internal-support planning does not exist anywhere in this workspace**, so even a supportable drilled shell would ship today with no internal supports. |
| FU-29 | The **OQ-3 adequacy stitch site has zero corpus coverage**, and its `max_loop_vertices` cap is an unexercised refusal path | **Opened 2026-08-08 as ruling condition (2) of the FU-10 phase-C re-pin** ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). §8.5 measurement (3) said the adequacy-probe path "moves zero rows on all seven models"; instrumentation at both stitch sites shows it **ran** zero rows: 2,956 print-layer rows against **0** adequacy rows, because `find_local_minima` always reaches `normalized_slice_layers(SliceStitchSpace::AdequacyProbe, …)` with an empty `probe_heights` — every local minimum matched a birth, so `suspects` is empty and the batch carries zero planes. "Moves zero rows" and "ran zero rows" are different claims and the record did not distinguish them. Consequence: the adequacy site's normalization has **no corpus-level evidence at all** — unit tests only — and the `max_loop_vertices` cap that phase-C deviation 2 adds there (a site that previously had no loop-vertex cap) is a **new refusal path that has never executed on real data**. This is a coverage gap, not a known defect. **First action:** construct a model, or a synthetic layer set, whose local minima do *not* all match births, so `probe_heights` is non-empty and the site executes on real geometry; then decide whether the cap belongs there at the value it has. |
| FU-30 | Corpus coverage of `coalesce_exact_regular_seats` **halved** at the FU-10 phase-C cutover | **Opened 2026-08-08 by the re-pin** ([`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08)). The specialized golden asserted `coalesced_seat_count == 1` for both corpus models; measured at the re-pinned tip it is `toed slab` **0** and `raking blade` **1**. With the corrected slice geometry `make_toed_slab` no longer produces a co-located regular seat pair at all, so the terminal coalescing pass is a measured no-op there and the count is now pinned per model. The pass is still exercised — by `make_raking_blade` and by `specialized`'s unit tests — but by one corpus model instead of two, and the `DuplicatePhysicalSeat` failure mode it was built to prevent now has a single corpus witness. The disappearance is *consistent with* the four phantom island births removed from `make_toed_slab` (print layers 49, 52, 226, 228) but that causal link was **not** independently proven; only the measurement is on the record. **First action:** decide whether one corpus witness plus unit tests is adequate, or whether a fixture should pin the duplicate-seat path deliberately rather than incidentally. |
| FU-31 | The Weeping Treant now fails `Specialized(DuplicatePhysicalSeat)` before ever reaching the initial-contact preflight — FU-30's "one corpus witness" question, answered NO on the treant | **Opened 2026-08-08 by FU-8g phase H0** ([`#fu8g-h0-2026-08-08`](#fu8g-h0-2026-08-08)), directly answering [FU-30](#follow-ups-2026-08-04)'s own "First action". At `develop @ e3168c5`, three reproductions (`cargo run --release --offline --features census --example embed_incidence_census -p plinth-support`, `PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`) all fail identically with `Preparation: Specialized(DuplicatePhysicalSeat { left: 163, right: 164 })`, before `build_initial_contacts` is ever called — three stages before the `Initial(Limit { resource: "work items", ... })` wall *FU-8f phase G6d measured 2026-08-08* ([`#fu8f-g6d-2026-08-08`](#fu8f-g6d-2026-08-08)) recorded as the treant's stopping point. That measurement was taken at `fe79124` (the G6c tip), **before** the FU-10 phase-C polygon-union cutover merged into `develop`; FU-30's own re-pinned test comment shows that same cutover moving `make_toed_slab` and `make_raking_blade`'s contact/coalesced-seat counts, and states plainly that `coalesce_exact_regular_seats`'s corpus coverage fell to one model. The treant is the second witness FU-30 asked whether was needed, and on the treant the pass does not catch the collision: physical seats 163 and 164 land at the same position under the corrected slice geometry and nothing merges them. **Consequence:** FU-8g's own design (`fu8g-design-2026-08-08.md`) is premised on a treant stopping point that is not reachable at this tip, and its H1–H6 phases cannot exercise the treant until this is fixed. **Not FU-8g's defect to fix** — it sits entirely inside `Specialized` contact-plan construction, upstream of anything FU-8g's design touches. **First action:** diagnose why seats 163/164 collide under the FU-10 slice geometry and why `coalesce_exact_regular_seats` does not merge them (a third corpus witness, or a widening of that pass's merge condition, per FU-30's own open question) — then FU-8g's H0 can be re-run to get the treant's honest crossings/traversal-test distribution. See *FU-8g phase H0 measured 2026-08-08* §H0.2 for the full derivation and the three reproduction transcripts' peak-RSS/wall-clock evidence. **[DIAGNOSED 2026-08-08 (**[`#fu31-diagnosed-2026-08-08`](#fu31-diagnosed-2026-08-08)**), fix not landed.** Seats 163/164 are **one island birth** (print layer 1593, face 137983, a 0.001 mm² twig tip) fanned out to its **two owning islands** (111/113) as two bit-identical demands, both uncovered, both pushed as pure specialized seats — one value copied twice, not a G6b-style bit tie. `coalesce_exact_regular_seats` misses them by **class** (demand-free-regular-only), not tolerance; `attach_exact_mixed_contacts` needs a demand-free side; the gated tournament — the only machine admitting the pair — gathered it in 492 rounds and the gate refused every variant on one measured component: `load_violation_count` 3 vs baseline 2 (co-located seats split nearest-won load rows and each ducks the tip safety factor; the merged seat carries the honest union and trips it). The collision is **pre-existing**: the `fe79124` control run mints the same birth (centroid moved ~1e-10 mm by the union) in a 375-group population and merges them all — specialized passes there with 542 contacts, zero duplicates — so the cutover shifted the load landscape under exactly this pair rather than creating the phenomenon. Corpus provably never builds the configuration (zero identical-position pushed groups on all seven models; digests byte-identical with instrumentation live). **Recommended fix: (b) aggregate uncovered demands with identical nominated position bits into one seat at push time** — invariant holds by construction, corpus untouched structurally, treant unblocked toward the `initial::preflight` wall; coalesce-widening (a) just re-dies as `MergeGateRegression` on the same 3-vs-2, and "two seats are real" (c) is contradicted by the data model, the other 197 merged groups, and this very pair's own history.]** **[FU-31 FIXED 2026-08-08 (**[`#fu31-fixed-2026-08-08`](#fu31-fixed-2026-08-08)**).** Option (b) landed at `55b4d99`: `dispose_demands` aggregates same-position uncovered demands into one seat at push time, keyed on position bits normalized to `same_point`'s `==` equality; mutation-proven fixture reproduces the exact `DuplicatePhysicalSeat` with the aggregation disabled. Treant acceptance (attempt 1 of 2): specialized passes with **553** contacts (was 554 with the un-merged pair) and the run advances to `Preparation: Initial(Limit { resource: "work items", required: 237305572, limit: 4000000 })` — the FU-8g wall, now measured in-engine at 553 × (4 + 429,120). FU-8g's H0 treant leg captured in the same run: crossings mean **10.906** (p50 7, max 298), tests mean **191.982**, honest work Σcrossings **6,031** vs charged 237,303,360 (**39,347×**), honest scratch **142 MiB** vs charged 603 GiB. All seven frozen corpus digests byte-identical; one unit-fixture re-pin (raking blade, raw orientation, four real two-owner births now born aggregated), attributed in full in the section. FU-8g H1–H6 are unblocked.]** |
| FU-32 | CCO routing re-digests the whole accumulated transcript once per terminal — the batch-first violation family a sixth time, and the first one no cap could have caught | **Opened 2026-08-09 by FU-8g phase H6** ([`#fu8g-h6-2026-08-09`](#fu8g-h6-2026-08-09)), the run that first carried the Weeping Treant past `initial::build_initial_contacts`. With the initial-contact wall gone (H1/H2's honest charge: **108,378 vs the untouched 4,000,000**, 2.71 %), the treant's stopping point is no longer a refusal at all: both attempts ran **EXCEEDS-TIMEBOX at 600 s** with the main thread **100 % inside `PreparedSupportRun::route_initial_cco`** from t ≈ 267 s to the kill. **Measured, not argued:** across sample windows at t = 280 / 360 / 440 / 520 / 575 s the share of routing wall-clock in `CcoController::digest` **rises** 19.7 % → 64.4 % → 77.2 % → 77.2 % → 77.1 % while the constant-per-terminal `validate_replay` term's share **falls** 31.0 % → 13.8 % → 9.0 % → 9.0 % → 9.0 % — a growing term overtaking a constant one, reproduced across three windows 135 s apart. **Derived to the line from source, independently:** the routing loop (`runner.rs:1650–1666`) calls `grow_terminal_with_finalized_anchor` once per terminal; that function's **first statement** (`cco.rs:1397`) is `let controller_digest_before = self.digest();`; and `CcoController::digest` (`cco.rs:1775–1862`) is not the cached `const fn` at `:863` but a full walk of the accumulated controller state — every terminal in `terminal_cadence` (`:1786`), every record in `terminals` (`:1790`), each `UnroutedNoWinner` record's **entire** anchor-refusal list via `digest_anchor_seat_refusals` (`:1818`, `:1824`, `:1834`; five words per refusal at `:2128–2149`) and `digest_anchor_refusals`, then the whole host kd-tree (`indexed_members` `:1851`, `samples` `:1856`). Cost `Θ(N²) + Θ(N·R) + Θ(N·H)` for `N` cadence terminals (553 specialized physical contacts plus the regular terminals appended at `runner.rs:1649`), `R` accumulated refusals, `H` host samples. The `Θ(N·R)` term dominates on this model, consistent with the standing record that **0 of 3429 attempted model anchors survive corpus-wide**, which makes `UnroutedNoWinner` common and refusal lists long. **A second, independent per-terminal re-digest sits in the same loop** and is the ~9 % constant term: `validate_replay` → `validate_finalized_anchor_route` → `BoundPhysicalAnchorContact::new` → `SpecializedContactPlan::digest` → `GenericContactPlan::digest` → `ContactCandidatePool::digest`, plus `SurfaceAnchorPool::validate_against` → `surface_anchor_pool_digest` and `FinalizedContactLoads::validate_binding` → `finalized_load_rows_fingerprint` — **every one of those structures is immutable for the whole routing stage and every one is re-hashed from scratch per terminal.** Together ≈86 % of the stage. **This is NOT a cap and no cap would fix it**: nothing refuses, no resource is over-charged, no limit is consulted — which is why the measurement-first doctrine found it with a profiler rather than a preflight, and is the reason this row exists. **The remedy is not this row's to choose**, but the shape bounds it: both terms re-derive state whose deltas are already known at the call site, and the house precedent is incremental maintenance (FU-8f G3's prefix evaluation, G6's cross-round retention), not a bigger budget. **FIRST ACTION — and it is not the fix.** H6 also found that **an EXCEEDS-TIMEBOX run is structurally counter-free**: `plinth_support::census` flushes only from `census::finish` (model completed) or `census::report_failure` (typed refusal), there is **no signal handler anywhere** in `census.rs` or `examples/embed_incidence_census.rs`, and the watchdog kills with `SIGKILL` — so **both attempts emitted zero `CENSUS` rows**. The design's "EXCEEDS-**with counters**" outcome is therefore not achievable with the present harness on the very outcome it ranked most likely (~45 %), H4's rider (`rounds_committed`, `exit_reason`, the three-way `PoisonCause` split) **could not fire**, and the treant's **21 poison-driven rebuilds remain unattributed**. Give the census a periodic or signal-triggered flush *before* the next treant run; otherwise the next run buys another verdict with no evidence. **Memory is not the problem and should not be treated as one:** whole-process peak was **3,721 MiB**, 4.0 % of the 90 GB watchdog arm — H1's per-row reserve collapse confirmed at treant scale, against a design prediction of 14–32 GB under the old reserves. Routing's own growth (+1,591 MiB over 332 s, ≈4.8 MiB/s) is honestly rows-scaled transcript accumulation, and binds nothing. **The brace/joints/raft caps registered by H3 remain wholly unexercised** — the run never reached brace — so §18's statement of absence stands unchanged and this row licenses no brace number. **NO CAP RAISED, LOWERED OR TOUCHED**, and D5.3's cap-scaling contingency is not triggered and not added. See *FU-8g phase H6 measured 2026-08-09* and `support-resource-caps-2026-08-05.md` §19. The treant has **no seal**. **[J0–J3 LANDED; THE MOVEMENT IS ATTRIBUTED AND THE RULING IS PENDING — 2026-08-09 (**[`#fu32-j3-2026-08-09`](#fu32-j3-2026-08-09)**).** J0 (`41246f6`) gave `plinth_support::census` a periodic **and** signal-triggered flush, so the counter-free killed run this row opened against is fixed at its root. J1 (`059b444`) hoisted the immutable routing authority (`VerifiedRoutingAuthority`, the G2 precedent applied verbatim) — **value-preserving; all seven OD-8 digests byte-identical**. J2 (`c14754e`) replaced `CcoController::digest`'s per-terminal whole-state walk with the design's state-pure incremental accumulators, kept **both F6 tamper tripwires genuinely recomputing** through `rebuild_accumulators()` (`runner.rs:2775`, `:2848`), and **MOVED ALL SEVEN frozen OD-8 snapshot digests**. **The sentence above stating the treant has no seal was written before J2 and stands as the record of its date: the Weeping Treant SEALED for the first time at J2** — complete pipeline (routing, rewire, brace, seal) in 463 s wall-clock, peak RSS ≈5.4 GiB (6 % of the 90 GB arm), `routed_contacts=306`, `final_live_members=1383`, `snapshot_digest=423276951048565625` — **a first measurement, not a pin**, and §A2's predicted next binder (`rewire::optimize`, ~55 %) did **not** bind. J3 adds **no engine code** and is the FU-10 attribution: the digest movement is **DECISION-NEUTRAL**, proved twice over — from source (all 30 `controller_digest_before` occurrences audited at this tip; **zero comparisons outside `#[cfg(test)]`**, and `finalize_transcript_row`'s admission check reads six other fields and deliberately not this one) and by parent-vs-tip routing-output dumps on all seven corpus models (**275 transcript rows and 687,288 content lines byte-identical**; of the transcript's 10 header digests only the 3 that fold the moved value move, and of `verification_snapshot_digest`'s 17 inputs exactly 1 does). **Three code pins and one golden will need re-pinning at J4**, enumerated by name in the J3 section — and **NOTHING is re-pinned, no golden refreshed, no doc table rewritten and no test expectation relaxed** pending the owner's ruling. **NO CAP, floor, constant or registry entry RAISED, LOWERED OR TOUCHED.** Gates 1058 default / 1063 all-features with **exactly** the two expected digest-pin failures and no others; clippy, fmt and `git diff --check` clean. **Still open: J4 (owner ruling, re-pin, record sweep), J5 (the honest routing charge, §D3) and J6 (the treant measurement and the record).**]** **[J4 LANDED; THE OWNER RULED AND ALL SEVEN ARE RE-PINNED — 2026-08-09 (**[`#fu32-j4-2026-08-09`](#fu32-j4-2026-08-09)**).** Asked whether the value-changing digest fix may proceed given that all seven OD-8 fingerprints move, the owner ruled, verbatim: *"The fingerprints aren't sacred  if things change slightly that's to be expected (because the algorithm wasn't right when the fingerprints were generated!)"* — sought after J3's attribution and before any re-pin, which is FU-10 order. **The new OD-8 authority table is in the J4 section** and supersedes the 2026-08-08 one at [`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08): Sphere `2297157889062365577`, Cylinder `6853793710942395608`, make_box `8709076999423814412`, make_overhang_table `16396858890798947253`, make_toed_slab `133000971864625681`, make_raking_blade `18388631460456455184`, make_thin_fin `14656364680512457190` — **all seven digests move, NOT ONE count moves**, which is exactly what separates this movement from the 2026-08-08 one (that one moved two models' counts because real island births were removed). Three code pins re-pinned from J4's **own** re-measurement rather than hand-copied (`support_runner_integration.rs` toed_slab `13_346_779_153_602_682_749 -> 2_198_459_883_570_957_390` and raking_blade `8_660_653_339_934_460_766 -> 74_030_269_957_898_085`), and the workspace's one golden **regenerated through its own producer path, never hand-edited** (`coverage_gate_overhang_table.json` `snapshot_digest 5470664714994646368 -> 3706765241747593242`; diffed field by field afterwards — 185 leaves, exactly 1 moved, no evidence lost). `verification_snapshot_digest_input_list_is_pinned` was deliberately **not** touched and stayed **green**: the digest's input *shape* did not widen, only one input's value moved. Dated supersession lines were appended at every doc site quoting any of the seven superseded values (107 occurrences across 9 files); **no earlier citation was edited or deleted**. Census re-run at this tip reproduces the new septet byte for byte — the third concordant measurement — with `routed_contacts` and `final_live_members` unchanged on every model. Gates **1060 default / 1065 all-features, ALL GREEN**, zero failures; clippy, fmt and `git diff --check` clean; **J4 adds zero tests and moves zero counts. NO CAP, floor, constant or registry entry RAISED, LOWERED OR TOUCHED. Still open: J5 (the honest routing charge, §D3 — an addition, not a move) and J6 (the treant measurement and the record).**]** **[J5 LANDED; ROUTING IS CHARGED — 2026-08-09 (**[`#fu32-j5-2026-08-09`](#fu32-j5-2026-08-09)**).** The sealed CCO routing cadence, which had **no `Limit`, no counter and no clock**, now carries a counted preflight charge against **two NEW registered caps** — `SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS` and `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS`, both 536,870,912 by one stated rule — with a meter beside the charge and a receipt (`executed ≤ charged` or `ChargeDrift`). Registry 284 → **286**; caps-doc §21, physics-doc §3.10.11, FU-24 ledger rows and a producer-regenerated `constants.md` in the same commit. **Result-inert: OD-8 seven byte-identical, `routed_contacts`/`final_live_members` unmoved.** Gates **1066 / 1071 ALL GREEN** (+6). **NO EXISTING CAP RAISED, LOWERED OR TOUCHED** — the two are additions to a stage that had none. `rewire::optimize`'s absent budget opened as **FU-33**, deferred not skipped.]** **[J6 MEASURED — FU-32 IS COMPLETE, J0–J6, AND THE TREANT SEALS TWICE UNDER THE STANDING MEMORY PROTOCOL — 2026-08-09 (**[`#fu32-j6-2026-08-09`](#fu32-j6-2026-08-09)**).** This row's headline sentence — *"The treant has **no seal**"* — was written at H6 and is **superseded**, first by J2's seal (the first in project history, 463 s, unprotocolled) and now by the measured record. **Two attempts, both COMPLETED: 470.83 s and 470.64 s of the 600 s box, exit code 0, peak RSS 5,585.6 / 5,610.9 MiB = 6.06 % / 6.09 % of the 90 GB arm, which never fired.** Seal tuple identical in both runs and to J2's: `routed_contacts=306`, `final_live_members=1383`, `snapshot_digest=423276951048565625`; four self-proving identities hold in the transcript (CCO winners 199+4+103 = 306; surviving model anchors 4 = committed 4; `T5` members sum to 1,383; stubs 306). **Determinism, the G6d standard: 13,441 terminal census rows per run, SHA-256 identical, ZERO differing rows**, wall-clock within 0.04 %. **THE WALL THIS ROW WAS OPENED AGAINST IS GONE:** the cadence loop that consumed ≥335 s without finishing at `bfa5b3b` — and whose hashing §M4 projected at ≈691 s — completes in **≤20.7 s**, bracketed by J5's own preflight-charge and receipt census rows. J5's charge/receipt reproduce **to the unit** (164,993,019 evaluations and 162,719,910 retained rows, 30.7 % / 30.3 % of the untouched cap). **Predictions scored:** §A2's ~55 % call (rewire binds next) **MISSED** — rewire cost 41.5–46.5 s, 8.8–9.9 % of the run, with its evaluated candidates bounded by the census at 619 clearance rows and 3,076 anchor attempts against a 364,223-row pool; the per-move shape the design named was right, the number of moves was never estimated, and **FU-33 is not closed, downgraded or answered**. The ~10 % call (a clean seal) **LANDED, on cause as well as outcome**. §A2's ~25 % call (a brace/raft refusal) **MISSED**: **the brace and raft stages RAN for the first time** — 748 rung rows, 450 Brace members in the sealed forest — **and none of the ten registered brace/raft caps refused; but none of them is metered, so there is still NO demand of record for any of the ten.** H3's "no run has ever reached them" is superseded; its "no measured demand" stands. **Design §M3's 30.3 B per retained refusal is corrected DOWNWARD to ≤8.8–9.3 B/row at completion — and the cap does NOT move, in either direction.** **NO cap, floor, constant, coefficient or registry entry raised, lowered or touched; NO engine code changed** (`git diff` over `crates/` and `rust/` empty); **the treant is still NOT in the frozen digest set and `423276951048565625` is still NOT a pin.** What stands between this seal and the `.chitubox` checkpoint is the **emit path** — `examples/first_artifact.rs --all` still excludes the treant for a reason that is no longer true — and that is export work, not FU-32's. Per §A4 step 5, a first seal is an **owner's** decision point.]** |
| FU-33 | `rewire::optimize` is an **unbounded loop**: `REWIRE_WORK_BUDGET` is `TrialSetting::disabled()` and there is no wall-clock deadline anywhere in `plinth-support` for rewire | **Opened 2026-08-09 by FU-32 phase J5** ([`#fu32-j5-2026-08-09`](#fu32-j5-2026-08-09)), which charged routing and **deliberately did not charge this**. The shape is named from source in the design's §A2 #1 and ranked the **most likely next binder at ~55 %**: `rewire.rs`'s `'sweeps: loop` iterates `sweep_order` (one descriptor per live non-stub member, up to four kinds each) and **per move** does `RewireCcoBinding::bind` (a `forest.clone()` plus **two full `Forest::digest()` walks**), `derive_brace_attachments`, `total_volume_ml`, a whole-forest `validate()` per candidate, `changed_forest_volume_delta` per candidate, and `discover_rewire_anchor_seats` over **the entire 364,223-row anchor pool**. `REWIRE_WORK_BUDGET` is `Disabled` (`crates/plinth-core/src/constants/placeholders.rs`), so the loop has no budget at all and the only thing that has ever stopped it is an external `SIGKILL`. **Why J5 did not fix it, stated rather than assumed:** a work budget that stops a search early is **result-changing** — it changes which forest is returned and therefore every downstream digest — whereas J5's routing charge is result-inert by construction (proved: all seven OD-8 digests byte-identical at its tip). Landing both in one phase would put a value-moving ruling inside a resource-charging phase, which is the mixing FU-10 protocol exists to prevent. **What this row owes:** measure `rewire::optimize`'s real demand on the treant first (it has never been instrumented — the treant reached rewire for the first time at FU-32 J2), then decide between a charged preflight on a knowable worst case (J5's shape, result-inert) and an enabled `REWIRE_WORK_BUDGET` (result-changing, and therefore owner-ruled under FU-10 protocol before it is built). **Do NOT enable the budget without that measurement and that ruling.** No cap, floor or constant is touched by opening this row. |
| FU-34 | `plinth_hollow::run` **refuses the Weeping Treant's shell outright** — `InvalidShellTopology`, so the project's flagship model cannot be hollowed and gets no resin saving at all | **Opened 2026-08-09 by the treant `.chitubox` checkpoint** ([`#checkpoint-treant-artifact-2026-08-09`](#checkpoint-treant-artifact-2026-08-09)). **Number note: this row was adjudicated as "FU-33", which was already taken** by the `rewire::optimize` row directly above (opened 2026-08-09 by FU-32 J5); it is filed at the next free number instead and **no existing row was renumbered, edited or displaced**. **The measurement.** `target/release/examples/first_artifact "Weeping Treant v1.2"` at `063115f`, twice, unsandboxed with the 90 GB RSS watchdog armed: both attempts exit 1 at **22.29 s / 22.42 s** (peak RSS 1.69 / 1.72 GiB, watchdog never fired, 1200 s timebox never approached) with the byte-identical line `hollow: native shell did not contain one negative cavity enclosed by positive outer material`. The typed value is `plinth_hollow::HollowError::InvalidShellTopology` (`crates/plinth-hollow/src/lib.rs:1866`, category `Shell`), raised from the post-Boolean shell check — `orient_boolean_cavity` (defined `:995`, called `:863`) and `validate_shell_topology` (`:949`, called `:864`) — both of which require the shell to triage as `TriageError::PreHollowed` with **exactly one** cavity component and to certify ≥2 closed components with **exactly one** of negative signed volume. Which of the two sites fires is **not** distinguishable from outside; both carry the same message and separating them needs instrumentation, which the checkpoint run was forbidden to add. **This is a hard error, not an ineligibility:** `plinth_hollow::run` (`:351–370`) converts only `IneligibleNoCavity`, `IneligibleBelowBulk` and `IneligibleSavings` into a typed `HollowStageOutput::Solid`; everything else propagates, and this propagates. **Why it was never seen before, stated precisely:** the OD-8 census and every sealed-run gate bypass the hollow stage entirely — `embed_incidence_census.rs:142` builds `HollowedModel::solid_decision(mesh, orientation, BelowThreshold)` straight from `oriented.into_parts()` — so **FU-32 J6's 470 s seal is on the oriented solid and the treant had never entered `plinth_hollow::run` in project history** until this checkpoint. **The treant is not alone, and that was already on the record:** `export-path-design-2026-08-07.md`'s corpus-sweep addendum filed `Cylinder` and `make_box` as a "Refusal finding" with **this same message**, unfixed. So the count is **three of eight corpus models refused by the cavity check**, and the largest, most branched model is one of them — consistent with (but **not proved by**) the reading that a branching form yields several disjoint interior voids where the check demands one. **What this row does NOT claim:** that the check is wrong. It may be correctly refusing a shell that is genuinely not printable; nobody has looked at the geometry. **First action:** dump the treant's post-Boolean shell and count its cavity components and its negative-volume closed components, which decides between "the model produces a multi-cavity shell and the check is right" and "the Boolean/marching path is producing a broken shell". Only then choose a remedy. **Product consequence, stated because it is the reason this row is not cosmetic:** hollowing is where the resin saving lives, and the flagship model currently gets none. **The checkpoint artifact is NOT blocked by this** — it emits from the oriented solid through the `HollowStageRefused` fallback, with the refusal recorded verbatim in the artifact's own `SupportedGeometry` — and no cap, floor, constant or registry entry is touched by opening this row. |
| FU-35 | **FU-8f phase G6c's "a retained round executes no more than it is charged" invariant is FALSE on the `Cylinder`** — executed 23,203 against a charge of 4,146, a 5.6× overrun | **Opened 2026-08-09 by the treant `.chitubox` checkpoint** ([`#checkpoint-treant-artifact-2026-08-09`](#checkpoint-treant-artifact-2026-08-09)), found incidentally while gating the FU-34 fallback and **not caused by it**. `crates/plinth-support/src/specialized.rs:4858` carries `debug_assert!(executed_operations <= baseline_resource.operations)` under a comment stating the property is **"provable, not merely hoped"** and that it is "checked here every round in debug, and pinned by `fu8f_g6c_retained_round_executes_no_more_than_it_is_charged` in release". On the `Cylinder` it is false: the assertion fires with `a retained round executed 23203 load operations against a charge of 4146`. **Independent of the checkpoint's change, and proved so rather than asserted:** `examples/embed_incidence_census` shares none of the export-seam code and takes the same `HollowedModel::solid_decision(BelowThreshold)` handoff, and a **debug** build of it at this tip panics identically — `PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Cylinder" ./target/debug/examples/embed_incidence_census`. The census's own last flushed row before the panic reads `retained_load_charged_operations=4146 retained_load_executed_operations=4146`, so the overrun belongs to a **later** round than the one the meter had flushed. **Why nobody had seen it:** every routine consumer of this path is a **release** build, where `debug_assert!` is compiled out, and the release pin named in the comment is a unit fixture rather than a corpus run — so no gate in the workspace has ever carried the `Cylinder` into `run_support` in a debug profile. `make_box` on the same path is clean (executed 532 ≤ charged 716), which is why the checkpoint's new test uses `make_box`; the checkpoint deliberately did **not** pick a model that hides this. **What this row does NOT claim:** that the charge is too low, or that the meter is wrong, or that any cap should move — the direction of the defect is exactly what has not been established. It could be an under-charge (a real accounting gap) or an over-count in `state.executed_operations` (a metering bug costing nothing real); those have opposite remedies and **the evidence does not yet separate them**. **First action:** run the `Cylinder` in debug, capture the round index and the per-round charge/executed pair for every round rather than the flushed subset, and determine which of the two figures is wrong before touching either. **Do NOT raise the charge to make the assertion pass** — that would enshrine the defect as a budget, which is FU-8b's whole lesson. **No cap, floor, constant or registry entry is touched by opening this row**, and the treant's own artifact is unaffected: the treant does not trip this assertion and the checkpoint's release runs are unaffected either way. |
| FU-36 | **Every emitted sphere carries a 16-sliver south-pole seam**: `sphere_triangles` closes its north cap on `sin(0) == 0` but its south cap on `sin(PI) == 1.2246e-16`, so the emitted sphere is not exactly closed — and the one slackened bound in the emit closure test exists solely to accommodate it | **Opened 2026-08-09 by the `wind_outward` ruling execution** (`05-chitubox-and-corpus.md` §2, the *Executed (2026-08-09)* record's second measured item, where the full derivation lives). **Number note: filed at the next free number under the recorded collision protocol** — FU-34 was appended to this same table earlier on 2026-08-09 by concurrent checkpoint work, which is precedent, not an obstacle — and **no existing row was renumbered, edited or displaced**. **The measurement.** `crates/plinth-emit/src/lib.rs`'s `sphere_triangles` walks its rings on `sin(theta)` with `theta` spanning `0..=PI`; `f64::sin(PI)` is `1.2246467991473532e-16`, not `0.0`. With `SPHERE_LATITUDES = 8` and `SPHERE_LONGITUDES = 16`, the north cap therefore closes on a single true point while the south cap's 16 fan apices land on a **16-gon of radius `r * 1.2246e-16`** instead of one point — so each sphere soup is a closed surface with a 16-sided sliver seam of about `1e-16` of its area. Every `ContactSphere` (type 7) and `JointSphere` (type 9) record this crate writes carries it; on the seven-model corpus sweep that is **624 of 1,728 records** — 257 type-7 contact spheres and 367 type-9 joint spheres. **Why this is a ledger item and not just a curiosity:** the closure bound in `every_emitted_soup_is_closed_which_the_winding_decision_requires` is `1e-12` rather than exactly `0.0` **solely** to accommodate this seam — the seam clears it by four orders and a real leak would not — so the single loosened bound on the emit side has, until this row, had no tracked item to trace to, and the implementer's own test comment asks for an owner ruling that a topic-journal note alone would never prompt. **What this row does NOT claim:** that any emitted byte is wrong, or that any artifact already shipped is unsound. The area defect is ~`1e-16` of the sphere and it changes no winding decision — the signed-volume sign is stable across it, proved on the same seven-model sweep (1,728 soups, every one reading outward, 257 correctly reversed). **This is not a bound to raise:** `1e-12` is the accommodation already made and it must not be widened; if a future closure defect exceeds it, the geometry is wrong, not the bound (FU-8b's standing lesson). **Why it was not repaired when found:** the `wind_outward` change was a relocation with a bit-identity acceptance condition, and **repairing the seam moves emitted geometry**: the 16 south apices move in x and y from `r*1.2246e-16` to `0.0` in every sphere record the crate writes. Whether that reaches the wire is per-record and must be measured rather than assumed — the apices are `center + r*sin*cos`, so for a sphere centred well away from the model origin the offset rounds away in f32 and the bytes hold, while near the origin, or if the now-degenerate slivers are dropped rather than kept (which changes the record's triangle count), they do not. Either way it is a numerical change needing its own phase, and the relocation's acceptance condition was bit-identity, so it could not be made there. **First action:** an owner ruling on whether the seam is repaired at all (it costs nothing today), and if so, that the repair lands in a change permitted to move emitted bytes, with the corpus artifacts re-measured against the A/B baseline recorded in `05-chitubox-and-corpus.md` and the `1e-12` bound **tightened** rather than left slack once the seam is gone. |
| FU-37 | **The selected hollow wall is recorded in-process only — no persisted artifact says which wall built it** | **Opened 2026-08-09 by the wall-knob restoration review.** **Number note: this row was opened as "FU-36" on `feature/hollow-wall`, and "FU-36" was already taken** by the sphere south-pole-seam row directly above (opened 2026-08-09 by the `wind_outward` ruling execution, landed on `develop` first); it is filed at the next free number instead and **no existing row was renumbered, edited or displaced** — the same collision protocol FU-34 records. `CavityEvidence::wall_thickness_mm` (`crates/plinth-hollow/src/lib.rs`) does record the operator's stated wall, and that is what §3.10.3 leans on, but `CavityEvidence` is **not `Serialize`**: it hangs off `HollowedModel::cavity_authority` and dies with the process. The serialized `HollowReport`, `CavityRecord` and `DrillingEvidence` carry **no wall field**, so two artifacts prepared at 2.5 mm and 3.0 mm are byte-indistinguishable in metadata in every file the pipeline writes — the geometry differs, the record does not say why. **What this row does NOT claim:** that the wall is unrecorded (it is, in-process, and the CLI refuses walls the lattice cannot produce), or that any gate is wrong. **Why it is deferred rather than fixed in the restoration commit:** adding a wall field to `HollowReport` changes serialized report bytes and therefore moves frozen goldens, which is a separate, gated change with its own re-pin. **First action:** decide whether the wall belongs on `HollowReport` or on the drilling evidence, then land the field and the golden re-pin together. Until then §3.10.3 states the in-process limitation explicitly rather than claiming persisted auditability. **No cap, floor, constant or registry entry is touched by opening this row.** |

## Test-count counting recipe (2026-08-07) {#test-count-recipe-2026-08-07}

Certified gate test counts have been re-litigated across agents more than once — 946/947 and
947/948 historically, 970/971 immediately before this section was written. **The pair is real,
not noise, and both halves are correct — they are just two different, both-valid, invocations.**
The gap is caused by exactly one feature-gated unit test:
`chitubox_io::py::tests::every_format_gate_error_has_a_stable_python_kind` in
`rust/chitubox_io/src/py.rs`, compiled only when `chitubox_io`'s sole optional feature (`python`,
the PyO3 binding) is enabled. `python` is not in chitubox_io's `default = []` feature set, so:

- `cargo test --workspace --offline --no-fail-fast` (gate 1, default features) compiles without
  it and passed **971** as this section was written.
- `cargo test --workspace --offline --all-features --no-fail-fast` (gate 2) turns `python` on
  and passed **972** — one more, always that one test, verified by diffing the two runs'
  per-binary `running N tests` lines: every other of the 82 test-result sections (65 unit /
  integration binaries plus 14 per-crate doc-test batches, one per workspace member — the
  workspace has 14 members, but `plinth-cli` alone contributes two unit-test binaries,
  `src/lib.rs` and `src/main.rs`, which is why unit-test binaries outnumber doc-test batches by
  exactly one crate's worth) reports an identical count in both logs.

So: **"the test count" is not one number**, it is a pair, one per gate invocation, and quoting
either half without saying which gate produced it is what has been re-litigated as drift. Prior
disagreements (946/947, 947/948) are almost certainly the same ±1 default/all-features pair
observed at earlier points in the workspace's growth, not a flaky suite.

**A second, independent trap for a naive count**: do not grep the raw log text for lines that
look like individual test results and sum them. `crates/plinth-core/tests/compile_fail.rs` and
`crates/plinth-support/tests/compile_fail.rs` are `trybuild` harnesses — each is exactly **one**
`#[test]` fn as far as cargo's own libtest accounting is concerned (each contributes 1 to its
binary's own `running N tests` / `test result: ok. N passed`), but internally `trybuild` prints
its own `test tests/compile_fail/<fixture>.rs ... ok` line per fixture file it compile-checks —
23 such lines across the two harnesses in this workspace (3 + 20), none of which is a libtest
test and none of which appears in any `test result:` summary. A log-wide
`grep -c '\.\.\. ok$'` can therefore overcount by up to 23, depending on colour/tty: `trybuild`
colours its own output independently of libtest, so with ANSI colour on, those 23 fixture lines
do not end in a bare `ok` and the naive grep only *coincidentally* matches the libtest lines
alone — do not rely on that coincidence, because with colour off the same grep counts all 23.
Only a sum keyed off the `test result: ok. N passed` summary lines — the number libtest itself
already computed per binary — is authoritative.

**Canonical counting command** (run whichever gate's count you need to state, and say which):

```sh
cargo test --workspace --offline --all-features --no-fail-fast 2>&1 \
  | tee "${TMPDIR:-/tmp}/plinth-test-run.log" \
  | tail -5
grep -oE '^test result: ok\. [0-9]+ passed' "${TMPDIR:-/tmp}/plinth-test-run.log" \
  | grep -oE '[0-9]+' \
  | awk '{s+=$1} END{print s}'
```

Swap in `--offline --no-fail-fast` (no `--all-features`) for the default-feature count. Report
the number **with** the invocation it came from (e.g. "971 default-feature / 972 all-features"),
never a bare figure — that is the one canonical command and the one canonical report shape;
future verifiers should stop re-deriving this from scratch.

## Ordered resumption checklist

1. Reconcile the live head and preserve the recorded SCAR acceptance. **Updated 2026-08-05:
   the head to reconcile is `develop` in the definitive checkout; the integration worktree
   these commands used to name is deleted.**

   ```sh
   git -C /Users/craig/code/hughescr/dnd-campaigns/plinth-3d status --short --branch
   git -C /Users/craig/code/hughescr/dnd-campaigns/plinth-3d log -3 --oneline --decorate
   git -C /Users/craig/code/hughescr/dnd-campaigns/plinth-3d diff --check
   ```

   Then cut a `feature/*` branch off `develop` for whatever comes next; do not commit
   directly to `develop` except for merges, and do not push to `github` without the owner
   saying so.

   The independent ACCEPT for `b5a3290` and its integrated successors are recorded above. If the live head has advanced,
   inspect its ancestry and require independent review for each new change before extending
   that acceptance.

2. **Done — do not repeat.** The frozen coverage golden was regenerated in the `1673a16`
   lineage. `codex/integrate-canonical-delicacy-o5` and its worktree were deleted. The
   accepted preparation interface and stable CLI parser are integrated as `92fa26a` and
   `2b933c`; canonical delicacy as merge `240d144`; the preparation resource policy as merge
   `c6a0884`; the M0-value/signed-zero delicacy regressions as merge `1eb538c`; the corpus
   defect fixes as merge `6558255`; and **the embed/contact-physics package as merge
   `81ec3d4`**. The blocked resource-policy commits were replayed as `73e7c13`/`0b0f27a`
   with their three missing test classes and merged; `codex/prepare-interface` was retired
   with a recorded containment proof.

3. **Do not re-ask for rulings that have landed.** OD-1..OD-9 are all discharged and
   OQ-1..OQ-7 and OQ-11 are closed — see *Embed/contact package complete (2026-08-05)*.
   The still-open owner questions are **OQ-8** (refuse or resize a partially engaged
   contact), **OQ-9** (PR-5's normative text is lost — reconstruct or drop), and **OQ-10**
   (revive the CCO model-anchor family, WITH PR-7 seating, or leave it dead). OQ-10 is a
   print-consequence tradeoff, not a physics question; do not resolve it by inference.

4. **Done — the gate is green.** FU-19 (re-pin the two stale `support_runner_integration`
   goldens, `f77dfe1`), FU-18 (4 `rustfmt` violations, `cf2c2ab`) and FU-20 (retire
   `codex/embed-zones` and `census/embed-incidence-2026-08-04`) all landed on 2026-08-05
   (late). The re-pin's full attribution is in *Post-merge interaction delta*; do not redo
   it. **`make_toed_slab` still needs re-pinning once more when FU-7 lands** — its freeze is
   provisional on this new baseline, which was a deliberate, recorded choice rather than an
   oversight.

### Critical path (2026-08-05) — the next five items, in order

5. **Specialized merge-gate proposal quadratic (FU-8c)** — successor to FU-8b, which is
   now fully resolved. This stays first because it is still the single largest blind spot
   in the whole engine: the corpus's only real mini contributes nothing, so every physics
   conclusion currently rests on procedural fixtures plus two primitives. As of 2026-08-06
   the treant clears perception, the candidate-surface grid, the specialized gate's
   proposal-free batch and the **entire load stage**, in 81.5 s at 2.84 GB peak RSS. It
   stops at
   `Specialized(GateOperationLimit { required: 21755124103146, limit: 35445500736 })` —
   a stated demand against a *measured, model-scaled* bound, 613× over. The work is the
   defect listed under FU-8c: the merge gate re-evaluates every contact against every
   coverage sample once per merge proposal, and the treant offers 39,223 proposals. Do
   **not** raise the cap: no honest coefficient covers a plan quadratic in the batch, and
   one that did would take most of the corpus off its floors. Price and evaluate the
   proposal variants incrementally against the base batch instead, re-measure, then
   re-derive `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` rather than inheriting it. Re-run
   the census after each fix; the treant's curvature and self-intersection are what OD-9
   and PR-4 were written for. The full measurement record, the exact derivation of the
   21,755,124,103,146 demand, the census proof that no geometry moved, and owner ruling R6
   are in *FU-8b resolved; the treant's stopping point moved (2026-08-06)*.

   > **SUPERSEDED 2026-08-06 (FU-8c landed) — dated addendum; the item above is left
   > standing.** The proposal factor is gone: the merge gate evaluates once per batch and
   > only route-local deltas per proposal, and `gate_resource_plan` is repriced to describe
   > that. The treant's demand fell **108×**, from 21,755,124,103,146 to
   > **201,480,535,329** against the same unchanged 35,445,500,736 cap, in 129 s at 2.53 GB
   > peak RSS. **It still refuses**, 5.68× over, so this item stays first on the critical
   > path — but its remaining work is now **FU-8d**, and both parts of it are measured, not
   > guessed: 97.4 % of the residual is per-proposal `sources x samples` work on the
   > perturbed route key (fixable by retaining the three nearest sources per sample), and
   > behind it sits a `variants x faces x contacts` finalizer term the plan charges as
   > `variants x faces`. Full record, derivation and the exact census transcript:
   > *FU-8c fixed 2026-08-06 — the proposal factor is gone; the treant's demand fell 108×
   > and still refuses*.

   > **SUPERSEDED AGAIN 2026-08-06 (FU-8d phase F2 landed) — dated addendum; both blocks
   > above are left standing.** The `sources × samples` residual named above is gone: a
   > merge proposal answers its nearest from the baseline sweep's retained witnesses plus
   > one merged-seat kNN batch, so that term fell **94.2×** and the treant's demand fell a
   > further **27.6×**, to **7,299,316,559** against the same unchanged 35,445,500,736 —
   > **`gate_resource_plan` now passes.** This item stays first on the critical path, and
   > its remaining work is the *second* part of FU-8d, which is now the treant's whole
   > wall: the `variants × faces × contacts` finalizer and the per-variant owner-face
   > descriptor scan. The scan crosses the limit at **variant 275 of 39,224**
   > (`required: 35445618202, limit: 35445500736`), and over those 275 variants the exact
   > finalizer term is already 3,036,066,151,957 — extrapolating to ~4.33 × 10¹⁴. 129 s at
   > 2,641,504 KB (2.52 GiB) peak RSS; still a refusal, not an EXCEEDS-TIMEBOX. Full
   > record: *FU-8d phase F2 landed 2026-08-06 — the nearest delta, and the first stopping
   > point to move since FU-8b*.

   > **SUPERSEDED AGAIN 2026-08-06 (FU-8d phase F3 landed) — dated addendum; all three
   > blocks above are left standing.** The second part named above is gone too. The load
   > partition is finalized **once** per batch and a proposal re-answers only
   > `elig(left) ∪ elig(right)` against it; the per-variant owner-face descriptor scan falls
   > from 35,445,417,382 at variant 275 to **136,791,451** charged once; the
   > `variants × faces × contacts` finalizer term ceases to exist; and the per-variant window
   > matrices — a tens-of-gigabyte byte wall that had never fired only because the operations
   > check refused first — are deleted rather than repriced. The treant's first merge batch
   > states **9,273,857,317** against the **unchanged** 35,445,500,736, **26.2 %**, and
   > **executes**: it evaluates 39,224 variants, accepts a merge, and opens the next batch.
   > **The merge gate is no longer this model's stopping point.** What is: the per-variant
   > `assess_coverage`, `39,224 × 405,146 ≈ 1.59 × 10¹⁰` uncharged sample visits per batch,
   > opened as **FU-8e**. The treant planned eight batches inside the 600 s timebox and did
   > not finish the tournament — an **EXCEEDS-TIMEBOX with counters**, not a refusal, at
   > ~2.4–2.5 GB peak RSS. Full record: *FU-8d phase F3 landed 2026-08-06 — the load
   > partition delta, and the treant enters its merge tournament*.

   > **AMENDED AGAIN 2026-08-06 (FU-8d phase F4 measured) — dated addendum; all four blocks
   > above are left standing exactly as the owner and the measurements made them.** One clause
   > in the block immediately above is wrong in proportion and is corrected here rather than
   > rewritten there: "What is: the per-variant `assess_coverage`". F4 sampled the running
   > treant instead of inferring from counts, and `assess_coverage` is the **second** frame at
   > 24–34 % of main-thread samples. The **leader, at 59–62 %, is F2's own
   > `fill_proposal_nearest_delta`**, which builds a fresh `PointCloudIndex` kd-tree over a
   > **one-point** cloud once per proposal — 39,223 constructions and teardowns per gate
   > batch, dominated by allocator traffic. **FU-8e is opened on both**, in that order. The
   > gate itself is confirmed clear: **two** 600 s attempts at the F3 tip, both
   > **EXCEEDS-TIMEBOX**, **no typed refusal at any cap in either**, 2.5 GiB peak RSS with the
   > 90 GB watchdog never firing. Neither run completes the gate, so **ruling R-f is still
   > unsatisfied** and FU-23's coefficient is still not re-derived. **No cap was raised,
   > lowered or touched.** Full record: *FU-8d phase F4 measured 2026-08-06 — the treant is
   > sampled, not inferred; EXCEEDS-TIMEBOX twice*.

   > **AMENDED AGAIN 2026-08-07 (FU-8e phases E1–E4 landed) — dated addendum; all five blocks
   > above are left standing exactly as written.** FU-8e is **RESOLVED on the defect it
   > named**, and the block immediately above has two clauses that were wrong about
   > *mechanism* and are corrected here rather than rewritten there. **The kd-trees were
   > free**: one one-point `PointCloudIndex` costs 83.3 ns, so the 39,223 of them total
   > **≈3.27 ms** per batch **[arithmetic corrected 2026-08-07 (records fix): published as
   > "16.7 microseconds", an arithmetic slip in the design pass; 39,223 × 83.3 ns = 3.267 ms,
   > and 16.7 µs would have implied 0.426 ns per build. The per-build 83.3 ns stands; only the
   > product is corrected and the conclusion is unchanged — 0.009 % of the 37.33 s frame, so
   > construction is negligible and the fix targets the query loop. Primary note:
   > `support-resource-caps-2026-08-05.md` §12.2]**. What cost was the **query loop** — ~1.04 × 10⁹ query iterations
   > and ~2.08 × 10⁹ heap allocations per batch. And `assess_coverage`'s cost was **not**
   > allocation churn but **two unconditional full scans** of the nearest array per variant,
   > ≈254 GB of reads per batch. E1 gives the kernel a one-source fast path whose bit-identity
   > with the tree's answer is argued in five clauses and pinned on `to_bits`; E2 makes the
   > gate consume it; E3 hands a variant the retained baseline objective unless one of its
   > rewritten rows moved an uncovered row. **Measured 2026-08-07 under ruling R-g**: the
   > merge round falls **63.83 s → 14.68 s (4.35×)**, coverage **17.05 → 0.001 s**,
   > `touched_rows` **1,041,544,686 to the row**, **zero** coverage fallbacks in 33 rounds,
   > peak RSS 2.446 GiB. **The stated demand did not move — 9,273,857,317 against the
   > unchanged 35,445,500,736 — and no cap, floor or coefficient was touched.** The treant
   > still has **no seal**: the outcome is **EXCEEDS-TIMEBOX at 601 s with 33 merge rounds**,
   > and the arithmetic says one round commits one merge, so ≤1,365 rounds by the contact
   > bound and ~460 at `make_raking_blade`'s ratio put the gate stage at **≈1.8–5.5 h, 11–33×
   > the box**. **Ruling R-f remains UNSATISFIED and nothing was re-derived.** The successor
   > is **FU-8f**, recorded as a finding **awaiting owner ruling** and deliberately not built.
   > Full record: *FU-8e phases E1–E4 landed 2026-08-07* and *FU-8f opened 2026-08-07*.

   > **SUPERSEDED AGAIN 2026-08-07 (FU-8f phases G1–G5 landed) — dated addendum; all four
   > blocks above are left standing.** The owner ruling came (Craig, 2026-08-07) and FU-8f was
   > built, so "awaiting owner ruling and deliberately not built" is history. **G1** replaced
   > the load finalizer's dense `contacts × rows` scan with a contact-major CSR transpose —
   > 8,210,643 eligible cells of 160,834,206 visited, bit-identical including the
   > `(distance, contact_id)` tie-break. **G2** hashes the mesh once per gate stage instead of
   > twice per batch, the *check* staying at both sites. **G3** stops the variant loop at the
   > first proposal that is not worse than the baseline — the batch is still **charged** for
   > 39,224 variants and now **executes 2 to 4** of them, charged and executed severed and both
   > stated, `GateRoutePlanMismatch` re-founded prefix-wise. **G4** counted the rounds at last,
   > on a quiet machine: **436 and 430** merge rounds in two 602 s attempts, one merge per round
   > exactly, both **EXCEEDS-TIMEBOX**, peak RSS 2.554/2.528 GiB, watchdog never fired. **G5**
   > repriced the load charge to match the code G1 had already changed: the stated demand falls
   > **9,273,857,317 → 4,525,792,233** (26.2 % → **12.8 %** of the same unchanged
   > 35,445,500,736) and `exact_operations` **10,457,910,996 → 5,709,845,912**, entirely via the
   > load term **6,592,813,742 → 1,844,748,658**. **No cap was raised, lowered or touched in any
   > of the five phases**, and all seven frozen OD-8 digests are byte-identical throughout.
   > **This item stays on the critical path and the treant still has no seal**: the stopping
   > point is unchanged and is still wall-clock, **ruling R-f remains UNSATISFIED**, and nothing
   > was re-derived. Full record: *FU-8f phase G3 landed 2026-08-07*, *FU-8f phase G4 measured
   > 2026-08-07*, *FU-8f phase G5 landed 2026-08-07*.

   > **SUPERSEDED AGAIN 2026-08-08 (FU-8f phase G6d measured) — dated addendum; all five blocks
   > above are left standing exactly as written.** The clause every block above repeats — "the
   > stopping point is still wall-clock", "ruling R-f remains UNSATISFIED" — has stopped being
   > true, and is corrected here rather than rewritten there. **The treant's gate stage COMPLETED
   > for the first time in this project**: the merge tournament ran to its own end at **823
   > rounds** over 826 gate batches, and the whole model run finished in **339 s against the
   > 600 s box** — a **1.77× margin** — in **two** attempts that are **identical on every
   > counter**. **The stopping point has therefore moved OUT of the gate**, to a new typed
   > refusal in previously-unexercised code, printed identically by both attempts:
   > `Preparation: Initial(Limit { resource: "work items", required: 232585208, limit: 4000000 })`.
   > That is opened as **FU-8g** and deliberately not fixed: `initial::preflight` bounds
   > `work = contact_count × (4 + nfaces)` = **542 × 429,124 = 232,585,208** against a **flat,
   > unscaled** `max_work_items = 4,000,000` — **58.1×** over, and **the FU-8b / FU-8c shape a
   > fourth time**, a `contact_count 542 × 429,124 faces` product measured against a bound that
   > does not scale with the model. It is a preflight **bound**, not a measured demand
   > (`hits_per_ray = nfaces` is the "every face could be hit" worst case while the executed query
   > is BVH-bounded), and it is **NOT a cap to raise without measurement**: the remedy, as in
   > FU-8b, FU-8c and G5, is to make the bound honest rather than the cap bigger. **Ruling R-f's
   > precondition is now MET** — a treant run has completed the gate, twice, with counters — so
   > re-deriving `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is **UNBLOCKED**, and the
   > re-derivation is **deliberately not performed**, per the G6 design's **D-G6-6**; no
   > coefficient moved. **No cap, floor or coefficient was raised, lowered or touched by any G6
   > phase**, and all seven frozen OD-8 digests are byte-identical. This item stays on the
   > critical path and the treant still has **no seal**. Full record: *FU-8f phase G6d measured
   > 2026-08-08*.

   > **ADDENDUM 2026-08-09 (FU-8g H1/H2 landed) — the block above is left standing exactly as
   > written; one number in it has stopped describing the engine and is corrected here.** The
   > remedy the block names — "make the bound honest rather than the cap bigger" — was applied.
   > `initial::preflight` now charges the census pass's counted `Σ tests` (executed traversal
   > effort) rather than `contact_count × (4 + nfaces)`, `work` and `hits` are severed, every
   > eager reserve is right-sized to a counted per-row budget, and a charged-vs-executed receipt
   > is checked at the charge site. **The treant's charge is 108,378 at 553 contacts — 2.71 % of
   > the same, untouched 4,000,000 cap**, not 58.1× it. **No cap, floor or constant was raised,
   > lowered or touched**, the D5.3 cap-scaling contingency is **not triggered and not added**,
   > and a fixture proves an over-demand mesh still refuses at the unchanged caps. Two corpus
   > models' honest charge is *higher* than the worst case it replaced, four orders of magnitude
   > under the cap. All seven frozen OD-8 digests byte-identical. **The treant still has no
   > seal**, and what stops it next is unmeasured — H6 owns that run. Full record: *FU-8g phase
   > H2 landed 2026-08-09* ([`#fu8g-h2-2026-08-09`](#fu8g-h2-2026-08-09)).

   > **ADDENDUM 2026-08-09 (FU-8g H3 landed) — the block above is left standing exactly as
   > written; the "still owed by H3" pointer in H2's own record is now satisfied.** The four
   > `ContactConstructionLimits` numbers, brace's six `PenetrationBatchLimits` fields and raft's
   > four are registered in the M0 registry as `Placeholder`, classification
   > `runtime_resource_cap`, with honest provenance — measured-demand single-specimen markers
   > (per R4/FU-23) where H0/H1/H2 measured one, a plain disclosure of absence where no run has
   > ever reached the cap (all of `SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS` and the ten brace/raft
   > constants). `constants.md` regenerated by its producer; the FU-24 ledger gate is green.
   > **No cap, floor or byte term was raised, lowered or touched.** This item stays on the
   > critical path: H3 changes no code, so nothing about the treant's next stopping point moved.
   > Full record: *FU-8g phase H3 landed 2026-08-09* ([`#fu8g-h3-2026-08-09`](#fu8g-h3-2026-08-09)).

   > **ADDENDUM 2026-08-09 (FU-8g H4 landed) — the block above is left standing exactly as
   > written; H4 is the independent rider, off the critical path, and touches nothing this item
   > describes.** `RetainedLoadRebuildCause::Poisoned` now carries a `PoisonCause`
   > (`Guard3Generic` | `CommitShape` | `IdentityWalk`) so the treant's 21 poison-driven rebuilds
   > (§G6d.8) can be attributed by a future run instead of only counted, and
   > `census::MergeTournamentCensus` adds a committed-round counter and a named exit reason
   > (`"no_proposals"` | `"no_safe_proposal"` | `"iteration_cap"`) beside the existing gate census
   > row, so a transcript proves its own round count without scratch instrumentation. **Neither is
   > hashed into any digest** — determined from source and proven by a corpus census run: all
   > seven frozen digests reproduced byte-identical with both counters live. **No cap, floor or
   > constant was raised, lowered or touched.** This item stays on the critical path: H4 changes
   > no code this item's own stopping-point analysis depends on. Full record: *FU-8g phase H4
   > landed 2026-08-09* ([`#fu8g-h4-2026-08-09`](#fu8g-h4-2026-08-09)).

   > **ADDENDUM 2026-08-09 (FU-8g H5 landed) — the block above is left standing exactly as
   > written; H5 adds no engine code, only the separated proof.** The census-vs-execution
   > equivalence harness (`crates/plinth-support/tests/penetration_census_equivalence.rs`) proves
   > on real corpus fixtures (`make_raking_blade`, `make_overhang_table` with its genuine hole
   > row) and a synthetic 120-crossings-on-one-ray dense stack that execution under counted
   > budgets is `to_bits`-identical to execution under worst-case budgets, that every row's
   > counted `crossings`/`tests` equal its executed `hits`/`work`, and that over-demand refuses
   > as the typed preflight at the exact counted number. The mutation triad was applied, observed
   > and reverted — notably, the budget-broadcast mutation (c) is caught by NOTHING else in the
   > workspace (`native_api` passes 32/32 under it), so the harness is load-bearing, not
   > decorative. **No cap, floor, constant or digest input was touched**; all seven OD-8 digests
   > reproduced byte-identical. H1–H5 are now complete; **only H6 (the treant run under the
   > memory protocol) remains.** Full record: *FU-8g phase H5 landed 2026-08-09*
   > ([`#fu8g-h5-2026-08-09`](#fu8g-h5-2026-08-09)).

   > **ADDENDUM 2026-08-09 (FU-8g H6 measured — the phase is complete) — the block above is left
   > standing exactly as written; the wall it describes no longer stops the treant, and what does
   > is a different defect entirely.** H6 added no engine code (`git diff` over `crates/` and
   > `rust/` empty) and ran the treant twice under the standing memory protocol. **The
   > initial-contact wall is broken:** `build_initial_contacts` ran to completion on the Weeping
   > Treant for the first time — no run in this project's history had ever passed it — confirming
   > H2's 108,378 charge against the **untouched** 4,000,000 floor is sufficient and not merely
   > smaller. **D5.3's cap-scaling contingency is not triggered and not added; no cap, floor or
   > constant was raised, lowered or touched.** Both attempts then ended **EXCEEDS-TIMEBOX at
   > 600 s**, peak RSS **3,693 / 3,721 MiB** — 4.0 % of the 90 GB watchdog arm, confirming H1's
   > reserve collapse at treant scale against a predicted 14–32 GB. The treant's successor
   > stopping point is **not a cap**: a per-terminal re-digest of the whole accumulated CCO
   > transcript, measured rising 19.7 → 77.1 % of the routing stage and derived to the line from
   > `runner.rs:1650–1666` / `cco.rs:1397` / `cco.rs:1775–1862`. Opened as **FU-32**, together
   > with the finding that a watchdog-killed run emits **no census rows at all**, so H4's rider
   > could not fire and the 21 poison rounds are still unattributed. All seven OD-8 digests
   > byte-identical; gates 1052/1054, 0 failed. **FU-8g H0–H6 are complete. The treant still has
   > no seal.** Full record: *FU-8g phase H6 measured 2026-08-09*
   > ([`#fu8g-h6-2026-08-09`](#fu8g-h6-2026-08-09)).


6. **Model-anchor bundle — PR-7 seating + tournament-bias investigation + tripwire
   retirement.** Treat these three as one unit; landing any alone is incoherent. **Organic
   minis need this family**: a treant or a PC mini wants to anchor to the model, and today
   *every* CCO winner corpus-wide is a plate or a bifurcate — **0 of 3429 attempted model
   anchors survives, on any model**, and all 3186 attested anchor seats are
   `centreline = false`. So the bundle is: (a) find out whether the tournament is *biased*
   against model anchors or whether they are correctly losing, (b) land PR-7
   `CCO_ANCHOR_MASK_AT_THE_SEAT` so a revived anchor has a proven interface rather than a
   voxel-rounding kiss, and (c) retire the commit-time `NotAttached` tripwire (`6a35d45`),
   which exists precisely to hold the line until (b). Gated on owner ruling **OQ-10** —
   reviving this family deliberately scars the mini. Do the tournament-bias measurement
   first; it is evidence the owner needs to answer OQ-10, and it requires no ruling.

7. **`make_toed_slab` parity investigation (FU-7) — freeze is PROVISIONAL.** Vertex-probe
   vs. interior-probe containment. It moves the member count (`72 -> 42` when last
   attempted), so the required-table freeze `11_859_964_443_161_510_103` cannot be trusted
   and the `support_runner_integration` re-pin (FU-19) should wait on it. Sequence this
   before FU-19, not after.

   > **CLOSED 2026-08-08 — dated addendum; the item above is left standing as written.**
   > The FU-10 phase-C cutover retires the vertex-probe entirely rather than hardening it,
   > and lands on the same `72 -> 42` this item predicted, digit for digit. See
   > [`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08) and the FU-7 ledger row above.

8. **Per-layer polygon union under a fill rule (FU-10) — owner-endorsed.** Supersedes the
   Tier-1 unowned-hole fallback that currently lets the treant past perception. Candidate
   nomination must consume normalized layers per OD-9. Pairs naturally with item 5: the
   treant is the model whose slices motivate it, and FU-16 (bbox-claimed vs. discarded
   unowned-hole split) is the measurement that closes it out.

   > **LANDED 2026-08-08 — dated addendum; the item above is left standing as written.**
   > Phases A-C shipped on `feature/slicer-union`; the Tier-1 fallback is deleted, not
   > bypassed. The pairing with item 5 predicted here did not occur as sketched: FU-16 did
   > not get a treant measurement, because phase C deletes the bbox-claim machinery FU-16
   > would have measured, mooting rather than closing it out — see the FU-16 ledger row
   > above and [`#fu10-repin-2026-08-08`](#fu10-repin-2026-08-08).

9. **Prepare execution runner -> CLI -> `.chitubox` output for the minis.** This is the item
   that turns the engine into the thing the project exists for. Resolve a complete
   `PrepareRequest`, load/cache inputs, run the move-only stage chain, emit the
   artifact/report once, keep resource admission at stage boundaries, then wire the stable
   CLI parser through to a `.chitubox` file the owner can open. Items 5–8 are what make the
   output trustworthy; this is what makes it exist. Note FU-2 (`validate()` width vs
   per-type constructor contracts) was explicitly deferred until this lands, because only
   here do the real call sites exist to judge against.

### Longer-horizon milestones

10. Complete former M7 in its documented gate order:

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

   The preparation interface and CLI parser are integrated; the execution runner itself is
   item 9 above. The blocked resource patch is not authorization to bypass its tests.

11. Run the M7 acceptance commands in `04-migration-plan.md` only after every slice above is
   integrated. Record full-corpus score evidence and owner ChituBox review separately; the
   two GUI checkpoints in this file do not by themselves satisfy all M7 acceptance.

12. Continue M8: dense L2/L1/exact orientation, Pareto records, `plinth calibrate`,
   LoadSharing A/B, exact-byte 1/4/16-thread determinism, cache-on/off equality, and kernel
   benchmarks. O-6 makes redesign/calibration post-Python backlog, while determinism of the
   minimally ported pipeline remains a deletion gate.

13. Complete M9/native-handoff deletion under O-6/O-7: eliminate every remaining Python,
   PyO3, maturin, shim, and Python-only control; update docs/skills; run workspace test,
   Clippy, format, zero-Python searches, prepare/score, and the documented clean-checkout
   re-fire. The owner-print checkpoint remains binding. Never claim M9 from an existing
   dirty or artifact-bearing worktree.

14. Before any final merge, re-run:

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
