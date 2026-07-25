# Physics audit: contacts & tips (dimension `contacts_tips`)

Repo: `resin-print-prep`, master f02b7af.
Files audited: `perception.py`, `struts.py`, `config.py`, `engine.py` contact-demand
functions. `placement.py` (tip formula), `routing.py`, `verify.py`, `cco.py` were read
opportunistically for grounding (the tip-radius formula lives in `placement.py` but is
config-owned and engine-invoked, so it is included; anything from those files is marked
"context, not primary scope").

All line numbers verified 2026-07-22 against
`scripts/support_placement/` at master f02b7af.

---

## 1. Complete table of physical constants & formulas

| Name | Value | File:line | Units | Provenance | Real resin property it depends on |
|---|---|---|---|---|---|
| `peel_pressure_pa` | 3000.0 | config.py:23 | Pa | **assumed** — "geometric mean of a plausible 1–5 kPa range," explicitly flagged soft in `notes()` (config.py:410) | peel stress (FEP release load) — see §3/§4 note: this is an *areal suction load*, not the same physical quantity as a resin's bond "peel stress" |
| `tensile_green_mpa` | 10.0 | config.py:29 | MPa | **assumed/measured-conservative** — low end of design.md's claimed 10–25 MPa green-tensile range, deliberately chosen weak-side | green tensile |
| `modulus_green_gpa` | 0.5 | config.py:30 | GPa | **assumed/measured-conservative** — low end of claimed 0.5–1.5 GPa | green modulus (used only in shaft deflection, `placement._shaft_r_for_L`; **not used by the tip formula at all**) |
| `tip_safety_factor` (SF) | 8.0 | config.py:34 | dimensionless | **assumed** — design.md's stated 5–10 range, "weighted toward the safe side" | none (a chosen design margin, not a resin measurement) |
| `tip_r_min_mm` | 0.2 | config.py:39 | mm | **assumed** — explicit docstring: "hobbyist-handling constraint, not a physics one; do not derive it" | none |
| `tip_r_max_mm` | 1.0 | config.py:40 | mm | **assumed** — same as above | none |
| `tip_radius_mm()` formula: `r = sqrt(F·SF/(π·tensile))`, `F = carried_area·peel_pressure` | — | placement.py:113-122 (context; called engine.py:83) | mm out, N/Pa/m² internal | **derived** (standard axial tensile-fuse equation) from the four rows above | peel pressure × green tensile (product/ratio) |
| `_r_tension_mm()` — same formula, unclamped, reused for shaft/junction load-conservation sizing | — | cco.py:118-128 (context) | mm | **derived**, explicitly the same math as `tip_radius_mm` per design-v4.md §6 | same as above |
| `contact_depth_mm` | 0.3 | config.py:122 | mm | **assumed/format-convention** — matches the native CHITUBOX type-7 tip convention ("0.3mm inside surface"), not a mechanics derivation | none |
| `standoff_mm` | 0.6 | config.py:116 | mm | **measured-ish/legacy** — "v0 shipped 0.6mm as centerline clearance", carried forward as the full-envelope margin | none (geometric/manufacturing tolerance; arguably should track printer XY accuracy, not a resin property) |
| `lift_mm` | 5.0 default (canonical run used 3.0, per prompt) | config.py:107; consumed routing.py:91-94, engine.py:67 | mm | **assumed** — "matches v0 proto's lift_mm=5.0" | none directly; interacts with total support-column volume, not resin mechanics |
| `island_merge_radius_mm` | 2.0 | config.py:102 | mm | **assumed** geometric closing radius, explicitly "not a physics constant" (config.py:99) | none |
| `bridge_free_gap_mm` | 1.0 | config.py:89 | mm | **assumed** — flat threshold standing in for "the cured layer stiffness spans it" | *should* depend on green modulus + layer thickness + span self-weight (density), but does not — see §4 finding |
| `near_plate_mm` | 1.5 | config.py:93 | mm | **assumed** | none |
| `overhang_angle_deg` | 45.0 | config.py:82 | degrees | **convention-matched** — mirrors CHITUBOX's `autosupportangle` | none (pure geometry) |
| `minima_connect_layers` | 2.0 | config.py:351 | layers (→ mm via `× layer_height_mm`) | **assumed** — "Craig's rule," a chosen safety margin, not computed from a force balance | conceptually should depend on birth-patch area/mass vs. per-layer suction force; not computed from either |
| `minima_eps_mm` | 0.02 | config.py:354 | mm (flat) | **assumed**, own docstring says "Sub-layer scale" | none — **see §4 finding: this is exactly the class of tolerance design-v4.md §11's 2026-07-22 amendment requires be layer-derived, and it isn't** |
| `minima_contact_radius_mm` | 1.0 | config.py:359 | mm (flat, 3‑D Euclidean) | **assumed, self-flagged loose** — engine.py comments call it "the old 1mm," "too-loose" (engine.py:351-364, perception.py:414) | none — still an **active** gate input, see §4 |
| `ISLAND_ATTACH_REACH_MM` | 0.2 | perception.py:420 | mm (xy only) | **derived/pinned** — explicitly set equal to `tip_r_min_mm`, "the smallest tip this engine ever builds" | tip_r_min_mm (indirectly), which itself has no resin dependency |
| `_ISLAND_BIRTH_LINK_R_MM` | 0.6 | engine.py:412 | mm | **assumed/measured-once** — "tuned once against the 0.05mm-layer Treant corpus," self-flagged as needing re-check at finer layer heights | none |
| `strut_min_length_saved_mm` | 8.0 | config.py:304 | mm | **assumed** — "don't chase tiny savings for a second scar" | none |
| `strut_max_length_mm` | 20.0 | config.py:314 | mm | **assumed** — measured-motivated cap (uncapped search picked bad anchors 307/353 times on the Treant, per comment) but the cap value itself is a round-number choice | none |
| `strut_fallback_max_angle_deg` | 60.0 | config.py:262 | degrees | **assumed** — "REVIVED v3.7 knob," matches the `branch_angle_deg` family | none directly (paired with the angle-aware peel load term for sizing, see placement.py:130-153, out of scope here) |
| `scar_cost_weight` | 1.0 | config.py:317 | dimensionless weight | **assumed** — a chosen relative cost weight, not physical | none |
| `material_cost_per_mm` | 0.05 | config.py:322 | cost/mm | **assumed** — its own docstring: "trades length against scar count, not physics" | none |
| `struts.cost()` = `material_cost_per_mm·length + scar_cost_weight·(delicacy₁+delicacy₂)` | — | struts.py:14-19 | mixed/no SI unit (an ad hoc scalar) | **heuristic, not physical** | none |
| `tip_ball_r_factor` | 1.5 | config.py:295 | dimensionless (× tip r) | **assumed/aesthetic** — "review: 1.25x read as lumpy" | none |
| `joint_ball_factor` | 1.0 | config.py:292 | dimensionless (× max member r) | **assumed/aesthetic** | none |
| `delicacy_curvature_weight` / `delicacy_thinness_weight` | 0.6 / 0.4 | config.py:329-330 | dimensionless weights | **assumed** | none (proxies for "what decimation would protect," not a resin property) |
| `overhang_angle_deg` `down_facing_mask` test: `nz < -sin(θ)` | — | perception.py:20-25 | dimensionless (unit normal component) | **derived** (correct trig for CHITUBOX's lean-from-vertical convention) | none |

---

## 2. Per-formula physics check

### 2.1 `tip_radius_mm` — the tensile fuse (placement.py:113-122, config-owned constants)
```
force_n = carried_area_m2 × peel_pressure_pa
r_m = sqrt(force_n × SF / (π × tensile_pa))
```
**Law encoded:** simple axial tensile stress, `σ = F/(πr²)`, inverted and multiplied by a
safety factor: `r = sqrt(F·SF/(π·σ_allow))`. This is the textbook fuse-neck sizing
equation.

**Dimensional check:** `F [N] = Pa·m²`. `σ_allow [Pa] = N/m²`. `r² = F·SF/(π·σ_allow)
= (N)/(N/m²) = m²` ⇒ `r = m`. **Consistent.** The mm↔m conversions (`×1e-6` for area in,
`×1000` for radius out) are applied correctly at the boundaries (placement.py:117,121).

**Where it looks oversimplified:**
- It models exactly one failure mode — clean axial tension through the tip's own
  cross-section. There is **no adhesion/bond check anywhere in this dimension**: whether
  the ~0.3mm embedded contact patch (`contact_depth_mm`, config.py:122) actually bonds to
  the green, still-uncured model surface with enough *interfacial* (peel/shear) strength
  is never computed. `contact_depth_mm` is a geometry/format convention (matches the
  native CHITUBOX tip type), not the output of any strength calc. See §4 issue 4.
- `carried_area_mm2` is a **steady-state, end-of-print** Voronoi share of the whole
  island's down-facing area (`placement.load_partition`, called once on the final
  contact set). It is the same number used for every contact regardless of whether that
  contact's job is to hold a large, long-cured region or to hold a just-born, still
  fingernail-sized disconnected patch at a layer-island birth (`minimum=True` /
  `island_birth=True` contacts, engine.py:378-380, 568-569). There is no birth-moment
  (first-few-layers) force computation anywhere — see §4 issue 4.
