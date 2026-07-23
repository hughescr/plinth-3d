"""Permanent differential/parity suite: geom_kernel (Rust) vs the frozen
ExactSurface oracle (SPEC-RUST-KERNEL.md sec 6). Marked @pytest.mark.kernel so
the conftest gate FAILS (never silently skips) on a missing/stale kernel.

The Python ExactSurface is imported ONLY here (and in its own unit tests): it is
now the frozen reference, not production code. Distances agree within 1e-9
relative (bit-identical where the math is exact); boolean verdicts agree EXACTLY
except at measured ties (documented, telemetered, never silent).
"""
import os

import numpy as np
import trimesh
import pytest

from support_placement import geometry_exact as GE
from support_placement import geometry_kernel as GK
from support_placement import mesh_io

pytestmark = pytest.mark.kernel


# --------------------------------------------------------------------------
# fixtures (mirror test_geometry_exact.py)
# --------------------------------------------------------------------------
def _soup(*meshes):
    merged = trimesh.util.concatenate(list(meshes))
    return trimesh.Trimesh(vertices=merged.vertices.copy(),
                           faces=merged.faces.copy(), process=False)


def _box(size, center=(0, 0, 0)):
    b = trimesh.creation.box(extents=size)
    b.apply_translation(center)
    return trimesh.Trimesh(vertices=b.vertices.copy(), faces=b.faces.copy(),
                           process=False)


def _two_box(gap_center=6.0):
    A = trimesh.creation.box(extents=(10, 10, 10))
    B = trimesh.creation.box(extents=(10, 10, 10)); B.apply_translation((gap_center, 0, 0))
    return _soup(A, B)


def _both(mesh):
    return GE.ExactSurface(mesh), GK.KernelSurface(mesh)


def _rel(a, b):
    return abs(a - b) / max(1.0, abs(b))


FIXTURES = {
    "box": lambda: _box((10, 10, 10)),
    "overhang_table": mesh_io.make_overhang_table,
    "thin_fin": mesh_io.make_thin_fin,
    "toed_slab": mesh_io.make_toed_slab,
    "raking_blade": mesh_io.make_raking_blade,
    "two_box": lambda: _two_box(6.0),
    "twig": lambda: _box((6, 6, 0.08)),
    "thick_slab": lambda: _box((20, 20, 5)),
}


# ==========================================================================
# 1. FIXTURE BATTERY — pinned scenario verdicts identical on oracle + kernel
# ==========================================================================
def test_thin_twig_breach_both():
    twig = _box((6, 6, 0.08))
    o, k = _both(twig)
    c = np.array([[0, 0, 0.04]]); r = np.array([1.0])
    bo = o.sphere_breach(c, r); bk = k.sphere_breach(c, r)
    assert bool(bo[0][0]) is True and bool(bk[0][0]) is True
    assert _rel(bo[1][0], bk[1][0]) <= 1e-9
    assert _rel(bo[2][0], bk[2][0]) <= 1e-9


def test_thick_slab_anchor_both():
    o, k = _both(_box((20, 20, 5)))
    c = np.array([[0, 0, 2.5]]); r = np.array([1.0])
    assert bool(o.sphere_breach(c, r)[0][0]) is False
    assert bool(k.sphere_breach(c, r)[0][0]) is False


def test_sanctioned_stub_embed_both():
    o, k = _both(_box((20, 20, 5)))
    p0 = np.array([0, 0, 2.2]); p1 = np.array([0, 0, 5.2])
    io = o.penetration_intervals(p0, p1, 0.3, 0.3)
    ik = k.penetration_intervals(p0, p1, 0.3, 0.3)
    assert len(io) == len(ik) == 1
    assert _rel(io[0]["s_a"], ik[0]["s_a"]) <= 1e-9 or abs(io[0]["s_a"] - ik[0]["s_a"]) < 1e-9
    assert _rel(io[0]["s_b"], ik[0]["s_b"]) <= 1e-9
    assert io[0]["entered"] == ik[0]["entered"] and io[0]["exited"] == ik[0]["exited"]


def test_stub_through_fin_both():
    o, k = _both(_box((0.5, 10, 10)))
    p0 = np.array([-3.0, 0, 0]); p1 = np.array([3.0, 0, 0])
    io = o.penetration_intervals(p0, p1, 0.3, 0.3)
    ik = k.penetration_intervals(p0, p1, 0.3, 0.3)
    assert len(io) == len(ik) == 1
    assert io[0]["entered"] == ik[0]["entered"] and io[0]["exited"] == ik[0]["exited"]
    assert _rel(io[0]["s_b"] - io[0]["s_a"], ik[0]["s_b"] - ik[0]["s_a"]) <= 1e-9


def test_strut_end_embed_and_midpath_clip_both():
    slab = _box((20, 20, 5), center=(0, 0, -2.5))
    o, k = _both(slab)
    p0 = np.array([[0, 0, 8.0]]); p1 = np.array([[0, 0, -0.3]])
    inset1 = np.array([0.3 + 0.6 + 0.6])
    co, _ = o.segment_clearance(p0, p1, 0.6, 0.6, 0.6, inset1s=inset1)
    ck, _ = k.segment_clearance(p0, p1, 0.6, 0.6, 0.6, inset1s=inset1)
    assert (co[0] > -1e-6) == (ck[0] > -1e-6)  # both sanctioned-embed pass
    fin = _box((0.2, 10, 4), center=(0, 0, 5.0))
    o2, k2 = _both(_soup(slab, fin))
    co2, _ = o2.segment_clearance(p0, p1, 0.6, 0.6, 0.6, inset1s=inset1)
    ck2, _ = k2.segment_clearance(p0, p1, 0.6, 0.6, 0.6, inset1s=inset1)
    assert (co2[0] < 0) and (ck2[0] < 0)  # both flag mid-path clip


