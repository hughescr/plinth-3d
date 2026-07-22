"""Global joint-ball pass (v3.6, Craig's design -- queue item 1).

Replaces the scattered per-call ball sizing that lived inside
synth_v3_support / synth_v3_trunk / brace_record. Those embedded balls
were sized at EMISSION time from whatever radius the local call happened
to know, which created a whole bug class (gapped undersized balls when a
later pass fattened a member; doubled spheres where two members shared an
endpoint and each brought its own ball).

This pass runs after ALL geometry and radii are final:

  1. collect every member (stubs, shoots, trunk columns, Y-diagonals,
     risers, braces, rungs) with its final endpoint radii;
  2. cluster joint points: endpoint coincidence within a small tolerance,
     PLUS on-axis proximity for balls landing mid-member (a shoot
     junction on a trunk column, a rung end on a sibling shoot);
  3. emit exactly ONE ball per joint, sized to the FATTEST member
     touching it (joint_ball_factor * max radius at that point).

Balls are assigned an OWNER record (the fattest touching member's
entity) so emission can append each sphere into an existing record's
mesh blob -- record count and Chitubox-side grouping stay identical to
the ball-free structure.

Deliberate conventions preserved from the per-member era:
  - the tip CONTACT ball is not a joint (it is the removal-friendly
    point contact, still emitted by synth_v3_support);
  - a model-embedded end (strut anchor foot) keeps its 1.4x adhesion
    factor -- there is no second member to size against, the ball is
    the adhesion fillet against the model surface.
"""
import numpy as np

_MISSING = object()

MODEL_FOOT_FACTOR = 1.4     # adsorbed strut foot ball vs its rod radius
COINCIDE_TOL_MM = 0.15      # endpoint clustering: true coincidences only
ON_AXIS_TOL_MM = 0.2        # node-to-member-axis distance for sizing
PLATE_BAND_MM = 0.2         # same plate-band rule verify.connectivity_report
                             # rule (b) uses -- a node this close to plate_z
                             # is plate-rooted, owned by the foot flare/pad,
                             # not a joint (F1: no balls sunk into the plate)
COLINEAR_ANGLE_DEG = 10.0   # F3(a): two-member pass-through anti-parallel
                             # tolerance (straight continuation, no ball)
COLINEAR_RADIUS_FRAC = 0.20  # F3(a): end-radius match tolerance for the
                              # same pass-through rule
_COLINEAR_COS_MAX = float(np.cos(np.radians(180.0 - COLINEAR_ANGLE_DEG)))


def _point_segment_dist(points, p0, p1):
    """Distances from (N,3) points to one segment p0->p1."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-18:
        return np.linalg.norm(points - p0, axis=1)
    t = np.clip((points - p0) @ d / L2, 0.0, 1.0)
    proj = p0[None, :] + t[:, None] * d[None, :]
    return np.linalg.norm(points - proj, axis=1)


def _dist_and_local_radius(center, p0, p1, r0, r1):
    """Distance from `center` to segment p0->p1, plus the segment's OWN
    radius at the nearest point (linear taper r0->r1 evaluated at the
    same clamped t used for the distance) -- F3(c): the radius a joint
    ball should be sized against is the member's radius AT THE POINT the
    joint actually lands on, not blindly max(r0, r1) (which lets an
    irrelevant far taper end dominate a joint sitting at the near end)."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-18:
        return float(np.linalg.norm(center - p0)), float(r0)
    t = float(np.clip((center - p0) @ d / L2, 0.0, 1.0))
    proj = p0 + t * d
    dist = float(np.linalg.norm(center - proj))
    local_r = float(r0) + (float(r1) - float(r0)) * t
    return dist, local_r


