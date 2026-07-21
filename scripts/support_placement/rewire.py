"""v4 topology mutation (design-v4.md #7): the piece v3 lacked. cco.py's
insertion is greedy and order-sensitive -- it never revisits a decision
once made, which is exactly how parallel-column waste happens (two
terminals that would merge cheaply if inserted in the other order stay
on separate columns forever). rewire.optimize runs AFTER insertion (and,
per the charter, would run again after any later structural pass; Phase
3 has no later passes yet, so it runs once) and mutates topology with
three move types, all transactional via tree.Forest's checkpoint/
rollback:

  (a) SUBTREE REATTACH -- detach a junction-rooted subtree, try
      reattaching its root elsewhere (nearest members / fresh plate
      descent / model anchor), re-propagate Murray radii on BOTH the
      slimming old path and the fattening new path;
  (b) JUNCTION SLIDE -- bounded numeric re-polish of an existing
      junction's position, minimizing real volume subject to the
      printability cone, never accepting worse than current;
  (c) TWIG PRUNE -- fold a degenerate (<0.5mm) pass-through span (a
      junction reduced to one child by a reattach) into its surviving
      child.

REUSE, not reimplementation (design-v4.md #7 "same candidate machinery
as #4"): (a) calls cco.py's own `_plate_descent_builders`,
`_model_anchor_builder`, and `_bifurcation_builders` directly -- the
exact functions cco.build uses per terminal, with zero change to their
logic. The one real gap (cco.py never needed to exclude a host, since a
fresh terminal can never be its own ancestor) is closed by the minimal
`exclude=` parameter added to `_KDCache.nearest_members` and
`_bifurcation_builders` (both default to `None`, reproducing Phase 2's
exact candidate set when omitted).

Deletion vs retirement (recorded ambiguity): tree.Forest's checkpoint/
rollback protocol is append-only -- rollback truncates the member list
from the TAIL only (`del self.members[n0:]`), so an interior member can
never be safely removed mid-list without breaking every stored index in
every live checkpoint frame. TWIG PRUNE therefore never deletes a row;
it folds the degenerate member's span into its surviving child and marks
the row `dead` (tree.Forest.dead, added this phase) -- the same
"present in forest.members but excluded from every downstream
consumer" pattern already established for honestly-unrouted stubs.

No RNG: sweep order is a stable sort (volume, kind, index tiebreak);
re-running optimize on an identical forest is bit-identical.
"""
import time

import numpy as np

from . import cco, perception, placement, tree

TWIG_MERGE_LEN_MM = 0.5   # design-v4.md #7 "merge degenerate spans (<0.5mm)"
MAX_SLIDE_EVALS = 50      # design-v4.md #7 "<=50 evals per junction"
ROLL_WINDOW = 20          # design-v4.md #7 default W=20
ROLL_EPS = 0.0025         # design-v4.md #7 default eps=0.25%


# ------------------------------------------------------------- accounting --
def _live_member_indices(forest):
    """Every member that actually contributes to the printed forest: not
    retired by twig-prune, and not an honestly-unrouted stub (a stub
    whose terminal never attached anywhere -- forest.parent is None for
    those, same test engine.py uses via its `unrouted` list, derived
    here straight from the forest so rewire.py doesn't need that list
    threaded through its signature)."""
    return [i for i, m in enumerate(forest.members)
            if i not in forest.dead
            and not (m.kind == "stub" and forest.parent[i] is None)]


def _total_volume_ml(forest):
    total = 0.0
    for i in _live_member_indices(forest):
        m = forest.members[i]
        total += cco._member_vol_ml(m.p0, m.p1, m.r0, m.r1)
    return total


def _subtree_indices(forest, root_idx):
    """root_idx and every descendant (BFS over forest.children)."""
    out = set()
    stack = [root_idx]
    while stack:
        i = stack.pop()
        if i in out:
            continue
        out.add(i)
        stack.extend(forest.children[i])
    return out


