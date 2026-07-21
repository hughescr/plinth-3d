# Support placement design charter

## Objective (the user's, verbatim in spirit)

Minimize total model damage + removal effort, subject to every region
held within mechanical safety margins. Fixed palettes are a UI
convenience, not engineering: the record format stores arbitrary float
radii, so sizes are COMPUTED per support.

Hard requirements from the user:
1. Easy removal: pryable, not too densely packed (spacing floor;
   tip necking so supports snap at the tip, never tear the model).
2. Minimize surface marking, ESPECIALLY delicate regions. The
   decimation stage's protected-verts/error map doubles as the
   delicacy field - scars are priced by local detail.
3. Scar cost feeds ORIENTATION too: rotate delicate regions "up" in
   slicer space (away from support-facing down/side surfaces) - same
   cost function, applied one stage earlier.
4. Model-to-model struts are first-class: sometimes the model is a
   better base than the plate. Price honestly: two scars + short stiff
   member vs one scar + long slender shaft + braces.

## Mechanics framework (bottom-up MSLA)

- Dominant load = PEEL: a few kPa over each region's newly-cured area,
  arriving as TENSION in that region's supports. Gravity ~cancelled by
  buoyancy. Supports are GREEN resin: tensile ~10-25 MPa, E ~0.5-1.5
  GPa (vs ~40-60 MPa / ~2 GPa post-cure). Design green, with SF 5-10.
- Consequence: shafts almost never fail in tension. The TIP is the
  fuse: tip radius = f(carried area x peel pressure x SF), clamped to
  a removal-friendly 0.2-1.0mm. Carried area via Voronoi partition of
  down-facing surface among contacts.
- Second driver is STIFFNESS, not strength: deflection ~ L^3/r^4;
  long slender shafts whip under resin drag during lifts -> ribbing.
  Cap slenderness given free length; where the cap demands ugly-thick
  shafts, ADD BRACES instead (halving free length cuts deflection 8x)
  or split the load with another contact.
- Articulation: tip stub enters along surface normal (2-3mm), elbow,
  vertical descent. Records are polyline chains natively (type-3
  joiners); kinked+braced is native vocabulary.

## Calibration epistemics (measure, don't guess - and grade the source)

The user's support profiles are NOT Chitubox defaults: they came from
earlier LLM (Opus) conversations improving on defaults, without
holistic placement redesign and without checking support lore against
physics. Treat them - and the 469-support corpus generated with them -
as EVIDENCE OF SUFFICIENCY (these prints succeed), never as optima.
Policy: physics decides; the corpus bounds the feasible region from
above (3.1mm median tip spacing is likely OVER-dense); print outcomes
are the final arbiter. v2 ships a defensible-margin physics config
(SF over green-state properties), then walks density/size down across
test prints. Corpus remains useful for: brace placement vs free length
(slenderness constant), elbow geometry, realized spatial distribution
(where a competent-if-unprincipled algorithm judged support necessary).

## v0 -> v2 work list (priority order)

1. Clearance-envelope routing (v0 checks ray centerline only: 27/137
   shafts violated 0.6mm standoff, some clipping the model).
2. Density calibration to measured 3-4mm class-dependent spacing.
3. Load partition -> continuous tip/shaft sizing (framework above).
4. Articulated routing (normal stub + elbow + descent; kinked crevice
   escapes - v0 runs full-shaft diagonals).
5. Brace graph (neighbor shafts within cross-width; ladder spacing
   from config; slenderness-triggered).
6. Model-to-model struts with two-sided scar pricing.
7. Scar-cost field (delicacy from decimation error map + visibility)
   used in BOTH placement and orientation scoring.
8. Emission via injection (requires synthesized-mesh validation).
9. Self-verify stays mandatory: zero unsupported islands, clearance
   check, plus per-region load check against green-strength SF.

## v2 status (implemented)

