# plinth-3d remediation plan

A sequenced path from the review's 203 verified findings to a codebase that is correct,
tested, and honestly named — before any new feature work begins. All paths are relative
to the repository root.

---

## 1. Decisions taken

Three rulings shape everything below. They were taken on 26 August 2026.

**Orphaned code is deleted.** The scoring and auditing crate (`crates/plinth-metrics`,
9,176 lines), the transit measurement module
(`crates/plinth-support/src/transit_measurement.rs`, 3,788 lines) and the designed-mask
validation table (`crates/plinth-support/src/designed_embed.rs:201`) all go. None has a
production caller. Roughly 13,000 lines leave the tree, and eleven findings are resolved
by deletion rather than by work.

**The retired Python implementation no longer constrains the Rust.** About a dozen
checks across four files exist to reproduce Python's exact results. Those pins may be
broken where current physics disagrees with them, with the new expected values recorded
and the reason written down. This unblocks the honest version of roughly six fixes,
including the strut-volume formula and the island-birth overlap test.

**The pre-write safety gate gets honest reporting immediately and real coverage in
stages.** Disclose which checks did not run now; implement `Load` and `BallBreach` next,
because their underlying measurements already exist; schedule the remaining nine.

### The consequence of taking the first and third together

Deleting the auditing crate removes the one component whose job was to check the engine
independently. Porting two of twelve gates does not replace it. For a period, the project
has no independent check of its own output.

This is not a loss of a working safeguard — the auditor is already disconnected and the
gate already passes eleven checks without running them, so the current independence is
zero either way. But it does close the cheap route back. One mitigation is mandatory
rather than optional:

> **The two ported gates must judge evidence recorded during the run, not re-run the
> engine's own code.** A checker that calls the thing it checks cannot catch that thing's
> mistakes. This is exactly the defect the one implemented gate already has
> (`crates/plinth-verify/src/lib.rs:330`), and porting two more in the same shape would
> add work without adding safety.

---

## 2. The spine: one home per fact

Nearly every finding is one failure repeated. A fact is established in one place, then
restated by hand somewhere else, and the two copies drift. A limit lives in the constants
registry and is retyped as a bare number two hundred lines away. A distance formula is
written once properly and copied three times with different rules for handling a
zero-length input. A word means the bottom of a printed part in one module and the
smallest allowed value in another.

So the campaign has one goal, and every phase serves it: **give each fact exactly one
home, and make the compiler or a test enforce it.** Where a rule cannot be made
mechanical, it is not a rule — it is a hope.

One structural observation drives several phases at once. The low-level geometry crate
(`rust/geom_kernel`) keeps three genuinely useful things private: its vector arithmetic
(`mod vec3`), its cached thread pools (`mod par`), and a batch query variant that already
does what callers hand-roll. Every crate above it therefore re-implements them — vector
maths appears seven times, four crates copy the thread-pool workaround, and the wrong
projection path is the one that carries the review's most direct bug. Making those three
things public is a small change that dissolves a column of findings.

---

## 3. Phases

Effort is `trivial` (under an hour), `contained` (a session), `substantial` (a focused
piece of work), `project`.

### Phase 0 — Make change safe

Nothing here changes behaviour. Everything here exists so that later phases can be
proven rather than believed.

The argument for doing this first is specific: **no test drives a model through the full
hollowing chain**, and the two largest hollowing defects live downstream of the last
assertion in the suite. Two named regressions are guarded only by tests marked `#[ignore]`
that read large model files not stored in the repository, so a clean checkout cannot run
them. Until that is fixed, no hollowing fix can be shown to work and no regression can be
caught.

