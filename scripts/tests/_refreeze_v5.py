"""Gate-clean, full-signature v5 fixture refreeze helper.

For each fixture, run the complete engine twice, require every hard verification
gate to be empty, and require byte-identical canonical output before writing
``fixtures/v5_baseline.json``. Timing and RSS telemetry are deliberately
excluded; geometry, volume, contact provenance, routing, verification, prepared
selection, and deterministic transaction logs are included.
"""
import hashlib
import json
import os
import struct
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from support_placement import engine, config as config_mod, mesh_io  # noqa: E402


HARD_GATE_FIELDS = (
    "uncovered_islands",
    "clearance_violations",
    "ball_breach_violations",
    "load_violations",
    "unsupported_minima",
    "uncovered_island_births",
    "regular_contact_invariant_violations",
    "unrouted_specialized_contacts",
    "connectivity_violations",
    "printability_violations",
    "murray_violations",
)
_TIMING_KEYS = frozenset(("dt_s", "elapsed_s", "duration_s", "wall_s"))
_RSS_KEYS = frozenset(("rss_base", "rss_peak", "rss_delta"))


def run_once(mesh_fn):
    return engine.run(mesh_fn(), config_mod.SupportConfig())


def _canonical_value(value):
    """JSON-safe exact representation, preserving dtype and float bits."""
    if isinstance(value, np.ndarray):
        array = np.ascontiguousarray(value)
        return {
            "$ndarray": [
                array.dtype.str, list(array.shape), array.tobytes().hex()]
        }
    if isinstance(value, np.generic):
        array = np.ascontiguousarray(np.asarray(value))
        return {"$npscalar": [array.dtype.str, array.tobytes().hex()]}
    if isinstance(value, float):
        return {"$f64": struct.pack(">d", value).hex()}
    if isinstance(value, dict):
        return {
            "$dict": [
                [str(key), _canonical_value(item)]
                for key, item in sorted(
                    value.items(), key=lambda pair: str(pair[0]))
            ]
        }
    if isinstance(value, tuple):
        return {"$tuple": [_canonical_value(item) for item in value]}
    if isinstance(value, list):
        return {"$list": [_canonical_value(item) for item in value]}
    if isinstance(value, set):
        normalized = [_canonical_value(item) for item in value]
        return {
            "$set": sorted(
                normalized,
                key=lambda item: json.dumps(item, sort_keys=True))
        }
    if isinstance(value, (str, int, bool)) or value is None:
        return value
    raise TypeError(
        f"unsupported canonical signature type {type(value)!r}: "
        f"{value!r}")


def _deterministic_log(value):
    """Remove process telemetry while retaining routing/transaction evidence."""
    if isinstance(value, dict):
        return {
            key: _deterministic_log(item)
            for key, item in value.items()
            if not (
                isinstance(key, str)
                and (
                    key.startswith("t_")
                    or key.startswith("measured_")
                    or key in _TIMING_KEYS
                    or key in _RSS_KEYS))
        }
    if isinstance(value, list):
        return [_deterministic_log(item) for item in value]
    if isinstance(value, tuple):
        return tuple(_deterministic_log(item) for item in value)
    return value


def deterministic_payload(result):
    """Full deterministic engine evidence used to authorize a refreeze."""
    selected_rows = tuple(
        int(contact["candidate_pool_row"])
        for contact in result["contacts"]
        if "candidate_pool_row" in contact)
    return {
        "volume": result["volume"],
        "verify": result["verify"],
        "n_unrouted": len(result["unrouted"]),
        "unrouted": tuple(sorted(int(index) for index in result["unrouted"])),
        "rewire_accepted": result["log"].get(
            "rewire", {}).get("accepted"),
        "contacts": result["contacts"],
        "selected_pool_rows": selected_rows,
        "synthetic_selection": result["log"].get(
            "synthetic_regular_coverage_selection"),
        "members": result["members"],
        "balls": result["balls"],
        "carried_area_mm2": result["carried_area_mm2"],
        "tip_r_mm": result["tip_r_mm"],
        "islands": result["islands"],
        "minima": result["minima"],
        "island_births": result["island_births"],
        "coverage_failed_candidate_pool_rows": result[
            "coverage_failed_candidate_pool_rows"],
        "deterministic_log": _deterministic_log(result["log"]),
    }


def canonical_signature(result):
    return json.dumps(
        _canonical_value(deterministic_payload(result)),
        sort_keys=True, separators=(",", ":")).encode("utf-8")


def require_gate_clean(name, result):
    verify = result["verify"]
    if verify.get("ok") is not True:
        raise RuntimeError(f"{name}: verify.ok is not true")
    if result["log"].get("gates_failed") is not False:
        raise RuntimeError(f"{name}: log.gates_failed is not false")
    for field in HARD_GATE_FIELDS:
        if verify.get(field) != []:
            raise RuntimeError(
                f"{name}: hard gate {field} is not empty: "
                f"{verify.get(field)!r}")


def _baseline_row(result, signature):
    volume = result["volume"]
    selection = result["log"]["synthetic_regular_coverage_selection"]
    return {
        "total_ml": volume["total_ml"],
        "member_ml": volume["member_ml"],
        "ball_ml": volume["ball_ml"],
        "by_kind_ml": volume["by_kind_ml"],
        "n_unrouted": len(result["unrouted"]),
        "rewire_accepted": result["log"]["rewire"]["accepted"],
        "n_contacts": len(result["contacts"]),
        "n_members": len(result["members"]),
        "n_balls": len(result["balls"]),
        "synthetic_selected_rows": list(selection["selected_rows"]),
        "synthetic_exact": bool(selection["exact"]),
        "signature_bytes": len(signature),
        "signature_sha256": hashlib.sha256(signature).hexdigest(),
    }


def run_pair(name, mesh_fn):
    first = run_once(mesh_fn)
    require_gate_clean(f"{name} run 1", first)
    first_signature = canonical_signature(first)
    second = run_once(mesh_fn)
    require_gate_clean(f"{name} run 2", second)
    second_signature = canonical_signature(second)
    if first_signature != second_signature:
        raise RuntimeError(
            f"{name}: NON-DETERMINISTIC full signature "
            f"{hashlib.sha256(first_signature).hexdigest()} != "
            f"{hashlib.sha256(second_signature).hexdigest()}")
    return first, first_signature


def main():
    out = {}
    for name, mesh_fn in (
            ("table", mesh_io.make_overhang_table),
            ("toed_slab", mesh_io.make_toed_slab)):
        result, signature = run_pair(name, mesh_fn)
        out[name] = _baseline_row(result, signature)
        print(
            f"{name}: total_ml={out[name]['total_ml']:.16g} "
            f"member_ml={out[name]['member_ml']:.16g} "
            f"n_unrouted={out[name]['n_unrouted']} "
            f"rewire_accepted={out[name]['rewire_accepted']} "
            f"signature={out[name]['signature_bytes']}B/"
            f"{out[name]['signature_sha256']} "
            "(two gate-clean full signatures identical)",
            flush=True)
    path = os.path.join(
        os.path.dirname(__file__), "fixtures", "v5_baseline.json")
    with open(path, "w") as handle:
        json.dump(out, handle, indent=1)
        handle.write("\n")
    print("WROTE", path, flush=True)
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
