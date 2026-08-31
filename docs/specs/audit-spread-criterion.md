# Audit spread criterion — the arrangement gate `physics.md` §2(c) demands

Status: **NORMATIVE for `tools/plinth-audit/`.** This document extends
[`audit-invariants.md`](audit-invariants.md) with one further gating invariant,
**I9 — arrangement spread**, and it is the specification the implementing agent codes
against. It changes no emitted byte and no engine behaviour: like the rest of the audit
it is read-only with respect to `crates/` and `rust/`.

Written 2026-08-30, closing finding 2 of the cross-family review of the audit crate:
*there is no arrangement check at all — every gate is per-contact or per-arm.*

The clause this exists to implement, verbatim from
[`docs/physics.md`](../physics.md) §2, finding (c):

> **`X ≤ 12` is necessary, not sufficient.** The ratio is blind to arrangement. Three
> collinear arms have no lateral stability perpendicular to their line, at any X. An
> arrangement check has to include a spread criterion, not just a per-arm ratio.

and its longer form in
[`03-physics-and-constants.md`](../engineering-journal/03-physics-and-constants.md)
§2.4, "X is blind to arrangement".

The four rules of `audit-invariants.md` §0 bind everything below without exception, and
one of them decides most of the design: **"not measured", "not implemented" and
"indeterminate" never map to pass.** The second decisive rule is this project's own
standing doctrine — a same-everywhere millimetre constant with no derivation is debt
(`docs/physics.md` §12, and the constants registry's 273 placeholders against 22
measured values). Between those two, the gate below **refuses only what it can prove has
no equilibrium at all, and measures everything else as a number.**

---

## 0. The short version

