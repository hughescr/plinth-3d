# Seat footprint: exempt the entry joint, not its neighbours

**Date:** 2026-08-23

**Status:** DESIGN FOR OWNER DECISION; no production code in this lane

**Branch:** `feature/a4-controller`
**Authority:** Craig's 2026-08-23 ruling that the exemption covers what the foot
actually presses on or occupies, with no distance-connected slack; Craig's
same-day narrowed monotonicity ruling; and the designed-embed rulings in
`SPEC-EMBED-POLICY.md` and `land-at-strike-2026-08-22.md`.

> **Recommendation [requires an exact entry-sheet patch and a contact-aware
> clearance query; a face-ID flood fill is not sufficient].** Define the physical
> footprint from the final emitted embed-end solid at its intended entry crossing.
> Never obtain its identity by walking mesh adjacency. For a clearance demand
> `g`, test the full arm against every model-surface point except the part of the
> **same entry sheet** covered by the `g`-clearance envelope of that embed solid.
> Freeze that patch before grading. Re-cut it after any change to the contact
> geometry, not only a radius change. This keeps the seat legal, leaves an
> opposite crevice wall visible, adds no decision number, and preserves the
> narrowed monotonicity guarantee within each frozen grading pass.
>
> **The bracket is material.** The literal physical footprint alone cannot keep
> an attached arm legal under the unchanged full-axis `+0.300 mm` or `+0.600 mm`
> clearance formula. At the footprint boundary the true surface-to-surface gap is
> zero. The implementation therefore needs both the physical patch and its
> same-entry-sheet clearance shadow. Calling the larger shadow "the physical
> footprint" would be false.

---

## 1. The measured defect [the current rule is structurally unable to distinguish a seat from a connected crevice wall]

The landed candidate on `feature/f1-refreeze` defines

```text
Z = the mesh-connected component containing the stored seat face of
    {model faces within r_contact + STANDOFF_MM of the live arm axis}.
```

It freezes `Z` at contact creation and re-cuts it after sizing. The implementation
is batch-first: one `faces_near_segments_batch` call gathers all bands, then CPU
flood fills run in parallel.

That lifecycle solved the stale-creation-radius self-refusal, but the set predicate
is wrong. Mesh adjacency says only that two triangles belong to one object. It
does not say that one support foot presses on both triangles.

### 1.1 Finding F, reproduced as geometry rather than as a favourable summary

The heavy challenge built `crevice_slab` from the 0.25 mm-pitch fine slab. It did
not add or rewire faces. It raised every top-grid vertex beyond one grid column by
2.0 mm. The result is a watertight, mesh-connected surface with:

* a horizontal seat sheet containing the seat at `(0, 0)`;
* a near-vertical opposite wall at `x = wall_x_mm`; and
* a continuous mesh path from the seat to that wall across the crevice bottom.

The arm is vertical, with **radius** `r_final = 1.000 mm`. Its creation radius is
`0.350 mm`. The final hard demand is `g = 0.300 mm` true surface-to-surface. Thus a
wall whose nearest point is less than `1.300 mm` from the axis is inside the final
refusal envelope.

**MEASURED** in
`.git/seal-landing-2026-08-23/chal-probes2.log`:

| one rule and one final arm | wall-axis distance, mm | wall faces creation → re-sized (**MEASURED**) | true wall gap, mm (**INFERRED from fixture coordinates**) | wall-only final clearance, mm (**INFERRED**) | aggregate verdict creation → re-sized (**MEASURED**) |
|---|---:|---:|---:|---:|---:|
| connected crevice | 0.75 | 11 → 23 / 64 | −0.25 penetration | −0.55 | −0.3000 → **+0.3008** |
| connected crevice | 1.00 | 0 → **20 / 64** | 0.00 | **−0.30** | −0.3000 → **+0.3008** |
| connected crevice | 1.25 | 0 → **15 / 64** | 0.25 | **−0.05** | −0.3000 → **+0.3008** |
| connected crevice | 1.50 | 0 → 11 / 64 | 0.50 | +0.20 | −0.3000 → **+0.3008** |
| connected crevice | 1.75 | 0 → 0 / 64 | 0.75 | +0.45 | −0.3000 → +0.3008 |
| connected crevice | 2.00 | 0 → 0 / 64 | 1.00 | +0.70 | −0.3000 → +0.3008 |

