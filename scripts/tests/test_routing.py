import numpy as np
from support_placement import routing


def test_route_geometry_stub_follows_normal_then_descends(cfg):
    contact = dict(position=np.array([0.0, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0]))
    anchor = np.array([0.0, 0.0, -5.0])
    route = routing.route_to_anchor(contact, anchor, r_tip=0.3, r_shaft=0.6, cfg=cfg)
    pts = route.points
    # first point is embedded slightly below contact along -normal
    assert pts[0][2] < 10.0
    # stub end is above contact along +normal by stub_length_mm
    stub_end = pts[1]
    assert abs(stub_end[2] - (10.0 + cfg.stub_length_mm)) < 1e-6
    # path ends at the anchor
    assert np.allclose(pts[-1], anchor)


def test_route_free_length_is_positive_and_reasonable(cfg):
    contact = dict(position=np.array([0.0, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0]))
    anchor = np.array([0.0, 0.0, -5.0])
    route = routing.route_to_anchor(contact, anchor, r_tip=0.3, r_shaft=0.6, cfg=cfg)
    assert route.free_length_mm > 15.0 * 0.9  # stub + ~15mm descent


def test_clearance_flags_a_route_straight_through_the_model(box_mesh, cfg):
    # box spans x,y in [-5,5], z in [0,10]. Routing straight down through
    # its center must violate clearance.
    contact = dict(position=np.array([0.0, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0]))
    anchor = np.array([0.0, 0.0, -5.0])
    route = routing.route_to_anchor(contact, anchor, r_tip=0.3, r_shaft=0.6, cfg=cfg)
    violations = routing.check_clearance(box_mesh, route, cfg)
    assert len(violations) > 0


def test_clearance_clears_a_route_that_avoids_the_model(box_mesh, cfg):
    # x=8 is well outside the box's x in [-5,5] footprint.
    contact = dict(position=np.array([8.0, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0]))
    anchor = np.array([8.0, 0.0, -5.0])
    route = routing.route_to_anchor(contact, anchor, r_tip=0.3, r_shaft=0.6, cfg=cfg)
    violations = routing.check_clearance(box_mesh, route, cfg)
    assert violations == []


def _mixed_routes(cfg):
    """A mix of clearly-violating, clearly-clearing, and tip-embedded
    routes against the box fixture (box spans x,y in [-5,5], z in [0,10])."""
    mk = lambda px, ax: routing.route_to_anchor(
        dict(position=np.array([px, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0])),
        np.array([ax, 0.0, -5.0]), r_tip=0.3, r_shaft=0.6, cfg=cfg)
    return [mk(0.0, 0.0),    # straight through the box: violates
            mk(8.0, 8.0),    # well clear of the box
            mk(0.0, 20.0),   # embedded tip, foot jogs far away
            mk(-8.0, -8.0)]  # clear on the other side


def test_batch_clearance_matches_per_route_results(box_mesh, cfg):
    """check_clearance_batch is the vectorized fast path the engine and
    verifier use on real models -- it must agree exactly, violation for
    violation, with the single-route reference implementation."""
    routes = _mixed_routes(cfg)
    singles = [routing.check_clearance(box_mesh, r, cfg) for r in routes]
    batched = routing.check_clearance_batch(box_mesh, routes, cfg)
    assert batched == singles
    # and the fixture actually exercises both outcomes
    assert any(v for v in singles) and any(not v for v in singles)


def test_batch_clearance_matches_with_surface_tree(box_mesh, cfg):
    """Same agreement through the approximate KDTree path (with its
    distance_upper_bound pruning), which is what real runs actually use."""
    tree = routing.build_surface_tree(box_mesh)
    routes = _mixed_routes(cfg)
    singles = [routing.check_clearance(box_mesh, r, cfg, surface_tree=tree)
               for r in routes]
    batched = routing.check_clearance_batch(box_mesh, routes, cfg, surface_tree=tree)
    assert batched == singles
    assert any(v for v in singles) and any(not v for v in singles)


def test_upper_bound_pruning_does_not_change_violations(box_mesh, cfg):
    """distance_upper_bound is a pure traversal optimization: any point at
    least radius+standoff from the surface has clearance >= 0 whatever its
    true distance is, so pruned (inf) distances must yield the same
    violations as unpruned ones."""
    tree = routing.build_surface_tree(box_mesh)
    for route in _mixed_routes(cfg):
        pts, radii, seg_ids = routing.sample_route_envelope(route, cfg)
        if len(pts) == 0:
            continue
        d_full = routing._surface_distances(box_mesh, pts, tree)
        d_pruned = routing._surface_distances(
            box_mesh, pts, tree, upper_bound=float(radii.max()) + cfg.standoff_mm)
        v_full = routing._violations_from(route, pts, radii, seg_ids, d_full, cfg)
        v_pruned = routing._violations_from(route, pts, radii, seg_ids, d_pruned, cfg)
        assert v_pruned == v_full


def test_build_surface_tree_is_deterministic_despite_global_rng_state(box_mesh):
    """Charter: no RNG. build_surface_tree's mesh.sample() draws from the
    GLOBAL numpy random state -- if it isn't seeded internally, the tree
    (and every clearance query riding on it) silently varies with whatever
    ran before it. Perturb the global RNG differently before each call;
    the resulting point clouds must still match exactly."""
    np.random.seed(12345)
    tree_a = routing.build_surface_tree(box_mesh)
    np.random.seed(54321)
    tree_b = routing.build_surface_tree(box_mesh)
    assert tree_a.data.shape == tree_b.data.shape
    assert np.array_equal(tree_a.data, tree_b.data)


def test_embedded_tip_contact_itself_is_not_a_false_violation(box_mesh, cfg):
    # a route whose tip touches the box surface (as every real support
    # does) must not itself register as a clearance violation.
    contact = dict(position=np.array([0.0, 0.0, 10.0]), normal=np.array([0.0, 0.0, 1.0]))
    anchor = np.array([20.0, 0.0, -5.0])  # far away, path clears once past the stub
    route = routing.route_to_anchor(contact, anchor, r_tip=0.3, r_shaft=0.6, cfg=cfg)
    violations = routing.check_clearance(box_mesh, route, cfg)
    assert all(v["segment_kind"] != "stub" for v in violations)
