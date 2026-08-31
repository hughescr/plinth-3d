# Emitted-bytes audit baseline, hardened — 2026-08-31

The second run of `tools/plinth-audit/`, after an independent cross-family review returned
**NOT TRUSTWORTHY** on the first one and named three findings. This document reports what
closing them changed, measured against
[`audit-baseline-2026-08-30.md`](audit-baseline-2026-08-30.md) over the identical 31-file
corpus, and says plainly where they changed nothing.

**Instrument:** `tools/plinth-audit`, standalone crate, zero dependencies, no `plinth-*`
crate and no `geom_kernel`. Specifications `docs/specs/audit-contract.md`,
`docs/specs/audit-invariants.md`, and the three new ones —
`docs/specs/audit-span-criterion.md` (I9), `docs/specs/audit-spread-criterion.md` (I10),
`docs/specs/audit-interval-policy.md` (the tolerance direction rule). Binary sha256
`8f324d8424fad54b0b9f8dfe811ebe1dea60b757fb6e2f821fa64eedffd11a79`, **built from the sources
committed alongside this document, and that is checked rather than asserted.** The corpus was
swept twice: once on an earlier build, and again on this exact binary after every source edit
was complete. Every one of the 434 cells in §1 is byte-identical between the two runs. No
number here came from a patched build — the sabotage experiments of §4.3 were reverted and the
tree re-hashed before the run that produced these numbers.

**Run:** 31 artifacts, one process each, `timeout 1500` per model, 6-way parallel via
`xargs -P6`. Every model completed and **none came within an order of magnitude of the 1500 s
bound** — longest 125 s (`make_overhang_table`), median 29 s. The treant was not run.

**Verdict field renamed.** `print_candidate` is now `support_invariants_pass`, because the old
name claimed something the audit cannot establish. A `true` there means *the ten support
invariants this audit currently checks all passed*, and nothing stronger. `--help`, the human
summary line, the JSON schema and `AuditResult` all carry the new name.

---

## The headline

> ## 0 of 31 audited files passes all ten support invariants.
>
> Against `docs/job-manifest.md`'s denominator — 17 models required by the five jobs,
> 9 of them emitting — **0 of 9 job-manifest emitters passes.**

Mean gating invariants passed: **0.39 of 10** (12 gate-passes over 31 files), against
**0.32 of 8** (10 over 31) in the previous baseline. As a *share* of the gates offered the
corpus went slightly backwards, 4.03 % → 3.87 %, because two gates were added and only two
files pass either of them. The best file is still `sphere`, at 2 of 10.

Corpus totals over the 24 real miniatures — 8,070 support records, 1,330 contacts:

| invariant | failed | indeterminate | dominant code | vs. 2026-08-30 |
|---|--:|--:|---|---|
| I1 seat depth | 784 | 793 | `SEAT_DEPTH_INDETERMINATE`, then `SEAT_DEPTH_DECLARED_MISMATCH` | **identical** |
| I2 contact area | 306 | 982 | `CONTACT_AREA_INDETERMINATE` | **identical** |
| I3 min section | 851 | 399 | `MIN_SECTION_SHORT` | **identical** |
| I4 load ownership | 3 models failed, 1 passed | 20 indeterminate | `component_routing_failed` | **identical** |
| I5 closed load path | 60 | 2,388 | `PATH_INDETERMINATE reason=seat_indeterminate` | **identical** |
| I6 no intersection | 896 | 8 | `INTERSECT_OUTSIDE_SEAT` 870 | **identical** |
| I7 island coverage | 415 of 1,344 islands | 0 | `ISLAND_UNSUPPORTED` | **identical** |
| I8 no dead geometry | 2,696 | 994 | `GEOM_DEAD_MEMBER` 2,638 | **identical** |
| **I9 unsupported span** | **260 regions of 691,885** | **0** | `ISLAND_UNSUPPORTED` 241, `SPAN_UNSUPPORTED` 19 | new |
| **I10 arrangement spread** | **6,935 body-layer evals of 17,478** | **3,477** | `SPREAD_NO_SUPPORT`, `SPREAD_COLLINEAR_NO_EQUILIBRIUM` | new |

"Identical" there is not a figure of speech. Every one of the 310 `Nf/Mi` cells for I1–I8,
plus every record count and contact count, was machine-compared against the previous
baseline's table and **not one moved**. That is the central result of this run, and §4.3
explains it.

---

## 1. The table

One row per file, one column per gating invariant. A cell reads **`Nf/Mi`** — N items failed,
M items indeterminate. Both force `fail`; neither is a pass. `pass` means zero of both. The
`★` marks the 9 job-manifest emitters.

### 1.1 Real miniatures (24 emitting artifacts, 2026-08-25 rerun)

