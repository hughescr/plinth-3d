# `collinear_arms` — three individually-adequate arms with no lateral equilibrium

## What is planted, and where

Three independent support chains (TIP -> TAPER -> MEMBER -> RAFT pad each),
synthesised directly with `plinth_audit::synth`, seated at `x = -5.151819576,
0, +5.151819576 mm`, **all at `y = 0`** — perfectly collinear along the
model's local X axis.

The model is a flat box, `x` in `[-7.727729364326279, 7.727729364326279]`,
`y` in `[-2.0591966482059565, 4.544964839793506]`, `z` in `[-2, 2]`. Its
bottom face (`z = -2`, outward normal `(0,0,-1)`) is the **only** down-facing
geometry, so `D_total = 102.070345 mm^2` exactly (two triangles, the
diagonal split `box_soup` always uses). The three contacts, all at `y = 0`,
sit `2.059196648 mm` from the box's near (`y`-negative) edge and
`4.544964840 mm` from its far edge — **not** centred: the box's down-facing
centroid sits at `y = 1.2428840957937748 mm`, `1.24 mm` off the one line the
three contacts define.

The Voronoi partition (`audit-invariants.md` §2.2: nearest contact,
Euclidean, no reach limit) of three points that share one `y`-coordinate
splits the rectangle into three vertical strips at the `x`-midpoints,
**independent of `y`** — so each contact owns exactly
`a_owned = D_total / 3 = 34.023448438 mm^2`, regardless of where the box's
own centroid sits. Each tip is sized by the same inversion `double_load`
uses, against its own share:

```
A_implied = x_allowed * pi * r_tip^2 = a_owned   =>   r_tip = 0.95 mm
```

so **every one of the three arms individually satisfies the per-arm area
law** — `X = a_owned / (pi * r_tip^2) = 12.000` at the tip, `5.333` at the
taper/member cross-section (`arm_r = 1.5 * r_tip = 1.425 mm`), and `8.950`
at the raft pad's own fixed `2.2 mm` diameter (the narrowest point on any
chain, still comfortably under `X_ALLOWED = 12`). Ball radius
`r_ball = 1.5 * r_tip = 1.425 mm`; embedment `h_delivered = 0.471728024 mm`,
`30%` over `h_req = ball_seat_factor(1.5) * r_tip = 0.362867711 mm`.

| record | kind | contact | value |
|---|---|---|---|
| 0 | TIP | 0 | `x0 = -5.151819576`, `r1=r2=1.425 mm` |
| 1 | TAPER | — | narrow `r=0.95 mm` (=`r_tip`), wide `r=1.425 mm`, declared depth `0.471728024 mm` |
| 2 | MEMBER | — | constant `r=1.425 mm` |
| 3 | RAFT pad | — | `2.2 mm` diameter, `0.2 mm` height, on the plate |
| 4-7 | same chain | 4 | `x0 = 0.0` |
| 8-11 | same chain | 8 | `x0 = +5.151819576` |

