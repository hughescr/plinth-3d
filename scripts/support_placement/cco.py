"""v4 CCO insertion builder (design-v4.md #4, #5, #6): incremental
Constrained Constructive Optimization -- one terminal at a time, cheapest
REAL-volume candidate that survives clearance wins, transactionally.

Phase 2 scope (design-v4.md #14 phase 2): straight segments only (no
curvature -- that's Phase 4); the Xia closed form + cone clamp for
bifurcation, with numeric junction POLISH deferred to Phase 3's
rewire.py (this module still moves the junction off the naive segment
projection via a bounded Weiszfeld iteration toward the weighted-Fermat
optimum -- the same objective the closed-form angles are the optimality
conditions FOR -- rather than leaving it exactly on the host's original
line; see _weighted_fermat_point); the ported v3 spiral plate-anchor
search for candidate (a).

No RNG anywhere; every sort key is a stable tuple (index tiebreaks) so a
re-run on the same inputs reproduces the same forest exactly.
"""
import numpy as np
from scipy.spatial import cKDTree

from . import joints, placement, struts, tree

KDTREE_REBUILD_EVERY = 25   # design-v4.md #4: bifurcation candidate search


# ---------------------------------------------------------------- Xia law --
def xia_angles(m1, m2):
    """Weighted-Fermat (Xia, alpha=1/2) junction angles for a merge of two
    upstream loads m1, m2 into a downstream m0 = m1 + m2 (design-v4.md
    #5). Returns (turn1_deg, turn2_deg, theta12_deg): turn_i is child i's
    turn angle OFF THE PARENT CONTINUATION (the upward extrapolation of
    the downstream/sinkward edge past the junction); theta12 is the angle
    between the two children (always 90 deg at alpha=1/2, independent of
    the load split -- the load-dependence lives entirely in how the 90
    degrees is split between turn1 and turn2).

    Closed form at alpha=1/2 (k_i = m_i/m0 = (m_i/m0)^(2*alpha)):
        turn_i = arccos(sqrt(k_i))
    (re-derived from Xia's cos-theta-to-sinkward formula; turn_i =
    180 - theta_i since "parent continuation" is measured from the
    upward/anti-sinkward extrapolation, opposite the sinkward unit
    vector theta_i is defined against in design-v4.md #5.) turn1+turn2
    always sums to exactly 90 deg -- the closed-form identity the design
    calls out and test_cco.py's oracle test confirms numerically.
    """
    m0 = float(m1) + float(m2)
    if m0 <= 0:
        return 45.0, 45.0, 90.0
    k1 = max(0.0, min(1.0, float(m1) / m0))
    k2 = max(0.0, min(1.0, float(m2) / m0))
    turn1 = float(np.degrees(np.arccos(np.clip(np.sqrt(k1), -1.0, 1.0))))
    turn2 = float(np.degrees(np.arccos(np.clip(np.sqrt(k2), -1.0, 1.0))))
    return turn1, turn2, 90.0


def _weighted_fermat_point(targets_weights, j0, n_iter=12):
    """Weiszfeld iteration toward the point J minimizing sum(w_i * |J -
    T_i|) -- the weighted Fermat-Torricelli point. This is the exact
    objective Xia's cos-theta closed form gives the OPTIMALITY CONDITIONS
    for (design-v4.md #5): at alpha=1/2 the edge weight is load^alpha =
    sqrt(load), so passing w_i = sqrt(m_i) here and iterating converges
    to the same junction the closed-form angles describe, without
    needing the circle-intersection construction to get there directly.
    `j0` seeds the iteration (a good seed matters -- Weiszfeld can be slow
    far from the optimum); a small fixed iteration count is a deliberate
    Phase 2 approximation (design-v4.md #14 assigns full numeric polish
    to Phase 3's rewire.py) -- this already moves J off the naive
    projection toward the true optimum, it just isn't run to
    convergence."""
    J = np.asarray(j0, float).copy()
    for _ in range(n_iter):
        num = np.zeros(3)
        den = 0.0
        degenerate = False
        for T, w in targets_weights:
            d = float(np.linalg.norm(J - T))
            if d < 1e-6:
                degenerate = True
                break
            num += w * T / d
            den += w / d
        if degenerate or den <= 1e-12:
            break
        J = num / den
    return J


def _theta_max_deg(r, cfg):
    """Printability cone half-angle for a member of (worst-case) radius r
    -- same rule tree.printability_report checks (design-v4.md #3)."""
    return cfg.branch_angle_deg if r >= cfg.min_shoot_r_mm else cfg.max_segment_angle_deg


