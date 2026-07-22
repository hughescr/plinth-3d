"""Self-verify (design.md #9): "zero unsupported islands, clearance check,
plus per-region load check against green-strength SF." Mandatory, not
optional -- engine.py treats a non-empty report as a fix-pass trigger, not
just a warning.

Also hosts the reusable scar-cost orientation scorer (design.md
requirement 3: "same cost function, applied one stage earlier" -- rotate
delicate regions away from support-facing surfaces). This is exposed for
prep-mini's orient stage to call; the support-placement engine itself
doesn't choose orientation, it evaluates candidates handed to it.
"""
import numpy as np
from scipy.spatial import cKDTree

from . import perception, routing, joints, tree as tree_mod, removal as removal_mod


def coverage_report(mesh, islands, contacts, cfg):
    """Independent re-check (not just trusting sample_candidates by
    construction): for each island, the worst-covered face's distance to
    its nearest assigned contact. Flags islands whose worst point exceeds
    cfg.target_spacing_mm * cfg.coverage_radius_factor."""
    threshold = cfg.target_spacing_mm * cfg.coverage_radius_factor
    by_island = {}
    for c in contacts:
        by_island.setdefault(c["island_id"], []).append(c["position"])

    report = []
    for island_id, face_ids in enumerate(islands):
        positions = by_island.get(island_id)
        centers = mesh.triangles_center[face_ids]
        if not positions:
            report.append(dict(island_id=island_id, covered=False,
                                worst_gap_mm=float("inf"), n_faces=len(face_ids)))
            continue
        tree = cKDTree(np.array(positions))
        dist, _ = tree.query(centers, workers=routing.query_workers(len(centers)))
        worst = float(dist.max())
        report.append(dict(island_id=island_id, covered=worst <= threshold,
                            worst_gap_mm=worst, n_faces=len(face_ids)))
    return report


