# Load Model Design — Physics First

Status: DESIGN. No production behaviour changes with this document. Companion designs
exist from two other stances; this one starts from the forces and derives everything
downstream. Branch: `design/load-physics-first`.

Audience: the project executive, who has not read the code. Every term is explained at
first use. Every tip size is stated as RADIUS and as DIAMETER, because the code speaks
radius and the slicer UI speaks diameter, and confusing them lands 2x wrong.

---

## 0. Summary of the design in six sentences

A support carries **peeled area**: the area, in mm², of the worst single layer of model
material whose only path to the build plate runs through that support. That area is
computed layer-by-layer over a raster at fixed physical resolution, so it cannot depend
on how the mesh happens to be triangulated. The model's own material is treated as a
load path under the same measured law as a support arm — it is the same green resin —
so a region "hands off" to the model exactly when the model's connecting cross-section
is large enough, and demands supports exactly when it is not. Placement is driven by
that demand: supports are placed where and when (in layer order) unsupported peeled
area appears, densely enough that no support's share exceeds what its largest legal tip
can carry. Sizing is the measured area law applied at the narrowest cross-section of
the load path, using the registered design ratio directly, with clamp violations
refused rather than silently truncated. Where the measurement stops — position on the
plate, cells smaller than the smallest coupon cap, solid heavy parts, lift dynamics —
this design says so and names the coupon, rather than assuming past it.

---

## 1. The forces, from the top

### 1.1 Peel, per layer — the governing force

In bottom-up resin printing, each new layer cures against a transparent film and must
be pulled off it ("peeled") before the next layer. The peel force acts on the **area of
the layer being peeled**, and it acts **once per layer, on that layer's geometry at
that moment** — not on the finished part. Whatever holds the freshly cured material
(the model's own column below it, or support arms) must react that force every layer.

This is measured, not assumed. COUPON-PLATE-5 printed a 24-cell grid (8 tip sizes x 3
cap areas) built so that three competing force laws — area-pressure, perimeter
tension, squeeze-film — predict different cells failing, with a discriminant spanning
a factor of ~24,000. Area-pressure won. The failure boundary is a pure ratio:

    X = (peeled area carried) / (narrowest cross-section of the carrying arm)

- `X_STAR_CONE` in [25.00, 25.34] — plate 5, cone arms. The interval's lower edge
  (25.00) is a recorded read; the upper edge is a monotonicity inference.
- `X_STAR_CYLINDER` in (19.91, 30.48) — plate 6, cylinder arms, n = 1 at each edge.
  Contains the cone interval, which retired the "cone artifact" worry.
- `X_ALLOWED` = floor(25.00 / 2) = **12** — the design ratio. It derives from the
  recorded lower edge only, so it rests on observation even though the interval's
  width does not.

Two facts about peel that the design must carry and most schemes drop:

**(a) Peel pressure is strongly geometry-dependent.** This project's own two anchors on
this printer differ by 54–81x: a 0.05 mm slender wall saw an effective pressure of at
most 1.24 kPa (plate 1, saturated bound), while a flat disc 5–12.5 mm across is best
reconciled at ~67–101 kPa (plate-6 physicist memo, corroborated at the same order by
Cao's measured 4.8 MPa strut fracture stress / X* = 25 → ~192 kPa). The ratio law
absorbs this only within the regime it was measured in: flat patches of 3.14–122.7 mm².
Below 3.14 mm² the law is extrapolation — in the conservative direction, because the
slender regime's pressure is ~80x lower, so applying flat-patch X to a sliver oversizes
it. This design accepts that conservatism and says so (§10).

**(b) The failure section is not the tip disc.** On every large plate-5 cap, the break
was at the base of the cylindrical portion where it meets the cone — the narrowest
cross-section of the load path, not the nominal tip. The law applies at the narrowest
section wherever it is. Sizing (§6) therefore requires a minimum-section locator, and
the same rule is what lets the model's own material be a load path (§3).

### 1.2 Weight — omitted, by enumeration, not by silence

Weight is static and scales with volume; peel scales with area. The plates cannot
separate them (every specimen was a flat cap, where area and volume are proportional),
but arithmetic closes most of the gap:

- **Hollowed shell** (constant wall t): mass ~ rho·S·t and worst-layer peel ~ p·A both
  scale as length², so the weight-to-peel ratio is **size-invariant**: 0.11% at the
  101 kPa anchor, 0.87% at 12.5 kPa, 8.8% at the most weight-favourable 1.24 kPa —
  independent of model size.
- **Solid part**: the ratio grows linearly with size but never crosses 1 inside this
  printer's build volume (crossover diameter 171 mm at the weight-friendliest anchor,
  vs a 118 mm axis). Worst credible case: ~35% of peel for a 60 mm solid at 1.24 kPa.

So: **no weight term.** The stated residual: a large model that fails to hollow for a
geometric reason (a live regression — the Weeping Treant used to hollow) evaluated at
the lowest pressure anchor erodes the 2.08x design margin to ~1.54x. That is a
correction, not a governing term, and coupon C2 (§11) is the measurement that would
retire it. Note the hollow gate already refuses only small models, which are exactly
the ones whose absolute weight is negligible — aligned with the physics by
construction.

### 1.3 Position on the plate — real, unmeasured, given a slot

The printer releases by tilting, so the separation front travels across the plate; the
mechanism requires position dependence, and the front-concentration factor is already
worth a factor of two in the shipped low-point allowance (the ruled K = 1.41 vs 2.0).
But the only quantitative statement anywhere is a ≤1.53x front-to-back NON-detection at
plate 6's own ladder resolution; the purpose-built controls (plate 1 family D, plate 5
rows R1/R5) were never scored; and even the leading-edge direction is a project record
registered `Measured` on a source that merely restates it — no observation establishes
which edge leads.