def _lean_deg(p0, p1):
    d = np.asarray(p1, float) - np.asarray(p0, float)
    L = float(np.linalg.norm(d))
    if L < 1e-9:
        return 0.0
    return float(np.degrees(np.arctan2(np.linalg.norm(d[:2]), abs(d[2]))))


def _within_cone(p0, p1, r, cfg, tol_deg=1e-6):
    """tol_deg is deliberately near-zero (not a generous slack): the
    verify-side gate (tree.printability_report) re-checks the SAME cone
    rule with ~1e-9 tolerance in tan-space at final (possibly
    Murray-fattened) radii. A looser builder-side tolerance here would
    let the builder accept members the verify gate then rejects --
    exactly backwards (a verify gate must never be weaker than the
    thing that's supposed to satisfy it)."""
    return _lean_deg(p0, p1) <= _theta_max_deg(r, cfg) + tol_deg


def _member_vol_ml(p0, p1, r0, r1):
    L = float(np.linalg.norm(np.asarray(p1, float) - np.asarray(p0, float)))
    return np.pi / 3.0 * L * (r0 ** 2 + r0 * r1 + r1 ** 2) / 1000.0


def _r_tension_mm(carried_mm2, cfg):
    """Load-conservation (Murray) floor -- same form as
    placement.tip_radius_mm (design-v4.md #6 explicitly requires reusing
    that function's math/constants), WITHOUT the tip-only removal-
    friendly clamp: a trunk/shoot carrying real accumulated load needs to
    be sized by what it carries, not clamped to the 0.2-1.0mm tip band."""
    area_m2 = max(float(carried_mm2), 0.0) * 1e-6
    force_n = area_m2 * cfg.peel_pressure_pa
    tensile_pa = cfg.tensile_green_mpa * 1e6
    r_m = np.sqrt(max(force_n, 0.0) * cfg.tip_safety_factor / (np.pi * tensile_pa))
    return float(r_m * 1000.0)


def brace_estimate_ml(L2, cfg):
    """Phase 4c (SPEC-INTEGRATION #1): deterministic estimate of the brace
    volume a plate-descent column of height L2 will need once the bracing
    pass (bracing.propose_and_place) actually restrains it -- used ONLY to
    PRICE a plate-descent candidate in _plate_descent_builders (the
    measured trap: pricing a slender braced column without also pricing
    the braces that make it legal makes the ranking prefer a column that
    then needs MORE total volume once bracing lands, table 1.57ml vs
    0.845ml). Expected rung count n = floor(L2 / brace_free_span_mm)
    (v3's brace cadence); each rung priced as an X-pair at the assumed
    worst-case neighbor distance d = brace_neighbor_max_xy_mm / 2 (the
    same rod geometry bracing._cross_link_rods builds: rise = d, rod_len =
    d*sqrt(2), X-pair discount 0.5**0.25), floored by cfg.brace_r_mm same
    as the real placer."""
    n = max(0, int(L2 / cfg.brace_free_span_mm))
    if n == 0:
        return 0.0
    d = cfg.brace_neighbor_max_xy_mm / 2.0
    rod_len = d * np.sqrt(2.0)
    r_raw = placement._shaft_r_for_L(rod_len, cfg, cfg.max_segment_angle_deg)
    rod_r = max(cfg.brace_r_mm, r_raw * 0.5 ** 0.25)
    rung_ml = 2 * np.pi * rod_r ** 2 * rod_len / 1000.0
    return n * rung_ml


# ---------------------------------------------------------------- builder --
def build(mesh, contacts, tip_r, carried, minima_flags, plate_z, surface_tree, cfg,
          sample_points=None, sample_delicacy=None, query_fn=None):
    """Insert every contact's terminal into a Forest via CCO. Returns
    (forest, unrouted) where unrouted is a sorted list of contact indices
    that found no legal candidate (honest failure, design-v4.md #4)."""
    forest = tree.Forest()
    n = len(contacts)
    stub_ends = [None] * n
    for i, c in enumerate(contacts):
        pos = np.asarray(c["position"], float)
        normal = np.asarray(c["normal"], float)
        normal = normal / (np.linalg.norm(normal) or 1.0)
        p_tip = pos - normal * cfg.contact_depth_mm
        p_stub_end = pos + normal * cfg.stub_length_mm
        r_tip = float(tip_r[i])
        r_shoot_floor = max(r_tip, cfg.min_shoot_r_mm)
        stub = tree.Member(p0=p_tip, p1=p_stub_end, r0=r_tip, r1=r_shoot_floor,
                            kind="stub", owner=("contact", i), embed_end="p0",
                            carried_mm2=float(carried[i]))
        idx = forest.add_member(stub)
        forest.terminal_of[i] = idx
        stub_ends[i] = p_stub_end

    order1 = sorted((i for i in range(n) if minima_flags[i]),
                     key=lambda i: (float(stub_ends[i][2]), -float(carried[i]), i))
    order2 = sorted((i for i in range(n) if not minima_flags[i]),
                     key=lambda i: (-float(stub_ends[i][2]), -float(carried[i]), i))

    kdc = _KDCache()
    unrouted = []
    n_inserted = 0
    for i in order1 + order2:
        ok = _insert_terminal(forest, i, contacts, tip_r, carried, stub_ends, cfg,
                               plate_z, surface_tree, mesh, kdc,
                               sample_points, sample_delicacy, query_fn)
        if not ok:
            unrouted.append(i)
        n_inserted += 1
        if n_inserted % KDTREE_REBUILD_EVERY == 0:
            kdc.rebuild(forest)
    return forest, sorted(unrouted)


