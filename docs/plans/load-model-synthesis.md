# One Load Model: From the Peel Force to Every Seat and Every Tip

Status: DESIGN OF RECORD for support load modelling, superseding its three parent
designs. It changes no production behaviour; implementation is phased in §10 and the
first behaviour-changing phase is gated on the owner rulings listed in §10.5.

Provenance. Three designs were produced independently and judged on three lenses:

| Parent | Branch / commit | Physical soundness | Testability | Blast radius |
|---|---|---|---|---|
| Physics-first | `design/load-physics-first` @ b007edd | 4 | 7 | 6 |
| Strictly empirical | `design/load-strictly-empirical` @ c6b1e26 | 8 | 5 | 3 |
| Whole-pipeline coherence | `design/load-whole-pipeline-coherence` @ 4f54263 | 2 | 8 | 8 |

The whole-pipeline design wins on architecture: its load quantity lives on the slice
stack the printer computes anyway, so it exists before any support does, it is
tessellation-independent by construction, and its phasing and invariants were judged
the strongest of the three. But the physical-soundness judge found its sizing
quantity wrong — it lets most of the peel force vanish instead of routing it to the
supports — and that finding is correct. This synthesis keeps the whole-pipeline
chassis and replaces its sizing quantity with the routed one (§2.2, §2.4), which is
the physics-first design's quantity purged of the flaw the same judge found in it.
The strictly empirical design contributes its evidence discipline: the two coupon
plates every judge wanted (§7.3), the remesh test suite (§5, I4), and the rule that
joins use typed identifiers, never positions (§5, I11). Every conflict between
parents is ruled in §9, with what the losing option would have bought.

---

## 0. The verdict, and the owner's two questions

The engine gets one law right — one square millimetre of support cross-section may
carry twelve square millimetres of model area, measured on two printed plates — and
applies it to the wrong quantity, at the wrong moment, over the wrong geometry.
Supports are placed with no load quantity at all, then an accounting pass that can
only see a fraction of the model hands whole mesh triangles to single supports.
This design computes the load from the printed solid itself, before any support
exists, and lets that one quantity drive placement, sizing, and the final gate.

**"Why are those supports even in place if they carry nothing at all?"**
They do carry load. The ledger is blind, not the supports useless. Placement chooses
seats by height order and a fixed 4.0 mm spacing — no load number is consulted — and
the accounting afterwards can only assign load to faces inside detected overhang
islands, each face given whole to the single nearest support. On the measured
overhang-table fixture, 75.6% of the model's surface has no accounting row at all,
so 71 of 85 emitted supports are recorded as carrying exactly zero and are sized to
the smallest permitted tip (0.05 mm radius = 0.10 mm diameter) — while two supports
recorded as carrying 450 mm² each are ALSO sized at that same minimum, a 20x
violation of the engine's own law, shipped without complaint. The supports are where
coverage wanted them; the numbers describing them are fiction. Under this design a
support recorded as carrying nothing is a named defect (invariant I6), never a
disclosure.

**"Does the current allocation system make sense physically?"**
No, in three specific ways — and one idea in it is right and survives. Wrong:
(1) eligibility is provenance, not physics — a face gets load accounting only if an
island-detection pass happened to claim it, so most of the model's area is invisible
to the load model; (2) a face is never split — the nearest support takes the whole
triangle however large, so one support is overloaded and its neighbours record zero;
(3) load is computed only after every seat is already fixed, so it can never
influence where supports go. Right and kept: dividing area among supports by
nearness is the correct continuous reading of the measured group-additivity result
(plate 6), and the sizing arithmetic itself is today, by numerical coincidence,
exactly the measured area law at ratio 12. What changes is what gets fed in, when,
and over what geometry.

---

## 1. The model in plain language

This section is self-contained. Every term is defined when it first appears.

**How the printer loads the part.** A resin printer builds the part upside down,
hanging from a metal build plate. Each new layer of liquid resin is cured by light
against a transparent film at the bottom of the resin tank. To print the next layer,
the machine must lift the part and tear the fresh layer off that film. The film
holds on by suction. That tearing force — the **peel force** — happens once per
layer, and it is by far the largest force in the whole process: measured in the
literature at up to hundreds of newtons, where the part's own weight is a few
newtons at most. The peel force acts over the AREA of the layer being torn off. A
layer of twice the area holds on twice as hard, within the range we have measured
(§7.1 states where that range ends).

**Where the force goes.** The peel force pulls the part toward the film. The only
things holding the part up are the printed structure itself: the model's own cured
material, and the **supports** — thin printed pillars running from the build plate
(or from lower parts of the model) up to small contact tips on the model's surface.
When a layer is torn off the film, its peel force travels through every layer of
material between it and the plate, like weight on a chain of links. Any link too
thin for the force snaps. A snapped link is what a failed print looks like: a
support tip ripped off, or a model limb torn at its thinnest point.

**The one measured law.** Two printed coupon plates (72 and 34 specimens) measured
when that snap happens. The result is a pure ratio: a cross-section of green resin —
"green" means as-printed, before the hardening UV bath — fails when asked to carry
about 25 times its own area. The design ratio, holding back a factor of two, is
**12**: one square millimetre of cross-section may carry twelve square millimetres
of layer area. The plates also measured that the break happens at the NARROWEST
point of the load path, not necessarily at the tip: on every large specimen the
break was at the waist where the straight section meets the cone below it. And they
measured that several supports under one load share it at least additively: four
arms carry at least four times what one arm carries.

**What a support carries, in one sentence.** A support carries its share of the peel
force of the worst single layer of the part it helps hold — "worst" meaning the
layer whose share, for that support, is largest over the whole print.

**Where supports must exist.** Two different questions hide inside "the supports":

- *Where must a support be?* Wherever new material appears with nothing under it.
  When a layer contains a region that does not sit on the previous layer — the
  first slice of an outstretched hand, say — that region is stuck to the film with
  nothing holding it. A support must already be standing there to receive it. The
  same applies when a region grows sideways faster than its footing can carry.
- *How thick must each support be?* Thick enough that its share of the worst
  layer's area is at most twelve times its narrowest cross-section — checked at the
  tip, at the waist, and at every point down its trunk to the plate.

Today's engine answers the first question with a height-ordered scan and a fixed
4.0 mm grid, and the second with an accounting pass that cannot see most of the
model. This design answers both from the same computed quantity: a layer-by-layer
map of how much area is being torn off and which support's territory it lands in.

