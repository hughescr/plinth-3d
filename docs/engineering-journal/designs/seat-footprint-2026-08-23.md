# Seat footprint: exempt the declared entry carrier, not nearby geometry

**Date:** 2026-08-23; revised 2026-08-24 after round-3 review

**Status:** REVISED DESIGN FOR OWNER DECISION; no production code in this lane

**Branch:** `feature/a4-controller`
**Authority:** Craig's 2026-08-23 ruling that the exemption covers what the foot
actually presses on or occupies, with no distance-connected slack; Craig's narrowed
monotonicity ruling; and the designed-embed rulings in `SPEC-EMBED-POLICY.md` and
`land-at-strike-2026-08-22.md`.

> **Recommendation [adds one upstream surface-identity partition; the current seat
> face, axis, and mesh are not enough].** Authorize model preparation to supply an
> immutable exact **receiver partition** before support search. Each partition cell
> names one complete receiving boundary region. The contact constructor may select
> only the unique cell whose interior contains the intended entry crossing; that
> cell is `Kentry`. It may not draw, clip, or enlarge a carrier after it sees an arm
> radius or clearance result. Missing, overlapping, boundary-only, or semantically
> ambiguous cells make the contact unavailable.
>
> The physical footprint is the exact intersection of `Kentry` with the emitted
> embed solid. For a demand `g`, the clearance patch is the exact intersection of
> that same complete cell with the grader's exact refusal envelope. There is no
> radius-dependent connected-component selection, so radius growth cannot add a
> bridge and change carrier identity. All model surface outside that intersection
> remains graded.
>
> This is not a normal angle, geodesic radius, distance allowance, face flood, or a
> declaration made by the clearance path. It is new upstream surface identity.
> Without it, the raised-column and channel fixtures require contradictory
> continuations from the same geometric inputs. The previous “entry-sheet lift” is
> withdrawn: it climbs the raised wall, cannot include the channel floor without
> also admitting a bridge, and does not equal the physical intersection on curved
> seats.
>
> **Authorization bracket [material].** If adding the upstream receiver partition
> and refusing unpartitioned or ambiguous contacts are not acceptable, do not
> implement a footprint exemption from the current inputs. No repaired distance,
> projection, normal, or adjacency rule in this memo can establish both required
> outcomes without that new semantic input.

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
from `0.061688 mm` at **radius** `0.350 mm` to `0.089244 mm` at **radius**
`1.000 mm`. The fatter arm grades `0.027556 mm` better. At placement
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

### 3.3 `Kentry` comes from an upstream receiver partition [the contact constructor does not invent it]

Model preparation must supply an exact receiver partition `R` on `S` before support
search. `R` is new semantic input, carried with the prepared model and its geometry
digest. Its producer is the model-preparation/import stage, using source surface
provenance or an explicit authored receiver label. Support placement, clearance,
and candidate ranking are consumers, not producers. Automatic inference from an
unlabelled mesh is not authorized.

A cell of `R` is one complete receiving boundary region. “Complete” has this
normative meaning:

1. it contains every two-dimensional continuation of that receiver needed by any
   sanctioned embed and by both authorized demanded gaps, `0.600 mm` and
   `0.300 mm`, including continuation across a crease onto a legitimate channel
   floor;
2. it excludes every different receiver or collision surface, including an
   opposite or raised wall, lateral collision, unanchored re-entry, and the R4 far
   face, even when that surface is close, mesh-connected, smooth, or uniquely
   projected under `n`;
3. its exact clipped cells have disjoint interiors; shared edges and points do not
   transfer identity; and
4. its identity and full extent are independent of support axis, arm **radius**,
   `E0`, `Qg`, clearance result, candidate winner, and grading tier.

The partition producer must either establish those invariants from authoritative
source provenance or mark the region unavailable for model seating. It must not
choose the favourable side of a clearance result. A missing label, overlapping
cell interiors, a crossing `q` on only a shared boundary, or more than one eligible
cell is semantic ambiguity and makes that contact unavailable. It does not trigger
a geometric fallback.

The contact constructor obtains `Kentry` by exact lookup only:

```text
Kentry = the unique complete cell of R whose interior contains q
         and whose receiver direction admits the sanctioned entry through q.
```