| # | Work | Why | Effort |
|---|---|---|---|
| 0.1 | Add the `[lints.clippy]` block to the workspace manifest | The project has no lint configuration at all, so ~1,200 warnings carrying real signal are invisible in normal builds. The exact block is already drafted in `census-clippy.md`. | trivial |
| 0.2 | Commit a set of synthetic geometric fixtures — see section 3.1 below | The two skipped regression tests read STLs that are not in git. Owner ruling of 26 August 2026: use geometric constructions, not sculpted models. | contained |
| 0.3 | Add an end-to-end hollowing test that runs past `plan_drains` through drilling | `crates/plinth-hollow/src/drains.rs:3079` — the refill, the audit rescue and the trapped-cavity check are all downstream of every existing assertion. | contained |
| 0.4 | Snapshot the 203 findings with their current file and line into a tracking file | Later phases move thousands of lines; every reference in the review goes stale. | trivial |

#### 3.1 The test fixtures

Synthetic geometry, not decimated sculpts. A decimated mini is thousands of triangles and
still cannot be *aimed* at a specific defect; a purpose-built solid is tens of triangles and
reproduces one defect exactly, deterministically, in a file small enough to live in git.

The two properties that matter are independent, and the distinction is what makes this work:
**a fixture must be small as a file and large as an object.** Three registered thresholds
constrain the second half — the savings gate admits a model only if hollowing saves at least
1.5 mL, the bulk-size policy requires at least 10 mm, and the wall target is 2.5 mm with a
hard 2.0 mm minimum. A shape that clears all three can still be a dozen triangles.

`cube.stl` as supplied clears them already: 20 mm on a side, 12 triangles, 684 bytes, sitting
on the plate at Z 0–20. Hollowed at a 2.5 mm wall it has a 15 mm cubic cavity, saving
3.375 mL against the 1.5 mL minimum, and its 20 mm bulk clears the 10 mm policy. It works
unmodified as the baseline.

| Fixture | Targets | Shape | Triangles |
|---|---|---|---|
| `cube.stl` *(supplied)* | Baseline — one chamber, one drain, chain runs end to end | 20 mm cube on the plate | 12 |
| `legs-uneven.stl` | Short-list 3 — the drain-banding defect | A 40x40x20 body at Z 40–60 on two 12x12 legs of *different* lengths: one from Z 0, one from Z 25 | ~44 |
| `cascade-pocket.stl` | Short-list 4 — one-pass refill versus the required fixpoint | Three stacked chambers joined by offset necks, so refilling the middle traps the top | ~60 |
| `thin-fin.stl` | Short-list 11 — the unsettled near-wall question | A plate carrying a fin under 0.05 mm thick | ~20 |

`legs-uneven.stl` reproduces the drain defect by construction, and the arithmetic is worth
stating because it is what makes the fixture a proof rather than a hope. The drain search
admits a candidate only if it lies within 15 mm of the *whole cavity's* single lowest point.
The long leg's cavity bottom sits at about Z 2.5, so the band admits everything below about
Z 17.5. The short leg's cavity bottom sits at about Z 27.5 — outside the band. It therefore
gets no planned drain, is found undrainable at drilling, and is refilled to solid: the body
and one leg print hollow, the other leg prints solid. Total cavity is about 20.8 mL, well
clear of the gate. After the fix, the test asserts two drains and both legs hollow.

The last two need their exact geometry chosen after reading the drainability predicate and
the near-wall skip, because both depend on a threshold whose behaviour has to be read rather
than assumed. That reading is part of Phase 4.3 and 4.14 respectively.

One thing found while checking the gates, which belongs in Group D: `HOLLOW_BULK_MIN_MM`
decides whether a model is hollowed at all, and its entire recorded derivation is the
sentence *"Hollowing bulk-size policy."* It is a number with authority and no evidence.

### Phase 1 — Free wins

Roughly thirty findings, none able to change behaviour for the worse.

**Item 1.1 must come before Phase 2, and this ordering is the one to insist on.** Four
modules are labelled as unused while production calls them — `transient.rs:424`,
`legalization.rs:412`, `coverage_closure.rs:600`, and the three brace-geometry methods at
`legacy_compat.rs:38`. One is marked unused *one commit ahead of its reader*. These labels
are false, they change nothing when removed, and they are precisely what would mislead
someone deleting dead code in the next phase.

