# Physics

The support engine sizes structure from a failure boundary that was measured on printed
calibration plates, not from a heuristic. This document is the account of what is modelled,
what was measured, and where the model stops being valid.

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
carried on a tip of known diameter — printed and then pulled.

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
X_ALLOWED = floor(25.00 / 2) = 12
```

so every arm satisfies `A_arm ≥ A_supported / 12` — the measured boundary with a factor of
two held back. A second, independent plate using constant-diameter cylindrical arms rather
than tapered cones bracketed the boundary at `X* ∈ (19.91, 30.48)`, which leaves the
allowance **1.66×** of margin against the pessimistic end of an experiment it was not
derived from. That is a cross-check, deliberately not a second derivation input.

### Three findings that shape the design

**(a) Load scales with supported *area*.** Not with perimeter, and not with a squeeze-film
law. The plate deliberately crossed cap area against tip diameter; a perimeter or
squeeze-film law would not collapse onto a single `A_supported / A_arm` threshold across
three cap areas and a 25× span of tip diameters. It did.

Published work on *plate-level* separation force argues for a sublinear exponent in
cross-section. That is a different experiment, at cross-sections one to two decades larger
than a support tip, and extrapolating it down here is not warranted. The direct measurement
governs the sizing path.

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

**Peel direction.** The target machine is a tilt-release printer: the tank tilts, the release
front sweeps across the plate rather than lifting straight off, and it has a leading edge. A
model's position along the sweep axis therefore changes its peel load. The measured
sensitivity is small — around one rung of the test ladder from front to back — so the
guidance is to **randomize over that axis rather than tune it**, and to spend replication
budget on build-to-build variation instead. There is no orientation term in the sizing path
today; the input is plumbed and unused.

## 5. The seat and contact model

A support tip that merely kisses the model is not attached. Every model contact is **seated**
to a computed depth, and the geometry is then evaluated on the seated pose.

**Seat depth** takes the largest of three requirements, each binding in a different regime:

```
h = max( default_contact_depth,
         process_resolution + r_end · sin(theta),      full-cross-section engagement
         BALL_SEAT_FACTOR · r_tip )                    ball-tipped ends only
