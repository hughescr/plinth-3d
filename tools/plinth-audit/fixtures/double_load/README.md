# Fixture: `double_load`

## What is planted

One region of model surface is counted, in the engine's own books, as carried
by two different contacts.

The model is a flat box. Its bottom face is a single, flat, fully
down-facing 5 mm x 4 mm rectangle -- `D_total = 20.000 mm^2` of demand
(`docs/specs/audit-invariants.md` §2.1), and nothing else on the model faces
down. Two support tips (`contact_id` 0 and 4) are seated 2.5 mm apart on that
one rectangle, each properly embedded (`h_delivered = 0.300 mm`, comfortably
above its own `h_req = 0.2782 mm` at `ball_factor = 1.5`). Each tip's radius
is **`r_tip = 0.7283657 mm`, sized via `A_implied = x_allowed * pi *
r_tip^2` as though that ONE tip alone carried the WHOLE 20 mm^2 patch** --
not half of it, the whole thing, twice.

So the engine's declared books say this patch supplies

```
implied_lo = A_implied(0) + A_implied(4) = 20.000002 + 20.000002 = 40.000004 mm^2
```

against a reconstructed demand of `D_total = 20.000 mm^2` -- the patch is
booked at exactly double what it actually presents. The audit's own Voronoi
partition (§2.2, nearest-contact, no reach limit) is honest about the
geometry: it splits the rectangle at its midline and gives each contact
`A_owned = 10.000 mm^2`, which is the correct, non-double-counting answer.
The defect lives entirely in `A_implied`, i.e. in how the tips were *sized*,
not in how the audit partitions the surface.

## Where

`double_load.chitubox`, one model, 8 support records (two identical
tip -> taper -> member -> raft-pad chains, `x = -1.25 mm` and `x = +1.25 mm`,
`y = 0`):

| record | kind | contact/parent |
|---|---|---|
| 0 | TIP (type 7) | `contact_id = 0`, `r1 = r2 = 1.0925484 mm` (= `f * r_tip`, `f = 1.5`) |
| 1 | TAPER (type 1) | narrow end `r_start = 0.7283657 mm` = this tip's `r_tip` |
| 2 | MEMBER (type 3) | constant `r = 0.8 mm` |
| 3 | RAFT pad (type 6) | diameter 2.2 mm, height 0.2 mm, sits on the plate |
| 4 | TIP (type 7) | `contact_id = 4`, same radii as record 0 |
| 5 | TAPER (type 1) | same as record 1 |
| 6 | MEMBER (type 3) | same as record 2 |
| 7 | RAFT pad (type 6) | same as record 3 |

