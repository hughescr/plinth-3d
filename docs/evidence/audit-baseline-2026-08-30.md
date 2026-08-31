# Emitted-bytes audit baseline — 2026-08-30

The first run of `tools/plinth-audit/` over real emitted artifacts. It replaces the
metric "N of 37 models emit" — which counted files appearing on disk and could not tell
a printable file from an unprintable one — with **print-candidate models against
`docs/job-manifest.md`**, decided by reading only the emitted bytes.

**Instrument:** `tools/plinth-audit`, standalone crate, zero dependencies, no `plinth-*`
crate and no `geom_kernel`. Specifications `docs/specs/audit-contract.md` and
`docs/specs/audit-invariants.md`. Binary sha256
`c69367673c07452519db177f152b0325e86bfdcf98f9ee1919b874e8b796bf48`, built from the
sources in this commit; no `.rs` file under `src/` is newer than the binary that
produced every number below.

**Run:** 31 artifacts, one process each, `timeout 1500` per model, 6-way parallel.
Every model completed; **none hit the 1500 s bound** (longest 1330 s,
`make_overhang_table`). The treant was not prepared and was not put through a full
eight-invariant audit, per instruction; the one place it appears below is a seat-only
unit probe of the already-emitted `exports/weeping-treant-v1.2-first-artifact.chitubox`,
labelled as such.

---

## The headline

> ## 0 of 31 audited files is a print candidate.
>
> Against `docs/job-manifest.md`'s denominator — 17 models required by the five jobs,
> 9 of them emitting — **0 of 9 job-manifest emitters is a print candidate.**
> The manifest's provisional "0 print-candidates", written before any audit existed,
> is now a measured result rather than an absence of evidence.

Mean gating invariants passed: **0.32 of 8**. The best file in the corpus is the
synthetic `sphere` at 2 of 8. **No real miniature passes more than 1 of 8**: 20 of
the 24 pass none, and the other 4 pass exactly one.

Corpus totals over the 24 real miniatures — 8,070 support records, 1,330 contacts:

| invariant | failed | indeterminate | dominant code |
|---|--:|--:|---|
| I1 seat depth | 784 | 793 | `SEAT_DEPTH_INDETERMINATE` (normal ambiguous), then `SEAT_DEPTH_DECLARED_MISMATCH` |
| I2 contact area | 306 | 982 | `CONTACT_AREA_INDETERMINATE` |
| I3 min section | 851 | 399 | `MIN_SECTION_SHORT` |
| I4 load ownership | 3 models failed, 1 passed | 20 models indeterminate | `component_routing_failed`, `LOAD_BOOKS_DISAGREE` |
| I5 closed load path | 60 | 2,388 | `PATH_INDETERMINATE reason=seat_indeterminate` |
| I6 no intersection | 896 | 8 | `INTERSECT_OUTSIDE_SEAT` 870, `INTERSECT_FAR_SIDE_BREACH` 26 |
| I7 island coverage | 415 unsupported islands of 1,344 | 0 | `ISLAND_UNSUPPORTED` |
| I8 no dead geometry | 2,696 | 994 | `GEOM_DEAD_MEMBER` 2,638, `GEOM_MESH_DISAGREES_WITH_RECORD` 58 |

**Is the number too low to believe?** The brief said the honest expected answer was zero
or near it, and warned that a high number should make me suspect the tool. It came out
at zero, so the opposite suspicion applies: a tool that fails everything can be broken in
the failing direction. Three things argue it is not.

1. **The fixture harness is green and it is not vacuous.** All six planted-defect
   fixtures trip their own check with their own failure code, and the harness's own
   sabotage tests confirm it refuses a defect that does not fire and refuses a stub.
2. **Two of the eight invariants pass somewhere.** I5 passes on `sphere`, I7 on 8 files,
   I4 on `zombie-3-a`. A check that failed unconditionally could not do that.
3. **The failures are not uniform.** `INTERSECT_FAR_SIDE_BREACH` fires 26 times in
   8,070 records; `GEOM_MESH_DISAGREES_WITH_RECORD` 58 times. Those are the signatures
   of measurement, not of a blanket refusal.

What is *not* yet safe to read as an engine verdict is the **indeterminate** column,
which is large and is dominated by one tolerance. See "What the audit could not
establish" below.

---

## 1. The table

One row per file, one column per gating invariant. A cell reads **`Nf/Mi`** — N items
failed, M items indeterminate — because a bare verdict hides which of the two happened,
and they mean different things: `Nf` is a measured defect in the plan, `Mi` is the audit
declining to answer. Both force `fail` (`audit-invariants.md` §0 rule 3), neither is a
pass. `pass` means zero of both. The `★` marks the 9 job-manifest emitters.

### 1.1 Real miniatures (24 emitting artifacts, 2026-08-25 rerun)

