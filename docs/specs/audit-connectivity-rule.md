# The pixel-connectivity rule — what makes two cured regions one region

Status: written 2026-08-31 for `tools/plinth-audit/`. This document is a **ruling**. Every
place in the audit that decides whether two pieces of cured material are one region follows
it, and the sites that must not follow it are named individually in §4 with the reason.

**Why it exists.** The brief named three sites. There are **five**, and they give **four**
different answers to the same question — and nothing in the tool records that they disagree:

| site | rule in force | shared boundary needed to join |
|---|---|---|
| `src/raster.rs:640` `RunRaster::layer_components` | in-plane 4-connectivity | one pixel edge |
| `src/checks/unsupported_span.rs:470` `components_8` | in-plane 8-connectivity | a corner |
| `src/checks/arrangement_spread.rs:738` `inplane_labels` | parameterised, gate uses 4 | one pixel edge |
| `src/checks/intersection.rs:994` `components_26` | 3-D 26-connectivity | a corner, including across layers |
| `src/checks/contact_area.rs:1066` `NEIGHBOURS` | 3-D 6-connectivity | one voxel face |

Five sites, four different answers. The specifications disagree too, and in writing:
`docs/specs/audit-spread-criterion.md` §2.2 calls the diagonal touch "the in-plane form of
the fusion-uncertain band" and chooses 4 deliberately, with the cost disclosed;
`docs/specs/audit-span-criterion.md` §3.2 and §3.3 specify 8-connectivity twice and **give no
reason anywhere in the document.** Both were written on 2026-08-30, for the same tool, about
the same lattice.

**The owner's ruling, verbatim, which is the starting point and the constraint:**

> "For slice areas to be considered 'one region' I think that they generally need to be
> connected by more than just a 'bishop' diagonal single-pixel connection. The physics should
> probably inform this; I think 'rook' connections will be stronger than 'bishop' connections,
> and multi-pixel contact will be stronger than single-pixel, obviously. The method for making
> the decision should be consistent, everywhere, and it should be thoughtfully reasoned based
> on the physics of the resin and printer; what is enough of a connection to hold the two
> sections together and provide the needed support?"

So this is not a graph-theory convention. It is a load question: **what cured cross-section is
enough to carry the load that passes through it?** §1 measures the cross-section, §2 applies
the measured load law to it, §3 states the rule, §4 rules on every site, §5 reports what the
rule does to the corpus, and §6 says what it cannot see.

Three rules inherited from `docs/specs/audit-contract.md` and applied without exception:
anything the audit cannot establish is a hard failure; measurement uncertainty is subtracted
from an allowance and never added to it; and limits come from measurement — constants prune a
search, they never decide an outcome (`docs/physics.md` §12).

---

## 1. The geometry of a connection

### 1.1 The lattice

From `crates/plinth-core/src/constants/evidence.rs:154-159` and `:261-266` (both `Measured`,
vendor, evidence date 2025-01-22), and `docs/specs/audit-contract.md` §4 /
`audit-invariants.md` §1.5-§1.6, restated in `tools/plinth-audit/src/consts.rs:167-172`:

```
pitch_x  = 0.014 mm      (X, 15 120 px over 211.68 mm)
pitch_y  = 0.019 mm      (Y,  6 230 px over 118.37 mm)
h        = 0.050 mm      layer height, PROJECT RECORD, both calibration plates ran at it
```

A cured cell is therefore a `0.014 x 0.019 x 0.050` mm box. Its three face areas — and there
are only three — are the entire geometric content of the connectivity question:

| contact | shared face | area, mm² | what it means |
|---|---|--:|---|
| **rook along X** (same row, neighbouring columns) | `pitch_y · h` | **9.500e-4** | the plane `x = const` |
| **rook along Y** (neighbouring rows, same column) | `pitch_x · h` | **7.000e-4** | the plane `y = const` |
| **vertical** (same column, neighbouring layers) | `pitch_x · pitch_y` | **2.660e-4** | the plane `z = const`; this is `PIXEL_AREA_MM2` |
| **bishop** (in-plane diagonal) | a line | **0** | zero area, exactly |
| **3-D edge / corner** (diagonal across layers) | a line or a point | **0** | zero area, exactly |

An `n`-pixel shared edge is `n` times the corresponding row: `n · 7.000e-4 mm²` across rows,
`n · 9.500e-4 mm²` across columns. The two axes differ by `19/14 = 1.357x`, so **any rule
stated as a pixel count is 36 % stricter on one axis than the other**. The rule below is
therefore stated as an area and converted to counts per axis, never the reverse.

### 1.2 What a corner contact is once cure bleed is admitted — and this is an assumption

Mathematically a corner has zero area, so a bishop contact carries exactly zero load. Cured
resin is not a mathematical set: anti-aliased exposure cures a little outside the nominal
pixel footprint. Model that as an isotropic bleed `δ` per side. Two diagonal cells then
overlap in a `2δ x 2δ` square, and the union's two reflex vertices sit at opposite corners of
it, so the **narrowest section separating one cell from the other is the segment between them**:

```
w_bishop  = 2·sqrt(2)·δ                       the neck width
A_bishop  = 2·sqrt(2)·δ·h  =  0.141421·δ      mm², with δ in mm (z-bleed ignored, which
                                              overstates nothing and understates A_bishop)
```

