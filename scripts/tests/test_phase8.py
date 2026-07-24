"""Phase-8 FOUNDATION + CLASS-T STUB RE-AIM (SM-STUB R1) tests.

SPEC: docs/specs/SPEC-P8-RELOCATE.md (the amended DRAFT at HEAD is the law).
These pin the foundation surfaces the spec makes normative:

  * the FROZEN defect snapshot with stable site IDs and the census
    target-selection authority (§3.0, §5.3 Layer 2 -- mi=445-class);
  * the pinned R1 re-aim formulas with the invariant contact surface point
    (§3.1 amendment E);
  * the direction x length candidate grid, lean-SKIP (never clamp, Open-risk #1);
  * the ONE R1 selection algorithm: GLOBAL lexicographic minimisation over the
    full legal set with the pocket-density penalty in its pinned slot (§3.1
    amendment B);
  * the amendment-H per-candidate resin / quality guards AS LEGALITY;
  * BATCHED gather->scatter candidate evaluation (site indexing across the flat
    candidate matrix is order-invariant);
  * the T/Q/B phase order with the deferred state machines raising, not no-oping.

Site-derived (task binding rule, §6.8): a P1-cluster-class stub fixed by R1
re-aim; a firm/band member that must be rebuilt; an all-moot member that must
NOT; the mi=445 robust tip-anchor boundary treated as a target; the strict-`<`
0.08 boundary; a zero-NEW-firm-transit check on a re-aimed synthetic.
"""
import numpy as np
import trimesh
import pytest

from support_placement import phase8, verify
from support_placement.geometry_exact import ExactSurface
from support_placement.config import SupportConfig


# --------------------------------------------------------------------------
# deterministic triangle-soup fixtures (mirrors test_v5_gates._box/_soup)
# --------------------------------------------------------------------------
def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def _soup(*meshes):
    merged = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=merged.vertices.copy(),
                           faces=merged.faces.copy(), process=False)


def _fin_mesh(fin_z, fin_z0):
    """Slab (top face z=0) + a floating 10x10 fin of thickness `fin_z` whose
    underside is at `fin_z0`. A stub embedded 0.3mm in the slab and running up
    the +z normal welds THROUGH the fin -- the fin crossing is the only
    unanchored rule-4 transit, so its chord (== fin_z) alone decides moot vs
    firm, and its z-position decides which length factor clears it. The fin is
    wide (10x10) so no lean escapes it -- only shortening the stub clears."""
    slab = _box((20, 20, 5), center=(0, 0, -2.5))
    fin = _box((10, 10, fin_z), center=(0, 0, fin_z0 + fin_z / 2))
    return _soup(slab, fin)


def _vertical_site(mi=100, r_tip=0.2, cfg=None):
    cfg = cfg or SupportConfig()
    return phase8.StubSite(
        site_id=mi, c=np.array([0.0, 0.0, 0.0]), n0=np.array([0.0, 0.0, 1.0]),
        r_tip=r_tip, stub_length_mm=cfg.stub_length_mm,
        contact_depth_mm=cfg.contact_depth_mm, carried_mm2=1.0,
        owner=("contact", mi))


@pytest.fixture
def cfg():
    return SupportConfig()


# ==========================================================================
# Config constants pinned to SPEC-P8-RELOCATE §8 (ratification levers).
# ==========================================================================
def test_config_constants_pinned(cfg):
    assert cfg.p8_reaim_polar_deg == (0.0, 10.0, 20.0, 30.0)
    assert cfg.p8_reaim_azimuth_deg == (0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0)
    assert cfg.p8_reaim_length_factors == (1.0, 0.75, 0.5)
    assert cfg.p8_pocket_voxel_mm == 6.0
    assert cfg.p8_pocket_min_members == 8
    assert cfg.p8_pocket_corridor_pad_mm == 0.5
    assert cfg.p8_ball_slide_step_mm == 0.25
    assert cfg.p8_ball_slide_max_mm == 2.0
    assert cfg.p8_reaim_volume_delta_max_ml == 0.02
    assert cfg.p8_reaim_route_growth_max_factor == 1.50
    assert cfg.p8_strut_growth_max_factor == 1.25
    assert cfg.p8_resin_delta_max == 0.03
    assert cfg.p8_max_unresolved == 10
    assert cfg.p8_max_unresolved_weight == 3.0
    assert cfg.p8_chord_canon_micron == 1
    # the moot floor lives with the SCAR constants in verify (one owner).
    assert verify.PRINT_FLOOR_CHORD_MM == 0.08


