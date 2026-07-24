"""SPEC-V5-PERF sec 3 / 5.3: CLASS-B lever acceptance guards.

Every lever defaults OFF (a levers-off run is bit-identical to baseline, proven
by _perf_fingerprint / the frozen-baseline gate). With a lever ON on the
fixtures: the run stays DETERMINISTIC (two runs bit-identical -- the engine has
no RNG, and B5's budget removes the wall-clock nondeterminism) and the
STRUCTURAL hard gates (clearance / connectivity / printability / murray) stay
zero -- a lever trades COVERAGE/resin QUALITY, never a penetration or a broken
structure. Coverage deltas are measured by manual_perf_levers.py, not asserted
here (a value that loses coverage on a fixture is a per-lever owner decision).
"""
import dataclasses

import pytest

from support_placement import config as config_mod, engine, mesh_io


def _serialize(result):
    return [(m["p0"].tobytes(), m["p1"].tobytes(), m["r0"], m["r1"], m["kind"])
            for m in result["members"]]


def _structural_gates(result):
    v = result["verify"]
    return (len(v["clearance_violations"]) + len(v["connectivity_violations"])
            + len(v["printability_violations"]) + len(v["murray_violations"]))


@pytest.mark.parametrize("field,val", [
    ("perf_beam_k", 4),
    ("perf_anchor_cap", 4),
    ("perf_seed_spacing_factor", 1.3),
    ("perf_rewire_work_budget", 100),
    ("perf_fix_iter_caps", (1, 2)),
])
def test_lever_deterministic_and_structural_gates_clean(field, val):
    cfg = dataclasses.replace(config_mod.SupportConfig(), **{field: val})
    mesh = mesh_io.make_overhang_table()
    r1 = engine.run(mesh, cfg)
    r2 = engine.run(mesh_io.make_overhang_table(), cfg)
    assert _serialize(r1) == _serialize(r2), f"{field}={val} nondeterministic"
    assert _structural_gates(r1) == 0, (field, val, r1["verify"])


def test_lever_off_matches_default():
    """Each lever explicitly set to its OFF sentinel reproduces the default
    engine output (a no-op, so the levers can never silently perturb a
    lever-off run)."""
    mesh = mesh_io.make_overhang_table
    base = _serialize(engine.run(mesh(), config_mod.SupportConfig()))
    for field, off in (("perf_beam_k", None), ("perf_anchor_cap", None),
                       ("perf_seed_spacing_factor", None),
                       ("perf_fix_iter_caps", None),
                       ("perf_rewire_work_budget", None)):
        cfg = dataclasses.replace(config_mod.SupportConfig(), **{field: off})
        assert _serialize(engine.run(mesh(), cfg)) == base, f"{field} OFF perturbed output"


def test_b4_emission_gate_raises_only_when_gate_on_and_verify_fails():
    """B4/P14: with perf_gate_emit_on_verify set, a run whose verify.ok is
    False must be REFUSED (fatal). We force a failing verify with an
    aggressive fix cap on a fixture that needs its fix passes; without the
    gate the same run returns (report carried, caller decides)."""
    mesh_fn = mesh_io.make_overhang_table
    # find a cap that makes verify fail (if the fixture stays green even at the
    # tightest cap, the emission-gate path is still exercised by the OFF case).
    cfg_off = dataclasses.replace(config_mod.SupportConfig(),
                                  perf_fix_iter_caps=(1, 1),
                                  perf_gate_emit_on_verify=False)
    r = engine.run(mesh_fn(), cfg_off)          # never raises when gate off
    if r["verify"]["ok"]:
        pytest.skip("fixture stays gate-clean at the tightest fix cap; "
                    "emission-gate raise path unreachable on this fixture")
    cfg_on = dataclasses.replace(config_mod.SupportConfig(),
                                 perf_fix_iter_caps=(1, 1),
                                 perf_gate_emit_on_verify=True)
    with pytest.raises(RuntimeError, match="GATES-FAILED"):
        engine.run(mesh_fn(), cfg_on)
