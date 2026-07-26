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
    standard prep, and an input that already contains an internal cavity --
    a closed shell CONTAINED in another component's material -- is REFUSED
    rather than re-hollowed.
 1. Optional orientation search on a 10x-decimated proxy: +-30 deg sweeps
    about X/Y scored by 0.5*support_area_cm2 + 3*peak_crosssection_cm2 +
    0.3*height_mm. Orientation MUST be final before drilling -- hole
    placement is gravity-dependent. Model is re-rested on z=0.
 2. Hollow decision: voxelize 0.5mm -> fill -> EDT. A model is eligible only
    if max bulk thickness > 10mm AND a wall-thickness cavity saves >15ml or
    >25% of volume. Otherwise export solid (holes in small minis cost more
    than the resin they save). While the dedicated internal-support verifier
    is unavailable, eligible models also export the oriented SOLID mesh:
    generic exterior routing cannot prove that cavity births/islands are
    supported, so emitting its accidental internal components is unsafe.
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
import hashlib
import json
import os
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

# Step-0 triage thresholds. Re-measured per-component across the whole
# 37-model corpus (July 2026): 15 non-main components in 6 of 37 files.
# THIRTEEN of them are degenerate modelling shards, and they are strikingly
# uniform -- 2-46 faces, 0.0385-0.2561mm on the longest bbox axis, every one a
# CLOSED shell enclosing at most 0.00012mm^3, every one sharing no edge with
# anything, every one bowtie-pinched to the main body (a bit-identical shared
# VERTEX) and inside its bounding box. The predicate below is the conjunction
# of exactly those measured properties, so it flags those 13 and nothing else.
# The remaining 2 components both belong to Awakened Tree.stl: its real
# 4.13ml/9096-face internal cavity, and a 1.044mm closed shell that is NOT
# pinched to the body. Neither is junk; both are internal geometry, and that
# file is refused as pre-hollowed rather than stripped.
#: Face-count ceiling for a shard. The largest observed is 46.
SHARD_MAX_FACES = 64
#: Size ceiling for a shard, on its longest bbox axis. ~4x the largest real
#: shard (0.2561mm) and orders of magnitude below any printable feature --
#: a sanity bound, not a fitted threshold. This is the value the original
#: audit validated; it was briefly widened to 2.0mm for the sole purpose of
#: catching Awakened Tree.stl's 1.044mm shell, and that widening is reverted:
#: that shell is not bowtie-pinched so the predicate excludes it anyway, its
#: file is refused as pre-hollowed and routed to drill-only regardless, and a
#: 2mm cap is wide enough to swallow real sub-2mm detail (measured: a 1.53mm
#: gem, a 0.94mm spell-effect shell).
SHARD_MAX_DIM_MM = 1.0
#: A shard may enclose no more than this, in ABSOLUTE value -- i.e. zero
#: within STL float noise; the real shards top out at 0.00012mm^3, four
#: orders of magnitude under the cap. Two-sided on purpose. The earlier
#: one-sided form ("keep it only if it is a positively-wound solid over the
#: cap") let the cap be evaded by winding: an inverted-normal 1.53mm gem
#: enclosing 1.85mm^3 -- 185x the cap -- was dropped as junk because
#: ``is_volume`` said "not a solid", and triage runs before ``fix_normals``
#: so nothing ever got the chance to repair it. Magnitude is the question
#: being asked, so magnitude is what is measured. It is only asked of CLOSED
#: components: for an open patch trimesh's ``volume`` is a divergence sum that
#: depends on where the origin happens to be, so it is not a size at all --
#: an open patch is therefore never dropped on this clause, it is kept.
SHARD_MAX_VOL_MM3 = 0.01
#: Points sampled from a component's boundary for the containment test.
CONTAINMENT_SAMPLES = 32
#: Fraction of those samples that must read as inside another component's
#: material before the component is called internal geometry. A true void's
#: shell lies WHOLLY inside the wall around it (measured 1.000 on Awakened
#: Tree.stl's real cavity), so the slack is only there to absorb ray-parity
#: noise on a grazing hit -- it is deliberately far from 0.5, which would
#: start calling merely interpenetrating bodies (a sword through a fist)
#: internal.
CONTAINMENT_MIN_FRAC = 0.9

