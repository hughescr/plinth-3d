# Hollowing, drains and support placement

**Review-only design, 2026-11-11 — no implementation or print approval.** Baseline: develop `cc7d7f3`; lane `lane/hollowing`. The first deliverable hollows **both** `test-data/chitubox-native/Sphere.stl` and `test-data/chitubox-native/Cylinder.stl` in plinth and supports them for inspection. The owner authorised this development now. It passes no PRD stage. Giant Bat remains stage 5; the treant remains stage 6, with its entry gate intact.

Authority: [PRD](../PRD.md) §§4.3, 6–8 and [support construction](step11-construction.md), especially §§4–6 and 9.1. **[O]** means owner ruling/practice; **[P]** means a technical proposal, not measured; **[D]** means a geometric derivation, not a print result. Owner rulings of 2026-11-11 settle the 2.5 mm wall, 3 mm holes everywhere, placement priority and wholly interior H2 exemption. Remaining choices below do not waive hard limits.

## 1. Decisions and existing gaps

- **Solve holes and supports together first.** Build real hollow, drilled sphere and cylinder fixtures in-engine, then put adequate non-overlapping outside supports around their plate-side rims and permanent inside supports on their earlier inner walls. An external fixture would postpone the requested hollowing path; a general sparse-SDF engine would postpone the known support defect. Neither is the first deliverable.
- **Keep the exterior; construct the inside.** For these convex fixtures use a bounded convex offset and robust Boolean cuts (§2). Later use a sparse signed-distance field (SDF) for general models. Write outer surface, inner surface and bore walls into one model mesh; keep native supports separate.
- **Plan holes before contacts.** Reserve bores and flow paths during contact admission, routing, bracing and raft construction. Never place supports first and delete collisions: that strands early layers and overloads survivors.
- **Inside supports are not extra plate roots.** Their loads pass through the earlier shell and its outside supports. Apply local inside sizing as well as whole-part outside sizing (§5).
- **Separate drainage from cups.** H5 concerns the finished part; FR-17a concerns each exposure. Two visible holes prove neither. The first checker extension measures simple finished drainage; checker FR-17a and general certification follow later, with their missing results explicit.
- **Keep P5 = 20 and P4 = 40 [O].** No hollowing discount on these ratios and no invented suction multiplier.

| Current code | Gap this work must close |
|---|---|
| `checker/src/h5.rs` | Proves a simple outward genus-zero shell as H5 not applicable, rejects inward closed shells, leaves other topology unverified. It measures no drains. A one-opening hollow body can also be genus zero: topology alone must not exempt hollow geometry. |
| `checker/src/h3.rs:51–52,525` | Feet touching the model are unverified: model material is not sliced to establish their roots. Extend print-order rooting through the actual earlier shell; the upper attachment cannot root a floating strut. |
| `crates/support/src/route/mod.rs:43` | Routing is `route_columns_at_plate` only. The generator has no model-rooted support path. Add it; emitter capability is not generator capability. |
| `crates/emit/src/shapes/mod.rs:109–119`; format §10.4 | Native model-to-model foot ball/cone records already exist. Reuse them; no new invented record is needed. Native sample `03` establishes these records, not hollow-shell behaviour. |
| `checker/src/h7.rs` | Compares triangles against posed input without permitted hollowing changes. A narrow permitted-change comparison is needed later; first review outputs retain the raw failure/unverified status, never bypass H7. |
| `checker/src/h12.rs`, `h6.rs` | Need independently established interior classification before applying H1/H2/H6 exemptions. A generator's inside tag is not proof. |
| `crates/support/src/load/{stack,structure}.rs`, `crates/geom/src/label.rs::find_cups` | Current cup checks use model rasters and lower-layer paths, not measured throat widths or support/raft blockage. Unknown stays unverified. |
| `crates/support/src/load/data.rs` | `PartChain` describes contact-to-plate chains. Inside supports need model-root dependencies, local sizing and a non-duplicating outside load ledger. |

## 2. Hollow construction and file representation

### Wall and eligible regions

**2.5 mm wall [O], on both fixtures.** P19 is owner experience from hand prints, not a minimum derived from the 0.10 mm P10 test wall. Treat 2.5 mm as the requested minimum for this construction: numerical error may make a local wall thicker, never thinner. Measure and report the realised minimum and excess thickness separately. This construction choice does not promote P19 to a new PRD hard limit.

