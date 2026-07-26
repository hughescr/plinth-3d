import os
import struct
import numpy as np
import pytest
import trimesh

from support_placement import chitubox_emit as ce
from support_placement import routing
from support_placement import chitubox_inject as ci

# Committed donor retained for the legacy clone-path integration test below.
# The modern from-empty path authors type-6 geometry directly and does not
# consult this file.
CORPUS_PATH = None
_candidate = os.path.join(os.path.dirname(__file__), "fixtures", "chitubox",
                          "pad_bridge_donor.chitubox")
if os.path.isfile(_candidate):
    CORPUS_PATH = os.path.abspath(_candidate)
TWO_SUPPORTS_PATH = os.path.abspath(os.path.join(
    os.path.dirname(__file__), "fixtures", "chitubox",
    "two-supports.chitubox"))


def _assert_closed_outward(tri):
    tri = np.asarray(tri, dtype=np.float64)
    normals = np.cross(
        tri[:, 1] - tri[:, 0], tri[:, 2] - tri[:, 0])
    assert np.all(np.linalg.norm(normals, axis=1) > 1e-12)
    mesh = trimesh.Trimesh(
        **trimesh.triangles.to_kwargs(tri), process=False)
    mesh.merge_vertices()
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    assert mesh.volume > 0


def test_frustum_cylinder_is_watertight_with_expected_volume():
    tri = ce.frustum_mesh([0, 0, 0], [0, 0, 10], 1.0, 1.0, n_sides=24)
    mesh = trimesh.Trimesh(vertices=tri.reshape(-1, 3), faces=np.arange(tri.shape[0] * 3).reshape(-1, 3))
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    expected_vol = np.pi * 1.0 ** 2 * 10.0
    assert abs(mesh.volume - expected_vol) / expected_vol < 0.02  # 24-gon discretization error
    assert mesh.volume > 0  # outward-consistent winding


def test_frustum_taper_volume_matches_frustum_formula():
    r0, r1, h = 0.5, 1.0, 5.0
    tri = ce.frustum_mesh([0, 0, 0], [0, 0, h], r0, r1, n_sides=24)
    mesh = trimesh.Trimesh(vertices=tri.reshape(-1, 3), faces=np.arange(tri.shape[0] * 3).reshape(-1, 3))
    assert mesh.is_watertight
    assert mesh.is_winding_consistent
    expected = (np.pi * h / 3) * (r0 ** 2 + r0 * r1 + r1 ** 2)
    assert abs(mesh.volume - expected) / expected < 0.02


def test_frustum_triangle_count_matches_validated_topology():
    tri = ce.frustum_mesh([0, 0, 0], [0, 0, 1], 0.5, 0.5, n_sides=24)
    assert tri.shape[0] == 4 * 24  # 2*sides walls + sides caps, both ends


@pytest.mark.parametrize("type_id,r0,r1", [
    (ci.T_TAPER, 0.3, 0.75),
    (ci.T_JOINER, 0.4, 0.4),
    (ci.T_FLARE, 1.1, 0.75),
])
def test_generated_frustum_segment_types_are_outward_wound(
        type_id, r0, r1, cfg):
    tri = ce.frustum_mesh(
        [0.0, 0.0, 0.0], [1.0, -0.5, 3.0], r0, r1)
    _assert_closed_outward(tri)
    raw = ce.build_record(
        type_id, [0.0, 0.0, 0.0], [1.0, -0.5, 3.0],
        r0, r1, mesh_off=0, mesh_len=tri.nbytes, cfg=cfg)
    assert ci.Record(raw).type == type_id


def test_generated_pad_and_bridge_are_outward_wound_and_plate_safe():
    plate_z = -7.25
    pad = ce._pad_mesh([1.0, 2.0], plate_z)
    bridge = ce._bridge_mesh([1.0, 2.0], [4.0, -1.0], plate_z)

    _assert_closed_outward(pad)
    _assert_closed_outward(bridge)
    assert pad[..., 2].min() == pytest.approx(plate_z)
    assert bridge[..., 2].min() == pytest.approx(plate_z)


