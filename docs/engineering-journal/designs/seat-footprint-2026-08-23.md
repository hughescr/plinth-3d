# Seat footprint: exempt the declared entry carrier, not nearby geometry

**Date:** 2026-08-23; revised 2026-08-24 after round-1 review

**Status:** REVISED DESIGN FOR OWNER DECISION; no production code in this lane

**Branch:** `feature/a4-controller`
**Authority:** Craig's 2026-08-23 ruling that the exemption covers what the foot
actually presses on or occupies, with no distance-connected slack; Craig's narrowed
monotonicity ruling; and the designed-embed rulings in `SPEC-EMBED-POLICY.md` and
`land-at-strike-2026-08-22.md`.

> **Recommendation [adds one non-numeric semantic input; the current seat face,
> axis, and mesh are not enough].** Authorize an immutable **entry carrier** for
> each designed embed. The contact constructor must declare which local model
> surface is the intended receiving surface. The physical footprint is the
> entry-anchored component of the exact intersection of that carrier with the
> exact emitted embed solid. For a demand `g`, the clearance patch is the
> entry-anchored component of the carrier's exact intersection with the exact
> refusal envelope used by that same clearance query. All model surface outside
> that patch remains graded.
>
> This is not a normal angle, geodesic radius, distance allowance, or face flood.
> It is surface identity that the current record does not contain. Without that
> identity, the raised-column and channel fixtures require contradictory
> continuations from the same geometric inputs. The previous “entry-sheet lift”
> is withdrawn: it climbs the raised wall, cannot include the channel floor
> without also admitting a bridge, and does not equal the physical intersection
> on curved seats.
>
> **Authorization bracket [material].** If adding an entry-carrier semantic is not
> acceptable, do not implement a footprint exemption from the current inputs.
> No repaired distance, projection, normal, or adjacency rule in this memo can
> establish both required outcomes without a new decision predicate.

---

## 1. The measured defect [the current distance-connected rule forgives a real crevice refusal]

The landed candidate on `feature/f1-refreeze` freezes the mesh-connected component
containing the stored seat face of all model faces within `contact radius + 0.600 mm`
of the live arm axis. Re-cutting after sizing repairs stale radius, but not the set
predicate. Mesh adjacency says that triangles belong to one object. It does not say
that one support foot owns both triangles.

The heavy challenge used a watertight 0.250 mm-pitch slab. It raised the vertices
beyond one grid column by 2.000 mm. The arm had creation **radius** `0.350 mm` and
final **radius** `1.000 mm`. The final demanded true gap was `0.300 mm`.

**MEASURED** in `.git/seal-landing-2026-08-23/chal-probes2.log`:

| one final arm | wall-axis distance, mm | wall faces creation → final | wall-only final clearance, mm (**INFERRED**) |
|---|---:|---:|---:|
| connected crevice | 0.75 | 11 → 23 | −0.550 |
| connected crevice | 1.00 | 0 → **20** | **−0.300** |
| connected crevice | 1.25 | 0 → **15** | **−0.050** |
| connected crevice | 1.50 | 0 → 11 | +0.200 |
| connected crevice | 1.75 | 0 → 0 | +0.450 |
| connected crevice | 2.00 | 0 → 0 | +0.700 |

The wall-only arithmetic is `wall_x − 1.000 − 0.300 mm`. At 1.00 and 1.25 mm,
the wall must remain refusable. The re-sized zone instead admitted 20 and 15 wall
faces and changed the aggregate result from refused to legal.

No smaller distance band repairs the rule. A planar seat needs its own refusal disc
excluded through **radius** `r + g`. A foreign wall inside that disc is also inside
any Euclidean band large enough to keep the seat legal. Connectivity through the
band is the only remaining separator, and the challenge and committed `channel()`
fixture both show that a real connected path can lie inside it.

The Giant Bat is the production acceptance shape. **MEASURED** in
`honest-exemption-2026-08-22.md`, both investigated refusals were opposite crevice
walls. Slot 250 had approximately `0.291 mm` true gap, below the final `0.300 mm`
demand. A correct rule does not promise that the Bat emits. It promises that the
opposite wall stays in the tested surface and lets the common demand decide.

---

## 2. Why the previous entry-sheet rule is rejected [all three geometry defects are blocking]

