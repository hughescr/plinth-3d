# HANDOFF — geom_kernel to its callers

Written by the kernel-numerics lane. Everything below is verified against real
builds and real test runs; nothing here is a plan.

## WHERE THE WORK IS

The shared checkout was reset to `bdab3aa` mid-run. The kernel work is preserved
in git at commit **`623efec`** (branch `wip/correctness-salvage`), message
"SALVAGE: correctness phase work — DOES NOT COMPILE, do not build on this".

That warning is true of the commit AS A WHOLE (plinth-support and plinth-hollow
lanes were cut in half). It is **not** true of `rust/geom_kernel`, which is
self-contained and was verified in isolation:

```
git archive 623efec rust/geom_kernel | tar -x -C <somewhere>
# make it standalone: drop [lints] workspace=true, pin kiddo directly,
# append an empty [workspace] table to its Cargo.toml
cargo test
```

Result: geom_kernel builds clean and **122 of 126 tests pass**. All four failures
have the SAME cause and are documented in section 4 — they are pre-existing
fixtures that relied on the very defect this lane fixed.

A verified standalone copy is also at
`scratchpad/verify-salvage/rust/geom_kernel` -- salvaged source plus the four
section-4 fixture repairs, **126/126 green, zero failures, zero ignored**. The
repairs alone are saved as `scratchpad/review/kn-fixture-repairs.patch` (four
hunks, 74 lines).

## 1. WHAT LANDED IN THE KERNEL

### A — typed clearance (`queries.rs`)

`segment_clearance` used to return `Vec<f64>` in which a row that was NEVER
TESTED was encoded as `f64::INFINITY`. That sentinel is in band, orderable, and
larger than every demanded gap, so a caller grading with `>= demand` read "we
did not look" as "enormous clearance, definitely fine".

New shape:

```rust
pub enum UnmeasuredSegment {
    DegenerateAxis,       // |p1 - p0| < 1e-9: no axis to sweep
    InsetsConsumeAxis,    // inset0 + inset1 eat the whole axis
    AllFacesExcluded,     // the row's exclusion set removed every face
}

pub enum SegmentClearance {
    Measured { clearance_mm: f64, witness: [f64; 3] },
    Unmeasured(UnmeasuredSegment),
}

impl SegmentClearance {
    pub const fn clearance_mm(self) -> Option<f64>;
    pub const fn witness(self) -> Option<[f64; 3]>;
    pub const fn unmeasured(self) -> Option<UnmeasuredSegment>;
    pub fn clears_or(self, when_unmeasured: bool) -> bool;  // NO default
    pub fn to_lossy_pair(self) -> (f64, [f64; 3]);          // migration only
}
```

Entry points, all on `NativeSurface`:

| name | returns | status |
|---|---|---|
| `segment_clearance_excluding_typed` | `Vec<SegmentClearance>` | **the real predicate**; everything else delegates here |
| `segment_clearance_typed` | `Vec<SegmentClearance>` | the above with an empty exclusion set |
| `segment_clearance_excluding` | `(Vec<f64>, Vec<V3>)` | LOSSY SHIM, do not use in new code |
| `segment_clearance` | `(Vec<f64>, Vec<V3>)` | LOSSY SHIM, do not use in new code |

`clears_or` deliberately has no default argument. A stage that **refuses**
passes `false` — an untested row has earned nothing. A stage that only
**prunes** a search may pass `true`, because keeping an untested candidate alive
costs a later test rather than shipping an unsupported claim.

**READ THIS BEFORE WIRING ANYTHING.** The typed form does not fix the defect on
its own. It makes the honest answer EXPRESSIBLE. Every call site still on the
f64 shim still has the bug available to it, and `rewire.rs:1088` is the site
where it is live today. "Typed form added" is not "sentinel bug fixed".

### B — degenerate faces (`mesh.rs`, `lib.rs`)

`Mesh::from_indexed` divided the edge cross product by `length.max(1e-30)`, so a
zero-area face was stored with a "normal" of `[0, 0, 0]` — finite, therefore
invisible to the `is_finite` guard right below it, and read as a DIRECTION by
the breach predicate (`inward = -normal`), the exterior-face mask (samples
offset along `+normal`) and the contact tests (`dot(normal, axis)`).