class _KDCache:
    """cKDTree over member midpoint+endpoint samples for the bifurcation
    candidate search (design-v4.md #4), rebuilt lazily every
    KDTREE_REBUILD_EVERY insertions. Between rebuilds, members added since
    the last rebuild are invisible to the cached tree -- nearest_members
    always additionally brute-forces that (small, bounded by the rebuild
    cadence) tail so a fresh terminal is never simply invisible to
    bifurcation search until the next rebuild."""

    def __init__(self):
        self.tree = None
        self.owners = None
        self.indexed_count = 0

    def rebuild(self, forest):
        pts, owners = [], []
        for mi, m in enumerate(forest.members):
            if m.kind == "stub":
                continue
            pts.append(m.p0); owners.append(mi)
            pts.append(m.p1); owners.append(mi)
            pts.append((m.p0 + m.p1) / 2.0); owners.append(mi)
        self.indexed_count = len(forest.members)
        if not pts:
            self.tree = None
            self.owners = None
            return
        self.tree = cKDTree(np.array(pts))
        self.owners = np.array(owners)

    def nearest_members(self, point, k, forest, exclude=None):
        """`exclude` (Phase 3 addition, design-v4.md #7 "refactor
        minimally to expose"): an optional set/frozenset of member
        indices to skip -- rewire.py's subtree-reattach move needs this
        to keep a detached subtree from being offered its own descendant
        as a new host (a cycle); cco.build never passes it, so the
        default (None -> no exclusion) reproduces the exact Phase 2
        candidate set, zero behavior change to insertion."""
        point = np.asarray(point, float)
        exclude = exclude or ()
        scored = {}
        if self.tree is not None:
            k_samples = min(len(self.owners), max(k * 6, 12))
            dist, idx = self.tree.query(point, k=k_samples)
            for dd, ii in zip(np.atleast_1d(dist), np.atleast_1d(idx)):
                if ii >= len(self.owners):
                    continue
                mi = int(self.owners[ii])
                if mi in exclude:
                    continue
                dd = float(dd)
                if mi not in scored or dd < scored[mi]:
                    scored[mi] = dd
        for mi in range(self.indexed_count, len(forest.members)):
            if mi in exclude:
                continue
            m = forest.members[mi]
            if m.kind == "stub":
                continue
            d = float(np.linalg.norm((m.p0 + m.p1) / 2.0 - point))
            if mi not in scored or d < scored[mi]:
                scored[mi] = d
        if not scored:
            return []
        ranked = sorted(scored.items(), key=lambda kv: (kv[1], kv[0]))
        return [mi for mi, _ in ranked[:k]]


def _insert_terminal(forest, i, contacts, tip_r, carried, stub_ends, cfg,
                      plate_z, surface_tree, mesh, kdc,
                      sample_points, sample_delicacy, query_fn):
    stub_idx = forest.terminal_of[i]
    se = np.asarray(stub_ends[i], float)
    r_i = max(float(tip_r[i]), cfg.min_shoot_r_mm)
    carried_i = float(carried[i])

    builders = list(_candidate_builders(
        forest, i, stub_idx, se, r_i, carried_i, contacts, tip_r, carried,
        plate_z, surface_tree, mesh, cfg, kdc, sample_points, sample_delicacy, query_fn))

    scored = []
    for build_fn in builders:
        cp = forest.checkpoint()
        try:
            delta = build_fn()
        except _InfeasibleCandidate:
            forest.rollback(cp)
            continue
        violations = forest.revalidate(surface_tree, cfg)
        forest.rollback(cp)
        if violations:
            continue
        scored.append((delta, build_fn))

    if not scored:
        return False

    scored.sort(key=lambda t: t[0])
    _, best_fn = scored[0]
    cp = forest.checkpoint()
    best_fn()
    violations = forest.revalidate(surface_tree, cfg)
    assert not violations, (
        "candidate re-application produced a different clearance result "
        "(non-determinism in the builder) -- crash over corruption")
    forest.commit(cp)
    return True


