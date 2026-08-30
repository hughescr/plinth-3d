# Strictly empirical support-load design

> **Status:** proposed design and implementation plan. It changes no production behavior.
> Designed against `develop @ d1f53ea` on 2026-08-29.
>
> **Stance:** use only what the printed coupons measured. Refuse a model when the coupons do
> not justify the mapping. A refusal is better than a precise-looking number with no measured
> meaning.

## Decision in one paragraph

The load quantity is **coupon-equivalent cap area**, in square millimetres. It is not force,
weight, model surface area, or one mesh triangle. One admitted patch owns one support cluster.
For one arm, the patch area is assigned to that arm. For a measured multi-arm template, the
cluster owns the area collectively and identical arms receive equal accounting shares. The
required circular area at the narrowest section of each arm is at least its accounting share
divided by the registered design ratio, 12. Placement and sizing are one operation: the engine
fits a coupon-tested one-arm or multi-arm template to an admitted patch, then checks the final
routed geometry at its actual narrowest section. It does not construct contacts first and
invent their loads later. A patch with no legal template, a contact with no positive share, a
post-routing edit that invalidates a cluster, or geometry outside the coupon envelope causes a
named refusal. The present evidence is too narrow, and its resin identity is missing, so this
model is suitable for shadow measurement now but not for general production activation.

## Terms

- **Cap:** the flat, round coupon feature whose plan-view area was varied on plate 5 and
  plate 6. Its encoded boundary is the coupon's regular 48-sided polygon, not an arbitrary
  model shape.
- **Coupon-equivalent cap area:** an area that the engine may map to a coupon because the model
  feature meets the admitted geometry and process conditions below.
- **Minimum section:** the smallest cross-section anywhere on the load path immediately above
  the model contact. The observed coupon failures occurred there, not at an assumed contact
  disc.
- **Patch:** one connected model feature admitted as coupon-equivalent.
- **Cluster:** all support arms that jointly carry one patch.
- **Template:** a measured arrangement of one or more arms, including arm count, pitch,
  orientation, section shape, and free length.
- **Shadow mode:** the engine computes and reports a result, but that result cannot change a
  placement, radius, acceptance decision, or emitted file.

## 1. Evidence boundary

The numeric evidence comes from
[`plate-05-results.md`](../engineering-journal/reference/plate-05-results.md) and
[`plate-06-results.md`](../engineering-journal/reference/plate-06-results.md). The latter's
section 6 records the B6 admissibility override. These sources travel with every use of the
numbers below.

### 1.1 What the plates support

Plate 5 supports these statements:

1. The discriminating quantity is `X = A_cap / A_section`, with both areas in square
   millimetres. Area fits the observed survival pattern. Perimeter and `A^2 / d^3` do not.
2. The cap areas were 3.1416, 19.6350, and 122.7185 mm². The nominal tip diameters were
   0.100 through 2.500 mm.
3. The cone boundary is `X_STAR_CONE = [25.00, 25.34]`. Keep this interval separate from the
   cylinder interval.
4. The observed break was at the base of the cylindrical portion above the cone on every
   large cap. The load check therefore applies at the actual minimum section.
5. The lower boundary observation at 25.00 is recorded. Much of the second row is reconstructed
   from aggregate reads. The width of the interval is not wholly an individual-cell read.

Plate 6's admitted B6 family supports these statements:

1. The cylinder boundary is `X_STAR_CYLINDER in (19.91, 30.48)`, conditional on one 30.00 mm²
   cap and 4.00 mm free-length circular arms. Keep it separate from the cone interval.
2. Groups with `N in {2, 3, 4, 6}` co-carried one 30.00 mm² cap at 2.0 and 2.8 mm pitch, with
   arm lines along X and Y. The largest measured group is six.
3. Sharing is at least additive. It has no measured upper failure bracket. A section bonus at
   the strip roots accounts for some unknown part of the survival margin. The design may use
   additive sharing, but no super-additive credit.
4. The B6 result has an explicit post-result admissibility override after a plate-wide gate
   fired. A design must carry that caveat with the result.

