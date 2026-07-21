import numpy as np
import pytest
from support_placement import braces, placement
from support_placement import trunks as tm
from support_placement.routing import Segment, Route


def _shaft_route(xy, z0, z1, n_braces):
    seg = Segment(p0=np.array([xy[0], xy[1], z0]), p1=np.array([xy[0], xy[1], z1]),
                  r0=0.5, r1=0.5, kind="shaft")
    return Route(contact_idx=0, segments=[seg], anchor_kind="plate",
                  free_length_mm=z1 - z0, n_braces=n_braces)


def test_close_overlapping_shafts_get_braced(cfg):
    r1 = _shaft_route((0, 0), 0, 40, n_braces=3)
    r2 = _shaft_route((5, 0), 0, 40, n_braces=3)
    made, graph = braces.build_braces([r1, r2], cfg)
    assert len(made) > 0
    for b in made:
        assert abs(b["p0"][2] - b["p1"][2]) < 1e-9  # horizontal member
        assert np.linalg.norm(b["p0"][:2] - b["p1"][:2]) == 5.0


def test_distant_shafts_not_braced(cfg):
    r1 = _shaft_route((0, 0), 0, 40, n_braces=3)
    r2 = _shaft_route((100, 0), 0, 40, n_braces=3)
    made, graph = braces.build_braces([r1, r2], cfg)
    assert made == []


def test_non_overlapping_z_ranges_not_braced(cfg):
    r1 = _shaft_route((0, 0), 0, 10, n_braces=2)
    r2 = _shaft_route((5, 0), 20, 30, n_braces=2)
    made, graph = braces.build_braces([r1, r2], cfg)
    assert made == []


def test_unbraced_neighbors_are_left_alone(cfg):
    r1 = _shaft_route((0, 0), 0, 10, n_braces=0)
    r2 = _shaft_route((5, 0), 0, 10, n_braces=0)
    made, graph = braces.build_braces([r1, r2], cfg)
    assert made == []


def test_brace_count_does_not_exceed_what_was_needed(cfg):
    r1 = _shaft_route((0, 0), 0, 100, n_braces=1)
    r2 = _shaft_route((5, 0), 0, 100, n_braces=1)
    made, graph = braces.build_braces([r1, r2], cfg)
    assert len(made) <= 1


def test_twin_trunk_pair_gets_single_alternating_rods(cfg):
    """v3.6 (queue item 3): trunks left as unmerged twins (<=
    twin_single_brace_xy_mm apart) get ONE rod per rung, alternating
    direction, instead of a crossed X pair."""
    a = tm.Trunk([0.0, 0.0], z_top=40.0)
    a.n_braces = 3
    b = tm.Trunk([2.5, 0.0], z_top=40.0)
    b.n_braces = 3
    assert 2.5 <= cfg.twin_single_brace_xy_mm
    made, graph = braces.build_trunk_braces([a, b], plate_z=-5.0, cfg=cfg)
    n_rungs = graph[0][1]["n"]
    assert n_rungs > 1
    # one rod per rung, not two
    assert len(made) == n_rungs

    trunks = [a, b]
    p0_sides = [tuple(np.round(m["p0"][:2], 6)) for m in made]
    xy0 = tuple(np.round(a.xy, 6))
    xy1 = tuple(np.round(b.xy, 6))
    assert p0_sides[0] == xy0
    # consecutive rods alternate which trunk they start from
    for k in range(1, len(p0_sides)):
        assert p0_sides[k] != p0_sides[k - 1]
        assert p0_sides[k] in (xy0, xy1)

    # p0/p1 sit on the trunk xy named by their i/j keys respectively
    for m in made:
        assert np.allclose(m["p0"][:2], trunks[m["i"]].xy)
        assert np.allclose(m["p1"][:2], trunks[m["j"]].xy)

    # rod radius has NO pair-load-sharing discount
    rise = 2.5  # 45-degree diagonal: vertical rise == lateral span d
    rod_len = rise * np.sqrt(2.0)
    undiscounted = max(cfg.brace_r_mm,
                        placement._shaft_r_for_L(
                            rod_len, cfg,
                            angle_from_vertical_deg=cfg.max_segment_angle_deg))
    for m in made:
        assert m["r"] == pytest.approx(undiscounted)


def test_wider_trunk_pair_still_gets_crossed_pair(cfg):
    """Pairs wider than twin_single_brace_xy_mm keep the original
    crossed-X-pair behavior: two rods per rung."""
    a = tm.Trunk([0.0, 0.0], z_top=40.0)
    a.n_braces = 3
    b = tm.Trunk([8.0, 0.0], z_top=40.0)
    b.n_braces = 3
    assert 8.0 > cfg.twin_single_brace_xy_mm
    made, graph = braces.build_trunk_braces([a, b], plate_z=-5.0, cfg=cfg)
    n_rungs = graph[0][1]["n"]
    assert len(made) == 2 * n_rungs
