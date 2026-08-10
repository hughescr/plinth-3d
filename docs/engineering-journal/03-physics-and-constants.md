# 03 — Physics and Constants

Every physical rule and every numeric constant the Rust core must encode, with
provenance. This document is the authority for **values**. `02-target-architecture.md`
is the authority for **crate and type names** and for the provenance **mechanism**; if
the two ever disagree, follow 02 for the name and this document for the number.

`m0-constants-source-ledger.json` is a separate authority for **membership and
source disposition**: it says which live legacy literals become future runtime
declarations, aliases, retired rows, or exclusions. Its source-value snapshots are
nonnormative. The future generated `constants.md` is only a view of the runtime
registry. Neither file may override a value, shape, unit, or grade stated here.

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

Those are exactly the four wrappers. “Format”, “fixed”, “policy”, and
“owner-ratified” describe source or acceptance disposition; none is a fifth
provenance grade.

`Measured<T>` carries a required tagged `EvidenceSource`, not loose `plate` and
`date` strings. Calibration-plate evidence names its plate, repo-relative record,
stable locator, and `EvidenceDate`; vendor, installed-hardware, corpus-study,
project-record, and owner-decision evidence instead name a validated `EvidenceId`
plus the same record/locator/date fields. Dates are explicitly `Exact`,
`OnOrBefore`, or `Unknown`. `Unknown` has no date and is reserved for genuinely
unsourced evidence. Vendor evidence points to a checked-in datasheet that contains
the URL/retrieval metadata; runtime provenance never embeds a raw URL.

The measured wire has exactly `value`, `source`, `err`, and `note`, with `source`
and `err` required. `FiniteInterval` represents measured brackets using finite
`lower < upper` endpoints and explicit `BoundKind::{Inclusive, Exclusive}` on
each end. Intervals and categorical measurements serialize `"err": null`; their
uncertainty is in the interval or the condition-bearing note, not a fabricated
numeric error bar.

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
plate log. The source date is **`OnOrBefore { date: 2026-07-30 }`**: that is the
known upper bound, not an invented exact print/scoring date. Store the value as a
`FiniteInterval` with inclusive/inclusive bounds and explicit `err: null`.

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

**Rust requirement.** Store `X_ALLOWED = 12.0` as a `Derived<f64>`, with the
binding relation `floor(lower(X_STAR_CONE) / 2) = floor(25.00 / 2) = 12`, and **cross-check it
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

### 2.4 X is blind to arrangement — release-front sequencing was tested

`X ≤ 12` says nothing about how the arms are laid out relative to each other.
Two failure modes it cannot see:

- **Lateral instability.** Three collinear arms have zero stability perpendicular
  to their line, at any X. Any arrangement gate must include a
  non-degeneracy/spread criterion, not just a per-arm area ratio.
- **Sequencing against the release front.** On the Elegoo Saturn 4 Ultra,
  `leading_release_edge = NegativeY` and
  `release_front_sweep_direction = PositiveY`: the release front moves from
  `-Y` to `+Y`. Never call this a “-Y sweep” or collapse the two fields into a
  generic peel direction. The mechanism hypothesis was that a group serving one
  region would not meet the moving front simultaneously unless arranged across it.

| Arrangement | Relation to the `-Y → +Y` release front | Predicted loading condition |
|---|---|---|
| Arms strung out along **Y** | front reaches them **one at a time** | **zipper** — each arm meets the front alone, at full X |
| Arms strung out along **X** (constant Y) | front reaches them **together** | **sharing** — the group co-carries, `X_tot/N` governs |

That was plate 6 question (b), and **the measurement came back against the
arrangement hypothesis** (§4.2, 2026-07-31): strips along **both** axes survived at
`X_arm = 60.15` while the matched single arm died at the same X. Sharing is real and
is **at least additive**; the axis-dependent zipper collapse did not happen. Keep the
table as the mechanism the experiment was reasoned from, **not as a rule** — there is
**no orientation term in the engine** until something contradicts the separate
orientation limitation in §4.2. The axis effect input remains plumbed but is unused
today. Two
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

**Provenance tags:** `M` = Measured, with tagged evidence; `D` = Derived;
`P` = **Placeholder — build must fail if reachable from the accept path**; and
`Pend` = Pending measurement. These map one-to-one to the four wrappers in §0.
There is no `F`/format wrapper. Acceptance disposition is stated separately. In
the legacy load-split family specifically — `PEEL_PRESSURE_PA`,
`TENSILE_GREEN_MPA`, `TIP_SAFETY_FACTOR`, `MODULUS_GREEN_GPA`, and `X_ALLOWED`
— the first four are diagnostic Placeholders and **only `X_ALLOWED = 12.0` is
Derived and accept-safe**. Evidence-backed/fixed contracts such as the X brackets,
load-sharing result, and printer hardware profile retain their own stated
accept-safe dispositions.

### 3.1 The load family — one measured ratio, three knobs

> **⚠ Read as one unit.** `peel_pressure_pa`, `tensile_green_mpa` and
> `tip_safety_factor` are **not** three independent measurements. They are one
> measured ratio split three ways for modelling convenience.

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `X_STAR_CONE` *(new; named `X_STAR_CONE` in 02 §3.5)* | **[25.00, 25.34]**, inclusive/inclusive `FiniteInterval` | — | **M** calibration plate 5, `OnOrBefore(2026-07-30)`, `err: null` | `plinth-core::constants::load` | Store the interval, not a midpoint. One boundary inversion at 25.00. Plate-5 arms were **cones**; plate 6 reproduced the boundary on cylinders, so this is **not** a cone artifact — §4.1. |
| `X_STAR_CYLINDER` *(new; named `X_STAR_CYLINDER` in 02 §3.5)* | **(19.91, 30.48)**, exclusive/exclusive `FiniteInterval` | — | **M** calibration plate 6, `Exact(2026-07-31)`, `err: null` | `plinth-core::constants::load` | Cylindrical-arm boundary; geometric centre 24.6, **n = 1 at each bracket edge**, conditional on 4.00 mm arms and a 30 mm² cap. Store the interval, never 24.6. Contains `X_STAR_CONE` — that is what retires the cone-artifact question (§4.1). Was `Pending<FiniteInterval>` until 2026-07-31. |
| `LOAD_SHARING` *(new; explicitly serde-tagged `enum LoadSharing`, 02 §3.6)* | **`Additive { n_eff_cap: 6.0 }`** | — | **M** plate 6, 2026-07-31 | `plinth-core::load` (type now); `plinth-core::constants::load` (constant, later M0); `plinth-support::load` (later reexports/consumes) | The passive validated type lives in core now; the `LOAD_SHARING` declaration is later M0 constants work. `plinth-support::load` later reexports the type and owns `ArmGroup` / `effective_x` behaviour. 12/12 strips survived `X_arm = 60.15` at N = 2,3,4,6 while the matched mono died — sharing is **at least additive**, `Zipper` is refuted. `n_eff_cap = 6.0` is the largest group measured and must validate finite and `>= 1.0`. **All five §4.2 caveats must live in the `note` field.** Was `Pending<LoadSharing>` until 2026-07-31. A `Measured` value requires its date. Categorical payload → `err: None`, serialized exactly as `"err": null`; see §4.2. |
| `X_ALLOWED` *(new — does not exist in Python)* | **12.0** | — | **D** from `[X_STAR_CONE]`: `floor(lower(X_STAR_CONE)/2) = floor(25.00/2) = 12`; margin-checked against `X_STAR_CYLINDER` | `plinth-core::constants::load` | `Derived<f64>`, not `Measured` and not `Pending` — 02 §3.5 declares it exactly this way. All arm sizing derives from it; do not hardcode 12.0 anywhere else. Plate 6 did **not** move it: 12.0 clears the binding measured bracket edge by `19.91/12 = 1.66×` (§4.1). That is a margin check, not a derivation input. |
| `PEEL_PRESSURE_PA` (config.py:63) | 3000.0 | Pa | **P**, diagnostic only — order-of-magnitude legacy split | `plinth-core::constants::diagnostics` | Literature puts real MSLA peel stress at **12–13 kPa** (`../reference/literature/lit-green-peel.md`), i.e. this may be ~4× low. Must **not** be recombined to re-derive X or reach acceptance. |
| `TENSILE_GREEN_MPA` (config.py:91) | 0.288 | MPa | **P**, diagnostic only — algebraic legacy split, *not* a derived or datasheet property | `plinth-core::constants::diagnostics` | Chosen so `0.288e6/(3000·8) = 12.0`, but that historical arithmetic does not promote it to `Derived`. Pre-plate-5 it was 10.0 MPa. |
| `TIP_SAFETY_FACTOR` (config.py:106) | 8.0 | — | **P**, diagnostic only — policy, not measured | `plinth-core::constants::diagnostics` | Multiplies the legacy load before sizing. Retained for diagnosis only; the accept-safe rule reads `X_ALLOWED` directly. |
| `MODULUS_GREEN_GPA` (config.py:95) | **0.5** | GPa | **P**, diagnostic only | `plinth-core::constants::diagnostics` | This is the live legacy value. Plate 5 measured nothing about stiffness. Historical proposals including **0.35 GPa** do not change the source inventory; changing 0.5 needs a separate documented decision. |

### 3.2 Arm / tip geometry

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `TIP_RADIUS_DEFAULT_MM` (legacy `tip_r_min_mm`, config.py:117) | **0.200** | mm | **P**, diagnostic generation default | `plinth-support::placement` | The legacy name falsely presents the default as the validator minimum. It over-sizes (safely) any contact carrying < `π·0.2²·12 = 1.51 mm²`, but it is only the starting radius. |
| `TIP_DIAMETER_MIN_ALLOWED_MM` | **0.150** | mm | **M** owner decision; diagnostic until the consumer migration | `plinth-support::placement` / `plinth-verify` | Hard validator floor for this machine/resin, not a target and not a cap. Larger diameters, including >0.4 mm, may safely carry more area. Thin-tip formability remains separately uncalibrated. |
| `TIP_RADIUS_MIN_ALLOWED_MM` | **0.075** | mm | **D** from `TIP_DIAMETER_MIN_ALLOWED_MM / 2`, diagnostic | `plinth-support::placement` / `plinth-verify` | Mechanical conversion used by radius-valued APIs. **Do not substitute the 0.200 mm default.** |
| `tip_r_max_mm` (118) | 1.0 | mm | **P**, diagnostic legacy cap | `plinth-support::placement` | **Caps one tip at `π·1.0²·12 = 37.70 mm²` of supported area at X=12.** Root cause of the unsatisfiable load gate (§1.5). At X=12 this also caps hexagonal arm spacing at `sqrt(37.70·2/√3) = 6.60 mm`. |
| `tip_ball_r_factor` (423) | 1.5 | — | **P** | `plinth-support::placement` | Spherical contact at the tip. Aliased in `verify.BALL_FACTOR_STUB`. |
| `joints.MODEL_FOOT_FACTOR` | 1.4 | — | **P** | `plinth-support::placement` | Strut/rung foot balls. Aliased in `verify.BALL_FACTOR_FOOT`. |
| `joint_ball_factor` (420) | 1.0 | — | **P** — "1.25 read as lumpy" in review | `plinth-support::placement` | |
| `flare_len_mm` / `flare_r_factor` (425/426) | 1.5 / 1.6 | mm / — | **P** | `plinth-support::placement` | Conical flare at the pad. **Not** the fast-flare-above-the-tip term §1.3(b) requires — that term does not exist yet. |
| `DEFAULT_CONTACT_DEPTH_MM` (legacy `contact_depth_mm`, 250) | 0.3 | mm | **P** — generation/profile default | `plinth-support::placement` + `plinth-emit` | This is not a ChituBox format grade or fixed wire value; the format field is variable. Keep the 0.3 mm policy in geometry generation, not `chitubox_io` format constants. |
| `stub_length_mm` (247) | 2.5 | mm | **P** | `plinth-support::placement` | |
| `min_shaft_r_mm` (139) | 0.3 | mm | **P** | `plinth-support::placement` | |
| `max_unbraced_shaft_r_mm` (138) | 0.9 | mm | **P** — corpus-rationale policy; realized "Heavy" radius 0.75 mm does not derive the ceiling | `plinth-support::placement` | Above this, brace instead of thickening. |
| `min_trunk_r_mm` / `max_trunk_r_mm` (348/349) | 0.8 / 2.4 | mm | **P** — "removal-friendly band" | `plinth-support::placement` | The Rust owner column is true only for `min_trunk_r_mm`, read by `plinth-support::member_sizing`/`cco`. `max_trunk_r_mm` is **registered but unread**: its only reader was `trunks.py:size_trunks`, dead v3 code deleted with the Python migration (§3.3), and the ceiling was never carried forward. |
| `min_shoot_r_mm` (351) | 0.35 | mm | **P** | `plinth-support::placement` | Also the threshold selecting `branch_angle_deg` over `max_segment_angle_deg`. This is the **shaft/member** minimum and it governs **routing through narrow gaps** — a different question from tip size, and the tip-diameter floor above does **not** set it. Observed consequence: at 0.35 mm the router cannot reach into thin-rib features, which are then left genuinely unsupported. Counter-pressure in the other direction: Cao measured a 0.5 × 0.5 mm as-printed strut fracturing at **4.8 MPa**, so thin struts are real but weak. Resolving this one needs a strut ladder printed on this machine, not more reading. |

### 3.3 Deflection / bracing / transient

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| legacy `shaft_drag_force_n` (128) | 0.01 | N | **RETIRED — no runtime declaration** | — | Applied identically to every shaft and not a physical drag law. Delete the mechanism and derive side-load from the actual tip load path and moment arm; do not retune or port 0.01. |
| `max_deflection_mm` (132) | 0.15 | mm | **P** — visual-ribbing serviceability threshold | `plinth-core::constants::deflection` | |
| legacy `brace_free_span_mm` (365) | 18.0 | mm | **RETIRED — no runtime declaration** | — | It depends on the retired 0.01 N pseudo-drag. Re-derive a span rule from the replacement load model; do not carry 18.0 across. |
| `brace_neighbor_max_xy_mm` (297) | 15.0 | mm | **P** | `plinth-support::brace` | |
| `brace_reach_max_factor_x` (324) | 1.5 | — | **P** | `plinth-support::brace` | Hard ceiling on widened cross-link reach. |
| `brace_ladder_z_mm` (326) | 8.0 | mm | **P** | `plinth-support::brace` | |
| `brace_r_mm` (327) | 0.5 | mm | **P** | `plinth-support::brace` | |
| `twin_single_brace_xy_mm` (375) | 3.0 | mm | **P** | `plinth-support::brace` | |
| `model_brace_max_len_mm` (382) | 5.0 | mm | **P** | `plinth-support::brace` | |
| transient shoot-count exponent (`transient.r_transient_mm`) | `N^0.125` | — | **P** — inherited v3 heuristic; multiplying `0.5 × 0.25` does not establish physical authority | `plinth-support::transient` | v3 spelled it two ways; the composite is 1/8. |
| `trunk_merge_radius_mm` / `trunk_branch_merge_xy_mm` (368/413) | 3.0 / 10.0 | mm | **P** | `plinth-support` (dead v3 — not ported) | |
| `trunk_search_radii_mm` (343) | (0, 2, 4, 7, 10, 14, 19, 25) | mm | **P** | `plinth-support::cco` | |
| `trunk_search_directions` (344) | 12 | — | **P** | `plinth-support::cco` | |
| `shoot_max_xy_mm` / `trunk_max_shoots` (333/337) | 9.0 / 10 | mm / — | **P** | `plinth-support` (dead v3 — not ported) | v3-era; see `01-current-system.md` on dead v3 modules. |
| legacy `rewire_time_cap_s` (580) | 30.0 | s | **RETIRED — no-port** | — | A wall-clock cap makes the optimizer non-deterministic: a faster implementation completes more moves before the cap fires and silently changes topology. Rust has no wall-clock fallback. |
| `REWIRE_WORK_BUDGET` (legacy alias `config.perf_rewire_work_budget`) | `TrialSetting<NonZeroU64>::Disabled` | move evaluations | **P** — disabled exact-search baseline | `plinth-support::rewire` | No numeric enabled budget is selected. `Disabled` runs exact search to convergence, never the retired 30-second fallback. Any future `Enabled` value must be deterministic, nonzero, and evidence-backed. |

