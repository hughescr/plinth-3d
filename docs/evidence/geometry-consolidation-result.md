# Geometry consolidation — the integration record

What landed, every expectation that moved and why, what the corpus did, and what is still open.

Branch `feature/stage0-bytes-audit`, integrating the work of ten crate agents, a kernel agent
and a guard agent who could not see each other. The plan is
[`docs/plans/geometry-consolidation.md`](../plans/geometry-consolidation.md) (the census,
OP-1..OP-9, items C1..C8); the rulings are
[`docs/specs/geometry-canon.md`](../specs/geometry-canon.md). Both are committed here for the
first time.

The owner's standard for this work is the one it is written against:

> *"we don't need to write a test to confirm that it's bad and how bad and then fix it, and
> then make sure that the test that said it was bad no longer said it was bad... Just fix it —
> we know the fix! One common geometry module."*

So there are no characterisation tests of the retired code and no study of how far the old
divergence spread. What is here instead is the attribution.

---

## 0. The result in one paragraph

The workspace builds clean at `--release`. The full test suite is **1,638 passed / 1 failed**
at default features and **1,643 / 1** with `--all-features`; the one failure is the same
pre-existing deliberate red that `docs/development.md` already records. **One real defect
arrived with the sweep and is fixed here** (seven `clippy::float_cmp` errors in the new tests,
which would have failed the documented zero-warning gate). **Four expectations moved; all four
are attributed by bisect, not by argument**, and three of the four trace to one item (C7). Both
standing guards fire, and the import-rule guard's assertions are non-zero everywhere the census
measured zero. On the corpus, **all 24 round-3 emitting miniatures still emit**, and — the
measurement that actually carries information — **14 of 14 artifacts are byte-identical to a
pristine `814de04` control build**. The whole sweep, behaviour-changing items included, moved
**zero emitted bytes** on every model where a control exists.

**The change is sound.** Seven things are unfinished rather than wrong, and they are in §8.

---

## 1. What was consolidated, per operation

`geom_kernel` gained two public primitives that had **never** been public in the crate's
history (`closest::segseg`, `moller::moller`), one new public module (`poly2`), and one new
function (`vec3::normalize`). Sixty-three private helper functions were deleted from fifteen
files.

| # | operation | survivor | retired copies deleted from | left alone, and why |
|---|---|---|---|---|
| **OP-1** | segment/segment distance 3D | `geom_kernel::segment_segment_distance` (was the private `closest::segseg`) | `plinth-hollow/src/drilling.rs` | — |
| **OP-2** | ray/triangle | `geom_kernel::ray_triangle` + `RayTriangleHit` (was the private `moller::moller`) | `plinth-mesh/src/triage.rs` | triage's `Ambiguous`, its half-ray policy and its relative `1e-9` crossing dedup stay as **call-site policy**, marked |
| **OP-3** | point→segment 2D | `geom_kernel::poly2::point_segment_distance`, one degeneracy rule `DEGENERATE_EDGE_L2_MM2 = 2.5e-17` | `plinth-support/src/perception.rs`, `plinth-support/src/specialized.rs` | `joints.rs::segment_distance_2d` — `(axial, radial)` frustum profile, not slice data (canon §4.5) |
| **OP-4** | face normal | `geom_kernel::vec3::{cross, norm}`, exact `length > 0.0` degeneracy | `plinth-support/src/dyadic.rs`, `plinth-mesh/src/{lib,io,delicacy_eval}.rs` | — |
| **OP-5** | inside/outside by ray parity | **not consolidated** | — | `geom_kernel::parity` stays private; triage keeps its linear scan (census §7, canon §1 OP-5) |
| **OP-6** | `norm` / `normalize` | `vec3::norm` = `dot(a,a).sqrt()`; new `vec3::normalize -> Option<V3>` | 12 files (see §2) | 2-argument `hypot` (an XY magnitude, not a 3-vector norm) survives at 7 marked sites |
| **OP-7** | `dot` / `cross` | plain `vec3::dot` / `vec3::cross`, no FMA | 5 files (see §2) | `plinth-pipeline/src/coverage_evidence.rs` — 11 FMA sites, evidence-only by owner ruling, marked |
| **OP-8** | 2D polygon primitives | `geom_kernel::poly2::{signed_area, point_in_polygon, segments_intersect}` | `perception.rs`, `specialized.rs`, `plinth-layers/src/lib.rs`, `plinth-layers/tests/common/mod.rs` | — |
| **OP-9** | private 3-vector algebra | `geom_kernel::vec3` | 15 files, 63 functions | `tools/plinth-audit/src/vec3.rs` — **never**, its independence is its evidence |

`plinth-cli` and `plinth-verify` were checked and have **no** private vector algebra and no
`.mul_add(`: nothing to sweep, so they correctly carry no ratchet test.

### The one measured bug (C3)

