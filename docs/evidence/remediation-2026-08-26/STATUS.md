# Remediation status — where we stand against the plan

Measured 2026-08-27 against `develop` at `cbc9711`, whose merge base with the pre-remediation
tree is `041ba6d` (2026-08-25 21:33). All paths are relative to the repository root.

Range under discussion: `git log --oneline 041ba6d..develop` — 34 commits.
`git diff --shortstat 041ba6d..develop` — 195 files changed, +15,023 / −18,911. Split by area:
documentation +6,522 / −23; code under `crates/` and `rust/` +8,416 / −18,876.

Verified by me today, in the foreground: `cargo clippy --workspace --all-targets --all-features
--offline -- -D warnings` exits 0 with zero warnings, which is the gate `docs/development.md:39`
documents. `cargo fmt --all -- --check` reports 7 hunks against the documented ceiling of 104.
I did not run the full test suite; see section 3 for why and what would settle it.

---

## 1. Where we stand

Of the plan's eight phases, two are done, four are partly done, and two have not started.
Phase 0 (make change safe) and Phase 2 (delete what nothing runs) are complete. Phase 1 (free
wins) landed ten of its fourteen items. Phase 3 (give each fact one home) is roughly a quarter
done — the mechanical de-duplication inside the support engine landed, but the one item the plan
called its highest-leverage change, making the Rust compiler refuse an unmeasured number on any
accept-or-reject path, has not been started at all. Phase 4 (correctness) is the one that was
attempted and mostly lost: twelve parallel work streams ran in a single shared checkout, eleven
were destroyed by the resulting write race, and four have since been recovered and merged.
Phase 6 (documentation) has only the cheap numeric corrections that other commits swept up in
passing. Phases 5 (vocabulary) and 7 (architecture) have not started at all.

Against the state the review described, the codebase is meaningfully better in three specific
ways and unchanged in most others. It now has lint configuration where it had none, so roughly
1,300 real warnings that were invisible are now zero and stay zero (`92a1be1`). It has its first
test that drives a model through the entire hollowing chain, on four synthetic fixtures small
enough to live in git, where previously the last assertion in the suite stopped before the three
riskiest stages (`f3e2258`, `crates/plinth-hollow/tests/hollow_chain_end_to_end.rs`). And about
14,000 lines of code with no production caller are gone, along with eleven findings that went
with them (`3913d50`, `5d0420e`). Set against that: every one of the twelve items on the review's
short list is still open except its second, a one-token index fix (`a929cea`), and the disclosure
half of its first (`c9a9e4d`) — the two hollowing defects, the contact-set replay, the
island-birth test and the uncapped repair search are all untouched. The pre-write safety gate
still runs one of its twelve checks; it just now says so out loud. And deleting the auditing
crate removed the project's only independent check of its own output, with nothing yet replacing
it.

---

## 2. The ledger

