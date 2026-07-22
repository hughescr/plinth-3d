import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import tree as tm


def _member(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("contact", 0), embed_end=None):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner, embed_end=embed_end)


# ---------------------------------------------------------------- Member --
def test_fresh_member_starts_dirty():
    m = _member([0, 0, 0], [0, 0, -10])
    assert not m.cert_valid


def test_certificate_valid_after_matching_geom_version():
    m = _member([0, 0, 0], [0, 0, -10])
    m.cert_version = m.geom_version
    m.cert_radius = max(m.r0, m.r1)
    assert m.cert_valid


def test_radius_increase_invalidates_certificate():
    m = _member([0, 0, 0], [0, 0, -10], r0=0.5, r1=0.5)
    m.cert_version = m.geom_version
    m.cert_radius = 0.5
    assert m.cert_valid
    m.r0 = 0.8
    assert not m.cert_valid


def test_radius_decrease_keeps_certificate():
    """design-v4.md #4: 'radius DECREASE with unchanged geometry keeps
    the certificate (checked fat covers thin)'."""
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0)
    m.cert_version = m.geom_version
    m.cert_radius = 1.0
    m.r0 = 0.4
    m.r1 = 0.4
    assert m.cert_valid


def test_position_change_invalidates_certificate_even_with_unchanged_radius():
    """The escape hatch is radius-only: a geometry change must void the
    cert even though the (unchanged) radius alone would satisfy the
    max(r0,r1) <= cert_radius clause."""
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0)
    m.cert_version = m.geom_version
    m.cert_radius = 1.0
    m.p1 = np.array([5.0, 0.0, -10.0])
    assert not m.cert_valid


def test_position_change_then_reduced_radius_still_invalid():
    """Guards the specific failure mode the two-field (geom_version,
    cert_radius) model must not fall into: a position change followed by
    ANY radius value must not be validated by the decrease escape hatch."""
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0)
    m.cert_version = m.geom_version
    m.cert_radius = 1.0
    m.p0 = np.array([2.0, 0.0, 0.0])
    m.r0 = 0.1
    assert not m.cert_valid


# ---------------------------------------------------------------- Forest --
def _far_tree():
    return cKDTree(np.array([[1e6, 1e6, 1e6]]))


def test_forest_add_member_starts_as_root_and_dirty():
    f = tm.Forest()
    idx = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    assert idx in f.roots
    assert idx in f.dirty_members()


def test_forest_set_parent_updates_topology():
    f = tm.Forest()
    a = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    b = f.add_member(_member([5, 0, 0], [0, 0, 0]))
    f.set_parent(b, a)
    assert f.parent[b] == a
    assert b in f.children[a]
    assert b not in f.roots
    assert a in f.roots


def test_revalidate_certifies_clear_members(cfg):
    f = tm.Forest()
    f.add_member(_member([0, 0, 0], [0, 0, -10], r0=0.3, r1=0.3))
    violations = f.revalidate(_far_tree(), cfg)
    assert violations == []
    assert f.dirty_members() == []


def test_revalidate_reports_violation_and_leaves_member_dirty(cfg):
    f = tm.Forest()
    close_tree = cKDTree(np.array([[0.0, 0.0, -5.0]]))
    f.add_member(_member([0, 0, 0], [0, 0, -10], r0=0.3, r1=0.3))
    violations = f.revalidate(close_tree, cfg)
    assert len(violations) == 1
    assert f.dirty_members() != []


def _dense_line(p_a, p_b, t_lo, t_hi, n=40, lateral_offset=None):
    """Dense cluster of points along the segment p_a->p_b between
    fractions t_lo/t_hi (inclusive), optionally shifted by a fixed
    lateral offset -- robust to Forest.revalidate's own
    clearance_sample_step_mm grid quantization (a single point can
    otherwise miss the nearest actual sample by up to half a step)."""
    p_a = np.asarray(p_a, float)
    p_b = np.asarray(p_b, float)
    ts = np.linspace(t_lo, t_hi, n)
    pts = p_a[None, :] + ts[:, None] * (p_b - p_a)[None, :]
    if lateral_offset is not None:
        pts = pts + np.asarray(lateral_offset, float)[None, :]
    return pts