The registered design ratio is `X_ALLOWED = 12`. It is conservative against the recorded
plate-5 lower boundary and the admitted plate-6 cylinder lower boundary. Sizing reads this
ratio directly. It does not reconstruct 12 from peel pressure, green tensile strength, or a
safety multiplier.

The only interpolation this design accepts is the measured area-ratio law between the smallest
and largest tested cap areas and between the smallest and largest tested nominal sections. It
does not extrapolate beyond either measured range. This interpolation is still a model; the
three tested cap areas, rather than a continuous sweep, support it.

### 1.2 What the plates do not support

The model makes none of these claims:

- a peel force in newtons;
- a common pressure for different patch shapes, sizes, or positions;
- a load proportional to arbitrary curved surface area;
- a spatial Voronoi or nearest-contact load partition;
- support behavior for a solid heavy part, a hollow shell, or a thin arm as classes;
- cumulative load as a function of support height;
- independence from arm length, layer height, resin, temperature, lift profile, or plate
  position;
- a sharing rule above six arms;
- a rule for arbitrary cap shape, aspect ratio, or a cap smaller than 3.1416 mm²;
- a load-sized maximum radius;
- a physical meaning for `ContactCandidate.supported_area_mm2`.

The plate-5 and plate-6 resin is recorded only as black resin. The vendor and batch are not
recorded. A future resin cannot be shown to match the measured material. This fact alone blocks
strict production activation until an identified resin and print profile reproduce the
boundary.

## 2. The load model

### 2.1 Quantity

For an admitted patch `p`, the load index is

```text
A_p = plan-view area of the coupon-equivalent cap, in mm²
```

`A_p` is a measured geometric proxy. It is deliberately not named force. On the coupons, cap
area was also the horizontal section exposed during peel. On a sloped, curved, thick, hollow,
or branched model feature, those quantities separate. The strict model does not choose among
them without another plate.

The engine must not substitute any of these quantities for `A_p`:

- triangle area;
- the sum of arbitrary down-facing surface area;
- volume;
- mass;
- area of a nearest-neighbor cell;
- number of coverage samples;
- area of an overhang island unless that island itself passes the cap admission rule.

### 2.2 Admission rule

The first implementation admits only a feature that can be represented as a coupon cap and a
coupon-tested support template without a physical tolerance invented for this task.

A patch is admissible only when all of these facts are true:

1. Its load face is one connected plane perpendicular to build Z.
2. Its plan-view boundary is geometrically similar to the coupon's regular 48-sided polygon.
   A different outline is outside the evidence even when it has the same area.
3. Its area is within the plate-5 cap range, 3.1416 through 122.7185 mm².
4. It is supported by either the one-arm coupon form or an exact B6 group template.
5. Every property that affects the chosen template is inside its evidence record. A B6 group
   therefore uses a 30.00 mm² cap, `N in {2, 3, 4, 6}`, 2.0 or 2.8 mm pitch, an X- or Y-aligned
   arm line, identical circular arms, and 4.00 mm free length.
6. Printer, layer height, resin identity, temperature range, and motion profile have a
   successful calibration record that covers the run.
7. Plate position is covered by that calibration record.

Conditions 6 and 7 are not satisfiable from the present plate records. Shadow mode may report
that fact. Production mode must refuse it. It must not replace missing provenance with a
constant multiplier.

The geometry classifier may use a numerical tolerance only to account for encoded coordinate
error. That tolerance must come from the file's numeric precision and robust geometric
predicates. It is not a same-everywhere millimetre tolerance and it cannot make a physically
curved feature planar.

This narrow admission rule is intentional. Generalizing from a disc to a rectangular plate,
from a horizontal cap to a curved arm, or from one cap to a connected body would be a new load
model, not an implementation detail.

### 2.3 Allocation

An admitted patch has exactly one cluster owner. No patch may disappear because no contact is
near it.

For a one-arm template:

```text
A_arm[0] = A_p
```

For an admitted B6 group template with `N` identical arms:

```text
A_arm[i] = A_p / N, for every i in 0..N
sum(A_arm) = A_p
```

The equal values are an accounting convention for a symmetric tested group. The physical claim
is only group additivity. The model does not claim that a particular point on the cap belongs
to a particular arm. There is no face-to-arm or nearest-contact partition inside a cluster.

This rule has four consequences:

1. A model face is not a load row. Mesh refinement cannot create more owners.
2. A contact is created as part of a cluster. It cannot appear first and ask for a load later.
3. Every model contact has a positive accounting share. A zero-share model contact is invalid,
   not a minimum-sized support.
4. If routing removes, moves, merges, or adds an arm, the old cluster is invalid. The planner
   must fit a new admitted template and size it again, or refuse. Repartitioning without
   resizing is forbidden.

A face that no cluster claims is handled before construction:

- If it is outside the admission rule, report `UnsupportedByEmpiricalEnvelope` with the failed
  conditions and refuse strict production preparation.
- If it is admissible but no template fits, report `NoMeasuredTemplateFits` and refuse.
- If a construction edit leaves it unclaimed, roll back that edit. If no valid plan remains,
  refuse.

The engine never represents these cases as zero carried area.

## 3. Site selection and sizing are one search

### 3.1 Site selection

Yes, the selected spots decide which area needs support, but only after the area-bearing patch
is defined independently of those spots. A candidate contact does not bring its source
triangle's area with it.

For each admitted patch, the planner performs one batch operation:

1. Reconstruct the continuous cap geometry and `A_p` without using triangle count.
2. Enumerate only templates admitted by the evidence record.
3. Apply each template as a rigid placement on the patch. For a circular one-arm cap this is
   the measured centered placement. For a B6 cap this is the recorded line template with its
   measured pitch and axis.
4. Reject a placement when any seat fails contact legality, clearance, or attestation.
5. Compute its arm accounting shares and required section before routing.
6. Route all arms as one transaction.
7. Re-measure the final arm profiles. Accept the transaction only if every arm still satisfies
   its required minimum section and the cluster remains a measured template.

The search does not use fixed spacing to decide load capacity. It also does not optimize a
Voronoi score. Within the strict envelope, template geometry decides positions and patch area
decides section. Outside it, the result is refusal.

If more than one tested template fits, the deterministic order is:

1. the template with the fewest model contacts;
2. the template with the least total contact-scar area;
3. the template whose scars have the lowest existing delicacy cost;
4. its stable template identifier and rigid transform.

This ordering chooses among already measured configurations. It cannot admit a new
configuration. Delicacy affects only which legal measured template wins; it never reduces the
required section.

### 3.2 Sizing

For arm `i` with accounting share `A_arm[i]`, define

```text
required_section_area_mm2 = A_arm[i] / 12
required_section_radius_mm = sqrt(required_section_area_mm2 / pi)
required_section_diameter_mm = 2 * required_section_radius_mm
```

The implementation and every disclosure state both radius and diameter. The registry and Rust
geometry use radius. The owner and ChituBox use diameter.

This equation sizes the **required minimum circular section**. The final geometry check finds
the smallest cross-section along the built load path above the contact and compares its area to
`required_section_area_mm2`. It does not assume that the input tip radius, terminal sphere,
contact disc, stub root, or member radius is the minimum.

A contact profile may flare above the required section, but no section on the arm's load path
may narrow below it. A general taper profile is outside the cylinder result. Until a printed
profile family validates it, strict mode admits only a coupon-matching cone or constant-section
cylinder, with the matching evidence label kept in the output.

There is no load-sized maximum radius. If the required section does not fit a legal scar or a
measured template, the planner may choose another measured template with more arms. It may not
clamp the radius and emit an under-strength arm. If no measured template works, it refuses.

`TIP_RADIUS_MAX_GROWN_SCAR_MM = 0.400 mm radius = 0.800 mm diameter` limits attachment growth.
It is not permission to shrink a load-sized section. Attachment growth and load capacity are
separate checks, and both must pass.

## 4. Expected behavior

### 4.1 Wide core and thin arm

Within the admitted cap envelope, a larger cap gives a larger required total section. With one
arm, radius varies with the square root of cap area. With a measured group, the same total area
is spread equally over its identical arms. Thus a wide admitted cap gets a thicker arm or a
measured group, while a smaller admitted cap gets less total arm section. This behavior follows
from the measured area ratio. It is not tuned by feature name.

A real thin arm is usually curved, branched, and not a flat circular cap. The strict model
cannot call it a small cap. It refuses until a thin-arm coupon establishes the mapping.