- `tip_safety_factor` (used identically as a strength multiplier here) is described in
  design.md as making the tip "the fuse" (weakest deliberate point), yet larger SF here
  makes tips **bigger**, i.e. SF is being applied as an over-build margin against the
  computed peel force, not as a mechanism that guarantees the tip stays the weakest link
  relative to the rest of the tree. That guarantee (tip radius < shaft radius) is not
  checked anywhere in this formula; it is only true in practice because the shaft/Murray
  sizing (`cco._r_tension_mm`) uses the *same unclamped formula* upstream and floors
  members at `min_shoot_r_mm`/`min_trunk_r_mm`, which happen to sit above `tip_r_max_mm`.
  That relationship is incidental to the current constants, not enforced.

### 2.2 `cco._r_tension_mm` (context, cco.py:118-128)
Identical formula, unclamped. Dimensionally identical, correctly reused per
design-v4.md §6 ("same form as `placement.tip_radius_mm`"). Confirmed consistent — this
closes what looked like a plausible finding (that `struts.py`'s model-anchor candidates
might be geometrically filtered but never load-sized): `cco._make_model_anchor_builder`
(cco.py:430-444) does call `_r_tension_mm` on the anchor member, so model-strut routes
get the same tensile check as plate routes. **Not a bug** — noted so the next reviewer
doesn't re-flag it.

