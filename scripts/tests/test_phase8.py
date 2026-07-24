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


def test_pocket_solver_implemented_not_raising(cfg):
    """The atomic-commit pocket solver (§4) is now implemented -- every Phase-8
    state machine (SM-STUB, SM-STRUT, SM-BALL, pocket) runs; none is a skeleton
    that raises. run_pocket_solver over an empty site list is a clean no-op
    report (the §6.10 idempotence base case: nothing to fix -> nothing done)."""
    geom = ExactSurface(_fin_mesh(0.25, 1.4))
    rep = phase8.run_pocket_solver([], geom, cfg)
    assert rep.pockets == [] and rep.isolated == {} and rep.dispositions == {}


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


# ==========================================================================
# SM-STRUT (§3.1s) -- the 17 class-T interior struts, S1->S2->S3.
# ==========================================================================
def _strut_graze_mesh(wall_z0=0.5, wall_h=0.5, wall_x=0.45, wall_t=0.12):
    """mi=1138-lineage strut-graze fixture: a foot-anchor slab (top z=0) plus a
    thin vertical wall offset in +x by ~r_foot. A vertical strut embedded 0.3mm
    in the slab has a CLEAN axis but its +x capsule-offset ray grazes the wall --
    a firm capsule-graze transit only the offset rays (rays='all') catch, exactly
    the census mi=1138 strut subclass. The wall is low enough that no length
    shortening clears it; only a FOOT RE-AIM lean moves the shaft off the wall."""
    base = _box((20, 20, 4), center=(0, 0, -2))
    wall = _box((wall_t, 4, wall_h), center=(wall_x, 0, wall_z0 + wall_h / 2))
    return _soup(base, wall)


def _strut_site(mi=1138, cfg=None, r_foot=0.5, r_root=0.35, strut_len=2.5,
                orig_len=None, criticality=2.0, orphan=False):
    cfg = cfg or SupportConfig()
    return phase8.StrutSite(
        site_id=mi, c=np.array([0.0, 0.0, 0.0]), n0=np.array([0.0, 0.0, 1.0]),
        r_foot=r_foot, r_root=r_root, strut_length_mm=strut_len,
        foot_depth_mm=cfg.contact_depth_mm, carried_mm2=1.0, owner=("strut", mi),
        orig_strut_length_mm=orig_len, criticality=criticality,
        orphan_if_dropped=orphan)


def test_strut_reaim_foot_formulas_and_kind(cfg):
    """S1 foot re-aim uses the SAME pinned formulas as the stub (§3.1s) but
    kind='strut' -- so the SCAR verdict picks BALL_FACTOR_FOOT (1.4, not the 1.5
    stub tip) and the transit classifier sweeps the capsule-offset rays. Contact
    (foot) surface point c is invariant; p0 = c - n*delta; embed_end p0."""
    c = np.array([1.0, 2.0, 3.0]); n_hat = np.array([0.0, 0.0, 1.0])
    m = phase8.reaim_foot(c, n_hat, cfg.contact_depth_mm, 2.0, 0.5, 0.35,
                          ("strut", 1138), 1.0)
    assert m["kind"] == "strut" and m["embed_end"] == "p0"
    assert np.allclose(m["p0"], c - n_hat * cfg.contact_depth_mm)
    assert np.allclose(m["p1"], c + n_hat * 2.0)
    assert np.allclose(m["ball_ctr"], m["p0"]) and np.allclose(m["axis_u"], -n_hat)
    assert m["r0"] == 0.5 and m["r1"] == 0.35


def test_strut_s1_capsule_graze_needs_lean(cfg):
    """SITE-DERIVED (mi=1138 lineage): a strut whose AXIS is clean but whose
    capsule-offset ray grazes a thin wall is a FIRM transit only rays='all'
    catches. Every polar-0 (vertical) length still grazes, so S1 resolves it ONLY
    by a foot-re-aim LEAN (polar > 0); the winner is legal on the independent
    axis+capsule gate."""
    geom = ExactSurface(_strut_graze_mesh())
    site = _strut_site(mi=1138, cfg=cfg)
    # the vertical full strut is firm ONLY on the capsule rays (axis is clean)
    m0 = phase8.reaim_foot(site.c, site.n0, cfg.contact_depth_mm,
                           site.strut_length_mm, site.r_foot, site.r_root,
                           site.owner, 1.0)
    iv_all = geom.penetration_intervals(m0["p0"], m0["p1"], m0["r0"], m0["r1"], rays="all")
    iv_ax = geom.penetration_intervals(m0["p0"], m0["p1"], m0["r0"], m0["r1"], rays="axis")
    vd = verify._embed_scar_verdicts(geom, [m0], [0], {0: iv_all}, cfg)
    assert verify._transit_classify(iv_all, m0, vd.get(0))[0] is True   # capsule firm
    assert verify._transit_classify(iv_ax, m0, vd.get(0)) == (False, False)  # axis clean
    res = phase8.run_phase2_struts([site], geom, cfg)[1138]
    assert res.disposition == phase8.Disposition.STRUT_S1_REAIM
    assert res.rung == "S1" and res.contract == "reaim" and res.winner is not None
    assert res.winner.polar_deg > 0.0                    # a LEAN, not a shorten
    wm = res.winner.member
    ivw = geom.penetration_intervals(wm["p0"], wm["p1"], wm["r0"], wm["r1"], rays="all")
    vdw = verify._embed_scar_verdicts(geom, [wm], [0], {0: ivw}, cfg)
    assert verify._transit_classify(ivw, wm, vdw.get(0)) == (False, False)


