import numpy as np
import pytest
from scipy.spatial import cKDTree
from support_placement import (
    coverage, perception, placement, routing, topology, verify, mesh_io,
    joints, specialized_contacts,
)
from support_placement import tree as tm


def _m(p0, p1, r0, r1, owner, kind, embed_end=None):
    """v4 member dict (design-v4.md #2 schema) -- migrated mechanically
    from the v3 Trunk/brace-dict fixtures (same geometry), assertions
    unchanged."""
    return dict(p0=np.asarray(p0, float), p1=np.asarray(p1, float),
                r0=r0, r1=r1, owner=owner, kind=kind, embed_end=embed_end)


def _member(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("contact", 0), embed_end=None):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner, embed_end=embed_end)


def test_coverage_report_flags_island_with_no_contacts(table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    report = verify.coverage_report(table_mesh, islands, [], cfg)
    assert len(report) == len(islands)
    assert all(not r["covered"] for r in report)


def test_coverage_report_empty_islands_needs_no_mesh_or_sidecars(cfg):
    assessments = []
    report = verify.coverage_report(
        None, [], [], cfg, assessment_out=assessments)

    assert report == []
    assert len(assessments) == 1
    assessment = assessments[0]
    assert assessment.__dataclass_params__.frozen
    for field, dtype in (
            ("covered_mask", np.bool_),
            ("nearest_mm", np.float64),
            ("sample_components", np.int64),
            ("failing_sample_mask", np.bool_)):
        value = getattr(assessment, field)
        assert value.shape == (0,)
        assert value.dtype == np.dtype(dtype)
    assert assessment.failing_route_keys == ()


def test_coverage_report_passes_with_virtual_low_vertex_pool(table_mesh, cfg):
    """Coarse source triangles remain coverable without centroid/RNG seats."""
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    contacts = placement.sample_candidates(
        table_mesh, islands, cfg, candidate_pool_prepared=pool)
    report = verify.coverage_report(table_mesh, islands, contacts, cfg)
    assert all(r["covered"] for r in report), report


def test_coverage_report_shared_faces_route_by_each_named_island(cfg):
    """Legacy scalar specialized routes still credit their named island."""
    import trimesh
    from support_placement import coverage, topology

    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.0, 0.2, 0.0],
        ]),
        faces=np.array([[0, 1, 2]], dtype=np.int64),
        process=False)
    islands = [
        np.array([0], dtype=np.int64),
        np.array([0], dtype=np.int64),
    ]
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    legacy_specialized = dict(
        position=samples.positions[samples.island_slice(0).start].copy(),
        normal=mesh.face_normals[0].copy(),
        island_id=0, minimum=True, coverage_face_id=0)
    assert list(specialized_contacts.coverage_routes(
        legacy_specialized)) == [(0, 0, None)]
    contacts = [
        legacy_specialized,
        dict(
            position=samples.positions[samples.island_slice(1).start].copy(),
            normal=mesh.face_normals[0].copy(),
            island_id=1, face_id=0),
    ]

    report = verify.coverage_report(
        mesh, islands, contacts, cfg,
        coverage_samples=samples, face_topology=sidecar)

    assert [row["covered"] for row in report] == [True, True]
    assert all(row["covered_fraction"] == 1.0 for row in report)