# ==========================================================================
# Frozen defect snapshot (§3.0) -- stable site IDs; census target authority.
# ==========================================================================
def test_frozen_snapshot_population():
    """Amendment-A corrected population: 143 non-exempt / 50 moot / band 25,
    with mi=420 all-moot (never a target) and mi=445 non-exempt (a target)."""
    snap = phase8.load_frozen_snapshot()
    s = snap.summary()
    assert s["n_non_exempt"] == 143
    assert s["n_moot"] == 50
    assert s["partition"]["band_0_08_to_0_10"] == 25
    assert s["partition"]["firm_ge_0_10"] == 118
    assert s["partition"]["moot_all_lt_0_08"] == 50
    assert 143 + 50 == 193
    assert snap.provenance["state_md5"] == "aab04cf3d0be6fdb6454ce01c3274dfc"


def test_snapshot_target_selection_census_authority():
    """§5.3 Layer 2 (conservative): the census non-exempt count is the
    acceptance TARGET authority. mi=445 (kernel exempts, census counts) IS a
    target; the all-moot mi=420 is NEVER a target; the 77 Q-REAL condemned
    stubs are targets (same defect, caught pre-insertion)."""
    snap = phase8.load_frozen_snapshot()
    assert snap.is_relocation_target(445) is True          # census-nonexempt -> target
    assert snap.is_relocation_target(420) is False         # all-moot -> never
    # the 143/50 census partition is disjoint by construction
    assert not (set(snap.non_exempt) & set(snap.moot))
    # NO moot member is a target -- moot is never remediated (§2, §3.0), even a
    # Q-REAL condemned stub the census reads moot (448/492/497 on this snapshot).
    assert not (set(snap.moot) & snap._target_set)
    assert {448, 492, 497} <= set(snap.quarantine_real)     # raw-predicate Q-REAL
    assert {448, 492, 497} & set(snap.moot) == {448, 492, 497}
    assert not ({448, 492, 497} & snap._target_set)         # ... but dropped as moot
    # every non-moot Q-REAL condemned stub IS a target
    assert (set(snap.quarantine_real) - set(snap.moot)) <= snap._target_set
    assert len(snap.quarantine_real) == 77


# ==========================================================================
# Pinned R1 re-aim geometry (§3.1 amendment E).
# ==========================================================================
def test_reaim_directions_grid(cfg):
    """25 directions: polar 0 emitted ONCE + 3 polar rings x 8 azimuths, nested
    polar-major / azimuth-minor. polar 0 recovers the outward normal n0."""
    n0 = np.array([0.0, 0.0, 1.0])
    dirs = phase8.reaim_directions(n0, cfg)
    assert len(dirs) == 25
    assert sum(1 for p, a, n in dirs if p == 0.0) == 1
    # first entry is polar 0 == n0
    p0, a0, n_first = dirs[0]
    assert p0 == 0.0 and np.allclose(n_first, n0)
    # polar-major: the four polar values appear in blocks
    polars = [p for p, a, n in dirs]
    assert polars[0] == 0.0
    assert polars[1:9] == [10.0] * 8
    assert polars[9:17] == [20.0] * 8
    assert polars[17:25] == [30.0] * 8
    # every direction is a unit vector
    for p, a, n in dirs:
        assert abs(float(np.linalg.norm(n)) - 1.0) < 1e-12


