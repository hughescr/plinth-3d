"""Overhang/island detection and the scar-cost (delicacy) field.

Design choice (documented, not silent): v0's island scan was voxel-grid
based (see design.md history). v2 uses a face-adjacency-graph definition
instead: an "island" is a connected component, over shared mesh edges, of
faces that (a) are down-facing beyond the overhang threshold and (b) are
not self-grounded (near the model's own base, or shadowed by more model
material a short bridge-gap below). This is simpler to reason about,
cheaper to test, and composes directly with the face-based scar-cost field
-- both walk the same face graph. Coverage (which islands still need
support after N contacts are placed) is then a proximity query against
placed contacts, not a second voxel pass. See references/design.md for the
charter items this satisfies (#1 groundwork, #7).
"""
import numpy as np
from scipy.sparse import coo_matrix
from scipy.sparse.csgraph import connected_components


def down_facing_mask(mesh, cfg):
    """True for faces whose normal points down beyond cfg.overhang_angle_deg
    from vertical (matches CHITUBOX's autosupportangle convention: at
    exactly the threshold lean-from-vertical, nz = -sin(threshold))."""
    nz = mesh.face_normals[:, 2]
    return nz < -np.sin(np.radians(cfg.overhang_angle_deg))


def grounded_mask(mesh, cfg, down_mask=None, column_pitch_mm=1.0):
    """True for down-facing faces that don't actually need a printed
    support: near the model's own lowest point, or shadowed by more model
    material within a short bridge-spannable gap directly below.

    Column-cached ray casting: one ray per occupied XY grid COLUMN
    (multiple_hits=True, so every z-crossing in that column is captured
    in one query), not one ray per down-facing FACE. On a real ~576k-face
    organic mesh (Weeping Treant) there are ~53k down-facing faces but
    only a few thousand occupied 1mm columns -- casting per-face against
    trimesh's non-embree RayMeshIntersector was intractable at that scale
    (each query walks an rtree over the whole mesh); per-column is the
    same physical answer for a fraction of the ray count.
    """
    if down_mask is None:
        down_mask = down_facing_mask(mesh, cfg)
    n = len(mesh.faces)
    grounded = np.zeros(n, dtype=bool)
    if not down_mask.any():
        return grounded
    centers = mesh.triangles_center
    zmin = float(mesh.bounds[0][2])
    near_plate = (centers[:, 2] - zmin) <= cfg.near_plate_mm
    grounded |= (down_mask & near_plate)

    idx = np.nonzero(down_mask & ~near_plate)[0]
    if len(idx) == 0:
        return grounded

    xy = centers[idx, :2]
    xmin = xy[:, 0].min()
    ymin = xy[:, 1].min()
    col_ix = np.floor((xy[:, 0] - xmin) / column_pitch_mm).astype(np.int64)
    col_iy = np.floor((xy[:, 1] - ymin) / column_pitch_mm).astype(np.int64)
    col_key = col_ix * (col_iy.max() + 1) + col_iy
    uniq_keys, inverse = np.unique(col_key, return_inverse=True)

    # One ray per unique column, from above the model straight down.
    col_ix_u = uniq_keys // (col_iy.max() + 1)
    col_iy_u = uniq_keys % (col_iy.max() + 1)
    col_x = xmin + (col_ix_u + 0.5) * column_pitch_mm
    col_y = ymin + (col_iy_u + 0.5) * column_pitch_mm
    ztop = float(mesh.bounds[1][2]) + 1.0
    origins = np.column_stack([col_x, col_y, np.full(len(col_x), ztop)])
    directions = np.tile([0.0, 0.0, -1.0], (len(col_x), 1))

    try:
        locations, ray_idx, _ = mesh.ray.intersects_location(
            origins, directions, multiple_hits=True)
    except Exception:
        raise RuntimeError(
            "ray intersection backend unavailable (trimesh.ray) -- "
            "install rtree, or pass a mesh with a working ray query")

    crossings_by_col = {}
    if len(locations) > 0:
        zs = locations[:, 2]
        order = np.argsort(ray_idx, kind="stable")
        ray_idx_s, zs_s = ray_idx[order], zs[order]
        split_at = np.searchsorted(ray_idx_s, np.arange(len(col_x) + 1))
        for c in range(len(col_x)):
            seg = zs_s[split_at[c]:split_at[c + 1]]
            if len(seg):
                crossings_by_col[c] = np.sort(seg)[::-1]

    eps = 1e-3
    for face_pos, face_idx in enumerate(idx):
        col = inverse[face_pos]
        crossings = crossings_by_col.get(col)
        if crossings is None or len(crossings) < 2:
            continue
        face_z = centers[face_idx, 2]
        below = crossings[crossings < face_z - eps]
        if len(below) and (face_z - below[0]) <= cfg.bridge_free_gap_mm:
            grounded[face_idx] = True
    return grounded


