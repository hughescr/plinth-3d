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

import numpy as np
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
# step 0: load-time component triage
# ---------------------------------------------------------------------------

def _one_body():
    """A single-body ball: 8.6mm of bulk, under the 10mm floor, so the
    decision is SOLID and the triaged mesh is exported -- which lets a test
    read the result back out of the tool's own output. (A box is no good
    here: it voxelizes with no background, so the EDT reads it as unbounded
    bulk and it hollows.)"""
    return trimesh.creation.icosphere(radius=4.0)


def _pinched_tetra(apex, edge, inward):
    """A 4-face tetrahedron hanging off ``apex`` by that ONE vertex.

    This is the shape of the real defect (measured on 8 of 37 production
    models): it touches the body at a bit-identical vertex position and
    shares no edge with it, so a bounding-box or vertex-distance test reads
    it as attached while an edge test reads it as separable. ``inward``
    flips the winding, making it a void rather than a solid -- the closed
    shards in the corpus are wound that way.
    """
    v = np.array([apex,
                  apex + np.array([edge, 0.0, 0.0]),
                  apex + np.array([0.0, edge, 0.0]),
                  apex + np.array([0.0, 0.0, edge])])
    f = np.array([[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]])
    t = trimesh.Trimesh(vertices=v, faces=f)
    trimesh.repair.fix_normals(t)
    if inward:
        t.invert()
    return t


def _body_plus(edge, inward, path):
    """The slab with a tetrahedron pinched onto its top-far corner."""
    body = _one_body()
    apex = body.vertices[np.lexsort(body.vertices.T)[-1]]
    tetra = _pinched_tetra(apex, -edge if inward else edge, inward)
    return _export(trimesh.util.concatenate([body, tetra]), path)


@pytest.fixture(scope="module")
def shard_stl(tmp_path_factory):
    """Body + a bowtie-pinched degenerate shard that the predicate drops.

    0.5mm edges give it |volume| 0.021mm3, ABOVE the 0.01mm3 cap: it is
    dropped because an inward-wound shell is not a solid, not because its
    volume rounds to zero. Awakened Tree.stl's real 20-face shard measures
    the same way (closed, inward, 0.0146mm3)."""
    return _body_plus(0.5, inward=True,
                      path=tmp_path_factory.mktemp("hollow") / "shard.stl")


@pytest.fixture(scope="module")
def small_solid_extra_stl(tmp_path_factory):
    """Body + a small pinched component that FAILS the predicate on exactly
    one clause: it is a real outward-wound solid of 0.167mm3, over the
    0.01mm3 cap. Everything else about it (4 faces, 1mm across, no shared
    edge) matches the junk profile, so this pins the volume clause."""
    return _body_plus(1.0, inward=False,
                      path=tmp_path_factory.mktemp("hollow") / "extra.stl")


@pytest.fixture(scope="module")
def one_body_stl(tmp_path_factory):
    return _export(_one_body(), tmp_path_factory.mktemp("hollow") / "slab.stl")


@pytest.fixture(scope="module")
def pre_hollowed_stl(tmp_path_factory):
    """A model that arrived already hollowed: r=20 sphere with an r=12
    spherical void. Watertight, is_volume True -- it sails through the only
    input gate that existed before, and step 2's voxel fill then reads the
    void as bulk and carves a replacement cavity."""
    outer = trimesh.creation.icosphere(subdivisions=2, radius=20.0)
    inner = trimesh.creation.icosphere(subdivisions=2, radius=12.0)
    inner.invert()
    m = trimesh.util.concatenate([outer, inner])
    return _export(m, tmp_path_factory.mktemp("hollow") / "pre_hollowed.stl")


