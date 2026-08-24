# Seat footprint: distinguish a buried receiver from nearby collision geometry

**Date:** 2026-08-23; revised 2026-08-24 after round-4 closing review

**Status:** REVISED DESIGN FOR OWNER DECISION; no production code in this lane

**Branch:** `feature/a4-controller`
**Authority:** Craig's 2026-08-23 ruling that the exemption covers what the foot
actually presses on or occupies, with no distance-connected slack; Craig's narrowed
monotonicity ruling; and the designed-embed rulings in `SPEC-EMBED-POLICY.md` and
`land-at-strike-2026-08-22.md`.

> **Recommendation [measure the nearest-feature/buried-portion rule before #77
> merges; no new upstream data or exact-arrangement kernel].** Make the Rule-Z family
> the leading candidate. For each model-surface point in the live refusal envelope,
> query the nearest point on the boundary of the support solid and exempt it only when
> every nearest boundary point is inside the model volume. Existing geometry can provide
> all three inputs: the live support solid, `NativeSurface::inside`, and the grader's
> clearance field. Evaluate retained fragments in one native batch.
>
> **Evidence bracket [INFERRED, not yet production-measured].** The closing review
> re-derived Rule Z on both required fixtures. On the raised column it exempts the
> seat exactly through radial distance `rho = 1.389 mm`, while the walls at axis
> distances `0.75`, `1.00`, and `1.25 mm` remain refusable, including the required
> `−0.050 mm` wall-only margin at `1.25 mm`. On the committed channel it keeps the
> floor legal and leaves all 11 far-wall faces graded.
>
> **Price bracket [a real false refusal must be bounded or measured].** Rule Z also
> refuses a shaft that grazes its own receiver just outside the occupied footprint.
> A shallow-angle seat is the binding witness. Rule Z is not radially monotone, so it
> does not inherit the previous band proof; every distance, transit, degenerate-
> incidence, and grazing-emergence branch must pass the narrowed radius contract.
>
> **Partition alternative [future authored content only; unavailable on today's STL
> corpus and dependent on a new robust exact surface-arrangement kernel].** A receiver
> partition cannot satisfy its own proposed invariants on the committed channel, and
> its ambiguity path silently truncates a patch. With no unlabelled-mesh fallback,
> every model contact on every current corpus STL is unavailable: zero model-seated
> supports, including the Giant Bat. Do not recommend it for #77.

---

## 1. The measured defect [the current distance-connected rule forgives a real crevice refusal]

The landed candidate on `feature/f1-refreeze` freezes the mesh-connected component
containing the stored seat face of all model faces within `contact radius + 0.600 mm`
of the live arm axis. Re-cutting after sizing repairs stale radius, but not the set
predicate. Mesh adjacency says that triangles belong to one object. It does not say
that one support foot owns both triangles.

The heavy challenge used a watertight `0.250 mm`-pitch slab. It raised the vertices
beyond one grid column by `2.000 mm`. The arm had creation **radius** `0.350 mm` and
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

The wall-only arithmetic is `wall_x − 1.000 − 0.300 mm`. At `1.00` and
`1.25 mm`, the wall must remain refusable. The re-sized zone instead admitted 20 and
15 wall faces and changed the aggregate result from refused to legal.

No smaller concentric distance band repairs that rule. A planar seat needs its own
refusal disc excluded through **radius** `r + g`. A foreign wall inside that disc is
also inside any Euclidean band large enough to keep the seat legal. Connectivity
through the band is the only remaining separator within that family, and the raised-
column and committed `channel()` fixtures both provide a connected path.

The Giant Bat is the production acceptance shape. **MEASURED** in
`honest-exemption-2026-08-22.md`, both investigated refusals were opposite crevice
walls. Slot 250 had approximately `0.291 mm` true gap, below the final `0.300 mm`
demand. A correct rule does not promise that the Bat emits. It promises that the
opposite wall stays in the tested surface and lets the common demand decide.

---

## 2. What the earlier candidates established [the impossibility claim is retracted]

### 2.1 A shadow and its grader must use one envelope

The previous memo formed a shadow from the emitted foot ball, then graded by
subtracting only member radius `r`. For ball factor `f > 1` and demanded gap `g`,
the retained planar boundary is

```text
d = sqrt(r² + 2 f r g + g²).
```

The reported clearance was therefore `d − r − g`, not zero. **INFERRED** with
`f = 1.4` and final `g = 0.300 mm`, the margin rises from `0.061688 mm` at
**radius** `0.350 mm` to `0.089244 mm` at **radius** `1.000 mm`. The fatter arm
grades `0.027556 mm` better. At placement `g = 0.600 mm`, the margins are
`0.08465` and `0.14356 mm`.

One geometry authority must therefore produce the support solid, refusal envelope,
nearest-feature query, and signed-clearance field. A ball-derived shadow cannot be
paired with a cylinder-radius subtraction.