def test_reaim_formulas_contact_invariant(cfg):
    """The demanded contact surface point c is invariant across every candidate;
    p0 = c - n*delta moves on a sphere of radius delta about c; p1 = c + n*L;
    ball_ctr = p0; u = -n (§3.1 amendment E, pinned formulas)."""
    c = np.array([1.0, 2.0, 3.0])
    delta = cfg.contact_depth_mm
    L = 2.0
    for n_hat in ([0, 0, 1], [1, 0, 0], [0.3, 0.4, 0.8]):
        n_hat = np.asarray(n_hat, float)
        n_hat = n_hat / np.linalg.norm(n_hat)
        m = phase8.reaim_stub(c, n_hat, delta, L, 0.2, 0.35, ("contact", 0), 1.0)
        assert np.allclose(m["p0"], c - n_hat * delta)
        assert np.allclose(m["p1"], c + n_hat * L)
        assert np.allclose(m["ball_ctr"], m["p0"])
        assert np.allclose(m["axis_u"], -n_hat)
        # c is recovered from p0 by adding delta along +n_hat (contact invariant)
        assert np.allclose(m["p0"] + n_hat * delta, c)
        # p0 sits on the sphere of radius delta about c
        assert abs(float(np.linalg.norm(m["p0"] - c)) - delta) < 1e-12
        assert m["embed_end"] == "p0" and m["kind"] == "stub"


def test_local_frame_degenerate_fallback(cfg):
    """|n0.z|>0.999 falls back to the x-axis frame (§3.1) -- azimuth 0 stays
    well-defined for a vertical contact normal."""
    e, e2 = phase8.local_frame(np.array([0.0, 0.0, 1.0]))
    assert abs(float(np.dot(e, [0, 0, 1]))) < 1e-9   # spans the perp plane
    assert abs(float(np.dot(e2, [0, 0, 1]))) < 1e-9
    assert abs(float(np.dot(e, e2))) < 1e-9


# ==========================================================================
# Candidate grid: count, index order, lean-SKIP (never clamp) -- §3.1.
# ==========================================================================
def test_stub_candidates_count_and_index(cfg):
    """A vertical site keeps all 3 lengths x 25 directions = 75 candidates;
    cand_index is length-major then direction order (a total order tiebreak)."""
    cands = phase8.stub_candidates(_vertical_site(cfg=cfg), cfg)
    assert len(cands) == 75
    # indices are exactly 0..74, length-major (step 0 -> 0..24, step 1 -> 25..49)
    assert sorted(c.cand_index for c in cands) == list(range(75))
    for c in cands:
        assert c.cand_index == c.length_step * 25 + (c.cand_index % 25)
    # length_step 0 is f=1.0 (full length), step 2 is f=0.5 (shortest)
    by_step = {c.length_step: c.f for c in cands}
    assert by_step[0] == 1.0 and by_step[1] == 0.75 and by_step[2] == 0.5


def test_lean_skip_not_clamp(cfg):
    """Open-risk #1 fixture: a near-horizontal contact normal pushes some cone
    candidates past the 60deg lean limit; those are SKIPPED (absent), never
    clamped. Every KEPT candidate is within the limit, and the count is < 75."""
    n0 = np.array([1.0, 0.0, 0.2]); n0 = n0 / np.linalg.norm(n0)
    site = phase8.StubSite(site_id=1, c=np.zeros(3), n0=n0, r_tip=0.2,
                           stub_length_mm=cfg.stub_length_mm,
                           contact_depth_mm=cfg.contact_depth_mm, owner=("contact", 1))
    cands = phase8.stub_candidates(site, cfg)
    assert 0 < len(cands) < 75                       # some skipped
    theta_max = cfg.branch_angle_deg
    for c in cands:                                   # none clamped -> all legal-lean
        assert phase8._lean_deg(c.member["p0"], c.member["p1"]) <= theta_max + 1e-9


