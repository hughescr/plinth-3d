import dataclasses

import numpy as np
from support_placement import perception


def test_down_facing_mask_flags_flat_underside_of_table(table_mesh, cfg):
    down = perception.down_facing_mask(table_mesh, cfg)
    centers = table_mesh.triangles_center
    zmin, zmax = table_mesh.bounds[0][2], table_mesh.bounds[1][2]
    # underside of the slab is near the top of the model, flat, facing -z
    slab_underside = down & (centers[:, 2] > zmax - 6)
    assert slab_underside.sum() > 0
    # the box's own bottom cap (sitting on the plate) should NOT show up
    # as a down-facing OVERHANG requirement source once grounded-filtered,
    # but down_facing_mask alone doesn't know about plate proximity --
    # that's grounded_mask's job (tested separately).


def test_box_bottom_face_is_grounded_when_model_is_on_plate(box_mesh, cfg):
    down = perception.down_facing_mask(box_mesh, cfg)
    plate_z = float(box_mesh.bounds[0][2])
    grounded = perception.grounded_mask(box_mesh, cfg, plate_z, down)
    centers = box_mesh.triangles_center
    zmin = box_mesh.bounds[0][2]
    bottom = down & (centers[:, 2] - zmin < 0.1)
    assert bottom.sum() > 0
    assert grounded[bottom].all()


def test_lifted_box_bottom_face_is_not_grounded(box_mesh, cfg):
    down = perception.down_facing_mask(box_mesh, cfg)
    plate_z = float(box_mesh.bounds[0][2]) - cfg.lift_mm
    grounded = perception.grounded_mask(box_mesh, cfg, plate_z, down)
    centers = box_mesh.triangles_center
    bottom = down & np.isclose(centers[:, 2], box_mesh.bounds[0][2])
    assert bottom.sum() > 0
    assert not grounded[bottom].any()


def test_bridge_shadow_still_grounds_a_face_above_nearby_material(cfg):
    import trimesh

    lower = trimesh.creation.box(extents=(10.0, 10.0, 1.0))
    lower.apply_translation((0.0, 0.0, 0.5))
    upper = trimesh.creation.box(extents=(10.0, 10.0, 1.0))
    upper.apply_translation((0.0, 0.0, 2.0))
    mesh = trimesh.util.concatenate((lower, upper))

    down = perception.down_facing_mask(mesh, cfg)
    grounded = perception.grounded_mask(mesh, cfg, plate_z=-5.0,
                                         down_mask=down)
    upper_bottom = down & np.isclose(mesh.triangles_center[:, 2], 1.5)
    assert upper_bottom.sum() > 0
    assert grounded[upper_bottom].all()


def test_vertical_wall_is_not_down_facing(box_mesh, cfg):
    down = perception.down_facing_mask(box_mesh, cfg)
    normals = box_mesh.face_normals
    vertical = np.abs(normals[:, 2]) < 1e-6
    assert not down[vertical].any()


def test_table_underside_forms_islands_needing_support(table_mesh, cfg):
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(table_mesh, cfg, plate_z)
    assert len(islands) >= 1
    total_faces = sum(len(i) for i in islands)
    assert total_faces > 0


def test_box_on_plate_has_no_overhang_islands(box_mesh, cfg):
    on_plate = dataclasses.replace(cfg, lift_mm=0.0)
    plate_z = float(box_mesh.bounds[0][2]) - on_plate.lift_mm
    islands = perception.overhang_islands(box_mesh, on_plate, plate_z)
    assert islands == []


def test_lifted_box_bottom_is_an_overhang_island(box_mesh, cfg):
    plate_z = float(box_mesh.bounds[0][2]) - cfg.lift_mm
    islands = perception.overhang_islands(box_mesh, cfg, plate_z)
    bottom = set(np.flatnonzero(np.isclose(
        box_mesh.triangles_center[:, 2], box_mesh.bounds[0][2])))
    demanded = set(np.concatenate(islands))
    assert bottom <= demanded