`drilling.rs`'s `segment_distance` compared the mm⁴ determinant `|u|²|v|² − (u·v)²` against
the millimetre constant `PROOF_EPSILON_MM = 1e-7`. At drain-keepout scale that fired on
35.98 % of pairs and answered wrong by up to 4.494e-2 mm — 4.5 × 10⁵ times the margin of the
comparison it feeds. It is gone; `validate_final_holes` now calls the kernel and keeps its own
zero-length-axis refusal at the caller, as canon §1 OP-1 requires.

**The kernel agent corrected the canon while publishing it.** Canon §1 OP-1 says the kernel's
near-parallel branch "recovers a correct minimum". Measured over 20,000 near-parallel pairs at
drain-keepout scale against a 20,001-sample brute-force sweep, it does not: it lands up to
**5.145e-6 mm ABOVE** the true minimum (2.49e-3 relative), shrinking to 4.10e-8 mm at 0.5–5 mm
and 4.50e-9 mm at 5–50 mm. The distinction the canon draws still holds — 5.145e-6 mm is
1/58,000 of the 0.3 mm hard limit and 8,700× smaller than drilling's error — but the guard
prunes **by a measured margin, not by exactness**, and the public doc comment now says so.

---

## 2. Sites the sweep consolidated that the census never listed

The census's site list was not complete. Each of these was found by an agent applying canon §2
step 2 to its own crate, and each is a real instance of the class the canon named — but none
was in the ruled site list, and two of them have consequences outside the canon's stated
bounds. They are listed here rather than buried.

| site | class | what changed | consequence |
|---|---|---|---|
| `plinth-mesh/src/topology.rs` vertex weld | C8b | `dx.hypot(dy).hypot(dz) <= tolerance` → `norm(sub(..))` | a **weld** tolerance decision; ≤ 2 ULP |
| `plinth-mesh/src/triage.rs::point_on_triangle` | C8b | hypot chain → `norm` | ≤ 2 ULP |
| `plinth-support/src/specialized.rs::distance3` | C8b | hypot chain → `vec3::dist` | seat coalescing + attach reach comparisons |
| `plinth-support/src/member_sizing.rs::MemberSpan::new` | C8b | `horizontal_mm.hypot(dz)` → `norm([dx,dy,dz])` | **outside the C8b bound** — see §3.4 |
| `plinth-support/src/{census,coverage_closure,coverage_repair,lib,runner}.rs::dist_sq` | C7 | `mul_add` chains → `dot(sub, sub)` | few ULP |
| `plinth-mesh/src/{lib,io,delicacy_eval}.rs`, `plinth-orient/src/score.rs`, `plinth-hollow/src/drains.rs` | OP-6 spelling | componentwise `x / n` → `vec3::normalize`'s `x * (1.0 / n)` | ≤ 1 ULP per component — see §3.1 |

**`plinth-mesh/src/io.rs` measured its own blast radius and it is the model the others should
follow:** 27.26 % of f64 normal components move by exactly 1 ULP (≤ 1.110e-16), and **0 of
1,303,152 differ after the f32 cast its caller applies**, so no emitted STL byte moves.

### Two agents read the same rule two ways, and neither is wrong

Canon §2 step 2(d) makes "any divide of a vector by its own norm, however spelled"
MUST-CONSOLIDATE. Canon §3 bounds C7 to the FMA removal and C8b to ≤ 2 ULP.

- `plinth-mesh`, `plinth-orient` and `plinth-hollow` **adopted** `vec3::normalize`, measured
  the 1-ULP spelling movement, and documented it.
- `plinth-support` **declined** the same swap at `perception.rs::find_local_minima` and
  `dyadic.rs::face_normal`, keeping componentwise division and marking both `GEOM-EXEMPT`,
  on the ground that canon §3 "does not authorise" that movement for their items.

Both readings are defensible and they are inconsistent with each other. Nothing was silently
chosen: the difference is reported here and left in place, because changing either side now
would move bytes that this integration has measured to be still. **It needs a canon
sentence**, and it is §8 item 1.

### Ten guards, ten shapes

Every swept crate gained `tests/no_private_vector_algebra.rs` as canon §2 requires. The ten
are functionally equivalent and structurally divergent: the allowlist is a 2-, 3- or 4-tuple
depending on the crate, the forbidden list is called `FORBIDDEN_PATTERNS` / `BANNED_PATTERNS`
/ `FORBIDDEN` / `BANNED` / `FORBIDDEN_NEEDLES`, and `plinth-hollow` and `plinth-pipeline` key
their allowlists on **line numbers** while `plinth-support` keys on **line text** and argues
in its own module doc why a line number is the one part of the triple that goes stale
silently.