def test_strut_s1_axis_only_would_miss_graze(cfg):
    """The strut ray set is normative: evaluating the graze on the STUB (axis-
    only) ray set would call the firm strut legal (a false pass). Pins that
    SM-STRUT uses rays='all' -- the census mi=1138 capsule-graze rule."""
    geom = ExactSurface(_strut_graze_mesh())
    site = _strut_site(mi=1138, cfg=cfg)
    m0 = phase8.reaim_foot(site.c, site.n0, cfg.contact_depth_mm,
                           site.strut_length_mm, site.r_foot, site.r_root,
                           site.owner, 1.0)
    ax = geom.penetration_intervals(m0["p0"], m0["p1"], m0["r0"], m0["r1"], rays="axis")
    al = geom.penetration_intervals(m0["p0"], m0["p1"], m0["r0"], m0["r1"], rays="all")
    vd = verify._embed_scar_verdicts(geom, [m0], [0], {0: al}, cfg)
    assert verify._transit_classify(ax, m0, vd.get(0))[0] is False     # axis-only misses
    assert verify._transit_classify(al, m0, vd.get(0))[0] is True      # capsule catches


def test_strut_s1_growth_guard_rejects(cfg):
    """Amendment-H S1 no-unbounded-strut-growth guard (part of LEGALITY): a
    candidate whose free strut length exceeds p8_strut_growth_max_factor x the
    ORIGINAL strut length is illegal. orig_len 1.0, factor 1.25 -> cap 1.25mm; a
    site whose design length is 2.5mm has its f=1.0 (2.5) and f=0.75 (1.875)
    candidates rejected, only f=0.5 (1.25 == cap) survives the guard."""
    geom = ExactSurface(_box((20, 20, 4), center=(0, 0, -2)))   # clean slab, no wall
    tight = _strut_site(mi=1, cfg=cfg, orig_len=1.0)
    res = phase8.evaluate_s1([tight], geom, cfg)[1]
    assert res.winner is not None
    assert res.winner.f == 0.5                            # only the shortest survives
    # generous baseline: every length legal-lean survives the guard
    loose = _strut_site(mi=2, cfg=cfg, orig_len=2.5)
    assert phase8.evaluate_s1([loose], geom, cfg)[2].winner.f == 1.0


def test_strut_s2_reroute_marks_dead_never_quarantine(cfg):
    """S1 empty (revalidate rejects every foot re-aim) -> S2 re-route. The old
    foot's disposal contract is mark_dead, NEVER mark_quarantined (amendment D:
    that stub-only contract does not cover interior struts)."""
    geom = ExactSurface(_strut_graze_mesh())
    site = _strut_site(mi=816, cfg=cfg)
    res = phase8.run_phase2_struts([site], geom, cfg,
                                   revalidate_fn=lambda c: False,
                                   reroute_fn=lambda s: (True, 0.0, 5.0))[816]
    assert res.disposition == phase8.Disposition.STRUT_S2_REROUTE
    assert res.rung == "S2" and res.contract == "mark_dead"
    assert res.contract != "mark_quarantined"


def test_strut_s3_retirement_is_mark_dead_not_quarantine(cfg):
    """S1 and S2 both empty -> S3 honest STRUCTURAL failure: mark_dead + an
    unresolved STRUT (§6.7), NEVER mark_quarantined. Pins the amendment-D
    contract fix: no strut disposition maps to the stub quarantine path."""
    geom = ExactSurface(_strut_graze_mesh())
    site = _strut_site(mi=824, cfg=cfg, criticality=2.0)
    res = phase8.run_phase2_struts([site], geom, cfg,
                                   revalidate_fn=lambda c: False)[824]   # no reroute_fn
    assert res.disposition == phase8.Disposition.STRUT_S3_DEAD
    assert res.rung == "S3" and res.contract == "mark_dead" and res.unresolved is True
    assert res.criticality == 2.0 and res.trace == ("S1", "S2", "S3")
    # the contract table itself never contains the stub quarantine path
    assert "mark_quarantined" not in set(phase8._STRUT_CONTRACT.values())


def test_strut_s3_raft_connectivity_guard_escalates(cfg):
    """Raft-connectivity guard (amendment G, §6.9): if dropping the strut would
    orphan a subtree from the raft, S3 mark_dead is DISALLOWED -- the strut
    ESCALATES to the owner as a criticality-weighted unresolved, never silently
    severed. orphan_fn True flips STRUT_S3_DEAD -> STRUT_S3_ESCALATE."""
    geom = ExactSurface(_strut_graze_mesh())
    site = _strut_site(mi=845, cfg=cfg, criticality=2.5)
    res = phase8.run_phase2_struts([site], geom, cfg,
                                   revalidate_fn=lambda c: False,
                                   orphan_fn=lambda s: True)[845]
    assert res.disposition == phase8.Disposition.STRUT_S3_ESCALATE
    assert res.contract == "escalate" and res.unresolved is True
    assert res.criticality == 2.5


