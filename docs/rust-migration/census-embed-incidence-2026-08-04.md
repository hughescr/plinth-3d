# Embed-incidence census — 2026-08-04

Empirical measurement only. No design decisions, no production behavior change.
Branch `census/embed-incidence-2026-08-04`, forked from `codex/embed-zones` at
`029ced3`. The instrumentation in this commit is throwaway and must never merge.

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