### 4.2 Lower and higher supports

The plates did not vary a support's vertical role in a branched model. They do not establish
that a lower support carries the area assigned to higher features. The strict model therefore
has no Z multiplier and no cumulative-load rule.

A lower support is thicker only when its own admitted patch has more coupon-equivalent area. If
a lower and higher support own equal admitted caps, they get equal required sections. This does
not satisfy the owner's full observation. Adding thickness because a contact is low would be a
tuning rule with no plate behind it.

The missing measurement is a vertical branching coupon. It must put calibrated weak sections
on lower and higher arms, vary upper branch area independently of the lower cap, and bracket
failure. That print decides whether load is local, cumulative, or shared through the model.

## 5. Tessellation independence

For the geometry admitted by this design, tessellation independence is a hard invariant.

The patch identifier and area come from the reconstructed planar region and its plan-view
boundary. They do not come from source face identifiers, face count, candidate count, or island
order. Rigid template placement is evaluated against that continuous region. The same solid
with reordered, split, or merged coplanar triangles therefore produces the same patch,
template, contact positions, shares, and required sections, subject only to numeric encoding
error.

Required tests use the same represented cap with:

- the original triangulation;
- every triangle subdivided one and two times;
- a legal decimation;
- reordered faces and vertices;
- different diagonals across the same planar polygon.

The admitted patch set, cluster set, contact coordinates, total area, per-arm shares, and final
radii must match within a tolerance derived from numeric representation. A triangle count in a
tie-break, owner identifier, digest input, or load row fails this test.

This guarantee does not cover two remeshes that represent materially different curved
surfaces. The strict model refuses curved load patches, so it does not hide that ambiguity.

## 6. Degenerate cases

| Case | Strict result | Reason |
|---|---|---|
| Thin arm | Refuse unless the load feature is itself an admitted cap | No thin-arm load coupon exists. |
| Large flat plate | Refuse unless the whole feature is one admitted circular cap | Tiling a plate into independent cap cells and choosing a spatial partition are unmeasured. |
| Hollow shell | Refuse as a shell load model | The coupons do not establish a shell mapping. No weight credit is taken. |
| Solid heavy part | Refuse unless it duplicates an admitted cap geometry and process record | Cap area and cap volume were confounded on the plates. |
| Model contact with zero share | Invalid plan; remove it or refuse | Every model contact belongs to one admitted cluster and has a positive share. |
| Plate, brace, or support-on-support member | Outside this contact-load model | It is structure, not a model-contact owner. Its own sizing contract still applies. |
| Area below 3.1416 mm² | Refuse | It is below the smallest measured cap. The existing radius minimum is not evidence. |
| More than six sharing arms | Refuse | Six is the largest measured group. |
| Route changes arm count or location | Replan the cluster or refuse | The measured template and old sizing no longer apply. |
| Required section exceeds available scar | Try another admitted template, else refuse | A radius clamp would knowingly violate the area law. |

## 7. Testable invariants

Each invariant is checked in memory after planning and again from the final emitted support
geometry where possible.

1. **Patch ownership:** every admitted patch has exactly one cluster.
2. **No silent omission:** every empirical-eligibility row is either owned or appears in a
   named refusal. The successful output has no unclaimed row.
3. **Area conservation:** for every cluster, `sum(A_arm) = A_p` within checked floating-point
   error. Across the plan, cluster totals equal admitted patch totals.
4. **Positive share:** every emitted model contact has `A_arm > 0`.
5. **Measured group:** arm count, pitch, orientation, section shape, free length, cap area, and
   process evidence match one recorded template.
6. **Direct law:** every arm has `12 * minimum_section_area_mm2 >= A_arm`.
7. **Final-geometry law:** the minimum section is measured from the final live geometry, not
   copied from planner intent.
8. **No clamp concealment:** a section that cannot reach its requirement causes refusal.
9. **Transactional edits:** any arm edit invalidates and rechecks the whole cluster before
   acceptance.
10. **Stable identity:** joins use stable geometric patch, cluster, and contact identifiers.
    Positional vector indexes are not identifiers.
11. **Remesh metamorphism:** the tessellation variants in section 5 produce equivalent
    clusters and sizing.
