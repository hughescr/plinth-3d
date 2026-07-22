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

    # SPEC-ARM-FIX fix B (Phase 4d): a_idx no longer "inherits" the raw
    # 45.0 unchanged -- it now gets the per-member MOMENT-ARM arm, not
    # the flat span. a_idx spans [20, 55]; against the dominant interval
    # (R=0, a=45) its own arm is 45 - max(20, 0) == 25 (smaller than the
    # flat 45, since a_idx's own base (20) is ABOVE R -- the load below
    # a_idx's own base is the trunk's debt, not a_idx's); against the
    # trailing interval (R=20, a=58) its arm is 58 - max(20, 20) == 38
    # (a_idx IS the base member of THAT later span, since its own z_min
    # (20) sits AT R, so max(z_min, R) reduces to R -- full arm for that
    # span). worst_span_mm(a_idx) == max(25, 38) == 38 -- corrected down
    # from the old (wrong) flat-span answer of 45.
    assert report[a_idx]["worst_span_mm"] == pytest.approx(38.0)
    assert report[a_idx]["worst_span_mm"] != pytest.approx(45.0)  # the old flat-span (wrong) answer
    # the trunk (entirely below the junction) DOES still get the full
    # 45.0 -- it is the base member of the DOMINANT span (its own z_min
    # is 0, always <= R for every event on this path), so max(z_min, R)
    # reduces to R and the arm formula collapses to the old flat a - R
    # for it specifically. This is the "base member of a span keeps the
    # full arm" consequence, not a coincidence: it is the physical
    # restraint transmitting that free cantilever's reaction force at
    # its own tip.
    assert report[trunk_idx]["worst_span_mm"] == pytest.approx(45.0)
    # b's own path is short enough that A's (much later, 58) completion
    # never arrives within B's own build window (z_top=45): active_z
    # clamps down to 45, i.e. the restraint might as well not exist --
    # the RAW span is the full 45 (unbraced-equivalent path height).
    # SPEC-ARM-FIX fix B: b_idx's own z_min (20) sits ABOVE R=0 for the
    # dominant span, so its arm is reduced to 45 - max(20, 0) == 25, same
    # as its arm against the trailing (20, 45) span (b IS that span's
    # base member: max(20, 20) == 20). worst_span_mm(b) == 25.0 --
    # REWRITTEN from the pre-fix pin (45.0, the old flat-span answer).
    assert report[b_idx]["worst_span_mm"] == pytest.approx(25.0)
    assert report[b_idx]["worst_span_mm"] != pytest.approx(45.0)  # the old flat-span (wrong) answer


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
    worst_gap = max(hi - lo for lo, hi in info["free_gaps"])
    own_length = float(np.linalg.norm(np.asarray(m.p1, float) - np.asarray(m.p0, float)))
    z_extent = max(abs(float(m.p0[2]) - float(m.p1[2])), 1e-9)
    worst_gap_3d = worst_gap * (own_length / z_extent)
    r_deflect = placement._shaft_r_for_L(worst_gap_3d, cfg, lean)
    r_trans = tr.r_transient_mm(info["worst_span_mm"], lean, info["n_terminals"], cfg)
    kind_floor = cfg.min_trunk_r_mm if m.kind == "trunk" else cfg.min_shoot_r_mm
    return max(r_murray, r_tension, r_deflect, r_trans, kind_floor)


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


