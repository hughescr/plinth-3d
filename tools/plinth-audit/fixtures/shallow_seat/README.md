# `shallow_seat` — a tip that never touches, and a tip that touches too little

## What is planted, and where

One model (`shallow_seat`), a closed box `[-1.3025,-1.3025,0] .. [1.3025,1.3025,4]`
mm, with two independent support chains, each `TIP(7) -> TAPER(1) -> MEMBER(3)
-> RAFT(6)` (record indices `0..3` and `4..7`). Both tip balls have radius
`R = 0.45 mm`; at `ball_factor_stub = 1.5` that is `r_tip = R/f = 0.3 mm` and
`h_required = ball_seat_factor(1.5) * r_tip = 0.3819660112501051 * 0.3 =
0.11458980337503153 mm` for both.

* **Record 0 (contact 0), chain A — `SEAT_DEPTH_ZERO`.** The tip's `start`
  point (`p0`, the ball's near pole — the point that is supposed to be pushed
  into the model) sits at `(-0.6, -0.6, -0.15)`: **0.15 mm clear of the
  model's flat bottom face at `z=0`, entirely outside the solid.** There is no
  gap-free "kissing" ambiguity here on purpose (see *Why a clear gap, not an
  exact-surface touch* below). Reconstructed: `h_delivered_mm = 0.0` exactly,
  `indeterminate = None`.
* **Record 4 (contact 4), chain B — `SEAT_DEPTH_SHALLOW`.** The tip's `p0`
  sits at `(0.6, 0.6, 0.05)`: 0.05 mm *inside* the box (which spans `z in
  [0,4]`). Reconstructed: `h_delivered_mm = 0.05000000074505806` (the `f32`
  round-trip of 0.05), against `h_required_mm = 0.11458980033943841` —
  **56.4 % short**, deficit `0.0645898 mm`. `abs_cos = 1.0` (the axis is
  vertical, perpendicular to the flat face), so the binding term is `ball`
  (`0.11459` beats both `default_contact_depth_mm=0.100` and
  `layer_height_mm=0.050`), not incidence.

Both chains' TAPER records declare a contact depth (trailing slot 0) **equal
to their own chain's planted `h_delivered`** (`0.0` and `0.05` respectively) —
deliberately, so `SEAT_DEPTH_DECLARED_MISMATCH` (reason `taper_trailing`)
does **not** also fire and blur which code is being exercised. Likewise both
chains' three tip-radius estimates (`r_tip_A` from the TIP record, `r_tip_B`
from the adjacent TAPER's `r_start`, `r_tip_C` fitted from the TIP's own
sphere soup) agree to `~2e-8 mm`, well inside `tol_geom`, so
`SEAT_DEPTH_DECLARED_MISMATCH` (reason `tip_radius_three_way`) does not fire
either. The fixture isolates the depth predicate itself.

`translate_mm = [0, 0, 5.023]` (not a multiple of `layer_height_mm = 0.05`)
so the seat-cut plane at `z=0` does not land exactly on a raster layer
boundary — see the note on `raster_exceeds_ideal` below.

### Why a clear gap, not an exact-surface touch

`Seat` reconstruction (`src/seat.rs`) branches on `mesh.point_inside(p0)`:
`Outside` gives `h_delivered = Some(0.0)` directly (no ray cast at all);
`Inside` casts a ray and measures the crossing; a point sitting *exactly on*
the mesh boundary is the one case liable to come back
`Inside::Indeterminate` (ray-parity ambiguity), which reports
`SEAT_DEPTH_INDETERMINATE`, not `SEAT_DEPTH_ZERO`. A 0.15 mm clear gap is
comfortably away from that boundary and lands deterministically in the
`Outside` branch, which is the only branch that ever produces the *hard*,
established `h_delivered = 0.0`.

## How this was verified (not just asserted)

I did not trust the byte layout by inspection alone. I wrote a throwaway
generator crate (`plinth-audit` as a path dependency, outside this repo, in
`$TMPDIR`) that:

1. builds the file with `plinth_audit::synth`;
2. **re-opens it and calls `seat::reconstruct_seats` directly**, printing
   every field of both `Seat`s before any `Check` ever sees them;
3. then runs the real `plinth_audit::run::audit_file` driver, the same
   entry point `tests/fixtures.rs`'s harness uses, and prints every
   invariant's status, codes and `measured` block.

Reconstructed seats (direct, bypassing any check):

```
contact_id=0 indeterminate=None h_delivered=Some(0.0)                h_required=Some(0.11458980033943841) binding=Ball
             r_tip_a=0.29999999205271405 r_tip_b=Some(0.30000001192092896) r_tip_c=Some(0.299999995569532) spread=1.99e-8 agrees=true
             declared=Some(0.0)                abs_cos=None
contact_id=4 indeterminate=None h_delivered=Some(0.05000000074505806) h_required=Some(0.11458980033943841) binding=Ball
             r_tip_a=0.29999999205271405 r_tip_b=Some(0.30000001192092896) r_tip_c=Some(0.2999999994702322) spread=1.99e-8 agrees=true
             declared=Some(0.05000000074505806) abs_cos=Some(1.0)
```

