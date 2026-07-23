"""SPEC-V5-PERF sec 2.3 / 5.1.4 (A2): price-ordered trials with early exit.

The full-trial-reference equivalence (early-exit winner == baseline min-delta
survivor) is proven at fixture scale by the _perf_fingerprint byte-compare of
table + toed_slab against the pre-A2 baseline (test_engine's frozen-baseline
gate) -- the baseline engine WAS the full-trial version, so that compare is
literally early-exit-vs-full-trial on real adversarial candidate sets with
clearance failures. This file adds the direct invariants A2's identity sketch
turns into executable assertions: determinism, and the early-exit correctly
skipping a cheaper NON-surviving candidate in favor of a costlier clean one.
"""
import numpy as np
import trimesh
import pytest

from support_placement import cco, config
from support_placement import tree as tm
from support_placement.geometry_exact import ExactSurface


@pytest.fixture
def cfg():
    return config.SupportConfig()


def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def _soup(*meshes):
    merged = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=merged.vertices.copy(),
                           faces=merged.faces.copy(), process=False)


def _plate_run(cfg, mesh, geom, contacts):
    tip_r = np.array([0.2] * len(contacts))
    carried = np.array([1.0] * len(contacts))
    minima = [False] * len(contacts)
    return cco.build(mesh, contacts, tip_r, carried, minima, -12.0, geom, cfg,
                     sample_points=None, sample_delicacy=None)


def _serialize(forest):
    out = []
    for m in forest.members:
        out.append((m.p0.tobytes(), m.p1.tobytes(), m.r0, m.r1, m.kind,
                    tuple(m.owner) if isinstance(m.owner, tuple) else m.owner,
                    m.embed_end))
    return out


def test_a2_build_is_deterministic(cfg):
    """A2's price sort is a total order (finiteness asserted), so cco.build is
    bit-for-bit reproducible across runs."""
    plate = _box((40, 40, 0.5), center=(0, 0, 0))
    geom = ExactSurface(plate)
    contacts = [dict(position=np.array([x, 0.0, -0.25]),
                     normal=np.array([0.0, 0.0, -1.0]))
                for x in (-8.0, 0.0, 8.0)]
    f1, u1 = _plate_run(cfg, plate, geom, contacts)
    f2, u2 = _plate_run(cfg, plate, geom, contacts)
    assert u1 == u2
    assert _serialize(f1) == _serialize(f2)


def test_a2_skips_cheaper_nonsurviving_candidate(cfg):
    """Early exit must pass over a cheaper candidate that FAILS clearance and
    land on the costlier clean one. An obstacle wall directly below the contact
    blocks the cheapest straight-down descent; the routed column must avoid it
    (its xy is displaced off the obstacle), and the result is still clean."""
    plate = _box((40, 40, 0.5), center=(0, 0, 0))
    # a thin wall well below the stub_end (z=-2.75), spanning z in [-11,-6] at
    # x=0: it blocks the cheapest straight-down descent corridor but leaves the
    # stub clear and gives diagonals ~3mm of vertical room to escape in x
    # before reaching the wall top.
    wall = _box((0.6, 12.0, 5.0), center=(0.0, 0.0, -8.5))
    mesh = _soup(plate, wall)
    geom = ExactSurface(mesh)
    contacts = [dict(position=np.array([0.0, 0.0, -0.25]),
                     normal=np.array([0.0, 0.0, -1.0]))]
    tip_r = np.array([0.2])
    carried = np.array([1.0])
    forest, unrouted = cco.build(mesh, contacts, tip_r, carried, [False], -15.0,
                                 geom, cfg, sample_points=None, sample_delicacy=None)
    assert unrouted == [], "contact must route via a diagonal around the wall"
    tm.assert_all_certified(forest)
    # the descending column's foot must be displaced in x off the wall
    # centerline (x=0) -- i.e. the cheap straight-down candidate was skipped.
    trunks = [m for m in forest.members if m.kind == "trunk"]
    assert trunks, "expected a plate-descent column"
    foot_x = [abs(float(m.p1[0])) for m in trunks]
    assert max(foot_x) > 0.4, ("straight-down column would foot at x~0 on the "
                               "wall; early exit should have skipped it")