# ------------------------------------------------------ (a) subtree reattach --
def _slim_ancestors(forest, start_idx, removed_carried_mm2, cfg):
    """Inverse of cco._propagate_murray for the OLD path a detached
    subtree leaves behind (design-v4.md #7 "old parent slims"): walk
    start_idx -> root, removing the detached subtree's load and
    re-deriving each ancestor's radius from its CURRENT (post-detach)
    children -- Murray sum over what's actually still attached, floored
    by deflection/tension/kind exactly as cco.py sizes every span
    (design-v4.md #6). Called after the subtree has already been
    detached (forest.set_parent(root_idx, None)), so forest.children at
    each ancestor no longer includes it."""
    idx = start_idx
    while idx is not None:
        m = forest.members[idx]
        m.carried_mm2 = max(0.0, m.carried_mm2 - removed_carried_mm2)
        children = forest.children[idx]
        if children:
            r_murray = sum(forest.members[c].r1 ** 4 for c in children) ** 0.25
        else:
            r_murray = 0.0
        L = float(np.linalg.norm(m.p1 - m.p0))
        lean = cco._lean_deg(m.p0, m.p1)
        r_defl = placement._shaft_r_for_L(L, cfg, lean)
        r_tension = cco._r_tension_mm(m.carried_mm2, cfg)
        kind_floor = cfg.min_trunk_r_mm if m.kind == "trunk" else cfg.min_shoot_r_mm
        r_new = max(r_murray, r_defl, r_tension, kind_floor)
        m.r0 = r_new
        m.r1 = r_new
        idx = forest.parent[idx]


def _reattach_candidate_builders(forest, root_idx, se, r_i, carried_root, plate_z,
                                  sample_points, sample_delicacy, cfg, kdc, exclude):
    """Same three candidate families cco._candidate_builders offers a
    fresh terminal (design-v4.md #7: "same candidate machinery as #4"),
    called with `root_idx` standing in for a terminal's stub member --
    each builder only ever mutates topology (forest.set_parent) plus
    whatever NEW members it creates; root_idx's own geometry is touched
    only by the bifurcation path (which must draw a real connecting
    member from root_idx's fixed exit point `se` to wherever the new
    merge point lands), exactly as it would for a fresh stub. `root_idx`
    also fills the owner-tag slot cco's builders hardcode as
    `("contact", i)` -- these are rewire-created connectors, not contact
    terminals, so the tag reads `("contact", root_idx)` (a member index,
    not a contact index); recorded ambiguity, since owner is an opaque
    2-tuple grouping key downstream (joint balls / emission), not
    contractually a contact index.

    Performance-motivated scope reduction (recorded, does not touch
    cco.build/insertion at all): cco._plate_descent_builders' full spiral
    fan (trunk_search_radii_mm x trunk_search_directions, ~96 candidates)
    exists so the FIRST terminal into an empty forest can still route
    around canopy obstruction (design-v4.md #4) -- a cost the insertion
    builder pays once per TERMINAL. rewire re-evaluates every eligible
    subtree root every sweep until convergence; paying the full fan per
    attempt made a single engine.run on the tiny table fixture take 18s+
    (measured), before any real Treant-scale run. Bifurcation-onto-an-
    existing-member is the move that actually eliminates parallel-column
    waste (design-v4.md #7's whole point); a fresh plate descent is a
    fallback the reattach search still offers, just at one candidate
    (straight down from the freed root) instead of the full spiral --
    the straight-down case is always cco._plate_descent_builders' first
    yield."""
    plate_candidates = cco._plate_descent_builders(forest, root_idx, root_idx, se, r_i,
                                                    carried_root, plate_z, cfg)
    for build_fn in plate_candidates:
        yield build_fn
        break
    mb = cco._model_anchor_builder(forest, root_idx, root_idx, se, r_i, carried_root,
                                    sample_points, sample_delicacy, None, cfg)
    if mb is not None:
        yield mb
    yield from cco._bifurcation_builders(forest, root_idx, root_idx, se, r_i, carried_root,
                                          plate_z, cfg, kdc, exclude=exclude)