| file | recs | tips | I1 | I2 | I3 | I4 | I5 | I6 | I7 | I8 | gates | candidate |
|---|--:|--:|---|---|---|---|---|---|---|---|--:|---|
| Bat Cat Owl ★ | 125 | 20 | **8f/11i** | **6f/12i** | **16f/4i** | **0f/1i** | **0f/20i** | **8f/0i** | **5f/0i** | **48f/12i** | 0/8 | no |
| Crawler Zombie | 378 | 61 | **39f/42i** | **13f/47i** | **44f/13i** | **0f/1i** | **0f/138i** | **25f/0i** | **15f/0i** | **128f/50i** | 0/8 | no |
| Decomposing Zombie | 641 | 105 | **70f/64i** | **22f/81i** | **64f/30i** | **0f/1i** | **0f/341i** | **58f/0i** | **21f/0i** | **229f/79i** | 0/8 | no |
| Deinonychus ★ | 1138 | 182 | **187f/86i** | **61f/119i** | **86f/80i** | **0f/1i** | **0f/394i** | **135f/0i** | **34f/0i** | **424f/162i** | 0/8 | no |
| Giant Wolf Spider ★ | 320 | 50 | **30f/33i** | **8f/41i** | **37f/12i** | **0f/1i** | **0f/62i** | **18f/0i** | pass | **116f/43i** | 1/8 | no |
| Morel base ★ | 544 | 96 | **54f/50i** | **16f/74i** | **45f/43i** | **0f/1i** | **20f/115i** | **132f/0i** | **33f/0i** | **141f/59i** | 0/8 | no |
| Myconid Adult ★ | 353 | 59 | **20f/33i** | **18f/41i** | **46f/13i** | **1f/0i** | **0f/64i** | **34f/0i** | pass | **116f/33i** | 1/8 | no |
| Plague Zombie | 311 | 50 | **27f/35i** | **9f/41i** | **37f/12i** | **0f/1i** | **0f/100i** | **25f/0i** | **25f/0i** | **108f/39i** | 0/8 | no |
| Round 100 | 200 | 28 | **9f/7i** | **21f/7i** | **15f/13i** | **0f/1i** | **10f/11i** | **20f/6i** | **95f/0i** | **75f/13i** | 0/8 | no |
| Round 25 | 36 | 7 | **2f/2i** | **5f/2i** | **0f/7i** | **0f/1i** | **0f/2i** | **13f/1i** | **6f/0i** | **1f/3i** | 0/8 | no |
| Round 50 | 89 | 16 | **11f/2i** | **11f/5i** | **5f/11i** | **0f/1i** | **15f/3i** | **28f/1i** | **25f/0i** | **12f/7i** | 0/8 | no |
| Skeleton_2 ★ | 433 | 74 | **31f/60i** | **4f/69i** | **49f/20i** | **0f/1i** | **0f/140i** | **59f/0i** | **25f/0i** | **132f/61i** | 0/8 | no |
| Skulker Zombie | 225 | 40 | **15f/26i** | **10f/29i** | **22f/18i** | **0f/1i** | **0f/44i** | **41f/0i** | **11f/0i** | **62f/28i** | 0/8 | no |
| Tavey Badger | 113 | 18 | **7f/10i** | **5f/12i** | **13f/3i** | **1f/0i** | **0f/36i** | **5f/0i** | **8f/0i** | **38f/10i** | 0/8 | no |
| Tavey Base ★ | 146 | 25 | **4f/18i** | **3f/22i** | **17f/7i** | **0f/1i** | **0f/28i** | **21f/0i** | **22f/0i** | **47f/18i** | 0/8 | no |
| Tavey ★ | 669 | 112 | **65f/74i** | **16f/87i** | **80f/20i** | **0f/1i** | **0f/254i** | **65f/0i** | **25f/0i** | **222f/85i** | 0/8 | no |
| Thorny Vegepygmy ★ | 329 | 56 | **38f/30i** | **16f/39i** | **29f/24i** | **0f/1i** | **5f/91i** | **52f/0i** | **5f/0i** | **109f/51i** | 0/8 | no |
| Zombie Cat | 179 | 29 | **16f/16i** | **6f/23i** | **24f/5i** | **0f/1i** | **0f/37i** | **12f/0i** | **15f/0i** | **56f/24i** | 0/8 | no |
| zombie-1-a | 300 | 50 | **27f/30i** | **8f/40i** | **42f/7i** | **0f/1i** | **5f/86i** | **18f/0i** | **5f/0i** | **109f/32i** | 0/8 | no |
| zombie-3-a | 215 | 35 | **12f/21i** | **4f/29i** | **23f/5i** | pass | **0f/62i** | **17f/0i** | **16f/0i** | **76f/27i** | 1/8 | no |
| zombie-5-a | 409 | 67 | **34f/39i** | **21f/44i** | **46f/20i** | **0f/1i** | **0f/102i** | **36f/0i** | **8f/0i** | **131f/42i** | 0/8 | no |
| zombie-6-a | 231 | 39 | **18f/21i** | **12f/25i** | **29f/10i** | **0f/1i** | **5f/48i** | **25f/0i** | **8f/0i** | **72f/27i** | 0/8 | no |
| zombie-8-a | 480 | 76 | **45f/57i** | **8f/62i** | **60f/13i** | **1f/0i** | **0f/155i** | **23f/0i** | **8f/0i** | **174f/57i** | 0/8 | no |
| zombie-9-b | 206 | 35 | **15f/26i** | **3f/31i** | **22f/9i** | **0f/1i** | **0f/55i** | **26f/0i** | pass | **70f/32i** | 1/8 | no |

### 1.2 Corpus development fixtures (`exports/`)

Kept separate because they are synthetic test shapes, not miniatures, and their
axis-aligned geometry behaves differently under the audit (see finding F5).


| corpus fixture | recs | tips | I1 | I2 | I3 | I4 | I5 | I6 | I7 | I8 | gates | candidate |
|---|--:|--:|---|---|---|---|---|---|---|---|--:|---|
| cylinder | 363 | 54 | **23f/13i** | **29f/13i** | **1f/48i** | **0f/1i** | **0f/26i** | **7f/6i** | pass | **132f/28i** | 1/8 | no |
| make_box | 69 | 11 | **3f/6i** | **3f/6i** | **0f/10i** | **0f/1i** | **0f/7i** | **0f/6i** | pass | **26f/11i** | 1/8 | no |
| make_overhang_table | 544 | 73 | **16f/28i** | **43f/28i** | **42f/30i** | **0f/1i** | **0f/46i** | **6f/16i** | pass | **245f/47i** | 1/8 | no |
| make_raking_blade | 171 | 27 | **8f/6i** | **19f/8i** | **14f/13i** | **0f/1i** | **0f/15i** | **5f/6i** | **17f/0i** | **61f/15i** | 0/8 | no |
| make_thin_fin | 109 | 17 | **7f/4i** | **11f/6i** | **8f/9i** | **0f/1i** | **0f/6i** | **3f/4i** | pass | **40f/12i** | 1/8 | no |
| make_toed_slab | 402 | 64 | **3f/18i** | **44f/18i** | **44f/19i** | **0f/1i** | **0f/31i** | **1f/10i** | **2f/0i** | **166f/25i** | 0/8 | no |
| sphere | 70 | 11 | **4f/0i** | **9f/2i** | **5f/6i** | **0f/1i** | pass | **2f/0i** | pass | **26f/2i** | 2/8 | no |

