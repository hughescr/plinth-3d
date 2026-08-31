# Known-good files — the fixtures that prove a check can say yes

Status: **DESIGN, not built.** This document specifies five `.chitubox` files (plus two
conditional probes) whose correctness a human can confirm by looking at them, and which
the audit is then asked to agree with. Nothing here is built; a later agent builds from
it. Where this document and an audit check disagree about a file, that disagreement is
the *result* the exercise is hunting, and §8 says what to do with it.

---

## 0. Why this exists, and the one sentence that governs every design decision

Eleven fixtures exist under `tools/plinth-audit/fixtures/` and **every one is a
deliberately broken file**. All eleven expectations are `check_fails`; the harness
supports `check_passes` and nobody has used it
(`docs/evidence/audit-baseline-hardened.md` §3.4). So the tool has never once been asked
to say *yes*, and the headline — **0 of 31 files passes all ten support invariants** —
cannot be read. It is consistent with "the corpus is bad" and equally consistent with
"a check refuses everything", and nothing in the repository separates the two.

The owner's own reading, which corrects an overstatement in earlier documents, is that
the headline is probably too harsh:

> *"my visual inspection doesn't say that 0/31 will actually print — I think some of
> them, especially the smaller/simpler ones might well print — just that many of them
> are badly constructed and that on many of them, some parts of the print might fail."*

And the constraint that governs this whole document:

> *"I should probably examine the synthetically good files to confirm that they are
> indeed good and not just that **you** think that they're good! We don't want to have
> fixtures which confirm bad patterns!"*

**So a known-good file is not "a file the audit passes". It is a file whose correctness
a human can confirm by looking at it, and which the audit then agrees with.** The
owner's inspection is the acceptance test. The audit's verdict is the *measurement being
taken*, not the standard being met.

Three consequences, and they shape every choice below.

1. **Every file is small enough to hold in your head.** A slab on four posts. A plate on
   a nine-post lattice. A block with a recess and posts inside it. Not a miniature.
2. **Every number in every file is derived by hand, from `docs/physics.md` and the
   printer profile, in this document, before anything is built.** Each file's README
   repeats that derivation so a reviewer can re-do the arithmetic rather than accept it.
3. **Where I am not certain a design is correct, it says so and asks for a ruling**
   (§9). A known-good fixture that is subtly bad teaches the tool to accept a bad
   pattern permanently, and that is the worse of the two failures available here.

---

## 1. What a correct support is — derived from physics, not from the checks

Everything in §2 and §3 follows from these five facts. None of them is read from a
check's source; §7.2 lists what I did and did not open.

### 1.1 The load law (`docs/physics.md` §2)

```
X = A_supported / A_arm_cross_section          both mm², X dimensionless
X* ∈ [25.00, 25.34]                            Measured, plates 5 and 6
X_ALLOWED = floor(25.00 / 2) = 12              Derived, a factor of two held back
```

so for one contact carrying a Voronoi cell of down-face area `A_cell`, the **required
minimum section anywhere on its load path** is

```
A_req = A_cell / 12
```

and by `physics.md` §2(b) that requirement binds at the arm's *narrowest* section, not
only at the contact.

### 1.2 The scar invariant (`physics.md` §3), which fixes the tip radius

`Scar / A_face = 1/12 = 8.33 %` however you tile it. Combined with the measured fact
that the engine sizes every tip to exactly `X = 12`
(`audit-invariants.md` §2.3, max deviation 7e-15), the design convention for every file
here is:

```
r_tip = sqrt( A_cell / (12 π) )                 exact X = 12 sizing
```

This is not a convenience. `audit-invariants.md` §3.4 clause (d) compares
`Σ 12 π r_tip²` against the reconstructed down-face demand `D_total` with a tolerance of
`max(4096·eps64·D_total, 1e-9) mm²`. **Any tip sized with margin makes the books
disagree and fails I4.** So the margin has to be bought somewhere else — and §1.3 is
where.

### 1.3 The seat, and what `ball_seat_factor` actually means geometrically

The emitted tip is a ball of radius `R = f·r_tip`, `f = 1.5`, buried by `h` measured
from its deepest pole. The cut disc it delivers is

```
a(h) = sqrt(2·f·r_tip·h − h²)
```

and the ruled requirement (`audit-invariants.md` §3.1) is

```
h_req = max( 0.100,  0.050 + r_tip·sin θ,  ball_seat_factor(f)·r_tip )
ball_seat_factor(1.5) = 1.5 − sqrt(1.25) = 0.3819660113
```

**Solve `a(h) = r_tip` and you get `h = ball_seat_factor·r_tip` exactly.** That is what
the ball term *is*: the depth at which the ball's cut disc equals the tip circle. Put
that together with §1.2 and a fact falls out that this project should know:

> A tip sized to exactly `X = 12` and seated at exactly the ruled ball-term depth
> delivers an **ideal** disc of exactly `A_req` — and therefore a **cured** disc smaller
> than `A_req`, because the printer's full-containment raster erodes it. Such a contact
> fails I2 by the size of the erosion, every time, by construction.

That is a hypothesis about why I2 fails so widely on the corpus (306 failed, 982
indeterminate); it is stated as a hypothesis, it is not measured here, and it is not
this document's job to settle. What it *does* settle is the design rule for these files:
**buy the load-law margin with seat depth, never with tip radius.** Every file uses

```
h = β · r_tip,        β = 0.60 (KG-1, KG-3, KG-4, KG-5),  β = 0.54 (KG-2)
a(0.60·r) = 1.2 · r_tip      →  A_ideal = 1.44 · A_req
a(0.54·r) = 1.1525 · r_tip   →  A_ideal = 1.33 · A_req
```

`β = 0.60` sits 57 % above the binding ruled minimum at these radii and stays far below
`f = 1.5` (the ball centre must stay outside the model, §2.4 rule R3).

### 1.4 The maximum support pitch on a flat down-face — derived from the span constant

`audit-span-criterion.md` §2.6 derives, from plates 1 and 4:

```
L_SPAN_ALLOWED_MM = 2.1300322500 / 1.53 − 2·d2 = 1.3449762463 mm
```

