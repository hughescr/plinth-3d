"""R3 (ball-gate census-parity semantics) reconstruction on the archived
run pickle: recompute verify.ball_breach_report over the pickle's own
members/balls/cfg against a freshly-built KernelSurface oracle on the real
STL, split the combined result by `source` exactly as self_verify now does,
and confirm the gated (ball-sourced) count matches the independent census's
antipodal-span true-breach count for the SAME forest (rerun_
penetration_census.json's n_breaching_balls, computed from this exact
pickle -- census_core.py's CENSUS_STATE default IS this file).

argv: <pickle_path> [census_summary_json]
"""
import sys, os, json, pickle, numpy as np
sys.path.insert(0, "/Users/craig/code/hughescr/dnd-campaigns/.worktrees/wt-perf/scripts")
from support_placement import mesh_io, joints, verify, routing
from support_placement.geometry_kernel import KernelSurface

STL = "/Users/craig/code/hughescr/dnd-campaigns/Weeping Treant v1.2.stl"
HERE = os.path.dirname(os.path.abspath(__file__))
pkl_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "..", "..", "..",
    "..", "..", "..", "..", "v5p_treant_run1.pkl")  # not expected to be used; pass explicitly
census_json = sys.argv[2] if len(sys.argv) > 2 else os.path.join(
    HERE, "results", "rerun_penetration_census.json")

st = pickle.load(open(pkl_path, "rb"))
members = st["members"]; balls = st["balls"]; cfg = st["cfg"]
digest = st["digest"]

mesh = mesh_io.load_stl(STL)
mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2)
geom = KernelSurface(mesh)

raw = verify.ball_breach_report(geom, members, balls, cfg)
ball_only = [v for v in raw if v.get("source") == "ball"]
endcap_only = [v for v in raw if v.get("source") == "endcap"]
other = [v for v in raw if v.get("source") not in ("ball", "endcap")]

# Cross-check against self_verify's own split (the actual code path a real
# engine.run/verify call exercises), not just the module-level filter above.
# forest/unrouted/islands/contacts deliberately omitted -- this script's
# purpose is the ball_breach split, not a full gate re-run (removal_report
# alone is ~300 trees of handle-run search; skip it here, it is untouched
# by R3 and already covered by the acceptance evidence at ba378aa).
surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)
rep = verify.self_verify(mesh, [], [], members, np.array([]), np.array([]), cfg,
                          st["plate_z"], surface_tree=surface_tree,
                          geom=geom, balls=balls)

census_n = None
if os.path.exists(census_json):
    census_n = json.load(open(census_json))["summary"]["n_breaching_balls"]

print(f"PICKLE {pkl_path}")
print(f"digest combined_sha = {digest['combined_sha']}")
print(f"n_members={len(members)} n_balls={len(balls)}")
print(f"ball_breach_report RAW (combined)     = {len(raw)}")
print(f"  source=ball                          = {len(ball_only)}")
print(f"  source=endcap                        = {len(endcap_only)}")
print(f"  source=other (unexpected)             = {len(other)}")
print(f"self_verify ball_breach_violations      = {len(rep['ball_breach_violations'])}")
print(f"self_verify embed_end_breach_violations = {len(rep['embed_end_breach_violations'])}")
print(f"census n_breaching_balls (same forest, {os.path.basename(census_json)}) = {census_n}")

ok = True
if len(ball_only) != len(rep["ball_breach_violations"]):
    print("MISMATCH: module-level ball-only filter != self_verify's gated list"); ok = False
if len(endcap_only) != len(rep["embed_end_breach_violations"]):
    print("MISMATCH: module-level endcap-only filter != self_verify's report-only list"); ok = False
if other:
    print(f"UNEXPECTED: {len(other)} entries with neither source=ball nor source=endcap"); ok = False
if census_n is not None and len(rep["ball_breach_violations"]) != census_n:
    print(f"MISMATCH: gated ball_breach_violations={len(rep['ball_breach_violations'])} "
          f"!= census n_breaching_balls={census_n}")
    ok = False
print("RESULT:", "PASS" if ok else "FAIL")
sys.exit(0 if ok else 1)