Only `plinth-support`'s carries the inline-**shape** matcher (`[0] * `, `[1] * `, `[2] * `
together on one production line) and explicit `MIN_SCANNED_FILES` / `MIN_SCANNED_LINES` /
`MIN_PRODUCTION_LINES` gates plus an assertion that **every allowlist entry matched something**
(a stale entry fails the test). The other nine gate on a scanned-file count only. A renamed
helper — `fn seat_len(` — walks through nine of the ten. Guard 2 covers that gap workspace-wide
by AST shape, so the tree is protected today; the per-crate ratchets are uneven.

---

## 3. Every moved expectation, attributed

**Four expectations moved. Nothing was deleted or weakened.** Across the whole engine diff:
removed `assert` lines **0**, removed `#[test]` **0**, removed `.expect(` **0**. 75 tests were
added (19 inline, 56 in new files).

Each attribution below was established by **bisect** — reverting exactly one change with every
other change in place, and observing the old value return — not by reasoning about which
change ought to be responsible.

### 3.1 `plinth-orient::orientation_dense_sweep::the_fibonacci_lattice_is_a_frozen_golden`

| | |
|---|---|
| old | `f10c343324d390bd3f72074484270d67db57f401991e7e8320b86fd292bce051` |
| new | `67a4fa9dad43aed93db4296eb636e371e4b1d8ce43d357106ce4b3a40a44f089` |
| item | **C4 / canon §1 OP-6** — `UpDirection::new` (`plinth-orient/src/score.rs`) |
| bisect | restore componentwise division there, leave everything else: golden returns to `f10c3433…` exactly |

The retired body divided each component by the norm; `vec3::normalize`'s canon-ruled body is
`scale(a, 1.0 / norm(a))`. `x / n` and `x * (1.0 / n)` are different `f64`s, so every direction
the 512-point lattice constructs may move by up to 1 ULP per component. The **norm itself did
not change** — the retired expanded sum-of-squares is bit-identical to `dot(a,a).sqrt()`
(census measurement, 0 differences over 2,000,000 vectors).

**Why the new value is correct.** The refusal condition is unchanged: `normalize` returns
`None` exactly where the old finite/positive-norm guard refused. Nothing downstream moved —
the eight sibling tests in that binary still pass, including the dense sweep reaching the same
pose the committed `Sphere` baseline records, and
`orientation_rederivation::every_pinned_orientation_record_rederives_its_committed_oriented_solid_artifact`
is green. This test's own doc comment predicts exactly this failure mode ("fails the moment
the expression order … moves"). The prior value is kept verbatim above the new one, per the
append-don't-rewrite convention in `docs/development.md`.

### 3.2–3.3 Three `plinth-support::specialized` digests

| test | model | old | new |
|---|---|---|---|
| `oriented_toed_slab_and_raking_blade_finalize_after_terminal_seat_coalescing` | toed slab | `5_397_140_793_948_005_744` | `10_015_988_567_687_648_348` |
| same | raking blade | `9_036_091_150_423_983_742` | `6_765_996_461_499_092_628` |
| `required_table_raking_and_thin_fin_fixtures_finalize_full_evidence` | toed slab | `11_859_964_443_161_510_103` | `12_394_382_057_751_394_813` |

| | |
|---|---|
| item | **C7 / canon §1 OP-4** — `plinth-support/src/dyadic.rs` de-FMA |
| bisect | restore ONLY `dyadic.rs`'s fused `dist_sq` / `triangle_area_mm2` / `face_normal`, leave every other sweep change in place: **all three digests return exactly** |

`dyadic.rs` broke the kernel's no-FMA house rule (`vec3.rs:1-5`, spec sec 5.1) in three
functions, and those three feed `build_rows` → the coverage sampler, which is what these
digests bind. Canon §3 C7 predicts precisely this: *"every face normal, edge length, and
triangle area in the coverage sampler … island demands may shift at sampling thresholds."*

**Everything else these tests assert is unchanged and still checked**: contact counts,
coverage-sample counts, load-row counts, `coalesced_seat_count`, `shared_seats`,
`baseline == accepted` (non-worsening), every contact on a distinct physical position,
`unique_faces`, `thin_fin_coverage`. Only the last bits of the sampled normals moved.

**This also discharges a canon "stop" condition.** Canon §3 says `specialized` is expected to
move **zero** digests under C6 and that movement there "means the ruling was misapplied: stop".
The bisect shows the movement is C7's, not C6's: with `dyadic.rs` alone reverted and C6, C8b's
`distance3`, C2, C8a, C1, C5 and C4 all still in place, the digests are the pre-sweep values.
**C6 is confirmed neutral on `specialized`, exactly as ruled.** An earlier bisect had already
cleared `distance3`: restoring its `hypot` chain alone changed nothing.

### 3.4 `plinth-support::member_sizing::frozen_deflection_formula_uses_its_dimensional_micro_span_clamp`