def test_two_box_parity_vote_both():
    o, k = _both(_two_box(6.0))
    pts = np.array([[-3, 0, 0], [9, 0, 0], [3, 0, 0], [0, 8, 0], [5.0, 0, 0]])
    go = o.inside(pts); gk = k.inside(pts)
    assert list(go) == [True, True, True, False, True]
    assert list(gk) == list(go)


def test_exterior_mask_two_box_exact():
    o, k = _both(_two_box(6.0))
    mo = o.exterior_face_mask(); mk = k.exterior_face_mask()
    assert np.array_equal(mo, mk)  # per-face boolean vector EXACTLY equal


def test_exterior_mask_all_fixtures_exact():
    for name, fn in FIXTURES.items():
        o, k = _both(fn())
        assert np.array_equal(o.exterior_face_mask(), k.exterior_face_mask()), name


# ---- Amendment A2: chained-crossing parity dedup (cluster count) ----------
def test_chained_crossing_parity_cluster_count():
    # three parallel coincident z-walls < zdedup (5e-3) apart, as triangle soup
    walls = []
    for z in (0.0, 0.004, 0.008):
        walls.append(_box((4, 4, 1e-6), center=(0, 0, z)))
    mesh = _soup(*walls)
    o, k = _both(mesh)
    # probe below all three walls: 1 crossing cluster (odd => inside on that axis)
    pts = np.array([[0, 0, -0.5], [0, 0, 0.5], [0, 0, 0.002]])
    go = o.inside(pts); gk = k.inside(pts)
    assert list(go) == list(gk)


# ---- Amendment A13: representative face + anchor selection ----------------
def test_cast_rays_anchor_first_exterior_both():
    o, k = _both(_box((10, 10, 10)))
    O = np.array([[8.0, 0, 0]]); D = np.array([[-16.0, 0, 0]])
    ro = o.cast_rays(O, D, 1.0)[0]; rk = k.cast_rays(O, D, 1.0)[0]
    assert len(ro[0]) == len(rk[0]) and len(ro[0]) >= 1
    assert _rel(ro[0][0], rk[0][0]) <= 1e-9
    fo = O[0] + ro[0][0] * D[0]; fk = O[0] + rk[0][0] * D[0]
    assert abs(fo[0] - 5.0) < 1e-6 and abs(fk[0] - 5.0) < 1e-6


# ---- Amendment A14: Tier-1 gate fixture ----------------------------------
def test_tier1_open_space_deviation_verdict_preserving():
    # a segment far from a small box: oracle Tier-1 (NaN argmin, positive cert),
    # kernel exact. Both must be strictly positive (clear), clr_k <= clr_o.
    o, k = _both(_box((2, 2, 2)))
    p0 = np.array([[20.0, 20.0, 20.0]]); p1 = np.array([[20.0, 20.0, 25.0]])
    co, ao = o.segment_clearance(p0, p1, 0.5, 0.5, 0.5)
    ck, ak = k.segment_clearance(p0, p1, 0.5, 0.5, 0.5)
    tier1 = bool(np.isnan(ao[0]).any()) and np.isfinite(co[0]) and co[0] > 0
    assert tier1, "expected oracle Tier-1 signature (NaN argmin, positive cert)"
    assert ck[0] > 0                      # kernel also clear (verdict preserved)
    assert ck[0] <= co[0] + 1e-9          # one-directional ordering


# ---- Amendment A12: nearest-tie pins --------------------------------------
def test_query_upper_bound_hit_at_exact_bound():
    o, k = _both(_box((10, 10, 10)))
    # nearest surface dist from (0,0,20) is 15 exactly; bound=15 must RETURN it
    d, ti = k.query(np.array([[0.0, 0, 20]]), distance_upper_bound=15.0)
    assert np.isfinite(d[0]) and ti[0] < k.nfaces
    do, tio = o.query(np.array([[0.0, 0, 20]]), distance_upper_bound=15.0)
    assert np.isfinite(do[0])


def test_query_upper_bound_over_returns_inf():
    o, k = _both(_box((10, 10, 10)))
    d, ti = k.query(np.array([[100.0, 0, 0]]), distance_upper_bound=10.0)
    assert not np.isfinite(d[0]) and ti[0] == k.nfaces


def test_equal_distance_subtree_tie_lowest_index():
    # Amendment A12: a point equidistant to mirror-image triangle clusters
    # placed in DIFFERENT BVH subtrees. The LOWEST face index must win the tie
    # across the whole tree -- not merely deterministically, and not merely
    # within a leaf. This pins BOTH the leaf comparison (`t < best_i`, guards a
    # highest-index tie regression) AND the strict `>` pruning (guards a `>=`
    # regression that would prune the lower-index subtree once best_d is set).
    #
    # Faces 0-7 are 8 coincident copies of a RIGHT triangle (x in [1,3]);
    # faces 8-15 are 8 coincident copies of its LEFT mirror (x in [-3,-1]).
    # >8 faces forces a top-level split into left/right subtrees; the left
    # subtree (higher indices 8-15) is traversed first, so the winner index 0
    # is only reached in the second-visited subtree -- exactly where a `>=`
    # prune would drop it.
    right = np.array([[1, -1, 0], [1, 1, 0], [3, 0, 0.0]])
    left = np.array([[-1, -1, 0], [-1, 1, 0], [-3, 0, 0.0]])
    verts = []
    faces = []
    for _ in range(8):                       # faces 0-7: right
        base = len(verts)
        verts.extend(right.tolist())
        faces.append([base, base + 1, base + 2])
    for _ in range(8):                       # faces 8-15: left mirror
        base = len(verts)
        verts.extend(left.tolist())
        faces.append([base, base + 1, base + 2])
    mesh = trimesh.Trimesh(vertices=np.array(verts, float),
                           faces=np.array(faces), process=False)
    k = GK.KernelSurface(mesh)
    assert k.build_log["bvh_nodes"] >= 3, "fixture must split into subtrees"
    P = np.array([[0.0, 0, 5.0]])
    d, ti = k.query(P)
    d2, ti2 = k.query(P)
    assert ti[0] == ti2[0]  # deterministic
    # genuine tie: a right copy (face 0) and a left copy (face 8) sit at
    # bit-identical distance (mirror geometry => dx^2 identical).
    A = k.V0; B = k.V0 + k.E1; C = k.V0 + k.E2
    Pb = np.broadcast_to(P[0], A.shape)
    dists = np.linalg.norm(Pb - GE.closest_on_tri(Pb, A, B, C), axis=1)
    assert dists[0] == dists[8], "fixture must be a genuine equal-distance tie"
    assert dists.min() == dists[0]
    # A12 contract: the LOWEST face index wins the tie across the tree.
    assert ti[0] == 0, f"A12: tie must resolve to the lowest face index, got {ti[0]}"


