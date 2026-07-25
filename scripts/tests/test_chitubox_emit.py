import os
import struct
import numpy as np
import pytest
import trimesh

from support_placement import chitubox_emit as ce
from support_placement import routing
from support_placement import chitubox_inject as ci

# Committed donor: a minimal single-model project carrying one real PAD and
# one real BRIDGE record. Pads/bridges are the only record types this module
# cannot author from scratch (see chitubox_emit's docstring), so the injection
# test needs genuine ones to clone. Replaces a 65MB out-of-repo file that was
# consulted for exactly these two records.
CORPUS_PATH = None
_candidate = os.path.join(os.path.dirname(__file__), "fixtures", "chitubox",
                          "pad_bridge_donor.chitubox")
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
    assert len(recs) == len(members)
    off = next_off
    for rec_bytes, blob in recs:
        rec = ci.Record(rec_bytes)
        assert rec.mesh_off == off
        assert rec.mesh_len == len(blob) == rec.mesh_len
        off += len(blob)
    # stub (tapered + embed_end) -> T_TAPER with contact-depth trailing;
    # trunk column (constant radius, no embed) -> T_SHAFT
    stub_rec = ci.Record(recs[0][0])
    assert stub_rec.type == ci.T_TAPER
    depth = struct.unpack_from("<f", recs[0][0], 48)[0]
    assert abs(depth - cfg.contact_depth_mm) < 1e-6
    trunk_rec = ci.Record(recs[1][0])
    assert trunk_rec.type == ci.T_SHAFT


def test_synth_member_chain_stub_gets_contact_ball(cfg):
    members = [_mk_member([0, 0, 0], [0, 0, 2], 0.3, 0.4, "stub", ("contact", 0), embed_end="p0")]
    recs = ce.synth_member_chain(members, [], cfg, 0)
    blob = np.frombuffer(recs[0][1], "<f4").reshape(-1, 3)
    ball_r = cfg.tip_ball_r_factor * 0.3
    assert blob[:, 2].min() <= 0.0 - 0.8 * ball_r


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


def test_synth_member_chain_asserts_unassignable_ball():
    from support_placement import config as cfg_mod
    cfg = cfg_mod.SupportConfig()
    members = [_mk_member([0, 0, 0], [0, 0, 2], 0.3, 0.3, "trunk", ("contact", 0))]
    stray = [dict(center=np.array([50.0, 50.0, 50.0]), r=0.5, owner=("contact", 0))]
    with pytest.raises(AssertionError):
        ce.synth_member_chain(members, stray, cfg, 0)


# --- F2: conical feet on plate-rooted members --------------------------

def test_synth_member_chain_plate_root_gets_foot_flare(cfg):
    """v4 real root convention (tree.py): p1 is the root-ward end -- a
    member whose p1 sits at plate_z gets v3's conical foot flare."""
    members = [_mk_member([4.0, 0.0, 20.0], [4.0, 0.0, -5.0], 1.0, 1.0, "trunk", ("contact", 0))]
    recs = ce.synth_member_chain(members, [], cfg, 0, plate_z=-5.0)
    assert len(recs) == 2  # flare record + shortened column record
    flare_rec = ci.Record(recs[0][0])
    assert flare_rec.type == ci.T_FLARE
    flare_blob = np.frombuffer(recs[0][1], "<f4").reshape(-1, 3)
    at_plate = flare_blob[np.abs(flare_blob[:, 2] - (-5.0)) < 1e-4]
    rad = np.linalg.norm(at_plate[:, :2] - np.array([4.0, 0.0]), axis=1)
    assert rad.max() > 1.0 * 1.3  # wider than the column radius at the plate
    assert abs(rad.max() - cfg.flare_r_factor * 1.0) < 0.05
    col_rec = ci.Record(recs[1][0])
    assert col_rec.type == ci.T_SHAFT
    assert abs(float(col_rec.start[2]) - (-5.0 + cfg.flare_len_mm)) < 1e-6


def test_synth_member_chain_no_flare_when_plate_z_omitted(cfg):
    """plate_z defaults to None: emits exactly as before -- the driver
    call site (manual_treant_v4.stage_emit) is unaffected."""
    members = [_mk_member([4.0, 0.0, 20.0], [4.0, 0.0, -5.0], 1.0, 1.0, "trunk", ("contact", 0))]
    recs = ce.synth_member_chain(members, [], cfg, 0)
    assert len(recs) == 1
    rec = ci.Record(recs[0][0])
    assert rec.type == ci.T_SHAFT
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
    flare_rec = ci.Record(recs[0][0])
    expected_col0_z = -5.0 + short_len / 2.0
    assert abs(float(flare_rec.end[2]) - expected_col0_z) < 1e-6
    col_rec = ci.Record(recs[1][0])
    assert abs(float(col_rec.start[2]) - expected_col0_z) < 1e-6


def test_synth_member_chain_plate_root_still_carries_its_joint_balls(cfg):
    """A plate-rooted member's non-plate joint balls (e.g. a mid-column
    junction, since F1 already excludes the true plate-band node from
    ever being produced) still land in exactly one record's blob."""
    members = [_mk_member([4.0, 0.0, 20.0], [4.0, 0.0, -5.0], 1.0, 1.0, "trunk", ("contact", 0))]
    mid = np.array([4.0, 0.0, 10.0])
    balls = [dict(center=mid, r=1.3, owner=("contact", 0))]
    recs = ce.synth_member_chain(members, balls, cfg, 0, plate_z=-5.0)
    hits = 0
    for _, blob in recs:
        pts = np.frombuffer(blob, "<f4").reshape(-1, 3)
        d = np.linalg.norm(pts - mid, axis=1)
        near = d[d <= 1.3 + 0.05]
        if len(near) and float(near.max()) >= 0.9:
            hits += 1
    assert hits == 1


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