def overhang_islands(mesh, cfg):
    """Connected components of down-facing, non-grounded faces, merging
    across BOTH true mesh-edge adjacency AND cfg.island_merge_radius_mm of
    spatial proximity (see that field's docstring: pure topological
    adjacency badly over-fragments organic/textured surfaces). Returns
    list of np.ndarray face-index arrays, largest first."""
    from scipy.spatial import cKDTree

    down = down_facing_mask(mesh, cfg)
    grounded = grounded_mask(mesh, cfg, down)
    need = down & ~grounded
    face_ids = np.nonzero(need)[0]
    if len(face_ids) == 0:
        return []
    remap = -np.ones(len(mesh.faces), dtype=np.int64)
    remap[face_ids] = np.arange(len(face_ids))
    m = len(face_ids)

    adj = mesh.face_adjacency
    a, b = remap[adj[:, 0]], remap[adj[:, 1]]
    keep = (a >= 0) & (b >= 0)
    a_list, b_list = [a[keep]], [b[keep]]

    if cfg.island_merge_radius_mm > 0 and m > 1:
        tree = cKDTree(mesh.triangles_center[face_ids])
        pairs = tree.query_pairs(r=cfg.island_merge_radius_mm, output_type="ndarray")
        if len(pairs):
            a_list.append(pairs[:, 0])
            b_list.append(pairs[:, 1])

    a_all = np.concatenate(a_list) if a_list else np.array([], dtype=np.int64)
    b_all = np.concatenate(b_list) if b_list else np.array([], dtype=np.int64)
    graph = coo_matrix((np.ones(len(a_all)), (a_all, b_all)), shape=(m, m))
    n_comp, labels = connected_components(graph, directed=False)
    islands = [face_ids[labels == k] for k in range(n_comp)]
    islands.sort(key=len, reverse=True)
    return islands


