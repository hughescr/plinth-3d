"""The emit boundary: ``support_placement.emit.emit_project``.

Two things must hold and be measurable here:
  1. the refactor that split geometry (``member_chain_specs``) out of the
     legacy byte path (``synth_member_chain``) changed NOTHING;
  2. the file that lands on disk carries the model it was told to carry.
"""
import os
import struct

import numpy as np
import pytest

from support_placement import chitubox_emit as ce
from support_placement import chitubox_inject as ci
from support_placement import config, emit, engine, mesh_io

FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures", "chitubox")
CYLINDER_STL = os.path.join(FIXTURES, "Cylinder.stl")


def _stl_face_count(path):
    with open(path, "rb") as fh:
        fh.seek(80)
        return struct.unpack("<I", fh.read(4))[0]


def _mk_member(p0, p1, r0, r1, kind, owner, embed_end=None):
    return dict(p0=np.asarray(p0, float), p1=np.asarray(p1, float),
                r0=float(r0), r1=float(r1), kind=kind, owner=owner,
                embed_end=embed_end)


# ---------------------------------------------------------------------------
# the refactor is behavior-preserving
# ---------------------------------------------------------------------------

def _spec_cases(cfg):
    """Members exercising every branch member_chain_specs has: plate-rooted
    (flare split), short plate-rooted (half flare), a stub (contact ball), a
    tapered member with an embed_end (T_TAPER), a plain shoot (T_SHAFT)."""
    return [
        _mk_member([4, 0, 3], [4, 0, -5], 1.0, 1.0, "trunk", ("contact", 0)),
        _mk_member([0, 0, -5 + cfg.flare_len_mm * 1.2], [0, 0, -5],
                   0.8, 0.8, "trunk", ("contact", 1)),
        _mk_member([1, 1, 6], [1, 1, 3], 0.25, 0.5, "stub", ("contact", 2),
                   embed_end="p0"),
        _mk_member([5, 0, 2], [0, 0, 2], 0.3, 0.4, "shoot", ("contact", 3)),
    ]


def test_specs_and_legacy_byte_path_agree(cfg):
    """Same count, same type ids, same geometry, BYTE-IDENTICAL blobs, and
    contiguous offsets. This is what stops the surviving chitubox_inject path
    and the chitubox_io path from drifting."""
    members = _spec_cases(cfg)
    specs = ce.member_chain_specs(members, [], cfg, plate_z=-5.0)
    recs = ce.synth_member_chain(members, [], cfg, 4096, plate_z=-5.0)
    assert len(specs) == len(recs)

    off = 4096
    for spec, (rec_bytes, blob) in zip(specs, recs):
        rec = ci.Record(rec_bytes)
        assert rec.type == spec.type_id
        assert np.allclose(rec.start, spec.start)
        assert np.allclose(rec.end, spec.end)
        assert rec.r1 == pytest.approx(spec.r0, rel=1e-6)
        assert rec.r2 == pytest.approx(spec.r1, rel=1e-6)
        assert blob == spec.mesh.tobytes()
        assert rec.mesh_off == off
        assert rec.mesh_len == len(blob)
        off += len(blob)


def test_specs_carry_the_trailing_convention(cfg):
    """T_TAPER carries contact_depth in trailing[0:4]; every other type is
    24 zero bytes (the corpus-evidenced convention)."""
    members = _spec_cases(cfg)
    specs = ce.member_chain_specs(members, [], cfg, plate_z=-5.0)
    tapers = [s for s in specs if s.type_id == ci.T_TAPER]
    assert tapers, "fixture must include a tapered embed_end member"
    for s in specs:
        assert len(s.trailing) == 24
        if s.type_id == ci.T_TAPER:
            assert struct.unpack("<f", s.trailing[:4])[0] == pytest.approx(
                cfg.contact_depth_mm)
        else:
            assert s.trailing == bytes(24)


def test_specs_meshes_are_c_contiguous_float32(cfg):
    for s in ce.member_chain_specs(_spec_cases(cfg), [], cfg, plate_z=-5.0):
        assert s.mesh.dtype == np.dtype("<f4")
        assert s.mesh.ndim == 3 and s.mesh.shape[1:] == (3, 3)


def test_support_arrays_pack_one_concatenated_buffer(cfg):
    specs = ce.member_chain_specs(_spec_cases(cfg), [], cfg, plate_z=-5.0)
    a = emit.support_arrays(specs)
    n = len(specs)
    assert a["seg_type"].shape == (n,) and a["seg_type"].dtype == np.uint32
    assert a["start"].shape == (n, 3) and a["end"].shape == (n, 3)
    assert a["radii"].shape == (n, 2) and a["trailing"].shape == (n, 24)
    assert a["mesh_len"].sum() == a["mesh_flat"].size
    # the concatenated buffer must reproduce each record's own blob
    off = 0
    for s, ln in zip(specs, a["mesh_len"]):
        assert np.array_equal(a["mesh_flat"][off:off + ln],
                              s.mesh.reshape(-1))
        off += int(ln)


def test_support_arrays_refuses_an_empty_forest(cfg):
    with pytest.raises(emit.EmitError, match="empty forest"):
        emit.support_arrays([])


# ---------------------------------------------------------------------------
# emit_project against a real (small) engine run
# ---------------------------------------------------------------------------

@pytest.fixture(scope="module")
def table_run():
    """One real engine run, reused by the emit tests in this module."""
    cfg = config.SupportConfig(lift_mm=3.0)
    mesh = mesh_io.make_overhang_table()
    mesh.apply_translation(-(mesh.bounds[0] + mesh.bounds[1]) / 2.0)
    return cfg, mesh, engine.run(mesh, cfg)


