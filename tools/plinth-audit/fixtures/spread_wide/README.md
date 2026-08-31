# `spread_wide` — KG-1 `kg_slab_four_post`, the first known-good fixture

Status, measured 2026-08-31: **8 of 10 checks pass** (I1, I2, I4, I5, I6, I7, I9, I10).
Two fail: **I3 (min section)** and **I8 (no dead geometry)** — both `INDETERMINATE`, not a
measured undersize. Both are reported below exactly as `docs/specs/audit-known-good.md`
§8 requires: unedited failure lines, my own re-derivation, and a classification of the
finding. **This file was not adjusted to make either check happy.**

This is `docs/specs/audit-known-good.md` §3.1's `KG-1`, built first per that document's
own §7.3 ("Build KG-1 first. Run the full audit on it. Publish its ten-check output
before building anything else.") — it is the base recipe every other known-good file in
that document reuses.

---

## 1. What is built, and why it is correct — derived from physics, not from the checks

A **4.00 × 4.00 × 2.00 mm slab**, floating with its underside 4.00 mm above the plate,
carried on **four posts** at `(x, y) = (±1.00, ±1.00)` mm — the perpendicular-bisector
centres of the slab's four 2×2 mm quadrants. Bounding-box-centred frame: slab
`x, y ∈ [−2, 2]`, `z ∈ [−1, +1]`; target `z_plate = −5.0` mm.

### 1.1 The sizing arithmetic (`docs/physics.md` §2, §9; `audit-known-good.md` §1–§2)

The four post axes are the perpendicular bisectors of the square, so the underside's
Voronoi partition is **exactly** the four 2×2 mm quadrants — no approximation, hand or
audit should get the same number:

```
A_cell   = 2.00 × 2.00                     = 4.0000 mm²    (exact, by symmetry)
A_req    = A_cell / X_ALLOWED              = 4.0 / 12      = 0.333333 mm²
r_tip    = sqrt(A_cell / (12·π))                            = 0.325701 mm  (target; the
                                                                built file's actual value
                                                                is R6/R7-perturbed, §2 below)
R        = 1.5 · r_tip           (ball radius = arm radius) = 0.488552 mm
h        = 0.60 · r_tip          (β = 0.60, §1.3 of the spec)= 0.195421 mm
h_req    = max(0.100, 0.050, ball_seat_factor(1.5)·r_tip)
         = max(0.100, 0.050, 0.381966·0.325701)             = 0.124407 mm   (ball term binds)
a        = sqrt(2·1.5·r_tip·h − h²)  = 1.2·r_tip             = 0.390842 mm
A_ideal  = π·a²                                              = 0.479903 mm²  (1.44 × A_req)
r_arm    = 1.5 · r_tip           (same formula as R, by design)= 0.488552 mm
arm_sect = π · r_arm²                                        = 0.750000 mm²  (2.25 × A_req)
```

`X_ALLOWED = 12` and the whole area-ratio law are `docs/physics.md` §2 (`X* ∈
[25.00,25.34]`, halved and floored). `ball_seat_factor(f) = f − sqrt(f²−1)`,
`0.381966011250105` at `f = 1.5`, is `docs/physics.md` §9 — reproduced by hand above and
cross-checked, not copied, against `src/consts.rs::ball_seat_factor` (I did read that one
function; see §7 for exactly what "reading the audit" meant here). The **maximum span**
allowance, `L_SPAN_ALLOWED_MM = 1.344976246291647 mm`, is
`audit-span-criterion.md` §2.6's own derivation from plates 1 and 4, restated in
`audit-known-good.md` §1.4; it is not re-derived here, only used.

### 1.2 R6/R7: why the four tips are not bit-identical

I4 clause (d) is graded as a near-equality — `Σ 12π·r_tip² == D_total` within
`max(4096·eps64·D_total/12, 1e-9) mm²` — and `π` is irrational, so **no single f32
`r_tip`** makes four identical tips hit that floor exactly. Per `audit-known-good.md`
§2.5, three tips share one radius (`r_a`) and the fourth absorbs the residual (`r_n`), both
individually constrained by rule R6 (`R = 1.5·r_tip` must lose no bits to f32 rounding —
I1 triangulates `r_tip` three ways and fails on disagreement).

The generator (`generate.rs`) does this by an ULP sweep: for each candidate `r_a` within
±60,000 f32 ULPs of the analytic target that also satisfies R6, solve
`r_n = sqrt(D_total/(12π) − 3·r_a²)`, search ±400 ULPs of the ideal `r_n` for one that
*also* satisfies R6, and keep the candidate **closest to the symmetric ideal** among all
pairs that clear the `1e-9 mm²` floor (not the single best residual anywhere in the
window — a solution 2% off-centre clears the floor exactly as well as one near it, and
staying near-centre keeps the "four equal cells" property closer to literally true).

**Achieved, measured from the built file:**

```
r_a = 0.3257014751434326 mm   (posts 0, 1, 2 — three of four)
r_n = 0.3258355855941772 mm   (post 3 — the fourth)
difference: 1.341e-4 mm, in the 4th decimal, not the 7th the design doc's general
            estimate suggested for an unconstrained sweep — the R6 constraint (both
            radii independently f32-exact under ×1.5) is the binding one here, not the
            I4(d) floor itself
residual  = 2.662688e-10 mm²   (measured; matches I4's own report to 5 significant
            figures — see §3 below), 3.76× under the 1e-9 mm² floor
ULP sweep width: ±60,000 (r_a) × ±400 (r_n), both filtered by R6 before scoring
```

Physically the two tips are indistinguishable (0.0134% radius difference); the 90 %
confidence this doesn't matter is standard — I2, I3(when measurable) and R2–R5 all have
double-digit-percent margins, so a fourth-decimal radius shift changes nothing that
matters. **Disclosed, not hidden**, per §2.5's own requirement.

### 1.3 The support chain (identical structure at all four posts, `audit-known-good.md` §2.2)

Per post, four records, no raft:

| rec | type | axis span (local `z`, post at `(x0,y0)`) | radii |
|---|---|---|---|
| TIP (7) | ball | `p0=(x0,y0,z_face+h)` → `p0+2R·(0,0,−1)` | `r1=r2=R` |
| TAPER (1) | frustum | ball centre `p0−R·(0,0,1)` → node 0.50mm further | `r_tip → R` |
| MEMBER (3) | frustum | node+0.10mm (R1 overlap) → `z_plate+0.90` | `R` constant |
| FLARE (4) | frustum | `z_plate+1.00` (R1 overlap) → `z_plate` | `R → 0.90mm` |

`z_face = −1.0` (slab underside, exactly the mesh's own bbox-min z). Every joint
overlaps its neighbour by ≥ 0.10 mm along the axis (rule R1,
`audit-invariants.md` §3.5's fusion-uncertain band is `(0, 0.05529] mm`, so 0.10 mm
clears it by ≈ 2×). No raft: a flare landing on the plate is grounded by
`audit-invariants.md` §3.5 clause (i) on its own — **measured true in this build**, see
§4's write-up of the bug that had to be fixed first to make that measurement honest.

### 1.4 The eight rules, checked by hand against this file

| rule | check | this file |
|---|---|---|
| R1 overlap ≥ 0.10mm | every joint | exactly 0.10mm at TAPER↔MEMBER and MEMBER↔FLARE; ≈0.489mm at TIP↔TAPER (ball radius) |
| R2 tip axis ≥ R from other surfaces | nearest other tip axis | 2.00mm (axis) / 1.41mm (diagonal) ≫ R=0.489mm |
| R3 h < R | every tip | 0.1954 < 0.4886 ✓ |
| R4 tip axis ≥ a from face edge | post to slab edge | 1.00mm ≫ a=0.391mm |
| R5 arm-arm clearance ≥ 0.30mm | adjacent posts, 2.00mm apart, 2×R=0.977mm of arm | gap = 2.00 − 0.977 = **1.023mm** ≫ 0.30mm |
| R6 R6 exact-×1.5 in f32 | r_a, r_n | confirmed by construction (§1.2) and by I1's `r_tip_agrees: true`, spread ≤ 2.27e-9mm |
| R7 declared depth = stored f32 delta | every TAPER | computed from f32-rounded `p0.z`/`z_face`, not the real-valued `h` (see `delivered_depth()` in `generate.rs`) |
| R8 no zero-area pole triangles | every TIP soup | `sphere_soup` output passed through `drop_zero_area()` (§7.2) |

---

## 2. What I did and did not consult (`audit-known-good.md` §6, Trap 1)

**Not opened:** any file under `src/checks/`, `src/seat.rs`, `src/ownership.rs`,
`src/raster.rs`, `src/graph.rs`. Every threshold and geometric rule above traces to
`docs/physics.md`, `docs/specs/audit-invariants.md`, `docs/specs/audit-span-criterion.md`
and `docs/specs/audit-known-good.md` — the project's physics and contract documents, not
the instrument's implementation.

**Opened, and why:** `src/synth.rs` (the writer API this file must use — its public
functions and doc comments, not test internals), `src/consts.rs` (pinned constants and
their derivations — `X_ALLOWED`, `ball_seat_factor`, pixel pitches, tolerances — all
already cited from `docs/physics.md`/`audit-invariants.md`, restated here with their
citation, never invented from the constant alone), `src/primitives.rs`'s
`Primitive::from_record` (to confirm the WIRE record ↔ solid mapping — e.g. that a
`RootFlare` record really does reconstruct as a frustum from `start`/`r_start` to
`end`/`r_end` — this is format/contract knowledge, the same class as `consts.rs`, not
check logic), `src/context.rs::resolve_plate` (see §4 — needed to fix a genuine build bug
in my own translate_z, not to reverse-engineer a check), `tests/fixtures.rs` (the harness
contract), and three existing generators for the writer-API pattern and known synth.rs
defects: `fixtures/double_load/generate.rs` (the `drop_zero_area` workaround and, as it
turns out, the correct `z_plate` formula — see §4), `fixtures/starved_patch/generate.rs`
(the TAPER-connects-at-ball-centre convention), `fixtures/bare_cavity_ceiling/README.md`
(the `Reproducing` recipe used verbatim in §6).

**Diagnostic-only, never shipped:** one throwaway variant with a raft pad added under
each flare, built in `$TMPDIR`, run once, and discarded — used only to isolate *whether*
I5's plate clause needed a raft present (§4). It is not part of this fixture and never
touched `fixtures/spread_wide/`.

---

## 3. Measured results, per check

Built and audited via `./target/release/plinth-audit fixtures/spread_wide/spread_wide.chitubox`,
the same entry point the test harness uses (`plinth_audit::run::audit_file`), 2026-08-31.

```
I1  seat depth         PASS    4 checked, 0 failed, 0 indeterminate
I2  contact area       PASS    4 checked, 0 failed, 0 indeterminate
I3  min section        FAIL    4 checked, 0 failed, 4 indeterminate   <- see section 5
I4  load ownership     PASS    1 checked, 0 failed, 0 indeterminate
I5  closed load path   PASS   16 checked, 0 failed, 0 indeterminate
I6  no intersection    PASS   16 checked, 0 failed, 0 indeterminate
I7  island coverage    PASS    1 checked, 0 failed, 0 indeterminate
I8  no dead geometry   FAIL   16 checked, 0 failed, 12 indeterminate  <- see section 5
I9  unsupported span   PASS    1 checked, 0 failed, 0 indeterminate
I10 arrangement spread PASS   40 checked, 0 failed, 0 indeterminate
```

### 3.1 Per-contact measured numbers (predicted vs. measured)

| quantity | predicted (§1) | measured (I1/I2 JSON) | agreement |
|---|---|---|---|
| `h_delivered` (posts 0-2) | 0.195421 mm | 0.19542086124420166 mm | matches to the predicted precision |
| `h_delivered` (post 3) | — (r_n-specific) | 0.19550132751464844 mm | consistent with `r_n` |
| `h_required` | 0.124407 mm | 0.1244068933188122 mm | matches |
| `A_ideal` (posts 0-2) | 0.479903 mm² | 0.4799011339706953 mm² | matches to 5 s.f. |
| `A_rast` (posts 0-2) | not predicted by hand (raster is numerical-only, §3.0 of the spec) | 0.40564999999998863 mm² | — |
| `X_delivered` (posts 0-2) | ≤ 9.86 (spec §3.1, quoting the same formula) | 9.86071736718874 | **matches to 4 s.f.** |
| raster loss | not predicted | 0.15472173061220718 (15.47%) | — |
| `worst reach` (I9) | 1.0233mm euclid / 1.1144mm chamfer (82.9%) | 1.0516347451388866 mm (78.2% of 1.344976mm) | measured chamfer 5.6% below the hand estimate — plausibly the corner-of-cell hand estimate and the audit's own voxel-grid worst point are not the identical point; both agree the region passes comfortably |
| I4 books ratio | residual → 0 by construction | ratio 1.0000000000166418, residual 2.6647e-10 mm² | **matches my own generator's reported residual (2.662688e-10) to 5 s.f.** |
| I10 `d` / `σ` | d=1.000mm, σ≈0.077mm (≈13σ) | 40/40 body-layer evaluations pass (0 failed, 0 indeterminate); `m_hat_worst_per_body`=0.5 (the JSON gives only this per-body aggregate, not a per-layer trace) | audit does not expose raw d/σ in this build's JSON; the hand cross-check independently derived in the spec is the only d/σ figure available, and it is consistent with a unanimous, comfortable pass |

`a_min_mm2`, `a_owned_mm2`, `station_mm` etc. for I3 are all `null` — the check never
reached a measurable station on any of the four contacts (§5).

### 3.2 File identity

```
sha256: 24238cd210346fe6aca43bb453af775348f0974c2a73fe577211bc31864247a1
bytes:  58396
records: 16 (4 chains × 4), contacts: 4, model triangles: 12
```

Reproduced byte-for-byte from `generate.rs` (§6) — confirmed by `cmp` against a second,
independent run in a scratch directory.

---

## 4. A build bug I found and fixed — not a tool defect, but worth a flag

`audit-invariants.md` §1.2 states the frame relation as:

```
z_plate = -translate_z            (RECONSTRUCTED from model record off264)
```

I built the first version of this file taking that line at face value:
`z_plate = -5.0` target → `translate_z = 5.0`. The **measured** result was `I5 closed
load path FAIL — 4 free ends`, all four on the FLARE's own end, `nearest_solid_mm ==
radius_mm` on every one — i.e. the flare (which geometrically sits exactly at my
intended `z_plate = -5.0`) was not being recognised as touching the plate at all.

`--dump-graph` on that build showed `z_plate=-6 (contract_relation_declared)` — **not**
`-5`. Reading `src/context.rs::resolve_plate` (format/contract code, not a check) showed
why: the actual relation, for a non-empty mesh, is

```
z_plate = mesh.bbox.min[2] - translate_z
```

which only reduces to `-translate_z` when the mesh's own bounding-box minimum `z` is
`0`. This fixture's mesh is **bounding-box-CENTRED** (`audit-known-good.md`'s own stated
frame convention), so its bbox-min `z` is `-1.0`, not `0` — and
`-1.0 - 5.0 = -6.0`, exactly the wrong value `--dump-graph` reported.

**This same, correct formula is already used by `fixtures/double_load/generate.rs`**
(its own comment: `z_plate = mesh.bbox.min.z - translate_z = -2 - 5.03 = -7.03`) — so the
mechanism was knowable from a sibling fixture already in this directory tree, and
`audit-known-good.md` §3.1's own stated pair (`z_plate = -5.0; translate_z = 5.0`) carries
the same error I made, for the same reason.

**Fix:** `translate_z = mesh.bbox.min[2] - z_plate = -1.0 - (-5.0) = 4.0`. Note that for
this simple flat-bottomed slab, `z_face == mesh.bbox.min[2]`, so `translate_z` ends up
numerically equal to the physical float height (4.00 mm) stated in
`audit-known-good.md` §3's own header — which is the more useful way to remember the
relation for a flat-bottomed model.

**Classification:** a documentation-precision issue, not an instrument defect — the
*measured* reconstruction (`resolve_plate`) is correct and is what a real emitted file's
`z_plate` actually needs; the audit-invariants.md §1.2 one-liner is a simplification that
happens to be exact only for the (common, but not universal) case of a bbox that touches
`z=0`. Flagging it because it cost real build time and the design spec itself
independently made the identical error.

---

## 5. The two failing checks — reported, not fixed

Per `audit-known-good.md` §8: neither check's threshold, radius, pitch, or tolerance was
touched to make it pass. Both are reported as measured, with re-derivation and
classification.

### 5.1 I8 no dead geometry — **predicted**, confirmed

`audit-known-good.md` §5.1 predicted, before any file was built, that I8 as specified
cannot be passed by any well-formed known-good file: a correctly seated TIP is its own
ground (`audit-invariants.md` §3.5's clause (ii)), so no TAPER/MEMBER/FLARE ever
satisfies role R2 ("removing it disconnects a live TIP from ground" — nothing routes
*through* them to reach ground), R3 needs a second live member sharing the same route
(none exists here, one arm per tip), and R4 is raft-only (none here).

**Measured:** `16 checked, 0 failed, 12 indeterminate` — the 4 TIPs are `live` (roles
r1=true), and every TAPER/MEMBER/FLARE (12 records) reports:

```
GEOM_INDETERMINATE file=.../spread_wide.chitubox model=0 record=<r> reason=path_indeterminate
```

for `r` in `{1,2,3, 5,6,7, 9,10,11, 13,14,15}` — i.e. every non-tip record.

**Refinement to the prediction:** §5.1 predicted these would resolve as `dead` (by role
analysis). The measured reason is `path_indeterminate`, the same reason family I3 reports
(§5.2) — suggesting I8's liveness evaluation for non-TIP records shares the same
path-establishment step I3 uses, and that step is what is failing, not a role
determination reaching `dead`. The *outcome* §5.1 predicted (I8 refuses every
well-formed KG-1-shaped file) is confirmed; the *mechanism* looks closer to I3's own gap
(§5.2) than to the role-exhaustion argument in §5.1's own text. Classification:
**(b) modelling gap**, per `audit-known-good.md` §8(a)/(b)/(c) — I8's grounding-by-role
model has no route for "a correctly overlapping, fully-grounded arm chain with exactly
one tip and no bracing," which is the overwhelming common case, not an edge case.

### 5.2 I3 min section — **not predicted**, the main finding of this fixture

`audit-known-good.md` §3.1 predicted I3 would **pass** (arm section `0.75mm² = 2.25×
A_req` at every post, comfortable margin). Measured:

```
MIN_SECTION_INDETERMINATE file=.../spread_wide.chitubox model=0 contact=0  reason=path_open
MIN_SECTION_INDETERMINATE file=.../spread_wide.chitubox model=0 contact=4  reason=path_open
MIN_SECTION_INDETERMINATE file=.../spread_wide.chitubox model=0 contact=8  reason=path_open
MIN_SECTION_INDETERMINATE file=.../spread_wide.chitubox model=0 contact=12 reason=path_open
```

`path_records` for every contact is `[<the TIP record's own index>]` — a one-element
list — and `primitive_record` is `null`: I3's own path-walk never advances past the TIP,
on all four contacts, in both variants tested (§5.2.1).

**This is not explained by the z_plate bug in §4.** I confirmed on the *broken* build
(§4, before the fix) that I3 failed identically; after the fix, with I5 now reporting the
whole file `0 orphaned, 0 free ends` (fully closed and grounded, both via plate and via
seat), **I3 still reports `path_open` on all four contacts, unchanged.**

#### 5.2.1 A/B evidence: I5 and I3 disagree on the identical, correctly-connected file

I built one throwaway diagnostic variant (never shipped, §2) identical to this fixture
except for one addition: a raft pad (type 6) touching each flare's own end point. Result:

| | no-raft (this fixture) | +raft (diagnostic only) |
|---|---|---|
| I5 closed load path | **PASS**, 0 free ends | PASS, 0 free ends |
| I3 min section | FAIL, `path_open`, `path_records=[tip]` only | **FAIL, identically** — `path_open`, `path_records=[tip]` only |
| I8 no dead geometry | FAIL, 12 `path_indeterminate` | FAIL — 4 raft pads now `dead` (role R4 exhausted), the 12 non-tip records still `path_indeterminate`, unchanged |

Adding a raft changes I5's grounding path (as `audit-known-good.md` §2.2 predicted it
would *not* need to) not at all in this case — I5 already passed without one, once
`z_plate` was reconstructed correctly (§4) — and changes I3 **not at all**. This rules
out "the file's load path is genuinely open" as the explanation: I5, whose own
specification (`audit-invariants.md` §3.5) is exactly "every primitive lies in the
connected component of the ground set, no free ends, acyclic to ground," reports the
identical byte-for-byte support geometry as fully closed. I3 disagrees with I5 on what
should be the same underlying graph.

**One cross-check against the real corpus:** `exports/make_box-first-artifact.chitubox`
has at least one contact I3 *does* measure (`contact 8`, `path_records: [8, 9, 26]`,
`status: "pass"`, `a_min_mm2: 0.4471`) — so I3 is not universally inert, and whatever it
needs to advance past a TIP, some real geometry provides it and this fixture's does not.
I did not open `src/checks/*.rs` or `src/graph.rs` to find the exact mechanism (out of
scope per §2/Trap 1) — the finding is the measured, reproducible disagreement between two
checks that `audit-invariants.md` describes as sharing one connection graph, not a
diagnosis of graph.rs's internals.

**Classification:** per `audit-known-good.md` §8, this is either **(b) a modelling gap**
(I3 uses a materially different, stricter connectivity test than I5's documented
solid-overlap graph — plausibly a coincident-endpoint test that R1's *mandatory*
≥0.10 mm joint overlap never satisfies, since that overlap is deliberately not zero) or
**(c) an instrument defect** (I3's implementation does not reuse the graph
`audit-invariants.md` §3.5 specifies, or has a bug walking it). I cannot distinguish (b)
from (c) without reading the forbidden files, and the task does not require me to.
**What is established, measured, and reproducible: a support chain built exactly to the
letter of R1 (mandatory overlap joints, not touching joints), fully connected and
grounded by I5's own accounting, cannot have its narrowest section measured by I3 at
all — not "measured and found short," not "measured with a value" — never measured.**
If this generalises (§5.2's corpus cross-check suggests it does not fire universally, but
also that it is not rare), a large share of the corpus's 851 `MIN_SECTION_SHORT` +
many-hundred `MIN_SECTION_INDETERMINATE` count (`docs/evidence/audit-baseline-hardened.md`)
may be measuring an artefact of how joints happen to be encoded rather than a property of
the arm section — worth the owner's attention before trusting I3's headline number
anywhere in the corpus.

---

## 6. Reproducing

```
mkdir -p /tmp/gen_spread_wide/src && cd /tmp/gen_spread_wide
cat > Cargo.toml <<'EOF'
[workspace]
[package]
name = "gen_spread_wide"
version = "0.1.0"
edition = "2021"
[dependencies]
plinth-audit = { path = "/absolute/path/to/tools/plinth-audit" }
EOF
cp /absolute/path/to/fixtures/spread_wide/generate.rs src/main.rs
RUSTC_WRAPPER="" cargo build --release
./target/release/gen_spread_wide out.chitubox
cmp out.chitubox /absolute/path/to/fixtures/spread_wide/spread_wide.chitubox
# -> byte identical (confirmed 2026-08-31)
```

The generator also writes `out-model.stl` and `out-supports.stl` (ASCII STL, local
bbox-centred frame, no translate applied) — `spread_wide-model.stl` and
`spread_wide-supports.stl` ship alongside this README so the geometry can be inspected in
any STL viewer, independent of ChituBox (`audit-known-good.md` §7.4).

### ChituBox openability

`generate.rs` splices a 396-byte all-zero settings blob after the 24-byte header and
shifts every downstream absolute offset to match (`docs/specs/chitubox-format.md` §1/§6;
`audit-known-good.md` §7.2), so the shipped file carries `settings_size=420` like a real
populated project rather than the `0`/`off8=24` a raw `synth::SynthFile::to_bytes()`
produces. This extends §7.2's own list of fields to shift with two it does not name but
that `reader.rs`'s own parsing requires: the support index's `records_off` (validated
against `index_off + 16 + 400`, so leaving it unshifted plants a mismatch of the audit's
own making) and each individual support record's own `mesh_off` (each record's soup
triangles are themselves data that physically moved). **I could not visually confirm the
file opens correctly in ChituBox** — this environment has no way to drive or screenshot
the GUI app — so this remains the one item in `audit-known-good.md` §7.2 the owner's own
inspection, not this build, must settle. If it does not open, the STLs above and this
README's per-record table are the fallback the owner can use to judge the geometry
directly.

---

## 7. For the owner

*A 4 mm square slab, 2 mm thick, held 4 mm off the plate on four posts — one under the
middle of each quarter of its underside. Each post carries a 2 mm × 2 mm patch: 4 mm².
The measured rule says an arm must be at least a twelfth of what it carries, so
0.333 mm² at its thinnest point. The ball on top of each post is buried about 0.195 mm
into the slab, cutting a circle 0.48 mm² across before the printer's own pixel grid
erodes it — measured, not estimated, at 0.406 mm², still 22% more than required. The
post above the ball is 0.75 mm² at its narrowest, more than twice what it needs. The
furthest any point of the underside sits from a post is about 1.05 mm against a measured
allowance of 1.34 mm. The four posts sit far enough apart (1.0 mm clear gap between
adjacent arms) that nothing crowds.*

*The tool agrees with eight of its ten checks on this file — including the ones about
seat depth, contact area, load bookkeeping, connectivity, no illegal overlap, island
coverage, span, and side-to-side stability. Two checks (I3 "narrowest section" and I8 "no
dead support") refuse this file, but not because anything measured is too thin or too
weak — both report that they could not establish a measurement at all on a perfectly
ordinary, fully-connected support chain. Section 5 above is the detailed account; the
short version is that I3 appears not to recognise a support joint built the way this
project's own rules require it to be built (overlapping by a deliberate margin, not just
touching), and if that is right, I3's headline numbers across the whole 31-file corpus
may need a second look before anyone trusts them at face value. If the audit refuses this
file, on I1, I2, I4, I5, I6, I7, I9, or I10, that would be new information worth
re-examining this README's arithmetic for — I could not find an error in it. On I3 and
I8, the tool is refusing a file I am confident is correct, and that refusal, not the
file, is the thing I'd want your read on next.*

**Not certain, flagged rather than settled:** whether §5.2's I3 finding generalises to a
meaningful share of the corpus's I3 failures, or is specific to this file's exact joint
construction — I did not run I3 across the corpus to check (out of this fixture's scope,
and it would need the forbidden files to interpret). Whether the raft-less flare
construction that now correctly grounds via the plate (§4) is itself an acceptable
practice for files you will print — no plate-adhesion criterion exists anywhere in the
project (`audit-known-good.md` §9, question U2), and I have not tested printing one.