# ==========================================================================
# 2. PRIMITIVE BIT-PARITY PINS
# ==========================================================================
def test_primitive_bit_parity_single_triangle():
    tri = trimesh.Trimesh(vertices=np.array([[0, 0, 0], [4, 0, 0], [0, 4, 0.0]]),
                          faces=np.array([[0, 1, 2]]), process=False)
    o, k = _both(tri)
    pts = np.array([[0, 0, 2.0], [7, 0, 1.5], [1, 1, 3.0], [-2, 1, 1.5], [2, 2, 0.7]])
    do, cpo, tio, _ = o.project(pts)
    dk, cpk, tik, _ = k.project(pts)
    # same face => same formula => BIT-identical distance and closest point
    for i in range(len(pts)):
        if tio[i] == tik[i]:
            assert do[i] == dk[i], f"dist bit mismatch at {i}: {do[i]!r} {dk[i]!r}"
            assert np.array_equal(cpo[i], cpk[i]), f"cp bit mismatch at {i}"


# ==========================================================================
# 3. SEEDED FUZZ (per fixture mesh; bounded, RSS < 2GB)
# ==========================================================================
def _fuzz_meshes():
    return [(n, fn()) for n, fn in FIXTURES.items()]


def test_fuzz_query_project_inside():
    rng = np.random.default_rng(0xB51)
    for name, mesh in _fuzz_meshes():
        o, k = _both(mesh)
        lo, hi = mesh.bounds
        span = hi - lo
        pts = lo - 0.25 * span + rng.uniform(0, 1.5, size=(3000, 3)) * span
        do, tio = o.query(pts); dk, tik = k.query(pts)
        # distances 1e-9 relative
        assert np.all(np.abs(do - dk) <= 1e-9 * np.maximum(1.0, np.abs(do))), name
        # indices agree EXCEPT at measured ties (equal distance)
        mism = tio != tik
        if mism.any():
            assert np.all(np.abs(do[mism] - dk[mism]) <= 1e-9 * np.maximum(1.0, np.abs(do[mism]))), name
        # inside EXACT per point
        assert np.array_equal(o.inside(pts), k.inside(pts)), name
        # project normals up to sign, except at measured ties
        _, _, tio2, no = o.project(pts)
        _, _, tik2, nk = k.project(pts)
        tie = (tio2 != tik2) & (np.abs(do - dk) <= 1e-12 * np.maximum(1.0, np.abs(do)))
        ok = ~tie
        dotn = np.abs(np.einsum('ij,ij->i', no, nk))
        assert np.all(dotn[ok] >= 1 - 1e-9), f"{name}: normal parity off-tie"


def test_fuzz_segment_clearance_tier_aware():
    # Unified contract (Amendment A1 + confirmed oracle behavior): the kernel
    # computes the TRUE exact segment-to-surface minimum; the oracle's crop is
    # complete only at/below the decision threshold (where clearance <= 0), and
    # OVER-estimates positive clearances (crop misses the true nearest when it
    # is comfortably clear — Tier-1 NaN or a farther cropped triangle). So:
    #   * clr_kernel <= clr_oracle       ALWAYS
    #   * sign parity (both <=0 or both >0), away from a 1e-6 knife edge
    #   * exact 1e-9 value parity ONLY when clr_oracle < 0 (crop complete)
    rng = np.random.default_rng(0x5E9)
    n_neg = 0
    n_pos = 0
    for name, mesh in _fuzz_meshes():
        o, k = _both(mesh)
        lo, hi = mesh.bounds
        ctr = 0.5 * (lo + hi)
        for _ in range(300):
            if rng.random() < 0.5:
                base = lo + rng.uniform(-0.5, 1.5, size=3) * (hi - lo)
            else:
                base = ctr + rng.uniform(-1, 1, size=3) * (hi - lo) * 3.0
            d = rng.uniform(-1, 1, size=3)
            L = rng.uniform(0.1, 40.0)
            d = d / (np.linalg.norm(d) or 1.0) * L
            p0 = base[None, :]; p1 = (base + d)[None, :]
            r = float(rng.uniform(0.1, 1.0))
            co, ao = o.segment_clearance(p0, p1, r, r, 0.5)
            ck, ak = k.segment_clearance(p0, p1, r, r, 0.5)
            if not np.isfinite(co[0]):
                continue
            # kernel never OVER-states clearance vs the oracle
            assert ck[0] <= co[0] + 1e-9, f"{name}: clr_k {ck[0]} > clr_o {co[0]}"
            # sign parity away from knife edge
            if abs(co[0]) > 1e-6 and abs(ck[0]) > 1e-6:
                assert (co[0] > 0) == (ck[0] > 0), f"{name}: sign flip {co[0]} vs {ck[0]}"
            if co[0] < -1e-9:
                n_neg += 1
                # negative => crop complete => exact value parity
                assert abs(co[0] - ck[0]) <= 1e-9 * max(1.0, abs(co[0])) + 1e-9, \
                    f"{name}: negative-clearance value {co[0]} vs {ck[0]}"
                assert not np.isnan(ak[0]).any(), f"{name}: kernel argmin NaN on contact"
            else:
                n_pos += 1
    assert n_neg > 0 and n_pos > 0, f"clearances exercised: neg={n_neg} pos={n_pos}"