| # | Work | Findings resolved | Effort |
|---|---|---|---|
| 1.1 | Delete the false "unused code" labels across `plinth-support` | 4 findings; restores the reachability signal Phase 2 depends on | trivial |
| 1.2 | Replace `face_ids()[tid as usize]` with `tid` at `rust/geom_kernel/src/queries.rs:1827` and `:2142`, and extend the subset test to reach both APIs | Short-list 2 — the review's most direct defect: a panic in a worker thread, or the wrong faces exempted from a clearance safety check | trivial |
| 1.3 | Carry `pending_gates` into the artifact outcome and print it; fix the guard test that passes an empty list | Short-list 1, disclosure half — the run report currently cannot say which safety checks did not run | trivial |
| 1.4 | Make `mod vec3`, `mod par` and the batch projection variant public in `rust/geom_kernel` | Removes the *pressure* behind ~10 duplication and efficiency findings; prerequisite for Phase 3 | trivial |
| 1.5 | The remaining ~25 `trivial` findings, batched by file | Assorted; includes three stale comments, two unregistered constants, and the `to_ne_bytes` ordering that reaches a sealed digest | contained |

### Phase 2 — Delete what nothing runs

Now that the labels are honest.

**Do 2.1 before 2.2.** The only correctly-named constant for converting cubic millimetres
to millilitres — `CUBIC_MILLIMETRES_PER_MILLILITRE` — lives in the crate being deleted
(`crates/plinth-metrics/src/supports.rs:31`). The support engine's own copy is called
`MILLIMETRES_PER_MILLILITRE` (`crates/plinth-support/src/cco.rs:37`), which is
dimensionally meaningless. Deleting the crate first would lose the right name and keep the
wrong one.

| # | Work | Effort |
|---|---|---|
| 2.1 | Move `CUBIC_MILLIMETRES_PER_MILLILITRE` into `plinth-core` and repoint every divisor spelling to it | trivial |
| 2.2 | Delete `crates/plinth-metrics` (9,176 lines) and its workspace member entry | contained |
| 2.3 | Delete `transit_measurement.rs` (3,788 lines) and its four runner wrappers | contained |
| 2.4 | Delete the designed-mask mint: `DesignedMaskTable`, `validate_forest`, `validate_members_batch` | substantial |
| 2.5 | Delete the genuinely obsolete shims: `penetration_intervals_batch` (retained for a PyO3 surface that does not exist), the four disabled trial constants, the unreachable `cached_hosts` fallback, the `--no-cache` flag that controls nothing | contained |
| 2.6 | Gate `pub mod testkit` behind a feature so release builds stop shipping brute-force oracles and two `unwrap`ping constructors | contained |
| 2.7 | Decide `Pending<T>`: a fully implemented, tested provenance category used by none of the 306 registered constants | trivial |

Deleting `plinth-metrics` resolves eleven findings outright, including one of the four
ray/triangle implementations, two exact-float-equality gates, and a normative accept
threshold built on an admittedly assumed blend.

### Phase 3 — One home per fact

The duplication and design-flaw work, together, because they are the same problem seen
from two directions. Four groups, four different answers.

#### Group A — Small geometry helpers copied everywhere

Vector arithmetic seven times, polygon area three times, point-in-polygon twice, four
point-to-line distance implementations with three different rules for a zero-length input,
three ties-to-even rounding implementations in one crate where the standard library has
had one since Rust 1.77.

**Answer:** Phase 1.4 already made the kernel's operations public. Delete the copies, and
register the tolerance values as named constants with recorded derivations. Then add the
standing rule — *no crate above the geometry kernel defines a geometry primitive* — as an
`ast-grep` check in CI, so it cannot silently return.

#### Group B — Four ray-hits-triangle implementations