`z_plate = -7.0300002 mm` (`translate_mm.z = 5.03`, deliberately not a
multiple of `layer_height_mm = 0.05` -- see "bytes changed / construction
notes" below for why).

## Why this is a defect in physical terms

Contact area is the thing standing between the model and a print failure
(`docs/physics.md`, the owner's 2026-08-30 ruling bound to I2: "contact area
still governs, and depth never overrides it"). A tip's radius is the engine's
own claim about how much of the down-facing surface it is responsible for
carrying -- that is exactly what `A_implied = x_allowed * pi * r_tip^2`
recovers by inversion (§2.3): the engine sizes every real tip to exactly
`X = 12.000` (measured 2026-08-30, 7e-15 max deviation), so the radius *is*
the claim.

Here the same 20 mm^2 patch backs *two* such claims at full size. Read
naively from the support-record radii alone (as the current project metric
and the emitted-file record layout do -- there is no field anywhere that
declares "this patch is shared" or "this tip owns only half"), it looks like
the file is carrying 40 mm^2 of redundant, doubly-verified support over that
rectangle. It is not: it is carrying 20 mm^2 once, with two tips each sized
as if the other did not exist. That is not a safety margin, it is a
bookkeeping fiction -- indistinguishable, from the numbers alone, from a
support forest that assigns the same load twice and never notices that
either tip could be pulled to half its size, or that the two tips are
competing for the same material rather than reinforcing each other.

(`docs/physics.md` §4 does allow that *some* sharing between nearby contacts
is real -- I4's own "smallest defect it cannot see" section names this and
says a books-vs-demand ratio "near one" may be honest sharing. This fixture
is not that: it plants a ratio of exactly 2, i.e. each tip is booked as if it
alone carried the entire patch, not as if the two of them jointly and
plausibly shared it.)

## What the audit says about it (measured, current source tree)

Built and run against a clean snapshot of the current `tools/plinth-audit`
source (see "provenance" below for why a snapshot rather than the live
shared tree). `i4_load_ownership` fails, and only for the planted reason:

```
LOAD_BOOKS_DISAGREE file=... model=0 d_total_mm2=20 implied_lo_mm2=40.00000431860326
  implied_hi_mm2=40.00000431860326 clamped_contacts=0 ratio=0.49999994601746506
```

The invariant's own `measured.clauses` block shows the other four gating
clauses passing cleanly on this file, isolating the failure to clause (d):

```
"clauses": {
  "a_unowned":                    "pass",   -- 0 mm^2 unowned, 0.0 %
  "b_partition_identity":         "pass",   -- accounted 20 == D_total 20
  "c_component_routing":          "pass",   -- both contacts -> the one surface component
  "d_books_bracket":              "fail",   -- 20 mm^2 demand vs a [40, 40] mm^2 book
  "e_demand_double_reconstruction":"pass"   -- raster D_total agrees within its own bound
}
```

Per-contact rows confirm the partition split the patch correctly (10 mm^2
each, summing to the true 20) while each contact's own declared book still
claims the full 20:

```
contact 0: a_owned_mm2=10, a_implied_mm2=20.00000216, r_tip_mm=0.72836566
contact 4: a_owned_mm2=10, a_implied_mm2=20.00000216, r_tip_mm=0.72836566
```

`ratio = d_total_mm2 / implied_lo_mm2 = 0.5` -- exactly double-booked, by
construction.

**No other check fires for a reason connected to this defect.** On the
current tree: `i1_seat_depth` PASS, `i2_contact_area` PASS, `i5_closed_load_path`
PASS, `i6_no_intersection` PASS, `i7_island_coverage` PASS. `i3_min_section`
is still the unimplemented stub (`AUDIT_CHECK_NOT_IMPLEMENTED`, which the
harness reports PENDING, never satisfied, for any fixture that names it) --
that is a property of the current tree, not of this file, and it is expected
to clear on its own once I3 lands. See "known non-target failure" below for
`i8_no_dead_geometry`, which is NOT clean on this file for a reason that has
nothing to do with the planted defect.

## Bytes changed / construction notes

This fixture was **synthesised directly** with
[`plinth_audit::synth`](../../src/synth.rs) rather than built by perturbing a
real emitted artifact, because the planted defect is about how the two tips'
radii were *chosen* relative to each other and to the shared patch -- there
is no single real emitted file with exactly one isolated, symmetric,
two-contact patch to perturb cleanly, and hand-picking the exact radii needed
for an unambiguous ratio-of-2 requires generating from the geometry rather
than nudging an existing file's bytes. The generator (kept alongside this
fixture as `generate.rs`, a standalone `fn main` -- not part of the crate,
not built by its `Cargo.toml`, kept only as the exact record of how the bytes
were produced) computes:

- `r_tip = sqrt(D_total / (x_allowed * pi))` with `D_total = 20.0` --
  the radius a SINGLE honest contact would need to carry the whole patch --
  and gives BOTH contacts that same radius.
- `translate_mm.z = 5.03`, not `5.0`: with a round `5.0` the model's entry
  plane (`z = -2.0`) lands exactly on a raster layer boundary
  (`(z_entry - z_plate) / layer_height_mm` was exactly `100`), which trips
  `i2_contact_area`'s coincident-face handling and reports
  `raster_exceeds_ideal` -- a real, reproducible edge case in that check
  worth knowing about, but not one this fixture is trying to demonstrate.
  Offsetting the lift by 0.03 mm moves the cut plane off the lattice
  boundary and `i2_contact_area` passes cleanly, which is what isolates the
  fixture to the one planted defect.
- The TIP soup is `synth::sphere_soup(...)` with its zero-area pole
  triangles filtered out before use (`drop_zero_area` in `generate.rs`).
  `sphere_soup`'s `la == 0` band emits triangles with two bit-identical
  vertices (both computed from `sin(0.0) == 0.0` exactly), which
  `i8_no_dead_geometry` correctly flags `GEOM_DEGENERATE
  reason=zero_area_triangle`. This is a **pre-existing defect in the shared
  test helper** (`tools/plinth-audit/src/synth.rs`), not something planted
  here or something this fixture is entitled to fix (this directory owns
  only itself) -- see "known non-target failure" below for the proof that it
  is not this fixture's doing.
- The RAFT pad soup is built by hand
  (`frustum_soup(base, r, base + [0,0,height], r, 20)`, a cone with equal end
  radii, i.e. exactly the cylinder `Primitive::RaftPad`'s own
  `signed_distance` checks a raft against) because
  `SynthRecord::with_matching_soup` does not cover `RaftPadOrBridge` at all
  (it returns an empty soup for that kind, by that function's own doc
  comment), and an empty soup is itself `GEOM_DEGENERATE reason=empty_soup`.

Both of the last two points are worked around here (not fixed at the
source) purely so this fixture's own `i8_no_dead_geometry` reading is not
polluted by unrelated, already-known synth-helper gaps; `tools/plinth-audit/src/synth.rs`
is not owned by this directory.

## Known non-target failure: `i8_no_dead_geometry` (not caused by this fixture)

With the two workarounds above applied, `degenerate_count = 0` on this file
(no `GEOM_DEGENERATE` finding of any kind), but `dead_count = 4`: both
TAPERs and both MEMBERs are reported `GEOM_DEAD_MEMBER`.

This is **not** an artifact of the double-load plant. It reproduces exactly
on `i8_no_dead_geometry`'s own bundled, single-contact, non-double-loaded
reference fixture,
`checks::dead_geometry::tests::clean_chain_file` -- running that check's own
`a_fully_clean_grounded_chain_has_no_degenerate_and_no_dead_geometry` test
against the current tree fails the same way (confirmed directly: `cargo test
--lib checks::dead_geometry::tests::a_fully_clean...` fails on an unmodified
checkout, asserting `degenerate_count == Some(0.0)` when the actual value is
`Some(2.0)`, and separately reporting the same two `GEOM_DEAD_MEMBER`
findings for its taper and member once the pre-existing synth-soup issues
above are also worked around for it).

The mechanism, traced with a small out-of-tree probe against
`dead_geometry.rs`'s own public API (`SupportGraph::distance_to_ground` /
`ground_reachable_without`): the check's grounding predicate treats a
properly embedded TIP as its own valid "ground" node (module doc, clause
(ii): "a TIP whose I1 predicate is established"). That is correct on its
own terms -- a tip really is anchored once embedded. But it means role R2
("removing this record disconnects at least one live TIP from ground") can
**never** fire for a plain taper or member sitting between a live tip and
the plate: the tip is already counted as its own ground point regardless of
whether the arm beneath it is removed, so removing the arm never changes
`dist[tip].is_some()` from true, and `ground_reachable_without` still marks
the tip reachable (it seeds directly from the tip itself). Roles R1
(tip-only), R3 (degree-3 braces only, and this is a plain degree-2 chain)
and R4 (raft-only) do not apply to a taper or a member either. The result:
**a plain straight arm segment directly under an already-embedded tip has no
role in R1-R4 that can ever mark it live**, on this file or on the check's
own reference fixture. Confirmed with a probe printing `is_ground(r)` for
every record and `ground_reachable_without(2, is_ground)`:
`is_ground(2) [member] = true` via the plate-touch clause, `dist =
[Some(0), Some(1), Some(0), Some(0), ...]` (the tip is distance 0, i.e.
already its own ground), and removing the member leaves every other record
`reach = true` because the tip re-grounds itself independently.

This looks like a genuine gap in `i8_no_dead_geometry`'s liveness role model
(not disclosed in that file's own "Known simplifications" section, which
covers two different, narrower issues), reported here as observed evidence
rather than fixed, since `src/checks/dead_geometry.rs` is not owned by this
directory. **Do not read `i8`'s failure on this file as evidence about the
double-load plant** -- it is orthogonal, and it reproduces with the plant
removed entirely.

## Fixture-harness layout note

`tests/fixtures.rs`'s `fixtures_dir()` reads `*.expect.json` directly from
`tools/plinth-audit/fixtures/` (non-recursive `std::fs::read_dir`), one flat
pair per fixture. This fixture was assigned its own subdirectory,
`fixtures/double_load/`, for write isolation while several fixtures were
being built in the same checkout in parallel. **As shipped, the harness will
not discover `fixtures/double_load/double_load.expect.json` in its current,
non-recursive form** -- either the harness needs to walk one level of
subdirectories, or this pair needs to be promoted to
`fixtures/double_load.chitubox` / `fixtures/double_load.expect.json` at
integration time. Neither `tests/fixtures.rs` nor the top-level `fixtures/`
listing is owned by this directory, so that reconciliation is left to
whoever integrates the parallel fixture lanes.
