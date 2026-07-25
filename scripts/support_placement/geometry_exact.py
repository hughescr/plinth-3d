"""Surface-EXACT geometric oracle over a triangle mesh (v5 geometry core).

SPEC: docs/specs/SPEC-V5-GEOM.md (the authority). This module is the swap
boundary the spec mandates: it evicts the sampled surface point cloud
(routing.build_surface_tree) from load-bearing geometric decisions and answers
every distance / intersection / inside query against the ACTUAL triangles via a
centroid cKDTree + Moller-Trumbore + multi-axis ray-parity.

Nothing consumes this module yet (per the task: deliver the core + tests +
micro-benchmark, do NOT swap consumers). Consumer swap is a follow-up.

Math is ported from the exact penetration census that measured 48 real
support-vs-model crossings the sampled check reported as zero:
    scripts/tools/census_core.py   (100% heavy-9-ray agreement)
Ported verbatim where possible: closest_on_tri (Ericson), ray_crossings
(Moller-Trumbore), VerticalParity / ParityVote (3-axis majority inside test).

Design principles (binding, from the spec):
  - Surface-exact: never a sampled proxy of the triangles.
  - Inside/outside via 3-axis ray-parity MAJORITY vote, never nearest-face
    signed-distance sign (unreliable ~3x over-report at thin features). The
    mesh is triangle soup (unmerged verts) but winding-consistent -> parity
    works, topology-dependent methods do not.
  - Sanctioned embedding is semantics: the caller supplies inset zones; the
    checker measures only OUTSIDE those zones (ALLOWS designed end-embedding,
    FORBIDS mid-path penetration + thin-feature ball breach).
  - Swappable kernel: a native (C/Rust/embree BVH) kernel may replace the
    internals behind this exact API with zero call-site changes.
  - Memory discipline (a prior agent OOMed at 173GB): hard 2GB RSS self-check
    in every long loop; product-bounded adaptive batching (queries x candidate
    triangles, ROW_BUDGET rows), ragged crops concatenated never padded;
    NEVER build query-count x all-576k-faces arrays.
"""
import resource
import sys

import numpy as np
from scipy.spatial import cKDTree

EPS = 1e-6
# Product budget: max rows in an O(queries x candidates) intermediate. The
# exact kernels (closest_on_tri and friends) materialize ~15 float64 (M,3)
# arrays; ~2M rows -> ~0.7GB working set, well under the 2GB per-op budget.
ROW_BUDGET = 2_000_000
# Per-operation RSS-GROWTH budget. NB: resource.getrusage's ru_maxrss is the
# process-wide PEAK since start and never decreases, so an ABSOLUTE cap would
# false-trip in a shared pytest session where an earlier full-engine test
# already peaked high. The load-bearing quantity is how much MY loop grows the
# footprint: each batch method captures a base at entry and asserts the peak
# stays within RSS_LIMIT_GB of it. A runaway (the prior agent's 173GB OOM)
# blows past this instantly; a bounded ROW_BUDGET loop never approaches it.
RSS_LIMIT_GB = 2.0


def rss_gb():
    """Peak resident set size in GB (getrusage; ru_maxrss is bytes on darwin,
    KiB on linux). Monotonic non-decreasing (high-water mark)."""
    r = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return (r / 1e9) if sys.platform == "darwin" else (r / 1.048576e6)


def _rss_ok(base=0.0):
    """Assert this operation has not grown the footprint by more than the
    per-op budget past `base` (captured at the method's entry)."""
    grew = rss_gb() - base
    assert grew < RSS_LIMIT_GB, (
        f"RSS grew {grew:.2f}GB past op base (cap {RSS_LIMIT_GB}GB) -- "
        f"a batch method is materializing an unbounded array")
    return grew


def _query_workers(n_points):
    """scipy spawns a fresh thread pool per query; parallelism only pays once
    the query is big enough to amortize pool setup (routing.query_workers)."""
    return -1 if n_points >= 4096 else 1


# ---------------------------------------------------------------------------
# Ported census primitives (census_core.py) -- self-contained, no path hacks.
# ---------------------------------------------------------------------------
def closest_on_tri(P, A, B, Cc):
    """Vectorized closest point on triangle (Ericson, Real-Time Collision
    Detection). All args (M,3). Verbatim from census_core.closest_on_tri."""
    ab = B - A; ac = Cc - A; ap = P - A
    d1 = (ab * ap).sum(1); d2 = (ac * ap).sum(1)
    bp = P - B; d3 = (ab * bp).sum(1); d4 = (ac * bp).sum(1)
    cp = P - Cc; d5 = (ab * cp).sum(1); d6 = (ac * cp).sum(1)
    vc = d1 * d4 - d3 * d2; vb = d5 * d2 - d1 * d6; va = d3 * d6 - d5 * d4
    denom = 1.0 / (va + vb + vc + 1e-300)
    v = (vb * denom)[:, None]; w = (vc * denom)[:, None]
    out = A + ab * v + ac * w
    with np.errstate(divide='ignore', invalid='ignore'):
        tAB = np.where((d1 - d3) != 0, d1 / (d1 - d3), 0.0)
        tAC = np.where((d2 - d6) != 0, d2 / (d2 - d6), 0.0)
        tBC = np.where(((d4 - d3) + (d5 - d6)) != 0, (d4 - d3) / ((d4 - d3) + (d5 - d6)), 0.0)
    cAB = A + ab * tAB[:, None]; cAC = A + ac * tAC[:, None]; cBC = B + (Cc - B) * tBC[:, None]
    mBC = (va <= 0) & ((d4 - d3) >= 0) & ((d5 - d6) >= 0)
    mAC = (vb <= 0) & (d2 >= 0) & (d6 <= 0)
    mAB = (vc <= 0) & (d1 >= 0) & (d3 <= 0)
    mC = (d6 >= 0) & (d5 <= d6); mB = (d3 >= 0) & (d4 <= d3); mA = (d1 <= 0) & (d2 <= 0)
    out = np.where(mBC[:, None], cBC, out)
    out = np.where(mAC[:, None], cAC, out)
    out = np.where(mAB[:, None], cAB, out)
    out = np.where(mC[:, None], Cc, out)
    out = np.where(mB[:, None], B, out)
    out = np.where(mA[:, None], A, out)
    return out


