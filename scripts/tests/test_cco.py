import numpy as np
import pytest
from scipy.optimize import minimize
from scipy.spatial import cKDTree

from support_placement import cco, placement, tree as tm, verify, engine as eng


# ------------------------------------------------------- Xia closed form --
@pytest.mark.parametrize("m1,m2", [(1.0, 1.0), (1.0, 3.0), (1.0, 9.0),
                                    (2.0, 7.0), (1.0, 100.0)])
def test_xia_children_always_ninety_degrees_apart(m1, m2):
    turn1, turn2, theta12 = cco.xia_angles(m1, m2)
    assert theta12 == pytest.approx(90.0)
    assert (turn1 + turn2) == pytest.approx(90.0, abs=1e-6)


def test_xia_equal_load_gives_45_45():
    turn1, turn2, _ = cco.xia_angles(1.0, 1.0)
    assert turn1 == pytest.approx(45.0, abs=1e-6)
    assert turn2 == pytest.approx(45.0, abs=1e-6)


def _weighted_fermat_numeric(A, B, C, w1, w0, w2):
    """Independent numeric minimization of the weighted-Fermat objective
    (scipy.optimize, not cco's own Weiszfeld iteration) -- the oracle
    test_cco.py's charter calls for: brute-force-verify the closed-form
    angles are the true optimum, not just self-consistent with cco's own
    approximation method."""
    A, B, C = np.asarray(A, float), np.asarray(B, float), np.asarray(C, float)

    def obj(J):
        return (w1 * np.linalg.norm(J - A) + w0 * np.linalg.norm(J - B)
                + w2 * np.linalg.norm(J - C))

    x0 = (A + B + C) / 3.0
    res = minimize(obj, x0, method="Nelder-Mead",
                    options=dict(xatol=1e-9, fatol=1e-12, maxiter=20000, maxfev=20000))
    assert res.success, res
    return res.x


@pytest.mark.parametrize("m1,m2", [(1.0, 1.0), (1.0, 3.0), (1.0, 9.0)])
def test_xia_oracle_matches_scipy_numeric_minimization(m1, m2):
    """design-v4.md #13/#14 Xia oracle: for load ratios 1:1, 1:3, 1:9,
    scipy.optimize numeric minimization of the weighted Fermat objective
    (alpha=1/2 weights = sqrt(load)) reproduces the closed-form angles
    within 1 degree."""
    A = np.array([-8.0, 2.0, 3.0])    # host.p0 -- child 1 ("old continuation")
    B = np.array([1.0, -3.0, -10.0])  # host.p1 -- sinkward/parent direction
    C = np.array([6.0, 5.0, 1.0])     # terminal -- child 2
    w1, w2 = m1 ** 0.5, m2 ** 0.5
    w0 = (m1 + m2) ** 0.5
    J = _weighted_fermat_numeric(A, B, C, w1, w0, w2)

    uA = (A - J) / np.linalg.norm(A - J)
    uB = (B - J) / np.linalg.norm(B - J)
    uC = (C - J) / np.linalg.norm(C - J)

    turn1, turn2, theta12 = cco.xia_angles(m1, m2)

    theta12_numeric = float(np.degrees(np.arccos(np.clip(np.dot(uA, uC), -1, 1))))
    assert abs(theta12_numeric - theta12) < 1.0

    # theta_i is measured to the SINKWARD unit vector uB (design-v4.md
    # #5's own convention); turn_i = 180 - theta_i since "parent
    # continuation" is the upward extrapolation past J, opposite uB.
    theta1_numeric = float(np.degrees(np.arccos(np.clip(np.dot(uA, uB), -1, 1))))
    theta2_numeric = float(np.degrees(np.arccos(np.clip(np.dot(uC, uB), -1, 1))))
    assert abs((180.0 - theta1_numeric) - turn1) < 1.0
    assert abs((180.0 - theta2_numeric) - turn2) < 1.0


