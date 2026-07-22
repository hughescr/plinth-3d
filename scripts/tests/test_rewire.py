"""Phase 3 tests (design-v4.md #7, #13): rewire.optimize's topology
mutation -- monotonicity, exact terminal accounting, §6 floors,
determinism, a hand-built twin-parallel-plate-root merge, full-engine
gate cleanliness, and the frozen-baseline A/B volume pin.
"""
import numpy as np
import pytest
from scipy.spatial import cKDTree

from support_placement import (cco, engine, perception, placement, routing,
                                rewire, transient, tree as tm, verify)
from test_cco import _own_floor_mm  # Phase 2's floor-check helper -- reused, not
                                     # reimplemented (design-v4.md instruction).

V3_TABLE_BASELINE_ML = 0.8955
AB_PIN_ML = V3_TABLE_BASELINE_ML * 1.15  # 1.0298...


# Module-scoped: engine.run is the expensive (insertion + rewire sweep)
# operation. Several tests below only READ its result (floors, gate
# cleanliness, the A/B pin) and don't need a FRESH run each -- sharing
# one run per fixture keeps the suite inside its wall-clock budget
# (only test_rewire_deterministic_across_repeated_runs needs two
# independent calls, which is the whole point of that test). Built
# directly (not via the function-scoped cfg/table_mesh fixtures --
# pytest forbids a module-scoped fixture depending on a function-scoped
# one) from the exact same plain constructors those fixtures use.
@pytest.fixture(scope="module")
def table_result():
    from support_placement import config as config_mod, mesh_io
    return engine.run(mesh_io.make_overhang_table(), config_mod.SupportConfig())


@pytest.fixture(scope="module")
def toed_slab_result():
    from support_placement import config as config_mod, mesh_io
    return engine.run(mesh_io.make_toed_slab(), config_mod.SupportConfig())


