"""Unit tests for the v5 exact-geometry core (geometry_exact.ExactSurface).

SPEC: docs/specs/SPEC-V5-GEOM.md sec 6 (test plan). Nothing consumes the module
yet -- these test the oracle in isolation. All fixtures deterministic, no
unseeded sampling. The micro-benchmark (test_microbenchmark_reported) prints
numbers and never gates on wall time (Craig's correctness-over-speed ruling).
"""
import time

import numpy as np
import trimesh
import pytest

from support_placement import geometry_exact as G
from support_placement import mesh_io


# --------------------------------------------------------------------------
# fixtures (deterministic, triangle-soup where non-watertightness matters)
# --------------------------------------------------------------------------
def _soup(*meshes):
    """Concatenate meshes into a single unmerged triangle soup (process=False)
    -- the real Weeping Treant is a non-watertight overlapping assembly."""
    merged = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=merged.vertices.copy(),
                           faces=merged.faces.copy(), process=False)


def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def _two_box(gap_center=6.0):
    """Two overlapping 10mm boxes: A at x in [-5,5], B at x in
    [gap_center-5, gap_center+5]. Overlap when gap_center < 10."""
    A = trimesh.creation.box(extents=(10, 10, 10))
    B = trimesh.creation.box(extents=(10, 10, 10)); B.apply_translation((gap_center, 0, 0))
    return _soup(A, B)


def _brute_dist(gs, pts):
    """Reference: exact unsigned point-to-surface distance by min over ALL
    triangles (closest_on_tri). O(pts x faces) -- only for small fixtures.
    Face-chunked so the O(faces) intermediate never spikes memory."""
    pts = np.atleast_2d(pts)
    A = gs.V0; B = gs.V0 + gs.E1; C = gs.V0 + gs.E2
    nf = len(A)
    out = np.empty(len(pts))
    for i, p in enumerate(pts):
        best = np.inf
        for s in range(0, nf, 4000):
            Ab = A[s:s + 4000]; Bb = B[s:s + 4000]; Cb = C[s:s + 4000]
            P = np.broadcast_to(p, Ab.shape)
            cpt = G.closest_on_tri(P, Ab, Bb, Cb)
            best = min(best, float(np.linalg.norm(P - cpt, axis=1).min()))
        out[i] = best
    return out


# --------------------------------------------------------------------------
# 1. thin-twig ball breach (census worst case, ball 715 class)
# --------------------------------------------------------------------------
def test_thin_twig_ball_breach_flagged():
    twig = _box((6, 6, 0.08))                     # 0.08mm-thick plate at z in [-0.04,0.04]
    gs = G.ExactSurface(twig)
    breached, thick, poke = gs.sphere_breach(np.array([[0, 0, 0.04]]), np.array([1.0]))
    assert bool(breached[0]) is True
    assert abs(thick[0] - 0.08) < 0.02          # measured far-wall thickness
    assert poke[0] > 0.8                          # r - thickness ~ 0.92


def test_thick_slab_ball_anchor_passes():
    slab = _box((20, 20, 5))                       # 5mm-thick slab, top face z=2.5
    gs = G.ExactSurface(slab)
    breached, thick, poke = gs.sphere_breach(np.array([[0, 0, 2.5]]), np.array([1.0]))
    assert bool(breached[0]) is False             # sanctioned shallow anchor (150/194 census class)
    assert poke[0] == 0.0


