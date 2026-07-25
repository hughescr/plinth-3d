"""Supervised hollowing + drain drilling for resin printing. STL in, STL out.

This is the library form of the tool that used to live wholly inside
``scripts/tools/hollow_drill.py``. That file is still the operator CLI (and
still the path the ``prep-mini`` skill documents); it now parses argv, maps
:func:`run`'s ``status`` onto an exit code, and nothing else. The algorithm
below is unchanged -- no thresholds moved, no decisions changed -- so that
``support_placement.pipeline`` can compose the stage as a function call
instead of shelling out to a script and parsing its stdout.

Runtime: ~20s at 0.5mm pitch on a 500k-face model -- run it in the
FOREGROUND of a single shell call; do not background it in sandboxed
environments (reaped between calls). Note the orientation search changes
the bounding dims relative to the sized input (footprint scaling is
unaffected) -- expect the final dims report to differ from pre-hollow.

Needs no Blender; the dependencies are declared in the project's
``pyproject.toml`` (trimesh, scipy, scikit-image, manifold3d, shapely, and
optionally fast-simplification).

Pipeline (all steps verified on real prints, July 2026):
 1. Optional orientation search on a 10x-decimated proxy: +-30 deg sweeps
    about X/Y scored by 0.5*support_area_cm2 + 3*peak_crosssection_cm2 +
    0.3*height_mm. Orientation MUST be final before drilling -- hole
    placement is gravity-dependent. Model is re-rested on z=0.
 2. Hollow decision: voxelize 0.5mm -> fill -> EDT. Hollow only if max
    bulk thickness > 10mm AND a wall-thickness cavity saves >15ml or >25%
    of volume. Otherwise export solid (holes in small minis cost more
    than the resin they save).
 3. Cavity = the LARGEST connected eroded component ONLY. Small pockets
    stay solid -- blind hollowing (e.g. slicers') creates sealed resin
    traps; a real model showed 1 true cavity vs 21 junk pockets.
 4. Drains: one 3mm-dia hole per low lobe (cavity components below
    lowz+15mm, >0.5ml), at each lobe's lowest point, drilled along the
    shortest exit ray (mostly-downward candidates). One drain trapped
    14ml on a tree-shaped cavity; per-lobe drains cut it to 0.27ml.
 5. Vent: 2.5mm-dia hole near cavity top through the thinnest wall
    (horizontal candidates). A sealed cavity is a pressure vessel.
 6. Pooling audit: sweep z-cuts; cavity components below a cut that
    contain no drain = trapped resin. Reported so you can add drains.
 7. Booleans via manifold3d. Marching-cubes shells come out INSIDE-OUT:
    fix_normals before use, assert .is_volume on every boolean input.
 8. Verify: exactly 1 body, watertight, winding-consistent, volume ==
    solid - cavity (+-2%), and a flood-fill from outside must reach the
    cavity through the holes. Export only on pass.

THE STATUS CONTRACT (why this module reports rather than returns quietly)
------------------------------------------------------------------------
Step 8 can fail, and when it does **nothing is exported**. Until this was
split out, that outcome was reported only as a ``status`` key buried in a
JSON blob printed to stdout, and the CLI exited 0 anyway -- an operator (or a
batch loop, or the pipeline) saw a success and no output file. Every return
path from :func:`run` now sets ``log["status"]`` to one of :data:`STATUS_OK` /
:data:`STATUS_VERIFY_FAILED`, and the failing path additionally sets
``log["failed_checks"]`` (the named checks that did not hold) and
``log["check_detail"]`` (what each one measured). A legitimate SOLID decision
is a success: it exports the oriented mesh and reports ``STATUS_OK``.

Do NOT post-process the output to silence slicer "error detection":
sub-pixel seam/interior slivers are intrinsic and harmless (true
degenerates: zero), collapse-based cleanup fragmented a watertight body
into 41 pieces in testing, and slicer auto-repair may delete the cavity
shell ("excess shell") or seal the drains ("holes"). Validate in the
slice preview instead: two contours in hollow regions, open drains.
"""
import time

import numpy as np
import trimesh
from scipy import ndimage
from skimage import measure

PITCH = 0.5
BULK_MIN_MM = 10.0
SAVE_MIN_ML = 15.0
SAVE_MIN_FRAC = 0.25
LOBE_MIN_ML = 0.5
LOBE_BAND_MM = 15.0
DRAIN_R = 1.5
VENT_R = 1.25

