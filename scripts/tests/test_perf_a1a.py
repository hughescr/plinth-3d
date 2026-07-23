"""SPEC-V5-PERF sec 2.1a / 5.1.4 (A1a): embed-end certificate soundness.

The baseline scalar certificate (cert_valid iff max(r0,r1) <= cert_radius,
cert_radius reset only on POSITION writes) is UNSOUND for embed_end members:
their sanctioned inset is radius-dependent, so a radius DECREASE shrinks the
inset and GROWS the checked window -- geometry that was never checked becomes
load-bearing while the scalar rule keeps the cert valid. A1a voids the cert on
ANY radius write for embed members, and makes embed_end/kind invalidating
writes, while leaving the sound non-embed fast path untouched.
"""
import numpy as np
import trimesh
import pytest

from support_placement import config
from support_placement import tree as tm
from support_placement.geometry_exact import ExactSurface


@pytest.fixture
def cfg():
    return config.SupportConfig()


def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def test_embed_member_radius_decrease_voids_certificate(cfg):
    """An embed_end member's certificate must be voided by a radius DECREASE
    (baseline kept it valid via max(r0,r1) <= cert_radius). A non-embed member
    keeps the sound 'checked fat covers thin' escape hatch."""
    m = tm.Member(p0=np.array([0.0, 0.0, 0.0]), p1=np.array([0.0, 0.0, 5.0]),
                  r0=0.5, r1=1.0, kind="strut", owner=("contact", 0),
                  embed_end="p1")
    # certify it (simulate a clean revalidate)
    m.cert_version = m.geom_version
    m.cert_radius = max(m.r0, m.r1)
    assert m.cert_valid
    m.r1 = 0.6                     # radius DECREASE at the embedded end
    assert not m.cert_valid, "embed member must void cert on radius decrease"

    # non-embed control: same shrink keeps the certificate (sound fast path)
    n = tm.Member(p0=np.array([0.0, 0.0, 0.0]), p1=np.array([0.0, 0.0, 5.0]),
                  r0=0.5, r1=1.0, kind="trunk", owner=("contact", 1),
                  embed_end=None)
    n.cert_version = n.geom_version
    n.cert_radius = max(n.r0, n.r1)
    assert n.cert_valid
    n.r1 = 0.6
    assert n.cert_valid, "non-embed member keeps the sound resize-down cert"


def test_embed_end_and_kind_are_invalidating_writes(cfg):
    """Mutating embed_end or kind WITHOUT a coincident position/radius write
    must void the certificate and bump geom_version (they select the
    validation predicate)."""
    for field, newval in (("embed_end", None), ("kind", "trunk")):
        m = tm.Member(p0=np.array([0.0, 0.0, 0.0]), p1=np.array([0.0, 0.0, 5.0]),
                      r0=0.5, r1=1.0, kind="strut", owner=("contact", 0),
                      embed_end="p1")
        m.cert_version = m.geom_version
        m.cert_radius = max(m.r0, m.r1)
        gv = m.geom_version
        assert m.cert_valid
        setattr(m, field, newval)
        assert m.geom_version > gv, f"{field} write must bump geom_version"
        assert not m.cert_valid, f"{field} write must void the certificate"


def test_embed_shrink_reveals_planted_illegal_transit(cfg):
    """The soundness payoff: an embed-end member whose radius shrinks so its
    grown checked window now contains an illegal transit is DETECTED on
    re-check (the baseline's unsound cert would have skipped revalidation
    entirely and silently emitted it).

    Geometry: an overhang plate at z in [-0.25,0.25] (embed surface) with a
    thin fin floating below at z in [-2.0,-1.5]. A strut embedded at the plate
    underside (embed_end p1 near z=-0.25) running down to z=-3.0. With a LARGE
    embed radius the sanctioned inset swallows the fin crossing; shrink the
    radius and the inset recedes, exposing the through-fin transit."""
    plate = _box((40, 40, 0.5), center=(0, 0, 0))
    fin = _box((6, 6, 0.5), center=(0, 0, -1.75))
    merged = trimesh.util.concatenate([plate, fin])
    mesh = trimesh.Trimesh(vertices=merged.vertices.copy(),
                           faces=merged.faces.copy(), process=False)
    geom = ExactSurface(mesh)

    # A strut whose embedded (p1) end sits at the plate underside and whose
    # shaft dives straight down through the fin to open air at z=-3.0. Model as
    # embed_end="p0" at the plate so p0 is the sanctioned end; the shaft below
    # transits the fin.
    strut = tm.Member(p0=np.array([0.0, 0.0, -0.2]),   # embedded in plate
                      p1=np.array([0.0, 0.0, -3.0]),   # open air below the fin
                      r0=3.0, r1=0.35, kind="strut", owner=("contact", 0),
                      embed_end="p0")
    f = tm.Forest()
    f.add_member(strut)
    # With the huge r0=3.0 the inset (contact_depth + (r0+standoff)/cosang) is
    # large enough to exempt the fin crossing -> certifies clean.
    big = f.revalidate(geom, cfg)
    assert big == [], "large embed radius should exempt the crossing"
    assert f.dirty_members() == []

    # Now shrink r0. A1a voids the cert (embed member, radius write) so the
    # member re-enters revalidation; the grown window now contains the fin
    # transit -> flagged. (Baseline: max(r0,r1) <= cert_radius kept it certified
    # and it would never be re-checked.)
    strut.r0 = 0.35
    assert not strut.cert_valid
    small = f.revalidate(geom, cfg)
    assert [v["member_idx"] for v in small] == [0], (
        "shrunk embed member's exposed transit must be detected")
