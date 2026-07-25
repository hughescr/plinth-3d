"""Full-scale Weeping Treant acceptance run on the kernel-backed engine.run
(canonical cfg SupportConfig(lift_mm=3.0), DEFAULT config = class A only).
Instrumented for the SPEC-RUST-KERNEL sec 8 / A23 acceptance obligations.

argv: <tag> [profile]
  tag=run1  -> histogram+telemetry, state pkl, clean timing (primary)
  tag=run2  -> determinism source
  'profile' -> wrap engine.run in cProfile (A23(b))
Env: GEOM_CAPTURE_CORPUS=1 to also capture the A23(a) replay corpus.

Pickle path is derived from tag: SCR/v5p_treant_<tag>.pkl (run1 == the
task-mandated v5p_treant_run1.pkl).
"""
import sys, os, time, json, pickle, resource, faulthandler, threading, cProfile, pstats, io

faulthandler.enable()

import _toolpaths
WT = _toolpaths.REPO_ROOT
SCR = _toolpaths.scratch_dir()
STL = _toolpaths.resolve_stl()
sys.path.insert(0, _toolpaths.SCRIPTS_DIR)

import numpy as np
from support_placement import mesh_io, config, engine
import instrument   # sibling module in scripts/tools/
import common       # sibling module in scripts/tools/


def rss_gb():
    r = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return (r / 1e9) if sys.platform == "darwin" else (r / 1e6)


def _heartbeat(tag, stop):
    t0 = time.time()
    while not stop.is_set():
        print(f"[{tag}] HB t=+{time.time()-t0:.0f}s peakRSS={rss_gb():.2f}GB",
              file=sys.stderr, flush=True)
        stop.wait(30)


def _daemonize(logpath):
    """Double-fork + setsid so the run outlives the launching shell/agent and
    lives in its own session (macOS has no setsid(1); do it in-process)."""
    if os.fork() > 0:
        os._exit(0)
    os.setsid()
    if os.fork() > 0:
        os._exit(0)
    sys.stdout.flush(); sys.stderr.flush()
    f = open(logpath, "a", buffering=1)
    os.dup2(f.fileno(), sys.stdout.fileno())
    os.dup2(f.fileno(), sys.stderr.fileno())
    devnull = open(os.devnull, "r")
    os.dup2(devnull.fileno(), sys.stdin.fileno())


