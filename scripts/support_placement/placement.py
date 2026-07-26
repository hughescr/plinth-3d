"""Candidate sampling, load partition, and physics-based tip/shaft sizing.

Implements design.md work-list items #2 (density calibration) and #3
(load partition -> continuous tip/shaft sizing). See config.py for the
mechanics constants and their provenance.
"""
from dataclasses import dataclass, fields
import resource
import sys

import numpy as np
from scipy.spatial import cKDTree

from . import specialized_contacts


CONTACT_POOL_SCHEMA = "support-placement/contact-candidate-pool"
CONTACT_POOL_SCHEMA_VERSION = 1
CONTACT_POOL_BYTES_PER_CANDIDATE = 64
CONTACT_POOL_PEAK_RSS_HIGH_WATER_GROWTH_LIMIT_GB = 2.0
CONTACT_POOL_PLAN_FACE_BATCH = 2_048
CONTACT_POOL_BATCH_VIRTUAL_CELLS = 65_536


def _peak_rss_high_water_gb():
    """Peak RSS in GiB; ru_maxrss is bytes on macOS and KiB elsewhere."""
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform != "darwin":
        rss *= 1024
    return float(rss) / float(1024 ** 3)


def _guard_pool_peak_rss_high_water(base_gb, where):
    """Fail if the process peak-RSS high-water mark rises by 2 GiB.

    This is a last-resort process high-water assertion, not a current-RSS
    measurement. Aggregate cell/nomination/payload caps are the primary
    allocation-prevention mechanism.
    """
    grew = _peak_rss_high_water_gb() - float(base_gb)
    if grew >= CONTACT_POOL_PEAK_RSS_HIGH_WATER_GROWTH_LIMIT_GB:
        raise MemoryError(
            f"contact-pool peak RSS high-water mark grew {grew:.2f} GiB "
            f"past build-entry high-water mark "
            f"(cap {CONTACT_POOL_PEAK_RSS_HIGH_WATER_GROWTH_LIMIT_GB:.2f} GiB) "
            f"at {where}")


def _has_immutable_bytes_backing(arr):
    root = arr
    while isinstance(root.base, np.ndarray):
        root = root.base
    return isinstance(root.base, bytes)


def _immutable_array(value, dtype):
    if (isinstance(value, np.ndarray)
            and value.dtype == np.dtype(dtype)
            and value.flags.c_contiguous
            and not value.flags.writeable
            and _has_immutable_bytes_backing(value)):
        return value
    canonical = np.array(value, dtype=dtype, order="C", copy=True)
    # bytes is an immutable owner: unlike an ndarray that merely has
    # WRITEABLE=False, callers cannot re-enable writes with setflags().
    return np.frombuffer(
        canonical.tobytes(order="C"), dtype=dtype).reshape(canonical.shape)


def _immutable_join(arrays, dtype, row_shape):
    """Join C-contiguous arrays into one final immutable bytes allocation."""
    dtype = np.dtype(dtype)
    arrays = tuple(arrays)
    n_rows = sum(int(len(arr)) for arr in arrays)
    if not n_rows:
        return _immutable_array(np.empty((0,) + tuple(row_shape), dtype=dtype),
                                dtype)
    views = []
    for arr in arrays:
        arr = np.asarray(arr)
        if arr.dtype != dtype or not arr.flags.c_contiguous:
            arr = np.ascontiguousarray(arr, dtype=dtype)
        views.append(memoryview(arr).cast("B"))
    payload = b"".join(views)
    return np.frombuffer(payload, dtype=dtype).reshape(
        (n_rows,) + tuple(row_shape))