def test_resize_forest_r_deflect_uses_worst_free_gap_not_raw_own_length(cfg):
    """SPEC-ARM-FIX fix B follow-on (Phase 4d, recorded in resize_forest's
    own docstring): design-v4.md #6 requires r = max(r_load, r_deflect,
    r_transient, r_bend) -- r_deflect is an always-active floor, computed
    "per-segment, not per-member" (charter #6's own words): over the
    member's WORST FREE GAP between restraints (info["free_gaps"]), not
    its raw p0->p1 length. Before this fix, resize_forest never computed
    r_deflect at all -- a first attempt at fixing that used the raw
    length instead, which is correct for an UNBRACED member (free_gaps
    == the whole extent then) but silently defeats bracing's entire
    purpose for a BRACED one: a member with rungs placed along it would
    stay floored at its full original length forever, no matter how many
    restraints get inserted (caught by test_engine.py's own out-of-
    ownership test_brace_and_size_shrinks_a_braceable_tall_twin_column
    smoke test, which needs member_ml to actually SHRINK once braces are
    placed).

    Forest: a tall single member M (0->80) split by a brace attaching to
    its OWN axis at z=40 -- free_gaps become [(0,40), (40,80)], each
    HALF of M's raw 80mm length. This pins that r_deflect is computed
    from the worst SUB-gap (40mm), not the raw 80mm span."""
    f = tm.Forest()
    m = _member([0.0, 0.0, 80.0], [0.0, 0.0, 0.0], r0=0.05, r1=0.05, kind="trunk")
    m_idx = f.add_member(m)
    stub = _member([0.0, 0.0, 82.0], [0.0, 0.0, 80.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub), m_idx)

    brace = _member([0.0, 0.0, 40.0], [3.0, 0.0, 40.0], kind="brace")
    b_idx = f.add_member(brace)
    f.roots.remove(b_idx)
    f.braces = [b_idx]

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert report[m_idx]["free_gaps"] == [pytest.approx((0.0, 40.0)), pytest.approx((40.0, 80.0))]

    lean = cco._lean_deg(m.p0, m.p1)
    r_deflect_free_gap = placement._shaft_r_for_L(40.0, cfg, lean)
    r_deflect_raw_length = placement._shaft_r_for_L(80.0, cfg, lean)
    assert r_deflect_free_gap < r_deflect_raw_length  # sanity: the two formulations genuinely differ

    out = tr.resize_forest(f, plate_z=0.0, cfg=cfg)
    r_new = next(r for (i, _old, r) in out["resized"] if i == m_idx)
    expected = _expected_r(f, m_idx, cfg, report)
    assert r_new == pytest.approx(expected)


def test_resize_forest_r_deflect_scales_z_span_to_true_3d_length_for_leaning_member(cfg):
    """SPEC-ARM-FIX fix B follow-on (Phase 4d, second design decision in
    resize_forest's own docstring): free_gaps bounds live in Z-coordinate
    space, but placement._shaft_r_for_L wants the member's TRUE physical
    (3D) cantilever length -- for a LEANING member those differ (z-span <
    3D length whenever lean != 0). Passing the raw z-span directly
    silently undersizes r_deflect for any non-vertical member. Caught by
    test_cco.py's own test_engine_every_member_meets_its_own_load_and_
    deflection_floor on toed_slab_mesh (several ~57-60deg-leaning
    members failed the z-span-only version of this fix, unrelated to any
    bracing/free_gaps interaction -- toed_slab_mesh places zero braces in
    that run).

    Forest: an UNBRACED single leaning member M with equal xy-run and
    z-rise (an exact 45-degree lean, cco._lean_deg == atan2(dxy,dz)) --
    z-extent 40mm, true 3D length 40*sqrt(2) ~= 56.57mm. With no bracing,
    free_gaps == the whole z-extent (40mm) -- the z-span-only (broken)
    version would compute r_deflect from 40mm; the fixed version scales
    that to the true ~56.57mm 3D length via the member's own length/
    z-extent ratio, giving a strictly LARGER (correct) r_deflect."""
    f = tm.Forest()
    m = _member([40.0, 0.0, 40.0], [0.0, 0.0, 0.0], r0=0.05, r1=0.05, kind="trunk")
    m_idx = f.add_member(m)
    stub = _member([41.0, 0.0, 42.0], [40.0, 0.0, 40.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub), m_idx)

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert report[m_idx]["free_gaps"] == [pytest.approx((0.0, 40.0))]

    lean = cco._lean_deg(m.p0, m.p1)
    own_length = float(np.linalg.norm(np.asarray(m.p1) - np.asarray(m.p0)))
    assert own_length == pytest.approx(40.0 * (2 ** 0.5))
    r_deflect_z_span_only = placement._shaft_r_for_L(40.0, cfg, lean)
    r_deflect_true_3d = placement._shaft_r_for_L(own_length, cfg, lean)
    assert r_deflect_true_3d > r_deflect_z_span_only  # sanity: the two genuinely differ

    out = tr.resize_forest(f, plate_z=0.0, cfg=cfg)
    r_new = next(r for (i, _old, r) in out["resized"] if i == m_idx)
    assert r_new >= r_deflect_true_3d - 1e-9
    expected = _expected_r(f, m_idx, cfg, report)
    assert r_new == pytest.approx(expected)


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


