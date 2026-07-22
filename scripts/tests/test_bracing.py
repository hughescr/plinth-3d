import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import bracing, config, joints, placement, tree as tm
from support_placement import transient, verify


def _member(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("contact", 0), embed_end=None):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner, embed_end=embed_end)


def _far_tree():
    return cKDTree(np.array([[1e6, 1e6, 1e6]]))


def _column_forest(*columns):
    """columns: list of (xy, z0, z1) -- adds one vertical member per
    column, kind "trunk", returns (forest, [member_idx...])."""
    f = tm.Forest()
    idxs = []
    for xy, z0, z1 in columns:
        idxs.append(f.add_member(_member([xy[0], xy[1], z0], [xy[0], xy[1], z1])))
    return f, idxs


# ------------------------------------------------------------ compute_needs --
def test_compute_needs_pins_exact_rung_count_for_65mm_gap(cfg):
    """gap 65mm vertical: n_rungs == max(n_slender from _shaft_r_for_L,
    int(65/brace_free_span_mm)) -- pinned against the SAME formula the
    module uses, per SPEC-BRACING #5."""
    f, (mi,) = _column_forest(((0.0, 0.0), 0.0, 80.0))
    spans_report = {mi: dict(worst_span_mm=65.0, restraint_zs=[],
                              free_gaps=[(0.0, 65.0)], n_terminals=1)}

    def n_slender(gap):
        for n in range(51):
            r = placement._shaft_r_for_L(gap / (n + 1), cfg, 0.0)
            if r <= cfg.max_unbraced_shaft_r_mm:
                return n
        return 50

    expected = max(n_slender(65.0), int(65.0 / cfg.brace_free_span_mm))
    requests = bracing.compute_needs(f, spans_report, cfg)
    assert requests == [(mi, 0.0, 65.0, expected)]


def test_compute_needs_no_request_under_both_thresholds(cfg):
    """A gap short enough that neither the slenderness nor the build-time
    span trigger fires produces NO request."""
    f, (mi,) = _column_forest(((0.0, 0.0), 0.0, 20.0))
    spans_report = {mi: dict(worst_span_mm=10.0, restraint_zs=[],
                              free_gaps=[(0.0, 10.0)], n_terminals=1)}
    assert bracing.compute_needs(f, spans_report, cfg) == []


def test_compute_needs_lean_deg_threading_demands_more_rungs(cfg):
    """SPEC-BRACING should-fix #4 (non-vertical lean coverage):
    compute_needs threads cco._lean_deg(member) into _shaft_r_for_L for
    n_slender, but no existing test exercises a leaning member -- a
    45-degree member's angle-aware peel term (placement._shaft_r_for_L's
    tan(theta) load) inflates the required shaft radius at any given
    segment length relative to a vertical member of the same z-span, so
    it needs MORE rungs to bring that radius back under
    cfg.max_unbraced_shaft_r_mm. Both counts are pinned directly against
    placement._shaft_r_for_L, per the same pattern as
    test_compute_needs_pins_exact_rung_count_for_65mm_gap.

    Design decision (recorded, measured not assumed): under DEFAULT cfg
    the build-time span trigger (n_span = gap // brace_free_span_mm)
    saturates n_rungs at BOTH 0 and 45 degrees for every gap swept up to
    400mm -- it would silently mask a broken/dropped lean value (e.g. a
    regression that hardcoded lean=0.0) behind an unrelated trigger. This
    test uses a locally-relaxed (effectively infinite) brace_free_span_mm
    to isolate the slenderness trigger the lean value actually feeds,
    exactly the kind of one-field cfg override test_model_rung_lone_
    column_near_surface_wall already established as house style."""
    local_cfg = config.SupportConfig(brace_free_span_mm=1.0e6)
    gap = 40.0

    def n_slender(g, lean_deg):
        for n in range(51):
            r = placement._shaft_r_for_L(g / (n + 1), local_cfg, lean_deg)
            if r <= local_cfg.max_unbraced_shaft_r_mm:
                return n
        return 50

    expected_vertical = n_slender(gap, 0.0)
    expected_45 = n_slender(gap, 45.0)
    assert expected_45 > expected_vertical  # sanity: the angle genuinely bites

    f = tm.Forest()
    # Equal xy-run and z-rise over the member's length -> exactly 45
    # degrees off vertical (cco._lean_deg == atan2(dxy, dz)).
    leaning = f.add_member(_member([0.0, 0.0, 0.0], [gap, 0.0, gap]))
    vertical = f.add_member(_member([100.0, 0.0, 0.0], [100.0, 0.0, gap]))
    spans_report = {
        leaning: dict(worst_span_mm=gap, restraint_zs=[],
                      free_gaps=[(0.0, gap)], n_terminals=1),
        vertical: dict(worst_span_mm=gap, restraint_zs=[],
                       free_gaps=[(0.0, gap)], n_terminals=1),
    }
    requests = bracing.compute_needs(f, spans_report, local_cfg)
    by_member = {r[0]: r[3] for r in requests}
    assert by_member[leaning] == expected_45
    assert by_member[vertical] == expected_vertical
    assert by_member[leaning] > by_member[vertical]


