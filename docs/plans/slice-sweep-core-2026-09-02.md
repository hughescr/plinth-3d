# Slice-sweep support core — unified build document (v1)

Status: **BUILDABLE DESIGN**, written 2026-09-02 read-only against `feature/stage0-bytes-audit` @ `02b1d78`.
No cargo was run to produce it. Every code claim is cited `path:line` at that commit and was
re-verified for this document; where the four source specs disagreed on a line number the
verified one is used and the disagreement is recorded in §2.

It unifies four specs written in parallel:

| source | lives at | role here |
|---|---|---|
| algorithm | `plinth-evidence/slice-sweep-2026-09-02/specs/sweep-algorithm.md` | §4–§8, §11 |
| reuse inventory | `.../specs/reuse-inventory.md` | §9, §10 file lists |
| oracles | `.../specs/oracles.md` | §7 (arithmetic corrected here) |
| orientation peel | `.../specs/orientation-peel.md` | §12 (LANE 3) |

**Why this exists.** `plinth-evidence/disc-diagnosis-2026-09-02/DISC-DIAGNOSIS.md` §0/§3:
flat discs fail because the support stage keys its coverage raster and its load ledger to
**triangles** (RC3: a 98 mm sliver costs 65,536 refinement cells; RC4: each whole triangle is
charged to the seat nearest its centroid, so half a fan's seats carry nothing and emit as
hairline tips while a fifth clamps at the 1.0 mm maximum, load-bearing tips 8.5–9.5 mm apart
against a 2.6 mm measured reach), and because the orientation objective double-charges the
correct flat pose through two placeholder weights.

**The gate for everything below**: Round 25/50/100 emit in seconds, recess UP, underside
supported at 3–5 mm spacing, zero bare islands, no refusal — judged by the owner opening the
files in ChituBox, produced through `plinth_emit::write` (`crates/plinth-emit/src/lib.rs:98`)
and no other writer.

---

## 1. The algorithm in one paragraph

Slice the oriented, lifted model onto a square working raster, one mask per print layer.
Sweep **upward** carrying a dimensionless reach potential `U` through cured material: a cell
is *rooted* when `U ≤ 1`, where each millimetre of path costs `1/(κ·τ)` with `τ` the cured
thickness along that step and `κ` the measured lamina reach ratio. Model cells with `U > 1`
are demand; insert struts at maximum radius, greedily, each covering the largest reachable
demand area, each descending straight down to the cheapest rooted landing (plate, model, or
another strut). Then sweep **downward** carrying a load raster in mm² of reference peel: per
layer, spread the incoming load into this layer's cured cells with force conserved, take the
per-cell **maximum** against this layer's own peel event, let struts read their share and
carry it as columns without spreading, inject a landing strut's carried load at its footprint,
and size each strut's section at each layer as `A = ℓ / X_ALLOWED`. Drop struts whose load is
below the smallest printable tip and hand their load to neighbours. Emit through the existing
`SupportedModel` → `plinth_emit::write` boundary. Never refuse; warn.

---

## 2. Conflicts between the four specs, and how each is resolved

Every row is resolved on evidence, not preference. **C1, C2 and C3 change numbers the oracle
tests will assert**, so they are the load-bearing part of this document.

### C1 — Disc demand area: measured bottom face, not `πD²/4`

The algorithm spec sized the disc oracle from the full footprint `πD²/4` (Round 50 = 1963.5 mm²
⇒ N = 167); the oracle spec used the measured flat bottom face (1809.5 mm² ⇒ N = 154).

**Resolved: the measured bottom face.** A probe of the source STLs
(`plinth-evidence/disc-diagnosis-2026-09-02/first-principles/FINDINGS.md` §0) shows the rim is a
**drafted frustum**, `r 24→25 over z −3.5→0` on Round 50, normal `nz = −0.276`. The
bottom-most layer's cross-section is therefore a circle of `r = 24.0`, area
`π·24² = 1809.6 mm²`, which matches the measured 1809.5 mm² to 4 significant figures. The
rim's projected annulus (`560.3 × 0.276 = 154.6 mm²`) is not demand at layer 0; it appears
layer by layer as a 0.0143 mm/layer step and is self-rooted (§C4). `1809.5 + 154.6 = 1964.1 ≈
πD²/4 = 1963.5` closes the arithmetic. Same check on Round 100: `7538.4 + 1132.0·0.276 =
7850.8 ≈ 7854.0`.

### C2 — Conservation total: the **peak layer**, not the bottom layer, and not the footprint

Both the algorithm spec's O1 (`Σℓ = A = πD²/4`) and the oracle spec's O1 (`Σℓ = 1809.5`) are
wrong for the same reason: with **max over events** (§6.3) the load delivered to the plate is
the largest single layer cross-section, and for a *drafted, recessed* disc that layer is
neither the bottom nor the rim top.

Round 50 profile: rim `r = 24 + (z+3.5)/3.5`, recess void from `z = −2.5` upward. The largest
solid cross-section is the layer immediately **below the recess floor**, `z = −2.5⁻`:

```
r_out(−2.5) = 24 + 1.0/3.5 = 24.28571 mm      A_peak = π·589.795 = 1852.83 mm²
Round 100:  r_out(−2.5) = 49.28571 mm         A_peak = π·2429.08 = 7630.94 mm²
```