def _build_forest(mesh, cfg):
    from support_placement import perception
    islands = perception.overhang_islands(mesh, cfg)
    minima = perception.local_z_minima(mesh, cfg)
    contacts = placement.sample_candidates(mesh, islands, cfg)
    contacts, islands, _ = engine._ensure_minima_contacts(mesh, islands, contacts, minima, cfg)
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)
    carried = placement.load_partition(mesh, islands, contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    minima_flags = [bool(c.get("minimum")) for c in contacts]
    sample_points, _, sample_delicacy, _ = perception.delicacy_field(mesh, cfg)
    forest, unrouted = cco.build(mesh, contacts, tip_r, carried, minima_flags, plate_z,
                                  surface_tree, cfg, sample_points=sample_points,
                                  sample_delicacy=sample_delicacy)
    return forest, unrouted, surface_tree, plate_z


def _live_volume_ml(forest):
    idxs = rewire._live_member_indices(forest)
    return tm.volume_report([forest.members[i] for i in idxs], []).get("total_ml")


@pytest.mark.parametrize("fixture_name", ["table_mesh", "toed_slab_mesh"])
def test_rewire_monotonic_and_curve_nonincreasing(fixture_name, cfg, request):
    """design-v4.md #7: rewire never makes total volume worse, and the
    logged convergence curve (volume vs accepted move) is non-increasing
    by construction (every accepted move strictly decreased real total
    volume when it was accepted)."""
    mesh = request.getfixturevalue(fixture_name)
    forest, _unrouted, surface_tree, _plate_z = _build_forest(mesh, cfg)
    pre_vol = _live_volume_ml(forest)

    log = {}
    rewire.optimize(forest, mesh, surface_tree, cfg, log=log)
    post_vol = _live_volume_ml(forest)

    assert post_vol <= pre_vol + 1e-9
    curve = log["rewire"]["curve"]
    for (_, v_before), (_, v_after) in zip(curve, curve[1:]):
        assert v_after <= v_before + 1e-9


@pytest.mark.parametrize("fixture_name", ["table_mesh", "toed_slab_mesh"])
def test_rewire_preserves_exact_terminal_accounting(fixture_name, cfg, request):
    """rewire.py never touches "stub" members (excluded from every move's
    eligibility check) -- every contact's routed/unrouted status must be
    IDENTICAL before and after, not just "no worse"."""
    mesh = request.getfixturevalue(fixture_name)
    forest, unrouted, surface_tree, _plate_z = _build_forest(mesh, cfg)
    pre_attached = {i: (forest.parent[forest.terminal_of[i]] is not None)
                    for i in forest.terminal_of}

    rewire.optimize(forest, mesh, surface_tree, cfg, log={})

    post_attached = {i: (forest.parent[forest.terminal_of[i]] is not None)
                     for i in forest.terminal_of}
    assert post_attached == pre_attached
    # and the raw unrouted COUNT never increases (a stronger equality
    # already holds above; this is the literal "never more unrouted"
    # phrasing as an independent, cheap re-check).
    post_unrouted = [i for i, attached in post_attached.items() if not attached]
    assert len(post_unrouted) <= len(unrouted)


@pytest.mark.parametrize("fixture_name", ["table_mesh", "toed_slab_mesh"])
def test_rewire_floors_hold_post_rewire(fixture_name, cfg, request):
    """design-v4.md #6 floors (r_load, r_deflect, kind floor), reusing
    Phase 2's own exact regression helper (test_cco._own_floor_mm) rather
    than re-deriving the formula -- run the FULL engine (insertion +
    rewire) and check every live member."""
    result = request.getfixturevalue(fixture_name.replace("_mesh", "_result"))
    forest = result["forest"]
    unrouted_stub_idx = {forest.terminal_of[i] for i in result["unrouted"]}
    violations = []
    for mi, m in enumerate(forest.members):
        if mi in unrouted_stub_idx or mi in forest.dead or m.kind == "stub":
            continue
        floor = _own_floor_mm(m, cfg)
        r_have = max(m.r0, m.r1)
        if r_have < floor - 1e-6:
            violations.append(dict(member_idx=mi, kind=m.kind,
                                    r=round(r_have, 4), floor=round(floor, 4)))
    assert violations == [], violations


def test_rewire_deterministic_across_repeated_runs(cfg, table_mesh):
    """No RNG anywhere (design-v4.md #4/#7): re-running insertion+rewire
    on identical inputs reproduces the identical total volume. Uses the
    cheaper cco.build/rewire.optimize path (not the full engine.run
    fix-pass wrapper, whose own determinism test_engine_insertion_
    accounting-adjacent coverage already exercises) -- rewire's own
    determinism is what this test is chartered to pin, and building the
    forest twice this way keeps the suite's wall-clock budget."""
    f1, _u1, st1, _pz1 = _build_forest(table_mesh, cfg)
    log1 = {}
    rewire.optimize(f1, table_mesh, st1, cfg, log=log1)
    vol1 = _live_volume_ml(f1)

    f2, _u2, st2, _pz2 = _build_forest(table_mesh, cfg)
    log2 = {}
    rewire.optimize(f2, table_mesh, st2, cfg, log=log2)
    vol2 = _live_volume_ml(f2)

    assert vol1 == pytest.approx(vol2, rel=1e-12)
    assert log1["rewire"]["accepted"] == log2["rewire"]["accepted"]
    assert log1["rewire"]["curve"] == log2["rewire"]["curve"]


def test_rewire_merges_hand_built_twin_parallel_plate_roots(cfg, box_mesh):
    """The textbook parallel-column waste (design-v4.md #7's motivating
    case): two independent plate-rooted columns, 2mm apart in xy, each
    carrying identical load, far from the mesh so clearance is trivial.
    Greedy insertion never revisits this kind of decision; rewire must:
    a parentless root's free end is its p0 (_try_root_reanchor), so this
    also exercises that path specifically, not just junction-child
    reattach."""
    mesh = box_mesh
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3)
    z_top = plate_z + 20.0
    carried = 300.0

    def make_column(x):
        L = z_top - plate_z
        r = max(cco._r_tension_mm(carried, cfg),
                placement._shaft_r_for_L(L, cfg, 0.0), cfg.min_trunk_r_mm)
        return tm.Member(p0=[x, 40.0, z_top], p1=[x, 40.0, plate_z], r0=r, r1=r,
                          kind="trunk", owner=("test", int(x)), embed_end=None,
                          carried_mm2=carried)

    forest = tm.Forest()
    forest.add_member(make_column(30.0))
    forest.add_member(make_column(32.0))
    vol_before = rewire._total_volume_ml(forest)
    n_roots_before = len([r for r in forest.roots if r not in forest.dead])
    assert n_roots_before == 2

    log = {}
    rewire.optimize(forest, mesh, surface_tree, cfg, log=log)

    vol_after = rewire._total_volume_ml(forest)
    n_roots_after = len([r for r in forest.roots if r not in forest.dead])
    assert vol_after < vol_before - 1e-9
    assert n_roots_after < n_roots_before or vol_after < vol_before * 0.9
    assert verify.murray_report(forest, cfg) == []


