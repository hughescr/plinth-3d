"""Removal-leverage gate (design-v4.md #10, Craig's removal model, Phase 5).

Per-tree accessibility evaluated on FINAL geometry, two independent
predicates plus an iterated ordered-removal simulation:

  HANDLE: a contiguous member run with r >= cfg.grip_handle_r_mm, length
    >= cfg.grip_handle_len_mm, and radial free space >= cfg.grip_clear_mm
    against BOTH the model and every OTHER tree. "Contiguous member run"
    (design decision, recorded -- the charter doesn't nail the exact
    graph unit): the maximal CONNECTED subgraph (forest.parent/children
    edges) of non-"stub" members that individually clear the radius
    threshold. A stub is excluded outright regardless of its own radius
    -- its p0 end is the tiny embedded contact tip, never a grip point
    (same exemption class as routing.sample_route_envelope's stub
    skip and tree.printability_report's stub exemption). Free space vs
    the model is folded into candidacy (a run must ALSO clear
    grip_clear_mm against the model to be a real candidate); free space
    vs other trees is evaluated separately per candidate region because
    it depends on which OTHER trees are still present (design-v4.md #10:
    "trees that only become removable after neighbors go").

  PEEL: some direction from a small outward-biased set (see
    _PEEL_ELEVATIONS_DEG) translates every point of the WHOLE tree
    (embedded-end first millimeter excluded, per the charter's literal
    wording) through cfg.pull_dist_mm without entering the model's
    standoff envelope (cfg.standoff_mm). Recorded design decision: PEEL
    checks ONLY the model, never other trees -- the charter's own prose
    names just "the model's standoff envelope" for this predicate, and
    since the model is static this makes PEEL a one-time per-tree
    property (never re-evaluated as neighbors are removed), while HANDLE
    carries the entire neighbor-dependency the ordered-removal loop is
    for. (If real prints show a tree peeling straight into a neighbor
    that's still present, the documented fix is a same-shape "vs other
    trees" sweep added to PEEL, not a HANDLE workaround.)

  ORDERED REMOVAL: iterate -- remove every tree currently passing
    HANDLE+PEEL given the remaining forest; repeat. A tree removed after
    round 1 carries removal_effort = rounds_waited (REPORTED, not priced
    into any acceptance criterion, design-v4.md #10). A residual set no
    round ever clears is the hard-failure BLOCKER case (mutually caging
    or model-caged, or -- a recorded interpretation this implementation
    makes explicit -- a tree whose OWN geometry never satisfies HANDLE
    regardless of any neighbor's presence: see `_blocked_reason`'s
    "no_handle_member" / "handle_too_short" /
    "handle_insufficient_model_clearance" categories, distinguished from
    genuine caging ("neighbor_caged") and model-caging ("peel_blocked")
    -- the charter's own hard-failure parenthetical names only caging,
    but a tree that can never grow a grippable run is just as truly
    unclearable by any removal order, so it is reported the same way,
    tagged with its own distinct reason).
"""
import numpy as np

from . import routing, transient


# Small outward-biased direction set for the PEEL translation sweep
# (design-v4.md #10 names pull_dist_mm as a knob but not the directions
# themselves -- this fixed set is a recorded implementation choice, not a
# config knob: elevation measured from horizontal-outward (0 deg) to
# straight up (90 deg), blended with the tree's own outward xy bearing
# (_outward_xy_bearing) so "outward-biased" has a concrete meaning per
# tree instead of a single global direction.
_PEEL_ELEVATIONS_DEG = (0.0, 25.0, 45.0, 65.0, 90.0)