# ============================================================ 13. verifier should-fixes: closed-interval + min-aggregation pins
def test_worst_span_closed_interval_boundary_touch(cfg):
    """SPEC-ARM-FIX fix B (Phase 4d): REWRITES the old (pre-fix) pin,
    deliberately -- this is a corrected spec, not a weakened gate. The
    old version of this test pinned the OLD flat-span attribution (a
    span touching M's own zlo at a single boundary point still charged M
    the FULL a-R span, giving 90.0). Under the NEW moment-arm formula
    (a - max(z_min(M), R)), a span that only touches M's zlo at a single
    point now correctly contributes ZERO from that span (M's own tip IS
    the point R touches, so max(z_min(M), R) == R == hi there -- no arm
    left) -- this is no longer where the closed/open boundary distinction
    is observable, since a zlo-touch collapses to 0 under the new
    formula regardless of open/closed. The closed-interval boundary that
    NOW matters is the other one: a restraint landing EXACTLY at a
    member's own zhi (its upper tip) -- whether that later, larger span
    counts M as "the base member" (closed: it does) or excludes it
    (open: it doesn't) is what decides whether M inherits that later
    span's full arm at all.

    Forest: trunk T(0->5) forks at z=5 into a zero-height decoy branch
    (immediate restraint: point_z=5.0, active_z=5.0, since the decoy's
    own completion equals the junction height) and a long pass-through
    path up to z_top=100 via shoot(5->100)+stub. T's own extent is
    [0, 5]: restraint R=5 lands EXACTLY on T's own zhi (5.0) -- a
    knife-edge touch, not genuine interior overlap.

    T's path carries one event, (point_z=5.0, active_z=5.0), producing
    two candidate spans: (R=0, a=5) [T's own early exposure, arm
    5 - max(0, 0) == 5] and (R=5, a=100) [T is the base member of THIS
    span too, since T's own z_min (0) <= R (5), so max(0, 5) == 5 -- arm
    100 - 5 == 95]. worst_span_mm(T) == max(5, 95) == 95.0 under the real
    closed-interval rule (lo <= zhi, i.e. 5 <= 5).

    Mutation check (recorded): temporarily changed the overlap gate to
    strict `if hi > zlo and lo < zhi:` (open-interval). Under that
    mutant, the (R=5, a=100) span is excluded for T (5 < 5 is False,
    fails the open lo<zhi test) -- T is left with only its own early
    exposure span, giving 5.0. Actual result when re-run against the
    mutant: AssertionError: assert 5.0 == 95.0 +- ... (report[trunk_idx]
    ["worst_span_mm"] came back 5.0, not 95.0). Mutation reverted exactly
    afterward (git diff on transient.py clean); this test passes again
    against the real closed-interval code.

    Second mutation check (recorded, discriminates the arm refinement
    itself against a "plain R" regression back to the OLD flat formula):
    temporarily changed the contribution to plain `hi - lo` (dropping
    the `max(zlo, lo)` refinement entirely). T is the base member here
    (z_min(T) == 0 <= every R on this path), so `max(zlo, lo) == lo`
    ALWAYS holds for T -- the plain-R mutant is mathematically identical
    to the real code for T, and does NOT get caught by this fixture
    (recorded honestly: this is exactly why
    test_moment_arm_tapers_along_chain_under_one_span below exists, using
    a member whose z_min is ABOVE some R, where the two formulas
    genuinely diverge)."""
    f = tm.Forest()
    trunk = _member([0.0, 0.0, 5.0], [0.0, 0.0, 0.0], kind="trunk")
    trunk_idx = f.add_member(trunk)

    decoy = _member([5.0, 0.0, 5.0], [0.0, 0.0, 5.0], kind="stub", embed_end="p0")
    decoy_idx = f.add_member(decoy)
    f.set_parent(decoy_idx, trunk_idx)

    shoot = _member([0.0, 0.0, 98.0], [0.0, 0.0, 5.0], kind="shoot")
    shoot_idx = f.add_member(shoot)
    f.set_parent(shoot_idx, trunk_idx)

    stub = _member([0.0, 0.0, 100.0], [0.0, 0.0, 98.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub), shoot_idx)

    # sanity: the path-level sweep matches the hand-derived spans.
    assert tr.transient_worst_span([(5.0, 5.0)], 0.0, 100.0) == pytest.approx(95.0)

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert report[trunk_idx]["worst_span_mm"] == pytest.approx(95.0)
    assert report[trunk_idx]["worst_span_mm"] != pytest.approx(5.0)  # the open-interval (wrong) answer


