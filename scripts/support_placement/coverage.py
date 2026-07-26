"""Prepared, grouped, digest-backed surface samples for A3 coverage."""
from dataclasses import dataclass, fields
import hashlib
import resource
import sys

import numpy as np

from . import placement, topology


COVERAGE_SAMPLES_SCHEMA = "support-placement/coverage-samples"
COVERAGE_SAMPLES_SCHEMA_VERSION = 3
COVERAGE_SAMPLE_BYTES = 49
COVERAGE_PEAK_RSS_HIGH_WATER_GROWTH_LIMIT_GB = 2.0
COVERAGE_PLAN_FACE_BATCH = 2_048


def _peak_rss_high_water_bytes():
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(rss if sys.platform == "darwin" else rss * 1024)


def _peak_rss_high_water_gb():
    return float(_peak_rss_high_water_bytes()) / float(1024 ** 3)


def _guard_peak_rss(base_gb, where):
    grew = _peak_rss_high_water_gb() - float(base_gb)
    if grew >= COVERAGE_PEAK_RSS_HIGH_WATER_GROWTH_LIMIT_GB:
        raise MemoryError(
            f"coverage-sample peak RSS high-water mark grew {grew:.2f} GiB "
            f"past build-entry high-water mark "
            f"(cap {COVERAGE_PEAK_RSS_HIGH_WATER_GROWTH_LIMIT_GB:.2f} GiB) "
            f"at {where}")


def _has_immutable_bytes_backing(arr):
    root = arr
    while isinstance(root.base, np.ndarray):
        root = root.base
    return isinstance(root.base, bytes)


def _immutable_array(value, dtype):
    dtype = np.dtype(dtype)
    if (isinstance(value, np.ndarray) and value.dtype == dtype
            and value.flags.c_contiguous and not value.flags.writeable
            and _has_immutable_bytes_backing(value)):
        return value
    arr = np.ascontiguousarray(value, dtype=dtype)
    return np.frombuffer(
        arr.tobytes(order="C"), dtype=dtype).reshape(arr.shape)


def _immutable_join(arrays, dtype, row_shape=()):
    arrays = tuple(arrays)
    if not arrays:
        return _immutable_array(
            np.empty((0,) + tuple(row_shape), dtype=dtype), dtype)
    joined = np.concatenate([
        np.asarray(a, dtype=dtype).reshape((-1,) + tuple(row_shape))
        for a in arrays], axis=0)
    payload = joined.tobytes(order="C")
    return np.frombuffer(payload, dtype=dtype).reshape(joined.shape)


def islands_fingerprint(islands):
    h = hashlib.sha256()
    for island_id, face_ids in enumerate(islands):
        arr = np.ascontiguousarray(face_ids, dtype="<i8")
        h.update(int(island_id).to_bytes(8, "little"))
        h.update(len(arr).to_bytes(8, "little"))
        h.update(arr.tobytes())
    return h.hexdigest()


def _coverage_digest(samples):
    h = hashlib.sha256()
    h.update(samples.mesh_fingerprint.encode("ascii"))
    h.update(samples.islands_fingerprint.encode("ascii"))
    for name in (
            "positions", "area_weights_mm2", "face_ids", "island_ids",
            "refine_depths", "island_offsets", "source_face_offsets",
            "source_face_ids", "source_face_island_ids"):
        arr = np.ascontiguousarray(getattr(samples, name))
        h.update(name.encode("ascii"))
        h.update(arr.dtype.str.encode("ascii"))
        h.update(np.asarray(arr.shape, dtype="<i8").tobytes())
        h.update(arr.tobytes(order="C"))
    for value in (
            samples.n_source_faces, samples.n_virtual_cells_planned,
            samples.max_refine_depth, samples.n_islands,
            samples.n_mesh_faces, samples.n_mesh_vertices,
            samples.max_working_cells, samples.compact_retained_bytes,
            samples.planned_peak_build_bytes,
            samples.planned_peak_validation_bytes):
        h.update(int(value).to_bytes(8, "little", signed=False))
    return h.hexdigest()


