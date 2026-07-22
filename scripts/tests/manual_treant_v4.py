"""Staged v4 Treant driver (foreground 45s-call discipline).

Stages checkpoint to STATE_DIR so each fits one bounded call:
  prep      -- load STL, center, perception/minima/contacts/delicacy
  build     -- engine.construct (optionally first N contacts: build:N)
  rewire    -- rewire.optimize, budget = cfg.rewire_time_cap_s per call
  brace     -- engine.brace_and_size: bracing + transient resize fixed
               point (design-v4.md #8/#9, Phase 4)
  verify    -- engine.finish: gates + volume report
  emit      -- chitubox_emit.synth_member_chain -> treant_v4_interim.chitubox,
               INTERIM review artifact of current forest state (no bracing
               yet if run before `brace` -- tall columns honestly oversized,
               Phase 4 fixes that; see chitubox_emit.py for conventions)
Usage: python manual_treant_v4.py <stage>   (env V4_STATE_DIR overrides /tmp/v4)
Note: the staged path skips run()'s fix-pass loop and its extra
rewire/brace passes; run() stays canonical for the full pipeline.
"""
import sys
import os
import time
import pickle

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from support_placement import mesh_io, config, engine, routing, tree, rewire, chitubox_emit as ce
import chitubox_inject as ci
import numpy as np

STATE_DIR = os.environ.get("V4_STATE_DIR", "/tmp/v4")
STL = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..",
                                   "Weeping Treant v1.2.stl"))
CORPUS_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..",
                                           "supported-treant.chitubox"))
OUT_PATH = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "..",
                                        "treant_v4_interim.chitubox"))


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


def stage_brace():
    t0 = time.time()
    st = pickle.load(open(_p("prep"), "rb"))
    bd = pickle.load(open(_p("build"), "rb"))
    cfg, pre = st["cfg"], st["pre"]
    mesh = _mesh()
    log = {}
    engine.brace_and_size(mesh, bd["forest"], bd["unrouted"], _tree(mesh),
                          pre["plate_z"], cfg, log=log)
    with open(_p("build"), "wb") as f:
        pickle.dump(bd, f)
    live = [m for i, m in enumerate(bd["forest"].members) if i not in bd["forest"].dead]
    vol = tree.volume_report(live, [])
    print(f"brace {time.time()-t0:.1f}s: {log.get('brace_and_size')}", flush=True)
    print(f"members={len(live)}, braces={len(bd['forest'].braces)}, "
          f"member_ml={vol['member_ml']:.2f}", flush=True)


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


def stage_emit():
    """INTERIM review emission (not final -- see module docstring): emits
    whatever forest state is currently checkpointed (run `rewire`/`brace`
    stages first as desired; if `brace` was never run this honestly
    carries Phase 4's known caveat -- tall columns oversized, no cross-
    bracing yet). Mirrors manual_emit_treant.stage_emit's pad+MST-raft
    structure, but member/joint-ball geometry comes from chitubox_emit.
    synth_member_chain over the v4 Forest instead of v3's routes/trunks/
    risers/braces reconstruction."""
    t0 = time.time()
    st = pickle.load(open(_p("prep"), "rb"))
    bd = pickle.load(open(_p("build"), "rb"))
    res = pickle.load(open(_p("result"), "rb"))
    cfg, pre = st["cfg"], st["pre"]
    plate_z = pre["plate_z"]
    forest, unrouted = bd["forest"], bd["unrouted"]
    out = res["out"]
    members, balls = out["members"], out["balls"]

    proj = ci.Project(CORPUS_PATH)
    pad_template = next(r for r in proj.records if r.type == ci.T_PAD
                         and np.allclose(r.start[:2], r.end[:2]))
    _, bridge_templates = proj.supports()
    bridge_template = bridge_templates[0]

    next_off = proj.mesh_off + proj.mesh_size
    new_records = ce.synth_member_chain(members, balls, cfg, next_off)
    for _, blob in new_records:
        next_off += len(blob)
    print(f"synth: {len(new_records)} member records, {len(balls)} joint balls "
          f"({time.time()-t0:.1f}s)", flush=True)

    # plate-root feet: Forest roots (parent is None) whose foot actually
    # lands on the plate (as opposed to a model-anchor strut root, whose
    # p1 sits on the model surface, not at plate_z). Unrouted/dead are
    # excluded the same way live_member_lists does.
    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted}
    pad_xy = []
    for idx in forest.roots:
        if idx in forest.dead or idx in unrouted_stub_idx:
            continue
        m = forest.members[idx]
        if abs(float(m.p1[2]) - plate_z) < 1e-3:
            pad_xy.append(np.asarray(m.p1[:2], float))

    for xy in pad_xy:
        pad_rec, pad_blob = ce.place_pad(proj, pad_template, xy, next_off)
        new_records.append((pad_rec, pad_blob))
        next_off += len(pad_blob)

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
              f"longest {float(D[mst.row, mst.col].max()):.1f}mm", flush=True)

    print(f"built {len(new_records)} records total (CLEAN replace, corpus "
          f"supports dropped)", flush=True)
    proj.write_replace(OUT_PATH, new_records)
    print(f"wrote {OUT_PATH} in {time.time()-t0:.1f}s", flush=True)

    # Reparse + verify. v4 emits NO T_TIP records (each Member is one
    # T_SHAFT/T_TAPER frustum, see chitubox_emit.synth_member_chain) so
    # Project.supports()'s per-contact grouping (which keys off T_TIP
    # boundaries) does not apply here -- report record-type counts and
    # bridges directly instead of pretending the v3 grouping still means
    # anything.
    reparsed = ci.Project(OUT_PATH)
    type_counts = {}
    for r in reparsed.records:
        type_counts[r.type] = type_counts.get(r.type, 0) + 1
    _, bridges_out = reparsed.supports()
    stub_z = [float(m["p0"][2]) for m in members if m["kind"] == "stub"]
    print(f"reparse: {reparsed.n_recs} records, types={type_counts}, "
          f"{len(bridges_out)} bridges, {len(pad_xy)} pads "
          f"(corpus had {proj.n_recs} records)", flush=True)
    if stub_z:
        print(f"tip z range (stub contacts): {min(stub_z):.1f}..{max(stub_z):.1f} "
              f"(model bounds: -64.0..64.0)", flush=True)


STAGES = dict(prep=stage_prep, rewire=stage_rewire, brace=stage_brace, verify=stage_verify,
              emit=stage_emit)

if __name__ == "__main__":
    arg = sys.argv[1] if len(sys.argv) > 1 else "prep"
    if arg.startswith("build"):
        stage_build(int(arg.split(":")[1]) if ":" in arg else None)
    else:
        STAGES[arg]()