### 2.2 Projection, normal, crease, and adjacency rules still fail

The challenge wall rises `2.000 mm` over one `0.250 mm` grid column. Its
**INFERRED** angle is `atan(2/0.25) = 82.87 degrees`. It is a single-valued height
field under the slab normal, so a projected lift climbs it.

The committed `channel()` fixture has a **radius** `0.400 mm` arm only `0.300 mm`
above the floor. The member cylinder occupies the floor, and the factor-1.4 foot ball
has **radius** `0.560 mm`. **MEASURED** in the committed test, the connected band
contains 13 near-wall, 16 floor, and 11 far-wall faces. A rule that forbids crossing
the near-wall/floor crease omits a legitimate occupied floor patch. A rule that
relaxes continuation across the crease can bridge to the far wall.

A tangent-plane stencil also fails on curved seats. For a ball of **radius**
`R = f r` whose center lies `c = sqrt(f² − 1) r` below the tangent plane, actual
occupation of `z = delta(rho)` is

```text
rho² + (delta(rho) + c)² <= R².
```

Both curvature signs separate this physical intersection from a lifted disc.

These results reject the tested projection, normal, crease, adjacency, and
concentric-band rules. They do **not** prove that every rule from current inputs must
fail. The earlier impossibility statement is retracted. Rule Z is a construction from
current inputs that separates both fixtures in the required direction.

---

## 3. Exact objects and Rule Z [current inputs; nearest buried support feature]

For one designed embed, define:

* `S`: the exact model boundary used by placement and grading;
* `M`: the model volume whose boundary is `S`, queried by the same inside semantics
  already used through `NativeSurface::inside` at `brace.rs:1949`;
* `q`: the stored intended entry crossing on the stored seat face;
* `n`: the stored outward entry normal;
* `A`: the live member axis and its exact tapered station data;
* `Iembed`: the closed, tip-anchored sanctioned embed interval;
* `B`: the exact closed live support solid used by the grader;
* `E0`: the exact closed portion of `B` emitted inside `Iembed`; and
* `Qg`: the exact closed refusal envelope implied by the grader's signed-clearance
  field for the already-authorized demand `g`.

### 3.1 `Iembed`, `E0`, and `B` have one authority

`Iembed` starts at the designed terminal end and ends at the stored intended entry
crossing along the contact axis. If R4 truncates the seat first, the far face replaces
the untruncated terminal endpoint. There is no interval beyond the entry crossing or
beyond far-face truncation.

The authority resolves the live axis, endpoint **radii**, taper stations, emitted ball
or frustum class, ball factor, and derived seat depth; applies the tip anchor and R4
truncation; clips class-specific end primitives to `Iembed` with the emitter's exact
cap semantics; and unions those primitives as `E0`. It also supplies the complete
emitted `B` used by clearance. Placement, Rule Z, grading, scar measurement, transit,
and emission consume the same geometry fingerprint.

### 3.2 `Qg` and the signed-clearance field are grader-owned

For active demand `g`:

```text
x is in Qg  <=>  a surface consisting only of point x would be refused
                  by the exact live-support clearance query at demand g.
```

This includes exact cylinder, taper, endpoint, cap, and station semantics. Rule Z and
the clearance kernel must receive the same prepared support/envelope token. No path
may rebuild a convenient ball shadow or use a different radius convention.

### 3.3 The Rule-Z predicate

For every point `x` of `S ∩ Qg`, let `N_∂B(x)` be the set of nearest points on the
boundary `∂B` of the closed live support solid `B`. The conservative tie rule is that
every nearest boundary point must be buried; a nearest-point tie that includes an
in-air feature remains graded. A point inside `B` is never exempt on that fact alone.

```text
Z_g(x) = true  iff  every y in N_∂B(x) is inside M.

Pg(Z) = { x in S ∩ Qg | Z_g(x) }.
Tg(Z) = S \ Pg(Z).
```

The physical scar remains `P0 = S ∩ E0`. `Pg(Z) \ P0` is clearance forgiveness, not
contact area. Every retained exact fragment is graded by the existing signed-clearance
field. A triangle crossing the Rule-Z boundary is split; whole-face exclusion would
add mesh-size forgiveness.

The predicate asks which support-boundary feature makes `x` close: a buried boundary
feature may explain the receiver; a boundary shaft feature standing in air may not
explain a nearby wall. This is neither a radial band nor a surface flood.

No new upstream data is required. `E0` is derived from axis, endpoint **radius**,
ball-seat factor, taper, and truncation data already needed by emission. Model-inside
queries and the grader's clearance field already exist. This candidate does not need
an exact surface-arrangement or Boolean-intersection kernel.

---

## 4. Rule Z on both required fixtures [INFERRED derivations; production measurement still gates]