**Three consequences the owner will notice.**

- *Hollowing finally counts.* A hollowed model's layers are thin rings, not solid
  discs. The load model reads layer cross-sections directly, so a hollowed model
  demands far less support section than a solid one. Today the load model cannot
  see hollowing at all.
- *Mesh quality stops mattering.* The load is computed from the sliced solid, never
  from mesh triangles. Re-triangulating the same shape cannot move a tip size.
  Today, subdividing a test fixture's triangles — same solid, same surface area —
  moves the median emitted tip from 0.100 mm to 0.631 mm diameter.
- *Thick and thin become explainable.* Every tip diameter is the direct image of a
  disclosed area number through one formula. "Why is this support thick?" always
  has the answer "because it is the nearest carrier of this many square millimetres
  of worst-layer area, here."

---

## 2. The precise model

Units: millimetres and square millimetres throughout. Internal radii are RADII in
mm. Every human-facing disclosure states both radius and diameter, because the owner
and the ChituBox UI speak diameter while this registry speaks radius; a single-unit
disclosure is a defect (invariant I13).

### 2.1 Definitions

- **Layer k** — the print slice at height z_k, thickness h (0.050 mm on every plate
  printed so far; no other layer height has any evidence, §7.1).
- **S_k** — the solid cross-section of model plus supports at layer k, as polygons
  with holes. The engine already computes this for the whole stack
  (`perception::normalized_slice_layers`); the demand field is a new consumer of an
  existing, already-paid-for artifact, not a new geometry stage.
- **Cell** — a square of the demand grid, pitch **q = 0.25 mm** (area 0.0625 mm²).
  q is DERIVED, not tuned: one cell must never carry more area than the smallest
  permitted tip can hold, so q <= sqrt(X_ALLOWED * pi * r_min²) = 0.3069 mm, and
  0.25 is the largest convenient binary fraction under that bound. It moves if and
  only if one of its two registered parents moves. (This replaces the physics-first
  parent's reuse of `cell_edge_limit_mm`, which is itself derived from the spacing
  constant that design was deriving from the raster — a verified circularity.)
- **Component** — a connected body of already-printed material at time k: cells
  connect within a layer by adjacency and across layers by overlap. Components are
  tracked in print order (increasing z). This connectivity is part of the
  definition: the existing `point_reach_mm` has a filed KNOWN GAP where the nearest
  solid one layer down may belong to a different body (a neighbouring limb, another
  model on a crowded plate); the field builder computes reach against the SAME
  component only, closing that gap at the definition layer instead of patching it
  later.
