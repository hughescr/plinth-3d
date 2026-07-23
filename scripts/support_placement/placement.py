"""Candidate sampling, load partition, and physics-based tip/shaft sizing.

Implements design.md work-list items #2 (density calibration) and #3
(load partition -> continuous tip/shaft sizing). See config.py for the
mechanics constants and their provenance.
"""
import numpy as np
from scipy.spatial import cKDTree


def sample_candidates(mesh, islands, cfg, seed=0):
    """Greedy blue-noise contact sampling per island: dense sample the
    island's surface, then keep points no closer than
    cfg.target_spacing_mm apart (never closer than cfg.spacing_floor_mm),
    prioritizing lower-z (higher load share, closer to plate -- design.md:
    "bottom quarter of the model carries 40% of all tips") points first so
    coverage concentrates where the corpus shows it's actually needed.
    Islands smaller than the spacing floor still get
    cfg.min_contacts_per_island contacts.

    Returns a list of dicts: {position, normal, island_id, face_id}.
    """
    # trimesh's Trimesh.sample() draws from the GLOBAL numpy random state,
    # not a passed-in Generator -- seed it explicitly here so `seed` is
    # actually reproducible (otherwise every call, including repeated
    # calls within one engine.run fix-pass loop, silently drew different
    # candidate points). Deterministic batch tool, not concurrent, so a
    # global seed is safe.
    np.random.seed(seed)
    contacts = []
    for island_id, face_ids in enumerate(islands):
        if len(face_ids) == 0:
            continue
        sub = mesh.submesh([face_ids], append=True)
        area = float(sub.area)
        n_dense = int(np.clip(area / (cfg.spacing_floor_mm ** 2 * 0.3), 20, 20000))
        pts, sub_face_idx = sub.sample(n_dense, return_index=True)
        normals = sub.face_normals[sub_face_idx]
        order = np.argsort(pts[:, 2])  # lowest z first
        pts, normals = pts[order], normals[order]

        chosen_idx = []
        chosen_pts = np.empty((0, 3))
        for i in range(len(pts)):
            p = pts[i]
            if len(chosen_pts):
                d = np.linalg.norm(chosen_pts - p, axis=1).min()
                if d < cfg.target_spacing_mm:
                    continue
            chosen_idx.append(i)
            chosen_pts = np.vstack([chosen_pts, p])

        if len(chosen_idx) < cfg.min_contacts_per_island:
            # force coverage on small/thin islands: fall back to farthest-
            # point selection down to the spacing floor instead of the
            # (too coarse for this island) target spacing.
            chosen_idx = _farthest_point_fallback(
                pts, cfg.min_contacts_per_island, cfg.spacing_floor_mm)

        for i in chosen_idx:
            contacts.append(dict(
                position=pts[i], normal=normals[i],
                island_id=island_id, area_mm2=area))
    return contacts


def _farthest_point_fallback(pts, k, floor_mm):
    k = min(k, len(pts))
    if k <= 0:
        return []
    chosen = [0]
    d = np.linalg.norm(pts - pts[0], axis=1)
    while len(chosen) < k:
        nxt = int(np.argmax(d))
        chosen.append(nxt)
        d = np.minimum(d, np.linalg.norm(pts - pts[nxt], axis=1))
    return chosen


def load_partition(mesh, islands, contacts, cfg):
    """Exact, deterministic per-face load partition (SPEC-V5-GEOM sec 2.5).

    Replaces the seed-dependent Monte-Carlo `sub.sample` Voronoi partition
    (which made carried areas -- hence tip radii, hence member volumes --
    depend on the RNG seed and on whatever perturbed the global numpy state
    before it). Each island face's CENTROID is assigned to its nearest
    contact (a cKDTree over that island's contacts), and the face's FULL
    `mesh.area_faces` value accrues to that contact. No RNG, no sampling:
    exact at face granularity (mesh faces are small vs cfg.target_spacing_mm
    on this corpus, so per-face assignment is a faithful Voronoi partition
    of the island's own area). Per-island carried areas sum EXACTLY to the
    island's face-area total by construction. Returns an array of
    carried_area_mm2 aligned with `contacts`."""
    carried = np.zeros(len(contacts))
    by_island = {}
    for i, c in enumerate(contacts):
        by_island.setdefault(c["island_id"], []).append(i)

    centers = mesh.triangles_center
    face_area = mesh.area_faces
    for island_id, idxs in by_island.items():
        face_ids = islands[island_id]
        if len(face_ids) == 0:
            continue
        if len(idxs) == 1:
            carried[idxs[0]] = float(face_area[face_ids].sum())
            continue
        positions = np.array([contacts[i]["position"] for i in idxs])
        tree = cKDTree(positions)
        # nearest contact per island face centroid -- exact & deterministic
        # (NN assignment is independent of `workers`; ties break by index).
        _, nearest = tree.query(centers[face_ids], workers=-1)
        areas = np.asarray(face_area[face_ids], float)
        for j, i in enumerate(idxs):
            carried[i] = float(areas[nearest == j].sum())
    return carried