| δ per side | `A_bishop`, mm² | in units of one rook-along-Y face |
|--:|--:|--:|
| 0.0005 mm | 7.071e-5 | 0.10 |
| 0.001 mm | 1.414e-4 | 0.20 |
| 0.002 mm | 2.828e-4 | 0.40 |
| 0.005 mm | 7.071e-4 | 1.01 |
| 0.007 mm (half a pixel in X) | 9.899e-4 | 1.41 |
| 0.013 mm | 1.838e-3 | 2.63 |

**`δ` has never been measured on this machine.** It was designed to be: plate 5 carries three
witness pads for exactly this quantity — `W5A`, `W5B` and `Q2`, "printed-vs-nominal at all
eight `TIP_DT_MM` rungs, plus the parallax key"
(`docs/engineering-journal/reference/plate-05-results.md` §"Validity piers and witness pads"),
and `docs/evidence/plate-generator-PLATE-LAYOUT.md` §"Family W" says of them **"measure these,
do not score them"**. No reading of any of the three appears in the plate-5 record or anywhere
else in `docs/`. The one magnitude the corpus quotes — "0.026 mm of over-cure: 0.013 mm per
side" (`PLATE-LAYOUT.md`, Family W rationale) — is an illustrative figure inside an argument
about why the witnesses are needed, not a measurement. It is used in the table above as the
top of the plausible range and **must not be cited as data.**

`audit-invariants.md` §3.2 states the same gap from the other side: the audit "cannot see
over-exposure bleed, which is real on this machine".

**The consequence, and it is the whole of the bishop question.** Over a plausible and entirely
unmeasured range of `δ`, a corner contact's cross-section runs from 10 % of a rook face to
2.6x a rook face. The physics does not say a bishop connection is weak. It says **nobody
knows what a bishop connection is**, and the number that would settle it was designed,
printed, and never read.

That is precisely the definition of the audit's own **fusion-uncertain band**: "two solids
separated by a gap in `(0, d3]` may or may not cure into one piece, and the audit refuses to
decide. Any check that lands in that band returns `indeterminate`"
(`audit-invariants.md` §1.5, `d3 = 5.529014e-2 mm`). A corner contact is that band expressed
in the lattice: the two cells are separated by a gap of exactly 0 in the nominal geometry and
by an unknown amount in cured resin. Under the contract's first rule — anything the audit
cannot establish is a hard failure, and indeterminate never maps to pass — **a corner contact
contributes zero to a joint.**

---

## 2. The load that crosses the connection

### 2.1 The law, and it is the only one there is

`docs/physics.md` §2: peel is the only modelled load, it acts over area, and one member
carries the peel of the area it owns through its own narrowest cross-section:

```
X = A_supported / A_section                 both mm², X dimensionless
X* ∈ [25.00, 25.34]                         Measured, plate 5 (cones); X_STAR_CONE
X_ALLOWED = floor(25.00 / 2) = 12           Derived; the design point, half the boundary
```

A connection between two cured regions is the same object as an arm's narrowest section: a
cured cross-section that the peel load of everything on the far side must pass through. The
law applies to it directly.

### 2.2 Inverted — how much area one face can carry

```
A_carried ≤ X_ALLOWED · A_joint
```

| joint | area, mm² | carried at `X_ALLOWED = 12` | in pixels of down-face |
|---|--:|--:|--:|
| one rook-along-Y face | 7.000e-4 | 8.400e-3 mm² | **31.58 px** |
| one rook-along-X face | 9.500e-4 | 1.140e-2 mm² | **42.86 px** |
| one vertical face | 2.660e-4 | 3.192e-3 mm² | **12.00 px** |
| one bishop contact, δ unmeasured | 0 … 1.8e-3 | 0 … 2.2e-2 mm² | **0 … 83 px** |

At the measured boundary `X* = 25.00` rather than the design point, one rook-along-Y face
carries 65.79 px and one rook-along-X face 89.29 px.

The vertical row is worth pausing on: **one single cured pixel column, layer to layer, carries
exactly `X_ALLOWED` pixels of down-face area** — 12, by construction, because the vertical face
*is* the pixel. That is a pleasing check that the arithmetic is the physics and not a
coincidence of units, and it is the one row whose loading geometry matches the calibration
exactly (see §2.4).

### 2.3 Inverted again — the minimum joint for a region

For a region of `n` pixels (`A_R = n · 2.66e-4 mm²`) hanging off the rest of the layer through
one joint, the area law demands

```
A_joint  ≥  A_R / X_ALLOWED
n_faces  ≥  n / 31.58        (rows)        n_faces ≥ n / 42.86   (columns)
```

So one rook face is enough for any region up to **31 pixels** (8.4e-3 mm², a square 92 µm on a
side), and a region of 3 274 px — the detached island `sphere-first-artifact` actually
contains, 0.870884 mm² — would need **104 rook-along-Y faces, 1.45 mm of shared boundary.**
Hold that number; §3.4 is about why it must not be used as a connectivity threshold.

### 2.4 Where the calibration stops, exactly

Both plates are the evidence and both have a smallest section.

| | smallest section built | smallest section that **survived** anything |
|---|--:|--:|
| plate 5 (cones), `plate-05-results.md` §Row A | `A_tip = 0.007854 mm²` (`TA03`, `TA06`, `TA09`, tip Ø 0.100 mm) | `A_tip = 0.310982 mm²` (`TA04`, X = 10.102, **PRESENT**) |
| plate 6 (cylinders), `plate-06-results.md` §4 | `A_arm = 0.047780 mm²` (d 0.247 mm) | — (not separated in the record) |