@pytest.mark.parametrize("fixture_name", ["table_mesh", "toed_slab_mesh"])
def test_full_engine_gates_clean_post_rewire(fixture_name, cfg, request):
    """design-v4.md #11 gates that must be clean on both charter fixtures
    after insertion+rewire (uncovered_islands/coverage is a known, pre-
    existing Phase 2 residual on the adversarial table fixture -- not
    asserted here, matching test_engine.py's own existing scope)."""
    result = request.getfixturevalue(fixture_name.replace("_mesh", "_result"))
    v = result["verify"]
    assert v["clearance_violations"] == [], v
    assert v["connectivity_violations"] == [], v
    assert v["printability_violations"] == [], v
    assert v["murray_violations"] == [], v
    assert v["unsupported_minima"] == [], v
    tm.assert_all_certified(result["forest"])


def test_ab_pin_table_volume_vs_v3_baseline(table_result):
    """A/B pin (Phase 3 charter): table total_ml must not exceed v3's
    frozen baseline (0.8955 ml) by more than 15% (1.0298 ml). Reported
    prominently regardless of outcome -- see the phase report."""
    total_ml = table_result["volume"]["total_ml"]
    print(f"\nA/B PIN: table total_ml = {total_ml:.4f} "
          f"(v3 baseline {V3_TABLE_BASELINE_ML}, pin <= {AB_PIN_ML:.4f})")
    assert total_ml <= AB_PIN_ML, (
        f"table total_ml {total_ml:.4f} exceeds the 15%-over-v3 pin {AB_PIN_ML:.4f}")


# =================================================================
# Phase 4c brace-awareness (SPEC-INTEGRATION #2, "stale-brace impossibility")
# =================================================================
def _far_tree():
    return cKDTree(np.array([[1e6, 1e6, 1e6]]))


def _member(p0, p1, r0=0.5, r1=0.5, kind="trunk", owner=("t", 0), carried_mm2=0.0):
    return tm.Member(p0=p0, p1=p1, r0=r0, r1=r1, kind=kind, owner=owner, carried_mm2=carried_mm2)


