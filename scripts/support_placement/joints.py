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

MODEL_FOOT_FACTOR = 1.4     # adsorbed strut foot ball vs its rod radius
COINCIDE_TOL_MM = 0.15      # endpoint clustering: true coincidences only
ON_AXIS_TOL_MM = 0.2        # node-to-member-axis distance for sizing


def _point_segment_dist(points, p0, p1):
    """Distances from (N,3) points to one segment p0->p1."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-18:
        return np.linalg.norm(points - p0, axis=1)
    t = np.clip((points - p0) @ d / L2, 0.0, 1.0)
    proj = p0[None, :] + t[:, None] * d[None, :]
    return np.linalg.norm(points - proj, axis=1)


def collect_members(routes, trunks, risers, braces, plate_z):
    """Every final member as dicts {p0, p1, r0, r1, owner, kind}.
    owner is ("route", i) | ("trunk", i) | ("riser", i) | ("brace", i).
    kind is the semantic role; joint NODES are derived from these by
    compute_joint_balls (which knows which endpoints are joints)."""
    members = []
    for ti, t in enumerate(trunks):
        base = t.base_z(plate_z)
        members.append(dict(
            p0=np.array([t.xy[0], t.xy[1], base]),
            p1=np.array([t.xy[0], t.xy[1], t.z_top]),
            r0=t.r, r1=t.r, owner=("trunk", ti), kind="column"))
        if t.foot_kind == "branch" and t.parent_idx is not None:
            parent = trunks[t.parent_idx]
            members.append(dict(
                p0=np.array([parent.xy[0], parent.xy[1], t.z_split]),
                p1=np.array([t.xy[0], t.xy[1], t.z_start]),
                r0=t.r, r1=t.r, owner=("trunk", ti), kind="y_diagonal"))
    for ri, rt in enumerate(routes):
        for seg in rt.segments:
            if seg.kind == "stub":
                members.append(dict(p0=np.asarray(seg.p0, float),
                                    p1=np.asarray(seg.p1, float),
                                    r0=seg.r0, r1=seg.r1,
                                    owner=("route", ri), kind="stub"))
            elif seg.kind == "shoot":
                members.append(dict(p0=np.asarray(seg.p0, float),
                                    p1=np.asarray(seg.p1, float),
                                    r0=seg.r0, r1=seg.r1,
                                    owner=("route", ri),
                                    kind=("strut" if rt.anchor_kind == "model"
                                          else "shoot")))
    for ki, rs in enumerate(risers or []):
        members.append(dict(p0=np.asarray(rs["p0"], float),
                            p1=np.asarray(rs["p1"], float),
                            r0=rs["r"], r1=rs["r"],
                            owner=("riser", ki), kind="riser"))
    for bi, b in enumerate(braces or []):
        members.append(dict(p0=np.asarray(b["p0"], float),
                            p1=np.asarray(b["p1"], float),
                            r0=b["r"], r1=b["r"],
                            owner=("brace", bi),
                            kind=("model_rung" if b.get("j") == -1 else "brace")))
    return members


def compute_joint_balls(routes, trunks, risers, braces, plate_z, cfg):
    """One ball per joint, sized to the fattest member touching it.
    Returns list of dicts {center (3,), r, owner} with owner as in
    collect_members. See module docstring for the rules."""
    members = collect_members(routes, trunks, risers, braces, plate_z)
    if not members:
        return []

    # --- joint NODES: member endpoints that are articulation points.
    nodes = []  # (point, radius_at_end, owner, embedded_model)
    for m in members:
        kind = m["kind"]
        if kind == "column":
            # top is a joint (dome / meeting point of top junctions);
            # a PLATE base is flare+pad territory, a BRANCH base
            # coincides with its y_diagonal's end which supplies the node
            nodes.append((m["p1"], m["r1"], m["owner"], False))
        elif kind == "y_diagonal":
            nodes.append((m["p0"], m["r0"], m["owner"], False))  # on parent
            nodes.append((m["p1"], m["r1"], m["owner"], False))  # child base
        elif kind == "stub":
            # p0 is the tip contact (contact-ball domain, not a joint)
            nodes.append((m["p1"], m["r1"], m["owner"], False))
        elif kind == "shoot":
            nodes.append((m["p0"], m["r0"], m["owner"], False))  # elbow
            nodes.append((m["p1"], m["r1"], m["owner"], False))  # junction
        elif kind == "strut":
            nodes.append((m["p0"], m["r0"], m["owner"], False))
            nodes.append((m["p1"], m["r1"], m["owner"], True))   # model foot
        elif kind in ("riser", "brace"):
            nodes.append((m["p0"], m["r0"], m["owner"], False))
            nodes.append((m["p1"], m["r1"], m["owner"], False))
        elif kind == "model_rung":
            nodes.append((m["p0"], m["r0"], m["owner"], False))
            nodes.append((m["p1"], m["r1"], m["owner"], True))   # model end

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
    # actually sits on, not just who ends there)
    axis_members = [m for m in members
                    if m["kind"] in ("column", "y_diagonal", "riser", "shoot")]

    balls = []
    for idxs in clusters.values():
        cluster_nodes = [nodes[i] for i in idxs]
        r_end = max(n[1] for n in cluster_nodes)
        center = cluster_nodes[int(np.argmax([n[1] for n in cluster_nodes]))][0]
        embedded = any(n[3] for n in cluster_nodes)
        owner = None
        r_max = r_end
        # fattest member whose axis passes through this joint
        for m in axis_members:
            dist = _point_segment_dist(center[None, :], m["p0"], m["p1"])[0]
            if dist <= ON_AXIS_TOL_MM and max(m["r0"], m["r1"]) >= r_max:
                r_max = max(m["r0"], m["r1"])
                owner = m["owner"]
        if owner is None:
            # no fatter through-member: owned by the fattest endpoint
            owner = cluster_nodes[int(np.argmax([n[1] for n in cluster_nodes]))][2]
        r_ball = cfg.joint_ball_factor * r_max
        if embedded and len(cluster_nodes) == 1:
            # adsorbed model foot: adhesion fillet, no second member
            r_ball = MODEL_FOOT_FACTOR * r_end
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