def _moller(O, Dv, V0, E1, E2, t_max=1.0):
    """Moller-Trumbore ray/segment vs a candidate triangle set.

    Ray point = O + t*Dv. Returns (hit_bool[M], t[M]) for t in [0, t_max].
    O,Dv are (3,) (one ray); V0,E1,E2 are (M,3). Locations only -- side of
    surface is decided by parity, never by this. Ported from
    census_core.ray_crossings (kept the face index by not collapsing to the
    hit subset)."""
    M = len(V0)
    Db = np.broadcast_to(Dv, (M, 3))
    pvec = np.cross(Db, E2)
    det = np.einsum('ij,ij->i', E1, pvec)
    valid = np.abs(det) > 1e-14
    inv = np.zeros_like(det); inv[valid] = 1.0 / det[valid]
    tvec = O - V0
    u = np.einsum('ij,ij->i', tvec, pvec) * inv
    qvec = np.cross(tvec, E1)
    v = np.einsum('ij,ij->i', Db, qvec) * inv
    t = np.einsum('ij,ij->i', E2, qvec) * inv
    hit = (valid & (u >= -EPS) & (v >= -EPS) & (u + v <= 1 + EPS)
           & (t >= -EPS) & (t <= t_max + EPS))
    return hit, t


def dedup_loc(ts, seg_len, tol_mm=3e-3):
    """Merge crossings closer than tol_mm in arc length (coincident /
    double-wall artifacts). Verbatim from census_core.dedup_loc."""
    if len(ts) == 0:
        return ts
    dt = tol_mm / max(seg_len, 1e-9)
    keep = [ts[0]]
    for x in ts[1:]:
        if x - keep[-1] > dt:
            keep.append(x)
    return np.array(keep)


def perp_basis(d):
    a = np.array([1.0, 0.0, 0.0]) if abs(d[0]) < 0.9 else np.array([0.0, 1.0, 0.0])
    e1 = np.cross(d, a); e1 /= (np.linalg.norm(e1) or 1.0)
    e2 = np.cross(d, e1); e2 /= (np.linalg.norm(e2) or 1.0)
    return e1, e2


def member_rays(p0, p1, r0, r1, n_off=8):
    """Axis ray + n capsule-offset rays. Verbatim from census_core."""
    d = p1 - p0
    L = float(np.linalg.norm(d))
    if L < 1e-9:
        return [("axis", p0.copy(), (p1 - p0), 0.0)]
    du = d / L
    rays = [("axis", p0.copy(), d.copy(), L)]
    e1, e2 = perp_basis(du)
    for k in range(n_off):
        th = 2 * np.pi * k / n_off
        u = np.cos(th) * e1 + np.sin(th) * e2
        a = p0 + r0 * u; b = p1 + r1 * u
        rays.append((f"off{k}", a, (b - a), float(np.linalg.norm(b - a))))
    return rays


