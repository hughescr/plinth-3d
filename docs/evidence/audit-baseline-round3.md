# Emitted-bytes audit baseline, round 3 — 2026-08-31

The third run of `tools/plinth-audit/`, after two things landed from agents who could not
see each other's work: a unified pixel-connectivity rule
(`docs/specs/audit-connectivity-rule.md`, Rule K1 + K4) and the first **known-good**
fixtures — five files built to PASS, derived by hand from `docs/physics.md` before the
audit was allowed near them (`docs/specs/audit-known-good.md`).

Measured against [`audit-baseline-hardened.md`](audit-baseline-hardened.md) over the
identical 31-file corpus.

**Instrument:** `tools/plinth-audit`, standalone crate, zero dependencies. Binary sha256
`6ecfde17890e08d0b87b25ed70e580f0114a60dc7c3750a2e14c81911447ac75`, built from the sources
committed alongside this document. Its provenance is checked, not asserted: the binary's
mtime postdates every file under `src/`, and it hashes to the same value **after** the full
test suite as it did before the sweep, so every number below came from this exact build.
Suite: `RUSTC_WRAPPER="" cargo test --release` → **237 + 8 + 7 passed, 0 failed, exit 0**.

**Run:** 31 artifacts, one process each, `timeout 1500` per model, 6-way parallel via
`xargs -0 -P6`. Every model completed; longest **118 s** (`make_overhang_table`), median
**28 s**, mean 37 s — none within an order of magnitude of the bound. The treant was not
run. Exit code was **1 on all 31 files**.

---

## 0. The headline result of this round

> ## The known-good files do NOT pass.
>
> **No file passes all ten. Every one of the five fails I8. The best, `trivial_cube`,
> reaches 9 of 10.**

Five files whose correctness is derived by hand in their own READMEs, from the load law
and the printer profile, before any check was run:

| known-good file | design | I1 | I2 | I3 | I4 | I5 | I6 | I7 | I8 | I9 | I10 | gates |
|---|---|---|---|---|---|---|---|---|---|---|---|--:|
| `trivial_cube` | KG-1 slab on 4 posts | pass | pass | pass | pass | pass | pass | pass | **12f** | pass | pass | **9/10** |
| `spread_wide` | KG-1 slab on 4 posts | pass | pass | **4f/4i** | pass | pass | pass | pass | **12f/12i** | pass | pass | 8/10 |
| `four_leg_overhang` | KG-1 slab on 4 posts | pass | pass | **4f/4i** | pass | pass | pass | pass | **12f/12i** | pass | pass | 8/10 |
| `marginal_but_adequate` | KG-1, thinnest seat | pass | pass | **4f/4i** | pass | pass | pass | pass | **12f/12i** | pass | pass | 8/10 |
| `hollow_drained` | KG-4 recessed block, 16 posts | pass | pass | pass | **1f** | pass | pass | pass | **48f** | pass | pass | 8/10 |
| *(diagnostic probe, not known-good)* `four_leg_overhang_probe_tight_joint` | KG-1, joints shrunk to 0.0005 mm | pass | pass | pass | pass | **4f/4i** | pass | pass | **12f** | pass | pass | 8/10 |

**So the answer to "is 0 of 31 a statement about the files or about the tool" is: both,
and now we can say which parts are which.** Counted over the five known-good artifacts,
**nine of the ten checks pass at least one file a human derived**:

| | I1 | I2 | I3 | I4 | I5 | I6 | I7 | **I8** | I9 | I10 |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| known-good files passed | 5/5 | 5/5 | 2/5 | 4/5 | 5/5 | 5/5 | 5/5 | **0/5** | 5/5 | 5/5 |

**I8 has never passed anything.** I3 and I4 pass some and refuse others, and §0.2 and §0.3
say exactly which construction each refuses and by how much.

### 0.1 I8 refuses every well-formed support chain — no construction escapes it

This is the largest finding of the round, and it was **predicted in writing before any
known-good file existed** (`audit-known-good.md` §5.1). Measured now on six independent
artifacts:

* `trivial_cube` — 12 of 16 records `GEOM_DEAD_MEMBER`, and **nothing else in the whole
  file fails**. Every TIP is live; every TAPER, MEMBER and FLARE is dead.
* `hollow_drained` — 48 of 64, the same 3-of-4 pattern at 16 posts.
* `spread_wide`, `four_leg_overhang`, `marginal_but_adequate` — 12 of 16 each, by the
  *other* mechanism: `GEOM_INDETERMINATE reason=path_indeterminate` (§0.2).

The mechanism is in the check's own module docs (`src/checks/dead_geometry.rs`). A record
is live only by role R1 (a TIP carrying demand), R2 (removing it disconnects a live TIP
from ground), R3 (it braces a degree-3 junction) or R4 (it is a raft primitive under a live
TIP). Ground clause (ii) is the seat itself: **a correctly seated TIP is its own ground.**
So for an unbraced, raft-free chain, R1 is TIP-only, R2 can never fire because the TIP
above is already grounded through its seat, R3 needs a brace the chain does not have, and
R4 needs a raft the file does not have. There is no route to "live" for the column that
holds the tip up.