### 2.3 `perception.local_z_minima` connection-time tolerance (perception.py:145-249)
```
tol = minima_connect_layers × layer_height_mm     (perception.py:195)
```
**Law encoded:** not a stress/strength law — a topological/timing rule: "a disconnected
patch is safe if it merges with strictly-lower (already-cured) material within `tol` mm
of z," i.e. within N real print layers. **Dimensionally trivial and correctly
layer-derived** (layers × mm/layer = mm), which is exactly the form design-v4.md §11's
2026-07-22 amendment demands. This function itself is compliant.

**Physics gap:** the choice of `minima_connect_layers = 2.0` is asserted ("Craig's
rule"), not derived from any force balance (e.g. "N layers of FEP peel force applied to
an unsupported patch of area A and green modulus E won't crack/detach it"). It is a
plausible, conservative-sounding number but has no computed backing in this codebase.

### 2.4 `perception.island_births` slicing (perception.py:423-471)
Slices at the real print layer pitch (`cfg.layer_height_mm`) and looks for xy-disjoint
polygon components slice-to-slice — a geometric/topological ground-truth detector, not a
mechanics formula. No dimensional issue (step = layer_height_mm used directly as the
z-slice pitch, correct).

### 2.5 `ISLAND_ATTACH_REACH_MM` (perception.py:344-420)
Pinned at `tip_r_min_mm` (0.2mm), reasoned as: "a tip whose center sits up to its own
radius outside the birth polygon can still have its own footprint touch the polygon
boundary." That is a legitimate geometric argument for using the *smallest possible* tip
radius as a conservative reach (never over-claims coverage), and it is explicitly and
correctly exempted from the layer-derivation amendment (its rationale is an xy mechanical
reach, independent of layer height — perception.py:399-410 argues this well). **No
issue** — this constant is one of the better-justified numbers in the file.

Note for the caller, though: because it uses the *minimum* possible tip radius rather
than each specific contact's actual (possibly up to 5× larger) tip radius, it is
conservative in the "under-count coverage" direction only — never in the direction of
falsely certifying an island as held. Confirmed safe by inspection, not a finding.

### 2.6 `struts.cost()` (struts.py:14-19)
```
cost = material_cost_per_mm × free_length_mm + scar_cost_weight × (contact_delicacy + anchor_delicacy)
```
**Law encoded:** none — a search/ranking heuristic (design-v4.md explicitly keeps this as
the model-anchor candidate pricing function, §4b, "resurrected"). Dimensional consistency
doesn't strictly apply to a chosen scalar objective, but worth flagging plainly: this
additively combines a length (mm) and a unitless delicacy score (0–1) with independently
tuned weights into one number that has no physical unit and encodes no strength/adequacy
claim at all. It is a *cost*, not a *capacity* check — `struts.py`'s own candidate filter
(`find_model_anchor_candidates`) never checks whether the resulting member can hold the
contact's peel load; that check happens downstream, correctly, in `cco.py` (§2.2). Fine
as designed, but a reader skimming `struts.py` alone (as this audit's file list implies)
could easily mistake `cost()` for a physical adequacy test. It is not one.

### 2.7 `find_model_anchor_candidates` lean-angle filter (struts.py:22-72)
```
lean = degrees(arctan2(dxy, drop))
```
Pure trigonometry (angle of the candidate from the contact, measured off the downward
vertical). Dimensionally and logically correct — no issue.

---

## 3. Ranked resin-sensitivity items

Ranked by expected impact on the engine's conclusions if the parallel team's real
ABS-like resin data (density ~1.05–1.2, post-cured tensile ~30–50 MPa, green much lower)
differs from what this dimension's code assumes. Each item states the mechanism so it can
be scored once real numbers land — no web data was fetched here.

1. **`peel_pressure_pa` (3000 Pa assumed) — HIGH impact, and a possible name collision
   with the incoming data.** `r_tip ∝ sqrt(peel_pressure)`. Back-of-envelope (computed
   from the current formula + constants, NOT measured from a run): at 3 kPa, a contact
   needs ≥52.4 mm² of carried area before the tensile-fuse formula asks for more than the
   0.2mm removal floor (`F_floor = π·(0.2mm)²·tensile/SF = 0.157N`); at the configured
   `target_spacing_mm=4.0` default, a hex-packed Voronoi cell is only ~13.9 mm² (~5.4 mm²
   at the 2.5mm spacing floor) — meaning **most ordinary interior-island contacts are
   floor-clamped to 0.2mm regardless of the load formula, at today's constants.** A
   change in `peel_pressure_pa` therefore does *nothing* to most tips until it gets large
   enough (or carried areas get concentrated enough — sparse islands, minima/birth
   contacts sharing a whole island) to lift contacts off the floor; from that point the
   sensitivity is direct (√). **Important scoping note for the evaluators of the fetched
   data:** this constant is used as an *areal suction load* (an environmental/process
   parameter — how hard the FEP pulls on a newly-cured cross-section during release), not
   as a resin *bond peel strength* (an ASTM-style interfacial adhesion property). If the
   parallel team's "peel stress" figure is a bond-strength number, it does not map
   directly onto `peel_pressure_pa` — flag this explicitly rather than substituting it
   in-place.

2. **`tensile_green_mpa` (10 MPa assumed, low end of a 10–25 MPa charter guess) — HIGH
   impact, direct.** `r_tip ∝ 1/√tensile`. The code deliberately underestimates green
   tensile for conservatism (documented at config.py:26-28). If real ABS-like green
   tensile is *even lower* than 10 MPa (plausible: the prompt's 30–50 MPa figure is
   post-cure; no green figure is given), every non-floor-clamped tip needs to grow, which
   pushes more large-area contacts (minima/birth contacts on sparse islands, or islands
   assigned `min_contacts_per_island=1`) toward or past the `tip_r_max_mm=1.0mm` cap. The
   hard `load_report` gate (verify.py:172-189) exists specifically to catch a
   clamped-but-underpowered tip and currently reports **0 violations** on the canonical
   run — but that is a statement about *today's* constants, not evidence the model is
   robust to a materially lower real green tensile. Re-run `load_report`'s margins (not
   just its pass/fail) once a real green-tensile number is available.

3. **Green modulus (0.5 GPa assumed) — LOW impact within this dimension.** Not used by
   the tip formula at all (`placement.tip_radius_mm` uses only tensile + peel pressure).
   It only feeds shaft deflection sizing (`_shaft_r_for_L`, out of primary scope here),
   where the dependence is a 4th root (`r ∝ E^{-1/4}`) — weak. Listed for completeness
   since it shares this dimension's config file.

4. **Density (~1.05–1.2 g/cm³) — effectively ZERO impact under the current model, by
   design, and that design choice is itself resin-sensitive.** design.md:26 states
   "Gravity ~cancelled by buoyancy" for a submerged MSLA print — density is never read
   anywhere in `perception.py`, `struts.py`, `config.py`, or `engine.py`'s contact-demand
   code. If that buoyancy-cancellation assumption doesn't hold in practice (partial
   emersion during a lift stroke, a resin denser/lighter than the model material changing
   the net buoyant force, tall members that clear the resin line) density would start to
   matter and there is currently no formula anywhere in this dimension that would pick it
   up.

