"""Emit a CLEAN v2-only supported Treant project: run the v2 engine on the
real Weeping Treant STL and REPLACE the corpus project's supports with the
result (write_replace -- no stacked generations while iterating).

Fixes vs the first emission attempt (which appended raw-STL-frame records):
  1. FRAME: record coordinates are model-centered (verified: corpus tips
     sit at median 0.30mm from the centered STL surface = contact depth;
     raw frame was off by the 64mm z-center). The mesh is centered BEFORE
     engine.run so every route/brace/pad is natively in record frame.
  2. PLATE: scene lift is derived from the project itself (pad blob z vs
     centered model zmin -> 3.0mm for this corpus), not from cfg default.
  3. RAFT: v2 pads are tied into a connected raft with minimum-spanning-
     tree bridge records (the corpus raft is pads + bridge boxes; pads
     alone slice as disconnected dots).

Not a pytest file. Usage: python3 manual_emit_treant.py [run|emit]
State checkpoints to /tmp/treant_emit_state.pkl between stages.
"""
import sys
import os
import time
import pickle

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from support_placement import mesh_io, config, engine, chitubox_emit as ce
import chitubox_inject as ci
import numpy as np

STL_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "Weeping Treant v1.2.stl"))
CORPUS_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "supported-treant.chitubox"))
OUT_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..", "treant_v3_supports.chitubox"))
STATE_PATH = "/tmp/treant_emit_state.pkl"


def scene_frame(proj, mesh):
    """(center, lift_mm) for this scene: records are model-centered; the
    plate z comes from the corpus pads' own geometry."""
    center = (mesh.bounds[0] + mesh.bounds[1]) / 2.0
    pad = next(r for r in proj.records if r.type == ci.T_PAD
               and np.allclose(r.start[:2], r.end[:2]))
    plate_z = float(proj.blob(pad)[:, 2].min())
    zmin_centered = float((mesh.bounds[0] - center)[2])
    return center, round(zmin_centered - plate_z, 3)


def stage_run():
    t0 = time.time()
    mesh = mesh_io.load_stl(STL_PATH)
    proj = ci.Project(CORPUS_PATH)
    center, lift = scene_frame(proj, mesh)
    print(f"scene frame: center={np.round(center,3)}, lift={lift}mm")
    mesh.apply_translation(-center)  # engine now works in record frame
    cfg = config.SupportConfig(lift_mm=lift)
    result = engine.run(mesh, cfg)
    print(f"engine.run: {time.time()-t0:.1f}s, {len(result['contacts'])} supports, "
          f"{len(result['trunks'])} trunks, verify ok={result['verify']['ok']}", flush=True)
    print("log:", {k: v for k, v in result["log"].items()
                   if k.startswith(("trunks", "verify"))})
    with open(STATE_PATH, "wb") as f:
        pickle.dump(dict(routes=result["routes"], tip_r=result["tip_r_mm"],
                         carried=result["carried_area_mm2"],
                         braces=result["braces"], trunks=result["trunks"],
                         risers=result["risers"],
                         plate_z=result["plate_z"], cfg=cfg), f)
    print("saved stage=run")


