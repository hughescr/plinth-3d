"""Articulated routing and clearance-envelope checking.

design.md #4: "tip stub enters along surface normal (2-3mm), elbow,
vertical descent. Records are polyline chains natively (type-3 joiners);
kinked+braced is native vocabulary." design.md #1: v0 only checked the
ray centerline (27/137 shafts violated standoff, some clipping the
model) -- v2 sweeps the full capsule (segment + radius + standoff).
"""
from dataclasses import dataclass, field
import numpy as np


@dataclass
class Segment:
    p0: np.ndarray
    p1: np.ndarray
    r0: float          # radius at p0
    r1: float           # radius at p1 (tapered if != r0)
    kind: str            # "stub" | "shaft" | "foot"
    embedded_p0: bool = False   # deliberately touching model at p0 (skip near-end clearance)
    embedded_p1: bool = False


@dataclass
class Route:
    contact_idx: int
    segments: list
    anchor_kind: str     # "plate" | "model"
    free_length_mm: float
    n_braces: int = 0

    @property
    def points(self):
        pts = [self.segments[0].p0]
        for s in self.segments:
            pts.append(s.p1)
        return np.array(pts)


def route_to_anchor(contact, anchor_point, r_tip, r_shaft, cfg, anchor_kind="plate",
                     detour_xy=None):
    """Build the articulated polyline for one support: embedded tip ->
    normal stub -> elbow -> (optional lateral detour) -> vertical descent
    -> (jog) -> anchor.

    `detour_xy`, if given, routes the vertical descent through that xy
    instead of straight down from the stub -- a kinked path (design.md
    #4: "kinked+braced is native vocabulary") used when a straight
    descent from this contact's own xy would clip an obstruction directly
    below it (v0's exact failure mode: "27/137 shafts violated 0.6mm
    standoff... some clipping the model", design.md work-list #1).

    `contact` is a dict with 'position' and 'normal' (as produced by
    placement.sample_candidates). Returns a Route.
    """
    pos = np.asarray(contact["position"], float)
    normal = np.asarray(contact["normal"], float)
    normal = normal / (np.linalg.norm(normal) or 1.0)
    anchor = np.asarray(anchor_point, float)

    p_tip = pos - normal * cfg.contact_depth_mm
    p_stub_end = pos + normal * cfg.stub_length_mm

    segs = [Segment(p_tip, p_stub_end, r_tip, r_shaft, "stub", embedded_p0=True)]
    cursor = p_stub_end
    free_length = cfg.stub_length_mm

    if detour_xy is not None:
        p_detour = np.array([detour_xy[0], detour_xy[1], p_stub_end[2]])
        out_len = float(np.linalg.norm(p_detour - cursor))
        if out_len > 1e-9:
            segs.append(Segment(cursor, p_detour, r_shaft, r_shaft, "detour"))
            free_length += out_len
            cursor = p_detour

    p_descend = np.array([cursor[0], cursor[1], anchor[2]])
    shaft_len = float(np.linalg.norm(p_descend - cursor))
    if shaft_len > 1e-9:
        segs.append(Segment(cursor, p_descend, r_shaft, r_shaft, "shaft"))
        free_length += shaft_len
    jog_len = float(np.linalg.norm(anchor - p_descend))
    if jog_len > 1e-9:
        segs.append(Segment(p_descend, anchor, r_shaft, r_shaft, "foot",
                             embedded_p1=(anchor_kind == "model")))
        free_length += jog_len

    return Route(contact_idx=-1, segments=segs, anchor_kind=anchor_kind,
                 free_length_mm=free_length)


def plate_anchor(contact, mesh_zmin, cfg):
    pos = contact["position"]
    z = mesh_zmin - cfg.lift_mm
    return np.array([pos[0], pos[1], z])


def build_surface_tree(mesh, pitch_mm=0.4, max_points=150000, seed=0):
    """Dense point-sample KDTree over the mesh surface, for fast
    approximate clearance queries at real-model scale. Exact
    trimesh.proximity.closest_point measured ~0.4-2s PER CALL on the
    576k-face Weeping Treant (an engine.run pass needs hundreds of these,
    plus dozens more per repair candidate) -- intractable. A dense-enough
    point sample's nearest-neighbor distance approximates true
    point-to-surface distance to within roughly the sample pitch, which is
    small next to cfg.standoff_mm as long as pitch stays well under it.
    Build once per mesh, reuse across every check_clearance call in a run.
    """
    from scipy.spatial import cKDTree
    # trimesh's Trimesh.sample() draws from the GLOBAL numpy random state,
    # not a passed-in Generator -- seed it explicitly here so this tree is
    # actually reproducible (otherwise every call, including once per
    # staged-driver stage and once per engine.run, silently drew a
    # different surface sample depending on whatever ran before it; see
    # placement.sample_candidates' identical note).
    np.random.seed(seed)
    n = int(np.clip(mesh.area / (pitch_mm ** 2), 5000, max_points))
    points = mesh.sample(n)
    return cKDTree(points)


def query_workers(n_points):
    """Thread count for a cKDTree.query of n_points. scipy spawns a FRESH
    thread pool per query call (no reuse); profiled on the 576k-face
    Weeping Treant repair loop, workers=-1 on tiny per-segment queries put
    ~90% of wall time into thread start/join/lock churn (74,535 thread
    spawns for 10.7s of an 10.7s run). Parallelism only pays when the
    query is big enough to amortize pool setup -- below that, single-
    threaded is strictly faster. The batch entry points below exist to
    make queries big on purpose."""
    return -1 if n_points >= 4096 else 1


