"""Vectorized exact-bit face topology for ``process=False`` source meshes."""
from dataclasses import dataclass, fields
import hashlib
import json
import resource
import sys

import numpy as np
from scipy.sparse import csr_matrix
from scipy.sparse.csgraph import connected_components


FACE_TOPOLOGY_SCHEMA = "support-placement/face-topology"
FACE_TOPOLOGY_SCHEMA_VERSION = 3

TOPOLOGY_CUT_BOUNDARY = np.uint8(1)
TOPOLOGY_CUT_NONMANIFOLD = np.uint8(2)
TOPOLOGY_CUT_SAME_DIRECTION = np.uint8(4)
TOPOLOGY_CUT_DUPLICATE = np.uint8(8)
TOPOLOGY_CUT_POINT_ONLY = np.uint8(16)
TOPOLOGY_CUT_DEGENERATE = np.uint8(32)
TOPOLOGY_KNOWN_CUT_MASK = np.uint8(63)

DEFAULT_TOPOLOGY_MAX_FACES = 750_000
DEFAULT_TOPOLOGY_MAX_COMPACT_PAYLOAD_BYTES = 64 * 1024 * 1024
DEFAULT_TOPOLOGY_MAX_PEAK_BYTES = 512 * 1024 * 1024


def _peak_rss_high_water_bytes():
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(rss if sys.platform == "darwin" else rss * 1024)


def mesh_fingerprint(mesh):
    """The prepared-state mesh identity shared with the engine."""
    h = hashlib.sha256()
    for label, value, dtype in (
            ("vertices", mesh.vertices, np.dtype("<f8")),
            ("faces", mesh.faces, np.dtype("<i8"))):
        arr = np.ascontiguousarray(np.asarray(value, dtype=dtype))
        h.update(label.encode("ascii"))
        h.update(json.dumps(
            list(arr.shape), separators=(",", ":")).encode("ascii"))
        h.update(arr.tobytes(order="C"))
    return h.hexdigest()


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
    canonical = np.array(value, dtype=dtype, order="C", copy=True)
    return np.frombuffer(
        canonical.tobytes(order="C"), dtype=dtype).reshape(canonical.shape)


def _topology_digest(topology):
    h = hashlib.sha256()
    h.update(topology.mesh_fingerprint.encode("ascii"))
    for name in ("indptr", "indices", "component_ids", "cut_flags"):
        arr = np.ascontiguousarray(getattr(topology, name))
        h.update(name.encode("ascii"))
        h.update(arr.dtype.str.encode("ascii"))
        h.update(np.asarray(arr.shape, dtype="<i8").tobytes())
        h.update(arr.tobytes(order="C"))
    for value in (
            topology.n_faces, topology.n_vertices,
            topology.n_directed_edges, topology.compact_retained_bytes,
            topology.planned_peak_build_bytes,
            topology.planned_peak_validation_bytes):
        h.update(int(value).to_bytes(8, "little", signed=False))
    h.update(np.asarray(topology.cut_counts, dtype="<i8").tobytes())
    return h.hexdigest()


def _topology_retained_bytes(n_faces, n_directed_edges):
    """Exact compact payload bytes, excluding Python object allocator overhead."""
    n_faces = int(n_faces)
    n_edges = int(n_directed_edges)
    arrays = 8 * (n_faces + 1) + 8 * n_edges + 8 * n_faces + n_faces
    # schema bytes, version, six uint64 counters/plans, 64 cut counters, and
    # the two retained ASCII SHA-256 strings.
    metadata = (
        len(FACE_TOPOLOGY_SCHEMA.encode("ascii"))
        + 8 + 6 * 8 + 64 * 8 + 2 * 64)
    return int(arrays + metadata)


