# One load quantity for the whole support pipeline

**Status:** design and implementation plan. No production behaviour is changed by this
document.
**Stance:** whole-pipeline coherence. The engine currently holds nine different notions of
"area per contact" and no two stages agree. This design names ONE quantity, computed once,
before any support exists, that perception, selection, routing, rewiring, bracing, sizing and
the pre-write gate all consume. It says which of the nine are deleted.

---

## 0. Vocabulary, defined once

Terms a reader who has not read this code will need. Each is used only after it is defined
here.

| Term | Meaning |
|---|---|
| **Peel** | The force that pulls a freshly cured layer off the transparent film at the bottom of the resin tank. It happens once per layer, and it is the dominant force in this printing process. |
| **Layer footprint** | The area of the model's horizontal cross-section at one layer. This is the area the film actually grips at that layer. Units mm². |
| **Green** | Resin as printed, before the ultraviolet post-cure bath. Much weaker than any datasheet number. |
| **Orphan** | A piece of a layer's footprint that sits over nothing — no solid material in the layer immediately before it. Peel pulls it and nothing above it holds it. This is what a support exists to hold. |
| **X** | `carried_area / arm_cross_section`, both mm², dimensionless. "How much area one arm is asked to hold, per unit of the arm's own narrowest cross-section." |
| **`X_ALLOWED` = 12** | The design value of X, `floor(25.00 / 2)`, half the measured failure ratio. Registered as `Derived` evidence. |
| **Tip radius** | The RADIUS of a support where it touches the model, in mm. **UNITS TRAP:** this repository and this document speak RADIUS; the owner and the ChituBox user interface speak DIAMETER. Every radius below is stated with its diameter beside it. |
| **Scar** | The mark a support leaves on the model. Its area is the tip's cross-section, `pi * r^2`. |
| **Demand** | The new quantity this design introduces. Defined in §2. |

---

## 1. What is actually wrong today

Three findings, all measured in this repository during the ground-truth phase of this work.

**1.1 The engine holds nine notions of "area per contact" and three of them are dead
weight.** The full census is in §7. The two that matter here:

* `ContactCandidate.supported_area_mm2` is the area of the single mesh triangle a candidate
  sits on. It is written, range-checked, copied and hashed into determinism digests — and
  read by **no comparison, no sort key, no score and no radius anywhere in the workspace**.
  It cannot rank anything, and measurement confirms it: on the `make_overhang_table` fixture
  all 1089 candidates carry the identical value 450.000 mm², because every candidate sits on
  one of the two 450 mm² underside triangles. 63 are selected anyway, by a sort on Z.
* `FinalizedFaceLoad.carried_area_mm2` is the quantity that sizes every tip. A face is
  eligible for it only by *provenance* — it must belong to an overhang island or be named by
  a reservation — and each eligible face is given **whole** to its nearest contact, because
  `finalize_load_rows` hard-codes `CarriableMaximum::SingleCarrier`.

**1.2 The consequence is a 20x inversion in tip diameter on one flat plane.** On
`make_box` — a 10x10 mm flat underside, two triangles — the engine emits 11 supports: three
carry 50.0 mm² each and print at **2.000 mm diameter** (the clamp maximum; the law wanted them
larger still), and the other eight carry 0.0 mm² and print at **0.100 mm diameter**. On the
overhang table, 71 of 85 emitted supports carry zero, while the two most heavily loaded
supports in the whole run — 450.0 mm² each — print at the registered **minimum** 0.100 mm
diameter where the law demands 2.000 mm. That is not a spread. It is an inversion, and it is
the owner's reported "weird thick/thinness".

**1.3 Refining the mesh makes supports thicker, not thinner, and by a factor of 17.**
Subdividing `make_overhang_table` twice (identical solid, 60 -> 240 -> 960 triangles) leaves
the surface area at exactly 3688.0 mm² and divides `supported_area_mm2` by exactly 16 —
changing nothing, because nothing reads it. Meanwhile the *upper-quartile emitted tip
diameter* moves 0.100 -> 0.461 -> **1.727 mm**. The mechanism is that the eligibility set is
made of mesh faces: island face count goes 2 -> 8 -> 30, so more of the model becomes visible
to the load model and fewer contacts are left carrying nothing.

**The root cause common to all three: the load model is built out of triangles, and it is
built after the supports exist.** Triangles are an accident of how the file was exported.
And a quantity that does not exist until supports exist cannot possibly decide where supports
go.

---

## 2. The quantity

> **Peel demand.** For one support, the largest single-layer orphan footprint area, in mm²,
> whose peel load has no path to the plate except through that support.

Formally, for layer `k` at height `z_k` with layer height `h`:

```
S_k          = the set of solid cross-section regions of the model at layer k
O_k          = { p in S_k : reach(p, S_{k-1}) > K * h }        the ORPHAN set at layer k
K            = sqrt(X_ALLOWED / 6) = sqrt(2) = 1.41421
reach(p, S)  = distance in the layer plane from p to the nearest point of S (0 if inside)
```

`O_k` is the part of layer `k` that is over nothing and is too far from carried material to
be held sideways by its own layer. Every point of `O_k` is charged to exactly one support
(§3). A support's demand is then

```
demand(c) = max over k of ( area of the part of O_k charged to c )      [mm^2]
```

and a structural member's demand is the sum of the demands of the supports in its subtree
(§5.3 states why sum, and what would replace it).

### 2.1 Why this quantity and not surface area