@dataclass(frozen=True)
class ContactCandidatePool:
    """Compact, picklable struct-of-arrays for regular-contact provenance.

    One retained candidate occupies exactly 64 bytes of ndarray payload:
    position and raw source-face normal (two float64[3] rows), source face id,
    and island id (two int64 scalars). Position is the deterministic candidate
    identity; the other columns pin ownership and class scope.
    """
    positions: np.ndarray
    normals: np.ndarray
    face_ids: np.ndarray
    island_ids: np.ndarray
    n_virtual_cells_planned: int = 0
    n_candidate_nominations_upper_bound: int = 0
    n_source_faces: int = 0
    max_refine_depth: int = 0
    schema: str = CONTACT_POOL_SCHEMA
    version: int = CONTACT_POOL_SCHEMA_VERSION

    def __post_init__(self):
        # Copy so no caller retains a writable base that can mutate authority
        # behind the pool's read-only view.
        for name, dtype in (
                ("positions", np.float64), ("normals", np.float64),
                ("face_ids", np.int64), ("island_ids", np.int64)):
            arr = _immutable_array(getattr(self, name), dtype)
            object.__setattr__(self, name, arr)
        _validate_pool_structure(self)

    def __getstate__(self):
        return {f.name: getattr(self, f.name) for f in fields(self)}

    def __setstate__(self, state):
        # Pickle does not make ndarray writeability an authority guarantee.
        # Restore every field explicitly, deep-copy arrays, and re-freeze before
        # returning the object to caller code.
        for f in fields(self):
            object.__setattr__(self, f.name, state[f.name])
        for name, dtype in (
                ("positions", np.float64), ("normals", np.float64),
                ("face_ids", np.int64), ("island_ids", np.int64)):
            arr = _immutable_array(getattr(self, name), dtype)
            object.__setattr__(self, name, arr)
        _validate_pool_structure(self)

    def __len__(self):
        return len(self.face_ids)

    @property
    def nbytes(self):
        return int(self.positions.nbytes + self.normals.nbytes
                   + self.face_ids.nbytes + self.island_ids.nbytes)

    def island_slice(self, island_id):
        """Return the contiguous slice for ``island_id`` (pool is island-sorted)."""
        lo = int(np.searchsorted(self.island_ids, int(island_id), side="left"))
        hi = int(np.searchsorted(self.island_ids, int(island_id), side="right"))
        return slice(lo, hi)

    def iter_rows(self):
        """Compatibility iterator; materializes at most one row dict at a time."""
        for i in range(len(self)):
            yield dict(
                position=self.positions[i].copy(),
                normal=self.normals[i].copy(),
                island_id=int(self.island_ids[i]),
                face_id=int(self.face_ids[i]),
                candidate_id=("position",) + tuple(float(x)
                                                   for x in self.positions[i]),
            )

    __iter__ = iter_rows


def _validate_pool_structure(pool):
    if (pool.schema != CONTACT_POOL_SCHEMA
            or pool.version != CONTACT_POOL_SCHEMA_VERSION):
        raise ValueError(
            "prepared contact pool schema/version is unsupported; rerun prepare")
    specs = (
        ("positions", np.dtype(np.float64), 2),
        ("normals", np.dtype(np.float64), 2),
        ("face_ids", np.dtype(np.int64), 1),
        ("island_ids", np.dtype(np.int64), 1),
    )
    for name, dtype, ndim in specs:
        arr = getattr(pool, name)
        if not isinstance(arr, np.ndarray):
            raise ValueError(f"contact pool {name} is not an ndarray")
        if arr.dtype != dtype or arr.ndim != ndim or not arr.flags.c_contiguous:
            raise ValueError(
                f"contact pool {name} has invalid dtype/shape/contiguity")
        if arr.flags.writeable:
            raise ValueError(f"contact pool {name} must be read-only")
        if not _has_immutable_bytes_backing(arr):
            raise ValueError(
                f"contact pool {name} lacks immutable bytes backing")
    n = len(pool.face_ids)
    if (pool.positions.shape != (n, 3) or pool.normals.shape != (n, 3)
            or pool.island_ids.shape != (n,)):
        raise ValueError("invalid contact candidate pool array shapes")
    if not np.all(np.isfinite(pool.positions)) or not np.all(
            np.isfinite(pool.normals)):
        raise ValueError("contact pool positions/normals must be finite")
    counter_names = (
        "n_virtual_cells_planned",
        "n_candidate_nominations_upper_bound",
        "n_source_faces",
        "max_refine_depth",
    )
    for name in counter_names:
        value = getattr(pool, name)
        if (isinstance(value, (bool, np.bool_))
                or not isinstance(value, (int, np.integer))
                or int(value) < 0):
            raise ValueError(f"contact pool planning counter {name} is invalid")
    if int(pool.n_candidate_nominations_upper_bound) != (
            3 * int(pool.n_virtual_cells_planned)):
        raise ValueError("contact pool nomination counter is inconsistent")
    if int(pool.n_source_faces) > int(pool.n_virtual_cells_planned):
        raise ValueError("contact pool source-face/cell counters are inconsistent")
    if n > int(pool.n_candidate_nominations_upper_bound):
        raise ValueError("contact pool retained count exceeds planned nominations")
    n_source = int(pool.n_source_faces)
    n_cells = int(pool.n_virtual_cells_planned)
    max_depth = int(pool.max_refine_depth)
    if n_source == 0:
        if n_cells != 0 or max_depth != 0 or n != 0:
            raise ValueError("empty contact pool planning counters are inconsistent")
    else:
        min_cells = (4 ** max_depth) + n_source - 1
        max_cells = n_source * (4 ** max_depth)
        if not (min_cells <= n_cells <= max_cells):
            raise ValueError(
                "contact pool depth/source/cell counters are inconsistent")
    if n:
        order = np.lexsort((
            pool.face_ids, pool.positions[:, 1], pool.positions[:, 0],
            pool.positions[:, 2], pool.island_ids))
        if not np.array_equal(order, np.arange(n, dtype=np.int64)):
            raise ValueError(
                "contact pool rows violate island/z/x/y/face total ordering")
        duplicate_position = (
            (pool.island_ids[1:] == pool.island_ids[:-1])
            & np.all(pool.positions[1:] == pool.positions[:-1], axis=1))
        if np.any(duplicate_position):
            raise ValueError("contact pool contains duplicate island/position rows")
    return pool


