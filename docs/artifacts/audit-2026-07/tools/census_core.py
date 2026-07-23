"""Exact-geometry penetration census core.

Robust hybrid method (mesh is winding-consistent but NOT watertight):
  1. Moller-Trumbore gives EXACT surface-crossing LOCATIONS (t) of a finite
     segment (member axis or a capsule-offset line) against local triangles.
  2. The crossings partition the segment; each sub-segment's MIDPOINT is tested
     inside/outside via nearest-triangle signed distance. Maximal runs of
     "inside" sub-segments = penetration intervals. Decoupling boundaries
     (Moller) from side-of-surface (point sign) is robust to coincident /
     double-wall / thin-shell artifacts that break pure winding parity.

Memory-bounded: per-member cKDTree crop of candidate triangles; two-pass so a
single batched nearest-face query serves all midpoints.
"""
import numpy as np, os, sys, pickle, resource, trimesh
from scipy.spatial import cKDTree

WT = "/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad/wt-emit-final/scripts"
sys.path.insert(0, WT); sys.path.insert(0, os.path.join(WT, "support_placement"))
SCR = "/private/tmp/claude-501/-Users-craig-Documents-Claude-Projects-D-D-Campaigns/f0760b14-ba8b-49b8-8c2c-e73e83d65858/scratchpad"
STATE = "/Users/craig/.claude/jobs/0ddf6d41/tmp/engine_run_state.pkl"

def rss_gb():
    r = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return (r / 1e9) if sys.platform == "darwin" else (r / 1e6)

def load_all():
    S = pickle.load(open(STATE, "rb"))
    mesh = trimesh.load(os.path.join(SCR, "audit", "treant_real_copy.stl"), process=False)
    ctr = (mesh.bounds[0] + mesh.bounds[1]) / 2.0
    mesh.apply_translation(-ctr)
    return S, mesh

class MeshIndex:
    """Triangle store for Moller-Trumbore + centroid KDTree crop."""
    def __init__(self, mesh, large_thr=1.5):
        tri = np.asarray(mesh.triangles, dtype=np.float64)
        self.V0 = tri[:, 0, :].copy()
        self.E1 = (tri[:, 1, :] - tri[:, 0, :]).copy()
        self.E2 = (tri[:, 2, :] - tri[:, 0, :]).copy()
        self.centroids = tri.mean(axis=1)
        self.tree = cKDTree(self.centroids)
        ext = np.linalg.norm(tri - tri.mean(axis=1, keepdims=True), axis=2).max(axis=1)
        self.tri_extent = float(ext.max())
        self.large = np.nonzero(ext > large_thr)[0]   # never-miss safety net
        self.nfaces = len(self.V0)
        del tri
    def candidates(self, mid, radius):
        return np.array(self.tree.query_ball_point(mid, radius), dtype=int)

EPS = 1e-6

def ray_crossings(O, D, idx, MI):
    """Return sorted t-values (in [0,1]) where segment O->O+D crosses the
    candidate triangles idx. Locations only (sign handled elsewhere)."""
    if len(idx) == 0:
        return np.empty(0)
    V0 = MI.V0[idx]; E1 = MI.E1[idx]; E2 = MI.E2[idx]
    Db = np.broadcast_to(D, E2.shape)
    pvec = np.cross(Db, E2)
    det = np.einsum('ij,ij->i', E1, pvec)
    valid = np.abs(det) > 1e-14
    inv = np.zeros_like(det); inv[valid] = 1.0 / det[valid]
    tvec = O - V0
    u = np.einsum('ij,ij->i', tvec, pvec) * inv
    qvec = np.cross(tvec, E1)
    v = np.einsum('ij,ij->i', Db, qvec) * inv
    t = np.einsum('ij,ij->i', E2, qvec) * inv
    hit = valid & (u >= -EPS) & (v >= -EPS) & (u + v <= 1 + EPS) & (t >= -EPS) & (t <= 1 + EPS)
    th = np.sort(t[hit])
    return th

def dedup_loc(ts, seg_len, tol_mm=3e-3):
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

def closest_on_tri(P, A, B, Cc):
    """Vectorized closest point on triangle (Ericson). All (M,3)."""
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

