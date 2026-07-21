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
    # far_tree: nothing blocks clearance, so the DIRECT move must win --
    # the survivor stays at its ORIGINAL xy, not the midpoint (v3.6's
    # midpoint retry is a fallback, never preferred over a clear direct
    # merge).
    trunks, assignment = tm.consolidate([a, b], assignment, stub_ends, tip_r,
                                         -5.0, far_tree, cfg)
    assert len(trunks) == 1
    assert sorted(trunks[0].shoots) == [0, 1]
    assert assignment[0][0] == assignment[1][0] == 0
    assert trunks[0].carried_mm2 == pytest.approx(18.0)
    assert np.allclose(trunks[0].xy, [0.0, 0.0])


def test_consolidate_leaves_distant_trunks_alone(cfg, far_tree):
    a = tm.Trunk([0.0, 0.0], z_top=30.0)
    a.shoots = [0]
    b = tm.Trunk([20.0, 0.0], z_top=25.0)
    b.shoots = [1]
    stub_ends = [np.array([0.0, 0.0, 32.0]), np.array([20.0, 0.0, 27.0])]
    assignment = [(0, 30.0, True), (1, 25.0, True)]
    trunks, _ = tm.consolidate([a, b], assignment, stub_ends,
                                np.array([0.3, 0.3]), -5.0, far_tree, cfg)
    assert len(trunks) == 2


def test_consolidate_midpoint_retry_when_direct_move_blocked(cfg):
    """v3.6 (queue item 3): an obstacle sitting right next to trunk a's
    xy blocks the direct move (gone's shoot re-aimed straight at
    survivor.xy clips it), but the MIDPOINT between the two trunks is
    clear for every shoot of both trunks AND the column -- consolidate
    must retry there rather than leaving the twins standing."""
    a = tm.Trunk([0.0, 0.0], z_top=30.0)
    a.shoots = [0]
    a.carried_mm2 = 5.0
    b = tm.Trunk([2.5, 0.0], z_top=30.0)
    b.shoots = [1]
    b.carried_mm2 = 4.0
    stub_ends = [np.array([0.0, 0.0, 32.0]), np.array([2.5, 0.0, 32.0])]
    tip_r = np.array([0.3, 0.3])
    assignment = [(0, 30.0, True), (1, 30.0, True)]
    # obstacle right beside a's xy: blocks b's shoot re-aimed directly at
    # a (junction lands within r+standoff of it) but is far from the
    # midpoint (1.25, 0)'s shoots and column.
    tree = cKDTree(np.array([[-0.5, 0.0, 29.5]]))
    trunks, assignment = tm.consolidate([a, b], assignment, stub_ends, tip_r,
                                         -5.0, tree, cfg)
    assert len(trunks) == 1
    merged = trunks[0]
    assert np.allclose(merged.xy, [1.25, 0.0])
    assert sorted(merged.shoots) == [0, 1]
    # both shoots' junctions moved to their NEW (midpoint) values
    new_jz = 32.0 - 1.25  # 45-degree drop over the new 1.25mm lateral reach
    assert assignment[0][0] == assignment[1][0] == 0
    assert assignment[0][1] == pytest.approx(new_jz)
    assert assignment[1][1] == pytest.approx(new_jz)
    assert assignment[0][2] is True and assignment[1][2] is True
    assert merged.z_top == pytest.approx(new_jz)
    assert merged.carried_mm2 == pytest.approx(9.0)


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


def test_coalesce_single_level_invariant_pins_chained_parent_bug(cfg, far_tree):
    """v3.7 regression pin: the OLD code's child-side filter only checked
    `child.foot_kind != 'plate'` -- a trunk that had ALREADY become a
    PARENT earlier in this same pass (foot_kind stays 'plate', only its
    would-be children change) could still be picked up as someone ELSE's
    child later in the same pass, chaining trunk[A] under trunk[C] while
    trunk[A] was itself already the parent of trunk[B]. Construct exactly
    that ordering (B merges into A first -- lowest z_top is processed
    first -- then A, still 'plate'/parent_idx=None, would be the closest
    remaining candidate for C to adopt): the fix's `ci in parents` guard
    must refuse to let A be adopted, so A stays a plain parent and C
    merges into A directly (a second child under A, not a chain) instead."""
    A = tm.Trunk([6.0, 0.0], z_top=60.0)
    A.shoots = [0]
    A.r = 1.0
    B = tm.Trunk([0.0, 0.0], z_top=20.0)
    B.shoots = [1]
    B.r = 1.0
    C = tm.Trunk([12.0, 0.0], z_top=120.0)
    C.shoots = [2]
    C.r = 1.0
    trunks = [A, B, C]
    juncs = {0: [(60.0, 62.0)], 1: [(20.0, 22.0)], 2: [(120.0, 122.0)]}
    merges = tm.coalesce_columns(trunks, juncs, plate_z=-5.0,
                                  surface_tree=far_tree, cfg=cfg)
    assert len(merges) == 2  # B->A and C->A both go through; A->C must not
    # the crux: A must never have been adopted as anyone's child, even
    # though it was still 'plate'/parent_idx=None (the old candidate
    # filter's only check) after taking B as a child.
    assert trunks[0].foot_kind == "plate" and trunks[0].parent_idx is None
    # no trunk has both children and a branch foot; every branch trunk's
    # parent is itself a plain (non-branch, non-child) plate trunk, and
    # its z_split lands within that parent's actual column span.
    for t in trunks:
        if t.foot_kind == "branch":
            parent = trunks[t.parent_idx]
            assert parent.foot_kind == "plate" and parent.parent_idx is None
            assert parent.base_z(-5.0) <= t.z_split <= parent.z_top


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