@pytest.fixture
def table_stl(tmp_path_factory, table_run):
    """The centered table written out as the STL emit_project consumes.

    NOTE the frame: emit_project re-centers whatever STL it is handed, so an
    already-centered mesh round-trips to itself and plate_z stays valid.
    """
    _, mesh, _ = table_run
    p = tmp_path_factory.mktemp("stl") / "table.stl"
    mesh.export(str(p))
    return str(p)


@pytest.mark.kernel
@pytest.mark.chitubox
def test_emit_project_writes_the_forest(table_run, table_stl, tmp_path):
    cfg, mesh, result = table_run
    out = tmp_path / "table.chitubox"
    rep = emit.emit_project(table_stl, result, str(out), cfg,
                            allow_failed_gates=True)

    specs = ce.member_chain_specs(result["members"], result["balls"], cfg,
                                  plate_z=result["plate_z"])
    assert rep.n_records == len(specs)
    assert rep.n_members == len(result["members"])
    assert rep.reparse_ok is True
    assert out.is_file() and rep.bytes_written == out.stat().st_size

    # the frame invariant, not re-derived from a corpus specimen
    zmin_centered = float(mesh.bounds[0][2])
    assert zmin_centered - rep.plate_z == pytest.approx(cfg.lift_mm, abs=1e-6)
    assert rep.lift_mm == pytest.approx(cfg.lift_mm)


@pytest.mark.kernel
@pytest.mark.chitubox
def test_emitted_records_bottom_out_at_the_plate(table_run, table_stl, tmp_path):
    import chitubox_io
    cfg, _, result = table_run
    out = tmp_path / "plate.chitubox"
    emit.emit_project(table_stl, result, str(out), cfg, allow_failed_gates=True)
    recs = chitubox_io.read_project(str(out))["models"][0]["supports"]
    zs = [min(r["start"][2], r["end"][2]) for r in recs]
    assert min(zs) == pytest.approx(result["plate_z"], abs=ce.PLATE_BAND_MM)
    assert min(zs) >= result["plate_z"] - ce.PLATE_BAND_MM


@pytest.mark.kernel
@pytest.mark.chitubox
def test_emit_report_is_json_safe(table_run, table_stl, tmp_path):
    import json
    cfg, _, result = table_run
    rep = emit.emit_project(table_stl, result, str(tmp_path / "j.chitubox"),
                            cfg, allow_failed_gates=True)
    json.dumps(rep.to_dict())  # numpy scalars leaking here have bitten before


@pytest.mark.kernel
@pytest.mark.chitubox
def test_emit_refuses_a_gate_failing_result(table_run, table_stl, tmp_path):
    cfg, _, result = table_run
    bad = dict(result, verify=dict(result["verify"], ok=False))
    out = tmp_path / "refused.chitubox"
    with pytest.raises(emit.EmitRefused):
        emit.emit_project(table_stl, bad, str(out), cfg)
    assert not out.exists(), "refusal must happen BEFORE anything is written"


@pytest.mark.kernel
@pytest.mark.chitubox
def test_emit_rejects_a_frame_mismatch(table_run, table_stl, tmp_path):
    """A result whose plate_z does not sit lift_mm below the STL's zmin is not
    from this STL's run -- refuse rather than emit a misplaced forest."""
    cfg, _, result = table_run
    wrong = dict(result, plate_z=result["plate_z"] - 5.0)
    with pytest.raises(emit.EmitError, match="frame mismatch"):
        emit.emit_project(table_stl, wrong, str(tmp_path / "x.chitubox"), cfg,
                          allow_failed_gates=True)


@pytest.mark.chitubox
def test_emit_rejects_a_missing_stl(tmp_path):
    cfg = config.SupportConfig(lift_mm=3.0)
    with pytest.raises(emit.EmitError, match="not found"):
        emit.emit_project(str(tmp_path / "nope.stl"),
                          dict(members=[], balls=[], plate_z=0.0,
                               verify=dict(ok=True)),
                          str(tmp_path / "o.chitubox"), cfg)


# ---------------------------------------------------------------------------
# model fidelity (task #60): the file carries the model it was handed
# ---------------------------------------------------------------------------

@pytest.mark.chitubox
def test_emitted_model_matches_the_source_stl(tmp_path):
    """One synthetic member over the Cylinder fixture: the embedded model must
    be the CYLINDER, with the cylinder's own triangle count and bbox."""
    import chitubox_io
    cfg = config.SupportConfig(lift_mm=3.0)
    src = mesh_io.load_stl(CYLINDER_STL)
    center = (src.bounds[0] + src.bounds[1]) / 2.0
    zmin_centered = float(src.bounds[0][2] - center[2])
    plate_z = zmin_centered - cfg.lift_mm

    members = [_mk_member([0, 0, zmin_centered], [0, 0, plate_z],
                          0.5, 0.5, "trunk", ("contact", 0))]
    result = dict(members=members, balls=[], plate_z=plate_z,
                  verify=dict(ok=True))
    out = tmp_path / "cyl.chitubox"
    rep = emit.emit_project(CYLINDER_STL, result, str(out), cfg)

    assert rep.model_triangles == _stl_face_count(CYLINDER_STL)
    assert rep.model_triangles == rep.source_triangles == len(src.faces)
    assert rep.center_delta_mm < 1e-3

    m = chitubox_io.read_project(str(out))["models"][0]
    assert m["bbox_min"] == pytest.approx(list(src.bounds[0] - center), abs=1e-3)
    assert m["bbox_max"] == pytest.approx(list(src.bounds[1] - center), abs=1e-3)
    assert m["placement"] == pytest.approx((0.0, 0.0, cfg.lift_mm))
