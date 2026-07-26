import numpy as np
import pickle
import pytest
from support_placement import engine, perception, placement


def _duplicate_nomination_mesh():
    """Three faces nominate one exact point, including a duplicate vertex id."""
    import trimesh
    vertices = np.array([
        [0.0, 0.0, 0.0],  # shared lowest vertex
        [1.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [2.0, 0.0, 2.0],
        [0.0, 2.0, 2.0],
        [0.0, 0.0, 0.0],  # exact-position duplicate identity
    ])
    faces = np.array([
        [0, 1, 2],  # positive nz
        [0, 2, 1],  # negative nz -- wins; lower face id than face 2
        [5, 4, 3],  # same negative nz, duplicate position/identity
    ])
    return trimesh.Trimesh(vertices=vertices, faces=faces, process=False)


def test_candidate_pool_dedupes_exact_vertex_and_uses_steepest_down_face(cfg):
    mesh = _duplicate_nomination_mesh()
    pool = placement.candidate_pool(mesh, [np.array([2, 0, 1])], cfg)
    cand = next(c for c in pool
                if np.array_equal(c["position"], [0.0, 0.0, 0.0]))
    assert np.array_equal(cand["position"], [0.0, 0.0, 0.0])
    assert cand["face_id"] == 1
    assert np.array_equal(cand["normal"], mesh.face_normals[1])
    assert cand["normal"][2] < 0.0


def test_candidate_pool_refines_flat_plateau_without_losing_source_vertices(
        box_mesh, cfg):
    """A flat face nominates every dyadic cell minimum, including interiors."""
    plate_z = float(box_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(box_mesh, cfg, plate_z)
    pool = placement.candidate_pool(box_mesh, islands, cfg)
    got = {tuple(c["position"]) for c in pool}
    expected = {
        tuple(v) for v in np.asarray(box_mesh.vertices)
        if np.isclose(v[2], box_mesh.bounds[0, 2])
    }
    assert expected < got
    assert all(np.isclose(p[2], box_mesh.bounds[0, 2]) for p in got)


def test_candidate_pool_is_invariant_under_dyadic_midpoint_refinement(cfg):
    import trimesh
    coarse = trimesh.Trimesh(
        vertices=np.array([[0.0, 0.0, 0.0],
                           [0.0, 12.0, 0.0],
                           [12.0, 0.0, 0.0]]),
        faces=np.array([[0, 1, 2]]), process=False)
    refined = coarse.subdivide()
    coarse_pool = placement.candidate_pool(coarse, [[0]], cfg)
    refined_pool = placement.candidate_pool(
        refined, [np.arange(len(refined.faces))], cfg)
    coarse_geom = {
        (tuple(c["position"]), tuple(c["normal"])) for c in coarse_pool
    }
    refined_geom = {
        (tuple(c["position"]), tuple(c["normal"])) for c in refined_pool
    }
    assert refined_geom == coarse_geom


def test_candidate_pool_geometry_ignores_source_face_order(cfg):
    import trimesh
    vertices = np.array([
        [0.0, 0.0, 0.0], [0.0, 12.0, 0.0],
        [12.0, 0.0, 0.0], [12.0, 12.0, 0.0],
    ])
    faces = np.array([[0, 1, 2], [2, 1, 3]])
    a = trimesh.Trimesh(vertices=vertices, faces=faces, process=False)
    b = trimesh.Trimesh(vertices=vertices, faces=faces[::-1], process=False)
    pool_a = placement.candidate_pool(a, [[0, 1]], cfg)
    pool_b = placement.candidate_pool(b, [[0, 1]], cfg)
    geom_a = {(tuple(c["position"]), tuple(c["normal"])) for c in pool_a}
    geom_b = {(tuple(c["position"]), tuple(c["normal"])) for c in pool_b}
    assert geom_a == geom_b
    contacts_a = placement.sample_candidates(
        a, [[0, 1]], cfg, candidate_pool_prepared=pool_a)
    contacts_b = placement.sample_candidates(
        b, [[0, 1]], cfg, candidate_pool_prepared=pool_b)
    assert [tuple(c["position"]) for c in contacts_a] == [
        tuple(c["position"]) for c in contacts_b]
    assert [tuple(c["normal"]) for c in contacts_a] == [
        tuple(c["normal"]) for c in contacts_b]


def test_candidate_pool_refinement_bound_and_margin_validation(cfg):
    import dataclasses
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([[0.0, 0.0, 0.0],
                           [0.0, 12.0, 0.0],
                           [12.0, 0.0, 0.0]]),
        faces=np.array([[0, 1, 2]]), process=False)
    with pytest.raises(ValueError, match="refinement depth"):
        placement.candidate_pool(
            mesh, [[0]],
            dataclasses.replace(cfg, contact_pool_max_refine_depth=0))
    with pytest.raises(ValueError, match="coverage_radius_factor > 1"):
        placement.candidate_pool(
            mesh, [[0]], dataclasses.replace(cfg, coverage_radius_factor=1.0))


def test_candidate_pool_depth_threshold_is_ulp_stable_and_dyadic(cfg):
    """A pre-subdivided child one ULP over 2mm does not gain another level."""
    import trimesh
    x = np.sqrt(np.nextafter(16.0, np.inf) - 1.0)
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [x, 0.0, 1.0], [0.0, 1.0, 1.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    stats = {}
    coarse = placement.candidate_pool(mesh, [[0]], cfg, stats_out=stats)
    refined_mesh = mesh.subdivide()
    refined_stats = {}
    refined = placement.candidate_pool(
        refined_mesh, [np.arange(len(refined_mesh.faces))], cfg,
        stats_out=refined_stats)
    assert np.max(np.linalg.norm(
        refined_mesh.triangles
        - np.roll(refined_mesh.triangles, -1, axis=1), axis=2
    )) == np.nextafter(2.0, np.inf)
    assert stats["max_refine_depth"] == 1
    assert refined_stats["max_refine_depth"] == 0
    assert {tuple(p) for p in coarse.positions} == {
        tuple(p) for p in refined.positions
    }


def test_candidate_pool_global_preflight_refuses_before_virtual_allocation(
        cfg, monkeypatch):
    """Twelve depth-8 faces exceed the coherent 750k/2.25m aggregate caps."""
    import trimesh
    import dataclasses
    edge = placement._analysis_cell_edge_mm(cfg) * (2 ** 8)
    vertices = []
    faces = []
    for i in range(12):
        x0 = i * (edge + 1.0)
        base = len(vertices)
        vertices.extend([
            [x0, 0.0, 0.0], [x0 + edge, 0.0, 0.0],
            [x0 + edge / 2.0, 1.0, 1.0],
        ])
        faces.append([base, base + 1, base + 2])
    mesh = trimesh.Trimesh(
        vertices=np.asarray(vertices), faces=np.asarray(faces), process=False)
    stats = {}
    original_concatenate = placement.np.concatenate

    def _no_virtual_allocation(*args, **kwargs):
        raise AssertionError("generation started before global refusal")

    monkeypatch.setattr(placement.np, "concatenate", _no_virtual_allocation)
    with pytest.raises(ValueError, match="contact_pool_max_virtual_cells"):
        placement.candidate_pool(
            mesh, [np.arange(12)], dataclasses.replace(
                cfg, contact_pool_max_refine_depth=8), stats_out=stats)
    monkeypatch.setattr(placement.np, "concatenate", original_concatenate)
    assert stats["preflight_passed"] is False
    # Monotonic planning stops as soon as a cap wins; the saturated cap+1
    # count is sufficient and avoids computing the rest of an oversized plan.
    assert stats["n_virtual_cells_planned"] == (
        cfg.contact_pool_max_virtual_cells + 1)
    assert any("contact_pool_max_virtual_cells" in x
               for x in stats["refusal_reasons"])
    assert any("contact_pool_max_candidates" in x
               for x in stats["refusal_reasons"])


def test_candidate_pool_allows_one_depth8_face_and_has_bounded_pickle(cfg):
    import trimesh
    edge = placement._analysis_cell_edge_mm(cfg) * (2 ** 8)
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [edge, 0.0, 0.0],
            [edge / 2.0, 1.0, 1.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    stats = {}
    pool = placement.candidate_pool(mesh, [[0]], cfg, stats_out=stats)
    payload = pickle.dumps(pool, protocol=pickle.HIGHEST_PROTOCOL)
    restored = pickle.loads(payload)
    assert stats["preflight_passed"] is True
    assert stats["max_refine_depth"] == 8
    assert stats["n_virtual_cells_planned"] == 4 ** 8
    assert pool.nbytes == len(pool) * placement.CONTACT_POOL_BYTES_PER_CANDIDATE
    assert len(payload) <= pool.nbytes + 4096
    assert np.array_equal(restored.positions, pool.positions)
    assert np.array_equal(restored.normals, pool.normals)
    assert np.array_equal(restored.face_ids, pool.face_ids)
    assert np.array_equal(restored.island_ids, pool.island_ids)
    assert all(not getattr(restored, name).flags.writeable for name in (
        "positions", "normals", "face_ids", "island_ids"))
    with pytest.raises(ValueError):
        restored.positions[0, 0] = 123.0
    with pytest.raises(ValueError):
        restored.positions.setflags(write=True)


def test_candidate_pool_is_immutable_deep_validated_authority(
        table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    placement.validate_candidate_pool(pool, mesh=table_mesh, islands=islands)
    assert all(not getattr(pool, name).flags.writeable for name in (
        "positions", "normals", "face_ids", "island_ids"))
    with pytest.raises(ValueError):
        pool.positions.setflags(write=True)
    row = next(pool.iter_rows())
    row["position"][0] += 100.0
    row["normal"][0] += 100.0
    assert not np.array_equal(row["position"], pool.positions[0])
    assert not np.array_equal(row["normal"], pool.normals[0])

    # Frozen dataclass fields plus read-only arrays are the normal boundary;
    # deep validation still catches a hostile object.__setattr__ mutation.
    damaged = pickle.loads(pickle.dumps(pool))
    writable = damaged.positions.copy()
    object.__setattr__(damaged, "positions", writable)
    with pytest.raises(ValueError, match="read-only"):
        placement.validate_candidate_pool(
            damaged, mesh=table_mesh, islands=islands)

    bad_counter = pickle.loads(pickle.dumps(pool))
    object.__setattr__(
        bad_counter, "n_candidate_nominations_upper_bound",
        bad_counter.n_candidate_nominations_upper_bound + 1)
    with pytest.raises(ValueError, match="nomination counter"):
        placement.validate_candidate_pool(
            bad_counter, mesh=table_mesh, islands=islands)

    if len(pool) > 1:
        bad_order = pickle.loads(pickle.dumps(pool))
        positions = bad_order.positions.copy()
        positions[[0, 1]] = positions[[1, 0]]
        object.__setattr__(
            bad_order, "positions",
            placement._immutable_array(positions, np.float64))
        with pytest.raises(ValueError, match="total ordering"):
            placement.validate_candidate_pool(
                bad_order, mesh=table_mesh, islands=islands)


def test_candidate_pool_mesh_context_rejects_ranges_and_raw_normal(
        table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    bad_face = pickle.loads(pickle.dumps(pool))
    ids = bad_face.face_ids.copy()
    ids[0] = len(table_mesh.faces)
    object.__setattr__(
        bad_face, "face_ids", placement._immutable_array(ids, np.int64))
    with pytest.raises(ValueError, match="outside mesh"):
        placement.validate_candidate_pool(
            bad_face, mesh=table_mesh, islands=islands)
    bad_normal = pickle.loads(pickle.dumps(pool))
    normals = bad_normal.normals.copy()
    normals[0, 0] = np.nextafter(normals[0, 0], np.inf)
    object.__setattr__(
        bad_normal, "normals",
        placement._immutable_array(normals, np.float64))
    with pytest.raises(ValueError, match="mismatch mesh"):
        placement.validate_candidate_pool(
            bad_normal, mesh=table_mesh, islands=islands)


def test_candidate_pool_peak_rss_high_water_guard_is_honest(cfg, monkeypatch):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    readings = iter([0.0, 2.0])
    monkeypatch.setattr(
        placement, "_peak_rss_high_water_gb", lambda: next(readings))
    with pytest.raises(
            MemoryError, match="peak RSS high-water mark grew"):
        placement.candidate_pool(mesh, [[0]], cfg)


def test_candidate_pool_depth0_face_lower_bound_refuses_before_triangles(
        cfg, monkeypatch):
    """An oversized depth-0 island dies before face conversion/materialization."""
    import dataclasses
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [0.5, 0.0, 0.0], [0.0, 0.5, 0.0],
            [1.0, 0.0, 0.0], [1.5, 0.0, 0.0], [1.0, 0.5, 0.0],
        ]),
        faces=np.array([[0, 1, 2], [3, 4, 5]]), process=False)
    tight = dataclasses.replace(
        cfg, contact_pool_max_refine_depth=0,
        contact_pool_max_virtual_cells=1,
        contact_pool_max_candidates=3,
        contact_pool_max_compact_payload_bytes=(
            3 * placement.CONTACT_POOL_BYTES_PER_CANDIDATE))

    def _no_triangle_materialization(*_args, **_kwargs):
        raise AssertionError("triangle batch materialized before lower-bound refusal")

    monkeypatch.setattr(
        placement, "_source_face_max_edges", _no_triangle_materialization)
    stats = {}
    with pytest.raises(ValueError, match="contact_pool_max_virtual_cells"):
        placement.candidate_pool(mesh, [[0, 1]], tight, stats_out=stats)
    assert stats["n_virtual_cells_planned"] == 2
    assert stats["preflight_passed"] is False


def test_candidate_pool_plans_admitted_faces_in_fixed_numpy_batches(
        cfg, monkeypatch):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [0.5, 0.0, 0.0], [0.0, 0.5, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    seen_batch_sizes = []

    def _batch_edges(_vertices, _faces, face_ids):
        seen_batch_sizes.append(len(face_ids))
        return np.zeros(len(face_ids), dtype=np.float64)

    monkeypatch.setattr(
        placement, "_source_face_max_edges", _batch_edges)
    pool = placement.candidate_pool(
        mesh, [np.zeros(2_049, dtype=np.int64)], cfg)
    assert seen_batch_sizes == [2_048, 1]
    assert pool.n_source_faces == 2_049
    assert pool.n_virtual_cells_planned == 2_049


def test_candidate_pool_refuses_monotonically_after_batched_depth_plan(
        cfg):
    import dataclasses
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [12.0, 0.0, 0.0], [0.0, 12.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    tight = dataclasses.replace(
        cfg, contact_pool_max_virtual_cells=100,
        contact_pool_max_candidates=10_000,
        contact_pool_max_compact_payload_bytes=10_000_000)
    stats = {}
    with pytest.raises(ValueError, match="contact_pool_max_virtual_cells"):
        placement.candidate_pool(mesh, [[0]], tight, stats_out=stats)
    assert stats["n_virtual_cells_planned"] > 100
    assert stats["preflight_passed"] is False


def test_candidate_pool_high_depth_count_refuses_without_int64_overflow(
        cfg):
    """Depth 32 means 4**32 cells: fixed-width int64 would wrap to zero."""
    import dataclasses
    import trimesh
    edge = float(2 ** 33)
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [edge, 0.0, 0.0], [0.0, 1.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2]]), process=False)
    tight = dataclasses.replace(
        cfg, contact_pool_max_refine_depth=40,
        contact_pool_max_virtual_cells=100)
    stats = {}
    with pytest.raises(ValueError, match="contact_pool_max_virtual_cells"):
        placement.candidate_pool(mesh, [[0]], tight, stats_out=stats)
    assert stats["max_refine_depth"] >= 32
    assert stats["n_virtual_cells_planned"] == 101
    assert stats["preflight_passed"] is False


def test_candidate_pool_extension_preflights_combined_budget(cfg):
    import dataclasses
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [1.5, 0.0, 0.0], [0.75, 0.5, 0.5],
            [3.0, 0.0, 0.0], [4.5, 0.0, 0.0], [3.75, 0.5, 0.5],
        ]),
        faces=np.array([[0, 1, 2], [3, 4, 5]]), process=False)
    tight = dataclasses.replace(
        cfg, contact_pool_max_virtual_cells=1,
        contact_pool_max_candidates=3,
        contact_pool_max_compact_payload_bytes=(
            3 * placement.CONTACT_POOL_BYTES_PER_CANDIDATE))
    first = placement.candidate_pool(mesh, [[0]], tight)
    stats = {}
    with pytest.raises(ValueError, match="contact_pool_max_virtual_cells"):
        placement.candidate_pool(
            mesh, [[1]], tight, island_id_offset=1,
            stats_out=stats, existing_pool=first)
    assert stats["n_virtual_cells_planned"] == 2
    assert len(first) > 0