class _FakeMesh:
    """Duck-typed stand-in exposing only what down_facing_mask reads --
    trimesh's face_normals is a derived, read-only property, so a real
    Trimesh can't have its normal forced directly for a single-triangle
    probe of the threshold formula itself."""
    def __init__(self, normals):
        self.face_normals = np.asarray(normals, dtype=float)


def test_overhang_angle_threshold_matches_vertical_lean_convention(cfg):
    # a face tilted 30deg from vertical (lean < 45deg threshold) should be
    # self-supporting; tilted 60deg from vertical should need support.
    cfg.overhang_angle_deg = 45.0
    for lean_deg, expect_down in [(30.0, False), (60.0, True)]:
        nz = -np.sin(np.radians(lean_deg))
        n = np.array([[np.sqrt(1 - nz**2), 0.0, nz]])
        down = perception.down_facing_mask(_FakeMesh(n), cfg)
        assert bool(down[0]) == expect_down, f"lean {lean_deg} expected down={expect_down}"


def test_local_z_minima_finds_both_toe_apexes(toed_slab_mesh, cfg):
    minima = perception.local_z_minima(toed_slab_mesh, cfg)
    zs = np.array([m["z"] for m in minima])
    # steep toe apex (z=9): flanks ~76deg from horizontal are INVISIBLE to
    # the overhang-angle island model -- the minima rule must still see it
    assert np.any(np.abs(zs - 9.0) < 1e-3), zs
    # shallow toe apex (z=11)
    assert np.any(np.abs(zs - 11.0) < 1e-3), zs
    for m in minima:
        # birth regions stay inside the connection band above their minimum
        assert m["region_pts"][:, 2].max() <= m["z"] \
            + cfg.minima_connect_layers * cfg.layer_height_mm + 1e-9


def test_local_z_minima_no_near_plate_exemption(box_mesh, cfg):
    # Under lift the model's own lowest point is the FIRST island of the
    # whole print: a flat-bottomed box is one bottom-plateau minimum, one
    # region (the tie-flood merges the whole plateau), honestly reported.
    minima = perception.local_z_minima(box_mesh, cfg)
    assert len(minima) == 1
    assert abs(minima[0]["z"] - 0.0) < 1e-6


def test_local_z_minima_flags_only_true_minima_on_tilted_box(cfg):
    import trimesh
    from support_placement import mesh_io
    b = mesh_io.make_box(size=(20, 20, 4), translate=(0, 0, 10))
    b.apply_transform(trimesh.transformations.rotation_matrix(
        np.radians(5.0), (0, 1, 0), point=[0, 0, 12]))
    b = trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(), process=True)
    minima = perception.local_z_minima(b, cfg)
    # only the lowest edge of the tilted bottom face is a birth minimum
    # (its two corners tie through the shared edge); the higher corners
    # have lower neighbors and must NOT be flagged
    assert len(minima) == 1
    zs = b.vertices[:, 2]
    assert abs(minima[0]["z"] - zs.min()) < 1e-6


# =================================================================
# v4 p6 (I1): slice-based island-birth detection, Craig's strict
# criterion
# =================================================================
def test_island_births_matches_single_bottom_plateau_on_box(box_mesh, cfg):
    """Sanity/cross-check against local_z_minima's own box test: a flat
    box under lift has exactly one island-birth event (its own bottom
    cross-section, the very first slice with nothing below it), matching
    local_z_minima's single bottom-plateau minimum."""
    births = perception.island_births(box_mesh, cfg)
    assert len(births) == 1
    assert abs(births[0]["z"] - (box_mesh.bounds[0][2] + cfg.layer_height_mm / 2)) < 1e-6