One rook-along-Y face, 7.000e-4 mm², is **11.22x below the smallest section ever built** and
**444x below the smallest section ever observed to hold a load.**

Two further gaps between the calibration and a pixel-scale joint, neither measured:

- **Loading mode.** An arm's section is horizontal and the peel pulls across it: tension.
  An in-plane joint's section is *vertical* and the peel pulls along it: **shear**. The area
  law was measured in tension. `audit-invariants.md` §3.2 already books the same debt for I2
  ("the area law was measured in pure tension … a patch at 70 degrees of incidence is graded by
  a number measured at 0"). The vertical joint (§2.2, third row) is the only one free of it.
- **Notch.** Plate 5's cones and plate 6's cylinders are smooth. A one-pixel neck between two
  lobes is a sharp double notch with a root radius of order the pixel, and a brittle green
  polymer is notch-sensitive. The area law carries no stress-concentration term, so applied to
  a neck it **overestimates** strength by an unmeasured factor.

Both point the same way: the naive extrapolation of `X_ALLOWED` down to one pixel edge is
optimistic, and by an unmeasured amount.

---

## 3. The rule

### 3.1 Rule K, stated once, for the whole tool

> **K1 — identity.** Two cured cells are joined only through a shared **face**. The joint
> between two cell sets is the total area of the faces they share:
>
> ```
> A_joint = n_x·(pitch_y·h) + n_y·(pitch_x·h) + n_z·(pitch_x·pitch_y)
>         = n_x·9.500e-4  +  n_y·7.000e-4  +  n_z·2.660e-4     mm²
> ```
>
> **Edge contacts and corner contacts contribute zero and never join.** In lattice terms:
> in-plane 4-connectivity, 3-D 6-connectivity, uniformly, at every site §4 assigns to this
> rule.
>
> **K4 — the censor travels with the component, not the fragment.** Any minimum-area censor
> (I7's `pixel_area` island censor, I9's `regions_censored_one_pixel`) is evaluated on the
> **K1 component**. A rule that subdivides material may never delete it from a denominator.
> §3.5 and §5.4 are why this clause is binding rather than advisory.
>
> **K2 — the joint's load capacity, which is a *cut* criterion and NOT part of identity.**
>
> ```
> A_joint  ≥  max( A_JOIN_MIN , A_light / X_ALLOWED )
>
> A_JOIN_MIN = π · TIP_RADIUS_MIN_ALLOWED_MM²  =  7.853982e-3 mm²
> A_light    = the down-face area of the lighter side of the cut
> X_ALLOWED  = 12
> ```
>
> §3.2 derives `A_JOIN_MIN`; §3.4 and §5.5 are why it is a cut criterion and where it goes.

In whole faces on a single axis, which is what an implementation of K2 counts:

| axis | face area, mm² | faces to reach `A_JOIN_MIN` | joint width |
|---|--:|--:|--:|
| across rows (Y) | 7.000e-4 | `ceil(11.2200)` = **12** | 0.168 mm |
| across columns (X) | 9.500e-4 | `ceil(8.2673)` = **9** | 0.171 mm |
| vertical (Z) | 2.660e-4 | `ceil(29.5262)` = **30** | 30 px of footprint |

An implementation **must compute those counts from `A_JOIN_MIN`, `pitch_x`, `pitch_y` and `h`
at run time and must not hard-code them**, and must echo all four inputs in `declared_inputs`.
A constant whose derivation is not in the code has been converted into a chosen number, which
is what physics-first doctrine forbids. The three counts differ by 36 % between the two
in-plane axes; that is why the rule is an area and the counts are derived from it, never the
reverse.

`TIP_RADIUS_MIN_ALLOWED_MM = 0.050 mm` is already registered, `Derived` from
`TIP_DIAMETER_MIN_ALLOWED_MM = 0.100 mm` (`crates/plinth-core/src/constants/evidence.rs:352-358`),
and it is the **validator** minimum: the smallest cross-section the project admits as a
load-bearing member anywhere. K2 invents no limit. It applies the one the project already
carries to the one place that never had it.

### 3.2 The bracket, because the physics does not fix N

The physics brackets `N` rather than fixing it, and the bracket has three candidate ends. All
three are stated so the choice is visible:

| end | value on the Y axis | what it assumes | verdict |
|---|--:|---|---|
| **N_load** — apply `X_ALLOWED` at pixel scale | **1 face** (one face carries 31.58 px) | that the area law holds 11x below its smallest built section, in shear rather than tension, across a sharp notch | **rejected as the answer**, kept as the lower bracket end |
| **N_built** — the joint must reach the smallest section the calibration ever contained | **12 faces** = 7.853982e-3 mm² | that the law may be used down to, and no further than, `TIP_RADIUS_MIN_ALLOWED_MM` — which is exactly what the engine's own validator already assumes | **taken** |
| **N_held** — the joint must reach the smallest section ever observed to *survive* | 444 faces = 0.310982 mm² (`TA04`) | that a section is unusable until a specimen of that size has survived a load | **rejected**, see below |

`N_held` is rejected for a stated reason, not for being inconvenient. Every plate-5 specimen
below 0.310982 mm² of section was loaded at `X` of 400, 2 500 and 15 625 — one to three orders
of magnitude past the boundary — so all of them failing is exactly what the law predicts and is
no evidence about size at all. The plate never tried a small section at a small ratio. That is
**absent** evidence, not negative evidence, and turning it into a limit would fork a registered
constant by re-deciding the same question in a second place. §6.6 records it as the honest
residual uncertainty instead.

**The conservative end of the admissible bracket is `N_built`, and §3.1's K2 takes it.**

### 3.3 What K1 rules on the owner's question

The owner asked whether a bishop connection joins. K1 says no — and the reason is *not* that a
corner is weak. §1.2 shows nobody knows whether it is weak. The reason is that its strength is
a function of an unmeasured quantity that the project designed an instrument to measure and
never read, so it lands in the fusion-uncertain band, and the contract's first rule sends that
to failure rather than to pass. A `δ` measurement from `W5A`/`W5B`/`Q2` would **supersede** this
clause rather than collide with it — which is why K1 is written as "contributes zero", not as
"corners are weak".

The owner's second observation — "multi-pixel contact will be stronger than single-pixel,
obviously" — is K2, and it is right: §2.2 puts one rook face at 31.58 pixels of carrying
capacity and it scales linearly. What §3.4 and §5.5 add is *where* that demand can be evaluated
without lying.

### 3.4 Why the load tier is a cut criterion and cannot be an adjacency test

K1 is **local**: it looks at one pair of cells, or one pair of run intervals. K2 is a statement
about a **cut** — the total joint across an interface between two candidate parts. The two are
not interchangeable, and substituting one for the other is an arithmetic error, not a
conservatism.

A solid 3 274-pixel blob (the detached island `sphere-first-artifact` actually contains,
0.870884 mm²) is held together by thousands of one-face links. K2 says its cut to the rest of
the layer must be at least `0.870884 / 12 = 0.0726 mm²`, which is 104 rook-along-Y faces —
1.45 mm of shared boundary. No individual link is anywhere near that, and none has to be: the
interface is the sum. Evaluate K2 pairwise and the blob shatters.

**This was measured, not argued.** §5.5 runs the corpus with K2's 12-face floor applied
per-run-pair. A smooth sphere goes from 1 detached island to **307**; Round 25 goes from 1
prefix-connected body to **74**; two files *gain* a gate because their defects fragment under
the area censor. None of that is a fact about those files. It is the rasteriser's staircase
being read as structure.

**Ruling.** The tool ships **K1 + K4** as the identity rule at the sites §4 names. **K2 —
including its `A_JOIN_MIN` floor and its load-proportional term — is assigned to a new
cut-level check**, which finds each region's narrowest internal joint and grades it by the area
law against the lighter side's own area, under its own failure code. The conservative end of
§3.2's bracket is still taken; it is taken *there*, where a cut criterion can be computed on a
cut. Until that check exists, the tool has no gate on a thin neck in model geometry, and §6.2
says so.

### 3.5 The censor corollary — required, and measured, not predicted

Splitting a region is *usually* the gating direction, and both existing specifications say so.
It is not always, and the exception was found by running it.

I7 skips any island with `area(C) ≤ pixel_area` (`audit-invariants.md` §3.7); I9 reports
`regions_censored_one_pixel`. When a stricter rule shatters a failing region, the fragments can
fall **below the censor** and stop being reported at all. Measured on `Bat Cat Owl`, moving I9
from 8-connectivity to K1:

```
regions_censored_one_pixel   1  ->  3
regions_infinite_reach       6  ->  5
regions_failed               6  ->  5           the file got BETTER by getting stricter
max_reach_mm            0.802429 -> 0.802429    the geometry did not move
```

Two more files do the same thing (§5.4), and under the invalid `c12` configuration two files
gain a whole gate that way (§5.5). Hence **K4**: the censor is evaluated on the K1 component,
never on a fragment. Without K4 this rule can lower a failure count by fragmenting a defect out
of existence — this project's signature defect class wearing a new hat.

---

## 4. Every connectivity site in `tools/plinth-audit/src/`, ruled individually

Some connectivity in the tool is a load question. Some is bookkeeping, where a physical rule
would be actively wrong. Each site is ruled on its own.

| # | site | today | question it answers | ruling |
|---|---|---|---|---|
| 1 | `raster.rs:613-653` `layer_components`, join test at `:640` (in-plane 4-conn on runs). Callers: `island_coverage.rs:124` (I7), `:524` (test) | 4 | which cured pixels are one island | **K1 + K4 apply.** K1 is already satisfied here; K4 is new and binding (see §3.5). |
| 2 | `unsupported_span.rs:441-493` `components_8` (join test at `:470`) at `:1065` — down-face regions of `unattached(k)` | 8 | which unsupported pixels are one region | **K1 + K4 apply. This is the site whose numbers move.** 8-connectivity → K1. |
| 3 | `unsupported_span.rs:441-493` `components_8` at `:1298` — `support_borne(k)` | 8 | which support pixels are one cured piece, so "is this blob itself attached below" | **K1 + K4 apply.** Same lattice, same load meaning: a support component held on by a corner is held on by nothing established. |
| 4 | `arrangement_spread.rs:721-747` `inplane_labels`, gate call `:988` | 4 | which model material is one rigid body that can rotate | **K1 + K4 apply.** K1 is already satisfied here; K4 is new. Rigidity is exactly a "does this cross-section transmit moment" question, and a corner transmits none that has been measured. |
| 5 | `arrangement_spread.rs:798` via `count_bodies(raster, true)` at `:1655` — the `sensitivity.a_bodies_merged_under_8_connectivity` disclosure | 8 | *how much does the choice cost* | **K1 does NOT apply — this is an instrument, not a verdict.** It must keep computing the looser answer, because its whole job is to price the stricter one. Rename it to name the new baseline. |
| 6 | `intersection.rs:950-1010` `components_26` (join test at `:994`), used at `:460` | **26** | which exterior voxels are one breach of the model surface (I6) | **K1 + K4 apply, and this is the loosest and most consequential site.** `breach_vol` sums every component except the primary one, so merging across corners **shrinks the reported breach** — the non-gating direction, at a verdict-bearing site. Its own comment defends 26 as avoiding "breaches out of raster staircase artefacts". That concern is real, and K1 does not raise it: an ordinary staircase keeps a shared face between consecutive steps, so only a genuinely diagonal-only contact splits. What K1 costs and buys here was measured separately — §5.6. |
| 7 | `contact_area.rs:1066-1073` `NEIGHBOURS` (3-D 6-conn) at `:1007` — I2's patch component | 6 | which cured voxels form the disc this contact actually carries load through | **Already compliant with K1**, and K4 does not arise. K2 must NOT be added here: the object measured is a planar cut whose *area* is the graded quantity, and a minimum-joint test inside it would grade the same quantity twice. |
| 8 | `narrowest_section.rs:670-710` `cross_section_area` — 4-neighbour flood fill, step `sqrt(pitch_x·pitch_y)` | 4 | the area of the arm's section at one station (I3) | **K1's spirit only.** This lattice is a resampling of the solid at a step of 0.0163 mm, not the printer's pixels; its "faces" are quadrature cells, not cured faces. Keep 4-neighbour. Do **not** apply K2 — the section area *is* the graded quantity and I3 already grades it by the same area law. |
| 9 | `island_coverage.rs:305` `is_attached` — vertical, one shared column | 1 column | which pixels have material directly beneath (the demand set) | **Out of scope, deliberately.** By §2.2 one vertical face carries 12 px, so a 1-column attachment is *not* obviously safe and the vertical K2 count is 30. But this predicate defines `unattached(k)`, which `audit-invariants.md` §2.1 requires to reconstruct `D_total`, the demand every load invariant is built on: "A change to `is_attached` breaks I2, I3 and I4 as well" (`audit-span-criterion.md` §3.1). Changing the demand set and the connectivity rule in the same round would make neither measurable. **Recorded as the next question, not answered here.** |
| 10 | `arrangement_spread.rs:754-781` `for_each_vertical_overlap` — vertical, ≥1 shared column | 1 column | which in-plane body at layer `k` continues a body at `k-1` | **Out of scope, same reason as #9**, and it must stay identical to #9 by `audit-spread-criterion.md` §2.2 ("identical to I7's own attachment definition, so the two invariants read the same connectivity"). If #9 moves, this moves with it, in the same change. |
| 11 | `mesh.rs:433-459` — triangle components under shared undirected edge | mesh topology | which triangles are one surface sheet (§2.2 surface components, I2 seat routing, the honest-exemption `Z`) | **K1 must NOT apply.** This is topology on a mesh, not adjacency on a lattice; there are no pixels, no faces and no cross-section. `docs/physics.md` §6 rests on this being ordinary surface connectivity, and the crevice ruling of 2026-08-25 depends on it staying so. |
| 12 | `graph.rs:368-407` — support-record components over reconstructed endpoint joins; read by I5 (`load_path.rs`) and I8 (`dead_geometry.rs`) | endpoint coincidence within `JOIN_TOL_*` | which support primitives are one structure | **K1 must NOT apply.** These are analytic primitives, not cured cells; the corresponding physical question (do two overlapping solids cure into one piece) is already handled by `audit-invariants.md` §3.5's fusion-uncertain overlap rule, which is the same doctrine expressed for solids. |

Summary: **K1 + K4 apply at sites 1, 2, 3, 4 and 6. K1 is already satisfied at 7. Both are
explicitly excluded at 5, 8, 9, 10, 11 and 12**, each for a stated reason. **K2 applies at no
site in this table**: it is the threshold of the new cut-level check of §3.4.

One stale comment found on the way and worth fixing in the same change:
`raster.rs:610` says `layer_components` "is I7's island primitive and I2's patch-component
primitive". It is not I2's — I2 uses `cut_patch`'s 6-connected voxel walk (site 7). The
comment is the only place in `src/` that claims the two share a connectivity rule.

---

## 5. The measured effect

A rule that changes nothing has not been applied, so the effect was measured rather than
predicted. Method and provenance are in §7; the short version is a scratch copy of the crate
with one environment knob wired into the connectivity sites, each defaulting to the shipped
behaviour, swept over the identical 31-file corpus. **With the knob unset the probe reproduces
every published headline of `audit-baseline-hardened.md` exactly**, so the deltas below belong
to the knob.

Four configurations:

| mode | in-plane rule at raster / span / spread |
|---|---|
| `base` | 4 / **8** / 4 — as shipped |
| `c0` | 8 / 8 / 8 — the loose end, for pricing the current choice |
| `c1` | **K1 everywhere** — face adjacency, the ruling of §3.1 |
| `c12` | K2's 12-face floor applied **locally, per run pair** — see §5.4 |

### 5.1 Totals

Over the 24 real miniatures (the corpus the published baseline reports):

| mode | gate-passes | I7 failed / islands | I9 failed / regions | `SPAN_UNSUPPORTED` | `ISLAND_UNSUPPORTED` | I10 bodies | I10 failed / evals |
|---|--:|--:|--:|--:|--:|--:|--:|
| `base` | 6 | 415 / 1 344 | 260 / 691 885 | 19 | 241 | 73 | 6 935 / 17 478 |
| `c0` | 7 | 242 / 1 106 | 260 / 691 885 | 19 | 241 | 33 | 6 087 / 16 625 |
| **`c1`** | **6** | **415 / 1 344** | **277 / 1 345 164** | **33** | **244** | **73** | **6 935 / 17 478** |
| `c12` | 5 | 73 107 / 85 414 | 1 769 / 10 683 503 | 88 | 952 | 6 598 | 134 070 / 146 515 |

Over all 31 files, the same four modes give **12 / 13 / 12 / 9** gate-passes and
`SPAN_UNSUPPORTED` of **40 / 40 / 54 / 245**.

`c1` leaves I7 and I10 bit-identical to `base` at every one of the 31 files, which is the
probe's own control: `c1` changes only the site that was already at 4 for those two invariants,
so anything that moved has to be I9. It is.

### 5.2 What the current disagreement is already worth

The tool's existing disclosure, summed across the corpus, answers the owner's question about
frequency directly: over the 24 miniatures **40 of 73 prefix-connected model bodies — 55 % —
are joined to another body by nothing but a corner** (`sensitivity.
a_bodies_merged_under_8_connectivity`). On `Tavey` it is 12 of 14.

That is not a rounding effect, and the `c0` row prices it: moving I7 and I10 to the loose rule
instead removes **173 of I7's 415 island failures (−41.7 %)** and **848 of I10's 6 935
(−12.2 %)**, and hands `Round 100` a gate pass it does not have. The connectivity choice is
worth more than four tenths of one invariant's entire output. Nothing in the tool recorded that
until now.

### 5.3 What Rule K1 does at the three in-plane sites

Among the three in-plane sites, I9 alone moves, and it fires on **9 of the 31 files** (I6 is
the fourth site and is §5.6):

| file | I9 failed | `SPAN_UNSUPPORTED` | regions |
|---|--:|--:|--:|
| Round 100 | 3 → **15** | 3 → **15** | ×1.96 |
| Tavey Base | 9 → **12** | 5 → **7** | ×1.42 |
| Skulker Zombie | 8 → **10** | 0 → 0 | ×2.00 |
| Deinonychus | 25 → **26** | 3 → 3 | ×1.98 |
| Plague Zombie | 23 → **24** | 2 → 2 | ×2.02 |
| zombie-6-a | 7 → **8** | 1 → 1 | ×1.76 |
| Bat Cat Owl | 6 → **5** | 0 → 0 | ×2.30 |
| Crawler Zombie | 12 → **11** | 0 → 0 | ×2.10 |
| Decomposing Zombie | 20 → **19** | 1 → 1 | ×1.82 |

Corpus effect: `SPAN_UNSUPPORTED` **19 → 33 on the miniatures (+74 %)**, 40 → 54 over all 31
files (+35 %); `ISLAND_UNSUPPORTED` 241 → 244; down-face regions **×1.94** on the miniatures and ×1.96 over all 31 files. No file
gains or loses a gate — the newly-refused regions are on files that were already failing I9.

**And now the caution the count needs.** `max_reach_mm` is **identical to the last digit on
all 31 files** between `base` and `c1`. No pixel got further from its carrier, so K1 did not
expose material that the loose rule had certified: `support_borne` never lost a carrier that
mattered to a worst-case pixel. What changed is the **partition** — the same over-length
material, previously glued into a few large regions by corner contacts, is now reported as 14
more separate regions, 12 of them on Round 100. That is a real improvement in what the output
says (a region is the object the criterion grades, and gluing two of them together through a
corner reports one defect where there are two) but it is **not** 14 newly-discovered defects,
and this document will not claim it is. Whether any sub-maximal pixel reach moved is not
separated by this sweep (§6.7).

### 5.4 The three files that got *better*, and why it is a defect

Bat Cat Owl, Crawler Zombie and Decomposing Zombie each lose one failure under the **stricter**
rule. Verified on all three, and it is the same mechanism every time:

| file | `regions_censored_one_pixel` | `regions_infinite_reach` | `max_reach_mm` |
|---|--:|--:|--:|
| Bat Cat Owl | 1 → 3 | 6 → **5** | 0.802429 → 0.802429 |
| Crawler Zombie | 1 → 6 | 12 → **11** | 0.938030 → 0.938030 |
| Decomposing Zombie | 2 → 5 | 19 → **18** | 1.391429 → 1.391429 |

The geometry does not move — `max_reach_mm` is identical to the last digit. A failing island
was fragmented until its pieces fell under the one-pixel area censor and stopped being
reported. Corpus-wide the censored count goes 165 → 208. **This is corollary K4's reason for
existing**, and it is why K4 is binding rather than advisory.

### 5.5 Why `c12` is evidence about the implementation, not about the physics

`c12` applies §3.2's conservative end — 12 shared row-faces — as a **local test between one
pair of runs in adjacent rows**. The corpus refuses it, and the refusal is unambiguous:

- `sphere-first-artifact`, a ball, goes from **1 detached island to 307**, 216 of them failing.
- Myconid Adult goes from 1 prefix-connected body to **47**; Round 25 from 1 to **74**.
- Over 31 files, I7 failures go 434 → **76 576** and I10 refusals 7 439 → **140 219**.
- **Two files gain a gate**: Round 25 (I9 1 failure → 0) and Round 50 (6 → 0), and in both cases
  `max_reach_mm` is unchanged to the last digit while `regions_censored_one_pixel` rises
  (14 → 26 on Round 50). A stricter rule handed two files a pass.

A smooth sphere is not made of 0.17 mm-wide connections, so 307 islands is not a finding about
the sphere. It is exactly the arithmetic error §3.4 names: **the joint
between two candidate regions is the sum of shared faces over the whole interface, and a
per-run-pair overlap is not that sum.** Two adjacent rows of any curved silhouette overlap by
few columns wherever the boundary is steep, and a local threshold cuts there.

**This is why §3.1's Rule K ships as K1 + K4 only, and the whole load tier — the `A_JOIN_MIN`
floor as well as the load-proportional term — goes to the cut-level neck check of §3.4.** The
conservative end of the bracket is still taken; it is taken as the neck check's threshold,
where a cut criterion can be computed on a cut, rather than as an identity predicate where the
corpus has now demonstrated it measures the rasteriser instead of the resin.

### 5.6 I6, measured separately

Site 6 needed a second build, because `components_26` sits in a different function from the
three in-plane sites. Same corpus, same method, and the same provenance check: with the knob
unset this build reproduces the committed I6 numbers file for file — Deinonychus 135, Morel
base 132, Tavey 65, corpus total 920 failed records over 31 files.

Replacing 26-connectivity with K1 — face adjacency in 3-D, so a voxel touching only along an
edge or a corner no longer joins — over all 31 files:

| | `base` (26-conn) | `c1` (K1) | |
|---|--:|--:|--:|
| I6 records failed | 920 | **987** | +67 (+7.3 %) |
| I6 indeterminate | 56 | 56 | unchanged |
| contacts checked | 1 587 | 1 587 | unchanged |
| clause-b breaches | 26 | **93** | ×3.58 |
| total breach volume, mm³ | 0.0073682 | **0.5111456** | **×69.4** |
| worst exterior-component count on one contact | 3 | 6 | |

**18 of the 31 files move, and every one of them moves in the failing direction** — not one
file's I6 count goes down. The largest movers are Decomposing Zombie (58 → 73 records, 2 → 17
breaches), Deinonychus (135 → 145, 12 → 22), Giant Wolf Spider (18 → 23, 1 → 6) and
zombie-1-a (18 → 22, 0 → 4). Ten files go from **zero** clause-b breaches to a nonzero count.

