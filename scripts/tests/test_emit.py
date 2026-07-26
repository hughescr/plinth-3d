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
    specs += ce.bottom_connected_specs(result["members"], result["plate_z"])
    assert rep.n_records == len(specs)
    assert rep.n_pads >= 1 and rep.n_bridges == rep.n_pads - 1
    assert rep.n_members == len(result["members"])
    assert rep.reparse_ok is True
    assert out.is_file() and rep.bytes_written == out.stat().st_size

    # the frame invariant, not re-derived from a corpus specimen
    zmin_centered = float(mesh.bounds[0][2])
    assert zmin_centered - rep.plate_z == pytest.approx(cfg.lift_mm, abs=1e-6)
    assert rep.lift_mm == pytest.approx(cfg.lift_mm)


@pytest.mark.kernel
@pytest.mark.chitubox
def test_every_emitted_mesh_vertex_is_on_or_above_plate(table_run, table_stl, tmp_path):
    cfg, _, result = table_run
    plate_z = result["plate_z"]
    specs = ce.member_chain_specs(
        result["members"], result["balls"], cfg, plate_z=plate_z)
    specs += ce.bottom_connected_specs(result["members"], plate_z)
    ce.validate_support_mesh_plate_bound(specs, plate_z)
    all_z = np.concatenate([s.mesh[..., 2].reshape(-1) for s in specs])
    assert all_z.min() == pytest.approx(plate_z, abs=ce.PLATE_MESH_TOL_MM)
    assert all_z.min() >= plate_z - ce.PLATE_MESH_TOL_MM

    out = tmp_path / "plate.chitubox"
    emit.emit_project(table_stl, result, str(out), cfg, allow_failed_gates=True)
    assert out.is_file()


def test_plate_bound_rejects_bad_mesh_even_when_endpoints_are_valid(cfg):
    good = ce.member_chain_specs(
        [_mk_member([0, 0, 5], [0, 0, -5], 0.5, 0.5,
                    "trunk", ("contact", 0))],
        [], cfg, plate_z=-5.0)
    bad_mesh = good[0].mesh.copy()
    bad_mesh[0, 0, 2] = -5.1
    bad = ce.SegmentSpec(
        type_id=good[0].type_id,
        start=good[0].start.copy(),
        end=good[0].end.copy(),
        r0=good[0].r0,
        r1=good[0].r1,
        trailing=good[0].trailing,
        mesh=bad_mesh)
    assert min(bad.start[2], bad.end[2]) >= -5.0
    with pytest.raises(emit.EmitError, match="mesh reaches"):
        emit.support_arrays([bad], plate_z=-5.0)


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


# ---------------------------------------------------------------------------
# the forest survives the crossing, record for record
# ---------------------------------------------------------------------------

def _cylinder_frame(cfg):
    """(source mesh, centered zmin, plate_z) for the Cylinder fixture."""
    src = mesh_io.load_stl(CYLINDER_STL)
    center = (src.bounds[0] + src.bounds[1]) / 2.0
    zmin_centered = float(src.bounds[0][2] - center[2])
    return src, zmin_centered, zmin_centered - cfg.lift_mm


def _mixed_forest(cfg, plate_z, zmin_centered):
    """A forest that exercises every branch of the spec builder: a plate-rooted
    trunk (flare + column = TWO records), a second differently-sized root,
    a tapered stub with an embed_end (T_TAPER + tip ball), a plain shoot
    (T_SHAFT), and a free joint ball."""
    members = [
        _mk_member([0, 0, zmin_centered], [0, 0, plate_z], 0.6, 0.6,
                   "trunk", ("contact", 0)),
        _mk_member([10, 0, zmin_centered], [10, 0, plate_z], 1.1, 1.1,
                   "trunk", ("contact", 3)),
        _mk_member([3, 0, -4], [3, 0, -8], 0.25, 0.5, "stub", ("contact", 1),
                   embed_end="p0"),
        _mk_member([5, 0, -6], [0, 0, -6], 0.3, 0.3, "shoot", ("contact", 2)),
    ]
    balls = [dict(center=np.array([0.0, 0.0, -6.0]), r=0.4)]
    return members, balls


