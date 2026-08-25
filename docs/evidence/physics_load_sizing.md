> **Evidence record — non-normative.** Relocated from `docs/engineering-journal/reference/physics_load_sizing.md` in the 2026-08 docs restructure; content preserved verbatim below.

# Physics audit: load_sizing (v4 CCO resin support engine)

Repo: resin-print-prep, master f02b7af.
Scope files: `verify.py`, `tree.py`, `cco.py`, `config.py`, plus
`placement.py` and `transient.py` (the actual homes of `_shaft_r_for_L`
and `r_transient_mm`, which `tree.py`/`cco.py` only call — noted since
the task description places "r_deflect" under tree.py but it does not
live there in the current source layout).

All line numbers below refer to files under
`scripts/support_placement/` at master f02b7af.

---

## 0. Forces modeled — headline answer

**Only one force is modeled: PEEL** (a static pressure over newly-cured
area, resolved into either axial tension at a tip/trunk or a lateral
bending load on an inclined shaft). There is **no self-weight, no
gravity, no buoyancy, no density, and no viscosity term anywhere in the
codebase.**

- Grep across every `.py` file in `scripts/support_placement/` for
  `density|gravity|9\.8|self_weight|kg/m|g/cm|buoyan|viscosity` turns up
  zero physics uses (`config.py:65/67/194` and `corpus.py:8/95` use
  "density" to mean *spatial sampling density* of contacts, not mass
  density; `struts.py:36`'s "fighting gravity" is a comment about routing
  economy, not a computed force).
- The companion charter `design.md` states the modeling decision in
  prose (`design.md:26-27`): *"Dominant load = PEEL: a few kPa over each
  region's newly-cured area, arriving as TENSION in that region's
  supports. Gravity ~cancelled by buoyancy."* This is an explicit,
  unverified simplifying assumption baked into the model at the
  documentation level — no code computes a self-weight term, checks
  whether buoyancy actually cancels it for the actual (unknown) resin
  density, or flags the assumption as suspect for any particular
  geometry (e.g. a long horizontal-ish cantilever where the moment arm
  of self-weight could matter even if net vertical force is small).
- No resin **density** (g/cm³) constant exists anywhere in `config.py`.
  Support mass, model mass, and buoyant displacement are never computed.
  This means real resin density data (the parallel team's ~1.05–1.2
  g/cm³ figure) **cannot currently calibrate anything in this engine** —
  there is no formula for it to feed into. See §3 item 6.

Everything else in this report characterizes the PEEL-only static load
model: how peel pressure becomes force, how force becomes radius (three
distinct, not-obviously-reconciled power laws), and the various
non-physics floors that often dominate the "real" answer anyway.

---

## 1. Complete table: constants and formulas

### 1a. Scalar physical constants (`config.py`)

| Name | Value | File:line | Units | Provenance | Depends on (resin property) |
|---|---|---|---|---|---|
| `peel_pressure_pa` | 3000.0 | config.py:23 | Pa | **Assumed**, explicitly flagged rough estimate. Docstring claims "geometric mean of a plausible 1–5 kPa range" — this is wrong arithmetic (see §4 issue 4); 3.0 is the *arithmetic* mean of 1 and 5, the geometric mean is ≈2.24 kPa. | peel stress |
| `tensile_green_mpa` | 10.0 | config.py:29 | MPa | **Assumed**, conservative (weak) end of design.md's stated 10–25 MPa green-tensile range | green tensile |
| `modulus_green_gpa` | 0.5 | config.py:30 | GPa | **Assumed**, conservative (weak) end of design.md's stated 0.5–1.5 GPa green-modulus range | green modulus |
| `tip_safety_factor` | 8.0 | config.py:34 | dimensionless | **Assumed/magic** — design.md gives SF 5–10; 8 is a policy choice ("weighted toward the safe side"), not a measured number | none directly — multiplies tensile |
| `tip_r_min_mm` / `tip_r_max_mm` | 0.2 / 1.0 | config.py:39-40 | mm | **Magic number** — explicitly documented as a *hobbyist-handling* constraint ("do not derive it"), not physics | none |
| `shaft_drag_force_n` | 0.01 | config.py:50 | N | **Assumed**, "order-of-magnitude guess," explicitly flagged rough estimate; constant for *every* member regardless of its own radius, length, or orientation | none — would need viscosity + lift velocity, neither modeled |
| `max_deflection_mm` | 0.15 | config.py:54 | mm | **Assumed/magic** — a serviceability (visual-ribbing) threshold with no stated numeric derivation anywhere in config.py or design.md beyond prose | none (subjective/visual criterion) |
| `max_unbraced_shaft_r_mm` | 0.9 | config.py:60 | mm | **Empirical/derived**, but from the *corpus* (prior successful Chitubox-profile prints), not from resin properties: "set near the corpus's realized Heavy shaft radius (0.75mm)" | none |
| `min_shaft_r_mm` | 0.3 | config.py:61 | mm | **Magic** floor, no stated derivation | none |
| `min_trunk_r_mm` / `max_trunk_r_mm` | 0.8 / 2.4 | config.py:220-221 | mm | **Magic**, "removal-friendly band" | none |
| `min_shoot_r_mm` | 0.35 | config.py:223 | mm | **Magic** floor | none |
| `brace_free_span_mm` | 18.0 | config.py:225-237 | mm | **Derived from one measured example** (a 65mm bare column at r=1mm deflecting 2.3mm, ~15x the `max_deflection_mm` budget) — real measurement, but of a single case, then generalized | indirectly modulus_green_gpa via `_shaft_r_for_L` |
| `layer_height_mm` | 0.05 | config.py:346 | mm | **Measured** (Saturn 4 Ultra @ 50µm printer profile) | none (printer, not resin) |