### 4.1 Raised column

Use `f = 1.4`, support **radius** `r = 1.000 mm`, final demand `g = 0.300 mm`, foot-ball
**radius** `R = f r = 1.400 mm`, and ball-center depth
`c = sqrt(f² − 1) r = sqrt(0.96) mm` below the seat plane.

**INFERRED:** at seat-plane radial distance `rho = 1.20 mm`, the ball is the nearest
support-boundary feature: its distance is `0.1494 mm`, versus `0.200 mm` to the shaft.
The nearest point on the ball has `z = −0.0945 mm`, inside the slab model volume, so
Rule Z exempts the seat point. The exempt seat set ends exactly at
`rho = sqrt(r² + 2 f r g + g²) = sqrt(1.93) = 1.389 mm`, the same boundary at which
the point ceases to be self-refusing.

**INFERRED:** at wall-axis distances `0.75`, `1.00`, and `1.25 mm`, the nearest
support-boundary feature is the shaft surface standing in air. In particular, the
raised-wall point `(0.80, z = 1.0)` is inside the shaft, but its nearest `∂B` point is
the shaft surface at horizontal distance `1.000 mm`, standing in air. Rule Z therefore
retains that wall, which is refusable at its required wall-only final margin
`−0.550 mm`; the `1.00` and `1.25 mm` rows have margins `−0.300` and `−0.050 mm`.
Thus the seat is legal while both measured binding wall populations remain refusable.

### 4.2 Committed channel

The committed bottom case has support **radius** `r = 0.400 mm`, axis height
`z = 0.300 mm`, and a cylinder reaching to `z = −0.100 mm` through the channel floor.

**INFERRED:** the channel floor point `(0.3, 0)` is inside the cylinder, but its
nearest `∂B` point at `z ≈ −0.055 mm` is buried, versus `z ≈ 0.745 mm` on the upper
surface. Rule Z exempts that floor, so the physically occupied channel floor remains
legal. At far wall `x = 1.200 mm`, the nearest support-boundary point is on the shaft
near `x = 0.650 mm` and is in air. Rule Z retains the far wall. All 11 pinned far-wall
faces stay graded.

These are analytic re-derivations using the memo's dimensions. They are **INFERRED**,
not **MEASURED** production results. The seat-plane derivation at `rho = 1.20 mm` and
the `1.389 mm` boundary are unaffected because those points are outside `B`.
Acceptance gates require exact-fragment output and live final, placement, transit, and
CCO verdicts on both fixtures.

---

## 5. The named cost of Rule Z [own-receiver grazing can become a false refusal]

Rule Z can refuse a shaft that grazes its own receiver immediately outside `P0`.
At a shallow-angle seat, a receiver point can be closest to an exposed shaft station
in air even though that surface is the intended receiver. The same discriminator that
protects the raised wall then treats part of the receiver as foreign. **INFERRED:** the
graze cost and the monotonicity risk are one mechanism: the ball reaches out with
`f · r` while the shaft reaches with `r`, and the ball center deepens as
`sqrt(f² − 1) · r`, so the ball-versus-shaft crossover moves outward with radius. A
receiver point at the crossover is explained by the buried ball for the fat arm and
by the in-air shaft for the thin one, so the thinner arm refuses where the fatter
passes. On a flat seat the ball always wins (tie at `rho = r`), so the inversion needs
a tilted axis; the shallow-angle sweep is the binding witness for both.

This cost must be bounded or measured before implementation authority:

1. sweep shallow entry angles without adding a new decision angle;
2. include smooth convex, smooth concave, folded, and coarse-triangle receivers;
3. record retained own-receiver area, minimum signed clearance in **mm**, and which
   nearest primitive decided each fragment;
4. pass each row through the real placement and final clearance consumers; and
5. report corpus frequency and moved model-seated-support outcomes, including the
   Giant Bat.

A favourable raised-column/channel result does not erase this price. If shallow-angle
or corpus evidence shows material false refusal, Craig must either accept that strict
class, authorize a different discriminator, or choose an alternative in section 10.
No new fixed angle, distance, curvature, face-count, or tolerance is authorized here.

---

## 6. Narrowed monotonicity for Rule Z [not radial; all four refusal branches must order]

Craig's guarantee is: against any surface outside its own footprint patch, a thinner
arm never grades worse. A sanctioned pair has the same axis, endpoints, length, entry
point and normal, taper family and station parameterization, policy, and demand; only
**radius** changes.

The family must first prove exact-solid nesting and pointwise signed-clearance ordering:

```text
r_thin <= r_fat  =>  Bthin subset-of Bfat
                      and C_thin(x) >= C_fat(x) for every x in S.
```