def tip_radius_mm(carried_area_mm2, cfg):
    """Green-state tensile fuse sizing (design.md mechanics framework):
    r = sqrt(F * SF / (pi * tensile_strength)), clamped to the
    removal-friendly [tip_r_min_mm, tip_r_max_mm] band."""
    area_m2 = np.asarray(carried_area_mm2) * 1e-6
    force_n = area_m2 * cfg.peel_pressure_pa
    tensile_pa = cfg.tensile_green_mpa * 1e6
    r_m = np.sqrt(np.maximum(force_n, 0) * cfg.tip_safety_factor / (np.pi * tensile_pa))
    r_mm = r_m * 1000.0
    return np.clip(r_mm, cfg.tip_r_min_mm, cfg.tip_r_max_mm)


def _peel_tan(angle_from_vertical_deg):
    return float(np.tan(np.radians(angle_from_vertical_deg))) \
        if angle_from_vertical_deg > 1e-9 else 0.0


def _shaft_r_for_L(L_mm, cfg, angle_from_vertical_deg=0.0):
    """Unclamped cantilever point-load deflection solve:
    delta = 4*F*L^3/(3*E*pi*r^4)  =>  r = (4*F*L^3/(3*E*pi*delta))^0.25
    (design.md: "deflection ~ L^3/r^4"). All SI internally.

    v3.6 angle-aware load term (Craig, queue item 2): an inclined member
    presents an elliptical layer cross-section of area pi*r^2/cos(theta)
    to the peel; resolving that peel normal load onto the member's
    lateral (bending) axis multiplies by sin(theta):
        F_eff = F_drag + peel_pressure * (pi*r^2/cos t) * sin t
              = F_drag + peel_pressure * pi * r^2 * tan(theta).
    F_eff depends on r, so the solve is a fixed-point iteration -- it
    contracts fast (r ~ F^(1/4), F ~ r^2 => net exponent 1/2 < 1).
    Vertical members (theta=0) reduce exactly to the old closed form."""
    L_m = max(L_mm, 1e-6) / 1000.0
    E = cfg.modulus_green_gpa * 1e9
    delta = cfg.max_deflection_mm / 1000.0
    tan_t = _peel_tan(angle_from_vertical_deg)
    geom = 4 * L_m ** 3 / (3 * E * np.pi * delta)
    r_m = (geom * cfg.shaft_drag_force_n) ** 0.25
    for _ in range(6 if tan_t else 0):
        F = cfg.shaft_drag_force_n + cfg.peel_pressure_pa * np.pi * r_m ** 2 * tan_t
        r_m = (geom * F) ** 0.25
    return max(r_m * 1000.0, cfg.min_shaft_r_mm)


def max_unbraced_len_mm(r_mm, cfg, angle_from_vertical_deg=0.0):
    """Inverse of _shaft_r_for_L: the longest free cantilever a member of
    radius r may run while meeting the deflection budget. Used to decide
    where a long member needs bracing rungs (v3.4: brace-then-thin).
    With radius KNOWN the angle-aware load is closed-form (no iteration):
    F_eff = F_drag + peel * pi * r^2 * tan(theta)."""
    r_m = max(r_mm, 1e-6) / 1000.0
    E = cfg.modulus_green_gpa * 1e9
    delta = cfg.max_deflection_mm / 1000.0
    F = cfg.shaft_drag_force_n \
        + cfg.peel_pressure_pa * np.pi * r_m ** 2 * _peel_tan(angle_from_vertical_deg)
    L_m = (r_m ** 4 * 3 * E * np.pi * delta / (4 * F)) ** (1 / 3)
    return L_m * 1000.0


def shaft_radius_for_length_mm(L_mm, cfg):
    """Slenderness-capped shaft sizing (design.md #5 trigger): if the
    stiffness-required radius for the FULL free length would be
    "ugly-thick" (> cfg.max_unbraced_shaft_r_mm), add braces to shorten
    the effective free length instead of thickening further -- "halving
    free length cuts deflection 8x" (L^3 term). Returns
    (radius_mm, n_braces, effective_free_length_mm)."""
    r = _shaft_r_for_L(L_mm, cfg)
    if r <= cfg.max_unbraced_shaft_r_mm or L_mm <= 0:
        return round(r, 3), 0, L_mm
    # Find the segment count n+1 (n braces) such that L/(n+1) brings the
    # required radius back under the unbraced cap.
    n_braces = 0
    seg = L_mm
    while True:
        n_braces += 1
        seg = L_mm / (n_braces + 1)
        r = _shaft_r_for_L(seg, cfg)
        if r <= cfg.max_unbraced_shaft_r_mm or n_braces > 50:
            break
    return round(r, 3), n_braces, seg
