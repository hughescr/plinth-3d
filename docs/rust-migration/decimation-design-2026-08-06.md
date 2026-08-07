# Decimation design record — 2026-08-06

> **Provenance.** Produced 2026-08-06 by a read-only architect agent against `develop @ eb36358`
> (working tree untouched), drawing on the live measurement artifact
> `target/plinth-decimate-corpus-budgets/corpus-budget-report.json` (written 2026-08-05 22:15 by
> `crates/plinth-decimate/tests/corpus_budgets.rs`). This document is the evidence behind the owner's
> 2026-08-06 re-prioritization ruling: **FU-8b (gate/load quadratics) runs first; decimation is
> demoted** to a diagnosis + witness follow-up, because the measurements below falsified the
> compounding-speedup premise of the original decimation priority. Handed off verbatim by the
> orchestrator; commit under `docs/rust-migration/` unmodified below this banner.

---

## 0. Four things in the original framing that the source contradicts

**(a) The treant's search result is NOT discarded by the utility gate. It is discarded by the topology gate, and `assess_downstream_utility` never matters.**

Every reduced treant candidate fails `topology_not_increased` (`crates/plinth-decimate/src/lib.rs:1843`), because `nonmanifold_edges` goes from 0 to non-zero at *every* attempted ratio:

| target faces | reduction | nonmanifold edges after | result |
|---|---|---|---|
| 425,934 | 0.74 % | 3 | TopologyIncreased |
| 422,750 | 1.5 % | 3 | TopologyIncreased |
| 419,564 | 2.2 % | 6 | TopologyIncreased |
| 416,380 | 3.0 % | 7 | TopologyIncreased |
| 410,010 | 4.5 % | 11 | TopologyIncreased |
| 403,640 | 5.9 % | 16 | TopologyIncreased |
| 390,898 | 8.9 % | 24 | TopologyIncreased |

`geometrically_eligible_candidate_count` for the treant is **0**. Granting `DownstreamUseful` — by any definition — changes the treant by exactly zero faces. The `NoDownstreamUtilityEvidence` refusal recorded in its trace is the *last* refusal appended at `lib.rs:1427`, not the operative one; the operative ones are the two `TopologyIncreased` entries at `lib.rs:1330`.

**(b) Even ignoring topology, the treant's ratified geometric budget admits only ~6–9 % reduction, not a shrink that "compounds across everything".**

From the same report, sampled p99/p99.9 against the limits 0.0095 / 0.019 mm:

| requested ratio | output faces | p99 mm | p99.9 mm | passed |
|---|---|---|---|---|
| 0.5250 | 225,242 | 0.02340 | 0.03983 | no |
| 0.7625 | 327,190 | 0.01379 | 0.02639 | no |
| 0.8813 | 378,160 | 0.00959 | 0.02044 | no |
| 0.9406 | 403,640 | 0.00643 | 0.01557 | **yes** |
| 0.9109 | 390,898 | 0.00815 | 0.01814 | **yes** |

The treant is genuinely near print-scale at half-a-pixel p99. A 47 % reduction already costs 23 µm of p99 — 1.2 pixels. So the FU-8b arithmetic the motivation rests on (faces × minima, 81F+36R+4, 18.6 rows/face) improves by ~9 % at best, not by an order of magnitude. At r ≈ 0.91 the gate charge (quadratic, `unique_island_face_count`, `crates/plinth-support/src/specialized.rs:2858-2885`) falls from ~1.68 × 10¹⁰ to ~1.39 × 10¹⁰ — still 7× over the 2 × 10⁹ cap. The treant refuses in the same place with a smaller demand, exactly as predicted, but for the opposite reason: because the reduction is *small*, not because it is large.

**(c) The treant DOES have a sealed committed artifact, and a test that explicitly forbids decimating it.**

`corpus/baseline/Weeping Treant v1.2.prepped.oriented-solid.stl` is 21,456,084 bytes and is **tracked in git** (`git ls-files corpus/baseline`). `crates/plinth-orient/tests/orientation_rederivation.rs:30-40` lists the treant in `REDERIVABLE`, and at `:129-136` asserts:

```rust
assert_eq!(decimated.mesh().tris().len(), source_faces,
  "{name}: the corpus is entirely already-print-scale, so orientation must see the \
   unreduced source; a reduction here would invalidate the artifact comparison rather \
   than fail it");
```

then compares vertex-for-vertex against the artifact (`:96-105`, `:170-181`) at a ~3.9 × 10⁻⁶ mm f32-quantization bound. "Changing the treant's mesh is delta-free downstream today" is false: it breaks this gate.

**(d) Two *small* models would decimate the instant a witness is granted — the size threshold is proven necessary, not merely prudent.**

- `Sphere` (4,760 faces): two `GeometricallyEligible` final attempts, at **4,052** faces (`achieved_ratio` 0.85126) and 4,194; plus five `GeometricallyEligible` adaptive probes. Frozen census digest `17667468971446251870` would move.
- `make_toed_slab` (140 faces): one `GeometricallyEligible` final attempt at **134** faces (`achieved_ratio` 0.95714). Frozen digest `11199296506741562956` and the provisional seal tuple `64 / 179 / 4826202191472851835` would move.
- The other five (`Cylinder`, `make_box`, `make_overhang_table`, `make_raking_blade`, `make_thin_fin`) all refuse at `ProtectionCap` before the search — structurally immune.

Minor corrections:

- **Printer specs.** `docs/datasheets/printers/saturn-4-ultra-16k.md:31,57-58`: XY pitch is **14 × 19 µm** (project X = 14, project Y = 19), not 19–24. Layer 0.050 mm is a *project* record (`:34`), not a vendor default. `DecimationBudget::ratified_default` (`lib.rs:211-220`) uses the coarser 19 µm Y axis.
- **Surface builds.** The treant's run builds **21** `NativeSurface`es, not ~11: 1 (feature protection, `lib.rs:1964`) + 5 bisections × 2 + 5 adaptive probes × 2 (`lib.rs:1983-1984`). Ten of those twenty are redundant rebuilds of the *unchanged original* mesh.
- **`support-resource-caps-2026-08-05.md:15`** says "429,120 support faces after decimation and hollowing". The raw STL triangle count is 429,120 (`od -An -tu4 -j80 -N4` on the file). Decimation is a no-op and the census hollows via `HollowedModel::solid_decision` (`crates/plinth-support/examples/embed_incidence_census.rs:93-95`). Neither stage touched it.
- **README.md:120** — "It now decimates, orients…" is the same overstatement. It runs the search and returns the original.
- **Decimation never moves a vertex.** `compact_result` writes `mesh.verts()[original as usize]` (`lib.rs:2466`) — original f64 bits. `quantize_positions` (`lib.rs:2350`) f32-quantizes *only* the meshopt input; `max_input_quantization_delta` is diagnostic. So "decimation-moved vertices" does not exist as a phenomenon; decimation is pure vertex/face **removal**.

---

## 1. Machinery map

**Entry point and search.**
- `plinth_decimate::run` (`lib.rs:844-877`) → scale sanity → `select_feature_protection` → `run_budget_search`. A `FeatureProtectionError::CapExceeded` short-circuits to `already_print_scale` (this is what stops 5 of 8 corpus models).
- `select_feature_protection` (`lib.rs:905-926`): one ratio-0.20 `simplify_locked`, one `backward_error_map` (1 surface), then `select_feature_protection_from_errors` (`:928-1019`) — a seed threshold + radius halo + structural lock closure, with a 12 % cap and 3 bounded backoffs.
- `run_budget_search` (`lib.rs:1170-1431`): identity precondition step (`canonical_identity_step`, `:1572`), 5 bisections in [0.05, 1.00] (`:1221-1242`), candidates sorted by exact face ratio (`:1249-1255`), then per-candidate deterministic rebuild → fingerprint match → `build_face_topology` → **full-population** `probe_two_sided` → `classify_final_attempt` → `assess_downstream_utility`.
- `run_adaptive_diagnostics` (`lib.rs:1683-1833`): up to 5 more integer-face targets, diagnostics only (A-2 forbids them authorizing production).