def _coverage_retained_bytes(n_cells, n_source_faces, n_islands):
    """Exact compact payload bytes, excluding Python object allocator overhead."""
    n = int(n_cells)
    s = int(n_source_faces)
    i = int(n_islands)
    # Nine retained ndarrays: 49 bytes/sample, three 8-byte source-face rows,
    # and the two offset terminators.
    arrays = n * COVERAGE_SAMPLE_BYTES + 24 * s + 8 * i + 16
    # schema bytes, version, ten uint64 counters/plans, and three retained
    # ASCII SHA-256 strings.
    metadata = (
        len(COVERAGE_SAMPLES_SCHEMA.encode("ascii"))
        + 8 + 10 * 8 + 3 * 64)
    return int(arrays + metadata)


def _planned_peak_build_bytes(
        n_cells, n_source_faces, working_cells, n_islands=0,
        n_mesh_faces=0, n_mesh_vertices=0):
    """Conservative maximum of the planning, generation, and final phases."""
    n = int(n_cells)
    s = int(n_source_faces)
    w = int(working_cells)
    i = int(n_islands)
    f = int(n_mesh_faces)
    v = int(n_mesh_vertices)
    retained = _coverage_retained_bytes(n, s, i)

    # Planning retains sorted face/depth plans (9S), while a 2,048-face batch
    # may hold indexed triangles, edge vectors/norms, depths, and sort copies.
    planning = 35 * s + 256 * min(s, COVERAGE_PLAN_FACE_BATCH)
    # Generation retains completed 49-byte chunks and plans. A midpoint-refine
    # step can co-live old/new cell tensors, four inner stack results, midpoint
    # arrays, centroids, repeats, and the current output chunk: 192 bytes/cell.
    generation = 49 * n + 9 * s + 192 * w
    # Completion co-lives retained chunks, joined/sorted originals, the 8N
    # lexsort order, immutable bytes owners, plan/group/offset originals, and
    # the largest one-at-a-time fingerprint/digest bytes copy.
    final = (
        retained + 49 * n + 49 * n + 8 * n
        + 9 * s + 8 * s + 8 * (s + 1)
        + 8 * (i + 1) + 8 * i
        + max((24 * n, 24 * f, 24 * v, 8 * s), default=0))
    return int(max(planning, generation, final))


def _planned_peak_validation_bytes(
        n_cells, n_source_faces, n_islands,
        n_mesh_faces, n_mesh_vertices):
    """Conservative deep-validation maximum after builder scratch is gone."""
    n = int(n_cells)
    s = int(n_source_faces)
    i = int(n_islands)
    f = int(n_mesh_faces)
    v = int(n_mesh_vertices)
    retained = _coverage_retained_bytes(n, s, i)
    phases = (
        # Canonical-order lexsort output/scratch plus comparison arange.
        retained + 24 * n,
        # Group counts/starts/metadata, one repeat, left-shift, and bincount.
        retained + 8 * n + 49 * s + 8 * i,
        # Digest or mesh/island fingerprint one-at-a-time bytes scratch.
        retained + max((24 * n, 24 * f, 24 * v, 8 * s), default=0),
        # area_faces, selected weights, depth cast/exp/divide temporaries.
        retained + 40 * n + 8 * f,
        # mesh.triangles, selected triangles, lo/hi, and comparison masks.
        retained + 123 * n + 72 * f,
        # Island-membership search/sort scratch.
        retained + 16 * n + 16 * s,
    )
    return int(max(phases))


# Compatibility for v2 tests/callers; new code records both phase plans.
def _planned_peak_bytes(n_cells, n_source_faces, working_cells):
    return _planned_peak_build_bytes(
        n_cells, n_source_faces, working_cells)


