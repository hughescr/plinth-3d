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

from . import perception, routing


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


def self_verify(mesh, islands, contacts, routes, carried_area_mm2, tip_r_mm, cfg,
                 surface_tree=None):
    """Full design.md #9 gate. Returns a report dict; `ok` is True only if
    all three checks are clean."""
    coverage = coverage_report(mesh, islands, contacts, cfg)
    clearance = clearance_report(mesh, routes, cfg, surface_tree=surface_tree)
    load = load_report(carried_area_mm2, tip_r_mm, cfg)
    uncovered = [c for c in coverage if not c["covered"]]
    return dict(
        ok=(len(uncovered) == 0 and len(clearance) == 0 and len(load) == 0),
        uncovered_islands=uncovered,
        clearance_violations=clearance,
        load_violations=load,
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