class VerticalParity:
    """Robust inside/outside via axis-aligned ray parity (odd crossings on the
    +axis side = inside). Works directly on triangle soup (ray-triangle needs
    no topology). Verbatim from census_core.VerticalParity."""
    def __init__(self, mesh, h=2.0, ax=2):
        from collections import defaultdict
        self.ax = ax
        self.pp = [i for i in range(3) if i != ax]
        p = self.pp
        tri = np.asarray(mesh.triangles, float)
        self.A2 = tri[:, 0][:, p]; self.B2 = tri[:, 1][:, p]; self.C2 = tri[:, 2][:, p]
        self.Az = tri[:, 0, ax]; self.Bz = tri[:, 1, ax]; self.Cz = tri[:, 2, ax]
        self.h = h
        self.origin = mesh.bounds[0, p] - 1.0
        self.NY = int(np.floor((mesh.bounds[1, p[1]] + 1 - self.origin[1]) / h)) + 3
        xymin = tri[:, :, p].min(1); xymax = tri[:, :, p].max(1)
        cmin = np.floor((xymin - self.origin) / h).astype(int)
        cmax = np.floor((xymax - self.origin) / h).astype(int)
        grid = defaultdict(list)
        single = (cmin[:, 0] == cmax[:, 0]) & (cmin[:, 1] == cmax[:, 1])
        sidx = np.nonzero(single)[0]
        sid = cmin[sidx, 0] * self.NY + cmin[sidx, 1]
        order = np.argsort(sid, kind="stable"); sid_s = sid[order]; sidx_s = sidx[order]
        uq, starts = np.unique(sid_s, return_index=True)
        ends = list(starts[1:]) + [len(sid_s)]
        self.grid = {}
        for u, st, en in zip(uq, starts, ends):
            self.grid[int(u)] = sidx_s[st:en]
        extra = defaultdict(list)
        for ti in np.nonzero(~single)[0]:
            for ix in range(cmin[ti, 0], cmax[ti, 0] + 1):
                for iy in range(cmin[ti, 1], cmax[ti, 1] + 1):
                    extra[ix * self.NY + iy].append(ti)
        for k, v in extra.items():
            base = self.grid.get(k)
            self.grid[k] = np.concatenate([base, np.array(v, int)]) if base is not None else np.array(v, int)

    # per-cell (points x triangles) product budget -- bounds the 2D barycentric
    # intermediates when many surface points AND many triangles share one grid
    # cell (the 576k offset face-centroids of exterior_face_mask cluster densely
    # on the surface, unlike the census's sparse member samples).
    CELL_PRODUCT_BUDGET = 2_000_000

    def inside(self, pts, zdedup=5e-3):
        P = np.asarray(pts, float); N = len(P)
        res = np.zeros(N, bool)
        if N == 0:
            return res
        p = self.pp; axc = P[:, self.ax]
        c0 = P[:, p[0]]; c1 = P[:, p[1]]
        cid = (np.floor((c0 - self.origin[0]) / self.h).astype(int) * self.NY
               + np.floor((c1 - self.origin[1]) / self.h).astype(int))
        order = np.argsort(cid, kind="stable"); cid_s = cid[order]
        uq, starts = np.unique(cid_s, return_index=True)
        ends = list(starts[1:]) + [N]
        for u, st, en in zip(uq, starts, ends):
            pidx = order[st:en]
            cand = self.grid.get(int(u))
            if cand is None or len(cand) == 0:
                continue
            A = self.A2[cand]; B = self.B2[cand]; Cc = self.C2[cand]
            v0 = Cc - A; v1 = B - A
            den = v0[:, 0] * v1[:, 1] - v1[:, 0] * v0[:, 1]
            good = np.abs(den) > 1e-14
            ncand = len(cand)
            Az = self.Az[cand]; Bz = self.Bz[cand]; Cz = self.Cz[cand]
            # sub-block the cell's points so each (sub x ncand) intermediate
            # stays within the product budget (memory contract).
            sub = max(1, self.CELL_PRODUCT_BUDGET // max(1, ncand))
            for bs in range(0, len(pidx), sub):
                bidx = pidx[bs:bs + sub]
                pc0 = c0[bidx]; pc1 = c1[bidx]; pch = axc[bidx]
                d0 = pc0[:, None] - A[None, :, 0]; d1 = pc1[:, None] - A[None, :, 1]
                with np.errstate(divide="ignore", invalid="ignore"):
                    a = (d0 * v1[None, :, 1] - v1[None, :, 0] * d1) / den[None, :]
                    b = (v0[None, :, 0] * d1 - d0 * v0[None, :, 1]) / den[None, :]
                    # degenerate triangles (den~0) yield nan in a/b; the `good`
                    # mask excludes them -- keep the comparisons inside errstate
                    # so nan comparisons do not spam RuntimeWarnings (census
                    # suppressed these globally via np.seterr; we scope it).
                    insxy = good[None, :] & (a >= -1e-9) & (b >= -1e-9) & (a + b <= 1 + 1e-9)
                    z = (1 - a - b) * Az[None, :] + a * Cz[None, :] + b * Bz[None, :]
                # crossings STRICTLY ABOVE each point along +axis, deduped by
                # zdedup, parity -> inside. Fully vectorized over the sub-block
                # (replaces the per-point Python loop that made the 10M-sample
                # exterior_face_mask intractable); identical dedup semantics.
                above = insxy & (z > pch[:, None] + 1e-9)
                zc = np.where(above, z, np.inf)
                zc.sort(axis=1)                              # inf sinks to the tail
                nfin = np.isfinite(zc).sum(axis=1)
                if zc.shape[1] >= 2:
                    with np.errstate(invalid="ignore"):
                        dz = np.diff(zc, axis=1)            # inf-inf=nan in the tail, masked below
                    newc = (dz > zdedup) & np.isfinite(zc[:, 1:]) & np.isfinite(zc[:, :-1])
                    uniqn = np.where(nfin > 0, 1 + newc.sum(axis=1), 0)
                else:
                    uniqn = np.where(nfin > 0, 1, 0)
                res[bidx] = (uniqn % 2 == 1)
        return res


class ParityVote:
    """Majority of 3 axis-aligned ray-parity tests. inside = >= 2 of 3 axes
    report inside. Verbatim from census_core.ParityVote -- the ONLY sanctioned
    inside/outside classifier."""
    def __init__(self, mesh, h=2.0):
        self.vp = [VerticalParity(mesh, h=h, ax=a) for a in (0, 1, 2)]

    def inside(self, pts):
        pts = np.asarray(pts, float)
        s = np.zeros(len(pts), int)
        for vp in self.vp:
            s += vp.inside(pts).astype(int)
        return s >= 2


def _segseg(P1, Q1, P2, Q2):
    """Vectorized closest points / distance between two segments P1Q1 and
    P2Q2 (Ericson ClosestPtSegmentSegment, clamped form). All (M,3). Returns
    (dist[M], c1[M,3], c2[M,3]). P1,Q1 may be broadcast (one segment vs many
    edges)."""
    P1 = np.broadcast_to(P1, Q2.shape); Q1 = np.broadcast_to(Q1, Q2.shape)
    d1 = Q1 - P1; d2 = Q2 - P2; r = P1 - P2
    a = (d1 * d1).sum(1); e = (d2 * d2).sum(1); f = (d2 * r).sum(1)
    c = (d1 * r).sum(1); b = (d1 * d2).sum(1)
    denom = a * e - b * b
    with np.errstate(divide="ignore", invalid="ignore"):
        s = np.where(denom > 1e-14, (b * f - c * e) / denom, 0.0)
    s = np.clip(s, 0.0, 1.0)
    e_safe = np.where(e > 1e-14, e, 1.0)
    t = (b * s + f) / e_safe
    t = np.where(e > 1e-14, t, 0.0)
    t = np.clip(t, 0.0, 1.0)
    a_safe = np.where(a > 1e-14, a, 1.0)
    s = np.where(a > 1e-14, (b * t - c) / a_safe, 0.0)
    s = np.clip(s, 0.0, 1.0)
    c1 = P1 + d1 * s[:, None]
    c2 = P2 + d2 * t[:, None]
    dist = np.linalg.norm(c1 - c2, axis=1)
    return dist, c1, c2


class ExactSurface:
    """Surface-exact geometric oracle over a triangle mesh.

    One instance is built once per mesh and passed everywhere `surface_tree`
    flows today. Internals (numpy/scipy, ported from census_core) are
    replaceable by a native kernel behind this exact API.

    Batch methods enforce product-bounded batching (queries x candidate
    triangles, ROW_BUDGET rows; ragged crops concatenated, never padded) and a
    2GB RSS assert in every loop.
    """

    # ---- construction -----------------------------------------------------
    def __init__(self, mesh, grid_pitch_mm=2.0, large_cap=2000, verbose=False):
        tri = np.asarray(mesh.triangles, dtype=np.float64)
        self.V0 = tri[:, 0, :].copy()
        self.E1 = (tri[:, 1, :] - tri[:, 0, :]).copy()
        self.E2 = (tri[:, 2, :] - tri[:, 0, :]).copy()
        self.centroids = tri.mean(axis=1)
        self.tree = cKDTree(self.centroids)
        self.face_normals = np.asarray(mesh.face_normals, dtype=np.float64).copy()
        self.face_areas = np.asarray(mesh.area_faces, dtype=np.float64).copy()
        self.nfaces = len(self.V0)

        # per-triangle extent = max centroid-to-vertex distance
        ext = np.linalg.norm(tri - tri.mean(axis=1, keepdims=True), axis=2).max(axis=1)
        self.tri_extent_global = float(ext.max()) if self.nfaces else 0.0
        # large_thr DERIVED PER MESH (spec sec 1, review points 1+10). The
        # never-miss `large` set exists for ONE reason: to let the Tier-1 bound
        # tri_extent_nonlarge stay tight when a FEW genuinely oversized triangles
        # would otherwise inflate it. The spec's "smallest threshold keeping
        # |large| <= large_cap" is a CAP, not a target: on a near-uniform mesh
        # (all extents ~equal, e.g. a subdivided sphere) every threshold catches
        # ~large_cap triangles that do NOT tighten the bound (nonlarge ~= global
        # regardless) yet get appended to every candidate crop -- pure cost. So
        # the large set is kept ONLY when it tightens the bound by >2x; else it
        # is empty and the bound is the global max (which is already tight when
        # the distribution is uniform). This gives empty `large` on uniform
        # meshes (fast) and captures the real outlier tail on the Treant.
        self.large = np.empty(0, dtype=int)
        self.large_thr = self.tri_extent_global
        self.tri_extent_nonlarge = self.tri_extent_global
        if self.nfaces > large_cap:
            thr = float(np.partition(ext, self.nfaces - large_cap)[self.nfaces - large_cap])
            nonlarge = ext[ext <= thr]
            nonlarge_ext = float(nonlarge.max()) if len(nonlarge) else self.tri_extent_global
            if nonlarge_ext < 0.5 * self.tri_extent_global:  # the tail is real -> the set pays for itself
                self.large_thr = thr
                self.large = np.nonzero(ext > thr)[0]
                self.tri_extent_nonlarge = nonlarge_ext
        self._ext = ext

        # three parity grids (ax=0,1,2) for the 3-axis majority vote
        self.pv = ParityVote(mesh, h=grid_pitch_mm)

        self._exterior_mask = None  # lazy (sec 2.4)
        self._mesh_bounds = np.asarray(mesh.bounds, float).copy()
        self.build_log = dict(
            nfaces=self.nfaces, large_thr=round(self.large_thr, 4),
            n_large=int(len(self.large)),
            tri_extent_global=round(self.tri_extent_global, 4),
            tri_extent_nonlarge=round(self.tri_extent_nonlarge, 4),
            grid_pitch_mm=grid_pitch_mm)
        if verbose:
            print("ExactSurface build:", self.build_log, "RSS=%.2fGB" % rss_gb(), flush=True)

    # ---- exact nearest triangle (with certificate) ------------------------
    def _nearest_unsigned(self, pts, k0=64):
        """Exact unsigned point-to-surface (dist, tri_idx, closest_point).

        k-nearest centroids (+ never-miss `large`) -> closest_on_tri, plus an
        exactness CERTIFICATE (spec sec 1, review point 8): with d_k = distance
        to the k-th nearest centroid, the answer is certified exact iff
        best_dist + tri_extent_nonlarge <= d_k (no un-examined non-large
        triangle can beat best_dist; large triangles are always in the
        candidate set). Uncertified queries escalate via a centroid ball query
        at radius best_dist + tri_extent_nonlarge -- one step suffices and is
        exact. `query` is therefore exact by construction, at adaptive cost."""
        _base = rss_gb()
        pts = np.asarray(pts, float)
        N = len(pts)
        dist = np.full(N, np.inf)
        tri = np.full(N, -1, dtype=int)
        cp = np.zeros((N, 3))
        if N == 0:
            return dist, tri, cp
        large = self.large
        n_large = len(large)
        kk = min(k0, self.nfaces)
        dd, idx = self.tree.query(pts, k=kk, workers=_query_workers(N * kk))
        if kk == 1:
            idx = idx[:, None]; dd = dd[:, None]
        d_k = dd[:, -1]
        cand = idx
        if n_large:
            cand = np.concatenate([idx, np.broadcast_to(large, (N, n_large))], axis=1)
        bd, bt, bcp = self._closest_over_candidates(pts, cand)
        dist[:] = bd; tri[:] = bt; cp[:] = bcp
        # certificate: examined all non-large within d_k, plus all large.
        certified = (bd + self.tri_extent_nonlarge <= d_k + 1e-12)
        if kk >= self.nfaces:
            certified[:] = True
        unc = np.nonzero(~certified)[0]
        if len(unc):
            radii = bd[unc] + self.tri_extent_nonlarge + 1e-9
            ball = self.tree.query_ball_point(pts[unc], radii)
            for j, qi in enumerate(unc):
                c = np.asarray(ball[j], dtype=int)
                if n_large:
                    c = np.union1d(c, large)
                if len(c) == 0:
                    continue
                bd2, bt2, bcp2 = self._closest_over_candidates(pts[qi:qi + 1], c[None, :])
                if bd2[0] < dist[qi]:
                    dist[qi] = bd2[0]; tri[qi] = bt2[0]; cp[qi] = bcp2[0]
        _rss_ok(_base)
        return dist, tri, cp

    def _closest_over_candidates(self, q, cand):
        """q (N,3), cand (N,m) uniform candidate face indices. Returns
        (best_dist[N], best_tri[N], best_cp[N,3]). Product-bounded blocks over
        N so N_block * m <= ROW_BUDGET; ragged crops handled by the callers
        that build ragged candidate sets (segment_clearance/cast_rays)."""
        _base = rss_gb()
        q = np.asarray(q, float); N = len(q)
        m = cand.shape[1]
        best_d = np.full(N, np.inf); best_t = np.full(N, -1, int)
        best_cp = np.zeros((N, 3))
        block = max(1, min(N, ROW_BUDGET // max(1, m)))
        for s in range(0, N, block):
            e = min(s + block, N)
            qb = q[s:e]; cb = cand[s:e]; nb = e - s
            P = np.repeat(qb, m, axis=0)
            fi = cb.reshape(-1)
            A = self.V0[fi]; B = A + self.E1[fi]; Cc = A + self.E2[fi]
            cpt = closest_on_tri(P, A, B, Cc)
            d = np.linalg.norm(P - cpt, axis=1).reshape(nb, m)
            jb = np.argmin(d, axis=1); rows = np.arange(nb)
            best_d[s:e] = d[rows, jb]
            best_t[s:e] = cb[rows, jb]
            best_cp[s:e] = cpt.reshape(nb, m, 3)[rows, jb]
            _rss_ok(_base)
        return best_d, best_t, best_cp

    # ---- cKDTree-compatible facade ---------------------------------------
    def query(self, pts, distance_upper_bound=np.inf, workers=None, k=1):
        """EXACT unsigned point-to-surface distance, signature-compatible with
        cKDTree.query so passive consumers (removal.py diagnostics,
        connectivity_report) swap by duck-typing. Only k=1 is supported
        (the only form those consumers use). Points farther than
        distance_upper_bound return inf / index=nfaces, same contract as
        cKDTree."""
        if k != 1:
            raise NotImplementedError("ExactSurface.query supports k=1 only")
        pts = np.asarray(pts, float)
        scalar = pts.ndim == 1
        if scalar:
            pts = pts[None, :]
        dist, tri, _ = self._nearest_unsigned(pts)
        if np.isfinite(distance_upper_bound):
            over = dist > distance_upper_bound
            dist = dist.copy(); tri = tri.copy()
            dist[over] = np.inf
            tri[over] = self.nfaces
        if scalar:
            return float(dist[0]), int(tri[0])
        return dist, tri

    @property
    def n(self):
        """cKDTree.n compatibility (number of indexed items = faces here)."""
        return self.nfaces

    def project(self, pts):
        """Exact on-surface projection: (dist, closest_point, tri_idx,
        face_normal). dist is UNSIGNED (the sign is never trusted -- sec 0)."""
        pts = np.asarray(pts, float)
        scalar = pts.ndim == 1
        if scalar:
            pts = pts[None, :]
        dist, tri, cp = self._nearest_unsigned(pts)
        fn = self.face_normals[tri]
        if scalar:
            return float(dist[0]), cp[0], int(tri[0]), fn[0]
        return dist, cp, tri, fn

    # ---- inside/outside ---------------------------------------------------
    def inside(self, pts, batch=40000):
        """bool[N] -- 3-axis ray-parity MAJORITY vote (ParityVote). The ONLY
        sanctioned inside/outside classifier (sec 0)."""
        pts = np.asarray(pts, float)
        scalar = pts.ndim == 1
        if scalar:
            pts = pts[None, :]
        N = len(pts)
        out = np.zeros(N, bool)
        _base = rss_gb()
        for s in range(0, N, batch):
            e = min(s + batch, N)
            out[s:e] = self.pv.inside(pts[s:e])
            _rss_ok(_base)
        return bool(out[0]) if scalar else out

    # ---- ray casting ------------------------------------------------------
    def cast_rays(self, origins, dirs, t_max=1.0):
        """Batched Moller-Trumbore against KDTree-cropped candidates. Ray i is
        O_i + t*D_i for t in [0, t_max]. Returns a list of (t_sorted, faces)
        per ray -- crossing arc-parameters (deduped by location) and the hit
        face indices. Candidate crop per ray from segment-midpoint ball query;
        rays processed in product-bounded blocks; never rays x all-faces."""
        origins = np.atleast_2d(np.asarray(origins, float))
        dirs = np.atleast_2d(np.asarray(dirs, float))
        n = len(origins)
        results = [None] * n
        _base = rss_gb()
        # crop radius per ray = half segment + nonlarge extent (never-miss large
        # folded in explicitly so the crop radius uses the tight nonlarge bound)
        seglen = np.linalg.norm(dirs, axis=1) * t_max
        # gather ragged candidates, block by candidate-count product budget
        i = 0
        large = self.large
        while i < n:
            block_faces = []
            block_rayid = []
            j = i
            tot = 0
            while j < n:
                mid = origins[j] + 0.5 * t_max * dirs[j]
                R = 0.5 * seglen[j] + self.tri_extent_nonlarge + 1e-6
                cand = np.asarray(self.tree.query_ball_point(mid, R), dtype=int)
                if len(large):
                    cand = np.union1d(cand, large)
                if tot + len(cand) > ROW_BUDGET and j > i:
                    break
                block_faces.append(cand)
                block_rayid.append(np.full(len(cand), j, dtype=int))
                tot += len(cand)
                j += 1
            if not block_faces:
                i = j
                continue
            fi = np.concatenate(block_faces)
            rid = np.concatenate(block_rayid)
            if len(fi):
                O = origins[rid]; D = dirs[rid]
                V0 = self.V0[fi]; E1 = self.E1[fi]; E2 = self.E2[fi]
                Db = D
                pvec = np.cross(Db, E2)
                det = np.einsum('ij,ij->i', E1, pvec)
                valid = np.abs(det) > 1e-14
                inv = np.zeros_like(det); inv[valid] = 1.0 / det[valid]
                tvec = O - V0
                u = np.einsum('ij,ij->i', tvec, pvec) * inv
                qvec = np.cross(tvec, E1)
                v = np.einsum('ij,ij->i', Db, qvec) * inv
                t = np.einsum('ij,ij->i', E2, qvec) * inv
                hit = (valid & (u >= -EPS) & (v >= -EPS) & (u + v <= 1 + EPS)
                       & (t >= -EPS) & (t <= t_max + EPS))
            else:
                hit = np.zeros(0, bool); t = np.zeros(0)
            for rj in range(i, j):
                sel = hit & (rid == rj)
                if not sel.any():
                    results[rj] = (np.empty(0), np.empty(0, int))
                    continue
                tt = t[sel]; ff = fi[sel]
                order = np.argsort(tt, kind="stable")
                tt = tt[order]; ff = ff[order]
                # dedup coincident crossings by location along this ray
                keep_t = dedup_loc(tt, max(seglen[rj], 1e-9))
                # map kept t back to a representative face (first at each t)
                kf = []
                ptr = 0
                for kt in keep_t:
                    while ptr < len(tt) and tt[ptr] < kt - 1e-12:
                        ptr += 1
                    kf.append(ff[ptr] if ptr < len(ff) else ff[-1])
                results[rj] = (keep_t, np.asarray(kf, int))
            _rss_ok(_base)
            i = j
        return results

    # ---- exact segment-vs-triangle minimum distance -----------------------
    def _seg_tri_dist(self, A, B, faces):
        """Exact minimum distance from segment A->B to each triangle in
        `faces`. Returns (dist[M], tri_point[M,3]). Complete decomposition
        (sec 1): min over (a) endpoint-to-triangle closest_on_tri for BOTH
        endpoints, (b) segment-vs-each-of-3-triangle-edges segment/segment
        distance, (c) 0 if the segment pierces the triangle interior (Moller).
        These candidates are provably exhaustive for segment-vs-triangle."""
        M = len(faces)
        V0 = self.V0[faces]; e1 = self.E1[faces]; e2 = self.E2[faces]
        P = V0; Q = V0 + e1; R = V0 + e2
        Ab = np.broadcast_to(A, (M, 3)); Bb = np.broadcast_to(B, (M, 3))
        best_d = np.full(M, np.inf); best_pt = np.zeros((M, 3))

        def _upd(d, pt):
            nonlocal best_d, best_pt
            m = d < best_d
            best_d = np.where(m, d, best_d)
            best_pt = np.where(m[:, None], pt, best_pt)

        # (a) endpoints vs triangle
        cptA = closest_on_tri(Ab, P, Q, R)
        _upd(np.linalg.norm(Ab - cptA, axis=1), cptA)
        cptB = closest_on_tri(Bb, P, Q, R)
        _upd(np.linalg.norm(Bb - cptB, axis=1), cptB)
        # (b) segment vs each triangle edge
        for (X, Y) in ((P, Q), (Q, R), (R, P)):
            d, _c1, c2 = _segseg(A, B, X, Y)
            _upd(d, c2)
        # (c) interior pierce -> distance 0 at the crossing point
        Dv = (B - A)
        hit, t = _moller(A, Dv, V0, e1, e2, t_max=1.0)
        if hit.any():
            pierce_pt = Ab + t[:, None] * Dv[None, :]
            _upd(np.where(hit, 0.0, np.inf), pierce_pt)
        return best_d, best_pt

    def segment_clearance(self, p0s, p1s, r0s, r1s, standoff,
                          inset0s=None, inset1s=None, chunk_mm=5.0):
        """Per segment: min over the sub-segment [inset0, L-inset1] of
        (exact_capsule_axis_distance - r_max - standoff), plus the argmin
        surface point. Returns (clearance[N], argmin_point[N,3]).

        EXACT segment-vs-triangle minimum distance (NOT path sampling). The
        sub-segment excludes the sanctioned embed zones (sec 0). Linear taper
        handled by the CONSERVATIVE r_max = max(r over the sub-segment) fallback
        the spec sanctions (may only over-flag, never under-flag; the tapered
        per-triangle solve is a follow-up). Two-tier for speed: an empty local
        candidate crop (open space) certifies the segment clear at KDTree cost
        (Tier-1); only near-surface segments run the exact pass (Tier-2). Crops
        use the tight tri_extent_nonlarge radius with the never-miss `large`
        list folded in, so a few huge triangles do not inflate every crop."""
        p0s = np.atleast_2d(np.asarray(p0s, float))
        p1s = np.atleast_2d(np.asarray(p1s, float))
        N = len(p0s)
        r0s = np.broadcast_to(np.asarray(r0s, float), (N,))
        r1s = np.broadcast_to(np.asarray(r1s, float), (N,))
        inset0s = np.zeros(N) if inset0s is None else np.broadcast_to(np.asarray(inset0s, float), (N,))
        inset1s = np.zeros(N) if inset1s is None else np.broadcast_to(np.asarray(inset1s, float), (N,))
        clr = np.full(N, np.inf)
        argp = np.full((N, 3), np.nan)
        _base = rss_gb()
        for i in range(N):
            clr[i], argp[i] = self._one_segment_clearance(
                p0s[i], p1s[i], float(r0s[i]), float(r1s[i]), standoff,
                float(inset0s[i]), float(inset1s[i]), chunk_mm)
            _rss_ok(_base)
        return clr, argp

    def _one_segment_clearance(self, p0, p1, r0, r1, standoff, inset0, inset1, chunk_mm):
        d = p1 - p0
        L = float(np.linalg.norm(d))
        if L < 1e-9:
            return np.inf, np.full(3, np.nan)
        u = d / L
        s_lo = inset0
        s_hi = L - inset1
        if s_hi - s_lo <= 1e-9:
            return np.inf, np.full(3, np.nan)  # entirely inside the embed zones
        A = p0 + u * s_lo
        B = p0 + u * s_hi
        r_lo = r0 + (r1 - r0) * (s_lo / L)
        r_hi = r0 + (r1 - r0) * (s_hi / L)
        r_max = max(r_lo, r_hi)              # conservative over the sub-segment
        thresh = r_max + standoff
        seglen = s_hi - s_lo
        # chunk the sub-segment so crop radii stay local on long members
        nchunk = max(1, int(np.ceil(seglen / chunk_mm)))
        cand_set = set()
        for c in range(nchunk):
            a = s_lo + seglen * c / nchunk
            b = s_lo + seglen * (c + 1) / nchunk
            mid = p0 + u * (0.5 * (a + b))
            half = 0.5 * (b - a)
            R = half + thresh + self.tri_extent_nonlarge + 1e-6
            cand_set.update(int(x) for x in self.tree.query_ball_point(mid, R))
        if len(self.large):
            cand_set.update(int(x) for x in self.large)
        if not cand_set:
            # Tier-1 certified clear (open space): no non-large centroid within
            # the sound bound of any chunk, and no large triangle exists.
            dmin, _ = self.tree.query(np.array([A, B]), k=1)
            return float(dmin.min() - thresh), np.full(3, np.nan)
        faces = np.fromiter(cand_set, dtype=int, count=len(cand_set))
        best_d = np.inf; best_pt = np.full(3, np.nan)
        _base = rss_gb()
        block = max(1, ROW_BUDGET // 8)  # _seg_tri_dist keeps ~6 (M,3) arrays
        for s in range(0, len(faces), block):
            fb = faces[s:s + block]
            d_m, pt_m = self._seg_tri_dist(A, B, fb)
            j = int(np.argmin(d_m))
            if d_m[j] < best_d:
                best_d = float(d_m[j]); best_pt = pt_m[j]
            _rss_ok(_base)
        return best_d - thresh, best_pt

    # ---- penetration intervals (census PASS A-C) --------------------------
    def penetration_intervals(self, p0, p1, r0, r1, n_offset_rays=8,
                              majority_step_mm=0.12, cap=80, min_illegal_len_mm=0.05,
                              rays="axis"):
        """Census PASS A-C for one member. Moller crossings partition each ray;
        each sub-segment is classified inside/outside by a dense-MAJORITY parity
        vote of its interior samples; maximal inside runs become intervals.
        EXACT boundary locations; parity (never signed distance) for side of
        surface.

        rays="axis" (default) returns intervals along the AXIS ray only -- the
        load-bearing semantics for the stub through-penetration rule (sec 2.1,
        census axis-only) and the mid-path belt (sec 2.1). rays="all" adds the
        n capsule-offset rays (census non-stub PASS A). Returns a list of dicts
        {s_a, s_b, entered, exited, max_depth_mm, ray} in arc length along each
        ray. Only intervals longer than min_illegal_len_mm are returned
        (census MIN_ILLEGAL_LEN)."""
        p0 = np.asarray(p0, float); p1 = np.asarray(p1, float)
        L = float(np.linalg.norm(p1 - p0))
        if L < 1e-9:
            return []
        if rays == "axis":
            raylist = [("axis", p0.copy(), (p1 - p0), L)]
        else:
            raylist = member_rays(p0, p1, r0, r1, n_off=n_offset_rays)
        center = 0.5 * (p0 + p1)
        rmax = max(r0, r1)
        R = 0.5 * L + rmax + self.tri_extent_global + 0.6
        idx = np.asarray(self.tree.query_ball_point(center, R), dtype=int)
        if len(self.large):
            idx = np.union1d(idx, self.large)
        out = []
        if len(idx) == 0:
            return out
        _base = rss_gb()
        V0 = self.V0[idx]; E1 = self.E1[idx]; E2 = self.E2[idx]
        for label, O, Dv, seg in raylist:
            if seg < 1e-9:
                continue
            hit, tvals = _moller(O, Dv, V0, E1, E2, t_max=1.0)
            ts = dedup_loc(np.sort(tvals[hit]), seg) if hit.any() else np.empty(0)
            b = np.concatenate([[0.0], ts, [1.0]])
            # dense samples per sub-segment
            sub_bounds = []
            sample_pts = []
            slices = []
            run = 0
            for si in range(len(b) - 1):
                ta, tb = b[si], b[si + 1]
                Lsub = (tb - ta) * seg
                ns = min(cap, max(3, int(Lsub / majority_step_mm)))
                tt = np.linspace(ta, tb, ns + 2)[1:-1]
                sp = O[None, :] + tt[:, None] * Dv[None, :]
                sample_pts.append(sp)
                slices.append((run, run + len(sp)))
                run += len(sp)
                sub_bounds.append((ta, tb, si != 0, (si + 1) != (len(b) - 1)))
            allp = np.concatenate(sample_pts) if sample_pts else np.zeros((0, 3))
            ins_all = self.inside(allp) if len(allp) else np.zeros(0, bool)
            # depth: unsigned distance of the inside samples to surface
            if ins_all.any():
                dist_all = np.full(len(allp), 0.0)
                di, _, _ = self._nearest_unsigned(allp[ins_all])
                dist_all[ins_all] = di
            else:
                dist_all = np.zeros(len(allp))
            insub = []
            depsub = []
            for (a0, a1) in slices:
                si_ins = ins_all[a0:a1]
                insub.append(bool(si_ins.mean() >= 0.5) if len(si_ins) else False)
                dd = dist_all[a0:a1][si_ins] if len(si_ins) else np.array([])
                depsub.append(float(dd.max()) if len(dd) else 0.0)
            insub = np.array(insub); nsub = len(insub)
            i = 0
            while i < nsub:
                if insub[i]:
                    jrun = i
                    while jrun + 1 < nsub and insub[jrun + 1]:
                        jrun += 1
                    ta = sub_bounds[i][0]; tb = sub_bounds[jrun][1]
                    entered = sub_bounds[i][2]; exited = sub_bounds[jrun][3]
                    depth = max(depsub[i:jrun + 1])
                    s_a = ta * seg; s_b = tb * seg
                    if (s_b - s_a) > min_illegal_len_mm:
                        out.append(dict(s_a=float(s_a), s_b=float(s_b),
                                        entered=bool(entered), exited=bool(exited),
                                        max_depth_mm=float(depth), ray=label))
                    i = jrun + 1
                else:
                    i += 1
            _rss_ok(_base)
        return out

    # ---- sphere / ball breach (census ball test) --------------------------
    def sphere_breach(self, centers, radii, eps_mm=0.05, half_deg=25.0, n_cone=8):
        """Census ball breach test. For spheres that reach the surface
        (r - project(c).dist > eps), measure local feature thickness along the
        inward-normal +/- half_deg cone (1 + n_cone Moller rays, skipping the
        near wall); breach iff thickness < r - eps. Returns arrays
        (breached[N] bool, feature_thickness_mm[N], poke_out_mm[N]). A ball
        anchored shallow on a thick feature passes by construction (the test
        measures far-wall thickness, not surface contact)."""
        centers = np.atleast_2d(np.asarray(centers, float))
        N = len(centers)
        radii = np.broadcast_to(np.asarray(radii, float), (N,))
        breached = np.zeros(N, bool)
        thick_out = np.full(N, np.inf)
        poke_out = np.zeros(N)
        if N == 0:
            return breached, thick_out, poke_out
        _base = rss_gb()
        nd, _cp, _ti, fn = self.project(centers)  # unsigned surface distance + face normal
        for i in range(N):
            r = float(radii[i])
            if r - nd[i] <= eps_mm:
                continue  # sphere does not cross the surface -> nothing to breach
            c = centers[i]
            inward = -fn[i]
            inward = inward / (np.linalg.norm(inward) or 1.0)
            idx = np.asarray(self.tree.query_ball_point(c, r + self.tri_extent_global + 0.1), dtype=int)
            if len(self.large):
                idx = np.union1d(idx, self.large)
            if len(idx) == 0:
                continue
            V0 = self.V0[idx]; E1 = self.E1[idx]; E2 = self.E2[idx]
            thick = np.inf
            for dcone in _cone_dirs(inward, half_deg, n_cone):
                hit, tv = _moller(c, dcone * r, V0, E1, E2, t_max=1.0)
                if hit.any():
                    dd = tv[hit] * r
                    dd = dd[dd > nd[i] + 0.05]  # skip the near wall
                    if len(dd):
                        thick = min(thick, float(dd.min()))
            thick_out[i] = thick
            if thick == np.inf or thick >= r - eps_mm:
                continue  # feature thicker than the ball -> sanctioned shallow anchor
            breached[i] = True
            poke_out[i] = r - thick
            _rss_ok(_base)
        return breached, thick_out, poke_out

    # ---- exterior-face mask (sec 2.4) -------------------------------------
    def exterior_face_mask(self, gap_warn_mm=0.15):
        """bool[nfaces], cached. A face INTERIOR to the solid union (the STL is
        a non-watertight overlapping assembly) has solid on BOTH sides, so an
        outward-offset point tests inside.

        Robust classification (spec sec 1, review point 3 -- a mis-vote can
        silently DELETE real support demand): 3 interior barycentric points x 2
        offsets eps in {0.05, 0.15}mm along +normal = 6 parity samples per face;
        face is INTERIOR iff a strict majority (>= 4/6) test inside. FAIL-OPEN:
        ties / ambiguous faces classify EXTERIOR (a mis-kept interior face costs
        resin; a mis-dropped overhang costs the print). A build-time inter-shell
        gap diagnostic (outward-normal first-crossing distance for a sample of
        faces) is logged; if a material fraction of gaps falls below the larger
        eps the ladder should be re-derived.

        PERF NOTE (follow-up for the perception consumer swap, sec 2.4): a
        one-time, cached, per-run cost. At full 576k-face scale the 6-sample x
        3-axis vote is ~minutes (measured; dense crown grid cells are
        superlinear), NOT the "seconds" spec sec 5 optimistically projected. It
        is on no hot loop and nothing consumes it yet, so it does not gate this
        core delivery; before the perception swap it wants a Tier-1-style
        pre-pass (a face whose outward-normal ray escapes with no re-entry is
        trivially exterior and skips the full vote) or the native kernel.
        Correctness is unaffected."""
        if self._exterior_mask is not None:
            return self._exterior_mask
        n = self.nfaces
        fn = self.face_normals
        # 3 fixed interior barycentric points (centroid + two off-center)
        bary = np.array([[1 / 3, 1 / 3, 1 / 3], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2]])
        A = self.V0; B = self.V0 + self.E1; C = self.V0 + self.E2
        base_pts = []
        for (wa, wb, wc) in bary:
            base_pts.append(wa * A + wb * B + wc * C)  # (n,3)
        eps_ladder = (0.05, 0.15)
        # Issue all 6 sample sets (3 barycentric x 2 eps) as ONE combined
        # inside() call rather than 6 separate 3-axis passes: identical result,
        # fewer per-cell grouping setups. (The dominant per-cell parity cost is
        # quadratic in points x triangles and thus invariant to batching; the
        # real speedup lever for the consumer swap is a finer parity grid --
        # halving grid_pitch cuts the per-cell quadratic ~4x -- and a ray-escape
        # pre-pass; see the method docstring PERF NOTE.)
        sample_sets = [bp + fn * eps for bp in base_pts for eps in eps_ladder]
        Q = np.concatenate(sample_sets, axis=0)                # (6n, 3)
        ins = self.inside(Q)
        votes = ins.reshape(len(sample_sets), n).sum(axis=0)
        interior = votes >= 4
        self._exterior_mask = ~interior
        # gap diagnostic (logged, not gating): outward first-crossing distances
        try:
            self._gap_diag = self._gap_diagnostic(gap_warn_mm)
        except Exception as ex:  # diagnostic must never break the mask
            self._gap_diag = dict(error=str(ex))
        return self._exterior_mask

    def _gap_diagnostic(self, gap_warn_mm, sample=1500):
        n = self.nfaces
        rng = np.random.RandomState(0)  # seeded, diagnostic only
        sel = np.arange(n) if n <= sample else rng.choice(n, sample, replace=False)
        cents = self.centroids[sel]
        fn = self.face_normals[sel]
        origins = cents + fn * 1e-4  # nudge off the face so t=0 self-hit is skipped
        # cast a short outward ray; first crossing = distance through any other
        # shell material sitting outside this face
        far = 4.0
        res = self.cast_rays(origins, fn * far, t_max=1.0)
        gaps = []
        for (ts, _f) in res:
            if len(ts):
                gaps.append(float(ts.min() * far))
        gaps = np.array(gaps)
        if len(gaps) == 0:
            return dict(n_sampled=int(len(sel)), n_with_far_wall=0)
        frac_below = float((gaps < gap_warn_mm).mean())
        return dict(n_sampled=int(len(sel)), n_with_far_wall=int(len(gaps)),
                    gap_p01=round(float(np.percentile(gaps, 1)), 4),
                    gap_p05=round(float(np.percentile(gaps, 5)), 4),
                    gap_median=round(float(np.median(gaps)), 4),
                    frac_below_warn=round(frac_below, 4), warn_mm=gap_warn_mm)


def _cone_dirs(axis, half_deg=25.0, n=8):
    """Central axis + n directions on a cone of half-angle half_deg around it.
    Ported from census_run.cone_dirs."""
    axis = axis / (np.linalg.norm(axis) or 1.0)
    e1 = perp_basis(axis)[0]; e2 = np.cross(axis, e1)
    th = np.radians(half_deg); dirs = [axis]
    for k in range(n):
        ph = 2 * np.pi * k / n
        d = np.cos(th) * axis + np.sin(th) * (np.cos(ph) * e1 + np.sin(ph) * e2)
        dirs.append(d / np.linalg.norm(d))
    return dirs