It records that cell's immutable identity, exact clipped boundary, producer
version, and partition digest. It does not draw or clip the cell to the current
embed. Thus `Kentry` already contains the complete entry-side portion of
`S ∩ Qg` at both authorized tiers. It cannot stop between `E0` and a refusal
envelope, and it cannot broaden to admit a foreign wall.

This contract intentionally prices the missing information rather than hiding it
inside “the constructor declares it.” The raised-column and channel fixtures prove
that seat face, axis, normal, and unlabelled triangle adjacency do not determine
`R`. If the project cannot supply this upstream partition, this design stops.

---

## 4. The revised footprint rule [actual intersections only]

Form the exact arrangements of `S`, `Kentry`, `E0`, and `Qg`.

The physical footprint is

```text
P0 = S ∩ Kentry ∩ E0.
```

Only exact two-dimensional intersection cells belong to `P0`. A zero-area point or
edge tie does not admit a neighboring cell. Every occupied cell in the unique
receiver belongs, not only cells reachable through the current intersection. `P0`
therefore records surface actually occupied by the emitted semantic solid and
cannot lose a valid lobe because the triangulation separates it. It is not a
tangent-plane stencil.

For the active demand `g`, the clearance patch is

```text
Pg = S ∩ Kentry ∩ Qg.
```

Then grade the exact outside fragments:

```text
Tg = S \ Pg
clearance_g = min over x in Tg of C_live(x),
```

where `C_live(x)` is the grader's exact signed-clearance field in **mm**: positive
outside the live support solid, zero at contact, and negative in penetration. The
binary decision at demanded gap `g` is derived from that same field. `Qg` is its
sublevel envelope at `g`; it is not a separate approximation.

`P0` is the physical scar. `Pg \ P0` is not called contact area. It is the part of
the complete declared receiver that lies inside the exact refusal envelope of the
attached joint. Every surface outside `Kentry`, including an opposite crevice wall,
remains in `Tg` even when it lies in `Qg`.

Removing radius-dependent connected-component selection is deliberate. For fixed
`Kentry`, envelope growth can add patch cells but cannot bridge to a second carrier
or reclassify an existing cell's identity. Foreign walls remain outside `Kentry` by
partition invariant, rather than by the connectivity of `Pg`.

Placement uses `g = 0.600 mm`. Final grading uses `g = 0.300 mm`. They use the same
`Kentry`, `E0`, partition identity, and query semantics, but distinct prepared `Qg`
and `Pg` records. Final grading may not reuse the placement record.

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

## 6. Narrowed monotonicity [distance, transit, and incidence verdicts are ordered for one radius-only family]

Craig's guarantee is: against any surface outside its own footprint patch, a thinner
arm never grades worse. A sanctioned comparison has the same axis, endpoints,
length, entry point and normal, taper family and station parameterization, `Kentry`,
policy, and demand; only **radius** changes. “Never grades worse” means that every
production model-surface predicate which accepts the fatter arm also accepts the
thinner arm. It is not proved by calling one distance number the production grade.

The family must first prove both exact-solid nesting, `Bthin subset-of Bfat`, and
pointwise signed-clearance ordering in **mm**:

```text
r_thin <= r_fat  =>  C_thin(x) >= C_fat(x)  for every x in S.
```

Equivalently, every clearance-level envelope nests. A family that cannot prove both
laws is ineligible for radius comparison.

### 6.1 Distance predicate

For `T(r,g) = S \ (Kentry ∩ Q(r,g))`, define only the distance component:

```text
Ddistance(r,g) = min(0 mm, inf over x in T(r,g) of (C_r(x) - g)).
```

Patch nesting makes every point tested only for the thinner arm lie outside
`Qthin`, where its deficit is non-negative. Common tested points obey the field
order. Therefore `Ddistance_thin >= Ddistance_fat`. This does not claim that
`Ddistance` includes transit or incidence refusals.

### 6.2 Axis-transit predicate

Transit must use the same exact retained fragments `T(r,g)`. A foreign-transit
witness is an interval of the live support solid inside model volume entered across
a fragment of `T(r,g)`. The sanctioned own embed is ignored only where its entry
crossing lies in `Pg`; a **radius**-derived axial seat mask may not hide model
surface from this predicate. Let `Lforeign(r,g)` be the maximum witness length in
**mm**, and define the current threshold margin

