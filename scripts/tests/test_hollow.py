"""The hollow/drill stage and its exit-code contract.

The thing under test is NOT the hollowing geometry (that is validated on real
prints, not in CI): it is the promise that a run which writes no file cannot
look like a run that did. Before the split, a failed self-verification set a
``status`` key inside a JSON blob on stdout and the tool exited 0.
"""
import json
import os
import subprocess
import sys

import pytest
import trimesh

from support_placement import hollow, mesh_io

TOOLS = os.path.join(os.path.dirname(__file__), "..", "tools")
HOLLOW_DRILL = os.path.abspath(os.path.join(TOOLS, "hollow_drill.py"))
FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures", "chitubox")


def _export(mesh, path):
    mesh.export(str(path))
    return str(path)


@pytest.fixture(scope="module")
def solid_stl(tmp_path_factory):
    """A mini small enough that hollowing is not worth it: the standard
    overhang table at 0.7 scale measures 8.1mm of bulk, under the 10mm floor,
    so the decision is SOLID_below_threshold -- a SUCCESS that exports."""
    m = mesh_io.make_overhang_table()
    m.apply_scale(0.7)
    return _export(m, tmp_path_factory.mktemp("hollow") / "small_table.stl")


@pytest.fixture(scope="module")
def verify_failing_stl(tmp_path_factory):
    """A shape the hollower mangles: the toed slab hollows (10.8mm bulk) and
    then fails step 8. Same failure class as the Cylinder fixture, ~4x faster.
    If the algorithm ever learns to handle this shape, this fixture must be
    re-picked -- the point is to exercise the FAILURE PATH, not to freeze this
    particular mesh's fate."""
    return _export(mesh_io.make_toed_slab(),
                   tmp_path_factory.mktemp("hollow") / "toed_slab.stl")


# ---------------------------------------------------------------------------
# the library: every return path sets a status
# ---------------------------------------------------------------------------

def test_solid_decision_is_a_success_and_exports(solid_stl, tmp_path):
    out = tmp_path / "solid.stl"
    log = hollow.run(solid_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["decision"].startswith("SOLID")
    assert log["exported"] == str(out)
    assert out.is_file() and out.stat().st_size > 0
    assert hollow.failed_checks(log) == []


def test_verify_failure_writes_nothing_and_names_the_checks(verify_failing_stl, tmp_path):
    out = tmp_path / "must_not_exist.stl"
    log = hollow.run(verify_failing_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_VERIFY_FAILED
    assert not out.exists(), "a failed verification must not export"
    failed = hollow.failed_checks(log)
    assert failed, "a failed run must name at least one check"
    assert set(failed) <= set(hollow.CHECK_DESCRIPTIONS)
    assert log["failed_checks"] == failed
    lines = hollow.describe_failure(log)
    assert len(lines) == len(failed)
    for name, line in zip(failed, lines):
        assert line.startswith(name + ":")
        assert "measured" in line


def test_check_detail_covers_every_documented_check(verify_failing_stl, tmp_path):
    log = hollow.run(verify_failing_stl, str(tmp_path / "x.stl"), orient=False)
    assert set(log["check_detail"]) == set(hollow.CHECK_DESCRIPTIONS)


def test_non_volume_input_raises_rather_than_reporting(tmp_path):
    """A torn mesh is a bad input, not an outcome of the algorithm."""
    m = trimesh.Trimesh(vertices=[[0, 0, 0], [1, 0, 0], [0, 1, 0]], faces=[[0, 1, 2]])
    p = _export(m, tmp_path / "sheet.stl")
    with pytest.raises(hollow.HollowError):
        hollow.run(p, str(tmp_path / "out.stl"), orient=False)


def test_helpers_are_quiet_on_a_log_with_no_checks():
    assert hollow.failed_checks({}) == []
    assert hollow.describe_failure({"decision": "SOLID_no_cavity"}) == []


# ---------------------------------------------------------------------------
# the CLI: the exit code is the contract
# ---------------------------------------------------------------------------

def _run_cli(*argv):
    return subprocess.run([sys.executable, HOLLOW_DRILL, *argv],
                          capture_output=True, text=True)


def test_cli_exits_zero_on_a_solid_decision(solid_stl, tmp_path):
    out = tmp_path / "solid.stl"
    r = _run_cli(solid_stl, str(out), "--no-orient")
    assert r.returncode == 0, r.stderr
    assert out.is_file()
    assert json.loads(r.stdout)["status"] == hollow.STATUS_OK


def test_cli_exits_two_on_a_verify_failure(verify_failing_stl, tmp_path):
    out = tmp_path / "nope.stl"
    report = tmp_path / "r.json"
    r = _run_cli(verify_failing_stl, str(out), "--no-orient", "--report", str(report))
    assert r.returncode == 2, r.stdout
    assert not out.exists()
    # the operator must be told WHICH check failed, on stderr, unmissably
    assert "FAILED self-verification" in r.stderr
    assert "FAILED CHECK" in r.stderr
    payload = json.loads(report.read_text())
    assert payload["status"] == hollow.STATUS_VERIFY_FAILED
    assert payload["failed_checks"]
    for name in payload["failed_checks"]:
        assert name in r.stderr
    # the machine-readable record survives the failure on stdout too
    assert json.loads(r.stdout)["failed_checks"] == payload["failed_checks"]


def test_cli_exits_one_on_a_missing_input(tmp_path):
    r = _run_cli(str(tmp_path / "no_such.stl"), str(tmp_path / "o.stl"))
    assert r.returncode == 1
    assert "no such STL" in r.stderr
    assert json.loads(r.stdout)["status"] == "ERROR"


@pytest.mark.parametrize("name", ["Cylinder.stl"])
def test_cli_exit_code_on_the_committed_cylinder_fixture(name, tmp_path):
    """The originally-reported case (task #79): the 30mm cylinder hollows into
    4 bodies with a sealed cavity. Kept as a fixture-backed regression so the
    reported bug has a test that names it."""
    out = tmp_path / "cyl.stl"
    r = _run_cli(os.path.join(FIXTURES, name), str(out), "--no-orient", "--quiet")
    assert r.returncode == 2
    assert not out.exists()
    assert "single_body" in r.stderr or "cavity_breathes" in r.stderr
