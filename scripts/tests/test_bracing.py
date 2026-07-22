import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import bracing, config, placement, tree as tm
from support_placement import verify


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


# ---------------------------------------------------------------- X-pair ----
def test_xpair_geometry_pin_wider_columns(cfg):
    """Two vertical columns 8mm apart (v3's
    test_wider_trunk_pair_still_gets_crossed_pair, adapted): 2 rods,
    rise == 8.0 exactly, endpoints on both axes, discounted radius."""
    d = 8.0
    assert d > cfg.twin_single_brace_xy_mm
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
    # gap sized to yield exactly n_rungs == 1 (z_lo=0, z_hi=20 -> z=10)
    spans_report = {mi: dict(worst_span_mm=20.0, restraint_zs=[],
                              free_gaps=[(0.0, 20.0)], n_terminals=1)}
    surface_tree = _far_tree()

    result = bracing.propose_and_place(f, surface_tree, plate_z=0.0, cfg=cfg,
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
    per rung, alternating p0 side by rung parity, UNdiscounted radius."""
    d = 2.5
    assert d <= cfg.twin_single_brace_xy_mm
    f, (mi, ni) = _column_forest(((0.0, 0.0), 0.0, 40.0), ((d, 0.0), 0.0, 40.0))
    # gap sized to yield exactly n_rungs == 2 (z_lo=0, z_hi=36 -> z=12, 24)
    spans_report = {mi: dict(worst_span_mm=36.0, restraint_zs=[],
                              free_gaps=[(0.0, 36.0)], n_terminals=1)}

    result = bracing.propose_and_place(f, _far_tree(), plate_z=0.0, cfg=cfg,
                                        spans_report=spans_report)
    assert result["rungs_placed"] == 2
    assert result["rungs_unplaceable"] == 0
    assert result["by_kind"] == {"brace": 2}  # one rod per rung, two rungs
    assert len(result["placed"]) == 2

    rod_len = d * np.sqrt(2.0)
    expected_r = max(cfg.brace_r_mm,
                      placement._shaft_r_for_L(rod_len, cfg, cfg.max_segment_angle_deg))
    rods = [f.members[idx] for idx in result["placed"]]
    assert [round(float(r.p0[2]), 6) for r in rods] == [12.0, 24.0]
    for rod in rods:
        assert rod.r0 == pytest.approx(expected_r)
        assert abs(float(rod.p1[2] - rod.p0[2])) == pytest.approx(d)
    # alternating p0 side: rung 0 (even) starts at M's xy (0.0), rung 1
    # (odd) starts at N's xy (2.5)
    assert rods[0].p0[0] == pytest.approx(0.0)
    assert rods[1].p0[0] == pytest.approx(d)


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
    assert bracing._find_cross_link_candidates(f, mi, z, cfg) == []


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
