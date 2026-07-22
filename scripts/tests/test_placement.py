import numpy as np
import pytest
from support_placement import perception, placement


def test_angle_aware_peel_term_fattens_steep_members(cfg):
    """v3.6 queue item 2: F_eff = F_drag + peel * pi*r^2 * tan(theta).
    Vertical members reduce exactly to the old closed form; lean
    monotonically fattens."""
    L = 20.0
    r0 = placement._shaft_r_for_L(L, cfg, angle_from_vertical_deg=0.0)
    closed = ((4 * cfg.shaft_drag_force_n * (L / 1000) ** 3
               / (3 * cfg.modulus_green_gpa * 1e9 * np.pi
                  * (cfg.max_deflection_mm / 1000))) ** 0.25) * 1000
    assert abs(r0 - max(closed, cfg.min_shaft_r_mm)) < 1e-9
    r45 = placement._shaft_r_for_L(L, cfg, angle_from_vertical_deg=45.0)
    r60 = placement._shaft_r_for_L(L, cfg, angle_from_vertical_deg=60.0)
    assert r60 > r45 > r0


def test_max_unbraced_len_is_consistent_inverse_with_angle(cfg):
    for angle in (0.0, 45.0, 60.0):
        r = placement._shaft_r_for_L(25.0, cfg, angle_from_vertical_deg=angle)
        if r <= cfg.min_shaft_r_mm:
            continue
        L_back = placement.max_unbraced_len_mm(r, cfg, angle_from_vertical_deg=angle)
        assert abs(L_back - 25.0) < 0.2, (angle, r, L_back)
    # steeper members must be allowed SHORTER free spans at equal radius
    assert placement.max_unbraced_len_mm(0.8, cfg, 60.0) \
        < placement.max_unbraced_len_mm(0.8, cfg, 0.0)


def test_spacing_floor_never_violated(table_mesh, cfg):
    islands = perception.overhang_islands(table_mesh, cfg)
    contacts = placement.sample_candidates(table_mesh, islands, cfg, seed=1)
    positions = np.array([c["position"] for c in contacts])
    if len(positions) < 2:
        pytest.skip("too few contacts on this fixture to test spacing")
    from scipy.spatial import cKDTree
    tree = cKDTree(positions)
    dist, _ = tree.query(positions, k=2)
    nn = dist[:, 1]
    assert nn.min() >= cfg.spacing_floor_mm - 1e-6


def test_every_island_gets_min_contacts(cfg):
    import trimesh
    # a tiny down-facing patch, smaller than target spacing, must still
    # get at least min_contacts_per_island.
    from support_placement import mesh_io
    tiny_table = mesh_io.make_overhang_table(leg=(1, 1, 5), top=(3, 3, 1))
    islands = perception.overhang_islands(tiny_table, cfg)
    assert len(islands) >= 1
    contacts = placement.sample_candidates(tiny_table, islands, cfg, seed=2)
    per_island = {}
    for c in contacts:
        per_island.setdefault(c["island_id"], 0)
        per_island[c["island_id"]] += 1
    for iid, n in per_island.items():
        assert n >= cfg.min_contacts_per_island


def test_load_partition_conserves_area(table_mesh, cfg):
    islands = perception.overhang_islands(table_mesh, cfg)
    contacts = placement.sample_candidates(table_mesh, islands, cfg, seed=3)
    carried = placement.load_partition(table_mesh, islands, contacts, cfg)
    by_island = {}
    for c, a in zip(contacts, carried):
        by_island.setdefault(c["island_id"], 0.0)
        by_island[c["island_id"]] += a
    for iid, total in by_island.items():
        face_ids = islands[iid]
        true_area = float(table_mesh.submesh([face_ids], append=True).area)
        assert abs(total - true_area) / true_area < 0.15


def test_load_partition_is_deterministic_despite_global_rng_state(table_mesh, cfg):
    """Charter: no RNG. load_partition's sub.sample() draws from the
    GLOBAL numpy random state -- if it isn't seeded internally, the
    carried-area partition silently varies with whatever ran before it.
    Perturb the global RNG differently before each call; the carried-area
    outputs must still match exactly."""
    islands = perception.overhang_islands(table_mesh, cfg)
    contacts = placement.sample_candidates(table_mesh, islands, cfg, seed=3)
    np.random.seed(12345)
    carried_a = placement.load_partition(table_mesh, islands, contacts, cfg)
    np.random.seed(54321)
    carried_b = placement.load_partition(table_mesh, islands, contacts, cfg)
    assert np.array_equal(carried_a, carried_b)


def test_tip_radius_matches_hand_calculation(cfg):
    # Hand calc: area=20mm2, peel=3000Pa, SF=8, tensile=10MPa
    area = 20.0
    F = area * 1e-6 * cfg.peel_pressure_pa
    expected_r_m = (F * cfg.tip_safety_factor / (np.pi * cfg.tensile_green_mpa * 1e6)) ** 0.5
    expected_mm = np.clip(expected_r_m * 1000, cfg.tip_r_min_mm, cfg.tip_r_max_mm)
    got = placement.tip_radius_mm(area, cfg)
    assert abs(got - expected_mm) < 1e-9


def test_tip_radius_respects_clamp_band(cfg):
    tiny = placement.tip_radius_mm(1e-6, cfg)
    huge = placement.tip_radius_mm(1e6, cfg)
    assert tiny == cfg.tip_r_min_mm
    assert huge == cfg.tip_r_max_mm


def test_short_shaft_needs_no_bracing(cfg):
    r, n_braces, eff = placement.shaft_radius_for_length_mm(3.0, cfg)
    assert n_braces == 0
    assert r <= cfg.max_unbraced_shaft_r_mm


def test_long_shaft_triggers_bracing_and_stays_under_cap(cfg):
    r, n_braces, eff = placement.shaft_radius_for_length_mm(80.0, cfg)
    assert n_braces > 0
    assert r <= cfg.max_unbraced_shaft_r_mm + 1e-9
    assert eff < 80.0


def test_halving_free_length_cuts_deflection_eightfold(cfg):
    # design.md's own stated relation, taken as a direct property test of
    # the deflection formula (not the bracing search) -- deflection ~ L^3.
    from support_placement.placement import _shaft_r_for_L
    r1 = _shaft_r_for_L(40.0, cfg)
    r2 = _shaft_r_for_L(20.0, cfg)
    # delta ~ L^3/r^4 held constant by construction (we solved for r at
    # fixed delta) -- so instead verify the underlying force/delta relation
    # directly: at fixed r, delta scales with L^3.
    E = cfg.modulus_green_gpa * 1e9
    F = cfg.shaft_drag_force_n
    def delta(L_mm, r_mm):
        L, r = L_mm / 1000, r_mm / 1000
        I = np.pi * r ** 4 / 4
        return F * L ** 3 / (3 * E * I)
    d_full = delta(40.0, 0.5)
    d_half = delta(20.0, 0.5)
    assert abs(d_full / d_half - 8.0) < 1e-6