Two independent constructions were tried — the R1-compliant overlapped joint and the
exact-coincidence joint — and I8 refuses both, by two different codes. **In the whole
known-good set there is no I8-passing construction.**

Corpus corroboration, and it is what makes the count unreadable rather than merely high:

| | non-TIP records | `GEOM_DEAD_MEMBER` | share |
|---|--:|--:|--:|
| `trivial_cube` (zero defects) | 12 | 12 | **100 %** |
| `hollow_drained` (zero defects) | 48 | 48 | **100 %** |
| the 24 real miniatures | 6,740 | 2,638 | 39.1 % |
| all 31 corpus files | 8,211 | 3,301 | **40.2 %** |

A file with no seat defects at all scores **worst**. The two endpoints are measured; the
mechanism between them is read from `dead_geometry.rs` and is only partly confirmed, so it
is stated with its limit:

* **Read from the source, not measured:** when a TIP's seat does *not* ground it, its only
  route to ground runs down its own column, so removing the taper *would* disconnect it,
  R2 fires, and the column is credited as live. That predicts a file's dead share should
  fall as its I1 failures rise.
* **Measured, and it does not confirm that cleanly:** across the 31 files the dead share of
  non-TIP records sits in a narrow band — min 0.034, median 0.399, max 0.499 — with no
  clean relation to the per-file I1 failure rate. So R2 is not the only thing separating
  the 100 % endpoint from the 40 % one: `GEOM_INDETERMINATE` takes 1,134 records over the
  31 files — 1,037 `ownership_indeterminate` (the ownership partition hit its sample budget,
  so R1 cannot be established for any TIP in that model) and 97 `path_indeterminate`
  (class B, §3) — and every one of those is excluded from role evaluation before liveness
  is ever decided.

What *is* established without inference: **on a file with zero defects, I8 reports every
non-TIP record dead, and it does so through two different codes on two different joint
constructions.** So the 2,638 `GEOM_DEAD_MEMBER` failures over the 24 real miniatures —
the largest single failure count in the whole audit — cannot be read as a defect count,
and no threshold on them can be either: a perfect file scores 100 %.

### 0.2 I3 and I8 reconstruct connectivity one way; I5 reconstructs it another

Three agents independently built the *same specified design* (KG-1, a 4×4×2 mm slab on
four posts) and got **three different byte-streams and two different I3 verdicts**. That
disagreement is the measurement.

`audit-known-good.md` §2.4 rule R1, grounded in `audit-invariants.md` §3.5, says a joint
must be built with a deliberate axial **overlap**, because a separation in `[0, d3]`
(`d3 = 0.0552901438 mm`) is fusion-uncertain and two solids that merely touch are not
established to cure as one. `spread_wide`, `four_leg_overhang` and `marginal_but_adequate`
built that joint, at 0.10 mm.

But I3 (`narrowest_section.rs`) and I8 (`dead_geometry.rs`) walk `ctx.graph`
(`graph.rs::SupportGraph`), which links two records only when their declared connective
points **coincide within `JOIN_TOL_EMITTED_MM = 1e-3 mm`** — 100× tighter than the overlap
the physics rule prescribes. So the walk never starts:
`MIN_SECTION_INDETERMINATE reason=path_open` on every contact, and
`GEOM_INDETERMINATE reason=path_indeterminate` on all 12 non-TIP records. I5
(`load_path.rs`) does not use that graph; it rebuilds its own from true solid-to-solid
separation, and its module docs say why — *"only the overlap graph is literally what the
specification asks for"*. I5 passes the same bytes cleanly.

`four_leg_overhang`'s agent then built the confirming experiment rather than asserting the
mechanism: `four_leg_overhang_probe_tight_joint`, byte-identical except the joints shrink
from 0.10 mm to 0.0005 mm, inside I3's tolerance. **I3 passes and I5 breaks**
(`PATH_INDETERMINATE reason=fusion_uncertain separation_mm=0.026118 band_mm=0.0552901`).

`trivial_cube` found the one construction that threads both — exact-coincidence joints
*plus* the taper lengthened from 0.50 mm to 0.60 mm, because at 0.50 mm this design's own
ball radius (0.4886 mm) brings the ball within 0.026666 mm of the member's start disc and
re-triggers the same fusion-uncertain flag between two records that are not even adjacent
in the chain. That is why `trivial_cube` reaches 9 of 10 and its two siblings reach 8.

**Read together: the tool accepts exactly one joint construction, and it is not the one the
project's own physics specification prescribes.** No fixture was adjusted to make a check
happy; the disagreement is shipped as three `check_fails` expectations that record it.

### 0.3 I4's books-equality floor does not scale with contact count

`hollow_drained` fails I4 clause (d) alone — every other clause passes, `clamped_contacts=0`,
so this is the strict equality, not a bracket:

```
d_total    = 70.55999359130874 mm2
implied    = 70.55999359372093 mm2
residual   = 2.4121931119225337e-9 mm2       ratio 0.99999999996581
floor      = max(4096*eps64*d_total, 1e-9)
           = max(6.417394e-11, 1e-9) = 1e-9 mm2      -> over by 2.41x
```

The relative error is **3.42e-11**. The same construction at four contacts
(`trivial_cube`, `spread_wide`, `four_leg_overhang`) lands at 2.66e-10 mm² and passes.
The dominant error source is the f32 quantisation of the tip radii the books are
reconstructed from: at this file's `r_tip = 0.3420308828 mm`, one f32 ULP of the radius
moves `12*pi*r^2` by **7.686e-7 mm²**, so the 1e-9 mm² floor is **769× tighter than a
single ULP of a single contact**. The fixture's own ULP sweep got the 16-contact sum to
0.31 % of one ULP-step and still failed. **The floor is absolute while the error it
bounds accumulates with the number of contacts.**

### 0.4 What the known-good files establish that nothing did before

The harness supported `check_passes` and, until this round, **not one fixture used it**.
All eleven expectations were `check_fails`, so a check that refused every file in the world
would have had a perfect fixture record. The tally now:

| | before | round 3 |
|---|--:|--:|
| expectations discovered | 11 | **60** |
| of which `check_passes` | **0** | **42** |
| of which `check_fails` | 11 | 18 (11 planted defects + 7 measured refusals of good files) |
| fixture directories on disk | 10 | **15** |
| distinct artifact byte-streams | 12 | **18** |

Both counts are now asserted separately in `tests/fixtures.rs`, because a total cannot see
a swap: if `satisfied_passing` ever returns to zero the tool has lost the only evidence it
has that its checks can pass a good file.

---

## 1. The corpus table

One row per file, one column per gating invariant. A cell reads **`Nf/Mi`** — N items
failed, M indeterminate. Both force `fail`; neither is a pass. `pass` means zero of both.
`★` marks the 9 job-manifest emitters. **`(was …)` marks every cell that moved since
`audit-baseline-hardened.md`, and only I7 and I9 have any.**

### 1.1 Real miniatures (24 emitting artifacts, 2026-08-25 rerun)