def _dense_background_cloud(n_side=150, spacing_mm=0.3, offset=(1.0e5, 1.0e5, 1.0e5)):
    """A large, uniformly-spaced (deterministic, no RNG -- a plain grid,
    not a random sample), FAR-AWAY point cloud standing in for "the rest
    of a real mesh's surface_tree" in a masking test. A single bare test
    point (or two) makes routing.surface_tree_pitch_bound_mm measure that
    test's OWN incidental point spacing (often many mm, since test points
    are placed for geometric convenience, not surface density) instead of
    anything representative -- this cloud dilutes a test's own handful of
    specific points to a negligible (<1%) fraction of the tree, the same
    way a real ~hundred-thousand-point mesh cloud dwarfs any one strut's
    local geometry in production. Every point in a regular grid has
    nearest-neighbor distance EXACTLY `spacing_mm` (its immediate row/
    column neighbor, grid corners included), so this cloud's own
    contribution to ANY percentile is the single deterministic value
    spacing_mm -- 0.3mm here, matching production's own measured p99
    (table_mesh 0.359mm, toed_slab_mesh 0.360mm, real Weeping Treant
    0.351mm at the default cfg.surface_pitch_bound_percentile=99) closely
    enough to be representative while staying a clean, hand-verifiable
    number. Offset far from any test geometry (1e5mm) so cKDTree's
    distance_upper_bound pruning skips it entirely in every clearance
    query -- it can only affect the density MEASUREMENT, never a
    clearance DISTANCE."""
    idx = np.arange(n_side, dtype=float)
    gx, gy = np.meshgrid(idx, idx)
    pts = np.stack([gx.ravel(), gy.ravel(), np.zeros(gx.size)], axis=1) * spacing_mm
    return pts + np.asarray(offset, float)


def _pitch_bound_mm(cfg):
    """The pitch_bound_mm Forest.revalidate will itself measure for any
    tree built as `_dense_background_cloud()` unioned with a test's own
    (much smaller) point set -- computed via the REAL production function
    (routing.surface_tree_pitch_bound_mm) against the background alone,
    not hand-asserted, so a masking test's boundary placement is derived
    from the same formula it's testing (per the original charter
    instruction: "make the formula the same one your failing test
    computes independently"), now extended to this second term."""
    from support_placement import routing
    return routing.surface_tree_pitch_bound_mm(
        cKDTree(_dense_background_cloud()), percentile=cfg.surface_pitch_bound_percentile)


# --------------------------------------- BUG 2: embed-end exemption (p6) --
@pytest.mark.parametrize("r_end", [0.35, 0.5, 0.8])
def test_revalidate_passes_on_surface_anchored_strut_with_nothing_else_nearby(cfg, r_end):
    """MASKING TEST (a): an on-surface-anchored strut (r 0.35-0.8) with
    nothing else nearby must PASS revalidate. FAILS ON MASTER (the old
    fixed contact_depth_mm+0.2=0.5mm inset is radius-independent -- its
    own worst-case bound, inset+sample_step-r-standoff, is negative for
    every r >= 0.35mm at these cfg defaults). Includes a realistic
    background cloud (production-representative pitch_bound_mm ~0.3mm,
    Finding B) rather than a bare single point -- this is provably still
    the HARDER case, not an easier one: a nonzero pitch_bound_mm only
    ever WIDENS the exemption zone relative to the degenerate
    single-point (pitch_bound_mm=0.0) tree, so passing here implies
    passing with an even larger, more forgiving bound too."""
    anchor = np.array([0.0, 0.0, 0.0])   # p1 -- the designed on-surface foot
    se = np.array([8.0, 0.0, 6.0])       # p0 -- ~10mm strut, otherwise clear
    m = _member(se, anchor, r0=0.3, r1=r_end, kind="strut",
                owner=("contact", 0), embed_end="p1")
    f = tm.Forest()
    f.add_member(m)
    surface_tree = cKDTree(np.vstack([[anchor], _dense_background_cloud()]))
    violations = f.revalidate(surface_tree, cfg)
    assert violations == []
    assert f.dirty_members() == []