#: Wrote ``out_path``. Either the model was hollowed and passed every check in
#: step 8, or it was legitimately decided SOLID and exported as-is (oriented).
STATUS_OK = "OK"
#: Hollowing produced a mesh that failed step 8. NOTHING was written.
STATUS_VERIFY_FAILED = "NOT_EXPORTED_verify_failed"

# Machine-readable reason used by the temporary safe fallback. Keep this
# stable: batch reports and the future A5 migration key off the exact value.
INTERNAL_SUPPORT_VERIFIER_UNAVAILABLE = "internal_support_verifier_unavailable"

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

    Every component that is not the main body is classified as one of three
    things:

    * a degenerate modelling shard -- DROPPED, and reported under
      ``log["dropped_shards"]``;
    * internal geometry, i.e. a closed shell sitting INSIDE another
      component's material -- meaning the file arrived hollowed, so
      :class:`PreHollowedError` is raised and nothing is written;
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
    # PRECONDITION, ENFORCED RATHER THAN ASSUMED. Components are found through
    # face adjacency, which trimesh builds from shared vertex INDICES, not
    # positions. On an unwelded mesh -- ``trimesh.load(..., process=False)``,
    # i.e. exactly what a raw STL's triangle soup deserialises to -- no two
    # triangles share an index, so EVERY FACE is its own component and the
    # whole model reads as 1279 stray shards around one "main" triangle
    # (measured: 1280 faces in, 1 out, no error raised). ``run`` happens to
    # load with process=True, but this is a public entry point and that
    # failure mode is silent geometry destruction, so weld here. On an
    # already-welded mesh it is a no-op.
    m.merge_vertices()

    labels = trimesh.graph.connected_component_labels(
        m.face_adjacency, node_count=len(m.faces))
    n_comp = int(labels.max()) + 1
    log["components"] = n_comp
    if n_comp == 1:
        return m
    face_count = np.bincount(labels, minlength=n_comp)

    # per-component bounding box, from face corners so that a vertex shared by
    # two components (the bowtie pinch below) is counted in both
    fv = m.vertices[m.faces]
    lo = np.full((n_comp, 3), np.inf)
    hi = np.full((n_comp, 3), -np.inf)
    np.minimum.at(lo, labels, fv.min(axis=1))
    np.maximum.at(hi, labels, fv.max(axis=1))
    dims = hi - lo
    diag = np.linalg.norm(dims, axis=1)

    # MAIN is the component of greatest EXTENT, ties broken by face count --
    # NOT the one with the most faces. Tessellation density says nothing about
    # which shell is the model: a cavity is routinely finer than the wall
    # around it, and when it out-faces that wall the face-count rule crowns
    # the VOID as the body, files the real body under "extra bodies", and
    # never tests the cavity at all. Measured on an r=20 shell (320 faces)
    # around an r=12 void (1280 faces): triage passed it, the void was
    # hollowed over, and the run reported status OK. Extent cannot invert like
    # that -- a shell contained in another component's material has, by
    # construction, the smaller bounding box.
    main = int(np.lexsort((face_count, diag))[-1])

    # Hole-safety rests on EDGES, and it is MEASURED here rather than assumed
    # from the component split. These shards are non-manifold bowtie pinches:
    # they share bit-identical VERTEX positions with the main body (measured
    # gap 0.0 at every epsilon down to 1e-12), so any test based on vertex or
    # bounding-box separation concludes -- wrongly -- that they are attached.
    # Watertightness is an edge-valence property, so a component that
    # contributes no edge to any OTHER component cannot change any surviving
    # edge's valence, and dropping it provably cannot open a hole. "Any other"
    # and not "the main body": the main body is not the only thing we keep, so
    # a flap edge-joined to a detached accessory has to be spared too. The
    # test is not redundant with the split either: trimesh builds face
    # adjacency only across 2-valent edges, so a patch joined along a
    # non-manifold (>2 faces) edge lands in its own component while genuinely
    # sharing an edge. That is the case this clause catches.
    e = m.edges_sorted
    keys = e[:, 0].astype(np.int64) * len(m.vertices) + e[:, 1]
    corner_label = np.repeat(labels, 3)  # 3 edges (and 3 corners) per face
    # distinct (edge, component) pairs, packed into one sortable int64; an
    # edge carried by more than one component is an edge we may not remove
    pair = np.unique(keys * n_comp + corner_label)
    _, per_edge = np.unique(pair // n_comp, return_counts=True)
    shares_foreign_edge = np.zeros(n_comp, bool)
    shares_foreign_edge[(pair % n_comp)[np.repeat(per_edge > 1, per_edge)]] = True

    # The bowtie pinch itself, as positive evidence: does this component share
    # a bit-identical VERTEX with the main body? All 13 real shards do; none
    # of the legitimate small parts that the size clauses alone were eating
    # did (a separately authored gem, eye or spell effect is placed in its own
    # coordinates and does not land exactly on one of the body's vertices).
    # Requiring it turns the drop test into "this is a modelling artefact of
    # the body" instead of "this is small enough that nobody will miss it".
    in_main = np.zeros(len(m.vertices), bool)
    in_main[m.faces[labels == main].reshape(-1)] = True
    pinch = in_main[m.faces.reshape(-1)] & (corner_label != main)
    touches_main_vertex = np.zeros(n_comp, bool)
    touches_main_vertex[corner_label[pinch]] = True

    inside_main_bbox = ((lo >= lo[main] - 1e-9).all(axis=1)
                        & (hi <= hi[main] + 1e-9).all(axis=1))

    # face indices grouped by component, in one sort rather than one scan of
    # the label array per component
    groups = np.split(np.argsort(labels, kind="stable"),
                      np.cumsum(face_count)[:-1])
    extra = [c for c in range(n_comp) if c != main]
    subs = dict(zip(extra, m.submesh([groups[c] for c in extra], repair=False)))

    def sub_of(c):
        if c not in subs:  # only the main body, and only if containment needs it
            subs[c] = m.submesh([groups[c]], repair=False)[0]
        return subs[c]

    junk = [c for c in extra
            if not shares_foreign_edge[c]
            and touches_main_vertex[c]
            and inside_main_bbox[c]
            and face_count[c] <= SHARD_MAX_FACES
            and dims[c].max() <= SHARD_MAX_DIM_MM
            # closed, and enclosing nothing -- see SHARD_MAX_VOL_MM3 for why
            # the magnitude is what is measured and why openness keeps a
            # component rather than condemning it
            and subs[c].is_watertight
            and abs(subs[c].volume) <= SHARD_MAX_VOL_MM3]

    # CONTAINMENT is what makes a component a cavity: is it inside another
    # component's MATERIAL? Winding sign is not -- it was the previous test
    # and it fails in both directions. It missed real voids, because one
    # flipped triangle anywhere on the shell makes ``is_winding_consistent``
    # False and the void fell through to "extra body" (measured: 0.506ml of
    # real void destroyed, volume_balance passed at 0.52 vs 0.67ml tolerance,
    # file written, status OK -- and the tolerance is 2% of filled volume, so
    # any pre-existing void under ~0.4ml on a 20ml mini is invisible end to
    # end). And it invented voids that were not there, because ANY
    # inward-wound closed shell qualified: a whole file exported inside-out --
    # mini plus separate base, the classic bad export that ``fix_normals``
    # handled correctly -- was refused as pre-hollowed, and so was a box 200mm
    # away from the body. Containment answers the question actually being
    # asked, and it is orientation-blind: ray parity does not care which way
    # the normals point, so a repairable export is repaired instead of
    # refused. Only CLOSED components are eligible -- an open patch encloses
    # no volume, so it cannot be a void, and burying one in the material does
    # not make the file pre-hollowed (it makes it non-manifold, which the
    # is_volume gate downstream already refuses, with the right diagnosis).
    rest = [c for c in extra if c not in set(junk)]
    closed = [c for c in rest if subs[c].is_watertight]
    cavities = []
    if closed:  # otherwise never touch the main body's submesh, which on a
        # real mini is by far the most expensive object in this function
        containers = sorted([b for b in closed + [main] if sub_of(b).is_watertight],
                            key=lambda b: -diag[b])
        probes = {}
        for c in closed:
            for b in containers:
                # cheap bbox reject first: a contained shell's box is inside
                # its container's box, so this culls nearly everything
                # without casting a single ray
                if b != c and (lo[c] >= lo[b] - 1e-9).all() \
                        and (hi[c] <= hi[b] + 1e-9).all():
                    probes.setdefault(b, []).append(c)
        for b, cs in probes.items():
            # one batched ray query per container, not one per component
            pts = [sub_of(c).vertices[::max(1, len(sub_of(c).vertices)
                                            // CONTAINMENT_SAMPLES)][:CONTAINMENT_SAMPLES]
                   for c in cs]
            inside = np.asarray(sub_of(b).contains(np.concatenate(pts)))
            at = 0
            for c, p in zip(cs, pts):
                frac = inside[at:at + len(p)].mean()
                at += len(p)
                if frac >= CONTAINMENT_MIN_FRAC and c not in cavities:
                    cavities.append(c)
        cavities.sort()  # report in component order, not in ray order
    bodies = [c for c in rest if c not in cavities]

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
        described = _describe(cavities)
        log["cavity_components"] = described
        big = max(described, key=lambda d: d["faces"])
        raise PreHollowedError(
            f"input is already hollowed{': ' + source if source else ''}"
            f" -- {len(cavities)} closed shell(s) sealed inside another "
            f"component's material (largest: {big['faces']} faces, "
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


def _array_fingerprint(parts):
    h = hashlib.sha256()
    for name, value, dtype in parts:
        arr = np.ascontiguousarray(np.asarray(value, dtype=dtype))
        h.update(name.encode("ascii"))
        h.update(np.asarray(arr.shape, dtype="<i8").tobytes())
        h.update(arr.tobytes())
    return h.hexdigest()


def run(in_path, out_path, wall=2.5, orient=True, log=None,
        allow_hollow_output=False, cavity_out=None, solid_backup_path=None):
    """Hollow (or decide not to) and drill ``in_path``, writing ``out_path``.

    Returns the log dict (the same object if one was passed in), always
    carrying ``status``. ``status == STATUS_OK`` is the only outcome under
    which ``out_path`` was written; ``STATUS_VERIFY_FAILED`` additionally
    carries ``failed_checks`` and ``check_detail``. Raises for input problems
    (unreadable mesh, non-volume mesh, already-hollowed mesh) rather than
    reporting them -- those are not outcomes of the algorithm, they are bad
    inputs.

    ``allow_hollow_output`` is the explicit internal-verifier hook. It must
    remain false unless the caller has a dedicated internal-support verifier
    wired after support placement. ``cavity_out`` is a runtime-only dict which
    receives the exact post-refill cavity mask and its original voxel
    transforms; reader-facing identity/provenance is copied into ``log``.
    ``solid_backup_path`` preserves the already-oriented solid for fail-safe
    fallback without repeating orientation.
    """
    log = {} if log is None else log
    if cavity_out is not None:
        cavity_out.clear()
    t0 = time.time()
    m = trimesh.load(in_path)
    m = triage_components(m, log, source=in_path)  # welds the mesh itself
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
    log.update(
        hollow_attempted=True,
        solid_ml=round(solid_ml, 1),
        solid_volume_estimate_ml=round(solid_ml, 3),
        dims_mm=[round(x, 1) for x in m.extents],
    )

    def _export_solid(decision, *, eligible=False, fallback=False,
                      reason=None, hollow_volume_estimate_ml=None):
        m.export(out_path)
        log.update(
            decision=decision,
            hollow_eligible=bool(eligible),
            hollow_fallback_applied=bool(fallback),
            hollow_fallback_reason=reason,
            hollow_volume_estimate_ml=(
                None if hollow_volume_estimate_ml is None
                else round(float(hollow_volume_estimate_ml), 3)
            ),
            final_geometry="solid",
            final_ml=round(solid_ml, 1),
            status=STATUS_OK,
            exported=out_path,
            verify="N/A_solid",
            elapsed_s=round(time.time() - t0, 1),
        )
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
    hollow_volume_estimate_ml = max(0.0, solid_ml - cavity_ml)
    if bulk < BULK_MIN_MM or (cavity_ml < SAVE_MIN_ML and cavity_ml < SAVE_MIN_FRAC * solid_ml):
        return _export_solid(
            "SOLID_below_threshold",
            hollow_volume_estimate_ml=hollow_volume_estimate_ml,
        )
    if not allow_hollow_output:
        return _export_solid(
            "SOLID_internal_support_verifier_unavailable",
            eligible=True,
            fallback=True,
            reason=INTERNAL_SUPPORT_VERIFIER_UNAVAILABLE,
            hollow_volume_estimate_ml=hollow_volume_estimate_ml,
        )
    if solid_backup_path is not None:
        backup_dir = os.path.dirname(os.path.abspath(os.fspath(solid_backup_path)))
        if backup_dir:
            os.makedirs(backup_dir, exist_ok=True)
        m.export(solid_backup_path)
        log["oriented_solid_backup"] = os.fspath(solid_backup_path)
    log.update(
        decision="HOLLOW",
        hollow_eligible=True,
        hollow_fallback_applied=False,
        hollow_fallback_reason=None,
        hollow_volume_estimate_ml=round(hollow_volume_estimate_ml, 3),
        final_geometry="hollow",
    )

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
    cyls, drains, drill_keepouts = [], [], []
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
        drill_keepouts.append({
            "role": "drain",
            "p0": (p - 4.0 * d).tolist(),
            "p1": (p + (L + 4.0) * d).tolist(),
            "axis": d.tolist(),
            "radius_mm": float(DRAIN_R),
        })
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
    drill_keepouts.append({
        "role": "vent",
        "p0": (vp - 4.0 * vd).tolist(),
        "p1": (vp + (ventL + 5.0) * vd).tolist(),
        "axis": vd.tolist(),
        "radius_mm": float(VENT_R),
    })
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
                drill_keepouts.append({
                    "role": "audit_drain",
                    "p0": (pk - 4.0 * d).tolist(),
                    "p1": (pk + (L + 4.0) * d).tolist(),
                    "axis": d.tolist(),
                    "radius_mm": float(DRAIN_R),
                })
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
    cavity_fingerprint = _array_fingerprint((
        ("cavity_mask", np.packbits(main, bitorder="little"), np.uint8),
        ("transform", T, "<f8"),
        ("inverse_transform", Tinv, "<f8"),
    ))
    inner_fingerprint = _array_fingerprint((
        ("vertices", inner.vertices, "<f8"),
        ("faces", inner.faces, "<i8"),
    ))
    keepout_json = [{
        "role": d["role"],
        "p0": [round(float(v), 6) for v in d["p0"]],
        "p1": [round(float(v), 6) for v in d["p1"]],
        "axis": [round(float(v), 9) for v in d["axis"]],
        "radius_mm": round(float(d["radius_mm"]), 6),
    } for d in drill_keepouts]
    descriptor_payload = {
        "schema": "support-placement/a5-cavity",
        "schema_version": 1,
        "pitch_mm": float(PITCH),
        "shape": [int(v) for v in main.shape],
        "cavity_voxels": int(main.sum()),
        "solid_voxels": int((mat & ~main).sum()),
        "outside_voxels": int((~mat).sum()),
        "cavity_fingerprint": cavity_fingerprint,
        "inner_mesh_fingerprint": inner_fingerprint,
        "transform": T.tolist(),
        "inverse_transform": Tinv.tolist(),
        "drain_keepouts": keepout_json,
    }
    descriptor_fingerprint = hashlib.sha256(json.dumps(
        descriptor_payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    descriptor_payload["descriptor_fingerprint"] = descriptor_fingerprint
    log["cavity_descriptor"] = descriptor_payload
    if cavity_out is not None:
        cavity_mask = np.frombuffer(
            np.ascontiguousarray(main, dtype=np.bool_).tobytes(),
            dtype=np.bool_,
        ).reshape(main.shape)
        solid_mask = np.frombuffer(
            np.ascontiguousarray(mat & ~main, dtype=np.bool_).tobytes(),
            dtype=np.bool_,
        ).reshape(main.shape)
        cavity_out.update(
            schema=descriptor_payload["schema"],
            schema_version=descriptor_payload["schema_version"],
            pitch_mm=float(PITCH),
            cavity_mask=cavity_mask,
            solid_mask=solid_mask,
            transform=np.array(T, dtype=np.float64, copy=True),
            inverse_transform=np.array(Tinv, dtype=np.float64, copy=True),
            inner_mesh=inner,
            drill_keepouts=tuple(drill_keepouts),
            cavity_fingerprint=cavity_fingerprint,
            inner_mesh_fingerprint=inner_fingerprint,
            descriptor_fingerprint=descriptor_fingerprint,
        )
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