def test_sweep_order_skips_braces(cfg):
    """forest.braces members are graph edges, never move candidates for
    any of the three move types -- excluded from _sweep_order entirely,
    even though a brace member would otherwise look like an ordinary
    reattach-root candidate (parentless, kind != stub)."""
    f = tm.Forest()
    root = f.add_member(_member([0.0, 0.0, 10.0], [0.0, 0.0, 0.0], kind="trunk"))
    a = f.add_member(_member([2.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="shoot"))
    f.set_parent(a, root)
    b = f.add_member(_member([-2.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="shoot"))
    f.set_parent(b, root)
    brace = f.add_brace(_member([0.0, 0.0, 5.0], [3.0, 0.0, 8.0], kind="brace"))

    order = rewire._sweep_order(f)
    idxs = {idx for _, idx in order}
    assert brace not in idxs
    assert root in idxs  # sanity: a real >=2-child junction still appears


def test_junction_slide_rejects_when_junction_itself_is_braced_host(cfg):
    """Geometry deliberately off the weighted-Fermat optimum so the
    unguarded slide demonstrably improves (sanity check) -- the guarded
    call on the SAME geometry must reject outright, proving the braced-
    host check fires before any search, not merely that no improvement
    existed."""
    def build():
        f = tm.Forest()
        root = f.add_member(_member([6.0, 0.0, 10.0], [0.0, 0.0, 0.0], r0=0.8, r1=0.8,
                                     kind="trunk", carried_mm2=50.0))
        a = f.add_member(_member([6.0, 0.0, 20.0], [6.0, 0.0, 10.0],
                                  kind="shoot", carried_mm2=20.0))
        f.set_parent(a, root)
        b = f.add_member(_member([-6.0, 0.0, 20.0], [6.0, 0.0, 10.0],
                                  kind="shoot", carried_mm2=20.0))
        f.set_parent(b, root)
        return f, root, a, b

    f, root, a, b = build()
    ok, saved = rewire._try_junction_slide(f, root, _far_tree(), cfg, set())
    assert ok is True and saved > 0.0  # sanity: this geometry genuinely improves

    f, root, a, b = build()
    ok, saved = rewire._try_junction_slide(f, root, _far_tree(), cfg, {root})
    assert (ok, saved) == (False, 0.0)


def test_junction_slide_rejects_when_a_child_is_braced_host(cfg):
    def build():
        f = tm.Forest()
        root = f.add_member(_member([6.0, 0.0, 10.0], [0.0, 0.0, 0.0], r0=0.8, r1=0.8,
                                     kind="trunk", carried_mm2=50.0))
        a = f.add_member(_member([6.0, 0.0, 20.0], [6.0, 0.0, 10.0],
                                  kind="shoot", carried_mm2=20.0))
        f.set_parent(a, root)
        b = f.add_member(_member([-6.0, 0.0, 20.0], [6.0, 0.0, 10.0],
                                  kind="shoot", carried_mm2=20.0))
        f.set_parent(b, root)
        return f, root, a, b

    f, root, a, b = build()
    ok, saved = rewire._try_junction_slide(f, root, _far_tree(), cfg, {a})
    assert (ok, saved) == (False, 0.0)


def test_twig_prune_rejects_when_m_idx_is_braced_host(cfg):
    """m is fat (r0=2.0) over a degenerate 0.2mm span, c is thin (r=0.5)
    -- an unguarded prune genuinely saves volume (sanity check); the
    braced-host guard on m_idx must block it outright (m_idx retires,
    stranding anything attached to its axis)."""
    def build():
        f = tm.Forest()
        m = f.add_member(_member([0.0, 0.0, 10.2], [0.0, 0.0, 10.0], r0=2.0, r1=0.3,
                                  kind="shoot"))
        c = f.add_member(_member([0.0, 0.0, 20.0], [0.0, 0.0, 10.2], r0=0.5, r1=0.5,
                                  kind="shoot"))
        f.set_parent(c, m)
        return f, m, c

    f, m, c = build()
    ok, saved = rewire._try_twig_prune(f, m, _far_tree(), cfg, set())
    assert ok is True and saved > 0.0  # sanity: this geometry genuinely improves

    f, m, c = build()
    ok, saved = rewire._try_twig_prune(f, m, _far_tree(), cfg, {m})
    assert (ok, saved) == (False, 0.0)


def test_twig_prune_rejects_when_surviving_child_is_braced_host(cfg):
    def build():
        f = tm.Forest()
        m = f.add_member(_member([0.0, 0.0, 10.2], [0.0, 0.0, 10.0], r0=2.0, r1=0.3,
                                  kind="shoot"))
        c = f.add_member(_member([0.0, 0.0, 20.0], [0.0, 0.0, 10.2], r0=0.5, r1=0.5,
                                  kind="shoot"))
        f.set_parent(c, m)
        return f, m, c

    f, m, c = build()
    ok, saved = rewire._try_twig_prune(f, m, _far_tree(), cfg, {c})
    assert (ok, saved) == (False, 0.0)


def test_root_reanchor_rejects_when_root_is_braced_host(cfg, box_mesh):
    """Reuses the exact twin-parallel-plate-root geometry
    test_rewire_merges_hand_built_twin_parallel_plate_roots already proves
    merges (unguarded sanity check below); the braced-host guard on the
    retiring root must block it outright."""
    mesh = box_mesh
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3)
    z_top = plate_z + 20.0
    carried = 300.0
    sample_points, _, sample_delicacy, _ = perception.delicacy_field(mesh, cfg)

    def make_column(x):
        L = z_top - plate_z
        r = max(cco._r_tension_mm(carried, cfg),
                placement._shaft_r_for_L(L, cfg, 0.0), cfg.min_trunk_r_mm)
        return tm.Member(p0=[x, 40.0, z_top], p1=[x, 40.0, plate_z], r0=r, r1=r,
                          kind="trunk", owner=("test", int(x)), embed_end=None,
                          carried_mm2=carried)

    def build():
        f = tm.Forest()
        r0 = f.add_member(make_column(30.0))
        r1 = f.add_member(make_column(32.0))
        return f, r0, r1

    f, r0, r1 = build()
    kdc = cco._KDCache(); kdc.rebuild(f)
    ok, saved = rewire._try_root_reanchor(f, r0, surface_tree, cfg, kdc, plate_z,
                                           sample_points, sample_delicacy, set())
    assert ok is True and saved > 0.0  # sanity: unguarded, this exact geometry merges

    f, r0, r1 = build()
    kdc = cco._KDCache(); kdc.rebuild(f)
    ok, saved = rewire._try_root_reanchor(f, r0, surface_tree, cfg, kdc, plate_z,
                                           sample_points, sample_delicacy, {r0})
    assert (ok, saved) == (False, 0.0)


def test_subtree_reattach_excludes_braced_host_from_bifurcation_candidates(cfg, box_mesh):
    """A junction-child (A) reattaches by bifurcating onto host H when H is
    unguarded (sanity check: H's own geometry visibly changes -- p1 moves
    to the split point, r1 changes). When H is a braced host, it must
    never be chosen as a bifurcation target (splitting it would strand
    whatever brace is attached to its axis) -- H's geometry must come out
    of the call completely untouched, whether the move falls back to a
    different candidate or fails outright."""
    mesh = box_mesh
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3)
    sample_points, _, sample_delicacy, _ = perception.delicacy_field(mesh, cfg)

    def build():
        f = tm.Forest()
        p = f.add_member(_member([50.0, 0.0, 20.0], [50.0, 0.0, 10.0],
                                  r0=1.0, r1=1.0, kind="trunk", carried_mm2=40.0))
        a = f.add_member(_member([55.0, 0.0, 30.0], [50.0, 0.0, 20.0],
                                  r0=0.5, r1=0.5, kind="shoot", carried_mm2=20.0))
        f.set_parent(a, p)
        b = f.add_member(_member([45.0, 0.0, 30.0], [50.0, 0.0, 20.0],
                                  r0=0.5, r1=0.5, kind="shoot", carried_mm2=20.0))
        f.set_parent(b, p)
        h = f.add_member(_member([52.0, 0.0, 25.0], [52.0, 0.0, 0.0],
                                  r0=0.8, r1=0.8, kind="trunk", carried_mm2=60.0))
        return f, p, a, b, h

    f, p, a, b, h = build()
    kdc = cco._KDCache(); kdc.rebuild(f)
    ok, saved = rewire._try_subtree_reattach(f, a, surface_tree, cfg, kdc, plate_z,
                                              sample_points, sample_delicacy, set())
    H = f.members[h]
    assert ok is True and saved > 0.0  # sanity: unguarded, A bifurcates onto H
    assert H.p1[2] != pytest.approx(0.0)  # H's own axis visibly changed (split)

    f, p, a, b, h = build()
    H_p0, H_p1, H_r0, H_r1 = (f.members[h].p0.copy(), f.members[h].p1.copy(),
                               f.members[h].r0, f.members[h].r1)
    kdc = cco._KDCache(); kdc.rebuild(f)
    rewire._try_subtree_reattach(f, a, surface_tree, cfg, kdc, plate_z,
                                  sample_points, sample_delicacy, {h})
    H = f.members[h]
    assert np.array_equal(H.p0, H_p0) and np.array_equal(H.p1, H_p1)
    assert H.r0 == H_r0 and H.r1 == H_r1