| | |
|---|---|
| old | a subnormal span `[0, 0, f64::from_bits(1)]` reached the micro-span clamp and returned a radius bit-equal to the 1e-6 mm span's |
| new | that span is `MemberSizingError::DegenerateSpan` |
| item | **C8b, applied to a site the canon does not list** — `MemberSpan::new` |
| bisect | restore `horizontal_mm.hypot(dz)`, leave everything else: test passes |

`MemberSpan::new`'s 3-D length was the hypot chain and is now `vec3::norm = dot(d,d).sqrt()`.
The two agree to ≤ 2 ULP everywhere a member can exist and differ **in kind** only below the
point where the *square* of the length underflows. Measured (`rustc -O`, this tree):

| `dz` | hypot chain | `vec3::norm` |
|---|---|---|
| `1e-100` | `1e-100` | `1e-100` |
| `1e-154` | `1e-154` | `1e-154` |
| `1e-160` | `1e-160` | `9.99994e-161` (subnormal square) |
| `1.5e-162` and below | exact | **`0.0`** |
| `f64::from_bits(1)` = `4.94e-324` | `4.94e-324` | **`0.0`** |

`PositiveFinite::new(0.0)` then refuses.

**What was done, and why.** The clamp claim is unchanged, so the test keeps it and re-reaches
it with a `1e-100` mm span — still 94 orders of magnitude below the 1e-6 mm clamp floor and 98
below the printer's 14 um pixel, so it exercises the same branch. A **new** test,
`a_span_whose_squared_length_underflows_is_refused_not_sized`, pins the moved boundary in both
directions so it cannot move again unnoticed. No assertion was weakened.

**Flagged, not settled.** `member_sizing` is not one of canon §1 OP-6's three ruled `hypot`
sites, and turning an answer into a refusal is outside canon §3's C8b bound of "≤ 2 ULP,
≤ 1.421e-14 mm". It is kept rather than reverted for a measured reason: the consolidated
formula is proved byte-neutral against the corpus control (§5) and the reverted one is not.
It needs a canon amendment — §8 item 2.

### What did NOT move

Every other pinned expectation in the tree held, including the corpus-model digests in
`plinth-decimate/tests/corpus_budgets.rs`, `plinth-mesh/tests/topology_triage.rs`,
`plinth-mesh/examples/corpus_gate.rs`, the zero-field-divergence fixture, the six support
ratchets in `plinth-support/tests/ratchet_harness.rs`, and the thread-sweep determinism checks.
C1 (`plinth-emit`), C2 (2D polygon primitives) and C8a were byte-neutral as the plan predicted:
**zero digests moved for them.**

---

## 4. Build, gates and test state

