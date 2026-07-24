"""SPEC-V5-PERF sec 5.1.4 (A0) fuzz/negative-control suite: the poisoning fix.

A0a -- pre-existing-violation snapshot in cco._insert_terminal keyed
(member_idx, geom_version): a persistent violator no longer false-rejects
unrelated candidates, and a violator MUTATED by a trial (version bumped) is NOT
waived.
A0b -- quarantine of condemned stubs in cco.build: a contact whose own stub
welds through a thin feature is honestly unrouted, its stub member quarantined
(excluded from dirty_members / assert_all_certified / live lists), while clean
contacts in the same build route normally.

Fixtures deterministic (triangle soup + hand-built forests), no unseeded
sampling. All checks are against the production exact oracle (KernelSurface).
"""
import numpy as np
import trimesh
import pytest

from support_placement import cco, config, engine, joints
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


def _soup(*meshes):
    merged = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=merged.vertices.copy(),
                           faces=merged.faces.copy(), process=False)


def _overhang_plate():
    """A thin downward-facing overhang plate at z in [-0.25, 0.25]: contacts
    sit on its UNDERSIDE (normal -z) so their stubs and plate-descent columns
    run DOWNWARD in open air to a plate far below -- a routable geometry."""
    return _box((40, 40, 0.5), center=(0, 0, 0))


def _through_fin_mesh():
    """Overhang plate + a floating 0.5mm fin at z in [-1.5,-1.0] under the
    origin. A downward stub tip-embedded in the plate and ending at z=-2.75
    welds through the fin (census member-357 class); a contact 15mm away has
    clear air below it."""
    fin = _box((6, 6, 0.5), center=(0, 0, -1.25))
    return _soup(_overhang_plate(), fin)


def _buried_end_mesh():
    """Overhang plate + a SOLID block at z in [-5,-2] under the origin. A
    downward stub tip-embedded in the plate whose FREE end (p1) lands at
    z=-3.5 buries that end deep INSIDE the block -- an illegal weld into the
    model body whose stub end is INTERIOR, not the through-and-through class.
    A contact 15mm away has clear air below it (no block)."""
    block = _box((6, 6, 3), center=(0, 0, -3.5))
    return _soup(_overhang_plate(), block)


# ==========================================================================
# A0a -- snapshot semantics on hand-built forests
# ==========================================================================
def test_a0a_persistent_violator_does_not_false_reject_clean_candidate(cfg):
    """A member left persistently violating (a condemned stub) must NOT cause a
    clean, unrelated candidate to be rejected: with the snapshot, only NEW
    violations reject. Mirrors the sec-0 poisoning at unit scale."""
    geom = ExactSurface(_through_fin_mesh())
    forest = tm.Forest()
    # member 0: the condemned through-fin stub (persistent violator).
    bad = tm.Member(p0=np.array([0.0, 0.0, 0.05]), p1=np.array([0.0, 0.0, -2.75]),
                    r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                    embed_end="p0")
    forest.add_member(bad)
    viol = forest.revalidate(geom, cfg)
    assert [v["member_idx"] for v in viol] == [0]   # stays dirty forever

    # Emulate _insert_terminal's snapshot + trial for a CLEAN candidate far
    # from the fin (a bare vertical strut in open air at x=15).
    pre_violators = {(idx, forest.members[idx].geom_version)
                     for idx in forest.dirty_members()}
    assert pre_violators == {(0, forest.members[0].geom_version)}

    cp = forest.checkpoint()
    clean = tm.Member(p0=np.array([15.0, 0.0, -2.75]), p1=np.array([15.0, 0.0, -8.0]),
                      r0=0.8, r1=0.8, kind="trunk", owner=("contact", 1),
                      embed_end=None)
    forest.add_member(clean)
    violations = forest.revalidate(geom, cfg)
    new_viol = [v for v in violations
                if (v["member_idx"], forest.members[v["member_idx"]].geom_version)
                not in pre_violators]
    # the clean candidate introduces no new violation; the old stub's
    # violation is waived by the snapshot.
    assert new_viol == [], new_viol
    forest.commit(cp)