### 1.3 Worked failures — the measured value beside the verdict

A verdict without its number cannot be argued with. Representative measured lines,
copied from the JSON `failures[].message` (byte-identical to the human log, schema
rule 4):

```
MIN_SECTION_SHORT  model=0 contact=0 record=596 station_mm=4.48 a_min_mm2=0.015582
    a_required_mm2=0.07601817 a_owned_mm2=0.91221804 x_delivered=0.04048902
    r_at_min_mm=0.3499999940395355                                   (Deinonychus)
    — the narrowest section carries 4.9x less area than the law allows, at an arm
      sitting exactly on the MIN_SHOOT_R_MM placeholder

CONTACT_AREA_SHORT  model=0 contact=42 record=42 a_rast_mm2=0.216225
    a_required_mm2=0.394280 a_owned_mm2=4.731363 a_ideal_mm2=0.229180
    a_implied_mm2=1.235822 raster_loss=0.056527 x_delivered=21.881676  (Deinonychus)
    — the engine's own books say this tip carries 1.24 mm2; the reconstructed
      partition says it owns 4.73 mm2, and the cured patch delivers 0.216 mm2

INTERSECT_FAR_SIDE_BREACH  model=0 contact=4 record=4 exterior_components=2
    breach_volume_mm3=0.001915 breach_centroid=-15.651125,13.006819,3.998486
                                                                     (Deinonychus)
    — two exterior blobs means the tip came out the far side

ISLAND_UNSUPPORTED  model=0 layer=135 z_mm=-2.03474531173706 area_mm2=0.016758
    centroid=5.376555,0.990865 pixels=63                            (Bat Cat Owl)

GEOM_MESH_DISAGREES_WITH_RECORD  model=0 record=313 type=member
    max_deviation_mm=0.0000029197 tolerance_mm=0.0000027280  (Decomposing Zombie)
    — the shipped triangles are not quite the solid the record describes

SEAT_DEPTH_DECLARED_MISMATCH  model=0 contact=610 record=610 declared_mm=0.30000001
    reconstructed_mm=0 tolerance_mm=0.000016785 reason=taper_trailing
                                            (treant, seat-only unit probe, not a run)
```

The last one is the point of the whole exercise: the record declares 0.300 mm and the
geometry delivers 0.000 mm, and only an instrument that ignores the record can say so.
`SEAT_DEPTH_ZERO` fires on **147 of 1,322** real-mini tips that have a reconstructed
depth — 11.12 % of tips are balls resting on the surface with no overlap at all.

---

## 2. The fixture harness — the gate that licenses everything above

**Result: 7 of 7 expectations satisfied, 0 pending, 0 failed.** Every one of the six
fixtures trips its target check with the target failure code.

| fixture | target check | verdict | also failing (collateral) |
|---|---|---|---|
| `bare_island` | I7 island coverage | **ok** | I4, I8 |
| `broken_path_cycle` | I5 closed load path | **ok** | I2, I3, I4, I8 |
| `broken_path_free_end` | I5 closed load path | **ok** | I2, I3, I4, I8 |
| `buried_arm` | I6 no intersection | **ok** | I1, I2, I3, I4, I8 |
| `double_load` | I4 load ownership | **ok** | I8 |
| `shallow_seat` | I1 seat depth | **ok** | I2, I3, I4, I8 |
| `starved_patch` | I2 contact area | **ok** | I4, I8 |

**"And only its target check" is not met, and the collateral column is how you can see
that** rather than having to take my word for it. The harness as landed only asserted
that the target check fires; I added the collateral measurement so the claim is a number.
Most of the collateral is legitimate — a fixture with a deliberately broken load path
really does have contacts whose owned area and liveness cannot be established — but it
is disclosed, not assumed. Two of the entries shrank as a direct result of the I3 repair
in finding F2: `double_load` lost I3 and `starved_patch` lost I3 from their collateral
once I3 could measure anything at all.

**The harness's own sabotage tests pass**, which is what makes the green meaningful:
it confirms a defect that really fires, *refuses* a fixture whose claimed defect does
not fire, treats a stub as PENDING rather than satisfied, and refuses
`AUDIT_CHECK_NOT_IMPLEMENTED` as an expected code.

Full suite: **`cargo test --release` exit 0 — 175 unit + 7 fixture + 7 real-artifact
tests, 0 failed.**

---

## 3. Integration findings — what was wrong when the eight modules met

Eight check modules and six fixtures landed from agents who could not see each other's
work. These are the defects that only appeared once they were run together. Each one is
stated with the measurement that establishes it.

### F1 — The fixture harness found **zero** fixtures and passed vacuously (fixed)

`tests/fixtures.rs` scanned `fixtures/*.expect.json`; every agent wrote
`fixtures/<name>/<name>.expect.json`. The glob matched nothing, `load_expectations()`
returned an empty vector, and the test printed "no fixtures yet" and **returned `ok`**.
The gate the whole stage depends on was green because it had run nothing.

Fixed: discovery descends one directory level, and an empty discovery is now an
`assert!` failure rather than an early return — a harness that finds nothing must fail,
never pass. This is the single most important defect found in this stage, and it is the
exact shape of the "sabotage proves firing, not sufficiency" lesson: the harness was
never asked whether it could see its own fixtures.

### F2 — I3 could not measure a single cross-section on any file (fixed)

`i3_min_section` reported **0 of 11 measurable on `sphere`, 0 of 54 on `cylinder`, and
`section_component_empty` on 6 of the 7 fixtures.** The whole invariant produced no
number anywhere.

