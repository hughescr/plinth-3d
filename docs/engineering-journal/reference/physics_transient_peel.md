# Physics audit — dimension: transient_peel

Repo: resin-print-prep, master f02b7af. Files in scope: `transient.py`,
`bracing.py`, `engine.py::brace_and_size`, `config.py`. (`placement.py`
and `cco.py` read as required dependencies of the sizing formulas these
files call — `_shaft_r_for_L`, `_r_tension_mm`, `_lean_deg`.)

Charter: `skills/support-placement/references/design-v4.md` §6 (Sizing),
§8 (Build-time honesty on a tree), §9 (Bracing).

---

## 1. Complete constants/formulas table

### 1a. Physical constants (config.py)

| Name | Value | File:line | Units | Provenance | Real resin property |
|---|---|---|---|---|---|
| `peel_pressure_pa` | 3000.0 | config.py:23 | Pa | **Assumed** — "design.md says 'a few kPa'; 3.0 kPa is the geometric mean of a plausible 1-5 kPa range." Explicitly flagged soft in the docstring and in `notes()`. | peel stress |
| `tensile_green_mpa` | 10.0 | config.py:29 | MPa | **Assumed** (conservative/weak end of design.md's stated 10-25 MPa green-tensile range; deliberately picked low to bias toward safety) | green tensile |
| `modulus_green_gpa` | 0.5 | config.py:30 | GPa | **Assumed** (weak end of design.md's stated 0.5-1.5 GPa green-modulus range) | green modulus |
| `tip_safety_factor` | 8.0 | config.py:34 | dimensionless | **Assumed** (design.md SF range 5-10, 8 "sits mid-range") | none (engineering margin) |
| `shaft_drag_force_n` | 0.01 | config.py:50 | N | **Assumed / guessed** — docstring: "ROUGH ESTIMATE (order-of-magnitude guess ...) pending calibration against observed ribbing on real prints ... treat shaft_r outputs as relative/ordinal ... more confidently than absolute." No measurement basis at all. | none named; stands in for viscous/lift drag on a thin shaft (closest real property: resin viscosity + lift velocity, neither modeled) |
| `max_deflection_mm` | 0.15 | config.py:54 | mm | **Assumed** (visible-ribbing tolerance, a print-quality criterion not a resin property) | none |
| `max_unbraced_shaft_r_mm` | 0.9 | config.py:60 | mm | **Assumed/calibrated-by-corpus** ("set near the corpus's realized Heavy shaft radius") — a practicality ceiling, not derived from resin data | none |
| `min_shaft_r_mm` | 0.3 | config.py:61 | mm | **Assumed** (removal/printability floor) | none |
| `min_trunk_r_mm` | 0.8 | config.py:220 | mm | **Assumed** (removal-friendly band) | none |
| `min_shoot_r_mm` | 0.35 | config.py:223 | mm | **Assumed** | none |
| `max_segment_angle_deg` | 45.0 | config.py:165 | deg | **Assumed/derived-qualitatively** (layer-continuity + peel-moment-arm reasoning in the comment, no closed-form derivation) | indirectly green tensile/modulus (peel moment arm) |
| `branch_angle_deg` | 60.0 | config.py:277 | deg | **Assumed** (v3.6 Craig call, "REQUIRED PAIRING" with angle-aware load term) | none directly |
| `strut_fallback_max_angle_deg` | 60.0 | config.py:262 | deg | **Assumed** | none |
| `brace_free_span_mm` | 18.0 | config.py:237 | mm | **Measured-then-generalized**: "measured example (65mm bare run at r=1mm) deflects ~2.3mm under even the configured 0.01N lateral load, 15x the budget" — but the measurement itself rests on the unmeasured `shaft_drag_force_n` | modulus (via the deflection formula used to derive it) |
| `twin_single_brace_xy_mm` | 3.0 | config.py:247 | mm | **Assumed** (geometric judgment call: below this spacing a second X-pair rod "adds no stiffness, only lattice clutter") | none |
| `brace_r_mm` | 0.5 | config.py:199 | mm | **Assumed** (floor, "same class as v3.6") | none |
| `model_brace_max_len_mm` | 5.0 | config.py:254 | mm | **Assumed** | none |
| `brace_neighbor_max_xy_mm` | 15.0 | config.py:169 | mm | **Assumed**, widened by `brace_reach_max_factor_x` (1.5, config.py:196) on **one measured data point** (member 2337/1015 case, config.py:172-190) | none |
| `tip_r_min_mm` / `tip_r_max_mm` | 0.2 / 1.0 | config.py:39-40 | mm | **Assumed** (hobbyist-handling constraint, explicitly "do not derive it") | none |

**No resin density constant exists anywhere in config.py / transient.py / bracing.py / placement.py.** `grep -i "density"` in these files turns up nothing except the unrelated *tip-spacing* density (`target_spacing_mm`, a geometric sampling density, not a material property). None of the transient/peel sizing formulas depend on resin density (kg/m³ or g/mL) at all — only on green tensile, green modulus, and the assumed peel/drag forces. This matters directly for the ChituBox-vs-engine volume-ml comparison Craig flagged: our `member_ml`/`ball_ml` accounting is a pure geometric volume (mm³→mL), not mass-derived, so density plays no role in either side of that 12.84 vs 9.99 ml discrepancy — it is not a candidate explanation.

### 1b. Formulas

| Name | Formula | File:line | Beam/load model | Units check |
|---|---|---|---|---|
| `transient_worst_span` | sweep over `(point_z, active_z)` restraint events, sorted by `active_z`; `worst = max(a - highest_active)` | transient.py:49-72 | pure geometry (no physics) — restraint-activation timing | mm in, mm out — consistent |
| `_span_intervals` | same sweep, returns every `(lo,hi)` candidate span | transient.py:75-95 | geometry | consistent |
| `transient_report` moment-arm | `contribution = hi - max(z_min(M), R)` per overlapping interval, member's `worst_span_mm` = max over all paths/intervals | transient.py:262-441, esp. 403-407 | geometry (activation-aware taper of a chained cantilever's arm) | mm; consistent |
| `_shaft_r_for_L` (deflection solve) | `δ = 4·F·L³/(3·E·π·r⁴)` ⇒ `r = (4·F·L³/(3·E·π·δ))^0.25`; angle-aware `F_eff = F_drag + peel_pressure·π·r²·tan θ`, 6-iteration fixed point when θ>0 | placement.py:130-153 | **cantilever, point load at the free tip** (`δ = FL³/3EI`, `I=πr⁴/4` ⇒ `δ=4FL³/3Eπr⁴`) — classic Euler-Bernoulli cantilever tip deflection | SI internally: L_m³·[N]/([Pa]·[m⁴]) → checked below, consistent |
| `r_transient_mm` | `_shaft_r_for_L(worst_span_mm, cfg, lean_deg) · max(n_terminals,1)^0.125` | transient.py:454-461 | same cantilever model, scaled by a ported v3 load/terminal-count heuristic | consistent (dimensionless exponent on a dimensionless count) |
| `resize_forest` r_req | `r_req = max(r_murray, r_tension, r_deflect, r_trans, kind_floor)` | transient.py:598-610 | combines Murray conservation, axial tension, cantilever deflection (both static-gap and transient-span variants) | consistent |
| `_r_tension_mm` (Murray/load floor) | `r = sqrt(F·SF/(π·σ_tensile))`, `F = carried_area_m2 · peel_pressure_pa` | cco.py:118-128 | axial stress fuse, **not** a bending model | `[N]/[Pa]` = m² ⇒ sqrt ⇒ m; consistent |
| X-pair load-sharing discount | `duty_r = r_raw · 0.5**0.25` | bracing.py:738, config comment bracing.py:12-13 | derived from the SAME cantilever formula's `r ∝ F^0.25`: splitting one restraint duty force between 2 rods ⇒ each carries F/2 ⇒ `r=(geom·F/2)^0.25 = r_raw·2^-0.25` | consistent, **and internally self-derived** (not an independent magic number given the deflection model) |
| Brace's own r_deflect floor | `own_floor = _shaft_r_for_L(own_len, cfg, own_lean)`, `r = max(brace_r_mm, own_floor, duty_r)` | bracing.py:702-745 | cantilever, each rod's own real endpoint geometry (not the 45°-construction approximation `r_raw` uses) | consistent |
| `max_unbraced_len_mm` | inverse of `_shaft_r_for_L`: `L = (r⁴·3·E·π·δ / 4F)^(1/3)` | placement.py:156-168 | same cantilever model, closed form (angle known ⇒ no iteration needed) | consistent |
| `compute_needs` static trigger | `n_slender = _n_slender(gap, lean)`; `n_span = int(span/brace_free_span_mm)`; `n_rungs = max(n_slender, n_span)` | bracing.py:259-261 | slenderness (deflection-budget) OR fixed-cadence rule, whichever demands more rungs | consistent (dimensionless counts) |
| `compute_needs` activation trigger | fires iff `r_transient_mm(...) > max_unbraced_shaft_r_mm OR worst_span_mm > brace_free_span_mm` | bracing.py:290-291 | same cantilever-derived radius vs. a fixed span ceiling | consistent |

---

## 2. Per-formula physics assessment

**`_shaft_r_for_L` / cantilever deflection.** This is the single physics
core of the whole dimension — both `r_deflect` (static free_gaps) and
`r_transient` (activation-aware worst_span) call it. It correctly
implements the standard cantilever point-load tip-deflection formula
δ=FL³/(3EI) with I=πr⁴/4 for a circular section, algebraically solved
for r. Dimensional check done by hand: `geom = 4·L_m³/(3·E·π·δ)` has
units m³/(Pa·m) = m⁴/N; `geom·F` → m⁴; `(·)^0.25` → m. Consistent.

**Beam model used: cantilever only, single end fixed.** There is no
"both-ends" (simply-supported or fixed-fixed) beam model anywhere in
this dimension. For the *transient* (mid-print) case this is the
physically correct choice — by definition, structure above the current
layer height does not exist yet, so the growing member genuinely has a
free tip. For the *static* case (`resize_forest`'s `r_deflect` over
`info["free_gaps"]`, transient.py:602-607; also `bracing.py:542`), this
is **not** always correct: once a brace/rung lands on *both* ends of a
free_gap (the common post-bracing state — that is the entire point of
bracing), the gap is no longer a cantilever, it is closer to a
simply-supported or fixed-fixed span, which is 8x–64x stiffer than a
cantilever of the same length (`δ_ss=FL³/48EI`, `δ_ff=FL³/192EI` vs.
`δ_cant=FL³/3EI`). Since `r∝δ^(-1/4)`, using the cantilever formula on
an interior, both-sides-restrained gap over-radiuses that segment by up
to `64^0.25≈2.83x` versus what a correctly-modeled fixed-fixed span
would need. This is conservative (never undersizes — a real safety
property) but is an oversimplification the charter doesn't call out,
and it directly inflates `member_ml` (§4 of this report has the
volume-relevance ranking). See Suspected Issue B below.

**`peel_pressure_pa`'s double duty.** The same 3000 Pa constant is used
in two physically distinct ways: (1) `_r_tension_mm`/`tip_radius_mm`
(cco.py:118-128, placement.py:113-122) — pressure over the *island's own
carried overhang area*, a reasonably well-posed peel-adhesion load; and
(2) the lean-driven term inside `_shaft_r_for_L`'s iteration
(placement.py:151) — pressure applied to the *shaft's own tiny
cross-sectional footprint* as it leans, a different physical event (a
thin support rod's own layer releasing from the FEP film) with no
stated reason the same pressure constant should govern both. The
elliptical-footprint/`tan θ` resolution (placement.py:136-140) is
internally consistent trigonometry, but the underlying premise —
same peel pressure regardless of the peeling patch's own area/geometry
— is asserted, not derived; real FEP peel mechanics is known to be
non-uniform with peeled area in practice.

**`shaft_drag_force_n`.** Purely a guess (config.py:43-50 says so
explicitly). It is the *entire* lateral-load basis for a vertical or
near-vertical member (θ≈0 ⇒ the peel term vanishes ⇒ `F_eff ≈
shaft_drag_force_n` alone), which is most trunks and vertical model
rungs in the tree. Nothing anchors this number to physical drag/viscous
force; "order-of-magnitude guess" is the codebase's own characterization.

**`n_terminals**0.125` (r_transient_mm, transient.py:454-461).** Ported
from v3 ("resize_for_spans applies N**0.125 directly, size_trunks
applies N**0.5 then a further **0.25"). The self-consistency of v3's two
formulations is a *bookkeeping* check, not a *physics* derivation — no
citation or measurement ties total lateral drag force to
`n_terminals^0.5` in the first place. Given the deflection model's own
`r∝F^0.25`, an `F∝N^0.5` assumption produces `r∝N^0.125` cleanly, so the
exponent is arithmetically consistent with *itself*, but the underlying
`N^0.5` load-vs-terminal-count scaling is unvalidated against any real
print data.

**X-pair discount `0.5**0.25`.** This is the one quantity in the table
that is genuinely *derived*, not assumed: given the deflection model's
`r∝F^0.25` and an even 50/50 load split between the two rods of an
X-pair, `r_shared=(F/2)^0.25/F^0.25 · r_solo = 2^-0.25·r_solo`. It falls
out of the model rather than being hand-picked, and — because it's a
ratio — it is **robust to resin-data uncertainty**: whatever the real
`E`, `peel_pressure_pa`, or `shaft_drag_force_n` turn out to be, the
discount factor stays `2^-0.25≈0.8409` as long as the 50/50 split
assumption holds. The 50/50 split itself is an engineering assumption
(not measured), but the arithmetic built on it is sound. The code
additionally guards this discount from ever violating a rod's own
survival floor (`own_floor`, bracing.py:707-726) — a self-aware
"discount vs. per-rod floor" reconciliation the module docstring frames
as an explicit ORCHESTRATOR RULING, correctly reasoned.

**Brace radius floors.** `_cross_link_rods` (bracing.py:702-745)
computes `r_raw` from the 45°-construction-approximation length and
`max_segment_angle_deg`, then (for X-pairs) re-derives each rod's *own*
floor from its *true* endpoint geometry (`own_len`, `own_lean` via
`cco._lean_deg`) rather than the approximation — correctly identified
in the code's own comment as necessary because "on a LEANING host, a
rod's true endpoint-to-endpoint geometry drifts from that
approximation," with a measured example cited (two mirrored rods of one
X-pair: floors 0.6765mm vs 0.6508mm, differing because the two hosts'
own leans differ). This is sound: it uses the same deflection formula
consistently, just evaluated at the right geometry.

**What "layer lift" physically means here, and how well the model
captures it.** Two distinct real events at each layer transition are
modeled: (1) the cured layer's adhesive release from the FEP film
(peel), producing an upward/lateral pull roughly proportional to newly
cured area — modeled as `peel_pressure_pa` × area; and (2) the
mechanical whip/flex of an as-yet-unrestrained free-standing member as
the build plate rises and resin flows around it — modeled as a
constant point lateral force `shaft_drag_force_n` plus (for leaning
members) an additional `peel_pressure_pa`-driven component. The
`transient_worst_span`/`transient_report` machinery correctly captures
*when* a given cross-section is genuinely unsupported (nothing above
the current print height exists; a restraint isn't "real" until its own
supporting structure has itself finished printing) — that part is sound
process modeling, independent of the specific force constants' accuracy.
What's weaker is the two force *magnitudes* themselves (see the
resin-sensitivity ranking below) and the always-cantilever assumption
once a span has restraints on both sides.

---

## 3. Resin-sensitivity ranking (for cross-check against real ABS-like water-washable data)

Ranked by how much the transient/peel dimension's *conclusions*
(required radii, brace trigger points, member_ml) would change if real
resin data diverges from the assumed values. Do not resolve these here —
hand off against the parallel team's real-resin numbers.

1. **`modulus_green_gpa` (assumed 0.5 GPa, charter range 0.5-1.5 GPa).**
   Highest-leverage constant in this whole dimension: it is the sole
   material property inside `_shaft_r_for_L`'s `E` term, which drives
   BOTH `r_deflect` and `r_transient` — the two floors that (per §4
   below) size the majority of `member_ml`. `r∝E^-0.25`. If real green
   modulus for the target ABS-like resin sits at 1.5 GPa (3x the assumed
   0.5), required transient/deflect radii shrink by `3^0.25≈1.316x`
   (≈24% radius reduction, ≈42% volume reduction per member in the
   deflection-dominated regime) — potentially a large fraction of the
   member_ml vs. ChituBox gap. If real green modulus is *lower* than
   0.5 GPa (plausible for a "much lower than post-cure" green state),
   the engine is already under-building relative to what its own model
   would demand.
2. **`shaft_drag_force_n` (assumed 0.01 N, admittedly an unmeasured
   guess).** Dominant/sole force term for vertical and near-vertical
   members (the majority of trunks/model rungs, where the peel-lean term
   vanishes). `r∝F^0.25`, so a 10x calibration error (plausible given
   zero measurement basis) moves radius ~1.78x, volume ~3.2x for that
   population. This constant has no grounding in any resin's stated
   properties (viscosity, density, or tensile) — it's not clear a
   density/viscosity-informed calibration from real resin data would
   even fully determine it, since it also encodes plate-lift kinematics.
   Still the single most consequential *unknown* in the dimension.
3. **`peel_pressure_pa` (assumed 3000 Pa, "few kPa" charter range).**
   Enters this dimension via the lean-driven term in `_shaft_r_for_L`
   (matters increasingly as lean → 60°, where `branch_angle_deg`/
   `strut_fallback_max_angle_deg` allow steep members) and via the
   `r_load` floor that competes in `resize_forest`'s `max(...)`. `r∝F^
   0.25` again, so ~19% radius swing per 2x error. Real "peel stress"
   for a specific water-washable ABS-like resin at a given exposure/
   lift-speed profile could plausibly sit outside the 1-5 kPa charter
   range — worth checking directly against the parallel team's data if
   they have any peel-force numbers (vs. just tensile/density).
4. **`tensile_green_mpa` (assumed 10 MPa, weak end of 10-25 MPa
   charter range).** Feeds `r_load`/`_r_tension_mm` (cco.py) which
   competes in the same `max(...)` as this dimension's own
   `r_deflect`/`r_transient` — so it changes which term actually *binds*
   more than it changes the transient formulas themselves. `r∝σ^-0.5`
   (weaker sensitivity than the deflection terms' `^-0.25` might suggest
   at first glance — for tension it's actually stronger, `-0.5`). If
   real green tensile for an ABS-like water-washable resin is much lower
   than 10 MPa (plausible, since post-cure 30-50 MPa is a *cured* number
   and green tensile is typically a small fraction of that), `r_load`
   could become the dominant floor across much of the tree, which this
   dimension's own report should account for in the volume story even
   though it isn't itself a transient/peel formula.
5. **X-pair `2^-0.25` discount and `n_terminals^0.125` exponent.**
   Low sensitivity to resin data specifically — the X-pair discount is a
   ratio, internally invariant to `E`/`peel_pressure_pa`/
   `shaft_drag_force_n` value (only the 50/50-split assumption matters,
   which is a *load-sharing geometry* assumption, not a material one).
   `n_terminals^0.125` is a topology heuristic, also not resin-property
   driven — its uncertainty should be resolved by print-corpus
   calibration, not by better resin data.
6. **Practicality floors** (`brace_r_mm`, `min_shaft_r_mm`,
   `min_trunk_r_mm`, `min_shoot_r_mm`, `max_unbraced_shaft_r_mm`,
   `tip_r_min/max_mm`). Removal-ergonomics choices, not physics — inert
   to resin-property data. They matter only as a *ceiling on how much
   volume savings better resin data can unlock*: even if real E/tensile
   turn out very favorable, these floors put a hard lower bound on
   member_ml that no amount of accurate resin data changes.

---

## 4. Suspected issues

**A. `r_bend` (charter §6's fourth sizing floor) is not implemented
anywhere in the codebase.** design-v4.md §6 defines final radius as
`max(r_load, r_deflect, r_transient, r_bend)`. `resize_forest`
(transient.py:590-626) computes exactly `r_murray, r_tension, r_deflect,
r_trans, kind_floor` — no `r_bend` term, no eccentricity/bending-under-
peel-tension calc anywhere. The only occurrence of "r_bend" in the whole
support_placement package is a comment referencing the charter
(transient.py:520), inside a docstring explaining a *different* fix
(SPEC-ARM-FIX). File:line: transient.py:598-610 (the actual `r_req`
formula, missing the term); design-v4.md:254-270 (the spec). Severity
guess: MEDIUM — if v4's constructor is not yet emitting genuinely curved
(non-collinear) multi-segment members at this phase, this is a dormant
gap (nothing to size); if it is, any curved span currently sizes without
its bending-tax term and could be undersized relative to the charter's
own stated physics. Confirm experiment: scan the audited run's member
list (`engine_run_results.json`) for any member/path whose polyline
deviates from a straight p0→p1 chord by more than a token tolerance; if
any exist, r_bend's absence is live, not dormant.

**B. Cantilever deflection formula applied uniformly to free_gaps that
may be restrained on both ends.** `resize_forest`'s `r_deflect`
(transient.py:602-607) and `bracing._trial_resize_r`'s equivalent
(bracing.py:538-542) both feed `info["free_gaps"]`'s widest sub-span
into `_shaft_r_for_L`, which is a single-end-fixed cantilever model,
regardless of whether both boundaries of that gap are real restraints
(the common case once bracing has run — restraining both sides is the
entire premise of §9 Bracing). A both-sides-restrained span is 8x-64x
stiffer than a cantilever of the same length, so this over-radiuses such
segments by up to `64^0.25≈2.83x`. File:line: transient.py:590-610
(esp. 602-607); bracing.py:523-545. Severity guess: MEDIUM-HIGH — this
directly inflates `member_ml` (the dominant 11.49 ml of the reported
12.84 ml total) independent of Craig's overlap-double-counting
hypothesis for the ChituBox gap, and is a *plausible additional,
non-overlap source* of the 12.84 vs 9.99 ml discrepancy worth ruling in
or out before concluding the gap is 100% double-counted geometry.
Confirm experiment: pick several braced interior members from the
audited run, recompute `r_deflect` using a simply-supported point-load
formula (`δ=FL³/48EI`) instead of the cantilever formula for gaps with
restraints on both ends, and sum the resulting member_ml delta across
the forest.

**C. `peel_pressure_pa` reused for two different physical peel events
without justification.** Same 3000 Pa constant governs both the
overhang-area peel force (cco.py:118-128, placement.py:113-122) and the
shaft's-own-footprint lean-driven peel term inside `_shaft_r_for_L`
(placement.py:136-153) — different peeled-area scales with no stated
reason the pressure should be area-invariant across both. File:line:
placement.py:151, cco.py:125. Severity guess: LOW-MEDIUM (only matters
for steep/leaning members, where `branch_angle_deg`=60° and
`strut_fallback_max_angle_deg`=60° already allow significant lean).
Confirm experiment: real-print measurement of layer-shift/whip on
thin near-vertical vs. steeply-leaning shafts of matched radius, to see
whether the same pressure constant actually predicts both.

**D. `shaft_drag_force_n` is a zero-evidence guess governing the
majority-population (vertical/near-vertical) member sizing.** Already
self-flagged in config.py:43-50 ("treat shaft_r outputs as
relative/ordinal ... more confidently than absolute"), but worth
escalating here because it is the *sole* force term for any member at or
near vertical lean (the peel-lean term vanishes at θ≈0), which given
`branch_angle_deg` defaults and typical trunk geometry is likely the
plurality of members. File:line: config.py:43-50. Severity guess: HIGH
sensitivity / unknown direction (§3 item 2 above). Confirm experiment:
same as item C — back out an implied `shaft_drag_force_n` from measured
deflection/ribbing on a real print at known r, L, lean, then compare
against 0.01N.

**E. `n_terminals**0.125` load-sharing exponent is a ported v3 heuristic
with no independent physical derivation or citation, only internal
self-consistency between v3's two equivalent spellings.** File:line:
transient.py:454-461. Severity guess: LOW (topology heuristic, not
resin-sensitive, but still an unvalidated physics claim baked into
every `r_transient` calc). Confirm experiment: on real prints with
varying downstream terminal counts and matched worst_span/lean, measure
whether observed deflection/ribbing tracks `N^0.5` force scaling (the
implicit assumption) versus linear-in-N or some other relationship.

**F. Restraints modeled as instantly, ideally rigid at `active_z`.**
Not a single-line bug but an architectural assumption throughout
`transient_report`/`resize_forest`: the instant a brace's/junction's own
supporting structure finishes printing, it is treated as a perfectly
rigid restraint from that height on (no green-strength ramp-up, no
compliance). File:line: transient.py:17-26 (module docstring stating
the rule explicitly), transient.py:214-236 (`_path_events`). Severity
guess: LOW (reasonable simplification; would matter more for a resin
with an unusually slow green-strength development curve, which is a
real resin-property difference the parallel team's data might surface).

---

## Summary of what "worst_span activation" + deflection model actually assumes

- Beam model: **cantilever, single end fixed, point load at the free
  tip** — the correct choice for the genuinely-growing (mid-print) case,
  applied uniformly (arguably over-conservatively, see Issue B) even to
  already-both-sides-restrained static gaps.
- Lateral/peel load during transients: `F_eff = shaft_drag_force_n [N,
  constant, unmeasured] + peel_pressure_pa [Pa, "few kPa" estimate] ·
  π·r² · tan(lean)` — a constant baseline drag plus an angle-scaled peel
  component that only engages for leaning members.
- X-pair sharing discount `2^-0.25`: **derived**, not assumed, from the
  deflection model's own `r∝F^0.25` under a 50/50 load-split assumption
  between the two rods; robust to resin-data uncertainty.
- Brace radius floors: each rod additionally floored at its own
  `_shaft_r_for_L` value computed from its TRUE endpoint geometry (not
  the 45°-construction approximation), so the discount can never push a
  rod below what its own real geometry would require to survive peel on
  its own.
- Physically, "layer lift" is modeled as two separate real mechanisms
  (FEP-release peel force on newly-cured area; viscous/mechanical whip
  on unrestrained shafts) with the timing (when a restraint counts as
  "real") handled soundly via the activation-height sweep, but the force
  *magnitudes* for both mechanisms rest on unmeasured or weakly-measured
  constants (`shaft_drag_force_n` most of all, `peel_pressure_pa`
  second), and no resin density plays any role anywhere in this
  dimension.