| file | recs | tips | I1 | I2 | I3 | I4 | I5 | I6 | I7 | I8 | I9 | I10 | gates | passes |
|---|--:|--:|---|---|---|---|---|---|---|---|---|---|--:|---|
| Bat Cat Owl ★ | 125 | 20 | **8f/11i** | **6f/12i** | **16f/4i** | **0f/1i** | **0f/20i** | **8f/0i** | **10f/0i** (was 5f/0i) | **48f/12i** | **7f/0i** (was 6f/0i) | **181f/40i** | 0/10 | no |
| Crawler Zombie | 378 | 61 | **39f/42i** | **13f/47i** | **44f/13i** | **0f/1i** | **0f/138i** | **25f/0i** | **45f/0i** (was 15f/0i) | **128f/50i** | **16f/0i** (was 12f/0i) | **143f/89i** | 0/10 | no |
| Decomposing Zombie | 641 | 105 | **70f/64i** | **22f/81i** | **64f/30i** | **0f/1i** | **0f/341i** | **58f/0i** | **35f/0i** (was 21f/0i) | **229f/79i** | **22f/0i** (was 20f/0i) | **218f/237i** | 0/10 | no |
| Deinonychus ★ | 1138 | 182 | **187f/86i** | **61f/119i** | **86f/80i** | **0f/1i** | **0f/394i** | **135f/0i** | **53f/0i** (was 34f/0i) | **424f/162i** | **34f/0i** (was 25f/0i) | **401f/426i** | 0/10 | no |
| Giant Wolf Spider ★ | 320 | 50 | **30f/33i** | **8f/41i** | **37f/12i** | **0f/1i** | **0f/62i** | **18f/0i** | pass | **116f/43i** | **1f/0i** | **937f/212i** | 1/10 | no |
| Morel base ★ | 544 | 96 | **54f/50i** | **16f/74i** | **45f/43i** | **0f/1i** | **20f/115i** | **132f/0i** | **81f/0i** (was 33f/0i) | **141f/59i** | **37f/0i** (was 32f/0i) | **282f/180i** | 0/10 | no |
| Myconid Adult ★ | 353 | 59 | **20f/33i** | **18f/41i** | **46f/13i** | **1f/0i** | **0f/64i** | **34f/0i** | pass | **116f/33i** | pass | **324f/122i** | 2/10 | no |
| Plague Zombie | 311 | 50 | **27f/35i** | **9f/41i** | **37f/12i** | **0f/1i** | **0f/100i** | **25f/0i** | **32f/0i** (was 25f/0i) | **108f/39i** | **26f/0i** (was 23f/0i) | **155f/120i** | 0/10 | no |
| Round 100 | 200 | 28 | **9f/7i** | **21f/7i** | **15f/13i** | **0f/1i** | **10f/11i** | **20f/6i** | **279f/0i** (was 95f/0i) | **75f/13i** | **15f/0i** (was 3f/0i) | **1068f/39i** | 0/10 | no |
| Round 25 | 36 | 7 | **2f/2i** | **5f/2i** | **0f/7i** | **0f/1i** | **0f/2i** | **13f/1i** | **46f/0i** (was 6f/0i) | **1f/3i** | **1f/0i** | **239f/148i** | 0/10 | no |
| Round 50 | 89 | 16 | **11f/2i** | **11f/5i** | **5f/11i** | **0f/1i** | **15f/3i** | **28f/1i** | **100f/0i** (was 25f/0i) | **12f/7i** | **6f/0i** | **535f/201i** | 0/10 | no |
| Skeleton_2 ★ | 433 | 74 | **31f/60i** | **4f/69i** | **49f/20i** | **0f/1i** | **0f/140i** | **59f/0i** | **32f/0i** (was 25f/0i) | **132f/61i** | **24f/0i** (was 23f/0i) | **333f/221i** | 0/10 | no |
| Skulker Zombie | 225 | 40 | **15f/26i** | **10f/29i** | **22f/18i** | **0f/1i** | **0f/44i** | **41f/0i** | **22f/0i** (was 11f/0i) | **62f/28i** | **10f/0i** (was 8f/0i) | **177f/152i** | 0/10 | no |
| Tavey ★ | 669 | 112 | **65f/74i** | **16f/87i** | **80f/20i** | **0f/1i** | **0f/254i** | **65f/0i** | **73f/0i** (was 25f/0i) | **222f/85i** | **32f/0i** (was 20f/0i) | **334f/416i** | 0/10 | no |
| Tavey Badger | 113 | 18 | **7f/10i** | **5f/12i** | **13f/3i** | **1f/0i** | **0f/36i** | **5f/0i** | **9f/0i** (was 8f/0i) | **38f/10i** | **9f/0i** (was 8f/0i) | **65f/29i** | 0/10 | no |
| Tavey Base ★ | 146 | 25 | **4f/18i** | **3f/22i** | **17f/7i** | **0f/1i** | **0f/28i** | **21f/0i** | **27f/0i** (was 22f/0i) | **47f/18i** | **12f/0i** (was 9f/0i) | **65f/23i** | 0/10 | no |
| Thorny Vegepygmy ★ | 329 | 56 | **38f/30i** | **16f/39i** | **29f/24i** | **0f/1i** | **5f/91i** | **52f/0i** | **14f/0i** (was 5f/0i) | **109f/51i** | **5f/0i** | **775f/151i** | 0/10 | no |
| Zombie Cat | 179 | 29 | **16f/16i** | **6f/23i** | **24f/5i** | **0f/1i** | **0f/37i** | **12f/0i** | **18f/0i** (was 15f/0i) | **56f/24i** | **15f/0i** (was 12f/0i) | **256f/82i** | 0/10 | no |
| zombie-1-a | 300 | 50 | **27f/30i** | **8f/40i** | **42f/7i** | **0f/1i** | **5f/86i** | **18f/0i** | **8f/0i** (was 5f/0i) | **109f/32i** | **5f/0i** | **98f/114i** | 0/10 | no |
| zombie-3-a | 215 | 35 | **12f/21i** | **4f/29i** | **23f/5i** | pass | **0f/62i** | **17f/0i** | **19f/0i** (was 16f/0i) | **76f/27i** | **16f/0i** | **78f/59i** | 1/10 | no |
| zombie-5-a | 409 | 67 | **34f/39i** | **21f/44i** | **46f/20i** | **0f/1i** | **0f/102i** | **36f/0i** | **14f/0i** (was 8f/0i) | **131f/42i** | **9f/0i** | **91f/121i** | 0/10 | no |
| zombie-6-a | 231 | 39 | **18f/21i** | **12f/25i** | **29f/10i** | **0f/1i** | **5f/48i** | **25f/0i** | **9f/0i** (was 8f/0i) | **72f/27i** | **9f/0i** (was 7f/0i) | **64f/81i** | 0/10 | no |
| zombie-8-a | 480 | 76 | **45f/57i** | **8f/62i** | **60f/13i** | **1f/0i** | **0f/155i** | **23f/0i** | **9f/0i** (was 8f/0i) | **174f/57i** | **9f/0i** | **59f/138i** | 0/10 | no |
| zombie-9-b | 206 | 35 | **15f/26i** | **3f/31i** | **22f/9i** | **0f/1i** | **0f/55i** | **26f/0i** | pass | **70f/32i** | pass | **57f/76i** | 2/10 | no |

### 1.2 Corpus development fixtures (`exports/`)

