"""A5 internal-cavity demand, routing, and independent verification.

The exterior engine intentionally excludes inner-wall faces.  This module
therefore derives every demand from hollowing's exact post-refill cavity
voxel authority and never presents an internal member to the exterior
connectivity verifier.  The pipeline calls this only after the exterior
result has passed, then appends the independently-gated members.
"""
import hashlib
import json

import numpy as np
from scipy import ndimage
from scipy.spatial import cKDTree
import trimesh

from . import placement
from . import topology


SCHEMA = "support-placement/a5-internal"
SCHEMA_VERSION = 1


def _hash_arrays(parts):
    h = hashlib.sha256()
    for name, value, dtype in parts:
        arr = np.ascontiguousarray(np.asarray(value, dtype=dtype))
        h.update(name.encode("ascii"))
        h.update(np.asarray(arr.shape, dtype="<i8").tobytes())
        h.update(arr.tobytes())
    return h.hexdigest()


def _failure(code, detail, *, provenance=None):
    report = {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "ok": False,
        "fallback_required": True,
        "reason_code": str(code),
        "reason": str(detail),
        "members": [],
        "gates": {
            "ok": False,
            "descriptor_violations": [],
            "endpoint_violations": [],
            "containment_violations": [],
            "clearance_violations": [],
            "keepout_violations": [],
            "uncovered_overhangs": [],
            "uncovered_minima": [],
            "uncovered_births": [],
            "uncovered_a3": [],
            "unsupported_lobes": [],
            "provenance_violations": [],
        },
    }
    if provenance:
        report["provenance"] = provenance
    return report


def _cavity_fingerprint(mask, transform, inverse_transform):
    return _hash_arrays((
        ("cavity_mask", np.packbits(mask, bitorder="little"), np.uint8),
        ("transform", transform, "<f8"),
        ("inverse_transform", inverse_transform, "<f8"),
    ))


def _keepout_mask(shape, keepouts, inverse_transform, pitch, extra_radius):
    out = np.zeros(shape, dtype=bool)
    for keepout in keepouts:
        p0 = np.asarray(keepout["p0"], dtype=float)
        p1 = np.asarray(keepout["p1"], dtype=float)
        length = float(np.linalg.norm(p1 - p0))
        n = max(2, int(np.ceil(length / (pitch * 0.5))) + 1)
        t = np.linspace(0.0, 1.0, n)
        points = p0[None, :] + t[:, None] * (p1 - p0)[None, :]
        ijk = trimesh.transformations.transform_points(
            points, inverse_transform).round().astype(np.int64)
        inside = ((ijk >= 0) & (ijk < np.asarray(shape))).all(axis=1)
        ijk = ijk[inside]
        if len(ijk) == 0:
            continue
        seeds = np.zeros(shape, dtype=bool)
        seeds[tuple(ijk.T)] = True
        radius = float(keepout["radius_mm"]) + float(extra_radius)
        cells = int(np.ceil(radius / pitch))
        q = np.arange(-cells, cells + 1)
        xx, yy, zz = np.meshgrid(q, q, q, indexing="ij")
        ball = xx * xx + yy * yy + zz * zz <= cells * cells
        out |= ndimage.binary_dilation(seeds, structure=ball)
    return out


def _disk_for_swept_radius(radius, pitch):
    cells = int(np.ceil(float(radius) / pitch))
    q = np.arange(-cells, cells + 1)
    xx, yy = np.meshgrid(q, q, indexing="ij")
    # Include every voxel cell the continuous disk could touch, not only
    # cells whose centers land inside the nominal radius.
    cover = float(radius) + pitch / np.sqrt(2.0)
    return (xx * pitch) ** 2 + (yy * pitch) ** 2 <= cover ** 2


def _swept_containment(mask, route_voxels, lower_z, radii, pitch):
    """Full vertical-cylinder containment for each route's cavity-side span.

    Only the explicit contact-depth extensions beyond the upper/lower
    boundary planes are absent from this voxel span.  No approach-band
    exemption exists here: a sloped or narrowed wall that clips the emitted
    radius is rejected even when the centerline itself remains in the void.
    """
    clean = np.zeros(len(route_voxels), dtype=bool)
    # Radius is continuous, but the conservative disk changes only when its
    # touched-cell footprint changes.  Group exact footprints deterministically.
    groups = {}
    for i, radius in enumerate(np.asarray(radii, dtype=float)):
        disk = _disk_for_swept_radius(radius, pitch)
        key = disk.shape + (disk.tobytes(),)
        groups.setdefault(key, (disk, []))[1].append(i)
    for disk, indices in groups.values():
        structure = disk[:, :, None]
        eroded = ndimage.binary_erosion(
            mask, structure=structure, border_value=0)
        bad_prefix = np.cumsum(~eroded, axis=2, dtype=np.int32)
        ii = np.asarray(indices, dtype=np.int64)
        valid = lower_z[ii] >= 0
        if not valid.any():
            continue
        ii = ii[valid]
        counts = _prefix_interval_count(
            bad_prefix, route_voxels[ii], lower_z[ii],
            route_voxels[ii, 2])
        clean[ii] = counts == 0
    return clean


