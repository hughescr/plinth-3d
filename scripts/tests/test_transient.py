"""Phase 4a tests (design-v4.md #8): build-time honesty on the v4 forest.

Ported v3 oracle pins (scripts/tests/test_trunks.py's
test_junction_restraint_does_not_exist_until_shoot_completes, reproduced
verbatim against the new module) come first, then the v4-specific
per-path restraint derivation (derive_brace_attachments, transient_report)
and the global resize pass (resize_forest). Every hand-built forest below
sets Member.p1 as the root-ward end and Member.p0 as the terminal-ward
end (tree.py's documented convention: a child's p1 coincides with its
parent's p0).
"""
import numpy as np
import pytest

from support_placement import cco, placement, tree as tm, transient as tr, verify


def _member(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("m", 0), embed_end=None,
            carried_mm2=0.0):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner,
                      embed_end=embed_end, carried_mm2=carried_mm2)


# ============================================================ 1. v3 oracle pins
def test_v3_oracle_lagged_junction_equals_unbraced():
    """A lagged junction restrains NOTHING until its shoot completes --
    identical to no restraint at all."""
    lagged = tr.transient_worst_span([(30.0, 60.0)], -5.0, 60.0)
    unbraced = tr.transient_worst_span([], -5.0, 60.0)
    assert lagged == pytest.approx(unbraced) == pytest.approx(65.0)


def test_v3_oracle_brace_restrains_immediately():
    braced = tr.transient_worst_span([(30.0, 30.0)], -5.0, 60.0)
    assert braced == pytest.approx(35.0)


# ============================================================ 2. zero / clamped
def test_zero_restraints_gives_full_height():
    assert tr.transient_worst_span([], 0.0, 50.0) == pytest.approx(50.0)


def test_out_of_range_restraint_is_clamped_not_dropped():
    """A restraint whose point_z lies ABOVE z_top must still be honored
    (clamped to z_top), not silently filtered out -- clamping it in
    changes the answer from the "drop it" answer (50.0, i.e. full
    height) to 10.0: once clamped, the grip point sits at the very top,
    so nothing above height 10 can ever run longer than 10mm free."""
    result = tr.transient_worst_span([(70.0, 10.0)], 0.0, 50.0)
    assert result == pytest.approx(10.0)
    assert result != pytest.approx(50.0)  # the "dropped" (wrong) answer


def test_restraint_entirely_below_base_has_no_effect():
    result = tr.transient_worst_span([(-100.0, -50.0)], 0.0, 50.0)
    assert result == pytest.approx(50.0)


# ============================================================ 3. sort discrimination
def test_active_z_sort_not_point_z_sort():
    """Two restraints whose point_z order is the REVERSE of their
    active_z order. Sorting by point_z (wrong) gives 40.0; sorting by
    active_z (correct, per the ported v3 rule) gives 10.0."""
    restraints = [(40.0, 10.0), (10.0, 40.0)]
    correct = tr.transient_worst_span(restraints, 0.0, 50.0)
    assert correct == pytest.approx(10.0)
    assert correct != pytest.approx(40.0)  # the point_z-sorted (wrong) answer


# ============================================================ 4. hand-built: unbraced column
def _plate_column_forest(top_z=50.0, tip_z=52.0):
    """plate(0,0,0) -> trunk -> stub(tip at tip_z). No branching, no
    braces: the whole thing is one uninterrupted free cantilever until
    the very last layer (the stub tip) prints."""
    f = tm.Forest()
    trunk = _member([0.0, 0.0, top_z], [0.0, 0.0, 0.0], r0=0.8, r1=0.8, kind="trunk")
    trunk_idx = f.add_member(trunk)
    stub = _member([0.0, 0.0, tip_z], [0.0, 0.0, top_z], r0=0.3, r1=0.35,
                    kind="stub", embed_end="p0")
    stub_idx = f.add_member(stub)
    f.set_parent(stub_idx, trunk_idx)
    return f, trunk_idx, stub_idx


def test_unbraced_column_worst_span_is_full_height_plus_stub_top(cfg):
    f, trunk_idx, stub_idx = _plate_column_forest(top_z=50.0, tip_z=52.0)
    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert stub_idx not in report  # stubs are never reported
    assert report[trunk_idx]["worst_span_mm"] == pytest.approx(52.0)