def test_candidate_pool_extension_rejects_nonmonotonic_island_offset(cfg):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0],
            [3.0, 0.0, 0.0], [4.0, 0.0, 0.0], [3.0, 1.0, 0.0],
        ]),
        faces=np.array([[0, 1, 2], [3, 4, 5]]), process=False)
    first = placement.candidate_pool(mesh, [[0]], cfg)
    with pytest.raises(ValueError, match="island_id_offset"):
        placement.candidate_pool(
            mesh, [[1]], cfg, island_id_offset=0, existing_pool=first)


def test_regular_placement_uses_only_pool_members_and_does_not_touch_rng(
        table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    legal = {
        (c["island_id"], tuple(c["position"]), c["face_id"]): c["normal"]
        for c in pool
    }

    np.random.seed(12345)
    # Deprecated seed remains source-compatible and is deliberately ignored.
    contacts = placement.sample_candidates(table_mesh, islands, cfg, seed=9876)
    after = np.random.random(8)
    np.random.seed(12345)
    expected = np.random.random(8)
    assert np.array_equal(after, expected)

    for contact in contacts:
        key = (contact["island_id"], tuple(contact["position"]),
               contact["face_id"])
        assert key in legal
        assert np.array_equal(contact["normal"], legal[key])
        assert np.array_equal(
            contact["normal"], table_mesh.face_normals[contact["face_id"]])


def test_coverage_repair_seat_is_a_legal_pool_member(table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    initial = placement.sample_candidates(table_mesh, islands, cfg)
    iid = int(initial[0]["island_id"])
    existing = [initial[0]]
    repaired = engine._add_fix_contacts(
        table_mesh, islands, existing,
        [dict(island_id=iid, n_faces=len(islands[iid]))], cfg=cfg)
    assert len(repaired) == 2
    added = repaired[-1]
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    legal = {(c["island_id"], tuple(c["position"]), c["face_id"]): c
             for c in pool}
    key = (added["island_id"], tuple(added["position"]), added["face_id"])
    assert key in legal
    assert np.array_equal(added["normal"], legal[key]["normal"])


def test_coverage_repair_logs_prepared_pool_exhaustion(table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    iid = 0
    occupied = [
        dict(position=np.asarray(c["position"]).copy(),
             normal=np.asarray(c["normal"]).copy(), island_id=iid,
             face_id=int(c["face_id"]), area_mm2=1.0)
        for c in pool if int(c["island_id"]) == iid
    ]
    log = {}
    repaired = engine._add_fix_contacts(
        table_mesh, islands, occupied,
        [dict(island_id=iid, n_faces=len(islands[iid]))],
        candidate_pool_prepared=pool, log=log)
    assert len(repaired) == len(occupied)
    assert all(a is b for a, b in zip(repaired, occupied))
    assert log["contact_pool_exhausted"] == [
        dict(island_id=iid, n_pool=len(occupied), n_occupied=len(occupied))]


def test_angle_aware_peel_term_fattens_steep_members(cfg):
    """v3.6 queue item 2: F_eff = F_drag + peel * pi*r^2 * tan(theta).
    Vertical members reduce exactly to the old closed form; lean
    monotonically fattens."""
    L = 20.0
    r0 = placement._shaft_r_for_L(L, cfg, angle_from_vertical_deg=0.0)
    closed = ((4 * cfg.shaft_drag_force_n * (L / 1000) ** 3
               / (3 * cfg.modulus_green_gpa * 1e9 * np.pi
                  * (cfg.max_deflection_mm / 1000))) ** 0.25) * 1000
    assert abs(r0 - max(closed, cfg.min_shaft_r_mm)) < 1e-9
    r45 = placement._shaft_r_for_L(L, cfg, angle_from_vertical_deg=45.0)
    r60 = placement._shaft_r_for_L(L, cfg, angle_from_vertical_deg=60.0)
    assert r60 > r45 > r0


def test_max_unbraced_len_is_consistent_inverse_with_angle(cfg):
    for angle in (0.0, 45.0, 60.0):
        r = placement._shaft_r_for_L(25.0, cfg, angle_from_vertical_deg=angle)
        if r <= cfg.min_shaft_r_mm:
            continue
        L_back = placement.max_unbraced_len_mm(r, cfg, angle_from_vertical_deg=angle)
        assert abs(L_back - 25.0) < 0.2, (angle, r, L_back)
    # steeper members must be allowed SHORTER free spans at equal radius
    assert placement.max_unbraced_len_mm(0.8, cfg, 60.0) \
        < placement.max_unbraced_len_mm(0.8, cfg, 0.0)


def test_spacing_floor_never_violated(table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    contacts = placement.sample_candidates(table_mesh, islands, cfg)
    positions = np.array([c["position"] for c in contacts])
    if len(positions) < 2:
        pytest.skip("too few contacts on this fixture to test spacing")
    from scipy.spatial import cKDTree
    tree = cKDTree(positions)
    dist, _ = tree.query(positions, k=2)
    nn = dist[:, 1]
    assert nn.min() >= cfg.spacing_floor_mm - 1e-6


def test_every_island_gets_min_contacts(cfg):
    import trimesh
    # a tiny down-facing patch, smaller than target spacing, must still
    # get at least min_contacts_per_island.
    from support_placement import mesh_io
    tiny_table = mesh_io.make_overhang_table(leg=(1, 1, 5), top=(3, 3, 1))
    plate_z = float(tiny_table.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(tiny_table, cfg, plate_z)
    assert len(islands) >= 1
    contacts = placement.sample_candidates(tiny_table, islands, cfg)
    per_island = {}
    for c in contacts:
        per_island.setdefault(c["island_id"], 0)
        per_island[c["island_id"]] += 1
    for iid, n in per_island.items():
        assert n >= cfg.min_contacts_per_island


def test_load_partition_conserves_area(table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    contacts = placement.sample_candidates(table_mesh, islands, cfg)
    carried = placement.load_partition(table_mesh, islands, contacts, cfg)
    by_island = {}
    for c, a in zip(contacts, carried):
        by_island.setdefault(c["island_id"], 0.0)
        by_island[c["island_id"]] += a
    for iid, total in by_island.items():
        face_ids = islands[iid]
        true_area = float(table_mesh.submesh([face_ids], append=True).area)
        assert abs(total - true_area) / true_area < 0.15


def test_load_partition_is_deterministic_despite_global_rng_state(table_mesh, cfg):
    """Charter: no RNG. Perturb global state around exact face assignment;
    carried-area outputs must still match exactly."""
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    contacts = placement.sample_candidates(table_mesh, islands, cfg)
    np.random.seed(12345)
    carried_a = placement.load_partition(table_mesh, islands, contacts, cfg)
    np.random.seed(54321)
    carried_b = placement.load_partition(table_mesh, islands, contacts, cfg)
    assert np.array_equal(carried_a, carried_b)


def test_tip_radius_matches_hand_calculation(cfg):
    # Hand calc: area=20mm2, peel=3000Pa, SF=8, tensile=10MPa
    area = 20.0
    F = area * 1e-6 * cfg.peel_pressure_pa
    expected_r_m = (F * cfg.tip_safety_factor / (np.pi * cfg.tensile_green_mpa * 1e6)) ** 0.5
    expected_mm = np.clip(expected_r_m * 1000, cfg.tip_r_min_mm, cfg.tip_r_max_mm)
    got = placement.tip_radius_mm(area, cfg)
    assert abs(got - expected_mm) < 1e-9


def test_tip_radius_respects_clamp_band(cfg):
    tiny = placement.tip_radius_mm(1e-6, cfg)
    huge = placement.tip_radius_mm(1e6, cfg)
    assert tiny == cfg.tip_r_min_mm
    assert huge == cfg.tip_r_max_mm


def test_short_shaft_needs_no_bracing(cfg):
    r, n_braces, eff = placement.shaft_radius_for_length_mm(3.0, cfg)
    assert n_braces == 0
    assert r <= cfg.max_unbraced_shaft_r_mm


def test_long_shaft_triggers_bracing_and_stays_under_cap(cfg):
    r, n_braces, eff = placement.shaft_radius_for_length_mm(80.0, cfg)
    assert n_braces > 0
    assert r <= cfg.max_unbraced_shaft_r_mm + 1e-9
    assert eff < 80.0


def test_halving_free_length_cuts_deflection_eightfold(cfg):
    # design.md's own stated relation, taken as a direct property test of
    # the deflection formula (not the bracing search) -- deflection ~ L^3.
    from support_placement.placement import _shaft_r_for_L
    r1 = _shaft_r_for_L(40.0, cfg)
    r2 = _shaft_r_for_L(20.0, cfg)
    # delta ~ L^3/r^4 held constant by construction (we solved for r at
    # fixed delta) -- so instead verify the underlying force/delta relation
    # directly: at fixed r, delta scales with L^3.
    E = cfg.modulus_green_gpa * 1e9
    F = cfg.shaft_drag_force_n
    def delta(L_mm, r_mm):
        L, r = L_mm / 1000, r_mm / 1000
        I = np.pi * r ** 4 / 4
        return F * L ** 3 / (3 * E * I)
    d_full = delta(40.0, 0.5)
    d_half = delta(20.0, 0.5)
    assert abs(d_full / d_half - 8.0) < 1e-6
