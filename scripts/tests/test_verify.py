import numpy as np
from support_placement import perception, placement, routing, verify, mesh_io


def test_coverage_report_flags_island_with_no_contacts(table_mesh, cfg):
    islands = perception.overhang_islands(table_mesh, cfg)
    report = verify.coverage_report(table_mesh, islands, [], cfg)
    assert len(report) == len(islands)
    assert all(not r["covered"] for r in report)


def test_coverage_report_passes_with_real_sampling(table_mesh, cfg):
    islands = perception.overhang_islands(table_mesh, cfg)
    contacts = placement.sample_candidates(table_mesh, islands, cfg, seed=5)
    report = verify.coverage_report(table_mesh, islands, contacts, cfg)
    assert all(r["covered"] for r in report), report


def test_clearance_report_aggregates_route_violations(box_mesh, cfg):
    contact = dict(position=np.array([0.0, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0]))
    bad_route = routing.route_to_anchor(contact, np.array([0.0, 0.0, -5.0]),
                                         r_tip=0.3, r_shaft=0.6, cfg=cfg)
    good_contact = dict(position=np.array([8.0, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0]))
    good_route = routing.route_to_anchor(good_contact, np.array([8.0, 0.0, -5.0]),
                                          r_tip=0.3, r_shaft=0.6, cfg=cfg)
    report = verify.clearance_report(box_mesh, [good_route, bad_route], cfg)
    assert all(v["route_idx"] == 1 for v in report)
    assert len(report) > 0


def test_load_report_flags_clamped_overloaded_tip(cfg):
    # an absurdly large carried area forces the tip to clamp at r_max,
    # which can't actually carry that load at the target safety factor.
    carried = np.array([5000.0])  # mm^2
    r = placement.tip_radius_mm(carried, cfg)
    report = verify.load_report(carried, r, cfg)
    assert len(report) == 1
    assert report[0]["realized_sf"] < cfg.tip_safety_factor


def test_load_report_clean_for_normal_load(cfg):
    carried = np.array([15.0])  # typical corpus-scale carried area
    r = placement.tip_radius_mm(carried, cfg)
    report = verify.load_report(carried, r, cfg)
    assert report == []


def test_self_verify_ok_on_a_clean_small_case(table_mesh, cfg):
    islands = perception.overhang_islands(table_mesh, cfg)
    contacts = placement.sample_candidates(table_mesh, islands, cfg, seed=7)
    carried = placement.load_partition(table_mesh, islands, contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    zmin = table_mesh.bounds[0][2]
    routes = []
    for c, r in zip(contacts, tip_r):
        anchor = routing.plate_anchor(c, zmin, cfg)
        shaft_r, n_braces, eff = placement.shaft_radius_for_length_mm(
            abs(c["position"][2] - anchor[2]), cfg)
        route = routing.route_to_anchor(c, anchor, r_tip=r, r_shaft=shaft_r, cfg=cfg)
        route.n_braces = n_braces
        routes.append(route)
    report = verify.self_verify(table_mesh, islands, contacts, routes, carried, tip_r, cfg)
    assert report["uncovered_islands"] == []
    assert report["load_violations"] == []


def test_orientation_scar_cost_prefers_hiding_the_delicate_fin(fin_mesh, cfg):
    delicacy = perception.delicacy_field(fin_mesh, cfg, n_samples=6000)
    identity = np.eye(3)
    flip_x = np.diag([1.0, -1.0, -1.0])  # flips which side faces "down"
    scores = verify.orientation_scar_cost(fin_mesh, cfg, [identity, flip_x], delicacy=delicacy)
    assert scores[0] != scores[1]
    assert min(scores) >= 0