def test_compute_needs_sorted_by_member_then_z_lo(cfg):
    f, (m0, m1) = _column_forest(((0.0, 0.0), 0.0, 80.0), ((20.0, 0.0), 0.0, 80.0))
    spans_report = {
        m1: dict(worst_span_mm=65.0, restraint_zs=[], free_gaps=[(0.0, 65.0)], n_terminals=1),
        m0: dict(worst_span_mm=65.0, restraint_zs=[],
                  free_gaps=[(40.0, 65.0), (0.0, 30.0)], n_terminals=2),
    }
    requests = bracing.compute_needs(f, spans_report, cfg)
    idx_pairs = [(r[0], r[1]) for r in requests]
    assert idx_pairs == sorted(idx_pairs)
    assert idx_pairs[0] == (m0, 0.0)
    assert idx_pairs[1] == (m0, 40.0)
    assert idx_pairs[2] == (m1, 0.0)


# --------------------------------------------- activation-aware trigger -----
def test_compute_needs_activation_trigger_fires_on_worst_span_despite_tiny_static_gap(cfg):
    """SPEC-ARM-FIX fix A (Phase 4d, design-v4.md #9's literal rule): a
    pass-through member's own STATIC free_gap can be tiny (no internal
    restraint points on its own extent) while its worst_span_mm --
    transient.transient_report's activation-aware moment-arm value,
    inherited from a lagged restraint elsewhere on its root->terminal
    path -- is large enough to demand bracing on its own. compute_needs
    must catch this even though the static (gap-only) trigger alone
    requests nothing for this member."""
    f, (mi,) = _column_forest(((0.0, 0.0), 0.0, 5.0))
    worst_span_mm = 80.0
    n_terminals = 1
    lean = 0.0

    # sanity: the STATIC trigger alone (worst_span_mm == 0) requests
    # nothing for this member -- span 5.0 is far under both thresholds.
    static_only = {mi: dict(worst_span_mm=0.0, restraint_zs=[],
                             free_gaps=[(0.0, 5.0)], n_terminals=n_terminals)}
    assert bracing.compute_needs(f, static_only, cfg) == []

    r_trans = transient.r_transient_mm(worst_span_mm, lean, n_terminals, cfg)
    assert (r_trans > cfg.max_unbraced_shaft_r_mm) or (worst_span_mm > cfg.brace_free_span_mm)

    def n_slender(gap):
        for n in range(51):
            r = placement._shaft_r_for_L(gap / (n + 1), cfg, lean)
            if r <= cfg.max_unbraced_shaft_r_mm:
                return n
        return 50

    n_activation = max(n_slender(worst_span_mm), int(worst_span_mm / cfg.brace_free_span_mm))
    expected = bracing._rungs_that_fit(5.0, n_activation)
    assert expected >= 1

    spans_report = {mi: dict(worst_span_mm=worst_span_mm, restraint_zs=[],
                              free_gaps=[(0.0, 5.0)], n_terminals=n_terminals)}
    requests = bracing.compute_needs(f, spans_report, cfg)
    assert requests == [(mi, 0.0, 5.0, expected)]


