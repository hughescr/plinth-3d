# `starved_patch` — a contact whose cured patch is starved although its ideal disc passes

## What is planted, and where

One model (a flat-bottomed box), one support chain, synthesised directly with
`plinth_audit::synth` (a fixture whose whole point is a specific quantisation
outcome is cleanest built from exact numbers, not perturbed from a real
artifact — see "Construction notes" below).

**The single planted fact:** the tip's contact geometry is correct and
sufficiently deep by every quantity the record fields declare — but the
CURED (rasterised) contact patch, measured on the printer's own `0.014 x
0.019 x 0.050 mm` lattice under full-containment fill, is smaller than the
area law requires, even though the IDEAL geometric disc at the same depth
is not.

| record | kind | value |
|---|---|---|
| 0 | TIP (type 7) | `r1 = r2 = 0.2775 mm` (`= f * r_tip`, `f = ball_factor_stub = 1.5`); `start = p0 = (0,0,0.103)`, `end = (0,0,0.103 - 2*0.2775) = (0,0,-0.452)` |
| 1 | TAPER (type 1) | `start` = the ball's own connective point (its CENTRE, `(0,0,-0.0245)`, not a pole — `docs/specs/chitubox-format.md` §5); `r_start=0.185 mm` (`=r_tip`), widening to `r_end=0.35 mm`; declared contact depth (trailing slot 0) `= 0.103 mm`, matching `h_delivered` exactly |
| 2 | MEMBER (type 3) | constant `r = 0.35 mm`, plain design joint with the TAPER (no radius mismatch, no axial nudge — see "Known non-target failures") |
| 3 | RAFT pad (type 6) | diameter `2.2 mm`, height `0.2 mm`, sits on the plate |

Model mesh: a box, bottom face at `z = 0` (down-facing, outward normal
`(0,0,-1)`), footprint `1.2925562859 x 0.9982173443 mm` (chosen so its
demand — see below — lands as close to a target value as an `f32`-quantised
two-triangle rectangle can get), height `2 mm`. `translate_mm = (0, 0,
4.723210...)` so `z_plate = -translate_z` matches the frame check's own
reconstruction (the lowest non-raft support point, the MEMBER's end).

The tip's axis is exactly vertical (`u = (0,0,-1)`), so `p0 = (0,0,0.103)`
sits `0.103 mm` inside the box (`h_delivered`), embedding straight down
through the box's own bottom face.

## Why this is a defect in physical terms

The owner's 2026-08-30 ruling on seat depth is explicit and bound to one
condition: *"contact area still governs, and depth never overrides it."* A
shallower seat is a RETRACTED ball, not a trimmed one — its cut disc shrinks
as `a = sqrt(2*f*r_tip*h - h^2)` — and the whole point of I2 is to measure
that disc **as the slicer would actually cure it**, not as the algebra says
it should look.

Here the algebra says the seat is fine: `h_delivered = 0.103 mm` clears
`h_required = 0.100 mm` (the ruled default; the ball term at this `r_tip` is
only `0.0707 mm` and the incidence term is `0.050 mm`, both smaller), and the
ideal disc at that depth, `A_ideal = pi * (2*0.2775*0.103 - 0.103^2) =
0.14626 mm^2`, comfortably exceeds what this contact owes,
`A_required = A_owned / 12 = 0.10752 mm^2` (a 36% margin). Read the record
fields alone — which is exactly what the project's prior metric did — and
this tip looks properly sized and properly seated.

It is not. `0.185 mm` is a small tip radius relative to the printer's own
`0.014 x 0.019 mm` pixel pitch (about 13 x 10 pixels across the ball's own
diameter at this depth), and full-containment rasterisation — a pixel
column is only cured when its WHOLE footprint sits inside the solid at that
layer's sampling plane, never partially — erodes a disc this size by a real,
non-trivial fraction: **26.5% of the ideal area is lost to quantisation
alone**, measured on the actual cured voxel union, not estimated. The
resulting patch, `A_rast = 0.10746 mm^2`, is `0.053%` — a genuine, if small,
deterministically-computed deficit — short of what the contact owes. This is
not a borderline seat and not an undersized tip by the declared numbers; it
is exactly the gap the owner's ruling anticipated and bound I2 to close: the
geometry the file declares is fine, the pixels the printer will actually
cure are not quite enough.

## What the audit says about it (measured, current source tree, 2026-08-30)

Built and run against the current, shared `tools/plinth-audit` source
through `plinth_audit::run::audit_file` — the same entry point
`tests/fixtures.rs`'s harness uses — via a throwaway generator crate outside
this repository (see "Construction notes"). Every number below is read back
from that run, not computed by hand and asserted.

```
i1_seat_depth:        pass
i2_contact_area:      fail   CONTACT_AREA_SHORT              <- THE TARGET
i3_min_section:       fail   MIN_SECTION_INDETERMINATE       (see below)
i4_load_ownership:    fail   LOAD_BOOKS_DISAGREE             (see below)
i5_closed_load_path:  pass
i6_no_intersection:   pass
i7_island_coverage:   pass
i8_no_dead_geometry:  fail   GEOM_DEGENERATE, GEOM_DEAD_MEMBER, GEOM_INDETERMINATE  (see below)
```

`i2_contact_area`'s own per-contact row, exactly as measured:

```json
{
  "contact_id": 0,
  "r_tip_mm": 0.1850000023841858,
  "r_ball_mm": 0.2775000035762787,
  "h_delivered_mm": 0.10300000011920929,
  "a_ideal_mm2": 0.1462599900256828,
  "a_rast_mm2": 0.10746400000000061,
  "a_owned_mm2": 1.2902521030068541,
  "a_required_mm2": 0.10752100858390451,
  "deficit_mm2": 0.00005700858390389474,
  "raster_loss": 0.2652536077629277,
  "x_delivered": 12.006365880730726,
  "fails_even_at_ideal_disc": false,
  "patch_voxels": 404,
  "patch_components": 1,
  "status": "fail"
}
```

The failure line, exactly as emitted:

```
CONTACT_AREA_SHORT file=<path> model=0 contact=0 record=0 a_rast_mm2=0.10746400000000061
  a_required_mm2=0.10752100858390451 a_owned_mm2=1.2902521030068541 a_ideal_mm2=0.1462599900256828
  a_implied_mm2=1.2902521360855523 raster_loss=0.2652536077629277 x_delivered=12.006365880730726