def test_island_births_reveals_raking_ladder_local_z_minima_misses(raking_blade_mesh, cfg):
    """TDD failing-first case (fails on master's local_z_minima-only
    detector): a thin blade raking far from vertical is topologically ONE
    connected minimum (a single lowest vertex) -- local_z_minima correctly
    finds just that one apex and stops, since its vertex-graph flood
    connects the whole blade as one continuous surface. But the blade's
    real-print-layer cross-section migrates sideways faster than its own
    width, so consecutive real layers genuinely do NOT overlap: this is
    Craig's "lump floating in the resin tank" case, and island_births
    (I1's ground-truth slice scan) must see a whole ladder of distinct
    birth events climbing the blade that the vertex-graph model cannot
    express as multiple minima."""
    minima = perception.local_z_minima(raking_blade_mesh, cfg)
    births = perception.island_births(raking_blade_mesh, cfg)
    assert len(minima) <= 2, (
        "local_z_minima should see the blade as (at most) its own apex "
        f"plus one unrelated slab-geometry minimum, not a ladder: {minima}")
    assert len(births) >= 5, (
        f"island_births should reveal a real ladder of >=5 birth events "
        f"climbing the raking blade, found {len(births)}")
    assert len(births) > len(minima) + 3


def test_island_births_deterministic(raking_blade_mesh, cfg):
    """Charter requirement: no RNG anywhere in the slice-based detector --
    two independent calls on the same mesh must be bit-identical (count,
    xyz, area, and the raw loop geometry)."""
    a = perception.island_births(raking_blade_mesh, cfg)
    b = perception.island_births(raking_blade_mesh, cfg)
    assert len(a) == len(b)
    for ba, bb in zip(a, b):
        assert ba["x"] == bb["x"] and ba["y"] == bb["y"] and ba["z"] == bb["z"]
        assert ba["area_mm2"] == bb["area_mm2"]
        assert np.array_equal(ba["loop"], bb["loop"])


def test_island_attach_reach_mm_is_pinned():
    """Regression pin (adversarial-review blocker): perception.
    ISLAND_ATTACH_REACH_MM must stay at 0.2mm -- config.py's own
    tip_r_min_mm, the smallest tip this engine ever builds, so the strict
    at-birth coverage gate's xy tolerance never assumes a bigger tip than
    is actually guaranteed to exist. This constant is read internally by
    engine._ensure_minima_contacts, engine._ensure_island_birth_contacts,
    and verify.island_birth_report -- every one of their existing tests
    that touches "the tolerance" derives its own expectation FROM this
    constant, so widening it (e.g. back toward the old, too-loose 1.0mm
    minima_contact_radius_mm) would sail through those tests unnoticed.
    This direct value assertion, plus
    test_verify.test_island_birth_report_hardcoded_boundary_catches_reach_widening
    (a HARDCODED literal distance, deliberately not derived from this
    constant), together close that gap."""
    assert perception.ISLAND_ATTACH_REACH_MM == 0.2


def test_island_birth_xy_ok_inside_polygon_and_within_boundary_tolerance():
    poly = np.array([[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]])
    ok, d = perception.island_birth_xy_ok(1.0, 1.0, poly, tol_mm=0.2)
    assert ok and d == 0.0
    ok, d = perception.island_birth_xy_ok(2.1, 1.0, poly, tol_mm=0.2)
    assert ok and abs(d - 0.1) < 1e-9
    ok, d = perception.island_birth_xy_ok(3.0, 1.0, poly, tol_mm=0.2)
    assert not ok and abs(d - 1.0) < 1e-9


def test_delicacy_field_scores_thin_fin_higher_than_base(fin_mesh, cfg):
    points, normals, w, query = perception.delicacy_field(fin_mesh, cfg, n_samples=6000)
    fin_pts_mask = points[:, 2] > 6.0  # fin sits above the 4mm-tall base
    base_pts_mask = points[:, 2] < 3.0
    assert fin_pts_mask.sum() > 20 and base_pts_mask.sum() > 20
    assert w[fin_pts_mask].mean() > w[base_pts_mask].mean(), \
        "thin fin should score more delicate than the bulky base"


