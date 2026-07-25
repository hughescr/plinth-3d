"""Anti-drift FROZEN parity baseline (SPEC-RUST-KERNEL.md sec 6 item 6,
Amendment A20; A2 table row A20 at :1016).

The differential suite (test_kernel_parity.py) builds BOTH the oracle and the
kernel LIVE on every run and asserts they agree. That is necessary but NOT
sufficient: if an accidental edit to the frozen oracle
(scripts/support_placement/geometry_exact.py) shifts its answers, and the kernel
happens to drift the same way (or the oracle edit is itself a bug the kernel is
then "pinned" to), every differential test still passes green — the contract is
silently re-baselined with no tripwire. The oracle is FROZEN reference code now.

This module closes that hole. It recomputes ORACLE-side answers over a fixed,
deterministic corpus spanning every oracle method across every parity fixture,
canonically serialises them (dtype+shape+tobytes for arrays; sorted keys for
dicts; bit-exact struct-packed floats), SHA-256 hashes each case, and compares
against scripts/tests/fixtures/kernel_parity.json — which also carries an
ENVIRONMENT STAMP (python/numpy/scipy versions; the raw-float hashes are
environment-sensitive).

Failure modes (all are suite FAILURES, never skips):
  * baseline file missing                 -> fail: freeze it explicitly
  * env stamp mismatch                    -> fail: re-freeze under the new env
  * case set differs (added/removed case) -> fail: structure drift
  * any per-case hash mismatch            -> fail: naming the drifted cases

The baseline is NEVER auto-created. First-freeze / intentional re-freeze happens
only via `pytest --freeze-kernel-baseline` (conftest.py), in its own commit with
a written rationale.
"""
import hashlib
import json
import os
import struct

import numpy as np
import trimesh
import pytest

from support_placement import geometry_exact as GE
from support_placement import mesh_io

pytestmark = pytest.mark.kernel

BASELINE_PATH = os.path.join(os.path.dirname(__file__), "fixtures",
                             "kernel_parity.json")


# --------------------------------------------------------------------------
# Fixtures — self-contained mirror of the differential suite's FIXTURES. Kept
# local ON PURPOSE: a frozen contract must not depend on another test module's
# (mutable) definitions.
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


# --------------------------------------------------------------------------
# Canonical serialisation (Amendment A19: byte identity, not np.array_equal
# which treats +0.0 == -0.0; ragged/dict outputs canonicalised first).
# --------------------------------------------------------------------------
def _canon(obj):
    if isinstance(obj, np.ndarray):
        a = np.ascontiguousarray(obj)
        return (b"A" + str(a.dtype).encode() + b";"
                + repr(a.shape).encode() + b";" + a.tobytes())
    if isinstance(obj, (list, tuple)):
        return b"L[" + b",".join(_canon(x) for x in obj) + b"]"
    if isinstance(obj, dict):
        return (b"D{" + b",".join(k.encode() + b":" + _canon(v)
                                  for k, v in sorted(obj.items())) + b"}")
    if isinstance(obj, (bool, np.bool_)):
        return b"b1" if bool(obj) else b"b0"
    if isinstance(obj, (int, np.integer)):
        return b"i" + str(int(obj)).encode()
    if isinstance(obj, (float, np.floating)):
        return b"f" + struct.pack("<d", float(obj))  # bit-exact, NaN/inf included
    if isinstance(obj, str):
        return b"s" + obj.encode()
    if obj is None:
        return b"N"
    raise TypeError(f"uncanonicalisable oracle answer part: {type(obj)!r}")


def _hash(obj):
    return hashlib.sha256(_canon(obj)).hexdigest()


# --------------------------------------------------------------------------
# Oracle-answer corpus. Deterministic (seeded per fixture name), covering every
# oracle method the parity contract relies on. Each (fixture, method) is its own
# hashed case so a drift is localised to the method that moved.
# --------------------------------------------------------------------------
def _seed(name):
    return int(hashlib.sha256(name.encode()).hexdigest()[:8], 16)