**Unlike I9's, this is a detection change, not a re-partition**, and the volume column is why.
`breach_vol` is an exact voxel count of support material cured outside the seat and *not*
connected to the arm's own exit path. Under 26-connectivity a lump touching the exit component
at a single corner was absorbed into it and counted as zero breach; under K1 it is counted.
The corpus total goes from 0.0074 mm³ — 554 voxels, essentially nothing — to 0.511 mm³. The
old number was not a measurement of how much support material sits illegally inside these
models; it was a measurement of how often a corner touch let the check look away.

This is the site where the connectivity choice was doing the most damage, and it is the one no
specification ever discussed.


---

## 6. What this rule cannot see

Required by `docs/specs/audit-contract.md`, and the list is not short.

1. **It cannot see whether two cells actually fused, and the instrument that would say was
   never read.** Everything in §1.2 rests on `δ`. `W5A`, `W5B` and `Q2` were printed on plate 5
   to measure printed-vs-nominal width and no reading of any of them exists in `docs/`. K1 is
   an *epistemic* ruling — corners contribute zero because their contribution is unknown — and
   a `δ` measurement supersedes it. It is entirely possible that `δ ≥ 0.005 mm`, in which case
   a bishop contact really is worth about one rook face and K1 is over-strict by a factor the
   corpus would feel.

