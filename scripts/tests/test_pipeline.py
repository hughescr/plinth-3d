"""The end-to-end pipeline: STL -> hollow/drill -> supports -> .chitubox.

What is being pinned here is the OUTCOME CONTRACT, not the geometry:

  * every call returns a report, and the report always carries a status and
    an exit code;
  * a stage that declines to produce output produces a non-zero exit code AND
    a JSON record on disk naming the stage -- the pair, every time;
  * no path can end in "succeeded, wrote nothing".

Each failure class gets its own exit code, so the wiring tests below drive
the stages to fail deliberately rather than hoping a real model misbehaves.
"""
import json
import os
import subprocess
import sys

import numpy as np
import pytest
import trimesh

from support_placement import config, emit, hollow, mesh_io, pipeline

TOOLS = os.path.join(os.path.dirname(__file__), "..", "tools")
PREP_ONE = os.path.abspath(os.path.join(TOOLS, "prep_one.py"))


@pytest.fixture(scope="module")
def source_stl(tmp_path_factory):
    """A mini the whole pipeline can chew through in a couple of seconds: the
    overhang table at 0.7 scale. Hollowing legitimately decides SOLID on it
    (8.1mm of bulk, under the 10mm floor) and the overhang still needs a real
    support forest, so all three stages do actual work."""
    m = mesh_io.make_overhang_table()
    m.apply_scale(0.7)
    p = tmp_path_factory.mktemp("src") / "small_table.stl"
    m.export(str(p))
    return str(p)


def _empty_result():
    return dict(members=[], balls=[], plate_z=0.0, verify=dict(ok=True),
                unrouted=[], contacts=[], volume={})


# ---------------------------------------------------------------------------
# the happy path, all three stages, for real
# ---------------------------------------------------------------------------

@pytest.mark.kernel
@pytest.mark.chitubox
def test_prep_one_runs_all_three_stages(source_stl, tmp_path):
    out = tmp_path / "table.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), orient=False,
                            allow_failed_gates=True)

    assert rep.status == pipeline.STATUS_OK
    assert rep.exit_code == pipeline.EXIT_OK and rep.ok
    assert rep.failed_stage is None and rep.error is None
    assert [s.name for s in rep.stages] == list(pipeline.STAGES)
    assert all(s.status == pipeline.STATUS_OK for s in rep.stages)

    assert out.is_file() and out.stat().st_size > 0
    # the intermediate is an OUTPUT: the mesh the supports were computed on
    assert os.path.isfile(rep.prepped_stl)
    assert rep.prepped_stl == pipeline.default_prepped_path(str(out))

    hollow_rep = rep.stage("hollow").report
    assert hollow_rep["status"] == hollow.STATUS_OK
    assert hollow_rep["decision"].startswith("SOLID")

    sup = rep.stage("supports").report
    assert sup["n_members"] > 0
    # A1 restores the lifted bottom-face demand this 0.7-scale wiring
    # fixture used to omit. Its downstream ball geometry is not yet clean,
    # so this test exercises the explicit development override without
    # weakening the engine gate or pretending the layout is printable.
    assert sup["gates"]["ok"] is False
    assert sup["gates"]["ball_breach_violations"] > 0
    assert sup["plate_z"] == pytest.approx(
        rep.stage("emit").report["plate_z"], abs=1e-9)

    em = rep.stage("emit").report
    assert em["n_records"] > 0
    assert em["reparse_ok"] is True
    assert em["bytes_written"] == out.stat().st_size
    # named after the SOURCE mini, not after the .prepped intermediate
    assert em["model_name"] == "small_table"


@pytest.mark.kernel
@pytest.mark.chitubox
def test_report_is_written_to_disk_and_json_safe(source_stl, tmp_path):
    out = tmp_path / "r.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), orient=False,
                            allow_failed_gates=True)
    assert rep.report_path == pipeline.default_report_path(str(out))
    on_disk = json.loads(open(rep.report_path).read())
    assert on_disk == json.loads(json.dumps(rep.to_dict()))
    assert on_disk["stages"][1]["report"]["gates"]["ok"] is False


@pytest.mark.kernel
@pytest.mark.chitubox
def test_no_hollow_uses_the_source_mesh_untouched(source_stl, tmp_path):
    rep = pipeline.prep_one(
        source_stl, str(tmp_path / "nh.chitubox"), hollow=False,
        allow_failed_gates=True)
    assert rep.status == pipeline.STATUS_OK
    assert rep.stage("hollow").status == pipeline.STATUS_SKIPPED
    assert rep.prepped_stl == source_stl
    assert rep.stage("emit").report["model_stl"] == source_stl