def test_compute_needs_activation_trigger_merges_with_static_not_duplicates(cfg):
    """Dedup rule (SPEC-ARM-FIX fix A): when the activation trigger's
    target (the member's LARGEST free gap) is the SAME gap the static
    trigger already produced a request for, the two merge into ONE
    request (max of the two counts) -- never a second, duplicate tuple
    for the same (member_idx, z_lo, z_hi) height band (propose_and_place
    would otherwise place overlapping rungs at different offsets within
    the identical band)."""
    f, (mi,) = _column_forest(((0.0, 0.0), 0.0, 20.0))
    worst_span_mm = 80.0
    lean = 0.0

    def n_slender(gap):
        for n in range(51):
            r = placement._shaft_r_for_L(gap / (n + 1), cfg, lean)
            if r <= cfg.max_unbraced_shaft_r_mm:
                return n
        return 50

    n_static = max(n_slender(20.0), int(20.0 / cfg.brace_free_span_mm))
    n_activation_raw = max(n_slender(worst_span_mm), int(worst_span_mm / cfg.brace_free_span_mm))
    n_activation = bracing._rungs_that_fit(20.0, n_activation_raw)
    expected = max(n_static, n_activation)
    assert n_static >= 1  # sanity: the static trigger already fires on this gap
    assert expected > n_static  # sanity: the activation trigger genuinely adds MORE

    spans_report = {mi: dict(worst_span_mm=worst_span_mm, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    requests = bracing.compute_needs(f, spans_report, cfg)
    # exactly ONE entry for this member/gap, merged to the larger count.
    assert requests == [(mi, 0.0, 20.0, expected)]


def test_compute_needs_activation_trigger_capped_by_gap_fit(cfg):
    """The activation trigger's rung count is capped by what physically
    fits in the target gap at >= 1mm spacing (_rungs_that_fit) -- an
    enormous worst_span_mm must not demand more rungs than a short gap
    can actually hold."""
    f, (mi,) = _column_forest(((0.0, 0.0), 0.0, 3.0))
    spans_report = {mi: dict(worst_span_mm=500.0, restraint_zs=[],
                              free_gaps=[(0.0, 3.0)], n_terminals=1)}
    requests = bracing.compute_needs(f, spans_report, cfg)
    # gap_span=3.0 -> floor(3-1)=2 max rungs fit at >=1mm spacing.
    assert requests == [(mi, 0.0, 3.0, 2)]


def test_compute_needs_activation_trigger_absent_when_neither_condition_holds(cfg):
    """Control: a worst_span_mm that clears BOTH the r_transient
    threshold and the brace_free_span_mm cadence must not fire the
    activation trigger at all."""
    f, (mi,) = _column_forest(((0.0, 0.0), 0.0, 5.0))
    worst_span_mm = 5.0  # tiny -- well under brace_free_span_mm (18.0)
    r_trans = transient.r_transient_mm(worst_span_mm, 0.0, 1, cfg)
    assert r_trans <= cfg.max_unbraced_shaft_r_mm
    assert worst_span_mm <= cfg.brace_free_span_mm
    spans_report = {mi: dict(worst_span_mm=worst_span_mm, restraint_zs=[],
                              free_gaps=[(0.0, 5.0)], n_terminals=1)}
    assert bracing.compute_needs(f, spans_report, cfg) == []


# ---------------------------------------------------------------- X-pair ----
def test_xpair_geometry_pin_wider_columns(cfg):
    """Two vertical columns 8mm apart (v3's
    test_wider_trunk_pair_still_gets_crossed_pair, adapted): 2 rods,
    rise == 8.0 exactly, endpoints on both axes, discounted radius.

    Should-fix #3 (cross-link connectivity assertion): columns are sized
    to exactly z+rise (18.0mm) rather than an arbitrarily-tall 40mm --
    each column's own tip then coincides with the OTHER rod's landing
    point (rod1 lands on N at 18, rod2 lands on M at 18), so the whole
    forest (both hosts, both braces) certifies connectivity-clean with
    no filtering: this is the gate that would have caught the r1
    floating-brace blocker."""
    d = 8.0
    assert d > cfg.twin_single_brace_xy_mm
    plate_z = 0.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 18.0), ((d, 0.0), 0.0, 18.0))
    # gap sized to yield exactly n_rungs == 1 (z_lo=0, z_hi=20 -> z=10)
    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    surface_tree = _far_tree()

    result = bracing.propose_and_place(f, surface_tree, plate_z=plate_z, cfg=cfg,
                                        spans_report=spans_report)
    assert result["rungs_placed"] == 1
    assert result["rungs_unplaceable"] == 0
    assert result["by_kind"] == {"brace": 2}
    assert len(result["placed"]) == 2

    rod_len = d * np.sqrt(2.0)
    expected_r = max(cfg.brace_r_mm,
                      placement._shaft_r_for_L(rod_len, cfg, cfg.max_segment_angle_deg)
                      * 0.5 ** 0.25)
    rods = [f.members[idx] for idx in result["placed"]]
    for rod in rods:
        assert rod.kind == "brace"
        assert abs(float(rod.p1[2] - rod.p0[2])) == pytest.approx(8.0)
        assert rod.r0 == pytest.approx(expected_r)
        assert rod.r1 == pytest.approx(expected_r)
    # endpoints on both axes: one rod M->N, one rod N->M
    p0_xs = sorted(round(float(r.p0[0]), 6) for r in rods)
    assert p0_xs == [0.0, 8.0]
    for rod in rods:
        if rod.p0[0] == pytest.approx(0.0):
            assert rod.p1[0] == pytest.approx(d)
            assert rod.p0[2] == pytest.approx(10.0)
            assert rod.p1[2] == pytest.approx(18.0)
        else:
            assert rod.p1[0] == pytest.approx(0.0)
            assert rod.p0[2] == pytest.approx(10.0)
            assert rod.p1[2] == pytest.approx(18.0)

    members = joints.collect_members(list(f.members))
    assert verify.connectivity_report(members, plate_z, cfg, surface_tree=surface_tree) == []