# =============================================================== union-find helper
def _connected_components(member_set, forest):
    """Union-find over `member_set` using forest.parent/children edges
    restricted to that set (an edge is used only if BOTH endpoints are in
    the set). Deterministic: iterates in sorted member-index order and
    returns components sorted by their own minimum member index (no
    reliance on Python set/dict iteration order for the RESULT, only for
    O(1) membership tests)."""
    members = sorted(member_set)
    uf = {i: i for i in members}

    def find(a):
        while uf[a] != a:
            uf[a] = uf[uf[a]]
            a = uf[a]
        return a

    def union(a, b):
        ra, rb = find(a), find(b)
        if ra != rb:
            uf[rb] = ra

    member_set_fast = set(members)
    for i in members:
        p = forest.parent[i]
        if p is not None and p in member_set_fast:
            union(i, p)

    groups = {}
    for i in members:
        groups.setdefault(find(i), []).append(i)
    result = [sorted(v) for v in groups.values()]
    result.sort(key=lambda v: v[0])
    return result


def forest_trees(forest, unrouted_stub_idx=None):
    """Partition forest.members into per-tree member-index lists: one
    physically-rigid structure per group (design-v4.md #10 "per-tree").
    Dead members (tree.Forest.dead) and honestly-unrouted terminal stubs
    (`unrouted_stub_idx`, the caller's {forest.terminal_of[i] for i in
    unrouted} -- nothing was ever built for these, see cco.build) are
    excluded entirely: they are not part of any physical tree to remove.

    Recorded design decision: a tree here is the union of (a) ordinary
    parent/child topology (forest.parent/children -- what `roots` seeds)
    AND (b) any live BRACE (tree.Forest.braces) that structurally
    attaches to members in two different topological components
    (transient.derive_brace_attachments' geometric host matching) --
    such a brace is real material rigidly joining what would otherwise
    be two separate trees, so removing one requires dealing with the
    other too; they are folded into ONE tree for HANDLE/PEEL purposes. A
    brace attaching to only one host (the common case: an ordinary
    cross-link within one tree, or a model rung) simply joins that one
    tree, no cross-tree merge.
    """
    unrouted_stub_idx = set(unrouted_stub_idx) if unrouted_stub_idx else set()
    live = {i for i in range(len(forest.members))
            if i not in forest.dead and i not in unrouted_stub_idx}
    if not live:
        return []

    comps = _connected_components(live, forest)
    comp_of = {}
    for ci, members_idx in enumerate(comps):
        for m in members_idx:
            comp_of[m] = ci

    uf = list(range(len(comps)))

    def find(a):
        while uf[a] != a:
            uf[a] = uf[uf[a]]
            a = uf[a]
        return a

    def union(a, b):
        ra, rb = find(a), find(b)
        if ra != rb:
            uf[rb] = ra

    host_map = transient.derive_brace_attachments(forest)
    brace_hosts = {}
    for host_idx, entries in sorted(host_map.items()):
        if host_idx not in live:
            continue
        for _z, b_idx in entries:
            if b_idx not in live:
                continue
            brace_hosts.setdefault(b_idx, set()).add(host_idx)

    for b_idx in sorted(brace_hosts):
        hosts = sorted(brace_hosts[b_idx])
        base = comp_of.get(b_idx)
        for h in hosts:
            if base is None:
                base = comp_of[h]
            else:
                union(base, comp_of[h])

    merged = {}
    for ci in range(len(comps)):
        merged.setdefault(find(ci), []).extend(comps[ci])
    result = [sorted(v) for v in merged.values()]
    result.sort(key=lambda v: v[0])
    return result


# =============================================================== sampling
def _sample_member(p0, p1, r0, r1, step_mm, embed_end, inset_mm):
    """Points + linearly-tapered radii along one member, at `step_mm`
    spacing, insetting `inset_mm` from whichever end is the member's
    designed embed_end (if any) -- same shape as verify.clearance_report/
    tree.Forest.revalidate's own sampling, parametrized so callers can
    use the gate-appropriate inset (HANDLE reuses the codebase's existing
    contact_depth_mm+0.2 embed convention; PEEL uses the charter's
    literal "tip's embedded first millimeter", 1.0mm)."""
    p0 = np.asarray(p0, float)
    p1 = np.asarray(p1, float)
    length = float(np.linalg.norm(p1 - p0))
    if length < 1e-9:
        return p0[None, :], np.array([max(r0, r1)])
    n = max(int(length / step_mm), 1)
    t = np.linspace(0.0, 1.0, n + 1)
    if embed_end == "p0":
        t = t[t * length >= inset_mm]
    elif embed_end == "p1":
        t = t[(1 - t) * length >= inset_mm]
    if len(t) == 0:
        return np.zeros((0, 3)), np.zeros(0)
    pts = p0[None, :] + t[:, None] * (p1 - p0)[None, :]
    radii = r0 + t * (r1 - r0)
    return pts, radii


