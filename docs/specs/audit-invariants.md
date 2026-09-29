# Audit invariants — the emitted-bytes print-candidate contract

Status: **NORMATIVE for `tools/plinth-audit/`.** This file is the specification every
audit implementation agent codes against. It defines eight gating invariants, one
non-gating report, and one output schema. Where this file and any engine source
disagree, this file governs the audit — the audit is not permitted to agree with the
engine by construction.

## 0. Why this exists, and the rules that override convenience

The engine's own records are known to lie. Every support record declares a seat depth
of exactly 0.300 mm and the geometry delivers it on 11.6 % of 189 real-mini tips; a
clearance predicate returns a value meaning "measured nothing" and three production
sites read it as a pass; the project's regression reference file passes while 7 of its
7 tips sit unembedded. An audit that consumes the engine's disclosures inherits every
one of those defects.

So the audit reads **only the emitted bytes** of a `.chitubox` file. It is a standalone
Rust crate at `tools/plinth-audit/` with its own `Cargo.toml`, deliberately **not** a
workspace member, with **zero** dependency on any `plinth-*` crate or on `geom_kernel`.
It reimplements every geometric primitive it needs. That independence is the whole
value of the instrument. Wanting to import an engine type is the signal to reimplement.

Four rules bind every check below, and no implementation convenience overrides them.

1. **Reconstruct where geometry allows it.** A fact that can be recovered from the
   emitted triangles or the emitted record coordinates is `RECONSTRUCTED`. A fact that
   can only be read from a stored field is `DECLARED`. Every value in the output is
   tagged with which it is.
2. **A disagreement between declared and reconstructed is a hard failure.** Wherever
   both exist, both are computed and both are reported, and the check fails on
   disagreement beyond the stated tolerance. It never prefers one silently.
3. **"Not measured", "not implemented" and "indeterminate" never map to pass.** Any
   item a check cannot establish increments that invariant's `indeterminate` count and
   forces its status to `fail`. There is no third status. There is no skip.
4. **Every invariant is a number, not a judgement.** Each one below states a predicate
   over measured quantities, a tolerance with its derivation, and the exact failure
   line it emits.

The audit changes no emitted bytes and no engine behaviour. It is read-only with
respect to `crates/` and `rust/`.

---

## 1. Inputs

### 1.1 What the audit reads from the file

Parse by the pointer chain only, never by scanning for magics. Offsets are pinned by
[`chitubox-format.md`](chitubox-format.md) and by `rust/chitubox_io/src/consts.rs`;
the audit copies the *numbers* from those documents and reimplements the reader.

```
header[0]   u32  FILE_MAGIC     = 0xAB231243
header[4]   u32  model_count
header[8]   u32  off8           model-table offset
header[20]  u32  off20          = off8 + 680*model_count
model[256]  3xf32 translate     (X, Y, Z) mm, plate placement
model[268]  u32  support_index_ptr   0 = unsupported
model[272]  u32  mesh_off
model[276]  u32  mesh_size      = 36 * n_triangles
index       {u32 n_records, u32 records_off, u32 mesh_off, u32 mesh_size}   16 B
record      72 B, magic 0xEA342389 at +0, type u32 at +4,
            start 3xf32 at +8, end 3xf32 at +20, r1 f32 at +32, r2 f32 at +36,
            mesh_off u32 at +40, mesh_len u32 at +44, trailing 24 B at +48
```

Record type to role, from `crates/plinth-emit/src/lib.rs` `segment_type`. **The
format's own names are misleading; use these roles and no others:**

| wire type | role in this document |
|---:|---|
| 7 `TIP` | contact sphere — the emitted tip ball |
| 1 `TAPER` | contact taper — the frustum immediately above the tip ball |
| 3 `JOINER` | member — the structural arm |
| 9 `SHAFT` | joint sphere — a junction ball |
| 4 `FLARE` | root flare — the boss above the raft |
| 6 `PAD_OR_BRIDGE` | raft pad or raft bridge, distinguished by trailing float 0 (`1.0` = bridge) |

Any record whose type is not one of these six is `FILE_RECORD_SEMANTICS_UNKNOWN` and
fails the file. An unknown type is not skippable: the audit cannot establish anything
about a solid it cannot classify.

### 1.2 The one coordinate frame

Both the model mesh blob and every support record coordinate are stored in the
**model's bounding-box-centred frame** (`crates/plinth-emit/src/lib.rs`
`record_frame_and_soup` subtracts the same `center` from support points that the model
mesh is centred on). The audit therefore compares model triangles and support geometry
directly, with no transform, and must not apply the model translate to one side only.

The plate is world `z = 0`, so in the centred frame it is at

```
z_plate = -translate_z            (RECONSTRUCTED from model record off264)
```

Cross-check: the minimum `z` over all type-6 raft geometry must equal `z_plate` within
`tol_geom` (§1.5). Disagreement is `FILE_FRAME_MISMATCH` and fails the file. This is
the one check that catches a mis-parsed lift, and it must run before anything else.

### 1.3 Declared inputs the file does not carry

The settings blob is unmapped and all-zero in this corpus, so the print process
parameters are not in the file. They are supplied to the audit as declared inputs,
recorded verbatim in the output with their provenance, and **an absent one fails the
file with `FILE_DECLARED_INPUT_MISSING`** — the audit never substitutes a guess.

| name | value | units | provenance | used by |
|---|---:|---|---|---|
| `pixel_pitch_x_mm` | 0.014 | mm | Measured, `PIXEL_PITCH_X_UM` | I2, I5, I6, I7, I8, report |
| `pixel_pitch_y_mm` | 0.019 | mm | Measured, `PIXEL_PITCH_Y_UM` | same |
| `layer_height_mm` | 0.050 | mm | Measured, `LAYER_HEIGHT_DEFAULT_MM` | I1, I2, I7 |
| `default_contact_depth_mm` | 0.100 | mm | **Owner ruling 2026-08-30** | I1 |
| `ball_factor_stub` | 1.5 | — | Placeholder, `BALL_FACTOR_STUB` | I1, I2 |
| `x_allowed` | 12 | — | Derived, `X_ALLOWED` = floor(25.00/2) | I2, I3, I4 |
| `tip_radius_min_allowed_mm` | 0.050 | mm radius | Derived, `TIP_RADIUS_MIN_ALLOWED_MM` | I4 |
| `tip_radius_max_mm` | 1.000 | mm radius | Placeholder, `TIP_RADIUS_MAX_LEGACY_MM` | I4 |

`default_contact_depth_mm` is **0.100 mm**, not the 0.300 mm the ledger still records.
The owner ruled on 2026-08-30 that burial depth is not an adhesion requirement —
support and model cure in the same exposure, so touching and overlapping cure
identically and there is no bond to develop between two cured solids. The only real
failure is a rasterisation gap, bounded by one pixel and one layer. The ruling is
bound to a condition that this document enforces in I2: **contact area still governs,
and depth never overrides it.** A shallower seat is a *retracted* ball, not a trimmed
one, so its cut disc shrinks as `a = sqrt(2*f*r*h - h*h)`; where the delivered patch
would fall short of the area law the tip must grow or the seat must stay deep, and I2
is the check that says so.

