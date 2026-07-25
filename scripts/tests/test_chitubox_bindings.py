"""The chitubox_io PyO3 boundary: container writes from an EMPTY template.

Marked ``chitubox`` -- conftest FAILS (never skips) these when the extension
is missing, stale, or API-mismatched, because a stale binding is exactly how a
wrong-model emit would sneak past a green suite.
"""
import os
import struct

import numpy as np
import pytest
import trimesh

pytestmark = pytest.mark.chitubox

FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures", "chitubox")
CYLINDER_STL = os.path.join(FIXTURES, "Cylinder.stl")


@pytest.fixture
def cx():
    import chitubox_io
    return chitubox_io


def _box_triangles(extents=(10.0, 10.0, 10.0)):
    b = trimesh.creation.box(extents=extents)
    return np.ascontiguousarray(np.asarray(b.triangles, dtype=np.float32))


def _one_record(mesh, seg_type=9, start=(0, 0, -5), end=(0, 0, 5), radii=(0.5, 0.5)):
    flat = np.ascontiguousarray(mesh, dtype=np.float32).reshape(-1)
    return {
        "seg_type": np.array([seg_type], np.uint32),
        "start": np.array([start], np.float32),
        "end": np.array([end], np.float32),
        "radii": np.array([radii], np.float32),
        "trailing": np.zeros((1, 24), np.uint8),
        "mesh_flat": flat,
        "mesh_len": np.array([flat.size], np.uint32),
    }


def _stl_face_count(path):
    with open(path, "rb") as fh:
        fh.seek(80)
        return struct.unpack("<I", fh.read(4))[0]


# ---------------------------------------------------------------------------
# surface
# ---------------------------------------------------------------------------

def test_extension_exposes_version_and_digest(cx):
    from support_placement import emit
    assert cx.API_VERSION == emit.EXPECTED_CHITUBOX_API_VERSION
    assert isinstance(cx.SOURCE_DIGEST, str) and len(cx.SOURCE_DIGEST) == 64


def test_no_template_parameter_anywhere(cx, tmp_path):
    """The wrong-model failure mode is removed STRUCTURALLY: there is no way
    to ask this surface to build on top of an existing file. Every project
    starts from the crate's embedded empty container."""
    with pytest.raises(TypeError):
        cx.write_project(str(tmp_path / "x.chitubox"), [], template=b"")
    with pytest.raises(TypeError):
        cx.write_project(str(tmp_path / "x.chitubox"), [], base=b"")
    sig = cx.write_project.__text_signature__
    assert sig is not None
    assert [p.strip() for p in sig.strip("()").split(",")] == ["out_path", "models"]


def test_write_then_read_round_trips_one_record(cx, tmp_path):
    tris = _box_triangles()
    seg = tris[:4].copy()
    out = str(tmp_path / "box.chitubox")
    wrote = cx.write_project(out, [{
        "name": "box", "triangles": tris, "placement": (1.0, 2.0, 3.0),
        "supports": _one_record(seg),
    }])
    assert os.path.isfile(out)
    assert wrote["model_count"] == 1
    assert wrote["support_counts"] == [1]
    assert wrote["reparse_ok"] is True
    assert wrote["bytes_written"] == os.path.getsize(out)

    back = cx.read_project(out)
    assert back["model_count"] == 1
    m = back["models"][0]
    assert m["name"] == "box"
    assert m["triangle_count"] == len(tris)
    assert m["support_record_count"] == 1
    r = m["supports"][0]
    assert r["seg_type"] == 9
    assert r["start"] == pytest.approx([0.0, 0.0, -5.0])
    assert r["end"] == pytest.approx([0.0, 0.0, 5.0])
    assert r["radii"] == pytest.approx((0.5, 0.5))
    assert r["triangle_count"] == len(seg)


def test_lift_mm_round_trips_into_placement_z(cx, tmp_path):
    tris = _box_triangles()
    out = str(tmp_path / "lift.chitubox")
    cx.write_project(out, [{
        "name": "m", "triangles": tris, "placement": (0.0, 0.0, 3.0),
        "supports": _one_record(tris[:2].copy()),
    }])
    assert cx.read_project(out)["models"][0]["placement"] == pytest.approx((0.0, 0.0, 3.0))


