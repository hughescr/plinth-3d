import os
import struct
import numpy as np
import pytest
import trimesh

from support_placement import chitubox_emit as ce
from support_placement import routing
import chitubox_inject as ci

CORPUS_PATH = None
_candidate = os.path.join(os.path.dirname(__file__), "..", "..", "..", "supported-treant.chitubox")
if os.path.isfile(_candidate):
    CORPUS_PATH = os.path.abspath(_candidate)


def test_frustum_cylinder_is_watertight_with_expected_volume():
    tri = ce.frustum_mesh([0, 0, 0], [0, 0, 10], 1.0, 1.0, n_sides=24)
    mesh = trimesh.Trimesh(vertices=tri.reshape(-1, 3), faces=np.arange(tri.shape[0] * 3).reshape(-1, 3))
    assert mesh.is_watertight
    expected_vol = np.pi * 1.0 ** 2 * 10.0
    assert abs(mesh.volume - expected_vol) / expected_vol < 0.02  # 24-gon discretization error
    assert mesh.volume > 0  # outward-consistent winding


def test_frustum_taper_volume_matches_frustum_formula():
    r0, r1, h = 0.5, 1.0, 5.0
    tri = ce.frustum_mesh([0, 0, 0], [0, 0, h], r0, r1, n_sides=24)
    mesh = trimesh.Trimesh(vertices=tri.reshape(-1, 3), faces=np.arange(tri.shape[0] * 3).reshape(-1, 3))
    assert mesh.is_watertight
    expected = (np.pi * h / 3) * (r0 ** 2 + r0 * r1 + r1 ** 2)
    assert abs(mesh.volume - expected) / expected < 0.02


def test_frustum_triangle_count_matches_validated_topology():
    tri = ce.frustum_mesh([0, 0, 0], [0, 0, 1], 0.5, 0.5, n_sides=24)
    assert tri.shape[0] == 4 * 24  # 2*sides walls + sides caps, both ends


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