| Phase | What it was for | Status | Evidence |
|---|---|---|---|
| **0 — Make change safe** | Lint configuration, committed test fixtures, an end-to-end hollowing test, and a findings tracker, so later phases could be proven rather than believed | **DONE** | Lints `13d4686` then rewritten `92a1be1` (gate verified at 0 warnings today); fixtures `aa67c1b` (`crates/plinth-hollow/tests/fixtures/`, 4 STLs, 684–3,684 bytes); chain test `f3e2258`; tracker `0cf9d7a` (`docs/remediation-tracker.md`) |
| **1 — Free wins** | ~30 findings that cannot change behaviour for the worse, starting with the false "unused code" labels that a deletion pass would have been misled by | **PARTIAL** | 10 of the tracker's 14 Phase-1 rows landed: `fb4fe23` (false labels), `a929cea` (the review's most direct defect), `c9a9e4d` (gate disclosure), `dc2e0d0` (kernel modules opened), `2d41082`, `cdb61bd`. **Missing:** T3.3-1 (anchor re-qualification, `crates/plinth-support/src/anchor.rs:1193`), T3.7-1 (two projection paths), T3.10-1 (radius parameter naming), T3.12-1 (the "kd-tree" that is a linear scan — `KDTREE_REBUILD_EVERY` is still at `crates/plinth-support/src/cco.rs:39`) |
| **2 — Delete what nothing runs** | Remove ~13,000 lines with no production caller; keep the correctly-named unit-conversion constant before deleting the crate that held it | **DONE** | `367c2af` (constant moved first, as the plan required), `3913d50` (`crates/plinth-metrics` gone, 12,751 lines), `5d0420e` (`transit_measurement.rs` gone, 3,788 lines; the designed-mask table gone, 997 lines from `designed_embed.rs`), `dc2e0d0` (test toolkit put behind a build flag). Three of the nine rows resolved as **keep, with a recorded reason**: `bdab3aa`, `c3a009b`, `FIX-AFTER-RUN.md:§2b` |
| **3 — One home per fact** | Delete duplicated geometry helpers, consolidate the four ray-hits-triangle implementations, extract one function per physical law, and make the compiler refuse an unmeasured number on any accept path | **PARTIAL** | 8 of 32 rows landed, all mechanical: `2d41082` (hash fold, route-point gather, plate-topology predicate), `cdb61bd` (rounding), `3913d50` (two by deletion), `242587e` (registry no longer overridden by a literal in `rewire.rs`). **Missing:** the whole of Group D — `sanctioned_search_read` does not exist anywhere in the tree, and `AcceptSafe` is still a bound in exactly one production signature (`crates/plinth-core/src/layer.rs:28`) against 382 unrestricted `.diagnostic()` calls. Also missing: the tapered-strut volume law still has three owners, and the third hand-written ray/triangle routine is still at `crates/plinth-mesh/src/triage.rs` |
| **4 — Correctness** | All 15 named correctness items, including the two hollowing defects that make limbs print solid, and the host lookup rebuild | **PARTIAL** | Attempted with twelve parallel streams; eleven destroyed (`623efec`, preserved and does not compile). Four recovered and merged since: the geometry kernel's typed "not measured" answers (`2333009`), durable file renames (`ad335d9`), the caller half of the clearance fix plus three more defects in `rewire.rs` (`242587e`), and the shipped report naming each chamber (`a4deda4`). **Missing:** short-list items 3, 4, 5, 7, 11 and 12 — the drain-banding fix, the refill fixpoint, the contact-set replay, the hole-aware island test, the near-wall question and the uncapped repair search — plus the host lookup rebuild (4.2) and about 100 further rows |
| **5 — Vocabulary** | Stop "floor" meaning "smallest allowed value" in nine modules where it also means the bottom of a printed part | **NOT STARTED** | `grep -rho floor crates/ rust/ --include="*.rs"` returns 532 occurrences across 51 files, unchanged in substance; `patch_area_floor_mm2` is still the public accessor name at `crates/plinth-support/src/coverage_measurement.rs` |
| **6 — Documentation** | Rewrite crate docs to describe only the present; fix the numeric claims that disagree with the code | **PARTIAL** | The cheap numeric fixes landed opportunistically inside `cdb61bd` (registry count 287→306, traversal cost 13→12, two dead citations) and `c3a009b` corrected `docs/architecture.md` and `docs/status.md` after the crate deletion. The crate-doc rewrite (6.1), the retracted-paragraph strip (6.2) and the kernel-tolerance provenance (6.6) are untouched |
| **7 — Architecture** | Port two real safety gates, split the stage-handoff types out, lift the misplaced general facilities, and schedule the remaining nine gates | **NOT STARTED** | `crates/plinth-pipeline/src/gates.rs` is unchanged except for 5 lines of lint annotation; eleven of twelve gates still return "not yet ported". Disclosure landed (`c9a9e4d`) but that was Phase 1.3, not this |