def _min_gap_to_members(pts, radii, other_members):
    """min over all `pts` (own radii `radii`) of point-to-segment gap
    (axis distance - own radius - other member's LOCAL radius at the
    nearest point) against every member in `other_members`. inf if
    either side is empty. Vectorized + chunked over `pts`, mirroring
    verify.connectivity_report's point-vs-all-axes batching."""
    if len(other_members) == 0 or len(pts) == 0:
        return float("inf")
    p0 = np.array([m.p0 for m in other_members], dtype=float)
    p1 = np.array([m.p1 for m in other_members], dtype=float)
    r0 = np.array([m.r0 for m in other_members], dtype=float)
    r1 = np.array([m.r1 for m in other_members], dtype=float)
    d_axis = p1 - p0
    L2 = np.einsum("ij,ij->i", d_axis, d_axis)
    L2safe = np.where(L2 < 1e-18, 1.0, L2)
    zero_len = L2 < 1e-18

    best = float("inf")
    CHUNK = 256
    n = len(pts)
    for start in range(0, n, CHUNK):
        end = min(start + CHUNK, n)
        chunk_pts = pts[start:end]
        chunk_r = radii[start:end]
        rel = chunk_pts[:, None, :] - p0[None, :, :]
        t = np.einsum("cnk,nk->cn", rel, d_axis) / L2safe[None, :]
        t = np.clip(t, 0.0, 1.0)
        proj = p0[None, :, :] + t[:, :, None] * d_axis[None, :, :]
        dist = np.linalg.norm(chunk_pts[:, None, :] - proj, axis=2)
        if zero_len.any():
            dist[:, zero_len] = np.linalg.norm(
                chunk_pts[:, None, :] - p0[None, zero_len, :], axis=2)
        local_r = r0[None, :] + t * (r1[None, :] - r0[None, :])
        gap = dist - local_r - chunk_r[:, None]
        chunk_best = float(gap.min())
        if chunk_best < best:
            best = chunk_best
    return best


# =============================================================== HANDLE
def _handle_runs(tree_members_idx, forest, mesh, surface_tree, cfg):
    """Every maximal connected run of non-"stub" members whose OWN radius
    clears cfg.grip_handle_r_mm, with length + worst model-clearance
    diagnostics attached (see module docstring: "contiguous member run").
    Returns [] iff no member in the tree ever meets the radius threshold
    at all ("no_handle_member")."""
    thick = {i for i in tree_members_idx
             if forest.members[i].kind != "stub"
             and min(forest.members[i].r0, forest.members[i].r1) >= cfg.grip_handle_r_mm}
    runs = []
    for group in _connected_components(thick, forest):
        pts_list, r_list = [], []
        length_mm = 0.0
        for idx in group:
            m = forest.members[idx]
            length_mm += float(np.linalg.norm(m.p1 - m.p0))
            inset = cfg.contact_depth_mm + 0.2  # same designed-embed inset as clearance_report
            pts, radii = _sample_member(m.p0, m.p1, m.r0, m.r1,
                                         cfg.clearance_sample_step_mm, m.embed_end, inset)
            if len(pts):
                pts_list.append(pts)
                r_list.append(radii)
        if pts_list:
            pts = np.concatenate(pts_list)
            radii = np.concatenate(r_list)
            ub = float(radii.max()) + cfg.grip_clear_mm
            dist = routing._surface_distances(mesh, pts, surface_tree, upper_bound=ub)
            worst_gap = float((dist - radii).min())
        else:
            pts = np.zeros((0, 3))
            radii = np.zeros(0)
            worst_gap = float("inf")
        runs.append(dict(
            member_idx=group, length_mm=length_mm, worst_model_gap_mm=worst_gap,
            passes_length=length_mm >= cfg.grip_handle_len_mm - 1e-9,
            passes_model_clear=worst_gap >= cfg.grip_clear_mm - 1e-9,
            _pts=pts, _radii=radii))
    return runs