# --------------------------------------------------------------------------
# 2. sanctioned stub embed vs stub driven through a fin
# --------------------------------------------------------------------------
def test_sanctioned_stub_embed_confined_to_embed_zone():
    slab = _box((20, 20, 5))                       # top face z=2.5
    gs = G.ExactSurface(slab)
    # stub tip 0.3mm inside (z=2.2), exits +z along the normal; the only inside
    # interval must sit within the p0 embed zone (arc length ~ contact_depth).
    ivs = gs.penetration_intervals(np.array([0, 0, 2.2]), np.array([0, 0, 5.2]), 0.3, 0.3)
    assert len(ivs) == 1
    iv = ivs[0]
    assert iv["s_a"] < 1e-6 and iv["s_b"] < 0.4   # embed zone only, near s=0
    # the consumer's inset (contact_depth + r0 + standoff) exempts this zone;
    # nothing illegal outside it.
    inset0 = 0.3 + 0.3 + 0.6
    illegal_outside = max(0.0, iv["s_b"] - max(iv["s_a"], inset0))
    assert illegal_outside <= 0.05                # census MIN_ILLEGAL_LEN


def test_stub_through_fin_flagged():
    fin = _box((0.5, 10, 10))                      # 0.5mm-thick fin, x in [-0.25,0.25]
    gs = G.ExactSurface(fin)
    ivs = gs.penetration_intervals(np.array([-3.0, 0, 0]), np.array([3.0, 0, 0]), 0.3, 0.3)
    assert len(ivs) == 1
    iv = ivs[0]
    assert iv["entered"] and iv["exited"]          # through-and-through
    assert (iv["s_b"] - iv["s_a"]) > 0.05          # illegal length > MIN_ILLEGAL_LEN
    assert abs((iv["s_b"] - iv["s_a"]) - 0.5) < 0.05   # ~ fin thickness


# --------------------------------------------------------------------------
# 3. strut end-embed vs mid-path clip (segment_clearance)
# --------------------------------------------------------------------------
def test_strut_end_embed_passes_but_midpath_clip_fails():
    # slab whose top is z=0; a strut descends from above and embeds its p1 end.
    slab = _box((20, 20, 5), center=(0, 0, -2.5))  # top face z=0
    gs = G.ExactSurface(slab)
    # end-embed: vertical strut from z=8 down to z=-0.3 (embeds 0.3 into slab).
    # inset1 exempts the embedded end; the free span is clear.
    p0 = np.array([[0, 0, 8.0]]); p1 = np.array([[0, 0, -0.3]])
    inset1 = np.array([0.3 + 0.6 + 0.6])           # contact_depth + r_end + standoff
    clr, _ = gs.segment_clearance(p0, p1, 0.6, 0.6, 0.6, inset1s=inset1)
    # The inset formula places the free-span end exactly r+standoff from the
    # surface by construction, so a straight normal embed sits at clearance ~0;
    # the point is it is NOT a real (negative) violation.
    assert clr[0] > -1e-6                            # sanctioned embed passes

    # mid-path clip: add a thin 0.2mm fin standing in the strut's free span,
    # finer than any old cloud pitch -- the case the sampled check read as 0.
    fin = _box((0.2, 10, 4), center=(0, 0, 5.0))    # fin around z in [3,7], x in [-0.1,0.1]
    gs2 = G.ExactSurface(_soup(slab, fin))
    clr2, _ = gs2.segment_clearance(p0, p1, 0.6, 0.6, 0.6, inset1s=inset1)
    assert clr2[0] < 0                              # mid-path penetration flagged


# --------------------------------------------------------------------------
# 4. parity vote + exterior mask on a non-watertight two-box assembly
# --------------------------------------------------------------------------
def test_parity_vote_two_box_soup():
    gs = G.ExactSurface(_two_box(gap_center=6.0))   # overlap x in [1,5]
    pts = np.array([
        [-3, 0, 0],   # inside A only
        [9, 0, 0],    # inside B only
        [3, 0, 0],    # inside the overlap (both)
        [0, 8, 0],    # outside both
        [5.0, 0, 0],  # ON A's +x face but INSIDE B -- signed-distance-by-face gets this wrong
    ])
    got = gs.inside(pts)
    assert list(got) == [True, True, True, False, True]


