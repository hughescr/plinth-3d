"""KernelSurface — drop-in Rust-backed replacement for ExactSurface.

SPEC: docs/specs/SPEC-RUST-KERNEL.md. This wrapper owns the numpy-boundary
conventions (atleast_2d, scalar passthrough, ragged reassembly, broadcasting)
so the Rust `geom_kernel._native` layer only ever sees clean C-contiguous
f64/i64 arrays. Production code goes through THIS class, never the native
module directly (spec sec 1.1).

The native layer copies every input batch into Rust-owned buffers, releases the
GIL for the pure-Rust compute, and returns freshly allocated numpy arrays
(Amendment A16). Determinism is bit-identical across runs AND thread counts
(GEOM_KERNEL_THREADS; spec sec 4).

Tier-1 deviation (Amendment A1/A14): segment_clearance always returns the EXACT
segment-to-surface clearance and a real argmin point; it does NOT reproduce the
oracle's open-space certificate value (endpoint-to-centroid distance − thresh)
or its NaN argmin. This is verdict-preserving (clr_kernel ≤ clr_oracle, both
positive in the oracle's Tier-1 regime). R9: any NEW consumer comparing the
returned clearance VALUE against a threshold must repeat the A14
outcome-equivalence analysis and add a fixture.
"""
import time

import numpy as np

try:
    import geom_kernel._native as _native
except ImportError as ex:  # pragma: no cover - surfaced by the conftest gate
    raise ImportError(
        "geom_kernel._native is not built. Build it with:\n"
        "  scripts/build_kernel.sh\n"
        "(or: cd rust/geom_kernel && ../../.venv/bin/maturin develop --release)"
    ) from ex

EXPECTED_API_VERSION = 1
EXPECTED_NUMPY_MAJOR = 2  # venv runs numpy 2.x; the numpy crate matches (A20)

if _native.KERNEL_API_VERSION != EXPECTED_API_VERSION:
    raise ImportError(
        f"geom_kernel API version mismatch: native={_native.KERNEL_API_VERSION} "
        f"expected={EXPECTED_API_VERSION} — rebuild: scripts/build_kernel.sh"
    )
if int(np.__version__.split(".")[0]) != EXPECTED_NUMPY_MAJOR:
    raise ImportError(
        f"numpy major {np.__version__} != expected {EXPECTED_NUMPY_MAJOR}; the "
        f"geom_kernel numpy-crate ABI is pinned to numpy {EXPECTED_NUMPY_MAJOR}.x "
        f"(spec sec 1.2 Amendment A20) — rebuild against the new numpy."
    )


def _finite_or_raise(a, name):
    if not np.all(np.isfinite(a)):
        raise ValueError(f"{name} contains non-finite values (NaN/inf)")