@pytest.mark.kernel
@pytest.mark.chitubox
def test_the_emitted_model_is_the_PREPPED_mesh_not_the_source(
        source_stl, tmp_path, monkeypatch):
    """Stage composition, at the only point where it can go wrong silently.

    The supports are computed on the hollow/drill stage's OUTPUT, so that is
    the mesh the project must embed. If emit were handed ``stl_path`` instead
    of ``prepped``, the file would carry supports computed for a different
    mesh -- the same wrong-model class of defect the emit path was rebuilt to
    make impossible.

    The fake hollow stage writes a mesh that is deliberately DIFFERENT from
    the source (half scale), because a hollow stage that happens to reproduce
    its input cannot tell the two apart. Everything downstream is real: real
    engine run, real emit, and the bbox is read back out of the finished file.
    """
    import chitubox_io
    half = mesh_io.load_stl(source_stl)
    half.apply_scale(0.5)

    def fake_hollow(in_path, out_path, wall=2.5, orient=True, log=None):
        half.export(out_path)
        log.update(status=hollow.STATUS_OK, decision="SOLID (test double)")
        return log

    monkeypatch.setattr(pipeline.hollow_mod, "run", fake_hollow)
    out = tmp_path / "prepped.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), orient=False)
    assert rep.status == pipeline.STATUS_OK, rep.error

    assert rep.prepped_stl != source_stl and os.path.isfile(rep.prepped_stl)
    assert rep.stage("emit").report["model_stl"] == rep.prepped_stl

    prepped = mesh_io.load_stl(rep.prepped_stl)
    center = (prepped.bounds[0] + prepped.bounds[1]) / 2.0
    m = chitubox_io.read_project(str(out))["models"][0]
    assert m["bbox_min"] == pytest.approx(list(prepped.bounds[0] - center), abs=1e-3)
    assert m["bbox_max"] == pytest.approx(list(prepped.bounds[1] - center), abs=1e-3)
    # and it is NOT the source: half scale means half the extents
    src = mesh_io.load_stl(source_stl)
    src_ext = src.bounds[1] - src.bounds[0]
    got_ext = np.asarray(m["bbox_max"]) - np.asarray(m["bbox_min"])
    assert got_ext == pytest.approx(src_ext * 0.5, abs=1e-3)
    # the supports were computed in that same frame: plate_z is lift below it
    assert rep.stage("supports").report["plate_z"] == pytest.approx(
        float(prepped.bounds[0][2] - center[2]) - rep.settings["lift_mm"], abs=1e-6)
    # the model is still named after the SOURCE mini, not the intermediate
    assert rep.stage("emit").report["model_name"] == "small_table"


def test_orientation_search_is_on_by_default(source_stl, tmp_path, monkeypatch):
    """Models arrive head-up but not support-optimised, and orientation must
    be final before drilling. Assert the DEFAULT, not just that the flag works."""
    seen = {}

    def fake_run(in_path, out_path, wall=2.5, orient=True, log=None):
        seen["orient"] = orient
        raise hollow.HollowError("stop here")

    monkeypatch.setattr(pipeline.hollow_mod, "run", fake_run)
    rep = pipeline.prep_one(source_stl, str(tmp_path / "o.chitubox"))
    assert seen["orient"] is True
    assert rep.settings["orient"] is True


# ---------------------------------------------------------------------------
# every failure class: non-zero exit AND a record
# ---------------------------------------------------------------------------

def _assert_failed(rep, stage, exit_code, out_path):
    assert rep.status == pipeline.STATUS_FAILED
    assert rep.ok is False
    assert rep.exit_code == exit_code
    assert rep.failed_stage == stage
    assert rep.error
    assert not os.path.exists(out_path), "a failed run must not leave an output"
    on_disk = json.loads(open(rep.report_path).read())
    assert on_disk["status"] == pipeline.STATUS_FAILED
    assert on_disk["failed_stage"] == stage
    assert on_disk["exit_code"] == exit_code
    assert on_disk["error"] == rep.error
    with pytest.raises(pipeline.PipelineError):
        rep.raise_for_status()


