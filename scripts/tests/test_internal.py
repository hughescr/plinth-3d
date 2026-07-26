import hashlib
import json
from types import SimpleNamespace

import numpy as np
import pytest
from scipy import ndimage
import trimesh

from support_placement import config, hollow, internal, pipeline


def _authority(boxes, *, shape=(40, 40, 40), pitch=0.5, keepouts=(),
               modifier=None):
    cavity = np.zeros(shape, dtype=bool)
    for lo, hi in boxes:
        cavity[lo[0]:hi[0], lo[1]:hi[1], lo[2]:hi[2]] = True
    if modifier is not None:
        modifier(cavity)
    solid = ndimage.binary_dilation(
        cavity, structure=np.ones((3, 3, 3), dtype=bool)) & ~cavity
    transform = np.eye(4)
    transform[0, 0] = transform[1, 1] = transform[2, 2] = pitch
    inverse = np.linalg.inv(transform)
    cavity_fp = hollow._array_fingerprint((
        ("cavity_mask", np.packbits(cavity, bitorder="little"), np.uint8),
        ("transform", transform, "<f8"),
        ("inverse_transform", inverse, "<f8"),
    ))
    keepout_json = [{
        "role": d["role"],
        "p0": [round(float(v), 6) for v in d["p0"]],
        "p1": [round(float(v), 6) for v in d["p1"]],
        "axis": [round(float(v), 9) for v in d["axis"]],
        "radius_mm": round(float(d["radius_mm"]), 6),
    } for d in keepouts]
    descriptor = {
        "schema": "support-placement/a5-cavity",
        "schema_version": 1,
        "pitch_mm": pitch,
        "shape": list(shape),
        "cavity_voxels": int(cavity.sum()),
        "solid_voxels": int(solid.sum()),
        "outside_voxels": int((~(solid | cavity)).sum()),
        "cavity_fingerprint": cavity_fp,
        "inner_mesh_fingerprint": "synthetic-inner",
        "transform": transform.tolist(),
        "inverse_transform": inverse.tolist(),
        "drain_keepouts": keepout_json,
    }
    descriptor_fp = hashlib.sha256(json.dumps(
        descriptor, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    descriptor["descriptor_fingerprint"] = descriptor_fp
    sidecar = {
        "schema": descriptor["schema"],
        "schema_version": 1,
        "pitch_mm": pitch,
        "cavity_mask": cavity,
        "solid_mask": solid,
        "transform": transform,
        "inverse_transform": inverse,
        "drill_keepouts": tuple(keepouts),
        "cavity_fingerprint": cavity_fp,
        "inner_mesh_fingerprint": "synthetic-inner",
        "descriptor_fingerprint": descriptor_fp,
    }
    return sidecar, {"cavity_descriptor": descriptor}


def _plan(sidecar, report, cfg=None):
    mesh = trimesh.creation.box(extents=(20.0, 20.0, 20.0))
    return internal.plan_and_verify(
        mesh=mesh,
        result={"verify": {"ok": True}},
        cfg=cfg or config.SupportConfig(),
        hollow_report=report,
        cavity=sidecar,
        mesh_center=np.zeros(3),
    )


def test_broad_inner_roof_gets_contained_columns_and_all_demands_pass():
    sidecar, report = _authority([((4, 4, 4), (36, 36, 36))])
    got = _plan(sidecar, report)
    assert got["ok"] is True
    assert got["n_members"] > 1
    assert got["n_demands"] > 800
    assert got["n_overhang_demands"] == got["n_a3_demands"]
    assert got["n_minimum_demands"] > 0
    assert got["n_birth_demands"] > 0
    assert got["radius_max_mm"] <= 1.0
    assert got["max_coverage_distance_mm"] <= 4.0
    assert got["gates"]["ok"] is True
    assert all(not v for k, v in got["gates"].items() if k != "ok")
    assert all(m["kind"] == "internal" and m["embed_end"] is None
               for m in got["members"])


def test_real_hollow_sidecar_preserves_post_refill_authority(tmp_path):
    source = tmp_path / "sphere.stl"
    prepped = tmp_path / "sphere.hollow.stl"
    backup = tmp_path / "sphere.oriented-solid.stl"
    trimesh.creation.icosphere(subdivisions=3, radius=18.0).export(source)
    cavity = {}
    hreport = {}
    hollow.run(
        str(source), str(prepped), wall=2.5, orient=False, log=hreport,
        allow_hollow_output=True, cavity_out=cavity,
        solid_backup_path=str(backup),
    )
    assert hreport["status"] == hollow.STATUS_OK, (
        hreport.get("failed_checks"), hreport.get("check_detail"))
    assert hreport["final_geometry"] == "hollow"
    assert prepped.is_file() and backup.is_file()
    assert cavity["cavity_fingerprint"] == (
        hreport["cavity_descriptor"]["cavity_fingerprint"])
    assert np.array_equal(
        cavity["cavity_mask"],
        np.frombuffer(cavity["cavity_mask"].tobytes(), dtype=bool).reshape(
            cavity["cavity_mask"].shape))
    assert cavity["cavity_mask"].flags.writeable is False


def test_drain_keepout_is_clear():
    keepout = {
        "role": "drain",
        "p0": [10.0, 10.0, 0.0],
        "p1": [10.0, 10.0, 20.0],
        "axis": [0.0, 0.0, 1.0],
        "radius_mm": 1.5,
    }
    sidecar, report = _authority(
        [((4, 4, 4), (36, 36, 36))], keepouts=(keepout,))
    got = _plan(sidecar, report)
    assert got["ok"] is True
    for member in got["members"]:
        xy = np.asarray(member["p0"])[:2]
        assert np.linalg.norm(xy - [10.0, 10.0]) > 1.5 + member["r0"] + 0.6
    assert got["gates"]["keepout_violations"] == []


def test_narrow_shell_crossing_corridor_is_refused():
    def pinched(cavity):
        cavity[8:32, 8:32, 4:24] = True
        cavity[20:21, 20:21, 24:31] = True
        cavity[19:22, 19:22, 31:36] = True

    sidecar, report = _authority([], modifier=pinched)
    got = _plan(sidecar, report)
    assert got["ok"] is False
    assert got["fallback_required"] is True
    assert got["reason_code"] == "unroutable_or_trapped_internal_lobe"


def test_narrowed_approach_rejects_centerline_only_false_positive():
    def neck(cavity):
        cavity[:] = False
        cavity[4:36, 4:36, 4:28] = True
        cavity[20:21, 20:21, 28:31] = True
        cavity[19:22, 19:22, 31:36] = True

    sidecar, report = _authority([], modifier=neck)
    got = _plan(sidecar, report)
    assert got["ok"] is False
    assert got["reason_code"] == "unroutable_or_trapped_internal_lobe"
    assert got["provenance"]["route_candidate_counts"][
        "swept_radius_contained"] < got["provenance"]["route_candidate_counts"][
            "same_run_lower_wall"]


def test_one_unroutable_lobe_refuses_the_whole_hollow():
    def trapped(cavity):
        cavity[3:18, 3:18, 4:36] = True
        cavity[30:31, 30:31, 8:18] = True
        cavity[29:32, 29:32, 18:30] = True

    sidecar, report = _authority([], modifier=trapped)
    got = _plan(sidecar, report)
    assert got["ok"] is False
    assert got["reason_code"] == "unroutable_or_trapped_internal_lobe"
    assert got["provenance"]["unsupported_lobes"]


def test_determinism_and_provenance_fingerprint():
    sidecar, report = _authority([((4, 4, 4), (36, 36, 36))])
    a = _plan(sidecar, report)
    b = _plan(sidecar, report)
    assert a["demand_fingerprint"] == b["demand_fingerprint"]
    assert a["support_fingerprint"] == b["support_fingerprint"]
    assert a["provenance"] == b["provenance"]
    sidecar["cavity_mask"] = sidecar["cavity_mask"].copy()
    sidecar["cavity_mask"][10, 10, 10] = False
    bad = _plan(sidecar, report)
    assert bad["reason_code"] == "cavity_fingerprint_mismatch"


def test_pipeline_a5_refusal_uses_preserved_solid_and_reruns_exterior(
        tmp_path, monkeypatch):
    source = tmp_path / "source.stl"
    trimesh.creation.box(extents=(20.0, 20.0, 20.0)).export(source)
    sidecar, hreport = _authority([((4, 4, 4), (36, 36, 36))])
    calls = []

    def fake_hollow(in_path, out_path, wall=2.5, orient=True, log=None,
                    allow_hollow_output=False, cavity_out=None,
                    solid_backup_path=None):
        mesh = trimesh.load(in_path)
        mesh.export(out_path)
        mesh.export(solid_backup_path)
        cavity_out.update(sidecar)
        log.update(
            hreport,
            status=hollow.STATUS_OK,
            decision="HOLLOW",
            final_geometry="hollow",
            hollow_eligible=True,
            hollow_fallback_applied=False,
            oriented_solid_backup=str(solid_backup_path),
        )

    def fake_engine(mesh, cfg, log=None):
        calls.append(len(mesh.faces))
        return {
            "members": [],
            "balls": [],
            "unrouted": [],
            "contacts": [],
            "plate_z": float(mesh.bounds[0][2] - cfg.lift_mm),
            "verify": {"ok": True, "uncovered_islands": []},
            "volume": {},
        }

    def fake_emit(model_stl, result, out_path, cfg, **kwargs):
        with open(out_path, "wb") as fh:
            fh.write(b"a5-solid-fallback")
        return SimpleNamespace(
            n_records=1,
            bytes_written=len(b"a5-solid-fallback"),
            to_dict=lambda: {
                "model_stl": model_stl,
                "n_records": 1,
                "bytes_written": len(b"a5-solid-fallback"),
            },
        )

    monkeypatch.setattr(pipeline.hollow_mod, "run", fake_hollow)
    monkeypatch.setattr(pipeline.engine_mod, "run", fake_engine)
    monkeypatch.setattr(
        pipeline.internal_mod, "plan_and_verify",
        lambda **kwargs: {
            "ok": False,
            "fallback_required": True,
            "reason_code": "unroutable_or_trapped_internal_lobe",
            "reason": "synthetic refusal",
            "members": [],
        })
    monkeypatch.setattr(pipeline.emit_mod, "emit_project", fake_emit)

    out = tmp_path / "fallback.chitubox"
    got = pipeline.prep_one(str(source), str(out), orient=False)
    assert got.ok is True
    assert len(calls) == 2
    assert got.stage("hollow").report["final_geometry"] == "solid"
    assert got.stage("hollow").report["hollow_fallback_reason"] == (
        "unroutable_or_trapped_internal_lobe")
    assert got.stage("supports").report["solid_fallback"] is True
    assert got.stage("supports").report["gates"]["ok"] is True
    json.dumps(got.to_dict())


def test_allow_failed_exterior_cannot_bypass_a5(tmp_path, monkeypatch):
    source = tmp_path / "source.stl"
    trimesh.creation.box(extents=(20.0, 20.0, 20.0)).export(source)
    sidecar, hreport = _authority([((4, 4, 4), (36, 36, 36))])
    called = {"a5": 0, "emit": 0}

    def fake_hollow(in_path, out_path, wall=2.5, orient=True, log=None,
                    allow_hollow_output=False, cavity_out=None,
                    solid_backup_path=None):
        mesh = trimesh.load(in_path)
        mesh.export(out_path)
        mesh.export(solid_backup_path)
        cavity_out.update(sidecar)
        log.update(
            hreport,
            status=hollow.STATUS_OK,
            decision="HOLLOW",
            final_geometry="hollow",
            hollow_eligible=True,
            hollow_fallback_applied=False,
            oriented_solid_backup=str(solid_backup_path),
        )

    def fake_engine(mesh, cfg, log=None):
        return {
            "members": [], "balls": [], "unrouted": [], "contacts": [],
            "plate_z": float(mesh.bounds[0][2] - cfg.lift_mm),
            "verify": {"ok": False, "uncovered_islands": [1]},
            "volume": {},
        }

    member = {
        "p0": np.array([0.0, 0.0, 2.0]),
        "p1": np.array([0.0, 0.0, -2.0]),
        "r0": 0.3, "r1": 0.3,
        "owner": ("internal", 0), "kind": "internal", "embed_end": None,
    }

    def fake_a5(**kwargs):
        called["a5"] += 1
        assert kwargs["result"]["verify"]["ok"] is False
        return {
            "ok": True, "fallback_required": False,
            "gates": {"ok": True, "containment_violations": []},
            "members": [member],
        }

    def fake_emit(model_stl, result, out_path, cfg, **kwargs):
        called["emit"] += 1
        assert kwargs["allow_failed_gates"] is True
        assert len(result["members"]) == 1
        with open(out_path, "wb") as fh:
            fh.write(b"allowed-exterior-only")
        return SimpleNamespace(
            n_records=1, bytes_written=21,
            to_dict=lambda: {"n_records": 1, "bytes_written": 21},
        )

    monkeypatch.setattr(pipeline.hollow_mod, "run", fake_hollow)
    monkeypatch.setattr(pipeline.engine_mod, "run", fake_engine)
    monkeypatch.setattr(pipeline.internal_mod, "plan_and_verify", fake_a5)
    monkeypatch.setattr(pipeline.emit_mod, "emit_project", fake_emit)
    got = pipeline.prep_one(
        str(source), str(tmp_path / "allowed.chitubox"),
        orient=False, allow_failed_gates=True)
    assert got.ok is True
    assert called == {"a5": 1, "emit": 1}
    assert got.stage("supports").report["n_internal_members"] == 1
    assert got.stage("supports").report["exterior_hollow_gates"]["ok"] is False


@pytest.mark.chitubox
def test_successful_pipeline_appends_internal_member_and_emits_without_raft(
        tmp_path, monkeypatch):
    source = tmp_path / "source.stl"
    trimesh.creation.box(extents=(20.0, 20.0, 20.0)).export(source)
    sidecar, hreport = _authority([((4, 4, 4), (36, 36, 36))])

    def fake_hollow(in_path, out_path, wall=2.5, orient=True, log=None,
                    allow_hollow_output=False, cavity_out=None,
                    solid_backup_path=None):
        mesh = trimesh.load(in_path)
        mesh.export(out_path)
        mesh.export(solid_backup_path)
        cavity_out.update(sidecar)
        log.update(
            hreport,
            status=hollow.STATUS_OK,
            decision="HOLLOW",
            final_geometry="hollow",
            hollow_eligible=True,
            hollow_fallback_applied=False,
            oriented_solid_backup=str(solid_backup_path),
        )

    def fake_engine(mesh, cfg, log=None):
        return {
            "members": [], "balls": [], "unrouted": [], "contacts": [],
            "plate_z": float(mesh.bounds[0][2] - cfg.lift_mm),
            "verify": {"ok": True, "uncovered_islands": []},
            "volume": {},
        }

    monkeypatch.setattr(pipeline.hollow_mod, "run", fake_hollow)
    monkeypatch.setattr(pipeline.engine_mod, "run", fake_engine)
    monkeypatch.setattr(
        pipeline.internal_mod, "plan_and_verify",
        lambda **kwargs: {
            "ok": True,
            "fallback_required": False,
            "support_fingerprint": "pipeline-test",
            "gates": {"ok": True, "containment_violations": []},
            "members": [{
                "p0": np.array([0.0, 0.0, 2.0]),
                "p1": np.array([0.0, 0.0, -2.0]),
                "r0": 0.3, "r1": 0.3,
                "owner": ("internal", 0),
                "kind": "internal",
                "embed_end": None,
            }],
        })

    out = tmp_path / "internal.chitubox"
    got = pipeline.prep_one(str(source), str(out), orient=False)
    assert got.ok is True
    assert got.stage("supports").report["n_internal_members"] == 1
    emitted = got.stage("emit").report
    assert emitted["n_members"] == 1
    assert emitted["n_pads"] == 0
    assert emitted["n_bridges"] == 0
    assert emitted["reparse_ok"] is True
    assert out.is_file() and out.stat().st_size > 0