def _surface_distances(mesh, pts, surface_tree, upper_bound=None):
    """Nearest-surface distance for a batch of points: fast approximate
    KDTree path when a build_surface_tree is supplied, exact (slow)
    trimesh closest_point otherwise.

    `upper_bound`, if given, lets the KDTree prune traversal for points
    farther than it (they come back as inf). Clearance checking only
    needs to know IF a point is within radius+standoff of the surface --
    any point at least that far has clearance >= 0 regardless of its true
    distance, so passing upper_bound = max(radius)+standoff yields
    identical violations while skipping most of the tree walk for the
    (typical) majority of samples hanging in open space."""
    if surface_tree is not None:
        kw = {} if upper_bound is None else dict(distance_upper_bound=float(upper_bound))
        dist, _ = surface_tree.query(pts, workers=query_workers(len(pts)), **kw)
        return dist
    import trimesh
    _, dist, _ = trimesh.proximity.closest_point(mesh, pts)
    return dist


def sample_route_envelope(route, cfg):
    """Sample the swept capsule envelope of every checkable segment of
    `route`: points along each segment with the local (tapered) radius.
    Stub segments are skipped by design -- the stub is rooted against the
    surface it springs from (that's the tip cone's job); design.md's
    standoff violations were shaft violations, not stub ones. Standoff is
    enforced from the elbow onward; a stub that wandered into unrelated
    geometry is still caught because the shaft/foot segments downstream
    inherit its endpoint. Deliberately-embedded endpoints are excluded
    from a short inset so the intentional touch point doesn't register as
    a false violation.

    Returns (pts[N,3], radii[N], seg_ids[N]) with seg_ids indexing
    route.segments. N may be 0.
    """
    step = cfg.clearance_sample_step_mm
    inset = max(cfg.contact_depth_mm, 0.2)
    all_pts, all_radii, all_sid = [], [], []
    for si, seg in enumerate(route.segments):
        if seg.kind == "stub":
            continue
        length = float(np.linalg.norm(seg.p1 - seg.p0))
        if length < 1e-9:
            continue
        n = max(int(length / step), 1)
        t = np.linspace(0, 1, n + 1)
        if seg.embedded_p0:
            t = t[t * length >= inset]
        if seg.embedded_p1 and len(t):
            # an adsorbed model foot (strut anchor) approaches the surface
            # tangentially on purpose -- exclude a longer run-in than the
            # point-contact tip inset or the deliberate touch registers as
            # a violation
            end_inset = max(inset, 2.0) if seg.kind == "shoot" else inset
            t = t[(1 - t) * length >= end_inset]
        if len(t) == 0:
            continue
        all_pts.append(seg.p0[None, :] + t[:, None] * (seg.p1 - seg.p0)[None, :])
        all_radii.append(seg.r0 + t * (seg.r1 - seg.r0))
        all_sid.append(np.full(len(t), si, dtype=int))
    if not all_pts:
        return np.zeros((0, 3)), np.zeros(0), np.zeros(0, dtype=int)
    return np.concatenate(all_pts), np.concatenate(all_radii), np.concatenate(all_sid)


def _violations_from(route, pts, radii, seg_ids, dist, cfg):
    """Per-segment worst-point violations from precomputed surface
    distances (shared by the single and batch clearance checkers)."""
    clearance = dist - radii - cfg.standoff_mm
    violations = []
    for si in np.unique(seg_ids):
        m = seg_ids == si
        c = clearance[m]
        worst = int(np.argmin(c))
        if c[worst] < 0:
            violations.append(dict(segment_kind=route.segments[si].kind,
                                    min_clearance_mm=float(c[worst]),
                                    at_point=pts[m][worst].tolist()))
    return violations


def check_clearance(mesh, route, cfg, surface_tree=None):
    """Sweep every segment of `route` and report the worst (most negative)
    clearance = surface_distance - segment_radius - cfg.standoff_mm.

    `surface_tree`, if given (see build_surface_tree), uses fast
    approximate nearest-surface-point distance instead of exact
    trimesh.proximity.closest_point -- pass one for anything beyond a
    handful of calls on a real-sized mesh (see build_surface_tree's
    docstring for why the exact path doesn't scale).

    Checking MANY routes? Use check_clearance_batch -- one big surface
    query instead of one small one per route.

    Returns list of dicts: {segment_kind, min_clearance_mm, at_point}.
    Empty list = fully clear.
    """
    pts, radii, seg_ids = sample_route_envelope(route, cfg)
    if len(pts) == 0:
        return []
    dist = _surface_distances(mesh, pts, surface_tree,
                               upper_bound=float(radii.max()) + cfg.standoff_mm)
    return _violations_from(route, pts, radii, seg_ids, dist, cfg)


def check_clearance_batch(mesh, routes, cfg, surface_tree=None):
    """check_clearance over many routes with ONE concatenated surface
    query. This is where the multi-core actually happens: a single query
    over all routes' sample points is large enough for query_workers to
    hand scipy's C-level thread pool real work, instead of paying pool
    setup per tiny per-segment query (see query_workers for the measured
    cost of getting this wrong).

    Returns a list of violation-lists, parallel to `routes`.
    """
    samples = [sample_route_envelope(r, cfg) for r in routes]
    counts = [len(s[0]) for s in samples]
    if sum(counts) == 0:
        return [[] for _ in routes]
    pts_all = np.concatenate([s[0] for s in samples if len(s[0])])
    ub = max(float(s[1].max()) for s in samples if len(s[0])) + cfg.standoff_mm
    dist_all = _surface_distances(mesh, pts_all, surface_tree, upper_bound=ub)
    out, offset = [], 0
    for route, (pts, radii, seg_ids), n in zip(routes, samples, counts):
        if n == 0:
            out.append([])
            continue
        out.append(_violations_from(route, pts, radii, seg_ids,
                                     dist_all[offset:offset + n], cfg))
        offset += n
    return out