```

- *Slicer resolution.* A fused region thinner than one layer is not resolvable by the
  printer, so the layer height sets a floor. It is a datum the run already carries, not an
  invented number.
- *Full-cross-section engagement.* An arm meeting the surface at incidence `theta` has its
  end disc tilted; only `h ≥ r_end · sin(theta)` engages the whole end cross-section rather
  than smearing a partial bond.
- *Ball-tip cap geometry.* The plane at depth `h` cuts an emitted tip ball in a disc
  `a² = 2·f·r_tip·h − h²`. Requiring that cut not to be the narrowest section (`a ≥ r_tip`)
  gives `BALL_SEAT_FACTOR = f − sqrt(f² − 1) = 0.3819660112501051` at `f = 1.5` — derived in
  code, never hard-coded. A shaft exists at all only while `r_tip < 0.9549150281252627`;
  above that the stub is ball-dominated and the clause becomes an interface-area maximiser,
  which gives the same `h`.

**One pass, no iteration.** Seating changes the incidence, which raises the question of
whether a depth computed at the unseated pose is still valid at the seated one. It is, and
provably: seating raises *both* `|cos(theta)|` and the length, so it can only over-seat,
never under-seat. That is what licenses a single pass.

**Clearance mask.** The arm must not foul the model along its approach:

```
u* = ( h + r_max + standoff + MARGIN ) / |cos(theta_seated)|
```

with `r_max` the larger end radius and `MARGIN` the process resolution. This solves the
criterion the geometry kernel actually evaluates — a sphere-swept capsule against the
surface — rather than a tilted-end-disc approximation of it. The `h` term appears only for
members that are genuinely seated.

**Emergence.** A contact is admissible only if `u*_terminalward + u*_rootward ≤ L − min_len`.
Both floors are parameter-free consequences of the geometry: a member that never emerges from
the standoff band is a rod lying inside the mini, which after union disfigures it rather than
supporting it; and a free span shorter than the minimum cannot be given a legality verdict at
all. **There is no fixed incidence-angle rule** — the effective limit falls out of the radii,
the seat depth, the standoff and the length, and differs for every candidate.

**Area sufficiency** is evaluated on the *minimal separating cut* — the ball's cut disc for a
ball-tipped end, the end radius otherwise — against the same registered `X_ALLOWED`. There is
exactly one area-ratio allowance in the engine and every check reads it. A second hand-typed
copy of the same number is not redundancy but a divergence hazard: a future recalibration
would move one and silently leave the other stale.

**Attestation, not assumption.** Every tip that mints a model contact must be *proven*
attached across its required contact area, by witnessing rays that penetrate from the
contact. That inference is exact for locally flat or convex seats and a sampling bound
otherwise; deeply concave seats are unmeasured. A tip that engages nothing is quarantined
rather than shipped.

**Buried surface is refused.** Real mini meshes are watertight and manifold yet
self-interpenetrating, so a support foot can land on a surface patch that lies *inside* the
printed solid. Such a foot passes naive verification trivially — its whole neighbourhood is
interior. An explicit exterior-ness screen catches that case, and refusals route to
quarantine rather than failing the run.

## 6. Where the model stops being valid

Stated plainly, because the numbers are quotable and the boundaries are not obvious:

- **One printer, one resin class.** Every measured constant comes from plates printed on a
  single tilt-release machine in a specific resin. Another machine or another resin needs its
  own plates.
- **Coupon to interface.** The boundary was measured on coupons whose arm *and* head were
  both support material. Applying it to a support-to-model interface crosses three
  boundaries: the interface is a virtual cut through one continuous cured solid rather than a
  machined section (defensible — in resin printing the union is monolithic, there is no bond
  line, so a minimal-cut criterion is the right generalization of "narrowest section"); the
  loaded material is model resin rather than support resin (same resin, same green state);
  and the coupon was loaded in tension while the interface sees mixed tension and shear at
  incidence. The factor of roughly two to three between `X = 12` and the measured boundary is
  what absorbs that mode difference. No tension/shear interaction criterion is attempted,
  because the ratio is unmeasured for green resin — that is where the next calibration plate
  should be spent.
- **Concave seats are a sampling bound**, not a proof.
- **Most constants are not measured.** The registry currently holds **287** values: **20**
  measured, **11** derived from measured ones, and **256** placeholders. That ratio is the
  point rather than an embarrassment — see below.

## 7. Every constant carries its provenance

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

## 8. Resources: counted, then charged, then spent

A real mini is two orders of magnitude larger than a test fixture, and an algorithm that is
accidentally quadratic will happily consume a machine. The engine's answer is not a timeout.

**Work is counted before it is charged, and charged before it is spent.** Each stage states
what a batch will cost, in explicit units, before it allocates anything. That statement is
checked against a cap. If it does not fit, the stage returns a **typed refusal naming the
resource, the requirement and the limit** — it does not silently reduce quality, drop
candidates, or approximate. A refusal is a result you can act on; a degraded answer is one
you cannot even detect.

Where a plan must be charged for more than it turns out to execute — because no plan can know
at plan time which branches a search will take — the charge and the execution are **severed
and both reported**, with the execution metered against the charge. Charging the admissible
worst case is honest; quietly executing more than was charged is not.

**A cap's job is to refuse work that is superlinear in model size, not to refuse large
models.** A flat constant conflates the two: it fires on honest linear work at 400k faces and
lets a genuine quadratic through at 24 faces. So the caps that govern per-face work take the
shape

```
cap = max(floor, per_face × face_count)
```

with the coefficient derived from measurement by one stated rule rather than picked. `k·N` is
still a bound: it refuses anything worse than linear, which is exactly the runaway each cap
exists to catch.

When a cap fires, the remedy is to **make the bound honest or fix the algorithm** — not to
raise the cap. A cap raised to admit a quadratic has stopped being a cap.

## 9. Determinism and numeric contracts

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
[`development.md`](development.md).
