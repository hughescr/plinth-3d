"""Build-time honesty on the v4 forest (design-v4.md #8, Phase 4a).

Generalizes v3 trunks.transient_worst_span (per-TRUNK: one column, one
restraint list) to v4's forest (per-PATH: any root->terminal walk through
tree.Forest). The core physics is UNCHANGED from v3 -- ported, not
reinvented; see `transient_worst_span` below, which reproduces v3's
sweep verbatim (trunks.py, RETIRED, read for reference only, never
imported):

  - Structure ABOVE the current build height does not exist yet -- at
    any moment, a member is a free cantilever from the highest ACTIVE
    restraint below it up to wherever the build has currently reached.
  - A restraint is a pair (point_z, active_z): point_z is where it grips
    the path; active_z is the build height at which it actually starts
    existing (v3: "a restraint doesn't exist until its OWN connecting
    structure has fully built").
  - A brace (or model rung) activates the instant its own layer prints,
    i.e. once BOTH its endpoints exist: active_z == max(z of both rod
    endpoints) -- deliberately more conservative than v3 (which used
    each side's own attach z): charter #8's build-time-honesty mandate
    overrides. Pinned by test_braced_column_activates_at_upper_rod_endpoint.
  - A junction where sibling branches merge only restrains the path once
    the shorter sibling's ENTIRE subtree has finished printing:
    active_z = min over live siblings of that sibling's own completion
    height (max z over every endpoint of every live member in its
    subtree); point_z = the junction height itself.

Public API (engine wiring by another agent depends on these signatures):
    transient_worst_span(restraints, base_z, z_top) -> float
    derive_brace_attachments(forest) -> dict[int, list[tuple[float, int]]]
    transient_report(forest, plate_z, cfg) -> dict[int, dict]
    r_transient_mm(worst_span_mm, lean_deg, n_terminals, cfg) -> float
    resize_forest(forest, plate_z, cfg) -> dict(resized=..., report=...)
"""
import numpy as np

from . import cco, placement


# =============================================================== the ported pure function
def transient_worst_span(restraints, base_z, z_top):
    """v3 trunks.transient_worst_span, ported VERBATIM (semantics frozen
    by the two oracle pins in test_transient.py, reproduced from retired
    test_trunks.py). `restraints` is an iterable of (point_z, active_z)
    pairs. Pure geometry: no cfg, no forest.

    Porting traps (each pinned by a test):
      - sort by active_z, NOT point_z -- highest_active accumulates
        point_z, but the SORT ORDER (the order restraints are allowed to
        take effect in) is active_z. Conflating the two passes brace-only
        fixtures and fails lagged-junction ones.
      - out-of-range restraints are CLAMPED into [base_z, z_top], never
        filtered out.
      - zero restraints -> full height, via the trailing check alone (no
        special case needed)."""
    events = sorted(restraints, key=lambda t: t[1])
    worst = 0.0
    highest_active = base_z
    for point_z, active_z in events:
        a = min(max(active_z, base_z), z_top)
        worst = max(worst, a - highest_active)
        highest_active = max(highest_active, min(point_z, z_top))
    worst = max(worst, z_top - highest_active)
    return worst


def _span_intervals(restraints, base_z, z_top):
    """The SAME sweep as transient_worst_span, but returning every
    candidate (lo, hi) span considered along the way (including the
    trailing one) instead of collapsing to just the max. Used by
    transient_report to find which candidate span(s) actually intersect
    a given member's own z-extent -- transient_worst_span alone only
    tells you the overall worst number for the whole path, not which
    part of the path it belongs to.

    Consistency with transient_worst_span (both implement the identical
    sweep, independently) is pinned by
    test_span_intervals_max_matches_public_worst_span."""
    events = sorted(restraints, key=lambda t: t[1])
    intervals = []
    highest_active = base_z
    for point_z, active_z in events:
        a = min(max(active_z, base_z), z_top)
        intervals.append((highest_active, a))
        highest_active = max(highest_active, min(point_z, z_top))
    intervals.append((highest_active, z_top))
    return intervals