def test_exterior_face_mask_two_box_soup():
    gs = G.ExactSurface(_two_box(gap_center=6.0))
    mask = gs.exterior_face_mask()
    cent = gs.centroids; fn = gs.face_normals
    # A's +x face (x=5, normal +x) is buried inside B -> interior (mask False)
    a_buried = np.where((np.abs(cent[:, 0] - 5) < 1e-6) & (fn[:, 0] > 0.9))[0]
    assert len(a_buried) > 0 and (~mask[a_buried]).all()
    # B's -x face (x=1, normal -x) is buried inside A -> interior
    b_buried = np.where((np.abs(cent[:, 0] - 1) < 1e-6) & (fn[:, 0] < -0.9))[0]
    assert len(b_buried) > 0 and (~mask[b_buried]).all()
    # every outer-shell face is exterior
    outer = np.where((np.abs(cent[:, 0] + 5) < 1e-6) & (fn[:, 0] < -0.9))[0]
    assert len(outer) > 0 and mask[outer].all()


# --------------------------------------------------------------------------
# 5. interior demand phantom kill (mask marks a buried down-face interior)
# --------------------------------------------------------------------------
def test_interior_downface_marked_interior():
    # Box A with a smaller box B fully overlapping A's top region so A's top
    # face is buried inside B. A's top face is down/up-facing but interior ->
    # perception must never seed demand there (mask False).
    A = trimesh.creation.box(extents=(10, 10, 10))                 # z in [-5,5]
    # B fully covers A's top face in x,y (12 > 10) so A's whole top face is
    # buried inside B -- a fully-buried face, not a partially-exposed one.
    B = trimesh.creation.box(extents=(12, 12, 10)); B.apply_translation((0, 0, 8))  # z in [3,13]
    gs = G.ExactSurface(_soup(A, B))
    cent = gs.centroids; fn = gs.face_normals
    # A's top face at z=5 is inside B -> interior (both its triangles)
    a_top_buried = np.where((np.abs(cent[:, 2] - 5) < 1e-6) & (fn[:, 2] > 0.9))[0]
    mask = gs.exterior_face_mask()
    assert len(a_top_buried) > 0
    assert (~mask[a_top_buried]).all()


# --------------------------------------------------------------------------
# 6. exact segment-vs-triangle distance in closed-form-checkable positions
# --------------------------------------------------------------------------
def test_exact_segment_distance_closed_form():
    # single triangle in the z=0 plane: (0,0,0),(4,0,0),(0,4,0)
    tri = trimesh.Trimesh(vertices=np.array([[0, 0, 0], [4, 0, 0], [0, 4, 0.0]]),
                          faces=np.array([[0, 1, 2]]), process=False)
    gs = G.ExactSurface(tri)
    faces = np.array([0])
    # (a) endpoint nearest: segment hovering above vertex (0,0,0) at height 2
    d, _ = gs._seg_tri_dist(np.array([0, 0, 2.0]), np.array([0, 0, 5.0]), faces)
    assert abs(d[0] - 2.0) < 1e-9
    # (b) edge nearest: segment parallel to +y beyond the x=4 vertex, dist to
    # the hypotenuse edge. Point (3,3,0) is on the hypotenuse (x+y=... no; use
    # a clean edge case): segment far out on +x, nearest is vertex (4,0,0).
    d2, _ = gs._seg_tri_dist(np.array([7, 0, 0.0]), np.array([7, 0, 3.0]), faces)
    assert abs(d2[0] - 3.0) < 1e-9               # dist from x=7 to vertex x=4
    # (c) plane piercing: segment through the triangle interior -> distance 0
    d3, _ = gs._seg_tri_dist(np.array([1, 1, -2.0]), np.array([1, 1, 2.0]), faces)
    assert d3[0] < 1e-9
    # (d) parallel hover over the interior -> perpendicular distance = height
    d4, _ = gs._seg_tri_dist(np.array([-2, 1, 1.5]), np.array([3, 1, 1.5]), faces)
    assert abs(d4[0] - 1.5) < 1e-9