@pytest.mark.parametrize("r_end", [0.35, 0.5, 0.8])
def test_revalidate_passes_on_surface_anchored_model_rung_with_nothing_else_nearby(cfg, r_end):
    """MASKING TEST (a), model_rung variant: same guarantee for the OTHER
    embed_end='p1' kind the charter names (bracing.py's model-rung
    foot)."""
    anchor = np.array([0.0, 0.0, 0.0])
    p0 = np.array([3.0, 0.0, 0.0])       # short rod, like a real model_rung
    m = _member(p0, anchor, r0=r_end, r1=r_end, kind="model_rung",
                owner=("model_rung", 0), embed_end="p1")
    f = tm.Forest()
    f.add_member(m)
    surface_tree = cKDTree(np.vstack([[anchor], _dense_background_cloud()]))
    violations = f.revalidate(surface_tree, cfg)
    assert violations == []


def test_revalidate_catches_midshaft_clip_just_outside_exemption_zone(cfg):
    """MASKING TEST (b): a member whose MID-SHAFT passes illegally close
    to the model, just past the new exemption boundary (contact_depth_mm
    + pitch_bound_mm + r_end + standoff_mm, Finding B's completed
    derivation) and far from the embedded end otherwise, must still be
    caught -- the exemption may grow only as far as the anchor geometry
    AND the measured cloud density require, never further."""
    anchor = np.array([0.0, 0.0, 0.0])
    se = np.array([8.0, 0.0, 6.0])
    r_end = 0.5
    m = _member(se, anchor, r0=0.3, r1=r_end, kind="strut",
                owner=("contact", 0), embed_end="p1")
    pitch_bound = _pitch_bound_mm(cfg)
    inset = cfg.contact_depth_mm + pitch_bound + r_end + cfg.standoff_mm
    length = float(np.linalg.norm(se - anchor))
    # dead-on the centerline, starting just past the exemption boundary
    t_clip_lo = 1.0 - (inset + 1.5) / length
    t_clip_hi = 1.0 - (inset + 0.05) / length
    clip_cloud = _dense_line(se, anchor, t_clip_lo, t_clip_hi)
    f = tm.Forest()
    f.add_member(m)
    surface_tree = cKDTree(np.vstack([[anchor], clip_cloud, _dense_background_cloud()]))
    violations = f.revalidate(surface_tree, cfg)
    assert len(violations) == 1
    assert violations[0]["member_idx"] == 0


@pytest.mark.parametrize("offset_dir", [
    np.array([1.0, 0.0, 0.0]),
    np.array([0.0, 1.0, 0.0]),
    np.array([1.0, 1.0, 0.0]) / np.sqrt(2.0),
])
def test_revalidate_catches_grazing_clip_at_various_angles_beyond_exemption(cfg, offset_dir):
    """MASKING TEST (b), grazing variant: not dead-on, just close enough
    (within r+standoff) at a point well beyond the exemption zone,
    approached from different lateral directions -- must still be
    caught."""
    anchor = np.array([0.0, 0.0, 0.0])
    se = np.array([8.0, 0.0, 6.0])
    r0, r_end = 0.3, 0.5
    m = _member(se, anchor, r0=r0, r1=r_end, kind="strut",
                owner=("contact", 0), embed_end="p1")
    pitch_bound = _pitch_bound_mm(cfg)
    inset = cfg.contact_depth_mm + pitch_bound + r_end + cfg.standoff_mm
    length = float(np.linalg.norm(se - anchor))
    s = inset + 2.0  # comfortably beyond the exemption boundary
    t = 1.0 - s / length
    r_here = r0 + t * (r_end - r0)
    margin = r_here + cfg.standoff_mm - 0.05  # just inside the required clearance
    t_lo, t_hi = t - 0.01, t + 0.01
    clip_cloud = _dense_line(se, anchor, t_lo, t_hi, n=15, lateral_offset=offset_dir * margin)
    f = tm.Forest()
    f.add_member(m)
    surface_tree = cKDTree(np.vstack([[anchor], clip_cloud, _dense_background_cloud()]))
    violations = f.revalidate(surface_tree, cfg)
    assert len(violations) == 1