The audit must reject a `default_contact_depth_mm` above 0.300 mm or below
`layer_height_mm` as `FILE_DECLARED_INPUT_MISSING` with reason `out_of_ruled_range`:
below one layer the seat is not resolvable, above the retired value the caller is
loosening a ruling rather than configuring an instrument.

### 1.4 Reconstructed solids

Two solids, both built from the emitted triangles alone.

**Model solid `M`.** The model mesh blob, `mesh_size/36` triangles, 9 `f32` per
triangle, no normals, no index. Weld vertices by **exact `f32` bit identity** — the
emitter wrote `STL - bbox_center` in `f32`, so identical corners are identical bit
patterns and welding is exact, never a distance merge. Then require closure: every
undirected edge carries exactly two faces with opposite directed incidence. A model
that is not closed fails the file with `FILE_MESH_NOT_CLOSED` — inside/outside is
undefined on an open soup, and the never-pass rule forbids proceeding on a guess.
Facet normals come from the winding, `n = normalize((v1-v0) x (v2-v0))`; the outward
sense is fixed once per shell by the signed tetrahedron volume sum and applied to every
facet of that shell.

**Support solid `S`.** The union of analytic primitives reconstructed from the records,
not from the support triangle meshes:

- type 7 and type 9 — a ball of radius `r1` centred at `midpoint(start, end)`;
- types 1, 3, 4 — a right circular frustum from `start` radius `r1` to `end` radius `r2`;
- type 6 — the raft pad or bridge solid from the pinned 2.2 mm diameter / 0.2 mm height
  pad and 12-triangle bridge contract.

The analytic form is used because it is exact and cheap. It is then **verified against
the emitted mesh**, which is the declared-versus-reconstructed check for geometry
itself: for each record, every vertex of its own triangle soup must lie on the analytic
surface within `tol_geom`, and the soup's bounding box must equal the analytic bounding
box within `tol_geom`. Failure is `GEOM_MESH_DISAGREES_WITH_RECORD` and fails I8. This
catches a record whose numbers say one thing and whose shipped triangles say another —
exactly the class of defect the engine's disclosures were unable to see.

**Inside tests.** Point-in-solid on `M` is exact ray parity against the welded closed
mesh with a deterministic ray-direction sequence: on any hit within `tol_geom` of a
triangle edge or vertex the query retries with the next direction of the fixed sequence
`d_k = normalize(golden_spiral(k))`, `k = 0..15`. Sixteen exhausted retries make the
query `indeterminate`, which fails whichever invariant asked. No perturbation of the
query point is permitted — moving the point changes the question.

### 1.5 Tolerances, all derived

Nothing here is a round number chosen for looks.

- `eps32 = 2^-24 = 5.960464477539062e-8` — binary32 relative precision. The file stores
  every coordinate and radius as `f32`, so this is the resolution of the evidence.
- `tol_geom(p, q) = 4 * sqrt(3) * eps32 * max(|p|_inf, |q|_inf, 1.0)` mm. Four rounded
  operations (two stored endpoints, one differencing, one norm), three coordinates in
  quadrature. At a 100 mm coordinate this is `4.130e-5 mm`. It is used as a *function of
  magnitude*, never flattened to a constant.
- `eps64 = 2.220446049250313e-16`. Sums over reconstructed samples use the project's
  own derived summation bound, cited rather than re-derived: relative
  `4096 * eps64 = 9.09495e-13` and absolute `1e-9` in the metric's own unit
  (`docs/development.md`, tolerance rule). The 4096 is headroom over the largest
  corpus fold, on the order of 10^3 items.
- `pixel_area = 0.014 * 0.019 = 2.66e-4 mm^2` — the smallest area the printer can
  render. This is the audit's own censor. It is deliberately far stricter than the
  engine's 0.16 mm^2 A3 coverage censor, which is a sampling artefact of a 0.4 mm
  analysis pitch and has no standing here.
- `voxel_volume = 0.014 * 0.019 * 0.050 = 1.33e-5 mm^3` — the smallest volume the
  printer can render, and the audit's volume censor.
- `d2 = sqrt(0.014^2 + 0.019^2) = 2.360085e-2 mm` — in-layer pixel diagonal.
- `d3 = sqrt(0.014^2 + 0.019^2 + 0.050^2) = 5.529014e-2 mm` — voxel diagonal, and the
  width of the **fusion-uncertain band**: two solids separated by a gap in `(0, d3]`
  may or may not cure into one piece, and the audit refuses to decide. Any check that
  lands in that band returns `indeterminate`.

### 1.6 Rasterisation, once, batch-first

Several invariants need the model and the support as the slicer would cure them. The
audit rasterises **once per file** into the printer's own grid — `0.014 x 0.019` mm in
plane, `0.050` mm in Z, origin at the centred frame's origin — and every check reads
that one product.

The representation is **run-length, never a pixel array**: slice each solid at every
layer plane `z_k = z_plate + (k + 0.5) * layer_height`, take the resulting polygons,
and store, per pixel row, the sorted list of half-open column intervals the polygon
covers. Interval lists are O(edges), the attachment test between layers `k` and `k-1`
is an interval intersection per row, and connected components are computed on interval
runs. A per-pixel loop over a 16K grid is a defect, not an implementation choice: a
200 mm model is on the order of 1.5e11 pixel visits and the interval form is on the
order of 1e7 edge visits for the same answer.

**Fill rule — the conservative choice.** A pixel column is cured at layer `k` only if
the solid contains the **whole** pixel footprint at `z_k`, not merely its centre.
Partial pixels are treated as **not cured**. The printer's real partial-exposure
behaviour is unmeasured, and a full-containment rule means no audit pass ever rests on
an unmeasured partial-cure claim. The error direction is toward failing, which is the
safe direction for a gate. The cost is real and must be disclosed, not hidden: a
delivered patch loses roughly a one-pixel rim, about 10 % of area at the corpus median
tip diameter of 0.394 mm and much more below 0.1 mm diameter. The count of contacts
whose verdict would flip under a centre-sampling rule is reported in
`report.raster_erosion_sensitivity` so the conservatism is visible and arguable.

---

## 2. The shared reconstruction every load invariant reads

I2, I3 and I4 all need "the model area this contact carries". The engine's number for
that is not in the file, so the audit builds its own, once.

### 2.1 The demand set `D`

Peel acts on the surface the release film touches. For a model triangle `t` with
outward unit normal `n` and area `A(t)`, the film-contact area it presents is its
**projection onto the plate**:

```
demand(t) = A(t) * |n . z_hat|      when n . z_hat < 0
demand(t) = 0                       otherwise
```

This is derived, not chosen. It is exactly the quantity the calibration coupons
measured — their caps were flat and parallel to the plate, where projected area and
true area coincide — so `X_ALLOWED` is applied to the same quantity it was calibrated
on. It degrades to zero for a vertical wall, which is correct: a vertical wall
presents no film contact. It needs no self-support angle, no reach radius and no
placeholder.

`D_total = sum over triangles of demand(t)`, in mm^2.

**Reconstructed a second way, and required to agree.** The same quantity falls out of
the raster: summed over layers, the area of pixels cured at layer `k` whose own column
is not cured at layer `k-1` is the plate-projected down-facing area. Compute both and
require

