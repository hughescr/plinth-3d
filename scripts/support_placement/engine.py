"""v4 slim orchestrator (design-v4.md #2, #14 phase 2): perception ->
sampling -> load partition -> tip sizing -> delicacy field -> cco.build
-> verify.self_verify, with a bounded (<=2 pass) fix loop for islands
that came up short on coverage. NO stability loop, NO route/trunk
machinery -- those retire with trunks.py/braces.py's policy role
(design-v4.md #2); the CCO builder's own transactional certificate rule
(tree.py) replaces v3's repeated post-hoc gate/repair passes.

See references/design-v4.md for the charter and __init__.py for the
module map.
"""
import time
import numpy as np

from . import perception, placement, routing, verify, joints, cco, tree


def run(mesh, cfg, log=None):
    """Run the full v4 pipeline on an already-loaded trimesh.Trimesh.
    Returns a result dict: islands, contacts, carried_area_mm2, tip_r_mm,
    forest, members, balls, unrouted, minima, plate_z, verify, volume,
    log."""
    log = log if log is not None else {}
    t0 = time.time()

    islands = perception.overhang_islands(mesh, cfg)
    log["islands"] = len(islands)
    log["t_islands"] = round(time.time() - t0, 2)

    # v3.6 (Craig, top priority, carried forward unchanged): island-birth
    # minima. Detected before sampling so every minimum the overhang-angle
    # model missed (steep flanks) still ends up with a contact AT its
    # birth region.
    minima = perception.local_z_minima(mesh, cfg)
    log["minima"] = len(minima)
    log["t_minima"] = round(time.time() - t0, 2)

    contacts = placement.sample_candidates(mesh, islands, cfg)
    log["contacts_initial"] = len(contacts)
    contacts, islands, n_min_added = _ensure_minima_contacts(
        mesh, islands, contacts, minima, cfg)
    log["minima_contacts_added"] = n_min_added

    plate_z = float(mesh.bounds[0][2]) - cfg.lift_mm

    if not contacts:
        log["t_total"] = round(time.time() - t0, 2)
        empty_forest = tree.Forest()
        return dict(
            islands=islands, contacts=[], carried_area_mm2=np.array([]),
            tip_r_mm=np.array([]), forest=empty_forest, members=[], balls=[],
            unrouted=[], minima=minima, plate_z=plate_z,
            verify=dict(ok=True, uncovered_islands=[], clearance_violations=[],
                        load_violations=[], unsupported_minima=[],
                        connectivity_violations=[], printability_violations=[],
                        murray_violations=[]),
            volume=dict(member_ml=0.0, ball_ml=0.0, total_ml=0.0, by_kind_ml={}),
            log=log)

    # Built once, reused for every clearance query this run makes (see
    # routing.build_surface_tree's docstring). Tighter sample than the
    # default: every no-intersection guarantee in this engine rides on
    # this tree, so keep its approximation error small against
    # standoff_mm.
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)
    log["t_surface_tree"] = round(time.time() - t0, 2)

    sample_points, _sample_normals, sample_delicacy, _query = perception.delicacy_field(mesh, cfg)
    log["t_delicacy"] = round(time.time() - t0, 2)

    forest = members = balls = report = carried = tip_r = unrouted = None
    max_fix_passes = 2
    for fix_pass in range(max_fix_passes):
        carried = placement.load_partition(mesh, islands, contacts, cfg)
        tip_r = placement.tip_radius_mm(carried, cfg)
        minima_flags = [bool(c.get("minimum")) for c in contacts]

        forest, unrouted = cco.build(mesh, contacts, tip_r, carried, minima_flags,
                                      plate_z, surface_tree, cfg,
                                      sample_points=sample_points,
                                      sample_delicacy=sample_delicacy)
        # Certificate rule (design-v4.md #4): nothing downstream may trust
        # a forest with a void certificate. Stands in for "emission
        # asserts zero dirty members" until Phase 7 wires a real emitter.
        tree.assert_all_certified(forest)
        # An honestly-unrouted terminal's stub member is still sitting in
        # the forest (every terminal gets one up front, before insertion
        # is attempted) but was never attached to anything -- it must not
        # leak into the connectivity/volume/printability accounting as a
        # floating member; `unrouted` already reports it honestly.
        unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted}
        live_members = [m for mi, m in enumerate(forest.members)
                        if mi not in unrouted_stub_idx]
        members = joints.collect_members(live_members)
        balls = joints.compute_joint_balls(live_members, plate_z, cfg)
        log[f"forest_pass{fix_pass}"] = dict(
            n_members=len(members), n_balls=len(balls), n_unrouted=len(unrouted))
        # Verifier should-fix: n_unrouted was only ever visible nested
        # inside forest_pass{N} (or by len()-ing the unrouted list
        # yourself) -- surface it as its own top-level log key so a
        # regression vs the frozen v3 baselines (fixtures_v3_baseline.json:
        # table=2, toed_slab=1) shows up directly in test/log output
        # without digging into a per-pass dict or a result list.
        log["n_unrouted"] = len(unrouted)

        routed_contacts = [c for i, c in enumerate(contacts) if i not in unrouted]
        report = verify.self_verify(mesh, islands, routed_contacts, members, carried, tip_r,
                                     cfg, plate_z, surface_tree=surface_tree, minima=minima,
                                     forest=forest)
        log[f"verify_pass{fix_pass}"] = dict(
            ok=report["ok"],
            n_uncovered=len(report["uncovered_islands"]),
            n_clearance_violations=len(report["clearance_violations"]),
            n_load_violations=len(report["load_violations"]),
            n_unsupported_minima=len(report["unsupported_minima"]),
            n_connectivity_violations=len(report["connectivity_violations"]),
            n_printability_violations=len(report["printability_violations"]),
            n_murray_violations=len(report["murray_violations"]),
        )
        if report["ok"] or not report["uncovered_islands"]:
            break
        # Only add fix contacts on islands that are uncovered because
        # they lacked contacts -- islands whose contacts exist but proved
        # UNROUTABLE would just accumulate more unroutable contacts.
        unroutable_islands = {contacts[i]["island_id"] for i in unrouted}
        fixable = [u for u in report["uncovered_islands"]
                   if u["island_id"] not in unroutable_islands]
        if not fixable:
            break
        contacts = _add_fix_contacts(mesh, islands, contacts, fixable)

    volume = tree.volume_report(members, balls)
    log["t_total"] = round(time.time() - t0, 2)
    return dict(
        islands=islands, contacts=contacts, carried_area_mm2=carried, tip_r_mm=tip_r,
        forest=forest, members=members, balls=balls, unrouted=unrouted, minima=minima,
        plate_z=plate_z, verify=report, volume=volume, log=log,
    )


