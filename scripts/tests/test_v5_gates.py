"""Consumer-level negative controls for the two v5 hard gates that a prior
pass DETECTED but never WIRED (see SPEC-V5-GEOM sec 2.2/2.1, test plan 6.1/6.2):

  1. Ball-breach is a HARD gate: verify.self_verify's `ok` must go False when a
     joint ball OR a synthesized embedded end-cap sphere breaches a sub-mm
     feature -- and the builders (cco model-anchor, bracing model-rung) must
     stop CREATING such anchors so the gate is satisfiable.
  2. Stub through-transit: BOTH independent clearance implementations
     (tree.Forest.revalidate AND verify.clearance_report) must flag a stub whose
     shaft welds through a thin fin, while still certifying a sanctioned embed
     and NOT flagging an interior/unroutable buried stub.

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


def test_buried_unroutable_stub_not_flagged(cfg):
    """Discriminator guard: a stub whose stub_end (p1) is INSIDE the solid is
    the interior/unroutable class (outward normal into the model body) --
    discarded via the unrouted path, never printed -- and MUST NOT be flagged
    by either implementation (that would crash assert_all_certified on
    geometry that never prints). Locks in the p1-exterior gate."""
    slab = _box((20, 20, 10), center=(0, 0, 0))    # solid z in [-5, 5]
    geom = ExactSurface(slab)
    buried = dict(p0=np.array([0.0, 0.0, 4.7]),    # tip 0.3 inside top face z=5
                  p1=np.array([0.0, 0.0, 2.0]),    # stub_end still deep inside
                  r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                  embed_end="p0")
    f = tm.Forest()
    f.add_member(tm.Member(**buried))
    assert f.revalidate(geom, cfg) == []
    assert f.dirty_members() == []
    assert verify.clearance_report(None, [buried], cfg, geom=geom) == []
