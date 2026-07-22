"""Bracing-as-restraint (design-v4.md #9, SPEC-BRACING workstream B).

Braces are NON-TREE Forest members (tree.Forest.add_brace): graph edges,
not topology. Two shapes carry over from v3's retired braces.py (read,
never imported, see that module + scripts/tests/test_braces.py for the
geometry authority this reimplements against tree.Forest):

- Cross-link (preferred): a rod (or a pair, "X-pair") between the member
  needing a restraint (M) and a nearby neighbor member (N), climbing at
  v3.6's hardcoded 45-degree construction (rise == the xy distance
  crossed, ALWAYS -- never the printability angle). Pairs wider than
  cfg.twin_single_brace_xy_mm get a crossed X-pair (both rods discounted
  0.5**0.25 for load-sharing); pairs at or under that spacing get ONE
  rod per rung instead, alternating which member it starts from by rung
  parity, sized WITHOUT the discount (v3.6's twin rule -- a second rod
  between columns millimetres apart is cosmetic waste, not a load-sharing
  partner).
- Model rung (fallback, only when no cross-link candidate clears): a
  short, EXACTLY HORIZONTAL rod from M's axis to a nearby model surface
  point, capped at cfg.model_brace_max_len_mm -- the Chitubox-native
  mini-brace, embed_end="p1" so tree.printability_report's length-capped
  near-horizontal exemption applies (kind "model_rung" only -- the gate
  is not weakened for anything else).

This module only proposes and transactionally places braces for ONE
snapshot of a forest against a caller-supplied spans_report (transient.
transient_report's output shape -- another workstream; tests here hand-
build the dicts, that is the decoupling contract per SPEC-BRACING). No
resizing of host members, no engine wiring, no restraint/span computation
beyond consuming spans_report.
"""
import numpy as np

from . import cco, placement, tree

# Half of the connectivity-restraint-dedup tolerance lives in cfg
# (brace_ladder_z_mm/2); see propose_and_place's restraint_zs skip.


def _n_slender(gap_mm, cfg, lean_deg):
    """Smallest n >= 0 (capped at 50, v3's runaway guard) such that
    splitting `gap_mm` into n+1 equal segments brings the deflection-
    required shaft radius for one segment back under
    cfg.max_unbraced_shaft_r_mm at the member's own lean."""
    for n in range(51):
        seg = gap_mm / (n + 1)
        r = placement._shaft_r_for_L(seg, cfg, lean_deg)
        if r <= cfg.max_unbraced_shaft_r_mm:
            return n
    return 50


def compute_needs(forest, spans_report, cfg):
    """For each member and each free gap (z_lo, z_hi) in spans_report,
    decide how many rungs that gap needs (SPEC-BRACING Part 2): the max
    of the slenderness-triggered count (n_slender, deflection budget) and
    the build-time trigger (n_span, v3's brace_free_span_mm cadence).
    Returns an ordered list of (member_idx, z_lo, z_hi, n_rungs) requests
    for n_rungs >= 1, sorted by (member_idx, z_lo) -- deterministic
    regardless of spans_report's own iteration/dict order."""
    requests = []
    for member_idx, info in spans_report.items():
        member = forest.members[member_idx]
        lean = cco._lean_deg(member.p0, member.p1)
        for gap in info.get("free_gaps", []):
            z_lo, z_hi = float(gap[0]), float(gap[1])
            span = z_hi - z_lo
            if span <= 0:
                # Design decision (recorded): a degenerate/zero-length
                # gap needs no rung -- defensive, spans_report is another
                # workstream's output.
                continue
            n_slender = _n_slender(span, cfg, lean)
            n_span = int(span / cfg.brace_free_span_mm)
            n_rungs = max(n_slender, n_span)
            if n_rungs >= 1:
                requests.append((member_idx, z_lo, z_hi, n_rungs))
    requests.sort(key=lambda r: (r[0], r[1]))
    return requests


# ---------------------------------------------------------------- geometry --
def _z_extent(m):
    z0, z1 = float(m.p0[2]), float(m.p1[2])
    return (z0, z1) if z0 <= z1 else (z1, z0)


def _axis_xy_at_z(m, z):
    p0, p1 = m.p0, m.p1
    dz = float(p1[2] - p0[2])
    if abs(dz) < 1e-9:
        return np.array([p0[0], p0[1]], dtype=float)
    t = (z - p0[2]) / dz
    return p0[:2] + t * (p1[:2] - p0[:2])


def _axis_point_at_z(m, z):
    xy = _axis_xy_at_z(m, z)
    return np.array([xy[0], xy[1], z], dtype=float)


def _live_neighbor_indices(forest):
    braces_set = set(forest.braces)
    return [i for i, m in enumerate(forest.members)
            if i not in forest.dead and i not in braces_set and m.kind != "stub"]