The aggregate `−0.3000 mm` before re-size is **not** a wall-only measurement. The
later bare control in `chal-probes3.log` shows `−1.3000 mm` because it also sees
the arm's own seat at zero centerline distance. Those controls show why some seat
exemption is mandatory; they do not identify the binding wall.

The wall-only column is arithmetic from the fixture, not another kernel run:
`clearance = wall_x − 1.000 − 0.300 mm`. At `x = 1.00 mm` the wall has `0.00 mm`
true surface-to-surface gap, and at `x = 1.25 mm` it has `0.25 mm`. Both are below
the final `0.300 mm` demand and must remain refusable. The measured facts that
make this more than hypothetical are that 20 and 15 of their wall faces entered
the exemption and the aggregate verdict became legal.

The `x = 1.75` and `2.00 mm` rows are the unfavourable population in the same
table: no wall face entered the patch and their wall-only clearances are legal,
yet other connected seat-sheet faces still moved the aggregate result to
`+0.3008 mm`. The required regression must therefore assert the wall's retained
patch and its wall-only clearance, not only the aggregate sign.

### 1.2 No radius for the current band can repair the rule

For an arm of radius `r` to be legal against its own locally planar seat under a
clearance demand `g`, the unchanged centerline query must ignore the seat sheet
through radius at least `r + g`. Otherwise an unignored surface point exists at a
centerline distance below `r + g`, and the arm refuses itself.

A foreign wall inside the same radius is also returned by the Euclidean band. If
it is mesh-connected to the seat through band faces, the flood fill admits it.
Thus:

```text
band large enough for self-legality  =>  contains the whole refusal disc
mesh-connected foreign face in disc  =>  may enter Z
```

Changing `STANDOFF_MM`, using `FINAL_CLEARANCE_GAP_MM`, or choosing an intermediate
band moves the defect. It does not remove it. The missing quantity is **surface
identity**, not a better radius.

### 1.3 The Giant Bat is the real acceptance shape

**MEASURED** by the mask-honesty audit and retained in
`honest-exemption-2026-08-22.md`:

* both investigated Bat refusals were opposite crevice walls, not the intended
  seat sheet;
* the closest faces were across 11–24 triangles and 133–151 degrees from the
  seat's own closest point;
* slot 250's final true surface-to-surface gap was approximately `0.291 mm`, below
  the final `0.300 mm` hard demand.

A correct rule does not promise that every Bat arm refuses. It promises that the
opposite wall remains in the tested surface. The one rule then applies the same
`0.300 mm` final demand: slot 250 should refuse by approximately `0.009 mm` if no
other face binds; any wall with more than `0.300 mm` true surface-to-surface gap
should remain legal.

---

## 2. Quantities already authorized by the contact physics

This design mints no geometric decision constant. It uses geometry that the
support already intends to emit.

For each designed embed end, define:

* `S`: the model boundary;
* `q`: the stored intended entry crossing on the stored seat face;
* `n`: the stored outward entry normal used by the seating construction;
* `A`: the live member axis;
* `E`: the final semantic embed-end solid, including the end ball and adjoining
  tapered shaft portion that the emitter will union into the model;
* `g`: the already-authorized clearance demand for the reader: `0.600 mm` at
  placement or `0.300 mm` at final grading.

`E` is class-specific and must come from one shared geometry authority:

* a contact stub uses its emitted `ContactSphere` plus its adjacent
  `ContactTaper`;
* an embedded model foot uses the emitted foot ball plus its adjacent member
  frustum;
* a seated model rung without a distinct retained ball uses the emitted member
  end that actually crosses the entry surface;
* if an end ball is certified redundant because the adjacent frusta enclose it,
  `E` is the same union solid, not a synthetic ball that emission removed.

For a ball-tipped end, `contact.rs` already derives the seat depth. With ball
factor `f` and shaft radius `r`, the minimum depth that leaves a separating cut no
narrower than the shaft is