def _planned_peak_build_bytes(n_faces, n_vertices):
    """Conservative phase maximum before any ``3F`` allocation."""
    f = int(n_faces)
    v = int(n_vertices)
    retained_upper = _topology_retained_bytes(f, 3 * f)

    # Each coefficient is bytes per source face/vertex simultaneously live in
    # that phase. They include the source arrays and NumPy/SciPy output/sort
    # scratch, rather than counting only the final CSR:
    #   vertex: source + unique input/sorted/order/inverse/output + cf
    #   duplicate: source/cf + tri key/order/sorted/masks
    #   edge: original and sorted endpoint/face/direction arrays plus worst-case
    #         (3F groups) start/stop/count/id/flag/reduction temporaries
    #   CSR: pair/row/col/order/sorted/count arrays and sparse traversal scratch
    #   fan: 3F incidence arrays/order/nodes plus the 4-edge local-fan sparse
    #        graph, labels, second sort, grouped reductions, and traversal work
    #   finalize: mutable CSR + immutable bytes backing + largest tobytes
    #             fingerprint/digest scratch.
    phases = (
        48 * f + 88 * v,
        84 * f + 32 * v,
        527 * f + 32 * v,
        300 * f + 32 * v,
        900 * f + 32 * v,
        2 * retained_upper + max(24 * f, 24 * v),
    )
    return int(max(phases, default=0))


def _planned_peak_validation_bytes(
        n_faces, n_vertices, n_directed_edges):
    """Conservative deep-validation phase maximum after build scratch dies."""
    f = int(n_faces)
    v = int(n_vertices)
    e = int(n_directed_edges)
    retained = _topology_retained_bytes(f, e)
    # CSR proof: repeat/arange rows, pairs, two lexsort orders, original and
    # sorted undirected pairs, group starts/counts, and lexsort scratch.
    csr_proof = retained + 8 * f + 97 * e
    # Sparse component proof: retained CSR plus sparse data/traversal and labels.
    component_proof = retained + 16 * f + 10 * e
    # Digest or optional mesh fingerprint: largest one-at-a-time bytes copy.
    fingerprint = retained + max(8 * (f + 1), 8 * e, 24 * f, 24 * v)
    return int(max(csr_proof, component_proof, fingerprint))


# Compatibility for callers created with the v2 field name.
def _planned_working_bytes(n_faces, n_vertices):
    return _planned_peak_build_bytes(n_faces, n_vertices)


@dataclass(frozen=True)
class FaceTopology:
    schema: str
    version: int
    indptr: np.ndarray
    indices: np.ndarray
    component_ids: np.ndarray
    cut_flags: np.ndarray
    n_faces: int
    n_vertices: int
    n_directed_edges: int
    compact_retained_bytes: int
    planned_peak_build_bytes: int
    planned_peak_validation_bytes: int
    cut_counts: tuple
    mesh_fingerprint: str
    content_digest: str

    def __post_init__(self):
        for name, dtype in (
                ("indptr", np.int64), ("indices", np.int64),
                ("component_ids", np.int64), ("cut_flags", np.uint8)):
            object.__setattr__(
                self, name, _immutable_array(getattr(self, name), dtype))

    def __getstate__(self):
        return {f.name: getattr(self, f.name) for f in fields(self)}

    def __setstate__(self, state):
        for f in fields(self):
            object.__setattr__(self, f.name, state[f.name])
        for name, dtype in (
                ("indptr", np.int64), ("indices", np.int64),
                ("component_ids", np.int64), ("cut_flags", np.uint8)):
            object.__setattr__(
                self, name, _immutable_array(getattr(self, name), dtype))
        validate_face_topology(self)

    @property
    def nbytes(self):
        return self.compact_retained_bytes

    @property
    def array_nbytes(self):
        return sum(getattr(self, f.name).nbytes for f in fields(self)
                   if isinstance(getattr(self, f.name), np.ndarray))

    def neighbors(self, face_id):
        face_id = int(face_id)
        return self.indices[self.indptr[face_id]:self.indptr[face_id + 1]]


def _canonical_component_labels(indptr, indices, n_faces):
    graph = csr_matrix(
        (np.ones(len(indices), dtype=np.uint8), indices, indptr),
        shape=(n_faces, n_faces))
    _n_components, labels = connected_components(graph, directed=False)
    return np.asarray(labels, dtype=np.int64)


