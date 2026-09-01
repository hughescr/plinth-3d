# Geometry canon — the surviving implementation of every duplicated operation

Status: **ratified rulings, binding on the consolidation sweep.** This document rules on the
census in `docs/plans/geometry-consolidation.md` (OP-1..OP-9, plan items C1..C8). Every agent
implementing a C-item follows this file; where this file and the census disagree, this file
wins and says why at the point of disagreement.

Scope of authority: which implementation survives, its exact contract (signature, units,
behaviour on every degenerate input), the one value of every tolerance that survives, the
triage rule for inline sites, and the expected behaviour changes. It does not re-measure the
old code (owner ruling: *"Just fix it - we know the fix! One common geometry module."*) and it
does not order the work — the census's C1..C8 ordering stands.

Facts this file derives from, verified against source at `feature/stage0-bytes-audit`:

- Printer: ELEGOO Saturn 4 Ultra 16K — XY pixel pitch **14 × 19 µm** (0.014 / 0.019 mm),
  project layer height **0.050 mm** (`docs/datasheets/printers/saturn-4-ultra-16k.md`).
- Slice lattice: `SLICE_UNION_QUANTUM_MM = 1.0e-8` mm (`crates/plinth-layers/src/lib.rs:69`),
  equal by design to perception's stitch key `LOOP_MERGE_MM = 1e-8`. All slice-loop
  coordinates are `round(value / 1e-8)` i64 lattice points dequantized back to f64.
- Clearance doctrine: 0.6 mm placement margin, 0.3 mm hard limit
  (`docs/physics.md`, owner ruling 2026-08-22).
- Kernel house rule: no FMA, no reassociation (`rust/geom_kernel/src/vec3.rs:1-5`, spec
  sec 5.1); oracle-pinned operand order in `closest.rs` / `moller.rs` / `parity.rs`
  (Amendments A6/A21).

---

## 1. Per-operation rulings

### OP-1 — Segment vs segment distance, 3D

**Survivor: `rust/geom_kernel/src/closest.rs` `segseg()`** — census pick **ratified**.
Published as:

```rust
pub fn segment_segment_distance(p1: V3, q1: V3, p2: V3, q2: V3) -> (f64, V3, V3)
```

`crates/plinth-hollow/src/drilling.rs:1962` `segment_distance()` is deleted (C3).

**Contract.**
- Units: all inputs mm; returns `(distance_mm, closest_on_p1q1, closest_on_p2q2)`.
- Evaluation order is the A21-pinned re-clamp sequence (`s` from the denominator, clamp;
  `t` from `s`, clamp; `s` recomputed from `t`, clamp). The public doc comment MUST carry the
  A21 pin verbatim from the module header so a later "simplification" fails review, not parity
  (census §5.3, ratified).
- **Parallel / near-parallel** (`denom = |u|²|v|² − (u·v)² ≤ SEGSEG_EPS`): takes the `s = 0`
  branch, then the re-clamp recovers a correct minimum. This is the decisive difference from
  the retired drilling copy and the reason the guard value is safe — see the tolerance ruling
  below.
- **Zero-length first segment** (`a ≤ SEGSEG_EPS`): `s = 0`; degenerates to point-vs-segment.
  Correct finite answer, no refusal.
- **Zero-length second segment** (`e ≤ SEGSEG_EPS`): `t = 0`; point-vs-point when both are
  degenerate. Correct finite answer.
- **Non-finite input**: not guarded here; output is non-finite. Callers that must refuse
  (drilling does) check before or after the call.
- **Refusal semantics stay at the caller** (census C3, ratified): `validate_final_holes`
  checks for zero-length keepout axes itself and raises `InvalidHoleGeometry` itself. The
  shared primitive returns a distance for every input; it never encodes one caller's refusal.