def _try_subtree_reattach(forest, root_idx, surface_tree, cfg, kdc, plate_z,
                           sample_points, sample_delicacy):
    """Move (a). Returns (accepted, volume_saved_ml)."""
    if root_idx in forest.dead:
        return False, 0.0
    root = forest.members[root_idx]
    if root.kind == "stub":
        return False, 0.0
    old_parent = forest.parent[root_idx]
    if old_parent is None:
        # A forest root's FREE end (design-v4.md #7's "detach... try
        # reattaching") is its p0, not its p1: p1 is already resting on
        # the plate/anchor (nothing is sinkward of the sink), so a
        # junction-child's "slide p1 to a new merge point" framing below
        # is geometrically meaningless here. _try_root_reanchor handles
        # this distinct (but equally common -- every never-merged plate
        # column is a parentless root) case: two independent plate-rooted
        # columns serving nearby terminals, the textbook parallel-column
        # waste this phase exists to fix.
        return _try_root_reanchor(forest, root_idx, surface_tree, cfg, kdc, plate_z,
                                   sample_points, sample_delicacy)
    if len(forest.children[old_parent]) < 2:
        return False, 0.0  # would leave old_parent dangling with zero children

    subtree = _subtree_indices(forest, root_idx)
    se = root.p1.copy()
    r_i = max(root.r1, cfg.min_shoot_r_mm)
    carried_root = root.carried_mm2

    vol_before = _total_volume_ml(forest)
    cp = forest.checkpoint()
    forest.set_parent(root_idx, None)
    _slim_ancestors(forest, old_parent, carried_root, cfg)

    exclude = subtree | forest.dead | {old_parent}
    best_fn, best_vol = None, None
    for build_fn in _reattach_candidate_builders(forest, root_idx, se, r_i, carried_root,
                                                  plate_z, sample_points, sample_delicacy,
                                                  cfg, kdc, exclude):
        cp2 = forest.checkpoint()
        try:
            build_fn()
        except cco._InfeasibleCandidate:
            forest.rollback(cp2)
            continue
        violations = forest.revalidate(surface_tree, cfg)
        vol = _total_volume_ml(forest)
        forest.rollback(cp2)
        if violations:
            continue
        if best_vol is None or vol < best_vol:
            best_vol, best_fn = vol, build_fn

    if best_fn is None:
        forest.rollback(cp)
        return False, 0.0

    best_fn()
    violations = forest.revalidate(surface_tree, cfg)
    assert not violations, (
        "rewire candidate re-application produced a different clearance result "
        "(non-determinism) -- crash over corruption")
    vol_after = _total_volume_ml(forest)
    if vol_after >= vol_before - 1e-9:
        forest.rollback(cp)
        return False, 0.0
    forest.commit(cp)
    return True, vol_before - vol_after


