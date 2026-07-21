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