**What `assess_downstream_utility` sees today.** `lib.rs:1453-1457` — signature is `fn assess_downstream_utility(_candidate: &SimplifyOutcome)`, argument ignored, returns `NoDownstreamUtilityEvidence` unconditionally. At the single call site (`lib.rs:1361`) the following are all in scope and *not* passed: `final_probe: TwoSidedProbe` (carries `p99_mm`, `p99_9_mm`, **`max_mm`**, `distance_digest`), `topology_before`/`topology_after`, `error_budget`, `original`, `policy`. Widening the signature is a local change.

**The sealed type.** `DownstreamUseful { witness_id: &'static str }` (`lib.rs:366-376`) — private field, no public constructor, `compile_fail,E0451` doctest at `:361-365`. `DownstreamUtilityEvidence` (`:380-384`) is the two-state enum. Minting one is a one-line `const fn` inside the crate.

**Is downstream wiring dead? No — it is live and complete.** `BudgetSearchOutcome::into_decimated_model` (`lib.rs:634-666`) maps `Verified` → `DecimationDisposition::Reduced` and moves the mesh into `DecimatedModel`; `plinth_orient::run` (`crates/plinth-orient/src/lib.rs:76-92`) does `let (mesh, decimation) = model.into_parts();` and transforms *that* mesh; the census (`embed_incidence_census.rs:77-95`) carries it straight into `HollowedModel::solid_decision` and `run_support`. A granted witness would genuinely reach every downstream stage. The only unwired thing is `SimplifyOutcome::retained_original_vertex_ids` (`lib.rs:48`), which `run_budget_search` discards — it is the map you would need to compare a decimated mesh against a full-resolution artifact.

**Quality/error metrics already computed.** `probe_two_sided` (`lib.rs:1976-2031`) does two whole-batch exact nearest-surface queries — candidate face centres → original surface (forward), original vertices → candidate surface (backward) — concatenates them, and reports NumPy-linear `p99`, `p99.9`, `max`, plus an order-preserving `distance_digest`. This is a two-sided sampled Hausdorff. The budget (`lib.rs:1190-1193`, `search_policy` at `:1927-1949`) is:

```
p99_mm   = pixel_um(19.0) × DECIMATION_P99_FACTOR (1/2000) = 0.0095 mm
p99_9_mm = pixel_um(19.0) × DECIMATION_P999_FACTOR(1/1000) = 0.0190 mm
final    = ×1.1 slack → 0.01045 / 0.02090 mm
```

Because `pixel_um` carries a **µm-valued** number and the product is consumed as **mm**, the `/1000` *is* the µm→mm conversion. So the two factors are exactly **0.5 pixel** and **1.0 pixel** of the coarser printer axis. That is a real, defensible printer-resolution bound that happens to be encoded as two `Placeholder` fractions.

**The gap.** `TwoSidedProbe.max_mm` is measured on every probe and **is never checked anywhere**. Nothing bounds the worst-case deviation.

---

## 2. Evidence semantics — what `DownstreamUseful` should require

`DownstreamUseful` must be minted only when **all five** hold. Three are already computed; two are new.

**U1 — genuine reduction.** `output_face_count < input_face_count`. Already available.

**U2 — bounded typical deviation, from AcceptSafe provenance.** `p99 ≤ DECIMATION_MAX_P99_DEVIATION_MM` and `p99.9 ≤ DECIMATION_MAX_P999_DEVIATION_MM`, where these are **new `Derived` constants** from the `Measured` vendor pitch:

```
DECIMATION_MAX_P999_DEVIATION_MM = PIXEL_PITCH_Y_UM / 1000            = 0.019  mm  (one pixel, coarser axis)
DECIMATION_MAX_P99_DEVIATION_MM  = DECIMATION_MAX_P999_DEVIATION_MM/2 = 0.0095 mm  (half a pixel)
```

Derivation rule to state on the constant: *the printer addresses the XY plane on a 14 × 19 µm lattice; a deviation at or below one lattice step on the coarser axis cannot move a printed surface by more than one addressable step, and half that is the quantization half-step the printer already introduces.* `from = [PIXEL_PITCH_Y_UM]`, wrapper `Derived`, provenance chain terminating in the vendor datasheet.

This is numerically identical to what the search enforces today. It must nonetheless be a *separate* declaration, because `DECIMATION_P99_FACTOR` / `DECIMATION_P999_FACTOR` are `Placeholder` (`03-physics-and-constants.md:786-793`) and "arithmetic from a Placeholder cannot improve its provenance". The witness must assert the **Derived** bound, and a test must pin that the Derived bound and the Placeholder-search bound agree — so if either moves, the disagreement is loud.

**U3 — bounded worst-case deviation.** `final_probe.max_mm ≤ DECIMATION_MAX_ABSOLUTE_DEVIATION_MM`. **This is the genuinely new physics.** p99.9 leaves 0.1 % of samples unbounded; on a 429 k-face treant that is ~1,700 unbounded points, any one of which could be a collapsed twig. Candidate sources, in order of preference:

1. `PRINT_FLOOR_CHORD_MM = 0.08 mm` (`Measured`, `owner_decision(print-floor-owner-decision-2026-07-24)`, `03-physics-and-constants.md:768`) — the registered smallest printable chord. Rule: *a deviation below the smallest chord the process can print cannot produce a printed difference.* `Derived` from one `Measured` input. Cleanest.
2. `LAYER_HEIGHT_DEFAULT_MM = 0.050 mm` — matches the project's own established "one process resolution" argument (`crates/plinth-support/src/contact.rs:150-155`) and the same 0.05 mm figure PR-1(a)/PR-8 already use.
3. Half the finest pitch, `min(14,19)/2000 = 0.007 mm` — most conservative, almost certainly unachievable at any useful ratio.

**Set the number on Phase-1 data, not on this page.** The trace already records `max_mm` for every probe (`lib.rs:2021`) but `corpus_budgets.rs` does not serialize it — one line to add. Measure `max_mm` across the ratio sweep first; if 0.08 mm forces the achievable ratio to ~1.0, decimation is a no-op again and we have learned that honestly rather than by shipping it.

**U4 — structural invariants downstream stages actually assume.** Not "watertight" as a slogan; the exact things that break:

- **Sliceability.** `stitch_layer` (`crates/plinth-support/src/perception.rs:1213-1270`) builds a quantized point/edge graph at `PLANE_SLICE_MERGE_TOLERANCE_MM` and raises `OpenSliceSeam` if **any** vertex has degree ≠ 2, or if a walk revisits a consumed edge. That is exactly a manifold-closed-surface requirement. `after.nonmanifold_edges ≤ before.nonmanifold_edges` and `after.boundary_edges ≤ before.boundary_edges` (`lib.rs:1843-1846`) is the right proxy and must **stay** in the witness, not be relaxed. For a watertight input (`before.boundary_edges == 0`) this already implies the output is watertight.
- **Strict audit.** `Mesh::audit_strict` (`crates/plinth-mesh/src/lib.rs:130-165`) — no repeated indices, no zero-area/collinear triangles. Already run inside `compact_result` (`lib.rs:2484-2488`) and inside every probe (`lib.rs:2038-2049`).
- **Exterior-ness (OD-9).** Decimation cannot *create* buried surface out of nothing, but it can merge an exterior patch into a near-buried one. The OD-9 exterior screen runs later, in the contact machinery, and will catch it. No new gate needed; state the reliance explicitly.

