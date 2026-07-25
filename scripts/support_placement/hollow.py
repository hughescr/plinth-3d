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
 0. Load-time triage of the input's connected components (see
    :func:`triage_components`): degenerate modelling shards are stripped as
    standard prep, and an input that already contains an internal cavity is
    REFUSED rather than re-hollowed.
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

Bad INPUTS are not statuses -- they raise :class:`HollowError`, which the CLI
maps to exit 1 and the pipeline to a failed hollow stage. There are two:
a mesh that is not a watertight volume, and a mesh that arrived ALREADY
HOLLOWED (:class:`PreHollowedError`, raised by step 0). Neither is an outcome
of the algorithm; both mean this stage is the wrong tool for that file. The
error type is recorded in the CLI's JSON report under ``error_type``, so a
caller can tell "unreadable" from "route this to drill-only".

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

# Step-0 triage thresholds. Measured across the 37-model corpus (July 2026):
# 14 degenerate shards in 9 of 37 models, 2-46 faces each, 0.005-1.04mm
# across, each a closed shell of zero-to-noise volume sitting inside the main
# bbox and sharing no edge with it. The whole predicate flags those 14 and
# nothing else -- in particular it does not touch Awakened Tree.stl's real
# 4.13ml, 9096-face cavity.
#: Face-count ceiling for a shard. The largest observed is 46.
SHARD_MAX_FACES = 64
#: Size ceiling for a shard, on its longest bbox axis. ~8x the largest junk
#: in the 8 models the original audit covered (0.256mm), and orders of
#: magnitude below any printable feature -- this is a sanity bound, not a
#: fitted threshold. Originally validated at 1.0mm; raised to 2.0 because
#: Awakened Tree.stl's own shard measures 1.044mm and was spared by 44um.
#: Re-validated at 2.0 against all 37 models: still exactly those 14, still
#: no false positives.
SHARD_MAX_DIM_MM = 2.0
#: A shard may not enclose a SOLID bigger than this -- i.e. zero within STL
#: float noise; the 4 shards that are outward-wound solids measure at most
#: 0.00012mm^3. The clause is one-sided on purpose. Its job is to protect
#: genuine detached geometry, and genuine geometry is solid: a closed,
#: consistently-wound shell of POSITIVE volume, which is exactly what
#: trimesh's ``is_volume`` reports. A component that is not a solid at all --
#: an open patch, an inward-wound shell (a void, not a body), a double-sided
#: triangle -- encloses nothing worth keeping, so it is junk whatever a
#: signed divergence sum over its faces happens to total. That branch is
#: load-bearing: the other 10 shards go through it, and Awakened Tree.stl's
#: totals -0.0146mm^3, over this cap in absolute terms. The caps above bound
#: the worst it can cost at 8mm^3 of sealed void -- and an undrainable pocket
#: that small is a resin trap this stage would refuse to hollow anyway.
SHARD_MAX_VOL_MM3 = 0.01

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


class PreHollowedError(HollowError):
    """The input arrived already hollowed, so this stage cannot run on it.

    Measured, not theoretical: ``m.voxelized(PITCH).fill()`` (step 2) fills an
    existing cavity back in and reads the wall as bulk -- 40.4mm of "bulk" on
    a test sphere whose wall is 2.5mm. The HOLLOW decision then fires, derives
    a brand new cavity from scratch, and the real one is discarded. On the
    symmetric test shape ``volume_balance`` happened to catch it; on an
    asymmetric model it could pass by coincidence, exporting a mini whose
    cavity is not the one it was designed with. So this is a refusal, not a
    repair: such a file needs the drill-only path.
    """


