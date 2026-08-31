# Audit interval policy — conservative bounds for every threshold comparison

Status: written 2026-08-30 to close Finding 3 of the cross-family NOT TRUSTWORTHY
review of `tools/plinth-audit/`. This document changes no code. It is the work
order a later agent applies mechanically to `tools/plinth-audit/src/`.

**The defect this closes.** Several checks implement `delivered >= required - tol`.
That form passes a value that is genuinely short of the requirement whenever the
shortfall is smaller than the measurement uncertainty. It reads a value the audit
cannot tell apart from "meets the requirement" and reports "meets the requirement"
instead of "cannot tell". `docs/specs/audit-contract.md`'s own governing rule says
the opposite is required: **anything not established is a hard failure.** A value
inside the uncertainty band has not established that it meets the requirement, so
it may not pass.

**How to read this document.** §1 is the exhaustive inventory — every comparison
against a threshold in `tools/plinth-audit/src/`, one row per comparison, with a
verdict. §2 derives the uncertainty magnitude for every source that produces one.
§3 states the interval rule and, separately, which bound is conservative for each
comparison's direction. §4 names the comparisons that are exempt because their
tolerance is intentional, not measurement error. §5 is the reporting change. §6
predicts, per check, whether the fix moves anything in the current baseline.

---

## 1. The inventory

Every comparison in `tools/plinth-audit/src/` that tests a measured or
reconstructed quantity against a threshold, fixed value, or another measured
quantity. "Verdict" is this document's classification: **OPTIMISTIC** (passes a
value the evidence does not establish), **CONSERVATIVE** (already fails on doubt),
**EQUALITY-BAND** (a two-sided consistency check, a different category from
"optimistic/conservative" — see §3.3), or **EXEMPT** (tolerance-of-intent, see
§4). A row marked OPTIMISTIC is a required change. A row marked CONSERVATIVE,
EQUALITY-BAND or EXEMPT is not, and is listed so the sweep does not touch it by
mistake.

### 1.1 I1 — seat depth (`src/checks/seat_depth.rs`, and its three reimplementations)

