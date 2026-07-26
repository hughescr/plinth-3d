"""Consumer-level negative controls for the two v5 hard gates that a prior
pass DETECTED but never WIRED (see SPEC-V5-GEOM sec 2.2/2.1, test plan 6.1/6.2):

  1. Ball-breach is a HARD gate: verify.self_verify's `ok` must go False when
     a joint ball breaches a sub-mm feature -- and the builders (cco
     model-anchor, bracing model-rung) must stop CREATING such anchors so the
     gate is satisfiable. (R3, 2026-07-23: the gate's SCOPE is pinned to the
     census's own antipodal-span reference -- actual joint balls only. A
     synthesized embedded end-cap sphere breach is still detected and
     reported (endcap_breach_diagnostic) but is report-only, not gating
     -- see the R3 test block below for why.)
  2. Stub through-transit: BOTH independent clearance implementations
     (tree.Forest.revalidate AND verify.clearance_report) must flag a stub whose
     shaft welds through a thin fin OR buries its free end inside the model
     body, while still certifying a sanctioned tip embed. (The former
     p1-interior exemption was removed 2026-07-23: the full-scale treant
     EMITTED buried-free-end stubs -- the 4->15 illegal-member regression the
     independent census caught -- so a buried end is condemned, not trusted to
     the unrouted path.)

These are negative controls on the CONSUMERS (the oracle-level detectors are
covered by test_geometry_exact.py). Without them, a future edit could silently
un-wire either gate -- exactly the two blockers this file guards -- on green
fixtures. All fixtures deterministic, no unseeded sampling.
"""
import numpy as np
import trimesh
import pytest

from support_placement import verify, cco, routing
from support_placement import tree as tm
from support_placement.geometry_exact import ExactSurface
from support_placement.config import SupportConfig


# --------------------------------------------------------------------------
# deterministic triangle-soup fixtures (mirrors test_geometry_exact._box/_soup)
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


def _m(p0, p1, r0, r1, kind, owner, embed_end=None):
    return dict(p0=np.asarray(p0, float), p1=np.asarray(p1, float),
                r0=float(r0), r1=float(r1), kind=kind, owner=owner,
                embed_end=embed_end)


@pytest.fixture
def cfg():
    return SupportConfig()


# ==========================================================================
# Blocker 1 -- ball-breach hard gate (SPEC sec 2.2, test 6.1 / 6.14)
# ==========================================================================
def test_self_verify_hard_gates_on_ball_breach(cfg):
    """A joint ball that engulfs a 0.08mm twig and pokes out the far side
    (census ball-715 class) MUST drive self_verify's `ok` to False -- and the
    ball-breach gate MUST be what flips it (with the gate off, the identical
    breach is non-gating). Everything else is clean by construction (no
    islands/members/forest), so `ok` is a pure function of this gate."""
    twig = _box((6, 6, 0.08))
    geom = ExactSurface(twig)
    balls = [dict(center=np.array([0.0, 0.0, 0.04]), r=1.0, owner=("joint", 0))]

    rep = verify.self_verify(twig, [], [], [], np.array([]), np.array([]),
                             cfg, 0.0, geom=geom, balls=balls)
    assert len(rep["ball_breach_violations"]) == 1
    assert rep["ok"] is False                     # HARD gate now wired on

    rep_off = verify.self_verify(twig, [], [], [], np.array([]), np.array([]),
                                 cfg, 0.0, geom=geom, balls=balls,
                                 gate_on_ball_breach=False)
    assert len(rep_off["ball_breach_violations"]) == 1
    assert rep_off["ok"] is True                  # the gate is exactly what flips ok


def test_self_verify_passes_sanctioned_shallow_anchor(cfg):
    """The inverse guard (census 150/194 class): a r=1.0 ball anchored shallow
    on a 5mm-thick slab is a sanctioned anchor, not a breach -- `ok` stays
    True. Proves the gate does not simply reject every surface-reaching ball."""
    slab = _box((20, 20, 5))
    geom = ExactSurface(slab)
    balls = [dict(center=np.array([0.0, 0.0, 2.5]), r=1.0, owner=("joint", 0))]
    rep = verify.self_verify(slab, [], [], [], np.array([]), np.array([]),
                             cfg, 0.0, geom=geom, balls=balls)
    assert rep["ball_breach_violations"] == []
    assert rep["ok"] is True


def test_ball_breach_report_synthesizes_endcap_for_ball_less_embed(cfg):
    """SPEC sec 2.2 amendment 5 / test 6.14: a ball-LESS strut whose p1 embed
    terminates inside a 0.3mm fin (poke-out > 0.05) MUST be flagged via the
    synthesized end-cap sphere; the same strut into a 5mm slab MUST pass."""
    fin = _box((6, 6, 0.3))                        # z in [-0.15, 0.15]
    geom = ExactSurface(fin)
    strut = _m([0, 0, 3.0], [0, 0, 0.15], 0.6, 0.6, "strut", ("contact", 0),
               embed_end="p1")
    viol = verify.ball_breach_report(geom, [strut], [], cfg)
    assert len(viol) == 1
    assert viol[0]["source"] == "endcap" and viol[0]["end"] == "p1"

    slab = _box((20, 20, 5), center=(0, 0, -2.5))  # top face z=0
    g2 = ExactSurface(slab)
    strut_ok = _m([0, 0, 3.0], [0, 0, -0.3], 0.6, 0.6, "strut", ("contact", 0),
                  embed_end="p1")
    assert verify.ball_breach_report(g2, [strut_ok], [], cfg) == []


