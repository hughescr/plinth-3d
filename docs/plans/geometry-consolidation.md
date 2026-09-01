# Duplicated geometry in the engine — census, cause, and a consolidation plan

Status: **plan, not yet executed.** Read-only survey of `crates/` and `rust/` at
`feature/stage0-bytes-audit` @ `32a5423`. No engine file was changed to produce it.

Audience: the owner, who asked *"I thought we have a single geometry crate that does all the
geometry in a shared way to ensure that it's consistent? Is that not true? Why do we have 4
separate geometry models??"*, and whoever executes the sweep afterwards.

**Every number below was computed**, either by `grep`/`git log` over this tree or by a
measurement harness that re-implements each in-repo variant verbatim and runs 2,000,000
random inputs through it. Where a claim is a judgement rather than a measurement it says so.

---

## 0. The short answer to the question that started this

`rust/geom_kernel` **is** the single home, and it is a good one. It is not the problem.

The problem is that it is a home nobody was made to live in. Thirteen files outside the kernel
carry their own private 3-vector algebra; four geometric *predicates* exist in two or three
independent versions with different tolerance rules; and two of the kernel's own primitives —
ray/triangle and segment/segment — **have never been public in the crate's entire history**, so
a caller who wanted to reuse them could not, and wrote their own.

So the answer is not "four geometry models" but something more awkward: **one geometry model
plus twelve ad-hoc fragments of one**, and — the part worth acting on — **one measured
correctness defect** that a shared implementation would have prevented.

**The honest framing before any of it: none of this explains the headline.** 0 of 31 emitted
files passes all ten audit invariants, and the owner's own inspection says supports are too
thick, too tightly spaced, and not on the critical parts. Nothing in this document causes that.
The largest *physically meaningful* divergence found here, outside the one real bug, is
2.1 × 10⁻¹⁴ mm — thirteen orders of magnitude below the 0.3 mm hard clearance limit. This sweep
is hygiene plus one bug fix. It is not the fix for the corpus. §7 says which parts of it are
not worth doing at all.

### Corrections to the framing this task arrived with

Two claims in the brief did not survive checking, and one grew:

| claim as received | what the tree says |
|---|---|
| "`triage.rs` makes exactly one call into the kernel" | `triage.rs` makes **zero**. Its single `geom_kernel` mention (line 1453) is a comment explaining why it does *not* use the kernel. The one real kernel import in `plinth-mesh` is in a different file, `delicacy_eval.rs:70`. |
| "the kernel kept its vector maths and thread pools private until recently; they were made public; nobody switched over" | True and confirmed (`dc2e0d0`, 2026-08-26; zero consumers today). But it is **only half the cause**, and it does not explain ray/triangle at all — see §2. |
| point-to-line-segment: "at least three copies" | Three in 2D, confirmed at the exact lines given. There is also a **fourth** in 3D — the kernel's own `segment_batch.rs` — which is already correctly consolidated and carries a *deliberate* internal split. §5. |

---

## 1. The census

Ranked by whether the divergence can change an emitted file, and by whether the change is a
*category* flip (a refusal, a hit/miss) or a *magnitude* nudge in the last bits.

### Summary table

| # | operation | implementations | worst measured divergence | can it move emitted bytes? |
|---|---|--:|---|---|
| **OP‑1** | segment/segment distance, 3D | 2 | **4.49 × 10⁻² mm, wrong answer** | yes — hollow refusal |
| **OP‑2** | ray/triangle intersection | 2 | categorical: Hit ↔ Miss ↔ Ambiguous over a 10⁻⁶ band | yes — whole-model triage refusal |
| **OP‑3** | point→segment distance, 2D | 3 | 3.16 × 10⁻⁸ mm (= 3.16 slice quanta) | yes — support birth |
| **OP‑4** | face normal | 3 | FMA vs non-FMA cross | yes — coverage sampling |
| **OP‑5** | inside/outside by ray parity | 2 | whole different algorithm | yes — triage admission |
| **OP‑6** | `norm` / `normalize` | 3 formulas, 3 zero-guards | 1.42 × 10⁻¹⁴ mm; one guard returns **NaN** | bits only, except the NaN |
| **OP‑7** | `dot` / `cross` | 2 formulas (plain, FMA) | 44.76 % of inputs differ | no (the FMA site is evidence-only) |
| **OP‑8** | 2D polygon primitives | 2 copies each, **text-identical** | none | no — byte-neutral by construction |
| **OP‑9** | 3-vector algebra generally | 13 private copies | see OP‑6/7 | mostly no |

---

### OP‑1 — Segment vs segment distance, 3D. **A measured bug, not a style divergence.**

| | |
|---|---|
| canonical | `rust/geom_kernel/src/closest.rs:96` `segseg()` — **private module, never public** |
| copy | `crates/plinth-hollow/src/drilling.rs:1962` `segment_distance()` |
| consumer | `validate_final_holes` (`drilling.rs:1465`) — refuses a hollow when two drain-hole keepout axes come within `r_left + r_right + PROOF_EPSILON_MM` |

On 2,000,000 random well-conditioned segment pairs at ±10 mm the two agree to
**5.33 × 10⁻¹⁵ mm** (zero pairs differ by more than 10⁻¹²). The divergence is not in the general
case. It is in two guards.

**(a) The parallel-degeneracy guard is a units error.** `drilling.rs:1975` reads

```rust
let denom = a * c - b * b;
let mut s = if denom.abs() < PROOF_EPSILON_MM { 0.0 } else { ... };
```

`denom` is `|u|²|v|² − (u·v)² = L₁²L₂²sin²θ`, whose units are **mm⁴**. `PROOF_EPSILON_MM` is
`1.0e-7` — a **millimetre** constant (`drilling.rs:34`), used correctly as a millimetre
everywhere else in the file. Comparing them means the short-circuit fires whenever
`L₁·L₂·|sin θ| < √(10⁻⁷) = 3.162 × 10⁻⁴ mm²`, which is a threshold on *area*, not on angle.
Measured, for an equal-length pair:

| segment length | short-circuits as "parallel" for angles below |
|--:|--:|
| 1.0 mm | 0.018° |
| 0.2 mm | 0.453° |
| 0.1 mm | 1.812° |
| 0.05 mm | 7.267° |
| 0.02 mm | **52.239°** |
| 0.01 mm | **90° — always, at every angle** |

A perfectly orthogonal equal-length pair short-circuits for any `L < (10⁻⁷)^¼ = 0.017783 mm`.
Drain-hole keepout axes live at exactly that scale.