def test_strut_s1_determinism_and_batched(cfg):
    """No RNG / no wall-clock: S1 winners are bit-identical run-to-run, and
    evaluating struts TOGETHER (batched gather->scatter, rays='all') gives the
    SAME per-site winner as alone."""
    geom = ExactSurface(_strut_graze_mesh())
    sites = [_strut_site(mi=867, cfg=cfg), _strut_site(mi=874, cfg=cfg)]
    a = phase8.evaluate_s1(sites, geom, cfg)
    b = phase8.evaluate_s1(sites, geom, cfg)
    for sid in (867, 874):
        assert a[sid].score == b[sid].score
        assert a[sid].winner.cand_index == b[sid].winner.cand_index
        alone = phase8.evaluate_s1([_strut_site(mi=sid, cfg=cfg)], geom, cfg)[sid]
        assert alone.winner.cand_index == a[sid].winner.cand_index
        assert alone.n_legal == a[sid].n_legal


# ==========================================================================
# SM-BALL (§3.2) -- the 15 true joint-ball breaches, B1->B2->B3, run LAST.
# ==========================================================================
def _ball53_mesh(near=0.2191, thick=0.0623):
    """Reconstruction of archived breaching ball 53 (census breaching_balls):
    r=0.35, center outside the surface (surface_dist 0.2191), the ball spanning a
    thin 0.06mm foliage knife-edge whose FAR wall sits 0.2814mm from the center
    -> feature_thickness_mm 0.2814, poke_out 0.0686. A thin slab whose near face
    is `near` below the ball center and whose far face is `near+thick` below it
    reproduces those exact numbers; sliding the joint root-ward clears it."""
    return _box((4, 4, thick), center=(0.0, 0.0, -(near + thick / 2.0)))


def _ball_site(bid=53, cfg=None, root_dir=(0, 0, 1), r=0.35, criticality=1.5,
               orphan=False):
    return phase8.BallSite(ball_id=bid, center=np.array([0.0, 0.0, 0.0]), r=r,
                           root_dir=np.asarray(root_dir, float),
                           owner=("contact", 55), criticality=criticality,
                           orphan_if_dropped=orphan)


def test_ball53_reconstruction_matches_archived(cfg):
    """The fixture reproduces archived ball 53's measured numbers exactly (r
    0.35, feature_thickness 0.2814, poke_out 0.0686) -- the ball-breach
    reconstruction anchor from the archived forest census."""
    geom = ExactSurface(_ball53_mesh())
    br, th, pk = geom.sphere_breach(np.array([[0.0, 0.0, 0.0]]), np.array([0.35]),
                                    eps_mm=0.05)
    assert bool(br[0]) is True
    assert abs(float(th[0]) - 0.2814) < 1e-3
    assert abs(float(pk[0]) - 0.0686) < 1e-3


def test_ball_b1_axial_slide_resolves_breach(cfg):
    """SITE-DERIVED (archived ball 53): the joint breaches a thin feature; B1
    slides it minimally along the member axis to a clean position. The winner is
    the MINIMAL slide (0.25mm, root-ward rank 0), the ball radius is INVARIANT
    (never shrunk), and disposition is BALL_B1_SLIDE."""
    geom = ExactSurface(_ball53_mesh())
    site = _ball_site(bid=53, cfg=cfg)
    res = phase8.run_phase3_balls([site], geom, cfg)[53]
    assert res.disposition == phase8.Disposition.BALL_B1_SLIDE
    assert res.rung == "B1" and res.ball_shrunk is False
    assert res.slide.slide_distance_mm == cfg.p8_ball_slide_step_mm   # minimal 0.25
    assert res.slide.direction_rank == 0                              # root-ward
    # the slid center is sphere-clean at the UNCHANGED radius (never shrunk)
    br, _t, _p = geom.sphere_breach(res.slide.center[None, :], np.array([site.r]),
                                    eps_mm=0.05)
    assert bool(br[0]) is False


def test_ball_slide_grid_order_and_no_zero(cfg):
    """The B1 slide grid is magnitude-major then direction-minor (root-ward rank
    0 before tip-ward rank 1), excludes the zero slide, and spans step..max. For
    step 0.25 / max 2.0 that is 8 magnitudes x 2 directions = 16 candidates; the
    slide_index total order makes first-clean == lex-min (slide_distance,
    direction_rank)."""
    site = _ball_site(cfg=cfg)
    cands = phase8.ball_slide_candidates(site, cfg)
    assert len(cands) == 16
    assert all(c.slide_distance_mm > 0 for c in cands)               # no zero slide
    assert [c.slide_index for c in cands] == list(range(16))
    # first two share the smallest magnitude, root-ward (0) before tip-ward (1)
    assert cands[0].slide_distance_mm == cands[1].slide_distance_mm == 0.25
    assert cands[0].direction_rank == 0 and cands[1].direction_rank == 1
    assert cands[-1].slide_distance_mm == 2.0


