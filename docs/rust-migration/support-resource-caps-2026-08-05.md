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
against an unchanged 35,445,500,736 and is **admitted**. Peak RSS is 2.5 GiB
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
> rather than an oversight.

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