All nine items above are implemented in `scripts/support_placement/`
(package layout in its `__init__.py`) with a pytest suite
(`scripts/tests/`, 51 tests) covering each module on synthetic fixtures,
plus a regression test that re-derives the corpus's own documented
figures (469 supports, 261 bridges, ~3.1mm median spacing, ~40%
bottom-quartile share) from `supported-treant.chitubox` directly, to
cross-validate the format-reading code against the earlier
reverse-engineering work.

Design choices worth flagging (deviations from a literal v0 rebuild,
made deliberately, documented in-module):
- **Islands** are a face-adjacency-graph construction, not v0's voxel
  grid, merged across `cfg.island_merge_radius_mm` of spatial proximity
  as well as true mesh-edge adjacency -- pure topological adjacency badly
  over-fragmented the Treant's organic/bark-textured surface (21,243
  singleton-heavy islands before the merge radius; 146 after, close to
  v0's 54).
- **Scar-cost/delicacy field** is a standalone geometric proxy (local
  curvature + a KDTree opposite-normal thickness estimate), not a live
  read of decimate_pipeline.py's error map (that map isn't persisted
  anywhere Blender isn't running) -- documented as the integration point
  if a persisted decimation map shows up later.
- **Clearance checking at real-model scale required two performance
  rewrites**, not just correctness fixes: (a) trimesh's non-embree ray
  intersector does not amortize across a ray batch (measured: ~50 rays
  ~830MB RSS, ~200 rays ~2.2GB, OOM by ~500 on the 576k-face Treant in a
  3.8GB sandbox) -- `perception.grounded_mask` now casts one ray per
  occupied XY column instead of one per down-facing face; (b)
  `trimesh.proximity.closest_point` measured ~0.4-2s PER CALL at this
  mesh size, intractable for the hundreds of calls one placement run
  needs -- `routing.build_surface_tree` + a dense-point-sample KDTree
  (`workers=-1`) replaced it for anything beyond a handful of calls.
  `trimesh.curvature.discrete_gaussian_curvature_measure` has a similar
  steep-memory-per-sample profile and is now capped at 4000 samples with
  nearest-neighbor propagation to the full sample set.
- **Model-to-model struts (#6) got a locality cap** (`strut_max_length_mm`,
  20mm default) after an uncapped "nearest safe point anywhere on the
  model" search chose model anchors for 307/353 contacts on the Treant,
  most producing clearance violations from the strut's foot segment
  clipping unrelated branches on its way to a distant anchor. design.md's
  own framing ("sometimes the model is a better base than the plate") is
  about local bracing, not long-reach struts.
- **Clearance repair is a bounded local search** (`engine.
  _repair_clearance_with_detours`): a stub+elbow+descent kink is not a
  general path planner, so when the straight descent is blocked, the
  engine tries a bounded set of lateral detour offsets before descending,
  and falls back a model-strut contact to the plate if ITS route can't be
  made to clear. This is a deliberate scope boundary, not an oversight --
  see the real-world result below for how far it gets, honestly reported
  rather than silently forcing `ok=True`.

## v2 real-world result (Weeping Treant v1.2.stl, 576k faces)

Full pipeline, single `engine.run()` call, ~26s wall time on a 4-core/
3.8GB sandbox:

| | v0 (proto, ray-centerline check) | v2 |
|---|---|---|
| supports | 137 | 359 |
| islands found | 54 | 146 (finer decomposition, see above) |
| clearance violations | 27/137 (centerline only -- undercounts) | 116/359 (full swept-capsule envelope) |
| uncovered islands after fix pass | 0 | 0 |
| load (green-SF) violations | not checked | 0 |
| braces | not modeled | 584 |
| model-to-model struts | not modeled | 0 survived verification (307 attempted, self-corrected to plate -- see locality cap note above) |