def validate_candidate_pool(pool, mesh=None, islands=None):
    """Deep-check immutable A2 pool authority at every staged boundary."""
    if not isinstance(pool, ContactCandidatePool):
        raise ValueError(
            "prepared contact pool is missing or uses an old schema; rerun prepare")
    _validate_pool_structure(pool)
    if mesh is not None:
        n_faces = len(mesh.faces)
        if len(pool) and (np.any(pool.face_ids < 0)
                          or np.any(pool.face_ids >= n_faces)):
            raise ValueError("contact pool source face id is outside mesh")
        expected = np.asarray(mesh.face_normals, dtype=np.float64)[pool.face_ids]
        if not np.array_equal(pool.normals, expected):
            raise ValueError("contact pool raw source-face normals mismatch mesh")
    if islands is not None and len(pool):
        if (np.any(pool.island_ids < 0)
                or np.any(pool.island_ids >= len(islands))):
            raise ValueError("contact pool island id is outside prepared islands")
        for iid in np.unique(pool.island_ids):
            sl = pool.island_slice(int(iid))
            legal_faces = np.asarray(islands[int(iid)], dtype=np.int64)
            if not np.all(np.isin(pool.face_ids[sl], legal_faces)):
                raise ValueError(
                    "contact pool source face is outside its prepared island")
    return pool


def _analysis_cell_edge_mm(cfg):
    """Largest virtual-cell edge compatible with the coverage contract.

    Greedy placement leaves every unselected pool point within
    ``target_spacing_mm`` of a selected one. Bounding each virtual cell's edge
    by the remaining coverage-radius margin therefore keeps every source-face
    point reachable without inventing centroid/random seats.
    """
    target = float(cfg.target_spacing_mm)
    radius = target * float(cfg.coverage_radius_factor)
    margin = radius - target
    if target <= 0.0 or margin <= 0.0:
        raise ValueError(
            "regular-contact virtual refinement requires positive target "
            "spacing and coverage_radius_factor > 1")
    return margin


def _refine_depths(max_edges, cell_edge_limit):
    """Minimum dyadic depths, stable at a one-ULP nominal threshold."""
    max_edges = np.asarray(max_edges, dtype=np.float64)
    ratio = np.zeros_like(max_edges)
    positive = max_edges > 0.0
    ratio[positive] = max_edges[positive] / float(cell_edge_limit)
    depths = np.zeros(len(max_edges), dtype=np.int64)
    above = ratio > 1.0
    depths[above] = np.ceil(np.log2(ratio[above])).astype(np.int64)

    # ``ceil(log2())`` can spuriously add one level when an analytically exact
    # dyadic boundary arrives one representable float high. Accept that single
    # ULP, including at deeper powers, then verify the resulting bound.
    has_parent = depths > 0
    parent_bound = float(cell_edge_limit) * np.exp2(
        np.maximum(depths - 1, 0).astype(np.float64))
    lower = has_parent & (
        max_edges <= np.nextafter(parent_bound, np.inf))
    depths[lower] -= 1
    bound = float(cell_edge_limit) * np.exp2(depths.astype(np.float64))
    too_small = max_edges > np.nextafter(bound, np.inf)
    depths[too_small] += 1
    return depths


def _source_face_max_edges(vertices, faces, face_ids):
    """Materialize at most one bounded planning batch of source triangles."""
    tris = vertices[faces[face_ids]]
    return np.linalg.norm(
        np.roll(tris, -1, axis=1) - tris, axis=2).max(axis=1)


def _bounded_batch_cell_count(depths, remaining_cell_budget):
    """Count ``sum(4**depth)`` without fixed-width overflow.

    Planning only needs an exact value while the batch can still fit. If any
    power, or their object-integer batch sum, exceeds the remaining aggregate
    cell budget, return the monotonic saturation value ``budget + 1`` so the
    caller refuses immediately. The batch is bounded to 2,048 faces and no
    Python per-face scalar loop is used.
    """
    depths = np.asarray(depths, dtype=np.int64)
    remaining_cell_budget = int(remaining_cell_budget)
    if not len(depths):
        return 0
    if remaining_cell_budget < 0:
        return remaining_cell_budget + 1
    if np.any(depths < 0):
        raise ValueError("contact-pool refinement depth must be nonnegative")
    if remaining_cell_budget == 0:
        return 1
    max_admissible_depth = (
        int(remaining_cell_budget).bit_length() - 1) // 2
    if np.any(depths > max_admissible_depth):
        return remaining_cell_budget + 1
    powers = np.power(
        np.asarray(4, dtype=object), depths.astype(object))
    total = int(np.sum(powers, dtype=object))
    return min(total, remaining_cell_budget + 1)