Now `SurfaceBuildError::DegenerateFace { face, cause }` with
`DegenerateFaceCause::{RepeatedVertexIndex { corners, index }, CoincidentVertices { corners }, CollinearVertices }`.
The predicate is exact — `length > 0.0` — and carries no tolerance, so a real
sliver a nanometre wide is still a real face. Do not add an epsilon here; that
would be a magic number deciding accept/reject.

### D — per-point-nearest batch (`segment_batch.rs`)

New: `point_nearest_segment_batch(points, segment_starts, segment_ends, limits)
-> Result<PointNearestSegmentBatch, PointSegmentBatchError>`, plus
`NearestSegment { segment(), distance_mm() }`.

Reduces over the SAME `point_segment_evaluation` the dense form uses, so the two
cannot disagree. Ties resolve to the lowest segment index. Peak output is one
`Option<NearestSegment>` per POINT rather than one f64 per point-segment pair.
Validation is shared with the dense form through a common `plan_batch`, so both
entry points are held to one input contract.

## 2. THE REWIRE DROP-IN (defect D's caller)

`crates/plinth-support/src/rewire.rs`, function `assert_no_stale_braces`.

**Delete** the private helper at **rewire.rs:1202-1209** entirely:

```rust
fn point_segment_distance(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> f64 {
    let d = sub(b, a);
    let l2 = dot(d, d);
    if l2 < 1e-18 {            // <-- a private duplicate of the kernel's epsilon
        return distance(p, a);
    }
    let t = (dot(sub(p, a), d) / l2).clamp(0.0, 1.0);
    distance(p, add(a, scale(d, t)))
}
```

That `1e-18` is a second copy of `segment_batch.rs`'s own degeneracy epsilon.
Two copies of one predicate drift, and the copy in the caller is the one nobody
tests.

**Replace** the per-endpoint scalar fold at **rewire.rs:1143-1152**:

```rust
// OLD — one scalar loop per endpoint, inside a loop over every brace
let nearest = hosts
    .iter()
    .filter_map(|key| forest.member(*key))
    .map(|host| {
        point_segment_distance(
            point,
            host.p0_mm().map(Finite::get),
            host.p1_mm().map(Finite::get),
        )
    })
    .fold(f64::INFINITY, f64::min);
if nearest > brace_host_tol_mm {
    return Err(RewireError::StaleBrace);
}
```

with ONE batched call for the whole brace-endpoint set. Gather the endpoints
first (they are already being enumerated), then:

```rust
// NEW — one batch for every endpoint against every host
let host_starts: Vec<[f64; 3]> = hosts
    .iter()
    .filter_map(|key| forest.member(*key))
    .map(|host| host.p0_mm().map(Finite::get))
    .collect();
let host_ends: Vec<[f64; 3]> = hosts
    .iter()
    .filter_map(|key| forest.member(*key))
    .map(|host| host.p1_mm().map(Finite::get))
    .collect();

let limits = geom_kernel::PointSegmentBatchLimits::for_dimensions(
    endpoints.len(),
    host_starts.len(),
)
.map_err(|e| RewireError::Native(e.to_string()))?;

let nearest = geom_kernel::point_nearest_segment_batch(
    &endpoints,
    &host_starts,
    &host_ends,
    limits,
)
.map_err(|e| RewireError::Native(e.to_string()))?;

for row in 0..endpoints.len() {
    // An EMPTY host set has no nearest host. Do not treat that as "far away":
    // that is precisely the f64::INFINITY seed the old fold used, and it made
    // an unanswerable question look like a comfortable pass.
    let Some(found) = nearest.nearest(row) else {
        return Err(RewireError::StaleBrace);
    };
    if found.distance_mm() > brace_host_tol_mm {
        return Err(RewireError::StaleBrace);
    }
}
```

Two behaviour notes, both deliberate:

* the old fold seeded with `f64::INFINITY`, so **a forest with no live hosts
  reported every brace as stale via a distance of infinity** — accidentally the
  right verdict, reached through a sentinel. The `None` arm above reaches the
  same verdict on purpose.
* the batch is charged `for_dimensions(endpoints, hosts)` because that bounds
  the WORK. It no longer bounds an allocation: the output is O(endpoints).

## 3. DEFECTS IN THE LANDED IMPLEMENTATION — must be fixed before this lane is done