The clearance numbers are NOT apples-to-apples: v0 checked only the ray
centerline, v2 sweeps the full segment+radius+standoff envelope, so a
stricter, more honest check finding more violations on the same
underlying geometry is expected, not a regression. The 116 remaining
violations are real and are surfaced by self_verify rather than hidden --
this is a densely-branched organic model where a fixed-topology 2-kink
router genuinely cannot route around every obstruction; the corpus's own
469 hand/magic-support-generated supports on the same model is the
existing evidence that a real slicer's fuller path search handles cases
this engine's bounded local search doesn't. `treant_v2_results.json` (in
the plugin root) has the full run log; `treant_v2_supports.chitubox` (in
the project folder, sibling to the calibration corpus) is the emitted,
re-parse-verified artifact -- a CLEAN v2-only plate (the corpus's 469
supports are dropped via `Project.write_replace`; append-only stacking
made iteration review impossible).

### Emission frame lessons (first in-Chitubox review, 2026-07-20)

The first emitted artifact failed visual review in three ways, all now
fixed in `manual_emit_treant.py` + `chitubox_inject.py`:

1. **Coordinate frame.** Record coordinates are model-centered (the
   format map said so; emission ignored it and wrote raw-STL-frame
   coordinates). Every v2 support rendered 64mm too high -- a mid-air
   forest above the model. Verified fix: corpus tips sit at median
   0.30mm from the CENTERED STL surface (= contact embed depth, no
   rotation); v2 tips now match at 0.35mm. The mesh is centered before
   engine.run so all downstream geometry is natively in record frame.
2. **Scene lift.** cfg default lift_mm=5 but the corpus scene lifts the
   model 3.0mm (plate z = -66.976 = centered zmin - 3.0). Emission now
   derives lift from the project's own pad geometry.
3. **Raft connectivity.** The corpus raft is pads + 261 bridge boxes;
   v2 emitted bare pads, which slice as disconnected dots. Emission now
   ties all v2 pads into one raft with minimum-spanning-tree bridges
   (358 edges, longest 6.6mm, within the corpus's own 0.07-10.3mm
   bridge-length range).

### v3.1: build-time stability (second review, 2026-07-20)

Craig's second in-Chitubox review caught that a 65mm bare vertical run
"will dangle loose from the plate as we build" -- and the engine's own
deflection formula agrees: r=1mm at L=65mm gives ~2.3mm tip deflection
under even the configured 0.01N lateral load, 15x budget. Root cause of
BOTH that and the identical-thickness look: shaft_radius_for_length_mm
capped radius at max_unbraced_shaft_r_mm by assuming braces would be
added, and nothing reconciled the debt when no neighbor was in reach --
every tall trunk converged to the cap, slender and unbraced. v3.1:

- braces are height-driven (any trunk taller than brace_free_span_mm
  gets ladder rungs at that cadence, regardless of the static calc);
- after braces are PLACED, trunks.resize_for_spans re-sizes each trunk
  for its real worst build-time span (gaps between raft / junctions /
  brace attachments), unclamped -- isolated tall trunks now come out
  visibly thick (Treant radii: 0.8..2.0mm, was uniform ~0.8);
- trunks.consolidate merges parallel twin/triplet columns within 3mm;
- contacts whose column never cleared get a MODEL-ANCHORED strut
  (adsorbed foot, like the corpus's own 227 model-adsorbed supports;
  steeper 55-degree budget since it's rooted on printed model, not
  dangling from the raft) instead of a through-model column;
- joint balls sized to exactly match the fatter member (1.25x read as
  lumpy in review).

Treant result: 142 trunks + 29 struts serve 359 contacts, 536 braces,
92 clearance violations (v2: 116) -- the residual 45 unclear contacts
sit under dense canopy where neither a column, spiral offset, nor
<=32mm strut clears; that's genuine path-planning territory (follow-up
#2 below).

### v3.2: the no-intersection guarantee (third review, 2026-07-20)