**It is the quantity the plates measured.** Every plate-5 and plate-6 specimen was a flat
disc cap on an arm. A flat horizontal cap's whole area appears in **one layer**, so for those
specimens *cap surface area* and *peak single-layer orphan footprint* are the same number.
`X_STAR_CONE ∈ [25.00, 25.34]` and `X_STAR_CYLINDER ∈ (19.91, 30.48)` were therefore measured
in footprint units. Adopting footprint keeps the calibration honest on the specimens it came
from and extends it in the one direction the specimens could not distinguish.

**It is what the film actually grips.** The separation force acts on the area of the interface
being pulled off the film, which is the layer's cross-section — not the surface area behind
it. Cao: *"separation forces can reach several hundred newtons, whereas gravity typically does
not exceed 10 newtons."*

**The engine already reasons this way, in the one place it reasons about load at all.** The
low-point reach criterion (`perception::region_reach_mm`) is per-layer by construction: *"at a
non-birth low point one layer of green resin cures with nothing beneath it, joined sideways
within its own layer to resin that IS sitting on the previous layer, and the peel pull bends
that tongue at its root."* `O_k` is that same sentence, applied to every layer instead of only
to detected local minima.

**It sees hollowing for free.** `SliceSolid::area_mm2()` already subtracts hole areas. A
hollowed shell's layer footprint is the ring, not the disc, because the film never touches the
cavity. So the load model automatically distinguishes a hollow shell from a solid — which
today's engine cannot do at all.

**It sees cavity ceilings for free.** A cavity roof is new solid appearing where a hole was.
That is orphan material by the same rule, with no special case. This satisfies the standing
ruling that cavity ceilings must be supported from the cavity's lower interior.

### 2.2 Where this goes beyond what was measured — stated, not buried

1. **Footprint versus surface area is UNMEASURED.** Both plates used flat caps, where the two
   are equal. Nothing in this repository distinguishes them. §8 pre-registers the coupon that
   does. **Direction of the error if I am wrong:** this design under-sizes supports under
   *tilted* overhangs, where footprint is much smaller than surface area. It is exactly right
   on flat ones. Until the coupon is read, §9 phase 2 runs the new quantity in shadow beside
   the old one and refuses to ship a *reduction* on any tilted face.
2. **The self-support angle is derived, not measured.** §4.1. Corroborated by universal
   practice, unmeasured here.
3. **X is applied uniformly across footprint sizes** even though this project's own two peel-
   pressure anchors differ by 54–81x with contact geometry (a slender 0.05 mm finger at
   `<= 1.24 kPa`; a 12.5 mm flat disc at `~101 kPa`). A per-layer orphan strip on a tilted
   face is a slender shape, and the slender regime has the *lower* pressure — so the flat-cap
   X applied to a strip is **conservative**. Below 3.14 mm² (the smallest cap ever printed) it
   is extrapolation, and at that size the tip minimum binds anyway.
4. **No weight and no inertia term.** Justified by arithmetic, not by any plate: for a
   constant-wall shell the weight-to-peel ratio is *size-invariant* (mass ~ rho·S·t and peel ~
   p·A both scale as L², so L cancels) and lands at 0.11 % at 101 kPa, 0.87 % at 12.5 kPa and
   8.8 % at the most weight-favourable 1.24 kPa anchor. Even a solid part never crosses over
   inside this printer's build volume. The enumeration this relies on is stated in §6.4.
5. **No plate-position term.** The only number in the repository is a `<= 1.53x`
   *non-detection* at plate 6's ladder resolution. This design ignores position, and says so
   as an assumption rather than a finding.

---

## 3. The allocation

Every unit of orphan area lands on exactly one support, or is named. There is no third
outcome.

### 3.1 The grid, and why its pitch is derived

`O_k` is sampled on a fixed square grid of pitch `q`, aligned to the build volume and shared
by every layer. `q` is **derived, not tuned**:

```
q = sqrt( X_ALLOWED * pi * TIP_RADIUS_DEFAULT_MM^2 )
  = sqrt( 12 * pi * 0.050^2 ) = sqrt(0.094248) = 0.3069 mm
q registered = the largest binary fraction of a millimetre under that bound = 0.25 mm
```

The rule: *one grid cell must never carry more than the smallest permitted tip can hold.* If
a cell could exceed 0.0942 mm² the partition could hand a minimum tip an over-load it cannot
subdivide. Rounding down to a binary fraction keeps the grid exactly representable and its
subdivision exact. Nothing about `q` is a taste judgement, and if `X_ALLOWED` or
`TIP_RADIUS_DEFAULT_MM` move, `q` moves with them.

Note that the engine's existing `VOXEL_PITCH_MM = 0.4` is **too coarse** to serve here — it
exceeds the bound — so this is a new registered value derived from two existing ones, not a
reuse.

### 3.2 Building `O_k`

Per layer, over the normalized slice solids the engine **already computes**
(`perception::normalized_slice_layers` produces `Vec<Vec<SliceSolid>>`, and `find_births`
already walks that stack pairwise):

1. Mark the cells whose centres lie inside a layer-`k` solid. (`solid_contains` exists.)
2. Of those, the ones also inside a layer-`k-1` solid are **carried by the model** — discard.
   This is the cheap test and it eliminates the interior, which is most cells.
3. For the thin remaining band, evaluate `reach` exactly against the layer-`k-1` polygons
   (`point_reach_mm` exists). Cells with `reach <= K*h` are self-carried — discard.
4. What remains is `O_k`, as a sparse list of `(ix, iy)` cells, each worth `q²` mm².
5. **Rescale for exactness.** Compute each orphan region's exact polygon area by shoelace and
   scale its cells so the cell sum equals the exact area to the last bit. This removes the
   grid's quadrature bias entirely and makes conservation (invariant I1) hold by
   construction rather than to a tolerance.

