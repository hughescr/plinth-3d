# Physics

The support engine sizes structure from a failure boundary that was measured on printed
calibration plates, not from a heuristic. This document is the account of what is modelled,
what was measured, the owner rulings that govern seating and clearance behaviour (each dated
by when it was ruled, not narrated), and where the model stops being valid.

---

## 1. What is modelled

**Peel is the only load.** During a print the dominant force on a support is the peel that
separates the freshly cured layer from the release film. There is no gravity, buoyancy,
resin-density or viscosity term anywhere in the engine — a resin's density cannot calibrate
anything here, because no formula consumes it.

That is a deliberate simplification and a real limitation. It is sound for the regime the
engine was measured in: a tilt-release machine, minis at this scale, green resin. It has not
been tested against geometries where self-weight could plausibly matter.

## 2. The load rule: an area ratio, never a force

For one support arm carrying one patch of down-facing model:

```
X = A_supported / A_arm_cross_section          (both mm², X dimensionless)
```

- `A_supported` — the down-facing model area whose peel load that one arm carries.
- `A_arm_cross_section` — the arm's load-bearing cross-section at its narrowest point.

**Higher X means more area per unit of arm, and therefore more likely to snap.** The engine
never computes a force. It sizes to the ratio.

### The measured boundary

The boundary was measured directly, on plates of purpose-built coupons — a cap of known area
carried on a tip of known diameter — printed and then pulled. Records:
[`docs/engineering-journal/reference/plate-05-results.md`](engineering-journal/reference/plate-05-results.md)
and [`plate-06-results.md`](engineering-journal/reference/plate-06-results.md) (the plate 5 and
plate 6 calibration results, code-pinned and never moved); plate 6's review memos are in
`docs/evidence/` (`plate-06-verification-protocol.md`, `plate-06-verification-physicist.md`).

Two earlier plates were also printed and are recorded in the same directory, but **neither
carries this law**: `plate-01-results.md` bounds peel pressure from one side only (nothing
on it failed), and `plate-04-results.md` bracketed a critical overhang length but produced
no usable quantitative figure — its factors were assigned by formula rather than
counterbalanced, so the result is real and unidentified. They are cited here so their
absence from the law above reads as a decision rather than an omission.

| | |
| --- | --- |
| Design | Three cap areas (3.1, 19.6, 122.7 mm²) crossed against tip diameters spanning 0.100–2.500 mm, in two independent rows |
| Result | Every specimen at **X ≤ 25.3 survived**; every specimen at **X ≥ 25.0 failed** |
| Boundary | **X\* ∈ [25.00, 25.34]**, with exactly one inversion inside that bracket |
| Span | Survivors from X = 0.64; failures out to X = 6229 — about four decades |

Quote it as an interval, not as `25 ± something`. The plate brackets the transition; it does
not resolve inside the bracket. Outside the bracket the answer is unambiguous in both
directions.

### The allowance the engine actually uses

```
X_ALLOWED = 18        (Craig ruling, 2026-09-30; was floor(25.00 / 2) = 12)
```

so every arm satisfies `A_arm ≥ A_supported / 18`: 1.389× below the plate-5 cone boundary
(25.00) and 1.106× below the plate-6 cylinder bracket's lower edge (19.91). The ruling
followed the 2026-09-30 over-support attribution, which found every law-sized tip at exactly
`A_supported / (π · 12)`, so the old factor of two was the whole margin. It is registered as a
Measured owner decision, bounded by both plates (the registry refuses to construct if either
lower edge falls to 18 or below), and is no longer derived from them.
`docs/engineering-journal/03-physics-and-constants.md` §3.1 carries the ruling note.

Until 2026-09-30 the text below held: `X_ALLOWED = floor(25.00 / 2) = 12`, so every arm
satisfied `A_arm ≥ A_supported / 12`, the measured boundary with a factor of two held back. A second, independent plate using constant-diameter cylindrical arms rather
than tapered cones bracketed the boundary at `X* ∈ (19.91, 30.48)`, which leaves the
allowance **1.66×** of margin against the pessimistic end of an experiment it was not
derived from. That is a cross-check, deliberately not a second derivation input.
`X_ALLOWED` and `X_STAR_CONE` are registered constants (`docs/engineering-journal/m0-constants-source-ledger.json`,
current values confirmed unchanged: `X_STAR_CONE = [25.00, 25.34]` Measured, `X_ALLOWED = 12`
Derived from it).

### Three findings that shape the design

**(a) Load scales with supported *area*.** Not with perimeter, and not with a squeeze-film
law. The plate deliberately crossed cap area against tip diameter; a perimeter or
squeeze-film law would not collapse onto a single `A_supported / A_arm` threshold across
three cap areas and a 25× span of tip diameters. It did.

**(b) The failure section is the narrowest cross-section of the arm just above the contact
— not the model contact patch.** Both the conical and the cylindrical plates put the
boundary in the same place, so this is not an artifact of taper, and equally there is **no
measured taper penalty** to claim. Two consequences:

- A long slow taper is a liability. An arm should **flare fast above the tip**.
- Sizing one radius at the contact is not enough. The rule applies at the minimum section,
  wherever along the column that is.

**(c) `X ≤ 12` is necessary, not sufficient.** The ratio is blind to arrangement. Three
collinear arms have no lateral stability perpendicular to their line, at any X. An
arrangement check has to include a spread criterion, not just a per-arm ratio.

**This is the support-arm area law**: `supported_area / arm_section ≈ 25` is where arms snap;
the engine designs at `12`, half that boundary.

## 3. The scar invariant

Every support leaves a scar. For a down-face of area `A_face` tiled by arms each carrying a
cell of area `A_cell = X · A_arm`:

```
N     = A_face / A_cell
Scar  = N · A_arm = A_face / X
```

The arm cross-section **cancels**. Thin-and-dense and thick-and-sparse leave exactly the same
total scar area. At `X = 12`:

```
Scar / A_face = 1/12 = 8.33 %
```