def test_delicacy_query_matches_precomputed_weights(box_mesh, cfg):
    points, normals, w, query = perception.delicacy_field(box_mesh, cfg, n_samples=2000)
    got = query(points[:5])
    assert np.allclose(got, w[:5])


# =================================================================
# Filter (B) -- island-birth + ADEQUACY (Craig's refinement ruling,
# 2026-07-24). Nothing exercised minima_require_island_birth or
# _filter_minima_to_island_births before these, which is exactly why
# two special-case branches drifted from the stated predicate.
# =================================================================
def _raw_minima(mesh, cfg):
    """The candidate set filter (B) is handed: local_z_minima with BOTH
    optional filters off."""
    c = dataclasses.replace(cfg, minima_require_island_birth=False,
                            minima_drop_interior_faces=False)
    return perception.local_z_minima(mesh, c)


def _unrefined_filter_B(mesh, minima, births, cfg):
    """The UNREFINED filter (B) exactly as it behaved before the adequacy
    refinement: a minimum survives iff it coincides with a real island
    birth, and NOTHING survives when there are no births at all.

    Expressed here as the refined filter with a constant-True adequacy
    predicate -- which is precisely what threshold 0.0 must reduce to. Any
    branch that decides adequacy without consulting the threshold (the two
    that were found and removed: the empty-births early return, and
    treating an unmeasurable section as inadequate) makes the refined
    filter at 0.0 disagree with this oracle."""
    from unittest import mock
    with mock.patch.object(
            perception, "_minima_support_adequate",
            lambda mesh, ms, cfg: np.ones(len(ms), dtype=bool)):
        return perception._filter_minima_to_island_births(
            mesh, minima, births, cfg)


def _probe(mesh, xyz):
    """A single synthetic filter-(B) candidate at xyz, shaped like the
    dicts local_z_minima emits."""
    p = np.asarray(xyz, dtype=float)
    return [dict(position=p.copy(), z=float(p[2]),
                 normal=np.array([0.0, 0.0, -1.0]),
                 region_pts=p.reshape(1, 3).copy())]


def test_filter_B_defaults_are_on_and_pinned(cfg):
    """Both halves of filter (B) ship ON, at one tip footprint."""
    assert cfg.minima_require_island_birth is True
    assert cfg.minima_support_area_tips == 1.0
    assert abs(np.pi * cfg.tip_r_min_mm ** 2 - 0.12566370614359174) < 1e-12


def test_filter_B_threshold_zero_reproduces_unrefined_filter(table_mesh, cfg):
    """0.0 == the old filter, on a REAL mesh where it actually drops
    something (5 candidates, 4 births, 1 dropped)."""
    raw = _raw_minima(table_mesh, cfg)
    births = perception.island_births(table_mesh, cfg)
    assert len(raw) == 5 and len(births) == 4
    c0 = dataclasses.replace(cfg, minima_support_area_tips=0.0)
    kept = perception._filter_minima_to_island_births(
        table_mesh, raw, births, c0)
    old = _unrefined_filter_B(table_mesh, raw, births, c0)
    assert len(kept) == 4, len(kept)
    assert [m["z"] for m in kept] == [m["z"] for m in old]
    assert len(raw) - len(kept) == 1, "must actually drop one, else vacuous"


def test_filter_B_threshold_zero_reproduces_unrefined_filter_with_no_births(
        table_mesh, cfg):
    """The degenerate empty-births case the removed early return got wrong:
    at 0.0 every candidate is a non-birth AND adequate, so all of them go --
    the old filter's `return []`."""
    raw = _raw_minima(table_mesh, cfg)
    assert len(raw) == 5, "fixture must supply candidates, else vacuous"
    c0 = dataclasses.replace(cfg, minima_support_area_tips=0.0)
    kept = perception._filter_minima_to_island_births(table_mesh, raw, [], c0)
    old = _unrefined_filter_B(table_mesh, raw, [], c0)
    assert kept == [] and old == []


