# Bracing and load shedding on the slice-sweep path — plan of record, 2026-09-06

Owner's asks (2026-09-05/06), in order: fix the Morel regression; a slenderness
constraint whose remedy is merging, then `|X|` angled cross-bracing (never a
horizontal span), dropping a new mast when no neighbour is in reach; and
shedding the load that piles up under a laid-over Cylinder to arms that seat
higher on the model. Owner's standing rulings that bind here: emit and warn,
never refuse; physics decides, constants only prune; spacing/density HELD;
no coupon (guess a defensible number, register it as a placeholder, let the
owner eyeball a sample); try the basic greedy approach before optimising.

Baseline: `feature/stage0-bytes-audit` @ f8656cf plus the shaft-landing work
parked on `wip/shaft-landings-2026-09-05` @ 505d413 (measured: plate contacts
roughly halve on six of ten models, Cylinder overloaded arm-layers 200 → 0,
support volume +1..+15 % because section = load/X makes volume =
Σ load × path / X and a lean is a longer path). Merging is therefore a
CONSTRAINT-satisfier (lateral stability), not a saving; that is the framing
for lane 2.

## Lane 1 — land shaft landings, fix the Morel regression

1. `remove_under_loaded` (plinth-sweep lib.rs) decides an arm is removable by
   looking only at its SEAT layer's coverage. With leaning, load re-splits and
   five more Morel arms fall under the minimum-load line; a gap then opens on
   layers BELOW the seat that the check never inspects (uncarried 0 → 23.2 mm²).
   Fix: an arm is removable only if every layer of its carried span stays
   covered by the remaining carriers (batch: re-run the reach potential on the
   candidate set once per removal pass, not per arm), and never if its removal
   raises `uncarried_load` — the down-sweep already computes that quantity.
2. Re-cut the eight tests the new rule turned red, each as a stronger claim
   under the new rule, never a pasted number: the flat-disc oracles now assert
   that every lean on a flat face lands on a shaft (never on the model) and
   that plate contact AREA is invariant to four decimals; the drain fixture is
   rebuilt so the plain field really does wall the outlet in (a denser ring)
   or the assertion is replaced by "the merged field opens the mouth and the
   keep-out opens it further"; `interior` is evaluated at the arm's LANDING
   column, not its seat column; the 2-ULP scar identity gets the same 1e-9
   relative tolerance the area guarantee already carries; the tip_seating arm
   196 case is explained (a merged, fatter tip) and the assertion re-derived
   from the arm's own radius.
3. Verify (one opus reviewer), merge the wip branch onto the feature branch.

## Lane 2 — slenderness: merge first, then `|X|` bracing, then a mast

### The constraint

An arm may not run unbraced longer than `L_max(r)`. Lateral disturbance
during printing is resin drag on the retract/return stroke and the shear of
the tilt release; the arm is a cantilever fixed at its last brace (or the
plate). For a distributed lateral load `q` per unit length on a cylinder of
radius `r` and modulus `E`, tip deflection is `δ = q L⁴ / (8 E I)` with
`I = π r⁴ / 4`, so `δ = q L⁴ / (2π E r⁴)`. Requiring the deflection to stay
under one pixel `p` (a layer printed on a swayed arm lands one pixel off):

```
L_max(r) = ( 2π E r⁴ p / q )^(1/4)  ∝ r        →  L_max = S · r
```

`S` is a pure slenderness ratio. Order-of-magnitude inputs: `q ≈ 4π μ v`
(low-Reynolds drag per length, μ ≈ 0.2 Pa·s, v ≈ 2 mm/s → 5e-3 N/m),
`p = 0.019 mm`, `E` between green (~1e8 Pa) and cured (~1e9 Pa) resin give
`S ≈ 40` (green) to `S ≈ 70` (cured). Register `SLENDERNESS_LIMIT_RATIO = 50`
as a PLACEHOLDER with this derivation and its bracket in the note; the owner
eyeballs a sample; a coupon can replace it later. The engine already
publishes per-arm slenderness (Sphere has arms near 80:1, so the constraint
will bite there first).

### Remedies, in order of cost

1. **Merge.** A lean onto a neighbouring shaft within the overhang law is a
   lateral tie. When an arm's unbraced run reaches `L_max(r)` during descent,
   the descent must choose a shaft landing if one is in reach; straight-down
   is no longer admissible for that arm past that layer. This reuses the
   shaft-landing descent as it stands; the only new input is the constraint.
