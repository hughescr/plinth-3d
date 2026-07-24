"""Shared helpers: canonical geometry extraction for float-exact determinism
comparison between run #1 and run #2."""
import hashlib
import numpy as np


def _f(m, name, default=None):
    if isinstance(m, dict):
        return m.get(name, default)
    return getattr(m, name, default)


def geom_arrays(members, balls):
    """Return (member_arr (M,8), ball_arr (B,4)) in the engine's list order.
    Member row = [p0(3), p1(3), r0, r1]; ball row = [center(3), r].
    List order is itself part of what determinism must preserve."""
    mrows = []
    for m in members:
        p0 = np.asarray(_f(m, "p0"), float).reshape(3)
        p1 = np.asarray(_f(m, "p1"), float).reshape(3)
        r0 = float(_f(m, "r0"))
        r1 = float(_f(m, "r1"))
        mrows.append([p0[0], p0[1], p0[2], p1[0], p1[1], p1[2], r0, r1])
    brows = []
    for b in balls:
        c = np.asarray(_f(b, "center"), float).reshape(3)
        r = float(_f(b, "r"))
        brows.append([c[0], c[1], c[2], r])
    marr = np.asarray(mrows, float) if mrows else np.zeros((0, 8))
    barr = np.asarray(brows, float) if brows else np.zeros((0, 4))
    return marr, barr


def digest(members, balls):
    marr, barr = geom_arrays(members, balls)
    h = hashlib.sha256()
    h.update(np.ascontiguousarray(marr).tobytes())
    h.update(b"|")
    h.update(np.ascontiguousarray(barr).tobytes())
    return dict(n_members=len(members), n_balls=len(balls),
                member_sha=hashlib.sha256(np.ascontiguousarray(marr).tobytes()).hexdigest(),
                ball_sha=hashlib.sha256(np.ascontiguousarray(barr).tobytes()).hexdigest(),
                combined_sha=h.hexdigest())