Rule Z is **not radially monotone**. **INFERRED:** the graze cost and monotonicity risk
are one mechanism: the ball reaches out with `f · r` while the shaft reaches with `r`,
and the ball center deepens as `sqrt(f² − 1) · r`, so the ball-versus-shaft crossover
moves outward with radius. A receiver point at the crossover is explained by the buried
ball for the fat arm and by the in-air shaft for the thin one, so the thinner arm
refuses where the fatter passes. On a flat seat the ball always wins (tie at `rho = r`),
so the inversion needs a tilted axis; the shallow-angle sweep is the binding witness
for both. Therefore neither `Pg(Z, thin) subset-of Pg(Z, fat)` nor its reverse is
assumed. The former band proof does not apply. The Rule-Z family is eligible only if the
following production-level contract holds on its full authorized family, with a
counterexample rejecting the candidate rather than being labelled slack.

### 6.1 Distance refusal

For `T_Z(r,g) = S \ Pg(Z,r,g)`, define

```text
Ddistance(r,g) = min(0 mm, inf over x in T_Z(r,g) of (C_r(x) - g)).
```

The implementation must establish `Ddistance_thin >= Ddistance_fat` directly. For
points retained only for the thinner arm it must prove or measure a non-negative
thin-arm deficit; common points use field ordering. Patch nesting may not be cited.

### 6.2 Axis-transit refusal [the own-embed whitelist ends at `Iembed`]

Transit consumes the same exact retained fragments `T_Z(r,g)`. A foreign-transit
witness is an interval of live support inside model volume entered across a retained
fragment. The whitelist for sanctioned own-embed transit is bounded only by the
closed station interval `Iembed`, after tip anchoring and R4 far-face truncation. It
may ignore crossings caused by `E0` only at support stations in `Iembed`. It must not
use `Pg`, `Qg`, a tier-dependent seat mask, or any continuation beyond `Iembed`.

Let `Lforeign(r,g)` be the longest non-whitelisted witness in **mm** and define the
current threshold margin:

```text
Dtransit(r,g) = min(0 mm, min_illegal_len_mm - Lforeign(r,g)).
```

Rule Z must establish `Lforeign_thin <= Lforeign_fat`, hence
`Dtransit_thin >= Dtransit_fat`, through the real final and placement consumers.
Because retained fragments do not necessarily nest, exact-solid nesting alone is not
a proof.

### 6.3 Degenerate-incidence refusal

For a sanctioned pair the axis and entry normal are identical, so a radius-independent
degeneracy predicate must return the same result. The ladder must record this branch
separately and reject any implementation that lets the changing Rule-Z patch or
nearest primitive alter it.

### 6.4 Grazing-emergence refusal

Let `M(r)` be the total claimed mask extent in **mm** and `L` the common seated-member
length. Every taper family must prove `Mthin <= Mfat`, then

```text
Demerge(r) = min(0 mm, L - min_free_span_mm - M(r))
```

orders. The emergence mask must be derived from exact end geometry and `Iembed`, not
from `Pg` or `Qg`. If mask ordering is not proved for every contact end in the family,
that family is ineligible.

Production placement conjoins distance and transit. Final grading stamps transit into
its distance verdict. CCO conjoins distance, degenerate incidence, and grazing
emergence. Six-radius same-axis, same-taper ladders must pass through all three real
consumers at both authorized demands. Each row records all four components and the
combined verdict. Required witnesses include a shallow-angle own-receiver graze, a
foreign-transit boundary, a grazing-emergence boundary, the old radius-growth bridge,
and the **INFERRED** `0.027556 mm` shadow/grader reversal. Helper-only or distance-only
ladders do not count.

Positive ranking after both arms are legal remains separate. It uses a pair-bound
common retained population and the exact fields; one radius-bound token may not be
reused for two radii.

---

## 7. Lifecycle, batching, and cost [pointwise fragment work; batch or reject]

Store one immutable radius-bound `PreparedBuriedFeatureFootprint` containing `q`, `n`,
contact end, `Iembed`, exact support geometry and fingerprint, model geometry digest,
tier, prepared `Qg`, exact Rule-Z boundary fragments, retained fragments, and nearest-
primitive classifications. Every consumer recomputes the live fingerprint and rejects
a stale token before grading.

Register every creator, mutator, copier, and consumer by construction: initial
contacts; CCO preparation, commit, and commit-copy; brace radius and endpoint changes;
rewire slide, retire, root-reanchor, and replacement; replay and winner copy; placement
and final preparation; transit; CCO incidence; positive ranking; and each distance
consumer. Every registry entry needs a named red sabotage.

The cost is pointwise work on retained fragments. The path remains batch-first:

1. prepare all live support solids and `Qg` requests;
2. broad-phase all model faces against all row envelopes in one native batch;
3. compute nearest support-boundary features and batched model-inside queries for all
   exact boundary events needed to classify retained fragments;
4. split boundary triangles and pack retained fragments in CSR-style row storage; and
5. issue one native patch-aware clearance batch.