def test_missing_input_is_exit_1(tmp_path):
    out = tmp_path / "x.chitubox"
    rep = pipeline.prep_one(str(tmp_path / "nope.stl"), str(out))
    _assert_failed(rep, "input", pipeline.EXIT_INPUT, str(out))
    assert "no such STL" in rep.error


def test_hollow_verify_failure_is_exit_2(source_stl, tmp_path, monkeypatch):
    def fake_run(in_path, out_path, wall=2.5, orient=True, log=None):
        log.update(status=hollow.STATUS_VERIFY_FAILED,
                   check_detail={"single_body": {"ok": False, "measured": "4 bodies"},
                                 "cavity_breathes": {"ok": True, "measured": "True"}})
        log["failed_checks"] = hollow.failed_checks(log)
        return log

    monkeypatch.setattr(pipeline.hollow_mod, "run", fake_run)
    out = tmp_path / "h.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), orient=False)
    _assert_failed(rep, "hollow", pipeline.EXIT_HOLLOW, str(out))
    assert "single_body" in rep.error, "the failed check must reach the caller"
    assert rep.stage("hollow").report["failed_checks"] == ["single_body"]


def test_hollow_raising_is_also_exit_2(source_stl, tmp_path, monkeypatch):
    def boom(in_path, out_path, wall=2.5, orient=True, log=None):
        raise hollow.HollowError("input is not a watertight volume")

    monkeypatch.setattr(pipeline.hollow_mod, "run", boom)
    out = tmp_path / "h2.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), orient=False)
    _assert_failed(rep, "hollow", pipeline.EXIT_HOLLOW, str(out))
    assert "watertight" in rep.error


def test_support_engine_raising_is_exit_5(source_stl, tmp_path, monkeypatch):
    monkeypatch.setattr(pipeline.engine_mod, "run",
                        lambda *a, **k: (_ for _ in ()).throw(ValueError("kernel says no")))
    out = tmp_path / "s.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), hollow=False)
    _assert_failed(rep, "supports", pipeline.EXIT_SUPPORTS, str(out))
    assert "kernel says no" in rep.error


def test_failed_gates_are_exit_3(source_stl, tmp_path, monkeypatch):
    monkeypatch.setattr(pipeline.engine_mod, "run", lambda *a, **k: _empty_result())

    def refuse(*a, **k):
        raise emit.EmitRefused("refusing to emit: engine verify.ok is False.")

    monkeypatch.setattr(pipeline.emit_mod, "emit_project", refuse)
    out = tmp_path / "g.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), hollow=False)
    _assert_failed(rep, "emit", pipeline.EXIT_GATES, str(out))


def test_emit_failure_is_exit_4(source_stl, tmp_path, monkeypatch):
    monkeypatch.setattr(pipeline.engine_mod, "run", lambda *a, **k: _empty_result())

    def blow_up(*a, **k):
        raise emit.EmitError("chitubox_io refused the write [NonFiniteValue]")

    monkeypatch.setattr(pipeline.emit_mod, "emit_project", blow_up)
    out = tmp_path / "e.chitubox"
    rep = pipeline.prep_one(source_stl, str(out), hollow=False)
    _assert_failed(rep, "emit", pipeline.EXIT_EMIT, str(out))
    assert "NonFiniteValue" in rep.error


def test_deleted_geometry_is_announced_to_whoever_is_watching(tmp_path, monkeypatch):
    """Triage DELETES geometry, and it was reported only into the JSON blob:
    ``say`` printed decision/solid/final and nothing else, so an operator
    watching a batch scroll past had no way to know a component had been
    stripped, and no reason to go looking in the report. Detached bodies that
    were left alone go on the console for the same reason.

    (The support engine is stubbed out so the run stops right after the
    hollow stage -- the milestones already said are what is under test.)"""
    m = mesh_io.make_overhang_table()
    m.apply_scale(0.7)
    apex = m.vertices[np.lexsort(m.vertices.T)[-1]]
    e = -0.2
    tetra = trimesh.Trimesh(
        vertices=np.array([apex, apex + [e, 0, 0], apex + [0, e, 0], apex + [0, 0, e]]),
        faces=np.array([[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]]))
    trimesh.repair.fix_normals(tetra)
    tetra.invert()
    src = tmp_path / "shardy.stl"
    trimesh.util.concatenate([m, tetra]).export(str(src))

    monkeypatch.setattr(pipeline.engine_mod, "run",
                        lambda *a, **k: (_ for _ in ()).throw(ValueError("stop here")))
    said = []
    rep = pipeline.prep_one(str(src), str(tmp_path / "s.chitubox"),
                            orient=False, progress=said.append)
    assert rep.failed_stage == "supports", "the hollow stage must have run"
    assert rep.stage("hollow").report["dropped_shard_faces"] == 4

    dropped = [line for line in said if "dropped" in line]
    assert len(dropped) == 1, said
    assert "1 degenerate shard" in dropped[0] and "4 faces" in dropped[0]
    assert "0.200mm" in dropped[0]
    # the table is five overlapping boxes, so the four legs are extra bodies
    detached = [line for line in said if "detached body" in line]
    assert len(detached) == 1 and "4 detached" in detached[0]


