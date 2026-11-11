# Hollowing, drains and support placement

**Proposal, 2026-11-11 — no implementation or print approval.** Baseline: develop `5313342`; review-mode lane `13e75f2`. The first output is the 20 mm `test-data/chitubox-native/Sphere.stl`, hollowed and supported for inspection in review mode. It completes no PRD stage. Giant Bat remains stage 5; the treant remains stage 6, with its entry gate intact.

Authority: [PRD](../PRD.md) §§4.3, 6–8 and [support construction](step11-construction.md), especially §§4–6 and 9.1. **[O]** means current owner ruling/practice; **[P]** means a proposal, not adopted or measured; **[D]** means a geometric derivation, not a print result. This document does not silently change H2 or the staging contract.

## 1. Decisions and existing gaps

- **Keep the exterior; construct the inside.** Use a sparse, adaptive signed-distance field (SDF) to make the inner surface, not an offset of each input triangle. Cut drains locally. Write exterior, inner surface and bore walls into the **same model mesh**, with native supports stored separately.
- **Plan holes before contacts.** Reserve the actual bores and flow paths in contact admission, column routing, bracing and raft construction. Never place supports first and delete those that hit a drain: that strands first layers and concentrates load on the survivors.
- **Retain permanent inside supports.** Root them on an earlier-printed inner wall, not through a drain to the plate. Their loads continue through the shell and its external supports. They are not extra independent roots.
- **Separate drainage from cups.** H5 concerns the finished part; FR-17a concerns every exposure. Two visible holes in the finished sphere prove neither.
- **Keep P5 = 20 and P4 = 40 [O].** There is no hollowing discount on these ratios and no invented suction multiplier.

The implementation is not already capable of these checks:

| Current code | Gap this work must close |
|---|---|
| `checker/src/h5.rs` | Proves a simple, closed, outward shell with Euler characteristic 2 as H5 not applicable; rejects inward closed shells; other topology is unverified. It measures no drains. A hollow body with only one opening can also have a single genus-zero surface: topology alone must not exempt it once hollowing is supported. |
| `checker/src/h7.rs` | Permits no geometry changes; compares triangles with the posed input. Hollowing needs a narrowly scoped permitted-change comparison, not an H7 bypass or comparison only to our own hollowed STL. |
| `checker/src/h12.rs`, `h6.rs` | Need independently proved interior classification before applying cavity exceptions. A generator's “inside” tag is not proof. |
| `crates/support/src/load/{stack,structure}.rs`, `crates/geom/src/label.rs::find_cups` | The current cup check uses model rasters and paths propagated from lower layers. It does not establish a usable throat width or account for support/raft blockage. `stack` only copies a known cup result; an unknown result must instead remain explicitly unverified. |
| `crates/support/src/load/data.rs` | `PartChain` describes a contact-to-plate chain. Internal supports need explicit model-root dependencies and load accounting, not another ordinary plate-rooted contact. |

## 2. Hollow construction and file representation

### Wall and eligible regions

**Propose 2.5 mm nominal wall [P], retaining P19's existing starting value [O].** Evidence class: **owner experience**, medium confidence—his ChituBox setting works in hand prints. It is not derived from the 0.10 mm test wall in P10, which says nothing about a miniature shell's stiffness. Ruling R1 confirms using it on this first sphere, rather than introducing a thinner value to improve savings.

For inward distance `d` from the original solid boundary, the candidate void is where `d ≥ wall`. Regions with less than twice the wall thickness cannot contain a cavity and remain solid: fingers, blades, wing edges, branches and narrow connecting necks. Keep every qualifying cavity component, not just the largest. A narrow connection can leave two cavities, each needing its own openings. Existing cavities must also be found and drained (FR-4), even if no new hollowing is worthwhile. Never silently fill an existing cavity or remove detail to make drainage easier.

