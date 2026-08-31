# `buried_arm` — an arm passing through the model outside any seat, and a tip protruding out the far side of a thin wall

## Finding first, because it affects how this fixture gets exercised

**`tests/fixtures.rs`'s `fixtures_dir()`/`load_expectations()` scans only the
top level of `fixtures/` with a plain `std::fs::read_dir`, not recursively.**
Every existing fixture (`bare_island/`, `broken_path/`, `double_load/`,
`shallow_seat/`, `starved_patch/`, and now this one) lives one directory
down, as `fixtures/<name>/<name>.expect.json` + `.chitubox`. A run of
`cargo test --release --test fixtures -- --nocapture` on the checkout as it
stands prints:

```
no fixtures in .../tools/plinth-audit/fixtures yet. Each implementing agent
adds <name>.chitubox + <name>.expect.json there; see this file's header for
the format.
```
`test every_fixture_trips_the_check_it_names ... ok` (vacuously — zero
expectations loaded, 7 tests total, all passing) confirms none of the
existing per-directory fixtures are currently reachable by the harness. This
is a property of `tests/fixtures.rs`, a file this fixture does not own and
was told not to edit, so it is reported here rather than fixed. Every claim
below was instead verified by loading `plinth-audit` as a path dependency
from a throwaway crate outside this repository and calling
`plinth_audit::run::audit_file` directly — the same entry point the harness
itself calls — exactly as `fixtures/shallow_seat/README.md` already
documents doing for the same reason.

## What is planted, and where

One model (`buried_arm`), a single thin shared wall,
`x ∈ [-4,4], y ∈ [-2,2], z ∈ [-0.1,0.1]` (0.2 mm thick), `translate_mm =
[0,0,5.0]`, so `z_plate = mesh.bbox.min.z − translate_z = -0.1 - 5.0 =
-5.1 mm`. Two independent support chains sit on it, 4 mm apart in X, each
built by `plinth_audit::synth` (`generate.rs` in this directory is the exact
generator used, byte-verified to reproduce the checked-in file — see
"Reproducing" below).

### Chain A (records 0–4, `x = -2`) — plants `INTERSECT_FAR_SIDE_BREACH`

A TIP ball (`r_ball = 0.3 mm`, `r_tip = 0.2 mm`) whose `p0` (the ball's own
embedded pole) sits at `z = 0.2` — **0.1 mm past the wall's FAR face**
(`z = 0.1`), not the near one. This is not a new construction: it is
`checks/intersection.rs`'s own bundled unit test
`a_tip_driven_clean_through_a_thin_wall_fires_the_far_side_clause`
(`punched_file(wall_half=0.1, p0_z=0.2)`), reproduced number-for-number and
translated sideways to `x = -2` so it can share a model with chain B. Taper
(`r_tip=0.2 → 0.35`), member (`r=0.35`, length 3 mm), root-flare
(`0.35 → 1.1`) and a raft pad complete the chain down to the plate at
`z = -5.1`.

**Why `p0` past the far face is required, not incidental — a proof, not just
a probe.** The ball's own embedded pole `p0` is, by construction, the single
most-extreme point of the whole sphere along the seat axis in the embedding
direction: `p0 = centre - R·u`, and no point of a sphere lies farther from
its centre than `R` in any direction. So **nothing of the ball can ever
extend past `p0` along the axis** — if `p0` itself does not reach beyond a
wall's far face, no part of the ball does either, and there is no breach to
find. A genuine far-side punch-through therefore requires `p0` to sit beyond
the far face, which places `p0` outside the model overall (`mesh.point_inside
== Outside`), which is exactly `SEAT_DEPTH_ZERO`. **A real
`INTERSECT_FAR_SIDE_BREACH` and a clean `i1_seat_depth` cannot coexist on the
same contact under the current geometry.** This is the same *shape* of
finding `fixtures/shallow_seat/README.md` already proved for its own
zero-depth contact (that a real `SEAT_DEPTH_ZERO` and clean `i2`/`i4` verdicts
cannot coexist); the "collateral" section below traces this fixture's own
`i1`/`i2`/`i3`/`i4` lines to precisely that same mechanism.