def test_moment_arm_excludes_member_entirely_below_a_later_restraint(cfg):
    """SPEC-ARM-FIX fix B, the overlap-gate design decision (see
    transient_report's docstring): a member entirely below a restraint
    that activates LATER in the sweep must get ZERO contribution from
    the span that opens up above that restraint -- it was already
    anchored by an EARLIER event, and charging it for load far above a
    restraint it doesn't even reach would resurrect a worse version of
    the bug this fix corrects.

    Forest: base(0->10) -> shoot(10->55, brace attaches at its OWN axis
    z=15) -> stub tip at 100. The brace's OTHER end floats at z=55 (so
    the restraint doesn't activate until the print reaches 55 -- v3's
    lagged-activation rule). Events on the path: [(15.0, 55.0)].
    Candidate spans: (R=0, a=55) [base's own early exposure, dominant for
    base: arm 55 - max(0, 0) == 55] and (R=15, a=100) [the LATER span,
    opening only once the brace is active].

    base's own extent is [0, 10] -- entirely below R=15 for the second
    span. Base's own worst_span_mm must be 55.0 (from its early
    exposure), NOT 100 - 15 == 85 (what the ungated bare formula
    `a - max(z_min, R)` would give, since max(0, 15) == 15 too -- the
    gate is what actually excludes it, not the arm refinement alone)."""
    f = tm.Forest()
    base = _member([0.0, 0.0, 10.0], [0.0, 0.0, 0.0], kind="trunk")
    base_idx = f.add_member(base)
    shoot = _member([0.0, 0.0, 55.0], [0.0, 0.0, 10.0], kind="shoot")
    shoot_idx = f.add_member(shoot)
    f.set_parent(shoot_idx, base_idx)
    stub = _member([0.0, 0.0, 100.0], [0.0, 0.0, 55.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub), shoot_idx)

    # brace: on-axis end at z=15 (attaches to `shoot`), floating end at
    # z=55 (gates activation -- v3's upper-endpoint rule).
    brace = _member([0.0, 0.0, 15.0], [20.0, 0.0, 55.0], kind="brace")
    b_idx = f.add_member(brace)
    f.roots.remove(b_idx)
    f.braces = [b_idx]

    attach = tr.derive_brace_attachments(f)
    assert attach[shoot_idx] == [(15.0, b_idx)]

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert report[base_idx]["worst_span_mm"] == pytest.approx(55.0)
    assert report[base_idx]["worst_span_mm"] != pytest.approx(85.0)  # the ungated (wrong) answer
    # shoot itself spans [10, 55] -- straddles R=15 (its own z_min (10)
    # <= R), so it IS the base member of the later span and correctly
    # gets that span's full arm: 100 - max(10, 15) == 85.
    assert report[shoot_idx]["worst_span_mm"] == pytest.approx(85.0)


