"""One-shot full-pipeline runner: engine.run() (NOT the staged driver) on
the canonical Weeping Treant v1.2 STL with the canonical cfg
(SupportConfig(lift_mm=3.0)), dumping a JSON results file + a pickle of
everything the chitubox_emit machinery needs to emit + reparse-verify
afterward. Lives OUTSIDE the tracked worktree per the runner's mandate
(throwaway, not a source edit).

Usage: uv run --frozen python /Users/craig/.claude/jobs/0ddf6d41/tmp/run_engine_full.py
Run from inside the worktree (cwd matters for `scripts` import + relative
STL/corpus paths are made absolute below regardless).
"""
import sys
import os
import time
import json
import pickle

WORKTREE = "/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad/wt-emit-final"
sys.path.insert(0, os.path.join(WORKTREE, "scripts"))

import numpy as np
from support_placement import mesh_io, config, engine, tree

STL = os.path.join(os.path.dirname(WORKTREE), "Weeping Treant v1.2.stl")  # scratchpad-root symlink
RESULTS_JSON = "/Users/craig/.claude/jobs/0ddf6d41/tmp/engine_run_results.json"
STATE_PKL = "/Users/craig/.claude/jobs/0ddf6d41/tmp/engine_run_state.pkl"


def jsonable(x):
    if isinstance(x, dict):
        return {str(k): jsonable(v) for k, v in x.items()}
    if isinstance(x, (list, tuple)):
        return [jsonable(v) for v in x]
    if isinstance(x, np.ndarray):
        return jsonable(x.tolist())
    if isinstance(x, (np.floating,)):
        return float(x)
    if isinstance(x, (np.integer,)):
        return int(x)
    if isinstance(x, (np.bool_,)):
        return bool(x)
    return x


def _field(m, name, default=None):
    if isinstance(m, dict):
        return m.get(name, default)
    return getattr(m, name, default)


def main():
    print("loading STL + centering", flush=True)
    mesh = mesh_io.load_stl(STL)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    print(f"mesh: {len(mesh.faces)} faces, bounds {mesh.bounds.tolist()}", flush=True)

    cfg = config.SupportConfig(lift_mm=3.0)
    log = {}

    t0 = time.time()
    result = engine.run(mesh, cfg, log=log)
    wall_s = time.time() - t0
    print(f"engine.run() complete in {wall_s:.1f}s", flush=True)

    v = result["verify"]
    members = result["members"]
    balls = result["balls"]
    unrouted = result["unrouted"]
    contacts = result["contacts"]
    forest = result["forest"]

    gate_counts = {k: len(v[k]) for k in (
        "clearance_violations", "connectivity_violations",
        "printability_violations", "murray_violations",
        "unsupported_minima", "load_violations",
        "uncovered_island_births", "uncovered_islands")}
    gate_counts["ok"] = bool(v["ok"])
    gate_counts["removal_violations"] = len(v["removal_violations"])

    volume = result["volume"]

    kind_counts = {}
    for m in members:
        k = _field(m, "kind")
        kind_counts[k] = kind_counts.get(k, 0) + 1

    max_r = 0.0
    for m in members:
        r0 = float(_field(m, "r0"))
        r1 = float(_field(m, "r1"))
        max_r = max(max_r, r0, r1)

    max_ball_r = 0.0
    for b in balls:
        max_ball_r = max(max_ball_r, float(_field(b, "r")))

    fix_pass_iters = len([k for k in log if k.startswith("verify_pass")])

    out = dict(
        wall_s=round(wall_s, 1),
        fix_pass_iterations=fix_pass_iters,
        gates=gate_counts,
        unrouted_count=len(unrouted),
        n_contacts=len(contacts),
        n_members=len(members),
        member_kind_counts=kind_counts,
        n_balls=len(balls),
        max_structural_radius_mm=round(max_r, 4),
        max_ball_radius_mm=round(max_ball_r, 4),
        volume=dict(member_ml=round(volume["member_ml"], 4),
                    ball_ml=round(volume["ball_ml"], 4),
                    total_ml=round(volume["total_ml"], 4),
                    by_kind_ml={k: round(vv, 4) for k, vv in volume["by_kind_ml"].items()}),
        n_contacts_pre_stage=len(result.get("contacts", [])),
        log_summary={k: (v_ if not isinstance(v_, dict) else v_)
                     for k, v_ in log.items()
                     if k in ("islands", "minima", "island_births", "t_islands", "t_minima",
                               "t_island_births", "t_surface_tree", "t_total",
                               "contacts_initial", "minima_contacts_added",
                               "island_birth_contacts_added", "n_unrouted")},
        n_forest_members_total=len(forest.members),
        n_forest_dead=len(forest.dead),
    )
    # verify_pass / forest_pass per-iteration detail, jsonable
    out["verify_passes"] = {k: jsonable(vv) for k, vv in log.items() if k.startswith("verify_pass")}
    out["forest_passes"] = {k: jsonable(vv) for k, vv in log.items() if k.startswith("forest_pass")}

    with open(RESULTS_JSON, "w") as f:
        json.dump(jsonable(out), f, indent=2)
    print(f"wrote {RESULTS_JSON}", flush=True)
    print("GATE_SUMMARY " + json.dumps(jsonable(gate_counts)), flush=True)
    print("VOLUME_SUMMARY " + json.dumps(jsonable(out["volume"])), flush=True)
    print("COUNTS_SUMMARY " + json.dumps(dict(
        unrouted=out["unrouted_count"], n_contacts=out["n_contacts"],
        n_members=out["n_members"], n_balls=out["n_balls"],
        max_structural_radius_mm=out["max_structural_radius_mm"],
        fix_pass_iterations=out["fix_pass_iterations"], wall_s=out["wall_s"],
        member_kind_counts=out["member_kind_counts"])), flush=True)

    # Save everything the emit stage needs (mirrors manual_treant_v4.py's
    # pickled state: cfg, plate_z, forest, unrouted, members, balls).
    state = dict(
        cfg=cfg,
        plate_z=result["plate_z"],
        forest=forest,
        unrouted=unrouted,
        members=members,
        balls=balls,
        verify=v,
        volume=volume,
    )
    try:
        with open(STATE_PKL, "wb") as f:
            pickle.dump(state, f)
    except Exception as e:
        print(f"pickle failed ({e!r}), retrying with forest cache cleared", flush=True)
        forest._pitch_bound_cache = None
        with open(STATE_PKL, "wb") as f:
            pickle.dump(state, f)
    print(f"wrote {STATE_PKL}", flush=True)
    print("DONE", flush=True)


if __name__ == "__main__":
    main()
