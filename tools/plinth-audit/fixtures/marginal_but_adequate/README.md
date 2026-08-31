# `marginal_but_adequate` — a support design at its thinnest defensible margin

**KNOWN-GOOD (positive control). No defect is planted in this file.** It is
`docs/specs/audit-known-good.md` section 3.2's **KG-2** (`kg_slab_min_seat`),
built to the directory name this round assigned rather than that document's
own naming — the name fits: this is deliberately the file with the smallest
margin in that document's five-file set, and it is still genuinely adequate.

**Status of the ten checks, measured against the current tree (2026-08-31):**

| result | checks |
|---|---|
| **PASS** (8 of 10) | i1, i2, i4, i5, i6, i7, i9, i10 |
| **FAIL** (2 of 10) — measured tool findings, not fixture defects | i3, i8 |

Both failures trace to **one** root cause (below), independently corroborated
by two other fixtures in this same tree (`shallow_seat`, `four_leg_overhang`)
built by other agents this same round. If you read one section of this file,
read "The one thing this file is refused for" below.

---

## 1. Geometry

A 4.00 × 4.00 × 2.00 mm slab, floating 4.00 mm above the plate on four
identical posts at `(±1.00, ±1.00)` mm — the perpendicular-bisector centres
of the square, so the Voronoi partition of the underside (nearest contact,
Euclidean, `audit-invariants.md` §2.2) is **exactly** the four 2×2 mm
quadrants, by symmetry. Bounding-box-centred frame: slab `x,y ∈ [−2,2]`,
`z ∈ [−1,+1]`. No cavity, no recess, no overhang beyond the slab's own edge.

Support chain, four records per post (`audit-known-good.md` §2.2):

| # | kind (wire) | span | radii |
|---|---|---|---|
| 1 | TIP (7) | `p0 → p0 − 2R·ẑ` | `R, R` |
| 2 | TAPER (1) | ball centre → 0.50 mm below it | `r_tip → R` |
| 3 | MEMBER (3) | (taper end + 0.10 mm) → `z_plate + 0.90` | `R` constant |
| 4 | FLARE (4) | `z_plate + 1.00` → `z_plate` | `R → 0.90 mm` |

No raft pad (type 6). A flare landing on the plate is its own closed load
path (`audit-invariants.md` §3.5 clause (i)) and needs no pad; it also
sidesteps a documented, pre-existing tool defect — `synth::SynthRecord::
with_matching_soup` returns an *empty* soup for `RecordKind::RaftPadOrBridge`
by its own contract, which several sibling fixtures' READMEs independently
report tripping `GEOM_DEGENERATE reason=empty_soup`.

## 2. Arithmetic — derived by hand, from `docs/physics.md`, before this file was measured

```
A_face  = 16.0000 mm²           the slab's whole underside (4.00 × 4.00)
A_cell  =  4.0000 mm²           = A_face / 4, exact by symmetry
A_req   =  0.333333 mm²         = A_cell / X_ALLOWED, X_ALLOWED = 12 (physics.md §2:
                                   arm snaps at supported_area/arm_section ≈ 25,
                                   design point held back to half of it)
r_tip   =  0.325735 mm          = sqrt(A_cell / (12π)), 12π = 37.6991118431 —
                                   this is exact X = 12 sizing at the tip
R       =  0.488603 mm          = 1.5 · r_tip   (f = BALL_FACTOR_STUB = 1.5)
```

**β = 0.54** (KG-1 uses 0.60; this is the one number KG-2 changes):

```
h        =  0.175897 mm         = β · r_tip
h_req    =  0.124197 mm         = ball_seat_factor(1.5) · r_tip,
                                   ball_seat_factor(1.5) = 1.5 − sqrt(1.25) = 0.3819660113
margin   =  41.6 %               (h / h_req − 1) — genuinely tight, still over the minimum
a        =  0.375430 mm         = sqrt(2·1.5·r_tip·h − h²), the ideal cut-disc radius
A_ideal  =  0.442800 mm²        = π·a² = 1.33 × A_req
```

By `audit-invariants.md` §1.3/§2.1's own measured full-containment raster
erosion, the *cured* patch is smaller than the ideal disc; this file's own
measured erosion (§4 below) lands the delivered `X` at **10.7–10.9**, against
`X_ALLOWED = 12` — the tightest margin of anything this fixture passes.

