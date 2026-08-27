# Damage to repair after the current run finishes

Found 2026-08-27 by a `gpt-sol-high` verifier running the code-review skill over the
remediation branch. Verified independently against the repository by the orchestrator.
Two of these were caused by briefs the orchestrator wrote.

---

## 1. `--no-cache` must be restored — ORCHESTRATOR'S FAULT, highest priority

**What happened.** The deletions brief said: *"the `--no-cache` flag is parsed, threaded and
reported, and controls nothing because no cache exists. Remove the flag and its plumbing."*
An agent did exactly that, and even added a test asserting the spelling is now unknown
(`no_cache_and_its_aliases_are_all_unknown_flags_now`, plinth-cli/src/lib.rs:431).

**Why it was wrong.** `docs/engineering-journal/06-conventions.md:473-478` states:

> **Cache-disabled equality is CI-mandatory:** a run with the `plinth-pipeline` cache off
> must produce byte-identical reports to a cached run. **The cache-off mechanism is the flag
> `plinth prepare --no-cache`** — one spelling, defined here, used verbatim in M8's
> acceptance commands. There is no environment-variable form ... a second way to disable the
> cache is a second thing that can silently stop working.

And `docs/engineering-journal/04-migration-plan.md:1729-1730` runs it verbatim in the
determinism acceptance commands. `docs/evidence/emission-canary.md:81` records a run made
with it.

The review finding was accurate that the flag *controls nothing today*. It was wrong to
conclude it should be deleted: it is the single documented spelling of a CI-mandatory
procedure, deliberately kept to one spelling so it cannot silently stop working.

**Fix.** Restore `--no-cache` as an accepted flag. It stays a no-op while no cache exists,
documented as reserved for the determinism protocol. Delete the test that asserts it is
unknown, and replace it with one asserting it is ACCEPTED. Re-check
`docs/remediation-tracker.md` row T3.6-12 and mark it "keep — CI-mandated spelling".

**Lesson for future briefs:** "nothing reads this value" is not the same as "nothing depends
on this name". Grep the docs tree, not just the code, before deleting a public surface.

---

## 2. The lint configuration breaks the documented zero-warning gate — ORCHESTRATOR'S FAULT

**What happened.** The groundwork brief told the lints lane to adopt the census-drafted block:
pedantic and nursery at `warn` with the style classes allowed. Commit `13d4686` did that.

**Why it was wrong.** `docs/development.md:39` documents the gate as

    cargo clippy --workspace --all-targets --all-features --offline -- -D warnings

with an expected warning count of **0** (`development.md:68`), and
`06-conventions.md:570` requires clippy clean *before every commit*. Under `-D warnings`,
every newly enabled pedantic/nursery warning becomes an error, so the branch cannot satisfy
its own documented gate. The lane's own commit note observed the block surfaced ~990 more
warnings than the census predicted.