sha256 of the checked-in `.chitubox`: `f8dbed4212c40f2f6b492c242cad1f1697721a543d7fbf2512014654595d9f9f`
(27472 bytes).

## The audit's own output on this fixture — snapshot 2026-08-30

**This scaffold is being built by several agents in parallel, in this same
session, and every one of the eight `Check`s went from `not_implemented` to a
real implementation *while this fixture was being built* (verified: my very
first run showed `i1_seat_depth` as `AUDIT_CHECK_NOT_IMPLEMENTED`; a rebuild
minutes later showed it firing for real).** The following is what
`plinth-audit` reports on `shallow_seat.chitubox` **as of this snapshot**;
re-running `cargo test` later may show different collateral detail as those
other checks continue to be tuned, but `i1_seat_depth`'s two codes are the
fixture's actual target and are the ones the `.expect.json` grades.

```
i1_seat_depth:       fail   SEAT_DEPTH_ZERO, SEAT_DEPTH_SHALLOW      <- THE TARGET
i2_contact_area:     fail   CONTACT_PATCH_DISCONNECTED, CONTACT_AREA_SHORT
i3_min_section:      fail   MIN_SECTION_INDETERMINATE (x2)
i4_load_ownership:   fail   LOAD_OWNERSHIP_INDETERMINATE, LOAD_BOOKS_DISAGREE
i5_closed_load_path: pass
i6_no_intersection:  pass
i7_island_coverage:  pass
i8_no_dead_geometry: fail   GEOM_DEGENERATE (x4), GEOM_DEAD_MEMBER (x4)
```

`i1_seat_depth`'s own failure lines, exactly as emitted:

```
SEAT_DEPTH_ZERO    ... model=0 contact=0 record=0 h_delivered_mm=0 p0=-0.6,-0.6,-0.15000000596046448 u=0,0,-1
SEAT_DEPTH_SHALLOW ... model=0 contact=4 record=4 h_delivered_mm=0.05000000074505806 h_required_mm=0.11458980033943841
                       binding_term=ball r_tip_mm=0.29999999205271405 sin_theta=0 deficit_mm=0.06458979959438035
```

**Both target codes fire, both name the right measured numbers, and the
`.expect.json`'s `min_failures: 2` requires both to be present — the acceptance
test's first half.**

## The second half of the acceptance test, honestly

The instructions are explicit that "no other check fails on it" is not
optional, and equally explicit that if a fixture cannot be built without
collateral, that is a finding to report rather than paper over. I could not
build one without collateral, and I can show precisely why, for two distinct
reasons — one architectural and unavoidable, one a property of the shared
fixture-writing conventions this whole `fixtures/` corpus depends on.

### 1. `i2_contact_area` and `i4_load_ownership`: unavoidable for a genuine `SEAT_DEPTH_ZERO`

`Seat::entry_component` (`src/seat.rs`) is populated **only** on the
`Inside::Inside` branch of `mesh.point_inside(p0)` — never on `Outside`,
which is the *only* branch that ever produces the hard `h_delivered = 0.0`
(`SEAT_DEPTH_ZERO`). Both downstream checks read that same field to route a
contact to a surface component:

* `i2_contact_area` (`src/checks/contact_area.rs`): when `seat.entry_component`
  is `None`, `ctx.a_owned_mm2` cannot be looked up and the contact is graded
  `component_routing_failed`. On this fixture that surfaces as
  `CONTACT_PATCH_DISCONNECTED` for contact 0 (no material at `p0`, which is
  correct — there genuinely is none) and pushes the *entire* model's owned
  demand onto contact 4 alone (`CONTACT_AREA_SHORT`, `a_owned_mm2 =
  6.785840`, `a_required_mm2 = 0.565487`, far beyond what one `r_tip=0.3 mm`
  ball can ever deliver, `fails_even_at_ideal_disc: true`).
* `i4_load_ownership` (`src/checks/load_ownership.rs`), clause (c): reports
  `LOAD_OWNERSHIP_INDETERMINATE reason=component_routing_failed
  unrouted_contacts=1 first_contact=0` for exactly the same reason.

I verified this is not an artifact of my scene's *size* or *shape* by
building a **control**: a single, perfectly healthy tip (`h_delivered ==
h_required` exactly, everything else identical) sharing no model with any
zero-depth contact — `i2` and `i4` pass cleanly on it once the demand is
sized to the tip's own implied capacity. Then I put a genuinely zero-depth
tip *anywhere* in the same model and both checks fail again, every time,
regardless of geometry, because the failure is keyed on `entry_component`,
not on area or distance. **A `SEAT_DEPTH_ZERO` contact and a clean
`i2`/`i4` verdict cannot coexist in the same model under the current
implementation — proving that is itself the finding, not a defect in this
fixture.**

### 2. `i3_min_section` and `i8_no_dead_geometry`: properties of the shared `synth`/graph conventions, not of this fixture

I reproduced `tests/fixtures.rs`'s own `clean_artifact()` byte-for-byte — the
harness's **own designated "small, valid, well-formed file"**, used
elsewhere in that same file to prove the harness *rejects* a false claim —
and ran it through the identical audit driver. Snapshot, same session:

```
i1_seat_depth:       fail   SEAT_DEPTH_SHALLOW   (its own tip: h_delivered=0.100 < h_required=0.11459 mm for r_tip=0.3 — a pre-existing
                                                   coincidence in that fixture's own numbers, unrelated to anything I built)
i2_contact_area:     fail   CONTACT_AREA_SHORT
i3_min_section:      fail   MIN_SECTION_INDETERMINATE
i4_load_ownership:   fail   LOAD_BOOKS_DISAGREE
i5/i6/i7:            pass
i8_no_dead_geometry: fail   GEOM_DEGENERATE (x2), GEOM_DEAD_MEMBER (x2)
```

Same code families as this fixture's collateral, at half the count (one
chain instead of two). Concretely:

* **`GEOM_DEGENERATE reason=zero_area_triangle value=32`** on every TIP
  record. `plinth_audit::synth::sphere_soup(centre, r, 8, 16)` — the *only*
  sphere generator the crate ships, and the one `with_matching_soup` uses for
  every `ContactSphere`/`JointSphere` — collapses to a shared pole vertex at
  `la=0` and `la=lat`; after `f32` rounding at `r=0.45 mm` sixteen triangles
  at each pole round to zero area. `value=32 = 2 * lon(16)`. This fires on
  *any* fixture using `.with_matching_soup()` on a ball record, including the
  project's own reference.
* **`GEOM_DEGENERATE reason=empty_soup`** on every RAFT record.
  `SynthRecord::with_matching_soup` returns `Vec::new()` for
  `RecordKind::RaftPadOrBridge` by its own documented contract, and
  `clean_artifact()` doesn't call it on the pad at all either — every raft
  built the documented way carries no soup.
* **`GEOM_DEAD_MEMBER`** (roles R1–R4 all false) on every TAPER and MEMBER.
  Traced to the format's own documented convention (`docs/specs/chitubox-format.md`
  §5): "*a plate root's type-3 column ends at a type-4 boss top; the boss ends
  at `plate_z+0.2`, the top of the... raft*" — i.e. a MEMBER's `end` and its
  RAFT's point are legitimately **not** coincident (0.2 mm apart here, same
  as `clean_artifact()`), which is *more* than `SupportGraph`'s
  `JOIN_TOL_EMITTED_MM = 1e-3 mm`. `i5_closed_load_path` evidently bridges
  this gap some other way (it reports these same records `"grounded"` and
  passes cleanly), but `i8`'s R2/R3 role analysis walks the plain
  `SupportGraph` join relation, sees no edge from MEMBER to RAFT, and
  concludes removing either the TAPER or the MEMBER can't disconnect anything
  from ground — because the graph never connected it in the first place. That
  is a disagreement between `i5` and `i8`'s two notions of "grounded" worth
  someone's attention, not a property of the seat-depth defect.
* **`i3_min_section` `MIN_SECTION_INDETERMINATE`** on the reference file
  (reason not printed above, but on this fixture it is `path_open` for the
  routed contact — the same "MEMBER never joins its RAFT within tolerance"
  fact read by a third consumer) and `ownership_indeterminate` for the
  unrouted one (reason 1, restated).
* **`LOAD_BOOKS_DISAGREE`**, clause (d): `implied_lo/hi` collapse to a single
  point (`d_total` must equal it within `tol_sum = max(4096*eps64*d_total,
  1e-9)` — effectively `1e-9 mm²` absolute at this magnitude) but `d_total`
  is reconstructed from `f32`-stored mesh vertices, whose own quantisation at
  the ~mm scale used here is `~4e-7` — three orders of magnitude coarser than
  the tolerance demands. I sized this fixture's footprint to the two
  contacts' combined implied budget (`half = sqrt(6.785839772227211)/2`) and
  got within `2.46e-7 mm²` of it (`ratio = 1.0000000363`) — as close as a
  two-triangle rectangular footprint stored in `f32` can get without a
  deliberate multi-vertex cancellation trick. `clean_artifact()`'s own
  arbitrary box footprint misses by far more (`LOAD_BOOKS_DISAGREE` fires
  there too, unbounded by comparison). This looks like a tolerance the check
  inherited from `audit-invariants.md`'s pure-`f64`-summation error model
  without accounting for `f32` mesh storage — worth flagging to whoever owns
  `i4`, independent of this fixture.

**Conclusion:** every collateral code on this fixture reproduces on the
project's own "clean" reference file, or is a proven, code-cited, unavoidable
consequence of the one thing this fixture exists to plant
(`entry_component` being absent exactly when `h_delivered` is the hard
`0.0`). I did not weaken the fixture to dodge these, and I did not touch any
`src/checks/*.rs` file to make it agree with me — both against the brief.
If a future implementer changes how ownership routes an unrouted contact, or
gives `synth` a pole-free sphere generator, or bridges the raft-height gap in
`SupportGraph` itself, some of this collateral may disappear on its own; none
of it is this fixture's to fix.

## Reproducing

```sh
cd tools/plinth-audit
cargo test --release --test fixtures every_fixture_trips_the_check_it_names -- --nocapture
```