def test_regular_contact_invariant_gate_rejects_stale_provenance(
        table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    contacts = placement.sample_candidates(
        table_mesh, islands, cfg, candidate_pool_prepared=pool)
    assert verify.regular_contact_invariant_report(
        table_mesh, contacts, pool, islands=islands) == []

    bad = [dict(c) for c in contacts]
    bad[0] = dict(bad[0], position=np.asarray(bad[0]["position"]).copy())
    bad[0]["position"][0] = np.nextafter(bad[0]["position"][0], np.inf)
    violations = verify.regular_contact_invariant_report(
        table_mesh, bad, pool, islands=islands)
    assert violations[0]["reason"] == "not-in-prepared-pool"

    bad = [dict(c) for c in contacts]
    bad[0] = dict(bad[0], normal=np.asarray(bad[0]["normal"]).copy())
    bad[0]["normal"][0] = np.nextafter(bad[0]["normal"][0], np.inf)
    violations = verify.regular_contact_invariant_report(
        table_mesh, bad, pool, islands=islands)
    assert violations[0]["reason"] == "face-normal-mismatch"

    bad = [dict(c) for c in contacts]
    bad[0]["candidate_pool_row"] = len(pool)
    violations = verify.regular_contact_invariant_report(
        table_mesh, bad, pool, islands=islands)
    assert violations[0]["reason"] == "invalid-candidate-pool-row"

    specialized = dict(
        position=np.zeros(3), normal=np.array([0.0, 0.0, -1.0]),
        island_id=0, minimum=True, region_pts=np.zeros((1, 3)), face_id=0,
        coverage_face_id=0)
    violations = verify.regular_contact_invariant_report(
        table_mesh, [specialized], pool, islands=islands)
    assert violations[0]["reason"] == "specialized-has-regular-face-id"


def test_regular_contact_invariant_fails_closed_without_prepared_pool(
        table_mesh, cfg):
    regular = dict(
        position=np.zeros(3), normal=np.array([0.0, 0.0, -1.0]),
        island_id=0, face_id=0)
    violations = verify.regular_contact_invariant_report(
        table_mesh, [regular], None)
    assert violations == [
        dict(contact_idx=0, reason="missing-prepared-pool")]
    report = verify.self_verify(
        table_mesh, [], [regular], [], np.array([]), np.array([]),
        cfg, plate_z=0.0)
    assert report["ok"] is False
    assert report["regular_contact_invariant_violations"] == violations


def test_regular_contact_invariant_rejects_pool_face_in_wrong_island(cfg):
    import trimesh
    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [0.5, 0.0, 1.0], [0.0, 0.5, 1.0],
            [3.0, 0.0, 0.0], [3.5, 0.0, 1.0], [3.0, 0.5, 1.0],
        ]),
        faces=np.array([[0, 1, 2], [3, 4, 5]]), process=False)
    islands = [[0], [1]]
    pool = placement.candidate_pool(mesh, islands, cfg)
    wrong_ids = pool.island_ids.copy()
    # Keep the asserted contact's own row (row 0, island 0, face 0) intact.
    # Corrupt only the OTHER row so an old lookup-only verifier would still
    # find and accept the asserted contact.
    wrong_ids[1] = 0
    corrupted = placement.ContactCandidatePool(
        positions=pool.positions,
        normals=pool.normals,
        face_ids=pool.face_ids,
        island_ids=wrong_ids,
        n_virtual_cells_planned=pool.n_virtual_cells_planned,
        n_candidate_nominations_upper_bound=(
            pool.n_candidate_nominations_upper_bound),
        n_source_faces=pool.n_source_faces,
        max_refine_depth=pool.max_refine_depth)
    contact = next(iter(pool))
    # Structural and mesh-only validation deliberately pass: only contextual
    # island membership exposes that face 1 was assigned to island 0.
    assert placement.validate_candidate_pool(
        corrupted, mesh=mesh) is corrupted
    assert np.array_equal(corrupted.positions[0], contact["position"])
    assert int(corrupted.face_ids[0]) == contact["face_id"]
    violations = verify.regular_contact_invariant_report(
        mesh, [contact], corrupted, islands=islands)
    assert violations == [
        dict(contact_idx=0, reason="missing-prepared-pool")]


def test_specialized_provenance_rejects_wrong_island_and_route_component(cfg):
    import trimesh

    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0], [0.2, 0.0, 0.0], [0.0, 0.2, 0.0],
            [1.0, 0.0, 0.0], [1.2, 0.0, 0.0], [1.0, 0.2, 0.0],
        ]),
        faces=np.array([[0, 1, 2], [3, 4, 5]], dtype=np.int64),
        process=False)
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    assert sidecar.component_ids.tolist() == [0, 1]
    islands = [
        np.array([0], dtype=np.int64),
        np.array([1], dtype=np.int64),
    ]
    base = dict(
        position=mesh.triangles_center[0].copy(),
        normal=mesh.face_normals[0].copy(),
        island_id=0, minimum=True,
        region_pts=mesh.triangles_center.copy(),
        coverage_face_id=0, coverage_component_id=0)

    wrong_island = dict(base, coverage_face_id=1)
    violations = verify.regular_contact_invariant_report(
        mesh, [wrong_island], None, islands=islands,
        face_topology=sidecar)
    assert violations == [dict(
        contact_idx=0,
        reason="specialized-coverage-face-outside-island",
        island_id=0, coverage_face_id=1)]

    spanning_island = [np.array([0, 1], dtype=np.int64)]
    wrong_component = dict(
        base, island_id=0, coverage_face_id=1, coverage_component_id=0)
    violations = verify.regular_contact_invariant_report(
        mesh, [wrong_component], None, islands=spanning_island,
        face_topology=sidecar)
    assert violations == [dict(
        contact_idx=0,
        reason="specialized-coverage-component-mismatch",
        coverage_face_id=1, coverage_component_id=0,
        resolved_component_id=1)]