def _fixture_cases(name, mesh):
    o = GE.ExactSurface(mesh)
    lo, hi = mesh.bounds
    span = hi - lo
    ctr = 0.5 * (lo + hi)
    rng = np.random.default_rng(_seed(name))

    cases = {}

    # ---- query / project / inside over a seeded point cloud straddling bounds
    pts = lo - 0.25 * span + rng.uniform(0, 1.5, size=(256, 3)) * span
    d, ti = o.query(pts)
    cases["query"] = _hash([d, ti])
    dd, cp, idx, fn = o.project(pts)
    cases["project"] = _hash([dd, cp, idx, fn])
    cases["inside"] = _hash(o.inside(pts))

    # ---- cast_rays: bounds-corner origins toward the opposite corner + a few
    #      centre-crossing rays (ragged output, canonically serialised)
    origins = np.array([
        lo - 0.3 * span, hi + 0.3 * span, ctr + np.array([0, 0, 1.0]) * span,
        ctr - np.array([1.0, 0, 0]) * span,
    ])
    targets = np.array([hi + 0.3 * span, lo - 0.3 * span, ctr, ctr])
    dirs = targets - origins
    res = o.cast_rays(origins, dirs, 1.0)
    cases["cast_rays"] = _hash([[t, f] for (t, f) in res])

    # ---- segment_clearance over surface-anchored + open-space segments
    surf = cp[:16]                      # first 16 projected surface points
    p0 = surf
    p1 = surf + rng.uniform(-1, 1, size=surf.shape)
    r = np.full(len(p0), 0.5)
    clr, argp = o.segment_clearance(p0, p1, r, r, 0.5)
    cases["segment_clearance"] = _hash([clr, argp])

    # ---- penetration_intervals (axis + all) over centre-crossing segments
    pi_axis = []
    pi_all = []
    for j in range(12):
        a = ctr + rng.uniform(-1, 1, size=3) * span * 0.6
        b = a + rng.uniform(-1, 1, size=3) * span * 0.6
        if np.linalg.norm(b - a) < 1e-6:
            b = a + span * 0.3
        pi_axis.append(o.penetration_intervals(a, b, 0.3, 0.3, rays="axis"))
        pi_all.append(o.penetration_intervals(a, b, 0.3, 0.3, rays="all"))
    cases["penetration_intervals_axis"] = _hash(pi_axis)
    cases["penetration_intervals_all"] = _hash(pi_all)

    # ---- sphere_breach over surface-anchored centres +/- jitter
    centers = cp[:32] + rng.uniform(-0.5, 0.5, size=cp[:32].shape)
    radii = rng.uniform(0.3, 1.5, size=len(centers))
    br, th, pk = o.sphere_breach(centers, radii)
    cases["sphere_breach"] = _hash([br, th, pk])

    # ---- exterior-face mask (per-face boolean vector)
    cases["exterior_mask"] = _hash(o.exterior_face_mask())

    return {f"{name}.{k}": v for k, v in cases.items()}


def _build_corpus():
    corpus = {}
    for name, fn in FIXTURES.items():
        corpus.update(_fixture_cases(name, fn()))
    return corpus


def _env_stamp():
    import numpy as _np
    import scipy as _sp
    import platform as _pf
    return {
        "python": _pf.python_version(),
        "numpy": _np.__version__,
        "scipy": _sp.__version__,
    }


# ==========================================================================
# THE ANTI-DRIFT TEST
# ==========================================================================
def test_frozen_oracle_parity_baseline(request):
    corpus = _build_corpus()
    stamp = _env_stamp()
    freeze = request.config.getoption("--freeze-kernel-baseline")

    if freeze:
        payload = {
            "_comment": ("FROZEN oracle-side parity hashes (SPEC-RUST-KERNEL.md "
                         "sec 6 item 6, Amendment A20). Regenerate ONLY via "
                         "`pytest --freeze-kernel-baseline`, in its own commit "
                         "with a written rationale. Do not hand-edit."),
            "env_stamp": stamp,
            "hashes": dict(sorted(corpus.items())),
        }
        with open(BASELINE_PATH, "w") as fh:
            json.dump(payload, fh, indent=1, sort_keys=False)
            fh.write("\n")
        return  # freeze pass: file written, nothing to compare against

    if not os.path.exists(BASELINE_PATH):
        pytest.fail(
            "frozen parity baseline missing: "
            f"{BASELINE_PATH}\nFirst-freeze it explicitly (own commit, written "
            "rationale):\n  pytest scripts/tests/test_kernel_parity_baseline.py "
            "--freeze-kernel-baseline")

    with open(BASELINE_PATH) as fh:
        frozen = json.load(fh)

    frozen_stamp = frozen.get("env_stamp", {})
    if frozen_stamp != stamp:
        pytest.fail(
            "frozen parity baseline ENV STAMP mismatch — raw-float hashes are "
            f"environment-sensitive.\n  frozen : {frozen_stamp}\n  current: "
            f"{stamp}\nRe-freeze under the new environment (own commit):\n  "
            "pytest scripts/tests/test_kernel_parity_baseline.py "
            "--freeze-kernel-baseline")

    frozen_hashes = frozen.get("hashes", {})

    missing = sorted(set(frozen_hashes) - set(corpus))   # in baseline, not computed
    added = sorted(set(corpus) - set(frozen_hashes))     # computed, not in baseline
    if missing or added:
        pytest.fail(
            "frozen parity baseline case-set DRIFT (the corpus definition and "
            "the frozen file disagree on which cases exist).\n"
            f"  in baseline but not recomputed: {missing}\n"
            f"  recomputed but not in baseline: {added}\n"
            "If this change to the corpus is intentional, re-freeze in its own "
            "commit:  pytest ... --freeze-kernel-baseline")

    drifted = sorted(k for k in corpus if corpus[k] != frozen_hashes[k])
    assert not drifted, (
        "FROZEN ORACLE DRIFT — the oracle's answers changed for these cases "
        f"since the baseline was frozen:\n  {drifted}\n"
        "This means scripts/support_placement/geometry_exact.py (the FROZEN "
        "reference oracle) now produces different geometry than the pinned "
        "parity contract. Investigate the oracle change; do NOT silently "
        "re-baseline. A legitimate, reviewed oracle change is re-frozen in its "
        "own commit:  pytest ... --freeze-kernel-baseline")