def test_cone_clamp_engages_when_free_fermat_point_violates_cone(cfg):
    """Cone clamp activation (design-v4.md #5): demonstrate the FREE
    (unconstrained) weighted-Fermat point for a deliberately lopsided
    geometry (terminal far out sideways, barely below the host's own
    top) violates the printability cone, then show the actual builder
    output does not -- the clamp/fallback made the difference."""
    A = np.array([0.0, 0.0, 0.0])      # host.p0
    B = np.array([0.0, 0.0, -20.0])    # host.p1 (sinkward, toward plate)
    C = np.array([15.0, 0.0, -0.5])    # terminal stub_end
    m1, m2 = 40.0, 5.0
    w1, w0, w2 = m1 ** 0.5, (m1 + m2) ** 0.5, m2 ** 0.5
    j0 = A + 0.5 * (B - A)
    J_free = cco._weighted_fermat_point([(A, w1), (B, w0), (C, w2)], j0, n_iter=30)
    free_lean = cco._lean_deg(C, J_free)
    assert free_lean > cfg.branch_angle_deg + 1.0  # the free optimum DOES violate the cone

    f = tm.Forest()
    host = tm.Member(p0=A, p1=B, r0=1.0, r1=1.0, kind="trunk",
                      owner=("contact", 0), embed_end=None, carried_mm2=m1)
    host_idx = f.add_member(host)
    stub = tm.Member(p0=C - np.array([0.0, 0.0, 0.3]), p1=C, r0=0.3, r1=0.4,
                      kind="stub", owner=("contact", 1), embed_end="p0", carried_mm2=m2)
    stub_idx = f.add_member(stub)
    build_fn = cco._make_bifurcation_builder(f, 1, stub_idx, C, 0.4, m2, host_idx, -20.0, cfg)
    build_fn()

    child = next(m for m in f.members if m.kind == "shoot")
    assert cco._within_cone(child.p0, child.p1, child.r1, cfg)


# ------------------------------------------------------------ Murray law --
def test_murray_propagation_matches_r4_sum_formula(cfg):
    f = tm.Forest()
    root = tm.Member(p0=[0.0, 0.0, 10.0], p1=[0.0, 0.0, -30.0], r0=2.0, r1=2.0,
                      kind="trunk", owner=("root", 0), embed_end=None, carried_mm2=500.0)
    root_idx = f.add_member(root)
    r_child_new = 0.9
    cco._propagate_murray(f, root_idx, r_child_new, added_carried_mm2=10.0, cfg=cfg)
    expected_murray = (2.0 ** 4 + r_child_new ** 4) ** 0.25
    # a fat pre-existing root (2.0mm) with a small added child load makes
    # Murray the binding floor over tension/deflection/kind -- exact
    # equality, not just >=.
    assert root.r0 == pytest.approx(expected_murray, rel=1e-6)
    assert root.r0 == root.r1


def test_murray_conservation_holds_across_hand_built_three_terminal_tree(cfg):
    """Murray r^4 conservation checked on a hand-built, stacked-junction
    3-terminal tree (terminals A, B merge into `mid`; `mid` and terminal
    C merge into `root`) -- exercises propagation through TWO junction
    levels, not just one."""
    f = tm.Forest()
    root = tm.Member(p0=[0.0, 0.0, 10.0], p1=[0.0, 0.0, -30.0], r0=1.0, r1=1.0,
                      kind="trunk", owner=("root", 0), embed_end=None, carried_mm2=0.0)
    root_idx = f.add_member(root)
    mid = tm.Member(p0=[5.0, 0.0, 20.0], p1=[0.0, 0.0, 10.0], r0=1.0, r1=1.0,
                     kind="shoot", owner=("mid", 0), embed_end=None, carried_mm2=0.0)
    mid_idx = f.add_member(mid)
    f.set_parent(mid_idx, root_idx)
    term_c = tm.Member(p0=[-8.0, 0.0, 15.0], p1=[0.0, 0.0, 10.0], r0=0.5, r1=0.5,
                        kind="shoot", owner=("contact", 2), embed_end=None, carried_mm2=6.0)
    term_c_idx = f.add_member(term_c)
    f.set_parent(term_c_idx, root_idx)
    term_a = tm.Member(p0=[10.0, 0.0, 25.0], p1=[5.0, 0.0, 20.0], r0=0.4, r1=0.4,
                        kind="shoot", owner=("contact", 0), embed_end=None, carried_mm2=4.0)
    term_a_idx = f.add_member(term_a)
    f.set_parent(term_a_idx, mid_idx)
    term_b = tm.Member(p0=[0.0, 0.0, 25.0], p1=[5.0, 0.0, 20.0], r0=0.45, r1=0.45,
                        kind="shoot", owner=("contact", 1), embed_end=None, carried_mm2=5.0)
    term_b_idx = f.add_member(term_b)
    f.set_parent(term_b_idx, mid_idx)

    mid.r0 = mid.r1 = (term_a.r1 ** 4 + term_b.r1 ** 4) ** 0.25
    root.r0 = root.r1 = (mid.r1 ** 4 + term_c.r1 ** 4) ** 0.25

    assert verify.murray_report(f, cfg) == []

    root.r0 = root.r1 = 0.3  # break conservation at the ROOT junction only
    violations = verify.murray_report(f, cfg)
    assert len(violations) == 1
    assert violations[0]["member_idx"] == root_idx


