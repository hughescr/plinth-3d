"""Validate the buried-free-end stub fix on a full-scale run pickle.
Counts residual illegal stub members via the census axis-interval test
(census inset CD+PB+r+standoff AND the engine's own angle-corrected inset),
prints the emitted verify gates + digest for determinism comparison.
argv: <pickle_path>
"""
import sys, pickle, numpy as np
import _toolpaths
sys.path.insert(0, _toolpaths.SCRIPTS_DIR)
from support_placement import mesh_io, joints, routing
from support_placement.geometry_kernel import KernelSurface

STL = _toolpaths.resolve_stl()
pkl = sys.argv[1]
st = pickle.load(open(pkl, "rb"))
members = joints.collect_members(st["members"]); cfg = st["cfg"]
v = st["verify"]; forest = st["forest"]; unrouted = st["unrouted"]
mesh = mesh_io.load_stl(STL); mesh.apply_translation(-(mesh.bounds[0]+mesh.bounds[1])/2)
geom = KernelSurface(mesh)
stree = routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)
PB = float(routing.surface_tree_pitch_bound_mm(stree, percentile=cfg.surface_pitch_bound_percentile))

def gm(m, k): return m[k] if isinstance(m, dict) else getattr(m, k)

def illegal(mi, use_pb):
    m = members[mi]; p0 = np.asarray(gm(m, "p0"), float); p1 = np.asarray(gm(m, "p1"), float)
    r0 = float(gm(m, "r0")); r1 = float(gm(m, "r1")); L = float(np.linalg.norm(p1-p0))
    iv = geom.penetration_intervals(p0, p1, r0, r1, rays="axis", min_illegal_len_mm=0.05)
    if use_pb:
        inset0 = cfg.contact_depth_mm + PB + r0 + cfg.standoff_mm
    else:
        u = (p1-p0)/L
        _d,_c,_t,ns = geom.project(p0[None]); n = np.asarray(ns[0], float); nn = np.linalg.norm(n)
        cos = max(abs(float(np.dot(u, n/nn))), 0.5) if nn > 1e-9 else 1.0
        inset0 = cfg.contact_depth_mm + (r0+cfg.standoff_mm)/cos
    lo, hi = inset0, L
    if hi-lo <= 1e-9: return False
    return any(min(x["s_b"], hi)-max(x["s_a"], lo) > 0.05 for x in iv)

stubs = [i for i, m in enumerate(members) if gm(m, "kind") == "stub"]
ill_census = [mi for mi in stubs if illegal(mi, True)]
ill_engine = [mi for mi in stubs if illegal(mi, False)]
print(f"PICKLE {pkl}")
print(f"emitted members={len(st['members'])} stubs={len(stubs)} balls={len(st['balls'])}")
print(f"n_unrouted={len(unrouted)} n_quarantined={len(getattr(forest,'quarantined',set()))}")
print(f"ILLEGAL STUB MEMBERS (census inset CD+PB+r+standoff) = {len(ill_census)}")
print(f"ILLEGAL STUB MEMBERS (engine angle-corrected inset)  = {len(ill_engine)}")
print("verify gates:", {k: (len(v[k]) if isinstance(v[k], list) else v[k])
      for k in ["ok","uncovered_islands","clearance_violations","ball_breach_violations",
                "unsupported_minima","uncovered_island_births","connectivity_violations",
                "printability_violations","murray_violations"]})
print("digest combined_sha =", st["digest"]["combined_sha"])
print("volume total_ml =", st["volume"]["total_ml"])
