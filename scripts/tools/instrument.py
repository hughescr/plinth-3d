"""Instrumentation harness that monkeypatches KernelSurface (the class) so a
full-scale engine.run records, with negligible overhead:

  * per-method call-size histogram (batch N reaching each native entry point)
    -> A23(d) "no surviving per-element FFI hot loop"
  * (opt-in) a seeded deterministic reservoir corpus of REAL geometry-call
    inputs (per-method, globally bounded ~100k rows) -> A23(a) production-mesh
    replay. OFF unless GEOM_CAPTURE_CORPUS=1, and even then only ACCEPTED rows
    are materialized (Algorithm R, vectorized accept), so no per-row Python hot
    loop runs over the ~10^7-row stream.
  * every constructed KernelSurface instance (to read telemetry + build_log
    after the run) -> A23(c),(e)

Import and call install() BEFORE engine.run; read the module globals after.
"""
import os
import numpy as np
from collections import Counter
from support_placement import geometry_kernel as GK

_CAPTURE = os.environ.get("GEOM_CAPTURE_CORPUS", "") == "1"

# ---- collected state -------------------------------------------------------
INSTANCES = []                      # every KernelSurface built during the run
CALL_HIST = {}                      # method -> Counter{batch_N: count}
CALL_TOTAL = Counter()             # method -> total native calls
ROW_TOTAL = Counter()              # method -> total rows (sum of batch N)
CORPUS = {}                         # method -> list of sampled input tuples
_SEEN = Counter()                   # method -> rows seen (for reservoir R)
_RNG = np.random.RandomState(0xACCE)  # deterministic reservoir

# per-method reservoir caps (~100k total)
CAPS = dict(query=20000, project=10000, inside=20000, segment_clearance=20000,
            sphere_breach=10000, cast_rays=10000, penetration_intervals=10000,
            penetration_intervals_batch=10000)


def _record(method, N):
    CALL_TOTAL[method] += 1
    ROW_TOTAL[method] += N
    CALL_HIST.setdefault(method, Counter())[N] += 1


def _reservoir(method, N, buildrow):
    """Deterministic reservoir (Algorithm R) over the full per-method row
    stream, but `buildrow(i)` is invoked ONLY for accepted local rows i, so
    total Python row-materialization is bounded by the cap (~100k)."""
    if not _CAPTURE:
        return
    cap = CAPS.get(method, 0)
    if cap <= 0 or N == 0:
        return
    buf = CORPUS.setdefault(method, [])
    base = int(_SEEN[method])
    _SEEN[method] += N
    i = 0
    while len(buf) < cap and i < N:
        buf.append(buildrow(i)); i += 1
    if i >= N:
        return
    rem = np.arange(i, N)
    gidx = base + rem                              # 0-based global index
    j = (_RNG.random_sample(len(rem)) * (gidx + 1)).astype(np.int64)
    for a in np.nonzero(j < cap)[0]:
        buf[int(j[a])] = buildrow(int(rem[a]))