The sphere is a **deliberate test exception to automatic selection**. An ideal 20 mm sphere with a 2.5 mm wall has a 7.5 mm inner radius: removing its interior saves about **1.77 mL, 42.2% [D]**, before holes and supports. That is below FR-14's proposed 10 mL threshold (also below its hidden-hole 5 mL threshold). These are analytic design figures, not measurements of the STL or emitted file. Review mode may explicitly request hollowing this fixture; this must not make small minis hollow by default. Report both shell resin saved and net assembly saving after added supports.

### Geometry method

1. Validate/repair only as FR-7 permits, merging touching solids before hollowing. Ambiguous material sign or unapproved repair is a located refusal, not a guessed interior.
2. Build a shared triangle BVH and **batched** signed-distance queries over sparse blocks. Start with **0.10 mm cells [P]** near the inner surface, refining ambiguous cells and narrow passages. Store distant full/empty regions compactly. No dense printer-sized volume and no ray-against-every-triangle matrix.
3. Extract a manifold inner surface with normals pointing **into the void**. Use distance/error intervals to admit triangles only when they leave the requested wall; refine before admission when the interval is inconclusive. Proposed local distance error target: **≤ 0.05 mm [P]**, tightened where needed. Do not hide a large global voxel margin inside “2.5 mm”: report requested wall, certified realized minimum, local excess thickness and its uncertainty. This is a construction target, not a new PRD hard limit. Original thin regions left solid are reported separately from hollow-wall measurements.
4. Copy original outer triangles unchanged except where a declared bore cuts them. Do not voxel-remesh the exterior. Use robust local cuts with bounded predicates and sewn bore walls; reject an unresolved cut rather than smooth or patch away a feature. All geometric reservations use the final rounded f32 triangles before support admission.
5. Check wall bounds, orientation, self-intersection and topology at construction. Drill plans are private proposals: a failed one is discarded/replanned before any contacts are committed. Once the shell and holes are frozen, they cannot move under admitted supports.

An inward triangle-normal offset is simpler on the sphere but folds or self-intersects at concavities and thin limbs. The SDF gives a consistent definition on those regions. Its discretisation still needs bounded checks; “voxel based” alone is not robustness. Preserve the exterior exactly to avoid spending FR-7's 0.007 mm exterior error budget on hollowing.

For later pose search, cache the object-space distance field and undrilled cavity. Transform/query that data in batches for each pose; drill and evaluate each pose's shell, early openings and supports together (FR-15). Do not assume lattice-dependent extraction is rotation invariant.

### `.chitubox` contract

[Format documentation](../format/chitubox-v1.4.0.md) §§4–6 describes one raw triangle-soup mesh per model: 9 f32 values/triangle, 36 bytes/triangle, mesh offset/length in the model table, translation separate, rotation baked into vertices. There is **no documented hollowing or drain metadata**. Proposed encoding needs no invented fields: store the outer and inner surfaces and the bore-wall triangles in that mesh, update lengths/offsets, retain one model identity and native editable support records in its local frame. At each hole, the inner and outer surfaces join through the bore wall; a drilled shell is a closed material boundary, not a mesh with uncapped boundary edges. An inner surface is not a second solid model to be unioned back into the void.

The format document does **not** establish hollow/drain behaviour from a v1.4.0 hollowed sample. Before calling this supported, obtain a native sphere saved solid, hollowed, then drilled, plus a native internal support rooted on the wall. Compare the native meshes/records and open, save and slice our output. Check decoded layer masks against the emitted material, including inner-shell winding and bores; a screenshot or library round trip is insufficient. A native resave must not fill the cavity, lose a hole, duplicate the inner model or detach the interior support. Native samples are ground truth; generated fixtures only test our code.

## 3. Drains, vents and the two meanings of “low”

**Coordinate convention:** plate `z = 0`; print order increases in `+z`, away from the plate. When the raised plate hangs over the vat, gravity points approximately **+z** in those model coordinates. Thus the point **nearest the plate prints earliest**, but the gravitationally lowest point **after printing is farthest from the plate**. This follows FR-17; calling both “the bottom” causes holes on the wrong side.