# =============================================================== PEEL
def _outward_xy_bearing(mesh, pts):
    centroid_xy = np.asarray(mesh.centroid, float)[:2]
    tree_xy = np.mean(pts[:, :2], axis=0)
    v = tree_xy - centroid_xy
    n = float(np.linalg.norm(v))
    return (v / n) if n > 1e-6 else np.array([1.0, 0.0])


def _peel_directions(mesh, pts):
    xy = _outward_xy_bearing(mesh, pts)
    dirs = []
    for elev in _PEEL_ELEVATIONS_DEG:
        z = float(np.sin(np.radians(elev)))
        w = float(np.cos(np.radians(elev)))
        d = np.array([xy[0] * w, xy[1] * w, z])
        n = float(np.linalg.norm(d))
        dirs.append(d / n if n > 1e-9 else np.array([0.0, 0.0, 1.0]))
    return dirs


def _sweep_clear(mesh, surface_tree, pts, radii, direction, cfg):
    """True iff translating every point in `pts` (own `radii`) along
    `direction` from 0 to cfg.pull_dist_mm never enters the model's
    standoff envelope, at cfg.clearance_sample_step_mm sampling density
    along the sweep. One batched surface query for the whole sweep."""
    if len(pts) == 0:
        return False  # nothing to verify against -- fail closed, not silently pass
    step = cfg.clearance_sample_step_mm
    n_steps = max(2, int(round(cfg.pull_dist_mm / step)) + 1)
    fracs = np.linspace(0.0, 1.0, n_steps)
    offsets = fracs * cfg.pull_dist_mm
    all_pts = (pts[None, :, :] + offsets[:, None, None] * direction[None, None, :]).reshape(-1, 3)
    all_r = np.tile(radii, n_steps)
    ub = float(radii.max()) + cfg.standoff_mm
    dist = routing._surface_distances(mesh, all_pts, surface_tree, upper_bound=ub)
    gap = dist - all_r
    return bool(float(gap.min()) >= cfg.standoff_mm - 1e-9)


def _peel_ok(tree_members_idx, forest, mesh, surface_tree, cfg):
    """(ok, direction_used_or_None). Samples the WHOLE tree (every live
    member, including braces -- "moves the whole tree"), excluding the
    charter's literal "tip's embedded first millimeter" inset at any
    designed embed_end."""
    pts_list, r_list = [], []
    for idx in tree_members_idx:
        m = forest.members[idx]
        pts, radii = _sample_member(m.p0, m.p1, m.r0, m.r1,
                                     cfg.clearance_sample_step_mm, m.embed_end, inset_mm=1.0)
        if len(pts):
            pts_list.append(pts)
            r_list.append(radii)
    if not pts_list:
        return False, None
    pts = np.concatenate(pts_list)
    radii = np.concatenate(r_list)
    for d in _peel_directions(mesh, pts):
        if _sweep_clear(mesh, surface_tree, pts, radii, d, cfg):
            return True, d.tolist()
    return False, None


# =============================================================== report assembly
def _blocked_reason(peel_ok, runs):
    if not peel_ok:
        return "peel_blocked"
    if not runs:
        return "no_handle_member"
    if not any(r["passes_length"] for r in runs):
        return "handle_too_short"
    if not any(r["passes_length"] and r["passes_model_clear"] for r in runs):
        return "handle_insufficient_model_clearance"
    return "neighbor_caged"


def _public_run(r):
    out = dict(member_idx=r["member_idx"], length_mm=round(r["length_mm"], 4),
                worst_model_gap_mm=(round(r["worst_model_gap_mm"], 4)
                                     if np.isfinite(r["worst_model_gap_mm"]) else None),
                passes_length=r["passes_length"], passes_model_clear=r["passes_model_clear"])
    if "blocked_by" in r:
        out["blocked_by"] = r["blocked_by"]
    return out