class VerticalParity:
    """Robust inside/outside via ray parity along axis `ax` (odd crossings on
    the +axis side = inside). Works directly on triangle soup (ray-triangle
    needs no topology). A 2D grid over the two perpendicular coords buckets
    triangles by bbox for fast candidate lookup; query points grouped by cell
    for vectorized barycentric point-in-triangle. Faces parallel to the ray
    (zero projected area) are naturally ignored. Memory-light & fast."""
    def __init__(self, mesh, h=2.0, ax=2):
        from collections import defaultdict
        self.ax = ax
        self.pp = [i for i in range(3) if i != ax]      # perpendicular coords
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
            pc0 = c0[pidx]; pc1 = c1[pidx]; pch = axc[pidx]
            A = self.A2[cand]; B = self.B2[cand]; Cc = self.C2[cand]
            v0 = Cc - A; v1 = B - A
            den = v0[:, 0] * v1[:, 1] - v1[:, 0] * v0[:, 1]
            good = np.abs(den) > 1e-14
            d0 = pc0[:, None] - A[None, :, 0]; d1 = pc1[:, None] - A[None, :, 1]
            with np.errstate(divide="ignore", invalid="ignore"):
                a = (d0 * v1[None, :, 1] - v1[None, :, 0] * d1) / den[None, :]
                b = (v0[None, :, 0] * d1 - d0 * v0[None, :, 1]) / den[None, :]
            insxy = good[None, :] & (a >= -1e-9) & (b >= -1e-9) & (a + b <= 1 + 1e-9)
            z = (1 - a - b) * self.Az[None, cand] + a * self.Cz[None, cand] + b * self.Bz[None, cand]
            for r, pi in enumerate(pidx):
                m = insxy[r]
                if not m.any():
                    continue
                zc = z[r, m]
                zc = zc[zc > pch[r] + 1e-9]
                if len(zc) == 0:
                    continue
                zc = np.sort(zc)
                uniqn = 1 + int((np.diff(zc) > zdedup).sum())
                res[pi] = (uniqn % 2 == 1)
        return res

class ParityVote:
    """Majority of 3 axis-aligned ray-parity tests. As robust as many-ray
    voting, fast. inside = at least 2 of 3 axes report inside."""
    def __init__(self, mesh, h=2.0):
        self.vp = [VerticalParity(mesh, h=h, ax=a) for a in (0, 1, 2)]
    def inside(self, pts):
        pts = np.asarray(pts, float)
        s = np.zeros(len(pts), int)
        for vp in self.vp:
            s += vp.inside(pts).astype(int)
        return s >= 2

class SignedTester:
    """Memory-bounded signed distance: k-nearest triangles (centroid KDTree)
    + exact point-to-triangle. inside = signed < 0. `flip` orients normals."""
    def __init__(self, mesh, MI, k=64, flip=False):
        self.MI = MI
        self.fn = np.asarray(mesh.face_normals, float)
        self.k = k
        self.flip = flip
    def signed(self, pts, batch=800):
        pts = np.asarray(pts, float)
        if len(pts) == 0:
            return np.empty(0)
        MI = self.MI; k = self.k; large = MI.large
        out = np.empty(len(pts))
        for s in range(0, len(pts), batch):
            q = pts[s:s + batch]; N = len(q)
            _, idx = MI.tree.query(q, k=k)
            idx = np.atleast_2d(idx)
            # candidate face indices per query = k-nearest UNION all large tris
            cand = np.concatenate([idx, np.broadcast_to(large, (N, len(large)))], axis=1)
            m = cand.shape[1]
            P = np.repeat(q, m, axis=0)
            fi = cand.reshape(-1)
            A = MI.V0[fi]; B = A + MI.E1[fi]; Cc = A + MI.E2[fi]
            cpt = closest_on_tri(P, A, B, Cc)
            d = np.linalg.norm(P - cpt, axis=1).reshape(N, m)
            jb = np.argmin(d, axis=1); rows = np.arange(N)
            bfi = cand[rows, jb]
            bcp = cpt.reshape(N, m, 3)[rows, jb]
            dist = d[rows, jb]
            outward = ((q - bcp) * self.fn[bfi]).sum(1)
            out[s:s + N] = np.where(outward < 0, -1.0, 1.0) * dist
        return -out if self.flip else out
    def inside(self, pts):
        return self.signed(pts) < 0

    def nearest(self, pts, batch=800):
        """Return (signed_dist, tri_index, closest_point, outward_normal)."""
        pts = np.asarray(pts, float); N = len(pts)
        MI = self.MI; k = self.k; large = MI.large
        sd = np.empty(N); ti = np.empty(N, int)
        cp = np.empty((N, 3))
        for s in range(0, N, batch):
            q = pts[s:s + batch]; nb = len(q)
            _, idx = MI.tree.query(q, k=k); idx = np.atleast_2d(idx)
            cand = np.concatenate([idx, np.broadcast_to(large, (nb, len(large)))], axis=1)
            m = cand.shape[1]
            P = np.repeat(q, m, axis=0); fi = cand.reshape(-1)
            A = MI.V0[fi]; B = A + MI.E1[fi]; Cc = A + MI.E2[fi]
            cpt = closest_on_tri(P, A, B, Cc)
            d = np.linalg.norm(P - cpt, axis=1).reshape(nb, m)
            jb = np.argmin(d, axis=1); rows = np.arange(nb)
            bfi = cand[rows, jb]; bcp = cpt.reshape(nb, m, 3)[rows, jb]; dist = d[rows, jb]
            outward = ((q - bcp) * self.fn[bfi]).sum(1)
            sgn = np.where(outward < 0, -1.0, 1.0)
            sd[s:s + nb] = (-sgn if self.flip else sgn) * dist
            ti[s:s + nb] = bfi; cp[s:s + nb] = bcp
        fnrm = self.fn[ti] * (-1.0 if self.flip else 1.0)
        return sd, ti, cp, fnrm