| | |
| --- | --- |
| What is gated | one predicate: the peel resultant on a cured model body must fall in the **interior** of that body's resisting support polygon, by more than a derived uncertainty band |
| Threshold | **none.** The predicate is a statics feasibility test. No constant is chosen; the only tolerances are propagated `f32` error, a fill-rule sensitivity, and the printer's own pixel quantum |
| What "one region" is | a **prefix-connected cured model body** — a connected component of the model material cured at layers `0..=k` — evaluated at **every** layer of its independent life |
| What is deliberately not gated | polygon eccentricity above containment, overturning of a tall narrow group, lateral drift, sag/span of an attached cantilever (that is finding 1's criterion, not this one) |
| Failure codes | `SPREAD_NO_SUPPORT`, `SPREAD_NO_EQUILIBRIUM`, `SPREAD_COLLINEAR_NO_EQUILIBRIUM`, `SPREAD_INDETERMINATE` |
| Schema key | `i9_arrangement_spread` |

---

## 1. The instability modes, and which of them this gates

### 1.1 The load model, and why it differs from the demand set `D`

Peel is the only load (`physics.md` §1). Two facts fix the load for an arrangement
question, and both are derived rather than chosen:

1. **At layer `k`, the film is in contact with the whole cured cross-section of layer
   `k`.** Every cured pixel of that layer was cured against the film. So the peel
   traction on a body acts over its **footprint at layer `k`**, not over its lifetime
   down-facing area.
2. **The traction is uniform per unit area.** That is plate 5's finding (a) — load
   scales with supported *area*, not with perimeter and not by a squeeze-film law, a
   conclusion that survives three cap areas and a 25× span of tip diameters
   (`03-physics-and-constants.md` §1.3(a)).

The resultant of a uniform traction over a region is at that region's **area centroid**,
and its magnitude `P = p · A_k` never has to be evaluated: the predicate below depends
only on where the resultant acts, so the unknown traction `p` cancels. That is the single
most important property of this criterion — **it is scale-free, and therefore it needs no
peel force.** The engine never computes a force either (`physics.md` §2), and no measured
peel force exists in this project: plate 1 bounds peel pressure from one side only.

**This is a different load model from the demand set `D` of `audit-invariants.md` §2.1,
and the difference is deliberate.** `D` is plate-projected *down-facing* area — the
lifetime load charged to a contact for sizing, chosen to match the geometry the coupons
were measured on. The arrangement question is not about lifetime load; it is about
whether the body is in equilibrium during each individual peel event, where the film
touches the entire cross-section. Neither model has been reconciled against the other by
measurement, and §6 lists that as an exposure rather than hiding it.

**Direction, stated once.** In the file's model-centred frame the plate is below
(`z_plate`, `audit-invariants.md` §1.2) and layers stack upward. The film is against the
newest layer, so the peel force on a body acts along **`+z`, away from the plate**, and
the supports hold the body **toward** the plate, in **tension**.

### 1.2 Mode A — rank deficiency (collinearity, and its limit cases) — **GATED**

If every contact holding a body has its force line, projected to the loaded layer plane,
on one straight line, then no combination of member tensions — positive or negative —
produces a moment about that line. The body's only resistance to rotating about it is the
members' own bending, and no measurement in this project bounds that: there is no green-
resin modulus anywhere in the registry, `max_deflection_mm = 0.15` is a Placeholder
serviceability number, and the retired `shaft_drag_force_n` was deleted precisely because
it was not a physical law (`03-physics-and-constants.md` §3.3).

Two limit cases belong to the same mode and are gated by the same predicate: **one
contact** (rank 0) and **two contacts** (rank 1).

### 1.3 Mode B — static inadmissibility of the support polygon — **GATED**

Supports pull; they cannot push a body toward the film. For a rigid body held by
tension-only members and loaded by a single resultant, an equilibrium exists only if that
resultant falls inside the polygon spanned by the members' lines of action, evaluated in
the plane the load acts in. Outside it, **no** assignment of non-negative member tensions
balances the moment — the body must rotate. That is a proof of instability, not an
estimate, and it costs no constant. §3 derives it.

This mode covers "a support polygon too small or too eccentric relative to the region it
carries" **exactly at the point where eccentricity becomes provable**, and no earlier.

### 1.4 Mode C — eccentric but still containing — **NOT gated, reported**

A group whose contacts cluster to one side, with the load resultant inside the hull but
close to an edge, is worse than one whose resultant sits centrally. Turning that into a
verdict needs a minimum normalised margin, and **no measurement in this project fixes
one.** Inventing a number here is exactly the debt the standing doctrine forbids. The
normalised margin is therefore reported as a number, per body and per layer, so a reviewer
can argue about it with data (§5).

### 1.5 Mode D — overturning of a tall narrow group under lateral peel — **NOT gated, reported**

Gating an overturning check needs two quantities this project does not have:

- **a lateral force magnitude.** The engine never computes a force; the release-front
  geometry that would set the lateral component is not vendor-established at all — the
  Saturn 4 datasheet states "Tilt direction / physical-axis mapping: **Not specified**",
  and `leading_release_edge = NegativeY` / `release_front_sweep_direction = PositiveY` are
  recorded there explicitly as **local project records, not vendor claims**;
- **a restoring stiffness**, which needs a green-resin modulus. There is none.

So overturning is measured and disclosed: the group slenderness `h/s` (§5) is the number
that would drive it, and the measurement that would close it is named in §6.

### 1.6 Mode E — lateral drift, and the horizontal force residual — **NOT gated**

The predicate below uses three of the six equilibrium equations: vertical force balance
and the two horizontal moment balances. It does **not** use horizontal force balance or
the moment about the vertical axis. With tilted members a tension set that satisfies the
three gated equations can leave a residual horizontal force, which the members carry in
bending — a *deflection* question, not a *rotation* question, and deflection has no
measured limit here. The residual is reported, never gated.

### 1.7 The counter-evidence, handled: plate 6 printed collinear groups and they survived

This must travel with the criterion, because it is the only measurement the project has on
collinear arrangements and it does not say what clause (c) says.

Plate 6, 2026-07-31 (`03-physics-and-constants.md` §4.2): **12 of 12 strip cells
survived** at a per-arm ratio of `X_arm = 60.15` — `N ∈ {2, 3, 4, 6}` arms **in a line**,
pitches 2.0 and 2.8 mm, lines run along **both X and Y** — while the matched single-arm
cell died at the same ratio. Those strips are collinear groups, and none of them failed.

Three things reconcile that with gating mode A, and all three are limitations rather than
dismissals:

1. **Plate 6 scored breakage, not rotation.** Its outcome variable is whether the
   specimen was still attached. A collinear group that rotated a few degrees under load
   and printed anyway scores as a survivor. The mode gated here has never been *measured*
   in this project — only reasoned.
2. **The strip arms are stubby.** 0.798 mm diameter over 4.00 mm of length, `L/d ≈ 5`.
   Bending resistance, the very thing this criterion neglects, is at its largest there.
   The corpus is not in that regime: 36–54 % of emitted arms sit at exactly the
   `MIN_SHOOT_R_MM = 0.35 mm` placeholder radius (`audit-invariants.md` §4), and the
   audit's own baseline measures a median arm radius of 0.35–0.47 mm across the 24 real
   miniatures (`docs/evidence/audit-baseline-2026-08-30.md`).
3. **The strip caps were symmetric about the line.** A load resultant that lands on the
   line produces no moment about it, so the specimen was in equilibrium — a *neutral*
   one. The predicate below returns exactly that: not a refusal, an **indeterminate**,
   which fails under rule 3 because neutral stability cannot be established without the
   bending term nobody has measured. A plate-6 strip would be reported as
   `rank_deficient_load_on_axis`, with its own diagnostic line, not as a proven collapse.

---

## 2. What "one region" is

This is the hard modelling decision, and getting it wrong produces a gate that cannot
fire — the failure mode this project keeps reproducing.

### 2.1 Three candidate groupings, rejected, with reasons

**Per island (a detached run-component at one layer).** Rejected: an island is a
single-layer object and it is already I7's. Worse, it is ephemeral — the same lobe is a
new island at every layer — so a per-island criterion would have no memory of the contacts
made below and would re-refuse an adequately held lobe forever.

**Per connected down-facing surface component.** This is the grouping `audit-invariants.md`
§2.2 uses for ownership, and it is the trap. A real mini's mesh is watertight and its
surface is **one** component, so this grouping collapses to "the whole model", where the
support polygon is the hull of every contact on the model and the load centroid is deep
inside it. The gate would pass every corpus file while watching nothing. **Rejected
explicitly, and named here so nobody re-proposes it.**

**Per load-path cluster (grouping on the support side — contacts sharing a trunk).**
Rejected because it is the wrong side of the contact. The thing that rotates is *model*
material; the support-side cluster is already the object I3 walks and I5 grounds. A
support cluster's own stability is a slenderness question, which §1.5 does not gate.

### 2.2 The definition

> **A region is a prefix-connected cured model body**: a connected component of the model
> material cured at layers `0..=k`, evaluated at every layer `k` of its life.

Formally, over the model raster of `audit-invariants.md` §1.6:

```
cell(k, r, c)  is cured-model iff column (r,c) is cured in the MODEL raster at layer k
adjacency:  in-plane   4-connectivity within a layer   (edge-sharing pixels)
            vertical   same column, layers k and k-1
body(k)     = the connected components of { cured-model cells at layers 0..=k }
```

Support material is **excluded** from a body. A body is a free body of model resin; the
supports enter only as forces at their contacts.

**In-plane connectivity is 4, not 8, and that is a choice with a direction.** Whether two
pixels touching only at a corner fuse into one piece is unmeasured — it is the in-plane
form of the fusion-uncertain band of §1.5. Four-connectivity splits them, producing *more*
bodies and therefore *more* gating, which is the safe direction for a gate. The cost is
made visible: the count of bodies that would merge under 8-connectivity is reported (§5).
Vertical adjacency is same-column between `k` and `k-1`, identical to I7's own attachment
definition, so the two invariants read the same connectivity.

### 2.3 Why this grouping is the right one

- **It is the body that can actually rotate.** Cured resin is rigid; a maximal connected
  piece of it is a rigid body whose only ties to anything else are its contacts.
- **It generalises I7 in exactly the missing direction.** I7 asks whether a component is
  detached at its birth layer. This asks whether a component, once born, is *held in a
  way that has an equilibrium* — at birth and at every layer after, until it merges.
- **It fires early, where the owner's complaint lives.** A wingtip, a raised foot, a
  lifted tail is born in mid-air, held by two or three tips, and stays an independent body
  for hundreds of layers before it joins the rest of the model. That interval is exactly
  where a collinear or one-sided arrangement destroys a print, and exactly where a
  per-contact ratio sees nothing.
- **It goes quiet when the physics goes quiet.** Once a lobe merges into the parent body
  it is held by the parent, and asking about its own contacts' arrangement would be a
  false refusal. The grouping stops asking, for a stated reason.
- **It separates cleanly from finding 1.** An attached cantilever — a 20 mm ledge, a
  cavity ceiling closing over a hollow — is part of its parent body, so this criterion
  says nothing about it. Its problem is unsupported *span*, which is the unsupported-span
  criterion's job. Arrangement is about a body that can rotate; span is about a body that
  can sag. Two criteria, two objects, no overlap.

### 2.4 The contacts of a body

A contact `c` is identified as in `audit-invariants.md` §3 — by the record index of its
type-7 TIP record. For this criterion each contact contributes a **line of action**, not a
point:

```
x_c   = midpoint(start, end) of the TIP record          the ball centre     RECONSTRUCTED
u_c   = normalize(node_c - x_c)                          rootward unit axis  RECONSTRUCTED
```

where `node_c` is the far endpoint of the type-1 TAPER record joined to that tip — the
member the contact terminates. `audit-contract.md` §1.3 pins both readings: for a ball the
connective point is `midpoint(start,end)` and the poles connect to nothing, and the TAPER
runs from the ball centre to the first structural node. The join is the graph of
`audit-contract.md` §3.2 at its measured tolerance; a tip with no joined taper, or with
more than one, is an indeterminacy (§3.6), never a skip.

**Force along the member axis is a two-force-member idealisation** — the joint transmits
no moment. That is the same neglect as tension-only and errs the same way, toward refusal.

**A contact holds a body only from the layer at which both exist.** Contact `c` enters
`contacts(B, k)` at the first layer `k_c` at which the rasterised tip ball of `c` and the
body `B` share a cell, and stays for every `k ≥ k_c` while `B` persists. A contact cured
above the layer under evaluation does not hold anything yet.

**Contacts are resisting or non-resisting**, by the sign of `u_{c,z}`:

```
resisting      u_{c,z} < 0     the member pulls the body toward the plate
non-resisting  u_{c,z} >= 0    tension in it pulls the body toward the film
```

Only resisting contacts can be *relied on*; non-resisting ones can still participate in an
equilibrium, so they widen what is provable and are handled in §3.4.

### 2.5 Plate-grounded bodies

If a body's cured cells reach the plate — any cell at or below `z_plate + layer_height` —
its plate footprint is a support too, and a bilateral one. Each such cell contributes a
vertical line of action at its own pixel centre. A body resting on the plate over an area
therefore carries a large polygon by construction, which is correct and not an exemption.

### 2.6 Cost, and the batch-first requirement

One upward sweep over the layer stack, in the run-length representation of §1.6:

- union-find over runs: in-plane run adjacency within a layer, column-interval
  intersection between `k` and `k-1` — the same interval intersection I7 already performs;
- per body per layer: footprint area and centroid by one pass over its runs (or over its
  slice loops, §3.5), and a convex hull over its contacts' projected lines of action.

The hull must be **rebuilt per layer**, not maintained incrementally, because the
projection of a line of action depends on the layer plane (§3.2). The cost is
`O(K · C log C)` for `K` layers and `C` contacts on a body — with the corpus's 1,330
contacts over 24 miniatures, negligible beside the raster that already exists.

**A per-pixel loop, or a per-(contact, layer) query against the model mesh, is a defect
and not an implementation choice** — the same rule §1.6 states for rasterisation.

**A bounded budget, and exceeding it fails.** `max_body_layer_evaluations` is finite;
exceeding it emits `SPREAD_INDETERMINATE reason=budget_exceeded` and fails, never a
silent coarsening. This follows the precedent of `SampleBudget` in
`tools/plinth-audit/src/ownership.rs`, whose default had to be raised by measurement
rather than by allowing exhaustion to pass.

---

## 3. The predicate

### 3.1 The statement, in one line

> **The peel resultant on body `B` at layer `k` must lie in the interior of the convex
> hull of `B`'s resisting lines of action, taken in the plane of layer `k`, by more than
> the derived uncertainty `sigma`.**

Everything else in this section is the derivation of that sentence, its tolerance, and its
outcome table.

### 3.2 Derivation — why lines of action, and not contact points

Let body `B` at layer `k` carry contacts `c = 1..n` with ball centres
`x_c = (a_c, b_c, z_c)` and rootward unit axes written

```
u_c = (h_c, -v_c),      h_c in R^2 the horizontal part,  v_c = -u_{c,z}
```

so `v_c > 0` for a resisting contact. Member `c` carries tension `t_c >= 0` and applies
`t_c u_c` to the body. The peel resultant is `P z_hat` at `q = (q_x, q_y)` in the plane
`z = z_k`, with `P = p A_k > 0`.

Three of the six equilibrium equations are used. Vertical force:

```
sum_c t_c v_c  =  P                                                        (E1)
```

and the two horizontal moment balances, **taken about the point `o = (0, 0, z_k)`** — the
reference plane matters and §3.2.1 says why. With the moment of `t_c u_c` applied at
`x_c` and of `P z_hat` applied at `(q, z_k)`, and writing `w_c = t_c v_c`:

```
P q  =  sum_c w_c * ( x_c^xy + ((z_c - z_k) / v_c) * h_c )                  (E2, E3)
```

The bracket is exactly the point where member `c`'s line of action, extended, crosses the
plane `z = z_k`. Name it

```
g_c(k)  =  x_c^xy  +  ((z_c - z_k) / v_c) * h_c                            mm, RECONSTRUCTED
```

Then (E1) and (E2,E3) together say

```
q  =  sum_c (w_c / P) * g_c(k),      w_c >= 0,   sum_c w_c = P
```

which is precisely the statement `q in conv{ g_c(k) }`.

**Two consequences, both load-bearing.**

- The support polygon is the hull of the **lines of action taken at the loaded layer
  plane**, not the hull of the contact points. Those differ whenever a member is tilted
  and the load is at a different height from the contact, which is the ordinary case for a
  body that has grown above its seats. A contact-point hull is an *approximation in an
  unknown direction* and this specification forbids it.
- The traction magnitude `P` cancels. The criterion is scale-free, which is why it can
  exist at all in a project that never computes a force.

`g_c(k)` is undefined when `v_c = 0` — a horizontal member. That is an indeterminacy
(§3.6), never a skip.

### 3.2.1 The reference plane is a choice, and what that costs

The moment balance is origin-independent only when the *full* force balance holds. This
derivation uses three of the six equations and leaves horizontal force balance out (mode
E, §1.6), so the condition it produces depends on the reference plane: the same algebra
about `o = (0, 0, z_0)` yields `q in conv{ g_c(z_0) }` for **every** `z_0`, each one an
equally valid necessary condition, differing from the next by the horizontal residual.

This specification evaluates the condition **at the load plane `z_0 = z_k`**, because that
is the reading with a geometric meaning — the lines of action taken where the load acts —
and because the extrapolation error `sigma_H` of §3.5 is then smallest for the contacts
nearest the load. Two consequences follow and both are honest limitations, not details:

- **A refusal is sound but not the strongest available.** A body that clears the test at
  `z_k` may fail it at some other `z_0`, so this gate is one member of a family and it
  takes the weakest refusal in that family. An implementation may intersect over several
  planes and refuse on any of them — that is strictly more refusing and equally sound —
  but it must then report which plane refused, and the specified gate is the single-plane
  form.
- **A pass is not a proof of full equilibrium.** It establishes that the three gated
  equations are jointly satisfiable. The unbalanced remainder is a net horizontal force,
  which the members carry in bending — a deflection question with no measured limit, and
  §5's `horizontal_residual` is where its size is reported.

### 3.3 The quantities, all reconstructed

| quantity | how | tag |
|---|---|---|
| `x_c`, ball centre | `midpoint(start, end)` of the TIP record | RECONSTRUCTED |
| `u_c`, rootward axis | normalize(far end of the joined TAPER − `x_c`) | RECONSTRUCTED |
| `g_c(k)` | §3.2 | RECONSTRUCTED |
| `A_k`, footprint area of `B` at layer `k` | sum over `B`'s runs at layer `k`, times `pixel_area` | RECONSTRUCTED |
| `q_B(k)`, load point | area centroid of that footprint | RECONSTRUCTED |
| `z_k` | `z_plate + (k + 0.5) * layer_height` | RECONSTRUCTED from the frame check of §1.2 |

Nothing here is DECLARED. There is no field in a `.chitubox` file that states an
arrangement, so rule 2 of `audit-invariants.md` §0 has nothing to compare against, and
this invariant is wholly reconstructed. That is worth stating rather than leaving as an
absence: **I9 cannot catch a lying field, because there is no field to lie.**

### 3.4 The outcome table

Let

```
Gplus  = { g_c(k) : c in contacts(B,k), c resisting }
Gall   = { g_c(k) : c in contacts(B,k) }
Hplus  = conv(Gplus)
d      = signed distance from q to the boundary of Hplus, positive inside
aff    = the affine hull of Gall, resolved at sigma (a point, a line, or the plane)
sigma  = the uncertainty band of §3.5
```

Evaluated in this order:

| # | condition | outcome | code |
|---:|---|---|---|
| 1 | `contacts(B,k)` is empty and `B` is not plate-grounded | **REFUSE** | `SPREAD_NO_SUPPORT` |
| 2 | `dim(aff) <= 1` and `dist(q, aff) > sigma` | **REFUSE** | `SPREAD_COLLINEAR_NO_EQUILIBRIUM` |
| 3 | `dim(aff) <= 1` and `dist(q, aff) <= sigma` | **INDETERMINATE** | `SPREAD_INDETERMINATE reason=rank_deficient_load_on_axis` |
| 4 | `d > sigma` | **PASS** | — |
| 5 | `d < -sigma` and no non-resisting contact on `B` | **REFUSE** | `SPREAD_NO_EQUILIBRIUM` |
| 6 | `d < -sigma` and at least one non-resisting contact | **INDETERMINATE** | `SPREAD_INDETERMINATE reason=non_resisting_contact` |
| 7 | `|d| <= sigma` | **INDETERMINATE** | `SPREAD_INDETERMINATE reason=margin_within_tolerance` |

Every non-PASS outcome fails the invariant; `indeterminate` is counted separately and
forces `status = "fail"` exactly as rule 3 requires.

**Why each branch is sound.**

- **Row 1.** An unheld body has no equilibrium at all. This deliberately overlaps I7,
  which catches the same body at its birth layer by a different route. Two independent
  instruments agreeing is evidence, not duplication.
- **Rows 2 and 3.** With all lines of action on one line, the reachable set of load points
  is that line's affine hull, *whatever the signs of the tensions*. So a load off the line
  is refusable even in the presence of non-resisting members (row 2). A load on the line
  is in equilibrium, but a neutral one: the restoring moment is the members' own bending,
  which is unmeasured, so stability is not established (row 3). Row 3 is the plate-6 strip
  case of §1.7, and it fails as an indeterminate with its own reason string rather than
  being reported as a proven collapse.
- **Row 4 is sound in the presence of non-resisting members**, which is why the pass test
  reads `Hplus` and not `conv(Gall)`: a solution of the gated subsystem that uses only the
  resisting members, setting every non-resisting tension to zero, is still a solution.
  Satisfiability needs only a feasible subset. It is satisfiability of three equations,
  not a full equilibrium — §3.2.1.
- **Rows 5 and 6.** Outside the resisting hull there is no equilibrium *using resisting
  members only*. A non-resisting member acts as a guy wire and widens the balanceable set
  without bound, because bounding it would need the tension magnitudes, which need the
  peel force, which is never computed. So the refusal is available only when no such
  member exists (row 5); otherwise the honest answer is that statics cannot decide
  (row 6). Both fail, and the codes distinguish them.
- **Row 7** is the tolerance band, and its polarity is the subject of §3.5.

### 3.5 The tolerance, and the polarity finding 3 requires

**The polarity, stated first, because it is the review finding this document must not
repeat.** The pass condition is

```
d > +sigma            NOT   d >= -sigma
```

A margin inside the uncertainty band does **not** pass. It is `indeterminate`, and
`indeterminate` fails. Anything not established is a hard failure — `audit-contract.md`,
standing rule 3.

**This is the minimum-type form of [`audit-interval-policy.md`](audit-interval-policy.md)
§3.2, and the three outcomes of §3.4's table are exactly that document's §3.1 triple.**
The delivered quantity is the containment margin `d`, the requirement is zero, and the
conservative bound of `d` is its lower bound `d - sigma`, so `d - sigma > 0` is the pass.
The two documents were written in the same run against the same review finding; where they
ever disagree, the interval policy governs the direction and this document governs what
`d` and `sigma` are.

`sigma` has three terms, all derived, none chosen:

```
sigma  =  sigma_q  +  sigma_H  +  d2 / 2
```

**`sigma_q` — the load point.** The audit's fill rule cures a pixel only when the solid
contains its whole footprint (§1.6), so the rasterised footprint is eroded relative to the
true section by a band of width at most one in-plane pixel diagonal `d2`. Removing a
boundary band of area `R <= perimeter * d2` moves the centroid by at most

```
sigma_q  =  ( R / (A_rast + R) ) * rho,      rho = max distance from q to the footprint bbox corner
```

Every term is available from the runs themselves. Where the implementation can attribute
slice loops to bodies, it should instead take `q` from the exact polygon centroid
(shoelace, f64) and set `sigma_q` to the *measured* difference between the polygon and
raster centroids plus `tol_geom(q)`. That is tighter and equally derived; both forms are
permitted, and the output must say which was used.

**`sigma_H` — the polygon.** `g_c` is built from `f32` endpoints and then extrapolated
along the member axis to the layer plane, which magnifies the axis's angular error by the
extrapolation length:

```
s_c        = (z_c - z_k) / v_c                          extrapolation length along the axis, mm
delta_c    = 2 * tol_geom(node_c, x_c) / L_c            angular uncertainty of the axis, rad
sigma_g(c) = tol_geom(x_c) + |s_c| * delta_c
sigma_H    = max over c of sigma_g(c)
```

with `tol_geom` and `L_c` (the member's length) as in `audit-invariants.md` §1.5. This
term grows for tilted members far below the layer under evaluation — correctly, since that
is exactly where the projection is least certain.

**`d2 / 2 = 1.1800425e-2 mm` — the printer's placement quantum.** The in-plane pixel
diagonal is `d2 = 2.360085e-2 mm` (§1.5). A point's position within its own pixel is
uncertain by at most half of that, so an arrangement narrower than this is not realisable
as an arrangement: the machine renders it as one column of cured material. Including it in
`sigma` is what makes row 2's "collinear" mean *collinear as printed*, not merely
collinear in `f64`.

No fourth term is added for tilt. §3.2's line-of-action projection is exact for arbitrary
tilt; the older, cruder route — a contact-point hull dilated by a bound
`max_c |z_c - z_k| tan(theta_c)` — is what the projection replaces, and it is mentioned
here only so nobody re-derives it and treats the bound as a constant.

### 3.6 Indeterminacy — the exhaustive list

Every one of these fails, with `reason` on the line:

| reason | meaning |
|---|---|
| `rank_deficient_load_on_axis` | row 3: collinear arrangement, load on the axis, neutral equilibrium |
| `non_resisting_contact` | row 6: a member pointing away from the plate makes the refusal unprovable |
| `margin_within_tolerance` | row 7 |
| `axis_horizontal` | `v_c = 0`; the line of action never meets the layer plane |
| `taper_join_ambiguous` | the tip joins no taper, or more than one, at the graph tolerance of `audit-contract.md` §3.2 |
| `raster_failed` | the model raster could not be built at this layer |
| `budget_exceeded` | §2.6 |
| `no_bodies_found` | see §3.7 — the anti-vacuity rule |

### 3.7 The anti-vacuity rule

**A model with at least one cured model layer must produce at least one body-layer
evaluation.** If the sweep discovers no bodies, the invariant emits

```
SPREAD_INDETERMINATE file=<path> model=<m> reason=no_bodies_found bodies=0 layers=<n> contacts=<n>
```

and `status = "fail"`. A check whose scope is zero passes vacuously, and this project has
already shipped that defect once: in the run that built this crate, the fixture harness
reported green because it had globbed a path where no fixtures were and had therefore
discovered nothing. `checked == 0` is never evidence of anything, and this rule is what
stops it being read as such.

### 3.8 Failure lines

Byte-identical to the JSON `message` field, per `audit-invariants.md` §5 rule 4.

```
SPREAD_NO_SUPPORT file=<path> model=<m> body=<b> layer=<k> z_mm=<v> footprint_mm2=<v> centroid=<x,y> contacts=0
  a cured body of model material is held by no support at this layer

SPREAD_NO_EQUILIBRIUM file=<path> model=<m> body=<b> layer=<k> z_mm=<v> centroid=<x,y> margin_mm=<v> sigma_mm=<v> hull_vertices=<n> hull_area_mm2=<v> contacts=<n> resisting=<n> contact_ids=<c,...>
  the peel resultant on this body falls outside its support polygon: no set of member tensions balances it

SPREAD_COLLINEAR_NO_EQUILIBRIUM file=<path> model=<m> body=<b> layer=<k> z_mm=<v> centroid=<x,y> s_min_mm=<v> sigma_mm=<v> axis_deg=<v> offset_mm=<v> contacts=<n> contact_ids=<c,...>
  every line of action holding this body lies on one line, and the load does not

SPREAD_INDETERMINATE file=<path> model=<m> body=<b> layer=<k> reason=<...> margin_mm=<v> sigma_mm=<v> contacts=<n>
  the arrangement of the supports holding this body could not be established
```

`checked` counts **body-layer evaluations**. `failed` counts evaluations landing on rows
1, 2 or 5; `indeterminate` counts rows 3, 6, 7 and every reason of §3.6. A body's own
verdict is its worst layer, and the `measured` object carries the per-body worst margin
for every body, not only the failing ones (§5 rule 3).

---

## 4. Interaction with the load law

### 4.1 What the per-arm ratio establishes

`X = A_supported / A_arm <= X_ALLOWED = 12` is a **strength** condition on one member:
given the load that member carries, its narrowest section survives. It is measured —
`X* in [25.00, 25.34]` on plate 5, with the allowance holding a factor of two back — and
I2, I3 and I4 grade it from the emitted bytes.

### 4.2 What this criterion adds

I9 is an **equilibrium** condition on a group: *does any admissible distribution of member
forces balance the peel wrench at all?* The two are logically independent, and each can
pass while the other fails:

- every arm can be adequately sized and the body still have no equilibrium — three
  collinear fat arms under an off-axis load;
- the arrangement can be impeccable and every arm undersized.

Read together, they are closer to sufficiency than either alone: I9 establishes that a
balancing set of tensions **exists**, and I2/I3 establish that each member survives the
share the audit's own partition charges it.

There is also a direct dependency in the other direction. The measured group rule — plate
6's *at least additive* sharing, `X_tot = A_cap / (N * A_arm)` — presumes a group that
co-carries one region. **A set of contacts with no equilibrium is not a group**, so I9 is
the precondition under which the group form of the load rule may be quoted at all.

### 4.3 What the pair still does not establish

- **Magnitudes.** With four or more contacts the equilibrium is statically indeterminate:
  many tension sets balance the same wrench, and which one the structure actually takes is
  set by stiffness, which is unmeasured. I9 proves existence, never the distribution.
- **That the two load models agree.** I4's ownership is a Voronoi partition of plate-
  projected down-facing area; I9's is the equilibrium distribution over a layer footprint.
  These are different assignments of load to contacts and nothing requires them to
  coincide. Neither has been checked against a measurement. This is the sharpest open
  question in the pair, and §6 of `audit-invariants.md` is where a reviewer should file it.
- **Deflection.** Equilibrium is not small displacement. `max_deflection_mm = 0.15` is a
  Placeholder and no invariant reads it.
- **Dynamics.** Peel is a transient; there is no rate term anywhere in this project.
- **The arms' own peel load.** The demand set `D` counts model triangles only, so no
  invariant charges a support for the peel on its own cross-section. I9 does not fix that
  either — the arm's own load does not act on the model body whose equilibrium it tests.

---

## 5. The non-gating report

Written under `report.spread`, verdict field `"disclosed"`, which is not a passing value
anywhere in the schema (`audit-invariants.md` §4).

```
report.spread.bodies
    count of prefix-connected model bodies discovered, count of body-layer evaluations,
    and the distribution of body lifetimes in layers (min, median, max).
    A model whose bodies count is 1 for its whole height is a model with no
    independent lobes -- report it, do not assume it.

report.spread.margin
    the normalised containment margin  m_hat = d / sqrt(hull_area)  -- dimensionless,
    so it compares across scales -- per body at its worst layer: min, p05, median, max,
    histogram at 0.05.  THIS IS MODE C (section 1.4), measured and not gated.

report.spread.rank
    s_min, the RMS perpendicular spread of the lines of action about their best-fit
    line, mm, per body at its worst layer: min, median, max; and the anisotropy
    sqrt(lambda_min / lambda_max) of their second-moment matrix.  s_min is the number
    a future measurement would put a threshold on.

report.spread.slenderness
    h / s_min, with h = z_k - (mean contact height), per body-layer: max and histogram.
    THIS IS MODE D (section 1.5), the overturning number, measured and not gated.

report.spread.horizontal_residual
    for each PASSING body-layer, the minimum over balancing tension sets of
    |sum_c t_c h_c| / P -- dimensionless.  THIS IS MODE E (section 1.6).
    Where computing the minimum is not tractable, report the residual of the
    least-norm balancing set and say so in the field name.

report.spread.release_sweep                       DISCLOSED, and doubly so
    with the release front a line y = y_f sweeping -Y to +Y, the fraction of front
    stations across the body's footprint at which the line-restricted resultant lies
    inside the support polygon, and the worst excursion in mm.
    Parameter-free: no band width is assumed, because none is measured.
    READ THE PROVENANCE: the sweep axis is a LOCAL PROJECT RECORD. The Saturn 4
    datasheet states the tilt direction is NOT SPECIFIED by the vendor. Nothing
    resting on this field may ever become gating without a measurement of the tilt
    axis and the release band.

report.spread.sensitivity
    (a) bodies that would merge under 8-connectivity instead of 4 (section 2.2);
    (b) body-layer evaluations whose verdict would change under a pixel-centre fill
        rule instead of full containment;
    (c) body-layer evaluations whose verdict would change if the support polygon were
        built from CONTACT POINTS instead of LINES OF ACTION (section 3.2).
    (c) is the witness that the projection is doing work; see section 7.
```

---

## 6. What this criterion CANNOT see

Required, and written before the implementation so it cannot be written to flatter it.

1. **Stiffness, in either direction.** It neglects member bending, the seat's own moment
   capacity — a ball embedded to depth `h` resists rotation over its patch, and here it is
   lumped to a point force on an axis — and joint moment transfer. All three neglects
   remove capacity, so the error direction is toward **false refusal**. That direction is
   the safe one for a gate and it is the same choice §1.6 makes for the fill rule, but it
   is a real cost: a small part genuinely held by one deep seat is refused by row 2. It
   cannot be quantified here, because quantifying it needs a green-resin modulus and there
   is none.
2. **Force magnitudes.** Being scale-free, it cannot distinguish a marginal arrangement
   carrying 0.1 mm² from the identical arrangement carrying 100 mm². Two bodies with the
   same geometry and hugely different loads receive the same verdict.
3. **Which tension set the structure takes.** §4.3.
4. **A lobe after it merges.** Once a body joins its parent the criterion stops watching
   it, and the neck it joined through is invisible — a lobe hanging off a 0.2 mm neck is
   treated as rigidly held. Model-section strength is unmeasured in this project (the
   ~1.5 mm neck wall minimum is explicitly provisional and unregistered), so there is
   nothing to gate it with.
5. **Body flexibility.** The body is assumed **rigid**. For a thin wing or a broad cavity
   ceiling that is simply false: the load does not distribute to distant contacts, and a
   containing support polygon does not mean the material between the contacts is held.
   Where rigidity fails, the governing criterion is span, not arrangement — the
   unsupported-span criterion of finding 1, not this one.
6. **The moving release front.** It gates the simultaneous-release resultant. The swept
   resultant is disclosed only, and the axis it would sweep along is not vendor-
   established (§1.5, §5).
7. **Horizontal force balance and the vertical-axis moment.** Three of six equations are
   used; the residuals of the other three are reported (§5).
8. **Sub-pixel connectivity.** A body split by the conservative fill rule or by
   4-connectivity is evaluated as two bodies, each with fewer contacts than the piece that
   actually cures. This inflates refusals; the sensitivity counts in §5 are the size of it.
9. **Compression and buckling.** Members are tension-only here. During the down-stroke and
   recoat they may be loaded in compression, and no buckling constant exists to grade it.
10. **Everything about arrangement quality short of infeasibility.** A group whose
    contacts huddle in one corner with a single distant outlier passes containment while
    being nearly degenerate. That is mode C, and it is a number in the report, not a
    verdict — because no measurement fixes the threshold that would make it one.
11. **The refusals available at other reference planes.** §3.2.1: the gate evaluates one
    member of a family of equally valid necessary conditions and therefore takes the
    weakest refusal in that family. A body this gate passes may be refusable at another
    plane. That is a deliberate narrowing, and it is the reason a passing I9 must be read
    as "no infeasibility was proven at the load plane" and never as "this arrangement is
    stable".

**The measurement that would close the largest of these** is an arrangement coupon: a
ladder of contact arrangements — collinear, near-collinear at a swept perpendicular
offset, and a spread triple — each carrying an identical cap at a fixed per-arm ratio well
inside `X_ALLOWED`, scored on **deviation of the cap from its designed pose**, not on
whether the arm broke. Plate 6 already printed the collinear rungs of that ladder and
scored the wrong variable (§1.7). With such a plate, mode C gets a threshold, mode A's
row-3 indeterminacy can become a verdict, and the `s_min` distribution in
`report.spread.rank` becomes the axis to read it on.

---

## 7. Proving the check fires

The implementing agent ships **four** fixtures under
`tools/plinth-audit/fixtures/<name>/`, one directory per fixture with the
`<name>.chitubox` / `<name>.expect.json` pair the harness in
`tools/plinth-audit/tests/fixtures.rs` discovers by descending the tree. Build them with
`plinth_audit::synth`, which validates nothing, so a planted defect stays planted.

| fixture | plant | expect |
|---|---|---|
| `spread_collinear` | a slab body carried by three tips on one line, the slab's centroid offset from that line | `check_fails`, `SPREAD_COLLINEAR_NO_EQUILIBRIUM` |
| `spread_eccentric` | the same slab, three tips clustered at one end so the centroid falls outside their hull | `check_fails`, `SPREAD_NO_EQUILIBRIUM` |
| `spread_supported` | **the negative control.** The same slab on four tips near its corners, centroid deep inside | `check_passes` |
| `spread_tilted_axes` | contacts whose ball centres project *inside* the hull but whose **lines of action at the loaded layer** project outside — tilted members, load plane well above the seats | `check_fails`, `SPREAD_NO_EQUILIBRIUM` |

`spread_supported` is not optional. Without a passing control, a check that fails
everything looks perfect — the harness's own documented reason for the `check_passes`
expectation.

`spread_tilted_axes` is the **witness that the specified path actually runs**. It is
constructed so a contact-point implementation returns the opposite verdict from a
line-of-action implementation, which means it cannot be satisfied by an implementation
that quietly skipped §3.2. Its expectation file must additionally be accompanied by a unit
test asserting `report.spread.sensitivity` item (c) is **non-zero** on it: a count, not a
claim. This project has already shipped a byte-identity proof whose model never fired the
changed path; a fixture that does not measurably exercise the branch it is named for is
not evidence.

Two further assertions belong to the implementation's own test module, not to a fixture:

- on every file in `exports/`, `i9_arrangement_spread.checked` is **non-zero** and
  `report.spread.bodies.count` is **non-zero** — the §3.7 anti-vacuity rule, asserted
  against real bytes rather than trusted;
- on at least one real emitted artifact, the count of **passing** body-layer evaluations is
  non-zero. A gate that refuses 100 % of evaluations on every corpus file, the mature main
  body included, is reporting a defect in the instrument rather than a finding about the
  plan, and must be diagnosed before its results are quoted.

---

## 8. Registry and schema changes another agent must make

This document owns no code. Landing I9 requires these edits, listed exactly so they are
made together and by whoever owns each file:

| file | change |
|---|---|
| `tools/plinth-audit/src/checks/arrangement_spread.rs` | **new.** The check, `id() = "i9_arrangement_spread"`, `title() = "I9 arrangement spread"` |
| `tools/plinth-audit/src/bodies.rs` | **new.** The prefix-connected body sweep of §2.2, shared with the report |
| `tools/plinth-audit/src/checks/mod.rs` | `pub mod arrangement_spread;`; add to `gating()`; `GATING_IDS` and `GATING_TITLES` become `[&str; 9]` |
| `tools/plinth-audit/src/checks/report_metrics.rs` | the `report.spread` block of §5 |
| `docs/specs/audit-invariants.md` | a §3.9 pointing here; §5 schema gains the key; §5 rule 2 "all eight" becomes "all nine"; `gating_total` 8 becomes 9; the human summary gains its line |
| `docs/specs/audit-contract.md` | no change. Every quantity I9 needs is already in its §3.1 table as RECONSTRUCTIBLE |

**On the ordinal.** The name is load-bearing, the number is not.
[`audit-span-criterion.md`](audit-span-criterion.md), the companion closing finding 1,
**replaces I7 in place** rather than claiming a new index, so `i9_` is free as this is
written. If that changes, this one becomes `i10_` and the only thing that must move with
it is the `GATING_IDS` / `GATING_TITLES` / `gating()` triple, whose order the harness in
`tests/fixtures.rs` validates. The integrator picks, once.

**One coordination point with the span criterion.** Both walk the model layer raster and
both care about connectivity between layers. They ask different questions of it — span
asks how far cured material reaches from what carries it, arrangement asks whether a
connected body has an equilibrium — and neither may assume the other's data structure.
If the body sweep of §2.2 and the span walk end up sharing a pass, that is an
optimisation for the integrator to make deliberately, not an implicit dependency for
either agent to create.

---

## 9. Provenance of every number this document uses

| number | value | provenance |
|---|---|---|
| `X_ALLOWED` | 12 | Derived from `X* in [25.00, 25.34]`, plate 5, 2026-07-30. Quoted here only for §4; **I9 uses it nowhere** |
| `d2`, in-plane pixel diagonal | 2.360085e-2 mm | Derived from pitch 0.014 × 0.019 mm, `audit-invariants.md` §1.5 |
| `d2 / 2` | 1.1800425e-2 mm | Arithmetic on the line above |
| `pixel_area` | 2.66e-4 mm² | Derived, same source |
| `layer_height` | 0.050 mm | Measured, project record (plate 6), a declared input of §1.3 |
| `tol_geom` | function of magnitude | Derived from `eps32 = 2^-24`, `audit-invariants.md` §1.5 |
| plate 6 strips | 12/12 survived, `X_arm = 60.15`, `N in {2,3,4,6}`, pitch 2.0 / 2.8 mm, cap 30 mm², arm 0.798 mm dia × 4.00 mm | Measured, 2026-07-31, `03-physics-and-constants.md` §4.2, with its five caveats |
| corpus scale | 24 real miniatures, 8,070 support records, 1,330 contacts | Measured 2026-08-30, `docs/evidence/audit-baseline-2026-08-30.md` |
| median arm radius | 0.35 – 0.47 mm | Same |
| release axis | `leading_release_edge = NegativeY`, sweep `PositiveY` | **Local project record, NOT vendor.** The datasheet states the tilt direction is not specified. Report-only, §5 |

**No constant is introduced by this document.** The predicate of §3 contains no chosen
number; its only tolerances are propagated measurement error and the printer's own
quantum. That is the property that makes it shippable in a project where 273 of 306
registered constants are placeholders.

## See also

- [`audit-invariants.md`](audit-invariants.md) — the eight gating invariants this extends,
  their shared reconstruction (§2), tolerances (§1.5), raster (§1.6) and output schema (§5).
- [`audit-contract.md`](audit-contract.md) — what the bytes can answer; §1.3 record roles,
  §3.2 the graph and its measured tolerance.
- [`audit-span-criterion.md`](audit-span-criterion.md) — finding 1's criterion. It gates
  the *attached* cantilever this document deliberately does not: span is a body that can
  sag, arrangement is a body that can rotate.
- [`audit-interval-policy.md`](audit-interval-policy.md) — finding 3's policy. It governs
  the direction of every tolerance comparison, including §3.5 here.
- [`../physics.md`](../physics.md) §2 finding (c) — the clause this implements; §4 —
  sharing and peel direction.
- [`../engineering-journal/03-physics-and-constants.md`](../engineering-journal/03-physics-and-constants.md)
  §2.4 and §4.2 — arrangement blindness, and the release-front experiment that refuted the
  zipper.
- [`../datasheets/printers/saturn-4-ultra-16k.md`](../datasheets/printers/saturn-4-ultra-16k.md)
  — the vendor record, and its "not specified" on tilt direction.
