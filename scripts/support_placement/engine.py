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

from . import perception, placement, routing, verify, joints, cco, tree, rewire
from . import transient, bracing


def prepare(mesh, cfg, log=None):
    """Stage 1 of the pipeline: everything that depends only on the mesh.
    Returns a picklable dict (no KDTree -- rebuild surface_tree per
    process with routing.build_surface_tree): islands, minima, contacts,
    plate_z, sample_points, sample_delicacy, log. Extracted from run()
    so Treant-scale runs can checkpoint between bounded foreground
    calls; run() remains the canonical composition."""
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
    sample_points, _sn, sample_delicacy, _q = perception.delicacy_field(mesh, cfg)
    log["t_delicacy"] = round(time.time() - t0, 2)
    return dict(islands=islands, minima=minima, contacts=contacts,
                plate_z=plate_z, sample_points=sample_points,
                sample_delicacy=sample_delicacy, log=log)


def construct(mesh, pre, surface_tree, cfg, log=None, limit_contacts=None):
    """Stage 2: load partition + tip sizing + cco.build (NO rewiring --
    the caller owns rewire.optimize so its wall budget is its own).
    `limit_contacts` truncates the contact list for profiling only.
    Returns (forest, unrouted, carried, tip_r, contacts)."""
    log = log if log is not None else {}
    contacts = pre["contacts"][:limit_contacts] if limit_contacts else pre["contacts"]
    carried = placement.load_partition(mesh, pre["islands"], contacts, cfg)
    tip_r = placement.tip_radius_mm(carried, cfg)
    minima_flags = [bool(c.get("minimum")) for c in contacts]
    forest, unrouted = cco.build(mesh, contacts, tip_r, carried, minima_flags,
                                  pre["plate_z"], surface_tree, cfg,
                                  sample_points=pre["sample_points"],
                                  sample_delicacy=pre["sample_delicacy"])
    tree.assert_all_certified(forest)
    return forest, unrouted, carried, tip_r, contacts


def live_member_lists(forest, unrouted, plate_z, cfg):
    """Stage 3a: the verify/emission-facing member + ball lists. An
    honestly-unrouted terminal's never-attached stub, and rewire-retired
    (Forest.dead) members, are excluded -- `unrouted` reports the former
    honestly and the latter were folded into survivors."""
    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted}
    live = [m for mi, m in enumerate(forest.members)
            if mi not in unrouted_stub_idx and mi not in forest.dead]
    members = joints.collect_members(live)
    balls = joints.compute_joint_balls(live, plate_z, cfg)
    return members, balls


def _live_member_ml(forest, unrouted):
    """member_ml over exactly the same live set live_member_lists uses
    (honestly-unrouted stubs and rewire-dead members excluded), without
    paying for compute_joint_balls -- brace_and_size only needs the member
    total for its before/after log, not the ball list."""
    unrouted_stub_idx = {forest.terminal_of[i] for i in unrouted}
    live = [m for mi, m in enumerate(forest.members)
            if mi not in unrouted_stub_idx and mi not in forest.dead]
    return tree.volume_report(live, [])["member_ml"]