@dataclass(frozen=True)
class CoverageSamples:
    schema: str
    version: int
    positions: np.ndarray
    area_weights_mm2: np.ndarray
    face_ids: np.ndarray
    island_ids: np.ndarray
    refine_depths: np.ndarray
    island_offsets: np.ndarray
    source_face_offsets: np.ndarray
    source_face_ids: np.ndarray
    source_face_island_ids: np.ndarray
    n_source_faces: int
    n_virtual_cells_planned: int
    max_refine_depth: int
    n_islands: int
    n_mesh_faces: int
    n_mesh_vertices: int
    max_working_cells: int
    compact_retained_bytes: int
    planned_peak_build_bytes: int
    planned_peak_validation_bytes: int
    mesh_fingerprint: str
    islands_fingerprint: str
    content_digest: str

    def __post_init__(self):
        for name, dtype in (
                ("positions", np.float64),
                ("area_weights_mm2", np.float64),
                ("face_ids", np.int64), ("island_ids", np.int64),
                ("refine_depths", np.uint8),
                ("island_offsets", np.int64),
                ("source_face_offsets", np.int64),
                ("source_face_ids", np.int64),
                ("source_face_island_ids", np.int64)):
            object.__setattr__(
                self, name, _immutable_array(getattr(self, name), dtype))

    def __getstate__(self):
        return {f.name: getattr(self, f.name) for f in fields(self)}

    def __setstate__(self, state):
        for f in fields(self):
            object.__setattr__(self, f.name, state[f.name])
        for name, dtype in (
                ("positions", np.float64),
                ("area_weights_mm2", np.float64),
                ("face_ids", np.int64), ("island_ids", np.int64),
                ("refine_depths", np.uint8),
                ("island_offsets", np.int64),
                ("source_face_offsets", np.int64),
                ("source_face_ids", np.int64),
                ("source_face_island_ids", np.int64)):
            object.__setattr__(
                self, name, _immutable_array(getattr(self, name), dtype))
        validate_coverage_samples(self)

    def __len__(self):
        return len(self.face_ids)

    @property
    def nbytes(self):
        return self.compact_retained_bytes

    @property
    def array_nbytes(self):
        return sum(getattr(self, f.name).nbytes for f in fields(self)
                   if isinstance(getattr(self, f.name), np.ndarray))

    def island_slice(self, island_id):
        island_id = int(island_id)
        return slice(
            int(self.island_offsets[island_id]),
            int(self.island_offsets[island_id + 1]))