`translate_mm = (0, 0, 5.03)`, matching `double_load`'s own choice of a
non-multiple-of-`layer_height` offset (see that fixture's README) so the
model's entry plane does not land on a raster layer boundary.

`generate.rs` in this directory is the exact, re-runnable, standalone record
of how the bytes were produced (same convention as `double_load` /
`starved_patch`: compiled ad hoc against the built `plinth_audit` rlib,
never wired into the crate's own `Cargo.toml`). Every margin and radius in
it is **derived from a formula and printed at generation time**, not a
guessed literal — see "Construction notes" below for why that mattered.

## Why this is a defect in physical terms

`docs/physics.md` §2, finding (c), verbatim:

> **`X <= 12` is necessary, not sufficient.** The ratio is blind to
> arrangement. Three collinear arms have no lateral stability perpendicular
> to their line, at any X. An arrangement check has to include a spread
> criterion, not just a per-arm ratio.

This fixture is that sentence made into bytes. Every individual arm passes
the strength condition the file's own fields declare (`X <= 12` everywhere
along its length, computed above). But three tension-only members whose
lines of action all lie on one straight line can produce a moment about
that line **only from their own bending** (`docs/specs/audit-spread-criterion.md`
§1.2) — no combination of member tensions, however large, resists a load
that acts off that line. The load here is not incidental: this fixture
deliberately gives the box a real, physical 2-D footprint (a real mini's
underside is never a 1-D strip) whose own down-facing centroid does not sit
on the line the three arms hold. Three individually well-sized supports
holding a real 2-D patch, arranged so the patch's own weight acts off their
line, is a real, physically unstable arrangement no per-arm ratio can see
— which is exactly the gap Finding 2 of the cross-family review named.

## What the audit says about it (measured, current source tree, 2026-08-30)

Built and run against the current, live `tools/plinth-audit` checkout with
`RUSTC_WRAPPER="" cargo build --release`, then
`./target/release/plinth-audit fixtures/collinear_arms/collinear_arms.chitubox --json --out-dir <dir>`.

**`i10_arrangement_spread` fails, for the planted reason, naming the
planted number:**

```
SPREAD_COLLINEAR_NO_EQUILIBRIUM file=.../collinear_arms.chitubox model=0 body=0 layer=101
  z_mm=-1.955000 centroid=0.000000,1.244500 s_min_mm=0.000000 sigma_mm=0.096663
  axis_deg=0.000000 offset_mm=1.244500 contacts=3 contact_ids=0,4,8
```

`measured.headline`:

```
1 bodies, 80 body-layer evaluations: 0 pass, 80 refused, 0 indeterminate;
worst containment margin -1.2445 mm at layer 101
```

Read against the outcome table of `audit-spread-criterion.md` §3.4, this is
row 2 (`dim(aff) <= 1` and `dist(q, aff) > sigma` -> REFUSE,
`SPREAD_COLLINEAR_NO_EQUILIBRIUM`), exactly and only:

- `s_min_mm = 0` — the RMS perpendicular spread of the three lines of
  action about their own best-fit line is **exactly zero**: the three
  contacts are not merely close to collinear, they are bit-for-bit on one
  line (all `y = 0`, straight vertical axes), so `dim(aff) = 1` (`aff_dim`
  in the raw JSON) with no floating slack at all.
- `axis_deg = 0` — that line is the model's local X axis, matching the
  plant.
- `offset_mm = 1.2445` — the load centroid's distance from that line,
  matching the designed `1.2428840957937748 mm` eccentricity to within
  `0.0016 mm`, itself inside the check's own disclosed raster-erosion term
  `sigma_q_mm = 0.0849` (`measured.sensitivity`/per-failure `sigma_q_mm`)
  for the conservative fill rule of `audit-invariants.md` §1.6.
- `sigma_mm = 0.096663` — the check's own derived uncertainty band. The
  offset (`1.2445 mm`) is **13 times** `sigma`, nowhere near the
  indeterminate band of row 3 (`rank_deficient_load_on_axis`) — this is a
  clean, unambiguous refusal, not a marginal one.
- **All 80 of 80 body-layer evaluations fail**, `0` pass, `0`
  indeterminate. The box is one prefix-connected body for its entire
  80-layer height (nothing else to merge with, and its footprint never
  changes shape), so its arrangement question is asked at every layer and
  answered the same way every time: refuse.

This is the check's REAL, current implementation (not the `NOT_IMPLEMENTED`
stub) — see "A note on timing" below.

## Isolation — what else fires, and why

Per the harness's own convention (`tests/fixtures.rs::collateral_failures`),
measured on the SAME run:

```
i1_seat_depth          pass  (checked=3,  failed=0)
i2_contact_area        pass  (checked=3,  failed=0)
i3_min_section         pass  (checked=3,  failed=0)
i4_load_ownership      FAIL  (checked=1,  failed=0, indeterminate=1)
i5_closed_load_path    pass  (checked=12, failed=0)
i6_no_intersection     pass  (checked=12, failed=0)
i7_island_coverage     pass  (checked=1,  failed=0)
i8_no_dead_geometry    FAIL  (checked=12, failed=6)
i9_unsupported_span    FAIL  (checked=1,  failed=1)
i10_arrangement_spread FAIL  (checked=80, failed=80)   <- the target
```

Three collateral failures, each investigated rather than waved through.

### `i4_load_ownership` — collateral, and NOT a property of this plant

```
LOAD_BOOKS_DISAGREE d_total_mm2=102.07034493646529 implied_lo_mm2=102.07033848412429
  implied_hi_mm2=102.07033848412429 clamped_contacts=0 ratio=1.0000000632146526
LOAD_OWNERSHIP_INDETERMINATE reason=partition_identity_violated
  accounted_mm2=102.07034493810173 d_total_mm2=102.07034493646529
  residual_mm2=1.6364e-9 tolerance_mm2=1e-9
```