**U5 — the actual downstream-utility claim, stated so it is provable rather than treant-shaped.** A-1 (`06-conventions.md:390`) requires a *registered* work-or-bounded-resource saving. The honest, model-independent claim is:

> Reducing the support-mesh face count `F` strictly reduces the demand on two **registered** execution-authority resources whose demand is an exact function of `F` alone: the minima-kNN batch's `source_count` (= `F`; cap `SUPPORT_KNN_MAX_WORK_PER_FACE`, `03-physics-and-constants.md:824-829`) and the `81·F` per-triangle term of the parity-grid projection `81·F + 36·R + 4` (`rust/geom_kernel/src/parity.rs:60-82`).

The witness must **not** claim the `36·R` reference term falls — `R/F` is measured to rise as a mesh coarsens (526 refs/face on the 244-face Cylinder against 2.83 on the treant, `support-resource-caps-2026-08-05.md:69-73`), so total `R` falls sublinearly and could in principle be flat. Saying only what is exactly provable is what keeps the witness honest and keeps it from inheriting the n=1 treant coefficients (R4).

Witness id: something like `"support-face-scaled-resource-demand"`, registered in the M0 registry notes of the two constants it names, so the reference is bidirectional.

---

## 3. Size threshold and the golden story

**Placement — put the threshold in the witness, not in front of the search.** This matters more than the value.

If the gate sits at the top of `run`, every small mesh takes a new short-circuit and `BudgetSearchTrace` changes shape for `make_box` (12 faces), the octahedron unit fixtures, `locked_box`, etc. That breaks `exhausted_reduction_search_returns_audited_already_print_scale` (`lib.rs:3116`), `five_bisections_select_the_lowest_observed_passing_ratio_with_honest_locks` (`:3174`), `orientation_boundary_preserves_identity_without_registered_utility` (`:3193`), and the whole `corpus_budgets` report shape — and it deletes the search's own test coverage on small fixtures.

If instead `assess_downstream_utility` returns `NoDownstreamUtilityEvidence` whenever `input_face_count < threshold`:

- every trace below the threshold is **byte-identical to today**, including `final_attempts[].downstream_utility` and the `NoDownstreamUtilityEvidence { geometrically_eligible_candidate_count }` refusal;
- the returned mesh is the same `original.clone()` from `already_print_scale` (`lib.rs:1560-1568`);
- therefore all 7 reachable corpus digests are frozen **structurally**, not merely measured — and the OD-8 census check becomes confirmation of a proof rather than the proof itself;
- zero test churn on the small path.

The wall-clock cost of *not* skipping the search is nil: the 5 sub-5,000-face models cost milliseconds. The 18 s is entirely the treant, which is above any threshold.

**Value — `32,768`, with `65,536` as the conservative alternative.**

Constraints: strictly above 4,760 (Sphere, largest reachable) and at or below 429,120 (treant). Argument for 32,768: it is the smallest power of two strictly above 4× the largest reachable corpus model, and — decisively — it sits **below the 65,024-face UV sphere** that `crates/plinth-decimate/tests/thread_sweep.rs:263-305` builds. That fixture would start genuinely reducing (a 10 mm-radius sphere at a 0.0095 mm sagitta bound admits ~0.87 mm edges → roughly 4,000 faces, a ~16× reduction), which turns `protected_budget_search_is_bit_identical_at_1_4_and_16_threads` from a determinism test of the identity path into a determinism test of the **reduced** path at 1/4/16 threads. That is exactly the coverage this change needs and it costs no frozen digest — the sweep compares thread-count reports against each other, with no pinned constant.

If maximum conservatism is preferred, 65,536 keeps the sweep fixture untouched; then the reduced path has no cross-thread determinism coverage and one must be added deliberately.

**Provenance class — `Placeholder`, and say so plainly.** A face-count threshold is not derivable from the datasheet. The derivable alternative fails: gating on "the mesh is finer than the printer can resolve" (`surface_area / (pixel × layer)` = `area / 9.5e-4 mm²`) yields a resolvable-face count of ~10⁶ for a mini-sized model — the treant's 429,120 faces are **coarser** than the printer's resolution, so that rule would forbid decimating the treant too. The honest statement: *the justification for decimating a large mesh is not that its detail is invisible to the printer; it is that downstream cost is superlinear in face count and the geometric price of the reduction is below print resolution.* The threshold is a corpus-shaped policy gate.

Register it as `DECIMATION_UTILITY_MIN_INPUT_FACES`, wrapper **`Placeholder`**, class `scalar`, alongside the existing `DECIMATION_*` placeholders in `crates/plinth-core/src/constants/placeholders.rs:1071-1096`, read through `.diagnostic()` exactly as `search_policy` already does (`lib.rs:1932-1947`). Its note must carry the R4-shaped standing trigger: **n = 1 large specimen; a second corpus model above ~4,760 faces triggers re-derivation, never silent inheritance.**

**The OD-8 zero-delta census check** stays exactly as run for R1–R5 (`CURRENT-STATUS.md:830-863`). Expect: 3,399 lines, all 3,390 `CENSUS` body lines byte-identical, all seven `snapshot_digest` values unmoved, the treant's `T0_stage_error` row unchanged apart from deliberately-moved fields. Compare census-to-census only; never a census digest against a seal-test digest (`CURRENT-STATUS.md:685-706`).

---

## 4. Measurement plan

1. **Extend the existing report** (`corpus_budgets.rs`) to serialize `max_mm` for every sampled and full probe, per-phase wall-clock (feature protection; each bisection; each final verification; each adaptive probe; total), `SimplifyOutcome::meshopt_advisory_error`, and `retained_original_vertex_ids.len()`. All already computed in memory.
2. **Face count.** Before: 429,120. After: whatever the search selects. On today's evidence the geometric budget alone caps it at 403,640 (r = 0.9406, −5.9 %) — and today the topology gate takes even that away.
3. **Error metrics** p99 / p99.9 / **max** at the selected ratio against the Derived bounds (`final_p99_limit_mm` reported as 0.01045).
4. **Per-stage wall-clock** and surface-build count (21 today).
5. **Downstream demand** from the census `MODEL` row (`faces=`, `max_grid_bytes=`, `knn_max_work=` on the failure path). Predictions at reduction ratio `r`:

| quantity | today | scaling | at r = 0.94 | cap | verdict |
|---|---|---|---|---|---|
| support faces | 429,120 | `r·F` | 403,640 | — | — |
| minima-kNN work (`F × minima`) | 298,667,520 | ~`r²` | ~264 M | `max(1e8, 2048·r·F)` = 827 M | clears |
| parity grid bytes `81F+36R+4` | 78,482,308 | `81·r·F` + sublinear `R` | ~74 M | `max(6.71e7, 512·r·F)` = 207 M | clears |
| load-eligibility rows (18.6/face) | 7,988,078 | `r` | ~7.5 M | — | — |
| gate ops (selection sort, `≈F²`) | ~1.68 × 10¹⁰ | `r²` | ~1.48 × 10¹⁰ | 2 × 10⁹ | **still refuses, 7× over** |

6. **Corpus census before/after must be byte-identical** — as in §3.

**Also measure, because it decides the design:** the `TopologyIncreased` diagnosis. For the mildest treant candidate (425,934 faces, 3 non-manifold edges), dump the offending edges and their incident faces. Three edges out of ~640,000 is a tiny, tractable defect; whether it comes from (i) coincident sheets in the self-interpenetrating regions, (ii) duplicate faces produced by collapse, or (iii) the operational lock closure (`lib.rs:1077-1128`) pinching a fan, determines whether the fix is a `SimplifyOptions` flag, a targeted post-pass, or a search-space restriction.

---

## 5. Risks and interactions