def test_xpair_rods_pass_printability_report(cfg):
    from support_placement import tree as tree_mod
    d = 8.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    bracing.propose_and_place(f, _far_tree(), plate_z=0.0, cfg=cfg, spans_report=spans_report)
    assert tree_mod.printability_report(f.members, cfg) == []


# ----------------------------------------------------------------- twin -----
def test_twin_geometry_pin_close_columns(cfg):
    """Columns 2.5mm apart (v3's
    test_twin_trunk_pair_gets_single_alternating_rods, adapted): ONE rod
    per rung, alternating p0 side by rung parity, UNdiscounted radius.

    Should-fix #3 (cross-link connectivity assertion): with 2 alternating
    rungs, N (the neighbor) needs its z-extent to cover BOTH rungs'
    [z, z+rise] unconditionally (rung 0's [12, 14.5] and rung 1's
    [24, 26.5]) regardless of parity -- so N's column can't simply be
    trimmed to its rung-0 landing height the way the X-pair test trims
    to a single shared landing height. M's tip DOES land naturally
    (rung 1, odd-parity, climbs N->M and lands on M at 26.5, so M is
    sized to exactly that). N's tip has no natural landing at all in
    this 2-rung fixture -- it is given a designed embed_end="p1" contact
    with a real nearby surface point instead (a realistic stand-in for
    "this trunk terminates against the model", the same role
    test_model_rung_lone_column_near_surface_wall's rung plays for its
    own p1). That test's own comment already measured (not assumed) that
    revalidate's clearance inset is tighter than standoff_mm+brace_r_mm
    for ANY embed_end="p1" endpoint under default cfg; the SAME
    locally-relaxed standoff isolates that separate, pre-existing
    tension here too, so the whole forest (both hosts, both braces)
    certifies connectivity-clean with no filtering -- the gate that
    would have caught the r1 floating-brace blocker."""
    d = 2.5
    assert d <= cfg.twin_single_brace_xy_mm
    local_cfg = config.SupportConfig(standoff_mm=0.05)
    plate_z = 0.0
    # M: natural top landing (rung 1 lands on M at z+rise=26.5) -> sized
    # to exactly that height, no synthetic embed needed.
    # N: must reach 26.5 too (unconditional neighbor-extent requirement,
    # both rungs) but nothing lands exactly at its tip -> designed embed.
    f = tm.Forest()
    mi = f.add_member(_member([0.0, 0.0, 0.0], [0.0, 0.0, 26.5]))
    ni = f.add_member(_member([d, 0.0, 0.0], [d, 0.0, 26.5], embed_end="p1"))
    surface_tree = cKDTree(np.array([[d + 0.5, 0.0, 26.5]]))
    # gap sized to yield exactly n_rungs == 2 (z_lo=0, z_hi=36 -> z=12, 24)
    spans_report = {mi: dict(worst_span_mm=36.0, restraint_zs=[],
                              free_gaps=[(0.0, 36.0)], n_terminals=1)}

    result = bracing.propose_and_place(f, surface_tree, plate_z=plate_z, cfg=local_cfg,
                                        spans_report=spans_report)
    assert result["rungs_placed"] == 2
    assert result["rungs_unplaceable"] == 0
    assert result["by_kind"] == {"brace": 2}  # one rod per rung, two rungs
    assert len(result["placed"]) == 2

    rod_len = d * np.sqrt(2.0)
    expected_r = max(local_cfg.brace_r_mm,
                      placement._shaft_r_for_L(rod_len, local_cfg, local_cfg.max_segment_angle_deg))
    rods = [f.members[idx] for idx in result["placed"]]
    assert [round(float(r.p0[2]), 6) for r in rods] == [12.0, 24.0]
    for rod in rods:
        assert rod.r0 == pytest.approx(expected_r)
        assert abs(float(rod.p1[2] - rod.p0[2])) == pytest.approx(d)
    # alternating p0 side: rung 0 (even) starts at M's xy (0.0), rung 1
    # (odd) starts at N's xy (2.5)
    assert rods[0].p0[0] == pytest.approx(0.0)
    assert rods[1].p0[0] == pytest.approx(d)

    members = joints.collect_members(list(f.members))
    assert verify.connectivity_report(members, plate_z, local_cfg,
                                       surface_tree=surface_tree) == []