| file | recs | tips | I1 | I2 | I3 | I4 | I5 | I6 | I7 | I8 | I9 | I10 | gates | passes |
|---|--:|--:|---|---|---|---|---|---|---|---|---|---|--:|---|
| Bat Cat Owl ★ | 125 | 20 | **8f/11i** | **6f/12i** | **16f/4i** | **0f/1i** | **0f/20i** | **8f/0i** | **5f/0i** | **48f/12i** | **6f/0i** | **181f/40i** | 0/10 | no |
| Crawler Zombie | 378 | 61 | **39f/42i** | **13f/47i** | **44f/13i** | **0f/1i** | **0f/138i** | **25f/0i** | **15f/0i** | **128f/50i** | **12f/0i** | **143f/89i** | 0/10 | no |
| Decomposing Zombie | 641 | 105 | **70f/64i** | **22f/81i** | **64f/30i** | **0f/1i** | **0f/341i** | **58f/0i** | **21f/0i** | **229f/79i** | **20f/0i** | **218f/237i** | 0/10 | no |
| Deinonychus ★ | 1138 | 182 | **187f/86i** | **61f/119i** | **86f/80i** | **0f/1i** | **0f/394i** | **135f/0i** | **34f/0i** | **424f/162i** | **25f/0i** | **401f/426i** | 0/10 | no |
| Giant Wolf Spider ★ | 320 | 50 | **30f/33i** | **8f/41i** | **37f/12i** | **0f/1i** | **0f/62i** | **18f/0i** | pass | **116f/43i** | **1f/0i** | **937f/212i** | 1/10 | no |
| Morel base ★ | 544 | 96 | **54f/50i** | **16f/74i** | **45f/43i** | **0f/1i** | **20f/115i** | **132f/0i** | **33f/0i** | **141f/59i** | **32f/0i** | **282f/180i** | 0/10 | no |
| Myconid Adult ★ | 353 | 59 | **20f/33i** | **18f/41i** | **46f/13i** | **1f/0i** | **0f/64i** | **34f/0i** | pass | **116f/33i** | pass | **324f/122i** | 2/10 | no |
| Plague Zombie | 311 | 50 | **27f/35i** | **9f/41i** | **37f/12i** | **0f/1i** | **0f/100i** | **25f/0i** | **25f/0i** | **108f/39i** | **23f/0i** | **155f/120i** | 0/10 | no |
| Round 100 | 200 | 28 | **9f/7i** | **21f/7i** | **15f/13i** | **0f/1i** | **10f/11i** | **20f/6i** | **95f/0i** | **75f/13i** | **3f/0i** | **1068f/39i** | 0/10 | no |
| Round 25 | 36 | 7 | **2f/2i** | **5f/2i** | **0f/7i** | **0f/1i** | **0f/2i** | **13f/1i** | **6f/0i** | **1f/3i** | **1f/0i** | **239f/148i** | 0/10 | no |
| Round 50 | 89 | 16 | **11f/2i** | **11f/5i** | **5f/11i** | **0f/1i** | **15f/3i** | **28f/1i** | **25f/0i** | **12f/7i** | **6f/0i** | **535f/201i** | 0/10 | no |
| Skeleton_2 ★ | 433 | 74 | **31f/60i** | **4f/69i** | **49f/20i** | **0f/1i** | **0f/140i** | **59f/0i** | **25f/0i** | **132f/61i** | **23f/0i** | **333f/221i** | 0/10 | no |
| Skulker Zombie | 225 | 40 | **15f/26i** | **10f/29i** | **22f/18i** | **0f/1i** | **0f/44i** | **41f/0i** | **11f/0i** | **62f/28i** | **8f/0i** | **177f/152i** | 0/10 | no |
| Tavey ★ | 669 | 112 | **65f/74i** | **16f/87i** | **80f/20i** | **0f/1i** | **0f/254i** | **65f/0i** | **25f/0i** | **222f/85i** | **20f/0i** | **334f/416i** | 0/10 | no |
| Tavey Badger | 113 | 18 | **7f/10i** | **5f/12i** | **13f/3i** | **1f/0i** | **0f/36i** | **5f/0i** | **8f/0i** | **38f/10i** | **8f/0i** | **65f/29i** | 0/10 | no |
| Tavey Base ★ | 146 | 25 | **4f/18i** | **3f/22i** | **17f/7i** | **0f/1i** | **0f/28i** | **21f/0i** | **22f/0i** | **47f/18i** | **9f/0i** | **65f/23i** | 0/10 | no |
| Thorny Vegepygmy ★ | 329 | 56 | **38f/30i** | **16f/39i** | **29f/24i** | **0f/1i** | **5f/91i** | **52f/0i** | **5f/0i** | **109f/51i** | **5f/0i** | **775f/151i** | 0/10 | no |
| Zombie Cat | 179 | 29 | **16f/16i** | **6f/23i** | **24f/5i** | **0f/1i** | **0f/37i** | **12f/0i** | **15f/0i** | **56f/24i** | **12f/0i** | **256f/82i** | 0/10 | no |
| zombie-1-a | 300 | 50 | **27f/30i** | **8f/40i** | **42f/7i** | **0f/1i** | **5f/86i** | **18f/0i** | **5f/0i** | **109f/32i** | **5f/0i** | **98f/114i** | 0/10 | no |
| zombie-3-a | 215 | 35 | **12f/21i** | **4f/29i** | **23f/5i** | pass | **0f/62i** | **17f/0i** | **16f/0i** | **76f/27i** | **16f/0i** | **78f/59i** | 1/10 | no |
| zombie-5-a | 409 | 67 | **34f/39i** | **21f/44i** | **46f/20i** | **0f/1i** | **0f/102i** | **36f/0i** | **8f/0i** | **131f/42i** | **9f/0i** | **91f/121i** | 0/10 | no |
| zombie-6-a | 231 | 39 | **18f/21i** | **12f/25i** | **29f/10i** | **0f/1i** | **5f/48i** | **25f/0i** | **8f/0i** | **72f/27i** | **7f/0i** | **64f/81i** | 0/10 | no |
| zombie-8-a | 480 | 76 | **45f/57i** | **8f/62i** | **60f/13i** | **1f/0i** | **0f/155i** | **23f/0i** | **8f/0i** | **174f/57i** | **9f/0i** | **59f/138i** | 0/10 | no |
| zombie-9-b | 206 | 35 | **15f/26i** | **3f/31i** | **22f/9i** | **0f/1i** | **0f/55i** | **26f/0i** | pass | **70f/32i** | pass | **57f/76i** | 2/10 | no |