def test_ball_b1_minimal_slide_when_multiple_clean(cfg):
    """When several slide positions are clean, B1 takes the MINIMAL-slide one
    (lex-min of (slide_distance, direction_rank)) -- not merely any clean
    position. Root-ward 0.25 is clean AND root-ward 0.5 is clean; 0.25 wins."""
    geom = ExactSurface(_ball53_mesh())
    b1 = phase8.evaluate_b1([_ball_site(cfg=cfg)], geom, cfg)[53]
    assert b1.n_clean >= 2                                            # multiple clean
    assert b1.winner.slide_distance_mm == 0.25 and b1.winner.direction_rank == 0


def test_ball_b2_local_reroute(cfg):
    """B1 empty (slide-revalidate rejects every clean slide -- e.g. both adjoining
    segments would go illegal) -> B2 local re-route; disposition BALL_B2_REROUTE,
    ball never shrunk."""
    geom = ExactSurface(_ball53_mesh())
    site = _ball_site(bid=751, cfg=cfg)
    res = phase8.run_phase3_balls([site], geom, cfg,
                                  slide_revalidate_fn=lambda s, sl: False,
                                  reroute_fn=lambda s: True)[751]
    assert res.disposition == phase8.Disposition.BALL_B2_REROUTE
    assert res.rung == "B2" and res.ball_shrunk is False


def test_ball_b3_honest_failure_never_shrinks(cfg):
    """B1 and B2 both fail -> B3 honest failure: an unresolved BALL (§6.7), the
    breach never suppressed and the ball NEVER shrunk (the only remedy on offer
    is relocation). ball_shrunk stays False; unresolved True."""
    geom = ExactSurface(_ball53_mesh())
    site = _ball_site(bid=773, cfg=cfg, criticality=1.5)
    res = phase8.run_phase3_balls([site], geom, cfg,
                                  slide_revalidate_fn=lambda s, sl: False)[773]  # no reroute
    assert res.disposition == phase8.Disposition.BALL_B3_UNRESOLVED
    assert res.rung == "B3" and res.unresolved is True and res.ball_shrunk is False
    assert res.trace == ("B1", "B2", "B3") and res.criticality == 1.5


def test_ball_b1_batched_order_invariant(cfg):
    """BATCH-FIRST: evaluating several balls TOGETHER (one flat slide matrix, ONE
    sphere_breach call) yields the SAME per-ball winner as evaluating each alone
    -- the scatter bookkeeping a per-ball scalar loop would sidestep."""
    geom = ExactSurface(_ball53_mesh())
    sites = [_ball_site(bid=53, cfg=cfg), _ball_site(bid=289, cfg=cfg),
             _ball_site(bid=388, cfg=cfg)]
    together = phase8.evaluate_b1(sites, geom, cfg)
    for s in sites:
        alone = phase8.evaluate_b1([s], geom, cfg)[s.ball_id]
        assert together[s.ball_id].winner.slide_index == alone.winner.slide_index
        assert together[s.ball_id].n_clean == alone.n_clean


def test_ball_radius_invariant_across_slides(cfg):
    """The ball radius is never touched by SM-BALL (relocation, not shrinking):
    every slide candidate carries the ORIGINAL center offset only, and the B1
    winner is tested at the unchanged radius. No slide candidate encodes a
    radius, so shrinking is structurally impossible."""
    site = _ball_site(cfg=cfg, r=0.35)
    cands = phase8.ball_slide_candidates(site, cfg)
    for c in cands:
        assert not hasattr(c, "r")                       # no radius on a slide
        assert np.isclose(np.linalg.norm(c.center - site.center),
                          c.slide_distance_mm)            # pure translation


def test_phase2_phase3_implemented_not_raising(cfg):
    """Phase 2 (SM-STRUT) and Phase 3 (SM-BALL) are implemented state machines --
    they return results, they do not raise."""
    geom_s = ExactSurface(_strut_graze_mesh())
    geom_b = ExactSurface(_ball53_mesh())
    assert phase8.run_phase2_struts([_strut_site(cfg=cfg)], geom_s, cfg)   # no raise
    assert phase8.run_phase3_balls([_ball_site(cfg=cfg)], geom_b, cfg)     # no raise


# ==========================================================================
# §4 POCKET SOLVER -- detection, conflict graph, atomic-commit loop.
# ==========================================================================
def _pocket_mesh(fin_z=0.25, fin_z0=1.4, extent=60.0):
    """A wide slab (top z=0) + a wide fin: a vertical stub embedded 0.3mm welds
    THROUGH the fin (firm rule-4 transit); shortening to f=0.5 (tip z=1.25)
    clears it. Wide enough that a whole cluster of clustered contacts shares the
    same clearing geometry -- the canopy-pocket class."""
    slab = _box((extent, extent, 5), center=(0, 0, -2.5))
    fin = _box((extent, extent, fin_z), center=(0, 0, fin_z0 + fin_z / 2))
    return _soup(slab, fin)