def test_exit_codes_are_distinct():
    codes = [pipeline.EXIT_OK, pipeline.EXIT_INPUT, pipeline.EXIT_HOLLOW,
             pipeline.EXIT_GATES, pipeline.EXIT_EMIT, pipeline.EXIT_SUPPORTS]
    assert len(set(codes)) == len(codes)


# ---------------------------------------------------------------------------
# --skip-existing: cheap, idempotent re-runs over a folder
# ---------------------------------------------------------------------------

def test_skip_existing_does_no_work_and_touches_nothing(source_stl, tmp_path, monkeypatch):
    out = tmp_path / "already.chitubox"
    out.write_bytes(b"pre-existing bytes")

    def must_not_run(*a, **k):  # pragma: no cover - the assertion is that it isn't
        raise AssertionError("skip_existing did work anyway")

    monkeypatch.setattr(pipeline.hollow_mod, "run", must_not_run)
    monkeypatch.setattr(pipeline.engine_mod, "run", must_not_run)
    rep = pipeline.prep_one(source_stl, str(out), skip_existing=True)

    assert rep.status == pipeline.STATUS_SKIPPED
    assert rep.exit_code == pipeline.EXIT_OK and rep.ok
    assert rep.stages == ()
    assert out.read_bytes() == b"pre-existing bytes"
    rep.raise_for_status()  # a skip is not a failure


def test_skip_existing_ignores_an_empty_output(source_stl, tmp_path, monkeypatch):
    """A zero-byte file is the fingerprint of an interrupted run, not a
    finished one -- do the work."""
    out = tmp_path / "empty.chitubox"
    out.write_bytes(b"")
    monkeypatch.setattr(pipeline.hollow_mod, "run",
                        lambda *a, **k: (_ for _ in ()).throw(hollow.HollowError("ran")))
    rep = pipeline.prep_one(source_stl, str(out), skip_existing=True)
    assert rep.status == pipeline.STATUS_FAILED and "ran" in rep.error


@pytest.mark.kernel
@pytest.mark.chitubox
def test_second_run_with_skip_existing_is_a_no_op(source_stl, tmp_path):
    out = tmp_path / "idem.chitubox"
    first = pipeline.prep_one(source_stl, str(out), orient=False,
                              skip_existing=True, allow_failed_gates=True)
    assert first.status == pipeline.STATUS_OK
    stamp = (out.stat().st_mtime_ns, out.stat().st_size)
    second = pipeline.prep_one(source_stl, str(out), orient=False,
                               skip_existing=True, allow_failed_gates=True)
    assert second.status == pipeline.STATUS_SKIPPED
    assert (out.stat().st_mtime_ns, out.stat().st_size) == stamp


# ---------------------------------------------------------------------------
# batch + path derivation
# ---------------------------------------------------------------------------

def test_default_paths_are_derived_from_the_output():
    assert pipeline.default_prepped_path("/o/Morel.chitubox") == "/o/Morel.prepped.stl"
    assert pipeline.default_report_path("/o/Morel.chitubox") == "/o/Morel.pipeline.json"
    assert pipeline.default_out_path("/in/Morel Mudskipper.stl", "/o") \
        == "/o/Morel Mudskipper.chitubox"


def test_prep_batch_keeps_going_after_a_failure(source_stl, tmp_path, monkeypatch):
    monkeypatch.setattr(pipeline.hollow_mod, "run",
                        lambda *a, **k: (_ for _ in ()).throw(hollow.HollowError("nope")))
    missing = str(tmp_path / "gone.stl")
    reps = pipeline.prep_batch([source_stl, missing], str(tmp_path / "out"))
    assert len(reps) == 2
    assert [r.failed_stage for r in reps] == ["hollow", "input"]
    assert max(r.exit_code for r in reps) == pipeline.EXIT_HOLLOW
    assert reps[0].out_path == pipeline.default_out_path(source_stl, str(tmp_path / "out"))