```text
h = ball_seat_factor(f) * r
ball_seat_factor(f) = f - sqrt(f^2 - 1)
a_cut = ball_cut_radius_mm(r, h, f)
      = sqrt(2*f*r*h - h^2)
```

At that derived depth, `a_cut = r`. For feet, the same-day load-sized ruling uses
`f = BALL_FACTOR_FOOT = 1.4` and derives `h = 0.4202041 × r_foot`; for contact
stub balls the registered factor is `1.5`. These factors are emitted-geometry
inputs already in the registry. They are not new footprint allowances.

`SPEC-EMBED-POLICY.md` supplies two more constraints:

1. only the tip-anchored entry interval is sanctioned; an unanchored crossing or
   re-entry remains a collision; and
2. a deep seat is truncated at the far face along its own line. It never creates
   a backside patch.

Therefore `E` means the **sanctioned entry embed**. A lateral intersection with
another wall, a re-entry, or the far face is not silently reclassified as a second
part of the intended footprint. The known lateral spanning exposure remains open;
this footprint rule does not cite truncation as closing it.

**Contingency:** the truncation text is ahead of some implementation paths. The
footprint implementation cannot claim one shared emitted solid until the same
clamp authority feeds placement, grading, scar measurement, and emission. If
those paths disagree, construction must fail rather than select the most
forgiving solid.

---

## 3. The proposed rule [physical patch plus a derived same-entry-sheet clearance shadow]

### 3.1 Physical footprint `P0`

Let `Π` be the tangent plane through `q` with normal `n`. Let

```text
D0 = Π ∩ E
```

be the exact planar cut occupied by the final sanctioned embed solid. This is a
disc for a perpendicular circular cut and the exact ball/frustum union cut at
oblique incidence. Do not replace it with a circle of `r + constant`, an
area-equivalent circle, or a normal-angle threshold.

Lift `D0` onto the model by the entry construction:

1. through each point of `D0`, consider the line parallel to `n`;
2. select the model crossing on the entry sheet that is continuously connected to
   the stored seat crossing `q` in this projected arrangement; and
3. stop where that entry-sheet crossing ceases to exist or ceases to be unique.
   Do not walk around a model crease by triangle adjacency.

The resulting subset of `S` is `P0`, the **physical footprint patch**.

This is an exact topological selection, not a sampled normal test. Edge and vertex
ties need the kernel's existing deterministic lowest-face-ID tie-break only to
choose a representation; a tie must not enlarge the selected region.

### 3.2 Why `P0` alone cannot keep the seat legal

Consider the favourable case: a perpendicular cylindrical arm of radius `r`
through a planar seat. `P0` is a disc of radius `r`. Immediately outside its
boundary, retained seat points have centerline distance arbitrarily close to
`r`. The unchanged clearance is

```text
inf(dist(A, S \ P0)) - r - g = r - r - g = -g.
```

The arm therefore refuses itself for every positive `g`. This is geometry, not
mesh resolution. Enlarging the face set until it passes would hide the enlargement
inside tessellation and would restore the same structural defect on coarse faces.

### 3.3 Clearance shadow `Pg`

For a reader demanding true surface-to-surface gap `g`, form the exact clearance
envelope of the final embed solid:

```text
Eg = E Minkowski-summed with a closed sphere of radius g
Dg = Π ∩ Eg
```

Apply the **same entry-sheet lift** used for `P0` to `Dg`. The result is `Pg`.
Then grade

```text
clearance_g = min distance from the full live axis/member to (S \ Pg)
              minus the member radius and g,
```

using the existing tapered-member semantics where they apply.

The physical claim and the clearance claim remain separate:

* `P0` says what the designed end presses on or occupies;
* `Pg \ P0` is not reported as physical contact. It is the exact part of the same
  entry sheet that an attached joint necessarily places inside its requested
  clearance envelope;
* no second mesh sheet enters merely because it is close, connected, or similarly
  oriented.

