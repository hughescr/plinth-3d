# Engineering journal archive — 2026-08-09

Date range: 2026-08-09. Moved verbatim from CURRENT-STATUS.md 2026-08-12.

<!-- Source line ranges, ascending source order: [1728,1860], [1861,1960], [6327,6503], [6504,6599], [6600,6706], [6707,6781], [6782,7014], [7015,7279], [7280,7407], [7408,7548], [7549,7921], [7922,8112] (source CURRENT-STATUS.md as of 2026-08-12, pre-split). -->
<!-- Non-contiguous grouping: this file opens with [1728,1960] (prepare-execution / pre-write-gate, which sit far earlier in the source), then continues with the [6327,8112] 2026-08-09 bulk; each internal range remains in ascending source order. -->
<!-- Closed follow-up rows carried in this file's "Closed follow-up rows" table below: FU-26, FU-32. One link in the FU-32 row was rewritten from an in-page anchor to a relative sibling path (target now lives in STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md); no other edit was made to row text. -->

## 2026-08-09 — `plinth prepare` executes, and it cannot yet orient anything

<a id="prepare-execution-2026-08-09"></a>

Design phases E4 and E5 (`export-path-design-2026-08-07.md`) land together.
`plinth_pipeline::prepare(PrepareRequest) -> Result<PrepareReport, PipelineError>` is
real; `crates/plinth-cli` no longer prints "execution is not implemented".

**The composition moved rather than being copied.** The whole end-to-end chain lived in
`crates/plinth-pipeline/examples/shared/first_artifact_chain.rs`, reachable only from a
test and an artifact-producing example. It is now `crates/plinth-pipeline/src/prepare.rs`,
and the example's shared module is a thin corpus-fixture wrapper over it — so the code the
first-artifact test gates and the code the CLI runs are one implementation (O-5). All five
`tests/first_artifact.rs` cases still pass unchanged, including the same three gate
refusals, which is the evidence that the promotion was behaviour-neutral.

**What the executor guarantees.** Sources are `*.stl` in `input_models_dir`, enumerated in
sorted order; each emits `<stem>.chitubox` into `output_dir`. **One bad model never aborts
the folder**: a load failure, an unorientable model, a stage error and a gate refusal are
all per-model dispositions in `PrepareReport`, and the returned `Err` is reserved for
failures of the *run* (unreadable input folder, uncreatable output folder, unassemblable
resource policy). **No model's artifact path survives its own run**: every model's target
is cleared before that model is attempted, so a folder re-run after a model started failing
reports the failure *and* the earlier artifact is gone. A gate refusal writes no file and
projects to `PipelineExitCode::Gates = 3`. The run's exit code is `Gates` if **any** model
in the run was gate-refused, and otherwise the first non-`Ok` disposition in report order.
`Gates` is the only sticky code: it is the one code with a documented meaning callers act
on, and first-in-order alone would let a `._sidecar` that fails to load mask a physics
refusal behind exit 1. Every other code stays first-in-order, because the exit-code
contract defines no severity ranking over them and inventing one would be a second
contract.

**The run report carries the assumed print size.** Every model that reached decimation
reports `trace.scale.longest_axis_mm` and its bounding box, on the success path as well as
on refusals. A mini silently processed at a tenth of its intended size produces a
completely plausible artifact and an unusable print, and this line is the only place that
is visible.

**The blocker, stated plainly: the CLI can prepare nothing today.** `parse_args` hardcodes
`OrientationPolicy::LegacyCoarse`, the ratified command line has no `--orient` flag, and
the native coarse orientation search of `02-target-architecture.md` §6 is unported. So
`plinth prepare` reaches a per-model orientation refusal for every model — named,
non-fatal, and never a silent identity, because choosing identity in the search's absence
would be *inferring* an orientation and `plinth-orient` refuses to do that even for
identity. **`prepare` itself is complete**: under `OrientationPolicy::Pinned` it produces a
real `.chitubox` that survives the independent disk audit and the `plinth-verify` format
gate (`crates/plinth-pipeline/tests/prepare_folder.rs`). The only missing piece between
here and a usable `plinth prepare` is the `--orient pinned:<baseline>` flag §6 already
specifies, which is a parser change and therefore a separate landing.