**8.33 % of every down-face is scar, however you tile it.** This is the design frame, and
several intuitions die here:

- Shrinking tip diameter as a scar-reduction strategy is arithmetic that has already
  cancelled. Scar *count* and scar *placement* are free; scar *area* is not.
- The only two levers are `X` itself — which trades scar directly against snap risk, and is
  a physics decision rather than a tuning one — and **where the scars land**.
- Placement is therefore a scar-cost-weighted problem: put the invariant 8.33 % where it
  hurts least. That is why orientation, which decides what faces down at all, is the hub of
  the whole optimization.

**Arrangement.** For a fixed worst-case gap `R` (no point of the face further than `R` from
an arm), a triangular/hexagonal lattice needs cells of `≈2.598 R²` against a square grid's
`2 R²` — `0.385/R²` points against `0.500/R²`, about **23 % fewer points for the same
worst-case gap**. Seed hexagonally.

On real curved geometry there is no lattice, but the identity survives: **each arm's Voronoi
cell area on the down-face is its `A_supported`**, which turns placement into sizing with no
extra model:

```
for each arm:  A_arm_min = area(voronoi_cell(arm)) / X_ALLOWED
```

## 4. Load sharing and peel direction

**Sharing is real and at least additive.** The natural worry was a zipper: if a moving
release front reaches a group of arms one at a time, each meets it alone at full X. That was
tested directly, and the zipper did not happen — strips of arms along *both* horizontal axes
survived at a per-arm ratio of 60.15 while a matched single arm at the same ratio died.

The engine sizes each arm for its own full cell, which is now known to be **conservative
rather than correct**. A measured sharing law can only ever relax that, never tighten it, so
nothing already shipped becomes unsafe.

**Peel direction (Saturn 4, tilt-release).** The target machine tilts the tank and the
release front sweeps front-to-back rather than lifting straight off, so layers release
front-to-back and a model's position along that axis changes its peel load. **Y-sensitivity
is settled**: the measured effect is small, around one rung of the test ladder from front to
back. The standing guidance is to **randomize over that axis rather than tune it**, and to
spend replication budget on build-to-build variation instead. There is no orientation term in
the sizing path today; the input is plumbed and unused.

## 5. Clearance: two demands with two different jobs

Two registered clearance numbers govern how close a member may pass another surface, and they
are not the same requirement measured twice:

| constant | value | job | can it refuse a model? |
| --- | --- | --- | --- |
| `STANDOFF_MM` | 0.600 mm true surface-to-surface gap | **placement-family search margin** — prunes candidate tournaments (CCO candidacy, rewire changed-set validation, brace candidate gathering) | **No.** A candidate it drops just loses a tournament entry; `Unplaceable` is an accepted termination (owner ruling, 2026-08-22) |
| `FINAL_CLEARANCE_GAP_MM` | 0.300 mm true surface-to-surface gap | **the final, refusing demand** — the only clearance number in the engine that can condemn a placed member, over every contact-declaring and non-contact member alike | **Yes.** This is the pass whose verdict refuses geometry |

**Both stages call the same shared clearance predicate; each reads its own registered value
through `ContactPolicy`, so the math is never forked.**

**Provenance, ruled by Craig 2026-08-22, verbatim: *"0.3mm gap features are preserved fine in
prints."*** `FINAL_CLEARANCE_GAP_MM` is registered as **Measured**, class `external_requirement`
— it is field data about what a printer can preserve, not a derived physical bound, and it is
registered that way on purpose so a future coupon measurement of gap-feature preservation
supersedes it rather than colliding with it (the gap-ladder coupon is deferred; see
`docs/plans/support-clearance.md`).

**The two-tier split has a history that matters to reading old numbers.** Before 2026-08-22
there was one demand, tested on a member's centreline rather than its true surface-to-surface
gap. Both numbers meant something different under that test — `STANDOFF_MM`'s 0.6 mm of
*centreline* demand was roughly 0.6 mm minus the arm radius of *true* gap once the arm's own
radius is subtracted, so the same registered number silently tightened by one arm radius when
the predicate moved to true-gap. The **honest-exemption audit (2026-08-22)** measured the
consequence directly on the Giant Bat: the old rule excused an arm standing 0.242 mm *inside*
the model while refusing a thinner arm on the same axis with 0.569 mm of real gap. The
two-tier split, read as true-gap throughout, is what frees the Bat from that inversion.