def collect_members(members):
    """v4 (design-v4.md #2): the CCO builder (cco.build) produces the
    member list NATIVELY (tree.Member objects, or plain dicts with the
    same schema) instead of v3's four separate routes/trunks/risers/
    braces collections. This function inverts from a from-parts
    CONSTRUCTOR to a validation/identity layer: assert the schema, and
    normalize every member (Member object or dict) to the dict schema
    {p0, p1, r0, r1, owner, kind, embed_end} that compute_joint_balls,
    verify.connectivity_report, and emission consume.

    embed_end replaces v3's kind/sentinel logic (j == -1 model rung,
    i == j == -3 thinning rung) for embed-eligibility; a plain dict
    fixture that omits it is treated as None (no designed embed), same
    as a freshly-built tree.Member's default.
    """
    out = []
    for m in members:
        p0 = np.asarray(_field(m, "p0"), float)
        p1 = np.asarray(_field(m, "p1"), float)
        r0 = float(_field(m, "r0"))
        r1 = float(_field(m, "r1"))
        kind = _field(m, "kind")
        owner = _field(m, "owner")
        assert p0.shape == (3,) and p1.shape == (3,), f"member p0/p1 must be 3-vectors: {m}"
        assert isinstance(kind, str), f"member kind must be a str: {m}"
        assert isinstance(owner, tuple) and len(owner) == 2, f"member owner must be a 2-tuple: {m}"
        embed_end = _field(m, "embed_end", default=None)
        assert embed_end in ("p0", "p1", None), f"member embed_end must be p0/p1/None: {m}"
        out.append(dict(p0=p0, p1=p1, r0=r0, r1=r1, owner=owner, kind=kind,
                         embed_end=embed_end))
    return out


def _field(m, name, default=_MISSING):
    if isinstance(m, dict):
        if name in m:
            return m[name]
        if default is not _MISSING:
            return default
        raise KeyError(f"member dict missing required field {name!r}: {m}")
    if hasattr(m, name):
        return getattr(m, name)
    if default is not _MISSING:
        return default
    raise AttributeError(f"member object missing required field {name!r}: {m}")


