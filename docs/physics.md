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
