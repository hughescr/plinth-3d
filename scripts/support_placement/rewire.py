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
`_model_anchor_builders`, and `_bifurcation_builders` directly -- the
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

Brace-awareness (Phase 4c, SPEC-INTEGRATION #2, "the stale-brace-
impossibility invariant"): once bracing.propose_and_place has run
(engine.brace_and_size calls rewire.optimize a SECOND time against
braced reality), some live members carry a brace attached to their own
axis (transient.derive_brace_attachments -- geometric, host indices
with >=1 live brace endpoint within 0.25mm of that host's axis segment,
computed here once per sweep, see optimize()). None of the three move
types may move, retire, or split a braced host's geometry:
  - reattach (junction-child case): the detached subtree's OWN geometry
    never moves (only its topology/parent pointer does -- see
    _try_subtree_reattach's docstring), so a braced host anywhere INSIDE
    the subtree is fine; the risk is the bifurcation CANDIDATE the
    subtree might land on -- splitting a host moves its axis (see
    cco._make_bifurcation_builder), so braced hosts (and braces
    themselves) are excluded from that candidate set.
  - reattach (root-reanchor case): root_idx itself retires (mark_dead)
    on acceptance -- if root_idx is a braced host, retiring it strands
    that brace, so this move is refused outright when root_idx is one.
  - junction slide: mutates the junction's own p0 AND every child's p1 --
    refused if the junction or any child is a braced host.
  - twig prune: folds m_idx into its surviving child, moving the
    child's p1 -- refused if either m_idx or the child is a braced host.
Braces are themselves never move candidates (_sweep_order skips
forest.braces entirely) and are excluded from every bifurcation
candidate set the same way as braced hosts (cco._make_bifurcation_
builder splits whatever host it's given; a brace is a graph edge, not a
thing with its own children/Murray role to preserve, so it must never
be chosen as one). `_live_member_indices` needs no change: braces ARE
live printed volume (tree.Forest.braces' own docstring), already
included by that filter (brace kind != "stub"). optimize() ends with an
independent geometric ASSERT (crash, not a warning) that no live brace
was left stale by any accepted move -- the invariant the rules above
exist to make geometrically impossible, checked rather than trusted.
"""
import time

import numpy as np

from . import cco, perception, placement, tree, transient

TWIG_MERGE_LEN_MM = 0.5   # design-v4.md #7 "merge degenerate spans (<0.5mm)"
MAX_SLIDE_EVALS = 50      # design-v4.md #7 "<=50 evals per junction"
ROLL_WINDOW = 20          # design-v4.md #7 default W=20
ROLL_EPS = 0.0025         # design-v4.md #7 default eps=0.25%

# SPEC-PATHRUNG W2: a TRUE zero-length member (Treant verify's member_idx
# 577 repro: kind=shoot, length ~1e-6) is a different class from
# TWIG_MERGE_LEN_MM's "short but real, worth folding for volume" spans --
# this is a coordinate collapse (a connector built from se to a point
# that coincides with se to float precision, or a slide that walked a
# child's far endpoint onto its own near endpoint), never geometrically
# meaningful. Deliberately far tighter than TWIG_MERGE_LEN_MM so the two
# classes never overlap/interact: nothing between DEGENERATE_LEN_MM and
# TWIG_MERGE_LEN_MM changes behavior here.
DEGENERATE_LEN_MM = 1e-4


def _is_degenerate_span(p0, p1, tol=DEGENERATE_LEN_MM):
    return float(np.linalg.norm(np.asarray(p1, float) - np.asarray(p0, float))) < tol


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
                                  sample_points, sample_delicacy, cfg, kdc, exclude,
                                  surface_tree=None):
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
    # p6-routing (BUG 1 fix, cco.py): cco._model_anchor_builder (single
    # globally-nearest, no cone filter) was renamed/replaced by
    # cco._model_anchor_builders (plural: cone-pre-filtered, multi-
    # candidate, nearest-first, capped ~40) -- call-site update only, zero
    # change to this function's own move logic. This incidentally gives
    # the reattach search the same fix cco.build's insertion gets: a
    # cone- or clearance-infeasible nearest model anchor no longer kills
    # the model-anchor option for a reattach the way the single-candidate
    # version did.
    yield from cco._model_anchor_builders(forest, root_idx, root_idx, se, r_i, carried_root,
                                           sample_points, sample_delicacy, None, cfg,
                                           geom=surface_tree)
    yield from cco._bifurcation_builders(forest, root_idx, root_idx, se, r_i, carried_root,
                                          plate_z, cfg, kdc, exclude=exclude)


def _try_subtree_reattach(forest, root_idx, surface_tree, cfg, kdc, plate_z,
                           sample_points, sample_delicacy, braced_hosts):
    """Move (a). Returns (accepted, volume_saved_ml). `braced_hosts` (see
    module docstring's brace-awareness section) is a set of member indices
    with a live brace on their own axis, derived once per sweep by the
    caller (transient.derive_brace_attachments)."""
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
                                   sample_points, sample_delicacy, braced_hosts)
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

    # Brace-awareness (SPEC-INTEGRATION #2): the detached subtree's OWN
    # geometry never moves here (only its topology does -- see this
    # function's docstring / module docstring), so a braced host INSIDE
    # `subtree` is not itself at risk; what IS at risk is the bifurcation
    # candidate this subtree might get merged onto -- splitting a host
    # moves its axis (cco._make_bifurcation_builder), which would strand
    # any brace already attached to it. Braced hosts and braces themselves
    # are therefore excluded from the candidate set, not just `subtree`.
    exclude = subtree | forest.dead | {old_parent} | braced_hosts | set(forest.braces)
    best_fn, best_vol = None, None
    for build_fn in _reattach_candidate_builders(forest, root_idx, se, r_i, carried_root,
                                                  plate_z, sample_points, sample_delicacy,
                                                  cfg, kdc, exclude, surface_tree=surface_tree):
        cp2 = forest.checkpoint()
        n0 = len(forest.members)
        try:
            build_fn()
        except cco._InfeasibleCandidate:
            forest.rollback(cp2)
            continue
        # SPEC-PATHRUNG W2 (prevent the creation, preferred over a later
        # cure): a connector built from se to a merge/projection point
        # that coincides with se to float precision is a degenerate
        # (near-zero-length) member -- reject the candidate outright,
        # same as a clearance violation, rather than ever committing it.
        if any(_is_degenerate_span(forest.members[i].p0, forest.members[i].p1)
               for i in range(n0, len(forest.members))):
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
                        sample_points, sample_delicacy, braced_hosts):
    """Reattach variant for a parentless root (see _try_subtree_reattach's
    dispatch comment): root.p0 is the fixed anchor (wherever root's own
    children, if any, already attach); root.p1 offers nothing to search
    from since it's already at the sink. Uses the SAME candidate families
    (bifurcation onto another member / plate descent / model anchor),
    treating root_idx itself as the thing being merged (`se = root.p0`,
    exactly the role a fresh terminal's stub_end plays for cco.build) --
    if a candidate wins, root's own former children are spliced onto the
    NEW connector and root_idx itself retires (tree.Forest.dead): its
    whole old p0->plate run is superseded, not just one endpoint moved.

    Brace-awareness (SPEC-INTEGRATION #2): retiring root_idx would strand
    any brace attached to root_idx's OWN axis, so this move is refused
    outright when root_idx is a braced host -- unlike the junction-child
    case, root_idx's geometry (not just its topology) goes away here.
    Braced hosts (and braces) are also excluded from the bifurcation
    candidate set below, same reasoning as _try_subtree_reattach: bi-
    furcating a candidate host moves ITS axis too."""
    if root_idx in braced_hosts:
        return False, 0.0
    root = forest.members[root_idx]
    own_children = list(forest.children[root_idx])
    subtree = _subtree_indices(forest, root_idx)
    se = root.p0.copy()
    r_i = max(root.r0, cfg.min_shoot_r_mm)
    carried_root = root.carried_mm2
    exclude = subtree | forest.dead | {root_idx} | braced_hosts | set(forest.braces)

    vol_before = _total_volume_ml(forest)
    best_fn, best_vol = None, None
    for build_fn in _reattach_candidate_builders(forest, root_idx, se, r_i, carried_root,
                                                  plate_z, sample_points, sample_delicacy,
                                                  cfg, kdc, exclude, surface_tree=surface_tree):
        cp2 = forest.checkpoint()
        n0 = len(forest.members)
        try:
            build_fn()
        except cco._InfeasibleCandidate:
            forest.rollback(cp2)
            continue
        # SPEC-PATHRUNG W2: same degenerate-connector reject as
        # _try_subtree_reattach above -- see that call site's comment.
        if any(_is_degenerate_span(forest.members[i].p0, forest.members[i].p1)
               for i in range(n0, len(forest.members))):
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
    is violated for any member at ITS OWN new floor-respecting radius.

    SPEC-PATHRUNG W2 (prevent the creation): a candidate J that walks a
    child's far endpoint onto (or within DEGENERATE_LEN_MM of) its own
    near endpoint -- or onto m's own far endpoint -- collapses that
    span to zero length, one root-cause suspect for the Treant verify
    member_idx 577 class (kind=shoot, length ~1e-6). Rejected here,
    upstream of every caller (the hill-climb search AND the final
    application both route through this one function), so a slide can
    never even CONSIDER a degenerate candidate, let alone accept one."""
    if _is_degenerate_span(J, m.p1):
        return None
    child_r1 = []
    for cm in child_members:
        if _is_degenerate_span(cm.p0, J):
            return None
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


def _try_junction_slide(forest, j_idx, surface_tree, cfg, braced_hosts):
    """Move (b): bounded pattern-search re-polish of junction j_idx's
    position (design-v4.md #5/#7), minimizing the REAL volume of the
    junction member itself plus every child that lands on it, subject to
    each incident member's printability cone -- never accepting a point
    worse than the current one (a plain hill-climb: every accepted step
    strictly improves, so "current" is always a valid fallback).

    Brace-awareness (SPEC-INTEGRATION #2): a slide mutates j_idx's OWN p0
    AND every child's p1 -- refused outright if j_idx or ANY child is a
    braced host (either would move geometry a live brace is attached to)."""
    if j_idx in forest.dead:
        return False, 0.0
    if j_idx in braced_hosts:
        return False, 0.0
    m = forest.members[j_idx]
    children = forest.children[j_idx]
    if len(children) < 2:
        return False, 0.0
    if any(c in braced_hosts for c in children):
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


def _try_twig_prune(forest, m_idx, surface_tree, cfg, braced_hosts):
    """Move (c): fold a degenerate (<TWIG_MERGE_LEN_MM) pass-through span
    -- a junction reduced to a single child by an earlier accepted
    reattach (design-v4.md #7 "pass-through junctions left by
    reattachment") -- into its one surviving child, preserving Murray
    consistency (the child's far end is re-floored at its new, slightly
    longer length/lean/load, never below what it already had). `m_idx`
    is marked dead rather than removed (see module docstring / tree.py's
    `dead` field -- interior deletion isn't safe under the append-only
    checkpoint protocol).

    Brace-awareness (SPEC-INTEGRATION #2): m_idx retires (its geometry
    disappears) and the surviving child's p1 moves to m_idx's old p1 --
    refused if EITHER is a braced host."""
    if m_idx in forest.dead:
        return False, 0.0
    if m_idx in braced_hosts:
        return False, 0.0
    m = forest.members[m_idx]
    if m.kind == "stub":
        return False, 0.0
    children = forest.children[m_idx]
    if len(children) != 1:
        return False, 0.0
    c_idx = children[0]
    if c_idx in braced_hosts:
        return False, 0.0
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


# --------------------------------------------- (d) degenerate-member retire --
def _try_degenerate_retire(forest, m_idx, surface_tree, cfg, braced_hosts):
    """Move (d), SPEC-PATHRUNG W2: fold/retire an ALREADY-EXISTING
    degenerate (< DEGENERATE_LEN_MM) member that _try_twig_prune's own
    len(children) == 1 gate never reaches -- the Treant verify member_
    idx 577 class (kind=shoot, length ~1e-6, near_horizontal
    printability violation) has 0 or 2+ children, not 1. This is a
    RETROFIT cure for whatever slips through despite the "prevent the
    creation" guards added to _try_subtree_reattach / _try_root_reanchor
    / _junction_geometry_at above (a construction path this module
    doesn't own -- e.g. cco.py insertion -- could in principle also
    produce one; W2's own repro test pins the specific creation path
    found).

    2+ children: their own p0 already coincides with m's p1 (m is
    near-zero length) to well within DEGENERATE_LEN_MM, so splicing them
    onto m_idx's OWN parent needs no geometry change -- exactly
    _try_root_reanchor's own "own_children just moved here" reasoning,
    one level generalized (m_idx need not be a parentless root here).
    0 children and m.kind != "stub" (a real contact stub is NEVER
    degenerate-retired regardless of length -- it is the model surface
    contact, not a topology artifact): dead weight, detach and
    mark_dead.

    p6-reach (verifier-authorized extension, exactly this function + its
    own tests -- struts now exist, newly reaching a shape neither prior
    move handled): EXACTLY 1 child that is itself a STUB is also this
    function's job, not _try_twig_prune's -- twig-prune's own module
    docstring already refuses ANY stub child unconditionally (the stub
    is the model-surface contact, never folded/merged into a host's own
    geometry), so it can never act on this shape regardless of length.
    The OLD gate here deferred to twig-prune on ANY len(children)==1
    without checking twig-prune could actually act, so a degenerate
    member whose SOLE child is a stub (verifier's live find: full-scale
    Treant, owner ('contact', 205), kind shoot, length 4.6e-7mm, at a
    strut-junction where contact 205's own terminal stub merged directly
    onto contact 206's strut route) fell through BOTH moves -- an
    unclearable printability violation forever. Handled by the SAME
    splice-onto-parent path already used for the 2+ children case below
    (which needs no change: `for c in children: set_parent(c, p_idx)`
    already generalizes correctly to a single-stub-child list) --
    geometrically a no-op, since the stub's own p0 already sits at m's
    p1 to well under DEGENERATE_LEN_MM, unaffected by which member it is
    parented under. A len(children)==1 NON-stub child is UNCHANGED --
    still deferred to twig-prune, per test_degenerate_retire_leaves_
    single_child_pass_through_to_twig_prune (unmodified, still pinned).
    _try_twig_prune itself is untouched by this fix (its stub refusal is
    correct and stays exactly as it is).

    Deliberately NOT gated on strict volume decrease like the other
    three move types -- a near-zero-length member's own volume is
    itself near zero, so "does this save real print material" is the
    wrong question; the reason to retire it is that it is a printability
    violation waiting to happen (near_horizontal, per tree.
    printability_report), which volume accounting cannot see at all.
    Still gated on `forest.revalidate` staying clean, same as every
    other move -- this can only ever REMOVE a violating member, never
    introduce a new clearance/connectivity one."""
    if m_idx in forest.dead:
        return False, 0.0
    if m_idx in braced_hosts:
        return False, 0.0
    m = forest.members[m_idx]
    if m.kind == "stub":
        return False, 0.0
    if not _is_degenerate_span(m.p0, m.p1):
        return False, 0.0
    children = list(forest.children[m_idx])
    if len(children) == 1:
        sole = children[0]
        if sole in forest.dead or forest.members[sole].kind != "stub":
            return False, 0.0  # _try_twig_prune's own job (it CAN act
            # here), unchanged, not duplicated -- see this function's
            # own docstring for the single-STUB-child case, which
            # twig-prune can NEVER act on and falls through to here.
    if any(c in braced_hosts for c in children):
        return False, 0.0

    p_idx = forest.parent[m_idx]
    vol_before = _total_volume_ml(forest)
    cp = forest.checkpoint()
    for c in children:
        forest.set_parent(c, p_idx)
    forest.set_parent(m_idx, None)
    forest.mark_dead(m_idx)
    if p_idx is not None:
        _reconcile_ancestors(forest, p_idx, cfg)
    violations = forest.revalidate(surface_tree, cfg)
    if violations:
        forest.rollback(cp)
        return False, 0.0
    forest.commit(cp)
    vol_after = _total_volume_ml(forest)
    return True, max(0.0, vol_before - vol_after)


# ---------------------------------------------------------- scheduling ------
def _sweep_order(forest):
    """Deterministic sweep order (design-v4.md #7): descending estimated
    saving, stable tiebreak by (kind, idx) so a re-run visits moves in
    exactly the same order. Estimated saving is each candidate's own
    member volume (the reattach/slide/prune move each act ON) -- cheap,
    real-volume-derived, and comparable across all three move types.

    Brace-awareness (SPEC-INTEGRATION #2): members in forest.braces are
    graph edges, not tree topology -- never move candidates for any of the
    three move types, skipped entirely here (the braced-HOST protections
    in _try_junction_slide/_try_root_reanchor/_try_twig_prune/
    _try_subtree_reattach's candidate-set exclusion are the separate,
    complementary rule that a brace's ATTACHMENT HOST also can't move).

    SPEC-PATHRUNG W2: a 4th move kind, "retire" (_try_degenerate_retire),
    for a < DEGENERATE_LEN_MM member with 0 or 2+ children -- the ONE
    class "prune" (len(children) == 1 only) never reaches. Not mutually
    exclusive with "slide" (2+ children) -- both get queued; whichever
    the sweep tries first, the other's own guard (m_idx in forest.dead)
    makes the second a harmless no-op once the first retires it.

    p6-reach (verifier-authorized extension, same one-item grant as
    _try_degenerate_retire itself -- this predicate is the dispatch gate
    that must actually OFFER the new shape to it, or the fix above is
    unreachable from rewire.optimize's real sweep loop): len(children)==1
    is no longer an unconditional retire-exclusion -- when that SOLE
    child is a stub (_try_degenerate_retire's own new case; twig-prune
    can never act on it, see that function's docstring), "retire" is
    queued ALONGSIDE "prune" for the SAME idx, same harmless-no-op-on-
    the-second-mover guarantee as the slide/retire overlap above (each
    move's own m_idx-in-forest.dead check makes order-independence exact
    regardless of which one commits first). A len(children)==1 NON-stub
    child is UNCHANGED -- "prune" only, never offered to "retire".

    p6-reach should-fix (same one-item grant, verifier's live full-scale
    introspection): a degenerate member's own volume is near-zero by
    definition, which sorts its "retire" entry near the TAIL of this
    descending-volume list -- for the single-stub-child case specifically
    (the ONLY shape neither "prune" nor "slide" can ever reach), that
    tail position risks optimize()'s own rolling-savings convergence
    check (itself volume-driven: diminishing returns over the last
    ROLL_WINDOW ACCEPTED moves) breaking the sweep loop before this
    candidate's own turn ever comes up. Confirmed by direct
    instrumentation on the real, full-scale Treant: this predicate
    correctly offered ('retire', 1065) every sweep, yet optimize()'s
    inner loop converged and broke before ever reaching it -- the SAME
    printability violation survived a complete rewire + brace_and_size
    pass. _try_degenerate_retire's own docstring already establishes the
    governing principle for this exact move ("deliberately NOT gated on
    strict volume decrease... a near-zero-length member's own volume is
    itself near zero, so 'does this save real print material' is the
    wrong question"); extending that from the ACCEPTANCE gate to this
    move's own SCHEDULING PRIORITY is the same principle applied
    consistently, not a new one -- a printability fix must not be
    starved by a volume-savings convergence check that was never meant
    to gate it. Sentinel priority (sorts before every real-volume
    candidate, ties broken by the SAME deterministic (kind, idx)
    tiebreak as everything else) applies ONLY to this exact
    single-stub-child shape -- the pre-existing 0/2+-children retire
    path (already established against the Treant's own member_idx 577
    repro, per this function's own docstring) is UNCHANGED, still
    sorted by its own real volume like every other move."""
    braces_set = set(getattr(forest, "braces", ()))
    items = []
    for idx, m in enumerate(forest.members):
        if idx in forest.dead or m.kind == "stub" or idx in braces_set:
            continue
        vol = cco._member_vol_ml(m.p0, m.p1, m.r0, m.r1)
        parent_idx = forest.parent[idx]
        if parent_idx is None or len(forest.children[parent_idx]) >= 2:
            items.append((vol, "reattach", idx))
        children = forest.children[idx]
        single_stub_child = False
        if len(children) >= 2:
            items.append((vol, "slide", idx))
        elif len(children) == 1:
            span = float(np.linalg.norm(m.p1 - m.p0))
            if span < TWIG_MERGE_LEN_MM:
                items.append((vol, "prune", idx))
            sole = children[0]
            single_stub_child = (sole not in forest.dead
                                  and forest.members[sole].kind == "stub")
        if (len(children) != 1 or single_stub_child) and _is_degenerate_span(m.p0, m.p1):
            retire_key = float("inf") if single_stub_child else vol
            items.append((retire_key, "retire", idx))
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

    `surface_tree` (v5, SPEC-V5-GEOM): kept its pre-v5 name (no big-bang
    rename, per the spec's naming note) but IS the surface-EXACT oracle
    in production (engine.py threads `geom` through this parameter for
    both revalidate's clearance queries and this function's own
    model-anchor candidate pool, sec 2.1/2.3); a plain cKDTree still
    works for legacy/unit-test callers via duck-typing at every consumer.
    """
    log = log if log is not None else {}
    t0 = time.time()
    cap_s = cfg.rewire_time_cap_s
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    # v5 (SPEC-V5-GEOM sec 2.3): model-anchor candidate pool is exterior-
    # face centroids when `surface_tree` is a surface-EXACT oracle (it is,
    # in production -- see this function's own docstring on the
    # surface_tree/geom naming convention); falls back to delicacy_field's
    # seeded cloud verbatim for a plain cKDTree (tests/legacy).
    sample_points, sample_delicacy = perception.exterior_anchor_pool(mesh, surface_tree, cfg)

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
        # Brace-awareness (SPEC-INTEGRATION #2): derived once per sweep
        # (geometric, not cached across sweeps -- a prior sweep's accepted
        # moves can change which members a brace's endpoints land on).
        braced_hosts = set(transient.derive_brace_attachments(forest).keys())
        order = _sweep_order(forest)
        any_accepted = False
        for kind, idx in order:
            if time.time() - t0 > cap_s:
                cap_hit = True
                break
            if kind == "reattach":
                ok, saved = _try_subtree_reattach(forest, idx, surface_tree, cfg, kdc,
                                                   plate_z, sample_points, sample_delicacy,
                                                   braced_hosts)
            elif kind == "slide":
                ok, saved = _try_junction_slide(forest, idx, surface_tree, cfg, braced_hosts)
            elif kind == "retire":
                ok, saved = _try_degenerate_retire(forest, idx, surface_tree, cfg, braced_hosts)
            else:
                ok, saved = _try_twig_prune(forest, idx, surface_tree, cfg, braced_hosts)
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
    _assert_no_stale_braces(forest)
    return forest


def _assert_no_stale_braces(forest, tol=0.25):
    """End-of-optimize invariant (SPEC-INTEGRATION #2, "stale-brace
    impossibility"): every live brace's non-embedded endpoint(s) must
    still lie within `tol` mm of some live non-brace member's axis --
    exactly the geometric rule the brace-awareness move guards above exist
    to make impossible; this independently CHECKS it rather than trusting
    the guards, crashing (not warning) if one ever has a hole. A
    self-contained re-derivation of transient.derive_brace_attachments's
    per-host geometry (rather than reusing that host->[(z, brace_idx)]
    mapping directly): that mapping loses per-endpoint identity when both
    of a brace's endpoints could coincidentally share a z (this function
    needs a hard per-endpoint pass/fail, not a per-host aggregate)."""
    braces_set = set(getattr(forest, "braces", ()))
    live_braces = [b for b in braces_set if b not in forest.dead]
    if not live_braces:
        return
    host_idxs = [i for i in range(len(forest.members))
                 if i not in forest.dead and i not in braces_set]
    if not host_idxs:
        raise AssertionError(
            f"rewire.optimize left {len(live_braces)} live brace(s) with no "
            f"live host at all (forest idx: {live_braces})")

    h_p0 = np.array([forest.members[i].p0 for i in host_idxs], dtype=float)
    h_p1 = np.array([forest.members[i].p1 for i in host_idxs], dtype=float)
    d_axis = h_p1 - h_p0
    L2 = np.einsum("ij,ij->i", d_axis, d_axis)
    L2safe = np.where(L2 < 1e-18, 1.0, L2)
    zero_len = L2 < 1e-18

    stale = []
    for b_idx in live_braces:
        b = forest.members[b_idx]
        for end_name, pt in (("p0", b.p0), ("p1", b.p1)):
            if b.embed_end == end_name:
                continue  # designed to embed into the model, not a member
            pt = np.asarray(pt, dtype=float)
            rel = pt[None, :] - h_p0
            t = np.clip(np.einsum("nk,nk->n", rel, d_axis) / L2safe, 0.0, 1.0)
            proj = h_p0 + t[:, None] * d_axis
            dist = np.linalg.norm(pt[None, :] - proj, axis=1)
            if zero_len.any():
                dist[zero_len] = np.linalg.norm(pt[None, :] - h_p0[zero_len], axis=1)
            nearest = float(dist.min())
            if nearest > tol:
                stale.append((b_idx, end_name, pt.tolist(), round(nearest, 4)))
    assert not stale, (
        f"rewire.optimize left {len(stale)} stale brace endpoint(s) -- a live "
        f"brace endpoint no longer lies within {tol}mm of any live host axis "
        f"(brace_idx, end, point, nearest_host_dist_mm): {stale}")