```
|D_total - D_raster| <= 0.5 * perimeter_total * d2 + 4096*eps64*D_total
```

where `perimeter_total` is the summed length of the down-face boundary across layers.
The first term is the quantisation bound: a rasterised boundary can differ from the
true boundary by at most half a pixel diagonal along its length. Disagreement beyond it
means the mesh is self-intersecting or not the solid it claims to be, and fails the
file with `FILE_MESH_NOT_CLOSED` reason `demand_reconstruction_disagrees`.

`D` is then sampled for ownership: subdivide each down-facing triangle by dyadic 1-to-4
subdivision until every edge is no longer than `d2`, and take each terminal centroid as
a sample carrying its own share of `demand(t)`. Terminating at the in-plane pixel
diagonal ties the sampling resolution to the printer rather than to an analysis
constant.

### 2.2 Ownership — a complete Voronoi partition, no reach constant

Surface components are the connected components of the welded model mesh under
exact-`f32`-vertex adjacency (§1.4). Each contact is routed to the component containing
its seat entry triangle (§3.1).

```
owner(s) = argmin over contacts c on the same surface component as s
                  of  |x(s) - p0(c)|
A_owned(c) = sum of demand weights of samples s with owner(s) = c
```

Ties within `tol_geom` resolve to the lower record index, so the partition is
deterministic. There is **no reach limit**. This is the project's own doctrine —
`docs/physics.md` §3, "each arm's Voronoi cell area on the down-face is its
`A_supported`" — and it is deliberately free of `TARGET_SPACING_MM`,
`coverage_radius_factor` and every other placeholder. A contact standing alone under a
large face genuinely owns that face, and the area law is entitled to say so.

Samples on a surface component that carries **no** contact are unowned. Their total is
`A_unowned`, and it is I4's hard failure.

### 2.3 The engine's own books, recovered by inversion

Measured 2026-08-30: `X = carried_area / (pi * r_tip^2) = 12.000` on every contact,
maximum deviation 7e-15. The engine sizes every tip exactly to the law, so the tip
radius in the file **is** the engine's declared carried area, recoverable:

```
A_implied(c) = x_allowed * pi * r_tip(c)^2         (DECLARED, by inversion)
```

The inversion is injective only away from the sizing clamps. Where
`r_tip(c) <= tip_radius_min_allowed_mm + tol_geom` or
`r_tip(c) >= tip_radius_max_mm - tol_geom` the tip is clamp-limited and
`A_implied(c)` is not a point value. Those contacts are flagged
`implied_area_unrecoverable` and enter the conservation check in §6.4 as an interval,
never as a point and never as an exemption.

---

## 3. The eight gating invariants

Each contact is identified by `contact_id` = the record index of its type-7 TIP record
within its model's record array, which is stable for a given file.

### 3.1 I1 — Seat depth

**Reconstruction.** The TIP record stores `start = p0` (the ball's deepest point, its
south pole) and `end = p0 + 2R u`, radii `(R, R)`, where `u` is the outward unit axis
and `R = f * r_tip` with `f = ball_factor_stub`. So

```
u          = normalize(end - start)                       RECONSTRUCTED
R          = r1 = r2                                      DECLARED, cross-checked
r_tip_A    = R / f                                        from the TIP record
r_tip_B    = r_start of the adjacent TAPER record         from the TAPER record
r_tip_C    = radius of the sphere fitted to the TIP soup  RECONSTRUCTED
```

All three must agree within `tol_geom`. Disagreement is
`SEAT_DEPTH_DECLARED_MISMATCH` reason `tip_radius_three_way`. This triangulation is
free and it catches a record whose radii and whose shipped ball are different objects.

```
h_delivered(c) = the distance from p0 to the first crossing of the model surface,
                 travelling from p0 along +u                    RECONSTRUCTED
```

Measured along the axis, because the seat formula's cut plane is perpendicular to that
axis. If `p0` is outside `M`, `h_delivered = 0`. Report also `h_perp(c)`, the
perpendicular distance from `p0` to the nearest point of the model surface, which is
diagnostic only.

**The requirement, per contact, from the contact's own geometry.**

```
n          = outward model normal at the entry crossing        RECONSTRUCTED
abs_cos    = |u . n|
sin_theta  = sqrt(max(0, 1 - abs_cos^2))
ball_seat_factor(f) = f - sqrt(f*f - 1)          = 0.3819660112501051 at f = 1.5

h_req(c) = max( default_contact_depth_mm,
                layer_height_mm + r_tip(c) * sin_theta,
                ball_seat_factor(f) * r_tip(c) )
```

`n` is taken from the crossed triangle and cross-checked against the area-weighted mean
normal over the intersection of the ball `B(centre, R)` with the model surface; a
disagreement above 5 degrees marks the contact `normal_ambiguous` and the contact is
`indeterminate`, because the incidence term is then not established.

**Predicate.**

```
h_delivered(c) > 0                                and
h_delivered(c) >= h_req(c) - tol_geom(p0, entry)
```

A tip at exactly zero depth is a hard failure regardless of anything else — a ball
resting on the surface transfers load across a tangent point, and no cut disc exists.

**Declared comparison.** The adjacent TAPER record carries the declared contact depth in
trailing float slot 0. Require `|h_declared - h_delivered| <= tol_geom`. This is the
check that turns the known 0.300 mm lie into a per-file, per-contact finding.

**Note on where each term binds.** At normal incidence the ball term overtakes the
0.100 mm default only for `r_tip > 0.100 / 0.3819660112501051 = 0.261803 mm`; the
incidence term overtakes it only for `r_tip * sin_theta > 0.050 mm`. Below both, the
ruled default binds, which is the intended effect of the ruling.

**Failure lines.**

```
SEAT_DEPTH_ZERO file=<path> model=<m> contact=<c> record=<r> h_delivered_mm=<v> p0=<x,y,z> u=<x,y,z>
  the tip ball is not embedded: its deepest point lies on or outside the model surface

SEAT_DEPTH_SHALLOW file=<path> model=<m> contact=<c> record=<r> h_delivered_mm=<v> h_required_mm=<v> binding_term=<default|incidence|ball> r_tip_mm=<v> sin_theta=<v> deficit_mm=<v>
  the delivered seat is shallower than this contact's own requirement

SEAT_DEPTH_DECLARED_MISMATCH file=<path> model=<m> contact=<c> record=<r> declared_mm=<v> reconstructed_mm=<v> tolerance_mm=<v> reason=<taper_trailing|tip_radius_three_way>
  the record declares a seat the geometry does not deliver

SEAT_DEPTH_INDETERMINATE file=<path> model=<m> contact=<c> record=<r> reason=<ray_parity_exhausted|normal_ambiguous|no_adjacent_taper>
  the seat depth could not be established; an unestablished seat is a failure
```

**The smallest defect I1 cannot see.** I1 measures depth along one axis at one entry
point. It cannot see a tip that is deeply embedded on its axis but whose ball is
mostly outside the material because the surface is convex at a radius comparable to
`R` — a ball on a 0.4 mm-radius knuckle can satisfy `h_delivered >= h_req` while
overlapping the model in a sliver far smaller than the cut disc implies. The delivered
patch, not the depth, is the load-bearing quantity, and only I2 sees that sliver.
I1 also cannot see a correct depth achieved against the **wrong** surface: a tip seated
into a stray internal shell of a self-intersecting mesh crosses a surface and reports a
depth, and only I6's far-side and seat-zone tests notice the shell is not the outer one.
Finally the incidence term uses the crossed triangle's normal, so on a mesh whose
tessellation is coarser than `R` the normal can be up to the local facet angle wrong,
and the audit only flags that above 5 degrees.