def triage_components(m, log, source=""):
    """Step 0: classify ``m``'s connected components, strip the junk, refuse
    the already-hollowed. Mutates and returns ``m``.

    Every component that is not the main body (the one with the most faces) is
    classified as one of three things:

    * a degenerate modelling shard -- DROPPED, and reported under
      ``log["dropped_shards"]``;
    * an internal cavity, i.e. an inward-wound closed shell -- meaning the
      file arrived hollowed, so :class:`PreHollowedError` is raised and
      nothing is written;
    * any other detached body -- left exactly as it was (this stage has
      always passed multi-body files through), noted in
      ``log["extra_bodies"]`` so it is at least visible.

    This is deliberately a per-component DROP test and NOT "keep only the
    largest component". The two agree on every model that has junk and
    disagree catastrophically on the one that matters: keep-largest would
    silently delete Awakened Tree.stl's real 4.13ml internal cavity, which is
    exactly the geometry the refusal above exists to protect.

    Runs before the ``is_volume`` gate, because a shard can be what makes a
    mesh fail it, and before everything downstream: the SOLID path exports
    ``m`` with no cleanup at all, so an unstripped shard rides through into
    the printed STL.
    """
    if len(m.faces) == 0:
        return m
    labels = trimesh.graph.connected_component_labels(
        m.face_adjacency, node_count=len(m.faces))
    n_comp = int(labels.max()) + 1
    log["components"] = n_comp
    if n_comp == 1:
        return m
    face_count = np.bincount(labels, minlength=n_comp)
    main = int(np.argmax(face_count))

    # Hole-safety rests on EDGES, and it is MEASURED here rather than assumed
    # from the component split. These shards are non-manifold bowtie pinches:
    # they share bit-identical VERTEX positions with the main body (measured
    # gap 0.0 at every epsilon down to 1e-12), so any test based on vertex or
    # bounding-box separation concludes -- wrongly -- that they are attached.
    # Watertightness is an edge-valence property, so a component that
    # contributes no edge to the main body cannot change any main-body edge's
    # valence, and dropping it provably cannot open a hole. The test is not
    # redundant with the split either: trimesh builds face adjacency only
    # across 2-valent edges, so a patch joined to the body along a
    # non-manifold (>2 faces) edge lands in its own component while genuinely
    # sharing an edge with it. That is the case this clause catches.
    e = m.edges_sorted
    keys = e[:, 0].astype(np.int64) * len(m.vertices) + e[:, 1]
    edge_label = np.repeat(labels, 3)  # edges_sorted is 3 per face, in face order
    main_keys = np.unique(keys[edge_label == main])
    shared = (edge_label != main) & np.isin(keys, main_keys)
    shares_main_edge = np.zeros(n_comp, bool)
    shares_main_edge[edge_label[shared]] = True

    # per-component bounding box, from face corners so that a vertex shared by
    # two components (the pinch again) is counted in both
    fv = m.vertices[m.faces]
    lo = np.full((n_comp, 3), np.inf)
    hi = np.full((n_comp, 3), -np.inf)
    np.minimum.at(lo, labels, fv.min(axis=1))
    np.maximum.at(hi, labels, fv.max(axis=1))
    dims = hi - lo

    # face indices grouped by component, in one sort rather than one scan of
    # the label array per component
    groups = np.split(np.argsort(labels, kind="stable"),
                      np.cumsum(face_count)[:-1])
    extra = [c for c in range(n_comp) if c != main]
    junk, cavities, bodies = [], [], []
    for c, sub in zip(extra, m.submesh([groups[c] for c in extra], repair=False)):
        if (not shares_main_edge[c]
                and face_count[c] <= SHARD_MAX_FACES
                and dims[c].max() <= SHARD_MAX_DIM_MM
                # ``is_volume`` is the "is this a solid?" test: closed,
                # consistently wound, POSITIVE volume. Anything failing it
                # encloses nothing -- see SHARD_MAX_VOL_MM3.
                and (not sub.is_volume or sub.volume <= SHARD_MAX_VOL_MM3)):
            junk.append(c)
        elif sub.is_watertight and sub.is_winding_consistent and sub.volume < 0:
            # A closed shell wound INWARD is a void in the material, not a
            # body: negative volume is the STL convention for "normals point
            # away from solid" read in reverse. That is what an
            # already-hollowed model looks like, and it is what separates a
            # cavity from a merely-detached second body (a base, a dropped
            # weapon) -- those are outward-wound and positive, and this stage
            # has always passed them through untouched.
            cavities.append(c)
        else:
            bodies.append(c)

    def _describe(ids):
        return [{"faces": int(face_count[c]),
                 "dims_mm": [round(float(x), 4) for x in dims[c]]} for c in ids]

    if bodies:  # not ours to touch, but the operator should know they are there
        log["extra_bodies"] = _describe(bodies)
    if junk:
        # silent mesh mutation is not acceptable: say what left and how big
        log["dropped_shards"] = _describe(junk)
        log["dropped_shard_faces"] = int(face_count[junk].sum())
        m.update_faces(~np.isin(labels, junk))
        m.remove_unreferenced_vertices()

    if cavities:
        log["cavity_components"] = _describe(cavities)
        big = max(_describe(cavities), key=lambda d: d["faces"])
        raise PreHollowedError(
            f"input is already hollowed{': ' + source if source else ''}"
            f" -- {len(cavities)} internal cavity shell(s) besides the main "
            f"body (largest: {big['faces']} faces, "
            f"{'x'.join(f'{d:g}' for d in big['dims_mm'])}mm). Hollowing it "
            f"again would fill that cavity in, read the wall as bulk and "
            f"carve a new cavity from scratch, discarding the real one. "
            f"Re-run it through the drill-only path instead.")
    return m


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
    (unreadable mesh, non-volume mesh, already-hollowed mesh) rather than
    reporting them -- those are not outcomes of the algorithm, they are bad
    inputs.
    """
    log = {} if log is None else log
    t0 = time.time()
    m = trimesh.load(in_path); m.merge_vertices()
    m = triage_components(m, log, source=in_path)
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