There is no scalar native call per contact and no scalar `inside` call per fragment.
A counter sabotage must detect more than one call at each declared batch boundary.
Checked resource limits may refuse a whole batch; they may not truncate a patch or
drop a boundary fragment.

Unlike the partition alternative, Rule Z needs no new authored labels, no upstream
partition, and no robust exact surface-arrangement kernel. It does require a nearest-
feature kernel that reports ties and primitive identity, batched inside tests, exact
fragment retention at the Rule-Z boundary, storage, fingerprints, and production-path
tests. Measure fragment counts, tie counts, peak bytes, and native batch time on the
fine slab, both crevices, shallow-angle seats, Giant Bat rows, and a corpus model that
actually places a model anchor.

---

## 8. Receiver partition re-grade [future authored content; not today's recommendation]

A receiver partition would label complete receiving boundary cells before support
search, then use exact intersections such as `S ∩ Kentry ∩ E0` and
`S ∩ Kentry ∩ Qg`. That machinery depends on a **new robust exact surface-arrangement
kernel on triangle meshes**, including exact Boolean/intersection cells, boundary
splitting, and patch-aware clearance. Plinth does not have that kernel today. It is the
largest implementation dependency and must remain bracketed wherever this option is
described.

The candidate also fails its own current contract on the committed channel. The
channel is `1.200 mm` wide. A sanctioned **radius** `0.400 mm` embed at `x = 0.600 mm`
has a final envelope that reaches `0.700 mm` beyond `x = 1.200 mm` onto the far wall.
The proposed invariant 1 requires the floor cell to contain every continuation needed
by the sanctioned embed and envelope; invariant 2 requires it to exclude the far wall.
They are jointly unsatisfiable. The producer can only mark the whole channel
unavailable, so former acceptance gate 10 is unreachable for the geometry the design
was meant to serve.

Patch-extent ambiguity is not made loud. A unique cell can contain `q` while `Qg`
continues beyond the cell edge. Lookup succeeds, `Pg` is silently truncated, and the
seat can refuse itself in the common crevice case. “No fallback” does not detect that
failure.

Production input is unlabelled STL minis. With no unlabelled-mesh fallback, **every
model contact on every current corpus model becomes unavailable: zero model-seated
supports, including the Giant Bat.** Nothing in current provenance can split the
raised column's wall from the slab out of which it grows.

A partition may survive as a future authored-content option if a source format carries
receiver labels, the invariant is redesigned for channels, truncation becomes an
explicit refusal, and the new robust exact surface-arrangement kernel is separately
authorized and priced. It is not the recommendation for `feature/f1-refreeze` or #77.

---

## 9. Acceptance estate [favourable and unfavourable populations]

1. **Matched support authority.** The support solid, nearest-feature query, `Qg`, and
   signed-clearance field use identical ball, shaft, taper, cap, station, and **radius**
   semantics at `g = 0.600 mm` and `g = 0.300 mm`.
2. **Raised-column ladder.** Run wall-axis distances `0.75`, `1.00`, `1.25`, `1.50`,
   `1.75`, and `2.00 mm`. Measure seat legality, nearest support-boundary primitive and
   inside result, retained wall polygons, and wall-only final clearances approximately
   `−0.550`, `−0.300`, `−0.050`, `+0.200`, `+0.450`, and `+0.700 mm`.
3. **Committed channel, both heights.** The bottom arm keeps its occupied floor legal
   and all 11 pinned far-wall faces graded. The upper arm keeps its intended receiver
   without a far-wall exemption.
4. **Shallow-angle receiver graze.** Sweep entry angle without introducing a decision
   angle. Measure false refusals where an exposed shaft becomes the nearest feature.
5. **Curved and coarse receivers.** Convex, concave, folded, smooth, and coarse-
   triangle seats split exact retained fragments; whole-face forgiving and whole-face
   strict sabotages each move a witness.
6. **R4 and lateral overlap.** `Iembed` ends at the far-face truncation. No transit,
   patch, or emergence whitelist extends beyond it. A lateral collision remains
   refusable.
7. **Nearest-feature ties.** A tie with any in-air nearest feature remains graded. A
   favourable tie-break sabotage must fail.
8. **Geometry mutation.** Radius growth and shrink, axis translation and shortening,
   endpoint move, taper change, model digest change, and tier change invalidate the
   prepared token. **MEASURED** CHAL-A axis swing leaves 24 faces under-forgiven
   (`116 → 140`); **MEASURED** CHAL-A2 axis shorten leaves 16 faces over-forgiven
   (`155 → 139`). Both remain mandatory.
9. **Monotonicity.** Six-radius ladders pass through real final, placement, and CCO
   consumers and record distance, transit, degenerate incidence, grazing emergence,
   and combined verdict. No branch may cite radial patch nesting.