### 3.2 I2 — Rasterised contact area

The load-bearing check, and the one no instrument has measured. The ideal cut disc is
not what cures; the pixels are.

**The plane.** The separating cut through the tip, perpendicular to the support axis
`u`, positioned at the delivered seat depth:

```
P(c) = { x : (x - p0) . u = h_delivered(c) }
```

That plane is chosen because it is where the physics places the minimal separating cut
(`docs/physics.md` §9, "the ball's cut disc for a ball-tipped end"). A horizontal
Z-plane would measure a different quantity and would change with the arm's tilt.

**The quantisation.** The printer rasterises in XY at `0.014 x 0.019` mm with `0.050`
mm layers, not in `P(c)`. So the patch is measured on the **cured** support solid, not
the ideal one: take the run-length raster of `S` from §1.6, treat each cured voxel as
the closed box it occupies, and compute the exact planar area of

```
Patch(c) = the connected component containing (p0 + h_delivered * u)
           of  P(c)  intersected with  the cured voxel union of S
A_rast(c) = area(Patch(c))                                  mm^2, RECONSTRUCTED
```

The component restriction matters: another member crossing the same plane contributes
its own disc, and that disc carries none of this contact's load.

**Partial pixels are not cured** (§1.6). This is the conservative choice and its cost
is disclosed rather than absorbed.

**Predicate.**

```
A_rast(c) >= A_owned(c) / x_allowed  -  tol_area(A_owned(c))
tol_area(A) = max(4096 * eps64 * A / x_allowed, 1e-9)        mm^2
```

The tolerance is the project's own derived summation bound (§1.5) applied to the one
summed input, `A_owned`. `A_rast` is an exact sum of polygon areas and needs none of
its own.

**Three-way disclosure, all reported per contact.**

```
A_ideal(c)   = pi * a^2,  a^2 = 2*f*r_tip*h_delivered - h_delivered^2   RECONSTRUCTED, ideal
A_rast(c)                                                               RECONSTRUCTED, cured
A_implied(c) = x_allowed * pi * r_tip^2                                 DECLARED, by inversion
raster_loss(c) = 1 - A_rast(c) / A_ideal(c)
```

`A_rast > A_ideal + tol_area` is itself a failure, reason `raster_exceeds_ideal`: the
cured patch cannot be larger than the geometry, so that combination means the plane,
the axis or the union is wrong.

**Failure lines.**

```
CONTACT_AREA_SHORT file=<path> model=<m> contact=<c> record=<r> a_rast_mm2=<v> a_required_mm2=<v> a_owned_mm2=<v> a_ideal_mm2=<v> a_implied_mm2=<v> raster_loss=<v> x_delivered=<v>
  the cured contact patch cannot carry the model area this contact owns

CONTACT_PATCH_DISCONNECTED file=<path> model=<m> contact=<c> record=<r> components=<n>
  the cut plane meets no cured support material at the axis point

CONTACT_AREA_INDETERMINATE file=<path> model=<m> contact=<c> record=<r> reason=<seat_indeterminate|raster_exceeds_ideal|ownership_indeterminate>
  the delivered patch could not be established; an unestablished patch is a failure
```

**The smallest defect I2 cannot see.** The check is a single planar cut. It cannot see
a patch that is adequate in area but is **shaped** to fail — a crescent or an annulus
of the same area as a disc will tear from its thin end first, and the area law was
calibrated on round caps. It cannot see a patch that is adequate at `P(c)` but whose
support material is not continuously cured between `P(c)` and the model: a one-voxel
void between the ball and the wall, produced by the full-containment rule combined with
a grazing surface, sits inside the fusion-uncertain band and reports indeterminate
rather than being resolved. It cannot see over-exposure bleed, which is real on this
machine and would *increase* the delivered patch — so I2 fails some contacts that would
print. And it cannot see the tension-versus-shear difference at incidence: the area law
was measured in pure tension, `docs/physics.md` §11 says no interaction criterion is
attempted, so a patch at 70 degrees of incidence is graded by a number measured at 0.

### 3.3 I3 — The area law at the arm's narrowest section

`docs/physics.md` §2(b): the failure section is the narrowest cross-section of the arm
just above the contact, not the tip. This has never been checkable, and it is the check
that catches an engine that sizes the tip correctly and then hangs it on a 0.35 mm
placeholder arm.

**The load path.** For each contact `c`, walk the connection graph of §3.5 from the TIP
record rootward, appending each primitive, and stop at the first primitive at which a
second live load path merges (in-degree greater than one) or at ground. The result is
`path(c)`, an ordered list of primitives, and its centreline polyline `L(c)`.

**The stations.** Sample `L(c)` at arc-length step

```
step = min(pixel_pitch_x, pixel_pitch_y, layer_height) / 2 = 0.007 mm
```

Half the finest raster quantum, so no feature the printer can render is stepped over.

**The section at a station.** At station `s` with unit tangent `t(s)`, take the plane
through `x(s)` with normal `t(s)`, intersect it with the analytic union of `path(c)`'s
primitives, and take the area of the connected component containing `x(s)`:

```
A_sec(s) = area of that component                          mm^2, RECONSTRUCTED
A_min(c) = min over stations of A_sec(s)
```

The tip's own cut disc is included by construction: the first station lies at
`h_delivered` inside the ball, where `A_sec = A_rast(c)` if the cured union is used and
`pi*a^2` if the analytic one is. **Use the cured union**, consistently with I2, so the
two invariants grade the same solid.

**Predicate.**

```
A_min(c) >= A_owned(c) / x_allowed  -  tol_area(A_owned(c))
```

with `tol_area` as in I2.

**Failure lines.**

```
MIN_SECTION_SHORT file=<path> model=<m> contact=<c> station_mm=<s> primitive_record=<r> a_min_mm2=<v> a_required_mm2=<v> a_owned_mm2=<v> x_delivered=<v> r_at_min_mm=<v>
  the arm's narrowest section cannot carry the area this contact owns

MIN_SECTION_INDETERMINATE file=<path> model=<m> contact=<c> reason=<path_open|path_cycles|ownership_indeterminate|section_component_empty>
  the load path or its narrowest section could not be established
```

**The smallest defect I3 cannot see.** The minimum is taken over cuts **perpendicular
to the local tangent**. At an elbow — a member meeting a joint ball at an angle — a
slanted cut across the inside of the bend can be smaller than any perpendicular cut, so
the reported `A_min` is an **upper bound** on the true minimal separating cut and the
check is optimistic exactly at bends. The per-junction turn angle `beta` is therefore
reported in `report.junction_turn_angles_deg` so the size of the exposure is visible;
a reviewer should read a large `beta` with a marginal `A_min` as unproven, not as a
pass. I3 also cannot see a section that is adequate in area but is a thin ribbon rather
than a disc, for the same reason I2 cannot. And it stops the walk at the first merge,
so it cannot see a shared trunk that is too thin for the **sum** of the loads that merge
into it — that defect belongs to a trunk-load check this specification does not define,
and its absence is named here rather than left to be discovered.

