# `hollow_drained` — the first known-good fixture: a roofed recess, correctly posted from inside

This is `docs/specs/audit-known-good.md` §3.4's **KG-4** (`kg_recess_posted`), built
geometry-for-geometry at this task's assigned directory name (`hollow_drained`) rather
than the spec's own fixture id — same substitution convention as `fixtures/long_ledge`
(built for `span_ledge_long`) and `fixtures/starved_patch`/`fixtures/double_load`. KG-4
was picked over the other four candidate files because it is the one in the set that is
both "hollow" (a roofed recess) and "drained" (open downward, "so nothing traps resin" —
the spec's own words), and because its posts are sized directly from the physics of what
each one carries, with no borrowed-demand oddity to explain (unlike KG-5).

**This is the first fixture in this tool that is asked to PASS.** All eleven fixtures
that existed before this one are deliberately broken files; the harness has never once
been asked whether it can say yes. It can, on eight of the ten gating checks — see
"What the audit measured", below, and "Findings against the tool" for the two it cannot.

## For the owner (this is the acceptance test — the arithmetic below is what to check, not what to trust)

An 8.4 mm cube-ish block, 4.5 mm tall, sitting 4 mm above the print plate on sixteen
posts arranged in a 4×4 grid, 2.1 mm apart. Cut up into the underside of the block is a
4.2 mm square hole, 3 mm deep — a recess with a 1.5 mm roof, open at the bottom so
nothing traps resin inside it. Twelve of the sixteen posts hold up the solid rim around
the hole; the other four run up **inside the hole** and hold up the roof from underneath,
exactly the way `docs/physics.md` says a hollow's ceiling has to be supported. Every post
carries the same amount of underside — a 2.1×2.1 mm square, 4.41 mm² — because the grid
is even and the hole lines up exactly with four of the grid cells. Each post therefore
needs the same size ball and arm, worked out from the measured area rule (§below): a ball
about 0.68 mm across buried about a fifth of a millimetre in, on top of an arm about
1.03 mm across. Every number below is that same arithmetic, done by hand, so you can
re-check it rather than take the tool's word (or mine) for it.

**If you open this in ChituBox and it looks under-built or over-built anywhere, that is
the finding, not something to fix quietly** — see "Open questions", below, and
`docs/specs/audit-known-good.md` §9 for the standing list this file inherits.

## 1. Geometry

Outer block: `x, y ∈ [−4.2, 4.2]` mm, `z ∈ [Z0, Z2] = [−2.25, 2.25]` mm in the
bounding-box-centred mesh frame (so the mesh's own bbox is exactly `[−4.2,−4.2,−2.25]` to
`[4.2,4.2,2.25]`, 8.4 × 8.4 × 4.5 mm). Recess: `x, y ∈ [−2.1, 2.1]`, `z ∈ [Z0, Z1] =
[−2.25, 0.75]` — 4.2 × 4.2 × 3.0 mm, removed from the block's underside, leaving a
1.5 mm roof (`z ∈ [0.75, 2.25]`) and a solid rim everywhere `|x| > 2.1` or `|y| > 2.1`.
Support lattice: 4×4 posts at `x, y ∈ {−3.15, −1.05, +1.05, +3.15}` mm. The perpendicular
bisectors of that grid fall at `x, y ∈ {−2.1, 0, +2.1}` — exactly the recess boundary —
so the sixteen Voronoi cells are exactly the sixteen 2.1×2.1 mm squares, and the four
inner cells (`|x|, |y| < 2.1`, i.e. the posts at `±1.05, ±1.05`) are exactly the recess
mouth. The four inner posts seat on the roof's underside (`z_face = Z1 = 0.75`); the
twelve outer posts seat on the rim's underside (`z_face = Z0 = −2.25`). The whole model
floats 4.00 mm above the plate (`z_plate = −6.25` mm; see §5 for how `translate_mm`
actually has to be set to get that, which is not what the spec's own shorthand formula
gives).

```
A_face  = 70.5600 mm²   = 8.4² = 16 × 4.41            (rim 52.92 + roof 17.64)
A_cell  =  4.4100 mm²   = 2.1 × 2.1, exact by the bisector symmetry, all 16 posts alike
```

## 2. Arithmetic — the sizing procedure, and the numbers it produces

Following `audit-known-good.md` §2.1's five-line procedure, from `docs/physics.md`'s
area law (`X_ALLOWED = floor(25.00/2) = 12`) and the scar-invariant tip sizing
(`r_tip = sqrt(A_cell / (12π))`, `12π = 37.6991118431`):

```
A_req   = A_cell / 12        = 0.367500 mm²
r_tip   = sqrt(A_cell/12π)   = 0.342022 mm    (design; the emitted file's actual stored
                                                values are 0.3420309 mm on 15 posts and
                                                0.3418846 mm on the 16th — see §6)
R = r_arm = 1.5 × r_tip      = 0.513046 mm    (arm section π·R² = 0.826919 mm² = 2.2501 × A_req)
h = 0.60 × r_tip             = 0.205219 mm    (h_required = 0.130644 mm, ball-term-bound → 57.1 % over)
a = 1.2 × r_tip              = 0.410437 mm    (ideal cut-disc radius at depth h)
A_ideal = π·a²               = 0.529228 mm²   = 1.4401 × A_req
```

`r_arm = R` exactly (both equal `1.5·r_tip`), so the arm is never the binding section —
the binding section is the seat's own cut disc, exactly as `audit-known-good.md` §2.1
predicts. The 16th post (a corner post, `x=y=3.15`) differs from the other fifteen in
the seventh significant figure only (§6), so every number above holds for it too to five
decimal places.

**Clearances, computed by hand, not read from any check:**

```
adjacent post true gap (rim, pitch 2.10 mm)   = 2.10 − 2×0.513046 = 1.0739 mm  (R5: > 0.30 mm ✓, 3.6×)
diagonal post true gap (pitch 2.10√2 mm)      = 2.9698 − 2×0.513046 = 1.9437 mm
interior post axis-to-cavity-wall distance    = 2.10 − 1.05 = 1.05 mm          (R2: > R=0.513 mm ✓, 2.0×)
interior post arm-to-wall true gap            = 1.05 − 0.513046 = 0.5370 mm    (R5: > 0.30 mm ✓, 1.8×)
tip axis to its own down-face edge            = 1.05 mm                       (R4: > a=0.410 mm ✓, 2.6×)
worst reach (square lattice, s=2.10 mm)       = (√2/2)·s − a = 1.0745 mm       = 79.9 % of the 1.344976 mm
                                                                                  span allowance (I9)
```

**For the owner, in the same terms as §3.4's own paragraph:** *each post carries a
2.1 × 2.1 mm patch of underside — 4.41 mm². The measured rule says the section anywhere
on the load path must be at least a twelfth of that, 0.368 mm². The ball on top of each
post is buried about 0.205 mm into the model, cutting a circle 0.821 mm across —
0.529 mm², 1.44 times what is required. The arm above the ball is 1.026 mm across,
0.827 mm², 2.25 times what is required. The furthest any point of a post's own patch sits
from that post is 1.07 mm; the measured span allowance is 1.34 mm — 80 % of it. Every
post — rim or roof — clears the 0.30 mm final-clearance ruling by at least 1.8×.*

## 3. Support chain (identical shape on all 16 posts; radii per §2 and §6)

| record | kind | from → to | radii |
|---|---|---|---|
| 0 | 7 TIP | `p0 = (x,y,z_face+h)` → `p0 − 2R·ẑ` | `r1=r2=R` |
| 1 | 1 TAPER | ball centre `p0 − R·ẑ` → `ball_centre − (R+0.10)·ẑ` | `r_tip → r_arm` |
| 2 | 3 MEMBER | = TAPER's own end (zero declared gap) → `z_plate+0.90` | `r_arm` constant |
| 3 | 4 FLARE | = MEMBER's own end (zero declared gap) → `z_plate` | `r_arm → 0.90 mm` |

No type-6 raft records: every flare lands directly on the plate, grounded by
`audit-invariants.md` §3.5 clause (i) on its own — see §7's note on why a raft was not
used. 64 records total (16 chains × 4), 92 model mesh triangles, 90 printer layers.

**The taper's length deviates from `audit-known-good.md` §2.2's flat "0.50 mm below the
ball centre", and §5 explains why in full — the short version is that a flat 0.50 mm
drop is unsafe whenever `R > 0.50 mm`, which THIS file's own `r_tip` produces (`R =
0.513 mm`), and the audit caught it.**

## 4. What I deliberately did not read

Everything in §2 comes from `docs/physics.md` (the area law, the scar invariant, the
0.30 mm final-clearance ruling) and from `audit-invariants.md` / `audit-span-criterion.md`
for the measured printer profile and `L_SPAN_ALLOWED_MM = 1.3449762463 mm`. I did **not**
open `src/checks/*.rs` (`contact_area.rs`, `narrowest_section.rs`, `load_ownership.rs`,
`dead_geometry.rs`, `unsupported_span.rs`, `arrangement_spread.rs`, `interval.rs`, or
any other check) or `src/seat.rs`, `src/ownership.rs`, `src/raster.rs`, `src/graph.rs`.
No threshold or geometric choice in §2 or §3 was reverse-engineered from a check.

**What I did open, and why it's disclosed rather than hidden:** `src/synth.rs` (the
byte-level writer this fixture is built with — its public API, not its callers),
`tests/fixtures.rs` (the harness contract §7 below explains I had to work within),
`src/consts.rs` (the same pinned printer/tolerance constants every existing fixture
README already cites — `X_ALLOWED`, `ball_seat_factor`, `D3_MM`, `EPS32`), and the
`plinth-audit --json` / `--dump-graph` output of my OWN file, used as a measurement
instrument the same way `starved_patch`'s and `double_load`'s generators used it to
ULP-search for an f32-exact radius — never read as a spec for what "should" pass.
§5 and §6 below name exactly which measured numbers drove which change, so the
provenance of every deviation from the literal spec text is traceable.

## 5. What the audit found, and what I changed (both are measurements, not guesses)

**5.1 — the frame formula.** `audit-known-good.md` §2.3 says `z_plate = −translate_z`.
The first build used that literally (`translate_z = 6.25`) and the audit reconstructed
`z_plate = −8.5`, not the intended `−6.25` — every flare's declared bottom sat 2.25 mm
*above* the plate the audit actually computed, and every one of the 16 chains reported
`I5 PATH_FREE_END`. The real relation, confirmed against `fixtures/double_load`'s own
generator comment (`z_plate_mm = mesh.bbox.min.z - translate_z`, matching its own
`-2 - 5.03 = -7.03`), includes the mesh's own bounding-box minimum, which is **not** zero
for a bounding-box-centred mesh (it is `−Z2 = −2.25` here). Solving that formula for the
translate this file needs gives `translate_z = bbox.min.z − z_plate = −2.25 − (−6.25) =
4.00` — which is simply the "floats 4.00 mm above the plate" figure the spec already
states, so the fix is `translate_mm = [0,0,4.00]`, not `[0,0,6.25]`. §2.3's shorthand
holds only for a mesh whose bbox bottom sits at `z=0`; it does not for this design.

**5.2 — the taper's length.** §2.2's table fixes the taper's drop at a flat 0.50 mm below
the ball centre. That is safe for KG-1 (`R = 0.4886 mm < 0.50`), but this file's own
`r_tip` gives `R = 0.5130 mm`, marginally **above** 0.50 mm — the ball's own bottom pole
then sits 0.013 mm *below* where a flat-0.50 taper would end. Building it that way and
letting the member start right at the taper's end (needed so the record-adjacency graph
— which links records by declared-endpoint coincidence within about 0.02 mm, not by 3D
solid overlap — treats taper and member as one chain) put the member's own start within
0.013 mm of the tip's own ball surface: inside `D3_MM = 0.0552901438 mm`, the
fusion-uncertain band `audit-invariants.md` §3.5 defines, and I5 reported
`PATH_INDETERMINATE reason=fusion_uncertain` between the TIP and the MEMBER on all 16
chains (not between the taper and the member — the pair the flat drop was clearly meant
to protect). **The fix used here is a taper drop of `R + 0.10 mm`** instead of the flat
0.50 mm — a strictly more conservative version of the same rule (it guarantees at least
0.10 mm of tip-to-member clearance on ANY `r_tip`, where the flat constant only does so
below `R = 0.40 mm`), not a different design. `docs/specs/audit-known-good.md` should
carry this correction forward to KG-1/2/3/5 if their own `r_tip` values are re-derived.

**5.3 — record adjacency needs near-zero gaps, not R1's literal "≥0.10 mm overlap".**
With the flat 0.50 mm taper *and* a 0.10 mm declared gap between TAPER's end and MEMBER's
start (as §2.2's "node + 0.10 mm (into the taper)" literally reads), the join graph
(tolerance ≈0.02 mm for a file the audit classifies as "native" — see §7.2) does **not**
link TAPER to MEMBER: I5 reported 48 separate one-record components instead of 16
four-record chains, and I3 (which walks that same graph to find the narrowest section
along a whole load path) could not get past the TIP (`MIN_SECTION_INDETERMINATE
reason=path_open`, `path_records=[0]` only, on all 16 contacts). Both are MEASURED, not
inferred. Setting each join to the OTHER record's own declared endpoint exactly (zero
gap) — the same convention `fixtures/starved_patch` and `fixtures/double_load` already
use — closes the graph (I5: 0 failures) and lets I3 walk the whole chain (I3: 0
failures). Physically this is not "no overlap": since TAPER's end radius and MEMBER's
start radius are the SAME value (`r_arm`) at the SAME declared point, the two frustums
share an exact, flush, same-radius cross-section — an unambiguous fusion, not a
`separation ∈ (0, d3]` case R1 is protecting against. **R1's literal "≥0.10 mm" appears
to describe a margin of safety for records whose radii do NOT match exactly at the join,
not a requirement on every join regardless of radius match — this reading is not
confirmed against the check's own source (§4), so it is flagged here as a build note,
not asserted as settled.**

Every one of these three was measured on THIS file, corrected once, and re-measured —
never adjusted a second time to chase a specific verdict (§8 of the governing spec).

## 6. The I4(d) exactness sweep, in full

`I4` clause (d) is an equality: `|Σ 12π·r_tip² − D_total| ≤ max(4096·eps64·D_total,
1e-9) mm²`, which at this file's scale is the flat `1e-9 mm²` floor. `D_total`,
reconstructed from the STORED f32 mesh vertices exactly as the audit would (plain
triangle-area arithmetic, not read from any check — `reconstruct_down_facing_area` in
`generate.rs`), came out to `70.559993591309 mm²` — 6.4 µm² off the nominal `70.56` from
ordinary f32 vertex rounding.

Fifteen posts share one f32 tip radius; the sixteenth (a corner post) is solved so the
f64 sum matches `D_total` as closely as an f32-resolution search over ±2000 ULPs can
land it (same method as `starved_patch`'s and `double_load`'s own generators). **First
attempt, ±2000/±4000-ULP search, ordinary round-trip filter:** residual **4.099e-6 mm²**
— 4000× the floor. Measuring the audit's own JSON explained why: `i1_seat_depth`'s
per-contact output reports THREE independently-derived `r_tip` values per contact
(`r_tip_a` = `R/1.5` at full f64 precision, never re-rounded; `r_tip_b` = the declared
TAPER `r_start`; `r_tip_c`, the fitted soup) with a measured `r_tip_spread_mm ≈ 1.06e-8
mm` even when `r_tip_a` and `r_tip_b` round-trip cleanly through TWO f32 roundings — and
`I2`/`I4`'s own "implied capacity" arithmetic uses `r_tip_a` (full f64 precision), not
the stored `r_tip_b`. **Second attempt:** tightened the sweep's own filter to require
`1.5 × r_tip` to be EXACTLY representable in f32 with ZERO rounding (not merely
round-trip-stable through two roundings) — see `clean_ball_radius` in `generate.rs`.
Residual: **2.412e-9 mm²** — ratio `1.0000000034`, 3.4 parts per billion, 2.4× the floor.

```
achieved sum  Σ12π r_tip²  = 70.55999359372093 mm²
D_total (reconstructed)    = 70.55999359130874 mm²
residual                   = 2.412e-9 mm²   (floor: 1e-9 mm², 2.4x over)
r_tip (15 posts)           = 0.34203088 mm
r_tip (1 corner post)      = 0.34188461 mm    (|Δ| = 1.46e-4 mm, four decimals, not the
                                                seventh — disclosed, not the invisible
                                                deviation §2.5 of the governing spec
                                                predicts; a wider ULP search (±20000)
                                                found a marginally lower residual,
                                                1.44e-9 mm², at the cost of a 4.98e-3 mm
                                                deviation — a strictly worse trade for a
                                                physically meaningful design, so it was
                                                NOT used)
```

**This residual is reported, not hidden, and the file was not adjusted further to chase
it below the floor** — see "Findings against the tool", next.

## 7. Findings against the tool

Both are registered against their own `check_fails` expectation in this directory
(`hollow_drained_i4_load_ownership.expect.json`, `hollow_drained_i8_no_dead_geometry.expect.json`),
so they are harness-verified, reproducible facts, not prose claims.

### KG-F1 — I4 clause (d) refuses a file that was built specifically to satisfy it

**Measured:** `LOAD_BOOKS_DISAGREE d_total_mm2=70.55999359130874 implied_lo_mm2=70.55999359372093 implied_hi_mm2=70.55999359372093 ratio=1.0000000034`.
`clamped_contacts=0` — this is `audit-known-good.md` §4's "strict" case, an equality
test, not a wide bracket; the file takes no clamp escape. Every OTHER I4 clause passes:
`a_unowned` (pass, 0 mm² unowned), `b_partition_identity` (pass, residual 2.29e-11 mm²),
`c_component_routing` (pass), `e_demand_double_reconstruction` (pass). Only `d` fails,
by 2.412e-9 mm², after the two rounds of measurement-driven refinement in §6.

**Classification (per governing spec §8):** most likely **(a) over-harsh threshold** —
the `1e-9 mm²` absolute floor sits below the resolution of a THREE-way f32 cross-check
that has its own irreducible ~1e-8 mm per-contact spread (§6), propagated through 16
contacts. It could also be **(c) instrument defect** if `I2`'s and `I4`'s "implied
capacity" is meant to use the DECLARED `r_tip` (the taper's own stored field) rather than
`R/ball_factor` at un-rounded f64 precision — in which case the fix is to round `r_tip_a`
to f32 before using it, which the measured evidence in §6 suggests would make the residual
exactly zero on a file like this one. **I did not open `src/checks/load_ownership.rs` to
settle which; the classification is offered as a hypothesis from measured behaviour, not
asserted as read from the source.**

**Corroborating evidence from a sibling known-good file, found in the same harness run
(§11):** the KG-1 fixtures built independently in this same session (`trivial_cube`,
`spread_wide`, `four_leg_overhang` — 4 contacts, not 16) report I4(d) residuals of
`2.66e-10 mm²` and `2.663e-10 mm²` — UNDER the `1e-9 mm²` floor, a clean pass. The
per-contact `r_tip_a`/`r_tip_b` disagreement this section measures (~1e-8 mm) is roughly
constant per contact regardless of file size, so it accumulates with contact count:
4 contacts stays under the floor, 16 contacts (this file) lands just over it
(`2.412e-9 mm²`, 2.4×). **This sharpens the classification: I4(d)'s `1e-9 mm²` floor is
not simply too tight — it does not scale with the number of contacts it sums over, while
its own dominant error source does.** A floor of the form `max(1e-9, k·n_contacts)` mm²
would likely pass every physically-adequate file in this set, KG-4 included, without
loosening the equality's intent.

### KG-F2 — I8 refuses every well-formed support chain, exactly as predicted in advance

**Measured:** all 48 non-TIP records (16 TAPER + 16 MEMBER + 16 FLARE) report
`GEOM_DEAD_MEMBER`, `roles_tested=R1,R2,R3,R4`. Zero TIP records are dead (all 16 pass,
as I1 and I8's own R1 role require of a correctly-seated contact). This is **exactly**
`audit-known-good.md` §5.1's prediction, made before this file was built: *"a correctly
seated TIP is its own ground… So every TAPER, and probably every MEMBER and FLARE, is
dead by roles."* Corroborating measured evidence already in the repository: every one of
the 31 corpus files fails I8, and `GEOM_DEAD_MEMBER` fires 2,638 times over the 24 real
miniatures.

**Classification (per governing spec §8):** **(b) modelling gap** — I8's liveness roles
(R1–R4) do not have a route for "carries a live tip's entire load to ground and is
therefore obviously alive", because role R2 ("removing it disconnects a live TIP from
ground") requires the TIP to reach ground THROUGH something, and a correctly-seated TIP
already reaches ground on its own by I5/I6's own rules. This is a specification-level
finding, not a defect in this file — no well-formed, non-branching support chain can pass
I8 as currently specified. The fix is the owner's ruling on what "ground" means for role
R2 (plate-and-raft only, vs. any valid seat, per `audit-known-good.md`'s own open
question), not a change to this fixture.

## 8. What the audit measured — all ten checks, this file, current tree

Run with `plinth-audit hollow_drained.chitubox --json`, the real instrument, not a
hand-written expectation.

```
I1  seat depth          PASS   16 checked, 0 failed, 0 indeterminate
                         "16 contacts meet their own seat requirement"
                         h_required 0.13059-0.13064 mm, h_delivered 0.20513-0.20522 mm

I2  contact area         PASS   16 checked, 0 failed, 0 indeterminate
                         "worst X delivered 9.8 against 12, median raster loss 0.145"
                         a_rast 0.4522-0.4527 mm² (needs 0.3675 mm²), 1.23-1.24x

I3  min section          PASS   16 checked, 0 failed, 0 indeterminate
                         "worst A_min 0.5478 mm2 needs 0.3675" -- 1.49x

I4  load ownership       FAIL   1 checked, 1 failed  -- KG-F1, see sec 7
                         LOAD_BOOKS_DISAGREE, ratio 1.0000000034 (2.412e-9 mm2 over a
                         1e-9 mm2 floor); every other clause (a,b,c,e) passes

I5  closed load path     PASS   64 checked, 0 failed, 0 indeterminate
                         "0 orphaned, 0 free ends, 0 indeterminate (of 64 records)"

I6  no intersection      PASS   64 checked, 0 failed, 0 indeterminate
                         "outside-seat 0 mm3 ... in 0 of 64 records (0.00%)"

I7  island coverage      PASS    1 checked, 0 failed, 0 indeterminate
                         "1 detached islands above the one-pixel censor, all
                          intersected by support"

I8  no dead geometry     FAIL   64 checked, 48 failed -- KG-F2, see sec 7
                         GEOM_DEAD_MEMBER x48 (16 taper + 16 member + 16 flare);
                         predicted in advance by audit-known-good.md sec 5.1

I9  unsupported span     PASS    2 checked, 0 failed, 0 indeterminate
                         "0 of 2 down-face regions exceed the 1.344976 mm allowance;
                          worst reach 1.108437 mm" -- 82.4% of the allowance

I10 arrangement spread   PASS   90 checked, 0 failed, 0 indeterminate
                         "1 bodies, 90 body-layer evaluations, every one in
                          equilibrium by more than sigma"
```

**8 of 10 gating checks pass, non-vacuously** (`checked > 0`, `indeterminate = 0`
everywhere they pass). The two that fail are both measured, both predicted or explained
in advance (§5.1 for I8; the ULP-sweep methodology of §2.5 for I4), and both registered
as findings against the tool per `audit-known-good.md` §8 — **this file was never
adjusted to make either of them pass.**

## 9. Reproducing

`generate.rs` in this directory is the exact, re-runnable generator. Not built by this
crate's own `Cargo.toml` (no `[[bin]]` entry). To regenerate:

```
mkdir /tmp/gen && cd /tmp/gen
cat > Cargo.toml <<'EOF'
[package]
name = "gen_hollow_drained"
version = "0.1.0"
edition = "2021"
[dependencies]
plinth-audit = { path = "/absolute/path/to/tools/plinth-audit" }
EOF
mkdir src && cp /path/to/fixtures/hollow_drained/generate.rs src/main.rs
RUSTC_WRAPPER="" cargo build --release
./target/release/gen_hollow_drained out.chitubox
```

`out.chitubox` is byte-identical to `hollow_drained.chitubox` (confirmed:
`sha256 = 77316f27e0f37737071226d68bcfd02bac3ccc1edbdae6cc74f5860f34bd1bda`, 230,224
bytes). The other nine `.chitubox` files in this directory are plain byte-for-byte
copies of the same file — see "Why ten copies of one file", below.

## 10. Why ten copies of one file

`tests/fixtures.rs` binds ONE expectation to ONE artifact by matching filename stems
(`<name>.expect.json` → `<name>.chitubox`), and `expect` takes `check_fails`,
`check_passes` or `file_fails` — one check per expectation. A known-good file needs ALL
TEN checks graded, and `audit-known-good.md` §7.1 asks for a new `all_checks_pass` +
`min_checked` harness verb that does not exist yet in `tests/fixtures.rs` (which this
directory does not own — see the task brief). Rather than wait on that, or write a
single misleading `check_passes` expectation naming only one of the ten checks, this
directory carries **ten byte-identical copies** of the design, one artifact stem per
check, so the CURRENT harness can assert all ten verdicts against the exact same bytes:
eight `check_passes` (I1, I2, I3, I5, I6, I7, I9, I10) and two `check_fails` locking in
the measured findings of §7 (I4: `LOAD_BOOKS_DISAGREE`; I8: `GEOM_DEAD_MEMBER` × 48).
This is a real, if inelegant, use of `expect.json`'s existing schema — not a workaround
that hides anything: every expectation's `description` field states in full what it is
asserting and points back to this README. When `tests/fixtures.rs` gains
`all_checks_pass`, the nine extra copies can be deleted and replaced by one expectation
naming `min_checked` per §7.1's own proposed schema, with `accepted_deviations` carrying
KG-F1 and KG-F2 forward.

## 11. Verification — the discovery gate, and the real harness run

Per this task's own instruction ("in an earlier run every fixture agent wrote its
expectation one directory deeper than the harness looked… verify by running the
harness"): `cargo test --release --test fixtures` was run against the real tree with
this directory in place (2026-08-31; this is a shared checkout, so other agents' own
known-good directories — `four_leg_overhang`, `trivial_cube`, `spread_wide`,
`marginal_but_adequate` — were present alongside `hollow_drained` in this same run).

`discovery_reaches_every_fixture_directory_on_disk` — the gate that specifically catches
the "wrote to the wrong depth, harness found nothing" defect — **passed**, confirming
`hollow_drained/` is reached like every other directory on disk.

`every_fixture_trips_the_check_it_names`'s own stdout, verbatim, for every
`hollow_drained` line:

```
running 60 expectations drawn from 15 fixture directories on disk
  ok  hollow_drained                       [i1_seat_depth]
  ok  hollow_drained_i2_contact_area       [i2_contact_area]
  ok  hollow_drained_i3_min_section        [i3_min_section]
  ok  hollow_drained_i4_load_ownership     [i4_load_ownership]   (check_fails, KG-F1)
  ok  hollow_drained_i5_closed_load_path   [i5_closed_load_path]
  ok  hollow_drained_i6_no_intersection    [i6_no_intersection]
  ok  hollow_drained_i7_island_coverage    [i7_island_coverage]
  ok  hollow_drained_i8_no_dead_geometry   [i8_no_dead_geometry] (check_fails, KG-F2)
  ok  hollow_drained_i9_unsupported_span   [i9_unsupported_span]
  ok  hollow_drained_i10_arrangement_spread[i10_arrangement_spread]
...
fixtures: 60 satisfied, 0 pending, 0 failed
```

**All ten of this fixture's expectations are satisfied** — the eight `check_passes`
genuinely passed and the two `check_fails` genuinely failed with exactly the codes
declared (`LOAD_BOOKS_DISAGREE`, `GEOM_DEAD_MEMBER`×48). Every OTHER fixture in the tree
(60 total, including the sibling known-good files other agents built concurrently) is
also satisfied — `0 failed, 0 pending`.

**The overall test run still reports FAILED**, but not because of anything in this
directory: `tests/fixtures.rs:603` hard-codes an eleven-fixture backstop
(`assert_eq!(left, right)` against the original eleven planted-defect directories) that
panics once ANY fixture is added — by design, so a fixture silently vanishing is loud.
With this directory plus the other agents' known-good directories now on disk, the count
is 60, not 11, and the backstop fires exactly as documented. That assertion lives in
`tests/fixtures.rs`, which this task does not own; updating it is the harness owner's
job, not a defect in `hollow_drained`. Full run: `708.04s`, `7 passed; 1 failed` (the
count backstop) among the file's own 8 top-level tests.

**A methodology note on run time:** an earlier attempt to isolate `collinear_arms.chitubox`
alone with a 15–30s timeout looked like a hang (`SIGTERM` at the timeout, 99% CPU, no
output) and was reported as a suspected infinite loop in an earlier draft of this
document. The full harness run above shows `collinear_arms` completing and satisfying
its own expectation normally — the isolated test was mistaking heavy CPU contention from
several concurrent sibling agents' own `cargo build`/`cargo test` runs (the same shared
machine) for a hang. **Retracted; no hang exists.** Left here rather than deleted, because
the correction itself is the useful record: a 15–30s timeout on a shared, heavily loaded
machine is not evidence of an infinite loop.

## 12. Open questions (owner's ruling, not settled here)

Inherits `audit-known-good.md` §9's U1–U8 in full (this file uses β=0.60, no rafts, and
predicts §5.4's plate-footprint gap does not apply since it floats). Two more, specific
to this build:

- **U9.** §5.3's reading of R1 ("≥0.10 mm overlap") as applying only to joins whose
  radii do NOT match exactly — is that the right reading, or should R1 hold literally
  even at a flush, same-radius join? This build took the empirically-necessary reading
  (§5.3) without opening the check that enforces it.
- **U10.** §6's `r_tip_a` vs `r_tip_b` disagreement (~1e-8 mm per contact, from dividing
  a stored f32 by 1.5 without re-rounding) — should I2/I4's "implied capacity"
  arithmetic round `r_tip_a` to f32 before using it? If so, KG-F1 may be a one-line fix
  rather than a tolerance question.