def test_filter_B_no_births_is_not_a_free_pass_at_the_default(box_mesh, cfg):
    """Empty births must not become a blanket rescue either: a candidate
    standing on 100 mm2 of solid is adequately carried and is dropped at the
    default just as it is at 0.0."""
    mid = _probe(box_mesh, [0.0, 0.0, 5.0])
    assert perception._minima_support_adequate(box_mesh, mid, cfg).tolist() \
        == [True]
    assert perception._filter_minima_to_island_births(
        box_mesh, mid, [], cfg) == []
    c0 = dataclasses.replace(cfg, minima_support_area_tips=0.0)
    assert perception._filter_minima_to_island_births(
        box_mesh, mid, [], c0) == []


def test_filter_B_default_keeps_candidate_on_sub_tip_footprint_sliver(cfg):
    """The defect the refinement exists to fix: a candidate resting on a
    0.0625 mm2 post -- half a support tip's own 0.1257 mm2 footprint -- is
    NOT carried, so the default keeps it while the old filter dropped it."""
    from support_placement import mesh_io
    post = mesh_io.make_box(size=(0.25, 0.25, 6.0))
    cand = _probe(post, [0.0, 0.0, 3.0])
    # brackets the measured carried area between 0.4x and 0.6x a tip
    # footprint (0.0503-0.0754 mm2), i.e. genuinely sub-footprint.
    for tips, expect in [(0.4, True), (0.6, False), (1.0, False), (0.0, True)]:
        c = dataclasses.replace(cfg, minima_support_area_tips=tips)
        assert perception._minima_support_adequate(
            post, cand, c).tolist() == [expect], tips
    assert len(perception._filter_minima_to_island_births(
        post, cand, [], cfg)) == 1, "sliver-borne candidate must be KEPT"
    c0 = dataclasses.replace(cfg, minima_support_area_tips=0.0)
    assert perception._filter_minima_to_island_births(
        post, cand, [], c0) == [], "old behaviour at 0.0: dropped"


def test_filter_B_unmeasurable_section_kept_at_default_dropped_at_zero(
        box_mesh, cfg):
    """A probe one layer below the model's own floor has no section at all.
    That yields carried area 0.0 and the ONE comparison governs: kept at the
    default (inconclusive -> keep demanding support), dropped at 0.0 (the
    old filter's unconditional drop)."""
    zmin = float(box_mesh.bounds[0][2])
    cand = _probe(box_mesh, [0.0, 0.0, zmin])
    assert box_mesh.section(plane_origin=[0, 0, zmin - cfg.layer_height_mm],
                            plane_normal=[0, 0, 1]) is None, \
        "fixture must genuinely have no section there, else vacuous"
    assert perception._minima_support_adequate(
        box_mesh, cand, cfg).tolist() == [False]
    assert len(perception._filter_minima_to_island_births(
        box_mesh, cand, [], cfg)) == 1
    c0 = dataclasses.replace(cfg, minima_support_area_tips=0.0)
    assert perception._minima_support_adequate(
        box_mesh, cand, c0).tolist() == [True]
    assert perception._filter_minima_to_island_births(
        box_mesh, cand, [], c0) == []


def test_filter_B_default_rescues_sites_the_unrefined_filter_dropped(
        table_mesh, toed_slab_mesh, cfg):
    """End-to-end through local_z_minima on two real fixtures: the refined
    default keeps strictly more than the unrefined filter, and the rescued
    sites are exactly the non-births that are not adequately carried."""
    for mesh, n_raw, n_old in [(table_mesh, 5, 4), (toed_slab_mesh, 3, 2)]:
        raw = _raw_minima(mesh, cfg)
        births = perception.island_births(mesh, cfg)
        assert len(raw) == n_raw
        c0 = dataclasses.replace(cfg, minima_support_area_tips=0.0)
        old = perception._filter_minima_to_island_births(mesh, raw, births, c0)
        new = perception._filter_minima_to_island_births(mesh, raw, births, cfg)
        assert len(old) == n_old
        assert len(new) == n_raw
        assert {id(m) for m in old} < {id(m) for m in new}