def _brute_canonical_groups(mesh, o_surf, O, D, t_max=1.0):
    """Test-side canonicalization (A13): brute-force all-triangle Moller,
    dedup by arc-location, map each group to its LOWEST face index."""
    V0 = o_surf.V0; E1 = o_surf.E1; E2 = o_surf.E2
    hit, t = GE._moller(O, D, V0, E1, E2, t_max=t_max)
    idx = np.nonzero(hit)[0]
    if len(idx) == 0:
        return np.empty(0), np.empty(0, int)
    tt = t[idx]; order = np.argsort(tt, kind="stable")
    tt = tt[order]; ff = idx[order]
    seg = np.linalg.norm(D) * t_max
    dt = 3e-3 / max(seg, 1e-9)
    groups_t = []; groups_face = []
    kept = tt[0]; gmin = ff[0]
    for j in range(1, len(tt)):
        if tt[j] - kept > dt:
            groups_t.append(kept); groups_face.append(gmin)
            kept = tt[j]; gmin = ff[j]
        else:
            gmin = min(gmin, ff[j])
    groups_t.append(kept); groups_face.append(gmin)
    return np.array(groups_t), np.array(groups_face, int)


def test_fuzz_cast_rays():
    rng = np.random.default_rng(0xCA57)
    for name, mesh in _fuzz_meshes():
        o, k = _both(mesh)
        lo, hi = mesh.bounds
        for _ in range(150):
            O = lo + rng.uniform(-0.3, 1.3, size=3) * (hi - lo)
            P = lo + rng.uniform(-0.3, 1.3, size=3) * (hi - lo)
            D = P - O
            if np.linalg.norm(D) < 1e-6:
                continue
            ro = o.cast_rays(O[None, :], D[None, :], 1.0)[0]
            rk = k.cast_rays(O[None, :], D[None, :], 1.0)[0]
            # crossing counts exact
            assert len(ro[0]) == len(rk[0]), f"{name}: count {len(ro[0])} vs {len(rk[0])}"
            if len(rk[0]) == 0:
                continue
            # t-values 1e-9 rel (sorted; alignment by index after both sorted)
            to = np.sort(ro[0]); tk = np.sort(rk[0])
            assert np.all(np.abs(to - tk) <= 1e-9 * np.maximum(1.0, np.abs(to))), name
            # faces exact after brute-force canonicalization to lowest-index
            _, cf = _brute_canonical_groups(mesh, o, O, D, 1.0)
            # kernel already returns lowest-index rep; align by t-order
            korder = np.argsort(rk[0], kind="stable")
            kf = rk[1][korder]
            assert np.array_equal(np.sort(kf), np.sort(cf)), \
                f"{name}: face set {sorted(kf)} vs canonical {sorted(cf)}"


def test_fuzz_sphere_breach():
    rng = np.random.default_rng(0xBA11)
    for name, mesh in _fuzz_meshes():
        o, k = _both(mesh)
        # seed centers ON the surface +/- jitter
        lo, hi = mesh.bounds
        probes = lo + rng.uniform(0, 1, size=(200, 3)) * (hi - lo)
        _, cp, _, _ = o.project(probes)
        centers = cp + rng.uniform(-0.5, 0.5, size=cp.shape)
        radii = rng.uniform(0.3, 1.5, size=len(centers))
        bo, tho, po = o.sphere_breach(centers, radii)
        bk, thk, pk = k.sphere_breach(centers, radii)
        # The breach cone is shot along the projected face NORMAL. project() is
        # bit-exact in distance, so where oracle and kernel pick the SAME argmin
        # face the whole computation is identical; where they pick different
        # (equidistant) tied faces the inward normal differs and thickness is
        # legitimately incomparable (measured-tie telemetry, spec sec 2.3/A15).
        _, _, tio, _ = o.project(centers)
        _, _, tik, _ = k.project(centers)
        same = tio == tik
        # same face => breach + thickness identical (kernel's conservative-OR
        # only ADDS breaches at coincident opposing-normal ties, never removes)
        assert np.all(bk[same] >= bo[same]), f"{name}: under-flagged breach (same face)"
        fin = same & (bk == bo) & np.isfinite(tho) & np.isfinite(thk)
        if fin.any():
            assert np.all(np.abs(tho[fin] - thk[fin]) <= 1e-9 * np.maximum(1.0, np.abs(tho[fin]))), \
                f"{name}: thickness parity (same face)"


def test_fuzz_penetration_intervals():
    rng = np.random.default_rng(0x9E7)
    for name, mesh in _fuzz_meshes():
        o, k = _both(mesh)
        lo, hi = mesh.bounds
        ctr = 0.5 * (lo + hi)
        for _ in range(80):
            p0 = ctr + rng.uniform(-1, 1, size=3) * (hi - lo) * 0.6
            p1 = p0 + rng.uniform(-1, 1, size=3) * (hi - lo) * 0.6
            if np.linalg.norm(p1 - p0) < 1e-6:
                continue
            r0 = float(rng.uniform(0.1, 0.6)); r1 = float(rng.uniform(0.1, 0.6))
            for rays in ("axis", "all"):
                io = o.penetration_intervals(p0, p1, r0, r1, rays=rays)
                ik = k.penetration_intervals(p0, p1, r0, r1, rays=rays)
                assert len(io) == len(ik), f"{name}/{rays}: {len(io)} vs {len(ik)}"
                # sort both by (ray,s_a) for stable alignment
                so = sorted(io, key=lambda d: (d["ray"], round(d["s_a"], 6)))
                sk = sorted(ik, key=lambda d: (d["ray"], round(d["s_a"], 6)))
                for a, b in zip(so, sk):
                    assert a["ray"] == b["ray"]
                    assert a["entered"] == b["entered"] and a["exited"] == b["exited"]
                    assert abs(a["s_a"] - b["s_a"]) <= 1e-9 * max(1.0, abs(a["s_a"])) + 1e-9
                    assert abs(a["s_b"] - b["s_b"]) <= 1e-9 * max(1.0, abs(a["s_b"])) + 1e-9
                    assert abs(a["max_depth_mm"] - b["max_depth_mm"]) <= 1e-9 * max(1.0, abs(a["max_depth_mm"])) + 1e-9


