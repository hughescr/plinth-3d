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
    kills.

    The trunk's top sits only 0.8mm above the junction (< the ball's own
    1.2mm radius) so the sized ball genuinely pokes past the trunk's own
    end -- keeps this a KEPT ball under the enclosure diet (F5) while
    leaving the sizing/ownership computation (which only cares that the
    junction is on-axis and mid-span, not close to an end) unchanged.
    A trunk long enough to clear the ball on both sides is exactly the
    "fully enclosed, correctly dropped" case -- see
    test_mid_column_ball_fully_enclosed_by_fat_trunk_is_dropped."""
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 9.3], 1.2, 1.2, ("trunk", 0), "trunk")
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
    per-rod emission put 2+ spheres at each shared point.

    `ta`'s top sits only 0.7mm above the shared point (< the 1.0mm sized
    radius) so the ball genuinely protrudes past ta's own end and stays
    a KEPT ball under the enclosure diet (F5) -- a ta long enough to
    clear the ball on both sides is the "fully enclosed" case (see
    test_mid_column_ball_fully_enclosed_by_fat_trunk_is_dropped)."""
    ta = _m([0.0, 0.0, -5.0], [0.0, 0.0, 10.7], 1.0, 1.0, ("trunk", 0), "trunk")
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
    """`parent`'s top sits only 1.5mm above the split (< the 1.8mm sized
    radius) so the ball genuinely protrudes past parent's own end and
    stays a KEPT ball under the enclosure diet (F5) -- a parent long
    enough to clear the ball on both sides is the "fully enclosed" case
    (see test_mid_column_ball_fully_enclosed_by_fat_trunk_is_dropped)."""
    parent = _m([0.0, 0.0, -5.0], [0.0, 0.0, 11.5], 1.8, 1.8, ("trunk", 0), "trunk")
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


# --- F1: no joint balls at the plate ---------------------------------

def test_plate_rooted_column_base_gets_no_ball(cfg):
    """Craig: spheres half-sunk into the build plate at column bases."""
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 20.0], 1.2, 1.2, ("trunk", 0), "trunk")
    balls = joints.compute_joint_balls([trunk], plate_z=-5.0, cfg=cfg)
    at_base = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, -5.0], atol=1e-6)]
    assert len(at_base) == 0
    # the top dome ball is untouched
    at_top = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, 20.0], atol=1e-6)]
    assert len(at_top) == 1


def test_junction_5mm_above_plate_still_gets_a_ball(cfg):
    """trunk's top sits only 0.9mm above the junction (< the 1.2mm sized
    radius) so the ball genuinely protrudes past trunk's own top end and
    stays a KEPT ball under the enclosure diet (F5); the bottom end stays
    at plate_z (unaffected) so the plate-band story this test is actually
    about -- a junction 5mm above the plate, well outside PLATE_BAND_MM
    -- is untouched. A trunk long enough to clear the ball on both sides
    is the "fully enclosed" case (see
    test_mid_column_ball_fully_enclosed_by_fat_trunk_is_dropped)."""
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 0.9], 1.2, 1.2, ("trunk", 0), "trunk")
    shoot = _m([0.0, 0.0, 3.0], [4.0, 0.0, 0.0], 0.4, 0.4, ("route", 0), "shoot")
    balls = joints.compute_joint_balls([trunk, shoot], plate_z=-5.0, cfg=cfg)
    at_junction = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, 0.0], atol=1e-6)]
    assert len(at_junction) == 1


def test_plate_band_exclusion_applies_to_p1_end_too(cfg):
    """v4's real root convention lands a root's p1 (not p0) at plate_z --
    the exclusion must not be hardcoded to one end."""
    root = _m([4.0, 0.0, 20.0], [4.0, 0.0, -5.0], 1.2, 1.2, ("trunk", 0), "trunk")
    balls = joints.compute_joint_balls([root], plate_z=-5.0, cfg=cfg)
    at_base = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, -5.0], atol=1e-6)]
    assert len(at_base) == 0