12. **Read-back agreement:** the pre-write report and the emitted artifact report the same
    contact identifiers, shares, minimum sections, and pass/fail result.
13. **Units:** every public radius value is labeled radius; owner-facing output also gives
    diameter.
14. **Evidence completeness:** no strict success is possible when resin, profile, temperature,
    layer height, or position provenance is missing.

Property tests generate batches of cap areas and measured templates. They must never call the
planner once per face or once per candidate in a hot path. Batch geometry and batch section
measurement are part of the contract.

## 8. Measurements that decide whether this model can grow

### 8.1 Required before production activation

Print at least three independently prepared calibration plates using the named production
resin and batch, recorded temperature, 0.050 mm layer height, and the exact Saturn 4 Ultra
profile. Each plate repeats the cone and cylinder boundary cells around X = 12, 20, 25, and 30.
The result must preserve separate cone and cylinder brackets. It must also repeat the B6 sharing
cells with failure rungs, because the current sharing result has no upper bracket.

This is the measurement that can establish whether `X_ALLOWED = 12` applies to the current
material and process. Without it, strict mode remains shadow-only.

### 8.2 Required for general model support

These are separate experiments. One cannot substitute for another.

1. **Spatial-allocation plate:** a common flat cap with deliberately unequal arm positions and
   calibrated minimum sections. Cross cap shape, aspect ratio, arm pitch, and edge distance.
   Failure brackets distinguish equal sharing, nearest-cell sharing, and lever-like sharing.
2. **Vertical-branch plate:** lower and higher calibrated arms with independently varied upper
   and lower cap areas. It tests whether lower arms carry local or cumulative area.
3. **Thin-arm plate:** model-like narrow limbs crossed by limb section, carried area, arm
   position, and support section. It tests the owner's core-versus-arm observation directly.
4. **Shape and slope plate:** equal plan-view area at several cap outlines and surface slopes.
   It decides whether plan-view area, true surface area, perimeter, or another quantity
   transfers beyond a flat disc.
5. **Solid-versus-shell plate:** equal projected area with independently varied mass and shell
   thickness. It separates peel area from weight.
6. **Position plate:** repeat boundary cells across plate Y and record the actual leading edge.
7. **Length plate:** repeat the boundary at multiple arm free lengths. The 6 mm A6 family is
   void and cannot answer this.
8. **Layer/profile/resin plates:** vary one process input at a time. The model cannot claim
   invariance before these exist.

A real-model corpus histogram is useful for estimating refusal rate and implementation value,
but it is not physical validation. A remesh comparison proves numerical independence, not the
load law.

## 9. Implementation plan

No phase changes production behavior until the activation gate in phase 8.

### Phase 1 — evidence registry and template extraction

1. Add typed evidence records for cone and cylinder intervals. Keep them separate.
2. Represent every admitted coupon template from the plate generator source, not from copied
   prose: cap geometry, arm count, arm section, pitch, orientation, free length, plate
   position, layer height, and process provenance.
3. Mark missing resin identity and missing process fields as unavailable, not default values.
4. Give each record an evidence citation and an admissibility status, including the B6
   override.
5. Add a constructor invariant that sizing reads `X_ALLOWED` directly.

Output: a read-only template catalogue and tests against the plate tables.

### Phase 2 — tessellation-independent patch census in shadow mode

1. Build planar-region reconstruction over the prepared model geometry.
2. Compute plan-view boundaries and areas in batches.
3. Classify each region against the strict admission rule.
4. Report admitted area and one named reason for every rejected region.
5. Run the remesh tests before any site-selection integration.

Output: instrumentation only. No existing candidate, digest, or radius changes.

### Phase 3 — cluster planner

1. Introduce stable `PatchId`, `ClusterId`, `TemplateId`, and `ContactId` types.
2. Enumerate measured templates per patch as a batch.
3. Perform seat legality, clearance, and attestation for each template transactionally.
4. Produce one cluster with positive equal shares, or a named refusal.
5. Keep the old face-row partition out of this path. Do not adapt
   `CarriableMaximum::AreaLaw`; it still represents a face split that this design does not
   claim.