### 3.4 Placement density and coverage

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `target_spacing_mm` (148) | 4.0 | mm | **P** — corpus realized median 3.1 mm shows sufficiency, not an optimum or derived boundary | `plinth-support::placement` | Cross-check: hexagonal cells at 4.0 mm spacing have area `(√3/2)·16 = 13.86 mm²`; at X=12 that needs `A_arm = 1.155 mm²`, i.e. `r = 0.606 mm` — comfortably inside the [0.2, 1.0] band. The spacing default and the load rule are mutually consistent. |
| `spacing_floor_mm` (152) | 2.5 | mm | **P** — selected "pryable, not too densely packed" ergonomics policy | `plinth-support::placement` | Never place two tips closer, even where load math allows. Also the specialized-merge proximity threshold. |
| `coverage_radius_factor` (609) | 1.5 | — | **P** | `plinth-support::coverage` | Coverage radius = `4.0 × 1.5 = 6.0 mm`. Deliberately looser than spacing so verify doesn't nag in-spec gaps. |
| `min_contacts_per_island` (154) | 1 | — | **P** | `plinth-support::placement` | |
| `island_merge_radius_mm` (230) | 2.0 | mm | **P** — geometric closing radius, explicitly *not* physics | `plinth-support::perceive` | Must stay well under `target_spacing_mm`. |
| `overhang_angle_deg` (210) | 45.0 | ° | **P** — legacy ChituBox convention, not a measurement or binding derivation | `plinth-support::perceive` | Down-facing test is `n_z < −sin(45°)`. **The angle is only half an instrument.** `perception.down_facing_mask` judges each face by its **normal alone**, with no notion of how far the overhang spans, so a 2 mm overhang and a 20 mm overhang at the same angle are treated identically. **Geometric extent is the missing second criterion** — carry it as an input to `perceive`, alongside the force-side test that `load.rs` already provides. Literature context for *why*, not for a number: a commercial tool at a fixed 30° failed 4 of 5 models, and tuned per model it needed 55°/30°/49°/41°/46° — a **25° spread** — while still producing 85 % more supports than a force-aware method (`../reference/literature/LITERATURE-NOTES.md`). Cao's **2.4 mm** self-supporting reach is one machine/resin result and is explicitly **not approved as any shipping spacing, reach, span or default** here; see §5 conflict 7. |
| `voxel_pitch_mm` (213) | 0.4 | mm | **P** — legacy working-scale policy inferred from one v0 corpus run | `plinth-support::perceive` | Also the coverage-patch area floor: `0.4² = 0.16 mm²`. |
| `BRIDGE_FREE_GAP_MM` (legacy `bridge_free_gap_mm`, 217) | 1.0 | mm | **P** — free-gap perception default | `plinth-support::perceive` | A local unsupported-gap rule. It is not the bridge/sag span below. |
| `BRIDGE_SAG_SPAN_APPROX_MM` *(new)* | ~3.0, conditional on a 0.300 mm section | mm | **P** — owner-supplied approximate limit | `plinth-emit` / `plinth-verify` diagnostics | Keep the span and its section condition together. It does not replace or derive from `BRIDGE_FREE_GAP_MM = 1.0`. |
| `near_plate_mm` (221) | 1.5 | mm | **P** | `plinth-support::perceive` | |
| `perception.ISLAND_ATTACH_REACH_MM` | 0.2 | mm | **P** — tuning tied to a legacy Placeholder tip default | `plinth-support::perceive` | Test-pinned (`test_island_attach_reach_mm_is_pinned`). |
| `engine._ISLAND_BIRTH_LINK_R_MM` | 0.6 | mm | **P** *(tuned once, no error bar)* | `plinth-support` | Tuned against the 0.05 mm-layer Weeping Treant corpus. **Its own comment flags that dynamic layer height invalidates the tuning** — see §3.7. |

### 3.5 Clearance, routing, printability

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `standoff_mm` (244) | 0.6 | mm | **P** — legacy clearance policy; corpus use does not derive the selected distance | `plinth-support::cco` | v0 checked only the ray centerline and still had 27/137 violations; v4+ checks the full swept envelope, so this is now genuinely free clearance. |
| `clearance_sample_step_mm` (252) | 0.3 | mm | **P** — legacy sampled path only | `plinth-support::cco` | The sampled clearance path is legacy/test-only; production uses the exact kernel. |
| `surface_pitch_bound_percentile` (278) | 99.0 | — | **RETIRED — no-port** | — | Sampled-cloud pitch uncertainty is identically zero on exact triangle paths; `SPEC-V5-GEOM` §2.1/§2.6 retires this control. |
| `max_segment_angle_deg` (293) | 45.0 | ° | **P** — qualitative layer-continuity policy, not measured or binding-derived | `plinth-verify` (gate) / `plinth-support::cco` (sizing) | Keeps every layer >90 % seated on cured material for members ≥0.7 mm across, and bounds the peel moment arm. Pure horizontal runs are banned except two-end-supported raft bridges at plate level. |
| `branch_angle_deg` (405) | 60.0 | ° | **P** — widened by fiat (v3.6) | `plinth-verify` (gate) / `plinth-support::cco` (sizing) | **Required pairing:** any member past 45° must be sized with the angle-aware term `F_eff = F_drag + peel · π·r²·tan θ` in `_shaft_r_for_L`. Do not allow the wider cone without the load term. |
| `strut_fallback_max_angle_deg` (390) | 60.0 | ° | **P** | `plinth-support::anchor` | |
| `strut_min_length_saved_mm` (432) | 8.0 | mm | **P** | `plinth-support::anchor` | Minimum material saving to justify a second scar. **Registered but unread** in `plinth-support::anchor`: it gated `struts.py`'s second-scar cost tradeoff, dead Python deleted with the migration; the ported anchor module accepts the cheapest candidate by real volume delta instead (`design-v4.md`), not by a length-saved threshold. |
| `strut_max_length_mm` (442) | 20.0 | mm | **P** — corpus outcomes do not establish a safe maximum | `plinth-support::anchor` | Uncapped, 307/353 Treant contacts chose model anchors, most producing clearance violations; that rationale does not make 20.0 AcceptSafe. |
| `scar_cost_weight` / `material_cost_per_mm` (445/450) | 1.0 / 0.05 | — / per-mm | **P** | `plinth-support::anchor` | Cost-function weights. **Registered but unread** in `plinth-support::anchor`: both priced `struts.py`'s two-scar cost function (`material_cost_per_mm × length + scar_cost_weight × delicacy`), dead Python deleted with the migration; the ported anchor module accepts the cheapest candidate by real volume delta instead (`design-v4.md`). |
| `LIFT_MM` (legacy config `lift_mm`, 235) | **5.0** | mm | **P**, diagnostic local profile default | `plinth-core::printer` | Canonical future value and one owner. It is not vendor evidence. Retire the pipeline's 3.0 override and remove any emit-time authority: emit consumes the plated frame/profile and cannot override lift. |
| `wall_mm` (`pipeline.prep_one` signature) | 2.5 | mm | **P** — bare default, no cited measurement | `plinth-hollow` | |

### 3.6 Embed / scar policy (SPEC-EMBED-POLICY)

These are spec literals. Every consumer and disk-first audit must use the same typed
registry values; O-5 forbids hand-copied audit constants and algorithms. The engine
gate and disk-first adapter count the same population through the sole implementation;
fresh disk provenance and non-circular fixtures provide independent evidence.

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `verify.A_ABS_MM2` | 2.0 | mm² | **P from spec** — owner-ratified, no measurement (1.60 mm equivalent scar dia) | `plinth-core::constants::scar` | `../reference/literature/LITERATURE-NOTES.md` explicitly found **no literature support** for this or the next two. |
| `verify.KAPPA` | 1.25 | — | **P from spec** | `plinth-core::constants::scar` | Headroom multiplier on the designed crossing disc. |
| `verify.L_SCAR_MAX_MM` | 2.5 | mm | **P from spec** | `plinth-core::constants::scar` | Max scar patch dimension. |
| `verify.PRINT_FLOOR_CHORD_MM` | 0.08 | mm | **M** `OwnerDecision` `print-floor-owner-decision-2026-07-24`, `Exact(2026-07-24)`, `err: null` | `plinth-core::constants::scar` | A rule-4 unanchored transit whose chord is below this is MOOT — a foliage knife-edge that cannot print as coherent material. Test-pinned. |
| `verify.MIN_ILLEGAL_LEN_MM` | 0.05 | mm | **P** — sliver cutoff | `plinth-core::constants::scar` | **Ordering is load-bearing:** the length filter is applied *after* tip anchoring, per crossing. A pre-anchoring filter promotes an unanchored transit into the tip slot where a legal scar verdict exempts it. Do not offer a kernel-side pre-anchoring filter at all. |
| `verify.TOL_MM` | 0.05 | mm | **P** — interval anchoring / detection tolerance | `plinth-core::constants::scar` | Same number as above, different concept. Keep them distinct. |
| `verify.EPS_COS` | 1e-9 | — | **P** — selected divide-by-zero guard, explicitly **not** a physics floor | `plinth-core::constants::scar` | No measurement or binding proof selects 1e-9. |
| `joints.ENCLOSURE_TOL_MM` | 0.02 | mm | **P** — legacy tolerance with no valid pixel derivation | `plinth-support::joints` | The old resolution-based derivation is invalid; printer pitch does not establish this enclosure-certificate tolerance. |
| `joints.ENCLOSURE_PROOF_TOL_MM` | 0.018 | mm | **P**, diagnostic, exactly `0.9 × ENCLOSURE_TOL_MM` | `plinth-support::joints` | Arithmetic cannot improve the Placeholder authority of the source tolerance. |
| `joints.COINCIDE_TOL_MM` / `ON_AXIS_TOL_MM` / `PLATE_BAND_MM` | 0.15 / 0.2 / 0.2 | mm | **P** | `plinth-support::joints` | `PLATE_BAND_MM` is **duplicated by hand** in `chitubox_emit.py` and mirrored in `verify.connectivity_report`. One constant, one owner, in Rust. |
| `joints.COLINEAR_ANGLE_DEG` / `COLINEAR_RADIUS_FRAC` | 10.0 / 0.20 | ° / — | **P** | `plinth-support::joints` | |
| `p8_chord_canon_micron` (736) | 1 | µm | **M** `CorpusStudy` `phase8-micron-chord-canonicalization-2026-07-24`, `Exact(2026-07-24)`, `err: null` | `plinth-core::constants::scar` | Integer-micron f64 chord canon; record `SPEC-P8-RELOCATE` §5.3 Layer 3/amendment G. |
| concession standoff ladder (SPEC-P8-REACH Feature B) | (0.3, 0.1) | mm | **P** — owner-authorized 2026-07-23, floored > 0 | `plinth-support::repair` | Locally weakens the clearance envelope to 0.1 mm expecting a human to buff the print. **Not a target design pattern** — the port should need fewer concessions, not reproduce the ladder as load-bearing. |

### 3.7 Printer profile — layer height

| Name | Value | Unit | Prov. | Rust owner | Notes |
|---|---|---|---|---|---|
| `layer_height_mm` (495) | 0.050 | mm | **M** `ProjectRecord`, not a vendor default | `plinth-core::printer::PrinterProfile` | **Architectural commitment #5: this is a PER-LAYER ARRAY in the data model from day one**, shipping a constant 0.050 mm until calibrated. It is a scalar in Python. |
| build volume, project axes X/Y/Z | 211.68 × 118.37 × 220 | mm | **M** `VendorDocument`, known source, vendor order mapped first/second/third to X/Y/Z | `plinth-core::printer::PrinterProfile` | Official values from `../datasheets/printers/saturn-4-ultra-16k.md`; mapping is a named project disposition, not an Elegoo axis label. |
| LCD pixel pitch, project axes X/Y | 14 × 19 | µm | **M** `VendorDocument`, known source, vendor order mapped first/second to X/Y | `plinth-core::printer::PrinterProfile` | `19.0` (the coarser Y axis) is the canonical source for the decimation pixel input; its legacy copy is an alias, not another measurement. |
| release-front kinematics | `leading_release_edge = NegativeY`; `release_front_sweep_direction = PositiveY` | enum pair | **M** `ProjectRecord` | `plinth-core::printer::PrinterProfile` | The front moves `-Y → +Y`. Never call this a “-Y sweep.” Axis effect is an unused input today. |
| release film | **official Elegoo PFA, 0.127 mm** | mm | **M** — the installed hardware, fitted 2026-07-25 | `plinth-core::printer::PrinterProfile` | **A model input that does not exist in `config.py` at all.** On a tilting machine the film feeds the adhesion term directly, and `../datasheets/printers/README.md` already states that the film changes the peel number. **No paper in the literature set measures PFA** — the available data is FEP (Manizani) and PDMS (Cao), so every peel figure quoted here is a *transfer*, not a match, and fluoropolymer films fare worst under tilt. Stable by inertia: swapping it is a 27-screw job, not a casual change. Record the film alongside any peel figure so a future measurement is attributable to an interface. |
| decimation error budget | p99 ≤ `pixel_um/2000` mm, p99.9 ≤ `pixel_um/1000` mm | mm | **P** — selected fractions of measured pixel pitch; arithmetic does not establish authority | `plinth-decimate` | Measured **two-sided** (decimated face centers → original surface, and original verts → decimated surface). Final full-population re-check's **1.1× slack** is also Placeholder policy. |
| `minima_connect_layers` (500) | 2.0 | layers | **P** — corpus rule of thumb; ~1.1 observed layers plus selected headroom is not a measured boundary | `plinth-support::perceive` | `z_tol = 2.0 × 0.05 = 0.1 mm` **today**. Under a per-layer array this must be computed from the actual local layer heights, not a scalar product. |
| `minima_birth_match_layers` (555) | 2.0 | layers | **P** — same corpus rule of thumb and selected headroom | `plinth-support::perceive` | |
| `minima_eps_mm` (503) | 0.02 | mm | **P** — sub-layer tie tolerance | `plinth-support::perceive` | |
| `minima_contact_radius_mm` (508) | 1.0 | mm | **P** — discretization slack, explicitly **not** a coverage radius | `plinth-support::perceive` | |
| `minima_support_area_tips` (571) | 1.0 | — | **P** — selected in a flat sensitivity region; flatness does not measure a preferred value | `plinth-support::perceive` | Threshold is `× π·tip_r_min_mm² = 0.1257 mm²`. |
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
| `p8_pocket_voxel_mm` (666) | 6.0 | mm | **M** `OwnerDecision` `p8-pocket-voxel-owner-decision`, `Exact(2026-07-25)`, `err: null` |
| `p8_pocket_min_members` (667) | 8 | — | **P** |
| `p8_pocket_corridor_pad_mm` (668) | 0.5 | mm | **P** |
| `p8_ball_slide_step_mm` / `p8_ball_slide_max_mm` (672/673) | 0.25 / 2.0 | mm | **P** |
| `p8_max_relocate_sweeps` / `_rounds` / `p8_resize_fixed_point_max_rounds` (681/693/696) | 8 / 4 / 4 | — | **P** — backstop caps, not the termination mechanism |
| `p8_reaim_volume_delta_max_ml` (701) | 0.02 | mL | **P** |
| `p8_reaim_route_growth_max_factor` / `p8_strut_growth_max_factor` (702/703) | 1.50 / 1.25 | — | **P** |
| `p8_reseat_radius_mm` / `p8_reseat_max_seeds` (716/717) | 2.0 / 24 | mm / — | **P** |
| `p8_junction_reseat_step_mm` / `_max_mm` (729/730) | 0.25 / 1.0 | mm | **P** |
| `p8_resin_delta_max` / `p8_max_unresolved` / `p8_max_unresolved_weight` (733/734/735) | 0.03 / 10 / 3.0 | — | **P** — honest-failure bounds |