```text
Dtransit(r,g) = min(0 mm, min_illegal_len_mm - Lforeign(r,g)).
```

A thinner witness cannot enter through `Kentry \ Qthin`: that surface has no contact
with `Bthin` at demand `g`. Its entry is therefore also retained for the fatter arm.
Exact-solid nesting can only preserve or extend that occupied interval, so
`Lforeign_thin <= Lforeign_fat` and `Dtransit_thin >= Dtransit_fat`.

This is a required replacement contract for the final and placement transit
consumers. The current radius-dependent seat insets are not evidence for it: a
fatter inset can hide an interval still tested for the thinner arm. No sanctioned
comparison may use that path until it consumes `T(r,g)` as above.

### 6.3 Degenerate- and grazing-incidence predicates

Degeneracy depends only on the common axis and entry normal, so it is identical for
the pair. For emergence, let `M(r)` be the total claimed mask extent in **mm** and
`L` the common seated member length. Every sanctioned taper family must prove
`Mthin <= Mfat` from the exact seat-depth and mask formulas. Then

```text
Demerge(r) = min(0 mm, L - min_free_span_mm - M(r))
```

obeys `Demerge_thin >= Demerge_fat`. If mask ordering is not proved for every
contact end in the family, that family is ineligible for radius comparison.

Production placement conjoins distance and transit. Final grading stamps transit
into its distance verdict. CCO conjoins distance, degenerate incidence, and grazing
emergence. The three proofs above therefore establish the narrowed guarantee for
all registered production verdicts, rather than only for the distance field.

Positive ranking after both arms are legal is separate. It uses a pair-bound
`CommonRetainedMask`, `Tcommon = Tthin ∩ Tfat`, and
`R(r,Tcommon) = inf C_r(x)` over that common population. Pointwise ordering proves
`Rthin >= Rfat`. Both radius-bound tokens and the pair token are fingerprint-checked.

The six CHAL-B radii must pass through the real final, placement
`candidate_revalidation`, and CCO clearance consumers at both authorized demands.
Each ladder records distance, transit, degenerate-incidence, and grazing-emergence
components before the combined verdict. Required rows include a foreign-transit
boundary, a grazing-emergence boundary, the old radius-growth bridge, and the
**INFERRED** `0.027556 mm` distance reversal. Helper-only or distance-only ladders
do not count. Different axes, carriers, or taper families are outside this theorem.

---

## 7. Lifecycle and production wiring [every creator, mutator, copier, and consumer is registered]

Store one immutable radius-bound `PreparedEntryFootprint` containing `q`, `n`,
contact end, `Iembed`, the semantic geometry object and fingerprint, upstream
partition producer/version/digest, complete `Kentry`, `P0`, tier, prepared `Qg`,
`Pg`, exact boundary clips, and a digest. A `CommonRetainedMask` is a separate
pair-bound comparison object; it never substitutes one footprint token for another
radius.

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

* model-preparation/import production and validation of the receiver partition;
* initial contact/stub/foot creation and unique partition-cell lookup;
* CCO candidate preparation, commit, and commit-copy;
* brace radius growth, shrink, endpoint, taper, and commit paths;
* rewire slide, retire, root-reanchor, and replacement paths;
* candidate replay and winner copy;
* placement and final tier preparation;
* final and placement foreign-transit preparation and consumption;
* CCO degenerate-incidence and grazing-emergence preparation and consumption;
* pair-bound common-mask preparation and candidate-ranking consumption; and
* every distance-clearance consumer.

Each registered path gets a named red sabotage that disables only that path's
produce, validate, lookup, prepare, invalidate, copy, mask, or consume transition
and then exercises the production path. Partition import/validation, CCO
commit/copy, brace mutation, rewire, and common-mask ranking paths are mandatory,
not examples. A helper-level test does not count.

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
9. **Monotonicity.** Six-radius same-axis, same-taper ladders pass through the real
   final, placement, and CCO consumers. They record distance, foreign transit,
   degenerate incidence, grazing emergence, combined verdict, and pair-bound positive
   rank. A fatter-only transit mask that hides a foreign interval, an unproved mask
   formula, the ball-shadow/member-radius mismatch, one-token/two-radius reuse, or a
   helper-only distance ladder must fail.
