"""Orchestrates the full v2 pipeline end to end: overhang/island detection
-> density-calibrated candidate sampling -> load partition -> tip/shaft
sizing -> plate-vs-model-strut anchor choice -> articulated routing ->
brace graph -> self-verify (with a bounded fix pass, mirroring v0's
"fix pass added N heavy; islands now 0") -> optional CHITUBOX emission.

This is the one module allowed to know about all the others; every other
module stays independently testable. See references/design.md for the
charter and __init__.py for the module map.
"""
import time
import numpy as np

from . import perception, placement, routing, braces as braces_mod, struts, verify, mesh_io, trunks as trunks_mod


def _lean_deg(p0, p1):
    """Lean from vertical of the member p0->p1, for the angle-aware
    deflection sizing (v3.6)."""
    d = np.asarray(p1, float) - np.asarray(p0, float)
    L = float(np.linalg.norm(d))
    if L < 1e-9:
        return 0.0
    return float(np.degrees(np.arctan2(np.linalg.norm(d[:2]), abs(d[2]))))


def _trunk_route(contact, stub_end, tip_r_i, shoot_r, junction, cfg):
    """Route for a trunk-attached contact: embedded tip stub along the
    surface normal, then a single diagonal shoot (<= printability angle)
    down to the trunk junction. No horizontal members."""
    pos = np.asarray(contact["position"], float)
    normal = np.asarray(contact["normal"], float)
    normal = normal / (np.linalg.norm(normal) or 1.0)
    p_tip = pos - normal * cfg.contact_depth_mm
    p_stub_end = np.asarray(stub_end, float)
    segs = [routing.Segment(p_tip, p_stub_end, tip_r_i, shoot_r, "stub",
                             embedded_p0=True)]
    free = cfg.stub_length_mm
    jp = np.asarray(junction, float)
    L = float(np.linalg.norm(jp - p_stub_end))
    if L > 1e-9:
        segs.append(routing.Segment(p_stub_end, jp, shoot_r, shoot_r, "shoot"))
        free += L
    return routing.Route(contact_idx=-1, segments=segs, anchor_kind="trunk",
                          free_length_mm=free)


def _remap_parents(trunk_list, remap):
    """After dropping trunks, fix branch children's parent_idx; a child
    whose parent was dropped reverts to a plate foot (its lower column
    is restored conceptually -- radius/sizing rechecks follow)."""
    for t in trunk_list:
        if t.parent_idx is not None:
            if t.parent_idx in remap:
                t.parent_idx = remap[t.parent_idx]
            else:
                t.foot_kind, t.parent_idx = "plate", None
                t.z_split = t.z_start = None


def _trunk_pseudo_routes(trunk_list, plate_z, cfg):
    """Trunk columns as Route objects so the standard batched clearance
    checker can sweep them (kind='shaft' so nothing is skipped)."""
    out = []
    for t in trunk_list:
        base = t.base_z(plate_z)
        p0 = np.array([t.xy[0], t.xy[1], base])
        p1 = np.array([t.xy[0], t.xy[1], t.z_top])
        segs = [routing.Segment(p0, p1, t.r, t.r, "shaft")]
        if t.foot_kind == "branch" and t.parent_idx is not None:
            parent = trunk_list[t.parent_idx]
            p_split = np.array([parent.xy[0], parent.xy[1], t.z_split])
            segs.append(routing.Segment(p_split, p0, t.r, t.r, "shaft"))
        out.append(routing.Route(contact_idx=-1, segments=segs,
                                  anchor_kind="plate", free_length_mm=t.z_top - base))
    return out