def _try_root_reanchor(forest, root_idx, surface_tree, cfg, kdc, plate_z,
                        sample_points, sample_delicacy):
    """Reattach variant for a parentless root (see _try_subtree_reattach's
    dispatch comment): root.p0 is the fixed anchor (wherever root's own
    children, if any, already attach); root.p1 offers nothing to search
    from since it's already at the sink. Uses the SAME candidate families
    (bifurcation onto another member / plate descent / model anchor),
    treating root_idx itself as the thing being merged (`se = root.p0`,
    exactly the role a fresh terminal's stub_end plays for cco.build) --
    if a candidate wins, root's own former children are spliced onto the
    NEW connector and root_idx itself retires (tree.Forest.dead): its
    whole old p0->plate run is superseded, not just one endpoint moved."""
    root = forest.members[root_idx]
    own_children = list(forest.children[root_idx])
    subtree = _subtree_indices(forest, root_idx)
    se = root.p0.copy()
    r_i = max(root.r0, cfg.min_shoot_r_mm)
    carried_root = root.carried_mm2
    exclude = subtree | forest.dead | {root_idx}

    vol_before = _total_volume_ml(forest)
    best_fn, best_vol = None, None
    for build_fn in _reattach_candidate_builders(forest, root_idx, se, r_i, carried_root,
                                                  plate_z, sample_points, sample_delicacy,
                                                  cfg, kdc, exclude):
        cp2 = forest.checkpoint()
        try:
            build_fn()
        except cco._InfeasibleCandidate:
            forest.rollback(cp2)
            continue
        violations = forest.revalidate(surface_tree, cfg)
        vol = _total_volume_ml(forest)
        forest.rollback(cp2)
        if violations:
            continue
        if best_vol is None or vol < best_vol:
            best_vol, best_fn = vol, build_fn

    if best_fn is None:
        return False, 0.0

    cp = forest.checkpoint()
    best_fn()
    # best_fn attached root_idx as a CHILD of the new connector (correct
    # for a junction-child reattach; here root_idx is being retired
    # instead) -- splice root's own former children onto the new
    # connector (their p1 already coincides with se == the connector's
    # p0 exactly, so no geometry change needed for them) and kill
    # root_idx's now-superseded old run.
    new_parent = forest.parent[root_idx]
    forest.set_parent(root_idx, None)
    for c in own_children:
        forest.set_parent(c, new_parent)
    forest.mark_dead(root_idx)
    # The new connector's radius was sized by _reattach_candidate_builders
    # as if for a FRESH terminal (design-v4.md #4's r_i/carried_i inputs
    # cover a terminal's own tip + tension, never a Murray requirement
    # from inherited children -- cco._plate_descent_builders' column
    # sizing in particular never even looks at r_i for a straight-down
    # candidate). own_children just moved here for real, so re-derive the
    # connector's floor from what it ACTUALLY carries now, same
    # from-scratch recompute twig-prune/junction-slide already use.
    _reconcile_ancestors(forest, new_parent, cfg)
    violations = forest.revalidate(surface_tree, cfg)
    if violations:
        forest.rollback(cp)
        return False, 0.0
    vol_after = _total_volume_ml(forest)
    if vol_after >= vol_before - 1e-9:
        forest.rollback(cp)
        return False, 0.0
    forest.commit(cp)
    return True, vol_before - vol_after


# -------------------------------------------------------- (b) junction slide --
def _own_floor(p0, p1, carried_mm2, kind, cfg):
    """design-v4.md #6's r = max(r_load, r_deflect, kind floor) for a
    span whose OWN geometry is (p0, p1) -- the non-Murray terms, i.e.
    what a member must meet regardless of what merges onto it."""
    L = float(np.linalg.norm(np.asarray(p1, float) - np.asarray(p0, float)))
    lean = cco._lean_deg(p0, p1)
    r_defl = placement._shaft_r_for_L(L, cfg, lean)
    r_tension = cco._r_tension_mm(carried_mm2, cfg)
    kind_floor = cfg.min_trunk_r_mm if kind == "trunk" else cfg.min_shoot_r_mm
    return max(r_defl, r_tension, kind_floor)


def _junction_geometry_at(J, m, child_members, cfg):
    """Re-derive every incident member's floor-respecting radius AT
    candidate position J (design-v4.md #6/#7): sliding a junction changes
    every incident member's length and lean, so a radius that was correct
    at J0 need not be correct at J -- recomputing this at every candidate
    (not just position) is what keeps a slide from silently undersizing a
    member the way the certificate rule forbids for insertion (this bug
    was caught by test_engine_every_member_meets_its_own_load_and_
    deflection_floor: a slide moved a child's endpoint ~8mm with its
    radius untouched). Returns (child_r1_list, m_r) or None if the cone
    is violated for any member at ITS OWN new floor-respecting radius."""
    child_r1 = []
    for cm in child_members:
        r_floor = _own_floor(cm.p0, J, cm.carried_mm2, cm.kind, cfg)
        r_new = max(cm.r0, r_floor)
        if not cco._within_cone(cm.p0, J, r_new, cfg):
            return None
        child_r1.append(r_new)
    r_murray = sum(r ** 4 for r in child_r1) ** 0.25
    m_floor = _own_floor(J, m.p1, m.carried_mm2, m.kind, cfg)
    m_r = max(r_murray, m_floor)
    if not cco._within_cone(J, m.p1, m_r, cfg):
        return None
    return child_r1, m_r