def test_ball_sphere_may_not_cross_plate_plane_even_when_node_clears_band(cfg):
    """Adversarial-verifier specimen: the node-band exclusion tests only
    the NODE z against PLATE_BAND_MM, before the ball's radius is known.
    Real-data joint at [13.40, -7.33, -66.17], plate_z=-66.97575, sits
    0.806mm above the plate -- comfortably outside the 0.2mm node band --
    but sizes to r=0.8268, so its sphere BOTTOM (center_z - r) lands at
    -66.9968, which is 0.0211mm BELOW plate_z. No emitted ball's sphere
    geometry may cross the plate plane, node-band pass or not."""
    plate_z = -66.97575
    node = [13.40, -7.33, -66.17]
    r = 0.8268
    assert abs(node[2] - plate_z) > joints.PLATE_BAND_MM  # node-band alone would pass this
    assert node[2] - r < plate_z  # but the sized sphere's bottom crosses the plate
    trunk = _m([13.40, -7.33, -40.0], node, r, r, ("trunk", 0), "trunk")
    balls = joints.compute_joint_balls([trunk], plate_z=plate_z, cfg=cfg)
    at_node = [b for b in balls if np.allclose(b["center"], node, atol=1e-6)]
    assert len(at_node) == 0, "ball sphere crosses the plate plane -- must be dropped"
    for b in balls:
        assert b["center"][2] - b["r"] >= plate_z - 1e-9


# --- F3(a): colinear pass-throughs get no ball ------------------------

def test_colinear_pass_through_no_ball(cfg):
    """Two members meeting nearly head-on (anti-parallel through the
    point) with matching end radii is a straight continuation
    (bifurcation split / rewire seam) -- v3 merged these cosmetically."""
    a = _m([0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 0.5, 0.5, ("trunk", 0), "trunk")
    b = _m([0.0, 0.0, 10.0], [0.0, 0.0, 20.0], 0.52, 0.52, ("trunk", 1), "trunk")
    balls = joints.compute_joint_balls([a, b], plate_z=-100.0, cfg=cfg)
    at_seam = [ball for ball in balls if np.allclose(ball["center"], [0, 0, 10.0], atol=1e-6)]
    assert len(at_seam) == 0


def test_elbow_at_90deg_still_gets_a_ball_not_colinear(cfg):
    a = _m([0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 0.5, 0.5, ("trunk", 0), "trunk")
    b = _m([0.0, 0.0, 10.0], [10.0, 0.0, 10.0], 0.5, 0.5, ("trunk", 1), "trunk")
    balls = joints.compute_joint_balls([a, b], plate_z=-100.0, cfg=cfg)
    at_elbow = [ball for ball in balls if np.allclose(ball["center"], [0, 0, 10.0], atol=1e-6)]
    assert len(at_elbow) == 1


def test_colinear_but_mismatched_radii_still_gets_a_ball(cfg):
    """Anti-parallel through the point but end radii differ > 20% -- a
    genuine taper transition, not a cosmetic seam; keep the ball."""
    a = _m([0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 0.5, 0.5, ("trunk", 0), "trunk")
    b = _m([0.0, 0.0, 10.0], [0.0, 0.0, 20.0], 1.2, 1.2, ("trunk", 1), "trunk")
    balls = joints.compute_joint_balls([a, b], plate_z=-100.0, cfg=cfg)
    at_seam = [ball for ball in balls if np.allclose(ball["center"], [0, 0, 10.0], atol=1e-6)]
    assert len(at_seam) == 1


# --- F3(c): axis-match sizing uses the LOCAL (taper-interpolated)
# radius at the cluster point, not the member's overall max end radius --

def test_axis_match_sizes_by_local_radius_not_far_taper_end(cfg):
    """Reproduces the F3(b) forensics finding on the real snapshot: a
    tapered member's own thin end (0.35) IS the joint, but its far end
    (1.647, elsewhere on the same member) must not dominate the ball."""
    diag = _m([0.0, 0.0, 20.0], [-12.5, 21.65, 5.6], 0.35, 1.647,
               ("contact", 144), "trunk")
    balls = joints.compute_joint_balls([diag], plate_z=-100.0, cfg=cfg)
    at_thin_end = [b for b in balls if np.allclose(b["center"], [0.0, 0.0, 20.0], atol=1e-6)]
    assert len(at_thin_end) == 1
    assert abs(at_thin_end[0]["r"] - cfg.joint_ball_factor * 0.35) < 1e-9


# --- enclosure diet (F5): drop a joint ball whose whole sphere is
# provably already inside the union of its touching members' own
# frustums -- see joints.py module docstring for the volume-forensics
# basis (measured 72.5% of the Treant's joint-ball volume is this shape).

def test_mid_column_ball_fully_enclosed_by_fat_trunk_is_dropped(cfg):
    """The measured majority case: a thin brace's endpoint lands mid-span
    on a much fatter trunk, comfortably clear of the trunk's own ends
    (15mm/20mm clearance vs the trunk's own 2.0mm radius). The ball sizes
    to (and centers on the axis of) the trunk; with that much clearance
    on both sides, every sampled point on the sphere resolves inside the
    trunk's own frustum -- no protrusion to fill, so it's dropped."""
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 30.0], 2.0, 2.0, ("trunk", 0), "trunk")
    brace = _m([10.0, 0.0, 10.0], [4.0, 0.0, 10.0], 0.3, 0.3, ("brace", 0), "brace")
    balls = joints.compute_joint_balls([trunk, brace], plate_z=-100.0, cfg=cfg)
    at_joint = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, 10.0], atol=1e-6)]
    assert at_joint == []