def validate_coverage_samples(samples, mesh=None, islands=None):
    """Digest-backed deep structural validation at prepared boundaries."""
    if (not isinstance(samples, CoverageSamples)
            or samples.schema != COVERAGE_SAMPLES_SCHEMA
            or samples.version != COVERAGE_SAMPLES_SCHEMA_VERSION):
        raise ValueError(
            "prepared coverage samples schema/version is unsupported; "
            "rerun prepare")
    specs = (
        ("positions", np.dtype(np.float64), 2),
        ("area_weights_mm2", np.dtype(np.float64), 1),
        ("face_ids", np.dtype(np.int64), 1),
        ("island_ids", np.dtype(np.int64), 1),
        ("refine_depths", np.dtype(np.uint8), 1),
        ("island_offsets", np.dtype(np.int64), 1),
        ("source_face_offsets", np.dtype(np.int64), 1),
        ("source_face_ids", np.dtype(np.int64), 1),
        ("source_face_island_ids", np.dtype(np.int64), 1),
    )
    for name, dtype, ndim in specs:
        arr = getattr(samples, name)
        if (not isinstance(arr, np.ndarray) or arr.dtype != dtype
                or arr.ndim != ndim or not arr.flags.c_contiguous
                or arr.flags.writeable or not _has_immutable_bytes_backing(arr)):
            raise ValueError(
                f"prepared coverage samples {name} is not canonical immutable "
                f"{dtype}")
    n = len(samples)
    if samples.positions.shape != (n, 3) or any(
            len(getattr(samples, name)) != n
            for name in ("area_weights_mm2", "island_ids", "refine_depths")):
        raise ValueError("prepared coverage sample array lengths disagree")
    if (samples.n_virtual_cells_planned != n
            or samples.n_source_faces != len(samples.source_face_ids)
            or len(samples.source_face_island_ids) != samples.n_source_faces):
        raise ValueError("prepared coverage sample counters disagree")
    expected_retained = _coverage_retained_bytes(
        n, samples.n_source_faces, samples.n_islands)
    expected_build_peak = _planned_peak_build_bytes(
        n, samples.n_source_faces, min(n, samples.max_working_cells),
        samples.n_islands, samples.n_mesh_faces, samples.n_mesh_vertices)
    expected_validation_peak = _planned_peak_validation_bytes(
        n, samples.n_source_faces, samples.n_islands,
        samples.n_mesh_faces, samples.n_mesh_vertices)
    if (samples.compact_retained_bytes != expected_retained
            or samples.planned_peak_build_bytes != expected_build_peak
            or samples.planned_peak_validation_bytes
            != expected_validation_peak):
        raise ValueError("prepared coverage resource plans disagree")
    if samples.max_working_cells <= 0:
        raise ValueError("prepared coverage working-cell budget is invalid")
    if (len(samples.island_offsets) != samples.n_islands + 1
            or samples.island_offsets[0] != 0
            or samples.island_offsets[-1] != n
            or np.any(samples.island_offsets[1:] < samples.island_offsets[:-1])):
        raise ValueError("prepared coverage island offsets are invalid")
    if (len(samples.source_face_offsets) != samples.n_source_faces + 1
            or samples.source_face_offsets[0] != 0
            or samples.source_face_offsets[-1] != n
            or np.any(samples.source_face_offsets[1:]
                      <= samples.source_face_offsets[:-1])):
        raise ValueError("prepared coverage source-face offsets are invalid")
    if n and (not np.all(np.isfinite(samples.positions))
              or not np.all(np.isfinite(samples.area_weights_mm2))
              or np.any(samples.area_weights_mm2 <= 0.0)):
        raise ValueError("prepared coverage samples contain invalid values")
    if n and int(samples.refine_depths.max()) != samples.max_refine_depth:
        raise ValueError("prepared coverage max refinement depth disagrees")
    # Canonical island/face/position order and exact source-group completeness.
    order = np.lexsort((
        samples.positions[:, 2], samples.positions[:, 1],
        samples.positions[:, 0], samples.face_ids, samples.island_ids))
    if not np.array_equal(order, np.arange(n, dtype=np.int64)):
        raise ValueError("prepared coverage samples are not canonical sorted")
    del order
    counts = np.diff(samples.source_face_offsets)
    starts = samples.source_face_offsets[:-1]
    group_face = samples.face_ids[starts]
    group_island = samples.island_ids[starts]
    group_depth = samples.refine_depths[starts]
    if (not np.array_equal(group_face, samples.source_face_ids)
            or not np.array_equal(group_island, samples.source_face_island_ids)
            or np.any(samples.face_ids != np.repeat(group_face, counts))
            or np.any(samples.island_ids != np.repeat(group_island, counts))
            or np.any(samples.refine_depths != np.repeat(group_depth, counts))
            or np.any(counts != np.left_shift(
                np.ones(len(counts), dtype=np.int64),
                2 * group_depth.astype(np.int64)))):
        raise ValueError("prepared coverage source-face groups are incomplete")
    expected_island_counts = np.bincount(
        samples.island_ids, minlength=samples.n_islands)
    if not np.array_equal(
            np.diff(samples.island_offsets), expected_island_counts):
        raise ValueError("prepared coverage island offsets disagree")
    del (
        counts, starts, group_face, group_island, group_depth,
        expected_island_counts)
    if samples.content_digest != _coverage_digest(samples):
        raise ValueError("prepared coverage samples content digest mismatch")
    if mesh is not None:
        if (len(mesh.faces) != samples.n_mesh_faces
                or len(mesh.vertices) != samples.n_mesh_vertices):
            raise ValueError("prepared coverage mesh dimensions mismatch")
        if topology.mesh_fingerprint(mesh) != samples.mesh_fingerprint:
            raise ValueError("prepared coverage mesh fingerprint mismatch")
        if n and (np.any(samples.face_ids < 0)
                  or np.any(samples.face_ids >= len(mesh.faces))):
            raise ValueError("prepared coverage sample face is outside mesh")
        expected_weight = (
            np.asarray(mesh.area_faces, dtype=np.float64)[samples.face_ids]
            / np.exp2(2.0 * samples.refine_depths.astype(np.float64)))
        if not np.array_equal(samples.area_weights_mm2, expected_weight):
            raise ValueError(
                "prepared coverage sample exact area weight mismatches mesh")
        del expected_weight
        triangles = np.asarray(mesh.triangles, dtype=np.float64)[samples.face_ids]
        lo = triangles.min(axis=1)
        hi = triangles.max(axis=1)
        if np.any(samples.positions < lo) or np.any(samples.positions > hi):
            raise ValueError("prepared coverage sample lies outside source face")
        del triangles, lo, hi
    if islands is not None:
        if islands_fingerprint(islands) != samples.islands_fingerprint:
            raise ValueError("prepared coverage islands fingerprint mismatch")
        if len(islands) != samples.n_islands:
            raise ValueError("prepared coverage island count mismatch")
        for iid, face_ids in enumerate(islands):
            sl = samples.island_slice(iid)
            if not np.all(np.isin(
                    samples.face_ids[sl],
                    np.asarray(face_ids, dtype=np.int64))):
                raise ValueError(
                    "prepared coverage sample face is outside its island")
    return samples