def test_revalidate_no_exemption_when_embed_end_is_not_on_model_surface(cfg):
    """MASKING TEST (c): a member whose embedded end is NOT a designed
    on-model anchor (embed_end=None -- a plate root or a bifurcation
    junction) must get NO new exemption at all: a clip well inside where
    an actual anchor's exemption zone WOULD have covered it must still
    be caught. pitch_bound_mm never enters this member's formula at all
    (embed_end is None, so neither exemption branch in revalidate ever
    runs for it) -- no background cloud needed here."""
    p0 = np.array([8.0, 0.0, 6.0])
    p1 = np.array([0.0, 0.0, 0.0])
    r_end = 0.5
    m = _member(p0, p1, r0=0.3, r1=r_end, kind="trunk",
                owner=("contact", 0), embed_end=None)
    length = float(np.linalg.norm(p1 - p0))
    pitch_bound = _pitch_bound_mm(cfg)
    inset = cfg.contact_depth_mm + pitch_bound + r_end + cfg.standoff_mm  # what WOULD apply if embed_end="p1"
    clip_near_p1 = p1 + (p0 - p1) / length * (inset * 0.5)  # well inside that hypothetical zone
    f = tm.Forest()
    f.add_member(m)
    surface_tree = cKDTree(np.array([clip_near_p1]))
    violations = f.revalidate(surface_tree, cfg)
    assert len(violations) == 1


def test_revalidate_rejects_anchored_strut_that_also_clips_other_geometry(cfg):
    """MASKING TEST (d): an otherwise-legitimate on-surface-anchored
    strut (its own anchor region passes cleanly on its own, confirmed
    below) that ADDITIONALLY clips a different, unrelated part of the
    model along its shaft must still be rejected -- the exemption must
    not spill over to excuse an unrelated clip just because the member
    also has a legitimate anchor somewhere."""
    anchor = np.array([0.0, 0.0, 0.0])
    se = np.array([8.0, 0.0, 6.0])
    r_end = 0.5
    length = float(np.linalg.norm(se - anchor))
    pitch_bound = _pitch_bound_mm(cfg)
    inset = cfg.contact_depth_mm + pitch_bound + r_end + cfg.standoff_mm
    other_clip = anchor + (se - anchor) / length * (inset + 3.0)  # unrelated, well past the zone
    background = _dense_background_cloud()

    f_clean = tm.Forest()
    f_clean.add_member(_member(se, anchor, r0=0.3, r1=r_end, kind="strut",
                                owner=("contact", 0), embed_end="p1"))
    assert f_clean.revalidate(cKDTree(np.vstack([[anchor], background])), cfg) == []

    f = tm.Forest()
    f.add_member(_member(se, anchor, r0=0.3, r1=r_end, kind="strut",
                          owner=("contact", 0), embed_end="p1"))
    surface_tree = cKDTree(np.vstack([[anchor, other_clip], background]))
    violations = f.revalidate(surface_tree, cfg)
    assert len(violations) == 1


