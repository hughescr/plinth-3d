"""Staged v4 Treant driver (foreground 45s-call discipline).

Stages checkpoint to STATE_DIR so each fits one bounded call:
  prep      -- load STL, center, perception/minima/contacts/delicacy
  build     -- engine.construct (optionally first N contacts: build:N)
  rewire    -- rewire.optimize, budget = cfg.rewire_time_cap_s per call
  verify    -- engine.finish: gates + volume report
Usage: python manual_treant_v4.py <stage>   (env V4_STATE_DIR overrides /tmp/v4)
Note: the staged path skips run()'s fix-pass loop; run() stays canonical.
"""
import sys
import os
import time
import pickle

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from support_placement import mesh_io, config, engine, routing, tree, rewire
import numpy as np

STATE_DIR = os.environ.get("V4_STATE_DIR", "/tmp/v4")
STL = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..",
                                   "Weeping Treant v1.2.stl"))


def _p(name):
    return os.path.join(STATE_DIR, f"treant_v4_{name}.pkl")


def _mesh():
    mesh = mesh_io.load_stl(STL)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    return mesh


def _tree(mesh):
    return routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)


def stage_prep():
    t0 = time.time()
    mesh = _mesh()
    cfg = config.SupportConfig(lift_mm=3.0)
    pre = engine.prepare(mesh, cfg)
    with open(_p("prep"), "wb") as f:
        pickle.dump(dict(cfg=cfg, pre=pre), f)
    print(f"prep {time.time()-t0:.1f}s: {len(pre['contacts'])} contacts, "
          f"{len(pre['minima'])} minima", flush=True)


def stage_build(n=None):
    t0 = time.time()
    st = pickle.load(open(_p("prep"), "rb"))
    cfg, pre = st["cfg"], st["pre"]
    mesh = _mesh()
    forest, unrouted, carried, tip_r, contacts = engine.construct(
        mesh, pre, _tree(mesh), cfg, limit_contacts=n)
    with open(_p("build"), "wb") as f:
        pickle.dump(dict(forest=forest, unrouted=unrouted, carried=carried,
                         tip_r=tip_r, contacts=contacts), f)
    live = [m for i, m in enumerate(forest.members) if i not in forest.dead]
    vol = tree.volume_report(live, [])
    print(f"build {time.time()-t0:.1f}s: n={n or len(pre['contacts'])}, "
          f"members={len(live)}, unrouted={len(unrouted)}, "
          f"member_ml={vol['member_ml']:.2f}", flush=True)


def stage_rewire():
    t0 = time.time()
    st = pickle.load(open(_p("prep"), "rb"))
    bd = pickle.load(open(_p("build"), "rb"))
    cfg = st["cfg"]
    mesh = _mesh()
    log = {}
    rewire.optimize(bd["forest"], mesh, _tree(mesh), cfg, log=log)
    with open(_p("build"), "wb") as f:
        pickle.dump(bd, f)
    print(f"rewire {time.time()-t0:.1f}s: {log.get('rewire')}", flush=True)


def stage_verify():
    t0 = time.time()
    st = pickle.load(open(_p("prep"), "rb"))
    bd = pickle.load(open(_p("build"), "rb"))
    cfg, pre = st["cfg"], st["pre"]
    mesh = _mesh()
    out = engine.finish(mesh, pre, bd["forest"], bd["unrouted"], bd["contacts"],
                        bd["carried"], bd["tip_r"], _tree(mesh), cfg)
    with open(_p("result"), "wb") as f:
        pickle.dump(dict(out=out, unrouted=bd["unrouted"]), f)
    v = out["verify"]
    print(f"verify {time.time()-t0:.1f}s:", flush=True)
    print("gates:", {k: len(v[k]) for k in (
        "clearance_violations", "connectivity_violations",
        "printability_violations", "murray_violations",
        "unsupported_minima", "load_violations")}, flush=True)
    print("volume:", {k: round(out["volume"][k], 2)
                      for k in ("member_ml", "ball_ml", "total_ml")},
          "unrouted:", len(bd["unrouted"]), flush=True)


STAGES = dict(prep=stage_prep, rewire=stage_rewire, verify=stage_verify)

if __name__ == "__main__":
    arg = sys.argv[1] if len(sys.argv) > 1 else "prep"
    if arg.startswith("build"):
        stage_build(int(arg.split(":")[1]) if ":" in arg else None)
    else:
        STAGES[arg]()
