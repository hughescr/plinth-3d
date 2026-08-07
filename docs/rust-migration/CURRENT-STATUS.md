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
| FU-8 | ~~Treant resource caps defect — "production caps do not admit a real mini"~~ | **PARTLY RESOLVED 2026-08-05, and the brief's framing was corrected by measurement.** Two of the three named caps were genuinely too small and are now model-scaled on the treant's own measurement: `KnnBatchLimits.max_work` → `max(100_000_000, 2048 × faces)` = 878,837,760, and `CandidateSurfaceGridLimit` → `max(67_108_864, 512 × faces)` = 219,709,440. The treant clears both. The third, Specialized `GateOperationLimit` (`2_000_000_000`), **was falsely accused and is deliberately NOT raised**: its reported `required: 2000000001` was `limit + 1`, a charge counter tripping on a selection sort in `unique_island_face_count`. That sort is gone as of 2026-08-06 (FU-8b fix (A)) and the treant now refuses with a *stated* demand, `required: 12714828544`, on gate work the fix does not touch; the cap was still not raised: FU-8b closed 2026-08-06 and it became the FLOOR of a measured batch-product rule (§3.10.10), unchanged at 2,000,000,000 and still the whole cap for every corpus model. Three further caps nobody had seen (`max_load_operations`, `max_load_scratch_bytes`, `max_gate_peak_bytes`) were correctly refusing an 8.0 M-row load-eligibility explosion on a 429k-face mesh; FU-8b fix (B) collapsed those rows to the 134,368 distinct faces they always were, after which the two byte caps are met with room to spare and only the operation cap needed the measured rule. **[Corrected 2026-08-06 (record fixes F1, F3): the preceding sentence is left standing but is wrong twice — it was a *predicted* row count that collapsed, not a row set (`build_load_eligibility` was already a per-face `BTreeMap` collapse at `eb36358`), and the post-fix figure is 130,548, not 134,368. See the dated addendum under *FU-8b resolved; the treant's stopping point moved*.]** Remaining work is algorithmic, tracked as FU-8b. See `docs/rust-migration/support-resource-caps-2026-08-05.md`, and *Owner caps-landing rulings R1–R5 (2026-08-05, late)* for the five adjudicated findings from that landing's review — R1 in particular removed a **third**, undisclosed scaled cap that this row's "two of three" framing did not count. |
| FU-8b | ~~Treant support gate/load stage is quadratic~~ | **RESOLVED 2026-08-06.** All three defects fixed and the deferred cap bound then set on post-fix measurement. **(a)** (`0bdec0c`) `unique_island_face_count` / `visit_served_owner_faces` found distinct face ids by repeated-minimum selection sort; they now gather each island once and `sort_unstable` + `dedup` in one batch. **(b)** (`1706c5d`) every consumer that had to predict the extended load row count took the `(owner, face)` **pair** count instead of the distinct **face** count — 7,988,078 against 134,368, 59.5× inflated; the pairs are now grouped by face once (`GroupedOwnerFaces`) and the two counts are stated separately (`SpecializedLoadCounts`). **(c)** (`e0a4bed`) `finalize_specialized_load_partition` rescanned the pair list per base row and per pair (~6e13 uncharged comparisons, the 16-hour hang, 4,127/4,127 stack samples in that one frame); it now joins base rows to the existing grouping in one linear merge, proved against a verbatim copy of the replaced algorithm over a 4×5 cross product of shapes. **Then measured, not assumed:** the treant's honest post-fix demands are 12,714,828,544 gate operations on its proposal-free batch and 5,402,535,619 load operations over 134,368 rows **[corrected 2026-08-06 (record fixes F1, F2): 130,548 rows — the demand is right, the row count paired with it was not; and in (b) above, 7,988,078 → 130,548 is 61.2×, and it was a *prediction* that was inflated, not a row set]**, with load scratch (144,753,256 B) and gate peak (215,061,920 B) comfortably under their unchanged caps **[corrected 2026-08-06 (record fix F7): the gate peak is **216,203,088 B** = 215,061,920 + 1,141,168 face-scratch as of `23d929d`, which charged the batch's shared distinct-face buffer against the bound; 215,061,920 was the peak at `d82c895` only. The 1,073,741,824 B cap is unchanged and the batch is still at 20.1 % of it]**, at 2.84 GB peak RSS and 81.5 s to refusal. Both operation caps therefore became `max(floor, 64 × batch_product)` on measurement — **not** on face count, since 24-face `make_raking_blade` charges 612,887,212 gate operations against the 429,120-face treant's 12,714,828,544. Every corpus model stays bit-identical at the floor and the census prints each materialization. See `03-physics-and-constants.md` §3.10.10 and `support-resource-caps-2026-08-05.md` §7. The treant now clears the gate's proposal-free batch and the whole load stage; its remaining refusal is a **new** defect, FU-8c. |
| FU-8c | ~~Specialized **merge** gate re-evaluates the whole batch per proposal~~ | **RESOLVED 2026-08-06** (`a3308ec`); the finding as first written is left standing below and the resolution is appended to it, not substituted for it. Opened 2026-08-06 by the FU-8b post-fix measurement, and the successor stopping point for the Weeping Treant. `gate_resource_plan` prices a merge batch at `(samples + demands + 1) × (proposals × (contacts − 1) + contacts)`: every merge proposal re-evaluates the entire contact set against every coverage sample. The treant offers **39,223 proposals** over 1,366 contacts and 405,146 samples, so the plan states **21,755,124,103,146** operations against a materialized cap of 35,445,500,736 — 613× over, and refused. This is the same batch-first violation as FU-8b (a)/(b)/(c) in a fourth dress: a per-item rescan of a whole batch standing in for one batched evaluation. It is **not** a cap to raise — admitting it would need a per-batch-unit coefficient near 39,300, which would multiply every corpus model's cap by the same factor and take the Cylinder (already at 89.6% of the floor), `make_raking_blade` and others off their floors, i.e. it would admit real quadratics on small models. The fix is to price and evaluate the proposal variants incrementally against the base batch rather than re-running it, then re-measure. Note for FU-23: fixing it will change the gate's charged work, so `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` should be re-derived afterwards rather than inherited. **[Resolution appended 2026-08-06 (`a3308ec`).** The proposal factor is gone. `GateBatchIndex` builds the proposal-invariant state — samples by route key, contacts by route key, three satisfying contacts per demand — once per batch; each proposal re-answers only the route keys its own merged pair serves and re-tests each demand once against the merged seat. `gate_resource_plan` is repriced to `(samples + demands + 1) × contacts + measured proposal work`, and the predecessor's per-variant sweep is retained `#[cfg(test)]` as the reference the delta path is compared against row by row on `f64::to_bits`. **No cap was changed** and no cap code was touched. The treant's stated demand fell from 21,755,124,103,146 to **201,480,535,329** — 108× — against the same 35,445,500,736. **It still refuses**: 5.68× over, at the same gate, re-measured byte-identically at `8695398` (124 s, 2.50 GB peak, watchdog never fired). The residual is 97.4 % of the demand and is a *different* defect, opened as **FU-8d**; FU-8c is closed on the defect it named, not on the treant reaching a seal. The FU-23 note above still stands and is now due. See *FU-8c fixed 2026-08-06*.**] |
| FU-8d | Per-proposal nearest is still `sources × samples` on the perturbed key, `unique_face_finalizer_work` is under-charged, and the per-proposal planning passes are uncharged and doubled | Opened 2026-08-06 by the FU-8c post-fix measurement; the successor stopping point for the Weeping Treant, and both parts are measured rather than predicted. **(1)** After FU-8c the per-proposal work is route-local but still a full `sources × samples` sweep of the perturbed key. On the treant that residual is **196,264,386,588** of the printed 201,480,535,329 — **97.4 %**, averaging 5,003,809 per proposal — because its coverage samples concentrate in a few very large route keys and its merge proposals sit on exactly those. It need not be `sources × samples`: retaining the **three** nearest sources per sample during the baseline sweep answers "nearest among the survivors" for any two-contact removal by inspection, leaving one distance test per touched sample against the merged seat, so the term becomes `samples` on the perturbed key. That is FU-8c's own top-`k` witness argument applied to the nearest instead of the demands. **(2)** The exact fold's `unique_face_finalizer_work` is `variants × faces × contacts` while `gate_resource_plan` charges it as `variants × faces`. On the treant the true term is ≥ 117,741 × 1,365 × 39,223 ≈ 6.3 × 10¹², which is also the arithmetic proving the refusal came from the plan and not from a completed fold. The under-charge is why this term has never been the refusing one; its superlinearity is why it will be as soon as (1) lands, so the two parts should be fixed together. **(3)** *[Third part added 2026-08-06; (1) and (2) are left standing as opened. Deliberately NOT coded yet — recorded so FU-8d is scoped honestly, and the cheapest of the three.]* **The per-proposal planning passes are themselves uncharged, and there are two of them.** `evaluate_gate_batch` runs `plan_proposal_nearest_delta` once per proposal inside `plan_gate_nearest_batch` (`specialized.rs:3553`, to build the plan) and **again** once per proposal inside the `exact_operations` fold (`specialized.rs:3571`, to charge it) — on the treant **2 × 39,223 ≈ 78,446 delta passes** — while the check that refuses the batch, `if exact_operations > gate_operation_limit`, is at `specialized.rs:3629`, **after both**. The entire cost of deciding the treant may not proceed is therefore paid *before* the refusal and **none of it is charged**: the plan charges the delta's *result* (`nearest_work`), never the cost of computing the delta. That is why the treant burns 124 s to state a demand it is then refused on, and it is a hole in what the cap means as much as a cost — a resource plan that cannot be afforded until after it has been executed bounds nothing. The second pass is a **free 2×** to reclaim: the fold recomputes a delta the planning pass already computed for the same proposal, so retaining the planning pass's per-proposal deltas turns two passes into one with no change of result. Sequence it **with (1), not before it** — (1) rewrites what a delta pass *is* (`sources × samples` on the perturbed key becomes `samples`), and halving the count of a pass about to change shape optimizes the wrong thing. Corollary, and the reason this is a part rather than a footnote: after (1) and (3) the planning work is small enough to charge, so **FU-8d should charge it**, rather than leaving the gate free to spend unbounded effort discovering it cannot afford to proceed. A fourth candidate — the per-variant `assess_coverage`, `O(samples)` per variant under its own `max_coverage_operations` cap — is recorded as **unmeasured**, not as a finding, because the treant has never reached it. As with FU-8c none of this is **a cap to raise**. Note for FU-23: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` should be re-derived after FU-8d lands, not after FU-8c. **[Phase F1 landed 2026-08-06 — the finding stays OPEN in all three parts; F1 changes no answer and buys no speed.** The baseline coverage sweep now retains `min(3, source_count)` nearest sources per sample — `(source_slot, squared_distance)` in the kernel's emission order — which is the witness table part (1)'s delta will read; nothing reads it yet. Pinned first by two tests: `fu8d_knn_row_zero_is_k_independent_bit_for_bit` (the kernel's row 0 is bit-identical at k = 1 and k = 3, over engineered exact ties and random clouds — the design's riskiest claim) and `fu8d_three_retained_witnesses_answer_any_two_removal` (three retained slots answer any two-contact removal exactly, against a kernel query over the survivors; it fails if the slot count is set to 2). Charges: **no operation charge moves** — `KnnBatchPlan`'s work is `source_count x query_count`, k-independent (ruling R-a, recorded in the `NEAREST_WITNESS_SLOTS` doc comment). Bytes do move and are charged: `+2 x query_count x 16 B` per route inside `KnnBatchPlan::logical_peak_bytes`, and `samples x 3 x 16 B` per batch as the new `GatePeakUsage::nearest_witness_bytes` (~19 MB on the treant, against the unchanged 1 GiB gate peak). `coverage_route_knn.max_neighbors` goes 1 -> 3 (ruling R-b), stated once in `SpecializedLimits::production`: a `KnnBatchLimits` **shape field, not a cap**, and the `SUPPORT_KNN_MAX_NEIGHBORS` registry note, ledger entry, `constants.md` and §3.10.9 table each carry the dated amendment. **No cap raised, no cap code touched.** Five gates green; all seven frozen OD-8 digests byte-identical; the treant's refusal line unchanged at `required: 201480535329, limit: 35445500736` — that immobility is the phase's proof. See *FU-8d phase F1 landed 2026-08-06*.]** **[Phase F2 landed 2026-08-06 — part (1) is CLOSED; parts (2) and (3) stay OPEN, and part (2) is now the treant's wall.** A merge proposal's nearest is now the minimum of two things: the first surviving retained witness, read as a *value*, and ONE `KnnBatchPlan(1 source, touched_rows queries, k = 1)` over the merged seat covering every perturbed key at once — combined by `f64::total_cmp` with the square root taken once *after* the minimum, so an exact survivor/merged tie is a tie between bit-identical `f64`. Pinned by `fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit` against the retained `#[cfg(test)]` full sweep, on a fixture that asserts it reached a narrow key, a total removal, a displaced holder, a steal, and an exact tie; truncating the witness scan to slot 0, or dropping the merged seat from the minimum, each make it fail. Repriced in `gate_resource_plan`: `nearest_work` becomes `Σ 1 × touched_rows`, `key_visits` is newly charged. Measured on the treant: the per-proposal nearest term **196,264,386,588 → 2,083,167,818 (94.2×)** — reconciling exactly as `195,222,841,902 + 1,041,544,686` — and the stated demand **201,480,535,329 → 7,299,316,559 (27.6×)**, **20.6 % of the unchanged 35,445,500,736**, so that check now PASSES. **No cap raised, no cap code touched.** The refusal moved one site: `variant_specialized_owner_face_count`'s descriptor budget crosses at **variant 275 of 39,224**, printing `required: 35445618202, limit: 35445500736` — a crossing, not a total. Over those 275 variants `unique_face_finalizer_work` is already 3,036,066,151,957, extrapolating to ~4.33 × 10¹⁴, against routed work of 3,912,181: **part (2) is the whole remaining wall.** Rulings recorded: **R-c** measured and NOT adopted (the radius bound removes 42 % of rows, a 1.7× constant, and the margin is 4.9× — over the 2× threshold); **R-d** precondition met (28 of 48 perturbed keys carry ≥ 10 proposals, one carries 26,543) but deliberately not built; **R-f** NOT satisfied — `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` waits for F3 *and* a treant run that COMPLETES the gate, which this is not. Five gates green; all seven frozen OD-8 digests byte-identical, `make_raking_blade`'s 1,402 real merge proposals through the rewritten code being the live witness; treant run 129 s at 2,641,504 KB (2.52 GiB) peak RSS. See *FU-8d phase F2 landed 2026-08-06*.]** **[Phase F3 landed 2026-08-06 — parts (2) and (3) are CLOSED, and with (1) closed at F2 this finding is RESOLVED on the defect it named. The treant's merge gate no longer refuses.** The load partition is finalized **once** per batch, retaining three nearest eligible contacts per row plus each contact's won rows, carried area and violation state; a proposal re-answers only `elig(left) ∪ elig(right)`, which the merged seat serves *exactly* because `parts_serves_owner` is true for an island precisely when `left` or `right` serves it. Rows outside the union keep a bit-identical candidate set in the same order, so their argmin is the baseline's. Carried areas are **re-summed** in the fold's own ascending row order rather than adjusted incrementally, because `f64` addition is not associative. Pinned by `fu8d_load_delta_matches_the_per_variant_finalize` against the whole production finalizer run on hand-renumbered variant inputs, row by row, on `load_violation_count`, and on carried area by `f64::to_bits`; it asserts it reached an exact survivor/survivor tie, an exact survivor/merged-seat tie, a row with no survivor, a displaced holder, a steal, and a row outside the union. The **per-variant window matrices are deleted, not repriced** — `gate_window_matrix_bytes` stated tens of gigabytes against the 512 MiB `max_gate_contact_matrix_bytes` and would have become the next refusal the moment the operations wall fell. Planning is 4 → 1 with a count-derived pre-check (`proposal_planning_precheck`) run *before* the loop, and the per-variant contact/demand rescans are derived (`GateVariantTotals`; a proposal variant's logical contact work is the baseline's minus one, exactly). Repriced to ONE authority: the `variants × faces` load model is replaced by `specialized_load_resource_plan`'s own figure — 4,618,272,984 → **6,592,813,742**, i.e. the honest number is 1.43× the under-charge part (2) named — while `descriptor_operations` falls from 35,445,417,382 at variant 275 (~5.06 × 10¹² extrapolated) to **136,791,451** charged once, and `unique_face_finalizer_work` (~4.33 × 10¹⁴ extrapolated) ceases to exist. Stated demand **9,273,857,317** and `exact_operations` **10,457,910,996** against the **unchanged** 35,445,500,736 — 26.2 % and 29.5 % — and the batch **executes**. `GateRoutePlanMismatch` is re-founded as plan-vs-execution and is explicitly **weaker**; the record says so. **No cap raised, lowered or touched.** The treant planned 8 gate batches (proposal-free + 7 merge rounds, 1,366 → 1,360 contacts, share flat at 26.2 %) inside the 600 s timebox and did **not** finish the tournament: recorded as EXCEEDS-TIMEBOX with counters, ~2.4–2.5 GB peak RSS, watchdog armed at 90 GB and never fired. The successor is the per-variant `assess_coverage` — 39,224 × 405,146 ≈ 1.59 × 10¹⁰ uncharged sample visits per batch — opened as **FU-8e** on this measured evidence. **R-f still NOT satisfied**: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` waits for a run that COMPLETES the gate. Five gates green; all seven frozen OD-8 digests byte-identical, with `make_raking_blade`'s 23 batches and 1,402 proposals through the rewritten *load* path the live corpus witness. See *FU-8d phase F3 landed 2026-08-06*.]** **[Phase F4 measured 2026-08-06 — FU-8d stays RESOLVED; F4 lands no engine code and closes the finding's record rather than its algorithm.** F4's deliverables are a clean treant run at the committed F3 tip and the record sweep F3 still owed. **Outcome (c) EXCEEDS-TIMEBOX, twice** — 607 s / 2.49 GiB and 602 s / 2.48 GiB, `PLINTH_THREADS=4`, watchdog armed at 90 GB and **never fired**, **no typed refusal at any cap in either attempt**, no batch counters (the tip carries no `PLINTH_FU8D_MEASURE` instrumentation, verified absent by grep, so §10's per-batch table stays the counter record). Attempt 2 sampled six windows across the timebox because attempt 1's single window at t = 600 could not separate "dominates the run" from "was running at the end". **The design predicted the wrong frame.** `assess_coverage` (part (4)/P4, the deferred candidate) is real but **second**, at 24–34 % of main-thread samples; the leader at **59–62 %** is **F2's own `fill_proposal_nearest_delta`**, and its cost is construction rather than arithmetic — it builds a fresh `PointCloudIndex` kd-tree over a **one-point** cloud once per proposal, 39,223 constructions and teardowns per gate batch, its call-graph children being `knn_batch_planned` over `_xzm_free`/`_free`/`_xzm_xzone_malloc` allocator traffic. Batched *within* a proposal, unbatched *across* proposals. Both go to **FU-8e**, in measured order. Also recorded, not acted on: `PLINTH_THREADS=4` buys the merge gate nothing (all four rayon workers in `__psynch_cvwait` for the whole gate stage while preparation is genuinely parallel — a note, **not** a reason to raise thread counts), and `merged_violation_count` is a steady third at ~4.6 %. **R-f STILL NOT satisfied** (neither attempt completes the gate, so `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is not re-derived); **R-c revived as a question** — the frame a radius bound would have shrunk is now the measured leader — recorded as FU-8e input and not adopted. **No cap raised, lowered or touched.** F4 also pays what F3's every-record-site sweep missed: `03-physics-and-constants.md` §3.10, the `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` and `SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS` registry notes (hence a regenerated `constants.md`), and `specialized.rs`'s `gate_operation_limit` doc comment all still said the treant was refused at the merge gate; and the caps doc's phase-F3 section had landed as a duplicate `## 9`, now **§10**. See *FU-8d phase F4 measured 2026-08-06* and `support-resource-caps-2026-08-05.md` §11.]** |
| FU-8e | The merge gate's per-proposal kd-tree construction, and the per-variant `assess_coverage`, are both uncharged and both unbatched across proposals | **Opened 2026-08-06 on FU-8d phase F4's measured evidence**, which is the footing the deferral ruling required — and opened on **two** parts in the order the sampler found them, not the one the design predicted. The treant's merge gate no longer refuses anything: since F3 it states 9,273,857,317 against an unchanged 35,445,500,736 and is admitted. What stops the treant is **wall-clock**: two 600 s attempts, both EXCEEDS-TIMEBOX, at 2.5 GiB peak RSS with the 90 GB watchdog never firing. **(1) `fill_proposal_nearest_delta` — 59–62 % of main-thread samples, the leader.** F2 correctly batched a proposal's nearest into one `KnnBatchPlan` covering all its touched rows; it did not batch *across* proposals. Each of the treant's 39,223 proposals constructs a `PointCloudIndex` kd-tree over a **one-point** cloud (`[proposal.position_mm]`), queries it, and drops it — 39,223 constructions and teardowns per gate batch, whose sampled children are `knn_batch_planned` over `_xzm_free` / `_free` / `_xzm_xzone_malloc` and `kiddo`'s `nearest_n_within_recurse` descending a single-node tree. A distance to one point needs no spatial index at all; the batch-first fix is to answer every proposal's touched rows without per-proposal index construction. **(2) `assess_coverage` — 24–34 %, second.** `O(samples)` per variant with ~7 `Vec::with_capacity(405,146)` allocations per call: `39,224 × 405,146 ≈ 1.59 × 10¹⁰` sample visits and of order a terabyte of allocation churn per gate batch, none charged against `gate_operation_limit`. This was the *only* part the design predicted, and F3's record asserted it was "what the 600 s is being spent on"; F4 measured it at about a quarter. Both parts are **uncharged**, so **neither is a cap to raise** — a cap cannot be too tight for work it never prices, and the remedy removes work rather than admitting more. Third and named so it is not a later surprise: `SpecializedLoadBaseline::merged_violation_count` at a steady ~4.6 %. Two riders from FU-8d: **ruling R-c is revived as a question** (§9.5 declined the radius-bounded steal path at a 4.9× margin against a 1.7× constant — F4 makes the frame it would have shrunk the measured leader, so re-measure before deciding), and **`PLINTH_THREADS=4` buys the merge gate nothing** (all four rayon workers sit in `__psynch_cvwait` for the whole gate stage while preparation is genuinely parallel), which is a note about the wall-clock being single-core and **not** a licence to raise thread counts. **Ruling R-f still gates FU-23**: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is re-derived only from a treant run that COMPLETES the gate, and no run yet has. See *FU-8d phase F4 measured 2026-08-06* and `support-resource-caps-2026-08-05.md` §11. |
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
| FU-24 | ~~`m0-constants-source-ledger.json` is hand-maintained and had drifted from the registry~~ | **RESOLVED 2026-08-07.** The ledger's `registry_entries` array is prose provenance for the live `M0PlaceholderConstants`/`M0EvidenceConstants` registry (264 published ids as of this row); nothing mechanically tied the two together, so a new registry constant could land without its ledger entry and nobody would notice. Diffing ledger `id`s against the registry's own published id set found exactly two drifted: `INITIAL_AXIS_MAJORITY_CAP` and `INITIAL_AXIS_MAJORITY_STEP_MM`, added to the registry 2026-08-02 (commit `3d79a07`, "Add initial contact stub quarantine leaf") with no matching ledger entry ever added. Both are now entered with honest provenance — `Placeholder`, classification `runtime_search_cap`/`runtime_search`, normative `04-migration-plan.md §O-10` (there is no §3.x entry in `03-physics-and-constants.md`), and source locators pointing at the Rust declaration and consumer sites rather than a nonexistent Python legacy source, since this pair was a native-Rust O-10 declaration from the start. The drift was silent because nothing checked it: `crates/plinth-core/tests/constants_ledger_consistency.rs` now parses the ledger's `registry_entries` and asserts its id set equals the registry's published id set in both directions, cheaply (one JSON parse against the one registry construction every other constants test already pays for). Verified to fail on reintroduced drift before landing. `constants.md` was unaffected throughout — it renders from the registry alone, never from the ledger, so it already carried both ids; only the ledger and this row were behind. |
| FU-25 | Decimation — topology diagnosis, then a real downstream-utility witness | **DEMOTED 2026-08-06 by owner ruling R6**, from "runs first" to a diagnosis + witness follow-up, because measurement falsified the compounding-speedup premise. The blocker is `topology_not_increased`, not `assess_downstream_utility`: `nonmanifold_edges` goes 0 → non-zero at every attempted treant ratio and `geometrically_eligible_candidate_count` is **0**, so minting a witness today moves the treant by zero faces. The ratified geometric budget admits only ~6–9 % reduction anyway (p99 0.00643 mm at r = 0.9406), which would still leave the gate charge ~7× over the flat floor. Sequenced as: diagnose the 3 non-manifold edges of the 425,934-face candidate → register constants and mint the witness (zero corpus delta) → make the reduction reachable and re-pin what it moves. Note the two live constraints: `crates/plinth-orient/tests/orientation_rederivation.rs` hard-asserts **no** reduction for the treant against its tracked 21,456,084-byte artifact, and `Sphere` + `make_toed_slab` would decimate the instant a witness exists, moving frozen digests `17667468971446251870` and `11199296506741562956` — hence the size threshold is proven necessary, not merely prudent. Full design, evidence and open questions Q1–Q8: [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md). |

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
`grep -c '\.\.\. ok$'` therefore overcounts by the fixture count; only a sum keyed off the
`test result: ok. N passed` summary lines — the number libtest itself already computed per
binary — is authoritative.

**Canonical counting command** (run whichever gate's count you need to state, and say which):

```sh
cargo test --workspace --offline --all-features --no-fail-fast 2>&1 \
  | tee /tmp/plinth-test-run.log \
  | tail -5
grep -oE '^test result: ok\. [0-9]+ passed' /tmp/plinth-test-run.log \
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