**A fixture finding, recorded rather than papered over.** No checked-in
`corpus/baseline/*.prepped.stl` is refused by the pre-write gates: both
`make_overhang_table.prepped.stl` and `Sphere.prepped.stl` seal and emit through the whole
chain, while the procedural `make_overhang_table()` and the provisioned
`corpus/models/Sphere.stl` are refused for uncovered islands. That is consistent — a
`.prepped.stl` is the legacy prep's *output*, already oriented and cleaned — but it means
the folder-level refusal test is driven by a procedural mesh written out as a binary STL.

**Three constants were registered and no number moved.** `ProbeConfig::new(1.0, 64 MiB,
10_000.0)` was restated by hand at 18 call sites and never registered. `prepare` reads it
in production, so it is now `DECIMATION_PROBE_GRID_PITCH_MM`,
`DECIMATION_PROBE_MAX_GRID_BYTES` and `DECIMATION_PROBE_NEAREST_UPPER_BOUND_MM`
(Placeholder), read through `plinth_pipeline::registered_decimation_budget`. The registry
is 290 constants (259 Placeholder). A unit test asserts the registered budget equals the
hand-written one every gate already used, which is why no decimation digest moved. The run
lift is likewise read off the profile's registered `LIFT_MM` rather than restated, and the
layer pitch off `LAYER_HEIGHT_DEFAULT_MM`.

**No cap was raised and no gate was relaxed.** `judge_then_emit` is still the only door to
the gate set and still takes no override parameter.

### 2026-08-09, second pass — the review's seven, and the one that is a follow-up

The landing above was reviewed and seven findings were fixed on the same branch. They are
listed here because five of them are *silent-substitution* defects, which is the class this
whole export path exists to eliminate, and one of them changes a documented contract.

**Stale artifacts no longer survive a pre-gate failure (BLOCKING).** `judge_then_emit`
owned the no-file guarantee only for models that reached it. A model that stopped earlier —
load, decimation, scale sanity, orientation, layer planning, the sealed support run — never
touched its target, so re-running a folder whose model had started failing reported ERROR
while leaving the previous run's `.chitubox` standing for the next reader to take as this
run's answer. Every model's target is now cleared once, at the top of `prepare_one`, before
anything can fail; a removal that itself fails is `PipelineError::StaleArtifact`, a named
per-model failure, never swallowed.

**The oriented-solid reserve answers only a support refusal.** The guard was
`Err(stop) if stop.is_gates()`, so `Lowering`, `Emit` (the post-write disk audit refusing
the bytes), `LayerPlan` and `DegenerateExtent` all took the fallback: the chain re-planned
the oriented solid, wrote it to the same target, labelled it `DrilledShellRefusedBySupport`
with an unrelated error as the "refusal", and reported success. The reserve's own claim is
"this *geometry* could not be sealed", so it now fires only on `SealedRun`/`Support`.

**The hollow-stage fallback no longer launders configuration errors.** Case 0 converted
*every* `HollowError` into `solid_decision(BelowThreshold)` and emitted — including
`PrinterProfileMismatch`, `WallBelowVoxelPitch`, constants-registry failures and the voxel
resource caps. An operator who passed a parse-valid but inadmissible `--wall-mm` got a solid
artifact reported as success instead of the error naming their request.
`hollow_error_refuses_this_geometry` now classifies the variants exhaustively and without a
`_` arm, so a new `HollowError` is a compile error rather than a silent inheritance of the
fallback; only geometry refusals (the `InvalidShellTopology` class OQ-1/FU-34 authorized)
reach the reserve, and resource-cap refusals explicitly do not (FU-8b: a cap that was
reached is a finding, not a reason to take another route to an artifact).

**A successful artifact now says what it is made of.** `ModelPreparationOutcome::Artifact`
carried only `{ emit }`, so an artifact produced by either fallback was indistinguishable in
the report from the intended drilled shell — the exact substitution `Refused`'s `geometry`
field exists to expose. It now carries `geometry` and `hollow_disposition`, and the CLI
prints them, spelling substitutions `SUBSTITUTED solid -- ...` rather than a bare `EMITTED`.