**The up-sweep reads both tiers on every descent (fix/drawn-solid-clearance, 2026-10-04).** An arm's
descent was tested by its centreline cell alone, and the radius it is drawn at was never in the
test, so a post could stand 0.325 mm from a leg face and be drawn 0.374 mm inside it (overhang
table member 30). Every descent, the straight one included, is now read as the solid the file will
draw: its straight axis, outside its own seat zone and (on the model) its landing zone, at a LOWER
bound on its radius (the larger of the registered minimum tip radius and the area-law radius
`√(A / (X·π))` of the demand its seat covers), against the cured model. The candidates keep their
order (cheapest first, a tie to the straight descent). A straight descent that clears the 0.3 mm
final gap STAYS: the 0.6 mm margin is a placement-family search margin and only the final gap
refuses geometry (and the Round discs, whose posts stand 0.3 to 0.6 mm from their ring's walls,
are the byte-identity control). Where the straight descent does not clear the final gap, a
costlier lean is admitted, and the preferred margin chooses: a candidate that clears 0.6 mm beats
one that clears only the final gap, which beats none. The zone exemption grows with the gap
asked, so a descent counts as clearing the preferred margin only if it also clears the final gap
at the final gap's own zones (a short descent can lie wholly inside its zones at 0.6 mm and still
have a testable stretch at 0.3 mm). Where no descent clears the final gap, the
seat may move only to a cell whose reach disc covers the SAME demand
(`up_sweep::reseat_cells`); where none does, the seat is kept and the census names it (emit and
warn). Each shaft is excused only its OWN contact zones: the seat zone is where the capsule first
stands `gap` clear of the seat face's plane, `(gap + r − h) / (n·m)` along the axis (`n` that
face's outward normal, `m` the axis from the seat to the base, `h` the seat's height over the
plane), and a model landing's zone is the same at its landing face; neither is taken longer
than the longest the lean law admits (`LeanLaw::face_zone_factor`, `n·m` never below
`cos(steepest lean)`). A flat seat's zone is `gap + r`, not `1.86 (gap + r)`, so a short branch
under a flat seat is tested below it. The raster only flags (its reach carries half a cell's
diagonal); a flagged shaft is confirmed by the kernel's clearance predicate, the census's own,
on the same axis and zones, and refused only when that reads it under the gap. A thickening that would raise a member, or a trunk raised with it, into the final gap
where it stands clear now is refused (`ThickenRefusal::Model`) and the run takes the next rung;
the comparison is made slot by slot, the profile the plan commits against the profile drawn
there now, each read as the frustum it is (its clearance is the least over the axis of distance
minus radius), so a wider section or an existing violation elsewhere excuses nothing. Each of the
two solids is excused only its own contact zones, read from the faces its ends stand against as
above, at the radius that solid has at the zone's edge (a taper is wider there than at its end,
so the zone is taken to a fixed point); a trunk raised under a flat seat is tested below
`gap + r`, not `1.86 (gap + r)`. A slot the raster flags is decided by the census's own predicate
on the drawn frustum, all of a plan's slots in one batch: the raise enters the gap only where
the predicate PROVES it, a measured read under the gap or an upper bound under the gap (the
kernel's one-sided read, where it could not decide a near-parallel face, says the clearance is
AT MOST the number: an upper bound at or above the gap is undecided and refuses nothing, as the
census does not count it either), and a slot counts as inside the gap already only
where the predicate reads the drawn slot under the gap. The raster's flag is never that proof;
without a model surface nothing is proved and a flagged raise is refused. A zone is an
exemption, so where no face is known (no surface, or the query names none) the zone is zero,
never the longest: the raster then reads the solid up to its contact, the flag reaches that
refusal, and such a member is never counted inside the gap already. (The descent test without
any surface keeps the longest zones, since placement cannot refuse every seated shaft; the
census, which cannot run either, warns.)
The lip-seat gate is unchanged: a member may land "no nearer than it stood".

**One support per spike (quality lane Q2, owner ruling 2026-10-10).** A sharp tip first appears as a
hairline the raster covers but does not carry, so the next layer, the first that carries, finds its
column unrooted and seats a second arm one layer above the first (before the rule, 53 pairs of tip
balls overlapped on the Deinonychus, 29 of them under 0.1 mm apart). The second arm's shaft stood
in the first arm's head's way (`HitsMember`, 49 of 68 refusals), and with no head the whole arm
leaned to the plate.
Now a seat whose tip ball overlaps the ball of an arm already seated in the SAME column (`|c1 - c2| <
r1 + r2` at the smallest tip the area law allows for each seat's demand, times the registered ball
factor; both balls on the column's axis, so the distance is the height between the seats, read from
where the earlier arm first seated) raises that arm to the new layer and gives it the demand the
seat would have covered (`up_sweep::twin_host`, `raise_host_seat`). The raised arm keeps its landing,
post and head; it is alive on the new layer, so the down-sweep routes the twin's load into its
contact and sizes it for both. Dropping the second seat WITHOUT raising the first was built and
measured first and is wrong: no arm was alive on the new layer, and 75 mm2 of Deinonychus load
reached no carrier (`tests/it/twin_merge.rs` grades this). Twins in two different columns (about
6 % of the pairs) are not merged: the raised arm would have to change column. A pair that overlaps
only after the sizing has grown the tips is not seen at seating.

The arm a twin was merged into keeps the place its twin gave it (`SweepStrut::twin_host`). Before the
merge the twin's shaft landed on that arm, and an arm another member rests on is never re-routed by the
passes that move arms: the congestion lean, the lip-seat re-landing, the graze conversion and the
slenderness merges (`landing_check::carries_someone`); lip-seat also leans a whole arm along one axis
when its tip meets a steep face, which is the 30-46 deg post this rule removes. A merged host that those
passes could move leaned away as a lone leaf: first measured at 5.55 mm out for a 0.018 mm sliver
between two census root discs (Deinonychus arm 183), and four ridge posts that stood vertical leaned
33-46 deg over a free vertical landing. The host is therefore counted as carrying, in `carries_someone`
and in the congestion lean, and lip-seat locks it outright. A bite it cannot lose stays a seat-trim WARNING.

**A contact that carries an island keeps the reach it had (quality lane Q2, round 3).** With the twin
arms gone, a Deinonychus arm whose head had been refused for `HitsMember` (the twin's shaft stood in the
way) could now take the graze head (`graze::convert`: 1 conversion before the rule, 2 after). The
straight arm it replaced was slid 0.18 mm along its seat face by the footprint rule, whose whole job is
to put the contact disc on the face; that slide put its ball's section at the birth layer of a 24-pixel
detached island 0.14 mm inside the island's contour (audit island at layer 169). A head is frozen at
placement and the footprint rule never slides it (`Footprint::MoveRefused`), so the converted arm kept
its ball at the seat cell, 22 % of its disc off the face, and its section at the same layer missed the
island by 0.03 mm. The cause is not the merge's routing but the order of the passes: the graze
conversion plans the head before the footprint slide exists. The rule now: a grazer whose straight tip's
contact disc is not wholly on its face (graded by `footprint::regrade`, the footprint rule's own disc,
lattice and rim, in ONE batched pass over the grazers) is left straight, named `SlidesToItsFace`, and the
slide runs as it always did. No tolerance is new; the grade is the footprint rule's.