Over 2,000,000 random pairs with both segments 0.005–0.05 mm long (the hole-keepout regime),
drilling took the parallel branch on **719,573 pairs — 35.98 %** — and the resulting distance
was wrong by up to **4.494 × 10⁻² mm** absolute, **2.55×** relative. On the worst case an
independent 20,001 × 20,001 brute-force sweep of the two segments confirms which is right:

```
brute force  5.52620025428943507e-1   (at s = 1, t = 0)
kernel       5.52620025428943507e-1   bit-identical to brute force
drilling     5.97563440746216500e-1   wrong by 4.494e-2 mm
```

That error is **4.494 × 10⁵ times** the `1e-7 mm` margin of the very comparison it feeds. This
is a real defect: `validate_final_holes` can pass a pair of overlapping drain holes, or refuse a
clean one, at exactly the scale drain holes are drilled.

**(b) A zero-length segment is a refusal in one and an answer in the other.** `drilling.rs:1971`
returns `Err(InvalidHoleGeometry)` when `a <= 0.0 || c <= 0.0`; the kernel returns a finite
distance (measured: `1.0` for a degenerate segment at the origin against `[1,0,0]→[1,1,0]`).
That is a deliberate-looking choice in drilling, and it should be **preserved** by the caller,
not by the shared primitive — see §3.

---

### OP‑2 — Ray vs triangle. **Categorical divergence over a 10⁻⁶ band.**

| | |
|---|---|
| canonical | `rust/geom_kernel/src/moller.rs:13` `moller()` — **private module, never public** (`git log -S 'pub mod moller'` is empty over the whole history) |
| copy | `crates/plinth-mesh/src/triage.rs:1099` `ray_triangle_distance()` |
| consumers | `classify_ray` → `ContainmentClass` → `triage_components` → `repair.rs`, i.e. whether a model is admitted to the pipeline at all |

The 2026-08-26 review's count of four is now two: the other two died with `plinth-metrics`.

They disagree in four independent places. Measured on the unit triangle in `z = 0` with a ray
straight down:

| input class | kernel | triage |
|---|---|---|
| barycentric just outside, `u = −5 × 10⁻⁷` | **hit**, `t = 1.0` | `Miss` |
| barycentric outside kernel slack, `u = −2 × 10⁻⁶` | miss | `Miss` (agree) |
| `t = −5 × 10⁻⁷` (just behind origin) | **hit** | `Miss` |
| `t = −2 × 10⁻⁶` | miss | `Miss` (agree) |
| `t = 5 × 10⁻¹⁰` | **hit** | **`Ambiguous`** |
| interior but `v = 5 × 10⁻¹¹` from an edge | **hit** | **`Ambiguous`** |
| clean interior | hit | `Hit(1.0)` (agree) |

The four disagreeing rules, by name:

1. **Barycentric slack.** Kernel: `u ≥ −EPS`, `v ≥ −EPS`, `u+v ≤ 1+EPS` with `EPS = 1e-6`
   (`consts.rs:6`). Triage: `u ∈ [0,1]`, `v ≥ 0`, `u+v ≤ 1` — no slack at all. A 10⁻⁶-wide
   barycentric shell around every triangle is a hit to one and a miss to the other.
2. **Ray-parameter slack.** Kernel accepts `t ∈ [−EPS, t_max + EPS]`. Triage rejects any
   `t < 0`. Every `t ∈ (−10⁻⁶, −10⁻⁹)` is a kernel hit and a triage miss.
3. **Determinant guard.** Kernel: `|det| > 1e-14` (`DET_EPS`). Triage: `|det| < 1e-12 → Miss`.
   A **100×-wide band** `[1e-14, 1e-12)` where the kernel measures a hit and triage refuses to
   look.
4. **`Ambiguous` has no counterpart.** Triage's `ORIGIN_EPS = 1e-9` and `EDGE_EPS = 1e-10`
   produce a third outcome the kernel does not have. One `Ambiguous` face makes the whole
   component `ContainmentClass::Ambiguous`. This is the categorical consequence: a model is
   refused where the kernel would have counted a clean parity.

There is also a **contract** difference, not just a tolerance one. The kernel consumes the raw
stored `E1`/`E2` (Amendment A6, stated in `moller.rs:2-4`); triage reconstructs `b − a`,
`c − a` from vertices. Those differ in the last bits whenever the stored edges were not
computed by that exact subtraction.

Downstream, the two dedup crossings differently too: triage uses a *relative* `1e-9`
(`triage.rs:1069`), the kernel an *absolute* `DEDUP_LOC_MM = 3e-3` — six orders of magnitude
apart, on the same job.

---

### OP‑3 — Point to segment, 2D. **Three degeneracy rules on the same slice data.**

| where | zero-length test | threshold on `|ab|` |
|---|---|--:|
| `crates/plinth-support/src/perception.rs:2788` `point_segment_distance` | `denominator < 1.0e-15` | `< 3.162 × 10⁻⁸ mm` |
| `crates/plinth-support/src/specialized.rs:8286` `segment_distance` | `length_squared == 0.0` | `= 0` exactly |
| `crates/plinth-support/src/joints.rs:1505` `segment_distance_2d` | `l2 < 1e-18` (`DEGENERATE_LENGTH_SQUARED`, `joints.rs:22`) | `< 10⁻⁹ mm` |

The first two are the ones required to agree: `perception::point_reach_mm` /
`birth_xy_ok` (call sites `perception.rs:2694`, `2777`) and
`specialized::boundary_distance` / `point_in_solid` (`specialized.rs:8275`, `8250`) both walk
the **same `SliceSolid` loops** and both answer "is this point within reach of that boundary".
They differ by a factor of **3.16 × 10⁷** in where they decide an edge has no length.

The third, `joints.rs`, is on **different data** — an `(axial, radial)` frustum profile, not a
slice loop — so it is a third copy of the *formula* but not a third opinion about the same
input. Worth stating precisely, because it changes what "must agree" means for it.

**Measured.** Over 2,000,000 random near-degenerate segments (length log-uniform in
`[10⁻¹², 10⁻⁶]` mm):

- max `|perception − specialized|` = **3.1586 × 10⁻⁸ mm**, at segment length 3.159 × 10⁻⁸ mm
- max `|perception − joints|` = **3.1586 × 10⁻⁸ mm**

Worked case — a segment of **3 slice quanta** (`3 × SLICE_UNION_QUANTUM_MM` = 3 × 10⁻⁸ mm,
`plinth-layers/src/lib.rs:69`) along `+x` from the origin, queried at `(1, 0)`:

```
perception   1.00000000000000000e0     (calls the edge a point, measures to A)
specialized  9.99999969999999960e-1    (projects onto the edge)
joints       9.99999969999999960e-1
```