def test_bowtie_shard_is_dropped_and_the_drop_is_reported(shard_stl, tmp_path):
    out = tmp_path / "clean.stl"
    log = hollow.run(shard_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["components"] == 2
    assert log["dropped_shard_faces"] == 4
    assert len(log["dropped_shards"]) == 1
    (shard,) = log["dropped_shards"]
    assert shard["faces"] == 4
    assert max(shard["dims_mm"]) == pytest.approx(0.5, abs=1e-3)

    result = trimesh.load(str(out))
    assert len(result.split(only_watertight=False)) == 1
    assert result.is_watertight, "dropping an edge-disjoint component cannot open a hole"
    assert result.is_volume
    body = _one_body()
    assert len(result.faces) == len(body.faces)
    assert result.volume == pytest.approx(body.volume, rel=1e-6)
    assert np.allclose(result.bounds, body.bounds, atol=1e-4)


def test_a_small_real_solid_is_not_a_shard(small_solid_extra_stl, tmp_path):
    """Failing any one clause keeps the component. The predicate is a DROP
    test on named evidence, not "keep the largest component"."""
    out = tmp_path / "kept.stl"
    log = hollow.run(small_solid_extra_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["components"] == 2
    assert "dropped_shards" not in log
    assert len(trimesh.load(str(out)).split(only_watertight=False)) == 2


def test_pre_hollowed_input_is_refused_before_anything_is_rehollowed(
        pre_hollowed_stl, tmp_path):
    out = tmp_path / "must_not_exist.stl"
    log = {}
    with pytest.raises(hollow.PreHollowedError) as ei:
        hollow.run(pre_hollowed_stl, str(out), orient=False, log=log)
    assert not out.exists()
    assert "already hollowed" in str(ei.value)
    # refused, NOT re-hollowed: the hollow decision was never reached
    assert "decision" not in log and "bulk_mm" not in log
    # and the cavity that triggered it is named, not quietly discarded
    assert log["cavity_components"] and log["cavity_components"][0]["faces"] > 64
    assert "dropped_shards" not in log


def test_pre_hollowed_refusal_is_a_bad_input_not_a_status(pre_hollowed_stl, tmp_path):
    """Per the module's status contract, bad INPUTS raise rather than
    reporting a status, so callers that only know ``HollowError`` still
    handle this one correctly."""
    assert issubclass(hollow.PreHollowedError, hollow.HollowError)
    with pytest.raises(hollow.HollowError):
        hollow.run(pre_hollowed_stl, str(tmp_path / "x.stl"), orient=False)


def test_a_sound_single_body_mesh_is_left_alone(one_body_stl, tmp_path):
    log = hollow.run(one_body_stl, str(tmp_path / "s.stl"), orient=False)
    assert log["components"] == 1
    assert "dropped_shards" not in log and "cavity_components" not in log


def test_multi_body_input_is_neither_stripped_nor_refused(solid_stl, tmp_path):
    """The overhang table is five overlapping boxes. Detached bodies are not
    this stage's business and never were -- only inward-wound shells (real
    cavities) and degenerate shards are."""
    out = tmp_path / "table.stl"
    log = hollow.run(solid_stl, str(out), orient=False)
    assert log["status"] == hollow.STATUS_OK
    assert log["components"] == 5
    assert len(log["extra_bodies"]) == 4
    assert "dropped_shards" not in log and "cavity_components" not in log
    assert len(trimesh.load(str(out)).split(only_watertight=False)) == 5


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


def test_cli_exits_one_on_pre_hollowed_input(pre_hollowed_stl, tmp_path):
    """Already-hollowed is a bad input (exit 1), not a verification failure
    (exit 2): nothing about the algorithm went wrong, the file is for the
    drill-only path. ``error_type`` is what tells a caller which."""
    out = tmp_path / "nope.stl"
    r = _run_cli(pre_hollowed_stl, str(out), "--no-orient")
    assert r.returncode == 1, r.stderr
    assert not out.exists()
    assert "already hollowed" in r.stderr
    assert "drill-only" in r.stderr
    payload = json.loads(r.stdout)
    assert payload["status"] == "ERROR"
    assert payload["error_type"] == "PreHollowedError"


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