# --------------------------------------------------------------------------
# 8. anchor exactness via cast_rays (first exterior crossing on the surface)
# --------------------------------------------------------------------------
def test_cast_rays_anchor_on_surface():
    gs = G.ExactSurface(_box((10, 10, 10)))        # faces at +-5
    # ray from outside (8,0,0) aimed at the +x face along -x, reaching past it
    O = np.array([[8.0, 0, 0]]); D = np.array([[-16.0, 0, 0]])   # point = O + t*D, t in [0,1]
    res = gs.cast_rays(O, D, t_max=1.0)
    ts, faces = res[0]
    assert len(ts) >= 1
    first = O[0] + ts[0] * D[0]
    # first crossing is the +x face at x=5
    assert abs(first[0] - 5.0) < 1e-6
    # the hit point lies ON a mesh triangle (distance to surface ~ 0)
    d = _brute_dist(gs, first[None, :])[0]
    assert d < 1e-6


def test_cast_rays_no_hit_returns_empty():
    gs = G.ExactSurface(_box((10, 10, 10)))
    O = np.array([[8.0, 0, 0]]); D = np.array([[0, 0, 1.0]])   # parallel miss, upward
    res = gs.cast_rays(O, D, t_max=1.0)
    ts, faces = res[0]
    assert len(ts) == 0


# --------------------------------------------------------------------------
# 9. duck-type facade: geom.query vs brute-force exact distance
# --------------------------------------------------------------------------
def test_query_matches_brute_force_exact():
    sph = trimesh.creation.icosphere(subdivisions=3, radius=5.0)
    sph = trimesh.Trimesh(vertices=sph.vertices.copy(), faces=sph.faces.copy(), process=False)
    gs = G.ExactSurface(sph)
    rng = np.random.RandomState(0)
    pts = rng.uniform(-8, 8, size=(200, 3))
    d, ti = gs.query(pts)
    ref = _brute_dist(gs, pts)
    assert np.allclose(d, ref, atol=1e-9)
    assert (ti >= 0).all() and (ti < gs.nfaces).all()


def test_query_distance_upper_bound_contract():
    gs = G.ExactSurface(_box((10, 10, 10)))
    # point at (100,0,0): nearest surface ~95mm; with a 10mm bound -> inf, idx=n
    d, ti = gs.query(np.array([[100.0, 0, 0]]), distance_upper_bound=10.0)
    assert not np.isfinite(d[0])
    assert ti[0] == gs.nfaces


# --------------------------------------------------------------------------
# 15. query exactness certificate (nearest face's centroid ranks beyond k)
# --------------------------------------------------------------------------
def test_query_certificate_escalation():
    # Build a cluster of many tiny triangles around a query point, plus ONE
    # medium (~1.2mm) triangle that is actually NEAREST but whose centroid
    # ranks beyond k=64. query() must still return the true distance.
    verts = []
    faces = []
    rng = np.random.RandomState(1)
    # 120 tiny triangles on a shell at radius ~0.8 from origin (centroids ~0.8)
    for _ in range(120):
        c = rng.normal(size=3); c = c / np.linalg.norm(c) * 0.8
        e = rng.normal(size=(3, 3)) * 0.01
        base = len(verts)
        verts.extend((c + e).tolist())
        faces.append([base, base + 1, base + 2])
    # one medium triangle whose nearest EDGE sits 0.3 from origin but whose
    # centroid is ~0.9 away (big triangle, extent ~0.85 < large_thr)
    base = len(verts)
    verts.extend([[0.3, -0.9, 0.0], [0.3, 0.9, 0.0], [1.9, 0.0, 0.0]])
    faces.append([base, base + 1, base + 2])
    mesh = trimesh.Trimesh(vertices=np.array(verts, float), faces=np.array(faces, int),
                           process=False)
    gs = G.ExactSurface(mesh)
    q = np.array([[0.0, 0.0, 0.0]])
    d, ti = gs.query(q)
    ref = _brute_dist(gs, q)[0]
    assert abs(d[0] - ref) < 1e-9
    assert abs(d[0] - 0.3) < 1e-9                  # the medium triangle wins
    assert ti[0] == gs.nfaces - 1                  # the medium triangle's index