**Why that is reachable rather than theoretical.** `plinth-layers` quantizes slice vertices to a
`1e-8 mm` lattice and `perception`'s `point_key` rounds identically (`plinth-layers/src/lib.rs:
276`). An edge of 1–3 quanta survives that quantization as two *distinct* lattice points — the
union normalizer treats it as a real edge — while `perception` treats it as a point. The two
stages disagree about whether a thing exists.

**But be honest about the size.** The value divergence is bounded by the segment length, so at
most 3.16 × 10⁻⁸ mm. It flips a support-birth decision only when the reach comparison sits
within 3 × 10⁻⁸ mm of its threshold. This is a *consistency* defect worth closing, not a
mechanism that puts supports in the wrong place.

---

### OP‑4 — Face normal. **Three formulas, one of them FMA.**

| where | cross product | length | degenerate face |
|---|---|---|---|
| `rust/geom_kernel/src/mesh.rs:301-359` | plain | `sqrt` of sum | typed error, refuses zero area |
| `crates/plinth-mesh/src/lib.rs:235` `face_normals_z` | plain | `.map(v*v).sum::<f64>()`, then `sqrt` | `MeshError::DegenerateTriangle` |
| `crates/plinth-support/src/dyadic.rs:799` `face_normal` | **`mul_add` (FMA)** | `dist_sq(n,[0;3]).sqrt()` | `Option::None` |

The first two are bit-identical in the cross and the length (an iterator `sum` over three
contiguous elements associates left-to-right, exactly as `dot` does — confirmed by measurement:
`dot(a,a).sqrt()` and `(x*x+y*y+z*z).sqrt()` differed on **0 of 2,000,000** inputs).

`dyadic.rs` is the outlier, and it is the one that matters: it feeds `build_rows` →
`coverage.rs:31` → which islands are demanded → which supports are born. `geom_kernel/src/vec3.rs`
states the house rule at the top of the file — *"no FMA, no reassociation (spec sec 5.1)"* —
and `dyadic.rs` breaks it inside the coverage sampler.

---

### OP‑5 — Inside/outside by ray parity. **Two entirely different algorithms.**

- `rust/geom_kernel/src/parity.rs` — a per-axis CSR grid, ported verbatim from the retired
  oracle, cluster-count dedup at `ZDEDUP = 5e-3`. **Private module** (`lib.rs:24`).
- `crates/plinth-mesh/src/triage.rs:1043` `classify_ray` — a linear scan over every face, an
  even-odd count, relative `1e-9` dedup, and a third `Ambiguous` outcome.

Not a tolerance divergence. A different algorithm answering the same question, with different
work complexity (`O(faces)` vs indexed) and a different failure vocabulary. Both decide whether
a component is inside a solid.

---

### OP‑6 — `norm` and `normalize`. **Three formulas; three zero-guards, one of which is NaN.**

`norm`, three formulas:

| formula | sites |
|---|---|
| `dot(a,a).sqrt()` | `geom_kernel/vec3.rs:41`, `drilling.rs:1944`, `rewire.rs:1431`, `brace.rs:3236`, `emit/lib.rs:1029` (`length`) |
| `(x*x + y*y + z*z).sqrt()` | `delicacy_eval.rs:417`, `orient/score.rs:1151`, `joints.rs:1572` |
| `x.hypot(y).hypot(z)` | `anchor.rs:2085`, `cco.rs:5846`, and `drains.rs:2076` inside `normalize` |

Measured over 2,000,000 random vectors in a 60 mm cube (a full plate):

- formula 1 vs 2: **0 differences.** Bit-identical. Consolidating them is free.
- formula 1 vs 3: **401,523 differ (20.08 %)**, max **2 ULP**, max absolute difference
  **1.421 × 10⁻¹⁴ mm**.

The 0.6 mm placement margin is 4.22 × 10¹³ times that difference; the 0.3 mm hard clearance
limit is 2.11 × 10¹³ times it. **`hypot` cannot move a clearance decision by magnitude.** It
moves bits, and bits move digests. That is the entire risk, and it is worth saying plainly
rather than dressing up.

`hypot` *is* semantically better at the extremes — measured, `[1e200; 3]` gives `inf` from
`dot.sqrt` and `1.732e200` from `hypot`; `[1e-200; 3]` gives `0` and `1.732e-200`. Those
magnitudes are unreachable for a mini on a 218 × 123 mm plate, so this is a real property with
no application here.

`normalize`, three zero-guards — this one is not cosmetic:

| where | guard | result on `[0,0,0]` |
|---|---|---|
| `drilling.rs:1947` | `!n.is_finite() \|\| n <= 0.0` | `Err(InvalidHoleGeometry)` |
| `drains.rs:2075` | `!length.is_finite() \|\| length == 0.0` | `Err(NonFinite)` |
| `joints.rs:1575` | **none** — `scale(a, 1.0 / norm(a))` | **measured: `[NaN, NaN, NaN]`** |

`joints::normalize([1e-200; 3])` measures `[inf, inf, inf]`. A NaN normal propagates silently
through every comparison it touches — every `<` and `>` against it is `false`, so a guard
downstream reads "not violated". This is exactly the failure class the project keeps
reproducing: **a value meaning "measured nothing" read as a pass.**

---

### OP‑7 — `dot` and `cross`: plain versus FMA

`geom_kernel/src/vec3.rs:25` and eleven copies use left-to-right `a0*b0 + a1*b1 + a2*b2`.
`crates/plinth-pipeline/src/coverage_evidence.rs:927` uses
`left[0].mul_add(right[0], left[1].mul_add(right[1], left[2]*right[2]))` — **both** fused and
right-associated, two simultaneous violations of the rule stated at the top of `vec3.rs`.

Measured over 2,000,000 random pairs in a ±10 cube: **895,119 differ — 44.76 %** — max
**2,969,542 ULP** (the ULP figure blows up precisely on the near-cancelling dot products
that orientation tests care about, which is the point).

**It cannot move a byte, and that is documented.** `coverage_evidence.rs:1-14` states the ruling:
all three evidence forms are *"evidence only, and never a gate input"*, the module *"is never
imported by `gates.rs`"*, and its `dot` is used at exactly two call sites (`:996`), both inside
the SVG projection. Ranked accordingly in §7.

---

### OP‑8 — 2D polygon primitives: duplicated, and **text-identical**

- `point_in_polygon`: `perception.rs:1613` and `specialized.rs:8261` — **`diff` is empty**
  modulo the loop-variable name (`i` vs `index`). A third public copy lives in
  `plinth-layers/tests/common/mod.rs:56`, doc-commented *"the perception stitcher's exact
  formula"*.
- `signed_area`: `perception.rs:1600` and `plinth-layers/src/lib.rs:292` — **`diff` is empty**,
  byte for byte.
- `segments_intersect`: `perception.rs:1627`, one copy only.

`plinth-layers/src/lib.rs:283-291` already records the whole problem in a doc comment, and it is
the most useful sentence in the tree on this subject:

> *"...intended to match `plinth-support::perception`'s private `signed_area` byte for byte, but
> nothing enforces that: the two are separate hand-copies with no shared implementation or
> cross-crate pin test. `geom_kernel::vec3` (public since this note was written) does not close
> this gap — it is 3-vector arithmetic on `[f64; 3]`, not the 2D polygon primitives... Moving
> those to one shared home with one named degeneracy tolerance is tracked as the standing
> duplication-consolidation sweep, not done here."*

That note is correct on every point, including the one people will get wrong: **opening `vec3`
did not and could not close the 2D gap.** The kernel has no 2D polygon module at all.

---

### OP‑9 — Private 3-vector algebra, thirteen sites

`grep -rln 'fn sub(|fn subtract(|fn dot(|fn cross(|fn norm(|fn length('` over `crates/*/src/`
and `rust/*/src/` returns fourteen files. One is the canonical `geom_kernel/src/vec3.rs`. The
other thirteen:

```
rust/geom_kernel/src/segment_batch.rs:665   (a private dot INSIDE the kernel itself)
crates/plinth-emit/src/lib.rs:1010-1030     add sub scale dot cross length
crates/plinth-hollow/src/drains.rs:2085     normalize add sub cross scale
crates/plinth-hollow/src/drilling.rs:1925   add sub scale dot cross norm normalize
crates/plinth-mesh/src/delicacy_eval.rs:401 sub dot cross norm
crates/plinth-mesh/src/triage.rs:1223       subtract cross dot
crates/plinth-orient/src/score.rs:1139      subtract cross norm
crates/plinth-pipeline/src/coverage_evidence.rs:927  dot (FMA)
crates/plinth-support/src/anchor.rs:2076    sub add scale norm(hypot)
crates/plinth-support/src/brace.rs:3224     sub scale dot norm
crates/plinth-support/src/cco.rs:5834       sub add scale dot norm(hypot)
crates/plinth-support/src/joints.rs:1560    add sub scale dot norm normalize angle
crates/plinth-support/src/dyadic.rs:781     dist_sq + an inline FMA cross
```

Plus three outside `src/`: `plinth-pipeline/examples/fu61_bore_penetration.rs:1079`,
`plinth-solid/tests/solid_spike.rs:121`, `plinth-layers/tests/common/mod.rs`.
Plus `tools/plinth-audit/src/vec3.rs` — **deliberate, and protected**; see §5.

**The one worth naming for its blast radius:** `plinth-emit`'s copy is what builds every emitted
triangle. `frustum_triangles` (`emit/lib.rs:768`) calls `sub`, `length`, `scale`, `add`, `cross`
to generate the ring vertices of every frustum, ball, and raft pad written into the `.chitubox`
file. Every vertex in the corpus passed through it. Its formulas are **bit-identical to the
kernel's**, so consolidating it is provably byte-neutral — which makes it the safest item in
this document and the natural pilot.

---

## 2. Why it happened — the private-kernel theory, tested

**Theory as received:** the kernel kept `vec3` and `par` private, so callers could not reach
them and copied instead; they were opened; nobody switched.

**What the history says.** `pub mod vec3` and `pub mod par` land in exactly one commit —
`dc2e0d0`, **2026-08-26**, *"Open geom_kernel's vec3 and par modules, gate testkit behind a
feature"*. First appearance of each private copy, by `git log -S` on the function text:

| copy | first appears | `geom_kernel` was a dependency of that crate from |
|---|---|---|
| `drilling.rs`, `drains.rs` | 2026‑08‑01 | 2026‑08‑01 (`c3be3ca`) |
| `triage.rs` | 2026‑08‑01 (`9f24937`) | **2026‑08‑10** — nine days later |
| `anchor.rs` | 2026‑08‑01 (`fbd4672`) | 2026‑08‑01 (`d6d828d`) |
| `emit/lib.rs` | 2026‑08‑01 (`1017fe8`) | **2026‑08‑09** |
| `perception.rs`, `specialized.rs`, `joints.rs`, `cco.rs`, `brace.rs` | 2026‑08‑02 | 2026‑08‑01 |
| `orient/score.rs` | 2026‑08‑10 (`9c1030c`) | 2026‑08‑10 — **the same commit** |
| `delicacy_eval.rs` | 2026‑08‑10 (`17880ae`) | 2026‑08‑10 — **the same commit** |
| `coverage_evidence.rs` | 2026‑08‑12 (`6ddf051`) | **never** — `plinth-pipeline` has no `geom_kernel` dependency to this day |

**Verdict: the theory holds, for about two thirds of the sites, and it is not the whole story.**

Three distinct causes, and they need three different fixes:

**Cause A — the module was private (the stated theory).** `plinth-support` and `plinth-hollow`
had `geom_kernel` as a dependency from day one and still wrote their own `sub`/`dot`/`norm`,
because `mod vec3;` made them unreachable. `orient/score.rs` and `delicacy_eval.rs` are the
cleanest evidence: both added the `geom_kernel` dependency **and** a private `cross`/`norm`
block in the same commit. Someone reached for the kernel, found the vectors locked, and typed
them out. That is the theory, confirmed at commit granularity.

**Cause B — no dependency edge existed yet.** `triage.rs` (2026‑08‑01) predates
`plinth-mesh`'s kernel dependency by nine days; `emit/lib.rs` predates its own by eight.
Privacy was irrelevant — there was nothing to import from. `plinth-pipeline` still has no
`geom_kernel` dependency at all, so `coverage_evidence.rs` had no option either.

**Cause C — and this one is still live.** `moller`, `closest`, and `parity` are **private
today**, and `git log -S 'pub mod moller'` / `-S 'pub mod closest'` over the whole history return
**nothing**. They have never been public. `dc2e0d0` opened `vec3` and `par` only, and its diff
touches four files — it was done to gate the `testkit` feature, not as a consolidation step. So
OP‑1, OP‑2 and OP‑5 — the three highest-ranked items in the census — **cannot be consolidated
by "switching over" at all.** They need new public API in the kernel first.

**Why nobody switched after 2026‑08‑26.** Measured: `grep -rn 'geom_kernel::vec3\|vec3::'` over
`crates/` returns exactly **one hit**, and it is the doc comment in `plinth-layers` complaining
about the gap. Zero code consumers. In the 20 commits since `dc2e0d0` there is no consolidation
commit. The opening was a side effect of a testkit change, announced nowhere, with no follow-up
task — so it changed what was *possible* and nothing about what was *done*.

**The durable fix, therefore, is not "open more modules."** It is:

1. Publish the three primitives that are still private (Cause C), because no amount of
   discipline closes a duplication whose canonical form is unreachable.
2. Add the missing dependency edges, or accept the copies where the edge is deliberate
   (Cause B — `plinth-pipeline`'s isolation from the kernel may well be worth keeping; see §5).
3. Make the copies **fail the build**, not merely be discouraged (Cause A). A `clippy.toml`
   `disallowed-methods` entry, or a `tests/no_private_vector_algebra.rs` grep-test in each crate
   that lists its permitted exceptions by name and line. Without that, this document's sweep
   decays at the same rate the last one did.

---

## 3. The consolidation plan

**Read this first: every item below except C1 and C2 changes program behaviour.** Different
epsilon rules mean different answers on edge cases; different answers move emitted bytes; moved
bytes break every digest pinned to them. That is not a risk to be mitigated, it is the
definition of what is being done. The plan is ordered so the items that change bytes come only
after the item that proves the harness can tell.

### Ordering

| stage | item | changes bytes? | why here |
|---|---|---|---|
| **C1** | `plinth-emit` → `geom_kernel::vec3` | **no — provably** | the pilot: proves the mechanism on the most byte-critical file with zero byte movement |
| **C2** | 2D polygon primitives → one shared home | **no — text-identical** | second free item; closes the gap the `plinth-layers` note names |
| **C3** | OP‑1 drilling `segment_distance` → kernel `segseg` | **yes** | the only measured *bug*; highest value per unit of risk |
| **C4** | OP‑6 `normalize` zero-guards | **yes, on degenerate input only** | removes a NaN-returning predicate |
| **C5** | OP‑2 ray/triangle → public kernel `moller` | **yes, categorically** | highest risk; do it when C1–C4 have proven the equivalence-check machinery |
| **C6** | OP‑3 point→segment 2D, one degeneracy rule | **yes** | needs an owner ruling on *which* rule |
| **C7** | OP‑4 `dyadic::face_normal` de-FMA | **yes** | small, isolated, but moves coverage sampling |
| **C8** | OP‑9 remaining `vec3` copies | **only where `hypot` is involved** | mechanical; split into a byte-neutral batch and a `hypot` batch |
| **—** | OP‑5 parity, OP‑7 evidence FMA | — | **not recommended.** §7 |

### Per item

#### C1 — `plinth-emit` adopts `geom_kernel::vec3`. Byte-neutral, provable.

- **Canonical:** `geom_kernel::vec3::{add, sub, scale, dot, cross, norm}`.
- **Changes:** delete `emit/lib.rs:1010-1031`; `use geom_kernel::vec3::*`; rename `length` → `norm`
  at its 3 call sites (`:780`, `:945`, `:988`).
- **Breaks:** nothing. The formulas are character-for-character the same operations in the same
  order. `plinth-emit` already depends on `geom_kernel` (since `df2b3d2`, 2026‑08‑09).
- **Pinned digests that move:** **zero.** This is the claim the pilot exists to prove.

#### C2 — One home for the 2D polygon primitives. Byte-neutral.

- **Canonical:** a new `geom_kernel::poly2` — `signed_area`, `point_in_polygon`,
  `point_segment_distance`, `segments_intersect` — with **one named degeneracy constant**, as
  the `plinth-layers` note asks for.
- **Changes:** replace `perception.rs:1600,1613,1627`, `specialized.rs:8261`,
  `plinth-layers/src/lib.rs:292`; repoint `plinth-layers/tests/common/mod.rs:56` at the shared
  one so the test helper stops being a fourth copy.
- **Breaks:** nothing *for `signed_area` and `point_in_polygon`* — `diff` between the copies is
  empty. **The `point_segment_distance` part of `poly2` is C6, not C2** — do not fold them
  together, or the free change and the behaviour change land in one commit and neither can be
  attributed.
- **Prerequisite:** `plinth-layers` gains a `geom_kernel` dependency (it has one already,
  `1e17c8c`).
- **Pinned digests that move:** zero.

#### C3 — Drilling adopts the kernel's `segseg`. **The bug fix.**

- **Canonical:** `geom_kernel::closest::segseg` — verified bit-identical to a 20,001² brute-force
  minimum on the worst divergent case.
- **Kernel work required (Cause C):** `closest` is private. Publish `segseg` — as
  `pub fn segment_segment_distance(...) -> (f64, V3, V3)` — with a doc comment stating the A21
  re-clamp pin so no one "tidies" it later.
- **Changes:** delete `drilling.rs:1962-1993`. Keep the caller's refusal semantics **at the
  caller**: `validate_final_holes` explicitly checks `a <= 0.0 || c <= 0.0` before the call and
  raises `InvalidHoleGeometry` itself. Do not push that refusal into the shared primitive — the
  kernel's contract is "return a distance", and a shared function that refuses for one caller's
  reason is how the next duplication starts.
- **Breaks:** every hollow whose drain-hole pair fell in the 35.98 % short-circuit band gets a
  different — correct — distance, up to 4.494 × 10⁻² mm different, which is 4.5 × 10⁵ × the
  decision margin. **Expect models to change hollow/refuse status.** That is the fix working.
- **Pinned digests that move:** any model whose hollow decision flips changes the emitted file
  wholesale. Candidates are the 8 digests in `crates/plinth-decimate/tests/corpus_budgets.rs`
  and the 8 in `crates/plinth-mesh/examples/corpus_gate.rs` (mesh-stage, so likely safe) and the
  hollow-stage expectations in `crates/plinth-hollow/src/lib.rs` tests. **I did not measure how
  many actually move** — that needs a build, and this round is read-only. The measurement is
  step 1 of executing C3, not an input to deciding it.

#### C4 — One `normalize` contract; delete the NaN path.

- **Canonical:** `geom_kernel::vec3::normalize(a) -> Option<V3>`, `None` on non-finite or
  zero length. Add it — the kernel has no `normalize` today.
- **Changes:** `joints.rs:1575` loses its unguarded divide; `drilling.rs:1947` and
  `drains.rs:2075` become thin wrappers mapping `None` to their own typed refusal.
- **Breaks:** `joints.rs` gains a refusal path where it silently produced `[NaN, NaN, NaN]`.
  **This can turn a currently-emitting model into a refusal**, which looks like a regression and
  is not one. Note also that `drains.rs::normalize` uses the `hypot` chain, so switching it to
  the kernel's `dot.sqrt` moves bits — up to 1.42 × 10⁻¹⁴ mm — on every drain direction.
- **Pinned digests that move:** unmeasured; any drain-bearing model.

#### C5 — Ray/triangle. **The highest-risk item; do it last of the behaviour changes.**

- **Canonical:** the kernel's `moller`. It is the version pinned to the retired oracle and the
  version the rest of the ray machinery already agrees with.
- **Kernel work (Cause C):** publish `moller` behind a named wrapper, e.g.
  `pub fn ray_triangle(o, dv, v0, e1, e2, t_max) -> Option<f64>`, and publish the raw-`E1`/`E2`
  contract (A6) in the doc so a caller cannot accidentally pass reconstructed edges.
- **Changes:** `triage.rs:1099` deleted. `classify_ray` reconstructs `e1 = b − a`, `e2 = c − a`
  and calls the kernel. The `Ambiguous` outcome does **not** survive as an epsilon inside the
  shared primitive; if triage still wants a near-degenerate escape hatch it computes it from the
  returned `t` and the barycentrics at *its own* call site, with its own named constants, and a
  comment saying it is a triage policy and not a geometry rule.
- **Breaks:** everything in the OP‑2 table. Faces within 10⁻⁶ barycentric of an edge flip from
  `Miss` to hit; rays with `t ∈ (−10⁻⁶, −10⁻⁹)` flip from `Miss` to hit; `|det| ∈ [10⁻¹⁴, 10⁻¹²)`
  flips from `Miss` to measured. Each flip changes a parity count, which changes an
  inside/outside classification, which changes whether a component is admitted. **Whole models
  will change admission status in both directions.**
- **Pinned digests that move:** `crates/plinth-mesh/tests/topology_triage.rs` carries 1;
  `crates/plinth-mesh/examples/corpus_gate.rs` carries 8. Beyond those, any model whose triage
  verdict flips changes every downstream digest. Unmeasured, and it should be measured before
  the change is written, not after.

#### C6 — One degeneracy rule for 2D point→segment. **Needs an owner ruling.**

The three thresholds are `3.162 × 10⁻⁸`, `0`, and `10⁻⁹` mm on `|ab|`. There is a principled
answer available and it is not any of the three: the slice lattice quantum is
`SLICE_UNION_QUANTUM_MM = 1e-8`, so **an edge shorter than one quantum cannot be distinguished
from a point by the data that produced it**, and `|ab| < 1e-8` is the physically-derived rule
(`l2 < 1e-16`). That is the physics-first-constants form of the answer, and it is a ruling to
put to the owner rather than a choice to make in a refactor.

- **Changes:** `perception.rs:2788` and `specialized.rs:8286` both call `poly2`'s single
  implementation. `joints.rs:1505` **stays where it is** unless the owner wants it moved — it is
  on `(axial, radial)` profile data, not slice data, and a shared 2D primitive tuned to the slice
  lattice has no business setting its tolerance.
- **Breaks:** whichever of the two moves toward the new rule changes support-birth reach by up to
  3.16 × 10⁻⁸ mm on degenerate edges.
- **Pinned digests that move:** unmeasured; support-stage expectations in
  `crates/plinth-support/tests/`.

#### C7 — `dyadic::face_normal` stops using FMA.

- **Canonical:** the kernel's plain cross and `dot.sqrt` length.
- **Changes:** `dyadic.rs:799-809` uses `geom_kernel::vec3::cross` and `norm`.
- **Breaks:** every normal computed in the coverage sampler moves by up to a few ULP, which
  changes which faces are sampled at a threshold, which changes island demands.
- **Alternative worth considering before doing it:** make the *kernel* FMA instead. It is not
  obviously wrong to prefer the fused form. But the kernel's `vec3.rs:2-4` says the operand order
  is pinned to the retired oracle for bit-parity, so that is a spec change and a much larger
  conversation. Default to C7 as written; do not open the other question inside this sweep.

#### C8 — The remaining `vec3` copies, in two batches.

- **Batch 8a, byte-neutral:** `triage.rs`, `orient/score.rs`, `delicacy_eval.rs`, `brace.rs`,
  `rewire.rs`, `joints.rs` (`norm` only), `drilling.rs`, `segment_batch.rs:665`. All use
  `dot.sqrt` or the expanded form, which measured **0 differences on 2,000,000 inputs**. Verify
  per file with a digest run; expect no movement.
- **Batch 8b, `hypot` sites:** `anchor.rs:2085`, `cco.rs:5846`, `drains.rs:2076`. Switching these
  to `dot.sqrt` moves bits (20.08 % of values, ≤ 2 ULP, ≤ 1.42 × 10⁻¹⁴ mm) on anchor placement
  and CCO. **Land 8b as its own commit** so any digest movement is attributable to it.

---

## 4. Would the audit tool catch a regression? Per item, honestly.

The audit reads **emitted bytes only** and gates on ten support invariants over the support
records and the emitted model mesh. That shapes what it can and cannot see.

| item | audit detects a regression? | reasoning |
|---|---|---|
| **C1** emit `vec3` | **N/A — nothing to detect** | byte-neutral by construction. The check is the digest, not the audit. |
| **C2** polygon primitives | **N/A** | text-identical. Same. |
| **C3** drilling `segseg` | **partly — and the important half is invisible** | If a cavity changes shape, the emitted shell changes and **I6** (support/model intersection) and **I8** (dead geometry) see it. But the dominant outcome of C3 is a *hollow refusal flipping*, and a model that stops emitting **produces no file for the audit to read**. The audit's denominator silently shrinks. **Needs its own check:** a hollow-stage test asserting, per corpus model, that the hollow verdict is what it was — not a digest, a verdict table. |
| **C4** `normalize` | **no** | The change is a refusal on degenerate input. A refused model emits nothing; a model that no longer NaNs emits *different* records the audit will grade, but the audit cannot tell "this improved" from "this is a different plan". **Needs its own check:** a unit test per site that a zero vector yields the typed refusal, plus the same verdict table as C3. |
| **C5** ray/triangle | **no — and this is the dangerous one** | The whole consequence is at the **triage admission** boundary, upstream of emission. A model whose `ContainmentClass` flips to `Ambiguous` is refused and never reaches the emitter, so the audit sees one fewer file and cannot say why. A model that flips the other way appears from nowhere and gets graded on its merits with no baseline. **Needs its own check, and it is a prerequisite, not a follow-up:** a triage-verdict table over all 31 corpus models, captured before the change and diffed after. |
| **C6** 2D point→segment | **yes** | Changes where supports are born. Record counts, **I7**/**I9** island and span coverage, and **I4** ownership all move measurably, and the baseline in `docs/evidence/audit-baseline-hardened.md` gives a per-model, per-invariant `Nf/Mi` cell to diff against. This is the one item the audit was built for. |
| **C7** `dyadic::face_normal` | **yes, weakly** | Changes island demands → **I7**/**I9** counts move. But a few-ULP normal change may move nothing at all, in which case the audit reports "identical" and that is a true negative, not a missed one. |
| **C8a** byte-neutral `vec3` | **N/A** | zero expected movement; the digest is the check. |
| **C8b** `hypot` sites | **yes, weakly** | Anchor and CCO positions move in the last bits → **I1** seat depth and **I5** load path could move by one item. Most likely nothing moves, and "nothing moved" is the expected result. |

**The pattern the owner should take from this table.** The audit is a good instrument for the
items that change *where supports go* (C6, C7, C8b) and a **blind** instrument for the items
that change *whether a model is processed at all* (C3, C4, C5) — which are three of the four
highest-value items. The sequencing decision (audit first, then consolidate) is right, but it
does not cover what it was assumed to cover. **Three of these consolidations need a
verdict-table instrument that does not exist yet**, and building it is a prerequisite for C3,
and a hard prerequisite for C5.

The instrument is small: one table per corpus model of (triage verdict, hollow verdict, emitted
or refused, and the refusal reason), captured to a committed file, diffed. It is not an audit
invariant. It belongs beside the audit, not inside it.

---

## 5. What must NOT be consolidated

Five things. The first is the most important, and the one a well-meaning sweep is most likely
to break.

### 5.1 `tools/plinth-audit/src/vec3.rs` — never. This is the whole point of the tool.

The audit crate has its own vector algebra, and it uses **FMA in both `dot` and `cross`** where
the kernel forbids it. That is not drift. `vec3.rs:3-8` states it:

> *"Reimplemented rather than imported. It deliberately differs from `geom_kernel` in one
> respect worth naming: **nothing here is robust predicate arithmetic.** `geom_kernel` uses the
> `robust` crate for orientation tests; the audit uses plain `f64` and states an explicit
> tolerance at every comparison instead. A disagreement between the two on a near-degenerate
> triangle is a finding to investigate, not a bug in either."*

`docs/specs/audit-contract.md:11-13` makes it a rule of the crate. An auditor that shares an
implementation with the thing it audits verifies nothing — and this project has already been
bitten by exactly that class of defect. **Any sweep that touches `tools/plinth-audit/` has
failed.** Put it in the sweep's own gate: the consolidation must not add a `geom_kernel` or
`plinth-*` dependency to `tools/plinth-audit/Cargo.toml`, and a test should assert that.

Same reasoning, weaker force, for `plinth-verify`: check before consolidating anything there
whether its independence from the producer is load-bearing.

### 5.2 `segment_batch::Association::{AcceptedDistance, FrozenJoint}` — the deliberate split

This is the "formula written to match a retired implementation on purpose" the census was asked
to find, and it is inside the kernel. `point_segment_evaluation` (`segment_batch.rs:619`) takes
an `Association` and computes the offset **two different ways**:

```rust
Association::AcceptedDistance => [ relative[0] - t*axis[0], ... ]
Association::FrozenJoint      => [ point[0] - (start[0] + t*axis[0]), ... ]
```

Algebraically identical, not numerically. Measured over 2,000,000 random rows: they differ on
**479,933 — 24.00 %** — by up to **366 ULP** and **2.13 × 10⁻¹⁴ mm**. `FrozenJointTaperBatch`'s
doc names the contract:

> *"Sealed request for the frozen joint arithmetic. Private fields prevent a caller from
> selecting an association order independently of this named compatibility contract."*

The fields are private precisely so nobody can pick the "tidier" association. **Collapsing these
two moves every joint ball in the corpus.** Leave both. If anything, add a test that asserts
they still differ, so a future cleanup that unifies them fails loudly instead of silently.

### 5.3 The oracle-pinned operand orders in `closest.rs`, `moller.rs`, `parity.rs`, `vec3.rs`

Every one of these files opens with a header saying its evaluation order is pinned, not
idiomatic. `closest.rs:1-4`: *"ported TERM-FOR-TERM from geometry_exact.py with the pinned
operand-construction sequences of Amendments A6/A21. **Idiomatic reassociation is a spec
violation here even when algebraically neutral** — these outputs feed bit-parity pins."*

Publishing `moller` and `segseg` (C3, C5) must publish the pin with them. A newly-public
function is a newly-attractive refactor target, and the first "simplification" someone applies
to `segseg`'s three-step re-clamp will silently break parity. Carry the A6/A21 note into the
public doc comment, not just the private module header.

### 5.4 `geom_kernel::consts::NEAR_WALL_SKIP` — retired, and kept on purpose

`consts.rs` marks it `#[allow(dead_code)]` with a long note: the predicate it served was
**removed** because a distance cannot distinguish a near face from a thin wall, but the *name*
is still cited by `crates/plinth-hollow/tests/fixtures/PROVENANCE.md` (the 0.02 mm thin-fin
fixture was cut to straddle it) and by `crates/plinth-support/src/contact.rs`. A dead-code sweep
will delete it and strand both references. It is not geometry duplication; it is a named limit
kept for its citations.