**Both of the two below were found, fixed, and verified before the reset. They
are already correct in `623efec`.** They are recorded here in full anyway,
because `scratchpad/review/FIX-AFTER-RUN.md` §5 still lists them as outstanding
and anyone working from that note will either redo them or distrust good code.
Each entry ends with a one-command check.

### (a) The missing third unmeasured variant — `AllFacesExcluded`

**The scenario.** A row is handed an exclusion set that covers every face the
distance walk can reach — the whole-surface exemption. `seg_surface_min_excluding`
seeds `best_d` with `f64::INFINITY` and only ever lowers it, so with no surviving
candidate it returns `INFINITY`. The clearance arithmetic then computes
`INFINITY - (r_max + standoff)`, which is `INFINITY`: a perfectly finite-LOOKING
result of a subtraction.

**Why it is worse than the float bug it replaced.** With only two variants
(`DegenerateAxis`, `InsetsConsumeAxis`), that row comes back as
`Measured { clearance_mm: INFINITY }`. The type now ASSERTS that a measurement
happened when nothing was tested, and `clears_or(false)` — the arm a REFUSING
stage passes — returns `true`. Before the fix a caller was reading a float
sentinel wrongly. After a two-variant fix a caller would be reading a type that
lies. That is strictly worse, because the lie is load-bearing and looks like
rigour.

**The fix**, `queries.rs`, in `seg_clear_one_excluding`, immediately after the
walk and BEFORE the `best_d - thresh` subtraction:

```rust
let (best_d, best_pt) = self.seg_surface_min_excluding(a, b, excluded);
if !best_d.is_finite() {
    return SegmentClearance::Unmeasured(UnmeasuredSegment::AllFacesExcluded);
}
```

`!best_d.is_finite()` is exactly "the walk found no testable face": the mesh
cannot be empty (`SurfaceBuildError::EmptyFaces` forbids it), and the walk's
prune is `> best_d`, never true while `best_d` is infinite, so every leaf is
descended into and every surviving face is evaluated.

**The tell, and it matters.** `rust/geom_kernel/tests/face_excluded_clearance.rs`
had a test — `excluding_every_face_leaves_nothing_to_grade` — that asserted
`clearance[0].is_infinite()` on exactly this row, with the comment "the caller's
admissibility rule is what must refuse it". That test **passes unchanged against
the two-variant version while the hole is wide open**, because the float shim
still hands back `INFINITY` either way. It has been rewritten to assert
`SegmentClearance::Unmeasured(UnmeasuredSegment::AllFacesExcluded)`.

> If that test still passes without being changed, the hole is still open.

Check: `grep -c AllFacesExcluded rust/geom_kernel/src/queries.rs` — expect 3 at
`623efec`, 0 if it has been lost again.

**And the caller half.** The test's own comment was right that a caller must
refuse this row, and `rewire.rs:1088` is a caller that does not:

```rust
Ok(clearances.into_iter().all(|value| value >= 0.0))
```

`AllFacesExcluded` is **not** a large clearance. Whoever wires the typed form
into rewire must map it through `clears_or(false)` — this is a validation pass
over an already-accepted move, so an untested row must refuse, not pass.

### (b) The empty-segment-set batch that erases its own points

**The scenario.** `point_nearest_segment_batch` is called with points but zero
segments. One draft returned `nearest: Vec::new()` for that case while
`point_count()` is defined as `self.nearest.len()`. A batch handed 100 points
then reports `point_count() == 0` and `all()` yields nothing, so a caller zipping
the rows against its own points iterates **zero times and reports success**.

Same class as the `INFINITY` clearance — an absent answer wearing the costume of
a valid one — except that it fails silently rather than permissively, which is
harder to notice.

**The fix**: allocate one row per point unconditionally and leave them `None`,
which is what the accessor's own doc already promises:

```rust
let mut nearest: Vec<Option<NearestSegment>> = vec![None; plan.point_count];
```

Check: `grep -n "vec!\[None; plan.point_count\]" rust/geom_kernel/src/segment_batch.rs`
— expect one hit. A `if plan.segment_count == 0 { Vec::new() }` guard anywhere
near it is the defect.

Guarded by `kn_point_nearest_segment_batch.rs::an_empty_segment_set_yields_one_none_per_point_not_an_empty_batch`.

