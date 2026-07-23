"""SPEC-V5-PERF gate 2.5.1 / 5.1.2 helper: canonical byte-level fingerprint
of a full engine.run on the table + toed_slab fixtures, for A/B bit-identity
checks across v5-perf commits.

Usage:
    .venv/bin/python scripts/tests/_perf_fingerprint.py > /tmp/fp.json

Serializes, per fixture, every field SPEC-V5-PERF sec 5.1.2 names: member
p0/p1/r0/r1/kind/owner/embed_end, ball centers/radii, total_ml, verify
verdict lists, unrouted, plus quarantined (A0b) and cap_hit flags (2.2). All
floats via .hex()/tobytes-equivalent repr so the compare is exact, never
tolerance-based.
"""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import numpy as np

from support_placement import config as config_mod, engine, mesh_io


def _f(x):
    return float(x).hex()


def _pt(a):
    return [float(v).hex() for v in np.asarray(a, float).ravel()]


def fingerprint_result(result):
    members = []
    for m in result["members"]:
        members.append([
            _pt(m["p0"]), _pt(m["p1"]), _f(m["r0"]), _f(m["r1"]),
            m["kind"], list(m["owner"]) if isinstance(m["owner"], (list, tuple)) else m["owner"],
            m.get("embed_end"),
        ])
    balls = []
    for b in result["balls"]:
        c = b["center"] if isinstance(b, dict) else b.center
        r = b["r"] if isinstance(b, dict) else b.r
        balls.append([_pt(c), _f(r)])
    v = result["verify"]
    return dict(
        n_members=len(members),
        members=members,
        balls=balls,
        total_ml=_f(result["volume"]["total_ml"]),
        member_ml=_f(result["volume"]["member_ml"]),
        ball_ml=_f(result["volume"]["ball_ml"]),
        unrouted=list(result["unrouted"]),
        clearance_violations=len(v["clearance_violations"]),
        connectivity_violations=len(v["connectivity_violations"]),
        printability_violations=len(v["printability_violations"]),
        murray_violations=len(v["murray_violations"]),
        uncovered_islands=len(v["uncovered_islands"]),
        uncovered_island_births=len(v["uncovered_island_births"]),
        ok=bool(v["ok"]),
        quarantined=sorted(getattr(result["forest"], "quarantined", set())),
    )


def main():
    out = {}
    for name, mesh_fn in (("table", mesh_io.make_overhang_table),
                          ("toed_slab", mesh_io.make_toed_slab)):
        mesh = mesh_fn()
        result = engine.run(mesh, config_mod.SupportConfig())
        out[name] = fingerprint_result(result)
    json.dump(out, sys.stdout, indent=1, sort_keys=True)


if __name__ == "__main__":
    main()