5. **Viscosity — ZERO direct use anywhere in this dimension.** It plausibly *drives* the
   real value of `peel_pressure_pa` (higher viscosity → higher FEP release suction, all
   else equal) but the code never derives `peel_pressure_pa` from viscosity; it is a flat
   empirical guess. If the fetched viscosity data materially changes what a realistic
   peel/suction load looks like, that can only be applied by manually re-picking
   `peel_pressure_pa` — there is no formula path from viscosity to a support-sizing
   number.

---

## 4. Suspected issues (file:line, severity, confirming experiment)

**Issue 1 — HIGH — `minima_report`'s active coverage gate uses a looser, non-layer
criterion than the codebase's own stated standard, and than its sibling gate.**
`verify.py:214` (`if gap > cfg.minima_contact_radius_mm`) checks coverage with a flat
1.0mm **3‑D Euclidean** distance from the birth region to the nearest routed contact — no
separate xy/z treatment. `minima_contact_radius_mm`'s own docstring
(config.py:355-359) and two separate code comments (engine.py:350-364, perception.py:414)
call this exact number "too-loose" and explicitly say it lets a contact "20 layers away"
pass. The honest sibling gate (`island_birth_report`, verify.py:220-279) was built
specifically to fix this, using `ISLAND_ATTACH_REACH_MM=0.2mm` (xy) plus
`minima_connect_layers × layer_height_mm` (z) — but `minima_report` (a *different*,
still-active, still-counted gate feeding the canonical run's `unsupported_minima=28`) was
never updated to match. Both gates run on every `self_verify` call and both numbers are
reported (`unsupported_minima=28` vs `uncovered_island_births=21` in the canonical run) —
they are measuring conceptually the same thing at two different, inconsistent strictness
levels.
*Confirming experiment:* re-run `minima_report` with the gap test split into a
layer-derived z bound (`minima_connect_layers × layer_height_mm`) and an xy bound
(`ISLAND_ATTACH_REACH_MM`), instead of one flat 1.0mm 3‑D radius, and see how much
`unsupported_minima` grows on the frozen Treant run — if it grows substantially, `28` is
an undercount of the real physical risk under Craig's own stated criterion.

**Issue 2 — HIGH (charter-compliance) — `minima_eps_mm` violates the 2026-07-22
layer-derivation amendment by its own docstring's admission.** config.py:352-354: "Tie
tolerance for flat plateaus ... **Sub-layer scale**." `0.02mm` is a flat millimetre
constant whose stated rationale ("sub-layer scale") is exactly the class of tolerance
design-v4.md §11's binding amendment (quoted in the charter, added the same day as this
audit) requires be expressed as a multiple of `cfg.layer_height_mm`, not a pinned
absolute mm value — the amendment's own worked example is "a plateau-tie epsilon,"
which is precisely what this field is. At today's 0.05mm profile it reads as 0.4 layers
(comfortably sub-layer); at a future 0.02mm fine-detail schedule it would equal *one full
layer*, silently stopping being "sub-layer" without any code change flagging it.
*Confirming experiment:* express as `0.4 × layer_height_mm`, re-run `local_z_minima` on
the Treant, and confirm the minima count/regions are unchanged at the current 0.05mm
profile (proving the fix is a no-op today and only changes behavior once layer height
changes) — that is the actual test the charter calls for.