def brace_and_size(mesh, forest, unrouted, surface_tree, plate_z, cfg, log=None):
    """design-v4.md #8/#9 fixed point (SPEC-INTEGRATION #3): bounded
    propose-braces / resize-down-and-up / batch-revalidate loop (<=3
    iterations), then a SECOND rewire.optimize pass against the now-braced
    reality (design-v4.md #7: rewire after ANY structural pass -- braces
    just changed which spans are restrained), then a final re-fix so the
    forest handed back to the caller is clearance-clean at its final
    (braced) radii. Mutates `forest` in place; returns the log dict this
    call wrote to log['brace_and_size'].

    A member that GREW past clearance during the loop is never silently
    shrunk back -- its need (a real transient span) and its violation (a
    real clearance conflict) are both honest; the fix is more restraint
    next iteration (another brace), not corruption-by-omission.
    """
    log = log if log is not None else {}
    member_ml_before = _live_member_ml(forest, unrouted)

    iterations = 0
    rungs_placed = 0
    rungs_unplaceable = 0
    n_resized_down = 0
    n_resized_up = 0
    for it in range(3):
        iterations = it + 1
        report = transient.transient_report(forest, plate_z, cfg)
        placed = bracing.propose_and_place(forest, surface_tree, plate_z, cfg, report, log=log)
        resize = transient.resize_forest(forest, plate_z, cfg)
        violations = forest.revalidate(surface_tree, cfg)
        rungs_placed += placed["rungs_placed"]
        rungs_unplaceable += placed["rungs_unplaceable"]
        for _idx, r_old, r_new in resize["resized"]:
            if r_new < r_old - 1e-9:
                n_resized_down += 1
            elif r_new > r_old + 1e-9:
                n_resized_up += 1
        if not placed["placed"] and not resize["resized"] and not violations:
            break

    # Design-v4.md #7: rewire again after ANY structural pass -- the
    # braces/resizes above just changed which spans are restrained and how
    # fat every member is, exactly the kind of neighborhood change that can
    # newly expose a parallel-column merge or a now-degenerate twig.
    rewire.optimize(forest, mesh, surface_tree, cfg, log=log)

    # Re-fix: rewire may have created or moved unbraced members (a fresh
    # reattach connector, a slid junction) -- one more propose/resize pass
    # against that post-rewire reality before certifying.
    report = transient.transient_report(forest, plate_z, cfg)
    bracing.propose_and_place(forest, surface_tree, plate_z, cfg, report, log=log)
    transient.resize_forest(forest, plate_z, cfg)
    violations = forest.revalidate(surface_tree, cfg)
    assert not violations, (
        f"brace_and_size fixed point ended with {len(violations)} "
        f"clearance-dirty member(s) after re-fix (an honest engine failure, "
        f"not a warning) -- member_idx: radius_mm = " + ", ".join(
            f"{v['member_idx']}: "
            f"{round(max(forest.members[v['member_idx']].r0, forest.members[v['member_idx']].r1), 4)}"
            for v in violations))
    tree.assert_all_certified(forest)

    member_ml_after = _live_member_ml(forest, unrouted)
    log["brace_and_size"] = dict(
        iterations=iterations, rungs_placed=rungs_placed,
        rungs_unplaceable=rungs_unplaceable, n_resized_down=n_resized_down,
        n_resized_up=n_resized_up, member_ml_before=member_ml_before,
        member_ml_after=member_ml_after)
    return log["brace_and_size"]


def finish(mesh, pre, forest, unrouted, contacts, carried, tip_r,
           surface_tree, cfg, log=None):
    """Stage 3b: verify gates + volume report on the final forest."""
    log = log if log is not None else {}
    plate_z = pre["plate_z"]
    tree.assert_all_certified(forest)
    members, balls = live_member_lists(forest, unrouted, plate_z, cfg)
    routed_contacts = [c for i, c in enumerate(contacts) if i not in unrouted]
    report = verify.self_verify(mesh, pre["islands"], routed_contacts, members,
                                 carried, tip_r, cfg, plate_z,
                                 surface_tree=surface_tree, minima=pre["minima"],
                                 forest=forest, unrouted=unrouted)
    volume = tree.volume_report(members, balls)
    log["n_unrouted"] = len(unrouted)
    return dict(members=members, balls=balls, verify=report, volume=volume,
                plate_z=plate_z, log=log)