def compute_joint_balls(members, plate_z, cfg):
    """One ball per joint, sized to the fattest member touching it.
    Returns list of dicts {center (3,), r, owner} with owner as in
    collect_members. See module docstring for the rules.

    v4: joint-ness is now determined structurally instead of by a
    per-kind branch table -- every member contributes a node at p1, and
    at p0 UNLESS its kind is "stub" (a stub's p0 is the tip CONTACT, the
    removal-friendly point contact, never a joint -- same rule v3 had,
    now expressed once instead of per-kind). An endpoint is the
    model-adsorbed-foot case (MODEL_FOOT_FACTOR) iff it is that member's
    embed_end AND it isn't the stub-tip case already excluded above --
    replaces v3's kind-specific "strut"/"model_rung" embedded flags.

    F1 (Craig's Chitubox review): a node within PLATE_BAND_MM of plate_z
    is plate-rooted -- excluded here, same plate-band rule
    verify.connectivity_report rule (b) uses. The foot flare + pad
    (chitubox_emit's per-member plate_z handling) own that region
    instead; a ball there would sink half into the build plate.

    F3(a): a cluster of exactly two endpoints whose members are
    anti-parallel through the point (within COLINEAR_ANGLE_DEG) AND
    whose end radii match within COLINEAR_RADIUS_FRAC is a straight
    continuation (a bifurcation split or rewire seam, not a real
    junction) -- v3 merged these cosmetically; no ball."""
    members = collect_members(members)
    if not members:
        return []

    # --- joint NODES: member endpoints that are articulation points.
    # outward_dir points AWAY from the node, along the member, toward its
    # other end -- used by F3(a)'s colinear-pass-through test.
    nodes = []  # (point, radius_at_end, owner, embedded_model, outward_dir)
    for m in members:
        d = m["p1"] - m["p0"]
        L = float(np.linalg.norm(d))
        d_unit = d / L if L > 1e-9 else np.zeros(3)
        if m["kind"] != "stub":
            if abs(float(m["p0"][2]) - plate_z) > PLATE_BAND_MM:
                nodes.append((m["p0"], m["r0"], m["owner"], m["embed_end"] == "p0", d_unit))
        if abs(float(m["p1"][2]) - plate_z) > PLATE_BAND_MM:
            nodes.append((m["p1"], m["r1"], m["owner"], m["embed_end"] == "p1", -d_unit))

    if not nodes:
        return []
    pts = np.array([n[0] for n in nodes])

    # --- cluster coincident endpoints (doubled X-brace ends, fan hubs)
    from scipy.spatial import cKDTree
    tree = cKDTree(pts)
    pairs = tree.query_pairs(r=COINCIDE_TOL_MM, output_type="ndarray")
    parent = np.arange(len(nodes))

    def find(a):
        while parent[a] != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    for a, b in pairs:
        ra, rb = find(int(a)), find(int(b))
        if ra != rb:
            parent[rb] = ra
    clusters = {}
    for i in range(len(nodes)):
        clusters.setdefault(find(i), []).append(i)

    # --- axis members for mid-member landings (sized by what the ball
    # actually sits on, not just who ends there). v4: a bifurcation SPLITS
    # its host member at the junction, so "mid-member" landings are now
    # the exception rather than v3's routine case (a rung/riser floating
    # onto an unsplit column's surface) -- kept as a safety net for any
    # near-coincident-but-not-exactly-split geometry. Every non-stub
    # member is eligible (stubs are excluded: their p0 is the tip contact
    # domain, not structural axis a joint could land on).
    axis_members = [m for m in members if m["kind"] != "stub"]

    balls = []
    for idxs in clusters.values():
        cluster_nodes = [nodes[i] for i in idxs]
        if len(cluster_nodes) == 2:
            # F3(a): exactly two endpoints, anti-parallel through the point
            # and near-equal radii -- a straight continuation (bifurcation
            # split / rewire seam), not a real junction. No ball.
            (_, ra, _, _, dir_a), (_, rb, _, _, dir_b) = cluster_nodes
            cosang = float(np.dot(dir_a, dir_b))
            r_hi = max(ra, rb)
            radii_close = r_hi <= 1e-12 or (r_hi - min(ra, rb)) <= COLINEAR_RADIUS_FRAC * r_hi
            if cosang <= _COLINEAR_COS_MAX and radii_close:
                continue
        r_end = max(n[1] for n in cluster_nodes)
        center = cluster_nodes[int(np.argmax([n[1] for n in cluster_nodes]))][0]
        embedded = any(n[3] for n in cluster_nodes)
        owner = None
        r_max = r_end
        # fattest member whose axis passes through this joint. F3(c): size
        # by the radius AT THE CLUSTER POINT (linear taper evaluated at the
        # nearest point on the member's own axis), not the member's overall
        # max end radius -- a tapered member's FAR end (irrelevant to this
        # joint) must not dominate a ball sized for its near end.
        for m in axis_members:
            dist, local_r = _dist_and_local_radius(center, m["p0"], m["p1"], m["r0"], m["r1"])
            if dist <= ON_AXIS_TOL_MM and local_r >= r_max:
                r_max = local_r
                owner = m["owner"]
        if owner is None:
            # no fatter through-member: owned by the fattest endpoint
            owner = cluster_nodes[int(np.argmax([n[1] for n in cluster_nodes]))][2]
        r_ball = cfg.joint_ball_factor * r_max
        if embedded and len(cluster_nodes) == 1:
            # adsorbed model foot: adhesion fillet, no second member
            r_ball = MODEL_FOOT_FACTOR * r_end
        # F1 addendum (verifier should-fix): the node-band exclusion above
        # only tests the NODE z against PLATE_BAND_MM, BEFORE the ball's
        # radius is known -- a node comfortably outside the 0.2mm band can
        # still size up into a sphere whose BOTTOM crosses the plate
        # (real-data specimen: node 0.806mm above plate_z, r=0.8268,
        # sphere bottom 0.0211mm below plate_z). Now that r_ball is final,
        # judge the actual sphere geometry: center_z - r < plate_z means
        # the sphere would poke through the build plate.
        #
        # Design decision: DROP the ball rather than clamp its radius or
        # lift its center. These near-plate balls sit on structure whose
        # reinforcement the foot flare/pad already provides -- the same
        # F1 rationale as the node-band exclusion just above -- so a
        # dropped ball loses no real support. Clamping would silently
        # shrink or reposition a ball sized against a real member's
        # radius and make the emitted geometry lie about what was
        # actually decided; a missing entry is honest and cheap to audit.
        if center[2] - r_ball < plate_z:
            continue
        balls.append(dict(center=np.asarray(center, float), r=float(r_ball),
                          owner=owner))
    return balls


def balls_by_owner(balls):
    """Group compute_joint_balls output for the emitter:
    {("trunk", i): [(center, r), ...], ...}"""
    out = {}
    for b in balls:
        out.setdefault(b["owner"], []).append((b["center"], b["r"]))
    return out