def _junction_feasible_vol(J, m, child_members, cfg):
    geo = _junction_geometry_at(J, m, child_members, cfg)
    if geo is None:
        return None
    child_r1, m_r = geo
    v = cco._member_vol_ml(J, m.p1, m_r, m_r)
    for cm, r in zip(child_members, child_r1):
        v += cco._member_vol_ml(cm.p0, J, cm.r0, r)
    return v


def _try_junction_slide(forest, j_idx, surface_tree, cfg):
    """Move (b): bounded pattern-search re-polish of junction j_idx's
    position (design-v4.md #5/#7), minimizing the REAL volume of the
    junction member itself plus every child that lands on it, subject to
    each incident member's printability cone -- never accepting a point
    worse than the current one (a plain hill-climb: every accepted step
    strictly improves, so "current" is always a valid fallback)."""
    if j_idx in forest.dead:
        return False, 0.0
    m = forest.members[j_idx]
    children = forest.children[j_idx]
    if len(children) < 2:
        return False, 0.0
    child_members = [forest.members[c] for c in children]
    J0 = m.p0.copy()
    orig_vol = _junction_feasible_vol(J0, m, child_members, cfg)
    if orig_vol is None:
        return False, 0.0  # current state should already be feasible; defensive only

    best_J, best_vol = J0.copy(), orig_vol
    step = 1.0
    evals = 0
    directions = np.array([[1, 0, 0], [-1, 0, 0], [0, 1, 0],
                            [0, -1, 0], [0, 0, 1], [0, 0, -1]], dtype=float)
    while evals < MAX_SLIDE_EVALS and step > 0.02:
        improved = False
        for d in directions:
            if evals >= MAX_SLIDE_EVALS:
                break
            cand = best_J + d * step
            v = _junction_feasible_vol(cand, m, child_members, cfg)
            evals += 1
            if v is not None and v < best_vol - 1e-9:
                best_vol, best_J, improved = v, cand, True
        if not improved:
            step *= 0.5

    if best_vol >= orig_vol - 1e-9:
        return False, 0.0
    geo = _junction_geometry_at(best_J, m, child_members, cfg)
    assert geo is not None, "best_J was accepted as feasible above -- must still be feasible"
    child_r1, m_r = geo

    # Full-forest accounting (not just the local m+children delta): m_r
    # can differ from m's OLD radius, and m's parent chain was sized
    # against that old value -- _reconcile_ancestors below may need to
    # grow (or could, in principle, be able to shrink) ancestors further
    # up, which the local estimate above doesn't see. Matches how the
    # other two move types (reattach, prune) both gate on strict real
    # total-volume decrease, not a local proxy.
    vol_before = _total_volume_ml(forest)
    cp = forest.checkpoint()
    m.p0 = best_J
    m.r0 = m_r
    m.r1 = m_r
    for cm, r in zip(child_members, child_r1):
        cm.p1 = best_J.copy()
        cm.r1 = r
    _reconcile_ancestors(forest, forest.parent[j_idx], cfg)
    violations = forest.revalidate(surface_tree, cfg)
    if violations:
        forest.rollback(cp)
        return False, 0.0
    vol_after = _total_volume_ml(forest)
    if vol_after >= vol_before - 1e-9:
        forest.rollback(cp)
        return False, 0.0
    forest.commit(cp)
    return True, vol_before - vol_after