def clearance_report(mesh, members, cfg, surface_tree=None):
    """v4: independent full-envelope clearance re-check over the FINAL
    member list. Deliberately does not reuse tree.Forest.revalidate's
    certificate bookkeeping (a builder bug that mis-certifies a member
    must not also fool the verify gate) -- same sampling math (sample at
    clearance_sample_step_mm, linear radius taper, skip the embed-end
    inset described below), applied unconditionally to every member
    regardless of cert status, as an honest independent check.
    `surface_tree` (see routing.build_surface_tree), if given, uses the
    fast approximate path real runs always use; None falls back to exact
    trimesh.proximity (routing._surface_distances) the same way
    routing.check_clearance's legacy path does -- fine for a handful of
    members in a test, not for a real mesh (see build_surface_tree's own
    docstring for why). No surface_tree also means no pitch_bound_mm
    (routing.surface_tree_pitch_bound_mm needs a tree to measure) -- the
    exact-trimesh path is inherently pitch-uncertainty-free (it queries
    the true continuous surface, not an approximating point cloud), so
    pitch_bound_mm is 0.0 in that branch, correctly (nothing to budget
    for).

    "stub" members are skipped entirely, same as routing.
    sample_route_envelope's existing precedent and tree.Forest.
    revalidate's identical rule (see that docstring for why a fixed inset
    alone isn't enough for a member that travels along the surface normal
    at a near-grazing angle).

    Embed-end exemption (p6-routing, Findings A/B): a SECOND, INDEPENDENT
    implementation of the SAME radius- and pitch-principled inset
    tree.Forest.revalidate derives (see that function's docstring for the
    full derivation) -- inset = contact_depth_mm + pitch_bound_mm + r_end
    + standoff_mm. Deliberately reimplemented here rather than calling
    into tree.py (preserving this function's own stated independence:
    "a builder bug... must not also fool the verify gate" applies just as
    much to a clearance-gate bug as a certificate bug) -- only the
    GENERIC point-cloud-density primitive (routing.surface_tree_
    pitch_bound_mm, exactly the same kind of shared low-level utility
    this function already calls via routing._surface_distances /
    routing.build_surface_tree) is shared, never the masking/exemption
    logic itself. This was ALSO the OLD fixed contact_depth_mm+0.2 inset
    (byte-identical to the pre-fix tree.py formula, independently proven
    stale on the real Weeping Treant model: 26 false-positive clearance_
    violations on table_mesh alone, all traced to this exact formula and
    confirmed to vanish when this same fix is applied).

    Returns list of dicts {member_idx, kind, min_clearance_mm, at_point}.
    """
    members = joints.collect_members(members)
    out = []
    if not members:
        return out
    pitch_bound_mm = (routing.surface_tree_pitch_bound_mm(
        surface_tree, percentile=cfg.surface_pitch_bound_percentile)
        if surface_tree is not None else 0.0)
    all_pts, all_r, all_mi = [], [], []
    for mi, m in enumerate(members):
        if m["kind"] == "stub":
            continue
        p0, p1 = np.asarray(m["p0"], float), np.asarray(m["p1"], float)
        r0, r1 = float(m["r0"]), float(m["r1"])
        length = float(np.linalg.norm(p1 - p0))
        if length < 1e-9:
            pts = p0[None, :]
            radii = np.array([max(r0, r1)])
        else:
            n = max(int(length / cfg.clearance_sample_step_mm), 1)
            t = np.linspace(0.0, 1.0, n + 1)
            if m.get("embed_end") == "p0":
                inset = cfg.contact_depth_mm + pitch_bound_mm + r0 + cfg.standoff_mm
                t = t[t * length >= inset]
            if m.get("embed_end") == "p1":
                inset = cfg.contact_depth_mm + pitch_bound_mm + r1 + cfg.standoff_mm
                t = t[(1 - t) * length >= inset]
            if len(t) == 0:
                continue
            pts = p0[None, :] + t[:, None] * (p1 - p0)[None, :]
            radii = r0 + t * (r1 - r0)
        all_pts.append(pts)
        all_r.append(radii)
        all_mi.append(np.full(len(radii), mi, dtype=int))
    if not all_pts:
        return out
    pts_all = np.concatenate(all_pts)
    r_all = np.concatenate(all_r)
    mi_all = np.concatenate(all_mi)
    ub = float(r_all.max()) + cfg.standoff_mm
    dist = routing._surface_distances(mesh, pts_all, surface_tree, upper_bound=ub)
    clearance = dist - r_all - cfg.standoff_mm
    for mi, m in enumerate(members):
        mask = mi_all == mi
        if not mask.any():
            continue
        worst = float(clearance[mask].min())
        if worst < 0:
            widx = np.nonzero(mask)[0][int(np.argmin(clearance[mask]))]
            out.append(dict(member_idx=mi, kind=m["kind"], min_clearance_mm=worst,
                             at_point=pts_all[widx].tolist()))
    return out


def murray_report(forest, cfg, tolerance=0.05):
    """New (design-v4.md #5, #11): per-junction Murray-law conservation
    sanity check, r_parent^4 >= sum(r_child^4) * (1 - tolerance). A
    parent member's radius at its p0 (the junction end -- children attach
    there, see tree.Forest's parent/child convention) may legitimately
    sit ABOVE conservation (physics floors -- min radius, transient spans
    -- may push it up), but never meaningfully below: that would mean an
    upstream sizing bug. Only evaluated at true branch junctions (>= 2
    children) -- a single pass-through attachment is topology, not a
    merge, and has nothing to conserve against. Honestly-unrouted
    terminals never enter the forest, so they are excluded by
    construction (nothing to sum).

    Returns list of dicts {member_idx, r_parent, r_child_equiv, deficit}.
    """
    violations = []
    for pi, children in enumerate(forest.children):
        if len(children) < 2:
            continue
        parent = forest.members[pi]
        r_parent4 = float(parent.r0) ** 4
        sum_child4 = sum(float(forest.members[ci].r1) ** 4 for ci in children)
        if r_parent4 < sum_child4 * (1.0 - tolerance):
            violations.append(dict(member_idx=pi, r_parent=float(parent.r0),
                                    r_child_equiv=sum_child4 ** 0.25,
                                    deficit=sum_child4 - r_parent4))
    return violations