Design decision: the model carries a **position severity factor λ(x, y)** multiplying
carried area, **frozen at 1.0 everywhere**, registered as a Placeholder with the
provenance above. Measuring it (coupon C1, §11) is then a data change, not a design
change. No term in this design may depend on the tilt *direction* until the direction
is observed.

### 1.4 Dynamics — flagged, unbounded

Lift speed, lift distance, retract speed and acceleration are all "Not specified" in
the vendor datasheet, and peel force is roughly linear in lift speed over the measured
literature range. Nothing in this repository bounds the inertial term. This design
does not model it; §11 item E6 names the cheap first step (read the slicer profile).

### 1.5 What may never be computed

X is a phenomenological ratio, not a stress ratio: recombining `PEEL_PRESSURE_PA`,
`TENSILE_GREEN_MPA` and `TIP_SAFETY_FACTOR` into forces is forbidden (their naive
combination predicts X* in the thousands; measured is ~25). **This design computes no
force in newtons anywhere.** Every quantity is an area in mm², a length in mm, or a
dimensionless ratio. That is not a limitation; it is the shape of the evidence.

---

## 2. The quantity: worst-layer carried area

**Definition.** For a support s:

    A_carried(s) = max over layers z of A_alloc(s, z)        [mm²]

where `A_alloc(s, z)` is the portion of layer z's *unsupported peeled area* allocated
to s (§4). "Unsupported" means: belonging to a connected region of that layer's
cross-section that does not yet qualify as carried by the model's own material (§3).

**Why worst-layer, not down-facing surface area.** The coupon measured exactly this.
When a plate-5 cap prints, every layer of the cap spans the full cap area, and each
layer's peel force passes through the arm below — the arm carries the cap's *layer
cross-section*, which for a flat cap equals its down-facing area. On a general model
the two diverge: a region's cross-section grows layer by layer, and once it fuses into
the model's supported bulk the load path moves into the model. Down-facing surface
area is the special case; worst-layer cross-section is the measured general law.

**Beyond the measurement, stated:** the plates loaded one arm (or one strip group)
with one cap. Treating a per-layer maximum over a growing region as "the cap area" is
an extension from static coupon geometry to a layer-resolved history. It is the
minimal extension consistent with the per-layer nature of peel, and E3 (§11) measures
how much it differs from the naive down-face rule on the real corpus before we pay for
the full machinery.