| corpus fixture | recs | tips | I1 | I2 | I3 | I4 | I5 | I6 | I7 | I8 | I9 | I10 | gates | passes |
|---|--:|--:|---|---|---|---|---|---|---|---|---|---|--:|---|
| cylinder-first-artifact | 363 | 54 | **23f/13i** | **29f/13i** | **1f/48i** | **0f/1i** | **0f/26i** | **7f/6i** | pass | **132f/28i** | **1f/0i** | **0f/600i** | 1/10 | no |
| make_box-first-artifact | 69 | 11 | **3f/6i** | **3f/6i** | **0f/10i** | **0f/1i** | **0f/7i** | **0f/6i** | pass | **26f/11i** | **1f/0i** | **0f/200i** | 1/10 | no |
| make_overhang_table-first-artifact | 544 | 73 | **16f/28i** | **43f/28i** | **42f/30i** | **0f/1i** | **0f/46i** | **6f/16i** | pass | **245f/47i** | **5f/0i** | **284f/1472i** | 1/10 | no |
| make_raking_blade-first-artifact | 171 | 27 | **8f/6i** | **19f/8i** | **14f/13i** | **0f/1i** | **0f/15i** | **5f/6i** | **17f/0i** | **61f/15i** | **28f/0i** | **66f/31i** | 0/10 | no |
| make_thin_fin-first-artifact | 109 | 17 | **7f/4i** | **11f/6i** | **8f/9i** | **0f/1i** | **0f/6i** | **3f/4i** | pass | **40f/12i** | **1f/0i** | **0f/52i** | 1/10 | no |
| make_toed_slab-first-artifact | 402 | 64 | **3f/18i** | **44f/18i** | **44f/19i** | **0f/1i** | **0f/31i** | **1f/10i** | **2f/0i** | **166f/25i** | **3f/0i** | **154f/11i** | 0/10 | no |
| sphere-first-artifact | 70 | 11 | **4f/0i** | **9f/2i** | **5f/6i** | **0f/1i** | pass | **2f/0i** | pass | **26f/2i** | **1f/0i** | **0f/35i** | 2/10 | no |

### 1.3 Corpus totals over the 24 real miniatures — 8,070 support records, 1,330 contacts

| invariant | failed | indeterminate | of | vs. hardened baseline |
|---|--:|--:|--:|---|
| I1 seat depth | 784 | 793 | 1,330 | **identical** |
| I2 contact area | 306 | 982 | 1,330 | **identical** |
| I3 min section | 851 | 399 | 1,330 | **identical** |
| I4 load ownership | 3 models | 20 | 24 | **identical** |
| I5 closed load path | 60 | 2,388 | 8,070 | **identical** |
| I6 no intersection | 896 | 8 | 8,070 | **identical** |
| **I7 island coverage** | **935** | 0 | **1,953** | **415 of 1,344 → +520 failed, +609 islands** |
| I8 no dead geometry | 2,696 | 994 | 8,070 | **identical** |
| **I9 unsupported span** | **320** | 0 | **1,345,207** | **260 of 691,885 → +60 failed, +653,322 regions** |
| I10 arrangement spread | 6,935 | 3,477 | 17,478 | **identical** |

"Identical" is machine-compared, cell by cell, not a figure of speech: every one of the
310 `Nf/Mi` cells for I1–I6, I8 and I10 across all 31 files is unchanged, and **not one
moved cell in the whole table lies outside I7 and I9.**

---

## 2. The delta against `audit-baseline-hardened.md`, attributed

### 2.1 Did the connectivity rule move verdicts? No. Did it move counts? Enormously.

| quantity | hardened | round 3 | Δ |
|---|--:|--:|--:|
| files passing all ten gates | 0 of 31 | **0 of 31** | 0 |
| total gate-passes over the corpus | 12 | **12** | **0** |
| files whose gate count changed | — | **0 of 31** | 0 |
| I7 failed islands (all 31) | 434 | **954** | **+520 (+119.8 %)** |
| I7 failed islands (24 minis) | 415 | **935** | **+520 (+125.3 %)** |
| I7 islands examined (24 minis) | 1,344 | **1,953** | **+609 (+45.3 %)** |
| I9 failed regions (all 31) | 300 | **360** | **+60 (+20.0 %)** |
| I9 failed regions (24 minis) | 260 | **320** | **+60 (+23.1 %)** |
| I9 regions examined (24 minis) | 691,885 | **1,345,207** | **+94.4 %** |
| files with any moved cell | — | **21 of 31** | |

The I7 delta is the same 520 at both scopes because **not one of the seven `exports/`
fixtures moved**: all the movement is in the 24 real miniatures. The hardened baseline
recorded only 24-miniature denominators, so the "examined" rows are given at that scope;
the round-3 all-31 figures are 1,998 islands and 1,462,004 regions.

**Every moved cell moved in the failing direction. Not one file's I7 or I9 count went
down, and no file gained or lost a gate.** The set of 12 gate-passes is the same set,
file for file: `sphere` (I5, I7), `Myconid Adult` (I7, I9), `zombie-9-b` (I7, I9),
`Giant Wolf Spider` (I7), `zombie-3-a` (I4), `cylinder` (I7), `make_box` (I7),
`make_overhang_table` (I7), `make_thin_fin` (I7).

### 2.2 The rule was applied, and the sites it moved are exactly the sites §4 assigns it