**Issue 3 — MEDIUM — the tip radius at exactly the contacts this audit cares about most
(layer-island births) is sized from a crude, ad hoc proxy area, not from the one area
measurement the codebase treats as ground truth for the same birth event.**
`engine.py:551-570` (`_ensure_island_birth_contacts`) and `engine.py:365-380`
(`_ensure_minima_contacts`) both size a new contact's carried-area (and hence tip radius,
via `load_partition`/`tip_radius_mm`) from whichever pre-existing overhang island the
nearest face happens to belong to, or — when the birth sits on a steep flank outside
every known overhang island (exactly the case these two passes exist to catch) — from a
crude synthetic island built from `centers_tree.query_ball_point(pos, r=1.5)`
(engine.py:365/553), a flat 1.5mm face ball. Meanwhile `perception.island_births` has
already computed a precise polygon area for that exact birth event
(`b["area_mm2"]`, perception.py:468) — that number is present on the input dict `b` but
is **never propagated** into the new contact or the synthetic island; `engine.py:567`
recomputes `area = mesh.area_faces[islands[iid]].sum()` from the crude ball instead. The
tip radius at a birth contact is therefore decoupled from the codebase's own best measure
of how much area that birth actually needs held.
*Confirming experiment:* on a real run, log both the assigned island/synthetic-island
area used for `load_partition` at each birth/minima contact, and the birth's own
`b["area_mm2"]`, side by side; if they diverge by more than a small factor, decide which
should drive sizing (likely: use `b["area_mm2"]` directly for `island_birth=True`
contacts rather than routing through the synthetic-island machinery).

