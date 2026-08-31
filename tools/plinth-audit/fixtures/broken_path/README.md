# `broken_path` -- I5 (closed load path) adversarial fixtures

Two planted defects, one per file, proving `i5_closed_load_path` actually
fires on the two failure modes `docs/specs/audit-invariants.md` §3.5 names by
number: a cycle whose only route to ground is through itself (clause (c)),
and an arm that terminates in free space (clause (b)).

## Files

| file | defect | expected code | expected count |
|---|---|---|---|
| `broken_path_cycle.chitubox` + `.expect.json` | orphaned triangular loop | `PATH_ORPHAN` | 6 |
| `broken_path_free_end.chitubox` + `.expect.json` | dangling stub | `PATH_FREE_END` | 1 |
| `broken_path_control.chitubox` | **not a fixture** (no `.expect.json`) -- the shared baseline with *neither* defect, kept only as evidence; see "Isolation" below |

All three share one baseline: a single legitimate support column (TIP ->
TAPER -> MEMBER -> raft PAD, records 0-3) straight down the model's Z axis
to the plate. `broken_path_cycle.chitubox` and `broken_path_free_end.chitubox`
are that same baseline plus exactly one planted defect appended as extra
records. The defect is additive, never a mutation of an existing byte, so
"what changed" is exactly "the records after index 3."