Two items in the plan's own framing are now **ABANDONED** rather than pending. The Blender-based
fixture generator was committed (`aa67c1b`) and then removed (`fb620bc`) because the repository
has a guard that fails the build on any Python file, enforcing the 2026-07-28 decision. Its
reasoning and measurements survive as prose in
`crates/plinth-hollow/tests/fixtures/PROVENANCE.md`, but the fixtures can no longer be
regenerated from the tree. And the plan's mitigation for deleting the auditing crate — port two
gates that judge recorded evidence rather than re-run the engine — was never started, so the
plan's own stated consequence stands unmitigated: the project has no independent check of its own
output, and `c3a009b` records a second casualty, that the ratified `SCAR-AREA-CAP` specification
names two implementations that must agree and both are now gone.

---

## 3. Findings accounting

**The tracker is stale.** `docs/remediation-tracker.md` has a Status column and it is empty on
every one of its 189 rows. It was written at `0cf9d7a` and updated once at `caec601`, both before
any fix landed, and no lane ever wrote back to it. It also disagrees with the review about the
denominator — the review's own method section says 203 findings survived verification
(`REVIEW.md:926`), the tracker's summary says systematic extraction yields 189 deduplicated
entries, and the tracker's own extraction contains at least three internal duplicates (the raft
pad/bridge dispatch is both T3.2-19 and T3.12-11; the test toolkit is both T3.6-13 and T3.6-16;
the 32,768-face threshold is both T3.2-18 and T3.12-10).

**My count, and the method.** I read every commit message in `041ba6d..develop` and matched the
work each one claims against the tracker's rows by file, line and description, then spot-checked
the tree with `grep` for the ones I could verify mechanically. A row counts as fixed only if I
can name the commit that fixed it.

| | Count | Basis |
|---|---:|---|
| Fixed, each attributable to a named commit | **55** | Enumerated below; 57 rows minus the 2 the tracker records twice |
| Deferred with a recorded, written reason | **7** | T3.6-10, T3.6-11, T3.6-12, T3.12-7, SL-3, SL-11, SL-12 |
| Untouched | **127** | 189 − 55 − 7 |

The 55, by phase: Phase 0 — SL-0.3, T3.8-1. Phase 1 — SL-2, SL-1-disc, SL-10, T1.4, T3.2-1,
T3.2-2, T3.4-1, T3.6-1, T3.6-2, T3.6-3. Phase 2 — T3.6-4 through T3.6-9. Phase 3 — T3.7-6,
T3.7-7, T3.7-9, T3.7-11, T3.7-13, T3.7-14, T3.7-16, T3.2-13. Phase 4 block — T3.3-4, T3.3-8,
T3.4-4, T3.4-5, T3.4-10, T3.5-9, T3.5-26, T3.5-35, T3.5-36, T3.6-13, T3.8-9, T3.8-11, T3.9-8,
T3.10-2, T3.10-9, T3.11-4, T3.11-5, T3.11-6, T3.12-2, T3.12-4, T3.12-5, T3.12-9, T3.12-11.
Backlog — T3.2-19, T3.3-9, T3.3-10, T3.3-11, T3.5-38, T3.6-15, T3.6-16, T3.10-16.

Eleven of the 55 were resolved by deleting the code they lived in rather than by fixing anything
(`3913d50`, `5d0420e`). One of the 55 is only partly closed: T3.6-15 made the vendored-dependency
checksum file actually verify (`cdb61bd`), but the verifier walks the manifest rather than the
tree, so deleting a manifest row still silently exempts a file — recorded at `FIX-AFTER-RUN.md:§3`.

**Error bar, stated honestly.** The uncertainty is concentrated in the 112-row Phase 4 block,
where several rows describe an inefficiency in a file that was edited for an unrelated reason. I
resolved those against the row's specific claim, not against whether the file changed. If you want
a number you can audit rather than one you have to trust, the tracker needs a Status column filled
in per row with its commit — that is a mechanical afternoon's work and it is the thing that would
settle this.