## 4. FOUR PRE-EXISTING TESTS THAT DEFECT B'S FIX BREAKS — all four repaired

Every one of these fails because its FIXTURE contains a degenerate face and was
silently relying on the `length.max(1e-30)` behaviour. The new error is right;
the fixtures are what must move. All four repairs below were applied and
verified together: **126/126 green** in `scratchpad/verify-salvage/rust/geom_kernel`,
saved as `scratchpad/review/kn-fixture-repairs.patch`.

**1 + 2. `src/parity.rs` `memory_tests`** (lines ~677 and ~717). Both use
`faces = [[0, 0, 0]]` purely as a cheap way to give the mesh some BOUNDS; the
faces are incidental to what is being tested (grid-dimension and flat-id
overflow). Replace them with real triangles spanning the same bounds — the
assertions are untouched and still pass:

```rust
// grid_dimension_rejects_saturating_quotient_without_debug_release_divergence
let vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
let faces = [[0, 1, 2]];

// grid_layout_rejects_flat_id_overflow_before_estimator_builder_diverge
let vertices = [
    [0.0, 0.0, 0.0],
    [overflowing_extent, overflowing_extent, 0.0],
    [0.0, 0.0, overflowing_extent],
];
let faces = [[0, 1, 2]];
```

**3. `tests/native_api.rs:2146` `indexed_repeated_index_face_keeps_kernel_degenerate_behavior`.**
This test does not merely trip over the defect — it FREEZES it:

```rust
assert_eq!(surface.geometry().normals(), &[[0.0, 0.0, 0.0]]);
```

This is the OD-8 case. The expected value moves, and the argument that the new
value is RIGHT is not "the code now does this": a zero vector is not a normal,
every consumer of `normals()` reads it as a direction, and the old value was the
thing that made the defect invisible. Rewrite as a rejection, and rename the test
so it stops advertising the old behaviour:

```rust
assert_eq!(
    NativeSurface::from_indexed(&[[0.0, 0.0, 0.0]], &[[0, 0, 0]], config).err(),
    Some(SurfaceBuildError::DegenerateFace {
        face: 0,
        cause: DegenerateFaceCause::RepeatedVertexIndex { corners: (0, 1), index: 0 },
    })
);
```

(Verified exact output: `DegenerateFace { face: 0, cause: RepeatedVertexIndex { corners: (0, 1), index: 0 } }`.)

**4. `tests/native_api.rs:175` `slice_z_batch_matches_frozen_shared_edge_coplanar_vertex_and_tolerance_fixture`.**
Face index 2 of the frozen slicing fixture is `[0, 1, 4]` = `(0,0,0)`, `(1,0,0)`,
`(2,0,0)` — three points on the x axis, **collinear**, commented "fully coplanar
patch". Verified exact output: `DegenerateFace { face: 2, cause: CollinearVertices }`.

So this fixture has been slicing against a face with no plane since it was
frozen. I expected this to be the hard OD-8 case and it is not: giving face 2 a
real third corner while keeping it entirely in the z = 0 plane preserves the
"fully coplanar patch" intent, and **every frozen expectation below it holds
unchanged** — `plane_offsets == vec![0, 3]`, the `face_id` list `vec![4, 0, 6]`,
all of it. No expected value moves, so there is no OD-8 decision to make here:

```rust
[0, 1, 5],  // fully coplanar patch   (was [0, 1, 4], which was collinear)
```

Vertex 5 is `(2.0, 1.0, 0.0)`, already in the fixture and already in the z = 0
plane. Verified: the test passes, and the whole crate goes to 126/126.

## 5. CALL SITES STILL ON THE LOSSY SHIM — the migration pass

Thirteen production call sites still use the float entry points. Each one needs
the same decision made explicitly: what does an untested row mean to THIS stage?
Refusing stages take `clears_or(false)`; pruning stages may take
`clears_or(true)` and should say why in a comment.

