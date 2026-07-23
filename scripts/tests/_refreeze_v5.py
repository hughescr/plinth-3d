"""One-shot re-freeze helper (SPEC-V5-GEOM sec 4): run the table + toed_slab
fixtures through the new exact-core engine TWICE each, prove byte-identical
volume reports (determinism), and emit fixtures_v5_baseline.json. Not a test;
invoked manually during the v5 swap, then left in-tree for provenance."""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from support_placement import engine, config as config_mod, mesh_io  # noqa: E402


def run_once(mesh_fn):
    mesh = mesh_fn()
    r = engine.run(mesh, config_mod.SupportConfig())
    return r


def main():
    out = {}
    for name, mesh_fn in (("table", mesh_io.make_overhang_table),
                          ("toed_slab", mesh_io.make_toed_slab)):
        r1 = run_once(mesh_fn)
        r2 = run_once(mesh_fn)
        assert r1["volume"] == r2["volume"], (name, "NON-DETERMINISTIC volume", r1["volume"], r2["volume"])
        v = r1["volume"]
        rewire_accepted = r1["log"].get("rewire", {}).get("accepted")
        out[name] = dict(
            total_ml=v["total_ml"],
            member_ml=v["member_ml"],
            n_unrouted=len(r1["unrouted"]),
            rewire_accepted=rewire_accepted,
        )
        print(f"{name}: total_ml={v['total_ml']:.10f} member_ml={v['member_ml']:.10f} "
              f"n_unrouted={len(r1['unrouted'])} rewire_accepted={rewire_accepted} "
              f"t_total={r1['log'].get('t_total')} (determinism OK)", flush=True)
    path = os.path.join(os.path.dirname(__file__), "fixtures_v5_baseline.json")
    with open(path, "w") as f:
        json.dump(out, f, indent=1)
        f.write("\n")
    print("WROTE", path, flush=True)
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
