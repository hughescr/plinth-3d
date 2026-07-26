"""Manual end-to-end run against the real Weeping Treant model, staged so
each step fits the sandbox's 45s-per-call ceiling. Not a pytest file (not
named test_*.py). Usage: `python3 manual_run_treant.py <stage>` where
stage is one of: islands, contacts, route, verify, report. State
checkpoints to /tmp/treant_v2_state.pkl between stages.
"""
import sys
import os
import json
import time
import pickle

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from support_placement import mesh_io, config, engine, corpus, perception, placement, routing, braces as braces_mod, struts, verify

STL_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "Weeping Treant v1.2.stl"))
CORPUS_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "supported-treant.chitubox"))
STATE_PATH = "/tmp/treant_v2_state.pkl"


def load_state():
    with open(STATE_PATH, "rb") as f:
        return pickle.load(f)


def save_state(**kw):
    state = {}
    if os.path.isfile(STATE_PATH):
        state = load_state()
    state.update(kw)
    with open(STATE_PATH, "wb") as f:
        pickle.dump(state, f)
    return state


def stage_islands():
    t0 = time.time()
    mesh = mesh_io.load_stl(STL_PATH)
    cfg = config.SupportConfig()
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(mesh, cfg, plate_z)
    print(f"loaded {len(mesh.faces)} faces, {len(islands)} islands in {time.time()-t0:.1f}s")
    save_state(cfg=cfg, islands=islands)
    print("saved stage=islands")


def stage_contacts():
    state = load_state()
    mesh = mesh_io.load_stl(STL_PATH)
    cfg = state["cfg"]
    islands = state["islands"]
    t0 = time.time()
    contacts = placement.sample_candidates(mesh, islands, cfg)
    print(f"{len(contacts)} contacts in {time.time()-t0:.1f}s")
    t0 = time.time()
    points, normals, weights, _query = perception.delicacy_field(mesh, cfg)
    print(f"delicacy_field in {time.time()-t0:.1f}s")
    # the closure `query` fn isn't picklable -- save the raw arrays and
    # rebuild a cKDTree-backed query() in whichever stage needs it.
    save_state(contacts=contacts, delicacy_points=points, delicacy_normals=normals,
               delicacy_weights=weights)
    print("saved stage=contacts")