def run(mesh, cfg, max_fix_passes=3, log=None, do_emit=False):
    """Run the full pipeline on an already-loaded trimesh.Trimesh. Returns
    a result dict with contacts, routes, braces, verify report, and
    timing -- everything engine callers (CLI, tests, emission) need."""
    log = log if log is not None else {}
    t0 = time.time()

    islands = perception.overhang_islands(mesh, cfg)
    log["islands"] = len(islands)
    log["t_islands"] = round(time.time() - t0, 2)

    # v3.6 (Craig, top priority): island-birth minima. Detected before
    # sampling so every minimum the overhang-angle model missed (steep
    # flanks) still ends up with a contact AT its birth region.
    minima = perception.local_z_minima(mesh, cfg)
    log["minima"] = len(minima)
    log["t_minima"] = round(time.time() - t0, 2)

    contacts = placement.sample_candidates(mesh, islands, cfg)
    log["contacts_initial"] = len(contacts)
    contacts, islands, n_min_added = _ensure_minima_contacts(
        mesh, islands, contacts, minima, cfg)
    log["minima_contacts_added"] = n_min_added

    if not contacts:
        log["t_total"] = round(time.time() - t0, 2)
        return dict(islands=islands, contacts=[], carried_area_mm2=np.array([]),
                    tip_r_mm=np.array([]), routes=[], braces=[], trunks=[],
                    plate_z=float(mesh.bounds[0][2]) - cfg.lift_mm, brace_graph=None,
                    minima=minima,
                    verify=dict(ok=True, uncovered_islands=[], clearance_violations=[],
                                load_violations=[], unsupported_minima=[]),
                    log=log)

    zmin = float(mesh.bounds[0][2])
    # Built once, reused for every clearance query this run makes (see
    # routing.build_surface_tree's docstring -- exact closest_point does
    # not scale to hundreds of routes on a real mesh). v3's trunk planner
    # also uses it to place clear columns up front.
    # Tighter sample than the default: every no-intersection guarantee in
    # this engine rides on this tree, so keep its approximation error
    # (~sample pitch) small against standoff_mm.
    surface_tree = routing.build_surface_tree(mesh, pitch_mm=0.3, max_points=400000)
    log["t_surface_tree"] = round(time.time() - t0, 2)
    # Grounded (non-overhang) surface points: anchor pool for fallback
    # model struts (contacts whose trunk column can't clear the model).
    g_pts, g_fidx = mesh.sample(30000, return_index=True)
    g_normals = mesh.face_normals[g_fidx]
    ground_points = g_pts[g_normals[:, 2] >= -np.sin(np.radians(cfg.overhang_angle_deg))]

    plate_z = zmin - cfg.lift_mm
    for fix_pass in range(max_fix_passes):
        carried = placement.load_partition(mesh, islands, contacts, cfg)
        tip_r = placement.tip_radius_mm(carried, cfg)

        # stub ends: out along the surface normal from each contact
        stub_ends = []
        for c in contacts:
            pos = np.asarray(c["position"], float)
            n = np.asarray(c["normal"], float)
            n = n / (np.linalg.norm(n) or 1.0)
            stub_ends.append(pos + n * cfg.stub_length_mm)

        trunk_list, assignment = trunks_mod.plan(
            contacts, tip_r, carried, stub_ends, plate_z, surface_tree, cfg)
        trunk_list, assignment = trunks_mod.consolidate(
            trunk_list, assignment, stub_ends, tip_r, surface_tree, cfg)

        # Fallback: contacts whose column never cleared get a strut
        # anchored ON the model below instead of a through-model column
        # (second review: those columns were visibly penetrating the
        # model on the way up).
        strut_anchor = {}
        for i, (t_idx, jz, clear) in enumerate(assignment):
            if clear:
                continue
            r_shoot = max(float(tip_r[i]), cfg.min_shoot_r_mm)
            anchor = trunks_mod.model_strut(contacts[i], stub_ends[i], r_shoot,
                                             ground_points, surface_tree, cfg)
            if anchor is not None:
                strut_anchor[i] = anchor
                trunk = trunk_list[t_idx]
                trunk.shoots.remove(i)
                trunk.carried_mm2 -= float(carried[i])
        # drop now-empty trunks, renumber
        if strut_anchor:
            keep = [t for t in range(len(trunk_list)) if trunk_list[t].shoots]
            remap = {old: new for new, old in enumerate(keep)}
            trunk_list = [trunk_list[t] for t in keep]
            assignment = [(remap.get(t), jz, c) for (t, jz, c) in assignment]
            for t in trunk_list:
                # top shrinks to the highest REMAINING junction
                t.z_top = max(assignment[i][1] for i in t.shoots)

        # ---- v3.5: Y-branch coalescing (sixth review) -- runs BEFORE
        # the stability loop so brace building, span sizing, and the
        # final-radius gates all operate on the final tree topology.
        junctions_by_trunk = {
            t: [(assignment[i][1], stub_ends[i][2]) for i in trunk.shoots]
            for t, trunk in enumerate(trunk_list)}
        merges = trunks_mod.coalesce_columns(
            trunk_list, junctions_by_trunk, plate_z, surface_tree, cfg)
        log[f"coalesce_pass{fix_pass}"] = dict(
            n_merges=len(merges),
            saved_mm3=round(sum(m["saved_mm3"] for m in merges), 1))

        # ---- Stability loop (third review: placement checked columns at
        # min radius, then span-resizing fattened them past the margin;
        # braces were never checked at all). Everything is re-checked at
        # FINAL radii; whatever can't be made clear is demoted to a model
        # strut or left honestly unsupported -- never emitted through the
        # mesh.
        unrouted = set()
        made_braces, brace_graph = [], None
        for _stability in range(3):
            trunks_mod.size_trunks(trunk_list, plate_z, cfg)
            made_braces, brace_graph = braces_mod.build_trunk_braces(
                trunk_list, plate_z, cfg)
            made_braces = _filter_clear_braces(made_braces, mesh, cfg, surface_tree)
            junctions_by_trunk = {
                t: [(assignment[i][1], stub_ends[i][2]) for i in trunk.shoots]
                for t, trunk in enumerate(trunk_list)}
            # spans no trunk-neighbor could restrain get short rungs to
            # the MODEL instead -- keeps isolated canopy trunks slender
            # (a slender column clears where a span-fattened one clips)
            made_braces += braces_mod.build_model_braces(
                trunk_list, made_braces, junctions_by_trunk, ground_points,
                surface_tree, plate_z, cfg)
            trunks_mod.resize_for_spans(trunk_list, made_braces,
                                         junctions_by_trunk, plate_z, cfg)
            bad = [ti for ti, t in enumerate(trunk_list)
                   if not trunks_mod.trunk_column_ok(t, plate_z, surface_tree, cfg)]
            if not bad:
                break
            drop = set()
            for ti in bad:
                t = trunk_list[ti]
                if t.foot_kind == "branch":
                    # a branch column that no longer clears reverts to a
                    # plate foot and gets re-judged next iteration
                    parent = trunk_list[t.parent_idx]
                    parent.carried_mm2 -= t.carried_mm2
                    parent.extra_shoots -= len(t.shoots) + t.extra_shoots
                    t.foot_kind, t.parent_idx = "plate", None
                    t.z_split = t.z_start = None
                    continue
                res = trunks_mod.relocate_trunk(t, stub_ends, tip_r, plate_z,
                                                 surface_tree, cfg)
                if res is not None:
                    xy, juncs = res
                    t.xy = np.asarray(xy, float)
                    for i, jz in juncs.items():
                        assignment[i] = (ti, jz, True)
                    t.z_top = max(juncs.values())
                else:
                    for i in list(t.shoots):
                        # v3.6: dissolving a whole cluster because no ONE
                        # xy serves every shoot loses solvable singles
                        # (toed-slab fixture: the apex minimum's own
                        # straight-down column was fine; it dissolved with
                        # five doomed flank neighbors). Retry each shoot
                        # as a solo column before strut/unrouted.
                        solo = trunks_mod.Trunk(np.asarray(stub_ends[i][:2], float),
                                                 stub_ends[i][2])
                        solo.shoots = [i]
                        solo.carried_mm2 = float(carried[i])
                        solo.r = cfg.min_trunk_r_mm
                        res2 = trunks_mod.relocate_trunk(
                            solo, stub_ends, tip_r, plate_z, surface_tree, cfg)
                        if res2 is not None:
                            xy2, juncs2 = res2
                            solo.xy = np.asarray(xy2, float)
                            solo.z_top = max(juncs2.values())
                            trunk_list.append(solo)
                            assignment[i] = (len(trunk_list) - 1, juncs2[i], True)
                            continue
                        r_shoot = max(float(tip_r[i]), cfg.min_shoot_r_mm)
                        anchor = trunks_mod.model_strut(
                            contacts[i], stub_ends[i], r_shoot,
                            ground_points, surface_tree, cfg)
                        if anchor is not None:
                            strut_anchor[i] = anchor
                        else:
                            unrouted.add(i)
                    drop.add(ti)
            if drop:
                keep = [ti for ti in range(len(trunk_list)) if ti not in drop]
                remap = {old: new for new, old in enumerate(keep)}
                trunk_list = [trunk_list[ti] for ti in keep]
                assignment = [((remap.get(tj) if tj is not None else None), jz, c)
                              for (tj, jz, c) in assignment]
                _remap_parents(trunk_list, remap)

        log[f"trunks_pass{fix_pass}"] = dict(
            n_trunks=len(trunk_list),
            n_struts=len(strut_anchor),
            n_unrouted=len(unrouted),
            max_shoots=max(len(t.shoots) for t in trunk_list) if trunk_list else 0,
            n_braces=len(made_braces),
            r_range=[round(min(t.r for t in trunk_list), 2),
                     round(max(t.r for t in trunk_list), 2)] if trunk_list else None)

        routes = []
        for i, (c, se) in enumerate(zip(contacts, stub_ends)):
            if i in unrouted:
                continue
            if i in strut_anchor:
                anchor = strut_anchor[i]
                shoot_len = float(np.linalg.norm(anchor - se))
                r_shoot = trunks_mod.shoot_radius(float(tip_r[i]), shoot_len,
                                                   float(carried[i]), cfg,
                                                   angle_deg=_lean_deg(se, anchor))
                route = _trunk_route(c, se, float(tip_r[i]), r_shoot, anchor, cfg)
                route.segments[-1].embedded_p1 = True  # adsorbed model foot
                route.anchor_kind = "model"
                route.contact_idx = i
                route.trunk_idx = None
                routes.append(route)
                continue
            t_idx, jz, _clear = assignment[i]
            trunk = trunk_list[t_idx]
            junction = np.array([trunk.xy[0], trunk.xy[1], jz])
            shoot_len = float(np.linalg.norm(junction - se))
            r_shoot = trunks_mod.shoot_radius(float(tip_r[i]), shoot_len,
                                               float(carried[i]), cfg)
            route = _trunk_route(c, se, float(tip_r[i]), r_shoot, junction, cfg)
            route.contact_idx = i
            route.trunk_idx = t_idx
            routes.append(route)

        # Final gate on the contact routes themselves: anything still
        # violating (at final radii) is demoted -- strut if one clears,
        # honestly unsupported otherwise -- and its trunk bookkeeping is
        # unwound so no stale column or brace outlives it.
        def _make_strut(i):
            r_probe = max(float(tip_r[i]), cfg.min_shoot_r_mm)
            anchor = trunks_mod.model_strut(contacts[i], stub_ends[i], r_probe,
                                             ground_points, surface_tree, cfg)
            if anchor is None:
                return None
            # size for the strut's actual length and lean (it grows from
            # the model anchor as a free cantilever until it reaches the
            # stub; steeper lean = bigger peel slice, angle-aware term)
            L = float(np.linalg.norm(np.asarray(anchor) - stub_ends[i]))
            r_shoot = trunks_mod.shoot_radius(float(tip_r[i]), L,
                                               float(carried[i]), cfg,
                                               angle_deg=_lean_deg(stub_ends[i], anchor))
            strut = _trunk_route(contacts[i], stub_ends[i], float(tip_r[i]),
                                  r_shoot, anchor, cfg)
            strut.segments[-1].embedded_p1 = True
            strut.anchor_kind = "model"
            strut.contact_idx = i
            strut.trunk_idx = None
            if routing.check_clearance(mesh, strut, cfg, surface_tree=surface_tree):
                return None
            return strut

        route_viol = routing.check_clearance_batch(mesh, routes, cfg,
                                                    surface_tree=surface_tree)
        cleaned, changed = [], False
        for route, viol in zip(routes, route_viol):
            if not viol:
                cleaned.append(route)
                continue
            i = route.contact_idx
            changed = True
            if route.trunk_idx is not None:  # unwind the trunk's bookkeeping
                trunk = trunk_list[route.trunk_idx]
                trunk.shoots.remove(i)
                trunk.carried_mm2 -= float(carried[i])
            strut = None if route.anchor_kind == "model" else _make_strut(i)
            if strut is not None:
                cleaned.append(strut)
            else:
                unrouted.add(i)
        routes = cleaned

        if changed:
            # drop emptied trunks (+ braces referencing them), refresh
            # tops/spans, and re-gate the fattened columns one last time:
            # any trunk STILL violating is dissolved into struts/unrouted.
            keep = [ti for ti, t in enumerate(trunk_list) if t.shoots]
            remap = {old: new for new, old in enumerate(keep)}
            trunk_list = [trunk_list[ti] for ti in keep]
            assignment = [((remap.get(tj) if tj is not None else None), jz, c)
                          for (tj, jz, c) in assignment]
            _remap_parents(trunk_list, remap)
            made_braces = [dict(b, i=remap[b["i"]],
                                 j=(remap[b["j"]] if b["j"] in remap else b["j"]))
                           for b in made_braces
                           if b["i"] in remap and (b["j"] == -1 or b["j"] in remap)]
            for route in routes:
                if route.trunk_idx is not None:
                    route.trunk_idx = remap[route.trunk_idx]
            for t in trunk_list:
                t.z_top = max(assignment[i][1] for i in t.shoots)
            junctions_by_trunk = {
                t: [(assignment[i][1], stub_ends[i][2]) for i in trunk.shoots]
                for t, trunk in enumerate(trunk_list)}
            trunks_mod.resize_for_spans(trunk_list, made_braces,
                                         junctions_by_trunk, plate_z, cfg)
            still_bad = {ti for ti, t in enumerate(trunk_list)
                         if not trunks_mod.trunk_column_ok(t, plate_z,
                                                            surface_tree, cfg)}
            if still_bad:
                dissolved = {i for ti in still_bad for i in trunk_list[ti].shoots}
                routes2 = []
                for route in routes:
                    i = route.contact_idx
                    if i not in dissolved:
                        routes2.append(route)
                        continue
                    strut = _make_strut(i)
                    if strut is not None:
                        routes2.append(strut)
                    else:
                        unrouted.add(i)
                routes = routes2
                keep = [ti for ti in range(len(trunk_list)) if ti not in still_bad]
                remap = {old: new for new, old in enumerate(keep)}
                trunk_list = [trunk_list[ti] for ti in keep]
                assignment = [((remap.get(tj) if tj is not None else None), jz, c)
                              for (tj, jz, c) in assignment]
                _remap_parents(trunk_list, remap)
                made_braces = [dict(b, i=remap[b["i"]],
                                     j=(remap[b["j"]] if b["j"] in remap else b["j"]))
                               for b in made_braces
                               if b["i"] in remap and (b["j"] == -1 or b["j"] in remap)]
                for route in routes:
                    if route.trunk_idx is not None:
                        route.trunk_idx = remap[route.trunk_idx]
        log[f"unrouted_pass{fix_pass}"] = len(unrouted)

        # ---- Economic cross-bracing pass (Craig, fifth review): rungs
        # from long diagonals to trunks / sibling shoots / model let the
        # diagonal be THINNER; each member keeps its rung set only if
        # thinned member + rungs beats the fat unbraced member on resin.
        # Thinning is monotone-safe for clearance (smaller always clears
        # where bigger did), so no re-search is needed.
        def _member_volume():
            v = sum(np.pi * t.r ** 2 * (t.z_top - plate_z) for t in trunk_list)
            for rt in routes:
                for sg in rt.segments:
                    v += np.pi * sg.r0 ** 2 * float(np.linalg.norm(sg.p1 - sg.p0))
            v += sum(np.pi * b["r"] ** 2
                      * float(np.linalg.norm(np.asarray(b["p1"]) - np.asarray(b["p0"])))
                      for b in made_braces)
            return v

        vol_before = _member_volume()
        # v3.5b: bundle long parallel shoots into collector hubs + one
        # shared riser (the green-pair case: contacts close ON THE MODEL,
        # feet from anywhere) BEFORE rung-thinning, so rungs see the
        # short fan shoots and skip them.
        risers = trunks_mod.bundle_shoots(routes, trunk_list, tip_r, plate_z,
                                           surface_tree, cfg)
        # bundling may shrink trunk tops -- drop braces now attached above
        def _attached_below_top(b):
            for side, key in ((b["i"], "p0"), (b.get("j"), "p1")):
                if isinstance(side, (int, np.integer)) and 0 <= side < len(trunk_list):
                    if float(np.asarray(b[key])[2]) > trunk_list[side].z_top + 0.1:
                        return False
            return True
        made_braces = [b for b in made_braces if _attached_below_top(b)]
        rungs = braces_mod.brace_and_thin_shoots(
            routes, trunk_list, plate_z, ground_points, surface_tree, cfg)
        made_braces = made_braces + rungs
        log[f"thinning_pass{fix_pass}"] = dict(
            n_rungs=len(rungs), n_risers=len(risers),
            riser_saved_mm3=round(sum(r["saved_mm3"] for r in risers), 1),
            member_vol_mm3=round(_member_volume(), 1),
            saved_mm3=round(vol_before - _member_volume(), 1))

        # verify sweeps contact routes AND trunk columns; coverage is
        # judged on the contacts that actually GOT a support, so islands
        # of unrouted contacts surface as uncovered (honest reporting)
        pseudo = _trunk_pseudo_routes(trunk_list, plate_z, cfg)
        for rs in risers:  # riser members swept by verify too
            pseudo.append(routing.Route(contact_idx=-1, segments=[
                routing.Segment(np.asarray(rs["p0"]), np.asarray(rs["p1"]),
                                 rs["r"], rs["r"], "shaft")],
                anchor_kind="plate", free_length_mm=0.0))
        routed_contacts = [c for i, c in enumerate(contacts) if i not in unrouted]
        report = verify.self_verify(mesh, islands, routed_contacts, routes + pseudo,
                                     carried, tip_r, cfg, surface_tree=surface_tree,
                                     minima=minima)
        log[f"verify_pass{fix_pass}"] = dict(
            ok=report["ok"],
            n_uncovered=len(report["uncovered_islands"]),
            n_clearance_violations=len(report["clearance_violations"]),
            n_load_violations=len(report["load_violations"]),
            n_unsupported_minima=len(report["unsupported_minima"]),
        )
        if report["ok"] or not report["uncovered_islands"]:
            break
        # Only add fix contacts on islands that are uncovered because
        # they lacked contacts -- islands whose contacts exist but proved
        # UNROUTABLE would just accumulate more unroutable contacts
        # (third review run: 359 contacts ballooned to 507 this way).
        unroutable_islands = {contacts[i]["island_id"] for i in unrouted}
        fixable = [u for u in report["uncovered_islands"]
                   if u["island_id"] not in unroutable_islands]
        if not fixable:
            break
        contacts = _add_fix_contacts(mesh, islands, contacts, fixable)

    log["t_total"] = round(time.time() - t0, 2)
    return dict(
        islands=islands, contacts=contacts, carried_area_mm2=carried,
        tip_r_mm=tip_r, routes=routes, braces=made_braces, trunks=trunk_list,
        risers=risers, unrouted=sorted(unrouted), minima=minima,
        plate_z=plate_z, brace_graph=brace_graph, verify=report, log=log,
    )