This is a **contact-aware boundary condition**, not ownership of `Pg \ P0`: the
requested gap is suspended only against that entry sheet where it would condemn
the joint that creates the boundary condition. The same gap still applies to every
other surface. If the clearance kernel can represent this boundary condition
directly without materializing `Pg`, that is the same rule; it must return the same
clipped patch and crevice verdicts.

On a perpendicular planar seat, `Dg` reaches exactly `r + g`; the arm is legal at
algebraic zero margin. There is no tessellation-derived positive headroom to quote.
On the challenge crevice, the horizontal entry sheet is lifted from the disc, but
the near-vertical wall has no two-dimensional entry-sheet image under the stored
normal. Its faces stay in `S \ Pg`, even where the crease line touches the disc.

**This is one rule for both tiers.** Placement uses `g = 0.600 mm`; final uses
`g = 0.300 mm`. The patch identity and construction are identical. Only the
reader's already-authorized demand changes. Final grading must not reuse the
placement patch: that would grade the final population with the more forgiving
rule.

### 3.4 Exact patch boundaries are required; whole-face exclusion is not the rule

A triangle may cross the boundary of `Pg`. Excluding the whole face adds mesh-size
slack. Retaining the whole face reintroduces self-refusal. Therefore a literal
no-slack implementation must represent:

* faces fully inside `Pg`;
* faces fully outside `Pg`; and
* clipped boundary polygons on partially covered faces.

The clearance kernel must measure against the outside fragments of boundary
faces. `Arc<[u32]>` alone cannot represent the rule.

This is the main price increase from the current design. The existing
frozen-record, batch boundary, and immutable grading-row machinery remain useful,
but the record becomes a frozen patch with boundary clips rather than only a
sorted face-ID exclusion set.

---

## 4. Candidate rules, one table [every candidate sees the favourable slab and the unfavourable connected crevice]

| candidate | fine 0.25 mm slab, final radius 1.000 mm | connected wall at 1.00–1.25 mm from axis | batched cost shape | decisive failure or bracket |
|---|---|---|---|---|
| **Current Euclidean tube + mesh flood fill** | **MEASURED:** re-size grows zone 116 → 304 faces and keeps seat legal at +0.3008 mm final | **MEASURED:** swallows 20 and 15 wall faces; changes REFUSED → LEGAL | one batched near-face query; parallel CPU fills | rejects: proximity plus adjacency is not contact identity; no band size repairs it |
| **Surface geodesic disc from seat face** | can keep the same sheet local if its radius is expanded to the tier demand | lower wall is reached across the crevice bottom; at 1.00 mm it is inside a 1.300 mm final geodesic reach before any climb | candidate gathering can batch; shortest-path state is per row and mesh-scale | rejects as the deciding rule: actual contact has no physics-derived geodesic radius; using `r + g` repeats the refusal disc on the surface; triangulation and folds move the path |
| **Face-normal filter** | planar slab is favourable because all seat normals agree | the challenge wall at a sharp 90-degree turn looks separable | batched candidate query plus parallel dot products | rejects: a curved legitimate seat changes normals continuously; the Bat's opposite wall also faces the arm. Any allowed angle needs a new authority and turns tessellation noise into a verdict |
| **All model surface intersected by the final embed solid** | physical cut is selected without a distance allowance | usually keeps a separated wall, but a laterally spanning ball or shaft can intersect another wall and would then forgive an unanchored collision | batched solid/triangle broad phase plus exact intersections | rejects in this broad form: `S ∩ E` confuses the intended entry crossing with lateral overlap, re-entry, and the R4 far face |
| **Entry-sheet physical patch `P0` only** | identifies the right physical disc | wall remains visible | same exact patch work as recommendation, smaller domain | rejects alone: unchanged positive-gap full-axis grading self-refuses by exactly `g` at the footprint boundary |
| **Load-bearing area from PR-4 / load attestation** | proves the separating cut can carry the load | equal area can occur on the seat, an opposite wall, or several disconnected patches | scalar arithmetic after geometry measurement | rejects as identity: area sizes the required anchor; it does not say which surface the foot owns |
| **Recommended: final embed solid's entry sheet, expanded only by the active demand** | **INFERRED from exact planar geometry:** physical disc is `P0`; `Pg` reaches the required `r + g`, so the own seat is legal with zero invented margin | **INFERRED from fixture construction:** vertical wall has no two-dimensional lift on the horizontal entry sheet; its wall-only final clearances remain `−0.300 mm` at 1.00 and `−0.050 mm` at 1.25 | one batched broad phase and one batched clipped-patch/clearance query; parallel exact clipping per row | recommends, with two brackets: exact boundary fragments are required, and ambiguous/non-height-field entry projections must fail strict rather than jump sheets |