2. **It cannot see a joint that is legal at every step and inadequate as a whole.** A 1-pixel
   ribbon 500 pixels long is one region under K1, at every link. The cut question is K2's, and
   K2 is not implemented anywhere yet (§3.4). Until the neck check exists, the tool has no gate on a thin
   isthmus in model geometry, and this document does not create one.

3. **It cannot see across layers.** Two cells that touch only at a corner in layer `k` may be
   solidly joined by material in layer `k±1`. K1 splits them in-plane anyway, and the vertical
   clause that would notice is out of scope by §4 #9/#10. So K1 under-merges by an amount
   nobody has bounded.

4. **It cannot see a false split created by the fill rule itself.** Full containment
   (`audit-invariants.md` §1.6) erodes about one pixel of rim. Two solids that genuinely
   overlap across a wide joint can raster to a corner touch — and then K1 reports two regions
   where the geometry has one, at any size. This is the smallest defect K1 cannot see and it is
   unbounded in consequence: **the erosion decides the topology.** `audit-span-criterion.md`
   §8.10 books the same problem for reach ("removing its rim … can disconnect a thin neck");
   K1 inherits it and makes it decide region identity rather than only region length.

5. **It cannot see the difference between shear and tension, or the notch.** §2.4. Both are
   unmeasured, both point the same way, and `A_JOIN_MIN` is not derived from either — it is
   derived from where the calibration stopped.