def load_report(carried_area_mm2, tip_r_mm, cfg):
    """Per-contact realized safety factor against green tensile strength;
    flags any contact whose tip radius got CLAMPED (design.md's 0.2-1.0mm
    removal-friendly band) below what its carried load actually needs --
    those are real capacity shortfalls, not just cosmetic clamping, and
    should drive adding a neighboring contact to split the load."""
    area_m2 = np.asarray(carried_area_mm2) * 1e-6
    force_n = area_m2 * cfg.peel_pressure_pa
    r_m = np.asarray(tip_r_mm) / 1000.0
    tensile_pa = cfg.tensile_green_mpa * 1e6
    stress_pa = force_n / (np.pi * np.maximum(r_m, 1e-9) ** 2)
    realized_sf = tensile_pa / np.maximum(stress_pa, 1e-9)
    out = []
    for i, sf in enumerate(realized_sf):
        if sf < cfg.tip_safety_factor - 1e-6:
            out.append(dict(contact_idx=i, realized_sf=float(sf),
                             required_sf=cfg.tip_safety_factor))
    return out


def minima_report(minima, routed_contacts, cfg):
    """Layer-island check (v3.6, Craig's rule): every island-birth
    minimum (perception.local_z_minima -- already filtered to the ones
    that do NOT connect to earlier-printed material within the layer
    tolerance) must have a ROUTED contact touching its birth region.
    Proximity elsewhere on the model does not count: a contact 1mm away
    laterally on a shallow toe is 10+ layers above the island's birth.
    Returns the unsupported minima as dicts {z, position, min_gap_mm}."""
    if not minima:
        return []
    out = []
    positions = np.array([np.asarray(c["position"], float)
                          for c in routed_contacts]) if routed_contacts else None
    tree = cKDTree(positions) if positions is not None and len(positions) else None
    for m in minima:
        if tree is None:
            out.append(dict(z=m["z"], position=np.asarray(m["position"]).tolist(),
                            min_gap_mm=float("inf")))
            continue
        dist, _ = tree.query(m["region_pts"],
                             workers=routing.query_workers(len(m["region_pts"])))
        gap = float(np.min(dist))
        if gap > cfg.minima_contact_radius_mm:
            out.append(dict(z=m["z"], position=np.asarray(m["position"]).tolist(),
                            min_gap_mm=gap))
    return out


def island_birth_report(island_births, routed_contacts, cfg):
    """I3: the HONEST island-birth coverage gate (v4 p6, Craig's strict
    criterion, verbatim: "every island birth ... must have a support
    contact within ~2 real print layers (0.1mm z) landing ON the birth
    component's footprint (or within that contact's own attach radius of
    its boundary)"). Consumes perception.island_births (I1's ground-truth
    slice-based birth detection at the real print layer height), NOT
    local_z_minima's birth regions -- minima_report (above) already gates
    those independently; this is a second, geometrically-independent
    check, not a stricter version of the same one.

    A birth is covered iff some ROUTED contact (proximity elsewhere,
    or an unrouted stub, does not count -- an unrouted contact provides
    zero physical support) satisfies BOTH:
      - z: |contact.z - birth.z| <= cfg.minima_connect_layers *
        cfg.layer_height_mm (the ~2-real-print-layer / 0.1mm budget);
      - xy: on the birth polygon, or within
        perception.ISLAND_ATTACH_REACH_MM of its boundary.
    This REPLACES minima_report's cfg.minima_contact_radius_mm as the
    coverage radius for THIS data -- that field's own docstring says it
    was "NOT a coverage radius... only absorbs sampling gaps", which is
    exactly how the pre-p6 engine could self-certify while a full-model
    ground-truth audit found real uncovered births (design scratchpad:
    island_audit_fullmodel_0.05mm_result.json, 77/211 uncovered against a
    gate that reported 51).

    Returns list of uncovered-birth dicts {x, y, z, area_mm2, gap_mm,
    gap_layers}, gap_mm being the true nearest-in-3D-space routed contact
    (an honest number even when nothing anywhere qualifies on xy) -- an
    empty list means every slice-detected birth is genuinely supported at
    birth, not merely "close to something.\""""
    if not island_births:
        return []
    z_tol_mm = cfg.minima_connect_layers * cfg.layer_height_mm
    attach_r = perception.ISLAND_ATTACH_REACH_MM
    positions = np.array([np.asarray(c["position"], float)
                          for c in routed_contacts]) if routed_contacts else np.zeros((0, 3))
    out = []
    for b in island_births:
        bz = b["z"]
        covered = False
        if len(positions):
            z_ok = np.nonzero(np.abs(positions[:, 2] - bz) <= z_tol_mm)[0]
            for k in z_ok:
                cx, cy, _cz = positions[k]
                ok, _d = perception.island_birth_xy_ok(float(cx), float(cy), b["loop"], attach_r)
                if ok:
                    covered = True
                    break
        if covered:
            continue
        if len(positions):
            d3 = np.linalg.norm(
                positions - np.array([b["x"], b["y"], bz]), axis=1)
            gap = float(d3.min())
        else:
            gap = float("inf")
        out.append(dict(x=b["x"], y=b["y"], z=bz, area_mm2=b["area_mm2"],
                        gap_mm=gap,
                        gap_layers=(gap / cfg.layer_height_mm
                                    if np.isfinite(gap) else float("inf"))))
    return out


