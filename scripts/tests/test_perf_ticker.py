"""SPEC-V5-PERF sec 4 (task #17): the off-hot-path progress ticker.

Two obligations: (1) the compute path is byte-identical with the ticker on or
off -- the only compute-path work is the monotonic int the counter already
tracked; (2) a slowed stage actually emits at least one ticker line on stderr.
"""
import time

import numpy as np
import trimesh
import pytest

from support_placement import cco, config, engine
from support_placement.geometry_exact import ExactSurface


@pytest.fixture
def cfg():
    return config.SupportConfig()


def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def _serialize(forest):
    return [(m.p0.tobytes(), m.p1.tobytes(), m.r0, m.r1, m.kind,
             tuple(m.owner) if isinstance(m.owner, tuple) else m.owner,
             m.embed_end) for m in forest.members]


def test_ticker_compute_path_byte_identical(cfg):
    """cco.build with a progress cell must produce byte-identical forest output
    to cco.build without one (the counter write is result-inert)."""
    plate = _box((40, 40, 0.5), center=(0, 0, 0))
    geom = ExactSurface(plate)
    contacts = [dict(position=np.array([x, y, -0.25]),
                     normal=np.array([0.0, 0.0, -1.0]))
                for x in (-8.0, 0.0, 8.0) for y in (-6.0, 6.0)]
    tip_r = np.array([0.2] * len(contacts))
    carried = np.array([1.0] * len(contacts))
    minima = [False] * len(contacts)

    f_off, u_off = cco.build(plate, contacts, tip_r, carried, minima, -12.0,
                             geom, cfg, sample_points=None, sample_delicacy=None,
                             progress=None)
    cell = engine._ProgressCell()
    f_on, u_on = cco.build(plate, contacts, tip_r, carried, minima, -12.0,
                           geom, cfg, sample_points=None, sample_delicacy=None,
                           progress=cell)
    assert u_off == u_on
    assert _serialize(f_off) == _serialize(f_on)
    # the cell advanced to the full contact count
    assert cell.n == len(contacts)


def test_ticker_emits_line_on_slow_stage(capfd):
    """A stage that outlives one tick interval must print at least one
    ``[v5] .. {label} {n}/{total} contacts`` line to stderr."""
    cell = engine._ProgressCell()
    with engine._ticker("construct[p0]", lambda: cell.n, 10, interval=0.05):
        cell.n = 3
        time.sleep(0.22)
    _out, err = capfd.readouterr()
    assert "[v5] .. construct[p0] 3/10 contacts" in err, err


def test_ticker_silent_and_joins_on_fast_stage(capfd):
    """A stage shorter than one interval prints nothing and the daemon thread
    is joined cleanly on exit (no dangling thread -- risk P7)."""
    import threading
    before = {t.name for t in threading.enumerate()}
    cell = engine._ProgressCell()
    with engine._ticker("construct[p0]", lambda: cell.n, 10, interval=5.0):
        cell.n = 1
    _out, err = capfd.readouterr()
    assert "construct[p0]" not in err
    after = {t.name for t in threading.enumerate()}
    assert "ticker-construct[p0]" not in after