# =============================================================== brace geometry
def derive_brace_attachments(forest):
    """GEOMETRIC derivation only (design-v4.md #8's deliberate fix for
    v3's i/j-swap bookkeeping bug class: 122/300 endpoints mislabeled on
    the real Treant). Maps live non-brace member idx -> sorted list of
    (z, brace_idx) for every live brace endpoint lying within 0.25mm of
    that member's axis segment (point-to-segment distance -- same rule
    and tolerance as verify.connectivity_report's rule (a), vectorized
    the same way: point-vs-all-axes chunks).

    An endpoint that IS the brace's own designed embed_end (a
    model_rung's model-anchored end) attaches to the MODEL, not a
    member -- skipped, never matched against a host axis at all.

    A brace endpoint may legitimately attach to MORE than one host (e.g.
    landing exactly at a point where two member axes meet) -- every host
    within tolerance is recorded, not just the nearest.
    """
    brace_idxs = [b for b in getattr(forest, "braces", []) if b not in forest.dead]
    out = {}
    if not brace_idxs:
        return out
    brace_set = set(getattr(forest, "braces", []))
    host_idxs = [i for i in range(len(forest.members))
                 if i not in forest.dead and i not in brace_set]
    if not host_idxs:
        return out

    h_p0 = np.array([forest.members[i].p0 for i in host_idxs], dtype=float)
    h_p1 = np.array([forest.members[i].p1 for i in host_idxs], dtype=float)
    d_axis = h_p1 - h_p0
    L2 = np.einsum("ij,ij->i", d_axis, d_axis)
    L2safe = np.where(L2 < 1e-18, 1.0, L2)
    zero_len = L2 < 1e-18

    ep_brace, ep_pts = [], []
    for b_idx in brace_idxs:
        b = forest.members[b_idx]
        for end_name, pt in (("p0", b.p0), ("p1", b.p1)):
            if b.embed_end == end_name:
                continue  # designed to embed into the model, not a member
            ep_brace.append(b_idx)
            ep_pts.append(pt)
    if not ep_pts:
        return out
    pts = np.array(ep_pts, dtype=float)
    ep_brace = np.array(ep_brace)

    n_ep = len(ep_brace)
    CHUNK = 256
    for start in range(0, n_ep, CHUNK):
        end = min(start + CHUNK, n_ep)
        chunk_pts = pts[start:end]                                # (c,3)
        rel = chunk_pts[:, None, :] - h_p0[None, :, :]             # (c,n,3)
        t = np.einsum("cnk,nk->cn", rel, d_axis) / L2safe[None, :]
        t = np.clip(t, 0.0, 1.0)
        proj = h_p0[None, :, :] + t[:, :, None] * d_axis[None, :, :]
        dist = np.linalg.norm(chunk_pts[:, None, :] - proj, axis=2)   # (c,n)
        if zero_len.any():
            dist[:, zero_len] = np.linalg.norm(
                chunk_pts[:, None, :] - h_p0[None, zero_len, :], axis=2)
        within = dist <= 0.25
        for ci in range(end - start):
            b_idx = int(ep_brace[start + ci])
            z = float(chunk_pts[ci][2])
            for hj in np.nonzero(within[ci])[0]:
                host_idx = host_idxs[int(hj)]
                out.setdefault(host_idx, []).append((z, b_idx))

    for host_idx in out:
        out[host_idx].sort(key=lambda t: (t[0], t[1]))
    return out


# =============================================================== subtree helpers
def _live_children(forest, idx):
    return [c for c in forest.children[idx] if c not in forest.dead]


def _subtree_completion(forest, idx, cache):
    """max z over ALL endpoints of every live member in idx's subtree
    (idx included) -- "how far this subtree has grown once it is
    entirely built". Memoized (shared across every terminal-path walk in
    one transient_report call)."""
    if idx in cache:
        return cache[idx]
    m = forest.members[idx]
    top = max(float(m.p0[2]), float(m.p1[2]))
    for c in _live_children(forest, idx):
        top = max(top, _subtree_completion(forest, c, cache))
    cache[idx] = top
    return top


def _subtree_terminal_count(forest, idx, cache):
    """Count of live kind=='stub' descendants of idx (idx included)."""
    if idx in cache:
        return cache[idx]
    m = forest.members[idx]
    count = 1 if m.kind == "stub" else 0
    for c in _live_children(forest, idx):
        count += _subtree_terminal_count(forest, c, cache)
    cache[idx] = count
    return count


def _root_to_terminal_path(forest, terminal_idx):
    path = []
    cur = terminal_idx
    while cur is not None:
        path.append(cur)
        cur = forest.parent[cur]
    path.reverse()
    return path


def _path_events(forest, path, brace_attach, completion_cache):
    """Restraint events for the FULL root->terminal path (design-v4.md
    #8's per-path derivation, spec section "Restraint events for the
    path root -> member M", generalized to the whole path). The root's
    own base is NOT an event here -- it seeds base_z, handled by the
    caller."""
    events = []
    for k, idx in enumerate(path):
        for (z, b_idx) in brace_attach.get(idx, ()):
            if b_idx in forest.dead:
                continue
            b = forest.members[b_idx]
            active_z = max(float(b.p0[2]), float(b.p1[2]))
            events.append((float(z), active_z))
        if k + 1 < len(path):
            m = forest.members[idx]
            next_idx = path[k + 1]
            siblings = [c for c in _live_children(forest, idx) if c != next_idx]
            if siblings:
                active_z = min(_subtree_completion(forest, s, completion_cache)
                                for s in siblings)
                events.append((float(m.p0[2]), active_z))
    return events