Built with `plinth_audit::synth` from a small standalone builder (not checked
in; the file's content is what matters, not the tool that wrote it). Every
number below was read back from the actual audit binary, not computed by hand
and asserted — the two exceptions are noted where hand arithmetic is quoted
alongside the audit's own number for cross-check.

## Defect 1 -- the cycle (`broken_path_cycle.chitubox`)

Records 4-9, appended after the baseline's 4:

```
r4  JointSphere  centre (20.0,  0.0,   -3.0)  r=0.35
r5  JointSphere  centre (23.0,  0.0,   -3.0)  r=0.35
r6  JointSphere  centre (21.5,  2.598, -3.0)  r=0.35   (equilateral, side 3.0 mm)
r7  Member       (20.0,0.0,-3.0) -> (23.0,0.0,-3.0)   r=0.35
r8  Member       (23.0,0.0,-3.0) -> (21.5,2.598,-3.0) r=0.35
r9  Member       (21.5,2.598,-3.0) -> (20.0,0.0,-3.0) r=0.35
```

Three joint balls at the vertices of an equilateral triangle, three rods for
its edges -- a plain triangular brace, floating in air: centred at x=20-23mm,
14-19mm sideways of the model's own x in [-5,5] footprint, 3.6mm above the
plate (z_plate=-7.0, ball bottom at -3.35, plate+layer_height at -6.95; by
hand: -3.35-(-6.95)=3.60mm, and the audit's own `nearest_ground_mm` line
below reports exactly that). It shares no point and no volume with the
baseline column, the model mesh, or the plate.

Every arm's endpoint is placed exactly at its neighbouring ball's centre, so
the rods run directly through real ball volume (radius 0.35mm) rather than
merely touching a boundary plane -- see "the touching-vs-overlapping
distinction" below for why that matters under I5's actual implementation.

This is a **cycle**: three joints, three edges, each joint has degree 2. It
is legitimate bracing shape-wise (§3.5's own text: "A cycle in the undirected
connection graph is legitimate -- bracing makes them on purpose"), and the
ONLY thing wrong with it is that none of its six primitives has any route to
the plate or to a valid model seat. That is exactly clause (c)'s target case,
stated in the spec's own words: "a set of primitives whose only route to
ground is through each other."

**Observed audit output** (`plinth-audit fixtures/broken_path/broken_path_cycle.chitubox --json`):

```
I5 closed load path  FAIL  10 checked, 6 failed, 0 indeterminate
  6 orphaned, 0 free ends, 0 indeterminate (of 10 records)
```

All 6 failures are `PATH_ORPHAN`, one per planted record (4-9), each reading:

```
PATH_ORPHAN file=... model=0 record=<4..9> type=<joint|member> component_size=6 nearest_ground_mm=3.600000
```

`component_size=6` is exactly the six planted records; `nearest_ground_mm=3.6`
matches the hand vertical-clearance computation above. The baseline's own 4
records (0-3) contribute **zero** I5 failures -- see "Isolation."

## Defect 2 -- the free end (`broken_path_free_end.chitubox`)

Record 4, appended after the baseline's 4:

```
r4  Member  (0.0,0.0,-4.0) -> (4.0,0.0,-4.0)  r=0.35
```

Its **start** point (0,0,-4.0) is the exact coordinate of the baseline's own
TAPER-end / MEMBER-start junction -- the same point already shared by
records 1 and 2 -- so this stub is a real branch off the model's own
load-bearing chain, not a second isolated island. Its **end** point
(4,0,-4.0) touches nothing: 2mm below the model's bottom face (z=-2) minus
its own 0.35mm radius clears the model by 1.65mm; 2.05mm above the plate
(z=-6.95) minus radius clears the plate by 1.70mm; and it shares no point
with any other record. (The audit's own `nearest_solid_mm=3.65` below is the
authoritative number; the two clearances above are cheaper independent hand
checks that it is not accidentally zero.)

This is clause (b)'s case: "for each frustum end disc that is neither
grounded nor overlapped by another primitive's solid, that end is in free
space" -- an arm that starts attached to a real structure and simply stops,
mid-air, at its far end.

**Observed audit output**:

```
I5 closed load path  FAIL  5 checked, 1 failed, 0 indeterminate
  0 orphaned, 1 free ends, 0 indeterminate (of 5 records)
```

The one failure:

```
PATH_FREE_END file=... model=0 record=4 type=member end=end point=4.000000,0.000000,-4.000000 radius_mm=0.350000 nearest_solid_mm=3.650000
```

`end=end` correctly identifies the FAR end as the free one, not the
attached start. The baseline's own 4 records contribute zero I5 failures.

## Isolation -- proving the defect, and nothing else, is what I5 sees

`broken_path_control.chitubox` is the baseline alone (records 0-3, no
appended defect). Running all three files through the same audit binary and
comparing I5's status:

| file | I5 status | I5 detail |
|---|---|---|
| control | **pass** | `4 checked, 0 failed, 0 indeterminate` -- `0 orphaned, 0 free ends, 0 indeterminate (of 4 records)` |
| cycle | FAIL | `10 checked, 6 failed` -- exactly the 6 planted records, as above |
| free_end | FAIL | `5 checked, 1 failed` -- exactly the 1 planted record, as above |

The baseline alone passes I5 cleanly. Each defect fixture fails I5 by
exactly the number of records the defect added, and the baseline's own
records never appear in either failure list. The defect is what moved I5
from pass to fail, and the count matches the planted geometry exactly.

## The touching-vs-overlapping distinction, and why it mattered

I5's own implementation (`src/checks/load_path.rs`, landed mid-way through
building this fixture -- it was still the `not_implemented` stub when work
started) builds its OWN connection graph from `Primitive::separation_mm`
being **strictly negative** (real solid overlap), deliberately not reusing
`crate::graph::SupportGraph`'s endpoint-coincidence joins. Two solids that
only *touch* -- share a boundary point or plane, separation exactly 0 -- are
NOT an edge in this graph; a separation in `(0, d3]` is worse, an
indeterminate edge that itself fails I5 (`PATH_INDETERMINATE`,
`reason=fusion_uncertain`).

A first draft of the baseline column ended the MEMBER exactly at the raft
PAD's top surface (`z=-6.8`, one raft height above the pad's own connective
point at `z=-7.0`). That is a textbook **touching** case: the member's
bottom face and the pad's top face share a plane with separation exactly 0 --
inside the fusion-uncertain band, not an overlap. The baseline was changed to
extend the MEMBER all the way down to the pad's own base point (`z=-7.0`),
so the two solids share real volume over the pad's whole 0.2mm height, and
the member's own solid crosses the plate half-space directly regardless of
the pad. The cycle's own three joints were built the same way from the
start: every rod's endpoint sits exactly at its ball's *centre*, deep inside
0.35mm of real ball volume, never merely at the ball's surface.