For inward distance `d` from the original boundary, the void is where `d ≥ 2.5 mm`. Thin fingers, blades, wing edges, branches and narrow necks remain solid. Keep every eligible cavity component, not just the largest; each disconnected chamber needs drainage. Existing cavities must also be found and drained (FR-4). Never silently fill one or remove detail to simplify drainage.

Both fixtures are explicit review exceptions to automatic hollow selection. An ideal 20 mm sphere with inner radius 7.5 mm saves **1.77 mL, 42.2% [D]** before holes/supports: below FR-14's proposed thresholds. Measure both fixtures from the written files; report shell saving and net assembly saving after supports. This does not make small minis hollow by default.

### First implementation: exact convex construction, not a general hollowing engine

1. Validate that each supplied fixture is a closed, consistently oriented convex polyhedron; group coplanar faces and retain the input triangles outside cuts. Validate convexity with bounded predicates, not its file name. If either fails this precondition, stop this path with a located finding rather than taking its convex hull and losing detail.
2. Represent the convex solid as intersecting half-spaces. Move each supporting plane inward by 2.5 mm and intersect them to construct the void. For a convex solid this is its erosion by a 2.5 mm ball: every admitted void point is at least that far from every boundary plane. It is not the unsafe operation of moving each vertex along an averaged normal. An empty intersection means no cavity; an unexpected result on either fixture is a defect to investigate.
3. Use a small robust polyhedral Boolean implementation/library with exact or bounded predicates to subtract this void and the two bore cutters from the original mesh. The numeric implementation must bound plane-normal/distance error too; an exact predicate over inaccurate offsets is insufficient. Choose and test the Rust dependency during implementation, not by assuming bindings to manifold3d alone prove wall thickness or drainage. This task introduces no dependency yet.
4. Bore cutters are circumscribed polygonal prisms sized to give **at least 3 mm clear inscribed diameter in the written f32 mesh**. Report the actual diameter and round-off allowance. Join inner/outer surfaces with bore walls. Preserve uncut exterior triangles and all separate components; no outer voxel remesh or cosmetic smoothing.
5. Certify the one-sided minimum wall outside declared bore cuts against the original surface before admitting the cavity, and again check the emitted geometry. Keep every ambiguity explicit. Test the actual faceted rim at layer phases (§4); do not assume its cut edge is level. Freeze rounded shell/bores before support admission.

This bounded convex path is suited to the supplied closed sphere and cylinder, and directly exercises plinth hollowing. It is intentionally not the Giant Bat algorithm. Keep its output interface (shell, holes, reservations) reusable; do not grow a fixture-specific collection of concave special cases.

### Later: general sparse-SDF engine

Validate/repair only as FR-7 permits, merge touching solids, and use a shared triangle BVH with batched distance queries over sparse blocks. Start near the inner surface at **0.10 mm cells [P]**, refine ambiguous cells and narrow passages, and store distant empty/full regions compactly. Proposed distance error target **≤ 0.05 mm [P]** is a refinement target, not permission for a 2.45 mm wall. Admit surfaces only when a one-sided distance bound establishes the minimum wall. No dense printer-sized volume or per-ray/per-triangle matrix.

Inner normals point into the void. Inner-surface simplification is **one-way: move only into the void**, reducing void volume and thickening the wall, never toward the exterior. Check minimum wall one-sided; do not require an artificial symmetric maximum-thickness band. Report local thickening, lost saving and uncertainty; keep thin passages and chamber topology, or replan drainage before contacts. Preserve the outer surface outside cuts. Later pose search caches object-space cavity/distance data and batches queries per pose; it does not assume lattice extraction is rotation invariant.

### `.chitubox` contract

[Format documentation](../format/chitubox-v1.4.0.md) §§4–6 gives one raw triangle-soup mesh per model (9 f32 values/triangle), translation separate and rotation baked into vertices. No documented hollow/drain metadata is needed: outer, inner and bore surfaces form one closed **material boundary**, not uncapped edges or a second solid model that fills the void. Keep one model identity and separate native editable support records in its local frame. Reuse model-to-model records from §10.4.

Native hollow/drain samples are **deferred**, not a prerequisite for the first support-layout review and not silently assumed to exist. Before hollowing is called supported for printing, obtain native solid → hollow → drilled saves and an inner-wall support, compare records/meshes, and open/save/slice our exact hashed file. Check decoded masks, winding, holes and editable roots after resave. Generated fixtures test our code; they are not native format ground truth. The later sample request is explicit in §8.