### 1b. Formulas (`placement.py`, `cco.py`, `transient.py`, `verify.py`, `tree.py`)

| Name | File:line | Formula | Units | Physical law encoded | Resin dependency |
|---|---|---|---|---|---|
| `tip_radius_mm` | placement.py:113-122 | `r = sqrt(F·SF / (π·σ_tensile))`, `F = carried_area_m2 · peel_pressure_pa` | m→mm | Axial tensile failure with safety factor: `σ = F/(πr²) ≤ σ_tensile/SF` | peel_pressure_pa, tensile_green_mpa, tip_safety_factor |
| `_r_tension_mm` | cco.py:118-128 | identical to above, **unclamped** | mm | Same axial-tension law, used as a trunk/shoot load floor, not just the tip fuse | same three |
| `_shaft_r_for_L` | placement.py:130-153 | `δ = 4FL³/(3Eπr⁴) ⇒ r=(4FL³/(3Eπδ))^(1/4)`; angle-aware: `F_eff = F_drag + peel·π·r²·tan θ` (fixed-point iterated) | m→mm | Cantilever point-load elastic deflection (`δ=FL³/3EI`, `I=πr⁴/4`) — a **stiffness/serviceability** limit, not a strength limit | modulus_green_gpa, shaft_drag_force_n, max_deflection_mm; peel_pressure_pa only in the angle-aware term |
| `max_unbraced_len_mm` | placement.py:156-168 | closed-form inverse of the above (`L` from known `r`) | mm | Same law, solved the other way | same as above |
| `shaft_radius_for_length_mm` | placement.py:171-191 | iterative brace-count search wrapping `_shaft_r_for_L` | mm, count | Not new physics — search over "how many braces to hit the unbraced-radius cap" | same as `_shaft_r_for_L` |
| Murray propagation | cco.py:618-666, esp. 654 | `r_new = max((r0⁴+r_child⁴)^(1/4), r_deflect, r_tension, kind_floor)` | mm | A bookkeeping convention from the α=1/2 branched-transport model (design-v4.md §5): **not** literal biological Murray's law (which is r³ for laminar-flow drag minimization) despite the name | none directly — combines whatever radii the children already have, from whichever formula set them |
| `murray_report` | verify.py:143-169 | `r_parent⁴ ≥ Σ r_child1⁴ · (1−tol)` | mm⁴ | Sanity/self-consistency check on the *same* r⁴ convention `_propagate_murray` already enforces during construction | none — see §4 issue 3 |
| `r_transient_mm` | transient.py:454-461 | `r = _shaft_r_for_L(worst_span, lean) · N_terminals^0.125` | mm | Deflection sizing over the worst build-time cantilever span, scaled by a ported v3 heuristic (net exponent 1/8 = 0.5×0.25, from "size by sqrt(N) then further ^0.25") | modulus_green_gpa, shaft_drag_force_n (via `_shaft_r_for_L`); N_terminals is topology, not a resin property |
| `resize_forest` r_req | transient.py:493-620, esp. 610 | `r_req = max(r_murray, r_tension, r_deflect, r_transient, kind_floor)` | mm | The full design-v4.md §6 formula, **minus `r_bend`** (see §4 issue 5 — not implemented) | union of the above |
| `load_report` realized SF | verify.py:172-189 | `σ = F/(πr²)`, `SF_realized = σ_tensile/σ`, flag if `SF_realized < tip_safety_factor` | dimensionless | Sanity check: did tip-radius clamping (0.2–1.0mm band) leave any contact under-strength | peel_pressure_pa, tensile_green_mpa |
| `volume_report` | tree.py:515-537 | frustum `V=πL(r0²+r0r1+r1²)/3000`, ball `V=4πr³/3000` | mL | Exact solid geometry (no physics assumption) | none |
| `printability_report` | tree.py:540-589 | `max(0, drift+taper) ≤ tan θ_max(r)` | dimensionless | Geometric overhang/layer-continuity rule, not a load model | none (θ_max tiers are calibration-hook, not resin-derived) |

