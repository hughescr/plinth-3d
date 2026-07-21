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


def test_box_bottom_face_is_down_facing_but_grounded(box_mesh, cfg):
    down = perception.down_facing_mask(box_mesh, cfg)
    grounded = perception.grounded_mask(box_mesh, cfg, down)
    centers = box_mesh.triangles_center
    zmin = box_mesh.bounds[0][2]
    bottom = down & (centers[:, 2] - zmin < 0.1)
    assert bottom.sum() > 0
    assert grounded[bottom].all(), "box resting on its own base must be grounded, not an island"


def test_vertical_wall_is_not_down_facing(box_mesh, cfg):
    down = perception.down_facing_mask(box_mesh, cfg)
    normals = box_mesh.face_normals
    vertical = np.abs(normals[:, 2]) < 1e-6
    assert not down[vertical].any()


def test_table_underside_forms_islands_needing_support(table_mesh, cfg):
    islands = perception.overhang_islands(table_mesh, cfg)
    assert len(islands) >= 1
    total_faces = sum(len(i) for i in islands)
    assert total_faces > 0


def test_box_alone_has_no_islands(box_mesh, cfg):
    # a simple box resting on the plate needs no support: its only
    # down-facing face is its grounded base.
    islands = perception.overhang_islands(box_mesh, cfg)
    assert islands == []


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