def _ensure_minima_contacts(mesh, islands, contacts, minima, cfg):
    """Ported verbatim from v3 engine.py (design-v4.md #4 orchestration
    step list): every island-birth minimum (perception.local_z_minima)
    gets a contact touching its birth region unless one already does. A
    contact merely NEAR the minimum does not count -- the satisfaction
    test is distance to the birth-region vertices, which only spans the
    first couple of layers of the island (Craig: what matters is how many
    layers pass before the island connects, not lateral proximity).

    Minima outside every overhang island (flanks steeper than the
    down-facing test -- exactly the class the island model misses) get a
    small synthetic island appended so load partition and coverage treat
    them like any other region. Mutates neither input list in place;
    returns (contacts, islands, n_added)."""
    from scipy.spatial import cKDTree
    if not minima:
        return contacts, islands, 0
    contacts = list(contacts)
    islands = list(islands)
    have = np.array([np.asarray(c["position"], float) for c in contacts]) \
        if contacts else np.zeros((0, 3))
    tree_have = cKDTree(have) if len(have) else None
    face_island = -np.ones(len(mesh.faces), dtype=np.int64)
    for iid, fids in enumerate(islands):
        face_island[fids] = iid
    centers_tree = cKDTree(mesh.triangles_center)
    added = 0
    for m in minima:
        if tree_have is not None:
            d, _ = tree_have.query(m["region_pts"],
                                    workers=routing.query_workers(len(m["region_pts"])))
            if float(np.min(d)) <= cfg.minima_contact_radius_mm:
                continue  # an existing contact already holds this birth island
        near = centers_tree.query_ball_point(m["position"], r=1.5)
        if not near:
            _, nearest = centers_tree.query(m["position"])
            near = [int(nearest)]
        best = min(near, key=lambda f: float(
            np.linalg.norm(mesh.triangles_center[f] - m["position"])))
        if face_island[best] >= 0:
            iid = int(face_island[best])
        else:
            fids = np.array(sorted(near), dtype=np.int64)
            islands.append(fids)
            iid = len(islands) - 1
        area = float(mesh.area_faces[islands[iid]].sum())
        contacts.append(dict(position=np.asarray(m["position"], float),
                             normal=np.asarray(m["normal"], float),
                             island_id=iid, area_mm2=area, minimum=True))
        added += 1
    return contacts, islands, added


def _add_fix_contacts(mesh, islands, contacts, uncovered):
    """Ported verbatim from v3 engine.py: one extra contact at the
    worst-covered point of each still-uncovered island."""
    centers = mesh.triangles_center
    normals = mesh.face_normals
    by_island = {}
    for c in contacts:
        by_island.setdefault(c["island_id"], []).append(np.asarray(c["position"]))

    extra = list(contacts)
    for u in uncovered:
        iid = u["island_id"]
        face_ids = islands[iid]
        pts = centers[face_ids]
        existing = np.array(by_island.get(iid, []))
        if len(existing):
            from scipy.spatial import cKDTree
            tree_existing = cKDTree(existing)
            dist, _ = tree_existing.query(pts, workers=routing.query_workers(len(pts)))
        else:
            dist = np.full(len(pts), np.inf)
        worst = int(np.argmax(dist))
        extra.append(dict(position=pts[worst], normal=normals[face_ids[worst]],
                           island_id=iid, area_mm2=u["n_faces"]))
    return extra