### 3.4 I4 — Unique load ownership

Every unit of carried model area has exactly one owner: none double-counted, none
unassigned.

**Predicate, four clauses, all gating.**

```
(a) A_unowned  <=  pixel_area                                     one printer pixel
(b) | sum over c of A_owned(c)  -  D_total |  <=  max(4096*eps64*D_total, 1e-9)
(c) every contact is routed to exactly one surface component, and owns samples
    from no other component
(d) the engine's books bracket the reconstructed demand:
       lo  =  sum over unclamped c of A_implied(c)
       hi  =  lo + (count of clamped c) * x_allowed * pi * tip_radius_max_mm^2
       lo - tol  <=  D_total  <=  hi + tol,      tol = max(4096*eps64*D_total, 1e-9)
```

Clause (a) is "none unassigned", censored at one pixel because the printer cannot render
less. Clause (b) is the partition identity and is the audit auditing itself: a Voronoi
partition cannot double-count, so a violation means the sampler or the component routing
is wrong, and that is a failure of the instrument which must fail rather than pass.
Clause (c) is the cross-component leak: a contact on the left wing owning area on the
right wing is a routing defect that inflates nothing visible until the print fails.

Clause (d) is the declared-versus-reconstructed comparison and it is the sharpest thing
in this document. `A_implied` recovers what the engine believed each tip carried. If the
engine's books total far less than the reconstructed demand, the engine assigned load
that does not exist and the tips are sized for a fiction. If they total far more, area
is double-counted or padded. The clamped contacts enter as an interval rather than a
point, so the check never rests on an inversion that is not injective, and it stays
gating because an interval still refuses.

**Failure lines.**

```
LOAD_UNOWNED file=<path> model=<m> a_unowned_mm2=<v> censor_mm2=2.66e-4 components_without_contact=<n> largest_unowned_component_mm2=<v>
  down-facing model area has no support responsible for it

LOAD_COMPONENT_UNSUPPORTED file=<path> model=<m> component=<k> area_mm2=<v> demand_mm2=<v> centroid=<x,y,z>
  a connected model surface component carries demand and no contact at all

LOAD_BOOKS_DISAGREE file=<path> model=<m> d_total_mm2=<v> implied_lo_mm2=<v> implied_hi_mm2=<v> clamped_contacts=<n> ratio=<v>
  the load the tips are sized for and the load the geometry presents are different numbers

LOAD_OWNERSHIP_INDETERMINATE file=<path> model=<m> reason=<partition_identity_violated|component_routing_failed|sample_budget_exceeded>
  the ownership partition could not be established
```

**The smallest defect I4 cannot see.** Ownership is nearest-contact by straight-line
distance on the same surface component. It cannot see that a contact and the area it
owns are on opposite sides of a **notch**: two points 0.5 mm apart in space and 40 mm
apart across the surface are the same component and the nearer contact wins, so the
partition can hand a contact area it could never hold. A geodesic partition would see
it; this one does not, and the exposure grows with surface concavity. It also cannot
see whether the load is *shared*: `docs/physics.md` §4 records that sharing is real and
at least additive, so a complete Voronoi partition is conservative rather than correct,
and a plan that fails I4 clause (d) by a factor near one may be honest. Finally, clause
(d) rests on the measured fact that the engine sizes every tip to exactly `X = 12`; if a
future engine sizes tips any other way, the inversion silently becomes meaningless and
this clause must be re-derived rather than re-tuned.

### 3.5 I5 — Closed load path

**The connection graph.** Nodes are primitives. An edge joins two primitives whose
analytic solids **overlap**, meaning their signed separation is strictly negative. A
separation in `[0, d3]` is inside the fusion-uncertain band and produces an
`indeterminate` edge, which fails I5 rather than being resolved either way.

**Ground.** A primitive is grounded if either

```
(i)  its solid intersects the half-space z <= z_plate + layer_height     the plate
(ii) it holds a valid seat: its solid intersects M, its seat zone passes I6, and
     the contact it belongs to passes I1                                 the model
```

Clause (ii) is the three-seat rule of `docs/physics.md` §8: plate, model surface
including a cavity's own interior, or another support. Standing on another support is
handled by connectivity, not by a third ground clause.

**Predicate.**

```
(a) every primitive lies in the connected component of the ground set
(b) no primitive has a free end: for each frustum end disc that is neither grounded
    nor overlapped by another primitive's solid, that end is in free space
(c) the rootward relation is acyclic: order primitives by graph distance to ground,
    and require every primitive except a grounded one to have at least one neighbour
    strictly closer to ground
(d) every plate root stands on the plate: the lowest point of every FLARE record
    (type 4, a plate root by format definition) is z_plate, or the FLARE overlaps a
    raft record (type 6) whose own lowest point is z_plate. Both equalities are read
    through tol_geom.
```

Clause (c) is the honest form of "no cycles". A cycle in the undirected connection graph
is legitimate — bracing makes them on purpose. What is not legitimate is a set of
primitives whose only route to ground is through each other, and (c) is exactly that
statement as a number.

Clause (d) was added 2026-09-28 (fix/plate-gap). Ground clause (i) asks whether a solid
reaches the plate's first layer; that is the right test for connectivity and the wrong
test for a root. A root's lowest point is its contact with the plate. If that point is
above the plate, the first layer of the root cures against the vat film and not against
the plate. The owner saw this in ChituBox: every slice-sweep root flare stopped 0.2 mm
(four layers) above the plate, at the top of a raft that was never emitted. Clause (i)
could not see a root that stops inside the first layer, and it let a floating root pass
whenever the tree was grounded through a model seat. A root below the plate is the
frame check's failure (§1.2), not this clause's.

**Addendum 2026-09-29 (fix/sweep-raft): the raft's own frame.** Every type-6 record in
every ChituBox-authored file writes start/end `z = 0.0` while its mesh sits on the plate in
the model-centred frame; plinth's emitter now writes the same, and the slice sweep emits a
raft again with each flare ending on its pad top. The audit therefore reconstructs a type-6
record whose ends are exactly `z = 0.0` ON the plate (`Primitive::in_raft_frame`, applied
once when the model context is built), and keeps the declared `z` of any other type-6
record (the legacy model-frame pads). So "a raft record whose own lowest point is z_plate"
holds for native-frame and legacy pads alike, and the §1.2 cross-check (`raft_agrees`) now
reads true on native files, where it used to read false.