Third review found supports still running THROUGH the model. Three
holes, all fixed: (1) placement checked columns at min_trunk_r but
span-resizing fattened them afterwards, eating the whole margin; (2)
braces were never clearance-checked (the X lattice was cutting through
leaves); (3) the give-up contacts still emitted their known-bad columns.

The engine now runs a stability loop where every member is re-checked
at its FINAL radius: violating trunks are re-searched at final radius,
then dissolved if nothing clears; braces are clearance-filtered (a
dropped brace lengthens the span and correctly thickens its trunk --
and resize_for_spans is now AUTHORITATIVE, so a well-braced trunk can
come back DOWN to slender, which clears where fat clips); trunks with
no reachable neighbor get short rungs to the MODEL (two-end-supported
bridges <=5mm, like Chitubox's own support-to-model mini braces); and
whatever still can't clear is demoted to a model strut or reported
unrouted -- NEVER emitted through the mesh. Test-pinned: the table
fixture asserts clearance_violations == [] always.

Treant v3.2: 281 supports (230 trunk-attached + 51 model struts), 82
trunks, ZERO clearance violations, 76 contacts on ~55 small canopy
islands honestly unsupported (no clear column, spiral offset, strut, or
model brace reachable -- these need real path planning or manual
supports; the slicer UI shows exactly where, since the islands are
uncovered).

## Known follow-ups (recorded, not silently dropped)

1. ~~**Vectorize the per-contact routing/repair loop.**~~ **DONE.**
   Profiling on the real Treant showed the dominant cost wasn't even
   interpreter overhead -- it was scipy spawning a FRESH thread pool for
   every tiny per-segment `cKDTree.query(..., workers=-1)`: 74,535 thread
   spawns accounted for ~90% of the repair loop's wall time. Fix, in two
   layers (routing.py): (a) `check_clearance_batch` concatenates sample
   points across many routes into ONE surface query (and the detour
   repair builds its whole 60-candidate fan and checks it in one call),
   so `workers=-1` finally gets work big enough to amortize the pool --
   `query_workers()` now refuses to spawn threads below 4096 points; (b)
   `distance_upper_bound=max(radius)+standoff` on the query, a pure
   traversal-pruning optimization (any point at least that far has
   clearance >= 0 regardless of true distance -- provably identical
   violations, verified by test). Result: repair pass 10.7s -> 1.28s
   (8.4x), route-for-route identical output; full engine.run on the
   576k-face Treant now 7.1s end-to-end with byte-identical final
   numbers (359 supports, 584 braces, 116 clearance violations). Three
   regression tests pin batch==single equivalence and pruning-neutrality.
2. **116 residual clearance violations on the Treant.** Options not yet
   tried: multi-hop detours (current search is a single lateral jog),
   routing along the mesh's own accessibility gradient instead of fixed
   compass directions, or accepting model-strut routes with THEIR OWN
   local detour repair (currently only plate routes get repaired --
   model-strut violations just fall back to plate rather than being
   locally fixed).
3. **peel_pressure_pa and shaft_drag_force_n are rough estimates**
   (config.py flags these explicitly) -- design.md's own calibration
   epistemics call for walking density/sizing against real print
   outcomes; that loop hasn't run yet because no v2-supported print has
   been made.
4. **Pad geometry is cloned, not measured for the Treant's own feet.**
   `place_pad` clones one template pad from the corpus and translates it
   -- foot diameter (4.4mm from the template) may be oversized or
   undersized for some of the Treant's smaller/larger tip classes;
   continuous foot sizing (matching the continuous tip/shaft sizing this
   whole engine is built around) is unimplemented.

### v3.6: layer-island minima directly supported (this session)

Queue item 0 (Craig, TOP PRIORITY), upgraded mid-session by Craig from
"contact within ~1mm" to a CONNECTION-TIME rule: proximity is the wrong
metric; what matters is how many layers pass before a birth island
connects to anything. Implemented as perception.local_z_minima:

- every strict 1-ring local minimum (eps ties merge flat plateaus into
  one representative) births a cross-section island;
- a bounded flood inside the birth band [z_min - eps, z_min + tol]
  (tol = minima_connect_layers * layer_height_mm, default 2 x 0.05mm)
  exempts a minimum only if it reaches a strictly LOWER vertex --
  earlier-printed material, inductively supported at its own birth by
  the same rule. The lower vertex terminates the flood (traversing it
  would flood the whole model below -- first implementation did, and
  took minutes on the Treant instead of ~3s);
- NO near-plate exemption (unlike grounded_mask's island rule): lifted
  scenes make the model's own lowest point the FIRST island of the
  print. Consequence, test-pinned: a flat-bottomed box honestly gets
  one bottom-plateau contact ("box needs no supports" was a miss);
- engine._ensure_minima_contacts adds a contact AT each unmet minimum
  (synthetic islands for minima the overhang-angle model can't see --
  steep-flank toes); verify.minima_report gates on ROUTED contacts
  touching the birth region (min gap <= minima_contact_radius_mm) and
  unsupported_minima is part of verify ok.

Two engine bugs this fixture flushed out: (1) trunks.consolidate merged
trunks and grew the survivor's column without the column-extension
clearance check plan()'s join path has -- six under-cone contacts merged
onto a column through the cone apex; (2) cluster dissolution was
all-or-nothing -- when relocation can't serve every shoot from one xy,
solvable singles died with the cluster. The stability loop now retries
each shoot as a SOLO column (spiral search) before strut/unrouted.

Treant: 379 minima detected in ~3s (median birth region 2 vertices,
max 59); global-min rearmost toe at z=-63.98 caught.

### v3.6: global joint-ball pass (queue item 1, Craig's design)

joints.py replaces every per-call embedded ball (elbow/junction balls in
synth_v3_support, dome + Y-split ball in synth_v3_trunk, end balls in
brace_record, bundle_shoots' hub_ball_r). After ALL geometry and radii
are final: collect every member; cluster joint points (endpoint
coincidence within 0.15mm, PLUS on-axis proximity 0.2mm for balls
landing mid-member -- shoot junctions on trunk columns, rung ends on
sibling shoots); emit exactly ONE ball per joint sized to the FATTEST
member touching it. Kills the gapped-undersized-ball class (e.g. the
Y-split ball was sized by the CHILD; the parent column through the
joint can be fatter) and dedups the doubled X-brace endpoint spheres.

Ball ownership: each ball is appended into the fattest touching
member's record blob, so record count and Chitubox grouping are
unchanged. Conventions kept: the tip contact ball is a contact, not a
joint (stays in synth_v3_support); a lone model-embedded strut foot
keeps the 1.4x adhesion fillet. Also fixed in passing: stage_emit
zipped routes with tip_r, but routes exclude unrouted contacts -- tip
radii after the first unrouted index were off by one; now looked up by
route.contact_idx.

### v3.6: branch angle 60 + angle-aware peel load (queue item 2)

branch_angle_deg = 60 now governs Y-diagonals, hub fans, risers, and
the strut anchor cone (strut_fallback_max_angle_deg 55 -> 60); plain
trunk shoots stay at 45 (length-optimal there). PAIRED, as required,
with the angle-aware load term in placement._shaft_r_for_L:

    F_eff = F_drag + peel_pressure * ellipse_area(theta) * sin(theta)
          = F_drag + peel_pressure * pi * r^2 * tan(theta)

(an inclined member's layer slice is an ellipse of area pi*r^2/cos t;
the peel normal load on it resolves a lateral bending component
proportional to sin t). F_eff depends on r, so the solve is a
fixed-point iteration (contracts, net exponent 1/2); theta=0 reduces
exactly to the old closed form (test-pinned). max_unbraced_len_mm takes
the same angle closed-form. Members now sized at their ACTUAL lean:
shoots at 45, X-brace rods at 45, risers and struts at their real
geometry. Y-diagonals stay drawn at the child column radius -- at their
bounded length (<= ~12mm) the angle-aware requirement never exceeds
min_trunk_r, checked, documented in coalesce_columns.

Search-space widening comes from the geometry itself (same lateral
reach costs tan60/tan45 = 1.73x less height under the junction/z_top
ceilings); the explicit xy economy caps (trunk_branch_merge_xy_mm,
shoot_max_xy_mm) were left unchanged deliberately.

### v3.6: twin-column merge cleanup (queue item 3)

Two prongs. (a) consolidate() now takes plate_z and, when the direct
move (gone's shoots re-aimed at survivor.xy) fails clearance, retries
the merge at the MIDPOINT xy -- every shoot of BOTH trunks re-aimed and
checked (reach/plate-floor guards, segment clearance, full-height
column clearance) before the survivor relocates there. (b) twins at or
under twin_single_brace_xy_mm (3.0) that still can't merge get single
ALTERNATING brace diagonals instead of crossed X pairs -- between
near-parallel columns millimetres apart the second rod of each X adds
no stiffness, only lattice clutter. Single rods are sized WITHOUT the
X-pair 0.5^0.25 load-sharing discount, and the brace dict's i/j keys
follow the geometry (p0 always on trunk i) so resize_for_spans'
attachment bookkeeping stays truthful. Implemented by a Sonnet
subagent to this spec; 86 tests green.

## Next-session queue (agreed with Craig, 2026-07-20 session end)

1. ~~**Global joint-ball pass (Craig's design, replaces scattered per-call
   sizing).**~~ **DONE (v3.6 above).** After ALL geometry and radii are final: collect every
   member (stubs, shoots, trunk columns, Y-diagonals, risers, braces,
   rungs); cluster joint points (endpoint coincidence within tolerance,
   PLUS on-axis proximity for balls landing mid-column on a trunk or
   riser); emit exactly ONE ball per joint sized to the FATTEST member
   touching it. Strip the per-member embedded balls from
   synth_v3_support/synth_v3_trunk/brace_record. Kills the
   gapped-undersized-ball bug class entirely and dedups doubled spheres.
2. ~~**branch_angle_deg ~= 60 for Y-diagonals, hub fans, risers, struts**~~ **DONE (v3.6 above).**
   (45 stays for plain trunk shoots -- it is length-optimal there),
   PAIRED with the angle-aware load term in the deflection solve
   (F_eff = F_drag + peel_pressure * ellipse_area(theta) * sin(theta))
   so steep members are honestly fattened. Widens the coalesce/bundle
   search spaces and the strut anchor cone (attacks the ~60 unrouted
   canopy contacts).
3. ~~Twin-column merge improvement (doubled X-lattice between unmerged
   twins is cosmetic waste).~~ **DONE (v3.6 above).**

0. ~~**LOCAL Z-MINIMA MUST BE DIRECTLY SUPPORTED (Craig, session end --
   TOP PRIORITY, correctness not optimization).**~~ **DONE (v3.6 above;
   spec upgraded by Craig to the layers-until-connection rule).** The rearmost toe case:
   a local minimum's first layers cure as a disconnected island stuck to
   the FEP (glob/tear risk) until something connects. The overhang-angle
   island model MISSES minima whose flanks are steeper than the 45deg
   down-facing test, and spacing-based coverage allows the nearest
   contact ~6mm away -- useless for a layer island. Implement: detect
   local z-minima (vertex v minimal within an xy-disk radius via KDTree,
   eps for flats), require a contact within ~1mm of EACH minimum, add
   dedicated min-contacts + routes where missing (minima are bottom-most
   so straight plate columns usually clear). Verify gains a
   layer-island check: every minimum has a contact at its lowest point.
   Full rigor later = per-layer connected-component flood from plate
   (what slicers do); the minima check catches toes/fingertips/drips.