---

## 3. The model is a load path too: the neck rule

The model's own cured material is the same green resin as a support arm, so the same
area law governs its narrowest section. This single idea replaces today's
"eligibility by provenance" (a face counts only if it is in an overhang island) with a
physical criterion, and it answers the question today's engine answers with silence:
*which surface area is peel-loaded onto supports, and which is carried by the model
itself?*

**The neck rule.** Track each connected region R of cured material from the layer it
first appears ("birth") upward. At each layer z, the region's *neck* is the horizontal
overlap area between R at z and already-carried material at z − 1. Define
`A_neck_min(R, z)` as the smallest such overlap seen so far on R's history. Then:

    R is model-carried at layer z  ⟺  area(R, z) ≤ X_ALLOWED · A_neck_min(R, z)

- A region born over nothing (a classic overhang island) has neck 0: it demands
  supports for its entire area from birth. This is why island births must be supported
  — recovered here as a theorem, not a rule.
- A region growing off a fat column hands off immediately: its neck is huge.
- A region hanging off a thin spike (neck 0.5 mm² carries at most 6 mm²) demands
  supports for the *excess* the moment it outgrows the neck — v1 is binary: once the
  inequality fails, the whole region's area re-enters demand (conservative; a
  subtractive credit for the neck would be an unmeasured sharing assumption between
  model and arms, deferred to coupon C5's territory).

**Assumption, stated:** load paths are treated as vertical, so horizontal sections are
the relevant necks. A strongly oblique load path could have a narrower diagonal
section; nothing here models that. The existing low-point reach allowance is the
degenerate single-point case of this rule and stays consistent with it.

Supports themselves obey the same rule: a support-on-support seat, a raft member, a
trunk — every cross-section of the support forest must satisfy
`routed area ≤ X_ALLOWED × section area`, where "routed area" sums everything whose
path passes through that section (§6).

---

## 4. Allocation: layer-resolved area Voronoi at fixed physical resolution

Every mm² of unsupported peeled area at every layer must land on exactly one support.
The rule:

1. **Raster, not faces.** Each layer band's cross-section is evaluated on a raster
   whose cell edge is a fixed physical length (reuse `cell_edge_limit_mm`, the same
   physical resolution the dyadic coverage machinery already uses). Mesh triangles
   never appear in the load model; they only bound the solid being rasterised. This is
   what makes the model tessellation-independent (§8), and it removes the documented,
   never-re-checked SPEC-V5-GEOM §2.5 precondition ("faces are small vs spacing")
   whose failure is today's zero-carry defect.
2. **Bands.** Evaluation may proceed in bands of a fixed physical height rather than
   every 0.05 mm layer, taking each band's maximum region cross-section
   (conservative). Band height is a registered constant that prunes cost; it must
   never change which supports exist.
3. **Nearest-support allocation.** Within an unsupported region at band z, area is
   allocated to the nearest engaged support contact (distance to seat point),
   restricted to supports whose contact exists at or below z. This is an area Voronoi
   partition of the region — the continuous limit of plate 6's additive sharing, and
   the same partition the deleted auditor's `voronoi_area_mm2` input presumed and the
   retired Python computed at face granularity. Per-arm enforcement
   (`cell area / arm section ≤ 12`) is strictly conservative relative to plate 6's
   measured group rule, and needs no sharing extrapolation beyond N = 6.
4. **Conservation, or a named error.** Per band:
   Σ allocated area + Σ model-carried area = total cross-section area, exact to the
   raster quantum. A region with demand and no support in reach is a **named
   failure** (the deleted auditor's one great policy: a missing partition entry is an
   error, never a plausible zero). Today's `OwnerlessLoadFace` becomes that error's
   disclosure; silence is abolished.