# ==========================================================================
# 5. DETERMINISM / THREAD-INVARIANCE SWEEP (Amendment A4/A19)
# ==========================================================================
def _canon_bytes(*arrays):
    parts = []
    for a in arrays:
        a = np.ascontiguousarray(a)
        parts.append(str(a.dtype).encode())
        parts.append(str(a.shape).encode())
        parts.append(a.tobytes())
    return b"|".join(parts)


def test_thread_invariance_bit_identical():
    mesh = mesh_io.make_overhang_table()
    k = GK.KernelSurface(mesh)  # build is serial + immutable
    rng = np.random.default_rng(7)
    lo, hi = mesh.bounds
    pts = lo - 0.2 * (hi - lo) + rng.uniform(0, 1.4, size=(4096, 3)) * (hi - lo)
    prev_env = os.environ.get("GEOM_KERNEL_THREADS")
    seen_counts = []
    results = []
    try:
        cases = ["1", "2", "0", "0", "1"]  # sweep + repeats
        for env in cases:
            os.environ["GEOM_KERNEL_THREADS"] = env
            seen_counts.append(k.current_threads())
            d, ti = k.query(pts)
            ins = k.inside(pts)
            dd, cp, idx, nrm = k.project(pts)
            results.append(_canon_bytes(d, ti, ins, dd, cp, idx, nrm))
    finally:
        if prev_env is None:
            os.environ.pop("GEOM_KERNEL_THREADS", None)
        else:
            os.environ["GEOM_KERNEL_THREADS"] = prev_env
    # the sweep must actually vary the effective thread count (A4)
    assert len(set(seen_counts[:3])) >= 2, f"thread count did not vary: {seen_counts}"
    # every run byte-identical
    assert len(set(results)) == 1, "outputs differ across thread counts / repeats"


# ==========================================================================
# 8. STRUCTURAL PROPERTY: BVH nearest vs brute force (Python side)
# ==========================================================================
def test_bvh_nearest_vs_brute_python():
    sph = trimesh.creation.icosphere(subdivisions=3, radius=5.0)
    sph = trimesh.Trimesh(vertices=sph.vertices.copy(), faces=sph.faces.copy(), process=False)
    k = GK.KernelSurface(sph)
    # brute force via oracle's closest_on_tri over all faces
    A = k.V0; B = k.V0 + k.E1; C = k.V0 + k.E2
    rng = np.random.RandomState(0)
    pts = rng.uniform(-8, 8, size=(200, 3))
    dk, tik = k.query(pts)
    for i, p in enumerate(pts):
        P = np.broadcast_to(p, A.shape)
        cpt = GE.closest_on_tri(P, A, B, C)
        dref = float(np.linalg.norm(P - cpt, axis=1).min())
        assert abs(dk[i] - dref) <= 1e-9 * max(1.0, dref)


# ==========================================================================
# 9. THRESHOLD-BAND TELEMETRY (spec R3 / sec 4.2) — the band_counter must
#    actually count near-threshold continuous decisions, else the full-scale
#    acceptance log's R3 tripwire reads 0 exactly when it matters.
# ==========================================================================
def test_threshold_band_counter_counts_near_threshold_decisions():
    # clearance-vs-0 band: a segment whose exact clearance is 0 to f64
    # (best_d 1.0 == r_max+standoff 1.0) must register; a comfortably-clear one
    # must not. Fresh surface => counter starts at 0.
    k = GK.KernelSurface(_box((20, 20, 5)))
    assert k.telemetry()["threshold_band"] == 0
    k.segment_clearance(np.array([[-5, 0, 10.0]]), np.array([[5, 0, 10.0]]), 0.5, 0.5, 0.5)
    assert k.telemetry()["threshold_band"] == 0, "far decision must NOT band"
    k.segment_clearance(np.array([[-5, 0, 3.5]]), np.array([[5, 0, 3.5]]), 0.5, 0.5, 0.5)
    assert k.telemetry()["threshold_band"] >= 1, "clearance==0 must land in the band"

    # breach reach-precondition band: r - nd = 0.05 exactly at eps_mm=0.05.
    k2 = GK.KernelSurface(_box((20, 20, 5)))
    k2.sphere_breach(np.array([[0, 0, 0.0]]), np.array([1.0]))  # r-nd = -1.5, far
    assert k2.telemetry()["threshold_band"] == 0
    k2.sphere_breach(np.array([[0, 0, 1.5]]), np.array([1.05]))  # nd=1.0, r-nd=0.05
    assert k2.telemetry()["threshold_band"] >= 1, "r-nd==eps must land in the band"


def test_threshold_band_counter_thread_invariant():
    # the COUNT is a deterministic function of the inputs (each decision
    # inspects only its own quantity), so it must not vary with thread count.
    import os
    mesh = _box((20, 20, 5))
    centers = np.array([[0, 0, 1.5], [0, 0, 0.0], [0, 0, 1.05], [3, 3, 1.5]])
    radii = np.array([1.05, 1.0, 1.05, 1.05])
    prev = os.environ.get("GEOM_KERNEL_THREADS")
    counts = []
    try:
        for env in ("1", "0", "2"):
            os.environ["GEOM_KERNEL_THREADS"] = env
            kk = GK.KernelSurface(mesh)
            kk.sphere_breach(centers, radii)
            counts.append(kk.telemetry()["threshold_band"])
    finally:
        if prev is None:
            os.environ.pop("GEOM_KERNEL_THREADS", None)
        else:
            os.environ["GEOM_KERNEL_THREADS"] = prev
    assert len(set(counts)) == 1, f"band count varied with threads: {counts}"