def validate_face_topology(topology, mesh=None):
    """Digest-backed structural proof without rebuilding source edge arrays."""
    if (not isinstance(topology, FaceTopology)
            or topology.schema != FACE_TOPOLOGY_SCHEMA
            or topology.version != FACE_TOPOLOGY_SCHEMA_VERSION):
        raise ValueError(
            "prepared face topology schema/version is unsupported; rerun prepare")
    specs = (
        ("indptr", np.dtype(np.int64), 1),
        ("indices", np.dtype(np.int64), 1),
        ("component_ids", np.dtype(np.int64), 1),
        ("cut_flags", np.dtype(np.uint8), 1),
    )
    for name, dtype, ndim in specs:
        arr = getattr(topology, name)
        if (not isinstance(arr, np.ndarray) or arr.dtype != dtype
                or arr.ndim != ndim or not arr.flags.c_contiguous
                or arr.flags.writeable or not _has_immutable_bytes_backing(arr)):
            raise ValueError(
                f"prepared face topology {name} is not canonical immutable {dtype}")
    n = int(topology.n_faces)
    if n < 0 or topology.n_vertices < 0:
        raise ValueError("prepared face topology counters are negative")
    if (len(topology.indptr) != n + 1
            or len(topology.component_ids) != n
            or len(topology.cut_flags) != n):
        raise ValueError("prepared face topology array lengths disagree")
    expected_retained = _topology_retained_bytes(
        n, topology.n_directed_edges)
    expected_build_peak = _planned_peak_build_bytes(
        n, int(topology.n_vertices))
    expected_validation_peak = _planned_peak_validation_bytes(
        n, int(topology.n_vertices), topology.n_directed_edges)
    if (topology.n_directed_edges != len(topology.indices)
            or topology.compact_retained_bytes != expected_retained
            or topology.planned_peak_build_bytes != expected_build_peak
            or topology.planned_peak_validation_bytes
            != expected_validation_peak):
        raise ValueError("prepared face topology counters disagree")
    if (topology.indptr[0] != 0
            or topology.indptr[-1] != len(topology.indices)
            or np.any(topology.indptr[1:] < topology.indptr[:-1])):
        raise ValueError("prepared face topology CSR indptr is invalid")
    if len(topology.indices) and (
            np.any(topology.indices < 0) or np.any(topology.indices >= n)):
        raise ValueError("prepared face topology CSR index is outside face range")
    rows = np.repeat(
        np.arange(n, dtype=np.int64), np.diff(topology.indptr))
    if len(rows):
        if np.any(rows == topology.indices):
            raise ValueError("prepared face topology contains a self edge")
        pairs = np.column_stack((rows, topology.indices))
        order = np.lexsort((pairs[:, 1], pairs[:, 0]))
        if not np.array_equal(order, np.arange(len(order))):
            raise ValueError("prepared face topology CSR is not canonical sorted")
        if len(pairs) > 1 and np.any(np.all(
                pairs[1:] == pairs[:-1], axis=1)):
            raise ValueError("prepared face topology contains duplicate CSR edges")
        undirected = np.sort(pairs, axis=1)
        uorder = np.lexsort((undirected[:, 1], undirected[:, 0]))
        undirected = undirected[uorder]
        starts = np.r_[True, np.any(
            undirected[1:] != undirected[:-1], axis=1)]
        counts = np.diff(np.r_[np.nonzero(starts)[0], len(undirected)])
        if np.any(counts != 2):
            raise ValueError("prepared face topology CSR is not symmetric")
        del pairs, order, undirected, uorder, starts, counts
    del rows
    recomputed = _canonical_component_labels(
        topology.indptr, topology.indices, n)
    if not np.array_equal(recomputed, topology.component_ids):
        raise ValueError("prepared face topology component labels are invalid")
    if np.any(topology.cut_flags & np.uint8(~int(TOPOLOGY_KNOWN_CUT_MASK) & 0xff)):
        raise ValueError("prepared face topology contains unknown cut flags")
    cut_counts = tuple(
        int(x) for x in np.bincount(topology.cut_flags, minlength=64))
    if tuple(topology.cut_counts) != cut_counts:
        raise ValueError("prepared face topology cut counters disagree")
    if (not isinstance(topology.mesh_fingerprint, str)
            or len(topology.mesh_fingerprint) != 64):
        raise ValueError("prepared face topology mesh fingerprint is invalid")
    if topology.content_digest != _topology_digest(topology):
        raise ValueError("prepared face topology content digest mismatch")
    if mesh is not None:
        if len(mesh.faces) != n or len(mesh.vertices) != topology.n_vertices:
            raise ValueError("prepared face topology dimensions mismatch mesh")
        if mesh_fingerprint(mesh) != topology.mesh_fingerprint:
            raise ValueError("prepared face topology mesh fingerprint mismatch")
    return topology