### 1.2 Corpus development fixtures (`exports/`)

Kept separate because they are synthetic test shapes, not miniatures.

| corpus fixture | recs | tips | I1 | I2 | I3 | I4 | I5 | I6 | I7 | I8 | I9 | I10 | gates | passes |
|---|--:|--:|---|---|---|---|---|---|---|---|---|---|--:|---|
| cylinder-first-artifact | 363 | 54 | **23f/13i** | **29f/13i** | **1f/48i** | **0f/1i** | **0f/26i** | **7f/6i** | pass | **132f/28i** | **1f/0i** | **0f/600i** | 1/10 | no |
| make_box-first-artifact | 69 | 11 | **3f/6i** | **3f/6i** | **0f/10i** | **0f/1i** | **0f/7i** | **0f/6i** | pass | **26f/11i** | **1f/0i** | **0f/200i** | 1/10 | no |
| make_overhang_table-first-artifact | 544 | 73 | **16f/28i** | **43f/28i** | **42f/30i** | **0f/1i** | **0f/46i** | **6f/16i** | pass | **245f/47i** | **5f/0i** | **284f/1472i** | 1/10 | no |
| make_raking_blade-first-artifact | 171 | 27 | **8f/6i** | **19f/8i** | **14f/13i** | **0f/1i** | **0f/15i** | **5f/6i** | **17f/0i** | **61f/15i** | **28f/0i** | **66f/31i** | 0/10 | no |
| make_thin_fin-first-artifact | 109 | 17 | **7f/4i** | **11f/6i** | **8f/9i** | **0f/1i** | **0f/6i** | **3f/4i** | pass | **40f/12i** | **1f/0i** | **0f/52i** | 1/10 | no |
| make_toed_slab-first-artifact | 402 | 64 | **3f/18i** | **44f/18i** | **44f/19i** | **0f/1i** | **0f/31i** | **1f/10i** | **2f/0i** | **166f/25i** | **3f/0i** | **154f/11i** | 0/10 | no |
| sphere-first-artifact | 70 | 11 | **4f/0i** | **9f/2i** | **5f/6i** | **0f/1i** | pass | **2f/0i** | pass | **26f/2i** | **1f/0i** | **0f/35i** | 2/10 | no |

### 1.3 Worked failures from the two new gates

```
SPAN_UNSUPPORTED  model=0 layer=451 z_mm=7.720534 z_above_plate_mm=22.575000
    reach_mm=1.436828 allowed_mm=1.344976 at_mm=0.567000,8.673500
    nearest_carrier_mm=1.687000,8.046500 carrier_kind=model
    region_area_mm2=2.104326 region_pixels=7911                      (Tavey Base)
    — 2.1 mm2 of down-facing material whose furthest pixel is 1.437 mm from anything
      cured under it, against a derived allowance of 1.345 mm

SPREAD_NO_EQUILIBRIUM  model=0 body=21 layer=170 z_mm=-3.952555
    centroid=1.450541,15.080398 margin_mm=-1.558802 sigma_mm=0.600876
    hull_vertices=4 hull_area_mm2=4.533758 contacts=12 resisting=12
    contact_ids=280,274,264,284,286,258,290,252,248,242,238,234     (Deinonychus)
    — twelve arms, all resisting, and the load resultant still falls 1.56 mm outside
      the hull they span: 2.6x the derived uncertainty on that measurement

SPREAD_COLLINEAR_NO_EQUILIBRIUM  model=0 body=0 layer=493 z_mm=-16.843215
    centroid=10.777336,15.605835 s_min_mm=0.000000 sigma_mm=2.136514
    axis_deg=0.000000 offset_mm=35.445922 contacts=1 contact_ids=52  (Round 100)
    — the degenerate end of the same criterion: ONE contact, so the affine hull of the
      lines of action is a point, and the load sits 35 mm off it
```

The derived gate constant is recomputed on every run and echoed in full:
`l_span_allowed_mm = l_survived_max_mm / condition_shift_max - 2*d2 = 2.13003225 / 1.53 -
2(0.023600847) = 1.344976 mm`, implying `alpha_min = 2.1290°` — against the legacy
`OVERHANG_ANGLE_DEG = 45°` still carried in the engine, **a factor of 26.9 in reach.** That
constant is disclosed in `declared_inputs` as a Placeholder and is read by nothing.

---

## 2. The delta against the previous baseline

**This is the section the document exists for.** Three changes landed; the table says which
of them moved which file.

### 2.1 Gate-count movement, per file

| file | 2026-08-30 | 2026-08-31 | Δ | attributable to |
|---|--:|--:|--:|---|
| Myconid Adult ★ | 1/8 | 2/10 | **+1** | I9 passes (0 of 52,565 down-face regions exceed the span allowance) |
| zombie-9-b | 1/8 | 2/10 | **+1** | I9 passes (0 regions exceed) |
| every other file (29) | unchanged | unchanged | 0 | — |