def stage_route():
    import numpy as np
    state = load_state()
    mesh = mesh_io.load_stl(STL_PATH)
    cfg = state["cfg"]
    islands = state["islands"]
    contacts = state["contacts"]
    sample_points = state["delicacy_points"]
    sample_normals = state["delicacy_normals"]
    sample_delicacy = state["delicacy_weights"]
    from scipy.spatial import cKDTree
    _tree = cKDTree(sample_points)

    def delicacy_query(pts):
        _, i = _tree.query(np.atleast_2d(pts), workers=-1)
        return sample_delicacy[i]

    zmin = float(mesh.bounds[0][2])
    anchor_safe = sample_normals[:, 2] >= -np.sin(np.radians(cfg.overhang_angle_deg))
    safe_points = sample_points[anchor_safe]
    safe_delicacy = sample_delicacy[anchor_safe]

    t0 = time.time()
    carried = placement.load_partition(mesh, islands, contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    contact_delicacy = delicacy_query(np.array([c["position"] for c in contacts]))
    print(f"load_partition+sizing in {time.time()-t0:.1f}s")

    t0 = time.time()
    routes = []
    anchor_kinds = {"plate": 0, "model": 0}
    for c, r, cd in zip(contacts, tip_r, contact_delicacy):
        plate_pt = routing.plate_anchor(c, zmin, cfg)
        probe = routing.route_to_anchor(c, plate_pt, r_tip=r, r_shaft=r, cfg=cfg)
        decision = struts.choose_anchor(c, probe.free_length_mm, float(cd),
                                         safe_points, safe_delicacy, cfg)
        anchor_kinds[decision["kind"]] += 1
        anchor_pt = decision["anchor"] if decision["kind"] == "model" else plate_pt
        shaft_r, n_braces, eff_len = placement.shaft_radius_for_length_mm(
            decision["free_length_mm"], cfg)
        route = routing.route_to_anchor(c, anchor_pt, r_tip=r, r_shaft=shaft_r,
                                         cfg=cfg, anchor_kind=decision["kind"])
        route.n_braces = n_braces
        routes.append(route)
    print(f"routed {len(routes)} in {time.time()-t0:.1f}s, anchors={anchor_kinds}")

    t0 = time.time()
    made_braces, brace_graph = braces_mod.build_braces(routes, cfg)
    print(f"braces: {len(made_braces)} in {time.time()-t0:.1f}s")

    save_state(carried=carried, tip_r=tip_r, routes=routes, braces=made_braces,
               anchor_kinds=anchor_kinds)
    print("saved stage=route")


def stage_surface_tree():
    mesh = mesh_io.load_stl(STL_PATH)
    t0 = time.time()
    tree = routing.build_surface_tree(mesh)
    print(f"surface_tree built in {time.time()-t0:.1f}s")
    save_state(surface_tree=tree)
    print("saved stage=surface_tree")


def stage_repair():
    state = load_state()
    mesh = mesh_io.load_stl(STL_PATH)
    cfg = state["cfg"]
    routes = state["routes"]
    contacts = state["contacts"]
    tip_r = state["tip_r"]
    surface_tree = state["surface_tree"]
    t0 = time.time()
    routes, n_repaired = engine._repair_clearance_with_detours(
        mesh, routes, contacts, tip_r, cfg, surface_tree=surface_tree)
    print(f"repaired {n_repaired} in {time.time()-t0:.1f}s")
    save_state(routes=routes)
    print("saved stage=repair")


def stage_verify():
    state = load_state()
    mesh = mesh_io.load_stl(STL_PATH)
    cfg = state["cfg"]
    islands = state["islands"]
    contacts = state["contacts"]
    routes = state["routes"]
    carried = state["carried"]
    tip_r = state["tip_r"]
    surface_tree = state["surface_tree"]
    t0 = time.time()
    report = verify.self_verify(mesh, islands, contacts, routes, carried, tip_r, cfg,
                                 surface_tree=surface_tree)
    print(f"verify in {time.time()-t0:.1f}s: ok={report['ok']}, "
          f"uncovered={len(report['uncovered_islands'])}, "
          f"clearance={len(report['clearance_violations'])}, "
          f"load={len(report['load_violations'])}")
    save_state(report=report)
    print("saved stage=verify")


def stage_report():
    import numpy as np
    from scipy.spatial import cKDTree
    state = load_state()
    contacts = state["contacts"]
    routes = state["routes"]
    braces = state["braces"]
    report = state["report"]
    anchor_kinds = state["anchor_kinds"]

    positions = np.array([c["position"] for c in contacts])
    tree = cKDTree(positions)
    dist, _ = tree.query(positions, k=2)
    median_spacing = float(np.median(dist[:, 1]))

    out = dict(
        n_supports=len(contacts), anchor_kinds=anchor_kinds, n_braces=len(braces),
        verify_ok=report["ok"], n_uncovered=len(report["uncovered_islands"]),
        n_clearance_violations=len(report["clearance_violations"]),
        n_load_violations=len(report["load_violations"]),
        v2_median_spacing_mm=round(median_spacing, 2),
    )
    if os.path.isfile(CORPUS_PATH):
        stats = corpus.analyze(CORPUS_PATH)
        out["corpus_n_supports"] = stats["n_supports"]
        out["corpus_median_spacing_mm"] = round(stats["tip_spacing_mm"]["median"], 2)

    print(json.dumps(out, indent=2))
    out_path = os.path.join(os.path.dirname(__file__), "..", "treant_v2_results.json")
    with open(out_path, "w") as f:
        json.dump(out, f, indent=2)
    print(f"wrote {out_path}")


STAGES = dict(islands=stage_islands, contacts=stage_contacts, route=stage_route,
              surface_tree=stage_surface_tree, repair=stage_repair, verify=stage_verify,
              report=stage_report)

if __name__ == "__main__":
    stage = sys.argv[1] if len(sys.argv) > 1 else "islands"
    STAGES[stage]()