6. **It cannot tell "no evidence" from "evidence of adequacy" below 0.310982 mm².** §3.2's
   `N_held` end was rejected as an overread, and that rejection is a judgement, not a
   measurement. Every joint this rule certifies between 7.853982e-3 and 0.310982 mm² is
   certified by extrapolation from specimens that were never loaded near the boundary at that
   size. If a coupon ever loads a small section at a small ratio, that result governs and this
   rule's threshold moves.

7. **It cannot see how much of the corpus effect is region-finding and how much is
   `support_borne`.** §4 sites 2 and 3 are the same function, and the measured `c1` delta in §5
   moves both at once. Separating them needs a further sweep that was not run.

8. **It says nothing about whether a region, once identified, is adequately supported.** That
   is I2, I3, I4, I5, I7, I9 and I10. This rule only decides what "a region" is — and by
   deciding it, changes what every one of those counts means. §5's region totals move by a
   factor of two with the geometry untouched; no count in this tool is comparable across a
   change to this rule, and any document quoting one must say which rule was in force.

9. **K4 is ruled, not measured.** The probe that produced every number in §5 implements K1
   without K4, so the three censor-driven decreases of §5.4 are still inside those totals: the
   real K1 effect on I9 is larger than +17 failures by however many fragments K4 rescues, and
   that quantity was not measured. K4's effect at sites 1 and 4 (I7's island censor, I10) was
   not measured at all — those sites already satisfy K1, so the sweep moved nothing there.