**No file lost a gate. No file gained a gate from I10, and no file gained a gate from the
tolerance sweep.** Two files gained one each, from I9, and only because I9 found nothing on
them — a gate passing is not new information about those files, it is the absence of new
information.

### 2.2 Movement by change, stated as a count

| change | files whose verdict moved | items whose verdict moved | what it added |
|---|--:|--:|---|
| **Finding 1** — I9 unsupported span | 2 (both gains) | **40 SPAN_UNSUPPORTED regions on 17 of 31 files**, of which **19 on 10 of the 24 real miniatures** | a defect class no previous gate could see |
| **Finding 2** — I10 arrangement spread | 0 | **6,935 refused body-layer evaluations on 27 of 31 files**; of the refusal *lines*, **58 involve three or more contacts** | a defect class no previous gate could see |
| **Finding 3** — conservative tolerance direction | **0** | **0** | nothing on this corpus; see §4.3 |

### 2.3 The corpus-total delta

| quantity | 2026-08-30 | 2026-08-31 |
|---|--:|--:|
| files passing every gate | 0 of 31 | 0 of 31 |
| gates offered per file | 8 | 10 |
| total gate-passes over the corpus | 10 | 12 |
| mean gates passed | 0.32 / 8 = 4.03 % | 0.39 / 10 = 3.87 % |
| files passing ≥1 gate | 9 | 9 |
| I1–I8 cells changed | — | **0 of 310** |

---

## 3. The fixture harness — the gate that licenses everything above

**Result: 11 of 11 expectations satisfied, 0 pending, 0 failed, from 10 fixture directories
on disk and 10 reached by discovery.**

### 3.1 Discovery is now gated on completeness, not on non-emptiness

The previous run's F1 was the harness discovering **zero** fixtures and returning `ok`. The
repair that shipped was `assert!(!expectations.is_empty())`. That catches the exact zero and
nothing else: **the same bug at N-1 walks straight through it.** A discovery that silently
misses one fixture directory still returns a non-empty vector, still goes green, and still
proves nothing about the check that fixture was written to exercise.

`discovery_reaches_every_fixture_directory_on_disk` now asserts that the set of directories
discovery reaches **equals**, name for name, the set on disk — enumerated a second time by a
flat `read_dir` that shares no line of code with the recursive walk it is checking, because
two calls to the same function cannot disagree. It also asserts that no `.chitubox` under
`fixtures/` is left uncovered by any expectation, with one declared exemption
(`broken_path_control`, the hand-audited negative control) named in a constant with its
reason, so a forgotten fixture cannot pass as a deliberate one.

Measured output:

```
discovery: 10 fixture directories on disk, 10 reached, 11 expectation files found
```

**Both branches were sabotaged and both fired.** Creating an empty `fixtures/SABOTAGE_TMP/`
produced `DISCOVERY GATE FAILED -- 1 fixture directory exists on disk that discovery never
reached: ["SABOTAGE_TMP"]`. Dropping an unnamed `SABOTAGE_orphan.chitubox` into an existing
fixture directory produced `planted-defect artifacts no expectation names:
["shallow_seat/SABOTAGE_orphan"]`. Both were removed and the gate went green again.

### 3.2 PENDING is now fatal, and so is a stub

Two further silent-green paths were closed, both of the same shape as F1:

* A fixture whose target check is still a stub was reported `PENDING` and **tolerated**. That
  was right while ten agents wrote ten checks in parallel and wrong the moment they all
  landed: a tolerated PENDING is a fixture that ran, proved nothing, and left the suite green.
  It now fails the suite.
* `every_unimplemented_check_reports_failure_never_pass` **printed** `"{stubs} of 10
  invariants are still stubs"` and asserted nothing about that number, so a check regressing
  to a stub would print a bigger number and pass. It now asserts `stubs == 0`. Its
  `if !sample.exists() { return; }` early exit — a test that skips itself into a pass — was
  turned into an assertion for the same reason.
* `checks/mod.rs::the_two_newest_invariants_are_registered_as_failing_stubs` built a
  `CheckOutcome` by hand and never called `Check::run`, so despite its name it established
  nothing about whether I9 or I10 was implemented. Renamed to
  `a_not_implemented_outcome_is_a_failure_for_every_registered_check`, widened to all ten,
  and its docstring now states what it does not prove.

### 3.3 Per-fixture result

| fixture | target check | verdict | also failing (collateral) |
|---|---|---|---|
| `bare_cavity_ceiling` | **I9 unsupported span** | **ok** | I4, I8, I10 |
| `bare_island` | I7 island coverage | ok | I4, I8, I9, I10 |
| `broken_path_cycle` | I5 closed load path | ok | I2, I3, I4, I8, I9, I10 |
| `broken_path_free_end` | I5 closed load path | ok | I2, I3, I4, I8, I9, I10 |
| `buried_arm` | I6 no intersection | ok | I1, I2, I3, I4, I8, I9, I10 |
| `collinear_arms` | **I10 arrangement spread** | **ok** | I4, I8, I9 |
| `double_load` | I4 load ownership | ok | I8, I9, I10 |
| `eccentric_polygon` | **I10 arrangement spread** | **ok** | I4, I8, I9 |
| `long_ledge` | **I9 unsupported span** | **ok** | I4, I10 |
| `shallow_seat` | I1 seat depth | ok | I2, I3, I4, I8, I9, I10 |
| `starved_patch` | I2 contact area | ok | I4, I8, I10 |

