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

from . import perception, routing, joints


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


def clearance_report(mesh, routes, cfg, surface_tree=None):
    """Flattened clearance violations across all routes (routing.py sweeps
    the full capsule envelope, not the v0 centerline-only check). Pass a
    routing.build_surface_tree(mesh) for real-model-scale runs -- see its
    docstring; the exact per-call path does not scale to hundreds of
    routes on a large mesh."""
    out = []
    all_violations = routing.check_clearance_batch(mesh, routes, cfg,
                                                    surface_tree=surface_tree)
    for i, violations in enumerate(all_violations):
        for v in violations:
            v["route_idx"] = i
            out.append(v)
    return out


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


def connectivity_report(routes, trunks, risers, braces, plate_z, cfg, surface_tree=None):
    """v3.7 hard gate (Craig's Chitubox review): the stale-brace-leak and
    chained-Y-branch defects both manifested the same way in the emitted
    file -- a member with an endpoint touching NOTHING (a floating rod).
    This makes "nothing floats" a checked invariant instead of a hope.

    Every member (joints.collect_members: stubs, shoots/struts, trunk
    columns, y-diagonals, risers, braces/rungs) has two endpoints; each
    must be ATTACHED by at least one of:
      (a) within 0.25mm of ANOTHER member's axis segment (point-to-segment
          distance, self excluded) -- the general "something is really
          there" rule, covering joints, mid-column brace/riser landings,
          and sibling-shoot rung targets;
      (b) plate level (abs(z - plate_z) <= 0.2mm) -- e.g. a plate trunk's
          column base. Column bases are widened to
          [plate_z, plate_z + cfg.flare_len_mm + 0.2] since the flare/pad
          below the column's own base_z is an emission-only detail this
          check doesn't model; this widening only ever applies to a
          column's OWN base endpoint, so it can't paper over a genuinely
          floating branch base (those sit well above plate_z by the
          coalesce_columns gates and get caught by rule (a) against their
          own y_diagonal instead);
      (c) embedded in the model surface (within cfg.contact_depth_mm+0.45)
          -- ONLY for the specific endpoints that are DESIGNED to embed:
          a stub's tip (p0), a model-anchored strut's foot (p1), a
          trunk-to-model rung's model end (p1), and a shoot-thinning
          rung's (i=j=-3) far end (p1, which may land on a trunk, a
          sibling shoot, or the model -- rule (a) already covers the first
          two, this only adds the model case). Any OTHER endpoint merely
          being near the model is explicitly NOT attachment -- that would
          silently wave through an unbraced floating rod that happens to
          pass close to the mesh.

    Returns a list of violation dicts {kind, endpoint (xyz list), gap_mm}
    -- gap_mm is the distance to the nearest legitimate attachment of the
    allowed kinds for that endpoint. Vectorized (member-axis distances
    computed in point-chunks against all member axes at once) to stay well
    under a second at real-model scale (~1350 members / 2700 endpoints).
    """
    members = joints.collect_members(routes, trunks, risers, braces, plate_z)
    if not members:
        return []

    brace_list = list(braces or [])
    n_members = len(members)
    m_p0 = np.array([m["p0"] for m in members], dtype=float)
    m_p1 = np.array([m["p1"] for m in members], dtype=float)
    d_axis = m_p1 - m_p0
    L2 = np.einsum("ij,ij->i", d_axis, d_axis)
    L2safe = np.where(L2 < 1e-18, 1.0, L2)
    zero_len = L2 < 1e-18

    # endpoints: (member_idx, point, embed_eligible)
    ep_mi, ep_pts, ep_embed, ep_kind = [], [], [], []
    for mi, m in enumerate(members):
        kind = m["kind"]
        owner_kind, owner_idx = m["owner"]
        embed0 = kind == "stub"                     # tip contact, p0
        embed1 = kind in ("strut", "model_rung")     # model foot / rung end, p1
        if owner_kind == "brace":
            b = brace_list[owner_idx]
            if b.get("i") == -3 and b.get("j") == -3:
                # shoot-thinning rung: far end (p1) may land on a trunk,
                # sibling shoot (both caught by rule (a) already) or the
                # model (needs rule (c)) -- widen eligibility, harmless
                # for the trunk/shoot cases since (a) alone already passes.
                embed1 = True
        ep_mi.append(mi); ep_pts.append(m["p0"]); ep_embed.append(embed0)
        ep_kind.append(kind)
        ep_mi.append(mi); ep_pts.append(m["p1"]); ep_embed.append(embed1)
        ep_kind.append(kind)

    ep_mi = np.array(ep_mi)
    pts = np.array(ep_pts, dtype=float)
    embed_ok = np.array(ep_embed, dtype=bool)
    # every member contributes p0 then p1 in order, so even flat-index ->
    # p0 (a column's base) and odd -> p1 (its top, not eligible here).
    is_col_base = np.array([members[mi]["kind"] == "column" and (idx % 2 == 0)
                             for idx, mi in enumerate(ep_mi)])

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
    attached_b = (np.abs(z - plate_z) <= 0.2) | \
        (is_col_base & (z >= plate_z - 1e-6) & (z <= plate_z + cfg.flare_len_mm + 0.2))

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


def self_verify(mesh, islands, contacts, routes, carried_area_mm2, tip_r_mm, cfg,
                 surface_tree=None, minima=None, conn_parts=None):
    """Full design.md #9 gate. Returns a report dict; `ok` is True only if
    all checks are clean (coverage, clearance, load, layer-island minima --
    only when the caller supplies the minima list from
    perception.local_z_minima; engine.run always does -- and, since v3.7,
    connectivity).

    conn_parts, if given, is (routes, trunks, risers, braces, plate_z) --
    the RAW parts exactly as emission sees them (not the routes+pseudo
    blend `routes` above uses for clearance sweeping). None skips the
    connectivity gate entirely (backward compatible, same optional
    pattern as `minima`)."""
    coverage = coverage_report(mesh, islands, contacts, cfg)
    clearance = clearance_report(mesh, routes, cfg, surface_tree=surface_tree)
    load = load_report(carried_area_mm2, tip_r_mm, cfg)
    unsupported_minima = minima_report(minima, contacts, cfg) if minima is not None else []
    if conn_parts is not None:
        c_routes, c_trunks, c_risers, c_braces, c_plate_z = conn_parts
        connectivity_violations = connectivity_report(
            c_routes, c_trunks, c_risers, c_braces, c_plate_z, cfg,
            surface_tree=surface_tree)
    else:
        connectivity_violations = []
    uncovered = [c for c in coverage if not c["covered"]]
    return dict(
        ok=(len(uncovered) == 0 and len(clearance) == 0 and len(load) == 0
            and len(unsupported_minima) == 0 and len(connectivity_violations) == 0),
        uncovered_islands=uncovered,
        clearance_violations=clearance,
        load_violations=load,
        unsupported_minima=unsupported_minima,
        connectivity_violations=connectivity_violations,
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