def test_model_from_stl_path_matches_the_stl(cx, tmp_path):
    out = str(tmp_path / "cyl.chitubox")
    cx.write_project(out, [{
        "name": "cyl", "stl_path": CYLINDER_STL, "placement": (0.0, 0.0, 0.0),
    }])
    m = cx.read_project(out)["models"][0]
    assert m["triangle_count"] == _stl_face_count(CYLINDER_STL)
    assert m["has_supports"] is False


def test_multi_record_batch_slices_mesh_flat_by_prefix_sum(cx, tmp_path):
    """Records of DIFFERENT triangle counts must come back with their own
    geometry -- the prefix-sum slicing of the single concatenated buffer is
    the whole batch-first design, so mis-slicing must be visible."""
    tris = _box_triangles()
    a, b, c = tris[:2], tris[:5], tris[:12]
    meshes = [np.ascontiguousarray(x, np.float32).reshape(-1) for x in (a, b, c)]
    sup = {
        "seg_type": np.array([7, 1, 9], np.uint32),
        "start": np.array([[0, 0, 0], [1, 0, 0], [2, 0, 0]], np.float32),
        "end": np.array([[0, 0, 1], [1, 0, 1], [2, 0, 1]], np.float32),
        "radii": np.array([[0.1, 0.2], [0.2, 0.3], [0.3, 0.4]], np.float32),
        "trailing": np.zeros((3, 24), np.uint8),
        "mesh_flat": np.concatenate(meshes),
        "mesh_len": np.array([m.size for m in meshes], np.uint32),
    }
    out = str(tmp_path / "multi.chitubox")
    cx.write_project(out, [{"name": "m", "triangles": tris,
                            "placement": (0.0, 0.0, 3.0), "supports": sup}])
    recs = cx.read_project(out)["models"][0]["supports"]
    assert [r["seg_type"] for r in recs] == [7, 1, 9]
    assert [r["triangle_count"] for r in recs] == [2, 5, 12]


# ---------------------------------------------------------------------------
# the empty-start regression (task #60)
# ---------------------------------------------------------------------------

def test_two_writes_from_one_binding_carry_their_own_models(cx, tmp_path):
    """THE regression this binding exists for.

    ``chitubox_inject.write_replace`` copies the template's byte range through
    the end of the mesh region, so every file it emits carries the TEMPLATE's
    model. Here two files written back-to-back from the same process must each
    contain ITS OWN model -- a property that path cannot satisfy.
    """
    small = _box_triangles((4.0, 4.0, 4.0))
    big = _box_triangles((40.0, 40.0, 40.0))
    p_small = str(tmp_path / "small.chitubox")
    p_big = str(tmp_path / "big.chitubox")
    cx.write_project(p_small, [{"name": "s", "triangles": small,
                                "placement": (0.0, 0.0, 0.0)}])
    cx.write_project(p_big, [{"name": "b", "triangles": big,
                              "placement": (0.0, 0.0, 0.0)}])

    ms = cx.read_project(p_small)["models"][0]
    mb = cx.read_project(p_big)["models"][0]
    assert ms["triangle_count"] == len(small)
    assert mb["triangle_count"] == len(big)
    assert ms["bbox_max"] == pytest.approx([2.0, 2.0, 2.0])
    assert mb["bbox_max"] == pytest.approx([20.0, 20.0, 20.0])
    assert ms["bbox_max"] != mb["bbox_max"]


def test_stl_model_is_not_polluted_by_a_previous_write(cx, tmp_path):
    """Same property across the STL path: writing a cylinder AFTER a box must
    still yield the cylinder's own triangle count."""
    box = _box_triangles()
    cx.write_project(str(tmp_path / "first.chitubox"),
                     [{"name": "box", "triangles": box, "placement": (0.0, 0.0, 0.0)}])
    out = str(tmp_path / "second.chitubox")
    cx.write_project(out, [{"name": "cyl", "stl_path": CYLINDER_STL,
                            "placement": (0.0, 0.0, 0.0)}])
    assert cx.read_project(out)["models"][0]["triangle_count"] == _stl_face_count(CYLINDER_STL)


# ---------------------------------------------------------------------------
# error mapping
# ---------------------------------------------------------------------------