**Every gate's fixture fires, so no gate's numbers in §1 are withheld.** Collateral is
reported, not asserted away — but two collateral entries are load-bearing evidence rather
than noise, and they are the subject of §4.

Full suite: **`cargo test --release` exit 0 — 231 unit + 8 fixture + 7 real-artifact tests,
0 failed.**

### 3.4 What the fixture set still lacks

**Not one of the eleven expectations is a negative control.** All eleven are `check_fails`;
`check_passes` is supported by the harness and used by nobody. The harness's own module doc
says "every agent should ship at least one, or a check that fails everything looks perfect",
and no agent did. The property is covered at the unit level — `span_ledge_short_passes`,
`span_cavity_narrow_passes`, `spread_supported_four_corner_tips_pass` (which additionally
asserts `indeterminate == 0` and `passed > 0`, so it cannot be satisfied by a check that
refuses to answer) — but it is not covered where the fixture harness can see it. The
spread specification §7 asked for four I10 fixtures; two shipped.

---

## 4. The three findings, answered

### 4.1 Finding 1 — does I9 catch a long unsupported ledge that the old island check passed?

**Yes, and the fixtures prove it on the exact geometry the review named.**

`long_ledge` is a one-layer ledge, 3.00 × 2.00 mm, half seated over a grounded spine and half
projecting 3.00 mm past its edge with nothing under it and no support anywhere in the file.
Run through the full audit:

| | I1 | I2 | I3 | I5 | I6 | **I7** | I8 | **I9** |
|---|---|---|---|---|---|---|---|---|
| `long_ledge` | pass | pass | pass | pass | pass | **pass** | pass | **fail** |
| `bare_cavity_ceiling` | pass | pass | pass | pass | pass | **pass** | 2f | **fail** |

I7 passes both files. It is not that I7 was tuned too loosely — it is structurally unable to
see them: it skips a layer component as soon as any pixel in it has the same column cured
below, and both defects are attached along their whole rim. I9's measured lines:

```
long_ledge           reach_mm=2.996000  allowed_mm=1.344976  carrier_kind=model
                     at_mm=2.989000,-0.978500  region_area_mm2=5.920096
bare_cavity_ceiling  reach_mm=2.506000  allowed_mm=1.344976  carrier_kind=model
                     at_mm=-0.007000,-0.009500  region_area_mm2=25.140192
```

2.996 mm and 2.506 mm of reach against a 1.345 mm allowance — 2.2× and 1.9× over. Both match
the values their fixture READMEs predicted from geometry before the check existed.

**How often does it fire on real files?** `SPAN_UNSUPPORTED` fires **40 times on 17 of the 31
files**, of which **19 times on 10 of the 24 real miniatures**: Decomposing Zombie 1,
Deinonychus 3, Giant Wolf Spider 1, Plague Zombie 2, Round 100 3, Skeleton_2 1, Tavey Base 5,
zombie-5-a 1, zombie-6-a 1, zombie-8-a 1. Six of the sixteen are files **I7 passes outright**
— cylinder, Giant Wolf Spider, make_box, make_overhang_table, make_thin_fin, sphere — so on
those six the span criterion is the only gate that sees anything at all in this class.

**Two cautions on I9's headline number, because the raw count overstates the new capability.**

1. Of I9's 260 real-miniature failures, **241 are `ISLAND_UNSUPPORTED`, not
   `SPAN_UNSUPPORTED`.** I9 re-derives detached islands as the infinite-reach case and reports
   them under I7's own code string. That is defensible inside I9 but it means the two
   invariants report an overlapping defect set under one code, with *different* numerators —
   Morel base is 32 in I9 against 33 in I7, and Round 100 is 0 in I9 against 95 in I7. A
   reader summing failure codes across invariants will double-count. **The genuinely new
   signal is the 40 `SPAN_UNSUPPORTED` regions, not the 300.**
2. **Most of the 40 refusals sit inside I9's own disclosed conservatism band, and the tool
   says so.** `span_margin` counts regions refused by the gate but *shorter* than
   2.592036 mm, the shortest span at which a plate failure was ever actually recorded — the
   price of dividing by the upper end of the unidentified 1.31–1.53× condition shift. Across
   the corpus that is **28 of the 40**. A further **13** fall inside
   `span_metric_conservatism`, the derived upper bound on how far the 8-neighbour chamfer can
   overestimate the true in-plane geodesic (+12.0418 %, derived in the source rather than
   quoted). The tool does not report the intersection of those two sets, so the strongest
   honest statement is: **only 12 of the 40 refused regions are longer than any span ever
   measured to fail on a plate** — `make_raking_blade` 9, `cylinder` 1, `make_box` 1,
   `Round 100` 1, and three of those four files are synthetic corpus shapes rather than
   miniatures.

   Both bands carry `"verdict": "disclosed"`, gate nothing, and I9 refuses all 40 regardless,
   which is the conservative direction and the right one. But a reader treating 40 as forty
   independently-established defects would be overreading it, and on the 24 real miniatures
   the 19 `SPAN_UNSUPPORTED` regions include 18 inside the margin band. **I9's demonstrated
   value on this corpus rests on the fixtures and on the six I7-passing files where it is the
   only gate that sees anything — not on the size of its raw count.**

### 4.2 Finding 2 — does I10 refuse a collinear arrangement that satisfies every per-arm ratio?

**Yes, and "satisfies every per-arm ratio" is measured, not assumed.**