def test_subtree_reattach_allowed_when_root_itself_is_braced_host(cfg, box_mesh):
    """The junction-child case never moves the detached subtree's OWN
    geometry (only its topology/parent pointer) -- a braced host AT
    root_idx itself must not block this move, unlike root-reanchor (where
    root_idx retires) or twig-prune/junction-slide (where geometry moves)."""
    mesh = box_mesh
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3)
    sample_points, _, sample_delicacy, _ = perception.delicacy_field(mesh, cfg)

    f = tm.Forest()
    p = f.add_member(_member([50.0, 0.0, 20.0], [50.0, 0.0, 10.0],
                              r0=1.0, r1=1.0, kind="trunk", carried_mm2=40.0))
    a = f.add_member(_member([55.0, 0.0, 30.0], [50.0, 0.0, 20.0],
                              r0=0.5, r1=0.5, kind="shoot", carried_mm2=20.0))
    f.set_parent(a, p)
    b = f.add_member(_member([45.0, 0.0, 30.0], [50.0, 0.0, 20.0],
                              r0=0.5, r1=0.5, kind="shoot", carried_mm2=20.0))
    f.set_parent(b, p)
    f.add_member(_member([52.0, 0.0, 25.0], [52.0, 0.0, 0.0],
                          r0=0.8, r1=0.8, kind="trunk", carried_mm2=60.0))
    A_p0, A_p1, A_r0, A_r1 = (f.members[a].p0.copy(), f.members[a].p1.copy(),
                               f.members[a].r0, f.members[a].r1)
    old_parent = f.parent[a]

    kdc = cco._KDCache(); kdc.rebuild(f)
    ok, saved = rewire._try_subtree_reattach(f, a, surface_tree, cfg, kdc, plate_z,
                                              sample_points, sample_delicacy, {a})
    assert ok is True and saved > 0.0
    A = f.members[a]
    assert np.array_equal(A.p0, A_p0) and np.array_equal(A.p1, A_p1)
    assert A.r0 == A_r0 and A.r1 == A_r1
    assert f.parent[a] != old_parent  # topology DID change -- only geometry is protected