def test_a0a_version_key_does_not_waive_a_mutated_still_violating_member(cfg):
    """The key is (idx, geom_version), not bare idx: if a trial MUTATES a
    pre-existing violator and it STILL violates, that is a NEW violation and
    must NOT be waived (bare-idx filtering would conceal it)."""
    geom = ExactSurface(_through_fin_mesh())
    forest = tm.Forest()
    bad = tm.Member(p0=np.array([0.0, 0.0, 0.05]), p1=np.array([0.0, 0.0, -2.75]),
                    r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                    embed_end="p0")
    forest.add_member(bad)
    forest.revalidate(geom, cfg)
    pre_violators = {(idx, forest.members[idx].geom_version)
                     for idx in forest.dirty_members()}
    gv_before = forest.members[0].geom_version

    cp = forest.checkpoint()
    # a trial mutates member 0 (bumps geom_version) but it still welds through.
    forest.members[0].p1 = np.array([0.0, 0.0, -2.85])
    assert forest.members[0].geom_version > gv_before
    violations = forest.revalidate(geom, cfg)
    new_viol = [v for v in violations
                if (v["member_idx"], forest.members[v["member_idx"]].geom_version)
                not in pre_violators]
    assert [v["member_idx"] for v in new_viol] == [0]   # NOT waived
    forest.rollback(cp)


# ==========================================================================
# A0b -- quarantine end-to-end through cco.build
# ==========================================================================
def _two_contact_build(cfg, geom):
    """A build with two contacts on the slab top: A (origin) welds through the
    fin; B (x=15, clean) routes to the plate by straight descent. Model-anchor
    builders disabled (sample_points=None) so B routes purely via plate
    descent -- keeps the fixture deterministic and independent of anchor
    pools."""
    contacts = [dict(position=np.array([0.0, 0.0, -0.25]), normal=np.array([0.0, 0.0, -1.0])),
                dict(position=np.array([15.0, 0.0, -0.25]), normal=np.array([0.0, 0.0, -1.0]))]
    tip_r = np.array([0.2, 0.2])
    carried = np.array([1.0, 1.0])
    minima_flags = [False, False]
    plate_z = -10.0
    return cco.build(_through_fin_mesh(), contacts, tip_r, carried, minima_flags,
                     plate_z, geom, cfg, sample_points=None, sample_delicacy=None)


def test_a0b_condemned_stub_is_quarantined_and_contact_unrouted(cfg):
    geom = ExactSurface(_through_fin_mesh())
    forest, unrouted = _two_contact_build(cfg, geom)

    # contact 0 (through-fin) is quarantined + unrouted; its stub member is in
    # forest.quarantined.
    stub0 = forest.terminal_of[0]
    assert stub0 in forest.quarantined
    assert 0 in unrouted

    # contact 1 (clean) routed: NOT unrouted, stub not quarantined.
    assert 1 not in unrouted
    assert forest.terminal_of[1] not in forest.quarantined

    # assert_all_certified PASSES despite the still-dirty condemned stub
    # (quarantine excludes it from dirty_members).
    assert stub0 not in forest.dirty_members()
    tm.assert_all_certified(forest)

    # live lists exclude the quarantined contact's stub (via the unrouted path).
    members, _balls = engine.live_member_lists(forest, unrouted, -10.0, cfg)
    owners = {tuple(m["owner"]) for m in members if m["kind"] == "stub"}
    assert ("contact", 0) not in owners
    assert ("contact", 1) in owners


def test_a0b_bit_identical_when_no_persistent_violator(cfg):
    """No condemned stub -> quarantined empty, no contact skipped: the
    quarantine pre-pass is a no-op (fixture-neutral). Same two clean contacts
    on a plain slab (no fin) both route."""
    plate = _overhang_plate()
    geom = ExactSurface(plate)
    contacts = [dict(position=np.array([0.0, 0.0, -0.25]), normal=np.array([0.0, 0.0, -1.0])),
                dict(position=np.array([15.0, 0.0, -0.25]), normal=np.array([0.0, 0.0, -1.0]))]
    tip_r = np.array([0.2, 0.2])
    carried = np.array([1.0, 1.0])
    forest, unrouted = cco.build(plate, contacts, tip_r, carried, [False, False],
                                 -10.0, geom, cfg, sample_points=None,
                                 sample_delicacy=None)
    assert forest.quarantined == set()
    assert unrouted == []
    tm.assert_all_certified(forest)