**Test state at the tip is unknown.** The recorded baseline of 84 binaries / 1,465 passing is from
`2333009`, four merges ago. I did not reproduce it, because `cargo test --workspace` does not
terminate on this tree: `support_runner_integration`'s coverage-repair search is unbounded and was
observed at 887 trials over 47 minutes without finishing (`FIX-AFTER-RUN.md`, "NOT damage"
section). What would settle it is a per-binary run with that one binary bounded by `gtimeout` —
the harness for exactly that is committed at
`docs/evidence/remediation-2026-08-26/baseline.sh`. Note its known trap: running a test binary
directly with `--test-threads=4` breaks the two compile-failure tests, which pass under plain
`cargo test`.

---

## 4. What the remediation found that the review did not

The review read the code and ran nothing — it says so at `REVIEW.md:990`, "Nothing was executed."
Four things surfaced only because the work was done.

**The nine-test cluster, and its cause.** Nine tests had been failing since 2026-08-21 and nobody
had diagnosed them. A bisect over 57 commits, six builds, found one cause: commit `3a6526f` fixed
a clearance mask, which let a support family route for the first time, which meant the two
fixtures chosen precisely *because* their support layout did not cover now cover on the first
attempt. The tests were not wrong; they had lost their subject
(`baseline-red-diagnosis.md:1-40`). What this cost is the important part: every assertion in the
workspace that a coverage repair was ever *committed* lived in those nine tests, so for five days
the repair-commit path had no passing test at all, and the review — which was reading, not
running — could not see that. Closed by building a synthetic model that genuinely needs repair
(`f28164e`, `plinth_mesh::make_uncovered_rim_table`), with the fix proved to have teeth by
suppressing the commit path and watching all eight restored tests fail.

**A sub-voxel cavity-reconstruction defect, caught by a fixture built for something else.** The
`thin-fin.stl` fixture was built to settle an open question about a 0.05 mm tolerance in the
support-foot breach test. Pushing it through the hollowing stage instead produced a refusal:
the hollowing pass carves cavities on a 0.5 mm voxel grid, the fin is 0.02 mm thick — 25 times
below one voxel — and the surface reconstruction of the carved cavity comes back with a spurious
extra closed surface, so drilling refuses with `SourceSurfaceCount { found: 3, expected: 2 }`
(`crates/plinth-hollow/tests/fixtures/PROVENANCE.md:243`). This is not in the review at all. It
means any model carrying a genuinely thin feature — a blade edge, a cloak hem, a wing membrane —
can be refused for a reason nothing in the codebase explains. It is now a regression test written
ahead of its fix, at `crates/plinth-hollow/tests/hollow_chain_end_to_end.rs:583`. Cost so far:
nothing, because it was caught before a user hit it.

**The repair search's regression rate, and what "regression" actually means.** An orphaned
production run left 225 lines of progress output. Reading them (`repair-regression-analysis.md`)
overturned the obvious interpretation. The counter labelled `regression` does not mean the trial
made things worse — the accept test returns the *first* failing clause, and that clause is
unreachable unless coverage strictly improved. So 119 of 224 trials found a real improvement and
threw it away, vetoed by a second rule the search does not model, cannot predict and learns
nothing from. The outcomes are clustered, not random (runs test z = −2.43; one streak of 21
consecutive vetoes costing about 37 minutes), and 238 trials across the four committed test
fixtures produce zero of this class — the rule that consumes the real model's entire run has no
test coverage whatsoever. The review had flagged the per-trial measurement as a cost concern and
ranked it last; measurement shows the cost is the six-stage pipeline rebuild the search performs
for a plan that differs by one row, at ~105 s each, and that fixing the item the review named
would not have saved the run.

**Two defects in the fix for the kernel defects.** The stream fixing the geometry kernel's
"not measured" sentinel found two defects in its own landed implementation. The first is the one
that matters: the fix replaced a floating-point infinity with a proper type, but modelled only
two of the three ways a measurement can fail to happen. The third — every candidate face excluded
by the caller's own exemption set — still came back as `Measured { INFINITY }`, a type asserting a
measurement that never occurred. That is strictly worse than the original defect, because a
caller misreading a float is a bug you can find, and a caller correctly reading a type that lies
is not. Both were fixed before shipping (`2333009`), and the accompanying test was rewritten so
that if it ever passes unchanged again, the hole is back. What this cost: nothing, because the
stream audited itself. What it would have cost is a load-bearing lie in a clearance safety check.