### 3.10.1 Remaining output-changing source controls

The M0 ledger is source-first: it also records named source controls which are
not part of a larger physics family above. These are all `Placeholder` tuning
until a later measurement or decision changes their disposition; none is a
resource exclusion merely because it limits work. `rewire_time_cap_s` is the
sole exception: it is retired because wall-clock stopping changes output across
machines. Its deterministic replacement is a work budget.

| Name | Value | Unit | Disposition |
|---|---|---|---|
| `rewire.TWIG_MERGE_LEN_MM` / `DEGENERATE_LEN_MM` | 0.5 / 1e-4 | mm | **P**, result-changing search geometry |
| `rewire.MAX_SLIDE_EVALS` / `ROLL_WINDOW` / `ROLL_EPS` | 50 / 20 / 0.0025 | — | **P**, deterministic search controls |
| `coverage_repair_max_attempts` / `bracing._MAX_STACKED_FALLBACKS` | 8 / 6 | — | **P**, result-changing repair caps |
| `bracing._MIN_CROSS_LINK_XY_MM` / `_REACH_TIER_BASE_FACTOR` / `_FALLBACK_FRACTIONS` | 1/sqrt(2) / 1.25 / (0.5, 0.25, 0.75, 0.1, 0.9) | mm / — | **P**, brace search controls |
| `struts` model-anchor defaults | 5.0 mm / 0.5 / 40 candidates | mm / — / count | **P**, candidate-selection controls |
| `hollow` policy family | pitch 0.5 mm; bulk 10.0 mm; save 15.0 mL / 0.25; lobe 0.5 mL / 15.0 mm; drain 1.5 mm; vent 1.25 mm | mixed | **P**, output-changing hollowing policy |
| ChituBox-emission geometry | 24 sides; plate tolerance 1e-5 mm; root 10 layers / 0.5 mm; pad 2.2 mm; raft 0.2 mm; pad 20 sides | mixed | format/generation contract, separate from binary offsets |

### 3.10.2 Closed source-ledger additions — support and verification

These independently varying frozen-Python controls are registry members. They
remain `Placeholder` unless another wrapper is stated explicitly.

| Stable ID | Value / disposition | Source meaning |
|---|---|---|
| `JOINT_BALL_FACTOR` | 1.0 | joint-ball geometry factor |
| `MAX_TRUNK_R_MM` | 2.4 mm | live trunk-radius ceiling |
| `MATERIAL_COST_PER_MM` | 0.05 per mm | strut-selection material weight |
| `TIP_RADIUS_MAX_LEGACY_MM` | 1.0 mm, **P** | live output-affecting placement clamp; legacy, but not merely diagnostic |
| `BRACING_MODEL_RUNG_MAX_CANDIDATES` / `BRACING_MODEL_RUNG_Z_TOL_MM` / `BRACING_MODEL_RUNG_MIN_LENGTH_MM` | 6 / 0.75 mm / 1.0 mm | model-rung candidate policy |
| `BALL_BREACH_EPS_MM` / `MURRAY_TOLERANCE` | 0.05 mm / 0.05 | verification tolerances |
| `CONNECTIVITY_AXIS_ATTACH_TOL_MM` / `CONNECTIVITY_EMBED_PAD_MM` | 0.25 mm / 0.45 mm | connectivity attachment policy |
| `GATE_ON_REMOVAL_DEFAULT` / `GATE_ON_BALL_BREACH_DEFAULT` | false / true | independent verification defaults |
| `SURFACE_TREE_PITCH_MM` / `SURFACE_TREE_MAX_POINTS` / `SURFACE_TREE_SEED` | 0.4 mm / 150000 / 0 | result-changing sampled-surface defaults, never batch/resource exclusions |
| `ENGINE_SURFACE_TREE_PITCH_MM` / `ENGINE_SURFACE_TREE_MAX_POINTS` | 0.3 mm / 400000 | live engine call-site overrides; distinct from the callable defaults and not aliases |

`verify.connectivity_report`'s 0.2 mm plate band is an alias of
`PLATE_BAND_MM`, not a third independently owned value.

### 3.10.3 Closed source-ledger additions — hollowing and drainage

| Stable ID | Value / disposition | Source meaning |
|---|---|---|
| `HOLLOW_WALL_DEFAULT_MM` | 2.5 mm, **P** | generation/tool default |
| `HOLLOW_WALL_HANDLING_HEAVY_MM` | 3.0 mm, **P** | the other value the operator wall option was given, for handling-heavy pieces; selected per model, never inferred |
| `HOLLOW_ORIENT_ENABLE_DEFAULT` | true, **P** | independently surviving pipeline default |
| `HOLLOW_SHARD_MAX_FACES` / `HOLLOW_SHARD_MAX_DIM_MM` / `HOLLOW_SHARD_MAX_VOLUME_MM3` | 64 / 1.0 mm / 0.01 mm3 | component-triage gates |
| `HOLLOW_CONTAINMENT_SAMPLES` / `HOLLOW_CONTAINMENT_MIN_FRACTION` | 32 / 0.9 | internal-component evidence gate |
| `HOLLOW_ALLOW_OUTPUT_DEFAULT` | false, **P** | hollow output stays disabled without the dedicated verifier |
| `HOLLOW_BREATHING_VOXEL_PITCH_MM` | 0.4 mm | final cavity-breathing verification/export voxel pitch. **Registered but unread**: it sized the final breathing check in `scripts/support_placement/hollow.py`, deleted with the Python migration; `plinth-hollow::drilling` proves drainability with exact ray casts against the frozen cavity field instead of a re-voxelized breathing check at this pitch. |
| `HOLLOW_VOLUME_BALANCE_REL_TOL` | 0.02 | final volume-balance tolerance relative to the solid-model volume |
| `HOLLOW_EXIT_RAY_MAX_MM` | 45 mm | drain/vent exit search ceiling |
| `HOLLOW_DRAIN_DIRECTIONS` | `(0,0,-1)`, `(0.5,0,-1)`, `(-0.5,0,-1)`, `(0,0.5,-1)`, `(0,-0.5,-1)` | one structured downward-direction policy |
| `HOLLOW_VENT_DIRECTIONS` | `(1,0,0)`, `(-1,0,0)`, `(0,1,0)`, `(0,-1,0)` | one ordered structured horizontal-direction policy |
| `HOLLOW_VENT_TOP_BAND_MM` | 2.0 mm | cavity-top band searched for a horizontal vent site |
| `HOLLOW_VENT_SAMPLE_DIVISOR` | 40 | deterministic top-band sampling divisor |
| `HOLLOW_EXTRA_DRAIN_POLICY` | min 0.5 mL; max 3; exit <= 8 mm; extensions 4/8/9 mm; 24 cylinder sides | one structured trapped-pocket repair policy |

**The wall is an operator input, not an inference.** Legacy exposed it as a
command-line option; the first Rust port read `HOLLOW_WALL_DEFAULT_MM` inside
`create_shell` and offered no way to say anything else, which silently dropped
the knob. It is restored on `HollowRequest::wall_mm` and spelled
`plinth prepare --wall-mm MM`, defaulting to the registered 2.5 mm.
`HOLLOW_WALL_HANDLING_HEAVY_MM` registers the 3.0 mm an operator reached for on
a piece that gets handled, at exactly the provenance the default carries — the
same unmeasured legacy option, **P**, no plate and no study behind either number.
**There is deliberately no classifier**: nothing decides that a model is
handling-heavy. The caller states the wall, and `CavityEvidence::wall_thickness_mm`
records what was stated — **in-process evidence only**. `CavityEvidence` is not
`Serialize`; it hangs off `HollowedModel::cavity_authority` and dies with the
process, while the serialized `HollowReport`, `CavityRecord` and
`DrillingEvidence` carry no wall field at all. Two artifacts prepared at 2.5 mm
and 3.0 mm are therefore **indistinguishable in every file the pipeline writes**.
Auditing the choice from a persisted artifact is **FU-37**, an open follow-up,
not a property this stage has today: persisting the wall moves frozen goldens
and is deliberately not done in the same commit that restored the knob.

**The wall has a floor, and it is the lattice's, not a policy's.** Cavity
eligibility is `edt > wall` over the frozen `HOLLOW_PITCH_MM` = 0.5 mm voxel
grid, so the smallest non-zero interior distance a voxel can carry is one pitch
and *every* wall strictly below the pitch selects the identical cavity. A
requested 1e-9 mm wall would have produced the 0-mm-wall shell while the
evidence recorded 1e-9 — a record of a wall that was never cut. So
`plinth_hollow::checked_wall_mm` refuses any wall finer than the pitch by name
(`HollowError::WallBelowVoxelPitch`, surfaced by the CLI as an `InvalidWallMm`
refusal of `--wall-mm`), and the pitch itself is admissible as the finest wall
this stage can honestly claim. This is a **representability bound, not a gate
threshold**: it moves only if the frozen pitch moves.

The frozen orientation heuristic is **retired/no-port as a policy**, not silently
unaccounted: proxy reduction 0.9; identity plus ±10/20/30 degrees about X and Y;
score `0.5*support + 3*peak + 0.3*height`; section margin 0.1 mm at a 4 mm step;
support-normal cutoff -0.707. The target dense orientation search in §3.8 replaces
that coupled heuristic. Its independently useful enable default remains registered.

### 3.10.4 Closed source-ledger additions — CCO, Phase 8 and performance trials

| Stable ID | Value / disposition | Source meaning |
|---|---|---|
| `CCO_FERMAT_MAX_ITERATIONS` / `CCO_CONE_TOLERANCE_DEG` | 12 / 1e-6 degrees | bounded Fermat solve and cone comparison |
| `CCO_XIA_ALPHA` | 0.5, **Placeholder** | legacy topology-search heuristic with weak/unmeasured justification; the equations describe consequences of choosing α=1/2 but do not derive that policy |
| `CCO_X_PAIR_LOAD_DISCOUNT` | `0.5**0.25`, **Derived**, `from=[]` | proof-only fourth-root load/radius relation for each rod in a load-sharing X pair; shared by CCO estimation and bracing geometry |
| `CCO_BIFURCATION_HOSTS` / `CCO_MIN_SPLIT_LENGTH_MM` | 5 / 0.5 mm | bifurcation candidate policy |
| `CCO_SPLIT_SLIDE_POLICY` | start 0.03; end 0.97; step 0.03; cap 40 | one structured deterministic slide grid |
| `P8_RESEAT_ENABLE` / `P8_JUNCTION_RESEAT_ENABLE` / `P8_RELOCATE_ENABLE` | true / true / true | independent Phase-8 enables; reseat's live fallbacks agree, while `engine.run` has a conflicting false relocation compatibility fallback that the migration eliminates rather than ports |
| `P8_JUNCTION_RESEAT_STEP_MM` / `P8_JUNCTION_RESEAT_MAX_MM` | 0.25 / 1.0 mm | junction re-seat grid |
| `P8_CHORD_CANON_MICRON` | 1 micron, **Measured** | `CorpusStudy` canonicalization result; canonical/config value and `phase8_driver.run_phase8` fallback agree |
| `P8_MAX_RELOCATE_SWEEPS` / `P8_MAX_RELOCATE_ROUNDS` / `P8_RESIZE_FIXED_POINT_MAX_ROUNDS` | 8 / 4 / 4 | canonical/config values and live engine/driver fallbacks agree |
| `PERF_BEAM_K` / `PERF_ANCHOR_CAP` / `PERF_SEED_SPACING_FACTOR` | `TrialSetting<T>::Disabled`, **Placeholder** | B1–B3 disabled preserve the full-search baseline; no enabled value has been selected |
| `PERF_FIX_ITER_CAPS` | `TrialSetting<T>::Disabled`, **Placeholder** | B4's nominal Python tuple has two fields but consumes only index 1; do not port the inert first field |
| `REWIRE_WORK_BUDGET` | `TrialSetting<NonZeroU64>::Disabled`, **Placeholder** | B5 disabled is the exact-search baseline with no wall-clock fallback; `config.perf_rewire_work_budget` is its legacy Python alias, not a second constant. No numeric enabled budget is selected; any future `Enabled` value must be deterministic, nonzero, and evidence-backed |
| `PERF_GATE_EMIT_ON_VERIFY` | **retired/no-port** | the false compatibility switch does not survive: Rust always refuses blocking defects and canonical `EMIT_REQUIRE_PASSED_GATES_DEFAULT=true` is the sole emission gate |

For B1–B5, any future enabled candidate remains experimental `Placeholder` and
non-emittable until a corpus protocol plus owner decision selects a concrete
value. Do not invent a candidate, `PlateId`, or null/zero sentinel. `Pending<T>`
remains unchanged: it requires two concrete executable branches and a real
`PlateId`. Do not generalize `ResolutionTarget` now; a corpus-resolved Pending
design is deferred until an actual candidate exists.