# ------------------------------------------------------- floating endpoint --
def test_cross_link_candidate_rejected_when_M_real_height_short_of_rise(cfg):
    """Verifier blocker #1: _find_cross_link_candidates validated N's
    z-extent covers [z, z+rise] but never M's OWN z-extent -- letting a
    neighbor clear even when M's real geometry doesn't reach z+rise, so
    the X-pair/twin-odd rod that reads M's axis AT z+rise
    (_axis_point_at_z is an unclamped linear parametrization) floats
    above M's real tip instead of touching it. M's real height here
    (14) is shorter than z+rise (9+8=17) even though the rung height
    z=9 itself is well within M -- exactly the stale-spans_report
    scenario connectivity_report's own docstring names
    ("stale-brace-leak"): the candidate must not clear."""
    d = 8.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 14.0), ((d, 0.0), 0.0, 40.0))
    z = 9.0
    # k is irrelevant here: d=8.0 is an X-pair (> twin_single_brace_xy_mm),
    # which always needs M's full [z, z+rise] regardless of rung parity.
    assert bracing._find_cross_link_candidates(f, mi, z, cfg, k=0) == []


def test_propose_and_place_rejects_rung_when_M_shorter_than_climb(cfg):
    """Integration-level repro of blocker #1: unfixed code places an
    X-pair whose mirrored rod's far endpoint sits above M's real
    physical tip -- a valid certificate (dirty_members()==[]) for a
    member disconnected from its host, corruption not a crash. Fixed
    code must reject the only candidate (M doesn't reach z+rise) and,
    with no other neighbor and no reachable model-surface anchor, count
    the rung unplaceable -- never place a floating brace."""
    d = 8.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 14.0), ((d, 0.0), 0.0, 40.0))
    surface_tree = _far_tree()
    # Certify the two columns clean first (mirrors
    # test_unclearable_rung_never_placed's pattern) so the pristine
    # assertion below isolates propose_and_place's OWN effect rather
    # than the base fixture's never-revalidated starting certificates.
    assert f.revalidate(surface_tree, cfg) == []
    assert f.dirty_members() == []
    n_members_before = len(f.members)
    spans_report = {mi: dict(worst_span_mm=18.0, restraint_zs=[],
                              free_gaps=[(0.0, 18.0)], n_terminals=1)}

    result = bracing.propose_and_place(f, surface_tree, plate_z=0.0, cfg=cfg,
                                        spans_report=spans_report)

    assert result["placed"] == []
    assert result["rungs_placed"] == 0
    assert result["rungs_unplaceable"] == 1
    assert len(f.members) == n_members_before
    assert f.braces == []
    assert f.dirty_members() == []


