# Geometry consolidation — adjudication of the six review objections

**Status: RULING (revision 4).** This document adjudicates the six objections the
cross-family reviewer raised against `docs/specs/geometry-canon.md`, judged against
the **landed code** of commit `487e71f` (branch `feature/stage0-bytes-audit`). The
canon document itself was rejected; where the implementers diverged from it in
code, the code is what is ruled on here. Where this document and the canon
disagree, this document wins.

**Revision record.** Revision 1 was rejected by the cross-family challenger.
Revision 2 changes, each re-verified against the tree on 2026-08-31:

* **O4 is reversed, REFUTED → UPHELD.** Revision 1's refutation rested on the
  adjacent-face argument; that argument holds only for a ray lying IN a face's
  plane. For a near-parallel ray piercing a nondegenerate face's INTERIOR, no
  adjacent face carries the crossing, and `DET_EPS` converts a real crossing into
  an asserted miss — the constant decides a parity outcome. Conceded and fixed.
* **O5 is rewritten.** "Oracle-pinned" and similar labels are chosen behavior
  pins — recorded provenance, not derivations, and not by themselves
  doctrine-satisfying. Every place a pinned constant decides an outcome is now
  named, and each is either fixed (O2, O4) or entered in the doctrine debt
  register below.
* **O6's marker mandate is withdrawn.** Revision 1 told an agent to add a
  `GEOM-EXEMPT` marker at a site whose own source comment says MUST-CONSOLIDATE
  (`crates/plinth-support/src/perception.rs:1694-1702`) — a direct violation of
  the precedence rule this same document ratified. The fix is now canonical 2-D
  primitives in `poly2` and retargets, not exemptions.
* **The `segment_segment_distance_min` export gap is closed.**
  `rust/geom_kernel/src/lib.rs:67` exports only `segment_segment_distance`;
  `lib.rs` is added to Agent K's write-set with the required `pub use`.

**Revision 3 (owner-ruled extension, 2026-08-31).** The four closed objections
(O1, O2, O3, O6) are untouched. One thing changes and one thing extends:

* **D2's deferral premise is corrected.** Revision 2 deferred the classified
  determinant handling everywhere except triage, "forced forward if the parity
  path is ever consumed by a decision." That condition was already met in the
  tree revision 2 ruled on: the kernel's own walks call `moller` at
  `rust/geom_kernel/src/queries.rs:1781`, `:1844` and `:2002`, and those walks
  feed the support anchor seat list (`crates/plinth-support/src/anchor.rs:1268`),
  hollowing's drillability decision (`crates/plinth-hollow/src/drilling.rs:1032`),
  and every other decision path in §EXT's census. The owner has ruled: extend
  the O4 fix to every live decision-feeding consumer, propagate `NearParallel`
  as explicit uncertainty, instrument the firing rate, and do not soften the fix
  to protect the current 24-of-37 emit count. §EXT below is that ruling; D2 is
  rewritten in place; the implementation order is amended in place.
* **The same correction applies to O2's residual-debt sentence**: the pinned
  `segment_segment_distance` already has a live decision-feeding consumer —
  `seg_tri_dist` (`queries.rs:1972`) uses it for its three edge candidates and
  feeds `segment_clearance*` and `faces_near_segments_batch`. §EXT-2c rules on
  it; the O2 section body (the drilling retarget, the `_min` primitive, the
  constant split) stands as written.

**Revision 4 (challenge round 2, 2026-09-01).** The §EXT-1 census survived the
cross-family check and is unchanged, as are the four closed objections and the
D2 rewrite. Four contract defects in §EXT's implementation contract are fixed,
and one challenger demand is refuted, each verified against the tree on
2026-09-01:

* **The additive lane could not compile as written.** The batch entry points
  are inherent methods on `NativeSurface` (`queries.rs:1115`), and Rust does
  not overload inherent methods, so "classified entry points beside the
  existing ones" without distinct names was impossible; and the test-caller
  sentence "they keep whatever API their assertions pin" contradicted K2's
  deletion gate (5 external test files hold 89 old-API call expressions —
  counted — and K2's write-set excluded all of them). Fixed: a definitive
  classified-name table with FINAL names (no rename-back), and counted
  migration ownership for every old-API test call site (§EXT-2, §EXT-1
  test-site table).
* **Site 3 is closed in the spec — against the challenger's direction.** The
  challenger demanded `grounded = true` for candidate faces in a
  near-parallel-affected column, reading "`!grounded` admits a support
  candidate" as the permissive absorption. Refuted from the code and from this
  document's own ratified justification-12 principle: `grounded = true` is a
  measured-distance claim (`lib.rs:973`) that a `NearParallel` event cannot
  supply, `grounded` is an exemption and exemptions grow only on proof, and a
  face wrongly exempted escapes island discovery and every downstream
  refusal (`lib.rs:995-996`). The ruled disposition is the opposite of the
  demand: affected faces stay candidates, counted (justification 3,
  rewritten). The earlier delegation to the implementing agent is withdrawn
  either way — the direction is now ruled here, which is what the challenger
  was right about.
* **Site 14 gets a real typed state.** `classify_initial_overlaps` reduces
  every row to a bare `f64` (`initial.rs:1440-1483`); the "module's typed
  refusal/unknown path" revision 3 cited did not exist. Fixed: a per-row
  `OverlapMeasure` whose uncertain arm is a named LOWER BOUND that condemns
  the stub with recorded evidence (justification 14, new).
* **EXT-4 had no route to run output.** A successful conservative outcome
  erased its tally at the first stage boundary (`evaluate_delicacy` returns
  only a `DelicacyField`; `perceive_contacts_on_surface` only a
  `ContactCandidatePool`), no lane owned a telemetry sink, and Agent R writes
  no source. Fixed: a `NearParallelLedger` carried across every stage
  boundary under a no-silent-drop rule, a `geom_uncertainty` block on
  `ModelPreparationReport` with the `repair` field's None-vs-zeros contract,
  a new Agent P lane owning the pipeline/CLI/orient threading, and an
  end-to-end planted-fixture test asserted on the run report itself (§EXT-4
  rewritten; order is now K → { L ∥ D ∥ S ∥ H ∥ M } → P → K2 → R, with K2
  expanded into the removal sweep).

**Method.** Every number below was measured on this tree (revision 1's probe
measurements on 2026-08-31, plus revision 2's line-pin and arithmetic
verifications the same day), not quoted from the review or the canon. Where a
claim could not be re-measured cheaply (the 14-artifact corpus head-to-head), that
is said explicitly and the re-measurement is assigned to an implementing agent.

Rulings at a glance:

| # | Objection | Ruling |
|---|-----------|--------|
| O1 | half-ray semantics lost in kernel adoption | **REFUTED** — resolved in the landed code; re-verified |
| O2 | segment-vs-segment inexact near-parallel; `SEGSEG_EPS` dimensional double duty | **UPHELD — fix required** (new kernel primitive; drilling retargets; constant split; crate-root export) |
| O3 | degeneracy rule assumes a lattice the pass-through path violates | **UPHELD — fix required** (derivation rewrite + false doc claim + witness test; **no behavior change**) |
| O4 | ray directions not unit length; `DET_EPS` scale-dependent and verdict-deciding | **UPHELD — fix required** (normalize triage directions; determinant uncertainty band routes to Ambiguous, not Miss) |
| O5 | tolerances presented as derived are chosen | **UPHELD — fix required** (honest reclassification as chosen behavior pins; decision sites named; doctrine debt register; **no value changes**) |
| O6 | triage-rule ambiguity at `island_view_basis`; guard ineffective | **UPHELD — fix required** (site ruled independent; canonical 2-D `dot`/`perp_dot` added and two verdict-feeding sites consolidated; three named guard gaps closed) |
| EXT | O4/O5's deferral premise false: the parity path IS consumed by decisions today | **OWNER-RULED — fix required** (full `moller` consumer census; classified propagation through every live decision path; per-site instrumentation; D2 rewritten; implementation order amended) |

---

## O1 — ray/triangle half-ray semantics. REFUTED.

The reviewer warned that adopting the kernel's `t >= -EPS` (`EPS = 1e-6`) acceptance
into the containment parity test would count intersections behind the ray origin.
The landed code keeps half-ray semantics at the caller, exactly as the objection
demanded:

* `crates/plinth-mesh/src/triage.rs:1299-1311` — `TRIAGE_ORIGIN_EPS = 1e-9` maps
  `|t| <= 1e-9` to `Ambiguous` (covers the kernel's at-origin slack more
  conservatively than the kernel's own band), and every remaining `t < 0.0` returns
  `Miss`. A behind-origin crossing cannot reach the parity count.

Verified (revision 1 run, 2026-08-31), not assumed:

1. `cargo test -p plinth-mesh`: all green, including the three path-exercising
   probes — `a_real_hit_behind_the_ray_origin_is_a_miss_not_a_crossing`
   (a hand-derived `t = -5` hit reads as `Miss`),
   `triage_call_site_policy_still_owns_the_origin_band_and_half_ray_rule`, and the
   mirror-face probe at `triage.rs:1968-2010` whose behind-origin face flips the
   verdict if the half-ray rule is ever dropped.
2. Re-ran `cargo run --release -p plinth-mesh --example triage_verdict_table` on
   this tree: the output reproduces the committed
   `docs/evidence/geometry-consolidation/triage-verdict-table-after-c5.md` table
   body byte-for-byte, and the committed before/after tables differ only in their
   prose headers. The probe rows carry the load: the plain corpus never reaches the
   ray caster (`classifiers = 0` on all 8 rows), while the nested-box probes drove
   **1,659,744** measured ray/triangle evaluations (1,642,722 on the Weeping
   Treant) with identical verdicts, hit and miss counts.

The objection was correct as a warning and is moot against the code as landed. No
action. Note for sequencing: O4's fix regenerates this verdict table and re-runs
these probes (the `t` scale changes when the directions are normalized); the O1
RULING is unaffected — the half-ray policy lives at `triage.rs:1303-1311` and
survives the O4 change untouched in structure.

---

## O2 — segment-vs-segment distance near-parallel. UPHELD — fix required.

### The defect

`rust/geom_kernel/src/closest.rs:151` (`segment_segment_distance`): when
`denom = a·e − b² <= SEGSEG_EPS = 1e-14` the code takes `s = 0` and recovers by one
coordinate-descent step (one `t` update, one re-clamped `s` update). For merely
NEAR-parallel pairs that is not a complete minimisation, and the function's own doc
comment concedes it by measurement (excess up to 5.145e-6 mm at drain-keepout
scale).

The reviewer's counterexample, re-measured this adjudication:

```
p1 = (0, 0, 0)
q1 = (0.10780992493971708, 0, 0)
p2 = (0.01755930967909119, -0.09805348888039589, -0.011442695278674364)
q2 = (0.11837187720683315, -0.09805257753576863, -0.011442695278674364)

denom               = 9.65346509326781e-15 mm^4   -> the s = 0 branch fires
kernel returns        0.09871890374623554 mm
true minimum          0.09871809337695624 mm  at s = 1, t = 0.8952405822914677
excess                8.103692793054451e-7 mm
independent check: 4001x4001 dense sweep = 0.09871809338152179 (upper bound, consistent)
```

Every reviewer digit reproduces exactly. The excess is **8.1× the
`PROOF_EPSILON_MM = 1e-7` slack** of the comparison this value feeds:
`crates/plinth-hollow/src/drilling.rs:1472-1478` compares the returned distance
against `radius + radius + PROOF_EPSILON_MM` to raise `AmbiguousHolePair`. The
kernel's error is one-sided (it returns the distance between two points that really
lie on the segments, so it only ever OVERestimates), and an overestimate here turns
a refusal into an acceptance: a hole pair that should be refused as ambiguous can be
drilled. That can change an emitted file, so deferral is not available for this
objection.

### The fix: a second kernel primitive, not a caller-side guard

A caller-side guard in drilling would re-create private geometry in the exact file
the sweep just cleaned. Changing the pinned function is also ruled out: it is
TERM-FOR-TERM parity-pinned (Amendment A21) and consumed by the kernel's own
parity path (`rust/geom_kernel/src/queries.rs:1994`, `seg_tri_dist`, mirroring
`geometry_exact.py:596-630`). Therefore:

**Add to `rust/geom_kernel/src/closest.rs`:**

```rust
pub fn segment_segment_distance_min(p1: V3, q1: V3, p2: V3, q2: V3) -> (f64, V3, V3)
```

**And export it from the crate root** — this was revision 1's compile-breaking
omission. `rust/geom_kernel/src/lib.rs` is in Agent K's write-set; the line at
`lib.rs:67` becomes:

```rust
pub use closest::{segment_segment_distance, segment_segment_distance_min};
```

with the doc comment split accordingly: `segment_segment_distance` stays labelled
canonical FOR THE PARITY PATH; `segment_segment_distance_min` is labelled the
complete minimisation for decision-feeding callers. Without this export neither
Agent D's retarget nor the integration test can name the function.

Algorithm (complete minimisation over the interior and the four boundary edges of
the `(s, t)` unit square; the squared distance is a convex quadratic on the box, so
its minimum is either the unconstrained critical point or on one of the four faces,
and each face restriction is a 1-D convex quadratic with an exact closed-form
minimiser):

* Compute `d1, d2, r, a, e, f, c, b, denom` exactly as the pinned function does.
* Candidates, each clamped componentwise to `[0, 1]²`:
  1. interior `((b·f − c·e)/denom, (a·f − b·c)/denom)` — included only when
     `denom > 0.0` and both quotients are finite;
  2. `s = 0, t = clamp(f / e)`;
  3. `s = 1, t = clamp((b + f) / e)`;
  4. `t = 0, s = clamp(−c / a)`;
  5. `t = 1, s = clamp((b − c) / a)`;
  where the `t`-forms are guarded by `e > 0.0` (else `t = 0`) and the `s`-forms by
  `a > 0.0` (else `s = 0`).
* Evaluate `‖(p1 + s·d1) − (p2 + t·d2)‖` at every candidate and return the least,
  with its two points, exactly as the pinned function's return contract
  (`distance` is `dist` of the two returned points).

Note what disappears: **no epsilon constant at all.** The parallel case needs no
guard (for a parallel or near-parallel pair the minimum lies on a boundary edge,
where the closed forms are exact; an ill-conditioned interior candidate is merely
one more evaluated point and can never make the minimum worse). Zero-length
segments fall out of the exact `a > 0.0` / `e > 0.0` guards, so a genuinely nonzero
segment of ANY length is projected onto — the mm² mislabeling (below) cannot occur
on this path.

Measured validation of exactly this candidate scheme (scratch probe, revision 1
run): over **20,000 deterministic near-parallel pairs at 0.005–0.05 mm scale**
(19,990 of which fire the pinned function's `s = 0` branch),
`max(candidates − converged alternating descent from all four corners) = 0.0`
exactly, while the pinned function's excess on the same battery reached
**1.754e-6 mm**. The descent oracle was itself spot-checked against 2001×2001 dense
sweeps on 20 pairs with no violation.

**Retarget the caller:** `crates/plinth-hollow/src/drilling.rs:1959`
(`keepout_axis_distance`) calls `segment_segment_distance_min` instead. Drilling's
own refusals (zero-length axis, non-finite) stay where they are. The pinned
`segment_segment_distance` remains untouched and remains canonical for the parity
path only; its doc comment gains one sentence pointing decision-feeding callers at
`_min`.

### The objective test target

Add to drilling's tests (the reviewer's counterexample converted, expected values
measured this adjudication):

```rust
#[test]
fn near_parallel_keepout_pair_is_measured_by_complete_minimisation() {
    let p1 = [0.0, 0.0, 0.0];
    let q1 = [0.10780992493971708, 0.0, 0.0];
    let p2 = [0.01755930967909119, -0.09805348888039589, -0.011442695278674364];
    let q2 = [0.11837187720683315, -0.09805257753576863, -0.011442695278674364];
    let measured = keepout_axis_distance(p1, q1, p2, q2).expect("finite, non-degenerate");
    // Complete minimisation lands at s = 1, t = 0.8952405822914677.
    assert!((measured - 0.09871809337695624).abs() <= 1.0e-12);
    // The retired one-step answer was 0.09871890374623554 - 8.1037e-7 mm above the
    // true minimum, which exceeds the 1e-7 mm slack of the comparison this value
    // feeds. This test fails if the one-step branch ever comes back.
    assert!(measured < 0.09871890374623554 - 5.0e-7);
}
```

A mirror of this pair (same numbers, tolerance 1e-12) goes in the kernel's own
tests for `segment_segment_distance_min` — importing it THROUGH THE CRATE ROOT
(`geom_kernel::segment_segment_distance_min`), so the test also proves the export —
plus a deterministic near-parallel battery asserted against a dense-sweep oracle
(reuse the existing `brute_force_segment_minimum` pattern in `drilling.rs:1982` —
an oracle that is structurally different from the candidate enumeration, per the
standing rule that a test must not re-implement production logic).

### `SEGSEG_EPS` compared against mm⁴ and mm² — also upheld

In the pinned function the one constant `SEGSEG_EPS = 1e-14` guards `denom` (mm⁴)
and `a`/`e` (mm²). Measured consequence of the mm² use: a genuinely nonzero segment
of length 1e-7 mm has `a = 9.999999999999998e-15`, fails `a > SEGSEG_EPS`, and is
treated as zero-length (error in that configuration: 4.9e-15 mm — harmless in
magnitude, dishonest in contract). One number cannot be a correct threshold in two
different dimensions.

Fix, byte-neutral by construction: split the NAME, not the value. In
`rust/geom_kernel/src/consts.rs:29` replace the single `SEGSEG_EPS` with
`SEGSEG_DENOM_EPS_MM4: f64 = 1e-14` and `SEGSEG_LEN2_EPS_MM2: f64 = 1e-14`
(both documented as chosen behavior pins carrying the values of
`geometry_exact.py:298` verbatim — see O5), and use each at its dimensionally
matching comparison in the pinned function. Bit behavior is unchanged (same
literals), parity pins hold; the contract stops lying. The new `_min` function
uses neither.

### Required re-measurement

Retargeting `keepout_axis_distance` CAN change drilling verdicts on near-parallel
keepout pairs — that is the point. Agent D must rebuild and re-run the
14-artifact control head-to-head from the commit message of `487e71f`, verify the
binary postdates the fix commits before trusting any "no change" result, and
attribute every moved byte by bisect. Expected outcome is zero movement (the corpus
may hold no near-parallel keepout pair), but a movement here is the fix working and
gets reported with attribution, not smoothed over.

---

## O3 — the degeneracy rule's lattice premise. UPHELD — fix required (documentation and a witness test; no behavior change).

### The premise is false, and it was tested, not argued

`rust/geom_kernel/src/poly2.rs:95` derives
`DEGENERATE_EDGE_L2_MM2 = 2.5e-17 mm²` from the claim that every input is a
dequantised lattice point, so "no representable input sits near" the threshold.
`crates/plinth-layers/src/lib.rs:24` supports it with "stitched vertices — already
snapped to that grid — quantize losslessly."

Both claims fail on the pass-through path, live-tested (revision 1 run) against the
real producer (`normalize_layers_batch`, scratch probe with a path dependency —
convert this into the committed witness test):

* Input polygon `[(0,0), (1,0), (1,1), (bx, 0.9), (ax, 0.9)]` with
  `ax = 5e-9 − 1e-12` (quantises to key 0) and `bx = 5e-9 + 1e-12` (key 1):
  two genuinely distinct points in adjacent rounding cells,
  `|b − a| = 2.0000000000006135e-12 mm`, far below the 5e-9 mm half-quantum
  threshold.
* Measured output: `passed_through = true`; both original f64 coordinates emitted
  **bit-identical**; the two vertices are adjacent, forming a real edge of the
  normalized loop.
* Feeding that edge to `geom_kernel::poly2::point_segment_distance`:
  `l2 = 4.000000000002454e-24 < 2.5e-17` — the degenerate branch fires on an edge
  the producer asserts exists.

The mechanism is exactly where the reviewer pointed: the stitcher welds by
quantised KEY but stores the first ORIGINAL coordinate per key
(`crates/plinth-support/src/perception.rs:1764-1765`,
`points.entry(ka).or_insert(a)` — slicer output, not lattice values), and the
pass-through arm emits those originals verbatim
(`crates/plinth-layers/src/lib.rs:518-545`). Non-lattice coordinates are the NORM
on clean layers, not an upstream defect.

### Why the behavior nevertheless stays

The consequence of taking the degenerate branch is bounded by the edge itself:
`dist(p, a)` differs from `dist(p, segment)` by at most `|ab|`, and every edge that
can reach this branch has `|ab| < 5e-9 mm` — six orders of magnitude below the
14 µm pixel, at the scale of ordinary f64 rounding in the consumers. Measured on
the witness edge: predicate answer `0.49999999500100001` vs true minimum
`0.49999999499900000`, error `2.00001e-12 mm` — bounded by `|ab| = 2.00000e-12 mm`
plus ULP-scale rounding (the 1.1e-17 excess is evaluation rounding of the two
distances, not the branch). No division occurs on the branch, so no `inf`/`NaN` —
the robustness property that motivated the constant survives, for a different
reason than the one written down. The threshold value and the code do not change.

### The fix

1. **Rewrite the derivation** in `poly2.rs:60-96`: the honest justification is
   "worst-case error of the degenerate branch is bounded by the threshold length
   itself (≤ 5e-9 mm), and the branch excludes `inf`/`NaN`" — not lattice
   representability. Delete or qualify the "no representable input sits near it"
   knife-edge claim: pass-through edges can sit anywhere in `(0, one quantum)`,
   including at the threshold; both branches agree there to within 5e-9 mm, which
   is why the knife-edge is harmless rather than absent.
2. **Correct the false claim** at `crates/plinth-layers/src/lib.rs:23-26` and `:68`:
   vertices are welded by quantised key but carry original coordinates; only the
   KEYS quantize losslessly (the pass-through integer comparison itself remains
   exact and needs no change).
3. **Commit the witness test** (new file `crates/plinth-layers/tests/passthrough_verbatim.rs`)
   with the polygon, the bit-identity assertions, the adjacency assertion, and the
   bounded-error assertion above, so the next agent who "re-derives" a lattice
   premise trips over a red test instead of shipping it.

---

## O4 — ray directions not unit length; `DET_EPS` decides. UPHELD — fix required.

### Revision 1's refutation is withdrawn

The facts were never in dispute: the three fixed sampling directions at
`crates/plinth-mesh/src/triage.rs:1127-1131` have norms **1.0755641559,
1.0279214683, 1.0617650488** (measured), so `t`, the kernel's `±EPS` slack, and
`TRIAGE_ORIGIN_EPS` are in units of `|direction|`, not millimetres, and the
determinant compared against the absolute `DET_EPS = 1e-14` scales linearly with
`|direction|` — the same geometric ray under a rescaled direction vector can land
on the other side of the band.

Revision 1 refuted the CONCLUSION on the adjacent-face argument, quoted from
`rust/geom_kernel/src/moller.rs`'s own degenerate-input contract: "a ray parallel
to a face's plane has its true crossings registered on the adjacent non-parallel
faces … `DET_EPS` is therefore a PRUNE — the answer is taken from a neighbouring
face — not a verdict."

**That argument is wrong for interior crossings, and the challenger's construction
proves it.** A ray parallel to a face's plane (det exactly 0) indeed crosses the
closed surface at edges, where adjacent faces answer. But a ray NEARLY parallel to
the plane — `0 < |det| <= DET_EPS` — with its origin close to that plane pierces
the face INTERIOR at a real, finite `t`. No adjacent face contains that crossing
point. `moller.rs:93-95` (`valid = det.abs() > DET_EPS` → `hit = false`) reports
Miss, the crossing vanishes from the parity count, and the containment verdict for
every point beyond it flips. `DET_EPS` decides an outcome there; byte identity with
the retired private copy (the 1,659,744-evaluation table in §O1) proves the defect
is INHERITED, not absent.

Concrete instance, exact-arithmetic-verified this revision (every product below is
exact in f64: doublings of a stored value and products with 0):

```
triangle  v0 = (0,0,0), e1 = (2,0,0), e2 = (0,2,0)     (vertices (0,0,0),(2,0,0),(0,2,0))
ray       o  = (0.4, 0.5, -1e-16),  dv = (1.0, 0.0, 2e-15)   (|dv| = 1.0 in f64)

pvec = cross(dv, e2) = (-4e-15, 0, 2)
det  = dot(e1, pvec) = -8e-15          ->  0 < |det| <= DET_EPS: ray_triangle returns hit = false
true crossing: t = 1e-16 / 2e-15 ≈ 0.05,  point (0.45, 0.5, 0),  barycentric u = 0.225, v = 0.25
```

A real forward crossing through the face interior — `t` five million times the
origin band, barycentrics deep inside — asserted a Miss. The landed code has this
defect; the ruling is UPHELD.

### The fix — two parts, split kernel/caller like O2

**Part 1 (Agent K, kernel). A classified variant; the pinned function untouched.**
Add to `rust/geom_kernel/src/moller.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RayTriangleClass {
    /// `|det| > DET_EPS` and the full acceptance region is met.
    Hit(RayTriangleHit),
    /// `|det| > DET_EPS` and acceptance failed: a definite miss.
    Miss,
    /// `0.0 < |det| <= DET_EPS`: the determinant is nonzero but inside the
    /// uncertainty band. Whether the ray crosses this face is UNDECIDABLE at
    /// this scale — the caller chooses the conservative side.
    NearParallel,
    /// `det == 0.0` exactly: degenerate triangle, zero direction, or a ray
    /// exactly in the plane. No interior crossing exists; edge crossings live
    /// on adjacent faces. A genuine prune.
    ExactParallel,
}

pub fn ray_triangle_classified(o: V3, dv: V3, v0: V3, e1: V3, e2: V3, t_max: f64) -> RayTriangleClass
```

Same arithmetic sequence as the pinned `ray_triangle`, with `valid` surfaced
instead of folded into `hit`. The pinned `ray_triangle` is NOT edited (its miss on
the band is oracle parity, `geometry_exact.py:110-131`) except for its degenerate-
input contract doc: the paragraph claiming "This is correct rather than
conservative … a PRUNE — not a verdict" is rewritten to state the truth measured
here — exact-parallel is a prune; the nonzero band is an undecidable region that
the pinned function maps to miss FOR ORACLE PARITY ONLY, and decision-feeding
callers must use `ray_triangle_classified`. Export both from `lib.rs` (the
`pub use moller::{ray_triangle, RayTriangleHit}` at `lib.rs:88` gains
`ray_triangle_classified, RayTriangleClass`).

Agreement contract, enforced by a kernel test battery (not by refactoring):
for all inputs, `ray_triangle(..).hit == matches!(classified, Hit(_))`, and a
`Hit` carries bit-identical `t, u, v`. Plus the counterexample above as a pinned
test: `ray_triangle(..).hit == false` AND
`ray_triangle_classified(..) == NearParallel` on those exact literals.

**Part 2 (Agent D, triage). Normalize the directions; route the band to Ambiguous.**
In `crates/plinth-mesh/src/triage.rs`:

1. **Normalize the three sampling directions.** Keep the existing literals as
   `RAW_DIRECTIONS` (they document the chosen sampling angles); at use, divide
   each componentwise by `geom_kernel::vec3::norm(raw)` (canonical norm, OP-6).
   Scalar-by-component division is not consolidated vector algebra and trips no
   guard pattern. After this, `t` IS millimetres, `TRIAGE_ORIGIN_EPS = 1e-9` is
   genuinely 1e-9 mm, the kernel's `±EPS` t-slack is 1e-6 mm, and `DET_EPS` is
   compared against `2A·cos θ` with no direction-scaling term — the hit/miss
   boundary no longer depends on the representation of the direction. Update the
   UNITS comments at `triage.rs:1284-1289` and `:1296-1298`, which currently
   document the non-unit choice.
2. **Retarget `ray_triangle_distance` onto `ray_triangle_classified`** and map:
   `Hit` → existing `t` policy unchanged (origin band → `Ambiguous`, `t < 0` →
   `Miss`, else `Hit(t)`); `Miss` → `Miss`; `ExactParallel` → `Miss` (prune,
   per the corrected contract); **`NearParallel` → `RayHit::Ambiguous`** — which
   the existing machinery at `triage.rs:1193-1196` already escalates to an
   ambiguous ray, excluded from the 2-of-3 vote at `triage.rs:1141-1147`.
   Uncertainty routes to refusal; no new constant is introduced.

**Disclosed behavior risk, not smoothed over:** a near-degenerate sliver face with
tiny NONZERO determinant now poisons any ray reaching its bbox to `Ambiguous`
(conservative direction). If Agent D's corpus re-run shows an Ambiguous flood from
sliver faces, that is a measured finding to report with counts — the fallback
(escalating only above a measured sliver-area threshold) introduces a new deciding
constant and requires a new ruling; do not improvise it.

### Required re-measurement

Normalizing the directions changes every `det` and `t` in floating point, and the
band routing changes verdicts wherever `NearParallel` fires. This is the one fix in
this document EXPECTED to move measured artifacts. Agent D must: re-run the O1
probes, regenerate the triage verdict table, re-run the 14-artifact head-to-head,
and attribute every changed row and byte to its cause (O2's drilling retarget,
O4's normalization, O4's band routing, or O6's coefficient regrouping — see O6) by
bisecting the four changes. "It moved, the fix moved it" without attribution is
not acceptance evidence.

---

## O5 — tolerances presented as derived are chosen. UPHELD — fix required (honest reclassification; no value changes).

### The principle, corrected per the challenge

Revision 1 ruled most constants "chosen; honestly labelled" and declared that
sufficient. It is not. **"Oracle-pinned" is a provenance record — it says where a
number came from, not why it is right.** Under this project's doctrine a constant
is clean only when it provably cannot decide an outcome (a prune), or when the
uncertainty it bounds routes to refusal. A chosen constant that decides an outcome
is doctrine DEBT even when its label is honest. This ruling therefore does three
things: reclassifies every named constant as a **chosen behavior pin** unless a
real derivation exists, names every site where one decides an outcome, and either
fixes that site (O2, O4) or records the debt explicitly below.

### Per-constant adjudication

| Constant | Classification | Where it decides an outcome | Disposition |
|---|---|---|---|
| `SEGSEG_EPS = 1e-14` (`consts.rs:29`) | Chosen behavior pin (values of `geometry_exact.py:298`, verbatim) | DECIDED drillability: the near-parallel one-step branch it gates overestimates past drilling's 1e-7 mm slack (O2's counterexample) | **Fixed by O2**: the decision path moves to `segment_segment_distance_min`, which has NO epsilon; the pin survives only on the parity path. Name split per dimension (O2). ~~Residual debt: any FUTURE decision-feeding caller of the pinned function re-opens this~~ **Corrected by revision 3: a PRESENT decision-feeding caller exists** — `seg_tri_dist` (`queries.rs:1994`, edge candidates) feeds `segment_clearance*` and `faces_near_segments_batch`. Ruled in §EXT-2c: the decision chain moves to `segment_segment_distance_min`; the pin survives on the parity/test path only. |
| Möller `EPS = 1e-6` (`consts.rs:6`) | Chosen behavior pin (`geometry_exact.py:40`) | Decides at every acceptance-region edge: the barycentric shell width and the `t` slack. The shared-edge-consistency rationale in `moller.rs` argues the DIRECTION of the slack, not the magnitude 1e-6. | **Debt recorded** (register entry D1). Parity-pinned; failure shape at silhouette shells is a double-count merged by dedup. No measurement establishes 1e-6 over 1e-5 or 1e-7. Value unchanged. |
| `DET_EPS = 1e-14` (`consts.rs:10`) | Chosen behavior pin (`geometry_exact.py:122`) | DECIDED containment parity: a nonzero-band interior crossing is asserted a miss (O4's counterexample). The moller.rs claim that it is "a PRUNE — not a verdict" is measured FALSE for that configuration. | **Fixed by O4** on the decision path: the band routes to `Ambiguous` (refusal side). Residual debt (register entry D2): the band EDGE still decides where conservatism begins, and 1e-14 is underived; on the pinned parity path the band remains a silent miss for oracle parity. |
| `TRIAGE_ORIGIN_EPS = 1e-9` (`triage.rs:1300`) | Chosen behavior pin (carried from the retired copy) | Decides `Ambiguous` vs definite at the band edge. INSIDE the band uncertainty routes to refusal — the conservative direction. But a crossing at `t = 1.001e-9` is counted definite on no derived basis. | **Debt recorded** (register entry D3). After O4's normalization the value is at least genuinely 1e-9 mm rather than 1e-9 direction-lengths. Value unchanged. |
| crossing-dedup relative `1e-9` (`triage.rs:1200-1211`) | Chosen behavior pin, flagged in code | Decides whether two crossings within a relative 1e-9 merge; diverges from the kernel's `DEDUP_LOC_MM = 3e-3` on walls thinner than 3 µm | **Debt already recorded in code** (GEOMETRY-CANON EXEMPT block, OP-5). Stays where the code parked it (register entry D4 cross-references OP-5). |
| `DEGENERATE_EDGE_L2_MM2 = 2.5e-17` (`poly2.rs:95`) | Was presented as DERIVED — the one full-strength instance of the objection | Selects the point-fallback branch of `point_segment_distance` | **Fixed by O3**: the false lattice derivation is replaced by a true bound (branch error ≤ threshold length ≤ 5e-9 mm, and the branch excludes `inf`/`NaN`). That bound is a real justification, not a retro-fitted derivation — the challenger accepted it. Value unchanged. |

### The fix

1. **Agent K** rewrites the provenance labels in `rust/geom_kernel/src/consts.rs`
   (and the affected doc comments in `moller.rs`, `closest.rs`): every
   "oracle-pinned" annotation becomes "chosen behavior pin: value carried verbatim
   from `<oracle location>` for parity; not derived." No constant changes value.
2. **The doctrine debt register below is normative.** The canon replacement
   document (out of scope of this run's agents, recorded at the end) must carry it
   forward. Closing an entry requires a measurement, not a rationale.

### Doctrine debt register

* **D1 — Möller `EPS = 1e-6` magnitude.** Decides acceptance-region edges on both
  the parity and decision paths. Forced forward if: any emitted-file difference is
  ever attributed to the shell width, or the oracle is retired.
* **D2 — `DET_EPS = 1e-14` magnitude.** (Rewritten by revision 3; the previous
  entry treated production consumption of the determinant band as a future
  trigger. It is not a future trigger: it was live in the tree revision 2 ruled
  on.) The band decides where conservatism begins at every live consumer of the
  kernel's Möller walks — the collecting walk (`queries.rs:1781`) behind
  `cast_rays`, the penetration interval batches and `sphere_breach`; the
  counting walk (`queries.rs:1844`) behind `cast_rays_census` and
  `penetration_census_batch`; and the pierce candidate of `seg_tri_dist`
  (`queries.rs:2002`) behind `segment_clearance*` and
  `faces_near_segments_batch`. Those entry points feed, among others, the
  anchor seat list (`anchor.rs:1268`), foot admission (`anchor.rs:1828`,
  `brace.rs:1442`), brace and CCO clearance acceptance (`brace.rs:1881`,
  `:2972`, `cco.rs:4263`, `rewire.rs:1245`), drain planning and the
  drilled-topology proofs (`drains.rs:710/754`, `drilling.rs:1032/1044`,
  `:1628/1640`) — gates, refusals and emitted bytes, per the §EXT-1 census.
  After §EXT lands, `0 < |det| <= DET_EPS` propagates as explicit
  `NearParallel` uncertainty through every one of those paths and routes to
  the refusal/conservative side at each caller (§EXT-2), with a per-site,
  per-model firing count in the run output (§EXT-4). What REMAINS debt is the
  magnitude itself: 1e-14 is a chosen behavior pin, underived, in units that
  scale with `|direction| · 2A` (the walks take caller-supplied, non-unit
  directions — e.g. `anchor.rs:1268` passes raw point differences), so where
  the uncertainty band begins still moves with input representation. The
  pinned `moller`/`ray_triangle` keep the silent in-band miss for oracle
  parity only, and after §EXT-3 no gate, refusal or emitted byte can consume
  them unclassified. Forced forward if: the §EXT-4 counters show `NearParallel`
  moving dispositions on real corpus models (owner rules on the honest-refusal
  vs silent-answer trade then, with the counts in hand), or a derivation for
  the band width is ever proposed.
* **D3 — `TRIAGE_ORIGIN_EPS = 1e-9` magnitude.** Decides definite-vs-ambiguous at
  the origin band edge. Forced forward if: a verdict-table change is ever
  attributed to the band edge.
* **D4 — triage dedup relative `1e-9` vs kernel `DEDUP_LOC_MM`.** Already flagged
  in code as an OP-5 question; unchanged by this adjudication.

---

## O6 — the ambiguous site, and the guard. UPHELD — fix required.

### The named site: `island_view_basis` stays independent. Ambiguity removed by ruling.

`crates/plinth-pipeline/src/coverage_evidence.rs:871` (`island_view_basis`) is
ruled **independent — do not consolidate**. Grounds, verified:

* The module is refusal-side evidence only, by owner ruling
  (2026-08-12 FU-batch, quoted in the module doc): "never a gate input", never
  imported by `gates.rs` — confirmed against the import graph
  (`coverage_evidence` is consumed by `lib.rs`/`prepare.rs` evidence plumbing
  only).
* The site already carries the full exemption apparatus: `GEOM-EXEMPT` marker
  blocks (`coverage_evidence.rs:863-870` and `:935-938`) and thirteen line-pinned
  allowlist entries for `src/coverage_evidence.rs` in
  `crates/plinth-pipeline/tests/no_private_vector_algebra.rs:49 ff.`, with
  stale-entry detection that fails loudly if the lines move.

**The precedence rule, ratified so two careful agents stop disagreeing:** a site
consolidates unless a formula divergence at that site can change (a) a gate verdict
or (b) a byte of an emitted print-path artifact. A named evidence-only site on a
refusal path is exempt under both, stays independent, and must carry BOTH a
`GEOM-EXEMPT` marker and a line-pinned allowlist entry — which this site already
does. The literal "consolidate everything" reading of the rejected canon is dead;
this rule replaces it. **The rule admits no exception for inconvenience: a
verdict-feeding site with no kernel home gets a kernel home built, not a marker.**
Revision 1 violated its own rule here; the two mandates below are the correction.

### Two verdict-feeding 2-D sites consolidate onto new poly2 primitives

Revision 1 ordered a `GEOM-EXEMPT` marker added above
`crates/plinth-support/src/perception.rs:1705`. **That order is withdrawn.** The
site's own source comment (`perception.rs:1694-1702`) already rules it: "The value
does reach a verdict (the winding vote below), which makes this a MUST-CONSOLIDATE
site with no survivor — reported to the canon rather than improvised into one."
The code reported honestly; the canon must now answer with a survivor, not an
exemption.

**Agent K adds to `rust/geom_kernel/src/poly2.rs`** (closing the "missing 2-D
home" open item, evidence doc §8.1):

```rust
/// 2-D dot product, `a[0]*b[0] + a[1]*b[1]` — IN EXACTLY THAT OPERAND AND TERM
/// ORDER. The order is the contract: call sites consolidated onto this
/// function were measured bit-identical only under this order.
#[inline]
pub fn dot(a: [f64; 2], b: [f64; 2]) -> f64 { a[0] * b[0] + a[1] * b[1] }

/// 2-D perp-dot (scalar cross), `a[0]*b[1] - a[1]*b[0]` — same order contract.
#[inline]
pub fn perp_dot(a: [f64; 2], b: [f64; 2]) -> f64 { a[0] * b[1] - a[1] * b[0] }
```

**Agent L retargets the two production sites:**

1. `crates/plinth-support/src/perception.rs:1703` becomes
   `poly2::dot(segment, derived)` and `:1705` becomes
   `poly2::perp_dot(segment, derived)`. Both are **bit-identical by operand
   order** (`segment[0]*derived[0] + segment[1]*derived[1]` and
   `segment[0]*derived[1] - segment[1]*derived[0]` — verified against the source
   this revision). Delete the now-false "no survivor" exemption block at
   `:1694-1702`; keep the semantic comments (`d = z_hat x n ...`).
2. `crates/plinth-orient/src/section.rs:336-342` (function `quadratic`,
   `section.rs:305` — production code feeding `section_area_mm2_at`'s
   peak-section scoring). The three coefficients become:
   * `0.5 * poly2::perp_dot(b_down, b_up)` — bit-identical (pure perp-dot,
     operand order preserved);
   * `0.5 * (poly2::perp_dot(a_down, b_up) + poly2::perp_dot(b_down, a_up))` —
     **NOT guaranteed bit-identical**: the landed expression evaluates
     `((p1 + p2) - p3) - p4` and the consolidated form evaluates
     `(p1 - p3) + (p2 - p4)`, a regrouping with a possible last-ULP difference;
   * `0.5 * poly2::perp_dot(a_down, a_up)` — bit-identical.
   The middle coefficient's ULP risk feeds orientation scoring and can in
   principle change a picked orientation, hence an emitted file. Under the
   precedence rule the site must consolidate anyway (leaving a verdict-feeding
   perp-dot pair inline with a marker is the exact violation being corrected);
   the risk is DISCLOSED and assigned: Agent D's head-to-head runs after this
   retarget, and any movement is attributed to its cause by bisect (§O4).

**Classified test-code, no action:** `crates/plinth-orient/src/score.rs:1251`
sits inside `#[cfg(test)] mod tests` (opens at `score.rs:1190`);
`crates/plinth-hollow/src/lib.rs:4543` sits inside the test module opening at
`lib.rs:4076`. Both verified this revision. They remain MAY-STAY test-code hits
under Guard 2's classification — a test re-implementing the formula it checks
against is the oracle-independence the standing rule requires.

### The guard: the "catches nothing inline" claim is refuted — and three real gaps are upheld

The claim "the generated grep gate catches private function declarations and
`.mul_add`, not the ~134 ordinary inline dot/cross/squared-length expressions" is
false of the landed tree. It describes only the per-crate string guards
(`tests/no_private_vector_algebra.rs`). The landed commit ALSO ships Guard 2
(`rust/geom_kernel/tests/kn_no_inline_geometry_guard.rs` +
`tests/guard_scripts/no_inline_geometry.sh`): four **ast-grep structural
patterns** — 3D dot, 3D cross, 2D and 3D squared length — that match exactly the
inline-expression class the reviewer says is uncovered, with metavariables, so
variable naming is irrelevant. Measured (revision 1 run): **scanned 91 `.rs` files
across 13 src roots; 20 raw hits → 20 test-code (MAY-STAY), 0 GEOM-EXEMPT, 0
violations**, and its self-test (`detector_fires_on_an_unmarked_site_...`) plants an
unmarked dot product every run and passed. The "~134" is the PRE-sweep census
(56 + 24 + 24 + 30, quoted in the guard's own module doc); the sweep consolidated
those sites, which is why only 20 test-code hits remain.

Three gaps are real, measured, and are the required fixes:

1. **No 2D-dot or 2D-perp-dot pattern.** Add to `no_inline_geometry.sh`:
   `DOT2D = '$A[0] * $B[0] + $A[1] * $B[1]'` and
   `CROSS2D = '$A[0] * $B[1] - $A[1] * $B[0]'`. Measured candidate hits on the
   pre-fix tree: DOT2D 8, CROSS2D 5. After Agent L's retargets above, the two
   production sites (`perception.rs:1703,1705`; `section.rs` coefficients) no
   longer match — the expected post-fix classification is **zero production
   hits**, with the `score.rs` and `hollow/lib.rs` hits classified test-code.
   Agent L asserts the post-fix counts from a FRESH guard run, not from this
   document, and the new patterns land in the SAME commit as the retargets so the
   tree is green at every commit.
2. **`.mul_add`-composed algebra is invisible to Guard 2** (an FMA dot matches none
   of the four shapes) and the per-crate string guards that DO ban `.mul_add(`
   exist in only 10 of the 12 workspace crates plus none in `rust/chitubox_io`.
   Missing: **`plinth-cli`, `plinth-verify`, `rust/chitubox_io`**. `plinth-verify`
   computes gate verdicts — the worst possible location for the blind spot. Add the
   standard `tests/no_private_vector_algebra.rs` (empty allowlist, counted-row
   gates, positive control) to all three. Measured: none of the three currently
   contains `.mul_add` in `src/`, so all three land green.
3. **The kernel exempts itself** (`segment_batch.rs` private `fn dot`, plus a
   fourth norm spelling) — already recorded as open item §8.4 of
   `docs/evidence/geometry-consolidation-result.md`. Noted here so it is not
   re-discovered; it stays with that document's owner, and is not expanded by this
   ruling.

Every guard change lands under Guard 2's existing discipline: counted rows
(files-scanned and hits are asserted non-vacuous), and the self-test extended to
plant one unmarked instance of EACH new pattern — a guard that cannot fail is the
failure mode this project keeps reproducing, so each new pattern must prove it can.

---

## EXT — the O4/O5 deferral extended to every live `moller` decision path. OWNER-RULED.

The challenger's instruction, ruled correct by the owner, is the mandate:
retarget every live decision-feeding `moller` use in
`rust/geom_kernel/src/queries.rs` onto classified handling; propagate
`NearParallel` as explicit uncertainty through `cast_rays_census`, `cast_rays`,
the penetration APIs and the segment-to-surface distance chain so callers
refuse rather than observe a missing crossing or a false positive distance;
keep the pinned function only where its oracle miss cannot decide a gate or an
emitted byte; and stop describing production parity consumption as a future
trigger.

The disclosed risk is restated, not softened: propagating uncertainty means
callers may refuse where they silently answered, and models that emit today
(24 of 37) may stop emitting. The ruling is to MEASURE that, per §EXT-4, and
put any disposition movement in front of the owner as a finding — an honest
refusal against a possibly-wrong silent answer is the owner's call, not this
document's and not an implementing agent's.

### EXT-1 — the census of `moller` consumers

**Method, so it can be re-run rather than trusted:** every `moller` and
`ray_triangle` reference in `crates/` and `rust/` was enumerated by grep on
this tree (2026-08-31); every reachable public entry point was then traced to
every workspace call site of its name; each call site was classified
production vs test by its position against its file's `#[cfg(test)] mod tests`
line, verified per file. Counted rows, per the standing rule: **3** `moller`
call sites in kernel production code (`queries.rs:1781`, `:1844`, `:2002`),
**1** production `ray_triangle` consumer outside the kernel
(`triage.rs:1289`), **21** production call expressions of the moller-backed
public entry points across **17** consumer sites (table below), **0**
`geom_kernel` references in
`crates/plinth-verify/src` and **2** in `crates/plinth-pipeline/src` (both in
`coverage_evidence.rs`, the O6 evidence-only exempt module — neither is a ray
or segment query).

**Kernel-internal chains (all three sites, with every public exit):**

| `moller` site | internal chain | public entry points |
|---|---|---|
| `queries.rs:1781` (collecting walk, `collect_hits_with_stack`) | ← `collect_hits_into`:1735 ← `collect_hits`:1726; ← direct at `pen_intervals_one`:2556; ← `cone_thickness`:3312 | `cast_rays`:1941 (via `cast_one`:1866); `penetration_intervals_batch_census_limits`:2976, `_bounded`:3080, `_bounded_metered`:3121 (via `pen_intervals_one`:2517); `sphere_breach`:3414 (via `breach_one`:3331 → `cone_thickness`:3300) |
| `queries.rs:1844` (counting walk, `count_hits_with_stack`) | ← `cast_rays_census`:1924; ← `census_one`:2827 | `cast_rays_census`:1910; `penetration_census_batch`:2848 |
| `queries.rs:2002` (`seg_tri_dist`, pierce candidate) | ← `seg_surface_min`:2019 / `seg_surface_min_excluding`:2043 ← `seg_clear_one_excluding`:2116; ← `faces_near_segment`:2479 | `segment_clearance_typed`:2181, `segment_clearance`:2220 (lossy shim), `segment_clearance_excluding_typed`:2283, `segment_clearance_excluding`:2326 (lossy shim) — all four funnel into `seg_clear_one_excluding` (verified: the non-excluding forms delegate with an empty exclusion set); `faces_near_segments_batch`:2375 |

Plus the exported primitive itself: `moller::ray_triangle` (`moller.rs:112`
carries the arithmetic; `moller` is its `(hit, t)` projection), re-exported at
`lib.rs:88`, consumed in production only by
`crates/plinth-mesh/src/triage.rs:1289` (`ray_triangle_distance`) — already
retargeted onto `ray_triangle_classified` by §O4 Part 2.

**Measured NON-consumers, so nobody re-audits them:** `inside`
(`queries.rs:1710`) runs on `ParityVote` (`parity.rs`), a separate z-column
parity engine with no `moller` reference (workspace grep) — see the out-of-scope
note below; the `project`/`query`/nearest chain runs on `closest_on_tri`;
`penetration_intervals_batch_worst_case_limits`:3048 is pure arithmetic (body
verified: it calls only `penetration_requirements`); `capsule_rays`:80 is a ray
GENERATOR (consumed by `designed_embed.rs:1893/:2119` for row/charge
construction — no surface walk); `testkit` is feature-gated
(`lib.rs:55-56`) with zero production consumers (the one `crates/` use,
`seat_zone.rs:534`, is inside its test module).

**The production consumers** — triage's direct primitive use (row 1) plus the
17 entry-point consumer sites (21 call expressions; rows 14 and 16–18 each
hold two) — each with what it decides and its `NearParallel` disposition. "Refuse/conservative"
column entries are ruled here; justifications follow the table. Labels are the
§EXT-4 instrumentation keys.

| # | Call site | Enclosing fn | Entry point | Decides | `NearParallel` disposition | Label |
|---|---|---|---|---|---|---|
| 1 | `crates/plinth-mesh/src/triage.rs:1289` | `ray_triangle_distance` | `ray_triangle` | containment parity → triage verdict (gate/refusal) | already ruled (§O4): → `Ambiguous`, excluded from the 2-of-3 vote | `triage.rays` |
| 2 | `crates/plinth-mesh/src/delicacy_eval.rs:363` | `thinness_by_face` | `cast_rays` | per-face thinness → delicacy → orientation scoring and scar cost (emitted bytes) | conservative: the face's thinness saturates to the THIN extreme (max delicacy contribution) and the event is counted; **owner reviews if it ever fires on corpus** (see EXT-2 justification 2) | `mesh.delicacy.thinness` |
| 3 | `crates/plinth-support/src/lib.rs:959` | `perceive_contacts_on_surface` | `cast_rays` | top-down grounded-face perception → support routing | RULED (rev 4): a `NearParallel` on a column's ray changes no `grounded` bit — the skipped crossing is treated as absent and the affected faces STAY candidates; `grounded` is an exemption and grows only on proof (justification 3); count per column | `support.perception.columns` |
| 4 | `crates/plinth-support/src/anchor.rs:1268` | `discover_anchor_seats_bound` | `cast_rays` | anchor seat list — where supports attach (emitted bytes) | refuse the CANDIDATE: new `AnchorSeatRefusalReason` variant naming near-parallel uncertainty; search continues over remaining candidates; count | `support.anchor.seat_snap` |
| 5 | `crates/plinth-support/src/anchor.rs:1828` | `validate_anchor_feet_bound` | `sphere_breach` | foot admission (gate) | uncertain row = treated as BREACHED (foot rejected); count | `support.anchor.foot_breach` |
| 6 | `crates/plinth-support/src/brace.rs:1442` | `gather_candidates` | `sphere_breach` | brace candidate pruning | uncertain row = treated as breached (candidate dropped — prune side; a dropped candidate costs coverage, and missing coverage is itself refused downstream); count | `support.brace.candidate_breach` |
| 7 | `crates/plinth-support/src/brace.rs:1881` | `evaluate_candidates` | `segment_clearance_excluding` (lossy float shim) | brace candidate acceptance (emitted bytes) | migrate to the typed form (the shim's own doc already mandates it); `UpperBound` row does NOT clear; count | `support.brace.clearance` |
| 8 | `crates/plinth-support/src/brace.rs:2070` | `axis_illegal_rows` | `penetration_intervals_batch_bounded` | illegal-axis detection → candidate rejection | uncertain row = row ILLEGAL (conservative); count | `support.brace.axis_penetration` |
| 9 | `crates/plinth-support/src/brace.rs:2972` | `exact_clearance_rows_detailed` | `segment_clearance_excluding` (lossy shim) | final-gap acceptance (emitted bytes) | migrate to typed; `UpperBound` does not clear; count | `support.brace.final_gap` |
| 10 | `crates/plinth-support/src/cco.rs:4263` | `clearance_survival` | `segment_clearance_excluding` (lossy shim) | CCO candidate survival (emitted bytes) | migrate to typed; `UpperBound` does not clear; count | `support.cco.clearance` |
| 11 | `crates/plinth-support/src/rewire.rs:1245` | `validate_changed_geometry` | `segment_clearance_excluding_typed` | rewire acceptance (emitted bytes) | already `clears_or(false)` (`rewire.rs:1271`); `UpperBound` maps to not-cleared under the extended predicate; count | `support.rewire.clearance` |
| 12 | `crates/plinth-support/src/seat_zone.rs:491` | `freeze_seat_zones` | `faces_near_segments_batch` | seat exemption bands — which faces later clearance walks EXEMPT | a `NearParallel` pierce candidate must NOT add a face to a band (an exemption may grow only on proof); count (see EXT-2 justification 12) | `support.seat_zone.bands` |
| 13 | `crates/plinth-support/src/initial.rs:763` | `build_initial_contacts` | `penetration_census_batch` | resource limits for the fill pass (census) | accounting only — the census count is unchanged by classification (identical hit predicate in both walks); the near-parallel tally is still returned and must equal the fill pass's tally per row (lockstep, `CensusDrift` discipline) | `support.initial.census` |
| 14 | `crates/plinth-support/src/initial.rs:774` and `:1462` | `build_initial_contacts` / `classify_initial_overlaps` | `penetration_intervals_batch_census_limits` / `_bounded_metered` | initial contact classification → support routing | uncertain row → `OverlapMeasure::NearParallelLowerBound`: the stub is CONDEMNED (quarantined) with named evidence, never reduced to a definite overlap (justification 14 — the typed path revision 3 cited did not exist and is defined there); count | `support.initial.contacts` |
| 15 | `crates/plinth-support/src/designed_embed.rs:298` | `witness_batch` | `penetration_intervals_batch_bounded` | designed-embed connectivity witness (gate) | uncertain row = witness INVALID → embed refused; count. (`designed_embed.rs:94` calls `_worst_case_limits` — pure arithmetic, not a census site) | `support.embed.witness` |
| 16 | `crates/plinth-hollow/src/drains.rs:710` / `:754` | `plan` | `cast_rays_census` + `cast_rays` | drain/vent exit paths → whether the model hollows, and hole placement (refusal + emitted bytes) | a `NearParallel` on a candidate's ray marks that CANDIDATE unavailable (existing `unavailable_candidates` plumbing); if that empties a required candidate set the plan refuses with a typed reason naming near-parallel uncertainty; count. **This is disclosed-risk site number one for the emit count.** | `hollow.drains.plan` |
| 17 | `crates/plinth-hollow/src/drilling.rs:1032` / `:1044` | `prepare_audit_plan` | `cast_rays_census` + `cast_rays` | audit drain selection (refusal + emitted bytes) | same contract as 16: affected audit candidate unavailable; typed refusal if selection becomes impossible; count | `hollow.drilling.audit` |
| 18 | `crates/plinth-hollow/src/drilling.rs:1628` / `:1640` | `prove_centerlines` | `cast_rays_census` + `cast_rays` | centerline crossing proof → `DrillingRefusal` (whether a model can be hollowed) | `NearParallel` on a hole's ray → that hole's proof is REFUSED with a typed reason naming the uncertainty (not the blanket `InvalidDrilledTopology`, which currently also swallows `BatchInputError` — the refusal must say what it saw); count | `hollow.drilling.centerlines` |

Test-code call sites (measured, classified by test-module position):
`drilling.rs:3326`; `drains.rs:2236/:3300/:3303/:3437`; `anchor.rs:2935/:2957/:2967`
(and the boundary test at `:3021`); `brace.rs:3652/:5834/:6108`;
`cco.rs:8700/:8746/:8803/:8914`; `contact.rs:761/:811` (contact.rs has NO
production clearance call — its test module opens at `:577`); `initial.rs:2494+`;
`seat_zone.rs:1059/:1135`; `rust/geom_kernel/src/lib.rs:1059` (after the
`#[cfg(test)]` at `:872`); the geom_kernel `tests/` and `testkit.rs` files.

**Test migration ownership (rev 4).** Revision 3's sentence "they keep whatever
API their assertions pin" stood in direct contradiction with K2's deletion gate
and is withdrawn. What survives K2 unchanged is only a test pinned to a
SURVIVING pinned primitive (`moller::ray_triangle`, `moller::moller`,
`segment_segment_distance`). Every test call to a REMOVED entry point migrates
to its classified twin (EXT-2 name table) before K2's gate runs; expected
values are unchanged wherever no tally fires — that is the walk-level agreement
contract — so these are call-shape migrations, not assertion changes. Inline
`#[cfg(test)]` sites migrate with their file's owning lane (the write-sets all
say "including inline tests"). The EXTERNAL test files, enumerated and counted
on 2026-09-01 (89 old-API call expressions in 5 files; 0 in any example, and no
bench targets exist):

| file | old-API calls | migrating lane |
|---|---|---|
| `rust/geom_kernel/tests/native_api.rs` | 52 | K2, same commit as the removal |
| `rust/geom_kernel/tests/face_excluded_clearance.rs` | 20 | K2 |
| `crates/plinth-support/tests/penetration_census_equivalence.rs` | 10 | S (file added to S's write-set) |
| `rust/geom_kernel/tests/kn_segment_clearance_unmeasured.rs` | 6 | K2 — and it gains the third-state pin: an `UpperBound` row is neither `Measured` nor `Unmeasured` |
| `rust/geom_kernel/tests/near_wall_thin_feature.rs` | 1 | K2 |

**Completeness is enforced structurally, not by this table.** The census above
was measured, but per EXT-3 the unclassified batch entry points are REMOVED at
the end of the implementation sequence, so a consumer this table missed fails
to compile rather than surviving silently. A grep row is not the gate; the
compiler is.

### EXT-2 — the classified API and its propagation contract

**At the primitive** (ruled in §O4, restated): `NearParallel` =
`0 < |det| <= DET_EPS` — whether the ray crosses this face is undecidable at
this scale; `ExactParallel` = `det == 0.0` exactly — no interior crossing
exists (a genuine prune; edge crossings register on adjacent faces). `t`, `u`,
`v` from a non-`Hit` are the guarded `inv = 0` products and MUST NOT cross any
API boundary; the classification and (for evidence) the face id are all that
propagate. One scoping fact bounds the blast radius: the walks only evaluate
Möller on faces whose padded bbox the ray reaches within `[-EPS, t_max + EPS]`
(`ray_box` broad phase), so a `NearParallel` tally is confined to faces the ray
could actually have crossed.

**At the walks (kernel).** Both walks adopt the classification with the SAME
arithmetic: the accepted-hit set is bit-identical to today's (`hit` ⇔ `Hit(_)`
is the §O4 agreement contract, extended to the walks by test), and each walk
ADDITIONALLY tallies `NearParallel` per ray. Every result whose tally is zero
is therefore byte-identical to the current tree — the head-to-head must confirm
exactly that on rows where nothing fired. The two walks are mirror twins by
standing rule; the tally is part of the mirror (census tally must equal
collecting tally per ray, enforced through the existing `CensusDrift` refusal).

**At the batch entry points — the names, final (rev 4).** Every batch entry
point is an inherent method on `NativeSurface` (`queries.rs:1115`), and Rust
does not overload inherent methods, so the classified twins land under
DISTINCT names. Those names are FINAL: K2 deletes the old names and does NOT
rename the twins back — reusing a deleted name would blunt the counted grep
gate that proves the removal, for no gain a later canon ruling could not
deliver on its own schedule. The table (K adds the left column's twin; K2
removes the left column's name):

| removed at K2 | classified twin (final name) |
|---|---|
| `cast_rays` (`queries.rs:1941`) | `cast_rays_classified` |
| `cast_rays_census` (`:1910`) | `cast_rays_census_classified` |
| `penetration_census_batch` (`:2848`) | `penetration_census_batch_classified` |
| `penetration_intervals_batch_census_limits` (`:2976`) | `penetration_intervals_batch_census_limits_classified` |
| `penetration_intervals_batch_bounded` (`:3080`) | `penetration_intervals_batch_bounded_classified` |
| `penetration_intervals_batch_bounded_metered` (`:3121`) | `penetration_intervals_batch_bounded_metered_classified` |
| `sphere_breach` (`:3414`) | `sphere_breach_classified` |
| `segment_clearance` (`:2220`, float shim) | none — callers move to the classified typed form |
| `segment_clearance_excluding` (`:2326`, float shim) | none — same |
| `segment_clearance_typed` (`:2181`) | `segment_clearance_classified` |
| `segment_clearance_excluding_typed` (`:2283`) | `segment_clearance_excluding_classified` |
| `faces_near_segments_batch` (`:2375`) | `faces_near_segments_batch_classified` |

NOT removed: `penetration_intervals_batch_worst_case_limits` (`:3048`) — pure
arithmetic, no Möller consumption (EXT-1's measured non-consumer list); it
keeps its name and its callers. The two-state `SegmentClearance` row type is
superseded by the three-state classified row type and is deleted with its
entry points at K2 (its external pins migrate per the test-ownership table).

Each classified twin returns its predecessor's existing payload PLUS per-row
uncertainty:

* `cast_rays` / `cast_rays_census` → per-ray `near_parallel: u32` (a vector
  parallel to the row order, or a field beside the ragged offsets — shape is
  implementer freedom; silent loss is not).
* `penetration_intervals_batch_*` / `penetration_census_batch` → per-row tally.
* `sphere_breach` → per-sphere tally (any cone ray's `NearParallel` marks the
  sphere's row).
* the typed clearance family → rows carry a third state beside `Measured` and
  `Unmeasured`: `UpperBound { clearance_mm, witness, near_parallel_faces }` —
  the walk DID measure, but a `NearParallel` pierce candidate was skipped, so
  the true clearance is AT MOST `clearance_mm` and may be lower, including
  below zero. This is deliberately not `Unmeasured` (a number exists; it is
  one-sided) and not `Measured` (it cannot affirm a gap). The clears
  predicate restates BOTH fallbacks explicitly (`when_unmeasured` and
  `when_upper_bound`, no defaults): a refusing stage maps `UpperBound` to
  not-cleared; a prune stage may keep the candidate alive only if a later
  refusing stage retests it, and must say which one in a comment.
  **Sequencing note:** adding a variant to the existing `SegmentClearance`
  would break every exhaustive match in one commit, so the extended type and
  its entry points land under new names in the additive K lane; the
  compile-forced enumeration of call sites happens at K2, when the old forms
  are deleted.
* `faces_near_segments_batch` → per-row tally, with the membership rule of
  justification 12 below.
* The float shims `segment_clearance` / `segment_clearance_excluding` are
  lossy by their own doc ("new code must not call this") and CANNOT carry the
  new state. Their three remaining production callers (sites 7, 9, 10) migrate
  to the classified forms as part of this ruling; the shims then have zero
  production callers (counted row required) and are removed with the other
  unclassified entry points in EXT-3's removal step.

**At the callers.** The dispositions are in the EXT-1 table. The governing
rule, which every disposition above instantiates: **a caller may tolerate
`NearParallel` only where the uncertain answer can move the outcome exclusively
toward refusal or pruning; wherever it could move the outcome toward an
acceptance, a pass, or an emitted byte, the row refuses, fails, or goes
ambiguous.** One clarification, ratified by the site-2 ruling below and applied
at site 3: where the only movement uncertainty can cause is toward the
PROTECTIVE side (more delicacy, more candidacy for support), the movement may
touch emitted bytes and is tolerated as count-and-surface — the tally makes it
visible and the owner reviews if it fires; what is never tolerated is movement
toward a pass, an acceptance, or an exemption. Justifications for the
non-obvious rows:

* **(2) delicacy.** A silently missed crossing OVERSTATES thickness (the ray
  saturates at `thin_feature_mm`), understating fragility — the dangerous
  direction, since delicacy exists to protect fragile geometry. The
  conservative extreme is the thin side. This does bias orientation scoring
  toward treating the face as delicate, which is why the disposition is
  count-and-surface: if the counter ever reads nonzero on a corpus model, the
  owner sees the count and rules; the mapping is not silently load-bearing.
* **(3) grounded columns — ruled in revision 4; the challenger's demanded
  direction is REFUTED.** The challenger demanded that an affected column mark
  its candidate faces `grounded = true` (excluding them from candidacy),
  reading "`!grounded` admits a support candidate" as the permissive
  absorption. Three measured facts rule the other way. **First**, `lib.rs:973`
  sets `grounded[face] = below.is_finite() && face_z - below <=
  config.bridge_free_gap_mm.get()` — `grounded = true` is a MEASURED-DISTANCE
  claim, "a surface exists within the bridge gap below this face". A
  `NearParallel` event carries no usable `t` (the primitive contract above:
  non-`Hit` products must not cross any API boundary), so affirming
  `grounded` from one asserts a distance nobody measured — mapping
  not-measured to the site's pass state, the exact doctrine violation this
  extension exists to remove. **Second**, `grounded` is an EXEMPTION from the
  support machinery, and the ratified justification-12 principle applies
  verbatim: an exemption may grow only on proof. The challenger's rule is the
  seat-band mistake with a different boolean. **Third**, the failure
  asymmetry: perception owns island discovery (`lib.rs:995-996` — S1 consumes
  perception's sidecar rather than rediscovering islands), so a face wrongly
  exempted here never enters an island and no downstream coverage refusal can
  see it — an unsupported overhang, print failure with no refusal anywhere.
  A face wrongly ADMITTED flows into seat discovery, foot validation and
  clearance acceptance — sites 4, 5, 7–11, every one classified by this same
  ruling — and ends as a measured seat or a typed refusal; its worst case is
  a redundant support: cost, visible, counted. Candidacy routes a face INTO
  scrutiny; grounding routes it out. The challenger mistook remediation for
  acceptance. **Ruling:** a `NearParallel` on a column's ray changes no
  `grounded` bit — the skipped crossing is treated as absent, which is both
  what the arithmetic already does and the fail-closed side — the event is
  counted per column under `support.perception.columns`, and the tally rides
  the perception result to the run report (§EXT-4). The earlier delegation to
  the implementing agent is withdrawn; the challenger was right that this
  belonged in the spec, and wrong about which way it goes.
* **(12) seat bands.** Bands are EXEMPTIONS: faces in a band are excluded from
  later clearance walks. Adding a face on an unproven pierce would grow an
  exemption on uncertainty — the permissive direction. Omitting it keeps the
  face in later clearance tests, where a genuine touch fails the row — the
  refusal direction. So membership stays proof-only (`d <= radius` from
  measured candidates), and the tally makes the omission visible instead of
  silent. Note the asymmetry with sites 7/9/10/11: the SAME `seg_tri_dist`
  uncertainty is band-tolerable and clearance-intolerable, because the two
  consumers sit on opposite sides of the exemption.
* **(13) census rows.** The census decides allocation charges, not geometry.
  Its crossing count is identical under classification (same hit predicate),
  so no census-driven refusal changes; the tally rides along solely for the
  lockstep check.
* **(14) initial-contact overlaps — the typed state, defined (rev 4).**
  Revision 3 said the uncertain row uses "the module's typed refusal/unknown
  path"; the challenger is right that no such path exists —
  `classify_initial_overlaps` (`initial.rs:1440`) reduces each seed's interval
  rows to a bare `f64` via `remaining_illegal_axis_overlap` (`initial.rs:1645`),
  and the one consumer decision is the boolean
  `initial_overlap_exceeds_cutoff(*overlap, cutoff, ..)` at `initial.rs:941`,
  where a LARGER overlap condemns. A skipped crossing UNDERSTATES the interval
  set, so an affected row's overlap is a LOWER BOUND — the true illegal
  overlap can only be equal or higher — and an understated overlap can move a
  seed from condemned to kept: absorption in the permissive direction.
  Revision 4 defines the path. `classify_initial_overlaps` returns
  `Vec<OverlapMeasure>` in place of `Vec<f64>`, with two arms:
  `Measured(f64)` and
  `NearParallelLowerBound { overlap_mm: f64, near_parallel: u32 }`.
  A `NearParallelLowerBound` row is CONDEMNED unconditionally — quarantined
  through the same `forest.quarantine` walk as an over-cutoff overlap — and
  recorded in a new evidence vector on `InitialContactBuild`
  (`uncertain_overlap: Vec<UncertainInitialStub>`, carrying terminal, key, the
  lower-bound overlap and the tally), under the module's existing discipline
  that every quarantine reason is recorded even when several fire
  (`initial.rs:891`). Condemnation is candidate-granular — one uncertain seed,
  one quarantined stub, the build continues — and sits on the refusal side by
  the same closed load path as site 6: a withheld stub costs coverage, and
  missing coverage is refused downstream. The type changes precisely because a
  bare `f64` cannot say "lower bound"; no sentinel value is invented.
* **(16–18) hollowing.** Candidate-granular refusal, not batch-granular: one
  uncertain ray kills one candidate/hole, and only an emptied required set
  refuses the plan. This is the narrowest honest propagation — and still the
  place a flood would land first, hence disclosed-risk site number one.

**EXT-2c — the segment chain's OTHER inexactness, same ruling.** Inside
`seg_tri_dist`, the three edge candidates use the pinned
`segment_segment_distance` (`queries.rs:1994`), whose near-parallel one-step
recovery is a measured one-sided OVERestimate (§O2: 8.1e-7 mm on the pinned
counterexample, 1.754e-6 mm max on the 20,000-pair battery). Every
`seg_tri_dist` error is therefore one-sided-high (endpoints exact via
`closest_on_tri`; edges overestimate; a missed pierce turns 0 into positive) —
which is exactly the false-positive-distance shape the mandate names, and
`segment_clearance` compares that distance against a demanded gap. Ruling:
`seg_tri_dist` retargets its edge candidates onto
`segment_segment_distance_min` (the O2 primitive — no epsilon, complete
minimisation), and its pierce candidate onto the classified Möller per this
section. The pinned `segment_segment_distance` keeps kernel-parity-test and
`_min`-oracle-cross-check duty only. CONSEQUENCE, disclosed: `seg_tri_dist` is
the mirror of `_seg_tri_dist` (`geometry_exact.py:596-630`); byte parity with
the oracle on near-parallel edge configurations is retired ON PURPOSE for the
decision chain. If a committed kernel parity test pins `seg_tri_dist` bytes on
such a configuration, it will move; the expected values update in the same
commit with the movement attributed — this adjudication did not enumerate the
parity-test corpus, so whether any such pin exists is flagged, not asserted.
Band membership (`faces_near_segment`) inherits `_min` too: distances only
tighten, bands only grow toward truth, and the fail-closed direction is
preserved.

### EXT-3 — where the pinned function legitimately stays, with proof

* **`moller::moller` / `moller::ray_triangle` as parity pins.** After this
  ruling their consumers are: the kernel parity tests
  (`rust/geom_kernel/tests/geometry.rs`, `canon_contracts.rs`,
  `kn_import_rule_guard.rs`), `testkit` (feature-gated at `lib.rs:55-56`, zero
  production consumers — measured), and the §O4 agreement battery that holds
  `ray_triangle` and `ray_triangle_classified` bit-identical on hits. None of
  these can decide a gate, a refusal, or an emitted byte: tests and a
  test-only feature are outside every production dependency chain, and the
  agreement battery is itself the proof the classified path did not drift.
* **`segment_segment_distance` (pinned)**: same status after EXT-2c — parity
  tests and the `_min` cross-check only. Its last production decision chains
  (drilling keepout via §O2; `seg_tri_dist` via EXT-2c) are both retargeted.
* **The removal step is the proof for everything else.** After all caller
  lanes land, the twelve names in EXT-2's removal table are DELETED from
  `queries.rs` in the K2 sweep, and the four kernel `tests/` files that call
  them migrate to the classified twins in the same commit (the test-ownership
  table in EXT-1; `penetration_intervals_batch_worst_case_limits` stays, per
  the same table). From that commit on, "a decision path consumes the silent
  miss" is not a claim this document asserts — it is a state the workspace
  cannot compile. The gate is a counted row per removed name: workspace grep
  for the method-call token `.NAME(` — the token form matters, since every
  removed name is a prefix of its classified twin, and the trailing
  parenthesis is what separates `.cast_rays(` from `.cast_rays_classified(` —
  returns 0 hits WORKSPACE-WIDE, kernel included, with each count asserted
  from grep's output rows, never from an exit code.
* **Out of scope, recorded so it is not silently dropped:** `ParityVote`
  (`parity.rs`) — the `inside` containment engine — is NOT a Möller consumer
  and is untouched here, but it answers the same geometric question with its
  own arithmetic and its own epsilons. Whether IT has a near-parallel analogue
  is a separate audit, added to the out-of-scope register below; asserting it
  clean without measurement would repeat the exact mistake D2 just corrected.

### EXT-4 — the instrumentation, end to end (rewritten in revision 4)

Revision 3 asserted a per-model × per-label table but built no route to run
output: a successful conservative outcome erased its tally at the first stage
boundary (`evaluate_delicacy` returns only a `DelicacyField`, delicacy_eval.rs;
`perceive_contacts_on_surface` only a `ContactCandidatePool`, lib.rs:990),
no lane owned a telemetry-sink file, and Agent R writes no source. The
challenger's objection is CONFIRMED. Revision 4 builds the route.

**What is counted:** one increment per Möller evaluation classified
`NearParallel`, attributed to (label from the EXT-1 table, batch row). Per
label, three counters: `near_parallel_evaluations`, `rows_affected`, and
`dispositions_moved` — the last incremented by the CALLER at the moment a
disposition actually moves (a condemned stub, a refused seat candidate, an
`UpperBound` row that failed a clears predicate, an `Ambiguous` triage ray, a
dropped drain candidate); the kernel cannot know what a caller decided, so
`dispositions_moved` is caller-side by construction. Counts are MEASURED
quantities under the counted-row rule: a run report that omits the table is
incomplete, a table of zeros is a result, and it must come from a binary whose
provenance postdates the fix commits.

**The carrier: `NearParallelLedger`.** A plain type in new
`rust/geom_kernel/src/telemetry.rs` (Agent K's additive lane): rows of
(label, the three counters), a `merge` for combining stage ledgers, and a
per-label constructor. `geom_kernel` has no serde dependency (measured:
`rust/geom_kernel/Cargo.toml` names none), so the ledger is NOT serialized in
the kernel — serialization happens once, in the pipeline's evidence type
below. Every crate in the chain already depends on `geom_kernel`, so the type
crosses each boundary without a new dependency edge.

**The threading rule, compiler-enforced: no stage may return a result that
discards a nonzero ledger.** Per lane:

* **M**: `evaluate_delicacy_classified(..) -> (DelicacyField, NearParallelLedger)`
  lands BESIDE the existing `evaluate_delicacy`, whose body becomes a wrapper
  delegating to the classified form (ledger dropped, documented as
  test-callers-only); the wrapper dies in the K2 sweep once P has migrated the
  two production callers — `crates/plinth-orient/src/score.rs:508` and
  `crates/plinth-pipeline/src/prepare.rs:117`, the only production callers
  (measured 2026-09-01). Label `mesh.delicacy.thinness`.
* **S**: `ContactCandidatePool`, `InitialContactBuild`, and the
  anchor/brace/cco/rewire result types carry a ledger — a new field where the
  addition is compile-safe for downstream crates, a classified-twin function
  where it is not (the same additive-then-remove discipline as the kernel;
  any such twin is enumerated in S's landing report for the K2 sweep).
  Support's `runner.rs` merges its stages' ledgers into the support result
  the pipeline consumes — `runner.rs` joins S's write-set for exactly this.
* **D**: the triage verdict table gains its `near_parallel` column (surfacing
  point 3), and drilling's plan/proof results carry their ledger like S's.
* **H**: the drain plan result carries its ledger.
* **P (new lane — the last mile)**: `ModelPreparationReport`
  (`crates/plinth-pipeline/src/lib.rs:788`) gains
  `#[serde(default)] pub geom_uncertainty: Option<GeomUncertaintyEvidence>`,
  a pipeline-owned serializable per-label table of the three counters, with
  the `repair` field's exact None-vs-zeros contract (`lib.rs:796-805`
  precedent): `None` means the classified stages never ran for this model; an
  all-zero table means they ran and nothing fired — the report keeps those
  two statements apart. The CLI prints one line for any model whose table is
  nonzero, mirroring the existing repair-line rule, and the per-model ×
  per-label table thereby lands in the `PrepareReport` that the 14-artifact
  head-to-head and every full-corpus run already read.

**Where it surfaces, all four places** (each now with a named carrier):

1. **In every classified return value** (EXT-2 shapes) — the caller cannot not
   see it.
2. **In every refusal it causes**: each typed refusal variant added by EXT-1
   carries the tally and the label, so a mystery refusal names itself
   ("centerline proof refused: 3 near-parallel faces on hole 7's ray") instead
   of surfacing as a blank `InvalidDrilledTopology`.
3. **In the triage verdict table**: a `near_parallel` column beside the
   existing per-row ray/triangle evaluation counts (the §O1 probe rows carried
   1,659,744 evaluations; the expectation for the plain corpus rows is 0, and
   it is asserted from the regenerated table, not assumed).
4. **In the run report**: the per-model `geom_uncertainty` table and its CLI
   line — which is what the head-to-head diffs. `dispositions_moved > 0` on
   any model is a STOP-AND-REPORT finding for the owner with the table
   attached; it is not license to tune `DET_EPS`, add a sliver-area threshold,
   or invent any other constant (the §O4 fallback language stands: a new
   deciding constant requires a new ruling).

**The end-to-end proof (Agent P's gate).** One integration test in
`crates/plinth-pipeline/tests/` runs a planted fixture — a mesh carrying a
near-parallel face on a delicacy ray path, a NON-refusal conservative event —
through the real prepare path and asserts the nonzero
`mesh.delicacy.thinness` row ON THE RUN REPORT, not on any intermediate
return. A companion assertion runs a clean fixture and requires `Some` with
all-zero rows, pinning the None-vs-zeros contract. A counter that cannot
reach the report is the checks-nothing failure mode this project keeps
reproducing; this test proves the pipe is connected and MUST fail if any
stage boundary drops the ledger.

A refusal flood therefore shows up as a number in four places on the first run
that produces it, rather than as mystery refusals — which is the whole point.

---

## Implementation order (amended by §EXT — folded in, not replaced)

Revision 2's order was three agents, sequential (K → L → D). §EXT folds in as
follows: **K → { L ∥ D ∥ S ∥ H ∥ M } → P → K2 → R.** Disjoint write-sets, each
file owned by exactly one agent AT A TIME (K2 runs strictly after every other
source lane, so its sweep may touch files an earlier lane owned — noted per
file below); the caller lanes after K are genuinely independent (no shared
files, verified against the write-sets below) and may run in parallel or in
any order. P runs after the caller lanes because it threads their new
signatures; K2 runs after P so its counted grep rules on the finished consumer
set. Three sequencing rules make the tree green at every commit:

* **K is ADDITIVE.** The classified walks and the new return shapes land as
  new entry points beside the existing ones (distinct names per EXT-2's
  table); nothing is removed and no existing signature changes, so every
  crate keeps compiling while the caller lanes migrate.
* **Every cross-crate signature change is additive in its lane.** A lane may
  add a field only where downstream crates it does not own still compile
  (no external struct-literal construction or exhaustive destructuring —
  checked by building the workspace, not assumed); anything that would break
  a crate outside the lane's write-set lands as a classified twin instead,
  and the superseded form dies in the K2 sweep. Each lane's landing report
  enumerates the twins it created, so the sweep's deletion list is a counted
  handoff, not a rediscovery.
* **K2 (the removal sweep) runs only after every other source lane has
  landed**, deletes the twelve EXT-2 table names, the superseded caller-crate
  wrappers/twins, migrates the four kernel test files, and is the
  compile-time census proof of EXT-3. R (the re-measure) runs last, against
  the finished tree, with binary provenance verified.

L's retargets still need K's `poly2` primitives; D still needs K's
`ray_triangle_classified`. The verify agent commits; nobody pushes.

### Agent K — kernel (first)

Write-set: `rust/geom_kernel/src/closest.rs`, `rust/geom_kernel/src/consts.rs`,
`rust/geom_kernel/src/poly2.rs`, `rust/geom_kernel/src/moller.rs`,
**`rust/geom_kernel/src/lib.rs`**, **`rust/geom_kernel/src/queries.rs`** (§EXT,
additive only in this lane), new **`rust/geom_kernel/src/telemetry.rs`**
(the `NearParallelLedger`, §EXT-4), new `rust/geom_kernel/tests/segseg_min.rs`,
new `rust/geom_kernel/tests/ray_class.rs`, new
`rust/geom_kernel/tests/walk_classified.rs`.

1. Add `segment_segment_distance_min` to `closest.rs` per §O2 (candidate scheme,
   no epsilons, pinned function untouched except a one-sentence cross-reference).
2. **Export it**: `lib.rs:67` becomes
   `pub use closest::{segment_segment_distance, segment_segment_distance_min};`
   with the doc comment split per §O2.
3. Split `SEGSEG_EPS` into `SEGSEG_DENOM_EPS_MM4` / `SEGSEG_LEN2_EPS_MM2` in
   `consts.rs`, values verbatim `1e-14`, used at the dimensionally matching
   comparisons. Byte-neutral: the existing closest.rs test suite passes unchanged.
4. Add `RayTriangleClass` + `ray_triangle_classified` to `moller.rs` per §O4;
   rewrite the false "PRUNE — not a verdict" contract paragraph; extend the
   `lib.rs:88` re-export with `ray_triangle_classified, RayTriangleClass`.
5. Add `poly2::dot` and `poly2::perp_dot` per §O6, operand-order contract in the
   doc comments.
6. Rewrite the `DEGENERATE_EDGE_L2_MM2` derivation in `poly2.rs` per §O3 (bounded
   error + NaN exclusion; delete the lattice premise and knife-edge claim).
   Constant value unchanged.
7. Rewrite provenance labels per §O5 ("chosen behavior pin", not "oracle-pinned"
   as a derivation) in `consts.rs`, `moller.rs`, `closest.rs`.
8. Tests: `segseg_min.rs` — the counterexample pin (tolerance 1e-12, imported
   through the crate root), the near-parallel battery vs a dense-sweep oracle,
   degenerate-input cases. `ray_class.rs` — the §O4 counterexample pin (exact
   literals; asserts pinned `hit == false` AND classified `NearParallel`), and the
   agreement battery (`hit` ⇔ `Hit(_)`, bit-identical `t,u,v` on hits).
9. **(§EXT, additive)** Classified twins of the two walks in `queries.rs`
   (identical traversal and hit predicate, plus the per-ray `NearParallel`
   tally, mirror-locked per EXT-2) and the classified entry points UNDER THE
   DISTINCT FINAL NAMES of EXT-2's removal table — `cast_rays_classified`,
   `cast_rays_census_classified`, the `_classified` penetration and census
   batches, `sphere_breach_classified`, `faces_near_segments_batch_classified`,
   and `segment_clearance_classified`/`segment_clearance_excluding_classified`
   returning the three-state row type with `UpperBound` and the restated
   clears predicate. The `NearParallelLedger` lands in `telemetry.rs` with a
   unit-test battery of its own (merge associativity; a planted row survives
   a merge chain). Existing entry points are NOT touched in this lane (K2
   removes them).
10. **(§EXT-2c)** `seg_tri_dist`'s decision-path form: edge candidates via
    `segment_segment_distance_min`, pierce via the classified Möller. The
    §O4 counterexample face, hit by a SEGMENT chain test, pins that a
    near-parallel pierce yields `UpperBound`, not a clean `Measured`.
11. Tests: `walk_classified.rs` — walk-level agreement battery (classified hit
    set bit-identical to the unclassified walk on a probe mesh; tallies equal
    between collecting and counting walks per ray), plus a planted
    near-parallel face asserting the tally is nonzero where constructed — a
    counter that cannot fire is the failure mode this project keeps
    reproducing, so it must prove it can.

### Agent L — layers, support, orient, guards (after K lands)

Write-set: `crates/plinth-layers/src/lib.rs` (doc lines only), new
`crates/plinth-layers/tests/passthrough_verbatim.rs`,
`crates/plinth-orient/src/section.rs` (the `quadratic` coefficient retarget only),
new `crates/plinth-cli/tests/no_private_vector_algebra.rs`,
new `crates/plinth-verify/tests/no_private_vector_algebra.rs`, and
new `rust/chitubox_io/tests/no_private_vector_algebra.rs`. (Revision 4 moves
the two Guard-2 files — `rust/geom_kernel/tests/guard_scripts/no_inline_geometry.sh`
and `rust/geom_kernel/tests/kn_no_inline_geometry_guard.rs` — to K2's
write-set: the DOT2D/CROSS2D patterns are satisfied only after BOTH
retargets land, and those retargets now live in two parallel lanes
(`section.rs` in L, `perception.rs` in S), so the patterns can only land in a
lane sequenced after both, which K2 is. `crates/plinth-hollow/src/lib.rs`
remains dropped: its hit is measured test-code and needs nothing.)

1. Land the O3 witness test and the two layers doc corrections (§O3 items 2–3;
   item 1 is K's).
2. (Moved to Agent S in revision 4.) The `perception.rs:1703/1705` retarget
   onto `poly2::dot`/`poly2::perp_dot` lands exactly as §O6 specifies, but is
   EXECUTED by S — S must also thread the perception ledger and site-3 counter
   through `perception.rs:1136`, and one file gets one owner. The §O6 content
   is unchanged. NO new GEOM-EXEMPT markers anywhere in this run (this rule
   still binds every lane).
3. Retarget the `section.rs` `quadratic` coefficients onto `poly2::perp_dot`
   (§O6), preserving operand order; the middle coefficient's regrouping and its
   ULP disclosure go in the commit message for Agent R's attribution.
4. (Moved to K2 in revision 4 — see the write-set note above.)
5. Add the three missing per-crate string guards (§O6 gap 2; measured: none of
   the three crates contains `.mul_add` in `src/`, so all three land green
   with no dependency on any retarget).

### Agent D — triage and drilling (after K lands; parallel with L, S, H, M)

Write-set: `crates/plinth-mesh/src/triage.rs` (including its inline tests),
`crates/plinth-hollow/src/drilling.rs` (including its inline tests).

1. Normalize triage's sampling directions and retarget `ray_triangle_distance`
   onto `ray_triangle_classified` with the §O4 mapping (`NearParallel` →
   `Ambiguous`, `ExactParallel` → `Miss`); update the units comments.
2. Retarget `keepout_axis_distance` to `segment_segment_distance_min`; update its
   doc comment (the "prunes instead of deciding" sentence becomes "complete
   minimisation; no parallel branch exists on this path").
3. Add `near_parallel_keepout_pair_is_measured_by_complete_minimisation` (§O2,
   verbatim numbers) and a triage-level test that the §O4 counterexample face
   yields an Ambiguous ray, not a silent miss.
4. **(§EXT sites 17–18)** Migrate `prepare_audit_plan` and `prove_centerlines`
   onto `cast_rays_classified`/`cast_rays_census_classified`, with the EXT-1
   dispositions (candidate-granular unavailability; typed refusals that name
   the near-parallel tally; labels `hollow.drilling.audit` and
   `hollow.drilling.centerlines`), and a test per site that a planted
   near-parallel face produces the typed refusal, not a silent count.

(Revision 2 gave Agent D the corpus re-measure as its step 4; that step moves
to Agent R below, because the head-to-head must run on the finished tree —
after S, H, M and K2 — not after D alone.)

### Agent S — plinth-support callers (after K lands; parallel lane)

Write-set: `crates/plinth-support/src/anchor.rs`, `lib.rs`, `rewire.rs`,
`brace.rs`, `cco.rs`, `designed_embed.rs`, `initial.rs`, `seat_zone.rs`,
**`perception.rs`** (from L, revision 4 — one owner per file),
**`runner.rs`** (ledger merging, §EXT-4), and
**`crates/plinth-support/tests/penetration_census_equivalence.rs`** (its 10
old-API calls migrate here, per the EXT-1 test-ownership table) — src files
including their inline tests. Owns §EXT sites 3–15 with the EXT-1
dispositions and labels, specifically:

1. Migrate the three lossy-shim callers (`brace.rs:1881`, `:2972`,
   `cco.rs:4263`) and the `_typed` caller (`rewire.rs:1245`, site 11) to
   `segment_clearance_excluding_classified`; every clears call site restates
   its `UpperBound` policy per EXT-2 (the new name and row type make the
   compiler enumerate them).
2. Anchor seats: the new `AnchorSeatRefusalReason` variant; foot and brace
   breach rows treated as breached when uncertain; penetration/contact rows
   routed per sites 8, 13 and 15.
3. Site 3 (`perceive_contacts_on_surface`): implement the revision-4 RULING —
   a `NearParallel` on a column's ray changes no `grounded` bit, the affected
   faces stay candidates, the per-column tally is counted under
   `support.perception.columns` and carried on the perception result
   (justification 3; the delegated-derivation language of revision 3 is
   withdrawn).
4. Site 14: `OverlapMeasure` with the `NearParallelLowerBound` arm,
   unconditional condemnation of uncertain rows through the existing
   quarantine walk, and the `uncertain_overlap` evidence vector on
   `InitialContactBuild`, per justification 14.
5. Seat bands: proof-only membership per EXT-2 justification 12.
6. Execute §O6's `perception.rs:1703/1705` retarget onto
   `poly2::dot`/`poly2::perp_dot` and delete the no-survivor exemption block —
   the §O6 specification is unchanged; only the executing lane moved.
7. Thread every stage ledger through `runner.rs` into the support result the
   pipeline consumes (§EXT-4 threading rule), enumerating in the landing
   report any classified twin created where a field addition would not
   compile downstream.
8. Per site: one test with a planted near-parallel configuration proving the
   conservative disposition fires and the tally surfaces.

### Agent H — plinth-hollow drains (after K lands; parallel lane)

Write-set: `crates/plinth-hollow/src/drains.rs` (including its inline tests).
§EXT site 16: `cast_rays_classified`/`cast_rays_census_classified` in `plan`,
candidate-granular unavailability through the existing `unavailable_candidates`
plumbing, typed refusal when a required set empties, label
`hollow.drains.plan`, and the planted-configuration test. Disjoint from Agent
D's `drilling.rs` by file.

### Agent M — plinth-mesh delicacy (after K lands; parallel lane)

Write-set: `crates/plinth-mesh/src/delicacy_eval.rs` (including its inline
tests) and `crates/plinth-mesh/src/lib.rs` (export lines only, for the
classified function). §EXT site 2: thin-side saturation on `NearParallel`,
counted, label `mesh.delicacy.thinness`, with the planted-configuration test.
Lands `evaluate_delicacy_classified` beside the existing `evaluate_delicacy`
per §EXT-4's threading rule (the old function becomes a delegating wrapper,
deleted in the K2 sweep after P migrates the two production callers). Disjoint
from Agent D's `triage.rs` by file.

### Agent P — pipeline, CLI and orient threading (after L, D, S, H, M ALL land)

New in revision 4 — the telemetry last mile (§EXT-4). Write-set:
`crates/plinth-pipeline/src/lib.rs`, `crates/plinth-pipeline/src/prepare.rs`,
`crates/plinth-cli/src/execute.rs`, `crates/plinth-orient/src/score.rs`
(disjoint from L's `section.rs` by file), and one new integration test file
in `crates/plinth-pipeline/tests/`. P sequences after the caller lanes
because it threads signatures they create (M's classified delicacy, S's
ledger-carrying results).

1. Migrate the two production `evaluate_delicacy` callers (`score.rs:508`,
   `prepare.rs:117`) to `evaluate_delicacy_classified` and thread the ledger
   upward; orient's score result carries its ledger like every other stage
   result.
2. `ModelPreparationReport` gains
   `#[serde(default)] pub geom_uncertainty: Option<GeomUncertaintyEvidence>`
   (pipeline-owned, serializable), populated by merging every stage ledger
   for the model, with the None-vs-zeros contract of §EXT-4.
3. The CLI prints one line per model with a nonzero table, mirroring the
   repair-line rule.
4. The end-to-end gate: the planted-fixture integration test of §EXT-4 — a
   NON-refusal conservative event asserted on the run report, plus the
   clean-fixture all-zeros companion. This test is P's acceptance gate, not
   an optional extra.

### Agent K2 — the removal sweep (after P lands; same owner as K)

Expanded in revision 4 from kernel-only removal to the workspace removal
sweep. Write-set: `rust/geom_kernel/src/queries.rs`,
`rust/geom_kernel/src/lib.rs`, the four kernel test files of the EXT-1
test-ownership table (`native_api.rs`, `face_excluded_clearance.rs`,
`kn_segment_clearance_unmeasured.rs`, `near_wall_thin_feature.rs`),
`rust/geom_kernel/tests/guard_scripts/no_inline_geometry.sh` and
`rust/geom_kernel/tests/kn_no_inline_geometry_guard.rs` (from L, revision 4),
plus the superseded caller-crate wrappers/twins enumerated in the caller
lanes' landing reports (e.g. the `evaluate_delicacy` wrapper in
`crates/plinth-mesh/src/delicacy_eval.rs` — file reuse is safe here because
K2 runs strictly after every lane that owned those files).

1. Delete the twelve EXT-2 table names and the two-state `SegmentClearance`
   row type from `queries.rs`/`lib.rs`; update exports.
   `penetration_intervals_batch_worst_case_limits` stays.
2. Migrate the four kernel test files to the classified twins in the same
   commit (89-call ownership table; expected values unchanged where no tally
   fires, per the agreement contract).
3. Delete the caller-crate wrappers/twins from the landing-report enumeration
   — a counted list handed forward, not rediscovered.
4. Add DOT2D (`$A[0] * $B[0] + $A[1] * $B[1]`) and CROSS2D
   (`$A[0] * $B[1] - $A[1] * $B[0]`) to the Guard-2 script, extend the kn
   self-test to plant one unmarked instance of EACH new pattern, and refresh
   the counted-row baselines from a fresh run (expected: zero production
   hits, the `score.rs`/`hollow/lib.rs` hits classified test-code — §O6 gap 1;
   moved here from L because the satisfying retargets live in two parallel
   lanes, L and S, and the pattern may land only after both).

Gates, per EXT-3: the workspace compiles (the census proof), and a COUNTED
grep row per removed name — the method-call token `.NAME(`, workspace-wide,
kernel included — shows 0 hits, asserted from grep's output rows, never from
an exit code.

### Agent R — the re-measure (last; no source writes)

Rebuild release (`RUSTC_WRAPPER="" cargo build --release`; sccache is broken
here), verify binary provenance postdates EVERY lane's commits, re-run the O1
probes, regenerate the triage verdict table (now including the `near_parallel`
column), re-run the 14-artifact control head-to-head from `487e71f`'s
protocol, and report with per-cause attribution (O2 retarget / O4
normalization / O4 band routing / O6 regrouping / §EXT classification) for
every moved row or byte, bisected, per §O4. Zero drilling movement is expected;
triage-table `t`-scale movement is CERTAIN (normalization) and must be shown to
be scale-only wherever verdicts did not change. R reads the EXT-4 per-model ×
per-label table out of the run reports (the plumbing is P's, gated by P's
end-to-end test; the measurement is R's) and includes it verbatim;
`dispositions_moved > 0` anywhere is a STOP-AND-REPORT finding for the owner,
not a value to tune away.

(The "Required re-measurement" paragraphs inside §O2 and §O4 assign this
re-measure to Agent D; §EXT supersedes that assignment — it is this lane, on
the finished tree. The measurements themselves are unchanged.)

### Out of scope of all lanes (recorded, not assigned)

The OP-5 dedup divergence (`triage.rs` relative `1e-9` vs kernel `DEDUP_LOC_MM`;
debt register D4), the kernel's own `segment_batch.rs` self-exemption (evidence doc
§8.4), the doctrine debt register entries D1–D3 (each names what forces it
forward), the `ParityVote` near-parallel audit (§EXT-3: `parity.rs` answers
containment with its own arithmetic and epsilons and is measured NOT to consume
Möller; whether it has an analogous undecidable band is an unmeasured question,
recorded rather than asserted clean), and the canon-replacement document, which
must carry: the O4 units and determinant-band corrections, the O6 precedence rule
(no-exceptions form), the O5 chosen-behavior-pin classifications, the doctrine
debt register, and §EXT's classified-propagation contract (EXT-2) with its
tolerability rule. The evidence doc §8.1 item (missing 2-D home) is CLOSED by
Agent K's `poly2` additions.