# ----------------------------------------------------------- (c) twig prune --
def _reconcile_ancestors(forest, start_idx, cfg):
    """After twig prune re-floors the surviving child `c` (which can grow
    its radius at its new, slightly longer span), c's NEW parent's Murray
    sum has changed -- re-derive every ancestor's floor from its CURRENT
    children (same formula as _slim_ancestors, but no carried-load change
    here: twig prune moves no load, only removes a redundant vertex), and
    stop as soon as an ancestor doesn't need to grow (nothing above it
    could need to grow either, since that ancestor's own r1 -- the only
    thing an ancestor further up sees -- didn't change)."""
    idx = start_idx
    while idx is not None:
        m = forest.members[idx]
        children = forest.children[idx]
        r_murray = (sum(forest.members[c].r1 ** 4 for c in children) ** 0.25
                    if children else 0.0)
        r_floor = _own_floor(m.p0, m.p1, m.carried_mm2, m.kind, cfg)
        r_new = max(r_murray, r_floor)
        if r_new <= max(m.r0, m.r1) + 1e-9:
            break
        m.r0 = r_new
        m.r1 = r_new
        idx = forest.parent[idx]


def _try_twig_prune(forest, m_idx, surface_tree, cfg):
    """Move (c): fold a degenerate (<TWIG_MERGE_LEN_MM) pass-through span
    -- a junction reduced to a single child by an earlier accepted
    reattach (design-v4.md #7 "pass-through junctions left by
    reattachment") -- into its one surviving child, preserving Murray
    consistency (the child's far end is re-floored at its new, slightly
    longer length/lean/load, never below what it already had). `m_idx`
    is marked dead rather than removed (see module docstring / tree.py's
    `dead` field -- interior deletion isn't safe under the append-only
    checkpoint protocol)."""
    if m_idx in forest.dead:
        return False, 0.0
    m = forest.members[m_idx]
    if m.kind == "stub":
        return False, 0.0
    children = forest.children[m_idx]
    if len(children) != 1:
        return False, 0.0
    c_idx = children[0]
    c = forest.members[c_idx]
    if c.kind == "stub":
        return False, 0.0
    span_len = float(np.linalg.norm(m.p1 - m.p0))
    if span_len >= TWIG_MERGE_LEN_MM:
        return False, 0.0

    p_idx = forest.parent[m_idx]
    new_p1 = m.p1.copy()
    L = float(np.linalg.norm(new_p1 - c.p0))
    lean = cco._lean_deg(c.p0, new_p1)
    r_defl = placement._shaft_r_for_L(L, cfg, lean)
    r_tension = cco._r_tension_mm(c.carried_mm2, cfg)
    kind_floor = cfg.min_trunk_r_mm if c.kind == "trunk" else cfg.min_shoot_r_mm
    new_r1 = max(c.r1, m.r1, r_defl, r_tension, kind_floor)

    # Quick reject before paying for a checkpoint: if even the LOCAL
    # (m + c) volume doesn't improve, the full-forest accounting below
    # (which also captures any ancestor re-floor from _reconcile_ancestors)
    # can only be worse still.
    local_before = (cco._member_vol_ml(m.p0, m.p1, m.r0, m.r1)
                    + cco._member_vol_ml(c.p0, c.p1, c.r0, c.r1))
    local_after = cco._member_vol_ml(c.p0, new_p1, c.r0, new_r1)
    if local_after >= local_before - 1e-9:
        return False, 0.0

    vol_before = _total_volume_ml(forest)
    cp = forest.checkpoint()
    c.p1 = new_p1
    c.r1 = new_r1
    forest.set_parent(c_idx, p_idx)
    _reconcile_ancestors(forest, p_idx, cfg)
    violations = forest.revalidate(surface_tree, cfg)
    if violations:
        forest.rollback(cp)
        return False, 0.0
    forest.mark_dead(m_idx)
    vol_after = _total_volume_ml(forest)
    if vol_after >= vol_before - 1e-9:
        forest.rollback(cp)
        return False, 0.0
    forest.commit(cp)
    return True, vol_before - vol_after