**R-1 (blocking, highest) — the topology gate, not the witness, is what stops the treant.** Bisection cannot route around it: non-manifold edges appear at 0.74 % reduction and grow monotonically, so there is no ratio band with zero. Options, in preference order: (a) diagnose the 3 edges and fix at the source; (b) try `SimplifyOptions::Prune` / `LockBorder` (`meshopt-0.6.2 src/simplify.rs:5-28`) — cheap to test; (c) a targeted post-collapse repair. **Do not** relax `topology_not_increased`: `stitch_layer` genuinely requires degree-2 adjacency (`perception.rs:1236-1238`), so relaxing it re-opens the seam the Tier-1 fix closed.

**R-2 (must guard) — decimation could convert the treant's fast typed refusal into a multi-hour hang.** The gate charge is `≈ 1.68e10 · r²` against a 2 × 10⁹ cap, so it stops refusing at `r < √(2e9/1.68e10) ≈ 0.345`. Below that the run proceeds into `finalize_specialized_load_partition` and hits the **uncharged** `O(n²)` rescan in `perception.rs` (FU-8b(c), `~6 × 10¹³ · r²` scalar comparisons, single-threaded). No cap catches it. Today's achievable r ≈ 0.91–0.94 is far above 0.345 — latent, not immediate, and a numerical coincidence rather than a designed guard. **Add an explicit guard**: pin `the_treant_still_refuses_at_the_gate_cap` as a test, and if any future change pushes r below ~0.35, land FU-8b first. (Resolved by ordering: FU-8b now runs first.)

**R-3 — seam re-opening.** "Decimation-moved vertices" does not apply: no vertex moves (`lib.rs:2466`). Surviving vertices keep bit-exact f64 coordinates. The real risk is topological — larger triangles produce a different intersection-segment set, and `point_key` quantization at `PLANE_SLICE_MERGE_TOLERANCE_MM` (`perception.rs:1183-1188`) could fuse two now-nearby crossings into one degree-4 node. Coarsening generally *reduces* segment density; not a proof. The census run is the test.

**R-4 — f32/f64: benign, provably.** The **weld** is decided in f64 `ExactBits` (`prepare_welded_input`, `lib.rs:2308-2344`) *before* `quantize_positions` (`:2263`), so index identity is never decided in f32. The QEM metric runs on f32 in both the procedural-f64 and STL-f32 cases, and `f64::from(procedural as f32) == stl` holds for every corpus coordinate (`CURRENT-STATUS.md:696`) — both variants quantize to *identical* f32 input, take an identical collapse sequence, and retain an identical vertex subset. **Decimation cannot create a new class of f32/f64 divergence; it can only propagate the existing one.** The threshold keeps all f64-procedural-vs-f32-STL twins below it anyway.

**R-5 — orientation.** (i) `orientation_rederivation.rs:129-136` hard-asserts no reduction for the treant; see §6 Phase 3. (ii) `transform_mesh` recomputes translation from the transformed bounds, so removing an extremal vertex would shift the whole model by that vertex's protrusion. `select_feature_protection` locks high-error vertices, which extremal spikes tend to be, but this is not guaranteed. Worth an explicit before/after bounding-box comparison in the measurement.

**R-6 — batch-first violations in the search itself.** The 18 s is not irreducible:
- `probe_two_sided` (`lib.rs:1983`) rebuilds the **original** surface on every call — 10 redundant 429 k-face `NativeSurface` builds per treant run. Build once, pass in.
- `simplify_locked` re-runs `mesh.audit_strict()` and `WeldedView::build(ExactBits)` over 1,287,360 source vertices on **every** of its 13 calls (`lib.rs:2209`, `:2312`). Weld once.
- `OperationalLockTopology::build` allocates `incident_faces: Vec<Vec<usize>>` (`lib.rs:1052`) — ~1.29 M individual heap allocations on the treant. Should be CSR.
- `select_feature_protection_from_errors:967-975` computes `max_output_pairs = seeds × verts` and `max_output_bytes` from it. On the treant with 10 k seeds that is 1.3 × 10¹⁰ pairs / 2 × 10¹¹ bytes — the halo query is **effectively uncapped**. This is a live unbounded-allocation hazard against the 100 GB memory directive, not just a performance note.
- `select_points` (`lib.rs:2142`) materializes `(0..1_287_360)` then truncates to 120,000, per call.

**Recommendation on the 18 s search: keep it, fix the redundancy, do not replace it.** It is the only instrument that produces the error/topology evidence the witness needs, and it already anticipates a `Verified` outcome (`corpus_budgets.rs:596-640`, `independently_audit_verified`). The threshold means it is only paid on models where it matters.

**R-7 — FU-8b/R4/FU-23 interplay.** If R-1 is ever fixed well enough for a deep reduction, the treant may fall **under both cap floors** (at F = 43,000: parity ≈ 14 MB vs the 67 MB floor; kNN ≈ 9 M vs the 100 M floor). Then `SUPPORT_KNN_MAX_WORK_PER_FACE` and `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` — both `Derived`, both n = 1, both measured on the *undecimated* treant — become inert on the entire corpus, and their single specimen no longer exists in the form that produced them. That is precisely the FU-23/R4 re-derivation trigger. Any deep-reduction landing must re-measure those two coefficients on the decimated treant and say so.

**R-8 — `make_toed_slab` (FU-7).** Its freeze is already PROVISIONAL. A threshold at ≥ 32,768 keeps it at 140 faces, so this change does not disturb FU-7.

---

## 6. Workflow slicing (as designed; Phases renumbered when scheduled)

Feature branch off `develop` per git flow; never push. Five gates on every phase.

**Phase 1 — Measure, and diagnose the real blocker (no behaviour change).** Extend `corpus_budgets.rs` (report-schema bump `plinth/m3-decimation-corpus-budget-report/v4`): serialize `max_mm`, per-phase wall-clock, surface-build count. Treant-only diagnostic dumping the 3 non-manifold edges of the 425,934-face candidate with incident faces and welded-vertex provenance. Test `SimplifyOptions::Prune`/`LockBorder`. Produce the max-deviation-vs-ratio curve that sets `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM`. **Exit criterion:** whether the treant can be decimated at all, and at what max deviation. *If it cannot, stop and report.*

**Phase 2 — Register the constants and mint the witness (still no corpus delta).** M0 registration updating all four membership artifacts (golden 260 → 264, classes `[18, 6, 236, 0]` → `[18, 9, 237, 0]`), `constants.md` regenerated via producer. Widen `assess_downstream_utility` to `(&SimplifyOutcome, &TwoSidedProbe, EdgeTopologyCounts, EdgeTopologyCounts)`, implement U1–U5, private `DownstreamUseful::witness(...)` constructor (compile-fail doctest stays green). Witness unit tests (each of U1–U5 refuses in isolation); threshold-agreement test pinning Derived bounds == Placeholder search bounds. OD-8 census: zero delta, measured.

**Phase 3 — Make the treant's reduction reachable, and re-pin what it moves.** Land the Phase-1 topology remedy. Add an explicit threshold field to `DecimationBudget` (defaulted from the registry in `ratified_default`) so `orientation_rederivation.rs` can construct a budget whose threshold is above 429,120 and prove what it actually claims — the *pinned orientation record* rederives its committed artifact — without a decimation confound. Delta: the treant's mesh moves, and nothing else; prove with census; attribute every moved number.

**Phase 4 (optional, separable) — Batch-first repairs in the search.** Build the original surface once; weld once; CSR incidence; bound the halo query honestly. Byte-identical `corpus_budgets` report (apart from timings); `thread_sweep` still bit-identical at 1/4/16.

---

## 7. Open questions and recommended answers