The same predicate is independently reimplemented **four times** — once as I1
itself and three times as a "does this contact ground something else" test
inside other checks, per the project's own rule that "each invariant establishes
what it needs independently" (`narrowest_section.rs:348` doc comment). All four
carry the identical bug, because it was copied, not shared.

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 1 | `seat_depth.rs:399` (`shallow_deficit`) | `h_delivered_mm` | `h_required_mm` | `h_delivered_mm >= h_required_mm - tol_mm` | **OPTIMISTIC** |
| 2 | `narrowest_section.rs:356` (`seat_reaches_required_depth`) | `h` | `h_req` | `h >= h_req - seat.tol_geom_mm()` | **OPTIMISTIC** (duplicate of #1) |
| 3 | `dead_geometry.rs:277` (ground clause ii, inline) | `hd` | `hr` | `hd > 0.0 && hd >= hr - tol` | **OPTIMISTIC** (duplicate of #1) |
| 4 | `load_path.rs:506` (`seat_ground_status`) | `h` | `h_req` | `h < h_req - tol` → fail (i.e. pass iff `h >= h_req - tol`) | **OPTIMISTIC** (duplicate of #1) |

Two more comparisons live in the same function and are already correct:

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 5 | `seat_depth.rs:203` + `seat.rs:218-219` (`r_tip_agrees`) | `spread` of `{r_tip_a, r_tip_b, r_tip_c}` | `r_tol = max(tol_geom, radius_tol_mm(r_ball))` | `spread <= r_tol` | **EQUALITY-BAND**, already correct |
| 6 | `seat_depth.rs:338` (declared-vs-reconstructed) | `\|declared − h\|` | `tol = seat.tol_geom_mm()` | `(declared - h).abs() > tol` → fail | **EQUALITY-BAND**, already correct |

And one selection threshold feeding I5's narrowed far-side probe, reached only
through `load_path.rs`'s copy of the I1 predicate:

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 7 | `load_path.rs:561` (`seat_zone_breach_probe`) | `axial` (a ball-surface sample's position along the contact axis) | `band_hi` (top of the legal seat zone) | `axial > band_hi + tol` selects a point as a breach candidate | **OPTIMISTIC** — narrows the candidate set instead of widening it (§3.2) |

### 1.2 I2 — contact area (`src/checks/contact_area.rs`)

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 8 | `contact_area.rs:415` | `a_rast` | `a_required = a_owned / x_allowed` | `a_rast >= a_required - tol` | **OPTIMISTIC** |
| 9 | `contact_area.rs:387` | `a_rast` | `a_ideal + tol` | `a_rast > a_ideal + tol` → `indeterminate` (reason `raster_exceeds_ideal`), never a silent pass | **CONSERVATIVE**, already correct — excess beyond tolerance is refused, not accepted |

Row 8 is the load-bearing gate: "the cured contact patch cannot carry the model
area this contact owns" (`CONTACT_AREA_SHORT`). It is I2's entire pass/fail
decision.

### 1.3 I3 — narrowest section (`src/checks/narrowest_section.rs`)

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 10 | `narrowest_section.rs:318` (`passes_area_law`) | `a_min_mm2` | `required = a_owned_mm2 / x_allowed` | `a_min_mm2 >= required - tol` | **OPTIMISTIC** |

(Row 2 above, `seat_reaches_required_depth`, also lives in this file and is
counted once in §1.1.)

A separate, non-directional gap in the same file: `ground_flags` (line 331) tests
`p.aabb().min[2] <= plate_z` with **no tolerance at all**, while the structurally
identical test in `load_path.rs:212` and `dead_geometry.rs:267` adds
`+ ground_tol`. This is not the optimistic/conservative bug — a stricter-than-
needed test is the safe direction — but it is an inconsistency across three
copies of the same test and is listed in §3.4 as a coincidence check that should
carry the same tolerance everywhere it is written.

### 1.4 I4 — load ownership (`src/checks/load_ownership.rs`)

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 11 | `load_ownership.rs:118` (`clause_a_unowned`) | `a_unowned_mm2` | `censor_mm2` (one pixel) | `a_unowned_mm2 <= censor_mm2` | **OPTIMISTIC in principle, negligible in practice** (§2.4) |
| 12 | `load_ownership.rs:136` (`clause_b_identity`) | `\|accounted − D_total\|` | `tol_sum(D_total)` | `<= tol_sum(D_total)` → pass | **EQUALITY-BAND**, already correct |
| 13 | `load_ownership.rs:178-180` (`clause_c_components`) | `\|owned − expected\|` per component | `tol_sum(max(expected, want))` | `resid > tol` → **indeterminate** (not fail, not pass) | **CONSERVATIVE**, already correct — ambiguity never resolves to pass |
| 14 | `load_ownership.rs:206` (`clause_d_books`) | `D_total` | bracket `[lo, hi]` | `d_total_mm2 >= lo_mm2 - tol && d_total_mm2 <= hi_mm2 + tol` | **OPTIMISTIC — the sharpest instance in the corpus** (§3.3) |
| 15 | `load_ownership.rs:231` (`clause_e`, demand reconstruction) | `\|D_total − D_raster\|` | `0.5·perimeter·d2 + tol_sum(D_total)` | `<= bound` → pass | **EQUALITY-BAND**, already correct |

Row 14 widens the passing region on **both** ends of the bracket simultaneously —
the most optimistic comparison in the codebase, because it compounds the bug in
two directions on the same predicate. See §3.3 for the correct three-outcome form.

### 1.5 I5 — closed load path (`src/checks/load_path.rs`)

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| — | `load_path.rs:168-172` (pairwise overlap graph) | `sep` (signed separation) | `tol`, then `D3_MM` | `sep <= tol` → connected; `sep <= D3_MM` → indeterminate; else disconnected | **CONSERVATIVE**, already correct — this is the three-outcome form §3.2 asks for everywhere |
| — | `load_path.rs:212` (grounding, primitive bbox) | `bb.min[2]` | `z_ground` | `bb.min[2] <= z_ground + tol` | **EQUALITY-BAND** (coincidence test, not a margin test — §3.4), already correct |
| — | `load_path.rs:802` (`end_status`, per-end grounding) | `end[2]` | `z_ground` | `end[2] <= z_ground + tol` | **EQUALITY-BAND**, already correct, consistent with the row above |
| — | `load_path.rs:824-826` (`end_status`, overlap classification) | `best_d` | `tol`, then `D3_MM` | same three-outcome form as the pairwise loop | **CONSERVATIVE**, already correct |
| (dup of #4) | `load_path.rs:506` | — | — | — | counted in §1.1 |
| (dup of #7) | `load_path.rs:561` | — | — | — | counted in §1.1 |

I5's own predicates — the ones it owns rather than borrows from I1 — are the best
implementation of the three-outcome pattern in the codebase today. Nothing here
needs to change; it is the model the fixes in §1.1, §1.3 and §1.4 should copy.

### 1.6 I6 — no intersection (`src/checks/intersection.rs`)

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| — | `intersection.rs:339`, `:611` (clause a) | `residual_mm3` (exact voxel count × `voxel_mm3`) | `voxel_mm3` | `residual_mm3 <= voxel_mm3` | **CONSERVATIVE / not applicable** — an exact integer voxel count carries no continuous measurement uncertainty; see §3.4 |
| — | clause (b), exterior component count | integer count | `== 1` | exact | **CONSERVATIVE / not applicable**, same reason |

I6 is graded entirely on the cured raster specifically so its quantities are
exact voxel sums (`intersection.rs:1-4` module doc: "so the volumes are exact
voxel sums and the censor is a count of one"). There is no interval to widen.

### 1.7 I7 — island coverage (`src/checks/island_coverage.rs`)

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| — | `island_coverage.rs:126` | `area_mm2` (exact pixel count × `pixel_area`) | `pixel_area` | `area_mm2 <= pixel_area` | **CONSERVATIVE / not applicable**, same reason as I6 |

I7's structural gap (the cantilever hole, Finding 1) is a different defect and is
out of scope for this document. Its one threshold comparison is exact and needs
no interval.

### 1.8 I8 — no dead geometry (`src/checks/dead_geometry.rs`)

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 16 | `dead_geometry.rs:324` (R1 liveness) | `a` (`A_owned(c)`) | `PIXEL_AREA_MM2` | `a > c::PIXEL_AREA_MM2` | **OPTIMISTIC in principle, negligible in practice** (§2.4) |
| — | `dead_geometry.rs:161`, `:169`, `:180` (degeneracy) | exact fields | `0`, `> 0`, non-finite | exact, `<= 0`, no tolerance | **CONSERVATIVE / not applicable** — the spec requires these exact, §3.8 of `audit-invariants.md`: "Degenerate, exact, no tolerance" |
| — | `dead_geometry.rs:211-238` (`verify_against_soup` via `primitives.rs:559-589`) | `max_dev`, `overflow` | `tol_geom_mag(mag)` | `<= tol` → agrees | **EQUALITY-BAND**, already correct |
| (dup of #3) | `dead_geometry.rs:277` | — | — | — | counted in §1.1 |

### 1.9 Non-gating report (`src/checks/report_metrics.rs`)

The report never produces pass/fail, but one of its metrics exists specifically
to measure how many contacts would flip verdict under a different fill rule
(`report.raster_erosion_sensitivity`), so its predicate must track I2's real one
or the disclosed sensitivity count is measuring the wrong gate.

| # | file:line | quantity | threshold | current form | verdict |
|---|---|---|---|---|---|
| 17 | `report_metrics.rs:671` (`full_pass`) | `full_area` | `required` | `full_area >= required - tol` | **OPTIMISTIC**, mirrors row 8 — fix together |
| 18 | `report_metrics.rs:672` (`centre_pass`) | `centre_area` | `required` | `centre_area >= required - tol` | **OPTIMISTIC**, mirrors row 8 — fix together |

Two report comparisons are correctly exempt and untouched: `report_metrics.rs:230-231`
(`below_floor`/`below_target` against `SPACING_MINIMUM_REF_MM`/`TARGET_SPACING_REF_MM`,
explicitly reference lines, never thresholds — `audit-invariants.md` §4) and
`report_metrics.rs:295` (`count_at_min_shoot_r`, an equality-band coincidence
count against `MIN_SHOOT_R_MM`, non-gating).

### 1.10 Summary count

**Ten OPTIMISTIC comparisons** require a change: rows 1–4 (one predicate, four
copies), 7, 8, 10, 14, 17, 18. Two more (11, 16) are optimistic in form but the
uncertainty they would admit is 4–5 decades below the threshold they test,
so the fix is required for correctness but is predicted to change nothing
measurable (§2.4, §6). Everything else in the inventory is already correct,
either because it is an equality-band check (§3.3), an exact integer quantity
(§3.4), or exempt tolerance-of-intent (§4).

---

## 2. Interval semantics — the uncertainty sources, with magnitudes

Every measured or reconstructed quantity the audit compares against a threshold
carries uncertainty from one or more of four sources. All four are already
derived, with citations, in `docs/specs/audit-contract.md` §1.7 and
`docs/specs/audit-invariants.md` §1.5, and restated as code in
`tools/plinth-audit/src/consts.rs`. This section names which source applies to
which comparison; it derives nothing new.

### 2.1 Source A — f32 storage on disk (`eps32`, `tol_geom`)

Every coordinate and radius in the `.chitubox` file is `binary32`
(`audit-contract.md` §1.1). The audit works in `f64`. `eps32 = 2^-24 =
5.960464477539063e-8` is the relative rounding bound of one stored value
(`consts.rs:139`). Composed over the operations a comparison actually performs —
two stored endpoints, one differencing, one Euclidean norm, three coordinates in
quadrature — `audit-invariants.md` §1.5 derives

```
tol_geom(p, q) = 4 * sqrt(3) * eps32 * max(|p|_inf, |q|_inf, 1.0)      mm
```

implemented at `consts.rs:245-262` as `tol_geom`/`tol_geom_at`/`tol_geom_mag`,
**used as a function of magnitude, never flattened to a constant.** At a 100 mm
coordinate this is `4.130e-5 mm`. This is the source behind every `h_delivered`
vs `h_required` comparison (I1 and its three duplicates), the seat-zone axial
band (row 7), and the plate-touch coincidence tests (§1.5, §3.4).

A separate, narrower form applies to a single radius against a reconstruction:
`radius_tol_mm(r) = max(eps32 * r, 1e-9)` (`consts.rs:262-267`), used in the
tip-radius three-way agreement (row 5).

### 2.2 Source B — rasterisation at the pixel pitch and layer height

The printer renders at `0.014 x 0.019` mm in plane and `0.050` mm in Z
(`audit-contract.md` §4). Two derived quantities carry this into geometric
comparisons: `d2 = 2.360085e-2 mm` (in-layer pixel diagonal) and `d3 =
5.529014e-2 mm` (voxel diagonal, `consts.rs:190,194`). `d3` is the width of the
**fusion-uncertain band**: a gap in `(0, d3]` between two solids may or may not
cure into one piece, and I5's pairwise overlap graph already refuses to decide
inside it (`load_path.rs:168-172`, `:824-826`). Areas and volumes that are
themselves exact multiples of `pixel_area` or `voxel_volume` (I6, I7) carry
**no** rasterisation uncertainty of this kind — see §3.4.

### 2.3 Source C — floating-point accumulation over a sum

A quantity built by summing many reconstructed samples (`D_total`, `A_owned`,
the partition's `accounted`) carries the project's own derived summation bound,
cited rather than re-derived (`docs/development.md`'s tolerance rule,
restated at `audit-invariants.md` §1.5):

```
tol_sum(A) = max(4096 * eps64 * |A|, 1e-9)            in the metric's own unit
tol_area(A, x_allowed) = max(4096 * eps64 * A / x_allowed, 1e-9)      mm^2
```

`eps64 = 2.220446049250313e-16`; `4096` is headroom over the largest corpus fold
(`consts.rs:141,145,147`; `tol_sum` at `consts.rs:272-275`, `tol_area` at
`consts.rs:280-283`). This is the source behind every `A_owned`,
`D_total` and `A_min` comparison: rows 8, 10, 11, 12, 14, 15, 16, 17, 18.

### 2.4 Where Source C is real but the magnitude makes it moot

Rows 11 (`A_unowned <= pixel_area`) and 16 (`A_owned > pixel_area`) compare a
Source-C quantity against a *fixed printer censor* rather than against another
measured quantity. `tol_sum`'s absolute floor is `1.0e-9 mm²`; the censor,
`PIXEL_AREA_MM2 = 2.66e-4 mm²`, is five orders of magnitude larger. Even at the
relative branch (`4096 * eps64 * A`), `A` would have to exceed roughly `3e8 mm²`
before the relative term matched the censor — far outside anything printable.
**The interval is real, the fix is correct to make, and no file in the current
corpus is close enough to either censor for it to change a verdict** (§6).

### 2.5 Source D — geometric reconstruction tolerance

Distinct from Source A: this is the tolerance of a *derived geometric object*
(a fitted sphere, a cut plane, a normal), not of one stored value. It appears as
`tol_geom` applied to a reconstructed point (the tip-soup-fitted centre, the
axis-crossing entry point) rather than to a raw record field. It is the same
formula as §2.1 and is not a fifth constant — it is listed separately here only
because its *input* is a computed point, not a stored one, so the magnitude
argument to `tol_geom` must be the reconstructed point's own coordinates, not
the record's.

---

## 3. The rule, and which bound is conservative for which direction

### 3.1 The rule

For a measured or reconstructed quantity `V` with derived uncertainty `tol`,
treat `V` as the interval `[V - tol, V + tol]`. A predicate comparing `V`
against a requirement `R` (fixed, or itself an interval — see §3.3) has three
outcomes, never two:

```
PASS           the conservative bound of V clears R with room to spare
FAIL           the conservative bound of V clearly does not clear R
INDETERMINATE  the interval straddles R: neither bound resolves it
```

"Conservative" is direction-dependent and is the entire content of Finding 3.
Applying one rule blindly to both directions inverts half the checks — which is
exactly what rows 8, 10, and the lower branch of row 14 got backwards, in the
opposite sense from rows 1–4, 7, and the upper branch of row 14.

### 3.2 Minimum-type predicates — "delivered must reach or exceed a requirement"

Rows 1–4 (`h_delivered` vs `h_required`), 8 (`a_rast` vs `a_required`), 10
(`a_min` vs `a_required`), 16 (`A_owned` vs `pixel_area`, read as "must clearly
carry demand"), the lower branch of row 14 (`D_total` vs `lo`), and 17–18
(mirrors of row 8).

`V` is the delivered/measured quantity; `R` is the requirement. **The
conservative bound of `V` is its LOWER bound**, because passing must mean "even
in the worst case the evidence admits, this still clears the bar":

```
PASS   iff  V - tol >= R
FAIL   iff  V + tol <  R
INDETERMINATE   otherwise   (i.e. R - tol <= V < R + tol, and not already PASS)
```

The current buggy form is `V >= R - tol`, i.e. `V + tol >= R` — it uses `V`'s
**upper** bound against `R`, which is backwards for this direction. Correct
replacement, stated once for every row in this class:

```
deficit-style rows (1-4, 8, 10, 17, 18):
    PASS            V - tol >= R
    FAIL            V + tol <  R
    INDETERMINATE   R - tol <= V + tol  and  V - tol < R      (straddles)

censor-style rows (16, and the lower branch of 14):
    "clearly exceeds" reads the same way: V's lower bound must clear R.
```

Row 7 (`axial > band_hi + tol`, a *candidate-selection* threshold rather than a
final pass/fail) inverts the same principle from the selection side: the goal
there is to not miss a real breach, so the selection must be **inclusive** of
anything that might be beyond the band, i.e. it should use `band_hi`'s
effective lower edge: `axial > band_hi - tol`. Tightening the selection (`+
tol`, the current form) is the minimum-direction bug wearing a filter's
clothing — it silently excludes points that could be genuine breaches, biasing
`seat_zone_breach_probe` toward `Clean`.

### 3.3 Maximum-type predicates — "delivered must not exceed a limit"

Row 9 (`a_rast` vs `a_ideal`, already correct), row 11 (`A_unowned` vs
`censor`), and both branches of row 14 taken together (the bracket).

`V` is the delivered/measured quantity; `L` is the limit. **The conservative
bound of `V` is its UPPER bound**, because passing must mean "even in the worst
case, this still does not exceed the limit":

```
PASS   iff  V + tol <= L
FAIL   iff  V - tol >  L
INDETERMINATE   otherwise
```

Row 9 already does this correctly by routing "exceeds beyond tolerance" to
`indeterminate` rather than accepting it, which is the maximum-direction form
of never letting doubt resolve to pass.

**Row 14 is a two-sided bracket, `lo <= D_total <= hi`, and needs both rules at
once, not one rule applied twice with the same sign.** `D_total` must clear `lo`
from above (a *minimum*-type relation to `lo`) **and** must clear `hi` from
below (a *maximum*-type relation to `hi`):

```
PASS            D_total - tol >= lo   AND   D_total + tol <= hi
FAIL            D_total + tol <  lo   OR    D_total - tol >  hi
INDETERMINATE   otherwise (the interval overlaps lo or hi without clearing it)
```

The current code (`d_total_mm2 >= lo_mm2 - tol && d_total_mm2 <= hi_mm2 + tol`)
gets **both** halves backwards simultaneously: it uses `D_total`'s upper bound
against `lo` (should be the lower bound) and `D_total`'s lower bound against
`hi` (should be the upper bound). It is the widest of the ten OPTIMISTIC rows
because the two errors compound on the same predicate.

### 3.4 Equality-band predicates are a different category, not a third direction

Rows 5, 6, 12, 13, 15, and the coincidence tests in I5 and the plate-touch tests
in §1.3/§1.5, compare two independently reconstructed (or declared-vs-
reconstructed) values that are supposed to be **the same physical quantity**,
not a delivered value against a design requirement. There is no "which side is
conservative" question here, because there is no safety margin being tested —
only whether the two numbers are the same number up to the evidence's own
resolution:

```
combined_tol = the derived bound covering both sides' own uncertainty
                 (already what tol_geom / radius_tol_mm / tol_sum compute)
CONSISTENT (pass)   |a - b| <= combined_tol
DIFFERENT  (fail)   |a - b| >  combined_tol
```

There is no third INDETERMINATE state to add here: "within combined tolerance"
already means "the evidence cannot distinguish these, which is the expected
outcome for two correct reconstructions of one quantity" — that is a pass, not
an unresolved question. Converting these to the minimum/maximum interval form
would be a category error: `|a-b| <= tol` is already the correct, symmetric,
already-conservative form, and every equality-band row in the inventory already
uses it. **Do not touch rows 5, 6, 12, 13, 15, or the coincidence tests in I5,
`narrowest_section.rs`, `load_path.rs`, or `dead_geometry.rs`.** The one
genuine gap in this category is cosmetic, not directional: `narrowest_section.rs:331`
omits the `+ tol` that the structurally identical test carries in
`load_path.rs:212` and `dead_geometry.rs:267`. Add it there for consistency;
it does not change which direction is safe, because a coincidence test with no
tolerance is already at least as strict as one with it.

---

## 4. Tolerance-of-intent — named, and why it is not measurement uncertainty

Some bands in the code are not derived from any storage, rasterisation, or
summation uncertainty; they are design decisions about how much real geometric
variation to tolerate, and converting them to the §3 interval form would treat
an intentional design choice as an unresolved measurement. None of the
following change under this policy.

- **`JOIN_TOL_EMITTED_MM = 1.0e-3` / `JOIN_TOL_NATIVE_MM = 2.0e-2`**
  (`consts.rs:157-160`, used in `graph.rs:306` and nowhere else). This is a
  **coincidence radius for graph reconstruction** — the exact example named in
  this document's own charge. `audit-contract.md` §3.2 measured a five-decade
  empty band between the worst true coincidence (1e-6 mm) and the smallest real
  separation (1e-1 mm) on emitted files, and chose `1e-3` inside that band by
  policy, not by deriving a storage or rasterisation bound. It decides "are
  these two endpoints the same topological node", a graph-construction
  question, not "does this contact meet its physical requirement".

- **The 5-degree `normal_ambiguous` threshold** (`seat.rs:112-116, 344-348`).
  The angle between the crossed triangle's own normal and the area-weighted
  mean normal over the ball's intersection with the surface is compared
  against a flat 5 degrees, chosen because it is where the project judged a
  coarse tessellation makes the incidence term unreliable — a facet-coarseness
  admission, not a stored-value uncertainty. It already fails safe (routes to
  `indeterminate`, never to a pass), so there is nothing to change; it is named
  here only so a mechanical sweep does not "fix" it into an interval it was
  never meant to have.

- **`grazing_tol` in `mesh.rs`'s ray-parity retry loop** (`mesh.rs:581-645`,
  called at `intersection.rs:1207`, `load_path.rs:570` via `points_inside`).
  This decides when a ray hit is too close to a triangle edge or vertex to
  trust, triggering a retry with the next direction of the fixed golden-spiral
  sequence (`audit-invariants.md` §1.4). It is a numerical-robustness constant
  for a watertight point-in-solid test, not a measurement of the model's own
  geometry, and sixteen exhausted retries already produce `indeterminate`
  rather than a guessed answer.

- **The raft pad/bridge contract** (`RAFT_PAD_DIAMETER_MM = 2.2`,
  `RAFT_PAD_HEIGHT_MM = 0.2`, `RAFT_BRIDGE_TRIANGLES = 12`, `consts.rs:106-112`).
  These are a reverse-engineered fixed geometric contract for a primitive type,
  not a measured quantity with rounding error. Comparisons against them (mesh-
  agreement checks on type-6 records) already go through the same `tol_geom`-
  based `verify_against_soup` as every other primitive (§1.8), which is
  correct — the tolerance there covers the *stored f32 vertices'* rounding
  against the analytic pad, not the pad's own definition.

- **`SPACING_MINIMUM_REF_MM = 2.5` / `TARGET_SPACING_REF_MM = 4.0`**
  (`consts.rs:223,225`, used at `report_metrics.rs:230-231`). Explicitly
  documented as "reference lines, never thresholds" in
  `audit-invariants.md` §4 and never gating. Not a tolerance at all, and not
  in scope for interval treatment because nothing here is pass/fail.

---

## 5. The reporting change

Every comparison identified as OPTIMISTIC in §1 must report three states, not
two, at the per-item level:

```
PASS            conservative bound clears the requirement (§3.2 / §3.3)
FAIL            conservative bound clearly does not clear it
INDETERMINATE   the interval straddles the threshold
```

**INDETERMINATE counts as a failure for gating.** This is not new policy — it is
already `audit-invariants.md` §0 rule 3 and §5 pinned-rule 1
(`"status"` takes exactly `"pass"`/`"fail"`; `indeterminate > 0` forces
`"fail"`) — and the per-item JSON rows in `contact_area.rs` already write
`"status": "indeterminate"` distinctly from `"pass"`/`"fail"` for the cases the
code already treats this way (rows 9, 13). **This document's change is to bring
rows 1–4, 7, 8, 10, and the two halves of 14 into that same three-state shape**,
so a value inside a straddling band gets `"status": "indeterminate"` with the
interval reported (`value_mm ± tol_mm`, or the two edges `V - tol` and `V + tol`
explicitly), rather than falling through to `"pass"` as it does today. The
distinction must survive to the failure line and to `measured`, per
`audit-invariants.md` §5 pinned-rule 3: a passing invariant that reports no
numbers is not evidence, and neither is an indeterminate one that reports only
"indeterminate" with no interval a reader can judge for themselves. Every new
`*_INDETERMINATE` line this produces should name the interval, e.g.

```
SEAT_DEPTH_INDETERMINATE ... reason=within_tolerance_band
  h_delivered_mm=<v> h_delivered_interval_mm=[<v-tol>,<v+tol>] h_required_mm=<v> tolerance_mm=<tol>
```

so a reader can see, per `audit-invariants.md`'s own phrase, "how close it was."
The existing failure-code vocabularies in §3 of `audit-invariants.md` already
reserve a slot for this (`SEAT_DEPTH_INDETERMINATE`, `CONTACT_AREA_INDETERMINATE`,
`MIN_SECTION_INDETERMINATE`, `LOAD_OWNERSHIP_INDETERMINATE`); the new reason
token to add to each is `within_tolerance_band` (or, for row 14,
`books_bracket_straddled`), distinct from the reasons already listed there for
"could not compute at all".

---

## 6. Predicted effect on the current baseline

**These are predictions, not measurements.** No code has changed and the audit
has not been re-run for this document; that is the implementing agent's job,
with a before/after diff against `docs/evidence/audit-baseline-2026-08-30.md`.
The predictions below are bounded by the magnitude arguments of §2 and by the
baseline's own published per-invariant counts, not invented.

**I1, I2, I3 (rows 1–4, 8, 10): no predicted change to model-level gating
status.** In the 2026-08-30 baseline, every one of the 31 audited files already
has `failed > 0` on I1, I2 and I3 (`docs/evidence/audit-baseline-2026-08-30.md`
§1.1–1.2: no row shows a clean `pass` for any of these three invariants). Since
gating status is already `"fail"` for all 31 files on all three invariants, a
predicate correction that can only ever move an item from a false pass to a
true fail or indeterminate cannot make a `"fail"` model more failed. **What it
predicted to change is the per-item counts inside `failed`/`indeterminate`**:
some of the contacts currently counted as passing within an otherwise-failing
model will move to `indeterminate`. The size of that shift cannot be bounded
without re-running the tool, because it depends on how many individual contacts
sit within roughly `2 * tol_geom` (≈ 1e-5 to 1e-4 mm depending on coordinate
magnitude) or `2 * tol_area` (≈ 1e-9 to 1e-7 mm² for corpus-scale `A_owned`) of
their own requirement — a question about the corpus's own distribution, not
about the fix.

**Row 14 (I4 clause d): the one candidate for an actual model-level flip.**
`zombie-3-a` is I4's **only** clean pass in the 31-file corpus (§1.1 table,
line for `zombie-3-a`: `pass`). Row 14 is also the widest of the ten OPTIMISTIC
rows — it loosens both ends of a bracket simultaneously (§3.3) — so it is the
single most likely place in this document's scope for a real flip. **This
document predicts, without having measured it, that `zombie-3-a`'s I4 status is
the one result the implementing agent should check first**, by computing
`D_total`, `lo`, `hi` and `tol_sum(D_total)` for that file under both the old
and new predicate.

**Row 7 (the breach-probe selection threshold) affects I5 only through
`seat_ground_status`, and `sphere` is I5's only clean pass in the corpus** (§1.1
table: `sphere` shows `pass` on I5, no other file does). Widening the breach
probe's candidate selection (§3.2) can only ever find *more* breaches than
before, never fewer, so this predicts the same direction of risk as row 14:
**`sphere`'s I5 pass is the second result to check first.**

**Rows 11, 16 (I4 clause a, I8 R1): predicted to change nothing, anywhere.**
§2.4 shows the admitted interval is four to five orders of magnitude below the
threshold each compares against. No file in a corpus built from real STL
geometry at millimetre scale is expected to have `A_unowned` or `A_owned` land
within `1e-9 mm²` of a `2.66e-4 mm²` censor. Fix them for correctness and
consistency with the rest of this document; do not expect a measured baseline
diff from them.

**Rows 17, 18 (report-only mirrors of row 8): no gating effect by definition**
(non-gating report), but their `flip_count` numbers should be recomputed after
the row-8 fix, since they exist specifically to disclose how sensitive I2 is to
the fill-rule choice, and today they measure that sensitivity against the wrong
(optimistic) predicate.

**I5, I6, I7, I8's exact/count-based comparisons and every equality-band
comparison: no predicted change**, because §3.1's rule has nothing to correct
there — they were already conservative or already symmetric before this
document.

**If this policy is applied and produces zero change anywhere in the corpus,
that is itself a finding**, per this task's own instruction: a policy that
changes nothing everywhere would mean either the optimistic rows identified in
§1 are not reachable by any file in the corpus (possible for rows 11 and 16,
predicted above; not plausible for rows 1–4, 7, 8, 10, 14 given how densely
populated I1/I2/I3's `failed`/`indeterminate` counts already are) or that the
fix was not actually applied to all ten OPTIMISTIC rows. The two flagged
candidates above (`zombie-3-a`/I4, `sphere`/I5) are the fastest way to tell
the difference.