Take a square lattice of pitch `s` on a flat horizontal down-face, tips sized to exactly
`X = 12` (so `A_cell = s²`, `r_tip = s/sqrt(12π)`, `a = 1.2·r_tip = 0.195441·s`). The
worst point in a cell is its corner, at `s·sqrt(2)/2` from the tip axis, and the carrier
it must reach is the cured tip itself:

```
reach_max(s) = 0.707107·s − 0.195441·s = 0.511666·s  ≤  1.3449762463
        →  s ≤ 2.628623 mm            (cell 6.9097 mm²)
```

Under the criterion's own 8-neighbour chamfer metric, which overestimates a diagonal
path (§2.4 rule R7), the same calculation gives `s ≤ 2.4139 mm`. For a hexagonal lattice
— `physics.md` §3's recommendation, 23 % fewer points for the same worst-case gap — the
same derivation gives `p ≤ 3.4009 mm` and cells of 10.0168 mm², i.e. **45 % fewer
supports for the same span limit.** All the files here use a square lattice anyway,
because a square lattice's Voronoi cells are exactly equal by symmetry and therefore
hand-checkable; the hex gain is noted, not taken.

Every pitch in §3 is quoted as a percentage of the 1.3449762463 mm allowance under the
chamfer metric, so "how close to the limit is this file" is a number and not an
impression.

### 1.5 Where the supports may stand (`physics.md` §8)

