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
costlier lean is admitted, and NEARBY WINS (Craig 2026-10-20): the cheapest candidate that clears
the final gap is taken, and the preferred margin only breaks near-ties. A candidate that clears
0.6 mm replaces it when it costs at most `preferred − final` (0.3 mm) more and stands at most that
much further out sideways; the margin never buys a far lean. The search region is round: no
candidate stands further sideways than the search radius. The zone exemption grows with the gap
asked, so a descent counts as clearing the preferred margin only if it also clears the final gap
at the final gap's own zones (a short descent can lie wholly inside its zones at 0.6 mm and still
have a testable stretch at 0.3 mm). Where no descent clears the final gap, the
seat may move to a cell whose reach disc covers the SAME demand
(`up_sweep::reseat_cells`); where none does, it moves off the wall (lane wall, 2026-10-24,
`SweepPolicy::off_wall_seats`) to the nearest cell whose straight drop clears the 0.6 mm margin at
the radius its demand needs and whose whole contact disc bears on the underside, giving up only the
rim of the reach it had (the cells in the last `d` of it, for a move of `d`: the cover loop seats them
like any demand; a cell is never left unrooted), or failing that to a cell that clears the final gap;
where none does, the seat is kept and the census names it (emit and warn). A straight drop that clears the
final gap only is asked the same margin, because the sizing grows a post with the load that comes to rest
on it (overhang table post 91: 0.318 mm clear at its 0.257 mm lower-bound radius, 0.067 mm at the
0.508 mm its branch made it); it keeps its seat when no cell clears the margin. A lean that would
take such a post off the wall is not the answer: the lean gate refuses a thin post's shake. Each shaft is excused only its OWN contact zones: the seat zone is where the capsule first
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

**Head clearance at placement.** A stamped member cell locates its axis, not its solid.
The head test keeps its existing raster refusals, then widens the raster search by the obstacle
radius and cell rounding. For the member ids found there, it measures the finite component-axis
distance minus both section radii against the final gap. During the up-sweep, each member reserves
its section under the sizing law for the load already committed to it: its own planned demand plus
the demand of children already landed or joined on it. A new member commit adds its demand along
its landing chain; a merged seat or footprint-held birth adds only its new demand cells to its host
and that host's chain. The totals update at
commit, not by rescanning members during head checks. A member without children is judged at its
planned section, not the registered ceiling. A headed post applies its planned full-lever bending
law to the committed-load section, retaining known sized sections and stored minima. Heads retain
their planned section law. A model foot uses the same committed load, its foot sizing law, and the
owner's profile factor at that neck size. The broad search reach alone retains the registered
maximum and the lean limit; it adds no fixed mm margin. Normal and tilted heads exempt only the
intended post host, using the same descent lookup.

A live member view takes constant time to construct; raster-hit ids directly select components.
It neither rebuilds all members per seat/grazer nor scans them per candidate. The graze pass runs
after the down-sweep and uses a fresh occupancy history, not the up-sweep's committed-load totals.
Its obstacle sections retain the already-sized radii, which include child load; it measures their
maximum once for the batch. Extra host load from a graze re-landing is later growth for the final
CL audit, not a new entry in this placement ledger. There is no model-mesh query per obstacle.
`PLINTH_HEAD_GROWTH_TRACE` records candidates refused by committed child load that their own-demand
planned sections would admit, on the same axis in the same run. This local count is not a second
placement with counterfactual topology. Later growth beyond the load committed at an earlier head
check is caught by the written-file clearance check (CL); final lowered surfaces still require
that audit. Normal,
tilted and converted graze heads also share the sharp `model_blocks` test along their own cells
below the seat. A separate thin model sliver is not exempt merely because it is near the seat
plane. A refused head leaves the seat to the existing next candidate; it does not discard load.

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

**One arm per seat (2026-10-21).** The stranded-seat rule (a pivot inside a drain's keep-out, seated from a
legal host outside it) picks the host by the demand in the host's straight-line disc, but the host's reach
field can be shorter. The pivot then stays unrooted, the next stranded pivot picks the same host (the key
prefers a cell that already holds an arm), and the host was seated again, identically: Giant_Bat drew 282 arms
on one cell, each landing on the one before (672 copy arms in all; the copies printed as one tip, and each load
was drawn once, not summed). `up_sweep::commit_seat` now refuses a seat that roots nothing new in a cell and
layer where an arm already seats, counts it (`same_seat_refused`, in the `seats asked for where an arm already
seats` log line) and leaves the pivot to the cover loop, which steps past it as past any pivot no seat can root.
A seat in such a cell that does root new demand is drawn (`same_seat_new_demand`; measured 0 on Giant_Bat and 3 on the lying treant, 1,836 and 1,590 copies refused).
`tests/it/same_seat_copies.rs` grades it on a keep-out island whose host reach is cut by a ring with no cover.

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

A twin merged into a HEADED arm is carried by that arm's frozen head, planned at placement for the
host's own seat: the twin's load goes out along the head's lever `|DK|`, and the head law
(`r^3 = a(r + 4L)`, `a = r_t^2/c`) sizes the joint, and the post under it, for that cantilever. On
`make_laid_tube` four such merges grew 3 mm columns from r 0.23 to r 0.82-1.0 (+63 mm3); refusing
headed hosts restores them but brings Deinonychus defects back (2 thin unbraced rods, 2 leaning
posts; measured 2026-10-12), so it is counted
(`twin_seats_headed_host`) and left open (`q12/REGRESSIONS.md`).

**A lean the slenderness law cannot hold is taken only where its tip seats and the standing arm's
does not (quality lane Q1, refined 2026-10-12 and 2026-10-14).** The lip-seat rule leans a whole arm
along one axis. Where its run is longer than `S` times the thickest radius at which it keeps the final
gap (the drawn radius grown by the spare gap measured for it, up to the registered maximum), the lean
is refused and the arm keeps its seat with its seat-bite warning (Deinonychus arms 215 and 220). The
refusal trades the lean for the arm as it stands, so that arm must be there to keep: standing on its
ground, its solid not entering the model. Then the lean replaces it only if the lean's tip really
seats on its face and the standing arm's tip does not. Seating is the real contact (the model surface
inside the drawn ball, measured exactly) reaching the planned contact to within one pixel of area, the
ball growth's own test, read for both tips in one batched call per round. The shape of the standing
arm decides nothing: a shape test ("a one-layer stub on the model is no support") took Bat_Cat_Owl
arm 20's 40 degree lean, a 0.10 mm rod 3.4 times over `S * r`, in place of a stub on the claw's
0.018 mm edge; both tips had 0.0000 mm2 of contact, so the lean added a rod and no seat (joint round 5,
`CLAW-POST.md`). The slenderness test itself is arithmetic on the clearance the rule already reads for
every proposal in one batched kernel call.

**One contact, one arm, at a first layer over nothing (joint round 4).** A birth the cover bridged to
rooted material is given an arm only if no arm of its layer stands in it or holds it. An arm holds a
birth when every pixel of the birth lies inside the arm's planned tip ball (the contact ball of its
smallest tip): one contact touches them all. Bat_Cat_Owl seated four arms 0.05-0.1 mm apart on one
layer, and the third hung as a long thin rod. The holder covers the held birth's cells, which is how
the reach field and the removal replay read the birth as rooted.