---

## 2. Per-formula assessment: law, dimensional consistency, physics quality

**`tip_radius_mm` / `_r_tension_mm` (axial tension + SF).**
Law: simple axial stress with a safety factor. Dimensional check:
`F·SF/(π·σ)` → `N/(N/m²) = m²`; `sqrt(...)` → `m`. **Consistent.** Physics
quality: reasonable and standard for a tension fuse; the peel force
itself (`area × peel_pressure`) assumes peel pressure acts uniformly
over the *Voronoi-partitioned down-facing area*, which is a defensible
first-order model but ignores any stress concentration at the tip/model
interface (the actual failure geometry — a small necked contact patch —
is not analyzed; the formula treats the *tip's own* cross-section as
carrying the *entire region's* peel force in pure tension, i.e. no shear
or bending at the joint is considered).

**`_shaft_r_for_L` (cantilever deflection).**
Law: standard point-load cantilever deflection with circular
cross-section (`I=πr⁴/4`). Dimensional check: `4FL³/(3Eπδ)` →
`N·m³/((N/m²)·m) = m⁴`; `^(1/4)` → `m`. **Consistent.** Physics quality:
the base (θ=0) form is textbook-correct. The angle-aware peel term
(`F_eff += peel·π·r²·tanθ`) is a plausible projected-area argument
(ellipse area `πr²/cosθ`, resolve peel force onto the member's lateral
axis via `sinθ`) — algebra checks out (`πr²/cosθ · sinθ = πr²tanθ`) —
but it is a *self-referential* correction (depends on the member's own
r, not on carried load/area at all) and it is the **only** place
`peel_pressure_pa` enters deflection sizing; the base drag term
`shaft_drag_force_n` is a flat 0.01N applied identically to a 0.35mm
shoot and a 2.4mm trunk, with no dependence on member size, orientation,
or how much load it carries. That is a real oversimplification: real
lift-drag on a shaft in viscous resin should scale with wetted
area/frontal profile and lift velocity, neither of which is modeled.

**Murray propagation (`r⁴` conservation).**
Law: **not** biological Murray's law (that's `r³`, derived from
minimizing viscous dissipation + material cost in laminar pipe flow).
It's a restatement of the branched-transport convention `r ∝ mass^(1/4)`
chosen in design-v4.md §1/§5 for the *cost-search heuristic*
(`α=1/2 ⇒ radius exponent 1/4`). Dimensionally, `mm⁴` isn't a
recognized conserved physical quantity (unlike, say, mm² for a
flow-continuity argument) — it's a self-consistent bookkeeping choice,
not an independently-derivable physical law, and design-v4.md is
explicit that it's a floor that "may sit ABOVE conservation, never
below" once other floors (tension, deflection, kind) win the `max()`.
**The dimensional-consistency question that actually matters here is
whether the r⁴ convention is compatible with the OTHER formulas that
set real radii** — see §4 issue 2, which is the most consequential
finding in this audit.

