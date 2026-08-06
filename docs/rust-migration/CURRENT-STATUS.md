# Rust migration current status

Durable handoff recorded 2026-08-03; updated after the 2026-08-04 integration pass, and
extended 2026-08-06 with the FU-8b resolution and owner ruling R6. This
file distinguishes integrated work from reviewed-but-branch-only work and from work still
awaiting review. A review result is not evidence that a commit has been merged.

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
| FU-7 | `make_toed_slab` parity investigation (vertex vs. interior containment probe) | The digest-moving containment-probe hardening was deferred; the current `make_toed_slab` required-table freeze (digest `11_859_964_443_161_510_103`) is **still PROVISIONAL** until this lands, and so is the 2026-08-05 seal re-pin (`4826202191472851835`) — the freeze is now provisional **on the new baseline**. See *Corpus defect fixes landed (2026-08-04, late)* and the cross-reference in *Post-merge interaction delta*. |
| FU-8 | ~~Treant resource caps defect — "production caps do not admit a real mini"~~ | **PARTLY RESOLVED 2026-08-05, and the brief's framing was corrected by measurement.** Two of the three named caps were genuinely too small and are now model-scaled on the treant's own measurement: `KnnBatchLimits.max_work` → `max(100_000_000, 2048 × faces)` = 878,837,760, and `CandidateSurfaceGridLimit` → `max(67_108_864, 512 × faces)` = 219,709,440. The treant clears both. The third, Specialized `GateOperationLimit` (`2_000_000_000`), **was falsely accused and is deliberately NOT raised**: its reported `required: 2000000001` was `limit + 1`, a charge counter tripping on a selection sort in `unique_island_face_count`. That sort is gone as of 2026-08-06 (FU-8b fix (A)) and the treant now refuses with a *stated* demand, `required: 12714828544`, on gate work the fix does not touch; the cap was still not raised: FU-8b closed 2026-08-06 and it became the FLOOR of a measured batch-product rule (§3.10.10), unchanged at 2,000,000,000 and still the whole cap for every corpus model. Three further caps nobody had seen (`max_load_operations`, `max_load_scratch_bytes`, `max_gate_peak_bytes`) were correctly refusing an 8.0 M-row load-eligibility explosion on a 429k-face mesh; FU-8b fix (B) collapsed those rows to the 134,368 distinct faces they always were, after which the two byte caps are met with room to spare and only the operation cap needed the measured rule. Remaining work is algorithmic, tracked as FU-8b. See `docs/rust-migration/support-resource-caps-2026-08-05.md`, and *Owner caps-landing rulings R1–R5 (2026-08-05, late)* for the five adjudicated findings from that landing's review — R1 in particular removed a **third**, undisclosed scaled cap that this row's "two of three" framing did not count. |
| FU-8b | ~~Treant support gate/load stage is quadratic~~ | **RESOLVED 2026-08-06.** All three defects fixed and the deferred cap bound then set on post-fix measurement. **(a)** (`0bdec0c`) `unique_island_face_count` / `visit_served_owner_faces` found distinct face ids by repeated-minimum selection sort; they now gather each island once and `sort_unstable` + `dedup` in one batch. **(b)** (`1706c5d`) every consumer that had to predict the extended load row count took the `(owner, face)` **pair** count instead of the distinct **face** count — 7,988,078 against 134,368, 59.5× inflated; the pairs are now grouped by face once (`GroupedOwnerFaces`) and the two counts are stated separately (`SpecializedLoadCounts`). **(c)** (`e0a4bed`) `finalize_specialized_load_partition` rescanned the pair list per base row and per pair (~6e13 uncharged comparisons, the 16-hour hang, 4,127/4,127 stack samples in that one frame); it now joins base rows to the existing grouping in one linear merge, proved against a verbatim copy of the replaced algorithm over a 4×5 cross product of shapes. **Then measured, not assumed:** the treant's honest post-fix demands are 12,714,828,544 gate operations on its proposal-free batch and 5,402,535,619 load operations over 134,368 rows, with load scratch (144,753,256 B) and gate peak (215,061,920 B) comfortably under their unchanged caps, at 2.84 GB peak RSS and 81.5 s to refusal. Both operation caps therefore became `max(floor, 64 × batch_product)` on measurement — **not** on face count, since 24-face `make_raking_blade` charges 612,887,212 gate operations against the 429,120-face treant's 12,714,828,544. Every corpus model stays bit-identical at the floor and the census prints each materialization. See `03-physics-and-constants.md` §3.10.10 and `support-resource-caps-2026-08-05.md` §7. The treant now clears the gate's proposal-free batch and the whole load stage; its remaining refusal is a **new** defect, FU-8c. |
| FU-8c | Specialized **merge** gate re-evaluates the whole batch per proposal | Opened 2026-08-06 by the FU-8b post-fix measurement, and the successor stopping point for the Weeping Treant. `gate_resource_plan` prices a merge batch at `(samples + demands + 1) × (proposals × (contacts − 1) + contacts)`: every merge proposal re-evaluates the entire contact set against every coverage sample. The treant offers **39,223 proposals** over 1,366 contacts and 405,146 samples, so the plan states **21,755,124,103,146** operations against a materialized cap of 35,445,500,736 — 613× over, and refused. This is the same batch-first violation as FU-8b (a)/(b)/(c) in a fourth dress: a per-item rescan of a whole batch standing in for one batched evaluation. It is **not** a cap to raise — admitting it would need a per-batch-unit coefficient near 39,300, which would multiply every corpus model's cap by the same factor and take the Cylinder (already at 89.6% of the floor), `make_raking_blade` and others off their floors, i.e. it would admit real quadratics on small models. The fix is to price and evaluate the proposal variants incrementally against the base batch rather than re-running it, then re-measure. Note for FU-23: fixing it will change the gate's charged work, so `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` should be re-derived afterwards rather than inherited. |
| FU-9 | Hollow EDT margin defect | `grid_spec_from_bounds`'s no-padding path plus its all-foreground special case fabricate a distance ramp; the committed Sphere hollow goldens are wrong today. Est 8/16 pts; regenerates four `plinth-hollow` oracles. |
| FU-10 | Per-layer polygon union under a fill rule | Owner-endorsed; supersedes the Tier-1 fallback. Candidate nomination must consume normalized layers per OD-9. |
| FU-11 | Opt-in SDF/remesh repair mode | Roadmap item. |
| FU-12 | ~~Census doc erratum~~ | **RESOLVED 2026-08-05** (`893dac3`). The erratum is written at the top of `census-embed-incidence-2026-08-04.md`: the Reachability table is marked superseded, the `make_box` rows are struck as invalid (borrowed -30deg pose), the deleted `embed_inset_mm`/cosine-floor formula is called out, and the replacement numbers are appended as *Unified census — 2026-08-05*. |
| FU-12b | ~~`PLINTH_CENSUS_SUBSTITUTE_ORIENT` is dead code~~ | **RESOLVED 2026-08-05** (`6fe750b`). The env var and its `SUBSTITUTE_ORIENT` donor table are deleted from the census example. `make_box` and `Cylinder` now orient natively from their repaired baselines, so no donor-substitution path is reachable; the `MODEL` row still prints `orientation=own_pinned` so the transcript states on its face that no pose was borrowed. |
| FU-13 | `DuplicatePhysicalSeat` error payload should carry position+routes instead of post-sort indices | Fold into any future `SpecializedError` change. |
| FU-14 | `coalesced_seat_count` is deliberately NOT in the plan digest | Kept fixtures byte-identical. Fold it in at the next authorized digest-moving change. |
| FU-15 | Corpus-gated end-to-end test tier, with loud skip for gitignored STLs | So the Cylinder and `make_box` sealed runs (digests `13310185350867878190` / `9657051231007502658`), currently test-uncovered, get pinned. |
| FU-16 | Measure the bbox-claimed vs. discarded unowned-hole split on the treant once caps lift | bbox claiming can under-report a solid's area; the Tier 2 union supersedes it. |
| FU-17 | Contact-area deficits: 12 stub + 554 CCO model-anchor | RECORDED as data by the 2026-08-05 unified census, not adjudicated. These contacts attach over measurably less interface than their tip radius claims. This is the print-quality question OQ-8 asks the owner; the 554 CCO figure is moot unless PR-7 revives that family. |
| FU-18 | ~~4 `rustfmt` violations inherited from the embed tip~~ | **RESOLVED 2026-08-05** (`cf2c2ab`). `cargo fmt --all` touched exactly `cco.rs:1425`, `cco.rs:2485`, `census.rs:852`, `designed_embed.rs:788` and nothing else. `cargo fmt --all -- --check` is now part of every gate run — see process rule 1 in *Post-merge interaction delta*. |
| FU-19 | ~~Re-pin the two stale `support_runner_integration` goldens~~ | **RESOLVED 2026-08-05** (`f77dfe1`). Re-pinned to `make_toed_slab` 64/179/`4826202191472851835` and `make_raking_blade` 32/89/`1709218010932846012`, with the whole delta attributed to PR-8 `NotAttached` on four named contacts and a control run proving nothing else moved. See *Post-merge interaction delta*. FU-7 was NOT settled first, deliberately: it is a separate, currently unscheduled investigation, and the tuple is re-pinnable independently — but `make_toed_slab` **will** need re-pinning again when FU-7 lands. |
| FU-20 | ~~Delete the retired `codex/embed-zones` branch + worktree and the `census/embed-incidence-2026-08-04` ref~~ | **RESOLVED 2026-08-05.** Both were proven contained in integration by `git merge-base --is-ancestor` before deletion; see *Post-merge interaction delta* and the worktree table. |
| FU-21 | Measure the **coverage-route kNN batch** | Opened by owner ruling R1 (2026-08-05). `SpecializedLimits::coverage_route_knn.max_work` now sits at the flat `SUPPORT_KNN_MAX_WORK_FLOOR` (100,000,000) precisely because **nothing has measured it** — the 696 work-units/face figure is the minima-to-face batch's, and borrowing it was the defect R1 removed. The treant's instrumented coverage-route demand is 15,282,619, but that is one point from a model that never reaches the stage in production. Measure the batch properly (it needs the FU-8b gate fix first, so the treant can get there), then either scale it on its **own** coefficient or ratify the floor on evidence. Until then the floor stands and `the_coverage_route_knn_cap_is_the_flat_floor_at_every_face_count` pins it. |
| FU-22 | Classify the **nested support authorities** R5 left out of scope | `CandidatePayloadConfig`, `ContactConstructionLimits`, `PenetrationBatchLimits` and `RaftLimits` — ~19 further values reachable from `SupportPreparationLimits::production` — still have **no registry entry**. They were excluded from R5 deliberately, not by oversight: `CandidatePayloadConfig` in particular mixes **result-changing refinement policy** with execution caps, so a blanket `runtime_resource_cap` sweep would misclassify them. This needs a per-field pass that decides, for each value, whether it can alter an admitted plan. See `03-physics-and-constants.md` §3.10.9. |
| FU-23 | Re-derive the two scaled coefficients when a **second large model** arrives | Standing trigger from owner ruling R4 (2026-08-05). `SUPPORT_KNN_MAX_WORK_PER_FACE` (2048) and `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` (512) rest on **n = 1**, the Weeping Treant v1.2 alone; the next largest corpus model is 90× smaller and sits under both floors. When a second model above ~4,760 faces is measured, re-run `next_power_of_two(2 × observed)` over **both** specimens rather than declaring the new model an outlier. The refs-per-face input is the one measured to vary 526 ↔ 2.83, so it is the likelier of the two to move. |
| FU-25 | Decimation — topology diagnosis, then a real downstream-utility witness | **DEMOTED 2026-08-06 by owner ruling R6**, from "runs first" to a diagnosis + witness follow-up, because measurement falsified the compounding-speedup premise. The blocker is `topology_not_increased`, not `assess_downstream_utility`: `nonmanifold_edges` goes 0 → non-zero at every attempted treant ratio and `geometrically_eligible_candidate_count` is **0**, so minting a witness today moves the treant by zero faces. The ratified geometric budget admits only ~6–9 % reduction anyway (p99 0.00643 mm at r = 0.9406), which would still leave the gate charge ~7× over the flat floor. Sequenced as: diagnose the 3 non-manifold edges of the 425,934-face candidate → register constants and mint the witness (zero corpus delta) → make the reduction reachable and re-pin what it moves. Note the two live constraints: `crates/plinth-orient/tests/orientation_rederivation.rs` hard-asserts **no** reduction for the treant against its tracked 21,456,084-byte artifact, and `Sphere` + `make_toed_slab` would decimate the instant a witness exists, moving frozen digests `17667468971446251870` and `11199296506741562956` — hence the size threshold is proven necessary, not merely prudent. Full design, evidence and open questions Q1–Q8: [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md). |

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

8. **Per-layer polygon union under a fill rule (FU-10) — owner-endorsed.** Supersedes the
   Tier-1 unowned-hole fallback that currently lets the treant past perception. Candidate
   nomination must consume normalized layers per OD-9. Pairs naturally with item 5: the
   treant is the model whose slices motivate it, and FU-16 (bbox-claimed vs. discarded
   unowned-hole split) is the measurement that closes it out.

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