def test_elbow_junction_ball_protrudes_and_is_kept(cfg):
    """The classic miter-gap case: two same-radius members meeting at a
    right angle. The joint ball fills the real void on the outside of
    the bend -- neither member's frustum reaches into that gap (each
    member's own flat end cap is exactly at the joint), so the enclosure
    diet must NOT drop it."""
    a = _m([0.0, 0.0, 0.0], [0.0, 0.0, 10.0], 0.6, 0.6, ("trunk", 0), "trunk")
    b = _m([0.0, 0.0, 10.0], [10.0, 0.0, 10.0], 0.6, 0.6, ("trunk", 1), "trunk")
    balls = joints.compute_joint_balls([a, b], plate_z=-100.0, cfg=cfg)
    at_elbow = [ball for ball in balls if np.allclose(ball["center"], [0, 0, 10.0], atol=1e-6)]
    assert len(at_elbow) == 1
    assert abs(at_elbow[0]["r"] - cfg.joint_ball_factor * 0.6) < 1e-9


def test_borderline_protrusion_over_tolerance_is_kept(cfg):
    """Conservatism pin: the trunk's top sits 0.9mm above the joint, just
    short of the ball's own 1.0mm radius -- a ~0.1mm protrusion, 5x
    ENCLOSURE_TOL_MM (0.02mm) and unambiguously real, but still small.
    The diet must only drop a ball when its ENTIRE surface resolves
    inside the frustum union; "close" is not "enclosed"."""
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 0.9], 1.0, 1.0, ("trunk", 0), "trunk")
    brace = _m([10.0, 0.0, 0.0], [4.0, 0.0, 0.0], 0.2, 0.2, ("brace", 0), "brace")
    balls = joints.compute_joint_balls([trunk, brace], plate_z=-100.0, cfg=cfg)
    at_joint = [b for b in balls if np.allclose(b["center"], [4.0, 0.0, 0.0], atol=1e-6)]
    assert len(at_joint) == 1
    assert abs(at_joint[0]["r"] - cfg.joint_ball_factor * 1.0) < 1e-9


def test_enclosure_filter_is_deterministic_across_repeated_calls(cfg):
    """Two independent calls on the same mixed (some dropped, some kept)
    input produce bit-identical ball lists -- the Fibonacci-sphere
    sampling (_fibonacci_sphere_dirs) is seedless/deterministic, so
    nothing here should ever vary call to call."""
    trunk = _m([4.0, 0.0, -5.0], [4.0, 0.0, 30.0], 2.0, 2.0, ("trunk", 0), "trunk")
    brace = _m([10.0, 0.0, 10.0], [4.0, 0.0, 10.0], 0.3, 0.3, ("brace", 0), "brace")
    elbow_a = _m([20.0, 0.0, 0.0], [20.0, 0.0, 10.0], 0.6, 0.6, ("trunk", 1), "trunk")
    elbow_b = _m([20.0, 0.0, 10.0], [30.0, 0.0, 10.0], 0.6, 0.6, ("trunk", 2), "trunk")
    members = [trunk, brace, elbow_a, elbow_b]
    balls_1 = joints.compute_joint_balls(members, plate_z=-100.0, cfg=cfg)
    balls_2 = joints.compute_joint_balls(members, plate_z=-100.0, cfg=cfg)
    assert len(balls_1) == len(balls_2)
    # sanity: this fixture really does mix a dropped case (the mid-column
    # brace/trunk joint) with a kept one (the elbow) -- a determinism
    # check over an all-dropped or all-kept list would prove less.
    assert not any(np.allclose(b["center"], [4.0, 0.0, 10.0], atol=1e-6) for b in balls_1)
    assert any(np.allclose(b["center"], [20.0, 0.0, 10.0], atol=1e-6) for b in balls_1)

    def _key(b):
        return (round(float(b["center"][0]), 9), round(float(b["center"][1]), 9),
                round(float(b["center"][2]), 9))

    for x, y in zip(sorted(balls_1, key=_key), sorted(balls_2, key=_key)):
        assert np.array_equal(x["center"], y["center"])
        assert x["r"] == y["r"]
        assert x["owner"] == y["owner"]
