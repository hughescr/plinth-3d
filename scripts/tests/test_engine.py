import numpy as np
from support_placement import engine


def test_engine_runs_end_to_end_on_table_fixture(table_mesh, cfg):
    # The table fixture is deliberately adversarial: overhang contacts on
    # the slab underside can land directly above a leg. That's an honest
    # limitation, not a bug: coverage and load must still be clean, and
    # self_verify must still SURFACE any remaining clearance issue rather
    # than silently reporting ok.
    result = engine.run(table_mesh, cfg)
    assert len(result["contacts"]) > 0
    # v4 contract: every contact either gets a member in the forest or is
    # honestly reported unrouted -- never a through-model member.
    n_terminal_members = sum(1 for m in result["members"] if m["kind"] == "stub")
    assert n_terminal_members + len(result["unrouted"]) == len(result["contacts"])
    # v3 baseline on this fixture (fixtures_v3_baseline.json) was 2
    # unrouted; v4 Phase 2 (straight segments only, no curved descent/
    # rewiring yet) is allowed up to 4 pending Phase 3 rewiring and Phase
    # 4 curvature attacking the residual -- also visible directly as
    # result["log"]["n_unrouted"] (verifier should-fix: surfaced at the
    # top level of the log so a regression shows up without digging into
    # the unrouted list or a per-pass dict).
    assert len(result["unrouted"]) <= 4
    assert result["log"]["n_unrouted"] == len(result["unrouted"])
    assert result["verify"]["load_violations"] == []
    # the no-intersection guarantee: zero clearance violations, always
    assert result["verify"]["clearance_violations"] == [], result["verify"]
    # nothing floats -- every emitted member endpoint attaches to
    # something (another member's axis, the plate, or the model).
    assert result["verify"]["connectivity_violations"] == [], result["verify"]
    assert result["log"]["t_total"] < 10.0


def test_engine_insertion_accounting_exact(table_mesh, cfg):
    """Every contact is routed (has a stub member in the forest) or
    honestly unrouted, exactly once -- no double-counting, nothing
    silently dropped."""
    result = engine.run(table_mesh, cfg)
    n = len(result["contacts"])
    routed_idx = {owner[1] for m in result["members"]
                  for owner in [m["owner"]] if m["kind"] == "stub"}
    unrouted_idx = set(result["unrouted"])
    assert routed_idx.isdisjoint(unrouted_idx)
    assert routed_idx | unrouted_idx == set(range(n))


def test_engine_per_segment_printability_green_on_table(table_mesh, cfg):
    """v4 new gate: per-segment tangent+taper printability (design-v4.md
    #3, #11), checked here directly against the returned forest (not just
    via verify['ok'], so a regression in this specific gate is caught
    even if something else masks 'ok')."""
    result = engine.run(table_mesh, cfg)
    assert result["verify"]["printability_violations"] == [], result["verify"]


def test_engine_murray_conservation_holds_on_table(table_mesh, cfg):
    result = engine.run(table_mesh, cfg)
    assert result["verify"]["murray_violations"] == [], result["verify"]


def test_engine_box_gets_exactly_the_bottom_plateau_minimum(box_mesh, cfg):
    """v3.6 rule, unchanged in v4: under lift a flat bottom is the
    print's FIRST FEP island, so the honest answer is one bottom-plateau
    minimum with one contact pinning it -- and nothing else."""
    result = engine.run(box_mesh, cfg)
    assert result["log"]["minima"] == 1
    assert len(result["contacts"]) == 1
    assert result["contacts"][0].get("minimum") is True
    assert result["verify"]["unsupported_minima"] == []
    assert result["verify"]["ok"], result["verify"]


def test_engine_supports_toe_minima_directly(toed_slab_mesh, cfg):
    """The layer-island rule end to end: both toe apexes (including the
    steep one the overhang-angle island model cannot see) end up with a
    routed contact AT the birth region, with the no-intersection
    guarantee intact."""
    result = engine.run(toed_slab_mesh, cfg)
    assert result["verify"]["clearance_violations"] == [], result["verify"]
    assert result["verify"]["connectivity_violations"] == [], result["verify"]
    assert result["verify"]["unsupported_minima"] == []
    routed = {m["owner"][1] for m in result["members"] if m["kind"] == "stub"}
    for apex in (np.array([8.0, 0.0, 9.0]), np.array([-8.0, 0.0, 11.0])):
        d = min(float(np.linalg.norm(
            np.asarray(result["contacts"][i]["position"]) - apex))
            for i in routed)
        assert d <= cfg.minima_contact_radius_mm, (apex, d)


def test_engine_wall_time_pin_on_table(table_mesh, cfg):
    result = engine.run(table_mesh, cfg)
    assert result["log"]["t_total"] < 10.0


def test_engine_no_contacts_returns_honest_empty_result(cfg):
    """Degenerate case: a mesh with essentially no down-facing overhang
    beyond near_plate_mm still returns a complete, well-formed result
    (the empty-contacts early-return path) instead of crashing."""
    import trimesh
    flat = trimesh.creation.box(extents=(10, 10, 0.5))
    m = trimesh.Trimesh(vertices=flat.vertices.copy(), faces=flat.faces.copy(),
                         process=True)
    result = engine.run(m, cfg)
    assert result["verify"]["clearance_violations"] == []
    assert result["verify"]["connectivity_violations"] == []
    assert set(result.keys()) >= {
        "islands", "contacts", "carried_area_mm2", "tip_r_mm", "forest",
        "members", "balls", "unrouted", "minima", "plate_z", "verify",
        "volume", "log"}
