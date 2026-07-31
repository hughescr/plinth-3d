# 03 — Physics and Constants

Every physical rule and every numeric constant the Rust core must encode, with
provenance. This document is the authority for **values**. `02-target-architecture.md`
is the authority for **crate and type names** and for the provenance **mechanism**; if
the two ever disagree, follow 02 for the name and this document for the number.

**Reading the "Rust owner" column.** Every constant below is *declared* in
`plinth-core::constants` (04-migration-plan.md M0's module table) with its provenance
wrapper. The column names the crate that **consumes** it, using 02 §2's real crate
names. There is no `plinth-physics` crate.

Read this before writing any sizing, placement, or gate code. Numeric accuracy
here is load-bearing: a wrong constant here produces a plausible-looking print
that snaps on the plate.

> **Guardrail.** Do not "clean up" a number you find surprising. Several constants
> in this document are deliberately not what they appear to be (see §3.1, the
> three-way split of a single measured ratio). If a value looks wrong, it is
> either flagged below as a defect to fix, or it is load-bearing for a reason
> recorded here.

---

## 0. The provenance type system (architectural commitment #2)

Every constant carries a typed provenance. The build **fails** if a `Placeholder`
value is reachable from the accept path (the code path that can emit a passing
`DefectReport`).

**The mechanism is normatively specified in `02-target-architecture.md` §3.5** —
four distinct **wrapper types** (`Measured<T>`, `Derived<T>`, `Placeholder<T>`,
`Pending<T>`) plus an `AcceptSafe` trait that `Placeholder` deliberately does not
implement, so a reachable `Placeholder` is a trait-resolution failure, proven by a
`trybuild` compile-fail test (04-migration-plan.md M0 work item 4). `Pending::resolve()`
returns a `Derived<T>` carrying the pending provenance, which is how a `Pending`
value reaches accept at all.

> **Do not implement a runtime `enum Provenance` inside a single `Constant<T>`
> struct.** It was proposed and rejected (02 §16, D-6): a uniformly-typed constant
> admits no trait-absence, so it cannot fail the build and M0's trybuild proof
> becomes unwritable. This document is the authority for **values**; 02 §3.5 is the
> authority for the **mechanism**.

Two rules that matter more than the shape:

1. **Provenance travels with the number, not with a comment.** In Python it lives
   in docstrings and in `SupportConfig.notes()`, which is why it drifted (see §5).
2. **`Pending` is not `Placeholder`.** A `Pending` constant may be reachable from
   the accept path — it ships an interim value and a named open question. It must
   be listed in the run's `DefectReport` metadata so an artifact is traceable to
   the physics vintage that produced it.

**Do not port `SupportConfig.notes()`.** Its two buckets
(`measured_or_charter`, `rough_estimate_pending_calibration`) are wrong today: it
lists `modulus_green_gpa` and `tip_safety_factor` as "measured or charter" when
both are Placeholders, and it separates `peel_pressure_pa` from
`tensile_green_mpa` when the two are one inseparable measured composite.

---

## 1. The load rule

### 1.1 Definition

Define, for one support arm contacting one down-facing region:

```
X = A_supported / A_arm_cross_section          (both mm², X dimensionless)
```

- `A_supported` — the down-facing model area whose peel load that one arm carries.
  In the current engine this is the arm's Voronoi cell on the down-face
  (`placement.load_partition`, exact nearest-eligible-contact face assignment).
- `A_arm_cross_section` — the arm's load-bearing cross-section. On calibration
  plate 5 this was the tip disc `π·d_t²/4`. In code it is `π·r_tip²`.

**Higher X = more area per unit of arm = more likely to snap.**

### 1.2 The measurement — plate 5

| Field | Value |
|---|---|
| Plate | **5** |
| Date | 2026-07-30 |
| Printer | Elegoo Saturn 4 Ultra |
| Resin | black, tank ~30 °C |
| Specimens | **30** ("mushroom cap on a tip"), two independent rows A and B of 15 cells each |
| Design grid | `A_cap ∈ {3.1, 19.6, 122.7} mm²` crossed against `d_t ∈ 0.100 … 2.500 mm` |
| Arm geometry | **tapered cones** (see §4, question (a)) |
| Result | every specimen at **X ≤ 25.3 survived**; every specimen at **X ≥ 25.0 failed** |
| Boundary inversion | exactly one — X = 25.00 failed while X = 25.17 and X = 25.34 survived |
| Survivor span | X = 0.64 … 25.3 |
| Failure span | X = 25 … 6229 |
| Agreement | rows A and B agree independently |

**Measured failure boundary: `X* ∈ [25.00, 25.34]`, one inversion at the boundary.**
Quote it as an interval, not as `25 ± something`; the plate brackets the transition,
it does not resolve inside the bracket. Anything outside `[25.00, 25.34]` is
unambiguous: below survives, above fails, across ~4 decades of X.

**Design target: `X ≤ 12`** — the measured boundary with a 2× safety factor.

Source of record for these numbers: `scripts/support_placement/config.py`,
the block comment above `peel_pressure_pa` (config.py:18–61 in the current
checkout). That comment is the only in-repo write-up of plate 5; treat it as the
plate log.

### 1.3 What the measurement establishes about the law

Three findings, all load-bearing for the Rust design:

**(a) Load scales with supported AREA.** Not with perimeter, and not with a
squeeze-film / Stefan-adhesion law. The plate deliberately crossed cap area
against tip diameter; a perimeter law or a squeeze-film law would not collapse
onto a single `A_supported/A_arm` threshold across 3 cap areas and a 25× span of
tip diameters. It did.

> Corroborating and complicating: the literature review in
> `../reference/literature/LITERATURE-NOTES.md` §"the linear area law is wrong"
> argues from three independent published datasets that separation **force** vs
> cross-section goes as `A^n` with `n ≈ 0.25–0.5`, not `n = 1`. That is a claim
> about the *total plate-level separation force vs the slice image*, measured on
> cross-sections ≥ 8.3 mm². Plate 5 measures *per-arm failure* at cap areas
> 3.1–122.7 mm². These are not the same experiment and the review itself warns
> that applying its `n` down to support-tip scale (0.1–0.8 mm²) is a 1–2 decade
> extrapolation. **Plate 5 governs.** Implement the linear-in-`A_supported` rule;
> record the sublinear literature as a `Pending`-adjacent open question in the
> physics crate docs, not in the sizing path.

**(b) The failure section is the narrowest cross-section of the arm just above
the contact — not the model contact patch.** Observed physically at the base of
the cylinder on the plate-5 specimens. Plate 6 (§4.1) later measured
**constant-diameter cylindrical** arms and found `X*` in the same place, so this is
not a cone-vs-cylinder artifact — but that also means **there is no measured taper
penalty**. The consequences below rest on the observed failure *location*, not on a
sized cone discount, and must not be quoted as if a plate had measured one.
Consequences:

- A long slow taper is a **liability**. The arm must **flare fast above the tip**.
- Sizing one radius at the contact is not sufficient. The Rust arm model must be
  able to answer "where along this column is the minimum cross-section, and what
  is `A_supported / A_min` there?" — the X rule applies at the minimum section,
  wherever it is.
- **The current Python engine does not model this at all.** `placement.tip_radius_mm`
  sizes one radius per contact; everything above it is fixed by `tip_ball_r_factor`
  (1.5), `joint_ball_factor` (1.0) and the stub-to-shoot frustum, with no
  fast-flare term and nothing that locates the minimum section. `config.py` logs
  this as a deliberate non-implementation. **The Rust port must implement it.**

**(c) `X ≤ 12` is necessary, not sufficient.** X is blind to arrangement. Three
collinear arms have zero lateral stability perpendicular to their line, whatever
their X. See §2.4.

### 1.4 How the constant is encoded today, and what Rust must do instead

`placement.tip_radius_mm` sizes the tip as:

```
A_tip = A_supported · peel_pressure_pa · tip_safety_factor / (tensile_green_mpa · 1e6)
r_tip = sqrt(A_tip / π),  clamped to [tip_r_min_mm, tip_r_max_mm]
```

`cco._r_tension_mm` and `trunks.size_trunks` reuse the same expression unclamped;
`verify.load_report` checks the realized safety factor against it. So the X the
engine actually permits is exactly:

```
X_allowed = tensile_green_pa / (peel_pressure_pa · tip_safety_factor)
          = 0.288e6 / (3000 · 8) = 12.0
```

**Only the composite is measured.** Plate 5 pins X and nothing else. It cannot
separate "peel pressure" from "green tensile strength". Neither 0.288 MPa nor
3000 Pa may be quoted as a physical property on its own. The split exists because
`peel_pressure_pa` is *also* read outside the tensile-sizing family — it is the
angle-aware lateral load term in `placement._shaft_r_for_L` /
`placement.max_unbraced_len_mm` — and plate 5 measured nothing about deflection
or bracing.

> **X is a phenomenological index, not a closed stress ratio — which is exactly why
> the engine must never compute a force.** The plate-6 scoring analysis (2026-07-31)
> tried to close it and could not: the measured slender-wall peel pressure (~1 kPa)
> predicts `X*` in the **thousands**, against an observed `X* ≈ 25`. The only
> self-consistent reading is a **cavitation-capped squeeze** of ~100 kPa acting on flat
> patches of ~5 mm and up, which lands ~2–3 MPa at the arm/flare notch — the right
> order for a green-resin snap. Record that as the physical story; do **not** wire it
> into sizing. Engine posture is unchanged and load-bearing: **never compute a force —
> use the ratio.** See §5 conflict 6 for how it sits against the owner's 40–60 kPa
> squeeze-film figure.

> Historical note worth carrying: before the plate-5 re-anchoring, this composed
> to `10e6 / (3000 · 8) = 416.7`, about **17× past the measured snap point**
> `X* ≈ 25` (and ≈ 35× past the `X ≤ 12` design target). `config.py:47-48` states
> this loosely as "35× past the snap point", conflating the target with the
> measurement — the numbers above are the correct reading. That is why the engine
> could certify a layout whose own tips were predicted to break. The re-anchor was
> applied to `tensile_green_mpa` (10.0 → 0.288).

**Rust requirement.** Store `X_ALLOWED = 12.0` as a `Derived<f64>`, derived from
`X_STAR_CONE` (`Measured`, plate 5) with a 2× safety factor and **cross-checked
against `X_STAR_CYLINDER`** (`Measured`, plate 6): 12.0 clears the binding measured
bracket edge with `19.91 / 12 = 1.66×` of margin (§4.1). Exactly as
`02-target-architecture.md` §3.5 declares it. Derive the sizing expression from it:

```rust
// A_arm_min ≥ A_supported / X_allowed   is THE rule.
let a_arm_min_mm2 = a_supported_mm2 / X_ALLOWED.value;
```

Keep `peel_pressure_pa`, `tensile_green_mpa`, `modulus_green_gpa` and
`tip_safety_factor` as **separately-typed `Placeholder` constants used only by the
deflection/bracing family**, never recombined to re-derive X. Recombining them is
how the number drifts: three knobs each individually adjustable, one measured
product. If a future calibration measures peel pressure directly, X_ALLOWED does
not change until a plate says so.

Also record in the type: `tip_safety_factor = 8.0` is retained even though the
design target implies only 2.08 against the measured boundary, so `tensile_green_mpa`
is additionally absorbing ~3.8× of conservatism. It is a **sizing** factor — it
multiplies the load *before* the arm is sized, so raising it makes arms fatter,
never weaker. Despite the "the tip is the fuse" language in
`skills/support-placement/references/design.md` (line 30; the same phrasing recurs in
`design-v4.md` line 302), it is not a weak-link setting.

### 1.5 The two independent defects in the load gate

The Python load gate is unsatisfiable by construction. There are **two separate
causes** and the Rust port must fix both; do not assume one fix covers the other.

**(i) No fixer responds to a load violation (architectural — fix regardless of
plate 6).** `verify.self_verify` (verify.py) ANDs `len(load_violations) == 0` into
`ok`, where `verify.load_report` flags any contact whose realized safety factor is
below `cfg.tip_safety_factor` after the tip radius was clamped at
`cfg.tip_r_max_mm = 1.0`. Nothing in `engine.py` ever adds a contact in response
to a load violation: the fix loop's only continuation condition is
`if not report['uncovered_islands']: break`, and the only repair-candidate
selector (`engine._rank_coverage_repair_rows`) ranks purely by marginal *uncovered
coverage-sample area*. Load violations appear only as a non-increase constraint in
`_coverage_repair_trial_verdict` — they can *reject* a repair but never *request*
one. (`engine._add_fix_contacts`, whose docstring is where the load→fix intent was
written down, is dead code called only from tests, and even it selects by coverage
distance.) A large flat down-face therefore fails `ok` forever.

This is the motivating bug for architectural commitment #3: **violation kinds are
an enum with an exhaustive match, so a gate that can fail without a fixer that can
respond is a compile error.**

**(ii) The data model has no concept of several arms co-carrying one region
(physics — plate 6 measured the formula: sharing is **at least additive**, §4.2).**
`tip_r_max_mm = 1.0` caps what one tip may carry at

```
π · 1.0² · 12 = 37.70 mm²
```

A down-face region larger than that cannot be served by one arm at X ≤ 12, full
stop. The correct answer is *more arms*, and — if load sharing is real — a group
sizing rule. Neither exists in the Python data model. See §4, question (b).

The `tip_r_min_mm = 0.2` floor conversely over-sizes (safely) any contact carrying
less than `π · 0.2² · 12 = 1.51 mm²`.

---

## 2. The scar invariant and where scars land

### 2.1 Total scar area is invariant at `A_face / X`

For a down-face of area `A_face` tiled by `N` arms, each arm with cross-section
`A_arm` carrying a cell of area `A_cell`:

```
A_cell = X · A_arm                    (the load rule, at equality)
N      = A_face / A_cell              (the cells tile the face)
Scar   = N · A_arm  =  A_face / (X · A_arm) · A_arm  =  A_face / X
```

The arm cross-section **cancels**. Thin-and-dense and thick-and-sparse leave
exactly the same total scar area. At `X = 12`:

```
Scar / A_face = 1/12 = 8.33 %
```

**8.33 % of every down-face is scar, at X = 12, regardless of how you tile it.**

Consequences, which the Rust support engine must treat as the design frame:

- **The only two scar levers are `X` and WHERE the scars land.** Debating tip
  diameter as a scar-reduction strategy is arithmetic that has already cancelled.
  Raising X trades scar against snap risk directly and is a physics decision, not
  a tuning one.
- Scar *count* and scar *placement* are free variables; scar *area* is not.
- Therefore the placement objective is a **scar-cost-weighted** placement problem:
  put the invariant 8.33 % where it hurts least. That is what
  `perception.delicacy_field` and `verify.orientation_scar_cost` exist to score,
  and it is why orientation is the coupling hub of the optimization triad.

### 2.2 Hexagonal lattice is the optimal arrangement

For a fixed worst-case gap (covering radius) `R` — i.e. no point of the face is
further than `R` from an arm — the cell areas are:

| Lattice | Cell area at covering radius `R` | Points per unit area |
|---|---|---|
| Triangular / hexagonal | `(3√3/2)·R² ≈ 2.598 R²` | `0.385 / R²` |
| Square | `2 R²` | `0.500 / R²` |

```
0.385 / 0.500 = 0.77  →  hexagonal needs ~23 % fewer points for the same worst-case gap
```

Use a hexagonal (triangular) lattice as the seed arrangement, not a square grid.

### 2.3 Voronoi cell area IS supported area — this gives per-arm sizing directly

On a real curved surface there is no lattice, but the identity survives: **each
arm's Voronoi cell area on the down-face is its `A_supported`**. That converts
placement into sizing with no extra model:

```
for each arm: A_arm_min = area(voronoi_cell(arm)) / X_allowed
```

This is exactly what `placement.load_partition` computes today (exact
nearest-eligible-contact face assignment — deterministic, no Monte Carlo). Keep
the semantics; batch the computation.

**Placement algorithm of record: CVT / Lloyd relaxation weighted by a scar-cost
field.** Seed hexagonally, then relax centroidal-Voronoi-style with the cell
"mass" weighted by delicacy (the scar-cost field), so cells grow over cheap-to-scar
regions and shrink over delicate ones. The load rule then automatically demands
smaller arms where cells are small. Batch-first: one weighted-centroid pass over
all cells per iteration, never per-arm queries in a loop.

### 2.4 X is blind to arrangement — the peel front decides

`X ≤ 12` says nothing about how the arms are laid out relative to each other.
Two failure modes it cannot see:

- **Lateral instability.** Three collinear arms have zero stability perpendicular
  to their line, at any X. Any arrangement gate must include a
  non-degeneracy/spread criterion, not just a per-arm area ratio.
- **Sequencing against the peel front.** On the Elegoo Saturn 4 Ultra the release
  is a **tilt-release**: the peel front sweeps **front-to-back in −Y**, the front
  (−Y) edge travels furthest, and layers release front-to-back. So a group of arms
  serving one region does *not* meet the front simultaneously unless it is
  arranged across the front.

| Arrangement | Relation to the −Y peel front | Predicted loading condition |
|---|---|---|
| Arms strung out along **Y** | front reaches them **one at a time** | **zipper** — each arm meets the front alone, at full X |
| Arms strung out along **X** (constant Y) | front reaches them **together** | **sharing** — the group co-carries, `X_tot/N` governs |

That was plate 6 question (b), and **the measurement came back against the
arrangement hypothesis** (§4.2, 2026-07-31): strips along **both** axes survived at
`X_arm = 60.15` while the matched single arm died at the same X. Sharing is real and
is **at least additive**; the axis-dependent zipper collapse did not happen. Keep the
table as the mechanism the experiment was reasoned from, **not as a rule** — there is
**no orientation term in the engine** until something contradicts §4.2 caveat 5. Two
design notes survive:

1. **Position along Y changes peel load, so randomize over Y and do not tune it.**
   Prior calibration settled that front-to-back sensitivity is ≤ 1 rung of the
   test ladder — real but small. Spend replication budget on build-to-build
   variation instead of on Y position.
2. **The per-arm sizing rule inherited from Python IS the zipper assumption** (each
   arm sized for its own full cell) — now known to be *conservative*, not correct. A
   measured sharing law can only *relax* sizing, never tighten it, so nothing already
   shipped becomes unsafe. But the group rule of §4.2 is what the Rust engine ships,
   because the per-arm rule is precisely what makes the load gate unsatisfiable
   (§1.5(ii)).

---

## 3. The constants

Values verified against `scripts/support_placement/config.py` and the named
modules in the current checkout. Line numbers are as-of-now and will drift — the
name and value are the contract.

**Provenance tags:** `M` = Measured (plate/instrument cited), `D` = Derived,
`P` = **Placeholder — build must fail if reachable from the accept path**,
`Pend` = Pending measurement — **none outstanding**, plate 6 resolved the last two on
2026-07-31 (§4), `F` = Format/interface contract (byte-level, not physics).

### 3.1 The load family — one measured ratio, three knobs

> **⚠ Read as one unit.** `peel_pressure_pa`, `tensile_green_mpa` and
> `tip_safety_factor` are **not** three independent measurements. They are one
> measured ratio split three ways for modelling convenience.

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `X_STAR_CONE` *(new; named `X_STAR_CONE` in 02 §3.5)* | **[25.00, 25.34]** | — | **M** plate 5, 2026-07-30 | `plinth-core::constants::load` | Store the interval, not a midpoint. One boundary inversion at 25.00. Plate-5 arms were **cones**; plate 6 reproduced the boundary on cylinders, so this is **not** a cone artifact — §4.1. |
| `X_STAR_CYLINDER` *(new; named `X_STAR_CYLINDER` in 02 §3.5)* | **(19.91, 30.48)** | — | **M** plate 6, 2026-07-31 | `plinth-core::constants::load` | Cylindrical-arm boundary; geometric centre 24.6, **n = 1 at each bracket edge**, conditional on 4.00 mm arms and a 30 mm² cap. Store the interval, never 24.6. Contains `X_STAR_CONE` — that is what retires the cone-artifact question (§4.1). Was `Pending<f64>` until 2026-07-31. |
| `LOAD_SHARING` *(new; `enum LoadSharing`, 02 §3.6)* | **`Additive { n_eff_cap: 6.0 }`** | — | **M** plate 6, 2026-07-31 | `plinth-support::load` | 12/12 strips survived `X_arm = 60.15` at N = 2,3,4,6 while the matched mono died — sharing is **at least additive**, `Zipper` is refuted. `n_eff_cap = 6.0` is the largest group measured. **All five §4.2 caveats must live in the `note` field.** Was `Pending<LoadSharing>` until 2026-07-31. Categorical payload → no meaningful `err`; see §4.2. |
| `X_ALLOWED` *(new — does not exist in Python)* | **12.0** | — | **D** from `X_STAR_CONE` with a 2× safety factor, margin-checked against `X_STAR_CYLINDER` | `plinth-core::constants::load` | `Derived<f64>`, not `Measured` and not `Pending` — 02 §3.5 declares it exactly this way. All arm sizing derives from it; do not hardcode 12.0 anywhere else. Plate 6 did **not** move it: 12.0 clears the binding measured bracket edge by `19.91/12 = 1.66×` (§4.1). That is a margin check, not an `X*/2` recomputation. |
| `peel_pressure_pa` (config.py:63) | 3000.0 | Pa | **P** — order-of-magnitude only | `plinth-core::constants::deflection` **only** | Literature puts real MSLA peel stress at **12–13 kPa** (`../reference/literature/lit-green-peel.md`), i.e. this may be ~4× low. Used unclamped as the angle-aware lateral load term in `_shaft_r_for_L`. Must **not** be recombined to re-derive X. |
| `tensile_green_mpa` (config.py:91) | 0.288 | MPa | **D** from `X_ALLOWED` — *not* a green-tensile datasheet value | `plinth-core::constants::load` (derived) | Chosen so `0.288e6/(3000·8) = 12.0`. Pre-plate-5 it was 10.0 MPa, which put X_allowed at 416.7. Plausibility check only: pairing X*≈25 with the literature's 12–13 kPa implies ~0.33 MPa, same order. |
| `tip_safety_factor` (config.py:106) | 8.0 | — | **P** — policy, not measured | `plinth-core::constants::load` | Multiplies load *before* sizing → fatter arms. Retained at 8.0 vs the 2.08 the target implies, so `tensile_green_mpa` absorbs ~3.8× extra conservatism. Also the threshold `load_report` compares realized SF against. |
| `modulus_green_gpa` (config.py:95) | 0.5 | GPa | **P** — conservative end of a charter range | `plinth-core::constants::deflection` | Plate 5 measured nothing about stiffness. `../reference/literature/LITERATURE-NOTES.md` §4 measures a green envelope of **0.2–0.8 GPa, median 0.4**; 0.5 sits *above* the measured median, i.e. optimistic, under-predicting deflection. A recorded proposal of **0.35 GPa** has two independent supports. Do not silently change it; make the change a documented decision. |

### 3.2 Arm / tip geometry

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `tip_r_min_mm` (117) | 0.2 | mm | **D** — hobbyist-handling constraint, explicitly *not* physics | `plinth-support::placement` | Over-sizes (safely) any contact carrying < `π·0.2²·12 = 1.51 mm²`. **Owner ruling, carry it in the doc comment:** for this machine and resin **0.15 mm tip *diameter* is an ALLOWED floor — not a target and not a cap**. Larger tips, including diameters **above 0.4 mm**, may safely carry more area and cut support count where load, surface and removal constraints permit. Conversely a thin tip is **not** automatically safe for clearing a diameter floor: load, angle and printability must all pass independently. Published context only, neither a floor nor a limit here: Cao's 0.4 mm minimum printable (that machine/resin, swept 0.1–1.0 mm) and Artemeva's 0.23 mm working tip. Plates 5 and 6 printed tips down to 0.100 mm but measured only the **load** side; thin-tip formability is still uncalibrated. |
| `tip_r_max_mm` (118) | 1.0 | mm | **D** — same band | `plinth-support::placement` | **Caps one tip at `π·1.0²·12 = 37.70 mm²` of supported area at X=12.** Root cause of the unsatisfiable load gate (§1.5). At X=12 this also caps hexagonal arm spacing at `sqrt(37.70·2/√3) = 6.60 mm`. |
| `tip_ball_r_factor` (423) | 1.5 | — | **P** | `plinth-support::placement` | Spherical contact at the tip. Aliased in `verify.BALL_FACTOR_STUB`. |
| `joints.MODEL_FOOT_FACTOR` | 1.4 | — | **P** | `plinth-support::placement` | Strut/rung foot balls. Aliased in `verify.BALL_FACTOR_FOOT`. |
| `joint_ball_factor` (420) | 1.0 | — | **P** — "1.25 read as lumpy" in review | `plinth-support::placement` | |
| `flare_len_mm` / `flare_r_factor` (425/426) | 1.5 / 1.6 | mm / — | **P** | `plinth-support::placement` | Conical flare at the pad. **Not** the fast-flare-above-the-tip term §1.3(b) requires — that term does not exist yet. |
| `contact_depth_mm` (250) | 0.3 | mm | **F** — ChituBox native convention ("start = contact point 0.3 mm INSIDE surface") | `chitubox_io` + `plinth-emit` | |
| `stub_length_mm` (247) | 2.5 | mm | **P** | `plinth-support::placement` | |
| `min_shaft_r_mm` (139) | 0.3 | mm | **P** | `plinth-support::placement` | |
| `max_unbraced_shaft_r_mm` (138) | 0.9 | mm | **D** from corpus — realized "Heavy" shaft radius was 0.75 mm | `plinth-support::placement` | Above this, brace instead of thickening. |
| `min_trunk_r_mm` / `max_trunk_r_mm` (348/349) | 0.8 / 2.4 | mm | **P** — "removal-friendly band" | `plinth-support::placement` | |
| `min_shoot_r_mm` (351) | 0.35 | mm | **P** | `plinth-support::placement` | Also the threshold selecting `branch_angle_deg` over `max_segment_angle_deg`. This is the **shaft/member** minimum and it governs **routing through narrow gaps** — a different question from tip size, and the tip-diameter floor above does **not** set it. Observed consequence: at 0.35 mm the router cannot reach into thin-rib features, which are then left genuinely unsupported. Counter-pressure in the other direction: Cao measured a 0.5 × 0.5 mm as-printed strut fracturing at **4.8 MPa**, so thin struts are real but weak. Resolving this one needs a strut ladder printed on this machine, not more reading. |

### 3.3 Deflection / bracing / transient

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `shaft_drag_force_n` (128) | 0.01 | N | **P** — explicit order-of-magnitude guess, and **it is not a drag force** | `plinth-core::constants::deflection` | Applied identically to a 0.35 mm shoot and a 2.4 mm trunk; no dependence on size, orientation or carried load. Treat `shaft_r` outputs as ordinal ("which shafts need bracing") more than absolute. **Computed properly it is 2–3 decades too large.** Lamb's Stokes solution for crossflow past a cylinder, `F/L = 4πμU / (0.5 − γ − ln(Re/4))`: nominal (μ = 0.2 Pa·s, U = 2 mm/s, r = 0.5 mm, L = 30 mm) gives **2.6×10⁻⁵ N**, i.e. **385× smaller**; an aggressive case (μ = 0.5, U = 20 mm/s, r = 1 mm, L = 50 mm) still gives 1.7×10⁻³ N, 6× smaller. It only reaches 0.01 N at a resin ten times more viscous than ours moving at 50 mm/s. So this is unlabelled safety margin, not viscosity. What actually side-loads a shaft is **peel traction transmitted down from the tips it carries** (peel reaches hundreds of newtons per build; gravity never exceeds 10 N). **Recommendation: do not retune it — delete the mechanism** and derive shaft side-load from the tip loads and their moment arm. It errs safe meanwhile, by an unknown and probably large factor. |
| `max_deflection_mm` (132) | 0.15 | mm | **P** — visual-ribbing serviceability threshold | `plinth-core::constants::deflection` | |
| `brace_free_span_mm` (365) | 18.0 | mm | **D from one measured example** — a 65 mm bare run at r=1 mm deflects ~2.3 mm under 0.01 N, ~15× the budget | `plinth-support::brace` | Real measurement, single case, generalized. **It inherits `shaft_drag_force_n`'s error**: the 0.01 N in that derivation is the row above, so this constant is a live cause of excessive cross-bracing. Re-derive it from the replacement side-load model, do not carry the number across. |
| `brace_neighbor_max_xy_mm` (297) | 15.0 | mm | **P** | `plinth-support::brace` | |
| `brace_reach_max_factor_x` (324) | 1.5 | — | **P** | `plinth-support::brace` | Hard ceiling on widened cross-link reach. |
| `brace_ladder_z_mm` (326) | 8.0 | mm | **P** | `plinth-support::brace` | |
| `brace_r_mm` (327) | 0.5 | mm | **P** | `plinth-support::brace` | |
| `twin_single_brace_xy_mm` (375) | 3.0 | mm | **P** | `plinth-support::brace` | |
| `model_brace_max_len_mm` (382) | 5.0 | mm | **P** | `plinth-support::brace` | |
| transient shoot-count exponent (`transient.r_transient_mm`) | `N^0.125` | — | **D from v3** — `0.5 × 0.25` | `plinth-support::transient` | v3 spelled it two ways; the composite is 1/8. |
| `trunk_merge_radius_mm` / `trunk_branch_merge_xy_mm` (368/413) | 3.0 / 10.0 | mm | **P** | `plinth-support` (dead v3 — not ported) | |
| `trunk_search_radii_mm` (343) | (0, 2, 4, 7, 10, 14, 19, 25) | mm | **P** | `plinth-support::cco` | |
| `trunk_search_directions` (344) | 12 | — | **P** | `plinth-support::cco` | |
| `shoot_max_xy_mm` / `trunk_max_shoots` (333/337) | 9.0 / 10 | mm / — | **P** | `plinth-support` (dead v3 — not ported) | v3-era; see `01-current-system.md` on dead v3 modules. |
| `rewire_time_cap_s` (580) | 30.0 | s | **P — DO NOT PORT** | — | A wall-clock cap makes the optimizer non-deterministic: a faster implementation completes more moves before the cap fires and silently changes topology. Use a deterministic work budget (`perf_rewire_work_budget`) as the only mode. |

### 3.4 Placement density and coverage

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `target_spacing_mm` (148) | 4.0 | mm | **D** — corpus realized median tip spacing 3.1 mm, documented as an **upper bound on required density** (evidence of sufficiency, not optimum) | `plinth-support::placement` | Cross-check: hexagonal cells at 4.0 mm spacing have area `(√3/2)·16 = 13.86 mm²`; at X=12 that needs `A_arm = 1.155 mm²`, i.e. `r = 0.606 mm` — comfortably inside the [0.2, 1.0] band. The spacing default and the load rule are mutually consistent. |
| `spacing_floor_mm` (152) | 2.5 | mm | **D** — "pryable, not too densely packed" hard floor | `plinth-support::placement` | Never place two tips closer, even where load math allows. Also the specialized-merge proximity threshold. |
| `coverage_radius_factor` (609) | 1.5 | — | **P** | `plinth-support::coverage` | Coverage radius = `4.0 × 1.5 = 6.0 mm`. Deliberately looser than spacing so verify doesn't nag in-spec gaps. |
| `min_contacts_per_island` (154) | 1 | — | **P** | `plinth-support::placement` | |
| `island_merge_radius_mm` (230) | 2.0 | mm | **P** — geometric closing radius, explicitly *not* physics | `plinth-support::perceive` | Must stay well under `target_spacing_mm`. |
| `overhang_angle_deg` (210) | 45.0 | ° | **D** — matches the ChituBox `autosupportangle` convention | `plinth-support::perceive` | Down-facing test is `n_z < −sin(45°)`. **The angle is only half an instrument.** `perception.down_facing_mask` judges each face by its **normal alone**, with no notion of how far the overhang spans, so a 2 mm overhang and a 20 mm overhang at the same angle are treated identically. **Geometric extent is the missing second criterion** — carry it as an input to `perceive`, alongside the force-side test that `load.rs` already provides. Literature context for *why*, not for a number: a commercial tool at a fixed 30° failed 4 of 5 models, and tuned per model it needed 55°/30°/49°/41°/46° — a **25° spread** — while still producing 85 % more supports than a force-aware method (`../reference/literature/LITERATURE-NOTES.md`). Cao's **2.4 mm** self-supporting reach is one machine/resin result and is explicitly **not approved as any shipping spacing, reach, span or default** here; see §5 conflict 7. |
| `voxel_pitch_mm` (213) | 0.4 | mm | **D** from v0's working scale (261×175×335 grid on a ~130 mm model) | `plinth-support::perceive` | Also the coverage-patch area floor: `0.4² = 0.16 mm²`. |
| `bridge_free_gap_mm` (217) | 1.0 | mm | **P** — cured-layer stiffness spans a short bridge | `plinth-support::perceive` | |
| `near_plate_mm` (221) | 1.5 | mm | **P** | `plinth-support::perceive` | |
| `perception.ISLAND_ATTACH_REACH_MM` | 0.2 | mm | **D** — tied to `tip_r_min_mm`, deliberately not a config field | `plinth-support::perceive` | Test-pinned (`test_island_attach_reach_mm_is_pinned`). |
| `engine._ISLAND_BIRTH_LINK_R_MM` | 0.6 | mm | **P** *(tuned once, no error bar)* | `plinth-support` | Tuned against the 0.05 mm-layer Weeping Treant corpus. **Its own comment flags that dynamic layer height invalidates the tuning** — see §3.7. |

### 3.5 Clearance, routing, printability

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `standoff_mm` (244) | 0.6 | mm | **D** from v0's 0.6 mm centerline clearance | `plinth-support::cco` | v0 checked only the ray centerline and still had 27/137 violations; v4+ checks the full swept envelope, so this is now genuinely free clearance. |
| `clearance_sample_step_mm` (252) | 0.3 | mm | **P** — legacy sampled path only | `plinth-support::cco` | The sampled clearance path is legacy/test-only; production uses the exact kernel. |
| `surface_pitch_bound_percentile` (278) | 99.0 | — | **M** from a real distribution — Weeping Treant at pitch 0.3 mm: p95 = 0.28, p99 = 0.35, max = 0.61 mm over 400k points | `plinth-support::cco` | 99 (not max) is deliberate: max over 400k points is single-outlier-sensitive. The anchor that crashed `brace_and_size` measured 0.537 mm — inside the tail p95 misses and p99 covers. |
| `max_segment_angle_deg` (293) | 45.0 | ° | **D** from a layer-continuity argument | `plinth-verify` (gate) / `plinth-support::cco` (sizing) | Keeps every layer >90 % seated on cured material for members ≥0.7 mm across, and bounds the peel moment arm. Pure horizontal runs are banned except two-end-supported raft bridges at plate level. |
| `branch_angle_deg` (405) | 60.0 | ° | **P** — widened by fiat (v3.6) | `plinth-verify` (gate) / `plinth-support::cco` (sizing) | **Required pairing:** any member past 45° must be sized with the angle-aware term `F_eff = F_drag + peel · π·r²·tan θ` in `_shaft_r_for_L`. Do not allow the wider cone without the load term. |
| `strut_fallback_max_angle_deg` (390) | 60.0 | ° | **P** | `plinth-support::anchor` | |
| `strut_min_length_saved_mm` (432) | 8.0 | mm | **P** | `plinth-support::anchor` | Minimum material saving to justify a second scar. |
| `strut_max_length_mm` (442) | 20.0 | mm | **D from measurement** — uncapped, 307/353 Treant contacts chose model anchors, most producing clearance violations | `plinth-support::anchor` | |
| `scar_cost_weight` / `material_cost_per_mm` (445/450) | 1.0 / 0.05 | — / per-mm | **P** | `plinth-support::anchor` | Cost-function weights. |
| `lift_mm` (235) | **5.0** | mm | **P** — matches v0 proto's Treant run | `plinth-core::printer` | **⚠ CONFLICT:** `pipeline.prep_one` constructs `SupportConfig(lift_mm=3.0)`, overriding this 5.0 default, *and* accepts a separate emit-time `lift_mm` that affects only emit. `plate_z = mesh.bounds[0][2] − cfg.lift_mm` is computed from the ENGINE cfg, so an emit-time override silently disagrees with the plate the supports were built against. **One value, one owner, in Rust.** |
| `wall_mm` (`pipeline.prep_one` signature) | 2.5 | mm | **P** — bare default, no cited measurement | `plinth-hollow` | |

### 3.6 Embed / scar policy (SPEC-EMBED-POLICY)

These are spec literals. They must be **bit-identical** to any independent audit
implementation, which is the whole point: the engine gate and the audit count the
same population from the same formulas without sharing code.

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `verify.A_ABS_MM2` | 2.0 | mm² | **P from spec** — owner-ratified, no measurement (1.60 mm equivalent scar dia) | `plinth-core::constants::scar` | `../reference/literature/LITERATURE-NOTES.md` explicitly found **no literature support** for this or the next two. |
| `verify.KAPPA` | 1.25 | — | **P from spec** | `plinth-core::constants::scar` | Headroom multiplier on the designed crossing disc. |
| `verify.L_SCAR_MAX_MM` | 2.5 | mm | **P from spec** | `plinth-core::constants::scar` | Max scar patch dimension. |
| `verify.PRINT_FLOOR_CHORD_MM` | 0.08 | mm | **D** — owner-ratified 2026-07-24; a ~20 µm guard band above the 0.0502 mm census population minimum | `plinth-core::constants::scar` | A rule-4 unanchored transit whose chord is below this is MOOT — a foliage knife-edge that cannot print as coherent material. Test-pinned. |
| `verify.MIN_ILLEGAL_LEN_MM` | 0.05 | mm | **P** — sliver cutoff | `plinth-core::constants::scar` | **Ordering is load-bearing:** the length filter is applied *after* tip anchoring, per crossing. A pre-anchoring filter promotes an unanchored transit into the tip slot where a legal scar verdict exempts it. Do not offer a kernel-side pre-anchoring filter at all. |
| `verify.TOL_MM` | 0.05 | mm | **P** — interval anchoring / detection tolerance | `plinth-core::constants::scar` | Same number as above, different concept. Keep them distinct. |
| `verify.EPS_COS` | 1e-9 | — | **D** — divide-by-zero guard, explicitly **not** a physics floor | `plinth-core::constants::scar` | |
| `joints.ENCLOSURE_TOL_MM` | 0.02 | mm | **D** from stated mSLA physical resolution (~0.05 mm layer/pixel pitch) | `plinth-support::joints` | |
| `joints.COINCIDE_TOL_MM` / `ON_AXIS_TOL_MM` / `PLATE_BAND_MM` | 0.15 / 0.2 / 0.2 | mm | **P** | `plinth-support::joints` | `PLATE_BAND_MM` is **duplicated by hand** in `chitubox_emit.py` and mirrored in `verify.connectivity_report`. One constant, one owner, in Rust. |
| `joints.COLINEAR_ANGLE_DEG` / `COLINEAR_RADIUS_FRAC` | 10.0 / 0.20 | ° / — | **P** | `plinth-support::joints` | |
| `p8_chord_canon_micron` (736) | 1 | µm | **D** — canonicalization granularity for cross-implementation equality | `plinth-core::constants::scar` | Integer-micron chord canon. |
| concession standoff ladder (SPEC-P8-REACH Feature B) | (0.3, 0.1) | mm | **P** — owner-authorized 2026-07-23, floored > 0 | `plinth-support::repair` | Locally weakens the clearance envelope to 0.1 mm expecting a human to buff the print. **Not a target design pattern** — the port should need fewer concessions, not reproduce the ladder as load-bearing. |

### 3.7 Printer profile — layer height

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `layer_height_mm` (495) | 0.050 | mm | **M** — printer setting, Saturn 4 Ultra @ 50 µm profile | `plinth-core::printer::PrinterProfile` | **Architectural commitment #5: this is a PER-LAYER ARRAY in the data model from day one**, shipping a constant 0.050 mm until calibrated. It is a scalar in Python. |
| LCD pixel pitch | 14 × 19 | µm | **M** — Saturn 4 Ultra 16K spec | `plinth-core::printer::PrinterProfile` | `19.0` (the coarser axis) is `decimate_pipeline.DEFAULT_PIXEL_UM` and the basis of the decimation error budget. |
| release film | **official Elegoo PFA, 0.127 mm** | mm | **M** — the installed hardware, fitted 2026-07-25 | `plinth-core::printer::PrinterProfile` | **A model input that does not exist in `config.py` at all.** On a tilting machine the film feeds the adhesion term directly, and `../datasheets/printers/README.md` already states that the film changes the peel number. **No paper in the literature set measures PFA** — the available data is FEP (Manizani) and PDMS (Cao), so every peel figure quoted here is a *transfer*, not a match, and fluoropolymer films fare worst under tilt. Stable by inertia: swapping it is a 27-screw job, not a casual change. Record the film alongside any peel figure so a future measurement is attributable to an interface. |
| decimation error budget | p99 ≤ `pixel_um/2000` mm, p99.9 ≤ `pixel_um/1000` mm | mm | **D** from pixel pitch — half a pixel / one pixel | `plinth-decimate` | Measured **two-sided** (decimated face centers → original surface, and original verts → decimated surface). Final full-population re-check applies **1.1× slack** on both — "intentional, not a loosened spec". |
| `minima_connect_layers` (500) | 2.0 | layers | **D** — rule of thumb; measured max real offset ~1.1 layers on the morel/treant corpus, so this keeps headroom | `plinth-support::perceive` | `z_tol = 2.0 × 0.05 = 0.1 mm` **today**. Under a per-layer array this must be computed from the actual local layer heights, not a scalar product. |
| `minima_birth_match_layers` (555) | 2.0 | layers | **D** — same measurement | `plinth-support::perceive` | |
| `minima_eps_mm` (503) | 0.02 | mm | **P** — sub-layer tie tolerance | `plinth-support::perceive` | |
| `minima_contact_radius_mm` (508) | 1.0 | mm | **P** — discretization slack, explicitly **not** a coverage radius | `plinth-support::perceive` | |
| `minima_support_area_tips` (571) | 1.0 | — | **M from a sensitivity sweep** — rescued-site count moves by only 5 across a 1×–8× sweep, so chosen in the flat region | `plinth-support::perceive` | Threshold is `× π·tip_r_min_mm² = 0.1257 mm²`. |
| `minima_require_island_birth` / `minima_drop_interior_faces` (538/549) | True / True | — | **M** — flipped 2026-07-24 after measuring **38.1 % / 21.7 %** false minima-stage contacts (treant / morel) under the old surface-only rule | `plinth-support::perceive` | |

**Per-layer-height propagation checklist.** When layer height stops being a
scalar, these are the constants whose tuning is invalidated and must be
recomputed per-layer rather than once: `minima_connect_layers`,
`minima_birth_match_layers`, `engine._ISLAND_BIRTH_LINK_R_MM` (0.6 mm — its own
comment says so), and `perception.island_births` slicing. Wire them as functions
of the local layer height, not products with a global scalar.

### 3.8 Delicacy / orientation scoring

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `delicacy_curvature_weight` / `delicacy_thinness_weight` (457/458) | 0.6 / 0.4 | — | **P** | `plinth-support::perceive` | Blend of two geometric proxies. |
| `thin_feature_mm` (460) | 1.5 | mm | **P** — saturation threshold | `plinth-support::perceive` | |
| `orient_scar_weight` (481) | 0.4 | — | **P** — "the starting default", additive/opt-in; 0.0 exactly reproduces the pre-scar-term search | `plinth-orient` | Scored as `sum(face_area_mm² × delicacy)` over down-facing faces, normalized `/100.0` to match the existing `sup` term's cm² scale. |
| orientation objective weights (`hollow.orient_search`) | `0.5·support_area_cm² + 3·peak_xsec_cm² + 0.3·height_mm` (+ scar term) | — | **P — ad hoc, DO NOT PORT AS-IS** | `plinth-orient` | 13 hand-picked candidates (identity + ±10/20/30° about X and Y only; **no Z, no combined axes**), scored on a 90 %-decimated proxy while the delicacy field is built on the full mesh. Plan of record is a **dense sweep on a decimated proxy** scoring the support / scar / height triad jointly. |

### 3.9 Removal ergonomics — all Placeholder, all human-calibrated

| Name | Value | Unit | Prov. | Rust owner |
|---|---|---|---|---|
| `grip_handle_r_mm` (585) | 1.0 | mm | **P** | `plinth-verify` (opt-in diagnostic) |
| `grip_handle_len_mm` (588) | 10.0 | mm | **P** | `plinth-verify` (opt-in diagnostic) |
| `grip_clear_mm` (593) | 5.0 | mm | **P** | `plinth-verify` (opt-in diagnostic) |
| `pull_dist_mm` (598) | 15.0 | mm | **P** | `plinth-verify` (opt-in diagnostic) |
| `removal._PEEL_ELEVATIONS_DEG` | (0, 25, 45, 65, 90) | ° | **P** — "a recorded implementation choice, not a config knob" | `plinth-verify` (opt-in diagnostic) |

`config.py`'s own note on this family: *"the calibrator is Craig's hands on the
first v4 print."* None of these has been calibrated. They are only reachable from
the accept path if the removal gate is enabled — and it is **inert today**
(`gate_on_removal` defaults False and no shipped caller passes True, while
`removal_report` is still computed on every verify pass). In Rust, make removal an
**opt-in metric**, not accept-path compute. That also keeps these Placeholders
legally unreachable until someone measures them.

### 3.10 Phase-8 relocation ladder — all Placeholder/tuned

None of these is a physics measurement; they are search-geometry and iteration
caps pinned by SPEC-P8-RELOCATE / SPEC-P8-REACH.

| Name | Value | Unit | Prov. |
|---|---|---|---|
| `p8_reaim_polar_deg` (657) | (0, 10, 20, 30) | ° | **P** — pinned cone grid |
| `p8_reaim_azimuth_deg` (658) | (0, 45, 90, 135, 180, 225, 270, 315) | ° | **P** — pinned enumeration order |
| `p8_reaim_length_factors` (659) | (1.0, 0.75, 0.5) | — | **P** |
| `p8_pocket_voxel_mm` (666) | 6.0 | mm | **D** from adjudicated cluster geometry |
| `p8_pocket_min_members` (667) | 8 | — | **P** |
| `p8_pocket_corridor_pad_mm` (668) | 0.5 | mm | **P** |
| `p8_ball_slide_step_mm` / `p8_ball_slide_max_mm` (672/673) | 0.25 / 2.0 | mm | **P** |
| `p8_max_relocate_sweeps` / `_rounds` / `p8_resize_fixed_point_max_rounds` (681/693/696) | 8 / 4 / 4 | — | **P** — backstop caps, not the termination mechanism |
| `p8_reaim_volume_delta_max_ml` (701) | 0.02 | mL | **P** |
| `p8_reaim_route_growth_max_factor` / `p8_strut_growth_max_factor` (702/703) | 1.50 / 1.25 | — | **P** |
| `p8_reseat_radius_mm` / `p8_reseat_max_seeds` (716/717) | 2.0 / 24 | mm / — | **P** |
| `p8_junction_reseat_step_mm` / `_max_mm` (729/730) | 0.25 / 1.0 | mm | **P** |
| `p8_resin_delta_max` / `p8_max_unresolved` / `p8_max_unresolved_weight` (733/734/735) | 0.03 / 10 / 3.0 | — | **P** — honest-failure bounds |

### 3.11 NOT constants — resource guards

These exist in `config.py` and in module scope but are **not physics and not
tuning levers**. They are numpy/CPython memory-accounting artifacts. In Rust most
of them cease to exist; the rest become runtime configuration, not typed
constants, and they must never appear in a physics module.

`contact_pool_max_refine_depth` (8), `contact_pool_max_virtual_cells` (750 000),
`contact_pool_max_candidates` (2 250 000),
`contact_pool_max_compact_payload_bytes` (144 MiB), `topology_max_faces` (750 000),
`topology_max_compact_payload_bytes` (64 MiB), `topology_max_peak_bytes` (512 MiB),
`coverage_samples_max_*`, `coverage_query_block_size` (65 536),
`coverage_score_matrix_max_entries` (65 536),
`synthetic_coverage_mask_max_words` (131 072),
`coverage_repair_max_attempts` (8), `engine._SYNTHETIC_SELECTOR_MAX_PEAK_BYTES`
(512 MiB), `geometry_exact.ROW_BUDGET` (2 000 000), `RSS_LIMIT_GB` (2.0),
`joints._CERT_MAX_DEPTH` (22), `_CERT_MAX_PATCHES` (2 000 000),
`cco.KDTREE_REBUILD_EVERY` (25), `bracing._MAX_STACKED_FALLBACKS` (6).

Special case: `topology`'s hand-counted per-face/per-vertex byte coefficients
(48f+88v, 84f+32v, 527f+32v, …) are currently **hashed into the topology sidecar's
content digest**, so the sidecar's identity depends on Python allocator
arithmetic. Redefine the digest over the topology arrays only and version-bump the
schema. Do not attempt byte-parity with the Python digest.

### 3.12 The Placeholder alarm list

Constants that are **`Placeholder` AND reachable from today's accept path**. Under
commitment #2 each must be re-provenanced, moved off the accept path, or explicitly
re-typed as `Pending` with a named measurement before the Rust accept path
compiles:

```
peel_pressure_pa            3000.0 Pa     ← literature says ~12–13 kPa; possibly 4× low
tip_safety_factor           8.0           ← policy, absorbs 3.8× of hidden conservatism
modulus_green_gpa           0.5 GPa       ← above the measured 0.2–0.8 GPa median
shaft_drag_force_n          0.01 N        ← order-of-magnitude guess, size-independent
max_deflection_mm           0.15 mm       ← subjective ribbing threshold
A_ABS_MM2 / KAPPA / L_SCAR_MAX_MM
                            2.0 / 1.25 / 2.5   ← spec literals, zero literature support
branch_angle_deg            60.0°         ← widened by fiat; only safe with the angle-aware load term
coverage_radius_factor      1.5           ← no measurement cited
engine._ISLAND_BIRTH_LINK_R_MM
                            0.6 mm        ← tuned once, invalidated by dynamic layer height
grip_* / pull_dist_mm       1.0/10.0/5.0/15.0  ← uncalibrated; keep off the accept path
orientation objective weights 0.5/3/0.3   ← ad hoc; being replaced wholesale
```

The pragmatic path needs no new mechanism: `plinth-core::constants` ships them as
`Placeholder<T>`, and every accept-path function is generic over `AcceptSafe`
(02 §3.5), so each of these is a **compile error** the moment it is wired into
accept. That makes the Placeholder set a visible, shrinking to-do list instead of an
invisible risk. Do **not** invent a `PhysicsProfile` indirection — the trait-absence
mechanism in 02 §3.5 already does this job, and a second enforcement path is a second
thing to keep honest.

---

## 4. MEASURED — calibration plate 6, scored 2026-07-31

Plate 6 was printed **2026-07-30** and **scored 2026-07-31**. Both pre-registered
questions resolved, and a third, unplanned result voided one of the two specimen
families. **Nothing in the port waits on this section any more** — it is the results
record, not a branch design.

| Field | Value |
|---|---|
| Plate | **6** (`md5 2fff011d1cfee56ac0b49b3e759e5a60`) |
| Printed / scored | 2026-07-30 / **2026-07-31** |
| Printer | Elegoo Saturn 4 Ultra |
| Specimens | **34**, in two families |
| Family **A6** | single-arm ladder, **6.00 mm** arms, caps at z **10.95–12.70 mm** — **VOID**, §4.3 |
| Family **B6** | multi-arm sharing, **4.00 mm** arms, caps at z **8.50–9.00 mm**, one cap area **30.00 mm²** |
| Scoring integrity | wash basket **empty** — every failure occurred **in-print**, none in post-print handling |
| Verification | two blind analysts plus an adversarial-refuter panel |

> **`docs/reference/plate-06-results.md` is the record of record** for the plate-6
> numbers (written 2026-07-31, §4.4). It carries the full 34-specimen table, the
> pre-registered gates and the override record; this section carries the
> **engine-facing** resolution of each constant. Where the two disagree, the
> results note governs.

### 4.1 Question (a) — RESOLVED: cylindrical `X*` brackets `(19.91, 30.48)`

**The question was.** Plate 5's arms were **tapered cones**. `A_arm_cross_section` was
computed from the assumed tip diameter `d_t`, but §1.3(b) established that the
failure section is the *narrowest* cross-section just above the contact — which on
a cone is not the tip disc. So `X* ≈ 25` might have been measuring a cone-specific
ratio between the nominal tip and the true minimum section, not a material property.

**The measurement.** Constant-diameter cylindrical arms, so the nominal section and
the minimum section are the same by construction:

| Cell | X | Verdict |
|---|---|---|
| **M3** | **19.91** | **ALIVE** (front row) |
| **M2** | **30.48** | **DEAD** |

**Measured failure boundary: `X*_cylinder ∈ (19.91, 30.48)`**, geometric centre
**24.6**, **n = 1 at each bracket edge**, conditional on **4.00 mm** arms and the
**30 mm²** cap. Quote the bracket, never the centre — this plate brackets the
transition far more loosely than plate 5's 30-specimen ladder did.

**What it establishes.** The cylindrical bracket **contains** plate 5's conical
`X* ∈ [25.00, 25.34]`. **The cone was not the confound:** `X* ≈ 25` reproduces on a
second geometry.

| Constant / branch | Resolution |
|---|---|
| `X_STAR_CYLINDER` | `Pending<f64>` → **`Measured<f64>`**, plate 6, 2026-07-31, carrying the interval `(19.91, 30.48)`. Store the interval, not 24.6. The `note` field must carry all three conditionals: n = 1 per bracket edge, 4.00 mm arms, 30 mm² cap. |
| `X_STAR_CONE` | Unchanged. **Keep it a separate named constant** — the two are measured on different geometries at very different precision, and merging them would launder that difference away. |
| `X_ALLOWED` | **Stays `Derived<f64> = 12.0`.** It is *not* re-derived as `X*/2` from the new bracket: plate 6 is single-specimen at each edge, so the re-derivation is a **margin check** against the binding (lower) edge — `19.91 / 12 = 1.66×` — and 12.0 passes it. **No arm in the engine resizes.** Do not hardcode 12.0 anywhere except this one derivation. |
| `tensile_green_mpa` | Unchanged at 0.288 MPa. `X_ALLOWED` did not move, so its derived carrier does not either. |
| Arm-geometry model | The brackets agree, so **there is no measured taper penalty** and tip-disc sizing stands. The fast-flare requirement of §1.3(b) keeps its original justification — the *observed* failure location — and the minimum-section locator stays **required**, because the X rule applies at the minimum section wherever it is. Neither is carrying a measured cone-vs-cylinder discount, and neither may be sold as if it were. |

**`X_ALLOWED` is still never a `Pending`** — 02 §3.5 declares it `Derived`, and after
2026-07-31 there is no open question left for it to carry.

### 4.2 Question (b) — RESOLVED: sharing is **at least additive**; the zipper model is **refuted**

**The question was.** When N arms serve one down-face region, does the group share the
load (**additive**: the region's total load is divided, so `X_tot/N` governs), or
does each arm meet the peel front alone at its own full X (**zipper**)? §2.4
explains why the printer's −Y tilt-release made this look like an
*arrangement-dependent* question rather than a global one.

**The measurement.** All **12** strip cells survived at per-arm **`X_arm = 60.15`** —
`N ∈ {2, 3, 4, 6}`, arm pitches **2.0 and 2.8 mm**, arm lines along **both X and Y**,
both replicates included — while the matched **mono** cell **M1 died at the same
`X = 60.15`**. Survival tracks the **group** ratio

```
X_tot = A_cap / (N · A_arm)
```

not the per-arm ratio. N arms do co-carry one region. **`LoadSharing::Zipper` is
refuted as a description of this printer's behaviour.**

**Five caveats that MUST travel with the constant** — all of them in the `Measured`
`note` field, not in a comment somewhere else:

1. Validated at **one** cap area (30 mm²), **one** arm diameter (0.798 mm), **one**
   arm length (4.00 mm).
2. **No strip ever failed**, so there is **no measured upper bracket on sharing**. The
   result bounds sharing from below only.
3. Strip arms carry a **~20 % fin-root section bonus** that the mono cells lack. Treat
   sharing as **"at least N×"** and **never design on a super-additive margin** — part
   of the observed headroom is section, not sharing.
4. On this plate `X_tot` is **algebraically degenerate** with total arm area (a single
   cap area was used). The **ratio** form rests on **plate 5's** cap-area spread, not
   on plate 6.
5. **No axis dependence at this load.** Strips along X and along Y both survived, so an
   axis-dependent zipper collapse at `X_arm = 60` is **excluded**. A smaller
   orientation effect *near the boundary* is **untested** — every strip survived, so
   the orientation contrast has zero variance.

**What updates:**

| Constant / branch | Resolution |
|---|---|
| `LOAD_SHARING` | `Pending<LoadSharing>` → **`Measured<LoadSharing>`**, plate 6, 2026-07-31, value **`LoadSharing::Additive { n_eff_cap: 6.0 }`**. Per-arm demand becomes `A_supported_group / (N_eff · X_allowed)`. |
| `n_eff_cap` | **6.0** — the largest group size measured. Caveat 2 gives no upper bracket, so the discount is granted only across the measured range; past N = 6 it is an extrapolation. This is the only free parameter the resolution introduces; it is a `Derived` choice from the plate's design grid, and it belongs in the same `note`. |
| `N_eff` | **All arms rigidly co-carrying the region — no front-band filter.** The pre-registered additive branch defined `N_eff` as "arms within a front-normal (Y) band"; caveat 5 removes that restriction, because sharing appeared along both axes. `N_eff` still requires genuine co-carrying (arms under one contiguous rigid region), never mere proximity. |
| Orientation term | **None.** Do not add a peel-axis term to sizing or placement until something contradicts caveat 5. Retain `peel_dir` as an input to `effective_x` (02 §3.6) so an orientation term can be added later without a signature change, and say in the doc comment that it is currently unused *by measurement*. |
| `LoadSharing::Zipper` | **Kept in the enum.** It is the strictly-conservative branch (per-arm sizing can only over-size) and it is the A/B arm that `cargo test -p plinth-support --test load_sharing_both` and `plinth calibrate` exercise. Refuted is not deleted. |
| `tip_r_max_mm` cap | Stops being a hard wall. A region needing more than 37.70 mm² is served by an `ArmGroup`, not by one fatter tip. **The load-driven fixer of §1.5(i) ships regardless** — a group still has to be *created* in response to a load violation. |
| Placement / CVT | The sharing term is a pluggable weight on the cell-area demand in the §2.3 objective, never an inline constant. It is now a *measured* weight rather than a speculative one, but the plumbing is unchanged. |

```rust
pub static LOAD_SHARING: Measured<LoadSharing> = /* LoadSharing::Additive { n_eff_cap: 6.0 }
    — plate 6, 2026-07-31. 12/12 strips survived X_arm = 60.15 at N = 2,3,4,6 (pitch 2.0
    and 2.8 mm, arm lines along X and along Y) while the matched mono died at the same X.
    Survival tracks X_tot = A_cap/(N*A_arm). AT LEAST additive: never credit more than N.
    Caveats 1-5 of 03 section 4.2 belong verbatim in `note`. */;
```

> **One mechanism wrinkle.** `Measured<T>` (02 §3.5) carries `err: Option<T>`. For a
> **categorical** payload like `LoadSharing`, use `err: None` and serialize it explicitly
> as `"err": null` alongside an explicitly tagged `LoadSharing` variant. Do **not** invent
> a numeric error, omit the field, or widen the enum to carry uncertainty: the real
> uncertainty — all five caveats above — belongs in `note`. A categorical measurement's
> lack of an error bar must stay visible in the type and wire form, not be papered over.

The name is still `LOAD_SHARING`, the payload is still the
`enum LoadSharing { Zipper, Additive { .. } }` from `plinth-support::load` — **not** a
scalar discount factor, and **not** a constant named `MULTI_ARM_SHARING`. Route **all**
group-sizing through it, so the A/B against `Zipper` stays runnable and so the
corpus-wide delta of the 2026-07-31 switch is measurable rather than assumed.

### 4.3 Family A6 is VOID — an unexplained in-print loss above z ≈ 9 mm

Plate 6's single-arm family **A6** (6.00 mm arms, caps at z 10.95–12.70 mm) is **void
by its own pre-registered validity gate**: control cell **KS at X = 8.0**, a
must-survive control, **died**. Nothing in the A6 family may be quoted as a load
measurement, and §4.1's bracket comes from the B6-family mono cells, which sit in the
well-behaved z band.

The failure is not a soft one. **A6-S was wiped 0/11 across X = 8…409** — a **51×
load range** with an identical outcome at every rung. An outcome invariant across 51×
of load is **not a load-capacity outcome**.

**Mechanism: UNIDENTIFIED.** Eliminated by the scoring panel:

- **Post-print handling** — the wash basket was empty, so every loss happened
  in-print; a quantitative inversion argument rules out handling independently.
- **Pure bending / slenderness** — every standard cantilever law misses by **≥ 2×**,
  and the A6-L sub-ladder is **non-monotone** in the direction a bending law requires.
- **Row gradient** — excluded.
- **Cap area** — excluded.

**Leading candidate: an in-print, family-confounded artifact, possibly tied to cap-plane
z.** Every well-behaved cap on this plate sits at **z ≤ 9.2 mm**; every anomalous loss
sits at **z ≥ 10.95 mm**. That split is **100 % confounded with family**, so the plate
cannot separate "z band" from "family A6" — which is exactly why it is a candidate and
not a finding.

**Free arm length is bounded, not measured.** The 4 → 6 mm arm-length change is bounded
to a **≤ ~18 %** effect, but it was never cleanly measured: the **C6** family that
would have isolated it was **dropped from the build**. Do not derive an arm-length term
from this plate.

**Two coupon-design rules follow, and they bind every future calibration plate:**

1. **Never score a feature above z ≈ 9 mm on this printer** until the A6-S artifact is
   understood. Above that band the printer is doing something this plate could not
   explain, and any number measured up there is uninterpretable.
2. **Never place a validity control in a different z band than the specimens it
   validates.** A6's control was in the anomalous band with its specimens, which is why
   the family self-voided cleanly — but the same mistake with the control in the *good*
   band would have certified an anomalous family as valid.

These join the standing coupon rules (mandatory control set, randomized row/column
assignment never by position formula, contiguous row bases, Y position randomized per
§2.4 note 1) and apply to the specimen design of every plate from 7 onward.

### 4.4 Slotting in a results note

**DONE for plate 6** — [`../reference/plate-06-results.md`](../reference/plate-06-results.md),
written 2026-07-31. It carries the bracketing interval for each `X*`, the number of
specimens and inversions, the per-group survival table for question (b), the A6 void
finding and its elimination list, and the printer/resin/temperature conditions, plus
the full 34-specimen table and the three adversarial-panel memos beside it. §4.1–§4.3
above were reconciled against it and agree. **Cite the results note, not this section,
for what the plate measured.**

**The same procedure for every plate after 6:**

1. Update the named `Pending` constants in the physics crate to `Measured`, citing the
   plate, the date and the interval — and put every conditional the plate could not
   remove into the `note` field, not into prose elsewhere.
2. Select the branch of any `Pending` enum. Keep the refuted variant in the enum as the
   conservative fallback and the A/B arm; refuted is not deleted.
3. Re-run the parity harness (see `04-migration-plan.md` / `05-chitubox-and-corpus.md`).
   A physics change moves every artifact, so re-freeze the Python baseline *before*
   comparing, or the comparison measures the physics change rather than the port.
4. Check the specimen design against §4.3's two coupon rules **before** the plate is
   built, not after it is scored.

---

## 5. Conflicts found — resolve in this order

The shared brief from the project owner is authoritative where sources disagree.
Every conflict below was found by reading the repo; each is a real trap for a
fresh reader.

1. **`docs/reference/physics_load_sizing.md` is pre-plate-5 and its constants
   table is STALE.** It lists `tensile_green_mpa = 10.0` MPa (config.py:29) and
   describes `peel_pressure_pa` as "the geometric mean of a plausible 1–5 kPa
   range". Neither statement matches the current `config.py`, where
   `tensile_green_mpa = 0.288` and the comment block is the plate-5 log. **Its
   formula analysis, dimensional checks, and §4 defect list remain valid and are
   worth reading; its numbers are not.**

2. **`docs/reference/literature/LITERATURE-NOTES.md` §"What this changes in our
   config" is also pre-plate-5.** It recommends *raising* `peel_pressure_pa` to
   8–10 kPa and *keeping* `tensile_green_mpa` at 10.0 MPa. Both recommendations
   were overtaken by plate 5, which re-anchored the composite. Its measured
   literature values (peel stress by interface and speed, green modulus envelope
   0.2–0.8 GPa, Cao's 4.8 MPa as-printed strut fracture stress) are good primary
   data and are cited above. Its config recommendations are not current guidance.

3. **RESOLVED 2026-07-31** — `docs/datasheets/README.md` used to quote a
   `config.py` comment that no longer exists (the "geometric mean of a plausible
   1–5 kPa range" line). The README was rewritten: the stale quote is gone, and
   the taxonomy is now **four-way** (`specs` = law, `datasheets` = vendor and
   hardware data, `reference` = our own findings, artifacts = not in git).
   Adopt the four-way split; the physics literature moved from
   `datasheets/physics/` to `reference/literature/` in the same change.

4. **The linear vs sublinear area law** — §1.3(a). Plate 5 (per-arm failure,
   direct) governs over the literature's `A^n, n ≈ 0.25–0.5` (plate-level
   separation force, extrapolated 1–2 decades down to tip scale).

5. **`lift_mm` has two live values** — `config.SupportConfig.lift_mm = 5.0` vs
   `pipeline.prep_one`'s `SupportConfig(lift_mm=3.0)`, plus a third emit-time
   override that affects only emit while `plate_z` is computed from the engine
   cfg. §3.5. One owner in Rust.

6. **Squeeze-film suction, 40–60 kPa at 30–40 °C.** The owner's brief records
   this figure. **No in-repo source states it.** The nearest repo corroboration is
   `LITERATURE-NOTES.md`: Cao measured 45 kPa on a **PDMS** vat (a materially
   stickier interface than FEP) and Materialise e-Stage ships a 45 kPa default.
   Both are the right order. Record it as an owner-supplied `Measured` value with
   this caveat, and note that **it is not the number the code uses** — the code's
   `peel_pressure_pa = 3000 Pa` sits an order of magnitude below it, which is one
   more reason that constant must never be quoted as a physical property.
   **Plate 6's scoring analysis lands in the same band from a different direction**
   (§1.4): the only self-consistent reading of `X* ≈ 25` is a cavitation-capped
   squeeze of ~100 kPa on flat patches of ~5 mm and up. Same order, independently
   arrived at — and still not a number the sizing path may use.

7. **Bridge/sag limit ~3 mm at 0.300 mm section.** The owner's brief records this
   as approximate. In-repo, `bridge_free_gap_mm = 1.0 mm` is the only self-support
   gap constant and it has no measured provenance;
   `LITERATURE-NOTES.md` reports a **2.4 mm** maximum self-supporting overhang
   distance from a different setup, explicitly flagged as setup-specific rather
   than a universal reach. Treat the ~3 mm figure as `Placeholder` with an
   approximate owner attribution, and do not derive anything load-bearing from it.

8. **`SupportConfig.notes()` mis-classifies its own constants** — §0. Do not port
   its buckets.

9. **`chitubox_emit.PLATE_BAND_MM` is a hand-maintained duplicate** of
   `joints.PLATE_BAND_MM` / `verify.connectivity_report`'s rule, kept independent
   "per the module's style". Nothing enforces they stay equal. One typed constant
   in Rust.

10. **Deliberate duplication that must be KEPT** — the SCAR-AREA-CAP and
    PRINT-FLOOR constants are copied by hand into the independent audit
    (`scripts/tools/census_run.py`) so the engine and the audit share no code.
    That independence is the point. If a Rust-side independent audit survives the
    port, keep the duplication **and add an automated equality assertion** so a
    change in one place is caught rather than silently divergent.

---

## 6. Where to read further

Supplemental depth only — this document is self-sufficient and nothing below is a
prerequisite.

| Path | What it adds | Caveat |
|---|---|---|
| `../reference/physics_load_sizing.md` | The most complete formula-by-formula audit: every sizing law, dimensional checks, the three not-obviously-reconciled power laws, and the "only peel is modeled — no gravity, no buoyancy, no density, no viscosity" finding. | Constants table is pre-plate-5 (conflict 1). |
| `../reference/physics_contacts_tips.md` | Contact/tip geometry reasoning behind the tip band and ball factors. | |
| `../reference/physics_coverage.md` | The A3 coverage model — sampling, patch areas, why the gate is area-weighted. | |
| `../reference/physics_printability.md` | The angle limits, and the clearest statement of what "SF 8" is a safety factor *of*, plus radius sensitivities (`√F` for tension-floored members, `F^¼` for deflection-floored). | **Its removal-timing aside is refuted.** It argues supports are snapped off *after* post-curing, "when the resin is 4–6× stronger". Artemeva measured the opposite practice as better on **both** axes — removing before post-processing rated 1/5 (very easy) versus 3/5 after, at both support densities, *and* gave dimensions closer to nominal — and Riccio's measured post-cure gain is **1.3–3.2×**, not 4–6× (`../reference/literature/LITERATURE-NOTES.md`). Constants table is also pre-plate-5 in the same way as conflict 1. |
| `../reference/physics_transient_peel.md` | The build-time transient model: restraint activation sweep, worst free-cantilever span, `N^0.125`. | |
| `../datasheets/README.md` | The `specs` / `datasheets` / artifacts taxonomy — where a new physics number belongs. | Quotes a stale comment (conflict 3). |
| `../reference/literature/LITERATURE-NOTES.md` | Primary-source measurements with test conditions attached to every figure: peel stress vs speed and interface, green modulus envelope, green tensile vs degree of cure, a measured as-printed strut fracture stress. | Config recommendations are pre-plate-5 (conflict 2). |
| `../reference/literature/lit-green-peel.md` | Earlier confidence-classed review (MEASURED-PEER-REVIEW / MEASURED-COMMUNITY / RULE-OF-THUMB / MARKETING tags). Source of the 12–13 kPa FEP peel figure and of the "no source quantifies sealed-cavity blowout" gap. | |
| `../datasheets/resins/RESIN-ENVELOPE.md` | Cross-manufacturer property envelope for the water-washable ABS-like class; flexural modulus spans ~790–2500 MPa, and **every** manufacturer number is post-cured or of unstated cure state. | |
| `../datasheets/printers/README.md` | Printer physical parameters. | |
| `../specs/SPEC-EMBED-POLICY.md` | Normative SCAR-AREA-CAP formula and the PRINT-FLOOR amendment. The law behind §3.6. | |
| `../reference/TREANT-V4-PRINT-AUDIT.md` | Plain-language audit of what the v4 engine does and does not model. | |

Sibling guide documents: `README.md`, `01-current-system.md`,
`02-target-architecture.md`, `04-migration-plan.md`, `05-chitubox-and-corpus.md`,
`06-conventions.md`.