def local_z_minima(mesh, cfg):
    """Island-birth minima that need direct support (v3.6, Craig's rule).

    A local minimum of the surface height function births a new
    cross-section island at its own z: those first layers are a
    disconnected patch curing onto the FEP until the island merges with
    something. Lateral proximity of a contact is NOT support -- on a
    shallow toe, a contact 1mm away laterally is 10+ layers above the
    island's birth. The test that matters is CONNECTION TIME, in layers.

    Method (per-minimum bounded flood, not full Morse persistence -- we
    only ever need to resolve "connects within tol", so each flood stays
    inside a tol-height band and regions are tiny except on plateaus,
    where sharing one region is exactly right):
      1. candidates: vertices no 1-ring neighbor undercuts by more than
         minima_eps_mm (eps admits flat-plateau ties);
      2. flood from each candidate (ascending z, visited-deduped) over
         mesh vertex adjacency restricted to z <= z_min + tol, where
         tol = minima_connect_layers * layer_height_mm;
      3. if the flooded region reaches a vertex strictly LOWER than the
         minimum (earlier-printed material -- inductively supported at
         its own birth by this same rule), the island connects within
         tol layers of birth: exempt.

    There is deliberately NO near-plate exemption here (unlike
    grounded_mask's island rule): the engine's scenes are lifted, so the
    model's own lowest point is the FIRST island of the entire print --
    exempting it would miss the flagship rearmost-toe case. A flat-
    bottomed model therefore honestly gets one contact on its bottom
    plateau.

    The returned birth region doubles as the support test: a minimum is
    satisfied only by a routed contact within minima_contact_radius_mm of
    its birth-region vertices (for a sharp toe that means AT the tip; for
    a flat plateau, anywhere on the plateau -- the whole plateau is one
    birth island and one contact holds all of it at birth).

    The mesh is vertex-welded on a local copy first (mesh_io deliberately
    loads process=False to keep caller-facing indices stable; an unwelded
    STL triangle soup has no cross-face vertex adjacency and would flood
    nothing). Region data is therefore returned as POSITIONS, not indices
    into the caller's mesh. Returns list of dicts, ascending z:
    {position, normal, z, region_pts} (region_pts = (N,3) float array).
    """
    import trimesh
    mw = trimesh.Trimesh(vertices=np.asarray(mesh.vertices).copy(),
                         faces=np.asarray(mesh.faces).copy(), process=True)
    z = mw.vertices[:, 2]
    nv = len(z)
    eps = cfg.minima_eps_mm
    tol = cfg.minima_connect_layers * cfg.layer_height_mm
    edges = mw.edges_unique
    if len(edges) == 0:
        return []

    # neighbor-min z per vertex (vectorized over the edge list)
    nmin = np.full(nv, np.inf)
    np.minimum.at(nmin, edges[:, 0], z[edges[:, 1]])
    np.minimum.at(nmin, edges[:, 1], z[edges[:, 0]])
    cand = np.nonzero(z <= nmin + eps)[0]
    if len(cand) == 0:
        return []

    # CSR adjacency for the floods
    e2 = np.concatenate([edges, edges[:, ::-1]])
    order = np.argsort(e2[:, 0], kind="stable")
    src = e2[order, 0]
    dst = e2[order, 1]
    starts = np.searchsorted(src, np.arange(nv + 1))

    visited = np.zeros(nv, dtype=bool)
    out = []
    for v in cand[np.argsort(z[cand], kind="stable")]:
        if visited[v]:
            continue
        z_v = z[v]
        lid = z_v + tol
        # bounded BFS strictly inside the birth band [z_v - eps, z_v + tol]:
        # a strictly-lower neighbor IS the connection event (earlier-born
        # material) and terminates the flood -- it must not be traversed,
        # or a connecting region would flood the whole model below it.
        region = [v]
        seen = {int(v)}
        stack = [int(v)]
        connects = False
        while stack and not connects:
            u = stack.pop()
            for w in dst[starts[u]:starts[u + 1]]:
                w = int(w)
                if w in seen or z[w] > lid:
                    continue
                if z[w] < z_v - eps:
                    connects = True
                    break
                seen.add(w)
                region.append(w)
                stack.append(w)
        region = np.array(region, dtype=np.int64)
        visited[region] = True
        if connects:
            continue  # merges with earlier-printed material within tol
        out.append(dict(position=mw.vertices[v].copy(),
                        normal=np.asarray(mw.vertex_normals[v], float).copy(),
                        z=float(z_v), region_pts=mw.vertices[region].copy()))
    return out