class _InfeasibleCandidate(Exception):
    """Raised by a candidate build_fn when it discovers (during
    construction, before any clearance check) that its geometry can't
    satisfy a hard constraint (e.g. the cone). Caught by the trial loop,
    which rolls back and simply doesn't score that candidate."""


def _candidate_builders(forest, i, stub_idx, se, r_i, carried_i, contacts, tip_r,
                         carried, plate_z, surface_tree, mesh, cfg, kdc,
                         sample_points, sample_delicacy, query_fn):
    yield from _plate_descent_builders(forest, i, stub_idx, se, r_i, carried_i, plate_z, cfg)
    yield from _model_anchor_builders(forest, i, stub_idx, se, r_i, carried_i,
                                       sample_points, sample_delicacy, query_fn, cfg,
                                       geom=surface_tree)
    yield from _bifurcation_builders(forest, i, stub_idx, se, r_i, carried_i,
                                      plate_z, cfg, kdc)


# ---------------------------------------------------------- (a) plate descent --
def _diagonal_top_z(se, xy, theta_max_deg):
    dxy = float(np.linalg.norm(np.asarray(xy, float) - se[:2]))
    drop = dxy / max(np.tan(np.radians(theta_max_deg)), 1e-9)
    return se[2] - drop, dxy


def _plate_descent_builders(forest, i, stub_idx, se, r_i, carried_i, plate_z, cfg):
    theta = cfg.branch_angle_deg  # trunk-search diagonals run at the widened family

    def _make(xy_top, z_top):
        def build():
            p_top = np.array([xy_top[0], xy_top[1], z_top], dtype=float)
            p_bot = np.array([xy_top[0], xy_top[1], plate_z], dtype=float)
            L1 = float(np.linalg.norm(p_top - se))
            L2 = float(z_top - plate_z)
            if L2 <= 1e-6:
                raise _InfeasibleCandidate()
            r_load = max(_r_tension_mm(carried_i, cfg), cfg.min_trunk_r_mm)
            r_col = max(r_load, placement._shaft_r_for_L(L2, cfg, 0.0), cfg.min_trunk_r_mm)
            diag_vol = 0.0
            parent_for_col = stub_idx
            col_p0 = se.copy()
            if L1 > 1e-6:
                lean1 = _lean_deg(se, p_top)
                r_diag_defl = placement._shaft_r_for_L(L1, cfg, lean1)
                r_diag_r1 = max(r_i, r_col, r_diag_defl)
                if not _within_cone(se, p_top, r_diag_r1, cfg):
                    raise _InfeasibleCandidate()
                diag = tree.Member(p0=se.copy(), p1=p_top, r0=r_i, r1=r_diag_r1,
                                    kind="trunk", owner=("contact", i), embed_end=None,
                                    carried_mm2=carried_i)
                d_idx = forest.add_member(diag)
                forest.set_parent(stub_idx, d_idx)
                diag_vol += _member_vol_ml(diag.p0, diag.p1, diag.r0, diag.r1)
                parent_for_col = d_idx
                col_p0 = p_top
                r_col = max(r_col, r_diag_r1)
            # Honest built geometry: the column is still constructed at its
            # full-length deflection-solved radius (r_col) -- construction
            # never changes here; the bracing pass (engine.brace_and_size)
            # resizes it down for real once braces actually land (design-v4.md
            # #9). Nothing below this point changes what gets BUILT.
            col = tree.Member(p0=col_p0, p1=p_bot, r0=r_col, r1=r_col, kind="trunk",
                               owner=("contact", i), embed_end=None, carried_mm2=carried_i)
            c_idx = forest.add_member(col)
            forest.set_parent(parent_for_col, c_idx)

            # Phase 4c braced-cost PRICING ONLY (SPEC-INTEGRATION #1): the
            # RETURNED value (used purely to RANK this candidate against
            # others in _insert_terminal's scored list) prices the column as
            # if it will be braced down to brace_free_span_mm, plus the
            # estimated brace volume that bracing will actually need to add
            # to make that legal -- landed together with bracing/resize in
            # the SAME merge (measured trap: pricing alone without effective
            # bracing made the table fixture WORSE, 1.57ml vs 0.845ml,
            # because it steers insertion toward tall slender columns that
            # then still get built honestly fat and never come back down).
            r_col_priced = max(r_load,
                                placement._shaft_r_for_L(min(L2, cfg.brace_free_span_mm),
                                                          cfg, 0.0),
                                cfg.min_trunk_r_mm)
            column_vol_priced = _member_vol_ml(col.p0, col.p1, r_col_priced, r_col_priced)
            price = diag_vol + column_vol_priced + brace_estimate_ml(L2, cfg)
            return price
        return build

    yield _make(se[:2], se[2])  # straight down: zero-length diagonal, one column
    for radius in cfg.trunk_search_radii_mm:
        if radius == 0.0:
            continue
        for k in range(cfg.trunk_search_directions):
            ang = 2 * np.pi * k / cfg.trunk_search_directions
            xy = se[:2] + radius * np.array([np.cos(ang), np.sin(ang)])
            z_top, _dxy = _diagonal_top_z(se, xy, theta)
            if z_top <= plate_z + 1.0:
                continue
            yield _make(xy, z_top)