class KernelSurface:
    """Surface-exact geometric oracle over a triangle mesh, Rust-backed.

    Drop-in for geometry_exact.ExactSurface: same constructor signature, same
    methods, same shapes/dtypes, same scalar/batch conventions, and the same
    data attributes consumers read (.centroids, .face_normals, .face_areas,
    .nfaces, .n, .build_log).
    """

    def __init__(self, mesh, grid_pitch_mm=2.0, large_cap=2000, verbose=False):
        # large_cap is accepted for signature parity and ignored (the BVH has no
        # never-miss set — spec sec 2.2).
        tri = np.ascontiguousarray(np.asarray(mesh.triangles, dtype=np.float64))
        if tri.ndim != 3 or tri.shape[1:] != (3, 3):
            raise ValueError(f"mesh.triangles must be (n,3,3), got {tri.shape}")
        if len(tri) == 0:
            raise ValueError("empty mesh (0 faces) — cannot build a surface")
        _finite_or_raise(tri, "mesh.triangles")
        if grid_pitch_mm <= 0:
            raise ValueError("grid_pitch_mm must be > 0")

        face_normals = np.ascontiguousarray(np.asarray(mesh.face_normals, dtype=np.float64))
        face_areas = np.ascontiguousarray(np.asarray(mesh.area_faces, dtype=np.float64))
        bounds = np.ascontiguousarray(np.asarray(mesh.bounds, dtype=np.float64))

        t0 = time.perf_counter()
        self._native = _native.NativeSurfacePy(
            tri, face_normals, face_areas, bounds, float(grid_pitch_mm)
        )
        t_build_ms = (time.perf_counter() - t0) * 1e3

        self.nfaces = int(self._native.nfaces)
        self.grid_pitch_mm = float(grid_pitch_mm)
        self.face_normals = face_normals
        self.face_areas = face_areas
        # .centroids: byte-identical to the oracle (geometry_exact.py:331)
        self.centroids = tri.mean(axis=1)
        # raw edge arrays (some diagnostics/tests read these on the oracle)
        self.V0 = tri[:, 0, :].copy()
        self.E1 = (tri[:, 1, :] - tri[:, 0, :]).copy()
        self.E2 = (tri[:, 2, :] - tri[:, 0, :]).copy()
        self._exterior_mask = None
        self._gap_diag = None

        blog = self._native.build_log()
        # retired oracle keys kept as None for one transition release (A18)
        blog.update(large_thr=None, n_large=None, tri_extent_global=None,
                    tri_extent_nonlarge=None, t_build_ms=round(t_build_ms, 3))
        self.build_log = blog
        if verbose:
            print("KernelSurface build:", self.build_log, flush=True)

    # ---- cKDTree-compatible facade ---------------------------------------
    def query(self, pts, distance_upper_bound=np.inf, workers=None, k=1):
        if k != 1:
            raise NotImplementedError("KernelSurface.query supports k=1 only")
        pts = np.asarray(pts, dtype=np.float64)
        scalar = pts.ndim == 1
        p = np.ascontiguousarray(np.atleast_2d(pts))
        if p.shape[1] != 3:
            raise ValueError(f"query points must be (...,3), got {pts.shape}")
        _finite_or_raise(p, "query pts")
        if len(p) == 0:
            return np.empty(0), np.empty(0, dtype=np.int64)
        dub = float(distance_upper_bound)
        dist, idx = self._native.query(p, dub)
        if scalar:
            return float(dist[0]), int(idx[0])
        return dist, idx

    @property
    def n(self):
        return self.nfaces

    def project(self, pts):
        pts = np.asarray(pts, dtype=np.float64)
        scalar = pts.ndim == 1
        p = np.ascontiguousarray(np.atleast_2d(pts))
        if p.shape[1] != 3:
            raise ValueError(f"project points must be (...,3), got {pts.shape}")
        _finite_or_raise(p, "project pts")
        if len(p) == 0:
            return (np.empty(0), np.empty((0, 3)), np.empty(0, dtype=np.int64),
                    np.empty((0, 3)))
        dist, cp, idx, fn = self._native.project(p)
        if scalar:
            return float(dist[0]), cp[0], int(idx[0]), fn[0]
        return dist, cp, idx, fn

    # ---- inside/outside ---------------------------------------------------
    def inside(self, pts, batch=40000):
        pts = np.asarray(pts, dtype=np.float64)
        scalar = pts.ndim == 1
        p = np.ascontiguousarray(np.atleast_2d(pts))
        if p.shape[1] != 3:
            raise ValueError(f"inside points must be (...,3), got {pts.shape}")
        _finite_or_raise(p, "inside pts")
        if len(p) == 0:
            return np.zeros(0, dtype=bool)
        out = self._native.inside(p)
        return bool(out[0]) if scalar else out

    # ---- ray casting ------------------------------------------------------
    def cast_rays(self, origins, dirs, t_max=1.0):
        origins = np.ascontiguousarray(np.atleast_2d(np.asarray(origins, dtype=np.float64)))
        dirs = np.ascontiguousarray(np.atleast_2d(np.asarray(dirs, dtype=np.float64)))
        if origins.shape != dirs.shape or origins.shape[1] != 3:
            raise ValueError("cast_rays origins/dirs must be matching (n,3)")
        if t_max <= 0:
            raise ValueError("t_max must be > 0")
        _finite_or_raise(origins, "cast_rays origins")
        _finite_or_raise(dirs, "cast_rays dirs")
        n = len(origins)
        if n == 0:
            return []
        offsets, t_all, f_all = self._native.cast_rays(origins, dirs, float(t_max))
        results = []
        for i in range(n):
            s, e = int(offsets[i]), int(offsets[i + 1])
            results.append((t_all[s:e], f_all[s:e].astype(int)))
        return results

    # ---- segment / capsule minimum distance ------------------------------
    def segment_clearance(self, p0s, p1s, r0s, r1s, standoff,
                          inset0s=None, inset1s=None, chunk_mm=5.0):
        p0 = np.ascontiguousarray(np.atleast_2d(np.asarray(p0s, dtype=np.float64)))
        p1 = np.ascontiguousarray(np.atleast_2d(np.asarray(p1s, dtype=np.float64)))
        if p0.shape != p1.shape or p0.shape[1] != 3:
            raise ValueError("segment_clearance p0s/p1s must be matching (N,3)")
        N = len(p0)
        r0 = np.ascontiguousarray(np.broadcast_to(np.asarray(r0s, dtype=np.float64), (N,)))
        r1 = np.ascontiguousarray(np.broadcast_to(np.asarray(r1s, dtype=np.float64), (N,)))
        i0 = (np.zeros(N) if inset0s is None
              else np.ascontiguousarray(np.broadcast_to(np.asarray(inset0s, dtype=np.float64), (N,))))
        i1 = (np.zeros(N) if inset1s is None
              else np.ascontiguousarray(np.broadcast_to(np.asarray(inset1s, dtype=np.float64), (N,))))
        _finite_or_raise(p0, "p0s"); _finite_or_raise(p1, "p1s")
        # Amendment A22: validate non-coordinate numeric args too. The `< 0`
        # checks below let NaN through (NaN < 0 is False), so NaN/inf radii,
        # insets, or standoff must be rejected explicitly BEFORE they cross FFI.
        _finite_or_raise(r0, "r0s"); _finite_or_raise(r1, "r1s")
        _finite_or_raise(i0, "inset0s"); _finite_or_raise(i1, "inset1s")
        if not np.isfinite(standoff):
            raise ValueError("standoff contains non-finite values (NaN/inf)")
        if np.any(r0 < 0) or np.any(r1 < 0):
            raise ValueError("radii must be nonnegative")
        if np.any(i0 < 0) or np.any(i1 < 0):
            raise ValueError("insets must be nonnegative")
        if N == 0:
            return np.empty(0), np.empty((0, 3))
        clr, argp = self._native.segment_clearance(
            p0, p1, r0, r1, float(standoff), i0, i1)
        return clr, argp

    # ---- penetration intervals -------------------------------------------
    def penetration_intervals(self, p0, p1, r0, r1, n_offset_rays=8,
                              majority_step_mm=0.12, cap=80, min_illegal_len_mm=0.05,
                              rays="axis"):
        p0 = np.ascontiguousarray(np.asarray(p0, dtype=np.float64).reshape(1, 3))
        p1 = np.ascontiguousarray(np.asarray(p1, dtype=np.float64).reshape(1, 3))
        r0a = np.array([float(r0)]); r1a = np.array([float(r1)])
        # Amendment A22: reject non-finite endpoints/radii before crossing FFI.
        # On NaN, the Moller/parity comparisons all evaluate false and the
        # kernel would return an EMPTY interval list, silently certifying a
        # member whose geometry is garbage instead of raising.
        _finite_or_raise(p0, "p0"); _finite_or_raise(p1, "p1")
        _finite_or_raise(r0a, "r0"); _finite_or_raise(r1a, "r1")
        if np.any(r0a < 0) or np.any(r1a < 0):
            raise ValueError("radii must be nonnegative")
        d = self._native.penetration_intervals_batch(
            p0, p1, r0a, r1a, int(n_offset_rays), float(majority_step_mm),
            int(cap), float(min_illegal_len_mm), rays == "all")
        return self._intervals_from_soa(d, 0)

    def penetration_intervals_batch(self, p0s, p1s, r0s, r1s, n_offset_rays=8,
                                    majority_step_mm=0.12, cap=80,
                                    min_illegal_len_mm=0.05, rays="axis"):
        """Batch entry point for hot-loop callers (Amendment A7). Returns a list
        of per-member interval-dict lists."""
        p0 = np.ascontiguousarray(np.atleast_2d(np.asarray(p0s, dtype=np.float64)))
        p1 = np.ascontiguousarray(np.atleast_2d(np.asarray(p1s, dtype=np.float64)))
        N = len(p0)
        r0 = np.ascontiguousarray(np.broadcast_to(np.asarray(r0s, dtype=np.float64), (N,)))
        r1 = np.ascontiguousarray(np.broadcast_to(np.asarray(r1s, dtype=np.float64), (N,)))
        # Amendment A22: validate before FFI (see penetration_intervals note).
        _finite_or_raise(p0, "p0s"); _finite_or_raise(p1, "p1s")
        _finite_or_raise(r0, "r0s"); _finite_or_raise(r1, "r1s")
        if np.any(r0 < 0) or np.any(r1 < 0):
            raise ValueError("radii must be nonnegative")
        if N == 0:
            return []
        d = self._native.penetration_intervals_batch(
            p0, p1, r0, r1, int(n_offset_rays), float(majority_step_mm),
            int(cap), float(min_illegal_len_mm), rays == "all")
        return [self._intervals_from_soa(d, i) for i in range(N)]

    @staticmethod
    def _intervals_from_soa(d, member):
        offs = d["offsets"]
        s, e = int(offs[member]), int(offs[member + 1])
        out = []
        for j in range(s, e):
            ray = int(d["ray"][j])
            label = "axis" if ray < 0 else f"off{ray}"
            out.append(dict(
                s_a=float(d["s_a"][j]), s_b=float(d["s_b"][j]),
                entered=bool(d["entered"][j]), exited=bool(d["exited"][j]),
                max_depth_mm=float(d["max_depth_mm"][j]), ray=label))
        return out

    # ---- sphere / ball breach --------------------------------------------
    def sphere_breach(self, centers, radii, eps_mm=0.05, half_deg=25.0, n_cone=8):
        centers = np.ascontiguousarray(np.atleast_2d(np.asarray(centers, dtype=np.float64)))
        if centers.shape[1] != 3:
            raise ValueError("sphere_breach centers must be (N,3)")
        N = len(centers)
        radii = np.ascontiguousarray(np.broadcast_to(np.asarray(radii, dtype=np.float64), (N,)))
        _finite_or_raise(centers, "centers")
        # Amendment A22: `radii < 0` alone lets NaN through (NaN < 0 is False).
        _finite_or_raise(radii, "radii")
        if np.any(radii < 0):
            raise ValueError("radii must be nonnegative")
        if N == 0:
            return np.zeros(0, dtype=bool), np.empty(0), np.zeros(0)
        breached, thick, poke = self._native.sphere_breach(
            centers, radii, float(eps_mm), float(half_deg), int(n_cone))
        return breached, thick, poke

    # ---- exterior-face mask ----------------------------------------------
    def exterior_face_mask(self, gap_warn_mm=0.15):
        if self._exterior_mask is not None:
            return self._exterior_mask
        self._exterior_mask = self._native.exterior_face_mask()
        try:
            self._gap_diag = self._gap_diagnostic(gap_warn_mm)
        except Exception as ex:  # diagnostic must never break the mask
            self._gap_diag = dict(error=str(ex))
        return self._exterior_mask

    def _gap_diagnostic(self, gap_warn_mm, sample=1500):
        # Wrapper-side per Amendment A10: draw the seeded sample + numpy
        # percentiles; the kernel only casts the rays.
        n = self.nfaces
        rng = np.random.RandomState(0)
        sel = np.arange(n) if n <= sample else rng.choice(n, sample, replace=False)
        cents = self.centroids[sel]
        fn = self.face_normals[sel]
        origins = cents + fn * 1e-4
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

    # ---- introspection ----------------------------------------------------
    def current_threads(self):
        return self._native.current_threads()

    def telemetry(self):
        return self._native.telemetry()