**Tolerance ruling — `SEGSEG_EPS = 1e-14` survives.** Its comparand `denom` has units mm⁴
(it is `L₁²L₂²sin²θ`), so as a *number* the guard is unit-inconsistent exactly the way
drilling's was. The reason it survives anyway, and drilling's `PROOF_EPSILON_MM = 1e-7` does
not, is that in the kernel the guard selects a **branch whose answer is still correct** — the
`s = 0` start plus the A21 re-clamp yields the true minimum for parallel and near-parallel
pairs (verified by the census's 20,001² brute-force sweep: kernel bit-identical to brute force
on the worst divergent case). In drilling the same guard, 10⁷ times wider, forced `s = 0`
*without* a correct recovery path and produced answers wrong by up to 4.494 × 10⁻² mm. The
lesson recorded for the next reader: **an epsilon that selects between two correct paths
prunes; an epsilon that selects between a correct and an incorrect path decides.** Only the
first kind may be unit-sloppy and survive. `1e-14` is oracle-pinned; changing it is a parity
break with no benefit.

### OP-2 — Ray vs triangle

**Survivor: `rust/geom_kernel/src/moller.rs` `moller()`** — census pick **ratified**.
`crates/plinth-mesh/src/triage.rs:1099` `ray_triangle_distance()` is deleted (C5).