# ============================================================ 5. + one brace
def test_braced_column_activates_at_upper_rod_endpoint(cfg):
    """One brace attaches to the trunk at z=15 (its LOWER end, geometrically
    matched by derive_brace_attachments); its OTHER (floating, unattached)
    end sits at z=32. The restraint must activate at z=32 (the rod's
    upper/top end), not at the z=15 attach point -- the exact v3 lagged-
    activation rule, ported. This also demonstrably CUTS the span versus
    the fully unbraced 40mm column (test 4's pattern, here z_top=40)."""
    f, trunk_idx, stub_idx = _plate_column_forest(top_z=38.0, tip_z=40.0)
    # p0 (z=15) is the on-axis attach point; p1's z (32) is the "upper
    # end" that must gate activation, not p0's own z.
    brace = _member([0.0, 0.0, 15.0], [20.0, 0.0, 32.0], r0=0.3, r1=0.3, kind="brace")
    b_idx = f.add_member(brace)
    f.roots.remove(b_idx)
    f.braces = [b_idx]

    attach = tr.derive_brace_attachments(f)
    assert attach[trunk_idx] == [(15.0, b_idx)]  # only the on-axis end attaches

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    # correct (upper-endpoint) activation: worst == 32.0 (see module design
    # notes / SPEC derivation: first candidate span 0->32 dominates the
    # 40-15=25 trailing span). A buggy point_z-as-active_z implementation
    # would instead give max(15, 40-15) == 25.0.
    assert report[trunk_idx]["worst_span_mm"] == pytest.approx(32.0)
    assert tr.transient_worst_span([(15.0, 32.0)], 0.0, 40.0) == pytest.approx(32.0)
    assert tr.transient_worst_span([(15.0, 15.0)], 0.0, 40.0) == pytest.approx(25.0)
    assert report[trunk_idx]["worst_span_mm"] < 40.0  # genuinely cut


# ============================================================ 6. Y-junction sibling timing
def _y_junction_forest():
    """trunk(0..20) forks at z=20 into child_A (taller, top=58 via its
    stub) and child_B (shorter sibling, top=45 via its stub)."""
    f = tm.Forest()
    trunk = _member([0.0, 0.0, 20.0], [0.0, 0.0, 0.0], kind="trunk")
    trunk_idx = f.add_member(trunk)
    child_a = _member([0.0, 0.0, 55.0], [0.0, 0.0, 20.0], kind="shoot")
    a_idx = f.add_member(child_a)
    f.set_parent(a_idx, trunk_idx)
    child_b = _member([0.0, 0.0, 42.0], [0.0, 0.0, 20.0], kind="shoot")
    b_idx = f.add_member(child_b)
    f.set_parent(b_idx, trunk_idx)
    stub_a = _member([0.0, 0.0, 58.0], [0.0, 0.0, 55.0], kind="stub", embed_end="p0")
    sa_idx = f.add_member(stub_a)
    f.set_parent(sa_idx, a_idx)
    stub_b = _member([0.0, 0.0, 45.0], [0.0, 0.0, 42.0], kind="stub", embed_end="p0")
    sb_idx = f.add_member(stub_b)
    f.set_parent(sb_idx, b_idx)
    return f, trunk_idx, a_idx, b_idx


def test_junction_restrains_taller_path_only_once_sibling_subtree_completes(cfg):
    f, trunk_idx, a_idx, b_idx = _y_junction_forest()
    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)

    # sibling completion timing: point_z is the junction height (20), but
    # active_z is B's OWN subtree top (45), not the junction height itself.
    assert tr.transient_worst_span([(20.0, 45.0)], 0.0, 58.0) == pytest.approx(45.0)
    # a buggy point-as-active implementation would give a different number
    assert tr.transient_worst_span([(20.0, 20.0)], 0.0, 58.0) == pytest.approx(38.0)

    assert report[a_idx]["worst_span_mm"] == pytest.approx(45.0)
    # the trunk (entirely below the junction) inherits the same worst
    # span: it is the physical restraint carrying that free cantilever's
    # reaction force at its own tip.
    assert report[trunk_idx]["worst_span_mm"] == pytest.approx(45.0)
    # b's own path is short enough that A's (much later, 58) completion
    # never arrives within B's own build window (z_top=45): active_z
    # clamps down to 45, i.e. the restraint might as well not exist --
    # b's worst span is its own FULL (unbraced-equivalent) path height.
    assert report[b_idx]["worst_span_mm"] == pytest.approx(45.0)


