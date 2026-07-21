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
    # A down-facing face within this height of the model's own lowest
    # point is exempt -- close enough to plate/raft that no dedicated
    # pillar is needed.
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

    # ---- Self-verify (design.md #9) ----
    # An island counts as "still unsupported" if any of its faces sits
    # farther than this multiple of target_spacing from the nearest
    # contact assigned to it -- looser than target_spacing so verify
    # doesn't nag about ordinary in-spec gaps, but still catches real
    # coverage holes (the case v0's "fix pass" existed to patch).
    coverage_radius_factor: float = 1.5

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
            ],
        }

    def as_dict(self):
        return asdict(self)