**The clearance re-route after the final sizing.** The up-sweep places an arm at a lower bound
of the radius it is later drawn at (the area law and the thickening grow it), so after the final
sizing and the ties the census names every arm whose drawn solid stands within the final gap
of the model. Each is re-landed at its sized radius with the lip-seat rule's machinery (priced
descents, path test, the lowering's measurement, the sized-field put-back for a host's load or a
lapped root), to a fixed point bounded by the registered pass count, with one difference: the
mover must clear the final gap outright, since it was sent for the gap. The arms the lip-seat
rule itself re-landed are left to that rule's gate (they stay warnings); an arm no candidate
clears stays and is named by the census (emit and warn). An arm that carries part of a frame the
slenderness ladder solved (a cross seated on it or on a member resting on it, or a brace, mast or
model tie resting on it) is kept where it stands and named in a WARNING with that reason: moving
it moves a junction of the frame, and carrying the frame along unsolved, or taking it down and
solving again, could leave a post over its slenderness limit (sway or snap mid-print) to buy
clearance for a member that already stands clear of the model. The re-route lands an arm only
on another ARM, never on a brace: the column history holds the cross diagonals too, and a
diagonal is sized for drag, not for a load path. When the re-route moved anything, the tie passes
and the model-tie passes run again on the field it left (they act only on runs over their limit),
because the move can make a run (a branch on a new host) or lower a limit (the old host's re-size
makes its sections thinner).

**The final gap does not apply inside a hidden cavity** (owner ruling 2026-10-05, on the
2026-08-31 ruling that supports hidden in a cavity are never removed or cleaned). The test is
the cavity itself (`cavity.rs`), not the scar flag `SweepStrut::interior` (model above the landing
in the same column), which is also true of a recess open to the outside and false for a member
that lands on another support. One flood per model, at the hollowing pitch: a voxel of a lattice
that reaches past the model is solid only when every layer it spans holds a WHOLE cell of the model
there, proved from the slice's geometry in both horizontal axes (the cell's centre inside and no
boundary edge through the cell's interior; not any overlap: an any-overlap mask rounds a span
outward, and an opening of 0.8 mm across two cells was sealed by both neighbours; and not a
row-centre sample: a slit narrow in Y between two row centres sampled as solid wall), the voxels
below the plate are solid, each drilled hole
(drain or vent) mouth is plugged with a ball wider than the bore (the bore does not open the
cavity), and the empty space is flooded from the lattice boundary. Empty voxels the flood never
reaches are enclosed air; a voxel is hidden when enclosed air lies within one voxel and no
exterior air does. A member is hidden when every sampled point of its axis (the foot, one point per
half voxel of height, the seat) is hidden, whatever it lands on, AND the member's solid, at the
radius it is drawn at or the plan would draw it at, stays in hidden voxels (so a raise that would
carry it to an exterior face is read as before, and one that touches the cavity wall a voxel deep
is not); a proposed landing, raise or tie is asked with its own geometry before it exists (a
re-route's mover on its proposed landing and path, and each child that follows it on its proposed
path onto the moved axis, at the radius its trial arm is drawn at, never on the axis it stood on).
A hidden
member is not read against the gap by the thickening refusal, is not counted by the census (so it
is never sent to the re-route), a mover that would land so is not read against the gap by the
re-route's landing search, a straight descent of the up-sweep (the cavity map is built before the
up-sweep, from all the slices) that stands so is not refused for the final gap (the model-crossing
and drain tests of the lean search are unchanged), a model tie so placed is confirmed clear by its
lone census, and a brace band whose diagonals stand so is not read against the model (its path
test still refuses a diagonal through model material). Every error runs toward "not hidden", which
is the old behaviour: an opening of any width in either horizontal axis and at any phase to the
lattice, a wall that holds no 6-connected run of whole cells (under 1.0 mm square to the lattice,
more on a slant; the hollow stage makes none under 2.0 mm) and a cavity too narrow for the lattice
exempt nothing. One limit stands: a lift under one voxel could not be told from a cup. Outside a
cavity nothing changes.

## 6. The seat's own contact zone — the honest exemption

A member seated into the model is legitimately inside the material it stands on. Something
must be exempt from the clearance test in §5, or every seated arm refuses itself against its
own contact.

**Until 2026-08-22 the exempt thing was a length** — an arc-length mask computed from the seat
depth and incidence, skipped from the contact end before the clearance test ran. That mask is
retired. The measured defect: the skip is proportional to radius, so a *fatter* arm was tested
over a *shorter* stretch of its own axis, and on the Giant Bat that inverted a verdict — an arm
at radius 1.71 mm passed while standing 0.242 mm inside the model, while the same axis at
radius 0.90 mm was refused with 0.569 mm of true gap. Both refusals, when it mattered, were
**foreign-feature proximity** — the opposite wall of a crevice — never the seat's own surface.

### The rule now in force

**Exempt a target, never a length.** The full axis is tested (no arc-length skip). What is
exempt is the connected surface component containing the seat's own face:

```
Z = the connected component, containing the seat's own face, of
    { x on the surface : dist(x, axis) < r_seat + standoff }
```

`Z` is **frozen** at contact creation, against the surface, once. A **discrete disjoint sheet
can never enter `Z`** — that is what stopped the Bat's foreign-wall inversion: the far wall of
a crevice is a different connected component from the seat's own wall whenever the crevice is
wide enough to be two sheets, and it stays in the tested surface.

**The 2026-08-23 re-cut ruling.** Craig ruled, verbatim: when the sizing pass thickens a
seated arm past its creation radius, *"the exemption always equals the arm's true footprint,
no slack."* So `Z` is re-frozen once, after each sizing pass that moves a seated member's
radius, to the member's **final** footprint — never padded at creation, never left stale.

### The monotonicity guarantee, narrowed to foreign surface

