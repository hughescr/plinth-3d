# plinth-3d duplication census

Scope: crates/ + rust/ (167k lines). Every site below was read; line numbers are
current as of this scan. "Drift" means the copies no longer compute the same
thing bit-for-bit, which is the dangerous case per the brief.

---

## Group 1 — `[f64;3]` vector arithmetic (`add`/`sub`/`scale`/`dot`/`norm`/`cross`) reimplemented in three files, none of which reuse the kernel's own copy

**Root cause**: `rust/geom_kernel/src/vec3.rs` already has `sub`, `add`, `scale`,
`dot`, `cross`, `norm`, `dist` — but `rust/geom_kernel/src/lib.rs:22` declares
`mod vec3;` as **private** and only re-exports the `V3` type alias
(`pub use vec3::V3;` at line 70). None of the arithmetic is reachable outside
the crate, even though both downstream crates already depend on geom_kernel.

Sites (each a private, byte-identical or near-identical reimplementation):
- `crates/plinth-support/src/rewire.rs:1217-1230` (`add`,`sub`,`scale`,`dot`,`norm`)
- `crates/plinth-support/src/joints.rs:1553-1568` (`add`,`sub`,`scale`,`dot`,`norm`,`normalize`)
- `crates/plinth-metrics/src/scar.rs:2260-2272` (`add`,`sub`,`scale`,`dot`,`cross`)

Evidence — `rewire.rs:1217`:
```rust
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
```
`joints.rs:1553-1556` is character-for-character the same two functions.
`scar.rs:2260-2263` is the same again, only the type alias name (`Vec3` vs `[f64;3]`) differs.
`joints.rs:1565` computes `norm` inline as `(a[0]*a[0]+a[1]*a[1]+a[2]*a[2]).sqrt()`
instead of calling its own `dot`, a small additional drift from `rewire.rs:1229`'s
`dot(a, a).sqrt()` (same value, one more redundant expression to keep in sync).

**Recommendation**: make `geom_kernel::vec3` `pub mod` (or re-export the six
functions from lib.rs) and delete all three private copies. Single home:
`geom_kernel::vec3`.

---

## Group 2 — 3D point-to-point Euclidean distance, six sites, three different formulas

- `crates/plinth-support/src/lower.rs:461` — iterator `.zip().map(pow).sum().sqrt()` over `Finite`
- `crates/plinth-support/src/initial.rs:1637` — **identical** to lower.rs (confirmed line-for-line)
- `crates/plinth-support/src/coverage_closure.rs:1281` (`squared_distance`, no sqrt) — `mul_add` chain
- `crates/plinth-support/src/lib.rs:2224` (`squared_distance`) — **identical** `mul_add` chain to coverage_closure.rs
- `crates/plinth-support/src/rewire.rs:1189` (`distance`) — `norm(sub(a,b))`, i.e. Group 1's private vector helpers
- `crates/plinth-support/src/specialized.rs:8278` (`distance3`) — `(dx).hypot(dy).hypot(dz)`, a third numeric strategy
- `crates/plinth-metrics/src/scar.rs:2282/2285` (`distance`, `distance_squared_between`) — `length(sub(...))` / `dot(sub,sub)`, a fourth restatement

All are mathematically equivalent for finite inputs, but `specialized.rs`'s
chained-`hypot` form has different overflow/underflow behavior than the
sum-of-squares forms, and nothing enforces that a future edit (e.g. adding an
epsilon guard for coincident points) lands in all six places. This is exactly
the kind of literal that geom_kernel's `vec3::dist` (Group 1) already exists to
be — it is simply unreachable.

**Recommendation**: one `geom_kernel::vec3::dist` (exported per Group 1),
callers stop hand-rolling it.

---

## Group 3 — 2D point-to-segment distance: four sites, three different degeneracy epsilons (real drift)

Same "project onto segment, clamp t to [0,1]" algorithm, but the guard for a
zero-length segment differs by up to 1000x between copies:

- `crates/plinth-support/src/joints.rs:22` — named constant `DEGENERATE_LENGTH_SQUARED: f64 = 1e-18`, used at lines 1474, 1502, 2290
- `crates/plinth-support/src/perception.rs:2604` — inline literal `denominator < 1.0e-15` (1000x looser than joints.rs)
- `crates/plinth-support/src/specialized.rs:8128` — `length_squared == 0.0` (exact-bit-pattern check, **no epsilon at all**)
- `crates/plinth-layers/tests/common/mod.rs:89` (`distance_to_boundary`) — `length2 > 0.0` guard, test-only

Evidence, `specialized.rs:8128-8136`:
```rust
fn segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let edge = [b[0] - a[0], b[1] - a[1]];
    let length_squared = edge[0] * edge[0] + edge[1] * edge[1];
    if length_squared == 0.0 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
    ...
```
vs `perception.rs:2604-2608`:
```rust
fn point_segment_distance(point: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let denominator = ab[0] * ab[0] + ab[1] * ab[1];
    if denominator < 1.0e-15 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
```
A segment with `length_squared` on the order of `1e-16` (a ~1e-8 mm edge —
plausible after quantization/decimation) takes the safe zero-length branch in
`perception.rs` but falls through to a near-zero division in `specialized.rs`.
This is the drift the brief specifically asked to flag: a fix to one copy's
epsilon (joints.rs already has a *named* constant, suggesting someone already
cared about this exact number) silently does not reach the other two.

A 3D sibling of the same algorithm also exists at
`crates/plinth-support/src/rewire.rs:1201` (`point_segment_distance`, guard
`l2 < 1e-18`, matching joints.rs's constant only by coincidence of being
copy-typed the same literal, not by sharing it), and it is called in a
**scalar double loop** at `rewire.rs:1131-1150` (`for brace_key in
forest.braces() { for host in hosts { point_segment_distance(...) } }`) —
`rust/geom_kernel/src/segment_batch.rs:225` already documents itself as *"the
single kernel primitive for callers that gather a bounded window of points and
evaluate it against one shared segment-axis batch"* for exactly this shape of
problem, unused here.