Cause, measured with an instrumented build: the station walk sampled arc-length `s` from
`0` to `total` **inclusive**, so its last station sat exactly on the terminal cap plane of
the grounded primitive. Under the full-containment fill rule no voxel is cured within
half a layer of a free surface, so `raster.contains_point` correctly returned `false`
there, and the code turned that single boundary station into an immediate
`Err(section_component_empty)` for the entire contact — discarding the ~714 stations that
had already measured successfully.

Fixed: an unseedable station is now counted and disclosed
(`stations_unseedable`, `unseedable_span_mm` on every contact row) instead of aborting
the contact. A contact with **zero** seedable stations is still `section_component_empty`,
so "measured nothing" still fails.

Cost of the repair, measured over the 24 real miniatures: **3,342 unseedable stations of
592,930 — 0.564 %**, all at the two ends of the polyline. Benefit: **931 of 1,330
contacts now have a measured narrowest section**, where the count was 0. The check's own
author's two unit tests (`a_small_demand_and_an_ordinary_arm_passes`,
`a_large_demand_on_the_same_arm_trips_min_section_short`) were failing on this and now
pass, which is the evidence that the repair restored intended behaviour rather than
invented new behaviour.

### F3 — The ownership sample budget was too small for real models, and one exhausted budget silenced four invariants (fixed)

`SampleBudget::max_samples` defaulted to 40,000,000. `exports/cylinder-first-artifact`
needs **132,907,008** samples to complete the Voronoi partition. Exhaustion makes
`A_owned` undefined, and I2, I3, I4 **and** I8 all read `A_owned`, so one exhausted
budget cascaded:

| cylinder | at 40 M (as landed) | at a sufficient budget |
|---|---|---|
| I2 contact area | 7 failed, **47 indeterminate** of 54 | 29 failed, 13 indeterminate |
| I3 min section | 0 failed, **54 indeterminate** of 54 | 1 failed, 48 indeterminate |
| I8 no dead geometry | 0 dead, **360 indeterminate** of 363 | **129 dead**, 3 degenerate |

The audit was reporting failure-by-ignorance and it looked identical to failure-by-defect.
Raised to 1,000,000,000, with the measurement recorded in the comment. Still finite,
still fails rather than coarsening. Sampling costs ~0.45 µs/sample, so the new ceiling is
about 450 s of one core — inside the 1500 s per-model bound. **Every number in this
document is from the raised budget**, and `sample_budget_exceeded` is `null` on all 31.

### F4 — Two agents read the same `Seat` field with two different establishment rules

`seat_depth` (I1) treats `normal_ambiguous` as indeterminate. `report_metrics` establishes
a scar whenever `entry_normal` is `Some` and ignores `normal_ambiguous` entirely, so
`report.scar_orientation.indeterminate_count` counts only contacts with *no* normal.
Measured gap:

| file | contacts | `normal_ambiguous` (I1) | `report.indeterminate_count` | under-reported by |
|---|--:|--:|--:|--:|
| Crawler Zombie | 61 | 42 | 8 | 34 |
| Decomposing Zombie | 105 | 64 | 10 | 54 |
| Giant Wolf Spider | 50 | 33 | 5 | 28 |
| Bat Cat Owl | 20 | 11 | 1 | 10 |

The scar distribution in §4 is therefore computed over a set that includes contacts whose
entry normal I1 refuses to trust. Reported here rather than silently repartitioned; the
right fix is for the report to disclose the ambiguous count, and it is not made in this
commit because it changes a non-gating number and belongs with the scar work in the next
stage.

### F5 — `island_coverage`'s own test asserted ideal geometry against a deliberately eroding rasteriser (fixed)

`an_unsupported_floating_island_fails_with_a_named_location` asserted the island area was
`1.0 mm² ± 0.05`. The measured value is **0.94962 mm²** — 0.0004 mm² outside the band.
The rasteriser is right and the test was wrong: the 1×1 mm island's faces lie exactly on
`x = 0` and `y = 0`, which are exactly a pixel-column and a pixel-row boundary of the
0.014 × 0.019 mm grid, so full containment blocks that column and that row and the cured
square is **70 × 51 pixels, not 71 × 52**:

```
70 * 0.014 * 51 * 0.019 = 0.949620 mm²   measured
71 * 0.014 * 52 * 0.019 = 0.982072 mm²   an off-lattice square of the same size
                          1.000000 mm²   ideal geometry
```

Fixed by asserting the pixel-exact value with the arithmetic in a comment, plus
`area < 1.0` to pin the direction of the erosion. Widening the band to make it green
would have deleted the only test that can catch a rasteriser regression here.

### F6 — Five number formatters, three run-subtractors, one shared helper with a silent precondition

Five agents each wrote a private float formatter for the human-readable failure lines
(`intersection::num`, `contact_area::num`, `load_ownership::num`, `seat_depth::fmt`,
`narrowest_section::fmt`), and they disagree: scientific notation below `1e-3`, below
`1e-4`, and never. The JSON values are unaffected (they all go through `Json::num`), so
this is a log-consistency defect, not a correctness one — reported, not changed, because
changing it churns five files for cosmetics.

More consequential: **two independent implementations of run-list subtraction**.
`load_ownership::subtract_runs` walks `b` with an early `break` on
`cut.c0 >= r.c1`, which is correct **only if `b` is sorted and disjoint** — an unstated
precondition. `intersection::subtract_runs` makes no such assumption (O(n·m), robust to
unsorted or overlapping cuts). `report_metrics::subtract_runs` is a third, different
signature. They agree on sorted disjoint input; the load-ownership variant is silently
wrong on anything else, and nothing in the code says so.

### F7 — `report.raster_erosion_sensitivity` is required by §4 and is two-thirds absent

The spec requires the count of contacts whose verdict would flip under a pixel-centre
fill rule, for I2, I3 and I6. Measured on every file: `i3_established: false`,
`i6_established: false`, and the I2 number is a **"local proxy, NOT I2's own
patch-component algorithm"** by its own disclosure. The stated reason is that
`report_metrics` "may not import" the sibling check modules — which is not true of Rust
(`crate::checks::narrowest_section` is importable); it is a coordination artifact of the
one-agent-one-file rule being read as a language restriction. The honest gap is disclosed
in the output rather than faked, which is the right call, but §4 is not delivered.