Fix an axis and a frozen `Z`. The graded clearance is `C − r_max − gap` with `C` a property of
the axis and the frozen face set alone, so within **one grading pass, against one frozen `Z`**,
a thinner arm can never grade worse than a fatter one on the same axis. That is a theorem, not
a measurement — but it is a **within-pass** statement: two gradings of the same member across
different sizing passes hold *different* `Z`s (the re-cut above), so the guarantee does not
extend across a run. What does hold across every pass, unconditionally, is narrower and more
useful: **a disjoint, foreign surface feature is never swallowed by `Z`, at any radius, in any
pass** — the far wall of a crevice stays in the tested surface no matter how the near arm
grows. The guarantee is against foreign surface, not against the seat's own connected wall.

### The crevice-forgiveness class is accepted behaviour

**Ruled by Craig, 2026-08-25, verbatim:** *"It's fine for a support arm in a narrow crevice to
touch the opposite wall in those circumstances - it's not ideal, but it's better than no
support at all."*

This settles the one remaining exposure the connected-component rule leaves: when a crevice is
*narrow enough* that both its walls are one connected surface sheet near the seat, `Z` can
legitimately swallow part of the opposite wall along with the seat's own — the rule cannot
tell them apart on a connected mesh, because in that geometry they are not separable features.
The ruling accepts that outcome as physically fine (arm-to-opposite-wall contact in a genuinely
narrow crevice, given the load involved, beats no support), rather than as a defect to chase
further. Per-arm disclosure of the forgiven area stays; the rationale is load, not tolerance —
an arm that would still be refused after losing the forgiveness stays refused.

### The footprint search is closed

Between the honest-exemption landing and the 2026-08-25 ruling, a dedicated search asked
whether any rule built from **current inputs** — an unlabelled STL mesh, the placed geometry,
and the grader's own clearance field — could separate "the seat's true receiver" from "a
foreign wall nearby" more precisely than connectivity does. Tested and rejected: a ball-shadow
projection, a tangent-plane/normal stencil, a crease/adjacency rule, and concentric distance
bands — all fail on the same fixtures the connected-component rule passes.

The strongest remaining candidate, **Rule Z** — exempt a point only when *every* nearest
boundary point of the live support solid is buried inside the model — was built, measured, and
**rejected**, 2026-08-25 (evidence: `../plinth-evidence/rule-z-2026-08-25/`, worktree checkout
`feature/rule-z-experiment @ 8803ff8`, kept unmerged as reference):

- On a plain wide slab it **false-refuses in 104 of 108 swept cells** (96 of those
  oracle-independently), starting at just 10° of tilt, with a worst retained clearance of
  **−1.54 mm**.
- On the deep-wall crevice it was built to fix, it **still forgives** 86 of 347 wall samples
  for a model-foot frustum and 223 of 347 for a production stub.
- On the Deinonychus fixture it **loses every refusal**: 4 final-pass refusal rows and 5
  abandoned passes both go to 0.

**Root cause, condensed so this is not re-attempted:** on an unlabelled mesh, burial is
approximately connectivity. In a narrow crevice, the far wall and the seat's own receiver *are
the same connected surface* — the same fact that makes the connected-component rule work at
all is the fact that makes every rule in this family unable to separate them there. The only
way out that current inputs offer is the one already tried (connectivity, now ruled accepted);
the alternative — author cavity/receiver identity onto the mesh instead of inferring it — was
considered and **declined**: it has no fallback on today's unlabelled corpus, so it would make
every model-seated support unavailable, including the Giant Bat, until every model carries
authored labels. **The footprint search is closed.** The connected-component rule plus the
2026-08-25 acceptance ruling is the standing answer.

## 7. Land-at-strike: when a stub's shaft runs into the model

**Craig's reading, which is the design (2026-08-22):** *"If it strikes the model in a line to
where it was supposed to be, then just land it there? ... if it's a valid spot to be the foot
of the support we can just land it there?"*

**The rule.** A support stub whose outward shaft strikes the model before reaching its
designed contact terminates at the **first strike**, with its foot seated on the struck
surface, rather than being discarded. Four rulings, all Craig's, all 2026-08-22, decide how:

- **Load-sized anchors.** *"If these struts are all under 2.5mm inside crevices then they're
  likely extremely thin and don't need a full 0.3mm to land in. ... as long as the physics
  says they'll be supported they can have much smaller anchors."* The landed foot's radius
  **and its embed depth** both derive from the load it carries — `h_design = ball_seat_factor
  (BALL_FACTOR_FOOT) × r_foot` — rather than a flat depth applied regardless of load.
- **Permanence, accepted as disclosed.** A landed strut is permanent (nothing in the engine
  models a cutting tool); this is not a gate, and each landed strut's permanence is recorded
  as a disclosure rather than refused or hidden.
- **Sub-width crevices are moot, not refused.** *"If resin seals anyways then the strut adds
  no extra resin. So this is moot I think."* Crevice width becomes a disclosure field, never a
  refusal.
- **Truncation at the opposite face — never exits.** *"A seat should be allowed to go as deep
  into the model as it likes, but it should not come out the other side of the model — so if
  the seat wants to go deeper than the thickness of the model, it should get truncated at the
  opposite face — it should not create a scar on the backside because that provides no
  additional support so there's no need for it to extend beyond the model anyway."* Depth stays
  unlimited; the seat, ball included, is clamped at the model's opposite face along its own
  line, so punch-through is impossible by construction: `h_eff = min(h_design, W)` with `W` the
  wall thickness measured perpendicular to the entry face.

**What this does not answer.** The lateral reach of a load-sized foot's ball can still exceed a
thin wall's own thickness even though the truncation clamp (which acts along the seat's own
line only) stops it exiting — a buried plug spanning a wall rather than a cap resting in a
half-space. That is a distinct, open question about whether the engine should price the wall
*around* the plug, not merely at the entry plane; it is design work, not a physics ruling, and
belongs to `docs/plans/support-clearance.md`.

### The seat normal is the footprint's average (Craig, 2026-10-05)

**Craig's ruling:** every support tip's seat normal is the AREA-WEIGHTED AVERAGE of the model's
outward normal over its actual contact footprint, never the normal of one nearest facet. The seat
law's `abs_cos`, the trim plane, the contact disc and the lip-seat approach angle all read it.

