"""Consumer-level negative controls for the two v5 hard gates that a prior
pass DETECTED but never WIRED (see SPEC-V5-GEOM sec 2.2/2.1, test plan 6.1/6.2):

  1. Ball-breach is a HARD gate: verify.self_verify's `ok` must go False when a
     joint ball OR a synthesized embedded end-cap sphere breaches a sub-mm
     feature -- and the builders (cco model-anchor, bracing model-rung) must
     stop CREATING such anchors so the gate is satisfiable.
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

from support_placement import verify, cco
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


def test_buried_free_end_stub_is_flagged(cfg):
    """A stub whose FREE end (p1) is buried INSIDE the solid welds into the
    model body -- an illegal print, not a harmless phantom. The full-scale
    Weeping Treant EMITTED this class (the independent census's axis-centerline
    illegal stub members, the 4->15 regression vs v1), disproving the old
    "interior => unroutable => never printed" premise. BOTH implementations
    must now flag it (the cco pre-pass then quarantines it, so
    assert_all_certified is satisfied by exclusion, not by exemption). An
    ordinary surface-hugging stub with p1 in open air is still NOT flagged --
    covered by the sanctioned-embed clean cases in the through-transit tests
    above and by test_perf_a0's negative control."""
    slab = _box((20, 20, 10), center=(0, 0, 0))    # solid z in [-5, 5]
    geom = ExactSurface(slab)
    buried = dict(p0=np.array([0.0, 0.0, 4.7]),    # tip 0.3 inside top face z=5
                  p1=np.array([0.0, 0.0, 2.0]),    # stub_end still deep inside
                  r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                  embed_end="p0")
    assert bool(geom.inside(np.array([buried["p1"]]))[0])   # free end interior
    f = tm.Forest()
    f.add_member(tm.Member(**buried))
    assert [v["member_idx"] for v in f.revalidate(geom, cfg)] == [0]
    assert f.dirty_members() == [0]                # stays uncertified (hard)
    rep = verify.clearance_report(None, [buried], cfg, geom=geom)
    assert [v["member_idx"] for v in rep] == [0]   # independent gate agrees


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
    capsule-offset ray transits the full length (a graze, entered/exited False,
    depth 0.264mm), and segment_clearance is >= 0 (the distance path is blind to
    the graze -- exactly the blind spot 1138 exploited). Records the `rays` mode
    per penetration_intervals_batch call so a test can prove stubs stay
    axis-only while non-stubs go capsule-aware. project() mirrors the scalar
    contract of ExactSurface.project (a 1-D point -> a (3,) outward normal)."""

    def __init__(self):
        self.rays_seen = []

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
            if rays == "all":            # census 1138: an offset ray grazes
                out.append([dict(s_a=0.0, s_b=L, entered=False, exited=False,
                                 max_depth_mm=0.264, ray="off6")])
            else:                        # axis-only: the census-1138 axis is empty
                out.append([])
        return out


def test_r2_predicate_axis_clean_capsule_graze():
    """The exact behaviour change, at the predicate: the axis intervals are
    EMPTY (census-1138 axis) so the old axis-only test PASSES the member; the
    SAME predicate over the capsule-offset interval FAILS it -- WHEN the embed
    inset leaves the offset run exposed. L from the diagnosed site
    (mi=1138, L=1.913); a representative embed inset well inside L."""
    L = 1.913
    assert verify._axis_interval_illegal([], L, 0.0, 0.9) is False
    off = [dict(s_a=0.0, s_b=L, entered=False, exited=False,
                max_depth_mm=0.264, ray="off6")]
    assert verify._axis_interval_illegal(off, L, 0.0, 0.9) is True


def test_r2_capsule_necessary_but_inset_gates_1138_end_to_end():
    """FAITHFUL fixture from the diagnosed site, pinning the R2/R4 SPLIT with the
    REAL kernel intervals + insets recorded on the acceptance run-1 geometry
    (mi=1138: strut, embed=p1, L=1.913, r=0.35).

    R2 supplies the NECESSARY capsule evidence: the axis interval is EMPTY and a
    capsule-offset ray (off6) grazes the full length to 0.264mm -- an axis-only
    predicate is structurally blind to it. But catching it END-TO-END also needs
    the embed inset aligned with the census (R4's job): the engine's ANGLE-AWARE
    inset here is 2.2mm (the 60deg floor: contact_depth + (r+standoff)/0.5),
    which EXCEEDS L=1.913 -> an EMPTY sanctioned window [0, -0.287] that exempts
    the whole member, so even R2's capsule ray cannot flag it. The census's FLAT
    inset (contact_depth + pitch_bound + r + standoff = 1.82mm) leaves a 0.09mm
    sliver exposed and flags it. This test locks in that R2 (capsule sensitivity)
    is necessary but not sufficient alone; R4 (census-aligned embed-inset
    accounting) closes the window."""
    L = 1.913
    off6 = dict(s_a=0.0, s_b=L, entered=False, exited=False,
                max_depth_mm=0.264, ray="off6")
    # (pre-R2) axis-only predicate is blind -- the axis interval is empty.
    assert verify._axis_interval_illegal([], L, 0.0, 1.82) is False
    # (R2, engine's CURRENT inset 2.2 > L) capsule ray present but window empty.
    assert verify._axis_interval_illegal([off6], L, 0.0, 2.2) is False
    # (R2 + R4's census-aligned inset 1.82) the graze is caught.
    assert verify._axis_interval_illegal([off6], L, 0.0, 1.82) is True


def test_r2_clearance_report_flags_capsule_graze_strut(cfg):
    """verify._clearance_report_exact catches the census-1138 class end-to-end:
    a non-stub whose segment_clearance is clear (min_clearance_mm >= 0, NOT a
    distance violation) and whose axis is empty, but a capsule-offset ray grazes
    a thin wall. Pre-R2 (axis-only intervals) delivered ZERO violations; R2
    flags it by consulting rays='all'."""
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
    R4 owns. Guard: with a geom whose offset rays ALL transit but whose axis is
    clean, the STRUT is flagged and the STUB is NOT, and the stub's intervals
    were requested axis-only."""
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