10. **Partition authority and ambiguity.** Provenance-labelled raised-column and
    channel models produce the required complete cells. Missing labels, overlapping
    interiors, boundary-only entry, and two eligible cells each refuse contact
    creation before clearance. An unlabelled-mesh inference fallback must fail.
11. **Production-site sabotage.** Every registry entry in section 7 has one named
    red. Disable all live produce/validate/lookup/prepare/re-cut/mask sites and the
    library or integration estate must not remain green.
12. **Stale-token sabotage.** A caller passes a valid token prepared before an
    axis-only mutation. The consumer rejects it before grading.
13. **Batch sabotage.** Scalar native calls are detected at the declared boundary.
14. **Giant Bat.** The two known opposite-wall populations remain tested; the common
    final `0.300 mm` rule decides them. “Bat emits” is not the criterion.
15. **Corpus cost witness.** Use a model that places a model anchor; `zombie-3-a`
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

## The dilemma [the current inputs cannot identify the foot's own surface]

The re-sized seat zone keeps an arm legal by forgiving every mesh-connected face
inside a distance tube. On the **MEASURED** raised-column fixture, a
`1.000 mm`-**radius** arm admitted `0 → 20` and `0 → 15` opposite-wall faces at
wall-axis distances `1.00 mm` and `1.25 mm`. Those walls have **INFERRED** final
clearances of `−0.300 mm` and `−0.050 mm` at demanded gap `g = 0.300 mm`. They must
remain refusable. No smaller tube fixes the rule because the tube must cover the
arm's own refusal disc.

The first replacement—a planar cut lifted onto an “entry sheet”—also fails. It
climbs the **INFERRED** `82.87°` raised wall. If it refuses to cross a crease, it
omits the committed channel's physically occupied floor. **MEASURED**, that fixture
has 16 floor faces and 11 far-wall faces in the connected band; the footprint must
include the occupied floor and retain all 11 far-wall faces for grading. On a
curved seat, a lifted stencil is not the emitted-solid intersection.

The old shadow also used a different envelope from the member-radius grader.
**INFERRED**, its positive margins were `0.061688 → 0.089244 mm`, so a fatter arm
graded `0.027556 mm` better at `g = 0.300 mm`.

The missing input is not another distance. It is authoritative identity and full
extent for the surface that the designed foot is intended to enter.

## Recommendation [authorize an upstream receiver partition, or stop]

Authorize model preparation to supply an immutable exact **receiver partition**
before support search. The partition producer must use source surface provenance or
an explicit authored receiver label. It must define complete, non-overlapping
receiving boundary cells. It must not infer the favourable answer from clearance,
arm **radius**, mesh connectivity, projection, or a normal angle.

The contact constructor performs only exact lookup. `Kentry` is the unique complete
partition cell whose interior contains the intended entry crossing and admits the
sanctioned entry direction. Missing labels, overlapping interiors, a boundary-only
crossing, or more than one eligible cell make that model contact unavailable. There
is no unlabelled-mesh fallback.

For each live arm:

1. `Iembed` uses the shared designed terminal endpoint and entry crossing, after R4
   far-face truncation;
2. `E0` is the exact union of emitted primitives clipped to `Iembed`;
3. `P0 = model surface ∩ Kentry ∩ E0` is the physical footprint;
4. `Qg` is the exact refusal envelope derived from the grader's exact signed-
   clearance field at demand `g`;
5. `Pg = model surface ∩ Kentry ∩ Qg` is the clearance patch; and
6. every exact model-surface fragment outside `Pg` remains graded.

The complete partition cell, not a radius-dependent connected component, supplies
surface identity. Radius growth can add patch cells inside that one cell, but it
cannot add a bridge that admits an opposite wall. The cell must already contain the
complete entry-side receiver needed at both `g = 0.600 mm` placement and
`g = 0.300 mm` final grading. Thus it cannot stop after the physical intersection
and restore own-seat refusal at a tier.

## Monotonicity [proved for every production model-surface refusal]