def main():
    tag = sys.argv[1] if len(sys.argv) > 1 else "run1"
    do_profile = "profile" in sys.argv[2:]
    if os.environ.get("DETACH") == "1":
        _daemonize(os.path.join(SCR, f"v5p_{tag}.log"))
    faulthandler.enable()
    import geom_kernel
    print(f"[{tag}] kernel api={geom_kernel._native.KERNEL_API_VERSION} "
          f"digest={geom_kernel._native.SOURCE_DIGEST} "
          f"threads={geom_kernel._native.current_threads()} "
          f"GEOM_KERNEL_THREADS={os.environ.get('GEOM_KERNEL_THREADS','<unset>')} "
          f"capture_corpus={instrument._CAPTURE}", flush=True)

    stop = threading.Event()
    threading.Thread(target=_heartbeat, args=(tag, stop), daemon=True).start()
    print(f"[{tag}] loading STL + centering", flush=True)
    mesh = mesh_io.load_stl(STL)
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    print(f"[{tag}] mesh: {len(mesh.faces)} faces bounds={mesh.bounds.tolist()}", flush=True)

    instrument.install()
    cfg = config.SupportConfig(lift_mm=3.0)
    log = {}

    ru0 = resource.getrusage(resource.RUSAGE_SELF)
    t0 = time.time()
    if do_profile:
        pr = cProfile.Profile()
        pr.enable()
        result = engine.run(mesh, cfg, log=log)
        pr.disable()
    else:
        result = engine.run(mesh, cfg, log=log)
    wall_s = time.time() - t0
    ru1 = resource.getrusage(resource.RUSAGE_SELF)
    utime = ru1.ru_utime - ru0.ru_utime
    stime = ru1.ru_stime - ru0.ru_stime
    print(f"[{tag}] engine.run complete wall={wall_s:.1f}s utime={utime:.1f}s "
          f"stime={stime:.1f}s cpu/wall={utime/max(wall_s,1e-9):.2f}x "
          f"peakRSS={rss_gb():.2f}GB", flush=True)

    members = result["members"]; balls = result["balls"]
    unrouted = result["unrouted"]; forest = result["forest"]
    contacts = result["contacts"]
    v = result["verify"]; volume = result["volume"]

    dig = common.digest(members, balls)
    print(f"[{tag}] DIGEST members={dig['n_members']} balls={dig['n_balls']} "
          f"member_sha={dig['member_sha']} ball_sha={dig['ball_sha']} "
          f"combined_sha={dig['combined_sha']}", flush=True)

    # ---- quarantine sites (P11 cross-examination inputs) ----
    quarantined = sorted(int(x) for x in getattr(forest, "quarantined", set()))
    stub_of = {idx: ci for ci, idx in forest.terminal_of.items()}
    q_sites = []
    for si in quarantined:
        m = forest.members[si]
        md = m.to_dict() if hasattr(m, "to_dict") else dict(
            p0=m.p0, p1=m.p1, r0=m.r0, r1=m.r1, kind=m.kind,
            owner=m.owner, embed_end=m.embed_end)
        ci = stub_of.get(si)
        cinfo = None
        if ci is not None and ci < len(contacts):
            c = contacts[ci]
            cinfo = dict(
                contact_idx=int(ci),
                position=np.asarray(c["position"], float).tolist(),
                normal=np.asarray(c.get("normal", [0, 0, 0]), float).tolist(),
                island_id=int(c.get("island_id", -1)) if c.get("island_id") is not None else None)
        q_sites.append(dict(
            stub_member=int(si), contact_idx=(int(ci) if ci is not None else None),
            p0=np.asarray(md["p0"], float).tolist(),
            p1=np.asarray(md["p1"], float).tolist(),
            r0=float(md["r0"]), r1=float(md["r1"]),
            kind=md["kind"], owner=md.get("owner"),
            embed_end=md.get("embed_end"), contact=cinfo))
    print(f"[{tag}] QUARANTINE n={len(q_sites)} log_n_quarantined={log.get('n_quarantined')}",
          flush=True)

    # telemetry + build_log from the engine's workhorse geom
    wh = instrument.workhorse_telemetry()
    telem = wh.telemetry() if wh else {}
    try:
        blog = wh._native.build_log() if wh else {}
    except Exception:
        blog = getattr(wh, "build_log", {}) if wh else {}

    gate_counts = {k: len(v[k]) for k in (
        "clearance_violations", "connectivity_violations",
        "printability_violations", "murray_violations",
        "unsupported_minima", "load_violations",
        "uncovered_island_births", "uncovered_islands", "removal_violations")}
    gate_counts["ball_breach_violations"] = len(v.get("ball_breach_violations", []))
    gate_counts["ok"] = bool(v["ok"])

    kind_counts = {}
    for m in members:
        k = common._f(m, "kind")
        kind_counts[k] = kind_counts.get(k, 0) + 1
    max_r = max([max(float(common._f(m, "r0")), float(common._f(m, "r1"))) for m in members], default=0.0)
    max_ball_r = max([float(common._f(b, "r")) for b in balls], default=0.0)

    # --- persist state (census + P11 inputs) ---
    state = dict(cfg=cfg, plate_z=result["plate_z"], forest=forest,
                 unrouted=unrouted, members=members, balls=balls,
                 contacts=contacts, verify=v, volume=volume,
                 log=dict(log), quarantine_sites=q_sites, digest=dig)
    pkl = os.path.join(SCR, f"v5p_treant_{tag}.pkl")
    try:
        with open(pkl, "wb") as f:
            pickle.dump(state, f)
    except Exception as e:
        print(f"[{tag}] pickle retry ({e!r})", flush=True)
        try:
            forest._pitch_bound_cache = None
        except Exception:
            pass
        with open(pkl, "wb") as f:
            pickle.dump(state, f)
    print(f"[{tag}] wrote {pkl} ({os.path.getsize(pkl)} bytes)", flush=True)

    if instrument._CAPTURE:
        with open(os.path.join(SCR, f"{tag}_corpus.pkl"), "wb") as f:
            pickle.dump(dict(corpus=instrument.CORPUS, seen=dict(instrument._SEEN),
                             caps=instrument.CAPS), f)
        print(f"[{tag}] wrote corpus", flush=True)

    if do_profile:
        s = io.StringIO()
        ps = pstats.Stats(pr, stream=s).sort_stats("tottime")
        ps.print_stats(45)
        prof_txt = s.getvalue()
        with open(os.path.join(SCR, f"{tag}_cprofile.txt"), "w") as f:
            f.write(prof_txt)
        # also cumulative view
        s2 = io.StringIO()
        pstats.Stats(pr, stream=s2).sort_stats("cumulative").print_stats(40)
        with open(os.path.join(SCR, f"{tag}_cprofile_cum.txt"), "w") as f:
            f.write(s2.getvalue())
        print(f"[{tag}] wrote cProfile", flush=True)

    stage_keys = [k for k in log if k.startswith("t_")]
    out = dict(
        tag=tag, wall_s=round(wall_s, 2), utime_s=round(utime, 2),
        stime_s=round(stime, 2), cpu_over_wall=round(utime / max(wall_s, 1e-9), 3),
        peak_rss_gb=round(rss_gb(), 3),
        kernel=dict(api=geom_kernel._native.KERNEL_API_VERSION,
                    digest=geom_kernel._native.SOURCE_DIGEST,
                    threads=geom_kernel._native.current_threads(),
                    env_threads=os.environ.get("GEOM_KERNEL_THREADS", None)),
        n_members=len(members), n_balls=len(balls), n_unrouted=len(unrouted),
        n_contacts=len(contacts),
        n_forest_members_total=len(forest.members), n_forest_dead=len(forest.dead),
        n_quarantined=len(q_sites),
        max_structural_radius_mm=round(max_r, 4), max_ball_radius_mm=round(max_ball_r, 4),
        member_kind_counts=kind_counts,
        gates=gate_counts,
        volume=dict(member_ml=round(volume["member_ml"], 5),
                    ball_ml=round(volume["ball_ml"], 5),
                    total_ml=round(volume["total_ml"], 5),
                    by_kind_ml={k: round(vv, 5) for k, vv in volume["by_kind_ml"].items()}),
        digest=dig,
        telemetry=telem, build_log=blog,
        stage_timings={k: log[k] for k in stage_keys},
        verify_passes={k: log[k] for k in log if k.startswith("verify_pass")},
        forest_passes={k: log[k] for k in log if k.startswith("forest_pass")},
        log_scalars={k: log[k] for k in log
                     if k in ("n_unrouted", "n_quarantined", "gates_failed",
                              "contacts_initial", "minima_contacts_added",
                              "island_birth_contacts_added")},
        instrumentation=instrument.summary(),
    )
    with open(os.path.join(SCR, f"{tag}_results.json"), "w") as f:
        json.dump(out, f, indent=2, default=str)
    print(f"[{tag}] TELEMETRY {json.dumps(telem)}", flush=True)
    print(f"[{tag}] CALL_TOTAL {json.dumps(dict(instrument.CALL_TOTAL))}", flush=True)
    print(f"[{tag}] ROW_TOTAL {json.dumps(dict(instrument.ROW_TOTAL))}", flush=True)
    print(f"[{tag}] STAGES {json.dumps(out['stage_timings'])}", flush=True)
    print(f"[{tag}] GATES {json.dumps(gate_counts)}", flush=True)
    print(f"[{tag}] VOLUME {json.dumps(out['volume'])}", flush=True)
    print(f"[{tag}] DONE", flush=True)
    stop.set()


if __name__ == "__main__":
    main()