## 3. Drains, vents and the two meanings of “low”

**Coordinates:** plate `z = 0`; print order is `+z`. When the raised plate hangs over the vat, gravity is approximately **+z** in those coordinates. Nearest the plate prints first; farthest from the plate is downhill after printing.

**3 mm holes everywhere [O].** The owner explains that sticky wet resin occupies about 0.5 mm on each side of a 3 mm hole, leaving about 2 mm of gap. Although air could use a smaller opening, he chose 3 mm for both drains and vents. The geometric requirement is a 3 mm clear bore in the emitted file; the approximately 2 mm wet gap is his physical explanation, not a smaller mesh target.

For the sphere use opposite pole openings. For the cylinder use centred openings through both end caps, with its axis in the print direction **[P]**. Each has a plate-facing opening that exists before the cavity starts, and an opposite opening for downhill drainage after printing. A later hole cannot relieve an earlier cup. Neither fixture has a known hidden surface or nook; both use their smooth pole/end-cap areas, with drainage geometry deciding the location. Hollow both as requested; measure the cylinder's actual dimensions rather than assuming sphere dimensions.

### Placement priority without region recognition

For each cavity and candidate pose, first require holes to reach its top and bottom, with every basin draining and air entering as resin leaves. Among feasible plans, use the owner's order **[O]**:

1. **Hidden surfaces:** bottom of a base, soles of feet or another truly hidden face.
2. **Nooks:** deep inside an open mouth, under an armpit or another recessed place.
3. **Flattest, least-detailed surfaces:** easiest to fill and sculpt after printing.

PRD §3.2 excludes semantic region recognition. Use geometric evidence only: a base underside can be inferred from an established base pose/planar bottom, but label that inference; smoothness alone cannot establish hiddenness. Concavity, recess depth and occlusion by nearby geometry rank nook candidates without naming anatomy. Local normal variation/curvature ranks flat, low-detail patches. An owner-declared face can establish hiddenness in a review fixture; a general face-hint interface would be a later scope decision, since v1 currently has only the keep-up hint. Do not invent an automatic foot/mouth detector. If hiddenness is unknown, report it and continue to geometric nooks/flat patches rather than claiming the hidden-hole threshold.

Two openings are a starting arrangement, not a maximum. Give each gravity basin a downhill outlet and each air trap a vent. Chambers may share openings only through usable passages. Start with **3 mm clear internal connecting passages [P]**; smaller paths do not silently merge chambers. Add openings or leave a newly proposed region solid, with the saving reported. This is a geometric starting criterion, not a flow-rate law. Reconsider pose and holes together if drainage and supports cannot coexist.

### Independent H5: first bounded check and later certification

The checker reads decoded model, supports and raft, using its own implementation. Hole descriptions may locate work, never define the entire cavity denominator. For the two convex fixtures first use a **fixed 0.10 mm void grid [P]**, triangle/cell boundary tests, independently detected inner regions/mouths, and directed height connectivity in the hanging gravity direction. Bound boundary cells as uncertain, not silently empty. Track all interior basins before clearance erosion; test 3 mm bulk routes separately so erosion cannot erase a small trapped pocket. A drain route must not go uphill; air must enter through a distinct vent to each air-trapping region. Include the full assembly, plate obstruction and external discharge route.

Report cavities found, drain/vent paths, measured least widths, residual-volume uncertainty and all boundary ambiguity. A route through uncertain cells, an unresolved pocket, missed interior classification or a resource limit is **unverified**, not passed or not applicable. This simple search can expose obvious defects; fixed resolution alone cannot certify arbitrary drainage. Bores get a separate solid-intersection check against **every** support part (balls, necks, columns, braces, feet and raft), not a centreline test.

Later replace unresolved grid cells with bounded adaptive free/occupied cells, explicit mouths and chamber boundary links. Independently check manifoldness, orientation and undeclared pockets. Certify downhill liquid and incoming air routes with clearance bounds, refine ambiguous regions, and report the trapped-volume bounds. A drained hollow body is H5 **passed**, not H5 not applicable. Geometry still does not prove that viscosity, cure bleed or washing leaves no resin; FR-17's owner observation after washing remains required.

### Print-order cup check (FR-17a)