Published as a wrapper that exposes the already-computed barycentrics, so call-site policy
(triage's `Ambiguous`) can be built without recomputation or a second formula:

```rust
pub struct RayTriangleHit { pub hit: bool, pub t: f64, pub u: f64, pub v: f64 }
pub fn ray_triangle(o: V3, dv: V3, v0: V3, e1: V3, e2: V3, t_max: f64) -> RayTriangleHit
```

`t`, `u`, `v` are the values the pinned sequence already computes; the wrapper adds **no**
arithmetic. When `det.abs() ≤ DET_EPS`, `hit` is false and `t`/`u`/`v` are the guarded
`inv = 0` products (all zero) — callers must not read them on a miss.

**Contract.**
- Units: `o`, `v0`, `e1`, `e2` in mm. `t` is in units of `|dv|`: ray point = `o + t·dv`. The
  ±`EPS` slack on `t` and the `t_max` bound are in those same units, so **the canonical usage
  passes unit-length `dv`, making `t` a millimetre**. A caller passing non-unit `dv` must
  scale `t_max` itself and accept that the `t` slack scales with it. (Stated, not enforced —
  enforcing it would add a normalize into a pinned hot path.)
- **A6 edge contract**: consumes the RAW stored `E1`/`E2`, never reconstructed vertices, when
  the mesh carries stored edges. A caller that has only vertices computes `e1 = sub(b, a)`,
  `e2 = sub(c, a)` with `geom_kernel::vec3::sub` — the same subtraction the kernel's own
  surface build uses — and must say in a comment that it is reconstructing. Carry A6 into the
  public doc comment.
- Acceptance: `u ≥ −EPS`, `v ≥ −EPS`, `u+v ≤ 1+EPS`, `t ∈ [−EPS, t_max+EPS]`, `|det| > DET_EPS`.
- **Degenerate triangle / parallel ray** (`|det| ≤ DET_EPS`): miss. Correct, because a ray
  parallel to a face's plane has its true crossings registered on the adjacent non-parallel
  faces; measuring through a near-singular `1/det` would amplify noise into a fake crossing.
- **Non-finite input**: not guarded; propagates. Triage's existing `is_finite` checks move to
  its call site (they are triage policy — the kernel's oracle had no such refusal).

**Tolerance rulings.**
- **`EPS = 1e-6` (barycentric and t slack) survives.** Oracle-pinned. Physical check: a t
  slack of 1e-6 mm is 1/50,000 of a layer and 1/14,000 of the finer pixel pitch — it cannot
  admit or reject anything printable. Its real job is *consistency at shared edges*: a ray
  crossing exactly through an edge shared by two faces must register in at least one of them.
  Triage's zero-slack version can miss **both** faces on such a crossing, silently flipping
  parity for every point beyond it — that, not the band arithmetic, is the defect C5 removes.
  The cost is a 1e-6-wide barycentric shell where a ray *outside* the surface can register a
  crossing on both faces of a silhouette edge; both hits land at the same `t` and are merged
  to one by any dedup, which is the oracle's accepted semantic.
- **`DET_EPS = 1e-14` survives; triage's `1e-12` dies.** The 100×-wide band
  `[1e-14, 1e-12)` becomes measured hits. `DET_EPS` is a prune (below it the answer is
  taken from neighboring faces), not a verdict.
- **`ORIGIN_EPS = 1e-9` survives only as triage call-site policy, renamed and commented as
  policy.** A crossing within 1e-9 mm of the query point genuinely has an untrustworthy sign;
  triage may keep mapping it to `ContainmentClass::Ambiguous`, built from the wrapper's `t`.
  It is not part of the geometry primitive.
- **`EDGE_EPS = 1e-10` dies with no replacement.** Its job — not trusting a hit within
  1e-10 barycentric of an edge — is exactly the shared-edge problem the kernel's slack solves
  correctly. Keeping it would re-refuse what the slack just fixed.
- **Crossing dedup: triage keeps its relative `1e-9` for this sweep.** The kernel's absolute
  `DEDUP_LOC_MM = 3e-3` belongs to the parity engine (OP-5), which is explicitly out of scope
  (census §7, ratified below). Both dedups merge the same-`t` duplicate hits that the slack
  produces, so C5 is safe under either; they differ only on genuinely thin (< 3 µm) walls,
  which is an OP-5 question. Mark the divergence at the call site
  (`EXEMPT-WITH-MARKER`, §2) so the next census does not re-discover it. *Flagged
  uncertainty:* if the C5 verdict table shows a model flipping admission that traces to dedup
  rather than to the four acceptance rules, stop and escalate to the owner rather than
  adjusting either constant.

### OP-3 — Point to segment, 2D (this is ruling C6)

**Survivor: one new implementation, `geom_kernel::poly2::point_segment_distance`**, replacing
`perception.rs:2788` and `specialized.rs:8286`. `joints.rs:1505` stays where it is
(EXEMPT-WITH-MARKER — different data, see §4).

```rust
pub fn point_segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64
```

Units: mm in, mm out; always ≥ 0 and finite for finite input. Body is the existing shared
formula (project-and-clamp, `hypot` for the final 2D distance — all three copies already
agree on everything but the degeneracy test).

**The degeneracy rule — derived, not chosen.**

The three incumbent tests on `l2 = |ab|²` are `< 1.0e-15` (perception, i.e. |ab| < 3.162e-8),
`== 0.0` (specialized), and `< 1e-18` (joints, |ab| < 1e-9). None of the printer-physics
scales can decide between them: the finest physical feature the machine resolves is the
14 µm pixel / 50 µm layer, and **all three candidate thresholds sit six or more orders of
magnitude below it**. So the ruling cannot come from what is printable. It comes from what
the *data can represent*:

1. Slice-loop coordinates are dequantized points of a 1e-8 mm lattice
   (`SLICE_UNION_QUANTUM_MM`). Two **distinct** lattice points are ≥ one quantum apart
   (up to f64 rounding of the dequantization, a relative error of a few ULP). Two
   **coincident** lattice points subtract to exactly 0.0.
2. Therefore, in the data these functions are fed, a segment is either exactly degenerate
   (`l2 == 0.0`) or has `|ab| ≥ 1e-8 mm (1 − O(ε))`. **There are no representable inputs in
   between.** A zero-length segment physically means "the union normalizer emitted the same
   lattice point twice" — a point, not an edge.
3. A 1–3-quantum edge (the census's measured divergence case, reachable because the union
   normalizer keeps such edges as real geometry) **is a real edge to its producer**, and the
   consumer must agree: the correct answer for a query at `(1,0)` against the 3-quantum edge
   `(0,0)→(3e-8,0)` is the projection `0.99999997…`, not the endpoint distance `1.0`.
   **`specialized`'s answer was correct; `perception`'s was wrong** — its 3.162e-8 threshold
   calls edges the producer asserts exist "points", making two stages disagree about whether
   a thing exists.

So exact-zero is the *semantically* right test on lattice data. It is not the *robust* form
of that test: on any non-lattice input (an upstream defect, or future non-slice data routed
here) a subnormal `l2` survives `== 0.0` and then `t = dot/l2` can produce `inf`/NaN, and a
NaN distance is **silently dropped by the `f64::min` reach-fold** — a value meaning
"measured nothing" read as "nothing nearby", the exact failure class this project keeps
reproducing. The surviving rule closes that hole while agreeing with exact-zero on every
representable input:

```rust
/// Mirror of plinth-layers::SLICE_UNION_QUANTUM_MM (dependency points the other
/// way); a cross-crate pin test in plinth-layers asserts equality.
pub const SLICE_QUANTUM_MM: f64 = 1.0e-8;
/// An edge shorter than half a lattice quantum cannot have come from two
/// distinct lattice points; it is a point to the data that produced it.
pub const DEGENERATE_EDGE_L2_MM2: f64 = 0.25 * SLICE_QUANTUM_MM * SLICE_QUANTUM_MM; // 2.5e-17

if l2 < DEGENERATE_EDGE_L2_MM2 { return dist(point, a); }  // hypot form, as today
```

**Why half a quantum and not one quantum** (this overrules the census's proposed
`|ab| < 1e-8` by a factor of 2, for its own reason): a true 1-quantum edge dequantizes with
f64 rounding, so its `l2` lands within a few ULP of 1e-16 — *on either side*. A threshold at
exactly `1e-16` puts every legitimate 1-quantum edge on a rounding knife-edge; a threshold at
`(Q/2)² = 2.5e-17` is 4× below the smallest representable real edge and ∞× above the only
representable degenerate one (exactly 0), so **no representable input sits near it**. That is
the property a degeneracy threshold must have, and neither incumbent nor the census's
proposal has it.

Where genuinely uncertain: nothing in this ruling; the inputs, the producer's semantics, and
the failure mode are all verified in-tree. The residual risk is a non-slice caller adopting
`poly2::point_segment_distance` for data on a different lattice — the doc comment must say
the constant is slice-lattice-derived and that other lattices need their own wrapper
(this is exactly why `joints.rs` stays out, census §5.5, ratified).

**Constant plumbing:** `geom_kernel` cannot import `SLICE_UNION_QUANTUM_MM`
(`plinth-layers` depends on `geom_kernel`, not vice versa). `poly2` declares the mirror
constant above; **`plinth-layers` gains a pin test asserting
`geom_kernel::poly2::SLICE_QUANTUM_MM == SLICE_UNION_QUANTUM_MM`** so the pair cannot drift
silently — the same pattern that already ties the quantum to `LOOP_MERGE_MM`.

### OP-4 — Face normal

**Survivor: the kernel's form** (`rust/geom_kernel/src/mesh.rs:301-359`): plain `vec3::cross`
(no FMA), length = `norm` (`dot(n,n).sqrt()` — measured bit-identical to the expanded
`(x²+y²+z²).sqrt()` on 0 of 2,000,000 inputs, so both spellings are one implementation).

**Contract.**
- Units: vertices mm; normal is unit (dimensionless); `area = 0.5 · length` mm².
- **Degenerate face: the predicate is exact — `length <= 0.0` — and carries no tolerance.**
  Ratified from the mesh.rs comment: `length > 0.0` is precisely the condition under which
  the three divisions produce a unit vector; there is nothing for a magic number to decide
  (physics-first constants). A zero-area face has **no** normal — never `[0,0,0]`, which
  every consumer would misread as a direction.
- The *refusal type* is per-caller policy: typed `DegenerateFace` error in the kernel's
  surface build, `Option::None` in `dyadic::face_normal`. Both are conforming; returning a
  zero or NaN vector is not.
- Non-finite input: refuse (kernel style) or `None` (dyadic style); never propagate.

**C7 scope extension (overrules the census's narrower scoping, same direction):**
`dyadic.rs` violates the no-FMA rule in **three** functions, not one — `face_normal`,
`triangle_area_mm2`, and `dist_sq` (which feeds `max_edge_mm`, a planning threshold input).
De-FMA all three in the C7 commit: same file, same violation, same consumer
(`build_rows` → coverage sampling), and leaving `dist_sq` fused would leave `face_normal`'s
replacement length computed through a fused `dist_sq` anyway. `face_normal` becomes
`vec3::{sub, cross, norm}`; `dist_sq(a,b)` becomes `dot(sub(a,b), sub(a,b))`;
`triangle_area_mm2` uses `vec3::cross` and `norm`. The census's alternative (make the kernel
FMA instead) is **rejected** for this sweep, for the census's own reason: vec3's operand
order is pinned to the retired oracle; changing it is a spec amendment, not a consolidation.

### OP-5 — Inside/outside by ray parity

**Not consolidated in this sweep — census §7 ratified.** `geom_kernel::parity` stays private;
`triage::classify_ray` keeps its linear scan, its relative dedup (see OP-2), and its
`Ambiguous` vocabulary. Unifying them is a rewrite with its own justification, landing on the
same code path C5 just destabilised. If the linear scan is ever a measured performance
problem, that is separate work.

### OP-6 — `norm` / `normalize`

**Survivor `norm`: `geom_kernel::vec3::norm` = `dot(a, a).sqrt()`.**
- Formula 2 (`(x*x + y*y + z*z).sqrt()`) is bit-identical (measured, 0/2,000,000) —
  consolidating those sites is free (C8a).
- Formula 3 (`hypot` chains: `anchor.rs:2085`, `cco.rs:5846`, `drains.rs:2076`) dies (C8b).
  `hypot` is semantically better only beyond ~1e±150 mm, unreachable on a 218 mm plate; its
  cost is a permanent 2-ULP divergence on 20 % of values. One formula wins.
- Contract: mm in, mm out; `norm([0,0,0]) = 0.0`; overflow/underflow at physically
  unreachable magnitudes is accepted and documented (`[1e200;3] → inf`, `[1e-200;3] → 0.0`).

**Survivor `normalize`: new, `geom_kernel::vec3::normalize(a: V3) -> Option<V3>`.**

```rust
pub fn normalize(a: V3) -> Option<V3> {
    let n = norm(a);
    (n.is_finite() && n > 0.0).then(|| scale(a, 1.0 / n))
}
```

- `None` iff the norm is non-finite or not strictly positive. This covers: the zero vector;
  any non-finite component; and sub-1e-154 mm vectors whose squared norm underflows to 0 —
  ruled acceptable because a vector that short does not encode a physical direction in this
  engine, and a caller needing a direction from one has an upstream defect.
- **The NaN path is deleted, with no compatibility escape.** `joints.rs:1575`'s unguarded
  `scale(a, 1.0/norm(a))` — measured `[NaN,NaN,NaN]` on zero and `[inf,inf,inf]` on
  `[1e-200;3]` — is the one incumbent that returns a value meaning "measured nothing" that
  every downstream `<`/`>` reads as "not violated". No caller may unwrap `None` into a
  default vector; each maps it to its own typed refusal (`drilling` →
  `InvalidHoleGeometry`, `drains` → `NonFinite`, `joints` → a new typed refusal chosen by
  the C4 implementer). A model that refuses where it previously NaN'd is the fix working;
  attribute it in the commit message (census C4, ratified).

### OP-7 — `dot` / `cross`, plain vs FMA

**Survivor: plain left-to-right `vec3::dot` / `vec3::cross` everywhere a value can reach a
verdict.** The one FMA site, `coverage_evidence.rs:927`, is **left in place** (census §7
ratified): the module records an owner ruling that it is evidence-only, never a gate input,
not imported by `gates.rs`, with two call sites inside an SVG projection. Add the one-line
doc comment saying the FMA is intentional and cannot move a byte, so the next census skips
it. EXEMPT-WITH-MARKER.

### OP-8 — 2D polygon primitives

**Survivor: new `geom_kernel::poly2`** carrying, verbatim (the copies are text-identical —
verified against `perception.rs:1600-1632`, `specialized.rs:8261`,
`plinth-layers/src/lib.rs:292`):

- `signed_area(points: &[[f64; 2]]) -> f64` — mm², shoelace, positive = counter-clockwise.
  Degenerate: `points.len() < 3` → `0.0` exactly. This is load-bearing: winding-derivation
  callers read the sign, and a 2-point "loop" must be sign-free.
- `point_in_polygon(point, polygon) -> bool` — even-odd crossing test, exactly the incumbent
  text. On-boundary behaviour is unspecified-but-pinned (a boundary point may report either
  side); every production caller already pairs it with a distance-tolerance check
  (`point_in_solid`, `solid_contains`), which is the contract: **do not use bare
  `point_in_polygon` for a verdict on points that can sit on the boundary.** Empty polygon →
  `false`.
- `segments_intersect(a, b, c, d) -> bool` — the strict-CCW form from `perception.rs:1627`.
  Collinear/touching contacts report `false` (strict inequalities); pinned, not improved.
- `point_segment_distance` — OP-3 above. **Ships in the C6 commit, not the C2 commit**, so
  the byte-neutral move and the behaviour change stay separately attributable (census C2
  warning, ratified).

`plinth-layers/tests/common/mod.rs:56` repoints at `poly2` so the test helper stops being a
fourth copy.

### OP-9 — Private 3-vector algebra, thirteen sites

**Survivor: `geom_kernel::vec3` (`add`, `sub`, `scale`, `dot`, `cross`, `norm`, `dist`,
plus the new `normalize`).** All private copies whose formulas are kernel-identical
(`emit`, `triage`, `orient/score`, `delicacy_eval`, `brace`, `rewire`, `drilling`,
`joints`' norm, `segment_batch.rs:665`'s private dot) are deleted in favour of imports —
C1 and C8a, byte-neutral, provable by digest. The `hypot` sites are C8b (see OP-6).
`emit`'s `length` is renamed to `norm` at its call sites. Sites listed in §4 are exempt.

---

## 2. The triage rule for the ~134 inline sites

The census's table (dot 56, cross 24, squared-length 54 sites outside the kernel) is a triage
list. Per-crate agents apply this procedure to each site **without re-deciding policy**:

**Step 1 — is it EXEMPT-WITH-MARKER?** Check the site against §4's list (audit tool,
`segment_batch` association split, `coverage_evidence` FMA dot, `joints` 2D tolerance,
`NEAR_WALL_SKIP`, `plinth-verify` independence). If listed, or inside
`tools/plinth-audit/` at all: leave the code, ensure the marker comment exists
(`// GEOMETRY-CANON EXEMPT: <one-line reason, citing docs/specs/geometry-canon.md §4.x>`),
and add the site to the crate's `no_private_vector_algebra` allowlist. Done.

**Step 2 — is it MUST-CONSOLIDATE?** Yes if **any** of:
- (a) the computed value is compared against any constant or tolerance — named or literal —
  within the same function, or is returned into a function that does so one call up;
- (b) it feeds a verdict: a `Result`/`Option`/`bool`/enum that gates admission, hollowing,
  placement, seating, emission, or validation;
- (c) its formula differs from the kernel's in association, fusion (`mul_add`), or `hypot`;
- (d) it is a normalize — any divide of a vector by its own norm, however spelled.
Consolidate to the §1 survivor. (c)/(d) sites are behaviour-changing: they land in the
C-item commit that owns them (C4, C7, C8b), never in a byte-neutral batch.

**Step 3 — otherwise MAY-STAY**, meaning: plain inline arithmetic with kernel-identical
association, in test/bench/example code, or a production one-off that feeds no comparison
and no verdict. MAY-STAY is permission, not a goal: in production code of a crate that
already depends on `geom_kernel`, prefer the import anyway when the swap is in a C8a batch
being done regardless — it is free. In tests, **leave it**: a test re-implementing
production arithmetic through the production crate is one step toward the
test-re-implements-production defect already on this project's record.

**Tie-breaker, so two agents agree:** deciding (a)/(b) requires tracing at most **one** call
up from the site. If after one hop it is still unclear whether the value can reach a
comparison or verdict, classify **MUST-CONSOLIDATE**. The failure cost is asymmetric: a
needless consolidation to an identical formula is byte-neutral; a missed tolerance-carrying
site is the next OP-1.

**Enforcement (census §2 fix 3, ratified):** each swept crate gains a
`tests/no_private_vector_algebra.rs` grep-test over its own `src/`, failing on
`fn dot(`/`fn cross(`/`fn norm(`/`fn normalize(`/`fn sub(`/`fn subtract(`/`.mul_add(`
outside its named allowlist (file + line + canon-section citation per entry). Gate the test
on a **count of files it actually scanned** being > 0 — a matcher that finds no files must
fail, not pass (the xargs incident rule).

---

## 3. Expected behaviour changes, per item

For the verify agent: attribute digest movement to these, in this order. Anything moving
outside this table is a defect in the sweep, not an expected change.

| item | inputs answered differently | old → new | why new is correct |
|---|---|---|---|
| **C1, C2, C8a** | none | — | byte-neutral by construction; **any** digest movement stops the batch |
| **C3** | segment pairs with `L₁·L₂·sinθ < 3.162e-4 mm²` — 35.98 % of pairs at drain-keepout scale | distance wrong by up to 4.494e-2 mm → brute-force-verified true minimum | the retired guard forced `s = 0` with no correct recovery; hollow verdicts may flip in **both** directions, which is the bug fix working |
| **C4** | zero, sub-1e-154 mm, and non-finite vectors reaching `normalize` | `joints`: `[NaN,NaN,NaN]` silently passing every guard → typed refusal | a NaN normal reads as "not violated" downstream; a refusal is loud. A model newly refusing here was never validly measured before |
| **C5** | rays within 1e-6 barycentric of a shared edge (old: could miss both faces); `t ∈ (−1e-6, 0)` (old: miss); `\|det\| ∈ [1e-14, 1e-12)` (old: unmeasured miss); edge-adjacent hits with `u`,`v`, or `1−u−v ≤ 1e-10` (old: whole-component `Ambiguous`) | Miss/Ambiguous → measured hit or definite miss per kernel rules | shared-edge crossings must register at least once or parity is silently wrong for every point beyond; the `Ambiguous` edge band re-refused what the slack fixes. **Whole models change admission status in both directions.** Prerequisite: the §4-census verdict table, captured before the change |
| **C6** | `perception` only: slice edges with `\|ab\|` ∈ [5e-9, 3.162e-8] mm — i.e. real 1–3-quantum lattice edges | endpoint distance → projection; reach values shrink by ≤ 3.16e-8 mm | the producer asserts these edges exist; the consumer must agree (§1 OP-3). `specialized` is expected to move **zero** digests (exact-zero and the new rule agree on all lattice inputs) — movement there means the ruling was misapplied: stop |
| **C7** | every face normal, edge length, and triangle area in the coverage sampler | FMA → plain, few-ULP shifts | restores the spec-5.1 no-FMA rule inside the sampler; island demands may shift at sampling thresholds |
| **C8b** | every `hypot`-derived length in anchor/cco/drains | ≤ 2 ULP, ≤ 1.421e-14 mm | one formula project-wide; sold as consolidation, explicitly **not** as a correctness fix (census §7, ratified) |

Verdict-table prerequisite (census §4, ratified as binding): C3, C4 and C5 change *whether a
model is processed*, which the byte audit cannot see (a refused model emits no file to read).
The per-model table of (triage verdict, hollow verdict, emitted/refused, refusal reason),
captured to a committed file before each of those items and diffed after, is a **hard
prerequisite for C5** and a prerequisite for C3/C4. Gate every capture on the row count
equalling the corpus model count.

---

## 4. What must NOT be consolidated

Ratified from census §5, each checked against source:

1. **`tools/plinth-audit/` — never, permanently** (`geometry-consolidation.md` §5.1): its
   independently duplicated vector maths is the only reason its verdicts are evidence about
   the kernel rather than about themselves — an auditor sharing an implementation with the
   audited verifies nothing.
   The sweep's gate: no `geom_kernel` or `plinth-*` dependency may appear in
   `tools/plinth-audit/Cargo.toml`, asserted by a test.
2. **`segment_batch::Association::{AcceptedDistance, FrozenJoint}`** — the deliberate
   numeric split (24 % of rows differ, ≤ 366 ULP), sealed behind private fields on purpose.
   Add the census's suggested test asserting the two associations still differ, so a future
   unification fails loudly. (`segment_batch.rs:665`'s private `dot` is C8a-consolidatable;
   the *association split* is not.)
3. **Oracle-pinned operand orders** in `closest.rs`, `moller.rs`, `parity.rs`, `vec3.rs` —
   publishing `segseg` and `moller` (C3, C5) must carry the A6/A21 pins into the public doc
   comments, not just the module headers.
4. **`geom_kernel::consts::NEAR_WALL_SKIP`** — retired predicate, kept as a named limit its
   fixtures and docs still cite. Not duplication; do not delete.
5. **`joints.rs::segment_distance_2d`** — `(axial, radial)` frustum-profile data, not slice
   data; a slice-lattice-derived tolerance is meaningless for it. Its `1e-18` guard on its
   own exact-zero-producing degeneracies (`r0`/`r1` caps) is sound. The formula stays put;
   EXEMPT-WITH-MARKER. Do not grow `poly2` a tolerance parameter to absorb it this sweep.
6. **`coverage_evidence.rs:927` FMA dot** — evidence-only by owner ruling; marker comment
   only (§1 OP-7).
7. **`plinth-verify`** — before consolidating any site there, the per-crate agent checks
   whether its independence from the producer is load-bearing (same argument as the audit
   tool, weaker force); if unclear after one reading, treat as EXEMPT-WITH-MARKER and record
   the question rather than consolidating.

---

## 5. Flagged uncertainties (conservative choices taken)

1. **OP-2 dedup** — ruled "stays relative at triage" on scoping grounds (it belongs to
   OP-5's algorithm, not OP-2's primitive). If the C5 verdict table contradicts the safety
   argument, escalate; do not tune.
2. **OP-2 `Ambiguous` origin band** — retained as call-site policy because a sign within
   1e-9 mm of the origin is genuinely untrustworthy; a case could be made for deleting it
   too (the `point_on_triangle` pre-check overlaps it). Kept: deleting refusal paths and
   changing the primitive in one item would blur attribution.
3. **OP-6 underflow-to-`None`** — vectors of length < ~1e-154 mm refuse rather than
   normalize (hypot could have handled them). Ruled acceptable as stated; if a real model
   ever hits it, that is an upstream-defect finding, not a reason to reinstate `hypot`.
4. **C6 half-quantum threshold** — differs from the census's one-quantum proposal; the
   knife-edge argument in §1 OP-3 is the entire justification. The two agree on every
   input either would ever see in the current corpus; the refinement only removes the
   rounding ambiguity at exactly one quantum.