**Issue 4 — MEDIUM — no birth-moment (transient release) force model exists; tip sizing
at a birth is a steady-state formula wearing a floor clamp, not a computed answer to "is
this enough at the moment that matters."** `placement.tip_radius_mm` (placement.py:113,
invoked identically for every contact at engine.py:83) uses the same
`peel_pressure_pa × carried_area` product for a contact whose sole job is anchoring a
just-born, still-tiny disconnected patch as for a contact holding a large, long-cured
region. There is no separate accounting anywhere in this dimension for the FEP
peel/suction event that actually detaches a freshly-cured layer — generally understood to
be the highest-magnitude, most time-critical force in the whole print, and the physical
reason layer-island births are risky in the first place (design.md:25, "arriving as
TENSION"; perception.py's own local_z_minima docstring frames the risk purely in terms of
*layer count* to connection, never in terms of a computed force). In practice this is
probably conservative on the *load* side (a birth-moment patch's real area is smaller
than its eventual full Voronoi share, so using the bigger number over-sizes rather than
under-sizes) — but the practical answer to "is 0.2mm reach/tip physically enough for what
island mass/area?" that the task poses is currently: "yes, by floor-clamp default,"
never "yes, because we computed the first-layer release force and it's under the floor's
0.157N capacity" (§3 item 1's derivation). No formula in this codebase currently performs
that specific, narrower check.
*Confirming experiment:* compute the first-real-layer release force explicitly —
`peel_pressure_pa × (birth polygon's own area at first detection, i.e. `b["area_mm2"]`
for the earliest slice a given footprint appears in)` — and compare against the floor
tip's computed capacity (0.157N at today's constants, §3 item 1). If every birth
polygon's first-detected area is comfortably under ~52 mm² (the area at which the formula
would ask for more than the floor anyway), the floor is provably fine for this failure
mode specifically; if any birth's first-slice area already exceeds that, the current
approach is silently under-computing for that case, for reasons unrelated to whether
the floor value itself is right.