10. **Production-site sabotage.** Every registered produce, copy, invalidate, prepare,
    classify, split, mask, and consume path has one named red.
11. **Batch sabotage.** Scalar native calls and scalar per-fragment inside calls are
    detected at declared boundaries.
12. **Giant Bat and corpus cost.** Both known opposite-wall populations stay tested;
    the common final `0.300 mm` demand decides. Record moved contacts and runtime on
    models that actually place model anchors.

The raised-column generator is preserved in Appendix A and as evidence at
`.git/seal-landing-2026-08-23/raised-column-fixture.rs`. Every sabotage must fail first
on the claim it attacks, not on fixture setup.

---

## 10. Owner choices and #77 [each option carries its price]

### Option A — Measure Rule Z, then decide #77 [recommended; merge remains held]

Keep #77 held while Rule Z is implemented only as an experiment and run through the
acceptance estate. If it preserves both crevices, bounds the shallow-angle graze cost,
and satisfies all four monotonicity branches, replace the landed distance-band rule
before #77 merges. Price: nearest-feature and batched-inside geometry work, exact
retained fragments, lifecycle wiring, and the possibility that measured graze refusal
rejects the candidate.

### Option B — Keep the landed distance band [smallest code change; disclosed forgiveness]

Merge #77 with its current re-sized distance-connected exemption. Price: accept the
**MEASURED** forgiveness class in which a `1.000 mm`-**radius** arm newly exempts 20
and 15 connected wall faces at `1.00` and `1.25 mm`; those walls have **INFERRED**
final margins `−0.300` and `−0.050 mm`. The same class matches both investigated Giant
Bat opposite-wall refusals. This is not a footprint repair; it is an explicit safety
exception.

### Option C — Do not grant a seat exemption [strict rollback; #77 cannot merge as landed]

Either revert the distance-band behaviour to accept-for-now semantics with no claimed
footprint repair, or refuse all seat exemptions. Reverting means #77 does not merge in
its current form and the pre-rule acceptance behaviour remains until a design lands.
Refusing all seat exemptions makes every arm grade against its own designed embed;
model-seated supports self-refuse, so #77 also cannot merge as landed and corpus
support yield falls sharply. Price: no foreign-wall forgiveness, but no useful own-seat
exception either.

The receiver partition is not a fourth near-term option. On today's unlabelled STL
corpus it removes every model contact, fails the committed-channel invariant, and
requires a new robust exact surface-arrangement kernel.

---

## 11. What this does not authorize

* no change to the `0.600 mm` placement margin or `0.300 mm` final hard demand;
* no new fixed angle, curvature, distance, face-count, or tolerance decision number;
* no ball-derived shadow paired with member-radius grading;
* no favourable nearest-point tie break;
* the nearest-point query is on `∂B`, and a point inside `B` is never exempt on that
  fact alone;
* no whole-face approximation presented as exact;
* no own-embed transit whitelist outside `Iembed` or based on `Pg` or `Qg`;
* no stale prepared token, unregistered mutation path, or helper-only wiring proof;
* no scalar per-contact or per-fragment native loop;
* no receiver partition without separately authorizing its robust exact surface-
  arrangement kernel and authored-content consequence; and
* no re-baseline of model outcomes before geometry witnesses explain every moved
  verdict.

---

# DECISION MEMO — for Craig

## The new fact [current inputs can separate both crevices; the earlier impossibility claim was wrong]

The landed distance band has a real defect. On the **MEASURED** raised-column fixture,
a `1.000 mm`-**radius** arm admitted `0 → 20` and `0 → 15` opposite-wall faces at
wall-axis distances `1.00` and `1.25 mm`. Those walls have **INFERRED** final
clearances of `−0.300` and `−0.050 mm` at the `0.300 mm` final demand. They must remain
refusable.

I previously said that no rule from the current inputs could keep the channel floor
legal and both walls refusable. That claim is retracted. The closing review supplied a
counterexample, Rule Z, and re-derived it on both fixtures.

Rule Z asks why each model-surface point is close to the support boundary. It exempts
the point only if every nearest support-boundary point is buried inside the model. A
point inside the support is not exempt on that fact alone. A buried boundary feature can
explain its receiver. A shaft boundary feature standing in air cannot explain a nearby
wall.

**INFERRED**, on the raised-column fixture, Rule Z keeps the seat legal exactly through
`rho = 1.389 mm` and retains the walls at `0.75`, `1.00`, and `1.25 mm`, including the
required `−0.050 mm` margin at `1.25 mm`. **INFERRED**, on the committed channel,
the floor's nearest support-boundary point is buried, while the far wall's nearest
boundary feature is the shaft in air. The floor stays legal and all 11 far-wall faces
stay graded.

## Recommendation [measure Rule Z before #77 merges; price its own-receiver graze]

