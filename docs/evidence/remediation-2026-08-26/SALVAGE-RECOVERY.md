# Salvage recovery — the merge decision

Written 2026-08-28 against `develop` at `2f6e1d6`. Every git fact below was re-checked here
rather than taken from a lane or a verifier report. Nothing was merged or pushed to produce it.

This supersedes the report at `docs/salvage-recovery-report` (`9af63db`), which was written
before `feature/salvage-hollow` had a commit. Three of that report's statements are now stale
and are corrected here; its other findings held up and are carried forward with attribution.

---

## 0. The cross-family challenge did not happen. Read this before anything else.

This phase existed for one reason: Claude models wrote these branches, Claude models graded
them, and a same-family proposer and reviewer share blind spots. OpenAI models were to
challenge both gradings.

**The challenge set came back empty.** The input to this reconciliation carried an empty list
where the cross-family challenges belonged. No OpenAI challenge was returned for
`feature/salvage-support-consumers`, and none for `feature/salvage-emit`.

An absent challenge is not agreement. State the consequence plainly:

> **Both verified branches are verified by one model family only.** The blind spot this phase
> was created to close is still open. Every "no defect found" below carries that qualifier.

This does **not** mean the gradings are wrong. Both Claude verifiers did adversarial work of
real quality — the emit verifier broke production code three times to prove its own point, and
the support verifier did the same and reproduced a 605-second search to the exact trial count.
What is missing is an outside reader. Two things follow:

1. Where a ruling below rests on **mechanical evidence** — a command exit code, a file that
   does or does not exist, a test that runs — same-family risk is low, because the evidence is
   reproducible by anyone.
2. Where a ruling rests on **judgement** — "this omission is justified", "this narrowing is
   acceptable" — same-family risk is real and is not retired by this document.

The findings in this report that were reached independently here, and that neither Claude
verifier reported, are marked **NEW**. There are five. Their existence is the best available
measure of how much a genuine outside challenge would still be worth: a second reader working
from the code found five things two prior readers missed, and one of them (§5.2) contradicts a
verifier's escalation outright.

---

## 1. Merge order