For the sphere propose **two separate 2.0 mm clear-diameter openings [P, based on P20 owner experience]**:

- A plate-facing opening at the `−z` pole, open before the cavity begins. It vents the forming cup and is nearest the plate, where early supports must fit around it.
- A `+z` pole opening, the downhill drain when the finished part hangs from the raised plate. The plate-facing opening then admits air. The later drain cannot be credited with relieving earlier cups.

Two is the starting arrangement, **not a maximum**. For general models, give every gravity basin a downhill outlet and every air trap an upward vent; connected chambers may share outlets only through verified passages. Add early openings wherever a basin would cup before its final holes print. Prefer short bores on hidden/known glue faces, then low-detail faces; no automatic claim that a smooth face is hidden. On the sphere no face is known hidden. On future models, a hidden hole is a preference, never a reason to leave a trapped pocket. Pose and holes must be reconsidered together if they cannot coexist with supports.

Require a full-width penetration from exterior to cavity, not just a centreline hit. **2.0 mm is the proposed minimum clear inscribed diameter**, measured after f32 emission, not the cutter's nominal diameter. Enlarge the cutter only enough to meet that value with its uncertainty; report actual mouth sizes. As a starting flow criterion, internal connecting throats must carry that same 2.0 mm clear passage [P]. Smaller passages do not silently merge chambers; refine, add openings to each chamber, or leave a newly proposed region solid and report the reduced saving. This is geometric clearance, not a validated flow-rate law.

### Independent H5 extension

Use the checker's own decoded triangles and geometry implementation, with the original input/declared cuts for H7. The checker may use proposed hole descriptions to locate work, but it must find all cavities independently and measure openings itself. Otherwise an omitted cavity disappears from the denominator.

1. **Material and cavities.** Validate orientation/manifoldness, containment and self-intersection. Build bounded adaptive void cells, surface links and opening portals. A drilled interior is connected to outside air, so a connected-component flood alone cannot identify it: use the inner boundary and measured mouths to delimit chambers, temporarily capping mouths only for classification. Check for undeclared pockets and blind passages as well as declared ones. An ambiguous interior/open-recess classification is unverified, not H5 not applicable.
2. **Finished drainage.** On the complete union of model, supports and raft, show that every cavity basin can reach an exterior drain without going uphill against gravity. Show an air route from a distinct vent to each air-trapping region as liquid leaves. Use directed height-level connectivity with certified free cells/links; measure bulk throat clearance separately from boundary-adjacent residual pockets. Eroding a void by a 1 mm ball must not erase a trapped pocket from the coverage count. A merely undirected outside path, or a hole centreline, is not sufficient. Verify that plate, raft and neighbouring supports do not cap a mouth or its discharge route.
3. **Bores.** Independently intersect every support solid, including balls, braces, feet and raft bridges, with the measured bore and its reserved external projection. Zero intrusions, with explicit measurement uncertainty; check vent bores too.
4. **Evidence.** Report each chamber, its volume, drain/vent locations, least clear throat, gravity-trapped volume bound and the first failed region. Give lower/upper bounds; refine ambiguous boundary cells locally. A resource limit or inability to exclude a pocket is unverified. Never claim a pass from selected ray samples or a single working hole.

H5 is **passed**, not not applicable, on a drained hollow sphere. These checks establish geometric routes at stated resolution; they do not prove that viscosity, surface tension, cure bleed or washing leaves no resin. FR-17's owner observation after his normal wash remains required.

### Print-order cup check (FR-17a, reported beside H5)

For every exposure, evaluate the void in the **printed prefix only**, including internal/external supports and raft, with the plate and current film acting as closed boundaries. A future opening or the as-yet-unprinted top of the cavity cannot be an escape path. Track exterior-connected void through the full prefix, allowing a path to turn up/down within already printed layers. Require an adequate, measured liquid passage, not one open pixel. Record first enclosed/narrowed layer, enclosed area and cavity location.

