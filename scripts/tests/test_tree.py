import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import tree as tm


def _member(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("contact", 0), embed_end=None):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner, embed_end=embed_end)


# ---------------------------------------------------------------- Member --
def test_fresh_member_starts_dirty():
    m = _member([0, 0, 0], [0, 0, -10])
    assert not m.cert_valid


def test_certificate_valid_after_matching_geom_version():
    m = _member([0, 0, 0], [0, 0, -10])
    m.cert_version = m.geom_version
    m.cert_radius = max(m.r0, m.r1)
    assert m.cert_valid


def test_radius_increase_invalidates_certificate():
    m = _member([0, 0, 0], [0, 0, -10], r0=0.5, r1=0.5)
    m.cert_version = m.geom_version
    m.cert_radius = 0.5
    assert m.cert_valid
    m.r0 = 0.8
    assert not m.cert_valid


def test_radius_decrease_keeps_certificate():
    """design-v4.md #4: 'radius DECREASE with unchanged geometry keeps
    the certificate (checked fat covers thin)'."""
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0)
    m.cert_version = m.geom_version
    m.cert_radius = 1.0
    m.r0 = 0.4
    m.r1 = 0.4
    assert m.cert_valid


def test_position_change_invalidates_certificate_even_with_unchanged_radius():
    """The escape hatch is radius-only: a geometry change must void the
    cert even though the (unchanged) radius alone would satisfy the
    max(r0,r1) <= cert_radius clause."""
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0)
    m.cert_version = m.geom_version
    m.cert_radius = 1.0
    m.p1 = np.array([5.0, 0.0, -10.0])
    assert not m.cert_valid


def test_position_change_then_reduced_radius_still_invalid():
    """Guards the specific failure mode the two-field (geom_version,
    cert_radius) model must not fall into: a position change followed by
    ANY radius value must not be validated by the decrease escape hatch."""
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0)
    m.cert_version = m.geom_version
    m.cert_radius = 1.0
    m.p0 = np.array([2.0, 0.0, 0.0])
    m.r0 = 0.1
    assert not m.cert_valid


# ---------------------------------------------------------------- Forest --
def _far_tree():
    return cKDTree(np.array([[1e6, 1e6, 1e6]]))


def test_forest_add_member_starts_as_root_and_dirty():
    f = tm.Forest()
    idx = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    assert idx in f.roots
    assert idx in f.dirty_members()


def test_forest_set_parent_updates_topology():
    f = tm.Forest()
    a = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    b = f.add_member(_member([5, 0, 0], [0, 0, 0]))
    f.set_parent(b, a)
    assert f.parent[b] == a
    assert b in f.children[a]
    assert b not in f.roots
    assert a in f.roots


def test_revalidate_certifies_clear_members(cfg):
    f = tm.Forest()
    f.add_member(_member([0, 0, 0], [0, 0, -10], r0=0.3, r1=0.3))
    violations = f.revalidate(_far_tree(), cfg)
    assert violations == []
    assert f.dirty_members() == []


def test_revalidate_reports_violation_and_leaves_member_dirty(cfg):
    f = tm.Forest()
    close_tree = cKDTree(np.array([[0.0, 0.0, -5.0]]))
    f.add_member(_member([0, 0, 0], [0, 0, -10], r0=0.3, r1=0.3))
    violations = f.revalidate(close_tree, cfg)
    assert len(violations) == 1
    assert f.dirty_members() != []


def test_checkpoint_rollback_restores_mutated_member_state(cfg):
    f = tm.Forest()
    idx = f.add_member(_member([0, 0, 0], [0, 0, -10], r0=0.3, r1=0.3))
    f.revalidate(_far_tree(), cfg)
    assert f.dirty_members() == []
    cp = f.checkpoint()
    f.members[idx].r0 = 5.0
    f.members[idx].p1 = np.array([9.0, 9.0, 9.0])
    assert f.dirty_members() == [idx]
    f.rollback(cp)
    assert f.members[idx].r0 == 0.3
    assert np.allclose(f.members[idx].p1, [0, 0, -10])
    assert f.dirty_members() == []


def test_checkpoint_rollback_removes_newly_appended_members():
    f = tm.Forest()
    a = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    cp = f.checkpoint()
    b = f.add_member(_member([5, 0, 0], [0, 0, 0]))
    f.set_parent(b, a)
    assert len(f.members) == 2
    f.rollback(cp)
    assert len(f.members) == 1
    assert f.roots == [a]


def test_checkpoint_rollback_restores_topology_of_existing_member():
    f = tm.Forest()
    a = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    b = f.add_member(_member([5, 0, 0], [0, 0, 0]))
    f.set_parent(b, a)
    cp = f.checkpoint()
    c = f.add_member(_member([1, 1, 1], [2, 2, 2]))
    f.set_parent(c, a)
    assert sorted(f.children[a]) == sorted([b, c])
    f.rollback(cp)
    assert f.children[a] == [b]
    assert f.parent[b] == a


def test_commit_keeps_mutations_and_pops_checkpoint():
    f = tm.Forest()
    idx = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    cp = f.checkpoint()
    f.members[idx].r0 = 2.0
    f.commit(cp)
    assert f.members[idx].r0 == 2.0
    assert f._cp_stack == []