---

## 7. Reproducing the measurements in §5

```sh
# a scratch copy of the crate, never committed: one env knob wired into the three
# in-plane sites (raster.rs:640, unsupported_span.rs:470, arrangement_spread.rs:738),
# each defaulting to its shipped behaviour when PLINTH_CONN_MIN is unset;
# a second copy wires the same knob into intersection.rs:994.
RUSTC_WRAPPER="" cargo build --release          # sccache is broken here
PLINTH_CONN_MIN=<0|1|12> ./target/release/plinth-audit <file>.chitubox --json --out-dir <dir>
```

Corpus: the identical 31 files the baseline used — 23 under
`plinth-rerun-2026-08-25/runs/*/out/`, `smoke/out/Round 25.chitubox`, and
`exports/*.chitubox` less the treant. Four modes x 31 files, `xargs -P6`, plus 2 x 31 for the
I6 build. Every model completed; no timeout, no budget exceeded.

**Provenance check, run first and passed:** with `PLINTH_CONN_MIN` unset the probe reproduces
`docs/evidence/audit-baseline-hardened.md` exactly — 12 gate-passes over 31 files; over the 24
miniatures I7 415 of 1,344 islands, I9 260 of 691,885 regions split 19 `SPAN_UNSUPPORTED` /
241 `ISLAND_UNSUPPORTED`, I10 6,935 of 17,478 body-layer evaluations. Four independent
published totals, all reproduced, so the deltas in §5 are attributable to the knob and not to
the copy.