### 5.5 `joints.rs::segment_distance_2d` — different data, not a third opinion

It operates on `(axial, radial)` frustum-profile coordinates (`joints.rs:1493-1495`), not on
slice loops. Folding it into a slice-lattice-tuned `poly2` would give a frustum profile a
tolerance derived from the slicing quantum, which is meaningless for it. Consolidate the
*formula* if you like; do **not** consolidate the *tolerance*.

---

## 6. The one-time equivalence check, per consolidation

The rule, stated once: **no consolidation lands without a test that exercises the changed path
and counts how many times it fired.** The project's own record is unambiguous on why —
`#77`'s byte-identity proof used a model that never fired the changed path, and two ACCEPTs
went by before a tripwire caught it. A green suite that never called the new function is the
default outcome, not the unlikely one.

And: **the test that would have caught the ray/triangle divergence lived in the deleted
`plinth-metrics` crate.** No consolidation below is planned without its replacement named.

| item | equivalence check | recorded deliberate differences |
|---|---|---|
| **C1** | `plinth-emit` emits the corpus; every artifact digest is **byte-identical**. Plus a hit counter proving `geom_kernel::vec3::cross` fired ≥ once per emitted frustum. | none — if any byte moves, C1 is wrong |
| **C2** | Differential test: 10⁶ random polygons and query points through old and new `signed_area` / `point_in_polygon`; assert **bit-identical**, and assert the new one was called (the copies are text-identical, so a non-firing test would pass anyway — the counter is the whole check). Lives in `rust/geom_kernel/tests/kn_poly2_matches_retired_copies.rs`, with the retired bodies pasted in as frozen reference. | none |
| **C3** | `rust/geom_kernel/tests/kn_segseg_matches_brute_force.rs`: for 10⁵ random pairs at hole-keepout scale, assert the kernel is within 10⁻¹² of a dense brute-force minimum. **Plus** a named regression test carrying the exact worst case measured here (`p1 = [-0.31000757761311015, 0.14392829166134802, -0.1294743934636583]` …) asserting kernel `5.526200254289435e-1` and documenting that the retired `drilling` version returned `5.975634407462165e-1`. This is the replacement for the deleted `plinth-metrics` test. | **Recorded and attributed:** every pair with `L₁·L₂·|sin θ| < 3.162 × 10⁻⁴ mm²` now gets a different — correct — answer. Owner-visible, because it flips hollow verdicts. |
| **C4** | Per-site unit test that `normalize([0,0,0])` yields the typed refusal, at all three sites. Plus a test asserting `joints`' *former* behaviour is gone: `assert!(!result.is_nan())`. | `joints.rs` now refuses where it returned NaN. Attribute in the commit message; it is a behaviour change that looks like a regression. |
| **C5** | `rust/geom_kernel/tests/kn_ray_triangle_divergence_classes.rs` — the OP‑2 table as seven named test cases, each asserting the kernel's answer and carrying the retired triage answer in a comment. Plus a fired-count assertion over a real corpus model, because a ray/triangle test on synthetic triangles proves nothing about a mini. **Plus** the triage-verdict table from §4, captured before the change. | **All seven rows of the OP‑2 table**, each attributed to its rule: barycentric slack `EPS = 1e-6`, `t` slack `EPS = 1e-6`, determinant band `[1e-14, 1e-12)`, and the removal of `Ambiguous` as a geometry-level outcome. |
| **C6** | Differential test across the three retired thresholds over 10⁶ near-degenerate segments, asserting the new single rule and recording the two divergence bands. **Plus** the audit's per-model `Nf/Mi` cells diffed against `audit-baseline-hardened.md`. | Every segment with `|ab|` in the band between the retired rule and the new one. Bounded by 3.16 × 10⁻⁸ mm; state the bound in the record. |
| **C7** | Differential test: 10⁶ random triangles through FMA and non-FMA `face_normal`, recording the difference distribution. Assert the sampler fired on a real model. | Any face whose normal moves by ≥ 1 ULP. Expect most of the corpus. |
| **C8a** | Digest-identity over the corpus. Zero movement expected; **any** movement means a site was misclassified as byte-neutral and the batch stops. | none |
| **C8b** | Digest run **as its own commit**. Record which digests moved and by how many records. | Every `hypot`-derived length, ≤ 2 ULP / 1.42 × 10⁻¹⁴ mm. |

