# `trivial_cube` — KG-1 `kg_slab_four_post`, the first known-good fixture

Status: **built, measured, 9 of 10 checks PASS.** This is the first fixture in
`tools/plinth-audit` that the audit is asked to *agree with* rather than to
*refuse*. All eleven pre-existing fixtures are deliberately broken files; this
one is deliberately correct, derived from `docs/physics.md` and the printer
profile, never from the checks' own source. The one check that still fails
(I8, dead-geometry) was **predicted to fail, in these terms, before this file
was built** — see `docs/specs/audit-known-good.md` section 5.1 — and the
measurement below confirms that prediction rather than contradicting it.

## For the owner

*A 4 mm square slab, 2 mm thick, held 4 mm off the plate on four posts — one
under the middle of each quarter of its underside. Each post therefore
carries a 2 mm × 2 mm patch of underside: 4 mm². The measured rule says an arm
must be at least one twelfth of what it carries, so 4 ÷ 12 = 0.333 mm² at its
thinnest point. The ball on top of each post is buried about 0.195 mm into
the slab, which cuts a circle roughly 0.78 mm across — about 0.48 mm², 1.44
times what is required — and the printer's own pixel grid is measured to eat
that down to about 0.406 mm², still about 1.22 times. The thinnest part of
the post above the ball is about 0.977 mm across, more than twice what it
needs. The furthest any point of the underside sits from a post is about
1.05 mm; the measured span allowance is 1.34 mm.*

*Nine of the ten checks agree this file is fine, including the tight one
(load bookkeeping, I4) once the four tip radii were chosen carefully rather
than all forced identical — the difference between the three "typical" posts
and the fourth is in the fifth decimal place of a millimetre, invisible to
the eye and to the printer. The tenth check (I8, "no dead geometry") refuses
every strut and ramp below the contact ball, on all four posts, because — as
best I can tell without reading that check's own code — a correctly-seated
tip is treated as already reaching ground on its own, so nothing downstream
of it ever gets credit for carrying load in the specific bookkeeping sense
that check looks for. If your own eye says the posts are obviously doing
their job, that disagreement is the finding, not the file. If ChituBox
refuses to open this file, or the model and supports don't show as separate
objects, that is also a finding — please tell me and I will investigate the
container splice below.*

## 1. Geometry

A 4.00 × 4.00 × 2.00 mm slab, bounding-box-centred (`x, y ∈ [−2, 2]`,
`z ∈ [−1, +1]`), floating 4.00 mm above the plate on four identical posts at
local `(x, y) = (±1.00, ±1.00)`. Each post is a TIP → TAPER → MEMBER → FLARE
chain, landing on the plate with no raft pad — a flare's own bottom disc
grounds directly (`audit-invariants.md` §3.5 clause (i)).

```
z_face  = -1.0          slab underside, local frame
z_plate = -5.0           4.00 mm below z_face (the design intent)
translate_z = 4.0        NOT 5.0 -- see "Build defect 1" below
```

## 2. Arithmetic, re-derived (not copied from `audit-known-good.md`)

The four posts sit at the perpendicular-bisector centres of the square, so
the Voronoi partition of the underside is exactly the four 2×2 mm quadrants —
exact by symmetry, no reach limit needed.

```
A_face  = 16.0000 mm²          D_total = 16.0000 mm² exactly (the sides are
A_cell  =  4.0000 mm²                    vertical, the top faces up; both
A_req   =  0.333333 mm²  = A_cell / 12    carry zero demand)
r_tip   =  0.325735 mm   = sqrt(A_cell / (12*pi))    (12*pi = 37.6991118431)
R = r_arm = 1.5 * r_tip = 0.488603 mm
h       =  0.60 * r_tip = 0.195441 mm     (h_required = 0.124420 mm -> 57% over)
a (cut disc, ideal) = 1.2 * r_tip = 0.390882 mm
A_ideal = 1.44 * A_req = 0.480000 mm²
arm section = pi * r_arm^2 = 0.750000 mm² = 2.25 * A_req  (never the binding section)
worst reach (Voronoi corner to tip axis) = 0.707107 * 2.0 - 0.195441*2 ... see
  audit-known-good.md section 1.4's derivation: s <= 2.628623 mm euclid /
  2.4139 mm chamfer for a square lattice at this tip size; this file's own
  pitch (2.00 mm) sits well under both.
```