def test_two_supports_donor_pins_native_segment_semantics():
    """UI-authored donor: contacts, joint spheres, rods, bosses, and raft."""
    project = ci.Project(TWO_SUPPORTS_PATH)
    assert [r.type for r in project.records] == [
        7, 1, 9, 6, 7, 3, 4, 3, 4, 9, 1, 6, 6]

    # A native contact's type 1 starts at the type-7 sphere midpoint and
    # contains a taper only. Native ordering is not universally adjacent:
    # the larger contact is records 4/10.
    for tip_i, taper_i, depth in ((0, 1, 0.1), (4, 10, 0.5)):
        tip = project.records[tip_i]
        taper = project.records[taper_i]
        center = (tip.start + tip.end) / 2.0
        radius = tip.r1
        assert tip.r2 == pytest.approx(radius)
        assert np.linalg.norm(tip.end - tip.start) == pytest.approx(
            2.0 * radius, abs=1e-6)
        assert taper.start == pytest.approx(center, abs=1e-6)
        assert bytes(tip.raw[48:72]) == bytes(24)
        assert struct.unpack_from("<f", taper.raw, 48)[0] == pytest.approx(
            depth)

        tip_pts = project.blob(tip)
        assert np.linalg.norm(tip_pts - center, axis=1) == pytest.approx(
            radius, abs=2e-6)
        taper_pts = project.blob(taper)
        axis = taper.end - taper.start
        length = np.linalg.norm(axis)
        axis_n = axis / length
        axial = (taper_pts - taper.start) @ axis_n
        assert np.all(
            np.isclose(axial, 0.0, atol=2e-6)
            | np.isclose(axial, length, atol=2e-6))

    # Native type 9 is a sphere-only joint record with vertical metadata
    # poles; ordinary column records are type 3.
    for joint_i in (2, 9):
        joint = project.records[joint_i]
        center = (joint.start + joint.end) / 2.0
        assert joint.start[:2] == pytest.approx(center[:2])
        assert joint.end[:2] == pytest.approx(center[:2])
        assert joint.start[2] > center[2] > joint.end[2]
        assert joint.r1 == pytest.approx(joint.r2)
        assert bytes(joint.raw[48:72]) == bytes(24)
        assert np.linalg.norm(
            project.blob(joint) - center, axis=1) == pytest.approx(
                joint.r1, abs=2e-6)

    # Each type-3 root column ends at a downward type-4 boss whose bottom is
    # the corresponding type-6 raft top, never the plate bottom.
    for column_i, boss_i, pad_i in ((5, 6, 3), (7, 8, 12)):
        column = project.records[column_i]
        boss = project.records[boss_i]
        pad = project.records[pad_i]
        assert column.type == ci.T_JOINER
        assert boss.type == ci.T_FLARE
        assert column.end == pytest.approx(boss.start, abs=0.0021)
        assert boss.start[2] > boss.end[2]
        assert boss.r1 == pytest.approx(column.r2, abs=1e-6)
        pad_mesh = project.blob(pad)
        assert boss.end[2] == pytest.approx(
            float(pad_mesh[:, 2].max()), abs=0.0021)
        assert float(pad_mesh[:, 2].max() - pad_mesh[:, 2].min()) == (
            pytest.approx(ce.RAFT_HEIGHT_MM, abs=2e-6))


def test_build_record_round_trips_through_chitubox_inject_Record():
    start, end = np.array([1.0, 2.0, 3.0]), np.array([1.0, 2.0, -3.0])
    raw = ce.build_record(ci.T_TIP, start, end, 0.4, 0.4, mesh_off=1000, mesh_len=17280, cfg=None)
    rec = ci.Record(raw)
    assert rec.magic == ci.SEG_MAGIC
    assert rec.type == ci.T_TIP
    assert np.allclose(rec.start, start)
    assert np.allclose(rec.end, end)
    assert abs(rec.r1 - 0.4) < 1e-6 and abs(rec.r2 - 0.4) < 1e-6
    assert rec.mesh_off == 1000 and rec.mesh_len == 17280


def test_build_record_taper_carries_documented_contact_depth(cfg):
    raw = ce.build_record(ci.T_TAPER, [0, 0, 0], [0, 0, -1], 0.3, 0.75, 0, 100, cfg)
    depth = struct.unpack_from("<f", raw, 48)[0]
    assert abs(depth - cfg.contact_depth_mm) < 1e-6