This is why the fixture cannot be built by "connect two frustums at a shared
point" the way the project's other existing chain-shaped test fixtures do
(see "Findings" below) -- under I5's actual (overlap, not coincidence) graph,
that shared-point pattern is a touching pair, not a joined one, unless one of
the two solids has enough of its own volume to make the junction a real
overlap (a ball, or a raft's own height).

## Baseline tuning, and why every other gating check was re-derived by hand

The task's acceptance test has two halves: I5 must fail for the planted
reason, and **no other check may fail because of it**. Three iterations were
needed to get the shared baseline (and hence both fixtures) genuinely clean
under every OTHER check as it was implemented, mid-session, out from under
this fixture:

1. **Seat depth (I1).** A first draft embedded the tip ball 0.10mm past the
   model surface, matching the ruled `default_contact_depth_mm=0.100`. Once
   I1 landed, it reported `worst deficit 0.0146mm`: for this tip's radius
   (`r_tip=0.3`), `h_required = ball_seat_factor(1.5)*r_tip = 0.11459mm`
   dominates the flat default, so 0.10mm falls short. Fixed by embedding
   0.15mm instead (declared depth raised to match). **I1 now passes** on all
   three files identically.
2. **The member/pad junction.** Covered above -- fixed by extending the
   member to the pad's own base point.
3. **`sphere_soup`'s pole triangles (a finding, not a fixture change).** See
   below; the tip ball and the cycle's three joint balls use a hand-written
   sphere generator instead of the shared helper for this reason.

After these fixes, **I1, I6 and I7 pass identically on all three files**
(control, cycle, free_end), and **I2, I3 and I4 fail identically on all
three files** -- same numbers to the decimal, confirmed by diffing the
actual audit output:

```
I2  worst X delivered 265.5 against 12, median raster loss -0.066         (all three, identical)
I3  worst A_min 0.3753 mm2 needs 8.3333                                   (all three, identical)
I4  books [3.3929198861136056, 3.3929198861136056] mm2, demand 100 mm2    (all three, identical)
```

**These three are baseline properties, not consequences of either planted
defect**, and this fixture does not attempt to fix them, for a reason worth
stating plainly: I2/I3/I4 grade whether a SINGLE tip's declared load-bearing
capacity (`x_allowed * pi * r_tip^2`, fixed by the tip's own radius) matches
the demand a real print's engine-sized support forest would place on it, and
that arithmetic has no solution for a hand-built single-tip fixture -- there
is no `r_tip` for which one contact's capacity equals a whole 100mm2 face's
demand while also satisfying I1's depth requirement and I3's own section-law
threshold at the SAME radius. This is not particular to this fixture: running
the audit against the project's own real engine artifact
(`exports/sphere-first-artifact.chitubox`, 11 real contacts) shows the SAME
three checks failing there too (I2, I3, I4 all FAIL, alongside I1 -- the
corpus's own documented `#2 finding "seat depth violations"` etc.). A
single-column toy fixture cannot pass I2/I3/I4 by construction; only a
correctly-engine-sized multi-contact forest can, and building one was out of
scope for an I5 fixture. Reported here as a genuine, checked fact rather than
worked around.

### One exception: I8 is NOT fully isolated on the cycle fixture, and this is structural rather than a fixture defect

I8 (`i8_no_dead_geometry`) fails on **all three** files, but not identically:

```
control    2 checked-dead (records 1, 2 -- the baseline's own TAPER and MEMBER)
free_end   1 checked-dead (record 1 only -- see note below)
cycle      8 checked-dead (records 1, 2, AND all six of records 4-9)
```

The baseline's TAPER (record 1) is dead-flagged in every file for a reason
that has nothing to do with either planted defect: I8's `is_ground`
predicate (like I5's) treats a TIP with a valid seat as grounded **on its
own**, independent of what connects to it. That decouples the TAPER from
"critical to the tip's grounding" (R2), and it is not itself a TIP (no R1),
does not bridge a degree-3 junction (R3), and is not a raft (R4) -- so it has
no role under any of I8's four liveness tests, on a single-column chain, by
construction. This reproduces identically on the neutral control, so it is a
baseline property, not something either fixture introduces (worth flagging
upstream: a straight, ordinary support column's own taper appears to be
dead-flagged by I8 as currently written, on any single-tip chain, not just
this one).

The cycle fixture's own six planted records are **also** flagged dead by I8,
and this is not avoidable by construction: a record with none of I8's R1-R4
roles is "dead" by definition, and an island with no route to a live tip and
no route to ground can never satisfy R1 (not a tip), R2 (nothing depends on
it reaching ground, because nothing downstream of it is a live, reachable
tip), R3 (no non-ground degree-3 junction among its neighbours in this
triangle shape), or R4 (not a raft). Being orphaned (I5's finding) and being
functionally dead (I8's finding) are two different instruments observing the
*same* underlying fact -- no route to ground -- and there is no floating,
disconnected island shape that is orphaned under I5 but alive under I8. The
free_end fixture does not have this overlap: its one planted record (4) is
NOT dead-flagged (it is a live branch off a load-bearing point, even though
its far end is free), so free_end fails I5 alone among the two
now-implemented checks that were not already failing on the neutral control.

## Findings, recorded because they change how other fixtures should be read

1. **`plinth_audit::synth::sphere_soup` emits 16 zero-area triangles at each
   pole of every ball it builds** (`la=0` and `la=lat-1` each reuse the same
   pole vertex for two corners of one triangle, since `sin(0)=0` erases the
   longitude term). Once I8 went from stub to implemented mid-session, this
   surfaced immediately: the tip ball built through the shared helper
   reported `GEOM_DEGENERATE reason=zero_area_triangle value=16`, and
   because I8 excludes a degenerate record from ALL liveness scoring, that
   one bad tip cascaded into `GEOM_DEAD_MEMBER` on the taper AND the member
   too (their only route to "live" was through a tip I8 had just discarded
   as unreadable). This is not particular to this fixture: the project's own
   `tests/fixtures.rs::clean_artifact()` and `src/graph.rs`'s `chain()` /
   `ball()` test helpers build balls the same way, and will show the same
   cascade under I8 today. Both fixtures here instead use a hand-written
   pole-fan sphere generator (in the (uncommitted) builder tool, not in
   `synth.rs`, which this agent does not own) that puts a proper triangle fan
   at each pole. Not fixed upstream because `synth.rs` is outside this
   agent's owned scope; flagged here for whoever owns it.
2. **`SynthRecord::with_matching_soup()` returns an empty soup for
   `RaftPadOrBridge`** (`synth.rs`'s own doc comment: "This is a test
   instrument, not a second emitter"). An empty soup is `GEOM_DEGENERATE
   reason=empty_soup` under I8. Both fixtures here instead give the raft PAD
   a real cylinder mesh (`frustum_soup` with equal top/bottom radius, which
   is exactly `RaftPad`'s own SDF shape at `ra==rb`) built in the builder
   tool, not a change to `synth.rs`.
3. **I5 landed non-stub partway through building this fixture** (it was
   still `CheckOutcome::not_implemented` when work started; `Overlapped`/
   `FusionUncertain` enum variants appeared in a routine rebuild). The
   fixture and this README describe the LANDED, non-stub behaviour, verified
   against the actual binary at every claim above -- nothing here is a
   prediction against the stub.

## A structural note on where this fixture had to live

The spawn brief for this task names `tools/plinth-audit/fixtures/broken_path/`
(a subdirectory) as the one directory this agent owns, and every file here
respects that boundary. `tests/fixtures.rs`'s own module documentation,
however, describes a **flat** namespace -- `fixtures/<name>.chitubox` +
`fixtures/<name>.expect.json` directly inside `fixtures/`, "so parallel
agents never touch the same file... there is deliberately no
`fixtures/manifest.json`" -- and its `fixtures_dir()` / `load_expectations()`
does a **non-recursive** `read_dir` over `fixtures/` itself. A pair placed one
level down, inside `fixtures/broken_path/`, is invisible to that scan: `cargo
test` in this crate will not discover `broken_path_cycle.expect.json` or
`broken_path_free_end.expect.json` as things stand, even though the audit
binary invoked directly against either `.chitubox` file (as quoted above)
proves the defect fires exactly as claimed. This is a coordination gap
between the subdirectory-per-agent ownership scheme and the harness's
documented flat-namespace contract, not something this agent can resolve
without either touching `tests/fixtures.rs` (not owned) or moving files to
the shared top-level `fixtures/` directory (outside the one directory this
agent was told to touch). Flagged for the orchestrator to reconcile -- most
likely by moving `broken_path_cycle.{chitubox,expect.json}` and
`broken_path_free_end.{chitubox,expect.json}` up to `fixtures/` directly, at
which point `cargo test` should pick them up unchanged.