| # | Question | Recommended answer |
|---|---|---|
| Q1 | Is granting the witness still wanted, given zero eligible treant candidates? | Yes, but only after Phase 1 — alone it is another measured no-op; its value is turning the blanket refusal into a real predicate. |
| Q2 | Which constant sources the absolute (max) deviation bound? | `PRINT_FLOOR_CHORD_MM` (0.08 mm, Measured, owner decision) first choice; `LAYER_HEIGHT_DEFAULT_MM` (0.050) second. Decide on the Phase-1 max-vs-ratio curve. |
| Q3 | Threshold 32,768 or 65,536? | **32,768** — gives the reduced path cross-thread determinism coverage via the 65,024-face `thread_sweep` fixture, at no digest cost. |
| Q4 | Is a `Placeholder` size threshold acceptable? | Yes — it decides only *whether to attempt*, never *whether the result is safe*; the safety bounds are `Derived`. That separation is the point. |
| Q5 | May `orientation_rederivation.rs`'s treant row use a decimation-disabling budget? | Yes — its subject is the orientation record, not decimation. **Do not regenerate the 21 MB artifact.** |
| Q6 | May an adaptive-diagnostic candidate win once a witness exists? | No — A-2 forbids it; nothing is lost (the final path already finds the better candidate). |
| Q7 | Must the `max_gate_operations` refusal survive decimation? | Yes, as an explicit pinned expectation — the alternative (R-2) is a multi-hour hang. If r ever falls below ~0.345, FU-8b first. |
| Q8 | Do the §0 doc corrections warrant their own commit? | Yes — dated addenda for `support-resource-caps-2026-08-05.md:15` and `README.md:120`, per the honest-addendum convention. |

---

## 8. Phase-1 diagnosis — measured 2026-08-07

> **Dated addendum, appended below the verbatim record.** §0–§7 above are left standing
> exactly as handed off; nothing in them is edited. This section reports what Phase 1 (§6)
> asked for and says plainly which of the recommendations above the measurements
> **supersede** (R-1 option (b), and the framing of Q2).

**Provenance.** Measured 2026-08-07 on `feature/decimation-followup` off `develop @ 7aa6b73`,
release build, macOS/arm64. Subject: `corpus/models/Weeping Treant v1.2.stl` — 429,120 faces,
1,287,360 source vertices, 214,562 `ExactBits`-welded vertices. Probe configuration is the
production one the corpus gate uses (`NativeSurfaceConfig { grid_pitch_mm: 1.0,
max_grid_bytes: 64 MiB }`, `nearest_upper_bound_mm: 10_000.0`); locks are the production
`select_feature_protection` output; every candidate is `simplify_locked` at the exact integer
face target the search itself would request (`ratio_for_integer_target`). **All probes below
are full-population** `probe_two_sided`, not the sampled probes §0(b) quotes — the two are not
comparable row for row, and the difference is population, not a discrepancy. The instrumentation
was scratch inside `crates/plinth-decimate` and was reverted before the gates; nothing in this
section changed engine behaviour, and the seven frozen census digests are byte-identical (§8.8).

### 8.1 The treant's feature protection is running in backoff

| attempt | seed threshold | halo radius | seeds | halo vertices | after lock closure | vs cap 154,483 |
|---|---|---|---|---|---|---|
| 0 | 0.050 mm | 0.40 mm | 36,732 | 684,486 | 705,306 | **refused** (4.6×) |
| 1 | 0.100 mm | 0.24 mm | 2,657 | 30,377 | **41,067** | accepted (26.6 %) |

`DECIMATION_PROTECTED_CAP_FRACTION` (0.12 of 1,287,360 = 154,483) binds on the treant, so the
ratified first attempt never runs: one backoff (`DECIMATION_ADAPTATION_POLICY` = 3 adaptations,
feature ×2.0, error ×0.6) discards **92.8 % of the seeds** and the run proceeds with 41,067
locked vertices — 3.19 % of source vertices. Any future argument that "stronger feature
protection would prevent the damage in §8.4" has to start here, not at the seed threshold.

### 8.2 The max-deviation curve

| target | output faces | reduction | p99 mm | p99.9 mm | **max mm** | non-manifold edges | bbox delta mm |
|---|---|---|---|---|---|---|---|
| 429,119 | 429,118 | 0.0005 % | 0.000000 | 0.000000 | 0.00000023 | 0 | 0 |
| 428,000 | 428,000 | 0.261 % | 0.000000 | 0.002531 | **0.0566357** | 1 | 0 |
| 425,935 | 425,934 | 0.742 % | 0.000000 | 0.007110 | 0.0566357 | 3 | 0 |
| 420,000 | 420,000 | 2.125 % | 0.002376 | 0.011737 | 0.0637372 | 5 | 0 |
| 410,010 | 410,010 | 4.453 % | 0.005636 | 0.015448 | 0.0876457 | 11 | 0 |
| 403,640 | 403,640 | 5.937 % | 0.006912 | 0.017087 | 0.0876457 | 16 | 0 |
| 390,898 | 390,896 | 8.907 % | 0.009026 | 0.019756 | 0.0881166 | 24 | 0 |
| 386,208 | 386,206 | 10.000 % | 0.009611 | 0.020697 | 0.0929040 | 28 | 0 |
| 343,296 | 343,288 | 20.002 % | 0.014267 | 0.027669 | 0.1217317 | 47 | 0 |
| 300,384 | 300,350 | 30.008 % | 0.017763 | 0.033570 | 0.1217317 | 70 | 0 |
| 257,472 | 257,452 | 40.005 % | 0.022193 | 0.041524 | 0.1247810 | 113 | 0.008503 |
| 214,560 | 214,492 | 50.016 % | 0.027863 | 0.050668 | 0.1292336 | 170 | 0.008503 |

Boundary edges are **0** at every row, as they are on the original: the damage is never an
opened boundary. §0(a)'s non-manifold column reproduces exactly where the targets coincide
(3 at 425,934; 11 at 410,010; 16 at 403,640; 24 at 390,898).

**R-5 is real but late.** The bounding box is bit-identical to the original's through
r = 0.70 and moves by 0.0085 mm only at r ≤ 0.60 — deeper than any reduction this model can
pass. Extremal-vertex protection is holding at every ratio that matters.

### 8.3 `max_mm` is a staircase, and it clears the candidate U3 bounds almost immediately

| faces removed | reduction | p99.9 mm | **max mm** | duplicate face groups | non-manifold edges |
|---|---|---|---|---|---|
| 2 | 0.0005 % | 0.000000 | 0.00000023 | 0 | 0 |
| 10 | 0.0023 % | 0.000000 | 0.0000313 | 0 | 0 |
| 20 | 0.0047 % | 0.000000 | 0.0000366 | 0 | 0 |
| 70 | 0.0163 % | 0.000000 | **0.0149534** | 0 | 0 |
| 120 | 0.0280 % | 0.000000 | 0.0154398 | 0 | 0 |
| 220 | 0.0513 % | 0.000000 | 0.0154398 | 0 | 0 |
| 320 | 0.0746 % | 0.000000 | 0.0154398 | 0 | 0 |
| 520 | 0.1212 % | 0.000073 | **0.0566357** | 0 | 0 |
| 720 | 0.1678 % | 0.000577 | 0.0566357 | **1** | 0 |
| 920 | 0.2144 % | 0.001467 | 0.0566357 | 1 | **1** |

Two things this settles.

**(a) The ratified p99/p99.9 budget is blind to the worst point.** At 520 faces removed the
worst deviation is 0.0566 mm while p99.9 is 0.000073 mm — a ratio of **776×** — and p99 is
still exactly 0. U3 is not a formality; it is the only clause that would see this.

**(b) Every candidate U3 constant caps the treant at effectively nothing.** Reading the last
passing and first failing measured row:

| candidate bound | source | admissible reduction | admissible faces removed |
|---|---|---|---|
| 0.007 mm | half the finest pitch (§2 option 3) | (0.0047 %, 0.0163 %] | (20, 70] |
| **0.050 mm** | `LAYER_HEIGHT_DEFAULT_MM` (§2 option 2) | (0.0746 %, 0.1212 %] | (320, 520] |
| **0.080 mm** | `PRINT_FLOOR_CHORD_MM` (§2 option 1) | (2.125 %, 4.453 %] | (9,120, 19,110] |

