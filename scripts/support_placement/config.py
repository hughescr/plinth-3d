"""Physics constants and tunable defaults for the v2 support engine.

Every number here is either (a) taken directly from the design charter
(references/design.md) or (b) an explicitly-flagged rough estimate pending
empirical calibration from real prints. Nothing is silently guessed: if a
constant is soft, its docstring says so and `SupportConfig.notes()` surfaces
it in run output so the caller sees what to recalibrate.

Units: millimetres, Pascals, Newtons, GPa/MPa where labelled. All physics
math converts to SI internally and back to mm for output.
"""
from dataclasses import dataclass, field, asdict


@dataclass
class SupportConfig:
    # ---- Mechanics framework (design.md "Mechanics framework") ----
    # Dominant load is PEEL: a few kPa over each region's newly-cured area.
    # NOT measured for this printer/resin yet -- design.md says "a few kPa";
    # 3.0 kPa is the geometric mean of a plausible 1-5 kPa range. Flagged
    # soft: recalibrate from print outcomes (tip failures -> raise; excess
    # tip mass / hard removal -> lower).
    peel_pressure_pa: float = 3000.0

    # Green-state (uncured, as-printed) resin properties. design.md: tensile
    # ~10-25 MPa, E ~0.5-1.5 GPa. Using the CONSERVATIVE (weak) end of both
    # ranges for sizing math, since under-sizing the tip is the failure mode
    # that ruins a print; over-sizing merely costs a slightly harder snap.
    tensile_green_mpa: float = 10.0
    modulus_green_gpa: float = 0.5

    # Tip is the designed fuse (SF 5-10 per design.md); 8 sits mid-range,
    # weighted toward the safe side since tip mass is cheap.
    tip_safety_factor: float = 8.0

    # Tip radius clamp -- explicit in design.md: "clamped to a
    # removal-friendly 0.2-1.0mm". This is a hobbyist-handling constraint,
    # not a physics one; do not derive it.
    tip_r_min_mm: float = 0.2
    tip_r_max_mm: float = 1.0

    # ---- Shaft stiffness / slenderness (design.md "second driver") ----
    # Point lateral drag force approximating resin-lift whip on one shaft.
    # design.md gives the qualitative relation (deflection ~ L^3/r^4,
    # standard cantilever point-load form F*L^3/(3EI)) but no measured
    # force. This value is a ROUGH ESTIMATE (order-of-magnitude guess at
    # peel-scale drag on a ~1mm shaft) pending calibration against observed
    # ribbing on real prints -- treat shaft_r outputs as relative/ordinal
    # (which shafts need bracing) more confidently than absolute.
    shaft_drag_force_n: float = 0.01

    # Maximum allowed lateral deflection at the tip under shaft_drag_force_n
    # before a shaft is considered whip-prone (drives visible ribbing).
    max_deflection_mm: float = 0.15

    # Above this shaft radius a shaft is "ugly-thick" per design.md --
    # instead of thickening further, split the free length with a brace.
    # Set near the corpus's realized Heavy shaft radius (0.75mm) so v2
    # doesn't casually exceed what's already known to work.
    max_unbraced_shaft_r_mm: float = 0.9
    min_shaft_r_mm: float = 0.3

    # ---- Density calibration (design.md "v0 -> v2 work list" #2) ----
    # Corpus realized median tip spacing was 3.1mm -- documented as an
    # UPPER BOUND on required density (evidence of sufficiency, not
    # optimum; likely over-dense). v2 ships looser than that and expects to
    # walk density DOWN (i.e. spacing further up) across test prints if
    # they succeed. Default sits at the sparse end of the measured
    # "3-4mm class-dependent" range from the charter.
    target_spacing_mm: float = 4.0
    # Hard floor regardless of load calc -- "pryable, not too densely
    # packed" (design.md requirement 1). Never place two tips closer than
    # this even where load math would allow it.
    spacing_floor_mm: float = 2.5
    # Small/isolated islands still need at least this many contacts.
    min_contacts_per_island: int = 1

    # ---- Overhang / island detection ----
    # A face counts as needing support if its normal points more than this
    # many degrees below horizontal (down-facing) AND is not already
    # shadowed by model geometry directly beneath it.
    overhang_angle_deg: float = 45.0
    # Voxel pitch for island/accessibility scanning. Matches v0's working
    # scale (supgen.log: 261x175x335 grid on a ~130mm model, pitch ~0.4mm).
    voxel_pitch_mm: float = 0.4
    # A down-facing face is exempt (self-supported) if a straight-down ray
    # from it hits more model geometry within this gap -- a short bridge
    # doesn't need a support, the cured layer stiffness spans it.
    bridge_free_gap_mm: float = 1.0
    # A down-facing face within this height above the actual build plate
    # is exempt -- close enough to plate/raft that no dedicated pillar is
    # needed. Lifted model faces are not plate-grounded.
    near_plate_mm: float = 1.5
    # Islands merge across this spatial gap in addition to true mesh-edge
    # adjacency -- pure topological adjacency badly over-fragments organic/
    # textured surfaces (bark, scales: thousands of singleton islands from
    # normal noise at the triangle scale) into islands too small to be a
    # meaningful "this region needs one contact" unit. This is a
    # geometric closing radius, not a physics constant -- tuned to bridge
    # surface-texture-scale gaps without merging genuinely separate
    # overhangs (should stay well under target_spacing_mm).
    island_merge_radius_mm: float = 2.0

    # Distance the model sits above the plate in the slicer (matches v0
    # proto's lift_mm=5.0 for the Weeping Treant run). Anchors route to
    # z = model_zmin - lift_mm in the engine's own local frame.
    lift_mm: float = 5.0

    # ---- Routing / clearance (design.md #1, #4) ----
    # Standoff: minimum gap kept between any support surface and the model
    # (other than the contact point itself). v0 shipped 0.6mm as centerline
    # clearance and still had 27/137 violations because it only checked the
    # ray centerline, not shaft radius. v2 checks the full swept envelope
    # (segment + radius + standoff), so this margin is now genuinely free
    # clearance, not a proxy.
    standoff_mm: float = 0.6
    # Length of the tip stub travelling straight along the surface normal
    # before the elbow into the vertical descent (design.md #4).
    stub_length_mm: float = 2.5
    # Tip embeds slightly INTO the model surface (format.md: type-7 tip
    # "start = contact point 0.3mm INSIDE surface" -- native convention).
    contact_depth_mm: float = 0.3
    # Sampling step along a segment when sweeping the clearance envelope.
    clearance_sample_step_mm: float = 0.3
    # Percentile of routing.surface_tree_pitch_bound_mm's own nearest-
    # neighbor-gap distribution used as the sampling-uncertainty term in
    # an embed-end clearance exemption (tree.Forest.revalidate,
    # verify.clearance_report -- p6-routing Finding B). A designed-embed
    # anchor point (struts.py's candidates, bracing.py's model_rung feet)
    # is drawn from a DIFFERENT sample than the surface_tree clearance is
    # checked against, so the exemption must budget for how far that
    # anchor could legitimately sit from surface_tree's nearest actual
    # point. build_surface_tree's own docstring calls its approximation
    # error "roughly the sample pitch" -- an AVERAGE-case claim for a
    # random (non-gridded) point sample; measured on the real Weeping
    # Treant model (nominal pitch_mm=0.3): p95=0.28mm, p99=0.35mm,
    # max=0.61mm across 400k points, while the specific anchor that
    # crashed engine.brace_and_size's hard assert (member owner=
    # ('contact', 421)) measured a 0.537mm real gap -- inside the tail
    # p95 misses but p99 covers with margin. 99.0 (not 100/max) is the
    # deliberate choice: max over ~400k points is a single-outlier-
    # sensitive statistic (one degenerate sliver triangle could set it
    # arbitrarily high on some other mesh); p99 is high enough to close
    # every measured Treant violation (all < 0.06mm, comfortably inside
    # the p99-p95 margin) while staying a robust, mesh-representative
    # measure of "how sparse does this cloud legitimately get" rather
    # than chasing a single worst point (design-v4.md's gate-safety bar:
    # "grows only as far as the anchor geometry itself requires, never
    # further").
    surface_pitch_bound_percentile: float = 99.0

    # ---- Printability angle limit (v3, from in-Chitubox review) ----
    # No support member may lean more than this many degrees from
    # vertical. Physics: at angle theta from vertical, each cured layer's
    # cross-section shifts laterally by layer_height*tan(theta) relative
    # to the one below -- layer-on-layer continuity itself survives to
    # steep angles for mm-scale members, but the practical limits are (a)
    # green-state bending of the inclined member under peel (unsupported
    # moment arm grows with tan(theta)) and (b) sag of near-horizontal
    # spans, which print as unsupported bridge islands until both ends
    # merge. 45 degrees keeps every layer >90% seated on cured material
    # for members >=0.7mm across AND keeps the peel moment arm bounded.
    # Pure horizontal runs are BANNED except two-end-supported raft
    # bridges at plate level.
    max_segment_angle_deg: float = 45.0

    # ---- Bracing (design.md #5; v3: diagonal X pairs, not horizontal) ----
    # Neighbour trunks within this cross-distance are brace candidates.
    brace_neighbor_max_xy_mm: float = 15.0
    # ---- Adaptive cross-link reach (p6-reach, "the pillar fix") ----
    # Hard ceiling on how far bracing.py's cross-link search may widen
    # PAST brace_neighbor_max_xy_mm, expressed as a multiple of that base
    # cap. Forensics (Craig's screenshot find, member 2337 serving contact
    # 240): a 112.4mm fully-unbraced plate column sized to r=2.9947 by
    # r_transient (the only member over the pillar's own reasonableness
    # ceiling) -- compute_needs's activation trigger fired correctly (6
    # requests), but every candidate at every requested height failed
    # honestly: model rungs clip the overhang, and cross-link candidates
    # came back EMPTY because the one useful neighbor (member 1015, a
    # 93mm column) sits 17.2mm away -- just outside the 15mm base cap --
    # while everything genuinely closer fails the rise gate (a short
    # shoot whose real z-extent doesn't reach the climb). MEASURED FIX:
    # widening the cross-link search to 18mm let ONE cross-link to 1015
    # place cleanly (clearance + printability both pass, X-pair
    # r~1.155mm) and r_transient dropped 2.9947 -> 2.1663, back under the
    # ceiling. 1.5x (the default 15mm base -> 22.5mm) is deliberately
    # generous ABOVE the single measured data point (18mm, ~1.2x) --
    # bracing.py tries an intermediate 1.25x (18.75mm) tier FIRST, only
    # reaching this hard ceiling on a still-emptier search; see
    # bracing._reach_tiers_mm.
    # This widening is STRICTLY GATED (bracing.py's own docstring/tests):
    # it only ever activates for a request whose activation trigger
    # genuinely fired AND whose standard-cap search came back with ZERO
    # candidates at that height -- never a density change anywhere the
    # unwidened search already produces candidates.
    brace_reach_max_factor_x: float = 1.5
    # Vertical spacing between rungs of a brace ladder along a trunk pair.
    brace_ladder_z_mm: float = 8.0
    brace_r_mm: float = 0.5

    # ---- Arterial trunks (v3: shared columns + diagonal shoots) ----
    # Max lateral reach of a diagonal shoot from a contact's stub end to
    # its trunk (the shoot descends at max_segment_angle_deg, so its
    # vertical drop matches this at 45 degrees).
    shoot_max_xy_mm: float = 9.0
    # Cap contacts sharing one trunk -- keeps single-point-of-failure and
    # removal effort bounded ("share arterial trunks with a maximum
    # number of shoots").
    trunk_max_shoots: int = 10
    # Spiral search offsets tried when placing a trunk column so it
    # clears the model (replaces v2's after-the-fact detour repair).
    # Radii beyond shoot_max_xy_mm apply only when FOUNDING a trunk (the
    # join cap is an economy limit; an unclear column is worse than one
    # long founding diagonal).
    trunk_search_radii_mm: tuple = (0.0, 2.0, 4.0, 7.0, 10.0, 14.0, 19.0, 25.0)
    trunk_search_directions: int = 12
    # Trunk radius bounds: axial (peel tension, accumulated tip loads) and
    # slenderness sizing run through the same formulas as v2 shafts, then
    # clamp to this removal-friendly band.
    min_trunk_r_mm: float = 0.8
    max_trunk_r_mm: float = 2.4
    # Thin diagonal shoots: sized by their own load + length, floored here.
    min_shoot_r_mm: float = 0.35

    # ---- Build-time stability (v3.1, from second review) ----
    # Max unbraced span a trunk may run during the BUILD (from the raft,
    # or from the last lateral restraint -- a brace or a shoot junction --
    # up to the next). Until a column merges with something it is a
    # cantilever rooted at the raft, flexing every peel cycle; the
    # measured example (65mm bare run at r=1mm) deflects ~2.3mm under
    # even the configured 0.01N lateral load, 15x the budget. Spans
    # longer than this must be braced; if no neighbor is in reach, the
    # trunk is THICKENED for its actual worst span instead (the old
    # sizing capped radius at max_unbraced_shaft_r_mm assuming braces
    # would exist -- nothing reconciled the debt when they couldn't be
    # placed, which is also why every trunk looked identical).
    brace_free_span_mm: float = 18.0
    # Merge trunks closer than this in xy (parallel twin/triplet columns
    # a few mm apart serve no one -- resin cost, no stiffness gain).
    trunk_merge_radius_mm: float = 3.0
    # v3.6 (queue item 3): twins at or under this spacing that STILL
    # can't merge (even via the midpoint retry) get single alternating
    # brace diagonals instead of crossed X pairs -- between near-parallel
    # columns millimetres apart the second rod of each X adds no
    # stiffness, only lattice clutter. Single rods are sized WITHOUT the
    # X-pair load-sharing discount.
    twin_single_brace_xy_mm: float = 3.0
    # Trunk-to-MODEL brace rungs (like Chitubox's own support-to-model
    # mini braces): when no neighbor trunk is in reach, a short rod to
    # already-printed model geometry restrains the span instead. Both
    # ends land on structure that exists at that layer, so this is a
    # two-end-supported short bridge (same class as the validated raft
    # bridges), not a floating horizontal run -- kept short.
    model_brace_max_len_mm: float = 5.0
    # Contacts whose column/spiral search found nothing clear fall back
    # to a strut anchored ON the model below (adsorbed foot, like the
    # corpus's own 227 model-adsorbed supports) -- steeper lean allowed
    # than trunk members (still layer-continuous), since the strut is
    # rooted on already-printed model, not dangling from the raft.
    # v3.6: raised 55 -> 60 with the rest of the branch-angle family;
    # struts are sized with their ACTUAL lean via the angle-aware term.
    strut_fallback_max_angle_deg: float = 60.0

    # ---- Widened branch angle (v3.6, queue item 2, Craig) ----
    # Y-diagonals, hub fans, risers, and strut members may lean up to
    # this angle from vertical -- steeper than the 45-degree default that
    # governs plain trunk shoots (45 is length-optimal there and stays).
    # The extra reach widens what the coalesce/bundle passes and the
    # strut anchor cone can serve (attacks the unrouted canopy
    # contacts). REQUIRED PAIRING: any member allowed past 45 degrees
    # must be sized with the angle-aware load term in
    # placement._shaft_r_for_L (F_eff = F_drag + peel_pressure *
    # ellipse_area(theta) * sin(theta)) so steep members are honestly
    # fattened -- the peel load on an inclined member's elliptical layer
    # slice resolves a lateral bending component that vertical members
    # don't see.
    branch_angle_deg: float = 60.0

    # ---- Y-branch column coalescing (v3.5, sixth review) ----
    # Near-parallel plate columns whose feet land this close in xy get
    # merged into a tree: the shorter keeps only its upper run, sprouting
    # from the survivor at a 45-degree Y-split. Saves the child's whole
    # lower column + pad for the cost of one short diagonal; also fewer
    # columns = easier removal. Each merge passes a per-merge resin gate.
    trunk_branch_merge_xy_mm: float = 10.0

    # ---- Joint/foot/tip geometry (v3, from in-Chitubox review) ----
    # Ball at every articulation, sized to MATCH the fatter member it
    # joins (covers the lens-shaped seam of an angled frustum joint
    # without lumping past the silhouette -- review: 1.25x read as
    # "lumpy").
    joint_ball_factor: float = 1.0
    # Spherical contact at the tip (easier removal, smaller scar than a
    # cylinder driven into the surface), relative to tip radius.
    tip_ball_r_factor: float = 1.5
    # Conical flare where a trunk meets its pad (adhesion + peel moment).
    flare_len_mm: float = 1.5
    flare_r_factor: float = 1.6

    # ---- Model-to-model struts (design.md #6) ----
    # A strut to nearby model surface is only considered if it is at least
    # this much shorter than the plate-anchored alternative (routing to a
    # nearby wall that saves little material isn't worth a second scar).
    strut_min_length_saved_mm: float = 8.0
    # Cap how far a candidate strut anchor may be from its contact.
    # Without this, "nearest safe point anywhere on the model" on a tall
    # model with a distant plate will almost always beat the plate on raw
    # length, picking anchors clear across the model that a straight
    # strut path is likely to clip OTHER geometry reaching for (measured:
    # uncapped, 307/353 contacts on the Weeping Treant chose model
    # anchors, most producing clearance violations). design.md's own
    # framing ("sometimes the model is a better base than the plate") is
    # about local bracing, not long-reach struts -- keep it local.
    strut_max_length_mm: float = 20.0
    # Relative cost weight of one additional scar (surface mark) in the
    # struts vs plate cost comparison; see routing.strut_cost().
    scar_cost_weight: float = 1.0
    # Cost per mm of shaft material (round member, load-independent
    # heuristic: longer/thinner = cheaper to cast off, but slenderness
    # already gates thinness elsewhere -- this weight trades length against
    # scar count, not physics).
    material_cost_per_mm: float = 0.05

    # ---- Scar cost / delicacy field (design.md #7) ----
    # Delicacy score blends two geometric proxies for "would decimation's
    # error map protect this vertex": local curvature (high curvature =
    # detail) and local thinness (small local feature size = fragile). See
    # perception.delicacy_field() for the full formula. Weight of each:
    delicacy_curvature_weight: float = 0.6
    delicacy_thinness_weight: float = 0.4
    # Thickness at/below this counts as maximally thin (weight saturates).
    thin_feature_mm: float = 1.5

    # ---- Local z-minima / layer-island birth support (v3.6, Craig:
    # TOP PRIORITY correctness) ----
    # Every local minimum of the surface height function births a new
    # cross-section island: its first layers cure as a disconnected patch
    # stuck to the FEP (glob/tear risk) until the growing island either
    # merges with earlier-printed material or meets a support tip. The
    # overhang-angle island model MISSES minima whose flanks are steeper
    # than the down-facing test, and spacing-based coverage allows the
    # nearest contact millimetres away laterally -- useless for a layer
    # island (Craig: proximity is the wrong metric; what matters is how
    # many LAYERS pass before the island connects to anything).
    # Print layer height for this printer profile (Saturn 4 Ultra @ 50um).
    layer_height_mm: float = 0.05
    # A minimum is exempt only if its birth island connects to strictly
    # lower (earlier-printed, inductively-supported) material within this
    # many layers of its birth; otherwise it must have a routed contact
    # touching the birth region itself.
    minima_connect_layers: float = 2.0
    # Tie tolerance for flat plateaus (vertices within this of each other
    # count as one minimum plateau, one representative). Sub-layer scale.
    minima_eps_mm: float = 0.02
    # Discretization slack when testing whether a contact touches a birth
    # region (contact points are face samples, regions are vertex sets).
    # This is NOT a coverage radius -- the region itself encodes the
    # 1-2-layer connection rule; this only absorbs sampling gaps.
    minima_contact_radius_mm: float = 1.0
    # Supportive-connectivity filter on local_z_minima (Craig's ruling,
    # 2026-07-24). local_z_minima's candidate test and flood are both PURE
    # SURFACE tests -- they never ask whether solid model material already
    # carries the point, so a dip on an upward/side-facing surface (chest
    # crease, armpit fold, dorsal saddle) was treated as an unsupported
    # island birth and got a strut into solid bulk. Craig: "'Directly
    # below' should include voxels below and to the side if the voxel above
    # (and side-by-side with the candidate) is connected. The voxel doesn't
    # need to be *directly* below the candidate, it just needs to be
    # supportive." Per-layer connected components ARE that predicate, and
    # "component with no overlap of the layer below" is exactly
    # perception.island_births -- so with this ON a minimum survives only
    # if it coincides with a real slice-based island birth. Default ON (the
    # surface-only behaviour is a measured defect: 38.1% of treant and
    # 21.7% of morel minima-stage contacts were false).
    #
    # DEFAULT ON as of the ADEQUACY refinement (Craig's ruling, 2026-07-24).
    # It was held back for one round because the original filter tested the
    # EXISTENCE of solid beneath a candidate, not its ADEQUACY, and the
    # owner's rule is that the material must be *supportive*: per-site
    # inspection of the 80 exterior down-facing treant minima it removed
    # found 31 standing on less than 1 mm2 or less than 0.05mm inside the
    # boundary -- the worst on 0.0076 mm2, far less than a support tip's own
    # footprint. That is a whisker, not support. The refined filter drops a
    # minimum only if it is BOTH not-an-island-birth AND adequately carried
    # (see minima_support_area_tips below), so those sites are now kept and
    # the filter is safe to enable. A2 exposure for comparison: filter A
    # removes 2 exterior down-facing contacts on the treant and 0 on the
    # morel; the unrefined version of this one removed 77.
    minima_require_island_birth: bool = True
    # FILTER (A), independent of the above (Craig's adjudication,
    # 2026-07-24: the two are DIFFERENT defects and must not be collapsed
    # into one mechanism, so a win in one cannot launder a loss in the
    # other). The model is a SINGLE HOLLOW SHELL -- not an assembly of
    # overlapping sub-bodies -- so its interior faces are the INNER WALL
    # surface. A minimum whose nearest face is interior is a phantom:
    # nothing is ever printed on the inside of the wall, so supporting it
    # is meaningless, whatever is or is not below it. Requires a
    # surface-EXACT oracle (engine.run always supplies one); fails open
    # without it.
    minima_drop_interior_faces: bool = True
    # Z half of that coincidence test, in real print layers (design-v4.md
    # section 11: a print-timing tolerance is expressed as a multiple of
    # layer_height_mm, never a raw mm). The birth plane sits at the first
    # slice ABOVE the minimum's tip vertex, so the offset is inherently
    # sub-layer; measured max on the morel/treant corpus is ~1.1 layers.
    minima_birth_match_layers: float = 2.0
    # ADEQUACY threshold for filter (B), in multiples of a support tip's own
    # footprint (pi * tip_r_min_mm**2 = 0.1257 mm2 at the 0.2mm tip floor).
    # The ruling is that material below a candidate must be SUPPORTIVE, not
    # merely present: a candidate resting on less cross-section than the tip
    # that would land on it is resting on a whisker. Chosen from the measured
    # distribution, not by taste -- treant supporting areas are bimodal
    # (median 5.24 mm2 vs sliver cases of 0.008-0.097 mm2) and the tip
    # footprint sits in the empty gap; the rescued-site count moves by only 5
    # across the whole 1x-8x range, so this sits in a flat region rather than
    # on an edge. See perception._minima_support_adequate.
    # Setting this to 0.0 reproduces the UNREFINED filter (B) exactly (every
    # non-birth is dropped): adequacy is decided by the single comparison
    # `carried_area >= threshold` with no special cases, and 0.0 >= 0.0 is
    # true for every candidate including ones whose supporting section is
    # missing or unmeasurable. That equivalence is pinned by tests.
    minima_support_area_tips: float = 1.0

    # ---- Rewiring / topology mutation (design-v4.md #7, Phase 3) ----
    # Wall-clock budget for rewire.optimize's sweep loop. Craig's directive
    # (design-v4.md #7): "diminishing returns, not iteration counts" -- the
    # rolling-savings convergence check is the PRIMARY stop condition; this
    # cap is the backstop so a pathological fixture can't blow the overall
    # 30-60s runtime budget (design-v4.md #12). 30.0s matches the design
    # doc's own stated default for the rewiring stage.
    rewire_time_cap_s: float = 30.0

    # ---- Removal-leverage gate (design-v4.md #10, Craig's removal model) ----
    # HANDLE: a contiguous member run at/above this radius is thick enough
    # to grip by hand.
    grip_handle_r_mm: float = 1.0
    # HANDLE: the qualifying thick run must span at least this much
    # contiguous length to count as a graspable handle.
    grip_handle_len_mm: float = 10.0
    # HANDLE: radial free space required around the handle run's axis,
    # against BOTH the model and every other tree, before it counts as
    # actually gripable (a thick-enough run wedged against the model or a
    # neighbor is not a real handle).
    grip_clear_mm: float = 5.0
    # PEEL: distance a tree must translate clear along some outward-biased
    # pull direction, without entering the model's standoff envelope,
    # before it counts as removable. Translation is the conservative proxy
    # for the real rotational peel motion (design-v4.md #10, #15).
    pull_dist_mm: float = 15.0
    # All four above are flagged rough-estimate-pending-calibration
    # (design-v4.md #10): "the calibrator is Craig's hands on the first
    # v4 print."

    # ---- Self-verify (design.md #9) ----
    # An island counts as "still unsupported" if any of its faces sits
    # farther than this multiple of target_spacing from the nearest
    # contact assigned to it -- looser than target_spacing so verify
    # doesn't nag about ordinary in-spec gaps, but still catches real
    # coverage holes (the case v0's "fix pass" existed to patch).
    coverage_radius_factor: float = 1.5

    # ---- SPEC-V5-PERF: construct-stage performance ----
    # CLASS A (exact-preserving, always on). Debug kill-switch for the
    # exactness-control gate (sec 2.5 gate 4) and the cache fuzz suite ONLY:
    # when True, Forest._revalidate_exact bypasses the geometry-keyed verdict
    # cache (A1) and recomputes every kernel query, so a run with it True vs
    # False must be BIT-IDENTICAL. Default False (cache ON). Not a tuning knob.
    perf_exact_opts_disable: bool = False

    # CLASS B approximation levers (SPEC-V5-PERF sec 3). Each defaults OFF
    # (None) -- a lever trades solution QUALITY (resin ml / support count),
    # never correctness, and is enabled only after its sec-5.3 measurement row.
    # B1: candidate beam pruning -- after A2's pricing pass, trial only the k
    #     cheapest candidates for clearance; if all k fail, honestly unroute.
    perf_beam_k: int = None
    # B2: model-anchor candidate cap -- truncate the ~40-candidate anchor list
    #     to the nearest `cap` before ray-exactification/builder creation.
    perf_anchor_cap: int = None
    # B3: coarser demand seeding -- multiply placement sampling target spacing
    #     by f > 1 in prepare (SEEDING ONLY; the coverage gate's
    #     target_spacing_mm is untouched, so coverage itself never loosens).
    perf_seed_spacing_factor: float = None
    # B4: convergence caps -- (engine_fix_passes, brace_internal_iters). Can
    #     leave residual uncovered islands, so it is gated by a mechanical
    #     emission gate on verify.ok (perf_gate_emit_on_verify).
    perf_fix_iter_caps: tuple = None
    # B4 companion: refuse to treat a run as emittable when verify.ok is False
    #     (required for B4 acceptance; harmless/off by default).
    perf_gate_emit_on_verify: bool = False
    # B5: deterministic rewire budget -- replace rewire's wall-clock cap with a
    #     move-evaluation (checkpoint/build/revalidate/rollback) budget; the
    #     precondition for any scale-level bit-compare that includes rewire.
    perf_rewire_work_budget: int = None

    # ---- Phase-8 relocation (SPEC-P8-RELOCATE.md §8, DRAFT+AMENDED) ----
    # All deterministic; no RNG anywhere. Every ordering in Phase-8 is a stable
    # sort on an explicit key with the candidate index as the final tiebreak.
    # The moot floor PRINT_FLOOR_CHORD_MM = 0.08 lives with the SCAR-AREA-CAP
    # normative constants in verify.py (literal-identical to the census); it is
    # not re-declared here to keep one owner for that ratified number.
    #
    # SM-STUB / SM-STRUT R1 re-aim candidate grid (§3.1): a fixed cone of
    # directions around the contact inward normal x a shortening-length ladder.
    # polar 0 recovers the original contact normal and is emitted once; the four
    # polar rings x eight azimuths give 25 directions, x three length factors =
    # <=75 candidates/site. Directions outside the printability lean limit are
    # SKIPPED, never clamped (§3.1, Open-risk #1).
    p8_reaim_polar_deg: tuple = (0.0, 10.0, 20.0, 30.0)
    p8_reaim_azimuth_deg: tuple = (0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0)
    p8_reaim_length_factors: tuple = (1.0, 0.75, 0.5)

    # Pocket detection / conflict-graph solver (§4). World-origin-pinned voxel
    # grid (NOT bbox-relative -- amendment F: pocket identity must be stable
    # under sub-voxel re-aim). A pocket is a 6-connected voxel component with
    # >= p8_pocket_min_members defect tips; corridor pad doubles as the
    # detection halo and the density-penalty radius.
    p8_pocket_voxel_mm: float = 6.0
    p8_pocket_min_members: int = 8
    p8_pocket_corridor_pad_mm: float = 0.5

    # SM-BALL relocation (§3.2): 1-D deterministic minimal-slide search along the
    # member axis, root-ward then tip-ward, taking the first clean position.
    p8_ball_slide_step_mm: float = 0.25
    p8_ball_slide_max_mm: float = 2.0

    # Fix-round F1 convergence: the relocation ladder repeats its Phase 1->2->3
    # sweep -- re-deriving the frozen defect snapshot from the mutated forest --
    # until a sweep commits zero splices (the true no-op fixed point) or the defect
    # population stops strictly decreasing, bounded by this cap. Converts the
    # previously-discarded first pass into cumulative progress and makes the §6.10
    # idempotence gate real (a subsequent run over the fixed point is a no-op).
    p8_max_relocate_sweeps: int = 8

    # OUTER FIXED-POINT ROUND CAP (residual round 2, 2026-07-24). A sweep loop
    # reaches only a LOCAL fixed point: its terminating sweep carries that
    # round's retirement set / progress ledger / boundary carry, all judgements
    # made against geometry later commits have since moved. `run_phase8`
    # therefore re-arms every per-call memory and re-enters, until a complete
    # FRESH-STATE round commits zero -- which is exactly what a fresh invocation
    # over the same forest would do, so the §6.10 strict no-op becomes idempotent
    # by construction. Termination rests on the D6 invariant (every commit
    # strictly decreases the global defect count), never on this cap; the cap is
    # a backstop and hitting it is reported honestly as converged=False.
    p8_max_relocate_rounds: int = 4

    # Amendment-H per-candidate resin / quality guards -- part of R1/S1 LEGALITY,
    # not merely acceptance levers: a candidate that violates one is illegal at
    # selection, never merely penalized (§3.1, §8).
    p8_reaim_volume_delta_max_ml: float = 0.02      # R1 per-relocation volume-delta cap
    p8_reaim_route_growth_max_factor: float = 1.50  # R1 downstream-route growth cap
    p8_strut_growth_max_factor: float = 1.25        # S1 no-unbounded-strut-growth cap

    # SM-STUB R2 -- SURFACE-SEED CONTACT RE-SEAT (§3.1 R2; orchestrator ruling (5),
    # 2026-07-24). Only entered when the R1 legal set is EMPTY: the demanded
    # contact is moved to an alternative MODEL-SURFACE seed inside its inviolable
    # class domain (REACH §1: same island), candidates ordered nearest-to-demanded
    # under the REACH §2b economy cap, each re-aimed at the seed's own outward
    # normal x the shortening ladder and judged by the SAME shared certificate +
    # zero-new-violation commit invariant as R1. `p8_reseat_radius_mm` bounds the
    # demand displacement (the contact is a DEMAND -- it may migrate, never
    # teleport); the coverage/birth guard (REACH §2b "neighbor-birth guard before
    # commit") is unconditional and not a config lever.
    p8_reseat_enable: bool = True
    p8_reseat_radius_mm: float = 2.0                # max demand displacement (REACH §1 domain)
    p8_reseat_max_seeds: int = 24                   # REACH §2b `reloc_max_candidates`

    # D5 -- PARENT-JUNCTION RE-SEAT (orchestrator ruling (5), 2026-07-24). The last
    # rung before honest failure: when neither re-aim (R1/S1) nor contact re-seat
    # (R2) can make a site legal, the site's PARENT JUNCTION node is re-seated
    # through the guarded co-moving splice (`_translate_joint` + the D6 invariant,
    # the same transaction SM-BALL B1 commits through). Candidates are the same
    # surface seeds when the junction sits ON the model surface (a strut/rung
    # foot), else a deterministic free-space lattice about the node; ordered
    # nearest-first, so the first legal one IS the lexicographic minimum.
    p8_junction_reseat_enable: bool = True
    p8_junction_reseat_step_mm: float = 0.25
    p8_junction_reseat_max_mm: float = 1.0

    # Acceptance levers (§6).
    p8_resin_delta_max: float = 0.03                # §6.6 aggregate resin backstop
    p8_max_unresolved: int = 10                     # §6.7 honest-failure count bound
    p8_max_unresolved_weight: float = 3.0           # §6.7 criticality-weighted bound
    p8_chord_canon_micron: int = 1                  # §6.3 integer-micron chord canon

    # Phase-8 RELOCATION STAGE (phase8_driver.run_phase8, wired into engine.run
    # post-brace / pre-final-verify). Default OFF -- like every other perf/quality
    # lever on this branch (perf_gate_emit_on_verify etc.), it is opt-in so the
    # frozen bit-identity/determinism baselines are untouched: with it OFF the
    # engine emits byte-identical geometry to HEAD. The full-scale acceptance run
    # turns it ON. When ON but a forest has zero defects the stage is a no-op
    # (idempotent by construction), so enabling it on already-clean geometry also
    # leaves the digest unchanged.
    p8_relocate_enable: bool = False

    def notes(self):
        """Flags for the caller: which constants are physics-derived vs
        rough estimates pending calibration. Surface this in run logs so a
        human calibrating against real prints knows what to touch."""
        return {
            "measured_or_charter": [
                "tensile_green_mpa", "modulus_green_gpa", "tip_safety_factor",
                "tip_r_min_mm", "tip_r_max_mm", "target_spacing_mm",
                "spacing_floor_mm", "standoff_mm",
            ],
            "rough_estimate_pending_calibration": [
                "peel_pressure_pa", "shaft_drag_force_n", "max_deflection_mm",
                "grip_handle_r_mm", "grip_handle_len_mm", "grip_clear_mm",
                "pull_dist_mm",
            ],
        }

    def as_dict(self):
        return asdict(self)