### F8 — The shared `synth` helper emits zero-area pole triangles, and only half the fixtures work around it

`synth::sphere_soup`'s pole triangles carry two bit-identical vertices, which
`i8_no_dead_geometry` correctly flags `GEOM_DEGENERATE reason=zero_area_triangle`.
Three fixtures filter them out in their generator; three do not. Measured:
`shallow_seat` reports 2 `zero_area_triangle` + 2 `empty_soup` degeneracies that belong
to the test scaffolding, `double_load` reports none. The defect is in
`src/synth.rs`, affects only synthetic files, and is left in place because fixing it
changes six committed fixture artifacts.

### F9 — `sccache` breaks the build in this environment

`cargo build` fails with `sccache: error: failed to execute compile ... Failed to read
response header` even after restarting the server unsandboxed. Every build and test in
this stage was run with `RUSTC_WRAPPER=""`. Not a defect in this crate; recorded because
the next person will hit it.

### F10 — The test suite took 38 minutes because one test audited the whole of `exports/` (fixed)

`a_file_is_never_a_print_candidate_while_a_stub_remains` walked every artifact in
`exports/`, including the 28 MB treant. The claim under test is a property of the driver,
not of any model, so it now runs on the two smallest artifacts and names them in the log.
Suite wall time: 38 min → 8 min.

---

## 4. The non-gating report — what drives the next stage

`verdict: "disclosed"`. None of this decides a print candidate. Over the 24 real
miniatures unless stated.

### 4.1 Spacing — contact-to-contact nearest neighbour

| quantity | measured |
|---|--:|
| per-model median, range across the 24 | **0.634 mm** (Deinonychus) to **4.377 mm** (Round 100) |
| median of the per-model medians | **1.255 mm** |
| closest pair anywhere in the corpus | **0.028 mm** (Morel base) |
| tips closer than `SPACING_FLOOR_MM` = 2.5 mm | **1,069 of 1,330 = 80.4 %** |
| tips closer than `TARGET_SPACING_MM` = 4.0 mm | **1,203 of 1,330 = 90.5 %** |

Both constants are `Placeholder` and are quoted as reference lines only; this metric does
not gate. Every one of the four display bases (`Round 25/50/100`, `Tavey Base`) sits near
4 mm; **every creature miniature sits near 1 mm.** The 4 mm target is being met only where
the geometry is a flat disc.

### 4.2 Arm radius at the narrowest section

| quantity | measured |
|---|--:|
| members at exactly `MIN_SHOOT_R_MM` = 0.350 mm | **954 of 2,317 = 41.2 %** pooled |
| per-model range | **20.0 %** (Round 25/50) to **53.6 %** (Morel base) |
| minimum arm radius, every single file | **0.350 mm** |
| median arm radius, real miniatures | 0.35 – 0.47 mm (0.416 mm on 12 of the 24) |
| median arm radius, corpus fixtures | 0.5 – 0.8 mm |

`MIN_SHOOT_R_MM` is an unmeasured placeholder and **it is the binding constraint on
41 % of every arm in the corpus.** The clustering at exactly 0.416 mm on half the real
miniatures is a second, undocumented attractor worth a look.

### 4.3 Scar placement

| quantity | measured |
|---|--:|
| `fraction_downward` (n·ẑ < 0) | 84.2 – 100 %, at 100 % on 9 of the 24 |
| `fraction_vertical` (\|n·ẑ\| ≤ 0.1) | 0 – 5.7 % |
| `fraction_upward` (n·ẑ > 0.1) | 0 – 10.5 % |

**Read this one with finding F4 in hand.** These fractions are computed over contacts
whose entry normal exists, which includes the 55–69 % of real-mini contacts that I1
calls `normal_ambiguous`. The buckets also overlap on (−0.1, 0) by construction, as
`audit-invariants.md` §4 defines them.

### 4.4 Resin split

| quantity | measured |
|---|--:|
| support as a fraction of poured resin, real miniatures | **4.2 % – 48.5 %** |
| highest | Deinonychus 48.5 %, Giant Wolf Spider 48.4 % |
| lowest | Round 25 4.2 %, Round 50 5.6 % (display bases) |

### 4.5 Per-file disclosed metrics

Column key: spacing median / minimum nearest-neighbour distance in mm; count of tips
closer than the 2.5 mm and 4.0 mm reference lines; tip count; fraction of members at
`MIN_SHOOT_R_MM`; member count; median and minimum arm radius in mm; scar fractions
downward / vertical / upward; contacts whose entry normal could not be established at
all (the `scar?` column, and see F4 — this under-counts); support as a fraction of
poured resin.
Corpus fixtures are in _italics_.