# ---- Slice-based island-birth detection (v4 p6, Craig's strict criterion:
# design doc / SETUP charter "island-coverage" item I1) ----
#
# local_z_minima (above) is a fast, topologically-exact first pass: it
# finds every vertex-graph local minimum and floods a bounded band to see
# if it connects to strictly-lower (already-printed) material within
# cfg.minima_connect_layers layers. It is precise but VERTEX-graph based,
# so it can only ever resolve connection events that a mesh EDGE can
# express -- a raking/migrating thin feature whose birth polygon appears,
# drifts, and reappears slice-to-slice (no single vertex 1-ring captures
# that motion) is invisible to it, and it says nothing about MULTIPLE
# distinct birth events at different z on what the vertex graph considers
# one connected region.
#
# island_births is the ground-truth cross-check (measured on the real
# Weeping Treant corpus: full_model_audit.py / island_audit_strict.py in
# the design scratchpad -- 211 real-layer births at 0.05mm full-height
# resolution, 77 uncovered by the pre-existing engine): actually slice the
# mesh at the REAL print layer height (cfg.layer_height_mm) over the
# model's full height and track, slice to slice, which polygon components
# have NO xy-overlap with any component in the layer directly below. Such
# a component is, by definition, a disconnected patch curing onto the FEP
# at its own birth layer -- Craig's "lump floating in the resin tank"
# criterion operates on exactly this event, not on vertex adjacency.
#
# Deterministic (trimesh.section_multiplane on a fixed height list, no
# RNG) and O(model height / layer_height_mm) slices -- measured ~53s on
# the full 128mm-tall, 576k-face Weeping Treant at 0.05mm (Craig-approved
# budget for a `prepare()` stage that runs once per print job). Processed
# in z-chunks purely to bound memory (section_multiplane keeps every
# section of a batch live at once); a single birth-connectivity state
# (`prev_comps`) is carried across chunk boundaries so chunking is not a
# source of boundary artifacts -- the result is identical to slicing the
# whole height in one call, just cheaper to hold in memory.

def _signed_area_xy(loop):
    """Shoelace signed area of an (N,2) xy polygon loop; trimesh's
    Path2D.discrete winds outer boundaries positive and holes negative --
    used below purely to drop holes (interior voids never a birth event
    on their own) and degenerate slivers."""
    x, y = loop[:, 0], loop[:, 1]
    return 0.5 * float(np.sum(x * np.roll(y, -1) - np.roll(x, -1) * y))


def _bbox_xy(loop):
    return (float(loop[:, 0].min()), float(loop[:, 0].max()),
            float(loop[:, 1].min()), float(loop[:, 1].max()))


def _bbox_overlap(b1, b2):
    return not (b1[1] < b2[0] or b2[1] < b1[0] or b1[3] < b2[2] or b2[3] < b1[2])


def _segments_intersect(p1, p2, p3, p4):
    def ccw(a, b, c):
        return (c[1] - a[1]) * (b[0] - a[0]) > (b[1] - a[1]) * (c[0] - a[0])
    return (ccw(p1, p3, p4) != ccw(p2, p3, p4)) and (ccw(p1, p2, p3) != ccw(p1, p2, p4))


def point_in_polygon_xy(px, py, poly):
    """Standard ray-casting point-in-polygon test (poly: (N,2) xy)."""
    x, y = poly[:, 0], poly[:, 1]
    x2, y2 = np.roll(x, -1), np.roll(y, -1)
    cond = ((y > py) != (y2 > py))
    with np.errstate(divide="ignore", invalid="ignore"):
        xin = x + (py - y) / (y2 - y) * (x2 - x)
    hit = cond & (px < xin)
    return bool(np.sum(hit) % 2 == 1)


def _point_to_segment_dist_xy(p, a, b):
    p, a, b = np.asarray(p, float), np.asarray(a, float), np.asarray(b, float)
    ab = b - a
    denom = float(np.dot(ab, ab))
    if denom < 1e-15:
        return float(np.linalg.norm(p - a))
    t = float(np.clip(np.dot(p - a, ab) / denom, 0.0, 1.0))
    proj = a + t * ab
    return float(np.linalg.norm(p - proj))


def point_to_polygon_boundary_dist_xy(px, py, poly):
    p = (px, py)
    n = len(poly)
    best = float("inf")
    for i in range(n):
        d = _point_to_segment_dist_xy(p, poly[i], poly[(i + 1) % n])
        if d < best:
            best = d
    return best


def island_birth_xy_ok(px, py, poly, tol_mm):
    """The xy half of Craig's strict at-birth criterion: True if (px,py)
    lands ON the birth footprint `poly`, or within `tol_mm` of its
    boundary otherwise. Returns (ok, dist_mm) -- dist_mm is 0.0 when
    inside (for an honest "no gap" report), else the boundary distance
    regardless of whether it cleared tol_mm (so callers can report how
    far off an out-of-tolerance case was)."""
    if point_in_polygon_xy(px, py, poly):
        return True, 0.0
    d = point_to_polygon_boundary_dist_xy(px, py, poly)
    return (d <= tol_mm), d