**Clause (e), no support island (added 2026-09-28, fix/no-dangling; written as clause (d)
on that branch and renumbered when it landed after fix/plate-gap's clause (d)).**
Connected is not held up. An arm whose tip is seated in the model and whose lower end
stops in the air is connected through the model and grounded by (ii), yet its lowest
layer prints on nothing (the "porcupine quills",
`plinth-evidence/slice-sweep-2026-09-02/field-2026-09-28/quills.md`). So, directionally:

```
(e) the LOWEST point of every primitive (a frustum's lower axis end, a ball's bottom)
    rests on one of
      the plate      z <= z_plate + layer_height; a root FLARE may end at the raft top,
                     z_plate + RAFT_PAD_HEIGHT_MM + layer_height (the format's flare
                     contract; counted apart as below_rooted_at_raft_top)
      the model      inside M, or within layer_height of its surface — NEVER for a tip
                     ball: a seated tip holds only its own member's top
      a primitive    whose solid is within layer_height of that point and whose own
                     lowest z is strictly lower
    and the chain it rests on ends at the plate or the model.
```

The member clause is a point test, not the overlap graph above, so a lowest point that
sits inside a neighbour is not lost in the fusion-uncertain band. A model test that
cannot be decided is counted as `below_indeterminate`, never as a pass or a failure.

Clauses (d) and (e) overlap on one case. Clause (e) accepts a FLARE that ends at the raft
top as rooted, because it asks only whether the flare is held up from below; clause (d)
still fails that flare as `PATH_ROOT_FLOATING` when no raft record lies under it, because
it asks whether the root stands on the plate. A raft-top flare with no raft under it is
therefore not a clause (e) failure but is a clause (d) failure, so the two lines never
report the same defect.

**Failure lines.**

```
PATH_ORPHAN file=<path> model=<m> record=<r> type=<t> component_size=<n> nearest_ground_mm=<v>
  this support has no route to the plate, to a valid model seat, or to another support

PATH_FREE_END file=<path> model=<m> record=<r> type=<t> end=<start|end> point=<x,y,z> radius_mm=<v> nearest_solid_mm=<v>
  an arm terminates in free space

PATH_ROOT_MISSING file=<path> model=<m> grounded_primitives=0 records=<n>
  no primitive reaches the plate or a valid model seat

PATH_ROOT_FLOATING file=<path> model=<m> record=<r> type=flare lowest_z_mm=<v> gap_above_plate_mm=<v>
  a plate root stops above the plate, with no raft on the plate under it (clause d)

PATH_NO_SUPPORT_BELOW file=<path> model=<m> record=<r> type=<t> lowest_point=<x,y,z> rests_on=<nothing|unrooted_member> nearest_lower_solid_mm=<v>
  clause (e): this primitive's lowest point rests on nothing that reaches the plate or the model

PATH_INDETERMINATE file=<path> model=<m> record=<r> other=<r2> separation_mm=<v> band_mm=5.529014e-2 reason=<fusion_uncertain|seat_indeterminate>
  two solids are close enough that fusion cannot be decided; an undecided path is a failure
```

**The smallest defect I5 cannot see.** Overlap is a *geometric* test on analytic solids,
and cure is a *raster* event. Two solids that overlap by less than one voxel of volume
are connected in this graph and may be two pieces in the print; the fusion-uncertain
band catches the *gap* side of that boundary but not the thin-overlap side, so an
overlap in `(0, voxel_volume]` passes as connected. I5 also says nothing about
stiffness: a path that is closed but consists of one 0.35 mm arm 40 mm long is a closed
path and a noodle, and only I3 grades its section, never its slenderness. Nothing here
implements a buckling check, and no buckling constant exists to implement it with.

### 3.6 I6 — No illegal intersection

Two distinct claims: support material inside the model outside its own seat, and a tip
protruding out the far side of a thin section.

**Seat zone, defined precisely.** For contact `c`, let `I(c)` be the connected component
containing `p0(c)` of the intersection solid `S ∩ M`, restricted to the primitives of
`c`'s own tip group (its type-7 TIP record and the adjacent type-1 TAPER record). The
**seat zone** is

```
Z(c) = { x in I(c) :  0 <= (x - p0) . u <= h_delivered(c) + layer_height
                      and  |x - (p0 + ((x-p0).u) u)| <= r_axis(c, (x-p0).u) + d2 }
```

where `r_axis` is the tip group's own radius at that axial station. In words: the seat
zone is the part of the intersection that lies inside the contact's own footprint,
between the ball's deepest point and one layer past the delivered entry.

This definition is deliberately **not** the engine's connected-surface-component
exemption of `docs/physics.md` §6. That rule exempts a target on the *surface* and is
the right rule for a clearance grader with a search to prune. The audit is grading
delivered solids, so it exempts a *bounded volume around the contact's own axis*, which
cannot be widened by a fat arm and cannot swallow a far wall.

**Predicate.**

```
(a) volume( (S ∩ M)  minus  union over c of Z(c) )  <=  voxel_volume
(b) for every contact c, the components of  S(c) minus M  number exactly one,
    where S(c) is the tip group's solid
```

Clause (a) is "no support geometry inside the model outside its own seat zone",
censored at one voxel because less than that is not rendered. Clause (b) is the
far-side test and it needs no wall-thickness measurement: a tip that punches through a
thin wall leaves a **second** blob of support material outside the model on the far
side, so counting exterior components of the tip group is exactly the punch-through
question. The truncation rule of `docs/physics.md` §7 says punch-through must be
impossible by construction; clause (b) is the check that it actually was.

Both clauses are evaluated on the **cured raster** union, not the analytic one, so the
volumes are exact sums of voxels and the censor is a count of one.

**Failure lines.**

```
INTERSECT_OUTSIDE_SEAT file=<path> model=<m> record=<r> type=<t> volume_mm3=<v> censor_mm3=1.33e-5 centroid=<x,y,z> nearest_contact=<c> distance_mm=<v>
  support material is buried in the model away from any contact's own seat

INTERSECT_FAR_SIDE_BREACH file=<path> model=<m> contact=<c> record=<r> exterior_components=<n> breach_volume_mm3=<v> breach_centroid=<x,y,z>
  the tip protrudes out the far side of the section it is seated in

INTERSECT_INDETERMINATE file=<path> model=<m> record=<r> reason=<ray_parity_exhausted|seat_indeterminate|raster_budget_exceeded>
  the intersection could not be established
```

**The smallest defect I6 cannot see.** Clause (a) exempts a bounded band along the
contact's own axis, so a tip that dives **along** its own axis far deeper than it needs,
tunnelling inside the model, is inside `Z(c)` up to `h_delivered + layer_height` and is
only caught beyond that — a seat that is 0.4 mm deep where 0.1 mm was required is legal
under I1 (deeper passes) and legal under I6 (it is its own zone), and it makes a scar
this audit will not refuse. Clause (b) counts exterior components of the tip group only;
a **member** that clips a thin fin far from any contact is caught by (a) but a member
that enters and exits through the same thin fin leaves one exterior component on each
side and clause (b) does not examine members at all. And the accepted crevice-contact
ruling of 2026-08-25 means an arm touching the opposite wall of a narrow crevice is
correct behaviour, so I6 must not and does not refuse it — which also means I6 cannot
distinguish that accepted case from an arm that ploughs through a genuinely thin
feature, when the two walls are one connected sheet.

### 3.7 I7 — Island coverage

**The demand this invariant gates.** From the layer raster of `M`: pixel column
`(i, j)` is *attached* at layer `k` if column `(i, j)` is cured at layer `k-1`. A
connected run-component `C` of cured pixels at layer `k` is a **detached island** if it
contains zero attached pixels and `k` is not the first layer above the plate.

That definition is fully derived from the printer grid. It uses no self-support angle
and no reach radius, and it is uncontestable physics: material with nothing under it
and nothing beside it falls into the vat.

**Predicate.**

```
for every detached island C at layer k with area(C) > pixel_area:
    the cured support raster at layer k must intersect C
```

**Failure lines.**

```
ISLAND_UNSUPPORTED file=<path> model=<m> layer=<k> z_mm=<v> area_mm2=<v> centroid=<x,y> pixels=<n>
  a detached island of model material is born at this layer with no support under it

ISLAND_INDETERMINATE file=<path> model=<m> layer=<k> reason=<slice_failed|raster_budget_exceeded>
  the slice stack could not be established at this layer
```

**The smallest defect I7 cannot see, and it is the largest hole in this document.**
I7 gates on **detached** islands only. It cannot see a **cantilever**: material that is
attached to the previous layer along one edge and reaches out unsupported for any
distance. A 20 mm horizontal ledge attached along one edge, a cavity ceiling that closes
over a hollow, and a wing that grows sideways one pixel per layer are all invisible to
this invariant, and cavity ceilings are exactly what `docs/physics.md` §8 says must be
supported from the cavity's lower interior.

Gating the cantilever case needs a measured self-support span, and the project has
none. The raster gives a derived but far too strict substitute — a surface is
self-supported only if it recedes by at most one pixel per layer, which is 74.36 degrees
from horizontal on the X pitch and 69.19 degrees on the Y pitch, so a 60-degree wall
that prints perfectly every day would be condemned. Using that as a gate would fail
every model for a reason nobody believes, which is worse than not gating.

So the cantilever exposure is measured and reported, never passed: `report.overhang`
carries the total unattached area, the maximum unattached run span in mm, and the
maximum unattached span whose layer contains no support pixel. Its verdict field is
literally `"disclosed"`, and `"disclosed"` is not a passing value anywhere in the schema.
**The measurement this needs is a self-support span coupon** — a ladder of cantilever
lengths at fixed layer height — and until it exists this invariant is honestly narrower
than its name. Note also that the demand set `D` of §2.1 *does* include every cantilever
and cavity ceiling by projected area, so I2, I3 and I4 still grade whether the supports
that exist are big enough for them; what is missing is only the demand that no support
was placed for at all.

### 3.8 I8 — No dead or degenerate geometry

**Degenerate, exact, no tolerance.** A primitive is degenerate if any of:

```
- it is type 1, 3 or 4 and |end - start| == 0 exactly
- any radius <= 0
- any stored coordinate or radius is not finite
- its triangle soup is empty, or contains a triangle of exactly zero area
- its soup disagrees with its record's analytic form beyond tol_geom (§1.4)
```

These are exact-zero tests because the emitter should never write them at all; the
corpus carries 5 to 43 zero-length segments per file, so this fires today.

**Liveness by structural role, never by load alone.** A member carrying no
directly-assigned model area may still brace another member or close a load path. A
primitive is **live** if it holds at least one of these roles, each of which is a
number:

```
R1  it is a TIP with A_owned(c) > pixel_area                      it carries demand
R2  removing it disconnects at least one live TIP from ground     it is a cut member
R3  removing it strictly increases the maximum free span of some
    live member, where free span is the distance between that
    member's consecutive junction contacts                        it braces
R4  it is a type-6 raft primitive and at least one live primitive
    is grounded through it                                        it is the interface
```

A primitive with none of R1 to R4 is **dead**. R3 is why zero carried load does not by
itself condemn a member: a brace holds no directly-assigned area and its removal is
measurable in the span it was preventing.

Conversely, a TIP that owns no demand area, whose removal disconnects nothing, and whose
removal changes no span is dead **even though its parent member is live**. Roles are
per-primitive and a parent's usefulness is never inherited downward. Corpus evidence:
16 of 96 supports carry zero area on one model.

**Predicate.**

```
degenerate_count == 0   and   dead_count == 0
```

**Failure lines.**

```
GEOM_DEGENERATE file=<path> model=<m> record=<r> type=<t> reason=<zero_length|nonpositive_radius|nonfinite|empty_soup|zero_area_triangle> value=<v>
  this record encodes geometry that cannot be printed

GEOM_MESH_DISAGREES_WITH_RECORD file=<path> model=<m> record=<r> type=<t> max_deviation_mm=<v> tolerance_mm=<v>
  the shipped triangles are not the solid this record describes

GEOM_DEAD_MEMBER file=<path> model=<m> record=<r> type=<t> volume_mm3=<v> roles_tested=R1,R2,R3,R4
  this member carries no load, closes no path, and braces nothing

GEOM_DEAD_TIP file=<path> model=<m> contact=<c> record=<r> a_owned_mm2=<v> parent_record=<r2>
  this tip owns no demand and has no structural role; its parent's load does not justify it

GEOM_INDETERMINATE file=<path> model=<m> record=<r> reason=<ownership_indeterminate|path_indeterminate>
  liveness could not be established
```

**The smallest defect I8 cannot see.** R2 and R3 are single-removal tests. A **pair** of
members that are each redundant alone but jointly the only route to ground both pass R2
individually and both are dead-by-pairs; the check never removes two at once, and the
number of pairs is quadratic so it deliberately does not try. R3's free span is measured
between junction contacts along one member, so a brace that stiffens a *joint* rather
than a span — a gusset at an elbow — registers no span change and is condemned as dead
although it is doing real work. And "dead" here is a structural verdict, not a resin
verdict: a live member can still be grossly oversized, and I8 will not say so. That
belongs to the non-gating resin split.

---

## 4. The non-gating report

These numbers describe the plan. They never decide a verdict, they carry no pass or
fail, and their verdict field is `"disclosed"`. They exist so that a plan that passes
every gate can still be argued about with data.

```
report.spacing
    contact-to-contact nearest-neighbour 3D distance over all contacts of a model:
    min, p01, p05, median, p95, max, count_below_2.5mm, count_below_4.0mm, histogram
    with 0.1 mm bins.  2.5 mm is SPACING_FLOOR_MM and 4.0 mm is TARGET_SPACING_MM,
    both Placeholder, quoted as reference lines and never as thresholds.
    Corpus reference 2026-08-30: median 1.03 mm, one pair at 0.049 mm.

report.arm_radius
    the radius at each member's own narrowest section, over all type-3 members:
    min, median, max, histogram with 0.01 mm bins, and
    count_at_min_shoot_r = number within tol_geom of 0.35 mm.
    Corpus reference: 36 to 54 percent sit at exactly MIN_SHOOT_R_MM = 0.35 mm,
    an unmeasured placeholder.

report.scar_orientation
    for each contact, the entry-surface normal component n . z_hat, bucketed at 0.1:
    fraction_downward (n.z < 0), fraction_vertical (|n.z| <= 0.1),
    fraction_upward (n.z > 0.1), and the total scar area in each bucket.
    Corpus reference: 28 to 31 percent of contacts land on near-vertical or
    upward-facing surfaces.

report.resin_split
    cured voxel volume of the support raster and of the model raster, in mL,
    and support_fraction = support / (support + model).
    Corpus reference: support is 21 to 48 percent of poured resin; the treant 47
    percent, prepared solid.

report.overhang                                   verdict "disclosed", never "pass"
    total unattached area mm2, max unattached run span mm, max unattached span in a
    layer with no support pixel, and the count of layers carrying unattached area.
    This is the cantilever exposure I7 does not gate; see §3.7.

report.raster_erosion_sensitivity
    count of contacts whose I2 verdict would change under a pixel-centre fill rule
    instead of full containment, and the same count for I3 and I6.
    This is the price of the conservative choice of §1.6, made visible.

report.junction_turn_angles_deg
    per junction, the turn angle between consecutive member axes: histogram, max.
    This is the size of I3's elbow exposure, made visible.
```

---

## 5. Output schema

One JSON document per audited `.chitubox` file, written to
`<out_dir>/<file_stem>.audit.json`, plus one human-readable summary on stdout. Every
implementation agent codes against this schema exactly; a field added later must not
rename or retype one that is here.

```json
{
  "schema": "plinth/audit-result",
  "version": 1,
  "tool": { "name": "plinth-audit", "version": "<semver>", "commit": "<git sha or null>" },
  "file": { "path": "<abs path>", "sha256": "<hex>", "bytes": 1234567 },
  "declared_inputs": {
    "<name>": { "value": 0.0, "units": "mm", "provenance": "Measured|Derived|Placeholder|OwnerRuling", "source": "<citation>" }
  },
  "models": [
    {
      "index": 0,
      "name": "<model record name>",
      "translate_mm": [0.0, 0.0, 3.0],
      "z_plate_mm": -3.0,
      "model_triangles": 202464,
      "support_records": 96,
      "contacts": 24,
      "invariants": {
        "i1_seat_depth":        { "status": "pass|fail", "checked": 24, "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] },
        "i2_contact_area":      { "status": "pass|fail", "checked": 24, "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] },
        "i3_min_section":       { "status": "pass|fail", "checked": 24, "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] },
        "i4_load_ownership":    { "status": "pass|fail", "checked": 1,  "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] },
        "i5_closed_load_path":  { "status": "pass|fail", "checked": 96, "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] },
        "i6_no_intersection":   { "status": "pass|fail", "checked": 96, "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] },
        "i7_island_coverage":   { "status": "pass|fail", "checked": 4123, "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] },
        "i8_no_dead_geometry":  { "status": "pass|fail", "checked": 96, "failed": 0, "indeterminate": 0, "measured": {}, "failures": [] }
      },
      "report": {
        "spacing": {}, "arm_radius": {}, "scar_orientation": {}, "resin_split": {},
        "overhang": { "verdict": "disclosed" },
        "raster_erosion_sensitivity": {}, "junction_turn_angles_deg": {}
      },
      "verdict": {
        "print_candidate": false,
        "gating_passed": 3,
        "gating_total": 8,
        "first_failure": "CONTACT_AREA_SHORT"
      }
    }
  ],
  "verdict": {
    "print_candidate": false,
    "models_total": 1,
    "models_print_candidate": 0,
    "file_level_failures": []
  }
}
```

Pinned rules on the schema, all of them load-bearing.

1. **`status` takes exactly two values, `"pass"` and `"fail"`.** There is no `"skip"`,
   no `"warn"`, no `"n/a"`. `indeterminate > 0` forces `status = "fail"`.
2. **`print_candidate` is `true` only when all eight gating invariants are `"pass"`.**
   The report section never contributes to it.
3. `measured` is a free-form object per invariant, but each one **must** carry the
   quantities its failure lines name, for every item checked and not only for failures.
   A passing invariant that reports no numbers is not evidence.
4. Each entry of `failures` is
   `{ "code": "<CODE>", "model": 0, "contact": 12, "record": 34, "values": { }, "message": "<the exact failure line of §3>" }`.
   The `message` is byte-identical to the human-readable line, so grepping the log and
   parsing the JSON find the same thing.
5. Every numeric field is a JSON number in the unit named by its suffix
   (`_mm`, `_mm2`, `_mm3`, `_ml`, `_deg`). No unit-free numbers, no strings holding
   numbers. Units are in the key because a number carried between a radius convention
   and a diameter convention lands a factor of two wrong, and this project has been
   bitten by that.
6. **The file-level failures `FILE_PARSE_FAILED`, `FILE_MESH_NOT_CLOSED`,
   `FILE_FRAME_MISMATCH`, `FILE_DECLARED_INPUT_MISSING`, `FILE_RECORD_SEMANTICS_UNKNOWN`
   abort before any invariant runs.** The document is still written, with every
   invariant `"fail"` and `indeterminate` equal to its checked count, so a parse failure
   is never confused with a clean file.
7. `declared_inputs` is echoed in full in every document, including provenance. A result
   that does not say what constants produced it cannot be re-read in six months.

**Human-readable summary.** One block per model on stdout, and nothing else on stdout,
so the tool composes:

```
<file stem>  model <i> "<name>"   NOT A PRINT CANDIDATE  (3/8)
  I1 seat depth        FAIL   24 checked, 21 failed, 0 indeterminate   worst deficit 0.100 mm
  I2 contact area      FAIL   24 checked, 24 failed, 0 indeterminate   worst X delivered 42.1 against 12
  I3 min section       FAIL   24 checked, 24 failed, 0 indeterminate   worst A_min 0.385 mm2 needs 0.433
  I4 load ownership    FAIL    1 checked,  1 failed, 0 indeterminate   books 140.2 mm2, demand 498.7 mm2
  I5 closed load path  pass   96 checked
  I6 no intersection   pass   96 checked
  I7 island coverage   pass 4123 checked
  I8 no dead geometry  FAIL   96 checked, 16 failed, 0 indeterminate   16 dead, 5 degenerate
  disclosed  spacing median 1.03 mm, min 0.049 mm | arms 41% at 0.350 mm | scars 29% not down-facing
  disclosed  support resin 47% | cantilever exposure 312.4 mm2, worst span 18.7 mm
```

A model line reads `PRINT CANDIDATE` only when all eight are `pass`. The word
`disclosed` prefixes every non-gating line so no reader can mistake one for a verdict.

---

## 6. What a later reviewer should attack first

Stated plainly, so the adversarial pass has a starting list rather than a search.

1. **I7's cantilever hole** (§3.7) is the largest. Cavity ceilings, ledges and wings are
   ungated, and closing it needs a self-support span coupon that does not exist.
2. **I3's elbow optimism** (§3.3): the minimum is over axis-perpendicular cuts, which is
   an upper bound on the true minimal separating cut, and it is loosest exactly where
   arms bend.
3. **I4 clause (d) depends on the engine sizing tips to exactly `X = 12`** (§2.3). That
   is measured today and is not a property of any future engine.
4. **I2 grades a round-cap law on patches of arbitrary shape** (§3.2), and grades a
   tension-calibrated number at arbitrary incidence.
5. **The full-containment fill rule** (§1.6) makes the audit fail contacts that would
   print. `report.raster_erosion_sensitivity` says how many.
6. **Ownership is straight-line, not geodesic** (§2.2), so notched surfaces mis-assign.
7. **I8's single-removal liveness** (§3.8) is blind to redundant pairs and to gussets.
8. **Every declared input in §1.3 is a place the audit trusts something it did not
   measure.** Five of the eight are Placeholders or an owner ruling, and each one is
   echoed in the output for exactly that reason.