**Every arm's own narrowest section** (the flared foot's neck, radius `R`) is
`π·R² = 2.25 × A_req`, more than twice what the area law needs — the binding
constraint here is the *seat*, never the arm.

**Span (I9).** Square lattice, pitch `s = 2.00 mm`. Worst point is a cell
corner, `s·√2/2 = 1.4142 mm` from its post's axis; the cured tip subtracts
`a`. Hand estimate (Euclidean): `1.4142 − 0.3754 = 1.0388 mm`, against the
derived allowance `L_SPAN_ALLOWED_MM = 1.344976 mm` (`audit-span-criterion.md`
§2.6) — **77 % of the limit**, comfortable.

**Spread (I10).** Four non-collinear posts at the corners of a 2.00 mm
square; the slab's own down-facing centroid sits at the square's centre —
maximal symmetry, the strongest arrangement in the known-good set (no
KG-2-specific number here: this geometry is byte-identical to KG-1's lattice,
only the seat depth changed).

## 3. For the owner

*A 4 mm square slab, 2 mm thick, on four posts — one under the middle of
each quarter of its underside, same layout as the simplest file in this set.
The difference here is how deep the ball is pushed in: 0.176 mm instead of
0.195 mm, about 10 % less. The circle each post's ball cuts is 0.75 mm
across; after the printer's own pixel grid takes its share, the contact
comes out to about 10.7–10.9 % more than the rule requires rather than
20–22 %. That is deliberately the thinnest margin I built this round —
narrow enough that if the rule is even slightly too strict, this is the file
that would show it, but wide enough that I would still expect it to hold on
the printer. **The furthest any point of the underside sits from a post is
1.04 mm hand-estimated (1.07 mm measured); the span allowance is 1.34 mm** —
that margin is not the tight one here, the seat depth is.*

*The audit refuses two of its ten checks on this file (i3, i8) — not because
anything is thin or badly placed, but because of a bookkeeping gap explained
below: the joints between the pieces of each post deliberately overlap by a
tenth of a millimetre (so two pieces are unambiguously fused rather than
merely touching), and two of the ten checks lose track of the post when it
does that, even though an eleventh check (which measures the exact same
question — is everything connected to the plate) has no trouble at all. I
did not shrink that overlap to make the two checks happy; doing so would
have meant undoing the thing the overlap exists to prove.*

## 4. Measured, read back from the audit's own JSON — not predictions

Built and audited against the live tree with
`RUSTC_WRAPPER="" cargo build --release`, then
`./target/release/plinth-audit fixtures/marginal_but_adequate/marginal_but_adequate.chitubox --json --out-dir <dir>`.

**Per-contact (I1/I2), all four:**

| contact | r_tip (mm) | R (mm) | h_delivered (mm) | h_required (mm) | margin | A_rast (mm²) | A_required (mm²) | X_delivered |
|---|---|---|---|---|---|---|---|---|
| 0 (−1,−1) | 0.32512411 | 0.48768619 | 0.17556703 | 0.12418637 | 41.4 % | 0.367878 | 0.333333 | **10.873** |
| 4 (+1,−1) | 0.32512411 | 0.48768619 | 0.17556703 | 0.12418637 | 41.4 % | 0.367878 | 0.333333 | **10.873** |
| 8 (−1,+1) | 0.32512411 | 0.48768619 | 0.17556703 | 0.12418637 | 41.4 % | 0.367878 | 0.333333 | **10.873** |
| 12 (+1,+1) | 0.32756081 | 0.49134123 | 0.17688286 | 0.12511710 | 41.4 % | 0.373464 | 0.333333 | **10.711** |