def test_checkpoint_rollback_restores_mutated_member_state(cfg):
    f = tm.Forest()
    idx = f.add_member(_member([0, 0, 0], [0, 0, -10], r0=0.3, r1=0.3))
    f.revalidate(_far_tree(), cfg)
    assert f.dirty_members() == []
    cp = f.checkpoint()
    f.members[idx].r0 = 5.0
    f.members[idx].p1 = np.array([9.0, 9.0, 9.0])
    assert f.dirty_members() == [idx]
    f.rollback(cp)
    assert f.members[idx].r0 == 0.3
    assert np.allclose(f.members[idx].p1, [0, 0, -10])
    assert f.dirty_members() == []


def test_checkpoint_rollback_removes_newly_appended_members():
    f = tm.Forest()
    a = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    cp = f.checkpoint()
    b = f.add_member(_member([5, 0, 0], [0, 0, 0]))
    f.set_parent(b, a)
    assert len(f.members) == 2
    f.rollback(cp)
    assert len(f.members) == 1
    assert f.roots == [a]


def test_checkpoint_rollback_restores_topology_of_existing_member():
    f = tm.Forest()
    a = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    b = f.add_member(_member([5, 0, 0], [0, 0, 0]))
    f.set_parent(b, a)
    cp = f.checkpoint()
    c = f.add_member(_member([1, 1, 1], [2, 2, 2]))
    f.set_parent(c, a)
    assert sorted(f.children[a]) == sorted([b, c])
    f.rollback(cp)
    assert f.children[a] == [b]
    assert f.parent[b] == a


def test_commit_keeps_mutations_and_pops_checkpoint():
    f = tm.Forest()
    idx = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    cp = f.checkpoint()
    f.members[idx].r0 = 2.0
    f.commit(cp)
    assert f.members[idx].r0 == 2.0
    assert f._cp_stack == []


# ------------------------------------------------------------- brace reg --
def test_forest_starts_with_empty_brace_registry():
    f = tm.Forest()
    assert f.braces == []


def test_add_brace_registers_member_without_entering_tree_topology():
    """design-v4.md #9 / SPEC-BRACING Part 1: a brace is a Forest member
    (rides certificates/volume/printability generically) but NEVER enters
    roots/children/terminal_of -- it's a graph edge, not topology."""
    f = tm.Forest()
    host = f.add_member(_member([0, 0, 0], [0, 0, -10]))
    b = f.add_brace(_member([1, 1, 1], [2, 2, 2], kind="brace",
                             owner=("brace", 0)))
    assert b in range(len(f.members))
    assert f.members[b].kind == "brace"
    assert b in f.braces
    assert b not in f.roots
    assert f.parent[b] is None
    assert f.children[b] == []
    for ch in f.children:
        assert b not in ch
    assert b not in f.terminal_of.values()
    # host member unaffected
    assert host in f.roots


def test_checkpoint_rollback_removes_newly_appended_brace_and_registry_entry():
    f = tm.Forest()
    f.add_member(_member([0, 0, 0], [0, 0, -10]))
    cp = f.checkpoint()
    b = f.add_brace(_member([1, 1, 1], [2, 2, 2], kind="brace",
                             owner=("brace", 0)))
    assert b in f.braces
    assert len(f.members) == 2
    f.rollback(cp)
    assert len(f.members) == 1
    assert f.braces == []


def test_checkpoint_commit_keeps_brace_and_registry_entry():
    f = tm.Forest()
    f.add_member(_member([0, 0, 0], [0, 0, -10]))
    cp = f.checkpoint()
    b = f.add_brace(_member([1, 1, 1], [2, 2, 2], kind="brace",
                             owner=("brace", 0)))
    f.commit(cp)
    assert b in f.braces
    assert len(f.members) == 2
    assert f._cp_stack == []