`pi` is irrational, so no single rational `r_tip` can make
`sum(12*pi*r_i^2)` equal `D_total` exactly — see §4 below for how the four
posts' radii were actually chosen and why they are not bit-identical.

## 3. Build defects found and fixed (not tool findings — see §6 for what a
## tool finding actually is)

**1. `z_plate`.** `audit-known-good.md` §3.1's own prose gives
`translate_z = 5.0` for "4.00 mm above the plate", following
`audit-invariants.md` §1.2's *simplified* relation `z_plate = -translate_z`.
The actual reconstruction (`src/context.rs::resolve_plate`, independently
confirmed by `fixtures/buried_arm/generate.rs`'s own comment on the identical
formula) is `z_plate = mesh.bbox.min.z - translate_z`, which only coincides
with the simplified form when the mesh's bbox bottom sits at local `z = 0` —
true for none of these bounding-box-centred meshes. With `translate_z = 5.0`
this file measurably put the plate at `z = -6.0` (one whole millimetre below
the intended `-5.0`), stranding every flare foot 1.00 mm above the real
plate — I5 correctly fired `PATH_FREE_END` on all four. Fixed by deriving
`translate_z = z_face - z_plate = 4.0` directly from the two frame values.

**2. The taper→member joint.** `audit-known-good.md` §2.2's table specifies
pushing the member's start point 0.10 mm axially "into" the taper (rule R1).
Built literally, that push (0.10 mm out of the taper's own 0.50 mm length)
lands the member's start point *inside the ball's own volume* — a three-way
overlap among ball, taper and member at once. I5 tolerated it (a real, if
excessive, overlap), but I3's and I8's own path walk could not:
`MIN_SECTION_INDETERMINATE reason=path_open` on every contact and
`GEOM_INDETERMINATE reason=path_indeterminate` on 12 of 16 records. Reverting
to an **exact-coincidence join** (member.start = the taper's node point
exactly, radius-matched — the same pattern `fixtures/collinear_arms/generate.rs`
already uses successfully) resolved I3 and I8 outright, but exposed a
**second, narrower** defect: at the taper's originally-specified 0.50 mm
length, the ball and the member — not adjacent in the declared chain, and not
overlapping — measurably come within `0.026666 mm` of each other
(`PATH_INDETERMINATE reason=fusion_uncertain`, band `0.055290 mm`), because
this file's `r_ball` (0.4886 mm) is close enough to the 0.50 mm taper length
that the ball's own bulge nearly reaches the member's start disc around the
taper's narrow waist. Lengthening the taper from 0.50 mm to **0.60 mm**
(radii, depths and every other number unchanged) clears that separation past
the fusion-uncertain band. Neither fix changed an area, a radius, or a depth —
only how far the taper travels before the arm takes over.

**3. I4(d) sizing — a search-strategy defect, not an impossibility.** A first
attempt gave all four tips one shared radius, or (when that could not clear
the 1e-9 mm² floor) let a wide, unconstrained ULP sweep pick whichever
`(r_a, r_n)` pair minimised the *raw residual*; the winner undersized one tip
by 12% (`X = 13.46` against `X_ALLOWED = 12`) purely to chase the tolerance —
exactly what §1.3 forbids. Cross-checking against `fixtures/spread_wide/generate.rs`
(an independently-built copy of this same KG-1 design, by a different agent
in this round) showed the tolerance *is* reachable without that cost: prefer
the `(r_a, r_n)` pair **closest to the exact-symmetric ideal** among every
candidate whose residual already clears the floor, not the one with the
globally smallest residual. Re-implemented that way — see §4.

## 4. The four tip radii, and the disclosure §2.5 requires

Three posts (0, 1, 2) share `r_a`; post 3 carries `r_n`. Both are R6-exact
(`R = 1.5*r` loses no bits in f32). The pair is the one closest to the
symmetric ideal (`|step| = 1125` f32 ULPs from the ideal, a relative
deviation of about `1.0e-4`) among every pair whose summed I4(d) books clear
the `1e-9 mm²` floor:

```
r_a (posts 0,1,2) = 0.3257014751 mm   R = r_arm = 0.4885522127 mm
                     X_delivered = 12.002471   (X_ALLOWED = 12.0, +0.02%)
r_n (post 3)      = 0.3258355856 mm   R = r_arm = 0.4887533784 mm
                     X_delivered = 11.992593   (X_ALLOWED = 12.0, -0.06%)
```

`X_delivered` for `r_a` sits marginally *above* 12 — a physically negligible
0.02% (well inside any real-world manufacturing tolerance, and four orders of
magnitude below the file's own `1.44x` I2 margin) — which is the actual cost
of hitting I4(d)'s floor, not the 12% the first attempt found. This is
disclosed rather than hidden: the four posts are not exactly identical, and
that is a deliberate, measured, minimal trade, not an oversight.

## 5. Measured results

```
$ plinth-audit trivial_cube.chitubox
trivial_cube  model 0 "trivial_cube"   SUPPORT INVARIANTS FAIL  (9/10)
  I1 seat depth        pass     4 checked, 0 failed, 0 indeterminate
  I2 contact area      pass     4 checked, 0 failed, 0 indeterminate
  I3 min section       pass     4 checked, 0 failed, 0 indeterminate
  I4 load ownership    pass     1 checked, 0 failed, 0 indeterminate
  I5 closed load path  pass    16 checked, 0 failed, 0 indeterminate
  I6 no intersection   pass    16 checked, 0 failed, 0 indeterminate
  I7 island coverage   pass     1 checked, 0 failed, 0 indeterminate
  I8 no dead geometry  FAIL    16 checked, 12 failed, 0 indeterminate
  I9 unsupported span  pass     1 checked, 0 failed, 0 indeterminate
  I10 arrangement spread pass  40 checked, 0 failed, 0 indeterminate
```

### Predicted (§2) vs measured, per contact

| quantity | predicted | measured | agreement |
|---|---:|---:|---|
| `A_ideal` (I2) | 0.480000 mm² | 0.479901–0.480296 mm² | matches to 3 s.f. |
| `A_rast` (I2, cured) | ≥0.405650 mm² | 0.405650–0.406182 mm² | matches to 5 s.f. on 3 of 4 posts |
| `X_delivered` (I2) | ≤9.86 | 9.8478–9.8607 | matches |
| `h_delivered` (I1) | 0.195441 mm | 0.195421–0.195501 mm | matches to 4 s.f. |
| `h_required` (I1) | 0.124420 mm | 0.124407–0.124458 mm | matches |
| `A_min` (I3) | 2.25×A_req = 0.750 mm² (arm) | 0.481474–0.482846 mm² (binding at the tip, not the arm) | binding section is lower on the path than §2.1 step 5 named; see note below |
| worst reach (I9) | 82.9% (chamfer) / 76.1% (euclid) of 1.344976 mm | 1.051635 mm = 78.2% | between the two predictions, closer to euclid |
| I4 books ratio | exact by construction (target) | 1.0000000000166 | matches, residual 2.663e-10 mm² |

**I3 note:** §2.1 step 5 predicted the arm (`2.25×A_req`) would never be the
binding section, and it isn't — I3's own binding section (`A_min`) is
measured near `A_ideal` (the cut disc), consistent with the taper's narrow
end being the true narrowest point on the load path, not the constant-radius
arm. Not a disagreement with §2.1; a confirmation of exactly what it said
("the binding section is the cut disc at the seat").

**I9 note:** the measured worst reach (1.051635 mm) sits between the
euclidean and chamfer predictions from `audit-known-good.md` §1.4, closer to
euclidean. `audit-span-criterion.md` §2.4 rule R7 notes the chamfer metric
overestimates a diagonal path by up to 12%, which is consistent with the
measured value landing below the chamfer prediction.

### I4 — a minor disclosed oddity, not a blocker

The invariant's own top-level status is unambiguously `pass` (`checked: 1,
failed: 0, indeterminate: 0` — the field the harness's `check_passes`
assertion reads). Its `measured.clauses.d_books_bracket` sub-field, a more
granular diagnostic, reads `"indeterminate"` even though the residual
(2.663e-10 mm²) is comfortably under the 1e-9 mm² floor. This was **not**
investigated further (it would require reading `src/checks/load_ownership.rs`,
which this build deliberately did not open), and it does not affect the
authoritative pass — flagged here so it is not silently invisible.

### I8 — the one genuine, predicted finding: `GEOM_DEAD_MEMBER` on 12 of 16 records

```
GEOM_DEAD_MEMBER file=trivial_cube.chitubox model=0 record=1 type=taper
  volume_mm3=0.316667 roles_tested=R1,R2,R3,R4
```
(and the same code on every taper, member and flare — records 1,2,3, 5,6,7,
9,10,11, 13,14,15 — never on a tip.)

`audit-known-good.md` §5.1 predicted this **before this file was built**,
from `audit-invariants.md` §3.5/§3.8 alone: a correctly-seated TIP satisfies
ground clause (ii) on its own (its solid intersects the model, its seat zone
passes I6, its contact passes I1), so nothing downstream of it ever needs to
"carry" it to ground in the sense role R2 tests. Role R1 (carries demand)
only fires on TIPs. Role R3 (removing it increases some live member's free
span) needs bracing between chains, which a simple four-post, unbraced design
never has. Role R4 is raft-only, and this file has no rafts. So every TAPER,
MEMBER and FLARE is dead **by the roles as specified**, regardless of how
obviously load-bearing it looks. This matches the corpus-wide pattern already
recorded in `audit-invariants.md` §3.8 (`GEOM_DEAD_MEMBER` fires 2,638 times
over 24 real miniatures, and all 31 corpus files fail I8) and now, for the
first time, the same mechanism is confirmed on a file a human can look at and
confirm is obviously fine.

**Classification (per `audit-known-good.md` §8 step 3): (b) modelling gap.**
I8 measures a different notion of "load-bearing" than the physical one — it
asks whether removing a primitive changes something ELSE's connectivity to
ground, not whether the primitive itself is carrying load along an already-
closed path (which I5 already establishes, separately, and which this file
passes). The fix is a ruling on what "ground" means for role R2 (plate-and-
seat only, as specified, or propagated through any grounded neighbour) — not
a change to this file. **This is `audit-known-good.md`'s registered finding
KG-F2.**

## 6. What this file does and does not establish

Nine of ten checks pass non-vacuously (every one examined all 4 contacts or
all 16 records, `indeterminate: 0` throughout) on a file whose correctness a
human can confirm by the arithmetic in §2. That is one data point against the
"0 of 31" headline: **the tool can say yes**, and when it refuses this file
(I8) the refusal traces to a specific, predicted, named mechanism in the
specification — not to a check that examined nothing, and not to a build
defect (both of those were found, in §3, and fixed rather than reported as
findings).

This file does **not** establish that I8 is wrong to fail every real
miniature — only that it fails a structurally sound four-post slab for the
same stated reason it fails everything else, which is exactly the
measurement `audit-known-good.md` §5.1 asked for.

## 7. Harness discovery — verified

```
$ cargo test --release --test fixtures every_fixture_trips_the_check_it_names -- --nocapture
  ok       trivial_cube [i1_seat_depth] ...
  ok       trivial_cube_i2_contact_area [i2_contact_area] ...
  ok       trivial_cube_i3_min_section [i3_min_section] ...
  ok       trivial_cube_i4_load_ownership [i4_load_ownership] ...
  ok       trivial_cube_i5_closed_load_path [i5_closed_load_path] ...
  ok       trivial_cube_i6_no_intersection [i6_no_intersection] ...
  ok       trivial_cube_i7_island_coverage [i7_island_coverage] ...
  ok       trivial_cube_i8_no_dead_geometry [i8_no_dead_geometry] ...
  ok       trivial_cube_i9_unsupported_span [i9_unsupported_span] ...
  ok       trivial_cube_i10_arrangement_spread [i10_arrangement_spread] ...
```

`discovery_reaches_every_fixture_directory_on_disk` also passes: this
directory is reached by the real recursive discovery walk, not just by a
flat top-level glob (the exact defect `audit-known-good.md` warned this round
about — every fixture agent one directory deeper than the harness looked).

The crate's overall `cargo test` run shows one unrelated failure: a
hardcoded fixture-count backstop in `tests/fixtures.rs` ("Eleven expectations
are expected here...") that has not been updated for the fixtures multiple
agents added in this round (including this one). That assertion is not owned
by this fixture and was not touched, per this round's write-scope rules.

## 8. Layout

```
trivial_cube.chitubox                    canonical artifact (bound to i1_seat_depth)
trivial_cube.expect.json
trivial_cube_i2_contact_area.chitubox    symlink -> trivial_cube.chitubox
trivial_cube_i2_contact_area.expect.json
... (one symlink + expect.json pair per remaining check, i3..i10)
trivial_cube-model.stl                   model mesh, local/untranslated frame
trivial_cube-supports.stl                all 16 support records' soups, same frame
generate.rs                              standalone generator (see below)
README.md                                this file
```

Nine `.chitubox` files on disk are symlinks to one set of bytes (not nine
copies) because the harness binds one `<name>.expect.json` to exactly one
`<name>.chitubox` and tests exactly one check per pair
(`tests/fixtures.rs`'s own documented contract) — this file needed all ten
checks graded on the SAME bytes, so the artifact is shared and only the
expectation differs.

`sha256(trivial_cube.chitubox) = 83576be07bc35056187c4a42c3471d54a2398bb28ac43fa85ae5d2090430eace`,
58396 bytes.

## 9. Building it

```
cd tools/plinth-audit
RUSTC_WRAPPER="" cargo build --release
rustc --edition 2021 -O -L target/release/deps \
  --extern plinth_audit=target/release/libplinth_audit.rlib \
  fixtures/trivial_cube/generate.rs -o /tmp/gen_trivial_cube
/tmp/gen_trivial_cube fixtures/trivial_cube/trivial_cube.chitubox \
  ../../corpus/baseline/Sphere.chitubox
```

The second argument is a **settings-blob donor**: a real, populated
`.chitubox` file to copy the 396-byte settings blob from, so this file opens
in ChituBox rather than looking like the empty-settings container every
pre-existing fixture uses (`docs/specs/audit-known-good.md` §7.2).
`corpus/baseline/Sphere.chitubox` was chosen because its own `off8 = 420`
shows it carries a settings blob and **zero preview bytes**, so no preview
splice is needed — confirmed by `xxd -l24`. **Not independently confirmed
that the file opens in ChituBox itself** — this build environment has no way
to run that program. The `.stl` pair ships specifically so the owner's
inspection does not depend on the splice being right; please flag it if
ChituBox refuses the file, per `audit-known-good.md` §7.2 step 3.

## 10. Open questions inherited from `audit-known-good.md` §9, relevant to this file

- **U1** — is `β = 0.60` (ball buried 1.6× the ruled minimum, per §1.3)
  acceptable practice, or does it enlarge the scar more than it should?
- **U2** — posts landing on the plate through a bare conical flare, no raft
  pad. This file is the first place that convention is actually measured
  (§5 above): I5 accepts it cleanly once `translate_z` is correct. Acceptable
  for the files you inspect?
- **U5** — this file's 2.00 mm pitch is sparser than the corpus median
  spacing (1.03 mm). Sparse is what the physics permits here; is it what you
  would print?
