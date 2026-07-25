"""The emit boundary: an ``engine.run`` result + the STL it was computed for
-> one ``.chitubox`` project, written from an EMPTY template.

WHY THIS MODULE EXISTS (the failure mode it removes)
----------------------------------------------------
The previous emitter cloned a "base" ``.chitubox`` and patched supports into
it (``chitubox_inject.write_replace``). That path copies the whole byte range
through the end of the template's mesh region -- including the TEMPLATE's own
model. Emitting against a model-bearing template therefore produced a file
that parsed, verified, and looked right, while carrying **correct supports
stapled onto the wrong model**. Nothing about the output announced it.

This module cannot do that. It builds through ``chitubox_io`` (Rust), whose
Python surface has no template parameter at all: every project starts from the
crate's embedded empty container, and the model mesh is written explicitly
from the STL path this emit was handed. The failure mode is gone
structurally, not guarded against at runtime.

THE FRAME (why the model is passed as a PATH, not as triangles)
---------------------------------------------------------------
``engine.run`` works in the model-centered frame: the caller centers the mesh
on its bbox center, and ``engine.prepare`` sets
``plate_z = mesh.bounds[0][2] - cfg.lift_mm`` in that frame. ``chitubox_io``
centers an added model on its own bbox center too, so passing the STL *path*
makes "the model in the file is the model the supports were computed for" a
single shared variable rather than an assertion someone has to maintain. The
only divergence is f32-vs-f64 rounding of that center (of order 1e-6 mm at
64 mm scale); ``emit_project`` measures it and reports it as
``center_delta_mm`` rather than pretending it is zero.

Support-record coordinates are already in that same centered frame, so they
cross unchanged. The plate lift travels as ``placement.z_lift`` and satisfies
``zmin_centered - plate_z == cfg.lift_mm`` -- asserted here, not re-derived
from a corpus specimen the way the old manual emitter did.

WHAT THIS MODULE DOES NOT EMIT
------------------------------
No PADs and no raft BRIDGEs. Plate-rooted members get the ``T_FLARE`` conical
foot that :func:`chitubox_emit.member_chain_specs` synthesizes; the v3 emitter
additionally cloned pad records and tied them into an MST bridge raft, and
that is a validated-by-cloning-only construction this from-scratch path
deliberately does not carry. Whether bare flares slice acceptably is a
print-validation question, open and owner-facing -- not something the emit
path can settle.
"""
import os
from dataclasses import dataclass, asdict

import numpy as np

from . import chitubox_emit
from . import chitubox_inject as ci
from . import mesh_io

#: The ``chitubox_io._native.API_VERSION`` this module is written against.
#: ``scripts/tests/conftest.py`` fails the ``chitubox``-marked tests when the
#: built extension disagrees, so a stale binding cannot certify itself green.
EXPECTED_CHITUBOX_API_VERSION = 1

#: Tolerance on the ``zmin_centered - plate_z == lift_mm`` frame invariant.
#: The two sides are computed in f64 from the same bounds, so anything above
#: float noise means the result and the STL disagree about the frame.
LIFT_TOL_MM = 1e-6


class EmitError(RuntimeError):
    """Emission could not produce a trustworthy file."""


class EmitRefused(EmitError):
    """Refused to emit on policy grounds (failed gates), before writing."""


@dataclass(frozen=True)
class EmitReport:
    """What one ``emit_project`` call measured about the file it wrote.

    Every field is a measurement of the OUTPUT (or of the input that produced
    it), not a restatement of the request. ``model_triangles`` and
    ``reparse_ok`` in particular come from re-opening the finished file.
    """
    out_path: str
    model_name: str
    model_stl: str
    #: triangle count read back out of the written file's model section
    model_triangles: int
    #: triangle count of the source STL, as trimesh loads it
    source_triangles: int
    #: max |bbox center(f32 in-file) - bbox center(f64 source)| over xyz, mm
    center_delta_mm: float
    n_records: int
    n_members: int
    n_balls: int
    n_flares: int
    support_mesh_bytes: int
    plate_z: float
    lift_mm: float
    placement_xy: tuple
    bytes_written: int
    reparse_ok: bool
    verify_ok: bool

    def to_dict(self):
        """JSON-safe dict (no numpy scalars)."""
        d = asdict(self)
        d["placement_xy"] = [float(v) for v in self.placement_xy]
        for k in ("center_delta_mm", "plate_z", "lift_mm"):
            d[k] = float(d[k])
        for k in ("model_triangles", "source_triangles", "n_records", "n_members",
                  "n_balls", "n_flares", "support_mesh_bytes", "bytes_written"):
            d[k] = int(d[k])
        for k in ("reparse_ok", "verify_ok"):
            d[k] = bool(d[k])
        return d