def test_murray_propagation_combines_against_entry_end_not_max(cfg):
    """B2 regression (verifier blocker): a member flows INTO its parent at
    the parent's p0 (tree.Forest's documented convention -- p1 is always
    the root-ward end, p0 the terminal-ward/entry-facing end; verify.
    murray_report checks exactly this: `parent.r0**4 >= sum(child.r1**4)`).
    So when _propagate_murray fattens an ancestor for a newly-merged
    child, it must combine the new child's radius against THAT
    ancestor's own r0 (the end its OWN children already flow into) -- not
    max(r0, r1), which pulls in the unrelated root-ward exit radius.

    This ancestor is deliberately NON-UNIFORM (r0=0.3, r1=1.2) to expose
    the bug: the buggy code combined against max(0.3, 1.2) = 1.2, giving
    (1.2**4 + 0.5**4)**0.25 ~= 1.209mm for a new r=0.5 child -- a 2.3x
    inflation over the correct (0.3**4 + 0.5**4)**0.25 ~= 0.515mm
    (verifier's exact repro numbers). Length/kind are chosen (L=5mm
    vertical, kind="shoot") so neither the deflection floor (clamps to
    min_shaft_r_mm=0.3 at this length) nor the shoot kind floor (0.35)
    exceeds the correct Murray value -- Murray is the binding term, so
    this is an exact-equality check, not just a lower bound."""
    f = tm.Forest()
    ancestor = tm.Member(p0=[0.0, 0.0, 0.0], p1=[0.0, 0.0, -5.0], r0=0.3, r1=1.2,
                          kind="shoot", owner=("contact", 0), embed_end=None,
                          carried_mm2=0.0)
    a_idx = f.add_member(ancestor)
    r_child = 0.5
    cco._propagate_murray(f, a_idx, r_child, added_carried_mm2=0.0, cfg=cfg)

    expected = (0.3 ** 4 + r_child ** 4) ** 0.25
    assert expected == pytest.approx(0.515, abs=0.01)
    assert ancestor.r0 == pytest.approx(expected, rel=1e-6)
    assert ancestor.r1 == pytest.approx(expected, rel=1e-6)

    buggy = (max(0.3, 1.2) ** 4 + r_child ** 4) ** 0.25
    assert buggy == pytest.approx(1.209, abs=0.01)
    assert ancestor.r0 < buggy - 0.5  # nowhere near the buggy (wrong-endpoint) value


# --------------------------------------------------------- certificates --
def test_certificate_rule_crashes_on_unrevalidated_radius_increase(cfg):
    """design-v4.md #4: a resize without recheck must be IMPOSSIBLE to
    ship silently -- forcing one must crash, not warn."""
    f = tm.Forest()
    idx = f.add_member(tm.Member(p0=[0, 0, 0], p1=[0, 0, -10], r0=0.3, r1=0.3,
                                  kind="trunk", owner=("contact", 0), embed_end=None))
    far = cKDTree(np.array([[1e6, 1e6, 1e6]]))
    f.revalidate(far, cfg)
    assert f.dirty_members() == []
    f.members[idx].r0 = 5.0  # radius increase, no revalidate call
    with pytest.raises(AssertionError):
        tm.assert_all_certified(f)


def test_checkpoint_rollback_restores_state_after_a_rejected_candidate(cfg):
    """A candidate that fails clearance must leave the forest exactly as
    it was -- the transactional core cco._insert_terminal/_try relies on."""
    f = tm.Forest()
    host = tm.Member(p0=[0, 0, 0], p1=[0, 0, -20], r0=1.0, r1=1.0, kind="trunk",
                      owner=("contact", 0), embed_end=None, carried_mm2=10.0)
    host_idx = f.add_member(host)
    n_before = len(f.members)
    snapshot_r = host.r0
    cp = f.checkpoint()
    host.r0 = 99.0
    f.add_member(tm.Member(p0=[1, 1, 1], p1=[2, 2, 2], r0=0.1, r1=0.1,
                            kind="shoot", owner=("contact", 1), embed_end=None))
    f.rollback(cp)
    assert len(f.members) == n_before
    assert f.members[host_idx].r0 == snapshot_r


