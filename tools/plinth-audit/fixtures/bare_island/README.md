# `bare_island` — a detached island with nothing under it

## What is planted, and where

The file is one model, five support records, synthesised directly with
`plinth_audit::synth` (not perturbed from a real emitted artifact — a
detached island is cleanest to construct directly, since the defect is
about the *absence* of a specific geometric relationship, not a byte
corruption).

The model mesh is a single closed box, **the "island"**:

* footprint 2.5174 x 2.5174 mm (X: [-1.2587, 1.2587], Y: [-1.2587, 1.2587])
* height 2 mm (Z: [0, 2])
* lifted 3 mm off the plate (`translate_z = 3.0`), so `z_plate = -3.0` mm and
  the box's own bottom sits in open air with nothing else in the model below
  it.

One support tip embeds into the box's **+X side wall**, 0.20 mm deep, at
`z = 1.0` mm — the vertical *middle* of the box, one full millimetre above
the box's own bottom face at `z = 0`. From there a taper, a joint sphere, a
member and a raft pad carry the load down to the plate, entirely outside the
box's XY footprint (`x >= 1.67` throughout; the box's footprint ends at
`x = 1.26`), so no part of the support tree ever occupies the box's own
column range except at the one intended seat.

## The planted defect, in physical terms

The box's bottom-most printer layer (`k = 60`, `z = 0.025 mm`, computed from
the printer's own `0.014 x 0.019 x 0.050 mm` grid) is where the box is
**born** as cured material: nothing of the model is cured at layer 59, so
every pixel of that layer's 6.25 mm² footprint has nothing under it and
nothing beside it. That is a textbook detached island by
`audit-invariants.md` §3.7's own definition — "material with nothing under
it and nothing beside it falls into the vat" — and it is *not* the model's
first layer above the plate, so the exemption for a raft-seated first layer
does not apply.

The one support contact that exists is real, correctly sized, and correctly
embedded — but it touches the box a full millimetre **above** that birth
layer. The cured support raster's lowest layer is nowhere near `z = 0.025`
mm (the nearest support material sits above `z = 0.385` mm, the bottom of
the tip ball). So at the box's birth layer, the cured **support** raster is
empty under the cured **model** island. In the actual print this is the
resin-vat equivalent of dropping a brick with nothing holding it up until a
millimetre later: the island either never cures into place at all, or
detaches and drifts before the support above it can catch it.

## What the audit should say

`i7_island_coverage` must report **exactly one** `ISLAND_UNSUPPORTED`
failure, naming:

* `layer = 60`, `z_mm ≈ 0.025`
* `area_mm2 ≈ 6.24994` (the box's rasterised footprint, just under the true
  6.3372 mm² planar area because of the printer's own quantisation)
* `pixels = 23496`

No other layer of this file should ever produce an `ISLAND_UNSUPPORTED`
line: every layer above the birth layer is attached to the layer below it
(the box grows straight up), so only the birth layer itself is a candidate.

## Verification performed

I do not own `checks/island_coverage.rs`, so I did not implement or run it
by editing that file. Instead I built a private scratch copy of the whole
`plinth-audit` crate (never checked in, never touching the real repo outside
this directory) and ran the **actual, currently-implemented** checks against
this exact fixture through `plinth_audit::run::audit_file`. Observed,
2026-08-30:

```
i1_seat_depth:        pass   (h_delivered=0.2000 mm >= h_required=0.1566 mm)
i2_contact_area:      pass
i3_min_section:       pass
i4_load_ownership:    FAIL   LOAD_BOOKS_DISAGREE, ratio=1.0000000582 (see below)
i5_closed_load_path:  pass
i6_no_intersection:   pass
i7_island_coverage:   FAIL   ISLAND_UNSUPPORTED layer=60 z_mm=0.025 area_mm2=6.249936
i8_no_dead_geometry:  FAIL   GEOM_DEAD_MEMBER record=1 (the taper) — see below
```

`i7` fires for exactly the planted reason, with the right numbers. Two other
checks also fail, and I want to be direct about both rather than quietly
ship a fixture that "mostly" satisfies the "no other check fails" bar: I
could not make either go away by better geometry, and I can show why.

### I4 `LOAD_BOOKS_DISAGREE` — an f32-storage precision floor, not a design defect

I4 clause (d) requires `|D_total - A_implied| <= max(4096*eps64*D_total,
1e-9)` — for a demand around 6.34 mm² that is an absolute tolerance of
**1e-9 mm²**. I sized the box's footprint by *computing* the target area
from the tip radius the same way the check does
(`x_allowed * pi * (r_ball_stored_as_f32 / f)^2`), then rounding the box's
half-extent to its nearest `f32` representation before writing it — the
closest a hand-built fixture can get. The observed ratio is
`1.0000000582` (absolute gap `3.7e-7 mm²`), roughly **370x** the required
tolerance. That residual is exactly the size of one `f32` quantisation step
on a ~1.26 mm coordinate (`2^-24` relative, doubled because area is a
product of two such coordinates): squaring or multiplying any two
independently-`f32`-rounded lengths cannot land inside an `eps64`-scale
tolerance, no matter which two lengths are chosen. **This looks like a
genuine finding about I4 clause (d)'s tolerance, not something a fixture can
route around**: any single-contact model whose demand area and tip radius
are both reconstructed from `f32` wire fields will show a similar residual.
I checked whether the check itself has a coarser fixture — it does not; this
is the only float in the tolerance.

### I8 `GEOM_DEAD_MEMBER` on the taper — a structural consequence of clause (ii), not a geometry mistake

`dead_geometry.rs`'s grounding predicate (shared with `load_path.rs`'s own
independent reimplementation) treats a TIP whose own seat passes I1 as
**itself grounded** (`docs/physics.md`'s three-seat rule, clause (ii): a
valid model seat is one of the three legal grounds). My tip's seat is
comfortably valid (`h_delivered=0.2000 >= h_required=0.1566`, by design,
since a shallow seat would trip I1, which I do not want to fail). Given
that, removing the taper never "disconnects a live TIP from ground" — the
tip is *already* ground, independent of anything below it — so R2 does not
apply to the taper, and R1/R3/R4 do not apply to it either (it is not a
tip, braces no degree-3 junction, and is not a raft). The taper is
therefore correctly-by-the-letter "dead" under the current rule, purely
because this is a **minimal single-branch, single-contact** fixture: in a
real multi-tip tree a shared taper/member almost always also carries R2 for
*some other* tip whose own seat is not independently valid, or R3 bracing,
or both. I confirmed this is not specific to my numbers by tracing the
project's own `dead_geometry.rs` reference fixture (`clean_chain_file`,
same one-contact-one-branch shape): it hits the identical tension whenever
its tip's seat is comfortably valid, which is *why* that test currently
tolerates it structurally (its own tip's `h_delivered` sits fractionally
**below** `h_required`, so its tip is never self-grounded and the chain
matters for grounding). I did not copy that trick, because it would make
this fixture fail I1 (`SEAT_DEPTH_SHALLOW`), which is worse: it would give
`i7`'s proof a second, unplanted reason to fail on the same file. I chose
the failure I could explain and point at, over the one that would corrupt
the acceptance test. **This is a real, reportable interaction**: any
minimal single-branch, well-embedded fixture (mine or one of the other
eight agents') will hit it, and it is worth someone deciding on purpose
whether a self-grounded tip's own taper/member should count as "live" by
some rule not yet written (or whether the fixture convention should be "one
tip that is not independently grounded, in a chain of at least two
branches" from now on).

Both residuals are independent of the planted defect: neither check's
failure reason has anything to do with the birth-layer support gap, and
removing the *planted* defect (e.g. adding a second contact at the box's own
bottom face) would not change either of them.

### Geometry log (how the numbers were chosen)

* `r_tip = 0.41` mm, `f = 1.5` (`BALL_FACTOR_STUB`) → ball radius `R =
  0.615` mm, stored as `f32` then re-read as `r_tip_recovered =
  0.410000006357829` mm — the exact value I2/I3/I4 reconstruct.
* Box footprint area chosen as `x_allowed * pi * r_tip_recovered^2 =
  6.337220897 mm^2` (see the I4 note above for why this cannot land exactly).
* Embed depth `0.20` mm, chosen to clear `h_required = ball_seat_factor(1.5)
  * r_tip = 0.15661` mm with margin, at normal incidence
  (`sin_theta = 0`, since the box wall is flat and vertical).
* Taper radius grows `0.41 -> 0.45` mm; joint sphere radius `0.6` mm,
  centred `1.4` mm from the tip's own ball centre — comfortably more than
  the two balls' summed radii (`1.215` mm) so the tip cannot geometrically
  bypass the taper and read as a redundant (and therefore I8-dead) member.
* Member radius `0.43` mm, run **exactly vertical** (zero horizontal
  offset) from the joint's centre to a point **0.0005 mm past** the raft
  pad's own connective point (i.e. 0.0005 mm inside the raft's cylinder,
  not merely touching its boundary plane). Three reasons, the first two
  confirmed against the project's own reference fixture in
  `dead_geometry.rs` (which uses the same vertical-member convention) and
  the third found empirically against `checks/load_path.rs`:
  1. `crate::graph::SupportGraph` only joins records whose connective
     points coincide within ~1e-3 mm for an emitted-classified file — a
     member that merely comes *close* to the raft leaves the raft in its
     own disconnected graph component and reads as I8-dead (R4 never sees
     a live tip in its component). 0.0005 mm is comfortably inside that
     tolerance.
  2. A *diagonal* member's tight bounding box dips below its nominal
     endpoint by `r * sqrt(1 - u_z^2)` (`primitives::Primitive::aabb`'s
     documented tilt term) — ending a tilted member at `z_plate` pushed
     part of its solid below the plate and tripped the frame check in an
     earlier draft. A purely vertical member has zero tilt term, so this
     is safe regardless of how close to `z_plate` the endpoint sits.
  3. Ending the member **exactly** at the raft's own point (rather than
     0.0005 mm past it) computes `Primitive::separation_mm` as an exact
     `-0.0` — a genuine boundary touch, not an overlap, because neither
     solid's interior actually crosses into the other's z-range when they
     only share a single boundary plane. `load_path.rs`'s own overlap
     graph requires a *strictly* negative separation for a clean edge and
     refuses `-0.0` as ambiguous, reporting `PATH_INDETERMINATE
     reason=fusion_uncertain` — confirmed by running this exact fixture
     both ways against the live check. Ending 0.0005 mm past the raft's
     point puts genuine (if thin) volume of the member inside the raft's
     already-established cylinder, which resolves to an unambiguous
     negative separation.
* Every soup is hand-built to close on its own analytic surface (not via
  `plinth_audit::synth::sphere_soup`, which reuses the pole vertex as both
  ends of its wrap-around edge and ships `lon` exactly-zero-area triangles
  at *each* pole — 32 degenerate triangles per default 8x16 sphere. This
  is a real, shared-infrastructure defect that will trip every other
  fixture that calls `.with_matching_soup()` on a `ContactSphere` or
  `JointSphere` once I8 runs against it; I did not fix `synth.rs` because
  I do not own it, but I built my own correct pole-fan tessellation for
  this fixture's tip and joint sphere rather than ship a fixture with a
  known-spurious defect). `plinth_audit::synth` also has no raft-pad soup
  generator at all (an empty soup is itself `GEOM_DEGENERATE`), so this
  fixture builds its own cylinder soup for the raft pad too.