**One structural rule for all of the above:** the frozen reference bodies pasted into these
tests must be *copies of the retired code*, never a re-derivation. A test that re-implements the
production formula proves the formula agrees with itself. That defect is already on this
project's record.

---

## 7. What is not worth doing

**OP‑7, the FMA `dot` in `coverage_evidence.rs`.** It differs from the plain form on 44.76 % of
inputs, which sounds alarming and is not. `coverage_evidence.rs:1-14` records an owner ruling
that the entire module is evidence-only and never a gate input, the module is not imported by
`gates.rs`, and the function has two call sites, both inside an SVG projection. It cannot move a
byte. Consolidating it costs a review and buys nothing. **Leave it, and add one line to its doc
comment saying the FMA is intentional and why**, so the next census does not re-open it.

**OP‑5, the parity/containment duplication.** Publishing `geom_kernel::parity` and rewriting
`triage::classify_ray` on top of it is a genuine improvement — an indexed grid instead of a
linear scan over every face — and it is a *rewrite*, not a consolidation. It changes the dedup
rule (relative `1e-9` → absolute `ZDEDUP = 5e-3`, six orders of magnitude), the work complexity,
and the failure vocabulary all at once, and it lands on top of C5 which has already destabilised
the same code path. **Do not do it in this sweep.** If triage's linear scan is a performance
problem, that is a separate, measured piece of work with its own justification.