# ==========================================================================
# R1 core (site-derived): a fin-transit stub fixed by R1 re-aim.
# ==========================================================================
def test_r1_reaim_resolves_fin_transit(cfg):
    """SITE-DERIVED (P1-cluster class: a canopy contact stub whose shaft transits
    adjacent foliage). A vertical stub welds through a wide 0.25mm fin at
    z in [1.4, 1.65] -- a FIRM rule-4 transit (chord 0.25 >= 0.08). R1 re-aim
    shortens the stub to clear it (f=0.5 ends at z=1.25 < 1.4); the fin is wide,
    so no lean escapes -- only the 25 shortest-length candidates are legal, and
    the GLOBAL lex-min is polar 0 (least deviation), f=0.5."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    site = _vertical_site(mi=445, cfg=cfg)
    # the original vertical full-length stub is a firm non-exempt transit
    orig = phase8.reaim_stub(site.c, site.n0, cfg.contact_depth_mm,
                             cfg.stub_length_mm, 0.2, 0.35, site.owner, 1.0)
    ivo = geom.penetration_intervals(orig["p0"], orig["p1"], orig["r0"], orig["r1"])
    vdo = verify._embed_scar_verdicts(geom, [orig], [0], {0: ivo}, cfg)
    assert verify._transit_classify(ivo, orig, vdo.get(0))[0] is True   # firm

    res = phase8.evaluate_r1([site], geom, cfg)[445]
    assert res.winner is not None and res.needs_r2 is False
    w = res.winner
    assert w.polar_deg == 0.0 and w.f == 0.5           # least deviation, shortest legal
    assert res.n_legal == 25                            # exactly the f=0.5 ring clears
    # the re-aimed winner is legal on the independent gate reading
    wm = w.member
    ivw = geom.penetration_intervals(wm["p0"], wm["p1"], wm["r0"], wm["r1"])
    vdw = verify._embed_scar_verdicts(geom, [wm], [0], {0: ivw}, cfg)
    assert verify._transit_classify(ivw, wm, vdw.get(0)) == (False, False)


def test_r1_global_lexicographic_min_unique(cfg):
    """The winner is the UNIQUE global lexicographic minimum over the FULL legal
    set (not first-legal-wins). Recompute the argmin independently and confirm
    it matches, and that the winning tuple is strictly < every other legal
    tuple (total order via the candidate-index tiebreak)."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    site = _vertical_site(mi=7, cfg=cfg)
    # reproduce the legal set + tuples by hand from the batched result internals
    cands = phase8.stub_candidates(site, cfg)
    members = [c.member for c in cands]
    mis = list(range(len(members)))
    ivmap = verify._batch_axis_intervals(geom, members, mis, rays="axis")
    verdicts = verify._embed_scar_verdicts(geom, members, mis, ivmap, cfg)
    tuples = []
    for j, c in enumerate(cands):
        vl = verdicts.get(j)
        if not vl:
            continue
        ne, _ = verify._transit_classify(ivmap.get(j, []), c.member, vl)
        if ne:
            continue
        nm = phase8._count_moot_axis(ivmap.get(j, []), c.member, vl)
        tuples.append((phase8._score_tuple(c, nm, 0, 0.0), c.cand_index))
    assert tuples, "fixture must have a non-empty legal set"
    ref_score = min(tuples, key=lambda t: t[0])[0]
    res = phase8.evaluate_r1([site], geom, cfg)[7]
    assert res.score == ref_score
    # strict uniqueness: no other legal tuple ties the winner
    assert sum(1 for t in tuples if t[0] == ref_score) == 1


