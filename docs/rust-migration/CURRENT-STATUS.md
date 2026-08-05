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

The current reviewed integration HEAD is `a3c7f6464dd219da7b7628183e96c740d5e194d6`
(`merge: embed-zones Phase B safe groundwork (EmbedEnd->ContactEnd rename; package BLOCKED
pending owner rulings)`), reached by the **second 2026-08-04 integration pass** from
`1673a1628b1af04ec180fc204db84675eec8fc10`, plus the embed-zones Phase B safe-groundwork
merge described below:

| Commit | What |
| --- | --- |
| `1eb538c72b957606e48ba665df609a0e657d1485` | merge of `codex/delicacy-regressions` — M0-value and signed-zero delicacy regressions (test-only, +69/-0 inside `delicacy.rs`'s `mod tests`) |
| `4c8881eaf4186b55a9f05254dde80769daad69bc` | comment correction on that test's sole-reader claim |
| `c6a0884a19c591f010d073b38e48e60495a469c9` | merge of `codex/prepare-interface-v2` — preparation resource policy plus three sealed-input test classes |
| `029ced3f97ccebb8ba6593efba2d732493c3c625` | `refactor(support): rename EmbedEnd to ContactEnd` from `codex/embed-zones` — safe groundwork only; see *Embed-zones Phase B outcome (2026-08-04)* |
| `a3c7f6464dd219da7b7628183e96c740d5e194d6` | merge of the rename above — `merge: embed-zones Phase B safe groundwork` |

The full gate is **green** at `a3c7f64`: `cargo test --workspace --offline` **889 passed /
0 failed**; clippy `-D warnings` clean; `cargo fmt --all -- --check` clean; `git diff --check`
clean. (Full gate was previously green at `c6a0884` with 877 passed; the higher count is
the embed-zones Phase A test surface — including `designed_embed.rs`'s own suite — entering
the workspace via the `e64f0ba` merge, not new tests from the rename itself.) This is the
first pass in which the workspace gate was fully green — the frozen coverage golden was
regenerated in the `1673a16` lineage, so it no longer masks regressions.

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
| endpoint-connected embed zones | `codex/embed-zones` at `/private/tmp/plinth-embed-zones` | `029ced3` (rename groundwork also integrated as `a3c7f64`) | **Phase B stopped by design; package BLOCKED on OQ-1..OQ-7** (2026-08-04) | Do not integrate the package. The safe groundwork (`029ced3`, `EmbedEnd`->`ContactEnd` rename) is already integrated. Further implementation is blocked until the owner rules on OQ-1 through OQ-7 — see *Embed-zones Phase B outcome (2026-08-04)*. A corpus incidence census on `census/embed-incidence-2026-08-04` (off `029ced3`) is running to inform OQ-1/OQ-2/OQ-3. |

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

This table is the live inventory as of the **second** 2026-08-04 integration pass: **6
branches, 6 worktrees**. Since the first pass, `codex/integrate-canonical-delicacy-o5`,
`codex/delicacy-regressions`, `codex/prepare-interface-v2`, and `codex/prepare-interface`
and their worktrees were deleted after their content was verified integrated. The rows for `codex/canonical-delicacy`, `codex/home-test-protocol`,
`codex/migration-handoff-2026-08-03`, `codex/prepare-cli`, `codex/chitubox-diagnosis`,
`codex/shared-surface-authority`, `codex/m3-decimate-proxy`, and the five
`codex/support-*` drafts were **removed** because those branches and their worktrees no
longer exist. See `WORKTREE-DISPOSITIONS.md` for the per-branch proof, archive tags, and
the owner's untagged-deletion decision.

| Worktree/branch | Observed head | State |
| --- | --- | --- |
| `/Users/craig/code/hughescr/dnd-campaigns/plinth-3d`, `master` | `5e37d8c` | main checkout and ref store; strict ancestor of integration |
| `/private/tmp/plinth-3d-pipeline-integration`, `codex/rust-pipeline-integration` | `a3c7f64` | clean; integration authority. Full gate green (889 tests, clippy/fmt/diff clean) |
| `/private/tmp/plinth-embed-zones`, `codex/embed-zones` (worktree now on `census/embed-incidence-2026-08-04`) | `029ced3` | Phase B stopped by design; safe groundwork (rename) merged to integration as `a3c7f64`; package BLOCKED on OQ-1..OQ-7; incidence census running — see *Embed-zones Phase B outcome (2026-08-04)* |
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

## Follow-ups ledger {#follow-ups-2026-08-04}

Non-blocking items raised by the 2026-08-04 reviews. None gates the current head; each
should be scheduled deliberately.

| # | Follow-up | Note |
| --- | --- | --- |
| FU-1 | Mechanize the class-(b) cap-table tie to `SupportPreparationInput::validate` | Currently the cap table and the validating boundary agree by inspection only. Needs a small production affordance to make the tie mechanical. |
| FU-2 | Decide `validate()` width vs per-type constructor contracts | Defer until the execution runner lands, when the real call sites exist to judge against. |
| FU-3 | Six sites still construct `NativeSurfaceConfig`/`ProbeConfig` with `usize::MAX` outside the validating boundary | The three latent fixture defects on the merged path were fixed; these six remain outside it. |
| FU-4 | Commit a corpus provisioning script, or make the corpus-dependent targets loud-skip | Today a bare checkout hard-fails two test targets with no explanation. See `corpus-provisioning.md`. |
| FU-5 | Pin the `scar.rs` duplicate registry-reader (`crates/plinth-metrics/src/scar.rs:183-186`) the way the new delicacy M0 test pins the `plinth-mesh` reader | The duplication is now documented in the delicacy test comment but is still unpinned on the `plinth-metrics` side. |
| FU-6 | `chitubox-native-preview` fix `9ffc829` still awaits review and cherry-pick | Sole live carrier of the unlanded embed-native-preview fallback. |

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

2. **Done — do not repeat.** The frozen coverage golden was regenerated in the `1673a16`
   lineage and `cargo test --workspace --offline` is green (877 passed) at `c6a0884`.
   `codex/integrate-canonical-delicacy-o5` and its worktree were deleted. The accepted
   preparation interface and stable CLI parser are integrated as `92fa26a` and `2b933c`;
   canonical delicacy as merge `240d144`; the preparation resource policy as merge
   `c6a0884`; and the M0-value/signed-zero delicacy regressions as merge `1eb538c`.

3. **Done — do not repeat.** The blocked resource-policy commits were replayed on the
   regenerated-golden base as `73e7c13`/`0b0f27a`, the three missing test classes were
   added (execution-budget refusal, malformed-cap refusal, sealed-input compile-fail),
   independent review returned ACCEPT, and the work merged as `c6a0884`. The superseded
   `codex/prepare-interface` was retired with a recorded containment proof.

4. **Phase B of `codex/embed-zones` stopped by design (2026-08-04) — do not resume it as an
   implementation task.** Safe groundwork (the `EmbedEnd`->`ContactEnd` rename, `029ced3`)
   is integrated as merge `a3c7f64`. The redesign correctly halted when it found OD-2's
   premise falsified rather than build on it — see *Embed-zones Phase B outcome
   (2026-08-04)*. The next actionable step is an **owner ruling on OQ-1 through OQ-7**, not
   further Rust implementation. A corpus incidence census on
   `census/embed-incidence-2026-08-04` (off `029ced3`) is running to give OQ-1/OQ-2/OQ-3
   empirical grounding first. Re-review remains BLOCKED until the owner rules and a
   subsequent Phase B pass implements the ruling.

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