def _build_coverage_samples_inner(mesh, islands, cfg, stats_out=None):
    """Two-pass dyadic sampling with refusal before virtual allocation."""
    vertices = np.asarray(mesh.vertices, dtype=np.float64)
    faces = np.asarray(mesh.faces, dtype=np.int64)
    pitch = float(cfg.voxel_pitch_mm)
    max_cells = int(cfg.coverage_samples_max_virtual_cells)
    max_bytes = int(cfg.coverage_samples_max_compact_payload_bytes)
    max_peak_bytes = int(getattr(
        cfg, "coverage_samples_max_peak_bytes",
        512 * 1024 * 1024))
    working_cap = int(cfg.coverage_samples_max_working_cells)
    if pitch <= 0.0:
        raise ValueError("voxel_pitch_mm must be positive for coverage samples")
    if min(max_cells, max_bytes, max_peak_bytes) < 0 or working_cap <= 0:
        raise ValueError("coverage-sample resource budgets are invalid")
    peak_base = _peak_rss_high_water_gb()
    plans = []
    n_cells = 0
    n_faces_planned = 0
    max_depth = 0

    def _stats(passed):
        return dict(
            n_source_faces=int(n_faces_planned),
            n_virtual_cells_planned=int(n_cells),
            max_virtual_cells=int(max_cells),
            compact_retained_bytes=int(_coverage_retained_bytes(
                n_cells, n_faces_planned, len(islands))),
            max_compact_payload_bytes=int(max_bytes),
            planned_peak_build_bytes=int(_planned_peak_build_bytes(
                n_cells, n_faces_planned, min(n_cells, working_cap),
                len(islands), len(faces), len(vertices))),
            planned_peak_validation_bytes=int(
                _planned_peak_validation_bytes(
                    n_cells, n_faces_planned, len(islands),
                    len(faces), len(vertices))),
            max_peak_bytes=int(max_peak_bytes),
            bytes_per_sample=int(COVERAGE_SAMPLE_BYTES),
            max_refine_depth=int(max_depth),
            max_working_cells=int(working_cap),
            preflight_passed=bool(passed))

    def _check():
        stats = _stats(False)
        reasons = []
        if n_cells > max_cells:
            reasons.append(
                f"coverage_samples_max_virtual_cells: {n_cells} > {max_cells}")
        if stats["compact_retained_bytes"] > max_bytes:
            reasons.append(
                "coverage_samples_max_compact_payload_bytes: "
                f"{stats['compact_retained_bytes']} > {max_bytes}")
        if stats["planned_peak_build_bytes"] > max_peak_bytes:
            reasons.append(
                "coverage_samples_max_peak_bytes(build): "
                f"{stats['planned_peak_build_bytes']} > {max_peak_bytes}")
        if stats["planned_peak_validation_bytes"] > max_peak_bytes:
            reasons.append(
                "coverage_samples_max_peak_bytes(validation): "
                f"{stats['planned_peak_validation_bytes']} > {max_peak_bytes}")
        if reasons:
            stats["refusal_reasons"] = reasons
            if stats_out is not None:
                stats_out.update(stats)
            raise ValueError(
                "coverage-sample global preflight refused: "
                + "; ".join(reasons))

    effective_cell_cap = min(
        max_cells, max_bytes // COVERAGE_SAMPLE_BYTES)
    for island_id, source_face_ids in enumerate(islands):
        _guard_peak_rss(peak_base, "planning loop")
        count = len(source_face_ids)
        if not count:
            continue
        n_cells += count
        n_faces_planned += count
        _check()
        n_cells -= count
        n_faces_planned -= count
        fid_batches, depth_batches = [], []
        for start in range(0, count, COVERAGE_PLAN_FACE_BATCH):
            stop = min(start + COVERAGE_PLAN_FACE_BATCH, count)
            fids = np.asarray(source_face_ids[start:stop], dtype=np.int64)
            if fids.ndim != 1 or len(fids) != stop - start:
                raise ValueError("coverage island face ids must be one-dimensional")
            if len(fids) and (np.any(fids < 0) or np.any(fids >= len(faces))):
                raise ValueError("coverage island face id is outside mesh")
            edges = placement._source_face_max_edges(vertices, faces, fids)
            depths = placement._refine_depths(edges, pitch)
            max_depth = max(max_depth, int(depths.max(initial=0)))
            max_working_depth = (working_cap.bit_length() - 1) // 2
            if np.any(depths > max_working_depth):
                j = int(np.nonzero(depths > max_working_depth)[0][0])
                raise ValueError(
                    "coverage-sample global preflight refused: "
                    "coverage_samples_max_working_cells: "
                    f"face_id={int(fids[j])} needs "
                    f"{4 ** int(depths[j])} > {working_cap}")
            batch_cells = placement._bounded_batch_cell_count(
                depths, effective_cell_cap - n_cells)
            n_cells += int(batch_cells)
            n_faces_planned += len(fids)
            remaining = count - stop
            n_cells += remaining
            n_faces_planned += remaining
            try:
                _check()
            finally:
                n_cells -= remaining
                n_faces_planned -= remaining
            fid_batches.append(fids)
            depth_batches.append(depths)
        fids = np.concatenate(fid_batches)
        depths = np.concatenate(depth_batches)
        order = np.argsort(fids, kind="stable")
        plans.append((
            int(island_id), np.ascontiguousarray(fids[order]),
            np.ascontiguousarray(depths[order])))

    stats = _stats(True)
    stats["refusal_reasons"] = []
    if stats_out is not None:
        stats_out.update(stats)

    position_chunks, weight_chunks = [], []
    face_chunks, island_chunks, depth_chunks = [], [], []
    area_faces = np.asarray(mesh.area_faces, dtype=np.float64)
    max_working_seen = 0
    for island_id, fids, depths in plans:
        for depth in np.unique(depths):
            depth = int(depth)
            depth_fids = fids[depths == depth]
            cells_per_face = 4 ** depth
            batch_faces = max(1, working_cap // cells_per_face)
            for start in range(0, len(depth_fids), batch_faces):
                _guard_peak_rss(peak_base, "generation loop")
                batch_fids = depth_fids[start:start + batch_faces]
                cells = vertices[faces[batch_fids]][:, None, :, :].copy()
                for _ in range(depth):
                    a, b, c = cells[:, :, 0], cells[:, :, 1], cells[:, :, 2]
                    ab = (a + b) * 0.5
                    bc = (b + c) * 0.5
                    ca = (c + a) * 0.5
                    cells = np.stack((
                        np.stack((a, ab, ca), axis=2),
                        np.stack((ab, b, bc), axis=2),
                        np.stack((ca, bc, c), axis=2),
                        np.stack((ab, bc, ca), axis=2),
                    ), axis=2).reshape((len(batch_fids), -1, 3, 3))
                n_working = int(cells.shape[0] * cells.shape[1])
                max_working_seen = max(max_working_seen, n_working)
                centroids = cells.mean(axis=2).reshape((-1, 3))
                sample_faces = np.repeat(batch_fids, cells_per_face)
                weights = (
                    area_faces[sample_faces] / np.exp2(2.0 * depth))
                position_chunks.append(np.ascontiguousarray(centroids))
                weight_chunks.append(np.ascontiguousarray(weights))
                face_chunks.append(np.ascontiguousarray(sample_faces))
                island_chunks.append(np.full(
                    len(sample_faces), island_id, dtype=np.int64))
                depth_chunks.append(np.full(
                    len(sample_faces), depth, dtype=np.uint8))

    positions = np.concatenate(position_chunks) if position_chunks else np.empty((0, 3))
    weights = np.concatenate(weight_chunks) if weight_chunks else np.empty(0)
    face_ids = np.concatenate(face_chunks) if face_chunks else np.empty(0, np.int64)
    island_ids = np.concatenate(island_chunks) if island_chunks else np.empty(0, np.int64)
    refine_depths = np.concatenate(depth_chunks) if depth_chunks else np.empty(0, np.uint8)
    order = np.lexsort((
        positions[:, 2], positions[:, 1], positions[:, 0],
        face_ids, island_ids))
    positions, weights = positions[order], weights[order]
    face_ids, island_ids = face_ids[order], island_ids[order]
    refine_depths = refine_depths[order]
    island_counts = np.bincount(island_ids, minlength=len(islands))
    island_offsets = np.r_[0, np.cumsum(island_counts, dtype=np.int64)]
    group_start = np.r_[0, np.nonzero(
        (island_ids[1:] != island_ids[:-1])
        | (face_ids[1:] != face_ids[:-1]))[0] + 1] if len(face_ids) else np.empty(0, np.int64)
    source_face_offsets = np.r_[group_start, len(face_ids)]
    source_face_ids = face_ids[group_start]
    source_face_island_ids = island_ids[group_start]
    compact_retained = _coverage_retained_bytes(
        n_cells, n_faces_planned, len(islands))
    planned_build_peak = _planned_peak_build_bytes(
        n_cells, n_faces_planned, min(n_cells, working_cap),
        len(islands), len(faces), len(vertices))
    planned_validation_peak = _planned_peak_validation_bytes(
        n_cells, n_faces_planned, len(islands), len(faces), len(vertices))
    samples = CoverageSamples(
        schema=COVERAGE_SAMPLES_SCHEMA,
        version=COVERAGE_SAMPLES_SCHEMA_VERSION,
        positions=_immutable_array(positions, np.float64),
        area_weights_mm2=_immutable_array(weights, np.float64),
        face_ids=_immutable_array(face_ids, np.int64),
        island_ids=_immutable_array(island_ids, np.int64),
        refine_depths=_immutable_array(refine_depths, np.uint8),
        island_offsets=_immutable_array(island_offsets, np.int64),
        source_face_offsets=_immutable_array(source_face_offsets, np.int64),
        source_face_ids=_immutable_array(source_face_ids, np.int64),
        source_face_island_ids=_immutable_array(
            source_face_island_ids, np.int64),
        n_source_faces=int(n_faces_planned),
        n_virtual_cells_planned=int(n_cells),
        max_refine_depth=int(max_depth), n_islands=len(islands),
        n_mesh_faces=len(faces), n_mesh_vertices=len(vertices),
        max_working_cells=int(working_cap),
        compact_retained_bytes=int(compact_retained),
        planned_peak_build_bytes=int(planned_build_peak),
        planned_peak_validation_bytes=int(planned_validation_peak),
        mesh_fingerprint=topology.mesh_fingerprint(mesh),
        islands_fingerprint=islands_fingerprint(islands),
        content_digest="")
    object.__setattr__(samples, "content_digest", _coverage_digest(samples))
    if len(samples) != n_cells or samples.compact_retained_bytes > max_bytes:
        raise ValueError(
            "coverage-sample exact retained size exceeds configured budget")
    if stats_out is not None:
        stats_out.update(
            n_samples=int(len(samples)),
            compact_retained_bytes=int(samples.compact_retained_bytes),
            max_working_cells_seen=int(max_working_seen))
    _guard_peak_rss(peak_base, "completion")
    return samples


def build_coverage_samples(mesh, islands, cfg, stats_out=None):
    """Build, release all construction scratch, then deep-validate samples."""
    rss_entry = _peak_rss_high_water_bytes()
    prepared = _build_coverage_samples_inner(
        mesh, islands, cfg, stats_out=stats_out)
    rss_after_build = _peak_rss_high_water_bytes()
    validated = validate_coverage_samples(
        prepared, mesh=mesh, islands=islands)
    rss_after_validation = _peak_rss_high_water_bytes()
    if stats_out is not None:
        stats_out.update(
            measured_build_rss_high_water_delta_bytes=max(
                0, rss_after_build - rss_entry),
            measured_total_rss_high_water_delta_bytes=max(
                0, rss_after_validation - rss_entry))
    return validated