def test_ball_breach_report_uses_true_emitted_stub_ball(cfg):
    class CaptureGeom:
        def sphere_breach(self, centers, radii, eps_mm):
            self.centers = centers.copy()
            self.radii = radii.copy()
            return (np.zeros(len(centers), dtype=bool),
                    np.zeros(len(centers)), np.zeros(len(centers)))

    p0 = np.array([1.0, -2.0, 3.0])
    p1 = np.array([4.0, 2.0, 15.0])
    r0 = 0.3
    stub = _m(p0, p1, r0, 0.5, "stub", ("contact", 0), embed_end="p0")
    geom = CaptureGeom()
    assert verify.ball_breach_report(geom, [stub], [], cfg) == []

    u = (p1 - p0) / np.linalg.norm(p1 - p0)
    radius = cfg.tip_ball_r_factor * r0
    assert geom.centers.shape == (1, 3)
    assert np.allclose(geom.centers[0], p0 + radius * u)
    assert geom.radii[0] == pytest.approx(radius)


# ==========================================================================
# R3 (2026-07-23) -- ball-gate census-parity scope: self_verify's HARD gate
# and its `ball_breach_violations` key are restricted to actual joint balls
# (the census's own reference antipodal-span scope); the embedded end-cap
# check (amendment 5, tests above) is NOT deleted -- still computed, still
# surfaced -- but moves to a report-only `endcap_breach_diagnostic` key
# that never gates `ok`. Regression sites: the real Weeping Treant forest
# (acceptance run 2026-07-23) had
# ball_breach_report return 199 (15 ball + 184 endcap); the independent
# census's antipodal-span test agrees with the engine EXACTLY on the ball
# category (15 == 15) and never evaluates the endcap category at all -- see
# scripts/tools/validate_r3_ball_gate.py for the full-scale reconstruction.
# ==========================================================================
def test_self_verify_does_not_gate_on_endcap_breach(cfg):
    """The amendment-5 endcap breach (test_ball_breach_report_synthesizes_
    endcap_for_ball_less_embed's exact fixture, run through self_verify this
    time -- p0 lifted to plate_z and a surface_tree supplied so connectivity
    passes by construction, isolating the ball-gate behaviour under test)
    must NOT fail `ok` -- it is a report-only diagnostic under R3, not the
    ball gate's namesake. It must also not be silently dropped: it still
    shows up, under endcap_breach_diagnostic."""
    fin = _box((6, 6, 0.3))                         # z in [-0.15, 0.15]
    geom = ExactSurface(fin)
    stree = routing.build_surface_tree(fin)
    strut = _m([0, 0, 3.0], [0, 0, 0.15], 0.6, 0.6, "strut", ("contact", 0),
               embed_end="p1")
    rep = verify.self_verify(fin, [], [], [strut], np.array([]), np.array([]),
                             cfg, 3.0, geom=geom, balls=[], surface_tree=stree)
    assert rep["ball_breach_violations"] == []
    assert len(rep["endcap_breach_diagnostic"]) == 1
    assert rep["endcap_breach_diagnostic"][0]["source"] == "endcap"
    assert rep["connectivity_violations"] == [] and rep["clearance_violations"] == []
    assert rep["ok"] is True                        # endcap alone does not gate


def test_self_verify_ball_gate_mixed_attributes_failure_to_true_breach_only(cfg):
    """Both categories present simultaneously (a true joint-ball breach +
    the amendment-5 endcap fixture's member, layered on the SAME thin fin
    geometry): `ok` goes False because of the TRUE (ball) breach -- proving
    the endcap category is not what's driving the failure -- and each
    violation lands in its own bucket, neither silently merged nor
    dropped."""
    fin = _box((6, 6, 0.3))                         # z in [-0.15, 0.15]
    geom = ExactSurface(fin)
    stree = routing.build_surface_tree(fin)
    balls = [dict(center=np.array([2.0, 2.0, 0.15]), r=1.0, owner=("joint", 0))]
    strut = _m([0, 0, 3.0], [0, 0, 0.15], 0.6, 0.6, "strut", ("contact", 0),
               embed_end="p1")

    rep = verify.self_verify(fin, [], [], [strut], np.array([]), np.array([]),
                             cfg, 3.0, geom=geom, balls=balls, surface_tree=stree)
    assert len(rep["ball_breach_violations"]) == 1
    assert rep["ball_breach_violations"][0]["source"] == "ball"
    assert len(rep["endcap_breach_diagnostic"]) == 1
    assert rep["endcap_breach_diagnostic"][0]["source"] == "endcap"
    assert rep["ok"] is False

    # Turning gate_on_ball_breach off drops the TRUE breach's contribution
    # to `ok` too (same inert-toggle contract as the existing gate test);
    # the endcap category was already never gated, so nothing else changes.
    rep_off = verify.self_verify(fin, [], [], [strut], np.array([]), np.array([]),
                                 cfg, 3.0, geom=geom, balls=balls, surface_tree=stree,
                                 gate_on_ball_breach=False)
    assert len(rep_off["ball_breach_violations"]) == 1
    assert len(rep_off["endcap_breach_diagnostic"]) == 1
    assert rep_off["ok"] is True