def run(mesh, cfg, log=None):
    """Run the full v4 pipeline on an already-loaded trimesh.Trimesh.
    Returns a result dict: islands, contacts, carried_area_mm2, tip_r_mm,
    forest, members, balls, unrouted, minima, plate_z, verify, volume,
    log."""
    log = log if log is not None else {}
    t0 = time.time()

    pre = prepare(mesh, cfg, log)
    islands, minima, contacts = pre["islands"], pre["minima"], pre["contacts"]
    plate_z = pre["plate_z"]

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
                        murray_violations=[],
                        removal_report=dict(ok=True, n_trees=0, trees=[],
                                             removal_order=[], removal_effort={},
                                             blocked_trees=[]),
                        removal_violations=[]),
            volume=dict(member_ml=0.0, ball_ml=0.0, total_ml=0.0, by_kind_ml={}),
            log=log)

    # Built once, reused for every clearance query this run makes (see
    # routing.build_surface_tree's docstring). Tighter sample than the
    # default: every no-intersection guarantee in this engine rides on
    # this tree, so keep its approximation error small against
    # standoff_mm.
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)
    log["t_surface_tree"] = round(time.time() - t0, 2)

    forest = members = balls = report = carried = tip_r = unrouted = None
    max_fix_passes = 2
    for fix_pass in range(max_fix_passes):
        pre["contacts"] = contacts  # fix-pass additions feed back in
        forest, unrouted, carried, tip_r, contacts = construct(
            mesh, pre, surface_tree, cfg, log=log)
        # Phase 3 (design-v4.md #7): topology mutation runs AFTER
        # insertion, BEFORE verify -- rewire.optimize mutates `forest` in
        # place (subtree reattach / junction slide / twig prune), each
        # move already transactional+certificate-checked internally;
        # re-asserting after is a cheap belt-and-suspenders check.
        rewire.optimize(forest, mesh, surface_tree, cfg, log=log)
        # Phase 4c (design-v4.md #8/#9): build-time-honest transient sizing
        # + bracing fixed point, then its own second rewire pass against the
        # braced reality -- runs AFTER topology settles, BEFORE the gates
        # that decide whether this fix-pass is done.
        brace_and_size(mesh, forest, unrouted, surface_tree, plate_z, cfg, log=log)
        tree.assert_all_certified(forest)
        members, balls = live_member_lists(forest, unrouted, plate_z, cfg)
        log[f"forest_pass{fix_pass}"] = dict(
            n_members=len(members), n_balls=len(balls), n_unrouted=len(unrouted))
        # Verifier should-fix: surface n_unrouted top-level so regressions
        # vs the frozen baselines show up without digging.
        log["n_unrouted"] = len(unrouted)

        routed_contacts = [c for i, c in enumerate(contacts) if i not in unrouted]
        report = verify.self_verify(mesh, islands, routed_contacts, members, carried, tip_r,
                                     cfg, plate_z, surface_tree=surface_tree, minima=minima,
                                     forest=forest, unrouted=unrouted)
        log[f"verify_pass{fix_pass}"] = dict(
            ok=report["ok"],
            n_uncovered=len(report["uncovered_islands"]),
            n_clearance_violations=len(report["clearance_violations"]),
            n_load_violations=len(report["load_violations"]),
            n_unsupported_minima=len(report["unsupported_minima"]),
            n_connectivity_violations=len(report["connectivity_violations"]),
            n_printability_violations=len(report["printability_violations"]),
            n_murray_violations=len(report["murray_violations"]),
            # Phase 5 (design-v4.md #10, amended 2026-07-22): MEASUREMENT
            # ONLY -- self_verify's default gate_on_removal=False means
            # this never affects `ok`. Per Craig's 2026-07-22 ruling
            # (design-v4.md #10 amendment, superseding the earlier
            # "BLOCKER gate" charter language), removal_report is
            # diagnostic/report-only and removability is the least
            # important optimization axis -- it must never block a
            # layout that's otherwise superior on structural/physics
            # quality or resin use, so gate_on_removal stays an inert
            # (unused-by-any-caller) toggle rather than something wired
            # to engine.run acceptance.
            n_removal_blocked_trees=len(report["removal_violations"]),
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
