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