def test_build_record_tip_trailing_is_zero_matching_corpus_evidence(cfg):
    raw = ce.build_record(ci.T_TIP, [0, 0, 0], [0, 0, -1], 0.4, 0.4, 0, 100, cfg)
    assert raw[48:72] == bytes(24)


@pytest.mark.skipif(CORPUS_PATH is None, reason="calibration corpus not found for injection integration test")
def test_synthesized_support_injects_and_reparses_cleanly(cfg, tmp_path):
    proj = ci.Project(CORPUS_PATH)
    templates = ce.find_template_records(proj)
    pad_template = next(r for r in proj.records if r.type == ci.T_PAD and np.allclose(r.start[:2], r.end[:2]))

    contact = dict(position=np.array([0.0, 0.0, 50.0]), normal=np.array([0.0, 0.0, 1.0]))
    anchor = np.array([0.0, 0.0, 0.0])
    route = routing.route_to_anchor(contact, anchor, r_tip=0.35, r_shaft=0.6, cfg=cfg)

    next_off = proj.mesh_off + proj.mesh_size
    new_records = ce.synth_chain_records(route, r_tip=0.35, r_shaft=0.6, next_off=next_off, cfg=cfg)
    next_off += sum(len(b) for _, b in new_records)
    pad_rec, pad_blob = ce.place_pad(proj, pad_template, anchor[:2], next_off)
    new_records.append((pad_rec, pad_blob))

    before_n = proj.n_recs
    out_path = str(tmp_path / "synth_out.chitubox")
    proj.write(out_path, new_records)

    reparsed = ci.Project(out_path)
    assert reparsed.n_recs == before_n + len(new_records)
    groups, bridges = reparsed.supports()
    tip_positions = [g[0].start for g in groups if g[0].type == ci.T_TIP]
    assert any(np.linalg.norm(np.array(p) - (contact["position"] - [0, 0, cfg.contact_depth_mm])) < 0.5
               for p in tip_positions)


def test_sphere_mesh_is_watertight_with_sphere_volume(cfg):
    tri = ce.sphere_mesh([1.0, -2.0, 3.0], 0.8)
    import trimesh
    m = trimesh.Trimesh(**trimesh.triangles.to_kwargs(tri.astype(np.float64)))
    m.merge_vertices()
    assert m.is_watertight
    # faceted UV sphere: volume a few percent under 4/3 pi r^3
    exact = 4.0 / 3.0 * np.pi * 0.8 ** 3
    assert 0.90 * exact < m.volume <= exact


def _v3_route():
    from support_placement import routing as rt
    seg_stub = rt.Segment(np.array([0.0, 0.0, 10.0]), np.array([0.0, 0.0, 12.5]),
                           0.3, 0.4, "stub", embedded_p0=True)
    seg_shoot = rt.Segment(np.array([0.0, 0.0, 12.5]), np.array([4.0, 0.0, 8.5]),
                            0.4, 0.4, "shoot")
    return rt.Route(contact_idx=0, segments=[seg_stub, seg_shoot],
                     anchor_kind="trunk", free_length_mm=8.0)


def test_v3_support_chain_has_contact_ball_and_bare_rod(cfg):
    """v3.6: the contact ball stays (it is a contact, not a joint); the
    per-member elbow/junction balls are gone -- joint balls arrive only
    via extra_balls from the global joint pass."""
    route = _v3_route()
    recs = ce.synth_v3_support(route, r_tip=0.3, r_shoot=0.4, cfg=cfg, next_off=0)
    assert len(recs) == 2
    tip_blob = np.frombuffer(recs[0][1], "<f4").reshape(-1, 3)
    ball_r = cfg.tip_ball_r_factor * 0.3
    assert tip_blob[:, 2].min() <= 10.0 - 0.8 * ball_r
    # bare rod: nothing in the shoot blob may extend beyond the rod ends
    # by more than the rod radius (no embedded junction sphere)
    shoot_blob = np.frombuffer(recs[1][1], "<f4").reshape(-1, 3)
    assert shoot_blob[:, 2].min() >= 8.5 - 0.4 - 1e-4