def test_nested_checkpoint_rollback_inner_keeps_outer_brace():
    """Nested transaction correctness: an outer-committed... no, an
    outer-LEVEL brace survives an INNER checkpoint's rollback, while the
    inner brace itself is undone."""
    f = tm.Forest()
    f.add_member(_member([0, 0, 0], [0, 0, -10]))
    cp_outer = f.checkpoint()
    b_outer = f.add_brace(_member([1, 1, 1], [2, 2, 2], kind="brace",
                                   owner=("brace", 0)))
    cp_inner = f.checkpoint()
    b_inner = f.add_brace(_member([3, 3, 3], [4, 4, 4], kind="brace",
                                   owner=("brace", 1)))
    assert sorted(f.braces) == sorted([b_outer, b_inner])
    f.rollback(cp_inner)
    assert f.braces == [b_outer]
    assert len(f.members) == 2
    f.commit(cp_outer)
    assert f.braces == [b_outer]
    assert f._cp_stack == []


def test_forest_pickle_round_trips_braces(cfg):
    f = tm.Forest()
    f.add_member(_member([0, 0, 0], [0, 0, -10]))
    f.add_brace(_member([1, 1, 1], [2, 2, 2], kind="brace", owner=("brace", 0)))
    f.add_brace(_member([1, 1, 1], [2, 2, 5], kind="model_rung",
                         owner=("model_rung", 0), embed_end="p1"))
    import pickle
    f2 = pickle.loads(pickle.dumps(f))
    assert f2.braces == f.braces
    assert len(f2.members) == len(f.members)
    assert [m.kind for m in f2.members] == [m.kind for m in f.members]
    assert f2.roots == f.roots


# ---------------------------------------------------------------- volume --
def test_volume_report_matches_hand_computed_frustum_and_ball():
    m = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=2.0, kind="trunk")
    expected_frustum = np.pi / 3.0 * 10.0 * (1.0 ** 2 + 1.0 * 2.0 + 2.0 ** 2) / 1000.0
    ball = dict(center=np.array([0.0, 0.0, -10.0]), r=1.5, owner=("contact", 0))
    expected_ball = 4.0 / 3.0 * np.pi * 1.5 ** 3 / 1000.0
    report = tm.volume_report([m], [ball])
    assert report["member_ml"] == pytest.approx(expected_frustum)
    assert report["ball_ml"] == pytest.approx(expected_ball)
    assert report["total_ml"] == pytest.approx(expected_frustum + expected_ball)
    assert report["by_kind_ml"]["trunk"] == pytest.approx(expected_frustum)


def test_volume_report_sums_multiple_kinds():
    m1 = _member([0, 0, 0], [0, 0, -10], r0=1.0, r1=1.0, kind="trunk")
    m2 = _member([0, 0, 0], [3, 0, -3], r0=0.4, r1=0.4, kind="shoot")
    report = tm.volume_report([m1, m2], [])
    v1 = np.pi * 1.0 ** 2 * 10.0 / 1000.0
    v2 = np.pi * 0.4 ** 2 * (3 ** 2 + 3 ** 2) ** 0.5 / 1000.0
    assert report["by_kind_ml"]["trunk"] == pytest.approx(v1)
    assert report["by_kind_ml"]["shoot"] == pytest.approx(v2, rel=1e-6)
    assert report["member_ml"] == pytest.approx(v1 + v2, rel=1e-6)


# ---------------------------------------------------------- printability --
def test_printability_report_catches_too_steep_segment(cfg):
    # 80 degrees from vertical at a fat radius (60-deg family) -- steeper
    # than either angle family allows.
    m = _member([0, 0, 10], [20 * np.tan(np.radians(80)), 0, 0],
                r0=1.0, r1=1.0, kind="trunk")
    violations = tm.printability_report([m], cfg)
    assert len(violations) == 1


def test_printability_report_catches_upward_flare_too_fast(cfg):
    """A member within angle budget geometrically, but whose radius grows
    fast enough going UP that drift+taper still exceeds tan(theta_max)."""
    # near-vertical (small drift) but flares from 0.2 to 3.0mm over 2mm rise
    m = _member([0, 0, 0], [0.1, 0, 2.0], r0=0.2, r1=3.0, kind="trunk")
    violations = tm.printability_report([m], cfg)
    assert len(violations) == 1