def _ensure_minima_contacts(mesh, islands, contacts, minima, cfg):
    """v3.6: every island-birth minimum (perception.local_z_minima) gets a
    contact touching its birth region unless one already does. A contact
    merely NEAR the minimum does not count -- the satisfaction test is
    distance to the birth-region vertices, which only spans the first
    couple of layers of the island (Craig: what matters is how many layers
    pass before the island connects, not lateral proximity).

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
    tree = cKDTree(have) if len(have) else None
    face_island = -np.ones(len(mesh.faces), dtype=np.int64)
    for iid, fids in enumerate(islands):
        face_island[fids] = iid
    centers_tree = cKDTree(mesh.triangles_center)
    added = 0
    for m in minima:
        if tree is not None:
            d, _ = tree.query(m["region_pts"],
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


def _filter_clear_braces(braces, mesh, cfg, surface_tree):
    """Drop any brace rod that would clip the model (third review: braces
    were never clearance-checked and the X lattice was cutting through
    geometry between trunks). resize_for_spans runs AFTER this, so a
    dropped brace correctly shows up as a longer unbraced span and
    thickens its trunks instead."""
    if not braces:
        return braces
    pseudo = [routing.Route(contact_idx=-1, segments=[
        routing.Segment(np.asarray(b["p0"], float), np.asarray(b["p1"], float),
                         b["r"], b["r"], "shaft")],
        anchor_kind="plate", free_length_mm=0.0) for b in braces]
    viol = routing.check_clearance_batch(mesh, pseudo, cfg,
                                          surface_tree=surface_tree)
    return [b for b, v in zip(braces, viol) if not v]


def _repair_clearance_with_detours(mesh, routes, contacts, tip_r, cfg,
                                     radii_mm=(3.0, 6.0, 10.0, 16.0, 25.0), n_directions=12,
                                     surface_tree=None):
    """v2's routing is a fixed-topology kink (design.md #4), not a general
    path planner. When a straight vertical descent is blocked by an
    obstruction directly below the contact (v0's exact failure mode --
    design.md #1), try a bounded set of lateral detour offsets before the
    descent and keep the first that fully clears. Repairs are LOCAL (only
    the violating route is rebuilt) and honest: if nothing in the search
    clears it, the original route is kept and the violation surfaces in
    self_verify rather than being hidden."""
    n_repaired = 0
    # One batched surface query across ALL routes (routing.query_workers
    # explains why: per-route queries put ~90% of this loop's wall time
    # into scipy thread-pool churn on the real 576k-face model).
    all_violations = routing.check_clearance_batch(mesh, routes, cfg,
                                                    surface_tree=surface_tree)
    for i, (route, violations) in enumerate(zip(routes, all_violations)):
        if not violations or route.anchor_kind != "plate":
            continue
        if not any(v["segment_kind"] in ("shaft", "detour") for v in violations):
            continue
        c = contacts[i]
        r = float(tip_r[i])
        base_anchor = route.segments[-1].p1
        shaft_r, n_braces, eff_len = placement.shaft_radius_for_length_mm(
            route.free_length_mm, cfg)
        # Build the whole (radius x direction) candidate fan up front and
        # clearance-check it in ONE batched query, then keep the first
        # clear candidate in the same (smallest radius first, then
        # direction) order the sequential search used -- identical
        # selection, one surface query instead of up to 60.
        candidates = []
        for radius in radii_mm:
            for k in range(n_directions):
                ang = 2 * np.pi * k / n_directions
                dx, dy = radius * np.cos(ang), radius * np.sin(ang)
                detour_xy = (c["position"][0] + dx, c["position"][1] + dy)
                candidate_anchor = np.array([detour_xy[0], detour_xy[1], base_anchor[2]])
                candidates.append(routing.route_to_anchor(
                    c, candidate_anchor, r_tip=r, r_shaft=shaft_r, cfg=cfg,
                    anchor_kind="plate", detour_xy=detour_xy))
        cand_violations = routing.check_clearance_batch(
            mesh, candidates, cfg, surface_tree=surface_tree)
        for candidate, cv in zip(candidates, cand_violations):
            if not cv:
                candidate.n_braces = n_braces
                routes[i] = candidate
                n_repaired += 1
                break
    return routes, n_repaired


def _add_fix_contacts(mesh, islands, contacts, uncovered):
    """One extra contact at the worst-covered point of each still-
    uncovered island -- mirrors v0's fix pass."""
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
            tree = cKDTree(existing)
            dist, _ = tree.query(pts, workers=routing.query_workers(len(pts)))
        else:
            dist = np.full(len(pts), np.inf)
        worst = int(np.argmax(dist))
        extra.append(dict(position=pts[worst], normal=normals[face_ids[worst]],
                           island_id=iid, area_mm2=u["n_faces"]))
    return extra