**Recommendation**: one shared 2D point-segment-distance helper with one named
epsilon constant (promote `joints.rs`'s `DEGENERATE_LENGTH_SQUARED`), and route
`rewire.rs`'s 3D scalar loop through geom_kernel's batched primitive.

---

## Group 4 — `point_in_polygon` ray-casting, verbatim in three files (two of them production)

Byte-identical 12-line even-odd ray-casting test:
- `crates/plinth-support/src/perception.rs:1537-1548`
- `crates/plinth-support/src/specialized.rs:8103-8114`
- `crates/plinth-layers/tests/common/mod.rs:56-67` (test-only)

```rust
fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        if (a[1] > point[1]) != (b[1] > point[1])
            && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
    }
    inside
}
```
No drift today (the two production copies in `plinth-support` are exact), but
two independent private copies of the same 12 lines in the same crate is the
textbook setup for the next edit (e.g. an on-boundary tie-break) to land in
only one.

**Recommendation**: one `pub(crate)` helper in `plinth-support`, e.g. beside
`perception.rs`'s `signed_area`.

---

## Group 5 — `signed_area` (shoelace), duplicated across the crate boundary — and the codebase already admits it

- `crates/plinth-support/src/perception.rs:1524-1534`
- `crates/plinth-layers/src/lib.rs:278-287`

`plinth-layers/src/lib.rs:276-277`'s own doc comment:
```rust
/// Shoelace area over emitted f64 points, identical to the perception
/// stitcher's `signed_area`.
fn signed_area(points: &[[f64; 2]]) -> f64 {
```
The two function bodies are in fact identical (confirmed by direct read of
both). This is the one group in this census where the duplication is
self-documented rather than merely discovered — the author knew and shipped it
anyway, which means there is no test tying the two together if one drifts.

**Recommendation**: move `signed_area` into a small shared geometry-2d module
(or `plinth-core`, which both crates could reasonably depend on) instead of
documenting the duplicate in a comment.

---

## Group 6 — signed-tetrahedron mesh volume, three independent implementations, one silently unchecked

Same `dot(a, cross(b, c)) / 6` per-triangle term, summed:

- `rust/geom_kernel/src/winding.rs:136` (`soup_volume`) — checks `!term.is_finite()` (→ `NonFiniteTetrahedron`) **and** `!sum.is_finite()` (→ `NonFiniteAccumulation`) per triangle, private to geom_kernel (not exported for this purpose — the exported surface is `signed_soup_volumes` for a different call shape).
- `crates/plinth-metrics/src/topology.rs:597-619` (`signed_volume`) — reimplements the same finite checks, but collapses both cases into one `MetricsError::ArithmeticOverflow` variant instead of geom_kernel's two distinct variants; also adds face-flip support geom_kernel's copy doesn't have.
- `crates/plinth-mesh/src/fixtures.rs:964-977` (`mesh_signed_volume`) — **same formula, zero finiteness checking**:
```rust
fn mesh_signed_volume(mesh: &Mesh) -> f64 {
    mesh.tris().iter().map(|triangle| {
        let a = mesh.verts()[triangle[0] as usize];
        ...
        (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0
    }).sum()
}
```
A NaN/Inf vertex silently propagates to a NaN/Inf volume here, where the other
two copies of this exact formula explicitly refuse. (Lower severity in
practice: this copy is used only from `fixtures.rs:867` to build test
fixtures, not from production inference — but it is the same formula, drifted
in error-handling, and a future call site added by someone who assumes "this
crate's volume function is safe like the others" would be wrong.)

**Recommendation**: expose one checked `signed_tetrahedron_volume`-style
primitive from geom_kernel; have `plinth-metrics` and `plinth-mesh` both call
it instead of restating the term formula and its own finiteness policy.

---

## Group 7 — frustum (cone) volume formula duplicated 3x, with the mm³→mL constant duplicated under two names (one semantically wrong)

Formula `π/3 · L · (r0² + r0·r1 + r1²) / 1000` (cubic mm → mL):

- `crates/plinth-support/src/member_sizing.rs:601-611` (`frustum_volume_ml`, canonical/typed, validated) — divides by `MILLIMETRES_PER_METRE` (`member_sizing.rs:16`, `= 1_000.0`). **This constant's name describes mm→m conversion, not mm³→mL** — it is legitimately named for its other four uses in the same file (lines 387, 437, 439, 467) and only coincidentally has the right numeric value (1000) to also convert volume units here.
- `crates/plinth-support/src/cco.rs:5672-5690` (`member_volume`) — inline reimplementation of the identical formula, dividing by `cco.rs:37`'s own `MILLIMETRES_PER_MILLILITRE = 1_000.0` — a **correctly**-named constant, but a separate one from member_sizing.rs's, both equal to 1000.0 by coincidence rather than by sharing a definition.
- `crates/plinth-support/src/rewire.rs:1186-1188` (`frustum_raw`) — inline reimplementation again, this time with a **bare literal** `/ 1000.0`, no named constant at all:
```rust
fn frustum_raw(length: f64, r0: f64, r1: f64) -> f64 {
    std::f64::consts::PI / 3.0 * length * (r0 * r0 + r0 * r1 + r1 * r1) / 1000.0
}
```
Note `rewire.rs` also calls the *canonical* `frustum_volume_ml` via its own
`member_volume_ml` wrapper (`rewire.rs:1171`), so the same file has both the
validated call path and a private duplicate of the underlying math — the
duplicate exists for legitimate reasons (unwrapped-f64 fast path for
speculative junction-cost search at `rewire.rs:624/653/713`, not needing the
newtype/Result ceremony), but the raw formula itself should be a shared
`#[inline]` scalar core that both the validated wrapper and the raw path call,
rather than three independent transcriptions of `π/3·L·(r0²+r0r1+r1²)`.

**Recommendation**: one private `fn frustum_volume_raw_mm3(length, r0, r1) ->
f64` (in member_sizing.rs), reused by `frustum_volume_ml`, `cco::member_volume`,
and `rewire::frustum_raw`; retire the coincidental double-naming of the 1000.0
constant.

---

## Group 8 — `triangle_cube_overlaps`: verbatim 40-line SAT algorithm, copy-pasted within the same crate, doc comment lost in the copy

- `crates/plinth-hollow/src/lib.rs:3020-3067` (with a 7-line doc comment explaining the touching-counts-as-overlap semantics and citing Akenine-Möller 2001 / Ericson §5.2.9)
- `crates/plinth-hollow/examples/raster_delta.rs:271-306` — same 13-axis separating-axis test, same variable names (`v`, `e`, `BOX_AXES`), same three loop structures, confirmed line-for-line identical through the box-axis and edge-cross loops; only the final two lines differ in style (`if d > r || d < -r { return false } true` vs `d <= r && d >= -r`), which are logically equivalent. **The doc comment explaining the non-strict-separation / degenerate-triangle rationale is absent from the copy** — a future maintainer editing the example has no record of why touching must count as overlap.

**Recommendation**: `raster_delta.rs` is an example inside `plinth-hollow` — have it call `plinth_hollow`'s own (crate-internal or `pub`) function instead of restating it.

---

## Group 9 — ray–triangle intersection, three independent implementations with different degenerate-case semantics (two abstractions modelling one concept)

- `rust/geom_kernel/src/moller.rs:13` (`moller`) — Möller–Trumbore, returns `(bool, f64)`, private to geom_kernel, used internally by `queries.rs`.
- `crates/plinth-mesh/src/triage.rs:1099-1141` (`ray_triangle_distance`) — Möller–Trumbore with a three-way `RayHit::{Hit,Miss,Ambiguous}` result and two named epsilons (`ORIGIN_EPS = 1e-9`, `EDGE_EPS = 1e-10`) for near-origin and near-edge degeneracy.
- `crates/plinth-metrics/src/geometry.rs:226-264` (`ray_triangle_distance`) — a different formulation entirely: normalizes by the triangle's own coordinate scale first, then checks finiteness at each stage, returning `Result<Option<f64>, MetricsError>` with no "ambiguous" state.

plinth-mesh and plinth-metrics are independent crates (neither depends on the
other; both depend on geom_kernel, whose own third implementation is
unreachable — see Group 1's root cause). Same geometric primitive, same
function name, three different degenerate-case policies: a triangle nearly
edge-on to the ray is `Ambiguous` in one crate and either a definite `Some`
distance or an `ArithmeticOverflow` in the other, with no shared test fixture
enforcing agreement between them.

**Recommendation**: this is the deepest one — worth a design pass, not just an
extraction, since the three call sites want genuinely different result shapes.
At minimum, export geom_kernel's `moller` (or a wrapper with a documented
epsilon policy) as the one titled primitive, and have `plinth-mesh` and
`plinth-metrics` build their richer result types on top of it instead of
re-deriving the barycentric math independently.

---

## Top duplicate groups by (sites × drift risk) — summary

1. **Group 3** — 2D/3D point-segment distance, 4+ sites, 3 different epsilons, confirmed live numeric drift (1e-15 vs 1e-18 vs exact-0.0); also bypasses geom_kernel's documented batched primitive in a scalar O(n·m) loop (`rewire.rs:1131-1150`).
2. **Group 9** — ray-triangle intersection, 3 sites across 2 crates + geom_kernel's own private 4th, differing degenerate-case result types (`Ambiguous` vs `Option` vs bool), architecture-level.
3. **Group 1** — vec3 arithmetic, 3 sites, root-caused by geom_kernel's `mod vec3` being private; fixing the export deletes ~45 lines across 2 crates.
4. **Group 7** — frustum volume formula × 3 plus a coincidentally-equal, inconsistently-named unit constant (`MILLIMETRES_PER_METRE` doing mm³→mL duty).
5. **Group 6** — tetrahedron volume formula × 3, one copy (`plinth-mesh::fixtures::mesh_signed_volume`) silently drops the finiteness guards the other two enforce.
6. **Group 2** — point-point distance × 6-7, 3 distinct numeric strategies, no drift in outcome today but no shared home either.
7. **Group 8** — `triangle_cube_overlaps`, verbatim 40-line copy inside one crate (`plinth-hollow`), explanatory doc comment lost in the copy.
8. **Group 5** — `signed_area` shoelace, cross-crate duplicate the authors' own comment admits to.
9. **Group 4** — `point_in_polygon`, verbatim in 2 production + 1 test site, no drift yet.

All nine groups were confirmed by reading both/all sides; none are Rust
generics, derives, or per-variant match arms.