### 4.1 Why geodesic distance is not a hidden cheaper version

A geodesic disc answers "how far can I walk on the mesh from the seat?" The
physical question is "which surface did this designed solid enter here?" They
coincide on a broad plane and diverge at the first crease, which is exactly the
challenge geometry. At `x = 1.00 mm`, the bottom edge of the wall is one millimetre
of surface travel from the seat. Any geodesic radius large enough to give a
1.000 mm arm its final 0.300 mm self-clearance reaches that edge and some wall
area. A smaller radius restores self-refusal. No new geodesic number fixes that
trade.

Geodesic distance can remain a broad-phase pruning tool if its bound is derived
from `Dg` and cannot discard a true patch point. It must not decide patch identity.

### 4.2 Why normals are evidence, not a predicate

The Bat's measured 133–151 degree change supports the diagnosis that its nearest
faces were foreign. It does not authorize an angle. A round body can rotate its
normal through the same range while remaining one entry patch; a faceted body can
jump at a harmless edge. The challenge's 90-degree wall is deliberately easy.
Passing it with an angle test would not establish the production rule.

Normals are valid inputs to the exact entry projection because `n` is already the
seating direction. Comparing every candidate face normal to `n` and accepting an
angle band is not.

### 4.3 Why load area remains necessary but cannot select the patch

The same-day ruling makes foot radius and depth load-sized. The area attestation
ensures the cut carries `carried_mm2`; at the derived minimum ball seat the cut
radius equals the shaft radius. That supplies `E`'s dimensions. It does not supply
a surface label. Inflating an exemption until its area reaches the attested area
would be circular: the exemption would choose whichever nearby faces make the
area convenient.

Use the attestation to build the solid. Use the entry crossing to identify its
surface patch. Do not use attested area as an exemption quota.

---

## 5. Lifecycle and batching [the frozen-Z architecture survives, but radius-only invalidation does not]

### 5.1 Creation freeze

At contact creation, store one immutable `FrozenEntryFootprint` per designed
embed end. Conceptually it contains:

* stored seat face ID, exact entry point `q`, and entry normal `n`;
* the semantic embed-solid class and all geometry inputs needed to reproduce the
  emitted union;
* the physical patch `P0`;
* tier patches or exact patch data from which `Pg` is evaluated without changing
  identity during grading; and
* an exact geometry fingerprint.

The footprint constructor and lowering/emission must consume the same semantic
contact geometry object. Copying the formulas into `seat_zone.rs` would create a
second emitter and allow a legal-but-not-emitted patch.

### 5.2 Re-cut after every contact-geometry mutation

The re-cut remains between mutation and grading, never inside grading. Its dirty
predicate must cover every input that can move `E`, `Π`, or the entry lift:

```text
(p0, p1, contact-end radius, far-end radius where taper changes the cut,
 contact end, seat point, seat normal, seat face, ball/frustum class,
 ball factor, seat depth, truncation interval, active geometry authority)
```

Exact bit comparison is appropriate for values written and later re-read by the
same authority. A tolerance would add an unpriced region in which a stale patch
remains accepted.

A radius-only dirty gate is already refuted. **MEASURED** in retained challenge
logs:

* CHAL-A moved an axis without changing radius: `refrozen=0`, stored 116 faces,
  fresh 140 faces, 24 missing from the stored patch;
* CHAL-A2 shortened an axis without changing radius: `refrozen=0`, stored 155
  faces, correct 139 faces, **16 over-forgiven**.