A fifth item is worth recording as a process finding. A note in `FIX-AFTER-RUN.md:§5` recorded
both of those defects as still open, written from an earlier report without re-checking the tree.
They had already been fixed. The section is struck through rather than deleted, so the correction
is legible: a stale "still broken" note costs as much as a stale "already fixed" one.

---

## 5. What it cost

**Eleven of twelve correctness work streams were destroyed and are preserved unusable.** Twelve
agents were partitioned by file and run in one shared checkout. That fails two ways and both
happened: file partitioning does not survive a change that legitimately crosses files, and it
does not survive two agents accidentally running the same brief. A struct gained a field its
constructors in other files never learned about; one stream's edits to the drain planner were
silently reverted underneath it by a stale write from its own duplicate, leaving the drilling
half of a properly reasoned fix referencing functions that no longer exist. The result is commit
`623efec` on branch `wip/correctness-salvage` — about 3,400 added lines across 17 files plus four
new test files — which does not compile and was preserved rather than repaired. Four streams have
since been recovered and merged (`2333009`, `ad335d9`, `242587e`, `a4deda4`); the drain-banding
fix is saved standalone at
`docs/evidence/remediation-2026-08-26/patches/LOST-drain-banding-drilling.patch` and is worth
reviving; the rest is recovered by reading, not by cherry-picking.

The duplicate agents had a specific mechanism, and it is worth naming because it will recur:
replying to a work stream with a message *resumes* it, so answering a question from a stream that
a workflow was still running produced two agents with identical briefs in one directory. It
happened twice in one day (`FIX-AFTER-RUN.md:§6`).

**An orphaned production run burned 59 CPU-hours and produced no artifact.** It ran 6 hours 45
minutes unsupervised (24,279,530 ms of elapsed time on its own last progress line), completed 224
repair trials, committed 1, and wrote nothing — because the per-trial record only reaches disk at
a successful seal or a refusal, and the loop reached neither
(`EVIDENCE-treant-runaway.log`). The log is now the single most useful piece of evidence the
remediation produced, which is a consolation and not a justification.

**Three fixes were wrong and had to be reverted, and one more was refused before it landed.**
A command-line flag was deleted as dead surface; it is dead to the compiler and load-bearing for
a documented CI procedure that names it as its single permitted spelling, and it was restored
(`bdab3aa`, cause at `docs/engineering-journal/06-conventions.md:473-478`). The lint configuration
enabled two open lint groups wholesale, which raised 4,256 warnings and made the branch unable to
satisfy its own documented zero-warning gate; it was replaced with individually named lints
(`92a1be1`). The fixture generator was written in Python against a repository guard that fails the
build on any Python file, and was removed (`fb620bc`). A fourth instruction — delete four disabled
configuration constants — was refused by the agent that received it, correctly: they are the
default-off state of a documented set of five under a binding ruling, and deleting four of five
would have broken it asymmetrically (`FIX-AFTER-RUN.md:§2b`). Two of the three bad instructions
were the orchestrator's, and are recorded as such. Separately, one merged stream split an
enumeration without running the whole suite and turned a second test red on merge; the cause was
found by experiment and the golden value re-derived through the test's own producer rather than
hand-edited (`15e4a6f`).

The pattern behind all four: *"nothing reads this"* is a fact about the compiler, not a licence to
delete. Every one of them would have been caught by grepping the documentation tree for the name
before removing the surface.

**The durable output is the process lesson, and it is one line.** File-level partitioning does not
survive cross-file changes or duplicate agents. One git worktree per work stream — a separate
checked-out directory sharing one repository — is what the next attempt of this shape needs. It is
recorded in the salvage commit itself so it travels with the wreckage.

---

## 6. What to do next

The plan's ordering was written before any of this was measured. Three parts of it are now
wrong, and they are named in place below.