# ---------------------------------------------------------- (b) model anchor --
def _model_anchor_builders(forest, i, stub_idx, se, r_i, carried_i,
                            sample_points, sample_delicacy, query_fn, cfg, geom=None):
    """BUG 1 fix: yield one build_fn per cone-pre-filtered candidate anchor
    (struts.find_model_anchor_candidates, sorted nearest-first, capped
    ~40) instead of committing to a single globally-nearest point. Each
    candidate goes through cco's NORMAL insertion trial machinery in
    _insert_terminal (checkpoint, build, forest.revalidate, rollback,
    cheapest-clearance-surviving-candidate-wins) exactly like every other
    route family -- so a candidate that fails the builder's own cone
    check OR fails real clearance (e.g. its straight-line path clips
    other model geometry) is simply passed over in favor of the next one,
    rather than silently killing the whole model-anchor option for this
    contact.

    v5 exact-anchor swap (SPEC-V5-GEOM sec 2.3): `sample_points` is now
    the caller's exterior-face-centroid pool (perception.exterior_anchor_
    pool), not a seeded cloud -- struts.find_model_anchor_candidates
    itself is UNCHANGED, it just scores a different, exact-on-surface
    pool. When `geom` is a surface-EXACT oracle (duck-typed on
    `.cast_rays`), each surviving candidate is additionally made EXACT: a
    ray is cast from se toward the candidate; the anchor snaps to the
    FIRST crossing whose hit face is exterior (t in (0, 1+eps]) -- the
    strut foot lands on the TRUE surface intersection along that
    direction (which can sit nearer than the candidate centroid itself,
    if something else is in the way), not merely at a pool point. A
    candidate with no exterior hit along its own direction is discarded
    (SPEC: "no hit => candidate discarded" -- the builder-side half of
    the census's ball-breach fix; a thin-twig anchor a ball would
    breach is exactly the class this can also reject when its direction
    only grazes the model). `geom=None` (legacy / unit tests with a bare
    point pool) keeps the candidate point verbatim -- zero behavior
    change."""
    if sample_points is None or sample_delicacy is None:
        return
    contact = dict(position=se)
    candidates = struts.find_model_anchor_candidates(contact, sample_points,
                                                       sample_delicacy, cfg)
    if not candidates:
        return
    if geom is not None and hasattr(geom, "cast_rays"):
        ext = geom.exterior_face_mask()
        origins = np.broadcast_to(se, (len(candidates), 3))
        dirs = np.array([np.asarray(c[0], float) - se for c in candidates], dtype=float)
        hits = geom.cast_rays(origins, dirs, t_max=1.0)
        for (anchor_pt, _anchor_delicacy, _len), (ts, faces) in zip(candidates, hits):
            if len(ts) == 0:
                continue
            ext_sel = ext[faces]
            if not ext_sel.any():
                continue
            j = int(np.nonzero(ext_sel)[0][0])  # ts sorted ascending -> first exterior hit
            t_hit = float(ts[j])
            exact_pt = se + t_hit * (np.asarray(anchor_pt, float) - se)
            yield _make_model_anchor_builder(forest, i, stub_idx, se, r_i, carried_i,
                                              exact_pt, cfg, geom=geom)
        return
    for anchor_pt, _anchor_delicacy, _len in candidates:
        yield _make_model_anchor_builder(forest, i, stub_idx, se, r_i, carried_i,
                                          np.asarray(anchor_pt, float), cfg, geom=geom)