def test_v3_support_chain_carries_global_joint_balls(cfg):
    route = _v3_route()
    junction = np.array([4.0, 0.0, 8.5])
    recs = ce.synth_v3_support(route, r_tip=0.3, r_shoot=0.4, cfg=cfg, next_off=0,
                                extra_balls=[(junction, 1.2)])
    shoot_blob = np.frombuffer(recs[1][1], "<f4").reshape(-1, 3)
    d = np.linalg.norm(shoot_blob - junction, axis=1)
    assert (d <= 1.2 + 1e-4).any()
    assert shoot_blob[:, 2].min() <= 8.5 - 0.8 * 1.2
    # record metadata still describes the rod, not the balls
    rec = ci.Record(recs[1][0])
    assert np.allclose(rec.end, junction)


def _mk_member(p0, p1, r0, r1, kind, owner, embed_end=None):
    return dict(p0=np.array(p0, float), p1=np.array(p1, float), r0=r0, r1=r1,
                kind=kind, owner=owner, embed_end=embed_end)


def test_synth_member_chain_round_trips_with_contiguous_offsets(cfg):
    members = [
        _mk_member([0, 0, 0], [0, 0, 2], 0.3, 0.4, "stub", ("contact", 0), embed_end="p0"),
        _mk_member([0, 0, 2], [0, 0, 10], 0.4, 0.4, "trunk", ("contact", 0)),
        _mk_member([5, 0, 2], [0, 0, 2], 0.3, 0.4, "shoot", ("contact", 1)),
    ]
    next_off = 1000
    recs = ce.synth_member_chain(members, [], cfg, next_off)
    assert len(recs) == len(members) + 1  # every stub is adjacent T_TIP + T_TAPER
    off = next_off
    for rec_bytes, blob in recs:
        rec = ci.Record(rec_bytes)
        assert rec.mesh_off == off
        assert rec.mesh_len == len(blob) == rec.mesh_len
        off += len(blob)
    # stub -> adjacent T_TIP + T_TAPER with contact-depth trailing;
    # trunk column (constant radius, no embed) -> native type-3 rod
    assert [ci.Record(r[0]).type for r in recs[:2]] == [ci.T_TIP, ci.T_TAPER]
    assert recs[0][0][48:72] == bytes(24)
    depth = struct.unpack_from("<f", recs[1][0], 48)[0]
    assert abs(depth - cfg.contact_depth_mm) < 1e-6
    trunk_rec = ci.Record(recs[2][0])
    assert trunk_rec.type == ci.T_JOINER


def test_synth_member_chain_stub_uses_native_contact_pair_geometry(cfg):
    p0 = np.array([1.0, -2.0, 3.0])
    p1 = np.array([4.0, 2.0, 15.0])
    r0, r1 = 0.3, 0.4
    members = [_mk_member(
        p0, p1, r0, r1, "stub", ("contact", 0), embed_end="p0")]
    recs = ce.synth_member_chain(members, [], cfg, 0)
    assert len(recs) == 2
    tip = ci.Record(recs[0][0])
    taper = ci.Record(recs[1][0])
    assert [tip.type, taper.type] == [ci.T_TIP, ci.T_TAPER]

    u = (p1 - p0) / np.linalg.norm(p1 - p0)
    ball_r = cfg.tip_ball_r_factor * r0
    ball_center = p0 + ball_r * u
    assert np.allclose(tip.start, p0)
    assert np.allclose(tip.end, p0 + 2.0 * ball_r * u)
    assert tip.r1 == pytest.approx(ball_r)
    assert tip.r2 == pytest.approx(ball_r)
    assert np.allclose(taper.start, ball_center)
    assert np.allclose(taper.end, p1)
    assert taper.r1 == pytest.approx(r0)
    assert taper.r2 == pytest.approx(r1)
    assert recs[0][0][48:72] == bytes(24)
    assert struct.unpack_from("<f", recs[1][0], 48)[0] == pytest.approx(
        cfg.contact_depth_mm)

    tip_mesh = np.frombuffer(recs[0][1], "<f4").reshape(-1, 3, 3)
    taper_mesh = np.frombuffer(recs[1][1], "<f4").reshape(-1, 3, 3)
    expected_tip = ce.sphere_mesh(ball_center, ball_r)
    expected_taper = ce.frustum_mesh(ball_center, p1, r0, r1)
    assert np.array_equal(tip_mesh, expected_tip)
    assert np.array_equal(taper_mesh, expected_taper)  # no sphere payload
    _assert_closed_outward(tip_mesh)
    _assert_closed_outward(taper_mesh)