```

**`fails_even_at_ideal_disc: false` is the load-bearing field.** It says the
shortfall is *not* a case where no fill rule could ever rescue this contact
(`a_ideal + quantisation_allowance + tol < a_required`, `checks/contact_area.rs`
§5) — the ideal disc genuinely clears the requirement by 36%. The failure is
the rasterisation gap alone, exactly the case I2 exists to catch and the
declared geometry alone cannot.

## Known non-target failures

I did not weaken the fixture to make these disappear, and I did not touch
any file under `src/checks/` to make them agree with me. Both are
disclosed rather than hidden, per the brief.

### `i4_load_ownership` — an `f32`-storage precision floor, not a design defect

```
LOAD_BOOKS_DISAGREE d_total_mm2=1.290252103006992 implied_lo_mm2=1.2902521360855523
  implied_hi_mm2=1.2902521360855523 clamped_contacts=0 ratio=0.999999974362716
```

Clause (d) requires `|D_total - A_implied| <= max(4096*eps64*D_total, 1e-9)`
— at this magnitude, an absolute **1e-9 mm²**. I sized the box footprint by
computing the target area from the tip radius exactly the way the check
does (`x_allowed * pi * r_tip_stored² `, `r_tip_stored` widened from the
same `f32` the record carries), then searched nearby `f32` half-widths (a
30,000-ULP window on each axis) for the pair whose product lands closest to
that target — the closest a two-triangle rectangular footprint stored in
`f32` can get. The achieved residual is `2.6e-8` relative (`3.3e-8 mm²`
absolute), about **33x** the tolerance floor — far tighter than a naive
choice (which lands two to three orders of magnitude worse), but still over
`tol`. This reproduces the identical, already-reported finding on the
`bare_island` sibling fixture (`ratio=1.0000000582`, ~370x over tolerance,
there attributed to "one `f32` quantisation step on a...coordinate...
squaring or multiplying any two independently-`f32`-rounded lengths cannot
land inside an `eps64`-scale tolerance, no matter which two lengths are
chosen") and on `shallow_seat` (`ratio=1.0000000363`). **This is not this
fixture's defect** — it is a property of clause (d)'s tolerance being far
tighter than single-precision wire storage can satisfy for any
hand-built, single-contact model, and it has now been independently
reproduced by at least three fixtures built by different agents in this
session.

### `i3_min_section` — plausibly a second, indeterminate symptom of the SAME planted defect

```
MIN_SECTION_INDETERMINATE contact=0 reason=section_component_empty
  ("0 of 1 contacts had a measurable section", stations_evaluated=0)