def _cluster_stub(mi, x, y, cfg, wfc=0.25, n0=(0.0, 0.0, 1.0)):
    return phase8.StubSite(
        site_id=mi, c=np.array([float(x), float(y), 0.0]),
        n0=np.asarray(n0, float), r_tip=0.2, stub_length_mm=cfg.stub_length_mm,
        contact_depth_mm=cfg.contact_depth_mm, carried_mm2=1.0,
        owner=("contact", mi), frozen_worst_chord_mm=float(wfc))


def _dense_cluster(cfg, base_mi=100, origin=(1.0, 1.0), n=3, pitch=1.2):
    """An n x n grid of contact stubs packed on a `pitch` mm lattice near
    `origin` -- all tips fall inside one 6mm voxel neighbourhood, a single dense
    canopy pocket. Distinct descending worst-firm-chords pin the commit order."""
    sites = []
    k = 0
    for ix in range(n):
        for iy in range(n):
            sites.append(_cluster_stub(
                base_mi + k, origin[0] + ix * pitch, origin[1] + iy * pitch,
                cfg, wfc=0.30 - 0.001 * k))
            k += 1
    return sites


# ---- detection (§4.1) ------------------------------------------------------
def test_pocket_voxel_key_pinned_to_world_origin(cfg):
    """voxel_key is a pure function of WORLD coordinates pinned to (0,0,0), NOT a
    bbox-relative grid (amendment F: a bbox grid shifts under any geometry move
    and destabilises pocket identity). floor(x/6) with a strict cell boundary."""
    assert phase8.voxel_key((0.0, 0.0, 0.0), 6.0) == (0, 0, 0)
    assert phase8.voxel_key((5.999, 5.999, 5.999), 6.0) == (0, 0, 0)
    assert phase8.voxel_key((6.0, 0.0, 0.0), 6.0) == (1, 0, 0)     # boundary -> next
    assert phase8.voxel_key((-0.001, 0.0, 0.0), 6.0) == (-1, 0, 0)  # floor, not trunc
    # pure function: the SAME world point always maps to the SAME key
    assert phase8.voxel_key((13.2, -7.1, 49.5), 6.0) == phase8.voxel_key((13.2, -7.1, 49.5), 6.0)


def test_pocket_detection_min_members_threshold(cfg):
    """A pocket needs >= p8_pocket_min_members (8) defect tips. 7 clustered tips
    are NOT a pocket (they are isolated §4.6 members); 8 ARE."""
    seven = [_cluster_stub(200 + i, 1.0 + 0.3 * i, 1.0, cfg) for i in range(7)]
    assert phase8.detect_pockets(seven, cfg) == []
    eight = seven + [_cluster_stub(299, 1.0, 1.3, cfg)]
    pk = phase8.detect_pockets(eight, cfg)
    assert len(pk) == 1 and len(pk[0].member_ids) == 8