def test_printability_report_passes_45_degree_member(cfg):
    # exactly max_segment_angle_deg (45), radius below the 60-deg family
    # floor so the tighter 45-deg budget applies -- should just pass.
    r = cfg.min_shoot_r_mm - 0.05
    m = _member([0, 0, 10], [10, 0, 0], r0=r, r1=r, kind="shoot")
    violations = tm.printability_report([m], cfg)
    assert violations == []


def test_printability_report_credits_upward_taper(cfg):
    """An upward taper (shrinking radius going up) is a CREDIT: a member
    whose drift alone would be borderline should pass once the taper
    term is added, since taper_rate is negative."""
    theta = cfg.branch_angle_deg
    tanth = np.tan(np.radians(theta))
    dz = 10.0
    dxy = dz * tanth  # drift alone would sit exactly AT the limit
    r0, r1 = 1.0, 0.4  # shrinks going up (both >= min_shoot_r_mm, so the
    # 60-deg family applies) -> negative taper_rate credit
    m = _member([0, 0, 0], [dxy, 0, dz], r0=r0, r1=r1, kind="trunk")
    violations = tm.printability_report([m], cfg)
    assert violations == []
    # sanity: without the taper credit (constant radius) the same drift
    # would be right at the boundary, not comfortably clear
    m_flat = _member([0, 0, 0], [dxy, 0, dz], r0=1.0, r1=1.0, kind="trunk")
    v_flat = tm.printability_report([m_flat], cfg)
    assert v_flat == []  # boundary itself is not a violation (<=, not <)


def test_printability_report_exempts_stub_kind(cfg):
    # a stub following a near-horizontal surface normal must not be
    # flagged -- it's physically constrained, not a builder choice.
    m = _member([0, 0, 0], [2.5, 0, 0.01], r0=0.3, r1=0.3, kind="stub")
    assert tm.printability_report([m], cfg) == []


def test_printability_report_model_rung_exempt_when_short(cfg):
    m = _member([0, 0, 5], [cfg.model_brace_max_len_mm - 0.1, 0, 5],
                r0=0.5, r1=0.5, kind="model_rung")
    assert tm.printability_report([m], cfg) == []


def test_printability_report_flags_horizontal_non_rung(cfg):
    m = _member([0, 0, 5], [10, 0, 5], r0=0.5, r1=0.5, kind="brace")
    violations = tm.printability_report([m], cfg)
    assert len(violations) == 1
    assert violations[0]["reason"] == "near_horizontal"


def test_printability_report_passes_45_degree_brace_rod(cfg):
    """SPEC-BRACING #9: a 45-degree brace rod (v3.6's hardcoded rise==d
    construction) must pass the cone rule at any radius -- tan(45) is the
    minimum budget of either angle family."""
    m = _member([0, 0, 0], [8, 0, 8], r0=0.5, r1=0.5, kind="brace")
    assert tm.printability_report([m], cfg) == []


def test_volume_report_includes_brace_kinds_in_by_kind_ml():
    """SPEC-BRACING Part 1 item 4: brace kinds ride volume_report generically
    -- no special-casing needed, just confirm the by_kind_ml bucketing
    covers "brace" and "model_rung" like any other kind string."""
    m1 = _member([0, 0, 0], [8, 0, 8], r0=0.5, r1=0.5, kind="brace")
    m2 = _member([0, 0, 0], [3, 0, 0], r0=0.5, r1=0.5, kind="model_rung",
                  embed_end="p1")
    report = tm.volume_report([m1, m2], [])
    assert "brace" in report["by_kind_ml"]
    assert "model_rung" in report["by_kind_ml"]
    assert report["member_ml"] == pytest.approx(
        report["by_kind_ml"]["brace"] + report["by_kind_ml"]["model_rung"])