#: Wrote ``out_path``. Either the model was hollowed and passed every check in
#: step 8, or it was legitimately decided SOLID and exported as-is (oriented).
STATUS_OK = "OK"
#: Hollowing produced a mesh that failed step 8. NOTHING was written.
STATUS_VERIFY_FAILED = "NOT_EXPORTED_verify_failed"

#: The step-8 checks, in report order, with the failure they exist to catch.
#: :func:`run` records each one's outcome under ``log["check_detail"]``; the
#: CLI and the pipeline quote these when a run fails.
CHECK_DESCRIPTIONS = {
    "single_body": "the result must be exactly one connected body",
    "watertight": "the result must be watertight (no boundary edges)",
    "winding_consistent": "face winding must be consistent (no flipped normals)",
    "volume_balance": "final volume must equal solid - cavity within 2% of solid",
    "cavity_breathes": "a flood-fill from outside must reach the cavity "
                       "(drains/vent actually pierce the wall)",
}


class HollowError(RuntimeError):
    """The hollow/drill stage could not produce a trustworthy mesh."""


def orient_search(m, log):
    try:
        import fast_simplification
        v, f = fast_simplification.simplify(
            m.vertices.astype(np.float32), m.faces.astype(np.int32), target_reduction=0.9)
        proxy = trimesh.Trimesh(vertices=v, faces=f)
    except Exception:
        proxy = m
    def score(mesh):
        sup = mesh.area_faces[mesh.face_normals[:, 2] < -0.707].sum() / 100.0
        zs = np.arange(mesh.bounds[0][2] + 0.1, mesh.bounds[1][2] - 0.1, 4.0)
        peak = 0.0
        for s in mesh.section_multiplane([0, 0, 0], [0, 0, 1], zs):
            if s:
                peak = max(peak, sum(p.area for p in s.polygons_closed if p is not None))
        h = mesh.bounds[1][2] - mesh.bounds[0][2]
        return 0.5 * sup + 3.0 * peak / 100.0 + 0.3 * h
    best = (score(proxy), np.eye(4))
    for axis in ([1, 0, 0], [0, 1, 0]):
        for deg in (-30, -20, -10, 10, 20, 30):
            R = trimesh.transformations.rotation_matrix(np.radians(deg), axis)
            p = proxy.copy(); p.apply_transform(R)
            s = score(p)
            if s < best[0]:
                best = (s, R)
                log["orientation"] = {"axis": axis, "deg": deg, "score": round(s, 1)}
    m.apply_transform(best[1])
    m.apply_translation([-(m.bounds[0][0] + m.bounds[1][0]) / 2,
                         -(m.bounds[0][1] + m.bounds[1][1]) / 2, -m.bounds[0][2]])
    return m


def failed_checks(log):
    """The named step-8 checks that did not hold, in :data:`CHECK_DESCRIPTIONS`
    order. Empty for a passing run, a SOLID decision, or a log with no checks
    (e.g. the run raised before step 8)."""
    detail = log.get("check_detail") or {}
    return [k for k in CHECK_DESCRIPTIONS if k in detail and not detail[k]["ok"]]


def describe_failure(log):
    """One human-readable line per failed check, naming the check, what it
    requires and what this run measured. Empty list when nothing failed."""
    detail = log.get("check_detail") or {}
    return [f"{k}: {CHECK_DESCRIPTIONS[k]} — measured {detail[k]['measured']}"
            for k in failed_checks(log)]


