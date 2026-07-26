import dataclasses
import pickle

import numpy as np
import trimesh

from support_placement import (
    coverage, engine, placement, specialized_contacts, topology, verify,
)


def _connected_pair_mesh():
    return trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ], dtype=np.float64),
        faces=np.array([[0, 1, 2], [1, 3, 2]], dtype=np.int64),
        process=False)


def _disconnected_near_mesh():
    return trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.0, 0.2, 0.0],
            [0.3, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [0.3, 0.2, 0.0],
        ], dtype=np.float64),
        faces=np.array([[0, 1, 2], [3, 4, 5]], dtype=np.int64),
        process=False)


def _minimum_contact(mesh, island_id, face_id, position, region_pts):
    return dict(
        position=np.asarray(position, dtype=np.float64).copy(),
        normal=np.asarray(mesh.face_normals[face_id], dtype=np.float64).copy(),
        island_id=int(island_id),
        area_mm2=float(mesh.area_faces[face_id]),
        minimum=True,
        coverage_face_id=int(face_id),
        coverage_component_id=None,
        region_pts=np.asarray(region_pts, dtype=np.float64).copy(),
    )


def _authoritative_inputs(mesh, islands, contacts, cfg):
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    for contact in contacts:
        contact["coverage_component_id"] = int(
            sidecar.component_ids[contact["coverage_face_id"]])
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    return sidecar, samples


def test_cross_island_exact_demands_share_one_union_loaded_contact(cfg):
    mesh = _connected_pair_mesh()
    islands = [
        np.array([0], dtype=np.int64),
        np.array([1], dtype=np.int64),
    ]
    seat = np.array([0.5, 0.5, 0.0])
    region = np.array([seat])
    contacts = [
        _minimum_contact(mesh, 0, 0, seat, region),
        _minimum_contact(mesh, 1, 1, seat, region),
    ]
    sidecar, samples = _authoritative_inputs(
        mesh, islands, contacts, cfg)

    merged, stats = engine._consolidate_specialized_contacts(
        mesh, islands, contacts, cfg, sidecar, samples)

    assert len(merged) == 1
    assert stats["physical_contacts_removed"] == 1
    assert specialized_contacts.owner_island_ids(merged[0]) == (0, 1)
    assert len(specialized_contacts.logical_demands(merged[0])) == 2
    carried = placement.load_partition(mesh, islands, merged, cfg)
    assert carried.tolist() == [float(mesh.area_faces.sum())]
    sizing_cfg = dataclasses.replace(
        cfg, peel_pressure_pa=3_000_000.0,
        tip_r_min_mm=0.0, tip_r_max_mm=10.0)
    combined_tip = placement.tip_radius_mm(carried, sizing_cfg)[0]
    single_tip = placement.tip_radius_mm(
        [float(mesh.area_faces[0])], sizing_cfg)[0]
    assert combined_tip > single_tip

    coverage_report = verify.coverage_report(
        mesh, islands, merged, cfg, coverage_samples=samples,
        face_topology=sidecar)
    assert [row["covered"] for row in coverage_report] == [True, True]
    assert verify.regular_contact_invariant_report(
        mesh, merged, None, islands=islands,
        face_topology=sidecar) == []


def test_same_island_minima_inside_spacing_floor_cluster_when_one_seat_serves_both(
        cfg):
    mesh = _connected_pair_mesh()
    islands = [np.array([0, 1], dtype=np.int64)]
    first = np.array([0.2, 0.2, 0.0])
    second = np.array([0.566, 0.2, 0.0])
    # The authoritative birth regions overlap both nominated seats, as happens
    # for adjacent minima faces in one physical low region.
    shared_region = np.array([first, second])
    contacts = [
        _minimum_contact(mesh, 0, 0, first, shared_region),
        _minimum_contact(mesh, 0, 1, second, shared_region),
    ]
    sidecar, samples = _authoritative_inputs(
        mesh, islands, contacts, cfg)
    before = engine.specialized_physical_invariant_report(
        mesh, islands, contacts, cfg, sidecar, samples)
    assert before[0]["reason"] == "unmerged-compatible-specialized-pair"

    merged, _stats = engine._consolidate_specialized_contacts(
        mesh, islands, contacts, cfg, sidecar, samples)

    assert len(merged) == 1
    assert np.array_equal(merged[0]["position"], first)
    assert engine.specialized_physical_invariant_report(
        mesh, islands, merged, cfg, sidecar, samples) == []


def test_near_specialized_demands_on_incompatible_components_do_not_merge(cfg):
    mesh = _disconnected_near_mesh()
    islands = [
        np.array([0], dtype=np.int64),
        np.array([1], dtype=np.int64),
    ]
    first = np.asarray(mesh.triangles_center[0], dtype=np.float64)
    second = np.asarray(mesh.triangles_center[1], dtype=np.float64)
    contacts = [
        _minimum_contact(mesh, 0, 0, first, [first, second]),
        _minimum_contact(mesh, 1, 1, second, [first, second]),
    ]
    sidecar, samples = _authoritative_inputs(
        mesh, islands, contacts, cfg)
    assert sidecar.component_ids.tolist() == [0, 1]

    merged, stats = engine._consolidate_specialized_contacts(
        mesh, islands, contacts, cfg, sidecar, samples)

    assert len(merged) == 2
    assert stats["physical_contacts_removed"] == 0
    assert engine.specialized_physical_invariant_report(
        mesh, islands, merged, cfg, sidecar, samples) == []


def test_shared_specialized_provenance_is_deterministic_and_pickle_stable(cfg):
    mesh = _connected_pair_mesh()
    islands = [
        np.array([0], dtype=np.int64),
        np.array([1], dtype=np.int64),
    ]
    seat = np.array([0.5, 0.5, 0.0])
    contacts = [
        _minimum_contact(mesh, 0, 0, seat, [seat]),
        _minimum_contact(mesh, 1, 1, seat, [seat]),
    ]
    sidecar, samples = _authoritative_inputs(
        mesh, islands, contacts, cfg)

    forward, _ = engine._consolidate_specialized_contacts(
        mesh, islands, contacts, cfg, sidecar, samples)
    reverse, _ = engine._consolidate_specialized_contacts(
        mesh, islands, list(reversed(contacts)), cfg, sidecar, samples)
    reloaded = pickle.loads(pickle.dumps(
        forward, protocol=pickle.HIGHEST_PROTOCOL))

    assert engine._prepared_value_exact_equal(forward, reverse)
    assert engine._prepared_value_exact_equal(forward, reloaded)