# ==========================================================================
# 10. ADVERSARIAL THRESHOLD CORPUS (Amendment A21, sec 6 item 7) — a second
#     generator seeking the decision boundaries the seeded fuzz's redraw band
#     deliberately avoids. On pin-covered paths oracle and kernel compute
#     identical bits, so EXACT agreement is asserted even at the knife edge.
# ==========================================================================
def test_adversarial_moller_t_at_segment_endpoints():
    # a ray whose crossings sit EXACTLY at t=0 and t=t_max (origin ON one face,
    # terminus ON the opposite face) — both Moller parameter endpoints at once.
    o, k = _both(_box((10, 10, 10)))
    O = np.array([[5.0, 0, 0]])          # on the +x face (t=0 crossing)
    D = np.array([[-10.0, 0, 0]])        # ends on the -x face (t=1=t_max)
    ro = o.cast_rays(O, D, 1.0)[0]
    rk = k.cast_rays(O, D, 1.0)[0]
    assert len(ro[0]) == len(rk[0]), f"endpoint crossings {len(ro[0])} vs {len(rk[0])}"
    to = np.sort(ro[0]); tk = np.sort(rk[0])
    assert np.all(np.abs(to - tk) <= 1e-9 * np.maximum(1.0, np.abs(to)))
    _, cf = _brute_canonical_groups(_box((10, 10, 10)), o, O[0], D[0], 1.0)
    korder = np.argsort(rk[0], kind="stable")
    assert np.array_equal(np.sort(rk[1][korder]), np.sort(cf))


def test_adversarial_parity_coincident_shells_at_dedup_boundary():
    # three coincident walls at EXACTLY zdedup (5e-3) spacing: the cluster-count
    # dedup boundary. Probed above/below/between; inside verdicts EXACT vs oracle.
    walls = [_box((4, 4, 1e-6), center=(0, 0, z)) for z in (0.0, 5e-3, 1e-2)]
    mesh = _soup(*walls)
    o, k = _both(mesh)
    pts = np.array([[0, 0, -0.5], [0, 0, 0.5], [0, 0, 2.5e-3], [0, 0, 7.5e-3]])
    assert np.array_equal(o.inside(pts), k.inside(pts))


def test_adversarial_clearance_and_breach_at_thresholds():
    # clearance exactly at 0 (Tier-2, value 1e-9 parity + sign parity), and a
    # breach whose thickness is near r-eps: oracle and kernel agree.
    o, k = _both(_box((20, 20, 5)))
    co, _ = o.segment_clearance(np.array([[-5, 0, 3.5]]), np.array([[5, 0, 3.5]]), 0.5, 0.5, 0.5)
    ck, _ = k.segment_clearance(np.array([[-5, 0, 3.5]]), np.array([[5, 0, 3.5]]), 0.5, 0.5, 0.5)
    assert abs(co[0] - ck[0]) <= 1e-9 * max(1.0, abs(co[0]))
    assert (co[0] >= 0) == (ck[0] >= 0)
    # coincident shells: two identical boxes; every inside verdict EXACT.
    o2, k2 = _both(_soup(_box((10, 10, 10)), _box((10, 10, 10))))
    rng = np.random.default_rng(0xC0)
    pts = rng.uniform(-8, 8, size=(500, 3))
    assert np.array_equal(o2.inside(pts), k2.inside(pts))


# ==========================================================================
# 11. BATCHED-ENTRY BIT PINS + CANCELLATION-HEAVY FUZZ (Amendment A21,
#     sec 5.2/6 item 2) — scalar closed-form pins guard the wrong path; the
#     production per-element BATCH loop is what LTO exposes to SLP/loop
#     auto-vectorization, so the bit pins run through the batched entry points.
# ==========================================================================
def test_batched_primitive_bit_parity_single_triangle():
    tri = trimesh.Trimesh(vertices=np.array([[0, 0, 0], [4, 0, 0], [0, 4, 0.0]]),
                          faces=np.array([[0, 1, 2]]), process=False)
    o, k = _both(tri)
    rng = np.random.default_rng(0xB17)
    # 2048-point batch (the ≤2048 discipline) all resolving to the one face:
    pts = rng.uniform(-3, 6, size=(2048, 3))
    do, cpo, tio, _ = o.project(pts)
    dk, cpk, tik, _ = k.project(pts)
    same = tio == tik
    assert same.sum() > 100, "batch must exercise the shared-face path"
    # same face => same formula => BIT-identical distance and closest point,
    # through the vectorized batch entry (not just a 5-point scalar pin).
    assert np.array_equal(do[same], dk[same]), "batched distance bit mismatch"
    assert np.array_equal(cpo[same], cpk[same]), "batched closest-point bit mismatch"


def test_cancellation_heavy_primitive_bit_fuzz():
    # wide exponent range + near-cancellation operand constructions: a triangle
    # with mixed-magnitude coordinates, points drawn across many exponents.
    tri = trimesh.Trimesh(
        vertices=np.array([[1e-3, 2e2, -5e1], [3e2, -1e-2, 4e1], [-2e1, 5e1, 7e-3]]),
        faces=np.array([[0, 1, 2]]), process=False)
    o, k = _both(tri)
    rng = np.random.default_rng(0xCA9)
    exps = rng.integers(-4, 4, size=(4000,))
    signs = rng.choice([-1.0, 1.0], size=(4000, 3))
    mant = rng.uniform(1.0, 9.9, size=(4000, 3))
    pts = signs * mant * (10.0 ** exps)[:, None]
    do, cpo, tio, _ = o.project(pts)
    dk, cpk, tik, _ = k.project(pts)
    same = tio == tik
    assert same.any()
    assert np.array_equal(do[same], dk[same]), "cancellation-heavy distance bit drift"
    assert np.array_equal(cpo[same], cpk[same]), "cancellation-heavy cp bit drift"


