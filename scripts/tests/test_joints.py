import numpy as np
from support_placement import joints


def _m(p0, p1, r0, r1, owner, kind, embed_end=None):
    """v4 member dict (design-v4.md #2 schema) -- test fixtures build
    these directly instead of Route/Trunk objects; migrated mechanically
    from the v3 fixtures (same geometry), assertions unchanged."""
    return dict(p0=np.asarray(p0, float), p1=np.asarray(p1, float),
                r0=r0, r1=r1, owner=owner, kind=kind, embed_end=embed_end)


def test_junction_ball_sized_by_fattest_member_and_owned_by_trunk(cfg):
    """A shoot landing mid-column: the ball must be sized by the TRUNK
    (fattest member through the joint), even though only the shoot ENDS
    there -- the exact gapped-undersized-ball class the global pass
    kills."""
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 20.0], 1.2, 1.2, ("trunk", 0), "trunk")
    stub = _m([0, 0, 10], [0, 0, 12.5], 0.3, 0.4, ("route", 0), "stub", embed_end="p0")
    shoot = _m([0, 0, 12.5], [4.0, 0.0, 8.5], 0.4, 0.4, ("route", 0), "shoot")
    balls = joints.compute_joint_balls([trunk, stub, shoot], plate_z=-5.0, cfg=cfg)
    at_junction = [b for b in balls
                   if np.allclose(b["center"], [4.0, 0.0, 8.5], atol=1e-6)]
    assert len(at_junction) == 1
    assert abs(at_junction[0]["r"] - cfg.joint_ball_factor * 1.2) < 1e-9
    assert at_junction[0]["owner"] == ("trunk", 0)


def test_one_ball_per_joint_dedupes_shared_brace_endpoints(cfg):
    """Crossed X-pair rods share endpoints on the trunk axes; the old
    per-rod emission put 2+ spheres at each shared point."""
    ta = _m([0.0, 0.0, -5.0], [0.0, 0.0, 30.0], 1.0, 1.0, ("trunk", 0), "trunk")
    tb = _m([6.0, 0.0, -5.0], [6.0, 0.0, 30.0], 1.0, 1.0, ("trunk", 1), "trunk")
    shared = np.array([0.0, 0.0, 10.0])
    b1 = _m(shared.copy(), [6.0, 0.0, 16.0], 0.5, 0.5, ("brace", 0), "brace")
    b2 = _m([6.0, 0.0, 10.0], shared.copy(), 0.5, 0.5, ("brace", 1), "brace")
    balls = joints.compute_joint_balls([ta, tb, b1, b2], plate_z=-5.0, cfg=cfg)
    at_shared = [b for b in balls if np.allclose(b["center"], shared, atol=1e-6)]
    assert len(at_shared) == 1
    # sized by the trunk the rods land on, not the rod radius
    assert abs(at_shared[0]["r"] - cfg.joint_ball_factor * 1.0) < 1e-9


def test_elbow_ball_present_tip_contact_not_a_joint(cfg):
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 20.0], 1.2, 1.2, ("trunk", 0), "trunk")
    stub = _m([0, 0, 10], [0, 0, 12.5], 0.3, 0.4, ("route", 0), "stub", embed_end="p0")
    shoot = _m([0, 0, 12.5], [4.0, 0.0, 8.5], 0.4, 0.4, ("route", 0), "shoot")
    balls = joints.compute_joint_balls([trunk, stub, shoot], plate_z=-5.0, cfg=cfg)
    assert any(np.allclose(b["center"], [0, 0, 12.5], atol=1e-6) for b in balls)
    assert not any(np.allclose(b["center"], [0, 0, 10.0], atol=1e-6) for b in balls)


def test_trunk_top_gets_dome_ball(cfg):
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 20.0], 1.2, 1.2, ("trunk", 0), "trunk")
    balls = joints.compute_joint_balls([trunk], plate_z=-5.0, cfg=cfg)
    tops = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, 20.0], atol=1e-6)]
    assert len(tops) == 1 and abs(tops[0]["r"] - cfg.joint_ball_factor * 1.2) < 1e-9