The final checker must evaluate each printed prefix, including supports/raft, with plate and current film closed. Future openings and the unprinted top cannot provide an escape path. Measure usable passages through already printed layers; record first enclosed/narrow layer, area and location. Do not equate finished H5 with FR-17a or invent suction coefficients.

**Checker FR-17a is deferred beyond the first layout deliverable.** Preserve existing generator warnings, but label the missing independent full-prefix check unverified. A 3 mm early hole is a design intention, not a cup-free certificate. No printing or stage acceptance follows from this review checkpoint.

## 4. Drain keep-out is a placement constraint

The [2026-10-22 rim ruling](../../reference/owner-notes/drain-rim-ruling-2026-10-22.md) allows tiny contacts at the rim if their actual solids stay outside the bore. The [2026-10-20 note](../../reference/owner-notes/rulings-2026-10-20-islands-knees.md) says keep-out is about half a centimetre around holes, “certainly not a big long strip”; the later note says “projected down from the hole.”

**Working interpretation [P]:** protect the real bore and its outward discharge at **each mouth**; project toward the plate only for a **plate-facing mouth**. Do not project a far-pole mouth through the whole cavity and remove all inside support sites. Treat the half-centimetre note as a local-access warning, not a uniform 5 mm contact ban that contradicts permitted rim tips. This resolves the sphere/cylinder plan technically but is flagged for owner confirmation in §8; it does not silently rewrite FR-18 for all oblique holes.

Freeze these reservations with the shell:

- **Bore:** actual through-wall passage reaching definite void and exterior space.
- **Outward corridor:** from each outer mouth away from the model along its discharge direction, at least 5 mm locally **[P]**, then prove escape to exterior free space. Check nearby supports, feet and raft. A short checked segment alone is not proof of discharge.
- **Plate-facing projection:** sweep that mouth toward `z = 0`, stopping at the plate. For a vertical circular hole this is a finite cylinder, not a stripe across the model. For an oblique mouth protect both its outward corridor and downward projection.
- **Inside flow routes:** protect the passages used by drain/vent checks; a strut outside the bore may still form a dam.

Use candidate-specific offsets for fast routing proposals, then test complete rounded native meshes at admission. No largest-arm radius applied to every tip. Outside rim contacts must have real material patches, first-layer capacity and H1/H2/H6/H9 compliance; no sub-0.1 mm hair exception. Keep 0.012 mm patch-separation and 0.32 mm unrelated external-member construction reserves; designed joins are distinct from accidental overlaps.

Compute needs on the **drilled shell**, including first rim islands. Search around the hole in angle and height; do not project all rejected tips onto one rim point. Admit small separate contacts, route/brace before commitment, and replay P5 after each batch. Later flank contacts cannot rescue the first layer. If no plan works, replan private holes/pose and rebuild before admission. Distinguish search exhaustion from proved infeasibility. Never shrink holes or merge exterior tips to claim success.

### First pole ring with a 3 mm bore [D]

Use the ideal sphere only to seed candidates, then measure the faceted emitted file. Let outer radius `R = 10 mm`, bore radius `b = 1.5 mm`, and height `h` above the uncut lower pole. The first material is at

`h0 = R − sqrt(R² − b²) = 0.11314 mm`.

At `δ` above that edge, before the inner void begins, the horizontal section is an annulus:

`b ≤ r ≤ sqrt(b² + 2 sqrt(R² − b²) δ − δ²)`.

For a **0.05 mm slab starting at the edge**, its outer envelope radius is about **1.79894 mm**, radial width **0.29894 mm**, area **3.098 mm²**. The exact birth edge has zero width; real layer phase can give a much narrower first piece. This slab example is not a guaranteed first-layer capacity.

A conservative planar packing seed for a full projected contact ball of radius `q` has centre radius `b + q ≤ c ≤ 1.79894 − q`. At `c = 1.65 mm`:

| Seed head | Neck radius | Allowed centre range | Maximum balls around this circle with 0.012 mm separation |
|---|---|---|---|
| Ball radius about 0.0552 mm | 0.052 mm | 1.5552–1.7437 mm | 84 |
| Ball radius about 0.1103 mm | 0.104 mm | 1.6103–1.6886 mm | 44 |
| Ball radius 0.159 mm | 0.150 mm | Empty | 0 under this conservative seed |