```

I3 walks rootward from the tip along `path_records=[0,1,2]` (tip, taper,
member — the walk stops at the member because it touches the plate
half-space, clause (i)) and seeds its first arc-length station on the SAME
cured per-contact raster I2 reads. `stations_evaluated: 0` means the very
first seed point — at or immediately past the seat, the same location I2
measures as a starved patch — was not itself a cured pixel. I did not
instrument `checks/narrowest_section.rs` (I do not own it) to confirm the
exact station coordinate, so I report this as plausible rather than proven:
**the same rasterisation starvation this fixture plants may be blinding I3's
own first-station seed at the same location**, which I3 reports as
indeterminate rather than as a short number. If that reading is right, this
is not unrelated collateral so much as a second, differently-shaped
symptom of the one planted defect. Either way, `src/checks/narrowest_section.rs`
is not owned by this directory.

### `i8_no_dead_geometry` — one precedented finding, one this fixture cannot fully trace

```
GEOM_DEGENERATE   record=3 (raft) reason=empty_soup
GEOM_DEAD_MEMBER  record=1 (taper) roles_tested=R1,R2,R3,R4
GEOM_INDETERMINATE record=2 (member) reason=path_indeterminate
```

* **`GEOM_DEGENERATE reason=empty_soup` on the raft** — `SynthRecord::
  with_matching_soup` returns an empty soup for `RecordKind::
  RaftPadOrBridge` by its own documented contract; this reproduces on every
  sibling fixture that calls it on a pad (`double_load`, `broken_path`'s
  baseline). A hand-built substitute soup was tried here and reverted: it
  made BOTH `i3` and `i8` report the SAME `*_INDETERMINATE reason=
  path_indeterminate` on the member and the pad, trading one well-understood
  collateral for a less legible one. Left as the documented, precedented
  empty soup on purpose.
* **`GEOM_DEAD_MEMBER` on the taper** — this reproduces `bare_island`'s and
  `shallow_seat`'s identically-explained finding exactly: the grounding
  predicate (shared with `load_path.rs`'s own reimplementation) treats a
  TIP whose own I1 seat is valid as itself grounded (the three-seat rule's
  clause (ii)). This tip's seat is comfortably valid by design (failing I1
  is not an option here — that would give I2's proof a second, unplanted
  reason to fail). Given that, removing the taper never "disconnects a live
  TIP from ground" — the tip is already its own ground — so R2 does not
  apply, and R1/R3/R4 do not apply to a plain taper either. A structural
  consequence of a minimal single-branch fixture, not a geometry mistake;
  see `bare_island/README.md`'s identical analysis for the full trace.
* **`GEOM_INDETERMINATE reason=path_indeterminate` on the member** — I could
  not fully trace this one. `i5_closed_load_path`'s own, independent overlap
  reconstruction reports this exact file's taper/member/pad joins as clean
  (0 indeterminate pairs, `status: pass`), so whatever `i8` is reading is
  not literally the same graph, or resolves it differently. I do not own
  `src/checks/dead_geometry.rs` and did not instrument it to trace the exact
  mechanism.

## Construction notes

* `r_tip = 0.185 mm`, `f = 1.5` → `R = f*r_tip = 0.2775 mm`, stored `f32`
  then re-read as `r_tip_recovered = 0.1850000023841858 mm` — the value
  I1/I2/I3/I4 all reconstruct from.
* `h_delivered = 0.103 mm`, chosen to clear `h_required = 0.100 mm` (the
  ruled default) with a real, if modest, 3% margin at normal incidence
  (`sin_theta = 0`).
* Box footprint area targeted at `x_allowed * pi * r_tip_recovered^2 =
  1.2902521028 mm^2` — the area a properly-sized `X = 12.000` contact would
  carry for this exact `r_tip` (the measured corpus fact: every real
  contact is sized to that ratio, max deviation `7e-15`). This is what
  keeps `i3_min_section`'s own arm-section ceiling satisfied by the SAME
  choice that (nearly) satisfies `i4`'s declared/reconstructed bracket, so
  only `i2`'s rasterisation gap is left as a clean, isolated target — see
  the halfwidth search in "Known non-target failures" above for how close
  the box footprint lands to that target.
* The `(r_tip, h_delivered)` pair was found by a coarse-then-fine numerical
  search over the REAL, currently-implemented `i2_contact_area` check (not
  a hand-rolled approximation of its rasteriser) — a large-scale hand
  approximation of the raster undercounted the true cured area
  significantly, so results were only trusted once read back from
  `plinth_audit::run::audit_file` itself.
* An axial-overlap fix was tried at the TAPER/MEMBER design joint at one
  point (a narrower taper waist plus a `0.0004 mm` axial nudge, comfortably
  inside `JOIN_TOL_EMITTED_MM = 1e-3 mm`), aimed at a
  `PATH_INDETERMINATE reason=fusion_uncertain` that an earlier build of
  `i5_closed_load_path` reported on the plain, exactly-radius-matched joint
  (two coaxial frusta sharing an EXACT cap-to-cap point and radius can give
  `overlap_gap_mm` a literal zero separation — the fusion-uncertain band's
  own lower boundary, not a true ambiguity). That fix cleared `i5` but
  introduced the SAME `MIN_SECTION_INDETERMINATE` /
  `GEOM_INDETERMINATE reason=path_indeterminate` pair on `i3`/`i8` that the
  empty-vs-real pad soup experiment also triggered — trading one collateral
  for two — so it was reverted in favour of the plain joint shipped here.
  `i5_closed_load_path` was implemented and re-implemented several times
  by another agent over the course of this build (this crate is being
  built by several agents in parallel, live, in the same checkout); on the
  checked-in file, re-verified directly against
  `plinth_audit::run::audit_file` immediately before submission, `i5`
  reports **`pass`** (0 indeterminate) — the plain, unmodified joint is
  what ships, and it currently clears `i5` cleanly on its own. The `i3`/`i8`
  `*_INDETERMINATE` findings above are unaffected by any of this and are
  the reproduced, final-run reality.

## Fixture-harness layout note

`tests/fixtures.rs`'s `fixtures_dir()` reads `*.expect.json` directly from
`tools/plinth-audit/fixtures/` (non-recursive `std::fs::read_dir`), one flat
pair per fixture. This fixture was assigned its own subdirectory,
`fixtures/starved_patch/`, for write isolation while several fixtures were
being built in the same checkout in parallel (matching the `broken_path`,
`double_load` and `bare_island` siblings, all of which record the identical
note). **As shipped, the harness will not discover
`fixtures/starved_patch/starved_patch.expect.json` in its current,
non-recursive form** — either the harness needs to walk one level of
subdirectories, or this pair needs to be promoted to
`fixtures/starved_patch.chitubox` / `fixtures/starved_patch.expect.json` at
integration time. Neither `tests/fixtures.rs` nor the top-level `fixtures/`
listing is owned by this directory, so that reconciliation is left to
whoever integrates the parallel fixture lanes.

## Reproducing

```sh
cd tools/plinth-audit
cargo test --release --test fixtures every_fixture_trips_the_check_it_names -- --nocapture
```

`generate.rs` (kept alongside this fixture, not part of the crate's own
`Cargo.toml`, not built by `cargo build`) is the exact, re-runnable record of
how the checked-in bytes were produced: build it as a throwaway crate with
`plinth-audit` as a `path` dependency outside this repository, then
`cargo run --release -- <out.chitubox>`.

sha256 of the checked-in `.chitubox`: `5518000960a2016e73df2867149ed682f712d327d4e6bed402928fbcd50002ed`
(13360 bytes).