def test_cco_model_anchor_builder_rejects_thin_twig_anchor(cfg):
    """Builder-side half of the fix (SPEC sec 2.3): cco's model-anchor build_fn
    must REJECT (raise _InfeasibleCandidate) an anchor whose end-cap ball would
    breach a 0.3mm twig, so the trial machinery never commits a member the
    verify gate would then fail. The same anchor on a 5mm slab builds fine."""
    twig = _box((6, 6, 0.3))                       # top face z=0.15
    geom = ExactSurface(twig)
    forest = tm.Forest()
    stub = tm.Member(p0=[0, 0, 3.3], p1=[0, 0, 3.0], r0=0.3, r1=0.3,
                     kind="stub", owner=("contact", 0), embed_end="p0")
    stub_idx = forest.add_member(stub)
    se = np.array([0.0, 0.0, 3.0])
    anchor = np.array([0.0, 0.0, 0.15])            # on the thin twig -> breach
    build = cco._make_model_anchor_builder(forest, 0, stub_idx, se, 0.3, 1.0,
                                           anchor, cfg, geom=geom)
    with pytest.raises(cco._InfeasibleCandidate):
        build()

    slab = _box((20, 20, 5), center=(0, 0, -2.5))  # top face z=0
    g2 = ExactSurface(slab)
    forest2 = tm.Forest()
    stub2 = tm.Member(p0=[0, 0, 3.3], p1=[0, 0, 3.0], r0=0.3, r1=0.3,
                      kind="stub", owner=("contact", 0), embed_end="p0")
    s2 = forest2.add_member(stub2)
    build_ok = cco._make_model_anchor_builder(forest2, 0, s2, se, 0.3, 1.0,
                                              np.array([0.0, 0.0, 0.0]), cfg, geom=g2)
    assert build_ok() > 0.0                        # builds; returns member volume


# ==========================================================================
# Blocker 2 -- stub through-transit rule, BOTH implementations (SPEC sec 2.1)
# ==========================================================================
def _through_fin_mesh():
    """Emergence slab (top z=0) + a floating 0.5mm fin at z in [1.0,1.5]. A
    vertical stub tip-embedded in the slab and ending at z=2.5 welds THROUGH
    the fin (census member-357 class), ending in open air above it."""
    slab = _box((20, 20, 5), center=(0, 0, -2.5))          # top face z=0
    fin = _box((10, 10, 0.5), center=(0, 0, 1.25))         # z in [1.0, 1.5]
    return _soup(slab, fin)


def _through_fin_stub():
    return dict(p0=np.array([0.0, 0.0, -0.3]),   # tip 0.3mm inside the slab
                p1=np.array([0.0, 0.0, 2.5]),    # stub_end in open air above fin
                r0=0.2, r1=0.35, kind="stub", owner=("contact", 0), embed_end="p0")


def _clean_embed_stub():
    return dict(p0=np.array([0.0, 0.0, -0.3]), p1=np.array([0.0, 0.0, 2.5]),
                r0=0.2, r1=0.35, kind="stub", owner=("contact", 0), embed_end="p0")


def test_revalidate_flags_through_transit_stub(cfg):
    """Implementation #1 (tree.Forest.revalidate): a stub welding through the
    fin fails certification; a sanctioned embed (same stub, no fin) certifies
    clean."""
    geom = ExactSurface(_through_fin_mesh())
    forest = tm.Forest()
    s = _through_fin_stub()
    forest.add_member(tm.Member(**s))
    viol = forest.revalidate(geom, cfg)
    assert [v["member_idx"] for v in viol] == [0]
    assert forest.dirty_members() == [0]           # stays uncertified (hard)

    clean_geom = ExactSurface(_box((20, 20, 5), center=(0, 0, -2.5)))
    f2 = tm.Forest()
    f2.add_member(tm.Member(**_clean_embed_stub()))
    assert f2.revalidate(clean_geom, cfg) == []
    assert f2.dirty_members() == []                # certified


def test_clearance_report_flags_through_transit_stub(cfg):
    """Implementation #2 (verify.clearance_report, independently coded): same
    verdicts as revalidate on the same geometry, and self_verify's `ok` goes
    False on the through-transit stub."""
    geom = ExactSurface(_through_fin_mesh())
    rep = verify.clearance_report(None, [_through_fin_stub()], cfg, geom=geom)
    assert [v["member_idx"] for v in rep] == [0]
    assert rep[0]["kind"] == "stub"

    full = verify.self_verify(_through_fin_mesh(), [], [], [_through_fin_stub()],
                              np.array([]), np.array([]), cfg, -10.0, geom=geom)
    assert len(full["clearance_violations"]) == 1
    assert full["ok"] is False

    clean_geom = ExactSurface(_box((20, 20, 5), center=(0, 0, -2.5)))
    assert verify.clearance_report(None, [_clean_embed_stub()], cfg,
                                   geom=clean_geom) == []