def test_self_verify_hard_gates_unrouted_specialized_and_keeps_clean_empty(cfg):
    import trimesh

    mesh = trimesh.Trimesh(
        vertices=np.array([
            [0.0, 0.0, 0.0],
            [0.2, 0.0, 0.0],
            [0.0, 0.2, 0.0],
        ]),
        faces=np.array([[0, 1, 2]], dtype=np.int64),
        process=False)
    islands = [np.array([0], dtype=np.int64)]
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    specialized = dict(
        position=mesh.triangles_center[0].copy(),
        normal=mesh.face_normals[0].copy(),
        island_id=0, minimum=True,
        region_pts=mesh.triangles_center.copy(),
        coverage_face_id=0, coverage_component_id=0)
    assert verify.regular_contact_invariant_report(
        mesh, [specialized], None, islands=islands,
        face_topology=sidecar) == []

    clean = verify.self_verify(
        mesh, islands, [specialized], [], np.array([]), np.array([]),
        cfg, plate_z=-1.0, contact_candidate_pool=None,
        provenance_contacts=[specialized], coverage_samples=samples,
        face_topology=sidecar)
    assert clean["unrouted_specialized_contacts"] == []
    assert clean["regular_contact_invariant_violations"] == []

    failed = verify.self_verify(
        mesh, islands, [], [], np.array([]), np.array([]),
        cfg, plate_z=-1.0, unrouted=[0], contact_candidate_pool=None,
        provenance_contacts=[specialized], coverage_samples=samples,
        face_topology=sidecar)
    assert failed["unrouted_specialized_contacts"] == [
        dict(contact_idx=0, island_id=0)]
    assert failed["ok"] is False


def test_specialized_face_id_fails_class_scope_even_without_pool(
        table_mesh, cfg):
    islands = [np.arange(len(table_mesh.faces), dtype=np.int64)]
    sidecar = topology.build_face_topology(table_mesh, cfg=cfg)
    specialized = dict(
        position=np.zeros(3), normal=np.array([0.0, 0.0, -1.0]),
        island_id=0, minimum=True, region_pts=np.zeros((1, 3)), face_id=0,
        coverage_face_id=0,
        coverage_component_id=int(sidecar.component_ids[0]))
    violations = verify.regular_contact_invariant_report(
        table_mesh, [specialized], None, islands=islands,
        face_topology=sidecar)
    assert violations == [dict(
        contact_idx=0, reason="specialized-has-regular-face-id", face_id=0)]
    report = verify.self_verify(
        table_mesh, islands, [specialized], [], np.array([]), np.array([]),
        cfg, plate_z=0.0, face_topology=sidecar)
    assert report["ok"] is False
    assert report["regular_contact_invariant_violations"] == violations


def test_clearance_report_aggregates_route_violations(box_mesh, cfg):
    surface_tree = routing.build_surface_tree(box_mesh)
    bad = _m([0.0, 0.0, 10.0], [0.0, 0.0, -5.0], 0.3, 0.6, ("contact", 0), "trunk")
    good = _m([8.0, 0.0, 10.0], [8.0, 0.0, -5.0], 0.3, 0.6, ("contact", 1), "trunk")
    report = verify.clearance_report(box_mesh, [good, bad], cfg, surface_tree=surface_tree)
    assert all(v["member_idx"] == 1 for v in report)
    assert len(report) > 0


# ------------------------------- p6-routing Finding A: embed-end masking --
# verify.clearance_report is a DELIBERATE independent duplicate of tree.
# Forest.revalidate's embed-end exemption (see that module's docstring for
# the full inset = contact_depth_mm + pitch_bound_mm + r_end + standoff_mm
# derivation) -- these mirror test_tree.py's mandatory masking tests
# (a)/(b)/(c)/(d) against THIS function directly, independently, per the
# orchestrator's ruling: apply the same formula as a second implementation,
# never a shared helper across that boundary.
def _dense_line(p_a, p_b, t_lo, t_hi, n=40, lateral_offset=None):
    p_a = np.asarray(p_a, float)
    p_b = np.asarray(p_b, float)
    ts = np.linspace(t_lo, t_hi, n)
    pts = p_a[None, :] + ts[:, None] * (p_b - p_a)[None, :]
    if lateral_offset is not None:
        pts = pts + np.asarray(lateral_offset, float)[None, :]
    return pts


def _dense_background_cloud(n_side=150, spacing_mm=0.3, offset=(1.0e5, 1.0e5, 1.0e5)):
    """See test_tree.py's identical helper for the full rationale: a
    deterministic dense grid standing in for "the rest of a real mesh's
    surface_tree" so routing.surface_tree_pitch_bound_mm measures a
    representative ~0.3mm density instead of a bare test point's own
    incidental (often much larger) spacing."""
    idx = np.arange(n_side, dtype=float)
    gx, gy = np.meshgrid(idx, idx)
    pts = np.stack([gx.ravel(), gy.ravel(), np.zeros(gx.size)], axis=1) * spacing_mm
    return pts + np.asarray(offset, float)


def _pitch_bound_mm(cfg):
    return routing.surface_tree_pitch_bound_mm(
        cKDTree(_dense_background_cloud()), percentile=cfg.surface_pitch_bound_percentile)