### 2.1 The old shadow and grader used different envelopes [the claimed monotonicity was false]

The previous memo formed a shadow from the emitted foot ball, then graded by
subtracting only member radius `r`. For ball factor `f > 1` and demanded gap `g`,
the retained planar boundary is

```text
d = sqrt(r² + 2 f r g + g²).
```

The reported clearance was therefore

```text
d - r - g > 0,
```

not zero. **INFERRED** with `f = 1.4` and final `g = 0.300 mm`, the margin rises
from `0.06169 mm` at **radius** `0.350 mm` to `0.08924 mm` at **radius**
`1.000 mm`. The fatter arm grades `0.02756 mm` better. At placement
`g = 0.600 mm`, it rises from `0.08465 mm` to `0.14356 mm`.

The seat remains legal, but the narrowed monotonicity claim does not. The repair is
normative: one geometry authority must produce both the refusal envelope and its
patch. A ball-derived shadow cannot be paired with a cylinder-radius subtraction.

### 2.2 A projected lift climbs one crevice and cannot own the other correctly

The challenge wall is not vertical. It rises `2.000 mm` over one `0.250 mm` grid
column. Its **INFERRED** angle is `atan(2/0.25) = 82.87 degrees`. Under the slab
normal it is a single-valued height field with a unique crossing connected to the
seat. The old continuation rule therefore climbs it.

For final **radius** `1.000 mm`, `f = 1.4`, and `g = 0.300 mm`, the old ball shadow
reaches `sqrt(1.93) = 1.389 mm`. It reaches both binding wall positions at
1.000 and 1.250 mm. The wall-only clearances remain `−0.300 mm` and `−0.050 mm`,
but the lift can exempt their nearest strips. The old claim that the wall has no
two-dimensional entry-sheet image does not apply to this fixture.

The committed `channel()` fixture exposes the opposite error. Its bottom case has
a **radius** `0.400 mm` arm only `0.300 mm` above the floor. The member cylinder
occupies the floor, and the factor-1.4 foot ball has **radius** `0.560 mm`.
**MEASURED** in the committed test, the connected band contains 13 near-wall,
16 floor, and 11 far-wall faces. A rule that forbids crossing the near-wall/floor
crease omits a legitimate occupied floor patch. A rule that relaxes continuation
across that crease can bridge to the far wall.

These fixtures show that “unique projection,” “do not cross a crease,” and mesh
connectivity are not surface-ownership rules. They cannot decide both required
outcomes from the present inputs.

### 2.3 A lifted planar stencil is not the physical footprint on a curved seat

Let a ball have **radius** `R = f r` and center distance
`c = sqrt(f² − 1) r` below the tangent plane. On a curved height field
`z = delta(rho)`, actual occupation is

```text
rho² + (delta(rho) + c)² <= R².
```

The old lift admitted every lifted point with `rho <= r` without retesting the
solid intersection. One curvature sign over-exempts points outside the ball. The
other omits points the ball occupies and restores self-refusal. Smooth, unique
height fields exhibit both failures, so projection continuity does not repair it.

The same defect applies to a clearance patch. The rule must select an exact
entry-identified component of `S ∩ E0` and `S ∩ Qg`, not lift `Pi ∩ E0` or
`Pi ∩ Qg` onto `S`.

---

## 3. Exact geometry objects [endpoints and truncation order are normative]

For one designed embed, define:

* `S`: the exact model boundary used by placement and grading;
* `q`: the stored intended entry crossing on the stored seat face;
* `n`: the stored outward entry normal;
* `A`: the live member axis and its exact tapered station data;
* `Iembed`: the closed, tip-anchored sanctioned embed interval;
* `Kentry`: the immutable declared entry carrier on `S`;
* `E0`: the exact closed semantic embed solid emitted inside `Iembed`; and
* `Qg`: the exact closed refusal envelope used by the active grader for the
  already-authorized demand `g`.

### 3.1 `Iembed` and `E0` have one shared authority

`Iembed` starts at the designed terminal end recorded by the contact constructor
and ends at the stored intended entry crossing along the contact axis. If the R4
far-face rule truncates the seat first, the truncated terminal endpoint replaces
the untruncated endpoint. There is no interval after the entry crossing and no
interval after the far-face truncation.

Construction order is fixed:

1. resolve the final live axis, contact end, endpoint radii, taper, ball/frustum
   class, ball factor, and derived seat depth;
2. apply the tip anchor and R4 far-face truncation to obtain `Iembed`;
3. clip every class-specific end primitive to that sanctioned interval with the
   emitter's exact end-cap semantics; and
4. union those clipped primitives to form `E0`.

A contact stub uses its emitted `ContactSphere` and clipped adjacent
`ContactTaper`. An embedded model foot uses its emitted foot ball and clipped
adjacent member frustum. A seated rung without a retained ball uses the emitted
member end that crosses `q`. If a ball is removed because adjacent frusta enclose
it, `E0` uses the emitted union and does not restore a synthetic ball.

“Adjoining shaft” never means an implementation-selected length. The adjoining
portion is exactly the part of the named emitted primitive whose station lies in
`Iembed`, after truncation and before union. Placement, footprint construction,
grading, scar measurement, and emission must consume the same immutable semantic
geometry object and geometry fingerprint. If any path disagrees, construction
fails before grading.

### 3.2 `Qg` is defined by the grader, not approximated by a convenient dilation

For the active demand `g`, define `Qg` by observable query semantics:

```text
x is in Qg  <=>  a surface consisting only of point x would be refused
                  by the exact live-member clearance query at demand g.
```

This definition includes the grader's exact cylinder, taper, endpoint, cap, and
station semantics. If the implementation grades a closed support solid by a
Minkowski sum with a sphere of **radius** `g`, that exact sum is `Qg`. If the
implementation subtracts local member radius from an axis distance, `Qg` is the
corresponding swept local-radius envelope. The two forms may be identified only
when proved equal.

The footprint constructor and the clearance kernel must receive the same prepared
`Qg` token. Rebuilding a ball shadow in one path and subtracting shaft radius in
another is prohibited.

### 3.3 `Kentry` supplies surface identity, not clearance slack

`Kentry` is a clipped surface carrier declared by the contact construction before
clearance grading. It answers only: “which local boundary is the designed receiver
for this embed?” It does not answer how far the exemption reaches.

The carrier contract is:

1. `q` lies in its closure and identifies its entry component;
2. it includes every two-dimensional part of `S ∩ E0` that the designed foot is
   intended to press on or occupy, including a legitimate channel floor patch;
3. it excludes an opposite wall, lateral collision, unanchored re-entry, and R4
   far face even when those surfaces are close, mesh-connected, or uniquely
   projected under `n`;
4. its boundary is exact clipped geometry, not whole face IDs;
5. it is independent of `g` and is not grown from arm radius, a distance band,
   geodesic distance, normal angle, or a face count; and
6. it is frozen at creation and recreated only by the shared contact constructor
   after a contact-geometry mutation.

This is additional semantic data. It cannot be reconstructed honestly from only a
seat face, axis, and unlabelled triangle adjacency: the raised-column and channel
cases refute the proposed reconstructions. The implementation proof must show how
the placement/contact constructor declares it. Merely renaming a flood fill
`Kentry` does not satisfy the contract.

---

## 4. The revised footprint rule [actual intersections only]

Form the exact arrangements of `S`, `Kentry`, `E0`, and `Qg`.

The physical footprint is

```text
P0 = the entry-anchored connected component containing q of
     S ∩ Kentry ∩ E0.
```

Connectivity is over exact clipped intersection cells. A zero-area point or edge
tie does not admit a neighboring two-dimensional cell. `P0` therefore records only
surface actually occupied by the emitted semantic solid. It is not a tangent-plane
stencil.

For the active demand `g`, the clearance patch is

```text
Pg = the entry-anchored connected component containing P0 of
     S ∩ Kentry ∩ Qg.
```

Then grade the exact outside fragments:

```text
Tg = S \ Pg
clearance_g = exact clearance of the prepared live member against Tg at demand g.
```

`P0` is the physical scar. `Pg \ P0` is not called contact area. It is the part of
the declared receiving carrier that lies inside the exact refusal envelope of the
attached joint. Every surface outside `Kentry`, including an opposite crevice wall,
remains in `Tg` even when it lies in `Qg`.

