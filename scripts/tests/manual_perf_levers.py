"""SPEC-V5-PERF sec 5.3 measurement harness for the CLASS-B levers.

For each (lever, value) it runs the table + toed_slab fixtures through the full
engine.run with the lever OFF and ON, and prints one row per (lever, value,
fixture): determinism (two ON runs bit-identical), the UNGATED verify verdict
(hard gates must stay zero), the quality delta (total_ml, member/ball/unrouted
counts, uncovered islands/births) vs lever-off, and the stage wall delta.

Usage: .venv/bin/python scripts/tests/manual_perf_levers.py
Query counts / determinism are the load-bearing evidence; wall is secondary
(shared host, SPEC R-P8).
"""
import json
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import dataclasses

from support_placement import config as config_mod, engine, mesh_io

FIXTURES = (("table", mesh_io.make_overhang_table),
            ("toed_slab", mesh_io.make_toed_slab))

LEVERS = [
    ("perf_beam_k", [4, 8, 16]),
    ("perf_anchor_cap", [2, 4, 8]),
    ("perf_seed_spacing_factor", [1.2, 1.4]),
    ("perf_fix_iter_caps", [(1, 1), (1, 2)]),
    ("perf_rewire_work_budget", [50, 200]),
]


def _serialize(result):
    out = []
    for m in result["members"]:
        out.append((m["p0"].tobytes(), m["p1"].tobytes(), m["r0"], m["r1"],
                    m["kind"]))
    return out


def _summary(result, wall):
    v = result["verify"]
    hard = (len(v["clearance_violations"]) + len(v["connectivity_violations"])
            + len(v["printability_violations"]) + len(v["murray_violations"])
            + len(v["uncovered_island_births"]))
    return dict(
        total_ml=round(result["volume"]["total_ml"], 4),
        n_members=len(result["members"]), n_balls=len(result["balls"]),
        n_unrouted=len(result["unrouted"]),
        n_uncovered_islands=len(v["uncovered_islands"]),
        hard_gate_violations=hard, verify_ok=bool(v["ok"]),
        wall_s=round(wall, 2))


def _run(cfg, mesh_fn):
    t = time.time()
    r = engine.run(mesh_fn(), cfg)
    return r, time.time() - t


def main():
    rows = []
    base = {}
    for name, mesh_fn in FIXTURES:
        r, w = _run(config_mod.SupportConfig(), mesh_fn)
        base[name] = (_summary(r, w), _serialize(r))
    for field, values in LEVERS:
        for val in values:
            for name, mesh_fn in FIXTURES:
                cfg = dataclasses.replace(config_mod.SupportConfig(), **{field: val})
                try:
                    r1, w1 = _run(cfg, mesh_fn)
                    r2, _ = _run(cfg, mesh_fn)
                    deterministic = _serialize(r1) == _serialize(r2)
                    s = _summary(r1, w1)
                except Exception as e:  # B4 emission-gate raise etc.
                    rows.append(dict(lever=field, value=val, fixture=name,
                                     error=str(e)[:120]))
                    continue
                b = base[name][0]
                rows.append(dict(
                    lever=field, value=val, fixture=name,
                    deterministic=deterministic,
                    hard_gate_violations=s["hard_gate_violations"],
                    verify_ok=s["verify_ok"],
                    d_total_ml=round(s["total_ml"] - b["total_ml"], 4),
                    d_members=s["n_members"] - b["n_members"],
                    d_unrouted=s["n_unrouted"] - b["n_unrouted"],
                    d_uncovered=s["n_uncovered_islands"] - b["n_uncovered_islands"],
                    base_total_ml=b["total_ml"], on_total_ml=s["total_ml"],
                    d_wall_s=round(s["wall_s"] - b["wall_s"], 2)))
    print(json.dumps(dict(baseline={k: v[0] for k, v in base.items()},
                          rows=rows), indent=1, default=str))


if __name__ == "__main__":
    main()