# --------------------------------------------------------------------------
# 13. exterior-mask gap robustness: outer shell stays exterior across gaps
# --------------------------------------------------------------------------
@pytest.mark.parametrize("gap", [0.02, 0.1, 0.3])
def test_exterior_mask_gap_sweep(gap):
    # box A (x in [-5,5]) with box B fully buried inside it: B spans x in
    # [-3, 5-gap] so B's +x wall sits `gap` mm inside A's +x face at x=5, and
    # B's -x wall (x=-3) is deep inside A. Sweep the inter-shell gap across the
    # eps ladder {0.05, 0.15}: the outer shell must stay exterior, the buried
    # wall interior, at every gap.
    A = trimesh.creation.box(extents=(10, 10, 10))                 # x in [-5,5]
    b_lo, b_hi = -3.0, 5.0 - gap
    bx = b_hi - b_lo
    B = trimesh.creation.box(extents=(bx, 8, 8))
    B.apply_translation((0.5 * (b_lo + b_hi), 0, 0))               # +x wall at 5-gap, -x wall at -3
    gs = G.ExactSurface(_soup(A, B))
    mask = gs.exterior_face_mask()
    cent = gs.centroids; fn = gs.face_normals
    # A's outer shell faces MUST stay exterior at every gap
    outer_negx = np.where((np.abs(cent[:, 0] + 5) < 1e-6) & (fn[:, 0] < -0.9))[0]
    assert len(outer_negx) and mask[outer_negx].all()
    outer_posx = np.where((np.abs(cent[:, 0] - 5) < 1e-6) & (fn[:, 0] > 0.9))[0]
    assert len(outer_posx) and mask[outer_posx].all()
    # B's buried -x wall (x=-3, deep inside A) MUST stay interior
    b_negx = np.where((np.abs(cent[:, 0] - b_lo) < 1e-6) & (fn[:, 0] < -0.9))[0]
    assert len(b_negx) and (~mask[b_negx]).all()


# --------------------------------------------------------------------------
# 14. embedded end-cap breach (ball-less strut end that transits a thin fin)
# --------------------------------------------------------------------------
def test_endcap_breach_thin_fin_vs_thick_slab():
    # A synthesized end-cap sphere at a strut's embedded end sitting on the near
    # wall of a 0.3mm fin (mirrors the census anchor-foot geometry: center at
    # the surface, far wall within the ball radius).
    fin = _box((10, 10, 0.3))                       # 0.3mm-thick, top face z=0.15
    gs = G.ExactSurface(fin)
    breached, thick, poke = gs.sphere_breach(np.array([[0, 0, 0.15]]), np.array([0.6]))
    assert bool(breached[0]) is True and poke[0] > 0.05
    assert abs(thick[0] - 0.3) < 0.03               # far-wall thickness ~ fin thickness
    # same end-cap on a 5mm slab -> passes (thick feature)
    slab = _box((10, 10, 5))
    gs2 = G.ExactSurface(slab)
    b2, t2, p2 = gs2.sphere_breach(np.array([[0, 0, 2.5]]), np.array([0.6]))
    assert bool(b2[0]) is False