@pytest.mark.chitubox
def test_every_record_round_trips_through_the_container(tmp_path):
    """Read the finished file back and compare it to the SPECS, record for
    record: type, endpoints, both radii, and each record's own triangle count.

    A count-only check (which is all the other emit tests do) passes happily
    if the columnar batch is mis-packed -- start/end swapped, radii
    transposed, records reordered, or ``mesh_len`` off by one so every
    record's geometry is stolen from its neighbour. This is what fails then.
    """
    import chitubox_io
    cfg = config.SupportConfig(lift_mm=3.0)
    _, zmin_centered, plate_z = _cylinder_frame(cfg)
    members, balls = _mixed_forest(cfg, plate_z, zmin_centered)
    result = dict(members=members, balls=balls, plate_z=plate_z,
                  verify=dict(ok=True))

    out = tmp_path / "forest.chitubox"
    rep = emit.emit_project(CYLINDER_STL, result, str(out), cfg)

    specs = ce.member_chain_specs(members, balls, cfg, plate_z=plate_z)
    specs += ce.bottom_connected_specs(members, plate_z)
    assert len(specs) == 9, \
        "fixture must cover two flare-splits + stub + shoot + pads + bridge"
    assert {s.type_id for s in specs} == {
        ci.T_FLARE, ci.T_TAPER, ci.T_SHAFT, ci.T_PAD}
    assert rep.n_records == len(specs) and rep.n_flares == 2
    assert rep.n_pads == 2 and rep.n_bridges == 1
    assert rep.model_triangles == _stl_face_count(CYLINDER_STL)

    recs = chitubox_io.read_project(str(out))["models"][0]["supports"]
    assert len(recs) == len(specs)
    for i, (spec, r) in enumerate(zip(specs, recs)):
        assert r["seg_type"] == spec.type_id, f"record {i} type"
        assert r["start"] == pytest.approx(list(spec.start), abs=1e-5), f"record {i} start"
        assert r["end"] == pytest.approx(list(spec.end), abs=1e-5), f"record {i} end"
        assert r["radii"] == pytest.approx((spec.r0, spec.r1), abs=1e-6), f"record {i} radii"
        assert r["triangle_count"] == spec.mesh.shape[0], f"record {i} mesh slice"

    # Inspect the serialized record bytes and cached blobs after reopen. This
    # pins that the Rust from-empty writer preserves the exact native type-6
    # fields, not merely the Python-side specs before crossing the boundary.
    raw = ci.Project(str(out))
    type6 = [r for r in raw.records if r.type == ci.T_PAD]
    bridges = [r for r in type6 if not np.allclose(r.start[:2], r.end[:2])]
    pads = [r for r in type6 if np.allclose(r.start[:2], r.end[:2])]
    assert len(pads) == 2 and len(bridges) == 1
    for r in pads:
        assert bytes(r.raw[48:72]) == struct.pack("<2f", 0.0, 4.0) + bytes(16)
        assert r.r1 == pytest.approx(2.2) and r.r2 == pytest.approx(0.2)
        assert raw.blob(r).shape == (240, 3)  # 80 triangles
    bridge = bridges[0]
    assert bytes(bridge.raw[48:72]) == struct.pack("<2f", 1.0, 4.0) + bytes(16)
    assert bridge.r1 == pytest.approx(2.2) and bridge.r2 == pytest.approx(0.2)
    assert bridge.start[2] == 0 and bridge.end[2] == 0
    assert raw.blob(bridge).shape == (36, 3)  # 12 triangles


@pytest.mark.chitubox
def test_the_whole_forest_crosses_the_boundary_once(tmp_path, monkeypatch):
    """ONE ``write_project`` call carrying ALL records. Catches a regression to
    a per-record crossing (16 MB of triangle soup at treant scale would go over
    the boundary 2700 times instead of once)."""
    import chitubox_io
    cfg = config.SupportConfig(lift_mm=3.0)
    _, zmin_centered, plate_z = _cylinder_frame(cfg)
    members, balls = _mixed_forest(cfg, plate_z, zmin_centered)
    real = chitubox_io.write_project
    calls = []

    def counting(out_path, models):
        calls.append(models)
        return real(out_path, models)

    monkeypatch.setattr(chitubox_io, "write_project", counting)
    rep = emit.emit_project(CYLINDER_STL,
                            dict(members=members, balls=balls, plate_z=plate_z,
                                 verify=dict(ok=True)),
                            str(tmp_path / "once.chitubox"), cfg)

    assert len(calls) == 1, f"emit crossed the boundary {len(calls)} times"
    models = calls[0]
    assert len(models) == 1
    sup = models[0]["supports"]
    assert sup["seg_type"].size == rep.n_records
    # one concatenated mesh buffer, not one per record
    assert sup["mesh_flat"].ndim == 1
    assert int(sup["mesh_len"].sum()) == sup["mesh_flat"].size


# ---------------------------------------------------------------------------
# the wrong-model failure mode is gone STRUCTURALLY, not guarded at runtime
# ---------------------------------------------------------------------------