Extend the independent checker with this companion result; do **not** redefine FR-17a as finished H5 or let risk acceptance waive H5. A proved no-cup geometry removes the closed-cup concern, not all fluid resistance. For this first deliverable, require no unresolved cup in the proposed sphere; if a path is too narrow or unassessed, report it and replan holes rather than invent a force coefficient. The existing 0.5 mm² cup warning split in `load/structure.rs` is a starting classification, not a measured suction threshold.

## 4. Drain keep-out is a placement constraint

The owner's [drain-rim ruling](../../reference/owner-notes/drain-rim-ruling-2026-10-22.md) allows tiny contacts at the rim provided their **actual solids** stay outside the bore. The old uniform bore-radius-plus-largest-arm reservation stranded the first sphere layers. His [cluster feedback](../../reference/owner-notes/sweep-field-feedback-2026-09-29.md) also rejects fat overlapping lower posts and needless pairwise bracing.

Create immutable reservations with the frozen shell:

- **Bore volume:** the real through-wall passage, extended to reach unambiguous cavity/exterior space.
- **Below-hole exclusion:** the opening's footprint swept toward the plate (`−z`), stopped at `z = 0`. For a vertical circular hole this is a finite cylinder, not a stripe across the whole model or a continuation below the plate. Include every support part, not only column centre lines. Oblique holes need the bore itself plus the measured mouth's downward projection.
- **Flow corridors:** protected internal routes and exterior outlets used by the drainage/cup proof. Contacts and roots cannot close them. Final checks still use the full geometry, since avoiding a cylinder alone cannot prove drainage.

`probe` must check complete heads against these volumes. `probe_members` must check columns, joint balls, braces, feet and raft. For fast proposals, offset a route obstacle by **that candidate member's radius** plus the measured numerical error; for admission use the rounded native mesh. Do not impose a large fixed contact-free ring. A rim contact is allowed when its patch remains on real material, has certified capacity on its first layer, obeys exterior H1/H2/H6/H9, and its ball/neck stays out of the bore. No unruled sub-0.1 mm “hair” exception.

**Replan the private layout, not the admitted geometry:**

1. Compute islands, first-layer rim needs, coverage and loads on the drilled shell. Search both sides of each hole, including its first annular layers, before distributing load. Never project all rejected contacts onto the nearest rim point.
2. Batch candidates around the remaining surface, spread in angle/height to improve force and moment balance. Keep the current 0.012 mm patch-separation construction reserve and 0.32 mm gap for unrelated external members; check actual head solids rather than large bounding spheres. These are construction margins, not a new owner-mandated tip pitch. Designed load-bearing joins and raft unions are treated separately, not miscounted as accidental overlap.
3. Size for P5 using layer-specific capacity, adding separate small contacts where needed rather than making one rim cluster fat. Later flank contacts may share later layers; they cannot rescue an unsupported first rim layer. Replay the whole load model after each candidate batch and retain load-only additions only when they improve the unresolved condition without worsening another one.
4. Preview routes and brace partners before committing contacts. Keep bracing's current free-length/partner rules, test all braces against reservations, and report an unbraced member rather than constructing an X-pair for every nearby pair. No unassessed long diagonal as an escape from a blocked vertical route.
5. If coverage/P5 and reservations cannot be met, try another private hole/pose plan, then rebuild its supports. On exhaustion, report the missing first-layer anchor/coverage/load and whether the search was exhausted or infeasibility actually proved. Never shrink a required bore, merge exterior patches or delete a need to get a pass.

### Prove keep-out handling before hollowing exists

Use the production review planner on **solid sphere and cylinder fixtures with synthetic forbidden volumes**, plus a flat annular fixture for first-rim-layer cases. This changes placement only: it proves no cavity or drainage result. Compare no-obstacle and obstacle layouts from emitted supports on the same solid load.