# ==========================================================================
# A0b regression -- buried-free-end stubs (full-scale treant, 2026-07-23)
#
# The stub through-transit rule formerly ran ONLY on stubs whose free end (p1)
# parity-voted EXTERIOR, exempting p1-interior stubs on the assumption they are
# unroutable-by-construction. The full-scale Weeping Treant disproved that:
# contacts whose stub buries p1 in adjacent crown foliage were routed and
# EMITTED, surfacing as the independent census's axis-centerline illegal stub
# members (the 4->15 illegal-member regression vs v1). Every stub now gets the
# axis interval test; a buried-end stub is condemned like a through-transit one.
# ==========================================================================
def test_a0b_buried_free_end_stub_is_flagged(cfg):
    """A stub tip-embedded in the plate (sanctioned p0) whose FREE end p1 is
    buried INSIDE the solid block fails certification -- the p1-interior
    exemption is gone. Its axis has an inside-run (through the block) well
    outside the sanctioned p0 embed zone."""
    mesh = _buried_end_mesh()
    geom = ExactSurface(mesh)

    # Precondition: p1 really is interior (the case the old code exempted).
    p1 = np.array([0.0, 0.0, -3.5])
    assert bool(geom.inside(p1[None])[0])

    forest = tm.Forest()
    buried = tm.Member(p0=np.array([0.0, 0.0, 0.05]), p1=p1,
                       r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                       embed_end="p0")
    forest.add_member(buried)
    viol = forest.revalidate(geom, cfg)
    assert [v["member_idx"] for v in viol] == [0]   # flagged, stays dirty


def test_a0b_ordinary_stub_with_air_below_is_not_flagged(cfg):
    """Negative control on the SAME mesh: a stub 15mm away tip-embedded in the
    plate with clear AIR below (no block) keeps its whole inside-run within the
    p0 embed zone and is NOT flagged -- the interval test does not false-
    positive on an ordinary surface-hugging stub now that it runs on all stubs."""
    mesh = _buried_end_mesh()
    geom = ExactSurface(mesh)

    p1 = np.array([15.0, 0.0, -2.75])
    assert not bool(geom.inside(p1[None])[0])   # free end in open air

    forest = tm.Forest()
    ok_stub = tm.Member(p0=np.array([15.0, 0.0, 0.05]), p1=p1,
                        r0=0.2, r1=0.35, kind="stub", owner=("contact", 0),
                        embed_end="p0")
    forest.add_member(ok_stub)
    assert forest.revalidate(geom, cfg) == []
    tm.assert_all_certified(forest)


def test_a0b_buried_free_end_stub_is_quarantined_end_to_end(cfg):
    """End-to-end through cco.build: the buried-end contact is quarantined and
    unrouted (never emitted), while a clean contact 15mm away routes. Mirrors
    test_a0b_condemned_stub_is_quarantined_and_contact_unrouted for the
    p1-interior class the census flagged at full scale."""
    mesh = _buried_end_mesh()
    geom = ExactSurface(mesh)
    contacts = [dict(position=np.array([0.0, 0.0, -0.25]), normal=np.array([0.0, 0.0, -1.0])),
                dict(position=np.array([15.0, 0.0, -0.25]), normal=np.array([0.0, 0.0, -1.0]))]
    tip_r = np.array([0.2, 0.2])
    carried = np.array([1.0, 1.0])
    forest, unrouted = cco.build(mesh, contacts, tip_r, carried, [False, False],
                                 -10.0, geom, cfg, sample_points=None,
                                 sample_delicacy=None)
    stub0 = forest.terminal_of[0]
    assert stub0 in forest.quarantined
    assert 0 in unrouted
    assert 1 not in unrouted
    assert forest.terminal_of[1] not in forest.quarantined
    tm.assert_all_certified(forest)

    members, _balls = engine.live_member_lists(forest, unrouted, -10.0, cfg)
    owners = {tuple(m["owner"]) for m in members if m["kind"] == "stub"}
    assert ("contact", 0) not in owners   # buried-end stub never emitted
    assert ("contact", 1) in owners