`perf_exact_opts_disable = false` is excluded only because it disables
exact-result-preserving caches/accelerators; a cache-disabled equality run is the
required proof. `KDTREE_REBUILD_EVERY = 25` is likewise excluded only because
`_KDCache.nearest_members` brute-forces the unindexed tail before
returning exact nearest members. The exclusion is implementation exactness, not a
claim that 25 is physics.

### 3.10.5 Closed source-ledger additions — emission, decimation and tools

| Stable ID | Value / disposition | Source meaning |
|---|---|---|
| `PIPELINE_HOLLOW_ENABLE_DEFAULT` / `PIPELINE_P8_RELOCATE_DEFAULT` | true / false | pipeline/tool interface defaults; orientation aliases `HOLLOW_ORIENT_ENABLE_DEFAULT` |
| `EMIT_REQUIRE_PASSED_GATES_DEFAULT` | true | output authorization default |
| `EXPECTED_CHITUBOX_API_VERSION` | 1 | native writer compatibility contract, checked at emission and by the ChituBox test gate |
| `LIFT_TOL_MM` | 1e-6 mm | emitted output frame-invariant tolerance for `zmin_centered - plate_z == lift_mm` |
| `CHITUBOX_DEFAULT_PLACEMENT_XY_MM` | (0.0, 0.0) mm | emitted project placement default |
| `CHITUBOX_ROOT_LAYERS` / `CHITUBOX_ROOT_HEIGHT_MM` | 10 / 0.5 mm | root straightness contract |
| `CHITUBOX_SPHERE_MESH_TESSELLATION_POLICY` | `n_lat=8`, `n_lon=16` | default 224-triangle UV-sphere policy; changes the cached geometry of every default-emitted tip/contact and joint sphere |
| `DECIMATION_FEATURE_PROBE_RATIO` / `DECIMATION_SEED_ERROR_MM` / `DECIMATION_FEATURE_HALO_MM` / `DECIMATION_PROTECTED_CAP_FRACTION` | 0.2 / 0.050 mm / 0.4 mm / 0.12 | feature-protection controls |
| `DECIMATION_ADAPTATION_POLICY` | 3 adaptations; feature multiplier 2.0; error multiplier 0.6 | one structured adaptive policy |
| `DECIMATION_VERTEX_GROUP_FACTOR` | 10 | protected vertex-group sizing. **Registered but unread**: it sized a weighted Blender vertex group in the deleted `decimate_pipeline.py` (`skills/decimate-for-print`), removed with the Python migration; `plinth-decimate`'s meshopt QEM engine protects features with a boolean per-vertex lock (`SimplifyRequest::vertex_locks`) instead of a weighted group. |
| `DECIMATION_RATIO_SEARCH` | [0.05, 1.00], 5 bisections; only a genuinely reduced candidate may win | **Placeholder** search policy. The owner corrected the former 0.50 ceiling on 2026-08-01: it was unmeasured and must not force damage or rejection of already-print-scale geometry. |
| `DECIMATION_P99_SAMPLE_POLICY` / `DECIMATION_P999_SAMPLE_POLICY` | seed/cap 42/120000 and 1/150000 | independent error-sampling policies |
| `DECIMATION_SCALE_ACCEPTANCE_MM` | [10, 300] mm | scale sanity interval |
| `DECIMATION_PROBE_GRID_PITCH_MM` / `DECIMATION_PROBE_MAX_GRID_BYTES` / `DECIMATION_PROBE_NEAREST_UPPER_BOUND_MM` | 1.0 mm / 67108864 bytes (64 MiB) / 10000.0 mm | The D1 probe-surface configuration `plinth_pipeline::registered_decimation_budget` reads. All three are **Placeholder**: they are the values every sealed-run gate, corpus budget and frozen decimation digest in this workspace was already produced at, restated at 18 call sites and never registered until `plinth prepare` began reading them in production. Registering them changed no number. The pitch is result-changing — move it and every frozen decimation digest moves with it. The two ceilings are resource exclusions, not physics: a probe grid that would exceed 64 MiB is refused rather than coarsened, and the 10000 mm nearest bound is deliberately far above any Saturn build-volume diagonal so an oversized input model is still measured honestly before the emit bounds audit refuses it. |
| `GEOMETRY_SWEEP_MAX_PASSES` / `GEOMETRY_SWEEP_ZERO_THRESHOLD` | 8 / 1e-4 | Blender repair-loop defaults; retained only until the tool is retired |

`PIXEL_PITCH_Y_UM = 19` is the one canonical pixel source for decimation;
the decimator's local pixel input is an alias. The p99 `/2000`, p99.9 `/1000`,
and final `1.1` recheck slack remain three independently registered Placeholder
policies in §3.7: their arithmetic preserves the values but cannot promote
selected fractions/slack to AcceptSafe evidence.

### 3.10.6 Retired and runtime-control dispositions

- `P8_CONCESSION_STANDOFF_LADDER = (0.3, 0.1) mm` is explicit **retired/no-port**
  Feature-B concession behavior.
- Every live 3.0 mm lift override in `pipeline.py`, `emit_supports.py`,
  `prep_one.py`, `run_v5.py`, `run_engine_full.py`, and
  `recheck_rewire_cap.py` belongs to the one retired `LIFT_MM_PIPELINE_OVERRIDE`
  row. None aliases canonical `LIFT_MM = 5.0 mm`.
- `PLINTH_THREADS` is the new mandatory Rust workspace scheduling contract in
  `06-conventions.md` §3 and M8, outside the physics/tuning registry.
  `GEOM_KERNEL_THREADS` is a historical Python-compatibility control, not an alias or
  retained exclusion. O-7 removes it with the native `geom_kernel` boundary;
  `PLINTH_THREADS` is the sole live workspace control and the only determinism sweep.

### 3.10.7 Closed evidence-grade audit

Every `Measured` declaration below has enough information to construct its
`EvidenceSource` directly. Dates are evidence dates, not retrieval dates, and every
listed payload has `err: null`.

| Stable ID | `EvidenceSource` constructor inputs | Date / error and required note |
|---|---|---|
| `LAYER_HEIGHT_DEFAULT_MM` | `project_record(id=saturn4-ultra-layer-profile-plate6, record=docs/reference/plate-06-results.md, locator=§1 Layer height)` | `Exact(2026-07-30)`, `err: null` |
| `LOAD_SHARING` | `calibration_plate(plate=COUPON-PLATE-6-support-arm-capacity, record=docs/reference/plate-06-results.md, locator=§5 Finding 1)` | `Exact(2026-07-31)`, categorical `err: null`; note carries all five §4.2 caveats, orientation Finding 3, and the §6 B6 override citation |
| `MINIMA_DROP_INTERIOR_FACES` | `project_record(id=minima-contact-fix-2026-07-24, record=docs/reference/acceptance-run-2026-07-24-contactfix.md, locator=Provenance and What moved)` | `Exact(2026-07-24)`, `err: null`; cross-reference §3.7 |
| `MINIMA_REQUIRE_ISLAND_BIRTH` | `project_record(id=minima-contact-fix-2026-07-24, record=docs/reference/acceptance-run-2026-07-24-contactfix.md, locator=Provenance and What moved)` | `Exact(2026-07-24)`, `err: null`; cross-reference §3.7 |
| `PIXEL_PITCH_X_UM` | `vendor_document(id=elegoo-saturn4-ultra-16k-press-2025-01-22, record=docs/datasheets/printers/saturn-4-ultra-16k.md, locator=Physical parameters plus project axis mapping)` | `Exact(2025-01-22)`, `err: null`; vendor first pitch maps to project X |
| `PIXEL_PITCH_Y_UM` | same vendor document and locator | `Exact(2025-01-22)`, `err: null`; vendor second pitch maps to project Y |
| `SATURN_BUILD_VOLUME_MM` | same vendor document and locator | `Exact(2025-01-22)`, `err: null`; vendor first/second/third dimensions map to project X/Y/Z |
| `SATURN_LEADING_RELEASE_EDGE` | `project_record(id=saturn4-ultra-release-front-plate6, record=docs/reference/plate-06-results.md, locator=§1 Printer)` | `Exact(2026-07-31)`, `err: null`; project `NegativeY` |
| `SATURN_RELEASE_FRONT_SWEEP_DIRECTION` | same project record and locator | `Exact(2026-07-31)`, `err: null`; project `PositiveY`, front moves `-Y → +Y` |
| `SATURN_RELEASE_FILM_THICKNESS_MM` | `installed_hardware(id=saturn4-ultra-pfa-install-2026-07-25, record=docs/rust-migration/03-physics-and-constants.md, locator=§3.7 release film)` | `Exact(2026-07-25)`, `err: null`; nominal thickness identifies installed PFA and is not peel evidence |
| `TIP_DIAMETER_MIN_ALLOWED_MM` | `owner_decision(id=tip-diameter-floor-owner-decision, record=docs/rust-migration/03-physics-and-constants.md, locator=§3.2)` | `OnOrBefore(2026-07-31)`, `err: null` |
| `X_STAR_CONE` | `calibration_plate(plate=COUPON-PLATE-5-compliant-vs-stiff, record=docs/reference/plate-05-results.md, locator=§1 and §5)` | `OnOrBefore(2026-07-30)`, interval `err: null` |
| `X_STAR_CYLINDER` | `calibration_plate(plate=COUPON-PLATE-6-support-arm-capacity, record=docs/reference/plate-06-results.md, locator=§5 Finding 2)` | `Exact(2026-07-31)`, interval `err: null` |
| `P8_CHORD_CANON_MICRON` | `corpus_study(id=phase8-micron-chord-canonicalization-2026-07-24, record=docs/specs/SPEC-P8-RELOCATE.md, locator=§5.3 Layer 3/amendment G)` | `Exact(2026-07-24)`, f64/integer-micron `err: null` |
| `P8_POCKET_VOXEL_MM` | `owner_decision(id=p8-pocket-voxel-owner-decision, record=docs/specs/SPEC-P8-RELOCATE.md, locator=§4.1)` | `Exact(2026-07-25)`, `err: null` |
| `PRINT_FLOOR_CHORD_MM` | `owner_decision(id=print-floor-owner-decision-2026-07-24, record=docs/specs/SPEC-EMBED-POLICY.md, locator=PRINT-FLOOR amendment and Normative constants)` | `Exact(2026-07-24)`, `err: null` |
| `SUPPORT_KNN_WORK_PER_FACE_OBSERVED` | `corpus_study(id=support-resource-caps-weeping-treant-2026-08-05, record=docs/rust-migration/support-resource-caps-2026-08-05.md, locator=§2 Measured requirement)` | `Exact(2026-08-05)`, integer-count `err: null` |
| `SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED` | same corpus study and locator | `Exact(2026-08-05)`, `err: null` |

The vendor document's retrieval date, 2026-07-31, remains metadata; its evidence
date is the checked-in document's 2025-01-22 publication date.

Exactly six declarations remain `Derived`:

| Stable ID | `from` | Exact relation / proof |
|---|---|---|
| `CCO_X_PAIR_LOAD_DISCOUNT` | `[]` | `(F/2)^0.25 / F^0.25 = 0.5^0.25`; proof-only relation in `docs/reference/physics_transient_peel.md`, implemented in both `cco.py` and `bracing.py` |
| `N_EFF_CAP` | `[LOAD_SHARING]` | reads the measured `Additive` payload's largest tested group size, `6.0`; not a separate measurement |
| `SUPPORT_KNN_MAX_WORK_PER_FACE` | `[SUPPORT_KNN_WORK_PER_FACE_OBSERVED]` | `next_power_of_two(2 × 696) = next_power_of_two(1392) = 2048` |
| `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` | `[SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED]` | `next_power_of_two(2 × (81 + 36 × 2.8303132)) = next_power_of_two(366) = 512`, through the exact parity projection `81·faces + 36·references + 4` |
| `TIP_RADIUS_MIN_ALLOWED_MM` | `[TIP_DIAMETER_MIN_ALLOWED_MM]` | `TIP_DIAMETER_MIN_ALLOWED_MM / 2 = 0.075 mm` |
| `X_ALLOWED` | `[X_STAR_CONE]` | binding commitment/precedence: `floor(lower(X_STAR_CONE)/2) = floor(25.00/2) = 12`; `X_STAR_CYLINDER` only supplies the `19.91/12 = 1.66×` margin check |

The following source values remain unchanged but are `Placeholder`, hence
non-AcceptSafe: `DECIMATION_P999_FACTOR`, `DECIMATION_P99_FACTOR`,
`DECIMATION_RECHECK_SLACK`, `ENCLOSURE_PROOF_TOL_MM`, `EPS_COS`,
`ISLAND_ATTACH_REACH_MM`, `MAX_SEGMENT_ANGLE_DEG`,
`MAX_UNBRACED_SHAFT_R_MM`, `MINIMA_BIRTH_MATCH_LAYERS`,
`MINIMA_CONNECT_LAYERS`, `OVERHANG_ANGLE_DEG`, `SPACING_FLOOR_MM`,
`STANDOFF_MM`, `STRUT_MAX_LENGTH_MM`, `TARGET_SPACING_MM`,
`TRANSIENT_SHOOT_COUNT_EXPONENT`, and `VOXEL_PITCH_MM`. Their arithmetic,
version convention, or corpus rationale preserves an explanation, not
AcceptSafe authority; in particular arithmetic from a Placeholder cannot improve
its provenance. `MINIMA_SUPPORT_AREA_TIPS = 1.0` is likewise Placeholder tuning
selected in a flat sensitivity region, not a measurement.

`SURFACE_PITCH_BOUND_PERCENTILE` is retired/no-port: `SPEC-V5-GEOM` §2.1 and
§2.6 make sampled-cloud pitch uncertainty identically zero on exact triangle paths.

**Addendum 2026-08-07 (FU-25 Phase 2 — the decimation downstream-utility witness).** The
paragraphs and tables above are left standing as written; two counts in them have moved and
one new `Placeholder` joins §3.10.5's decimation block. The sentence "Exactly six
declarations remain `Derived`" was already superseded before this branch — the caps/FU-8d
work added `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` and
`SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT`, taking it to eight — and this addendum takes it to
**eleven**. The registry total is **268** and its class vector is `[20, 11, 237, 0]`
(`[measured, derived, placeholder, pending]`). The three new `Derived` rows:

| Stable ID | `from` | Exact relation / proof |
|---|---|---|
| `DECIMATION_MAX_ABSOLUTE_DEVIATION_MM` | `[PRINT_FLOOR_CHORD_MM]` | exact identity, no arithmetic: the bound **is** the registered print-floor chord, 0.08 mm, re-expressed as a worst-case decimation deviation limit, on the rule that a deviation below the smallest printable chord cannot produce a printed difference |
| `DECIMATION_MAX_P999_DEVIATION_MM` | `[PIXEL_PITCH_Y_UM]` | `PIXEL_PITCH_Y_UM / 1000 = 19 / 1000 = 0.019 mm`; the `/1000` is the µm→mm conversion, so the bound is exactly one addressable step of the coarser printer axis |
| `DECIMATION_MAX_P99_DEVIATION_MM` | `[DECIMATION_MAX_P999_DEVIATION_MM]` | `0.019 / 2 = 0.0095 mm`, half an addressable step — the quantization half-step the printer already introduces |