The `max_mm` staircase is not noise: each riser is a single collapse in a single thin twig, and
the same worst point holds flat across 520 → 3,186 faces removed.

### 8.4 The topology defect: what the offending edges actually are

**The original is clean.** Boundary edges 0, non-manifold edges 0, **duplicate face groups 0**.
Every defect below is created by the reduction.

| output faces | reduction | duplicate face groups | non-manifold edges (gate authority) |
|---|---|---|---|
| 429,118 | 0.0005 % | 0 | 0 |
| 428,000 | 0.261 % | 1 | 1 |
| 425,934 | 0.742 % | 5 | 3 |
| 403,640 | 5.937 % | 67 | 16 |
| 390,896 | 8.907 % | 109 | 24 |
| 214,492 | 50.016 % | 664 | 170 |

Anatomy of the offenders, dumped at r = 0.9109 (25 by a clean classification; see §8.6 for why
the gate says 24):

- **Every offender has exactly four incident faces.** Not five, not a fan — 25 of 25. Each
  defect is one extra pair of faces on one edge.
- **It is not the operational lock closure.** 24 of 25 have *neither* endpoint in the protected
  set. R-1's hypothesis (iii) is excluded.
- **It is not an f32/f64 artifact.** `max_input_quantization_delta` on the treant is exactly
  **0** (binary-STL coordinates are f32 and widen/narrow exactly), and the 214,562 welded
  vertices occupy **214,562 distinct f32 positions** — no two welded vertices collide under
  quantization, so no collapse is being made free by the f32 input. R-4's benign verdict holds,
  and is now measured rather than argued.
- **It is coincident sheets — hypothesis (i) — produced by the collapse, not pre-existing.**
  The maximum absolute cosine between incident-face normals around an offending edge ranges
  **0.919 to 1.000000**, i.e. the four faces are two near-parallel sheets (the mildest is 23°,
  the typical is under 2°, several are parallel to seven decimals). Smallest incident face area
  6.18 × 10⁻⁵ mm². In **18 of 25** the edge already existed in the original as an ordinary
  2-face manifold edge and the collapse dragged two further faces onto it; in the other **7**
  the edge did not exist in the original at all.
- **It is spatially concentrated in the thin twigs.** The 25 offenders fall into ten clusters,
  and **12 of 25** lie in one 2.9 mm column at x ∈ [−11.4, −9.4], y ≈ −6.3, z ∈ [88.8, 91.7] —
  the same locality as the `captured_treant_locked_fan` fixture already captured in
  `crates/plinth-decimate/src/lib.rs` and as the treant's known self-interpenetrating region.
  The *first* defect of all, the single edge at 428,000 faces, is inside that column at
  (−10.874, −6.350, 91.508), with its four faces parallel to 2.0°.

**Mechanism, stated once.** QEM collapse in a twig whose two walls are closer together than the
collapse's own error scale fuses the walls: the cheap collapse is the one that walks a vertex
across the gap. The product is a coincident-sheet pair — first as duplicate faces, then as
4-face bowtie edges. This is a property of the *model's* feature thickness against the *error
budget*, not of a meshopt flag, and it is why the two symptoms of §8.3 and §8.4 appear within
a few hundred collapses of each other in the same twig: they are the same event seen by two
different instruments.

### 8.5 `Prune` and `LockBorder` are dead ends — R-1 option (b) is closed

Three targets × four option sets, full-population probes:

| target | `ErrorAbsolute` | `+Prune` | `+LockBorder` | `+Prune+LockBorder` |
|---|---|---|---|---|
| 425,935 | 425,934 f / 3 nm / max 0.0566357 | identical | identical | identical |
| 403,640 | 403,640 f / 16 nm / max 0.0876457 | identical | identical | identical |
| 390,898 | 390,896 f / 24 nm / max 0.0881166 | identical | identical | identical |

"Identical" means every reported quantity — output faces, boundary edges, non-manifold edges,
p99, p99.9, max — matched the control to the printed precision. Both flags are inert here for
structural reasons, not by accident: **`LockBorder` has nothing to lock** (the treant is closed;
boundary edges are 0 before and after), and **`Prune` removes whole disconnected components**
while this damage is interior to one. §5 R-1 lists (b) as "cheap to test"; it was, and it fails.
Of R-1's three options only (a) diagnose-and-fix-at-source and (c) a targeted post-collapse
repair remain — and §8.7 argues neither is worth building for this model.

### 8.6 A defect found while reconciling the counter — reported, deliberately not fixed here

`plinth_mesh::topology::build_face_topology` reads `cut_flags` for its duplicate-face exclusion
**inside the same loop that writes it** (`crates/plinth-mesh/src/topology.rs:339-341` reads,
`:371-373` writes). A face that shares an edge with a genuine duplicate face therefore acquires
`TOPOLOGY_CUT_DUPLICATE` part-way through the loop, and any *later* edge key touching that face
is silently dropped from `nonmanifold_edges`. It is deterministic (`BTreeMap` key order) but
order-dependent, and it under-counts:

| candidate | frozen-snapshot classification | `build_face_topology` | excluded key |
|---|---|---|---|
| 390,896 faces | 25 | 24 | (144833, 144834) |
| 214,492 faces | 208 | 170 | 38 keys |

Separately and larger: **duplicate faces are excluded from `nonmanifold_edges` by design and are
counted nowhere else**, so `topology_not_increased` cannot see them at all — 664 duplicate-face
groups at r = 0.50 pass that gate unremarked. Both points make the gate's number a *floor* on
the damage, and the gap widens with reduction. Neither is fixed in Phase 1, because Phase 1 must
not move behaviour; both are logged as **FU-26**.

### 8.7 Exit criterion — Phase 1's question answered, and the Phase-3 go/no-go

§6's Phase-1 exit criterion is "whether the treant can be decimated at all, and at what max
deviation. *If it cannot, stop and report.*" **It cannot, and this is the report.**

1. **The topology gate stops it at ≈0.2 % reduction.** The first non-manifold edge appears
   between 720 and 920 faces removed (0.168 %–0.214 %); the first duplicate face between 520 and
   720 (0.121 %–0.168 %). There is no ratio band above that with zero — the count is monotone in
   faces removed across every row measured.
2. **A perfect topology remedy would not unlock a useful reduction.** U3 with
   `PRINT_FLOOR_CHORD_MM` (0.08 mm) caps the treant at (2.125 %, 4.453 %]; with
   `LAYER_HEIGHT_DEFAULT_MM` (0.050 mm) at ≈0.12 %. The p99/p99.9 budget independently caps at
   ~8.9 % — and note that at 390,896 faces the full-population p99.9 is **0.019756 mm**, over
   the un-slacked 0.019 limit, passing only on the ×1.1 final slack. The binding constraint in
   every case is a handful of thin twigs, and it binds at single-digit percent or less.
3. **A ≤2 % face reduction changes no downstream disposition.** The gate arithmetic in §0(b)
   and §5 R-2 has since been overtaken by FU-8d phases F1–F3: the treant's merge gate no longer
   refuses (9,273,857,317 against an unchanged 35,445,500,736) and its stopping point is now
   wall-clock, opened as FU-8e. Against FU-8e's recorded frames — a per-proposal kd-tree
   construction at 59–62 % that scales with *proposals*, and `assess_coverage` at 24–34 % that
   scales with samples — trimming 2 % of faces moves the total by well under one percent. The
   treant needs an order of magnitude; decimation offers, at the very most, two percent.
4. **R-2's hazard is confirmed dormant, and now provably so.** r never approaches the ≈0.345
   below which the uncharged rescan would open up; the achievable r is ≥ 0.978.

