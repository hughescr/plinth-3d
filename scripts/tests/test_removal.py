"""Removal-leverage gate tests (design-v4.md #10, Phase 5).

TDD discipline (design-v4.md #13): every fixture below is written against
`removal.py` BEFORE the real implementation exists (see the module's
initial NotImplementedError stub) and drives it to green.

Hand-built forests, matching test_transient.py's convention: Member.p1 is
always the root-ward end, Member.p0 the terminal-ward end. `_far_mesh()`
stands in for "no model obstruction anywhere reachable" (a box translated
100,000mm away); `_slab_wall` builds a single thin obstructing wall for
tests that need one specific, controlled obstruction.
"""
import numpy as np
import pytest
import trimesh

from support_placement import tree as tm, mesh_io, removal


def _member(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("m", 0), embed_end=None):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner, embed_end=embed_end)


def _far_mesh():
    return mesh_io.make_box(size=(10, 10, 10), translate=(100000.0, 0.0, 0.0))


def _to_mesh(parts):
    merged = trimesh.util.concatenate(parts) if len(parts) > 1 else parts[0]
    return trimesh.Trimesh(vertices=merged.vertices.copy(), faces=merged.faces.copy(),
                            process=False)


def _slab_wall(axis, near_face, thickness=1.0, extent=400.0):
    """A thin rectangular slab whose near face sits at `near_face` along
    `axis` (0=x, 1=y, 2=z), spanning `extent` in the other two axes -- a
    single controlled obstruction."""
    extents = [extent, extent, extent]
    extents[axis] = thickness
    b = trimesh.creation.box(extents=extents)
    center = [0.0, 0.0, 0.0]
    center[axis] = near_face + thickness / 2.0
    b.apply_translation(center)
    return b


# ============================================================ config defaults
def test_config_removal_knobs_match_charter_defaults(cfg):
    assert cfg.grip_handle_r_mm == pytest.approx(1.0)
    assert cfg.grip_handle_len_mm == pytest.approx(10.0)
    assert cfg.grip_clear_mm == pytest.approx(5.0)
    assert cfg.pull_dist_mm == pytest.approx(15.0)


# ============================================================ forest_trees
def test_forest_trees_merges_across_a_connecting_brace(cfg):
    f = tm.Forest()
    a = _member([0, 0, 10], [0, 0, -5], r0=1.2, r1=1.2)
    a_idx = f.add_member(a)
    b = _member([10, 0, 10], [10, 0, -5], r0=1.2, r1=1.2)
    b_idx = f.add_member(b)
    brace = _member([0, 0, 4], [10, 0, 4], r0=0.5, r1=0.5, kind="brace")
    brace_idx = f.add_brace(brace)
    trees = removal.forest_trees(f)
    assert len(trees) == 1
    assert set(trees[0]) == {a_idx, b_idx, brace_idx}


def test_forest_trees_excludes_unrouted_stub_and_dead_members(cfg):
    f = tm.Forest()
    routed = _member([0, 0, 10], [0, 0, -5], r0=1.2, r1=1.2)
    routed_idx = f.add_member(routed)
    orphan_stub = _member([50, 0, 1], [50, 0, 3], r0=0.3, r1=0.3, kind="stub", embed_end="p0")
    orphan_idx = f.add_member(orphan_stub)
    dead_member = _member([80, 0, 0], [80, 0, 5], r0=0.5, r1=0.5)
    dead_idx = f.add_member(dead_member)
    f.mark_dead(dead_idx)
    trees = removal.forest_trees(f, unrouted_stub_idx={orphan_idx})
    assert trees == [[routed_idx]]


def test_forest_trees_empty_forest_returns_no_trees(cfg):
    f = tm.Forest()
    assert removal.forest_trees(f) == []


# ============================================================ removal_report: single-tree HANDLE cases
def test_compliant_handle_and_open_peel_pass(cfg):
    mesh = _far_mesh()
    f = tm.Forest()
    f.add_member(_member([0, 0, 15], [0, 0, -5], r0=1.2, r1=1.2))
    report = removal.removal_report(f, mesh, None, cfg)
    assert report["ok"] is True
    assert report["blocked_trees"] == []
    assert report["n_trees"] == 1
    t = report["trees"][0]
    assert t["peel_ok"] is True
    assert t["handle_ok_isolated"] is True
    assert any(r["passes_length"] and r["passes_model_clear"] for r in t["handle_runs"])
    assert report["removal_order"] == [t["tree_id"]]
    assert report["removal_effort"][t["tree_id"]] == 0