def test_model_strut_foot_keeps_adhesion_factor(cfg):
    stub = _m([0, 0, 10], [0, 0, 12.5], 0.3, 0.4, ("route", 0), "stub", embed_end="p0")
    strut = _m([0, 0, 12.5], [3.0, 0.0, 9.0], 0.4, 0.4, ("route", 0), "strut", embed_end="p1")
    balls = joints.compute_joint_balls([stub, strut], plate_z=-5.0, cfg=cfg)
    feet = [b for b in balls if np.allclose(b["center"], [3.0, 0.0, 9.0], atol=1e-6)]
    assert len(feet) == 1
    assert abs(feet[0]["r"] - joints.MODEL_FOOT_FACTOR * 0.4) < 1e-9


def test_hub_ball_matches_fattest_of_fans_and_riser(cfg):
    """Bundled fan shoots + shared riser meet at the hub: one ball,
    sized to the riser when the riser is the fattest member there."""
    hub = np.array([2.0, 0.0, 11.0])
    stub1 = _m([0, 0, 12], [0.5, 0, 13.5], 0.3, 0.4, ("route", 0), "stub", embed_end="p0")
    fan1 = _m([0.5, 0, 13.5], hub, 0.4, 0.4, ("route", 0), "shoot")
    stub2 = _m([4, 0, 12], [3.5, 0, 13.5], 0.3, 0.35, ("route", 1), "stub", embed_end="p0")
    fan2 = _m([3.5, 0, 13.5], hub, 0.35, 0.35, ("route", 1), "shoot")
    riser = _m(hub.copy(), [6.0, 0.0, 7.0], 0.9, 0.9, ("riser", 0), "riser")
    balls = joints.compute_joint_balls([stub1, fan1, stub2, fan2, riser],
                                        plate_z=-5.0, cfg=cfg)
    at_hub = [b for b in balls if np.allclose(b["center"], hub, atol=1e-6)]
    assert len(at_hub) == 1
    assert abs(at_hub[0]["r"] - cfg.joint_ball_factor * 0.9) < 1e-9
    assert at_hub[0]["owner"] == ("riser", 0)


def test_y_split_ball_sized_by_parent_column(cfg):
    parent = _m([0.0, 0.0, -5.0], [0.0, 0.0, 30.0], 1.8, 1.8, ("trunk", 0), "trunk")
    y_diag = _m([0.0, 0.0, 10.0], [5.0, 0.0, 15.0], 1.0, 1.0, ("trunk", 1), "y_diagonal")
    child = _m([5.0, 0.0, 15.0], [5.0, 0.0, 25.0], 1.0, 1.0, ("trunk", 1), "trunk")
    balls = joints.compute_joint_balls([parent, y_diag, child], plate_z=-5.0, cfg=cfg)
    at_split = [b for b in balls if np.allclose(b["center"], [0, 0, 10.0], atol=1e-6)]
    assert len(at_split) == 1
    # fattest member through the split is the PARENT column (1.8), not
    # the child diagonal (1.0) the old per-member ball was sized by
    assert abs(at_split[0]["r"] - cfg.joint_ball_factor * 1.8) < 1e-9


def test_collect_members_normalizes_dicts_and_defaults_embed_end():
    out = joints.collect_members([dict(p0=[0, 0, 0], p1=[0, 0, 1], r0=0.3, r1=0.3,
                                        owner=("contact", 0), kind="stub")])
    assert out[0]["embed_end"] is None
    assert isinstance(out[0]["p0"], np.ndarray) and out[0]["p0"].shape == (3,)


def test_collect_members_accepts_member_objects():
    from support_placement import tree as tm
    m = tm.Member(p0=[0, 0, 0], p1=[0, 0, -5], r0=0.3, r1=0.4,
                   kind="trunk", owner=("contact", 0), embed_end=None)
    out = joints.collect_members([m])
    assert out[0]["kind"] == "trunk"
    assert out[0]["r1"] == 0.4