5. **A support allocated nothing** — max over all bands is zero — is either refused
   at emission, or retained with an explicit `coverage-only` disposition that names
   why it exists (e.g. a clearance or attestation requirement). Never silently
   minimum-sized. Today's measured state — 71 of 85 emitted contacts carrying zero on
   the overhang table, 60 of 60 appended contacts carrying exactly zero — becomes
   structurally impossible rather than merely discouraged.

---

## 5. Site selection: placement is the load model

The owner asked: *"Does the spot-selection where the supports land on the model not
effectively decide 'we need a support here in order to carry this area?'"* Yes —
and in this design that is literally the algorithm, run in layer order:

1. **Demand appears** where the neck rule fails: at region births (neck 0) and where a
   growing region outruns its neck. Demand is an area, in mm², at a band, on a
   footprint.
2. **A birth is seated first.** The earliest support(s) under a region carry its whole
   early cross-section — which is the mechanism behind the owner's observation that
   lower supports must be thicker (§9).
3. **Density comes from capacity, not a constant.** The largest legal load-sized tip
   (radius 1.0 mm; diameter 2.0 mm) carries at most
   `X_ALLOWED · π · 1.0² = 37.70 mm²`. So a support's Voronoi cell on the demand
   footprint must not exceed 37.70 mm², which on a triangular lattice means spacing
   ≤ 6.6 mm; a preferred tip of radius 0.6 mm (diameter 1.2 mm) gives cells of
   13.6 mm² and spacing ≈ 4.0 mm — today's `TARGET_SPACING_MM = 4.0` is thereby
   *derived* and demoted from a decision to a default preference (open task #58,
   resolved by derivation). Spacing is an output of (preferred tip size, X_ALLOWED),
   never an independent constant.
4. **Placement minimises scar cost, because it cannot minimise scar area.** The scar
   invariant: total scar area = carried area / X = 8.33% of supported area *no matter
   how the load is tiled* — thin-and-dense equals thick-and-sparse. So the objective
   is not "less scar" (impossible) but *where* scars land: rank candidate seats by
   marginal demanded-area coverage (the machinery coverage-repair already uses),
   tie-broken by delicacy weighting so scars migrate away from faces the owner marked
   delicate. Candidate seats remain constrained by the existing seat-validity rules
   (plate, model surface including cavity interiors, other supports).
5. **Refusal feeds back.** If sizing (§6) refuses a cell as over capacity, placement
   must add a contact and re-allocate — the loop closes here, instead of a clamp
   hiding the problem.

The never-built "hex lattice + Lloyd relaxation" plan of record is subsumed: the
lattice pitch falls out of step 3, and relaxation is replaced by demand-ranked seating
which serves the same goal (even cells) with a physical objective.

---

## 6. Sizing: the area law at the narrowest section, from X_ALLOWED directly

For each support, from allocated load to geometry:

1. **Tip section.** `π · r_tip² ≥ A_carried · λ / X_ALLOWED`, i.e.
   `r_tip = sqrt(A_carried / (12π))` at λ = 1. Sized **directly from the registered
   `X_ALLOWED`** — the four-placeholder legacy stress form (`sqrt(A·P·SF/(π·σ))`) is
   retired to a diagnostic. Its arithmetic is identical today (its implied ratio is
   exactly 12), so this changes provenance, not numbers — but it removes a law held up
   by four unmeasured placeholders whose silent movement nothing would catch, and it
   defuses open task #63 (the placeholder pair that implies failure at ~0.007%
   strain). This is the short-list 6 ruling, taken in the physics direction.
2. **Bounds are refusals, not clamps.**
   - Minimum: radius 0.05 mm (diameter 0.10 mm), the owner's field minimum.
   - Maximum load-sized tip: radius 1.0 mm (diameter 2.0 mm). A cell demanding more
     than 37.70 mm² is **refused back to placement** (§5.5). Today the engine clamps
     and ships the violation.
   - `TIP_RADIUS_MAX_GROWN_SCAR_MM` = 0.4 mm radius (0.8 mm diameter) remains what it
     is: a ceiling on growth-for-attachment only, never a load bound.