def install():
    K = GK.KernelSurface
    orig = {name: getattr(K, name) for name in (
        "__init__", "query", "project", "inside", "cast_rays",
        "segment_clearance", "penetration_intervals",
        "penetration_intervals_batch", "sphere_breach")}

    def wrap_init(self, mesh, *a, **kw):
        orig["__init__"](self, mesh, *a, **kw)
        INSTANCES.append(self)

    def wrap_query(self, pts, distance_upper_bound=np.inf, workers=None, k=1):
        p = np.atleast_2d(np.asarray(pts, dtype=np.float64))
        _record("query", len(p))
        dub = float(distance_upper_bound)
        _reservoir("query", len(p), lambda i: (tuple(p[i]), dub))
        return orig["query"](self, pts, distance_upper_bound, workers, k)

    def wrap_project(self, pts):
        p = np.atleast_2d(np.asarray(pts, dtype=np.float64))
        _record("project", len(p))
        _reservoir("project", len(p), lambda i: (tuple(p[i]),))
        return orig["project"](self, pts)

    def wrap_inside(self, pts, batch=40000):
        p = np.atleast_2d(np.asarray(pts, dtype=np.float64))
        _record("inside", len(p))
        _reservoir("inside", len(p), lambda i: (tuple(p[i]),))
        return orig["inside"](self, pts, batch)

    def wrap_cast(self, origins, dirs, t_max=1.0):
        o = np.atleast_2d(np.asarray(origins, dtype=np.float64))
        d = np.atleast_2d(np.asarray(dirs, dtype=np.float64))
        _record("cast_rays", len(o))
        _reservoir("cast_rays", len(o),
                   lambda i: (tuple(o[i]), tuple(d[i]), float(t_max)))
        return orig["cast_rays"](self, origins, dirs, t_max)

    def wrap_segclr(self, p0s, p1s, r0s, r1s, standoff,
                    inset0s=None, inset1s=None, chunk_mm=5.0):
        p0 = np.atleast_2d(np.asarray(p0s, dtype=np.float64))
        p1 = np.atleast_2d(np.asarray(p1s, dtype=np.float64))
        N = len(p0)
        _record("segment_clearance", N)
        if _CAPTURE:
            r0 = np.broadcast_to(np.asarray(r0s, dtype=np.float64), (N,))
            r1 = np.broadcast_to(np.asarray(r1s, dtype=np.float64), (N,))
            i0 = (np.zeros(N) if inset0s is None
                  else np.broadcast_to(np.asarray(inset0s, dtype=np.float64), (N,)))
            i1 = (np.zeros(N) if inset1s is None
                  else np.broadcast_to(np.asarray(inset1s, dtype=np.float64), (N,)))
            _reservoir("segment_clearance", N, lambda i: (
                tuple(p0[i]), tuple(p1[i]), float(r0[i]), float(r1[i]),
                float(standoff), float(i0[i]), float(i1[i])))
        return orig["segment_clearance"](self, p0s, p1s, r0s, r1s, standoff,
                                         inset0s, inset1s, chunk_mm)

    def wrap_pen(self, p0, p1, r0, r1, n_offset_rays=8, majority_step_mm=0.12,
                 cap=80, min_illegal_len_mm=0.05, rays="axis"):
        _record("penetration_intervals", 1)
        _reservoir("penetration_intervals", 1, lambda i: (
            tuple(np.asarray(p0, float).reshape(3)),
            tuple(np.asarray(p1, float).reshape(3)),
            float(r0), float(r1), int(n_offset_rays), float(majority_step_mm),
            int(cap), float(min_illegal_len_mm), rays))
        return orig["penetration_intervals"](self, p0, p1, r0, r1, n_offset_rays,
                                             majority_step_mm, cap,
                                             min_illegal_len_mm, rays)

    def wrap_penb(self, p0s, p1s, r0s, r1s, n_offset_rays=8,
                  majority_step_mm=0.12, cap=80, min_illegal_len_mm=0.05,
                  rays="axis"):
        p0 = np.atleast_2d(np.asarray(p0s, dtype=np.float64))
        p1 = np.atleast_2d(np.asarray(p1s, dtype=np.float64))
        N = len(p0)
        _record("penetration_intervals_batch", N)
        if _CAPTURE:
            r0 = np.broadcast_to(np.asarray(r0s, dtype=np.float64), (N,))
            r1 = np.broadcast_to(np.asarray(r1s, dtype=np.float64), (N,))
            _reservoir("penetration_intervals_batch", N, lambda i: (
                tuple(p0[i]), tuple(p1[i]), float(r0[i]), float(r1[i]),
                int(n_offset_rays), float(majority_step_mm), int(cap),
                float(min_illegal_len_mm), rays))
        return orig["penetration_intervals_batch"](self, p0s, p1s, r0s, r1s,
                                                   n_offset_rays, majority_step_mm,
                                                   cap, min_illegal_len_mm, rays)

    def wrap_sph(self, centers, radii, eps_mm=0.05, half_deg=25.0, n_cone=8):
        c = np.atleast_2d(np.asarray(centers, dtype=np.float64))
        N = len(c)
        _record("sphere_breach", N)
        if _CAPTURE:
            r = np.broadcast_to(np.asarray(radii, dtype=np.float64), (N,))
            _reservoir("sphere_breach", N, lambda i: (
                tuple(c[i]), float(r[i]), float(eps_mm), float(half_deg),
                int(n_cone)))
        return orig["sphere_breach"](self, centers, radii, eps_mm, half_deg, n_cone)

    K.__init__ = wrap_init
    K.query = wrap_query
    K.project = wrap_project
    K.inside = wrap_inside
    K.cast_rays = wrap_cast
    K.segment_clearance = wrap_segclr
    K.penetration_intervals = wrap_pen
    K.penetration_intervals_batch = wrap_penb
    K.sphere_breach = wrap_sph


def workhorse_telemetry():
    """The engine's single reused geom accumulates all telemetry; pick the
    instance with the largest counter sum (ignores my tiny test instances)."""
    best = None
    best_sum = -1
    for inst in INSTANCES:
        try:
            t = inst.telemetry()
        except Exception:
            continue
        s = sum(v for v in t.values() if isinstance(v, (int, float)))
        if s > best_sum or best is None:
            best_sum = s
            best = inst
    return best


def summary():
    hist = {m: dict(sorted(c.items())) for m, c in CALL_HIST.items()}
    return dict(
        capture=_CAPTURE,
        call_total=dict(CALL_TOTAL), row_total=dict(ROW_TOTAL),
        seen_total=dict(_SEEN),
        corpus_sizes={m: len(v) for m, v in CORPUS.items()},
        call_size_histogram=hist,
        n_instances=len(INSTANCES))