def test_non_finite_radius_maps_to_NonFiniteValue(cx, tmp_path):
    tris = _box_triangles()
    sup = _one_record(tris[:2].copy())
    sup["radii"] = np.array([[np.inf, 0.5]], np.float32)
    with pytest.raises(cx.ChituboxError) as e:
        cx.write_project(str(tmp_path / "bad.chitubox"),
                         [{"name": "m", "triangles": tris,
                           "placement": (0.0, 0.0, 3.0), "supports": sup}])
    assert e.value.kind == "NonFiniteValue"


def test_empty_record_mesh_maps_to_BadTriangleBuffer(cx, tmp_path):
    tris = _box_triangles()
    sup = _one_record(tris[:2].copy())
    sup["mesh_flat"] = np.zeros(0, np.float32)
    sup["mesh_len"] = np.array([0], np.uint32)
    with pytest.raises(cx.ChituboxError) as e:
        cx.write_project(str(tmp_path / "bad.chitubox"),
                         [{"name": "m", "triangles": tris,
                           "placement": (0.0, 0.0, 3.0), "supports": sup}])
    assert e.value.kind == "BadTriangleBuffer"


def test_ascii_stl_maps_to_AsciiStlUnsupported(cx, tmp_path):
    ascii_stl = tmp_path / "ascii.stl"
    ascii_stl.write_text(
        "solid t\nfacet normal 0 0 1\nouter loop\n"
        "vertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\n"
        "endloop\nendfacet\nendsolid t\n")
    with pytest.raises(cx.ChituboxError) as e:
        cx.write_project(str(tmp_path / "out.chitubox"),
                         [{"name": "m", "stl_path": str(ascii_stl),
                           "placement": (0.0, 0.0, 0.0)}])
    assert e.value.kind == "AsciiStlUnsupported"


def test_unwritable_directory_maps_to_Io(cx, tmp_path):
    with pytest.raises(cx.ChituboxError) as e:
        cx.write_project(str(tmp_path / "no" / "such" / "dir" / "o.chitubox"),
                         [{"name": "m", "triangles": _box_triangles(),
                           "placement": (0.0, 0.0, 0.0)}])
    assert e.value.kind == "Io"


def test_reading_a_non_chitubox_file_raises_typed(cx, tmp_path):
    junk = tmp_path / "junk.chitubox"
    junk.write_bytes(b"\x00" * 4096)
    with pytest.raises(cx.ChituboxError) as e:
        cx.read_project(str(junk))
    assert e.value.kind == "NotChitubox"


# ---------------------------------------------------------------------------
# caller-input validation (ValueError, not ChituboxError: these are Python-side
# shape bugs, not container-format problems)
# ---------------------------------------------------------------------------

def test_mesh_flat_shorter_than_mesh_len_is_rejected(cx, tmp_path):
    tris = _box_triangles()
    sup = _one_record(tris[:2].copy())
    sup["mesh_len"] = np.array([sup["mesh_flat"].size + 9], np.uint32)
    with pytest.raises(ValueError, match="too short"):
        cx.write_project(str(tmp_path / "o.chitubox"),
                         [{"name": "m", "triangles": tris,
                           "placement": (0.0, 0.0, 3.0), "supports": sup}])


def test_mesh_flat_longer_than_mesh_len_is_rejected(cx, tmp_path):
    tris = _box_triangles()
    sup = _one_record(tris[:2].copy())
    sup["mesh_len"] = np.array([9], np.uint32)
    with pytest.raises(ValueError, match="accounts for"):
        cx.write_project(str(tmp_path / "o.chitubox"),
                         [{"name": "m", "triangles": tris,
                           "placement": (0.0, 0.0, 3.0), "supports": sup}])


def test_both_stl_path_and_triangles_is_rejected(cx, tmp_path):
    with pytest.raises(ValueError, match="exactly one"):
        cx.write_project(str(tmp_path / "o.chitubox"),
                         [{"name": "m", "triangles": _box_triangles(),
                           "stl_path": CYLINDER_STL, "placement": (0.0, 0.0, 0.0)}])


def test_empty_model_list_is_rejected(cx, tmp_path):
    with pytest.raises(ValueError, match="at least one model"):
        cx.write_project(str(tmp_path / "o.chitubox"), [])