def test_pocket_detection_halo_absorbs_edge_straggler(cfg):
    """Detection halo (§4.1): a straggler whose tip voxel only touches the pocket
    diagonally (a separate 6-connected component) is still absorbed by the
    1-voxel dilation -- membership can't flip on a sub-voxel re-aim. A 9-tip block
    fills voxel (0,0,0); a straggler at voxel (1,1,0) (edge-diagonal, not
    6-connected) joins via the halo."""
    block = [_cluster_stub(300 + i, 0.5 + 0.4 * (i % 3), 0.5 + 0.4 * (i // 3), cfg)
             for i in range(9)]                        # all in voxel (0,0,0)
    for s in block:
        assert phase8.voxel_key(phase8._site_tip(s), cfg.p8_pocket_voxel_mm)[:2] == (0, 0)
    straggler = _cluster_stub(399, 7.0, 7.0, cfg)      # tip voxel (1,1,z): diagonal
    tv = phase8.voxel_key(phase8._site_tip(straggler), cfg.p8_pocket_voxel_mm)
    assert tv[:2] == (1, 1)                             # not 6-connected to (0,0)
    pk = phase8.detect_pockets(block + [straggler], cfg)
    assert len(pk) == 1 and 399 in pk[0].member_ids     # halo pulled it in


def test_pocket_detection_three_clusters_labeled_lexicographically(cfg):
    """Three separated dense clusters -> three pockets (the P1/P2/P3 shape),
    labeled in lexicographic voxel order (deterministic §4.1). Each member lands
    in exactly one pocket; the far-apart clusters share no member."""
    c1 = _dense_cluster(cfg, base_mi=400, origin=(1.0, 1.0))
    c2 = _dense_cluster(cfg, base_mi=500, origin=(31.0, 1.0))
    c3 = _dense_cluster(cfg, base_mi=600, origin=(1.0, 31.0))
    pk = phase8.detect_pockets(c1 + c2 + c3, cfg)
    assert len(pk) == 3
    assert [p.label for p in pk] == [0, 1, 2]           # dense-order labels
    sizes = {len(p.member_ids) for p in pk}
    assert sizes == {9}
    allmem = [m for p in pk for m in p.member_ids]
    assert len(allmem) == len(set(allmem)) == 27        # partition, no overlap


# ---- conflict graph (§4.2) -------------------------------------------------
def test_conflict_graph_envelope_overlap_edges(cfg):
    """Conflict graph G (§4.2): an edge iff reachable envelopes overlap. Two
    adjacent cluster members share an edge; a far member shares none. Edges are
    stored as sorted (min mid, max mid) pairs (determinism)."""
    near = [_cluster_stub(700, 1.0, 1.0, cfg), _cluster_stub(701, 1.4, 1.0, cfg)]
    far = _cluster_stub(702, 40.0, 40.0, cfg)
    env, adj, edges = phase8.build_conflict_graph(near + [far], cfg)
    assert (700, 701) in edges                          # adjacent -> edge
    assert 702 not in adj[700] and 702 not in adj[701]  # far -> no edge
    assert adj[702] == set()
    assert all(a < b for a, b in edges)                 # sorted pairs
    assert list(edges) == sorted(edges)                 # deterministic order


def test_conflict_graph_neighbourhood_is_full_dependency_set(cfg):
    """N(m) is the COMPLETE set of members whose legality could change when m is
    fixed (§4.2). In a tightly packed cluster every pair's envelopes overlap, so
    G is complete -- the full dependency set the §4.4 invalidation rule uses."""
    sites = _dense_cluster(cfg, base_mi=100)
    _env, adj, edges = phase8.build_conflict_graph(sites, cfg)
    n = len(sites)
    assert len(edges) == n * (n - 1) // 2               # complete graph
    assert all(len(adj[s.site_id]) == n - 1 for s in sites)


# ---- density penalty (§4.2) ------------------------------------------------
def test_density_penalty_counts_corridor_neighbours(cfg):
    """density_penalty (§4.2) counts OTHER members whose corridor a candidate
    shaft passes within p8_pocket_corridor_pad_mm of. A near neighbour is
    counted; a far member is 0 (so an isolated site's tuple slot degrades to the
    sparse ordering, §4.6)."""
    A = _cluster_stub(1, 1.0, 1.0, cfg)
    B = _cluster_stub(2, 1.3, 1.0, cfg)                 # within pad
    far = _cluster_stub(3, 50.0, 50.0, cfg)
    cand0 = phase8._member_candidates(A, cfg)[0]
    near_cor = {1: phase8._corridor_of(phase8._original_member(A)),
                2: phase8._corridor_of(phase8._original_member(B))}
    assert phase8._density_count(1, cand0, near_cor, cfg) == 1
    far_cor = {1: near_cor[1],
               3: phase8._corridor_of(phase8._original_member(far))}
    assert phase8._density_count(1, cand0, far_cor, cfg) == 0


# ---- atomic-commit loop (§4.3-4.5) -----------------------------------------
def test_pocket_convergence_dense_cluster_terminates(cfg):
    """CONVERGENCE / TERMINATION (§4.5) on a synthetic DENSE cluster: the loop
    runs <= |pocket| iterations, every member reaches a terminal state
    (COMMITTED or FALLEN, none left UNRESOLVED), and it halts. The dense 9-stub
    canopy pocket resolves fully via shortening (all clear the fin)."""
    geom = ExactSurface(_pocket_mesh())
    sites = _dense_cluster(cfg)
    rep = phase8.run_pocket_solver(sites, geom, cfg)
    assert len(rep.pockets) == 1
    r = rep.pockets[0]
    assert r.n_iterations <= len(r.member_ids)          # <= |pocket| commits
    assert r.n_committed + r.n_fallen == len(r.member_ids)  # all terminal
    assert r.n_committed == 9 and r.n_fallen == 0        # dense cluster resolves
    # every committed member is independently legal on the verify gate
    for mr in r.members.values():
        wm = mr.committed_cand.member
        ivw = geom.penetration_intervals(wm["p0"], wm["p1"], wm["r0"], wm["r1"])
        vdw = verify._embed_scar_verdicts(geom, [wm], [0], {0: ivw}, cfg)
        assert verify._transit_classify(ivw, wm, vdw.get(0)) == (False, False)


def test_pocket_monotone_objective_no_thrash(cfg):
    """NO-THRASH / MONOTONE OBJECTIVE (§4.5): the UNRESOLVED count strictly
    DECREASES by exactly one per iteration -- the resolved-count objective is
    strictly monotone, not merely bounded, so there is no sweep cap and no
    oscillation. objective_trace is strictly decreasing and has one entry per
    commit iteration."""
    geom = ExactSurface(_pocket_mesh())
    r = phase8.run_pocket_solver(_dense_cluster(cfg), geom, cfg).pockets[0]
    tr = r.objective_trace
    assert len(tr) == r.n_iterations
    assert all(tr[i] > tr[i + 1] for i in range(len(tr) - 1))   # strictly decreasing
    assert all(tr[i] - tr[i + 1] == 1 for i in range(len(tr) - 1))  # by exactly one
    assert tr[-1] == 0                                   # fully drained


def test_pocket_commit_priority_worst_chord_then_mid(cfg):
    """Commit priority (§4.3-2): worst-firm-chord DESC, then mid ASC -- a total
    order. With strictly descending frozen chords by construction, the commit
    order is exactly the descending-chord order."""
    geom = ExactSurface(_pocket_mesh())
    sites = _dense_cluster(cfg, base_mi=100)             # wfc 0.30, 0.299, ... desc by mi
    r = phase8.run_pocket_solver(sites, geom, cfg).pockets[0]
    wfc = {s.site_id: s.frozen_worst_chord_mm for s in sites}
    expected = tuple(sorted(wfc, key=lambda i: (-wfc[i], i)))
    assert r.commit_order == expected


def test_pocket_determinism_bit_identical(cfg):
    """DETERMINISM (§4.5): no RNG, no wall-clock. Two runs produce bit-identical
    commit order, dispositions, and committed member geometry -- the bedrock of
    the 5-run acceptance digest."""
    geom = ExactSurface(_pocket_mesh())
    sites = _dense_cluster(cfg)
    a = phase8.run_pocket_solver(sites, geom, cfg).pockets[0]
    b = phase8.run_pocket_solver(sites, geom, cfg).pockets[0]
    assert a.commit_order == b.commit_order
    assert a.objective_trace == b.objective_trace
    assert a.invalidations == b.invalidations
    for mid in a.member_ids:
        pa = a.members[mid].committed_cand.member["p1"]
        pb = b.members[mid].committed_cand.member["p1"]
        assert pa.tobytes() == pb.tobytes()             # bit-identical, not merely close


def test_pocket_idempotence_second_run_noop(cfg):
    """IDEMPOTENCE (§6.10): the solver is a pure function of geometry, so a
    second run over the same input reproduces the same report exactly -- the
    no-op-on-already-fixed-geometry gate at the solver level (dispositions and
    per-member scores identical)."""
    geom = ExactSurface(_pocket_mesh())
    sites = _dense_cluster(cfg)
    r1 = phase8.run_pocket_solver(sites, geom, cfg)
    r2 = phase8.run_pocket_solver(sites, geom, cfg)
    assert r1.dispositions == r2.dispositions
    for mid in r1.dispositions:
        s1 = r1.pockets[0].members[mid].score
        s2 = r2.pockets[0].members[mid].score
        assert s1 == s2


# ---- mutual-legality guard + full-dependency invalidation (§4.3-4.4) -------
def _guard_reject_vertical_when_committed(cand_member, committed, cfg):
    """A §5-law stand-in: once ANY neighbour is committed, forbid the candidate's
    vertical (polar-0) shaft -- as if the vertical corridor were now occupied.
    Forces the neighbour off its lex-min onto a lean."""
    if committed:
        n_hat = -np.asarray(cand_member["axis_u"], float)
        if float(n_hat[2]) > 0.99:
            return False
    return True


def test_pocket_mutual_guard_pushes_off_lexmin(cfg):
    """MUTUAL-LEGALITY guard (§4.3-3-ii): a committed neighbour forbids a
    member's lex-min candidate, so the atomic commit walks legal_sorted to the
    first candidate that passes the guard -- committing a NON-lex-min candidate
    rather than illegalising the committed set. solve_pocket is exercised
    directly (the loop mechanics, below the >=8 detection threshold)."""
    geom = ExactSurface(_pocket_mesh())
    A = _cluster_stub(1, 1.0, 1.0, cfg, wfc=0.30)       # commits first (higher chord)
    B = _cluster_stub(2, 1.5, 1.0, cfg, wfc=0.20)       # neighbour, pushed off vertical
    r = phase8.solve_pocket([A, B], geom, cfg,
                            mutual_legal_fn=_guard_reject_vertical_when_committed)
    assert (1, 2) in r.edges                             # they conflict
    assert r.members[1].committed_cand.polar_deg == 0.0  # A keeps the lex-min vertical
    assert r.members[2].status == phase8.PocketStatus.COMMITTED
    assert r.members[2].committed_cand.polar_deg > 0.0   # B pushed to a lean
    # soundness (§4.4): the final committed set satisfies the guard
    assert _guard_reject_vertical_when_committed(
        r.members[2].committed_cand.member, [r.members[1].committed_cand.member], cfg)


def test_pocket_mutual_guard_exhaustion_falls_to_r2(cfg):
    """When every candidate of a member conflicts with the committed set, it is
    FALLEN -- it leaves the pocket loop to SM-STUB R2 (§4.3-3), never
    force-committed illegally. The higher-priority member still commits; the
    committed set stays mutually legal (soundness §4.4)."""
    geom = ExactSurface(_pocket_mesh())
    A = _cluster_stub(1, 1.0, 1.0, cfg, wfc=0.30)
    B = _cluster_stub(2, 1.5, 1.0, cfg, wfc=0.20)

    def reject_all_once_committed(cand_member, committed, cfg):
        return not committed                            # any committed neighbour -> reject

    r = phase8.solve_pocket([A, B], geom, cfg, mutual_legal_fn=reject_all_once_committed)
    assert r.members[1].status == phase8.PocketStatus.COMMITTED
    assert r.members[2].status == phase8.PocketStatus.FALLEN
    assert r.members[2].escalate_rung == "R2"           # stub -> R2 (not quarantine)
    assert r.n_committed == 1 and r.n_fallen == 1


def test_pocket_full_dependency_set_invalidation(cfg):
    """FULL dependency-set invalidation (§4.4, amendment F): when a member
    commits, EVERY still-UNRESOLVED graph neighbour is marked dirty and
    re-evaluated -- NOT merely the members whose current best ray overlaps the
    committed shaft (the rejected candidate-ray-only rule). On the complete
    conflict graph of a dense cluster, the first commit invalidates ALL other
    unresolved members."""
    geom = ExactSurface(_pocket_mesh())
    sites = _dense_cluster(cfg)                          # complete conflict graph
    r = phase8.run_pocket_solver(sites, geom, cfg).pockets[0]
    first_commit, first_dirty = r.invalidations[0]
    ids = set(r.member_ids)
    # the first commit's dirty set == ALL other members (they were all UNRESOLVED)
    assert set(first_dirty) == ids - {first_commit}
    # each successive commit invalidates exactly the still-UNRESOLVED remainder
    committed_so_far = {first_commit}
    for m, dirty in r.invalidations:
        still_unresolved = ids - committed_so_far - {m}
        assert set(dirty) == still_unresolved
        committed_so_far.add(m)


def test_pocket_committed_set_mutually_legal_default_guard(cfg):
    """SOUNDNESS (§4.4) with the DEFAULT geometric guard: at termination no two
    committed members firm-transit each other (the invariant the mutual guard
    maintains inductively -- the §6.9 zero-NEW-firm-transit gate enforced locally
    at each commit). Verified independently against the serialized committed
    geometry (Codex acceptance addition G)."""
    geom = ExactSurface(_pocket_mesh())
    r = phase8.run_pocket_solver(_dense_cluster(cfg), geom, cfg).pockets[0]
    committed = [mr.committed_cand.member for mr in r.members.values()
                 if mr.status == phase8.PocketStatus.COMMITTED]
    for i in range(len(committed)):
        for j in range(i + 1, len(committed)):
            assert phase8._default_mutual_legal(committed[i], [committed[j]], cfg)


# ---- isolated members (§4.6) -----------------------------------------------
def test_pocket_isolated_members_use_plain_ladder(cfg):
    """§4.6: a member in NO pocket component runs the plain per-site R1 ladder
    with density_penalty 0 (the interaction machinery is pocket-only; sparse
    sites pay nothing). A lone stub is reported under `isolated`, not `pockets`,
    and its winning score carries density slot 0."""
    geom = ExactSurface(_pocket_mesh())
    lone = _cluster_stub(9001, 0.0, 0.0, cfg)
    rep = phase8.run_pocket_solver([lone], geom, cfg)
    assert rep.pockets == []                             # no pocket formed
    assert 9001 in rep.isolated
    r = rep.isolated[9001]
    assert r.winner is not None and r.score[1] == 0      # density slot 0 (sparse)
    assert rep.dispositions[9001] == "ISOLATED_RESOLVED"


def test_pocket_mixed_isolated_and_pocket(cfg):
    """A dense cluster (a pocket) plus a far-away lone stub (isolated): the
    solver routes each correctly -- the cluster through the atomic-commit loop,
    the straggler through the plain ladder -- and the two partitions are
    disjoint and complete."""
    geom = ExactSurface(_pocket_mesh())
    cluster = _dense_cluster(cfg, base_mi=100)
    lone = _cluster_stub(9001, 45.0, 45.0, cfg)
    rep = phase8.run_pocket_solver(cluster + [lone], geom, cfg)
    assert len(rep.pockets) == 1
    assert set(rep.pockets[0].member_ids) == {s.site_id for s in cluster}
    assert set(rep.isolated) == {9001}
    assert len(rep.dispositions) == len(cluster) + 1     # complete partition


def test_pocket_solver_batched_not_per_candidate(cfg, monkeypatch):
    """BATCH-FIRST (§7): the seed and every dirty re-evaluation cross the batched
    kernel boundary (`verify._batch_axis_intervals`) ONCE per (ray-kind x
    eval-round) -- never a per-candidate scalar probe. Pinned by spying on that
    boundary: the number of batched interval calls is bounded by (1 seed +
    <=|pocket| dirty re-eval rounds) x ray-kinds, FAR below the ~675
    (9 members x <=75 candidates) a per-candidate loop would issue."""
    geom = ExactSurface(_pocket_mesh())
    sites = _dense_cluster(cfg)                          # all stubs -> one ray-kind
    calls = {"n": 0}
    orig = verify._batch_axis_intervals

    def counting(*a, **k):
        calls["n"] += 1
        return orig(*a, **k)

    monkeypatch.setattr(verify, "_batch_axis_intervals", counting)
    phase8.run_pocket_solver(sites, geom, cfg)
    # 1 seed + at most |pocket| dirty re-eval rounds (one ray-kind: all stubs),
    # each a single batched interval call -- nowhere near one-per-candidate.
    assert calls["n"] <= 1 + len(sites)