Test centred, off-centre and two adjacent 2 mm cylinders; a cylinder that intersects a joint/brace but not its column axis; a near-tangent rim; and a deliberately impossible obstacle covering every first-layer anchor. Assert actual solid clearance, patch separation, no accidental duplicate/overlapping heads, first-layer rooting and time-aware coverage, P5 replay and unchanged failure classification. A case with a feasible alternative must use it; the impossible case must remain unresolved/unprintable, never a low-ratio pass. Record actual obstacle rejections and accepted alternative-route hits so an unchanged layout cannot pretend to exercise replanning. These tests can precede SDF work and directly target the owner's previous failure.

## 5. Permanent inside supports

“Inside” means the enclosed interior of a **drained** hollow body, not a literally sealed cavity and not an exposed recess. Apply exemptions only to geometry proved wholly interior; classify boundary-crossing parts conservatively as exterior. Keep native supports editable and visible as supports in ChituBox, not baked into the model to escape checks.

| Rule | Proposed treatment inside |
|---|---|
| H1, 0.50 mm² scar/contact maximum | Already exempt under FR-19. Permit a larger attachment if the load calls for it. Measure/report its area anyway. |
| H6, removal gap/contact-region length; scar and tool-access targets | H6 and removal requirements already exempt under FR-19. Inner-wall attachment and bracing may be broad; exterior geometry remains unchanged. |
| H2, fused patches | **Still applies in the current PRD.** Propose allowing fused patches wholly inside, since they are never removed, but only after R3 expressly changes H2/FR-19. Until then keep inside patches separate. |
| Neck size | No removal-driven upper cap is needed inside. Size necks/joins for load and available flow space. **H9's 0.1 mm minimum remains**, with the constructor's reserve; there is no thinner-tip exception. |
| H3, peel/load assessment, drainage and cup checks | Unchanged. No future-rooted islands, circular load paths, blocked passages or unsupported cavity ceiling. Stability and thin-shell transfer uncertainty still need located warnings. |

**Root on the inner wall already printed nearest the plate**, outside the openings and their corridors. This matches [owner practice](../../reference/owner-notes/supports-may-seat-on-model.md). A plate-rooted member through a drain would violate the bore rule, obstruct flow and force an unnecessary long member. Use short internal columns/struts and needed braces; their foot must overlap rooted material by the time it starts. The upper ceiling connection does not root a previously floating strut.

Model the dependency explicitly: ceiling → internal support → lower shell → external support(s) → plate. Check local shell transfer and the whole external chain; an internal strut must not be counted as new global plate capacity or cause its carried ceiling area to be charged twice. Add internal members' own peel area once. The rigid-carrier approximation may overstate a thin shell's stiffness: where that response is unassessed, report it rather than treating a connected shell as mechanically proven.

## 6. Peel area and suction

The existing area law remains: peeling material area divided by a certified load-bearing section area. The slicer already represents holes as oppositely wound polygon rings; validate that path on hollow fixtures instead of substituting the outer silhouette or manually subtracting two slab envelopes. The envelope of a difference is not generally the difference of envelopes.

An ideal equatorial slice of the proposed sphere is **π(10² − 7.5²) = 137.4 mm² [D]**, rather than the solid disc's 314.2 mm². Actual mesh slab envelopes differ; drains and support material also change the area. Early layers before the cavity exists still have solid-cap loads. Recompute all layers, first-layer capacities, moments, contacts, member self-peel and overlap from the emitted geometry. A volume-saving percentage is never a peel-load multiplier.

The current load model warns about cups; it does not add a calibrated suction force. Retain that honesty. Design openings to remove closed cups, use the same ring-area law at P5 = 20, and name residual hydraulic uncertainty. If the per-layer path is absent or inadequate, the reduced ring area alone cannot justify printing. A future suction term would need a stated pressure/flow model, printer/profile inputs and physical evidence before it sizes supports. The owner's tilt-release experience (P3) does not waive a detected cup.