The p99/p99.9 pair is **numerically identical** to what the budget search already enforces as
`PIXEL_PITCH_Y_UM × DECIMATION_P99_FACTOR` and `× DECIMATION_P999_FACTOR`, and is separately
declared for the reason the last paragraph above states: those two factors are `Placeholder`,
and arithmetic from a `Placeholder` cannot improve its provenance, so an acceptance path must
assert the `Derived` bound. The two are pinned to agree bit-for-bit by
`decimation_witness_bounds_agree_with_the_search_policy_bounds`
(`crates/plinth-decimate/src/lib.rs`), so a move in either side is loud rather than silent.
`DECIMATION_P99_FACTOR`, `DECIMATION_P999_FACTOR` and `DECIMATION_RECHECK_SLACK` are
unchanged and remain `Placeholder`.

The one new `Placeholder`, belonging with §3.10.5's decimation rows, is
`DECIMATION_UTILITY_MIN_INPUT_FACES = 32,768` (`count`): the input-face floor below which the
downstream-utility witness refuses without examining geometry. It is deliberately not
derivable — gating on "finer than the printer can resolve" (`area / (pixel × layer)`) puts a
mini-sized model's resolvable face count near 10⁶ and would forbid decimating the
429,120-face Weeping Treant too — so it is registered as the corpus-shaped policy gate it is.
It decides only *whether to attempt*; every safety bound in the witness is `Derived`.
Single-specimen basis, R4-shaped standing trigger: n = 1 large specimen, and a second corpus
model above ~4,760 faces triggers re-derivation rather than silent inheritance. Full
derivation, the measured consequences on the one large specimen, and the OD-8 evidence:
[`decimation-design-2026-08-06.md`](decimation-design-2026-08-06.md) §9.

### 3.10.8 Support execution-authority caps — the model-scaled pair

These are the first support-pipeline **execution-authority** caps to enter the
registry, under the new `runtime_resource_cap` classification. They are Rust-era
and have **no legacy Python source**, so §3.11's "not constants" ruling — which
is about numpy/CPython memory-accounting artifacts — does not cover them: these
are ratified numeric authority with a stated rule and a measurement of record.
The full measurement is `support-resource-caps-2026-08-05.md`.

Both caps take one shape, materialized per model by
`SupportPreparationLimits::input` against that model's own support-mesh face
count:

```
cap = max(floor, per_face × support_mesh_face_count)
```

and both coefficients come from one rule over the 2026-08-05 Weeping Treant
measurement: `per_face = next_power_of_two(2 × observed_per_face)`.

| Stable ID | Value | Unit | Wrapper | Source meaning |
|---|---|---|---|---|
| `SUPPORT_KNN_WORK_PER_FACE_OBSERVED` | 696 | count/face | **Measured** | `298,667,520 / 429,120`, exact; the treant's minima-kNN batch cost per face |
| `SUPPORT_KNN_MAX_WORK_PER_FACE` | 2048 | count/face | **Derived** | `next_power_of_two(2 × 696)` |
| `SUPPORT_KNN_MAX_WORK_FLOOR` | 100,000,000 | count | **Placeholder** | inherited unmeasured Codex-era floor (73e7c13 / a8acfb9), unchanged |
| `SUPPORT_PARITY_GRID_REFS_PER_FACE_OBSERVED` | 1,214,544 / 429,120 | count/face | **Measured** | the treant's parity `(cell, face)` references per face; inverts the exact projection `81·faces + 36·references + 4 = 78,482,308` |
| `SUPPORT_PARITY_GRID_MAX_BYTES_PER_FACE` | 512 | B/face | **Derived** | `next_power_of_two(2 × (81 + 36 × 2.8303132))` |
| `SUPPORT_PARITY_GRID_MAX_BYTES_FLOOR` | 67,108,864 | B | **Placeholder** | inherited unmeasured Codex-era floor (73e7c13), unchanged |

**Why scaled and not simply larger.** A cap's job is to refuse work that is
superlinear in model size, not to refuse a large model, and a flat constant
cannot tell those apart: at 429,120 faces the old constants fired on honest
linear work, while at 24 faces `make_raking_blade` was already burning 31% of an
unrelated 2 × 10⁹ gate cap on a genuine quadratic. `k·N` is still a bound — it
refuses anything worse than linear in `N`. An all-pairs kNN over the treant is
`429,120² = 1.84 × 10¹¹` and is refused 209× over; 512 B/face admits at most
`(512 − 81)/36 = 11.97` grid references per face against the treant's measured
2.83.

**These caps are result-inert.** They admit or refuse; they never alter an
admitted plan, and they enter no semantic or result digest. The floor governs
for every model the corpus can currently run — Sphere, the largest at 4,760
faces, needs 2,437,120 grid bytes and 9,748,480 kNN work — so introducing the
rule moved no corpus digest, which the 2026-08-05 before/after census confirms
line by line.

**Not raised, deliberately.** `max_gate_operations`, `max_load_operations`,
`max_load_scratch_bytes` and `max_gate_peak_bytes` stay at their inherited
values. Measurement showed they are refusing algorithm defects — a selection
sort charging ~1.7 × 10¹⁰ operations where a set charges 142,646, and an
8.0 M-row load-eligibility explosion on a 429,120-face mesh — not a large
model's honest requirement. Their constants must be set on **post-fix**
dimensions when the algorithm work (FU-8b) lands. (Owner ruling R5, 2026-08-05:
they are no longer *unregistered* — all four are registered as `Placeholder` in
§3.10.9 and carry that measurement in their registry notes. Registration
discloses the inherited number; it does not ratify it and does not authorize
raising it.) **2026-08-06:** the selection sort named above is fixed (FU-8b (A),
`0bdec0c`), and `max_gate_operations` now refuses a *stated* treant demand of
12,714,828,544 rather than a counter tripping at `limit + 1`. All four still
stay at their inherited values: FU-8b (b) and (c) are unfixed, so the post-fix
dimensions the constants must be set on do not exist yet.

**Addendum 2026-08-07 (FU-8f phase G1) — a new charged term, not a new cap.**
Everything above stands and all four caps remain at their inherited values.
`finalize_load_rows` now scans a **contact-major CSR transpose** of the load
eligibility instead of scanning `contacts × rows` and skipping the ineligible
cells — 8,210,643 eligible cells of 160,834,206 visited on the Weeping Treant
v1.2 at `7aa6b73`, so 94.9 % of the old visits were the skip. That buffer is
charged as a **new** `LoadResourcePlan::scratch_bytes` term,
`cells × 4 B + (contacts + 1) × 4 B` = **32,848,040 B**, which is **6.1 % of the
unchanged 512 MiB `max_load_scratch_bytes`**. No cap was raised, lowered or
touched, and **no charged operation count moved**: the load operation model
still prices the scan as `rows × contacts × (owner_log + 4)` and now
deliberately over-states it, on the FU-8e Q5 precedent that a plan admitting the
general path *a fortiori* admits its specialisation. Repricing that term is
sequenced as its own later phase (G5), after a treant run completes the gate.
See `support-resource-caps-2026-08-05.md` for the counter table, the
preflight-versus-exact-plan asymmetry, and the bit-identity evidence.

**Addendum 2026-08-07 (FU-8f phase G5) — the repricing above has landed, and its
precondition is corrected rather than quietly dropped.** All four caps remain at
their inherited values and the 32,848,040 B byte term is unchanged to the byte.
The sentence "no charged operation count moved" is now history: G5 retired
`rows × contacts × (owner_log + 4)` and replaced it with
`Σ |eligible_owners| × (log + 2) × 2` from the counts plus
`cells × (LOAD_WITNESS_SLOTS + 5)` from the exact cell count, so on the treant's
gate batch that term goes 4,342,523,562 → 207,690,028 + 65,685,144 and the
finalizer's whole exact plan goes 4,714,113,843 → 644,970,917. **The
precondition stated above — "after a treant run completes the gate" — was NOT
met, and G5 ran anyway.** That is deliberate and is stated so it is not read as
an oversight: the precondition belongs to **ruling R-f**, which governs
*re-deriving the batch-product coefficients from a completed run*, and G5
re-derives neither coefficient. What G5 does is correct a charge that was
over-stating executed work by a factor of ~21 on the term concerned; that is a
falling demand, it can create no refusal, and it needs no completed run to be
true. R-f is still unsatisfied and both coefficients are still not re-derived.
The preflight/exact-plan asymmetry the paragraph above describes for **bytes**
now holds for **operations** too, for the same reason and with the same
consequence: a later refusal, never unafforded execution.

### 3.10.9 Support execution-authority caps — the unmeasured inventory

Owner ruling **R5, 2026-08-05**. §3.10.8 registered six constants and left the
rest of the support pipeline's execution authority **unregistered**: real
numbers, live in production, refusing real work, with no registry entry, no
stated provenance, and no place a reader could look them up. Every cap below is
now registered as classification `runtime_resource_cap`, wrapper
**`Placeholder`**, with `source_locators` naming its own construction site.

Read the wrapper literally. **Registration is disclosure, not ratification.**
Every value here is an inherited Codex-era number with **no measurement of
record**; the `Placeholder` wrapper is what structurally bars it from any accept
path. Registering them changes nothing about what they are — it changes only
whether they can be found. Nothing here is a licence to *tune* them: a value
that acquires a measurement moves to `Measured`/`Derived` under §3.10.8's rule,
and until then the number stands as inherited.

The scope of this ruling is exactly the four authority types named in it:
`GenericPerceptionLimits`, `SpecializedLimits`, `KnnBatchLimits`, and the
remaining direct scalar fields of `SupportPreparationLimits`. It is **42
constants**. The four further nested authorities reachable from
`SupportPreparationLimits::production` — `CandidatePayloadConfig`,
`ContactConstructionLimits`, `PenetrationBatchLimits` and `RaftLimits`, ~19 more
values — are **not** covered: `CandidatePayloadConfig` in particular mixes
result-changing refinement policy with execution caps, so they need their own
classification pass rather than a blanket `runtime_resource_cap` sweep.