def _find_cross_link_candidates(forest, mi, z, cfg):
    """Neighbor candidates for member `mi` at rung height z (SPEC-BRACING
    rule 1): live non-brace non-stub members N != M whose z-extent covers
    [z, z+rise] (rise = d, the xy distance between the axes at z -- v3.6's
    hardcoded 45-degree construction), sorted by (d, idx).

    Fix (verifier blocker #1, floating brace endpoint): rule 1's
    endpoints are "ON THE AXES (centerline to centerline)" for BOTH
    members -- the X-pair's mirrored rod and the twin rule's odd-parity
    rung both read M's own axis AT z+rise (_cross_link_rods), and
    _axis_xy_at_z is an unclamped linear parametrization that silently
    extrapolates past a member's real endpoint rather than failing. A
    candidate whose rise would put z+rise beyond M's own physical
    z-extent is therefore not "on M's axis" at all -- it must be
    rejected here, exactly like N's own z-extent is already rejected
    below, or the rod gets certified while floating disconnected from
    M's real geometry (the 'stale-brace-leak' class connectivity_report
    exists to catch)."""
    M = forest.members[mi]
    z0_M, z1_M = _z_extent(M)
    if not (z0_M - 1e-9 <= z <= z1_M + 1e-9):
        # M's own axis doesn't even reach the requested rung height --
        # no candidate at this z can ever be valid.
        return []
    p_M = _axis_point_at_z(M, z)
    cands = []
    for ni in _live_neighbor_indices(forest):
        if ni == mi:
            continue
        N = forest.members[ni]
        z0, z1 = _z_extent(N)
        if not (z0 - 1e-9 <= z <= z1 + 1e-9):
            continue
        p_N = _axis_point_at_z(N, z)
        d = float(np.linalg.norm(p_N[:2] - p_M[:2]))
        if not (1e-6 < d <= cfg.brace_neighbor_max_xy_mm):
            continue
        rise = d
        if not (z0 - 1e-9 <= z + rise <= z1 + 1e-9):
            continue
        if not (z0_M - 1e-9 <= z + rise <= z1_M + 1e-9):
            continue
        cands.append((d, ni))
    cands.sort(key=lambda t: (t[0], t[1]))
    return cands


def _cross_link_rods(forest, mi, ni, z, d, k, cfg):
    """Rod geometry for one candidate (SPEC-BRACING rule 1): X-pair for
    d > cfg.twin_single_brace_xy_mm (both rods discounted), else a single
    alternating rod (twin rule, undiscounted). rise = d always."""
    M, N = forest.members[mi], forest.members[ni]
    rise = d
    p_M_z, p_N_z = _axis_point_at_z(M, z), _axis_point_at_z(N, z)
    p_M_hi, p_N_hi = _axis_point_at_z(M, z + rise), _axis_point_at_z(N, z + rise)
    rod_len = d * np.sqrt(2.0)
    r_raw = placement._shaft_r_for_L(rod_len, cfg, cfg.max_segment_angle_deg)
    if d <= cfg.twin_single_brace_xy_mm:
        r = max(cfg.brace_r_mm, r_raw)
        if k % 2 == 0:
            return [(p_M_z, p_N_hi, r)]      # even: M -> N climbing
        return [(p_N_z, p_M_hi, r)]          # odd: N -> M climbing
    r = max(cfg.brace_r_mm, r_raw * 0.5 ** 0.25)
    return [(p_M_z, p_N_hi, r), (p_N_z, p_M_hi, r)]


def _find_model_rung_anchor(forest, surface_tree, mi, z, cfg):
    """Rule 2 fallback: nearest surface point within cfg.model_brace_max_
    len_mm of M's axis at z, with |anchor_z - z| <= 0.75, sorted by
    (distance, index). Returns (p0, p1) with p0 = M's axis at the
    ANCHOR's z (so the rod is exactly horizontal by construction) and
    p1 = the anchor, or None if no candidate qualifies."""
    if surface_tree is None:
        return None
    M = forest.members[mi]
    p_M_z = _axis_point_at_z(M, z)
    pts = np.asarray(surface_tree.data, dtype=float)
    if pts.size == 0:
        return None
    dist = np.linalg.norm(pts - p_M_z[None, :], axis=1)
    anchor_z = pts[:, 2]
    mask = (dist <= cfg.model_brace_max_len_mm + 1e-9) & (np.abs(anchor_z - z) <= 0.75 + 1e-9)
    idxs = np.nonzero(mask)[0]
    if len(idxs) == 0:
        return None
    order = idxs[np.argsort(dist[idxs], kind="stable")]
    best = int(order[0])
    anchor = pts[best]
    p0 = _axis_point_at_z(M, float(anchor[2]))
    p1 = np.array(anchor, dtype=float)
    return p0, p1