| # | Branch | Commits | Action | Gate before it moves |
|---|---|---|---|---|
| 1 | `feature/salvage-support-consumers` | 14 (lands `-core`'s 3) | **MERGE** | None. Ready as it stands. |
| 2 | `feature/salvage-emit` | 1 | **AMEND, then merge** | Correct the commit message and doc comment (§3.2). |
| 3 | `feature/salvage-hollow` | 1 | **SPLIT + owner ruling, then merge** | §4. It now builds, lints and tests clean — measured here. |
| — | `feature/salvage-support-core` | 3 | **DO NOT MERGE SEPARATELY** | Subsumed by #1. See below. |
| — | `feature/salvage-kernel-tests` | 0 | **CLOSE. Delete the branch.** | Already on develop and passing (§5.1). |

### Why this order

**Support first, because it is the only branch that needs nothing done to it.** Its 14 commits
build, lint clean, and its notes are all track-after items, not prerequisites. It is also the
largest and the one most likely to conflict with future work, so it should stop being a branch
soonest. It closes remediation short-list item 5 (the contact-set replay, `5807242`), which
`STATUS.md:55` currently lists as missing.

**Emit second, because its fix is cheap and its defect is in the record, not the code.** The
change is additive test code in one crate. It cannot conflict with support. It must not merge
in its present form, but the correction is an amend, not a rewrite.

**Hollow last, because it is the only one that changes what the pipeline accepts.** Support and
emit change how the engine reasons; hollow changes which models are refused. It should land on
a develop that is otherwise settled, so that if a corpus model starts refusing, the cause is
unambiguous.

### Do not merge `feature/salvage-support-core` on its own

Merging `feature/salvage-support-consumers` also lands `feature/salvage-support-core`: the
consumers branch carries core's three commits through the merge commit `e5c69f6`, whose message
says so ("Merge salvage LAYER 1 ... as the base for LAYER 2 consumers").

It is tempting to merge core first anyway, to get a smaller revert boundary between the
perception/cco layer and the runner/specialized layer. **Do not, without running the gate on it
first.** The verifier built and tested the consumers branch at `2576a86`. Nobody built core
alone. Merging it as its own commit would put a state on develop that no one has ever compiled,
which is exactly the thing a future bisect trips over. If the owner wants that boundary, it is
one build and one clippy run away — but it is not free, and the default is one merge.

---

## 2. `feature/salvage-support-consumers` (with `-core`)

| | |
|---|---|
| **Commits** | 14 — 3 from `-core` (`5154a05`, `5c89ab8`, `239f159`), the merge `e5c69f6`, and 10 consumer commits `f5d29af`..`2576a86` |
| **Files** | 7, all under `crates/plinth-support/src/`: anchor, brace, cco, coverage_closure, perception, runner, specialized. +2011 / −261 |
| **Build** | Clean (verifier; `cargo build --workspace --offline` exit 0, run twice) |
| **Clippy** | Clean, zero warnings under `-D warnings` (verifier) |
| **Tests beyond baseline** | **None.** |
| **Verification family** | **Claude only.** No cross-family challenge returned. |
| **Verdict** | **MERGE.** |

### Tests, stated precisely

`cargo test -p plinth-support --lib` = 638 passed / 0 failed. The six fast integration binaries
= 8/1/4/4/12/13, 0 failed. `plinth-pipeline` green.

`support_runner_integration` reports **three** failures, not two. All three are accepted
baseline:

1. `:346` — `make_toed_slab` fingerprint. Counts `(64, 170)` unchanged; only the digest moved.
2. `:765` — the same, same cause.
3. `:1197` — `closure_repairs_censored_dust_because_that_repair_closes_a_real_hole`. Panics by
   design and reports a live support-planning defect. Recorded as pre-existing in
   `FIX-AFTER-RUN.md:107-110`, verified unchanged at `5d0420e`.

**The trap, and it will catch someone.** The third one reports only above roughly a 700-second
bound, because its repair search alone runs 605 seconds. The lane bounded at 600 s and therefore
reported "exactly the two accepted-baseline failures" in good faith. So does the brief that
framed this work. A merger who bounds more generously will see a third red and mistake it for a
regression from this branch. It is not. The verifier reproduced its recorded `candidates_tried:
274` exactly, on the branch — identical to the value recorded on develop.

### Disagreements, ruled

| Question | Ruling |
|---|---|
| Did the branch quietly re-record a frozen expectation to make a test pass? | **No.** Confirmed here independently: `git diff --stat develop..feature/salvage-support-consumers` lists 7 files, all `crates/plinth-support/src/*`. No file under `tests/` appears. The two disputed fingerprints live in `crates/plinth-support/tests/support_runner_integration.rs`, which the branch does not touch at all. This is the single most important thing to have checked, and it is checkable in one command. |
| Is "the branch changes no test file" the whole truth? | **Not quite, and the wording should not be reused.** `plinth-support` keeps its unit tests inline in `#[cfg(test)] mod tests` inside those same `src/` files, so the branch *does* add test code — the verifier names several new tests itself. The load-bearing claim is the narrower one: no *existing* pinned value was edited. That claim holds. **NEW** — this wording risk was not flagged by either verifier. |
| Do the new tests have teeth? | **Yes.** Three sabotages, three bites, all reverted. The strongest is the second: reverting `select_contacts_baseline` to `crate::select_contacts` produces `Err(StaleRegularContacts)` — the exact defect signature, not a generic failure. |
| Is the narrowed cap-preflight invariant acceptable? | **Yes, merge it.** `8e9f0c6` moves a key-column reservation ahead of the preflight loop, so a refused run now reserves 8 bytes per coverage sample under no cap, and the test that covered it was narrowed from the whole counter struct to four named counters. Both halves are disclosed in the commit message. The reservation is `try_reserve_exact` mapped to a `ResourceLimit` refusal, so it is fail-closed and cannot abort a run; and the load-bearing half still holds — the route-entry total is checked against `max_route_memberships` before anything is reserved. Merging with this costs a slightly smaller invariant. Holding the branch for it costs the other 13 commits. Merge. |
| Should the five declined salvage items have been declined? | **Yes, all five.** B3 (brace plane-slice band) would turn a missing-candidate defect into a hard run failure by guessing two limits. B1 (cap 80→188) is verdict-changing with no test and no corpus run behind it. A1 (anchor post-snap minimum-length re-screen) broke 8 tests whose only repair was rebuilding fixture geometry. S3's binding would move the exact fingerprint already under an unresolved ruling. S1's fixture would have died on its own anti-vacuity assertion. Each declination is recorded in a commit message rather than left silent, which is the behaviour to reward. |

---

## 3. `feature/salvage-emit`

| | |
|---|---|
| **Commits** | 1 — `ae57863` |
| **Files** | 1, `crates/plinth-emit/src/lib.rs`, +73 lines, test code only |
| **Build** | Clean (verifier) |
| **Clippy** | Clean (verifier) |
| **Tests beyond baseline** | **None.** `plinth-emit` 28 passed / 0 failed, plus 6 in `native_winding_oracle`. |
| **Verification family** | **Claude only.** No cross-family challenge returned. |
| **Verdict** | **MERGE, but only after the commit message and doc comment are corrected.** |

### 3.1 The disagreement, ruled

The lane says its new test prevents a regression that would delete `target_path` when the
durability sync fails. The verifier says the test cannot detect that regression, and proved it
by inserting `let _ = std::fs::remove_file(&request.target_path);` into `write()`'s
`DurabilitySyncFailed` arm and watching the suite stay at 28 passed / 0 failed.

**Ruling: the verifier is right, and it is provable by reading the test alone.**

`a_directory_that_cannot_be_synced_after_rename_keeps_the_correct_artifact` never calls
`write()`. It calls `write_staged`, then performs `std::fs::rename` itself, then calls
`sync_dir` itself, then asserts `target.exists()`. The artifact it observes surviving is the one
*the test itself* renamed. Nothing in `write()`'s error arm is on the path. No sabotage of that
arm can change the result. The verifier's sabotage confirms by measurement what the source shows
by inspection.

This is the house anti-pattern: a test that re-implements production logic and then asserts on
its own re-implementation.

### 3.2 What must change, and why an amend is enough

The test is **not worthless**, and it should not be deleted. It pins two real platform facts:
that `rename` succeeds on a directory with write and search but no read, and that `sync_dir`
fails there. Those facts are what make the production arm reachable at all, and its doc comment
explains honestly *why* it drives the two steps directly (restricting the directory any earlier
would break an inner fsync inside `chitubox_io::Project::save` and never reach the rename).

The defect is the **claim**, in two places:

- the doc comment opens "`write` must report [`EmitError::DurabilitySyncFailed`], not silently
  succeed and not delete the artifact" — the test never calls `write` and never inspects an
  `EmitError`;
- the commit message says the named regression "would go undetected" without this test.

A reviewer reading the log would believe `write()`'s durability path is protected. It is not —
`write()`'s durability contract has zero coverage on this branch.

**This must be fixed before merge, not tracked after.** A test with weak teeth is ordinary debt.
A git-log entry that asserts coverage which does not exist is worse than no test, because it
retires the question. The next person to ask "is the durability path covered?" will find this
commit and stop. That is how a verification defect climbs the stack.

The cheap fix is an amend: reword the commit message and the doc comment to say plainly that the
test covers `sync_dir`'s failure detection and the survival of a renamed artifact, and not
`write()`'s handling of that failure. The thorough fix is to rewrite the test to drive `write()`
and assert both `EmitError::DurabilitySyncFailed` and `target_path.exists()`. **Either is
acceptable.** The amend is enough to merge; the rewrite is the better outcome and belongs on the
track-after list either way.

### 3.3 Two lane-report errors, ruled

| Claim | Ruling |
|---|---|
| "`same_point` no longer exists anywhere in `plinth-emit` or `plinth-support`" | **False for `plinth-support`** — it is used at many sites in `specialized.rs` (2908, 2928, 3042, 3185, 7750, 7842-7843, 8073, …). The conclusion it supported holds within `plinth-emit`, so nothing downstream changes, but the stated evidence was not checked before it was written. |
| develop's durability behaviour is "strictly better" than the salvage's | **Overstated.** develop preserves the artifact when the sync fails; the salvage deleted it, to hold a "a refused write leaves no path" invariant. Those are opposite deliberate choices. develop's was ratified in `ad335d9`, so not porting the salvage's is correct — but it is a ratified tradeoff, not a strict improvement, and calling it the latter hides that a real invariant was given up. |

Neither changes the merge decision. Both are the same habit: a conclusion that happens to be
right, resting on evidence that was not checked. Worth naming because it is the habit that
produced the §3.1 defect too.

---

## 4. `feature/salvage-hollow`

| | |
|---|---|
| **Commits** | 1 — `e97743a`, explicitly marked WIP |
| **Files** | 1, `crates/plinth-hollow/src/lib.rs`, +123 / −11 |
| **Build** | **Clean — measured here.** |
| **Clippy** | **Clean, zero warnings — measured here.** |
| **Tests** | **All green — measured here.** |
| **Verification family** | **None, until now.** No lane, no verifier, and no challenger ever ran anything on this branch. |
| **Verdict** | **SPLIT into two commits, get one owner ruling, then merge.** |

### 4.1 The three unknowns are now answered

`e97743a`'s message says the lane that wrote it ended while waiting on a background build and
never learned whether the workspace builds, whether clippy is clean, or whether any test passes.
Those three questions are answerable by running them, so they were run here — the commit
cherry-picked onto develop at `2f6e1d6` in an isolated worktree:

| Check | Result |
|---|---|
| `cargo build --workspace --offline` | exit 0 |
| `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` | **exit 0** |
| `cargo test -p plinth-hollow --offline` | exit 0 — 138 passed, 0 failed, 5 ignored |
| `cargo test -p plinth-pipeline --offline` | exit 0 — 83 passed, 0 failed |

All three tests the commit adds or rewrites ran and passed, none silently skipped:
`shell_topology_refuses_a_component_within_the_degenerate_volume_noise_floor`,
`voxel_admission_has_exact_boundary_and_refuses_an_oversized_grid_before_voxelization`, and
`printable_envelope_is_gated_per_axis_against_the_registered_printer`.

The throwaway branch was deleted and the working tree confirmed clean. **The commit's own
"WHAT IS UNKNOWN" section is now obsolete, and the commit cannot merge carrying it** — its
message says in plain words that it must not be merged.

### 4.2 The design question **is** answerable from the code. Here is the answer.

The commit asks: `validate_printable_bounds` now compares the mesh's X/Y/Z extents against the
printer's X/Y/Z build volume axis by axis, which is correct only if the mesh is already in the
printer's frame. Establish which frame it is in.

**Answer: it is in the printer's frame, on all three axes, and the comparison pairs the right
axes.** Four facts settle it, each checkable:

1. **The gate is unreachable before orientation on the production path.** `plinth_hollow::run`
   takes a `plinth_orient::OrientedModel` — a type only the orient stage constructs. The
   pipeline order in `crates/plinth-pipeline/src/prepare.rs` is decimate → orient → hollow, and
   the private function holding the second call site is named `validate_oriented_solid`.
2. **Z is pinned exactly.** `UpDirection::rotation_to_build_frame`
   (`crates/plinth-orient/src/score.rs:160`) builds the shortest-arc rotation carrying the
   chosen up-direction to `+ẑ`. Its own doc states "Row 2 is `u` by construction, so
   `(R·v)_z = v·u`". After orientation the model's Z extent is its height on the plate.
3. **The registry declares the axis mapping.** `crates/plinth-core/src/constants/evidence.rs:269`
   registers the build volume as `211.68 x 118.37 x 220.0` with the note "The vendor's first,
   second, and third dimensions map to project X, Y, and Z". Printer frame and project frame are
   the same frame, by declaration.
4. **Nothing rotates the mesh after orientation.** The sweep searches `UpDirection` only — a
   unit 3-vector, two degrees of freedom (`score.rs:96`, `grid.rs`). There is no yaw search, no
   nesting stage, no plate-placement stage, and no rotation of any kind in `plinth-emit` or
   `plinth-hollow`. The orientation the model leaves the orient stage with is the orientation it
   is printed in.

So the model's X extent really is what will occupy the printer's X. **The comparison is sound.**

### 4.3 But the honest form of the answer has a caveat the commit did not ask about — **NEW**

Fact 4 cuts both ways. Because no stage ever chooses a rotation about Z, the assignment of the
model's two horizontal extents to project X (211.68 mm) and project Y (118.37 mm) is a
**byproduct of the shortest-arc construction**, not a decision made against the plate. The plate
is 1.8× wider on X than on Y, so which way round a model lands matters a great deal.

Concretely: the commit's own new test asserts that a 50 × 120 × 50 mm box is refused, because
120 mm breaches the registered 118.37 mm Y axis. That box **fits the plate perfectly** turned
90° about Z — 50 ≤ 118.37 and 120 ≤ 211.68. The pipeline refuses it only because it never tries
the rotation.

**Ruling: this is not a reason to hold the branch, for one decisive reason — the identical
assumption is already live in production today.** `crates/plinth-emit/src/lib.rs:502-522`
already gates the emitted artifact per axis against the same registered profile, in the same
form (`span[0] > volume.x()`, `span[1] > volume.y()`, `span[2] > volume.z()`), plus centring
constraints. A model 120 mm on Y is already refused at emit, on develop, today.

So the hollow change does not create the behaviour. It **propagates an existing behaviour
earlier and more cheaply** — refusing before hollowing rather than after supports and drilling —
and its error message, "geometry exceeds the registered printer's build volume on axis {axis}",
is honest about what happened.

The caveat that remains is a **capability gap, not a correctness bug**: the pipeline refuses
models that would fit under a free 90° yaw it never attempts. That gap belongs on the
track-after list against `plinth-emit`, where it originates. Neither verifier raised it, and no
code anywhere says it out loud.

### 4.4 What must happen before this branch is considered

1. **Rewrite the commit message.** It currently says "It is NOT landable as it stands and must
   not be merged". Its three unknowns are answered (§4.1) and its design question is resolved
   (§4.2). The message must state what was measured and by what.
2. **Split it into two commits.** `e97743a` bundles two unrelated changes:
   - **(a) Per-axis printable bounds** — replaces one flat `PRINTABLE_MAX_EXTENT_MM = 300.0` with
     a per-axis read of the registered build volume. This is a **behaviour change**: 300 mm is
     looser than the real machine on every axis, so develop today admits models that cannot be
     printed at all, and this tightens that. It needs the owner ruling in §9 D1.
   - **(b) `DEGENERATE_VOLUME_REL_TOL`** — replaces an exact `signed_volume_mm3() == 0.0`
     degenerate-component test with a relative tolerance. This is an independent correctness
     fix, and arguably the more valuable half: an accumulated tetrahedron-volume sum essentially
     never lands on exactly `0.0`, so the old test almost never fired, and worse, a degenerate
     component's classification as cavity or body was decided by the sign of summation noise.

   They should be reviewed and reverted separately.
3. **Weigh (b)'s one real risk before merging it — NEW.** Making the degenerate test a tolerance
   makes the gate **stricter**: components that used to pass can now be refused. The consequence
   is not small. `HollowError::InvalidShellTopology` propagates out of `plinth_hollow::run` and
   lands on the oriented-solid reserve (`prepare.rs:1694`) — **the model prints solid.** And
   `prepare.rs:233` records that this is the exact path the Weeping Treant used to take, as
   FU-34 and then FU-49, until the 2026-08-11 repair made it hollow. A false refusal here
   silently gives that win back.

   **Having said that, the risk is bounded and small.** The tolerance is
   `1e-9 × source_volume`, so for a 10,000 mm³ model it is 1e-5 mm³ — a cube about 0.02 mm on a
   side, below the printer's own resolution and far below the 0.05 mm minimum tip radius the
   project already works to. Nothing printable can fall under it. **Recommendation: merge (b),
   and run the corpus once afterwards to confirm no model changes disposition.** Track, do not
   block. Neither verifier raised this, because neither verifier looked at this branch.
4. **One cosmetic defect to fix while splitting — NEW.** The new guard
   `|| !degenerate_tolerance_mm3.is_finite()` can never fire. `source_volume_mm3.abs().max(1.0)`
   returns `1.0` when the input is NaN, because Rust's `f64::max` returns the other operand when
   one is NaN, so the tolerance is always finite. It reads as protection and provides none.
   Delete it, or make it a real check on `source_volume_mm3`.

### 4.5 Recovery copies

The commit message states that recovery copies live at `.git/salvage-hollow-rescue/`. Note that
`.git/` is not carried by a clone. Now that `e97743a` exists on a branch, the commit is the
durable copy and the `.git/` copies are redundant — but nothing else that lives only under
`.git/` is safe, including the three held LAYER 1 patches (§9 D4).

---

## 5. Items outside both verifiers' scope

### 5.1 `feature/salvage-kernel-tests` — confirmed empty, and correctly so

`git rev-list --count develop..feature/salvage-kernel-tests` = **0**, and the branch is an
ancestor of develop. The work was already merged (`2333009`).

The three test files named in `HANDOFF-kernel-to-rewire.md:405-407` all exist on develop and all
pass, run here:

| File | Tests | Result |
|---|---|---|
| `rust/geom_kernel/tests/kn_degenerate_face.rs` | 7 | 7 passed, 0 failed |
| `rust/geom_kernel/tests/kn_point_nearest_segment_batch.rs` | 6 | 6 passed, 0 failed |
| `rust/geom_kernel/tests/kn_segment_clearance_unmeasured.rs` | 6 | 6 passed, 0 failed |

19 of 19 pass, and the counts match the handoff's own table exactly. **Close the branch and
delete it.** An empty branch left standing invites someone to look for missing work that is not
missing.

### 5.2 The orphaned `rewire.rs` hunk — **decline it. It needs no owner.** — **NEW ruling**

The support verifier escalated this to the orchestrator: `crates/plinth-support/src/rewire.rs`
carries 33 insertions / 21 deletions in the salvage, is claimed by neither lane, and "needs an
owner or an explicit decline."

**It gets the explicit decline, and the evidence is unambiguous.** The hunk contains three
things, and all three are settled:

1. **Removing the `RewireError::ConstantMismatch` gate** — a precondition that refused to run at
   all unless five registered constants bit-matched hard-coded literals. Correct intent, and
   **already on develop**: `ConstantMismatch` no longer exists in the file, retired by `242587e`
   and replaced by `Config::check_usable` (`rewire.rs:186`).
2. **A guard in `try_retire`** — `if after >= before - VOLUME_EPS_ML { return Ok(None); }`,
   declining a retirement that does not shrink the forest. **develop already answered this
   question, and answered it the other way, deliberately, with a test that explains why.**
   develop's `try_retire` returns a `MoveOutcome`, commits on legality, and reports a **signed
   delta** (`rewire.rs:997`). The regression test at `rewire.rs:2400` states the reasoning:
   retiring a degenerate junction re-anchors its children and widens the ancestors they feed,
   which on its fixture costs about 78% of the forest's volume — "A volume test would therefore
   DECLINE this repair, which is why `try_retire` commits on legality instead." And declining is
   not the safe option: `rewire.rs:947` records that a degenerate root left standing is a plate
   root whose run to the plate is shorter than `root_boss_top` allows, "which fails lowering
   outright." **The salvage's guard would decline exactly the repair develop proves must be
   committed.** It is not unowned work. It is superseded and wrong.
3. **A leftover `eprintln!("DEBUG try_retire before={before:.12} ...")`** in production code.
   This alone disqualifies the hunk from landing in any form. There are no `eprintln!` calls
   anywhere in `crates/plinth-support/src/` on develop today.

Nothing from this hunk should be ported. This closes the one item the support verifier could not
close, and it removes an owner decision from the queue rather than adding one.

---

## 6. Must fix before merge

Short, and deliberately so. Everything not on this list is on §7.

| # | Branch | What | Why it blocks |
|---|---|---|---|
| **M1** | `salvage-emit` | Correct `ae57863`'s commit message **and** the test's doc comment, so neither claims `write()` is covered. Amending is sufficient; rewriting the test to drive `write()` is better. | A git-log entry asserting coverage that does not exist retires the question permanently. The next person to ask whether the durability path is covered will find this commit and stop looking. This is a verification defect, and verification defects climb the stack. |
| **M2** | `salvage-hollow` | Rewrite `e97743a`'s message. Remove the "WHAT IS UNKNOWN" section and the "must not be merged" instruction; record what was measured and by what. | The commit currently instructs the reader not to merge it. |
| **M3** | `salvage-hollow` | Split `e97743a` into the per-axis bounds change and the degenerate-tolerance change. | They are unrelated, carry different risks, and one needs an owner ruling the other does not. Bundled, they cannot be reverted independently. |
| **M4** | `salvage-hollow` | Owner ruling on §9 D1 — does the printable gate tighten from a flat 300 mm to the real machine now? | It changes which models the pipeline accepts. |

---

## 7. Can merge and track

Each of these is live debt. Each is **recorded in code or in a commit message** rather than
buried, which is why none of them blocks. Holding a branch for any one of them costs more than
merging with it.

| # | Where | Item | Is a defect live in production? | Does the code say so? |
|---|---|---|---|---|
| **T1** | `plinth-support` (merged with #1) | Transit-screen majority-sample cap is 80, covering 9.6 mm of a 22.5 mm reachable span. The derived value is 188. | **Yes.** Crossings between 9.6 mm and 22.5 mm are under-sampled today. | **Yes, well.** The literal is named `TRANSIT_SCREEN_MAJORITY_SAMPLE_CAP`, its doc states `80 × 0.12 = 9.6 mm` against the 22.5 mm span and says plainly that it is under-sized, and a test asserts both numbers. |
| **T2** | `plinth-support` (merged with #1) | `gate_work_per_batch_unit` and `load_work_per_batch_unit` classified `UnboundPendingRuling`. Two limit sets that refuse different plans publish the same source-authority digest. | **Yes.** | **Yes.** The mechanism to fix it landed and two tests assert the defect by name. Binding them would move the `make_toed_slab` fingerprint already under an unresolved ruling (D2). |
| **T3** | `plinth-support` (merged with #1) | Five dyadic `CandidatePayloadConfig` numbers have no registered owner; left as literals. | Latent. | **Yes**, recorded in `093c947`. |
| **T4** | `plinth-support` (merged with #1) | Cap-preflight invariant narrowed; 8 bytes per coverage sample now reserved ahead of the cap. | No — the reservation is `try_reserve_exact` mapped to a refusal, so it is fail-closed. | **Yes**, stated in `8e9f0c6` rather than hidden. |
| **T5** | `plinth-support` (merged with #1) | B1, B3, A1, S3, S1 — five salvage items deliberately not landed. | B1 is T1. A1 leaves open whether a seated anchor arm is owed a minimum length of its own (D3). The rest are latent or would have made things worse. | **Yes**, each declined in a commit message with its reason. |
| **T6** | `plinth-emit` (pre-existing) | Per-axis build-volume gate with no yaw ever chosen: models that fit at 90° are refused. §4.3. | **Yes**, today, on develop, at `lib.rs:502-522`. | **No.** Nothing says the pipeline never rotates about Z or that this is why a fitting model is refused. **This is the one item on this table the code does not disclose.** |
| **T7** | `plinth-hollow` (with #3) | Degenerate-tolerance change makes the gate stricter on the path that ends in a solid print. §4.4.3. | Not yet — unmeasured against the corpus. | Partly. The constant's doc explains its role well but does not name the solid-print consequence. |
| **T8** | `plinth-emit` (with #2) | `write()`'s durability contract still has no test that drives `write()`. | No — `write()` behaves correctly. Only the coverage is absent. | Only once M1 is done. That is what M1 is for. |
| **T9** | develop (pre-existing) | `closure_repairs_censored_dust…` red, and the uncapped repair search behind it. | **Yes** — a live support-planning defect, and the search cost 59 CPU-hours in one orphaned run for no artifact. | **Yes**, `FIX-AFTER-RUN.md` and `repair-cap-evidence.md`. |

---

## 8. Everything from the salvage that was not recovered

The salvage (`wip/correctness-salvage` vs `bdab3aa`) held 22 files, +4,609 / −472. Accounting for
all of it:

| Area | Status | Live defect today? | Does the code say so? |
|---|---|---|---|
| `plinth-support` × 7 src files | **Recovered** — branches `-core` + `-consumers` | See §7 T1–T5 | Yes |
| `plinth-support/src/rewire.rs` | **Declined** — §5.2 | No. Both intents are settled on develop, one of them the other way round on purpose. | Yes — `rewire.rs:947`, `:997`, `:2400` |
| `plinth-emit/src/lib.rs` | **Recovered** — branch `salvage-emit`; most of it was already on develop in a different shape | No | Yes |
| `rust/chitubox_io/` (project.rs, durable_rename.rs) | **Already on develop** (`ad335d9`). `durable_rename.rs` confirmed present here. | No | Yes |
| `rust/geom_kernel/` × 6 files incl. 3 new test files | **Already on develop** (`2333009`). All three test files confirmed present and passing (§5.1). | No | Yes |
| `plinth-hollow/src/lib.rs` | **Recovered** — branch `salvage-hollow`, WIP, §4 | The flat 300 mm gate is looser than the machine on all three axes, so develop admits unprintable models today. This branch is the fix. | Only after §4 lands. Today the message says "H0's printable 300mm extent", which states the number without saying it is wrong. |
| **`plinth-hollow/src/drilling.rs` — the drain-banding fix** | **NOT RECOVERED. The one genuine loss.** | **Yes.** This is remediation short-list item 3, one of the two hollowing defects that make limbs print solid. | **Yes, honestly and in detail.** `STATUS.md:55` lists it as missing by name; `STATUS.md:198-200` records that it is saved standalone at `docs/evidence/remediation-2026-08-26/patches/LOST-drain-banding-drilling.patch` and "is worth reviving"; `STATUS.md:271` makes recovering it recommendation 4, and notes that two regression tests are already written and waiting for it at `crates/plinth-hollow/tests/hollow_chain_end_to_end.rs:461` and `:583`. |

The per-chamber drain report from the same file **was** recovered separately (`a4deda4`);
`shipped_chambers` is on develop at `drilling.rs:19` and `:436`. Do not let that create the
impression that `drilling.rs` came through intact. It did not.

**Also still missing, and outside the salvage branches entirely** — short-list items 4, 7, 11
and 12 (the refill fixpoint, the hole-aware island test, the near-wall question, the uncapped
repair search) and the host lookup rebuild. All are recorded in `STATUS.md`. None of them is a
salvage-recovery question; they were never attempted or were reverted.

---

## 9. Owner decisions this work is waiting on

In plain language, for a reader who has not read the code. Ordered by what they unblock.

**D1. Should the size check match the real printer now?**
The stage that hollows a model currently refuses anything bigger than 300 mm in any direction.
The actual printer is 211.68 × 118.37 × 220 mm. So today the software accepts models it cannot
print, and only notices much later. The hollow branch fixes that. The catch is that it will
start refusing models that pass today — earlier, and with a clearer message, but refusing.
Nobody has run the model corpus to see whether any real model is affected. **The decision: turn
the accurate check on now and accept that some models may start refusing, or run the corpus
first and decide with the number in hand.** Recommendation: run the corpus first. It is cheap
next to a surprise refusal.

**D2. Two frozen numbers moved. Re-record them, or investigate?**
Two tests compare a "fingerprint" — a single number summarising a whole computed result — and
both now differ from what was recorded. The size of the result did not change (still 64 and
170); only the summary number did, because a change made a decision explicit instead of relying
on two floating-point values being exactly equal. This looks like an expected consequence.
**The decision: accept the new numbers and update the two tests, or ask someone to prove the
change is harmless first.** These two failures are blocking nothing, but they make the test
suite permanently red, which trains people to ignore red.

**D3. Should a support arm that lands on the model be required to have a minimum length?**
When a support arm reaches out and touches the model, the software checks the arm's length
before deciding where it lands, but not after. Making it check afterwards too broke eight
existing tests, and every model support the corpus has accepted has an arm well under the
minimum being applied. So either the minimum is the wrong rule for this case, or the corpus has
been accepting arms it should not. **The decision: is the minimum length a rule about which
candidates to consider, or a rule about what is acceptable once placed?** The branch takes the
first reading and says so in its commit message. This is a physics question, not a code
question.

**D4. Three fixes are written but held, waiting on you.**
They are saved as patches under `.git/salvage-l1-held-2026-08-28/` — P2 (`footprint_seed_xy_mm`),
P3 (incident-face exterior filter), C2 (`nearest_hosts`). Each changes a rule about what the
software accepts, which the lane that wrote them was not authorised to decide. **P2 should be
ruled on first: it unblocks a corpus model that is currently refused.** Note that `.git/` is not
copied by a clone, so these three patches exist on one machine only.

**D5. The support planner has a known defect that costs whole runs.**
One test fails on purpose to report it: a repair search with no upper bound. In one orphaned
production run it burned 59 CPU hours over 6 hours 45 minutes, committed one repair out of 224
trials, and wrote no file at all. **The decision: cap the search, or keep accepting that a run
can fail to finish.** This is the largest single cost in the test suite — 874 seconds in one
test against 184 seconds for everything else.

**D6. The lost drain-banding fix — revive it or write it off?**
One genuinely lost piece of work, saved as a patch at
`docs/evidence/remediation-2026-08-26/patches/LOST-drain-banding-drilling.patch`. It is one of
the two defects that make limbs print solid. Two tests are already written and waiting for it.
**The decision: schedule the revival, or record the defect as accepted and stop listing it as
pending.** Leaving it listed but unscheduled is the worst of the three.

**D7. Should the pipeline try turning a model to make it fit?**
Nothing in the pipeline ever rotates a model about the vertical axis. So a model 150 mm across
is refused for being too deep, even though it would fit if turned a quarter turn — which is free
and changes nothing else about the print. This is a missing capability, not a bug, and no code
anywhere says it out loud. **The decision: add the quarter-turn check, or write down that the
pipeline does not do this and why.** Recommendation: write it down now; build it later.

**D8. The cross-family review did not happen. Do we still want it?**
This document exists because the same model family both wrote and graded this work. The outside
challenge came back empty. Reconciling it here found five things two earlier readers missed,
which suggests an outside challenge would still find something. **The decision: merge on
same-family verification plus this reconciliation, or hold `feature/salvage-support-consumers`
for a genuine outside reader.** Recommendation: merge. The mechanical evidence is strong and
reproducible, the branch's own tests were proved to have teeth by sabotage, and the cost of
holding 14 correctness commits exceeds the residual risk. But it is a judgement, and it is
yours.

---

## Appendix — what was run here

Against `develop` at `2f6e1d6`, in an isolated worktree.

```
# feature/salvage-hollow (e97743a cherry-picked onto develop; branch deleted afterwards)
cargo build --workspace --offline                                              -> 0
cargo clippy --workspace --all-targets --all-features --offline -- -D warnings -> 0
cargo test -p plinth-hollow --offline    -> 0   (138 passed, 0 failed, 5 ignored)
cargo test -p plinth-pipeline --offline  -> 0   ( 83 passed, 0 failed)

# develop, for feature/salvage-kernel-tests
cargo test -p geom_kernel --offline \
  --test kn_segment_clearance_unmeasured \
  --test kn_point_nearest_segment_batch \
  --test kn_degenerate_face             -> 0   ( 19 passed, 0 failed)
```

No production code was sabotaged here. No branch was merged, pushed, or modified. The temporary
branch used for the hollow measurements was deleted and the working tree confirmed clean.

Build, clippy and test results for `feature/salvage-support-consumers` and
`feature/salvage-emit` are the Claude verifiers' own, re-used deliberately rather than re-run;
the checks performed here on those two branches were the structural ones the verdicts rest on
(`git diff --stat` for the frozen-expectation question, and reading the disputed emit test).