def test_r1_zero_new_firm_transit_synthetic(cfg):
    """Zero-NEW-firm-transit unit check (§6.9) on a re-aimed synthetic: the
    candidate R1 commits must itself carry zero firm transits. Verified
    INDEPENDENTLY -- re-derive the winner's intervals from the oracle and run the
    gate's own _transit_classify -- so the property holds against the verify
    gate, not merely the selector that chose it."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    res = phase8.evaluate_r1([_vertical_site(mi=3, cfg=cfg)], geom, cfg)[3]
    assert res.winner is not None
    wm = res.winner.member
    ivw = geom.penetration_intervals(wm["p0"], wm["p1"], wm["r0"], wm["r1"])
    vdw = verify._embed_scar_verdicts(geom, [wm], [0], {0: ivw}, cfg)
    nonexempt, _ = verify._transit_classify(ivw, wm, vdw.get(0))
    assert nonexempt is False                          # no firm transit introduced


def test_r1_mi445_class_boundary_target(cfg):
    """mi=445 robust tip-anchor boundary (§5.3 Layer 2): a member whose census
    reading is a FIRM ~0.2531mm chord transit is a relocation target; R1 re-aim
    makes it legal on BOTH readings. Fixture chord 0.2531 mirrors the archived
    mi=445 measurement; the site is confirmed a snapshot target."""
    snap = phase8.load_frozen_snapshot()
    assert snap.is_relocation_target(445) is True
    geom = ExactSurface(_fin_mesh(0.2531, 1.4))
    site = _vertical_site(mi=445, cfg=cfg)
    orig = phase8.reaim_stub(site.c, site.n0, cfg.contact_depth_mm,
                             cfg.stub_length_mm, 0.2, 0.35, site.owner, 1.0)
    ivo = geom.penetration_intervals(orig["p0"], orig["p1"], orig["r0"], orig["r1"])
    fin_iv = [iv for iv in ivo if iv["s_a"] > 0.5]     # the unanchored fin crossing
    assert fin_iv and (fin_iv[0]["s_b"] - fin_iv[0]["s_a"]) >= verify.PRINT_FLOOR_CHORD_MM
    res = phase8.evaluate_r1([site], geom, cfg)[445]
    assert res.winner is not None and res.needs_r2 is False
    wm = res.winner.member
    ivw = geom.penetration_intervals(wm["p0"], wm["p1"], wm["r0"], wm["r1"])
    vdw = verify._embed_scar_verdicts(geom, [wm], [0], {0: ivw}, cfg)
    assert verify._transit_classify(ivw, wm, vdw.get(0)) == (False, False)


def test_strict_0_08_boundary_is_firm_not_moot(cfg):
    """Strict-`<` PRINT-FLOOR boundary (amendment G / §6.8): a crossing chord
    EXACTLY at 0.08mm is FIRM (moot requires chord strictly < floor) -> the
    member is non-exempt and MUST be rebuilt. Pinned in phase8's own moot
    accounting: _count_moot_axis does NOT count a 0.08 chord as moot, and the
    gate classifies it non-exempt."""
    stub = dict(p0=np.array([0., 0., -0.3]), p1=np.array([0., 0., 2.5]),
                r0=0.2, r1=0.35, kind="stub", owner=("contact", 0), embed_end="p0")
    ivs = [dict(s_a=1.00, s_b=1.08, entered=True, exited=True,
                max_depth_mm=0.04, ray="axis")]        # chord 0.08 == floor
    assert phase8._count_moot_axis(ivs, stub, True) == 0        # not moot
    assert verify._transit_classify(ivs, stub, True)[0] is True  # firm -> target
    # strictly below floor IS counted moot
    ivs_moot = [dict(s_a=1.00, s_b=1.07, entered=True, exited=True,
                     max_depth_mm=0.03, ray="axis")]   # chord 0.07 < floor
    assert phase8._count_moot_axis(ivs_moot, stub, True) == 1


def test_all_moot_member_is_exempt_not_target(cfg):
    """An all-moot member (every rule-4 crossing a sub-0.08 knife-edge) is
    EXEMPT -- recorded, never remediated (§3.0). Pinned end-to-end: a wide
    0.06mm fin produces a moot-only transit, so the original stub is NOT a firm
    violation, and the archived all-moot mi=420 is excluded from the target
    set."""
    geom = ExactSurface(_fin_mesh(0.06, 1.4))
    site = _vertical_site(mi=420, cfg=cfg)
    orig = phase8.reaim_stub(site.c, site.n0, cfg.contact_depth_mm,
                             cfg.stub_length_mm, 0.2, 0.35, site.owner, 1.0)
    ivo = geom.penetration_intervals(orig["p0"], orig["p1"], orig["r0"], orig["r1"])
    vdo = verify._embed_scar_verdicts(geom, [orig], [0], {0: ivo}, cfg)
    nonexempt, moot = verify._transit_classify(ivo, orig, vdo.get(0))
    assert nonexempt is False and moot is True          # moot, not firm
    assert phase8.load_frozen_snapshot().is_relocation_target(420) is False


# ==========================================================================
# Amendment-H per-candidate resin / quality guards AS LEGALITY (§3.1, §8).
# ==========================================================================
def test_r1_volume_delta_guard_rejects(cfg):
    """The volume-delta cap is part of LEGALITY: a downstream re-route whose
    volume balloons past p8_reaim_volume_delta_max_ml illegalises EVERY
    candidate -> empty legal set -> needs_r2. Same fin fixture that otherwise
    resolves cleanly (test_r1_reaim_resolves_fin_transit), so the guard is
    exactly what flips the outcome."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    site = _vertical_site(mi=9, cfg=cfg)

    def fat_reroute(cand):
        return True, cfg.p8_reaim_volume_delta_max_ml + 1.0, 0.0   # +1ml route

    res = phase8.evaluate_r1([site], geom, cfg, reroute_fn=fat_reroute)[9]
    assert res.winner is None and res.needs_r2 is True
    # sanity: without the fat route, the same site resolves
    assert phase8.evaluate_r1([site], geom, cfg)[9].winner is not None