**Fix (orchestrator's call, override if you disagree).** Do not enable pedantic and nursery
wholesale. Enable only the classes the census proved carry real signal in a geometry
codebase — `float_cmp` and its variants, the `cast_*` family, `suspicious_operation_groupings`
— and then FIX those specific warnings, so the documented gate keeps passing at 0. That
preserves both the doctrine and the gate. Enabling a thousand style lints at `warn` under a
`-D warnings` gate just converts a real gate into a broken one.

---

## 2b. TrialSetting constants — CAUGHT BEFORE DAMAGE, no repair needed

The deletions agent refused a third bad instruction and asked for a ruling instead. The brief
said to delete "the four disabled TrialSetting constants" as obsolete shims. They are not
shims: they are the default-off state of documented class-B levers under a binding owner
ruling (`SPEC-V5-PERF.md:14-15`, "each behind a config flag, ALL DEFAULT OFF") and
`03-physics-and-constants.md:903` ("For B1-B5, any future enabled candidate remains
experimental `Placeholder` and non-emittable until a corpus protocol plus owner decision
selects a concrete value").

The brief also named four when there are FIVE of the same shape — `REWIRE_WORK_BUDGET`
(B5) at `placeholders.rs:1378` was missing from the list, and B5 is additionally a
precondition for scale-level identity evidence. Deleting four of five would have broken a
documented set asymmetrically.

RULED: keep all five, change nothing. Their registry notes already carry the honest
disclosure ("Registered but unread outside this disabled check"). Same for `Pending<T>`,
which the same journal line explicitly defers rather than abandons.

No repair needed — the agent stopped before acting. This is the failure mode working
as designed.

---

## 3. Vendored-checksum verifier trusts manifest completeness (agent finding, not ours)

`crates/plinth-solid/build.rs:46`. A lane made `FILES.sha256` actually verify — good — but the
verifier iterates the manifest, so deleting a manifest row silently exempts that vendored
file from verification. Fix: enumerate the vendored tree and require every compiled source to
have a manifest entry, not the reverse.

---

## 4. rustfmt debt grew past its frozen ceiling (agent finding)

`cargo fmt --all -- --check` reports 112 hunks against a documented maximum of 104, which the
conventions say must never grow. Run `cargo fmt` on the files this remediation touched.

---

## NOT damage — verified pre-existing, do not "fix"

- `closure_repairs_censored_dust_because_that_repair_closes_a_real_hole`
  (support_runner_integration.rs:1173) panics by design with its own `expect` message; it
  reports a live support-planning defect. Verified unchanged at `5d0420e` in a detached
  worktree: identical panic, identical `candidates_tried: 274`. Pre-existing deliberate red.
- `a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule` did not
  finish: **887 trials over 47 minutes**, 2,548 candidate rows still ranked. That is the
  uncapped repair search again, and it is WORSE than the 213-trial/874-second case already
  measured. Feed this to the repair-cap evidence lane.

---

## Note on the verifier itself

It self-reported: *"Single-pass inline review completed without the Agent tool; this was not
the full multi-agent fan-out."* So it graded itself down and still found four real problems.
Its verdict string was BROKEN; two of its seven findings (the panicking test, and by
extension the "branch cannot pass its gate" framing) attribute a pre-existing condition to
this branch and should be read as context, not damage.

---

## 5. ~~TWO KERNEL DEFECTS THAT MUST LAND~~ — CLOSED, both already fixed

**CORRECTION 2026-08-27, orchestrator.** This section was WRONG when written and is kept
only so the correction is legible. Both defects below were already fixed in the salvage
commit `623efec` before I recorded them as open — I wrote the note from the lane's earlier
report without re-checking the tree, and the lane's twin had landed both in the meantime.

Verified against `git show 623efec`:
- `AllFacesExcluded` — present, three occurrences in `queries.rs`.
- empty-segment-set — the surviving implementation is `vec![None; plan.point_count]`, one
  row per point. Correct.

Both are now on `feature/remediation-kernel`, with `geom_kernel` at **126 passed, 0 failed,
0 ignored**. Anyone working from the original text below would have redone finished work or
distrusted good code. The original follows, struck.

**LESSON: a note recording an open defect must be re-checked against the tree before anyone
acts on it. A stale "still broken" note is as costly as a stale "already fixed" one.**

### ~~Original text (superseded)~~

Found by the kernel-numerics lane in the *landed* implementation of its own defects. Neither
is in the tree. Both are the same class the lane existed to remove: an answer that was never
computed, wearing a type that says it was.

**(a) `segment_clearance` — missing third unmeasured variant.**
The landed enum models `DegenerateAxis` and `InsetsConsumeAxis`. It misses the case the
review actually cares about: when the exclusion set covers every face the walk can reach,
`seg_surface_min_excluding` never updates its `f64::INFINITY` initialiser, so the row returns
`Measured { clearance_mm: INFINITY }` and `clears_or(false)` returns TRUE for a row where
nothing was tested. Before the fix a caller misread a float; after it, a caller correctly
reads a type that lies — strictly worse, because the lie is load-bearing.
FIX: add `AllFacesExcluded`, returned when the walk found no testable face.
MUST ACCOMPANY IT: `rust/geom_kernel/tests/face_excluded_clearance.rs:99` currently asserts
`INFINITY` and would keep passing against the new enum while the hole stays open. Update it
to assert the variant. **If that test still passes unchanged, the hole is still open** — that
is the check that catches this class.
Note the test's own comment states the real contract: *"The caller's admissibility rule is
what must refuse it."* `rewire.rs:1088` is a caller that does not. The kernel was documenting
its side correctly all along; the missing rule is at the call site.

**(b) `point_nearest_segment_batch` — empty segment set erases the points.**
On `segment_count == 0` the landed version returns `nearest: Vec::new()`, but `point_count()`
is `self.nearest.len()`. A batch of 100 points against 0 segments reports `point_count() == 0`
and `rows()` yields nothing, so a caller zipping `rows()` against its own points silently
processes zero rows instead of 100 "no nearest" answers.
FIX: return one `None` per point — which the function's own `nearest()` doc already promises.

---

## 6. PROCESS FAILURE TO RECORD — duplicate lanes from SendMessage resume

Twice today the orchestrator replied to an agent with SendMessage and thereby RESUMED it while
the workflow's own copy of that lane was still running, producing two agents with identical
briefs and identical file lists in one checkout.

- First: the deletions lane. Cost — a blocked stage, a stalled workflow, two uncommitted
  edit-sets whose authors both died before committing.
- Second: the kernel-numerics lane. Cost — every file in the lane written twice, `geom_kernel`
  left non-compiling more than once, and one agent spending most of its run reconciling
  against a copy of itself.

RULE: **never SendMessage an agent that belongs to a running workflow.** A reply resumes it.
If a workflow lane asks a question, either let the workflow's own gate handle it, or stop the
workflow before answering. Answering is not free.

---

## 7. TWO FROZEN DIGESTS NEED AN ARGUED REFRESH — and why they were missed

**State:** `develop` at `cbc9711` has two failing tests, both pinning the same value:
`crates/plinth-support/tests/support_runner_integration.rs:346` and `:765`.

```
make_toed_slab's counts must not move: it takes zero repair rounds
  left:  (64, 170, 13198418135389558006)   <- actual now
  right: (64, 170, 12136443226163403608)   <- frozen
```

**The counts are identical.** Only the digest moved, so the plan has the same number of
contacts and members with different content.

**Cause, traced:** commit `ad335d9` (the durability lane) made raft pad-versus-bridge
dispatch explicit — `segment_kind == RaftPad` / `RaftBridge` — replacing a dispatch that
was correct only incidentally, because one producer happened to pass the same endpoint
twice and exact `f64` equality therefore selected pad geometry. A zero-length bridge is now
a genuine `InvalidPrimitive` rather than a silent fallback to pad geometry. That legitimately
reclassifies raft primitives, so the digest legitimately moves.

**This is the fix working.** The old digest encoded a classification made by accidental float
equality. Under OD-8 the refresh is therefore ALLOWED — but it must be recorded with that
argument, not silently re-recorded as "whatever the code now emits".

**WHY IT WAS MISSED, which is the more important half.** The lane DID notice the change moved
goldens: `15e4a6f` refreshed `crates/plinth-verify/tests/coverage_gate_toed_slab.json`. It
found one of the three places that pin this fixture and missed two. It missed them because
they live in `support_runner_integration`, and that binary **times out before reporting
anything** — the uncapped coverage-repair search (REVIEW.md short-list 12) makes it
un-runnable, so its tests never report pass or fail.

So the slow binary is not merely slow. **It has been silently unable to guard anything it
contains**, and a real behaviour change walked past it. Every finding about the uncapped
search now has a second cost attached: it disables a test binary's ability to catch
regressions, invisibly.

**Do:** refresh both digests in one commit whose message carries the raft-dispatch argument
above, and state in it that these were found only because the binary was run with a timeout
that let its early tests report before the kill.