def connectivity_report(members, plate_z, cfg, surface_tree=None):
    """v3.7 hard gate (Craig's Chitubox review), signature-migrated for v4
    (design-v4.md #2): the stale-brace-leak and chained-Y-branch defects
    both manifested the same way in the emitted file -- a member with an
    endpoint touching NOTHING (a floating rod). This makes "nothing
    floats" a checked invariant instead of a hope.

    Every member (joints.collect_members) has two endpoints; each must be
    ATTACHED by at least one of:
      (a) within 0.25mm of ANOTHER member's axis segment (point-to-segment
          distance, self excluded) -- the general "something is really
          there" rule, covering junctions and any mid-member landing;
      (b) plate level (abs(z - plate_z) <= 0.2mm) -- a plate-rooted
          member's base. (v4 simplification, recorded: v3 widened this
          band by flare_len_mm to cover an emission-only flare/pad detail
          below a column's modeled base; v4's plate-descent candidate has
          no such offset -- its root member's p1 lands exactly at
          plate_z -- so the widening is dropped rather than carried
          forward as unexplained slack.)
      (c) embedded in the model surface (within cfg.contact_depth_mm+0.45)
          -- ONLY for the endpoint that is that member's DESIGNED
          embed_end (replaces v3's kind/sentinel table: a stub's tip
          (p0), a model-anchor strut's foot (p1), or any other member
          explicitly built with an embed_end). Any OTHER endpoint merely
          being near the model is explicitly NOT attachment -- that would
          silently wave through an unbraced floating rod that happens to
          pass close to the mesh. Preserved verbatim: with no
          surface_tree, an embed-eligible endpoint is honestly flagged
          rather than silently passed (the fixture pinning this case
          stays a guard on engine.py always supplying one for real runs).

    Returns a list of violation dicts {kind, endpoint (xyz list), gap_mm}
    -- gap_mm is the distance to the nearest legitimate attachment of the
    allowed kinds for that endpoint. Vectorized (member-axis distances
    computed in point-chunks against all member axes at once) to stay well
    under a second at real-model scale.
    """
    members = joints.collect_members(members)
    if not members:
        return []

    m_p0 = np.array([m["p0"] for m in members], dtype=float)
    m_p1 = np.array([m["p1"] for m in members], dtype=float)
    d_axis = m_p1 - m_p0
    L2 = np.einsum("ij,ij->i", d_axis, d_axis)
    L2safe = np.where(L2 < 1e-18, 1.0, L2)
    zero_len = L2 < 1e-18

    # endpoints: (member_idx, point, embed_eligible)
    ep_mi, ep_pts, ep_embed = [], [], []
    for mi, m in enumerate(members):
        embed0 = m.get("embed_end") == "p0"
        embed1 = m.get("embed_end") == "p1"
        ep_mi.append(mi); ep_pts.append(m["p0"]); ep_embed.append(embed0)
        ep_mi.append(mi); ep_pts.append(m["p1"]); ep_embed.append(embed1)

    ep_mi = np.array(ep_mi)
    pts = np.array(ep_pts, dtype=float)
    embed_ok = np.array(ep_embed, dtype=bool)

    n_ep = len(ep_mi)
    best_axis_dist = np.full(n_ep, np.inf)
    CHUNK = 256
    for start in range(0, n_ep, CHUNK):
        end = min(start + CHUNK, n_ep)
        chunk_pts = pts[start:end]                              # (c,3)
        chunk_mi = ep_mi[start:end]                              # (c,)
        rel = chunk_pts[:, None, :] - m_p0[None, :, :]           # (c,n,3)
        t = np.einsum("cnk,nk->cn", rel, d_axis) / L2safe[None, :]
        t = np.clip(t, 0.0, 1.0)
        proj = m_p0[None, :, :] + t[:, :, None] * d_axis[None, :, :]
        dist = np.linalg.norm(chunk_pts[:, None, :] - proj, axis=2)  # (c,n)
        if zero_len.any():
            dist[:, zero_len] = np.linalg.norm(
                chunk_pts[:, None, :] - m_p0[None, zero_len, :], axis=2)
        rows = np.arange(end - start)
        dist[rows, chunk_mi] = np.inf  # exclude the endpoint's own member
        best_axis_dist[start:end] = dist.min(axis=1)

    attached_a = best_axis_dist <= 0.25

    z = pts[:, 2]
    attached_b = np.abs(z - plate_z) <= 0.2

    surf_dist = np.full(n_ep, np.inf)
    if surface_tree is not None and embed_ok.any():
        idx = np.nonzero(embed_ok)[0]
        dd, _ = surface_tree.query(pts[idx])
        surf_dist[idx] = dd
    attached_c = embed_ok & (surf_dist <= cfg.contact_depth_mm + 0.45)

    attached = attached_a | attached_b | attached_c
    plate_gap = np.abs(z - plate_z)
    gap_candidates = np.stack([best_axis_dist, plate_gap,
                                np.where(embed_ok, surf_dist, np.inf)], axis=1)
    gap_mm_arr = gap_candidates.min(axis=1)

    violations = []
    for k in np.nonzero(~attached)[0]:
        mi = int(ep_mi[k])
        gap = float(gap_mm_arr[k])
        violations.append(dict(kind=members[mi]["kind"],
                                endpoint=np.asarray(pts[k], float).tolist(),
                                gap_mm=(gap if np.isfinite(gap) else None)))
    return violations