# ==========================================================================
# 12. Amendment A13 — CONTESTED representative + anchor-consumption fixture.
#     Two coincident walls (< dedup tol) make the dedup-group representative
#     genuinely contested; the lowest-index rep and the exterior-mask value it
#     indexes (cco.py:451-460 anchor input) must agree oracle vs kernel.
# ==========================================================================
def test_coincident_wall_representative_and_anchor():
    v = np.array([
        [0, -1, -1], [0, 1, -1], [0, 1, 1], [0, -1, 1],                    # wall A @ x=0
        [1e-4, -1, -1], [1e-4, 1, -1], [1e-4, 1, 1], [1e-4, -1, 1],        # wall B @ x=1e-4
    ], float)
    faces = np.array([[0, 1, 2], [0, 2, 3], [4, 6, 5], [4, 7, 6]])          # B reversed winding
    mesh = trimesh.Trimesh(vertices=v, faces=faces, process=False)
    o, k = _both(mesh)
    O = np.array([[-2.0, 0, 0]]); D = np.array([[4.0, 0, 0]])
    ro = o.cast_rays(O, D, 1.0)[0]; rk = k.cast_rays(O, D, 1.0)[0]
    assert len(rk[0]) == len(ro[0]) == 1, "coincident walls must dedup to ONE crossing"
    # kernel returns the lowest-index representative; brute canonical agrees
    _, cf = _brute_canonical_groups(mesh, o, O[0], D[0], 1.0)
    assert list(rk[1]) == list(cf), f"representative {list(rk[1])} vs canonical {list(cf)}"
    # anchor-selection input: exterior mask indexed by the representative equal
    mo = o.exterior_face_mask(); mk = k.exterior_face_mask()
    assert np.array_equal(mo, mk)
    assert bool(mk[int(rk[1][0])]) == bool(mo[int(cf[0])])


# ==========================================================================
# 13. Amendment A14 — Tier-1 observability + revalidate-verdict replay.
#     The Tier-1 deviation is verdict-preserving; the load-bearing property is
#     (a) the one-directional ordering clr_kernel <= clr_oracle on every Tier-1
#     draw, and (b) EQUAL revalidate verdicts oracle vs kernel (sec 6 item 4).
# ==========================================================================
def test_a14_tier1_ordering_over_many_draws():
    # sweep segments across the open-space regime; every oracle Tier-1 draw
    # (NaN argmin, finite positive cert) must satisfy clr_kernel <= clr_oracle
    # and both strictly positive (verdict-preserving).
    o, k = _both(_box((2, 2, 2)))
    rng = np.random.default_rng(0x14A)
    n_tier1 = 0
    for _ in range(400):
        base = rng.uniform(-1, 1, size=3)
        base = base / (np.linalg.norm(base) or 1.0) * rng.uniform(3.0, 12.0)
        d = rng.uniform(-1, 1, size=3)
        d = d / (np.linalg.norm(d) or 1.0) * rng.uniform(0.1, 3.0)
        p0 = base[None, :]; p1 = (base + d)[None, :]
        co, ao = o.segment_clearance(p0, p1, 0.5, 0.5, 0.5)
        ck, ak = k.segment_clearance(p0, p1, 0.5, 0.5, 0.5)
        if not np.isfinite(co[0]):
            continue
        tier1 = bool(np.isnan(ao[0]).any()) and np.isfinite(co[0]) and co[0] > 0
        if not tier1:
            continue
        n_tier1 += 1
        assert co[0] > 0 and ck[0] > 0, "Tier-1 must be strictly positive both sides"
        assert ck[0] <= co[0] + 1e-9, f"Tier-1 ordering violated: {ck[0]} > {co[0]}"
    assert n_tier1 >= 50, f"too few Tier-1 draws exercised: {n_tier1}"


def test_a14_revalidate_verdict_replay_oracle_vs_kernel():
    # sec 6 item 4: run the SAME dirty members through tree.Forest.revalidate
    # with the oracle geom and the kernel geom; the violation member sets must
    # be EXACTLY equal (the A14 outcome-equivalence proof as a test).
    from support_placement import tree as TR, config
    mesh = _box((20, 20, 5))                     # top face at z=2.5
    o, k = _both(mesh)
    cfg = config.SupportConfig()
    # a clearly-clear strut, a clearly-violating (embedded) strut, and a
    # near-but-clear one — mm-scale margins so both geoms agree exactly.
    specs = [
        (( -5, 0, 10.0), (5, 0, 10.0)),          # far above: clear
        (( -5, 0, 2.7), (5, 0, 2.7)),            # skims the top face: violation
        (( -5, 0, 6.0), (5, 0, 6.0)),            # a few mm clear
    ]

    def _run(geom):
        f = TR.Forest()
        for p0, p1 in specs:
            f.add_member(TR.Member(p0=np.array(p0, float), p1=np.array(p1, float),
                                   r0=0.5, r1=0.5, kind="strut", owner=0))
        viol = f.revalidate(geom, cfg)
        return sorted(v["member_idx"] for v in viol)

    assert _run(o) == _run(k), "revalidate verdicts diverge oracle vs kernel"


# ==========================================================================
# 14. Amendment A18 — the stage-1 batch refactor: no per-element FFI hot loop
#     survives revalidate. The call-size histogram proof (fixture-scale).
# ==========================================================================
class _CountingGeom:
    """Transparent proxy counting per-member vs batched interval calls."""
    def __init__(self, inner):
        self._inner = inner
        self.n_per_member = 0
        self.batch_sizes = []

    def __getattr__(self, name):
        return getattr(self._inner, name)

    def penetration_intervals(self, *a, **kw):
        self.n_per_member += 1
        return self._inner.penetration_intervals(*a, **kw)

    def penetration_intervals_batch(self, p0s, *a, **kw):
        self.batch_sizes.append(len(p0s))
        return self._inner.penetration_intervals_batch(p0s, *a, **kw)


