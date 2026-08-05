# Embed-incidence census — 2026-08-04

Empirical measurement only. No design decisions, no production behavior change.
Branch `census/embed-incidence-2026-08-04`, forked from `codex/embed-zones` at
`029ced3`. The instrumentation in this commit is throwaway and must never merge.

## ERRATUM (2026-08-05) — read this before any number below {#erratum-2026-08-05}

Everything from *What was measured and why* down to *Caveats the ruling should
carry* is **superseded**. It is retained verbatim as the historical record of
what the engine looked like on 2026-08-04; it must not be cited as current.
The replacement numbers are in
[Unified census — 2026-08-05](#unified-census-2026-08-05) at the bottom of this
file, taken at integration merge `81ec3d4` (corpus fixes + contact physics
together, the first time both have ever been measured in one run).

Four independent reasons the original body is invalid:

1. **The measured formula no longer exists.** `ef081a2`
   (`feat(support)!: delete embed_inset_mm and both cosine floors; seat model
   rungs`) deleted `fn embed_inset_mm` and *both* `.abs().max(0.5)` cosine
   floors, and a source-text guard now enforces their absence. Measurements 1
   and 3 and headline numbers (a) and (c) characterise a code path that has
   been removed from production. The floor was also found to have had the
   opposite of its believed effect — it made the mask *smaller* and the check
   *stricter* — so the original framing of the risk was wrong as well as stale.
2. **The Reachability table is superseded.** It reported 4 of 8 models reaching
   brace/CCO, 1 of them on a borrowed pose. All three of its blocking causes
   have since been fixed: the `Cylinder`/`make_box` baselines were repaired to
   carry explicit identity orientation (and `plinth-orient` no longer discards a
   ratified orientation behind a failed hollow stage), exactly-coincident
   demand-free regular seats are now coalesced so `DuplicatePhysicalSeat` no
   longer blocks `make_toed_slab`/`make_raking_blade`/`Cylinder`, and an
   unclaimable slice hole is now survivable rather than a fatal `OpenSliceSeam`.
   **7 of 8 models now complete a sealed support run on their own pinned
   orientation.**
3. **Every `make_box` measurement row in the original body is invalid, not
   merely stale.** Those rows were computed under a pose *borrowed from*
   `make_overhang_table` (a -30 degree rotation that is not `make_box`'s
   orientation) via `PLINTH_CENSUS_SUBSTITUTE_ORIENT=1`. They describe a
   geometry that the pipeline never produces for that model. They are not
   comparable to the 2026-08-05 `make_box` rows, which are native. Any
   `make_box` row below marked `(subst.)` should be struck, not reconciled.
4. **The substitution mechanism itself is gone.** `PLINTH_CENSUS_SUBSTITUTE_ORIENT`
   and its `SUBSTITUTE_ORIENT` donor table were deleted from the harness in
   `6fe750b` as dead code, since every corpus member now orients natively. The
   *Harness* run command below, which passes that variable, will no longer work
   as written; use the command in the 2026-08-05 section.

What *does* survive from the original: the three digests in *Instrumentation is
result-inert* (`Sphere` / `make_overhang_table` / `make_thin_fin`) still
reproduce byte-identically at `81ec3d4`, so the instrumentation remains
result-inert and the contact-physics package moved no committed geometry on the
models that were reachable before it. See the delta discussion in the
2026-08-05 section.

## What was measured and why

`embed_inset_mm(contact_depth, radius, standoff, cosine) = contact_depth + (radius + standoff) / cosine`
(`crates/plinth-support/src/brace.rs`) is called with
`cosine = dot(axis, normal).abs().max(0.5)` at two sites: the brace model-rung
construction and the brace exact member-clearance pass. Flooring the cosine *up*
to 0.5 **shrinks** the inset relative to the true grazing geometry, and
`seg_clear_one` returns `INFINITY` — i.e. the candidate is **admitted with no
clearance check at all** — when `inset0 + inset1` consumes the whole segment
(`rust/geom_kernel/src/queries.rs`, `if s_hi - s_lo <= 1e-9 { return INFINITY }`).

At `|cos| < 0.5` the floored model-rung inset is the constant
`0.3 + (0.5 + 0.6) / 0.5 = 2.50 mm`, against a legal rung length band of
`[1.0, 5.0] mm`. Whether that matters on real geometry is what this census
answers.

## Harness

`crates/plinth-support/examples/embed_incidence_census.rs` runs the exact public
preparation path per model:

```
load_binary_stl(WeldPolicy::None)
  -> plinth_decimate::run(DecimationBudget::ratified_default(ProbeConfig::new(1.0, 64 MiB, 10_000.0)))
  -> plinth_orient::run(PinnedBaseline { corpus/baseline/<model>.pipeline.json })
  -> HollowedModel::solid_decision(BelowThreshold)
  -> plinth_support::run_support(lift 5.0 mm, SupportPreparationLimits::production(),
                                 LayerPlan 0.05 mm from z0 = 0.0, delicacy = 0.0)
```

This is the same construction the frozen `plinth-verify` coverage gate and the
`plinth-support` runner integration tests use, generalized over the corpus.

Run:

```
cargo build --offline --example embed_incidence_census -p plinth-support --profile dev-opt
PLINTH_CENSUS=1 PLINTH_CENSUS_SUBSTITUTE_ORIENT=1 ./target/dev-opt/examples/embed_incidence_census
```

`PLINTH_CENSUS=1` gates every counter; without it the identical binary produces
an un-instrumented run. `PLINTH_CENSUS_SUBSTITUTE_ORIENT=1` is discussed under
*Reachability* below.

### Instrumentation is result-inert (verified)

1. The harness was written and run **before** any library instrumentation
   existed. Per-model `snapshot_digest`, `routed_contacts`, and
   `final_live_members` were recorded.
2. The census module and every recording call site were then added.
3. Re-running with `PLINTH_CENSUS` unset and with `PLINTH_CENSUS=1` reproduces
   the pre-instrumentation values **byte-identically for every reachable model**:

| model | routed_contacts | final_live_members | snapshot_digest |
| --- | ---: | ---: | ---: |
| Sphere | 7 | 15 | 17667468971446251870 |
| make_overhang_table | 73 | 261 | 6686599816688114065 |
| make_thin_fin | 17 | 41 | 7016743535301716631 |

   Identical in all three runs (pre-instrumentation, instrumented-off,
   instrumented-on).
4. `cargo test -p plinth-support -p plinth-verify` — 383 tests pass, including
   `public_coverage_report_is_frozen_without_evidence_loss` (byte-frozen JSON)
   and the 1/4/16-thread determinism gate. `cargo clippy -p plinth-support
   --all-targets` is clean.

The `seg_clear_one` INFINITY predicate is **recomputed in the census**, not
patched into the kernel: `l = norm(p1 - p0)`, `degenerate = l < 1e-9`,
`over_masked = (l - inset1) - inset0 <= 1e-9`. Same operand association, same
`dot(a,a).sqrt()` norm, therefore bit-identical to the kernel's branch.

## Reachability — which models reached which stage

> **SUPERSEDED (2026-08-05).** All three blocking causes below are fixed; 7 of 8
> models now complete natively. See [ERRATUM](#erratum-2026-08-05) and
> [Unified census — 2026-08-05](#unified-census-2026-08-05). Every `make_box`
> row in this document is invalid — it was computed under a borrowed -30 degree
> pose.

| model | decimate | orient | perception/specialized | initial+CCO | rewire | brace | verdict |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Sphere | ok | ok (own pinned) | ok | ok | ok | ok | **covered** |
| Cylinder | ok | **refused** | — | — | — | — | blocked |
| make_box | ok | refused (own) / ok (substituted) | ok | ok | ok | ok | **covered, substituted orientation** |
| make_overhang_table | ok | ok (own pinned) | ok | ok | ok | ok | **covered** |
| make_toed_slab | ok | ok (own pinned) | **failed** | — | — | — | blocked |
| make_raking_blade | ok | ok (own pinned) | **failed** | — | — | — | blocked |
| make_thin_fin | ok | ok (own pinned) | ok | ok | ok | ok | **covered** |
| Weeping Treant v1.2 | ok | ok (own pinned) | **failed** | — | — | — | blocked |

Exact blocking reasons, from the instrumented stage-error capture:

| model | stage | error |
| --- | --- | --- |
| Cylinder | orientation | `PinnedBaselineInvalid` — `corpus/baseline/Cylinder.pipeline.json` has `stages[hollow].status == "failed"` and no `report.orientation` record, which `plinth_orient::selection_from_report` requires. |
| make_box | orientation | same: `make_box.pipeline.json` hollow stage failed, no orientation record. |
| Cylinder (after substituted orientation) | preparation / specialized | `Specialized(DuplicatePhysicalSeat { left: 0, right: 1 })` |
| make_toed_slab | preparation / specialized | `Specialized(DuplicatePhysicalSeat { left: 49, right: 50 })` |
| make_raking_blade | preparation / specialized | `Specialized(DuplicatePhysicalSeat { left: 20, right: 21 })` |
| Weeping Treant v1.2 | preparation / perception | `Perception(OpenSliceSeam { layer: 419 })` |

Both preparation failures were re-tested at layer pitches 0.03, 0.04, 0.05, and
0.10 mm. They persist at every pitch (only the reported layer/seat indices
move), so they are engine limitations at the specialized-contact and slicing
stages, not artifacts of the census's layer plan. They are upstream of brace and
CCO and cannot be reached by any existing harness.

`PLINTH_CENSUS_SUBSTITUTE_ORIENT=1` lets Cylinder and make_box borrow another
corpus member's ratified axis/degrees (Cylinder ← Sphere, make_box ←
make_overhang_table) purely so their geometry reaches the support engine. Only
make_box gets further with it; every make_box row below is flagged
`orientation=substituted` and should be read as supplementary, not as a
production orientation.

**Net: 4 of 8 models reach brace/CCO** (3 on their own pinned orientation, 1
substituted). Of those 4, only `make_overhang_table` generates any brace
model-rung candidates or any CCO model-anchor descriptors at all.

## Measurement 1 — brace model-rung candidate incidence

Rows reaching the incidence computation, histogram of raw `|cos(axis, normal)|`
(the value **before** the `.max(0.5)` floor):

| model | total | <0.3 | 0.3–0.5 | 0.5–0.7 | 0.7–0.9 | >=0.9 | \|cos\|<0.5 | of those, L<=2.50 mm | of those, L>2.50 mm |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Sphere | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_box (subst.) | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| make_overhang_table | 8 | 6 | 1 | 0 | 1 | 0 | 7 | 0 | 7 |
| make_thin_fin | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| **aggregate** | **8** | **6** | **1** | **0** | **1** | **0** | **7** | **0** | **7** |

Every model-rung row in the corpus, verbatim (`make_overhang_table`; all other
models produced none):

| i | raw \|cos\| | cos used | floored | L (mm) | radius (mm) | inset1 (mm) | usable (mm) | masked fraction |
| ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 0.431225 | 0.500000 | yes | 4.630548 | 0.5 | 2.500000 | 2.130548 | 54.0% |
| 1 | 0.108199 | 0.500000 | yes | 4.803552 | 0.5 | 2.500000 | 2.303552 | 52.0% |
| 2 | 0.003789 | 0.500000 | yes | 3.090579 | 0.5 | 2.500000 | 0.590579 | **80.9%** |
| 3 | 0.127310 | 0.500000 | yes | 4.607781 | 0.5 | 2.500000 | 2.107781 | 54.3% |
| 4 | 0.108199 | 0.500000 | yes | 4.803552 | 0.5 | 2.500000 | 2.303552 | 52.0% |
| 5 | 0.862506 | 0.862506 | no | 4.099993 | 0.5 | 1.575353 | 2.524640 | 38.4% |
| 6 | 0.257358 | 0.500000 | yes | 3.884180 | 0.5 | 2.500000 | 1.384180 | 64.4% |
| 7 | 0.089100 | 0.500000 | yes | 4.739332 | 0.5 | 2.500000 | 2.239332 | 52.7% |

Row 2 is effectively tangential (`|cos| = 0.0038`, ~89.8° off the surface
normal). The true unfloored inset there would be `0.3 + 1.1/0.003789 = 290 mm`
against a 3.09 mm rung — the floor replaces that with 2.50 mm, leaving 0.59 mm
of the rung actually clearance-checked.

Disposition of those 8 candidates:

| model | rows | floored-cos rows | admitted (viable) | floored admitted | over-masked | clearance == INFINITY |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| make_overhang_table | 8 | 7 | 3 | 2 | 0 | 0 |

`brace_append_model_rung` never fired on any model in the corpus: the 3 viable
rungs lost the per-need/per-site winner selection to cross-link candidates, and
**no model rung reached the final live forest on any corpus model** (see
Measurement 5). Consequently the member-level embed incidence table (the second
`.max(0.5)` site, `brace.rs` exact member clearance) is **empty for every
model** — that site skips `MemberKind::Stub`, and no non-stub live member on any
corpus model carries a `model_contact_end`.

## Measurement 2 — CCO model-anchor strut incidence at the model seat

`cco::apply_model_anchor` builds a strut whose rootward end is the model seat and
applies no incidence screen. Incidence is `|dot(seat - stub_end, surface normal
at seat)|`, resolved for the whole model in one batched `NativeSurface::project`.

Attempted population (every strut `apply_model_anchor` constructed, candidate
evaluation included):

| model | total | <0.3 | 0.3–0.5 | 0.5–0.7 | 0.7–0.9 | >=0.9 | \|cos\|<0.5 | share grazing |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Sphere | 0 | 0 | 0 | 0 | 0 | 0 | 0 | — |
| make_box (subst.) | 0 | 0 | 0 | 0 | 0 | 0 | 0 | — |
| make_overhang_table | 1741 | 389 | 400 | 434 | 377 | 141 | 789 | 45.3% |
| make_thin_fin | 0 | 0 | 0 | 0 | 0 | 0 | 0 | — |
| **aggregate** | **1741** | **389** | **400** | **434** | **377** | **141** | **789** | **45.3%** |

Surviving population (`MemberKind::Strut` with
`model_contact_end == Rootward` in the sealed verification snapshot):

| model | surviving model-anchor struts |
| --- | ---: |
| Sphere | 0 |
| make_box (subst.) | 0 |
| make_overhang_table | 0 |
| make_thin_fin | 0 |
| **aggregate** | **0** |

Reason, from the CCO winner census: the `ModelAnchor` descriptor **never wins the
CCO tournament** on any corpus model.

| model | winner = plate | winner = bifurcate | winner = model anchor |
| --- | ---: | ---: | ---: |
| Sphere | 6 | 1 | 0 |
| make_box (subst.) | 5 | 3 | 0 |
| make_overhang_table | 30 | 43 | 0 |
| make_thin_fin | 10 | 7 | 0 |
| **aggregate** | **51** | **54** | **0** |

So the missing incidence screen on CCO model anchors has **zero production
exposure on this corpus today**, while the raw incidence distribution of the
candidates it would have screened is 45.3% grazing.

Note also that CCO's own clearance rows use a flat inset —
`contact_depth_mm` terminalward / `standoff_mm` rootward, `cco.rs
clearance_rows` — with no cosine term at all, so the floored-cosine formula is
not reached on the CCO path.

## Measurement 3 — over-mask admissions (`seg_clear_one` INFINITY path)

Every row handed to `NativeSurface::segment_clearance` from `plinth-support`,
across all four call sites, with the kernel's INFINITY predicate recomputed:

| model | site | rows | rows with embed inset | cos-derived rows | floored-cos rows | **over-masked** | over-masked floored | over-masked \|cos\|>=0.5 | over-masked (no cos inset) | degenerate length |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Sphere | brace_exact_member_clearance | 16 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| Sphere | cco_candidate_clearance | 966 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| Sphere | rewire_clearance | 263 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_box (subst.) | brace_exact_member_clearance | 22 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_box (subst.) | cco_candidate_clearance | 1032 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_box (subst.) | rewire_clearance | 251 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_overhang_table | brace_candidate_clearance | 427 | 8 | 8 | 7 | **0** | 0 | 0 | 0 | 0 |
| make_overhang_table | brace_exact_member_clearance | 3132 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_overhang_table | cco_candidate_clearance | 16224 | 1498 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_overhang_table | rewire_clearance | 936 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_thin_fin | brace_exact_member_clearance | 48 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_thin_fin | cco_candidate_clearance | 1977 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| make_thin_fin | rewire_clearance | 421 | 0 | 0 | 0 | **0** | 0 | 0 | 0 | 0 |
| **aggregate** | all sites | **25715** | **1506** | **8** | **7** | **0** | **0** | **0** | **0** | **0** |

**Zero over-mask admissions corpus-wide.** No row on any reachable model takes
the `seg_clear_one` INFINITY path today, floored-cos or otherwise.

How close the embed-inset rows come to that cliff:

| model | site | embed-inset rows | min usable (mm) | median usable (mm) | max usable (mm) | min usable fraction | median usable fraction |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| make_overhang_table | brace_candidate_clearance | 8 | 0.590579 | 2.239332 | 2.524640 | 0.191090 | 0.472499 |
| make_overhang_table | cco_candidate_clearance | 1498 | 0.082556 | 11.212372 | 19.331578 | 0.120952 | 0.949206 |

The tightest cosine-derived row keeps 0.59 mm (19.1% of its length) inside the
clearance check. The tightest row overall is a CCO row at 0.083 mm — but that
inset is the flat `standoff_mm`, not a cosine-derived one.

## Measurement 4 — embedded-stub tip radii and local surface curvature

`loads.tip_radii_mm()[slot]` as actually consumed at `initial.rs` contact-stub
seeding:

| model | contacts | min | median | max | mean | distinct values in use |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| Sphere | 7 | 0.507172 | 0.613813 | 0.674343 | 0.594960 | 0.507172, 0.538150, 0.564515, 0.613813, 0.623558, 0.643170, 0.674343 (1 each) |
| make_box (subst.) | 8 | 0.200000 | 0.200000 | 1.000000 | 0.400000 | 0.200000 ×6, 1.000000 ×2 |
| make_overhang_table | 84 | 0.200000 | 0.200000 | 1.000000 | 0.269555 | 0.200000 ×74, 0.460659 ×4, 1.000000 ×6 |
| make_thin_fin | 18 | 0.200000 | 0.200000 | 1.000000 | 0.288889 | 0.200000 ×16, 1.000000 ×2 |
| **aggregate** | **117** | **0.200000** | **0.200000** | **1.000000** | — | 0.2 (96), 0.460659 (4), 0.507172, 0.538150, 0.564515, 0.613813, 0.623558, 0.643170, 0.674343 (1 each), 1.0 (10) |

Local curvature probe at each contact: four tangent-plane samples at the tip
radius, projected onto the surface in one batched call per model, classified by
the sign of their height along the contact normal (tolerance 2% of the probe
step).

| model | contacts | flat | convex | concave | saddle |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sphere | 7 | 0 | 7 | 0 | 0 |
| make_box (subst.) | 8 | 8 | 0 | 0 | 0 |
| make_overhang_table | 84 | 84 | 0 | 0 | 0 |
| make_thin_fin | 18 | 18 | 0 | 0 | 0 |
| **aggregate** | **117** | **110** | **7** | **0** | **0** |

The probe reproduces ground truth on the only curved reachable model (Sphere:
7/7 convex) and on the planar ones (110/110 flat), so it is behaving. But the
reachable corpus is almost entirely planar; it carries no information about
concave seats.

## Measurement 5 — final live member composition (context)

| model | Stub/terminalward | Shoot/none | Trunk/none | Brace/none | Strut/rootward | Other("model_rung")/rootward |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sphere | 7 | 1 | 7 | 0 | 0 | 0 |
| make_box (subst.) | 8 | 3 | 8 | 0 | 0 | 0 |
| make_overhang_table | 73 | 61 | 89 | 38 | 0 | 0 |
| make_thin_fin | 17 | 7 | 17 | 0 | 0 | 0 |

The only members that carry a model contact end in any final forest on this
corpus are the initial contact stubs, and those are excluded from the brace
exact-clearance embed site by its `MemberKind::Stub` filter.

## Headline numbers

- **(a) Brace model-rung rows with raw `|cos| < 0.5`, corpus-wide: 7 of 8
  (87.5%).** All 7 have rung length L > 2.50 mm, so none falls in the
  unconditional-admit band; 0 rows have L <= 2.50 mm. Their floored insets mask
  52.0%–80.9% of the rung. 3 of 8 rung candidates were revalidated as viable (2
  of them floored), but 0 were ever appended to the forest on any model.
- **(b) CCO model-anchor struts with `|cos| < 0.5`: 789 of 1741 attempted
  (45.3%), all on `make_overhang_table`. 0 surviving struts corpus-wide** —
  `ModelAnchor` never wins the CCO tournament on any corpus model (0 wins vs 51
  plate + 54 bifurcate).
- **(c) Total `seg_clear_one` INFINITY over-mask admissions corpus-wide: 0**, out
  of 25,715 clearance rows across all four `segment_clearance` call sites. 0
  floored-cos, 0 `|cos| >= 0.5`, 0 degenerate-length. Tightest cosine-derived row
  retains 0.590579 mm (19.1%) of usable segment.
- **(d) Tip radii in actual use at contact stubs: range [0.200000, 1.000000] mm**
  over 117 contacts; 96/117 are exactly 0.2 mm, 10/117 are exactly 1.0 mm, and
  the remaining 11 are load-derived intermediates in [0.460659, 0.674343].

## Caveats the ruling should carry

1. Only 4 of 8 corpus models reach brace/CCO, and only 1 of those 4
   (`make_overhang_table`, 60 faces) produces any model rungs or model-anchor
   candidates at all. The entire cosine-floor evidence base is 8 rows from one
   procedural test fixture.
2. The most representative model — `Weeping Treant v1.2` — is blocked at
   perception (`OpenSliceSeam`) and contributes nothing.
3. The reachable corpus is almost entirely planar (110/117 flat contacts). It
   cannot speak to concave or high-curvature seats.
4. Everything above measures the engine as it stands today; the fact that model
   rungs and CCO model anchors never survive means the floored-cosine formula
   currently has no committed geometry downstream of it on this corpus.

---

## Unified census — 2026-08-05 {#unified-census-2026-08-05}

First run of the census with the corpus fixes and the contact-physics package
**both** in the tree: integration merge `81ec3d4`
(`merge: contact-physics package complete`), harness at `6fe750b`. Every prior
census measured one half or the other.

```
PLINTH_CENSUS=1 cargo run --release --offline --features census \
  --example embed_incidence_census -p plinth-support
```

No `PLINTH_CENSUS_SUBSTITUTE_ORIENT` — the mechanism is deleted. Every model
below oriented from its own repaired baseline (`orientation=own_pinned` on every
`MODEL` row).

### Reachability — 7 of 8 native

| model | faces | verdict | routed_contacts | final_live_members | snapshot_digest |
| --- | ---: | --- | ---: | ---: | ---: |
| Sphere | 4760 | **OK** | 7 | 15 | 17667468971446251870 |
| Cylinder | 244 | **OK** | 54 | 119 | 13310185350867878190 |
| make_box | 12 | **OK** | 11 | 25 | 9657051231007502658 |
| make_overhang_table | 60 | **OK** | 73 | 261 | 6686599816688114065 |
| make_toed_slab | 140 | **OK** | 64 | 179 | 11199296506741562956 |
| make_raking_blade | 24 | **OK** | 32 | 89 | 13278215294653054002 |
| make_thin_fin | 24 | **OK** | 17 | 41 | 7016743535301716631 |
| Weeping Treant v1.2 | — | UNREACHABLE | — | — | — |

This is exactly the reachability the corpus-fixes section of `CURRENT-STATUS.md`
predicts. **No model that was expected to pass failed.**

The single stop, verbatim from the stage-error capture:

```
Preparation: Perception(PointIndex("kNN work requires 298667520; limit is 100000000"))
```

That is the `KnnBatchLimits.max_work` cap (FU-8), reproduced to the digit. It is
a **resource cap, not a geometry defect** — the treant's seam is fixed and it now
clears perception's slicing; production caps simply do not admit a real mini.

**Digest continuity — five of seven, and two real movements.** `Sphere`,
`make_overhang_table` and `make_thin_fin` reproduce the digests recorded
pre-contact-physics in *Instrumentation is result-inert*, byte for byte.
`Cylinder` (`13310185350867878190`) and `make_box` (`9657051231007502658`) match
the digests recorded under FU-15.

**`make_toed_slab` and `make_raking_blade` moved.** The
`support_runner_integration` goldens, last pinned at `414e13f` (corpus-fixes
era, i.e. before contact physics), disagree with this run:

| model | pinned (pre-contact-physics) | now | delta |
| --- | --- | --- | --- |
| make_toed_slab | 65 routed, 184 live, digest `15554091706332339621` | 64 routed, 179 live, digest `4826202191472851835` | -1 routed, -5 live |
| make_raking_blade | 35 routed, 93 live, digest `6761406004527881770` | 32 routed, 89 live, digest `1709218010932846012` | -3 routed, -4 live |

These are exactly the two models that carry PR-9 contact refusals in the T7
table below (`make_toed_slab` 6, `make_raking_blade` 5) and the only two with
`min_rim = 0/8`. The movement is the intended OD-8 consequence — contacts that
cannot prove an interface are refused, and rewiring redistributes what is left —
not an unexplained drift. Both models were newly reachable from the corpus
fixes, so their goldens were pinned in the window between the two packages and
have never described the contact-physics engine.

`cargo test --workspace --offline` is therefore **RED** at this head: 939 passed,
1 failed, the failure being
`public_runner_seals_toed_slab_and_raking_blade_after_seat_coalescing`. Re-pinning
those two goldens is a known, deliberately deferred item — it is a digest-moving
change and needed this census as its evidence. Do not re-pin them without
recording the delta above.

The narrower claim that does hold: the contact-physics package moved **no
geometry on any model that was already reachable before it**.

### T7 — embedded-stub terminal attestation

Per-terminal contact attestation at the initial contact stubs. `min_rim` is the
worst rim engagement observed on that model, out of 8 sampled rim points.

| model | total | fully_engaged | centreline_outside | min_rim | refused | area_deficit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Sphere | 7 | 7 | 0 | 8/8 | 0 | 0 |
| Cylinder | 54 | 34 | 5 | 3/8 | 0 | 6 |
| make_box | 11 | 5 | 0 | 3/8 | 0 | 0 |
| make_overhang_table | 84 | 50 | 8 | 3/8 | 0 | 4 |
| make_toed_slab | 72 | 54 | 2 | 0/8 | 6 | 1 |
| make_raking_blade | 40 | 24 | 6 | 0/8 | 5 | 1 |
| make_thin_fin | 18 | 11 | 0 | 3/8 | 0 | 0 |
| **aggregate** | **286** | **185** | **21** | **0/8** | **11** | **12** |

11 stub terminals are **refused** outright (`make_toed_slab` 6,
`make_raking_blade` 5) — PR-9 refusals, i.e. ordinary candidate rejections, not
pipeline failures. Those same two models are the only ones with `min_rim = 0/8`.

### T8 — seat attestation, brace model rungs and CCO model anchors

| model | family | total | fully_engaged | centreline_outside | min_rim | refused | area_deficit |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Cylinder | cco_model_anchor | 1681 | 0 | 1681 | 3/8 | 0 | 533 |
| make_box | cco_model_anchor | 7 | 0 | 7 | 2/8 | 0 | 2 |
| make_overhang_table | brace_model_rung | 2 | 2 | 0 | 8/8 | 0 | 0 |
| make_overhang_table | cco_model_anchor | 1498 | 0 | 1498 | 1/8 | 0 | 19 |
| **aggregate** | — | **3188** | **2** | **3186** | **1/8** | **0** | **554** |

Attestation is **live for all three contact families** (stub terminal, brace
model rung, CCO model anchor), and these row counts prove it executes on real
corpus geometry rather than only on fixtures.

The striking column is `centreline_outside`: **every one of the 3186 CCO
model-anchor seats has its centreline off the surface patch.** Combined with the
tournament result below (0 of 3429 anchors ever wins), this is the empirical
shape of the PR-7 deferral — CCO model-anchor seating is measurably wrong, and
the only reason it is not a live defect is that no such anchor is ever
committed. The commit-time tripwire (`6a35d45`) refuses an unattested CCO model
anchor, so on the day one does win it cannot land silently.

### Area deficits — the print-quality data, RECORDED not adjudicated

**12 stub + 554 CCO model-anchor** contacts carry a contact-area deficit (seated
area below the ratio the load wants). These are recorded as data for the
print-quality follow-up; nothing in this package acts on them.

| family | area deficits | of total |
| --- | ---: | ---: |
| stub terminal (T7) | 12 | of 286 |
| CCO model anchor (T8) | 554 | of 3186 |
| brace model rung (T8) | 0 | of 2 |

> **Correction to an in-flight figure.** The number quoted while this package was
> being assembled — *"4 stub + 19 CCO area deficits"* — is `make_overhang_table`
> **alone**, which was the only model reaching CCO when it was taken. The unified
> corpus figures are `12` and `554`. The growth is entirely reachability:
> `Cylinder` (533 CCO deficits of 1681 seats) and `make_box` (2 of 7) had never
> reached the CCO stage natively before this merge. No rule changed; the
> denominator did.

### Tournament counters — CCO model anchors still never win

| model | anchors attempted | surviving | cco_winner_plate | cco_winner_bifurcate | brace_append_crosslink |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sphere | 0 | 0 | 6 | 1 | 0 |
| Cylinder | 1681 | **0** | 48 | 6 | 0 |
| make_box | 7 | **0** | 8 | 3 | 0 |
| make_overhang_table | 1741 | **0** | 30 | 43 | 21 |
| make_toed_slab | 0 | 0 | 23 | 41 | 1 |
| make_raking_blade | 0 | 0 | 12 | 20 | 0 |
| make_thin_fin | 0 | 0 | 10 | 7 | 0 |
| **aggregate** | **3429** | **0** | **137** | **121** | **22** |

**0 of 3429 attempted model anchors survives, on any model.** The original
census saw this on one model with 1741 attempts; it now holds across three
models and roughly twice the attempts, so it is a property of the tournament,
not of `make_overhang_table`. Every committed CCO winner corpus-wide is a plate
or a bifurcate. This is the tournament-bias question the model-anchor bundle has
to answer: organic minis (the treant, the PC mini) are precisely the geometry
that *should* anchor to the model rather than run struts to the plate.

### T1 — brace model-rung candidates

Only `make_overhang_table` generates model rungs at all, and it now generates
**2** candidate rows where the 2026-08-04 census recorded 8. The populations are
not comparable: the rung site was rewritten by `ef081a2` (rungs are now *seated*
— PR-1 — rather than kissing the projected surface point) and by `69ffc43`
(printability evaluated at the UNSEATED projection point, BLOCKER-3), so what
counts as a candidate changed.

| i | raw \|cos\| | cos used | raw grazing | L (mm) | radius (mm) | inset1 (mm) | usable (mm) |
| ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 0 | 0.431225 | 0.513166 | yes | 4.867688 | 0.500000 | 3.217523 | 1.650165 |
| 1 | 0.862506 | 0.879755 | no | 4.364047 | 0.500000 | 1.651622 | 2.712425 |

Both are admitted viable, both have `L > 2.50 mm`, neither is over-masked. Note
that `cos_used > |cos|_raw` on both rows *without* being pinned to `0.5`: the
effective cosine is now derived from the seat, which is what replaced the floor.

### T3 — over-mask admissions (`seg_clear_one` INFINITY path)

**0 over-mask admissions corpus-wide, out of 53,622 clearance rows** across all
four `segment_clearance` call sites (up from 25,715 rows in the original
census). 0 floored-cos rows — the floor is gone — 0 rows with `|cos| >= 0.5`
over-masked, 0 degenerate-length rows.

3,188 rows are embed-inset rows, matching the T8 seat count exactly: every
seated contact is also clearance-checked. Tightest margins observed:

| site | embed-inset rows | min usable (mm) | min usable fraction |
| --- | ---: | ---: | ---: |
| cco_candidate_clearance (make_overhang_table) | 1498 | 0.082556 | 0.120952 |
| brace_candidate_clearance (make_overhang_table) | 2 | 1.650165 | 0.339004 |
| cco_candidate_clearance (make_box) | 7 | 2.393403 | 0.799559 |
| cco_candidate_clearance (Cylinder) | 1681 | 2.754595 | 0.821141 |

The 12.1% worst case is tighter than the original census's 19.1%, and it is a
CCO seat — but it still retains usable segment, so the INFINITY path remains
untaken.

### T4 — tip radii and local surface curvature

286 contacts. Radii remain bimodal at the two ratified endpoints: `0.2 mm`
dominates, `1.0 mm` accounts for most of the rest, with load-derived
intermediates in between.

| model | contacts | flat | convex | concave | saddle |
| --- | ---: | ---: | ---: | ---: | ---: |
| Sphere | 7 | 0 | 7 | 0 | 0 |
| Cylinder | 54 | 51 | 3 | 0 | 0 |
| make_box | 11 | 11 | 0 | 0 | 0 |
| make_overhang_table | 84 | 84 | 0 | 0 | 0 |
| make_toed_slab | 72 | 64 | 8 | 0 | 0 |
| make_raking_blade | 40 | 30 | 1 | 8 | 1 |
| make_thin_fin | 18 | 18 | 0 | 0 | 0 |
| **aggregate** | **286** | **258** | **19** | **8** | **1** |

The corpus is still overwhelmingly planar (258/286 flat), but it is no longer
*only* planar: `make_raking_blade` contributes the corpus's only concave (8) and
saddle (1) seats, and `Sphere`/`make_toed_slab` contribute convex ones. Some
curvature coverage exists now where the original census had none — but 9
non-flat-non-convex seats on one procedural fixture is thin, and the real
curvature evidence still waits on the treant.

### Caveats this census carries

1. **The treant still contributes nothing.** The most representative model — and
   the one whose organic geometry most needs model anchors and curved seats — is
   stopped by a resource cap. Until it runs, every conclusion here rests on
   procedural fixtures plus two primitives.
2. **CCO model-anchor seating is unvalidated at scale.** 3186 seats, 3186
   centreline-outside, 0 committed. The numbers are large enough to look like
   evidence and are not: nothing downstream consumes them.
3. **`make_toed_slab`'s freeze is provisional** pending the vertex-probe vs
   interior-probe parity investigation; its 179 members and its digest here
   inherit that provisional status. Its `support_runner_integration` golden is
   also stale (see *Digest continuity* above), so it is doubly unpinned right
   now — re-pin it only after the parity investigation settles, or the re-pin
   will have to be redone.
4. **Area deficits are recorded, not ruled on.** 566 deficits across two families
   is a print-quality question this package deliberately does not answer.