`docs/specs/audit-connectivity-rule.md` §4 rules K1 + K4 in at five sites. The measured
blast radius matches the ruling site for site:

| §4 site | file | check | predicted | measured |
|---|---|---|---|---|
| 1 `layer_components` | `raster.rs` | I7 | K1 already satisfied; **K4 new and binding** | **+520 failed islands, +609 islands examined** |
| 2, 3 `components_8` | `unsupported_span.rs` | I9 | **"the site whose numbers move"**, 8-conn → K1 | **+60 failed regions, regions examined ×1.94** |
| 4 `inplane_labels` | `arrangement_spread.rs` | I10 | K1 already satisfied → no movement | **bit-identical: 21,663 evaluations, 7,439 refusals, before and after** |
| 5 disclosure | `arrangement_spread.rs` | I10 | keep the looser rule, rename | **renamed; `sensitivity.a_bodies_merged_if_corner_contacts_joined` present in every document** |
| 7 `NEIGHBOURS` | `contact_area.rs` | I2 | already K1-compliant | **identical** |
| **6 `components_26`** | **`intersection.rs`** | **I6** | **K1 + K4 apply; §5.6 measured +67 records, breach volume ×69.4** | **NOT IMPLEMENTED — I6 is identical, 896f/8i** |

**Site 6 is a real, disclosed gap.** `raster.rs:661-665` says so in its own words: *"NOT
DONE HERE, and the gap is real… That file is outside this change's ownership and still
joins exterior voxels across corners and edges."* It is the loosest and most consequential
site in the tool, its own spec section measured what it would cost, and it did not land.
I left it that way deliberately: implementing an unreviewed change worth +67 failed records
and a ×69.4 breach volume in the same commit as this baseline would confound the very
attribution this document exists to make. It is the first item of the next round.

### 2.3 What the movement actually is — measured, not inferred

Both moved checks moved by **re-partition**, not by new detection: the same cured material,
cut into more pieces. The island count rose 45 % and the region count rose 94 % because K1
refuses to join two cells that meet only at a corner.

The failing islands are overwhelmingly the fragments that split creates:

| failing island size | count | share of the 954 |
|---|--:|--:|
| **exactly 1 pixel** (0.000266 mm²) | **520** | **54.5 %** |
| 2 pixels | 169 | 17.7 % |
| 3 pixels | 26 | 2.7 % |
| ≤ 3 pixels | **715** | **74.9 %** |
| > 10 pixels | 201 | 21.1 % |

(The coincidence that 520 one-pixel islands and the +520 delta are the same number is
observed, not derived: the hardened baseline's own size distribution was not recorded and
cannot be recomputed without rebuilding the old code, so no causal claim is made from it.)

This is **K4 working exactly as specified, with a side effect the specification did not
price.** I7 censors an island whose area is `≤ pixel_area`. K4 (`§3.1`, `§3.5`) rules that
the censor is evaluated on the **clump** — everything the looser rule would have joined —
so a defect cannot be fragmented below the censor and out of existence. That ruling is
right, and `island_coverage.rs:164-165` states its consequence correctly: *"the clump
contains the component by construction, so this is never looser than the old
`area_mm2 <= pixel_area`."* It is strictly **tighter**. And what it tightens onto, on this
corpus, is a rasteriser staircase: a single cured pixel on the shallow arc of a curved rim,
corner-attached to a 20-pixel clump, is now a reported unsupported island where the
one-pixel censor used to delete it.

`raster.rs`'s own unit test `the_one_pixel_censor_is_a_strict_boundary` still asserts *"one
pixel must NOT qualify as an island"*. That is now true only of an **isolated** one-pixel
component; the corpus produces 520 corner-attached ones that do qualify. The test passes
and no longer covers the case the check actually emits — worth a follow-up, and named here
rather than left silent.

---

## 3. Re-reading the corpus in the light of the owner's clarification

> *"my visual inspection doesn't say that 0/31 will actually print — I think some of them,
> especially the smaller/simpler ones might well print — just that many of them are badly
> constructed and that on many of them, some parts of the print might fail."*

The round-3 known-good measurements let three failure classes be separated from the rest,
each with a stated mechanism rather than a hunch:

| class | code(s) | corpus count | why it is a tool artefact |
|---|---|--:|---|
| **A — I8 liveness** | `GEOM_DEAD_MEMBER` | **3,301** | §0.1. Refuses 100 % of non-TIP records on a defect-free file; refuses 40.2 % on this corpus. Higher is *better*-built. |
| **B — I3/I8 joint graph** | `MIN_SECTION_INDETERMINATE reason=path_open`, `GEOM_INDETERMINATE reason=path_indeterminate` | **309 + 97** | §0.2. `SupportGraph`'s 1e-3 mm coincidence rule cannot see the ≥0.10 mm overlap the physics spec prescribes; I5 reads the same joints as connected. |
| **C — I7 pixel fragments** | `ISLAND_UNSUPPORTED` at ≤3 px | **715 of 954**, of which **520 are exactly one pixel** | §2.3. K1 splits a rasteriser staircase at a corner; K4 correctly aims the censor at the clump, which un-censors the fragment. Weaker than A and B: a one-pixel island is not *nothing*, it is **below the size at which I7's own censor was written to speak** — `raster.rs`'s unit test still asserts "one pixel must NOT qualify as an island". The claim here is that these 715 are not comparable to the 239 larger ones and must not be summed with them, not that each is harmless. |
| *(suspected, one file measured)* **D — I4 books floor** | `LOAD_BOOKS_DISAGREE` | 25 of 31 files | §0.3. The 1e-9 mm² absolute floor is 769× tighter than one f32 ULP of one tip radius, and does not scale with contact count. Established on `hollow_drained`; **not** established on any corpus file. |