def test_twin_even_rung_placed_when_M_covers_z_but_not_climb(cfg):
    """Should-fix #1 (twin k-even over-rejection): the blocker #1 fix
    above made M's [z, z+rise] coverage unconditional, but a rod only
    ever LANDS on M at z+rise for an X-pair's mirrored rod or a twin
    ODD-parity rung -- an EVEN-parity twin rung climbs M (read at z, M's
    own real point) -> N (read at z+rise), never touching M above z.
    Requiring M to also cover z+rise over-rejects a perfectly valid
    even-parity candidate. M here covers z=10 (its real top, chosen so
    the rod's own p0 lands exactly there) but NOT z+rise=12.5 -- must
    now clear (was rejected), and connectivity_report on the placed rod
    is clean (the invariant from the blocker fix stays absolute: no
    endpoint here extrapolates past a host's real end -- M's own tip and
    N's own tip are chosen to exactly match the rod's landing points, so
    the WHOLE forest, not just the rod, certifies clean)."""
    d = 2.5
    assert d <= cfg.twin_single_brace_xy_mm
    plate_z = 0.0
    # M's real top is exactly z (10.0); N's real top is exactly z+rise
    # (12.5) -- both hosts' own tips coincide with the rod's own
    # endpoints, so nothing floats anywhere in the forest.
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 10.0), ((d, 0.0), 0.0, 12.5))
    z = 10.0
    rise = d
    assert bracing._find_cross_link_candidates(f, mi, z, cfg, k=0) == [(d, ni)]

    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    surface_tree = _far_tree()
    result = bracing.propose_and_place(f, surface_tree, plate_z=plate_z, cfg=cfg,
                                        spans_report=spans_report)

    assert result["rungs_placed"] == 1
    assert result["rungs_unplaceable"] == 0
    assert result["by_kind"] == {"brace": 1}
    assert len(result["placed"]) == 1
    rod = f.members[result["placed"][0]]
    assert rod.kind == "brace"
    assert np.allclose(rod.p0, [0.0, 0.0, z])          # M's own real axis point at z
    assert np.allclose(rod.p1, [d, 0.0, z + rise])     # N's own real axis point at z+rise

    members = joints.collect_members(list(f.members))
    assert verify.connectivity_report(members, plate_z, cfg, surface_tree=surface_tree) == []


# ------------------------------------------------------------- model rung ---
def test_model_rung_lone_column_near_surface_wall(cfg):
    """Lone column (no other tree member in reach) near a wall of
    surface points: horizontal rung, kind model_rung, embed_end p1,
    r == brace_r_mm, length <= model_brace_max_len_mm; passes
    printability_report + connectivity_report.

    Design decision (recorded -- measured, not assumed): tree.Forest.
    revalidate's clearance inset (cfg.contact_depth_mm+0.2 == 0.5mm by
    default) is smaller than cfg.standoff_mm (0.6mm) ALONE, before even
    adding cfg.brace_r_mm's own radius -- i.e. the sample immediately
    outside the inset is, by construction, closer to the embedded point
    than the standoff+radius budget requires, for ANY embed_end="p1"
    member of ANY kind other than "stub" (confirmed by direct probe
    against both a synthetic isolated point and a real dense
    routing.build_surface_tree(box_mesh) cloud -- this is a property of
    the frozen contact_depth_mm/standoff_mm/brace_r_mm constants, not of
    this test's geometry, and pre-dates this module: no existing test
    anywhere in the suite exercises embed_end="p1" through
    Forest.revalidate). Reproducing that gap is out of SPEC-BRACING's
    scope (tree.py's revalidate is sacred; config.py isn't ours), so
    this ONE test uses a locally-relaxed standoff to isolate and pin
    propose_and_place's OWN placement logic (kind/embed_end/radius/
    length/geometry) independent of that separate, pre-existing tension
    -- flagged for the charter owner, see this phase's structured output."""
    from support_placement import joints, tree as tree_mod

    local_cfg = config.SupportConfig(standoff_mm=0.05)
    plate_z = 0.0
    # M: plate-rooted column with a free (non-embedded) top -- only the
    # RUNG's own connectivity is asserted below, not M's (a single
    # isolated column's own far end has nothing else to attach to in
    # this minimal fixture; that's a fixture limitation, not something
    # bracing.py is responsible for).
    M = _member([0.0, 0.0, plate_z], [0.0, 0.0, 20.0], kind="trunk")
    f = tm.Forest()
    mi = f.add_member(M)

    pts = np.array([
        [3.0, 0.0, 10.0],    # nearest candidate anchor: distance 3.0
        [4.0, 1.0, 10.3],
        [3.2, -1.0, 9.7],
        [4.5, 2.0, 10.5],
    ])
    surface_tree = cKDTree(pts)

    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    result = bracing.propose_and_place(f, surface_tree, plate_z=plate_z, cfg=local_cfg,
                                        spans_report=spans_report)

    assert result["rungs_placed"] == 1
    assert result["rungs_unplaceable"] == 0
    assert result["by_kind"] == {"model_rung": 1}
    assert len(result["placed"]) == 1
    rung = f.members[result["placed"][0]]
    assert rung.kind == "model_rung"
    assert rung.embed_end == "p1"
    assert rung.r0 == pytest.approx(local_cfg.brace_r_mm)
    assert rung.r1 == pytest.approx(local_cfg.brace_r_mm)
    assert abs(float(rung.p1[2] - rung.p0[2])) < 1e-9  # exactly horizontal
    length = float(np.linalg.norm(rung.p1 - rung.p0))
    assert 1.0 <= length <= local_cfg.model_brace_max_len_mm + 1e-9

    members = joints.collect_members(f.members)
    assert tree_mod.printability_report(f.members, local_cfg) == []
    connectivity = verify.connectivity_report(members, plate_z, local_cfg,
                                               surface_tree=surface_tree)
    rung_violations = [v for v in connectivity if v["kind"] == "model_rung"]
    assert rung_violations == []