```
crates/plinth-support/src/rewire.rs:1077     <- the LIVE bug (verdict at :1088)
crates/plinth-support/src/brace.rs:1850
crates/plinth-support/src/brace.rs:2884
crates/plinth-support/src/brace.rs:3436
crates/plinth-support/src/brace.rs:5593
crates/plinth-support/src/brace.rs:5867
crates/plinth-support/src/contact.rs:759
crates/plinth-support/src/contact.rs:809
crates/plinth-support/src/cco.rs:4228
crates/plinth-support/src/cco.rs:8564
crates/plinth-support/src/cco.rs:8610
crates/plinth-support/src/cco.rs:8667
crates/plinth-support/src/cco.rs:8774
```

The shims are NOT marked `#[deprecated]`, on purpose: `docs/development.md:39`
documents the gate as `cargo clippy --workspace --all-targets --all-features --
-D warnings` with an expected count of 0, so thirteen deprecation warnings would
become thirteen build ERRORS in crates other lanes own. This list is the
follow-up pass instead.

`SegmentClearance::to_lossy_pair` exists so a migration can be done in two steps:
call the typed form, re-encode, prove the numbers are bit-identical, and only
then change what the site does with them.

## 6. THE REGRESSION TESTS, AND THEIR TEETH

Three new files at `623efec`, prefixed `kn_` so they cannot collide:

| file | tests |
|---|---|
| `rust/geom_kernel/tests/kn_segment_clearance_unmeasured.rs` | 6 |
| `rust/geom_kernel/tests/kn_point_nearest_segment_batch.rs` | 6 |
| `rust/geom_kernel/tests/kn_degenerate_face.rs` | 7 |

Teeth were proved by reverting each fix in an isolated copy of the crate (never
in the shared checkout) and re-running:

| mutation | result |
|---|---|
| B: restore `length.max(1e-30)`, drop the typed error | `kn_degenerate_face` **5 of 7 FAIL**; the two controls (sound mesh keeps unit normals, thin sliver still builds) correctly still pass |
| A: delete the `AllFacesExcluded` early return | `kn_segment_clearance_unmeasured` **2 FAIL** |
| A: return `Measured { clearance_mm: INFINITY }` for all three unmeasured cases (the exact pre-fix semantics) | `kn_segment_clearance_unmeasured` **2 FAIL** |
| D: return `Vec::new()` when `segment_count == 0` | `an_empty_segment_set_yields_one_none_per_point_not_an_empty_batch` **FAILS** |
| D: loosen the tie test from `<` to `<=` | `a_tie_resolves_to_the_lowest_segment_index` **FAILS** |

All restored, all green afterwards.

**One honest gap.** A sixth mutation — pointing the reduction at
`Association::FrozenJoint` while the dense form kept `AcceptedDistance`, i.e.
simulating exactly the "second copy of the predicate" this function exists to
prevent — did **NOT** fail. On the tidy fixture coordinates the two associations
(`(p - s) - t*a` versus `p - (s + t*a)`) round to the same double, so
`the_nearest_distance_is_bit_identical_to_the_dense_row_minimum` cannot see the
difference. It has teeth against a wrong row or a wrong index, not against
low-bit association drift.

Closing that needs a discriminating fixture: 1e8-scale segment origins against
millimetre-scale offsets, with query points placed ON the axis so the residual is
pure rounding and dominates the norm. A drafted test doing this over a fixed-LCG
sweep of 80 points x 40 segments was written but lost in the reset and is **not**
in `623efec`. It is worth redoing — until then, treat the bit-identity claim as
covering gross divergence only.

## 7. CONSTANTS (defect C) — NOT DONE, and one live symptom

`consts.rs` was untouched by this lane. `ZDEDUP = 5e-3` and
`NEAR_WALL_SKIP = 0.05` still carry a header promising a link to
`geometry_exact.py` source lines for a file that no longer exists, and neither
has a measurement of record. The thin-wall investigation
(`rust/geom_kernel/tests/near_wall_thin_feature.rs`, present at `623efec`) was
another lane's and is unverified by me.

One concrete observation, from a real compiler warning during this lane's builds:

```
warning: constant `NEAR_WALL_SKIP` is never used
  --> rust/geom_kernel/src/consts.rs:35:11
```

At some point mid-run `NEAR_WALL_SKIP` had no remaining reader, which means
`cone_thickness` in `queries.rs:2930` had stopped using it. Whoever picks up
defect C should establish whether that was a deliberate step or a casualty of the
same race — a constant this lane's brief calls outcome-deciding should not go
unreferenced by accident.