The footprint rule is more axis-sensitive than the tube, not less. Reusing the
radius-only gate would knowingly carry both strict and forgiving stale outcomes.

### 5.3 One immutable grading token

After the re-cut, construct clearance rows that own or reference the frozen patch.
The grading API should accept an immutable prepared token, not a live radius plus
permission to discover a patch. This makes the prohibited operation—recomputing
an exclusion from the radius being graded—unrepresentable inside the grading
call.

Candidate evaluation may prepare many rows, but all rows for one batch are
prepared before the native clearance call. A candidate replay must either carry
the exact prepared winner patch or recreate the same semantic geometry and prove
its patch digest equal before grading.

### 5.4 Cost shape and limits

The current cheap operation is one near-segment batch plus face-ID fills. The
recommended operation is more expensive:

1. build all final semantic embed-end solids for the row batch;
2. broad-phase all `Eg` against the model in one native batch;
3. construct entry-sheet arrangements and clip boundary triangles in parallel;
4. issue one batched clearance query over all prepared rows and their frozen patch
   fragments; and
5. retain patch digests and per-stage counts for audit.

There must be no scalar native call per contact. Work arrays use CSR-style row
offsets so rows remain deterministic and batchable.

Limits may use the exact row count, mesh face count, and checked byte arithmetic.
They may refuse for resources before producing an outcome, but may not truncate a
patch or select a different sheet. A partial patch can move the verdict in either
direction. No new fixed face, fragment, path-length, angle, or byte number is
authorized by this memo.

**Price disclosure:** native geometry work, memory, record size, and test scope all
go up. The old `Arc<[u32]>` representation and flood fill are not enough. This
lane has not run a model or a benchmark, so it does not attach a favourable time
estimate. The first implementation proof must measure candidate faces, boundary
fragments, peak bytes, and native batch time on the fine slab, connected crevice,
and at least one corpus model that actually places a model anchor.

---

## 6. Narrowed monotonicity, proved by construction [within one frozen patch; not across re-cuts]

Craig narrowed the guarantee to:

> Against any surface outside its own footprint patch, a thinner arm never grades
> worse.

For one prepared grading row, fix:

* the axis and tapered-shape station set;
* the active demand `g`;
* the frozen entry patch `Pg`; and
* therefore the tested surface `T = S \ Pg`.

For the uniform-radius form, let

```text
C = min over the full axis of dist(axis station, T).
clearance(r) = C - r - g.
```

`C` does not depend on the radius being compared because the patch is already
frozen and the grading API cannot recompute it. For `r_thin <= r_fat`,

```text
clearance(r_thin) - clearance(r_fat) = r_fat - r_thin >= 0.
```

Therefore a thinner arm cannot grade worse against any retained surface point.
The same argument holds for a tapered member when every compared local radius is
no larger at every station: the distance field and `Pg` stay fixed while the
subtracted support envelope only shrinks.

This proves exactly the narrowed guarantee. It deliberately does **not** compare:

* two different axes;
* placement `P0.600` with final `P0.300`;
* a creation patch with a post-sizing patch; or
* two separately prepared final solids whose physical footprints differ.

Across a re-cut, both the tested surface and support radius can change, so either
clearance may be larger. The measured fine-slab ladder already shows a fatter arm
grading better across separate re-cuts (`+0.318 mm` at radius `0.50 mm`, then
`+0.325 mm` at radius `0.65 mm`). This memo does not relabel that as monotone.

The construction requirement is stronger than a comment: `Pg` must be absent from
the grading function's mutable inputs. A sabotage that substitutes a live
radius-dependent patch builder inside grading must fail an API/compile-time guard
or a test that compares two radii against one prepared token.

---

## 7. Failure policy and measurements that can refute the design