The tips here were sized so `A_implied` should equal `a_owned` **exactly**
— there is no doubling, no starving, nothing planted in this clause at all.
The residual is `1.6e-9 mm^2` against a tolerance of `1e-9 mm^2`
(`c::tol_sum`, `audit-invariants.md` §1.5): the box's own corner
coordinates are exact in `f32` (nice round numbers), but `r_tip` is stored
as `f32` too, and squaring a `~7`-significant-digit `f32` and multiplying by
`12*pi` carries a relative error of order `2*eps32 ~ 1.2e-7` — an
**absolute** error of order `34 * 1.2e-7 ~ 4e-6 mm^2` per contact, three
contacts, landing at `1.6e-9` after cancellation. That is three orders of
magnitude tighter than the intended "double-booking" signal `double_load`
plants (`ratio = 0.5`) but still four times over the `1e-9 mm^2` absolute
floor `d_books_bracket` and `b_partition_identity` enforce.

**This is not unique to this fixture.** It is a structural property of
building ANY synth fixture with real support geometry at this tolerance:
every one of the six pre-existing sibling fixtures shows the identical
collateral failure against the CURRENT tree (re-measured here, not merely
quoted from their own READMEs) —

```
bare_island            i4_load_ownership: fail
broken_path_cycle      i4_load_ownership: fail
broken_path_free_end   i4_load_ownership: fail
buried_arm             i4_load_ownership: fail
shallow_seat           i4_load_ownership: fail
starved_patch          i4_load_ownership: fail
```

— and even `double_load`, whose OWN intended defect is a `LOAD_BOOKS_DISAGREE`
at `ratio ~ 0.5`, shows the same `partition_identity_violated` /
`component_routing_failed` companions at the identical `~1e-9` residual
scale, riding along with its real `ratio=0.5` finding. Finding 3 of the
cross-family review is explicit that "anything not established is a hard
failure," and `d_books_bracket`'s `1e-9 mm^2` absolute floor is tighter than
`f32` tip-radius storage can generally satisfy for a hand-built fixture
whose box dimensions were not co-derived from the same rounding — so this
collateral is semantically connected to the review's OWN finding 3 (the
tolerance-direction fix), not to anything planted here.

### `i8_no_dead_geometry` — collateral, and a known, pre-existing gap

```
GEOM_DEAD_MEMBER record=1  type=taper   (chain at x=-5.15)
GEOM_DEAD_MEMBER record=2  type=member  (chain at x=-5.15)
GEOM_DEAD_MEMBER record=5  type=taper   (chain at x=0)
GEOM_DEAD_MEMBER record=6  type=member  (chain at x=0)
GEOM_DEAD_MEMBER record=9  type=taper   (chain at x=+5.15)
GEOM_DEAD_MEMBER record=10 type=member  (chain at x=+5.15)
```