`rust/geom_kernel/src/moller.rs` is canonical. `crates/plinth-metrics/src/geometry.rs`
goes with Phase 2.2 — and it is worth noting what leaves with it: that copy still contains
a behind-the-origin defect that was fixed in `plinth-mesh` long ago, and `docs/status.md`
lists it as open (#50). The remaining copy, `crates/plinth-mesh/src/triage.rs:1043`, is a
hand-written third implementation that also does a per-point full-face scan where a
batched kernel call exists two files away.

**Answer:** consolidate on the kernel. A triangle within one part in a million of an edge
is currently a hit for one implementation and a miss for another, so this is a real
divergence, not a tidiness question.

*Had the auditing crate been kept, the answer would have been two deliberately independent
implementations pinned against each other by a differential test. The deletion ruling
removes that option; see section 1.*

#### Group C — One law, several owners inside one crate

The tapered-strut volume calculation has three live implementations in `plinth-support`
(`member_sizing.rs:601`, `cco.rs:5672`, `rewire.rs:1186`), differing by rounding. They feed
candidate pricing and rewire savings, so a close decision can be made on a different number
than the sizing used. The same shape appears in the hand-inlined hash fold (four copies of
something factored out precisely to prevent that), the two restart-from-scratch merge loops,
and the two hand-kept route-point gatherings.

**Answer:** extract one function per law. The parity ruling in section 1 applies directly
here — `member_volume` re-derives length as a plain sum of squares instead of `hypot`
deliberately, to match the retired Python. That deliberate difference may now be removed
rather than preserved as a named variant. Record the change and re-pin the affected
expected values.

#### Group D — A registry value restated as a bare number

The largest group, and the one worth real architectural work. The pattern: a threshold with
genuine authority — it decides accept or reject, or how much material gets printed — written
as a bare millimetre literal at the point of use, often a few lines from a neighbouring value
that *is* read from the registry. `runner.rs:194` is the clearest case: it reads four caps
from the registry and restates thirty-four more as literals whose registry notes name that
very function.

The guard for this already exists and already works. `AcceptSafe` is a sealed trait with a
compile-fail test proving an unmeasured value cannot satisfy it — and it is used as a bound
in exactly **one** production signature (`crates/plinth-core/src/layer.rs:28`). Everywhere
else the value is taken through `.diagnostic()`, of which there are 414 calls. Three of them
decide whether a model may be written to disk.

**Answer, and this is the highest-leverage change in the plan:**

1. Make every function that produces a verdict take `&impl AcceptSafe<f64>`. The compiler
   then refuses an unmeasured value on any acceptance path.
2. Add a separate, explicitly named accessor — `sanctioned_search_read()` — for the
   legitimate case where a number only narrows a search and decides nothing. Every remaining
   unmeasured read becomes visible at its call site.
3. Reclassify the values that are genuinely owner decisions rather than guesses
   (`HOLLOW_SAVE_MIN_ML`, `HOLLOW_WALL_MIN_MM`) as measured with an owner-decision evidence
   source — the pattern `TIP_DIAMETER_MIN_ALLOWED_MM` already uses.

This converts the physics-first doctrine from a rule people remember into a build error, and
makes the whole class impossible to reintroduce.

Also in this phase, because they are the same failure: the seven joint literals duplicating
registry policy, the drilling keepout ladder re-derived as literals in the exact drift class
its own comment condemns, the census hard-coding a band transcribed from `brace.rs` so the
instrument would lie about the change it exists to measure, and `rewire.rs:163` reading five
registered values and then refusing any value but a hard-coded literal — so the registry can
break the behaviour but never move it.

### Phase 4 — Correctness

All of it. Each item is contained; the hollowing pair depends on Phase 0.3.

| # | Work | Severity | Effort |
|---|---|---|---|
| 4.1 | Re-run anchor qualification after the ray snaps to a nearer face, and record delicacy against the face actually seated on | high | contained |
| 4.2 | Rebuild the host lookup as a real spatial index — see section 4 below | high | contained |
| 4.3 | Band the drain search per drainage basin instead of per connected chamber; correct the doc that asserts behaviour the code lacks | short-list 3 | substantial |
| 4.4 | Loop the trapped-cavity refill to a fixpoint; keep the drilling check as the assertion it was meant to be | short-list 4 | contained |
| 4.5 | Replay the contact set through the same selector that produced it; delete or mode-gate the legacy pre-routing loop | short-list 5 | contained |
| 4.6 | Make the island-birth overlap test hole-aware, and fix the contour sampling with it so the pinned digests move once | short-list 7 | substantial |
| 4.7 | `try_retire` — stop committing unconditionally and clamping a volume *increase* to a zero saving | medium | contained |
| 4.8 | Make `slender_rungs` distinguish an unsatisfied search from a solution instead of returning its search bound | high | contained |
| 4.9 | Sync the directory after both renames, in `chitubox_io` and `emit` | medium | contained |
| 4.10 | Give `Mesh::from_indexed` a degenerate-face error instead of storing a zero normal consumers treat as a direction | medium | contained |
| 4.11 | Use the area centroid for the island-birth seed position, not the unweighted boundary-vertex mean | medium | contained |
| 4.12 | Stop `segment_clearance` returning infinity as an in-band "not measured" sentinel that a caller reads as a pass | medium | contained |
| 4.13 | Gate the printable envelope on the actual printer profile, which is in scope at the call site, instead of a 300 mm literal | high | contained |
| 4.14 | Derive `exterior_face_mask`'s probe offsets from something physical | high | contained |
| 4.15 | The remaining `high` and `medium` correctness and numerics entries | — | contained |

### Phase 5 — Vocabulary

"Floor" means "smallest allowed value" in nine modules across five crates, in a codebase
where it also means the bottom of a printed part and the build plate. 495 occurrences across
53 files. The ruling is quoted verbatim inside the codebase, in one constant's own registry
note, and was applied to that constant and nothing else.

**This is not a find-and-replace.** Three cautions:

- `on_floor`, `trunk_floor`, `at_floor`, `above_floor` and `below_floor` are *correct* uses.
  Leave them.
- `patch_area_floor_mm2` is not a minimum at all. It is an upper bound on ignorable area, so
  the public name points the wrong way across the comparison. The code already knows the right
  name: the runner reads it and immediately binds it to `censor_area_mm2`, in two places
  (`runner.rs:1259`, `:2029`). Mechanically substituting "minimum" would preserve the lie in
  tidier form. About a dozen names need this kind of judgement.
- `BelowFaceFloor` is a *serialized* enum variant, so the word is in the persisted orientation
  report on disk. Rename the Rust identifier and keep the stored name with a serde attribute,
  so existing reports stay readable.

Also in this phase: the radius parameters at the boundary every support crate crosses
(`queries.rs:1908`) carry no unit suffix and no radius-versus-diameter statement, two arguments
away from ones that do. Given the known confusion between this code's radii and the printing
tool's diameters, those get suffixes.

### Phase 6 — Documentation

Your rule: **comments describe the current state of the code, never its history.** The review
found the codebase honest about history but drifting about the present — 170 lines of doc on a
six-line function, four paragraphs of which are explicitly retracted by later ones; a crate doc
asserting there is no public entry point 160 lines above the public entry point.

| # | Work | Effort |
|---|---|---|
| 6.1 | Rewrite every crate-level doc to describe only what the crate now is | contained |
| 6.2 | Strip retracted and superseded paragraphs from module and function docs; move anything worth keeping to the engineering journal | contained |
| 6.3 | Verify every citation resolves; fix the two pointing into files deleted by the docs retarget | trivial |
| 6.4 | Fix the numeric claims that disagree with the code: the traversal cost stated as 13 and 12 where 11 is right, the registry count stated as 287 where 306 is verified | trivial |
| 6.5 | Correct `docs/architecture.md` — it presents the deleted auditing crate and an uncalled format gate as live, and states a slice-in/slice-out rule that is false for several public queries | contained |
| 6.6 | Add the missing provenance for the nine kernel tolerances whose only recorded source is a deleted Python file, with honest "inherited, no measurement of record" notes | contained |

A standing rule worth adding to `CLAUDE.md` so it survives this pass: *a comment states what
is true now; the reason for a change belongs in the commit message or the journal, not above
the code.*

### Phase 7 — Architecture

Only now, on a base that is correct, tested and honestly named.

| # | Work | Effort |
|---|---|---|
| 7.1 | Port the `Load` and `BallBreach` gates, judging recorded evidence rather than re-running producer code | substantial |
| 7.2 | Move the stage handoff types out of the implementation crates into a shared boundary crate, so `plinth-emit` stops inheriting a 100,000-line build closure and the CLI stops depending directly on two stage crates | substantial |
| 7.3 | Lift the general facilities that landed where they were first needed — the progress clock inside a coverage-repair module, the 2D contour stitcher inside the support crate — to where crates below them can reach them | project |
| 7.4 | Make `SupportPrimitive` unable to represent invalid states, rather than catching them at construction | substantial |
| 7.5 | Decide the seam split for `plinth-support` — still 63% of the workspace after Phase 2 removes 3,800 lines from it | project |
| 7.6 | Schedule the remaining nine safety gates | project |

The efficiency findings fold into this phase rather than standing alone, because they share
its causes: things rebuilt that did not change (the decimation surface, the forest clone per
candidate, the binding revalidation at seven stage boundaries), and per-item calls at batch
entry points (`classify_batch` is literally `points.iter().map(classify)`).

---

## 4. The host lookup: three findings, one fix

Worth pulling out because it is small, early, and resolves more than it appears to.

`crates/plinth-support/src/cco.rs:3528` is documented throughout as a kd-tree — a structure
for finding nearby points quickly — with a `KDTREE_REBUILD_EVERY` constant to match. There is
no kd-tree anywhere in the crate. It is a list, scanned end to end, plus a second full walk and
two allocations, once per support terminal, to return five nearest candidates.

The name is what makes the third finding invisible. Because it reads as a cache being
periodically rebuilt, the 25-terminal rebuild interval looks like harmless amortisation. It is
not: entries added since the last rebuild are ranked by a *different measurement* than older
ones — a stale three-sample minimum versus a current midpoint. **A performance-tuning number
currently decides which supports get built.**

One fix resolves all three. The spatial-index library (`kiddo`) is already a workspace
dependency and needs one line adding to this crate's manifest. Build a real index, rank every
candidate by one current measurement, and delete the rebuild interval. The name becomes true,
the scan disappears, and the tuning knob stops affecting output.

---

## 5. Working method

**Branches.** One feature branch per phase off `develop`, following the existing convention.
Phases 1 and 2 can merge quickly; Phases 3 and 4 want review before merge.

**The compiler is the rename verifier.** For Phases 3 and 5, script the change, then build —
anything missed or renamed wrongly fails to compile. The compiler cannot see four things:
text inside quoted strings, documentation comments, test names, and names written into saved
files. Those four get a hand audit each time. `rust-analyzer` does type-aware renaming but only
as an editor operation; its command-line binary exposes no rename verb and is not currently
installed for this toolchain. `ast-grep` is installed and understands structure but not scope,
which is sufficient here because these identifiers are long and distinctive.

**Every phase ends green.** Build clean, tests pass, new lint configuration clean. A phase that
cannot end green gets split.

**Re-pinning is recorded.** Where the parity ruling causes an expected value to change, the new
value and the reason go in the commit, and the old value stays visible in the journal. The
point of releasing the pins is to stop matching a deleted implementation, not to stop being
able to explain a number.

---

## 6. What to do first

Three commits, in this order, none of which can break anything:

1. **Lint configuration** — makes the remaining work visible in every build.
2. **The kernel index fix** — two tokens, and it is the review's most direct defect.
3. **The false unused-code labels** — changes no behaviour, and every later cleanup depends
   on the reachability signal it restores.

Then Phase 0.2 and 0.3 — the fixture and the end-to-end hollowing test — because everything in
Phase 4 that touches hollowing is unprovable until they exist.