def test_live_member_indices_includes_braces(cfg):
    """tree.Forest.braces' own docstring: braces ARE live printed volume
    -- _live_member_indices needs no special-case for them (kind != "stub"
    already keeps them in)."""
    f = tm.Forest()
    trunk = f.add_member(_member([0.0, 0.0, 10.0], [0.0, 0.0, 0.0], kind="trunk"))
    brace = f.add_brace(_member([0.0, 0.0, 5.0], [3.0, 0.0, 8.0], kind="brace"))
    live = rewire._live_member_indices(f)
    assert trunk in live
    assert brace in live


def test_stale_brace_assert_fires_on_hand_corrupted_forest(cfg):
    """A hand-corrupted forest (a host's axis moved out from under a brace
    endpoint WITHOUT going through any of the guarded move types) must
    make _assert_no_stale_braces raise -- crash over corruption, proving
    the independent check itself is a real tripwire and not a no-op."""
    f = tm.Forest()
    host = f.add_member(_member([0.0, 0.0, 20.0], [0.0, 0.0, 0.0], kind="trunk"))
    f.add_member(_member([5.0, 0.0, 20.0], [5.0, 0.0, 0.0], kind="trunk"))  # 2nd host for p1
    brace = f.add_brace(_member([0.0, 0.0, 10.0], [5.0, 0.0, 13.0], kind="brace"))
    rewire._assert_no_stale_braces(f)  # sanity: correctly-attached brace passes

    # corrupt: move the host's axis without any certificate/topology
    # bookkeeping (exactly what the guarded move types above exist to
    # make impossible) -- the brace's p0 (0,0,10) no longer lies on it.
    f.members[host].p0 = [8.0, 0.0, 20.0]
    f.members[host].p1 = [8.0, 0.0, 0.0]
    with pytest.raises(AssertionError):
        rewire._assert_no_stale_braces(f)


def test_optimize_never_strands_a_brace_on_a_real_run(cfg, box_mesh):
    """A hostile move sequence cannot strand a brace: the twin-parallel-
    plate-root pair from test_rewire_merges_hand_built_twin_parallel_
    plate_roots (which UNGUARDED rewire happily merges, retiring one
    root) gets an X-pair brace connecting the two columns -- both are now
    braced hosts. A real rewire.optimize() sweep over this forest must
    complete without raising, and every live brace endpoint must still be
    geometrically attached afterward (independently re-checked via
    transient.derive_brace_attachments, not just trusting no exception
    was raised)."""
    mesh = box_mesh
    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3)
    z_top = plate_z + 20.0
    carried = 300.0

    def make_column(x):
        L = z_top - plate_z
        r = max(cco._r_tension_mm(carried, cfg),
                placement._shaft_r_for_L(L, cfg, 0.0), cfg.min_trunk_r_mm)
        return tm.Member(p0=[x, 40.0, z_top], p1=[x, 40.0, plate_z], r0=r, r1=r,
                          kind="trunk", owner=("test", int(x)), embed_end=None,
                          carried_mm2=carried)

    f = tm.Forest()
    c1 = f.add_member(make_column(30.0))
    c2 = f.add_member(make_column(32.0))
    zmid = z_top - 10.0
    br_idx = f.add_brace(_member([30.0, 40.0, zmid], [32.0, 40.0, zmid],
                                  r0=cfg.brace_r_mm, r1=cfg.brace_r_mm, kind="brace"))

    attach_before = transient.derive_brace_attachments(f)
    assert set(attach_before.keys()) == {c1, c2}  # both columns are braced hosts

    log = {}
    rewire.optimize(f, mesh, surface_tree, cfg, log=log)  # must not raise

    assert br_idx in f.braces and br_idx not in f.dead
    attach_after = transient.derive_brace_attachments(f)
    # Every live brace endpoint (both, here -- neither end is embed_end)
    # must still resolve to a live host axis within tolerance -- at least
    # two (host, brace_idx) matches total, one per endpoint.
    matches = [host_idx for host_idx, entries in attach_after.items()
               for _z, bi in entries if bi == br_idx]
    assert len(matches) >= 2, f"brace {br_idx} left stranded: {attach_after}"