def _make_model_anchor_builder(forest, i, stub_idx, se, r_i, carried_i, anchor_pt, cfg,
                               geom=None):
    def build():
        L = float(np.linalg.norm(anchor_pt - se))
        if L <= 1e-6:
            raise _InfeasibleCandidate()
        r_load = max(_r_tension_mm(carried_i, cfg), cfg.min_shoot_r_mm)
        lean = _lean_deg(se, anchor_pt)
        r_defl = placement._shaft_r_for_L(L, cfg, lean)
        r_strut = max(r_i, r_load, r_defl, cfg.min_shoot_r_mm)
        # foot angle budget = theta_max of its own (60-class) radius
        if not _within_cone(se, anchor_pt, r_strut, cfg):
            raise _InfeasibleCandidate()
        # Builder-side ball-breach rejection (SPEC-V5-GEOM sec 2.2/2.3,
        # amendment 5): the strut foot embeds at anchor_pt (embed_end="p1"),
        # and joints.compute_joint_balls will adsorb a foot ball there sized
        # MODEL_FOOT_FACTOR (1.4x) the rod's end radius -- that LARGER ball,
        # not the bare rod radius, is what verify's ball_breach_report gates
        # on (the census's headline defect: a 1.4x foot ball engulfing a
        # sub-mm twig). Reject the anchor HERE, during candidate scoring,
        # against the ACTUAL emitted foot-ball radius, so the trial machinery
        # moves to the next candidate rather than committing a member whose
        # foot ball the gate would then fail. Cheap: one sphere_breach per
        # candidate. No-op for a non-exact geom (None / cKDTree lacks
        # sphere_breach) -- tests/legacy keep their behavior.
        if geom is not None and hasattr(geom, "sphere_breach"):
            r_foot_ball = joints.MODEL_FOOT_FACTOR * r_strut
            breached, _thick, _poke = geom.sphere_breach(
                np.asarray(anchor_pt, float)[None, :], np.array([r_foot_ball]))
            if bool(breached[0]):
                raise _InfeasibleCandidate()
        member = tree.Member(p0=se.copy(), p1=anchor_pt.copy(), r0=r_i, r1=r_strut,
                              kind="strut", owner=("contact", i), embed_end="p1",
                              carried_mm2=carried_i)
        idx = forest.add_member(member)
        forest.set_parent(stub_idx, idx)
        return _member_vol_ml(member.p0, member.p1, member.r0, member.r1)
    return build


# ---------------------------------------------------------- (c) bifurcation --
def _bifurcation_builders(forest, i, stub_idx, se, r_i, carried_i, plate_z, cfg, kdc,
                           exclude=None):
    hosts = kdc.nearest_members(se, 5, forest, exclude=exclude)
    for host_idx in hosts:
        yield _make_bifurcation_builder(forest, i, stub_idx, se, r_i, carried_i,
                                          host_idx, plate_z, cfg)