**Unsupported islands get no tip when they join supported model at once (Craig, 2026-10-20).** A birth
(ChituBox's "unsupported island") gets no tip and no warning of its own when, within
`TINY_ISLAND_JOIN_LAYERS` = 3 layers (0.15 mm), what rests on it joins model material that is
supported by OTHER means, it spans at most `TINY_ISLAND_MAX_EXTENT_PX` = 25 printer pixels in X and in Y on
every layer before it joins, and nothing still rests only on it 3 layers up. A small birth that never joins
the model and leaves nothing afloat within those layers is a loose FLECK (Craig, 2026-10-21: "I think we
don't need to support these loose flecks at all. And chitubox will probably eliminate them when it
slices"): no tip, no other tip kept for it, counted in the run's line as information. It prints flattened onto the
layer that joins it: "the detail lost will be vastly less than the scarring that would happen if we did
add supports there". Both numbers are the owner's field judgement, not derived. The island supports
NOTHING: "those unsupported island DO NOT THEMSELVES PROVIDE ANY SUPPORT ... otherwise thin fingers will
never get supported properly". So an island, and every piece that rests only on it, is not grounded when
another birth's joining is judged (two islands never join through each other, and a finger that starts as
a run of islands finds no support through them), and every working cell the island's own outline adds to
the cover (by the cover mask's own any-overlap rule, not only the cells its pixel centres fall in) and
no held material supplies is taken out of both layer masks, cover and carry, in the layer cache itself:
reach does not cross or climb from them, the down-sweep never offers them as carriers, and no landing reads
them as model. Every edit of a finished plan sizes its field on the same cache, and the footprint
moves' and second contact's ground checks read the same island-filtered masks (an island is no ground
under material whose contact moves). The slicer
drops loops under 0.001 mm2, which hides a spike's thin end and slivers beside an island, so a birth the
slices would leave is walked again on the mesh's own unfiltered section at the printer's pixel. There the
chain's real start is found conservatively: unsupported material under it within the rule's pixel plus
one of the printer's phase, including slivers no pixel centre shows (every pixel the outline touches),
makes the chain start earlier, and its 3 layers are counted from there (a reading that only ever moves the
start; it never counts as support). A wider span, a later join or material left afloat keeps its tip. Its
size is compared
with one pixel of lattice phase added, because the printer re-slices the file on its own pixel lattice,
and together with every afloat piece within one empty pixel of it; births within one empty pixel of each
other cure as one piece, so they are judged as one start with one verdict (join, afloat and size all read
the cluster, on every layer). No tip is kept for fragments a shaft might have held. "Held" has one
meaning for the engine and the acceptance check (Craig, 2026-10-21): only a tip contact (a tip ball or a
head's contact) holds a piece directly; a shaft never touches the model and holds nothing; material that
touches held material is held only within the planner's own overhang reach of a held pixel, `κ·h`
(`SweepPolicy::lamina_reach_mm`, 2.13 mm, the one formula every reader calls), measured through the
layer's own pixels. It is read per pixel, from the JOINING pixels: a piece resting on the chain is held
only when one of its pixels within one pixel of the chain lies within that reach of a pixel held by the
layer under it (model on held model, or a birth that keeps its tip), through pixels that are not the
chain's; the chain's own pixels never hold. So a birth that joins the far end of a ledge more than the
reach from where the ledge stands is not held, however the ledge's piece is grounded elsewhere, and
material grounded only through filaments longer than the reach (the treant's lace-like bark) holds
nothing. The same reach applies where the chain really starts on the mesh's own section: a held piece's
pixels beside the chain beyond the reach of its held pixels are the chain's material, so the 3 layers
count from them; and the mesh's reading of the birth's own layer holds it only within the reach. A loose
speck on the model's topmost layer never joins: it is a fleck. A
small birth judged
to NEED support gets the ordinary seating, and after every pass that moves or removes tips (the twin
merge, the seat moves, the footprint slides, the removal pass, the lip-seat drop, the hairs) it is
checked on the field as drawn, by the same meaning of held: a drawn tip contact holds one of its chain's
pixels, or model within the reach of one through that layer's own pixels. Only a birth left unheld gets a
FALLBACK PIN (Craig, 2026-10-21; the tip-cover ruling of 2026-10-16, restore the support a move took): one
thin tip placed and checked as a hair tip, sized by the area law at the area its chain puts on one layer
(never one working cell's), every one of the birth's pixels offered as its contact in one batch, nearest
the centroid first (a pixel that is model, never a gap between pixels), its contact's centre on the model;
where no pixel of the first layer gives one, the next layer of its chain, within the 3-layer window; where
no layer gives one at the 0.6 mm placement margin, every layer is tried again held to the 0.3 mm final
gap (Craig's two tiers: the margin is a preference, the final gap the hard limit). A pin candidate whose
run would shake its printing top over one pixel (the run-alone stage shake, `run_shake.rs`) is not offered,
and a pin whose drawn ball touches no pixel of the birth is taken back. A birth no pin serves is warned
about with its layer and place. No birth is placed below the first layer the slicer shows. The orientation search's
prefilter places no pin: at its 0.5 mm raster a pin on a one-pixel birth cannot be placed, and each try
costs a hair-tip screening; the finalists' production measurement places them. The local tip-cover
check and the hair rule's cover pieces leave the island
cells out exactly as the layer cache does. The decision is taken once at the printer's pixel (`births.rs`), and
the up-sweep and every replay read it through one call and one cache key. The run prints one line: "N unsupported islands left without a tip (each joins supported model
within 3 layers and stays at most 25 px across before it does ...)".

This reading is conservative and that is a known, open trade. A lean leaving its seat face reads its
clearance at the gap itself (its seat zone ends where the solid first stands the gap clear of the seat
plane), so it reads "cannot grow", although the thicken rung's taper (tip kept, root raised away from
the face) would hold many such leans clear of the model. Admitting those (tried 2026-10-12, `ff2e64c`)
took ten more leans on Deinonychus than Q12 did, two of them the long slanted runs Q12 removed (8.4 mm
at 48 deg and 12.7 mm at 30 deg, each about 6 mm sideways, both with model below), and changed
`make_brace_pair` only at arm 3 (kept, its seat slid along its face; both masts placed either way).
Whether a lean the taper holds should replace a standing support is the owner's call
(`q12/REGRESSIONS.md`).

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

**One drawing for every lip-seat judgment** (lane val S1a, 2026-10-24, `plinth-sweep/src/lip_draw.rs`).
The trial of a re-landing equals the adapter's complete
drawing of the same staged, pre-sizing field, with explicitly prepared seat facts and the same
held-profile overrides. It does not predict the finished file: the seat stage, the footprint slides, the
hairs and the lowering are checked at their own boundaries, and the final audit stays the backstop.
The survey, the live tips-inside field, the final gap and tip tests of each re-landing and the seat bite
all read arms drawn by that one function: junction following (a leaning plate post's boss line, a seated
ball's displaced shaft), the footprint normal of each edited tip measured from the mesh (the recipe of
the seat stage's first measure: `drawn_ends` + `sweep_arm`, before the following), and the profile the
pass holds for each arm it re-landed (its top-aligned proposal profile; the adapter raises it afresh at
every junction, so a later edit that raises an earlier re-landed arm is in the drawing).
`thickened_mm` written into a staged strut is not drawn by the adapter, so a trial is not the committed
geometry of the nearby landing's thicker root. The seat bite of a drawn arm is re-read in the full-field
lander context (it depends on the shallowest member landed on its host, so a host whose lander height
changed is read again). The clearance re-route's mover is no longer silently exempt from the shake gate
and the too-slender test: both are read, and a re-route that fails one is still taken, under the NAMED
opt-out `ClearanceTriggerNoLeanGate`; the failed judgments it takes are counted in the run log. It is
not an owner ruling. A lean taken because the standing arm is not there to keep (`slender_taken`)
keeps its cause (the shake gate or the slenderness law) and excuses the mover's own lean only. With
`PLINTH_SWEEP_DIFFERENTIAL` set (and in debug builds) every accepted re-landing is compared bit for
bit with an independent drawing of the raw field, normals measured again from the mesh.

**One validator over the drawn field** (lane val S1b, 2026-10-25, `plinth-sweep/src/validator.rs`).
`validate(env, baseline, trial, scope, opt_outs)` reads a trial field by every rule and answers one
outcome per rule: Passed, Failed, PartiallyMeasured (the subjects it could not measure, or a
structural invariant only), OptedOut, or NoReader. Only Passed is a proof; a missing reading is never
a pass. A finding's identity is `second_contact::Key`; its measurement stays under it (the model
gap's clearance, bound and witness from the finished-field census; a tip's depth in its holder; a
lean's pixels; a run's interval endpoints and sway direction; a bite's depth), and a violation is a
new subject OR a worse measurement. A fusion is judged by its patches, pair by pair: a pair is worse
only if it has a patch (centre and radius on the model) that overlaps none of that pair's baseline
patches. A patch's size is not compared: a tip ball moves a few hundredths of a millimetre with every
candidate's axis. A focused verdict, taken into the
field, replaces the baseline entries of the members it read and leaves the others alone. A rule is
skipped only by a NAMED opt-out (a rule, a scope, a reason), and every use is counted in the run log
(`lip seat validator` lines). Lip-seat's final acceptance runs on it: tips inside, the model gap, the
seat bite and the new **model-end fusion** rule. Fusion reads the final audit's own real-contact reader
(moved to `plinth_emit::real_contact`, not copied) on the solids the WRITER emits for the drawn
arms: each arm lowered by the production per-arm lowering in the lander context of its whole field
(`plinth_support::lower_arm_in_field`; a lander's height ends its host's head cone, so a host whose
arm is unchanged but whose lander changed is a subject too), then cut and trimmed by the writer's own
code (`plinth_emit::trimmed_support_solids`: outward winding, the seat trim against the model, the piece
that holds the shaft end, records wholly inside the model dropped). The model is registered natively
once per sweep, and pieces that all stand inside one 10 mm cell are cut against the model cropped to
that cell, which gives the same triangles as the whole model (the order of triangles can differ).
Two members that newly share a model patch are refused, except a pair the plan joins in the trial (a
lander and its host, a brace and the member it is seated on); that exemption is applied to the
violation, not to the baseline (a lander taken off its host leaves the patch it always made), and its
uses are counted in the run log. Only the cluster of the edited members (the pieces within the audit's
link distance of theirs, to a fixed point) is trimmed and given to the patch reader, and a trial whose
subjects draw the same model end, bit for bit, in the same lander context, reads no patch at all. A
member whose solids could not be made stays unknown: only a complete reading makes an entry known.
Lean gate (Arm and Tie only), slenderness (planned
intervals on the drawn radii) and host stability (the planned attachment graph) have readers, used by the
shadow run and not yet by lip-seat, which takes them as named opt-outs. Demand coverage and contact size
are structural invariants only. The lean law has no reader; the area law is judged on the sized field; the
drain keep-out stays deferred (the hair bore helper is not a general reader). With
`PLINTH_VALIDATOR_SHADOW=<dir>` (default phase `FinalSweep`, `PLINTH_VALIDATOR_SHADOW_PHASE` names
others, comma-separated) the whole field is validated with no opt-outs at every pass boundary of the
sweep and the seat stage, and logged to `<dir>/shadow-<pid>-<n>.tsv` (violations per rule, introduced
and cleared since the last boundary, the pass that last moved each violator); boundaries before the
seat stage are marked prospective (the seat facts are measured on a copy), and the seat stage's
boundaries read the carried state as it is. It never changes the field.

**A run keeps one plan and may sweep several.** The placement plan reused from the orientation
search's production measurement (one `Production` sweep per finalist pose) is the first plan; the
second contact's re-plan (`FinalSweep`) is rejected when the first plan is kept, and its field never
reaches the file. So a shadow of the default phase can describe a plan that was never written (lane
fp, 2026-10-25: the standing violations of the 566-member re-plan were read as the written file's).
Every shadow file therefore starts with an `I` row (sweep id, pid, perf phase, members at its first
boundary), and the pipeline writes the member count of the plan it keeps to `<dir>/selected.tsv`
when it chooses. Trace both with `PLINTH_VALIDATOR_SHADOW_PHASE=Production,FinalSweep`, and take an
attribution only from the file whose last member count is the selected one. The `I` row also carries
the pose (the bounding box of the swept mesh, which differs between build orientations). At the end
of each traced sweep the file gets an `E` row (the digest of the finished plan) and an `M` row per
member (member number, cell, top layer, kind, whether the down-sweep's load rule inserted it, the
final tip radius, and the final contact disc's radius and centre by the production disc reader);
`selected.tsv` repeats the digest of the plan the run writes, so a file is matched to the written
plan exactly, and a member number of the written file's audit is read as a seat key. The `V` rows end
with the seat keys (`cell:top layer:kind`) of their members.

**The up-sweep census** (`PLINTH_UPSWEEP_CENSUS=<dir>`, lane up1; phases named by
`PLINTH_UPSWEEP_CENSUS_PHASE`, default `Production,FinalSweep`). It changes nothing: unset, no probe
runs. Set, it records for every seat the up-sweep commits: the seat bite and the model contact the
chosen descent is forecast to make (a provisional member with the planned tip radius, read in one
batch by the production seat-face, footprint-normal, arm, bite and contact-disc code; no bite
formula of its own), the alternatives in the order the placement tries them (the straight drop; the
lean candidates in price order, at most 16 that clear, reading at most 1500, a candidate within 3
pixels of one already read left unread and counted; the head, planned but not read; every cell the
placement's own seat move may reach, nearest first, rooting the same demand, read at a fixed stride
so that at most 400 are rooted, at most 12 that clear recorded), the forecast
bite of each, whether the forecast contact is separate from the contacts of the seats committed
before it (a distance over the radii plus the link distance proves it), and the demand area and the
face tilt. The forecast is neither exact nor conservative (the tip grows with the load that comes to
rest on it, and the normal drifts after commit), so it is always read against the final file.

**One contact for two** (`SweepPolicy::fusion_merge`, default on; `up_sweep/fusion.rs`; Craig
2026-10-25: any merged contact patch is a defect, a contact is never wider than 0.8 mm). At the
commit of a seat, its model contact is forecast at the GROWN size by the production growth law
(`contact_growth::forecast_grown_ball`: the ungrown ball measured, grown toward the load its covered
cells give under the ceilings and the feature chord). The forecast arm is then drawn and read by the
audit's `real_contact` patch reader, including shaft contact and thin curved strips. Its enclosing
radius is the maximum distance of the measured contact points from the returned centre; multiple
patches are enclosed by the sum of the centre offset and each patch's radius. All physical centres,
including model feet, are in mesh space; only the axis-distance screen uses plate-relative height.
A distance between centres over the enclosing radii plus the link distance proves separation.
Otherwise the real patch reader decides whether the two forecast tips fuse. The nearest fused tip
may carry both demands: (a) after withdrawing the prospective seat's root, the production reach
field rooted at the carrier's REAL column must root every cell the new seat covered, (b) the enclosing ball must prove the combined contact no wider than 0.8 mm, or the
real patch reader must confirm that cap, (c) it must stay separate from every other contact (real
patch confirmation when the enclosing balls cannot prove it; unread contacts refuse the merge). The tip is then raised to
this layer through the twin mechanism (`raise_host_seat`), so the down-sweep sizes it for both loads,
and the new seat is not created. Otherwise both seats stay and a counted, named
`NoCleanPlacement{Fusion, reason}` says why (model foot, not a plain arm, column not cured, demand not
coverable, over the cap, a third contact). Every check that can fail runs before the column history or
the host changes. The reach-field trial is journaled; refusal restores the new seat's root and demand
exactly. Acceptance keeps ONLY the carrier's real root. A removed seat cannot leave zero potential
for upper layers to inherit, even when this layer's demand is within a Euclidean reach estimate. The load that lands on the arm from above after the seat is not known at the seat;
the audit's contact-size count says whether it matters. The model validator's planned-join exemption is
narrowed by contact location: a planned host and lander whose tips both stand in the fused patch are a
fusion all the same (`model_end_fusion::both_contacts_in_patch`). A carrier whose real column is
inside cured model can have no reach there: that is a refusal, not a reason to preserve the removed
seat's root. A production-path witness advances to an upper layer and checks that the required new
arm is actually drawn; preserving the removed root makes that witness fail.

Registration batches point projections of the seats (no Boolean) and caches each stable seat's
projection. Each down-facing seat reserves a CAPSULE: the vertical segment from its seat to
`seat - departure * Z`, where `departure = (2*b_max + r + layer_height)/abs(normal_z)`.
Its radius is `max(3*b_max, r)`, plus the projected face point's offset from the seat axis and
`LINK_MM`. Here `b_max` includes both the growth ceiling and the ungrown profile ball, even
when the load already makes that ball larger than the growth ceiling. A ceiling ball alone
is not a bound: on a near-vertical face, the shaft's contact stripe can extend arbitrarily
far below the seat. The capsule encloses the whole forecast ball and departure shaft.
A 3D index uses enclosing balls of these capsules for its largest query radius, then tests the
actual capsules before forecasting patches. It visits occupied buckets, not a cubic lattice.
Placement does not yet have the finished footprint normal: forecasts use the batched surface
projection, not a claimed committed seat normal. Readable neighbours then use measured patch
bounds. With no down-facing projection, a CHAIN of capsules follows the actual committed
seat-to-base route and path points at their layer heights. Each span reserves the maximum of
`b_max`, the registered area-law section ceiling, committed minimum/thickening, and the head's
final section law at that ceiling (the maximum of its carried law and the registered lean
envelope, including larger certified knots). Add the displacement from the chain's seat to the
writer's drawn start (ball centre, or its face-plane projection for a head), bounded by the
carried face offset plus `2*b_max`, and `LINK_MM`. This does NOT bound the seated pole D's
travel: on grazing seats D can travel farther, but the shaft never starts there. The centre
stays within `sqrt(2)*b_max`; growth keeps the deepest normal pole. A tilted head is not grazing
(the head's `skims` check excludes it), which also bounds its projected start. A frustum lies in its
endpoint segment swept by its largest section; endpoint
displacements interpolate and cannot exceed this bound. The local ball keeps its `3*b_max`
bound. Arm length is never used as a sphere radius. A general merge in that unread region
refuses with `UNREAD_NEIGHBOUR`, never a claim of separation.
Model feet retain their registered foot-factor/rootward-growth bounds, including ball seating.

The query is repeated at the combined size and on every later twin raise of a fusion carrier.
A later raise of an ordinary neighbour first compares its projected capsule to carrier capsules,
without a Boolean. Only a nearby carrier requires a forecast. A same-column twin raises its host
unconditionally: its overlapping ball makes a separate arm fuse with the host, and the separate
arm also remains exposed to a nearby carrier. The forecast only warns, with counted
`TWIN_RAISED_CAP_FORECAST_OVER`, `TWIN_RAISED_FUSION_FORECAST` and `TWIN_UNREAD_RAISED` events.
Unread warnings count a physical `(host, raise layer)` once, not each demand packet. Missing
projections use the committed route plus its raised extension for the spatial test, rather than
making every carrier nearby. A model-foot bound intersection is `FOOT_BOUND_UNPROVEN`, not an
unread forecast; the general merge still refuses it. General merges never select a same-column
carrier, and still refuse an over-cap or fused contact. Every refusal reason is counted in the
`NoCleanPlacement Fusion` log; refused merges keep their separate arm.
This preserves the twin rule's first-seat guard against chaining raises. `TWIN_GUARD_SEPARATE`
counts only committed separate same-column arms whose new ball reaches a host's current ball,
but whose seat lies outside that host's first-seat guard. It is a twin-rule diagnostic, not a general
merge refusal, and does not change decisions. The committed twin reuses
its forecast and rebuilds an unread route bound from the raised arm and its new load. Stable seat/load identities cache the
forecast arm and its native solid. Only the seat-local ball and departure taper are forecast,
never a provisional vertical shaft through unrelated model geometry. The selected plan carries
stable `(cell, layer)` carrier seats and forecast patch diameters into the final audit, not member
ids that later removals renumber. After writing, `[merge-audit]` resolves the current ids and reads
every TIP-class patch containing any carrier record, including a shaft fused to another tip.
Foot-only patches are excluded. Members sharing a seat are reported separately, not aggregated.
It reports the widest real TIP patch, the carrier's own-tip patch diameter, and FU participation
from any record of that member (not only the first). The forecast is the current read after the
last raise; an unread later forecast replaces the old value with `UNMEASURED`. A missing seat or
tip patch is `UNMEASURED`, never zero.

**The second-contact choice is a Pareto test** (`SweepPolicy::choice_pareto`, default on;
`plan_choice.rs`). The re-plan is kept only if it is less short and has no more unbraced runs (as
before) AND no worse than the first plan on every rule the shared validator reads COMPLETELY on both drawn plans
(whole field, no baseline, no opt-out, the arms as the lowering gets them; each plan read once, and
only when the old rule would keep the re-plan). A rule that is partial, has unmeasured subjects, is opted out or has no reader on EITHER plan is
excluded on BOTH and named with its side and reason, never counted as zero, except that a partial
re-plan count is a lower bound: when it already exceeds the first plan's complete count for that
rule/class it vetoes the re-plan. A lower bound at or below the complete count proves nothing.
One proven worse rule keeps the first plan, with no ranking between rules. Both readings and the
reason are logged on the `[choice]` line. Before this the choice read nothing about fusion, the seat
cut or the model gap: on Deinonychus one unbraced run decided between a first plan with 23 fusions and
a re-plan with about 100.

**The nearby landing, before the first ladder and again after the clearance re-route** (lane nb,
2026-10-22; the first run is there because the up-sweep, a merge or the lip-seat rule may have
refused a lean, and the ladder would brace that run before any landing was tried). Owner, on the r9 files:
a long thin support beside a thicker braced post "could land there, or be braced to it, but
it's just....not"; a diagonal "passing RIGHT BY some other supports". For a run over its slenderness
limit (a long lean is one) the order is land on a nearby post, then brace, then warn. Every arm with
such a run is offered the landings on another ARM that cost less than the descent it stands on
(the lean law's weighted length: the cost model's own reach, no constant), at the leanest drop
whose run holds up while it prints (the run-alone shake of one printing top, no more than one
pixel, or than the run the arm stands on moves; read at the taper the move commits, the tip
section at the top and the larger of it and `l / S` at the foot, which is written to the member's
minimum profile), and every post the arm's own chord meets. A candidate then passes what the
re-route's candidates pass (path test, the final gap outright, the lowering's measurement, the
plate rule, the sized-field put-back). A landing is on a member, never a new plate root, so the
plate-packing model-end zones are untouched. When it moved anything, the ties run again (the
ladder, then the model ties), then the census warns. It cannot move an arm whose seat face
leans away from the post (the approach would cut the shaft at the tip), a thin tip whose lean
cannot print, a shaft that stands inside the final gap by a hair so that no other line clears it,
or an arm a cross is tied to. `PLINTH_NEARBY_TRACE=<dir>` writes the reason for each.

**The lean gate** (lane nb, 2026-10-22). Owner: "that thin unsupported diagonal should never be
possible and should be detected and failed." One definition (`run_shake::admits`): the straight run a
pass makes or moves may shake its printing top no more than one pixel, or no more than the run the
member stands on now (a move may leave a run no worse than it found it, never worse, and never makes a
failing lean of a run that held). The passes that call it: the up-sweep's descent, the lip-seat rule
(through its keep-or-take logic: the arm is kept where it stands if it is a support; a lean is taken
only where it is not), the clearance re-route, the nearby landing, the slenderness merges and the
footprint's seat moves. The order after a refusal is: land on a nearby post, brace or tie (the ladder),
a straight drop where it clears, and only then the arm stays and is warned. The finished field is read
once more (`run_shake::failing_leans`): every arm whose straight run, as the file draws it (a headed
arm by its post), still moves its printing top over a pixel is named in a WARNING with the pass that
last moved it, worst first (`failing_lean:` lines), so none is silent. The census runs at the END
of the finish, after every pass that changes geometry (hair tips included), and reads the lowering's
own arms: the shaft line the file draws and the radius of the profile at each end, not the plan's
cells. The cover hair tips (the repair of a stranded spot) go through the gate too, both the
first-choice landing and the steeper re-plan. The census reads the arms AND the ties to the model
(`failing_lean: tie ...`), lists every failing lean (no cap), and reads a shaft that rises from its
seat to a foot above it (a model-footed arm) from its lower end up, with the radii turned over
(`(rises)`): such a run prints from the seat, and the lean tool names it. A drawn profile is read as
the piecewise run it is (`RunShake::profile_px`: the worst print stage over the profile, not the two
end radii: a long constant middle with a thin tip taper peaks before the tip prints), by the census,
by the far-face judgement and by the standing run a move is compared with. After the final head
re-size, every brace joint is widened again to its brace where a later pass left the post thinner
(`widen_posts_under_braces`). Cross braces and masts are
not read (both ends stand on posts; the construction-stage check judges them). The passes that do NOT call the gate yet are the head
joins, the graze conversion, the model-foot lowering and the stranded-branch re-landing; the census
names what they leave.

**Braces follow the physics, not a blanket rule** (Owner, 2026-10-22: "The rules should be based on
physics - if bracing is going to cause physical problems then it shouldn't be allowed, or other
adjustments need to be made to make it physics-compliant. But arbitrary rules like 'just never do
something' seem overly strict"). This replaces rule R2 of 2026-10-03 ("a brace is never thicker than
the thinner post it joins") for the crosses. A brace's radius follows its own length by the S law.
Where a post is narrower than the brace at a joint, the post is widened over the band and under it
along a ramp at the permitted underside slope (the lean law's step per layer), to the brace's
radius, the resin charged to the band (`joint_raise_plan`, for every post now, not only a post that
stands on the model). So no joint draws a shoulder, and no brace is drawn fatter than its post at
its joint. The brace's reach is read at the shaft radius where it joins, never the contact tip. A
band is refused only when the physics fails (the radius would pass the registered maximum, the ramp
does not fit above the foot, the raised solid would enter the model's final gap, or the post ends at
the band's top). The knob `slenderness_brace_cap` turns the whole check off. The masts and the
ties to the model follow the same rule: a mast cell whose diagonal needs more than the arm's radius
at the tie is no longer refused (the cross widens the arm there), a tie thicker than its post
widens the post at the joint when the tie set is built, and only a radius past the registered
maximum, or a post that cannot be widened, refuses. A mast is still never thicker than the arm it
braces (a guard against a second, stiffer post, not a joint).

**Every cross is needed on the finished ladder** (lane nb, review 2026-10-22). Each tie pass judges
its own crosses at its own end. A later rung can leave an earlier cross holding nothing, so one more
prune runs after the last tie pass (`slenderness::prune_redundant_crosses`): every cross on one pair of
posts is judged with the others of the pair, as the test "every cross is needed" grades it, and
crosses on a post a pass thickened stay (Owner ruling 2026-10-01).

**No tip or head inside another member's drawn solid** (`MODEL-FEET.md` F4, 2026-10-12). The
Deinonychus tip 145 had its 0.13 mm ball and its whole head inside column 142, and no rule measured
an arm's drawn solid against another arm's tip. `member_clearance::tips_inside_members` reads each
seated tip as the segment from its ball centre to where its post starts (the head's axis; the ball
centre alone for a straight tip) against every other member's drawn spans, each a frustum: the tip
is inside where the segment comes nearer a span's axis than the span's drawn radius there. A member
that lands or seats on the tip's own arm is joined to it by plan and is not read. The holding member,
when it is an arm, is sent to the clearance re-route (a tip never moves there; moving the holder off
it is the remedy), and the finished field's census warns on every tip still inside, with its host and
depth. The re-route never moves a headed arm or one that carries a headed member, so case B's
column (it carries the headed tip) stays and is warned.

**No move adds a member conflict** (lane gap, 2026-10-24). The final audit's CL line counts two
things: a member's drawn solid inside the 0.3 mm final gap of the model (outside its own zones),
and a tip inside another member. `HeadJoinContext::clearance_census` is the one function that reads
both (`within_gap` and `tips_inside`); the clearance re-route, the finished-field report and every
rule below judge by it, with no copy. Four passes created conflicts that nothing had checked, on
Deinonychus at `16c35ffc` (9 items): the footprint rule's slides re-tilt a shaft and its
followers (arms 6 and 44 into the gap, tips 20 and 60 into the arm they land beside); the
clearance re-route's trial read the mover against the model only (arm 103 buried arm 96's
head); and a hair was seated with its contact inside the arm it lands on (433 in 82). (Arms 6,
128 and 133 stand in the gap with no admissible cure and stay warnings; 128 and 133 are twin hosts,
locked.) Now:

* **The footprint rule's move check** reads the conflicts of the field with the round's slides
  written, against the field without them (`footprint::ConflictSource`,
  `head_join::ConflictReader`), on the members the slides displace only, drawn from the placed arms
  with each moved seat's re-measured normal. A conflict a slide adds is blamed on the slides its
  members inherit a displacement from, and the tip tries its next seat. (Remedy 3: a tip on its
  face already in a conflict is searched for a seat along the face where the conflict is gone,
  like a tip whose section is cut; it keeps its seat when there is none.)
* **A re-landing** (`lip_seat::apply`, every trigger) may not leave the mover or a follower
  holding the tip of a member it is not joined to that its solid did not already hold
  (`member_clearance::FieldTips::held_by`).
* **A hair tip** is refused where its contact ball is inside another member, or its solid holds
  another member's tip, where neither was before it (`HairRefusal::HairTipInside`). Its shaft
  touching a member is still welcome (ruling 7 above).
* **A tip counts as inside a member only deeper than half the printer's XY pixel pitch**
  (`member_clearance::tip_depth_tolerance_mm`, the registered printer's coarser pixel, 0.0095 mm on
  the Saturn 4 Ultra; Craig 2026-10-24: a depth the printer cannot resolve is not a physical
  conflict). It is the one tolerance of the tips-inside reading, so the audit, the re-route, the
  footprint rule and the hair tips agree. The twin lock (owner ruling 2026-10-10) is untouched.

The re-route says why each arm it sent stayed (`LipSeatOutcome::stayed`, printed on the CL
warnings).

**The graze conversion runs again at the final radius** (`MODEL-FEET.md` F3, 2026-10-12). The
conversion judges each straight tip on the field the lip seats left, at the radius it had then; the
head decision at placement assumed a vertical post at the planning radius (0.15 mm). The head joins,
ties, re-route and their sizings come after, and a tip they grow (case B: 0.15 mm planned, 0.671 mm
drawn, leaned 41 deg, 29 off its face normal) was never asked again. So the same conversion runs once
more after the clearance re-route, on the sized field, measuring each straight shaft as the lowering
draws it at its final radius and drawn angle; a grazer gets a head where a post from the joint lands
legally, and otherwise stays straight with its reason (`heads at the final radius:` line; the
finished field's graze warning names the rest). A tip the footprint rule slides afterwards is still
"made by a later slide".

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

### A tip stays on the overhang it was placed to hold (lane cr, 2026-10-22)

The lowering seats a tip on its carried face's own point where the raster seat stands on the model side
of that face's plane by more than the ball radius (round 6, the Deinonychus claw: a feature thinner than a
raster cell). The carried face is the nearest face the overhang law admits, at any distance. Where a seat
holds a thin tine or a steep wall, every near face is a wall, and the carried face is some flat underside
millimetres away. The old move carried the tip there and ran the shaft back to the base: a near-flat rod
across the gap, the seat left bare. The r9 upright Weeping Treant had 39 near-flat model-to-model rods of
1 mm or more (more than 57.5° from vertical, past the lean law; 19 of 5 mm or more), in fans of up to 14
from one point. After the fix 6 are left, 1.5 to 1.8 mm long, under the right foot (this rule leaves
them as they were; not diagnosed), and none is 5 mm or more.

`plinth_sweep::face_point` now judges every such move before the lowering draws it, on the shaft line the
file draws. The tip keeps its raster seat, and the far face is dropped from the member (warned, counted), if:

1. **The face is beyond the footprint rule's reach of the seat**: `2 · 2R + 2 · pitch`, the tip's ball
   diameter plus the carrier mask's two cells (`footprint`, "the one pruning bound"). Past it the face is
   another feature's, and the tip would leave the overhang it was placed for. This is the rule that takes
   out the crown rods.
2. **The move breaks a rule every seat move is held to**: the lean law, the member's tightest unbraced
   run, the drain keep-out (each "no worse than it was"), or the run's printing top would shake more than
   one pixel (`run_shake`) and more than the unmoved run.

No new number: both bounds are rules the engine already has. Built in `face_point.rs`
(`tests/it/far_face_rods.rs`: a pin bundle beside a flat tine, which on the old code drew five rods of 6 mm).

**Support ends in air are measured, never deleted** (`plinth_sweep::free_end`, `tests/it/free_ends.rs`). After
the lowering, the check reads the EMITTED segments, each at its local drawn radius (linear between its two end
radii). A member end touches if it is shared with another segment's end or a ball's centre, inside a ball,
within its own end radius plus the printer's one-layer-one-pixel allowance of another member's drawn surface
or of the model, or on the plate side (a root flare, a raft record, the plate). It counts and warns about every
end that touches nothing ("support end in air: N", with positions) and changes nothing: a deleted member could
strand one that rests on it. The first version read the host's widest radius along a planned axis and called
the upright treant's brace record 6627 (0.412 mm off its post) touching.

**Known, not fixed:** the upright treant's brace record 6627 (r9: 6685) has its seat end 0.41 mm off its post's
drawn solid, in air since r9. The measure above reports it. The junction hull pass (`lower/junction.rs`,
`hull_junctions`) joins only a member's LANDING end (`arms[i].landed`), so a brace's seat end gets no hull; why
the arms the lowering draws put the end where they do is not found
(`plinth-evidence/perf-2026-10-14/cr/RESULTS.md`, section 7).

### A tip ball is sized to the load and bounded by its shaft and its feature (round 5, 2026-10-14)

The emitter cuts a tip ball to the model mesh, so a tip's contact is the model surface inside its drawn
ball. The ball grows (`plinth_sweep::contact_growth`) only toward what the tip's load needs, and a grown ball
is never a lump on its shaft or wider than the feature it seats on (owner, Bat_Cat_Owl: the balls were "much
bigger than the support column or the feature that is being supported").

- **The target is the load, not the profile disc.** The need is the down-sweep's carried load at the
  contact over `X_ALLOWED`, `π r_load²`, before the radius minimum and clamps. A tip at the 0.05 mm minimum
  needs far less than its ball gives and is not grown. The growth aims at `min(need, planned disc)`, but the
  tip is graded and warned against the whole need: where a radius clamp or the seat geometry leaves the
  profile disc smaller than the load, the tip is reported short (`ShortReason::BeyondProfileDisc`), not
  sufficient.
- **A step must pay for itself.** A face that is flat at the kept seat depth `δ` gains `2 π δ` of area per
  mm of ball radius. A step that gains less only wraps the flanks (`ShortReason::FlankOnly`). On a convex
  underside every step gains less, so such a ball keeps the owner's profile size.
- **Three ceilings.** The ball-to-shaft ratio (the largest ball factor of the owner's profile table, 1.5,
  times the shaft radius at the seat); a contact disc no wider than the narrowest cured chord of the model
  section through the seat at the strike layer (`plinth_sweep::feature_chord`: the cured joint is only as
  strong as its narrowest section, and past it the break moves into the model); and the shaft's own carry
  (above). A tip a ceiling leaves short of its need is warned with the shortfall and the reason.
- **The owner's ball is the floor** (round 6, 2026-10-14). The ceilings limit GROWTH only; a ball is never
  drawn smaller than the owner's ChituBox ball for its neck (`f(r_n) r_n`, `f` 1.25-1.5 by tip size). Round 5
  drew a profile ball wider than its feature at the ceiling, floored at the shaft end, and 12 of 35
  Bat_Cat_Owl balls came out as wide as their own shaft (owner, picture 30: "rounded ends whose sphere
  diameter = the support diameter"). A profile ball past its feature or the ratio is counted and warned
  (`ContactOutcome::over_feature`); a contact that stays short of the need is warned against that need
  and the neck is never shrunk to the contact (the neck carries the demand; shrinking it would shrink
  the ball with it and the contact again).

### The member profile, head to foot (round 6, 2026-10-14)

A member reads, from the model down: contact ball, neck, a cone widening to the body, the body, and
(on a model foot) a cone narrowing to the foot's neck and ball. The owner's pictures 28, 30 and 31 are the
cases where it did not.

- **A ball is wider than its neck, and the shaft leaves it at the neck.** The first drawn radius is the
  neck's, at the ball centre (`plinth_support::lower::head_cone_from_the_neck`). A tip whose first span
  stands past the top of the owner's ball-to-neck table (1.5) times the neck is not carrying its own load
  there: it is a plateau (the joint-sized running maximum of a head the lowering cannot draw, 0.198 -> 0.233
  under a 0.067 mm ball, picture 31), and it is drawn as the owner's head cone, his `supporttiplength` at
  the neck's size (1.5 to 3.0 mm, `TipLengthProfiles`), from the neck to the section the shaft has that far
  down. **A landing inside the cone ends it**: the cone replaces the tip's own head minimum, never the
  section a landed member's load needs (`max(E, D)`). The shallowest member whose end stands in this
  shaft's solid (`shallowest_landers`: distance to the axis at most the two sections, no tolerance) ends
  the cone one layer under it, at the ledger's section there, and the ledger is drawn below unchanged.
  Any other first span that starts past the neck, and the post under a head leaving its joint
  sphere, widens to the ledger over one layer (a cone widening going down prints at any slope). A span
  that already starts at the neck is untouched.
- **A model foot is the owner's tail contact.** The foot's contact radius is the largest of the area
  law's, the neck under the ChituBox tail contact of a body of this size (`TailContactProfiles::neck_radius_mm`:
  the tail touch is a DIAMETER, the ball's radius half of it, the neck the ball over the tip table's
  ratio at the neck's own size; never wider than the body), and the radius the enlarge-foot rung gave the
  post. One reader (`model_feet::foot_radius_mm`) serves the structural reading, the final measurement,
  the adapter and the lowering.
- **Enlarge-foot rung.** A run whose neck and cone alone take the whole one-pixel budget cannot be held by
  any shaft width. After bands and the thicken rung, the foot grows to the neck of least resin (the plan's
  own and the foot ball's, `4/3 pi (f r)^3`) among those that let a root hold the run, read through that
  very neck (`slenderness::enlarge_foot`); the least neck alone needs a body at the registered maximum
  radius, and resin without the ball would run the neck up to the body's width. Where none up to the
  registered maximum holds, the `foot_neck_over_limit` warning stands.
- **Bay-local thickening is one lower cone.** The post widens from its foot to the root at the run's base
  in one straight cone, no steeper than the permitted underside slope (not a hairline under a fat
  section with a few layers of steepest ramp); a cone that enters the model's final gap falls back to the
  steep ramp.
- **Knees.** Where two chained frusta of a member turn by more than a pixel of their section, a sphere
  of the larger section sits at the vertex (`knee_spheres`). The sweep's clearance readers model a member
  as the tube about its axis, which already contains it.
- **A lean is a solid, not a column.** The up-sweep's path test asks whether an arm stands in a cell of
  the path; a shaft 0.12 mm off the path is in no cell and was gone through (round 6, Deinonychus: a 0.22
  mm descent through a post, then 13 mm more to a trunk). A lean path is also refused where it comes
  within (the descent's radius lower bound, the area law on what it covers) plus (the smallest tip) of an
  arm's axis that the seat's own column is not already that close to (`path_goes_through_a_shaft`). Both
  are lower bounds on the drawn radii, so a refused path touches in the file. The landing's own arm is
  exempt. What it cannot see: the radius an EARLIER arm grows to after placement (only the minimum tip is
  known for it), and straight descents.
- **A second contact for a tip its feature leaves short.** The contact growth runs on the finished field, so
  the placement could not know what a feature gives. `run_sweep_with_second_contact` records, for each tip
  left short, the contact it really made, and runs the placement again with that contact as the load the
  tip may carry at its tip layer (`down_sweep::ContactCaps`): the load-driven insertion that relieves an
  arm over the area law's maximum then seats a carrier beside the tip on the same underside and the load
  routes between them. The second plan is kept only where it leaves the summed shortfall (mm²) smaller.
  Costs one more sweep; reported in the log (`second contact:` line) and `SweepRunReport::seat_second_contact`.
  A tip left short after it is warned as before.
  **How it is made** (`SweepPolicy::second_contact_mode`, env `PLINTH_SECOND_CONTACT=replan|edit|
  edit-then-replan`; design `SC-DESIGN.md` §3-§4). The default is `replan`, the run above: on the real
  models the edit is always discarded (it skips the cleanup lanes the finished first plan went through;
  see `EditReport`), so `edit-then-replan` only adds the edit's cost in front of the same re-plan. In
  `replan` no edit work runs; the re-plan itself does not repeat the setup and the up-sweep: the sweep that
  made the first plan kept their result (the cap-independent prefix, `sweep_state::Prefix`: everything
  before the down-sweep's first insertion, the only reader of the caps) beside its seed, and the re-plan
  runs the capped remainder on a clone of it when the seed's key (mesh, layers, policy, drains, vents) and
  first plan are the request's, else the full sweep; the plan is the same bit for bit. The two edit modes (experiments only) do not plan again: they take the sweep's state at checkpoint C (the start of
  `StageJoints`, kept as an `EditSeed`), run ONE insertion phase on a working copy W with the caps, finish
  a CLONE of W, and set that finished field against the finished first plan (P1) on predicates recomputed
  after the last mutation of both (`second_contact::final_checks`, the drawn-solid collision census, the
  keyed per-member warnings, load and overload non-regression, the ground a shrunk contact keeps). A new
  support that fails, or that a regression on an old support is structurally linked to, is withdrawn from
  W with everything that rests on it (budget `second_contact_withdraw_rounds`, default 2; W is sized again
  with insertion off), and a fresh clone is finished. The edit is kept only if nothing regressed and the
  shortfall falls with no more unbraced runs (today's test, on final predicates). Every exit where it is
  thrown away (seed mismatch, no insertion, global regression, untraced regression, no gain, closure
  reaches an old support, nothing survives, budget) goes through ONE fallback: `edit` returns P1;
  `edit-then-replan` runs the `replan` path and its chooser (P1 where that rejects). Nothing reads the
  model's name. The log prints `second contact edit` lines (exit and reason, arms, regressions by class,
  seconds per step); `SecondContactOutcome::edit` holds them.

### Every change to a tip keeps layerwise support (joint review, 2026-10-12)

A solid that is connected when finished can still start a layer with nothing under it, and the
slicer cures that patch in free resin. Two tip rules are held to this:

- **A grown ball prints from its own shaft.** The ball's horizontal sections below its equator
  widen upward, each holding the one below, so its only possible new island is its LOWEST point,
  and that prints only when it lies inside the shaft the ball stands on (a straight arm's taper at
  the tip radius, a head's link). Contact growth moves the ball out along its seat normal; on an
  oblique shaft the lowest point leaves the shaft. Each tip's growth is bounded by the largest ball
  whose lowest point stays inside (`plinth_support::carried_ball_radii_mm`); a tip it stops short
  of plan is warned (`ShortReason::Overhang`).
- **A footprint slide keeps the seat it was placed for.** A disc wholly on some face higher up is
  not support for the material the tip was placed under. A slide is accepted only if the moved ball
  still holds the original seat to within one pixel, or the slide does not climb: the moved disc's
  lowest point stands no more than one layer above the placed disc's (a slide across a level face
  or along a tilted face's strike stays on the laminae the tip was placed under)
  (`footprint::keeps_seat`); otherwise the tip tries its next candidate and, when
  none keeps the seat, keeps its own with a warning (`Footprint::MoveRefused`).
- **A footprint slide leaves nothing it held over air** (fix lane A). The seat is one point; a
  ball holds every cell its section covers at the layer the cell cures on or the one under it. Every
  cell where model may stand (`cover`) that the ungrown placed ball holds and the ungrown moved ball
  does not must join, through cells of its own layer where model surely stands (`carry`), a cell with
  model surely under it, a cell the moved ball holds, or the edge of the window the slide reaches
  (`footprint::keeps_ground`). The no-climb clause alone passed a 0.51 mm slide DOWN a face that
  left its seat island floating, and slides that kept the seat point uncovered material beside it.
- **A junction stands on the shaft the file draws.** A seated member's shaft starts at its ball's
  centre, which the seat law puts off the seat along the seat normal and contact growth puts further
  off. Every member landed or seated on such a shaft follows the drawn line, exactly as it follows a
  leaning plate post's boss line (`adapter::drawn_line_offset`, read on
  `plinth_support::SweepRootFlare::shaft_line_of`).
- **A model foot fits at the size it is drawn.** The descent checks a foot at the tip's minimum;
  on the sized field the foot is drawn for the load it passes. The clearance re-route also sends a
  model landing whose load-sized foot does not lie wholly on the model, and offers it only model
  candidates that fit that foot; one with none left keeps its landing with a warning.

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

### Where the up-sweep asks for an articulated head (Craig, 2026-10-06 and 2026-10-20)

A head (a short straight approach along the face's own normal, then a joint, then the post) is
asked for at placement where the plain tip would SKIM its face: `(g + r)(1/cos x − 1)` above the
rasterisation allowance (`heads::skims`), `x` the angle between the face's outward normal and the
direction the plain arm leaves the seat in. Until 2026-10-20 that direction was taken as straight
down whatever the descent was. Craig: "even if it lands nearby, it *still* should consider an
angled head for the final approach to the model if the angled head is a better head". So `x` is now
the worse of a vertical post and the plain descent's own line, seat to base
(`heads::approach_cos`), for a seat whose straight descent was refused at the final gap (its
descent had to lean). A straight descent reads exactly as before. The same reading for EVERY seat
whose descent leans or merges (`PLINTH_APPROACH_HEADS=all`) is an experiment, not the default:
it turned 18 tests red, among them a load gap on the raking blade (lane t1, `t1/RESULTS.md`).
A built head's post is searched from the
joint by the same descent rules as any seat, and the head passes the same placement checks
(`heads::check_head`: the joint at the final gap from the model, no member, no other head). Where a
straight shaft would graze the model on the finished field, the graze conversion
(`graze.rs`) still applies.

**Tilted heads: an angled tip, then straight down (lane t1, 2026-10-20).** Craig: land on the
neighbouring post, "maybe with angled head to clear the feathers first", instead of "a weird angle
for the full length to the base plate"; "as soon as it clears the 0.6mm gap, it could 'turn' back to
vertical". A head now carries its seat plane's normal apart from its axis (`Head::seat_normal`,
`SweepHead::seat_normal_mm`; `None` for every head along the normal, which stays byte-identical).
The ball is seated against the face as a straight tip is, the head leaves along its own axis to the
joint `K`, and the post drops STRAIGHT from `K` to the plate, the model or another arm's shaft
(`heads::plan_tilted_head`). The joint is solved for that axis and that one straight post
(`heads::solve_joint_for_post`, the law the final re-size draws it with), not for every post the
lean law admits: on Deinonychus arm 196 the every-post joint was 0.655 mm, and with the 0.3 mm
separation it reached a neighbouring arm from every joint within the head's reach. A post re-routed
later grows its joint at the final re-size, and the downgrade refuses a grown joint that no longer
clears. The head's member test excepts the arm its post lands on: that overlap is the junction, as
for a lean's path into the arm it merges into. At every seat that does not
drop straight and clear, every column a joint can stand over is a candidate. A candidate is within
the head's reach, within the lean law, and its head does not skim its face. It is priced by the
up-sweep's own price (head plus post plus any model height still owed). The cheapest that passes the
head's placement checks and the post's final-gap test is taken when it is strictly cheaper than the
plain descent (`up_sweep::plan_tilted_seat_head`). The seat-plane readings (the seat trim and
ball seating in the lowering, the stand-off, `check_head`'s seat exemption, the census's head
chain) use the seat normal; the head law, the walks and the sizing use the axis.

**A lean is tested at the radius it will be drawn at near its seat.** Read at the lower bound
alone, a lean could pass the final gap by micrometres and stand inside it once drawn. A seat's lean
candidate is read at the straight taper from the lower bound to the root the slenderness law
holds the run at unbraced (`run / S`), where the longest final-gap seat zone ends
(`up_sweep::drawn_radius_near_seat_mm`). The straight descent and a head's post keep the lower
bound.

**A short seat move before a lean (orchestrator ruling, 2026-10-21).** Craig's "nearby wins":
lean only as far as needed. Where a seat's straight drop is refused at the final gap and its
descent leans, the seat first tries to move to a nearby cell (nearest first) and drop straight. The
move is no longer than the tip's own planned contact radius (`heads::planned_tip_radius_mm`), so
the moved contact still covers the point the seat was placed for. The moved seat must root the
same demand, its whole contact disc must bear on this layer's underside (no model directly under
any of it), and its straight drop must clear the final gap. Taken, the straight drop is kept over
any lean (`up_sweep::commit_seat`, counted as `descent_seats_moved_for_straight`). With no such
cell, the lean stands.

**Every lean candidate must print (coordinator, 2026-10-21).** Before its path is walked, a lean
candidate is asked the run-alone stage shake (`run_shake`, lane rod): the run from the seat to the
candidate's base, read at the radii it will be drawn at, may move its printing top no more than
one pixel, or than the straight descent's own (zero: it is vertical). The root is the one the
slenderness law draws a run of that length at, `max(radius_lb, run / S)` within the registered
maximum (`up_sweep::drawn_root_radius_mm`); the top is the drawn radius near the seat that the
clearance is read at. `p_b`, `q` and `S` are the stage-shake and slenderness numbers; nothing new.
A refusal is a path hit (`descent_lean_refused_shake`); with nothing left, the straight descent
and the fallbacks stand (emit and warn). Read at the lower bound at both ends (round 4), the rule
refused leans the drawn arm holds, since the shake goes as `r_t / r_b³`, and doubled the runs left
over their limit on Deinonychus and the lying treant. `PLINTH_TILTED_HEADS=off` turns the
tilted-head search off (diagnostic and rollback).

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
* **Kept as today.** A slot whose need is above today's radius and a slot a junction hull's ring
  stands on keep today's radius. Such a slot is drawn under the certificate's own need, so it is
  counted and the log warns with the worst member (`heads keep N post slots under the radius the
  certificate computes`; `CHONKY.md` P3: Deinonychus support A's column keeps 1.286 against a need
  of 1.395-1.603). The certificate never grows a post; the warning says where it stands short. A head that another member loads at or above its joint (shared)
  keeps the old law and is counted (`joint certificate:` line of the log): a member that lands on
  the joint's layer or above, a brace seated within one layer of the joint, or ANY other member's
  drawn body (routed descendants included) that touches the certified link, cone or joint. The
  cone and joint are sized for the head's own contact alone, and a feed that enters there would
  need a larger joint.
* **Junction hulls are today's, exactly (R8).** A junction hull's host, pull-backs and rings are
  planned from the radii of the shafts, so a thinner shaft would move every ring. The lowering
  therefore plans the hulls on the field as the old law drew it (`adapter::hull_reference_arms`,
  from `CertifiedHead::today_radius_mm`) and keeps those records bit for bit
  (`lower_sweep_arms_with_hull_reference`, `junction::keep_planned_hulls`). A reference whose
  shafts do not line up with the arms' is not used, and the lowering warns.
* **A junction cuts nothing out of a member; the drawn solid is the members' union with the
  hull.** The ledger is the load-sized minimum, and a section under it at any slot is an under-draw
  (owner's ruling, 2026-10-21). Every landed junction that is not buried in its host is FILLED: an
  added hull of the branch's whole end ring and the host's own sections a pull-back above and
  below the landing point `K` (`junction::hull_junctions`); a section that lies past the end of the
  host's drawn shaft (a branch landing just beyond it, beside the root) is taken at the shaft's own
  end ring. The host and the branch stay exactly as
  drawn, so no member is thinner anywhere than its own drawn shaft. BURIED means the branch's
  whole end ring lies inside the host's DRAWN solid, read in each host frustum's own section plane:
  square to its axis, or horizontal for a frustum drawn with horizontal rings (a leaned post with
  horizontal rings read square to its axis could take a branch that ends in air for buried). A
  branch buried on the reference field (R8, below) is judged again on the drawn field, where its
  host may be drawn thinner, and is joined there when it is not buried there. The log's
  `junction burial:` line counts both kinds.
  An added hull's own material (its surface outside the two members it joins) keeps the final gap
  from the model and stays out of every third member (`junction::choose_clear_hull`): the full
  hull is drawn where it is clear, else the first clear of the smaller hulls (both host sections at
  one pixel pitch; one side alone, at its pull-back and at one pitch; each again with the branch's
  section pulled back up the branch by its pull-back and by twice it, where the branch's end stands
  against the model), else the one that stands least into the model, warned and counted. A junction
  only chooses how much it adds; no shaft is touched. Members of the junction itself (arms landed
  on the host or the branch, and the arms these land on) are not third members.
  The host is the member the sweep PLANNED the landing on (`Landing::Strut`); the geometry's own
  choice is only a witness (`junction burial:` counts where it differs, and where it finds none). Until 2026-10-21 the pass cut
  the junction into the host: it shortened the branch, deleted the host between two rings, dropped
  the widening run just above `K` as a "collar", drew the host above at that run's thin top radius,
  and hulled the rings. The ledger is wide from one layer above `K` down, and a hull of two rings is
  a straight chord, so that under-drew wherever the thin run stood on the wide ledger, and wherever
  the host's profile had an outward corner between the rings. Measured on lane t1's brace pair:
  member 7 at 0.2507 mm over 27.6 mm where its ledger asks up to 0.824 mm; member 6 at 0.193 to
  0.283 mm where it asks 0.325 mm. On a load step a layer under `K`: 0.7646 mm where the host is
  0.824 mm.
* **The branch's end ring is the old ring, to the bit, but a shaft is never thinned to meet it.**
  The hull takes the branch's whole end ring. Where the reference's ring is thicker than the
  branch's chord there (the chord noise of the fit, or a thinner new sizing), the branch's last
  frustum ends on the ring's centre and radius, so the drawn ring and the hull ring are one ring.
  The difference is closed by a taper along the branch's own pieces, walking away from the ring
  (`junction::plan_transition`): it closes at `|difference| / pixel pitch` per mm at the fastest,
  and never so fast that a piece narrows going up by more than the lean law's step
  (`cot(theta_sat)` per mm of height, the slope the load-step cones are drawn at); a piece already at
  that slope carries the offset unchanged, and the offset closes on the next piece that has room.
  Where the reference's ring is thinner than the chord, the branch keeps its chord and the ring
  stands inside it. The log's `junction rings:` line counts both kinds, and a WARNING names the
  inside ones. A ring no taper can reach is left on the branch's own radius and warned.
* **Report, never refuse.** The log prints the largest certified demand over the drawn chords and
  warns when one exceeds `1 + 1e-9`. The link-overstress warning is read at the link end that is
  drawn and says when the joint is sized for what the link can pass.

What it does not claim: stiffness falls as sections thin (the link dominates compliance), and a
thinner path moves a little load onto a neighbour. Only a frame solve or a coupon settles that.

**A head's post that leans onto another member pays for the moment its offset hands the host**
(`CHONKY.md` P1, 2026-10-12). The descent prices a landing on a member at zero further path: the
host has already paid for its own way down (`up_sweep::deferred_path_mm`). That holds for the axial
load only. The contact `D` carries its load down a vertical line, and a post that lands on a host
whose axis stands `e` sideways of `D` hands the host that load at a lever `e`; with nothing holding
the host's top sideways (the engine cannot claim the model does) the host carries the moment to the
plate. The engine's own stress form (`heads::section_radius_mm`, `r^3 = r_a^2 r + 4 r_t^2 e`) gives the
section it needs, so the lean is charged the host's extra material per mm of its remaining path
(at least the landing's own height, `k_bottom h`), in units of the post's own axial section:
`k_bottom h (r(e)^2 - r(e_K)^2) / r_a^2` (`up_sweep::offset_moment_path_mm`). `e_K`, the horizontal
lever the head itself puts between `D` and the joint, is subtracted: every post from `K` carries
it, a vertical drop to the plate included, so a landing straight under `K` owes nothing. Only a head
post's LEAN onto a member is charged; no constant is new. Deinonychus support A (`CHONKY.md`): the
3.5 mm arm onto B's column is gone, the post drops vertically from `K` (r 1.01) to its own foot, and
the path from the contact to the plate falls from 48.9 to 27.9 mm3 of emitted mesh.

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

**The drain keep-out and the rim tip (ruled by Craig, 2026-10-22).** One physical rule for every
support, thick or thin: no FINAL DRAWN solid enters the bore cylinder projected from the hole,
and there is no standoff. Before an arm is sized its radius is not known, so placement keeps it
out of `bore + the widest solid an arm can be drawn with` (the tip ball at the largest registered
ball factor, the root boss at the Heavy boss factor, the raft pad, plus a 0.1 mm planning margin:
2.85 mm for the registered 1.25 mm bore and 1.0 mm maximum tip radius, the radius develop has
always planned at; without the margin the 0.1 mm ring moved the whole first plan of Giant_Bat,
resin +13 % and 7 new failing leans, and the drawn audit is what holds the rule). After lowering, every support primitive is drawn as
the writer draws it (shafts, balls, flares and bosses, junction hulls, raft pads and bridges) and
tested, triangle against cylinder, against each bore cylinder (`drain::emitted_bore_intrusions`,
which the late re-plan reads); the count that is reported is the final audit's, taken on the triangles
the written file holds (`drain::file_bore_intrusions`: trimmed records, the cylinder's outlet end flat
across the axis, the drains shifted into the file's frame by the writer's centre, rule DK in
`docs/development.md`, "The final audit"). What enters is a warning, never a refusal. A smaller free radius at the mouth is not a failure by
itself, provided nothing enters the cylinder. On a dome whose drain is at its lowest point the
bound leaves the first rim layers with no arm under them (the Sphere's layers 100-106 lie wholly
inside it). Craig's rule for those layers: *"we can remain clear of blocking resin flow while still supporting the lowest layers
with thin support that don't enter the cylinder below the hole - but they can come right up to
the edge and not be blocking"*, and *"hair tips can lean in and just touch the edge of the hole
without blocking drainage."* So a THIN member whose own radius is known (a hair tip, a rim tip)
may stand anywhere outside the BORE cylinder: its solid, `bore + its radius`, may not enter it,
and nothing more (no standoff, no margin for a radius it might grow to). The rim tips
(`hair_tip::repair_drain_rim`) serve the demand the keep-out refused: thin tips that lean in
from a host outside and touch the rim, first layer first; a layer they cannot hold is warned with
its layer, area and position. A rim tip is held to the rule on its drawn ends and widest section,
and the rim's service is counted by membership: a cell is served only by a contact within one
reach, or by a served cell of the layer below within one reach, never by reach from reach. Rim
hairs go through the lean gate of the nearby-brace rule like every hair (a hair's printing top may
shake one pixel at most; a hair has no standing run, so the limit is the pixel). A hair that
would shake is made a TAPER: the contact keeps its thin radius (a rim contact stands on a ring a
fraction of a millimetre wide, and a rim contact is a fraction of a millimetre from the bore wall),
and the root widens toward the host until the run moves 0.7 of the limit
(`RunShake::root_radius_for`), up to the tip maximum and no thicker than the host's own section at
the junction (a thicker root raises the host there, and a bump under a thin top shakes the host
more). The solid is held clear of the bore along its whole length, layer by layer in the screen
and piece by piece on the drawn profile, and a hair that stands on the plate is held to the whole
arm keep-out (its pad is wider than any hair). A contact widened later to meet its need is read
again on the drawn profile and the taper is planned again from it. Only where no hair that passes
can hold a rim patch is the least-shaking candidate used (a layer left floating fails for certain,
a hair that shakes a little marks the surface); the lean census names it with its pixel count, and
the rim report counts it.

The late re-plan of the lowering moves members after the sweep's census of failing leans was
taken, so it carries the moved field out and takes the census again on it (and the rim coverage),
and the report names every failing lean on that field with the pass that made it. Each look of
the loop (up to ten) is judged on that census (`adapter::lean_culprits`: piecewise, rising shafts
turned over, on the drawn profile, against what the plan's own census let each member stand at):
a member whose move made a failing lean, or whose move changed its host's run so that a guest
made one, has the move put back. A separation move so judged is never made again for that member;
an intrusion move is made again by the member's next way out (`drain::ways_out`: straight out, then
turned 30, 60 and 85 degrees either side, each first as a move parallel to itself, which keeps the
lean). Only a member with no way left is kept with its lean and named (a solid in a bore is the
limit). Re-landing the member through the sweep's re-route and shrinking an intruding ball are not
done: nothing needed them on the corpus run. A cheaper two-radius gate (`MoveGate`) pre-filters the
separation moves; it reads ledger radii and understates the shake, so only the census decides.

The limit is a hard limit in the emitted file (Craig, 2026-10-22), so a found intrusion is
re-planned, not only warned. The raft planner is told the bore cylinders and refuses any bridge
whose strip passes through one (a raft with no other route is two pieces, said in the report). A
contact ball, root pad or flare found inside after the lowering is moved out along the model
surface by its depth plus 2 microns, the same size, and the plan is lowered again (up to four
times; a ball that stays inside is moved eight times farther the next time, up to 0.8 mm). What
carries a ball is read from the drawn result: a tip's seat, a leaned member's base, a headed tip's
contact, or, for a model foot, the carried landed-face point (a foot ball is drawn there, not at
the member's base). Contacts at the model end near a drain stay separate (plate-packing rule
`ac2cd0a5`): pairs of contact balls within 2 mm of a bore wall that overlap are moved apart, each
by half the overlap plus the registered final gap; a pair on a vertical line or one that has no
owner is warned. The reported count of fused model ends is the final audit's (rule FU): the
contact patches on the model that two different members' records make, read on the written file.

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

- **Support record triangles have canonical wire order.** After narrowing to the file's f32
  coordinates, rotate each oriented triangle to its lexicographically smallest cyclic order,
  then sort the triangles. This preserves winding, coordinates and multiplicity. Manifold's
  process-global original/mesh IDs must not affect the bytes; unrelated forecasts or parallel
  poses may allocate those IDs in a different order. The model mesh is not reordered.

Reproducibility is checked, not assumed: see the determinism check in
[`development.md`](development.md). Digest movements caused by a behavioural change are a
stop-and-report event with an attributed from→to record (OD-8), never a silently edited
expectation.