**Recommendation: do not run Phase 3 as scoped.** Its deliverable — making the treant's
reduction reachable — is worth at most ~2 % of faces on this model and cannot change any
downstream outcome, while its cost includes a topology remedy for a coincident-sheet fusion and
a `DecimationBudget` threshold field threaded into `orientation_rederivation.rs`. The evidence
that would *reverse* this: a remedy that holds `max_mm` under 0.08 mm at ≥ 30 % reduction on
this model. Nothing measured here suggests one exists — at 30 % the max is 0.1217 mm, 1.5× the
bound, and it is set by twig features the printer *can* resolve.

**Phase 2 remains worth doing**, for the reason Q1 gives — it turns a blanket refusal into a
real predicate — but it must be entered knowing the predicate will refuse *this* model on U3
even if the topology gate is satisfied. The witness's value is on the next large model, not on
the treant.

**Q2's framing is superseded.** The recommendation was to decide the absolute bound on the
Phase-1 curve. The curve does not discriminate: 0.08 mm and 0.050 mm both reduce the treant to
nothing (2.1 % and 0.12 %), so the treant is not evidence for either. Decide it on the physics
argument alone — on which `PRINT_FLOOR_CHORD_MM` (0.08 mm, `Measured`,
`owner_decision(print-floor-owner-decision-2026-07-24)`) remains the better-provenanced choice,
being the registered smallest printable chord rather than a Z-axis quantum applied to an XY
deviation.

### 8.8 OD-8

Phase 1 landed **no engine change** — the diagnosis instrumentation was scratch and was reverted
by editing before the gates, leaving `crates/plinth-decimate/src/lib.rs` byte-identical to
`develop @ 7aa6b73`; `git diff` against develop is docs-only. The census was nevertheless
re-run at the branch tip, and all **seven** frozen `snapshot_digest` values are byte-identical:

| Model | faces | `snapshot_digest` at the Phase-1 tip | vs frozen |
|---|---|---|---|
| `Sphere` | 4,760 | `17667468971446251870` | unmoved |
| `Cylinder` | 244 | `13310185350867878190` | unmoved |
| `make_box` | 12 | `9657051231007502658` | unmoved |
| `make_overhang_table` | 60 | `6686599816688114065` | unmoved |
| `make_toed_slab` | 140 | `11199296506741562956` | unmoved |
| `make_raking_blade` | 24 | `13278215294653054002` | unmoved |
| `make_thin_fin` | 24 | `7016743535301716631` | unmoved |

Run as seven single-model invocations (`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY=<model>` against the
`--features census` release build of `embed_incidence_census`) rather than one 8-model
transcript: since FU-8d phase F3 the treant no longer refuses at the merge gate and instead
EXCEEDS-TIMEBOX, so an 8-model run would spend a timebox to produce no digest. The treant
carries no frozen digest and is not part of the OD-8 set. `routed_contacts` and
`final_live_members` are unmoved on all seven as well.

---

## 9. Phase-2 landing — the witness is registered and minted, 2026-08-07

> **Second dated addendum.** §0–§7 remain the handed-off record and §8 remains the Phase-1
> diagnosis; neither is edited. This section records what Phase 2 (§6) landed, which of §7's
> open questions it answers, and the single behavioural delta it produced.

**Provenance.** Landed 2026-08-07 on `feature/decimation-followup` off `develop @ 7aa6b73`,
macOS/arm64. Five gates green; OD-8 census re-measured at the tip (§9.6).

### 9.1 What Phase 2 changed

`assess_downstream_utility` was a one-line unconditional refusal
(`fn assess_downstream_utility(_candidate: &SimplifyOutcome)`, argument ignored). It is now
the registered predicate §2 specified, with the signature §6 asked for plus the policy the
run already reads once:

```rust
fn assess_downstream_utility(
    policy: UtilityPolicy,
    candidate: &SimplifyOutcome,
    final_probe: &TwoSidedProbe,
    topology_before: EdgeTopologyCounts,
    topology_after: EdgeTopologyCounts,
) -> DownstreamUtilityEvidence
```

`UtilityPolicy` is read once per run inside `search_policy()`, not once per candidate — the
registry lookup is not repeated per attempt. `DownstreamUseful` gains a crate-private
`const fn witness(&'static str)`; the field stays private and the `compile_fail,E0451`
doctest is unchanged and still green. The registered witness id is
`support-face-scaled-resource-demand`, exported as the public constant
`SUPPORT_FACE_SCALED_RESOURCE_DEMAND`.

Clause order in the implementation is threshold → U1 → U2 → U3 → U4 → U5, with two
deliberate details:

- **Non-finite is a refusal, not a comparison.** `max_mm` is `Option<f64>`; an absent or
  non-finite worst point returns `NoDownstreamUtilityEvidence`. `NaN > limit` is `false`, so
  an unguarded `>` would have read a missing measurement as a pass. `p99`/`p99.9` are
  finite-checked for the same reason.
- **U2 grants no slack.** The search's final classification admits a candidate on the
  `DECIMATION_RECHECK_SLACK` ×1.1 recheck; the witness asserts the un-slacked `Derived`
  bounds. A candidate that passes final verification only on the slack is therefore
  geometrically eligible and *not* downstream-useful. That asymmetry is intended and is
  pinned by `u2_refuses_on_either_percentile_independently_and_grants_no_slack`.

**U5 is machine-checked, not prose.** `registered_face_scaled_demand_falls` computes the two
face-exact registered demands — the minima-kNN batch's `source_count` (`= F`) and the `81·F`
per-triangle term of `81·F + 36·R + 4` — in `u128` and requires both to fall strictly. It is
implied by U1; it is computed anyway so the registered claim is checked rather than asserted.
The `36·R` term is deliberately absent, exactly as §2 U5 requires.

### 9.2 The four constants, and which of §7's questions they answer

| ID | Value | Wrapper | `from` |
|---|---|---|---|
| `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM` | 0.08 mm | `Derived` | `[PRINT_FLOOR_CHORD_MM]` |
| `DECIMATION_MAX_P999_DEVIATION_MM` | 0.019 mm | `Derived` | `[PIXEL_PITCH_Y_UM]` |
| `DECIMATION_MAX_P99_DEVIATION_MM` | 0.0095 mm | `Derived` | `[DECIMATION_MAX_P999_DEVIATION_MM]` |
| `DECIMATION_UTILITY_MIN_INPUT_FACES` | 32,768 | `Placeholder` | — |

Registry 264 → **268**; class vector `[measured, derived, placeholder, pending]`
`[20, 8, 236, 0]` → **`[20, 11, 237, 0]`**. (§6 predicted 260 → 264 and
`[18, 6, 236, 0]` → `[18, 9, 237, 0]`; the base moved because the caps/FU-8d work registered
two `Measured` and two `Derived` constants between the design record and this branch. The
*delta* is exactly the +3 `Derived` / +1 `Placeholder` §6 specified.) `constants.md` was
regenerated by `cargo run --example render_constants_golden -p plinth-core --offline`, never
hand-edited, and the source ledger gained four additive entries.

- **Q2 — which constant sources the absolute bound — is answered `PRINT_FLOOR_CHORD_MM`.**
  §8.7 already ruled the Phase-1 curve non-discriminating, so the decision rests on
  provenance alone: the registered smallest printable chord (`Measured`,
  `owner_decision(print-floor-owner-decision-2026-07-24)`) beats a Z-axis layer quantum
  applied to an XY deviation. The registry note records the runner-up and why it lost.
- **Q3 — 32,768 or 65,536 — is answered 32,768**, and §3's argument for it is now *measured*
  rather than predicted: see §9.4.
- **Q4 — is a `Placeholder` threshold acceptable — is answered yes**, and the separation is
  visible in the code: the threshold decides only whether to attempt; every safety bound in
  the predicate is `Derived`.