| case | one disposition | refuting measurement or required witness |
|---|---|---|
| physical footprint has no entry-sheet area containing `q` | exempt nothing and refuse the contact; do not jump to a neighbour | a valid emitted embed solid and stored entry crossing produce a non-empty exact `P0` |
| projection has two exact entry-sheet continuations | fail strict as ambiguous; no angle or distance tie-break may enlarge the patch | exact arrangement shows one branch is the unique continuation of the stored seat face |
| patch reaches the far face | far face stays tested; R4 permits no backside patch | a prepared patch contains any face belonging only to the far crossing |
| embed solid laterally intersects an opposite wall | wall stays tested unless it is the stored entry-sheet crossing; report the existing lateral-spanning exposure separately | wall-only clearance becomes absent because `S ∩ E` was used without entry identity |
| triangle crosses patch boundary | retain and grade its outside fragment | replacing clipped patches with whole face IDs leaves every acceptance verdict unchanged on a deliberately coarse boundary triangle |
| axis changes with identical radius | exact fingerprint changes and re-cut count is non-zero | reproduce CHAL-A or CHAL-A2 with `refrozen=0` |
| tier changes from placement to final | rebuild/select the same-rule patch for the new `g`; do not reuse placement patch | final row carries a `0.600 mm` patch digest or excludes any point in `P0.600 \ P0.300` |
| geometry authority differs from emission | fail before grading | footprint-solid digest differs from the lowered embed primitive digest |
| resource limit binds | refuse the batch as a resource outcome; never truncate | fragment count reaches a cap and a partial patch is nevertheless graded |

No headline says these cases cannot happen. Each has a witness that would disprove
the claimed construction.

---

## 8. Acceptance estate [the rule must pass all populations without changing predicates]

The implementation is not accepted by a legal seat alone or by a refused crevice
alone. Each fixture carries the favourable and unfavourable cases under the same
constructor and grader.

1. **Fine slab, physical patch.** On 0.25 mm triangles, show `P0`'s exact area and
   boundary clips. The physical cut must agree with the class-specific emitted
   solid. Do not substitute `r + g` and call it contact area.
2. **Fine slab, two demands.** The same row constructor with `g = 0.600 mm` and
   `g = 0.300 mm` keeps the own seat legal. It reports algebraic/tessellated
   clearance without claiming a positive structural margin.
3. **Connected crevice ladder.** Reuse the challenger's unchanged watertight
   fixture at wall-axis distances 0.75, 1.00, 1.25, 1.50, 1.75, and 2.00 mm. At
   every row, assert no two-dimensional opposite-wall patch is exempt. Then assert
   the wall-only final clearances from one rule: approximately −0.550, −0.300,
   −0.050, +0.200, +0.450, and +0.700 mm for radius 1.000 mm and final demand
   0.300 mm, subject only to exact fixture tessellation.
4. **Coarse boundary triangle.** One triangle straddles `Pg`. Its inside polygon
   is exempt and outside polygon remains measured. The whole-face forgiving and
   whole-face strict sabotages must each move a witness.
5. **Curved entry sheet.** A legitimate smooth/faceted seat whose normals rotate
   through the range that would defeat a normal threshold remains one projected
   entry patch without an angle constant.
6. **Folded foreign sheet.** A nearby, similarly oriented sheet inside the
   Euclidean envelope but not on the stored entry continuation remains tested.
7. **R4 far face and lateral overlap.** A truncated deep seat has one entry patch,
   no far patch, and keeps any lateral opposite-wall overlap refusable.
8. **Geometry mutation.** Radius growth, radius shrink, axis translation, axis
   shortening, contact-end move, and taper change each re-cut when and only when
   the exact geometry fingerprint changes. CHAL-A and CHAL-A2 are mandatory.
9. **Frozen monotonicity.** Two or more radii grade through the same immutable
   prepared patch token; thinner never reads lower. A named sabotage that rebuilds
   the patch from each graded radius must fail.
10. **Batch sabotage.** Replace the row batch with a scalar native call loop; a
    test or instrumented counter must detect more than one native call at the
    declared batch boundary.
11. **Giant Bat evidence.** Re-measure the two known crevice rows. Assert that the
    nearest opposite-wall faces remain in the tested surface, then let the common
    `0.300 mm` final rule decide them. Do not make "Bat emits" an acceptance
    criterion.
12. **Corpus cost witness.** Use a model that places a model anchor. The prior
    `zombie-3-a` run placed none, so byte identity there cannot price this path.