Above `z = −2.5` the section drops to an annulus (`≈286 mm²` on Round 50), so 1852.83 is the
supremum. The diagnosis's own tilt table quotes 1963/7854 at 0° because it integrates a
*plain cylinder* (`first-principles/FINDINGS.md` §1.2 header: "closed-form slab∩cylinder
integral") — a model that has no recess. **Resolved: `Σ carried = max over layers of the cured
cross-section area`, and the oracle harness must compute that maximum itself from the slice
stack rather than being handed a number.** The hand predictions above are the grading target.

This lands a second, useful identity: that maximum is exactly `profile.peak_mm2()`, the
quantity the orientation objective already computes (`crates/plinth-orient/src/score.rs:969`).
So **`total scar = peak_mm2 / X_ALLOWED`** ties LANE 1 and LANE 3 together, and both lanes
must report it.

### C3 — Reach fringe is a Minkowski sum with a **disc**, not a grown square

The oracle spec grew each 4×4 mm table leg to `8.26² = 68.23 mm²`. The reachable set from a
square root region is the square dilated by a disc of radius `ρ₁` — rounded corners:

```
A_fringe(w) = w² + 4·w·ρ₁ + π·ρ₁²
w = 4:   16 + 34.0805 + 14.2536 = 64.334 mm²   (not 68.23)
```

**Resolved: rounded corners.** Table strut count moves 54 → 55; leg `X_model` moves 4.26 → 4.02.

### C4 — Thickness-scaled reach: real, but never binding in v1's acceptance set

The oracle spec declined the brief's `reach = κ·τ` scaling as unmeasured beyond one lamina;
the algorithm spec's `U = ∫ds/(κτ)` has it built in. Both are right about something.

**Resolved: keep `U = ∫ds/(κτ)` (it is the physics — `L_crit = τ·√(σ_f/3P)` is exactly linear
in `τ`), and note that it never sets a number in this document's acceptance set**, because
every demand decision in the disc/T/cavity/cylinder oracles is taken at a **birth layer**,
where the newly cured material has `τ = h` and the reach is exactly the measured
`L_SURVIVED_MAX_MM = 2.1300322500 mm`. No extrapolation enters any predicted number. Where
the scaling *does* act (the rim ramp, a wall fringe reached through thick material) it only
makes things more permissive, and the run report publishes `max_tau_at_binding_reach_mm` so a
model where it binds is visible rather than silent.

Two supporting facts, so the form is not taken on faith:

* **Vertical travel is nearly free and provably bounded.** A column rooted at its base has
  `τ = k·h` at layer `k`, so the vertical path costs `Σ_{k=1..K} 1/(κ·k) = H_K/κ`. At
  `κ = 42.6006450`, `K = 2100` (the tallest corpus model) gives `8.25/42.6 = 0.194`. Vertical
  inheritance can never on its own unroot anything in this corpus; lateral excursion decides.
  Reported as `u_from_vertical_max`.
* **The rim ramp checks out.** Round 50's rim rises 3.487 mm over 70 layers while moving 1.0 mm
  outward. Lateral cost `Σ 0.0143/(κ·k·h) = (0.0143/2.130)·H_70 = 0.032`; vertical `0.113`;
  `U ≈ 0.15 ≪ 1` ⇒ **0 rim struts**, matching the ramp identity in `FINDINGS.md` §0. A
  single-lamina-only variant (`τ ≡ h`) would charge `70/42.6 = 1.64` for the same climb and
  wrongly demand rim supports — which is why the naive form is rejected, not merely disfavoured.

### C5 — `κ` for placement: 42.6006450, computed at run time, never rounded

The brief says "≈43"; the oracle spec asked whether to register `floor(42.6) = 42`; the
algorithm spec asked the owner to choose between 26.8995249 (audit gate), 42.6006450 (pooled
lower bound) and 51.84072 (first loss).

**Resolved: `LAMINA_REACH_RATIO = L_SURVIVED_MAX_MM / LAYER_HEIGHT_DEFAULT_MM`, computed from
its parents at run time and never hard-coded or rounded** — the rule
`docs/specs/audit-span-criterion.md:225-229` already states for this class of constant. The
pooled lower bound is the placement value on evidence:

| basis | reach `ρ₁` (mm) | hex spacing (mm) | implied `κ` |
|---|--:|--:|--:|
| audit gate `L_SPAN_ALLOWED_MM` (`audit-span-criterion.md:223`) | 1.3449762 | 2.3295 | 26.900 |
| **pooled lower bound `L_SURVIVED_MAX_MM` (chosen)** | **2.1300323** | **3.6892** | **42.601** |
| owner's ChituBox practice, 4 mm spacing | 2.3094 | 4.0000 | 46.19 |
| owner's Cylinder field print, 47 tips on 706.86 mm² | 2.4074 | 4.1697 | 48.15 |
| first loss (`audit-span-criterion.md:177`) | 2.5920360 | 4.4895 | 51.841 |

Two *independent* field data points (4 mm practice; the 47-tip Cylinder, from
`disc-diagnosis-2026-09-02/challenge-r2/FINDINGS.md:61,82`) sit inside the measured bracket and
far outside the audit gate's. The chosen value is the conservative end of the measured
bracket and therefore places **denser** than the owner's own practice. Placing at the audit
gate would place 2.5× denser still, for no measurement — the gate subtracts a 1.53× condition
shift and two pixel diagonals *because it exists to refuse a finished file*
(`audit-span-criterion.md:196-229` rule 2), and subtracting uncertainty is right for a gate and
wrong for placement.

The algorithm spec's supporting claim "≈86 seats on Round 50" does **not** follow from 4 mm
spacing and is dropped: 4 mm hex on 1809.5 mm² is `1809.5/(0.866·16) = 131` seats. Only the
spacing figure is used.

**Consequence, disclosed:** an emitted field placed at 42.601 will fail an audit span check
written at 26.900. That is OR-2 (§14) and is out of scope for LANE 1–3; the run report
publishes `kappa_used`, `kappa_audit_gate` and `will_pass_audit_span_check` so it is never
decided by accident, and sabotage S8 keeps the two values from being swapped in code.

### C6 — Sizing reads `X_ALLOWED` directly

`crates/plinth-support/src/perception.rs:641-648` sizes tips from
`TENSILE_GREEN_MPA / (PEEL_PRESSURE_PA · TIP_SAFETY_FACTOR)`. All three are registered
Placeholders (`placeholders.rs:1354`, `:1538`, and the safety factor) whose only justification
is that they multiply out to 12. **Resolved: the sweep reads `X_ALLOWED` (Derived, 12,
`crates/plinth-core/src/constants/evidence.rs:131,359`) directly.** Reading the triple
individually is three placeholders deciding an outcome.

### C7 — Peel field off by default; and there is no tilt term anywhere

The brief asks for a per-pixel peel field with an area exponent and a Saturn 4 front-to-back
tilt factor. The algorithm spec computes both and lets neither decide; the orientation spec
refuses the tilt factor outright.

**Resolved: `α ≡ 1` in the sweep (`SWEEP_PEEL_FIELD_MODE = Uniform`), and no tilt factor in
either the sweep or the orientation objective.** Evidence, not preference:

* `docs/physics.md:69-75` — the measured `X` boundary collapsed across a 40× cap-area span.
  That is direct evidence *against* a strong area exponent inside the load path.
* `docs/physics.md:150-158` — "**Y-sensitivity is settled**… around one rung… The standing
  guidance is to **randomize over that axis rather than tune it**… There is no orientation
  term in the sizing path today; the input is plumbed and unused."
* `crates/plinth-orient` searches `UpDirection` over S², which structurally has no azimuth;
  a tilt factor would require widening the search space, a materially larger change than the
  brief describes. The existing `z_spin_is_a_null_direction_of_every_term` test asserts this.

`β_area` and `β_tilt` are computed and **reported** so a future coupon can be graded against
them, and the mode switch exists. Turning either on is OR-1 (§14) because it contradicts the
brief's literal ask.

Where the area law *does* enter is LANE 3's orientation term, which compares whole poses
rather than sizing an arm — see §12.

### C8 — Line-number and naming disagreements (verified at `02b1d78`)

| claim | algorithm spec | reuse spec | verified |
|---|---|---|---|
| production `run_support` call site | `prepare.rs:1880` | `prepare.rs:1926` | **`prepare.rs:1880`** |
| `SupportedModel::from_parts` | `lib.rs:518-546` | `lib.rs:527-560` | **`crates/plinth-support/src/lib.rs:529`** |
| `is_semantically_valid` | `:591-624` | `:566-621` | **`:591`** |
| `SupportPrimitive` / `SupportSegmentKind` | `:445-462` / `:208-216` | `:196-464` | **`:445` / `:208`** |
| `SupportHistory::from_reports` | `:480` | `:418-518` | **`:480`** |
| `normalized_slice_layers` | `perception.rs:1506-1540` | `:1454-1560` | **`:1506`** |
| `orient_layers_batch` | — | `perception.rs:1961`, `pub(super)` | **`:1961`, inside private `mod orientation` at `:1627`** |
| `slice_z_batch` | `slicing.rs:112` | `:113+` | **`rust/geom_kernel/src/slicing.rs:112`** |
| `normalize_layers_batch` | `plinth-layers/src/lib.rs:689` | same | **`:689`** |
| `lower_stub` / `root_boss_top` | `lower.rs:359-410` / `:416-437` | — | **`:362` / `:416`** |
| new-crate name | `plinth_sweep` | `plinth-sweep` | **`crates/plinth-sweep`**, `members = ["crates/*"]` (`Cargo.toml:2`) so no workspace edit is needed |

### C9 — The orientation support term: no interim proxy

The orientation spec proposed an interim "discount the pose's own planar rest face" proxy
because the sweep did not exist, and flagged its own exact-angle cliff as a hazard.
**Resolved: dropped.** The lane order below puts the sweep crate (LANE 1) and its pipeline
wiring (LANE 2) *before* the orientation term (LANE 3), so LANE 3 consumes real coarse-raster
demand from `plinth-sweep` and the proxy — and its cliff, and the open question about its
tolerance band — never needs to exist.

---

## 3. Doctrine audit

| rule | how this design satisfies it | where to check |
|---|---|---|
| **No tuned constant** | `κ` is computed from two registered Measured parents at run time. `X_ALLOWED` is read as the registry publishes it. Nothing in the sweep is fitted to make a disc come out right; the disc numbers in §7 were derived before the code exists, which is the point of the oracles. | §5, §7 |
| **A constant may prune, never decide** | Four Placeholders touch the sweep. `SWEEP_CELL_LAYER_BUDGET` **coarsens and warns**, never refuses (§4.2). `SWEEP_STRUT_REMOVAL_PASSES` bounds a redistribution search whose rule *keeps* any strut whose demand would otherwise go uncovered (§6.6), so it can never create a bare island. `SWEEP_PEEL_FIELD_MODE` defaults to the identity, under which the sweep reduces exactly to the registered area law. `SWEEP_WORKING_PITCH_MM` is a discretization and *does* move outputs — so it is bounded by argument (§4.2), reported (`achieved_pitch_mm`), and pinned by a **pitch-halving invariance test** (P1, §8). | §5.2, §8 |
| **Emit and warn, never refuse** | There is no refusal path in the sweep. Every shortfall is a report field on an emitted file (§13). The only hard errors are internal-consistency assertions — a conservation residual above tolerance, a non-monotone carried profile, a primitive the emit boundary rejects — because those are defects in this code, not outcomes about a model. | §13 |
| **Batch-first** | Every per-layer step is a whole-array or scanline pass. Banned in this crate's hot path, by name: per-triangle refinement of any kind (the RC3 mechanism), `rust/geom_kernel/src/poly2.rs:232` `point_in_polygon` per cell, per-seat nearest-face queries (one batch per model instead, §9.3), and global rescoring inside the greedy loop. | §4.6, §6.4 |
| **Lift always on** | `plate_z = min_z − lift` (`crates/plinth-support/src/runner.rs:2178-2186`), reused unchanged; the sweep asserts `plate_z < min_z` on entry and S10 makes a zero-lift pose a red test. A flat underside on a short plate-rooted field is the *correct* outcome for a base, not a cost to be optimised away. | §4.1, §8 |
| **Verify against the real consumer** | Every emitted file goes through `plinth_emit::write` (`crates/plinth-emit/src/lib.rs:98`) with its reopen-and-audit; the adapter constructs `SupportPrimitive`s that satisfy `is_semantically_valid` (`crates/plinth-support/src/lib.rs:591`) rather than a bespoke synthesiser. LANE 2's gate is the owner opening the files in ChituBox. | §9, §11 |
| **Python banned; "minimum" not "floor"** | No `.py` in any lane. Every smallest-allowed value in this document is called a *minimum*. | — |

---

## 4. Data model and rasterisation

### 4.1 Frame

Identical to the existing support stage, so emitted primitives need no re-framing:

```
plate_z_mm = min_z(oriented mesh) − lift_mm         crates/plinth-support/src/runner.rs:2178-2186
layer k covers z ∈ [plate_z + k·h, plate_z + (k+1)·h)
sample plane   z_k = plate_z + (k + 0.5)·h
K = ceil((max_z − plate_z) / h)
```

`BuildFrame::new(plate_z, lift)` (`crates/plinth-support/src/lib.rs:423`) is handed to emit
unchanged. **The sweep derives its own `K` and truncates the pipeline's `LayerPlan` at the
model top**: `prepare.rs:1831-1833` builds the plan with a `+1.0 mm` headroom and a
1,000-layer minimum, which are the sealed-run gates' plan shape, not the sweep's; slicing 830
empty planes for a disc is waste, not error.

### 4.2 Working raster

Square cells of side `p = SWEEP_WORKING_PITCH_MM = 0.050 mm`, axis-aligned, origin at the
oriented bounding-box corner minus `TIP_RADIUS_MAX_LEGACY_MM + p` so a maximum-radius strut at
an edge cell is inside the grid. `W = ceil((bbox_x + 2·margin)/p)`, likewise `H`; cell area
`a = p² = 0.0025 mm²`.

Why `p` may be coarser than the printer pixel (0.014 × 0.019 mm, `crates/plinth-core/src/printer.rs:159-160`):

* Placement to the nearest cell moves a strut centre by at most `p/2 = 0.025 mm`, and
  `0.025 < d3 = 0.055290 mm` (`tools/plinth-audit/src/consts.rs:194`), the band inside which
  the audit itself refuses to decide fusion. A grid that cannot move a landing further than
  the undecidable band is not the binding error.
* Strut **sizing** is analytic (`A = ℓ/X`), never read off the raster.
* `p = h`, so the working voxel is isotropic and one length unit serves both the geodesic and
  the vertical step. `p` also equals `TIP_RADIUS_DEFAULT_MM`, giving two cells across a
  minimum tip diameter.

**Coarsening.** If `W·H·K > SWEEP_CELL_LAYER_BUDGET`, set `p ← 2p` and repeat; the achieved
pitch and step count are reported. It **never** fails Preparation — RC3's flat 2e6-cell cap
that failed the stage is exactly what converted a resolution question into a lost artifact. At
`p > 2·r_min` a minimum strut is sub-cell; its footprint is then capped analytically at
`min(cells within r, ceil(π r²/p²))` cells nearest the centre, so a coarse grid cannot turn a
thin strut into a fat carrier.

Budget arithmetic at the default 2.0e9 cell-layers (`K` includes the 5 mm lift):

| model | bbox mm | K | `p` chosen | `W×H` | cell-layers |
|---|---|--:|--:|---|--:|
| Round 25 | 25.00 × 25.00 × 3.50 | 170 | 0.05 | 542 × 542 | 4.99e7 |
| Round 50 | 50.00 × 50.00 × 3.50 | 170 | 0.05 | 1042 × 1042 | 1.85e8 |
| Round 100 | 99.98 × 99.99 × 3.50 | 170 | 0.05 | 2042 × 2042 | 7.09e8 |
| Cylinder | 30 × 30 × 30 | 700 | 0.05 | 642 × 642 | 2.89e8 |
| make_overhang_table | 30 × 30 × 24 | 580 | 0.05 | 642 × 642 | 2.39e8 |
| Weeping Treant v1.2 | 82.18 × 53.66 × 96.44 | 2029 | **0.10** (coarsened once, 3.82e9 → 9.54e8) | 843 × 558 | 9.54e8 |

### 4.3 Per-layer masks

Two masks per layer from one scanline pass:

* `Mcover[k]` — **any-overlap** fill (outward rounding). The **demand** mask. Over-stating
  demand is the safe direction.
* `Mcarry[k]` — **full-containment** fill (inward rounding). The **carrier** mask: what counts
  as cured material for reach and for load. Under-stating carriers is the safe direction. Same
  rule and same stated error direction as `tools/plinth-audit/src/raster.rs:13-17` — **as a
  precedent only**; `Cargo.toml:2-11` excludes `tools/plinth-audit` from the workspace
  precisely so the audit stays independent, and nothing may be imported from it.

`Mcover ⊇ Mcarry`; their difference is the one-cell boundary ring.

### 4.4 Scanline rasterisation

For layer `k`, for each raster row `j` (cell-centre `y_j`), collect crossings of every loop of
every solid with `y = y_j`, sort, pair by the normalized fill rule (unambiguous by
construction, `crates/plinth-layers/src/lib.rs:9-15`). Each pair `[x_a, x_b]` becomes a
`Mcover` span `[floor((x_a−x0)/p), ceil((x_b−x0)/p)−1]` and a `Mcarry` span
`[ceil((x_a−x0)/p), floor((x_b−x0)/p)−1]`. `Mcarry` is then eroded by one cell in Y wherever a
run's row neighbours differ — one pass over the run list that never over-states a carrier.

Cost is `O(edges·rows_touched + spans)`: proportional to boundary length and filled area, **not
to triangle count and not to any edge-length function**. A disc's 102-triangle fan and its
800-triangle uniform mesh produce the same runs; S6 (§8) makes that a test.

### 4.5 Strut record and field arrays

```
SweepStrut { id, cx, cy, cell:(row,col), k_top, k_bottom, landing,
             radius_mm: Vec<f32>, carried_mm2: Vec<f32>, seat_normal, seat_kind }
Landing = Plate | Model{ k, interior, delicacy } | Strut{ id, k }
```

A strut is analytic and rasterised on demand; it never exists as a stored voxel set. **A strut
roots only its centre column, not its rasterised disc**, so up-sweep rootedness is independent
of a radius the down-sweep has not chosen and shrinking can never invalidate a proved
placement. This under-states a fat strut's reach by up to `r` — the safe direction.

Per-layer arrays (one layer live at a time): `U: f32` (reach potential, `∞` = unreachable),
`tau: u16` (cured layer run), `top_model: u32`, `top_strut: u32`, `L_in`/`L_out: f32` (load,
mm² of reference peel). Peak resident memory for Round 100 at `p = 0.05` (4.17e6 cells) is
≈93 MB; the treant at 0.10 is ≈23 MB. Both far under the machine ceiling.

### 4.6 Batch-first rules (binding)

Every per-layer step is a whole-array or scanline pass. No per-cell `point_in_polygon`
(`rust/geom_kernel/src/poly2.rs:232`), no per-triangle refinement, no per-seat mesh query. The
greedy does **local** score invalidation within `2ρ̄` of each insertion, never a global rescore:
on Round 50's bottom layer that is `154 × (4ρ̄/p)² ≈ 4.5e6` cell visits (0.4 % of the layer
budget) against `154 × 1.09e6 = 1.7e8` for a global rescore. Reductions run in fixed row-major
order; any rayon use is per-row with partials combined in index order, so results are
bit-identical to the serial ones.

---

## 5. Reach, rootedness, and the constants

### 5.1 The reach potential

At layer `k`, `Cured(k) = Mcarry[k] ∪ Strut(k)`. For a cell `c ∈ Cured(k)`,

```
U_k(c) = min over paths through cured material to a true root of  ∫ ds / (κ · τ(s))
c is ROOTED  ⟺  U_k(c) ≤ 1
```

True roots (`U = 0`): the plate (a `Landing::Plate` strut's cells at its bottom layer) and a
strut's centre column at every layer it occupies. **Model material is never a root by itself.**

Rootedness is **not re-seeded per layer** — that re-seeding is the defect this design exists to
kill (S2, §8). A 20 mm shallow ramp accumulates `U` across its whole run and becomes unrooted
long before its far end, even though each layer overhangs the one below by 0.01 mm.

### 5.2 The recurrence, `O(cells)` per layer

```
step 1  vertical inheritance, for c ∈ Cured(k):
          U[c] = 0                      if c is a strut centre cell
               = U_prev[c] + h·w(c)     if column(c) ∈ Cured(k−1)
               = +inf                   otherwise
step 2  in-plane chamfer, two sweeps (forward row-major, then reverse):
          U[c] = min(U[c], min over 8-neighbours n ∈ Cured(k) of U[n] + d(n,c)·w(c))
          d = p for edge neighbours, p·√2 for diagonals
step 3  R = (U ≤ 1)
with    w(c) = 1 / (κ · τ_k(c))
```

`U` is monotone in the sweep direction (material only ever attaches at or below itself), so two
chamfer sweeps give the exact 8-neighbour geodesic. A third pass runs as a convergence check:
if it changes anything, that is a `chamfer_not_converged` **warning**, never a refusal.

**Why a potential and not "geodesic distance ≤ κ·τ".** The naive form evaluates `τ` at the
endpoint, so a thin tongue growing off a thick block inherits the block's allowance. The
potential integrates `1/(κτ)` along the path, so the **thinnest** section along the path binds
— which is the physics (`docs/physics.md:89-92`: "Sizing one radius at the contact is not
enough. The rule applies at the minimum section, wherever along the column that is"). The two
forms agree exactly on a uniform lamina, which is where `κ` was measured, so the change of form
extrapolates nothing.

### 5.3 Cured thickness `τ`

`tau[c] = tau_prev[c] + 1` while `c ∈ Cured(k)`, else 0; `τ = tau·h`. The vertical run
over-states the true section by `1/cos θ` on a wall inclined `θ` from vertical, which
over-states reach — **the unsafe direction**.

**Decision (closing the algorithm spec's OQ-2): v1 takes `τ = min(vertical run, 2 × in-plane
distance transform of `Mcarry`)`.** The in-plane distance transform is the same two-sweep
chamfer machinery already written for `U`, so this is two extra linear passes out of ≈11 —
about 20 % of the layer cost — and it removes the only unsafe-direction approximation in the
design. `tau_vertical / tau_used` is reported per layer; anything above 2 is warned.

### 5.4 The `κ` magnitude disagreement, disclosed not reconciled

A cantilever of thickness `τ` under uniform pressure `P` fails at
`L_crit = τ·√(σ_f/(3P))`, exactly linear in `τ` — the shape `LAMINA_REACH_RATIO` claims. The
**magnitude** does not agree with the registry: with `TENSILE_GREEN_MPA = 0.288 MPa` and
`PEEL_PRESSURE_PA = 3000 Pa` (`placeholders.rs:1538`, `:1354`), `√(288/9) = 5.657`, so the beam
model predicts `κ ≈ 5.7` where the plates measured 42.6 — a factor of 7.5. Both inputs are
registered Placeholders ("Legacy diagnostic split, potentially about four times low"), and the
literature says the pressure is low, which widens the gap rather than closing it.

**The measurement governs; the beam model supplies only the linear-in-`τ` shape.** The
disagreement is a standing question against any future green-strength coupon and must appear in
the run report's provenance block. It is not reconciled here and no constant is invented to
close it.

### 5.5 Constants to register

New, in `crates/plinth-core/src/constants/`. Every Derived value **must be computed from its
parents at run time and must not be hard-coded**.

| id | file | class | value / rule | parents | provenance note to register (abridged; write it in full) |
|---|---|---|---|---|---|
| `LAMINA_REACH_RATIO` | `evidence.rs` | Derived | `L_SURVIVED_MAX_MM / LAYER_HEIGHT_DEFAULT_MM = 2.1300322500 / 0.050 = 42.6006450` | `L_SURVIVED_MAX_MM` (Measured, plates 1 and 4 pooled, `docs/specs/audit-span-criterion.md:172-183`); `LAYER_HEIGHT_DEFAULT_MM` (Measured, `evidence.rs:590`) | "Reach of one cured lamina per unit of its own thickness — the largest span at which NO loss was recorded on either plate, expressed as a ratio so it can be applied at other cured thicknesses. It is a COVERING RADIUS (no point of a down-face further than this from a carrier), not a bridge span: the coupon fingers were single-root cantilevers (`audit-span-criterion.md:44-47`), and a point between two seats is stiffer than a free cantilever end, so reading it as a covering radius is the conservative reading. The linear-in-thickness form is the cantilever scaling `L_crit = τ·√(σ_f/3P)`; the MAGNITUDE disagrees with the registered `TENSILE_GREEN_MPA`/`PEEL_PRESSURE_PA` pair by 7.5×, both of which are Placeholders, and that disagreement is disclosed rather than reconciled. SUPERSEDED BY any coupon at a second layer thickness or a second overhang angle. This is a DESIGN-TIME placement input and is deliberately NOT `L_SPAN_ALLOWED_MM`, which is the verification-time gate and carries a 1.53× condition shift and a 2·d2 instrument subtraction that are right for refusing a finished file and wrong for placing a seat." |
| `LAMINA_REACH_RATIO_FIRST_LOSS` | `evidence.rs` | Derived | `2.592036 / 0.050 = 51.84072` | same | "The rung at which the FIRST loss was recorded (plate 4, G1aU, 1 of 5, `audit-span-criterion.md:177`). REPORTING ONLY — no decision may read it. It exists so the run report can state where the placement value sits inside the measured bracket." |
| `LAMINA_REACH_RATIO_AUDIT_GATE` | `evidence.rs` | Derived | `L_SPAN_ALLOWED_MM / LAYER_HEIGHT_DEFAULT_MM = 1.3449762463 / 0.050 = 26.8995249` | `L_SPAN_ALLOWED_MM` (`audit-span-criterion.md:219-223`) | "What the AUDIT allows when it gates an emitted file. Registered so the engine can state, in the run report, whether the field it placed will pass its own audit. Placement and gating are different jobs; see the placement-vs-gate ruling request. REPORTING ONLY in the sweep." |
| `SWEEP_WORKING_PITCH_MM` | `placeholders.rs` | Placeholder | `TIP_RADIUS_DEFAULT_MM = 0.050` | — | "Working raster pitch. Derived in form from a Placeholder parent, so registered as a Placeholder. Three cross-checks: two cells across a minimum tip diameter; `p/2 = 0.025 mm < d3 = 0.05529 mm`, the audit's own fusion-undecidable band, so quantisation cannot move a landing across a fusion decision by more than the band inside which fusion is undecidable anyway; and `p = LAYER_HEIGHT_DEFAULT_MM`, so the working voxel is isotropic. Tip SIZING is analytic and never quantised by this. Pinned by a pitch-halving invariance test." |
| `SWEEP_CELL_LAYER_BUDGET` | `placeholders.rs` | Placeholder | 2.0e9 cell-layers minimum | — | "IT COARSENS; IT NEVER REFUSES. On exceeding it the working pitch doubles and the achieved pitch is reported. This is RC3's lesson registered as behaviour: a flat cap that failed Preparation converted a resolution question into a lost artifact." |
| `SWEEP_STRUT_REMOVAL_PASSES` | `placeholders.rs` | Placeholder | 2 | — | "Bounds the under-loaded-strut redistribution search. PRUNES ONLY: a strut whose demand cells would otherwise go uncovered is KEPT, so this constant can never create a bare island and can never decide an outcome." |
| `SWEEP_PEEL_FIELD_MODE` | `placeholders.rs` | Placeholder | `Uniform` default, `AreaLaw`/`AreaLawTilt` plumbed | — | "`Uniform` means α ≡ 1 and the sweep reduces exactly to the registered area law. The other modes are computed and reported but must not be the default until a coupon or an owner ruling says otherwise: turning the area term on contradicts `docs/physics.md:69-75` (measured X boundary collapsed across a 40× cap-area span) and turning the tilt term on contradicts `docs/physics.md:150-158` (Y-sensitivity settled; randomize, don't tune)." |

Registry tests to update: `crates/plinth-core/tests/constants_evidence.rs` — append the three
ids to `EXPECTED_EVIDENCE_IDS` and `EXPECTED_DESCRIPTOR_CONTRACT` in declaration order,
`descriptors.len()` 33 → 36 (`:344`), `(measured, derived)` (22, 11) → (22, 14) (`:359`);
`crates/plinth-core/tests/constants_placeholders.rs` — counts 273 → 277 (`:588`) and 306 → 310
(`:687`).

Read, not restated: `X_ALLOWED = 12` (`evidence.rs:131,359`), `X_STAR_CONE = [25.00, 25.34]`,
`TIP_RADIUS_DEFAULT_MM = 0.050 mm **radius**` (`placeholders.rs:1542`),
`TIP_RADIUS_MAX_LEGACY_MM = 1.000 mm radius` (`:1544`), the owner's 0.400 mm-radius growth
ceiling which **explicitly does not bound load-sized tips** (`evidence.rs:352`),
`DEFAULT_CONTACT_DEPTH_MM = 0.100 mm`, `LIFT_MM` (`placeholders.rs:1232`). *UNITS TRAP: the
registry speaks RADIUS; ChituBox and the owner speak DIAMETER. State both.*
`target_spacing_mm = 4.0` (`placeholders.rs:1535`) is **not read** by the sweep — spacing is an
output of `κ` here — but is reported side by side so the change is visible.

---

## 6. The two sweeps

### 6.1 Up-sweep: demand

`Dm(k) = { c ∈ Mcover[k] : U_k(c) > 1 }`. Islands fall out automatically (`U = +inf`), which
subsumes the old core's specialized island-birth and local-minimum seats — S6's
triangulation-invariance test proves the subsumption rather than asserting it.

A cell may **host** a strut only if its column is clear downward to the landing; a shadowed
cell stays in the demand set (it may still be *covered* from a legal neighbour within reach).
Cells no legal candidate can cover are reported as `residual_demand`, a warning.

### 6.2 Up-sweep: greedy insertion

```
S[c] = |{ d ∈ Dm(k) : d within the reach disc of a new root at c }|      (integer cell counts)
while Dm(k) ≠ ∅:
    c* = argmax S, ties by (row, col) ascending;  break if S[c*] == 0
    insert strut at c*, radius r_max, seat layer k;  descend (§6.3)
    recompute U locally with c* as a new root within 2ρ̄, drop newly rooted cells from Dm(k)
    rebuild S only within 2ρ̄ of c*
```

On a fresh flat lamina the reach ball is a Euclidean disc of radius `ρ̄ = κ·h = 2.1300323 mm`,
and the score is a disc convolution computed once per layer by running-column update, then
patched locally. Scores are exact integers, so ties are exact and the tie-break is total;
determinism does not depend on floating point.

Among cells within one score of the maximum, prefer one whose `top_strut` column is non-zero
(extend an existing strut), then one whose `top_model` is highest (shortest descent). This is
the only consolidation mechanism in v1 — descent is straight down, never tilted, because a
tilted strut's cross-section is an ellipse whose rasterisation is a second geometry problem.

Every strut is inserted at `r_max`. The model is deliberately over-supported at the top of the
up-sweep; the down-sweep makes it right.

### 6.3 Descent and landing

Straight down. Candidate stops in increasing drop: an existing strut top (`top_strut`), a cured
model cell (`top_model`), then the plate. Cost
`w_len·(z_seat − z_land) + w_scar·δ(landing)·π r_max² − w_int·[interior]`.

* `w_len = 1.0` per mm — a declared **policy** weight that orders candidates and decides no
  physical quantity.
* **`w_scar = 0` in v1.** Delicacy does not choose the landing; instead every landing's
  delicacy is looked up in one batched nearest-face query after the up-sweep and **reported**,
  so the owner sees where the scars went. The field already exists and is already passed into
  the support stage (`crates/plinth-pipeline/src/prepare.rs:1839-1849`). Turning `w_scar` on is
  a v1.1 item.
* Interior bonus: a landing inside a cavity leaves a scar nobody will see (owner ruling
  2026-08-31, "hidden cavity supports never need removing"). v1's test is `Landing::Model` **and**
  cured model material somewhere above the seat in the same column. Exact for a cavity
  ceiling→floor strut; a re-entrant exterior pocket also tests true, which only grants an
  undeserved bonus. The report states the test's form so the reader is not misled.

A `Model` landing seats at the **strike** face and is truncated there — it never exits the far
side (land-at-strike ruling, `docs/physics.md:284-324`), embedded `DEFAULT_CONTACT_DEPTH_MM =
0.100 mm`. A landing is legal only if it is itself rooted: `Plate` always, `Strut` by
induction, `Model` iff that cell had `U ≤ 1` at its own layer (already known, the sweep has
passed it). Otherwise the descent continues. This is what makes "rooted carrier" closed rather
than circular.

Each insertion removes at least its own cell from `Dm(k)`, so the loop terminates.

### 6.4 Down-sweep: the layer's own event, spreading, and the maximum

Load units are **mm² of reference peel**, written `ℓ`; with `α ≡ 1` this is literally mm² of
cured area and the sizing law is exactly the registered one.

```
e_k(c) = a · α_k(c)                     a = p²;  α ≡ 1 in v1 (§C7)
for each s ∈ Cured(k+1) carrying L_in[s]:
    if column(s) ∈ Cured(k):   give all of L_in[s] to that cell            (O(1), the common case)
    else: walk carriers of Cured(k) ∩ disc(column(s), κ·τ_k) by increasing distance,
          accumulating until carrier area ≥ a, then split by area (weights sum to exactly 1)
    if the disc holds no carrier: warn `uncarried_load`, drop it to the plate as a diagnostic
                                   quantity so the totals still reconcile
L_k(c) = max( spread_in(c), e_k(c) )
```

**Nearest-first, area share** is the conservative rule for struts: a strut cross-section
directly under a load cell takes all of it rather than sharing with distant model.

**Conservation is checked, not assumed**: after every layer,
`|Σ L_out + Σ injected_to_struts + Σ uncarried − Σ L_in − Σ e_k| ≤ 1e-9 · Σ L_in`, a hard
assertion, because a conservation break is a defect and not a model outcome.

**Max, not sum.** Each layer's peel is a separate event; a print never peels two layers at
once. Collapsing the event history into one max-field *before* spreading is proved
conservative, not merely cheap: allocation is monotone and positively homogeneous, so
`spread(max(a,b)) ≥ max(spread(a), spread(b))` cellwise. The true design load is the right-hand
side; the sweep computes the left; it therefore never under-sizes, and `O(K²)` event
bookkeeping becomes `O(K)`.

### 6.5 Struts are columns; landing injection; sizing

A strut does not spread. `carried_s(k) = carried_s(k+1) + share_s(k) + Σ(struts landing on s at k)`;
its footprint cells are removed from `L_out` (accounted in the conservation check). At
`k_bottom`: `Plate` is the sink and the reaction is recorded; `Model` **injects** `carried_s`
into `L_k` at the footprint cells *before* layer `k` spreads onward; `Strut` adds.

```
A_req(s,k) = carried_s(k) / X_ALLOWED
r(s,k)     = clamp( sqrt(A_req/π), TIP_RADIUS_DEFAULT_MM, TIP_RADIUS_MAX_LEGACY_MM )
```

`carried_s` is non-decreasing downward, so `r` is too: **the arm flares toward its root and its
narrowest section is at the tip**, which is where the law is evaluated and exactly what
`docs/physics.md:89-92` asks for. The law holds at every layer, not only at the contact.

Clamp behaviour, per emit-and-warn: below the minimum → a removal candidate (§6.6); above the
maximum → emit at `r_max` and warn with the realized `X_real = carried/(π r_max²)`, naming
seat, layer, load and bound (`X·π·r_max² = 12π = 37.6991 mm²` is the largest area one strut may
legally carry — the same quantity `crates/plinth-support/src/perception.rs:610-612` computes
today); above the owner's 0.400 mm-radius growth ceiling → **reported, not clamped**, because
`evidence.rs:352` states in terms that the ceiling bounds growth-for-attachment only.

**The model's own material is never sized.** Where `L_k` sits on model material the sweep
computes per connected component `X_model = Σ L_k / component area`, and warns
`model_section_overload{layer, component, X_real}` above `X_ALLOWED`. The engine supports
models; it does not redesign them. This is the T oracle's quantity.

### 6.6 Removal and redistribution

`ℓ_min = X_ALLOWED · π · TIP_RADIUS_DEFAULT_MM² = 12π·0.0025 = 0.0942478 mm²` — the same
threshold `placeholders.rs:1542` states in prose. For at most `SWEEP_STRUT_REMOVAL_PASSES = 2`
passes: mark struts with `carried_s(k_top) < ℓ_min`; for each, in id order, re-run the coverage
test for the demand cells it covered against the survivors; delete it only if **every** such
cell is still covered, then re-run allocation locally so neighbours' radii grow. Otherwise
**keep** it — a hairline strut is better than a bare island, and that is what makes this
constant unable to decide an outcome. Struts surviving at `r_min` are reported with their
loads, so RC4's hairline-tip symptom is visible if it ever recurs.

---

## 7. Oracles — hand-derived expectations, arithmetic corrected

All values derived before the code exists, from the registered physics. Shared quantities at
`κ = 42.6006450`, `h = 0.050 mm`:

```
ρ₁ = κ·h                  = 2.1300323 mm      one-lamina reach (covering radius)
s  = √3·ρ₁                = 3.6892 mm         hex nearest-neighbour spacing
C  = (3√3/2)·ρ₁²          = 11.7876 mm²       hex Voronoi cell area          (docs/physics.md:128-135)
ℓ_min = 12π·0.05²         = 0.0942478 mm²     smallest printable strut load
ℓ_max = 12π·1.00²         = 37.6991 mm²       largest load one strut may carry
```

Hex tiling: `N = ceil(A_demand / C)`; per-strut load `ℓ̄ = A_peak / N`; tip radius
`r = sqrt(ℓ̄/(12π))`; **scar invariant `Σ π r² = A_peak / X_ALLOWED`, independent of `N`**
(`docs/physics.md:100-118`).

### 7.1 O1 — the discs (and the Cylinder cross-check)

Demand is the **bottom layer** (§C1); conservation is the **peak layer** (§C2).

| | Round 25 | Round 50 | Round 100 | Cylinder |
|---|--:|--:|--:|--:|
| pose | flat, recess UP, 0°, lift 5 mm | same | same | upright, flat cap down |
| print height / layers | 8.5 mm / 170 | 8.5 mm / 170 | 8.5 mm / 170 | 35 mm / 700 |
| bottom-layer demand `A_dem` | 415.4 mm² ⚠ | 1809.5 mm² | 7538.4 mm² | 706.86 mm² |
| **struts `N = ceil(A_dem/C)`** | **36** | **154** | **640** | **60** |
| peak-layer area `A_peak` | ≈474.2 mm² ⚠ | 1852.83 mm² | 7630.94 mm² | 706.86 mm² |
| per-strut load `ℓ̄` | 13.172 mm² | 12.031 mm² | 11.923 mm² | 11.781 mm² |
| tip radius / **diameter** | 0.5911 / **1.182 mm** | 0.5649 / **1.130 mm** | 0.5624 / **1.125 mm** | 0.5590 / **1.118 mm** |
| `Σ π r² = A_peak/12` | 39.52 mm² | 154.40 mm² | 635.91 mm² | 58.90 mm² |
| every landing | Plate | Plate | Plate | Plate |
| every strut length | exactly the lift | " | " | " |
| rim / recess / top struts | 0 / 0 / 0 | 0 / 0 / 0 | 0 / 0 / 0 | 0 (rim vertical) |
| bare islands, `residual_demand` | 0, 0 | 0, 0 | 0, 0 | 0, 0 |
| mean NN spacing | ≈3.7 mm, max ≤ 2ρ₁ = 4.26 mm | " | " | " |

⚠ **Round 25 is flagged, not trusted.** The probe records the bottom face as 415.4 mm² at
`r ≤ 12.0`, but `π·12² = 452.4 mm²` — 8.2 % higher, far more than faceting explains (the
Round 100 fan's 102-gon inscription accounts for only 0.05 %). Round 25 likely carries a
central feature on its underside. Its `A_peak` is inferred from the bbox and the Round 50/100
draft, not measured. **The oracle harness must compute both areas from the slice stack and
grade against them**; the Round 25 row above carries ±10 %.

Cross-check against field practice: the owner's 47-tip Cylinder print
(`disc-diagnosis-2026-09-02/challenge-r2/FINDINGS.md:61,82`) is 78 % of the predicted 60 —
inside the acceptance band, and the shortfall is expected because his practice mixes tip sizes
and takes the measured load-sharing credit that the engine's per-arm `X = 12` deliberately does
not (`docs/physics.md:140-150`).

This oracle exercises: the reach test, greedy covering, plate landings, conservation, the scar
invariant, and tip sizing inside both clamps (no clamp warning may fire on a flat disc — one
that does is a red flag against this oracle).

### 7.2 O2 — the T: `make_overhang_table` (existing corpus fixture)

`crates/plinth-mesh/src/fixtures.rs:219-249`: four `4×4×20 mm` legs at `(±10, ±10)`, a
`30×30×4 mm` slab on top, legs inset 3 mm. Its own doc comment (`:251-253`) already names it the
reach-spreading witness. Pose legs-down, lift 5 mm; 29 mm / 580 layers.

| quantity | value |
|---|--:|
| slab underside (the governing peel event) | 900 mm² |
| self-rooted fringe per leg, `w² + 4wρ₁ + πρ₁²` | 64.334 mm² (§C3) |
| self-rooted total | 257.34 mm² |
| slab demand needing struts | 642.66 mm² |
| **slab struts `ceil(642.66/C)`** | **55**, pitch 3.69 mm, ⌀1.113 mm, 20 mm tall (400 layers), landing Plate |
| leg-underside struts (16 mm² each) | hex minimum 2/leg = **8**; greedy 8–16, each carrying `64.334/n` |
| **total struts** | **63–71** |
| leg model-section check | `X_model = 64.334/16 = 4.02` ≤ 12 → **no warning** |
| conservation | `Σ carried = 257.34 + 642.66 = 900.00 mm²`, exact |
| `Σ π r²` | `900/12 = 75.00 mm²`, independent of how the leg struts split |

Fringe discs do not overlap (legs 20 mm apart, fringe half-width 4.13 mm) and stay inside the
slab (reach to 14.13 mm < 15 mm), so the arithmetic is a clean sum.

**The overload warning does not fire on the stock fixture** — the oracle spec implied it would.
To exercise §6.5's model-section warning a variant is needed:
`make_overhang_table_thin(w = 1.5 mm)` gives `X_model = (2.25 + 12.780 + 14.254)/2.25 = 13.02 >
12` → one `model_section_overload` warning, **file still emitted**. That variant is part of
LANE 1.

This oracle exercises: reach spreading (load walking inward along the bar), conservation through
a bottleneck, the `τ`-dependence of reach, and the model-section warning.

### 7.3 O3 — the cavity ceiling (new fixture `make_cavity_box`)

40 mm cube, 3 mm walls, 34 mm cubic cavity (`x,y,z ∈ [3,37]`). Ceiling underside at `z = 37`,
cavity floor at `z = 3`. Per `docs/physics.md:325-333` the ceiling must be supported from the
cavity's lower interior, never by punching the exterior wall.

| quantity | value |
|---|--:|
| ceiling underside | 1156 mm² |
| wall-rooted fringe (erosion of a square by a disc is a square) | `1156 − (34 − 2ρ₁)² = 1156 − 884.46 = 271.54 mm²` |
| interior demand | 884.46 mm² |
| **cavity struts `ceil(884.46/C)`** | **76**, ⌀1.111 mm, 34 mm long, landing `Model{ interior: true }` on the cavity floor |
| box underside demand | 1600 mm² |
| **plate struts** | **136**, ⌀1.118 mm |
| ceiling scar (hidden) | `884.46/12 = 73.70 mm²` |
| **plate reaction** | **1600 mm², NOT 2756 mm²** |
| naive floor-only baseline (no wall rooting) | `ceil(1156/C) = 99` cavity struts — a 30 % difference, which is why this oracle exists |

**The plate total is a max, not a sum**, and the oracle spec had it as a sum. The bottom wall's
own peel event is 1600 mm²; the ceiling event delivers 1156 mm² down the same path at a lower
per-cell density (0.765 inside the cavity footprint, 0.611 under the wall ring, against 1.000
for the bottom layer's own event), so the maximum is the bottom layer's own event everywhere.
Predicted `Σ_plate = 1600 mm² ± tiling slop`. A sum bug yields 2756 mm², +72 % — this is
sabotage S7's sharpest lesion.

**Recorded, not resolved:** each cavity strut is 34 mm at 0.556 mm radius, slenderness ≈ 61:1.
The area law governs peel failure at the narrowest section and says nothing about column
buckling, for which no constant is registered. The run reports a slenderness histogram; no gate,
and **no buckling constant is invented** to close it. A coupon is the correct fix.

This oracle exercises: interior landings, landing injection, demand *inside* the model,
max-over-events at the plate, and the hidden-cavity ruling.

### 7.4 Acceptance bands

These are **test tolerances**, not physical constants, and are documented as such in the test
files:

* strut count `N ∈ [0.9·N_hex, 1.5·N_hex]` — hex is the optimal covering, so a greedy result
  above it is expected; the 0.9 allows boundary effects on the small discs.
* area identities (`Σ carried`, `Σ π r²`) within **±5 %** of the harness-computed peak layer
  area — tiling slop at a circular boundary.
* conservation residual per layer within **1e-9 relative** — an assertion, not a tolerance.
* mean nearest-neighbour seat spacing in **[3.0, 5.0] mm**. Note the gate's phrasing is
  "~4–5 mm"; `κ = 42.6` places at 3.69 mm, *denser* than the owner's practice, so the band is
  widened downward deliberately and LANE 2 must not fail on that technicality.

### 7.5 The treant is deliberately not hand-derived

429,120 faces, organic and asymmetric. A hand-derived strut count would be an invented number.
Its LANE 2 row records what the sweep produces and compares it against the legacy engine's
resin, plate area and strut count; that measurement — not a prediction — decides whether
branching is needed (§14, D6). For scale only, not a target: its bbox footprint bounds the
plate field at a few hundred struts.

---

## 8. Sabotage tests

A suite that cannot be made to fail by breaking what it tests is not evidence. Each row is a
one-line mutation, the oracle it must break, and — required — **an asserted path-hit count**, so
a green result can never mean "the path never ran" (the witness-must-exercise-the-path lesson).

| # | mutation | must go RED | path-hit assertion |
|---|---|---|---|
| S1 | disable the reach test (`rooted := c ∈ Cured(k)`) | O1: 0 struts placed; O3 likewise | reach test evaluated ≥ 1 per model layer |
| S2 | re-seed rootedness per layer (`U := 0` over any cured cell) — the exact defect this design kills | O1: 0 struts; O2: 0 slab struts at any overhang | vertical-inheritance branch hit ≥ 1e5 |
| S3 | drop the allocation renormalisation (weights no longer sum to 1) | conservation assertion fires; O1 `Σℓ ≠ A_peak`; O2 neck load wrong | multi-carrier split path hit ≥ 1 |
| S4 | let struts spread (delete the column rule) | O1: per-strut load collapses below `ℓ_min`, removal empties the field | strut-share read hit ≥ N·K |
| S5 | delete the landing injection | O3: plate struts under-sized; `Σ_plate` loses the routed ceiling load | injection path hit exactly 76 on O3 |
| S6 | charge triangles instead of cells (re-introduce RC4) | **triangulation invariance**: the same disc as a 102-triangle fan and an 800-triangle uniform mesh must agree on strut count and per-strut load to within one raster cell | rasteriser entered once per layer per mesh |
| S7 | replace `max` over events with `sum` | O1: total load `K·A` instead of `A_peak`, every tip clamps at `r_max`; O3: `Σ_plate` 1600 → 2756 | max-combine hit ≥ 1 per layer |
| S8 | use `κ_audit_gate` where `κ` belongs, or the reverse | a **reporting** test: `will_pass_audit_span_check` must flip. Guards OR-2 against being decided by accident | both constants read exactly once per run |
| S9 | set the cell-layer budget below the model's need | must **coarsen and warn and still emit**; a refusal here is the RC3 regression and fails the test | coarsening loop hit ≥ 1 |
| S10 | zero the lift | must be rejected on entry (`plate_z < min_z` assertion) — a zero-lift pose is never a candidate | assertion hit exactly once |

Plus one property test, not a sabotage:

* **P1 pitch-halving invariance.** Run O1 at `p` and at `p/2`; strut count must agree within
  the acceptance band and `Σ π r²` within 1 %. This is what keeps `SWEEP_WORKING_PITCH_MM`
  from silently deciding an outcome.

---

## 9. The emit adapter

### 9.1 What the boundary consumes (verified at `02b1d78`)

```
plinth_emit::write(&SupportedModel, &EmitRequest) -> EmitReport   crates/plinth-emit/src/lib.rs:98
SupportedModel::from_parts(Mesh, Vec<SupportPrimitive>, SupportHistory)
                                                                  crates/plinth-support/src/lib.rs:529
SupportPrimitive { segment_kind, start_mm, end_mm, start_radius_mm, end_radius_mm,
                   contact_depth_mm, ring_normal, sphere_geometry }                    :445
SupportSegmentKind = ContactSphere | ContactTaper | JointSphere | Member | RootFlare
                   | RaftPad | RaftBridge                                              :208
SupportHistory::from_reports(HollowHistory, SupportReport, BuildFrame)                 :480
```

`from_parts` is a checked constructor: it audits the mesh, cross-checks primitive counts
against `SupportReport`, and validates every primitive via `is_semantically_valid` (`:591`) —
non-degenerate frusta, a raft pad is a point, `contact_depth_mm` present **iff** the kind is
`ContactTaper`, `FrustumRingNormal::PlateZ` only on `Member`/`ContactTaper`, and a sphere's
serialized poles exactly `SphereGeometry::canonical_poles(radius)`. That is the whole contract.

### 9.2 The primitive chain

One sweep strut lowers to the same chain the existing lowering emits for a plate-rooted stub:

```
1. ContactSphere   tip ball at the seat, poles from SphereGeometry::tip_with_exact_record_poles
2. ContactTaper    ball centre down to the first radius break, contact_depth_mm = Some(0.100)
3. Member × m      one frustum per radius-profile segment, top-down; the profile is run-length
                   merged so a segment is emitted only when the radius changes by more than
                   1 % or 0.005 mm, whichever is larger — a RECORD-COUNT economy, not a
                   physical rule; the unmerged profile stays in the ledger
4. RootFlare       only for Landing::Plate, root boss top down to plate_z
```

**As built (2026-09-28, field report "trunk-thinner").** Step 3 was not built until
2026-09-28: every strut was lowered as ONE taper from its tip radius to its root radius, which
drew every trunk under a mid-shaft junction thinner than its own ledger and thinner than the
branch landing on it. It is now built on a physical rule rather than the record-count economy
above: the drawn section at every height is at least the ledger's, `r_drawn(z) ≥ radius_mm(z)`
(the area law on the emitted geometry). `plinth_support::lower` (`drawn_spans`) cuts the shaft at
slot boundaries and splits a straight chord only where it would dip below the ledger (at the
cut where it dips furthest, recursively); a piecewise-constant ledger therefore breaks exactly at
each step and nowhere else, with an upward-facing shoulder at each junction. The 1 % / 0.005 mm
merge is NOT used — no tolerance beyond float noise (1e-9 relative) enters. The junction clause
(a trunk is at least as thick as each branch where it joins) holds by the ledger for support
arms; for a BRACE, sized by its own slenderness minimum and invisible to the trunk's ledger, the
adapter (`junction_profiles`) raises the host's DRAWN profile to the brace's end radius from the
joint down to the host's root (never a local collar: a member must not narrow downward, or it
leaves a down-facing ledge), which also widens that host's plate flare. The run report's
resin line now leads with the volume measured from the emitted records and keeps the ledger
volume beside it, labelled as the sizing figure.

**As built (2026-09-29, fix/contact-sizing; `lanes/wall-creep-diagnosis.md`).** A tip's contact
is sized from the load that crosses its own contact with the model, and nothing else. The top
ledger slot of a support arm is its contact (`tip_radius_mm`, the ball's radius, the scar
identity's term). A child that lands on the arm's HEAD (at its `k_top`, or at its top alive layer)
used to have its load booked into that slot, which over-sized the parent's ball (Round 25: arm
39 drawn at 0.746 mm where its own peel asks for 0.400 mm) and summed the child's load into two
scars. The down-sweep now books that load one slot lower, into the shaft under the contact, which
carries the sum; `r_drawn(z) ≥ radius_mm(z)` still holds, and the junction clause at a head joint
is met by `junction_profiles`, which raises the host's drawn profile to the branch's root from the
joint down. An arm with no slot below its contact (one layer long, terminating at its own tip
layer) keeps the load in its one slot and is counted (`contact_merges_kept_at_single_slot`).

**As built (2026-09-29, fix/head-join; `lanes/contact-sizing.md` §5).** The ledger rule above left
the GEOMETRY of a leaned head: the sum-sized shaft starts one slot under the seat, its tilted ring
rises through the seat plane (Round 25 stiffener arms 39, 41, 44, about 53° from vertical, by 0.41
to 0.48 mm), and the trim leaves that section on the model face, so the file's contact is not own
sized. `head_join` (run after the slenderness merges and before the ties, to a fixed point with the
down-sweep) moves such a lander so that (1) the section carrying the sum keeps
`FINAL_CLEARANCE_GAP_MM` (0.3 mm) from the seat plane of the member it joins, measured on the drawn
shaft from the ball centre (`tip_contact_discs`) as `d − R·√(1 − (n·m)²)`, and (2) the lander's own
solid, re-landed there, keeps the two-tier clearance from the model (0.6 mm placement margin, else
the 0.3 mm hard limit) by the kernel's segment-clearance predicate, its own seat zone excepted.
Candidates are the head arm's shaft below the booked slot, then its landing chain, highest first.
Members resting on a moved lander follow it onto its new axis (no nearer the model than they were).
A join that cannot move is kept and warned (`HeadJoinOutcome`, prepare log "head joins"), and the
emitted field is surveyed again after the footprint rule. **Scope (second round, same day):** every
member landed on a support arm, at any layer, is graded, because a lander one or three layers under a
leaned head is booked into the same slot as one at the head (Round 50 at its committed flip, arms 66,
68, 83, 98, their sums across the plane by 0.33 to 0.48 mm and unwarned while the rule looked at head
landings only). The seat zone is where the plane is the model: a section is in it only when it is
within 0.3 mm of the seat plane AND of the model itself, its disc measured as a circumscribed 16-gon
(edges and spokes) by the kernel's segment-clearance predicate. Under a steep face the infinite plane
passes below the model (Round 25 arm 6), and such a join is left alone; where the model IS there the
join is the rule's whatever the lean (Round 50 arm 155, vertical: its lander's solid was 0.088 mm
inside the model). The same zone decides which candidate joins may take a lander.

**Corrected 2026-09-28 (fix/plate-gap).** This line first read "down to `plate_z + raft_height`",
the legacy raft datum. v1 emits no raft (§14 OR-3), so that left every root flare 0.2 mm (four
layers) above the plate with nothing under it; the owner saw the bases float in ChituBox. The
flare now reaches `plate_z` itself with its full `FLARE_R_FACTOR × r_root` foot; a plate arm too
short for a boss runs its straight column to `plate_z` too. The boss TOP is unchanged: it still
comes from the shared `root_boss_top_at`, which reads the raft height as a datum.

**As built (2026-09-28, fix/tip-trim; field report "tip-clip").** Steps 1 and 2 are TRIMMED to
the seat. The ball goes to the full depth the seat law asks for (owner ruling 2026-09-05: never
retract), and the tip ball and the first shaft segment then keep only
`primitive ∩ {support side of the seat face's plane} ∖ model solid`. The plane is the carried
face's own (the nearest DOWN-FACING mesh face to the tip, its closest point and outward normal,
`tip_landing::tip_seat_faces`), not the raster seat, which the layer grid puts up to half a layer
off the face. `plinth-emit` does the cut for the whole field in ONE batched Manifold call
(`plinth_solid::ManifoldBackend::trim_and_subtract_each`: each piece `TrimByPlane`, then minus the
model, imported once). This replaced the horizontal "far face" clip on the ball's top, which left
every ball buried below it, said nothing about a face's edge, and never touched the taper. The
record's `start`/`end`/radii are unchanged; only its soup is cut.

**Corrected (2026-09-29, review-fixes).** The trim is about the shaft's SOLID, not about how the
drawn profile cuts it into records: every further `Member` span of a seated shaft whose solid
reaches the model side of the seat plane carries the same trim as the taper above it (the minimum
of the plane distance over its two end rings, `lower.rs::frustum_reaches_model_side`). On a sloped
face a profile break one slot under the seat left such a span emitted whole. Published as
`shaft_members_trimmed_to_seat`; pinned by `plinth-sweep` `tests/it/shaft_seat_trim.rs`.

The **footprint rule** goes with it (`plinth_sweep::footprint`): the contact disc the trim leaves
(the ball's section in the seat plane, computed by the lowering's own derivation,
`plinth_support::tip_contact_discs`) must lie on the seat face, graded in the eroded carrier
rasters. A seat whose disc hangs off the face slides along the face to the nearest position whose
whole disc is on it (search bounded by the ball diameter plus the lean's offset of the disc plus
the carrier mask's two-cell margin, a pruning Placeholder); a member joined
to a moved axis (a branch landing, a brace top) moves with it (`footprint::followed_shifts`). A
true knife edge keeps its seat, trimmed, and is warned about with the fraction of its disc on the
face.

**Corrected (2026-09-29, review-fixes).** The rule runs after the lean law, the slenderness lane and
the drain keep-out, so a move is now held to those rules on the DRAWN ends of every member it
displaces (`footprint::MoveLimits`): lateral travel per unit drop within the lean law, the drawn
chord within its tightest unbraced run's limit, no new lamina in the drain keep-out; each "or no
worse than before" for a member already past it. A refused move sends its tip to its next
candidate; a tip with none left keeps its seat with a WARNING (`Footprint::MoveRefused`). The
published run lengths are re-read on the moved chords. Pinned by `plinth-sweep`
`tests/it/footprint_moves_hold_the_rules.rs` (flipped Round 25: without the check two members
lean at 1.575 and 1.586 against the law's 1.5697).

**The adapter must not re-derive this geometry by hand.** `lower_stub`
(`crates/plinth-support/src/lower.rs:362`) and `root_boss_top` (`:416`) already compute the
canonical ball centre, poles and flare; LANE 2 promotes the geometry helpers to `pub` and the
adapter calls them. A second implementation of `canonical_poles` is exactly the defect that
`is_semantically_valid` exists to catch.

A `Landing::Model` strut ends embedded 0.100 mm at the strike face, no flare. A
`Landing::Strut` strut ends at the parent's surface with **no `JointSphere`** in v1 — the two
frusta abut, which the emitter accepts and ChituBox renders, but the union is not certified
enclosed the way the joints stage certifies it. Disclosed in §13.

### 9.3 Seat normals

The tip ball needs an outward axis. **One batched nearest-face query over all seat points at
once**, taking the native face normal of the nearest face — the rule `ContactCandidate::normal`
already uses ("deliberately the native surface normal for `face_id`, never a vertex normal",
`crates/plinth-support/src/lib.rs:674-680`). One batch per model, never one call per seat.

---

## 10. LANE 1 — core crate and oracles (no pipeline wiring)

**Goal:** the sweep produces a strut plan for a mesh, the three oracles agree with §7, and every
sabotage test goes red. Nothing in the production pipeline changes.

### Files created

```
crates/plinth-sweep/Cargo.toml
crates/plinth-sweep/src/lib.rs          run_sweep(&Mesh, &LayerPlan, &delicacy, SweepPolicy)
                                        -> SweepPlan;  SweepError;  re-exports
crates/plinth-sweep/src/limits.rs       SweepLimits (raster cells, cell-layers, coarsening)
crates/plinth-sweep/src/grid.rs         working raster, origin/margin, budget + coarsening loop
crates/plinth-sweep/src/raster.rs       scanline Mcover/Mcarry from NormalizedLayer (§4.4)
crates/plinth-sweep/src/reach.rs        tau, the U recurrence, the two chamfer sweeps (§5.2-5.3)
crates/plinth-sweep/src/up_sweep.rs     demand, seat legality, greedy, descent, landings (§6.1-6.3)
crates/plinth-sweep/src/down_sweep.rs   spread, max-over-events, strut columns, injection,
                                        sizing, removal (§6.4-6.6)
crates/plinth-sweep/src/peel.rs         alpha field; Uniform default; beta_area/beta_tilt reported
crates/plinth-sweep/src/report.rs       SweepRunReport, every warning in §13
crates/plinth-sweep/tests/oracle_disc.rs
crates/plinth-sweep/tests/oracle_tee.rs
crates/plinth-sweep/tests/oracle_cavity.rs
crates/plinth-sweep/tests/sabotage.rs           S1-S10 with path-hit counters
crates/plinth-sweep/tests/pitch_invariance.rs   P1
```

### Files modified

```
crates/plinth-core/src/constants/evidence.rs        + 3 Derived (§5.5): fields, construction,
                                                      accessors, ConstantDescriptor rows
                                                      (pattern at :735-757; X_ALLOWED at :359)
crates/plinth-core/tests/constants_evidence.rs      EXPECTED_EVIDENCE_IDS + DESCRIPTOR_CONTRACT
                                                      append; len 33->36 (:344);
                                                      (measured,derived) (22,11)->(22,14) (:359)
crates/plinth-core/src/constants/placeholders.rs    + 4 Placeholders (§5.5), DSL at :966
crates/plinth-core/tests/constants_placeholders.rs  273->277 (:588), 306->310 (:687)
crates/plinth-mesh/src/fixtures.rs                  + make_cavity_box(), make_overhang_table_thin(w),
                                                      + make_disc_fan(d) / make_disc_uniform(d) for S6
crates/plinth-support/src/lib.rs                    + pub fn slice_stack(mesh, &LayerPlan, limits)
                                                      -> NormalizedLayers, wrapping
                                                      normalized_slice_layers (perception.rs:1506);
                                                      mod orientation (:1627) stays private
crates/plinth-support/src/perception.rs             normalized_slice_layers :1506 private -> pub(crate)
```

`Cargo.toml` needs **no** edit: `members = ["crates/*"]` (`Cargo.toml:2`). `tools/plinth-audit`
must not be touched, imported from, or promoted (`Cargo.toml:3-11`).

### Reuse (call, do not reimplement)

`NativeSurface::slice_z_batch` (`rust/geom_kernel/src/slicing.rs:112`) — one call, all planes;
the wiring to copy is `crates/plinth-support/src/perception.rs:1166-1180`.
`plinth_layers::normalize_layers_batch` (`crates/plinth-layers/src/lib.rs:689`) and its
`NormalizedLoop`/`NormalizedSolid`/`NormalizedLayer` (`:190-228`) — disjoint CCW outers, CW
holes, even-odd and non-zero agreeing. `LayerPlan` (`crates/plinth-core/src/layer.rs:21-25`).
`plate_z = min_z − lift` (`crates/plinth-support/src/runner.rs:2178-2186`). `StageClock`
(`runner.rs:1641,1668,2341`) for named stage lines, so the run is not silent.
`crates/plinth-support/src/dyadic.rs` `refine_depth` (the per-triangle refinement) **must not**
be reused — it is RC3.

### Acceptance checks

1. `cargo test -p plinth-sweep` green; the three oracle tests match §7 inside §7.4's bands,
   with **areas computed by the harness from the slice stack**, not hand-fed.
2. All ten sabotage tests red-when-mutated **and** each asserts a non-zero path-hit count.
   P1 green.
3. Round 25's measured bottom-layer and peak-layer areas are **recorded** in the test output;
   if they contradict §7.1's flagged row, the oracle row is corrected and the correction is
   reported (not the code bent to match).
4. `cargo test -p plinth-core` green with the updated registry counts; every new constant
   computed from parents at run time (a test asserts `LAMINA_REACH_RATIO ==
   L_SURVIVED_MAX_MM / LAYER_HEIGHT_DEFAULT_MM` rather than a literal).
5. `cargo clippy --workspace --all-targets` clean. No `.py` anywhere.
6. `git diff --stat` touches **no** file under `crates/plinth-pipeline/` or `crates/plinth-cli/`
   — that is LANE 2's write-set.
7. Binary provenance: after the final build, a second `cargo build --release` prints **zero**
   `Compiling` lines; record `sha256` of `target/release/plinth`.
8. One heavy process at a time; check `uptime` and wait if the 1-minute load average is above 8.

---

## 11. LANE 2 — pipeline wiring behind a selector, and measurement

**Goal:** `plinth prepare --placement sweep` produces `.chitubox` files for six models through
the real writer, and `--placement legacy` is unchanged.

### Files created

```
crates/plinth-sweep/src/adapter.rs      SweepPlan -> Vec<SupportPrimitive> -> SupportedModel (§9)
corpus/models/Round_25.stl
corpus/models/Round_50.stl
corpus/models/Round_100.stl
```

The three disc STLs are **not** in the repo today (`corpus/models/` holds 8 models; the discs
exist only as run outputs under `plinth-evidence/`). They are committed here because a ratchet
row and an oracle that cannot be reproduced from a clean checkout is not finished work.

### Files modified

```
crates/plinth-cli/src/lib.rs             + enum PlacementSelection { Legacy, Sweep } near :76
                                         + field on PrepareSelection (:93-108)
                                         + parse arm in parse_prepare (:217), next to --orient (:287),
                                           with DuplicateFlag handling (:290) and an unknown-mode error (:127)
                                         + PREPARE_USAGE (:29) gains
                                           "[--placement legacy|sweep]" and its option line
                                         + tests mirroring :401 / :425-440 / :459-462
crates/plinth-cli/src/execute.rs         resolve() (:60-90) maps it onto PrepareRequest;
                                         report print site (:293-584) states the placement used
crates/plinth-pipeline/src/lib.rs        + pub enum Placement { Legacy, Sweep }
                                         + pub placement: Placement on PrepareRequest (:384-395)
crates/plinth-pipeline/src/prepare.rs    the fork at :1880:
                                           Legacy => run_support(hollowed, policy.support.clone(), input)
                                           Sweep  => plinth_sweep::run_sweep(..).and_then(adapter)
                                         truncate the LayerPlan (:1831-1833) at the model top for
                                         the sweep; pass the existing delicacy vector (:1839-1849)
crates/plinth-support/src/lower.rs       lower_stub (:362) / root_boss_top (:416) geometry helpers
                                         promoted to pub so the adapter cannot re-derive them
crates/plinth-support/tests/ratchet_harness.rs  CORPUS_MODELS [&str; 8] -> [&str; 11] (:174-183)
crates/plinth-support/tests/fixtures/support_ratchets.json  regenerated via PLINTH_RATCHET_UPDATE
                                         (:130) — never hand-edited
corpus/manifest.json                     3 new model entries in the existing schema (:19-46)
```

Both engines return `SupportedModel`, so `judge_then_emit`, the pre-write gates, `StageStop`
(`StageStop::Support` already stops right after either engine returns) and the run report need
no change. The selector is a **runtime enum on `PrepareRequest`, not a Cargo feature**, so both
engines run through one binary for side-by-side measurement.

### Measurement protocol

Six models, one at a time (one heavy process on the machine), invocation shape from
`plinth-evidence/overhang-weight-part1-2026-09-02/measure/run-measure.sh:1-15`:

```
plinth prepare --input-models-dir <dir-with-one-model> --output saturn4-ultra \
               --placement sweep --out-dir <dir>
```

Models: `Round_25`, `Round_50`, `Round_100`, `Cylinder`, `make_overhang_table`,
`Weeping Treant v1.2`. Each also run with `--placement legacy` for the comparison. All outputs,
logs and reports to
`/Users/craig/code/hughescr/dnd-campaigns/plinth-evidence/slice-sweep-2026-09-02/measure/`.

### Acceptance checks

1. **All six emit. Zero refusals.** A model that produces no file is a lane failure, not a
   finding about the model.
2. Discs: recess **UP**; `residual_demand_cells = 0`; mean nearest-neighbour seat spacing in
   [3.0, 5.0] mm; wall clock in **seconds** for Round 25/50/100 (§4.2's budget table predicts
   ≤ 20 s at 0.05 mm even at 3 ns/cell single-core).
3. `Σ π r²` within ±5 % of `peak_layer_area / 12` for every model; `conservation_residual` under
   1e-9 relative on every layer of every model.
4. Strut counts inside §7.4's band for Round 25/50/100, Cylinder and `make_overhang_table`.
5. **`--placement legacy` is byte-identical to today** on the existing 8 corpus models —
   compare emitted-file digests against a pre-change run. A behaviour change on the legacy path
   is a lane failure.
6. Ratchet rows recorded for Round 25/50/100 under both engines via `PLINTH_RATCHET_UPDATE`;
   the JSON is regenerated, never edited.
7. Every file opens in **ChituBox** — the owner's judgement, and the actual gate. Report what he
   says verbatim, including "the discs need a raft" if that is the answer (OR-3 in §14).
8. Treant row records struts, resin, plate contact area and wall clock for both engines. The
   legacy engine has never completed this stage (`placeholders.rs:1492` records EXCEEDS-TIMEBOX
   at 600 s); if it still does not, that is recorded as **unmeasured**, not as a win.
9. Binary provenance as in LANE 1: a second `cargo build --release` with zero `Compiling` lines,
   `sha256` recorded, and the measurement asserted to postdate the fix commits.

---

## 12. LANE 3 — the orientation peel term

**Goal:** the objective stops double-charging the correct flat pose, using the sweep's real
demand and a literature-anchored peel-force law.

### The change

Today (`crates/plinth-orient/src/score.rs:11-16`):

```
score(u) = (w_sup·support_area_cm²(u) + w_peak·peak_xsec_cm²(u)) + w_h·height_mm(u)
           + w_scar·Σ_down(area_mm²·delicacy)/100
```

with `w_sup = 0.5`, `w_peak = 3.0`, `w_h = 0.3`, all registered Placeholders the registry itself
describes as carried over "with NO measurement behind it" (`placeholders.rs:1296-1300`). On
Round 50 the peak term alone is 80–84 % of the flat pose's score, so every disc tilts.

**Two replacements, one interface:**

1. **`peak_xsec_cm2` → a peel-force term.** `F(A) = c·A^β` evaluated at the pose's already-computed
   `profile.peak_mm2()` (`score.rs:969`). Because `F` is monotonic increasing in `A` for every
   `β > 0`, "worst single peel event over all layers" collapses algebraically to `F(peak_mm2)` —
   no new per-layer scan, batch-first by construction. The weight `w_peak = 3.0` and the
   accessor are **retired**, not zeroed, following the scar-free precedent at `score.rs:520-532`
   (a structurally absent quantity, not a zeroed one that is still consulted).
2. **`support_area_cm2` → measured demand from the sweep.** A projected down-facing-area
   prefilter at the `SearchTier::Coarse` tier (`crates/plinth-orient/src/search.rs:258-272`),
   then a real coarse-raster sweep on the `ORIENT_REFINE_TOP_K` poses promoted to Refine/Commit.
   No interim proxy (§C9). The weight `w_sup = 0.5` is unchanged in value; only the quantity it
   multiplies changes.

#### AMENDMENT 2026-09-03 — which quantity, and on which raster

Both halves of item 2 as written above were tried and both were wrong. What replaced them was
measured, and the measurements are re-run in `crates/plinth-sweep/tests/pose_demand_probe.rs`
so this section cannot go stale.

**The quantity is the ARM COUNT, not `demand_area_mm2` and not `total_scar_mm2`.**

| pose (production pitch 0.05 mm) | demand mm² | arms | scar mm² |
|---|---|---|---|
| Round 50 flat | 1812.41 | 243 | 163.017 |
| Round 50 flipped 180° | 1778.12 | 283 | 162.837 |
| Round 50 tilted 10° | 21.29 | 495 | 165.080 |
| Round 25 flat | 416.85 | 70 | 40.596 |
| Round 25 flipped 180° | 417.39 | 88 | 40.518 |
| Round 25 tilted 3.6° | 29.84 | 117 | 40.956 |

`demand_area_mm2` counts model area with no rooted carrier *at its own birth layer*, so a flat
down-face contributes all of itself and a sloped one almost nothing — it is anti-correlated with
the burden, and substituting it tilted every disc and laid the overhang table on its side.
`total_scar_mm2` cannot rank poses at all: `SweepPlan::total_scar_mm2`'s own invariant is
`Σ π r² = injected_load / X_ALLOWED` (equal to `peak_layer_area / X_ALLOWED` only on
flat-bottomed models — the fixtures measured here — since a sloped underside injects load on
more than one layer), *independent of how many arms share the load*, so the
area law holds it nearly constant across poses (Round 25's four poses span 1.8 %, their arm
counts 73 %) and inside that band it ranks the 180° flip backwards on both discs. The arm count
ranks every measured comparison correctly and is the burden the owner actually pays.

**Unit bridge, and the open ruling.** The registered weight is denominated in cm², so the
burden is reported as `arms · π · TIP_RADIUS_DEFAULT_MM² / 100` — one fixed positive factor for
every pose, hence a unit conversion that cannot re-rank anything. `arms · mean tip area` is not
available as an alternative bridge: it is arithmetically identical to `total_scar_mm2`. **No
exchange rate between "one arm to remove" and a millimetre of height or a Newton of peel has
been measured, and none was invented here**; `weight_flips` publishes the crossover instead.

**The raster is the PRODUCTION pitch, on `ORIENT_COMMIT_TOP_K` finalists.** The coarse raster is
a real measurement and a wrong ranker — at 0.5 mm, Round 25's flat pose takes 102 arms and its
180° flip 88, the reverse of the production order — so it is demoted to a second prefilter over
the commit tier's whole admitted set, and only the top `ORIENT_COMMIT_TOP_K` are re-swept at
`SWEEP_WORKING_PITCH_MM`. A production-measured candidate sorts ahead of a prefilter-only one
regardless of score, so a coarse number can keep a pose in the running and never crown it.
Measured cost of the second stage, per model: 3.05 s (Round 25), 15.28 s (Round 50), 28.39 s
(Round 100), 15.26 s (overhang table), 37.38 s (Cylinder), 99.68 s (Weeping Treant).

**Acceptance, measured on the emitted meshes:** Round 25/50/100 all flat (no tilt), overhang
table at identity, Cylinder and Treant emit. Round 50 and Round 100 are recess UP. **Round 25 is
recess DOWN**, and the cause is *not* the support term: on a 180° flip the height, peak and peel
terms are identical to the last digit, the support term prefers the correct pose by 0.00071
points, and `ORIENT_SCAR_WEIGHT`'s scar term prefers the flip by 0.04482 — a factor of 63.
`ORIENT_WEIGHT_SUPPORT_AREA_CM2` would have to be ≈31.7 instead of 0.5 to overturn it. That is
an open question against the scar term, not a number to tune here.

Coefficient, from the literature's own suggested form
(`docs/evidence/literature/LITERATURE-NOTES.md:826`, `lit-green-peel.md`):

```
F(A) = P_ref·A_ref·(A/A_ref)^β ,  P_ref = 9000 Pa, A_ref = 314 mm²  (Manizani FEP, ~2 mm/s;
                                   LITERATURE-NOTES.md:136, :772)
c = P_ref·A_ref / A_ref^β  ⇒  β = 0.24: c = 2.826/3.9740 = 0.7112
                              β = 0.46: c = 2.826/14.079 = 0.2007          [arithmetic verified]
```

### The exponent and pressure are a **bracket**, evaluated at both ends

This closes three of the orientation spec's open questions without ratifying an invented point
value. The literature gives two exponents from two *different* experiments — `n ≈ 0.24` varying
porosity at a constant envelope (`LITERATURE-NOTES.md:813`), `n ≈ 0.46` shrinking the envelope
(`:815`) — and three pressures (9.6 kPa Manizani, 12–13 kPa AmeraLabs `lit-green-peel.md:20-22`,
15.6–19.6 kPa Pan et al.).

**Decision: register the bracket, evaluate the objective at all four corners
(β ∈ {0.24, 0.46} × P_ref ∈ {9.0, 19.6} kPa), and require the winning pose to be the same at
every corner.** Agreement is the decision; disagreement is a `pose_choice_not_robust` warning
naming the corner that disagrees and the crossover value. A constant that only prunes and
reports has not decided an outcome — and this is exactly where the orientation spec's own risk
analysis said Round 100 is fragile. For a pose comparison the envelope *does* change, so
`β = 0.46` is the more apt of the two; it is not therefore preferred, it is evaluated.

The same treatment retires the orientation spec's `ORIENT_PEEL_POINTS_PER_NEWTON = 1.0` problem.
No exchange rate between Newtons and the unmeasured height/scar "points" can be derived, so
inventing one would be a placeholder deciding an outcome. Instead: the weight is registered a
Placeholder with provenance that says it is a **policy** exchange rate, and **the run report
publishes, per model, the value of each weight at which the winning pose flips**. A crossover
within a factor of 2 of the registered value raises `pose_choice_not_robust`. The decision still
gets made; its fragility is never hidden.

### No tilt term

Decided in §C7 on `docs/physics.md:150-158` and the S²-only search space. Flagged as OR-1 only
because the brief asks for it literally.

### Files modified

```
crates/plinth-orient/src/score.rs        retire peak_xsec_cm2 (:195-196, :213-214, :235-240,
                                         :407-408, :997, :1002, :1020) and its weight assertion
                                         (:1546-1553); add peel_force_n from profile.peak_mm2()
                                         (:969); support term consumes PoseSupportDemand
crates/plinth-orient/src/search.rs       prefilter at Coarse (:258-272), sweep demand at
                                         Refine/Commit via the escort mechanism (:592-607)
crates/plinth-core/src/constants/evidence.rs
                                         + ORIENT_PEEL_REFERENCE_PRESSURE_PA (9000, bracket to
                                           19600), ORIENT_PEEL_REFERENCE_AREA_MM2 (314),
                                           ORIENT_PEEL_AREA_EXPONENT ([0.24, 0.46])
                                         registered Measured-EXTERNAL with the literature
                                         citations, following the precedent of
                                         TIP_RADIUS_MAX_GROWN_SCAR_MM (:343-352), which is
                                         already an external requirement in this file. Each note
                                         must say in terms: measured, but NOT on this machine
                                         and NOT with this resin.
crates/plinth-core/src/constants/placeholders.rs
                                         + ORIENT_PEEL_POINTS_PER_NEWTON (policy exchange rate)
                                         retire ORIENT_WEIGHT_PEAK_XSEC_CM2 (:1298)
crates/plinth-core/tests/constants_*.rs  counts and id order
crates/plinth-orient/tests/orientation_scoring.rs        re-derive the cube pin (:1220 in
                                         score.rs's own tests, score == 6.56) and the sphere
                                         scar-free pin (:94-112, 16.34494)
crates/plinth-orient/tests/orientation_dense_sweep.rs    re-derive :91-137 (16.3009 winner)
```

### Acceptance checks

1. Round 25/50/100 pick **flat, recess UP** at all four bracket corners. Any corner that
   disagrees is reported with its crossover, not tuned away.
2. `z_spin_is_a_null_direction_of_every_term` still green (no azimuth crept in).
3. **Every** corpus model's winning pose re-measured under the new objective and the diff
   reported model by model. No pin is silently re-pinned; a moved winner is a finding to state.
4. `make_bevelled_disc` — a genuinely non-planar rest face — is measured. The orientation spec
   *predicts* a mild tilt; that prediction is unverified and this is where it is checked.
5. The run report carries the flip-crossover block for every weight, and
   `pose_choice_not_robust` fires where a crossover is within 2× of the registered value.
6. Binary provenance as in LANE 1.

---

## 13. What v1 skips, and the exact disclosure

Skipping is legitimate; **hiding** a skip is not. Every row below must appear in the run report
of a `--placement sweep` run.

| old stage | v1 |
|---|---|
| `perception.rs` islands / minima / candidates | **Replaced.** Demand is `U > 1` on the raster. |
| `perception.rs` load ledger (`:641-648`, `:610-612`) | **Replaced.** RC4 dies; sizing reads `X_ALLOWED` directly. |
| `coverage.rs` / `dyadic.rs` triangle-refined raster with a flat 2e6 cap that fails Preparation | **Replaced.** RC3 dies; cost is `O(area/p²)` and the budget coarsens instead of refusing. |
| `coverage_closure.rs` / `coverage_repair.rs` (the multi-hour repair grind) | **Skipped.** The up-sweep *is* the closure: it terminates with `Dm(k) = ∅` or a named residual. There is no repair loop to grind. |
| `specialized.rs` island-birth / minimum consolidation | **Subsumed** — those are the `U = ∞` cells; S6 proves it. |
| `initial.rs`, `cco.rs`, `rewire.rs`, `forest.rs`, `joints.rs` (branching, joint balls) | **Skipped.** Straight vertical columns only. Costs plate area and resin against a branched forest; no certified joint enclosure at strut-onto-strut junctions. |
| `brace.rs` cross-links and model rungs | **Skipped.** `docs/specs/audit-spread-criterion.md`'s lateral modes are unenforced. Moot on a dense short disc field; material on tall models. |
| `anchor.rs`, `seat_zone.rs`, `designed_embed.rs`, `legalization.rs` | **Partly reproduced.** Land-at-strike, truncate-at-the-strike-face and the 0.100 mm embed depth are kept. **Clearance masks, the two-tier placement/hard margins and crevice forgiveness (`docs/physics.md:160-283`) are skipped**: a strut passing 0.2 mm from a wall is not detected in v1. |
| `member_sizing.rs`, `transient.rs` | **Replaced** by the area law along the column. |
| `lower.rs` | **Reused** for geometry, driven by the new adapter. |
| `raft.rs` | **Skipped**, with a `RootFlare` per plate landing standing in. |
| `census.rs`, `legacy_compat.rs`, `digest_seal.rs` | **N/A.** The sweep has its own digest and re-freezes nothing. |

### Run-report disclosure — the exact wording

Every `--placement sweep` run must print this block verbatim, with the bracketed values filled
in:

> **PLACEMENT: slice-sweep v1 (experimental). Read this before judging the file.**
> This support field was placed by the slice-sweep core, not the legacy engine. It supports the
> model by a measured reach criterion and sizes every arm by the registered area law
> `π r² ≥ carried_area / X_ALLOWED` with `X_ALLOWED = 12`.
> **Reach used for placement: `κ = [42.6006450]` (LAMINA_REACH_RATIO, Derived from
> L_SURVIVED_MAX_MM = 2.1300322500 mm and LAYER_HEIGHT_DEFAULT_MM = 0.050 mm, plates 1 and 4
> pooled).** The audit gates span at `κ = [26.8995249]`; this field
> **[will / will NOT] pass that check** — placement and gating are different jobs and this
> difference is disclosed, not resolved.
> **v1 does NOT do any of the following, and their absence is not a claim that they are
> unnecessary:** no branching (every arm is a straight vertical column), no cross-bracing, no
> joint spheres at arm-onto-arm junctions, no raft (a root flare stands in at each plate
> landing), no clearance checking of any kind (an arm passing close to a wall is **not**
> detected), and no repair loop. The peel field is uniform: the literature's area exponent and
> the machine's front-to-back tilt factor are computed and reported below but decide nothing.
> **Warnings on this file:** [residual demand cells / uncarried load / overloaded arms with
> their realized X / arms above the owner's 0.400 mm-radius growth ceiling / model sections
> carrying more than X_ALLOWED / arms surviving at the minimum tip radius / coarsening steps and
> achieved pitch / chamfer non-convergence / slenderness above 50:1 / reach decisions binding at
> cured thickness above one lamina].
> **A shortfall above is a WARNING on an emitted file, never a gate.** This output is diagnostic
> evidence about the engine.

Full report field list: `achieved_pitch_mm`, `coarsening_steps`, `layers`, `cells_per_layer`,
`kappa_used`, `kappa_first_loss`, `kappa_audit_gate`, `will_pass_audit_span_check`,
`max_tau_at_binding_reach_mm`, `u_from_vertical_max`, `tau_vertical_over_tau_used_max`,
`struts{total, by_landing{plate, model, strut}, interior_landings}`,
`seat_spacing{mean, p95, max}`, `tip_radius{min, p50, max, count_at_min, count_at_max}`,
`slenderness{p50, max}`, `peak_layer_area_mm2`, `total_scar_mm2` (which must equal
`injected_load_mm2 / X_ALLOWED` — the invariant of `docs/physics.md:100-118` — and that equals
`peak_layer_area_mm2 / X_ALLOWED` only on flat-bottomed models),
`residual_demand_cells`, `residual_demand_mm2`, `uncarried_load_mm2`, `overloaded_struts[]`,
`tips_above_owner_growth_ceiling[]`, `model_section_overload[]`, `landing_delicacy` histogram,
`beta_area_stats`, `beta_tilt_stats` (reporting only), `conservation_residual`, `sweep_digest`,
`target_spacing_mm_registered_but_unused`, and the provenance block naming `κ`'s parents and
§5.4's 7.5× beam-model disagreement.

`sweep_digest` covers `(p, K, per-strut (cell, k_top, k_bottom, landing, radius profile
quantized to 1e-6 mm, carried profile quantized to 1e-6 mm²))` in id order — the role
`SupportVerificationSnapshot::digest` plays today (`crates/plinth-support/src/runner.rs:4008`).

---

## 14. Decisions taken here, and the owner rulings still open

### Decisions (no ruling needed)

| id | decision |
|---|---|
| D1 | Disc demand = measured bottom-layer area; conservation total = **peak layer** area, computed by the harness from the slice stack (§C1, §C2). |
| D2 | Reach fringe is a Minkowski sum with a disc — rounded corners (§C3). |
| D3 | Keep `U = ∫ds/(κτ)`; no acceptance number extrapolates it; report `max_tau_at_binding_reach_mm` (§C4). |
| D4 | `κ = LAMINA_REACH_RATIO`, computed from parents at run time, never rounded; the audit-gate and first-loss values are reporting-only (§C5). |
| D5 | `τ = min(vertical run, 2 × in-plane distance transform)` in v1 — two extra passes to remove the design's only unsafe-direction approximation (§5.3, closing the algorithm spec's OQ-2). |
| D6 | No branching in v1. LANE 2 **measures** the resin and plate-area cost on the treant; that measurement decides v1.1, not a prediction (§7.5). |
| D7 | No clearance checking in v1. The gate models have no crevices; the treant is measured with the absence stated in the run report; the clearance batch is the first v1.1 item before the treant becomes a print candidate. |
| D8 | New `SweepLimits`; do not reuse `SupportPreparationLimits`, which is shaped for the legacy kNN/gate-batch resource model. |
| D9 | New `SweepRunReport`; do not overload `RepairRunReport`. |
| D10 | Expose slicing through one new `pub fn plinth_support::slice_stack`; `mod orientation` stays private and `orient_layers_batch` is not re-exported. |
| D11 | Commit `Round_25/50/100.stl` to `corpus/models/` with manifest entries in LANE 2 — a ratchet row that cannot be reproduced from a clean checkout is unfinished work. |
| D12 | Append the new evidence ids at the end of `EXPECTED_EVIDENCE_IDS`'s declaration order. |
| D13 | Orientation: no interim rest-face proxy; LANE 3 consumes the sweep's real demand (§C9). |
| D14 | Orientation: register the literature exponent and pressure as **brackets**, evaluate all four corners, require winner invariance, warn on disagreement — instead of ratifying an invented point value (§12). |
| D15 | Publish per-weight flip crossovers in the run report; warn when a crossover is within 2× of the registered weight. A policy weight may decide, but never invisibly (§12). |
| D16 | Column buckling at ~60:1 slenderness: report the histogram, invent no constant, gate nothing; a coupon is the fix (§7.3). |
| D17 | The acceptance bands in §7.4 are **test tolerances**, documented as such, not registered constants. |

### Owner rulings requested — three, concrete

**OR-1 — the peel field, and the tilt factor.** The brief asks for a per-pixel peel field with
a literature area exponent and the Saturn 4 front-to-back tilt factor as decision inputs. This
document computes both and lets neither decide, because turning the area term on contradicts
`docs/physics.md:69-75` (the measured `X` boundary collapsed across a 40× cap-area span) and
turning the tilt term on contradicts `docs/physics.md:150-158` (Y-sensitivity settled at ≤1 rung;
"randomize over that axis rather than tune it"), and because the orientation search space is
S²-only with no azimuth. **Does the brief supersede those two records, or do both stay
reporting-only until a coupon says otherwise?** (Default if no ruling: reporting-only, as built.)

**OR-2 — placement `κ` vs the audit's gate `κ`.** Placement uses 42.6006450 (the pooled measured
lower bound, corroborated by two independent field data points at 46.2 and 48.2); the audit
gates spans at 26.8995249 because it subtracts a 1.53× condition shift and two pixel diagonals
to refuse a finished file. Emitted sweep fields will therefore fail the audit's span check by
design. **Teach the audit a placement-vs-gate distinction, or accept that the check fails with
the reason disclosed in the run report?** Out of scope for LANE 1–3 either way; the report field
`will_pass_audit_span_check` exists so the answer is never assumed.

**OR-3 — raft.** v1 emits a `RootFlare` per plate landing and no raft. This is answerable only
by opening LANE 2's disc files in ChituBox. **Do the discs need a raft, or is the flare enough?**
If a raft is needed, `plinth_support::raft` operates on live forest members and needs a small
adapter — the first v1.1 item. (2026-09-28: while the question is open, the flare stands ON the
plate, not on the top of the absent raft; see §9.2.)

### Standing question, unresolved by measurement

The 7.5× gap between the plates' measured `κ = 42.6` and the cantilever-bending prediction of
5.657 from the registered `TENSILE_GREEN_MPA` / `PEEL_PRESSURE_PA` pair (§5.4). Both inputs are
Placeholders, so this is most likely a fact about them rather than about the plates, but it is
unexplained. It is recorded here as a standing question against any future green-strength
coupon and is printed in every run report's provenance block.

---

## 15. Out of scope

The corpus comparison protocol that decides when `--placement sweep` becomes the default;
hollowing, drainage, decimation and mesh repair; the audit's own span criterion (OR-2);
branching, bracing, joints, raft and clearance (v1.1, in that order, gated by LANE 2's
measurements).