| Stable ID | Value | Unit | Construction site |
|---|---|---|---|
| `SUPPORT_KNN_MAX_LOGICAL_PEAK_BYTES` | 2,147,483,648 | B | `KnnBatchLimits`.max_logical_peak_bytes |
| `SUPPORT_KNN_MAX_NEIGHBORS` | 1 | count | `KnnBatchLimits`.max_neighbors |
| `SUPPORT_KNN_MAX_QUERY_POINTS` | 20,000,000 | count | `KnnBatchLimits`.max_query_points |
| `SUPPORT_KNN_MAX_SOURCE_POINTS` | 20,000,000 | count | `KnnBatchLimits`.max_source_points |
| `SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_BYTES` | 2,147,483,648 | B | `GenericPerceptionLimits`.max_island_neighbor_bytes |
| `SUPPORT_PERCEPTION_MAX_ISLAND_NEIGHBOR_PAIRS` | 100,000,000 | count | `GenericPerceptionLimits`.max_island_neighbor_pairs |
| `SUPPORT_PERCEPTION_MAX_LOOP_VERTICES` | 100,000,000 | count | `GenericPerceptionLimits`.max_loop_vertices |
| `SUPPORT_PERCEPTION_MAX_MINIMA_VERTICES` | 20,000,000 | count | `GenericPerceptionLimits`.max_minima_vertices |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_BYTES` | 2,147,483,648 | B | `GenericPerceptionLimits`.max_overlap_bytes |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_EDGE_TESTS` | 100,000,000 | count | `GenericPerceptionLimits`.max_overlap_edge_tests |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_PAIRS` | 100,000,000 | count | `GenericPerceptionLimits`.max_overlap_pairs |
| `SUPPORT_PERCEPTION_MAX_OVERLAP_VERTEX_TESTS` | 100,000,000 | count | `GenericPerceptionLimits`.max_overlap_vertex_tests |
| `SUPPORT_PERCEPTION_MAX_PLANES` | 20,000 | count | `GenericPerceptionLimits`.max_planes |
| `SUPPORT_PERCEPTION_MAX_SLICE_INTERSECTIONS` | 100,000,000 | count | `GenericPerceptionLimits`.max_slice_intersections |
| `SUPPORT_PERCEPTION_MAX_SLICE_OUTPUT_BYTES` | 2,147,483,648 | B | `GenericPerceptionLimits`.max_slice_output_bytes |
| `SUPPORT_PERCEPTION_MAX_TRIANGLE_PLANE_TESTS` | 100,000,000 | count | `GenericPerceptionLimits`.max_triangle_plane_tests |
| `SUPPORT_PREPARATION_COVERAGE_SAMPLES_MAX_VIRTUAL_CELLS` | 2,000,000 | count | `SupportPreparationLimits`.coverage_samples_max_virtual_cells |
| `SUPPORT_PREPARATION_COVERAGE_SCORE_MATRIX_MAX_ENTRIES` | 65,536 | count | `SupportPreparationLimits`.coverage_score_matrix_max_entries |
| `SUPPORT_PREPARATION_MAX_PRIMITIVES` | 200,000 | count | `SupportPreparationLimits`.max_primitives |
| `SUPPORT_PREPARATION_MAX_PRIMITIVE_BYTES` | 134,217,728 | B | `SupportPreparationLimits`.max_primitive_bytes |
| `SUPPORT_SPECIALIZED_MAX_COVERAGE_OPERATIONS` | 2,000,000,000 | count | `SpecializedLimits`.max_coverage_operations |
| `SUPPORT_SPECIALIZED_MAX_COVERAGE_SCRATCH_BYTES` | 536,870,912 | B | `SpecializedLimits`.max_coverage_scratch_bytes |
| `SUPPORT_SPECIALIZED_MAX_GATE_CONTACT_MATRIX_BYTES` | 536,870,912 | B | `SpecializedLimits`.max_gate_contact_matrix_bytes |
| `SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS` | 2,000,000,000 | count | `SpecializedLimits`.max_gate_operations |
| `SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES` | 1,073,741,824 | B | `SpecializedLimits`.max_gate_peak_bytes |
| `SUPPORT_SPECIALIZED_MAX_GATE_VARIANTS` | 20,000,001 | count | `SpecializedLimits`.max_gate_variants |
| `SUPPORT_SPECIALIZED_MAX_GATE_VECTOR_BYTES` | 536,870,912 | B | `SpecializedLimits`.max_gate_vector_bytes |
| `SUPPORT_SPECIALIZED_MAX_GATE_WINDOW_VARIANTS` | 1,024 | count | `SpecializedLimits`.max_gate_window_variants |
| `SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS` | 2,000,000,000 | count | `SpecializedLimits`.max_load_operations |
| `SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES` | 536,870,912 | B | `SpecializedLimits`.max_load_scratch_bytes |
| `SUPPORT_SPECIALIZED_MAX_MERGE_COMMIT_BYTES` | 536,870,912 | B | `SpecializedLimits`.max_merge_commit_bytes |
| `SUPPORT_SPECIALIZED_MAX_MERGE_DESCRIPTOR_OPERATIONS` | 2,000,000,000 | count | `SpecializedLimits`.max_merge_descriptor_operations |
| `SUPPORT_SPECIALIZED_MAX_MERGE_ITERATIONS` | 100,000 | count | `SpecializedLimits`.max_merge_iterations |
| `SUPPORT_SPECIALIZED_MAX_MERGE_PAIRS` | 20,000,000 | count | `SpecializedLimits`.max_merge_pairs |
| `SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSALS` | 20,000,000 | count | `SpecializedLimits`.max_merge_proposals |
| `SUPPORT_SPECIALIZED_MAX_MERGE_PROPOSAL_BYTES` | 536,870,912 | B | `SpecializedLimits`.max_merge_proposal_bytes |
| `SUPPORT_SPECIALIZED_MAX_MERGE_TOTAL_PROPOSALS` | 100,000,000 | count | `SpecializedLimits`.max_merge_total_proposals |
| `SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_BYTES` | 2,147,483,648 | B | `SpecializedLimits`.max_satisfaction_neighbor_bytes |
| `SUPPORT_SPECIALIZED_MAX_SATISFACTION_NEIGHBOR_PAIRS` | 100,000,000 | count | `SpecializedLimits`.max_satisfaction_neighbor_pairs |
| `SUPPORT_SPECIALIZED_MAX_SATISFACTION_TESTS` | 100,000,000 | count | `SpecializedLimits`.max_satisfaction_tests |
| `SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_BYTES` | 536,870,912 | B | `SpecializedLimits`.max_synthetic_neighbor_bytes |
| `SUPPORT_SPECIALIZED_MAX_SYNTHETIC_NEIGHBOR_PAIRS` | 20,000,000 | count | `SpecializedLimits`.max_synthetic_neighbor_pairs |

All 42 are `Placeholder`, `scalar`, and result-inert on the same terms as
§3.10.8's floors: they admit or refuse, they never alter an admitted plan, and
they enter no semantic or result digest. Registering them moved no value and no
digest.

> **Dated addendum 2026-08-06 (FU-8d phase F1, ruling R-b) — the table above is
> left standing, and `SUPPORT_KNN_MAX_NEIGHBORS` is still 1 where the table says
> it is.** That row names one construction site, `KnnBatchLimits.max_neighbors`,
> and there are now two values behind it:
>
> | limits value | `max_neighbors` | who reads it |
> |---|---|---|
> | `SupportPreparationLimits::production`'s `knn`, carried verbatim into `GenericPerceptionLimits::production` | **1**, unchanged — this is the registered constant | the generic perception batches, which still ask for one neighbour |
> | `SpecializedLimits::production`'s `coverage_route_knn` | **3** | the merge gate's baseline coverage sweep, which asks for `min(3, source_count)` neighbours per sample |
>
> The gate's baseline sweep retains up to three nearest sources per coverage
> sample so a merge-proposal delta can answer "nearest among the survivors of a
> two-contact removal" by inspection instead of re-sweeping the perturbed route
> key. **The 3 is a `KnnBatchLimits` shape field, not a cap, and nothing in
> §3.10.8 or §3.10.9 was raised to make room for it.** A `KnnBatchPlan`'s charged
> work is `source_count × query_count` and is k-independent, so no operation
> charge anywhere moves with it (ruling R-a, recorded on
> `NEAREST_WITNESS_SLOTS` in `specialized.rs`). What it does cost is bytes, and
> both terms are charged rather than absorbed: `+2 × query_count × 16 B` per
> route inside `KnnBatchPlan::logical_peak_bytes` against that batch's 2 GB
> logical-peak limit, and `samples × 3 × 16 B` per batch as the new
> `GatePeakUsage::nearest_witness_bytes` term against the unchanged
> 1,073,741,824 B `SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES` — on the Weeping
> Treant's 405,146-sample batch that second term is ~19 MB beside a gate peak
> last measured at 216,203,088 B.
>
> Across the change all seven frozen OD-8 digests stayed byte-identical and the
> treant's stage error stayed verbatim at
> `Preparation: Specialized(GateOperationLimit { required: 201480535329, limit: 35445500736 })`.
> The registry note on `SUPPORT_KNN_MAX_NEIGHBORS` carries the same amendment,
> because its "the support pipeline only ever asks for the single nearest
> neighbor" clause was true when written and is not true now.
>
> **Addendum 2026-08-06 (FU-8d phase F2) — everything above stands; the last
> sentence's transcript is superseded.** F2 made the per-proposal delta *read*
> those retained witnesses, so the neighbour ceiling above is now load-bearing
> rather than merely allocated. **§3.10.8 and §3.10.9 remain untouched: F2 raised,
> lowered and moved no cap either.** The seven frozen digests are again
> byte-identical, but the treant's stage error is **not** — the merge gate's
> stated demand fell 27.6×, from 201,480,535,329 to **7,299,316,559** against the
> unchanged 35,445,500,736, so `gate_resource_plan` now passes and the refusal
> moved one site later, into the `exact_operations` fold's per-variant owner-face
> descriptor scan:
> `Preparation: Specialized(GateOperationLimit { required: 35445618202, limit: 35445500736 })`.
> That figure is the counter crossing at variant 275 of 39,224, not a total.
> See `support-resource-caps-2026-08-05.md` §9.

**Four of them are known to be refusing defects, and that is recorded on the
constant.** `SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS`,
`SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS`,
`SUPPORT_SPECIALIZED_MAX_LOAD_SCRATCH_BYTES` and
`SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES` carry the 2026-08-05 measurement in
their registry notes together with the instruction that they must **not** be
raised — see the *Not raised, deliberately* paragraph in §3.10.8 and FU-8b.
Two of those four, the gate and load **operation** caps, became the floors of a
measured rule on 2026-08-06 without their numbers changing; see §3.10.10.

> **Dated addendum 2026-08-09 (FU-8g phase H3) — the "not covered" sentence above is
> corrected, not rewritten, and three of the four nested authorities it named are
> now registered.** The paragraph naming the scope of this ruling said the four
> further nested authorities reachable from `SupportPreparationLimits::production`
> — `CandidatePayloadConfig`, `ContactConstructionLimits`, `PenetrationBatchLimits`
> and `RaftLimits`, ~19 more values — were **not** covered. FU-8g's own design
> (`fu8g-design-2026-08-08.md` F1/F2) found the same gap independently: the four
> `ContactConstructionLimits` numbers are bare literals at **two** independent
> sites (`ContactConstructionLimits::production`, `initial.rs`, and
> `SupportPreparationLimits::production`'s `construction_limits`, `runner.rs` —
> both hard-code the same four numbers, and this registration does not merge
> them), and brace's six `PenetrationBatchLimits` fields (`brace_penetration_limits`)
> plus `RaftLimits`'s four fields are equally unregistered. **All 14 are now
> registered** as classification `runtime_resource_cap`, wrapper `Placeholder`,
> exactly on §3.10.9's terms: **registration is disclosure, not ratification**,
> and no cap is raised, lowered or touched by it.
>
> | Stable ID | Value | Unit | Construction site |
> |---|---|---|---|
> | `SUPPORT_CONSTRUCTION_MAX_CONTACTS` | 1,000,000 | count | `ContactConstructionLimits`.max_contacts |
> | `SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS` | 10,000,000 | count | `ContactConstructionLimits`.max_load_rows |
> | `SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS` | 4,000,000 | count | `ContactConstructionLimits`.max_work_items |
> | `SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES` | 536,870,912 | B | `ContactConstructionLimits`.max_scratch_bytes |
> | `SUPPORT_BRACE_PENETRATION_MAX_ROWS` | 100,000 | count | `PenetrationBatchLimits`.max_rows (brace_penetration_limits) |
> | `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_WORK` | 1,000,000,000 | count | `PenetrationBatchLimits`.max_total_work |
> | `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_HITS` | 1,000,000,000 | count | `PenetrationBatchLimits`.max_total_hits |
> | `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_SAMPLES` | 10,000,000,000 | count | `PenetrationBatchLimits`.max_total_samples |
> | `SUPPORT_BRACE_PENETRATION_MAX_TOTAL_INTERVALS` | 1,000,000,000 | count | `PenetrationBatchLimits`.max_total_intervals |
> | `SUPPORT_BRACE_PENETRATION_MAX_SCRATCH_BYTES` | 100,000,000,000 | B | `PenetrationBatchLimits`.max_scratch_bytes |
> | `SUPPORT_RAFT_MAX_CANDIDATES` | 100,000 | count | `RaftLimits`.max_candidates |
> | `SUPPORT_RAFT_MAX_ROOTS` | 100,000 | count | `RaftLimits`.max_roots |
> | `SUPPORT_RAFT_MAX_PRIMITIVES` | 200,000 | count | `RaftLimits`.max_primitives |
> | `SUPPORT_RAFT_MAX_BYTES` | 134,217,728 | B | `RaftLimits`.max_bytes |
>
> **What this registration adds beyond disclosure, per constant, is honest about
> what is and is not measured** (single-specimen markers per R4/FU-23 where a
> measurement exists, and a plain statement of absence where it does not):
> `SUPPORT_CONSTRUCTION_MAX_WORK_ITEMS` and `SUPPORT_CONSTRUCTION_MAX_CONTACTS`
> carry the Weeping Treant v1.2's FU-8g H1/H2 measured demand (108,378 work
> items, 2.71 % of cap; 553 contacts, 0.055 %, both single specimen n = 1);
> `SUPPORT_CONSTRUCTION_MAX_SCRATCH_BYTES` carries H0's pre-H1 estimate
> (149,188,000 B, ≈142 MiB, 27.8 %) with H2's own disclaimer that the post-H1
> figure is H6's to measure, not claimed here; `SUPPORT_CONSTRUCTION_MAX_LOAD_ROWS`
> and all ten brace/raft constants carry **no** measured demand, honestly, because
> the brace and raft stages are unexercised on every corpus model and on the
> treant at this tip (§17.7) — the design's F7 finding for the six brace fields is
> a **derived** worst-case ceiling (production admits at most 85 clearance rods on
> the treant mesh once `max_scratch_bytes` is accounted for), not a measured
> demand, and is labelled as such in the registry note.
>
> **`CandidatePayloadConfig` remains genuinely out of scope**, exactly as R5 left
> it: it mixes result-changing refinement policy with execution caps and needs
> its own classification pass, which is not this registration's job. Full
> ledger rows, provenance and the FU-24 gate: `m0-constants-source-ledger.json`;
> caps-doc record: `support-resource-caps-2026-08-05.md` §18; dated
> `CURRENT-STATUS.md` section: *FU-8g phase H3 landed 2026-08-09*.
>
> **[ADDENDUM 2026-08-09 (FU-32 phase J6) — "the brace and raft stages are
> unexercised … on the treant at this tip" is SUPERSEDED; "no measured demand" is
> NOT.** The paragraph above is left standing as the record of its date. The
> Weeping Treant **sealed** twice under the standing memory protocol at FU-32 J5's
> tip, so brace, joints/raft and lowering all executed and none of the ten caps
> refused. **No demand figure of record exists for any of the ten even so**,
> because none of them is metered anywhere in the census — a stage that does not
> refuse tells you only that its demand was below the cap. The F7 ceiling remains
> **derived**, never measured; every registry note and every provenance marker
> above is unchanged; **no cap was raised, lowered or touched.** See
> `support-resource-caps-2026-08-05.md` §22.3 and *FU-32 phase J6 measured
> 2026-08-09* in `CURRENT-STATUS.md`.]**

### 3.10.10 Support execution-authority caps — the batch-product pair {#batch-product-caps}

FU-8b, 2026-08-06. §3.10.8's two caps scale on the support mesh's **face
count**, because both batches are built face by face. The specialized gate and
load stages do not work that way, and the corpus proves it: 24-face
`make_raking_blade` charges 612,887,212 gate operations while 429,120-face
Weeping Treant charges 12,714,828,544 — a 20× spread over an 18,000× face
spread. A per-face rule for those two would refuse the small model and admit
anything at all on the large one.

They scale on the **batch product** each stage is handed instead, under the same
`max(floor, coefficient × scale)` shape and the same derivation rule:

| cap | rule | floor (Placeholder, R5) | coefficient (Derived, n = 1) |
| --- | --- | --- | --- |
| specialized gate operations | `max(floor, K_G × (coverage_samples + 1) × (contacts + 1))` | `SUPPORT_SPECIALIZED_MAX_GATE_OPERATIONS` = 2,000,000,000 | `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` = 64 |
| specialized load operations | `max(floor, K_L × (rows + 1) × (contacts + 1))` | `SUPPORT_SPECIALIZED_MAX_LOAD_OPERATIONS` = 2,000,000,000 | `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` = 64 |

| constant | wrapper | value | unit | derivation |
| --- | --- | --- | --- | --- |
| `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED` | Measured | 23 | count/unit | 12,714,828,544 / 553,835,949 batch units = 22.958, ceiling. Weeping Treant v1.2, 2026-08-06 |
| `SUPPORT_GATE_MAX_WORK_PER_BATCH_UNIT` | Derived | 64 | count/unit | `next_power_of_two(2 × 23)` |
| `SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED` | Measured | 30 | count/unit | 5,402,535,619 / 183,682,423 batch units = 29.41, ceiling. Same run |
| `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` | Derived | 64 | count/unit | `next_power_of_two(2 × 30)` |

> **CORRECTED 2026-08-06 (record fix F1) — dated addendum; the two load rows above are left
> standing as written and are superseded by these.** Their divisor was taken over a
> 134,368-row batch that this build plans nowhere. The demand 5,402,535,619 is
> `finalize_specialized_load_partition`'s planned `resource.operations`, and that plan
> states **130,548** rows against 1,366 contacts — the same row count the census scaled-cap
> row prints, and the count one scratch-instrumented treant run printed beside the demand
> itself.
>
> | constant | wrapper | value | unit | derivation |
> | --- | --- | --- | --- | --- |
> | `SUPPORT_LOAD_WORK_PER_BATCH_UNIT_OBSERVED` | Measured | **31** | count/unit | 5,402,535,619 / 178,460,483 batch units = 30.28, ceiling. Same run |
> | `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` | Derived | **64** | count/unit | `next_power_of_two(2 × 31)` |
>
> **The coefficient is unchanged and nothing downstream moves**: `next_power_of_two` rounds
> 60 and 62 alike, so every materialized cap, every corpus model's floor identity and the
> treant's refusal line are bit-for-bit what they were. The registry, the M0 ledger and
> generated `constants.md` carry the corrected observation; the gate rows are untouched.

Neither floor was raised. Both are still the R5-registered inherited numbers;
they are now the constant term of a rule rather than the whole cap, and they
remain the whole cap for every corpus model — the widest corpus batch product is
the Cylinder's 27,998,135, and `64 × 27,998,135 = 1,791,880,640 < 2,000,000,000`.
The Cylinder at 89.6% of the floor is the near case: a coefficient of 128 would
take it off the floor, so a re-derivation must re-check that table rather than
assume the floors still govern.

Both caps are **declared** in the sense owner ruling R1 requires: a doc comment
on the rule at `SpecializedLimits::gate_operation_limit` and
`SpecializedLimits::load_operation_limit`, accessors that materialize them, a
pinning test written as its derivation
(`crates/plinth-support/tests/resource_cap_policy.rs`), a census row per model
per cap (`CENSUS T0_scaled_cap … materialized=… at_floor=…`), and these table
rows. Owner ruling R4's n = 1 marking and FU-23's re-derivation trigger apply to
both coefficients unchanged.

They still refuse. The treant's 39,223-proposal merge gate plans
21,755,124,103,146 operations on the same 553,835,949-unit batch — 613× the
coefficient — and is refused. That refusal is FU-8c, an algorithm defect of the
FU-8b family, not a cap to raise: admitting it would need a coefficient near
39,300, which would take most of the corpus off its floors. See
`support-resource-caps-2026-08-05.md` §7.

> **Note added 2026-08-06 (FU-8c landed at `a3308ec`) — dated addendum; the
> paragraph above is left standing as the measurement it was.** The number it
> quotes has moved and this note is the correction, not a rewrite. FU-8c was
> fixed: the merge gate no longer re-evaluates the whole batch once per proposal,
> and `gate_resource_plan` is repriced to
> `(samples + demands + 1) × contacts + measured proposal work`. On the same
> 553,835,949-unit batch and against the **same unchanged**
> `64 × 553,835,949 = 35,445,500,736` cap, the treant's stated demand is now
> **201,480,535,329** — 108× less than the 21,755,124,103,146 above, and **363.8×
> the coefficient** rather than 613×.
>
> **Both sentences above nevertheless still hold, and neither coefficient
> changed.** It *still refuses*, now 5.68× over instead of 613×, and it is still
> not a cap to raise: admitting 363.8 would need a coefficient near 364, which
> takes the Cylinder — at 89.6 % of the floor by the paragraph above — and most of
> the rest of the corpus off their floors, for the same reason 39,300 would. The
> residual is a further algorithm defect of the same family, opened as **FU-8d**
> and measured, not assumed: 97.4 % of it is per-proposal `sources × samples` work
> on the perturbed route key. Owner ruling R4's n = 1 marking and FU-23's
> re-derivation trigger are unaffected, except that FU-23 should now fire after
> FU-8d lands rather than after FU-8c. Full derivation and the exact census
> transcript: `CURRENT-STATUS.md` under *FU-8c fixed 2026-08-06*.
>
> **Note added 2026-08-06 (FU-8d phase F2) — dated addendum; the two notes above
> are left standing exactly as measured.** The 97.4 % residual they name is gone.
> A merge proposal now answers its nearest from the baseline sweep's retained
> witnesses plus one merged-seat kNN batch, so that term fell 94.2× — from
> 196,264,386,588 to 2,083,167,818 — and the treant's stated demand fell a further
> 27.6×, from 201,480,535,329 to **7,299,316,559** on the same 553,835,949-unit
> batch: **13.2× the coefficient**, and 20.6 % of the same unchanged
> `64 × 553,835,949 = 35,445,500,736`. **`gate_resource_plan` therefore now
> passes, and neither coefficient changed here either.** What does *not* hold any
> more is "it still refuses at this check": the treant still refuses at this gate,
> but from the `exact_operations` fold, where the per-variant owner-face
> descriptor scan crosses at variant 275 of 39,224 —
> `required: 35445618202, limit: 35445500736`. Over those 275 variants the exact
> `unique_face_finalizer_work` is already 3.04 × 10¹², extrapolating to
> ~4.3 × 10¹⁴ over the batch, and *that* is the remaining wall: **FU-8d part (2)**,
> the per-variant load/owner-face path, still an algorithm defect and still not a
> cap to raise. R4's n = 1 marking is unaffected; FU-23's trigger is now ruling
> R-f — re-derive only after FU-8d phase F3 lands **and** only from a treant run
> that **completes** the gate, which this is not. Full derivation:
> `support-resource-caps-2026-08-05.md` §9 and `CURRENT-STATUS.md` under *FU-8d
> phase F2 landed 2026-08-06*.

> **Note added 2026-08-06 (FU-8d phases F3 and F4) — dated addendum; the three
> notes above are left standing exactly as measured. This section carried "the
> treant is refused at this gate" from FU-8b through F2, and that sentence is now
> history: there is no refusal at this cap.** F3 finalizes the load partition
> **once** per gate batch instead of once per variant, deletes the per-variant
> `(owner, face)` window matrices, collapses the four per-proposal planning passes
> into one behind a count-derived pre-check, and prices the load phase from
> `specialized_load_resource_plan` itself rather than from the gate's own
> `variants × faces` model. The descriptor scan that crossed at variant 275 falls
> to **136,791,451 charged once**, the ~4.3 × 10¹⁴ `unique_face_finalizer_work`
> term ceases to exist, and the honest load figure — 6,592,813,742 against the
> 4,618,272,984 the model asserted — makes the **stated demand rise** to
> **9,273,857,317**, i.e. **26.2 %** of the same unchanged
> `64 × 553,835,949 = 35,445,500,736`, with `exact_operations` 10,457,910,996
> (29.5 %). **Both checks pass, the batch executes, and neither coefficient
> changed here either.** A demand that *rose* is the rule working: §7.2 asks a cap
> to price what a stage really does, and part (2) had recorded that the gate priced
> a `variants × faces × contacts` fold as `variants × faces`.
>
> **What the treant stops on now is wall-clock, not a limit.** F4's clean run at the
> F3 tip — release, `--features census`, `PLINTH_THREADS=4`, RSS watchdog armed at
> 90 GB and never fired, peak ~2.5 GiB — is an **EXCEEDS-TIMEBOX at 600 s**. So
> **ruling R-f is still not satisfied**: `SUPPORT_GATE_WORK_PER_BATCH_UNIT_OBSERVED`
> is re-derived only from a run that **completes** the gate, and this is not that
> run either. R4's n = 1 marking is likewise unaffected. F4 sampled the stack
> rather than inferring it, and the successor defect is opened as **FU-8e**; it is
> not a cap to raise. Full derivation: `support-resource-caps-2026-08-05.md` §10
> and §11, and `CURRENT-STATUS.md` under *FU-8d phase F3 landed 2026-08-06* and
> *FU-8d phase F4 measured 2026-08-06*.
>
> **Addendum 2026-08-07 — FU-8e phases E1–E4 landed; everything above stands, and
> neither coefficient moved here either.** The merge gate stopped building a kd-tree
> per proposal and stopped re-assessing coverage for a variant that cannot have
> moved the objective. **The demand these coefficients price did NOT move**:
> `plan_proposal_nearest_delta` is untouched, so the treant still states
> **9,273,857,317** against the unchanged **35,445,500,736** — a phase that removed
> 4.35× of the wall-clock and moved no charged number, which is what "no cap was
> raised" is supposed to look like. **Ruling R-f is STILL not satisfied**:
> FU-8e's own clean run is an EXCEEDS-TIMEBOX at 601 s having completed 33 of an
> estimated ~460 merge rounds, because the tournament commits one merge per round
> and the post-fix gate stage is ≈1.8–5.5 h. R4's n = 1 marking is again unaffected.
> The successor is **FU-8f**, a recorded finding awaiting owner ruling; it is not a
> cap to raise either. Full derivation:
> `support-resource-caps-2026-08-05.md` §12, and `CURRENT-STATUS.md` under
> *FU-8e phases E1–E4 landed 2026-08-07* and *FU-8f opened 2026-08-07*.
>
> **Addendum 2026-08-07 — FU-8f phase G5 landed; everything above stands, and for
> the first time since F3 the demand these coefficients price MOVES — downward.**
> G5 repriced `load_resource_plan_from_counts`'s scan term from
> `rows × contacts × (owner_log + 4)` — a model of the dense `contacts × rows`
> scan that phase G1 had *already* replaced with a CSR transpose, and which G1
> deliberately left standing and over-stating — to what the transpose really
> spends: `Σ |eligible_owners| × (log + 2) × 2` for its two eligibility
> traversals plus `cells × (LOAD_WITNESS_SLOTS + 5)` for the fill and the scan.
> Measured on the treant's round-1 merge-gate batch, 2026-08-07, on a **quiet**
> machine, one instrumented release run whose instrumentation was reverted by
> editing before the gates:
>
> | term | before G5 | after G5 |
> |---|---:|---:|
> | `baseline_rows` | 554,926,572 | 554,926,572 |
> | load (`specialized_load_resource_plan`) | 5,543,831,115 | **795,766,031** |
> | load proposal bound | 1,048,982,627 | 1,048,982,627 |
> | **gate `load` term** | **6,592,813,742** | **1,844,748,658** |
> | `demand_rows` | 42,949,185 | 42,949,185 |
> | proposal work | 2,083,167,818 | 2,083,167,818 |
> | **stated demand** | **9,273,857,317** (26.2 %) | **4,525,792,233** (12.8 %) |
> | `exact_operations` | **10,457,910,996** (29.5 %) | **5,709,845,912** (16.1 %) |
>
> The fall is **4,748,065,084 at both**, to the unit, and it is *entirely* the
> load term. **BOTH coefficients are UNCHANGED and neither is re-derived**, and
> for the gate that is arithmetic (the demand fell, the cap did not move) while
> for the load coefficient it is a deliberate refusal: the repriced load demand
> divides to 4.46 units per batch element where the derivation input was 31, and
> applying the rule would take `SUPPORT_LOAD_MAX_WORK_PER_BATCH_UNIT` from 64 to
> **8** — a cap *reduction*, which is the direction that creates refusals. FU-23
> and R4 trigger re-derivation on a **second specimen**, not on the same specimen
> getting cheaper. The derivation input of record stays the 2026-08-06 figure.
> **The finding that follows:** at 795,359,775 the treant's load demand is now
> *below* the 2,000,000,000 flat floor, so no corpus model leaves that floor any
> more; the scaling is kept regardless, because removing it would be a cap
> change. **Ruling R-f is STILL not satisfied** — G5 ran no completion attempt,
> and G4's two attempts are EXCEEDS-TIMEBOX at 602 s with 436 and 430 rounds
> counted. R4's n = 1 marking is again unaffected. Full derivation:
> `support-resource-caps-2026-08-05.md` §15, and `CURRENT-STATUS.md` under
> *FU-8f phase G5 landed 2026-08-07*.
>
> **Addendum 2026-08-08 — FU-8f phase G6c landed; every sentence above stands, and
> NO number in this section moved.** G6c added a metered "executed" figure beside
> the charge every gate round states (`GateLoadState::executed_operations`), and
> one new byte term (the retained transpose arena's held bytes) at
> `SUPPORT_SPECIALIZED_MAX_GATE_PEAK_BYTES` — a cap this section does not derive.
> Neither batch-product coefficient here is touched, re-derived, raised or
> lowered, and none of the figures in the table above moved: the CHARGE stays
> exactly what G5 repriced it to. Full account:
> `support-resource-caps-2026-08-05.md` §16, and `CURRENT-STATUS.md` under
> *FU-8f phase G6c landed 2026-08-08*.

### 3.10.11 Support execution-authority caps — the routing cadence's first charge {#route-cco-caps}

FU-32 phase J5, 2026-08-09. Everything above prices a stage that was already
charged. This pair prices a stage that **was not charged at all**: the sealed
initial-terminal CCO routing cadence (`route_initial_cco`, `runner.rs`) had no
`Limit`, no `required`/`limit` pair, no counter and no clock anywhere in
`cco.rs` or routing's half of `runner.rs` — the pre-FU-8b naked-hang shape,
recorded as finding F8 of `fu32-design-2026-08-09.md`. A 600 s wall inside an
uncharged loop is a **missing charge**, not a slow success.

Both caps are `Placeholder`, classification `runtime_resource_cap`, and both are
**additions**: no existing cap was raised, lowered or touched by this
registration.

| Stable ID | Value | Unit | Charge site |
|---|---|---|---|
| `SUPPORT_ROUTE_CCO_MAX_SEAT_EVALUATIONS` | 536,870,912 | count | `RouteCcoWorkCharge::preflight`, `runner.rs` |
| `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS` | 536,870,912 | count | same preflight, second resource |

**What is charged, and what is deliberately not.** Once J1 and J2 removed the
redundant hashing, the cadence's remaining honest work is a Θ(N·P) seat search:
every grown terminal's `discover_finalized_anchor_seats` classifies **every**
exterior anchor-pool row exactly once, and retains at most one
`AnchorSeatRefusal` row per row it refuses. Both worst cases are therefore
`cadence × pool_rows`, and both are knowable before any work is done — which is
what makes them chargeable. The controller and immutable-authority digests are
**not** charged: after J1/J2 they are O(1) and O(delta), and pricing them would
price an implementation detail (design D3, the same call G3 made when it
declined to charge the merge gate's executed prefix). The sequencing was not
negotiable either: charging before the fix would have written 843 GB of hashing
into a budget.

**Derivation, one rule for both:** `next_power_of_two(2 × the Weeping Treant
v1.2's charged demand)` = `next_power_of_two(2 × 553 × 364,223)` =
`next_power_of_two(402,830,638)` = **536,870,912**. Single specimen, n = 1, per
R4/FU-23.

**Measured at this tip, 2026-08-09** (`PLINTH_CENSUS=1`, `PLINTH_THREADS=4`,
census table `T0_route_cco_charge`, which is new in this phase and prints the
charge beside the meter so a worst case is never read as a measurement):

| model | cadence | pool_rows | charged | executed evaluations | executed refusal rows |
|---|---:|---:|---:|---:|---:|
| Sphere | 7 | 4,760 | 33,320 | 33,320 | 33,320 |
| Cylinder | 54 | 244 | 13,176 | 13,176 | 13,095 |
| make_box | 11 | 12 | 132 | 132 | 125 |
| make_overhang_table | 84 | 52 | 4,368 | 4,160 | 3,578 |
| make_toed_slab | 68 | 76 | 5,168 | 5,016 | 5,016 |
| make_raking_blade | 33 | 23 | 759 | 667 | 667 |
| make_thin_fin | 18 | 22 | 396 | 396 | 396 |
| **Weeping Treant v1.2** | **553** | **364,223** | **201,415,319** | **164,993,019** | **162,719,910** |

The treant's charge is **37.5 %** of the cap and its executed spend **30.7 %**;
the largest corpus charge (Sphere, 33,320) is 0.0062 %. `executed ≤ charged`
holds on every model and on both resources, and is enforced — not merely
reported — by `SupportRoutingError::ChargeDrift` at the end of the cadence. The
gap between charged and executed is the quarantined cadence entries, which a
preflight cannot know about in advance.

**The disclosed hazard.** At the 30.3 B per retained refusal measured in the
design's M3, `SUPPORT_ROUTE_CCO_MAX_SEAT_REFUSAL_ROWS` admits ≈16.3 GB of
retained refusal evidence — far above the ≈4.9 GB the treant's 162.7 M rows
imply, and below, but not comfortably below, the 90 GB watchdog arm and the
~100 GB machine ceiling. It is **disclosed, not ratified**. The underlying
question — is evidence proportional to the anchor pool, or to the decision? —
is the design's open question 4 and is deliberately left untouched here,
because summarising the refusal list would move every transcript digest.

Full ledger rows, provenance and the FU-24 gate:
`m0-constants-source-ledger.json`; caps-doc record:
`support-resource-caps-2026-08-05.md` §21; dated `CURRENT-STATUS.md` section:
*FU-32 phase J5 landed 2026-08-09*.

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
`p8_coverage_matrix_max_entries` (65 536),
`engine._SYNTHETIC_SELECTOR_MAX_PEAK_BYTES`
(512 MiB), `geometry_exact.ROW_BUDGET` (2 000 000), `RSS_LIMIT_GB` (2.0),
`joints._CERT_MAX_DEPTH` (22), `_CERT_MAX_PATCHES` (2 000 000),
`cco.KDTREE_REBUILD_EVERY` (25), and result-invariant batch/chunk controls.

Two similar-looking caps are **not** resource exclusions:
`coverage_repair_max_attempts = 8` and
`bracing._MAX_STACKED_FALLBACKS = 6` can change the emitted result, so they are
Placeholder registry tuning. The same rule applies generally: search/repair caps
that alter output are registry members; only demonstrated result-invariant
resource/memory guards are excluded.

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
tensile_green_mpa           0.288 MPa     ← algebraic legacy split, not a physical property
tip_safety_factor           8.0           ← policy, absorbs 3.8× of hidden conservatism
modulus_green_gpa           0.5 GPa       ← above the measured 0.2–0.8 GPa median
max_deflection_mm           0.15 mm       ← subjective ribbing threshold
DEFAULT_CONTACT_DEPTH_MM    0.3 mm        ← generation/profile default, not a format grade
LIFT_MM                     5.0 mm        ← local profile default, not vendor evidence
BRIDGE_SAG_SPAN_APPROX_MM   ~3.0 mm       ← only with the 0.300 mm section condition
ENCLOSURE_TOL_MM            0.02 mm       ← old pixel derivation rejected
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
| `X_STAR_CYLINDER` | `Pending<FiniteInterval>` → **`Measured<FiniteInterval>`**, `CalibrationPlate` 6 with `Exact(2026-07-31)`, carrying `(19.91, 30.48)` as exclusive/exclusive and `err: null`. Store the interval, not 24.6. The `note` field carries all three conditionals: n = 1 per bracket edge, 4.00 mm arms, 30 mm² cap. |
| `X_STAR_CONE` | Unchanged. **Keep it a separate named constant** — the two are measured on different geometries at very different precision, and merging them would launder that difference away. |
| `X_ALLOWED` | **Stays `Derived<f64> = 12.0` from `[X_STAR_CONE]`: `floor(lower(X_STAR_CONE)/2) = floor(25.00/2) = 12`.** It is *not* re-derived from the new plate-6 bracket: plate 6 is single-specimen at each edge, so it supplies only the **margin check** `19.91 / 12 = 1.66×`. **No arm in the engine resizes.** Do not hardcode 12.0 anywhere except this one derivation. |
| `TENSILE_GREEN_MPA` | Unchanged numerically at 0.288 MPa, but classified correctly as a diagnostic `Placeholder`, not a `Derived` carrier and never an input to acceptance. |
| Arm-geometry model | The brackets agree, so **there is no measured taper penalty** and tip-disc sizing stands. The fast-flare requirement of §1.3(b) keeps its original justification — the *observed* failure location — and the minimum-section locator stays **required**, because the X rule applies at the minimum section wherever it is. Neither is carrying a measured cone-vs-cylinder discount, and neither may be sold as if it were. |

**`X_ALLOWED` is still never a `Pending`** — 02 §3.5 declares it `Derived`, and after
2026-07-31 there is no open question left for it to carry.

### 4.2 Question (b) — RESOLVED: sharing is **at least additive**; the zipper model is **refuted**

**The question was.** When N arms serve one down-face region, does the group share the
load (**additive**: the region's total load is divided, so `X_tot/N` governs), or
does each arm meet the release front alone at its own full X (**zipper**)? §2.4
explains why `leading_release_edge = NegativeY` and a front moving toward
`PositiveY` made this look like an
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
   result bounds sharing from below only; N > 6 is extrapolation.
3. Strip arms carry a **~19.75 % fin-root section bonus** that the mono cells lack. Treat
   sharing as **"at least N×"** and **never design on a super-additive margin** — part
   of the observed headroom is section, not sharing.
4. On this plate `X_tot` is **algebraically degenerate** with total arm area (a single
   cap area was used). The **ratio** form rests on **plate 5's** cap-area spread, not
   on plate 6.
5. The strips-vs-mono flip at N = 2 is tight: alive `X_tot = 30.077` against dead
   `X = 30.481` forces an exact single-boundary additive model into a **1.33 %
   window**. The parsimonious reading includes caveat 3's section bonus; do not claim
   exact additivity.

**Separate orientation limitation.** Strips along X and Y both survived, so an
axis-dependent zipper collapse at `X_arm = 60` is excluded at this load. Every
strip survived, however, so a smaller near-boundary orientation effect is untested.
Keep the axis input, but it is unused by the current measured formula. This is not
a substitute for caveat 5.

**Separate admissibility limitation.** B6 was admitted only through the explicit
post-hoc override of a pre-registered gate whose literal text voided the whole plate.
The grounds and the unanswered counter-argument are recorded in
`../reference/plate-06-results.md` §6. That override must travel as a citation with
the constant; it is not silently normalized into the five core caveats.

**What updates:**

| Constant / branch | Resolution |
|---|---|
| `LOAD_SHARING` | `Pending<LoadSharing>` → **`Measured<LoadSharing>`**, `CalibrationPlate` 6 with `Exact(2026-07-31)`, explicit `err: null`, value **`LoadSharing::Additive { n_eff_cap: 6.0 }`**. The passive tagged type lives in `plinth-core::load`; the future declaration lives in `plinth-core::constants::load`, and `plinth-support::load` later owns `ArmGroup` / `effective_x` behavior. |
| `n_eff_cap` | **Derived 6.0 from `[LOAD_SHARING]`** — the measured `Additive` payload's largest tested group size. Validate it as finite and `>= 1.0`. Caveat 2 gives no upper bracket, so the discount is granted only across the measured range; past N = 6 it is an extrapolation. It belongs in the same `note`, not a separate measurement. |
| `N_eff` | **All arms rigidly co-carrying the region — no front-band filter.** The separate orientation result removes that restriction because sharing appeared along both axes. `N_eff` still requires genuine co-carrying under one contiguous rigid region, never mere proximity. |
| Orientation term | **None.** Do not add an axis term to sizing or placement until the separate orientation limitation is contradicted. Retain the release-axis input to `effective_x` so a term can be added later without a signature change; document that it is currently unused by measurement. |
| `LoadSharing::Zipper` | **Kept in the enum.** It is the strictly-conservative branch (per-arm sizing can only over-size) and it is the A/B arm that `cargo test -p plinth-support --test load_sharing_both` and `plinth calibrate` exercise. Refuted is not deleted. |
| `tip_r_max_mm` cap | Stops being a hard wall. A region needing more than 37.70 mm² is served by an `ArmGroup`, not by one fatter tip. **The load-driven fixer of §1.5(i) ships regardless** — a group still has to be *created* in response to a load violation. |
| Placement / CVT | The sharing term is a pluggable weight on the cell-area demand in the §2.3 objective, never an inline constant. It is now a *measured* weight rather than a speculative one, but the plumbing is unchanged. |

```rust
// plinth-core::load now — passive type and validation only.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EffectiveArmCount(Finite);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LoadSharingError {
    #[error("n_eff_cap must be finite")]
    NonFinite,
    #[error("n_eff_cap must be at least 1.0")]
    BelowOne,
}

impl EffectiveArmCount {
    pub fn new(value: f64) -> Result<Self, LoadSharingError> {
        let value = Finite::new(value).map_err(|_| LoadSharingError::NonFinite)?;
        if value.get() < 1.0 {
            return Err(LoadSharingError::BelowOne);
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoadSharing {
    Zipper,
    Additive { n_eff_cap: EffectiveArmCount },
}

impl LoadSharing {
    pub fn additive(n_eff_cap: f64) -> Result<Self, LoadSharingError> {
        Ok(Self::Additive { n_eff_cap: EffectiveArmCount::new(n_eff_cap)? })
    }
}

impl<'de> Deserialize<'de> for LoadSharing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum ZipperKind {
            Zipper,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum AdditiveKind {
            Additive,
        }

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ZipperWire {
            kind: ZipperKind,
        }

        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct AdditiveWire {
            kind: AdditiveKind,
            n_eff_cap: f64,
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Zipper(ZipperWire),
            Additive(AdditiveWire),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Zipper(ZipperWire {
                kind: ZipperKind::Zipper,
            }) => Ok(Self::Zipper),
            Wire::Additive(AdditiveWire {
                kind: AdditiveKind::Additive,
                n_eff_cap,
            }) => {
                Self::additive(n_eff_cap).map_err(serde::de::Error::custom)
            }
        }
    }
}

// The per-variant kind enums plus deny_unknown_fields make the wire strict:
// {"kind":"zipper","n_eff_cap":1.0} is rejected as a surplus/cross-variant field,
// and {"kind":"additive"} is rejected because the additive cap is required.

// Later M0 constants work: plinth-core::constants::load declares:
pub static LOAD_SHARING: Measured<LoadSharing> = /* LoadSharing::Additive { n_eff_cap: 6.0 }
    — plate 6, 2026-07-31. Measured requires this date. 12/12 strips survived X_arm = 60.15
    at N = 2,3,4,6 (pitch 2.0 and 2.8 mm, arm lines along X and along Y) while the matched mono
    died at the same X. Survival tracks X_tot = A_cap/(N*A_arm). AT LEAST additive: never credit
    more than N. `err: None` serializes exactly as `"err": null`. Caveats 1-5 of 03 section 4.2
    belong verbatim in `note`. `plinth-support::load` reexports LoadSharing and owns ArmGroup /
    effective_x behaviour. */;
```

> **One mechanism wrinkle.** `Measured<T>` (02 §3.5) carries `err: Option<T>`. For a
> **categorical** payload like `LoadSharing`, use `err: None` and serialize it explicitly
> as `"err": null` alongside an explicitly tagged `LoadSharing` variant. Do **not** invent
> a numeric error, omit the field, or widen the enum to carry uncertainty: the real
> uncertainty — all five caveats above — belongs in `note`. A categorical measurement's
> lack of an error bar must stay visible in the type and wire form, not be papered over.

The passive payload is the explicitly serde-tagged `enum LoadSharing { Zipper,
Additive { .. } }` from `plinth-core::load` now — **not** a scalar discount factor.
The name is still `LOAD_SHARING`, not `MULTI_ARM_SHARING`, when the later M0 constants
work declares it in `plinth-core::constants::load`. `plinth-support::load` later
reexports the type and owns `ArmGroup` / `effective_x` behaviour. Route **all**
group-sizing through that later declaration, so the A/B against `Zipper` stays runnable
and so the corpus-wide delta of the 2026-07-31 switch is measurable rather than assumed.

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

5. **RESOLVED for the port — `lift_mm` had two live values.**
   `config.SupportConfig.lift_mm = 5.0` is the canonical `LIFT_MM` diagnostic
   Placeholder. `pipeline.prep_one`'s `SupportConfig(lift_mm=3.0)` is retired,
   not aliased, and emit loses its independent override authority. One value and
   one profile/frame owner in Rust.

6. **Squeeze-film suction, 40–60 kPa at 30–40 °C.** The owner's brief records
   this figure. **No in-repo source states it.** The nearest repo corroboration is
   `LITERATURE-NOTES.md`: Cao measured 45 kPa on a **PDMS** vat (a materially
   stickier interface than FEP) and Materialise e-Stage ships a 45 kPa default.
   Both are the right order. Keep it as condition-bearing conflict context, not a
   runtime registry declaration, and note that **it is not the number the code uses** — the code's
   `peel_pressure_pa = 3000 Pa` sits an order of magnitude below it, which is one
   more reason that constant must never be quoted as a physical property.
   **Plate 6's scoring analysis lands in the same band from a different direction**
   (§1.4): the only self-consistent reading of `X* ≈ 25` is a cavitation-capped
   squeeze of ~100 kPa on flat patches of ~5 mm and up. Same order, independently
   arrived at — and still not a number the sizing path may use.

7. **`BRIDGE_SAG_SPAN_APPROX_MM ≈ 3.0` at a 0.300 mm section.** The owner's brief records this
   as approximate. In-repo, `bridge_free_gap_mm = 1.0 mm` is the only self-support
   gap constant and it has no measured provenance;
   `LITERATURE-NOTES.md` reports a **2.4 mm** maximum self-supporting overhang
   distance from a different setup, explicitly flagged as setup-specific rather
   than a universal reach. Treat the ~3 mm figure as `Placeholder` with an
   approximate owner attribution, and do not derive anything load-bearing from it.
   It remains distinct from the `BRIDGE_FREE_GAP_MM = 1.0` Placeholder.

8. **`SupportConfig.notes()` mis-classifies its own constants** — §0. Do not port
   its buckets.

9. **`chitubox_emit.PLATE_BAND_MM` is a hand-maintained duplicate** of
   `joints.PLATE_BAND_MM` / `verify.connectivity_report`'s rule, kept independent
   "per the module's style". Nothing enforces they stay equal. One typed constant
   in Rust.

10. **Historical deliberate duplication — superseded by O-5.** The Python
    SCAR-AREA-CAP and PRINT-FLOOR audit copied constants and algorithms by hand.
    Rust must not reproduce that duplication: one typed registry value and one
    capability implementation serve producers and the disk-first scoring adapter.
    Fresh disk provenance plus analytic, injected-corruption, and native fixtures
    provide the independent evidence instead.

11. **`contact_depth_mm = 0.3` was over-graded as format evidence.** ChituBox
    stores a variable field; 0.3 mm is the project's
    `DEFAULT_CONTACT_DEPTH_MM` generation/profile Placeholder. `chitubox_io`
    owns the field representation, not this default.

12. **`ENCLOSURE_TOL_MM = 0.02` had an invalid pixel derivation.** LCD pitch and
    layer height do not establish an enclosure-certificate tolerance. Keep 0.02
    as a Placeholder; `ENCLOSURE_PROOF_TOL_MM = 0.018` is only the diagnostic
    arithmetic `0.9 ×` of that Placeholder.

13. **Release-front direction was phrased backward.** The settled fields are
    `leading_release_edge = NegativeY` and
    `release_front_sweep_direction = PositiveY`; the front moves `-Y → +Y`.
    Older “front-to-back in -Y”/“-Y sweep” prose is retired wording, not a third
    kinematic quantity. The axis input is unused in the current load formula.

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