def _native():
    """Import the extension lazily so merely importing this module (e.g. for
    ``EXPECTED_CHITUBOX_API_VERSION`` in conftest's own gate) does not require
    a built binding."""
    try:
        import chitubox_io
    except ImportError as e:  # pragma: no cover - environment failure
        raise EmitError(
            "chitubox_io._native is not built. Build it with:\n"
            "  scripts/build_kernel.sh chitubox_io") from e
    if chitubox_io.API_VERSION != EXPECTED_CHITUBOX_API_VERSION:
        raise EmitError(
            f"chitubox_io API version {chitubox_io.API_VERSION} != expected "
            f"{EXPECTED_CHITUBOX_API_VERSION} — rebuild: "
            "scripts/build_kernel.sh chitubox_io")
    return chitubox_io


def support_arrays(specs):
    """Pack ``SegmentSpec``s into the columnar batch ``chitubox_io`` takes.

    ONE crossing for the whole forest: parallel arrays plus a single
    concatenated ``mesh_flat`` buffer with a per-record float count. At treant
    scale that buffer is ~16 MB; handing the boundary a record at a time would
    be a defect, not a style choice.
    """
    n = len(specs)
    if n == 0:
        raise EmitError("no support records to emit (empty forest)")
    meshes = [np.ascontiguousarray(s.mesh, dtype=np.float32).reshape(-1) for s in specs]
    return {
        "seg_type": np.fromiter((s.type_id for s in specs), dtype=np.uint32, count=n),
        "start": np.ascontiguousarray([s.start for s in specs], dtype=np.float32),
        "end": np.ascontiguousarray([s.end for s in specs], dtype=np.float32),
        "radii": np.ascontiguousarray([(s.r0, s.r1) for s in specs], dtype=np.float32),
        "trailing": np.ascontiguousarray(
            np.frombuffer(b"".join(s.trailing for s in specs), dtype=np.uint8).reshape(n, 24)),
        "mesh_flat": np.concatenate(meshes) if n else np.zeros(0, np.float32),
        "mesh_len": np.fromiter((m.size for m in meshes), dtype=np.uint32, count=n),
    }