**The `hypot` → `dot.sqrt` batch (C8b), on its stated merits.** The measured divergence is
1.42 × 10⁻¹⁴ mm, which is 2.1 × 10¹³ times smaller than the 0.3 mm hard clearance limit. No
placement decision can turn on it. It is worth doing **only** as part of getting to one
vector implementation, so a future reader does not have to re-derive that it does not matter —
and it should be sold to the owner on exactly that basis, not as a correctness fix.

**And the sweep as a whole, weighed against the headline.** C3 is a real bug and should be fixed
regardless. C5 removes a categorical inconsistency at a decision boundary and is worth the risk
*once the verdict-table instrument exists*. Everything else is hygiene with a measured impact of
at most 3.16 × 10⁻⁸ mm. **None of it will move "0 of 31" toward a printable file.** If the
choice is between this sweep and work on why supports are too thick, too tightly spaced, and off
the critical parts, the sweep loses — except for C3, which is small enough to do on its own.

---

## Appendix — how the numbers were produced

Every measured figure comes from one of three sources, all repeatable from this tree:

1. **Static census:** `grep -rn` over `crates/` and `rust/` with `--include='*.rs'`, and
   `diff` on extracted function bodies for the text-identity claims (OP‑8).
2. **History:** `git log -S'<verbatim function signature>' --reverse --date=short` per file for
   first-appearance dates; `git log -S'pub mod <name>' -- rust/geom_kernel/src/lib.rs` for
   visibility history; `git log -S'geom_kernel' -- crates/<c>/Cargo.toml` for dependency dates.
3. **Numerical harness:** three standalone `rustc -O` programs that re-implement each in-repo
   variant verbatim — copied from the source, not re-derived — and run 2,000,000 deterministic
   xorshift-seeded inputs through each pair, reporting difference counts, max ULP, max absolute
   difference, and named worst cases. The OP‑1 correctness claim additionally uses a
   20,001 × 20,001 dense sweep of the two segments as an independent third opinion.

The harness sources are scratch (`$TMPDIR/geomcensus/{m,n,b,c}.rs`) and are **not committed** —
they are measurement instruments for this document, not engine code. Anything from them that
must survive is captured as a named test in §6, with the retired bodies pasted in frozen.
