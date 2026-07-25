"""Verify (a) whether rewire's wall-clock cap tripped in the canonical full run
and (b) that engine.run reproduces the delivered forest bit-identically.
Read-only measurement: compares against the archived engine_run_state.pkl.
"""
import sys
import os
import time
import pickle

import _toolpaths  # noqa: E402  (sibling module, resolved via sys.path[0])

sys.path.insert(0, _toolpaths.SCRIPTS_DIR)

from support_placement import mesh_io, config, engine

STL = _toolpaths.resolve_stl()
PKL = os.path.join(_toolpaths.scratch_dir(), "engine_run_state.pkl")


def _field(m, name):
    if isinstance(m, dict):
        return m[name]
    return getattr(m, name)


def main():
    mesh = mesh_io.load_stl(STL)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    cfg = config.SupportConfig(lift_mm=3.0)
    log = {}
    t0 = time.time()
    result = engine.run(mesh, cfg, log=log)
    print(f"run complete in {time.time()-t0:.1f}s", flush=True)

    rew = log.get("rewire", {})
    print(f"REWIRE cap_hit={rew.get('cap_hit')} sweeps={rew.get('sweeps')} "
          f"accepted={rew.get('accepted')} rejected={rew.get('rejected')} "
          f"curve_points={len(rew.get('curve', []))}", flush=True)

    with open(PKL, "rb") as f:
        ref = pickle.load(f)

    members_new, members_ref = result["members"], ref["members"]
    balls_new, balls_ref = result["balls"], ref["balls"]
    ok = len(members_new) == len(members_ref) and len(balls_new) == len(balls_ref)
    print(f"counts: members {len(members_new)} vs {len(members_ref)}, "
          f"balls {len(balls_new)} vs {len(balls_ref)}", flush=True)
    if ok:
        for a, b in zip(members_new, members_ref):
            for fld in ("kind",):
                if _field(a, fld) != _field(b, fld):
                    ok = False
            for fld in ("p0", "p1"):
                pa, pb = _field(a, fld), _field(b, fld)
                if tuple(map(float, pa)) != tuple(map(float, pb)):
                    ok = False
            for fld in ("r0", "r1"):
                if float(_field(a, fld)) != float(_field(b, fld)):
                    ok = False
            if not ok:
                break
        for a, b in zip(balls_new, balls_ref):
            if tuple(map(float, _field(a, "center"))) != tuple(map(float, _field(b, "center"))) \
               or float(_field(a, "r")) != float(_field(b, "r")):
                ok = False
                break
    print("BIT_IDENTICAL: " + ("YES" if ok else "NO"), flush=True)
    print("DONE", flush=True)


if __name__ == "__main__":
    main()