2. **`|X|` brace.** If no shaft is reachable as a landing but a shaft `B` is
   within brace reach, tie `A` to `B` with two crossing diagonals, each a lean
   segment seated ON a shaft and landing on the other shaft (the descent
   machinery again: a brace is a strut whose seat is a shaft point), each
   inside the overhang law so nothing is horizontal. Brace radius: the area
   law does not size it (it carries no peel load), so it takes the minimum
   tip radius unless its own slenderness needs more: `r_b = max(r_min,
   ℓ / S)` for a diagonal of length `ℓ`. That closes the geometry: the X
   spans height `H` and gap `d` with `ℓ = √(d² + H²) ≤ S · r_b`, and the
   diagonals run no shallower than the saturated overhang angle, so
   `H ≥ d · tan θ_sat`. Braces repeat every `L_max(r_A)` up the arm. The
   brace's junctions split both shafts in the load ledger exactly as a
   landing does (`Landing::Strut{id, layer}`), carrying ~0 load.
3. **Mast.** If no shaft is within brace reach, drop a new plate-rooted arm
   at the nearest admissible plate cell within `d_max` and brace to it. The
   mast's own slenderness must hold up to the first brace, so its radius is
   `r_m = max(r_min, H_first / S)`; two masts within reach brace each other.
   `d_max` follows from the brace: `d_max = S · r_min / √(1 + tan² θ_sat)`
   at the thinnest brace, larger if a thicker brace is accepted; report the
   number it evaluates to (≈ 4 mm at `S = 50`, `r_min = 0.05`… the owner
   eyeballs it).
4. **Warn.** If neither is possible (keep-out, no plate in reach), emit with
   a per-arm `unbraced_over_limit` warning; never drop the arm.

### As reworked, 2026-09-29 (owner rulings on the 2026-09-28 field report)

Recorded here because it changes the ladder above; the code of record is
`crates/plinth-sweep/src/slenderness.rs` and the field report is
`plinth-evidence/.../field-2026-09-28/braces-masts.md`.

- **Order:** merge → un-merge pass → `|X|` → **thicken** → mast → warn. The
  un-merge pass that puts back merges a trunk cannot carry now runs BEFORE
  any tie, so no post it puts back is left unmeasured.
- **Every direction:** an `|X|` holds only along its own plane. A cross is
  credited with the sway directions within 45° of its plane (where it resists
  at least as much of the sway as it lets through); a member is measured in
  every direction its crosses tell apart, so a tie made of crosses needs
  planes whose ±45° arcs cover all 180° (two perpendicular planes, or three).
  Crosses are built one at a time; one that a later cross or the member's own
  thickening leaves unneeded is taken out at the end of its pass.
- **Thicken:** raise the root, keep the tip, up to the 1.0 mm maximum; the
  root is the smallest that brings the run inside the one-pixel law, carried
  down every trunk below, and a grown plate root must clear every other root
  disc.
- **Runs are measured on the drawn profile** with the taper integral
  (`δ/p = (4/S⁴) ∫ (L−x)³ / r(x)⁴ dx`), not the narrowest ledger radius.
- **Live re-measure:** each member's runs are cut again from the live field
  before every remedy; every partner in reach is tried, nearest first; a
  cross that a later one leaves unneeded is taken out at the end of its pass;
  the passes repeat (with a down-sweep between) to a fixed point.