Make Rule Z the first candidate. It uses inputs that exist today: emitted support
geometry, the model-inside query already used in production, and the grader's clearance
field. It needs no authored labels and no new exact surface-arrangement kernel. Its
pointwise fragment work must be batched.

The price is not zero. Rule Z also refuses a shaft that grazes its own receiver just
outside the occupied footprint. A shallow-angle seat is the binding case. Measure that
class across angle sweeps, curved and folded seats, the Giant Bat, and corpus models
that place model anchors. Record the false-refusal frequency and minimum clearance in
**mm**. Do not hide it behind a new angle or distance constant.

Rule Z is not radially monotone. A changing **radius** can switch the nearest feature
from a buried ball to an exposed shaft. Therefore it must pass Craig's narrowed
thinner-never-worse rule through all four production refusal branches: distance,
foreign transit, degenerate incidence, and grazing emergence. The own-embed transit
whitelist ends at `Iembed`; it may not grow to `Pg` or `Qg`. If any production ladder
fails, Rule Z is not ready to replace the landed band.

## Option A [recommended: hold #77 for Rule-Z evidence]

Keep #77 held. Build Rule Z as an experiment, run both fixtures and the shallow-angle
cost witness, then run six-radius ladders through final, placement, and CCO. If those
gates pass, replace the distance band and merge #77. Price: batched nearest-feature and
inside queries, exact retained fragments, lifecycle tests, and a possible rejection if
the graze cost is material.

## Option B [merge the landed band: accept a measured forgiveness class]

Keep the `feature/f1-refreeze` distance-connected rule and merge #77. Price: explicitly
accept that a final `1.000 mm`-**radius** arm can forgive the **MEASURED** 20 and 15
connected wall faces whose **INFERRED** margins are `−0.300` and `−0.050 mm`. Both
investigated Giant Bat refusals were opposite crevice walls in this class. This option
ships known forgiveness; it does not solve footprint identity.

## Option C [reject the exemption for now: #77 does not merge as landed]

Revert to accept-for-now behaviour, or refuse all seat exemptions. Reverting removes
the landed distance-band claim and leaves #77 unmergeable in its current form while a
replacement is designed. Refusing every exemption makes supports grade against their
own designed embed, so model-seated supports self-refuse and corpus yield falls
sharply; #77 still cannot merge as landed. Price: strict foreign-wall handling at the
cost of useful model seating.

## Partition is future authored content [zero current model contacts; new exact kernel]

Do not choose the receiver partition for #77. Its two invariants are jointly
unsatisfiable on the committed `1.200 mm` channel, and cell clipping can silently
restore self-refusal. Current inputs are unlabelled STL minis. With no unlabelled-mesh
fallback, every model contact on every corpus model is unavailable: zero model-seated
supports, Giant Bat included.

A future authored model format could carry receiver labels, but that option also needs
a new robust exact surface-arrangement kernel on triangle meshes. That is a separate,
large dependency, not a small extension of the current candidate.

## What I ask you to choose [three real options, with #77 consequence]

Choose one:

1. hold #77 and measure Rule Z against both its crevice success and its shallow-angle
   graze cost;
2. merge #77 with the landed distance band and accept the measured connected-wall
   forgiveness class; or
3. reject the exemption for now, so #77 does not merge as landed and model seating
   waits or self-refuses.

My recommendation is option 1. It is the only current-input path that has a working
construction on both required fixtures without silently accepting the known wall
forgiveness.

---

## Appendix A — raised-column fixture generator [preserved evidence, not production code]

This reconstructs challenge F from `f1-heavy-challenge-REJECT.md`. Start with the
watertight `fine_seat_slab` at `0.250 mm` pitch. The probe parameters that reproduce
the section 1 ladder are: axis at the slab centre; creation **radius** `0.350 mm`;
final **radius** `1.000 mm`; `STANDOFF 0.600 mm`; and final demanded gap `0.300 mm`.
For a chosen wall-axis distance, leave the grid column on the wall line at seat height
and raise every later x-column by `2.000 mm`. Moving vertices without adding, deleting,
or rewiring faces preserves the slab's watertight topology. Run distances `0.75`,
`1.00`, `1.25`, `1.50`, `1.75`, and `2.00 mm`.

The same complete source is preserved at
`.git/seal-landing-2026-08-23/raised-column-fixture.rs`. It writes one OBJ per distance
and checks each wall line lies on the `0.250 mm` grid.

