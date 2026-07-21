import numpy as np
from scipy.spatial import cKDTree
from support_placement import perception, placement, routing, verify, mesh_io, joints


def _m(p0, p1, r0, r1, owner, kind, embed_end=None):
    """v4 member dict (design-v4.md #2 schema) -- migrated mechanically
    from the v3 Trunk/brace-dict fixtures (same geometry), assertions
    unchanged."""
    return dict(p0=np.asarray(p0, float), p1=np.asarray(p1, float),
                r0=r0, r1=r1, owner=owner, kind=kind, embed_end=embed_end)


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
    surface_tree = routing.build_surface_tree(box_mesh)
    bad = _m([0.0, 0.0, 10.0], [0.0, 0.0, -5.0], 0.3, 0.6, ("contact", 0), "trunk")
    good = _m([8.0, 0.0, 10.0], [8.0, 0.0, -5.0], 0.3, 0.6, ("contact", 1), "trunk")
    report = verify.clearance_report(box_mesh, [good, bad], cfg, surface_tree=surface_tree)
    assert all(v["member_idx"] == 1 for v in report)
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
    plate_z = zmin - cfg.lift_mm
    members = []
    for ci, (c, r) in enumerate(zip(contacts, tip_r)):
        pos = np.asarray(c["position"], float)
        anchor = np.array([pos[0], pos[1], plate_z])
        shaft_r, _n_braces, _eff = placement.shaft_radius_for_length_mm(
            abs(pos[2] - anchor[2]), cfg)
        members.append(_m(pos, anchor, float(r), shaft_r, ("contact", ci), "trunk"))
    report = verify.self_verify(table_mesh, islands, contacts, members, carried, tip_r,
                                 cfg, plate_z)
    assert report["uncovered_islands"] == []
    assert report["load_violations"] == []


def test_minima_report_requires_contact_at_birth_region_not_nearby(cfg):
    """Craig's rule: lateral proximity is NOT support. A contact 1.5mm
    away laterally (10+ layers above the island's birth on a shallow toe)
    must NOT satisfy the minimum; a contact touching the birth region
    must."""
    minima = [dict(z=9.0, position=[8.0, 0.0, 9.0],
                   region_pts=np.array([[8.0, 0.0, 9.0], [8.1, 0.0, 9.05]]))]
    far = [dict(position=np.array([8.0, 1.5, 10.2]))]
    at_min = [dict(position=np.array([8.05, 0.0, 9.02]))]
    assert len(verify.minima_report(minima, far, cfg)) == 1
    assert verify.minima_report(minima, at_min, cfg) == []
    no_contacts = verify.minima_report(minima, [], cfg)
    assert len(no_contacts) == 1 and no_contacts[0]["min_gap_mm"] == float("inf")
    assert verify.minima_report([], at_min, cfg) == []


def test_connectivity_report_empty_inputs_returns_empty(cfg):
    assert verify.connectivity_report([], plate_z=0.0, cfg=cfg) == []


def test_connectivity_report_flags_floating_brace_endpoint(cfg):
    """v3.7 hard gate: a brace endpoint sitting in mid-air (nothing else's
    axis nearby, not at plate level, not embedded in any model) must be
    flagged -- this is the exact failure mode the stale-brace-leak defect
    produced (rods left pointing at trunks that had since moved/vanished)."""
    plate_z = -5.0
    a = _m([0.0, 0.0, plate_z], [0.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 0), "trunk")
    b = _m([10.0, 0.0, plate_z], [10.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 1), "trunk")
    good_brace = _m([0.0, 0.0, 4.0], [10.0, 0.0, 4.0], 0.5, 0.5, ("brace", 0), "brace")
    floating_brace = _m([0.0, 0.0, 6.0],      # still on trunk a's axis
                         [50.0, 50.0, 50.0],  # nowhere near anything
                         0.5, 0.5, ("brace", 1), "brace")
    violations = verify.connectivity_report([a, b, good_brace, floating_brace], plate_z, cfg)
    flagged = [tuple(v["endpoint"]) for v in violations]
    assert (50.0, 50.0, 50.0) in flagged
    assert (0.0, 0.0, 6.0) not in flagged  # on-axis end of the SAME rod is fine


def test_connectivity_report_brace_attached_to_trunk_axis_passes(cfg):
    """A rod whose endpoints sit on trunk column axes is attached (rule
    a) -- the ordinary trunk-to-trunk brace case."""
    plate_z = -5.0
    a = _m([0.0, 0.0, plate_z], [0.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 0), "trunk")
    b = _m([10.0, 0.0, plate_z], [10.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 1), "trunk")
    brace = _m([0.0, 0.0, 4.0], [10.0, 0.0, 4.0], 0.5, 0.5, ("brace", 0), "brace")
    violations = verify.connectivity_report([a, b, brace], plate_z, cfg)
    flagged = [tuple(v["endpoint"]) for v in violations]
    assert (0.0, 0.0, 4.0) not in flagged
    assert (10.0, 0.0, 4.0) not in flagged


def test_connectivity_report_model_rung_passes_with_embedded_end_and_surface_tree(cfg):
    """A trunk-to-model rung's model-facing end (embed_end="p1") is
    attached iff it is actually embedded in the model surface -- rule
    (c), and ONLY for the specific endpoint designed to embed."""
    plate_z = -5.0
    a = _m([0.0, 0.0, plate_z], [0.0, 0.0, 10.0], 1.0, 1.0, ("trunk", 0), "trunk")
    model_pt = np.array([5.0, 0.0, 4.0])
    rung = _m([0.0, 0.0, 4.0],   # on trunk a's axis
              model_pt,           # model end
              cfg.brace_r_mm, cfg.brace_r_mm, ("brace", 0), "model_rung", embed_end="p1")
    surface_tree = cKDTree(np.array([model_pt]))
    violations = verify.connectivity_report([a, rung], plate_z, cfg,
                                             surface_tree=surface_tree)
    flagged = [tuple(v["endpoint"]) for v in violations]
    assert tuple(model_pt.tolist()) not in flagged
    assert (0.0, 0.0, 4.0) not in flagged
    # without a surface tree the embed rule can't fire -- the model end is
    # honestly reported as unattached rather than silently passed
    violations_no_tree = verify.connectivity_report([a, rung], plate_z, cfg)
    flagged_no_tree = [tuple(v["endpoint"]) for v in violations_no_tree]
    assert tuple(model_pt.tolist()) in flagged_no_tree


def test_orientation_scar_cost_prefers_hiding_the_delicate_fin(fin_mesh, cfg):
    delicacy = perception.delicacy_field(fin_mesh, cfg, n_samples=6000)
    identity = np.eye(3)
    flip_x = np.diag([1.0, -1.0, -1.0])  # flips which side faces "down"
    scores = verify.orientation_scar_cost(fin_mesh, cfg, [identity, flip_x], delicacy=delicacy)
    assert scores[0] != scores[1]
    assert min(scores) >= 0