def _prefix_interval_count(prefix, xyz, starts, ends):
    x, y = xyz[:, 0], xyz[:, 1]
    count = prefix[x, y, ends].astype(np.int64)
    prior = starts - 1
    use = prior >= 0
    count[use] -= prefix[x[use], y[use], prior[use]]
    return count


def _select_supports(demand_points, candidate_points, demand_components,
                     candidate_components, target_spacing):
    selected = []
    assignments = np.full(len(demand_points), -1, dtype=np.int64)
    unsupported = []
    cell = float(target_spacing) / np.sqrt(3.0)
    for component in np.unique(demand_components):
        di = np.nonzero(demand_components == component)[0]
        ci = np.nonzero(candidate_components == component)[0]
        if len(ci) == 0:
            unsupported.append(int(component))
            continue
        keys = np.floor(candidate_points[ci] / cell).astype(np.int64)
        order = np.lexsort((
            candidate_points[ci, 2],
            candidate_points[ci, 1],
            candidate_points[ci, 0],
            keys[:, 2], keys[:, 1], keys[:, 0],
        ))
        sorted_ci = ci[order]
        sorted_keys = keys[order]
        first = np.r_[True, np.any(sorted_keys[1:] != sorted_keys[:-1], axis=1)]
        chosen = sorted_ci[first]

        tree = cKDTree(candidate_points[chosen])
        distance, nearest = tree.query(
            demand_points[di], distance_upper_bound=float(target_spacing))
        missed = ~np.isfinite(distance)
        if missed.any():
            all_tree = cKDTree(candidate_points[ci])
            d2, n2 = all_tree.query(
                demand_points[di[missed]],
                distance_upper_bound=float(target_spacing))
            if (~np.isfinite(d2)).any():
                unsupported.append(int(component))
                continue
            chosen = np.unique(np.r_[chosen, ci[n2]])
            tree = cKDTree(candidate_points[chosen])
            distance, nearest = tree.query(
                demand_points[di], distance_upper_bound=float(target_spacing))
        if (~np.isfinite(distance)).any():
            unsupported.append(int(component))
            continue
        base = len(selected)
        selected.extend(chosen.tolist())
        assignments[di] = base + nearest
    return np.asarray(selected, dtype=np.int64), assignments, sorted(set(unsupported))