1. **Make the coverage-repair search's per-trial record survive a killed run.** Put the name of
   the rejecting rule and the `own_contact_condemned` flag on the progress line the loop already
   prints unconditionally. This is disclosure only: no accept rule changes, no constant is
   introduced, nothing decides an outcome. It is the smallest change on this list and it unblocks
   everything else about that search, because today the evidence exists only in memory and only a
   run that terminates writes it (`repair-regression-analysis.md:§6`).

2. **Then run the flagship model for 45 supervised minutes, bounded by `gtimeout`, and read the
   answer.** With step 1 in place that single run settles whether the veto is punishing candidates
   for their own failure to route — in which case the accept rule is the fix site — or whether the
   search simply cannot learn which neighbourhoods cost it a terminal. Either way the ruling
   becomes possible on 45 minutes instead of the 107 days the current search would need.

3. **Build the missing fixture: a model where a repair contact displaces an already-routed
   terminal.** 238 trials across the four committed fixtures produce zero instances of the class
   that consumed the entire real run. Until that fixture exists, any change to the search is
   unprovable and any regression in it is undetectable.

   *Steps 1–3 are where the plan is most wrong.* It folded the repair search into Phase 7 as an
   efficiency item, on the review's judgement that without a measurement "this loop is O(n²)"
   establishes a defect but not its cost. The cost is now measured: it is the difference between
   the project's flagship model finishing and not finishing, and it is the largest single item in
   the test suite (874 s in one test, against 184 s for everything else — `build-cost.md:§1`).

4. **Recover the drain-banding fix from the salvage patch, and finish the two hollowing defects.**
   The fixtures and the end-to-end test now exist, which is exactly the precondition the plan set
   for this work, and two regression tests are already written and waiting on it
   (`crates/plinth-hollow/tests/hollow_chain_end_to_end.rs:461` and `:583`). This is the plan's
   Phase 4.3 and 4.4 and its ordering here was right.

5. **Rebuild the host lookup as a real spatial index.** Small, self-contained, three findings in
   one fix, and the dependency it needs is already in the workspace. Its current form lets a
   performance-tuning interval decide which supports get built, because entries added since the
   last rebuild are ranked by a different measurement than older ones. This was Phase 4.2 and it
   is still cheap; nothing has changed to make it harder.

6. **Then Phase 3 Group D — make the compiler refuse an unmeasured number on any accept path.**
   The plan called this its highest-leverage change and I agree with the reasoning, but its
   position has to move. It is a change to 382 call sites across every crate; running it now, on a
   tree where eleven correctness streams are still unmerged and the drain fix is still a patch
   file, would collide with every one of them. It should follow the correctness work, not precede
   it. *This is the plan's second wrong ordering.*

7. **Port the two safety gates that judge recorded evidence.** The plan made this mandatory rather
   than optional once the auditing crate was deleted, and then scheduled it last. Deletion has
   happened; the mitigation has not. There is now an additional reason: `c3a009b` found that a
   ratified specification names two implementations that must agree and both are gone, so the
   alignment check it mandates is not running. That needs a ruling from you, not a fix from
   anyone else. *This is the plan's third wrong ordering — Phase 7.1 is now more urgent than the
   plan's Phase 7 slot implies.*

8. **Fill in the tracker's Status column, one row, one commit hash.** Everything above is a
   judgement about 189 rows that currently carry no recorded state at all. A mechanical afternoon
   turns section 3's defensible-but-hand-built number into an auditable one.

9. **Last, and only then: vocabulary and documentation.** Phases 5 and 6 are cheap, they are
   genuinely worth doing, and they touch 53 files. Doing them before the correctness work lands
   would make every outstanding patch conflict for no gain. Nothing about them has become more
   urgent.

**Two things are unknown and neither is on the critical path.** Whether the test suite at the tip
reproduces its recorded baseline — settled by a per-binary run with the one unbounded binary
bounded, using the committed `baseline.sh`. And whether the fixtures can ever be regenerated from
the tree again — settled by deciding whether a Rust emitter is worth writing, given that
re-cutting the meshes would invalidate every measurement recorded against them
(`fb620bc`, `crates/plinth-hollow/tests/fixtures/PROVENANCE.md`).