def test_clearance_report_passes_on_surface_anchored_strut_nothing_else_nearby(box_mesh, cfg):
    """MASKING TEST (a): mirrors test_tree.py's identical case against
    verify.clearance_report directly -- fails on master's byte-identical
    stale duplicate (fixed contact_depth_mm+0.2 inset)."""
    anchor = np.array([0.0, 0.0, 0.0])
    se = np.array([8.0, 0.0, 6.0])
    r_end = 0.5
    m = _m(se, anchor, 0.3, r_end, ("contact", 0), "strut", embed_end="p1")
    surface_tree = cKDTree(np.vstack([[anchor], _dense_background_cloud()]))
    report = verify.clearance_report(box_mesh, [m], cfg, surface_tree=surface_tree)
    assert report == []


def test_clearance_report_catches_midshaft_clip_just_outside_exemption_zone(box_mesh, cfg):
    """MASKING TEST (b): dead-on mid-shaft clip just past the exemption
    boundary must still be caught."""
    anchor = np.array([0.0, 0.0, 0.0])
    se = np.array([8.0, 0.0, 6.0])
    r_end = 0.5
    m = _m(se, anchor, 0.3, r_end, ("contact", 0), "strut", embed_end="p1")
    pitch_bound = _pitch_bound_mm(cfg)
    inset = cfg.contact_depth_mm + pitch_bound + r_end + cfg.standoff_mm
    length = float(np.linalg.norm(se - anchor))
    t_clip_lo = 1.0 - (inset + 1.5) / length
    t_clip_hi = 1.0 - (inset + 0.05) / length
    clip_cloud = _dense_line(se, anchor, t_clip_lo, t_clip_hi)
    surface_tree = cKDTree(np.vstack([[anchor], clip_cloud, _dense_background_cloud()]))
    report = verify.clearance_report(box_mesh, [m], cfg, surface_tree=surface_tree)
    assert len(report) == 1
    assert report[0]["member_idx"] == 0


def test_clearance_report_no_exemption_when_embed_end_is_not_on_model_surface(box_mesh, cfg):
    """MASKING TEST (c): embed_end=None gets no new exemption at all."""
    p0 = np.array([8.0, 0.0, 6.0])
    p1 = np.array([0.0, 0.0, 0.0])
    r_end = 0.5
    m = _m(p0, p1, 0.3, r_end, ("contact", 0), "trunk", embed_end=None)
    length = float(np.linalg.norm(p1 - p0))
    pitch_bound = _pitch_bound_mm(cfg)
    inset = cfg.contact_depth_mm + pitch_bound + r_end + cfg.standoff_mm
    clip_near_p1 = p1 + (p0 - p1) / length * (inset * 0.5)
    surface_tree = cKDTree(np.array([clip_near_p1]))
    report = verify.clearance_report(box_mesh, [m], cfg, surface_tree=surface_tree)
    assert len(report) == 1


def test_clearance_report_rejects_anchored_strut_that_also_clips_other_geometry(box_mesh, cfg):
    """MASKING TEST (d): a legitimate anchor plus a separate, unrelated
    mid-shaft clip must still be rejected."""
    anchor = np.array([0.0, 0.0, 0.0])
    se = np.array([8.0, 0.0, 6.0])
    r_end = 0.5
    length = float(np.linalg.norm(se - anchor))
    pitch_bound = _pitch_bound_mm(cfg)
    inset = cfg.contact_depth_mm + pitch_bound + r_end + cfg.standoff_mm
    other_clip = anchor + (se - anchor) / length * (inset + 3.0)
    background = _dense_background_cloud()
    m = _m(se, anchor, 0.3, r_end, ("contact", 0), "strut", embed_end="p1")

    clean_tree = cKDTree(np.vstack([[anchor], background]))
    assert verify.clearance_report(box_mesh, [m], cfg, surface_tree=clean_tree) == []

    surface_tree = cKDTree(np.vstack([[anchor, other_clip], background]))
    report = verify.clearance_report(box_mesh, [m], cfg, surface_tree=surface_tree)
    assert len(report) == 1


def test_load_report_flags_clamped_overloaded_tip(cfg):
    # an absurdly large carried area forces the tip to clamp at r_max,
    # which can't actually carry that load at the target safety factor.
    carried = np.array([5000.0])  # mm^2
    r = placement.tip_radius_mm(carried, cfg)
    report = verify.load_report(carried, r, cfg)
    assert len(report) == 1
    assert report[0]["realized_sf"] < cfg.tip_safety_factor


def test_load_report_clean_for_normal_load(cfg):
    carried = np.array([15.0])  # typical corpus-scale carried area
    r = placement.tip_radius_mm(carried, cfg)
    report = verify.load_report(carried, r, cfg)
    assert report == []