def test_model_rung_retries_next_nearest_anchor_when_nearest_too_short(cfg):
    """Should-fix #2 (model-rung retry): the original fallback tried only
    the single nearest anchor and gave up entirely if it failed the
    >=1.0mm minimum rod length (or clearance) -- even when a perfectly
    good next-nearest anchor was right there. Fixed code must retry
    qualifying candidates in deterministic (distance, index) order, up
    to 6 (v3's cadence), and place on the first one that actually
    clears.

    Design decision (recorded, same measured tension as
    test_model_rung_lone_column_near_surface_wall above): a
    locally-relaxed standoff is needed for ANY model_rung placement to
    clear its own embedded-end clearance under default cfg
    (contact_depth_mm+0.2 inset < standoff_mm+brace_r_mm). M's own
    radius is shrunk to 0.1mm so its bare (non-embedded) shaft clears
    the intentionally-close (0.5mm offset) degenerate anchor point --
    both isolate the RETRY logic itself from that separate,
    pre-existing tension rather than depending on it."""
    local_cfg = config.SupportConfig(standoff_mm=0.05)
    plate_z = 0.0
    M = _member([0.0, 0.0, plate_z], [0.0, 0.0, 20.0], r0=0.1, r1=0.1, kind="trunk")
    f = tm.Forest()
    mi = f.add_member(M)

    # Rung target z == 10.0 (see spans_report below). Both anchors sit
    # within 0.75mm of z in their OWN z (so both qualify) but on
    # opposite sides of it, so the nearest-by-3D-distance-to-M-at-z
    # ordering below matches the nearest-by-own-rod-length ordering too.
    pts = np.array([
        [0.5, 0.0, 10.7],   # nearest (0.86mm to M-at-z=10): own rod
                             # length is the pure xy offset (0.5mm) --
                             # degenerate-short, below the 1.0mm floor.
        [2.0, 0.0, 9.3],    # second-nearest (2.12mm): own rod length
                             # 2.0mm, valid.
    ])
    surface_tree = cKDTree(pts)
    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}

    result = bracing.propose_and_place(f, surface_tree, plate_z=plate_z, cfg=local_cfg,
                                        spans_report=spans_report)

    assert result["rungs_placed"] == 1
    assert result["rungs_unplaceable"] == 0
    assert result["by_kind"] == {"model_rung": 1}
    assert len(result["placed"]) == 1
    rung = f.members[result["placed"][0]]
    assert rung.kind == "model_rung"
    assert np.allclose(rung.p1, [2.0, 0.0, 9.3])
    length = float(np.linalg.norm(rung.p1 - rung.p0))
    assert length == pytest.approx(2.0)