Per-file JSON was written to a scratch directory and is not committed: run outputs do not
belong in git.

---

## 8. What this document changes, and what it asks for next

**Changes now** — K1 and K4, at the five sites §4 assigns them (1, 2, 3, 4, 6). Two invariants'
numbers move: **I9** (§5.3 — a finer partition and +14 `SPAN_UNSUPPORTED` regions) and **I6**
(§5.6 — +67 failed records, breaches ×3.58, breach volume ×69.4). I7 and I10 do not move,
because their sites already satisfy K1 — but K4 is new to them and its effect there is
unmeasured (§6.9).

**Does not change** — `is_attached` and its twin (§4 #9, #10), mesh and graph connectivity
(#11, #12), the 8-connectivity disclosure instrument (#5), I2's 6-connected patch (#7, already
compliant), I3's quadrature lattice (#8).

**Asks for next, in order:**

1. **A `check_passes` fixture.** The harness supports one and nothing uses it
   (`audit-baseline-hardened.md` §3.4). A connectivity rule that only ever splits things needs
   a known-good file more than any other change in this tool: without one, K1's +35 % on
   `SPAN_UNSUPPORTED` is indistinguishable from a rule that refuses more because it refuses
   everything. Two fixtures are the minimum — a bishop-only pair that must read as two regions,
   and a rook-joined pair that must read as one and must **pass**.
2. **Read `W5A`, `W5B` and `Q2`.** They are printed. `δ` is the single unmeasured number this
   whole document turns on (§6.1), and reading three witness pads is cheaper than anything else
   here.
3. **The neck check (K2).** §3.4. It is the check that answers the owner's real question —
   "what is enough of a connection" — for joints above the identity threshold.
4. **The vertical question.** §4 #9. One cured pixel column carries 12 pixels of down-face by
   the same law that produced `A_JOIN_MIN`; the tool currently accepts one column as
   attachment. That is the same question this document asked in-plane, asked in Z, and it is
   the larger of the two.