- **Masts:** a cell is admissible only where the braced panel holds the arm to
  one pixel in its plane (the frame of the field report's Q4); an arm's mast
  is extended to a later tie before a second one is dropped.
- `S` stays 50 (until 2026-10-01; see below).

### Member clearance, 2026-09-29 (fix/member-clearance; `brace-collide-diagnosis.md`)

- **Drawn solid, not centreline.** A brace diagonal and a mast are tested as
  the solid they are drawn: radius `r` about the straight axis must clear the
  cured model by the registered final gap (0.3 mm, 2026-08-22) plus the
  raster's half cell diagonal, end layers included, and must not enter any
  member it is not joined to (axes `≥ r₁ + r₂` apart). The centreline-cell
  test stays as the cheap first refusal. Code: `crates/plinth-sweep/src/member_clearance.rs`.
- **Measured on the drawn spans (round-3 review, 2026-09-29).** A standing
  member's radius varies, so the closest pair of axes is not the closest pair
  of solids. The standing member is measured span by span as the lowering
  draws it (`plinth_support::drawn_spans`, the one rule), each span read
  round-ended at its wider end radius; a span is a chord and can stand wider
  than the ledger at a slot it bridges. Counted: `member_refusals_beyond_the_closest_axes`.
- **Crosses at one layer are credited together:** planes `θᵢ` at one layer
  hold `φ` when `Σ cos²(φ − θᵢ) ≥ ½` (one cross gives the ±45° arc above; two
  planes 60°–120° apart hold every direction). A further plane goes to a band
  top the member already has when that layer alone cuts the run inside the
  limit.
- **Masts:** never thicker than the member they brace; the cell needing the
  smallest mast wins (nearest among equals).
- **Thicken:** refused for its root disc only when the growth laps a root it
  did not lap before (orchestrator ruling 2026-09-29).
- **Seat trim:** a record the trim would empty keeps only its part outside the
  model; one wholly inside the model is left out of the file and counted.
- Arm shafts are NOT yet clearance-checked against the model (open).

### As reworked, 2026-10-01 (Craig's rulings; `stilt-and-brace-pair-diagnosis.md`, `brace-pair-resin.md`)

- **`S` = 30**, interim, pending a slenderness coupon (ChituBox practice
  braces at about 13 × r). The registry note keeps the drag-only derivation
  and says why the value now sits below its bracket.
- **Keep one-direction crosses.** A cross that ties two columns holds the
  sway in its own plane; thickening or a mast covers the direction across
  it. The prune never takes a cross out because a thickening also holds: a
  cross the first test finds unneeded is judged again on the posts as they
  stood before the pass's thickenings, in the directions the cross itself
  holds. A cross another cross or a landing makes redundant still goes.
  Counted: `crosses_kept_in_their_own_plane`.
- **Tie timing.** A branch that lands on a host holds the host only from the
  layer its own seat prints. A run is the cantilever from the highest tie
  that holds while a layer prints up to that layer, so the host is measured
  over (tie below, branch seat) and then (landing, next end). Brace landings
  are still credited at once (open question: the same physics applies over
  a band's depth). Counted: `landing_ties_held_late`.
- **Brace-band refusals by reason** (`BandRefusals`): posts at one cell,
  below the plate, in a root flare, leaving a post, too flat for the overhang
  law, end ring under the plate, centreline through a drain / the model / a
  member, solid within the final gap of the model / a member. The reasons sum
  to `brace_bands_refused`; the log prints them.

### Fixtures (new, procedural, in plinth-mesh fixtures.rs)

- `make_tall_stilt_plate`: a plate 30 mm up on a 4 mm slab — every arm is
  long and thin; asserts every arm's unbraced run ≤ `L_max(r)` after the
  lane, with braces or merges accounting for the difference.
- `make_lonely_peak`: a single small overhang far from anything else — forces
  the mast path (no neighbour within brace reach); asserts one mast dropped,
  braced, within `d_max`.
- `make_brace_pair`: two isolated overhangs at gap `d < d_max` — forces the
  `|X|` path; asserts two crossing diagonals, both inside the overhang law,
  no horizontal segment.
- Sabotage per remedy: disabling merging must produce braces; disabling
  braces must produce masts; disabling masts must produce warnings, never
  fewer arms.

Emit: diagonals and masts are ordinary struts (taper/shaft records); no new
record type. Measure: Sphere (the 80:1 arms), Round 25/50/100 (should be
untouched: short arms), Cylinder, Morel base, the three fixtures.

## Lane 3 — shed the Cylinder's bottom load to arms seated higher up

Why the bottom is a solid mass: a laid-over cylinder's whole peel load is
routed down through its own body to the line where it is lowest, and the
area law puts `peak / X` of arm section there whatever the arms' arrangement.
Merging cannot thin it (the section is invariant). Only two things can:
(a) put some of that section on the plate OUTSIDE the silhouette, spread
out; (b) route part of the load to the plate by a different path — arms
seated higher on the body (the flanks, 30–60° below the equator, where the
cover is wide) that lean outward to the plate. Both need leaning, which now
exists.

Rather than a literal third sweep, use the iterate the engine already has —
the load-driven insertion rounds (`InsertionPolicy::max_rounds`) — with a
second trigger beside overload: **congestion**. At the plate layer, rasterise
every plate-rooted arm's root disc; congestion = Σ disc area / area of the
union (>1 means overlap). (Corrected 2026-09-29, review-fixes: a root disc is
the flare of the root the member is DRAWN with — its ledger raised by the
junction clause where a brace joins — by the lowering's own rule,
`plinth_support::drawn_root_radius_mm`; the slenderness lane grows its discs
as crosses raise roots.) When a footprint is congested:

1. Let the congested arms' descents lean OUTWARD to plate cells outside the
   silhouette (the deferred-path charge already makes plate landings win
   over model stubs), so the same section spreads over a wider patch.
2. Insert new arms seated on the body's flanks at the highest layer whose
   cover admits the hex pitch (the layer where the body is widest below the
   equator is the natural shelf), descending outward to the plate; the
   down-sweep's routing then splits each layer's event among ALL carriers
   below it by the existing spreading rule, so load moves to the new paths
   without a new allocation law.
3. Re-run the down-sweep; stop when congestion ≤ 1 or the rounds are
   exhausted; publish `congestion_before/after`, arms inserted for
   congestion, and the section that left the footprint.

Fixture: the Cylinder as it stands (laid over) and a new `make_laid_tube`
(a horizontal cylinder on two feet, so orientation cannot dodge it).
Acceptance: no two root discs overlap at the plate on the Cylinder; total
plate contact area unchanged within the area law; uncarried load not worse.

## Order and gates

Lane 1 → Lane 2 → Lane 3. Each: tests + sabotage, one opus reviewer,
measured set for the owner in `../plinth-evidence/slice-sweep-2026-09-02/
<lane>/for-craig/`, byte-diff against the previous set with every change
explained. One heavy process at a time on the owner's laptop.