def test_handle_fails_too_short(cfg):
    """Thick enough (r=1.2 >= grip_handle_r_mm), clear of the (far-away)
    model, but only 8mm of contiguous run -- below grip_handle_len_mm."""
    mesh = _far_mesh()
    f = tm.Forest()
    f.add_member(_member([0, 0, 3], [0, 0, -5], r0=1.2, r1=1.2))  # length 8mm
    report = removal.removal_report(f, mesh, None, cfg)
    t = report["trees"][0]
    assert t["peel_ok"] is True
    assert any(not r["passes_length"] for r in t["handle_runs"])
    assert not any(r["passes_length"] and r["passes_model_clear"] for r in t["handle_runs"])
    assert report["ok"] is False
    assert [b["reason"] for b in report["blocked_trees"]] == ["handle_too_short"]


def test_handle_fails_too_thin(cfg):
    """Long enough (20mm >= grip_handle_len_mm) but r=0.6 < grip_handle_r_mm
    -- never qualifies as a handle candidate at all."""
    mesh = _far_mesh()
    f = tm.Forest()
    f.add_member(_member([0, 0, 15], [0, 0, -5], r0=0.6, r1=0.6))
    report = removal.removal_report(f, mesh, None, cfg)
    t = report["trees"][0]
    assert t["handle_runs"] == []
    assert report["ok"] is False
    assert [b["reason"] for b in report["blocked_trees"]] == ["no_handle_member"]


def test_handle_fails_insufficient_free_space_vs_model(cfg):
    """Thick (r=1.2) and long (20mm) enough, but a wall sits 4.0mm off the
    member's axis: gap = 4.0 - 1.2 = 2.8mm, above cfg.standoff_mm (so the
    member itself is legitimately constructible) but below
    grip_clear_mm=5.0 (so it fails the HANDLE-specific free-space check).
    PEEL must still pass -- the outward direction set pulls AWAY from the
    wall (see _outward_xy_bearing: outward = tree_xy - mesh_centroid_xy),
    isolating this as a pure HANDLE failure, not a PEEL failure."""
    wall = _slab_wall(axis=0, near_face=4.0)
    mesh = _to_mesh([wall])
    f = tm.Forest()
    f.add_member(_member([0, 0, 15], [0, 0, -5], r0=1.2, r1=1.2))
    report = removal.removal_report(f, mesh, None, cfg)
    t = report["trees"][0]
    assert t["peel_ok"] is True
    assert any(r["passes_length"] and not r["passes_model_clear"] for r in t["handle_runs"])
    for r in t["handle_runs"]:
        if r["passes_length"]:
            assert r["worst_model_gap_mm"] == pytest.approx(2.8, abs=0.05)
    assert report["ok"] is False
    assert [b["reason"] for b in report["blocked_trees"]] == ["handle_insufficient_model_clearance"]


def test_handle_excludes_stub_kind_even_if_nominally_thick(cfg):
    """Edge case (design-v4.md #10 implies it): a tree made entirely of
    members that don't count as grippable -- one genuinely thin shaft,
    plus a 'stub' member whose radius alone would clear grip_handle_r_mm
    but whose kind excludes it (a stub's tip end is the tiny embedded
    contact point, not something a human grips)."""
    mesh = _far_mesh()
    f = tm.Forest()
    shaft = _member([0, 0, 10], [0, 0, -5], r0=0.4, r1=0.4)
    shaft_idx = f.add_member(shaft)
    stub = _member([0, 0, 12], [0, 0, 10], r0=1.2, r1=1.2, kind="stub", embed_end="p0")
    stub_idx = f.add_member(stub)
    f.set_parent(stub_idx, shaft_idx)
    report = removal.removal_report(f, mesh, None, cfg)
    t = report["trees"][0]
    assert t["handle_runs"] == []
    assert report["ok"] is False
    assert [b["reason"] for b in report["blocked_trees"]] == ["no_handle_member"]


# ============================================================ removal_report: PEEL
def test_peel_fails_when_every_direction_is_caged_by_the_model(cfg):
    """A compliant HANDLE (clears grip_clear_mm at rest) sealed inside a
    solid box whose nearby faces sit only 6.0mm off every point of the
    member (x/y walls at +-7.2 vs the member's x=y=0 axis; z walls at
    +-12.2 vs its z=+-5 endpoints, same 7.2mm margin) -- enough to
    satisfy grip_clear_mm=5.0 at rest, but every one of the sweep
    directions collides with a wall long before pull_dist_mm=15mm."""
    box = trimesh.creation.box(extents=(2 * 7.2, 2 * 7.2, 2 * 12.2))
    mesh = _to_mesh([box])
    f = tm.Forest()
    f.add_member(_member([0, 0, 5], [0, 0, -5], r0=1.2, r1=1.2))  # length 10mm, endpoints at z=+-5
    report = removal.removal_report(f, mesh, None, cfg)
    t = report["trees"][0]
    assert t["handle_ok_isolated"] is True, t["handle_runs"]
    assert t["peel_ok"] is False
    assert report["ok"] is False
    assert [b["reason"] for b in report["blocked_trees"]] == ["peel_blocked"]