```rust
use std::fs::File;
use std::io::{BufWriter, Write};

fn fine_seat_slab_parts(pitch_mm: f64) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    const HALF_MM: f64 = 4.0;
    const DEPTH_MM: f64 = 10.0;
    let span_mm = 2.0 * HALF_MM;
    let steps = (span_mm / pitch_mm).round() as usize;
    assert!(steps >= 4, "top face must be tessellated");

    let mut vertices = Vec::with_capacity((steps + 1) * (steps + 1) + 4);
    for i in 0..=steps {
        for j in 0..=steps {
            vertices.push([
                -HALF_MM + span_mm * (i as f64) / (steps as f64),
                -HALF_MM + span_mm * (j as f64) / (steps as f64),
                0.0,
            ]);
        }
    }

    let top = |i: usize, j: usize| (i * (steps + 1) + j) as u32;
    let mut faces = Vec::with_capacity(2 * steps * steps + 4 * (steps + 1) + 2);
    for i in 0..steps {
        for j in 0..steps {
            faces.push([top(i, j), top(i + 1, j), top(i + 1, j + 1)]);
            faces.push([top(i, j), top(i + 1, j + 1), top(i, j + 1)]);
        }
    }

    let base = vertices.len() as u32;
    for corner in [
        [-HALF_MM, -HALF_MM],
        [HALF_MM, -HALF_MM],
        [HALF_MM, HALF_MM],
        [-HALF_MM, HALF_MM],
    ] {
        vertices.push([corner[0], corner[1], -DEPTH_MM]);
    }
    faces.push([base, base + 2, base + 1]);
    faces.push([base, base + 3, base + 2]);

    let walls = [
        (
            (0..=steps).map(|k| top(k, 0)).collect::<Vec<_>>(),
            base,
            base + 1,
        ),
        (
            (0..=steps).map(|k| top(steps, k)).collect::<Vec<_>>(),
            base + 1,
            base + 2,
        ),
        (
            (0..=steps)
                .map(|k| top(steps - k, steps))
                .collect::<Vec<_>>(),
            base + 2,
            base + 3,
        ),
        (
            (0..=steps).map(|k| top(0, steps - k)).collect::<Vec<_>>(),
            base + 3,
            base,
        ),
    ];
    for (edge, first_corner, last_corner) in walls {
        for k in 0..steps {
            faces.push([first_corner, edge[k + 1], edge[k]]);
        }
        faces.push([first_corner, last_corner, edge[steps]]);
    }
    (vertices, faces)
}

fn raised_column_fixture(
    pitch_mm: f64,
    wall_x_mm: f64,
    wall_h_mm: f64,
) -> (Vec<[f64; 3]>, Vec<[u32; 3]>) {
    const HALF_MM: f64 = 4.0;
    let (mut vertices, faces) = fine_seat_slab_parts(pitch_mm);
    let steps = (2.0 * HALF_MM / pitch_mm).round() as usize;
    let i_wall = ((wall_x_mm + HALF_MM) / pitch_mm).round() as usize;
    assert!(i_wall > 0 && i_wall < steps, "wall must sit inside slab");

    let actual_wall_x_mm = -HALF_MM + i_wall as f64 * pitch_mm;
    assert!(
        (actual_wall_x_mm - wall_x_mm).abs() < 1.0e-12,
        "wall axis distance must lie on pitch grid"
    );

    // Move vertices only. The face connectivity is unchanged, so the
    // watertight fine slab remains watertight. The column at i_wall stays at
    // z=0; every column beyond it rises to wall_h_mm.
    for i in (i_wall + 1)..=steps {
        for j in 0..=steps {
            vertices[i * (steps + 1) + j][2] = wall_h_mm;
        }
    }
    (vertices, faces)
}

fn write_obj(
    wall_x_mm: f64,
    vertices: &[[f64; 3]],
    faces: &[[u32; 3]],
) -> std::io::Result<()> {
    let name = format!("raised-column-{wall_x_mm:.2}mm.obj");
    let mut out = BufWriter::new(File::create(&name)?);
    writeln!(out, "# raised-column wall-axis distance {wall_x_mm:.2} mm")?;
    writeln!(out, "# pitch 0.25 mm; raised height 2.00 mm")?;
    for [x, y, z] in vertices {
        writeln!(out, "v {x:.12} {y:.12} {z:.12}")?;
    }
    for [a, b, c] in faces {
        writeln!(out, "f {} {} {}", a + 1, b + 1, c + 1)?;
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    const PITCH_MM: f64 = 0.25;
    const WALL_H_MM: f64 = 2.0;
    for wall_x_mm in [0.75_f64, 1.00, 1.25, 1.50, 1.75, 2.00] {
        let (vertices, faces) = raised_column_fixture(PITCH_MM, wall_x_mm, WALL_H_MM);
        write_obj(wall_x_mm, &vertices, &faces)?;
        println!(
            "wall-axis={wall_x_mm:.2} mm vertices={} faces={}",
            vertices.len(),
            faces.len()
        );
    }
    Ok(())
}
```

Compile and run the standalone evidence with `rustc raised-column-fixture.rs` from its
evidence directory. The generated OBJ files are run outputs and must not be committed.