def _build_face_topology_inner(mesh, cfg=None, stats_out=None):
    """Build exact shared-edge adjacency with fully array-oriented hot paths."""
    faces = np.asarray(mesh.faces, dtype=np.int64)
    vertices = np.asarray(mesh.vertices, dtype=np.float64)
    if faces.ndim != 2 or faces.shape[1:] != (3,):
        raise ValueError("face topology requires an (N,3) triangle index array")
    n_faces, n_vertices = len(faces), len(vertices)
    max_faces = int(getattr(
        cfg, "topology_max_faces", DEFAULT_TOPOLOGY_MAX_FACES))
    max_compact_bytes = int(getattr(
        cfg, "topology_max_compact_payload_bytes",
        DEFAULT_TOPOLOGY_MAX_COMPACT_PAYLOAD_BYTES))
    max_peak_bytes = int(getattr(
        cfg, "topology_max_peak_bytes",
        DEFAULT_TOPOLOGY_MAX_PEAK_BYTES))
    compact_upper = _topology_retained_bytes(n_faces, 3 * n_faces)
    planned_build_peak = _planned_peak_build_bytes(n_faces, n_vertices)
    planned_validation_upper = _planned_peak_validation_bytes(
        n_faces, n_vertices, 3 * n_faces)
    stats = dict(
        n_faces=int(n_faces), n_vertices=int(n_vertices),
        compact_retained_bytes_upper_bound=int(compact_upper),
        max_compact_payload_bytes=int(max_compact_bytes),
        planned_peak_build_bytes=int(planned_build_peak),
        planned_peak_validation_bytes_upper_bound=int(
            planned_validation_upper),
        max_faces=int(max_faces), max_peak_bytes=int(max_peak_bytes),
        preflight_passed=False)
    refusals = []
    if n_faces > max_faces:
        refusals.append(
            f"topology_max_faces: {n_faces} > {max_faces}")
    if compact_upper > max_compact_bytes:
        refusals.append(
            "topology_max_compact_payload_bytes: "
            f"{compact_upper} > {max_compact_bytes}")
    if planned_build_peak > max_peak_bytes:
        refusals.append(
            f"topology_max_peak_bytes(build): "
            f"{planned_build_peak} > {max_peak_bytes}")
    if planned_validation_upper > max_peak_bytes:
        refusals.append(
            f"topology_max_peak_bytes(validation): "
            f"{planned_validation_upper} > {max_peak_bytes}")
    stats["refusal_reasons"] = refusals
    if stats_out is not None:
        stats_out.update(stats)
    if refusals:
        raise ValueError(
            "face-topology global preflight refused before edge allocation: "
            + "; ".join(refusals))
    if n_faces and (np.any(faces < 0) or np.any(faces >= n_vertices)):
        raise ValueError("face topology source face references an invalid vertex")

    vertex_bits = np.ascontiguousarray(vertices).view(np.uint64).reshape((-1, 3))
    _unique_bits, canonical_vertex = np.unique(
        vertex_bits, axis=0, return_inverse=True)
    cf = canonical_vertex[faces]
    del vertex_bits, _unique_bits, canonical_vertex

    cut_flags = np.zeros(n_faces, dtype=np.uint8)
    duplicate_face = np.zeros(n_faces, dtype=bool)
    if n_faces:
        tri_key = np.sort(cf, axis=1)
        tri_order = np.lexsort((
            tri_key[:, 2], tri_key[:, 1], tri_key[:, 0]))
        sorted_tri = tri_key[tri_order]
        duplicate_sorted = np.zeros(n_faces, dtype=bool)
        same = np.all(sorted_tri[1:] == sorted_tri[:-1], axis=1)
        duplicate_sorted[:-1] |= same
        duplicate_sorted[1:] |= same
        duplicate_face[tri_order[duplicate_sorted]] = True
        cut_flags[duplicate_face] |= TOPOLOGY_CUT_DUPLICATE
        del tri_key, tri_order, sorted_tri, duplicate_sorted, same

    edge_a = cf[:, [0, 1, 2]].reshape(-1)
    edge_b = cf[:, [1, 2, 0]].reshape(-1)
    edge_lo = np.minimum(edge_a, edge_b)
    edge_hi = np.maximum(edge_a, edge_b)
    edge_face = np.repeat(np.arange(n_faces, dtype=np.int64), 3)
    edge_forward = edge_a < edge_b
    edge_degenerate = edge_a == edge_b
    edge_order = np.lexsort((edge_face, edge_hi, edge_lo))
    lo = edge_lo[edge_order]
    hi = edge_hi[edge_order]
    ef = edge_face[edge_order]
    forward = edge_forward[edge_order]
    degenerate = edge_degenerate[edge_order]
    if len(lo):
        group_start = np.r_[0, np.nonzero(
            (lo[1:] != lo[:-1]) | (hi[1:] != hi[:-1]))[0] + 1]
        group_stop = np.r_[group_start[1:], len(lo)]
        group_count = group_stop - group_start
        group_id = np.repeat(
            np.arange(len(group_start), dtype=np.int64), group_count)
        first_face = ef[group_start]
        second_index = np.minimum(group_start + 1, group_stop - 1)
        second_face = ef[second_index]
        distinct_two = (group_count == 2) & (first_face != second_face)
        opposite = forward[group_start] != forward[second_index]
        group_degenerate = np.logical_or.reduceat(degenerate, group_start)
        group_duplicate = np.logical_or.reduceat(
            duplicate_face[ef], group_start)
        valid_group = (
            distinct_two & opposite & ~group_degenerate & ~group_duplicate)

        occurrence_flags = np.zeros(len(ef), dtype=np.uint8)
        counts_per_occurrence = group_count[group_id]
        occurrence_flags[counts_per_occurrence == 1] |= TOPOLOGY_CUT_BOUNDARY
        occurrence_flags[
            (counts_per_occurrence != 1)
            & ~valid_group[group_id]
            & ~group_degenerate[group_id]
            & ~group_duplicate[group_id]
            & ~((group_count[group_id] == 2)
                & distinct_two[group_id]
                & ~opposite[group_id])] |= TOPOLOGY_CUT_NONMANIFOLD
        occurrence_flags[
            (group_count[group_id] == 2)
            & distinct_two[group_id]
            & ~opposite[group_id]] |= TOPOLOGY_CUT_SAME_DIRECTION
        occurrence_flags[
            group_degenerate[group_id]] |= TOPOLOGY_CUT_DEGENERATE
        occurrence_flags[
            group_duplicate[group_id]] |= TOPOLOGY_CUT_DUPLICATE
        np.bitwise_or.at(cut_flags, ef, occurrence_flags)
        valid_starts = group_start[valid_group]
    else:
        valid_starts = np.empty(0, dtype=np.int64)
    del (
        edge_a, edge_b, edge_lo, edge_hi, edge_face, edge_degenerate,
        lo, hi, forward, degenerate)
    if n_faces:
        del (
            group_start, group_stop, group_count, group_id, first_face,
            second_index, second_face, distinct_two, opposite,
            group_degenerate, group_duplicate, valid_group,
            occurrence_flags, counts_per_occurrence)
    del duplicate_face
    if len(valid_starts):
        pair_a = ef[valid_starts]
        pair_b = ef[valid_starts + 1]
        row = np.r_[pair_a, pair_b]
        col = np.r_[pair_b, pair_a]
        csr_order = np.lexsort((col, row))
        row, col = row[csr_order], col[csr_order]
        counts = np.bincount(row, minlength=n_faces)
        indptr = np.r_[0, np.cumsum(counts, dtype=np.int64)]
        indices = np.ascontiguousarray(col, dtype=np.int64)
    else:
        indptr = np.zeros(n_faces + 1, dtype=np.int64)
        indices = np.empty(0, dtype=np.int64)
    components = _canonical_component_labels(indptr, indices, n_faces)
    del ef
    if len(valid_starts):
        del pair_a, pair_b, row, col, csr_order, counts

    # Point-only cut: build the induced incident-face fan independently at
    # every exact vertex.  Global face components are insufficient here: two
    # faces which touch only at a vertex can still be connected by an edge
    # path elsewhere in the mesh.  Incidence nodes are keyed by
    # ``(vertex, face)``; every valid manifold edge connects its two face
    # incidences once at each endpoint.  One sparse connected-components pass
    # therefore computes all local fans without a per-vertex Python traversal.
    incidence_vertex = cf.reshape(-1)
    incidence_face = np.repeat(np.arange(n_faces, dtype=np.int64), 3)
    incidence_order = np.lexsort((incidence_face, incidence_vertex))
    iv = incidence_vertex[incidence_order]
    iface = incidence_face[incidence_order]
    unique_incidence = (
        np.r_[True, (iv[1:] != iv[:-1]) | (iface[1:] != iface[:-1])]
        if len(iv) else np.empty(0, dtype=bool))
    incidence_node_sorted = np.cumsum(
        unique_incidence, dtype=np.int64) - 1
    incidence_node = np.empty(len(incidence_order), dtype=np.int64)
    incidence_node[incidence_order] = incidence_node_sorted
    iv, iface = iv[unique_incidence], iface[unique_incidence]
    del (
        incidence_vertex, incidence_face, incidence_order,
        unique_incidence, incidence_node_sorted)
    if len(iv):
        if len(valid_starts):
            first_occurrence = edge_order[valid_starts]
            second_occurrence = edge_order[valid_starts + 1]
            first_next = (
                (first_occurrence // 3) * 3
                + (first_occurrence + 1) % 3)
            second_next = (
                (second_occurrence // 3) * 3
                + (second_occurrence + 1) % 3)
            first_start_node = incidence_node[first_occurrence]
            first_end_node = incidence_node[first_next]
            second_start_node = incidence_node[second_occurrence]
            second_end_node = incidence_node[second_next]
            first_is_forward = edge_forward[first_occurrence]
            second_is_forward = edge_forward[second_occurrence]
            first_lo_node = np.where(
                first_is_forward, first_start_node, first_end_node)
            first_hi_node = np.where(
                first_is_forward, first_end_node, first_start_node)
            second_lo_node = np.where(
                second_is_forward, second_start_node, second_end_node)
            second_hi_node = np.where(
                second_is_forward, second_end_node, second_start_node)
            fan_row = np.r_[
                first_lo_node, second_lo_node,
                first_hi_node, second_hi_node]
            fan_col = np.r_[
                second_lo_node, first_lo_node,
                second_hi_node, first_hi_node]
            fan_graph = csr_matrix(
                (np.ones(len(fan_row), dtype=np.uint8),
                 (fan_row, fan_col)),
                shape=(len(iv), len(iv)))
            del (
                first_occurrence, second_occurrence, first_next, second_next,
                first_start_node, first_end_node, second_start_node,
                second_end_node, first_is_forward, second_is_forward,
                first_lo_node, first_hi_node, second_lo_node, second_hi_node,
                fan_row, fan_col)
        else:
            fan_graph = csr_matrix((len(iv), len(iv)), dtype=np.uint8)
        _n_fans, fan_labels = connected_components(
            fan_graph, directed=False)
        fan_labels = np.asarray(fan_labels, dtype=np.int64)
        del fan_graph

        # Sort labels within each vertex group, then count distinct fan labels
        # by grouped reduction.  A count above one means every incident face
        # participates in an ambiguous point-only junction at that vertex.
        fan_order = np.lexsort((fan_labels, iv))
        fan_vertex = iv[fan_order]
        fan_face = iface[fan_order]
        fan_label = fan_labels[fan_order]
        del fan_order, fan_labels, iv, iface
        vertex_start = np.r_[
            0, np.nonzero(fan_vertex[1:] != fan_vertex[:-1])[0] + 1]
        component_start = np.r_[
            True,
            (fan_vertex[1:] != fan_vertex[:-1])
            | (fan_label[1:] != fan_label[:-1])]
        component_count = np.add.reduceat(component_start, vertex_start)
        vertex_stop = np.r_[vertex_start[1:], len(fan_vertex)]
        bad_vertex_group = component_count > 1
        bad_occurrence = np.repeat(
            bad_vertex_group, vertex_stop - vertex_start)
        np.bitwise_or.at(
            cut_flags, fan_face[bad_occurrence], TOPOLOGY_CUT_POINT_ONLY)
        del (
            fan_vertex, fan_face, fan_label, vertex_start, component_start,
            component_count, vertex_stop, bad_vertex_group, bad_occurrence)
    del cf, incidence_node, edge_order, edge_forward, valid_starts

    mesh_id = mesh_fingerprint(mesh)
    cut_counts = tuple(int(x) for x in np.bincount(cut_flags, minlength=64))
    compact_retained = _topology_retained_bytes(n_faces, len(indices))
    planned_validation = _planned_peak_validation_bytes(
        n_faces, n_vertices, len(indices))
    topology = FaceTopology(
        schema=FACE_TOPOLOGY_SCHEMA,
        version=FACE_TOPOLOGY_SCHEMA_VERSION,
        indptr=_immutable_array(indptr, np.int64),
        indices=_immutable_array(indices, np.int64),
        component_ids=_immutable_array(components, np.int64),
        cut_flags=_immutable_array(cut_flags, np.uint8),
        n_faces=int(n_faces), n_vertices=int(n_vertices),
        n_directed_edges=int(len(indices)),
        compact_retained_bytes=int(compact_retained),
        planned_peak_build_bytes=int(planned_build_peak),
        planned_peak_validation_bytes=int(planned_validation),
        cut_counts=cut_counts, mesh_fingerprint=mesh_id,
        content_digest="",
    )
    object.__setattr__(topology, "content_digest", _topology_digest(topology))
    stats.update(
        preflight_passed=True, refusal_reasons=[],
        n_directed_edges=int(len(indices)),
        compact_retained_bytes=int(topology.compact_retained_bytes),
        planned_peak_validation_bytes=int(planned_validation),
        n_components=int(
            components.max(initial=-1) + 1),
        n_cut_faces=int(np.count_nonzero(cut_flags)))
    if stats_out is not None:
        stats_out.update(stats)
    return topology


def build_face_topology(mesh, cfg=None, stats_out=None):
    """Build, release all builder scratch, then deep-validate the sidecar."""
    rss_entry = _peak_rss_high_water_bytes()
    prepared = _build_face_topology_inner(mesh, cfg=cfg, stats_out=stats_out)
    rss_after_build = _peak_rss_high_water_bytes()
    validated = validate_face_topology(prepared, mesh=mesh)
    rss_after_validation = _peak_rss_high_water_bytes()
    if stats_out is not None:
        stats_out.update(
            measured_build_rss_high_water_delta_bytes=max(
                0, rss_after_build - rss_entry),
            measured_total_rss_high_water_delta_bytes=max(
                0, rss_after_validation - rss_entry))
    return validated