def test_settings_record_the_config_that_ran(source_stl, tmp_path, monkeypatch):
    monkeypatch.setattr(pipeline.hollow_mod, "run",
                        lambda *a, **k: (_ for _ in ()).throw(hollow.HollowError("x")))
    cfg = config.SupportConfig(lift_mm=4.0, p8_relocate_enable=True)
    rep = pipeline.prep_one(source_stl, str(tmp_path / "c.chitubox"), cfg=cfg,
                            wall_mm=1.75)
    assert rep.settings["lift_mm"] == 4.0
    assert rep.settings["p8_relocate_enable"] is True
    assert rep.settings["wall_mm"] == 1.75


def test_report_path_none_writes_no_sidecar(source_stl, tmp_path, monkeypatch):
    monkeypatch.setattr(pipeline.hollow_mod, "run",
                        lambda *a, **k: (_ for _ in ()).throw(hollow.HollowError("x")))
    rep = pipeline.prep_one(source_stl, str(tmp_path / "n.chitubox"), report_path=None)
    assert rep.report_path is None
    assert not (tmp_path / "n.pipeline.json").exists()


# ---------------------------------------------------------------------------
# the operator CLI
# ---------------------------------------------------------------------------

def _run_cli(*argv):
    return subprocess.run([sys.executable, PREP_ONE, *argv],
                          capture_output=True, text=True)


@pytest.mark.kernel
@pytest.mark.chitubox
def test_cli_single_model_prints_one_json_line(source_stl, tmp_path):
    out = tmp_path / "cli.chitubox"
    r = _run_cli(source_stl, str(out), "--no-orient",
                 "--allow-failed-gates", "--quiet")
    assert r.returncode == 0, r.stderr
    lines = [ln for ln in r.stdout.splitlines() if ln.strip()]
    assert len(lines) == 1
    payload = json.loads(lines[0])
    assert payload["status"] == "ok" and payload["exit_code"] == 0
    assert out.is_file()


@pytest.mark.kernel
@pytest.mark.chitubox
def test_cli_batch_over_a_directory_with_skip_existing(source_stl, tmp_path):
    src_dir = os.path.dirname(source_stl)
    out_dir = tmp_path / "out"
    r1 = _run_cli("--out-dir", str(out_dir), src_dir, "--no-orient",
                  "--allow-failed-gates", "--skip-existing", "--quiet")
    assert r1.returncode == 0, r1.stderr
    produced = [json.loads(ln) for ln in r1.stdout.splitlines() if ln.strip()]
    assert produced and all(p["status"] == "ok" for p in produced)

    r2 = _run_cli("--out-dir", str(out_dir), src_dir, "--no-orient",
                  "--allow-failed-gates", "--skip-existing", "--quiet")
    assert r2.returncode == 0, r2.stderr
    again = [json.loads(ln) for ln in r2.stdout.splitlines() if ln.strip()]
    assert [p["status"] for p in again] == ["skipped"] * len(produced)


def test_cli_missing_stl_is_loud_on_stderr_and_exits_nonzero(tmp_path):
    out = tmp_path / "never.chitubox"
    r = _run_cli(str(tmp_path / "no_such.stl"), str(out))
    assert r.returncode == pipeline.EXIT_INPUT
    assert not out.exists()
    payload = json.loads(r.stdout.strip())
    assert payload["status"] == "failed"
    assert payload["failed_stage"] == "input"
    assert "FAILED" in r.stderr


def test_cli_rejects_ambiguous_argument_shapes(source_stl, tmp_path):
    r = _run_cli(source_stl, "--quiet")
    assert r.returncode == pipeline.EXIT_INPUT
    assert "exactly two positionals" in r.stderr

    r = _run_cli("--out-dir", str(tmp_path), source_stl, "--name", "x")
    assert r.returncode == pipeline.EXIT_INPUT
    assert "single-model options" in r.stderr

    r = _run_cli("--out-dir", str(tmp_path / "empty_dir_that_has_no_stls"),
                 str(tmp_path / "also_empty"))
    assert r.returncode == pipeline.EXIT_INPUT
