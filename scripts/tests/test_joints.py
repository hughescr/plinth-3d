import numpy as np
from support_placement import joints, routing, trunks as tm


def _route(p_tip, p_elbow, p_junction, r_tip=0.3, r_shoot=0.4, anchor="trunk",
           trunk_idx=0):
    stub = routing.Segment(np.asarray(p_tip, float), np.asarray(p_elbow, float),
                            r_tip, r_shoot, "stub", embedded_p0=True)
    shoot = routing.Segment(np.asarray(p_elbow, float), np.asarray(p_junction, float),
                             r_shoot, r_shoot, "shoot")
    rt = routing.Route(contact_idx=0, segments=[stub, shoot], anchor_kind=anchor,
                        free_length_mm=8.0)
    rt.trunk_idx = trunk_idx
    return rt


def _trunk(xy, z_top, r=1.2):
    t = tm.Trunk(xy, z_top)
    t.r = r
    return t


def test_junction_ball_sized_by_fattest_member_and_owned_by_trunk(cfg):
    """A shoot landing mid-column: the ball must be sized by the TRUNK
    (fattest member through the joint), even though only the shoot ENDS
    there -- the exact gapped-undersized-ball class the global pass
    kills."""
    trunk = _trunk([4.0, 0.0], z_top=20.0, r=1.2)
    route = _route([0, 0, 10], [0, 0, 12.5], [4.0, 0.0, 8.5])
    balls = joints.compute_joint_balls([route], [trunk], [], [], plate_z=-5.0, cfg=cfg)
    at_junction = [b for b in balls
                   if np.allclose(b["center"], [4.0, 0.0, 8.5], atol=1e-6)]
    assert len(at_junction) == 1
    assert abs(at_junction[0]["r"] - cfg.joint_ball_factor * 1.2) < 1e-9
    assert at_junction[0]["owner"] == ("trunk", 0)


def test_one_ball_per_joint_dedupes_shared_brace_endpoints(cfg):
    """Crossed X-pair rods share endpoints on the trunk axes; the old
    per-rod emission put 2+ spheres at each shared point."""
    ta = _trunk([0.0, 0.0], z_top=30.0, r=1.0)
    tb = _trunk([6.0, 0.0], z_top=30.0, r=1.0)
    shared = np.array([0.0, 0.0, 10.0])
    b1 = dict(i=0, j=1, r=0.5, p0=shared.copy(), p1=np.array([6.0, 0.0, 16.0]))
    b2 = dict(i=0, j=1, r=0.5, p0=np.array([6.0, 0.0, 10.0]), p1=shared.copy())
    balls = joints.compute_joint_balls([], [ta, tb], [], [b1, b2],
                                        plate_z=-5.0, cfg=cfg)
    at_shared = [b for b in balls if np.allclose(b["center"], shared, atol=1e-6)]
    assert len(at_shared) == 1
    # sized by the trunk the rods land on, not the rod radius
    assert abs(at_shared[0]["r"] - cfg.joint_ball_factor * 1.0) < 1e-9


def test_elbow_ball_present_tip_contact_not_a_joint(cfg):
    trunk = _trunk([4.0, 0.0], z_top=20.0, r=1.2)
    route = _route([0, 0, 10], [0, 0, 12.5], [4.0, 0.0, 8.5])
    balls = joints.compute_joint_balls([route], [trunk], [], [], plate_z=-5.0, cfg=cfg)
    assert any(np.allclose(b["center"], [0, 0, 12.5], atol=1e-6) for b in balls)
    assert not any(np.allclose(b["center"], [0, 0, 10.0], atol=1e-6) for b in balls)


def test_trunk_top_gets_dome_ball(cfg):
    trunk = _trunk([4.0, 0.0], z_top=20.0, r=1.2)
    balls = joints.compute_joint_balls([], [trunk], [], [], plate_z=-5.0, cfg=cfg)
    tops = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, 20.0], atol=1e-6)]
    assert len(tops) == 1 and abs(tops[0]["r"] - cfg.joint_ball_factor * 1.2) < 1e-9


def test_model_strut_foot_keeps_adhesion_factor(cfg):
    route = _route([0, 0, 10], [0, 0, 12.5], [3.0, 0.0, 9.0], anchor="model",
                   trunk_idx=None)
    balls = joints.compute_joint_balls([route], [], [], [], plate_z=-5.0, cfg=cfg)
    feet = [b for b in balls if np.allclose(b["center"], [3.0, 0.0, 9.0], atol=1e-6)]
    assert len(feet) == 1
    assert abs(feet[0]["r"] - joints.MODEL_FOOT_FACTOR * 0.4) < 1e-9


def test_hub_ball_matches_fattest_of_fans_and_riser(cfg):
    """Bundled fan shoots + shared riser meet at the hub: one ball,
    sized to the riser when the riser is the fattest member there
    (seventh-review finding, now owned by the global pass)."""
    hub = np.array([2.0, 0.0, 11.0])
    r1 = _route([0, 0, 12], [0.5, 0, 13.5], hub, r_shoot=0.4)
    r2 = _route([4, 0, 12], [3.5, 0, 13.5], hub, r_shoot=0.35)
    riser = dict(p0=hub.copy(), p1=np.array([6.0, 0.0, 7.0]), r=0.9, trunk_idx=0)
    balls = joints.compute_joint_balls([r1, r2], [], [riser], [], plate_z=-5.0, cfg=cfg)
    at_hub = [b for b in balls if np.allclose(b["center"], hub, atol=1e-6)]
    assert len(at_hub) == 1
    assert abs(at_hub[0]["r"] - cfg.joint_ball_factor * 0.9) < 1e-9
    assert at_hub[0]["owner"] == ("riser", 0)


def test_y_split_ball_sized_by_parent_column(cfg):
    parent = _trunk([0.0, 0.0], z_top=30.0, r=1.8)
    child = _trunk([5.0, 0.0], z_top=25.0, r=1.0)
    child.foot_kind = "branch"
    child.parent_idx = 0
    child.z_split = 10.0
    child.z_start = 15.0
    balls = joints.compute_joint_balls([], [parent, child], [], [],
                                        plate_z=-5.0, cfg=cfg)
    at_split = [b for b in balls if np.allclose(b["center"], [0, 0, 10.0], atol=1e-6)]
    assert len(at_split) == 1
    # fattest member through the split is the PARENT column (1.8), not
    # the child diagonal (1.0) the old per-member ball was sized by
    assert abs(at_split[0]["r"] - cfg.joint_ball_factor * 1.8) < 1e-9