def test_buried_deep_plug_stub_adjudicated_legal_by_scar_gate(cfg):
    """ADJUDICATION (SPEC-EMBED-POLICY, owner-ratified cd989fe): a stub tip-
    embedded in THICK solid whose free end (p1) is buried deeper -- a pure deep
    plug that never exits -- is a SINGLE tip-anchored interval with a SMALL
    surface-crossing patch. Under SCAR-AREA-CAP there is NO depth cap on the
    tip-anchored interval ('a deep perpendicular plug into thick solid has a
    small crossing patch and no further physical consequence' -- the owner's
    idempotence ruling), so the verify gate now deems it LEGAL. This is the
    ratified reversal of the old arc-inset flag; the independent census agrees
    (its 193 illegal members are ALL through-and-through transits, none a pure
    buried plug).

    Intended builder/verify DIVERGENCE this round: tree.Forest.revalidate is the
    PLACEMENT side, still on the retired arc-inset window (untouched this round,
    slated for the placement-rebuild round), so it STILL flags the plug; the
    verify gate, on the ratified SCAR-AREA-CAP policy, does not. Reconciling the
    two is the placement-rebuild round's job."""
    slab = _box((20, 20, 10), center=(0, 0, 0))    # solid z in [-5, 5]
    geom = ExactSurface(slab)
    buried = dict(p0=np.array([0.0, 0.0, 4.7]),    # tip 0.3 inside top face z=5
                  p1=np.array([0.0, 0.0, 2.0]),    # stub_end still deep inside
                  r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                  embed_end="p0")
    assert bool(geom.inside(np.array([buried["p1"]]))[0])   # free end interior
    # PLACEMENT side (unchanged arc-inset) still flags -- the divergence.
    f = tm.Forest()
    f.add_member(tm.Member(**buried))
    assert [v["member_idx"] for v in f.revalidate(geom, cfg)] == [0]
    assert f.dirty_members() == [0]                # stays uncertified (hard)
    # VERIFY gate on the ratified SCAR-AREA-CAP policy: legal deep plug, NOT
    # flagged (a single tip-anchored interval, small scar patch, thick solid).
    rep = verify.clearance_report(None, [buried], cfg, geom=geom)
    assert rep == []


# ==========================================================================
# Blocker 3 (R2) -- non-stub CAPSULE-GRAZE: axis clean, a capsule-offset ray
# grazes a thin wall (SPEC sec 2.1, census member 1138). The verify gate's
# segment_clearance is an axis-to-surface DISTANCE test, blind to a capsule
# surface that dips into a thin wall while the axis stays clear; the axis
# through-transit interval test is likewise empty. That strut slipped every
# check and was emitted (the acceptance census: n_capsule_clip_only=1,
# delivered_verify_clearance_violations=0). The gate must consult the capsule-
# offset rays for non-stubs -- the same evidence the census used.
# ==========================================================================
class _CapsuleGrazeGeom:
    """Duck-typed exact oracle returning CONTROLLED census-1138 evidence to
    ISOLATE the R2 wiring: the axis is clean (axis intervals EMPTY), one
    capsule-offset ray transits an UNANCHORED mid-strut span (a graze away from
    the p1 embed foot -- the census's 'unanchored capsule graze', flagged by
    SCAR-AREA-CAP rule 4 regardless of the tip verdict), and segment_clearance
    is >= 0 (the distance path is blind to the graze -- exactly the blind spot
    1138 exploited). Records the `rays` mode per penetration_intervals_batch call
    so a test can prove stubs stay axis-only while non-stubs go capsule-aware.
    query()/project() mirror the scalar+batch contracts of ExactSurface (a 1-D
    point -> scalar / (3,) outward normal)."""

    def __init__(self):
        self.rays_seen = []

    def query(self, pts, distance_upper_bound=np.inf, workers=None, k=1):
        pts = np.asarray(pts, float)
        if pts.ndim == 1:
            return 0.0, 0
        n = len(pts)
        return np.zeros(n), np.zeros(n, int)

    def project(self, pts):
        pts = np.asarray(pts, float)
        if pts.ndim == 1:
            return 0.0, pts.copy(), 0, np.array([0.0, 0.0, 1.0])
        n = len(pts)
        return (np.zeros(n), pts.copy(), np.zeros(n, int),
                np.tile([0.0, 0.0, 1.0], (n, 1)))

    def segment_clearance(self, p0s, p1s, r0s, r1s, standoff,
                          inset0s=None, inset1s=None):
        n = len(np.atleast_2d(np.asarray(p0s, float)))
        return np.full(n, 0.5), np.full((n, 3), np.nan)   # >= 0: distance path clean

    def penetration_intervals_batch(self, p0s, p1s, r0s, r1s, rays="axis",
                                    min_illegal_len_mm=0.05, **kw):
        self.rays_seen.append(rays)
        p0s = np.atleast_2d(np.asarray(p0s, float))
        p1s = np.atleast_2d(np.asarray(p1s, float))
        out = []
        for j in range(len(p0s)):
            L = float(np.linalg.norm(p1s[j] - p0s[j]))
            if rays == "all":            # census 1138: an offset ray grazes,
                # UNANCHORED -- a mid-strut span [0.3L, 0.8L], well clear of the
                # p1 foot (s=L), so rule 4 flags it whatever the tip verdict.
                out.append([dict(s_a=0.30 * L, s_b=0.80 * L, entered=True,
                                 exited=True, max_depth_mm=0.264, ray="off6")])
            else:                        # axis-only: the census-1138 axis is empty
                out.append([])
        return out