def candidate_pool(mesh, islands, cfg, island_id_offset=0, stats_out=None,
                   existing_pool=None):
    """Return the deterministic legal seat pool for regular contacts.

    Each source triangle is deterministically refined into virtual analysis
    cells whose maximum edge is bounded by the unused margin in the existing
    spacing/coverage contract. Refinement is uniform recursive 1-to-4 midpoint
    subdivision, so the candidate geometry is invariant under prior dyadic
    midpoint refinement of the source mesh. Every virtual cell nominates every
    co-lowest vertex. Co-lowest uses an 8-ULP-scale absolute tolerance so a flat
    cell does not depend on arbitrary vertex order, while geometrically distinct
    heights do not collapse. Nominations at the exact same position are
    deduplicated within the island. The owning SOURCE face is the nominator with
    the most-negative face-normal ``nz``; equal ``nz`` breaks by source face id.
    The candidate normal is always that owning FACE normal -- never an averaged
    vertex normal.

    This is a strict two-pass build. Pass 1 computes every source face's depth
    and the GLOBAL ``sum(4**depth)`` / ``3*cells`` upper bounds before any
    virtual cell is allocated. Pass 2 generates and deduplicates nominations
    into a compact :class:`ContactCandidatePool`.
    """
    vertices = np.asarray(mesh.vertices, float)
    faces = np.asarray(mesh.faces, dtype=np.int64)
    max_cell_edge = _analysis_cell_edge_mm(cfg)
    max_depth_allowed = int(cfg.contact_pool_max_refine_depth)
    max_virtual_cells = int(cfg.contact_pool_max_virtual_cells)
    max_candidates = int(cfg.contact_pool_max_candidates)
    max_pool_bytes = int(cfg.contact_pool_max_compact_payload_bytes)
    peak_rss_high_water_base_gb = _peak_rss_high_water_gb()
    if max_depth_allowed < 0:
        raise ValueError("contact_pool_max_refine_depth must be >= 0")
    if max_virtual_cells < 0 or max_candidates < 0 or max_pool_bytes < 0:
        raise ValueError("contact-pool aggregate budgets must be >= 0")
    if (isinstance(island_id_offset, (bool, np.bool_))
            or not isinstance(island_id_offset, (int, np.integer))
            or int(island_id_offset) < 0):
        raise ValueError("island_id_offset must be a nonnegative integer")
    island_id_offset = int(island_id_offset)
    if existing_pool is not None:
        existing_pool = validate_candidate_pool(existing_pool, mesh=mesh)
        base_virtual_cells = int(existing_pool.n_virtual_cells_planned)
        base_nomination_upper = int(
            existing_pool.n_candidate_nominations_upper_bound)
        base_source_faces = int(existing_pool.n_source_faces)
        base_max_refine_depth = int(existing_pool.max_refine_depth)
    else:
        base_virtual_cells = 0
        base_nomination_upper = 0
        base_source_faces = 0
        base_max_refine_depth = 0
    if (existing_pool is not None and len(existing_pool)
            and island_id_offset <= int(existing_pool.island_ids[-1])):
        raise ValueError(
            "island_id_offset must be greater than every existing pool "
            "island id before extension generation")

    # PASS 1: global plan and refusal. No virtual ``cells`` array exists until
    # every admitted face has contributed to these aggregate bounds. Each
    # island first faces a one-cell-per-face lower-bound check before face-id
    # conversion, sorting, or triangle materialization. Admitted plans are
    # then computed in fixed NumPy batches and refused monotonically.
    plans = []
    max_depth_seen = 0
    n_virtual_cells = 0
    n_source_faces = 0

    def _preflight_stats(new_cells, new_source_faces, preflight_passed=False):
        combined_cells = base_virtual_cells + int(new_cells)
        nominations = base_nomination_upper + 3 * int(new_cells)
        payload_bytes = nominations * CONTACT_POOL_BYTES_PER_CANDIDATE
        return dict(
            max_refine_depth=max(
                base_max_refine_depth, int(max_depth_seen)),
            max_refine_depth_allowed=int(max_depth_allowed),
            cell_edge_limit_mm=float(max_cell_edge),
            n_source_faces=base_source_faces + int(new_source_faces),
            n_virtual_cells_planned=int(combined_cells),
            n_virtual_cells=int(combined_cells),
            max_virtual_cells=int(max_virtual_cells),
            n_candidate_nominations_upper_bound=int(nominations),
            max_candidates=int(max_candidates),
            compact_payload_bytes_upper_bound=int(payload_bytes),
            max_compact_payload_bytes=int(max_pool_bytes),
            bytes_per_candidate=int(CONTACT_POOL_BYTES_PER_CANDIDATE),
            preflight_passed=bool(preflight_passed),
        )

    def _refusal_reasons(stats):
        refusals = []
        if stats["n_virtual_cells_planned"] > max_virtual_cells:
            refusals.append(
                "contact_pool_max_virtual_cells: "
                f"{stats['n_virtual_cells_planned']} > {max_virtual_cells}")
        if stats["n_candidate_nominations_upper_bound"] > max_candidates:
            refusals.append(
                "contact_pool_max_candidates: "
                f"{stats['n_candidate_nominations_upper_bound']} > "
                f"{max_candidates}")
        if stats["compact_payload_bytes_upper_bound"] > max_pool_bytes:
            refusals.append(
                "contact_pool_max_compact_payload_bytes: "
                f"{stats['compact_payload_bytes_upper_bound']} > "
                f"{max_pool_bytes}")
        return refusals

    def _check_preflight(new_cells, new_source_faces):
        stats = _preflight_stats(new_cells, new_source_faces)
        refusals = _refusal_reasons(stats)
        if not refusals:
            return
        stats["refusal_reasons"] = list(refusals)
        if stats_out is not None:
            stats_out.update(stats)
        raise ValueError(
            "contact-pool global preflight refused: " + "; ".join(refusals))

    # Existing authority may have been created under looser caps. Reject before
    # considering a new island if it cannot fit the current aggregate budget.
    _check_preflight(0, 0)
    max_new_cells_by_caps = min(
        max_virtual_cells - base_virtual_cells,
        (max_candidates - base_nomination_upper) // 3,
        ((max_pool_bytes // CONTACT_POOL_BYTES_PER_CANDIDATE)
         - base_nomination_upper) // 3,
    )
    for local_island_id, face_ids in enumerate(islands):
        _guard_pool_peak_rss_high_water(
            peak_rss_high_water_base_gb, "planning loop")
        island_id = island_id_offset + int(local_island_id)
        n_island_faces = len(face_ids)
        if not n_island_faces:
            continue
        # Every source face contributes at least one virtual cell and three
        # nomination slots. This refuses oversized depth-0 islands before
        # converting/sorting their face ids or gathering any triangles.
        _check_preflight(
            n_virtual_cells + n_island_faces,
            n_source_faces + n_island_faces)
        fid_batches = []
        depth_batches = []
        for start in range(
                0, n_island_faces, CONTACT_POOL_PLAN_FACE_BATCH):
            _guard_pool_peak_rss_high_water(
                peak_rss_high_water_base_gb, "planning face-batch loop")
            stop = min(start + CONTACT_POOL_PLAN_FACE_BATCH, n_island_faces)
            batch_fids = np.asarray(
                face_ids[start:stop], dtype=np.int64)
            if batch_fids.ndim != 1 or len(batch_fids) != stop - start:
                raise ValueError("island face ids must be a one-dimensional sequence")
            if (np.any(batch_fids < 0)
                    or np.any(batch_fids >= len(faces))):
                raise ValueError("island source face id is outside mesh")
            edge_max = _source_face_max_edges(vertices, faces, batch_fids)
            batch_depths = _refine_depths(edge_max, max_cell_edge)
            if np.any(batch_depths > max_depth_allowed):
                j = int(np.nonzero(
                    batch_depths > max_depth_allowed)[0][0])
                raise ValueError(
                    f"contact-pool refinement depth "
                    f"{int(batch_depths[j])} exceeds configured "
                    f"contact_pool_max_refine_depth={max_depth_allowed} "
                    f"for source face_id={int(batch_fids[j])} "
                    f"(max_edge_mm={float(edge_max[j]):.9g}, "
                    f"cell_edge_limit_mm={max_cell_edge:.9g})")
            max_depth_seen = max(
                max_depth_seen, int(batch_depths.max(initial=0)))
            batch_cells = _bounded_batch_cell_count(
                batch_depths,
                max_new_cells_by_caps - n_virtual_cells)
            n_virtual_cells += batch_cells
            n_source_faces += len(batch_fids)
            remaining_island_faces = n_island_faces - stop
            _check_preflight(
                n_virtual_cells + remaining_island_faces,
                n_source_faces + remaining_island_faces)
            fid_batches.append(batch_fids)
            depth_batches.append(batch_depths)
        fids = np.concatenate(fid_batches)
        depths = np.concatenate(depth_batches)
        face_order = np.argsort(fids, kind="stable")
        fids = np.ascontiguousarray(fids[face_order])
        depths = np.ascontiguousarray(depths[face_order])
        plans.append((island_id, fids, depths))

    new_nomination_upper = 3 * int(n_virtual_cells)
    combined_virtual_cells = base_virtual_cells + int(n_virtual_cells)
    nomination_upper = base_nomination_upper + new_nomination_upper
    planned_stats = _preflight_stats(
        n_virtual_cells, n_source_faces, preflight_passed=True)
    planned_stats["refusal_reasons"] = []
    if stats_out is not None:
        stats_out.update(planned_stats)

    # PASS 2: generate depth-homogeneous FACE BATCHES, then sort/deduplicate
    # whole-island nomination arrays. No per-nomination tuple/dict loop exists.
    normals = np.asarray(mesh.face_normals, float)
    position_chunks = []
    normal_chunks = []
    face_id_chunks = []
    island_id_chunks = []
    retained_total = 0
    for island_id, fids, depths in plans:
        _guard_pool_peak_rss_high_water(
            peak_rss_high_water_base_gb, "island generation loop")
        nomination_positions = []
        nomination_faces = []
        for depth in np.unique(depths):
            depth = int(depth)
            depth_fids = fids[depths == depth]
            cells_per_face = 4 ** depth
            batch_faces = max(
                1, CONTACT_POOL_BATCH_VIRTUAL_CELLS // cells_per_face)
            for start in range(0, len(depth_fids), batch_faces):
                _guard_pool_peak_rss_high_water(
                    peak_rss_high_water_base_gb,
                    "face-batch generation loop")
                batch_fids = depth_fids[start:start + batch_faces]
                cells = vertices[faces[batch_fids]][:, None, :, :].copy()
                for _ in range(depth):
                    _guard_pool_peak_rss_high_water(
                        peak_rss_high_water_base_gb, "refinement loop")
                    a, b, c = cells[:, :, 0], cells[:, :, 1], cells[:, :, 2]
                    ab = (a + b) * 0.5
                    bc = (b + c) * 0.5
                    ca = (c + a) * 0.5
                    cells = np.stack((
                        np.stack((a, ab, ca), axis=2),
                        np.stack((ab, b, bc), axis=2),
                        np.stack((ca, bc, c), axis=2),
                        np.stack((ab, bc, ca), axis=2),
                    ), axis=2).reshape(
                        (len(batch_fids), -1, 3, 3))
                z = cells[:, :, :, 2]
                z_min = z.min(axis=2)
                scale = np.maximum(1.0, np.max(np.abs(z), axis=2))
                z_tol = 8.0 * np.finfo(float).eps * scale
                nominate = np.abs(z - z_min[:, :, None]) <= z_tol[:, :, None]
                nomination_positions.append(
                    np.ascontiguousarray(cells[nominate]))
                face_grid = np.broadcast_to(
                    batch_fids[:, None, None], nominate.shape)
                nomination_faces.append(
                    np.ascontiguousarray(face_grid[nominate], dtype=np.int64))
        if not nomination_positions:
            continue
        nominated_positions = np.concatenate(nomination_positions, axis=0)
        nominated_faces = np.concatenate(nomination_faces)
        _guard_pool_peak_rss_high_water(
            peak_rss_high_water_base_gb, "nomination gather")
        owner_nz = normals[nominated_faces, 2]
        owner_order = np.lexsort((
            nominated_faces, owner_nz, nominated_positions[:, 2],
            nominated_positions[:, 1], nominated_positions[:, 0]))
        nominated_positions = nominated_positions[owner_order]
        nominated_faces = nominated_faces[owner_order]
        unique_position = np.ones(len(nominated_positions), dtype=bool)
        unique_position[1:] = np.any(
            nominated_positions[1:] != nominated_positions[:-1], axis=1)
        positions = nominated_positions[unique_position]
        face_ids = nominated_faces[unique_position]
        order = np.lexsort((
            face_ids, positions[:, 1], positions[:, 0], positions[:, 2]))
        positions = np.ascontiguousarray(positions[order])
        face_ids = np.ascontiguousarray(face_ids[order])
        position_chunks.append(positions)
        normal_chunks.append(np.ascontiguousarray(normals[face_ids]))
        face_id_chunks.append(face_ids)
        island_id_chunks.append(
            np.full(len(face_ids), int(island_id), dtype=np.int64))
        retained_total += len(face_ids)

    combined_candidates = retained_total + (
        len(existing_pool) if existing_pool is not None else 0)
    combined_nbytes = (
        combined_candidates * CONTACT_POOL_BYTES_PER_CANDIDATE)
    if combined_candidates > max_candidates or combined_nbytes > max_pool_bytes:
        raise ValueError(
            "contact-pool exact retained size exceeds configured budget: "
            f"candidates={combined_candidates}/{max_candidates}, "
            f"bytes={combined_nbytes}/{max_pool_bytes}")
    # One final immutable bytes allocation per SOA column. For extensions this
    # joins the prepared prefix and generated chunks directly, avoiding the old
    # new-pool canonical copy followed by concat followed by another copy.
    position_inputs = (
        ((existing_pool.positions,) if existing_pool is not None else ())
        + tuple(position_chunks))
    normal_inputs = (
        ((existing_pool.normals,) if existing_pool is not None else ())
        + tuple(normal_chunks))
    face_id_inputs = (
        ((existing_pool.face_ids,) if existing_pool is not None else ())
        + tuple(face_id_chunks))
    island_id_inputs = (
        ((existing_pool.island_ids,) if existing_pool is not None else ())
        + tuple(island_id_chunks))
    pool = ContactCandidatePool(
        positions=_immutable_join(
            position_inputs, np.float64, (3,)),
        normals=_immutable_join(
            normal_inputs, np.float64, (3,)),
        face_ids=_immutable_join(
            face_id_inputs, np.int64, ()),
        island_ids=_immutable_join(
            island_id_inputs, np.int64, ()),
        n_virtual_cells_planned=int(combined_virtual_cells),
        n_candidate_nominations_upper_bound=int(nomination_upper),
        n_source_faces=int(planned_stats["n_source_faces"]),
        max_refine_depth=max(
            int(base_max_refine_depth), int(max_depth_seen)),
    )
    if stats_out is not None:
        stats_out.update(
            n_candidates=int(len(pool)),
            compact_payload_bytes=int(pool.nbytes),
        )
    _guard_pool_peak_rss_high_water(
        peak_rss_high_water_base_gb, "pool completion")
    return pool


def sample_candidates(mesh, islands, cfg, seed=None, candidate_pool_prepared=None):
    """Greedy regular-contact placement over :func:`candidate_pool`.

    Candidates are considered lowest-z first and kept no closer than
    ``cfg.target_spacing_mm``. Small islands fall back to deterministic
    farthest-point selection, but every selected seat remains a legal pool
    member. There is no random sampling and this function never reads or
    mutates global RNG state.

    Specialized minimum/island-birth contacts are added by the engine's
    dedicated demand passes and intentionally are not governed by this regular
    contact pool.

    ``seed`` is a deprecated, ignored compatibility argument. It is retained so
    older callers do not break, but this deterministic implementation neither
    reads nor mutates RNG state.
    """
    del seed
    pool = (candidate_pool_prepared if candidate_pool_prepared is not None
            else candidate_pool(mesh, islands, cfg))
    validate_candidate_pool(pool, mesh=mesh, islands=islands)

    contacts = []
    for island_id, face_ids in enumerate(islands):
        island_slice = pool.island_slice(island_id)
        pts = pool.positions[island_slice]
        if not len(pts):
            continue
        chosen_idx = []
        chosen_pts = np.empty((0, 3))
        for i in range(len(pts)):
            p = pts[i]
            if len(chosen_pts):
                d = np.linalg.norm(chosen_pts - p, axis=1).min()
                if d < cfg.target_spacing_mm:
                    continue
            chosen_idx.append(i)
            chosen_pts = np.vstack([chosen_pts, p])

        if len(chosen_idx) < cfg.min_contacts_per_island:
            # Force coverage on small/thin islands via the same legal pool.
            chosen_idx = _farthest_point_fallback(
                pts, cfg.min_contacts_per_island, cfg.spacing_floor_mm)

        area = float(np.asarray(mesh.area_faces, float)[face_ids].sum())
        for i in chosen_idx:
            row = island_slice.start + int(i)
            contacts.append(dict(
                position=pool.positions[row].copy(),
                normal=pool.normals[row].copy(),
                island_id=island_id, face_id=int(pool.face_ids[row]),
                candidate_pool_row=int(row), area_mm2=area))
    return contacts


def _farthest_point_fallback(pts, k, floor_mm):
    k = min(k, len(pts))
    if k <= 0:
        return []
    chosen = [0]
    d = np.linalg.norm(pts - pts[0], axis=1)
    while len(chosen) < k:
        nxt = int(np.argmax(d))
        chosen.append(nxt)
        d = np.minimum(d, np.linalg.norm(pts - pts[nxt], axis=1))
    return chosen


def load_partition(mesh, islands, contacts, cfg):
    """Exact, deterministic per-face load partition (SPEC-V5-GEOM sec 2.5).

    Replaces the seed-dependent Monte-Carlo partition.  Every unique source
    face claimed by the named owner islands is assigned once to its nearest
    eligible physical contact, and its FULL ``mesh.area_faces`` value accrues
    there.  Shared specialized contacts participate for every logical owner;
    overlapping named islands therefore contribute the union of their faces,
    not duplicate load.  No RNG or sampling is involved.  Returns
    ``carried_area_mm2`` aligned with physical ``contacts``."""
    carried = np.zeros(len(contacts))
    owners_by_contact = [
        specialized_contacts.owner_island_ids(contact)
        for contact in contacts]
    contacts_by_face = {}
    for contact_idx, owner_ids in enumerate(owners_by_contact):
        for island_id in owner_ids:
            for face_id in np.asarray(
                    islands[int(island_id)], dtype=np.int64):
                contacts_by_face.setdefault(int(face_id), set()).add(
                    int(contact_idx))

    centers = np.asarray(mesh.triangles_center, dtype=np.float64)
    face_area = np.asarray(mesh.area_faces, dtype=np.float64)
    # Named islands may overlap.  A shared physical contact carries the union
    # of their source faces, never the sum of duplicate named claims.
    for face_id in sorted(contacts_by_face):
        eligible = sorted(contacts_by_face[face_id])
        if not eligible:
            continue
        point = centers[face_id]
        distances_sq = [
            float(np.dot(
                np.asarray(contacts[index]["position"], dtype=np.float64)
                - point,
                np.asarray(contacts[index]["position"], dtype=np.float64)
                - point))
            for index in eligible
        ]
        winner = eligible[int(np.argmin(distances_sq))]
        carried[winner] += float(face_area[face_id])
    return carried


def tip_radius_mm(carried_area_mm2, cfg):
    """Green-state tensile fuse sizing (design.md mechanics framework):
    r = sqrt(F * SF / (pi * tensile_strength)), clamped to the
    removal-friendly [tip_r_min_mm, tip_r_max_mm] band."""
    area_m2 = np.asarray(carried_area_mm2) * 1e-6
    force_n = area_m2 * cfg.peel_pressure_pa
    tensile_pa = cfg.tensile_green_mpa * 1e6
    r_m = np.sqrt(np.maximum(force_n, 0) * cfg.tip_safety_factor / (np.pi * tensile_pa))
    r_mm = r_m * 1000.0
    return np.clip(r_mm, cfg.tip_r_min_mm, cfg.tip_r_max_mm)


def _peel_tan(angle_from_vertical_deg):
    return float(np.tan(np.radians(angle_from_vertical_deg))) \
        if angle_from_vertical_deg > 1e-9 else 0.0


def _shaft_r_for_L(L_mm, cfg, angle_from_vertical_deg=0.0):
    """Unclamped cantilever point-load deflection solve:
    delta = 4*F*L^3/(3*E*pi*r^4)  =>  r = (4*F*L^3/(3*E*pi*delta))^0.25
    (design.md: "deflection ~ L^3/r^4"). All SI internally.

    v3.6 angle-aware load term (Craig, queue item 2): an inclined member
    presents an elliptical layer cross-section of area pi*r^2/cos(theta)
    to the peel; resolving that peel normal load onto the member's
    lateral (bending) axis multiplies by sin(theta):
        F_eff = F_drag + peel_pressure * (pi*r^2/cos t) * sin t
              = F_drag + peel_pressure * pi * r^2 * tan(theta).
    F_eff depends on r, so the solve is a fixed-point iteration -- it
    contracts fast (r ~ F^(1/4), F ~ r^2 => net exponent 1/2 < 1).
    Vertical members (theta=0) reduce exactly to the old closed form."""
    L_m = max(L_mm, 1e-6) / 1000.0
    E = cfg.modulus_green_gpa * 1e9
    delta = cfg.max_deflection_mm / 1000.0
    tan_t = _peel_tan(angle_from_vertical_deg)
    geom = 4 * L_m ** 3 / (3 * E * np.pi * delta)
    r_m = (geom * cfg.shaft_drag_force_n) ** 0.25
    for _ in range(6 if tan_t else 0):
        F = cfg.shaft_drag_force_n + cfg.peel_pressure_pa * np.pi * r_m ** 2 * tan_t
        r_m = (geom * F) ** 0.25
    return max(r_m * 1000.0, cfg.min_shaft_r_mm)


def max_unbraced_len_mm(r_mm, cfg, angle_from_vertical_deg=0.0):
    """Inverse of _shaft_r_for_L: the longest free cantilever a member of
    radius r may run while meeting the deflection budget. Used to decide
    where a long member needs bracing rungs (v3.4: brace-then-thin).
    With radius KNOWN the angle-aware load is closed-form (no iteration):
    F_eff = F_drag + peel * pi * r^2 * tan(theta)."""
    r_m = max(r_mm, 1e-6) / 1000.0
    E = cfg.modulus_green_gpa * 1e9
    delta = cfg.max_deflection_mm / 1000.0
    F = cfg.shaft_drag_force_n \
        + cfg.peel_pressure_pa * np.pi * r_m ** 2 * _peel_tan(angle_from_vertical_deg)
    L_m = (r_m ** 4 * 3 * E * np.pi * delta / (4 * F)) ** (1 / 3)
    return L_m * 1000.0


def shaft_radius_for_length_mm(L_mm, cfg):
    """Slenderness-capped shaft sizing (design.md #5 trigger): if the
    stiffness-required radius for the FULL free length would be
    "ugly-thick" (> cfg.max_unbraced_shaft_r_mm), add braces to shorten
    the effective free length instead of thickening further -- "halving
    free length cuts deflection 8x" (L^3 term). Returns
    (radius_mm, n_braces, effective_free_length_mm)."""
    r = _shaft_r_for_L(L_mm, cfg)
    if r <= cfg.max_unbraced_shaft_r_mm or L_mm <= 0:
        return round(r, 3), 0, L_mm
    # Find the segment count n+1 (n braces) such that L/(n+1) brings the
    # required radius back under the unbraced cap.
    n_braces = 0
    seg = L_mm
    while True:
        n_braces += 1
        seg = L_mm / (n_braces + 1)
        r = _shaft_r_for_L(seg, cfg)
        if r <= cfg.max_unbraced_shaft_r_mm or n_braces > 50:
            break
    return round(r, 3), n_braces, seg