# --------------------------------------------------------------------------
# batching / memory-bound test (task deliverable)
# --------------------------------------------------------------------------
def test_batching_and_memory_bound(monkeypatch):
    """Force a tiny ROW_BUDGET so the product-bounded blocking path is
    exercised, and assert (a) results stay EXACT and (b) RSS stays under the
    2GB hard cap across many queries. Guards the memory contract that a prior
    agent's OOM (173GB) motivated."""
    sph = trimesh.creation.icosphere(subdivisions=4, radius=10.0)   # ~5k faces
    sph = trimesh.Trimesh(vertices=sph.vertices.copy(), faces=sph.faces.copy(), process=False)
    gs = G.ExactSurface(sph)
    rng = np.random.RandomState(0)
    pts = rng.uniform(-15, 15, size=(3000, 3))
    # tiny budget -> _closest_over_candidates must block into many chunks
    monkeypatch.setattr(G, "ROW_BUDGET", 5000)
    base = G.rss_gb()
    d, ti = gs.query(pts)
    grew = G.rss_gb() - base                         # growth caused by this op
    ref = _brute_dist(gs, pts[:300])
    assert np.allclose(d[:300], ref, atol=1e-9)     # still exact under blocking
    assert grew < G.RSS_LIMIT_GB                     # bounded growth (memory contract)
    # inside() batching path over many points
    ins = gs.inside(pts, batch=256)
    ref_in = np.linalg.norm(pts, axis=1) < 10.0
    # icosphere is faceted; only assert on points comfortably in/out of r=10
    clear = (np.abs(np.linalg.norm(pts, axis=1) - 10.0) > 1.0)
    assert (ins[clear] == ref_in[clear]).all()


def test_rss_helper_and_build_log():
    gs = G.ExactSurface(_box((10, 10, 10)))
    assert G.rss_gb() > 0
    log = gs.build_log
    assert log["nfaces"] == 12
    assert "large_thr" in log and "tri_extent_nonlarge" in log
    # for a small mesh (< large_cap) the never-miss set is empty
    assert log["n_large"] == 0


def test_large_set_neverset_stays_exact():
    """A mesh with a genuine size-outlier tail (many small triangles + a few
    huge ones) must (a) populate a non-empty `large` never-miss set that
    tightens tri_extent_nonlarge, and (b) still return EXACT nearest distances
    for points nearest a huge triangle whose centroid is far away."""
    verts = []; faces = []
    rng = np.random.RandomState(2)
    # 3000 small triangles scattered on a shell at radius ~20
    for _ in range(3000):
        c = rng.normal(size=3); c = c / np.linalg.norm(c) * 20.0
        e = rng.normal(size=(3, 3)) * 0.05
        base = len(verts); verts.extend((c + e).tolist()); faces.append([base, base + 1, base + 2])
    # a few HUGE triangles (extent ~15) forming a slab near the origin plane z=1
    for (x0) in (-30, 0, 30):
        base = len(verts)
        verts.extend([[x0 - 15, -15, 1.0], [x0 + 15, -15, 1.0], [x0, 15, 1.0]])
        faces.append([base, base + 1, base + 2])
    mesh = trimesh.Trimesh(vertices=np.array(verts, float), faces=np.array(faces, int), process=False)
    gs = G.ExactSurface(mesh, large_cap=2000)
    assert len(gs.large) >= 1                       # the huge slab triangles are never-miss
    assert gs.tri_extent_nonlarge < gs.tri_extent_global   # the tail was excluded from the bound
    # a point just above the origin: nearest surface is the huge slab at z=1,
    # whose centroid is ~0 in xy but the triangle spans far -- exact distance 1.
    q = np.array([[0.0, 0.0, 3.0]])
    d, ti = gs.query(q)
    ref = _brute_dist(gs, q)[0]
    assert abs(d[0] - ref) < 1e-9
    assert abs(d[0] - 2.0) < 1e-9                    # 3.0 - 1.0


def test_large_threshold_derivation_keeps_neverset_small():
    # a mesh with > large_cap faces: large_thr derived so |large| <= large_cap
    sph = trimesh.creation.icosphere(subdivisions=5, radius=10.0)   # ~20k faces
    sph = trimesh.Trimesh(vertices=sph.vertices.copy(), faces=sph.faces.copy(), process=False)
    gs = G.ExactSurface(sph, large_cap=500)
    assert len(gs.large) <= 500
    assert gs.tri_extent_nonlarge <= gs.tri_extent_global + 1e-9