def test_r2_predicate_unanchored_capsule_graze_is_illegal():
    """SPEC-EMBED-POLICY rule 4 at the predicate (census-1138 class, mi=1138:
    strut, embed=p1, L=1.913). The axis interval is EMPTY so an axis-only test
    passes the member; the SAME predicate over an UNANCHORED capsule-offset
    interval (a mid-strut graze not reaching the p1 foot) flags it -- regardless
    of the tip verdict. An ANCHORED offset graze (reaching the foot, within TOL
    of s=L) is instead the sanctioned foot embed and is exempt when the verdict
    is legal."""
    L = 1.913
    strut = _m([0, 0, L], [0, 0, 0.0], 0.35, 0.35, "strut", ("contact", 0),
               embed_end="p1")
    # axis empty -> not illegal.
    assert verify._transit_illegal([], strut, True) is False
    # unanchored mid-strut offset graze -> illegal even with a LEGAL tip verdict.
    off_mid = [dict(s_a=0.5, s_b=1.2, entered=True, exited=True,
                    max_depth_mm=0.264, ray="off6")]
    assert verify._transit_illegal(off_mid, strut, True) is True
    # anchored offset graze reaching the p1 foot (s_b within TOL of L) -> the
    # sanctioned foot embed, exempt under a LEGAL verdict.
    off_foot = [dict(s_a=1.2, s_b=L - 0.01, entered=True, exited=False,
                     max_depth_mm=0.264, ray="off6")]
    assert verify._transit_illegal(off_foot, strut, True) is False
    # ... but an ILLEGAL tip verdict flags even the anchored foot interval.
    assert verify._transit_illegal(off_foot, strut, False) is True


def test_r4_near_tip_unanchored_transit_stub_is_illegal():
    """Census worst-class stubs (this run's mi=510/416): a stub with a SINGLE
    axis interval that starts JUST beyond the tip-anchor tolerance (s_a > TOL_MM)
    -- the solid begins a hair too far from the tip to anchor -- is an unanchored
    transit and ILLEGAL, no matter the SCAR verdict. The mirror sub-TOL case
    (s_a <= TOL_MM) anchors and, under a legal verdict, is exempt."""
    stub = _m([0, 0, 0.0], [0, 0, 2.8], 0.2, 0.35, "stub", ("contact", 0),
              embed_end="p0")
    unanchored = [dict(s_a=0.06, s_b=0.30, entered=True, exited=True,
                       max_depth_mm=0.01, ray="axis")]      # s_a 0.06 > TOL 0.05
    assert verify._transit_illegal(unanchored, stub, True) is True
    anchored = [dict(s_a=0.04, s_b=0.30, entered=True, exited=True,
                     max_depth_mm=0.01, ray="axis")]        # s_a 0.04 <= TOL
    assert verify._transit_illegal(anchored, stub, True) is False


def test_r4_mid_span_transit_after_legal_tip_is_illegal():
    """A designed embed with a LEGAL tip AND a separate mid-span transit: the
    tip interval is exempt, but the re-entry / mid-span interval is an
    unanchored transit -> the member is ILLEGAL (the through-transit stub class,
    e.g. a stub welding through a floating fin above its sanctioned embed)."""
    stub = _m([0, 0, -0.3], [0, 0, 2.5], 0.2, 0.35, "stub", ("contact", 0),
              embed_end="p0")
    ivs = [dict(s_a=0.0, s_b=0.30, entered=False, exited=True,
                max_depth_mm=0.3, ray="axis"),               # tip-anchored (exempt)
           dict(s_a=1.3, s_b=1.8, entered=True, exited=True,
                max_depth_mm=0.25, ray="axis")]              # mid-span transit
    assert verify._transit_illegal(ivs, stub, True) is True
    # with only the tip interval, the legal verdict exempts the member entirely.
    assert verify._transit_illegal(ivs[:1], stub, True) is False


def test_r2_clearance_report_flags_capsule_graze_strut(cfg):
    """verify._clearance_report_exact catches the census-1138 class end-to-end:
    a non-stub whose segment_clearance is clear (min_clearance_mm >= 0, NOT a
    distance violation) and whose axis is empty, but a capsule-offset ray grazes
    a thin wall UNANCHORED. Axis-only intervals deliver ZERO violations; the gate
    flags it by consulting rays='all' and applying rule 4."""
    geom = _CapsuleGrazeGeom()
    strut = _m([0, 0, 3.0], [0, 0, 0.0], 0.35, 0.35, "strut",
               ("contact", 72), embed_end="p1")
    rep = verify._clearance_report_exact([strut], cfg, geom)
    assert [v["member_idx"] for v in rep] == [0]
    assert rep[0]["kind"] == "strut"
    assert rep[0]["min_clearance_mm"] >= 0.0        # caught by capsule interval, not distance
    assert "all" in geom.rays_seen                  # capsule rays were consulted