Step 2 is what makes this affordable: the expensive distance query runs only on the newly
exposed band, which is `O(perimeter / q)` cells per layer.

### 3.3 Charging cells to supports — geodesic Voronoi inside the layer

Load travels through the green layer's own material, not through air. So:

> An orphan cell at layer `k` is charged to the support seat that is nearest **measured
> through solid material within layer `k`** (an eight-neighbour shortest path over the cells
> of `S_k`), among seats already standing at or below layer `k`.

This is a multi-source shortest-path sweep over the layer's cells — `O(cells)` per layer with
a bucketed queue — and it has four properties the current rule does not:

* **It never charges across a gap.** A cell on one limb cannot be charged to a support under
  a different limb, even if that support is closer as the crow flies. Today's nearest-contact
  rule has exactly that hole.
* **It splits naturally.** A large orphan region is divided among all the supports under it,
  in proportion to how much of it each is nearest to. No `SingleCarrier` special case, no
  `ceil(area/maximum)` equal-shares rule, no merge-tournament realignment problem
  (`perception.rs` block comment at `finalize_load_rows`, tasks #68/#69/#70): the multi-carrier
  question dissolves because the unit of allocation is a 0.0625 mm² cell, not a mesh face.
* **A cell with no reachable seat is a NAMED REFUSAL, not a silent zero.** This adopts the
  deleted auditor's one genuinely valuable idea — *"a missing load partition is a named
  failure, never a plausible zero score"* — and inverts today's behaviour, which records
  `carried_area_mm2 == 0` and prints a minimum tip.
* **It is keyed by canonical contact id, never a positional index.** The ledger refuses an id
  it does not know. This closes one of the two candidate mechanisms for the measured 20x
  inversion (`specialized.rs:1611` matches by `contact_id` against a positionally assigned
  id).

### 3.4 Demand is derived, never stored

**Rule R — the single most important rule in this design.**

> A support's demand is a function of (solid geometry, layer plan, seat set, forest topology).
> It is never a number that survives a plan edit.

The ledger owns the charge map. Every edit goes through it:

* **Removal** (an unrouted or abandoned contact): its cells are re-charged by re-running the
  sweep over the affected layers with that seat withdrawn. Cells that now reach no seat become
  uncarried demand, which either triggers a repair seat or a named refusal.
* **Append / repair:** the new seat enters the sweep and *takes* cells from its neighbours,
  whose demand falls. Today's append path measured 60 of 60 appended contacts landing at
  exactly `carried_area_mm2 == 0.0`, because the incremental ledger could only give an
  appended contact what was already disclosed as ownerless. Under Rule R that failure mode
  cannot be written down.
* **Rewire:** member demands are re-folded from the leaf charges up the new topology.
  `slim_rewire_ancestors`' subtract-on-detach and `propagate_carried`'s add-on-attach become
  *derived* results that the ledger recomputes, and invariant I5 asserts the incremental value
  equals a from-scratch recomputation bit for bit.

Rule R is the fix for the observed inversion. Today a tip is sized from the *prepared* plan's
partition (`initial.rs:668`) while the disclosure reads the *accepted* plan's re-partition
(`runner.rs:1252`), and nothing re-sizes a tip after routing removes 10 contacts. Under
Rule R there is no "after": demand and radius are recomputed together, or neither is.

---

## 4. Site selection

The owner asked: *"Does the spot-selection where the supports land on the model not
effectively decide 'we need a support here in order to carry this area?'"*

**Yes, and today it cannot, for a structural reason: the load quantity does not exist yet
when placement runs.** `carried_area_mm2` is a partition over contacts, so it is undefined
until contacts exist. That is why placement falls back on a sort by Z and a fixed 4.0 mm
Poisson-disk spacing.

**Peel demand is a property of the sliced solid alone. It exists before any support does.**
That is the architectural inversion this design turns on, and it is what makes load-driven
placement possible at all.

### 4.1 The demand field also decides WHETHER a face needs support

Today a face is an overhang candidate iff `normal[2] < -sin(OVERHANG_ANGLE_DEG)` with
`OVERHANG_ANGLE_DEG = 45.0`, a `Placeholder` with no measurement behind it. The demand field
replaces that test with a derived one. A planar surface at angle `beta` from horizontal
advances `h / tan(beta)` horizontally per layer, so it is self-carried exactly when

```
h / tan(beta) <= K * h     <=>     tan(beta) >= 1/K = 0.70711     <=>     beta >= 35.264 deg
```

Note `h` cancels: **the self-support angle is independent of layer height**, which is a real
prediction and a check on the derivation.

So the physics-derived threshold is 35.26° from horizontal, and the registered placeholder is
45°. The registered value is the *more conservative* of the two — it flags the 35–45° band
that the reach law says is self-carried. This design therefore **retires the constant and
keeps the conservative behaviour until §8's ramp coupon is read**, by taking `O_k` as defined
and additionally admitting anything the 45° mask flags, then measuring how much of the extra
is genuinely zero-demand. Two answers to "does this need support" currently live in the tree,
ten degrees apart; after this change there is one, and it is derived from `X_ALLOWED`.

### 4.2 The objective function, written down for the first time

The prior-art review found that nobody had ever written it. Here it is.

The scar invariant (`docs/physics.md`) says total scar area is `A / X` **regardless of how you
tile it** — thin-and-dense and thick-and-sparse leave identical total scar. So *minimising
total scar cannot be the objective*: it is a constant. What is left:

```
minimise    sum over contacts of  delicacy(seat_c) * pi * r_c^2        (put scar where it hurts least)
then        minimise the number of contacts                            (fewer, cleaner, less resin)

subject to  (a) every orphan cell is charged to some contact
            (b) charge(c) <= X_ALLOWED * pi * r_c^2                     the area law
            (c) r_c <= r_ceiling                                        the owner's scar ceiling
            (d) seat_c is admissible                                    PR-1..PR-5 as today
```

`r_ceiling` is the owner's field ceiling, `TIP_RADIUS_MAX_GROWN_SCAR_MM` = 0.400 mm radius
(**0.800 mm diameter**) — today that constant bounds only growth-for-attachment. Making it the
placement ceiling as well is a **policy choice for the owner**, not a physics claim, and §4.4
shows exactly what it costs. `TIP_RADIUS_MAX_LEGACY_MM` = 1.000 mm radius (2.000 mm diameter)
remains the hard bound.

### 4.3 The placement algorithm

Per orphan region, per layer band:

1. `D` = the region's total demand. `n_min = ceil( D / (X_ALLOWED * pi * r_ceiling^2) )`.
   This is a *derived* lower bound on the support count — a real answer to "we need a support
   here in order to carry this area".
2. Seed `n_min` seats on a hexagonal lattice over the region's geodesic metric, snapped to the
   nearest admissible seat, biased away from high delicacy. (The hexagonal lattice is the
   engine's own recorded placement algorithm of record, from the migration plan, and has never
   been implemented.)
3. Assign cells by the §3.3 sweep.
4. While any seat is over capacity, insert a seat at the over-capacity seat's farthest charged
   cell and re-assign. Each insertion strictly reduces the maximum load, so this terminates.
5. Size: `r_c = sqrt( charge(c) / (X_ALLOWED * pi) )`, raised to `TIP_RADIUS_DEFAULT_MM`.

`TARGET_SPACING_MM = 4.0` becomes a **seed hint for step 2**, never a decision. That is
open task #58, closed.

### 4.4 What that predicts, in numbers you can check against the owner's practice

Hexagonal packing puts one seat per `(sqrt(3)/2) s^2 = 0.8660 s^2` mm² of a flat down-face.
Setting that equal to one tip's capacity `X_ALLOWED * pi * r^2` gives a **derived spacing
law**:

```
s = r * sqrt(12*pi / 0.8660) = 6.598 * r
```

| tip radius | tip DIAMETER | capacity mm² | derived hex spacing |
|---|---|---|---|
| 0.050 mm (registered minimum) | 0.100 mm | 0.094 | 0.33 mm |
| 0.350 mm (`min_shoot_radius`) | 0.700 mm | 4.62 | 2.31 mm |
| **0.400 mm (owner's field ceiling)** | **0.800 mm** | **6.03** | **2.64 mm** |
| 0.606 mm (implied by today's 4.0 mm spacing) | 1.212 mm | 13.86 | 4.00 mm |
| 1.000 mm (registered maximum) | 2.000 mm | 37.70 | 6.60 mm |

Read the fourth row: **today's default 4.0 mm spacing implies a 1.21 mm diameter tip on a flat
down-face — half again the largest support the owner has ever needed in ChituBox.** That is
the pipeline disagreeing with itself in one line, and this design makes the owner's own
ceiling pick the spacing instead.

Worked cases, all arithmetic from the table above:

* **`make_box`, 10x10 mm flat underside = 100 mm².** Required total scar = 100/12 = 8.33 mm².
  At the owner's ceiling: 17 supports at 2.64 mm spacing, each 0.800 mm diameter. *Today the
  engine emits 11 supports whose total tip area is 9.49 mm² — enough in aggregate, but 8 of
  them carry nothing and print at 0.100 mm diameter while 3 sit at the 2.000 mm clamp.* Same
  total scar, distributed correctly.
* **`make_overhang_table`, 30x30 mm top = 900 mm² in one layer.** Required total scar
  = 75 mm². At the owner's ceiling: 149 supports at 2.64 mm. At the registered maximum: 24
  supports at 6.60 mm. Today: 63 seats at 4.0 mm, which the law would size at 1.23 mm
  diameter — and 71 of the 85 emitted carry zero.
* **A solid 30 mm sphere.** Its first layer is a cap of height 0.05 mm, radius
  `sqrt(2*15*0.05 - 0.05^2) = 1.224 mm`, footprint **4.71 mm²**. Required tip radius
  `sqrt(4.71/(12*pi)) = 0.353 mm` — **0.707 mm diameter, one support**. That is the answer any
  experienced operator gives for a sphere, produced from the area law with nothing tuned.
* **A horizontal solid arm, 10 mm diameter, 40 mm long.** Its first layer is a strip
  `2*sqrt(2*5*0.05 - 0.05^2) = 1.411 mm` wide by 40 mm = **56.4 mm²**. Required total scar
  4.70 mm² -> **10 supports at 0.800 mm diameter, about 4 mm apart along the arm**. Compare the
  same arm's *surface* area, which is `pi*10*40/2 = 628 mm²` for the lower half — 11x larger.
  This is the case where footprint and surface area diverge and it is where §8's coupon bites.

---

## 5. Sizing

### 5.1 The law, read from the registry

```
r_tip = sqrt( demand / (X_ALLOWED * pi) ),   clamped up to TIP_RADIUS_DEFAULT_MM
```

`X_ALLOWED` is read from the evidence registry. This **retires `FrozenTipLoadPolicy` as a
sizing input.** Today the shipped law is a legacy stress form
`r = sqrt(A*P*SF/(pi*sigma))` built from four `Placeholder` constants that the registry itself
labels *"legacy diagnostic, potentially about four times low"*. Measurement shows those four
currently multiply out to `sigma/(P*SF) = 0.288e6/(3000*8) = 12.000000` **exactly**, and
`tip_radius_mm(A)` matches `sqrt(A/(12*pi))` to six decimals at every area where the clamps do
not bind. So the shipped law *is* the measured area law today — **by coincidence of four
unmeasured placeholders, with no test tying it to `X_ALLOWED`.** Task #63 records that two of
those four jointly imply failure at ~0.007 % strain, about 1000x off plausible. If either
moves, every arm on every model re-sizes against a ratio with no relation to the measured snap
boundary and nothing in production notices.

This is `REVIEW.md` short-list 6, option (a), taken. `FrozenTipLoadPolicy` survives only as a
diagnostic, and `implied_area_ratio() <= X_ALLOWED` moves out of `#[cfg(test)]` into its
constructor.

### 5.2 The failure section is the narrowest section, and that must be an invariant

Plate 5, owner's observation: on **every** large cap the break was at *"the base of the
cylindrical portion where it meets the conical support"* — the arm's narrowest constant-
diameter run, not the disc where it touched the cap.

For the engine's stub the taper runs from `r_tip` at the model outward to
`max(r_tip, min_shoot_radius = 0.35 mm)` rootward, so it flares monotonically and its minimum
section *is* the tip end. **That is a coincidence of the current geometry, not a guarantee**,
and rewiring, junction solves and the transient sizing pass all move radii independently. So
the design states it as an obligation:

```
for every contact c, and every point s on the load path from c to the plate:
        section_area(s) >= demand_at(s) / X_ALLOWED
where   section_area(s) = pi * r(s)^2                 (a frustum's minimum is its smaller end)
        demand_at(s)    = the summed demand of the subtree below s
```

The binding point is normally just rootward of a junction, where the parent's demand has
jumped but its radius has not yet — exactly the geometry plate 5 broke at. A locator
`min_section_on_path()` is new code; the engine has no general one today
(`03-physics-and-constants.md` §3.2 records that the fast-flare term does not exist, and
`contact.rs:348` reasons about the cut disc for ball-tipped stubs only).

**A note on Murray's law, which is where this bites.** `member_sizing` maxes four candidate
radii, one of which is the fan-in `(sum r_child^4)^0.25`. For two equal children Murray gives
`2^0.25 r = 1.189 r` where the area law demands `sqrt(2) r = 1.414 r` — Murray is 16 % thin per
junction and compounds with depth. Today the peel-tension term is `max()`'d alongside it and
wins, so nothing breaks. Under this design the invariant above is what *guarantees* that,
rather than leaving it to whichever of four `max()` arms happens to be largest.

### 5.3 Members: sum of peaks, and the refinement that would replace it

A member's demand is the **sum** of its subtree's peak demands. This is conservative and it is
what `propagate_carried` already does.

It is conservative because different supports are orphaned at *different layers*, and peel is
never simultaneous across layers. The physically exact quantity is the **maximum over layers
of the sum of the subtree's per-layer charges** — max-of-sums, not sum-of-maxes. Computing it
needs each contact to keep a sparse per-layer profile and a sweep-line over layer events,
`O(contacts log contacts + layers)` per re-fold. That is a named, measurable follow-up
(the instrument: total member volume in millilitres on the corpus, before and after), not part
of the shipping design, because plate 6 gives load sharing **no measured upper bracket** — no
strip specimen ever failed — and this design does not build on the unbracketed side.

### 5.4 Bracing carries zero, and now provably

A brace is not on any path from an orphan cell to the plate — `Forest::add_brace` removes it
from `roots`, `set_parent` refuses it as child or parent, and `live_tree_members` filters it
out. So no cell can be charged to it. Today that zero is a decision defended by a 30-line
comment; under this design it is a consequence of the allocation rule, and invariant I9 tests
it. Raft members are the same. **The corollary is sharp: after this change, a zero-demand row
in the emitted set is always a defect, never scaffolding.** That is the classification the
ground-truth phase could only establish by hand.

---

## 6. Does it produce the behaviour the owner sees?

The owner: *"lower-positioned supports ought to be much thicker / more load-bearing than the
higher-up ones; supports on thin arms should support less than 'core' supports for wide
areas."*

### 6.1 Thin arms carry less than wide cores — yes, directly, by mechanism

A thin arm's per-layer orphan footprint is its own small cross-section. The 10 mm arm above
orphans 56.4 mm² *spread over 40 mm of length*, so each of its ~10 supports carries 5.6 mm²
and prints at 0.80 mm diameter. A 30x30 flat core underside orphans 900 mm² in one layer; if
the operator chose 24 supports they would each carry 37.5 mm² and print at the 2.00 mm maximum.
**A 2.5x ratio in tip diameter between an arm support and a core support, with nothing tuned.**
The mechanism is that footprint scales with the *cross-section* of the feature, which is
exactly the owner's intuition stated in physics.

### 6.2 Lower members are thicker — yes, by accumulation. Lower TIPS are not, and should not be

Two different claims hide in the owner's sentence and the design answers them differently.

* **Members:** a member near the plate carries the summed demand of everything above it
  (§5.3), so `r ~ sqrt(sum of demands)` grows monotonically rootward. That is the mechanism,
  and it is already how `propagate_carried` behaves.
* **Tips:** a tip's demand is its own orphan footprint, and that does **not** depend on how
  high up the model it sits. I decline to add a height term for tips, because peel acts at
  the current layer only and nothing in the evidence supports one. **This is a place an
  implementer should push back if the owner's observation is really about tips.**

There *is* a genuine height effect, and it is a **bending** load, not a peel load: during
tilt-release the whole part is dragged sideways, and a low member has a longer lever arm. The
engine already has terms for it — `diagnostic_deflection_radius_mm` and the transient
`N_terminals^0.125` shoot factor. The coherence rule is: **one peel quantity, consumed by the
bending terms as an input where they need one; the load model does not double as a bending
model.** That keeps the three unreconciled load-to-radius power laws
(`docs/evidence/physics_load_sizing.md` §4 issue 2) from silently competing.

### 6.3 Steep faces get nothing, flat faces get everything

Falls out of `O_k`: a face steeper than 35.26° from horizontal advances less than the reach
allowance per layer and produces no orphan cells at all.

### 6.4 The weight enumeration this relies on

Excluding weight is defensible only against a stated enumeration. It is: (a) whole-part weight
is 0.1–9 % of peel for any hollowed shell, size-independent; (b) whole-part weight for a solid
never reaches peel inside this build volume — crossover diameter is 171 mm at the most
weight-favourable pressure anchor against a 118 mm axis; (c) the worst per-arm concentration
enumerated is a horizontal solid limb, `V/A_down = pi*d/4`, giving 6.9 % at 10 mm diameter even
at that same anchor. **Not enumerated: the inertial term**, because the printer datasheet lists
lift speed, lift distance, retract speed and acceleration as "Not specified" and nothing in
this repository bounds it. If a heavy solid limb ever fails on a support the law says is
adequate, that is where to look first.

---

## 7. Which notions of area this RETIRES

Adding a tenth without removing nine makes this worse. The full census, with a verdict on
each.

| # | Site | What it is today | Verdict |
|---|---|---|---|
| 1 | `ContactCandidate.supported_area_mm2` (`lib.rs:984`) | one mesh triangle's area | **DELETE.** Read by no decision anywhere; only digests move. |
| 2 | same field on the dyadic path (`perception.rs:1145`, `specialized.rs:2207`) | the whole face's area stamped on *every* sub-face nomination — a latent double-count by the subdivision factor | **DELETE with #1.** |
| 3 | `SelectedContact.supported_area_mm2` (`lib.rs:1515`) | verbatim copy of #1 | **DELETE.** |
| 4 | `LoadEligibility.area_mm2` -> `FinalizedFaceLoad.carried_area_mm2` | face-provenance nearest-contact partition, whole faces only | **REPLACE.** The field name `carried_area_mm2` survives on contacts and members; its producer, its definition and its units of allocation all change. |
| 5 | `OwnerlessLoadFace` | disclosure of eligible rows that found no contact — reports 0 on both fixtures while 75.6 % of the model's area has no eligibility row at all | **REPLACE** with `UncarriedDemand`, and **promote from disclosure to named refusal**. |
| 6 | `AbandonedContactDisclosure::abandoned_area_mm2` | #4 for a contact PR-8 quarantined; load that vanished | **REPLACE.** Under Rule R the cells are re-charged, and only genuinely unreachable area is uncarried. |
| 7 | `EmittedContactDisclosure.carried_area_mm2` | #4 read off the accepted plan and joined by contact id to a tip radius from the final forest, as two independently sourced numbers | **KEEP as the one disclosed quantity**, sourced from the ledger, with the id join made canonical. |
| 8 | `CoverageSample.area_mm2` (`dyadic.rs:416`) | `triangle_area / dyadic_cells` — a sampling weight for coverage repair ranking | **KEEP in phase 1; RETIRE in phase 4.** Under this design "is the surface covered" and "is all demand charged" are the same question, and `UncarriedDemand == 0` answers both. Retiring it is the largest coherence win available and also the riskiest change, so it is staged last and gated on the two judgments agreeing on the corpus. |
| 9 | `IslandBirth.area_mm2` | the birth solid's exact slice area — computed and then **discarded** in favour of a 1.5 mm face-ball proxy (`physics_contacts_tips.md` issue 3) | **RETIRE the proxy.** A birth's area *is* its layer-0 orphan footprint; it is the same quantity, already in the same units. |

Also retired: **`OVERHANG_ANGLE_DEG = 45.0`** as a decision (§4.1); **`TARGET_SPACING_MM = 4.0`**
as a decision, demoted to a search seed (§4.3, task #58); **`MIN_CONTACTS_PER_ISLAND`**,
subsumed by `n_min = ceil(D / capacity)`; and **`LoadOwner` / island-provenance eligibility**
entirely — which also removes a second, unnoticed tessellation dependence, since islands are
today ordered by *triangle count* (`b.len().cmp(&a.len())`) and island id becomes
`LoadOwner::GenericIsland(id)`.

**Net: nine notions in, one out.** The one is `demand(contact)`, in mm² of peak single-layer
orphan footprint.

Two things this design deliberately does **not** invent, because they already exist and are
merely switched off: `CarriableMaximum::AreaLaw`, `carriable_share_count`,
`charge_face_from_takers` and `equal_shares_mm2` (tasks #68–#70). Those solve "how do I split
one large *face* among several contacts". Under this design the unit of allocation is a
0.0625 mm² cell, so there is nothing left to split and that machinery is deleted rather than
enabled — including `SpecializedLoadBaseline::assemble`'s row-indexed, one-winner-per-row,
three-witness representation, which the block comment at `finalize_load_rows` names as the
blocker for multi-carrier. **This design's cost is that representation's rewrite; its return
is that the rewrite is a deletion, not a generalisation.**

---

## 8. Testability — how anyone would know this is right

### 8.1 In-engine, no printing, and these are the acceptance gates

| # | Invariant | How it fails today |
|---|---|---|
| **I1** | **Conservation.** For every layer `k`: `sum over contacts of charge(c,k) + uncarried(k) == area(O_k)`, exactly (the §3.2 rescale makes this bit-exact, not tolerant). | No equivalent exists. 75.6 % of the overhang table's area has no row at all and `ownerless` still reports 0. |
| **I2** | **Exclusivity.** Every orphan cell appears in exactly one charge. | Faces are whole-assigned; ineligible faces appear in none. |
| **I3** | **Law at the minimum section.** For every emitted contact and every member on its path: `pi*r_min_section^2 >= demand_there / X_ALLOWED`. Grown-for-attachment tips exempt by name. | `prepare_folder.rs:445` asserts exactly this — **on the "mini" fixture only**, which passes. The identical check on `make_overhang_table` fails: 2 contacts below the law, worst shortfall 0.950 mm of radius. |
| **I4** | **Tessellation invariance.** `subdivide_once` (midpoint split, welded on exact bits) yields the identical solid, hence identical slice polygons, hence a **bit-identical demand field and bit-identical tip radii**. | Measured today: median tip diameter 0.100 -> 0.100 -> 0.631 mm, upper quartile 0.100 -> 0.461 -> 1.727 mm over two subdivisions. |
| **I5** | **No stale demand.** After any removal, append or rewire, the incremental ledger equals a from-scratch recomputation bit for bit. | The measured 20x inversion is exactly this failing. |
| **I6** | **No zero-demand emissions.** Every emitted contact carries `> 0`, or carries a declared non-load disposition. | 8 of 11 on `make_box`; 71 of 85 on the overhang table. |
| **I7** | **Hollow monotonicity.** For the same outer shape, `demand(hollowed) <= demand(solid)` per layer, and the ratio equals the ring-to-disc area ratio. | Nothing in the engine distinguishes hollow from solid when sizing. |
| **I8** | **Scar invariant.** `sum pi*r_tip^2 == total demand / X_ALLOWED + clamp slack`, independent of the number of seats chosen. | Not asserted anywhere. |
| **I9** | **Braces and raft members carry zero by construction**, and no cell can be charged to one. | True today, but by comment rather than by test. |
| **I10** | **Determinism and independence.** The demand field's digest is a function of (mesh fingerprint, grid pitch, layer plan, `X_ALLOWED`) only — **not of the contact set**. Perturbing the seat set must not change the field, only the charges. | Not applicable; today's quantity does not exist without contacts. |

Every one of these is a *sabotage* target: each ships with a test that perturbs the ledger and
asserts the gate goes red. A gate that has never been seen red is not a gate.

### 8.2 The one coupon that decides this design — pre-registered

**COUPON-PLATE-7-footprint-vs-surface.** The central claim of §2 is that a support carries
*layer footprint*, not *surface area*. Plates 5 and 6 cannot distinguish them because every
specimen was a flat cap. This is the discriminating design:

* Three cap tilts — 0° (flat), 45°, 70° from horizontal — at **matched cap SURFACE area**, on
  **identical arms** at a fixed X computed from the flat-cap area.
* Surface-area theory predicts: all three fail together, because all three present the same
  surface area to the same arm.
* Footprint theory predicts: the flat cap fails; the tilted caps spread their footprint over
  hundreds of layers and survive by a wide margin.
* **Discriminant, computed for a 12.5 mm diameter cap at 0.05 mm layers.** A tilted disc's
  peak single-layer footprint is its widest chord times the projected width of one layer's
  band, `2a * (h/sin(theta)) * cos(theta)`:

  | tilt from horizontal | peak single-layer footprint | relative |
  |---|---|---|
  | 0° (flat) | 122.72 mm² | 1 |
  | 45° | 0.625 mm² | 0.00509 |
  | 70° | 0.228 mm² | 0.00185 |

  A factor of **539** between the flat and the 70° cap at identical surface area. There is no
  reading of the outcome that is ambiguous.
* Controls the plate must carry, from this project's own rules: a must-survive positive
  control, a must-fail negative control, matched +/-Y pairs so position cannot be confounded
  with tilt, and **no factor assigned by a position formula**. Record the resin vendor and
  batch and the ambient temperature — neither plate 5 nor plate 6 did, and `X_ALLOWED = 12` is
  consequently conditional on an unidentified resin.
* **Do not score any feature above z ~ 9 mm.** Plate 6's family A6 died there for an
  unidentified reason and that standing rule constrains every new plate.

Two cheaper reads that may not need a new print at all, if the physical plates survive:
**plate 1 family D** (peel direction, 3 directions x 5 Y stations x 4 lengths) and **plate 5
rows R1/R5** (a matched +/-Y pair) were both *built* to answer the position question and were
**never scored**. Reading them is faster than printing anything.

### 8.3 Second and third coupons, in priority order

* **COUPON-PLATE-8-ramp-ladder.** Tests §4.1's derived 35.26° self-support angle: a ladder of
  planar ramps from 20° to 50° in 2.5° steps, unsupported, at matched +/-Y. The derived law
  predicts a clean transition at 35.3° and — the sharper prediction — **that the transition
  angle does not move when layer height changes**, because `h` cancels. Run at 0.03, 0.05 and
  0.08 mm. This is the only layer-height evidence the project would have; today there is none.
* **COUPON-PLATE-9-small-cap-ladder.** Cap areas from 0.10 to 3.14 mm², below the smallest cap
  ever printed, to close the bottom 1.5 decades that this design extrapolates over. It also
  tests whether X is a ratio or a threshold down there, which plate 6's single cap area left
  algebraically degenerate.

### 8.4 The corpus measurement to take before anything is implemented

Nobody has the histogram. Run the *current* engine on the real corpus with instrumentation
only, and record per model: contact count, eligible-row count, non-zero-carry count, and the
**distribution** of carried area — median, quartiles, maximum. A Weeping Treant has 558
contacts. That histogram says whether today's carried areas land inside or outside the
measured cap-area band of 3.14–122.72 mm², and it is the baseline every claim in §9 is
measured against.

---

## 9. Implementation plan

Five phases. Each ends at a committed, green, reviewable state, and only phase 4 changes what
gets printed.

**Phase 0 — baseline, no code.** Take §8.4's corpus histogram. Freeze it as evidence. Extend
`prepare_folder.rs:445`'s law-agreement assertion from the "mini" fixture to every corpus
model and **record how many models it reddens** (it will redden some — that is the point) as
`#[ignore]`d evidence rather than a failing build.

**Phase 1 — the field, computed and disclosed, deciding nothing.**
New crate module `plinth-support::demand`. Build `PeelDemandField` from the slice stack
perception already produces. Emit it as a sidecar. Assert I1, I2, I4, I7, I10. Nothing reads
it. **This is the phase that proves tessellation independence before anything depends on it.**

**Phase 2 — the ledger, shadowing the partition.**
`PeelDemandLedger` with `charge`, `recharge`, `demand(contact)` and `verify_conservation`,
keyed on canonical contact ids. Run it beside `FinalizedLoadPartition` on every fixture and
disclose both. Assert I5 and I9. Publish the divergence table: per contact, old carried area
versus new demand. **This is where the design either survives contact with the corpus or does
not**, and it is the natural review gate.

**Phase 3 — sizing reads the ledger.**
Tip radius from `X_ALLOWED` directly (§5.1). `min_section_on_path` locator and invariant I3.
`FrozenTipLoadPolicy` demoted to diagnostic; `implied_area_ratio() <= X_ALLOWED` moved out of
`#[cfg(test)]`. **Every emitted tip radius moves. Every frozen digest, every corpus baseline
and every resin-volume number must be re-frozen**, and the diff must be walked model by model
rather than accepted in bulk.

**Phase 4 — selection reads the ledger.**
The §4.3 algorithm replaces the Z-sort plus Poisson disk. `OVERHANG_ANGLE_DEG` and
`TARGET_SPACING_MM` demoted. Invariant I6 turns on: zero-demand contacts stop being emitted.
Notions 1, 2, 3 deleted; 4, 5, 6 replaced; `SpecializedLoadBaseline::assemble` and the
`CarriableMaximum` machinery deleted. Retire notion 8 only once coverage sufficiency and
`UncarriedDemand == 0` are shown to agree on the whole corpus.

**Phase 5 — the pre-write gate.** Port the load gate Python had and Rust never did (task #59).
It asserts I1, I3 and I6 **on the written artifact**, re-deriving the tip radius from the
disclosed demand rather than trusting the planner. A missing partition entry is a named error,
never a zero — the deleted auditor's policy, finally with a producer behind it.

### 9.1 Cost, with the arithmetic

For a 60 mm model at 0.05 mm layers (1200 layers) with a ~300 mm layer perimeter, at
`q = 0.25 mm`:

* Interior containment tests: bounded by the layer bitset, `O(cells)`, ~160k cells/layer for a
  100 mm plate — a bitset operation, not a geometry query.
* Exact `reach` queries: only on the newly exposed band, `perimeter/q` ≈ **1200 cells/layer**,
  ≈ 1.4 M for the print. The engine already runs the identical query per sampled region point
  in `region_reach_mm`.
* Geodesic sweep: `O(cells)` per layer with a bucketed queue.
* Memory: **fold per layer into per-contact accumulators; never materialize the charge list.**
  A treant already produces 70,737 partition rows under the *face* model; the cell model would
  produce millions. Retain a bounded evidence sample, not the rows.

This is a real charge on a stage that has one today, and it must be registered and metered
beside the charge in the engine's existing idiom, not left as a naked hang.

---

## 10. What I expect an implementer to argue with

Listed so the argument is easy to start.

1. **"Footprint is wrong; a support holds the whole overhang, not one layer of it."** The
   counter is §8.2's coupon, and I concede this is the design's single load-bearing unmeasured
   claim. If the coupon says surface area, the fix is local — swap the quantity, keep the
   ledger, the allocation, the placement algorithm and every invariant.
2. **"The grid is a betrayal of exactness."** It samples area only; `reach` is evaluated in
   continuous geometry and the §3.2 rescale makes conservation bit-exact. But the *partition*
   boundary is quantized at `q/2`, and a contact within 0.125 mm of a Voronoi boundary can
   flip. Whether that matters is measurable in phase 1.
3. **"Deleting `SpecializedLoadBaseline::assemble` is a bigger job than the whole rest of
   this."** Probably true, and it has no estimate anywhere. It is the honest headline cost.
4. **"149 supports on a 30 mm plate is too many."** That is the owner's 0.800 mm diameter
   ceiling talking, not the physics. The physics permits 24 at 2.000 mm diameter and the total
   scar is identical either way. §4.2 makes it an explicit policy dial; the arithmetic in §4.4
   is what the dial costs.
5. **"Sum-of-peaks over-sizes trunks."** Yes, and §5.3 names the exact refinement and why it
   is deferred: plate 6 gives load sharing no measured upper bracket.
6. **"You retired the 45° overhang constant on a derivation, not a measurement."** §4.1 keeps
   the conservative behaviour until COUPON-PLATE-8 is read. The derivation's value is that it
   makes a falsifiable prediction — layer-height independence — that the constant does not.