**Issue 5 — LOW — `standoff_mm` and `contact_depth_mm` carry no resin dependency at all,
and nothing in this dimension checks the physical claim `contact_depth_mm` implicitly
makes (that 0.3mm of embedment is enough for a real bond).** Both are manufacturing/format
tolerances (config.py:116, 122) with `none` in the property column — correctly so for
what they *are* (a clearance margin and a slicer-format convention), but
`contact_depth_mm` is the only place in the codebase that encodes "how much of the tip is
actually inside cured/curing material," and no formula anywhere multiplies that
embedment depth by any strength value to check bond adequacy (this is the same gap as
Issue 4, viewed from the geometry side rather than the force side). Severity kept LOW
because it is a documentation/completeness gap, not a demonstrated wrong number — the
same confirming experiment as Issue 4 would surface whether it matters in practice.

**Non-issue, confirmed by inspection (recorded so it isn't re-flagged):** `struts.py`'s
candidate filter (`find_model_anchor_candidates`) does not itself check load — this looks
like a gap on a struts.py-only read, but `cco._make_model_anchor_builder`
(cco.py:430-444) does call the same tensile formula (`_r_tension_mm`, §2.2) on the
resulting member before acceptance. Confirmed consistent; not a finding.

---

## Summary for the calling agent

Core tip-fuse formula (`r = sqrt(carried_area·peel_pressure·SF/(π·tensile))`,
placement.py:113-122) is dimensionally consistent and correctly reused unclamped for
shaft/junction sizing (`cco._r_tension_mm`, confirmed not a bug). Two concrete,
file:line-grounded issues stand out: (1) the active `minima_report` gate
(verify.py:214, config.py:359) still uses a flat 1.0mm 3‑D-distance criterion that the
codebase's own comments call "too-loose" and that a sibling gate
(`island_birth_report`) was specifically built to replace — the two gates disagree by
~5× on strictness and both feed the canonical run's reported numbers
(`unsupported_minima=28` vs `uncovered_island_births=21`); (2) `minima_eps_mm`
(config.py:354) is a flat-mm tolerance whose own docstring says "sub-layer scale" —
a textbook case the charter's 2026-07-22 layer-derivation amendment was written to
catch, and it isn't compliant. A third, more structural gap: tip sizing at layer-island
births is decoupled from the codebase's own best area measurement for those births
(`perception.island_births`'s `area_mm2` is computed but discarded in favor of a crude
1.5mm face-ball proxy, engine.py:551-570), and no formula anywhere computes a
birth-moment (first-layer release) force distinct from steady-state carried peel load —
back-of-envelope, most ordinary contacts are floor-clamped to 0.2mm regardless (typical
Voronoi cell ~14–5.4 mm² vs. the ~52 mm² needed to lift off the floor at today's 3kPa/10MPa
constants), so the honest answer to "is 0.2mm physically enough at birth" is currently
"yes by floor-clamp default," not "yes by computed birth-force check." Resin-sensitivity
ranking: `peel_pressure_pa` and `tensile_green_mpa` matter most (√-scaling, and most
contacts are currently floor-bound so headroom exists before it matters); green modulus
matters little to this dimension (4th-root, and unused by the tip formula itself);
density and viscosity currently have zero code path into any formula in this dimension
at all (density by an explicit buoyancy-cancellation design choice; viscosity because
`peel_pressure_pa` is a flat guess, not derived from it) — flag for evaluators that the
fetched "peel stress" figure may be a different physical quantity (interfacial bond
strength) than what `peel_pressure_pa` represents (an areal FEP-suction load), and should
not be substituted in without checking which one it is.