**A pinned baseline is per model, not per run.** `PrepareRequest.orientation` carried ONE
`OrientationPolicy::Pinned` baseline for the whole folder, and `plinth_orient::run` applies
the recorded rotation to whatever mesh it is handed without binding it to the source — so a
heterogeneous folder silently received one model's rotation for every model, and a
misoriented mini still seals, still gate-passes and still emits. `OrientationPolicy::Pinned`
is now a `BTreeMap<String, PinnedBaseline>` keyed by source stem, which is how the proven
composition always resolved it; a model the map does not name is
`PipelineError::OrientationBaselineMissing`, a per-model refusal, never a borrowed rotation.
**This binds the `--orient` follow-up**: the flag must name a baseline per model, not one
for the run.

**`stop_after` has the per-stage tests E4's acceptance item asked for.** All eight branch
points, every `Stopped` disposition, the `Ok(None)` protocol between `prepare_one`,
`run_from_oriented` and `seal_and_emit`, and the `expect()` guard that turns a broken
protocol into a whole-run panic were unexecuted workspace-wide.

**Still open, and it is the product-level blocker: the CLI can prepare nothing.** The
`--orient pinned:<baseline>` flag of `02-target-architecture.md` §6 is unbuilt, so
`parse_args` still hardcodes `LegacyCoarse` and every model on every invocation takes the
orientation-refusal path. It is a parser change and its own landing, and it must also add
the two process-level tests that are impossible until it exists: a real `.chitubox` produced
through the binary, and an observed exit 3.

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

## FU-8g phase H2 landed 2026-08-09 — the charge IS the counted demand, the receipt is checked against it, and the refusal path is proven at caps nobody touched {#fu8g-h2-2026-08-09}