def _polys_overlap_xy(loop_a, bbox_a, loop_b, bbox_b):
    """True if two xy polygon loops share any area: bbox reject, then
    vertex-in-other-polygon sampling, then edge-segment intersection --
    catches full containment (a shrinking/growing footprint) and partial
    overlap (a migrating/raking footprint) alike. Vertex/edge sampling is
    capped (not every point) since birth loops can have hundreds of
    points on a dense mesh and this runs O(slices) times per model --
    matches the measured-cost reference implementation
    (full_model_audit.py) this function is ported from."""
    if not _bbox_overlap(bbox_a, bbox_b):
        return False
    for (px, py) in loop_a[::max(1, len(loop_a) // 40)]:
        if point_in_polygon_xy(px, py, loop_b):
            return True
    for (px, py) in loop_b[::max(1, len(loop_b) // 40)]:
        if point_in_polygon_xy(px, py, loop_a):
            return True
    na, nb = len(loop_a), len(loop_b)
    stepa = max(1, na // 60)
    stepb = max(1, nb // 60)
    for i in range(0, na, stepa):
        p1, p2 = loop_a[i], loop_a[(i + stepa) % na]
        for j in range(0, nb, stepb):
            p3, p4 = loop_b[j], loop_b[(j + stepb) % nb]
            if _segments_intersect(p1, p2, p3, p4):
                return True
    return False


# Conservative attach-reach for the strict island-birth criterion: how far
# in xy a contact may sit from a birth footprint's boundary and still
# count as landing "on" it (used both to dedupe an already-covered birth
# in engine._ensure_island_birth_contacts and as the coverage gate's xy
# tolerance in verify.island_birth_report). Set at tip_r_min_mm
# (config.py's own removal-friendly tip floor) -- the smallest tip this
# engine ever builds -- so the tolerance never assumes a bigger tip than
# what's actually guaranteed to exist. Deliberately a plain module
# constant, not a SupportConfig field: config.py is owned by a parallel
# workstream (p6-config/cco/struts/tree) for this branch, and this value
# is a conservative geometric floor tied to an existing config constant's
# own documented meaning, not a new physics/tuning knob to expose.
#
# NOT subject to design-v4.md section 11's layer-derivation amendment
# (Craig, 2026-07-22: "any tolerance whose rationale is print resolution,
# layer-to-layer connection timing, or a build-plate/attachment z-window
# must be expressed as a multiple of cfg.layer_height_mm") -- that
# amendment governs Z-direction/print-timing tolerances. This is an XY
# mechanical reach tied to the smallest physically-buildable tip radius,
# independent of layer height at any resolution (a 0.2mm tip is exactly
# as wide at 0.05mm layers as at 0.02mm layers). See
# _ensure_island_birth_contacts's z_tol_mm (engine.py) and
# island_birth_report's z_tol_mm (verify.py) for the Z half of the
# criterion, which IS derived from cfg.layer_height_mm at the point of
# use, per the amendment.
#
# PINNED at 0.2mm -- do not casually widen. This is the floor Craig's
# strict criterion depends on to actually be strict: widening it back
# toward the old, too-loose 1.0mm minima_contact_radius_mm would silently
# reintroduce the self-certifying coverage gate this branch exists to
# fix (see test_perception.test_island_attach_reach_mm_is_pinned and
# test_verify.test_island_birth_report_hardcoded_boundary_catches_reach_widening
# -- the latter uses a HARDCODED literal distance specifically so a
# regression here cannot pass by construction).
ISLAND_ATTACH_REACH_MM = 0.2


def island_births(mesh, cfg, chunk_mm=16.0, min_area_mm2=1e-3):
    """I1: slice `mesh` at the real print layer height (cfg.layer_height_mm)
    over its full z range and return every polygon component with no xy
    overlap with the layer directly below it -- an "island birth" (see
    module-level comment above for the full rationale).

    Returns a list of dicts, ascending z: {x, y, z, area_mm2, loop} where
    (x, y) is the birth polygon's centroid (its own footprint's boundary
    passes through the actual mesh surface by construction, so the
    centroid of a birth-scale polygon -- these are new/small by
    definition -- sits at or extremely close to the real surface) and
    `loop` is the (N,2) xy polygon itself, consumed by
    island_birth_xy_ok for the strict on-footprint coverage test.
    """
    step = float(cfg.layer_height_mm)
    zmin = float(mesh.bounds[0][2])
    zmax = float(mesh.bounds[1][2])
    total_h = zmax - zmin
    if total_h <= 0:
        return []

    prev_comps = []  # [(loop, bbox)] from the previous real-print-layer slice
    births = []
    n_chunks = max(1, int(np.ceil(total_h / chunk_mm)))
    for ci in range(n_chunks):
        c0 = zmin + ci * chunk_mm
        c1 = min(zmin + (ci + 1) * chunk_mm, zmax)
        heights = list(np.arange(c0 + step / 2, c1, step))
        if not heights:
            continue
        sections = mesh.section_multiplane(
            plane_origin=[0, 0, 0], plane_normal=[0, 0, 1], heights=heights)
        for z, path2d in zip(heights, sections):
            cur_comps = []
            if path2d is not None:
                for loop in path2d.discrete:
                    a = _signed_area_xy(loop)
                    if a < 0 or a < min_area_mm2:
                        continue  # hole, or a sliver below the noise floor
                    cx, cy = float(loop[:, 0].mean()), float(loop[:, 1].mean())
                    cur_comps.append((loop, _bbox_xy(loop), float(a), (cx, cy)))
            for (loop, bb, area, (cx, cy)) in cur_comps:
                overlapped = any(_polys_overlap_xy(loop, bb, ploop, pbb)
                                  for (ploop, pbb, _, _) in prev_comps)
                if not overlapped:
                    births.append(dict(x=cx, y=cy, z=float(z), area_mm2=area,
                                        loop=loop))
            prev_comps = cur_comps
    return births


def delicacy_field(mesh, cfg, n_samples=20000, seed=0):
    """Scar-cost / delicacy weight in [0, 1] per surface sample point,
    returned as (sample_points, sample_normals, weights, query) where
    `query(points)` maps arbitrary points back to the nearest sample's
    weight via a KDTree -- this is what placement/orientation scoring call.

    Proxy for "what the decimation error map would protect" (design.md
    #7): high local curvature (surface detail) and small local thickness
    (fragile thin features -- fins, tendrils) both raise cost. This is
    deliberately standalone (no live Blender/decimation run required, so
    the engine works headless); if a sidecar delicacy map produced by a
    real decimation run exists (see decimate_pipeline.py), a future caller
    can substitute it here without changing anything downstream -- this
    function is the single integration point.
    """
    from scipy.spatial import cKDTree

    # trimesh's sample() draws from the GLOBAL numpy random state (see
    # placement.sample_candidates' identical note) -- seed it explicitly
    # or this field silently varies with whatever ran before it.
    np.random.seed(seed)
    n = min(n_samples, max(2000, len(mesh.faces)))
    points, face_idx = mesh.sample(n, return_index=True)
    normals = mesh.face_normals[face_idx]

    try:
        from trimesh.curvature import discrete_gaussian_curvature_measure
        radius = max(cfg.thin_feature_mm, 0.5)
        # discrete_gaussian_curvature_measure's memory use scales steeply
        # with sample count on a dense mesh (measured on the 576k-face
        # Weeping Treant: ~250MB per 1000 samples -- 20k samples
        # extrapolates past this environment's 3.8GB and OOMs). Compute it
        # on a capped subsample, then propagate to every sample point via
        # nearest-neighbor lookup -- coarser, but this field only needs to
        # rank REGIONS by delicacy, not resolve per-triangle detail.
        n_curv = min(n, 4000)
        curv_idx = np.linspace(0, n - 1, n_curv, dtype=int)
        curv_vals_sub = np.abs(discrete_gaussian_curvature_measure(
            mesh, points[curv_idx], radius))
        c_hi = np.percentile(curv_vals_sub, 95) or 1.0
        curv_score_sub = np.clip(curv_vals_sub / max(c_hi, 1e-9), 0, 1)
        if n_curv < n:
            curv_tree = cKDTree(points[curv_idx])
            _, nn = curv_tree.query(points, workers=-1)
            curv_score = curv_score_sub[nn]
        else:
            curv_score = curv_score_sub
    except Exception:
        curv_score = np.zeros(n)

    # Thickness via nearest OPPOSITE-facing surface point (KDTree over
    # face centers, not ray casting). Ray casting an opposite-wall hit per
    # sample point was tried first and is not viable at real-mesh scale:
    # trimesh's non-embree RayMeshIntersector's memory use did not
    # amortize across a batch (measured: ~50 rays ~830MB RSS, ~200 rays
    # ~2.2GB, OOM by ~500 on a 576k-face mesh in this environment). A
    # KDTree nearest-neighbor search with an opposite-normal filter is a
    # cheaper, coarser proxy for the same "how thin is this locally"
    # question and stays within the same cKDTree machinery already proven
    # fast elsewhere in this module (grounded_mask, island merging).
    try:
        search_r = cfg.thin_feature_mm * 4.0
        face_tree = cKDTree(mesh.triangles_center)
        k = min(40, len(mesh.faces))
        dist, idx = face_tree.query(points, k=k, distance_upper_bound=search_r, workers=-1)
        dist = np.atleast_2d(dist)
        idx = np.atleast_2d(idx)
        cand_normals = mesh.face_normals[np.clip(idx, 0, len(mesh.faces) - 1)]
        opposite = np.einsum('ij,ikj->ik', normals, cand_normals) < -0.3
        valid = np.isfinite(dist) & opposite
        dist_masked = np.where(valid, dist, np.inf)
        thickness = dist_masked.min(axis=1)
        thin_score = np.clip(1.0 - thickness / cfg.thin_feature_mm, 0, 1)
        thin_score[~np.isfinite(thickness)] = 0.0
    except Exception:
        thin_score = np.zeros(n)

    w = (cfg.delicacy_curvature_weight * curv_score
         + cfg.delicacy_thinness_weight * thin_score)
    wmax = w.max() if w.max() > 0 else 1.0
    w = np.clip(w / wmax, 0, 1)

    tree = cKDTree(points)

    def query(pts):
        pts = np.atleast_2d(pts)
        _, i = tree.query(pts, workers=-1)
        return w[i]

    return points, normals, w, query


def exterior_anchor_pool(mesh, geom, cfg):
    """v5 model-anchor candidate pool (SPEC-V5-GEOM sec 2.3): the point
    pool struts.find_model_anchor_candidates draws from, and the pool
    bracing._find_model_rung_anchors' legacy branch reads from -- swapped
    from delicacy_field's seeded random surface cloud to DETERMINISTIC,
    seed-free exterior-face centroids (geom.exterior_face_mask()) once a
    surface-EXACT oracle exists. The former load-bearing role of
    delicacy_field's own sample cloud as the anchor POSITION pool is
    retired by this swap (SPEC sec 2.6's inventory note); delicacy_field
    itself is UNCHANGED and keeps its seed -- its `query` function is
    reused here purely as a SCORING lookup (weights rank candidate
    regions, they are not a geometric measurement), consistent with
    delicacy_field's surviving role everywhere else.

    `geom` is duck-typed on `.exterior_face_mask` (a plain cKDTree, or
    None, is legacy/test input): callers without an exact oracle fall
    back to delicacy_field's own points/weights VERBATIM (zero behavior
    change -- the pre-v5 pool). Returns (points[N,3], delicacy[N]), the
    same shape contract delicacy_field's 1st/3rd return values have."""
    points, _normals, weights, query = delicacy_field(mesh, cfg)
    if geom is None or not hasattr(geom, "exterior_face_mask"):
        return points, weights
    ext = geom.exterior_face_mask()
    cents = geom.centroids[ext]
    if len(cents) == 0:
        return cents, np.zeros(0)
    return cents, query(cents)