def _make_bifurcation_builder(forest, i, stub_idx, se, r_i, carried_i,
                                host_idx, plate_z, cfg):
    def build():
        host = forest.members[host_idx]
        h_p0, h_p1 = host.p0.copy(), host.p1.copy()
        h_r0, h_r1 = host.r0, host.r1
        d = h_p1 - h_p0
        h_len = float(np.linalg.norm(d))
        if h_len < 1e-6:
            raise _InfeasibleCandidate()

        # Xia loads: m1 = host's existing downstream load (child 1, the
        # "old continuation" piece), m2 = this terminal's load (child 2).
        m1 = max(host.carried_mm2, 1e-9)
        m2 = max(carried_i, 1e-9)
        w1, w0, w2 = m1 ** 0.5, (m1 + m2) ** 0.5, m2 ** 0.5

        # Initial guess: project the terminal onto the host segment
        # (clamped off the exact endpoints so both split halves start
        # non-degenerate), then Weiszfeld-refine toward the weighted
        # Fermat point of the three fixed neighbors {h_p0, h_p1, se}
        # weighted by their Xia edge weights -- the exact objective the
        # closed-form angles are optimality conditions for.
        t0 = float(np.dot(se - h_p0, d) / max(float(d @ d), 1e-12))
        t0 = min(max(t0, 0.03), 0.97)
        j0 = h_p0 + t0 * d
        J = _weighted_fermat_point([(h_p0, w1), (h_p1, w0), (se, w2)], j0)

        r_child_floor = max(r_i, cfg.min_shoot_r_mm)

        MIN_SPLIT_LEN_MM = 0.5  # guards against degenerate near-zero-length split pieces

        def _feasible(Jp):
            if Jp[2] >= se[2] - 1e-9:
                return False
            if (float(np.linalg.norm(Jp - h_p0)) < MIN_SPLIT_LEN_MM
                    or float(np.linalg.norm(Jp - h_p1)) < MIN_SPLIT_LEN_MM):
                return False
            if not _within_cone(se, Jp, r_child_floor, cfg):
                return False
            r_at_j = _radius_along(h_p0, h_p1, h_r0, h_r1, Jp)
            if not _within_cone(h_p0, Jp, max(h_r0, r_at_j), cfg):
                return False
            if not _within_cone(Jp, h_p1, max(r_at_j, h_r1), cfg):
                return False
            return True

        if not _feasible(J):
            # Fall back to the guaranteed-constructible projection: slide
            # the split point toward the host's root end (t -> 1) until
            # the new child edge clears its own cone -- "projected to
            # satisfy the cone" (design-v4.md #5) when the Fermat point
            # itself doesn't already sit in the feasible region.
            t = t0
            J = h_p0 + t * d
            steps = 0
            while not _feasible(J):
                t += 0.03
                if t > 0.97:
                    raise _InfeasibleCandidate()
                J = h_p0 + t * d
                steps += 1
                if steps > 40:
                    raise _InfeasibleCandidate()

        r_J = _radius_along(h_p0, h_p1, h_r0, h_r1, J)
        vol_before_host = _member_vol_ml(h_p0, h_p1, h_r0, h_r1)

        # host's shortened p0->J piece is a NEW restraint-to-restraint span
        # (design-v4.md #6) the instant the split creates junction J --
        # r_J above is only the OLD taper's interpolated value between
        # h_r0 (an exempt tip-continuity end, deliberately allowed to sit
        # under any structural floor) and h_r1 (floored for the FULL
        # original length). Linear interpolation toward the r0 end can
        # undershoot this shortened span's own (length-independent) kind
        # floor -- same class of bug as B1's child edge, found by the same
        # every-span-meets-its-own-floor test; re-floor it the same way
        # the other construction paths size their far end, never
        # decreasing below the interpolated value (floors are hard, never
        # clamped down -- design-v4.md #5). Both `upper`'s r0 and host's
        # r1 land on this SAME physical cross-section at J, so both must
        # get this SAME re-floored value -- computed once, before either
        # member is touched, to keep the junction's Murray check
        # (parent.r0**4 >= sum(child.r1**4)) consistent from the start.
        host_len = float(np.linalg.norm(J - h_p0))
        host_lean = _lean_deg(h_p0, J)
        host_kind_floor = cfg.min_trunk_r_mm if host.kind == "trunk" else cfg.min_shoot_r_mm
        host_r_defl = placement._shaft_r_for_L(host_len, cfg, host_lean)
        host_r_tension = _r_tension_mm(host.carried_mm2, cfg)
        r_junction = max(r_J, host_r_tension, host_r_defl, host_kind_floor)

        # --- mutate: host becomes the LOWER piece (p0 -> J); a new
        # UPPER piece (J -> old p1) inherits host's old parent; the new
        # terminal edge (se -> J) is the second child.
        old_parent = forest.parent[host_idx]
        upper = tree.Member(p0=J.copy(), p1=h_p1.copy(), r0=r_junction, r1=h_r1,
                             kind=host.kind, owner=host.owner, embed_end=host.embed_end,
                             carried_mm2=host.carried_mm2)
        upper_idx = forest.add_member(upper)
        forest.set_parent(upper_idx, old_parent)

        host.p1 = J.copy()
        host.r1 = r_junction
        if host.embed_end == "p1":
            host.embed_end = None  # that end moved onto `upper`
        forest.set_parent(host_idx, upper_idx)

        # B1 fix (verifier blocker): r_child_floor above is a bare
        # tip-vs-min-shoot-r clamp -- tip radius only, with none of
        # design-v4.md #6's r = max(r_load, r_deflect, kind floor). The
        # other three construction paths (plate-descent diag, model-anchor
        # strut) all size their far/junction end this way; the
        # bifurcation child edge must too. r0 stays r_i (the tip-taper
        # end, unchanged); only the J-facing end (r1) gets the real floor,
        # computed from THIS child edge's own final length/lean/load (not
        # the pre-clamp/search-time r_child_floor, which only ever fed the
        # cone-feasibility search above and is fine for that -- cone
        # threshold only depends on r >= min_shoot_r_mm, true for both).
        child_len = float(np.linalg.norm(J - se))
        child_lean_deg = _lean_deg(se, J)
        r_child_tension = _r_tension_mm(carried_i, cfg)
        r_child_defl = placement._shaft_r_for_L(child_len, cfg, child_lean_deg)
        r_child_r1 = max(r_i, r_child_tension, r_child_defl, cfg.min_shoot_r_mm)

        child = tree.Member(p0=se.copy(), p1=J.copy(), r0=r_i, r1=r_child_r1,
                             kind="shoot", owner=("contact", i), embed_end=None,
                             carried_mm2=carried_i)
        child_idx = forest.add_member(child)
        forest.set_parent(child_idx, upper_idx)
        forest.set_parent(stub_idx, child_idx)

        vol_after_host = (_member_vol_ml(host.p0, host.p1, host.r0, host.r1)
                          + _member_vol_ml(upper.p0, upper.p1, upper.r0, upper.r1))
        vol = (vol_after_host - vol_before_host
               + _member_vol_ml(child.p0, child.p1, child.r0, child.r1))

        # --- Murray propagation up the path upper_idx -> root, fattening
        # every ancestor member: r_new = (r_old^4 + r_child^4)^(1/4),
        # floored (never clamped down) by r_tension/r_deflect/kind-floors.
        # Propagate the child's REAL r1 (r_child_r1, matching the member
        # we just built), not the pre-floor r_child_floor.
        vol += _propagate_murray(forest, upper_idx, r_child_r1, carried_i, cfg)
        return vol
    return build