| file | spacing med | min | <2.5 | <4.0 | tips | arms@0.35 | members | arm med | arm min | down | vert | up | scar? | support |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| Bat Cat Owl | 1.778 | 1.447 | 16 | 20 | 20 | 38.9% | 36 | 0.423 | 0.35 | 84.2% | 5.3% | 10.5% | 1 | 27.4% |
| Crawler Zombie | 0.924 | 0.134 | 56 | 61 | 61 | 40.7% | 123 | 0.416 | 0.35 | 98.1% | 0% | 1.9% | 8 | 21.9% |
| Decomposing Zombie | 0.672 | 0.064 | 92 | 100 | 105 | 37.6% | 178 | 0.416 | 0.35 | 97.9% | 3.2% | 1.1% | 10 | 32.9% |
| Deinonychus | 0.634 | 0.091 | 171 | 176 | 182 | 38.3% | 326 | 0.416 | 0.35 | 100% | 0% | 0% | 66 | 48.5% |
| Giant Wolf Spider | 1.475 | 0.221 | 30 | 38 | 50 | 40.4% | 94 | 0.416 | 0.35 | 100% | 0% | 0% | 5 | 48.4% |
| Morel base | 1.043 | 0.028 | 84 | 92 | 96 | 53.6% | 140 | 0.35 | 0.35 | 96.6% | 0% | 3.4% | 9 | 28.6% |
| Myconid Adult | 2.184 | 0.36 | 40 | 51 | 59 | 36.4% | 99 | 0.432 | 0.35 | 98.3% | 0% | 1.7% | 0 | 15.1% |
| Plague Zombie | 1.022 | 0.144 | 44 | 50 | 50 | 41.9% | 93 | 0.416 | 0.35 | 100% | 2.2% | 0% | 4 | 28.6% |
| Round 100 | 4.377 | 1.954 | 4 | 13 | 28 | 21.1% | 71 | 0.5 | 0.35 | 100% | 0% | 0% | 10 | 19.5% |
| Round 25 | 3.951 | 3.933 | 0 | 4 | 7 | 20% | 5 | 0.434 | 0.35 | 100% | 0% | 0% | 2 | 4.2% |
| Round 50 | 4.299 | 2.051 | 2 | 2 | 16 | 20% | 20 | 0.394 | 0.35 | 100% | 0% | 0% | 6 | 5.6% |
| Skeleton_2 | 1.065 | 0.092 | 56 | 63 | 74 | 46.2% | 117 | 0.416 | 0.35 | 98.6% | 0% | 1.4% | 1 | 38.5% |
| Skulker Zombie | 1.255 | 0.572 | 31 | 35 | 40 | 51.8% | 56 | 0.35 | 0.35 | 94.7% | 5.3% | 2.6% | 2 | 19.8% |
| Tavey Badger | 1.104 | 0.542 | 15 | 18 | 18 | 36.4% | 33 | 0.461 | 0.35 | 94.4% | 0% | 5.6% | 0 | 19.6% |
| Tavey Base | 4.155 | 0.354 | 10 | 12 | 25 | 39.5% | 38 | 0.474 | 0.35 | 100% | 0% | 0% | 0 | 11.2% |
| Tavey | 0.736 | 0.166 | 101 | 105 | 112 | 42.3% | 194 | 0.416 | 0.35 | 98.1% | 1.9% | 0.9% | 6 | 32.1% |
| Thorny Vegepygmy | 1.816 | 0.192 | 38 | 47 | 56 | 44% | 100 | 0.385 | 0.35 | 100% | 0% | 0% | 15 | 19.4% |
| Zombie Cat | 1.609 | 0.324 | 25 | 26 | 29 | 44.6% | 56 | 0.379 | 0.35 | 100% | 0% | 0% | 2 | 28.7% |
| zombie-1-a | 1.002 | 0.202 | 44 | 47 | 50 | 46.5% | 86 | 0.416 | 0.35 | 100% | 0% | 0% | 2 | 17.7% |
| zombie-3-a | 1.494 | 0.558 | 28 | 32 | 35 | 36.5% | 63 | 0.416 | 0.35 | 97.1% | 5.7% | 2.9% | 0 | 15.1% |
| zombie-5-a | 1.167 | 0.277 | 52 | 63 | 67 | 43% | 121 | 0.416 | 0.35 | 98.4% | 0% | 1.6% | 3 | 13.2% |
| zombie-6-a | 1.173 | 0.508 | 29 | 38 | 39 | 47.8% | 67 | 0.372 | 0.35 | 97.3% | 2.7% | 0% | 2 | 15.4% |
| zombie-8-a | 0.849 | 0.059 | 74 | 76 | 76 | 42% | 143 | 0.416 | 0.35 | 98.7% | 0% | 1.3% | 0 | 34.2% |
| zombie-9-b | 1.302 | 0.223 | 27 | 34 | 35 | 37.9% | 58 | 0.41 | 0.35 | 100% | 0% | 0% | 1 | 11.1% |
| _cylinder_ | 4.026 | 0.965 | 2 | 2 | 54 | 7.7% | 65 | 0.8 | 0.35 | 87.8% | 12.2% | 0% | 13 | 3.2% |
| _make_box_ | 4.507 | 4.507 | 0 | 0 | 11 | 14.3% | 14 | 0.8 | 0.35 | 60% | 40% | 0% | 6 | 15.2% |
| _make_overhang_table_ | 4.193 | 2.842 | 0 | 7 | 73 | 28.7% | 188 | 0.5 | 0.35 | 72.5% | 27.5% | 0% | 22 | 27.1% |
| _make_raking_blade_ | 4.507 | 0.778 | 6 | 11 | 27 | 33.3% | 45 | 0.8 | 0.35 | 100% | 0% | 0% | 9 | 12.8% |
| _make_thin_fin_ | 4.507 | 1.562 | 4 | 6 | 17 | 29.2% | 24 | 0.8 | 0.35 | 100% | 0% | 0% | 7 | 10% |
| _make_toed_slab_ | 4.193 | 0.938 | 13 | 31 | 64 | 37.7% | 106 | 0.8 | 0.35 | 98.1% | 0% | 1.9% | 11 | 14.9% |
| _sphere_ | 4.232 | 1.915 | 2 | 4 | 11 | 6.3% | 16 | 0.8 | 0.35 | 100% | 0% | 0% | 2 | 5.2% |

### 4.6 Cantilever exposure (`report.overhang`) and elbow exposure

Both are populated. `Bat Cat Owl`: 95.36 mm² total unattached area across 145 layers,
worst unattached run 1.582 mm, worst run in a layer with no support pixel 0.266 mm. The
junction turn-angle histogram is populated per pairwise member combination at each
junction, which is the size of I3's documented elbow optimism.
`raster_erosion_sensitivity` is the one §4 field not delivered — see F7.

---

## 5. Reconciliation against the hand measurements of 2026-08-30

Six of the eight hand figures in the brief reproduce and two disagree. The larger of the
two disagreements resolves completely — to a definitional difference I can demonstrate to
within 3 % on a named model — and that resolution is worth more than either number on its
own. Two further figures from the brief, not in this table, are contradicted outright;
they are in §5.3.