**Bidirectional witness registration — completed 2026-08-07, and it was missing when §9 was
first written.** §2's closing sentence requires the witness id to be *registered in the M0
registry notes of the two constants it names*, so that the reference runs both ways. The
Phase-2 landing named the constants from the witness but not the witness from the constants:
the string `support-face-scaled-resource-demand` appeared in no registry artifact at all, so
`constants.md` gave a reader of `SUPPORT_KNN_MAX_WORK_PER_FACE` no way to discover that a
decimation witness now depends on it. Both `Derived` notes in
`crates/plinth-core/src/constants/evidence.rs` now carry a dated *Witness registration* clause
naming `support-face-scaled-resource-demand` and
`plinth_decimate::SUPPORT_FACE_SCALED_RESOURCE_DEMAND`, the parity note restating why the
`36·R` term is deliberately excluded; `constants.md` was regenerated by the producer, never
hand-edited. No value, coefficient, cap, wrapper, `from` list or class count moved — the
registry is 268 constants at `[20, 11, 237, 0]` before and after, and the only rendered delta
is the two note strings.

The two numerically-identical-by-construction bounds are pinned against the search's
`Placeholder` products bit-for-bit (`f64::to_bits`) by
`decimation_witness_bounds_agree_with_the_search_policy_bounds`:
`19.0 / 1000.0 == 19.0 × (1.0/1000.0)` and `0.019 / 2.0 == 19.0 × (1.0/2000.0)` both hold
exactly at the ratified pitch. If either side ever moves, that test fails loudly rather than
the two silently diverging.

### 9.3 The threshold's placement is what freezes the corpus

The threshold is the witness's **first** clause, not a short-circuit in front of the search
(§3). Every model below it therefore runs the identical search, records the identical
`final_attempts[].downstream_utility`, appends the identical
`NoDownstreamUtilityEvidence { geometrically_eligible_candidate_count }` refusal, and returns
the identical `original.clone()`. The seven frozen OD-8 digests are frozen **structurally**;
§9.6's census confirms a proof rather than establishing one.

Reachability at this tip, all eight corpus models:

| Model | faces | where it stops | witness consulted? |
|---|---|---|---|
| `make_box` | 12 | `ProtectionCap` | no |
| `make_raking_blade` | 24 | `ProtectionCap` | no |
| `make_thin_fin` | 24 | `ProtectionCap` | no |
| `make_overhang_table` | 60 | `ProtectionCap` | no |
| `Cylinder` | 244 | `ProtectionCap` | no |
| `make_toed_slab` | 140 | threshold clause (140 < 32,768) | yes, refuses |
| `Sphere` | 4,760 | threshold clause (4,760 < 32,768) | yes, refuses |
| `Weeping Treant v1.2` | 429,120 | `topology_not_increased`, before the witness | no |

The treant is the one model above the threshold and it never reaches the predicate: every
candidate the bisection produces fails the topology gate first (§0(a), reproduced in §8.2).
Phase 2 therefore moves the treant by exactly zero faces, exactly as §8.7 said it would, and
`orientation_rederivation.rs`'s hard assertion that the treant does not reduce is untouched
and still green.

### 9.4 The one behavioural delta, measured

`crates/plinth-decimate/tests/thread_sweep.rs`'s 65,024-face UV sphere (10 mm radius,
256 × 128) is the only fixture in the workspace above the threshold that also clears the
topology gate. It now takes the **reduced** path:

| quantity | value | bound | headroom |
|---|---|---|---|
| input faces | 65,024 | — | — |
| output faces | **18,694** (−71.25 %) | — | — |
| `chosen_ratio` | 0.2874938484251969 | — | — |
| p99 | 0.009403548762485782 mm | 0.0095 | 1.010× under |
| p99.9 | 0.012589248125387647 mm | 0.019 | 1.509× under |
| **max** | 0.018350215095669874 mm | 0.08 | 4.36× under |

`protected_budget_search_is_bit_identical_at_1_4_and_16_threads` compares the three
child-process reports byte for byte and passes, so it is now a determinism test of the
**reduced** path rather than of the identity path — the coverage §3 chose 32,768 to obtain,
now demonstrated rather than predicted. That report is not a pinned constant (it compares
thread counts against each other), so no frozen digest moved to buy it. `status=verified` in
it is the first `BudgetSearchOutcome::Verified` anywhere in the workspace.

It is also the first datum on how the three bounds rank on a mesh that can actually reduce,
and it inverts the treant's picture: on a smooth sphere **p99 binds first** (98.99 % of its
limit at the winning ratio) and `max` is the loosest of the three, where on the treant the
order is reversed because its worst point is a twig-wall fusion rather than a uniform
sagitta. One specimen each; neither is a distribution.

### 9.5 One test-scaffolding change, stated plainly

`registry_rendering_is_deterministic_complete_and_in_stable_order`
(`crates/plinth-core/tests/constants_evidence.rs`) scanned the rendered Markdown for the
first *substring* occurrence of each ID. That is not a row-order test: a `Derived` entry
renders its inputs in `from=[...]`, and any note may name a sibling, so the first occurrence
of `PRINT_FLOOR_CHORD_MM` is now inside `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM`'s row, which
sorts earlier. The scan now anchors on the row's own leading cell (`"\n| <id> | "`). The
test's intent — that the renderer emits rows in stable sorted order — is unchanged and the
assertion is strictly stronger; the previous form merely happened to be safe while no
constant referenced an alphabetically later sibling.

### 9.6 OD-8

All **seven** frozen `snapshot_digest` values are byte-identical at the Phase-2 tip, together
with `faces`, `routed_contacts` and `final_live_members`:

| Model | faces | `snapshot_digest` | vs frozen |
|---|---|---|---|
| `Sphere` | 4,760 | `17667468971446251870` | unmoved |
| `Cylinder` | 244 | `13310185350867878190` | unmoved |
| `make_box` | 12 | `9657051231007502658` | unmoved |
| `make_overhang_table` | 60 | `6686599816688114065` | unmoved |
| `make_toed_slab` | 140 | `11199296506741562956` | unmoved |
| `make_raking_blade` | 24 | `13278215294653054002` | unmoved |
| `make_thin_fin` | 24 | `7016743535301716631` | unmoved |

Run as seven single-model invocations (`PLINTH_CENSUS=1 PLINTH_CENSUS_ONLY=<model>` against
the `--features census` release build of `embed_incidence_census`), for the reason §8.8
gives. Workspace tests 970 → **979**: the nine added are the witness suite — the mint, the
threshold at and below its floor, the two reachable corpus models, U1–U5 each in isolation,
and the bit-for-bit bound agreement.

### 9.7 What Phase 2 does *not* change

**Phase 3 stays recommended against.** §8.7's go/no-go rests on the treant's max-deviation
staircase and its topology defect, and Phase 2 re-measured neither. The evidence that would
reverse it is unchanged: a topology remedy that holds `max_mm` under 0.08 mm at ≥ 30 %
reduction on that model. Phase 2's own result is consistent with §8.7's closing sentence —
the witness's value showed up on the *next* mesh (§9.4), not on the treant.

**FU-26 is untouched.** `build_face_topology` still under-counts `nonmanifold_edges` and
still counts duplicate faces nowhere. U4 consumes that counter, so the witness inherits the
defect: its topology clause is a floor on the damage, not a measurement of it. No corpus
digest depends on that today because no corpus model decimates, and the one fixture that now
does (§9.4) is a closed sphere with zero non-manifold edges before and after. It is recorded
here because U4's strength is bounded by FU-26 until FU-26 lands.

**`retained_original_vertex_ids` is still discarded** by `run_budget_search` (§1). Nothing in
Phase 2 needed it; anything that later compares a decimated mesh against a full-resolution
artifact still will.