- **Orphan set O_k** — the cells of layer k that the layer below cannot carry:
  reach(p, S_{k-1} restricted to p's component) > K*h, with
  **K = sqrt(X_ALLOWED / 6) = sqrt(2)**. K is not new physics: it is bit-for-bit
  the engine's shipping low-point reach law
  (`registry_low_point_reach_allowance_mm`), today the sole production reader of
  X_ALLOWED, with Craig's 2026-08-19 ruling already fixing the conservative end of
  its bracket. A region born entirely orphan (a new island) has reach infinity
  everywhere: "support the island birth" becomes a theorem of the definition
  instead of a rule beside it.
- **Seat** — the point where a support touches what it stands on. Valid seats are
  unchanged: the plate, the model surface (cavity interiors included), or another
  support. A seat is **engaged at layer k** if its attachment height is at or below
  z_k — a wrist seat at 20 mm does not exist while the 5 mm layers peel, and the
  wrist does not exist for it to hold.

### 2.2 The two demand quantities — where, and how much

The parents split on one question and each lost points for its answer, so it is
stated here as the design's central distinction. There are two demand quantities,
computed from the same field by the same sweep, used for different decisions:

**Placement demand** (WHERE a support must stand) is the orphan field O_k. New
supports are demanded exactly where material appears that its own footing cannot
carry: island births and neck-outruns. This is the whole-pipeline parent's field,
kept intact, including its independence property: O_k is a function of (geometry,
layer plan, K, q) only — no support, no mesh triangle, no island id appears in it
(invariant I10).

**Sizing demand** (HOW MUCH each support carries) routes the FULL cross-section of
every layer, not just its orphan cells. This is the repair of the whole-pipeline
parent's judged-fatal flaw, and it is a force balance, not a preference: when layer
k peels, the suction acts on all of S_k — including every cell sitting comfortably
on the layer below. Overlap gives that force a path through the model, but the path
ends where every path ends: at the seats. Area that overlaps the layer below is
carried BY the model DOWN TO the supports, not instead of them. A demand rule that
drops overlap area makes force disappear — the judge's sphere example: one 0.7 mm
support sized from a 4.7 mm² first-layer cap while 707 mm² equator layers peel
above it. That support snaps.

### 2.3 The charge sweep

One pass over the layers in print order (increasing z, the order layers exist):

1. Build S_k on the grid (bitset containment), label components, compute O_k on the
   thin new band only — interior cells sitting on layer k-1 are settled by a cheap
   overlap test, and the exact continuous reach runs only near the boundary,
   O(perimeter/q) cells per layer.
2. Rescale each region's cells so their sum equals the exact polygon (shoelace)
   area, assigning the rounding residual to one designated last cell — conservation
   is then exact BY CONSTRUCTION (the physics-first parent's residual-assignment
   rule), not to a tolerance.
3. Charge every cell of S_k to a seat: each cell goes to the nearest engaged seat
   of its own component, distance measured THROUGH SOLID MATERIAL — within a layer
   by 8-neighbour shortest path over solid cells, across layers by inheriting
   labels through overlap with distance accumulating by h per layer. Load travels
   through green resin, never across an air gap; a cell can never be charged to a
   support under a different limb. Charges fold immediately into per-seat,
   per-layer accumulators; the cell-level charge list is never materialised (a
   treant already produces 70,737 rows under the face model; the cell model would
   produce millions).
4. A cell whose component has NO engaged seat is a **named refusal**
   (`UncarriedDemand`), never a zero — the deleted auditor's one genuinely valuable
   policy ("a missing load partition is a named failure, never a plausible zero
   score"), inverting today's engine, which silently records zero and sizes the
   minimum tip.

Then, per support c: **demand(c) = max over k of charge(c, k)** — max, not sum,
because peel happens once per layer; the forces of two layers never act together.
Per forest member m: demand folds over its subtree (§4.4).

**Rule R (kept from the whole-pipeline parent):** demand is a function of
(geometry, layer plan, seat set, forest topology). It is never a stored number that
survives a plan edit. Every seat add, removal, or rewire re-derives the charges; an
incremental ledger is welcome but must equal the from-scratch recomputation
(invariant I5). Rule R plus typed-identifier joins (I11) makes the measured 20x
inversion — 450 mm² recorded against a 0.10 mm-diameter tip — unreachable
regardless of which of its two suspected mechanisms is real.

### 2.4 What the sizing rule assumes, stated before anyone finds it the hard way

Routing the full layer is the CONSERVATIVE reading of the evidence, and its
conservatism has a shape the owner should know:

- Inside the measured band — per-seat charges of 3.14 to 122.7 mm², the cap areas
  plates 5 and 6 actually printed — this is the measured law, applied in the units
  it was measured in.
- Below 3.14 mm² it extrapolates in the safe direction: the same printer's own
  slender-specimen pressure anchor is ~50-80x LOWER than its flat-patch anchor, so
  small slivers are over-provided for, never under.
- Above ~122.7 mm² engaged at once it is a linear extrapolation that ignores the
  travelling peel front. The tilting tank does not release a big layer all at once;
  a separation front sweeps across it, so the instantaneous engaged area of a LARGE
  or ELONGATED layer is a band, not the whole thing. Nothing in this repository
  measures that band's width — and the sweep direction itself is unobserved (the
  registered `SATURN_LEADING_RELEASE_EDGE = Measured` traces to a provenance table
  that merely asserts it) — so this design takes NO front credit and over-sizes
  large flat and long horizontal geometry accordingly. §6.1 shows the worst case
  honestly; coupons C10 and C12 (§7.3) are the instruments that would buy the
  discount. Field practice on long limbs (about a dozen 0.8 mm-diameter tips
  holding a 400 mm² diametral plane — an apparent ratio near 60 against a measured
  failure boundary of 25 on compact caps) almost certainly works BECAUSE of the
  front; the design declines to assume it until it is measured.

### 2.5 Every number, traced

| Quantity | Value | Status / source |
|---|---|---|
| X_STAR_CONE | [25.00, 25.34] | Measured, plate 5, cones. Lower edge is a recorded read (specimen TB09); upper edge is a monotonicity inference — keep the interval, never a midpoint. |
| X_STAR_CYLINDER | (19.91, 30.48) | Measured, plate 6, cylinders, n = 1 per edge, admitted by a recorded post-hoc override of a pre-registered plate-wide void. Contains the cone interval; kept separate. |
| X_ALLOWED | 12 | Design ratio: half the recorded lower edge 25.00, rounded down. Rests on an observed specimen, not on the inference. CONDITIONAL on one unidentified black resin at ~30 C (vendor and batch never recorded) against a 2.4x green-strength class spread — a stated condition of use, registered as such (§10.4). |
| The law | pi * r² >= A / X_ALLOWED at every cross-section of the load path | Measured (plates 5 and 6); the narrowest-section form is the observed failure mode. |
| Sharing | additive, measured to N = 6, lower bound only | Plate 6 family B6. No strip ever failed: no upper bracket, and ~20% of the observed headroom is a fin-root section bonus. Never design on super-additive credit; never extrapolate past N = 6. |
| K (reach allowance) | sqrt(X_ALLOWED / 6) = 1.41421 | The shipping low-point law; conservative end ruled by Craig 2026-08-19. |
| q (demand grid pitch) | 0.25 mm | Derived: largest binary fraction under sqrt(X_ALLOWED * pi * r_min²) = 0.3069 mm. New registered constant — the only one this design adds. |
| Per-tip capacity | X_ALLOWED * pi * r²; 37.699 mm² at r = 1.0 | Derived from the law and the registered maximum tip. |
| r_min | 0.05 mm radius = 0.10 mm diameter | Owner's field minimum (short tips only, per field practice). |
| Scar ceiling | 0.400 mm radius = 0.800 mm diameter | Owner's field ceiling; external requirement, NOT physics; bounds growth-for-attachment only, never a load-sized tip. |
| Spacing law | s = 6.598 * r (hex packing at capacity) | Derived; replaces TARGET_SPACING_MM as a decision (task #58). |
| Self-support angle | 35.264 degrees | DERIVED prediction (h cancels: layer-height independent), not a decision. The registered 45 degree mask stays as the conservative admission gate until coupon C8 reads (§3.4). |
| lambda(x, y) (position severity) | 1.0 | Registered Placeholder slot. The repo's only position number is a <= 1.53x NON-DETECTION at plate 6's own ladder step; the purpose-built controls were never scored. Ignoring position is an assumption, declared, with C1 as the instrument. |
| Weight, inertia | absent | Weight omitted by enumeration: shell weight-to-peel is size-invariant at 0.1-8.8% across the three pressure anchors; a solid never crosses parity inside the build volume (crossover 171 mm at the most weight-favourable anchor vs a 118 mm axis). Inertia is UNBOUNDED — no lift kinematics are recorded anywhere; E4 reads them off the slicer profile. |

The four legacy placeholders (PEEL_PRESSURE_PA, TENSILE_GREEN_MPA,
TIP_SAFETY_FACTOR, and the stress-form radius built from them) retire to
diagnostics. Their combination happens to equal the measured law at ratio 12.000000
exactly, today — a coincidence of four unmeasured numbers with nothing tying it to
X_ALLOWED, and two of the four jointly imply failure at ~0.007% strain, about 1000x
off plausible (task #63). Sizing reads the measured constant directly; the
coincidence stops being load-bearing (§9, ruling R-d).

---

## 3. How placement changes

### 3.1 Why the load model should drive placement

Because capacity is bounded, density is a load consequence, not a style choice. The
largest legal tip (1.0 mm radius = 2.0 mm diameter) carries at most
12 * pi * 1.0² = 37.699 mm². Any patch of demand bigger than that NEEDS more than
one support, arithmetically, before any aesthetic enters. Today no load quantity
appears anywhere in selection — candidates are sorted by height, delicacy, and
position, then thinned to a fixed 4.0 mm grid. One line shows the incoherence: hex
packing at 4.0 mm spacing gives each seat a 13.86 mm² territory, which at the law
demands a 0.606 mm radius = 1.212 mm DIAMETER tip — half again the heaviest support
the owner has ever needed in ChituBox (0.800 mm diameter). Today's spacing silently
implies a tip size nobody chose.

### 3.2 The objective, written down

The scar invariant (docs/physics.md) says total scar area is demand / X_ALLOWED no
matter how the tiling is chosen, so total scar CANNOT be the objective — it is a
constant. The two levers that remain are where scars land and how many there are:

    minimise    sum over seats of delicacy(seat) * pi * r_tip²   (scar where it hurts least)
    then        minimise seat count
    subject to  (a) every orphan cell is charged
                (b) charge(c) <= X_ALLOWED * pi * r_c² for every seat and layer
                (c) r_c <= r_ceiling
                (d) every seat is admissible (existing PR-1..PR-5 checks, unchanged)

r_ceiling is a dial with two registered stops: the owner's field ceiling
(0.400 mm radius = 0.800 mm diameter — many small scars) and the registered maximum
(1.0 mm radius = 2.0 mm diameter — fewer, larger scars). Which stop is the default
is an owner ruling (§10.5, R3).

### 3.3 The algorithm, per orphan region

1. n_min = ceil(region worst-layer demand / (X_ALLOWED * pi * r_ceiling²)) — "we
   need this many supports here to carry this area", the owner's phrasing made
   arithmetic.
2. Seed n_min seats on a hexagonal lattice at the derived spacing s = 6.598 * r,
   snapped to admissible seats, biased away from high-delicacy surface — the
   migration plan's own named placement algorithm of record, never previously
   implemented.
3. Charge by the sweep (§2.3).
4. While any seat is over capacity, insert a new seat at its farthest charged cell
   and re-charge. Each insertion strictly reduces the maximum share, so the loop
   terminates. Sizing refusals feed back here: over-capacity demand adds seats;
   it is never clamped away (§4.2).
5. Size every seat from its charge (§4.1).

Derived spacing at the stops: r = 0.400 mm gives s = 2.64 mm; r = 1.0 mm gives
s = 6.60 mm; today's 4.0 mm sits at the tip size nobody chose (1.212 mm diameter).

### 3.4 What placement stops consulting, and the one gate that stays

Retired as decisions: TARGET_SPACING_MM (becomes a seed hint; task #58 closed);
MIN_CONTACTS_PER_ISLAND (subsumed by n_min); island provenance and `LoadOwner`
(and with them the second, unnoticed tessellation dependence: islands are today
ordered by TRIANGLE COUNT, and that ordering names the load owners).

The OVERHANG_ANGLE_DEG = 45 mask stays as the conservative outer admission gate
through the transition. The field predicts 35.264 degrees is the true self-support
threshold (steeper faces advance less per layer than the reach allowance and
produce no orphan cells), and predicts it does not move with layer height. Until
coupon C8 confirms that, the engine admits everything the 45 degree mask flags and
measures how much of the extra is genuinely zero-demand. A derived number this
design is proud of is still an unmeasured number.

During phases 1-3 the existing coverage judgment also stays on as a safety net, so
the new placement cannot silently drop a seat the coverage closure wanted.

---

## 4. How sizing changes

### 4.1 Tip radius from demand

    r_tip = sqrt(demand / (X_ALLOWED * pi)),  raised to r_min = 0.05 mm if smaller.

Read X_ALLOWED from the evidence registry. This is REVIEW.md short-list 6 taken in
the physics direction: `FrozenTipLoadPolicy` retires to a diagnostic, and its
`implied_area_ratio() <= X_ALLOWED` check moves out of `#[cfg(test)]` into the
constructor so the coincidence can never drift silently. Where today's clamps do
not bind, emitted numbers do not move — the legacy form already equals this law.

### 4.2 Bounds are refusals, never clamps

Above capacity (demand > 37.699 mm²) the seat is refused BACK TO PLACEMENT, which
inserts another seat and re-charges (§3.3 step 4). Today the engine clamps at
1.0 mm radius and ships the violation — the overhang table ships two tips carrying
450 mm² each at 0.05 mm radius = 0.10 mm diameter, worst shortfall 0.95 mm of
radius. Under this design an unattainable requirement is a named failure of the
run, never a quiet undersize. The grown-scar ceiling (0.400 mm radius) continues to
bound growth-for-attachment only; it can never shrink a load-required section.

### 4.3 The law holds at the narrowest section, everywhere on the path

Plate 5 broke every large cap at the waist above the cone, not at the tip disc. So
the obligation is stated over the whole load path:

    for every seat c and every point s on its path to the plate:
        pi * r(s)² >= demand_at(s) / X_ALLOWED

A frustum's minimum is its smaller end, so straight tapers are checked at their
ends; the binding point in a forest is normally just rootward of a junction, where
demand has jumped but radius has not. `min_section_on_path()` is a required
deliverable — the engine has no general locator today, and a fast-flare constraint
keeps the tip disc the minimum section of the stub. Where Murray's law sizes a
junction (two equal children: 1.189r) the area law demands more (1.414r) — Murray
is 16% thin per junction and compounds with depth; today the peel-tension term
happens to win the max() and hide this. Under this design the min-section invariant
guarantees it instead of leaving it to whichever of four max() arms is largest.

### 4.4 Members: sum of peaks now, max of sums when measured

A trunk carrying two subtrees needs, exactly, max over layers of the sum of its
subtrees' charges at that layer (different layers never peel together). The design
ships the simpler, strictly conservative fold — sum of each subtree's peak — and
names the instrument before the machinery: E3 measures total member volume in mL
across the corpus under both folds. If the saving is small, the sweep-line is not
worth its complexity. (Physics-first parent's discipline: measure before buying.)

### 4.5 Purity and the write-time gate

Sizing is a pure function of the accepted plan. After ANY sequence of plan edits,
the disclosed tip radius equals the radius recomputed from scratch (invariant I5 —
the cheapest high-value test in any parent, and it kills the measured inversion
without diagnosing it). At write time, a gate re-derives demand and the law from
the WRITTEN artifact — the load gate the Python engine had and Rust never ported
(task #59) — so the file on disk, not the planner's memory, is what passes.

---

## 5. The invariants

Each is written to become a test. THE SABOTAGE CLAUSE applies to all of them: every
invariant ships with a companion test that perturbs the ledger and asserts the gate
goes RED. A gate never seen red is not a gate — this project has already been
bitten by a byte-identity proof over a path that never fired.

- **I1 Conservation, exact by construction.** Per layer k: sum over seats of
  charge(c, k) + UncarriedDemand(k) == area(S_k), exactly — the residual-assignment
  rule (§2.3 step 2) makes the cell sum equal the polygon area by construction.
  Test: any fixture, every layer; sabotage by dropping one cell's charge.
  (Today this fails maximally: 75.6% of the overhang table's area has no
  accounting row, and the "ownerless" disclosure honestly reports zero because the
  invisible faces never became rows.)
- **I2 Exclusivity.** Every cell of S_k is charged to exactly one seat or to
  UncarriedDemand. Test: count labels per cell in a debug sweep.
- **I3 Law at the minimum section.** For every emitted seat and every member on its
  path to the plate: pi * r_min_section² >= demand_there / X_ALLOWED, grown-for-
  attachment tips exempt BY NAME. The assertion shape already exists at
  `prepare_folder.rs:445-520` but runs only on the one fixture that passes; it
  extends to every fixture and, in phase 5, to the written artifact. Test: the
  overhang table, which fails it today with a 0.95 mm radius shortfall.
- **I4 Tessellation invariance, to a stated tolerance.** Re-triangulating the same
  solid — subdivide, decimate without moving the surface, reorder, re-diagonalise
  (the strictly-empirical parent's full suite; re-diagonalisation is the
  perturbation a mesh library performs for free) — changes per-seat demand by at
  most a relative 1e-9 and no emitted tip radius by more than one raster quantum.
  NOT bit-identity: collinear vertices inserted by subdivision change floating-
  point summation order in the slice areas by design-irrelevant last-ulp amounts,
  and a welded midpoint need not be exactly collinear in f64. Claiming bit-identity
  would ship a red gate on day one and teach someone to loosen it (the judge's
  finding, adopted). Conservation (I1) stays exact regardless — the residual rule
  does not depend on the mesh. Test: the ground-truth lane's `subdivide_once`
  instrument, plus decimation/reorder/re-diagonalisation variants.
- **I5 No stale demand.** After any removal, append, or rewire, the incremental
  ledger equals a from-scratch recomputation, bit for bit, and the disclosed tip
  radius equals the recomputed one. Test: randomized edit sequences; sabotage by
  skipping one re-charge. (The measured 20x inversion is this invariant failing
  today: tips sized from the prepared plan while the disclosure reads the accepted
  plan's re-partition, with nothing re-sizing in between.)
- **I6 No zero-demand emissions.** Every emitted seat has demand > 0 or a declared
  non-load disposition naming why it exists. Test: the overhang table (71 of 85
  today), make_box (8 of 11 today).
- **I7 Hollowing is visible, and by the right amount.** For the same outer shape,
  the per-layer sizing charge of the hollowed solid equals the solid's times the
  ring-to-disc area ratio of that layer (sizing charges full cross-sections, so
  this is an equality per layer); placement demand satisfies the inequality
  demand(hollow) <= demand(solid). Test: sphere fixture, solid vs shelled.
- **I8 Scar accounting with bounded slack.** Sum over seats of pi * r_tip² ==
  total worst-layer demand / X_ALLOWED + slack, where slack = sum over minimum-
  bound tips of (pi * r_min² - demand/X_ALLOWED), computed, non-negative, and
  disclosed. No unbounded "clamp slack" term exists because clamps do not exist
  (§4.2) — the judged vacuity is removed by removing its source.
- **I9 Scaffolding carries zero by construction.** No cell can be charged to a
  member that is on no cell-to-plate path; braces and raft members are provably
  chargeless. Corollary, sharp: a zero-demand row in the emitted set is always a
  defect, never scaffolding. Test: charge-map assertion over a braced fixture.
- **I10 Field independence.** The demand field's digest is a function of (mesh
  fingerprint, q, layer plan, K) ONLY. Perturbing the seat set must not move the
  field, only the charges. This is the invariant that makes load-driven placement
  possible, it is not expressible against today's contact-indexed partition, and
  it is checkable in phase 1 before anything depends on the field — the cheapest
  falsification of the whole design.
- **I11 Identifiers, never positions.** Every join between charges, seats,
  disclosures, and artifacts uses the canonical typed id; a positional index in a
  join is a compile-error by type. (Strictly-empirical parent; closes the
  positional-id mechanism at `specialized.rs:1611` by construction.)
- **I12 Pre-write agreement.** The write-time gate's report and the emitted
  artifact agree on ids, demands, minimum sections, and pass/fail. Test: corrupt
  the artifact after planning; the gate must go red.
- **I13 Units.** Every human-facing disclosure prints radius AND diameter. Test:
  grep-level lint over disclosure formatting.
- **I14 Bounded cost.** The field build and every re-charge run under a registered,
  metered budget beside the engine's existing resource caps — never a naked hang.
  This project's signature trap is an unbounded search in a test that never
  terminates; the demand sweep must be structurally incapable of joining it.
  Test: budget exceeded => named failure, not a stall.

---

## 6. The degenerate cases

All arithmetic at X_ALLOWED = 12, h = 0.05 mm; every radius stated with diameter.

### 6.1 Thin arm — and the honest cost of the conservative sizing rule

A horizontal solid limb, 10 mm diameter, 40 mm long. Its first layer is a strip
1.41 x 40 mm = 56.4 mm² of pure orphan: placement demands seats along it
immediately (about ten at the 0.8 mm-diameter stop). Sizing then routes the worst
layer — the 10 x 40 = 400 mm² diametral plane — over those seats: 33.3 mm² of
total tip section, i.e. roughly 67 tips at 0.400 mm radius (0.800 mm diameter), or
the same section in fewer, larger tips below the 2.0 mm-diameter maximum. That is
MORE support than field practice uses on such a limb (a dozen 0.8 mm tips — which
sits at an apparent ratio near 60, past the measured compact-cap failure boundary
of 25, and survives, almost certainly because the travelling front never engages
the whole 400 mm² at once). This is the named price of not outrunning the data:
the design over-sizes elongated horizontal geometry until C10/C12 measure the
front and license the discount (§2.4). The orphan-only rule would have matched
field practice here and under-sized the sphere by 150x; between the two errors
this design buys the one that cannot snap. (§9, ruling R1 records the trade.)

### 6.2 Wide flat plate

A 30 x 30 mm underside: 900 mm² in ONE layer, all orphan at birth. Total tip
section 75 mm² regardless of tiling (the scar invariant). At the field ceiling:
~150 seats at ~2.6 mm spacing, 0.800 mm diameter each. At the registered maximum:
24 seats at 6.6 mm spacing, 2.0 mm diameter each. Today: 63 seats at 4.0 mm, of
which 71 of 85 emitted contacts (fixture-wide) record zero. Compact flat area is
exactly where the coupons are valid and where big prints genuinely tear off, so
no front credit is taken here even though the plate is large.

### 6.3 Sphere — the case that chose the sizing rule

Solid 30 mm sphere. First layer: a 4.71 mm² cap — placement seats it. Equator
layer: 706.9 mm². Ensemble requirement: 58.9 mm² of tip section across all engaged
seats — e.g. 19 seats at 2.0 mm diameter, spread over the lower dome by the
capacity loop. The orphan-only parent sized this print at ONE 0.707 mm-diameter
support; the routed rule prices a big solid sphere at what it costs to hold.
Hollowed at a 2.5 mm wall the equator ring is 216 mm² and the requirement drops to
18.0 mm² (6 seats at 2.0 mm diameter) — the hollowing lever, now visible (I7).

### 6.4 Hollow shell

Free, and by the right amount: `SliceSolid::area_mm2()` already subtracts holes,
so a shell's layers are rings and demand falls with the wall. Cavity ceilings are
new solid where a hole was — orphan by the same rule, demanding cavity-interior
seats, which satisfies the standing seat ruling with no special case. A hollowing
regression (the treant that used to hollow and no longer does) becomes a step
change in the demand field, not just a mL number.

### 6.5 Solid heavy part

Demand is peel only; weight is excluded BY ARITHMETIC, not by plate: shell
weight-to-peel is size-invariant (both scale as length squared) at 0.1-8.8%
across the three pressure anchors, and a solid never reaches parity inside the
build volume. The residual that no plate can currently separate — every specimen
ever printed was a flat cap where area and volume are proportional — is bounded
at ~35% in the worst credible corner and is closed by coupon C2. Inertia is the
un-enumerated hole: no lift kinematics exist in the repository (E4).

### 6.6 A support that genuinely carries nothing

Under this design it barely can exist: every seat takes the cells nearest it, so a
zero-demand seat means another seat is essentially coincident or the seat sits on
material that self-carries entirely. Disposition: remove it, or retain it with a
declared non-load disposition (stability, bracing anchor) — never silently
minimum-size it. Braces and raft members are excluded from charging structurally
(I9), so "it's just scaffolding" is never the explanation for a zero on a model
contact.

### 6.7 A face no support claims

Two readings, both answered. A face that needs no support (steep, or up-facing)
produces no orphan cells — but its material still appears in every S_k it
intersects, so its peel load is still routed and sized; nothing is invisible to
sizing. A DOWN-facing region whose orphan cells reach no engaged seat is
UncarriedDemand: a named refusal that fails the run, never a silent gap. Today's
engine has a third category — area outside any eligibility row, unmentioned by any
disclosure — and this design's I1 makes that category unrepresentable.

---

## 7. What this design cannot do, and what would extend it

### 7.1 Stated limits

1. It computes no force in newtons. The evidence is a ratio; recombining the
   placeholder pressure/strength/safety trio is forbidden by the project's own
   physics notes, and this design keeps that prohibition.
2. It applies one X across 0.094-37.7 mm² per-seat charges while the printer's own
   two pressure anchors differ by 54-81x with contact geometry. Below the measured
   band this errs safe (§2.4); C9 closes the bottom 1.5 decades.
3. It takes no travelling-front credit, so it over-sizes large compact undersides
   modestly and elongated horizontal geometry substantially (§6.1). C10/C12 are
   the ransom.
4. It has no plate-position term: lambda = 1.0 is a declared Placeholder. The only
   in-repo number is a non-detection at ladder resolution; even the leading release
   edge is unobserved. C1 — beginning with simply reading the never-scored
   existing plates — bounds it.
5. It has no weight term (bounded residual, §6.5) and no inertia term (unbounded;
   E4 first).
6. X* is corroborated on two arm geometries, not replicated to the project's own
   three-plate promotion rule; X_ALLOWED = 12 is conditional on an unidentified
   resin. C4 replicates with vendor and batch recorded.
7. Free length is unmeasured (the length-ladder family was dropped from plate 6's
   build); X* is conditional on 4.00 mm arms.
8. Sharing past N = 6 and super-additive credit are forbidden, not known.
9. The 35.264 degree self-support angle is derived, not measured; the 45 degree
   mask remains the admission gate until C8 reads.
10. Cost is modelled, not measured: ~160k grid cells and ~1200 exact reach queries
    per layer for a 60 mm model, folded per layer. Phase 1 must publish measured
    per-model build cost and a per-repair-trial budget (I14) — the repair loop
    already runs 274 trials at a 6.5 s median on one fixture, and Rule R
    multiplies into it.

### 7.2 In-engine evidence, before any printing

- **E0 (phase 0, no code changes beyond instrumentation):** the corpus histogram
  nobody has — per model: contact count, eligible-row count, zero-carry count, and
  the carried-area distribution. Also: extend the `prepare_folder` law assertion
  to every corpus model as #[ignore]d evidence and count how many models it
  reddens. This is the baseline every later claim is measured against.
- **E1:** minimum-section audit over every emitted member on the corpus — the
  distribution of routed-area to narrowest-section, locating today's real X per
  arm. Nothing today locates the section that actually broke on plate 5.
- **E2 (phase 2):** the shadow divergence table — per contact, old carried area
  beside new routed demand, on every fixture and corpus model. The adversarial
  review gate for the whole design, and the numbers Craig rules on before any tip
  moves (§10.5).
- **E3:** member volume in mL under sum-of-peaks vs max-of-sums (§4.4).
- **E4 (free):** read lift speed, lift distance, retract speed, and acceleration
  from the slicer profile — the entire current knowledge of dynamics is "Not
  specified".
- **E5 (half an hour, before phase 2):** one instrumented run printing, per
  contact, the PREPARED carried area and tip beside the ACCEPTED ones — separates
  the two candidate mechanisms of the 20x inversion. Rule R makes both
  unreachable, but if it is the id join alone, the phase-2 divergence table would
  otherwise look deceptively clean.

### 7.3 Coupon program — all pre-registered in plate 5's style

Standing rules for every plate: a computed discriminant, a must-survive and a
must-fail control, matched +/-Y pairs, no factor assigned by a position formula,
nothing scored above z ~ 9 mm (the standing rule from plate 6's void family), and
THE RESIN VENDOR, BATCH, AND AMBIENT RECORDED — neither shipped plate did.

- **C1 Position (read before printing anything):** plate 1 family D and plate 5
  rows R1/R5 were purpose-built for the position question and never scored. If
  the physical plates survive, reading them is cheaper than any new print; if
  not, one position-sensitive coupon at both ends of Y identifies the leading
  release edge and bounds lambda(y) in a single print.
- **C7 Footprint vs surface area — the plate that tests the load quantity.**
  Three cap tilts (0/45/70 degrees) at matched cap SURFACE area on identical arms
  at fixed X. Footprint theory: only the flat cap fails; surface theory: all
  three fail. Discriminant 539x between flat and 70 degrees. (Whole-pipeline
  parent; judged the only coupon in any parent meeting the project's own rules.)
- **C8 Ramp ladder.** Unsupported planar ramps 20-50 degrees in 2.5 degree steps,
  at 0.03 / 0.05 / 0.08 mm layers. Tests the derived 35.264 degree angle AND its
  sharper prediction that the angle does not move with layer height — the
  project's first layer-height evidence of any kind.
- **C9 Small-cap ladder.** Caps 0.10-3.14 mm², below the smallest cap ever
  printed; settles ratio-vs-threshold in the bottom decades where ordinary tips
  live.
- **C10 Vertical branch (strictly-empirical parent; judge 1's single best idea).**
  Independently varied upper-cap area, lower-cap area, and upper/lower arm
  sections, so failures distinguish LOCAL load from CUMULATIVE load transmitted
  down the model. This is the experiment that could relax §2.2's routed rule
  toward the cheaper orphan-local rule — the single most consequential physics
  question this design leaves open, and the one both ambitious parents guessed
  opposite answers to.
- **C11 Spatial allocation (strictly-empirical parent; judge 2's single best
  idea).** Unequal support positions under one cap with calibrated weak sections,
  crossing cap shape, aspect ratio, pitch, and edge distance, pre-registered so
  equal-share, nearest-cell (this design's rule), and lever-like sharing each
  predict a different first failure. Every in-engine invariant only proves the
  chosen allocation is applied consistently; C11 is the only instrument that can
  show the allocation itself wrong.
- **C12 Aspect / front width.** Long rectangular caps at matched area, varied
  length along each axis: measures engaged-area saturation and the front width,
  licensing the elongation discount §6.1 pays for today. Doubles as the first
  observation of which tank edge actually leads.
- **C2 Solid vs shell.** Matched projected area, varied mass and wall — the first
  specimens where area and volume diverge; separates weight from peel.
- **C4 Replication.** X = 22/25/28 triplets on three independently prepared
  plates, vendor and batch recorded — meets the project's own promotion rule for
  X*.

---

## 8. The census of "area per contact": nine notions, and what happens to each

The task sheet asked about five; the ground-truth lanes found nine. Disposition of
every one:

| # | Site | What it is today | Disposition |
|---|---|---|---|
| 1 | `ContactCandidate.supported_area_mm2` (lib.rs:984) | One triangle's area; read by NO decision — digests and validation only | DELETE. (The brief's "refine and supports get thinner" does not happen through it; measured refinement moves tips 17x the OTHER way, through eligibility.) |
| 2 | Same field, dyadic path (perception.rs:1145, specialized.rs:2207) | The WHOLE face's area stamped on every sub-face nomination — a latent double-count by the subdivision factor if anyone ever reads it | DELETE with #1. |
| 3 | `SelectedContact.supported_area_mm2` (lib.rs:1515) | Verbatim copy of #1 | DELETE. |
| 4 | `LoadEligibility.area_mm2` → `FinalizedFaceLoad.carried_area_mm2` | The only quantity that sizes anything; provenance-gated, whole-face, single-carrier | REPLACE: the name survives on seats and members; producer, definition, and unit of allocation all change to the charge ledger. |
| 5 | `OwnerlessLoadFace` | Discloses only ELIGIBLE faces that found no contact; the invisible 75.6% never becomes a row | REPLACE with `UncarriedDemand`, PROMOTED from disclosure to named refusal. |
| 6 | `AbandonedContactDisclosure::abandoned_area_mm2` | Load that vanishes when routing abandons a contact | REPLACE: under Rule R the cells re-charge to surviving seats; nothing vanishes. |
| 7 | `EmittedContactDisclosure.carried_area_mm2` | The disclosure the write gate reads | KEEP as the one disclosed quantity, sourced from the ledger, joined by canonical id (I11). |
| 8 | `CoverageSample.area_mm2` (dyadic.rs:416) | Tessellation-normalised sampling weight for coverage repair | KEEP through phase 3; RETIRE in phase 4, when "is the surface covered" and "is all demand charged" become the same question. |
| 9 | `IslandBirth.area_mm2` | Exact birth-slice area, computed then DISCARDED for a 1.5 mm face-ball proxy | RETIRE THE PROXY: a birth's area IS its layer's orphan footprint, already in the ledger's units. |

Net: nine notions in, ONE out — the charge ledger — plus one derived registered
constant (q) and one refusal category (`UncarriedDemand`).

---

## 9. Rulings between the parents

Each ruling names what the losing option would have bought.

- **R-a. Sizing quantity: routed worst-layer cross-section** (physics-first's
  quantity, purged of its neck-discharge flaw) **over orphan-only** (whole-pipeline)
  **and over refuse-everything** (strictly empirical). Orphan-only would have
  bought support counts matching today's field practice on limbs and spheres — at
  the price of a sizing rule that force balance refutes and that under-sizes a
  sphere 150x. The neck-discharge middle would have bought fewer heavy supports
  under self-carrying geometry — at the price of an untested transfer of the arm
  law onto model necks DECIDING that load vanishes; in this synthesis the neck
  rule survives only where it is anchored to the shipping low-point law, deciding
  WHERE (placement), never HOW MUCH (sizing). Refuse-everything would have bought
  certainty — at the price of refusing every model the owner prints. C10 is the
  standing offer to relax this ruling with data.
- **R-b. Placement: demand-driven** (both ambitious parents) **over
  template-fitting** (strictly empirical). Templates would have bought exact
  correspondence to printed evidence; they admit no geometry the owner owns.
- **R-c. The multi-carrier machinery: delete and replace, not rebuild.** Cell
  granularity leaves no face to split, so `CarriableMaximum`,
  `carriable_share_count`, `charge_face_from_takers`, `equal_shares_mm2`, and
  `SpecializedLoadBaseline::assemble`'s row-indexed one-winner representation go,
  closing tasks #68/#69/#70 — with the blast-radius judge's correction adopted:
  this is a REPLACEMENT, not a pure deletion, because that representation also
  serves the registered resource caps and the merge tournament's retention;
  `resource_cap_policy.rs` and the caps' registry notes move with it, and the
  ledger inherits the I5 obligation the old machinery discharged. Rebuilding
  in place (physics-first) would have bought continuity of the existing digests
  through phases 1-2 — at the price of generalising a representation whose own
  block comment says shearing it produces disagreeing partitions.
- **R-d. Sizing source: X_ALLOWED directly; the legacy stress form retires to a
  diagnostic.** All three parents converge here and the recommendation is
  unanimous, but it is CRAIG'S RULING to make (§10.5, R1) because it decides
  whether the change is behaviour-preserving where clamps bind. Keeping the
  legacy form would have bought zero numeric movement — at the price of the
  shipped law resting forever on four unmeasured placeholders whose agreement
  with the measured constant nothing checks in production.
- **R-e. Exactness: conservation exact by construction, tessellation invariance
  to a stated tolerance.** Bit-identity everywhere (whole-pipeline) would have
  bought the strongest possible headline and shipped a red gate on day one for a
  benign floating-point reason; a tolerance on conservation (physics-first's
  "raster quantum") would have left a same-everywhere test number deciding a
  pass. The synthesis takes exactness where construction gives it free (I1) and
  a derived tolerance where IEEE arithmetic is the only obstacle (I4).
- **R-f. Refusal over clamp.** Unanimous across parents that state it; adopted
  with placement feedback so refusal is productive (§4.2). Clamping would have
  bought never failing a run — by shipping violations, which is today's defect.
- **R-g. The self-support angle: derived 35.264 stays a prediction; 45 stays the
  gate.** Flipping now would have bought fewer admitted faces to examine — on an
  unmeasured number.

---

## 10. Blast radius and phasing

### 10.1 What is deleted, replaced, retired

Deleted: census rows 1-3 (§8). Replaced: rows 4-6, the provenance-eligibility
machinery, `LoadOwner`, the island face-count ordering, and the row-indexed load
transpose with its resource-cap policy notes (§9, R-c — the single largest
implementation cost in the design, and it carries no estimate; an implementer who
believes it outweighs everything else combined should say so before phase 2).
Retired as decisions: TARGET_SPACING_MM, MIN_CONTACTS_PER_ISLAND,
OVERHANG_ANGLE_DEG (after C8), `FrozenTipLoadPolicy` (to diagnostic). New:
the charge ledger, `UncarriedDemand`, `min_section_on_path()`, the write gate
(task #59), q = 0.25 mm registered, lambda = 1.0 registered Placeholder, sharing
cap N = 6 registered Measured, X_ALLOWED's resin-conditionality note.

### 10.2 New charged work

The field build: ~160k bitset cells and ~1200 exact reach queries per layer for a
60 mm model, one label-propagation sweep per layer, folded into per-seat
accumulators — registered and metered beside the engine's existing charges (I14).
Rule R multiplies into the repair loop (274 trials at 6.5 s median on one fixture
today, another fixture already failing to reach a verdict in 1200 s), so phase 1
publishes measured per-rebuild and per-trial cost, and the incremental ledger's
budget is part of the phase-2 gate, not an afterthought.

### 10.3 Phases — each moves exactly one class of value

- **Phase 0** — instrumentation only (E0, E5): the corpus baseline. No behaviour
  change, no digest moves.
- **Phase 1** — build the field, disclose it, let it decide NOTHING. Assert I1,
  I2, I4, I7, I10, I14 with their sabotage tests. This phase can kill the whole
  design at the cheapest possible price. No emitted value moves.
- **Phase 2** — the ledger shadows the existing partition on every fixture and
  corpus model; publish E2's per-contact divergence table. Adversarial review
  gate; owner rules on activation with the numbers in hand. No emitted value
  moves.
- **Phase 3** — sizing reads the ledger: every tip radius on every model moves;
  refusals replace clamps. Digest and baseline refreezes are walked model by
  model against the phase-2 table — never accepted in bulk (this project has
  already had a lane claim thirteen commits git did not hold).
- **Phase 4** — placement reads the ledger: contact counts move; coverage
  baselines and the 45-degree-mask telemetry move; CoverageSample retires.
- **Phase 5** — the write gate enforces I1/I3/I6/I12 on the written artifact.
  The pipeline gains its first end-to-end guarantee that what is printed obeys
  the measured law.

### 10.4 Registry honesty items that ride along

`SATURN_LEADING_RELEASE_EDGE` is downgraded from Measured to project-record until
an observation exists (C12 provides one). X_ALLOWED gains its resin-conditionality
note. The legacy placeholder trio is annotated as diagnostic-only. The Y-ruling
provenance stays what it is — an owner ruling, not a measurement — wherever the
docs currently imply otherwise.

### 10.5 Owner rulings required before phase 3

- **R1.** Size from X_ALLOWED directly, retiring the legacy stress form to a
  diagnostic (recommended by all three parents and this synthesis). This decides
  that the refreeze happens.
- **R2.** Accept the conservative routed sizing rule (§2.2) knowing it increases
  support counts and thicknesses on large compact undersides and, most visibly,
  on long horizontal limbs (§6.1) until C10/C12 license discounts. The phase-2
  divergence table presents the exact corpus-wide cost before this ruling is due.
- **R3.** Default r_ceiling: the 0.800 mm-diameter field ceiling (more, smaller
  scars) or the 2.0 mm-diameter registered maximum (fewer, larger scars).
- **R4.** Phase-4 activation, after walking the phase-2 table model by model.