def test_self_verify_ok_on_a_clean_small_case(table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    contacts = placement.sample_candidates(
        table_mesh, islands, cfg, candidate_pool_prepared=pool)
    carried = placement.load_partition(table_mesh, islands, contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    members = []
    for ci, (c, r) in enumerate(zip(contacts, tip_r)):
        pos = np.asarray(c["position"], float)
        anchor = np.array([pos[0], pos[1], plate_z])
        shaft_r, _n_braces, _eff = placement.shaft_radius_for_length_mm(
            abs(pos[2] - anchor[2]), cfg)
        members.append(_m(pos, anchor, float(r), shaft_r, ("contact", ci), "trunk"))
    report = verify.self_verify(table_mesh, islands, contacts, members, carried, tip_r,
                                 cfg, plate_z, contact_candidate_pool=pool)
    assert report["uncovered_islands"] == []
    assert report["load_violations"] == []
    assert report["regular_contact_invariant_violations"] == []


def test_self_verify_empty_clean_control_passes_prepared_pool(
        table_mesh, cfg):
    pool = placement.candidate_pool(table_mesh, [], cfg)
    report = verify.self_verify(
        table_mesh, [], [], [], np.array([]), np.array([]), cfg,
        plate_z=0.0, contact_candidate_pool=pool,
        provenance_contacts=[])
    assert report["ok"] is True


def test_unrouted_regular_contact_cannot_evade_provenance_gate(
        table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    contacts = placement.sample_candidates(
        table_mesh, islands, cfg, candidate_pool_prepared=pool)
    corrupt = [dict(c) for c in contacts]
    corrupt[0] = dict(
        corrupt[0], position=np.asarray(corrupt[0]["position"]).copy())
    corrupt[0]["position"][0] = np.nextafter(
        corrupt[0]["position"][0], np.inf)
    routed = corrupt[1:]
    report = verify.self_verify(
        table_mesh, islands, routed, [], np.array([]), np.array([]), cfg,
        plate_z=plate_z, contact_candidate_pool=pool,
        provenance_contacts=corrupt, unrouted=[0])
    assert report["ok"] is False
    assert report["regular_contact_invariant_violations"][0][
        "contact_idx"] == 0
    assert report["regular_contact_invariant_violations"][0][
        "reason"] == "not-in-prepared-pool"


# ------------------------------------------------ p6-reach consistency fix --
# self_verify's printability check used to run over the RAW forest.members
# (every member ever appended, including tree.Forest.dead retirees) instead
# of the SAME caller-filtered, dead-excluding `members` list every sibling
# check (clearance, connectivity) already uses -- verifier-authorized,
# one-line fix (self_verify's own docstring has the full rationale).

def test_self_verify_printability_ignores_dead_members(box_mesh, cfg):
    """TDD failing-first: forest.members (raw) still contains a DEAD,
    near-horizontal member (mark_dead never deletes -- tree.Forest's
    append-only protocol, so checkpoint/rollback indices stay stable) --
    exactly the shape rewire.py's degenerate-retire move leaves behind.
    A dead member is never emitted or printed (emission consumes the
    SAME filtered live set `members` is built from here), so gating on
    its own stale geometry checks something that will never actually
    print. `members` (the correctly-filtered list) excludes it entirely
    -- printability_violations must be empty."""
    f = tm.Forest()
    live_idx = f.add_member(_member([0.0, 0.0, 0.0], [0.0, 0.0, 10.0], kind="trunk"))
    dead_idx = f.add_member(_member([5.0, 0.0, 5.0], [5.0, 0.0, 5.0 + 1e-8], kind="shoot"))
    f.mark_dead(dead_idx)
    assert dead_idx in f.dead

    members = [f.members[live_idx]]  # mirrors engine.live_member_lists' own
    # dead-exclusion -- exactly what would actually be emitted/printed.

    report = verify.self_verify(box_mesh, [], [], members, np.array([]), np.array([]),
                                 cfg, 0.0, forest=f)
    assert report["printability_violations"] == []


def test_self_verify_printability_still_catches_a_live_violation(box_mesh, cfg):
    """The inverse guard: a LIVE near-horizontal member must still be
    caught -- this fix narrows the CHECKED SET to what's actually live,
    it must never weaken the rule itself. Passes both before and after
    the fix; pinned here so a future change can't silently break it
    while "fixing" the dead-member case above."""
    f = tm.Forest()
    bad_idx = f.add_member(_member([0.0, 0.0, 5.0], [0.0, 0.0, 5.0 + 1e-8], kind="shoot"))
    members = [f.members[bad_idx]]

    report = verify.self_verify(box_mesh, [], [], members, np.array([]), np.array([]),
                                 cfg, 0.0, forest=f)
    assert len(report["printability_violations"]) == 1
    assert report["printability_violations"][0]["kind"] == "shoot"
    assert report["printability_violations"][0]["reason"] == "near_horizontal"


def test_minima_report_requires_contact_at_birth_region_not_nearby(cfg):
    """Craig's rule: lateral proximity is NOT support. A contact 1.5mm
    away laterally (10+ layers above the island's birth on a shallow toe)
    must NOT satisfy the minimum; a contact touching the birth region
    must."""
    minima = [dict(z=9.0, position=[8.0, 0.0, 9.0],
                   region_pts=np.array([[8.0, 0.0, 9.0], [8.1, 0.0, 9.05]]))]
    far = [dict(position=np.array([8.0, 1.5, 10.2]))]
    at_min = [dict(position=np.array([8.05, 0.0, 9.02]))]
    assert len(verify.minima_report(minima, far, cfg)) == 1
    assert verify.minima_report(minima, at_min, cfg) == []
    no_contacts = verify.minima_report(minima, [], cfg)
    assert len(no_contacts) == 1 and no_contacts[0]["min_gap_mm"] == float("inf")
    assert verify.minima_report([], at_min, cfg) == []


# =================================================================
# v4 p6 (I3): the honest slice-based island-birth coverage gate
# =================================================================
def _square_loop(cx, cy, half=0.05):
    return np.array([[cx - half, cy - half], [cx + half, cy - half],
                      [cx + half, cy + half], [cx - half, cy + half]])


def test_island_birth_report_flags_uncovered_and_clears_when_on_footprint(cfg):
    """Gate test (charter TDD requirement): a birth with no qualifying
    routed contact must fail; a routed contact landing on the birth's own
    footprint within the ~2-layer z budget must clear it. Lateral
    proximity elsewhere (Craig's rule, same spirit as minima_report) does
    NOT count."""
    birth = dict(x=5.0, y=0.0, z=9.0, area_mm2=0.01, loop=_square_loop(5.0, 0.0))
    z_tol = cfg.minima_connect_layers * cfg.layer_height_mm

    far = [dict(position=np.array([5.0, 1.5, 9.0 + z_tol / 2]))]  # xy way off footprint
    late = [dict(position=np.array([5.0, 0.0, 9.0 + z_tol + 0.05]))]  # on xy, but too late
    on_birth = [dict(position=np.array([5.0, 0.0, 9.0 + z_tol / 2]))]  # on footprint, in budget

    assert len(verify.island_birth_report([birth], far, cfg)) == 1
    assert len(verify.island_birth_report([birth], late, cfg)) == 1
    assert verify.island_birth_report([birth], on_birth, cfg) == []
    assert verify.island_birth_report([birth], [], cfg)[0]["gap_mm"] == float("inf")
    assert verify.island_birth_report([], far, cfg) == []


def test_island_birth_report_honors_attach_radius_at_footprint_boundary(cfg):
    """A contact just outside the polygon but within
    perception.ISLAND_ATTACH_REACH_MM of its boundary still counts (the
    binding criterion's "or within that contact's own attach radius of
    its boundary" clause) -- one attach_reach past that, it does not."""
    from support_placement import perception
    r = perception.ISLAND_ATTACH_REACH_MM
    birth = dict(x=0.0, y=0.0, z=3.0, area_mm2=0.01, loop=_square_loop(0.0, 0.0))
    just_outside = [dict(position=np.array([0.05 + r * 0.5, 0.0, 3.0]))]
    too_far = [dict(position=np.array([0.05 + r * 3.0, 0.0, 3.0]))]
    assert verify.island_birth_report([birth], just_outside, cfg) == []
    assert len(verify.island_birth_report([birth], too_far, cfg)) == 1


def test_island_birth_report_hardcoded_boundary_catches_reach_widening(cfg):
    """Regression pin for perception.ISLAND_ATTACH_REACH_MM (adversarial-
    review blocker): every OTHER boundary test in this file derives its
    expected-covered/uncovered distance FROM perception.
    ISLAND_ATTACH_REACH_MM itself, so a future widening of that constant
    (e.g. someone "fixing a false uncovered report" by nudging it back
    toward the old, too-loose 1.0mm minima_contact_radius_mm) would sail
    through them unnoticed -- they'd just re-derive a new, looser
    expectation and keep passing. This test uses a HARDCODED literal xy
    distance instead, deliberately NOT read from the constant under test:
    the birth footprint's boundary sits at x=0.05 (_square_loop's default
    half-width), and a contact at x=0.40 is exactly 0.35mm outside it --
    farther than the correct 0.2mm reach, so it must be UNCOVERED. If
    ISLAND_ATTACH_REACH_MM were ever widened to 1.0mm, 0.35mm would fall
    back WITHIN reach and this assertion would fail, catching the
    regression by construction (verified directly: mutating the constant
    to 1.0 and re-running this test fails; restored byte-identical
    afterward -- see the p6-islands commit history for that proof run)."""
    birth = dict(x=0.0, y=0.0, z=5.0, area_mm2=0.01, loop=_square_loop(0.0, 0.0))
    contact = [dict(position=np.array([0.40, 0.0, 5.0]))]  # 0.35mm outside the x=0.05 boundary
    uncovered = verify.island_birth_report([birth], contact, cfg)
    assert len(uncovered) == 1, (
        "a contact 0.35mm outside the birth footprint must be UNCOVERED at "
        "the correct 0.2mm attach reach -- if this now passes as covered, "
        "perception.ISLAND_ATTACH_REACH_MM has silently widened")


def test_island_birth_report_z_tol_derives_from_layer_height_dynamically():
    """Charter amendment (design-v4.md section 11, Craig 2026-07-22:
    "islanding and other such features need to think about attachment in
    terms of much smaller distances" as layer height shrinks) --
    dynamic-layer-height compatibility pin: island_birth_report's z
    tolerance must be RE-DERIVED from cfg.layer_height_mm at call time,
    never baked in at the 0.05mm profile's current value.

    A fixed 0.06mm z-gap, xy-on-footprint contact: at the 0.05mm default
    (z_tol = minima_connect_layers(2.0) * layer_height_mm(0.05) = 0.10mm,
    0.06 <= 0.10) it is COVERED; at a finer 0.02mm profile (z_tol = 2.0 *
    0.02 = 0.04mm, 0.06 > 0.04) the SAME 0.06mm gap is now UNCOVERED --
    proving the tolerance tracks layer_height_mm rather than a pinned
    millimetre constant."""
    from support_placement import config as config_mod
    birth = dict(x=0.0, y=0.0, z=10.0, area_mm2=0.01, loop=_square_loop(0.0, 0.0))
    late_by_006mm = [dict(position=np.array([0.0, 0.0, 10.06]))]

    cfg_default = config_mod.SupportConfig(layer_height_mm=0.05)
    assert cfg_default.minima_connect_layers * cfg_default.layer_height_mm == pytest.approx(0.10)
    assert verify.island_birth_report([birth], late_by_006mm, cfg_default) == []

    cfg_fine = config_mod.SupportConfig(layer_height_mm=0.02)
    assert cfg_fine.minima_connect_layers * cfg_fine.layer_height_mm == pytest.approx(0.04)
    uncovered = verify.island_birth_report([birth], late_by_006mm, cfg_fine)
    assert len(uncovered) == 1, (
        "the same 0.06mm z-gap contact must become UNCOVERED at a finer "
        "layer_height_mm -- if this still reports covered, the z tolerance "
        "is pinned to the old profile instead of re-derived from cfg")


def test_self_verify_gates_on_uncovered_island_births(table_mesh, cfg):
    """self_verify's `ok` must go False when island_births reveals an
    uncovered birth, and stay unaffected (empty, non-gating) when the
    caller omits island_births entirely -- same optional-parameter
    contract as `minima` (backward compatible for any caller that hasn't
    adopted I1 yet)."""
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    pool = placement.candidate_pool(table_mesh, islands, cfg)
    contacts = placement.sample_candidates(
        table_mesh, islands, cfg, candidate_pool_prepared=pool)
    carried = placement.load_partition(table_mesh, islands, contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    members = []
    for ci, (c, r) in enumerate(zip(contacts, tip_r)):
        pos = np.asarray(c["position"], float)
        anchor = np.array([pos[0], pos[1], plate_z])
        shaft_r, _n_braces, _eff = placement.shaft_radius_for_length_mm(
            abs(pos[2] - anchor[2]), cfg)
        members.append(_m(pos, anchor, float(r), shaft_r, ("contact", ci), "trunk"))

    report_omitted = verify.self_verify(table_mesh, islands, contacts, members, carried, tip_r,
                                         cfg, plate_z,
                                         contact_candidate_pool=pool)
    assert report_omitted["uncovered_island_births"] == []
    assert report_omitted["regular_contact_invariant_violations"] == []

    far_birth = dict(x=1e6, y=1e6, z=1e6, area_mm2=0.01, loop=_square_loop(1e6, 1e6))
    report_gated = verify.self_verify(table_mesh, islands, contacts, members, carried, tip_r,
                                       cfg, plate_z, island_births=[far_birth],
                                       contact_candidate_pool=pool)
    assert report_gated["ok"] is False
    assert len(report_gated["uncovered_island_births"]) == 1


def test_self_verify_island_birth_clean_control_supplies_pool(cfg):
    import trimesh

    mesh = trimesh.Trimesh(
        vertices=np.array([
            [-0.1, -0.1, 0.0],
            [0.1, -0.1, 0.0],
            [0.0, 0.1, 0.0],
        ]),
        faces=np.array([[0, 1, 2]], dtype=np.int64),
        process=False)
    islands = [np.array([0], dtype=np.int64)]
    sidecar = topology.build_face_topology(mesh, cfg=cfg)
    samples = coverage.build_coverage_samples(mesh, islands, cfg)
    birth = dict(
        x=0.0, y=0.0, z=0.0, area_mm2=0.01,
        loop=_square_loop(0.0, 0.0))
    contact = dict(
        position=np.array([0.0, 0.0, 0.0]),
        normal=np.array([0.0, 0.0, -1.0]),
        island_id=0, minimum=True, island_birth=True,
        birth_loop=birth["loop"].copy(), birth_z=0.0,
        coverage_face_id=0, coverage_component_id=0)
    pool = placement.candidate_pool(mesh, islands, cfg)
    report = verify.self_verify(
        mesh, islands, [contact], [], np.array([]), np.array([]),
        cfg, plate_z=0.0, island_births=[birth],
        contact_candidate_pool=pool, provenance_contacts=[contact],
        coverage_samples=samples, face_topology=sidecar)
    assert report["uncovered_island_births"] == []
    assert report["regular_contact_invariant_violations"] == []
    assert report["ok"] is True


def test_connectivity_report_empty_inputs_returns_empty(cfg):
    assert verify.connectivity_report([], plate_z=0.0, cfg=cfg) == []


def test_connectivity_report_flags_floating_brace_endpoint(cfg):
    """v3.7 hard gate: a brace endpoint sitting in mid-air (nothing else's
    axis nearby, not at plate level, not embedded in any model) must be
    flagged -- this is the exact failure mode the stale-brace-leak defect
    produced (rods left pointing at trunks that had since moved/vanished)."""
    plate_z = -5.0
    a = _m([0.0, 0.0, plate_z], [0.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 0), "trunk")
    b = _m([10.0, 0.0, plate_z], [10.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 1), "trunk")
    good_brace = _m([0.0, 0.0, 4.0], [10.0, 0.0, 4.0], 0.5, 0.5, ("brace", 0), "brace")
    floating_brace = _m([0.0, 0.0, 6.0],      # still on trunk a's axis
                         [50.0, 50.0, 50.0],  # nowhere near anything
                         0.5, 0.5, ("brace", 1), "brace")
    violations = verify.connectivity_report([a, b, good_brace, floating_brace], plate_z, cfg)
    flagged = [tuple(v["endpoint"]) for v in violations]
    assert (50.0, 50.0, 50.0) in flagged
    assert (0.0, 0.0, 6.0) not in flagged  # on-axis end of the SAME rod is fine


def test_connectivity_report_brace_attached_to_trunk_axis_passes(cfg):
    """A rod whose endpoints sit on trunk column axes is attached (rule
    a) -- the ordinary trunk-to-trunk brace case."""
    plate_z = -5.0
    a = _m([0.0, 0.0, plate_z], [0.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 0), "trunk")
    b = _m([10.0, 0.0, plate_z], [10.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 1), "trunk")
    brace = _m([0.0, 0.0, 4.0], [10.0, 0.0, 4.0], 0.5, 0.5, ("brace", 0), "brace")
    violations = verify.connectivity_report([a, b, brace], plate_z, cfg)
    flagged = [tuple(v["endpoint"]) for v in violations]
    assert (0.0, 0.0, 4.0) not in flagged
    assert (10.0, 0.0, 4.0) not in flagged


def test_connectivity_report_model_rung_passes_with_embedded_end_and_surface_tree(cfg):
    """A trunk-to-model rung's model-facing end (embed_end="p1") is
    attached iff it is actually embedded in the model surface -- rule
    (c), and ONLY for the specific endpoint designed to embed."""
    plate_z = -5.0
    a = _m([0.0, 0.0, plate_z], [0.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 0), "trunk")
    model_pt = np.array([5.0, 0.0, 4.0])
    rung = _m([0.0, 0.0, 4.0],   # on trunk a's axis
              model_pt,           # model end
              cfg.brace_r_mm, cfg.brace_r_mm, ("brace", 0), "model_rung", embed_end="p1")
    surface_tree = cKDTree(np.array([model_pt]))
    violations = verify.connectivity_report([a, rung], plate_z, cfg,
                                             surface_tree=surface_tree)
    flagged = [tuple(v["endpoint"]) for v in violations]
    assert tuple(model_pt.tolist()) not in flagged
    assert (0.0, 0.0, 4.0) not in flagged
    # without a surface tree the embed rule can't fire -- the model end is
    # honestly reported as unattached rather than silently passed
    violations_no_tree = verify.connectivity_report([a, rung], plate_z, cfg)
    flagged_no_tree = [tuple(v["endpoint"]) for v in violations_no_tree]
    assert tuple(model_pt.tolist()) in flagged_no_tree


def test_orientation_scar_cost_prefers_hiding_the_delicate_fin(fin_mesh, cfg):
    delicacy = perception.delicacy_field(fin_mesh, cfg, n_samples=6000)
    identity = np.eye(3)
    flip_x = np.diag([1.0, -1.0, -1.0])  # flips which side faces "down"
    scores = verify.orientation_scar_cost(fin_mesh, cfg, [identity, flip_x], delicacy=delicacy)
    assert scores[0] != scores[1]
    assert min(scores) >= 0