# ============================================================ 7. derive_brace_attachments geometry
def test_derive_brace_attachments_geometric_rules(cfg):
    f = tm.Forest()
    host_a = _member([0.0, 0.0, 30.0], [0.0, 0.0, 0.0], kind="trunk")
    a_idx = f.add_member(host_a)
    host_b = _member([10.0, 0.0, 30.0], [10.0, 0.0, 0.0], kind="trunk")
    b_idx = f.add_member(host_b)

    # brace1: X-pair rod, one end on each host's axis exactly.
    brace1 = _member([0.0, 0.0, 10.0], [10.0, 0.0, 15.0], kind="brace")
    br1_idx = f.add_member(brace1)

    # brace2: a model_rung -- p0 lands on host_a's axis (must attach),
    # p1 is embed_end (model-anchored) and lands EXACTLY on host_b's axis
    # too, but must be SKIPPED because it's the designed embed end, not a
    # real member attachment.
    brace2 = _member([0.0, 0.0, 20.0], [10.0, 0.0, 20.0], kind="model_rung",
                      embed_end="p1")
    br2_idx = f.add_member(brace2)

    # brace3: both ends >0.25mm from every host axis -- attaches nowhere.
    brace3 = _member([5.0, 0.0, 5.0], [5.0, 0.0, 25.0], kind="brace")
    br3_idx = f.add_member(brace3)

    for idx in (br1_idx, br2_idx, br3_idx):
        f.roots.remove(idx)
    f.braces = [br1_idx, br2_idx, br3_idx]

    attach = tr.derive_brace_attachments(f)

    assert attach[a_idx] == [(10.0, br1_idx), (20.0, br2_idx)]
    assert attach[b_idx] == [(15.0, br1_idx)]  # brace2's p1 correctly excluded
    all_brace_idxs = {b for zs in attach.values() for _z, b in zs}
    assert br3_idx not in all_brace_idxs


# ============================================================ 8. free_gaps / restraint_zs
def test_restraint_zs_and_free_gaps_exact_interval_arithmetic(cfg):
    """trunk is a root (base z contributes) with 2 live children (its own
    p0 contributes) and a brace attaching mid-span; child_a's own p1
    contributes because ITS parent (trunk) is a >=2-children junction."""
    f = tm.Forest()
    trunk = _member([0.0, 0.0, 40.0], [0.0, 0.0, 0.0], kind="trunk")
    trunk_idx = f.add_member(trunk)
    child_a = _member([0.0, 0.0, 70.0], [0.0, 0.0, 40.0], kind="shoot")
    a_idx = f.add_member(child_a)
    f.set_parent(a_idx, trunk_idx)
    child_b = _member([0.0, 0.0, 50.0], [0.0, 0.0, 40.0], kind="strut")
    b_idx = f.add_member(child_b)
    f.set_parent(b_idx, trunk_idx)
    stub_a = _member([0.0, 0.0, 72.0], [0.0, 0.0, 70.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub_a), a_idx)
    stub_b = _member([0.0, 0.0, 52.0], [0.0, 0.0, 50.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub_b), b_idx)

    brace = _member([0.0, 0.0, 25.0], [20.0, 0.0, 25.0], kind="brace")
    br_idx = f.add_member(brace)
    f.roots.remove(br_idx)
    f.braces = [br_idx]

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)

    assert report[trunk_idx]["restraint_zs"] == pytest.approx([0.0, 25.0, 40.0])
    assert report[trunk_idx]["free_gaps"] == [pytest.approx((0.0, 25.0)),
                                               pytest.approx((25.0, 40.0))]

    assert report[a_idx]["restraint_zs"] == pytest.approx([40.0])
    assert report[a_idx]["free_gaps"] == [pytest.approx((40.0, 70.0))]


# ============================================================ 9. resize_forest
def _tall_unbraced_forest(r0=1.0):
    f, trunk_idx, stub_idx = _plate_column_forest(top_z=100.0, tip_z=102.0)
    f.members[trunk_idx].r0 = r0
    f.members[trunk_idx].r1 = r0
    f.members[stub_idx].r0 = 0.3
    f.members[stub_idx].r1 = 0.35
    return f, trunk_idx, stub_idx


