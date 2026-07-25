#!/usr/bin/env python
"""Micro-benchmark: geom_kernel (Rust) vs ExactSurface oracle throughput, plus
kernel thread-count scaling (spec SPEC-RUST-KERNEL.md sec 6/8, R1).

Never gates on wall time (Craig's correctness-over-speed ruling) — it prints
numbers so regressions in per-call FFI overhead and parallel scaling are
visible. Run:

    .venv/bin/python scripts/tools/bench_kernel.py [subdivisions]

subdivisions controls the icosphere size (default 5 ~ 20480 faces).
"""
import os
import sys
import time

import numpy as np
import trimesh

import _toolpaths  # noqa: E402  (sibling module, resolved via sys.path[0])

sys.path.insert(0, _toolpaths.SCRIPTS_DIR)
from support_placement import geometry_exact as GE  # noqa: E402
from support_placement import geometry_kernel as GK  # noqa: E402


def _mesh(subdiv):
    s = trimesh.creation.icosphere(subdivisions=subdiv, radius=10.0)
    return trimesh.Trimesh(vertices=s.vertices.copy(), faces=s.faces.copy(),
                           process=False)


def _time(fn, reps=3):
    best = float("inf")
    for _ in range(reps):
        t0 = time.perf_counter()
        fn()
        best = min(best, time.perf_counter() - t0)
    return best


def main():
    subdiv = int(sys.argv[1]) if len(sys.argv) > 1 else 5
    mesh = _mesh(subdiv)
    nf = len(mesh.faces)
    print(f"=== geom_kernel micro-benchmark (icosphere subdiv={subdiv}, "
          f"{nf} faces) ===")

    tb = time.perf_counter()
    o = GE.ExactSurface(mesh)
    t_oracle_build = time.perf_counter() - tb
    tb = time.perf_counter()
    k = GK.KernelSurface(mesh)
    t_kernel_build = time.perf_counter() - tb
    print(f"build: oracle {t_oracle_build*1e3:8.1f} ms   "
          f"kernel {t_kernel_build*1e3:8.1f} ms  "
          f"(t_build_ms={k.build_log['t_build_ms']})")

    rng = np.random.default_rng(0)
    lo, hi = mesh.bounds
    N = 20000
    pts = lo - 0.3 * (hi - lo) + rng.uniform(0, 1.6, size=(N, 3)) * (hi - lo)

    print(f"\n-- batch throughput ({N} queries), all-core kernel --")
    os.environ.pop("GEOM_KERNEL_THREADS", None)
    for label, ofn, kfn in [
        ("query", lambda: o.query(pts), lambda: k.query(pts)),
        ("inside", lambda: o.inside(pts), lambda: k.inside(pts)),
        ("project", lambda: o.project(pts), lambda: k.project(pts)),
    ]:
        to = _time(ofn)
        tk = _time(kfn)
        print(f"  {label:9s} oracle {to*1e3:9.1f} ms   kernel {tk*1e3:8.2f} ms  "
              f"speedup {to/tk:6.1f}x   ({tk/N*1e6:6.2f} us/query kernel)")

    # segment_clearance batch
    Ns = 4000
    p0 = lo - 0.2 * (hi - lo) + rng.uniform(0, 1.4, size=(Ns, 3)) * (hi - lo)
    p1 = p0 + rng.uniform(-1, 1, size=(Ns, 3)) * 3.0
    to = _time(lambda: o.segment_clearance(p0, p1, 0.5, 0.5, 0.5))
    tk = _time(lambda: k.segment_clearance(p0, p1, 0.5, 0.5, 0.5))
    print(f"  {'segclear':9s} oracle {to*1e3:9.1f} ms   kernel {tk*1e3:8.2f} ms  "
          f"speedup {to/tk:6.1f}x   ({tk/Ns*1e6:6.2f} us/seg kernel)")

    # per-call FFI overhead (single-point scalar calls)
    print("\n-- per-call FFI overhead (scalar calls) --")
    single = pts[0]
    t = _time(lambda: [k.query(single) for _ in range(2000)], reps=2) / 2000
    print(f"  scalar query round-trip: {t*1e6:6.2f} us/call")

    print("\n-- kernel thread-count scaling (query, "
          f"{N} pts) --")
    base = None
    for env in ["1", "2", "4", "8", "0"]:
        os.environ["GEOM_KERNEL_THREADS"] = env
        cur = k.current_threads()
        tk = _time(lambda: k.query(pts))
        if base is None:
            base = tk
        print(f"  threads={env:>2s} (eff={cur:2d}): {tk*1e3:8.2f} ms   "
              f"scaling {base/tk:5.2f}x vs 1-thread")
    os.environ.pop("GEOM_KERNEL_THREADS", None)

    # verify determinism across the sweep
    outs = []
    for env in ["1", "0"]:
        os.environ["GEOM_KERNEL_THREADS"] = env
        d, ti = k.query(pts)
        outs.append((d.tobytes(), ti.tobytes()))
    os.environ.pop("GEOM_KERNEL_THREADS", None)
    print(f"\n  determinism 1-thread vs all-core: "
          f"{'BIT-IDENTICAL' if outs[0] == outs[1] else 'DIFFER (BUG!)'}")


if __name__ == "__main__":
    main()
