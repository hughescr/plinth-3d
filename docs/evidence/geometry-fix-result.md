# The near-parallel geometry fix — result, and the cost it turned out to carry

Verification of the working tree on `feature/stage0-bytes-audit` against its parent
`487e71f`, under `docs/specs/geometry-adjudication.md`.

> ## THE HEADLINE: eight models that emitted at `487e71f` do not emit now.
>
> `Cylinder`, `make_box`, `make_overhang_table`, `make_thin_fin`, `Morel base`,
> `Round 25`, `Round 50`, `Round 100`. Every one of them refuses at the SAME call
> site — `crates/plinth-support/src/designed_embed.rs:340`
> (`support.embed.witness`), raising `DesignedMaskError::NearParallelWitness`,
> escalated to a whole-model refusal by `crates/plinth-support/src/initial.rs:1331`.
> Two more models (`make_raking_blade`, `make_toed_slab`) refused before and refuse
> now, but for this new reason instead of their old ones.
>
> **Refusing on uncertainty is not free. On this corpus it costs 8 of 29 emitters.**
>
> No tolerance was adjusted to recover them. That is the owner's ruling to make.

A second, independent regression is recorded in §7: the delicacy saturation added
at `crates/plinth-mesh/src/delicacy_eval.rs:400` **breaks the rotational invariance
of the orientation scar term**, which is the machine-checked property (T6) that
licenses the orientation search running over S² instead of SO(3). The brief for
this work said it must not touch orientation. It did, through `plinth-mesh`.

**Assessment: the correctness fix is right and the propagation is not sound as
landed.** The two counterexamples are real, the primitives that replace them are
correct and measured, and where the change does not refuse it moves zero emitted
bytes (4 of 4 controlled artifacts byte-identical, §5.4). What is unsound is the
consequence of routing an ABSOLUTE, non-scale-invariant determinant band to a
refusal in every consumer: on axis-aligned geometry the band fires in bulk, and
the pipeline, the hollow drill and the orientation scar term all change behaviour.

---

## 1. Provenance — which binaries produced which number

| | build | sha256 | source files newer than the binary |
|---|---|---|--:|
| **after** (working tree) | `RUSTC_WRAPPER="" cargo build --release`, exit 0 | `efbffde3f6adf15e96c1db2ee6e90b15e9bed7f9d39021513ad64426555c7b40` | **0**, checked with `find crates rust -name '*.rs' -newer target/release/plinth` |
| **before** (control) | `git archive HEAD` into a scratch tree, `cargo build --release --offline`, exit 0 | `dfa0d107a898ceb80573c9d7de3154e59c63eb820babb151c67a331eccb6a5c4` | n/a (archive of `487e71f`) |

The after-binary was copied out of `target/release/` **before** any `cargo test`
ran, so nothing a test build did can have moved it. Every corpus number below came
from those two files. This project has been burned by a stale binary faking a
lane's result; the check is run, not assumed.

`tools/plinth-audit/` was not touched, built, or run. Its working-tree state
(7 modified files, 11 untracked paths) is exactly as found and is excluded from the
commit.

---

## 2. What was implemented, and what deliberately was not

### Implemented

* **`ray_triangle_classified` + `RayTriangleClass`** (`rust/geom_kernel/src/moller.rs`)
  — four outcomes (`Hit`, `Miss`, `NearParallel`, `ExactParallel`) where the pinned
  `ray_triangle` had two. `det == 0.0` stays a prune; `0 < |det| <= DET_EPS` becomes
  an explicit uncertainty. The pinned function is untouched and keeps oracle parity.
* **`segment_segment_distance_min`** (`rust/geom_kernel/src/closest.rs`) — the
  complete five-candidate minimisation (interior critical point plus the four
  boundary-edge minimisers), no epsilon anywhere on its decision path. The pinned
  `segment_segment_distance` is untouched.
* **The propagation contract** at the decision callers: the two BVH walks
  (`queries.rs`), the classified batch entry points, and the consumers in
  `plinth-support`, `plinth-hollow`, `plinth-mesh` and `plinth-cli`.
* **The `SEGSEG_EPS` name split** into `SEGSEG_DENOM_EPS_MM4` (mm⁴) and
  `SEGSEG_LEN2_EPS_MM2` (mm²) — the NAME split only. Both literals stay `1e-14`, so
  the split is byte-neutral by construction and only the contract stops lying about
  one number being a threshold in two dimensions.
* **§O5 constant re-labelling** — "oracle-pinned" replaced by "chosen behavior pin",
  with each entry stating whether it can decide an outcome. **No value changed.**
