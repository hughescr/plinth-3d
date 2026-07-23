"""SPEC-V5-PERF sec 1.2 / 5.2 measurement harness: construct-stage kernel
query counting at treant scale.

Usage:
    .venv/bin/python scripts/tests/manual_perf_probe.py [N_CONTACTS] [STL_PATH]

Loads the treant STL, runs (and caches) engine.prepare, then runs cco.build
on the first N contacts with every KernelSurface entry point wrapped in query
counters and Forest.revalidate wrapped in dirty-set accounting. Emits a JSON
report on stdout with 25-insertion windows: revalidate calls, mean dirty-set
size, per-method call/element counts, an analytic parity-vote estimate for
penetration-interval jobs (min(80, max(3, L/0.12)) per member, single
sub-segment lower bound), and wall time per window.

Query COUNTS are the primary metric (deterministic, contention-immune);
wall-clock on a shared box is reported but secondary (SPEC-V5-PERF R-P8).

NOTE: calls cco.build directly (not engine.construct) because truncating the
contact list changes the load partition, which can leave a stub legitimately
uncertified and trip engine.construct's assert_all_certified — a truncation
artifact, not a defect (measured: N=150 trips it, full 747 does not).

The `--ab` bit-identity mode named by SPEC-V5-PERF sec 2.5 gate 4 lands with
the class-A implementation (it needs both engine versions importable).

Prepare cache: writes treant_pre_probe.pkl next to this file (gitignored
scale: ~25MB) so repeat probes skip the ~2min prepare.
"""
import json
import os
import pickle
import sys
import time
from collections import defaultdict

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import numpy as np
import trimesh

from support_placement import cco, config, engine, placement, routing, tree
from support_placement.geometry_kernel import KernelSurface

DEFAULT_STL = "/Users/craig/code/hughescr/dnd-campaigns/Weeping Treant v1.2.stl"
PRE_CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                         "treant_pre_probe.pkl")
WINDOW = 25

COUNTS = defaultdict(int)


def _wrap(obj, name, n_elems):
    orig = getattr(obj, name)

    def wrapped(*a, **k):
        COUNTS[f"{name}_calls"] += 1
        try:
            COUNTS[f"{name}_elems"] += n_elems(*a, **k)
        except Exception:
            pass
        return orig(*a, **k)

    setattr(obj, name, wrapped)


def instrument(surf):
    _wrap(surf, "inside", lambda pts, **k: len(np.atleast_2d(np.asarray(pts))))
    _wrap(surf, "project", lambda pts, **k: len(np.atleast_2d(np.asarray(pts))))
    _wrap(surf, "segment_clearance",
          lambda p0s, *a, **k: len(np.atleast_2d(np.asarray(p0s))))
    _wrap(surf, "sphere_breach",
          lambda c, *a, **k: len(np.atleast_2d(np.asarray(c))))
    _wrap(surf, "cast_rays",
          lambda o, *a, **k: len(np.atleast_2d(np.asarray(o))))

    orig_batch = surf.penetration_intervals_batch

    def batch(p0s, p1s, r0s, r1s, **k):
        p0 = np.atleast_2d(np.asarray(p0s, float))
        p1 = np.atleast_2d(np.asarray(p1s, float))
        COUNTS["pen_intervals_calls"] += 1
        COUNTS["pen_intervals_members"] += len(p0)
        L = np.linalg.norm(p1 - p0, axis=1)
        COUNTS["pen_intervals_est_votes"] += int(
            np.minimum(80, np.maximum(3, L / 0.12)).sum())
        return orig_batch(p0s, p1s, r0s, r1s, **k)

    surf.penetration_intervals_batch = batch


REVAL = dict(calls=0, dirty_total=0, dirty_max=0)
_orig_reval = tree.Forest.revalidate

# PERF_QUARANTINE=1: simulate SPEC-V5-PERF A0 (pre-existing persistent stub
# violators do not poison candidate trials) so the probe measures the
# HEALTHY regime instead of the sec-0 poisoned regime. Measurement shim
# only — the real A0 lands in cco per the spec.
QUARANTINE = set()


def _reval(self, geom, cfg):
    d = len(self.dirty_members())
    REVAL["calls"] += 1
    REVAL["dirty_total"] += d
    REVAL["dirty_max"] = max(REVAL["dirty_max"], d)
    out = _orig_reval(self, geom, cfg)
    if QUARANTINE:
        out = [v for v in out if v["member_idx"] not in QUARANTINE]
    return out