def test_synth_member_chain_emits_each_joint_ball_exactly_once(cfg):
    members = [
        _mk_member([0, 0, 0], [0, 0, 2], 0.3, 0.4, "stub", ("contact", 0), embed_end="p0"),
        _mk_member([0, 0, 2], [0, 0, 10], 0.4, 0.4, "trunk", ("contact", 0)),
        _mk_member([5, 0, 2], [0, 0, 2], 0.3, 0.4, "shoot", ("contact", 1)),
    ]
    joint_center = np.array([0.0, 0.0, 2.0])  # shared endpoint of all 3 members
    ball_r = 0.6  # bigger than any member's own radius (max 0.4) at this joint
    balls = [dict(center=joint_center, r=ball_r, owner=("contact", 0))]
    recs = ce.synth_member_chain(members, balls, cfg, 0)
    # only the record actually carrying the merged sphere_mesh can have a
    # point out near the ball's own radius from the joint center -- every
    # member's own frustum geometry stays within its own (<=0.4) radius
    # NEAR the joint (a member's far end can be arbitrarily far away, so
    # restrict to points actually within the ball's zone before comparing).
    hits = 0
    for _, blob in recs:
        pts = np.frombuffer(blob, "<f4").reshape(-1, 3)
        d = np.linalg.norm(pts - joint_center, axis=1)
        near = d[d <= ball_r + 0.05]
        if len(near) and float(near.max()) >= 0.5:
            hits += 1
    assert hits == 1  # ball landed in exactly one record's blob
    total_verts = sum(len(np.frombuffer(b, "<f4")) // 3 for _, b in recs)
    assert total_verts > 0


def test_synth_member_chain_emits_standalone_type9_joint_sphere():
    from support_placement import config as cfg_mod
    cfg = cfg_mod.SupportConfig()
    members = [_mk_member([0, 0, 0], [0, 0, 2], 0.3, 0.3, "trunk", ("contact", 0))]
    center = np.array([50.0, 50.0, 50.0])
    radius = 0.5
    balls = [dict(center=center, r=radius, owner=("contact", 0))]
    recs = ce.synth_member_chain(members, balls, cfg, 0)
    assert [ci.Record(raw).type for raw, _ in recs] == [
        ci.T_JOINER, ci.T_SHAFT]
    joint = ci.Record(recs[1][0])
    assert joint.start == pytest.approx(center + [0.0, 0.0, radius])
    assert joint.end == pytest.approx(center - [0.0, 0.0, radius])
    assert joint.r1 == pytest.approx(radius)
    assert joint.r2 == pytest.approx(radius)
    assert recs[1][0][48:72] == bytes(24)
    assert np.array_equal(
        np.frombuffer(recs[1][1], "<f4").reshape(-1, 3, 3),
        ce.sphere_mesh(center, radius))
    _assert_closed_outward(
        np.frombuffer(recs[1][1], "<f4").reshape(-1, 3, 3))


# --- F2: conical feet on plate-rooted members --------------------------

def test_synth_member_chain_plate_root_gets_foot_flare(cfg):
    """v4 real root convention (tree.py): p1 is the root-ward end -- a
    member whose p1 sits at plate_z gets v3's conical foot flare."""
    members = [_mk_member([4.0, 0.0, 20.0], [4.0, 0.0, -5.0], 1.0, 1.0, "trunk", ("contact", 0))]
    recs = ce.synth_member_chain(members, [], cfg, 0, plate_z=-5.0)
    assert len(recs) == 2  # shortened type-3 column + native type-4 boss
    col_rec = ci.Record(recs[0][0])
    boss_rec = ci.Record(recs[1][0])
    assert col_rec.type == ci.T_JOINER
    assert boss_rec.type == ci.T_FLARE
    raft_top = -5.0 + ce.RAFT_HEIGHT_MM
    assert boss_rec.start[2] == pytest.approx(raft_top + cfg.flare_len_mm)
    assert boss_rec.end[2] == pytest.approx(raft_top)
    assert boss_rec.r1 == pytest.approx(1.0)
    assert boss_rec.r2 == pytest.approx(cfg.flare_r_factor * 1.0)
    assert col_rec.start == pytest.approx([4.0, 0.0, 20.0])
    assert col_rec.end == pytest.approx(boss_rec.start)
    flare_blob = np.frombuffer(recs[1][1], "<f4").reshape(-1, 3)
    at_raft_top = flare_blob[np.abs(flare_blob[:, 2] - raft_top) < 1e-4]
    rad = np.linalg.norm(
        at_raft_top[:, :2] - np.array([4.0, 0.0]), axis=1)
    assert rad.max() > 1.0 * 1.3  # wider than the column radius at raft top
    assert abs(rad.max() - cfg.flare_r_factor * 1.0) < 0.05
    _assert_closed_outward(
        np.frombuffer(recs[1][1], "<f4").reshape(-1, 3, 3))


def test_plate_root_pedestal_is_vertical_before_angled_member(cfg):
    plate_z = -5.0
    members = [_mk_member(
        [8.0, 3.0, -3.0], [4.0, 1.0, plate_z + 0.1],
        0.7, 0.9, "trunk", ("contact", 0))]
    specs = ce.member_chain_specs(members, [], cfg, plate_z=plate_z)
    angled, boss = specs
    assert angled.type_id == ci.T_JOINER
    assert boss.type_id == ci.T_FLARE
    assert boss.start[:2] == pytest.approx([4.0, 1.0])
    assert boss.end == pytest.approx(
        [4.0, 1.0, plate_z + ce.RAFT_HEIGHT_MM])
    assert boss.start[2] - plate_z >= (
        ce.MIN_ROOT_NO_BEND_MM - ce.PLATE_MESH_TOL_MM)
    assert angled.end == pytest.approx(boss.start)
    assert not np.allclose(angled.start[:2], angled.end[:2])
    at_raft_top = boss.mesh[
        np.abs(boss.mesh[..., 2] - boss.end[2]) <= ce.PLATE_MESH_TOL_MM]
    assert len(at_raft_top) > 0
    assert np.ptp(at_raft_top[:, 0]) > 0
    assert np.ptp(at_raft_top[:, 1]) > 0


def test_short_wide_angled_root_starts_above_safe_run(cfg):
    """The Thorny record-129 split must keep its angled mesh above plate."""
    plate_z = -11.057884216308594
    members = [_mk_member(
        [-3.44175197, 9.28385124, -10.40368197],
        [-3.40738346, 9.71617020, plate_z],
        0.8, 0.8, "trunk", ("contact", 4))]

    angled, boss = ce.member_chain_specs(
        members, [], cfg, plate_z=plate_z)
    raw_angled = ce.frustum_mesh(
        angled.start, angled.end, angled.r0, angled.r1)

    assert raw_angled[..., 2].min() < boss.end[2]
    assert angled.end == pytest.approx(boss.start)
    assert not np.allclose(angled.start[:2], angled.end[:2])
    min_vertical = max(
        ce.MIN_ROOT_NO_BEND_MM,
        ce.MIN_ROOT_VERTICAL_LAYERS * cfg.layer_height_mm)
    assert min(angled.start[2], angled.end[2]) >= (
        plate_z + min_vertical - ce.PLATE_MESH_TOL_MM)
    assert angled.mesh[..., 2].min() >= plate_z
    ce.validate_support_mesh_plate_bound(
        [angled, boss], plate_z)


def test_plate_root_rejects_less_than_ten_layers_before_bend(cfg):
    plate_z = -5.0
    too_short = ce.MIN_ROOT_VERTICAL_LAYERS * cfg.layer_height_mm - 0.01
    members = [_mk_member(
        [3.0, 0.0, plate_z + too_short], [0.0, 0.0, plate_z],
        0.5, 0.5, "trunk", ("contact", 0))]
    with pytest.raises(ce.PlateGeometryError, match="needs"):
        ce.member_chain_specs(members, [], cfg, plate_z=plate_z)


def test_synth_member_chain_no_flare_when_plate_z_omitted(cfg):
    """plate_z defaults to None: emits exactly as before -- the driver
    call site (manual_treant_v4.stage_emit) is unaffected."""
    members = [_mk_member([4.0, 0.0, 20.0], [4.0, 0.0, -5.0], 1.0, 1.0, "trunk", ("contact", 0))]
    recs = ce.synth_member_chain(members, [], cfg, 0)
    assert len(recs) == 1
    rec = ci.Record(recs[0][0])
    assert rec.type == ci.T_JOINER
    assert np.allclose(rec.end, [4.0, 0.0, -5.0])


def test_synth_member_chain_non_root_member_unaffected_by_plate_z(cfg):
    """A member whose p1 does NOT land at plate_z (a shoot, a mid-tree
    member) gets no flare even when plate_z is supplied."""
    members = [_mk_member([5, 0, 2], [0, 0, 2], 0.3, 0.4, "shoot", ("contact", 1))]
    recs = ce.synth_member_chain(members, [], cfg, 0, plate_z=-5.0)
    assert len(recs) == 1
    rec = ci.Record(recs[0][0])
    assert np.allclose(rec.start, [5, 0, 2]) and np.allclose(rec.end, [0, 0, 2])


def test_synth_member_chain_short_plate_root_flares_lower_half_only(cfg):
    """Member shorter than 2*flare_len_mm: flare the lower half only
    (recorded decision, SPEC-EMIT-POLISH.md F2)."""
    short_len = cfg.flare_len_mm * 1.2  # < 2*flare_len_mm
    members = [_mk_member([4.0, 0.0, -5.0 + short_len], [4.0, 0.0, -5.0],
                           1.0, 1.0, "trunk", ("contact", 0))]
    recs = ce.synth_member_chain(members, [], cfg, 0, plate_z=-5.0)
    assert len(recs) == 2
    col_rec = ci.Record(recs[0][0])
    boss_rec = ci.Record(recs[1][0])
    available_above_raft = short_len - ce.RAFT_HEIGHT_MM
    expected_boss_top_z = (
        -5.0 + ce.RAFT_HEIGHT_MM + available_above_raft / 2.0)
    assert col_rec.end[2] == pytest.approx(expected_boss_top_z)
    assert boss_rec.start[2] == pytest.approx(expected_boss_top_z)
    assert boss_rec.end[2] == pytest.approx(
        -5.0 + ce.RAFT_HEIGHT_MM)


def test_synth_member_chain_plate_root_still_emits_its_joint_balls(cfg):
    """A plate-rooted member's non-plate joint ball is a standalone type 9."""
    members = [_mk_member([4.0, 0.0, 20.0], [4.0, 0.0, -5.0], 1.0, 1.0, "trunk", ("contact", 0))]
    mid = np.array([4.0, 0.0, 10.0])
    balls = [dict(center=mid, r=1.3, owner=("contact", 0))]
    recs = ce.synth_member_chain(members, balls, cfg, 0, plate_z=-5.0)
    assert [ci.Record(raw).type for raw, _ in recs] == [
        ci.T_JOINER, ci.T_FLARE, ci.T_SHAFT]
    joint = ci.Record(recs[-1][0])
    assert joint.start == pytest.approx(mid + [0.0, 0.0, 1.3])
    assert joint.end == pytest.approx(mid - [0.0, 0.0, 1.3])


def test_bottom_connected_exact_native_geometry_and_bytes():
    plate_z = -7.25
    members = [
        _mk_member([0, 0, 2], [0, 0, plate_z], 0.25, 0.4,
                   "trunk", ("contact", 0)),
        _mk_member([4, 0, 2], [4, 0, plate_z], 1.2, 1.4,
                   "trunk", ("contact", 1)),
        # Duplicate XY with a different column radius: still one fixed pad.
        _mk_member([4, 0, 3], [4, 0, plate_z], 0.5, 0.8,
                   "trunk", ("contact", 2)),
    ]
    specs = ce.bottom_connected_specs(members, plate_z)
    assert len(specs) == 3  # 2 pads + 1 bridge
    bridge = specs[0]
    pads = specs[1:]
    assert bridge.type_id == ci.T_PAD
    assert bridge.mesh.shape == (12, 3, 3) and bridge.mesh.nbytes == 432
    assert bridge.r0 == pytest.approx(2.2) and bridge.r1 == pytest.approx(0.2)
    assert bridge.start[2] == 0 and bridge.end[2] == 0
    assert bridge.trailing == struct.pack("<2f", 1.0, 4.0) + bytes(16)
    direction = bridge.end[:2] - bridge.start[:2]
    along = direction / np.linalg.norm(direction)
    across = np.array([-along[1], along[0]])
    xy = bridge.mesh[..., :2].reshape(-1, 2)
    assert np.ptp(xy @ along) == pytest.approx(np.linalg.norm(direction))
    assert np.ptp(xy @ across) == pytest.approx(2.2)
    assert bridge.mesh[..., 2].min() == pytest.approx(plate_z)
    assert bridge.mesh[..., 2].max() == pytest.approx(plate_z + 0.2)
    for pad in pads:
        assert pad.mesh.shape == (80, 3, 3) and pad.mesh.nbytes == 2880
        assert pad.r0 == pytest.approx(2.2) and pad.r1 == pytest.approx(0.2)
        assert pad.start == pytest.approx(pad.end)
        assert pad.start[2] == 0
        assert pad.trailing == struct.pack("<2f", 0.0, 4.0) + bytes(16)
        pts = pad.mesh.reshape(-1, 3)
        assert np.ptp(pts[:, 0]) == pytest.approx(2.2)
        assert np.ptp(pts[:, 1]) == pytest.approx(2.2)
        assert np.ptp(pts[:, 2]) == pytest.approx(0.2)


def test_bottom_connected_mst_is_deterministic_and_plate_frame_does_not_leak():
    roots = [
        _mk_member([0, 0, 2], [0, 0, -5], 0.5, 0.5, "trunk", ("c", 0)),
        _mk_member([2, 0, 2], [2, 0, -5], 0.5, 0.5, "trunk", ("c", 1)),
        _mk_member([0, 2, 2], [0, 2, -5], 0.5, 0.5, "trunk", ("c", 2)),
        _mk_member([2, 2, 2], [2, 2, -5], 0.5, 0.5, "trunk", ("c", 3)),
    ]
    a = ce.bottom_connected_specs(roots, -5.0)
    b = ce.bottom_connected_specs(list(reversed(roots)), -5.0)
    edges_a = [(tuple(s.start), tuple(s.end)) for s in a
               if not np.allclose(s.start[:2], s.end[:2])]
    edges_b = [(tuple(s.start), tuple(s.end)) for s in b
               if not np.allclose(s.start[:2], s.end[:2])]
    assert edges_a == edges_b and len(edges_a) == 3

    # Same current-frame roots at another local plate plane must rebuild every
    # mesh at that plane; no cached/native absolute Z may leak between calls.
    roots_2 = [
        dict(m, p1=np.array([m["p1"][0], m["p1"][1], -12.0]))
        for m in roots
    ]
    c = ce.bottom_connected_specs(roots_2, -12.0)
    assert min(s.mesh[..., 2].min() for s in a) == pytest.approx(-5.0)
    assert min(s.mesh[..., 2].min() for s in c) == pytest.approx(-12.0)
    assert all(s.start[2] == 0 and s.end[2] == 0 for s in c)


def test_v3_trunk_has_flare_wider_than_column(cfg):
    from support_placement import trunks as tm
    t = tm.Trunk([2.0, 3.0], z_top=40.0)
    t.r = 1.0
    recs = ce.synth_v3_trunk(t, plate_z=-5.0, cfg=cfg, next_off=0)
    assert len(recs) == 2
    flare_blob = np.frombuffer(recs[0][1], "<f4").reshape(-1, 3)
    # at plate level the flare must be wider than the column radius
    at_plate = flare_blob[np.abs(flare_blob[:, 2] - (-5.0)) < 1e-4]
    rad = np.linalg.norm(at_plate[:, :2] - np.array([2.0, 3.0]), axis=1)
    assert rad.max() > t.r * 1.3