Output: a shadow `EmpiricalLoadPlan` beside the existing plan.

### Phase 4 — minimum-section measurement and direct sizing

1. Compute required section from `A_arm / X_ALLOWED`.
2. Carry that requirement through initial contact construction, routing, member creation,
   attachment growth, and sealing.
3. Locate the minimum cross-section on the final load path in one batched geometry pass.
4. Reject a profile family that is not coupon-admitted.
5. Compare final geometry with the requirement. Never substitute the requested tip radius.

Output: a shadow sizing and violation report.

### Phase 5 — edit and routing transactions

1. Make every contact addition, removal, move, merge, and abandonment invalidate its cluster.
2. Refit the complete measured template before committing an edit.
3. Recompute shares and section requirements after every accepted edit.
4. Prove with sabotage tests that removing one arm cannot leave stale smaller radii or dropped
   area.

Output: transaction API and adversarial tests, still without activation.

### Phase 6 — pre-write gate and artifact read-back

1. Add all invariants in section 7 as a pre-write gate.
2. Read the actual emitted minimum section and identifiers from the artifact.
3. Compare independently sourced allocated area and final geometry.
4. Make a missing partition entry a named failure, never a plausible zero.

Output: a load report that can witness the exact emitted artifact.

### Phase 7 — shadow evaluation

Run procedural fixtures and the bounded corpus preparation path. Report:

- admitted and rejected patch counts by reason;
- area inside and outside the empirical envelope;
- template fit rate;
- current contacts with zero carried area;
- current versus empirical contact count and radius distribution;
- remesh deltas;
- every stale-size or read-back disagreement.

Do not run a full corpus job without the existing resource and process safeguards. Shadow
results may prioritize measurements. They do not widen the evidence envelope.

### Phase 8 — activation gate

Strict production activation requires all of these:

1. the identified-resin replication in section 8.1 passes;
2. every section-7 invariant passes from the emitted artifact;
3. all admitted fixtures pass remesh metamorphic tests;
4. the strict refusal rate and resulting usable corpus scope are presented to Craig;
5. Craig accepts the deliberate refusal boundary;
6. docs, constants provenance, CLI error contracts, and rollback instructions are complete.

Activation should be one removable policy selection. Rollback selects the previous planner and
restores its frozen baselines. It must not reinterpret empirical records or silently mix the two
allocations.

## 10. Code and baseline impact

The likely implementation surface is:

- `crates/plinth-core`: evidence types, separate measured intervals, process identity, and
  template records;
- `crates/plinth-support/src/lib.rs`: patch-to-template site selection and removal of inert
  triangle-area meaning from the new path;
- `crates/plinth-support/src/perception.rs`: a separate cluster allocation; the face-granular
  winner representation is not reusable as the empirical contract;
- `crates/plinth-support/src/specialized.rs`: stable contact joins and cluster-wide edit
  transactions;
- `crates/plinth-support/src/initial.rs`: carry required section into built geometry;
- `crates/plinth-support/src/member_sizing.rs`: final minimum-section check, with legacy stress
  placeholders excluded from empirical acceptance;
- `crates/plinth-support/src/runner.rs`: final recheck, refusal disclosure, and artifact
  read-back;
- pipeline tests: emitted-artifact agreement, refusal propagation, and sabotage tests;
- documentation and constants ledgers: evidence boundary, process conditions, and current
  activation status.

Shadow phases should not move frozen support digests because they cannot affect decisions.
Activation intentionally changes contact sets, radii, refusals, disclosures, printer artifacts,
and corpus baselines. Those values must be re-frozen only after physical evidence and emitted
artifact checks pass. This is not a local radius-formula change.

## 11. Why this design may be rejected

This design is internally testable, but it is not generally useful with today's evidence. It
rejects the shapes that dominate real miniatures, cannot establish lower-support thickening,
and cannot authenticate the resin used for the original boundary. Those are not implementation
problems. They are the cost of the strictly empirical stance.

The alternative is to add a transfer model: for example, surface Voronoi cells, layer-section
allocation, structural load paths, or a position multiplier. Each may be a good engineering
hypothesis. None is a plate result. If production needs one now, name it as an assumption and
review it under a different design stance rather than presenting it as empirical.
