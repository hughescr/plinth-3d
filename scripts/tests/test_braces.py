import numpy as np
from support_placement import braces
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