def test_a18_revalidate_batches_interval_calls():
    from support_placement import tree as TR, config
    mesh = _box((10, 10, 10))                    # +x face at x=5
    cfg = config.SupportConfig()                 # standoff 0.6
    # three struts whose clearance lands in [0, 0.1) => the interval belt fires
    # for all three; the refactor must service them with ONE batched native
    # call, never three per-member ones (R1 / Amendment A18).
    xs = [6.15, 6.16, 6.17]                       # dist-to-face 1.15.. => clr ~0.05..
    f = TR.Forest()
    for x in xs:
        f.add_member(TR.Member(p0=np.array([x, -3, 0.0]), p1=np.array([x, 3, 0.0]),
                               r0=0.5, r1=0.5, kind="strut", owner=0))
    cg = _CountingGeom(GK.KernelSurface(mesh))
    viol = f.revalidate(cg, cfg)
    assert cg.n_per_member == 0, "per-member penetration_intervals must NOT be called"
    assert cg.batch_sizes == [3], f"expected ONE batch of 3, got {cg.batch_sizes}"
    assert viol == [], "clear struts must not be flagged"


# ==========================================================================
# 15. Amendment A22 (spec sec 2.4) — wrapper-side input validation MUST reject
#     non-finite (NaN/inf) coordinates AND non-coordinate numeric args
#     (radii/insets/standoff) with a ValueError naming the offending argument,
#     BEFORE crossing the FFI. The `< 0` nonneg checks alone let NaN through
#     (NaN < 0 is False), which on the interval paths silently returns an EMPTY
#     interval list and certifies garbage geometry (the confirmed B-2 hole).
# ==========================================================================
def _kbox():
    return GK.KernelSurface(_box((10, 10, 10)))


def test_a22_penetration_intervals_rejects_nan_inf():
    k = _kbox()
    good0 = np.array([-3.0, 0, 0]); good1 = np.array([3.0, 0, 0])
    # NaN / inf endpoint
    for bad in (np.nan, np.inf):
        with pytest.raises(ValueError):
            k.penetration_intervals(np.array([bad, 0, 0]), good1, 0.3, 0.3)
        with pytest.raises(ValueError):
            k.penetration_intervals(good0, np.array([0, bad, 0]), 0.3, 0.3)
        # NaN / inf radius — the exact hole: NaN passes `r < 0`
        with pytest.raises(ValueError):
            k.penetration_intervals(good0, good1, bad, 0.3)
        with pytest.raises(ValueError):
            k.penetration_intervals(good0, good1, 0.3, bad)
    with pytest.raises(ValueError):
        k.penetration_intervals(good0, good1, -0.1, 0.3)   # negative still caught


def test_a22_penetration_intervals_batch_rejects_nan_inf():
    k = _kbox()
    p0 = np.array([[-3.0, 0, 0], [-3, 1, 0]])
    p1 = np.array([[3.0, 0, 0], [3, 1, 0]])
    with pytest.raises(ValueError):
        k.penetration_intervals_batch(np.array([[np.nan, 0, 0], [-3, 1, 0]]), p1, 0.3, 0.3)
    with pytest.raises(ValueError):
        k.penetration_intervals_batch(p0, p1, np.array([0.3, np.nan]), 0.3)
    with pytest.raises(ValueError):
        k.penetration_intervals_batch(p0, p1, 0.3, np.array([np.inf, 0.3]))
    with pytest.raises(ValueError):
        k.penetration_intervals_batch(p0, p1, np.array([-0.1, 0.3]), 0.3)


def test_a22_sphere_breach_rejects_nan_inf_radius():
    k = _kbox()
    c = np.array([[0, 0, 0.0]])
    with pytest.raises(ValueError):
        k.sphere_breach(c, np.array([np.nan]))   # the hole: NaN passes `radii < 0`
    with pytest.raises(ValueError):
        k.sphere_breach(c, np.array([np.inf]))
    with pytest.raises(ValueError):
        k.sphere_breach(np.array([[np.nan, 0, 0]]), np.array([1.0]))
    with pytest.raises(ValueError):
        k.sphere_breach(c, np.array([-0.5]))     # negative still caught


def test_a22_segment_clearance_rejects_nan_inf():
    k = _kbox()
    p0 = np.array([[-5.0, 0, 8.0]]); p1 = np.array([[5.0, 0, 8.0]])
    with pytest.raises(ValueError):
        k.segment_clearance(p0, p1, np.array([np.nan]), 0.5, 0.5)  # NaN radius
    with pytest.raises(ValueError):
        k.segment_clearance(p0, p1, 0.5, np.array([np.inf]), 0.5)
    with pytest.raises(ValueError):
        k.segment_clearance(p0, p1, 0.5, 0.5, 0.5, inset0s=np.array([np.nan]))  # NaN inset
    with pytest.raises(ValueError):
        k.segment_clearance(p0, p1, 0.5, 0.5, np.nan)             # NaN standoff
    with pytest.raises(ValueError):
        k.segment_clearance(p0, p1, -0.5, 0.5, 0.5)               # negative still caught


def test_a22_penetration_intervals_nan_does_not_silently_empty():
    # The precise failure scenario: a NaN endpoint must RAISE, never return an
    # empty (=no-penetration) interval list that would certify garbage geometry.
    k = _kbox()
    try:
        out = k.penetration_intervals(np.array([np.nan, 0, 0]), np.array([3.0, 0, 0]), 0.3, 0.3)
    except ValueError:
        return  # correct: rejected
    pytest.fail(f"NaN endpoint silently returned {out!r} instead of raising")
