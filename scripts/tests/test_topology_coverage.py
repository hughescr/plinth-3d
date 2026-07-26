import dataclasses
import json
import pickle
import subprocess
import sys
import textwrap

import numpy as np
import pytest
import trimesh

from support_placement import coverage, perception, topology, verify


_RESOURCE_PROBE = r"""
import json
import resource
import sys

import numpy as np
import trimesh

sys.path.insert(0, "scripts")
from support_placement import coverage, topology
from support_placement.config import SupportConfig


def rss_bytes():
    value = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(value if sys.platform == "darwin" else value * 1024)


nx, ny = 601, 481
grid = np.arange(nx * ny, dtype=np.int64).reshape((ny, nx))
v00 = grid[:-1, :-1].reshape(-1)
v01 = grid[:-1, 1:].reshape(-1)
v10 = grid[1:, :-1].reshape(-1)
v11 = grid[1:, 1:].reshape(-1)
faces = np.empty((2 * len(v00), 3), dtype=np.int64)
faces[0::2] = np.column_stack((v00, v01, v11))
faces[1::2] = np.column_stack((v00, v11, v10))
x, y = np.meshgrid(
    np.arange(nx, dtype=np.float64) * 0.1,
    np.arange(ny, dtype=np.float64) * 0.1)
vertices = np.column_stack((
    x.reshape(-1), y.reshape(-1), np.zeros(nx * ny)))
mesh = trimesh.Trimesh(vertices=vertices, faces=faces, process=False)
cfg = SupportConfig()
mode = sys.argv[1]
stats = {}
base = rss_bytes()
if mode == "topology":
    prepared = topology.build_face_topology(mesh, cfg=cfg, stats_out=stats)
    retained = prepared.compact_retained_bytes
else:
    islands = [np.arange(len(faces), dtype=np.int64)]
    prepared = coverage.build_coverage_samples(
        mesh, islands, cfg, stats_out=stats)
    retained = prepared.compact_retained_bytes
peak = rss_bytes()
print("RESOURCE_JSON=" + json.dumps({
    "mode": mode,
    "faces": len(faces),
    "retained": retained,
    "planned_build": stats["planned_peak_build_bytes"],
    "planned_validation": stats["planned_peak_validation_bytes"],
    "max_peak": stats["max_peak_bytes"],
    "rss_base": base,
    "rss_peak": peak,
    "rss_delta": max(0, peak - base),
    "reported_build_rss_delta":
        stats["measured_build_rss_high_water_delta_bytes"],
    "reported_total_rss_delta":
        stats["measured_total_rss_high_water_delta_bytes"],
}, sort_keys=True))
"""


def _run_resource_probe(mode):
    completed = subprocess.run(
        [sys.executable, "-c", _RESOURCE_PROBE, mode],
        check=True, capture_output=True, text=True, timeout=180)
    line = next(
        row for row in completed.stdout.splitlines()
        if row.startswith("RESOURCE_JSON="))
    return json.loads(line.removeprefix("RESOURCE_JSON="))


def _soup(triangles):
    vertices = np.asarray(triangles, dtype=np.float64).reshape((-1, 3))
    faces = np.arange(len(vertices), dtype=np.int64).reshape((-1, 3))
    return trimesh.Trimesh(vertices=vertices, faces=faces, process=False)


def _two_edge_faces(delta=0.0, same_direction=False):
    a = [0.0, 0.0, 0.0]
    b = [1.0, 0.0, 0.0]
    c = [0.0, 1.0, 0.0]
    a2 = [delta, 0.0, 0.0]
    b2 = [1.0 + delta, 0.0, 0.0]
    d = [0.0, -1.0, 0.0]
    second = [a2, b2, d] if same_direction else [b2, a2, d]
    return _soup([[a, b, c], second])