* **§O6's canonical 2-D primitives** — `geom_kernel::poly2::{dot, perp_dot}`, with
  `plinth-support/src/perception.rs` consolidated onto them.

### Deliberately NOT implemented — §EXT-4's per-model `NearParallelLedger`

The spec threads a per-model ledger through triage, repair, the hollow mesh path,
orientation search, the pipeline report and the CLI, adding an evidence type to
`ModelPreparationReport`. **Owner ruling: do not.** Three review rounds were spent
rediscovering that this plumbing keeps being dropped at a stage boundary, while the
correctness fix converged long ago; the instrumentation had grown larger than the
defect it watched.

What replaced it, in `rust/geom_kernel/src/telemetry.rs`: **one process-global
atomic per Möller call site inside the kernel**, incremented when the uncertain
classification fires, read once at end of run and printed by
`crates/plinth-cli/src/execute.rs::near_parallel_lines`. It answers the only
question being asked — *does refusing on uncertainty make models stop emitting, and
at what rate?* — and touches no report type, no orientation code and no repair path.

Per-model attribution comes from the other direction, and §5 is that comparison.
(Because this verification ran **one model per process**, the "process-global"
counters in §5.3 are in fact per-model. That is a property of how the run was
driven, not a claim the counters can make on their own.)

The zero case is stated rather than silent: a run where no site fires prints
`near-parallel geometry  none: all 3 kernel call site(s) read zero for this run`.
A quiet run and an unwired build must not look alike.

---

## 3. The two counterexamples

Both were re-derived from the literals in a **standalone program that shares no code
with the kernel** (`verify.rs` / `verify2.rs`, compiled with plain `rustc -O`): the
pre-fix routines transcribed by hand from `487e71f`, and the truth computed by a
structurally different route. Running the committed tests would only have shown that
the tests agree with the code they ship beside.

### 3.1 Ray vs triangle (§O4)

Triangle `(0,0,0), (2,0,0), (0,2,0)`; ray `o = (0.4, 0.5, -1e-16)`,
`dv = (1, 0, 2e-15)`, `t_max = 1`.

| | measured |
|---|---|
| **before** — `ray_triangle` (transcribed from `487e71f`) | `hit = false`, `t = -0.0`, `u = -0.0`, `v = -0.0` |
| determinant | `det = -8e-15` — nonzero (`det == 0.0` is **false**) and inside `DET_EPS = 1e-14` |
| **the truth**, from an independent plane solve `t = -n·(o-v0) / n·dv` | `t = 0.049999999999999996`, hit point `(0.45, 0.5, 0.0)`, `u = 0.225`, `v = 0.25`, `u+v = 0.475` — **deep in the interior, not on a shell** |
| **after** — `ray_triangle_classified` | `RayTriangleClass::NearParallel` |

So the pre-fix function reports a definite miss on a real interior crossing, and
the classified function names the uncertainty instead of hiding it. The pinned
function still returns `hit = false` after the change, by design — that is oracle
parity, and `tests/ray_class.rs` asserts both halves.

### 3.2 Segment vs segment (§O2)

`p1=(0,0,0)`, `q1=(0.10780992493971708,0,0)`,
`p2=(0.01755930967909119,-0.09805348888039589,-0.011442695278674364)`,
`q2=(0.11837187720683315,-0.09805257753576863,-0.011442695278674364)`.

| | measured (17 significant digits) |
|---|---|
| **before** — `segment_segment_distance` (transcribed from `487e71f`) | `0.09871890374623554` |
| **the truth**, independent 4001×4001 lattice sweep + local refinement to `step < 1e-18` | `0.09871809337695622` at `s = 1`, `t = 0.8952405853308737` |
| **after** — `segment_segment_distance_min` (five-candidate scheme, transcribed) | `0.09871809337695624` |
| pre-fix excess over the true minimum | **`8.103692793193229e-7` mm** |
| against `PROOF_EPSILON_MM = 1e-7`, the slack of the drilling comparison it fed | **8.104×** |

The closed form sits 2 ULP (≈2.8e-17 mm) above the lattice-refined minimum — floating
point noise ten orders of magnitude below the defect it removes. The error the fix
removes is one-sided HIGH, and an overestimated gap turns a refusal into an
acceptance, which is why it mattered.

The committed `TRUE_MINIMUM_MM = 0.098_718_093_376_956_24` in
`rust/geom_kernel/tests/segseg_min.rs` matches the independent computation exactly.