def test_r1_route_growth_guard_rejects(cfg):
    """no-unbounded-growth (amendment H): a re-route longer than
    p8_reaim_route_growth_max_factor x the original downstream route length is
    illegal. Original route 10mm, factor 1.5 -> cap 15mm; a 20mm re-route is
    rejected, an 12mm one passes."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    base = _vertical_site(mi=11, cfg=cfg)
    base.downstream_route_len_mm = 10.0
    base.downstream_route_vol_ml = 0.0

    def grow(route_len):
        def _fn(cand):
            return True, 0.0, route_len
        return _fn

    assert phase8.evaluate_r1([base], geom, cfg, reroute_fn=grow(20.0))[11].winner is None
    assert phase8.evaluate_r1([base], geom, cfg, reroute_fn=grow(12.0))[11].winner is not None


# ==========================================================================
# Selection hooks: pocket-density slot, revalidate, determinism, batching.
# ==========================================================================
def test_r1_density_penalty_slot_ranked_above_polar(cfg):
    """density_penalty occupies its pinned tuple slot, RANKED ABOVE polar_angle
    (§3.1 amendment B): penalising the otherwise-winning polar-0 candidate
    shifts the winner to a leaned candidate -- proving the slot exists and
    outranks lean. Isolated sites (penalty 0) keep the sparse ordering."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    site = _vertical_site(mi=13, cfg=cfg)
    base = phase8.evaluate_r1([site], geom, cfg)[13]
    assert base.winner.polar_deg == 0.0                 # sparse ordering: least lean

    def penalise_polar0(sid, cand):
        return 5 if cand.polar_deg == 0.0 else 0

    pen = phase8.evaluate_r1([site], geom, cfg, pocket_penalty_fn=penalise_polar0)[13]
    assert pen.winner is not None
    assert pen.winner.polar_deg > 0.0                   # density outranks polar
    assert pen.score[1] == 0                            # a zero-penalty candidate won