`collinear_arms` is three independent chains on one straight line, each sized so its own
Voronoi-owned share exactly satisfies the area law. `eccentric_polygon` is three tips in a
small triangle clustered to one side of the footprint, each oversized against its own share by
a 35 % margin (measured per-arm ratio 6.56–6.79 against `X_ALLOWED = 12`). On both files
**I1, I2, I3, I5, I6 and I7 all pass** and I10 refuses:

```
collinear_arms     SPREAD_COLLINEAR_NO_EQUILIBRIUM  axis_deg=0.000000
                   offset_mm=1.244500  sigma_mm=0.096663  contacts=3  contact_ids=0,4,8
eccentric_polygon  SPREAD_NO_EQUILIBRIUM  margin_mm=-0.158771  sigma_mm=0.078806
                   hull_vertices=3  hull_area_mm2=2.927166  contacts=3  resisting=3
```

The collinear offset is 12.9× its own uncertainty and the eccentric margin is 2.0× its own —
neither is a tolerance artefact. `docs/physics.md` clause (c) — *"three collinear arms have no
lateral stability perpendicular to their line, at any X"* — is now a number.

**On real files, read the breakdown and not the total.** I10 refuses 6,935 of 17,478
body-layer evaluations. Split by how many contacts the refused body actually had, over the 24
real miniatures, the refusal *lines* are:

| refusal | contacts = 1 | contacts = 2 | contacts ≥ 3 |
|---|--:|--:|--:|
| `SPREAD_COLLINEAR_NO_EQUILIBRIUM` | 501 | 74 | **18** |
| `SPREAD_NO_EQUILIBRIUM` | — | — | **40** |

plus 1,132 `SPREAD_NO_SUPPORT` lines for bodies with **zero** contacts. So the bulk of I10's
real-file firing is *under-supported lobes* — a true finding, and one no other gate reports,
but a much cruder one than the arrangement geometry Finding 2 named. **The clause-(c) case
proper — three or more arms, individually adequate, badly arranged — accounts for 58 refusal
lines across the 24 miniatures.** That is the honest number.

**I10 is not a blanket refuser**, which matters because a gate that fails everything is
indistinguishable from a broken one. It *passes* 361 of 396 evaluations on `sphere`, 536 of
982 on Myconid Adult and 972 of 2,079 on Round 100; its unit-level negative control passes
with `failed == 0` **and** `indeterminate == 0`. It also declines honestly: `cylinder` returns
600 of 600 `axis_horizontal` — the cylinder lies down, every contact axis is horizontal, and
I10 says so rather than guessing. Its budget was **never** exceeded on any file (peak 2,079
body-layer evaluations, on Round 100, against a 10,000,000 cap), so the `SpreadBudget` default is now
measured rather than assumed, at 4,800× the worst observed.

### 4.3 Finding 3 — did the tolerance sweep move any real file's verdict?

**No — not one item on any of the 31 files. And that is a measurement, not an absence of
one.**

The sweep was real and it was directional. `src/checks/interval.rs` is the single
implementation of the three-outcome comparison, with three separately named entry points
because "delivered must reach a requirement" and "delivered must not exceed a limit" take
*opposite* conservative bounds and a mechanical one-rule sweep would have inverted half the
checks. It is called from **six of the ten gating checks plus the report block**, at 19
sites — I1 (4), I2 (2), I3 (4), I4 (2), I5 (1), I8 (4), report (2). The retired
`v >= r - tol` form survives nowhere in `src/` except in comments naming what each site
replaced.

**The four gating checks that do NOT call it each have a stated reason, and none of them is
"nobody looked".** I6's breach volume and I7's island area are exact integer counts of a
printer quantum — a voxel, a pixel — so they carry no continuous uncertainty and have no
interval to widen; `island_coverage.rs` now carries that as a comment rather than leaving the
absence to be discovered. I9 pins its own comparison sign by sign because every uncertainty in
its criterion is already folded into `l_span_allowed_mm` in the failing direction (the erosion
band subtracted, the condition shift divided by the *larger* factor, the ladder read at the
*lower* rung), so routing it through `interval` would apply the same uncertainties twice in
opposite directions. I10 carries its own per-evaluation `sigma`, derived per body-layer rather
than per quantity. Those are four judgement calls, not four omissions, but they are four
places the policy is enforced by prose rather than by the shared helper.

The three ways this could be a false negative, each checked:

1. **"The optimistic comparisons were never found."** Ruled out by grep: no `>= r - tol`,
   `<= l + tol` or equivalent survives outside `interval.rs` and its own doc comments. The two
   remaining `+ tol` / `- tol` forms in `src/ownership.rs` are a tie-break and an at-the-clamp
   classifier, neither a safety-margin gate; equality-band predicates (`|a-b| <= tol`) are
   deliberately excluded by the policy, as are exact integer-quantum quantities such as I7's
   pixel-count areas, which `island_coverage.rs` now carries a comment explaining.
2. **"Nothing landed in the band."** Confirmed directly. The checks count their own straddles:
   `depth_band_indeterminate_count` (I1) and `area_law_band_indeterminate_count` (I3) sum to
   **0 across all 31 files**, and the reason token `within_tolerance_band` appears **zero
   times** in the entire corpus output. With `tol_geom` on the order of 1.7 × 10⁻⁵ mm, a
   continuous measured depth landing within ±1.7 × 10⁻⁵ mm of its threshold across 1,330 tips
   is rare, and it did not happen.