Plate, model surface (including a cavity's own interior), or another support. Four of
the five files stand every support on the plate, which is the simplest closed load path
and the one the owner can see at a glance. **A support seated on the model is
deliberately held back to a conditional probe (KG-7), because the audit as specified
cannot pass one — see §5.3.**

---

## 2. Rules every file obeys

### 2.1 The sizing procedure, in five lines

For each file: choose the down-face, choose a symmetric support lattice whose Voronoi
cells are exactly equal, then

```
1.  A_cell   = A_face / N                       exact, by symmetry
2.  A_req    = A_cell / 12                      physics.md §2
3.  r_tip    = sqrt(A_cell / (12π))             exact X = 12  (12π = 37.6991118431)
4.  h        = β · r_tip                        β = 0.60, or 0.54 for KG-2
5.  r_arm    = 1.5 · r_tip                      flare fast above the tip, physics.md §2(b)
```

Step 5's section is `π·(1.5·r_tip)² = 2.25·A_req`, so the arm is never the binding
section; the binding section is the cut disc at the seat, which is what I2 and I3 both
grade, and it stands at `1.44·A_req` ideal.

### 2.2 The support chain, four records, identical in every file

| # | type | from → to | radii |
|---|---|---|---|
| 1 | 7 TIP | `p0 = (x, y, z_face + h)` → `p0 + 2R·u`, `u = (0,0,−1)` | `r1 = r2 = R = 1.5·r_tip` |
| 2 | 1 TAPER | ball centre `p0 + R·u` → node 0.50 mm below | `r_tip → r_arm`; trailing slot 0 = the delivered depth |
| 3 | 3 MEMBER | node + 0.10 mm (into the taper) → `z_plate + 0.90` | `r_arm` constant |
| 4 | 4 FLARE | `z_plate + 1.00` (into the member) → `z_plate` | `r_arm → 0.90 mm` |

`u` is the outward axis, so `p0` — the ball's *deepest* point — sits `h` **above** the
down-face inside the model, and the ball's buried part is a dome of height `h`.

**No type-6 raft records.** A flare landing on the plate is grounded by
`audit-invariants.md` §3.5 clause (i) on its own, needs no pad, and avoids a known
fixture-tool defect: `synth::SynthRecord::with_matching_soup` returns an *empty* soup for
`RecordKind::RaftPadOrBridge` by its own documented contract, which
`i8_no_dead_geometry` correctly reports as `GEOM_DEGENERATE reason=empty_soup` on at
least three existing fixtures. Whether the owner wants rafts on files he inspects is
question **U2** in §9.

### 2.3 Frame

The model mesh is bounding-box-centred. `z_plate = −translate_z`, and every flare's
bottom disc sits **exactly** at `z_plate`, so the reconstructed plate and the declared
lift agree by construction. No support geometry ever goes below `z_plate`.

### 2.4 Eight rules, each with the physical reason it exists

| | rule | why |
|---|---|---|
| R1 | Every joint **overlaps** by ≥ 0.10 mm along the axis | `audit-invariants.md` §3.5: a separation in `[0, d3]`, `d3 = 0.0552901438 mm`, is fusion-uncertain and fails. Two solids that merely touch are not established to cure as one. |
| R2 | Every tip axis stands ≥ `R` (horizontally) from any other surface | the ball must meet exactly one surface; a side wall inside the ball's own radius makes the entry normal ambiguous, and physically it means the cut disc is clipped. All five files clear this by ≥ 2× |
| R3 | `h < R`, i.e. `β < 1.5` | the ball centre — where the taper starts — must stay **outside** the model, or taper material is buried outside its own seat zone |
| R4 | Tip axis ≥ `a` from the edge of its own down-face | the cut disc must land entirely on the face it is carrying, or the delivered patch is not the disc |
| R5 | Adjacent arms clear each other by ≥ 0.30 mm true surface gap | `FINAL_CLEARANCE_GAP_MM`, `physics.md` §5, Measured (owner, 2026-08-22). Smallest gap in the set is 1.02 mm |
| R6 | Choose `r_tip` with an **even f32 mantissa**, so `R = 1.5·r_tip` is exact in f32 | I1 triangulates `r_tip` three ways (`R/f`, the taper's `r_start`, the fitted soup) and fails on disagreement. Make them bit-identical rather than nearly equal |
| R7 | Every declared value is computed **from the stored f32s**, never from the intended real | the delivered depth is `p0.z − z_face` of two f32s; write *that* into the taper's trailing slot, not the `h` you designed |
| R8 | Soups carry vertices at the axis-aligned extremes (0°/90° azimuths, an equator ring) | `audit-invariants.md` §1.4 requires the soup's bounding box to equal the analytic one; an inscribed polyhedron without extreme vertices is smaller. Also: no zero-area triangles at the poles |

### 2.5 The f32 exactness step, and why it is not optional

I4 clause (d) is an **equality** with a 1e-9 mm² floor: `Σ 12π r_tip²` must equal
`D_total`. Three existing fixtures independently report missing it by 33× to 370×
(`starved_patch` ratio 0.999999974, `bare_island` 1.0000000582, `shallow_seat`
1.0000000363), each attributing it to f32 wire storage. Those were single-contact
models tuned on one knob. With `N ≥ 4` contacts it is reachable:

- `D_total` is a product of f32 edge lengths and is **exact in f64** (two 24-bit
  mantissas multiply into 48 bits).
- Sweep a common radius `r_a` (used by `N−1` tips) over ~10⁵ f32 ULPs; for each, solve
  the last tip `r_N = sqrt((D_total − (N−1)·12π·r_a²)/(12π))` and round to f32.
- One ULP of `r ≈ 0.34` moves `12π r²` by about `7.3e-7 mm²`, so 10⁵ candidates land the
  residual near `1e-11 mm²`, four orders inside the floor.

Cost: the last tip's radius differs from the others in about the seventh decimal —
physically identical, invisible in ChituBox, and it **must be disclosed** in the file's
README. **The achieved residual is a required reported number.** If a builder cannot get
under 1e-9 mm² with a 10⁶-ULP sweep, that is a finding about clause (d)'s tolerance, not
a reason to change the design (§8).

---

## 3. The files

Five files. Common to all: model floats 4.00 mm above the plate on its posts; slab/block
material is a closed axis-aligned box mesh welded by exact f32 bit identity; `12π =
37.6991118431`; the raster figures are estimates from the model in §3.0 and every one of
them is a **prediction the build must replace with a measurement**.

### 3.0 How the `A_rast` predictions were made, and their honesty

`A_rast` is not `π a²`. The printer cures a pixel only when the whole `0.014 × 0.019 mm`
footprint is inside the solid at that layer's sampling plane, and the sampling plane can
sit up to 0.025 mm away from the seat's cut plane, where the ball is narrower. I
estimated both terms numerically (full-containment pixel count on the real pitch, at 16
sub-pixel phases, with the radius taken at the worst sampling plane) and quote the
**worst** of them. Cross-check: the same model applied to the `starved_patch` fixture's
own geometry (`r_tip = 0.1850000024`, `h = 0.1030000001`, the values that fixture stores)
predicts a cured/ideal ratio of **0.7075 at the worst sub-pixel phase and 0.7220 at the
best**, against that fixture's measured **0.7347** — 1.7 % to 3.7 % low, i.e. wrong in the
conservative direction. One cross-check, on one contact, is why the numbers below are
quoted at all and is not a licence to trust them: each file's acceptance requires the
measured value, and question U8 in §9 says exactly that to the owner.

---

### 3.1 KG-1 `kg_slab_four_post` — the trivially correct case

**Geometry.** A 4.00 × 4.00 × 2.00 mm slab, underside 4.00 mm above the plate. Four
identical posts at `(±1.00, ±1.00)`. Bounding-box-centred frame: slab `x,y ∈ [−2,2]`,
`z ∈ [−1,+1]`; `z_plate = −5.0`; `translate_z = 5.0`.

**Arithmetic.** The four axes are the perpendicular-bisector centres of the square, so
the Voronoi partition of the underside is exactly the four 2 × 2 quadrants.

```
A_face  = 16.0000 mm²          D_total = 16.0000 mm² exactly (the sides are vertical,
A_cell  =  4.0000 mm²                    the top faces up; both carry zero demand)
A_req   =  0.333333 mm²        = A_cell / 12
r_tip   =  0.325735 mm         = sqrt(4.0000 / 37.6991118431)
R       =  0.488603 mm         = 1.5 r_tip
h       =  0.195441 mm         = 0.60 r_tip      (h_req = 0.124420 mm → 57 % over)
a       =  0.390882 mm         = 1.2 r_tip
A_ideal =  0.480000 mm²        = 1.44 × A_req
A_rast  ≥  0.405650 mm²        = 1.22 × A_req     predicted, worst phase
X_delivered ≤ 9.86             against X_ALLOWED = 12
r_arm   =  0.488603 mm         arm section 0.750000 mm² = 2.25 × A_req
worst reach 1.0233 mm euclid / 1.1144 mm chamfer = 82.9 % of 1.3449762463 mm
spread: hull = the 2.00 mm square of the four axes, load at its centre, d = 1.000 mm
        against σ ≈ 0.077 mm (σ_q ≤ 0.0653 + σ_H ≈ 1e-5 + d2/2 = 0.0118) → 13 σ
records 16 (4 chains × 4), contacts 4, model layers 40
```

**For the owner.** *A 4 mm square slab, 2 mm thick, held 4 mm off the plate on four
posts — one under the middle of each quarter of its underside. Each post therefore
carries a 2 mm × 2 mm patch of underside: 4 mm². The measured rule says an arm must be at
least one twelfth of what it carries, so 4 ÷ 12 = 0.333 mm² at its thinnest point. The
ball on top of each post is buried 0.195 mm into the slab, which cuts a circle 0.782 mm
across — 0.480 mm², about 1.44 times what is required, and the printer's own pixel grid
is expected to eat that down to about 0.406 mm², still 1.22 times. The thinnest part of
the post above the ball is 0.977 mm across, 0.75 mm², more than twice what it needs. The
furthest any point of the underside sits from a post is 1.02 mm; the measured span
allowance is 1.34 mm. If the audit refuses this file, the tool is wrong, not the file.*

**Checks exercised positively:** all ten are asked (I8 is predicted to refuse — §5.1).

---

### 3.2 KG-2 `kg_slab_min_seat` — correct, and close to the load limit

**Geometry.** Byte-for-byte KG-1 except one number: `β = 0.54` instead of 0.60, i.e. the
ball is buried 0.175897 mm instead of 0.195441 mm. Everything else — slab, lattice, tip
radii, arms — is identical, so any difference in verdict is attributable to that one
change and to nothing else.

```
h       =  0.175897 mm         = 0.54 r_tip     (h_req = 0.124420 mm → 41 % over)
a       =  0.375430 mm
A_ideal =  0.442800 mm²        = 1.33 × A_req
A_rast  ≥  0.367080 mm²        = 1.10 × A_req    predicted, worst phase
X_delivered ≤ 10.90            against X_ALLOWED = 12
worst reach 1.1299 mm chamfer = 84.0 % of the allowance
```

**Why this file exists.** An over-harsh check reveals itself at the limit, not in the
middle. This is a support that genuinely suffices — 10 % of cured patch in hand, and a
delivered `X` of 10.9 against a boundary measured at 25 — and it is the smallest margin
in the set. **The builder tunes `β` by measurement, not by this table**: start at 0.54,
measure `A_rast/A_req`, and land it in `[1.05, 1.15]`. Report the achieved value and the
`β` that produced it.

**For the owner.** *The same slab and the same four posts as the first file, with the
balls pushed in a little less — 0.176 mm instead of 0.195 mm. The circle each one cuts is
0.75 mm across instead of 0.78, and after the printer's pixel grid takes its share the
contact is expected to be about 10 % bigger than the rule demands rather than 22 %. This
is deliberately the thinnest margin in the set: it is the file that will tell us whether
the tool is refusing supports that would actually hold.*

**Checks exercised positively:** all ten, with I2 and I3 near their limits.

---

### 3.3 KG-3 `kg_plate_lattice` — correct, and close to the span limit

**Geometry.** A 6.60 × 6.60 × 2.00 mm plate, underside 4.00 mm above the plate, on a
**3 × 3 lattice at 2.20 mm pitch**: posts at `(x, y)` with `x, y ∈ {−2.2, 0, +2.2}`. The
perpendicular bisectors fall at `±1.1`, so the nine Voronoi cells are exactly the nine
2.2 × 2.2 squares.

```
A_face  = 43.5600 mm² = 9 × 4.84   D_total = 43.5600 mm² exactly
A_cell  =  4.8400 mm²
A_req   =  0.403333 mm²
r_tip   =  0.358309 mm             R = 0.537463    h = 0.214985  (h_req = 0.136860)
a       =  0.429970 mm             A_ideal = 0.580800 mm² = 1.44 × A_req
A_rast  ≥  0.500080 mm²            = 1.24 × A_req, X_delivered ≤ 9.68
r_arm   =  0.537463 mm             arm section 0.907500 mm² = 2.25 × A_req
worst reach 1.1257 mm euclid / 1.2259 mm chamfer = 91.1 % of the allowance
records 36, contacts 9, model layers 40
```

**Why this file exists.** 91 % of the measured span allowance, on a lattice one step
coarser than the derived square-lattice maximum of 2.4139 mm (chamfer) / 2.6286 mm
(euclid). Every arm is comfortably sized; the only thing near a limit is *how far apart*
the arms are. That isolates I9. If I9 refuses this file, read
`report.span_metric_conservatism` first: the chamfer metric overestimates a diagonal path
by up to 12.04 % by its own derivation, and this file's worst path is a diagonal.

**For the owner.** *A 6.6 mm square plate on nine posts in a 3 × 3 grid, 2.2 mm apart.
Each post carries a 2.2 × 2.2 mm square of underside, 4.84 mm², so it needs 0.403 mm² of
section and gets 0.58 mm² of cut circle. What is deliberately tight here is the spacing,
not the posts: the furthest corner of any square sits 1.126 mm from its post, against a
measured allowance of 1.345 mm. Posts any further apart than about 2.4 mm and the
material between them is reaching further than the coupons say it can.*

**Checks exercised positively:** all ten; I9 at 91 % of its allowance.

---

### 3.4 KG-4 `kg_recess_posted` — a roofed recess, carried from inside it

**Geometry.** An 8.40 × 8.40 × 4.50 mm block with a **4.20 × 4.20 mm square recess,
3.00 mm deep, open downward** — so the roof is 1.50 mm thick and the recess drains
straight down. Underside 4.00 mm above the plate.

Support lattice: **4 × 4 at 2.10 mm pitch**, posts at `x, y ∈ {±1.05, ±3.15}`. The
bisectors fall at `0, ±2.10`, so the sixteen cells are exactly the sixteen 2.1 × 2.1
squares — and the recess mouth is **exactly the four inner cells**. So the four inner
posts run 3.00 mm further up, through the recess mouth, and seat into the roof; the
twelve outer posts seat into the rim. Every post is otherwise identical.

```
A_face  = 70.5600 mm² = 16 × 4.41   (rim 52.92 + roof 17.64; the projected demand of the
A_cell  =  4.4100 mm²                block is its full 8.4 × 8.4 footprint either way)
A_req   =  0.367500 mm²
r_tip   =  0.342022 mm              R = 0.513033   h = 0.205213  (h_req = 0.130640)
a       =  0.410426 mm              A_ideal = 0.529200 mm² = 1.44 × A_req
A_rast  ≥  0.451670 mm²             = 1.23 × A_req, X_delivered ≤ 9.76
r_arm   =  0.513033 mm              arm section 0.826880 mm² = 2.25 × A_req
worst reach 1.0745 euclid / 1.1701 chamfer = 87.0 % of the allowance, rim and roof alike
interior post clearance: axis 1.05 mm from the recess wall (R = 0.513, rule R2 ✓);
      arm-to-wall true gap 1.05 − 0.513 = 0.537 mm > FINAL_CLEARANCE_GAP_MM = 0.300 ✓
records 64, contacts 16, model layers 90
```

**Why this file exists.** `audit-span-criterion.md` §5 records that its cavity clause is
**unexercised by the entire corpus** — the engine emits zero hollow files — and that
"a criterion whose cavity clause has never fired is a criterion whose cavity clause has
never been shown to work". This is the file that fires it in the passing direction: a
roof 4.2 mm across, above the 2.6900 mm self-bridging maximum, carried by supports that
come up **inside** the recess exactly as `physics.md` §8 requires. It is also the file
where I7 is structurally blind and I9 is not: the roof at its birth layer is continuous
with the surrounding wall, so it is never a detached island, and only the span criterion
measures it.

**A deliberate choice, and its cost.** The recess is open downward and the interior posts
stand on the **plate**, not on a cavity floor. A sealed cavity would have needed drain
bores to be printable, and bores break the exact-by-symmetry cell arithmetic that makes
every number above hand-checkable. The seated-on-the-model case — `physics.md` §7's
land-at-strike and §8's cavity-interior seat — is therefore **not** exercised by this
file, and §5.3 explains why the audit as specified cannot pass it at all.

**For the owner.** *An 8.4 mm block, 4.5 mm tall, with a 4.2 mm square hole cut 3 mm up
into its underside, leaving a 1.5 mm roof. Sixteen posts on a 2.1 mm grid: twelve under
the rim, and four that run up inside the hole to hold the roof. Each post carries exactly
a 2.1 × 2.1 mm square, 4.41 mm², needs 0.368 mm² and gets 0.529 mm² of cut circle. The
roof is 4.2 mm across, which is more than the 2.69 mm the coupons say material can bridge
unaided — which is precisely why there are posts inside it. The recess opens downward,
so nothing traps resin.*

**Checks exercised positively:** all ten; I9's cavity/roof clause is exercised here and
nowhere else in the set except KG-5.

---

### 3.5 KG-5 `kg_slot_selfbridging` — the hollow the gate must **not** refuse

**Geometry.** The same 8.40 × 8.40 × 4.50 mm block, but the recess is a **2.40 mm-wide
slot running the full length in Y**, 3.00 mm deep, open downward and open at both ends:
material removed at `x ∈ [−1.2, +1.2]`, all `y`. **No interior supports.** The roof spans
2.40 mm against the derived self-bridging maximum of `2 × 1.3449762463 = 2.6899524926
mm`, so it carries itself.

Support lattice: two rim strips, each 3.00 × 8.40 mm, each carrying 8 posts at
`x ∈ ±{1.95, 3.45}`, `y ∈ {±1.05, ±3.15}`. Bisectors at `x = ±2.70` and
`y = 0, ±2.10`, so cells are exactly 1.50 × 2.10 = 3.15 mm².

```
rim cells (8 outer posts, x = ±3.45)   A_cell = 3.1500 mm²
inner cells (8 posts, x = ±1.95)       A_cell = 3.1500 + 20.16/8 = 5.6700 mm²
        the slot roof (2.4 × 8.4 = 20.16 mm² of projected demand) has no contact of its
        own, so the ownership rule of audit-invariants.md §2.2 routes it to the nearest
        contacts — the eight inner posts, 2.52 mm² each, split by their own y-bisectors
D_total = 70.5600 mm² = 8 × 3.15 + 8 × 5.67   ✓
outer:  A_req = 0.262500  r_tip = 0.289061  R = 0.433592  h = 0.173437  a = 0.346873
        A_ideal = 0.378000 = 1.44 × A_req,  A_rast ≥ 0.312820 = 1.19 × A_req
inner:  A_req = 0.472500  r_tip = 0.387816  R = 0.581724  h = 0.232690  a = 0.465379
        A_ideal = 0.680400 = 1.44 × A_req,  A_rast ≥ 0.593180 = 1.26 × A_req
worst rim reach 0.9496 chamfer = 70.6 % of the allowance
slot roof reach 1.2000 mm, axis-aligned so the chamfer metric is exact = 89.2 %
records 64, contacts 16, model layers 90
```

**Why this file exists, and the thing it makes visible.** It is the control against a
gate that refuses every hollow: a roof that needs nothing, at 89 % of the measured
allowance. It also makes a real modelling gap concrete and countable. **The eight inner
posts are fatter than the physics of their own patch requires** — 0.388 mm tips instead of
0.289 mm — purely because the ownership rule charges them a share of a roof that carries
itself. Sized by the physics alone (3.15 mm² cells) they would be refused by I2 and I3
for area they do not carry. That is not a defect in this file; it is the demand model
having no self-support term, and it is question **U7** in §9.

**For the owner.** *The same block with a 2.4 mm slot milled up into its underside
instead of a square hole — open at both ends, so it drains, and narrow enough that its
roof holds itself up: the coupons say material can reach 1.345 mm from whatever carries
it, and the middle of this roof is 1.2 mm from the walls on either side. Sixteen posts
carry the two rim strips. The eight posts nearest the slot are noticeably fatter than the
eight outer ones — not because they carry more of the underside, but because the audit's
bookkeeping hands them a share of the slot roof even though the roof needs nothing. That
oddity is the point of the file, and it needs your ruling.*

**Checks exercised positively:** all ten; I9's self-bridging branch, which nothing else
in the repository exercises at file level.

---

### 3.6 Conditional probes — build only on evidence

| id | geometry | what it probes | build when |
|---|---|---|---|
| **KG-6** `kg_braced_pair` | KG-1 plus one horizontal member joining two adjacent posts at mid-height | whether I8's role R3 ("removing it increases some live member's free span") can make *anything* live under a validly seated tip | KG-1's I8 output shows `GEOM_DEAD_MEMBER` / `GEOM_DEAD_TIP` (§5.1) |
| **KG-7** `kg_landed_foot` | KG-1 with one post replaced by a strut whose foot **lands on the model** rather than the plate | `physics.md` §7 land-at-strike, `physics.md` §8's model seat, I5 ground clause (ii), and I4 clause (d) against a load-bearing contact that owns no demand (§5.3) | after KG-1 and KG-4 are measured; it is expected to fail I4 by construction, and that is its finding |

---

## 4. Coverage matrix

Every gating check must be exercised in its **passing** direction by at least one file,
and must be exercised **non-vacuously** — `checked > 0` and `indeterminate == 0`. A check
that passes having examined nothing is this project's signature defect (§7.1), and the
matrix records the item count each check must report, not just a status.

| check | KG-1 | KG-2 | KG-3 | KG-4 | KG-5 | `checked` must be ≥ |
|---|---|---|---|---|---|---|
| I1 seat depth | ✔ | ✔ limit | ✔ | ✔ | ✔ | 4 / 4 / 9 / 16 / 16 contacts |
| I2 contact area | ✔ | **✔ limit (1.10×)** | ✔ | ✔ | ✔ | same |
| I3 min section | ✔ | ✔ limit | ✔ | ✔ | ✔ | same |
| I4 load ownership | ✔ strict | ✔ strict | ✔ strict | ✔ strict | ✔ strict | 1 model each |
| I5 closed load path | ✔ | ✔ | ✔ | ✔ | ✔ | 16 / 16 / 36 / 64 / 64 records |
| I6 no intersection | ✔ | ✔ | ✔ | ✔ recess | ✔ slot | same |
| I7 island coverage | ✔ supported island at the birth layer | ✔ | ✔ | ✔ | ✔ | ≥ 1 island |
| I8 no dead geometry | **predicted fail — §5.1** | — | — | — | — | KG-6 is the probe |
| I9 unsupported span | ✔ 82.9 % | ✔ 84.0 % | **✔ 91.1 % limit** | **✔ roof, posted** | **✔ roof, self-bridging** | thousands of regions |
| I10 arrangement spread | ✔ 4 contacts, d = 13 σ | ✔ | ✔ 9 contacts | ✔ 16 | ✔ 16 | ≥ 40 body-layer evals |

"Strict" for I4 means the file contains **no clamp-limited contact**, so clause (d) is
tested as an equality rather than as a wide bracket. A file containing a tip at either
clamp (`0.050` or `1.000` mm radius) satisfies clause (d) trivially, because clamped
contacts leave `lo` and add `12π·1.0² = 37.699 mm²` each to `hi`. **No file in this set
takes that escape**, and any future known-good file that does must say so and must not be
counted as evidence for clause (d).

---

## 5. Predicted findings — stated before the build, so they cannot be rationalised after

These are predictions from the specifications and from measured evidence already in the
repository. They are **not measured here.** Each names the measurement that settles it.

### 5.1 I8 is predicted to refuse every file in this set, structurally

`audit-invariants.md` §3.5 grounds a primitive if "it holds a valid seat: its solid
intersects M, its seat zone passes I6, and the contact it belongs to passes I1". So a
correctly seated TIP **is its own ground**. §3.8's liveness roles are then:

- R1 (carries demand) — TIPs only. Our tips qualify.
- R2 (removing it disconnects a live TIP from ground) — cannot fire, because no live TIP
  reaches ground *through* anything.
- R3 (removing it increases some live member's free span) — the only remaining route.
- R4 — type-6 raft only, and there are none.

So every TAPER, and probably every MEMBER and FLARE, is dead by roles. **Corroborating
measured evidence:** `GEOM_DEAD_MEMBER` fires 2,638 times over the 24 real miniatures and
every one of the 31 corpus files fails I8; the `bare_island`, `shallow_seat` and
`starved_patch` READMEs each independently trace `GEOM_DEAD_MEMBER` on their taper to
exactly this mechanism, in those words.

**If that is right, I8 as specified cannot be passed by any well-formed file**, because a
TAPER is mandatory (I1 reads the tip radius and the declared depth from it, and its
absence is `SEAT_DEPTH_INDETERMINATE reason=no_adjacent_taper`). That is a specification
result, not a fixture problem, and the fix is a ruling on what "ground" means for R2 —
plate-and-raft only, or any valid seat. **Measurement that settles it:** KG-1's I8 block.
KG-6 exists only to test whether R3 rescues anything.

### 5.2 I4 clause (d) and the f32 floor

Predicted to be reachable by the ULP sweep of §2.5 and therefore **not** a finding — but
three fixtures have already missed it by 33× to 370×, so the achieved residual is a
required reported number for every file. If a 10⁶-ULP sweep cannot get under 1e-9 mm², the
tolerance is below the resolution of the evidence the file is stored in, and clause (d)
needs re-deriving against `eps32`, not against `eps64`.

### 5.3 I4 clause (d) refuses a landed foot, and therefore refuses `physics.md` §7 and §8

A support seated on the model — a landed strut foot, or the base of a post standing on a
cavity floor — carries load but owns **no down-facing demand** (an interior floor faces
up; `demand(t) = 0`). By I3 its own section must still satisfy the area law, so it must be
load-sized. But a load-sized foot is not clamp-limited, so its `12π·r²` enters clause
(d)'s `lo` while contributing nothing to `D_total`:

```
KG-7-shaped file, one landed foot sized for the same load as its tip:
lo = D_total + 12π·r_foot²  →  for KG-4's numbers, 70.560 + 4.41 = 74.970 vs 70.560
ratio 1.0625, four orders outside the tolerance → LOAD_BOOKS_DISAGREE
```

Sizing the foot at the minimum clamp (0.050 mm radius) removes it from `lo` and passes
clause (d) — and then I3 refuses it, correctly, because a 0.1 mm-diameter foot cannot
carry the load the arm above it carries. **The two clauses cannot both be satisfied by any
landed foot.** That is why KG-4 stands its interior posts on the plate, and why KG-7 is a
probe rather than a known-good file. It needs the owner's ruling (question **U3**).

### 5.4 A model resting on the plate is charged for its own footprint

Nothing in the demand set exempts a down-face that sits on the build plate: `demand(t)`
depends only on the normal. A model standing on the plate therefore presents its whole
footprint as demand, which I4 routes to the nearest contact and I2/I3 then require that
contact to carry. **All five files float**, so none of them exercises it, and the corpus
does not either (the engine lifts every model). It is named here so it is not discovered
later as a surprise.

### 5.5 The zero-margin-by-construction observation

§1.3: a tip sized to exactly `X = 12` and seated at exactly the ruled ball-term depth
delivers an ideal disc of exactly `A_req` and therefore a cured disc *below* it. If the
engine seats at or near the ball term, I2's corpus-wide failures are partly a property of
the sizing convention rather than of individual files. **Unverified.** The measurement
that settles it is a histogram of `h_delivered / (ball_seat_factor · r_tip)` over the
corpus, which the audit already has the inputs for.

---

## 6. The two traps, named

### Trap 1 — a file built by reading the audit's code passes by construction and proves nothing

Every dimension, radius and depth in §3 is derived in §1 and §2 from `docs/physics.md`
(the area law, the scar invariant, hexagonal seeding, the clearance ruling, the three-seat
rule) and from the two documents that carry the *measured* printer profile and coupon
results (`audit-invariants.md` §1.3/§1.5 for the pitches, layer height, tip-radius range
and derived tolerances; `audit-span-criterion.md` §2.6 for `L_SPAN_ALLOWED_MM` and its
four inputs). Those are the project's physics documents, not the instrument's
implementation.

**What I deliberately did not open:** `src/checks/*.rs` — all ten checks, including
`contact_area.rs`, `narrowest_section.rs`, `load_ownership.rs`, `island_coverage.rs`,
`unsupported_span.rs`, `arrangement_spread.rs`, `dead_geometry.rs`, `interval.rs` — and
`src/seat.rs`, `src/ownership.rs`, `src/raster.rs`, `src/graph.rs`. No threshold, margin
or geometric choice above was reverse-engineered from a check.

**What I did open, and why it is disclosed rather than hidden:**
`tests/fixtures.rs` (the harness contract — §7.1 asks for a change to it),
`src/synth.rs`'s module docs and writer (the byte-level build API these files must use),
`docs/specs/chitubox-format.md` (the container), the eleven fixture READMEs and
`expect.json` files, `docs/evidence/audit-baseline-hardened.md`, and one generator,
`fixtures/collinear_arms/generate.rs`. That generator's comments describe how `src/seat.rs`
computes its mean-normal cross-check. **I used that only as a build hazard, and rule R2 in
§2.4 is derived independently**: the cut disc must meet exactly one surface, so no other
surface may lie within the ball's own radius. The two agree; the rule stands on the
geometry, not on the code.

### Trap 2 — a known-good file that is subtly bad teaches a bad pattern permanently

This is the owner's stated fear and the worse of the two failures. Mitigations, in force:

1. Every file's README repeats the full derivation in the plain-language form of §3, so
   the owner is checking arithmetic he can follow rather than trusting a verdict.
2. Every uncertain choice is listed in §9 as a question, not buried as a decision.
3. No file takes a bookkeeping escape hatch that a physical support would not take —
   in particular no clamp-limited tips (§4) and no fixture-specific tuning of any
   threshold.
4. Where a design does something a physical support would not — KG-5's eight over-fat
   inner posts — the README says so in the owner's own terms and asks for the ruling.
5. **No file is adjusted to make a check pass.** §8.

---

## 7. Building them

### 7.1 The harness needs one change, and it is not optional

`tests/fixtures.rs` binds **one expectation to one artifact** (`<name>.expect.json` →
`<name>.chitubox`) and `expect` takes `check_fails`, `check_passes` or `file_fails`. A
known-good file needs *all ten* checks to pass, and `check_passes` asserts only a status —
so a check that examined **zero items** satisfies it. That is exactly the shape of the
defect that made the harness report green having discovered zero fixtures.

Required, from whoever owns `tests/fixtures.rs` (**not this document's author, and not the
fixture builder**):

```jsonc
{
  "description": "...",
  "expect": "all_checks_pass",         // NEW: every gating check must report "pass"
  "min_checked": {                     // NEW: and must have examined at least this many
    "i1_seat_depth": 4, "i2_contact_area": 4, "i3_min_section": 4,
    "i4_load_ownership": 1, "i5_closed_load_path": 16, "i6_no_intersection": 16,
    "i7_island_coverage": 1, "i8_no_dead_geometry": 16,
    "i9_unsupported_span": 1, "i10_arrangement_spread": 40
  },
  "accepted_deviations": [             // NEW: enumerated, never silent
    { "check": "i8_no_dead_geometry", "finding": "KG-F1", "ruled_by": "<owner, date>" }
  ]
}
```

`indeterminate` must be zero for every check on a known-good file: an unestablished item
is a failure by standing rule 3, and a known-good file that produces one is telling us
something. `accepted_deviations` exists so that a predicted tool finding (§5) does not
force the suite red for weeks *and* does not disappear: each entry names a finding id and
requires an owner sign-off, and the count of accepted deviations is a headline number in
the next baseline.

### 7.2 ChituBox openability is an acceptance criterion, not a nicety

The owner inspects in ChituBox, so a file he cannot open is not a known-good file.
**Measured facts about the current fixtures:** their headers carry `off8 = 24` and
`settings_size = 0` (`xxd` on `long_ledge.chitubox` and `bare_cavity_ceiling.chitubox`:
`00000000: 4312 23ab 0100 0000 1800 0000 0000 0000`), i.e. no settings blob and no preview
region. A real emitted file carries `settings_size = 420` and `off8 = 27416`
(`printable-chitubox-minis/Round 25.chitubox`), the gap being the 396-byte settings blob
plus a preview blob. `docs/specs/chitubox-format.md` §2 pins both forms: `0` in an empty
project, `420` in every populated one.

So the build must, per file:

1. Emit with `synth`, then splice: insert 396 zero bytes after the 24-byte header, set
   header field 12 to `420`, and add `396` to `off8`, `off20`, every model record's
   `mesh_off` and `support_index_ptr`, and each support index's own `mesh_off`.
2. **Open the first file in ChituBox before building the other four.** Confirm the model
   and the supports appear as separate objects, as they do for a real emitted file.
3. If it does not open, copy a preview blob verbatim from a corpus file into
   `[420, off8)` and shift every offset by `396 + preview_len` instead. If it still does
   not open, stop and report — do not build five files nobody can inspect.
4. Ship `<name>-model.stl` and `<name>-supports.stl` beside every fixture regardless, plus
   a dimensioned sketch in the README, so the inspection does not depend on one program.

### 7.3 Build order, because information beats throughput

**Build KG-1 first. Run the full audit on it. Publish its ten-check output before
building anything else.** KG-1 and KG-2 differ in one number; KG-3, KG-4 and KG-5 reuse
KG-1's entire support recipe. If KG-1 fails I8 and clause (d) for the structural reasons
of §5, the other four will fail identically and building them first buys nothing but four
copies of the same finding.

### 7.4 What every file's README must carry

| item | why |
|---|---|
| The derivation of §3 for that file, re-computed, not copied | the reviewer must be able to re-do it |
| The owner's plain-language paragraph | it is the acceptance test |
| Measured `A_rast`, `A_min`, `reach`, `d`, `σ` per contact, read back from the audit's own JSON | §3's numbers are predictions; the README carries measurements |
| The achieved I4(d) residual in mm² and its ULP sweep width | §2.5 |
| sha256 and byte count of the artifact | as the existing fixture READMEs do |
| Every check that did **not** pass, with its failure line, unedited | §8 |
| The predicted-vs-measured table, both columns, with any disagreement > the stated tolerance called out | a prediction that missed is a finding about §3 or about the tool, and either way it must not be quietly overwritten |

---

## 8. What happens if the audit fails a known-good file

**This is the outcome the exercise is hunting.** With 0 of 31 real files passing, a
refusal of a file whose correctness a human has confirmed is the single most informative
result available, and the procedure exists so it cannot be quietly disposed of.

1. **The file is not adjusted to make the check pass. Ever.** Not the seat depth, not the
   pitch, not the tip radius, not the tolerance, not the constant. A fixture tuned until
   the instrument agrees with it measures nothing.
2. **Re-derive by hand.** Compare the failure line's measured quantity against this
   document's predicted one. If they disagree by more than the stated tolerance, the most
   likely explanation is a **build defect** — check the bytes, the frame, the joint
   overlaps and the declared trailing values before suspecting the check.
3. **If the file is confirmed correct and the check still refuses, classify the finding:**
   - *(a) over-harsh threshold* — the check's constant or tolerance refuses geometry the
     physics permits (candidates named in advance: I2 against the full-containment fill
     rule, I9 against the 1.53× condition-shift discount and the chamfer overestimate,
     I4 clause (d)'s 1e-9 floor);
   - *(b) modelling gap* — the check measures a different quantity than the physics does
     (candidates: I4's demand model has no self-support term, §5.4 and U7; I8's liveness
     read against I5's ground rule, §5.1);
   - *(c) instrument defect* — the check does not compute what its own specification says.
4. **Write it up as a finding against the tool**, with the failure line verbatim, this
   document's derivation, and the measured margin. Register it with an id (`KG-F1`, …)
   and reference that id from the fixture's `accepted_deviations` (§7.1) so it is
   enumerable rather than absorbed.
5. **Only the owner relaxes a physics-derived threshold**, and only against evidence. A
   constant is never moved to make a fixture green. If a constant should move, it moves
   in its own document with its own derivation, and every corpus number that depends on it
   is re-run.
6. **Report the count.** "N of 5 known-good files are refused, on checks X and Y" belongs
   in the next baseline beside "0 of 31", because the two numbers only mean anything
   together. If N is 0 and all ten checks report non-vacuous passes, the 0-of-31 headline
   is corroborated. If N is 5, the headline says more about the instrument than about the
   corpus.

---

## 9. Open questions — the owner's ruling, not the builder's judgement

Flagged per Trap 2: these are places where I am **not certain** the design is right, and
presenting them as settled would be exactly the failure mode the owner named.

| id | question | why I cannot settle it |
|---|---|---|
| **U1** | Is `β = 0.60` (a ball buried 0.6 × its tip radius, ~1.6 × the ruled minimum) acceptable practice? It buys the whole I2 margin and it enlarges the scar. | The alternative — the ruled ball-term depth — leaves *zero* margin against raster erosion (§1.3). Nothing measured says which side to err on |
| **U2** | Posts landing on the plate through a conical flare, with **no raft pad**. Acceptable for the files you will inspect? | No plate-adhesion criterion exists anywhere in the project; a foot area is a chosen number. Rafts also currently trip a fixture-tool defect (§2.2) |
| **U3** | A landed foot — a support seated on the model — cannot satisfy I3 and I4(d) simultaneously (§5.3). Which gives: exempt zero-demand seated contacts from clause (d)'s `lo`, or something else? | It is a conflict between two specification clauses, both of which cite your rulings |
| **U4** | KG-2's ~10 % cured-patch margin: does that count as "genuinely suffices" in your judgement? | You have printed these; the audit has not. 10 % of a quantity whose erosion model is estimated is exactly where your field experience beats my arithmetic |
| **U5** | My lattices are 2.0–2.2 mm pitch; the corpus median contact spacing is **1.03 mm**, with one pair at 0.049 mm. My files are far sparser than anything the engine emits. Sparse is what the physics permits — is it what you would print? | `report.spacing` is disclosed, never gated, so nothing in the tool has an opinion |
| **U6** | KG-4 and KG-5 use recesses open downward rather than sealed cavities, so they drain. That means **no file here exercises a sealed cavity's separate surface component**, which is predicted to fail I4 (`LOAD_COMPONENT_UNSUPPORTED`) even when I9 says its roof is fine. Worth a sixth file? | A sealed cavity is not printable without drain bores, and bores break the exact-by-symmetry cell arithmetic |
| **U7** | KG-5's eight inner posts are ~34 % fatter than their own patch requires, because ownership charges them a share of a roof that carries itself. Should the demand set have a self-support term, so that area within `L_SPAN_ALLOWED_MM` of a carrier is not charged to anybody? | It is a physics-modelling question about how peel load reaches a support, and it would change I2, I3 and I4 together |
| **U8** | Every `A_rast` in §3 is a **prediction** from a numerical erosion model that agrees with the one measured fixture to 1.2 %. One data point. | Only the build measures it. If the measured values come in below §3's, the design margins move, not the checks |

---

## See also

- [`../physics.md`](../physics.md) — the area law, the scar invariant, the clearance and
  seating rulings, and §11's statement of where the model stops being valid
- [`audit-invariants.md`](audit-invariants.md) — I1–I8, the declared inputs, the demand
  set, the ownership partition, and the output schema
- [`audit-span-criterion.md`](audit-span-criterion.md) — I9, `L_SPAN_ALLOWED_MM`, and §5's
  cavity clause, unexercised by the corpus
- [`audit-spread-criterion.md`](audit-spread-criterion.md) — I10, the body definition, the
  lines-of-action predicate, and `sigma`
- [`audit-interval-policy.md`](audit-interval-policy.md) — which bound is conservative in
  which direction
- [`audit-connectivity-rule.md`](audit-connectivity-rule.md) — what makes two cured
  regions one region. Every file here is deliberately insensitive to it: every cured
  region in the set is a solid rectangle or disc tens of pixels across, and nothing in
  the set is joined to anything by a diagonal or single-pixel bridge. If a known-good
  file's I7/I9/I10 region counts move when that rule lands, that is a finding
- [`../evidence/audit-baseline-hardened.md`](../evidence/audit-baseline-hardened.md) —
  0 of 31, the per-file table, and §3.4's statement that not one of the eleven
  expectations is a negative control