# ---------------------------------------------------------- scheduling ------
def _sweep_order(forest):
    """Deterministic sweep order (design-v4.md #7): descending estimated
    saving, stable tiebreak by (kind, idx) so a re-run visits moves in
    exactly the same order. Estimated saving is each candidate's own
    member volume (the reattach/slide/prune move each act ON) -- cheap,
    real-volume-derived, and comparable across all three move types."""
    items = []
    for idx, m in enumerate(forest.members):
        if idx in forest.dead or m.kind == "stub":
            continue
        vol = cco._member_vol_ml(m.p0, m.p1, m.r0, m.r1)
        parent_idx = forest.parent[idx]
        if parent_idx is None or len(forest.children[parent_idx]) >= 2:
            items.append((vol, "reattach", idx))
        children = forest.children[idx]
        if len(children) >= 2:
            items.append((vol, "slide", idx))
        elif len(children) == 1:
            span = float(np.linalg.norm(m.p1 - m.p0))
            if span < TWIG_MERGE_LEN_MM:
                items.append((vol, "prune", idx))
    items.sort(key=lambda t: (-t[0], t[1], t[2]))
    return [(kind, idx) for _, kind, idx in items]


def optimize(forest, mesh, surface_tree, cfg, log=None):
    """Phase 3 entry point (design-v4.md #7): mutate `forest`'s topology
    in place and return it. Termination is diminishing-returns, not an
    iteration count (Craig's directive): stop when the rolling savings
    over the last ROLL_WINDOW accepted moves drop below ROLL_EPS of the
    current total, OR a full sweep accepts nothing, OR the wall-clock cap
    `cfg.rewire_time_cap_s` is hit (logged as `cap_hit`, distinct from
    convergence, so a cutoff reads as budget vs. an actual local optimum).
    """
    log = log if log is not None else {}
    t0 = time.time()
    cap_s = cfg.rewire_time_cap_s
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    sample_points, _sample_normals, sample_delicacy, _query = perception.delicacy_field(mesh, cfg)

    total_vol = _total_volume_ml(forest)
    curve = [(0, total_vol)]
    accepted = 0
    rejected = 0
    sweeps = 0
    cap_hit = False
    move_idx = 0
    recent = []
    converged = False

    while True:
        if time.time() - t0 > cap_s:
            cap_hit = True
            break
        sweeps += 1
        kdc = cco._KDCache()
        kdc.rebuild(forest)
        order = _sweep_order(forest)
        any_accepted = False
        for kind, idx in order:
            if time.time() - t0 > cap_s:
                cap_hit = True
                break
            if kind == "reattach":
                ok, saved = _try_subtree_reattach(forest, idx, surface_tree, cfg, kdc,
                                                   plate_z, sample_points, sample_delicacy)
            elif kind == "slide":
                ok, saved = _try_junction_slide(forest, idx, surface_tree, cfg)
            else:
                ok, saved = _try_twig_prune(forest, idx, surface_tree, cfg)
            if ok:
                accepted += 1
                any_accepted = True
                move_idx += 1
                total_vol -= saved
                curve.append((move_idx, total_vol))
                recent.append(saved)
                if len(recent) > ROLL_WINDOW:
                    recent.pop(0)
                if len(recent) == ROLL_WINDOW and sum(recent) < ROLL_EPS * max(total_vol, 1e-9):
                    converged = True
                    break
            else:
                rejected += 1
        if cap_hit or converged:
            break
        if not any_accepted:
            break  # full sweep accepted nothing

    log["rewire"] = dict(curve=curve, accepted=accepted, rejected=rejected,
                          sweeps=sweeps, cap_hit=cap_hit)
    return forest