def _manual_topology(mesh, pairs=(), cut_flags=None):
    n_faces = len(mesh.faces)
    pairs = np.asarray(list(pairs), dtype=np.int64).reshape((-1, 2))
    if len(pairs):
        row = np.r_[pairs[:, 0], pairs[:, 1]]
        col = np.r_[pairs[:, 1], pairs[:, 0]]
        order = np.lexsort((col, row))
        row, col = row[order], col[order]
        indptr = np.r_[0, np.cumsum(np.bincount(row, minlength=n_faces))]
        graph = [[] for _ in range(n_faces)]
        for a, b in pairs:
            graph[int(a)].append(int(b))
            graph[int(b)].append(int(a))
        components = np.full(n_faces, -1, dtype=np.int64)
        component = 0
        for face in range(n_faces):
            if components[face] >= 0:
                continue
            stack = [face]
            components[face] = component
            while stack:
                cur = stack.pop()
                for neighbor in graph[cur]:
                    if components[neighbor] < 0:
                        components[neighbor] = component
                        stack.append(neighbor)
            component += 1
    else:
        indptr = np.zeros(n_faces + 1, dtype=np.int64)
        col = np.empty(0, dtype=np.int64)
        components = np.arange(n_faces, dtype=np.int64)
    flags = np.zeros(n_faces, np.uint8) if cut_flags is None else cut_flags
    sidecar = topology.FaceTopology(
        schema=topology.FACE_TOPOLOGY_SCHEMA,
        version=topology.FACE_TOPOLOGY_SCHEMA_VERSION,
        indptr=topology._immutable_array(indptr, np.int64),
        indices=topology._immutable_array(col, np.int64),
        component_ids=topology._immutable_array(components, np.int64),
        cut_flags=topology._immutable_array(flags, np.uint8),
        n_faces=n_faces, n_vertices=len(mesh.vertices),
        n_directed_edges=len(col),
        compact_retained_bytes=topology._topology_retained_bytes(
            n_faces, len(col)),
        planned_peak_build_bytes=topology._planned_peak_build_bytes(
            n_faces, len(mesh.vertices)),
        planned_peak_validation_bytes=topology._planned_peak_validation_bytes(
            n_faces, len(mesh.vertices), len(col)),
        cut_counts=tuple(
            int(x) for x in np.bincount(flags, minlength=64)),
        mesh_fingerprint=topology.mesh_fingerprint(mesh),
        content_digest="")
    object.__setattr__(
        sidecar, "content_digest", topology._topology_digest(sidecar))
    return topology.validate_face_topology(sidecar, mesh=mesh)


def _manual_samples(mesh, island_ids=None):
    n = len(mesh.faces)
    iids = np.zeros(n, np.int64) if island_ids is None else np.asarray(
        island_ids, dtype=np.int64)
    order = np.lexsort((
        mesh.triangles_center[:, 2], mesh.triangles_center[:, 1],
        mesh.triangles_center[:, 0], np.arange(n), iids))
    face_ids = np.arange(n, dtype=np.int64)[order]
    iids = iids[order]
    positions = np.asarray(mesh.triangles_center, np.float64)[order]
    weights = np.asarray(mesh.area_faces, np.float64)[order]
    n_islands = int(iids.max(initial=-1)) + 1
    island_offsets = np.r_[
        0, np.cumsum(np.bincount(iids, minlength=n_islands))]
    source_face_offsets = np.arange(n + 1, dtype=np.int64)
    islands = [
        face_ids[iids == iid] for iid in range(n_islands)]
    samples = coverage.CoverageSamples(
        schema=coverage.COVERAGE_SAMPLES_SCHEMA,
        version=coverage.COVERAGE_SAMPLES_SCHEMA_VERSION,
        positions=coverage._immutable_array(positions, np.float64),
        area_weights_mm2=coverage._immutable_array(weights, np.float64),
        face_ids=coverage._immutable_array(face_ids, np.int64),
        island_ids=coverage._immutable_array(iids, np.int64),
        refine_depths=coverage._immutable_array(np.zeros(n), np.uint8),
        island_offsets=coverage._immutable_array(
            island_offsets, np.int64),
        source_face_offsets=coverage._immutable_array(
            source_face_offsets, np.int64),
        source_face_ids=coverage._immutable_array(face_ids, np.int64),
        source_face_island_ids=coverage._immutable_array(iids, np.int64),
        n_source_faces=n, n_virtual_cells_planned=n, max_refine_depth=0,
        n_islands=n_islands, n_mesh_faces=len(mesh.faces),
        n_mesh_vertices=len(mesh.vertices), max_working_cells=65_536,
        compact_retained_bytes=coverage._coverage_retained_bytes(
            n, n, n_islands),
        planned_peak_build_bytes=coverage._planned_peak_build_bytes(
            n, n, min(n, 65_536), n_islands,
            len(mesh.faces), len(mesh.vertices)),
        planned_peak_validation_bytes=coverage._planned_peak_validation_bytes(
            n, n, n_islands, len(mesh.faces), len(mesh.vertices)),
        mesh_fingerprint=topology.mesh_fingerprint(mesh),
        islands_fingerprint=coverage.islands_fingerprint(islands),
        content_digest="")
    object.__setattr__(
        samples, "content_digest", coverage._coverage_digest(samples))
    return coverage.validate_coverage_samples(
        samples, mesh=mesh, islands=islands)