3. **"The straddle path cannot fire at all"** — the failure mode this project keeps
   reproducing, and the one that had to be tested rather than argued. **Sabotage:** the
   tolerance at I3's area-law comparison alone was inflated by 0.05 mm², the tool rebuilt, and
   Bat Cat Owl re-run. I3 moved from `16f/4i` to `14f/6i`,
   `area_law_band_indeterminate_count` moved 0 → 2, and **two `reason=within_tolerance_band`
   lines appeared**. Two contacts moved FAIL → INDETERMINATE and none moved to PASS, which is
   the correct direction for a widened conservative band. The sabotage was reverted and the
   binary re-hashed to the value that produced this document.

So: the path is live, it is reachable, it reports itself when it fires, and on this corpus it
never fired. **The engine's defects are not sitting at the safety boundary; they are nowhere
near it.** The 851 `MIN_SECTION_SHORT` failures run to 16× deficits and the 147 zero-depth
tips are at exactly zero. A tolerance-direction bug was a real defect in the instrument and
fixing it was right, but it was never what was hiding this corpus's problems.

---

## 5. Integration findings — what was wrong when the new modules met

Three agents who could not see each other's work produced two checks, one shared helper, four
fixtures and a tolerance sweep. These are the things that only appeared once they were run
together.

### G1 — Three independent connected-component implementations, with three different connectivities

Not a difference of style. The same primitive — connected components of one layer's cured
pixels over a run-length raster — now exists three times, and the three do not agree about
what a component is:

| implementation | used by | connectivity |
|---|---|---|
| `raster.rs::layer_components` | **I7** island coverage | **4-connected** |
| `unsupported_span.rs::components_8` (+ its own `DisjointSet`) | **I9** | **8-connected**, diagonal included |
| `arrangement_spread.rs::inplane_labels` (+ its own `Uf`) | **I10** | **parameterised**; called with `false` (4) for the body sweep and `true` (8) for a disclosure metric |

Two diagonally-touching blobs are one region to I9 and two islands to I7, on the same layer of
the same file. That is a live consistency hazard between two gates that already report an
overlapping defect set under a shared code string (§4.1). Neither is wrong on its own terms —
I9's specification calls for 8-connectivity and I10 documents choosing 4 as "the more-gating
direction, with the merge count disclosed" — but **nothing in the tool records that they
differ**, and a reader comparing I7's and I9's island counts has no way to know why they do
not match. **Reported, not resolved:** unifying them would change emitted verdicts, which is
outside this run's remit. It should be the first item of the next one.

### G2 — Two union-find implementations, written independently, three days apart

`DisjointSet(Vec<u32>)` in `unsupported_span.rs` and `Uf` in `arrangement_spread.rs`. Same
algorithm, different types, different APIs, neither aware of the other. Harmless today; it is
the same shape as F6 of the previous baseline ("five number formatters, three run-subtractors")
and it is now five copies of two primitives.

### G3 — I10's failure-line policy and its observed line count disagree

The module states failure lines are "one per `(body, code, reason)`, at that body's worst
layer", and the code's key is exactly that triple. Round 100 reports `bodies.count = 1` and
**456 failure lines**. The reconciliation is that the key's body index is a per-sweep
union-find id and lobes merge as the sweep climbs, so many transient bodies existed that the
final count does not show. The counts are exact either way — only the printed evidence is
affected — but a reader who divides lines by `bodies.count` will get nonsense, and the
docstring should say so.

### G4 — Stale claims left behind by the parallel layout

The `mod.rs` registry doc, `context.rs`'s `SpreadBudget` doc, `main.rs --help`, `lib.rs`,
`README.md` and `check.rs` all still described **eight** invariants, a `print_candidate`
verdict, and I9/I10 as unimplemented stubs. `--help` in particular told the user the exit code
meant "every model is a print candidate" — the exact overstatement the rename existed to
remove. All corrected. `fixtures/long_ledge/README.md` and its `expect.json` description still
say "I9 is a stub today and this fixture is PENDING"; those are fixture-owned files and are
left for their owner, but they are wrong, and the harness would now fail the suite if they
were right.

### G5 — Two fixtures had to work around a crash in a check they do not target

`bare_cavity_ceiling`'s README documents adding a detached "witness island" 20 mm away
*"purely to keep the model's total contact count above zero"*, to avoid an `i4_load_ownership`
crash; `long_ledge`'s README records the same crash. Both fixtures ship with the workaround
rather than the crash being fixed. I4 fails on both files today (1f), so the workaround is
holding, but **a check that panics on a zero-contact model is a defect that two fixture
authors independently routed around instead of reporting.** Not fixed here — it is engine-side
of the audit's own I4 and would change I4's emitted verdicts.

---

## 6. Reconciliation against the previous baseline's own reconciliation

The 2026-08-30 baseline left three disagreements with earlier hand measurements unresolved.
**This run settles none of them, and none is quietly dropped.**

### 6.1 4.2 % (hand) versus 11.12 % (audit) of tips at exactly zero depth — STILL OPEN

Reproduced exactly and unchanged: `SEAT_DEPTH_ZERO` fires **147** times over the 24 real
miniatures, against 1,330 contacts — 11.05 %, or 11.12 % on the previous baseline's 1,322-tip
denominator. The full I1 decomposition is byte-identical to the previous run:

