# Support resource caps — measurement and policy, 2026-08-05

This is the evidence record cited by `SUPPORT_KNN_WORK_PER_FACE_OBSERVED` and
`SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED` (`corpus_study`, 2026-08-05). It
records what one real product mini actually needs from the support pipeline's
execution-authority caps, and the rule those needs were turned into.

`03-physics-and-constants.md` remains the authority for values; this document is
the measurement of record behind two of them.

---

## 1. The problem, stated exactly

The **Weeping Treant v1.2** (429,120 support faces after decimation and
hollowing) is the actual product mini and the only corpus model larger than
4,760 faces. Every other corpus model is between 12 and 4,760 faces; the treant
is **90× the next model**. It was refused at the first stage that meets a
resource cap:

```
Preparation: Perception(PointIndex("kNN work requires 298667520; limit is 100000000"))
```

> **CORRECTED 2026-08-06 — dated addendum, the sentence above is left standing.**
> "429,120 support faces **after decimation and hollowing**" attributes the count to two
> stages that did not produce it. **429,120 is the raw STL triangle count** of
> `corpus/models/Weeping Treant v1.2.stl` (`od -An -tu4 -j80 -N4` on the file). Decimation
> is a **no-op** on this model — every reduced candidate fails `topology_not_increased`, so
> `plinth_decimate::run` returns the original mesh unchanged — and the census hollows via
> `HollowedModel::solid_decision`
> (`crates/plinth-support/examples/embed_incidence_census.rs:93-95`), which decides *solid*
> and moves no triangle. **Neither stage touched the count.** Nothing else in this document
> depends on the attribution: every measurement below was taken on the 429,120-face mesh,
> which is what the pipeline actually hands the support stage. Evidence:
> [`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md) §0, and owner ruling
> R6 in `CURRENT-STATUS.md`.

Not one of the support pipeline's ~35 execution caps had a measurement of
record. `git log -S` puts them all in Codex-era commits **73e7c13** ("Assemble
preparation resource policy") and **c2b3b98** ("repair specialized verifier
authority and bounds"), with no measurement in either commit body or any doc,
and every value is a round decimal (1e8, 2e9) or round binary (64 MiB, 2 GiB) —
the signature of a guess.

## 2. Measured requirement

Instrumented run of the exact public `run_support` path over the provisioned
corpus, 2026-08-05, on a 128 GB machine. Peak RSS across the whole treant run:
**2.5 GB** — memory was never the constraint.

| Quantity | Measured on the Weeping Treant | Derivation |
|---|---|---|
| Support faces | **429,120** | census `faces=` field |
| minima-kNN batch work | **298,667,520** | `source_count × query_count` = `429,120 × 696`; the refusal above reports it |
| kNN work **per face** | **696** | 298,667,520 / 429,120, exact |
| parity axis-grid projected peak | **78,482,308 B** | `axis_memory_bounds`, `rust/geom_kernel/src/parity.rs` |
| parity grid `(cell, face)` references | **1,214,544** | inverts `81·faces + 36·references + 4 = 78,482,308`, exact |
| references **per face** | **2.8303132** | 1,214,544 / 429,120 |
| parity grid bytes **per face** | **182.8913** | `81 + 36 × 2.8303132` |

The parity projection is exactly `81·F + 36·R + 4`: 81 B of per-triangle
projected arrays per face, and 36 B per `(cell, face)` reference for the CSR
plus the transient pair vector.

> **Owner ruling R4 (2026-08-05) — this is n = 1, and it is marked as such.**
> Both observations in the table above, and therefore both coefficients derived
> from them, rest on a **single specimen**: the Weeping Treant v1.2, 429,120
> faces, 2026-08-05. There is no second large model in the corpus to check them
> against — the next largest is 4,760 faces, 90× smaller, and sits under both
> floors where the coefficients never bind. Two numbers from one mesh are a
> measurement, not a distribution.
>
> The consequence is recorded in the M0 registry notes of all four constants
> (`SUPPORT_KNN_WORK_PER_FACE_OBSERVED`, `SUPPORT_KNN_MAX_WORK_PER_FACE`,
> `SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED`,
> `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE`) and is binding: **a second
> large-model measurement triggers re-derivation of these coefficients, not
> silent inheritance of a treant-shaped constant.** The next large model may not
> ratify 696 or 2.83; if it does not, the rule is re-run over both specimens
> rather than the new model being declared an outlier.

**References, not faces, dominate that projection, and `R/F` falls as a mesh
refines** — 526 references per face on the 244-face Cylinder against 2.83 on the
429k-face treant, because a coarse mesh's large triangles each span many 1 mm
cells. So the treant is the *tight* case for a per-face coefficient, and the
byte floor is what actually governs every coarse model.

## 3. The rule

> A cap's job is to refuse work that is **superlinear in model size**, not to
> refuse **large models**. A flat constant conflates the two: at 429k faces it
> fires on honest linear work, and at 24 faces it lets a real quadratic through.

Both scaled caps therefore take one shape, materialized per model by
`SupportPreparationLimits::input` against that model's own face count:

```
cap = max(floor, per_face × support_mesh_face_count)
```

and both coefficients come from one stated rule over the measurement above:

```
per_face = next_power_of_two(2 × observed_per_face)
```

| Constant | Value | Rule |
|---|---|---|
| `SUPPORT_KNN_MAX_WORK_PER_FACE` | 2,048 | `next_power_of_two(2 × 696)` = `next_power_of_two(1392)` |
| `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` | 512 | `next_power_of_two(2 × 182.8913)` = `next_power_of_two(366)` |
| `SUPPORT_KNN_MAX_WORK_FLOOR` | 100,000,000 | inherited unmeasured value, unchanged |
| `SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR` | 67,108,864 | inherited unmeasured value, unchanged |

> **CORRECTED 2026-08-05 (late), owner ruling R1 — the scaled family is two, and
> now it really is two.** As landed at `ef88ab0`, `SupportPreparationLimits::input`
> scaled a **third** cap that this document, the registry, and the type's own doc
> comment all omitted: `SpecializedLimits::coverage_route_knn.max_work`, by the
> same 2,048 coefficient, purely because it shares the `KnnBatchLimits` shape with
> the minima batch. It had no measurement of record, no accessor, and no test.
> `SUPPORT_KNN_WORK_PER_FACE_OBSERVED` = 696 was measured on the **minima-to-face**
> batch alone (§2); nothing in this record measures the coverage-route batch.
>
> That scaling is **removed**. The coverage-route cap stays at the flat
> `SUPPORT_KNN_MAX_WORK_FLOOR` until someone measures the coverage-route batch.
> The removal is behaviorally inert on every model: the treant's instrumented
> coverage-route demand is **15,282,619** work units, **6.5× under** the flat
> floor, and the treant stops at the specialized gate (§6) before coverage
> routing runs at all; every other corpus model is ≤ 4,760 faces, where the floor
> governed under the scaling too. Pinned by
> `the_coverage_route_knn_cap_is_the_flat_floor_at_every_face_count`
> (`crates/plinth-support/tests/resource_cap_policy.rs`).

> **RATIFIED 2026-08-05, owner ruling R2 — the 512 B/face coefficient stands.**
> The pre-landing proposal carried **256** B/face for the parity grid; the
> landed value is **512**. That deviation is deliberate and is ratified as
> written, with no code change.
>
> The point of the deviation is that there is **one** rule,
> `next_power_of_two(2 × observed)`, applied uniformly to both coefficients.
> 256 would have required reading the rule differently for the grid than for
> kNN — `next_power_of_two(observed)` rounding 182.89 up to 256 — and a policy
> with two rules dressed as one is worse than a policy with a wider margin.
> Uniformity is the property being bought here.
>
> The margin it buys is wider, and that is also on purpose. 512 B/face admits
> **4.2×** the treant's observed 2.83 refs/face (11.97 admitted against 2.83
> observed), where the kNN coefficient admits only **1.7×** its observed 696.
> The asymmetry is justified by the corpus itself: the refs-per-face ratio is
> the one quantity here measured to vary by more than two orders of magnitude
> across the corpus — **526 refs/face on the 244-face Cylinder against 2.83 on
> the 429k-face treant** (§2). A coefficient derived from the *tight* end of a
> 186× spread earns a wider margin than one derived from a quantity with no
> demonstrated spread at all. Note also that 512 is inert on today's corpus in
> either case: the byte floor governs every reachable model (§4).

`k·N` is still a bound. It refuses anything worse than linear in `N`, which is
precisely the runaway each cap was written to catch:

- kNN: an all-pairs batch over this mesh is `429,120² = 1.84 × 10¹¹` work and is
  refused **209× over**.
- parity grid: 512 B/face admits at most `(512 − 81)/36 = 11.97` references per
  face. The treant needs 2.83. A 429k-face mesh whose triangles each spanned
  more than ~12 grid cells at 1 mm pitch is still refused.

### Resulting bounds on the treant

| Cap | Before | After | Measured need | Headroom |
|---|---|---|---|---|
| minima-kNN `max_work` | 100,000,000 | **878,837,760** | 298,667,520 | 2.94× |
| parity `max_grid_bytes` | 67,108,864 | **219,709,440** | 78,482,308 | 2.80× |

219,709,440 B is 210 MiB, for one axis grid at a time — three orders of
magnitude under this machine's 100 GB ceiling.

## 4. Effect on the corpus — none, and that is checkable

The floor governs for **every** model the corpus can currently run. The largest,
Sphere at 4,760 faces, needs `512 × 4,760 = 2,437,120` grid bytes and
`2,048 × 4,760 = 9,748,480` kNN work — both far under their floors. So every
reachable model receives byte-identical caps to the pre-policy ones, and these
are refusal-only bounds that never alter an admitted plan.

Measured, not assumed: the 2026-08-05 before/after census differs in **exactly
one line out of 3,399**, the treant's own stage-error row. All seven reachable
models keep their digests:

> **CORRECTED 2026-08-05 (late), owner ruling R3.** The headline above is
> **wrong as stated and is not the claim the measurement supports**. The true
> before/after diff over the 3,399-line census is **9 changed lines**, not one:
>
> - **8 additive harness fields** on the eight `MODEL` rows — the two
>   materialized caps (`max_grid_bytes=`, `knn_max_work=`) that this policy
>   makes observable per model. They are new *reporting*, present on every model
>   including the seven whose values did not move, and they are exactly why the
>   line count moved at all.
> - **1 substantive line**, the treant's own `T0_stage_error` row, which now
>   names the specialized gate instead of perception because the treant now
>   clears perception.
>
> What the measurement *does* establish, and what the sentence should have said:
> **all 3,390 instrumented `CENSUS` body lines are byte-identical**, and **all
> seven reachable digests are unmoved** — the table below is the full check. The
> zero-behavioral-delta claim survives the correction intact; the "one line"
> figure did not, and it was the more impressive of the two claims, which is
> precisely why it needed checking. Retained above rather than rewritten, per
> this project's honest-addendum convention.

| Model | faces | digest (before == after) |
|---|---|---|
| Sphere | 4,760 | 17667468971446251870 |
| Cylinder | 244 | 13310185350867878190 |
| make_box | 12 | 9657051231007502658 |
| make_overhang_table | 60 | 6686599816688114065 |
| make_toed_slab | 140 | 11199296506741562956 |
| make_raking_blade | 24 | 13278215294653054002 |
| make_thin_fin | 24 | 7016743535301716631 |

`every_reachable_corpus_model_keeps_the_pre_policy_caps_unchanged`
(`crates/plinth-support/tests/resource_cap_policy.rs`) asserts this from the
registry rather than from the transcript.

> **Digest supersession, 2026-08-09 (FU-32 phase J4) — dated addendum; nothing above is
> rewritten or retracted.** Every OD-8 `snapshot_digest` quoted above is correct **as a record
> of its own date** (this §4 table was already superseded once, on 2026-08-08, for FU-10 phase
> C). Under the owner's 2026-08-09 ruling — *"The fingerprints aren't sacred  if things change
> slightly that's to be expected (because the algorithm wasn't right when the fingerprints were
> generated!)"* — **all seven were re-pinned**, because FU-32 J2 replaced
> `CcoController::digest`'s per-terminal whole-state walk with state-pure incremental
> accumulators and the digest therefore folds a different stream. The live values are Sphere
> `2297157889062365577`, Cylinder `6853793710942395608`, `make_box` `8709076999423814412`,
> `make_overhang_table` `16396858890798947253`, `make_toed_slab` `133000971864625681`,
> `make_raking_blade` `18388631460456455184`, `make_thin_fin` `14656364680512457190`.
> **The support itself did not change**: 687,288 dumped routing-output content lines and 275
> transcript rows are byte-identical parent-to-tip across all seven models, and
> `routed_contacts` / `final_live_members` are unchanged on every one. **No cap in this
> document was raised, lowered or touched by that phase.** The authority table and the full
> attribution are `CURRENT-STATUS.md` `#fu32-j4-2026-08-09` and `#fu32-j3-2026-08-09`.

## 5. What was deliberately NOT raised

Four further caps were measured over the treant with caps hand-raised, and none
of them is raised here.

- **`max_gate_operations` (2 × 10⁹) stands.** Its reported
  `required: 2000000001` is `limit + 1` — the charge counter tripping, not a
  requirement. The charge comes from `unique_island_face_count` /
  `visit_served_owner_faces` in `specialized.rs`, which find distinct face ids
  by a repeated-minimum **selection sort** and charge one unit per inner
  comparison. Instrumented prediction for the treant: 16,795,425,332 charged
  operations; a scratch `BTreeSet` drop-in with identical output measured
  **142,646**, a ~117,000× reduction. **This cap needs no raise; it needs the
  quadratic removed.**
  - *Follow-up, 2026-08-06 — FU-8b fix (A) landed (`0bdec0c`).* The quadratic is
    gone: both gatherers now gather each island once into one reused buffer and
    `sort_unstable` + `dedup` in a single batch, charging `n` for the gather
    plus the `n·(⌈log₂(n+1)⌉+1)` sort/dedup comparison bound instead of
    `n·(distinct+1)`. The treant's refusal became a **stated** demand,
    `GateOperationLimit { required: 12714828544, limit: 2000000000 }` — 3.97 %
    above this study's instrumented exact figure of 12,229,532,794, the
    difference being the new sort-bound charge over the prototype's flat
    one-unit-per-id. The cap still stands unraised: what it refuses now is
    other gate work, and its measured bound stays deferred until (b) and (c)
    land. Peak RSS for that census row: 2.45 GB. Single specimen, n = 1.
- **`max_load_operations` (2 × 10⁹), `max_load_scratch_bytes` (512 MiB),
  `max_gate_peak_bytes` (1 GiB) stand.** With caps raised, the treant reaches
  `finalize_specialized_load_partition` and builds **7,988,078** load-eligibility
  rows for a 429,120-face mesh — 18.6 rows per face — because
  `specialized_owner_faces` pairs are appended without grouping. Feeding those
  into a dense contact × row scan is ~2.95 × 10¹¹ charged operations, and an
  uncharged O(n²) rescan inside the eligibility builder
  (`perception.rs`, two full linear scans of `specialized_owner_faces` per row)
  is ~6 × 10¹³ scalar comparisons single-threaded — `sample(1)` put
  **4,127 of 4,127** stack samples in that one frame. **Here the caps are doing
  their job**: they are catching a real defect. Raising them would convert a
  fast, honest refusal into a multi-hour hang.

> **CORRECTED 2026-08-06 (record fix F3) — dated addendum, the bullet above is left
> standing.** "**builds** 7,988,078 load-eligibility rows … because
> `specialized_owner_faces` pairs are appended without grouping" describes a build that
> never happened. At `eb36358`, `build_load_eligibility`
> (`crates/plinth-support/src/perception.rs`) was already a pure
> `BTreeMap<u32, BTreeSet<LoadOwner>>` collapse keyed on `face_id`, emitting exactly one
> row per distinct face; there were no ungrouped appends in it, then or now. **7,988,078
> was a *predicted* row count**, computed inside `specialized_load_resource_plan` as
> `plan.load_eligibility.len() + specialized_face_count` where `specialized_face_count`
> had been handed the `(owner, face)` **pair** count — 16,627 + 7,971,451 on the treant.
> FU-8b fix (B) corrected the prediction, not the data: the same function now states
> 16,627 + 113,921 = **130,548**, and the row set the builder produces is unchanged. The
> ~2.95 × 10¹¹ charge and the caps' verdict on it are unaffected — the charge was computed
> from the inflated prediction, so what the caps refused was an inflated *plan*, which is
> still exactly what a resource plan is for. The uncharged O(n²) rescan and the 4,127/4,127
> stack samples were real and were defect (C).

> **ADDENDUM 2026-08-07 (FU-8f phase G1) — a NEW term is charged against
> `max_load_scratch_bytes`; the cap itself is untouched.** Everything above is
> left standing. `max_load_scratch_bytes` remains **536,870,912 B (512 MiB)**,
> unraised and unlowered, and no charged *operation* count moved in this phase.
> What changed is what `finalize_load_rows` does and therefore what it must pay
> for. It no longer scans `contacts × rows` and `continue`s on the ineligible
> cells; it builds a **contact-major CSR transpose** of the eligibility it is
> handed and runs the identical loop body over the eligible cells only, in the
> identical order. Measured on the Weeping Treant v1.2's merge-gate batch at
> `7aa6b73`:
>
> | counter | value |
> |---|---:|
> | cells visited by the old `contacts × rows` scan | **160,834,206** |
> | cells that are actually eligible | **8,210,643** (19.6× fewer; 94.9 % of the old visits were the `continue`) |
> | `Σ|eligible_owners|` over rows | **7,988,078** (max **568** on one row) |
> | transpose bytes charged, `cells × 4 + (contacts + 1) × 4` | **32,848,040 B** |
> | share of this 512 MiB cap | **6.1 %** (3.1 % of the 1 GiB gate peak) |
>
> The term is added to `LoadResourcePlan::scratch_bytes` inside
> `load_resource_plan` — the finalizer's own **exact** plan — and deliberately
> **not** to `specialized_load_resource_plan`, the preflight: the term *is* the
> exact eligible-cell count, and every bound a preflight could state without the
> eligibility (`owner_count × contact_count`, `rows × contacts`) over-states it
> by two to three orders of magnitude and would refuse batches this build
> admits. Consequence, stated rather than left to be found: the preflight
> under-states this finalizer's scratch by the transpose term. That costs a
> **later** refusal, never an unafforded allocation — the buffer is allocated
> only after the exact check passes.
>
> **The operation model is left standing and over-stating, on purpose.**
> `load_resource_plan_from_counts` still prices the scan as
> `rows × contacts × (owner_log + 4)` ≈ 4.34 × 10⁹ on the treant where the
> executed work is now ~10⁸. That is the FU-8e Q5 precedent — a plan that admits
> the general path *a fortiori* admits its specialisation — and it is why **no
> charged operation count moves in this phase**. Repricing is sequenced as its
> own phase (G5), after the completed-gate measurement (G4), because G4's
> transcript is the last measurement stated in the old numbers.
>
> Equivalence is by construction and is measured, not argued: filling the
> buckets by walking rows **ascending** leaves every bucket ascending with no
> sort (`buckets_already_sorted = true`), so consuming contacts **ascending**
> reproduces the old visit sequence cell for cell; the in-process replica on the
> treant's real batch reported `nearest_bit_identical = true`, tie-breaks
> included. `fu8f_transposed_finalize_reproduces_the_scan_bit_for_bit` pins it
> against the retained `#[cfg(test)]` `contacts × rows` reference, and the OD-8
> census is the corpus gate: `Sphere`, `Cylinder` and `make_box` exercise this
> finalizer on the **accepted-load** path with no tournament at all.
>
> Two record notes, so nothing is "corrected" back:
> - **`Σ|eligible_owners| = 7,988,078` is a coincidence with the falsified
>   figure of the same value** that record fix F3/F4 corrected above. The
>   falsified 7,988,078 was a *predicted row count*; this 7,988,078 is a
>   *measured sum of eligible owners over rows* on the same model. Same digits,
>   unrelated quantities, both correct in their own sentence.
> - **Rows are not uniformly small.** One treant row carries **568** eligible
>   owners of 1,366 contacts. Nothing in the transpose assumes short buckets.

The remaining work is algorithmic, not policy. Constants for a future scaled
gate/load bound must be set on **post-fix** dimensions; today's numbers are
measurements of a defect, not of a requirement. Note also that the linear
dimension for that family is *not* faces: `make_raking_blade`, a **24-face**
model, burns 612,669,596 gate operations — 31% of the 2 × 10⁹ cap — because gate
operations track coverage samples × contacts, not face count. A face-scaled
formula would be wrong for that family on its face.

## 6. Where the treant stands now

Two of the three known walls are resolved on measurement. The census reports:

```
MODEL  Weeping Treant v1.2  UNREACHABLE  faces=429120
       max_grid_bytes=219709440  knn_max_work=878837760
       support: sealed support run failed during Preparation
CENSUS T0_stage_error  Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 2000000001, limit: 2000000000 })
```

It clears perception, clears the candidate-surface grid, and is refused at the
specialized gate stage — quickly, with a typed error, at 2.5 GB peak RSS. That
refusal is correct and is tracked as algorithm work, not as a cap to raise.

## 7. FU-8b post-fix measurement (2026-08-06)

Dated addendum. §2's gate and load figures were charges against defects, not
statements of demand; FU-8b removed the three quadratics behind them
(`0bdec0c`, `1706c5d`, `e0a4bed`) and the model was then re-measured on the
same corpus harness, release build, uncommitted capwatch instrumentation
reverted before every recorded run.

### 7.1 What the treant actually demands

| quantity | pre-fix | post-fix | verdict |
| --- | --- | --- | --- |
| specialized gate operations, proposal-free batch | `required: 2000000001` — the counter tripping at `limit + 1` inside a selection sort, not a demand | **12,714,828,544** stated | honest, linear in the batch |
| specialized load operations | 2.95e11 predicted, over 7,988,078 rows | **5,402,535,619** over 134,368 rows | honest; fix (B) collapsed the row count 59.5× |
| load scratch bytes | over the 512 MB cap | **144,753,256** | under cap, no change needed |
| gate peak bytes | unmeasured | **215,061,920** | under the 1 GB cap, no change needed |
| specialized gate operations, 39,223-proposal merge batch | never reached (the 16-hour hang was defect (C)) | **21,755,124,103,146** | **superlinear in the batch — refused, FU-8c** |
| peak RSS | — | **2.84 GB** | far under the 100 GB machine ceiling |
| wall clock to refusal | ~16 h (hang) | **81.5 s** | inside the 600 s timebox |

> **CORRECTED 2026-08-06 (record fixes F1, F2, F3) — dated addendum, the table above is
> left standing.** The "specialized load operations" row pairs a real demand with the
> wrong batch and states a collapse ratio that follows from the mismatch.
>
> - **The demand is right; the row count is not.** 5,402,535,619 is
>   `finalize_specialized_load_partition`'s planned `resource.operations`, and that plan
>   states **130,548** rows, not 134,368 — `16,627` generic load-eligibility rows plus the
>   `113,921` distinct faces of `7,971,451` specialized `(owner, face)` pairs. One
>   scratch-instrumented treant run printed `rows=130548` and `operations=5402535619`
>   together at that checkpoint. §7.4's own census transcript prints the same
>   `rows=130548`. **No 134,368-row load batch is planned anywhere**: had one been, it
>   would have materialized `64 × 134,369 × 1,367 = 11,755,675,072` and displaced the
>   `11,421,470,912` the census records, since `record_scaled_cap` keeps only the widest
>   materialization per kind.
> - **"collapsed the row count 59.5×" is the same error carried forward.** The predicted
>   row count fell from 7,988,078 to 130,548, i.e. **61.2×** — and it was a *prediction*
>   that fell, not a row set (record fix F3; see the §2 addendum above).
> - **Headroom.** 5,402,535,619 against the materialized 11,421,470,912 is **2.11×**, and
>   that pairing is now consistent: both numbers are stated over the same 130,548 × 1,366
>   batch.

> **CORRECTED 2026-08-06 (record fix F7) — dated addendum, the table above is left
> standing.** Its "gate peak bytes" row records **215,061,920 B**, which was the peak this
> section measured at tip `d82c895`. Record fix F6 (`23d929d`) then made the gate batch's
> shared distinct-face `Vec<u32>` a term of `GatePeakUsage` — FU-8b fix (A) had allocated
> it once per batch and threaded it through every gather without ever charging it — so the
> number that governs on this branch is larger:
>
> | quantity | at `d82c895` | at `23d929d` (governs) |
> |---|---|---|
> | treant gate peak bytes | 215,061,920 | **216,203,088** = 215,061,920 + 1,141,168 face-scratch |
> | face-scratch term | not charged | 570,584 B of live face ids, charged as 1,141,168 B after the doubling for `Vec` growth slack |
> | `max_gate_peak_bytes` | 1,073,741,824 | 1,073,741,824, **unchanged** |
> | verdict | under cap, no change needed | still **under** cap, no change needed — 20.1 % of the bound |
>
> The term is 0.53 % of the peak and 0.11 % of the cap, and it is a worst-case **capacity
> bound**, not an observation. Charging it flipped no refusal: the census re-run at
> `23d929d` kept all seven frozen OD-8 digests byte-identical, kept the treant's single
> stage error verbatim at
> `Preparation: Specialized(GateOperationLimit { required: 21755124103146, limit: 35445500736 })`,
> and left every corpus scaled-cap row `at_floor=true`. No cap, coefficient or digest moves;
> only this observation does.

> **SUPERSEDED IN ONE ROW 2026-08-06 (FU-8c landed at `a3308ec`) — dated addendum; §7.1's
> table and both corrections above are left standing exactly as they were measured.** The
> table's `post-fix` column means *post-FU-8b*, and for every row but one it still does. The
> exception is **"specialized gate operations, 39,223-proposal merge batch — 21,755,124,103,146
> — superlinear in the batch, refused, FU-8c"**. FU-8c has since been fixed, and on the same
> batch against the same unchanged 35,445,500,736 cap that demand is now **201,480,535,329**,
> 108× less. The two refusal lines quoted verbatim in the F7 correction above and in §7.1 are
> the lines that held **at `23d929d`**; they are correct records of that tip and are not
> rewritten. As of `a3308ec` the treant's single stage error reads
> `Preparation: Specialized(GateOperationLimit { required: 201480535329, limit: 35445500736 })`,
> reproduced byte-identically at `8695398`.
>
> The row's **verdict is unchanged in kind**: still superlinear in the batch, still refused,
> still an algorithm defect rather than a cap to raise — 5.68× over instead of 613×. The
> successor defect is **FU-8d**. Across the FU-8c landing the seven frozen OD-8 digests stayed
> byte-identical and every corpus scaled-cap row stayed `at_floor=true`, so nothing else in
> this table moves. Peak RSS 2.50 GB and 124 s to refusal also supersede the table's 2.84 GB
> and 81.5 s **for the merge-gate run only**; both remain far inside the machine ceiling and
> the timebox.
>
> **Note added 2026-08-06 on how much those seven digests prove.** Not as much as the
> sentence above could be read to claim, and the census now says so with numbers. Six of the
> seven reachable models offer the merge gate **zero** proposals, so their unchanged digests
> are silent about FU-8c's per-proposal delta path — a model with no proposals never enters
> it. `make_raking_blade` is the exception and offers **1,402** proposals over 23 gate
> batches, so its unchanged digest `13278215294653054002` is the one that carries real
> weight for that path. The proposal counts are a new additive census cell
> (`merge_proposals=`, `gate_batches=`) on the `specialized_gate_operations` scaled-cap row;
> full per-model table in `CURRENT-STATUS.md` (*Does any reachable model exercise the
> proposal-delta path?*). The `at_floor=true` claim above is unaffected.

> **Ruling note 2026-08-06 (FU-8d record close) — the standing of
> 12,714,828,544. Dated addendum; the table and every correction above are left
> standing.** This is the **primary note** on that figure. It is the treant's
> proposal-free gate batch charge as measured post-FU-8b, and it is the sole
> input behind `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED = 23`
> (`12,714,828,544 / 553,835,949 = 22.958`, §7.2). Three things hold at once,
> and the record has to carry all three rather than pick one:
>
> - **F3's repricing plausibly supersedes it as an input.** Phase F3 (§10)
>   changed what the gate charges for the same stage: the load phase is priced
>   from `specialized_load_resource_plan`'s own figure instead of the gate's
>   `variants × faces` model, the per-variant owner-face descriptor scan is
>   charged **once**, and `unique_face_finalizer_work` ceases to exist as a term.
>   A charge stated under the old pricing is not automatically a charge under the
>   new one.
> - **No re-measurement exists at this tip.** The census prints scaled-cap
>   materializations, `at_floor` flags and stage errors — **not per-batch
>   demands** — so nothing on this branch restates the proposal-free batch's
>   charge under F3's pricing. "Plausibly superseded" is therefore exactly as
>   strong a claim as the evidence supports, and no stronger. **23 is not
>   corrected, not restated and not re-derived**; it stands as recorded.
> - **Re-derivation is blocked regardless, by ruling R-f.**
>   `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is re-derived only from a treant
>   run that **COMPLETES** the gate stage, and phase F4's two clean runs (§11) are
>   EXCEEDS-TIMEBOX at 607 s and 602 s with no typed refusal in either. When R-f
>   is satisfied, the proposal-free batch charge and the 23 it yields are to be
>   re-measured **together**; neither is to be inherited into the other.
>
> The other sites carrying 12,714,828,544 — `constants/evidence.rs`
> (`SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` and
> `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED`, the latter being the primary
> in-code note), `constants.rs`'s `SupportResourceCapPolicy` doc comment,
> `specialized.rs`'s `gate_operation_limit` doc comment, §7.2's prose and rule
> table below, and the M0 ledger — quote it for the **face-count spread** against 24-face
> `make_raking_blade`'s 612,887,212, and that use is unaffected by any of the
> above. They point here rather than restating it.

### 7.2 The rule that was set, and the scale it uses

Gate and load work do **not** track face count. The corpus proves it: 24-face
`make_raking_blade` charges 612,887,212 gate operations while 429,120-face
Weeping Treant charges 12,714,828,544 — a 20× spread over an 18,000× face
spread. Scaling those two caps per-face, as §3.10.8 scales kNN work and
parity-grid bytes, would have refused the small model and admitted anything at
all on the large one.

They scale instead on the **batch product** each stage is handed:

```
max_gate_operations = max(SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS,
                          SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT
                              x (coverage_samples + 1) x (contacts + 1))

max_load_operations = max(SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS,
                          SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT
                              x (rows + 1) x (contacts + 1))
```

Both coefficients come from the same rule as §3.10.8's,
`next_power_of_two(2 x observed)`:

| constant | wrapper | value | derivation |
| --- | --- | --- | --- |
| `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` | Measured (n = 1) | 23 | 12,714,828,544 / ((405,146 + 1) x (1,366 + 1) = 553,835,949) = 22.958, ceiling |
| `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` | Derived | 64 | `next_power_of_two(2 x 23)` = `next_power_of_two(46)` |
| `SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED` | Measured (n = 1) | 30 | 5,402,535,619 / ((134,368 + 1) x (1,366 + 1) = 183,682,423) = 29.41, ceiling |
| `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` | Derived | 64 | `next_power_of_two(2 x 30)` = `next_power_of_two(60)` |

> **CORRECTED 2026-08-06 (record fix F1) — dated addendum, the two load rows above are
> left standing.** Their divisor came from the mis-paired 134,368; the registry now reads:
>
> | constant | wrapper | value | derivation |
> | --- | --- | --- | --- |
> | `SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED` | Measured (n = 1) | **31** | 5,402,535,619 / ((130,548 + 1) x (1,366 + 1) = 178,460,483) = 30.28, ceiling |
> | `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` | Derived | **64** | `next_power_of_two(2 x 31)` = `next_power_of_two(62)` |
>
> **The coefficient does not move, and no cap materializes differently anywhere.**
> `next_power_of_two` rounds 60 and 62 alike, so 64 stands, every corpus model stays at its
> floor, and the treant's materialized load cap stays 11,421,470,912. Only the observation
> and its stated derivation change. The gate rows are untouched: the gate observation's
> 405,146-sample batch is the one the census prints.

Both floors are the R5-registered inherited numbers, unchanged at 2,000,000,000
and **not raised**: they are now the constant term of a rule rather than the
whole cap.

Owner ruling R4 applies unchanged: both observations rest on the Weeping Treant
v1.2 alone, so both coefficients are **n = 1**, and FU-23's standing trigger
covers them — a second large model is re-derived over both specimens, never
inherited silently.

### 7.3 Effect on the corpus — none, and the census prints it

Every corpus model's batch product is far too small to leave the floor. The
widest is the Cylinder's gate batch, `(509,056 + 1) x (54 + 1) = 27,998,135`,
and `64 x 27,998,135 = 1,791,880,640 < 2,000,000,000`. All fourteen corpus
materializations are therefore bit-identical to the pre-rule caps, and the
census says so per model:

```
CENSUS T0_scaled_cap  Cylinder  kind=specialized_gate_operations
       floor=2000000000 per_batch_unit=64 rows=509056 columns=54
       materialized=2000000000 at_floor=true
```

`the_batch_scaled_caps_are_the_flat_floor_for_every_corpus_batch_shape` pins it
over all seven models and both caps.

**The Cylinder is the near case and it is worth recording as such.** At 89.6% of
the floor it is the only corpus model within an order of magnitude of leaving
it. A coefficient of 128 rather than 64 would move the Cylinder's gate cap off
the floor and change a corpus model's materialized authority. The rule chose 64
by derivation, not by fit, but the margin is thin enough that a future
re-derivation must re-check this table rather than assume the floors still
govern.

### 7.4 Where the treant stands now

```
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_gate_operations
       floor=2000000000 per_batch_unit=64 rows=405146 columns=1366
       materialized=35445500736 at_floor=false
CENSUS T0_scaled_cap  Weeping Treant v1.2  kind=specialized_load_operations
       floor=2000000000 per_batch_unit=64 rows=130548 columns=1366
       materialized=11421470912 at_floor=false
CENSUS T0_stage_error Weeping Treant v1.2
       Preparation: Specialized(GateOperationLimit { required: 21755124103146,
                                                     limit: 35445500736 })
```

It now clears the proposal-free gate batch **and the whole load stage**, and is
refused at the *merge* gate, on a demand 613× the coefficient. That is the cap
doing its job rather than failing at it: the admitted batch is linear in its own
shape, the refused one is quadratic in it, and the same constant separates them.

The refusal is a **new** finding, not a cap to raise. `gate_resource_plan`
prices a merge batch at `(samples + demands + 1) x (proposals x (contacts - 1) +
contacts)`: the gate re-evaluates the entire contact set once per merge
proposal, and the treant offers 39,223 proposals. No honest constant admits it —
covering it would need a coefficient near 39,300, which would multiply every
corpus model's cap by the same factor and take the Cylinder, `make_raking_blade`
and several others off their floors. It is opened as **FU-8c** and is algorithm
work of exactly the FU-8b kind: the proposal loop is a per-item rescan of a whole
batch standing in for a batched evaluation.

> **FU-8c FIXED 2026-08-06 — dated addendum; the paragraph above is left standing
> as the finding it was.** The merge gate no longer re-evaluates the entire
> contact set once per merge proposal, so `gate_resource_plan` no longer prices
> it that way. A proposal replaces two contacts with one merged seat and leaves
> every other contact's position and routes verbatim, so it can only move
> coverage rows whose route key its own pair serves, and can only unseat a demand
> one of its own pair was witnessing. `GateBatchIndex`
> (`crates/plinth-support/src/specialized.rs`) builds that proposal-invariant
> state once per batch; each proposal then re-answers only its own route keys and
> re-tests each demand once against the merged seat. The plan is repriced to
> describe that:
>
> ```text
> routed_rows = (samples + demands + 1) x contacts + measured proposal work
> ```
>
> On the treant's merge batch that is 554,926,572 routed rows plus 4,618,272,984
> face rows = **5,173,199,556**, against the **unchanged** 35,445,500,736. **No
> cap was raised, lowered or otherwise touched.** The sentence above — that no
> honest constant admits a plan quadratic in the batch — is still true, and is
> still the reason none was sought. Equivalence was proved before the hot path
> was swapped: the predecessor's per-variant sweep is retained as a `#[cfg(test)]`
> reference and compared row by row with `f64::to_bits` against the delta path on
> a seven-proposal fixture
> (`fu8c_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit`).
>
> The census transcript above is a **historical** refusal record and is not
> re-run by this addendum; the treant's post-fix stopping point is recorded in
> `CURRENT-STATUS.md` under *FU-8c fixed*.

> **Correction added 2026-08-06 to the addendum immediately above — it is
> incomplete in a way that reads as the opposite of the measurement, and this note
> is the correction.** The "**5,173,199,556** against the **unchanged**
> 35,445,500,736" figure is arithmetically right but is **not the treant's
> demand**, and standing beside the cap the way it does it invites the inference
> that the treant is now admitted. **It is not. The treant still refuses at this
> gate.** 5,173,199,556 is the repriced plan's two *count-derived* terms —
> `baseline_rows` 554,926,572 + `face_rows` 4,618,272,984 — evaluated with the
> third term, `measured proposal work`, at **zero**. That is what the pinning test
> `fu8c_repriced_plan_admits_the_treant_gate_shape_under_the_unchanged_cap`
> asserts, and its name is exact: the *shape* fits. The real batch supplies real
> proposal work, so the demand the treant actually states is
>
> ```text
> baseline_rows 554,926,572 + face_rows 4,618,272,984
>   + demand_rows 42,949,185 + proposal work 196,264,386,588
>   = 201,480,535,329   against 35,445,500,736  -> refused, 5.68x over
> ```
>
> — measured at `a3308ec` and reproduced byte-identically at `8695398` (124 s,
> 2.50 GB peak RSS, `PLINTH_THREADS=4`, 90 GB watchdog never fired). Read
> 5,173,199,556 as the floor of the repriced plan, never as the treant's price.
> The residual 196,264,386,588 — 97.4 % of the demand — is opened as **FU-8d**.
> Everything else in the addendum above stands unchanged.

> **Owner/orchestrator ruling 2026-08-06 — what pins 201,480,535,329, and what
> deliberately does not.** Recorded here because the verification of the two
> corrections above asked for it directly: *should there be a test asserting the
> 201,480,535,329 total, so a future change to the plan cannot move it
> silently?*
>
> **Ruling: no pinned-literal test of the total, and the omission is
> deliberate.** Three of the four terms above are count-derived and a test can
> assert them from the batch's own shape; the fourth, `proposal work`
> 196,264,386,588, is **caller-measured** — `plan_gate_nearest_batch` computes it
> by running a delta pass per proposal over the treant's real coverage-sample
> distribution, and no fixture in the test suite reproduces that distribution. A
> test carrying 201,480,535,329 as a literal would therefore be a **copy of a
> census transcript, not a derivation of it**: it would pass by agreeing with
> itself, it would pin nothing about the plan, and the first legitimate FU-8d
> change would have to edit the literal — which is exactly the "update the golden
> until it's green" ritual the shape-derived tests exist to avoid.
>
> **What is of record instead**, and it is stronger for this term, not weaker:
>
> - The **census transcript in the dated record is the measurement of record**
>   for the total. `CURRENT-STATUS.md` (*FU-8c fixed 2026-08-06*) carries it
>   twice, independently measured at `a3308ec` and `8695398` and byte-identical,
>   with the four-term decomposition beside it. A number that is measured is
>   re-measured when it is doubted; it is not asserted.
> - `fu8c_repriced_plan_admits_the_treant_gate_shape_under_the_unchanged_cap`
>   **keeps its derivation-shaped expectations** and is not widened to the total.
>   It pins the count-derived floor — `baseline_rows` + `face_rows` = 5,173,199,556
>   — computed from the batch's stated shape, so a change to the *pricing formula*
>   still fails it. Its name already says it covers the shape and not the batch;
>   see the correction above.
> - Equivalence of the per-proposal path is pinned where it can be pinned
>   exactly, on `f64::to_bits`, by
>   `fu8c_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit` and
>   `fu8c_witness_demand_counts_reproduce_the_full_sweep`.
>
> The reading rule that follows: **treat 201,480,535,329 as a dated measurement,
> not as an invariant.** FU-8d is expected to move it, and when it does the
> record gets a new dated transcript rather than an edited literal.

Out of scope and deliberately not touched: `finalize_load_rows` is a correctly
charged dense contact-by-face nearest scan. Post-fix it costs 134,368 x 1,366 =
1.8e8 row-contact pairs, not the 1.1e10 the pre-fix row count implied, and it
runs inside the timebox.

> **CORRECTED 2026-08-06 (record fixes F1, F2) — dated addendum, the block and paragraph
> above are left standing.** Two things to state precisely.
>
> - **The census transcript above is the authority, and it says 130,548.** The headline
>   figures elsewhere in this document (§7.1) paired that same 5,402,535,619 demand with
>   134,368 rows and read a 2.1× headroom off the mismatched pair. Corrected: the demand
>   and the `materialized=11421470912` bound are both stated over **130,548 rows × 1,366
>   contacts**, and the headroom is **2.11×**. The transcript never needed correcting.
> - **`finalize_load_rows` costs less than either figure.** Its `rows` is the *actual*
>   extended eligibility, the union of the 16,627 generic rows and the 113,921 pair faces:
>   **117,741** rows in one scratch-instrumented treant run, against the 130,548 the
>   partition's plan predicts. The plan is deliberately an upper bound — it adds the two
>   counts rather than unioning them, because the union does not exist yet when the bound
>   is checked. So the scan costs 117,741 x 1,366 = **1.6e8** row-contact pairs, its plan
>   charges 1.8e8, and both are far under the 1.1e10 the pre-fix prediction implied.

## 8. FU-8d phase F1 — witness retention, 2026-08-06

Dated addendum. Nothing in §§1–7 is rewritten: no cap in this document moved, no
coefficient was re-derived, and the treant's stated demand is unchanged at
**201,480,535,329** against the unchanged **35,445,500,736**. This section exists
because F1 changed two things a caps record has to carry — a `KnnBatchLimits`
shape field, and the term list of the gate peak bound.

**What F1 did.** The merge gate's *baseline* coverage sweep now asks the kernel
for `k = min(3, source_count)` neighbours per coverage sample instead of one, and
retains them as `(source_slot, squared_distance)` in the kernel's own emission
order. Nothing reads them yet; FU-8d's per-proposal delta (part (1)) will, and
that is what removes the `sources × samples` re-sweep of a perturbed route key.
Every answer this stage produces is unchanged: `nearest[sample]` is still
`squared_distances[row.start].sqrt()`, and row 0 of a planned kNN batch does not
depend on `k` — pinned against the kernel by
`fu8d_knn_row_zero_is_k_independent_bit_for_bit`.

### 8.1 The one limit that moved is not a cap

`SpecializedLimits::production` now states `coverage_route_knn.max_neighbors = 3`
where it previously carried the caller's 1. **This is a `KnnBatchLimits` shape
field, not an FU-8 resource cap, and §5's list of what was deliberately not
raised is untouched by it.** Owner ruling **R-b** (2026-08-06) records it as such,
with the arithmetic:

| term | what `k = 3` costs | against |
|---|---|---|
| per-route kNN CSR pair arrays | `+2 × query_count × 16 B` per route — single-digit MB on the treant's widest route | that batch's unchanged **2 GB** `max_logical_peak_bytes` |
| per-batch retained witness table | `samples × 3 × 16 B` = **19,447,008 B ≈ 19 MB** on the treant's 405,146 samples | the unchanged **1,073,741,824 B** `max_gate_peak_bytes`, whose last measured value on this model was 216,203,088 B |
| charged gate operations | **nothing** — `KnnBatchPlan`'s work term is `source_count × query_count` and is k-independent (ruling **R-a**) | — |

Ruling R-a is why the operations column is empty, and it is recorded on
`NEAREST_WITNESS_SLOTS` in `specialized.rs` as well as here, so that the silence
of the charge cannot be mistaken for an oversight. The `SUPPORT_KNN_MAX_NEIGHBORS`
registry entry keeps its value of **1** — it describes
`SupportPreparationLimits::production`'s own `KnnBatchLimits`, which did not
change and which the generic perception limits still carry verbatim — but its
note, its `m0-constants-source-ledger.json` entry, the regenerated `constants.md`
and §3.10.9's table in `03-physics-and-constants.md` all now carry the dated
amendment, because the note's claim that "the support pipeline only ever asks for
the single nearest neighbor" was true when written and is not true now.

### 8.2 The gate peak bound gained a term

`GatePeakUsage` gains `nearest_witness_bytes`, charged into the **retained**
total beside `face_scratch_bytes` (record fix F6) and `batch_index_bytes`
(FU-8c), because the witness table is built once at the top of the batch and held
across the phases. That is the same discipline F6 and F9 each had to impose after
finding a real allocation charged against nothing; here it is charged from the
first batch that builds one.

**The cap it is charged against — `max_gate_peak_bytes` = 1,073,741,824 — is
unchanged, and no model on the corpus comes near it.** The treant does not reach
this check at all: it refuses at `exact_operations` before the preflight that
performs it, so on that model the table is never allocated and its ~19 MB is a
**bound this run did not exercise**, not an observation. The §7.1 F7 correction's
216,203,088 B stands exactly as measured at `23d929d`; F1 does not restate it.

### 8.3 What was re-measured, and what it proves

Full 8-model census at the F1 tip, release, `--features census`,
`PLINTH_THREADS=4`:

- all seven frozen OD-8 digests **byte-identical**;
- all fourteen corpus `T0_scaled_cap` rows still `at_floor=true`;
- the treant's stage error **verbatim**:
  `Preparation: Specialized(GateOperationLimit { required: 201480535329, limit: 35445500736 })`.

**That immobility is the phase's proof**, and it is worth being exact about what
it does and does not show. It shows F1 moved no charge and no admitted result. It
does *not* show the retention runs at treant scale, because the treant never
reaches it. What shows the retention runs on real corpus geometry is
`make_raking_blade` — 23 gate batches, 1,402 merge proposals, digest
`13278215294653054002` unmoved — plus the two FU-8d tests. FU-23's trigger is
unchanged and still pending: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is to be
re-derived only from a treant run that **completes** the gate, which this is not.

## 9. FU-8d phase F2 — the nearest delta, and a stopping point that moved, 2026-08-06

Dated addendum. Nothing in §§1–8 is rewritten. **No cap in this document moved,
no coefficient was re-derived, and no cap code was touched.** This section exists
because two things a caps record must carry did change: the merge gate's stated
demand, and — for the first time since FU-8b — **which check refuses the Weeping
Treant**.

### 9.1 What F2 did

A merge proposal's nearest phase used to re-sweep every route key its pair
perturbs against that key's whole surviving source set: `Σ_{perturbed keys}
source_count × query_count`. It no longer does. The minimum over
`survivors(key) ∪ {merged seat}` splits into two parts that are each cheap:

- **the survivors' side** is *read* from the witnesses phase F1 retained, as a
  value — three retained slots answer any two-contact removal exactly, and the
  bits returned are the bits the kernel emitted during the baseline sweep;
- **the merged seat's side** is **one** `KnnBatchPlan(1 source, touched_rows
  queries, k = 1)` per proposal, covering every perturbed key's rows at once.

The two are combined by `f64::total_cmp` and the square root is taken **once,
after** the minimum, so the arithmetic applied to the winning value is identical
to the sweep's `squared_distances[row.start].sqrt()`, and an exact tie is a tie
between two bit-identical `f64`.

Equivalence was proved before the hot path was swapped, against the same
retained `#[cfg(test)]` full sweep FU-8c used
(`fu8d_proposal_nearest_deltas_reproduce_the_full_sweep_bit_for_bit`), on a
fixture grown until each arm is reached and asserted non-vacuous: a key with
fewer than three servers, a proposal that removes **every** server of a key, a
row whose baseline nearest is merged away, a **steal**, and an **exact distance
tie** between a survivor and the merged seat.

### 9.2 The demand this cap is handed

Measured on the Weeping Treant v1.2 (429,120 faces; 405,146 coverage samples,
1,095 demands, 1,366 contacts, 39,223 merge proposals), release,
`--features census`, `PLINTH_THREADS=4`, RSS watchdog armed:

| `gate_resource_plan` term | at F1 (§8) | at F2 | shape |
|---|---:|---:|---|
| `baseline_rows` | 554,926,572 | 554,926,572 | `(samples + demands + 1) × contacts` |
| per-proposal `nearest_work` | 195,222,841,902 | **1,041,544,686** | was `Σ s × q`; now `Σ 1 × touched_rows` |
| per-proposal `touched_rows` | 1,041,544,686 | 1,041,544,686 | rows written, scanned and restored |
| per-proposal `key_visits` | — (uncharged) | **78,446** | `routes(l)+demands(l)+routes(r)+demands(r)` |
| per-proposal `demand_rows` | 42,949,185 | 42,949,185 | `proposals × demands` |
| `face_rows` | 4,618,272,984 | 4,618,272,984 | `variants × distinct faces` |
| **stated demand** | **201,480,535,329** | **7,299,316,559** | |
| **cap (unchanged)** | 35,445,500,736 | 35,445,500,736 | `max(floor, K_G × (samples+1) × (contacts+1))` |

The F1 column reconciles exactly with the number FU-8c recorded:
`195,222,841,902 + 1,041,544,686 = 196,264,386,588`, the "per-proposal nearest"
residual, 97.4 % of the demand. The whole per-proposal nearest term is therefore
`196,264,386,588 → 2,083,167,818`, **94.2×**, and the stated demand is
`201,480,535,329 → 7,299,316,559`, **27.6×** — **20.6 % of the unchanged cap, so
`gate_resource_plan` now passes.** Per ruling **R-e** no literal of the post-fix
total is pinned in a test; the count-derived terms keep their derivation-shaped
pins and this dated transcript is the record.

### 9.3 The stopping point moved — and this is the phase's real result

The treant **still refuses at the merge gate**, and still with
`GateOperationLimit`, but no longer at the same check. `gate_resource_plan`
passes; the refusal now comes from the `exact_operations` fold, where
`variant_specialized_owner_face_count`'s descriptor budget crosses this same
limit:

```
CENSUS  T0_stage_error  Weeping Treant v1.2
        Preparation: Specialized(GateOperationLimit { required: 35445618202, limit: 35445500736 })
```

That number is **not a total**. The scan refuses the instant its running counter
crosses, which it does at **variant 275 of 39,224** — 0.70 % of the batch — so
`35,445,618,202` is "the cap plus one variant's descriptor work", and the true
demand is roughly `39,224 / 275 ×` larger. Measured at the crossing:

| counter, accumulated over the first 275 variants | value | extrapolated over 39,224 |
|---|---:|---:|
| `descriptor_operations` (owner-face scan) | 35,445,417,382 | ~5.06 × 10¹² |
| `unique_face_finalizer_work` inside `variant_operation_count` | 3,036,066,151,957 | **~4.33 × 10¹⁴** |
| routed (nearest) work | 3,912,181 | ~5.6 × 10⁸ |

The routed term is now about one part in a million of what the variant fold
costs (3,912,181 against 3,037,219,978,143 over the same 275 variants). **The wall is entirely FU-8d part (2)** — the per-variant load and
owner-face path — and the ~4.33 × 10¹⁴ above is the same order the FU-8d design
predicted for it. Phase F4 is what addresses it. **It is not a cap to raise**, on
exactly the grounds §5 and §7.2 already set out: admitting it would need a
per-batch-unit coefficient in the millions and would take the Cylinder and most
of the corpus off their floors.

### 9.4 What was re-measured, and what it proves

Full 8-model census at the F2 tip, release, `--features census`,
`PLINTH_THREADS=4`, 3,415 lines:

- all seven frozen OD-8 digests **byte-identical** (`Sphere 17667468971446251870`,
  `Cylinder 13310185350867878190`, `make_box 9657051231007502658`,
  `make_overhang_table 6686599816688114065`,
  `make_toed_slab 11199296506741562956`,
  `make_raking_blade 13278215294653054002`,
  `make_thin_fin 7016743535301716631`);
- all fourteen corpus `T0_scaled_cap` rows still `at_floor=true`;
- the treant's stage error as quoted in §9.3.

`make_raking_blade` carries the corpus weight, for the reason §8.3 gives: it is
the one reachable model that runs a genuine merge tournament — 23 gate batches,
1,402 merge proposals — through the exact code F2 rewrote, and its digest did not
move. The treant run itself is **129 s at 2,641,504 KB (2.52 GiB) peak RSS**,
watchdog armed and never fired, superseding the 124 s / 2.50 GB of §7.

**Ruling R-f is not satisfied by this phase.**
`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is to be re-derived only after phase
F3 lands **and** only from a treant run that **completes** the gate. This run does
not complete it, so nothing was re-derived.

### 9.5 Ruling R-c, measured and left unbuilt

> **Nomenclature pointer, added 2026-08-06 (FU-8d record close).** The FU-8d
> design named two measurement stages that this document records under their
> rulings instead, so a grep for either term lands nowhere. They map exactly:
> **Stage A** = the key-concentration and per-key proposal tables, recorded here
> as ruling **R-d** (the R-d paragraph at the end of this section); **Stage B** =
> the removed-holder, steal and radius-bound row counts, recorded here as ruling
> **R-c** (the table immediately below). No measurement is added or restated by
> this pointer — the substance is already of record under the ruling names.

The design's radius-bounded steal path (candidate `d`) was **not built**. F2's
scratch pass measured what it would have bought, over all 39,223 proposals, with
`R_max(key)` taken as the largest baseline nearest distance that key carries — a
merged seat farther than that from a sample cannot win it:

| per proposal | mean | p50 | p99 | max | sum |
|---|---:|---:|---:|---:|---:|
| touched rows | 26,554.4 | 22,868 | 41,422 | 41,422 | 1,041,544,686 |
| rows inside `R_max` | 15,399.5 | 13,270 | 29,303 | 38,679 | 604,013,078 |
| rows whose baseline nearest was merged away | 93.9 | 20 | 891 | 16,694 | 3,681,423 |
| steal rows | 0.449 | 0 | 13 | 535 | 17,594 |

The radius bound removes **42 %** of the rows — a 1.7× constant factor, not an
order. The adoption rule was "only if the measured margin is under 2×", and the
margin is 4.9× (7,299,316,559 against 35,445,500,736), so **R-c is not adopted**.
The rows that actually change are rarer still: 0.35 % of touched rows lose their
holder and 0.0017 % are steals, which is why the delta is a lookup with one
distance test rather than anything cleverer.

Ruling **R-d** (batching proposals by hot key) is likewise measured and left
unbuilt: 28 of the 48 perturbed keys carry ≥ 10 proposals, and one carries 26,543,
so the precondition holds — but it is a constant-factor cleanup on a term that is
now some five parts in a million of the wall, and it would be optimizing the
wrong thing while part (2) stands.

## 10. FU-8d phase F3 (2026-08-06) — the treant's merge gate stops refusing

> **Numbering correction added 2026-08-06 (phase F4).** This section landed as a
> second `## 9`, beside phase F2's, so "§9" was ambiguous for a day. It is **§10**;
> **§9 is phase F2**. Only the heading digit changed — no sentence, table or figure
> in either section was touched — and the pointers that meant *this* section
> (`README.md`, `03-physics-and-constants.md`, `specialized.rs`) now say §10.

> **Dated addendum. §§1–8 are left standing exactly as measured.** This section
> supersedes §7.4's and §8's *stopping point* for the Weeping Treant and nothing
> else. **No cap was raised, lowered or touched, and no cap code was edited.**

Phase F3 closed FU-8d parts (2) and (3): the load partition is finalized **once**
per gate batch instead of once per variant, the per-variant `(owner, face)` window
matrices are deleted, the four per-proposal planning passes become one behind a
count-derived pre-check, and the per-variant contact/demand rescans are derived
rather than repeated. Measured on the treant's first merge batch — samples
405,146, demands 1,095, contacts 1,366, proposals 39,223, variants 39,224,
distinct island faces 117,741:

| term | F2 | F3 |
|---|---:|---:|
| `baseline_rows` | 554,926,572 | 554,926,572 |
| proposal nearest (`nearest_work + touched_rows`) | 2,083,089,372 | 2,083,089,372 |
| proposal `key_visits` | 78,446 | 78,446, now pre-checked before it is spent |
| proposal `demand_rows` | 42,949,185 | 42,949,185 |
| load term | 4,618,272,984 (`variants × faces`, a **model**) | **6,592,813,742** (`specialized_load_resource_plan`'s own 5,543,831,115, plus a 1,048,982,627 proposal bound) |
| **stated demand** | 7,299,316,559 | **9,273,857,317** |
| `descriptor_operations` | 35,445,417,382 at variant 275 of 39,224 (~5.06 × 10¹² extrapolated) | **136,791,451**, charged once |
| `unique_face_finalizer_work` | ~4.33 × 10¹⁴ extrapolated | **term deleted** |
| `exact_operations` | refused at the crossing | **10,457,910,996** |
| **cap — UNCHANGED** | 35,445,500,736 | 35,445,500,736 |
| verdict | refused | **admitted — 26.2 % stated, 29.5 % exact — and executed** |

**The stated demand rose, and that is the point.** §7.2's rule is that a cap
prices what a stage really does. FU-8d part (2) recorded that the gate priced the
load phase at `variants × faces` while the fold really did
`variants × faces × contacts`; there is one authority now, and the honest figure
is 1.43× the model it replaced. Every term that fell is a *quadratic removed*, not
a bound relaxed.

> **Dated addendum 2026-08-07 — FU-8f phase G5. The table and the paragraph above
> are left standing exactly as measured; three of their figures have since moved,
> downward.** The `load term` **6,592,813,742 → 1,844,748,658** (its
> `specialized_load_resource_plan` component 5,543,831,115 → **795,766,031**, the
> 1,048,982,627 proposal bound unchanged), the `stated demand`
> **9,273,857,317 → 4,525,792,233** (26.2 % → **12.8 %** of the same unchanged
> cap), and `exact_operations` **10,457,910,996 → 5,709,845,912** (29.5 % →
> **16.1 %**). Every other row is byte-identical. The cause is neither a bound
> relaxed nor a quadratic removed from the *code* — phase G1 had already removed
> that one — but the **charge** catching up with the code:
> `load_resource_plan_from_counts` stopped pricing the load scan as
> `rows × contacts × (owner_log + 4)`. **The paragraph above still holds, in the
> other direction:** §7.2's rule is that a cap prices what a stage really does,
> and here what the stage really does is *less* than what it was charged. **No
> cap moved.** See §15.

**A byte wall went with it, and it had never fired only by accident.**
`gate_window_matrix_bytes` sized one `PhysicalContact` matrix and one
`(owner, face)` matrix per variant per window — on the treant 7,971,451 pairs × 8 B
*per variant* — against `SUPPORT_SPECIALIZED_MAX_GATE_CONTACT_MATRIX_BYTES` =
536,870,912. It stated a figure in the tens of gigabytes and would have refused the
moment the operations wall fell. The batch now holds one physical list and one pair
list; `gate_window_matrix_bytes` and `traverse_whole_gate_windows` are deleted and
`GatePeakUsage::matrix_bytes` is replaced by `load_baseline_bytes`. **The cap itself
is untouched**: its remaining consumer is `contact_window_bytes`, and its registered
value and provenance note are unchanged.

**Where the treant stops now, and it is not a refusal.** Under `PLINTH_THREADS=4`,
release, `--features census`, the treant planned eight gate batches inside the 600 s
timebox — the proposal-free batch plus seven merge rounds at 1,366 → 1,360 contacts,
the stated demand falling monotonically with the contact set and the share holding
flat at 26.2 % of each batch's own materialized cap — and did not finish the
tournament. That run is recorded as **EXCEEDS-TIMEBOX with counters**: peak RSS
~2.4–2.5 GB sampled once per second, watchdog armed at 90 GB and never fired, far
inside the 100 GB machine ceiling.

What the time is spent on is measured by implication and is **not a cap to raise**:
`assess_coverage` runs `O(samples)` per variant with about six
`Vec::with_capacity(405,146)` allocations per call, i.e. `39,224 × 405,146 ≈
1.59 × 10¹⁰` sample visits and of order a terabyte of allocation churn **per gate
batch**, none of it charged against `gate_operation_limit`. It is opened as
**FU-8e**.

**Ruling R-f remains unsatisfied.** `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED`
(FU-23) is re-derived only from a treant run that **completes** the gate. This is
not that run.

> **Correction added 2026-08-06 (phase F4) — §10 is left standing exactly as
> written, and the paragraph beginning "What the time is spent on is measured by
> implication" is now known to be wrong in proportion.** It is measured
> *directly* in §11, and `assess_coverage` is the **second** frame at about a
> quarter of main-thread time, not the whole of it. The leading frame is F2's own
> `fill_proposal_nearest_delta`, at about three fifths. The `1.59 × 10¹⁰`
> arithmetic above is right and still stands; the attribution of *all* the 600 s
> to it does not. Both are FU-8e, in measured order.

## 11. FU-8d phase F4 (2026-08-06) — the treant sampled rather than inferred

> **Dated addendum. §§1–10 are left standing exactly as measured.** F4 changes
> **no engine code**: it is a clean measurement at the committed F3 tip plus the
> record sweep the every-record-site rule still owed F3. **No cap was raised,
> lowered or touched, and no cap code was edited.**

### 11.1 The run, and why there are two of them

Ruling **R-g** governs both: the run *and* its RSS watchdog executed
**unsandboxed**, because `ps -o rss=` is blocked in the sandbox and a watchdog
that cannot read RSS is decoration. Release, `--features census`,
`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, `PLINTH_THREADS=4`,
RSS polled every 2 s, hard kill armed at 90 GB, 600 s timebox, two attempts (the
maximum).

| attempt | verdict | wall clock | peak RSS | watchdog | typed refusal |
|---|---|---:|---:|---|---|
| 1 | EXCEEDS-TIMEBOX | 607 s | 2,606,256 KB (2.49 GiB) | armed 90 GB, **never fired** | none |
| 2 | EXCEEDS-TIMEBOX | 602 s | 2,600,960 KB (2.48 GiB) | armed 90 GB, **never fired** | none |

**No cap refused anything in either attempt.** That is the headline for this
document: since F3 the treant's merge gate states a demand of 9,273,857,317
against an unchanged 35,445,500,736 and is **admitted**. *(Updated 2026-08-07,
FU-8f phase G5: that demand is now **4,525,792,233** against the same unchanged
35,445,500,736 — the load term repriced, no cap touched. This section's headline
is unaffected and only strengthened. See §15.)* Peak RSS is 2.5 GiB
against the 90 GB kill line and the 100 GB machine ceiling — memory has not been
this stage's constraint since FU-8b, and this run is four orders inside it.

Attempt 2 exists because attempt 1's evidence was one 5-second window at
t = 600, which cannot separate "this frame dominates" from "this frame was
running at the end". Attempt 2 sampled the same binary at six points across the
timebox. The runs are otherwise identical.

**No batch counters are reported here.** The tip carries no
`PLINTH_FU8D_MEASURE` instrumentation (verified absent by grep before building),
so §10's per-batch table remains the counter record and F4 does not restate it as
independently reproduced.

### 11.2 What the 600 s is actually spent on

Main-thread samples per window:

| t (s) | main-thread samples | `fill_proposal_nearest_delta` | `assess_coverage_with_mode` | `merged_violation_count` |
|---:|---:|---:|---:|---:|
| 60 | 2,033 | — still in preparation | — | — |
| 180 | 2,268 | 1,386 (61.1 %) | 554 (24.4 %) | 115 (5.1 %) |
| 300 | 2,278 | 1,366 (60.0 %) | 574 (25.2 %) | 112 (4.9 %) |
| 420 | 2,275 | 1,382 (60.7 %) | 579 (25.4 %) | 104 (4.6 %) |
| 540 | 2,278 | 1,393 (61.2 %) | 563 (24.7 %) | 95 (4.2 %) |
| 597 | 2,264 | 1,402 (61.9 %) | 542 (23.9 %) | 95 (4.2 %) |
| **pooled** | **11,363** | **6,929 (61.0 %)** | **2,812 (24.7 %)** | **521 (4.6 %)** |

Attempt 1's separate window gives 58.5 % and 34.1 %. Read the split as
**≈ 59–62 % / 24–34 %** across two runs and seven windows; per ruling **R-e** no
literal is pinned in a test and this dated transcript is the record.

The leading frame is not doing arithmetic — it is *constructing*:
`fill_proposal_nearest_delta` builds a fresh `PointCloudIndex` kd-tree over a
**one-point** cloud, once per proposal, and its children in the call graph are
`knn_batch_planned` over allocator traffic (`_xzm_free`, `_free`,
`_xzm_xzone_malloc`). 39,223 tree constructions and teardowns per gate batch, each
to answer distances to a single point. It is batched *within* a proposal and
unbatched *across* proposals.

### 11.3 What this does and does not license

**It licenses no cap change.** None of the dominant work is charged against
`gate_operation_limit` at all, so no cap is "too tight" here; the remedy removes
work rather than admitting more. Every cap, floor and coefficient in §§1–10 is
untouched by F3 and F4 alike.

**Ruling R-f remains unsatisfied — twice over.** Neither attempt completes the
gate stage, so `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` (FU-23) is **not**
re-derived. R4's n = 1 marking is likewise unaffected.

**Ruling R-c is revived as a question, not adopted.** §9.5 declined the
radius-bounded steal path because the margin was 4.9× and the bound was a 1.7×
constant. F4 changes that calculus's *subject*: the frame R-c would have shrunk is
now the measured leader. It is recorded as FU-8e input; nothing is built here.

**`PLINTH_THREADS=4` buys the merge gate nothing** — all four rayon workers sit in
`__psynch_cvwait` for the whole gate stage while preparation is genuinely
parallel. This is not a reason to raise thread counts; it is a note that the gate
wall-clock above is a single-core number.

## 12. FU-8e phases E1–E4 (2026-08-07) — the treant's merge round falls 4.35×, and still does not fit

> **Dated addendum. Nothing in §§1–11 is rewritten, and this section changes no
> cap, floor or coefficient.** The demand §10 states — **9,273,857,317** against
> the unchanged materialization **35,445,500,736** — is byte-identical after
> FU-8e, deliberately, and §12.6 says why that immobility is the phases' proof
> rather than an oversight. *(Updated 2026-08-07: the immobility claim is exact
> **as of the FU-8e tip**, which is what this section records. FU-8f phase G5
> later reprices the load term and that demand becomes **4,525,792,233** against
> the same unchanged 35,445,500,736 — see §15. Nothing in §12 is retrofitted.)*

### 12.1 Ruling R-f is STILL NOT satisfied, and here is the arithmetic

Before any measurement below is read: **R-f is not satisfied by this phase, and
was never going to be.** `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` (FU-23) is
re-derived only from a Weeping Treant run that **COMPLETES** the gate stage.
FU-8e's clean run at its own tip is an **EXCEEDS-TIMEBOX at 601 s**, exactly as
F3's and F4's were, and the reason is arithmetic rather than luck:

- The merge tournament commits **exactly one** merge per round — the
  `consolidate` loop gathers, evaluates, accepts the first safe proposal and
  starts over — so the round count is bounded by the contact count: 1,366
  contacts admit at most **1,365** rounds.
- `make_raking_blade`, the only other proposal-carrying corpus model, runs its
  tournament to completion in **21 rounds, 62 → 41 contacts** — 33.9 % of its
  contacts consumed. At the same ratio the treant needs about **460** rounds.
- FU-8e's measured round is **13.8 s** mean inside the gate batch (§12.3) and
  **14.4 s** end to end including the gather and the commit (§12.4). So the
  post-fix gate stage is **≈1.8 h at the blade ratio and ≈5.5 h at the contact
  bound** — **11× to 33× the 600 s box**.

The run reached **33 merge rounds**, which is 7.2 % of the blade-ratio estimate
and 2.4 % of the contact bound. **This is not a completed gate and must not be
read as one.** F3's record over-claimed in this direction once; this section
states the shortfall first, before any number that might tempt a reader to forget
it.

### 12.2 Two framing corrections, both measured, both of which changed the fix

The FU-8e finding as opened (see `CURRENT-STATUS.md`) named two costs and had the
*mechanism* of both wrong. The corrections are the reason the phases work.

1. **The 62 % was never index construction.** One
   `PointCloudIndex::new_for_knn_batch` over a one-point cloud costs **83.3 ns**;
   the 39,223 of them in a gate batch total **≈3.27 ms**. The trees were
   free. The cost was the **query loop** — per query a `nearest_n` descent
   allocating a `SortedVec`, a second `nearest_n_within_exclusive` descent
   allocating a `Vec`, and a `sort_by`: about **1.04 × 10⁹ query iterations and
   2.08 × 10⁹ heap allocations per batch**. Phase E1 answers a one-source batch
   without a tree; E2 makes the gate consume it.

   > **Arithmetic corrected 2026-08-07 (records fix; this is the PRIMARY note on
   > the construction total, and every other site carrying it points here).** The
   > total above first landed as **16.7 microseconds**, which was an **arithmetic
   > slip in the design pass**, not a second measurement: 39,223 × 83.3 ns =
   > **3.267 ms**, and 16.7 µs across 39,223 builds would have implied 0.426 ns
   > per construction — below one memory access, so physically implausible on its
   > face. **The per-build 83.3 ns is retained unchanged**; it is the design
   > pass's measurement of record and nothing re-measures it here. **Only the
   > product is corrected**, and the conclusion it was quoted for is unchanged:
   > 3.27 ms is **0.009 %** of the 37.33 s `fill_proposal_nearest_delta` frame,
   > so index construction remains **negligible** — the honest total is 196× the
   > figure the slip stated, and negligible either way — and the fix still targets the
   > **query loop** behind the trees, exactly as E1 and E2 built it. No measured
   > quantity, cap, floor or coefficient moves.

2. **The 28 % was never allocation churn.** In `RepairNoEvidence` mode
   `assess_coverage` allocates **six** `Vec::with_capacity(samples.len())`, not
   "~7 … of order a terabyte", and its traversal visits only *uncovered* rows.
   What costs is the **two unconditional full scans** bracketing that traversal —
   the uncovered filter and the `worst_gap_mm` fold: 39,224 × 405,146 × 2 × 8 B
   ≈ **254 GB of reads per batch**, which is ~17 s at this machine's memory
   bandwidth. So the fix is *not to scan*, not *not to allocate*; E3's fallback
   allocates exactly as it always did.

A third correction landed inside E3 and is recorded in its commit: the design's
status-**flip** trigger was **unsound**, because `worst_gap_mm` is a `max` over
the nearest *values* of a failing island's samples. A row that stays uncovered
and moves 12 mm → 11 mm lowers it while flipping nothing, and a flip-counting
trigger would have handed the gate 12. The shipped trigger is the wider "an
uncovered row **moved**", which strictly contains the flip test.

A fourth, in phase E4 part (a): folding `merged_violation_count`'s two
per-proposal `Vec`s into batch scratch was measured at **0.13 %** of that
function's cost and **not adopted** — a merge proposal touches a mean of **3.77
contacts**, so each allocation is about fifteen bytes. See the
`SpecializedLoadDeltaScratch` doc comment.

**The pattern across all four is one thing: at this scale an allocation count is
a poor proxy for cost, and the row traffic is the thing.**

### 12.3 The round, decomposed — measured at the FU-8e tip

Conditions (ruling **R-g**: the entire run *and* its watchdog ran unsandboxed,
because `ps -o rss=` is blocked in the sandbox): 2026-08-07,
`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, release,
`--features census`, `PLINTH_THREADS=4`, RSS polled every 2 s, watchdog armed at
**90 GB**, 600 s timebox, **one attempt** of a permitted two. Timers were
env-gated scratch counters at the branch tip, reverted by editing afterwards
(`git status` clean against the commit; a grep for every scratch identifier
returns nothing). Per ruling **R-e** no total below is pinned in a test — the
dated transcript is the record.

| round | contacts | proposals | batch s | prep s | loop s | fill s | merged_violation s | restore s |
|---|---|---|---|---|---|---|---|---|
| base | 1366 | 0 | 3.313 | 3.313 | 0.000 | 0.000 | 0.000 | 0.000 |
| 1 | 1366 | 39223 | 14.679 | 3.952 | 10.727 | 4.672 | 5.495 | 0.517 |
| 2 | 1365 | 39212 | 15.148 | 3.249 | 11.898 | 5.331 | 5.958 | 0.564 |
| 3 | 1364 | 39209 | 18.244 | 5.697 | 12.547 | 5.624 | 6.269 | 0.604 |
| 4 | 1363 | 39203 | 15.355 | 5.772 | 9.584 | 4.291 | 4.797 | 0.460 |
| 5 | 1362 | 39189 | 12.740 | 3.234 | 9.505 | 4.223 | 4.793 | 0.455 |
| 6 | 1361 | 39180 | 12.717 | 3.141 | 9.576 | 4.275 | 4.811 | 0.455 |
| 7 | 1360 | 39174 | 12.835 | 3.321 | 9.514 | 4.245 | 4.769 | 0.464 |
| 8 | 1359 | 39169 | 13.398 | 3.255 | 10.143 | 4.508 | 5.095 | 0.498 |
| 9 | 1358 | 39167 | 14.319 | 4.306 | 10.013 | 4.451 | 5.046 | 0.476 |
| 10 | 1357 | 39166 | 14.132 | 3.406 | 10.726 | 4.839 | 5.309 | 0.523 |
| 11 | 1356 | 39165 | 13.513 | 3.986 | 9.527 | 4.252 | 4.781 | 0.455 |
| 12 | 1355 | 39160 | 13.660 | 4.152 | 9.508 | 4.242 | 4.775 | 0.453 |
| 13 | 1354 | 39157 | 12.104 | 2.992 | 9.112 | 4.069 | 4.575 | 0.435 |
| 14 | 1353 | 39155 | 12.623 | 3.027 | 9.597 | 4.282 | 4.823 | 0.456 |
| 15 | 1352 | 39152 | 13.171 | 3.720 | 9.451 | 4.219 | 4.742 | 0.452 |
| 16 | 1351 | 39149 | 12.400 | 3.161 | 9.239 | 4.133 | 4.630 | 0.442 |
| 17 | 1350 | 39147 | 12.078 | 2.988 | 9.090 | 4.058 | 4.564 | 0.435 |
| 18 | 1349 | 39145 | 13.912 | 3.011 | 10.901 | 4.882 | 5.463 | 0.514 |
| 19 | 1348 | 39143 | 16.979 | 7.421 | 9.559 | 4.269 | 4.794 | 0.456 |
| 20 | 1347 | 39142 | 20.794 | 7.156 | 13.638 | 6.212 | 6.714 | 0.653 |
| 21 | 1346 | 39141 | 13.057 | 3.571 | 9.486 | 4.237 | 4.759 | 0.453 |
| 22 | 1345 | 39136 | 13.009 | 3.445 | 9.564 | 4.270 | 4.802 | 0.454 |
| 23 | 1344 | 39133 | 13.065 | 3.527 | 9.538 | 4.256 | 4.790 | 0.453 |
| 24 | 1343 | 39131 | 13.584 | 3.957 | 9.627 | 4.309 | 4.815 | 0.463 |
| 25 | 1342 | 39129 | 13.785 | 4.262 | 9.523 | 4.255 | 4.776 | 0.452 |
| 26 | 1341 | 39128 | 12.649 | 3.366 | 9.283 | 4.141 | 4.662 | 0.443 |
| 27 | 1340 | 39122 | 12.871 | 3.168 | 9.702 | 4.344 | 4.862 | 0.459 |
| 28 | 1339 | 39114 | 14.616 | 5.327 | 9.289 | 4.146 | 4.665 | 0.444 |
| 29 | 1338 | 39103 | 12.349 | 3.004 | 9.346 | 4.170 | 4.696 | 0.445 |
| 30 | 1337 | 39100 | 12.290 | 3.008 | 9.282 | 4.146 | 4.659 | 0.443 |
| 31 | 1336 | 39098 | 12.117 | 3.002 | 9.115 | 4.068 | 4.577 | 0.436 |
| 32 | 1335 | 39097 | 13.652 | 2.925 | 10.728 | 4.847 | 5.334 | 0.504 |
| 33 | 1334 | 39096 | 14.141 | 4.656 | 9.485 | 4.233 | 4.761 | 0.453 |

Over the 33 merge rounds: batch **mean 13.818 s, median 13.398 s, range
12.078–20.794 s**; prep mean 3.884 s; variant loop mean 9.934 s; within the loop,
`fill_proposal_nearest_delta` 4.439 s, `merged_violation_count` (with its
`merged_rows` gather) 4.981 s, the baseline-row restore 0.475 s,
`variant_unsupported_count` 0.029 s, `variant_coverage_objective` **0.0010 s**,
residual 0.010 s. Peak RSS **2,565,232 KiB = 2.446 GiB**; the 90 GB watchdog
**never fired**; **no typed refusal at any cap**; verdict **EXCEEDS-TIMEBOX at
601 s**.

Against the design pass at `7aa6b73` (all scratch reverted), first merge round:

| | design pass | E2 tip | E4 tip |
|---|---|---|---|
| round total | 63.83 s | — | **14.68 s** |
| batch preparation | 3.56 s | — | 3.95 s |
| variant loop | 60.23 s | 25.28 s | **10.73 s** |
| `fill_proposal_nearest_delta` | 37.33 s | 3.58 s | 4.67 s |
| `assess_coverage`, all variants in the round | 17.05 s | 16.84 s | **0.001 s** |
| `merged_violation_count` | 4.66 s | 2.63 s\* | 5.50 s |
| restore / other | ~1.2 s | — | 0.55 s |

**Every cell in this table is a whole-round total for the first merge round**, on
the same basis as the header line above it — including the coverage row, which is
the sum over all 39,224 variants of that round and **not** a per-variant figure.
*(Row relabelled 2026-08-07, records fix: it read "coverage per variant", which
named the wrong basis for a per-round number. Only the label changed; 17.05 /
16.84 / 0.001 s are the figures as measured. Per variant the E4 column would be
≈25 ns, not 0.001 s.)*

**4.35× on the round, 5.6× on the variant loop.** Two rows need honesty rather
than celebration:

- **The fill went 3.58 s → 4.67 s between E2 and E4, and that is E3 being paid
  for.** E3 folds its two-counter probe into the fill's own row loop — about
  1 ns over 1.04 × 10⁹ rows. It buys the 16.84 s → 0.001 s on the row above it.
  Net −15.9 s. The phases are not independent and the record should not pretend
  they are.
- **\*The E2 figure is not comparable to the other two.** E2's timer bracketed
  `merged_violation_count` alone; the design pass's 4.66 s and E4's 5.50 s both
  include the `merged_rows` gather. Read that row as **4.66 s → 4.98 s mean, i.e.
  unmoved** — FU-8e never touched it.

### 12.4 The counters, and what did not move

- **`touched_rows` = 1,041,544,686** in merge round 1 — **to the row**, the
  figure F2 recorded and E2 reproduced, in two independent E4 runs. That is the
  arithmetic proof that the same distances were asked for by a path that no
  longer builds a tree.
- **Coverage fallbacks: ZERO.** Across 33 rounds and ~1.29 million proposal
  variants, `moved_uncovered_rows` = 0 and `flipped_rows` = 0 — **not one**
  variant needed its own `assess_coverage`. With `make_raking_blade`'s 1,402
  proposals (E3's record), the fast path is what runs on both proposal-carrying
  models. **The speedup is model-dependent; the answer is not.** A model whose
  merges churn uncovered geometry falls back to `assess_coverage` verbatim and
  lands back at the pre-FU-8e cost, never on a different objective.
- **The stated demand did not move**, and nor did anything derived from it.
  `plan_proposal_nearest_delta` still builds `KnnBatchPlan::new(1, touched_rows,
  1, limits)`, `work()` is still `1 × touched_rows`, and `logical_peak_bytes`
  still charges the kiddo terms for a tree nobody builds — a deliberate ~64 B per
  proposal over-statement, recorded as conservatism in E2's doc comment rather
  than restated, because restating it would move a charged number to save tens of
  bytes per proposal.
- **Where the other ~142 s went.** The run's 601 s is ~**127 s** of pre-gate
  pipeline plus the proposal-free baseline batch, then ~474 s of tournament — so
  **14.4 s** per round end to end, of which the `O(contacts²)` proposal gather is
  **30 ms**. The gather is not the problem and never was.

### 12.5 The successor frame, named plainly

Inside the post-FU-8e variant loop the leader is **`merged_violation_count` (with
its `merged_rows` gather) at 4.98 s — 50.1 % of the loop and 36.0 % of the
round.** `fill_proposal_nearest_delta` is second at 4.44 s (44.7 % / 32.1 %), and
batch preparation is third at 3.88 s (28.1 % of the round). Coverage, the frame
that opened this finding at 24–34 %, is now **0.01 %**.

FU-8d phase F4 sampled `merged_violation_count` at "a steady ~4.6 %", and FU-8e's
design predicted ~29 % after the fix. **Both are low: it is 36 %.** It is named
here so the next phase does not rediscover it as a surprise. It is also *not* the
thing to fix next — see §12.6.

### 12.6 What this licenses, and what it does not

**No cap change, again.** Nothing FU-8e removed was ever charged against
`gate_operation_limit`, so no cap was "too tight" for it; the phases delete work
rather than admit more. Every cap, floor and coefficient in §§1–11 is untouched
by E1–E4. E3's coverage fallback is the one *new* charge, and it is charged as
spent, into the same `load_delta_limit` headroom `executed_load_work` already
draws on, checked as a **sum** so the pair cannot jointly exceed the budget the
batch was admitted under. On the treant it is zero.

**Ruling R-f: unsatisfied.** Stated first, in §12.1, and repeated here.
`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is **not** re-derived. R4's n = 1
marking is unaffected.

**Ruling R-c (radius-bounded steal): measured, NOT adopted.** §9.5 declined it at
a 4.9× margin against a 1.7× constant; F4 revived it as a question because it
would have shrunk the then-leader. Post-FU-8e the frame it shrinks is 4.44 s, so
the most it can save is ~42 % of that — **1.9 s of a 13.8 s round, 13.5 %** — and
it saves nothing at all if FU-8f lands, because FU-8f evaluates one variant per
round rather than 39,223. **Re-measure after the tournament's shape settles;
build nothing now.**

**Ruling R-d (route-key concentration): NOT adopted.** Its win is amortising a
per-key sweep across the proposals that share the key. The post-fix row body is
**6.30 ns** and gather-dominated — a scattered read of the witness table and of
the sample point — so there is no per-key sweep left for it to amortise.

**`PLINTH_THREADS=4` still buys the merge gate nothing.** The wall-clock above is
a single-core number. That is a note, not a licence to raise thread counts.

**The next finding is FU-8f, and it is a RECORDED FINDING AWAITING OWNER RULING,
not an implementation.** See `CURRENT-STATUS.md`. In one line: the 39,224-vector
`vectors` array is consumed by exactly two lines, `first_safe_proposal` returned
index **0** in every round measured on two models, and evaluating lazily would
take the round to ~3.9 s — but it severs charged-batch from executed-batch and
re-founds `GateRoutePlanMismatch` a second time after F3, which is an owner's
call and not an agent's.

> **ADDENDUM 2026-08-07 (FU-8f phase G3) — the ruling came, and the paragraph
> above is now past tense.** It is left standing exactly as written. FU-8f is no
> longer awaiting a ruling and is no longer unimplemented: phase G3 landed the
> lazy first-safe acceptance it describes. See §13.

## 13. FU-8f phase G3 (2026-08-07) — the gate evaluates a prefix, and no cap moves

**Nothing in §§1–12 is touched by G3.** No cap, floor or coefficient is raised,
lowered or touched; no charged operation count moves; no byte term moves. The
treant's stated demand is still **9,273,857,317** against the unchanged
**35,445,500,736**, `exact_operations` still **10,457,910,996**, because
`gate_resource_plan` still prices `plan.variants = proposals + 1` and the
`exact_operations` fold still folds all 39,224. All seven frozen OD-8 digests are
byte-identical at the G3 tip. *(Updated 2026-08-07: those two figures are exact
**as of the G3 tip**, which is what this section records. Phase G5 later reprices
the load term to **4,525,792,233** and **5,709,845,912** against the same
unchanged cap — see §15. Nothing in §13 is retrofitted, and G3 itself still moves
no charged number.)* **This section exists to record what G3 changes
about the *meaning* of a charge, not about its value.**

### 13.1 Charged and executed are severed, and both are stated

Before G3 the merge gate's charged batch and its executed batch were the same
set: `plan.variants` were priced, admitted and then all evaluated. G3 evaluates a
**prefix** — the variants up to and including the first one that is
`gate_not_worse` than the batch baseline — and the two numbers part company.

**The charged side does not move, and that is the whole safety argument.**

- `gate_resource_plan` prices every variant.
- The `exact_operations` fold folds every variant.
- The refusal at `exact_operations > gate_operation_limit` runs **before** the
  loop, as it always has.
- `plan.vector_bytes` still charges for the per-variant vector, which is no
  longer materialised at all under the prefix mode.

So the batch is admitted on what it **may** spend, and it may still spend all of
it: the no-safe-proposal case evaluates every variant. **Charging the prefix
would be a bound not knowable until after the work that determines it** — exactly
the hole FU-8d part (3) closed, and the reason design open question 3 is answered
*no, emphatically*. Open question 4 (should `vector_bytes` shrink?) is answered
**not in G3**: it is a charged byte term, and moving one alongside a control-flow
change mixes two record sweeps. The executed peak does fall; the charge does not
follow it here.

**The executed side is metered.** `GateBatchOutcome::executed_variant_count`
joins `executed_touched_rows` and `executed_load_work` — the same *state a bound
from the plan, meter the real spend as it runs* shape §12 records for
`load_delta_limit`, and G3 adds no limit of its own. The invariant asserted is
`executed_variant_count <= plan.variants`, with the exact length stated rather
than only bounded. It is reported out of the stage as
`SpecializedGateEvidence::evaluated_variant_count`, beside the renamed
`gathered_pair_count`.

**An over-charge can refuse; it cannot admit.** Every term G3 leaves standing
over-states what the prefix executes, and an over-stating plan is the safe
direction: it may cost a batch a refusal it would not strictly have needed, and
it can never let a batch execute work it was not first afforded. That is the same
disposition G1 took for `load_resource_plan_from_counts` and the FU-8e Q5
precedent took before it. Repricing is G5's, and is not to be done as a side
effect of anything else.

**Appended 2026-08-07 (post-verification record):** the paragraph above covers
the up-front `exact_operations` refusal; lazy acceptance also shrinks a
**metered mid-loop refusal** — `load_delta_limit` is checked inside the variant
loop against the running `executed_load_work + executed_coverage_work`, which
under `StopAtFirstNotWorse` accumulates over the executed prefix only, so a
batch that would previously have refused `GateOperationLimit` at a variant after
the accepted index now succeeds; the direction is safe (the prefix's spend is a
subset of the full evaluation's spend, and the limit bounds *executed* spend),
and OD-8 measured zero.

### 13.2 `GateRoutePlanMismatch`, re-founded a second time

Landed verbatim on the error variant's doc comment and appended to F3's own
section in `CURRENT-STATUS.md`:

> `GateRoutePlanMismatch` is re-founded for the second time, and it is weaker
> again — said plainly rather than left to be discovered. At F1–F2 it compared a
> plan against a **second plan**. FU-8d phase F3 re-founded it as
> plan-vs-execution, attesting *"the batch executed the work it planned, once per
> proposal and **in order**"*, counting `touched_rows` from `touched.len()` as
> each delta filled and comparing measured peak bytes. FU-8f severs
> charged-batch from executed-batch, so *"every planned proposal was executed"*
> is no longer true and **must not be claimed**.
>
> What it attests now: **"the batch executed a prefix of the work it planned —
> the first `executed_variant_count` variants, in plan order, each exactly once,
> at the peak the plan stated"**, together with `executed_variant_count <=
> plan.variants` and the equality case. The three fingerprints are therefore
> compared **prefix-wise**: the executed nearest plan must equal the plan's own
> first `executed_variant_count` records, not its total.
>
> What is lost is the attestation that the tail was executed at all — and the
> tail is precisely what the owner's ruling authorises not executing:
>
> > *"2.1 hours down to 28 minutes seems worth it; anywhere else that we can also
> > still shave more time off?"* — Craig, 2026-08-07
>
> The trade is deliberate and is the ruling's, not the implementation's. What is
> **not** weakened: the batch is still admitted on the whole batch's charge, so
> nothing is executed that was not first afforded; and the no-safe-proposal path
> still evaluates every variant, so the refusal path's attestation is exactly
> F3's.

Mechanically, `GateWholeRoutePlan::executed_prefix` replays the plan's own
retained per-proposal deltas — the plan's records, in the plan's order — onto the
plan's baseline-only snapshot, and short-circuits to the whole plan when the
whole batch ran. The byte terms and the baseline sweep's fingerprint are carried
through untouched, so those comparisons remain preflight-against-execution across
two independent computations. **Only the proposal tail is relaxed.**

### 13.3 What this licenses, and what it does not

**No cap change.** Nothing G3 removed was ever charged against
`gate_operation_limit` — it was charged, and still is; what changed is that less
of it is spent. The remedy removes work rather than admitting more, as with
FU-8c, FU-8d and FU-8e.

**Ruling R-f: still unsatisfied.** `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is
**not** re-derived and must not be. The treant runs at the G3 tip — release,
`--features census`, `PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`,
`PLINTH_THREADS=4`, RSS watchdog unsandboxed (ruling R-g) armed at 90 GB and
**never fired**, 600 s timebox, two attempts — are **both EXCEEDS-TIMEBOX at
600 s with no transcript**, so no run has yet completed the treant's gate. They
were taken on a **quiet machine with no concurrent workflow**, unlike every
measurement in §§11–12, so the wall-clock is a clean number rather than an
inflated one. Attempt 2's peak RSS is **3,479,600 KB (3.32 GiB)**, against
FU-8e's 2.446 GiB — recorded as observed and **not** explained here: G1's charged
transpose is ~33 MB and cannot account for ~0.9 GB, and "the run got further into
the tournament" is a hypothesis rather than a measurement while there are no
counters. Counting the rounds and attributing that peak are both **G4's**
deliverables.

**Ruling R-c: still not adopted, and now moot in the shape §12.6 described.**
§12.6 recorded that the radius-bounded steal path "saves nothing at all if FU-8f
lands, because FU-8f evaluates one variant per round rather than 39,223". FU-8f
has landed. Re-measure after the tournament's shape settles; build nothing now.

**`PLINTH_THREADS=4` still buys the merge gate nothing.** Unchanged by G3, and
still a note rather than a licence.

## 14. FU-8f phase G4 (2026-08-07) — the rounds are counted; no cap moves, and none came close

**G4 changed no engine code.** Two instrumented treant runs, per-round counters
from env-gated scratch that was reverted by editing before the gates, and a
record. `git diff` over `crates/` at the G4 tip is empty. **Nothing in §§1–13 is
touched.** The treant's stated demand is still **9,273,857,317** against the
unchanged **35,445,500,736**, `exact_operations` still **10,457,910,996**.
*(Updated 2026-08-07: exact **as of the G4 tip**. G4 was deliberately sequenced
before the repricing precisely so its transcript would be the last measurement
stated in these numbers; phase G5 then moves them to **4,525,792,233** and
**5,709,845,912** against the same unchanged cap. **§14 is not retrofitted** —
every figure in it is correct at its own tip. See §15.)*

### 14.1 The measurement, without a contention caveat for once

Release, `--features census`, `PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`,
`PLINTH_THREADS=4`, RSS watchdog unsandboxed (ruling R-g) polling at 2 s, armed
at 90 GB and **never fired**, 600 s timebox, two attempts. **The machine was
quiet — no concurrent workflow of any kind.** Every millisecond in §§11–13
carried a 5–30 % contention inflation; these do not, and the two attempts agree
round for round to within 2 %.

| | attempt 1 | attempt 2 |
|---|---:|---:|
| outcome | EXCEEDS-TIMEBOX 602 s | EXCEEDS-TIMEBOX 602 s |
| merge rounds | **436** | **430** |
| contacts 1,366 → | 931 | 937 |
| mean round | 1,099.4 ms | 1,121.4 ms |
| peak RSS | 2,677,888 KB (2.554 GiB) | 2,650,144 KB (2.528 GiB) |

Fitted over all 436 rounds: `round_ms = 304.17 + 0.6924 × contacts`. Gate stage
alone projects to **504.7 s (8.4 min)** at `make_raking_blade`'s 463-round ratio
and **1,061.6 s (17.7 min)** at the 1,365-round hard bound.

### 14.2 No cap binds, and the two that could are quantified

- **`max_merge_iterations` = 100,000** against a 1,365-round hard bound. Two
  orders of magnitude of headroom. Untouched.
- **`max_merge_total_proposals` = 100,000,000.** This one accumulates across
  rounds and is the only per-stage cap the tournament's *length* can approach.
  Measured cumulative gather over attempt 1's 436 rounds: **15,544,641**.
  Extrapolating the measured `proposals ≈ 7,318 + 24.67 × contacts` to the full
  1,365-round bound gives **≈33.0 M** — a **3× margin**. Recorded so that a
  later, longer run does not meet this cap as a surprise, and stated as
  measurement rather than as a request: **it is not raised, lowered or touched.**
- **`gate_operation_limit`, `max_load_scratch_bytes`, `max_load_operations_floor`,
  `load_work_per_batch_unit`, `max_merge_commit_bytes`, `max_coverage_*`** — all
  unmoved and all admitted every round of both runs.

### 14.3 Memory: far under everything, and G3's 3.32 GiB is not reproduced

Peak RSS **2.554 GiB** and **2.528 GiB** against the 90 GB watchdog and the
100 GB machine ceiling — 2.6 % of the ceiling. §13's G3 run recorded
**3,479,600 KB (3.32 GiB)** as observed-and-unexplained and left attributing it
to G4. **G4 does not reproduce it.** Two independent instrumented runs at the
same tip land ~0.8 GiB lower, in FU-8e's own neighbourhood. G1's charged
transpose is 32,848,040 B in all three runs and explains neither figure. This is
recorded as a **non-reproduction, not an explanation**: the G3 peak's cause is
unknown, and it is not a cap question in either direction.

### 14.4 What the round is actually spent on — and why D4 was out by 2.8×

Frame means over attempt 2's 430 rounds, summing to the measured round:

| frame | ms | % |
|---|---:|---:|
| `gather_pair_proposals` (outside the gate batch) | 28.56 | 2.5 % |
| `unique_island_face_count` | 1.44 | 0.13 % |
| `GateBatchIndex::build` | 6.64 | 0.59 % |
| `plan_gate_nearest_batch` | 9.98 | 0.89 % |
| **`build_gate_load_state`** | **958.37** | **85.5 %** |
| `gate_resource_plan` + `exact_operations` fold | 0.56 | 0.05 % |
| baseline nearest sweep | 114.39 | 10.2 % |
| **the variant loop (G3's prefix)** | **0.52** | **0.046 %** |

Inside `build_gate_load_state`, from a bounded 32-round sub-frame probe:
`GroupedOwnerFaces::from_pairs` 136.0, `extend_load_eligibility` 17.1,
`append_variant_load_inputs` 13.6, `contact_rows` 8.1, and the transpose —
**`LoadTransposeCounts::count` 406.4 + `finish` 410.5 = 816.9 ms of build**
against **`scan_transposed_load` at 47.4 ms**.

**The charging discipline that costs those 406 ms is correct and must not be
traded away.** `count` exists so the cells-sized buffer is **charged before it is
allocated** — the ordering FU-8d part (3) established and G1 deliberately
preserved, and the reason `LoadTransposeCounts` is a separate type at all. What
is wrong is only that the count is a **second full traversal** rather than a
quantity the batch already has: `build_gate_load_state` already walks
`owner_faces` through `face_row` to *count* `contact_rows`, at 8.1 ms. Any future
phase that fixes this **must keep count-before-charge-before-allocate**; the
target is to make the count cheap, never to skip it.

### 14.5 Ruling R-f — STILL UNSATISFIED

Neither run completed the gate, so
`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is **not** re-derived and must not
be. G4 supplies the round count and cost model that the re-derivation will need;
it does not supply the completed run that R-f names. That re-derivation is its
own follow-up task and is **unblocked only by a run that finishes the
tournament**.

### 14.6 The unexercised remainder, named so it is not assumed

The pipeline never left `prepare_support_run`. **Nothing** is measured or claimed
about `pen_intervals_one`'s eager `24 × (T+1) × cap` bytes per ray per thread
(predicted 14–32 GB at `T = 4`), about the brace's `max_scratch_bytes` of 1e11
sitting **above** the 90 GB watchdog line, or about a first treant seal. Those
predictions stand exactly as they were, untested.

**[ADDENDUM 2026-08-09 — every prediction above still stands untested; one
number is now lookup-able rather than a bare literal in `runner.rs`.** FU-8g H1
(`08db663`) collapsed the eager `24 × (T+1) × cap` reserve this section names —
the census pass right-sizes it to the counted crossing count — so that specific
prediction's subject no longer exists in the form measured here; whether the
collapsed reserve fits is still H6's to measure, unchanged. The brace
`max_scratch_bytes` cap named above is, as of FU-8g H3, `SUPPORT_BRACE_
PENETRATION_MAX_SCRATCH_BYTES` in the M0 registry — still 1e11, still above the
90 GB watchdog line, and still unexercised; registering it changed what can be
looked up, not what has run. See §18.]**

## 15. FU-8f phase G5 (2026-08-07) — the honest load charge; the demand falls 4.75e9 and no cap moves

**Scope first, because the direction matters.** G5 changes **no** executed work
and **no** geometry. It changes one **charge**: the operation model that prices
the load finalizer's nearest-and-witness scan. Phase G1 had already replaced the
dense `contacts × rows` scan with a contact-major CSR transpose and had
**deliberately left the model standing and over-stating**, on the FU-8e Q5
precedent that a plan admitting the general path *a fortiori* admits its
specialisation, with the repricing sequenced here by design. This section pays
that debt.

**No cap, floor or coefficient is raised, lowered or touched, and none is
re-derived.** The stated demand moves **downward** at four quoted values, and a
falling demand can create no refusal.

### 15.1 What the model said, and what it says now

The retired term was

```text
face_count × contact_count × (owner_log + 4)
```

— one eligibility binary search for every `(row, contact)` cell, which is
precisely what the pre-G1 code did. What replaces it, in the same
elementary-comparison unit every other term of the model counts:

| term | where | what it prices |
|---|---|---|
| `Σ\|eligible_owners\| × (log + 2) × 2` | `load_resource_plan_from_counts` | `LoadTransposeCounts::count` and `::finish` each walk the row-major eligibility once, running one forward `partition_point` over the contact order per `(row, eligible owner)` entry |
| `contact_count × (log + 6)` | same | the contact-order sort's comparison bound (`log + 2`, which this line already carried) plus the transpose's four contact-sized passes |
| `cells × (LOAD_WITNESS_SLOTS + 5)` | `LoadTransposeCounts::operations`, added by `load_resource_plan` | 1 per cell for the fill's store and cursor bump, 4 for the distance and the argmin comparison — the very allowance the retired `owner_log + 4` carried for everything that was *not* the binary search — and `LOAD_WITNESS_SLOTS` for `insert_load_witness`, which FU-8d had charged *inside* that binary-search allowance |
| `Σ\|eligible_owners\| × owner_log × 2`, `face_count × (face_log × 2 + 1)` | same | **unchanged**; the eligibility's own ordering and the row emit, neither of which is the scan |

**The `cells` term sits in the exact plan, not the preflight, and that asymmetry
is the same one G1 established for the transpose's bytes.** `cells` is knowable
only after `LoadTransposeCounts::count` has walked the eligibility; no preflight
can state it without over-stating by orders of magnitude. So
`specialized_load_resource_plan` under-states this finalizer's operations by the
cells term, exactly as it already under-states its scratch bytes. **That costs a
later refusal, never unafforded execution**: the fill and the scan run only after
`finalize_load_rows`'s own exact check, and that check is the one carrying the
term.

### 15.2 The treant's batch header, old against new

Conditions: release, `--features census`,
`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2" PLINTH_THREADS=4`, RSS
watchdog unsandboxed (ruling R-g) polling every 2 s, armed at 90 GB and **never
fired**. **The machine was quiet — no concurrent workflow** — as it was for G4.
One instrumented release run, stopped at 260 s once the gate's first two batches
had printed; the instrumentation was env-gated (`PLINTH_G5`), never reached
without it, and removed by editing before the gates. **This was not a completion
attempt and is not offered as one.**

Round-1 merge-gate batch — samples 405,146, demands 1,095, contacts 1,366,
proposals 39,223, variants 39,224:

| term | before G5 | after G5 |
|---|---:|---:|
| `baseline_rows` | 554,926,572 | 554,926,572 |
| load baseline (`specialized_load_resource_plan`) | **5,543,831,115** | **795,766,031** |
| load proposal bound | 1,048,982,627 | 1,048,982,627 |
| **gate `load` term** | **6,592,813,742** | **1,844,748,658** |
| `demand_rows` | 42,949,185 | 42,949,185 |
| proposal work | 2,083,167,818 | 2,083,167,818 |
| **stated demand** (`gate_resource_plan`) | **9,273,857,317** — 26.2 % | **4,525,792,233** — **12.8 %** |
| `exact_operations` | **10,457,910,996** — 29.5 % | **5,709,845,912** — **16.1 %** |
| **cap — UNCHANGED** | 35,445,500,736 | 35,445,500,736 |

**The fall is 4,748,065,084 at both sites, to the unit, and it is entirely the
load term.** Every other line is byte-identical, which is the check that the
repricing touched what it claimed to touch and nothing else.

Arithmetic, so it can be verified rather than believed. At the 134,368-row
preflight the retired term was `134,368 × 1,366 × 27 = 4,955,760,576`; what
replaces it is `7,988,078 × 13 × 2 = 207,690,028` plus the `contact_count × 4`
widening of `1,366 × 4 = 5,464`. Net: **−4,748,065,084**, exactly.

Two more figures from the same run:

- **The accepted-load path**, 130,548 rows: **5,402,535,619 → 795,359,775**, a
  6.79× fall. This is the figure `TREANT_LOAD_OPERATIONS_REQUIRED` pins.
- **The finalizer's own exact plan**, 117,741 rows / 1,366 contacts /
  `Σ|eligible_owners|` 7,988,078 / **8,210,643 cells**:
  **4,714,113,843 → 644,970,917**. Of the new total, 207,690,028 is the search
  term, 65,685,144 the cells term, and 371,595,745 the two untouched terms — so
  the **scan** term falls **21×** (4,342,523,562 → 273,375,172) while the
  **total** falls 7.3×. The gap is stated rather than smoothed over: what is
  left is dominated by `Σ|eligible_owners| × owner_log × 2 = 367,451,588`, which
  is the eligibility's ordering and not the scan, and G5 did not touch it.

### 15.3 The corpus — and one place the demand does NOT fall

Every reachable corpus model was re-measured under the same instrumentation, one
`PLINTH_CENSUS_ONLY` invocation each. **Every model's widest load demand falls:**

| model | widest before | widest after |
|---|---:|---:|
| `Sphere` | 86,709 | 61,481 |
| `Cylinder` | 42,078 | 26,509 |
| `make_box` | 658 | 445 |
| `make_overhang_table` | 11,690 | 3,354 |
| `make_toed_slab` | 10,704 | 3,291 |
| `make_raking_blade` | 12,426 | 5,696 |
| `make_thin_fin` | 595 | 480 |

**"A falling demand can create no refusal" is true of every model's widest
demand and of every treant site, and it is NOT literally true checkpoint by
checkpoint — recorded rather than glossed.** On `make_box` and `make_thin_fin`
three individual load checkpoints **rise**, by at most 56 units: `make_box`
409 → 445 at (3 rows, 11 contacts, 8 owner entries, 20 cells), `make_thin_fin`
424 → 480 at (2 rows, 18 contacts, 6 owner entries, 19 cells). The cause is the
new model's *shape*: it is `Σ|eligible_owners|`- and `cells`-shaped, and on a
batch of three rows those two counts are larger relative to `rows × contacts`
than they are on any real workload. Every one of those checkpoints is more than
four million times under the **2,000,000,000 flat floor**, so none is anywhere
near a refusal; but the claim is stated as it measures, not as it would be
tidier.

### 15.4 The finding: the treant no longer leaves the load floor

`SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS` is a flat floor of **2,000,000,000**
lifted by the batch-product rule `max(floor, 64 × (rows + 1) × (contacts + 1))`.
The Weeping Treant was **the only model that ever left that floor**. At
795,359,775 it no longer does. **The batch-product scaling at the load stage is
now load-bearing for nothing in the corpus.**

**It is kept, unchanged, and here is why that is not inertia.** Applying the
derivation rule to the repriced observation gives
`next_power_of_two(2 × ceil(795,359,775 / 178,460,483)) = next_power_of_two(10)
= 16`, or 8 by the `ceil` of 4.46 — either way a **reduction** of
`SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` from 64. Lowering a cap is the direction
that creates refusals, FU-23/R4 trigger re-derivation on a **second specimen**
rather than on the same specimen getting cheaper, and G5's mandate is that no
cap is raised, lowered or touched. The derivation input of record therefore
stays the 2026-08-06 figure of 31, dated and attributed in
`crates/plinth-support/tests/resource_cap_policy.rs`, which now pins
`TREANT_LOAD_OPERATIONS_REQUIRED` (the derivation input) and
`TREANT_LOAD_OPERATIONS_REPRICED` (what the treant now asks) side by side and
asserts the second is below the floor.

### 15.5 Open question 4 — `plan.vector_bytes` — deliberately LEFT

The design record's open question 4 asked whether `plan.vector_bytes` should
shrink under `StopAtFirstNotWorse`, answered "not in G3; move the charge in G5
if at all". **G5 leaves it, and the reason is a distinction rather than a
deferral.**

G5's subject is a model that was **always** an over-statement of what the code
did — a modelling error, corrected. `vector_bytes` is not that. It is a
**charged batch bound**, the byte twin of `plan.variants`, and open question 3
already settled the principle for the variant count: *charge the batch, meter
the prefix, state both*, because a bound not knowable until after the work that
determines it bounds nothing. Shrinking `vector_bytes` to the executed prefix
would re-open exactly that hole on the byte side. It is left charged for the
full batch, over-stating executed bytes by construction and refusing nothing
that was not already refused.

What *is* left undone, and is named here so it is not mistaken for settled:
`check_gate_peak` reports `result_bytes: plan.vector_bytes` as the **executed**
peak, so the executed byte figure it attests is an over-statement now that the
vectors are never materialised. Metering the executed prefix's real bytes beside
the charged bound — the D1 "state a bound, meter the spend" shape, applied to
bytes — is a **follow-up**, not a G5 omission, and it is a byte-term record
sweep of its own.

### 15.6 What this licenses, and what it does not

- **Licensed:** reading the treant's merge gate as sitting at 12.8 % of its cap
  rather than 26.2 %, and reading the load stage as no longer needing its
  batch-product scaling.
- **NOT licensed:** re-deriving either batch-product coefficient. **Ruling R-f
  is STILL NOT satisfied.** G5 ran no completion attempt at all; its single
  measurement run was stopped at 260 s having reached the gate stage, which is
  not a completed tournament. G4's two attempts remain the round-count record,
  both EXCEEDS-TIMEBOX at 602 s.
- **NOT licensed:** reading any of this as a wall-clock improvement. G5 changes
  no executed work. The treant's stopping point is unchanged and is still
  wall-clock.
- **G4's transcript is deliberately left in the OLD numbers.** §14 was the last
  measurement stated before the repricing, which is why G5 was sequenced after
  it; its tables are correct as of their tip and are not retrofitted.

**Memory, observed and not explained.** The G5 measurement run peaked at
**3,538,624 KB (3.375 GiB)** at ≈154 s, shortly after the gate stage opened,
falling back to ≈2.4 GiB for the remainder. That is a **third** data point
beside G3's unreproduced 3.32 GiB and G4's two runs at 2.554/2.528 GiB, and it
is recorded as an observation only: G5 changes no byte term, the transpose's
charged 32,848,040 B is present in all four runs and explains none of the
spread, and the honest statement is that a short-lived transient near
gate-stage entry has now been seen twice and sampled at 2 s in every run. It is
26× under the 90 GB watchdog and 29× under the 100 GB machine ceiling. It is
not a cap question in either direction.

**Gates and OD-8.** Five gates green, verbatim, tests with `--no-fail-fast`.
**All seven frozen census digests byte-identical** at this tip — `Sphere`
17667468971446251870, `Cylinder` 13310185350867878190, `make_box`
9657051231007502658, `make_overhang_table` 6686599816688114065, `make_toed_slab`
11199296506741562956, `make_raking_blade` 13278215294653054002, `make_thin_fin`
7016743535301716631 — which is the check that matters here: **repricing changes
charges, never geometry.**

## 16. FU-8f phase G6c (2026-08-08) — the meter beside the charge; the arena's held bytes join the gate peak; no cap moves

**Scope, stated first.** §15 (G5) repriced what the load finalizer's CHARGE
prices. This section changes neither that charge nor any cap: it adds a
SECOND, separately-stated number — what a round actually EXECUTES — beside
the charge every round still states, and adds one new byte term for state
G6a/G6b's retention now holds across rounds. Pointers: §10–§13 priced the
gate's operation model this section meters against; §14 counted rounds; §15
is the repricing this section's charge figure inherits untouched.

### 16.1 The charge stays the admissible worst case; the meter is new

Every gate round — retained or rebuilt — is still charged
`specialized_load_resource_plan`'s figure over the round's fresh pair counts,
exactly as G6b landed it (§ above, "charging is deliberately untouched").
That does not change here. What G6c adds is `GateLoadState::executed_operations`
and `executed_load_build` (`Rebuilt` | `Retained`):

- **`Rebuilt`:** `executed_operations == baseline_operations`. The full build
  IS what the charge prices, so the two coincide by construction, not
  measurement.
- **`Retained`:** `RetainedLoadTranspose::executed_operations` prices only
  what `finalize_retained_round` actually runs — the G5-repriced per-cell
  scan allowance (`LOAD_WITNESS_SLOTS + 5` per retained cell) over `cells`,
  the O(rows) fill, the O(contacts) hash, and the last `record_commit`'s
  O(|l| + |r| + contacts) remap. The two eligibility traversals a full build
  pays twice (`LoadTransposeCounts::count`/`::finish`) are not in this figure
  — a retained round never runs them (F-G6-2/F-G6-3, §12/§13's own findings
  on what G1 deliberately left standing).

`executed_operations <= baseline_operations` is checked in both
`build_gate_load_state` and `evaluate_gate_batch` (`debug_assert!`, the same
disposition as the G3 executed/charged invariant — compiled out of release,
enforced by the census meter and a pinned test in debug/test builds), and
pinned by `fu8f_g6c_retained_round_executes_no_more_than_it_is_charged`.

### 16.2 The census meters the run total

`RetainedLoadCensus` gains `charged_operations`/`executed_operations` — `u128`
running sums over every load-build round of a run, retained or rebuilt —
printed on the gate's `T0_scaled_cap` row as
`retained_load_charged_operations=`/`retained_load_executed_operations=`.
This is the run-level twin of the per-round assertion: a full corpus run's
totals are now readable from one census invocation rather than re-derived.

### 16.3 The arena's held bytes join the gate peak — a NEW term, a DIFFERENT cap than the design record's own comparison

`RetainedLoadTranspose::retained_bytes` — the carrier's own arena (charged at
its 2×-compaction CEILING, `2 x cells x 4 B`, never the live length, because
compaction runs only once waste crosses that bound), `buckets`,
`contact_witness` and the row invariants, at their REAL sizes — now joins
`gate_load_state_bytes` (a new `Option<&RetainedLoadTranspose>` parameter)
whenever retention is live. This term feeds `check_gate_peak`'s
`load_baseline_bytes` addend, checked against `max_gate_peak_bytes`
(`SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES`, 1 GiB, **UNCHANGED**).

**Stated because it is easy to misread:** the design record's own ~66 MB
estimate for this term compares itself against `max_load_scratch_bytes`
(`SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES`, 512 MiB) for SCALE only. That
is a DIFFERENT cap, checked only inside `SpecializedLoadBaseline::build`'s
own internal `finalize_load_rows` check (§1's `load_resource_plan`), which a
retained round never reaches at all — it calls `finalize_retained_round`
instead. The arena's bytes are charged against `max_gate_peak_bytes`, not
`max_load_scratch_bytes`; both are UNCHANGED by this phase.

### 16.4 G1's per-round transpose-scratch term retires on retained rounds — already true since G6b, stated here

The 32,848,040 B term §1's own addendum on
`SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES` describes is charged inside
`load_resource_plan`, reached only from `SpecializedLoadBaseline::build`. A
retained round never calls that function, so that check — and the term it
charges — simply does not run on a retained round. This was already true the
moment G6b landed the retained fast path; G6c is where it is stated as a
record fact rather than left to be inferred from the control flow. §16.3's
new arena term is a SEPARATE charge at a SEPARATE cap and does not
substitute for it.

### 16.5 What this licenses, and what it does not

- **Licensed:** reading a run's retained-vs-rebuilt split and its
  charged-vs-executed totals from one census invocation
  (`retained_load_rounds`/`retained_load_rebuilds`/
  `retained_load_charged_operations`/`retained_load_executed_operations`).
- **Licensed:** treating the arena's held-bytes term as bounded and charged,
  not merely designed-for — `RetainedLoadTranspose::retained_bytes` returns a
  checked `Result`, same as every other byte term in this file.
- **NOT licensed:** reading `executed_operations` as a new CHARGE. It is
  metered, asserted `<=` the charge, and never subtracted from any cap check
  — the exact "charge the worst case, meter the spend" shape §14's own
  citation of the G3/FU-8d precedent already named.
- **NOT licensed:** any cap, floor or coefficient change. None moved.
- **NOT licensed:** a wall-clock claim. **G6d remains the measuring phase.**

**Gates.** Five gates green, verbatim, tree clean before and after: **1003**
tests default / **1004** `--all-features`, 0 failed, the **+1** over the G6b
tip attributed to `fu8f_g6c_retained_round_executes_no_more_than_it_is_charged`.
`make_raking_blade`'s 21 retained rounds ran under the new `debug_assert!`s
during this same test pass and did not trip either — the invariant held on
real corpus geometry, not only the fixture.

## 17. FU-8f phase G6d (2026-08-08) — the completed gate, measured twice; no cap moves, and the successor wall is named

**Scope first.** G6d changed **no engine code**. `git diff` over `crates/` at
its tip is empty. Every per-round and stage-boundary timer was env-gated on
`PLINTH_G6D_ROUNDS`, was never reachable without it, and was removed by editing
before the gates — the §14 (G4) precedent, followed exactly. **No cap, floor,
coefficient, charged number or byte term is raised, lowered or touched by this
section.** Its whole content is measurement.

### 17.1 The gate completes; the tournament is 823 rounds

Conditions: release, `--features census`,
`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`, `PLINTH_THREADS=4`,
RSS watchdog unsandboxed (ruling R-g) sampling every 2 s, armed at 90 GB and
**never fired**, 600 s timebox, two attempts, at the G6c tip `fe79124`.

| | attempt 1 | attempt 2 |
|---|---:|---:|
| whole-run wall-clock | **339 s** | **339 s** |
| merge rounds committed | **823** | **823** |
| gate batches | **826** | **826** |
| contacts, first → tournament exit → after coalesce | 1,366 → 543 → **542** | identical |
| cumulative proposals gathered | **20,059,412** | **20,059,412** |
| tournament wall-clock | 183.29 s | 184.22 s |
| peak RSS | 2,851,632 KB (2.719 GiB) | 2,653,888 KB (2.530 GiB) |

**Zero differing rounds across all 823** on contacts, proposals and the
retained-load rebuild count; every census counter matches to the unit; only
wall-clock differs, by 0.51 %. That reproduction — not the pre-run `ps` polling,
though thirteen polls over twelve minutes found no sibling `cargo`/`rustc` and
CPU idle stayed 85–92 % — is why the wall-clock here is landed rather than
marked CONTENDED.

**The tournament ends because no proposal in the final batch is safe**, not
because proposals ran out and not on a cap: after round 822 the loop gathered
2 more proposals and the acceptance fold returned no accepted index. Both prior
predictions are falsified in opposite directions — the blade ratio's 463 too
low, the 1,365-contact hard bound too high.

### 17.2 No cap binds, and two §14 projections can now be checked against truth

- **`max_merge_total_proposals` = 100,000,000** against the whole tournament's
  **20,059,412** — a **4.99× margin**, now a total rather than an extrapolation.
  §14.2 projected "**≈33.0 M** at the hard bound"; the truth is 20.1 M, so that
  projection over-stated by **64 %**. §14's own measurement (15,544,641 over 436
  rounds) stands untouched; only its extrapolation is corrected, and **§14 is
  not retrofitted**.
- **§14.1's `round_ms = 304.17 + 0.6924 × contacts` is excellent inside its
  fitted range and does not extrapolate below it.** Its 504.7 s / 1,061.6 s gate
  projections rested on that extrapolation. The reason it fails is now measured:
  **proposals collapse superlinearly** (39,223 → 3 over the run; 24,826 → 65
  over the last 300 rounds), so the round falls far faster than contacts do.
- **`max_merge_iterations` = 100,000** against 823 tournament merges plus the
  coalesce pass's one — two orders of magnitude of headroom, now measured.
- **`specialized_gate_operations`** materialized **35,445,500,736**
  (`at_floor=false`) and **`specialized_load_operations`** **11,421,470,912** —
  both unchanged, both admitted every round of both runs.
- **Every other cap was admitted every round. None is raised, lowered or
  touched.**

### 17.3 Retention, measured on a whole tournament, with the pre-G6 round as an internal control

826 load builds; **804 retained (97.3 %)**; 22 counted rebuilds — 21 `poisoned`,
1 `contact_witness`. The `contact_witness` miss is the terminal audit after
`coalesce_exact_regular_seats`, which is **F-G6-4's predicted site**, once per
tournament, ≈0.2 s of a 339 s run. Run-wide charge-vs-meter (§16's severance):
charged **433,886,641,066**, executed **48,520,980,855** — **11.18 %** — with
`executed ≤ charged` holding every round.

The 21 rebuild rounds **are** the pre-G6 round, measured at this tip in the same
process; the 15 of them inside §14.1's fitted contact range reproduce §14.1's
model to a mean of **−0.97 %** (worst −3.2 %). Against that control the retained
round fits `round_ms = 71.46 + 0.1405 × contacts` (n = 802), means **206.5 ms**
against **879.6 ms**, and is **4.4–5.2×** cheaper across the fitted range. The
G6 design's D-G6-5 predicted 0.19 s and a 350–450 s whole run; measured 0.206 s
and 339 s — **the first prediction in this chain to land**, though it landed on
the round and **not** on the round *count*, which it priced at 1,365.

### 17.4 Memory: §14.3's open question is closed by falsification, not by explanation

Peak **2.719 GiB** (attempt 1, mid-tournament) and **2.530 GiB** (attempt 2,
≈10 s after gate-stage entry); watchdog armed at 90 GB, never fired; 2.7 % of
the 100 GB machine ceiling. §13 recorded 3,479,600 KB and §15 3,538,624 KB, both
unexplained, and the standing reading was that those runs "got further into the
tournament". **These two runs go 823 rounds — the entire tournament, 1.9× §14's
depth and further than any run in this project's history — and peak below both.
Tournament depth is therefore not the explanation, and that hypothesis is
retired.** What remains is an observation: the two attempts disagree on *when*
the peak falls and on its size by 7.4 % while being bit-identical on every
counter, which is 2 s sampling landing on a transient of a few hundred MB. No
run has exceeded 3.4 GiB. G6's retained arena is charged and ~66 MB-class and
explains none of the spread. **Not a cap question in either direction.**

### 17.5 The successor wall — named, derived, and NOT a cap to raise (FU-8g)

`Preparation: Initial(Limit { resource: "work items", required: 232585208,
limit: 4000000 })`, identically in both attempts, **after** the gate completes.
`initial::preflight` checks `work = contact_count × 4 + penetration.max_total_work`
against the **flat** `ContactConstructionLimits::production().max_work_items =
4_000_000`. With `rays_all = false`, `penetration_requirements` gives `rays = 1`
and `hits_per_ray = mesh.nfaces()`, so

> `work = contact_count × (4 + nfaces) = 542 × 429,124 = **232,585,208**`

reproducing the printed figure to the unit. The demand is **58.1×** the cap, and
at 429,120 faces the cap admits at most **9** physical contacts. **This is the
FU-8b / FU-8c shape a fourth time — a `contacts × faces` product against an
unscaled flat bound — and the quantity is a preflight *bound*, not a measured
demand**: `hits_per_ray = nfaces` is "every face could be hit" while the executed
query is BVH-bounded, the same over-statement §15 corrected on the load term.
**No cap is raised here.** The standing owner ruling holds; the remedy, when it
is designed, is §15's remedy — make the bound honest.

**[ADDENDUM 2026-08-09 — the remedy this section named was applied, and no cap
moved.** FU-8g phases H1/H2 made the bound honest exactly as §15 did on the load
term: `initial::preflight` now charges the census pass's counted `Σ tests`
(executed traversal effort — BVH nodes popped plus Möller evaluations) instead
of `rows × nfaces`, with `work` and `hits` severed, per-row budgets, and a
charged-vs-executed receipt checked at the charge site. **The formula quoted
above is therefore no longer what the engine computes**, and the treant's charge
at 553 contacts is **108,378** — **2.71 %** of the same, untouched 4,000,000 cap
rather than 58.1× it. `ContactConstructionLimits::production()` is byte-identical
to `develop @ e3168c5`: **no cap raised, lowered or touched**, and the D5.3
contingency (scaling the cap by the FU-8b batch-product rule) is **not
triggered and not added**. Two corpus models' honest charge is *higher* than the
worst case it replaced (`make_box` 205 vs 176, `make_raking_blade` 975 vs 924 —
node-visit overhead exceeds `nfaces` on a 12/24-face mesh), four orders of
magnitude under the cap in both cases. A fixture proves an over-demand mesh is
still refused at the unchanged caps. Full record, with the per-model
before/after table and the deviations: *FU-8g phase H2 landed 2026-08-09* in
[`CURRENT-STATUS.md`](CURRENT-STATUS.md#fu8g-h2-2026-08-09). M0 registration of
these four `ContactConstructionLimits` numbers, this document's §18 and the
`constants.md` regeneration remain owed by FU-8g H3.]**

**[ADDENDUM 2026-08-09 — the debt above is paid, and nothing here is
rewritten.** FU-8g phase H3 registered all four `ContactConstructionLimits`
numbers (plus brace's six `PenetrationBatchLimits` fields and raft's four,
found equally unregistered by the same design pass) as `Placeholder`,
classification `runtime_resource_cap`, with honest provenance — single-specimen
markers where a measurement exists, a plain statement of absence where it does
not. `constants.md` was regenerated by its producer. **No cap, floor or byte
term was raised, lowered or touched by the registration.** Full record: §18,
below.]**

### 17.6 Ruling R-f — the precondition is MET; the re-derivation is NOT done

R-f requires a treant run that **completes** the gate. Two now have, with
counters, reproducibly. **`SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` is
therefore UNBLOCKED and is deliberately NOT re-derived here** — the G6 design's
D-G6-6 assigns it to its own later phase, and G6d obeys that. **No coefficient
moved.** The inputs a re-derivation will want (826 gate batches, 20,059,412
proposals, materialized 35,445,500,736, twice) are recorded in §17.1–§17.2.

### 17.7 The unexercised remainder, still unexercised

`prepare_support_run` still returns `Err`. **There is no treant seal.** Routing,
rewire, penetration/brace, the CCO anchors, attestation, lowering,
`routed_contacts`, `final_live_members` and a snapshot digest are all still
untouched, and **nothing** is measured or claimed about `pen_intervals_one`'s
predicted 14–32 GB at `T = 4` or the brace's `max_scratch_bytes` of 1e11 sitting
above the 90 GB watchdog line. Those predictions stand exactly as §14.6 left
them. The only change is that the pipeline now reaches a **later** refusal than
it ever has.

**[ADDENDUM 2026-08-09 — the remainder is still unexercised; three more of its
caps are now named in the registry rather than only in `runner.rs`.** FU-8g H3
registered `SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES` and its five siblings
plus the four `SUPPORT_RAFT_*` caps as `Placeholder`, with honest provenance
disclosing that none of them has a measurement of record — the brace and raft
stages have never run. **No cap was raised, lowered or touched, and no run
happened.** See §18.]**

### 17.8 Gates

Five gates green, verbatim, tests with `--no-fail-fast`, tree clean before and
after: **1003** default / **1004** `--all-features`, **0 failed** — identical to
the G6c tip, as it must be, since G6d adds no test and changes no code. Clippy
clean at `-D warnings`, `cargo fmt --all -- --check` clean, `git diff --check`
clean. **G6e is not opened**: D-G6-6 makes it contingent on a whole-run margin
below ≈1.5× and the measured margin is **600 / 339 = 1.77×** — and more
decisively, the gate is no longer the binding stage, so the ≈117 ms/round
baseline sweep would move the stopping point not at all. Design open question 3
(retiring the per-round `owner_faces`/`pair_starts` rebuild) is **declined
again, measured**: gather runs 28.6 ms early and 5.7 ms late of a tournament
that is 183 s of a 339 s run.

## 18. FU-8g phase H3 (2026-08-09) — the four `ContactConstructionLimits`, six brace penetration limits and four raft limits are registered; nothing is raised, lowered or touched

**Scope, stated first.** H3 changes **no engine code**. `git diff` over `crates/`
and `rust/` at this tip is empty; the entire diff is the M0 placeholder
registry table, its three consuming test fixtures (id list, row table, entry
counts), the source ledger, the generated `constants.md`, and this document and
`03-physics-and-constants.md` §3.10.9. **No cap, floor, coefficient or byte term
is raised, lowered or touched.**

### 18.1 What was unregistered, and why it matters

F2 of `fu8g-design-2026-08-08.md` found that `SupportPreparationLimits::production`
(`runner.rs`) hard-codes `ContactConstructionLimits::new(1_000_000, 10_000_000,
4_000_000, 512 × 1024 × 1024)` (`:235–241` at this tip) — the same four numbers
`ContactConstructionLimits::production` (`initial.rs:63–70`) hard-codes
independently — plus `brace_penetration_limits` (`PenetrationBatchLimits`, six
fields, `:242–249`) and `raft_limits` (`RaftLimits`, four fields, `:250–251`).
Owner ruling R5 (2026-08-05) registered 42 other orphan caps but named these
four nested authorities **explicitly out of scope**, deferring them to "their
own pass" (`03-physics-and-constants.md` §3.10.9, and repeated verbatim in
§17.5's H2 addendum: "M0 registration of these four `ContactConstructionLimits`
numbers, this document's §18 and the `constants.md` regeneration remain owed by
FU-8g H3"). Until this section, **"no cap was raised" for the whole FU-8g
sequence — H1's census pass, H2's charging rebase — was unverifiable from the
registry**: the number a reader would need to diff against had no entry.

### 18.2 What is registered, and on what terms

All 14 numbers are registered as classification `runtime_resource_cap`, wrapper
`Placeholder`, exactly on §3.10.9's own terms — **registration is disclosure,
not ratification** — with `source_locators` naming both hard-coding sites for
the construction four (the design's F1 duplication finding is recorded, not
resolved: neither site reads the registry after this phase).

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

Every value is byte-identical to `develop @ e3168c5` and to `git blame`'s own
`73e7c132` on `runner.rs:235–251` — the same Codex-era commit R5's ledger rows
already cite for the sibling caps in this family. Registering a number does not
move it; the assertion in H2's refusal fixture already pins
`ContactConstructionLimits::production()` byte-identical, independent of this
section.

### 18.3 Provenance is honest about what is and is not measured — single-specimen markers where a measurement exists, and none invented where it does not

Per R4's precedent (single-specimen, n = 1, markers on measurements that rest
on the treant alone) and the standing rule against inventing a demand figure a
run has not produced:

- **`SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS`** carries FU-8g H1/H2's measured
  demand: the Weeping Treant v1.2 (553 contacts) charges **108,378** against
  this **unchanged** 4,000,000 cap — **2.71 %** — where the worst-case bound H1
  replaced charged 237,305,572, 58.1× over (single specimen, n = 1, per
  R4/FU-23; §17.5, §H2.1).
- **`SUPPORT_CONSTRUCTION_MAX_CONTACTS`** carries the same run's contact count,
  553 against 1,000,000 — **0.055 %**.
- **`SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES`** carries H0's **pre-H1** honest
  estimate, 149,188,000 B (≈142 MiB, 27.8 % of the 536,870,912 B cap), with
  H2.1's own disclaimer restated in the registry note: the post-H1 per-model
  byte charge is **not** claimed here and is H6's to measure.
- **`SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS`** and **all ten brace and raft
  constants** carry **no measured demand**, stated plainly rather than
  papered over: FU-8g's H0/H2 measurements priced work items and contacts, not
  load rows, and the brace and raft stages are unexercised on every corpus
  model and on the treant at this tip — no run has ever reached them (§17.7).
  The design's F7 finding for the six brace fields is registered as what it is,
  a **derived** worst-case ceiling from the pre-fix sizing formula (at
  `rays_all=false`, cap 80, `T = 429,120` faces, production admits at most 85
  clearance rods once `max_scratch_bytes` is accounted for), never as a
  measured demand — and `SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES`'s note
  additionally discloses that this same cap sits **above** the 90 GB watchdog
  line and the 100 GB machine ceiling (§17.4/A1), a standing hazard this
  registration surfaces and does not resolve.

### 18.4 `CandidatePayloadConfig` remains out of scope

R5's boundary stands: `CandidatePayloadConfig` mixes result-changing refinement
policy with execution caps and needs its own classification pass, not a
`runtime_resource_cap` sweep. This section does not touch it.

### 18.5 Records swept

- `03-physics-and-constants.md` §3.10.9 gains a dated addendum (2026-08-09)
  correcting its own "not covered" sentence for three of the four named
  authorities, with the full 14-row table.
- `constants.md` was regenerated **by its producer**
  (`cargo run -p plinth-core --offline --example render_constants_golden`);
  the diff is 14 additive rows, alphabetically placed, nothing else moved.
- `m0-constants-source-ledger.json` gains 14 `registry_entries` rows, appended
  after the existing tail (the same shape R5's own 42 additions used rather
  than a whole-file resort); `constants_ledger_consistency`'s FU-24 gate is
  green.
- §17.5's H2 addendum ("M0 registration ... remain owed by FU-8g H3") is
  satisfied by this section and is left standing, per the append-only
  convention — this section is the fulfillment, not a rewrite of that promise.
- §14.6 and §17.7, both of which record the brace/raft stages as unexercised
  and their predictions as unmeasured, gain a one-line dated addendum each
  pointing here: registration changed what can be looked up, not what has run.

### 18.6 Gates

Five gates green, verbatim, tree clean before and after: `cargo test
--workspace --offline --no-fail-fast` and `--all-features` both **1042 == 1042,
0 failed**, `cargo clippy --workspace --all-targets --all-features --offline --
-D warnings` clean, `cargo fmt --all -- --check` clean, `git diff --check`
clean. The seven frozen OD-8 census digests are unaffected by construction —
this phase touches no code the census exercises — and are not re-run for a
docs-and-registry-only phase; the standing digests are named in the branch's
own commit trailer discipline and remain the values of record.

## 19. FU-8g phase H6 (2026-08-09) — the treant clears the initial-contact wall; no cap moves, and the successor wall is not a cap at all

**No cap, floor, byte term or coefficient was raised, lowered or touched by this
phase.** H6 changes no code at all: `git diff` over `crates/` and `rust/` is
empty at its tip. This section records what the treant's caps did when a real
run finally reached and passed them, and names what binds instead.

### 19.1 The run

Two attempts, the standing memory protocol without deviation — release,
`--features census`, `PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY="Weeping Treant v1.2"`,
`PLINTH_THREADS=4`, RSS watchdog unsandboxed polling every 2 s with a hard kill
armed at 90 GB, 600 s timebox, two attempts and no more. Machine quiet at both
launches (no other `cargo`/`rustc`/census process). Full transcript and
diagnosis: *FU-8g phase H6 measured 2026-08-09*
([`#fu8g-h6-2026-08-09`](CURRENT-STATUS.md#fu8g-h6-2026-08-09)).

Both attempts ended **EXCEEDS-TIMEBOX at 600 s** (600 s / 601 s wall, SIGKILL by
the watchdog), peak RSS **3,693 MiB** and **3,721 MiB**.

### 19.2 What the caps did — the first time any of them was reached on the treant

- **`ContactConstructionLimits::production().max_work_items = 4_000_000`** — the
  cap this whole follow-up was opened against. **The treant PASSED it.**
  `initial::build_initial_contacts` ran to completion (sampled at t = 200 s,
  100 % of the main thread; complete by t = 267 s). H2's measured charge of
  **108,378 — 2.71 % of the untouched 4,000,000** — is confirmed not merely
  smaller but *sufficient*: the stage executed, `CensusDrift` never fired, and
  no downstream consumer of the census refused. Under §17's pre-H1 arithmetic
  the same run demanded 237,305,572, or **58.1×** this cap.
- **D5.3's contingency is NOT triggered and is NOT added.** The design bound it
  in advance: *if* H6 put the treant's `work` above the 4,000,000 floor, the
  remedy was a measured `SUPPORT_INITIAL_CONTACT_WORK_PER_CONTACT_OBSERVED` and
  a derived per-contact scaling under `next_power_of_two(2 × observed)`. The
  floor was not crossed, so **the scaling is not added** — per the design's own
  rule that adding unmeasured scaling is the same error as raising a cap, in a
  nicer suit.
- **The six `brace_penetration_limits` fields and the four `raft_limits`
  registered in §18 remain unexercised.** The run never left routing, so brace
  was never entered. §18's honest disclosure — "a plain statement of absence for
  all ten brace/raft constants, which no run has ever exercised" — **still
  stands, unchanged, at this tip.** §14.6's and §17.7's standing note that the
  brace/transit stage is unexercised and its predictions unmeasured is likewise
  unchanged. Nothing here licenses a brace number.
- **The 90 GB watchdog arm was never approached.** Whole-process peak 3,721 MiB
  = **4.0 %** of the arm. The standing hazard recorded in §17 and in the design's
  §A1 — that brace's `max_scratch_bytes = 1e11` sits above both the watchdog and
  the ~100 GB machine ceiling, so the machine binds before the cap does —
  **remains untested by measurement**, because brace was never reached. It is
  neither confirmed nor cleared by this run.

### 19.3 What binds instead — and why no cap can fix it

The treant's successor stopping point is **not a cap**. From t ≈ 267 s to the
600 s kill the main thread is **100 % inside
`PreparedSupportRun::route_initial_cco`**, and within it the share of wall-clock
spent in `CcoController::digest` **rises** 19.7 % → 64.4 % → 77.2 % → 77.2 % →
77.1 % across sample windows at t = 280 / 360 / 440 / 520 / 575 s, while the
constant-per-terminal `validate_replay` term's share **falls** 31.0 % → 13.8 % →
9.0 % → 9.0 % → 9.0 %. A growing term overtaking a constant one: the measured
signature of a quadratic.

Derived to the line, independently of the profile: the routing loop
(`runner.rs:1650–1666`) calls `grow_terminal_with_finalized_anchor` once per
terminal; its first statement (`cco.rs:1397`) is `self.digest()`; and
`CcoController::digest` (`cco.rs:1775–1862`) walks the entire accumulated
controller state on every call — every terminal in the cadence, every terminal
record, every `UnroutedNoWinner` record's **whole anchor-refusal list** via
`digest_anchor_seat_refusals` (`cco.rs:2128–2149`), and the whole host kd-tree
sample set. Cost `Θ(N²) + Θ(N·R) + Θ(N·H)`.

**This is the batch-first violation family a sixth time** (after FU-8b, FU-8c,
FU-8d, FU-8e and FU-8g's own `contacts × faces` bound), and it is the first
member of that family that **no cap policy could have caught**: nothing refuses,
no resource is over-charged, no limit is consulted. The measurement-first
doctrine this document exists to enforce found it anyway — but it found it with a
profiler, not with a preflight. Opened as **FU-32**.

### 19.4 Gates

Five gates green at this tip, verbatim, tree clean before and after:
`cargo test --workspace --offline --no-fail-fast` **1052 passed, 0 failed**, the same
`--all-features` **1054 passed, 0 failed**, `cargo clippy --workspace --all-targets
--all-features --offline -- -D warnings` clean, `cargo fmt --all -- --check`
clean, `git diff --check` clean. OD-8: all seven frozen census digests reproduced **byte-identical** from the release `--features census` build at this tip, one `PLINTH_CENSUS_ONLY` invocation each — Sphere 17667468971446251870, Cylinder 13310185350867878190, make_box 9657051231007502658, make_overhang_table 1875674282253663076, make_toed_slab 2823659516373762066, make_raking_blade 9567737006650358297, make_thin_fin 8797083662933568276. The Weeping Treant is deliberately excluded from the digest set: it has never sealed and produces no snapshot digest.

## 20. Digest supersession, 2026-08-09 (FU-32 phase J4) — dated addendum

**Nothing above in this document is rewritten, retracted or amended.** Every OD-8
`snapshot_digest` quoted anywhere in it — including §19.4's gate record immediately above and
§4's canonical table — is correct **as a record of its own date**. Under the owner's 2026-08-09
ruling, verbatim — *"The fingerprints aren't sacred  if things change slightly that's to be
expected (because the algorithm wasn't right when the fingerprints were generated!)"* — **all
seven OD-8 census digests were re-pinned**, because FU-32 J2 replaced `CcoController::digest`'s
per-terminal whole-state walk with state-pure incremental accumulators and the digest therefore
folds a different stream. The live values are Sphere `2297157889062365577`, Cylinder
`6853793710942395608`, `make_box` `8709076999423814412`, `make_overhang_table`
`16396858890798947253`, `make_toed_slab` `133000971864625681`, `make_raking_blade`
`18388631460456455184`, `make_thin_fin` `14656364680512457190`.

**The support itself did not change, and no cap in this document moved.** 687,288 dumped
routing-output content lines and 275 transcript rows are byte-identical parent-to-tip across
all seven models; `routed_contacts` and `final_live_members` are unchanged on every one; and
FU-32 phases J0–J4 raised, lowered or touched **no** cap, floor, constant or registry entry.
The authority table and the full attribution are `CURRENT-STATUS.md` `#fu32-j4-2026-08-09` and
`#fu32-j3-2026-08-09`.
