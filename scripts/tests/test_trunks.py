"""v3.1 build-time stability + consolidation unit tests (second review:
tall bare columns wobble during the build; parallel twin columns waste
resin; ball joints must match member size; unclear columns must not
punch through the model)."""
import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import trunks as tm, placement


@pytest.fixture
def far_tree():
    """A surface tree with everything a kilometer away -- all clear."""
    return cKDTree(np.array([[1e6, 1e6, 1e6]]))


def test_consolidate_merges_adjacent_twin_trunks(cfg, far_tree):
    a = tm.Trunk([0.0, 0.0], z_top=30.0)
    a.shoots = [0]
    a.carried_mm2 = 10.0
    b = tm.Trunk([1.5, 0.0], z_top=25.0)  # 1.5mm away: pointless twin
    b.shoots = [1]
    b.carried_mm2 = 8.0
    stub_ends = [np.array([0.0, 0.0, 32.0]), np.array([1.5, 0.0, 27.0])]
    tip_r = np.array([0.3, 0.3])
    assignment = [(0, 30.0, True), (1, 25.0, True)]
    trunks, assignment = tm.consolidate([a, b], assignment, stub_ends, tip_r,
                                         far_tree, cfg)
    assert len(trunks) == 1
    assert sorted(trunks[0].shoots) == [0, 1]
    assert assignment[0][0] == assignment[1][0] == 0
    assert trunks[0].carried_mm2 == pytest.approx(18.0)


def test_consolidate_leaves_distant_trunks_alone(cfg, far_tree):
    a = tm.Trunk([0.0, 0.0], z_top=30.0)
    a.shoots = [0]
    b = tm.Trunk([20.0, 0.0], z_top=25.0)
    b.shoots = [1]
    stub_ends = [np.array([0.0, 0.0, 32.0]), np.array([20.0, 0.0, 27.0])]
    assignment = [(0, 30.0, True), (1, 25.0, True)]
    trunks, _ = tm.consolidate([a, b], assignment, stub_ends,
                                np.array([0.3, 0.3]), far_tree, cfg)
    assert len(trunks) == 2


def test_unbraced_tall_trunk_is_thickened_not_ignored(cfg):
    """The 65mm-bare-run failure mode: a tall trunk with no braces and a
    single top junction must come out of resize_for_spans substantially
    thicker than the slender braced-assumption radius."""
    t = tm.Trunk([0.0, 0.0], z_top=60.0)
    t.shoots = [0]
    t.r = cfg.min_trunk_r_mm
    tm.resize_for_spans([t], braces=[], junctions_by_trunk={0: [60.0]},
                         plate_z=-5.0, cfg=cfg)
    # span is the full 65mm -> the unclamped deflection solve demands more
    r_span = placement._shaft_r_for_L(65.0, cfg)
    assert t.r == pytest.approx(min(r_span, cfg.max_trunk_r_mm), abs=1e-6)
    assert t.r > cfg.min_trunk_r_mm * 1.5


def test_braced_trunk_stays_slender(cfg):
    """Same trunk, but with braces cutting the span: stays thin."""
    t = tm.Trunk([0.0, 0.0], z_top=60.0)
    t.shoots = [0]
    t.r = cfg.min_trunk_r_mm
    braces = [dict(i=0, j=1, r=0.5,
                    p0=np.array([0.0, 0.0, z]), p1=np.array([5.0, 0.0, z + 5.0]))
              for z in (10.0, 25.0, 40.0)]
    tm.resize_for_spans([t], braces=braces, junctions_by_trunk={0: [60.0]},
                         plate_z=-5.0, cfg=cfg)
    thick = placement._shaft_r_for_L(65.0, cfg)
    assert t.r < thick


def test_model_strut_finds_anchor_below_not_above(cfg, far_tree):
    contact = dict(position=np.array([0.0, 0.0, 20.0]),
                    normal=np.array([0.0, 0.0, -1.0]))
    stub_end = np.array([0.0, 0.0, 17.5])
    ground = np.array([
        [2.0, 0.0, 25.0],    # above: never
        [3.0, 0.0, 11.0],    # below, lean ~25deg: good
        [18.0, 0.0, 16.0],   # below but lean ~85deg: too shallow
    ])
    anchor = tm.model_strut(contact, stub_end, 0.4, ground, far_tree, cfg)
    assert anchor is not None
    assert np.allclose(anchor, [3.0, 0.0, 11.0])


def test_model_strut_returns_none_when_nothing_qualifies(cfg, far_tree):
    contact = dict(position=np.array([0.0, 0.0, 20.0]),
                    normal=np.array([0.0, 0.0, -1.0]))
    stub_end = np.array([0.0, 0.0, 17.5])
    ground = np.array([[0.0, 0.0, 40.0]])  # only above
    assert tm.model_strut(contact, stub_end, 0.4, ground, far_tree, cfg) is None


def test_coalesce_y_diagonal_climbs_at_branch_angle(cfg, far_tree):
    """v3.6: Y-diagonals are branch-angle members (60deg): same lateral
    reach, less height -- rise must be d/tan(branch_angle_deg)."""
    parent = tm.Trunk([0.0, 0.0], z_top=40.0)
    parent.shoots = [0]
    parent.r = 1.0
    child = tm.Trunk([6.0, 0.0], z_top=38.0)
    child.shoots = [1]
    child.r = 1.0
    juncs = {0: [(40.0, 42.0)], 1: [(38.0, 40.0)]}
    merges = tm.coalesce_columns([parent, child], juncs, plate_z=-5.0,
                                  surface_tree=far_tree, cfg=cfg)
    assert len(merges) == 1
    rise = child.z_start - child.z_split
    expected = 6.0 / np.tan(np.radians(cfg.branch_angle_deg))
    assert rise == pytest.approx(expected, abs=1e-6)
    lean = np.degrees(np.arctan2(6.0, rise))
    assert lean == pytest.approx(cfg.branch_angle_deg, abs=0.1)


def test_strut_cone_widened_to_branch_angle(cfg, far_tree):
    """Queue item 2: the strut anchor cone follows the branch-angle
    family (60deg). An anchor at 58deg lean -- rejected at the old 55 --
    must now qualify."""
    assert cfg.strut_fallback_max_angle_deg == pytest.approx(60.0)
    contact = dict(position=np.array([0.0, 0.0, 20.0]),
                    normal=np.array([0.0, 0.0, -1.0]))
    stub_end = np.array([0.0, 0.0, 17.5])
    drop = 8.0
    lateral = drop * np.tan(np.radians(58.0))
    ground = np.array([[lateral, 0.0, 17.5 - drop]])
    anchor = tm.model_strut(contact, stub_end, 0.4, ground, far_tree, cfg)
    assert anchor is not None


def test_junction_restraint_does_not_exist_until_shoot_completes(cfg):
    """Fourth review (Craig): at every layer, the layers above DO NOT
    EXIST YET. A shoot junction at z=30 whose shoot only reaches its
    model contact at z=60 provides NO restraint while the trunk builds
    through 30..60 -- the transient worst span must equal the full
    unrestrained height, exactly as if the junction weren't there."""
    lagged = tm.transient_worst_span([(30.0, 60.0)], plate_z=-5.0, z_top=60.0)
    unbraced = tm.transient_worst_span([], plate_z=-5.0, z_top=60.0)
    assert lagged == pytest.approx(unbraced) == pytest.approx(65.0)
    # whereas a brace (active the moment its layer prints) at the same
    # point DOES cut the span
    braced = tm.transient_worst_span([(30.0, 30.0)], plate_z=-5.0, z_top=60.0)
    assert braced == pytest.approx(35.0)