def run(in_path, out_path, wall=2.5, orient=True, log=None):
    """Hollow (or decide not to) and drill ``in_path``, writing ``out_path``.

    Returns the log dict (the same object if one was passed in), always
    carrying ``status``. ``status == STATUS_OK`` is the only outcome under
    which ``out_path`` was written; ``STATUS_VERIFY_FAILED`` additionally
    carries ``failed_checks`` and ``check_detail``. Raises for input problems
    (unreadable/non-volume mesh) rather than reporting them -- those are not
    outcomes of the algorithm, they are bad inputs.
    """
    log = {} if log is None else log
    t0 = time.time()
    m = trimesh.load(in_path); m.merge_vertices()
    if not m.is_volume:
        trimesh.repair.fix_normals(m)
    if not m.is_volume:
        raise HollowError(
            f"input is not a watertight volume: {in_path} (run prep-mini's "
            f"repair playbook first)")
    if orient:
        m = orient_search(m, log)
    log["oriented"] = bool(orient)
    solid_ml = m.volume / 1000.0
    log.update(solid_ml=round(solid_ml, 1), dims_mm=[round(x, 1) for x in m.extents])

    def _export_solid(decision):
        m.export(out_path)
        log.update(decision=decision, status=STATUS_OK, exported=out_path,
                   verify="N/A_solid", elapsed_s=round(time.time() - t0, 1))
        return log

    vg = m.voxelized(PITCH).fill()
    T = np.array(vg.transform); Tinv = np.linalg.inv(T); mat = vg.matrix
    edt = ndimage.distance_transform_edt(mat) * PITCH
    bulk = 2 * edt.max()
    cav = edt > wall
    lbl, n = ndimage.label(cav)
    if n == 0:
        log.update(bulk_mm=round(bulk, 1))
        return _export_solid("SOLID_no_cavity")
    sizes = ndimage.sum(cav, lbl, range(1, n + 1))
    main = lbl == (np.argmax(sizes) + 1)
    cavity_ml = main.sum() * PITCH ** 3 / 1000.0
    log.update(bulk_mm=round(bulk, 1), cavity_ml=round(cavity_ml, 1),
               junk_pockets_left_solid=int(n - 1))
    if bulk < BULK_MIN_MM or (cavity_ml < SAVE_MIN_ML and cavity_ml < SAVE_MIN_FRAC * solid_ml):
        return _export_solid("SOLID_below_threshold")
    log.update(decision="HOLLOW")

    idx = np.array(np.nonzero(main)).T
    world = trimesh.transformations.transform_points(idx.astype(float), T)
    zw = world[:, 2]; lowz = zw.min()

    def exit_ray(p, dirs):
        best = None
        for d in dirs:
            d = np.asarray(d, float); d /= np.linalg.norm(d)
            for r in np.arange(0, 45, PITCH):
                q = p + d * r
                ijk = trimesh.transformations.transform_points([q], Tinv)[0].round().astype(int)
                if (ijk < 0).any() or (ijk >= np.array(mat.shape)).any() or not mat[tuple(ijk)]:
                    if best is None or r < best[0]:
                        best = (r, d)
                    break
        return best

    # drains: one per low lobe
    below = np.zeros_like(main)
    sel = idx[zw < lowz + LOBE_BAND_MM]
    below[tuple(sel.T)] = True
    lb, nb = ndimage.label(below)
    szs = ndimage.sum(below, lb, range(1, nb + 1)) * PITCH ** 3 / 1000.0
    down = [[0, 0, -1], [.5, 0, -1], [-.5, 0, -1], [0, .5, -1], [0, -.5, -1]]
    cyls, drains = [], []
    for k in np.argsort(-szs):
        if szs[k] < LOBE_MIN_ML:
            break
        cw = trimesh.transformations.transform_points(
            np.array(np.nonzero(lb == k + 1)).T.astype(float), T)
        p = cw[cw[:, 2] < cw[:, 2].min() + 1.0].mean(0); p[2] = cw[:, 2].min()
        L, d = exit_ray(p, down)
        seg = L + 8.0
        c = trimesh.creation.cylinder(radius=DRAIN_R, height=seg, sections=24)
        c.apply_transform(trimesh.geometry.align_vectors([0, 0, 1], d))
        c.apply_translation(p + d * (L - seg / 2 + 4.0))
        cyls.append(c)
        drains.append({"xyz": [round(v, 1) for v in p], "lobe_ml": round(float(szs[k]), 2),
                       "wall_mm": round(L, 1)})
    # vent: thinnest horizontal wall near cavity top
    high = world[world[:, 2] > zw.max() - 2.0]
    horiz = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0]]
    best = None
    for p in high[::max(1, len(high) // 40)]:
        r = exit_ray(p, horiz)
        if r and (best is None or r[0] < best[0]):
            best = (r[0], p.copy(), r[1])
    if best is None:
        raise HollowError(
            "no vent site found: no horizontal exit ray escaped the model "
            "within 45mm of the cavity top. A sealed cavity is a pressure "
            "vessel; refusing to export one.")
    ventL, vp, vd = best
    c = trimesh.creation.cylinder(radius=VENT_R, height=ventL + 9.0, sections=24)
    c.apply_transform(trimesh.geometry.align_vectors([0, 0, 1], vd))
    c.apply_translation(vp + vd * (ventL / 2 + 0.5))
    cyls.append(c)
    log.update(drains=drains, vent={"xyz": [round(v, 1) for v in vp], "wall_mm": round(ventL, 1)})

    # Drainability doctrine: every cavity voxel must either drain or not
    # exist. A voxel traps resin iff, at its own height, its cavity
    # component below that level contains no drain (resin flows down and
    # cannot climb saddles). Pockets with a discreet downward exit get an
    # extra drain (max 3); everything else -- e.g. hollowed limbs where a
    # hole would be visible -- is REFILLED to solid. Field-driven: arms
    # were hollowed with no acceptable drain spot and held sealed resin.

    def trapped_map(drain_list):
        """Exact per-voxel drainability via bottom-up union-find (priority
        flood): process cavity voxels level by level; v drains iff, once
        its own level is merged, its component already contains a drain.
        No z-banding -- a 1mm pocket saddling into the main cavity 1mm
        higher is correctly caught (field bug: 2mm bands missed short
        arm-stub pockets that merge with the main hollow at their top)."""
        vid = np.full(mat.shape, -1, dtype=np.int64)
        order = np.argsort(idx[:, 2], kind="stable")
        oidx = idx[order]
        vid[tuple(oidx.T)] = np.arange(len(oidx))
        parent = np.arange(len(oidx))
        has_drain = np.zeros(len(oidx), bool)
        def find(a):
            while parent[a] != a:
                parent[a] = parent[parent[a]]
                a = parent[a]
            return a
        def union(a, b):
            ra, rb = find(a), find(b)
            if ra != rb:
                parent[rb] = ra
                has_drain[ra] |= has_drain[rb]
        for d in drain_list:
            dj = np.clip(trimesh.transformations.transform_points([d["xyz"]], Tinv)[0]
                         .round().astype(int), 0, np.array(mat.shape) - 1)
            # mark cavity voxels in a small ball around the drain point
            for off in np.ndindex(3, 3, 3):
                q = np.clip(dj + np.array(off) - 1, 0, np.array(mat.shape) - 1)
                v = vid[tuple(q)]
                if v >= 0:
                    has_drain[v] = True
        nbrs = np.array([[1,0,0],[-1,0,0],[0,1,0],[0,-1,0],[0,0,1],[0,0,-1]])
        drainable = np.zeros(len(oidx), bool)
        ks = oidx[:, 2]
        i = 0
        N = len(oidx)
        shp = np.array(mat.shape)
        while i < N:
            j = i
            k = ks[i]
            while j < N and ks[j] == k:
                j += 1
            for t in range(i, j):  # merge this level with itself + below
                p0 = oidx[t]
                for nb in nbrs:
                    q = p0 + nb
                    if (q < 0).any() or (q >= shp).any():
                        continue
                    if q[2] > k:
                        continue  # only lateral and downward links exist yet
                    v = vid[tuple(q)]
                    if v >= 0 and ks[v] <= k:
                        union(vid[tuple(p0)], v)
            for t in range(i, j):  # judge at own level, after level merged
                drainable[t] = has_drain[find(vid[tuple(oidx[t])])]
            i = j
        trap = np.zeros_like(main)
        bad = oidx[~drainable]
        trap[tuple(bad.T)] = True
        return trap

    trap = trapped_map(drains)
    refilled_ml = 0.0
    extra = 0
    tl, tn = ndimage.label(trap)
    tsz = ndimage.sum(trap, tl, range(1, tn + 1)) * PITCH ** 3 / 1000.0
    for k in np.argsort(-tsz):
        vol_k = tsz[k]
        if vol_k <= 0:
            continue
        comp = tl == k + 1
        cw = trimesh.transformations.transform_points(
            np.array(np.nonzero(comp)).T.astype(float), T)
        pk = cw[cw[:, 2] < cw[:, 2].min() + 1.0].mean(0); pk[2] = cw[:, 2].min()
        drilled = False
        if vol_k >= 0.5 and extra < 3:
            r = exit_ray(pk, down)
            if r and r[0] <= 8.0:  # short, downward exit = discreet drain spot
                L, d = r
                seg = L + 8.0
                c = trimesh.creation.cylinder(radius=DRAIN_R, height=seg, sections=24)
                c.apply_transform(trimesh.geometry.align_vectors([0, 0, 1], d))
                c.apply_translation(pk + d * (L - seg / 2 + 4.0))
                cyls.append(c)
                drains.append({"xyz": [round(v, 1) for v in pk], "lobe_ml": round(float(vol_k), 2),
                               "wall_mm": round(L, 1), "added_by_audit": True})
                extra += 1
                drilled = True
        if not drilled:
            main &= ~comp  # refill: no acceptable drain -> stays solid
            refilled_ml += vol_k
    if extra:  # drains changed; refill anything still trapped
        trap2 = trapped_map(drains)
        main &= ~trap2
        refilled_ml += trap2.sum() * PITCH ** 3 / 1000.0
    cavity_ml = main.sum() * PITCH ** 3 / 1000.0
    # recompute cavity index/world arrays after refill
    idx = np.array(np.nonzero(main)).T
    world = trimesh.transformations.transform_points(idx.astype(float), T)
    zw = world[:, 2]
    log.update(trapped_resin_ml=0.0, audit_added_drains=extra,
               refilled_ml=round(float(refilled_ml), 2),
               cavity_ml=round(cavity_ml, 1), drains=drains)

    # interior shell (marching cubes is inside-out: fix normals, assert volume)
    mp = np.pad(main, 1)
    verts, faces, _, _ = measure.marching_cubes(mp.astype(np.float32), level=0.5)
    inner = trimesh.Trimesh(
        vertices=trimesh.transformations.transform_points(verts - 1.0, T), faces=faces)
    inner.merge_vertices(); trimesh.repair.fix_normals(inner)
    inputs = [m, inner] + cyls
    for c in cyls:
        c.merge_vertices()
    if not all(x.is_volume for x in inputs):
        raise HollowError("non-volume boolean input (cavity shell or a drill "
                          "cylinder is not a closed volume)")
    final = trimesh.boolean.difference(inputs, engine="manifold")
    # Do NOT merge_vertices here: manifold3d output is already welded, and
    # merging can pinch coincident seam vertices into a non-manifold edge,
    # failing verification on an otherwise-valid mesh (found in field test).

    # verification triad + volume + breathing
    bodies = len(final.split(only_watertight=False))
    vol = final.volume / 1000.0
    fv = final.voxelized(0.4); fmat = fv.matrix
    lbl2, _ = ndimage.label(~fmat)
    cpt = trimesh.transformations.transform_points(
        [world.mean(0)], np.linalg.inv(np.array(fv.transform)))[0].round().astype(int)
    breathes = bool(lbl2[tuple(np.clip(cpt, 0, np.array(fmat.shape) - 1))] == lbl2[0, 0, 0])
    # Same five conditions, same thresholds, in the same order as before --
    # named, so a failure can say WHICH one and what it measured instead of
    # collapsing to a bare False.
    vol_err = abs(vol - (solid_ml - cavity_ml))
    vol_tol = 0.02 * solid_ml
    detail = {
        "single_body": {"ok": bodies == 1, "measured": f"{bodies} bodies"},
        "watertight": {"ok": bool(final.is_watertight),
                       "measured": f"is_watertight={bool(final.is_watertight)}"},
        "winding_consistent": {"ok": bool(final.is_winding_consistent),
                               "measured": f"is_winding_consistent="
                                           f"{bool(final.is_winding_consistent)}"},
        "volume_balance": {"ok": bool(vol_err < vol_tol),
                           "measured": f"|{vol:.2f} - ({solid_ml:.2f} - {cavity_ml:.2f})|"
                                       f" = {vol_err:.2f} ml vs tol {vol_tol:.2f} ml"},
        "cavity_breathes": {"ok": breathes, "measured": f"cavity_breathes={breathes}"},
    }
    ok = all(c["ok"] for c in detail.values())
    log.update(final_ml=round(vol, 1), bodies=bodies, watertight=final.is_watertight,
               winding=final.is_winding_consistent, cavity_breathes=breathes,
               check_detail=detail, verify="PASS" if ok else "FAIL",
               elapsed_s=round(time.time() - t0, 1))
    if not ok:
        log.update(status=STATUS_VERIFY_FAILED, failed_checks=failed_checks(log))
        return log
    final.export(out_path)
    log.update(status=STATUS_OK, exported=out_path, failed_checks=[])
    return log