r_tip three-way agreement (R/f, taper's own `r_start`, fitted soup):
spread `1.48e-8 mm` (contacts 0/4/8), `9.93e-9 mm` (contact 12) — all
`agrees: true`, well inside the check's own tolerance (`5.4e-7 mm`).

Contact 12 differs from the other three by design (§6 below); I1/I2 grade it
independently and it clears the same margin the other three do (41.4 %
seat-depth margin both ways, `X_delivered` 10.7 vs 10.9 — both comfortably
under 12).

**I4 load ownership**, graded strict (no clamped tip):

```
d_total_mm2       = 16
implied_lo/hi_mm2 = 15.999999999999918   (bracket collapses to one point)
books_ratio       = 1.000000000000005
tolerance         = 1e-9 mm²
residual          = 8.2e-14 mm²           -- four orders inside the floor
```

**I5 closed load path**: 16 of 16 records — 4 grounded via seat (the tips),
4 grounded via plate (the flares), 8 "connected" up to one of those two
(the tapers and members); `overlap_edges: 16`, `free_ends: 0`,
`fusion_uncertain_pairs: 0`.

**I6 no intersection**: 16 of 16 records, 0 failures.

**I7 island coverage**: the slab's one down-facing surface component is a
supported island at its birth layer (`checked: 1`, `indeterminate: 0`).

**I9 unsupported span**: `0 of 1 down-face regions exceed the 1.344976 mm
allowance; worst reach 1.066437 mm at layer 80` — **79.3 %** of the
allowance under the audit's own (conservative) chamfer metric, a little
tighter than my 77 % Euclidean hand estimate, as expected (the chamfer
metric overestimates a diagonal path — `audit-span-criterion.md` §2.4 rule
R7).

**I10 arrangement spread**: `1 bodies, 40 body-layer evaluations, every one
in equilibrium by more than sigma`; normalised worst-per-body margin
`m_hat = 0.5` held constant across all 40 layers.

## 5. The one thing this file is refused for

**Finding KG-F-mba-1 — `i3_min_section` and `i8_no_dead_geometry` lose
connectivity across a joint that `i5_closed_load_path` establishes cleanly,
on the exact same 16 records.**

```
i3_min_section reports, all four contacts:
  MIN_SECTION_INDETERMINATE file=...marginal_but_adequate.chitubox model=0 contact=0 reason=path_open
  (path_records: [0] only — the walk never crosses the tip→taper joint)

i8_no_dead_geometry reports, all 12 non-tip records:
  GEOM_INDETERMINATE file=...marginal_but_adequate.chitubox model=0 record=1 reason=path_indeterminate
  (every TIP is "live", role R1; every TAPER/MEMBER/FLARE is "indeterminate")

i5_closed_load_path reports, on the SAME 16 records, in the SAME run:
  status=pass, checked=16, failed=0
  overlap_edges=16, free_ends=0, orphaned=0, fusion_uncertain_pairs=0
  grounded_via_seat=4, grounded_via_plate=4, connected_not_grounded=8
```

**Why the file is not the problem.** Every joint in this design deliberately
overlaps by ≥ 0.10 mm along the support axis — the member's start sits 0.10
mm *into* the taper's own span, and the flare's start sits 0.10 mm *into*
the member's own span (§1, rows 2–4). This is not casual: `audit-
invariants.md` §3.5 states that a separation in `[0, d3]`,
`d3 = 0.0552901438 mm` (the voxel diagonal), is **fusion-uncertain** — two
solids that merely *touch* are not established to cure into one piece during
printing, and the check refuses to decide. `docs/specs/audit-known-good.md`
§2.4 rule R1 exists precisely to stay clear of that band: "every joint
overlaps by ≥ 0.10 mm along the axis... two solids that merely touch are
not established to cure as one." **I followed that rule as specified, and it
is what makes i5 pass.**

Having built the file, I looked (not before) at why i3 and i8 disagree with
i5, restricted to the two files this project already designates as shared,
inspectable infrastructure: `tests/fixtures.rs` (the harness contract) and
`plinth_audit::synth`'s own public constants. `crate::consts::
JOIN_TOL_EMITTED_MM = 1.0e-3 mm` is documented there as "endpoint-coincidence
tolerance for our own emitted artifacts" — **two orders of magnitude
tighter** than the ≥ 0.10 mm overlap R1 requires. I did not open
`src/graph.rs`, `src/checks/narrowest_section.rs`, or
`src/checks/dead_geometry.rs` to confirm that this constant is what i3/i8's
own path-walk consumes — that is exactly the file I did not want to reverse-
engineer a fixture from. What I can say from the outside is only what is
measured: **i5's own path-following establishes the identical connectivity
question and gets a clean answer; i3 and i8, on the identical bytes, do
not.**

**This is not unique to this file, and I did not have to build a second
probe to show it — two other agents in this same session's parallel round
already did, independently:**

* `fixtures/shallow_seat/README.md` §2 measured the *same* i3/i5/i8
  disagreement on the project's own designated "clean" reference artifact
  (`tests/fixtures.rs::clean_artifact()`), for an analogous MEMBER→RAFT gap
  named in `docs/specs/chitubox-format.md` §5 (0.2 mm, also over the plain
  join tolerance), and named it in the same words I would: *"a disagreement
  between i5 and i8's two notions of grounded worth someone's attention."*
* `fixtures/four_leg_overhang/` (a KG-1-equivalent built by another agent
  this round) went further and built a **diagnostic probe**
  (`four_leg_overhang_probe_tight_joint_*`) that shrinks the same two
  overlaps from 0.10 mm to 0.0005 mm — *inside* `JOIN_TOL_EMITTED_MM` — on an
  otherwise byte-identical file. Measured result: **i3 then passes cleanly**,
  and **i8 resolves to a clean, definite `GEOM_DEAD_MEMBER`** (not
  indeterminate) on all 12 non-tip records — directly confirming
  `audit-known-good.md` §5.1's advance prediction that i8 cannot pass *any*
  well-formed multi-record support chain under the current liveness-role
  model. The same probe shows i5 then fails instead (`fusion_uncertain`,
  because 0.0005 mm sits inside the `[0, d3]` band R1 exists to avoid) — i.e.
  **the tight joint that satisfies i3/i8 is the one R1 says must not be
  built.** A file cannot simultaneously satisfy R1 and i3/i8's tighter
  reading of "joined."

**Classification (per `audit-known-good.md` §8):** this is either (b) a
modelling gap — i3/i8 read a plain-graph adjacency relation that does not
implement the same "overlap, not touch" standard `audit-invariants.md` §3.5
itself states — or (c) an instrument defect, if i3/i8 were intended to
consume the same connectivity i5 does and do not. I have not read enough of
`src/graph.rs` to tell those two apart, deliberately. **I did not touch this
file's overlaps to dodge either check.** Doing so would mean either
retreating into the fusion-uncertain band (a real defect) or asserting a
join distance this project's own physics does not support.

## 6. The one deliberate compromise, and its cost — I4's f32-exactness step

`i4_load_ownership` clause (d) is an equality: `Σ 12π·r_tip² = D_total`
within `max(4096·eps64·D_total, 1e-9) = 1e-9 mm²`. A single shared `r_tip`
for all four (symmetric) contacts has only one quantised f32 degree of
freedom; measured directly, that bottoms out at a residual of `5.09e-7 mm²`
— four orders too coarse.

**What I measured, that the design doc did not anticipate in this much
detail:** `i4_load_ownership` does not read the taper's own `r_start` field
for its `a_implied` computation — it reads `R` (the TIP's own ball radius)
and divides by the ball factor, in full `f64`, with no further rounding.
Sweeping the *taper's* `r_tip` field (a first attempt) tuned a number the
check does not consume, and measured `2.44e-7 mm²`, still four orders over.
Sweeping `R` itself — three contacts to a common `f32` value `R_a`, the
fourth solved in closed form against the exact remaining demand and rounded
once — over a ±100 000-ULP window (~±5.8 μm, far below the printer's own
14–19 μm pixel pitch) landed:

```
R_a (3 contacts) = 0.487686187 mm
R_n (4th contact) = 0.491341233 mm
difference        = 3.655e-3 mm  (122 643 ULP -- NOT invisible-in-ChituBox
                                   small; see caveat below)
achieved residual  = 8.17e-14 mm²   (measured; 1e-9 floor cleared by 4 orders)
```

`docs/specs/audit-known-good.md` §2.5 predicted this difference would land
"in about the seventh decimal... invisible in ChituBox." **Measured here: it
does not.** A first, wider sweep (±600 000 ULPs, breaking on the first
sub-`1e-12` hit) found an exact-sum candidate 1.37 **million** ULPs from
seed — `R_a = 0.474 mm`, `R_n = 0.530 mm` — which would have visibly
violated "every arm individually adequate" (§1 of the design doc). The
±100 000-ULP window used here keeps every individual contact within
`0.001 mm` of the hand-derived `R = 0.488603 mm` (both radii differ from the
design value by under 3 μm), so **every contact still individually clears
its own area law with margin** (§4's table: X_delivered 10.71–10.87, both
comfortably under 12) — but the fourth contact's `r_tip` is measurably
larger than the other three's by `0.0024 mm`, not by a seventh-decimal
amount. **Disclosed, not hidden**: this is a real cost of the f32-exactness
technique, worth the design doc's own author revisiting — a tighter window
trades exactness for individual-contact uniformity, and I chose the wider
tolerance on individual uniformity deliberately, over the tighter one on
"invisible" cosmetic difference.

## 7. What I did and did not open (`audit-known-good.md` §6, Trap 1)

Every dimension in §1–§2 is derived from `docs/physics.md` (the area law,
the scar invariant, the seating rule) and from `audit-invariants.md` /
`audit-span-criterion.md` for the measured printer profile (pitches, layer
height, tip-radius range, `L_SPAN_ALLOWED_MM`) — not from any check's
implementation.

**Opened, and why it's disclosed:** `tests/fixtures.rs` (the harness
contract this fixture must satisfy); `src/synth.rs`'s module docs and public
API (the byte-writer this file must use, including the documented
`RaftPadOrBridge` empty-soup gap); `src/reader.rs`'s `RecordKind` enum (wire
codes, needed to call `SynthRecord::new` correctly — a container-format fact,
not a check's judgement); `src/consts.rs` (every physics/format constant
this file cites, each with its own citation back to a measured source);
`docs/specs/chitubox-format.md` (the container, for the ChituBox-openability
splice, §8 below); one sibling fixture's `generate.rs`
(`fixtures/collinear_arms/generate.rs`, for the `step_f32` ULP-search
pattern and the sphere-soup pole-triangle workaround — a build technique,
used the same way that fixture's own README discloses); `fixtures/
shallow_seat/README.md` and (read-only, not modified) `fixtures/
trivial_cube/generate.rs` and `fixtures/four_leg_overhang/README.md` /
`spread_wide/README.md`, all written by other agents in this same round, for
the frame-formula and connectivity findings §5–§6 cite and corroborate.

**Deliberately not opened:** `src/checks/*.rs` — all ten checks, including
`narrowest_section.rs` and `dead_geometry.rs`, the two implicated in §5 —
and `src/graph.rs`, `src/seat.rs`, `src/ownership.rs`, `src/raster.rs`. No
threshold, margin, or geometric choice in this file was reverse-engineered
from a check. `JOIN_TOL_EMITTED_MM`'s value (§5) is public in `consts.rs`,
which is on the allowed list above; what it is *consumed by* is inferred
from measured behaviour, not read from source.

## 8. ChituBox-openability splice

`SynthFile::to_bytes()` (the shared `synth` writer) produces `settings_size
= 0`, `off8 = 24` — matching the eleven pre-existing (deliberately broken)
fixtures, not a real populated ChituBox project. Per
`docs/specs/audit-known-good.md` §7.2, `generate.rs` splices in a 396-byte
all-zero settings blob (`docs/specs/chitubox-format.md` §6: measured
all-zero even in populated specimens) and a preview blob copied verbatim
from `exports/sphere-first-artifact.chitubox` (a real emitted file locally
available, `off8 = 27416`, `settings_size = 420` — matches the format spec's
own reference numbers exactly), then shifts every absolute offset the
container stores (`off8`, `off20`, each model's `mesh_off` /
`support_index_ptr`, each support index's `records_off` / `mesh_off`, each
segment record's own `mesh_off`) by the inserted length. Verified by
re-parsing the header afterward (`off8 = 27416`, `settings_size = 420`,
`off20 = 28096 = off8 + 680`) and by the audit tool itself parsing the file
without any `FILE_PARSE_FAILED`/header-shape finding.

**Caveat, honestly flagged**: this environment has no way to launch ChituBox
itself, so "opens and shows the model and supports as separate objects" is
**not independently confirmed** here — only that the header/offset shape now
matches a real populated file's, byte-for-byte in the fields that matter.
This is the one item in `audit-known-good.md` §7.4's checklist I could not
close by measurement. `<name>-model.stl` / `<name>-supports.stl` (§7.2 item
4) were not generated either, for the same reason — ask if you want them
built as a fallback inspection path.

## 9. Byte identity

```
sha256: a6859d9e2963534d88e394d15e47726c8d878d739d4323365949ef41bbb2919a
bytes:  90000
```

Reproduce with: `cd tools/plinth-audit && RUSTC_WRAPPER="" cargo build --release
&& rustc --edition 2021 -O -L target/release/deps --extern
plinth_audit=target/release/libplinth_audit.rlib
fixtures/marginal_but_adequate/generate.rs -o /tmp/gen && cd
fixtures/marginal_but_adequate && /tmp/gen marginal_but_adequate.chitubox`
(the generator defaults its preview donor to
`../../../../exports/sphere-first-artifact.chitubox`, relative to this
directory; pass a second argument to override).

## 10. Harness discovery — verified two ways

**Real harness, full tree** (`cargo test --release --test fixtures
every_fixture_trips_the_check_it_names`, 2026-08-31, all 40 expectations
across every fixture directory in the tree): all 10 of this directory's
expectations reported `ok`, e.g. `ok marginal_but_adequate [i2_contact_area]
... also failing: i3_min_section, i8_no_dead_geometry` — exactly the state
this README claims, machine-checked. `discovery_reaches_every_fixture_
directory_on_disk` also reports this directory reached cleanly (`marginal_
but_adequate: 10 expectation file(s)`); the one directory that test
currently flags as unreached (`trivial_cube`) is a different, still-in-
-progress fixture, not owned by this directory. The suite's one other
failure is `assert_eq!(satisfied, 11)` at `tests/fixtures.rs:603` — a
hardcoded count now stale because multiple agents added fixtures this same
round (40 satisfied, not 11); that assertion is explicitly designed to be
"update this number DELIBERATELY," and doing so is `tests/fixtures.rs`'s
owner's call, not this directory's.

**Independent out-of-tree probe**, reproducing `collect_expect_files` /
`parse_expectation` / `run_fixture` verbatim against only this directory
(built for this fixture specifically, before the full-tree run above was
available):

```
discovered 10 expect.json file(s) under fixtures/marginal_but_adequate
  ok  marginal_but_adequate      [i2_contact_area]      check_passes
  ok  marginal_but_adequate_i1   [i1_seat_depth]         check_passes
  ok  marginal_but_adequate_i4   [i4_load_ownership]     check_passes
  ok  marginal_but_adequate_i5   [i5_closed_load_path]   check_passes
  ok  marginal_but_adequate_i6   [i6_no_intersection]    check_passes
  ok  marginal_but_adequate_i7   [i7_island_coverage]    check_passes
  ok  marginal_but_adequate_i9   [i9_unsupported_span]   check_passes
  ok  marginal_but_adequate_i10  [i10_arrangement_spread] check_passes
  ok  marginal_but_adequate_i3   [i3_min_section]        check_fails
  ok  marginal_but_adequate_i8   [i8_no_dead_geometry]   check_fails
10 satisfied, 0 pending, 0 failed
```

## 11. Layout note — one artifact, ten expectations

The harness binds one expectation to one artifact by filename stem
(`tests/fixtures.rs`'s own doc comment: "one pair per fixture, so parallel
agents adding fixtures never touch the same file"). A known-good file needs
*all ten* checks graded, and the current schema has no "all_checks_pass"
form (`audit-known-good.md` §7.1 names this as a harness change owed by
whoever owns `tests/fixtures.rs`, explicitly *not* this document's author or
the fixture builder). Rather than block on that change landing, or invent a
schema the harness cannot parse, this directory ships **ten stems**: the
real file (`marginal_but_adequate.chitubox`) plus nine same-content symlinks
(`marginal_but_adequate_i1.chitubox` … `_i10.chitubox`, skipping `_i2`
because the primary stem already carries `i2`), each with its own
`.expect.json` naming one check. This uses only the schema the harness
already supports, needs no change to any file outside this directory, and
was independently arrived at by two other agents' fixtures in this same
round (`four_leg_overhang`, `spread_wide`) — see §10's full-suite output.

## 12. Open questions this file surfaces (not this builder's to rule on)

* Should `i3_min_section` and `i8_no_dead_geometry` read the same
  connectivity `i5_closed_load_path` establishes, rather than a separate,
  tighter point-coincidence relation? (§5; corroborated by `shallow_seat`
  and `four_leg_overhang`.)
* `docs/specs/audit-known-good.md` §2.5's claim that the f32-exactness
  technique's cost lands "in about the seventh decimal" does not hold at a
  ±100 000-ULP search window on this file's numbers — it costs 122 643 ULPs
  (~2.4 μm on `r_tip`, still well under printer resolution, but not a
  seventh-decimal amount). Worth revisiting whether the design doc's
  intended window is wider than what individual-contact uniformity can
  tolerate (§6).
