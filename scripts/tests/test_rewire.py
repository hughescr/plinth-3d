"""Phase 3 tests (design-v4.md #7, #13): rewire.optimize's topology
mutation -- monotonicity, exact terminal accounting, §6 floors,
determinism, a hand-built twin-parallel-plate-root merge, full-engine
gate cleanliness, and the frozen-baseline A/B volume pin.
"""
import pytest

from support_placement import cco, engine, placement, routing, rewire, tree as tm, verify
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