The count is `floor(π / asin((2q + 0.012)/(2c)))`. It is only a circumference packing upper bound, not a proposed support count. Keeping the 0.32 mm unrelated-member reserve even at these tiny ball widths reduces the corresponding bounds to about **24** and **19**, before larger columns, routes and braces. Actual curved patches and oblique heads require solid tests, not this projected-disc approximation. A ball may sit below the birth edge and touch a small sloped patch, so the conservative seed is sufficient geometry for a search, not a proof that other placements are impossible.

At P5 = 20 that illustrative slab needs at least **0.1549 mm²** of certified total section. The ideal native 16-sided neck areas `3.0615 r²` are about 0.00828 and 0.03311 mm²: at least **19** minimum-size necks or **5** of the larger necks if their *whole* sections were active and their patches supplied that capacity. First-layer balls/patches may provide much less, so these are lower bounds on count, not a passed layout. P5, H9, patch separation, member clearance and time-aware capacity all have to hold together.

**Faceting and f32 matter:** the bore cut of the real STL can start as several disconnected first-layer pieces rather than one level ring. Do not count them as one island merely because the analytic sphere is symmetric. Test the written cut edge across layer phases, including just below/on/above each birth, after f32 rounding. Anchor every non-exempt piece by its birth layer; test its joining before sharing load. A level-edge construction would need a declared, measured cut that preserves the exterior outside the bore; do not flatten the whole pole as a hidden repair.

The **far-pole inside ceiling ring** is the inside twin of this problem: its last growing overhangs surround the opposite hole and need local contacts that keep the bore clear. For an ideal 7.5 mm inner radius the corresponding opening edge is `7.5 − sqrt(7.5² − 1.5²) = 0.15153 mm` from the inner pole. Apply the same actual-mesh, layer-phase and capacity tests there, not outside contacts borrowed from a future layer.

### Tests use real drilled material

The former solid-sphere test with a centred forbidden cylinder is unsatisfiable by construction: its first pole is inside the forbidden volume. Replace it with the **actual hollow, drilled sphere and cylinder** and a small drilled annular control. The removed pole is no longer a support need; the first real rim is.

Test centred and off-centre 3 mm bores, adjacent bores where geometry permits, a near-tangent head, and an obstacle hit by a ball/brace but not its column axis. Include a deliberately impossible reservation covering every genuine first-rim anchor. Assert actual solid clearance, no duplicate/overlapping exterior heads, first-layer rooting, coverage and P5 replay. Record obstacle rejections and accepted alternative routes so an unchanged layout cannot pretend to exercise replanning. An impossible case remains unresolved/unprintable, never a low-ratio pass.

## 5. Permanent inside supports

“Inside” is the interior of a **drained** hollow body, not a sealed pocket or exposed recess. Prove each exempt part wholly interior; treat boundary-crossing parts as exterior. When drainage/classification remains ambiguous, the exemption is unverified. Native supports remain editable, not baked into the model to hide from checks.

| Rule | Inside treatment |
|---|---|
| H1 and H6; tool/removal restrictions | Already exempt under FR-19. Larger/broader attachments may be used and still reported. Outside unchanged. |
| H2 | Owner ruling 2026-11-11 permits tips/patches to fuse or merge **wholly inside a drained cavity only**. Measure fused unions without counting overlap twice. Outside unchanged. |
| H9 | 0.1 mm minimum remains, with construction reserve. No thinner-tip exception. |
| H3, load model, drainage, cups | Still apply. No future roots, fictitious plate capacity, blocked flow or unsupported ceiling. |

Root each inside foot on the **already printed and rooted inner wall nearest the plate**, outside bore/flow reservations. Foot overlap and load-bearing section must exist when the member starts. The upper ceiling connection cannot retrospectively root it. Use the existing native foot records plus new model-root routing and checker H3 support.

**Local inside sizing [P]:** assign each ceiling patch to the inside contact(s) that cover it, with a stated non-duplicating partition. For each inside contact, size every section on its path so **its assigned ceiling area ÷ certified section area ≤ P5 (20)**, as if that patch hung only on its strut. Check the foot, neck, column and joins, including member self-peel once. Fusion does not create area: a shared section carries the sum of its assigned patches. Do not let the connected shell's global rigid-sharing calculation give each inside strut zero load and exempt it from sizing.

**Outside sizing:** charge the whole peeling layer, including inside support material, **once** to the outside chains. Do not add the ceiling load again because it passed through a strut. Inside sections do not enter the denominator as extra plate capacity. Dependency is ceiling → strut → earlier lower shell → outside supports → plate; local shell transfer is assessed and uncertainty located.