| hand measurement | audit | verdict |
|---|---|---|
| every record declares 0.300 mm | **1,322 of 1,322** tips with a reconstructed depth declare 0.300 mm | **agrees exactly** |
| `X = carried_area/(π r²) = 12.000` on every contact, max dev 7e-15 | n = 1,330, min 11.999999999999996, max 12.000000000000004, **max deviation 3.55e-15** | **agrees** (and is tighter) |
| spacing median 1.03 mm | median of per-model medians **1.255 mm**; **Morel base alone: 1.043 mm** | **agrees** — the hand figure is per-model, and matches Morel base |
| one pair 0.049 mm apart | closest pair **0.028 mm** (Morel base) | **agrees in kind**; the audit finds a closer one |
| 36–54 % of arms at `MIN_SHOOT_R_MM` | per-model **20.0 – 53.6 %**, pooled 41.2 % | **agrees** on the miniatures; the low end is the display bases |
| support 21–48 % of poured resin | **4.2 – 48.5 %** | **agrees**; the low end is again the display bases |
| 4.2 % of tips at exactly zero depth | **11.12 %** (147 of 1,322) | **disagrees** — see below |
| only 11.6 % of tips reach the declared 0.300 mm; 27.5 % under 0.10 mm | **87.59 %** reach it; 11.65 % under 0.10 mm | **disagrees, and it is a near-inversion** |

### 5.1 The seat-depth disagreement resolves to axial versus perpendicular depth

The audit measures `h_delivered` **along the tip's own axis** — the distance from the
ball's outer pole to where that axis leaves the model — because that is what
`audit-invariants.md` §3.1 defines and it is the quantity the retracted-ball cut disc
`a = sqrt(2·f·r·h − h²)` is a function of. Measuring the same 1,322 tips
**perpendicular to the entry surface** instead:

| | axial (`h_delivered`) | perpendicular (`h_perp`) | hand figure |
|---|--:|--:|--:|
| reach the declared 0.300 mm | 87.59 % | **8.62 %** | **11.6 %** |
| under 0.10 mm | 11.65 % | **23.83 %** | **27.5 %** |

The hand figures sit beside the perpendicular column and nowhere near the axial one.
Morel base settles it beyond doubt — the hand measurement reported *"tips under 0.10 mm
carry 94.2 of 177.5 mm² of load"* on that model:

| quantity | audit, engine's own declared books | hand |
|---|--:|--:|
| total carried area on Morel base | **182.28 mm²** | 177.5 mm² |
| carried by tips with **perpendicular** depth < 0.10 mm | **91.72 mm²** | **94.2 mm²** |
| carried by tips with **axial** depth < 0.10 mm | 14.64 mm² | — |

Both hand numbers reproduce to within 3 % under the perpendicular reading and under no
other reading. The two measurements are of different quantities and both are correct
measurements of what they measure.

**Which do I believe, and for what?** I believe the audit's axial number as the
quantity to compare against `default_contact_depth`, and I believe the hand
measurement as a description of how shallow these seats are in the direction that a
surface normal points. The hand comparison — perpendicular depth against a threshold
defined axially — double-counts incidence: a tip meeting a face at 60° with 0.300 mm of
axial burial has 0.150 mm perpendicular, and calling that "short of 0.300" charges it
twice, because §3.1's own requirement already adds `r_tip·sin θ` to the axial threshold
for exactly that reason.

**This does not rescue the engine.** Under the audit's own predicate the shallow-seat
code, `SEAT_DEPTH_SHALLOW`, fires **3 times in 1,322 tips — 0.23 %**. Almost wherever the
audit can establish a seat, the delivered axial depth meets the 0.100 mm ruling. The
784 I1 failures decompose as:

```
SEAT_DEPTH_DECLARED_MISMATCH   634    the record says 0.300 mm, the geometry says otherwise
SEAT_DEPTH_ZERO                147    the ball never touches the model
SEAT_DEPTH_SHALLOW               3    genuinely short of its own requirement
```

The engine's problem is not that its seats are shallow. It is that **its records state a
depth it does not deliver, and 11 % of its tips never touch the model at all.**

### 5.2 The zero-depth disagreement is the same disagreement

4.2 % (hand) versus 11.12 % (audit) for tips at exactly zero. A tip resting tangentially
on a surface has zero depth by both definitions, so this is not a definitional split in
the same way — but the hand sample was **189 tips** and the audit's is **1,322**, which is
seven times larger and covers all 24 emitters. The 189-tip sample is roughly two models'
worth. I believe the audit's 11.12 % as the corpus figure and read the hand's 4.2 % as a
correct figure for a smaller sample. This one is worth re-deriving if it matters, because
unlike §5.1 I cannot point at the exact subset that produces 4.2 %.

### 5.3 One further hand figure the audit flatly contradicts

The brief records *"files carry 5–43 degenerate zero-length segments."* The audit reports
**zero `GEOM_DEGENERATE reason=zero_length` across all 24 real miniatures**, and zero
degenerate records of any kind on them (3 on `cylinder`, 58
`GEOM_MESH_DISAGREES_WITH_RECORD` corpus-wide, which is a different finding).
`reconstruction.degenerate_records` is 0 on Morel base. A `--dump-graph` of Round 25
shows every record with distinct start and end points.

I believe the audit here, with a specific hypothesis for the hand figure: the format
encodes a `JointSphere` (type 9) junction ball **as a record whose start equals its end**.
A count of "records where start == end" would therefore count every junction ball as a
zero-length segment. `audit-invariants.md` §1.4 restricts the zero-length test to types
1, 3 and 4 for exactly this reason, and the audit implements it that way. This is a
hypothesis I did not confirm by re-running the original hand script, and it should be
checked before the "5–43 zero-length segments" figure is quoted again.