def stage_emit():
    t0 = time.time()
    with open(STATE_PATH, "rb") as f:
        state = pickle.load(f)
    routes, tip_r, braces = state["routes"], state["tip_r"], state["braces"]
    trunks, plate_z, cfg = state["trunks"], state["plate_z"], state["cfg"]
    proj = ci.Project(CORPUS_PATH)
    pad_template = next(r for r in proj.records if r.type == ci.T_PAD
                         and np.allclose(r.start[:2], r.end[:2]))
    _, bridge_templates = proj.supports()
    bridge_template = bridge_templates[0]

    next_off = proj.mesh_off + proj.mesh_size
    new_records = []

    # per-contact chains: contact ball + tip cone, elbow ball, diagonal
    # shoot, junction ball (trunk-attached; no per-contact pads)
    for route, r in zip(routes, tip_r):
        r_shoot = next((s.r0 for s in route.segments if s.kind == "shoot"),
                        float(r))
        hub_r = getattr(route, "hub_ball_r", None)
        if hub_r is not None:
            joint_r = hub_r  # bundled: ball matches the riser at the hub
        elif route.trunk_idx is not None:
            joint_r = trunks[route.trunk_idx].r
        else:
            # model-anchored strut: adsorbed foot ball, slightly larger
            # than the rod for adhesion (no trunk to size against)
            joint_r = float(r_shoot) * 1.4
        chain = ce.synth_v3_support(route, r_tip=float(r), r_shoot=float(r_shoot),
                                     trunk_r=joint_r, cfg=cfg, next_off=next_off)
        for rec, blob in chain:
            new_records.append((rec, blob))
            next_off += len(blob)

    # trunks: flare + column + dome (plate) or Y-diagonal + column
    # (branch); pads only at PLATE trunk feet
    pad_xy = []
    for trunk in trunks:
        parent_xy = (trunks[trunk.parent_idx].xy
                     if trunk.foot_kind == "branch" and trunk.parent_idx is not None
                     else None)
        chain = ce.synth_v3_trunk(trunk, plate_z, cfg, next_off, parent_xy=parent_xy)
        for rec, blob in chain:
            new_records.append((rec, blob))
            next_off += len(blob)
        if trunk.foot_kind == "plate":
            pad_rec, pad_blob = ce.place_pad(proj, pad_template, trunk.xy, next_off)
            new_records.append((pad_rec, pad_blob))
            next_off += len(pad_blob)
            pad_xy.append(np.asarray(trunk.xy, float))

    # shared risers from shoot bundling: rod + end balls
    for rs in state.get("risers", []):
        rec, blob = ce.brace_record(rs, next_off, cfg)
        new_records.append((rec, blob))
        next_off += len(blob)

    for brace in braces:
        # brace dict indices are trunk ids; geometry is already absolute
        rec, blob = ce.brace_record(brace, next_off, cfg)
        new_records.append((rec, blob))
        next_off += len(blob)

    # Raft connectivity: minimum spanning tree over pad positions, one
    # bridge record per MST edge (the corpus raft is exactly pads +
    # bridge boxes; pads alone slice as disconnected dots).
    n_bridges = 0
    if len(pad_xy) >= 2:
        from scipy.spatial.distance import pdist, squareform
        from scipy.sparse.csgraph import minimum_spanning_tree
        P = np.array(pad_xy)
        D = squareform(pdist(P))
        mst = minimum_spanning_tree(D).tocoo()
        for a, b in zip(mst.row, mst.col):
            rec, blob = ci.make_bridge(proj, [*P[a], 0.0], [*P[b], 0.0],
                                        template=bridge_template, next_off=next_off)
            new_records.append((rec, blob))
            next_off += len(blob)
            n_bridges += 1
        print(f"raft: {len(pad_xy)} pads, {n_bridges} MST bridges, "
              f"longest {float(D[mst.row, mst.col].max()):.1f}mm")

    print(f"built {len(new_records)} records (CLEAN replace, corpus supports dropped)")
    proj.write_replace(OUT_PATH, new_records)
    print(f"wrote {OUT_PATH} in {time.time()-t0:.1f}s", flush=True)

    reparsed = ci.Project(OUT_PATH)
    groups, bridges_out = reparsed.supports()
    tips = np.array([g[0].start for g in groups])
    print(f"reparse: {reparsed.n_recs} records = {len(groups)} supports "
          f"+ {len(bridges_out)} bridges (corpus had {proj.n_recs})")
    print(f"tip z range: {tips[:,2].min():.1f}..{tips[:,2].max():.1f} "
          f"(model centered: -64.0..64.0)")


STAGES = dict(run=stage_run, emit=stage_emit)

if __name__ == "__main__":
    STAGES[sys.argv[1] if len(sys.argv) > 1 else "run"]()