def _radius_along(p0, p1, r0, r1, point):
    """Linear-taper radius at the closest point on segment p0->p1 to
    `point` (used to read the host's own pre-fattening radius at a
    Fermat/projected junction that may not sit exactly ON the segment)."""
    d = p1 - p0
    L2 = float(d @ d)
    if L2 < 1e-12:
        return r0
    t = float(np.clip(np.dot(point - p0, d) / L2, 0.0, 1.0))
    return r0 + t * (r1 - r0)


def _propagate_murray(forest, start_idx, r_child, added_carried_mm2, cfg):
    """Fatten every member on the path start_idx -> root by Murray's law
    against the newly-added child radius, adding `added_carried_mm2` to
    each ancestor's cumulative carried load along the way and re-applying
    the r_tension/r_deflect/kind floors at the new load (design-v4.md
    #6). Ceilings (max_trunk_r_mm etc.) are soft -- never clamped down.
    Returns the net volume delta from every touched ancestor.

    B2 fix (verifier blocker): combine the newly-added child's radius
    against `m.r0`, NOT max(m.r0, m.r1). tree.Forest's documented flow
    convention (see its class docstring) is p1 = root-ward/exit end, p0 =
    terminal-ward/entry end -- a member flows INTO its parent at the
    PARENT's p0. verify.murray_report checks exactly this convention
    (`parent.r0**4 >= sum(child.r1**4)`): the end that receives its
    children's flow is r0, and that is what a newly-merged child's radius
    must combine against. `start_idx` (the first member visited, `upper`
    from the bifurcation that called this) was built with r0 already
    equal to the (already-floored) host radius at the junction
    (`r_junction`) -- exactly the "other branch" this new child merges
    with -- so m.r0 is the correct sibling value there. Using max(r0, r1)
    instead pulled in
    r1, an unrelated root-ward exit radius (e.g. from whatever this
    ancestor's OWN taper needed further up, nothing to do with this
    junction), inflating the result (verifier repro: r0=0.3/r1=1.2 + a
    r=0.5 child should combine to ~0.515mm; max(r0,r1) gave ~1.209mm,
    2.3x too fat). r_up is carried forward as m's own new (uniform)
    radius, which is consistently both its entry AND exit value once this
    loop sets m.r0 = m.r1 = r_new below -- so the same r0-only
    combination stays correct at every subsequent ancestor hop too."""
    vol = 0.0
    r_up = r_child
    idx = start_idx
    while idx is not None:
        m = forest.members[idx]
        vol_before = _member_vol_ml(m.p0, m.p1, m.r0, m.r1)
        m.carried_mm2 = m.carried_mm2 + added_carried_mm2
        r_murray = (m.r0 ** 4 + r_up ** 4) ** 0.25
        L = float(np.linalg.norm(m.p1 - m.p0))
        lean = _lean_deg(m.p0, m.p1)
        r_defl = placement._shaft_r_for_L(L, cfg, lean)
        r_tension = _r_tension_mm(m.carried_mm2, cfg)
        kind_floor = cfg.min_trunk_r_mm if m.kind == "trunk" else cfg.min_shoot_r_mm
        r_new = max(r_murray, r_defl, r_tension, kind_floor)
        m.r0 = r_new
        m.r1 = r_new
        vol += _member_vol_ml(m.p0, m.p1, m.r0, m.r1) - vol_before
        r_up = r_new
        idx = forest.parent[idx]
    return vol