## 7. Delivery order, acceptance and sabotage

**Smallest complete review deliverable:** the as-drawn sphere, lifted at least 5 mm, 2.5 mm wall, the two pole openings above, adequate non-overlapping external supports, and permanent interior supports wherever the load/coverage model requires them. Use the same constructor, emitter and independent checker as review mode. No alternate unchecked writer.

1. **Placement first:** synthetic keep-outs on solid shapes; no hollowing needed. In parallel or next, inspect the native hollow/drain/internal-support samples to settle encoding.
2. **One real hollow object:** construct the sphere and declared holes; add H5/FR-17a and H7 permitted-change measurements, interior classification and model-root load propagation. Then generate the supported review file. Scope hollowing requests to explicit review inputs; default transport and approved Round 25 behaviour remain unchanged.
3. **Owner inspection:** preserve all §9.1 review-only names, embedded labels, `print_candidate: false`, budgets, failure status and raw checker report. The contact planner's budget does not bound SDF/cavity checking: add cancellable bounded blocks and report those steps separately. An unverified check stays unverified. A review file with unresolved findings may be shown with those findings, but is not the completed geometry checkpoint and never a print candidate.
4. **Only after that:** agree a print decision and the staged integration. Giant Bat uses the same kernels, not a sphere-special algorithm. Before the treant, exercise multiple chambers, thin limbs, tight throats and increased spatial extent; meet §8.2's performance/repair gate. Do not start full treant iteration to discover basic keep-out defects.

**Review checkpoint checks, measured from the written file:**

- One correctly oriented, manifold material boundary with the intended void and through-holes; 2.5 mm wall target met within reported bounds outside deliberate bore cuts; original exterior and scale preserved outside those cuts. No lost separate component or thin detail. H7 verifies changes against original input, not a generator assertion.
- All applicable hard limits measured, no failed/unverified result for the completed checkpoint; H5 drainage/venting and bore checks positive; independent FR-17a result has no unresolved cup. Count chambers independently and report how many were proved, not only how many we generated.
- Zero unrooted first-layer supports or unanchored non-exempt islands; time-aware rim/ceiling coverage; additive and valid rigid-sharing sizing ≤ 20, with other model uncertainties explicitly located. Additive > 40 or absent rooted material routes keep their unprintable classification. Inside supports neither add fictitious external capacity nor block flow.
- Report per-hole actual diameter/location, thinnest hollow wall, narrowest passage, trapped-volume bounds, model/support/net resin, worst ratios/sections, spacing and braces, time per step and job peak memory. Counts name what they do **not** prove: zero bore collisions is not drainage proof; zero detected cups is not measured suction; supports emitted is not print success.
- Repeated 1-thread and allocated-thread runs deterministic. Sparse blocks, layer bands, triangle buckets, compact void runs and one bounded worker pool; reuse buffers and batch distance, collision, slicing and passage queries. Stay below the 100 GB job ceiling and pursue the 4 GB/model target. Do not raise a memory cap to accommodate a per-ray/per-triangle allocation. Measure growth by spatial extent, refinement, passages, cavities and support count, not triangle count alone.

**Targeted sabotage files**, made through the production writer and checked independently:

| Mutation/control | Required result |
|---|---|
| Seal one sphere hole; add an unlisted sealed chamber; leave only one opening | Missing vent/drain or pocket detected; no genus-zero H5 exemption. |
| Put a hole above a gravity basin; join chambers by a narrow/uphill neck | Final drainage fails even though an undirected outside path exists. |
| Stop a bore just short of the cavity; reduce its inscribed width after rounding | Full-width penetration/clearance fails. |
| Move the early opening to a later layer while keeping final two-hole drainage | H5 can pass; FR-17a must catch the earlier cup. |
| Add a ball/brace across a bore, raft under its mouth, or an internal strut dam outside the bore | Bore or flow/pocket check fails; checking only columns/cylinders cannot pass it. |
| Delete the first rim anchor; duplicate nearby external patches; enlarge a head after routing | H3/needs, H2 or admission/replay fails; no late geometry mutation escapes. |
| Mislabel an outside support as inside; fuse inside patches before R3 approval | Exterior limits and current H2 still fire. |
| Root an internal strut on future shell material; count it as an extra plate contact | Continuity/load ledger tests fail; no apparent capacity bonus. |
| Reverse the inner shell, fill the annulus in slicing, erase exterior detail under a broad “hollow” declaration | Material/layer oracle or narrowly scoped H7 rejects it. |
| Shift critical throat/rim across voxel, pixel and layer phases; force refinement/resource exhaustion | No sampling-phase false pass; exhaustion is unverified, never an empty successful result. |

Pair each negative with a valid neighbouring control and assert path-hit counts for rejection, replanning, interior classification and drain proof. These tests show each check fires; they do not prove the physics model sufficient. Run the changed tests, formatting/lint and one final focused integration check before landing implementation, not full-corpus gates on every edit.

**Owner's ChituBox review:** show solid versus hollow sphere side by side, same scale/pose; external view of the earliest rim and support spacing; section views through both holes and all inside roots; the first hollow layer, widest ring and ceiling layers. Captions identify “nearest plate / prints first” and “downhill drain when hanging.” Confirm native support edit/delete, save/reopen, slicing and island report on the exact hashed file. Ask whether the lower cluster is credible and removable, whether hole locations are acceptable, and whether internal support/flow space looks adequate. A print request is separate; after any approved print record trapped resin after normal washing, shell success and external support removal. No stage passes from this sphere review.

## 8. Owner rulings needed

Each question stands alone; suggested answers do not claim approval.

1. **First wall:** Keep **2.5 mm** for the 20 mm review sphere? This is your current working ChituBox wall setting (P19), not a measured minimum. It leaves a 15 mm inner diameter and removes about 1.77 mL before holes/supports. Recommendation: yes; do not start thinner merely to save more resin.
2. **Two pole holes and the fixture exception:** Approve a **2.0 mm clear hole at each pole** for this review sphere—one facing the plate to vent the cavity from its first layers, the other opposite it to drain resin downward when the completed part hangs from the raised plate? Neither sphere face is known hidden. This explicitly hollows a small test object below the proposed 10 mL automatic threshold; it changes no normal small-mini selection rule. Recommendation: yes for review only, not permission to print.
3. **Fused interior attachments:** May contacts merge **wholly inside a drained, inaccessible cavity**, where supports stay permanently, while all exterior contacts remain separate and load, H3, H9 and drainage rules remain unchanged? Current FR-19 expressly retains H2, despite the new request for more relaxed interior supports. Recommendation: permit interior fusion; until an explicit ruling updates the contract, implementation must retain H2 there.
4. **Development order only:** May the sphere hollow/drain/keep-out review task proceed now, before stages 1–4 have all passed their required prints? Recommendation: allow this named review task so the known drain/support conflict is solved early; do not waive any stage print, call the sphere a milestone, or start Giant Bat/treant acceptance work early.

No new ruling is needed to root inside supports on the inner wall or to remove H1/H6's interior removal/scar restrictions; FR-19/FR-27 already allow those. The open decisions are not permission to relax peel sizing, obstruct a drain or waive a failed hard check.

**Risk and rollback:** biggest risks are false cavity/drain proofs, incorrect void winding, falsely exempting exterior geometry, and treating model-rooted supports as extra capacity. Keep provenance and originals immutable, validate from emitted geometry, fail closed on ambiguity, and keep outputs review-only. No data migration or external service is required. Reverting this feature restores the current default path without changing source STLs or native fixtures. Any future approved rulings must update the PRD and its mirror before implementation relies on them.