def test_exact_bit_topology_connects_opposite_duplicate_vertex_rows():
    mesh = _two_edge_faces()
    assert len(mesh.face_adjacency) == 0
    sidecar = topology.build_face_topology(mesh)
    assert sidecar.neighbors(0).tolist() == [1]
    assert sidecar.neighbors(1).tolist() == [0]
    assert sidecar.component_ids.tolist() == [0, 0]


def test_exact_bit_topology_cuts_seam_point_same_direction_duplicate_and_nonmanifold():
    seam = topology.build_face_topology(_two_edge_faces(delta=4e-9))
    assert len(seam.indices) == 0

    point = topology.build_face_topology(_soup([
        [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
        [[0, 0, 0], [-1, 0, 0], [0, -1, 0]],
    ]))
    assert np.all(
        point.cut_flags & topology.TOPOLOGY_CUT_POINT_ONLY)

    same = topology.build_face_topology(
        _two_edge_faces(same_direction=True))
    assert len(same.indices) == 0
    assert np.all(
        same.cut_flags & topology.TOPOLOGY_CUT_SAME_DIRECTION)

    duplicate = topology.build_face_topology(_soup([
        [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
        [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
    ]))
    assert np.all(
        duplicate.cut_flags & topology.TOPOLOGY_CUT_DUPLICATE)

    nonmanifold = topology.build_face_topology(_soup([
        [[0, 0, 0], [1, 0, 0], [0, 1, 0]],
        [[1, 0, 0], [0, 0, 0], [0, -1, 0]],
        [[0, 0, 0], [1, 0, 0], [0, 0, 1]],
    ]))
    assert len(nonmanifold.indices) == 0
    assert np.all(
        nonmanifold.cut_flags & topology.TOPOLOGY_CUT_NONMANIFOLD)


def test_point_only_cut_uses_local_vertex_fan_not_global_face_component():
    # The four faces form one valid-edge chain globally: 0--1--2--3.
    # Faces 0 and 3 also share v, but neither chain edge incident to v joins
    # them there, so v has two local fan components and both endpoint faces
    # must be cut as a point-only junction.
    v = [0, 0, 0]
    a = [1, 0, 0]
    b = [1, 1, 0]
    c = [2, 1, 0]
    d = [2, 0, 0]
    mesh = _soup([
        [v, a, b],
        [b, a, c],
        [b, c, d],
        [d, c, v],
    ])

    sidecar = topology.build_face_topology(mesh)

    assert sidecar.component_ids.tolist() == [0, 0, 0, 0]
    assert [sidecar.neighbors(i).tolist() for i in range(4)] == [
        [1], [0, 2], [1, 3], [2]]
    point_only = (
        sidecar.cut_flags & topology.TOPOLOGY_CUT_POINT_ONLY) != 0
    assert point_only.tolist() == [True, False, False, True]


def test_topology_sidecar_pickle_immutability_and_no_mesh_mutation():
    mesh = _two_edge_faces()
    vertices = mesh.vertices.copy()
    faces = mesh.faces.copy()
    sidecar = topology.build_face_topology(mesh)
    restored = pickle.loads(pickle.dumps(sidecar))
    topology.validate_face_topology(restored, mesh=mesh)
    assert np.array_equal(mesh.vertices, vertices)
    assert np.array_equal(mesh.faces, faces)
    for name in ("indptr", "indices", "component_ids", "cut_flags"):
        arr = getattr(restored, name)
        assert not arr.flags.writeable
        with pytest.raises(ValueError):
            arr.setflags(write=True)


def test_topology_sidecar_deep_validation_rejects_array_and_counter_corruption():
    mesh = _two_edge_faces()
    sidecar = topology.build_face_topology(mesh)
    broken_indices = dataclasses.replace(
        sidecar,
        indices=topology._immutable_array(np.array([1]), np.int64))
    with pytest.raises(ValueError):
        topology.validate_face_topology(broken_indices, mesh=mesh)
    broken_counter = dataclasses.replace(
        sidecar, n_directed_edges=sidecar.n_directed_edges + 1)
    with pytest.raises(ValueError):
        topology.validate_face_topology(broken_counter, mesh=mesh)
    broken_digest = dataclasses.replace(sidecar, content_digest="0" * 64)
    with pytest.raises(ValueError, match="content digest"):
        topology.validate_face_topology(broken_digest, mesh=mesh)


def test_topology_empty_mesh_and_large_refusal_before_edge_allocation(
        cfg, monkeypatch):
    empty = trimesh.Trimesh(
        vertices=np.empty((0, 3)), faces=np.empty((0, 3), dtype=np.int64),
        process=False)
    sidecar = topology.build_face_topology(empty, cfg=cfg)
    assert sidecar.n_faces == 0
    assert sidecar.indptr.tolist() == [0]

    class LargeMesh:
        vertices = np.zeros((3, 3), dtype=np.float64)
        faces = np.zeros((576_000, 3), dtype=np.int64)

    tight = dataclasses.replace(cfg, topology_max_faces=575_999)

    def _no_unique(*_args, **_kwargs):
        raise AssertionError("edge/vertex canonicalization began before refusal")

    monkeypatch.setattr(topology.np, "unique", _no_unique)
    stats = {}
    with pytest.raises(ValueError, match="topology_max_faces"):
        topology.build_face_topology(LargeMesh(), cfg=tight, stats_out=stats)
    assert stats["n_faces"] == 576_000
    assert stats["preflight_passed"] is False


def test_topology_576k_peak_refuses_before_canonicalization(cfg, monkeypatch):
    n_faces = 576_000

    class LargeMesh:
        vertices = np.zeros((288_001, 3), dtype=np.float64)
        faces = np.zeros((n_faces, 3), dtype=np.int64)

    planned = topology._planned_peak_build_bytes(
        n_faces, len(LargeMesh.vertices))
    tight = dataclasses.replace(
        cfg, topology_max_faces=n_faces,
        topology_max_peak_bytes=planned - 1)

    def _no_unique(*_args, **_kwargs):
        raise AssertionError("canonicalization began before peak refusal")

    monkeypatch.setattr(topology.np, "unique", _no_unique)
    stats = {}
    with pytest.raises(ValueError, match=r"topology_max_peak_bytes\(build\)"):
        topology.build_face_topology(LargeMesh(), cfg=tight, stats_out=stats)
    assert stats["planned_peak_build_bytes"] == planned
    assert stats["planned_peak_validation_bytes_upper_bound"] < planned
    assert stats["preflight_passed"] is False


def test_topology_exact_retained_preflight_refuses_before_canonicalization(
        cfg, monkeypatch):
    n_faces = 576_000

    class LargeMesh:
        vertices = np.zeros((288_001, 3), dtype=np.float64)
        faces = np.zeros((n_faces, 3), dtype=np.int64)

    compact_upper = topology._topology_retained_bytes(n_faces, 3 * n_faces)
    tight = dataclasses.replace(
        cfg, topology_max_compact_payload_bytes=compact_upper - 1)

    def _no_unique(*_args, **_kwargs):
        raise AssertionError("canonicalization began before compact refusal")

    monkeypatch.setattr(topology.np, "unique", _no_unique)
    stats = {}
    with pytest.raises(
            ValueError, match="topology_max_compact_payload_bytes"):
        topology.build_face_topology(LargeMesh(), cfg=tight, stats_out=stats)
    assert stats["compact_retained_bytes_upper_bound"] == compact_upper
    assert stats["preflight_passed"] is False


def test_overhang_islands_uses_exact_topology_when_native_adjacency_empty(
        cfg, monkeypatch):
    mesh = _two_edge_faces()
    monkeypatch.setattr(
        perception, "down_facing_mask",
        lambda _mesh, _cfg: np.ones(2, dtype=bool))
    monkeypatch.setattr(
        perception, "grounded_mask",
        lambda _mesh, _cfg, _plate_z, _down: np.zeros(2, dtype=bool))
    local_cfg = dataclasses.replace(cfg, island_merge_radius_mm=0.0)
    islands = perception.overhang_islands(mesh, local_cfg, plate_z=-10.0)
    assert len(mesh.face_adjacency) == 0
    assert len(islands) == 1
    assert islands[0].tolist() == [0, 1]


def test_near_disconnected_sheets_do_not_route_coverage_across_component(cfg):
    mesh = _soup([
        [[0, 0, 0], [0.3, 0, 0], [0, 0.2, 0]],
        [[0, 0, 0.01], [0.3, 0, 0.01], [0, 0.2, 0.01]],
    ])
    islands = [np.array([0, 1])]
    sidecar = _manual_topology(mesh)
    samples = _manual_samples(mesh)
    contact = dict(
        position=mesh.triangles_center[0].copy(), island_id=0, face_id=0)
    assessments = []
    report = verify.coverage_report(
        mesh, islands, [contact], cfg, coverage_samples=samples,
        face_topology=sidecar, assessment_out=assessments)
    assert assessments[0].covered_mask.tolist() == [True, False]
    assert all(not any(key.startswith("_") for key in row) for row in report)


@pytest.mark.parametrize("area,expected", [(0.03, True), (0.17, False)])
def test_patch_area_floor_is_absolute_not_fractional(cfg, area, expected):
    mesh = _soup([[[0, 0, 0], [1, 0, 0], [0, 2 * area, 0]]])
    report = verify.coverage_report(
        mesh, [np.array([0])], [], cfg,
        coverage_samples=_manual_samples(mesh),
        face_topology=_manual_topology(mesh))
    assert report[0]["covered"] is expected
    assert report[0]["covered_fraction"] == 0.0


def test_separate_small_patches_pass_but_connected_sum_fails(cfg):
    mesh = _soup([
        [[0, 0, 0], [1, 0, 0], [0, 0.18, 0]],
        [[2, 0, 0], [3, 0, 0], [2, 0.18, 0]],
    ])
    samples = _manual_samples(mesh)
    separate = verify.coverage_report(
        mesh, [np.array([0, 1])], [], cfg, coverage_samples=samples,
        face_topology=_manual_topology(mesh))
    connected = verify.coverage_report(
        mesh, [np.array([0, 1])], [], cfg, coverage_samples=samples,
        face_topology=_manual_topology(mesh, [(0, 1)]))
    assert separate[0]["covered"] is True
    assert separate[0]["n_patches"] == 2
    assert connected[0]["covered"] is False
    assert connected[0]["max_patch_area_mm2"] == pytest.approx(0.18)


def test_coverage_radius_boundary_is_inclusive_and_nextafter_fails(cfg):
    mesh = _soup([[[0, 0, 0], [1, 0, 0], [0, 0.34, 0]]])
    samples = _manual_samples(mesh)
    center = samples.positions[0]
    radius = cfg.target_spacing_mm * cfg.coverage_radius_factor
    contact = dict(position=center + [radius, 0, 0], island_id=0, face_id=0)
    exact_assessment = []
    exact = verify.coverage_report(
        mesh, [np.array([0])], [contact], cfg, coverage_samples=samples,
        face_topology=_manual_topology(mesh),
        assessment_out=exact_assessment)
    contact["position"] = center + [np.nextafter(radius, np.inf), 0, 0]
    outside_assessment = []
    outside = verify.coverage_report(
        mesh, [np.array([0])], [contact], cfg, coverage_samples=samples,
        face_topology=_manual_topology(mesh),
        assessment_out=outside_assessment)
    assert exact_assessment[0].covered_mask.tolist() == [True]
    assert outside_assessment[0].covered_mask.tolist() == [False]
    assert outside[0]["covered"] is False


def test_coverage_samples_dyadic_weighted_equivalence(cfg):
    coarse = _soup([[[0, 0, 0], [1.6, 0, 0], [0, 1.0, 0]]])
    refined = coarse.subdivide()
    a = coverage.build_coverage_samples(coarse, [np.array([0])], cfg)
    b = coverage.build_coverage_samples(
        refined, [np.arange(len(refined.faces))], cfg)
    assert len(a) == len(b)
    assert np.array_equal(
        np.sort(a.area_weights_mm2), np.sort(b.area_weights_mm2))
    assert np.array_equal(
        np.array(sorted(map(tuple, a.positions))),
        np.array(sorted(map(tuple, b.positions))))


def test_coverage_samples_preflight_refuses_before_generation(
        cfg, monkeypatch):
    mesh = _soup([[[0, 0, 0], [1, 0, 0], [0, 1, 0]]])
    tight = dataclasses.replace(
        cfg, coverage_samples_max_virtual_cells=0)

    def _no_generation(*_args, **_kwargs):
        raise AssertionError("generation began before global refusal")

    monkeypatch.setattr(coverage.np, "concatenate", _no_generation)
    stats = {}
    with pytest.raises(
            ValueError, match="coverage_samples_max_virtual_cells"):
        coverage.build_coverage_samples(
            mesh, [np.array([0])], tight, stats_out=stats)
    assert stats["preflight_passed"] is False


def test_coverage_samples_pickle_and_bounded_query_instrumentation(cfg):
    mesh = _soup([[[0, 0, 0], [1.6, 0, 0], [0, 1.0, 0]]])
    local_cfg = dataclasses.replace(cfg, coverage_query_block_size=3)
    stats = {}
    samples = coverage.build_coverage_samples(
        mesh, [np.array([0])], local_cfg, stats_out=stats)
    restored = pickle.loads(pickle.dumps(samples))
    coverage.validate_coverage_samples(
        restored, mesh=mesh, islands=[np.array([0])])
    contact = dict(
        position=restored.positions[0], island_id=0, face_id=0)
    report = verify.coverage_report(
        mesh, [np.array([0])], [contact], local_cfg,
        coverage_samples=restored, face_topology=_manual_topology(mesh))
    assert report[0]["max_query_block_rows"] <= 3
    assert stats["max_working_cells_seen"] <= (
        local_cfg.coverage_samples_max_working_cells)


def test_coverage_samples_deep_validation_rejects_array_and_counter_corruption(
        cfg):
    mesh = _soup([[[0, 0, 0], [1.6, 0, 0], [0, 1.0, 0]]])
    islands = [np.array([0])]
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    positions = np.array(samples.positions, copy=True)
    positions[0, 0] = np.nextafter(positions[0, 0], np.inf)
    broken_positions = dataclasses.replace(
        samples,
        positions=coverage._immutable_array(positions, np.float64))
    with pytest.raises(ValueError):
        coverage.validate_coverage_samples(
            broken_positions, mesh=mesh, islands=islands)
    broken_counter = dataclasses.replace(
        samples, n_virtual_cells_planned=len(samples) + 1)
    with pytest.raises(ValueError, match="counters"):
        coverage.validate_coverage_samples(
            broken_counter, mesh=mesh, islands=islands)
    broken_digest = dataclasses.replace(samples, content_digest="0" * 64)
    with pytest.raises(ValueError, match="content digest"):
        coverage.validate_coverage_samples(
            broken_digest, mesh=mesh, islands=islands)


def test_coverage_samples_peak_preflight_refuses_before_generation(
        cfg, monkeypatch):
    mesh = _soup([[[0, 0, 0], [1, 0, 0], [0, 1, 0]]])
    tight = dataclasses.replace(
        cfg, coverage_samples_max_peak_bytes=0)

    def _no_generation(*_args, **_kwargs):
        raise AssertionError("generation began before peak-plan refusal")

    monkeypatch.setattr(coverage.np, "concatenate", _no_generation)
    stats = {}
    with pytest.raises(
            ValueError, match=r"coverage_samples_max_peak_bytes\(build\)"):
        coverage.build_coverage_samples(
            mesh, [np.array([0])], tight, stats_out=stats)
    assert stats["preflight_passed"] is False


def test_coverage_576k_depth0_refuses_before_depth_planning(cfg, monkeypatch):
    n_faces = 576_000

    class LargeMesh:
        vertices = np.zeros((3, 3), dtype=np.float64)
        faces = np.zeros((n_faces, 3), dtype=np.int64)

    compact = coverage._coverage_retained_bytes(n_faces, n_faces, 1)
    tight = dataclasses.replace(
        cfg, coverage_samples_max_compact_payload_bytes=compact - 1)

    def _no_face_measurement(*_args, **_kwargs):
        raise AssertionError("depth planning began before depth-0 lower-bound refusal")

    monkeypatch.setattr(
        coverage.placement, "_source_face_max_edges", _no_face_measurement)
    stats = {}
    with pytest.raises(
            ValueError, match="coverage_samples_max_compact_payload_bytes"):
        coverage.build_coverage_samples(
            LargeMesh(), [np.arange(n_faces, dtype=np.int64)],
            tight, stats_out=stats)
    assert stats["n_virtual_cells_planned"] == n_faces
    assert stats["compact_retained_bytes"] == (
        compact)
    assert stats["preflight_passed"] is False


def test_topology_576k_admitted_subprocess_rss():
    probe = _run_resource_probe("topology")
    assert probe["faces"] == 576_000
    assert probe["planned_build"] == topology._planned_peak_build_bytes(
        576_000, 289_081)
    assert probe["planned_build"] <= probe["max_peak"]
    assert probe["planned_validation"] <= probe["max_peak"]
    assert probe["rss_delta"] <= probe["max_peak"]
    assert probe["reported_build_rss_delta"] <= probe["rss_delta"]
    assert probe["reported_total_rss_delta"] == probe["rss_delta"]


def test_coverage_576k_depth0_admitted_subprocess_rss():
    probe = _run_resource_probe("coverage")
    assert probe["faces"] == 576_000
    assert probe["retained"] == coverage._coverage_retained_bytes(
        576_000, 576_000, 1)
    assert probe["planned_build"] <= probe["max_peak"]
    assert probe["planned_validation"] <= probe["max_peak"]
    assert probe["rss_delta"] <= probe["max_peak"]
    assert probe["reported_build_rss_delta"] <= probe["rss_delta"]
    assert probe["reported_total_rss_delta"] == probe["rss_delta"]