# ---------------------------------------------------------------- build --
def test_insertion_accounting_every_contact_routed_or_unrouted_exactly_once(cfg, table_mesh):
    from support_placement import perception, placement, routing
    islands = perception.overhang_islands(table_mesh, cfg)
    minima = perception.local_z_minima(table_mesh, cfg)
    contacts = placement.sample_candidates(table_mesh, islands, cfg)
    contacts, islands, _ = eng._ensure_minima_contacts(table_mesh, islands, contacts, minima, cfg)
    carried = placement.load_partition(table_mesh, islands, contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(table_mesh, pitch_mm=0.3)
    minima_flags = [bool(c.get("minimum")) for c in contacts]

    forest, unrouted = cco.build(table_mesh, contacts, tip_r, carried, minima_flags,
                                  plate_z, surface_tree, cfg)

    assert len(set(unrouted)) == len(unrouted)  # no duplicates
    unrouted_set = set(unrouted)
    for i in range(len(contacts)):
        stub_idx = forest.terminal_of[i]
        attached = forest.parent[stub_idx] is not None
        if i in unrouted_set:
            assert not attached, f"contact {i} is both unrouted and attached"
        else:
            assert attached, f"contact {i} is neither routed nor unrouted"
    assert forest.dirty_members() == []


def test_build_is_deterministic_across_repeated_runs(cfg, table_mesh):
    """No RNG anywhere (design-v4.md #4): re-running build on identical
    inputs must reproduce the identical forest (same unrouted set, same
    member count, same total volume)."""
    from support_placement import perception, placement, routing, tree as tm2
    islands = perception.overhang_islands(table_mesh, cfg)
    minima = perception.local_z_minima(table_mesh, cfg)
    contacts = placement.sample_candidates(table_mesh, islands, cfg)
    contacts, islands, _ = eng._ensure_minima_contacts(table_mesh, islands, contacts, minima, cfg)
    carried = placement.load_partition(table_mesh, islands, contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    plate_z = float(table_mesh.bounds[0][2]) - cfg.lift_mm
    surface_tree = routing.build_surface_tree(table_mesh, pitch_mm=0.3)
    minima_flags = [bool(c.get("minimum")) for c in contacts]

    f1, u1 = cco.build(table_mesh, contacts, tip_r, carried, minima_flags,
                        plate_z, surface_tree, cfg)
    f2, u2 = cco.build(table_mesh, contacts, tip_r, carried, minima_flags,
                        plate_z, surface_tree, cfg)
    assert u1 == u2
    assert len(f1.members) == len(f2.members)
    v1 = tm2.volume_report(f1.members, [])
    v2 = tm2.volume_report(f2.members, [])
    assert v1["total_ml"] == pytest.approx(v2["total_ml"], rel=1e-9)


# ------------------------------------------------ per-member sizing floor --
def _own_floor_mm(m, cfg):
    """design-v4.md #6: r = max(r_load, r_deflect, kind floor), recomputed
    fresh from a member's OWN current length/lean/carried load -- the
    verifier's audit approach, reused here as the regression test for the
    deleted test_engine_trunk_radius_meets_load_and_span_requirements."""
    L = float(np.linalg.norm(np.asarray(m.p1, float) - np.asarray(m.p0, float)))
    lean = cco._lean_deg(m.p0, m.p1)
    r_defl = placement._shaft_r_for_L(L, cfg, lean)
    r_tension = cco._r_tension_mm(m.carried_mm2, cfg)
    kind_floor = cfg.min_trunk_r_mm if m.kind == "trunk" else cfg.min_shoot_r_mm
    return max(r_defl, r_tension, kind_floor)


@pytest.mark.parametrize("fixture_name", ["table_mesh", "toed_slab_mesh"])
def test_engine_every_member_meets_its_own_load_and_deflection_floor(fixture_name, cfg, request):
    """B1 regression (verifier blocker), replacing the deleted
    test_engine_trunk_radius_meets_load_and_span_requirements: run the
    FULL engine on both charter fixtures (table + toed-slab) and, for
    EVERY live (routed) member in the final forest, recompute
    design-v4.md #6's floor fresh from that member's own final length,
    lean angle, and carried load, then assert its radius envelope meets
    it. The verifier's B1 repro was exactly this class of failure: a
    bifurcation child edge (`kind="shoot"`) sized ONLY to the tip radius
    at its junction end, skipping the load/deflection floor entirely (10
    undersized members measured on table, 12 on toed-slab; worst case
    L=11.36mm at 59deg lean, r=0.35 vs 0.60 required). After the fix, this
    must be zero violations on both fixtures.

    Stubs are exempt: the tip is a deliberate fuse sized separately by
    tip_radius_mm's own SF/clamp band (design-v4.md #6, "Tips unchanged"),
    not this structural floor -- verify.load_report already covers tip
    undersizing on its own (different) terms, and tree.printability_report
    exempts stubs from its per-segment rule for the same reason."""
    mesh = request.getfixturevalue(fixture_name)
    result = eng.run(mesh, cfg)
    forest = result["forest"]
    unrouted_stub_idx = {forest.terminal_of[i] for i in result["unrouted"]}
    violations = []
    for mi, m in enumerate(forest.members):
        if mi in unrouted_stub_idx or m.kind == "stub":
            continue
        floor = _own_floor_mm(m, cfg)
        r_have = max(m.r0, m.r1)
        if r_have < floor - 1e-6:
            violations.append(dict(
                member_idx=mi, kind=m.kind, r=round(r_have, 4), floor=round(floor, 4),
                L=round(float(np.linalg.norm(m.p1 - m.p0)), 3),
                lean_deg=round(cco._lean_deg(m.p0, m.p1), 2)))
    assert violations == [], violations