tree.Forest.revalidate = _reval

_orig_insert = cco._insert_terminal
INS = dict(n=0)
WINDOWS = []
_last = dict(counts={}, reval=(0, 0), t=time.time())


def _snap(forest=None):
    delta = {k: COUNTS[k] - _last["counts"].get(k, 0) for k in COUNTS}
    rc = REVAL["calls"] - _last["reval"][0]
    rd = REVAL["dirty_total"] - _last["reval"][1]
    now = time.time()
    row = dict(upto=INS["n"], dt=round(now - _last["t"], 2), reval_calls=rc,
               dirty_per_reval=round(rd / max(rc, 1), 2), **delta)
    if forest is not None:
        row["forest_members"] = len(forest.members)
    WINDOWS.append(row)
    _last["counts"] = dict(COUNTS)
    _last["reval"] = (REVAL["calls"], REVAL["dirty_total"])
    _last["t"] = now


def _insert_hook(forest, i, *a, **k):
    ok = _orig_insert(forest, i, *a, **k)
    INS["n"] += 1
    if INS["n"] % WINDOW == 0:
        _snap(forest)
    return ok


cco._insert_terminal = _insert_hook


def main():
    n_contacts = int(sys.argv[1]) if len(sys.argv) > 1 else 300
    stl = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_STL
    cfg = config.SupportConfig()
    t0 = time.time()
    mesh = trimesh.load(stl)
    print(f"[probe] mesh {len(mesh.faces)} faces t={time.time()-t0:.0f}s",
          file=sys.stderr, flush=True)
    geom = KernelSurface(mesh)
    instrument(geom)
    if os.path.exists(PRE_CACHE):
        with open(PRE_CACHE, "rb") as f:
            pre = pickle.load(f)
        print("[probe] prepare loaded from cache", file=sys.stderr, flush=True)
    else:
        pre = engine.prepare(mesh, cfg, geom=geom)
        with open(PRE_CACHE, "wb") as f:
            pickle.dump(pre, f)
        print(f"[probe] prepare: {len(pre['contacts'])} contacts "
              f"t={time.time()-t0:.0f}s", file=sys.stderr, flush=True)

    contacts = pre["contacts"][:n_contacts]
    carried = placement.load_partition(mesh, pre["islands"], contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    minima_flags = [bool(c.get("minimum")) for c in contacts]

    if os.environ.get("PERF_QUARANTINE") == "1":
        # identify persistent stub violators up front (same stub geometry
        # cco.build creates) and quarantine their violations (see above)
        f0 = tree.Forest()
        for i, c in enumerate(contacts):
            pos = np.asarray(c["position"], float)
            normal = np.asarray(c["normal"], float)
            normal = normal / (np.linalg.norm(normal) or 1.0)
            f0.add_member(tree.Member(
                p0=pos - normal * cfg.contact_depth_mm,
                p1=pos + normal * cfg.stub_length_mm,
                r0=float(tip_r[i]),
                r1=max(float(tip_r[i]), cfg.min_shoot_r_mm),
                kind="stub", owner=("contact", i), embed_end="p0",
                carried_mm2=float(carried[i])))
        _orig_reval(f0, geom, cfg)
        QUARANTINE.update(f0.dirty_members())
        print(f"[probe] quarantined {len(QUARANTINE)} persistent stub "
              f"violators", file=sys.stderr, flush=True)

    # reset counters so prepare-stage queries don't pollute the construct model
    COUNTS.clear()
    REVAL.update(calls=0, dirty_total=0, dirty_max=0)
    _last.update(counts={}, reval=(0, 0), t=time.time())

    t_c = time.time()
    forest, unrouted = cco.build(mesh, contacts, tip_r, carried, minima_flags,
                                 pre["plate_z"], geom, cfg,
                                 sample_points=pre["sample_points"],
                                 sample_delicacy=pre["sample_delicacy"])
    _snap(forest)
    out = dict(n_contacts=n_contacts, t_construct=round(time.time() - t_c, 1),
               n_members=len(forest.members), n_unrouted=len(unrouted),
               reval_dirty_max=REVAL["dirty_max"], windows=WINDOWS,
               quarantined=sorted(QUARANTINE),
               threads=os.environ.get("GEOM_KERNEL_THREADS", "default"))
    print(json.dumps(out, indent=1, default=str))


if __name__ == "__main__":
    main()