**Circular-load-path rule:** a strut from a part back to the same part is not automatically forbidden. The graph must distinguish layer/time regions rather than collapse the whole model to one node: an earlier, independently rooted lower wall holds the strut before it reaches the later ceiling. A same-layer component may be traversed only after finding an independent rooted route; a cycle with no rooted entry, or a foot relying on the future ceiling it is meant to hold, fails. Never count the upper/lower attachments as reciprocal new sources of support capacity.

Sphere inside columns can run **about 11–14 mm [D]**, not just a short tip. Existing free-length/bracing/slenderness rules still apply. A 4 mm free-length trigger does not disappear inside: plan brace partners clear of holes and flow, and warn on unsupported lengths/uncertain stiffness. Interior fusion removes removal constraints, not buckling or thin-shell transfer concerns. Name the far-pole ceiling ring and all model-rooted feet in the review pictures.

## 6. Peel area and suction

Keep the existing material-area law and P5 = 20. Validate opposite-winding slice rings on the emitted hollow meshes. Do not manually subtract independently computed slab envelopes: the envelope of a difference is not generally the difference of envelopes.

An ideal equatorial sphere slice is **π(10² − 7.5²) = 137.4 mm²**, versus a filled outline **314.2 mm² [D]**. Report **both ring-area and full-outline area ratios** at each critical section, with their worst layers, using the same certified capacity. Here the outline numerator is about 2.286 times the ring numerator before supports/holes. The outline result is a sensitivity, not a silent replacement law; divergence gets a located warning. Recompute early solid-cap/rim loads, moments, first-layer capacities and member self-peel from the written assembly. Volume saving is not a peel multiplier.

The [owner note on hollowing and area](../../reference/owner-notes/hollowing-aligns-weight-with-area.md) records **2.68× less cured area giving only 1.26× less force** in a perforated-layer comparison. That is evidence that flow resistance matters, not that force falls directly in proportion to the removed area or that all hollows behave like those holes. Geometric drainage does not measure flow fast enough during release. Report hydraulic uncertainty and existing cup warnings; no calibrated suction coefficient exists. Missing independent FR-17a stays unverified in the first review. Ring-area credit alone never justifies a print.

## 7. Delivery order, acceptance and sabotage

### First deliverable: holes clear, supports adequate on both fixtures

Deliver a **paired review-only sphere and cylinder**, lifted at least 5 mm, 2.5 mm wall and two 3 mm openings each, with outside rim supports and model-rooted inside ceiling supports. Use production review-mode constructor/emitter/checker, not an alternate unchecked writer. The order within this one deliverable is:

1. **In-engine hollow and drill both convex fixtures (§2).** This small closed-polyhedron method directly meets the request that plinth itself hollow them. External fixtures would test only supporting somebody else's hollowing; general SDF construction is not needed to expose the known rim problem.
2. **Hole-aware outside placement (§4).** Fit adequate, separate contacts around the actual plate-side rim; route all parts and connected raft clear of bores/outlets. Exercise real first-layer pieces after f32 and layer phase. Measure capacity rather than presenting a sparse layout as success.
3. **Inside supports on earlier inner walls (§5).** Add model-root routing, native feet, local ceiling-patch sizing and outside single-count load propagation. Exercise the far-pole ceiling ring and 11–14 mm internal runs with bracing; do not run posts through a drain to the plate.
4. **Focused checker extension.** Test bore intrusion for every support part; extend H3 to prove model-rooted feet from earlier material; add the fixed-resolution H5 drain/vent search with ambiguous cases unverified. Exempt inside H1/H2/H6 only where classification and drainage are established; otherwise retain unverified status. Measure on emitted f32 geometry, independently of generator declarations.
5. **Owner layout inspection.** Show the two written files with raw checker results, pictures, budgets and every unresolved finding. Preserve §9.1 review-only names/embedded labels and `print_candidate: false`. This checkpoint demonstrates the new construction and placement path, **not** all-hard-limits acceptance. Deferred H7/FR-17a or ambiguous H5 may remain failed/unverified; do not hide or relabel them to make a green report.