def _static_restraint_zs(forest, brace_attach, idx):
    """STATIC restraint points on member idx's own extent (activation-
    timing-independent, see transient_report's docstring for the exact
    source list)."""
    m = forest.members[idx]
    zs = [float(z) for (z, _b) in brace_attach.get(idx, ())]
    if len(_live_children(forest, idx)) >= 2:
        zs.append(float(m.p0[2]))
    parent_idx = forest.parent[idx]
    if parent_idx is None:
        zs.append(float(m.p1[2]))  # root base
    elif len(_live_children(forest, parent_idx)) >= 2:
        zs.append(float(m.p1[2]))  # parent junction, felt at this member's own p1
    zs.sort()
    return zs


def _free_gaps(zmin, zmax, restraint_zs):
    bounds = sorted(set([zmin, zmax] + [z for z in restraint_zs if zmin <= z <= zmax]))
    return [(bounds[i], bounds[i + 1]) for i in range(len(bounds) - 1)]


# =============================================================== the report
def transient_report(forest, plate_z, cfg):
    """Per-member build-time transient report (design-v4.md #8). For
    every live, non-brace, non-stub member M:

        worst_span_mm: activation-aware worst free-cantilever span that
            LOADS M -- brute-force over every root->terminal path
            through M (O(terminals x path length), fine at Treant scale
            ~465 x ~20): for each such path, run the restraint-activation
            sweep over the PATH's full event set (base_z from the path's
            root, z_top the path's own build top), then take the longest
            candidate span that intersects M's own [z_min, z_max]
            extent (a span loads M if it overlaps M's extent at all --
            M, as the physical support under that span, carries its
            reaction). The member's worst_span_mm is the max of that
            over every path through it.
        restraint_zs: STATIC restraint points on M's own extent (brace
            attachments on M, M's own p0 if it's a >=2-child junction,
            M's p1 if M's PARENT is a >=2-child junction, M's own base z
            if M is a root) -- activation-timing independent, sorted.
        free_gaps: maximal sub-intervals of [z_min(M), z_max(M)] between
            consecutive restraint_zs (extent ends always included as
            bounds, even with nothing restraining there).
        n_terminals: count of live stub descendants of M.

    `plate_z` is accepted for signature parity with the rest of the
    sizing pipeline (engine.brace_and_size threads the same (forest,
    plate_z, cfg) triple through every sizing call); root base_z is
    always derived from the root member's own p1.z (plate roots: that IS
    plate_z; strut roots (embed_end=="p1"): the model-anchor z, rigid
    once printed) rather than the parameter, per spec.
    """
    live_braces = {b for b in getattr(forest, "braces", []) if b not in forest.dead}
    brace_attach = derive_brace_attachments(forest)
    completion_cache = {}
    terminal_count_cache = {}

    eligible = [i for i, m in enumerate(forest.members)
                if i not in forest.dead and i not in live_braces and m.kind != "stub"]
    worst = {i: 0.0 for i in eligible}

    terminals = [i for i, m in enumerate(forest.members)
                 if m.kind == "stub" and i not in forest.dead
                 and forest.parent[i] is not None]

    for t in terminals:
        path = _root_to_terminal_path(forest, t)
        base_z = float(forest.members[path[0]].p1[2])
        z_top = max(max(float(forest.members[idx].p0[2]), float(forest.members[idx].p1[2]))
                    for idx in path)
        events = _path_events(forest, path, brace_attach, completion_cache)
        intervals = _span_intervals(events, base_z, z_top)
        for idx in path:
            if idx not in worst:
                continue
            m = forest.members[idx]
            zlo, zhi = sorted((float(m.p0[2]), float(m.p1[2])))
            best_here = 0.0
            for lo, hi in intervals:
                if hi >= zlo and lo <= zhi:
                    best_here = max(best_here, hi - lo)
            if best_here > worst[idx]:
                worst[idx] = best_here

    report = {}
    for idx in eligible:
        m = forest.members[idx]
        zlo, zhi = sorted((float(m.p0[2]), float(m.p1[2])))
        restraint_zs = _static_restraint_zs(forest, brace_attach, idx)
        gaps = _free_gaps(zlo, zhi, restraint_zs)
        n_term = _subtree_terminal_count(forest, idx, terminal_count_cache)
        report[idx] = dict(worst_span_mm=worst[idx], restraint_zs=restraint_zs,
                            free_gaps=gaps, n_terminals=n_term)
    return report