def test_r2_capsule_awareness_does_not_leak_into_stubs(cfg):
    """R2 adds capsule-offset sensitivity to NON-STUB members ONLY. A stub hugs
    its own emergence surface, so a capsule-offset ray grazes THAT surface --
    flagging stubs on offset rays would re-fire the P11 over-quarantine class
    SCAR-AREA-CAP owns. Guard: with a geom whose offset rays ALL transit but
    whose axis is clean, the STRUT is flagged and the STUB is NOT, and the stub's
    intervals were requested axis-only."""
    geom = _CapsuleGrazeGeom()
    strut = _m([0, 0, 3.0], [0, 0, 0.0], 0.35, 0.35, "strut",
               ("contact", 1), embed_end="p1")
    stub = _m([0, 0, 3.3], [0, 0, 0.0], 0.2, 0.35, "stub",
              ("contact", 2), embed_end="p0")
    rep = verify._clearance_report_exact([strut, stub], cfg, geom)
    flagged = {v["member_idx"] for v in rep}
    assert 0 in flagged            # strut: capsule graze caught
    assert 1 not in flagged        # stub: axis-only, emergence-surface graze ignored
    assert "axis" in geom.rays_seen and "all" in geom.rays_seen


def test_r2_capsule_offset_ray_surfaces_transit_axis_misses():
    """The census evidence R2 consults, on a REAL exact oracle: a strut whose
    AXIS runs clean beside a 0.3mm fin while a capsule-offset ray (at radius r)
    grazes INTO it. rays='axis' sees nothing; rays='all' surfaces the offset
    transit (census-1138 shape: axis empty, an offset ray grazes). Proves the
    offset evidence the kernel already computes is exactly what the gate reads."""
    fin = _box((0.3, 10, 12), center=(0, 0, 5))      # x in [-0.15, 0.15]
    geom = ExactSurface(fin)
    p0 = np.array([0.4, 0.0, 8.0])                    # axis at x=0.4, outside the fin
    p1 = np.array([0.4, 0.0, 2.0])
    axis_iv = geom.penetration_intervals(p0, p1, 0.35, 0.35, rays="axis")
    all_iv = geom.penetration_intervals(p0, p1, 0.35, 0.35, rays="all")
    assert axis_iv == []                              # axis never enters the fin
    off = [iv for iv in all_iv if iv["ray"] != "axis"]
    assert off, "a capsule-offset ray must surface the graze the axis misses"
    assert any((iv["s_b"] - iv["s_a"]) > 0.05 for iv in off)


# ==========================================================================
# SCAR-AREA-CAP embed-legality (SPEC-EMBED-POLICY.md, ratified cd989fe) --
# the ported per-embed-end sanction that REPLACES the retired arc/depth inset.
# Pure closed-form pins on verify._scar_area_cap_legal, mesh-free and literal
# to the spec's normative constants (A_ABS_MM2=2.0, KAPPA=1.25,
# L_SCAR_MAX_MM=2.5). Each case isolates one legality rule so a future edit to a
# single term is caught. These are the spec's own documented numbers (mi=415
# grazing rib; the 85deg / 4.59mm long-gash; the KAPPA-lifted designed foot).
# ==========================================================================
def test_scar_area_cap_legal_grazing_rib_mi415_class():
    """A LEGAL grazing rib stub (this run's mi=415 verdict inputs): moderate
    angle, small scar patch -> LEGAL, mid-pack under the area lens. No depth cap
    on the tip-anchored interval."""
    legal, met = verify._scar_area_cap_legal(
        T=1.7644, d=0.053, cosang=0.913, r_x=0.2945, r_end=0.2, R_b=0.3)
    assert legal is True
    assert met["A_patch"] == pytest.approx(0.5723, abs=2e-3)
    assert met["A_patch"] <= met["A_max"]                       # rule 1
    assert met["L_scar"] == pytest.approx(0.6451, abs=2e-3)     # rule 2
    assert met["A_anchor"] >= np.pi * 0.2 ** 2                  # rule 3


def test_scar_area_cap_long_gash_fails_scar_dimension():
    """Rule 2 (scar max dimension): the spec's / Codex's 85deg long-gash example
    -- L_scar via 2*r_x/cos = 4.59mm exceeds L_SCAR_MAX_MM (2.5) -> ILLEGAL,
    even though its AREA (rule 1) is under A_ABS. A cos floor would have hidden
    exactly this grazing growth."""
    cos85 = float(np.cos(np.radians(85.0)))
    legal, met = verify._scar_area_cap_legal(
        T=5.0, d=0.05, cosang=cos85, r_x=0.2, r_end=0.2, R_b=0.3)
    assert met["L_scar"] == pytest.approx(4.59, abs=0.02)       # spec's 4.59mm
    assert met["A_patch"] <= met["A_max"]                        # rule 1 passes
    assert met["L_scar"] > 2.5                                   # rule 2 fails
    assert legal is False


def test_scar_area_cap_grazing_scar_over_area_fails_scar_cap():
    """Rule 1 (scar cap): a grazing stub whose shaft patch exceeds A_ABS
    (>2.0mm2) while its scar dimension stays within 2.5mm -> ILLEGAL via the
    area bound alone. A_ball can never breach A_max on its own (always within the
    KAPPA headroom), so a scar-cap violation is necessarily a shaft-area gouge."""
    legal, met = verify._scar_area_cap_legal(
        T=2.0, d=0.05, cosang=0.48, r_x=0.6, r_end=0.4, R_b=0.6)
    assert met["A_max"] == pytest.approx(2.0, abs=1e-9)          # small stub ball
    assert met["A_patch"] > 2.0                                  # rule 1 fails
    assert met["L_scar"] <= 2.5                                  # rule 2 passes
    assert legal is False