Six failures, exactly the taper and member of each of the three straight
chains (never the tip, never the raft). This is `double_load`'s own
documented, pre-existing gap in `dead_geometry.rs`'s liveness role model
(that fixture's README, "Known non-target failure"), re-confirmed here
directly against the CURRENT tree rather than trusted from the older
README: `double_load` itself, re-run just now, shows the identical
four-failure pattern (its two chains' own tapers and members, records
1/2/5/6) on the code as it stands today. The mechanism (from that README,
independently reproduced): a properly embedded TIP is treated as its own
valid "ground" node, so removing the arm beneath it never disconnects it
from ground under the check's own reachability test — a plain straight arm
segment directly under an already-embedded tip has no role in that check's
`R1`-`R4` set that can ever mark it live. `src/checks/dead_geometry.rs` is
not owned by this directory, and this pattern is orthogonal to the
collinearity plant: it reproduces identically on a chain by itself, with no
arrangement question involved at all.

### `i9_unsupported_span` — collateral, and semantically connected to the plant

```
SPAN_UNSUPPORTED layer=101 z_mm=-1.955 z_above_plate_mm=5.075 reach_mm=4.269539
  allowed_mm=1.344976 at_mm=-2.583,4.5315 nearest_carrier_mm=-4.879,1.0165
  carrier_kind=support region_area_mm2=101.716804
```

This one is NOT incidental, and the reasoning is worth stating precisely
rather than waved through. `i9_unsupported_span` (Finding 1's own criterion)
and `i10_arrangement_spread` (Finding 2's) are documented as asking
different questions of the same kind of object and deliberately not
overlapping (`audit-spread-criterion.md` §2.3: "arrangement is about a body
that can ROTATE, span is about a body that can SAG... two criteria, two
objects, no overlap") — but "no overlap in what they gate" does not mean a
single geometry cannot trip both, and this one structurally must:

- Building a fixture that isolates `i10` requires the three contacts to be
  genuinely, bit-identically collinear (`s_min_mm = 0` above), which forces
  the model's down-facing footprint to be genuinely two-dimensional and
  bigger than the contacts' own line — a strip carried by three points on a
  line, with no extent off that line, would not exercise `i10` at all
  (there would be no load off the line to refuse against). So the box MUST
  extend well away from `y=0` in both directions for this fixture to exist.
- That same footprint size is bounded FROM BELOW by an unrelated
  constraint: `seat.rs`'s own `mean_normal` cross-check (see "Construction
  notes" below) requires each contact to sit at least
  `sqrt(h_delivered*(2*r_ball - h_delivered)) ~ 1.06 mm` (plus a working
  buffer) from every box edge, or the box's own side walls get folded into
  the entry-normal comparison and every contact reports
  `SEAT_DEPTH_INDETERMINATE reason=normal_ambiguous` instead of a clean
  seat — which would make `i1`, `i2`, `i5` and `i8` all indeterminate
  collaterally, a strictly worse outcome (see "Construction notes").
- Combining those two lower bounds (three contacts at least `~4 mm` apart
  in `x`, a box at least `~4-6 mm` deep in `y`) already produces a
  footprint whose far corner is several mm from the nearest of only three
  point contacts — comfortably past `i9`'s measured allowance of
  `1.34 mm` at this box's height above the plate. Shrinking the whole
  geometry by a common scale factor does not help: `a_owned` (and hence
  `r_tip`, `r_ball`, and the `normal_ambiguous` margin) all scale with the
  SAME factor as the box, while `i9`'s `allowed_mm` is an absolute quantity
  derived from a measured coupon result (`docs/specs/audit-span-criterion.md`
  §2.6, `L_SURVIVED_MAX_MM`), not a dimensionless ratio — so no uniform
  rescaling closes the gap.
- Physically, this is the same root cause wearing two faces: three sparse,
  collinear point supports under a real 2-D patch leave material far from
  any of them (finding 1's complaint) AND leave the patch's own peel load
  off the only line those supports can resist a moment about (finding 2's
  complaint). A print this thin on support really would have both defects,
  not just one.

I did not find a way to build a fixture that isolates `i10` alone without
either (a) accepting this `i9` collateral, or (b) shrinking `r_tip` toward
`TIP_RADIUS_MIN_ALLOWED_MM = 0.05 mm`, which risks retripping the
`load_ownership.rs` crash documented below (that check clamps at the LOW
end of the tip-radius range too) for no gain, since the geometry would
still need the same few-mm margins in absolute terms. I am reporting this
as a reasoned, unavoidable pairing rather than forcing it quiet.

## Construction notes: how the margins were derived (the `normal_ambiguous` trap)

A first attempt at this fixture placed the three contacts with only `1 mm`
of clearance to the box's near edge. Every one of the three contacts came
back:

```
SEAT_DEPTH_INDETERMINATE reason=normal_ambiguous
CONTACT_AREA_INDETERMINATE reason=seat_indeterminate / seat_reason=normal_ambiguous
PATH_INDETERMINATE reason=seat_indeterminate / detail=normal_ambiguous
GEOM_INDETERMINATE reason=ownership_indeterminate
```

Tracing it into `src/seat.rs` (`seat_for`'s `mean_normal` block): the seat's
own entry normal is cross-checked against the AREA-WEIGHTED MEAN normal of
every model triangle whose CLOSEST POINT lies within `r_ball` of the tip
ball's **centre** (not the entry point). The ball centre sits `r_ball`
below the entry point, i.e. at
`z = z_bottom_face + h_delivered - r_ball`, which is BELOW the box's
bottom face by `r_ball - h_delivered` whenever `r_ball > h_delivered` (true
here, and true for essentially any real contact). A box's own vertical side
walls therefore have their CLOSEST point to the ball centre at the wall's
own bottom edge, at 3-D distance

```
dist = sqrt(horiz_margin^2 + (r_ball - h_delivered)^2)
```

If `horiz_margin` (the plan-view distance from the contact to the nearest
box edge) is small enough that `dist <= r_ball`, the wall's horizontal
normal gets folded into the mean against the bottom face's vertical normal
— roughly `90 degrees` apart — and trips the check's own 5-degree gate.
Solving `dist > r_ball` gives the minimum safe margin used in
`generate.rs`:

```
horiz_margin_min = sqrt(h_delivered * (2*r_ball - h_delivered))
```

`generate.rs` computes this from the actual `r_tip`/`h_delivered` in force
(rather than a guessed literal), adds a `1 mm` working buffer, and derives
every other dimension (contact spacing, near/far `y` margins, the
eccentricity) from it and from `a_owned`, printing every intermediate value
at generation time — see the `eprintln!`s in that file for the exact,
re-derivable numbers this fixture ships with (`margin = 2.059196648 mm`,
achieved `half_x_margin = 2.575909788 mm`, `f = 2.059196648 mm`, `b =
4.544964840 mm`).

## A note on timing: `i10` was a stub when this fixture was started

`src/checks/arrangement_spread.rs` was, at the start of this task, the
registered stub the task brief describes: `id() = "i10_arrangement_spread"`,
`run()` returning `CheckOutcome::not_implemented` unconditionally. This
fixture was built to that contract regardless — geometry and expectation
written as if the check were real — because the task's own acceptance
condition anticipates exactly this ("if the target gate does not exist
yet... that is a real finding: report it with evidence").

Partway through this run, `src/checks/arrangement_spread.rs` and
`src/checks/unsupported_span.rs` were landed as full implementations by a
parallel lane of this same workflow (both files went from a ~160-line stub
to ~2,000+ lines; `git status` shows them `??` — new, uncommitted, in the
shared checkout this fixture also builds against). All numbers in this
README are measured against that now-real implementation, rebuilt from
source (`RUSTC_WRAPPER="" cargo build --release`) immediately before the
final measurement pass, not against the stub. **Both acceptance conditions
are therefore met against the real check, not merely against its
not-implemented placeholder**, which is a strictly stronger result than the
brief required.

For the record, against the ORIGINAL stub (verified before the real
implementation landed, by temporarily pointing the harness-style prober at
this same `.chitubox`): `i10_arrangement_spread` failed with
`AUDIT_CHECK_NOT_IMPLEMENTED`, `checked == indeterminate == 80`, which the
real harness (`tests/fixtures.rs::run_fixture`) would have reported as
`PENDING` rather than `Satisfied` — the code path this fixture's
`.expect.json` was written to survive either way, since
`AUDIT_CHECK_NOT_IMPLEMENTED` is refused as an expected `code` outright.

## Harness discovery — verified directly

`tools/plinth-audit/tests/fixtures.rs`'s own `every_fixture_trips_the_check_it_names`
currently cannot complete a full run — see "Finding: a crash bug in
`load_ownership.rs`" below, which is triggered by OTHER, unrelated sibling
fixtures now in the tree and takes the whole single-loop test down before
it finishes, regardless of this fixture. So discovery and satisfaction for
`collinear_arms` specifically were verified with an out-of-tree probe
(`harness_probe.rs`, not part of this directory, built in scratch) that
reproduces `tests/fixtures.rs`'s `collect_expect_files` / `parse_expectation`
/ `run_fixture` **verbatim**, using only that file's own public API
(`plinth_audit::run`, `::json`, `::checks::GATING_IDS`,
`::check::NOT_IMPLEMENTED_CODE`, `::context::AuditOptions`), scoped to run
against `fixtures/collinear_arms/` alone:

```
discovered 1 expect.json file(s) under fixtures/collinear_arms:
  fixtures/collinear_arms/collinear_arms.expect.json
ok       collinear_arms [i10_arrangement_spread] ...
           also failing: i4_load_ownership, i8_no_dead_geometry, i9_unsupported_span
```

This confirms the two things the task's own repeated warning is about: the
file sits at `fixtures/collinear_arms/collinear_arms.chitubox` +
`fixtures/collinear_arms/collinear_arms.expect.json` — matching the layout
`collect_expect_files`'s recursive descent requires (a flat, non-recursive
scan finds nothing, which is the exact failure mode the task brief warns
about) — and it is not merely present but ACTUALLY EVALUATED and
SATISFIED by that logic, not just sitting in the right directory.

## Finding: a crash bug in `load_ownership.rs` (I4), found and worked around, not fixed

Not this fixture's defect, and not owned by this directory
(`src/checks/load_ownership.rs`), but discovered while building this
fixture and confirmed to affect the wider fixture suite, so reported here
with full reproduction.

**The bug.** `checks::load_ownership::LoadOwnership::run`:

```rust
let ratio = if lo > 0.0 { d_total / lo } else { f64::INFINITY };
...
measured.set("books_ratio", Json::num(ratio));   // unconditional, not gated on state_d
```

`lo` is the sum of `a_implied_mm2` over every UNCLAMPED contact
(`ownership::implied_area` clamps at both
`r_tip_mm <= TIP_RADIUS_MIN_ALLOWED_MM + tol` and
`r_tip_mm >= TIP_RADIUS_MAX_MM - tol`). Whenever a model has **zero**
unclamped contacts — either because it has no support contacts at all, or
because every contact's declared `r_tip` sits at or beyond a sizing clamp —
`lo` is `0.0`, `ratio` becomes `f64::INFINITY`, and the unconditional
`Json::num(ratio)` two lines above **panics the whole binary**:

```
thread 'main' panicked at src/json.rs:34:9:
non-finite value reached the JSON writer; report it as a finding, never as null
```

This is not a corner case invented for this report. It was hit three
separate times while building this task:

1. A first attempt at this fixture sized `r_tip = 1.0300645 mm`, `0.03 mm`
   over `TIP_RADIUS_MAX_MM = 1.000 mm` — clamped, all three contacts, `lo
   = 0`, reproduced the panic above verbatim. (Worked around here by
   choosing `r_tip = 0.95 mm`, safely under the clamp — see `generate.rs`'s
   own comment on this.)
2. `fixtures/bare_cavity_ceiling/bare_cavity_ceiling.chitubox` — a sibling
   fixture landed by a parallel lane of this same workflow (Finding 1's
   span criterion, presumably a body with no support at all) — panics
   identically when audited directly:
   `./target/release/plinth-audit fixtures/bare_cavity_ceiling/bare_cavity_ceiling.chitubox --json --out-dir <dir>`
   exits with the same `src/json.rs:34` panic.
3. `fixtures/long_ledge/long_ledge.chitubox` — another sibling fixture from
   the same parallel lane — panics identically under the same
   reproduction.

**Consequence for the shared harness.** Because
`every_fixture_trips_the_check_it_names` iterates every discovered fixture
in one `#[test]` function, either of these two sibling panics currently
takes the ENTIRE test down (`cargo test --release --test fixtures` reports
`FAILED` at that panic) before it can report a result for ANY fixture,
including the six pre-existing ones and this one. This is why this
fixture's own discovery/satisfaction had to be verified with the
out-of-tree probe above rather than by quoting a green `cargo test`.

I have not touched `src/checks/load_ownership.rs`, `bare_cavity_ceiling`,
or `long_ledge` — none of those are owned by this directory, and the
instructions are explicit that a defect discovered elsewhere is reported,
not silently fixed here.

## Secondary observation (not verified further, out of scope): a NaN laundered as a JSON string

Sanity-checking that `i10_arrangement_spread` is not a trivial
always-refuse check, I ran it against a real corpus artifact,
`exports/make_box-first-artifact.chitubox`. There it reports `checked=200`,
`failed=0`, `indeterminate=200` — every body-layer evaluation lands on
`reason=axis_horizontal` (a degenerate all-vertical support set for that
model), which is a real, distinct outcome from this fixture's `failed=80`
and is useful confirmation the check is not "fail everything unconditionally."
But one of those failure records contains:

```
"margin_mm": "nan",
"dist_to_affine_hull_mm": "nan",
"offset_mm": "nan"
```

— as JSON **strings** containing the literal text `"nan"`, not JSON numbers
and not `null`. `json.rs`'s own module doc states the numeric-field
schema rule plainly: "Use `Json::num_or_null` where an absent measurement
is the honest answer, and never to launder a `NaN`." Encoding a NaN as the
STRING `"nan"` is a third path around that rule — it avoids the
`Json::num` panic this report documents above, but it also breaks the
"every numeric field is a JSON number, never a string" schema rule
(`audit-invariants.md` §0), and it hides that this measurement was never
established behind what a naive JSON reader would parse as a normal
string field. I did not chase this further (`arrangement_spread.rs` is not
owned by this directory, and `docs/specs/audit-spread-criterion.md` §7 also
separately requires its own implementer to confirm at least one real
corpus artifact produces a non-zero PASS count for `i10`, which
`make_box-first-artifact.chitubox`'s `0 pass / 0 fail / 200 indeterminate`
result does not satisfy either) — reporting it here as observed evidence
for whoever owns that file next.