**Focused first checks:** shell/void/through-hole topology; measured one-sided wall and 3 mm bore widths; real rim births rooted; inside feet rooted before their columns; outside patches separate; local inside and whole outside P5 accounting; zero bore/outlet collisions including balls, braces, feet and raft. These placement/rooting/sizing checks must succeed on both fixtures for the layout checkpoint to be complete. Uncovered ceilings, unrooted pieces, unresolved P5 sizing or collisions mean further layout work, not a completed deliverable. Report unassessed shell stiffness and free lengths separately. Both files must exercise hollowing, rim placement and inside supports, with measured path-hit counts. Deferred H7/independent FR-17a and ambiguous fixed-grid H5 remain visible and prevent print-candidate acceptance even when this narrower layout checkpoint succeeds.

Use only the failure controls needed for this path now: a brace/ball intruding into a bore while its column axis clears; a missing first-rim anchor; a future-rooted inside foot; and a sealed opening/blocked passage with a neighbouring valid control. No general sabotage campaign before seeing the support layout.

### Later, before a print candidate or general-model claim

Defer the **general sparse-SDF engine, H7 permitted-change checker, certified adaptive cavity cells, independent checker FR-17a, native hollow samples and most sabotage** until after this paired layout review. They are deferred work, not waived acceptance requirements. Native samples and exact-file open/save/slice checks must settle the consumer contract before a hollow print candidate. H7 must compare against original input plus narrowly declared cuts, never only our own hollowed STL.

Then test multiple chambers, uphill/narrow connections, undeclared pockets, a late vent that leaves an early cup, an internal dam outside a bore, reversed inner winding, inside-tag spoofing, double-counted capacity, exterior changes hidden by a broad declaration, and voxel/pixel/layer phase shifts. Pair broken files with valid controls and assert the relevant checker actually ran. Resource exhaustion remains unverified. These checks prove firing, not sufficient physics.

The completed print-candidate checkpoint needs all applicable hard checks passed, no unverified hard checks, resolved load/cup findings and H8 on the exact file. A print decision is separate. General Giant Bat integration follows the PRD stages; treant work still needs the §8.2 performance/repair gate. The convex fixture helper does not claim general hollowing support.

### Evidence and resource discipline

Report per hole actual diameter/location; minimum hollow wall; narrowest passage and residual-volume uncertainty; model/support/net resin; ring and full-outline ratios; local inside section ratios; outside single-count ledger; spacing, braces and uncovered regions; timing per step and whole-job peak memory. Counts state what they do **not** prove: zero bore collisions is not drainage; no detected cup is not measured suction; emitted supports are not print success.

Batch distance, collision, slicing and connectivity queries; bounded worker pool, compact layer bands and sparse blocks, reused buffers, cancellation between bounded units. Measure deterministic outputs at one and allocated threads. The planner's contact budget does not bound geometry/checker work; give those their own limits. Stay below 100 GB/job and pursue 4 GB/model. Never raise caps to retain per-ray/per-triangle allocations. Run changed tests, formatting/lint and one final focused integration check when implementing, not full-corpus gates per edit.

**Review pictures:** for each fixture, solid versus hollow side by side at the same pose/scale; outside plate-side rim spacing; sections through both holes and all inside feet; first cavity layer, widest ring and far-pole ceiling ring. Caption “nearest plate / prints first” and “downhill drain when hanging.” Ask whether the layout looks adequate and removable outside and whether inside members leave credible flow space. Review approval is not permission to print and passes no stage.

## 8. Remaining owner questions

The wall, hole size, both fixtures, interior fusion, placement priority and permission to proceed with review work are settled. Only these questions remain:

1. **For a hole facing away from the build plate, may we reserve open space outward from its mouth, without banning supports all the way down through the cavity to the plate? For a plate-facing hole we will keep its downward path to the plate clear. This follows your “not a big long strip” guidance while retaining the later downward-projection rule where it protects the outlet.**
2. **After the first sphere-and-cylinder layout review, will you save ChituBox examples of one sphere solid, then hollow, then drilled, plus one support standing on the inner wall? These are needed to verify native hollow geometry and support behaviour before we offer a hollow file for printing; they do not block this first review.**

**Risk and rollback:** main risks are false drain proofs, incorrect void winding, false interior exemptions, loss of exterior geometry and fictitious capacity from model-rooted supports. Keep originals/provenance immutable; validate the written file; mark ambiguity unverified; keep outputs review-only. No migration or external service is needed. No code changes are made by this design revision; implementation must preserve default transport and approved Round 25 behaviour. The PRD is updated in place for the binding owner rulings; its live mirror must be synchronised before implementation relies on them.
