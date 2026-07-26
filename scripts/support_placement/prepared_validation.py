"""Run-local authority for an already deep-validated prepared state.

The token in this module is deliberately private, identity-bound, and
unserializable.  It is only a capability to reuse a validation result inside
one in-memory engine run; prepared checkpoints remain self-validating at every
external staged boundary.
"""
from dataclasses import dataclass


_POOL_ARRAYS = ("positions", "normals", "face_ids", "island_ids")
_POOL_METADATA = (
    "schema", "version", "n_virtual_cells_planned",
    "n_candidate_nominations_upper_bound", "n_source_faces",
    "max_refine_depth",
)
_TOPOLOGY_ARRAYS = ("indptr", "indices", "component_ids", "cut_flags")
_TOPOLOGY_METADATA = (
    "schema", "version", "n_faces", "n_vertices", "n_directed_edges",
    "compact_retained_bytes", "planned_peak_build_bytes",
    "planned_peak_validation_bytes", "cut_counts", "mesh_fingerprint",
    "content_digest",
)
_COVERAGE_ARRAYS = (
    "positions", "area_weights_mm2", "face_ids", "island_ids",
    "refine_depths", "island_offsets", "source_face_offsets",
    "source_face_ids", "source_face_island_ids",
)
_COVERAGE_METADATA = (
    "schema", "version", "n_source_faces", "n_virtual_cells_planned",
    "max_refine_depth", "n_islands", "n_mesh_faces", "n_mesh_vertices",
    "max_working_cells", "compact_retained_bytes",
    "planned_peak_build_bytes", "planned_peak_validation_bytes",
    "mesh_fingerprint", "islands_fingerprint", "content_digest",
)


def _identity_tuple(value, names):
    return tuple(id(getattr(value, name, None)) for name in names)


def _metadata_tuple(value, names):
    return tuple(getattr(value, name, None) for name in names)


@dataclass(frozen=True, slots=True, repr=False)
class _PreparedValidationToken:
    _pre: object
    _mesh: object
    _cfg: object
    _islands: object
    _pool: object
    _topology: object
    _coverage: object
    _prepared_metadata: tuple
    _pool_arrays: tuple
    _pool_metadata: tuple
    _topology_arrays: tuple
    _topology_metadata: tuple
    _coverage_arrays: tuple
    _coverage_metadata: tuple

    def __reduce_ex__(self, _protocol):
        raise TypeError(
            "prepared validation tokens are run-local and unpicklable")

    def __copy__(self):
        raise TypeError("prepared validation tokens cannot be copied")

    def __deepcopy__(self, _memo):
        raise TypeError("prepared validation tokens cannot be copied")

    def _authority_is_unchanged(self):
        pre = self._pre
        return (
            isinstance(pre, dict)
            and pre.get("islands") is self._islands
            and pre.get("contact_candidate_pool") is self._pool
            and pre.get("face_topology") is self._topology
            and pre.get("coverage_samples") is self._coverage
            and (
                pre.get("schema"),
                pre.get("version"),
                pre.get("mesh_fingerprint"),
                pre.get("config_fingerprint"),
            ) == self._prepared_metadata
            and _identity_tuple(self._pool, _POOL_ARRAYS)
            == self._pool_arrays
            and _metadata_tuple(self._pool, _POOL_METADATA)
            == self._pool_metadata
            and _identity_tuple(self._topology, _TOPOLOGY_ARRAYS)
            == self._topology_arrays
            and _metadata_tuple(self._topology, _TOPOLOGY_METADATA)
            == self._topology_metadata
            and _identity_tuple(self._coverage, _COVERAGE_ARRAYS)
            == self._coverage_arrays
            and _metadata_tuple(self._coverage, _COVERAGE_METADATA)
            == self._coverage_metadata
        )

    def authorizes_prepared(self, pre, mesh, cfg):
        return (
            pre is self._pre
            and mesh is self._mesh
            and cfg is self._cfg
            and self._authority_is_unchanged()
        )

    def authorizes_domains(
            self, mesh, cfg, islands, candidate_pool=None,
            face_topology=None, coverage_samples=None):
        return (
            mesh is self._mesh
            and cfg is self._cfg
            and islands is self._islands
            and (candidate_pool is None or candidate_pool is self._pool)
            and (face_topology is None or face_topology is self._topology)
            and (coverage_samples is None or coverage_samples is self._coverage)
            and self._authority_is_unchanged()
        )


def _issue_prepared_validation_token(pre, mesh, cfg):
    """Issue after, never instead of, full ``validate_prepared_state``."""
    pool = pre["contact_candidate_pool"]
    face_topology = pre["face_topology"]
    coverage_samples = pre["coverage_samples"]
    return _PreparedValidationToken(
        _pre=pre,
        _mesh=mesh,
        _cfg=cfg,
        _islands=pre["islands"],
        _pool=pool,
        _topology=face_topology,
        _coverage=coverage_samples,
        _prepared_metadata=(
            pre["schema"], pre["version"],
            pre["mesh_fingerprint"], pre["config_fingerprint"]),
        _pool_arrays=_identity_tuple(pool, _POOL_ARRAYS),
        _pool_metadata=_metadata_tuple(pool, _POOL_METADATA),
        _topology_arrays=_identity_tuple(face_topology, _TOPOLOGY_ARRAYS),
        _topology_metadata=_metadata_tuple(
            face_topology, _TOPOLOGY_METADATA),
        _coverage_arrays=_identity_tuple(
            coverage_samples, _COVERAGE_ARRAYS),
        _coverage_metadata=_metadata_tuple(
            coverage_samples, _COVERAGE_METADATA),
    )


def _authorizes_prepared(token, pre, mesh, cfg):
    return (
        isinstance(token, _PreparedValidationToken)
        and token.authorizes_prepared(pre, mesh, cfg)
    )


def _authorizes_domains(
        token, mesh, cfg, islands, candidate_pool=None,
        face_topology=None, coverage_samples=None):
    return (
        isinstance(token, _PreparedValidationToken)
        and token.authorizes_domains(
            mesh, cfg, islands, candidate_pool=candidate_pool,
            face_topology=face_topology,
            coverage_samples=coverage_samples)
    )