@pytest.mark.chitubox
def test_two_emits_in_one_process_carry_their_own_models(tmp_path):
    """Emit the cylinder, then the sphere, from one process. Each file must
    carry its OWN mesh.

    This is the exact shape of the failure the old clone-and-patch emitter had:
    ``chitubox_inject.write_replace`` copies the template's model through into
    its output, so a second emit against a model-bearing template produced
    correct supports stapled onto the first model. Triangle counts and bboxes
    here differ enough that any such carry-over is visible.
    """
    import chitubox_io
    sphere_stl = os.path.join(FIXTURES, "Sphere.stl")
    cfg = config.SupportConfig(lift_mm=3.0)

    written = {}
    for path in (CYLINDER_STL, sphere_stl):
        src = mesh_io.load_stl(path)
        center = (src.bounds[0] + src.bounds[1]) / 2.0
        zmin_centered = float(src.bounds[0][2] - center[2])
        plate_z = zmin_centered - cfg.lift_mm
        result = dict(
            members=[_mk_member([0, 0, zmin_centered], [0, 0, plate_z],
                                0.5, 0.5, "trunk", ("contact", 0))],
            balls=[], plate_z=plate_z, verify=dict(ok=True))
        out = tmp_path / (os.path.basename(path) + ".chitubox")
        rep = emit.emit_project(path, result, str(out), cfg)
        written[path] = (rep, chitubox_io.read_project(str(out))["models"][0], src, center)

    (cyl_rep, cyl_m, cyl_src, cyl_c) = written[CYLINDER_STL]
    (sph_rep, sph_m, sph_src, sph_c) = written[sphere_stl]

    assert cyl_rep.model_triangles == _stl_face_count(CYLINDER_STL)
    assert sph_rep.model_triangles == _stl_face_count(sphere_stl)
    assert cyl_rep.model_triangles != sph_rep.model_triangles, \
        "fixtures must differ, or this test cannot see carry-over"
    assert cyl_m["name"] == "Cylinder" and sph_m["name"] == "Sphere"
    # each file's bbox is its own source's, in the centered frame
    for m, src, c in ((cyl_m, cyl_src, cyl_c), (sph_m, sph_src, sph_c)):
        assert m["bbox_min"] == pytest.approx(list(src.bounds[0] - c), abs=1e-3)
        assert m["bbox_max"] == pytest.approx(list(src.bounds[1] - c), abs=1e-3)
    assert cyl_m["bbox_max"] != pytest.approx(sph_m["bbox_max"], abs=1e-3)


def test_emit_offers_no_way_to_build_on_an_existing_file():
    """No template/base/donor parameter, and no call into the clone-and-patch
    writer. If someone reintroduces ``write_replace`` at this boundary -- the
    one path that can staple supports onto a foreign model -- this fails."""
    import ast
    import inspect
    params = set(inspect.signature(emit.emit_project).parameters)
    assert not (params & {"template", "base", "donor", "clone_from", "base_path"})
    with pytest.raises(TypeError):
        emit.emit_project("a.stl", {}, "b.chitubox", None, template=b"")

    # Real calls only -- the module docstring names write_replace on purpose,
    # to explain the failure mode it removes.
    tree = ast.parse(inspect.getsource(emit))
    called = set()
    for node in ast.walk(tree):
        if isinstance(node, ast.Call):
            fn = node.func
            called.add(fn.attr if isinstance(fn, ast.Attribute) else
                       getattr(fn, "id", ""))
    assert not (called & {"write_replace", "write_supports", "clone"})


# ---------------------------------------------------------------------------
# placement + naming
# ---------------------------------------------------------------------------

@pytest.mark.chitubox
def test_placement_xy_reaches_the_file(tmp_path):
    """Plate translation is not silently dropped: (x, y, lift) round-trips.
    Only (0, 0) was ever exercised before, which cannot distinguish "passed
    through" from "hardcoded to the plate centre"."""
    import chitubox_io
    cfg = config.SupportConfig(lift_mm=3.0)
    _, zmin_centered, plate_z = _cylinder_frame(cfg)
    result = dict(members=[_mk_member([0, 0, zmin_centered], [0, 0, plate_z],
                                      0.5, 0.5, "trunk", ("contact", 0))],
                  balls=[], plate_z=plate_z, verify=dict(ok=True))
    out = tmp_path / "placed.chitubox"
    rep = emit.emit_project(CYLINDER_STL, result, str(out), cfg,
                            placement_xy=(12.5, -7.25))
    assert rep.placement_xy == (12.5, -7.25)
    m = chitubox_io.read_project(str(out))["models"][0]
    assert m["placement"] == pytest.approx((12.5, -7.25, cfg.lift_mm))


@pytest.mark.chitubox
def test_default_model_name_is_the_stl_stem_capped_at_63(tmp_path):
    """The container's name field is 63 bytes. A long mini filename must be
    truncated on the way in, not overrun the field or be rejected -- and the
    name read back out of the finished file is what is checked."""
    long_stem = "Morel_Mudskipper_" + "x" * 80
    src = tmp_path / (long_stem + ".stl")
    src.write_bytes(open(CYLINDER_STL, "rb").read())
    cfg = config.SupportConfig(lift_mm=3.0)
    _, zmin_centered, plate_z = _cylinder_frame(cfg)
    result = dict(members=[_mk_member([0, 0, zmin_centered], [0, 0, plate_z],
                                      0.5, 0.5, "trunk", ("contact", 0))],
                  balls=[], plate_z=plate_z, verify=dict(ok=True))
    rep = emit.emit_project(str(src), result, str(tmp_path / "n.chitubox"), cfg)
    assert rep.model_name == long_stem[:63]
    assert len(rep.model_name) == 63