# =================================================================
# Phase 4e (SPEC-PATHRUNG W2): degenerate-member prevention + retirement
# =================================================================
def test_degenerate_retire_folds_two_or_more_children_onto_parent(cfg):
    """The Treant verify member_idx 577 class (kind=shoot, length ~1e-6,
    near_horizontal printability violation): a junction collapsed to a
    near-zero-length span with 2+ children -- _try_twig_prune's own
    len(children) == 1 gate never reaches this shape. Direct
    construction (per spec): parent P -> degenerate M (length 1e-8mm,
    well under DEGENERATE_LEN_MM) -> two children A, B. Retiring M
    splices A and B directly onto P (their own p0 already coincides
    with M's p1 to float precision, no geometry change needed) and
    marks M dead."""
    f = tm.Forest()
    p = f.add_member(_member([0.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="trunk"))
    m = f.add_member(_member([0.0, 0.0, 10.0], [0.0, 0.0, 10.0 - 1e-8],
                              kind="shoot"))
    f.set_parent(m, p)
    a = f.add_member(_member([2.0, 0.0, 0.0], [0.0, 0.0, 10.0 - 1e-8], kind="shoot"))
    f.set_parent(a, m)
    b = f.add_member(_member([-2.0, 0.0, 0.0], [0.0, 0.0, 10.0 - 1e-8], kind="shoot"))
    f.set_parent(b, m)
    assert rewire._is_degenerate_span(f.members[m].p0, f.members[m].p1)

    ok, saved = rewire._try_degenerate_retire(f, m, _far_tree(), cfg, set())

    assert ok is True
    assert saved >= 0.0
    assert m in f.dead
    assert set(f.children[p]) == {a, b}
    assert f.parent[a] == p and f.parent[b] == p
    assert f.dirty_members() == []
    live = [f.members[i] for i in range(len(f.members)) if i not in f.dead]
    assert tm.printability_report(live, cfg) == []


def test_degenerate_retire_detaches_zero_child_dead_weight(cfg):
    """A degenerate member with ZERO children and no stub role is dead
    weight per spec -- detach from its parent and mark_dead, no
    splicing needed since it has nothing to splice."""
    f = tm.Forest()
    p = f.add_member(_member([0.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="trunk"))
    m = f.add_member(_member([0.0, 0.0, 10.0], [0.0, 0.0, 10.0 - 1e-8], kind="shoot"))
    f.set_parent(m, p)

    ok, saved = rewire._try_degenerate_retire(f, m, _far_tree(), cfg, set())

    assert ok is True
    assert saved >= 0.0
    assert m in f.dead
    assert m not in f.children[p]
    assert f.dirty_members() == []
    live = [f.members[i] for i in range(len(f.members)) if i not in f.dead]
    assert tm.printability_report(live, cfg) == []


def test_degenerate_retire_skips_stub_regardless_of_length(cfg):
    """A real contact stub is never degenerate-retired regardless of
    length -- it is the model surface contact, not a topology artifact
    (mirrors printability_report's own stub exemption, design decision
    recorded there)."""
    f = tm.Forest()
    p = f.add_member(_member([0.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="trunk"))
    m = f.add_member(_member([0.0, 0.0, 10.0], [0.0, 0.0, 10.0 - 1e-8], kind="stub"))
    f.set_parent(m, p)
    assert rewire._try_degenerate_retire(f, m, _far_tree(), cfg, set()) == (False, 0.0)


def test_degenerate_retire_leaves_single_child_pass_through_to_twig_prune(cfg):
    """A degenerate member with EXACTLY 1 child is _try_twig_prune's own
    job (unchanged) -- _try_degenerate_retire must not duplicate or race
    it, even though such a member also satisfies the length check."""
    f = tm.Forest()
    p = f.add_member(_member([0.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="trunk"))
    m = f.add_member(_member([0.0, 0.0, 10.0], [0.0, 0.0, 10.0 - 1e-8], kind="shoot"))
    f.set_parent(m, p)
    c = f.add_member(_member([0.0, 0.0, 0.0], [0.0, 0.0, 10.0 - 1e-8], kind="shoot"))
    f.set_parent(c, m)
    assert rewire._try_degenerate_retire(f, m, _far_tree(), cfg, set()) == (False, 0.0)


def test_junction_geometry_at_rejects_a_collapsed_child_span(cfg):
    """SPEC-PATHRUNG W2 root-cause guard #2 ("junction slide collapsing a
    child"): a candidate slide position J landing within
    DEGENERATE_LEN_MM of a child's OWN p0 would create exactly the
    member_idx 577 shape (a near-zero-length span) -- _junction_geometry_
    at, the single choke point both the hill-climb search and the final
    application route through, must reject such a J outright (return
    None) rather than ever let it become a real candidate."""
    child = _member([1.0, 0.0, 5.0], [0.0, 0.0, 0.0])  # child's own p0
    m = _member([0.0, 0.0, 0.0], [0.0, 0.0, 20.0])       # m.p1 unrelated
    J_collapsed = np.array([1.0, 0.0, 5.0 + 1e-9])       # within DEGENERATE_LEN_MM of child.p0
    assert rewire._junction_geometry_at(J_collapsed, m, [child], cfg) is None

    J_ok = np.array([1.0, 0.0, 6.0])  # a real, non-degenerate position
    assert rewire._junction_geometry_at(J_ok, m, [child], cfg) is not None


def test_reattach_rejects_degenerate_connector_candidate(cfg, monkeypatch):
    """SPEC-PATHRUNG W2 root-cause guard #1 ("reattach connector built
    from se to a Fermat/projected point that coincides with se"): a
    candidate builder that would draw a connector from se to a
    coincident (or near-coincident) point must be rejected before ever
    being scored/committed -- proven directly against the exact choke
    point (the n0..len(forest.members) degenerate-span check added to
    _try_subtree_reattach's own candidate loop) by monkeypatching
    _reattach_candidate_builders to yield EXACTLY that one degenerate
    shape (isolating the guard from cco.py's real candidate machinery,
    which is not this module's ownership) -- with no other candidate
    available, the whole move must be rejected outright (False, 0.0),
    never silently commit a zero-length connector."""
    f = tm.Forest()
    parent = f.add_member(_member([0.0, 0.0, 30.0], [0.0, 0.0, 20.0], kind="trunk"))
    root = f.add_member(_member([2.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="shoot"))
    f.set_parent(root, parent)
    sibling = f.add_member(_member([-2.0, 0.0, 20.0], [0.0, 0.0, 10.0], kind="shoot"))
    f.set_parent(sibling, parent)
    se = f.members[root].p1.copy()

    def degenerate_builder():
        # se to a point coinciding with se -- exactly the "projected
        # point that coincides with se" shape the spec names.
        f.add_member(tm.Member(p0=se, p1=se.copy(), r0=0.5, r1=0.5,
                                kind="shoot", owner=("contact", root)))

    def fake_candidates(*_args, **_kwargs):
        yield degenerate_builder

    monkeypatch.setattr(rewire, "_reattach_candidate_builders", fake_candidates)
    n_members_before = len(f.members)

    ok, saved = rewire._try_subtree_reattach(f, root, _far_tree(), cfg, None,
                                              0.0, np.zeros((0, 3)), np.zeros(0), set())

    assert (ok, saved) == (False, 0.0)
    assert len(f.members) == n_members_before  # nothing committed, degenerate or not
    for i, m in enumerate(f.members):
        if i in f.dead:
            continue
        assert not rewire._is_degenerate_span(m.p0, m.p1)