Placement uses `g = 0.600 mm`. Final grading uses `g = 0.300 mm`. They use the same
`Kentry`, `E0`, component identity, and query semantics, but distinct prepared
`Qg` and `Pg` records. Final grading may not reuse the placement record.

A triangle crossing either boundary is split. Inside polygons are excluded and
outside polygons remain measured. Whole-face exclusion adds mesh-size slack;
whole-face retention restores self-refusal. Sorted face IDs alone cannot represent
this rule.

---

## 5. Outcomes on the two required crevices [both directions are acceptance gates]

On the raised-column fixture, the intended carrier is the slab receiving surface.
The raised opposite wall is not part of `Kentry`. Its nearest fragments therefore
remain in `Tg` even if `Qg` reaches them. For final **radius** `1.000 mm` and
`g = 0.300 mm`, the expected wall-only clearances remain **INFERRED**
`−0.300 mm` at 1.000 mm and `−0.050 mm` at 1.250 mm. The test must measure that no
wall polygon is excluded; aggregate sign alone is insufficient.

On the committed channel bottom fixture, `P0` must contain the exact near-wall and
floor portions occupied by `E0`. The far wall is not part of `Kentry` and stays in
`Tg`. This avoids the false choice in the old projection rule: crossing the
near-wall/floor crease is legal because actual `S ∩ E0` and the declared carrier
include the floor; continuing to the far wall is illegal because carrier identity,
not adjacency, stops it.

These results are requirements on carrier construction, not assumptions hidden in
the footprint query. A constructor that cannot declare both carriers from its
contact semantics must refuse the contact. It must not choose the most forgiving
surface after seeing clearance.

---

## 6. Narrowed monotonicity [the query envelope and patch must share one family]

Craig's narrowed guarantee is:

> Against any surface outside its own footprint patch, a thinner arm never grades
> worse.

Let `Q(r, g)` be the exact refusal envelope prepared by the grader for member
**radius** `r` and demand `g`. For equal axis, taper class, station parameterization,
contact carrier, and demand, the geometry family must prove

```text
r_thin <= r_fat  =>  Q(r_thin, g) subset-of Q(r_fat, g).
```

For any fixed retained surface point outside the compared footprint patches,
shrinking that exact envelope cannot change a legal point into a refused point.
The aggregate minimum over the common retained surface therefore cannot get worse.
No separate foot-ball envelope may alter the tested boundary.

The guarantee is pinned in two ways:

1. **Common-patch comparison.** Two radii grade against one immutable prepared
   carrier/patch token. The thinner exact envelope must not report a lower
   clearance.
2. **Own-patch planar ladder.** Each radius prepares its own exact `Qg` and `Pg`
   using the same geometry family. A perpendicular planar seat must report the
   same algebraic boundary margin for every radius. The former
   `0.06169 → 0.08924 mm` fatter-grades-better span is a mandatory red witness.

The claim does not compare different axes, different carriers, placement with
final, or two different taper families. A surface excluded by one patch and tested
by another has no pairwise grade; reports must not present that case as a
monotonicity result. Candidate ranking must compare the common retained population
or disclose the population change.

---

## 7. Lifecycle and production wiring [every creator, mutator, copier, and consumer is registered]

Store one immutable `PreparedEntryFootprint` containing `q`, `n`, contact end,
`Iembed`, the semantic geometry object and fingerprint, `Kentry`, `P0`, tier,
prepared `Qg`, `Pg`, exact boundary clips, and a digest.

The dirty fingerprint covers every input that can move `E0`, `Qg`, or `Kentry`:
axis endpoints, contact-end and far-end **radii**, taper stations, contact end, seat
point, seat normal, seat face, semantic class, ball factor, seat depth, truncation
endpoints, carrier identity, tier, and geometry-authority version. Exact bit
comparison is required for values written and reread by the same authority.

CHAL-A and CHAL-A2 are mandatory witnesses. **MEASURED**, an axis translation at
unchanged **radius** left 24 faces missing from a stored zone, and an axis shortening
left 16 faces over-forgiven. Radius-only invalidation is prohibited.

Production wiring is enumerated by construction through one registry or typestate
transition, not by a prose search for call sites. The registry must account for:

* initial contact/stub/foot creation;
* CCO candidate preparation, commit, and commit-copy;
* brace radius growth, shrink, endpoint, taper, and commit paths;
* rewire slide, retire, root-reanchor, and replacement paths;
* candidate replay and winner copy;
* placement and final tier preparation; and
* every clearance consumer.