def test_moment_arm_tapers_along_chain_under_one_span(cfg):
    """SPEC-ARM-FIX fix B's headline consequence: a chain of members
    under one long free-cantilever span sizes as a TAPER (each member
    higher up gets a strictly smaller arm), not the old uniform fat run
    (every member charged the full span length regardless of its own
    position within it).

    Forest: A(0->10) -> B(10->20, brace attaches at its own axis z=15,
    the brace's far end floats at z=16) -> C(20->25) -> stub tip at 27
    (path z_top == 27, from the stub's own tip). Events on the path:
    [(15.0, 16.0)]. Candidate spans: (R=0, a=16) [A's own early
    exposure, before the brace exists] and (R=15, a=27) [the later span,
    once the brace is active].

    - A (z_min=0): span2 doesn't overlap (its own zhi=10 < R=15) --
      A's worst_span_mm == 16.0 (span1 only: 16 - max(0,0) == 16).
    - B (z_min=10, straddles R=15 -- the base member of span2):
      span1 arm = 16 - max(10,0) == 6; span2 arm = 27 - max(10,15) == 12
      (max(z_min,R) == R here, full arm for the later span).
      worst_span_mm(B) == 12.0.
    - C (z_min=20, strictly ABOVE R=15 -- higher in the span): span1
      doesn't overlap (C's zlo=20 > span1's hi=16) -- span2 arm =
      27 - max(20,15) == 7 (max(z_min,R) == z_min here: C's OWN base
      reduces its arm below B's 12). worst_span_mm(C) == 7.0.

    A(16) > B(12) > C(7): strictly decreasing as position climbs within
    the structure -- the taper, not a flat 12.0 charged to all three.

    Mutation check #1 (recorded, "plain R" -- reverting to the pre-fix
    flat a - R formula, dropping the max-with-z_min refinement): B's
    span1 arm becomes 16 - 0 == 16 (instead of 6) and span2 arm becomes
    27 - 15 == 12 (unchanged, B is span2's base member either way) --
    worst_span_mm(B) becomes max(16, 12) == 16, not 12. C's span2 arm
    becomes 27 - 15 == 12 (instead of the correct 7) -- C would wrongly
    inherit the FULL later-span length despite sitting higher up. Actual
    result when re-run against the mutant: AssertionError: assert 16.0 ==
    12.0 +- ... (report[b_idx]["worst_span_mm"] came back 16.0, not
    12.0) and assert 12.0 == 7.0 +- ... (report[c_idx]["worst_span_mm"]
    came back 12.0, not 7.0). Mutation reverted exactly afterward (git
    diff on transient.py clean); this test passes again against the real
    code.

    Mutation check #2 (recorded, "plain z_min" -- using z_min(M) alone,
    dropping R from the max entirely): B's span2 arm becomes
    27 - z_min(B) == 27 - 10 == 17 (instead of the correct 12) -- B would
    be OVER-charged relative to the true base-member-gets-full-arm rule
    (B's own base sits BELOW R, so the physically correct arm is bounded
    by R, not by B's own lower tip). Actual result when re-run against
    the mutant: AssertionError: assert 17.0 == 12.0 +- ... (report[b_idx]
    ["worst_span_mm"] came back 17.0, not 12.0). Mutation reverted
    exactly afterward (git diff on transient.py clean); this test passes
    again against the real code (C does not discriminate this particular
    mutant -- z_min(C) already dominates R in the real formula too, so
    the plain-z_min mutant coincides with real code for C specifically;
    B alone carries this discrimination)."""
    f = tm.Forest()
    a = _member([0.0, 0.0, 10.0], [0.0, 0.0, 0.0], kind="trunk")
    a_idx = f.add_member(a)
    b = _member([0.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="shoot")
    b_idx = f.add_member(b)
    f.set_parent(b_idx, a_idx)
    c = _member([0.0, 0.0, 25.0], [0.0, 0.0, 20.0], kind="shoot")
    c_idx = f.add_member(c)
    f.set_parent(c_idx, b_idx)
    stub = _member([0.0, 0.0, 27.0], [0.0, 0.0, 25.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub), c_idx)

    brace = _member([0.0, 0.0, 15.0], [20.0, 0.0, 16.0], kind="brace")
    b_brace_idx = f.add_member(brace)
    f.roots.remove(b_brace_idx)
    f.braces = [b_brace_idx]

    attach = tr.derive_brace_attachments(f)
    assert attach[b_idx] == [(15.0, b_brace_idx)]

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert report[a_idx]["worst_span_mm"] == pytest.approx(16.0)
    assert report[b_idx]["worst_span_mm"] == pytest.approx(12.0)
    assert report[c_idx]["worst_span_mm"] == pytest.approx(7.0)
    assert report[a_idx]["worst_span_mm"] > report[b_idx]["worst_span_mm"] > report[c_idx]["worst_span_mm"]


# ============================================================ 14. Phase 4e (SPEC-PATHRUNG W6): worst_span_interval / worst_span_path
def test_worst_span_interval_and_path_on_y_junction(cfg):
    """Reuses the Y-junction fixture (test 6 above), which already pins
    worst_span_mm for trunk_idx (45.0, tied across BOTH terminal paths)
    and a_idx (38.0, only reachable via the a-path). This test pins the
    two NEW additive fields (Phase 4e, SPEC-PATHRUNG W6).

    trunk_idx: worst_span_mm==45.0 is achieved by BOTH the a-terminal's
    path (trunk=0, a=1, stub_a=3) and the b-terminal's path (trunk=0,
    b=2, stub_b=4) -- an EXACT tie. The pinned tie-break (smaller
    root-first member-index tuple) picks (0,1,3) over (0,2,4), since
    1 < 2 at the first differing position -- so the winning interval and
    path must come from the a-path's sweep, not b's, even though both
    give the identical worst_span_mm number. Winning interval (0.0, 45.0)
    (trunk's own dominant span, R=0 base_z, a=45 the sibling's
    completion). worst_span_path filters the winning path tuple to
    members whose OWN extent intersects (0.0, 45.0), excluding stubs:
    trunk [0,20] intersects; a_idx [20,55] intersects; stub_a excluded by
    kind regardless. -> [trunk_idx, a_idx].

    a_idx: worst_span_mm==38.0 is only reachable via the a-path (b's path
    never visits a_idx at all). Winning interval (20.0, 58.0) (a_idx's
    later span, where it IS the base member: max(z_min=20, R=20)==20).
    worst_span_path filters (trunk=0, a=1, stub_a=3) against (20, 58):
    trunk's own extent [0,20] touches the interval's lo (20) at a single
    boundary point -- CLOSED-interval overlap counts it (module
    convention, unchanged) -- so trunk_idx is included too, alongside
    a_idx itself; stub_a excluded by kind. -> [trunk_idx, a_idx]."""
    f, trunk_idx, a_idx, b_idx = _y_junction_forest()
    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)

    assert report[trunk_idx]["worst_span_mm"] == pytest.approx(45.0)
    assert report[trunk_idx]["worst_span_interval"] == (pytest.approx(0.0), pytest.approx(45.0))
    assert report[trunk_idx]["worst_span_path"] == [trunk_idx, a_idx]

    assert report[a_idx]["worst_span_mm"] == pytest.approx(38.0)
    assert report[a_idx]["worst_span_interval"] == (pytest.approx(20.0), pytest.approx(58.0))
    assert report[a_idx]["worst_span_path"] == [trunk_idx, a_idx]