def _tall_braced_forest(r0=1.0):
    f, trunk_idx, stub_idx = _tall_unbraced_forest(r0=r0)
    for z in (15.0, 35.0, 55.0, 75.0, 95.0):
        brace = _member([0.0, 0.0, z], [2.0, 0.0, z], kind="brace")
        b_idx = f.add_member(brace)
        f.roots.remove(b_idx)
    f.braces = [i for i in range(len(f.members)) if f.members[i].kind == "brace"]
    return f, trunk_idx, stub_idx


def _expected_r(forest, idx, cfg, report):
    m = forest.members[idx]
    lean = cco._lean_deg(m.p0, m.p1)
    children = [c for c in forest.children[idx] if c not in forest.dead]
    r_murray = (sum(forest.members[c].r1 ** 4 for c in children)) ** 0.25 if children else 0.0
    r_tension = cco._r_tension_mm(m.carried_mm2, cfg)
    info = report[idx]
    r_trans = tr.r_transient_mm(info["worst_span_mm"], lean, info["n_terminals"], cfg)
    kind_floor = cfg.min_trunk_r_mm if m.kind == "trunk" else cfg.min_shoot_r_mm
    return max(r_murray, r_tension, r_trans, kind_floor)


def test_resize_forest_braced_column_comes_down_vs_unbraced(cfg):
    f_u, trunk_u, _ = _tall_unbraced_forest()
    f_b, trunk_b, _ = _tall_braced_forest()

    out_u = tr.resize_forest(f_u, plate_z=0.0, cfg=cfg)
    out_b = tr.resize_forest(f_b, plate_z=0.0, cfg=cfg)

    r_new_u = next(r for (i, _old, r) in out_u["resized"] if i == trunk_u)
    r_new_b = next(r for (i, _old, r) in out_b["resized"] if i == trunk_b)

    assert out_b["report"][trunk_b]["worst_span_mm"] == pytest.approx(20.0)
    assert r_new_b < r_new_u

    expected_b = _expected_r(f_b, trunk_b, cfg, out_b["report"])
    assert r_new_b == pytest.approx(expected_b)
    assert f_b.members[trunk_b].r0 == pytest.approx(expected_b)
    assert f_b.members[trunk_b].r1 == pytest.approx(expected_b)


def test_resize_forest_unbraced_tall_column_goes_up(cfg):
    f, trunk_idx, _ = _tall_unbraced_forest(r0=0.1)
    out = tr.resize_forest(f, plate_z=0.0, cfg=cfg)
    r_new = next(r for (i, _old, r) in out["resized"] if i == trunk_idx)
    assert r_new > 0.1
    expected = _expected_r(f, trunk_idx, cfg, out["report"])
    assert r_new == pytest.approx(expected)
    # the transient floor is what's actually engaging (not just the bare
    # kind floor) -- a 102mm unbraced span demands real thickness.
    assert r_new > cfg.min_trunk_r_mm


def test_resize_forest_preserves_murray_conservation(cfg):
    f, trunk_idx, a_idx, b_idx = _y_junction_forest()
    # deliberately break conservation before resize: undersized trunk,
    # oversized children.
    f.members[trunk_idx].r0 = f.members[trunk_idx].r1 = 0.2
    f.members[a_idx].r0 = f.members[a_idx].r1 = 1.5
    f.members[b_idx].r0 = f.members[b_idx].r1 = 1.5

    tr.resize_forest(f, plate_z=0.0, cfg=cfg)

    assert verify.murray_report(f, cfg) == []


def test_resize_forest_never_touches_stubs(cfg):
    f, trunk_idx, stub_idx = _tall_unbraced_forest()
    stub_r0_before = f.members[stub_idx].r0
    stub_r1_before = f.members[stub_idx].r1
    out = tr.resize_forest(f, plate_z=0.0, cfg=cfg)
    assert f.members[stub_idx].r0 == stub_r0_before
    assert f.members[stub_idx].r1 == stub_r1_before
    assert all(i != stub_idx for (i, _old, _new) in out["resized"])