Each registered path gets a named red sabotage that disables only that path's
prepare, invalidate, copy, or consume transition and then exercises the production
path. CCO commit/copy, brace mutation, and rewire paths are mandatory, not examples.
A helper-level test does not count.

Every consumer recomputes the live geometry fingerprint and rejects a stale token
before native grading. A sabotage that passes a stale but well-formed token must
fail. Copy operations must either copy an exact digest-matching immutable token or
recreate it and prove digest equality. A new unregistered call site must fail a
compile-time exhaustiveness check or a registry coverage test.

---

## 8. Batch and cost shape [more geometry work than the rejected branch]

The path remains batch-first:

1. build semantic geometry and carrier requests for all rows;
2. prepare all exact `Qg` envelopes in one geometry batch;
3. broad-phase all carrier/envelope intersections in one native batch;
4. clip exact intersection cells in parallel per row into CSR-style storage; and
5. issue one native patch-aware clearance batch.

There is no scalar native call per contact. A counter sabotage that replaces the
row batch with a scalar loop must detect more than one call at the declared batch
boundary.

Checked limits may use row count, face count, intersection-cell count, and bytes.
A bound may refuse the entire batch as a resource outcome. It may not truncate a
patch, drop boundary polygons, or switch carriers. No new fixed face, fragment,
path-length, angle, curvature, tolerance, or byte number is authorized here.

The price increase is exact Boolean/intersection work, clipped-boundary storage,
carrier storage, larger fingerprints, production-path tests, and a patch-aware
clearance kernel. The first implementation proof must measure candidate cells,
boundary fragments, peak bytes, and native batch time on the fine slab, both
crevice fixtures, the Giant Bat rows, and a corpus model that actually places a
model anchor.

---

## 9. Acceptance estate [one constructor and grader must pass favourable and unfavourable populations]

1. **Exact curved physical intersection.** On convex and concave smooth/faceted
   seats, compare `P0` with exact `S ∩ Kentry ∩ E0`. A tangent-plane stencil must
   fail both curvature signs.
2. **Matched planar envelope.** At `g = 0.600 mm` and `g = 0.300 mm`, all CHAL-B
   radii keep their own planar seat legal. The old radius-dependent positive
   margins must be absent.
3. **Raised-column ladder.** Reuse wall-axis distances 0.75, 1.00, 1.25, 1.50,
   1.75, and 2.00 mm. No opposite-wall polygon enters `Pg`. Measure wall-only final
   clearances approximately −0.550, −0.300, −0.050, +0.200, +0.450, and +0.700 mm.
4. **Committed channel, both heights.** The upper arm keeps its intended near-wall
   patch without a far-wall patch. The bottom arm includes the physically occupied
   floor patch but keeps all 11 pinned far-wall faces tested.
5. **Coarse boundary triangle.** Inside polygons are excluded and outside polygons
   remain measured. Whole-face forgiving and whole-face strict sabotages each move
   a witness.
6. **Folded and smooth foreign sheets.** A nearby sheet, including a smooth
   connected crevice wall, stays tested when it is not in `Kentry`. Projection,
   normal, and adjacency substitutes must fail named witnesses.
7. **R4 and lateral overlap.** A truncated deep seat has no far patch. A lateral
   intersection outside `Kentry` remains refusable.
8. **Geometry mutation.** Radius growth, radius shrink, axis translation, axis
   shortening, endpoint move, taper change, carrier change, and tier change each
   invalidate exactly when the fingerprint changes. CHAL-A and CHAL-A2 are
   mandatory.
9. **Monotonicity.** Common-patch and own-patch planar ladders pass. Restoring the
   ball-shadow/member-radius mismatch must fail.
10. **Production-site sabotage.** Every registry entry in section 7 has one named
    red. Disable all live prepare/re-cut sites and the library or integration
    estate must not remain green.
11. **Stale-token sabotage.** A caller passes a valid token prepared before an
    axis-only mutation. The consumer rejects it before grading.
12. **Batch sabotage.** Scalar native calls are detected at the declared boundary.
13. **Giant Bat.** The two known opposite-wall populations remain tested; the common
    final `0.300 mm` rule decides them. “Bat emits” is not the criterion.