**`r_transient_mm` (N^0.125 scaling).**
Ported verbatim from v3 with a documented "net exponent" derivation
(0.5 then further 0.25 = 0.125) but that derivation is about *how many
downstream terminals a column serves*, not a resin property — it's a
topology/counting heuristic layered on top of the (resin-dependent)
`_shaft_r_for_L` deflection solve. Not independently dimensionally
checkable (it's a bare power on a dimensionless count), but its physical
justification ("more terminals ⇒ effectively worse transient load") is
plausible-but-unvalidated, same status as the rest of the transient
model (`peel_pressure_pa`/`shaft_drag_force_n` "remain rough," per
design-v4.md §8, and this formula inherits that flag).

---

## 3. Resin-sensitivity ranked list

*(Written for evaluation against real ABS-like water-washable resin
data — density ~1.05–1.2, green tensile far below the ~30–50 MPa
post-cure figure, green modulus likely well under 1 GPa. I have not
fetched or assumed any such numbers here — only computed what the code
would DO if fed different ones.)*

1. **`tensile_green_mpa` (10 MPa, config.py:29) — highest safety-critical
   sensitivity.** Every tension-floor radius (`tip_radius_mm`,
   `_r_tension_mm`) scales as `r ∝ 1/√σ_tensile`. If the real
   water-washable green tensile turns out to be materially *below* 10
   MPa (plausible — the user's own framing puts post-cure at 30–50 MPa,
   and green-vs-cured ratios in design.md's own numbers are roughly
   3–5x weaker), every tip and every load-floored trunk/shoot in the
   current run is **undersized relative to the code's own safety
   factor**, not just "conservative by a different margin." This is the
   one number where being wrong changes correctness (print failure
   risk), not just resin-cost bookkeeping.

2. **`peel_pressure_pa` (3000 Pa, config.py:23) — highest volume-cost
   sensitivity.** Same `r ∝ 1/√·` relationship, but inverted (higher
   peel pressure ⇒ fatter tips/floors). Governs the load side of every
   tension-floored member and the angle-aware term in `_shaft_r_for_L`.
   design.md only claims "a few kPa" as a qualitative range; this is
   the constant most likely to be *measurable* from real resin/printer
   data (peel force during separation is something a load cell on the
   printer, not the resin datasheet per se, would give) — flag that the
   parallel team's chemical-property data (density/tensile/modulus)
   will NOT directly calibrate this one; it needs a mechanical/process
   measurement, not a materials one.

3. **`modulus_green_gpa` (0.5 GPa, config.py:30) — muted sensitivity.**
   Enters `_shaft_r_for_L` as `r ∝ E^(-1/4)`; the fourth root damps the
   effect of even a large error (a 2x error in E only moves r by ~19%).
   Still worth checking against real green-modulus data, but this
   engine's output is much less sensitive to it than to tensile or peel
   pressure.

4. **Resin density — currently ZERO sensitivity (not modeled at all).**
   See §0. No formula in this codebase consumes a density value. If the
   parallel web team returns a density figure, it cannot change any
   current conclusion of this engine unless someone *adds* a self-weight
   term. Worth flagging back to the team: density data is orthogonal to
   this dimension's findings as the code stands today, but is exactly
   the input needed to *validate the "gravity ~cancelled by buoyancy"
   assumption* design.md states in prose and never checks (buoyancy
   cancels weight only while the piece is submerged and near
   density-matched to the liquid resin — post-cure or during a lift
   stroke the story may differ, and nothing here tests it).

5. **`tip_safety_factor` (SF=8, config.py:34) — indirect, policy-level
   sensitivity.** Resin *mean* property values don't calibrate SF
   directly; resin *variance/scatter* data would (if water-washable
   ABS-like resin shows more batch-to-batch or cure-time variability in
   green tensile than the 10–25 MPa range design.md assumed, SF=8 may
   be inadequate regardless of what the mean tensile turns out to be).
   Flag this as a "need variance, not just point estimate" item for the
   parallel team if such data exists.

6. **`shaft_drag_force_n` / `max_deflection_mm` — not resin-calibratable
   at all with the data the parallel team is fetching.** These are
   process/geometry constants (lift-drag force, subjective ribbing
   threshold) that no chemical resin datasheet number can validate;
   they need print-outcome or load-cell measurement, exactly as
   design.md's own "Calibration epistemics" section already says.
   Listed here only so the parallel team doesn't spend effort trying to
   back these out from resin chemistry data — it can't be done.

7. **Murray/branched-transport α=1/2 exponent and the r⁴ convention —
   not a resin-calibratable quantity at all.** It's a topology-search
   heuristic (design-v4.md §1 says so explicitly: "the search heuristic
   only"). No real resin number changes it. Lowest priority for the
   parallel team's data, but see §4 issue 2 for why its own *internal*
   justification is shaky independent of resin data.

---

## 4. Suspected issues (ranked)

**1. HIGH — `shaft_drag_force_n` is a single global constant (0.01N)
applied identically to every member regardless of size/orientation.**
`config.py:50`; consumed at `placement.py:130-153` (`_shaft_r_for_L`)
and everywhere that function is called (dozens of call sites in
`cco.py`, `transient.py`). No viscosity, lift velocity, or
member-frontal-area term exists. A 0.35mm shoot and a 2.4mm trunk see
the same base lateral drag force. Severity: real — this is the *primary
determinant* of deflection-floor radii for any member where tension
doesn't dominate (i.e., most of the tree away from tip fuses).
**Confirming experiment:** instrument a real lift cycle (high-speed
video or a load cell on a jig) and regress observed lateral force vs.
member radius/length/orientation across several shaft sizes; check
whether force scales with r, r², or is genuinely ~constant as the code
assumes.

**2. HIGH — the charter's foundational α=1/2 derivation
("deflection sizing gives r ∝ load^(1/4)", design-v4.md:36) is not
realized by any implemented formula.** `_shaft_r_for_L`
(placement.py:130-153) takes **span length** and a **fixed** drag force
as its primary inputs — it does not take carried load/area at all in
its base (θ=0) form, and only self-referentially (via the member's own
r, not the carried mass) in the angle-aware term. The formula that
*does* map carried load → radius in this codebase, `tip_radius_mm` /
`_r_tension_mm` (placement.py:113-122, cco.py:118-128), gives
`r ∝ load^(1/2)`, not `load^(1/4)`. `r_transient_mm`
(transient.py:454-461) gives yet a third relationship,
`r ∝ N_terminals^(1/8)`, based on a terminal *count*, not carried area.
Since `resize_forest`/`_propagate_murray` take the `max()` of all three
plus the r⁴ Murray floor, **whichever formula wins locally determines
the effective load-radius exponent**, and it is frequently the
`load^(1/2)` tension formula (peel-dominated regions) or a flat kind
floor, not `load^(1/4)`. This directly undermines the stated
justification for the Xia/branched-transport α=1/2 exponent that
drives the junction-angle math (`cco.py:27-53`, the 90°-children /
per-branch-turn-angle predictions) and the merge-incentive argument in
design-v4.md §1. It may be that the charter means something more
abstract by "load" here (network-topology-correlated span length rather
than literal carried mass), but as written the two are conflated.
**Confirming experiment:** across a real engine.run's member set,
log-log regress each member's *built* radius (the actual winning term
in the `max()`) against its own `carried_mm2` and separately against
its downstream terminal count; check which exponent (0.25, 0.5, 0.125,
or "flat/floor-bound") actually describes the bulk of the tree's
radii, and compare against the 0.25 the theory assumes.

**3. MEDIUM (mostly documentation risk) — `verify.murray_report`
(verify.py:143-169) is not an independent physics check.** It re-checks
the exact `r⁴` bookkeeping convention `cco._propagate_murray`
(cco.py:618-666) already enforces at construction time, using the same
formula. A genuine "Murray violation" can therefore only come from a
bug in the propagation bookkeeping itself (the file's own comments
document one such bug already found and fixed — the "B2 fix" at
cco.py:626-645, a `r0` vs `max(r0,r1)` convention mismatch that
inflated combined radii 2.3x before the fix). This is fine as a
self-consistency ratchet, and the docstrings are honest that floors
push results "above conservation, never below" — but nothing should
describe a passing `murray_report` as validating a real physical
conservation law; it validates that the code's own bookkeeping is
internally consistent. **Confirming check:** none needed against resin
data — this is a code-hygiene note, already partially self-documented.

**4. LOW — `peel_pressure_pa`'s docstring math is wrong.**
`config.py:20-24`: "3.0 kPa is the geometric mean of a plausible 1–5
kPa range." `sqrt(1×5) ≈ 2.236` kPa, not 3.0 — 3.0 is the *arithmetic*
mean. Doesn't change the constant's value or the engine's behavior, but
undermines confidence that the number's stated derivation method was
actually applied. **Confirming check:** `python3 -c "print((1*5)**0.5)"`
→ 2.236, vs the stated 3.0.

**5. LOW/EXPECTED — `r_bend` (design-v4.md §6, "eccentricity term for
curved spans") is specified but not implemented anywhere.** Grepped
`tree.py`, `cco.py`, `transient.py`, `placement.py`, `verify.py` for
`r_bend` — zero hits. `transient.py:520` even names it explicitly in a
comment describing the full charter formula
(`max(r_load, r_deflect, r_transient, r_bend)`) while the actual code
two lines below only computes the first three (`transient.py:610`).
This is consistent with the codebase being at Phase 4 (bracing/transient
sizing, per the phase plan design-v4.md §14 and the memory note this
run predates Phase 5/curvature) — not a bug given current scope, since
all currently-built members are straight and `r_bend` only matters for
curved spans. Would become a real correctness gap the moment curved
descent/merge candidates (Phase 5) start landing curved members without
this term. **Confirming check:** re-grep after Phase 5 lands; verify
`r_bend` appears in `resize_forest`'s `max()` before any curved member
ships.

**6. LOW — most radius floors are non-physics "magic numbers," not
derived from tensile/modulus/peel.** `tip_r_min/max` (0.2/1.0mm),
`min_shaft_r_mm` (0.3mm), `min_trunk_r_mm`/`max_trunk_r_mm`
(0.8/2.4mm), `min_shoot_r_mm` (0.35mm) — none trace to
`tensile_green_mpa`/`modulus_green_gpa`/`peel_pressure_pa` in any
formula; they're explicitly "removal-friendly"/hobbyist-handling
constants or corpus-empirical anchors. This means a non-trivial (but
currently unmeasured) fraction of the 1625-member Treant run may be
**floor-bound rather than load-bound**, which would *dampen* the
resin-sensitivity items in §3 below what a naive `r∝√(peel/tensile)`
scaling predicts. **Confirming experiment:** instrument `resize_forest`
(transient.py:590-620) to tag which term won the `max()` per member,
then report the floor-bound fraction of total member volume on a real
run — this doesn't exist today and would materially sharpen the
resin-sensitivity ranking in §3.

---

## Summary for the calling agent

- Load model is **peel-only**: a flat `peel_pressure_pa` (3000 Pa,
  assumed) over Voronoi-partitioned carried area, resolved into axial
  tension (tip/trunk fuse sizing) or, via a separate constant lateral
  drag force (`shaft_drag_force_n`, 0.01N, assumed), into bending
  deflection. **No self-weight, gravity, buoyancy, density, or
  viscosity term exists anywhere in the codebase** — design.md states
  "gravity ~cancelled by buoyancy" in prose and nothing checks it.
- Three *different, not-reconciled* load→radius power laws coexist:
  tension sizing gives `r∝load^0.5`, the charter's own branched-transport
  derivation claims `r∝load^0.25` for deflection sizing but no formula
  actually implements that dependency on carried load, and transient
  sizing uses `r∝N_terminals^0.125` (a terminal count, not carried
  area). All three, plus non-physics floors, get `max()`'d together;
  whichever wins locally sets the real exponent. This is the top
  finding (§4 issue 2).
- Highest resin-data sensitivity: `tensile_green_mpa` (safety-critical
  if real green tensile < 10 MPa) and `peel_pressure_pa` (volume-cost
  dominant, `r∝1/√·` both ways). `modulus_green_gpa` is damped by a
  4th-root. Density data has **zero** effect on this engine's output as
  currently written — nothing consumes it.
- All dimensional-consistency checks on the tension and deflection
  formulas passed (verified by hand: N/(N/m²)=m² for tension;
  N·m³/((N/m²)·m)=m⁴ for deflection). The Murray `r⁴` convention is not
  dimensionally "a conserved quantity" in the traditional sense — it's
  a self-consistent bookkeeping choice from the branched-transport
  model, and `verify.murray_report` checks that bookkeeping against
  itself, not an independent physical law.
- `r_bend` (curved-span bending stress) is specified in the charter but
  not implemented — expected at the current Phase 4 stage (no curved
  members exist yet), but worth a follow-up check once Phase 5 lands.

Full detail, all file:line citations, and the ranked resin-sensitivity
and suspected-issues lists are in this file.