### Chain B (records 5–11, `x = 2` to `x = 10`) — plants `INTERSECT_OUTSIDE_SEAT`

A TIP ball (`r_ball = 0.45 mm`, `r_tip = 0.3 mm`) seated **healthily**: `p0 =
(2, 0, 0.05)`, safely inside the wall, giving `h_delivered = 0.15 mm >
h_required = ball_seat_factor(1.5) × 0.3 = 0.11458980033943841 mm`. This
contact reconstructs with `indeterminate = None` and is never named in any
`i1_seat_depth` failure — I1 passes it cleanly. The taper exits normally,
0.9 mm below the wall.

Then the **arm** (`record 7`, a `Member`/JOINER, `r = 0.35 mm` throughout)
turns back and drives straight through the same wall a **second** time:
`start = (2, 0, -1.3)` [the taper's exit, already well clear below the wall]
`end = (3.0, 0, 0.6)` — a straight diagonal whose centreline crosses
`z ∈ [-0.1, 0.1]` at `x ∈ [2.63, 2.74]`, **1.6 mm from the seat's own axis**
at `x = 2`. From there the chain escapes sideways at `z = 0.6` (comfortably
more than the tube's own 0.35 mm radius above the wall's top face at 0.1, so
the escape leg's own *thickness* clears the slab, not merely its centreline
— an earlier draft of this fixture got this wrong: with the escape leg at
`z = 0.2` its 0.35 mm-radius tube still dipped down to `z = -0.15` and
grazed the wall along its whole length, planting a second, unintended
buried patch on a different record. Raising the escape leg to `z = 0.6`
removed it — verified below, only one arm record shows buried volume) to
`x = 10`, far outside the wall's `x ∈ [-4, 4]` footprint, then descends
straight down to a raft on the plate.

Nothing about chain B's own seat is defective. The defect is entirely in the
**arm downstream of a healthy seat** — literally "an arm passing through the
model outside any seat."

## Why this is a defect in physical terms

Support and model cure in the same exposure; touching costs nothing, but
support material **inside** the model where it was never meant to be is
either wasted resin walled off from the vat (harmless but silently
misleading about how much of the pour is support) or, worse, a sign the
support's true path never reached open air at all — exactly what chain B's
"passes through the wall itself" and chain A's "drove clean through
instead of resting on the surface" would both mean on a real print: a
raft-and-arm system generated for the wrong side of a thin feature, curing
half-buried inside geometry it should never have touched.

## How this was verified

Following `fixtures/shallow_seat/README.md`'s method exactly: a throwaway
crate outside this repository, `plinth-audit` as a path dependency, that (1)
builds the file with the generator in this directory, (2) re-opens it and
calls `seat::reconstruct_seats`/inspects `ModelContext` directly, printing
every field before any `Check` runs, then (3) runs the real
`plinth_audit::run::audit_file` — the same driver `tests/fixtures.rs` calls —
and prints every invariant's status and failure lines.

Reconstructed seats (direct, bypassing any `Check`):

```
contact_id=0 indeterminate=None h_delivered=Some(0.0)                h_required=Some(0.1)                binding=Some(Default)
contact_id=5 indeterminate=None h_delivered=Some(0.15000000223517418) h_required=Some(0.11458980033943841) binding=Some(Ball)
```

`mesh closed: true`. `file_level_failures: []`. `print_candidate: false`.

sha256 of the checked-in `.chitubox`:
`fcf9e13f37bf0de671a85f49cdd4efdc76b5a375fdbde72067379f7acadb0d90`
(40432 bytes) — reproduced byte-for-byte by re-running `generate.rs` in a
clean throwaway crate, hashes compared and confirmed identical.

## The audit's own output on this fixture — snapshot 2026-08-30

```
i1_seat_depth:       fail   SEAT_DEPTH_ZERO
i2_contact_area:     fail   CONTACT_PATCH_DISCONNECTED, CONTACT_AREA_INDETERMINATE
i3_min_section:      fail   MIN_SECTION_INDETERMINATE (x2)
i4_load_ownership:   fail   LOAD_OWNERSHIP_INDETERMINATE, LOAD_BOOKS_DISAGREE
i5_closed_load_path: pass
i6_no_intersection:  fail   INTERSECT_OUTSIDE_SEAT (x2), INTERSECT_FAR_SIDE_BREACH   <- THE TARGET
i7_island_coverage:  pass
i8_no_dead_geometry: fail   GEOM_DEAD_MEMBER (x6), GEOM_INDETERMINATE (x5)
```

`i6_no_intersection`'s own failure lines, exactly as emitted:

```
INTERSECT_OUTSIDE_SEAT    model=0 record=0 type=7 volume_mm3=0.044768 censor_mm3=1.33e-5
                          centroid=-2.000274,-8.246e-20,-0.008363 nearest_contact=0 distance_mm=0.208363
INTERSECT_OUTSIDE_SEAT    model=0 record=7 type=3 volume_mm3=0.081024 censor_mm3=1.33e-5
                          centroid=2.68931,-1.677e-18,0.001001 nearest_contact=5 distance_mm=0.691049
INTERSECT_FAR_SIDE_BREACH model=0 contact=0 record=0 exterior_components=2
                          breach_volume_mm3=0.007448 breach_centroid=-1.99905,-2.131e-18,0.137679
```

Read together, these three lines cover both halves of the brief and one
bonus corollary, all correctly attributed by record:

* **`record=7` (`type=3`, the JOINER/Member) is chain B's arm** — 0.081 mm³
  of it sits inside the model, 0.691 mm from the nearest contact (chain B's
  own tip, `nearest_contact=5`), which is exactly "an arm passing through the
  model outside any seat," attributed to the correct record by nearest-solid
  routing rather than lumped onto the tip.
* **`contact=0`/`record=0` (`INTERSECT_FAR_SIDE_BREACH`) is chain A's tip
  driven through the wall** — `exterior_components=2`, the punched-wall
  signature, `breach_centroid` at `z=0.138`, above the wall's far face
  (0.1), i.e. the second blob really is on the far side, not a rasterisation
  artefact at the near one.
* **`record=0` also appears under `INTERSECT_OUTSIDE_SEAT`** — a correctly
  reasoned corollary, not a duplicate or a confusion. Chain A's zone band is
  `t_max = h_delivered + layer_height = 0.0 + 0.05 = 0.05 mm` deep from
  `p0`, so only the outermost 0.05 mm sliver of the 0.3 mm-radius ball is
  ever inside its own zone; the other ~0.83 of the ball's volume that lands
  inside the wall (0.045 mm³) is honestly "outside any seat" by the same
  volume test clause (a) applies everywhere else. A tip driven all the way
  through with no real embedding is, correctly, unaccounted for by its own
  zone almost everywhere.

`min_failures: 3` in the `.expect.json` requires all three lines, which is
deliberate: dropping to 2 would no longer distinguish "clause (a) fired
twice" from "clause (a) fired once and clause (b) never fired at all," and
proving *both* clauses independently is the entire point of this fixture.

## The second half of the acceptance test, honestly

Every other check that fires here traces to one of two mechanisms already
proven, independently, by other fixtures in this corpus — nothing here is a
new failure mode this fixture introduces, and none of it was papered over by
weakening the plant or touching a `src/checks/*.rs` file.

### 1. `i1`/`i2`/`i3`/`i4`: the `SEAT_DEPTH_ZERO` → `entry_component = None` cascade

Identical mechanism to `fixtures/shallow_seat/README.md` §"1. `i2_contact_area`
and `i4_load_ownership`: unavoidable for a genuine `SEAT_DEPTH_ZERO`." Chain
A's `Outside` branch never populates `Seat::entry_component` (only the
`Inside` branch does), so:

* `i1_seat_depth`: `SEAT_DEPTH_ZERO` on contact 0 — the direct, intended
  reconstruction, proven above to be unavoidable for a genuine far-side
  breach.
* `i2_contact_area`: `CONTACT_PATCH_DISCONNECTED` for contact 0 (no routed
  component — correct, there genuinely is none) and `CONTACT_AREA_INDETERMINATE
  reason=raster_exceeds_ideal` for contact 5 — the entire model's demand
  (`d_total_mm2 = 32`, the whole 8×4 mm wall's one down-facing face) piles
  onto the one remaining routed contact, well past what its own `r_tip=0.3`
  ball could ever ideally deliver.
* `i3_min_section`: `MIN_SECTION_INDETERMINATE reason=ownership_indeterminate`
  for contact 0 (same routing failure, a third consumer) and
  `reason=path_open` for contact 5 — see mechanism 2 below, unrelated to the
  zero-depth contact.
* `i4_load_ownership`: `LOAD_OWNERSHIP_INDETERMINATE
  reason=component_routing_failed unrouted_contacts=1 first_contact=0` (the
  same fact again) and `LOAD_BOOKS_DISAGREE ratio=6.529` — with the entire
  patch's demand forced onto one contact, the declared/reconstructed area
  books disagree by construction, same as `shallow_seat`'s own
  `ratio`-style disagreement.

`i5_closed_load_path` and `i7_island_coverage` **pass cleanly** — the two
checks most likely to clash with a two-chain, cross-shaped support layout
are unaffected. I5 evidently does not depend on `entry_component`, and I7's
presence test is satisfied everywhere: the buried arm and the punched tip
are themselves cured support material sitting exactly where the model's own
birth layers need *something* present, so no detached-island line fires.

### 2. `i8_no_dead_geometry`: the raft-pad-height convention

Identical mechanism to `fixtures/shallow_seat/README.md` §"2.
`i3_min_section` and `i8_no_dead_geometry`." Every taper/member/flare in
both chains ends `RAFT_PAD_HEIGHT_MM = 0.2 mm` above its own raft's point
(`flare_end.z = -4.9`, `pad.z = -5.1` in both chains) — the documented
`chitubox-format.md` §5 convention that a plate root's column ends at a boss
top, not at the raft point itself, which sits `0.2 mm` beyond
`SupportGraph`'s own `JOIN_TOL_EMITTED_MM = 1e-3 mm`. `i8`'s R2/R3 role walk
sees no graph edge from the last arm segment to its raft and reports
`GEOM_DEAD_MEMBER` on every taper/member in both chains and
`GEOM_INDETERMINATE reason=path_indeterminate` on every flare/raft — the same
code family, at roughly double `shallow_seat`'s count, simply because this
fixture has two full chains (and chain B has three member segments instead
of one) where `shallow_seat` had two chains of one member each.

**Conclusion:** every collateral line reproduces a mechanism already proven
pre-existing and unavoidable by `fixtures/shallow_seat`, not something this
fixture's own geometry introduces. `i5` and `i7` — the two checks a
multi-branch support layout would most plausibly confuse — stay clean. No
`src/checks/*.rs` file was edited to reach this result, and the plant was not
weakened to dodge it.

## Reproducing

```sh
# tests/fixtures.rs does not currently reach this fixture (see the finding
# at the top of this file) -- verify directly instead:
mkdir -p /tmp/buried_arm_check/src && cd /tmp/buried_arm_check
cat > Cargo.toml <<'EOF'
[workspace]
[package]
name = "buried_arm_check"
version = "0.1.0"
edition = "2021"
[dependencies]
plinth-audit = { path = "/absolute/path/to/plinth-3d/tools/plinth-audit" }
EOF
cp /absolute/path/to/plinth-3d/tools/plinth-audit/fixtures/buried_arm/generate.rs src/main.rs
cargo run --release -- buried_arm.chitubox
shasum -a 256 buried_arm.chitubox   # compare against this README's hash
```