14. **Corpus cost witness.** Use a model that places a model anchor; `zombie-3-a`
    placed none and cannot price this path.

Every required sabotage must fail first on the claim it attacks, not on fixture
setup.

---

## 10. What this does not authorize

* no change to the `0.600 mm` placement margin or `0.300 mm` final hard demand;
* no distance-connected, geodesic, projection-only, or face-normal substitute for
  carrier identity;
* no new angle, curvature, distance, face-count, or tolerance decision number;
* no undefined “adjoining shaft” extent or geometry path that differs from emission;
* no ball-derived shadow paired with member-radius grading;
* no whole-face approximation presented as exact;
* no backside patch, unanchored re-entry, or lateral collision exemption;
* no grading-time patch reconstruction from mutable live radius;
* no stale prepared token, unregistered mutation path, or helper-only wiring proof;
* no scalar per-contact native loop; and
* no re-baseline of model outcomes before the required geometry witnesses explain
  every moved verdict.

---

# DECISION MEMO — for Craig

## The dilemma

The re-sized seat zone keeps an arm legal by forgiving every mesh-connected face
inside a distance tube. On the **MEASURED** crevice fixture, a 1.000 mm-radius arm
admitted 20 and 15 wall faces at 1.00 and 1.25 mm from its axis. Those walls have
**INFERRED** final clearances of `−0.300 mm` and `−0.050 mm`. They must remain
refusable. No smaller tube fixes this because the tube must cover the arm's own
refusal disc.

The first proposed replacement—a planar cut lifted onto an “entry sheet”—also does
not work. It climbs the challenge's 82.87-degree raised wall. If it refuses to cross
a crease, it omits the committed channel's physically occupied floor. On a curved
seat, its lifted stencil is not the actual emitted-solid intersection. Its ball
shadow also used a different envelope from the member-radius grader, which made a
fatter arm grade better by `0.02756 mm` across the challenged final-radius span.

The missing input is not another distance. It is the identity of the surface that
the designed foot is intended to enter.

## The candidates side by side

* **Distance tube plus mesh connection:** cheap and already built; **MEASURED**
  wrong on connected crevices.
* **Geodesic disc or projected entry sheet:** follows unlabeled surface continuity;
  it still walks onto the raised or opposite wall and fails curved physical-set
  equality.
* **Face normals or crease stops:** separates selected fixtures only by adding an
  angle or by rejecting a legitimate channel floor and faceted curved seat.
* **Every surface intersected by the embed solid:** exact but too broad; it forgives
  lateral collisions, re-entry, and a far face.
* **Physical intersection on a declared entry carrier:** exact contact geometry
  plus explicit surface identity. It includes legitimate occupied floor, excludes
  a foreign wall, and adds no numeric decision constant.

## Recommendation [authorize a new semantic carrier, or stop]

Authorize the **declared-entry-carrier footprint**:

1. placement/contact construction records an immutable clipped carrier for the
   intended receiving surface;
2. the physical footprint is the entry-anchored component of the exact
   `model surface ∩ carrier ∩ emitted embed solid` intersection;
3. each tier's clearance patch is the entry-anchored component of the exact
   `model surface ∩ carrier ∩ that grader's refusal envelope` intersection;
4. the footprint and grader consume the same exact envelope, so the previous
   ball-versus-member monotonicity defect is impossible;
5. the sanctioned embed interval has exact shared endpoints and far-face
   truncation order; and
6. every creator, mutator, copier, and consumer is registered, has a named red
   sabotage, and rejects a stale prepared token.

This is more geometry and more state than the current branch. It requires exact
clipped polygons, one patch-aware native batch, and a carrier-producing contact
constructor. The carrier is not clearance slack: it does not grow with
`0.300 mm`, `0.600 mm`, or arm radius. The exact grader envelope decides how much
of that carrier is excluded; all other surface stays tested.

## What I ask you to authorize

Please authorize the additional **entry-carrier semantic** and its disclosed cost.
It is a non-numeric surface-identity input, not a new clearance constant.

If that semantic is too large a change, reject the footprint implementation for
now. The present seat face, axis, and triangle connectivity do not contain enough
information to keep both the channel floor legal and the raised or opposite wall
refusable without an unpriced geometric predicate.
