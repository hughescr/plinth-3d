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