def emit_project(model_stl, result, out_path, cfg, *, name=None,
                 placement_xy=(0.0, 0.0), lift_mm=None,
                 allow_failed_gates=False):
    """Write ``out_path`` from the empty template: the model from ``model_stl``
    plus the support forest in ``result``.

    Parameters
    ----------
    model_stl : str
        Path to **the exact STL ``engine.run`` consumed** (before the caller's
        own centering -- centering is what both sides do independently and
        identically). This is the whole model-fidelity contract: the file gets
        the mesh at this path, so there is no way for it to get a different
        one.
    result : dict
        The dict ``engine.run`` returned. Reads ``members``, ``balls``,
        ``plate_z`` and ``verify``.
    out_path : str
        Destination ``.chitubox``. Written atomically by the Rust side.
    cfg : config.SupportConfig
        The same config the run used. Supplies ``lift_mm`` and the emission
        geometry factors (flare, tip ball, contact depth).
    name : str, optional
        Model name inside the project. Defaults to the STL's basename without
        its extension (63-byte cap, enforced by the container).
    placement_xy : (float, float)
        Plate translation. ``(0, 0)`` centers the model on the plate.
    lift_mm : float, optional
        Defaults to ``cfg.lift_mm``. Must satisfy
        ``zmin_centered - plate_z == lift_mm``; a mismatch means ``result``
        and ``cfg`` are not from the same run and raises.
    allow_failed_gates : bool
        By default a ``result`` whose ``verify.ok`` is False raises
        :class:`EmitRefused` **before anything is written** -- the same
        doctrine as ``cfg.perf_gate_emit_on_verify``, applied at the boundary
        where a bad layout would otherwise become a physical print.

    Returns
    -------
    EmitReport
    """
    cx = _native()

    if not os.path.isfile(model_stl):
        raise EmitError(f"model STL not found: {model_stl}")

    verify = result.get("verify") or {}
    verify_ok = bool(verify.get("ok", False))
    if not verify_ok and not allow_failed_gates:
        raise EmitRefused(
            "refusing to emit: engine verify.ok is False. Pass "
            "allow_failed_gates=True to emit a gate-failing layout anyway.")

    members = result["members"]
    balls = result["balls"]
    plate_z = float(result["plate_z"])
    lift = float(cfg.lift_mm if lift_mm is None else lift_mm)

    # Frame check against the STL itself, not against the caller's word for
    # it: reload the source mesh, center it the way both engine and container
    # do, and confirm the run's plate_z sits exactly lift_mm below its zmin.
    source = mesh_io.load_stl(model_stl)
    src_min = np.asarray(source.bounds[0], float)
    src_max = np.asarray(source.bounds[1], float)
    center = (src_min + src_max) / 2.0
    zmin_centered = float(src_min[2] - center[2])
    if abs((zmin_centered - plate_z) - lift) > LIFT_TOL_MM:
        raise EmitError(
            f"frame mismatch: zmin_centered - plate_z = "
            f"{zmin_centered - plate_z:.9f} mm but lift_mm = {lift:.9f} mm. "
            f"The result and the STL are not from the same run "
            f"(or cfg.lift_mm changed since).")

    # ONE geometry pass over the whole forest, then ONE packed crossing.
    specs = chitubox_emit.member_chain_specs(members, balls, cfg, plate_z=plate_z)
    arrays = support_arrays(specs)
    n_flares = int(sum(1 for s in specs if s.type_id == ci.T_FLARE))

    model = {
        "name": name if name is not None
        else os.path.splitext(os.path.basename(model_stl))[0][:63],
        "stl_path": os.fspath(model_stl),
        "placement": (float(placement_xy[0]), float(placement_xy[1]), lift),
        "supports": arrays,
    }
    try:
        wrote = cx.write_project(os.fspath(out_path), [model])
    except cx.ChituboxError as e:
        raise EmitError(f"chitubox_io refused the write [{getattr(e, 'kind', '?')}]: {e}") from e

    if not wrote["reparse_ok"]:
        raise EmitError(
            f"wrote {out_path} but re-parsing it disagreed with what was "
            f"written: {wrote!r}")

    # Model fidelity, measured off the finished file rather than assumed.
    back = cx.read_project(os.fspath(out_path))
    m = back["models"][0]
    # The in-file mesh is bbox-centered, so compare it against the SOURCE
    # bbox expressed in the same centered frame. Any residual is the
    # f32-vs-f64 rounding of the shared bbox center, nothing else.
    center_delta = float(max(
        np.abs(np.asarray(m["bbox_min"], float) - (src_min - center)).max(),
        np.abs(np.asarray(m["bbox_max"], float) - (src_max - center)).max()))

    return EmitReport(
        out_path=os.fspath(out_path),
        model_name=m["name"],
        model_stl=os.fspath(model_stl),
        model_triangles=int(m["triangle_count"]),
        source_triangles=int(len(source.faces)),
        center_delta_mm=center_delta,
        n_records=int(m["support_record_count"]),
        n_members=int(len(members)),
        n_balls=int(len(balls)),
        n_flares=n_flares,
        support_mesh_bytes=int(arrays["mesh_flat"].nbytes),
        plate_z=plate_z,
        lift_mm=lift,
        placement_xy=(float(placement_xy[0]), float(placement_xy[1])),
        bytes_written=int(wrote["bytes_written"]),
        reparse_ok=True,
        verify_ok=verify_ok,
    )