def test_resize_forest_tapered_member_keeps_r0(cfg):
    """A single-child pass-through member (a tapered shoot carrying a tip
    taper) must keep its OWN r0 unchanged and only ever grow r1 up to
    max(r_req, r0)."""
    f = tm.Forest()
    trunk = _member([0.0, 0.0, 20.0], [0.0, 0.0, 0.0], kind="trunk")
    trunk_idx = f.add_member(trunk)
    shoot = _member([0.0, 0.0, 40.0], [0.0, 0.0, 20.0], r0=0.4, r1=0.6, kind="shoot")
    shoot_idx = f.add_member(shoot)
    f.set_parent(shoot_idx, trunk_idx)
    stub = _member([0.0, 0.0, 42.0], [0.0, 0.0, 40.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub), shoot_idx)

    r0_before = f.members[shoot_idx].r0
    tr.resize_forest(f, plate_z=0.0, cfg=cfg)
    assert f.members[shoot_idx].r0 == pytest.approx(r0_before)
    assert f.members[shoot_idx].r1 >= f.members[shoot_idx].r0 - 1e-9


# ============================================================ 10. determinism
def test_transient_report_and_resize_are_deterministic(cfg):
    f1, t1, a1, b1 = _y_junction_forest()
    f2, t2, a2, b2 = _y_junction_forest()

    r1 = tr.transient_report(f1, plate_z=0.0, cfg=cfg)
    r2 = tr.transient_report(f2, plate_z=0.0, cfg=cfg)
    assert r1 == r2

    out1 = tr.resize_forest(f1, plate_z=0.0, cfg=cfg)
    out2 = tr.resize_forest(f2, plate_z=0.0, cfg=cfg)
    assert out1["resized"] == out2["resized"]
    for idx in range(len(f1.members)):
        assert f1.members[idx].r0 == f2.members[idx].r0
        assert f1.members[idx].r1 == f2.members[idx].r1


# ============================================================ 11. n_terminals^0.125
def test_r_transient_mm_scales_by_n_terminals_pow_0_125(cfg):
    # simple direct formula check (pure, no forest needed)
    a = tr.r_transient_mm(100.0, 0.0, 1, cfg)
    b = tr.r_transient_mm(100.0, 0.0, 2, cfg)
    assert b / a == pytest.approx(2.0 ** 0.125)


def test_n_terminals_counted_correctly_two_stub_vs_one_stub_column(cfg):
    f_one, trunk_one, _ = _plate_column_forest()
    report_one = tr.transient_report(f_one, plate_z=0.0, cfg=cfg)
    assert report_one[trunk_one]["n_terminals"] == 1

    f_two = tm.Forest()
    trunk = _member([0.0, 0.0, 40.0], [0.0, 0.0, 0.0], kind="trunk")
    trunk_idx = f_two.add_member(trunk)
    stub_a = _member([0.0, 0.0, 42.0], [0.0, 0.0, 40.0], kind="stub", embed_end="p0")
    f_two.set_parent(f_two.add_member(stub_a), trunk_idx)
    stub_b = _member([2.0, 0.0, 42.0], [0.0, 0.0, 40.0], kind="stub", embed_end="p0")
    f_two.set_parent(f_two.add_member(stub_b), trunk_idx)
    report_two = tr.transient_report(f_two, plate_z=0.0, cfg=cfg)
    assert report_two[trunk_idx]["n_terminals"] == 2

    r_one = tr.r_transient_mm(report_one[trunk_one]["worst_span_mm"], 0.0,
                               report_one[trunk_one]["n_terminals"], cfg)
    r_two = tr.r_transient_mm(report_one[trunk_one]["worst_span_mm"], 0.0,
                               report_two[trunk_idx]["n_terminals"], cfg)
    assert r_two / r_one == pytest.approx(2.0 ** 0.125)


# ============================================================ 12. internal consistency
def test_span_intervals_max_matches_public_worst_span():
    """Defensive consistency check: whatever internal per-interval
    machinery transient_report uses to find the span intersecting a
    given member must agree with the public transient_worst_span on the
    SAME (restraints, base_z, z_top) input -- both are the same sweep,
    computed two different ways (a full-history max vs a max of only
    the intervals overlapping some window)."""
    cases = [
        ([(30.0, 60.0)], -5.0, 60.0),
        ([(30.0, 30.0)], -5.0, 60.0),
        ([], 0.0, 50.0),
        ([(40.0, 10.0), (10.0, 40.0)], 0.0, 50.0),
        ([(20.0, 45.0)], 0.0, 58.0),
    ]
    for restraints, base_z, z_top in cases:
        intervals = tr._span_intervals(restraints, base_z, z_top)
        assert max(hi - lo for lo, hi in intervals) == pytest.approx(
            tr.transient_worst_span(restraints, base_z, z_top))