def plan_and_verify(*, mesh, result, cfg, hollow_report, cavity,
                    mesh_center):
    """Return a JSON-safe report plus runtime-only ``members``.

    The only route primitive admitted by this MVP is a near-vertical column
    from a broad inner ceiling patch to the lower wall of the *same* vertical
    cavity run.  Refusal is intentional for lobes that need a more general
    tree: the pipeline then uses the preserved oriented solid.
    """
    provenance = {
        "source": "hollow.post_refill_main_mask",
        "demand_source": "authoritative_cavity_boundary",
        "exterior_engine_covers_internal": False,
        "exterior_verify_ok_before_a5": bool(
            (result.get("verify") or {}).get("ok", False)),
    }
    if not isinstance(cavity, dict) or cavity.get("schema") != "support-placement/a5-cavity":
        return _failure("missing_cavity_authority",
                        "hollowing did not provide its post-refill cavity sidecar",
                        provenance=provenance)

    mask = np.asarray(cavity.get("cavity_mask"), dtype=bool)
    solid = np.asarray(cavity.get("solid_mask"), dtype=bool)
    transform = np.asarray(cavity.get("transform"), dtype=float)
    inverse_transform = np.asarray(cavity.get("inverse_transform"), dtype=float)
    pitch = float(cavity.get("pitch_mm", 0.0))
    if (mask.ndim != 3 or solid.shape != mask.shape or transform.shape != (4, 4)
            or inverse_transform.shape != (4, 4) or pitch <= 0.0):
        return _failure("invalid_cavity_authority",
                        "cavity sidecar shapes/transforms are invalid",
                        provenance=provenance)
    got_cavity_fp = _cavity_fingerprint(mask, transform, inverse_transform)
    if got_cavity_fp != cavity.get("cavity_fingerprint"):
        return _failure("cavity_fingerprint_mismatch",
                        "runtime cavity mask no longer matches hollowing provenance",
                        provenance=provenance)
    descriptor = hollow_report.get("cavity_descriptor") or {}
    if (descriptor.get("cavity_fingerprint") != got_cavity_fp
            or descriptor.get("descriptor_fingerprint")
            != cavity.get("descriptor_fingerprint")):
        return _failure("descriptor_provenance_mismatch",
                        "reader-facing and runtime cavity provenance disagree",
                        provenance=provenance)

    centered_transform = np.array(transform, copy=True)
    center_shift = np.eye(4)
    center_shift[:3, 3] = -np.asarray(mesh_center, dtype=float)
    centered_transform = center_shift @ centered_transform
    centered_inverse = np.linalg.inv(centered_transform)
    mesh_fp = topology.mesh_fingerprint(mesh)
    centered_descriptor_fp = _hash_arrays((
        ("centered_transform", centered_transform, "<f8"),
        ("mesh_center", mesh_center, "<f8"),
    ))
    provenance.update(
        cavity_fingerprint=got_cavity_fp,
        cavity_descriptor_fingerprint=cavity["descriptor_fingerprint"],
        centered_descriptor_fingerprint=centered_descriptor_fp,
        hollow_mesh_fingerprint=mesh_fp,
        pitch_mm=pitch,
        voxel_shape=[int(v) for v in mask.shape],
    )

    upper = np.zeros_like(mask)
    lower = np.zeros_like(mask)
    upper[:, :, :-1] = mask[:, :, :-1] & solid[:, :, 1:]
    lower[:, :, 1:] = mask[:, :, 1:] & solid[:, :, :-1]
    cavity_distance = ndimage.distance_transform_edt(
        mask, sampling=(pitch, pitch, pitch))
    outside_distance = ndimage.distance_transform_edt(
        ~mask, sampling=(pitch, pitch, pitch))
    signed_distance = cavity_distance - outside_distance
    gradient = np.gradient(signed_distance, pitch, edge_order=1)
    gradient_norm = np.sqrt(sum(g * g for g in gradient))
    down_z = np.divide(
        gradient[2], gradient_norm,
        out=np.zeros_like(gradient[2]), where=gradient_norm > 1e-12)
    upper &= down_z < -np.sin(np.radians(float(cfg.overhang_angle_deg)))
    provenance["overhang_angle_deg"] = float(cfg.overhang_angle_deg)
    demand_idx = np.argwhere(upper)
    if len(demand_idx) == 0:
        return _failure("no_inner_ceiling_demand",
                        "hollow cavity has no authoritative solid-above boundary",
                        provenance=provenance)

    component_labels, n_components = ndimage.label(
        upper, structure=np.ones((3, 3, 3), dtype=bool))
    demand_components = component_labels[tuple(demand_idx.T)]
    z_field = np.broadcast_to(
        np.arange(mask.shape[2], dtype=np.int32), mask.shape)
    local_min = upper & (
        z_field == ndimage.minimum_filter(
            np.where(upper, z_field, np.iinfo(np.int32).max),
            size=(3, 3, 3), mode="constant",
            cval=np.iinfo(np.int32).max))
    component_min_z = ndimage.minimum(
        z_field, labels=component_labels,
        index=np.arange(1, n_components + 1))
    minimum_by_component = np.r_[
        np.iinfo(np.int32).max,
        np.asarray(component_min_z, dtype=np.int32),
    ]
    births = upper & (
        z_field == minimum_by_component[component_labels])
    minimum_flags = local_min[tuple(demand_idx.T)]
    birth_flags = births[tuple(demand_idx.T)]

    planning_radius = float(cfg.tip_r_min_mm)
    required_clearance = planning_radius + float(cfg.standoff_mm)
    lower_seed = np.where(lower, z_field, -1)
    lower_z_all = np.maximum.accumulate(lower_seed, axis=2)
    lower_z = lower_z_all[tuple(demand_idx.T)]
    has_lower = lower_z >= 0
    lower_probe = demand_idx.copy()
    lower_probe[:, 2] = np.maximum(lower_z, 0)
    valid_endpoints = (
        upper[tuple(demand_idx.T)]
        & lower[tuple(lower_probe.T)]
        & has_lower
    )

    band_cells = int(np.ceil(required_clearance / pitch))
    interior_start = lower_z + band_cells
    interior_end = demand_idx[:, 2] - band_cells
    has_interior = interior_start <= interior_end
    bad_clearance = mask & (
        cavity_distance - pitch * 0.5 < required_clearance)
    clearance_prefix = np.cumsum(bad_clearance, axis=2, dtype=np.int32)
    clearance_counts = np.ones(len(demand_idx), dtype=np.int64)
    q = has_lower & has_interior
    if q.any():
        clearance_counts[q] = _prefix_interval_count(
            clearance_prefix, demand_idx[q],
            interior_start[q], interior_end[q])

    keepout = _keepout_mask(
        mask.shape, cavity.get("drill_keepouts", ()), inverse_transform,
        pitch, planning_radius + float(cfg.standoff_mm))
    keepout_prefix = np.cumsum(keepout, axis=2, dtype=np.int32)
    keepout_counts = np.ones(len(demand_idx), dtype=np.int64)
    qr = has_lower
    if qr.any():
        keepout_counts[qr] = _prefix_interval_count(
            keepout_prefix, demand_idx[qr], lower_z[qr], demand_idx[qr, 2])

    swept_clean = _swept_containment(
        mask, demand_idx, lower_z,
        np.full(len(demand_idx), planning_radius), pitch)
    candidate_ok = (
        has_lower & has_interior & valid_endpoints
        & swept_clean & (clearance_counts == 0) & (keepout_counts == 0)
    )
    candidate_demand_idx = np.nonzero(candidate_ok)[0]
    provenance["route_candidate_counts"] = {
        "demands": int(len(demand_idx)),
        "same_run_lower_wall": int(has_lower.sum()),
        "valid_endpoint_pairs": int(valid_endpoints.sum()),
        "interior_band": int(has_interior.sum()),
        "swept_radius_contained": int(swept_clean.sum()),
        "clearance_clean": int((clearance_counts == 0).sum()),
        "keepout_clean": int((keepout_counts == 0).sum()),
        "all_constraints": int(len(candidate_demand_idx)),
    }
    if len(candidate_demand_idx) == 0:
        failed = _failure(
            "no_contained_internal_route",
            "no ceiling-to-lower-wall column clears shell and drains",
            provenance=provenance)
        failed["gates"]["containment_violations"] = [
            "no_safe_vertical_candidate"]
        failed.update(
            n_demands=int(len(demand_idx)),
            n_inner_lobes=int(n_components))
        return failed

    demand_points = trimesh.transformations.transform_points(
        demand_idx.astype(float), centered_transform)
    candidate_points = demand_points[candidate_demand_idx]
    candidate_components = demand_components[candidate_demand_idx]
    selected_local, assignments, unsupported = _select_supports(
        demand_points, candidate_points, demand_components,
        candidate_components, float(cfg.target_spacing_mm))
    if unsupported or (assignments < 0).any():
        failed = _failure(
            "unroutable_or_trapped_internal_lobe",
            f"{len(unsupported)} inner-ceiling lobe(s) have no safe route within "
            f"target spacing {float(cfg.target_spacing_mm):g}mm",
            provenance={**provenance, "unsupported_lobes": unsupported},
        )
        failed["gates"]["unsupported_lobes"] = unsupported
        failed.update(
            n_demands=int(len(demand_idx)),
            n_inner_lobes=int(n_components))
        return failed

    selected_demand_idx = candidate_demand_idx[selected_local]
    selected_voxels = demand_idx[selected_demand_idx]
    selected_lower_z = lower_z[selected_demand_idx]
    area = np.bincount(
        assignments, minlength=len(selected_demand_idx)).astype(float) * pitch ** 2
    radii = placement.tip_radius_mm(area, cfg)
    top_boundary = trimesh.transformations.transform_points(
        selected_voxels.astype(float), centered_transform)
    bottom_voxels = selected_voxels.copy()
    bottom_voxels[:, 2] = selected_lower_z
    bottom_boundary = trimesh.transformations.transform_points(
        bottom_voxels.astype(float), centered_transform)
    top_boundary[:, 2] += pitch * 0.5
    bottom_boundary[:, 2] -= pitch * 0.5
    p0 = top_boundary.copy()
    p1 = bottom_boundary.copy()
    p0[:, 2] += float(cfg.contact_depth_mm)
    p1[:, 2] -= float(cfg.contact_depth_mm)

    members = [{
        "p0": p0[i],
        "p1": p1[i],
        "r0": float(radii[i]),
        "r1": float(radii[i]),
        "owner": ("internal", int(i)),
        "kind": "internal",
        "embed_end": None,
    } for i in range(len(p0))]

    support_fp = _hash_arrays((
        ("p0", p0, "<f8"), ("p1", p1, "<f8"), ("radius", radii, "<f8"),
    ))
    demand_fp = _hash_arrays((
        ("demand_voxels", demand_idx, "<i8"),
        ("minimum_flags", minimum_flags, np.uint8),
        ("birth_flags", birth_flags, np.uint8),
    ))

    actual_swept_clean = _swept_containment(
        mask, selected_voxels, selected_lower_z, radii, pitch)
    actual_clearance_clean = np.zeros(len(selected_voxels), dtype=bool)
    for i, radius in enumerate(radii):
        band = int(np.ceil(
            (float(radius) + float(cfg.standoff_mm)) / pitch))
        start = int(selected_lower_z[i] + band)
        end = int(selected_voxels[i, 2] - band)
        if start <= end:
            x, y = selected_voxels[i, :2]
            actual_clearance_clean[i] = bool(
                np.min(cavity_distance[x, y, start:end + 1] - pitch * 0.5)
                >= float(radius) + float(cfg.standoff_mm))
    actual_keepout = _keepout_mask(
        mask.shape, cavity.get("drill_keepouts", ()), inverse_transform,
        pitch, float(radii.max()) + float(cfg.standoff_mm))
    actual_keepout_prefix = np.cumsum(
        actual_keepout, axis=2, dtype=np.int32)
    actual_keepout_counts = _prefix_interval_count(
        actual_keepout_prefix, selected_voxels, selected_lower_z,
        selected_voxels[:, 2])

    assigned_distance = np.linalg.norm(
        demand_points - candidate_points[selected_local[assignments]], axis=1)
    uncovered = np.nonzero(
        assigned_distance > float(cfg.target_spacing_mm) + 1e-9)[0]
    component_supported = np.unique(
        demand_components[assignments >= 0])
    unsupported_lobes = sorted(set(range(1, n_components + 1))
                               - set(component_supported.tolist()))
    endpoint_violations = np.nonzero(~valid_endpoints[selected_demand_idx])[0].tolist()
    containment_violations = np.nonzero(
        ~actual_swept_clean)[0].tolist()
    clearance_violations = np.nonzero(
        ~actual_clearance_clean)[0].tolist()
    keepout_violations = np.nonzero(
        actual_keepout_counts != 0)[0].tolist()
    uncovered_minima = np.nonzero(minimum_flags & np.isin(
        np.arange(len(demand_idx)), uncovered))[0].tolist()
    uncovered_births = np.nonzero(birth_flags & np.isin(
        np.arange(len(demand_idx)), uncovered))[0].tolist()
    provenance_violations = []
    if got_cavity_fp != cavity["cavity_fingerprint"]:
        provenance_violations.append("cavity_fingerprint")

    gates = {
        "descriptor_violations": [],
        "endpoint_violations": endpoint_violations,
        "containment_violations": containment_violations,
        "clearance_violations": clearance_violations,
        "keepout_violations": keepout_violations,
        "uncovered_overhangs": uncovered.tolist(),
        "uncovered_minima": uncovered_minima,
        "uncovered_births": uncovered_births,
        "uncovered_a3": uncovered.tolist(),
        "unsupported_lobes": unsupported_lobes,
        "provenance_violations": provenance_violations,
    }
    gates["ok"] = not any(gates.values())
    report = {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "ok": bool(gates["ok"]),
        "fallback_required": not bool(gates["ok"]),
        "reason_code": None if gates["ok"] else "a5_gate_failure",
        "reason": None if gates["ok"] else "one or more internal gates failed",
        "n_members": int(len(members)),
        "n_demands": int(len(demand_idx)),
        "n_overhang_demands": int(len(demand_idx)),
        "n_minimum_demands": int(minimum_flags.sum()),
        "n_birth_demands": int(birth_flags.sum()),
        "n_a3_demands": int(len(demand_idx)),
        "n_inner_lobes": int(n_components),
        "target_spacing_mm": float(cfg.target_spacing_mm),
        "max_coverage_distance_mm": float(assigned_distance.max(initial=0.0)),
        "radius_min_mm": float(radii.min()),
        "radius_max_mm": float(radii.max()),
        "demand_fingerprint": demand_fp,
        "support_fingerprint": support_fp,
        "provenance": provenance,
        "gates": gates,
        # Runtime-only.  Pipeline removes this before serializing the report.
        "members": members,
    }
    return report