Nothing here says a file will print. It says which refusals are the tool talking about
itself. **These are the files to look at first.**

### 3.1 The near-miss list

"Measured defects" counts, per file, the gating checks that produced at least one failure
whose code is *not* an `…INDETERMINATE` — a check that established something — with I8
excluded for the reason in §0.1. "Indeterminate-only" counts checks that failed having
established nothing at all.

| rank | file | gates | checks with measured defects (I8 excl.) | indeterminate-only | I7 failures that are ≤3 px | the measured list |
|--:|---|--:|--:|--:|--:|---|
| 1 | **`make_box-first-artifact`** | 1/10 | **3** | 5 | 0 (I7 passes) | I1 3 shallow seats · I2 3 short areas · I9 1 span region |
| 2 | **`sphere-first-artifact`** | 2/10 | 6 | 1 | 0 (I7 passes) | I1 4 · I2 9 · I3 5 · I4 1 · I6 2 · I9 1 — 70 records total |
| 3 | **`Round 25`** | 0/10 | 7 | 2 | **46 of 46 — all of it** | I1 2 · I2 5 · I4 1 · I6 13 · I9 1 · I10 65 |
| 4 | `make_thin_fin-first-artifact` | 1/10 | 6 | 2 | 0 (I7 passes) | I1 7 · I2 11 · I3 8 · I4 3 · I6 3 · I9 1 |
| 5 | `zombie-9-b` ; `Myconid Adult` | 2/10 | 6 | 1 | 0 (I7 passes) | the two best-scoring real miniatures |
| 6 | `Tavey Badger` ; `zombie-8-a` ; `make_toed_slab` | 0/10 | 8 | 1 | 1 each | small files whose I7 is almost entirely real |

**`make_box-first-artifact` is the single closest file in the corpus.** It passes I7
outright, and five of its remaining nine gates failed having established nothing — I3, I4,
I5, I6 and I10 are entirely `…INDETERMINATE`. Its whole measured defect list is *three
shallow seats, three short contact discs, and one down-face region past the span
allowance*. Every other refusal on that file is the tool declining to decide.

**`Round 25` is the case the owner's clarification predicts.** It is the smallest artifact
in the corpus — 36 records, 7 tips — and it is the file whose I7 verdict is **100 %
artefact**: all 46 reported unsupported islands are 1–3 pixel fragments of the curved rim's
raster staircase, at radius ≈ 8 mm, layers 100–106. Its I3 is *entirely* class B
(7 of 7 `MIN_SECTION_INDETERMINATE`), and its I8 reports a single dead member on 36
records. What is left after A, B and C are set aside is a short, concrete list: **one
zero-depth seat and one declared-depth mismatch, four short contact discs, one disconnected
contact patch, thirteen outside-seat intersections, one span region**, and I10's 65
arrangement refusals. That is a file with real local defects, not a file that fails
everything — and it is exactly the shape of *"some parts of the print might fail"*.

The same reading applies with force to the two larger base plates: `Round 100`'s I7 is
**278 of 279** ≤3 px and `Round 50`'s is **100 of 100**. Those two I7 numbers — the two
largest single-check jumps in §2 — carry no information about those files.

### 3.2 What is NOT a tool artefact

I6 `INTERSECT_OUTSIDE_SEAT` fires 894 times and is fully established on every one of them;
I2 `CONTACT_AREA_SHORT` 347 times; I3 `MIN_SECTION_SHORT` 965 times; I1 `SEAT_DEPTH_ZERO`
169 times and `SEAT_DEPTH_DECLARED_MISMATCH` 657. None of the four classes above touches
any of these, and the known-good files pass all four checks cleanly. **The corpus's seat,
contact-area, section and intersection failures survive this round intact**, and they are
the same defect classes the owner's own inspection named — supports too thick, too tightly
spaced, not supporting critical parts.

---

## 4. Integration notes — where two agents did the same thing differently

Neither difference was silently resolved; both are recorded, and neither was changed.

### 4.1 Two conventions for binding many expectations to one artifact

The harness derives an expectation's artifact from the expectation's own filename
(`<stem>.expect.json` → `<stem>.chitubox`), so a fixture asserting all ten invariants on
one design needs ten artifact paths. Two agents solved that two ways:

| convention | fixtures | mechanism | cost |
|---|---|---|---|
| **symlink** | `trivial_cube`, `marginal_but_adequate` | 9 symlinks to the canonical `.chitubox` | one byte-stream on disk; depends on the checkout preserving symlinks |
| **byte-identical copy** | `spread_wide`, `hollow_drained`, `four_leg_overhang` | 7–10 real copies | portable; 61 artifact files where 18 distinct byte-streams exist |

Both satisfy the discovery gate and both produce identical verdicts (verified: every copy
in a directory hashes the same, and each symlink resolves to its canonical file). **I left
both in place** rather than normalise, because rewriting either would touch fixture bytes
in the same commit that reports what those bytes measure. The right fix is neither: give
`.expect.json` an optional `"artifact"` field so one file can carry ten expectations. That
is a harness change for the next round, and it retires 43 redundant files.

### 4.2 Three independent builds of one specified design

`trivial_cube`, `spread_wide` and `four_leg_overhang` are all `kg_slab_four_post`
(`audit-known-good.md` §3.1), built by three agents who could not see each other, and they
produced three different sha256s and two different I3 verdicts (§0.2). That is not waste:
it is the closest thing this project has had to an independent replication, and the
disagreement is the finding. `spread_wide` and `four_leg_overhang` declare no expectation
at all for I3 and I8 on the canonical stem; `marginal_but_adequate` and `trivial_cube` do.
The verdicts in §0 are measured directly from the audit, not read from the expectations,
so that gap does not affect this document — but it means the harness does not currently
gate `spread_wide`'s and `four_leg_overhang`'s I3/I8 behaviour at all.

### 4.3 The fixture gate

Verbatim from the run:

```
discovery: 15 fixture directories on disk, 15 reached, 60 expectation files found
    bare_cavity_ceiling: 1     bare_island: 1        broken_path: 2
    buried_arm: 1              collinear_arms: 1     double_load: 1
    eccentric_polygon: 1       four_leg_overhang: 11 hollow_drained: 10
    long_ledge: 1              marginal_but_adequate: 10
    shallow_seat: 1            spread_wide: 8        starved_patch: 1
    trivial_cube: 10
    exempt: broken_path/broken_path_control -- the negative control of the pair
fixtures: 60 satisfied (42 check_passes, 18 check_fails), 0 pending, 0 failed
```

`discovery_reaches_every_fixture_directory_on_disk` compares the set discovery reaches
against a second, independent flat enumeration, and additionally requires every
`.chitubox` on disk to be named by some expectation or declared exempt
(`broken_path_control` is the one declared exemption). `every_fixture_trips_the_check_it_names`
now asserts three numbers rather than one — total, `check_passes` and `check_fails` — so a
swap cannot hide in the total. The old `assert_eq!(satisfied, 11)` was the only thing that
failed on the first integration run, exactly as a deliberate backstop should.

---

## 5. Reproducing this

```sh
cd tools/plinth-audit
RUSTC_WRAPPER="" cargo build --release     # sccache is broken here
RUSTC_WRAPPER="" cargo test  --release     # 237 + 8 + 7, exit 0

# the sweep, as run for this document: 23 files under
# plinth-rerun-2026-08-25/runs/*/out/, plus smoke/out/Round 25.chitubox,
# plus exports/*.chitubox excluding the treant = 31
timeout 1500 ./target/release/plinth-audit <file>.chitubox --json --out-dir <dir>
```

Note for whoever runs the sweep next: **macOS `xargs` is BSD and has no `-d`**. The first
attempt here used `xargs -d '\n'`, which failed on the option, ran zero files, and still
wrote its completion marker — the same shape as finding F1, in the harness that was meant
to have retired it. Use `tr '\n' '\0' | xargs -0`, and gate on the row count of the index
rather than on the loop exiting.

Per-file JSON documents were written to a scratch directory and are not committed: run
outputs do not belong in git.

## 6. What this round leaves open

1. **§4 site 6 — `intersection.rs::components_26` never moved to K1.** Ruled in, measured
   at +67 failed I6 records and a ×69.4 breach volume, not implemented. First item next.
2. **I8's liveness model** cannot pass a correct support chain. Until it can, its 3,301
   corpus failures and its contribution to `support_invariants_pass` mean nothing.
3. **I3/I8 and I5 must share one connectivity reconstruction.** I5's overlap graph is the
   one the specification describes; `SupportGraph`'s endpoint coincidence is the one two
   checks actually use.
4. **I4 clause (d)'s floor** needs to scale with the contact count it sums over, or the
   books need reconstructing at f32 precision. `hollow_drained` fails it by 2.41×.
5. **I7's censor under K4** now reports 520 one-pixel islands. K4 is right; the censor
   needs a companion rule for fragments the *K1 split itself created*, and
   `the_one_pixel_censor_is_a_strict_boundary` no longer covers the case the check emits.
6. **No `check_passes` fixture exists for I3 or I8 on `spread_wide` or `four_leg_overhang`**
   (§4.2), and the harness has no `"artifact"` field, so 43 of the 61 artifact files are
   duplicates (§4.1).
