# `four_leg_overhang` — the audit's first known-good fixture, and where it still refuses a good file

Built to `docs/specs/audit-known-good.md`'s KG-1 spec (§3.1, `kg_slab_four_post`), under
this directory's assigned name. **Measured 2026-08-31: 8 of the 10 gating checks pass,
non-vacuously, on a file whose correctness is derived below from `docs/physics.md`
alone.** The other two — I3 and I8 — fail, and this README explains exactly why, with a
controlled diagnostic probe confirming the mechanism rather than asserting it.

This is not a small result. Before this fixture, the tool had never once been asked to
say yes: all eleven existing fixtures are deliberately broken files, and the corpus
headline "0 of 31 passes all ten checks" was consistent with either "the corpus is bad"
or "a check refuses everything," with nothing in the repository to tell the two apart.
This file is that instrument.

---

## For the owner

*A 4 mm square slab, 2 mm thick, held 4 mm off the plate on four posts — one under the
middle of each quarter of its underside. Each post carries a 2 mm × 2 mm patch of
underside, 4 mm². The measured rule says an arm must be at least a twelfth of what it
carries, so 4 ÷ 12 = 0.333 mm² at its thinnest point. The ball on top of each post is
buried about 0.195 mm into the slab, which cuts a circle about 0.78 mm across —
0.480 mm², about 1.44 times what is required. I measured what the printer's own pixel
grid actually does to that circle (not estimated it): it comes out to 0.406 mm², still
1.22 times what's required, and the tool's own headline number for this file agrees —
worst delivered load ratio 9.86 against the 12 allowance, 18% of margin still in hand.
The furthest any point of the underside sits from a post is about 1.05 mm; the measured
span allowance is 1.34 mm, so there's room there too. The four posts sit exactly under
the centre of their own quarter of the slab, so the load is centred over them with
nothing lopsided about it.*