Related, and also worth a check: the hand brief says *"16 of 96 supports carry zero area"*
on Morel base. Morel base has exactly 96 contacts, and the audit finds **9 of 96** owning
less than one pixel of demand (`a_owned_mm2 = 0` at the 0.000266 mm² censor). Same model,
same denominator, different numerator; the audit's partition is complete on that file
(42,310,573 samples, budget not exceeded, `a_unowned = 0`, `a_owned_total = d_total =
498.93 mm²`), so I believe the 9.

---

## 6. What the audit could not establish, and why that matters most

The `indeterminate` column is large and it is **not** an engine verdict. One tolerance
dominates it.

`audit-invariants.md` §3.1 cross-checks the crossed triangle's normal against the
area-weighted mean normal over the ball–surface intersection and marks the contact
`normal_ambiguous` above **5 degrees**. Measured disagreement angles on Crawler Zombie
run from 8.5e-7° to 70.2°. The 5° line fires on:

| file | contacts | `normal_ambiguous` | share |
|---|--:|--:|--:|
| Crawler Zombie | 61 | 42 | 69 % |
| Decomposing Zombie | 105 | 64 | 61 % |
| Giant Wolf Spider | 50 | 33 | 66 % |
| Bat Cat Owl | 20 | 11 | 55 % |
| `make_box`, `make_thin_fin`, `make_raking_blade` (synthetic) | 55 | **0** | 0 % |

The split between organic miniatures and axis-aligned synthetic boxes is the mechanism:
the rule fires where the mesh tessellation is coarse relative to the ball radius, which
is everywhere on a scanned miniature and nowhere on a box. It then cascades — an
indeterminate seat makes I5 report `PATH_INDETERMINATE reason=seat_indeterminate`
(2,388 of 8,070 records), and it removes the contact from I2's per-contact `A_owned`.

**This is honest but it is expensive**, and it is the first thing the next stage should
attack. The threshold is an unmeasured constant in the specification, not a physical one.
Either it is defensible — a normal that wrong really does leave the incidence term
unestablished — in which case the audit's reach on real miniatures is permanently
capped at about a third of contacts, or it is too tight, in which case two-thirds of the
corpus is being refused for no reason. **Deciding that needs a measurement, not an
opinion**, and it is the highest-value one available.

Three smaller gaps, all recorded rather than hidden:

- `seat.rs` maps three distinct failures — `point_inside` undecided, `axis_crossing`
  errored, and *"inside the solid but the axis never leaves it"* — onto the single token
  `ray_parity_exhausted`. On `make_box` that token accounts for 6 of 11 contacts and
  there is no way to tell from the output which of the three fired.
- I3 reports `path_open` on **244 of 1,330** contacts. This is I3's documented decision
  to exclude a contact's own seat from grounding its own walk, so a tip whose only route
  to ground is its own seat has no arm to grade. I5 calls the same contacts "grounded via
  seat" and passes them. Both are defensible; the divergence is documented in
  `narrowest_section.rs`'s module header and is not a defect, but a reader comparing the
  I3 and I5 columns will see it and should know why.
- `report.raster_erosion_sensitivity` — finding F7 — means the cost of the
  full-containment fill rule is not quantified for I3 or I6, on a corpus where that rule
  demonstrably removes real material (finding F5 measures it at one pixel column and one
  pixel row on a lattice-aligned edge).

---

## 7. Reproducing this

```sh
cd tools/plinth-audit
RUSTC_WRAPPER="" cargo build --release        # see finding F9
RUSTC_WRAPPER="" cargo test  --release        # 175 + 7 + 7, exit 0

# one file
./target/release/plinth-audit <file>.chitubox

# the sweep, as run for this document
./target/release/plinth-audit <file>.chitubox --json --out-dir <dir>
```

Exit code is 0 only when every model of the file passes all eight gating invariants. It
was 1 on all 31 files.

**Inputs are echoed in every document.** All 31 carry the same `declared_inputs`, five of
which are a `Placeholder` or an owner ruling rather than a measurement:

| input | value | provenance |
|---|--:|---|
| `pixel_pitch_x_mm` | 0.014 | Measured |
| `pixel_pitch_y_mm` | 0.019 | Measured |
| `layer_height_mm` | 0.050 | Measured |
| `default_contact_depth_mm` | **0.100** | **Owner ruling 2026-08-30**, down from the retired 0.300 |
| `ball_factor_stub` | 1.5 | Placeholder |
| `x_allowed` | 12 | Derived, `floor(25.00/2)` |
| `tip_radius_min_allowed_mm` | 0.050 (radius) | Derived |
| `tip_radius_max_mm` | 1.000 (radius) | Placeholder |

Per-file JSON documents (`<stem>.audit.json`) were written to a scratch directory and are
not committed: run outputs do not belong in git. Everything quoted above is reproducible
by re-running the sweep at this commit.

---

## 8. What this baseline says to do next, in order

1. **Measure the 5° normal-agreement threshold** (§6). It decides whether the audit can
   see two-thirds of the corpus. Nothing else changes as much per unit of work.
2. **Arm sizing.** 41.2 % of every arm in the corpus sits at the unmeasured
   `MIN_SHOOT_R_MM` placeholder, and I3 reports `MIN_SECTION_SHORT` on 851 contacts with
   deficits up to 16×. This is the defect the owner's "supports too thick, too tightly
   spaced" inspection was pointing at from the other side.
3. **Spacing.** 80.4 % of contacts sit closer than the 2.5 mm reference and 90.5 % closer
   than the 4.0 mm target; the target is met only on flat display bases.
4. **Dead geometry.** 2,638 `GEOM_DEAD_MEMBER` in 8,070 records — roughly a third of every
   emitted support carries no load, closes no path and braces nothing.
5. **Declared-versus-delivered.** The 0.300 mm seat-depth field is written on every record
   and is not what the geometry delivers. Either the field is made truthful or it is
   removed; a record that lies is worse than a record that is silent.
6. Close F4, F6 and F7 in the tool itself before the next sweep, so its own numbers stop
   needing footnotes.