3. **The narrowest section governs, wherever it is.** Every cross-section of the
   support member from seat to root must satisfy
   `routed area ≤ X_ALLOWED × section area`, where routed area at a section is the
   sum of carried areas of everything above it in the forest (so trunks, raft
   members and support-on-support seats are sized by conservation). The profile just
   above the seat must be non-decreasing (fast flare) so the tip disc *is* the
   minimum section — plate 5 broke every large cap at a waist above the tip, and the
   engine currently has no fast-flare term. A minimum-section locator over the routed
   member is a required deliverable, and its audit (E2) runs before anything prints.
   Stiffness criteria (Murray, deflection) remain as separate maxima — they answer a
   different question (green-modulus-dependent, 7x material spread) and are outside
   this document's scope.
4. **Sizing is a pure function of the accepted plan.** Any plan edit — routing
   removal, abandonment, merge, child edit — invalidates allocation and re-runs
   §4–§6 on the surviving plan. An abandoned contact's cells re-allocate to
   neighbours, which may re-size them or trigger refusal; the load never vanishes.
   A write-time gate (the unported Python `load_report`, open task #59) re-derives X
   for every emitted contact from the written artifact and refuses to write on
   violation. This makes the measured inversion — a contact carrying 450 mm² emitted
   at radius 0.05 mm (diameter 0.10 mm) while contacts carrying 40 mm² sat at the
   maximum — impossible *regardless of which of the two suspected mechanisms causes
   it today* (stale pre-routing tips, or the positional-id join). Diagnosing that bug
   remains worthwhile; fixing it stops being load-bearing.
5. **Sharing.** Additive only, per plate 6; never super-additive (part of the strips'
   headroom was fin-root section, not sharing); never extrapolated past N = 6
   (`n_eff_cap`, the largest group ever built). The per-arm Voronoi rule of §4 means
   v1 leans on sharing only as the physical reason neighbouring supports relieve each
   other through cell shrinkage — comfortably inside the measurement.

---

## 7. Worked example: the 10 × 10 mm box bottom

Today (measured): 11 emitted supports; 3 carry 50 mm² each — above the 37.70 mm²
single-tip capacity, silently clamped to radius 1.0 mm (diameter 2.0 mm) — and 8 carry
exactly zero at radius 0.05 mm (diameter 0.10 mm). A 20x diameter spread on one flat
plane: the owner's "weird thick/thinness", reproduced on ten lines of geometry.

Under this design: the bottom face is one region, born over nothing at its birth band,
neck 0, demand = 100 mm². Placement seats contacts so no cell exceeds 37.70 mm²; three
contacts give cells of ~33 mm² each → r = sqrt(33.3/(12π)) = 0.94 mm (diameter
1.88 mm) — or a preferred-tip layout seats ~7 at ~1.2 mm diameter. Every emitted
support carries load; none is refused; conservation holds at 100.0 mm². One band above
the bottom, the box's own 100 mm² column is its neck and the region hands off —
nothing above the bottom face loads these supports further.

---

## 8. Tessellation independence

The load model never sees a triangle. Regions, necks, allocations and demands are all
computed on rasters at a fixed physical cell edge; the mesh's only role is to bound
the solid being rasterised. Consequences:

- Two meshes of the same solid at different triangle counts produce identical demand,
  allocation and sizing, up to raster quantisation — which is set by a physical
  constant, not by the mesh.
- Region identity is geometric connectivity in the raster, ordered by area with a
  positional tie-break — this also removes the second, quieter tessellation
  dependency found in measurement (islands ordered by *face count*, which becomes the
  load owner's identity).
- The measured regression fixture already exists: subdividing the overhang table
  16x today moves the median emitted tip from 0.100 to 0.631 mm diameter and the
  upper quartile by 17.3x. Under this design that experiment must move carried areas
  by ≤ 5% and tip radii by at most one raster quantum — a standing CI test (§10a I4).
- Residual legitimate dependence: none from tessellation. If two meshes genuinely
  differ in shape (decimation that moves surfaces), supports may legitimately differ
  — that is geometry, not tessellation.

The measured warning that motivates the raster: today's failure is not "finer mesh →
thinner supports" (the sized-by-one-triangle field turned out to be dead — written,
hashed, and read by no decision); it is that the *eligibility set and candidate
density* track tessellation, so refinement makes supports **thicker** by waking the
partition up. Both directions of drift die with faces out of the load model.

---

## 9. The owner's observations, produced by mechanism

**"Lower-positioned supports ought to be much thicker / more load-bearing than the
higher-up ones."** Three mechanisms, all conservation, no tuning:
(1) a region's earliest (lowest) supports own its whole early cross-section until
more supports engage higher up and the Voronoi cells shrink; (2) every section of the
support forest carries the summed routed area of everything above it, so trunks and
near-plate members are thick and top twigs thin, by §6.3; (3) taller support columns
accumulate more routed demand bands. A support seated high on a small local feature
gets exactly that feature's worst band — small.

**"Supports on thin arms should support less than 'core' supports for wide areas."**
A thin arm's layer cross-sections are slivers of a few mm²; its regions hand off
early or demand little; its supports' cells are small → tips near the minimum
(0.10 mm diameter), which the owner's field practice already validates for short
supports. A wide underside is one large region whose cells run at tens of mm² → tips
toward 1–2 mm diameter. The thick/thin gradient the owner expects is the allocation,
verbatim.

---

## 10. What this design may not claim (honest limits)

1. No forces in newtons, ever — the evidence is a ratio.
2. Cells below 3.14 mm² (the smallest coupon cap) are governed by an extrapolated
   law; the error direction is conservative (slender-regime pressure is ~80x lower),
   so small cells oversize, never undersize. Coupon C3 shrinks this.
3. λ(x, y) = 1.0 is an assumption slot, not a finding. Nothing may depend on tilt
   direction until it is observed.
4. No weight term: enumerated residual ≤ ~9% (hollowed, size-invariant) and ≤ ~35%
   (worst credible solid at the weight-friendliest anchor), eroding the 2.08x margin
   to ≥ 1.54x in the worst corner. Coupon C2 is the measurement.
5. No dynamics term; unbounded by any evidence in the repository.
6. X* is corroborated on two arm geometries, not replicated; the project's own
   promotion rule (three independent plates) is unmet, and X_ALLOWED = 12 is
   conditional on one unidentified black resin at ~30 °C. Record vendor and batch
   from now on (costs nothing); C4 replicates.
7. Free length is unmeasured (family C6 dropped); X* is conditional on 4.00 mm arms.
8. The neck rule assumes vertical load paths (horizontal sections). Oblique
   min-sections are unmodeled.
9. Sharing above N = 6 and any super-additive credit are forbidden, not unknown.
10. Band-maximum evaluation is conservative by construction; its cost/precision knob
    (band height) prunes work and must never decide whether a support exists.

## 10a. Invariants (each testable as stated)

- **I1 Conservation.** Per band: Σ allocated + Σ model-carried = total cross-section
  area, to the raster quantum. Residual ⇒ named error, never a zero.
- **I2 Law everywhere.** For every emitted support and band:
  routed area ≤ X_ALLOWED × narrowest-section area, re-derived from the written
  artifact by the write gate. (Today's measured state violates this at 20x and ships.)
- **I3 No silent zero.** Every emitted contact has A_carried > 0 or a named
  coverage-only disposition.
- **I4 Tessellation.** `subdivide_once` x2 on any fixture: per-support carried areas
  within 5%, tip radii within one raster quantum.
- **I5 Purity.** Disclosure tip radius == tip radius recomputed from the accepted
  plan, exactly, after any sequence of plan edits.
- **I6 Refusal.** No emitted tip's carried area exceeds 37.699 mm²; excess demand
  adds contacts or fails the run by name.
- **I7 Monotonicity.** Adding peeled area (or removing a support) never decreases any
  remaining support's carried area.
- **I8 Units.** Internal radii in mm; every human-facing disclosure prints radius and
  diameter.

---

## 11. Evidence programme, cheapest first

In-engine (no printing, before implementation is trusted):
- **E1** Corpus histogram of carried area per contact (e.g. Weeping Treant, 558
  contacts) under the new allocator vs today — checks real cells land inside the
  measured 3.14–122.7 mm² band and quantifies how many contacts today's model starves.
- **E2** Minimum-section audit: locator over every emitted member; distribution of
  (routed area / min section) on the corpus. Today nothing locates the section that
  actually broke on plate 5.
- **E3** Worst-layer vs down-face-area delta per support across the corpus. If the
  delta is small everywhere, v1 may ship the simpler down-face allocator with the
  layer machinery as its checker; if large, the layer model earns its cost. Decide on
  the measurement, not taste.
- **E6** Read the slicer profile's lift speed/distance/acceleration — bounds the
  dynamics term for free.

Coupons (physical prints; all features below z ≈ 9 mm per the standing A6 rule):
- **C1 Position.** First, one coupon that identifies the leading release edge
  (position-sensitive feature at both Y extremes). If the physical plates survive,
  *score plate 1 family D and plate 5 rows R1/R5* — they were built for exactly this
  and never read; that is faster than any new print. Output: λ(y) or its bound.
- **C2 Weight vs peel.** Matched cap areas on identical arms, varying the solid
  volume behind the cap (thin plate vs tall solid block). The first specimen family
  ever printed where area and volume diverge.
- **C3 Small caps.** Caps 0.2–3.14 mm² at matched X — tests the ratio law in the
  bottom 1.5 decades, where every ordinary support tip actually lives.
- **C4 Replication.** X = 22/25/28 triplets, 3 plates, resin vendor and batch
  recorded — meets the project's own promotion rule for X*.
- **C5 Sharing bracket.** Only if a future design leans on groups > 6 or neck
  subtraction; v1 does not.

---

## 12. Blast radius and migration sketch

What changes (design intent; the implementation plan prices it):
- `perception`: eligibility-by-provenance, `finalize_load_rows`, and the
  `SingleCarrier` hard-code are replaced by the band allocator (§4). The merge
  tournament's retained representation (`SpecializedLoadBaseline::assemble`, one
  winner per row, 3 witnesses) must be rebuilt — this is open tasks #68/#69/#70 and
  the single largest implementation cost; the block comment at the `SingleCarrier`
  site already says so.
- `FrozenTipLoadPolicy`: sized from `X_ALLOWED` directly; `PEEL_PRESSURE_PA`,
  `TENSILE_GREEN_MPA`, `TIP_SAFETY_FACTOR` retire to diagnostics (short-list 6,
  task #63). `X_ALLOWED` gains the resin-conditionality note.
- Selection (`plinth-support/src/lib.rs`): comparator gains demand ranking;
  `TARGET_SPACING_MM` becomes a derived preference (task #58); the inert
  `ContactCandidate.supported_area_mm2` field is deleted — it decides nothing, and it
  has two mutually inconsistent constructions waiting to trap the first reader.
- `member_sizing`: peel-tension term becomes the X law over routed (summed) area;
  fast-flare constraint added; minimum-section locator added.
- `runner`: write-time load gate (task #59); re-size after any plan edit; disclosures
  print radius and diameter.
- Registry: new `Placeholder` λ = 1.0; band height; derived 37.699 mm² per-tip
  capacity; sharing cap 6 recorded `Measured`.
- **Every frozen digest and corpus baseline refreezes.** Behaviour change is the
  point: today's zero-carry and clamped contacts all move. Where clamps do not bind
  and the partition already worked, arithmetic is unchanged by construction (the
  legacy form's implied ratio is exactly 12). The refreeze, and the choice to size
  from `X_ALLOWED` directly, are Craig's ruling to make — this document recommends
  both.

Phasing: (1) instrumentation + E1–E3 beside the live path, no behaviour change;
(2) allocator lands behind the existing plan seam with the write gate in report-only;
(3) placement demand loop; (4) gate enforcing; refreeze. Each phase leaves the
pipeline shippable.