| gate | result |
|---|---|
| `RUSTC_WRAPPER="" cargo build --release --workspace` | **exit 0** (52m 14s; thin LTO) |
| `cargo test --workspace --offline --no-fail-fast` | **121 binaries, 1,638 passed, 1 failed, 7 ignored** |
| `cargo test --workspace --offline --no-fail-fast --all-features` | **121 binaries, 1,643 passed, 1 failed, 7 ignored** — same single failure |
| `cargo clippy --workspace --all-targets --all-features --offline -- -D warnings` | **exit 0, 0 warnings** — after the fix in bucket 1 below |
| `cargo fmt --all -- --check` | **7 diff locations in 2 files** — exactly the `docs/development.md` baseline, both pre-existing (`plinth-hollow/examples/wall_distribution.rs` ×6, `plinth-hollow/tests/repair_gate.rs` ×1) |
| `git diff --check` | clean, exit 0 |
| `plinth-support` lib tests | **660 passed, 0 failed** (`docs/development.md` records 653; +7 are this sweep's new tests) |

Both test invocations skip `a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule`;
see bucket 3.

The release build emits **5 `dead_code` warnings**, all in `rust/geom_kernel`
(`bvh::build_attempt_count`, `mesh::shares_geometry`, `parity::referenced_face_ids`,
`parity::axis_build_attempt_count`, `queries::seg_surface_min`). All five are called only from
`testkit.rs`, which is `#[cfg(feature = "testkit")]`; none is touched by this diff; and **the
pristine `814de04` control build emits the identical five**. Pre-existing, and the reason the
documented gate runs clippy and one test pass with `--all-features`.

### The three-way split, in full

**Bucket 1 — defects introduced by the consolidation: one, fixed here.** The sweep's new tests
added **seven `clippy::float_cmp` errors** — three in
`plinth-hollow/src/drains.rs::normalize_uses_the_canonical_reciprocal_spelling_not_componentwise_division`,
one in `plinth-support/src/dyadic.rs::a_degenerate_face_has_no_normal_at_all`, one in
`plinth-support/src/joints.rs::a_directionless_vector_refuses_instead_of_returning_nan`, and two
in `plinth-support/src/perception.rs`'s two `poly2` degeneracy tests. `docs/development.md`
records the clippy gate as **0 warnings**, held by the named-lints rule, so this was a real
break: `cargo clippy --workspace --all-targets --all-features -- -D warnings` exited 101 with
`could not compile plinth-hollow (lib test)` and `plinth-support (lib test)`.

Every one of the seven is a *legitimate* exact pin — each test's whole claim is that specific
bits do or do not match — so the fix is the project's own convention and not a weakened
assertion: `#[expect(clippy::float_cmp, reason = …)]` at each site, naming why the exactness is
the claim and citing `docs/evidence/remediation-2026-08-26/census/cast-audit.md`, exactly as
`plinth-layers/tests/registry_pin.rs` already does. Clippy is back to **exit 0, 0 warnings**.

Nothing else in bucket 1: the four test failures on the first integration run are all bucket 2,
each proved by bisect.

**Bucket 2 — expectations that legitimately moved: four.** §3.1 (C4, one golden), §3.2–3.3
(C7, three digests in two tests), §3.4 (C8b at an unruled site, one test repaired plus one new
boundary pin). Every one carries old value, new value, the item responsible, the bisect that
proves the attribution, and why the new value is correct.

**Bucket 3 — pre-existing failures, unrelated to this change: two, both already documented.**

1. `plinth-support/tests/support_runner_integration.rs::closure_repairs_censored_dust_because_that_repair_closes_a_real_hole`
   — a deliberate red that reports FINDING 1, a live support-planning defect.
   `docs/development.md` records it as "VERIFIED unchanged at `5d0420e` … identical panic,
   identical `candidates_tried: 274`". It failed here with `candidates_tried: 274`.
   **Unchanged.**
2. `…::a_spec6_removal_discloses_its_ownerless_islands_and_lets_the_measurement_rule` — the
   uncapped-trial-count test `docs/development.md` records as never having been observed to
   completion ("887 trials over 47 minutes … its verdict is unmeasured, before this change and
   after it"). It ran here to **877 trials / 2 commits / 2,558 remaining in 2,446 s** and did
   not terminate; the reported suite totals are with it skipped. **Still unmeasured, in both
   directions — not a pass.**

---

## 5. The corpus

### 5.1 The instrument, and what makes it trustworthy here

`tools/plinth-audit` was built out of tree and hashes to
`6ecfde17890e08d0b87b25ed70e580f0114a60dc7c3750a2e14c81911447ac75` — **bit-identical to the
binary named in [`audit-baseline-round3.md`](audit-baseline-round3.md)**. There is no
tool-version term in any comparison below.

The harness was gated before it was trusted: re-auditing round 3's own 31 artifacts with it
reproduces round 3 **exactly** — 954 failed I7 islands over 1,998 examined, 360 failed I9
regions over 1,462,004, I10's 21,663 evaluations and 7,439 refusals, 12 total gate-passes,
0 files passing all ten, and the I7 size distribution (520 one-pixel, 169 two-pixel, 26
three-pixel, 715 ≤ 3 px = 74.9 %, 201 > 10 px) cell for cell.

Engine binary provenance was asserted, not assumed, in both directions:

- **before the run**, **0** source files under `crates/` or `rust/` postdated
  `target/release/plinth` (sha256
  `0db4736f26c412ab1c0737afae455b33835d1f50588e9b5b5ddb68990154b8e4`);
- **after** every repair in §3 and §4 was applied, the workspace was rebuilt `--release` and the
  binary hashes to **the same `0db4736f…`**. So every post-run edit landed inside `#[cfg(test)]`
  or a test file, and the artifacts below were produced by the source being committed. That is
  checked, not argued — this project has been burned by a stale binary faking a lane's result.

### 5.2 Dispositions — the 24 round-3 emitting miniatures

**24 of 24 emitted.** Zero disposition regressions. `timeout -k 15 1500` per model, three
concurrent, treant excluded. No model hit the ceiling; longest 1,121 s (`Crawler Zombie`).

The 13 models that did not emit at round 3 were **not re-run** — the brief scopes the run to
round 3's set. A model that newly *started* emitting would therefore not appear here. The
verdict tables in §6 cover the admission question for the eight corpus fixtures; for the
thirteen real miniatures it is an unmeasured blind spot, named in §8.

### 5.3 The comparison that carries information

Round 3's artifacts were produced on **2026-08-25 from `develop @ 5d546cb` with a `gate`-profile
binary**. `git diff --stat 5d546cb..814de04 -- crates/ rust/` is **170 files, +18,295 −19,608**.
A round-3 delta therefore contains six days of unrelated engine work *and* this sweep, and
**cannot be attributed to the consolidation.** That is not a hedge: the pristine `814de04`
control refuses `make_raking_blade` at the pre-write coverage gate, where round 3 has an
artifact for it.

So a control was built: the same tree at `814de04` with **none** of the sweep, `git archive`d,
built `--release`, and run on the seven frozen-digest corpus models plus eight miniatures
(binary sha256 `012e49da…`).

> ### **14 of 14 artifacts byte-identical, control versus consolidated.**
>
> `cylinder`, `make_box`, `make_overhang_table`, `make_thin_fin`, `make_toed_slab`, `sphere`
> (`make_raking_blade` refuses in **both**, identically), and `Bat Cat Owl`, `Myconid Adult`,
> `Round 25`, `Round 50`, `Round 100`, `Tavey Badger`, `zombie-1-a`, `zombie-5-a`.
>
> `zombie-5-a` carries 412 support records and `zombie-1-a` 300, so this is not a smoke test on
> trivia. **The consolidation moved zero emitted bytes on every model where a control exists**,
> C3, C4, C5, C6, C7 and C8b included.

### 5.4 Against round 3 — and which half of it means anything

31 files before, 30 after (`make_raking_blade` no longer emits at `814de04`). Byte comparison:
**19 identical, 11 moved, 1 gone**.

Of the 11 movers, **7 are in the control set and were byte-identical there** — `cylinder`,
`make_box`, `make_overhang_table`, `make_thin_fin`, `make_toed_slab`, `sphere`, `zombie-5-a`.
Their movement is `5d546cb → 814de04`, measured, not inferred. The other four
(`Decomposing Zombie`, `Plague Zombie`, `Skeleton_2`, `Tavey Base`) have no control and cannot
be split.

Per-invariant totals, round 3's 31 files against this run's 30:

| invariant | round 3 (31 files) | now (30 files) |
|---|--:|--:|
| I1 seat depth | 848 f / 868 i of 1,587 | 838 f / 878 i of 1,608 |
| I2 contact area | 464 f / 1,063 i of 1,587 | 355 f / 1,188 i of 1,608 |
| I3 min section | 965 f / 534 i of 1,587 | 961 f / 567 i of 1,608 |
| I4 load ownership | 3 f / 27 i of 31 | 3 f / 26 i of 30 |
| I5 closed load path | 60 f / 2,519 i of 9,798 | 82 f / 2,531 i of 9,735 |
| I6 no intersection | 920 f / 56 i of 9,798 | 1,125 f / 46 i of 9,735 |
| I7 island coverage | 954 f of 1,998 | 932 f of 1,966 |
| I8 no dead geometry | 3,392 f / 1,134 i of 9,798 | 3,225 f / 1,112 i of 9,735 |
| I9 unsupported span | 360 f of 1,462,004 | 330 f of 1,468,181 |
| I10 arrangement spread | 7,439 f / 5,878 i of 21,663 | 7,419 f / 5,820 i of 21,362 |
| **total gate-passes** | **12** | **11** |
| **files passing all ten** | **0 of 31** | **0 of 30** |

One per-file gate count changed: `sphere-first-artifact` 2/10 → 1/10. **Its artifact is
byte-identical between the control and the consolidated build**, so that gate loss is
`5d546cb → 814de04`, not this sweep.

**Not one number in that table is attributable to the geometry consolidation**, and the
control in §5.3 is the reason we can say so rather than guess.

---

## 6. Reading the audit honestly

Round 3 established three defects in the instrument itself. This comparison inherits all
three, and the following deltas therefore **carry no information**:

- **I8 has never passed any file, including hand-built correct ones.** It reports 100 % of
  non-TIP records dead on a defect-free fixture and 40 % on this corpus, so a *lower* number
  can mean a *worse*-built file. The 3,392 → 3,225 move is unreadable in either direction.
- **520 of round 3's 954 I7 failures are single pixels** (0.000266 mm² each), fragments the K1
  corner rule creates and K4 correctly declines to censor. Measured again here: **the one-pixel
  count is 520 in both runs**, and ≤ 3 px is 715 of 954 before against 710 of 932 after. The
  I7 delta lives in the > 10 px band (201 → 184) on files that are not byte-comparable.
  `raster.rs`'s own `the_one_pixel_censor_is_a_strict_boundary` still asserts the opposite of
  what the check emits.
- **I3/I8 and I5 disagree about what a joint is.** `SupportGraph` links records whose declared
  points coincide within `JOIN_TOL_EMITTED_MM = 1e-3`; the physics spec prescribes a ≥ 0.10 mm
  *overlap*, 100× looser, so I3 and I8 report `path_open` / `path_indeterminate` on joints I5
  reads as connected. Their counts measure that disagreement, not the file.

Also inherited: **I4 clause (d)'s absolute 1e-9 mm² floor does not scale with contact count**
(it is 769× tighter than one f32 ULP of one tip radius), and **audit-connectivity site 6,
`intersection.rs::components_26`, was ruled in at +67 records and ×69.4 breach volume and never
implemented** — so I6's 920 → 1,125 move is being measured by a check its own spec says is
incomplete.

**The comparisons in this document that do carry information** are the byte identity of §5.3
(a control, same commit, same profile, same machine), the dispositions of §5.2, the four
attributed expectation moves of §3, and the guard evidence of §7. Everything in the §5.4 table
is context, not a finding.

---

## 7. Both guards fire, on the real tree

No planted violation survives anywhere in `crates/` or `rust/`. Both guards prove they *can*
fail on every run rather than having been demonstrated once.

**Guard 1 — the import rule** (`rust/geom_kernel/tests/kn_import_rule_guard.rs`, 3 tests,
green). Every public kernel surface must have a consumer outside the kernel. Measured before
the sweep (census §2) against measured now:

| surface | consumers before | consumers now |
|---|--:|--:|
| `geom_kernel::vec3` | 1 — and it was a doc comment complaining about the gap | **28 files** |
| `geom_kernel::poly2` | did not exist | **4 files** |
| `geom_kernel::par` | 0 | **10 files** |
| `ray_triangle` (OP-2 survivor) | **never public in the crate's history** | **1 file** |
| `segment_segment_distance` (OP-1 survivor) | **never public in the crate's history** | **1 file** |
| `geom_kernel::testkit` | — | 1 file |

The assertions the consolidation was supposed to make non-zero are non-zero. The walker is
gated on scanning > 50 files, and two sibling tests prove the detector reports zero for a
symbol that does not exist and non-zero for one that does, and that no bare needle collides
with an unrelated definition.

**Guard 2 — no hand-written geometry outside the kernel**
(`rust/geom_kernel/tests/kn_no_inline_geometry_guard.rs`, 3 tests, green). Four ast-grep
structural patterns over every crate's `src/`:

```
Guard 2: scanned 91 .rs files across 13 src roots; 20 raw pattern hits
         -> 20 test-code (MAY-STAY), 0 GEOM-EXEMPT marked, 0 violations
```

All 20 hits are inside inline `#[cfg(test)]` modules — deliberate scalar oracles in
`joints.rs`, `contact.rs`, `grid.rs`, `drains.rs` and `hollow/lib.rs` that must stay
independent of production. Its self-test writes a synthetic fixture with an unmarked dot
product, an unmarked one under a single-line marker, one under a multi-line marker block, and
one inside a test module, runs the **real** extraction script and the **real** classifier over
it, and asserts the unmarked production site is reported as a violation. The extraction script
refuses rather than reporting green if `ast-grep` or `jq` is missing, and fails if it walked
zero files.

**Per-crate ratchets** (canon §2 "Enforcement"): 10 crates, 13 tests, all green. Every one
gates on a non-zero scanned-file count. `plinth-support`'s additionally requires
> 12,000 production lines scanned and asserts that **every one of its 11 allowlist entries
matched something**, so a stale exemption fails loudly.

**The audit tool's isolation held.** `tools/plinth-audit/Cargo.toml` gained no `geom_kernel`
and no `plinth-*` dependency; nothing under `tools/plinth-audit/` was touched by this change.
Its own suite: see §8 item 5.

### The C5 prerequisite, re-derived rather than trusted

Canon §3 makes a captured triage-verdict table a **hard prerequisite** for C5. Both tables are
committed under [`geometry-consolidation/`](geometry-consolidation/). Re-running
`cargo run --release -p plinth-mesh --example triage_verdict_table` on the integrated tree
reproduces the committed **after** table exactly — all 8 corpus rows, all 8 nested-shell probe
rows, every digest, every count — **and it is identical to the committed BEFORE table too**.
The probe section is what makes that a real negative rather than an empty one: every plain
corpus row has `classifiers = 0`, so the corpus as shipped never reaches the ray caster, while
the probes nest a 2 mm box in each model and drove **1,659,744 measured ray/triangle
evaluations against real corpus triangles** (1,642,722 on the Weeping Treant alone) with
unchanged verdicts, hit counts and miss counts. C5's four changed acceptance rules do not flip
one of 1.66 M face verdicts on this geometry. They are narrow by construction and this corpus
does not reach them; the rules themselves are pinned by the four unit tests in `triage.rs` and
by `canon_contracts.rs`.

---

## 8. What a crate agent flagged, and what is still open

The first three are agent flags carried up rather than improvised on. All five are open.

1. **`perception.rs`'s 2-D dot and 2-D perp-dot have no survivor to consolidate to.**
   `vec3` is 3-vector algebra on `[f64; 3]`; `poly2` publishes only `signed_area`,
   `point_in_polygon`, `segments_intersect` and `point_segment_distance`. The value reaches a
   verdict (the winding vote), which makes it MUST-CONSOLIDATE by canon §2 step 2(a) with
   nothing to consolidate to. The agent marked it and reported it rather than inventing a
   home. `brace.rs`'s 2-D `dx.hypot(dy)` cross-link spacing is the same gap.
   **Needs: a `poly2` 2-D vector section, or a canon sentence saying 2-D scalar algebra is out
   of scope.**

2. **The `x / n` versus `x * (1.0 / n)` split (§2).** Three crates adopted `vec3::normalize`
   and one declined it, each citing the canon. **Needs one canon sentence.** Until then the
   tree has two spellings of "normalize" — which is the exact condition this sweep exists to
   remove.

3. **`member_sizing`'s C8b extension (§3.4)** turns a subnormal span from an answer into a
   refusal, at a site the canon does not list and outside the bound it states.
   **Needs a canon amendment or an owner ruling.**

4. **The kernel exempts itself from its own rule.** `rust/geom_kernel/src/segment_batch.rs` is
   **untouched** by the sweep and still carries a private `fn dot` (canon §1 OP-9 names it in
   the C8a batch), plus `accepted_norm` spelled `(x.powi(2) + …).sqrt()` — a **fourth** norm
   spelling the census never recorded — beside `joint_norm`'s expanded form. Guard 2's scan
   roots are `crates/*/src` and `rust/chitubox_io/src`; `rust/geom_kernel/src` is excluded by
   design, so nothing ratchets this. Canon §4.2 also asked for a test asserting the two
   `Association` arms still differ (24 % of rows, ≤ 366 ULP), so a future "tidy-up" fails
   loudly. **That test was not written.**

5. **`tools/plinth-audit`'s own suite has one failure, and it is not geometry.** Measured:
   **237 lib tests pass; `tests/fixtures.rs` is 7 passed / 1 failed** in 752 s. The failure is
   `discovery_reaches_every_fixture_directory_on_disk`, and its own message says why:

   > `DISCOVERY GATE FAILED -- 1 fixture directory exists on disk that discovery never
   > reached: [".claude"].`

   An **agent harness scratch directory**, `tools/plinth-audit/fixtures/.claude/.cc-writes`,
   was created at 15:25 on 2026-08-31 inside the fixture tree and is untracked. Expectation
   discovery is otherwise unchanged from round 3 — 15 real fixture directories reached, 60
   expectation files found, the same per-directory counts. Nothing here is caused by, or
   fixable inside, the geometry consolidation, and `tools/plinth-audit/` is out of this
   change's ownership permanently (canon §4.1), so it was left exactly as found. It is worth
   noting for its own sake: a *tooling* side effect made a fixture gate fail, which is the
   gate working.

   Separately, and also **not part of this change**: the working tree carries another
   workstream's uncommitted edits under `tools/plinth-audit/` — a second `[[bin]]`
   (`chitubox-to-stl`) and regenerated fixture STLs, timestamped 15:43–16:04 on 2026-08-31.
   Those 17 paths are **excluded from this commit**.

6. **The 13 non-emitting miniatures were not re-run** (§5.2), so a model that newly started
   emitting under C3/C5 would not have been seen.

7. **`make_overhang_table-first-artifact` hit the 1500 s audit ceiling on the first pass**
   (round 3 measured it at 118 s) purely from machine load, and was re-audited alone
   afterwards. Its numbers are in the tables; the ceiling was a scheduling artefact, not a
   finding.

---

## 9. Reproducing this

```sh
cd plinth-3d
RUSTC_WRAPPER="" cargo build --release --workspace
RUSTC_WRAPPER="" cargo test --workspace --offline --no-fail-fast -- \
    --skip a_spec6_removal_discloses          # the documented non-terminating test
RUSTC_WRAPPER="" cargo test --workspace --offline --no-fail-fast --all-features -- \
    --skip a_spec6_removal_discloses
RUSTC_WRAPPER="" cargo clippy --workspace --all-targets --all-features --offline -- -D warnings
cargo fmt --all -- --check                    # expect 7 locations, 2 files
git diff --check

# the C5 prerequisite, re-derived
RUSTC_WRAPPER="" cargo run --release -p plinth-mesh --example triage_verdict_table

# the guards, with their evidence printed
RUSTC_WRAPPER="" cargo test -p geom_kernel --test kn_import_rule_guard -- --nocapture
RUSTC_WRAPPER="" cargo test -p geom_kernel --test kn_no_inline_geometry_guard -- --nocapture
```

The attributable control is a `git archive` of `814de04` into a scratch directory, `corpus/`
copied in (it is gitignored), built `--release`, then
`plinth prepare --input-models-dir <one model> --output saturn4-ultra --out-dir <dir> --no-cache`
per model under `timeout -k 15 1500`, and
`cargo run --release --example first_artifact -p plinth-pipeline -- --all` for the exports set.
Artifacts are compared with `cmp`. Audit runs are
`timeout -k 15 1500 plinth-audit <file>.chitubox --json --out-dir <dir>`, gated on the index
row count rather than on the loop's exit code — macOS `xargs` is BSD and has no `-d`.

Run outputs are not committed.