# =============================================================== sizing
def r_transient_mm(worst_span_mm, lean_deg, n_terminals, cfg):
    """r_transient floor (design-v4.md #8): the angle-aware deflection
    solve over the worst transient span, scaled by v3's net N**0.125
    shoot-count rule (v3 spells this two ways -- resize_for_spans applies
    N**0.125 directly, size_trunks applies N**0.5 then a further
    **0.25 -- 0.5*0.25 == 0.125, the same net exponent; ported here in
    its single-step form)."""
    return placement._shaft_r_for_L(worst_span_mm, cfg, lean_deg) * max(n_terminals, 1) ** 0.125


def _postorder(forest):
    """Every live, non-stub, non-brace member index, ordered so a
    member's children always precede it -- forest.members' insertion
    order is not reliably topological (bifurcation splitting can create
    a junction "upper" member after its pre-existing child), so a DFS
    memoized over ALL members (not just forest.roots) is needed for
    correctness independent of forest.roots bookkeeping."""
    order = []
    visited = set()
    live_braces = {b for b in getattr(forest, "braces", [])}

    def rec(idx):
        if idx in visited:
            return
        visited.add(idx)
        if idx in forest.dead:
            return
        for c in forest.children[idx]:
            rec(c)
        if forest.members[idx].kind != "stub":
            order.append(idx)

    for idx in range(len(forest.members)):
        if idx in forest.dead or idx in live_braces:
            continue
        rec(idx)
    return order


def resize_forest(forest, plate_z, cfg):
    """Global re-size, DOWN AND UP, REPLACING (not max-ing with) every
    live non-stub non-brace member's previous r0/r1 -- v3's authoritative
    rule: "a well-braced trunk must be allowed to come back down to
    slender". Bottom-up (children before parents, via _postorder) so
    each member's Murray floor sees its children's ALREADY-FINAL r1.

    r_req = max(Murray fan-in over live children's r1 (unfiltered by
    kind -- stub children count too, matching verify.murray_report),
    cco._r_tension_mm, r_transient_mm, the kind floor). Uniform members
    (r0==r1) and any junction (>=2 children) get r0=r1=r_req; a tapered
    pass-through (shoot/diag/strut carrying a tip taper) keeps its OWN
    r0 untouched and only ever raises r1 to max(r_req, r0). NO ceiling
    clamp (v4: floors hard, ceilings soft). Stubs and braces are never
    resized (excluded by _postorder). All mutation via plain attribute
    assignment (tree.py's guarded setters handle certificates); a value
    that would not actually change is left unwritten so an unchanged
    member's certificate survives via geom_version equality, not just
    the radius-decrease fallback.

    Radius decreases keep certificates valid (checked fat covers thin);
    increases void them -- the CALLER (engine.brace_and_size) revalidates.
    This function does not call revalidate.

    Returns dict(resized=[(idx, r_old_max, r_new), ...], report=<the
    transient_report used>).
    """
    report = transient_report(forest, plate_z, cfg)
    resized = []
    for idx in _postorder(forest):
        if idx not in report:
            continue
        m = forest.members[idx]
        lean = cco._lean_deg(m.p0, m.p1)
        children = [c for c in forest.children[idx] if c not in forest.dead]
        r_murray = (sum(forest.members[c].r1 ** 4 for c in children)) ** 0.25 \
            if children else 0.0
        r_tension = cco._r_tension_mm(m.carried_mm2, cfg)
        info = report[idx]
        r_trans = r_transient_mm(info["worst_span_mm"], lean, info["n_terminals"], cfg)
        kind_floor = cfg.min_trunk_r_mm if m.kind == "trunk" else cfg.min_shoot_r_mm
        r_req = max(r_murray, r_tension, r_trans, kind_floor)

        r_old_max = max(m.r0, m.r1)
        uniform_or_branch = abs(m.r0 - m.r1) < 1e-9 or len(children) >= 2
        if uniform_or_branch:
            if abs(m.r0 - r_req) > 1e-9:
                m.r0 = r_req
            if abs(m.r1 - r_req) > 1e-9:
                m.r1 = r_req
            r_new = r_req
        else:
            r1_new = max(r_req, m.r0)
            if abs(m.r1 - r1_new) > 1e-9:
                m.r1 = r1_new
            r_new = max(m.r0, m.r1)
        resized.append((idx, r_old_max, r_new))
    return dict(resized=resized, report=report)