def test_scar_area_cap_detached_tip_fails_anchor_floor():
    """Rule 3 (anchor floor): a DETACHED tip -- no tip-anchored interval (T=0)
    and the surface farther than the ball radius (d > R_b) -- gives A_anchor = 0,
    below the pi*r_end^2 fuse-neck floor -> ILLEGAL. Depth checks were
    structurally blind in this direction."""
    legal, met = verify._scar_area_cap_legal(
        T=0.0, d=0.5, cosang=1.0, r_x=0.2, r_end=0.2, R_b=0.3)
    assert met["A_anchor"] == pytest.approx(0.0, abs=1e-12)
    assert met["A_anchor"] < np.pi * 0.2 ** 2                    # rule 3 fails
    assert met["A_patch"] <= met["A_max"] and met["L_scar"] <= 2.5
    assert legal is False


def test_scar_area_cap_large_designed_foot_legal_via_kappa():
    """The KAPPA lever (rule 1): a large designed strut foot (r_end=1.0 ->
    R_b=1.4) embeds a disc of ~6.1mm2 -- far above the flat 2.0mm2 allowance, but
    LEGAL by construction because A_max = KAPPA*pi*R_b^2 = 7.7mm2 scales with the
    designed ball. A flat cap below that would outlaw the sanctioned design."""
    legal, met = verify._scar_area_cap_legal(
        T=0.0, d=0.1, cosang=1.0, r_x=1.0, r_end=1.0, R_b=1.4)
    assert met["A_max"] == pytest.approx(1.25 * np.pi * 1.4 ** 2, rel=1e-9)
    assert met["A_patch"] > 2.0                                  # over the flat cap
    assert met["A_patch"] <= met["A_max"]                        # but under KAPPA cap
    assert legal is True


# ==========================================================================
# PRINT-FLOOR amendment (SPEC-EMBED-POLICY.md, OWNER RATIFIED 2026-07-24):
# a rule-4 UNANCHORED transit whose antipodal crossing chord
# (feature_thickness_mm = the straight-ray interval length s_b - s_a) is <
# PRINT_FLOOR_CHORD_MM (0.08mm) is downgraded from illegal to MOOT -- a
# foliage knife-edge that cannot print as coherent material. A member is
# EXEMPT (moot) iff ALL its rule-4 crossings are moot; NON-EXEMPT iff it has
# >=1 firm crossing (chord >= floor, or a SCAR-illegal tip). The hard gate
# fails ONLY on the non-exempt population; the moot bucket is reported
# separately and never gates `ok`. Predicate pins on verify._transit_classify
# / _transit_illegal plus end-to-end pins through clearance_report / self_verify.
# ==========================================================================
def test_printfloor_constant_pinned():
    """The moot threshold is pinned literal-identical to the census (a 20um
    guard band above the 0.0502mm archived-geometry population minimum)."""
    assert verify.PRINT_FLOOR_CHORD_MM == 0.08


def test_printfloor_firm_plus_small_crossing_is_nonexempt():
    """SITE-DERIVED (task fixture): a member with one 0.25mm-chord crossing AND
    one 0.05mm crossing is NON-EXEMPT -- the 0.25mm firm transit (>= 0.08 floor)
    makes the member fail the hard gate no matter how thin the other crossing is.
    The 0.05mm sliver is at/below MIN_ILLEGAL_LEN and moot either way; it never
    rescues a member that also has a firm crossing."""
    stub = _m([0, 0, -0.3], [0, 0, 2.5], 0.2, 0.35, "stub", ("contact", 0),
              embed_end="p0")
    ivs = [dict(s_a=0.0, s_b=0.30, entered=False, exited=True,
                max_depth_mm=0.3, ray="axis"),               # tip-anchored (exempt)
           dict(s_a=1.00, s_b=1.25, entered=True, exited=True,
                max_depth_mm=0.10, ray="axis"),              # 0.25mm FIRM transit
           dict(s_a=2.00, s_b=2.05, entered=True, exited=True,
                max_depth_mm=0.02, ray="axis")]              # 0.05mm sliver
    nonexempt, moot = verify._transit_classify(ivs, stub, True)
    assert nonexempt is True and moot is False
    assert verify._transit_illegal(ivs, stub, True) is True   # hard-gate predicate


def test_printfloor_all_crossings_below_floor_is_moot():
    """SITE-DERIVED (task fixture): a member whose EVERY rule-4 crossing is a
    sub-0.08mm knife-edge (0.06mm + 0.07mm) is MOOT -- exempt from the hard gate,
    but STILL reported (has_illegal, so `moot` is True, not (False, False))."""
    stub = _m([0, 0, -0.3], [0, 0, 2.5], 0.2, 0.35, "stub", ("contact", 0),
              embed_end="p0")
    ivs = [dict(s_a=0.0, s_b=0.30, entered=False, exited=True,
                max_depth_mm=0.3, ray="axis"),               # tip-anchored (exempt)
           dict(s_a=1.00, s_b=1.06, entered=True, exited=True,
                max_depth_mm=0.03, ray="axis"),              # 0.06mm moot
           dict(s_a=1.60, s_b=1.67, entered=True, exited=True,
                max_depth_mm=0.03, ray="axis")]              # 0.07mm moot
    nonexempt, moot = verify._transit_classify(ivs, stub, True)
    assert nonexempt is False and moot is True
    assert verify._transit_illegal(ivs, stub, True) is False  # not a hard violation