*The tool agrees with all of that — eight of its ten checks say yes, cleanly, having
actually looked. The other two (I3, "narrowest arm section," and I8, "no dead
geometry") say they can't tell, and I traced why: they use an older, stricter rule for
"is this the same joint" than the rule the tool's own I5 check uses, and my joints
(built the way the tool's own spec says a joint should be built, with a real 0.10 mm
overlap so two pieces are certainly the same solid, not just touching) fall outside
that older rule. I proved this by building a second file, identical except the joint
overlap is shrunk from 0.10 mm to 0.0005 mm: I3 then passes, but the same shrink breaks
I5 instead, and trips I8 into a clean, predictable failure the design document
predicted before I built anything. I did not touch the shipped file to make that
happen — the shipped file keeps the physically-motivated 0.10 mm overlap, and reports
what that costs it. My read is that this is a genuine, fixable inconsistency inside the
tool between two ways of deciding "is this joint the same solid," not a reason to doubt
the file. But it's your call, and I'd want you to look at the file yourself before
trusting mine.*

*One other thing I fixed before you saw this: my first build put the flare feet
0.20 mm apart at the plate, under the 0.30 mm gap you told us prints reliably keep open
— nothing in the tool checks that number at all, so nothing would have caught it. I
widened the gap to 0.70 mm by hand. Please look at the feet in the STL or ChituBox and
tell me if 0.65 mm radius feet, about 1.3 mm across, look reasonable to you, because
nothing else here has an opinion on plate-foot size (that's flagged open question U2 in
the design doc).*

---

## 1. Geometry

A 4.00 × 4.00 × 2.00 mm slab, underside 4.00 mm above the plate, on four identical
posts at `(x, y) = (±1.00, ±1.00)` mm. Bounding-box-centred in X/Y; in Z the slab
underside sits at **local `z = 0`** (not `z = −1`; see "What I got wrong," §5.1, for
why that matters). Slab spans `x, y ∈ [−2, 2]`, `z ∈ [0, 2]`.

Each post is a four-record chain, straight down (`u = (0, 0, −1)`):

| # | wire type | span | radius |
|---|---|---|---|
| TIP (`ContactSphere`, 7) | ball, two poles | `p0 → p0 − 2R·(0,0,1)` | `R` both ends |
| TAPER (`ContactTaper`, 1) | frustum | ball centre `→` node, 0.50 mm | `r_tip → R` |
| MEMBER (`Member`, 3) | frustum | node+0.10 (into taper) `→ z_plate+0.90` | `R` constant |
| FLARE (`RootFlare`, 4) | frustum | `z_plate+1.00` (into member) `→ z_plate` | `R → 0.65 mm` |

`p0 = (x, y, h)` is the ball's deepest pole, `h` mm above the underside. Four posts ×
four records = 16 support records, 0 raft/pad records (§2.2 of the design doc: a flare
landing on the plate is grounded on its own; no pad avoids a documented fixture-tool
defect in `RaftPadOrBridge` soup generation).

---

## 2. Arithmetic, derived from `docs/physics.md` alone — recomputed for these exact bytes

**What I deliberately did not open** while designing this file: `src/checks/*.rs` (all
ten checks), `src/seat.rs`, `src/ownership.rs`, `src/raster.rs`. Every number below
comes from `docs/physics.md` (the area law, the scar invariant, hexagonal-vs-square
placement) and from the two documents carrying the measured printer profile and coupon
results (`docs/specs/audit-invariants.md` for the tip-radius range, layer height and
derived tolerances; `docs/specs/audit-span-criterion.md` §2.6 for
`L_SPAN_ALLOWED_MM = 1.3449762463 mm`). **I only opened check source code AFTER the
file was built and failing, to diagnose why — never to choose a threshold, margin or
geometric value.** §4 says exactly which files and why.

### 2.1 The load law and the sizing procedure

```
A_face  = 4.00 x 4.00                     = 16.0000 mm^2   (the whole underside)
N       = 4 posts, each at the perpendicular-bisector centre of its own quadrant
A_cell  = A_face / N                      =  4.0000 mm^2   exact, by symmetry
A_req   = A_cell / X_ALLOWED (=12)        =  0.333333 mm^2
r_tip   = sqrt(A_cell / (12*pi))          =  0.325735 mm   (ideal, before f32 rounding)
R       = 1.5 * r_tip                     =  0.488603 mm   (ball radius = arm radius)
h       = 0.60 * r_tip                    =  0.195441 mm   (beta = 0.60, KG-1's value)
a       = sqrt(2*R*h - h^2) = 1.2*r_tip   =  0.390882 mm   (ideal cut-disc radius)
A_ideal = pi * a^2 = 1.44 * A_req         =  0.480000 mm^2
```

`h = 0.6 r_tip` is the design's chosen margin above the ruled minimum
`h_required = max(0.100, 0.050 + r_tip*sin(theta), ball_seat_factor(1.5)*r_tip)`. At
normal incidence (`sin(theta)=0`) and this `r_tip`, `h_required = 0.381966*0.325735 =
0.124407 mm` — `h` clears it by 57%.

### 2.2 f32 exactness (R6) and the fourth tip (audit-known-good.md §2.5)

`I1` triangulates `r_tip` three ways and `I4` clause (d) demands
`Sum(12*pi*r_tip_i^2) == D_total` to `max(4096*eps64*D_total/12, 1e-9) mm^2`. Both
require the *stored f32* radii, not the ideal real number, to satisfy `R = 1.5*r_tip`
exactly and to sum to `D_total = 16.00 mm^2` almost exactly. I searched f32 candidates
near the ideal `r_tip` for one satisfying `1.5*r_tip` round-tripping through f32 with
zero loss (**R6**), then swept outward from that candidate (closest-first, so the
deviation stays minimal) solving the fourth tip's radius to close the residual, exactly
as the design doc's §2.5 describes:

```
r_a (3 posts)  = 0.3257685304 mm   R = 0.4886527956 mm   h_delivered = 0.1954611242 mm
r_4 (1 post)   = 0.3256344199 mm   R = 0.4884516299 mm   h_delivered = 0.1953806579 mm
r_4 - r_a      = -0.0001341105 mm  (4th significant decimal -- more than the design
                                     doc's own "about the seventh decimal" estimate;
                                     disclosed, not hidden, see §4.3)
achieved I4(d) residual = 2.662688e-10 mm^2   against the 1e-9 mm^2 floor
```

I computed this residual twice, independently: once inside the generator's own sweep
(the number above), and once by reading it back from the audit's own JSON output
(`i4_load_ownership.measured`: `d_total_mm2=16`, `implied_lo_mm2=implied_hi_mm2=
15.999999999733731`, so `|16 - 15.999999999733731| = 2.66e-10`). **The two agree to the
displayed precision.** The audit's own `books_ratio` is `1.0000000000166418`.

### 2.3 The clearance and edge rules (R2, R4, R5)

```
R2  axis-to-slab-wall  = 2.00 - 1.00 = 1.00 mm   / R (0.4887 mm)      = 2.05x margin
R4  axis-to-edge       = 1.00 mm  / a (0.3909 mm)                    = 2.56x margin
R5  adjacent-post gap  = 2.00 - 2*R = 1.02 mm true gap along the arm's own length,
                         >> FINAL_CLEARANCE_GAP_MM = 0.300 mm
```

**R5 at the FEET is the one I got wrong on the first pass and had to fix by hand — see
§5.2.** Nothing in the ten audited checks polices `FINAL_CLEARANCE_GAP_MM` at all (a
repo-wide `grep -rn FINAL_CLEARANCE_GAP_MM src/` inside `tools/plinth-audit/` returns
nothing), so this margin is disclosed here for the owner's own inspection, not asserted
by any expectation file.

### 2.4 Predicted vs. measured — the honesty table §7.4 requires

Every `A_rast` prediction below used the numerical erosion-model cross-check
`docs/specs/audit-known-good.md` §3.0 describes (full-containment pixel count at 16
sub-pixel phases, worst phase quoted), which that document validated against
`starved_patch`'s own measured value to 1.2–3.7%, in the conservative direction.

| quantity | predicted (hand, before build) | measured (audit's own JSON) | agreement |
|---|---|---|---|
| `X_delivered` (posts 0–2) | <= 9.86 | 9.854255560263976 | matches |
| `X_delivered` (post 3) | <= 9.86 | 9.86071736718874 | matches |
| `A_rast` (posts 0–2) | >= ~0.4056 mm^2 (1.22x A_req) | 0.4059159999999886 mm^2 (1.218x) | 0.07% |
| `A_rast` (post 3) | >= ~0.4053 mm^2 | 0.40564999999998863 mm^2 (1.217x) | ~0.1% |
| worst reach, euclidean hand calc | 1.4142 - 0.3909 = **1.0233 mm** (76.1% of allowance) | I9's own chamfer-metric reach: **1.0516347451388866 mm** (78.2%) | 2.8% higher, as expected (chamfer overestimates a diagonal path, per `audit-span-criterion.md` §2.4 rule R7) |
| I10 hull margin `d` | **1.000 mm** exactly (hull is the 2.00mm square of the four axes, centroid at its centre) | not printed at pass (only on failure); `m_hat = d/sqrt(hull_area) = 1.000/2.000 = 0.500` reported, matching | matches |
| I4(d) residual | (not predicted numerically in advance; the *procedure* was, §2.2) | 2.662688e-10 mm^2 | — |

No prediction here missed by more than the stated tolerance.

---

## 3. What the audit measured — all ten checks, run 2026-08-31

```
sha256   93efd69a24f0005c94ad7195eff5695fefee26c01b18ef243affc2a18f29383d
bytes    58396
verdict  gating_passed 8 / 10, support_invariants_pass FALSE (I3, I8 gate it false)
```

| check | status | checked | indeterminate | failed | headline |
|---|---|---|---|---|---|
| I1 seat depth | **pass** | 4 | 0 | 0 | all 4 contacts, ball term binds, 57% margin over `h_required` |
| I2 contact area | **pass** | 4 | 0 | 0 | worst `X_delivered` 9.86 against 12; median raster loss 15.45% |
| I3 min section | **fail** | 4 | 4 | 0 | `MIN_SECTION_INDETERMINATE reason=path_open`, all 4 — see §4 |
| I4 load ownership | **pass** | 1 | 0 | 0 | `D_total=16.00mm^2` exact, books ratio 1.0000000000166 |
| I5 closed load path | **pass** | 16 | 0 | 0 | 0 free ends, 0 fusion-uncertain, 4 grounded via seat, 12 via plate |
| I6 no intersection | **pass** | 16 | 0 | 0 | 0 failures over 16 records |
| I7 island coverage | **pass** | 1 | 0 | 0 | the one island (the whole underside) is supported at its birth layer |
| I8 no dead geometry | **fail** | 16 | 12 | 0 | `GEOM_INDETERMINATE reason=path_indeterminate`, all 12 non-tip records — see §4 |
| I9 unsupported span | **pass** | 1 | 0 | 0 | worst reach 1.0516 mm (78.2%) of the 1.3450 mm allowance |
| I10 arrangement spread | **pass** | 40 | 0 | 0 | 40/40 body-layer evaluations in equilibrium beyond sigma |

**8 of 10 gating checks pass, non-vacuously** (every one above has `checked > 0` and,
where it passes, `indeterminate = 0`) — the coverage-matrix requirement
`docs/specs/audit-known-good.md` §4 states. Raw JSON for every run quoted in this
README is reproducible with `plinth-audit four_leg_overhang.chitubox --json`.

---

## 4. The I3/I8 finding — the reason this file does not reach 10 of 10

**This is the finding this round exists to produce, and it survived the file being
correct: I did not adjust the geometry to make I3 or I8 pass.**

### 4.1 What's measured

I3 (`src/checks/narrowest_section.rs`) walks from each contact's TIP record rootward
through `ctx.graph` (`src/graph.rs`'s `SupportGraph`) looking for a path to ground. On
this file, `dist[tip]` is `None` for all four contacts — the walk cannot even start —
so I3 reports `MIN_SECTION_INDETERMINATE reason=path_open` on every contact and never
measures a single cross-section. I8 (`src/checks/dead_geometry.rs`) walks the *same*
`ctx.graph` for the same purpose and, for the identical reason, reports
`GEOM_INDETERMINATE reason=path_indeterminate` on all 12 non-tip records (every TAPER,
MEMBER and FLARE).

### 4.2 Why — read from the graph reconstruction, after the failure, not before

`SupportGraph::build` (`src/graph.rs`) links two records only when their declared
connective points coincide within `JOIN_TOL_EMITTED_MM = 1e-3 mm`
(`src/graph.rs` module docs: "Rebuilt from endpoint coincidence"). My TAPER→MEMBER and
MEMBER→FLARE joints are built with a deliberate **0.10 mm** axial overlap — R1 in
`docs/specs/audit-known-good.md` §2.4, itself grounded in `audit-invariants.md` §3.5's
physics: a separation in `[0, d3]` (`d3 = 0.0552901438 mm`) is *fusion-uncertain*, and
"two solids that merely touch are not established to cure as one." **0.10 mm overlap is
100x larger than I3/I8's 1e-3 mm coincidence tolerance**, so their shared graph never
links these joints at all — even though the two solids genuinely, provably overlap.

I5 (`src/checks/load_path.rs`) does **not** use `ctx.graph` for this. It independently
rebuilds its own graph from true solid-to-solid separation (`overlap_gap_mm`), and its
own module docs say why: *"only the overlap graph is literally what the specification
asks for"* (line 13). That is the graph that correctly reports my 0.10 mm joints as
connected — which is exactly why I5 passes cleanly (§3) on the same bytes I3 and I8
cannot walk at all.

**So: two of the audit's ten checks (I3, I8) reconstruct connectivity one way; the
third (I5), built later and explicitly because the first way was wrong for this
purpose, reconstructs it another way. A file built the way the specification's own
physics says a joint should be built is invisible to the older reconstruction and
visible to the newer, correct one.**

### 4.3 The confirming experiment — `four_leg_overhang_probe_tight_joint`

To avoid asserting a mechanism I hadn't tested, I built a second file, byte-identical
to the shipped fixture except the two design-joint overlaps shrink from 0.10 mm to
0.0005 mm (inside I3/I8's 1e-3 mm tolerance). This is a **diagnostic probe, not a
known-good file** — its expectations are declared and covered (`docs/specs/
audit-known-good.md`'s discovery-gate rule requires it), but it is not offered as
correct geometry.

| check | shipped fixture (0.10 mm overlap) | probe (0.0005 mm overlap) |
|---|---|---|
| I3 | fail, `path_open` x4 | **pass**, 0 indeterminate |
| I5 | **pass**, 0 free ends, 0 fusion-uncertain | fail, `PATH_INDETERMINATE reason=fusion_uncertain` x4 |
| I8 | fail, `path_indeterminate` x12 | fail, **`GEOM_DEAD_MEMBER`** x12 (a different, definite failure) |

I3 passing on the probe confirms the coincidence-tolerance mechanism directly. I5's new
failure is itself informative and not confounded noise dressed up as one: shrinking
both joints by ~0.0995 mm each pulls the whole column 0.199 mm shorter, and the
resulting **tip-to-member** separation (0.0261 mm — a *different* pair than either
shrunk joint) lands inside the fusion-uncertain band, which I5 correctly refuses to
resolve either way rather than guess. And I8's failure on the probe is not a new
problem — it is `docs/specs/audit-known-good.md` §5.1's own prediction, made *before*
this file was built, coming true on contact: *"every TAPER, and probably every MEMBER
and FLARE, is dead by [I8's liveness] roles."* Measured here: exactly the 12 non-tip
records, exactly `GEOM_DEAD_MEMBER`, with `roles_tested=R1,R2,R3,R4` on every one.

### 4.4 The net, and its classification (per §8 of the design doc)

**I3 and I8, as currently implemented, cannot both be satisfied by a joint that also
satisfies I5 the way the specification's own fusion-uncertain-band physics calls for.**
A joint separation negative enough to be an I5-definite overlap and simultaneously
small enough (`<= 1e-3 mm`) to register as an I3/I8 coincidence is a narrow target that
a physically-motivated overlap margin (`0.10 mm`, ~1.8x the width of the
fusion-uncertain band itself) will always miss on the I3/I8 side — and shrinking to hit
that target instead trades the problem for a *different* fusion-uncertain failure
elsewhere on the same column (§4.3).

Classified per `docs/specs/audit-known-good.md` §8: this reads as **(c) instrument
defect** — I3 and I8 reconstruct connectivity with a mechanism that predates, and is
now inconsistent with, the one I5 was specifically built to correct. It is not (a) an
over-harsh threshold (the *thresholds* — `d3`, `JOIN_TOL_EMITTED_MM` — are each
individually reasonable; they simply disagree about which one governs "is this the
same joint") and it does not look like (b) a missing physics term. **Register this as a
finding for the tool** (suggested id `KG-F2`, alongside `KG-F1` for I8's structural
unsatisfiability, §5.1 of the design doc, now independently confirmed by §4.3 above):
`I1`/`I3`/`I8` should likely walk the same overlap-based graph `I5` already builds,
rather than `ctx.graph`'s point-coincidence reconstruction.

**If the file is wrong instead, the counter-argument would have to be**: a 0.10 mm
design-joint overlap is not "real" enough to count as one fused solid. I don't think
that is defensible against `audit-invariants.md` §3.5's own stated reasoning (the
fusion-uncertain band exists precisely because a *smaller* gap is not established to
cure as one), but it is the owner's call, not mine (open question, new).

---

## 5. What I got wrong, and fixed, before shipping

Both of these were physics/bookkeeping corrections, not check-driven retuning: neither
changed a single audited check's outcome from what it was before the fix (§5.2 is
confirmed by direct measurement; §5.1 is definitional — the file could not be measured
meaningfully at all before it was fixed).

### 5.1 The frame bug (z_plate), caught by I5's first run

My first build followed `docs/specs/audit-known-good.md` §2.3/§3.1 literally: a
Z-CENTRED slab (`z ∈ [−1, +1]`) with `translate_z = 5.0`, on the stated assumption
`z_plate = −translate_z = −5.0`. Running the audit immediately produced `I5
PATH_FREE_END` on all four flare feet.

The actual relation, `src/context.rs::resolve_plate` (`ContractRelation` branch, also
documented in `tools/plinth-audit/README.md` finding 1): `z_plate_mm =
mesh.bbox.min[2] − translate_mm[2]`, **not** `−translate_mm[2]` alone. For my Z-centred
slab, `mesh.bbox.min[2] = −1.0`, so the reconstructed plate was `−1.0 − 5.0 = −6.0 mm`
— one full millimetre below where the flares actually landed (`−5.0 mm`), hence every
foot registered as suspended a millimetre short of ground.

**The design doc's own §2.3/§3.1 arithmetic is only correct for a model mesh anchored
at local `z = 0`** (as `fixtures/starved_patch/generate.rs` and
`fixtures/double_load/generate.rs` already do) — a Z-centred slab silently breaks it.
**This affects every one of the doc's five planned KG files (KG-2 through KG-5 all
specify Z-centred meshes with the same `z_plate = −translate_z` assumption) and is
worth flagging to whoever builds them next.**

Fix: anchor the slab at local `z = 0` (underside), `translate_z = 4.0` mm (the model's
float height directly, since the underside now sits at local `z=0`). Confirmed:
`z_plate_mm` now reads `−4.0` exactly, and I5 passes with 0 free ends.

### 5.2 The flare-foot clearance (R5), caught by hand, not by any check

Described in §2.3. The design doc's own R5 arithmetic (`0.90 mm` foot radius) leaves
adjacent flare feet `2.00 − 2×0.90 = 0.20 mm` apart at the plate — under
`FINAL_CLEARANCE_GAP_MM = 0.300 mm` — and **nothing in the ten audited checks would
have caught it**, because `FINAL_CLEARANCE_GAP_MM` is not referenced anywhere under
`tools/plinth-audit/src/`. Fixed by hand: foot radius `0.65 mm`, leaving `0.70 mm`
(2.33x the rule). Re-ran the full audit after the change: identical result on all ten
checks (§3's table), confirming the fix touched nothing the tool grades.

---

## 6. Harness discovery — verified, not assumed

Given the project's own recorded failure mode (a fixture harness that globbed one path
while agents wrote to another, and reported green having discovered nothing), I ran the
discovery gate directly rather than trusting that these files would be found:

```
$ cargo test --release --test fixtures discovery_reaches_every_fixture_directory_on_disk -- --nocapture
...
    four_leg_overhang: 11 expectation file(s)
```

and the content-checking test:

```
$ cargo test --release --test fixtures every_fixture_trips_the_check_it_names -- --nocapture
...
  ok       four_leg_overhang [i2_contact_area] ...
  ok       four_leg_overhang_i1 [i1_seat_depth] ...
  ok       four_leg_overhang_i4 [i4_load_ownership] ...
  ok       four_leg_overhang_i5 [i5_closed_load_path] ...
  ok       four_leg_overhang_i6 [i6_no_intersection] ...
  ok       four_leg_overhang_i7 [i7_island_coverage] ...
  ok       four_leg_overhang_i9 [i9_unsupported_span] ...
  ok       four_leg_overhang_i10 [i10_arrangement_spread] ...
  ok       four_leg_overhang_probe_tight_joint_i3 [i3_min_section] ...
  ok       four_leg_overhang_probe_tight_joint_i5 [i5_closed_load_path] ...
  ok       four_leg_overhang_probe_tight_joint_i8 [i8_no_dead_geometry] ...
fixtures: 30 satisfied, 0 pending, 0 failed
```

All 11 of this directory's expectations are `ok`/satisfied (all 30 satisfied across the
whole fixtures tree that run, including sibling directories' work in progress at the
time — none of mine among the 0 failed). The suite's `assert_eq!(satisfied, 11, ...)`
backstop then fails, by design: it is a hard-coded count of the pre-existing eleven
broken-file fixtures, and adding any new expectation is *supposed* to trip it, so a
silently-vanishing fixture is loud. **Whoever owns `tests/fixtures.rs` needs to update
that count deliberately** (it should become `41` once every currently-in-progress
sibling KG fixture in this run has landed, not just this directory's 11 — I do not own
that file and have not touched it).

### Why this directory has 11 `.chitubox`/`.expect.json` pairs, not one

`tests/fixtures.rs` binds exactly one expectation to one artifact by filename, and the
harness has no `all_checks_pass` field yet (`docs/specs/audit-known-good.md` §7.1 asks
for one; that is explicitly not this builder's file to add). To declare "these eight
checks must pass" using the vocabulary the harness actually supports today, the
canonical file is hard-linked under one name per check (`four_leg_overhang.chitubox`
plus `_i1`/`_i4`/`_i5`/`_i6`/`_i7`/`_i9`/`_i10`, eight identical byte-for-byte copies,
`sha256` confirmed), each with its own `check_passes` expectation. The diagnostic probe
is the same pattern for its three checks. This is the same convention `broken_path`
already established for two artifacts in one directory; it just needed to scale to
eleven.

---

## 7. Files in this directory

| file | what it is |
|---|---|
| `four_leg_overhang.chitubox` + `_i1`/`_i4`/`_i5`/`_i6`/`_i7`/`_i9`/`_i10.chitubox` | 8 hard-linked copies (identical bytes) of the shipped known-good file, one name per passing check |
| `*.expect.json` (11) | one `check_passes`/`check_fails` expectation per named artifact above |
| `four_leg_overhang-model.stl`, `-supports.stl` | binary STL fallback for inspection without ChituBox (local/untranslated frame; both files share the same frame so their relative geometry is correct) |
| `four_leg_overhang_probe_tight_joint_i3/_i5/_i8.chitubox` | 3 hard-linked copies of the diagnostic probe (§4.3), one name per check it demonstrates |
| `four_leg_overhang_probe_tight_joint-model.stl`, `-supports.stl` | STL fallback for the probe |
| `generate.rs` | standalone generator, exact re-runnable record (see its header for build instructions) |
| `README.md` | this file |

## 8. ChituBox openability — disclosed limitation

Per `docs/specs/audit-known-good.md` §7.2, I spliced a 396-byte all-zero settings blob
after the header (matching the documented "empty project" baseline), set
`settings_size=420`, and shifted every absolute pointer that moves as a result (`off8`,
`off20`, each model's `mesh_off`/`support_index_ptr`, the support index's own
`records_off`/`mesh_off`, and every individual record's own `mesh_off`) — more fields
than the design doc's own bullet names, because a self-consistent file needs all of
them, not just the four it calls out. Verified header fields after the splice:
`settings_size=420`, `off8=420`, `const16=412` (unchanged), `off20=1100` (`= off8 +
1*680`, correct for one model). **I do not have ChituBox available in this environment
and could not perform the actual open-and-inspect step §7.2 asks for as an acceptance
criterion.** The STL files in this directory are the fallback the spec names for exactly
this situation; please open those, or the `.chitubox` itself, and tell me if it opens
cleanly.

## 9. A sibling fixture exists: `spread_wide`

While building this, another agent in the same session independently built
`fixtures/spread_wide/` from the same design doc (also KG-1, also a 4×4×2mm slab on
four posts at `(±1,±1)`), and independently hit and fixed the *identical* §5.1 frame
bug. Their measured numbers (worst `X_delivered` 9.86, worst reach 1.0516 mm, I4
residual ~2.66e-10 mm², I10 `m_hat=0.5`) agree with mine to displayed precision — a
useful independent cross-check on both builds. Their fixture set covers I1/I2/I4/I5/I6/
I7/I9/I10 (8 checks, same 8 that pass here); I did not see an I3 or I8 expectation
under their directory, so I cannot say whether they hit the same finding in §4 or built
around it. Worth reconciling when both land.