`tests/segseg_min.rs` also reports a 300-row near-parallel battery:
`worst(sweep − min) = 8.441456e-9`, `worst(pinned − min) = 2.889412e-9`.

---

## 4. The uncertainty counts

Per Möller call site inside the kernel, summed over the 31-model corpus run of §5,
one process per model:

| call site | count | share |
|---|--:|--:|
| `kernel.walk.collecting` (`collect_hits_with_stack`) | **112,773** | 97.3 % |
| `kernel.walk.counting` (`count_hits_with_stack`) | **2,169** | 1.9 % |
| `kernel.seg_tri.pierce` (`seg_tri_dist`'s interior-pierce candidate) | **1,017** | 0.9 % |
| **total** | **115,959** | |

### What these numbers actually say

1. **They are bimodal, not diffuse.** 19 of 31 models read **exactly zero at every
   site** — and 19 of the 21 emitters do, the two exceptions being `Sphere` (1,146)
   and `Tavey Base` (3,304). The whole 115,959 comes from 12 models, and
   **95.7 % of it from four**: `Round 50` 48,146, `Round 25` 28,154, `Morel base`
   19,148, `Round 100` 15,549 — 110,997 between them.
2. **A nonzero reading does not by itself kill a model.** `Tavey Base` reads 3,304
   and emits; `Sphere` reads 1,146 and emits. The counters aggregate every site,
   and only the `support.embed.witness` consumer refuses on them — the delicacy
   consumer saturates instead (§7), and the triage probe path decided all 1.66 M of
   its evaluations (§6).
3. **`kernel.seg_tri.pierce` fired on exactly one model** (`Sphere`, 1,017). Two of
   the three counters carry essentially the whole signal.
4. **The band is not scale-invariant, and that is what makes the counts large.**
   `DET_EPS = 1e-14` is an absolute bound on `e1·(dv × e2) ≈ 2A·|dv|·cos θ`. On
   axis-aligned geometry that has been rotated by the orientation search, faces that
   were exactly parallel to a ray (`det == 0.0`, a correct prune) become *almost*
   exactly parallel (`det` a few times `1e-15`, an uncertainty). `make_box` is a
   12-triangle cube and reports **12** near-parallel classifications — one per
   triangle. This is §O5's own debt entry D2 ("where the band BEGINS still moves
   with the representation of the direction vector") arriving with a measured price.
5. **The classification carries no proximity test.** `ray_triangle_classified`
   returns `NearParallel` on `|det|` alone, so a ray that is near-parallel to a
   face's plane is counted as undecidable **even when it passes far outside that
   triangle** — the barycentrics are the guarded `inv = 0` products and cannot say
   otherwise. How much of the 115,959 is genuinely undecidable versus a face the ray
   demonstrably misses is **not measured here**, and it is the first thing worth
   measuring before any ruling.

---

## 5. The corpus

### 5.1 Method

The round-3 set of `docs/evidence/audit-baseline-round3.md`: 24 real miniatures
(`../minis-to-process/`) plus the 7 corpus fixtures (`corpus/models/`), **treant
excluded**, 31 models. `timeout -k 15 1500` per model, one process per model, three
concurrent, `plinth prepare --output saturn4-ultra` with default dense orientation.

The control is the same 31-model harness against the `487e71f` binary, run on 14
models: the 10 that changed disposition or refusal reason, plus 4 that emit in both
for a byte comparison.

**One methodology correction, made rather than hidden.** In the first pass
`Crawler Zombie` and `Decomposing Zombie` hit the 1500 s ceiling (exit 124) — but a
full `cargo test` build was running concurrently. Re-run on an idle machine they
finished in **792 s** and **743 s** and both EMITTED. The 1500 s readings were CPU
contention, not a disposition, and they are not reported as one.

### 5.2 Dispositions — 21 of 31 emit, where 29 of 31 emitted before

| | before (`487e71f`) | after |
|---|--:|--:|
| emitted | 29 | **21** |
| refused / failed | 2 | **10** |

**Lost emitters — measured control vs. after on this machine, same corpus, same day:**

| model | before | after | near-parallel at the failing run | witness rows |
|---|---|---|--:|--:|
| `Round 50` | EMITTED (3 s) | **ERROR exit 5** | 48,146 | 143 |
| `Round 25` | EMITTED (1 s) | **ERROR exit 5** | 28,154 | 410 |
| `Morel base` | EMITTED (52 s) | **ERROR exit 5** | 19,148 | 2 |
| `Round 100` | EMITTED (3 s) | **ERROR exit 5** | 15,549 | 7 |
| `Cylinder` | EMITTED (6 s) | **ERROR exit 5** | 196 | 2 |
| `make_overhang_table` | EMITTED (1 s) | **ERROR exit 5** | 135 | 6 |
| `make_thin_fin` | EMITTED (0 s) | **ERROR exit 5** | 26 | 9 |
| `make_box` | EMITTED (0 s) | **ERROR exit 5** | 12 | 4 |

All eight refuse identically:

```
sealed support run failed during Preparation:
  Initial(Attestation(NearParallelWitness { near_parallel: N }))
```

raised at `crates/plinth-support/src/designed_embed.rs:340` and turned into a
whole-model failure at `crates/plinth-support/src/initial.rs:1331`, where an
`UnusableGeometry` cause on a COMMITTED member is collected into a total refusal.
The comment at `designed_embed.rs:334` describes this refusal as
"candidate-granular … not a pipeline failure"; on the committed-member path it
is measurably a pipeline failure, and that comment is wrong about where it lands.

**`Morel base` is the sharpest case: 2 undecidable witness rows refuse a
71,466-face model that emitted 52 seconds earlier on the control.**

**Refused before and after, reason changed:**

| model | before | after |
|---|---|---|
| `make_raking_blade` | `CoverageClosureRefusal { NoImprovingRepair, candidates_tried: 19 }` | `NearParallelWitness { near_parallel: 4 }` |
| `make_toed_slab` | `Brace(ClearanceViolation(1))` | `NearParallelWitness { near_parallel: 3 }` |

Neither is a new loss, but both now refuse at an earlier stage for a different
reason, so any downstream expectation keyed to the old refusal class has moved.

### 5.3 Full table

| model | disposition | near-par total | collecting | counting | pierce | s |
|---|---|--:|--:|--:|--:|--:|
| Bat Cat Owl | EMITTED | 0 | 0 | 0 | 0 | 144 |
| Crawler Zombie | EMITTED | 0 | 0 | 0 | 0 | 792 |
| Cylinder | **ERROR** | 196 | 154 | 42 | 0 | 3 |
| Decomposing Zombie | EMITTED | 0 | 0 | 0 | 0 | 743 |
| Deinonychus | EMITTED | 0 | 0 | 0 | 0 | 276 |
| Giant Wolf Spider | EMITTED | 0 | 0 | 0 | 0 | 448 |
| make_box | **ERROR** | 12 | 6 | 6 | 0 | 0 |
| make_overhang_table | **ERROR** | 135 | 106 | 29 | 0 | 0 |
| make_raking_blade | ERROR (both) | 16 | 8 | 8 | 0 | 0 |
| make_thin_fin | **ERROR** | 26 | 13 | 13 | 0 | 0 |
| make_toed_slab | ERROR (both) | 127 | 72 | 55 | 0 | 1 |
| Morel base | **ERROR** | 19,148 | 18,629 | 519 | 0 | 17 |
| Myconid Adult | EMITTED | 0 | 0 | 0 | 0 | 38 |
| Plague Zombie | EMITTED | 0 | 0 | 0 | 0 | 265 |
| Round 100 | **ERROR** | 15,549 | 15,227 | 322 | 0 | 4 |
| Round 25 | **ERROR** | 28,154 | 27,706 | 448 | 0 | 3 |
| Round 50 | **ERROR** | 48,146 | 47,419 | 727 | 0 | 6 |
| Skeleton_2 | EMITTED | 0 | 0 | 0 | 0 | 228 |
| Skulker Zombie | EMITTED | 0 | 0 | 0 | 0 | 275 |
| Sphere | EMITTED | 1,146 | 129 | 0 | 1,017 | 7 |
| Tavey | EMITTED | 0 | 0 | 0 | 0 | 802 |
| Tavey Badger | EMITTED | 0 | 0 | 0 | 0 | 82 |
| Tavey Base | EMITTED | 3,304 | 3,304 | 0 | 0 | 447 |
| Thorny Vegepygmy | EMITTED | 0 | 0 | 0 | 0 | 73 |
| Zombie Cat | EMITTED | 0 | 0 | 0 | 0 | 279 |
| zombie-1-a | EMITTED | 0 | 0 | 0 | 0 | 36 |
| zombie-3-a | EMITTED | 0 | 0 | 0 | 0 | 352 |
| zombie-5-a | EMITTED | 0 | 0 | 0 | 0 | 42 |
| zombie-6-a | EMITTED | 0 | 0 | 0 | 0 | 40 |
| zombie-8-a | EMITTED | 0 | 0 | 0 | 0 | 36 |
| zombie-9-b | EMITTED | 0 | 0 | 0 | 0 | 125 |

No model reached the 1500 s bound in the reported runs; longest 792 s
(`Crawler Zombie`).

### 5.4 The comparison that carries information: bytes

> ### 4 of 4 controlled artifacts are BYTE-IDENTICAL, control versus after.

| model | sha256 (both) | bytes |
|---|---|--:|
| `Bat Cat Owl` | `eaa775a9359a0861…` | 5,352,000 |
| `Myconid Adult` | `368c3f0f9104a7df…` | 3,314,040 |
| `zombie-1-a` | `1be894feb90e3fa8…` | 4,385,040 |
| `zombie-5-a` | `e41109737477bc16…` | 4,808,472 |

`Myconid Adult` carries 353 support records and `zombie-5-a` 412, so this is not a
smoke test on trivia. **Where the change does not refuse, it moves zero emitted
bytes.** That is the strongest positive result here, and it is what separates "the
new primitives compute something different" (they do not) from "the new refusals
change what gets built" (they do).

### 5.5 What is NOT compared, and why

* **No comparison against round 3's own artifacts is made.** Round 3's artifacts
  came from `develop @ 5d546cb` on 2026-08-25 with a `gate`-profile binary; the delta
  to `487e71f` contains days of unrelated engine work and cannot be attributed to
  anything here. `487e71f` is the only honest control, and it is the one used.
* **`tools/plinth-audit` was not run at all**, so no I1–I10 numbers appear in this
  document. That is deliberate. The audit tool has known defects of its own — its
  I8 dead-geometry check **has never passed any file, including the five hand-built
  known-good ones**, and 520 of its 954 I7 span failures are single pixels
  (`audit-baseline-round3.md` §0.1, §2). A delta in those checks is not evidence
  about this change, and quoting one would manufacture a comparison that carries no
  information. Byte identity (§5.4) and disposition (§5.2) are the comparisons that
  do carry information, and they are the ones reported.
* **The 7 non-emitting miniatures of round 3 were not run.** A model that newly
  *started* emitting would not appear here.

---

## 6. The triage verdict tables

`cargo run --release -p plinth-mesh --example triage_verdict_table`, compared with
`docs/evidence/geometry-consolidation/triage-verdict-table-after-c5.md`.

**Every corpus row is unchanged: verdict, component counts, digest, and every count,
all 8 models including the 429,120-face treant.** The single movement is a new
`near_par` column, added by this change to the example, which reads **0 on every
row**.

**And the probe still forces the path** — an unchanged table means nothing if
nothing exercised the code:

| | after-c5 (committed) | now |
|---|--:|--:|
| probe ray/triangle face tests, all rows | 1,659,744 | **1,659,744** |
| of which on `Weeping Treant v1.2` | 1,642,722 | **1,642,722** |
| probe `hits` / `misses` per row | unchanged | unchanged |
| probe `ambig` | 0 | 0 |
| probe `near_par` | — (column did not exist) | **0** |

So 1.66 M ray/triangle evaluations against real corpus triangles ran through
`ray_triangle_classified`, and **not one of them landed in the undecidable band**.
That is a real negative: on this geometry, driven by these rays, the classification
changes nothing.

It is also the sharpest contrast in this document. The same `make_box` that reports
`near_par = 0` here reports **12** near-parallel classifications in the pipeline run
of §5.3. Same mesh, same primitive, different rays: the triage probe casts from a
nested box centre, the support path casts capsule offset rays around members that
the orientation search has rotated. **The band's reachability is a property of the
ray family, not of the model** — which is exactly what a scale-relative bound would
neutralise and an absolute one cannot.

---

## 7. Build, tests, and the three-way split

### 7.1 Build

`RUSTC_WRAPPER="" cargo build --release` — **exit 0**.

`geom_kernel` emits **6** `dead_code` warnings where the control emits **5**. The
five pre-existing ones (`bvh::build_attempt_count`, `mesh::shares_geometry`,
`parity::referenced_face_ids`, `parity::axis_build_attempt_count`,
`queries::seg_surface_min`) are documented in
`geometry-consolidation-result.md` §"pre-existing". The sixth is new and
attributable: **`moller::moller` is never used** — the `(bool, f64)` projection of
the pinned primitive now has no production consumer, because every production walk
moved to `ray_triangle_classified`. That is the removal/twin table doing what it
said it would.

### 7.2 Suite totals

| run | binaries | passed | failed | ignored | cargo exit |
|---|--:|--:|--:|--:|--:|
| **after**, `--workspace --no-fail-fast` | 124 | **1,652** | **30** | 5 | 101 |
| **before** (control), `-p plinth-support -p plinth-hollow -p plinth-cli` | 20 | 897 | 1 | 3 | 101 |
| **before** (control), `-p plinth-pipeline -p plinth-orient -p plinth-mesh` | 30 | 237 | **0** | 1 | 0 |

Both invocations skip
`a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule`,
the uncapped-trial-count test `docs/development.md` records as never having been
observed to completion. **Still unmeasured, in both directions — not a pass.**
`--no-fail-fast` is required: without it cargo stops at the first failing binary
and reports a total that is missing most of the workspace. The first attempt at
this suite did exactly that and would have under-reported the failure count.

Every failing binary in the after-run belongs to one of the six crates covered by
the two control runs, so every failure below has a measured before-state.

### 7.3 Bucket 1 — introduced by this change: 29

**None of these is re-pinned.** Each is a behaviour change, not a moved constant,
and re-pinning a behaviour change would destroy exactly the information the test
exists to carry.

| # | cause | tests |
|--:|---|--:|
| a | the failure message NAMES `NearParallelWitness` — a refusal reaching a caller that expected a seal, an emission, or a different refusal class | **12** |
| b | model-anchor / brace / rewire / closure candidates that no longer survive candidacy on axis-aligned fixtures: the same witness refusal one level down, quarantining a candidate rather than naming itself in the panic | **11** |
| c | `UndecidedOpening` / "face crossings could not be decided" in `plinth-hollow`'s drilling | **4** |
| d | delicacy saturation (§7.5): one rotation-invariance break, one anchor delicacy cap | **2** |
| | **total** | **29** |

Classified by matching each failure's captured stdout, in that precedence order, so
a test whose dumped report merely mentions "delicacy" is not miscounted as one.

Named, with the assertion that moved:

* `per_chamber_drains::two_chambers_each_get_their_own_drain_and_vent` —
  `Drilling(UndecidedOpening { hole: 3, undecidable_faces: 4 })`. **The test file
  `crates/plinth-hollow/src/lib.rs` is byte-unmodified by this change** (`git diff`
  is empty for it) and `UndecidedOpening` does not exist at `487e71f`. A two-chamber
  cube that drilled before does not drill now.
* `the_shipped_report_can_tell_the_two_chambers_apart`,
  `the_engine_keeps_the_two_chambers_apart_through_drilling` — same refusal.
* `separated_drains_each_disconnected_chamber_on_its_own_after_the_real_preprocessing`
  — "centerline proof for opening 2 is undecidable: 15 face crossings could not be
  decided".
* `the_gate_dispatch_refuses_a_real_record_it_cannot_read_or_that_names_a_contact_twice`
  — "make_box seals and emits, which is the premise of everything below" — and it
  no longer does.
* `procedural_raking_blade_is_refused_before_any_file_exists`,
  `the_raking_blade_refuses_with_a_measured_class_per_route` — the blade is still
  refused, but by the near-parallel witness instead of coverage closure, so the
  *class* the tests assert has moved.
* `attempt_zero_names_every_stage_it_enters_and_the_trials_stay_quiet`,
  `the_loop_streams_progress_before_it_prints_its_summary` — "expected a
  coverage-closure refusal, got `Failed { phase: Preparation, detail:
  NearParallelWitness { near_parallel: 8 } }`".
* `runner::tests::canonical_empty_regular_selection_reaches_specialized_planning` —
  `Initial(Attestation(NearParallelWitness { near_parallel: 2 }))`.
* `support_ratchets_do_not_regress`, `a_sealed_run_refused_model_writes_no_artifact_and_exits_five`,
  `a_coverage_closure_refusal_is_not_masked_by_an_earlier_load_failure`,
  `a_coverage_refusal_removes_a_stale_artifact_already_at_the_target`,
  `shell_under_walled_solid_decision_still_reaches_a_written_artifact`,
  `initial::tests::an_incurable_refusal_is_quarantined_with_its_seat_load_and_every_remedy_disclosed`.
* `anchor::tests::{final_radius_is_used_and_can_reverse_the_provisional_breach_verdict,
  mixed_rebound_valid_and_breach_outcomes_scatter_in_request_order}`,
  `brace::tests::{a_seated_rung_on_a_raked_wall_stays_printable,
  model_rung_fallback_uses_real_surface_candidates}`,
  `cco::tests::{a_model_anchor_nub_is_refused_because_it_cannot_emerge,
  a_seated_model_anchor_attests_instead_of_kissing,
  detached_and_root_reanchor_model_anchor_replay_is_bit_exact,
  rewire_batch_is_indexed_stably_uses_one_clearance_batch_and_never_commits,
  the_seat_witness_refuses_a_model_anchor_at_candidacy}`,
  `runner::tests::braces_required_corpus_with_retained_rewire_evidence_and_named_termination`
  — model anchors and braces on axis-aligned fixtures no longer survive candidacy.
* `z_spin_is_a_null_direction_of_every_term`,
  `tests::the_committed_prepare_fixtures_stay_under_the_anchor_delicacy_cap` — §7.5.

### 7.4 Bucket 2 — expectations that legitimately moved: ZERO

**No expectation was re-pinned, and no digest was moved.** Every pinned digest in
the tree held: the corpus-model digests, the triage verdict table (§6), the
zero-field-divergence fixture, and the artifact bytes of §5.4. Nothing in this
document requires the reader to accept a changed constant.

That is the honest outcome. Every candidate for "an expectation that moved" turned
out on inspection to be a behaviour change with a measured before-state, and
belongs in bucket 1.

### 7.5 The orientation regression — separate, and not a near-parallel refusal

Two failures do not come from a refusal at all:

* **`plinth-orient` `z_spin_is_a_null_direction_of_every_term` (T6)** —
  "scar term under a 37° spin: expected 0.14400000000000002, got 0.5279999999999999
  (tolerance 1e-9)". This test spins a mesh about the up axis and requires every
  scoring term to be unchanged. It is not comparing against a stored constant; it is
  comparing a model against **itself under rotation**. There is no old-value /
  new-value to re-pin. **Rotational invariance is broken.**
* **`plinth-pipeline` `tests::the_committed_prepare_fixtures_stay_under_the_anchor_delicacy_cap`**
  — "make_overhang_table (procedural) peaks at delicacy 0.55, above the 0.5 anchor
  cap".

Both trace to one edit, `crates/plinth-mesh/src/delicacy_eval.rs:400`:

```rust
if cast.near_parallel[face] > 0 {
    by_face.push(1.0);   // saturate to the THIN extreme
    continue;
}
```

The reasoning in its doc comment is sound in isolation — an undecidable crossing
means the opposite wall may be nearer than anything measured, and delicacy exists
to protect fragile geometry, so saturating toward *more* delicacy is the protective
direction. The problem is not the direction. It is that **whether the band fires
depends on the mesh's rotation**, so a field that must be a property of the shape
becomes a property of its numerical presentation. Spin `make_overhang_table` by 37°
and the saturation fires on a different set of faces; the scar term goes from 0.144
to 0.528, a factor of 3.67.

`plinth-orient` was not edited by this change. The regression arrives through
`plinth-mesh`, which is why it was not caught by the "do not touch orientation"
scoping. T6 is the test that licenses searching orientations over S² instead of
SO(3); breaking it undermines the search space argument, not just a number.

### 7.6 Bucket 3 — pre-existing, unrelated: 1

`plinth-support/tests/support_runner_integration.rs::closure_repairs_censored_dust_because_that_repair_closes_a_real_hole`
— the deliberate red that reports FINDING 1, a live support-planning defect.
**It fails identically on the `487e71f` control**, which is how it is known to be
pre-existing rather than argued to be. Unchanged; left alone.

---

## 8. The guards

Both standing guards were verified the only way that means anything: a violation was
planted, the failure observed, the plant removed, and the green re-observed.
`git status` was checked afterwards and carries no trace of either plant.

### 8.1 Guard 1 — no Python

Planted `scripts/GUARD_PLANT_probe.py`:

```
test the_repository_contains_no_python_and_no_python_bindings ... FAILED
Python has reappeared in the repository (1 file(s)).
Offending files:
  scripts/GUARD_PLANT_probe.py
```

Plant removed (`find scripts -name '*.py' | wc -l` → **0**), re-run:
`test result: ok. 1 passed; 0 failed`.

### 8.2 Guard 2 — no inline geometry outside the kernel

Planted an unmarked inline 3-D dot product in `crates/plinth-core/src/lib.rs`:

```
test no_hand_written_geometry_outside_the_kernel ... FAILED
hand-written geometry found outside the kernel with no exemption marker and not in test code.
  crates/plinth-core/src/lib.rs:68 (dot-product-3d)
```

Plant removed (`git diff --stat crates/plinth-core` → 0 lines), re-run: **3 passed,
0 failed**, with the counted row

```
Guard 2: scanned 91 .rs files across 13 src roots; 20 raw pattern hits
  -> 20 test-code (MAY-STAY), 0 GEOM-EXEMPT marked, 0 violations
```

91 files and 20 hits, reproducing §O6's revision-1 measurement exactly. A guard that
reports zero violations having scanned zero files is the failure mode this project
keeps reproducing; this one states its file count and asserts it is non-vacuous.

### 8.3 The outstanding claim about Guard 2 — measured, and it is HALF true

The claim under review: *the inline-geometry guard matches function declarations and
`.mul_add` rather than the ordinary inline expressions it exists to catch.*

Measured by running `tests/guard_scripts/no_inline_geometry.sh` directly against a
purpose-built file containing one instance of each shape:

| planted construct | caught? |
|---|---|
| ordinary inline 3-D dot `a[0]*b[0] + a[1]*b[1] + a[2]*b[2]` | **yes** (`dot-product-3d`) |
| ordinary inline 3-D cross | **yes** (`cross-product-3d`) |
| ordinary inline 3-D squared length | **yes** (`squared-length-3d`, plus a `2d` sub-match on the left association) |
| ordinary inline 2-D squared length | **yes** (`squared-length-2d`) |
| inline body of a `fn dot(...)` declaration | **yes** — caught as the inline expression it is, not as a declaration |
| **inline 2-D dot** `a[0]*b[0] + a[1]*b[1]` | **NO** |
| **inline 2-D perp-dot** `a[0]*b[1] - a[1]*b[0]` | **NO** |
| **`.mul_add`-composed 3-D dot** | **NO** |

So the claim **as stated is refuted**: Guard 2 catches the ordinary inline
expressions, with metavariables, so variable naming is irrelevant. It is the
per-crate string guards (`tests/no_private_vector_algebra.rs`) that match `fn dot(`
and `.mul_add(`, and the claim describes only those.

**But the gaps §O6 upheld are real, they are measured above, and NONE of them is
closed in this working tree** — that is a finding:

1. **No `DOT2D` or `CROSS2D` pattern** in `no_inline_geometry.sh`. §O6 specifies both
   and predicts zero production hits after the `perception.rs` retarget. The
   retarget landed (`geom_kernel::poly2::{dot, perp_dot}` exists and
   `perception.rs` calls it); **the two patterns did not**. A 2-D dot or perp-dot
   written inline anywhere outside the kernel is still invisible.
2. **The three missing per-crate string guards.** Counted:
   **10 of 12 workspace crates carry `tests/no_private_vector_algebra.rs`**;
   `plinth-cli`, `plinth-verify` and `rust/chitubox_io` do not. `plinth-verify`
   computes gate verdicts, which §O6 calls "the worst possible location for the
   blind spot". All three currently contain **0** occurrences of `.mul_add` in
   `src/`, so all three would land green today — the cost of adding them is
   nothing and the gate is still absent.
3. **The kernel exempts itself** — already recorded as open item §8.4 of
   `geometry-consolidation-result.md`; unchanged, not expanded.

---

## 9. Open items

1. **Eight lost emitters, unresolved by design.** No tolerance was adjusted. The
   decision — accept the losses, bound the band relative to scale, qualify the
   classification by proximity, or route this consumer somewhere other than a total
   refusal — is the owner's.
2. **The orientation scar term is no longer rotation-invariant** (§7.5). This is
   the most serious item after the emitter losses, because it breaks a property the
   search space argument rests on, and it arrived in a change that was scoped not to
   touch orientation.
3. **`designed_embed.rs:334`'s comment is wrong.** It says the near-parallel refusal
   is "candidate-granular … not a pipeline failure". On the committed-member path
   through `initial.rs:1331` it is a whole-model failure, measured on eight models.
4. **How much of the 115,959 is genuinely undecidable is not measured.**
   `ray_triangle_classified` classifies on `|det|` alone with no proximity test, so
   a ray that misses a triangle entirely can still be counted uncertain about it.
   Sizing that fraction is cheap and should precede any ruling on item 1.
5. **§O6's three guard fixes are not implemented** (§8.3). The 2-D patterns and the
   three per-crate guards.
6. **`a_spec6_removal_...` remains unmeasured** in both directions.
7. **`kernel.seg_tri.pierce` fired on exactly one corpus model.** Its 1,017 events
   all come from `Sphere`. Whether the site is genuinely near-dead on real geometry
   or the corpus simply does not reach it is not established.
8. **The 7 non-emitting round-3 miniatures were not re-run**, so a model that newly
   *started* emitting would not be visible here.