Required red runs remove entry-sheet identity, replace clipped patches with whole
faces, skip an axis-only re-cut, and recompute a patch inside grading. Each must
fail first on the claim it sabotages, not on fixture setup.

---

## 9. What this does not authorize

* no change to the `0.600 mm` placement margin or `0.300 mm` final hard demand;
* no geodesic radius, normal angle, curvature allowance, face count, fragment
  count, or geometry tolerance;
* no claim that load-bearing area identifies the owned surface;
* no whole-face approximation presented as "no slack";
* no backside footprint contrary to R4;
* no claim that R4 closes lateral spanning;
* no grading-time patch recomputation;
* no per-contact native kernel loop; and
* no re-baseline of model outcomes before the required geometry witnesses explain
  each moved verdict.

---

# DECISION MEMO — for Craig

## The dilemma

The re-sized seat zone keeps an arm legal, but it does so by forgiving every
mesh-connected face inside a distance tube. On the measured crevice fixture, a
1.000 mm-radius arm grew its zone from 116 to 304 faces. At walls 1.00 and
1.25 mm from its axis, the zone swallowed 20 and 15 wall faces and turned a real
final refusal into a legal result. No smaller distance band fixes this: the band
has to cover the arm's own refusal disc or the arm refuses its own seat.

A literal physical contact patch has the opposite problem. It identifies the
right seat, but the existing positive-gap full-axis query refuses at the edge of
that patch by exactly the demanded gap. We cannot honestly call a larger disc
"what the foot touches."

## The candidates side by side

* **Distance tube plus mesh connection:** cheapest and already built; measured
  wrong on connected crevices.
* **Geodesic disc:** stays on the surface, but walks across the crevice bottom to
  the opposite wall. It still needs a radius that trades self-refusal against
  over-forgiveness.
* **Face normals:** separates the easy square fixture, but needs a new angle and
  fails on curved seats. The Bat's opposite wall also faces the arm, so opposition
  is not ownership.
* **Any surface touched by the embed solid:** physically motivated, but too broad;
  it would forgive lateral collisions, re-entry, or a far face.
* **Load-bearing area:** tells us how large and deep the anchor must be. It does
  not tell us which model surface belongs to that anchor.
* **Entry-sheet embed patch:** uses the final emitted ball/frustum and the stored
  entry crossing. It follows the sheet the foot actually entered, not mesh
  adjacency around a crease. The physical patch records contact. A second patch,
  derived from the same sheet and the already-authorized clearance demand, handles
  the unavoidable self-clearance transition.

## Recommendation [more geometry work than the current branch]

Authorize the **entry-sheet embed patch** rule:

1. build the footprint from the final semantic embed-end solid that emission uses;
2. select only the entry sheet connected to the stored seat crossing under the
   seating direction—never an adjacency flood fill;
3. keep the physical patch separate from its `0.300 mm` or `0.600 mm`
   same-entry-sheet clearance shadow;
4. freeze the exact clipped patch before grading and re-cut after any contact
   geometry change, including an axis-only change; and
5. grade every other surface, including an opposite crevice wall, by the existing
   one final or placement rule.

The reason is direct: it is the only candidate here that answers the physical
question "which surface did this designed foot enter?" and also explains how an
attached arm can satisfy a positive clearance demand without granting ownership
to a nearby wall. It adds no decision number. With a frozen patch, the narrowed
monotonicity proof is mechanical: the tested surface is fixed, so reducing the
arm radius cannot reduce clearance.

## What I ask you to authorize

Please authorize the rule and its disclosed price increase:

* exact entry-sheet patch construction shared with emitted contact geometry;
* clipped boundary polygons, not only face IDs;
* a batched patch-aware clearance query;
* tier-specific clearance shadows from the existing `0.600 mm` and `0.300 mm`
  demands; and
* invalidation on every contact-geometry change, not only radius changes.

This does **not** ask you for a new constant or to move either clearance demand.
If you want the exemption to be the literal physical patch with no clearance
shadow at all, then the full-axis positive-gap clearance semantics must change in
a different way; the planar proof shows that the current query cannot keep such a
seat legal.