# --------------------------------------------------------------- placement --
def _try_place_rods(forest, surface_tree, cfg, rods, kind, embed_end, counters):
    """Transactional placement (SPEC-BRACING rule 3): checkpoint, add all
    rods of this ONE candidate (both X-pair rods together, atomically),
    revalidate, rollback on any violation (returning the kind's owner
    counter to where it was so placed members keep a contiguous sequence
    number), else commit. Returns the new member indices or None."""
    cp = forest.checkpoint()
    idxs = []
    for p0, p1, r in rods:
        owner = (kind, counters[kind])
        counters[kind] += 1
        m = tree.Member(p0=p0, p1=p1, r0=r, r1=r, kind=kind, owner=owner,
                         embed_end=embed_end)
        idxs.append(forest.add_brace(m))
    violations = forest.revalidate(surface_tree, cfg)
    if violations:
        forest.rollback(cp)
        counters[kind] -= len(rods)
        return None
    forest.commit(cp)
    return idxs


def _place_one_rung(forest, surface_tree, cfg, mi, z, k, counters):
    """One rung request at height z on member mi: try cross-link
    candidates nearest-first, then the model-rung fallback. Returns a
    list of (member_idx, kind) placed, or None if unplaceable."""
    for d, ni in _find_cross_link_candidates(forest, mi, z, cfg):
        rods = _cross_link_rods(forest, mi, ni, z, d, k, cfg)
        placed = _try_place_rods(forest, surface_tree, cfg, rods, "brace", None, counters)
        if placed is not None:
            return [(idx, "brace") for idx in placed]

    anchor = _find_model_rung_anchor(forest, surface_tree, mi, z, cfg)
    if anchor is None:
        return None
    p0, p1 = anchor
    length = float(np.linalg.norm(p1 - p0))
    if not (1.0 - 1e-9 <= length <= cfg.model_brace_max_len_mm + 1e-9):
        return None
    placed = _try_place_rods(forest, surface_tree, cfg,
                              [(p0, p1, cfg.brace_r_mm)], "model_rung", "p1", counters)
    if placed is not None:
        return [(idx, "model_rung") for idx in placed]
    return None


def propose_and_place(forest, surface_tree, plate_z, cfg, spans_report, log=None):
    """Places braces transactionally against ONE snapshot of spans_report
    (SPEC-BRACING Part 2). `plate_z` is accepted per the interface
    contract (engine wiring depends on this exact signature) but unused
    here -- bracing never roots a member at the plate, only cross-links
    or model rungs; plate-relative placement decisions belong to cco.py.

    Returns dict(placed=[brace idx...], rungs_requested, rungs_placed,
    rungs_unplaceable, by_kind={"brace": n, "model_rung": m}).

    Design decision (recorded): a rung height skipped because an
    existing restraint (spans_report[...]["restraint_zs"]) already sits
    within cfg.brace_ladder_z_mm/2 of it is neither "placed" nor
    "unplaceable" -- it wasn't attempted because it wasn't needed, which
    is a different outcome than "tried and couldn't clear". It still
    counts toward rungs_requested (the raw ask from compute_needs, before
    any placement-time skip/failure)."""
    _ = plate_z  # accepted for interface parity; unused, see docstring
    if log is None:
        log = {}
    requests = compute_needs(forest, spans_report, cfg)
    placed = []
    rungs_requested = 0
    rungs_placed = 0
    rungs_unplaceable = 0
    by_kind = {}
    counters = {"brace": 0, "model_rung": 0}

    for member_idx, z_lo, z_hi, n_rungs in requests:
        rungs_requested += n_rungs
        restraint_zs = (spans_report.get(member_idx, {}).get("restraint_zs") or [])
        for k in range(n_rungs):
            z = z_lo + (k + 1) * (z_hi - z_lo) / (n_rungs + 1)
            if any(abs(z - rz) <= cfg.brace_ladder_z_mm / 2.0 for rz in restraint_zs):
                continue
            result = _place_one_rung(forest, surface_tree, cfg, member_idx, z, k, counters)
            if result is None:
                rungs_unplaceable += 1
                continue
            rungs_placed += 1
            for idx, kind in result:
                placed.append(idx)
                by_kind[kind] = by_kind.get(kind, 0) + 1

    log["bracing"] = dict(rungs_requested=rungs_requested, rungs_placed=rungs_placed,
                           rungs_unplaceable=rungs_unplaceable, by_kind=dict(by_kind))
    return dict(placed=placed, rungs_requested=rungs_requested, rungs_placed=rungs_placed,
                rungs_unplaceable=rungs_unplaceable, by_kind=by_kind)