For a sanctioned same-axis, same-taper, same-`Kentry`, same-demand pair, only
**radius** changes. The family must prove exact-solid nesting and pointwise signed-
clearance ordering in **mm**. The own-patch distance deficit then orders because
patches nest and common retained points obey the field order.

That distance proof is not the production theorem. Production also refuses
foreign axis transit and CCO degenerate or grazing incidence. Transit must be
redefined on the same exact retained fragments outside `Pg`: its numeric grade is
the threshold margin for the longest foreign occupied interval. A **radius**-derived
seat inset may not hide one. Exact-solid nesting then makes a thinner foreign
interval no longer than the fatter interval. Degeneracy is identical because axis
and normal are fixed. Grazing emergence orders only after each taper family proves
that its exact claimed mask extent is nondecreasing with **radius**. The combined
final, placement, and CCO verdicts are conjunctions of these ordered components.

The six CHAL-B radii must pass through the real final, placement, and CCO consumers.
Each witness records distance, transit, degenerate-incidence, grazing-emergence, and
combined verdict. It includes a foreign-transit boundary and a grazing boundary;
a distance-only helper witness does not count.

Positive ranking after both arms are legal remains a separate pair-bound
`CommonRetainedMask` query over one common population. One token cannot be reused
for two radii, and stale geometry must reject before comparison.

## Cost and proof obligations [new semantics, exact clipping, and production-path sabotage]

This is more geometry, state, and upstream data than the rejected branch. It needs
partition provenance, exact clipped cells, matched query envelopes, radius-bound
prepared tokens, a pair-bound comparison mask, one patch-aware native batch, and
fingerprints that include partition identity and geometry.

Every partition producer, validator, lookup, contact creator, CCO commit/copy,
brace mutation, rewire, replay, tier preparation, comparison-mask path, and
clearance consumer must be registered. Each registered production path needs its
own named red sabotage. Required gates include partition ambiguity/refusal, both
crevice directions, exact curved intersections, R4 truncation, lateral collisions,
pointwise numeric ordering, own-patch ordering, the radius-growth bridge,
pair-mask ordering, stale tokens, and batch-call count.

## What I ask you to authorize [one semantic addition with a refusal policy]

Please authorize the upstream receiver partition and the rule that unpartitioned or
ambiguous model contacts are unavailable. This is a non-numeric surface-identity
input, not a new clearance constant.

If that input and refusal policy are too large a change, reject the footprint
implementation for now. The present seat face, axis, normal, and unlabelled triangle
connectivity do not contain enough information to keep the channel floor legal and
the raised or opposite wall refusable.

## What changed [round-3 blocking finding folded in]

- Retracted the claim that the distance deficit alone is the production safety
  grade. It is now named only as the distance component.
- Enumerated all production model-surface refusals: final and placement foreign
  axis transit, plus CCO degenerate incidence and grazing emergence.
- Replaced radius-dependent axial seat masking in the monotonicity contract with a
  foreign-transit predicate over the same exact retained fragments outside `Pg`.
  A fatter seat mask may no longer hide a foreign interval tested for a thinner arm.
- Added the numeric transit threshold margin in **mm** and proved its radius ordering
  from exact-solid nesting.
- Proved degenerate incidence identical for a same-axis, same-normal pair. Made each
  sanctioned taper family prove that exact claimed mask extent is nondecreasing with
  **radius** before the grazing-emergence margin can enter a comparison.
- Combined the ordered distance, transit, degenerate-incidence, and emergence
  components explicitly for the real final, placement, and CCO verdicts.
- Required six-radius ladders through those real consumers, including foreign-
  transit and grazing boundaries. A helper-only or distance-only witness is not an
  acceptance gate.
- Registered transit and incidence producers and consumers for named red sabotage.
- Preserved the upstream receiver partition, matched exact `Qg`, exact clipping,
  pair-bound positive ranking, lifecycle, and batch-first requirements from round 2.
- Preserved the evidence with provenance: **MEASURED** `0 → 20` and `0 → 15` wall
  admissions; **INFERRED** `−0.300 mm` and `−0.050 mm` clearances; **MEASURED** 16
  channel-floor and 11 far-wall faces; and the **INFERRED** `0.061688 → 0.089244 mm`
  margins with `0.027556 mm` reversal.