# ============================================================ ordered removal: effort + hard failure
def test_neighbor_dependent_tree_removes_only_after_its_blocker_clears(cfg):
    """Tree A has a single candidate handle region close (2.4mm axis gap)
    to part of tree Q; Q also has an independent SECOND candidate region
    far from everything (joined to the near part only by a thin,
    non-qualifying bridge member, so the two never merge into one run).
    Q therefore passes HANDLE immediately via its far region (round 1)
    despite also having material close to A; once Q is gone, A's only
    blocker is gone and A removes in round 2 -- design-v4.md #10's
    'trees that only become removable after neighbors go', tracked in
    removal_effort."""
    mesh = _far_mesh()
    f = tm.Forest()
    a_idx = f.add_member(_member([0, 0, 15], [0, 0, -5], r0=1.2, r1=1.2, owner=("m", 0)))

    q_root = _member([2.4, 0, 10], [2.4, 0, -5], r0=1.2, r1=1.2, owner=("m", 1))
    q_root_idx = f.add_member(q_root)
    q_bridge = _member([30, 0, 10], [2.4, 0, 10], r0=0.5, r1=0.5, owner=("m", 2))
    q_bridge_idx = f.add_member(q_bridge)
    f.set_parent(q_bridge_idx, q_root_idx)
    q_far = _member([30, 0, 25], [30, 0, 10], r0=1.2, r1=1.2, owner=("m", 3))
    q_far_idx = f.add_member(q_far)
    f.set_parent(q_far_idx, q_bridge_idx)

    report = removal.removal_report(f, mesh, None, cfg)
    assert report["ok"] is True, report["blocked_trees"]
    assert report["n_trees"] == 2

    by_tid = {t["tree_id"]: t for t in report["trees"]}
    a_tree = by_tid[a_idx]
    q_tree = by_tid[q_root_idx]

    assert report["removal_effort"][q_root_idx] == 0
    assert report["removal_effort"][a_idx] == 1
    order = report["removal_order"]
    assert order.index(q_root_idx) < order.index(a_idx)


def test_mutual_caging_is_a_hard_failure(cfg):
    """Two otherwise-compliant trees positioned so each is the other's
    ONLY blocker (axis gap 2.4mm, well under grip_clear_mm=5.0, neither
    has an escape-hatch second region): no removal order can ever clear
    either -- design-v4.md #10's explicit BLOCKER case."""
    mesh = _far_mesh()
    f = tm.Forest()
    a_idx = f.add_member(_member([0, 0, 15], [0, 0, -5], r0=1.2, r1=1.2))
    b_idx = f.add_member(_member([2.4, 0, 15], [2.4, 0, -5], r0=1.2, r1=1.2))
    report = removal.removal_report(f, mesh, None, cfg)
    assert report["ok"] is False
    blocked_ids = {b["tree_id"] for b in report["blocked_trees"]}
    assert blocked_ids == {a_idx, b_idx}
    reasons = {b["tree_id"]: b["reason"] for b in report["blocked_trees"]}
    assert reasons[a_idx] == "neighbor_caged"
    assert reasons[b_idx] == "neighbor_caged"
    assert report["removal_order"] == []


# ============================================================ malformed input / determinism / empty
def test_removal_report_crashes_on_invalid_embed_end(cfg):
    """Crash-over-silent-corruption: an embed_end outside {p0,p1,None}
    would silently skip BOTH insets in the sampling helper (neither
    branch fires), quietly mis-sampling the member -- must raise instead
    of producing a wrong-but-plausible-looking report."""
    mesh = _far_mesh()
    f = tm.Forest()
    f.add_member(_member([0, 0, 15], [0, 0, -5], r0=1.2, r1=1.2, embed_end="bogus"))
    with pytest.raises(AssertionError):
        removal.removal_report(f, mesh, None, cfg)


def test_removal_report_deterministic_across_repeated_calls(cfg):
    mesh = _far_mesh()
    f = tm.Forest()
    f.add_member(_member([0, 0, 15], [0, 0, -5], r0=1.2, r1=1.2))
    f.add_member(_member([2.4, 0, 15], [2.4, 0, -5], r0=1.2, r1=1.2))
    r1 = removal.removal_report(f, mesh, None, cfg)
    r2 = removal.removal_report(f, mesh, None, cfg)
    assert r1 == r2


def test_removal_report_empty_forest_is_ok(cfg):
    mesh = _far_mesh()
    f = tm.Forest()
    report = removal.removal_report(f, mesh, None, cfg)
    assert report == dict(ok=True, n_trees=0, trees=[], removal_order=[],
                           removal_effort={}, blocked_trees=[])