def self_verify(mesh, islands, contacts, members, carried_area_mm2, tip_r_mm, cfg,
                 plate_z, surface_tree=None, minima=None, forest=None, unrouted=(),
                 gate_on_removal=False, island_births=None):
    """Full design-v4.md #11 gate. Returns a report dict; `ok` is True
    only if all checks are clean: coverage, clearance, load, layer-island
    minima (only when the caller supplies `minima` from
    perception.local_z_minima; engine.run always does), the v4 p6
    slice-based island-birth gate (only when the caller supplies
    `island_births` from perception.island_births; engine.run always
    does), connectivity, and -- new in v4 -- per-segment printability and
    per-junction Murray conservation.

    `members` replaces v3's conn_parts=(routes,trunks,risers,braces,
    plate_z): one flat member list (tree.Member objects or dicts) drives
    clearance_report, connectivity_report, AND printability_report (p6-
    reach consistency fix -- see below). `forest`, if given (a tree.
    Forest -- engine.run always supplies one in real runs, same optional
    pattern as `minima`/`island_births`/`surface_tree`), additionally
    gates printability and Murray conservation, and computes the
    design-v4.md #10 removal_report (`unrouted`, the caller's honestly-
    unrouted contact-index list, feeds forest_trees so a never-built stub
    isn't reported as its own unremovable singleton tree); omitting
    `forest` skips all three (reported as empty/None) rather than gating
    on them.

    p6-reach consistency fix (verifier-authorized, one line): printability
    used to run over the RAW `forest.members` (every member this Forest
    has ever appended, including tree.Forest.dead retirees -- rewire.py's
    twig-prune/degenerate-retire moves mark a member dead rather than
    delete it, by design, so checkpoint/rollback indices stay stable) --
    the ONE check in this function not built on the caller-filtered,
    dead-excluding `members` list every sibling check (clearance,
    connectivity) already uses. That was a consistency bug, not extra
    gate strictness: a dead member is never emitted or printed (emission
    consumes engine.live_member_lists' own filtered output, the same
    live set `members` is built from), so a printability rule checking
    dead geometry was gating on something that will never actually print
    -- the gate's own referent is what gets printed. Fixed by running
    printability_report over `members` (the SAME filtered basis as its
    sibling checks) instead of `forest.members` -- `forest` is still
    required to gate this sub-report at all (per the docstring above),
    just no longer the SOURCE of what gets checked.

    `gate_on_removal` (Phase 5 wiring decision, see the p5-removal commit
    message; amended 2026-07-22): the charter (#10) originally called the
    removal gate's hard-failure case -- a residual set no removal order
    clears -- "a BLOCKER gate, wired into verify ok". Craig's 2026-07-22
    ruling (design-v4.md #10 amendment) supersedes that language:
    removal_report is diagnostic/report-only, removability is the least
    important optimization axis, and nothing it finds -- caged sets
    included -- may block an otherwise-superior layout. Per that ruling,
    removal_report/removal_violations are ALWAYS computed and surfaced
    (measurement only); gate_on_removal stays an inert toggle no shipped
    caller sets (setting it True would still AND removal_violations into
    `ok` mechanically, but doing so now runs against the charter, not
    with it). Defaults False so no existing caller's acceptance behavior
    changes by importing this function. `island_births`'s gate is NOT
    inert: per Craig's strict I3 ruling (see island_birth_report), an
    uncovered slice-detected island birth DOES fail `ok` -- removal is a
    soft preference axis, island-birth coverage is not."""
    coverage = coverage_report(mesh, islands, contacts, cfg)
    clearance = clearance_report(mesh, members, cfg, surface_tree=surface_tree)
    load = load_report(carried_area_mm2, tip_r_mm, cfg)
    unsupported_minima = minima_report(minima, contacts, cfg) if minima is not None else []
    uncovered_island_births = island_birth_report(island_births, contacts, cfg) \
        if island_births is not None else []
    connectivity_violations = connectivity_report(members, plate_z, cfg,
                                                   surface_tree=surface_tree)
    if forest is not None:
        printability_violations = tree_mod.printability_report(members, cfg)
        murray_violations = murray_report(forest, cfg)
        removal = removal_mod.removal_report(forest, mesh, surface_tree, cfg, unrouted=unrouted)
    else:
        printability_violations = []
        murray_violations = []
        removal = None
    removal_violations = list(removal["blocked_trees"]) if removal is not None else []
    uncovered = [c for c in coverage if not c["covered"]]
    ok = (len(uncovered) == 0 and len(clearance) == 0 and len(load) == 0
          and len(unsupported_minima) == 0 and len(uncovered_island_births) == 0
          and len(connectivity_violations) == 0
          and len(printability_violations) == 0 and len(murray_violations) == 0)
    if gate_on_removal:
        ok = ok and len(removal_violations) == 0
    return dict(
        ok=ok,
        uncovered_islands=uncovered,
        clearance_violations=clearance,
        load_violations=load,
        unsupported_minima=unsupported_minima,
        uncovered_island_births=uncovered_island_births,
        connectivity_violations=connectivity_violations,
        printability_violations=printability_violations,
        murray_violations=murray_violations,
        removal_report=removal,
        removal_violations=removal_violations,
    )


def orientation_scar_cost(mesh, cfg, rotations, delicacy=None):
    """Reusable scar-cost orientation scorer (design.md requirement 3).
    `rotations` is a list of 3x3 rotation matrices (candidate print
    orientations, applied to the AS-IS mesh frame). For each, scores
    sum(face_area * delicacy) over faces that would be down-facing under
    that rotation -- lower is better (less delicate surface exposed to
    support-facing down/side orientation).

    `delicacy` may be a precomputed (points, normals, weights, query) tuple
    from perception.delicacy_field(); computed fresh if not given (a
    caller evaluating many rotations should compute it once and pass it
    in -- it does not depend on rotation).
    """
    if delicacy is None:
        delicacy = perception.delicacy_field(mesh, cfg)
    _, _, _, query = delicacy
    centers = mesh.triangles_center
    areas = mesh.area_faces
    face_delicacy = query(centers)
    normals = mesh.face_normals
    scores = []
    thresh = -np.sin(np.radians(cfg.overhang_angle_deg))
    for R in rotations:
        rn = normals @ np.asarray(R).T
        down = rn[:, 2] < thresh
        scores.append(float(np.sum(areas[down] * face_delicacy[down])))
    return scores