def removal_report(forest, mesh, surface_tree, cfg, unrouted=()):
    """Full design-v4.md #10 gate. `unrouted` is engine.py's list of
    honestly-unrouted contact indices (same value passed to
    engine.finish); used only to exclude their never-built stubs from
    tree grouping (see forest_trees). Deterministic; crashes (not a
    silent skip) on a member with an embed_end outside {p0,p1,None}.

    Returns dict(ok, n_trees, trees, removal_order, removal_effort,
    blocked_trees) -- `ok` is True iff blocked_trees is empty (the
    charter's hard-failure BLOCKER: a residual set no removal order
    clears). removal_effort maps tree_id -> rounds waited past the first
    (0 = removed immediately); only trees that WERE removed appear."""
    for m in forest.members:
        assert m.embed_end in ("p0", "p1", None), (
            f"removal_report: member has invalid embed_end {m.embed_end!r} "
            f"(crash over corruption, not a silent mis-sample)")

    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted if i in forest.terminal_of}
    trees = forest_trees(forest, unrouted_stub_idx)
    if not trees:
        return dict(ok=True, n_trees=0, trees=[], removal_order=[],
                     removal_effort={}, blocked_trees=[])

    tree_ids = [members[0] for members in trees]
    members_of = dict(zip(tree_ids, trees))

    peel_ok_map, peel_dir_map, runs_of = {}, {}, {}
    for tid in tree_ids:
        ok, d = _peel_ok(members_of[tid], forest, mesh, surface_tree, cfg)
        peel_ok_map[tid] = ok
        peel_dir_map[tid] = d
        runs_of[tid] = _handle_runs(members_of[tid], forest, mesh, surface_tree, cfg)

    candidates_of = {tid: [r for r in runs_of[tid]
                            if r["passes_length"] and r["passes_model_clear"]]
                      for tid in tree_ids}
    for tid in tree_ids:
        for region in candidates_of[tid]:
            blocked_by = set()
            for other in tree_ids:
                if other == tid:
                    continue
                other_members = [forest.members[i] for i in members_of[other]]
                gap = _min_gap_to_members(region["_pts"], region["_radii"], other_members)
                if gap < cfg.grip_clear_mm - 1e-9:
                    blocked_by.add(other)
            region["blocked_by"] = sorted(blocked_by)

    def handle_ok(tid, remaining):
        for region in candidates_of[tid]:
            if not (set(region["blocked_by"]) & remaining):
                return True
        return False

    remaining = set(tree_ids)
    order, round_removed = [], {}
    round_num = 0
    while remaining:
        round_num += 1
        removable = sorted(t for t in remaining
                            if peel_ok_map[t] and handle_ok(t, remaining))
        if not removable:
            break
        for t in removable:
            order.append(t)
            round_removed[t] = round_num
        remaining -= set(removable)
    blocked = sorted(remaining)
    removal_effort = {t: round_removed[t] - 1 for t in order}

    per_tree = []
    for tid in tree_ids:
        per_tree.append(dict(
            tree_id=tid, member_idx=members_of[tid], n_members=len(members_of[tid]),
            handle_runs=[_public_run(r) for r in runs_of[tid]],
            handle_ok_isolated=len(candidates_of[tid]) > 0,
            peel_ok=peel_ok_map[tid], peel_direction=peel_dir_map[tid],
            round_removed=round_removed.get(tid),
            removal_effort=removal_effort.get(tid)))

    blocked_trees_out = [
        dict(tree_id=t, member_idx=members_of[t],
             reason=_blocked_reason(peel_ok_map[t], runs_of[t]))
        for t in blocked
    ]

    return dict(
        ok=(len(blocked) == 0), n_trees=len(tree_ids), trees=per_tree,
        removal_order=order, removal_effort=removal_effort,
        blocked_trees=blocked_trees_out)