# ---------------------------------------------------------------- volume --
def test_volume_report_matches_hand_computed_frustum_and_ball():
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=2.0, kind="trunk")
    expected_frustum = np.pi / 3.0 * 10.0 * (1.0 ** 2 + 1.0 * 2.0 + 2.0 ** 2) / 1000.0
    ball = dict(center=np.array([0.0, 0.0, -10.0]), r=1.5, owner=("contact", 0))
    expected_ball = 4.0 / 3.0 * np.pi * 1.5 ** 3 / 1000.0
    report = tm.volume_report([m], [ball])
    assert report["member_ml"] == pytest.approx(expected_frustum)
    assert report["ball_ml"] == pytest.approx(expected_ball)
    assert report["total_ml"] == pytest.approx(expected_frustum + expected_ball)
    assert report["by_kind_ml"]["trunk"] == pytest.approx(expected_frustum)


def test_volume_report_sums_multiple_kinds():
    m1 = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0, kind="trunk")
    m2 = _member([0, 0, 0], [3, 0, -3], r0=0.4, r1=0.4, kind="shoot")
    report = tm.volume_report([m1, m2], [])
    v1 = np.pi * 1.0 ** 2 * 10.0 / 1000.0
    v2 = np.pi * 0.4 ** 2 * (3 ** 2 + 3 ** 2) ** 0.5 / 1000.0
    assert report["by_kind_ml"]["trunk"] == pytest.approx(v1)
    assert report["by_kind_ml"]["shoot"] == pytest.approx(v2, rel=1e-6)
    assert report["member_ml"] == pytest.approx(v1 + v2, rel=1e-6)


# ---------------------------------------------------------- printability --
def test_printability_report_catches_too_steep_segment(cfg):
    # 80 degrees from vertical at a fat radius (60-deg family) -- steeper
    # than either angle family allows.
    m = _member([0, 0, 10], [20 * np.tan(np.radians(80)), 0, 0],
                r0=1.0, r1=1.0, kind="trunk")
    violations = tm.printability_report([m], cfg)
    assert len(violations) == 1


def test_printability_report_catches_upward_flare_too_fast(cfg):
    """A member within angle budget geometrically, but whose radius grows
    fast enough going UP that drift+taper still exceeds tan(theta_max)."""
    # near-vertical (small drift) but flares from 0.2 to 3.0mm over 2mm rise
    m = _member([0, 0, 0], [0.1, 0, 2.0], r0=0.2, r1=3.0, kind="trunk")
    violations = tm.printability_report([m], cfg)
    assert len(violations) == 1


def test_printability_report_passes_45_degree_member(cfg):
    # exactly max_segment_angle_deg (45), radius below the 60-deg family
    # floor so the tighter 45-deg budget applies -- should just pass.
    r = cfg.min_shoot_r_mm - 0.05
    m = _member([0, 0, 10], [10, 0, 0], r0=r, r1=r, kind="shoot")
    violations = tm.printability_report([m], cfg)
    assert violations == []


def test_printability_report_credits_upward_taper(cfg):
    """An upward taper (shrinking radius going up) is a CREDIT: a member
    whose drift alone would be borderline should pass once the taper
    term is added, since taper_rate is negative."""
    theta = cfg.branch_angle_deg
    tanth = np.tan(np.radians(theta))
    dz = 10.0
    dxy = dz * tanth  # drift alone would sit exactly AT the limit
    r0, r1 = 1.0, 0.4  # shrinks going up (both >= min_shoot_r_mm, so the
    # 60-deg family applies) -> negative taper_rate credit
    m = _member([0, 0, 0], [dxy, 0, dz], r0=r0, r1=r1, kind="trunk")
    violations = tm.printability_report([m], cfg)
    assert violations == []
    # sanity: without the taper credit (constant radius) the same drift
    # would be right at the boundary, not comfortably clear
    m_flat = _member([0, 0, 0], [dxy, 0, dz], r0=1.0, r1=1.0, kind="trunk")
    v_flat = tm.printability_report([m_flat], cfg)
    assert v_flat == []  # boundary itself is not a violation (<=, not <)


def test_printability_report_exempts_stub_kind(cfg):
    # a stub following a near-horizontal surface normal must not be
    # flagged -- it's physically constrained, not a builder choice.
    m = _member([0, 0, 0], [2.5, 0, 0.01], r0=0.3, r1=0.3, kind="stub")
    assert tm.printability_report([m], cfg) == []


def test_printability_report_model_rung_exempt_when_short(cfg):
    m = _member([0, 0, 5], [cfg.model_brace_max_len_mm - 0.1, 0, 5],
                r0=0.5, r1=0.5, kind="model_rung")
    assert tm.printability_report([m], cfg) == []


def test_printability_report_flags_horizontal_non_rung(cfg):
    m = _member([0, 0, 5], [10, 0, 5], r0=0.5, r1=0.5, kind="brace")
    violations = tm.printability_report([m], cfg)
    assert len(violations) == 1
    assert violations[0]["reason"] == "near_horizontal"