def test_r1_revalidate_hook_rejects(cfg):
    """The injected §5-law collision revalidate is part of legality: a
    revalidate_fn that rejects every candidate empties the legal set ->
    needs_r2. The engine supplies the real forest.revalidate; None (the
    pure-geometry path) treats it as passing."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    site = _vertical_site(mi=15, cfg=cfg)
    reject = phase8.evaluate_r1([site], geom, cfg, revalidate_fn=lambda c: False)[15]
    assert reject.winner is None and reject.needs_r2 is True
    accept = phase8.evaluate_r1([site], geom, cfg, revalidate_fn=lambda c: True)[15]
    assert accept.winner is not None


def test_r1_determinism(cfg):
    """No RNG, no wall-clock: two evaluations produce bit-identical winners and
    score tuples (§8 determinism doctrine)."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    sites = [_vertical_site(mi=21, cfg=cfg), _vertical_site(mi=22, cfg=cfg)]
    r1 = phase8.evaluate_r1(sites, geom, cfg)
    r2 = phase8.evaluate_r1(sites, geom, cfg)
    for sid in (21, 22):
        assert r1[sid].score == r2[sid].score
        assert r1[sid].winner.cand_index == r2[sid].winner.cand_index
        assert np.allclose(r1[sid].winner.member["p1"], r2[sid].winner.member["p1"])


def test_r1_batched_gather_scatter_order_invariant(cfg):
    """BATCH-FIRST gather->scatter (§7): evaluating many sites TOGETHER (one
    flat candidate matrix, batched kernel calls, scattered back by site index)
    yields the SAME per-site winner as evaluating each site ALONE. This pins the
    cross-site indexing of the flat candidate list -- the scatter bookkeeping
    that a per-site scalar loop would sidestep."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    sites = [_vertical_site(mi=31, cfg=cfg), _vertical_site(mi=32, cfg=cfg),
             _vertical_site(mi=33, cfg=cfg)]
    together = phase8.evaluate_r1(sites, geom, cfg)
    for s in sites:
        alone = phase8.evaluate_r1([s], geom, cfg)[s.site_id]
        assert together[s.site_id].score == alone.score
        assert together[s.site_id].winner.cand_index == alone.winner.cand_index
        assert together[s.site_id].n_legal == alone.n_legal


# ==========================================================================
# Phase order (§3.0) + deferred state machines raise, never silently no-op.
# ==========================================================================
def test_phase_order_normative():
    assert phase8.PHASE_ORDER == ("phase1_stubs", "phase2_struts", "phase3_balls")


def test_deferred_state_machines_raise(cfg):
    """The later state machines are NOT no-ops: they raise until built, so a
    caller can never mistake a missing SM-STRUT/SM-BALL/pocket-solver for a
    clean pass (amendment C/D)."""
    with pytest.raises(NotImplementedError):
        phase8.run_phase2_struts()
    with pytest.raises(NotImplementedError):
        phase8.run_phase3_balls()
    with pytest.raises(NotImplementedError):
        phase8.run_pocket_solver()


def test_run_phase1_stubs_is_r1(cfg):
    """The Phase-1 driver is the SM-STUB R1 ladder entry point; a resolvable
    site returns a winner, an empty legal set returns needs_r2 (the R2 rung),
    never a crash or a silent drop."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    out = phase8.run_phase1_stubs([_vertical_site(mi=41, cfg=cfg)], geom, cfg)
    assert out[41].winner is not None
    # a site with NO clearing length (fin so low every length transits) -> R2
    geom_low = ExactSurface(_fin_mesh(0.25, 0.5))       # fin at [0.5,0.75]; f=0.5 ends 1.25 still crosses
    out2 = phase8.run_phase1_stubs([_vertical_site(mi=42, cfg=cfg)], geom_low, cfg)
    assert out2[42].winner is None and out2[42].needs_r2 is True