# ---------------------------------------------------------- unplaceable -----
def test_unclearable_rung_never_placed(cfg):
    """An obstacle point mid-rod blocks both candidate rods AND any model
    rung fallback: rung skipped, rungs_unplaceable == 1, forest
    unchanged (member count, registry, certificates all pristine)."""
    d = 8.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
    obstacle = np.array([[4.0, 0.0, 14.0]])  # midpoint of the M->N X-pair rod
    surface_tree = cKDTree(obstacle)

    # Certify the two columns clean against this surface first (they sit
    # far enough in xy from the single obstacle point).
    violations = f.revalidate(surface_tree, cfg)
    assert violations == []
    assert f.dirty_members() == []
    n_members_before = len(f.members)
    braces_before = list(f.braces)

    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    result = bracing.propose_and_place(f, surface_tree, plate_z=0.0, cfg=cfg,
                                        spans_report=spans_report)

    assert result["rungs_placed"] == 0
    assert result["rungs_unplaceable"] == 1
    assert result["placed"] == []
    assert len(f.members) == n_members_before
    assert f.braces == braces_before
    assert f.dirty_members() == []


# --------------------------------------------------------------- pristine ---
def test_all_placed_braces_certificates_valid(cfg):
    d = 8.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    result = bracing.propose_and_place(f, _far_tree(), plate_z=0.0, cfg=cfg,
                                        spans_report=spans_report)
    assert result["rungs_placed"] == 1
    assert f.dirty_members() == []


# ------------------------------------------------------------ determinism --
def test_determinism_identical_inputs_identical_placement(cfg):
    def build():
        d = 8.0
        f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
        spans_report = {mi: dict(worst_span_mm=65.0, restraint_zs=[],
                                  free_gaps=[(0.0, 65.0)], n_terminals=1)}
        return f, spans_report

    f1, sr1 = build()
    f2, sr2 = build()
    r1 = bracing.propose_and_place(f1, _far_tree(), plate_z=0.0, cfg=cfg, spans_report=sr1)
    r2 = bracing.propose_and_place(f2, _far_tree(), plate_z=0.0, cfg=cfg, spans_report=sr2)

    assert r1["placed"] == r2["placed"]
    assert r1["rungs_placed"] == r2["rungs_placed"]
    assert r1["rungs_unplaceable"] == r2["rungs_unplaceable"]
    assert r1["by_kind"] == r2["by_kind"]
    for idx in r1["placed"]:
        m1, m2 = f1.members[idx], f2.members[idx]
        assert np.array_equal(m1.p0, m2.p0)
        assert np.array_equal(m1.p1, m2.p1)
        assert m1.r0 == m2.r0
        assert m1.r1 == m2.r1
        assert m1.kind == m2.kind


# ---------------------------------------------------------- restraint dedup --
def test_rung_skipped_near_existing_restraint(cfg):
    """A rung height within cfg.brace_ladder_z_mm/2 of an existing
    restraint (spans_report's restraint_zs) is skipped entirely -- not
    placed, not counted unplaceable."""
    d = 8.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
    # single rung at z=10 (see xpair test); restraint at z=9 is within
    # brace_ladder_z_mm/2 (4.0mm default) of it.
    assert abs(10.0 - 9.0) <= cfg.brace_ladder_z_mm / 2.0
    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[9.0],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    n_members_before = len(f.members)

    result = bracing.propose_and_place(f, _far_tree(), plate_z=0.0, cfg=cfg,
                                        spans_report=spans_report)
    assert result["placed"] == []
    assert result["rungs_placed"] == 0
    assert result["rungs_unplaceable"] == 0
    assert result["rungs_requested"] == 1
    assert len(f.members) == n_members_before


def test_rung_not_skipped_when_restraint_far(cfg):
    """Control for the dedup rule: a restraint well outside the
    brace_ladder_z_mm/2 window does not suppress the rung."""
    d = 8.0
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
    assert abs(10.0 - 2.0) > cfg.brace_ladder_z_mm / 2.0
    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[2.0],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    result = bracing.propose_and_place(f, _far_tree(), plate_z=0.0, cfg=cfg,
                                        spans_report=spans_report)
    assert result["rungs_placed"] == 1
    assert result["placed"] != []