def test_printfloor_chord_at_floor_is_firm_not_moot():
    """Boundary pin: a crossing chord EXACTLY at the floor (0.08mm) is FIRM
    (moot requires chord STRICTLY < floor) -> non-exempt. Guards the comparison
    direction so a future `<=` typo is caught."""
    stub = _m([0, 0, -0.3], [0, 0, 2.5], 0.2, 0.35, "stub", ("contact", 0),
              embed_end="p0")
    ivs = [dict(s_a=1.00, s_b=1.08, entered=True, exited=True,
                max_depth_mm=0.04, ray="axis")]              # chord 0.08 == floor
    nonexempt, moot = verify._transit_classify(ivs, stub, True)
    assert nonexempt is True and moot is False


def test_printfloor_scar_illegal_tip_never_moot():
    """A SCAR-illegal tip interval is a rules-1-3 failure, NOT a rule-4 transit --
    it is never moot-eligible even when its chord is sub-floor. Same thin tip
    interval, verdict legal -> exempt (no illegal crossing); verdict ILLEGAL ->
    firm non-exempt (the tip is flagged), regardless of the 0.06mm chord."""
    stub = _m([0, 0, -0.3], [0, 0, 2.5], 0.2, 0.35, "stub", ("contact", 0),
              embed_end="p0")
    tip_thin = [dict(s_a=0.0, s_b=0.06, entered=False, exited=True,
                     max_depth_mm=0.06, ray="axis")]         # tip-anchored, chord 0.06
    assert verify._transit_classify(tip_thin, stub, True) == (False, False)   # legal -> exempt
    nonexempt, moot = verify._transit_classify(tip_thin, stub, False)         # verdict ILLEGAL
    assert nonexempt is True and moot is False


def _fin_stub():
    """A stub tip-embedded 0.3mm in the slab (sanctioned embed) whose shaft
    welds THROUGH a thin floating fin above it -- the fin crossing is the only
    unanchored rule-4 transit, so its chord alone decides moot vs firm."""
    return dict(p0=np.array([0.0, 0.0, -0.3]), p1=np.array([0.0, 0.0, 2.5]),
                r0=0.2, r1=0.35, kind="stub", owner=("contact", 0), embed_end="p0")


def _fin_mesh(fin_z):
    slab = _box((20, 20, 5), center=(0, 0, -2.5))              # top face z=0
    fin = _box((10, 10, fin_z), center=(0, 0, 1.0 + fin_z / 2))  # z in [1.0, 1.0+fin_z]
    return _soup(slab, fin)


def test_printfloor_moot_transit_surfaced_not_gated_end_to_end(cfg):
    """SITE-DERIVED end-to-end (the census through-transit stub class at a
    knife-edge fin): a stub welding through a 0.06mm fin above its sanctioned
    embed produces a rule-4 transit of chord 0.06 < floor. clearance_report's
    hard-gate list is EMPTY and the member surfaces in the moot bucket; the
    0.12mm-fin counterpart (chord 0.12 >= floor) is a FIRM hard violation with
    an empty moot bucket. Same geometry, only the fin thickness differs."""
    geom = ExactSurface(_fin_mesh(0.06))
    moot_out = []
    rep = verify.clearance_report(None, [_fin_stub()], cfg, geom=geom,
                                  moot_transit_out=moot_out)
    assert rep == []                                          # not a hard violation
    assert [v["member_idx"] for v in moot_out] == [0]         # reported as moot
    assert moot_out[0]["kind"] == "stub"

    geom2 = ExactSurface(_fin_mesh(0.12))
    moot2 = []
    rep2 = verify.clearance_report(None, [_fin_stub()], cfg, geom=geom2,
                                   moot_transit_out=moot2)
    assert [v["member_idx"] for v in rep2] == [0]             # firm hard violation
    assert moot2 == []                                        # nothing moot


def test_printfloor_self_verify_gate_ok_semantics(cfg):
    """self_verify gate-ok semantics: a moot-only transit (0.06mm fin) does NOT
    flip `ok` and lands in `moot_transit_members`; the firm counterpart (0.12mm
    fin) drives `ok` False via clearance_violations with an empty moot bucket.
    plate_z placed at the stub's free end (p1) and a surface_tree supplied so
    connectivity passes by construction, isolating the clearance gate under
    test (the only gate that changes with fin thickness)."""
    m1 = _fin_mesh(0.06)
    geom = ExactSurface(m1); st1 = routing.build_surface_tree(m1)
    rep = verify.self_verify(m1, [], [], [_fin_stub()],
                             np.array([]), np.array([]), cfg, 2.5,
                             geom=geom, surface_tree=st1)
    assert rep["clearance_violations"] == []
    assert [v["member_idx"] for v in rep["moot_transit_members"]] == [0]
    assert rep["connectivity_violations"] == []
    assert rep["ok"] is True                                  # moot never gates

    m2 = _fin_mesh(0.12)
    geom2 = ExactSurface(m2); st2 = routing.build_surface_tree(m2)
    rep2 = verify.self_verify(m2, [], [], [_fin_stub()],
                              np.array([]), np.array([]), cfg, 2.5,
                              geom=geom2, surface_tree=st2)
    assert [v["member_idx"] for v in rep2["clearance_violations"]] == [0]
    assert rep2["moot_transit_members"] == []
    assert rep2["ok"] is False                                # firm transit gates