def test_worst_span_interval_default_when_never_engaged(cfg):
    """A member with worst_span_mm == 0.0 (never the winner of any path's
    sweep -- can't happen for a live structural member in practice since
    the trailing interval alone always contributes SOMETHING > 0 unless
    the member has zero length, but the field must still have an honest,
    well-typed default) gets the degenerate (z_min, z_min) interval and
    an empty path -- not None, not a stale value from another member."""
    f = tm.Forest()
    # A zero-length member: p0 == p1, so every candidate interval's
    # contribution (hi - max(zlo, lo)) with zlo == zhi can still be
    # positive -- to genuinely get worst_span_mm == 0 we isolate the
    # member from every terminal's path entirely (no live stub
    # descendant reaches it -- it's a live root with no children and no
    # terminal registry entry, i.e. structurally orphaned).
    m = _member([0.0, 0.0, 10.0], [0.0, 0.0, 0.0], kind="trunk")
    idx = f.add_member(m)
    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert report[idx]["worst_span_mm"] == pytest.approx(0.0)
    assert report[idx]["worst_span_interval"] == (pytest.approx(0.0), pytest.approx(0.0))
    assert report[idx]["worst_span_path"] == []


def test_three_way_junction_uses_min_not_max_sibling_completion(cfg):
    """Pins min-over-siblings aggregation in _path_events (transient.py
    ~224-227, `active_z = min(completion(s) for s in siblings)`). Every
    OTHER fixture in this file has <=2 children per junction (1
    sibling, where min == max) -- this is the first genuinely 3-way
    junction in the suite, the only fixture that can discriminate min
    from max.

    trunk forks at z=20 into three children: a (the path under test,
    itself spanning 20->43, capped by its own stub to 45 -- "a's own
    path z_top == 45"), b (sibling, subtree completes at 60 via its own
    stub), and c (sibling, subtree completes at 40 via its own stub).
    The correct rule takes the MIN over siblings b and c:
    active_z = min(60, 40) == 40, point_z == 20 (the junction height).
    That yields two candidate spans on a's path: (0, 40) [dominant on
    the RAW sweep, length 40] and (20, 45) [length 25].

    SPEC-ARM-FIX fix B (Phase 4d) changed how those raw spans turn into
    a's own worst_span_mm: a's own z_min is 20 (ABOVE R=0 for the first
    span), so its arm is REDUCED to 40 - max(20, 0) == 20, not the raw
    40; against the second span a IS the base member (z_min 20 == R 20),
    so its arm is the full 45 - max(20, 20) == 25. worst_span_mm(a) ==
    max(20, 25) == 25.0 -- REWRITTEN from the pre-fix pin (40.0, the old
    flat-span answer) deliberately, per SPEC-ARM-FIX (a corrected spec,
    not a weakened gate). The min-vs-max discrimination itself has moved
    to trunk_idx below: trunk's own z_min (0) is <= every R on this path,
    so its arm formula collapses to the OLD flat a - R for it
    specifically (the "base member of a span keeps the full arm"
    consequence) -- trunk faithfully reflects the RAW event timing
    (min-based dominant span 'a' value == 40) the way a_idx no longer
    does after the arm refinement.

    Mutation check (recorded): temporarily changed line ~225's `min(...)`
    to `max(...)`. Under that mutant, active_z = max(60, 40) == 60,
    clamped down to a's own z_top (45) since 60 > 45 -- giving dominant
    candidate span (0, 45) instead of (0, 40). trunk_idx (z_min=0, base
    member, arm formula == raw span length) picks this up directly:
    worst_span_mm(trunk) becomes 45.0 instead of the real 40.0. Actual
    result when re-run against the mutant: AssertionError: assert 45.0 ==
    40.0 +- ... (report[trunk_idx]["worst_span_mm"] came back 45.0, not
    40.0). Mutation reverted exactly afterward (git diff on transient.py
    clean); this test passes again against the real min-based code."""
    f = tm.Forest()
    trunk = _member([0.0, 0.0, 20.0], [0.0, 0.0, 0.0], kind="trunk")
    trunk_idx = f.add_member(trunk)

    a = _member([0.0, 0.0, 43.0], [0.0, 0.0, 20.0], kind="shoot")
    a_idx = f.add_member(a)
    f.set_parent(a_idx, trunk_idx)
    stub_a = _member([0.0, 0.0, 45.0], [0.0, 0.0, 43.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub_a), a_idx)

    b = _member([0.0, 0.0, 58.0], [0.0, 0.0, 20.0], kind="shoot")
    b_idx = f.add_member(b)
    f.set_parent(b_idx, trunk_idx)
    stub_b = _member([0.0, 0.0, 60.0], [0.0, 0.0, 58.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub_b), b_idx)

    c = _member([0.0, 0.0, 38.0], [0.0, 0.0, 20.0], kind="shoot")
    c_idx = f.add_member(c)
    f.set_parent(c_idx, trunk_idx)
    stub_c = _member([0.0, 0.0, 40.0], [0.0, 0.0, 38.0], kind="stub", embed_end="p0")
    f.set_parent(f.add_member(stub_c), c_idx)

    assert len(f.children[trunk_idx]) == 3  # genuinely 3-way, not a Y

    report = tr.transient_report(f, plate_z=0.0, cfg=cfg)
    assert report[a_idx]["worst_span_mm"] == pytest.approx(25.0)
    assert report[a_idx]["worst_span_mm"] != pytest.approx(40.0)  # the old flat-span (wrong) answer
    assert report[trunk_idx]["worst_span_mm"] == pytest.approx(40.0)
    assert report[trunk_idx]["worst_span_mm"] != pytest.approx(45.0)  # the max-based (wrong) answer