# --------------------------------------------------------------------------
# 10. micro-benchmark (reported, NOT gated on wall time)
# --------------------------------------------------------------------------
def test_microbenchmark_reported(capsys):
    """Query throughput of the exact core vs the old sampled-cloud path, on a
    realistic-density mesh, plus the Tier-1 open-space reject rate on a
    revalidate-like segment trace. Printed via the timing-log convention so
    regressions are visible without gating on wall time (SPEC sec 5 / 6.10)."""
    from support_placement import routing

    sph = trimesh.creation.icosphere(subdivisions=5, radius=12.0)   # ~20k faces
    sph = trimesh.Trimesh(vertices=sph.vertices.copy(), faces=sph.faces.copy(), process=False)

    t0 = time.time()
    gs = G.ExactSurface(sph)
    t_build = time.time() - t0

    rng = np.random.RandomState(0)
    N = 5000
    pts = rng.uniform(-18, 18, size=(N, 3))

    # exact query throughput
    t0 = time.time()
    gs.query(pts)
    t_exact = time.time() - t0
    qps_exact = N / t_exact

    # old sampled-cloud path (build_surface_tree + cKDTree.query)
    t0 = time.time()
    stree = routing.build_surface_tree(sph)
    t_cloud_build = time.time() - t0
    t0 = time.time()
    stree.query(pts, workers=-1)
    t_cloud = time.time() - t0
    qps_cloud = N / t_cloud

    # Tier-1 open-space reject rate on a revalidate-like segment batch: random
    # segments, most in open space away from the shell.
    M = 400
    p0 = rng.uniform(-25, 25, size=(M, 3))
    dirs = rng.normal(size=(M, 3)); dirs /= np.linalg.norm(dirs, axis=1, keepdims=True)
    p1 = p0 + dirs * rng.uniform(3, 12, size=(M, 1))
    t0 = time.time()
    clr, _ = gs.segment_clearance(p0, p1, 0.3, 0.6, 0.6)
    t_seg = time.time() - t0
    # a segment is Tier-1 certified when its local crop is empty; approximate
    # the certify rate as segments comfortably clear of the shell.
    certified = np.isfinite(clr) & (clr > 2.0)
    tier1_rate = float(certified.mean())

    # projected full-scale impact: the cloud full run is ~194s; the exact
    # per-query cost ratio projects the geometry-query portion.
    ratio = t_exact / max(t_cloud, 1e-9)

    lines = [
        "",
        "=== geometry_exact micro-benchmark (icosphere ~%d faces) ===" % len(sph.faces),
        "build:   exact=%.3fs  cloud(build_surface_tree)=%.3fs" % (t_build, t_cloud_build),
        "query:   exact=%.0f q/s   cloud=%.0f q/s   (exact/cloud time ratio=%.1fx)"
        % (qps_exact, qps_cloud, ratio),
        "         exact %d pts in %.3fs ; cloud %d pts in %.3fs" % (N, t_exact, N, t_cloud),
        "segment_clearance: %d segments in %.3fs (%.0f seg/s), Tier-1 open-space "
        "reject rate=%.2f" % (M, t_seg, M / t_seg, tier1_rate),
        "ExactSurface build_log: %s" % gs.build_log,
        "projection: exact queries cost ~%.1fx the cloud per query; with Tier-1 "
        "absorbing the open-space majority and the interval test gated, SPEC's "
        "~400-900s full-scale estimate is the operative bound (validate on the "
        "real Treant run)." % ratio,
        "==========================================================",
    ]
    report = "\n".join(lines)
    with capsys.disabled():
        print(report)
    # sanity only -- never gate on wall time
    assert qps_exact > 0 and qps_cloud > 0