```
footprint = tip ball ∩ model surface,
            restricted to faces the seat-face rule admits (never a wall or a face past the band's
            steep end) and connected to the seat triangle through such faces across shared edges
            that themselves enter the ball
n̄         = normalize( Σ area(f) · n(f) ),   area(f) = exact area of triangle f inside the ball
```

A triangle's weight is its exact clipped area: the ball cuts the triangle's plane in a disc of
radius `√(R² − d²)`, and the area is that disc's overlap with the triangle, in closed form (a sum
over the three edges of the signed area of the disc inside the fan from its centre). So a patch cut
into triangles another way has the same total weight on each plane, and the normal does not depend
on the tessellation. The connected patch is the connected component of `ball ∩ surface`: two
admitted triangles join only across a shared edge that enters the ball (an edge wholly outside the
ball joins the two sides a finer cut of the same surface would leave apart), and the walk starts at
the seat triangle by identity. Where the seat triangle's patch does not reach the ball (the ball
overlaps only another sheet), or the area is nil, the tip keeps its seat triangle's normal and the
count says so (emit and warn); the walk never re-seeds on another sheet. Where the average equals the seat
triangle's normal to rounding, the triangle's bits are kept, so a flat underside (every Round disc)
is not perturbed. Because the admitted band's steep end (57.5° from horizontal) and the lean law's
steep end (57.5° from vertical) are the two ends of one registered band, any average of admitted
normals is inside the printable cone and needs no clamp. Model ties are not averaged (a tie may seat
on a wall, decision D8). The ball depends on the seat depth, which depends on the normal, so the
measurement runs twice (seat on the facet normal, measure, re-seat on the result, measure again);
the largest angle between the two answers is published. The normal describes the geometry the file
gets: each proposed slide of the footprint rule is re-measured at the MOVED ball (the seat triangle
re-derived there) and the disc graded, the section test and the accepted move use that normal; and
the emitted field (seat slides and followers' ends applied) is measured once more, a tip whose normal
changes has its disc graded again and a disc now off its face is warned about. Built in
`plinth_sweep::footprint_normal`
(`tests/it/footprint_normal.rs`); never run in the orientation prefilter.

Land-at-strike is a **ratified design principle** — the standing answer for how the engine
should treat a stub that runs into the model — rather than a claim that every piece of it is
built. Build status belongs to `docs/status.md`.

## 8. The three-seat rule and model-seated arms

**Craig's ruling, verbatim (2026-08-13), which is the authority for this section:**

> "We DO want supports inside the model. The hollow must be supported. But those supports will
> likely land on the lower part of the interior of the cavity, not directly to the base plate.
> The connection to the base plate would be via the model itself and its outer supports. In
> general supports should count as valid if they land on the model, on the base plate, or on
> other supports as long as the entirety is adequately supported per physics."

A support is legitimately seated when its root end is one of: **the build plate**; **the model
surface**, including a cavity's own interior wall (a cavity ceiling must be supported from the
cavity's lower interior); or **another support**. The test in every case is a closed load path
per physics, not which of the three a member happens to touch. Standing on the plate and
standing on another support (bifurcation) were already ordinary, working paths. Standing on the
model surface was not, until the seating fix below.

### Model-seated arms now actually seat

A model-anchored arm's root end is seated below the surface by the same seat-depth and
clearance-mask machinery the brace-rung family uses — `contact::seating_of` classifies a
`Strut` end at `ContactEnd::Rootward` as `Seated`, not `OnSurface`. The clearance mask is one
shared statement (`contact::placed_mask_inset_mm`) read by every site that builds a
segment-clearance inset — CCO, rewire, and brace alike. An unseated witness is an ordinary
candidacy refusal (grazing or degenerate incidence, or a genuinely unattested seat), not a
last-instant tripwire on a winner.

**What is still open, not built.** Whether the *struck model feature itself* demonstrably
carries the transferred load onward to its own supports (as opposed to the contact simply being
attested and legally scarred) is an unresolved doctrine question, and keeping interior arms
clear of a cavity's drain and vent bores is an unbuilt refusal. Neither loosens anything already
shipped; both are future work — see `docs/status.md` and `docs/plans/support-clearance.md`.

### The articulated head is sized for what its contact can pass (Craig, 2026-10-11)

A headed tip is a contact ball, a bare link, a cone and a joint sphere on top of a post. Until
2026-10-11 the cone, the joint and the post were sized by a bending law of the lean (`heads.rs`)
for a load line the contact cannot pass: on Deinonychus support A a link of radius 0.428 mm
carried a joint sphere of radius 1.333 mm, and the post, the host column and the plate foot
inherited the size. **Craig's ruling:** the "post is never thinner than a joint" rule stays; the
joint is fixed at its cause.

The rule now (`joint_cert.rs` holds the bound and its proofs in comments, `joint_size.rs` the
pass that applies it):

* **A local capacity certificate, not a network-strength claim.** The resized section never fails
  before the contacts that feed it. Each feed is two real sections in series (the seat disc and
  the link; the ball equator and the taper for a straight tip; a brace's own section), and the
  loads that break neither are exactly a polytope in (axial force, the two end moments, squared
  twist). Photopolymer resin is brittle, so the criterion is the maximum principal stress
  (Rankine, `s + tau^2 <= 1`) with the same `sigma_allow` the area law uses; no new constant.
* **One authoritative profile.** The cone knots and the joint radius come from one solve and
  travel together through `Head::certified`, `head_payload` and the lowering, which draws them
  verbatim (the joint sphere is the last knot's radius, not a wider one).
* **Certified everywhere.** On a straight frustum the demand is `A/r^2 + B/r^3 + C/r^6`, whose
  maximum is analytic. A post slot takes the larger of its two end needs (the bound is convex
  along a straight record), summed over every source that loads it from its entry down: the head's
  contact, every member landed on it (and what is routed through it), every brace, and every
  member whose drawn solid is fused into it. The lowering's chords never draw below the slot step.
* **Kept as today.** A slot whose need is above today's radius, a slot a junction hull's ring
  stands on (the hull keeps today's geometry), and a head another member loads above its joint
  (shared) keep the old law and are counted (`joint certificate:` line of the log).
* **Report, never refuse.** The log prints the largest certified demand over the drawn chords and
  warns when one exceeds `1 + 1e-9`. The link-overstress warning is read at the link end that is
  drawn and says when the joint is sized for what the link can pass.

What it does not claim: stiffness falls as sections thin (the link dominates compliance), and a
thinner path moves a little load onto a neighbour. Only a frame solve or a coupon settles that.

## 9. Admissibility: emergence, area sufficiency, attestation, buried surface

**Seat depth** (unchanged since 2026-08-04, still the formula every seating family reads)
takes the largest of three requirements, each binding in a different regime:

```
h = max( default_contact_depth,
         process_resolution + r_end · sin(theta),      full-cross-section engagement
         BALL_SEAT_FACTOR · r_tip )                    ball-tipped ends only
```

- *Slicer resolution.* A fused region thinner than one layer is not resolvable by the
  printer, so the layer height sets a minimum.
- *Full-cross-section engagement.* An arm meeting the surface at incidence `theta` has its
  end disc tilted; only `h ≥ r_end · sin(theta)` engages the whole end cross-section rather
  than smearing a partial bond.
- *Ball-tip cap geometry.* The plane at depth `h` cuts an emitted tip ball in a disc
  `a² = 2·f·r_tip·h − h²`. Requiring that cut not to be the narrowest section (`a ≥ r_tip`)
  gives `BALL_SEAT_FACTOR = f − sqrt(f² − 1)`, derived in code from the registered ball
  factor, never hard-coded.

**One pass, no iteration.** Seating raises both `|cos(theta)|` and length, so a depth computed
at the unseated pose is provably still valid at the seated one — that is what licenses a
single pass rather than an iterative re-seat.

**Emergence (PR-3).** A contact is admissible only if the exempt region (§6) does not consume
the whole member: `contact::emerges` requires `mask ≤ length − min_free_span`, so a mask that
would erase the member entirely is refused rather than silently admitting an untested arm as
`INFINITY`-clearance. There is no fixed incidence-angle rule — the effective limit falls out
of the radii, the seat depth, the standoff and the length, and differs for every candidate.

**Area sufficiency** is evaluated on the *minimal separating cut* — the ball's cut disc for a
ball-tipped end, the end radius otherwise — against the single registered `X_ALLOWED`. There is
exactly one area-ratio allowance in the engine and every check reads it, precisely to avoid a
second hand-typed copy silently going stale under a future recalibration.

**Attestation, not assumption.** Every tip that mints a model contact must be *proven*
attached across its required contact area, by witnessing rays that penetrate from the
contact. That inference is exact for locally flat or convex seats and a sampling bound
otherwise; deeply concave seats are unmeasured. A tip that engages nothing is quarantined
rather than shipped.

**Buried surface is refused (OD-9).** Real mini meshes are watertight and manifold yet
self-interpenetrating, so a support foot can land on a surface patch that lies *inside* the
printed solid. Such a foot passes naive verification trivially — its whole neighbourhood is
interior. An explicit exterior-ness screen catches that case, and refusals route to
quarantine rather than failing the run.

Fuller scar-pricing rules (patch area, scar length, anchor-area floors, the print-floor moot
band) are the binding spec, not restated here — see `docs/specs/embed-policy.md`.

## 10. Drainage and hollowing physics

Hollowing removes interior volume from Large-size-and-up minis to save resin; **the bulk-gate
refusal rate on smaller minis is expected behaviour, not a defect** (ruled by Craig,
2026-08-13) — a ~30 mm medium mini has too little interior to be worth removing, and hollowing
it would leave walls thin enough to cause support problems.

**The drainage doctrine (ruled by Craig, 2026-08-12).** Resin must be able to flow "down" out
of any cavity as the model lifts off the film, with an inlet above for replacement air:

- Interior connecting passages must be wide enough (the throat rule) **and** oriented so resin
  actually flows in the print orientation — drains sit at each cavity's own local low point,
  and any pocket that would pool with no downward path to a drain is a defect.
- Every admitted cavity needs both a downward outflow and an air inlet (a vent).
- A cavity may drain to the exterior **or** into another cavity that itself drains — drainage
  forms a chain that must terminate at the exterior — and drilling into a neighbouring cavity
  is preferred over scarring the exterior surface when both routes work.
- **Multi-cavity hollowing is allowed and encouraged**, verbatim: *"as long as each cavity is
  large enough and properly drainable."* Each candidate cavity is admitted independently against
  the existing size/savings machinery and its own drain/vent doctrine; a candidate failing
  either condition is left solid, never left as a trap.
- A narrow neck between cavities may be drilled out to full bore, admissible only behind (1) the
  realized wall at the widened neck staying at or above the print-strength wall spec, and (2)
  the model and its supports still holding up through printing and peel. A separate,
  **provisional and unregistered** cured-strength-only wall minimum for necks fully carried by
  external struts (~1.5 mm) needs coupon-plate provenance before it may be registered or relied
  on.

**Grid-phase sensitivity was closed without measurement (ruled by Craig, 2026-08-13).** Whether
a model's sub-voxel grid phase can shift a clearance reading enough to flip a hollow/no-hollow
verdict at the margins was ruled immaterial: even where it matters, the resin-savings stake is
too small next to the cost of chasing it. What was measured before the ruling closed the
question: the corpus's closest approach to the bulk-gate minimum was ~1.1 mm, roughly 5× the
largest observed grid-phase swing on this corpus — too far for phase alone to flip a verdict
here. This does not license loosening any gate, cap, floor or threshold to raise the hollow
rate; it only explains why chasing sub-voxel phase precision further was ruled not worth it.

Build status and open drainage work (per-cavity vent counts, pooling detection, cavity-to-cavity
bores, orientation-aware drain planning) belong to `docs/status.md`, not here.

## 11. Where the model stops being valid

Stated plainly, because the numbers are quotable and the boundaries are not obvious:

- **One printer, one resin class.** Every measured constant comes from plates printed on a
  single tilt-release machine (Saturn 4 Ultra) in a specific resin. Another machine or another
  resin needs its own plates.
- **Coupon to interface.** The area-ratio boundary was measured on coupons whose arm *and* head
  were both support material. Applying it to a support-to-model interface crosses three
  boundaries: the interface is a virtual cut through one continuous cured solid rather than a
  machined section (defensible — in resin printing the union is monolithic, there is no bond
  line, so a minimal-cut criterion is the right generalization of "narrowest section"); the
  loaded material is model resin rather than support resin (same resin, same green state);
  and the coupon was loaded in tension while the interface sees mixed tension and shear at
  incidence. The factor of roughly two to three between `X = 12` and the measured boundary is
  what absorbs that mode difference. No tension/shear interaction criterion is attempted,
  because the ratio is unmeasured for green resin.
- **Concave seats are a sampling bound**, not a proof.
- **`FINAL_CLEARANCE_GAP_MM` is field data, not a derived physical bound.** It is registered as
  an external requirement precisely so a future coupon measurement of gap-feature preservation
  can supersede it without colliding with anything (§5).
- **The footprint search is closed against current inputs, not against authored labels.** §6's
  conclusion holds for an unlabelled mesh; it says nothing about a corpus that carried authored
  cavity/receiver identity, which was considered and declined as out of reach today.
- **Most constants are not measured.** The registry
  (`docs/engineering-journal/m0-constants-source-ledger.json`) currently holds **306** entries:
  **22 measured**, **11 derived** from measured ones, and **273 placeholders**. That ratio is
  the point rather than an embarrassment — see below.

## 12. Every constant carries its provenance

Each registered value is typed as one of three things, in the type system, not in a comment:

- **measured** — carries the record it came from, and its date.
- **derived** — carries its chain of inputs and the exact relation.
- **placeholder** — explicitly *not* safe to accept a print on.

An unmeasured number is therefore visibly unmeasured, and a placeholder can never be quoted
as a safety claim. The measured/derived distinction is also what makes recalibration
tractable: move a measured value and everything derived from it moves with it, mechanically.

Promotion is deliberately slow. A tested calibration cell does not become a measured engine
constant off one plate — the promotion gate requires several independently prepared plates
before an exact tested cell is promoted. Duplicate thresholds are treated as a divergence
hazard and deleted rather than kept in sync by hand.

## 13. Resources: counted, then charged, then spent

A real mini is two orders of magnitude larger than a test fixture, and an algorithm that is
accidentally quadratic will happily consume a machine. The engine's answer is not a timeout.

**Work is counted before it is charged, and charged before it is spent.** Each stage states
what a batch will cost, in explicit units, before it allocates anything. That statement is
checked against a cap. If it does not fit, the stage returns a **typed refusal naming the
resource, the requirement and the limit** — it does not silently reduce quality, drop
candidates, or approximate. A refusal is a result you can act on; a degraded answer is one
you cannot even detect. The machine this runs on has a hard practical ceiling of roughly
100 GB even with an uncapped run; exceeding it is a finding to record, not a limit to raise
casually.

Where a plan must be charged for more than it turns out to execute — because no plan can know
at plan time which branches a search will take — the charge and the execution are **severed
and both reported**, with the execution metered against the charge. Charging the admissible
worst case is honest; quietly executing more than was charged is not.

**A cap's job is to refuse work that is superlinear in model size, not to refuse large
models.** A flat constant conflates the two: it fires on honest linear work at 400k faces and
lets a genuine quadratic through at 24 faces. So the caps that govern per-face work take the
shape

```
cap = max(minimum, per_face × face_count)
```

with the coefficient derived from measurement by one stated rule rather than picked. `k·N` is
still a bound: it refuses anything worse than linear, which is exactly the runaway each cap
exists to catch.

When a cap fires, the remedy is to **make the bound honest or fix the algorithm** — not to
raise the cap. A cap raised to admit a quadratic has stopped being a cap.

## 14. Determinism and numeric contracts

The artifact is a claim about physics, so it has to be reproducible before it is anything
else. Same input, same bytes — every time, at any thread count.

- **f64 through the whole geometry path**, including intermediate buffers and fixtures.
  `f32` exists only at the output format's boundary, converted outward and never back inward.
  A float32 round-trip is not a rounding inconvenience: it fragments vertex adjacency, which
  flips topology flags, which breaks coverage.
- **Non-finite values are rejected at creation** rather than discovered by a serializer, and
  signed zero is preserved.
- **No float reduction across parallel work items, ever.** Float addition is not associative,
  so a parallel sum makes the answer depend on the thread count. The only sanctioned parallel
  pattern is index-parallel map-collect, where every output lands in the slot of its own
  index. Sums are collected per index and added in a fixed serial order.
- **Slicing quantizes exactly once.** Per-layer polygon union takes one explicit
  quantization to a 1e-8 mm grid, does exact integer arithmetic inside, and dequantizes once
  on the way out. The quantum coincides with the upstream stitch grid, so already-stitched
  vertices quantize losslessly. Adaptive scales chosen from a model's bounding box are
  forbidden here — the scale would change when the model moved, silently moving results.
- **A layer already in normal form passes through verbatim**, as its original f64 loops
  rather than requantized ones, so clean layers are byte-identical by construction and the
  equality check doubles as an idempotence property running in production.

Reproducibility is checked, not assumed: see the determinism check in
[`development.md`](development.md). Digest movements caused by a behavioural change are a
stop-and-report event with an attributed from→to record (OD-8), never a silently edited
expectation.