**Scope, stated first.** H2 is the charging phase of
[`fu8g-design-2026-08-08.md`](../fu8g-design-2026-08-08.md) (its H2 row: preflight rebase,
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
([`#fu8g-h0-2026-08-08`](STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu8g-h0-2026-08-08)) §H0.1, and the treant row from the FU-31
acceptance run recorded in *FU-31 FIXED 2026-08-08*
([`#fu31-fixed-2026-08-08`](STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu31-fixed-2026-08-08)). **This is arithmetic on measurements
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
[`support-resource-caps-2026-08-05.md`](../support-resource-caps-2026-08-05.md). The design doc
[`fu8g-design-2026-08-08.md`](../fu8g-design-2026-08-08.md) is **not** amended: it was landed
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
[`fu8g-design-2026-08-08.md`](../fu8g-design-2026-08-08.md) (its H3 row: M0 registration + record
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
[`fu8g-design-2026-08-08.md`](../fu8g-design-2026-08-08.md) named independent of the kernel work (its
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
([`fu8g-design-2026-08-08.md`](../fu8g-design-2026-08-08.md), H5 row — "folding the proof into the
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
([`fu8g-design-2026-08-08.md`](../fu8g-design-2026-08-08.md), H6 row — "**no engine code**"). **No
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

**Opened as [FU-32](../CURRENT-STATUS.md#follow-ups-2026-08-04).** Naming the remedy is not this phase's business, but
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
([`#fu10-repin-2026-08-08`](STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08)).

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
protocol order ([`#fu10-repin-2026-08-08`](STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08)). The evidence it was made
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
including the 2026-08-08 table at [`#fu10-repin-2026-08-08`](STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08), which was
the authority until this line. The same holds for the three in-workspace pins that moved with
them — the seal-test tuples `13346779153602682749` / `8660653339934460766` and the frozen
coverage report's `5470664714994646368` — wherever they are cited, including that section's
re-pin table and the verbatim-landed evidence record
[`attribution-slicer-census-2026-08-07.md`](../attribution-slicer-census-2026-08-07.md), which is
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

## Closed follow-up rows

| # | Follow-up | Note |
| --- | --- | --- |
| FU-26 | `nonmanifold_edges` under-counts, and duplicate faces are counted nowhere | **Opened 2026-08-07 by the FU-25 Phase-1 diagnosis** (`decimation-design-2026-08-06.md` §8.6), deliberately **not** fixed there because Phase 1 must not move behaviour. Two parts. **(1)** `build_face_topology` reads `cut_flags` for its duplicate-face exclusion *inside the same loop that writes it* (`crates/plinth-mesh/src/topology.rs:339-341` reads, `:371-373` writes), so a face that shares an edge with a genuine duplicate face acquires `TOPOLOGY_CUT_DUPLICATE` part-way through and any **later** edge key touching that face is dropped from the count. Deterministic (`BTreeMap` key order) but order-dependent, and it under-counts: on decimated treant candidates 24 against a frozen-snapshot 25 at r = 0.911, and 170 against 208 at r = 0.50. **(2)** Larger: duplicate faces are excluded from `nonmanifold_edges` by design and counted **nowhere else**, so `topology_not_increased` (`crates/plinth-decimate/src/lib.rs:1843`) cannot see them at all — 664 duplicate-face groups at r = 0.50 pass that gate unremarked. Together these make the gate's number a **floor** on the damage, and the gap widens with reduction. This is a *counting* defect, not a licence to relax the gate: `stitch_layer` genuinely requires degree-2 adjacency. Fixing (1) will move the reported count on any decimated mesh, so it needs the usual re-pin discipline — no corpus digest depends on it today, because no corpus model decimates. Note: the number `FU-26` was assigned on `feature/decimation-followup` while `FU-24` was in flight elsewhere; renumber at merge if it collides — the finding, not the number, is the record. **[Headline evidence RE-DERIVED 2026-08-07 from a committed instrument; everything above stands except one face count.** The corpus gate's report schema is bumped to `plinth/m3-decimation-corpus-budget-report/v4` and now serializes `max_mm` on every probe, so this finding no longer rests on scratch instrumentation that was reverted. Re-run at the `feature/decimation-followup` tip, the r ≈ 0.911 treant candidate is **390,898 faces, not 390,896** — the search's bisection asks meshoptimizer for 390,901 faces (`429,120 × 0.91093749999999996`) where the reverted scratch asked for 390,898, and the two undershoot by 3 and 2 faces respectively. §0(b)'s original live report already said 390,898; the scratch was the odd one out. **The topology conclusion is unchanged:** `build_face_topology` reports **24** non-manifold edges and 0 boundary edges at that candidate, exactly as recorded, and the disposition is still `TopologyIncreased` before the witness is consulted. Attribution, for whoever lands the fix: the **24** and the **170** are engine output and are re-derivable from the v4 report by anyone with a clean checkout; the **25**, the **208** and every duplicate-face-group count (109 at r = 0.911, 664 at r = 0.50) are scratch measurements of 2026-08-07 and are *not* re-derivable, because no committed instrument counts them — which is this finding. Land the counter first, then the comparison becomes reproducible rather than remembered. Correction and re-derivation: [`decimation-design-2026-08-06.md`](../decimation-design-2026-08-06.md) §8.9.]** **[LANDED 2026-08-09 on `feature/fu26-topology`; everything above stands.** `build_face_topology` now takes its counts in a **second pass**, after every cut flag is written and against a **frozen duplicate-face snapshot** built before any edge is classified — the same "frozen-snapshot classification" the §8.6 table compares against, now the engine's own instrument rather than a scratch one. `TopologyCounts` (was `EdgeTopologyCounts`) carries two new fields, `duplicate_faces` and `duplicate_face_groups`, so part (2)'s 664 groups have a committed counter and the corpus gate's report schema goes to `plinth/m3-decimation-corpus-budget-report/v5`. **The counter is the only thing that moves.** The flag-writing pass is byte-for-byte what it was, including the propagation that gives a face meeting a duplicate its own `TOPOLOGY_CUT_DUPLICATE` — that propagation only ever *severs* adjacency, so removing it would relax cuts, and this finding is explicitly "a *counting* defect, not a licence to relax the gate". Adjacency, `component_ids`, `cut_flags` and the v4 topology digest are unchanged; all seven frozen OD-8 census digests re-measured byte-identical. `topology_not_increased` still binds on the two edge counts alone: whether a reduction may introduce duplicate faces at all is a separate gate decision with its own corpus consequences, and adding the clause could only ever refuse *more* candidates. **One caveat is named here rather than left to surface:** the monotone strengthening this commit buys belongs to the per-mesh **counter**, not to the gate. `nonmanifold_edges` for a given mesh can only rise — the frozen-snapshot exclusion is strictly narrower than the propagated-flag exclusion it replaces — but `topology_not_increased` is the *comparison* after, `after.nonmanifold_edges <= before.nonmanifold_edges`, and both operands are raised. Where the source mesh is dirtier than its candidate (duplicates in the original that decimation largely fuses away), `before` can rise by more than `after`, so a candidate the old counts refused could now pass: an old 8 > 0 refusal becomes a new 8 <= 100 acceptance. That is unreachable in the current corpus — all eight models take `already_print_scale`, where `before == after` — and the corrected counts are the right basis for the comparison in any case, so no code changes for it; it is recorded so the first genuinely dirty model does not surface it silently. Two tests pin it, both in `crates/plinth-mesh/tests/topology_triage.rs`: `a_late_nonmanifold_edge_survives_a_duplicate_flag_an_earlier_edge_wrote` is the 24-vs-25 class in five faces and reads 0 against the old predicate and 1 against the new; `duplicate_faces_and_their_groups_are_counted_and_a_clean_mesh_reports_none` pins the group arithmetic and the fact that the flag is deliberately broader than the count.]** **[The deferred gate decision is MADE 2026-08-11 on `feature/decimate-dupfaces`; everything above stands.** "`topology_not_increased` still binds on the two edge counts alone … a separate gate decision with its own corpus consequences" was the right call to defer and the wrong state to leave standing: a second review executed the branch and found the decimator emitting a *verified* `Tavey Base` candidate carrying 10 manufactured `TOPOLOGY_CUT_POINT_ONLY` faces from a source clean in all six classes — refused downstream by `validate_closed_mesh`, which refuses any nonzero cut flag, so the model was pipeline-refused *because of* the stage meant to help it. The predicate now binds on **every** field of `TopologyCounts`, which gains `point_only_cut_faces`, `same_direction_cut_faces` and `degenerate_cut_faces` (one pass over the finished `cut_flags`, same canonical build, topology digest unmoved), and destructures the struct exhaustively so a future count cannot be added without a verdict. Corpus consequences, measured: **none** — all eight models stay `already_print_scale`, and the seven local clean-raw minis stay verified with 0 → 0 in all six classes, `Tavey Base` moving to a different ratio (73,582 → 38,630) rather than being surrendered. Report schema v9.]** |
| FU-32 | CCO routing re-digests the whole accumulated transcript once per terminal — the batch-first violation family a sixth time, and the first one no cap could have caught | **Opened 2026-08-09 by FU-8g phase H6** ([`#fu8g-h6-2026-08-09`](#fu8g-h6-2026-08-09)), the run that first carried the Weeping Treant past `initial::build_initial_contacts`. With the initial-contact wall gone (H1/H2's honest charge: **108,378 vs the untouched 4,000,000**, 2.71 %), the treant's stopping point is no longer a refusal at all: both attempts ran **EXCEEDS-TIMEBOX at 600 s** with the main thread **100 % inside `PreparedSupportRun::route_initial_cco`** from t ≈ 267 s to the kill. **Measured, not argued:** across sample windows at t = 280 / 360 / 440 / 520 / 575 s the share of routing wall-clock in `CcoController::digest` **rises** 19.7 % → 64.4 % → 77.2 % → 77.2 % → 77.1 % while the constant-per-terminal `validate_replay` term's share **falls** 31.0 % → 13.8 % → 9.0 % → 9.0 % → 9.0 % — a growing term overtaking a constant one, reproduced across three windows 135 s apart. **Derived to the line from source, independently:** the routing loop (`runner.rs:1650–1666`) calls `grow_terminal_with_finalized_anchor` once per terminal; that function's **first statement** (`cco.rs:1397`) is `let controller_digest_before = self.digest();`; and `CcoController::digest` (`cco.rs:1775–1862`) is not the cached `const fn` at `:863` but a full walk of the accumulated controller state — every terminal in `terminal_cadence` (`:1786`), every record in `terminals` (`:1790`), each `UnroutedNoWinner` record's **entire** anchor-refusal list via `digest_anchor_seat_refusals` (`:1818`, `:1824`, `:1834`; five words per refusal at `:2128–2149`) and `digest_anchor_refusals`, then the whole host kd-tree (`indexed_members` `:1851`, `samples` `:1856`). Cost `Θ(N²) + Θ(N·R) + Θ(N·H)` for `N` cadence terminals (553 specialized physical contacts plus the regular terminals appended at `runner.rs:1649`), `R` accumulated refusals, `H` host samples. The `Θ(N·R)` term dominates on this model, consistent with the standing record that **0 of 3429 attempted model anchors survive corpus-wide**, which makes `UnroutedNoWinner` common and refusal lists long. **A second, independent per-terminal re-digest sits in the same loop** and is the ~9 % constant term: `validate_replay` → `validate_finalized_anchor_route` → `BoundPhysicalAnchorContact::new` → `SpecializedContactPlan::digest` → `GenericContactPlan::digest` → `ContactCandidatePool::digest`, plus `SurfaceAnchorPool::validate_against` → `surface_anchor_pool_digest` and `FinalizedContactLoads::validate_binding` → `finalized_load_rows_fingerprint` — **every one of those structures is immutable for the whole routing stage and every one is re-hashed from scratch per terminal.** Together ≈86 % of the stage. **This is NOT a cap and no cap would fix it**: nothing refuses, no resource is over-charged, no limit is consulted — which is why the measurement-first doctrine found it with a profiler rather than a preflight, and is the reason this row exists. **The remedy is not this row's to choose**, but the shape bounds it: both terms re-derive state whose deltas are already known at the call site, and the house precedent is incremental maintenance (FU-8f G3's prefix evaluation, G6's cross-round retention), not a bigger budget. **FIRST ACTION — and it is not the fix.** H6 also found that **an EXCEEDS-TIMEBOX run is structurally counter-free**: `plinth_support::census` flushes only from `census::finish` (model completed) or `census::report_failure` (typed refusal), there is **no signal handler anywhere** in `census.rs` or `examples/embed_incidence_census.rs`, and the watchdog kills with `SIGKILL` — so **both attempts emitted zero `CENSUS` rows**. The design's "EXCEEDS-**with counters**" outcome is therefore not achievable with the present harness on the very outcome it ranked most likely (~45 %), H4's rider (`rounds_committed`, `exit_reason`, the three-way `PoisonCause` split) **could not fire**, and the treant's **21 poison-driven rebuilds remain unattributed**. Give the census a periodic or signal-triggered flush *before* the next treant run; otherwise the next run buys another verdict with no evidence. **Memory is not the problem and should not be treated as one:** whole-process peak was **3,721 MiB**, 4.0 % of the 90 GB watchdog arm — H1's per-row reserve collapse confirmed at treant scale, against a design prediction of 14–32 GB under the old reserves. Routing's own growth (+1,591 MiB over 332 s, ≈4.8 MiB/s) is honestly rows-scaled transcript accumulation, and binds nothing. **The brace/joints/raft caps registered by H3 remain wholly unexercised** — the run never reached brace — so §18's statement of absence stands unchanged and this row licenses no brace number. **NO CAP RAISED, LOWERED OR TOUCHED**, and D5.3's cap-scaling contingency is not triggered and not added. See *FU-8g phase H6 measured 2026-08-09* and `support-resource-caps-2026-08-05.md` §19. The treant has **no seal**. **[J0–J3 LANDED; THE MOVEMENT IS ATTRIBUTED AND THE RULING IS PENDING — 2026-08-09 (**[`#fu32-j3-2026-08-09`](#fu32-j3-2026-08-09)**).** J0 (`41246f6`) gave `plinth_support::census` a periodic **and** signal-triggered flush, so the counter-free killed run this row opened against is fixed at its root. J1 (`059b444`) hoisted the immutable routing authority (`VerifiedRoutingAuthority`, the G2 precedent applied verbatim) — **value-preserving; all seven OD-8 digests byte-identical**. J2 (`c14754e`) replaced `CcoController::digest`'s per-terminal whole-state walk with the design's state-pure incremental accumulators, kept **both F6 tamper tripwires genuinely recomputing** through `rebuild_accumulators()` (`runner.rs:2775`, `:2848`), and **MOVED ALL SEVEN frozen OD-8 snapshot digests**. **The sentence above stating the treant has no seal was written before J2 and stands as the record of its date: the Weeping Treant SEALED for the first time at J2** — complete pipeline (routing, rewire, brace, seal) in 463 s wall-clock, peak RSS ≈5.4 GiB (6 % of the 90 GB arm), `routed_contacts=306`, `final_live_members=1383`, `snapshot_digest=423276951048565625` — **a first measurement, not a pin**, and §A2's predicted next binder (`rewire::optimize`, ~55 %) did **not** bind. J3 adds **no engine code** and is the FU-10 attribution: the digest movement is **DECISION-NEUTRAL**, proved twice over — from source (all 30 `controller_digest_before` occurrences audited at this tip; **zero comparisons outside `#[cfg(test)]`**, and `finalize_transcript_row`'s admission check reads six other fields and deliberately not this one) and by parent-vs-tip routing-output dumps on all seven corpus models (**275 transcript rows and 687,288 content lines byte-identical**; of the transcript's 10 header digests only the 3 that fold the moved value move, and of `verification_snapshot_digest`'s 17 inputs exactly 1 does). **Three code pins and one golden will need re-pinning at J4**, enumerated by name in the J3 section — and **NOTHING is re-pinned, no golden refreshed, no doc table rewritten and no test expectation relaxed** pending the owner's ruling. **NO CAP, floor, constant or registry entry RAISED, LOWERED OR TOUCHED.** Gates 1058 default / 1063 all-features with **exactly** the two expected digest-pin failures and no others; clippy, fmt and `git diff --check` clean. **Still open: J4 (owner ruling, re-pin, record sweep), J5 (the honest routing charge, §D3) and J6 (the treant measurement and the record).**]** **[J4 LANDED; THE OWNER RULED AND ALL SEVEN ARE RE-PINNED — 2026-08-09 (**[`#fu32-j4-2026-08-09`](#fu32-j4-2026-08-09)**).** Asked whether the value-changing digest fix may proceed given that all seven OD-8 fingerprints move, the owner ruled, verbatim: *"The fingerprints aren't sacred  if things change slightly that's to be expected (because the algorithm wasn't right when the fingerprints were generated!)"* — sought after J3's attribution and before any re-pin, which is FU-10 order. **The new OD-8 authority table is in the J4 section** and supersedes the 2026-08-08 one at [`#fu10-repin-2026-08-08`](STATUS-ARCHIVE-2026-08-06-2026-08-08-part2.md#fu10-repin-2026-08-08): Sphere `2297157889062365577`, Cylinder `6853793710942395608`, make_box `8709076999423814412`, make_overhang_table `16396858890798947253`, make_toed_slab `133000971864625681`, make_raking_blade `18388631460456455184`, make_thin_fin `14656364680512457190` — **all seven digests move, NOT ONE count moves**, which is exactly what separates this movement from the 2026-08-08 one (that one moved two models' counts because real island births were removed). Three code pins re-pinned from J4's **own** re-measurement rather than hand-copied (`support_runner_integration.rs` toed_slab `13_346_779_153_602_682_749 -> 2_198_459_883_570_957_390` and raking_blade `8_660_653_339_934_460_766 -> 74_030_269_957_898_085`), and the workspace's one golden **regenerated through its own producer path, never hand-edited** (`coverage_gate_overhang_table.json` `snapshot_digest 5470664714994646368 -> 3706765241747593242`; diffed field by field afterwards — 185 leaves, exactly 1 moved, no evidence lost). `verification_snapshot_digest_input_list_is_pinned` was deliberately **not** touched and stayed **green**: the digest's input *shape* did not widen, only one input's value moved. Dated supersession lines were appended at every doc site quoting any of the seven superseded values (107 occurrences across 9 files); **no earlier citation was edited or deleted**. Census re-run at this tip reproduces the new septet byte for byte — the third concordant measurement — with `routed_contacts` and `final_live_members` unchanged on every model. Gates **1060 default / 1065 all-features, ALL GREEN**, zero failures; clippy, fmt and `git diff --check` clean; **J4 adds zero tests and moves zero counts. NO CAP, floor, constant or registry entry RAISED, LOWERED OR TOUCHED. Still open: J5 (the honest routing charge, §D3 — an addition, not a move) and J6 (the treant measurement and the record).**]** **[J5 LANDED; ROUTING IS CHARGED — 2026-08-09 (**[`#fu32-j5-2026-08-09`](#fu32-j5-2026-08-09)**).** The sealed CCO routing cadence, which had **no `Limit`, no counter and no clock**, now carries a counted preflight charge against **two NEW registered caps** — `SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS` and `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS`, both 536,870,912 by one stated rule — with a meter beside the charge and a receipt (`executed ≤ charged` or `ChargeDrift`). Registry 284 → **286**; caps-doc §21, physics-doc §3.10.11, FU-24 ledger rows and a producer-regenerated `constants.md` in the same commit. **Result-inert: OD-8 seven byte-identical, `routed_contacts`/`final_live_members` unmoved.** Gates **1066 / 1071 ALL GREEN** (+6). **NO EXISTING CAP RAISED, LOWERED OR TOUCHED** — the two are additions to a stage that had none. `rewire::optimize`'s absent budget opened as **FU-33**, deferred not skipped.]** **[J6 MEASURED — FU-32 IS COMPLETE, J0–J6, AND THE TREANT SEALS TWICE UNDER THE STANDING MEMORY PROTOCOL — 2026-08-09 (**[`#fu32-j6-2026-08-09`](#fu32-j6-2026-08-09)**).** This row's headline sentence — *"The treant has **no seal**"* — was written at H6 and is **superseded**, first by J2's seal (the first in project history, 463 s, unprotocolled) and now by the measured record. **Two attempts, both COMPLETED: 470.83 s and 470.64 s of the 600 s box, exit code 0, peak RSS 5,585.6 / 5,610.9 MiB = 6.06 % / 6.09 % of the 90 GB arm, which never fired.** Seal tuple identical in both runs and to J2's: `routed_contacts=306`, `final_live_members=1383`, `snapshot_digest=423276951048565625`; four self-proving identities hold in the transcript (CCO winners 199+4+103 = 306; surviving model anchors 4 = committed 4; `T5` members sum to 1,383; stubs 306). **Determinism, the G6d standard: 13,441 terminal census rows per run, SHA-256 identical, ZERO differing rows**, wall-clock within 0.04 %. **THE WALL THIS ROW WAS OPENED AGAINST IS GONE:** the cadence loop that consumed ≥335 s without finishing at `bfa5b3b` — and whose hashing §M4 projected at ≈691 s — completes in **≤20.7 s**, bracketed by J5's own preflight-charge and receipt census rows. J5's charge/receipt reproduce **to the unit** (164,993,019 evaluations and 162,719,910 retained rows, 30.7 % / 30.3 % of the untouched cap). **Predictions scored:** §A2's ~55 % call (rewire binds next) **MISSED** — rewire cost 41.5–46.5 s, 8.8–9.9 % of the run, with its evaluated candidates bounded by the census at 619 clearance rows and 3,076 anchor attempts against a 364,223-row pool; the per-move shape the design named was right, the number of moves was never estimated, and **FU-33 is not closed, downgraded or answered**. The ~10 % call (a clean seal) **LANDED, on cause as well as outcome**. §A2's ~25 % call (a brace/raft refusal) **MISSED**: **the brace and raft stages RAN for the first time** — 748 rung rows, 450 Brace members in the sealed forest — **and none of the ten registered brace/raft caps refused; but none of them is metered, so there is still NO demand of record for any of the ten.** H3's "no run has ever reached them" is superseded; its "no measured demand" stands. **Design §M3's 30.3 B per retained refusal is corrected DOWNWARD to ≤8.8–9.3 B/row at completion — and the cap does NOT move, in either direction.** **NO cap, floor, constant, coefficient or registry entry raised, lowered or touched; NO engine code changed** (`git diff` over `crates/` and `rust/` empty); **the treant is still NOT in the frozen digest set and `423276951048565625` is still NOT a pin.** What stands between this seal and the `.chitubox` checkpoint is the **emit path** — `examples/first_artifact.rs --all` still excludes the treant for a reason that is no longer true — and that is export work, not FU-32's. Per §A4 step 5, a first seal is an **owner's** decision point.]** |