```
SEAT_DEPTH_INDETERMINATE      793
SEAT_DEPTH_DECLARED_MISMATCH  634
SEAT_DEPTH_ZERO               147
SEAT_DEPTH_SHALLOW              3
```

Nothing in I9, I10 or the tolerance sweep touches seat reconstruction, so this run has no new
evidence either way. It stays exactly where the last baseline left it: the audit's figure
rests on 1,330 tips across all 24 emitters, the hand figure on 189 tips, and **the specific
189-tip subset that produces 4.2 % has still not been identified.** Re-deriving it remains the
only way to close this, and it needs the original hand script, not another audit run.

### 6.2 "5–43 zero-length segments per file" versus the audit's zero — STILL OPEN

Reproduced and unchanged. `GEOM_DEGENERATE reason=zero_length` fires **zero** times across all
31 files, and `reconstruction.degenerate_records` is **0 on every file, including `cylinder`**
— which is itself a small change from the previous baseline's prose, which reported 3
degenerate records on `cylinder`. The audit's hypothesis for the hand figure is unchanged and
still unconfirmed: the format encodes a `JointSphere` (type 9) junction ball as a record whose
start equals its end, so a naïve "records where start == end" count would count every junction
ball. `audit-invariants.md` §1.4 restricts the zero-length test to types 1, 3 and 4 for that
reason. **Confirming this needs the original hand script re-run; nothing in this work touched
I8's degeneracy predicate.**

### 6.3 "16 of 96 zero-area supports on Morel base" versus the audit's 9 of 96 — STILL OPEN

Reproduced exactly: **9 of 96** contacts on Morel base own less than the 0.000266 mm² censor,
and all 9 own exactly zero. The partition on that file is complete and unforced —
`a_unowned_mm2 = 0`, `a_owned_total = d_total = 498.926 mm²`, `sample_budget_exceeded` false —
so the 9 is not an artefact of a truncated sample.

One observation, offered as a lead and **not** as a resolution: the same file reports
`clamped_contacts = 17`, tips whose radius sits at a clamp limit. It is the nearest
same-order quantity the audit computes on that file, and 17 is not 16. I have not checked
whether the hand script's "carry zero area" meant zero *owned* area or something else, and
without that check the resemblance is a coincidence rather than an explanation. **Open.**

---

## 7. What this baseline says to do next, in order

1. **Unify the three connected-component implementations (G1)** and decide, once, what
   connectivity this tool uses. Two gates disagreeing about what a component is, while sharing
   a failure code, is a defect waiting to be discovered by someone comparing their counts.
2. **Ship the missing fixtures (§3.4)**: at least one `check_passes` negative control, and the
   two I10 fixtures the spread specification asked for. Every check currently proves it can
   see its defect; no check proves at the fixture level that it can decline to.
3. **Separate I9's `ISLAND_UNSUPPORTED` from I7's**, or give it its own code. The 260 vs 40
   gap between I9's headline and its new signal is a reporting hazard.
4. **Measure the 5° normal-agreement threshold** — unchanged from the last baseline, still the
   single highest-value item, still deciding whether the audit can see two-thirds of the
   corpus.
5. **Fix the zero-contact crash two fixtures routed around (G5)** rather than shipping the
   workaround.
6. **Arm sizing and spacing**, unchanged and unaddressed: 851 `MIN_SECTION_SHORT` with deficits
   to 16×, 41.2 % of arms at the `MIN_SHOOT_R_MM` placeholder, 80.4 % of contacts closer than
   the 2.5 mm reference. This is what the owner's "supports too thick, too tightly spaced"
   inspection was pointing at, and no gate added in this run moves it.

---

## 8. Reproducing this

```sh
cd tools/plinth-audit
RUSTC_WRAPPER="" cargo build --release     # sccache is broken here
RUSTC_WRAPPER="" cargo test  --release     # 231 + 8 + 7, exit 0

# the sweep, as run for this document: 23 files under
# plinth-rerun-2026-08-25/runs/*/out/, plus smoke/out/Round 25.chitubox,
# plus exports/*.chitubox excluding the treant = 31
timeout 1500 ./target/release/plinth-audit <file>.chitubox --json --out-dir <dir>
```

Exit code is 0 only when every model passes all ten gating invariants. It was **1 on all 31
files**. Per-file JSON documents were written to a scratch directory and are not committed:
run outputs do not belong in git.

**Declared inputs**, echoed in every document. Three are new; five of the eleven are a
Placeholder or an owner ruling rather than a measurement:

| input | value | provenance |
|---|--:|---|
| `pixel_pitch_x_mm` | 0.014 | Measured |
| `pixel_pitch_y_mm` | 0.019 | Measured |
| `layer_height_mm` | 0.050 | Measured |
| `default_contact_depth_mm` | **0.100** | **Owner ruling 2026-08-30** |
| `ball_factor_stub` | 1.5 | Placeholder |
| `x_allowed` | 12 | Derived, `floor(25.00/2)` |
| `tip_radius_min_allowed_mm` | 0.050 (radius) | Derived |
| `tip_radius_max_mm` | 1.000 (radius) | Placeholder |
| `span_l_survived_max_mm` | **2.13003225** | **Measured** — plate 1 rung 14 |
| `span_condition_shift_max` | **1.53** | **Measured** — plate 4 §3.3, upper end of an unidentified 1.31–1.53× shift |
| `span_overhang_angle_legacy_deg` | 45.0 | **Placeholder, and read by nothing.** Disclosed only so the 26.9× contradiction with the measured span stays visible |
